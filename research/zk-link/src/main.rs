//! Research prototype: the receipt-account link (docs/LINK.md) without disclosing a viewing key.
//!
//! The shipped link shows the rail one receipt account's viewing key. Here the holder instead proves,
//! for each note the deposit `D` spends, an ordinary Ironwood spend of that note against a *synthetic
//! anchor*: the root of a tree whose leaves are exactly the funding payment `T1`'s output commitments.
//! The existing circuit (no changes) then shows that the note is one of `T1`'s outputs, that its
//! nullifier is the one `D` published, and that the prover controls it. Anyone can rebuild the root
//! from `T1`. The proofs can't be broadcast: consensus only accepts anchors that are real chain roots.
//!
//! The circuit skips the root check for zero-value notes, so the verifier requires a positive value
//! balance on every proof (checked by the binding signature) and a proof for **every** nullifier in
//! `D`. What a proof reveals: each input note's value and that it came from `T1`. Not which output of
//! `T1`, the address, the memo or any key.
//!
//! Unreviewed. See research/zk-link/README.md.

use std::{collections::HashSet, fs, io::Read, path::PathBuf, time::Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use bip0039::{English, Mnemonic};
use clap::{Parser, Subcommand};
use incrementalmerkletree::{Hashable, Level, frontier::CommitmentTree};
use orchard::{
    Anchor, Bundle, ValuePool,
    builder::{Builder, BundleType},
    bundle::{Authorized, BundleVersion},
    circuit::{OrchardCircuitVersion, ProvingKey, VerifyingKey},
    keys::{FullViewingKey, Scope, SpendAuthorizingKey, SpendingKey},
    note::{ExtractedNoteCommitment, Note},
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
};
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use serde::{Deserialize, Serialize};
use vintage_core::network::VintageNetwork;
use vintage_verifier::{link, load_registry, lwd, report, resolve_before, roots};
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_note_encryption::try_note_decryption;
use zcash_primitives::transaction::{
    Transaction,
    components::orchard::{read_v6_bundle, write_v6_bundle},
};
use zcash_protocol::{consensus::BranchId, value::ZatBalance};

const DEPTH: usize = 32;
const PERSONAL: &[u8; 16] = b"Vintage_ZkLink01";

#[derive(Parser)]
#[command(name = "zk-link", about = "Research prototype: prove a deposit's inputs came from a Vintage payment, without a viewing key.")]
struct Cli {
    /// lightwalletd/Zaino gRPC server host:port (TLS). Defaults to zec.rocks for mainnet.
    #[arg(long, global = true)]
    server: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Holder, locally: write proofs that every input of DEPOSIT is an output of FUNDING.
    Prove {
        #[arg(long)]
        deposit: String,
        #[arg(long)]
        funding: String,
        /// zcash-devtool wallet directory of the receipt account (reads keys.toml only).
        #[arg(long)]
        wallet: PathBuf,
        /// age identity file that decrypts the wallet's keys.toml.
        #[arg(long)]
        identity: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Anyone: check a proof file against the chain, and the funding payment against an event.
    Verify {
        file: PathBuf,
        /// A block height, or an event id from the registry.
        #[arg(long)]
        before: String,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

/// The file the holder hands to the rail. No key in it.
#[derive(Serialize, Deserialize)]
struct ZkLink {
    version: u32,
    network: String,
    deposit: String,
    funding: String,
    /// One serialized one-action Ironwood bundle (v6 transaction encoding) per input of the deposit.
    proofs: Vec<String>,
}

/// What every signature in the proofs signs: binds them to this deposit and this funding payment.
fn challenge(deposit: &str, funding: &str) -> [u8; 32] {
    let h = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(PERSONAL)
        .to_state()
        .update(deposit.as_bytes())
        .update(b"|")
        .update(funding.as_bytes())
        .finalize();
    h.as_bytes().try_into().unwrap()
}

/// Leaves of the synthetic tree: the funding payment's Ironwood output commitments, in order.
fn leaves(tx: &Transaction) -> Result<Vec<MerkleHashOrchard>> {
    let b = tx.ironwood_bundle().ok_or_else(|| anyhow!("the funding payment has no Ironwood outputs"))?;
    Ok(b.actions().iter().map(|a| MerkleHashOrchard::from_cmx(a.cmx())).collect())
}

/// Root and authentication path for leaf `pos` in a depth-32 tree holding exactly `leaves`.
fn synthetic_path(leaves: &[MerkleHashOrchard], pos: usize) -> (MerkleHashOrchard, [MerkleHashOrchard; DEPTH]) {
    let mut nodes = leaves.to_vec();
    let mut idx = pos;
    let mut auth = Vec::with_capacity(DEPTH);
    for l in 0..DEPTH {
        let level = Level::from(l as u8);
        let empty = MerkleHashOrchard::empty_root(level);
        auth.push(nodes.get(idx ^ 1).copied().unwrap_or(empty));
        nodes = nodes.chunks(2).map(|c| MerkleHashOrchard::combine(level, &c[0], c.get(1).unwrap_or(&empty))).collect();
        idx /= 2;
    }
    (nodes[0], auth.try_into().unwrap())
}

/// The same root via incrementalmerkletree, as an independent cross-check.
fn root_by_library(leaves: &[MerkleHashOrchard]) -> Result<MerkleHashOrchard> {
    let mut tree = CommitmentTree::<MerkleHashOrchard, 32>::empty();
    for l in leaves {
        tree.append(*l).map_err(|_| anyhow!("tree full"))?;
    }
    Ok(tree.root())
}

/// The receipt account's spending key, decrypted in memory only. Never printed or written.
fn spending_key(network: VintageNetwork, wallet: &PathBuf, identity: &PathBuf) -> Result<SpendingKey> {
    let keys = fs::read_to_string(wallet.join("keys.toml")).context("reading keys.toml")?;
    let start = keys.find("mnemonic = \"\"\"").ok_or_else(|| anyhow!("keys.toml has no mnemonic"))? + 15;
    let end = start + keys[start..].find("\"\"\"").ok_or_else(|| anyhow!("unterminated mnemonic"))?;
    let armored = keys[start..end].trim_start_matches('\n');
    let ids = age::IdentityFile::from_file(identity.display().to_string())?.into_identities()?;
    let dec = age::Decryptor::new(age::armor::ArmoredReader::new(armored.as_bytes()))?;
    let mut phrase = String::new();
    dec.decrypt(ids.iter().map(|i| i.as_ref() as &dyn age::Identity))?.read_to_string(&mut phrase)?;
    let seed = Mnemonic::<English>::from_phrase(phrase.trim()).map_err(|e| anyhow!("mnemonic: {e}"))?.to_seed("");
    let usk = UnifiedSpendingKey::from_seed(&network.params(), &seed, zip32::AccountId::ZERO)
        .map_err(|e| anyhow!("deriving the account key: {e:?}"))?;
    Ok(usk.orchard().clone())
}

/// One proof: an Ironwood spend of `note` against `anchor`, proven and signed over `challenge`.
fn disclose(pk: &ProvingKey, sk: &SpendingKey, note: Note, anchor: Anchor, path: MerklePath, challenge: [u8; 32]) -> Result<Bundle<Authorized, ZatBalance>> {
    let mut rng = UnwrapErr(SysRng);
    let version = BundleVersion::ironwood_v3();
    let mut b = Builder::new(BundleType::Transactional { bundle_required: true, pad_to_minimum: Some(1) }, version, version.default_flags(), anchor)
        .map_err(|e| anyhow!("{e:?}"))?;
    b.add_spend(FullViewingKey::from(sk), note, path).map_err(|e| anyhow!("add_spend: {e:?}"))?;
    let (bundle, _) = b.build::<ZatBalance>(&mut rng).map_err(|e| anyhow!("{e:?}"))?.ok_or_else(|| anyhow!("empty bundle"))?;
    let proven = bundle.create_proof(pk, &mut rng).map_err(|e| anyhow!("{e:?}"))?;
    proven.prepare(&mut rng, challenge).sign(&mut rng, &SpendAuthorizingKey::from(sk)).finalize().map_err(|e| anyhow!("{e:?}"))
}

async fn prove(server: &str, deposit: &str, funding: &str, wallet: &PathBuf, identity: &PathBuf, out: &PathBuf) -> Result<()> {
    let network = VintageNetwork::Main;
    let client = lwd::connect(server).await?;
    let (t1, d) = (link::fetch(&client, network, funding).await?, link::fetch(&client, network, deposit).await?);
    let sk = spending_key(network, wallet, identity)?;
    let fvk = FullViewingKey::from(&sk);

    // The receipt account's notes in T1, found the way a wallet finds them.
    let ivk = fvk.to_ivk(Scope::External).prepare();
    let t1_bundle = t1.tx.ironwood_bundle().ok_or_else(|| anyhow!("the funding payment has no Ironwood outputs"))?;
    let mine: Vec<(usize, Note)> = t1_bundle
        .actions()
        .iter()
        .enumerate()
        .filter_map(|(i, a)| try_note_decryption(&IronwoodDomain::for_action(a), &ivk, a).map(|(n, _, _)| (i, n)))
        .collect();
    ensure!(!mine.is_empty(), "this key received nothing in the funding payment");
    let leaves = leaves(&t1.tx)?;
    let d_nfs = link::nullifiers(&d.tx);
    println!("funding:  {funding}: {} Ironwood outputs, {} to this account", leaves.len(), mine.len());
    println!("deposit:  {deposit}: {} Ironwood inputs", d_nfs.len());

    let t = Instant::now();
    let pk = ProvingKey::build(OrchardCircuitVersion::PostNu6_3);
    println!("keys:     proving key built in {:.1}s", t.elapsed().as_secs_f64());

    let c = challenge(deposit, funding);
    let mut proofs = vec![];
    for nf in &d_nfs {
        let (pos, note) = mine
            .iter()
            .find(|(_, n)| n.nullifier(&fvk).to_bytes() == *nf)
            .ok_or_else(|| anyhow!("deposit input {} is not one of this account's outputs in the funding payment", hex::encode(nf)))?;
        let (root, auth) = synthetic_path(&leaves, *pos);
        let anchor = Anchor::from(root);
        let path = MerklePath::from_parts(*pos as u32, auth);
        ensure!(path.root(ExtractedNoteCommitment::from(note.commitment())) == anchor, "path does not lead to the synthetic root");
        let t = Instant::now();
        let bundle = disclose(&pk, &sk, *note, anchor, path, c)?;
        let mut bytes = vec![];
        write_v6_bundle(Some(&bundle), &mut bytes)?;
        println!("proof:    input {}… proven and signed in {:.1}s ({} bytes)", &hex::encode(nf)[..8], t.elapsed().as_secs_f64(), bytes.len());
        proofs.push(hex::encode(bytes));
    }
    let file = ZkLink { version: 1, network: network.name().into(), deposit: deposit.into(), funding: funding.into(), proofs };
    fs::write(out, serde_json::to_string_pretty(&file)? + "\n")?;
    println!("wrote:    {} (no key inside)", out.display());
    Ok(())
}

async fn verify(server: &str, file: &PathBuf, before: &str, registry: &PathBuf) -> Result<bool> {
    let z: ZkLink = serde_json::from_str(&fs::read_to_string(file)?)?;
    ensure!(z.version == 1 && z.network == "main", "unsupported file (version {}, network {})", z.version, z.network);
    let network = VintageNetwork::Main;
    let before = resolve_before(|| load_registry(registry), network, before)?;
    let h = before.height;
    let client = lwd::connect(server).await?;
    let (t1, d) = (link::fetch(&client, network, &z.funding).await?, link::fetch(&client, network, &z.deposit).await?);
    ensure!(t1.mined.is_some() && d.mined.is_some(), "both transactions must be mined");

    // 1. The synthetic root, rebuilt from public data two ways.
    let leaves = leaves(&t1.tx)?;
    let (root, _) = synthetic_path(&leaves, 0);
    ensure!(root_by_library(&leaves)? == root, "synthetic root disagrees with incrementalmerkletree");
    let anchor = Anchor::from(root);
    println!("funding root:  {} ({} outputs of {}…)", hex::encode(anchor.to_bytes()), leaves.len(), &z.funding[..8]);

    // 2. Every proof.
    let t = Instant::now();
    let vk = VerifyingKey::build(OrchardCircuitVersion::PostNu6_3);
    println!("keys:          verifying key built in {:.1}s", t.elapsed().as_secs_f64());
    let c = challenge(&z.deposit, &z.funding);
    let d_nfs: HashSet<[u8; 32]> = link::nullifiers(&d.tx).into_iter().collect();
    let mut covered = HashSet::new();
    let mut total: i64 = 0;
    for (i, p) in z.proofs.iter().enumerate() {
        let bytes = hex::decode(p)?;
        let b = read_v6_bundle(&bytes[..], BranchId::Nu6_3, ValuePool::Ironwood)?.ok_or_else(|| anyhow!("proof {i}: empty bundle"))?;
        ensure!(b.bundle_version() == BundleVersion::ironwood_v3(), "proof {i}: not an Ironwood bundle");
        ensure!(b.actions().len() == 1, "proof {i}: expected exactly one action");
        ensure!(*b.anchor() == anchor, "proof {i}: anchor is not the funding payment's outputs root");
        let t = Instant::now();
        b.verify_proof(&vk).map_err(|e| anyhow!("proof {i}: rejected ({e:?})"))?;
        let a = b.actions().first();
        a.rk().verify(&c, a.authorization()).map_err(|_| anyhow!("proof {i}: spend signature over the challenge fails"))?;
        b.binding_validating_key()
            .verify(&c, b.authorization().binding_signature())
            .map_err(|_| anyhow!("proof {i}: binding signature fails (value balance not proven)"))?;
        let v = i64::from(*b.value_balance());
        ensure!(v > 0, "proof {i}: zero value proves nothing (the circuit skips the root check for zero-value notes)");
        let nf = a.nullifier().to_bytes();
        ensure!(d_nfs.contains(&nf), "proof {i}: its nullifier is not spent by the deposit");
        ensure!(covered.insert(nf), "proof {i}: duplicate nullifier");
        total += v;
        println!("input {}…:  proof valid in {:.2}s, value ≥ {v} zat, came from the funding payment", &hex::encode(nf)[..8], t.elapsed().as_secs_f64());
    }
    let missing = d_nfs.len() - covered.len();
    if missing > 0 {
        println!("\nLINK FAIL: {missing} of the deposit's {} inputs have no proof", d_nfs.len());
        return Ok(false);
    }
    println!("coverage:      all {} inputs of {}… proven, {} zat in total", d_nfs.len(), &z.deposit[..8], total);

    // 3. The funding payment's own age, with the ordinary verifier.
    let cache = roots::default_cache_path().map(|p| roots::Cache::open(&p, network)).transpose()?;
    let r = report::verify(&client, network, cache.as_ref(), &z.funding, before.clone(), 8).await?;
    println!("funding age:   {}: {}", r.verdict, r.summary);
    if r.verdict != "PASS" {
        println!("\nLINK FAIL: the funding payment doesn't predate block {h}");
        return Ok(false);
    }
    println!("\nZK LINK PASS: every coin {}… spends came from a payment whose coins existed before block {h}.", &z.deposit[..8]);
    println!("Disclosed: each input's value. Not disclosed: any key, which output, addresses, memos.");
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let server = cli.server.unwrap_or_else(|| VintageNetwork::Main.default_server().to_string());
    match cli.cmd {
        Cmd::Prove { deposit, funding, wallet, identity, out } => prove(&server, &deposit, &funding, &wallet, &identity, &out).await,
        Cmd::Verify { file, before, registry } => {
            if !verify(&server, &file, &before, &registry).await? {
                std::process::exit(1);
            }
            Ok(())
        }
    }
    .or_else(|e| bail!("{e:#}"))
}

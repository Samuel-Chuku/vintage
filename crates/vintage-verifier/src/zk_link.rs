//! The receipt-account link without a viewing key. **Experimental: the construction is unreviewed.**
//!
//! Statement: every Ironwood coin spent by deposit `D` was created by the funding payment `T`, and `T`
//! passes the age check against `H`.
//!
//! For each note `D` spends, the holder proves an ordinary Ironwood spend of it against a *synthetic
//! anchor*: the root of a depth-32 tree whose leaves are exactly `T`'s output commitments, in order.
//! The existing circuit (unchanged) then shows the note is one of `T`'s outputs, that its nullifier is
//! the one `D` published, and that the prover controls it. Anyone can rebuild the root from `T`. The
//! proofs can't be broadcast: consensus only accepts anchors that are real chain roots.
//!
//! The circuit skips the root check for zero-value notes, so every proof must carry a positive value
//! balance (checked by the binding signature), and every nullifier in `D` needs a proof. Both
//! signatures sign a hash of the deposit and funding txids, so a proof can't be reused for another
//! pair. What a proof reveals: each input note's value and that it came from `T`. Not which output of
//! `T`, the address, the memo or any key.
//!
//! See research/zk-link/README.md and docs/ROADMAP.md (one-hop provenance).

use std::{collections::HashSet, time::Instant};

use incrementalmerkletree::{Hashable, Level, frontier::CommitmentTree};
use orchard::{
    Anchor, ValuePool,
    bundle::BundleVersion,
    circuit::{OrchardCircuitVersion, VerifyingKey},
    tree::MerkleHashOrchard,
};
use serde::{Deserialize, Serialize};
use vintage_core::{VintageNetwork, tx};
use zcash_primitives::transaction::{Transaction, components::orchard::read_v6_bundle};
use zcash_protocol::consensus::BranchId;

use crate::{
    link::{self, DepositResult},
    lwd,
    report::{self, Before, VerifyError},
    roots,
};

/// What the holder discloses, stated verbatim wherever the ZK link is offered.
pub const DISCLOSED: &str = "Each input's value, and that each input came from the funding payment. Not disclosed: \
     any key, which output of the payment, addresses or memos.";

/// Stated with every result.
pub const STATUS: &str = "Experimental: the zero-knowledge link is an unreviewed construction. Don't rely on it \
     for anything that matters until it has been reviewed.";

/// Most proofs one file may carry (one per deposit input).
pub const MAX_PROOFS: usize = 16;

/// Depth of the note commitment tree, and of the synthetic tree.
pub const DEPTH: usize = 32;

const PERSONAL: &[u8; 16] = b"Vintage_ZkLink01";

/// The file `vintage link prove --zk` writes and `vintage link verify` / `POST /verify-zk-link` read.
/// No key in it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkLinkFile {
    pub version: u32,
    pub network: String,
    /// The deposit `D` (txid).
    pub deposit: String,
    /// The funding payment `T` (txid).
    pub funding: String,
    /// One serialized one-action Ironwood bundle (v6 transaction encoding), hex, per input of `D`.
    pub proofs: Vec<String>,
}

/// What every signature in the proofs signs: binds them to this deposit and this funding payment.
pub fn challenge(deposit: &str, funding: &str) -> [u8; 32] {
    let h = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(PERSONAL)
        .to_state()
        .update(deposit.as_bytes())
        .update(b"|")
        .update(funding.as_bytes())
        .finalize();
    h.as_bytes().try_into().expect("32-byte hash")
}

/// Leaves of the synthetic tree: the funding payment's Ironwood output commitments, in order.
pub fn leaves(tx: &Transaction) -> Option<Vec<MerkleHashOrchard>> {
    let b = tx.ironwood_bundle()?;
    Some(b.actions().iter().map(|a| MerkleHashOrchard::from_cmx(a.cmx())).collect())
}

/// Root and authentication path for leaf `pos` in a depth-32 tree holding exactly `leaves`.
pub fn synthetic_path(leaves: &[MerkleHashOrchard], pos: usize) -> (MerkleHashOrchard, [MerkleHashOrchard; DEPTH]) {
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
    (nodes[0], auth.try_into().expect("DEPTH levels"))
}

/// The synthetic root, computed two independent ways (our path code and incrementalmerkletree).
pub fn synthetic_root(leaves: &[MerkleHashOrchard]) -> Result<Anchor, VerifyError> {
    let (root, _) = synthetic_path(leaves, 0);
    let mut tree = CommitmentTree::<MerkleHashOrchard, 32>::empty();
    for l in leaves {
        tree.append(*l).map_err(|_| VerifyError::BadRequest("the funding payment has too many outputs".into()))?;
    }
    if tree.root() != root {
        return Err(VerifyError::Upstream(anyhow::anyhow!("synthetic root disagrees with incrementalmerkletree")));
    }
    Ok(Anchor::from(root))
}

/// The Ironwood verifying key. Building it takes a few seconds: build it once and share it.
pub fn verifying_key() -> VerifyingKey {
    VerifyingKey::build(OrchardCircuitVersion::PostNu6_3)
}

/// One proof's result.
#[derive(Clone, Debug, Serialize)]
pub struct ProofResult {
    pub index: usize,
    pub valid: bool,
    /// Why it was rejected (null when valid).
    pub reason: Option<String>,
    /// The nullifier the proof is for, if the bundle could be read.
    pub nullifier: Option<String>,
    /// The proven value (null unless valid).
    pub value_zat: Option<u64>,
}

/// One input of the deposit.
#[derive(Clone, Debug, Serialize)]
pub struct ZkInput {
    /// Index of the Ironwood action in the deposit.
    pub action: usize,
    pub nullifier: String,
    /// The valid proof that covers this input, if any.
    pub proof: Option<usize>,
    pub value_zat: Option<u64>,
}

/// The cryptographic part of the check: no I/O, no age check.
#[derive(Clone, Debug, Serialize)]
pub struct ProofCheck {
    /// The synthetic root of the funding payment's outputs (hex).
    pub funding_root: String,
    pub funding_outputs: usize,
    pub proofs: Vec<ProofResult>,
    pub inputs: Vec<ZkInput>,
    pub proven_value_zat: u64,
}

impl ProofCheck {
    pub fn invalid(&self) -> Vec<&ProofResult> {
        self.proofs.iter().filter(|p| !p.valid).collect()
    }

    pub fn uncovered(&self) -> usize {
        self.inputs.iter().filter(|i| i.proof.is_none()).count()
    }
}

/// Check every proof in `file` against the funding payment and the deposit. Malformed input (bad hex,
/// an unreadable bundle, too many proofs) is a `BadRequest`; a proof that fails a check is reported
/// as invalid, with the reason.
pub fn check_proofs(vk: &VerifyingKey, funding: &Transaction, deposit: &Transaction, file: &ZkLinkFile) -> Result<ProofCheck, VerifyError> {
    if file.proofs.is_empty() || file.proofs.len() > MAX_PROOFS {
        return Err(VerifyError::BadRequest(format!("a file carries 1 to {MAX_PROOFS} proofs")));
    }
    let leaves = leaves(funding).ok_or_else(|| VerifyError::BadRequest("the funding payment has no Ironwood outputs".into()))?;
    let anchor = synthetic_root(&leaves)?;
    let c = challenge(&file.deposit, &file.funding);
    let deposit_nfs = link::nullifiers(deposit);
    let in_deposit: HashSet<[u8; 32]> = deposit_nfs.iter().copied().collect();

    let mut proofs = Vec::with_capacity(file.proofs.len());
    let mut covered: Vec<([u8; 32], usize, u64)> = vec![];
    for (i, p) in file.proofs.iter().enumerate() {
        let bytes = hex::decode(p).map_err(|_| VerifyError::BadRequest(format!("proof {i}: not hex")))?;
        let b = read_v6_bundle(&bytes[..], BranchId::Nu6_3, ValuePool::Ironwood)
            .map_err(|e| VerifyError::BadRequest(format!("proof {i}: not an Ironwood bundle ({e})")))?
            .ok_or_else(|| VerifyError::BadRequest(format!("proof {i}: empty bundle")))?;
        let mut r = ProofResult { index: i, valid: false, reason: None, nullifier: None, value_zat: None };
        let reject = |mut r: ProofResult, why: &str| {
            r.reason = Some(why.into());
            r
        };
        if b.bundle_version() != BundleVersion::ironwood_v3() {
            proofs.push(reject(r, "not an Ironwood bundle"));
            continue;
        }
        if b.actions().len() != 1 {
            proofs.push(reject(r, "expected exactly one action"));
            continue;
        }
        let a = b.actions().first();
        let nf = a.nullifier().to_bytes();
        r.nullifier = Some(hex::encode(nf));
        let v = i64::from(*b.value_balance());
        let r = if *b.anchor() != anchor {
            reject(r, "its anchor is not the root of the funding payment's outputs")
        } else if b.verify_proof(vk).is_err() {
            reject(r, "the zero-knowledge proof does not verify")
        } else if a.rk().verify(&c, a.authorization()).is_err() {
            reject(r, "the spend signature over this deposit and payment does not verify")
        } else if b.binding_validating_key().verify(&c, b.authorization().binding_signature()).is_err() {
            reject(r, "the binding signature does not verify (value not proven)")
        } else if v <= 0 {
            reject(r, "zero value proves nothing (the circuit skips the root check for zero-value notes)")
        } else if !in_deposit.contains(&nf) {
            reject(r, "its nullifier is not spent by the deposit")
        } else if covered.iter().any(|(n, _, _)| *n == nf) {
            reject(r, "duplicate: another proof already covers this input")
        } else {
            covered.push((nf, i, v as u64));
            ProofResult { valid: true, value_zat: Some(v as u64), ..r }
        };
        proofs.push(r);
    }

    let inputs = deposit_nfs
        .iter()
        .enumerate()
        .map(|(action, nf)| {
            let hit = covered.iter().find(|(n, _, _)| n == nf);
            ZkInput { action, nullifier: hex::encode(nf), proof: hit.map(|h| h.1), value_zat: hit.map(|h| h.2) }
        })
        .collect();
    Ok(ProofCheck {
        funding_root: hex::encode(anchor.to_bytes()),
        funding_outputs: leaves.len(),
        proofs,
        inputs,
        proven_value_zat: covered.iter().map(|c| c.2).sum(),
    })
}

/// The funding payment's age check, as it appears in the report.
#[derive(Clone, Debug, Serialize)]
pub struct FundingAge {
    pub txid: String,
    pub verdict: &'static str,
    pub summary: String,
    pub mined_height: Option<u32>,
    pub anchor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ZkLinkReport {
    pub network: &'static str,
    pub before: Before,
    /// `PASS`, `FAIL` or `NOT_PROVABLE`.
    pub verdict: &'static str,
    pub summary: String,
    /// True while the deposit or the funding payment is unmined.
    pub provisional: bool,
    pub deposit: DepositResult,
    pub funding: FundingAge,
    /// Null when the deposit is out of scope and the proofs weren't checked.
    pub proofs: Option<ProofCheck>,
    pub disclosed: &'static str,
    pub status: &'static str,
    pub elapsed_secs: f64,
}

/// Check a ZK link file against `before`. The file is only read; nothing is stored.
pub async fn verify_zk_link(
    client: &lwd::Client,
    network: VintageNetwork,
    cache: Option<&roots::Cache>,
    vk: &VerifyingKey,
    file: &ZkLinkFile,
    before: Before,
    concurrency: usize,
) -> Result<ZkLinkReport, VerifyError> {
    let t0 = Instant::now();
    if file.version != 1 {
        return Err(VerifyError::BadRequest(format!("unsupported file version {}", file.version)));
    }
    if file.network != network.name() {
        return Err(VerifyError::BadRequest(format!("file is for {}, this verifier is {}", file.network, network.name())));
    }
    if file.funding.eq_ignore_ascii_case(&file.deposit) {
        return Err(VerifyError::BadRequest("the funding payment can't be the deposit itself".into()));
    }
    let not_found = |what: &'static str, txid: String| {
        move |e| match e {
            VerifyError::NotFound => VerifyError::BadRequest(format!("{what} {txid} not found")),
            e => e,
        }
    };
    let d = link::fetch(client, network, &file.deposit).await.map_err(not_found("deposit", file.deposit.clone()))?;
    let t = link::fetch(client, network, &file.funding).await.map_err(not_found("funding payment", file.funding.clone()))?;
    let ds = tx::summarize(network, &d.raw, d.mined, d.mined.unwrap_or(d.tip + 1), None).map_err(|e| VerifyError::Upstream(e.into()))?;
    let age = report::verify(client, network, cache, &file.funding, before.clone(), concurrency).await?;

    let mut r = ZkLinkReport {
        network: network.name(),
        before: before.clone(),
        verdict: "NOT_PROVABLE",
        summary: String::new(),
        provisional: d.mined.is_none() || age.provisional,
        deposit: DepositResult { txid: ds.txid.clone(), mined_height: d.mined, reasons: ds.out_of_scope() },
        funding: FundingAge {
            txid: age.txid.clone(),
            verdict: age.verdict,
            summary: age.summary.clone(),
            mined_height: age.mined_height,
            anchor: age.anchor.clone(),
        },
        proofs: None,
        disclosed: DISCLOSED,
        status: STATUS,
        elapsed_secs: 0.0,
    };

    let phrase = report::before_phrase(before.height, &before.label);
    (r.verdict, r.summary) = if !r.deposit.reasons.is_empty() {
        ("NOT_PROVABLE", format!("Can't link this deposit: it has {}. The link only covers Ironwood inputs.", r.deposit.reasons.join(", ")))
    } else {
        // Proof verification is CPU-bound (about a second per proof): keep it off the async workers.
        let check = tokio::task::block_in_place(|| check_proofs(vk, &t.tx, &d.tx, file))?;
        let invalid = check.invalid();
        let uncovered = check.uncovered();
        let n = check.inputs.len();
        let out = if let Some(p) = invalid.first() {
            ("FAIL", format!("Can't link: proof {} is rejected: {}.", p.index, p.reason.as_deref().unwrap_or("invalid")))
        } else if uncovered > 0 {
            (
                "FAIL",
                format!(
                    "Can't link: {uncovered} of the deposit's {n} inputs have no proof. \
                     (A wallet's dummy input counts too: every input must be proven.)"
                ),
            )
        } else if age.verdict != "PASS" {
            ("FAIL", format!("Can't link: the funding payment doesn't pass the age check against {phrase}."))
        } else {
            ("PASS", format!("Every coin this deposit spends came from a payment whose coins existed before {phrase}."))
        };
        r.proofs = Some(check);
        out
    };
    if r.provisional {
        r.summary = format!("Provisional, not everything is mined yet: {} Check again once it is.", r.summary);
    }
    r.elapsed_secs = (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
    Ok(r)
}

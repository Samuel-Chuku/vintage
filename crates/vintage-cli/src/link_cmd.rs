//! `vintage link prove|verify`: the receipt-account link (see vintage_verifier::link).
//!
//! `prove` runs on the holder's machine: it reads the receipt account's viewing key from their own
//! zcash-devtool wallet and writes the bundle. `verify` does the same check as `POST /verify-link`,
//! locally, so a rail never has to send anyone a viewing key.

use std::{
    io::Write,
    path::PathBuf,
    process::Stdio,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use vintage_core::VintageNetwork;
use vintage_verifier::{
    link::{self, FundingRef, LinkBundle},
    load_registry, lwd, resolve_before, roots,
    zk_link::{self, ZkLinkFile},
};
use zcash_client_backend::data_api::{Account, WalletRead};
use zcash_client_sqlite::WalletDb;

use crate::send::fmt_amount;

#[derive(Subcommand)]
pub(crate) enum LinkCmd {
    /// Build a link bundle from your receipt wallet: its viewing key, the funding payments, the deposit.
    Prove {
        /// The deposit transaction (spends from the receipt account).
        #[arg(long)]
        deposit: String,
        /// The Vintage payment(s) that funded the receipt account, comma-separated.
        #[arg(long, value_delimiter = ',', required = true)]
        funding: Vec<String>,
        /// The receipt account's zcash-devtool wallet directory (single account).
        #[arg(long)]
        wallet: PathBuf,
        /// Write the bundle here (mode 600) instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Experimental: write zero-knowledge proofs instead of the viewing key. zcash-devtool proves and
        /// signs each input (Vintage never reads a key); needs --identity and exactly one --funding.
        #[arg(long, requires = "identity")]
        zk: bool,
        /// With --zk: the age identity file that zcash-devtool decrypts the receipt wallet's keys with.
        #[arg(short, long)]
        identity: Option<PathBuf>,
        /// With --zk: path to zcash-devtool (default: next to the vintage binary).
        #[arg(long)]
        devtool: Option<PathBuf>,
    },
    /// Check a link bundle, or a zero-knowledge link file (experimental), against a block height or event
    /// (exit code 0 PASS, 1 FAIL, 2 not provable).
    Verify {
        bundle: PathBuf,
        /// A block height, or an event id from the registry.
        #[arg(long)]
        before: String,
        #[arg(long)]
        json: bool,
        /// Root cache (SQLite). Default: ~/.cache/vintage/cache.sqlite.
        #[arg(long)]
        cache: Option<PathBuf>,
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
    },
}

pub(crate) async fn run(network: VintageNetwork, server: &str, registry: &std::path::Path, cmd: LinkCmd) -> Result<()> {
    match cmd {
        LinkCmd::Prove { deposit, funding, wallet, out, zk: false, .. } => prove(network, server, &deposit, &funding, &wallet, out).await,
        LinkCmd::Prove { deposit, funding, wallet, out, zk: true, identity, devtool } => {
            let [funding] = funding.as_slice() else {
                bail!("--zk takes exactly one --funding payment");
            };
            let identity = identity.ok_or_else(|| anyhow!("--zk needs --identity"))?;
            prove_zk(network, server, &deposit, funding, &wallet, &identity, devtool, out).await
        }
        LinkCmd::Verify { bundle, before, json, cache, concurrency } => {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&bundle).context("reading the bundle")?)
                .context("parsing the bundle")?;
            let before = resolve_before(|| load_registry(registry), network, &before)?;
            let client = lwd::connect(server).await?;
            let cache = match cache.or_else(roots::default_cache_path) {
                Some(p) => Some(roots::Cache::open(&p, network)?),
                None => None,
            };
            // A file with proofs and no viewing key is a zero-knowledge link.
            if v.get("proofs").is_some() {
                let z: ZkLinkFile = serde_json::from_value(v).context("parsing the zero-knowledge link file")?;
                return verify_zk(network, &client, cache.as_ref(), &z, before, json, concurrency).await;
            }
            let b: LinkBundle = serde_json::from_value(v).context("parsing the bundle")?;
            let r = link::verify_link(&client, network, cache.as_ref(), &b, before, concurrency).await?;
            let code = match r.verdict {
                "PASS" => 0,
                "FAIL" => 1,
                _ => 2,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                std::process::exit(code);
            }
            let zec = |z| fmt_amount(network, z);
            crate::style::banner(network.name());
            println!("{}{}  (mined {})", crate::style::key("deposit:     "), r.deposit.txid, r.deposit.mined_height.map_or("not yet".into(), |h| h.to_string()));
            println!("{}{}  ({})", crate::style::key("before H:    "), r.before.height, r.before.label);
            for f in &r.funding {
                println!("{}{}  {}  {}", crate::style::key("funding:     "), f.txid, f.verdict, f.anchor.as_deref().map_or("", |_| "(age check)"));
                for o in &f.received {
                    println!(
                        "               output {}  {:>16}  {}{}",
                        o.output,
                        zec(o.value_zat),
                        o.scope,
                        if o.spent_by_deposit { "  → spent by the deposit" } else { "" }
                    );
                }
            }
            for i in &r.inputs {
                match &i.funded_by {
                    Some(f) => println!("input {}:     {} from {}… output {}", i.action, zec(f.value_zat), &f.txid[..16], f.output),
                    None => println!("input {}:     NOT covered by the disclosed payments", i.action),
                }
            }
            println!("{}{}", crate::style::key("covered:     "), zec(r.covered_value_zat));
            println!("{}{}", crate::style::key("disclosed:   "), r.disclosed);
            println!("{}{:.2}s\n", crate::style::key("time:        "), r.elapsed_secs);
            println!("{}", crate::style::verdict(r.verdict, &r.summary));
            std::process::exit(code);
        }
    }
}

/// `vintage link verify` on a zero-knowledge link file (vintage_verifier::zk_link). Experimental.
async fn verify_zk(
    network: VintageNetwork,
    client: &lwd::Client,
    cache: Option<&roots::Cache>,
    z: &ZkLinkFile,
    before: vintage_verifier::report::Before,
    json: bool,
    concurrency: usize,
) -> Result<()> {
    if !json {
        eprintln!("building the verifying key …");
    }
    let vk = zk_link::verifying_key();
    let r = zk_link::verify_zk_link(client, network, cache, &vk, z, before, concurrency).await?;
    let code = match r.verdict {
        "PASS" => 0,
        "FAIL" => 1,
        _ => 2,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
        std::process::exit(code);
    }
    let zec = |z| fmt_amount(network, z);
    let key = crate::style::key;
    crate::style::banner(network.name());
    println!("{}{}  (mined {})", key("deposit:     "), r.deposit.txid, r.deposit.mined_height.map_or("not yet".into(), |h| h.to_string()));
    println!("{}{}  ({})", key("before H:    "), r.before.height, r.before.label);
    println!("{}{}  {}  (age check)", key("funding:     "), r.funding.txid, r.funding.verdict);
    if let Some(c) = &r.proofs {
        println!("{}{}  (root of its {} outputs)", key("synthetic:   "), c.funding_root, c.funding_outputs);
        for p in &c.proofs {
            match (&p.reason, p.value_zat) {
                (None, Some(v)) => println!("proof {}:     {} valid, {} from the funding payment", p.index, crate::style::gold("✓"), zec(v)),
                (reason, _) => println!("proof {}:     {} rejected: {}", p.index, key("×"), reason.as_deref().unwrap_or("invalid")),
            }
        }
        for i in &c.inputs {
            match i.proof {
                Some(p) => println!("input {}:     covered by proof {p}", i.action),
                None => println!("input {}:     NOT covered by any proof", i.action),
            }
        }
        println!("{}{}", key("proven:      "), zec(c.proven_value_zat));
    }
    println!("{}{}", key("disclosed:   "), r.disclosed);
    println!("{}{}", key("status:      "), r.status);
    println!("{}{:.2}s\n", key("time:        "), r.elapsed_secs);
    println!("{}", crate::style::verdict(if r.verdict == "PASS" { "ZK LINK PASS" } else { r.verdict }, &r.summary));
    std::process::exit(code);
}

/// The receipt account's viewing key, read from the holder's own wallet (no seed needed).
fn receipt_ufvk(network: VintageNetwork, wallet: &std::path::Path) -> Result<String> {
    let db = WalletDb::for_path(wallet.join("data.sqlite"), network.params(), (), ())?;
    let ids = db.get_account_ids()?;
    let [id] = ids.as_slice() else {
        bail!("the receipt wallet must have exactly one account (it has {})", ids.len());
    };
    let account = db.get_account(*id)?.ok_or_else(|| anyhow!("account not found"))?;
    account
        .ufvk()
        .ok_or_else(|| anyhow!("the account has no viewing key"))?
        .encode(&network.params())
        .map_err(|e| anyhow!("encoding the viewing key: {e:?}"))
}

/// `vintage link prove --zk`: one zero-knowledge proof per deposit input (vintage_verifier::zk_link).
///
/// Vintage reads only the receipt account's viewing key, to find which of the payment's outputs the
/// deposit spends, and computes each note's path in the tree of the payment's outputs and the message
/// to sign. zcash-devtool (`wallet disclose-spend`) reads the key, proves and signs. Vintage then runs
/// the same proof checks a rail runs before writing the file.
#[allow(clippy::too_many_arguments)]
async fn prove_zk(
    network: VintageNetwork,
    server: &str,
    deposit: &str,
    funding: &str,
    wallet: &std::path::Path,
    identity: &std::path::Path,
    devtool: Option<PathBuf>,
    out: Option<PathBuf>,
) -> Result<()> {
    let devtool_bin = crate::send::devtool_path(devtool)?;
    let fvk = link::decode_ufvk(network, &receipt_ufvk(network, wallet)?)?;
    let client = lwd::connect(server).await?;
    let t = link::fetch(&client, network, funding).await?;
    let d = link::fetch(&client, network, deposit).await?;
    let (funding, deposit) = (t.tx.txid().to_string(), d.tx.txid().to_string());
    let leaves = zk_link::leaves(&t.tx).ok_or_else(|| anyhow!("the funding payment has no Ironwood outputs"))?;
    let received = link::received_by(&t.tx, &fvk);
    if received.is_empty() {
        bail!("{funding}: none of its Ironwood outputs belong to this receipt account");
    }
    let message = hex::encode(zk_link::challenge(&deposit, &funding));
    let raw_hex = hex::encode(&t.raw);
    let d_nfs = link::nullifiers(&d.tx);
    eprintln!("funding {funding}: {} outputs, {} to this account", leaves.len(), received.len());
    eprintln!("deposit {deposit}: {} inputs", d_nfs.len());

    let mut proofs = vec![];
    for (i, nf) in d_nfs.iter().enumerate() {
        let Some(r) = received.iter().find(|r| r.nullifier == *nf) else {
            eprintln!("input {i}: not one of this account's outputs in the payment (a wallet's dummy input?); `link verify` will FAIL");
            continue;
        };
        let (root, auth) = zk_link::synthetic_path(&leaves, r.action);
        let auth_hex: String = auth.iter().map(|h| hex::encode(h.to_bytes())).collect();
        eprintln!("input {i}: output {} ({}), proving and signing in zcash-devtool …", r.action, fmt_amount(network, r.value));
        let mut child = crate::send::devtool(&devtool_bin, wallet)
            .arg("disclose-spend")
            .arg("-i")
            .arg(identity)
            .args(["--tx", "-", "--output", &r.action.to_string(), "--position", &r.action.to_string()])
            .args(["--anchor", &hex::encode(root.to_bytes()), "--auth-path", &auth_hex, "--message", &message])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("running zcash-devtool")?;
        child.stdin.take().expect("piped stdin").write_all(raw_hex.as_bytes())?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("zcash-devtool wallet disclose-spend failed (its error is above; it needs patches/zcash-devtool-disclose-spend.patch)");
        }
        let proof = String::from_utf8(output.stdout)?.lines().last().unwrap_or_default().trim().to_string();
        proofs.push(proof);
    }
    if proofs.is_empty() {
        bail!("the deposit spends none of this account's outputs in the payment");
    }

    let file = ZkLinkFile { version: 1, network: network.name().into(), deposit, funding, proofs };
    eprintln!("checking the proofs …");
    let check = zk_link::check_proofs(&zk_link::verifying_key(), &t.tx, &d.tx, &file)?;
    if let Some(p) = check.invalid().first() {
        bail!("proof {} doesn't verify: {}", p.index, p.reason.as_deref().unwrap_or("invalid"));
    }
    eprintln!(
        "{} of {} inputs proven, {} in total. Next: `vintage link verify <file> --before <event>` checks the payment's age too.",
        check.inputs.len() - check.uncovered(),
        check.inputs.len(),
        fmt_amount(network, check.proven_value_zat)
    );
    eprintln!("\nThis file discloses: {}\n{}", zk_link::DISCLOSED, zk_link::STATUS);
    let json = serde_json::to_string_pretty(&file)? + "\n";
    match out {
        Some(p) => {
            std::fs::OpenOptions::new().write(true).create_new(true).open(&p)?.write_all(json.as_bytes())?;
            eprintln!("written to {} (no key inside)", p.display());
        }
        None => print!("{json}"),
    }
    Ok(())
}

async fn prove(
    network: VintageNetwork,
    server: &str,
    deposit: &str,
    funding: &[String],
    wallet: &std::path::Path,
    out: Option<PathBuf>,
) -> Result<()> {
    let ufvk = receipt_ufvk(network, wallet)?;
    let fvk = link::decode_ufvk(network, &ufvk)?;

    let client = lwd::connect(server).await?;
    let d = link::fetch(&client, network, deposit).await?;
    let deposit_nfs = link::nullifiers(&d.tx);
    let mut refs = vec![];
    let mut matched = 0;
    for t in funding {
        let ft = link::fetch(&client, network, t).await?;
        let received = link::received_by(&ft.tx, &fvk);
        if received.is_empty() {
            bail!("{t}: none of its Ironwood outputs belong to this receipt account");
        }
        let spent: Vec<u32> =
            received.iter().filter(|r| deposit_nfs.contains(&r.nullifier)).map(|r| r.action as u32).collect();
        matched += spent.len();
        eprintln!("funding {t}: {} output(s) to this account, {} spent by the deposit", received.len(), spent.len());
        refs.push(FundingRef { txid: t.clone(), outputs: spent });
    }
    if matched < deposit_nfs.len() {
        eprintln!(
            "warning: the deposit has {} inputs and only {matched} come from these payments; `link verify` will FAIL. \
             (A one-note deposit always carries a dummy input: spend at least two receipt notes.)",
            deposit_nfs.len()
        );
    }
    let bundle = LinkBundle { version: 1, network: network.name().into(), ufvk, deposit: deposit.into(), funding: refs };
    let json = serde_json::to_string_pretty(&bundle)?;
    eprintln!("\nThis bundle discloses: {}", link::DISCLOSED);
    match out {
        Some(p) => {
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&p)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }
            f.write_all(json.as_bytes())?;
            eprintln!("bundle written to {} (mode 600)", p.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

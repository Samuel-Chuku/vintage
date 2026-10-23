//! `vintage link prove|verify`: the receipt-account link (see vintage_verifier::link).
//!
//! `prove` runs on the holder's machine: it reads the receipt account's viewing key from their own
//! zcash-devtool wallet and writes the bundle. `verify` does the same check as `POST /verify-link`,
//! locally, so a rail never has to send anyone a viewing key.

use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use vintage_core::VintageNetwork;
use vintage_verifier::{
    link::{self, FundingRef, LinkBundle},
    load_registry, lwd, resolve_before, roots,
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
    },
    /// Check a link bundle against a block height or event (exit code 0 PASS, 1 FAIL, 2 not provable).
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
        LinkCmd::Prove { deposit, funding, wallet, out } => prove(network, server, &deposit, &funding, &wallet, out).await,
        LinkCmd::Verify { bundle, before, json, cache, concurrency } => {
            let b: LinkBundle = serde_json::from_slice(&std::fs::read(&bundle).context("reading the bundle")?)
                .context("parsing the bundle")?;
            let before = resolve_before(|| load_registry(registry), network, &before)?;
            let client = lwd::connect(server).await?;
            let cache = match cache.or_else(roots::default_cache_path) {
                Some(p) => Some(roots::Cache::open(&p, network)?),
                None => None,
            };
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
            println!("deposit:     {}  (mined {})", r.deposit.txid, r.deposit.mined_height.map_or("not yet".into(), |h| h.to_string()));
            println!("before H:    {}  ({})", r.before.height, r.before.label);
            for f in &r.funding {
                println!("funding:     {}  {}  {}", f.txid, f.verdict, f.anchor.as_deref().map_or("", |_| "(age check)"));
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
            println!("covered:     {}", zec(r.covered_value_zat));
            println!("disclosed:   {}", r.disclosed);
            println!("time:        {:.2}s\n", r.elapsed_secs);
            println!("{} — {}", r.verdict, r.summary);
            std::process::exit(code);
        }
    }
}

async fn prove(
    network: VintageNetwork,
    server: &str,
    deposit: &str,
    funding: &[String],
    wallet: &std::path::Path,
    out: Option<PathBuf>,
) -> Result<()> {
    // The receipt account's viewing key, read from the holder's own wallet (no seed needed).
    let db = WalletDb::for_path(wallet.join("data.sqlite"), network.params(), (), ())?;
    let ids = db.get_account_ids()?;
    let [id] = ids.as_slice() else {
        bail!("the receipt wallet must have exactly one account (it has {})", ids.len());
    };
    let account = db.get_account(*id)?.ok_or_else(|| anyhow!("account not found"))?;
    let ufvk = account
        .ufvk()
        .ok_or_else(|| anyhow!("the account has no viewing key"))?
        .encode(&network.params())
        .map_err(|e| anyhow!("encoding the viewing key: {e:?}"))?;
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

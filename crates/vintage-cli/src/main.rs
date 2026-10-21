use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use vintage_core::{
    VintageNetwork,
    registry::{self, Registry},
    tx::{self, TxSummary},
    verify::{self, Verdict},
};

mod lwd;
mod registry_cmd;
mod witness;

#[derive(Parser)]
#[command(name = "vintage", about = "Prove your ZEC isn't the hacker's: with time, not a blocklist.")]
struct Cli {
    /// Network: main or test.
    #[arg(long, global = true, default_value = "main")]
    network: VintageNetwork,
    /// lightwalletd/Zaino gRPC server host:port (TLS). Defaults to zec.rocks for the network.
    #[arg(long, global = true)]
    server: Option<String>,
    /// Directory holding incidents.json, incidents.json.sig and pubkey.hex.
    #[arg(long, global = true, default_value = "registry")]
    registry: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show a transaction's Ironwood details: mined height, block time, anchor, value moved.
    Tx { txid: String },
    /// Check whether every input of a transaction existed before block H (or a registry incident).
    Verify {
        txid: String,
        /// A block height, or an incident id from the registry.
        #[arg(long)]
        before: String,
        /// Informational: search this many blocks below H to report the anchor's exact height.
        #[arg(long, default_value_t = 0)]
        locate_back: u32,
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
    },
    /// Read-only: ask a zcash-devtool wallet's Ironwood tree for each note's witness at a height.
    Witness {
        #[arg(long)]
        wallet: PathBuf,
        #[arg(long)]
        anchor_height: u32,
    },
    /// Incident registry: keygen, sign, check.
    #[command(subcommand)]
    Registry(registry_cmd::RegistryCmd),
}

fn load_registry(dir: &std::path::Path) -> Result<Registry> {
    let bytes = std::fs::read(dir.join("incidents.json")).context("reading incidents.json")?;
    let sig = std::fs::read_to_string(dir.join("incidents.json.sig")).context("reading incidents.json.sig")?;
    let pk = std::fs::read_to_string(dir.join("pubkey.hex")).context("reading pubkey.hex")?;
    Ok(registry::load_verified(&bytes, &sig, &pk)?)
}

/// Resolve `--before` to (height, label).
fn resolve_before(cli_registry: &std::path::Path, network: VintageNetwork, before: &str) -> Result<(u32, String)> {
    if let Ok(h) = before.parse::<u32>() {
        return Ok((h, format!("block {h}")));
    }
    let reg = load_registry(cli_registry)?;
    let inc = reg
        .find(before)
        .ok_or_else(|| anyhow!("no incident `{before}` in the registry"))?;
    if inc.network != network.name() {
        bail!("incident `{before}` is on {}, but --network is {}", inc.network, network.name());
    }
    Ok((inc.first_inflow_height, inc.label.clone()))
}

async fn fetch_summary(client: &lwd::Client, network: VintageNetwork, txid: &str) -> Result<(TxSummary, u32)> {
    let parsed = tx::parse_txid(txid)?;
    let raw = lwd::get_tx(client, &parsed).await?;
    let tip = lwd::tip(client).await?;
    // RawTransaction.height: 0 = mempool, u64::MAX (-1) = not mined (per the proto docs).
    let mined = u32::try_from(raw.height).ok().filter(|h| *h > 0);
    let summary = tx::summarize(network, &raw.data, mined, mined.unwrap_or(tip + 1), Some(&parsed))?;
    Ok((summary, tip))
}

fn fmt_time(unix: u32) -> String {
    // Minimal UTC formatting without extra dependencies.
    let secs = unix as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02} UTC")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let network = cli.network;
    let server = cli.server.clone().unwrap_or_else(|| network.default_server().to_string());

    match cli.cmd {
        Cmd::Registry(c) => registry_cmd::run(c, &cli.registry),
        Cmd::Witness { wallet, anchor_height } => witness::run(network, &wallet, anchor_height),
        Cmd::Tx { txid } => {
            let client = lwd::connect(&server).await?;
            let (s, tip) = fetch_summary(&client, network, &txid).await?;
            println!("txid:          {}", s.txid);
            println!("network:       {}", network.name());
            println!("version:       {}", s.version);
            match s.mined_height {
                Some(h) => println!(
                    "mined:         {h}  ({})",
                    fmt_time(lwd::block_time(&client, h).await?)
                ),
                None => println!("mined:         not yet (tip {tip})"),
            }
            println!("transparent:   {} input(s), {} output(s)", s.transparent_inputs, s.transparent_outputs);
            println!("sapling:       {} spend(s)", s.sapling_spends);
            println!("orchard:       spends {}", if s.orchard_spends_enabled { "enabled" } else { "none" });
            match &s.ironwood {
                None => println!("ironwood:      no bundle"),
                Some(i) => {
                    println!(
                        "ironwood:      {} action(s), spends {}, outputs {}",
                        i.actions,
                        if i.spends_enabled { "enabled" } else { "disabled" },
                        if i.outputs_enabled { "enabled" } else { "disabled" }
                    );
                    let v = i.value_balance;
                    if v < 0 {
                        println!("               {} zat ({:.8} ZEC) ENTERED Ironwood", -v, -v as f64 / 1e8);
                    } else {
                        println!("               {} zat ({:.8} ZEC) left Ironwood", v, v as f64 / 1e8);
                    }
                    println!("anchor:        {}", hex::encode(i.anchor));
                }
            }
            Ok(())
        }
        Cmd::Verify { txid, before, locate_back, concurrency } => {
            let (h, label) = resolve_before(&cli.registry, network, &before)?;
            let client = lwd::connect(&server).await?;
            let (s, tip) = fetch_summary(&client, network, &txid).await?;
            println!("txid:        {}", s.txid);
            println!("network:     {}", network.name());
            println!("mined:       {}", s.mined_height.map_or("not yet".into(), |h| h.to_string()));
            println!("chain tip:   {tip}");
            println!("before H:    {h}  ({label})");
            let reasons = s.out_of_scope();
            if !reasons.is_empty() {
                let v = Verdict::NotProvable { reasons };
                println!("\nFAIL — not provable: {}", match &v { Verdict::NotProvable { reasons } => reasons.join(", "), _ => unreachable!() });
                std::process::exit(2);
            }
            let iw = s.ironwood.as_ref().expect("in scope implies Ironwood bundle");
            println!("anchor:      {}", hex::encode(iw.anchor));
            if h > tip {
                bail!("H = {h} is above the chain tip {tip}");
            }
            let t = std::time::Instant::now();
            let (roots, empty) = lwd::roots_in_range(&client, h, tip, concurrency).await?;
            if empty == roots.len() {
                bail!("server returned an empty ironwoodTree for every height; it does not expose Ironwood roots");
            }
            println!("checked:     {} Ironwood roots for heights {h}..={tip} in {:.1}s", roots.len(), t.elapsed().as_secs_f64());
            if locate_back > 0 {
                let from = h.saturating_sub(locate_back);
                let (below, _) = lwd::roots_in_range(&client, from, h - 1, concurrency).await?;
                let found: Vec<u32> = below.iter().filter(|(_, r)| **r == iw.anchor).map(|(k, _)| *k).collect();
                match (found.first(), found.last()) {
                    (Some(lo), Some(hi)) => println!("located:     anchor = Ironwood root at heights {lo}..={hi}"),
                    _ => println!("located:     anchor not within {from}..{}", h - 1),
                }
            }
            println!();
            match verify::decide(&iw.anchor, &roots) {
                Verdict::Pass { roots_checked } => {
                    println!("PASS — all inputs existed before block {h} ({label})");
                    println!("evidence:    anchor matched none of the {roots_checked} Ironwood roots at heights {h}..={tip}; consensus requires it to be the root of an earlier block");
                    Ok(())
                }
                Verdict::Fail { matched_from, matched_to } => {
                    println!("FAIL — anchor is the Ironwood root at heights {matched_from}..={matched_to}, which is not before block {h} ({label})");
                    if matched_from == h {
                        println!("note:        no Ironwood notes were added between the anchor's block and H, so the root at H repeats it. The inputs may still predate H; choose an earlier boundary to prove it.");
                    }
                    std::process::exit(1);
                }
                Verdict::NotProvable { .. } => unreachable!(),
            }
        }
    }
}

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
mod roots;
mod send;
mod status;
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
        /// Print the verdict and evidence as JSON (exit code: 0 PASS, 1 FAIL, 2 not provable).
        #[arg(long)]
        json: bool,
        /// Root cache (SQLite). Default: ~/.cache/vintage/cache.sqlite.
        #[arg(long)]
        cache: Option<PathBuf>,
        /// Don't read or write the root cache.
        #[arg(long)]
        no_cache: bool,
    },
    /// Send from a zcash-devtool wallet with an anchor before block H, so the tx proves its inputs predate H.
    Send {
        /// zcash-devtool wallet directory.
        #[arg(long)]
        wallet: PathBuf,
        /// age identity file that decrypts the wallet seed (passed to zcash-devtool; vintage never reads it).
        #[arg(short, long)]
        identity: Option<PathBuf>,
        /// Recipient: Unified, Sapling or transparent address.
        #[arg(long)]
        to: String,
        /// Amount in zatoshis.
        #[arg(long)]
        value: u64,
        #[arg(long)]
        memo: Option<String>,
        /// A block height, or an incident id from the registry.
        #[arg(long)]
        before: String,
        /// Path to zcash-devtool (default: next to the vintage binary).
        #[arg(long)]
        devtool: Option<PathBuf>,
        /// Skip the sync (the wallet was just synced).
        #[arg(long)]
        no_sync: bool,
        /// Show the proposal and stop: nothing is signed or broadcast.
        #[arg(long)]
        dry_run: bool,
        /// Broadcast without the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Read-only: ask a zcash-devtool wallet's Ironwood tree for each note's witness at a height.
    Witness {
        #[arg(long)]
        wallet: PathBuf,
        #[arg(long)]
        anchor_height: u32,
    },
    /// For each registry incident: how much of a wallet's ZEC a Vintage send can prove predates it.
    Status {
        /// zcash-devtool wallet directory.
        #[arg(long)]
        wallet: PathBuf,
        /// Sync the wallet first (via zcash-devtool).
        #[arg(long)]
        sync: bool,
        /// Path to zcash-devtool (default: next to the vintage binary).
        #[arg(long)]
        devtool: Option<PathBuf>,
    },
    /// List txids with Ironwood actions in the last `blocks` blocks (test samples for benchmarks).
    #[command(hide = true)]
    Sample {
        #[arg(long, default_value_t = 50)]
        blocks: u32,
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
    let (raw, tip) = tokio::try_join!(lwd::get_tx(client, &parsed), lwd::tip(client))?;
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
        Cmd::Send { wallet, identity, to, value, memo, before, devtool, no_sync, dry_run, yes } => {
            let (h, label) = resolve_before(&cli.registry, network, &before)?;
            send::run(
                network,
                &server,
                send::Args { wallet, identity, to, value, memo, before: h, before_arg: before, label, devtool, no_sync, dry_run, yes },
            )
        }
        Cmd::Status { wallet, sync, devtool } => {
            let reg = load_registry(&cli.registry)?;
            status::run(network, &server, &reg, &wallet, sync, devtool)
        }
        Cmd::Sample { blocks } => {
            let client = lwd::connect(&server).await?;
            let tip = lwd::tip(&client).await?;
            for (h, txid) in lwd::ironwood_txids(&client, tip.saturating_sub(blocks) + 1, tip).await? {
                println!("{h} {txid}");
            }
            Ok(())
        }
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
        Cmd::Verify { txid, before, locate_back, concurrency, json, cache, no_cache } => {
            let t0 = std::time::Instant::now();
            let (h, label) = resolve_before(&cli.registry, network, &before)?;
            let client = lwd::connect(&server).await?;
            let (s, tip) = fetch_summary(&client, network, &txid).await?;
            let mut out = serde_json::json!({
                "txid": s.txid,
                "network": network.name(),
                "mined_height": s.mined_height,
                "chain_tip": tip,
                "before": { "height": h, "label": label, "requested": before },
            });
            if !json {
                println!("txid:        {}", s.txid);
                println!("network:     {}", network.name());
                println!("mined:       {}", s.mined_height.map_or("not yet".into(), |h| h.to_string()));
                println!("chain tip:   {tip}");
                println!("before H:    {h}  ({label})");
            }
            let reasons = s.out_of_scope();
            if !reasons.is_empty() {
                out["verdict"] = "NOT_PROVABLE".into();
                out["reasons"] = reasons.clone().into();
                if json {
                    println!("{out:#}");
                } else {
                    println!("\nFAIL — not provable: {}", reasons.join(", "));
                }
                std::process::exit(2);
            }
            let iw = s.ironwood.as_ref().expect("in scope implies Ironwood bundle");
            out["anchor"] = hex::encode(iw.anchor).into();
            if !json {
                println!("anchor:      {}", hex::encode(iw.anchor));
            }
            if h > tip {
                bail!("H = {h} is above the chain tip {tip}");
            }
            let mut cache = match (no_cache, cache.or_else(roots::default_cache_path)) {
                (false, Some(p)) => Some(roots::Cache::open(&p, network)?),
                _ => None,
            };
            let r = roots::roots_for(&client, cache.as_mut(), h, tip, concurrency).await?;
            let st = &r.stats;
            out["evidence"] = serde_json::json!({
                "blocks_covered": st.blocks,
                "distinct_roots": r.runs.len(),
                "blocks_downloaded": st.blocks_downloaded,
                "blocks_from_cache": st.blocks_from_cache,
                "server_tree_checks": st.server_tree_checks,
            });
            if !json {
                println!(
                    "checked:     {} blocks ({h}..={tip}) = {} distinct Ironwood roots, in {:.1}s",
                    st.blocks,
                    r.runs.len(),
                    st.secs
                );
                println!(
                    "             {} blocks downloaded and computed, {} from cache; {} server tree checks passed",
                    st.blocks_downloaded, st.blocks_from_cache, st.server_tree_checks
                );
            }
            let verdict = verify::decide_runs(&iw.anchor, &r.runs);
            out["elapsed_secs"] = ((t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0).into();
            if json {
                match verdict {
                    Verdict::Pass { .. } => out["verdict"] = "PASS".into(),
                    Verdict::Fail { matched_from, matched_to } => {
                        out["verdict"] = "FAIL".into();
                        out["matched_heights"] = serde_json::json!([matched_from, matched_to]);
                        out["root_existed_before_h"] = (matched_from == h && r.size_before == r.runs[0].0.size).into();
                    }
                    Verdict::NotProvable { .. } => unreachable!(),
                }
                println!("{out:#}");
                if !matches!(verdict, Verdict::Pass { .. }) {
                    std::process::exit(1);
                }
                return Ok(());
            }
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
            match verdict {
                Verdict::Pass { roots_checked } => {
                    println!("PASS — all inputs existed before block {h} ({label})");
                    println!("evidence:    anchor matched none of the {roots_checked} distinct Ironwood roots at heights {h}..={tip}; consensus requires it to be the root of an earlier block");
                    Ok(())
                }
                Verdict::Fail { matched_from, matched_to } => {
                    println!("FAIL — anchor is the Ironwood root at heights {matched_from}..={matched_to}, which is not before block {h} ({label})");
                    if matched_from == h && r.size_before == r.runs[0].0.size {
                        println!("note:        that same root already existed at block {} (no Ironwood notes were added in between), so the inputs may still predate H; anchoring to an earlier grid height would prove it.", h - 1);
                    }
                    std::process::exit(1);
                }
                Verdict::NotProvable { .. } => unreachable!(),
            }
        }
    }
}

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use vintage_core::{
    VintageNetwork,
    tx::{self, TxSummary},
};
use vintage_verifier::{load_registry, lwd, report, resolve_before, roots};

mod link_cmd;
mod registry_cmd;
mod request;
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
    /// Directory holding events.json (or the older incidents.json), its .sig, and pubkey.hex.
    #[arg(long, global = true, default_value = "registry")]
    registry: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show a transaction's Ironwood details: mined height, block time, anchor, value moved.
    Tx { txid: String },
    /// Check whether every input of a transaction existed before block H (or a registry event).
    Verify {
        txid: String,
        /// A block height, or an event id from the registry.
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
        /// Also show which outputs went to this viewing key and how much: a UFVK, or @file to read it
        /// from a file (keeps it out of shell history). Checked locally; never sent anywhere.
        #[arg(long)]
        received_by: Option<String>,
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
        #[arg(long, required_unless_present = "uri", conflicts_with = "uri", requires = "value")]
        to: Option<String>,
        /// Amount in zatoshis.
        #[arg(long, conflicts_with = "uri")]
        value: Option<u64>,
        #[arg(long, conflicts_with = "uri")]
        memo: Option<String>,
        /// Pay a ZIP-321 request instead (any number of payments), e.g. one from `vintage request`.
        #[arg(long)]
        uri: Option<String>,
        /// A block height, or an event id from the registry. Optional with --uri when the request's
        /// memo says `VINTAGE before=<H>`.
        #[arg(long)]
        before: Option<String>,
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
    /// Ask a payer for a Vintage payment: prints a ZIP-321 URI whose memo says `VINTAGE before=<H>`.
    Request {
        /// Recipient address (repeat with --amount for several payments).
        #[arg(long, required = true)]
        to: Vec<String>,
        /// Amount in ZEC, e.g. 0.003 (one per --to, in the same order).
        #[arg(long, required = true)]
        amount: Vec<String>,
        /// A block height, or an event id from the registry.
        #[arg(long)]
        before: String,
    },
    /// Read-only: ask a zcash-devtool wallet's Ironwood tree for each note's witness at a height.
    Witness {
        #[arg(long)]
        wallet: PathBuf,
        #[arg(long)]
        anchor_height: u32,
    },
    /// For each registry event: how much of a wallet's ZEC a Vintage send can prove predates it.
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
    /// The receipt-account link: prove fresh coins came from Vintage payments (discloses a viewing key).
    #[command(subcommand)]
    Link(link_cmd::LinkCmd),
    /// Event registry: keygen, sign, check.
    #[command(subcommand)]
    Registry(registry_cmd::RegistryCmd),
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
        Cmd::Request { to, amount, before } => {
            let b = resolve_before(|| load_registry(&cli.registry), network, &before)?;
            request::run(network, &to, &amount, b.height, &b.label)
        }
        Cmd::Send { wallet, identity, to, value, memo, uri, before, devtool, no_sync, dry_run, yes } => {
            let pay = match (uri, to, value) {
                (Some(u), _, _) => send::Pay::Uri(u),
                (None, Some(to), Some(value)) => send::Pay::One { to, value, memo },
                _ => anyhow::bail!("give --to and --value, or --uri"),
            };
            let from_memo = match &pay {
                send::Pay::Uri(u) => request::before_from_request(
                    &zip321::TransactionRequest::from_uri(u).map_err(|e| anyhow::anyhow!("invalid --uri: {e:?}"))?,
                )?,
                _ => None,
            };
            let before = match (before, from_memo) {
                (Some(b), Some(m)) if b.parse::<u32>().ok() != Some(m) && resolve_before(|| load_registry(&cli.registry), network, &b)?.height != m => {
                    anyhow::bail!("--before {b} disagrees with the request's memo (VINTAGE before={m})")
                }
                (Some(b), _) => b,
                (None, Some(m)) => {
                    println!("before H from the request's memo: VINTAGE before={m}");
                    m.to_string()
                }
                (None, None) => anyhow::bail!("--before is required (the request carries no Vintage memo)"),
            };
            let b = resolve_before(|| load_registry(&cli.registry), network, &before)?;
            send::run(
                network,
                &server,
                send::Args {
                    wallet,
                    identity,
                    pay,
                    before: b.height,
                    before_arg: before,
                    label: b.label,
                    devtool,
                    no_sync,
                    dry_run,
                    yes,
                },
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
        Cmd::Link(c) => link_cmd::run(network, &server, &cli.registry, c).await,
        Cmd::Verify { txid, before, locate_back, concurrency, json, cache, no_cache, received_by } => {
            let before = resolve_before(|| load_registry(&cli.registry), network, &before)?;
            let client = lwd::connect(&server).await?;
            let cache = match (no_cache, cache.or_else(roots::default_cache_path)) {
                (false, Some(p)) => Some(roots::Cache::open(&p, network)?),
                _ => None,
            };
            let r = report::verify(&client, network, cache.as_ref(), &txid, before, concurrency).await?;
            let code = match r.verdict {
                "PASS" => 0,
                "FAIL" => 1,
                _ => 2,
            };
            // --received-by: decrypt this tx's Ironwood outputs with the given viewing key, locally.
            let received = match &received_by {
                None => None,
                Some(k) => {
                    let key = match k.strip_prefix('@') {
                        Some(path) => std::fs::read_to_string(path)?,
                        None => k.clone(),
                    };
                    let fvk = vintage_verifier::link::decode_ufvk(network, &key)?;
                    let t = vintage_verifier::link::fetch(&client, network, &txid).await?;
                    Some(vintage_verifier::link::received_by(&t.tx, &fvk))
                }
            };
            if json {
                let mut v = serde_json::to_value(&r)?;
                if let Some(rs) = &received {
                    v["received_by_key"] = serde_json::json!({
                        "total_zat": rs.iter().map(|x| x.value).sum::<u64>(),
                        "outputs": rs.iter().map(|x| serde_json::json!({
                            "output": x.action, "value_zat": x.value, "scope": x.scope, "memo": x.memo,
                        })).collect::<Vec<_>>(),
                    });
                }
                println!("{}", serde_json::to_string_pretty(&v)?);
                std::process::exit(code);
            }
            let (h, label) = (r.before.height, &r.before.label);
            println!("txid:        {}", r.txid);
            println!("network:     {}", r.network);
            println!("mined:       {}", r.mined_height.map_or("not yet".into(), |h| h.to_string()));
            println!("chain tip:   {}", r.chain_tip);
            println!("before H:    {h}  ({label})");
            if let Some(a) = &r.anchor {
                println!("anchor:      {a}");
            }
            if let Some(e) = &r.evidence {
                println!(
                    "checked:     {} blocks ({h}..={}) = {} distinct Ironwood roots",
                    e.blocks_covered, r.chain_tip, e.distinct_roots
                );
                println!(
                    "             {} blocks downloaded and computed, {} from cache; {} server tree checks passed",
                    e.blocks_downloaded, e.blocks_from_cache, e.server_tree_checks
                );
            }
            println!("time:        {:.2}s", r.elapsed_secs);
            if let Some(rs) = &received {
                let total: u64 = rs.iter().map(|x| x.value).sum();
                println!("received:    {} by this viewing key, in {} output(s)", send::fmt_amount(network, total), rs.len());
                for x in rs {
                    println!(
                        "             output {}  {}  {}{}",
                        x.action,
                        send::fmt_amount(network, x.value),
                        x.scope,
                        x.memo.as_deref().map_or(String::new(), |m| format!("  memo {m:?}"))
                    );
                }
            }
            if locate_back > 0 && let Some(a) = &r.anchor {
                let from = h.saturating_sub(locate_back);
                let (below, _) = lwd::roots_in_range(&client, from, h - 1, concurrency).await?;
                let found: Vec<u32> =
                    below.iter().filter(|(_, root)| hex::encode(root) == *a).map(|(k, _)| *k).collect();
                match (found.first(), found.last()) {
                    (Some(lo), Some(hi)) => println!("located:     anchor = Ironwood root at heights {lo}..={hi}"),
                    _ => println!("located:     anchor not within {from}..{}", h - 1),
                }
            }
            println!();
            match (r.verdict, r.matched_heights) {
                ("PASS", _) => {
                    println!("PASS — {}", r.summary);
                    if let Some(e) = &r.evidence {
                        println!(
                            "evidence:    the anchor matched none of the {} distinct Ironwood roots at heights {h}..={}; consensus requires it to be the root of an earlier block",
                            e.distinct_roots, r.chain_tip
                        );
                    }
                }
                ("FAIL", Some([lo, hi])) => {
                    println!("FAIL — {}", r.summary);
                    println!("evidence:    the anchor is the Ironwood root at heights {lo}..={hi}, not before block {h}");
                }
                _ => println!("FAIL — not provable: {}", r.summary),
            }
            std::process::exit(code);
        }
    }
}

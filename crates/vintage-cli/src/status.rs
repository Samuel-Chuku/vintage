//! `vintage status --wallet <dir>`: for each registry event, how much of the wallet's ZEC a Vintage
//! send could prove predates it right now.
//!
//! A note counts for event H when it was mined at or before H_a (the newest grid boundary below H)
//! and the wallet holds a retained checkpoint at H_a to build its witness against.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};
use vintage_core::{VintageNetwork, grid, registry::Registry};

use crate::send::{self, fmt_amount};

pub(crate) fn run(
    network: VintageNetwork,
    server: &str,
    registry: &Registry,
    wallet: &Path,
    sync: bool,
    devtool: Option<PathBuf>,
) -> Result<()> {
    if sync {
        let status = send::devtool(&send::devtool_path(devtool)?, wallet)
            .args(["sync", "-s", server])
            .stdout(Stdio::null())
            .status()
            .context("running zcash-devtool sync")?;
        if !status.success() {
            bail!("zcash-devtool sync failed ({status})");
        }
    }
    let db_path = wallet.join("data.sqlite");
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", db_path.display()))?;
    let notes = send::unspent_ironwood_notes(&conn)?;
    let total: u64 = notes.iter().map(|n| n.value).sum();
    let tip: Option<u32> = conn.query_row("SELECT MAX(height) FROM blocks", [], |r| r.get(0))?;

    println!("wallet:    {}", wallet.display());
    println!("synced to: {}", tip.map_or("never".into(), |h| h.to_string()));
    println!("Ironwood:  {} in {} unspent note(s)\n", fmt_amount(network, total), notes.len());

    let events: Vec<_> = registry.events.iter().filter(|e| e.network == network.name()).collect();
    if events.is_empty() {
        println!("no {} events in the registry", network.name());
    }
    for inc in events {
        let h = inc.height;
        println!("{}  (block {h})", inc.id);
        println!("  {}", inc.label);
        let Some(h_a) = grid::anchor_height_for(h) else { continue };
        let old: u64 = notes.iter().filter(|n| n.mined_height.is_some_and(|m| m <= h_a)).map(|n| n.value).sum();
        if old == 0 {
            println!("  ✗ nothing provable: no unspent notes received at or before block {h_a}\n");
        } else if !send::has_retained_checkpoint(&conn, h_a)? {
            println!(
                "  ✗ {} predates it, but the wallet has no checkpoint at block {h_a}; rescan from before it\n",
                fmt_amount(network, old)
            );
        } else {
            println!("  ✓ {} provably predates it (anchor block {h_a})", fmt_amount(network, old));
            println!("    vintage send --before {} …\n", inc.id);
        }
    }
    Ok(())
}

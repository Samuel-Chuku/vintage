//! `vintage witness --wallet <dir> --anchor-height <H_a>`
//!
//! Read-only probe of a zcash-devtool wallet: for every Ironwood note the wallet has received, ask
//! the wallet's own Ironwood note commitment tree for the note's Merkle path (witness) as of the
//! checkpoint at `H_a`. A note created after `H_a` is not in that tree, so no witness can exist and
//! the tree reports `NotContained`. That is the exclusion property under test (spike Step 4).

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use incrementalmerkletree::Position;
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rusqlite::{Connection, OpenFlags};
use zcash_client_backend::data_api::WalletCommitmentTrees;
use zcash_client_sqlite::{WalletDb, util::SystemClock};
use vintage_core::VintageNetwork;
use zcash_protocol::consensus::BlockHeight;

struct NoteRow {
    id: i64,
    txid: String,
    mined_height: Option<u32>,
    value: u64,
    position: u64,
    spent: bool,
}

pub(crate) fn run(network: VintageNetwork, wallet_dir: &Path, anchor_height: u32) -> Result<()> {
    let db_path = wallet_dir.join("data.sqlite");

    // 1. List the wallet's Ironwood notes (read-only connection).
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", db_path.display()))?;
    let checkpoint_present: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ironwood_tree_checkpoints WHERE checkpoint_id = ?1)",
        [anchor_height],
        |r| r.get(0),
    )?;
    let retained: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ironwood_tree_retained_checkpoints WHERE checkpoint_id = ?1)",
        [anchor_height],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT n.id, t.txid, t.mined_height, n.value, n.commitment_tree_position,
                EXISTS(SELECT 1 FROM ironwood_received_note_spends s
                       WHERE s.ironwood_received_note_id = n.id)
         FROM ironwood_received_notes n
         JOIN transactions t ON t.id_tx = n.transaction_id
         WHERE n.commitment_tree_position IS NOT NULL
         ORDER BY n.commitment_tree_position",
    )?;
    let notes: Vec<NoteRow> = stmt
        .query_map([], |r| {
            let mut txid: Vec<u8> = r.get(1)?;
            txid.reverse();
            Ok(NoteRow {
                id: r.get(0)?,
                txid: hex::encode(txid),
                mined_height: r.get(2)?,
                value: r.get(3)?,
                position: r.get(4)?,
                spent: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    drop(conn);

    println!("wallet:            {}", wallet_dir.display());
    println!("anchor height H_a: {anchor_height}");
    println!(
        "checkpoint at H_a: {} (durable/retained: {})",
        if checkpoint_present { "present" } else { "ABSENT" },
        retained
    );

    // 2. Ask the wallet's Ironwood shard tree for each note's witness at H_a.
    let mut db = WalletDb::for_path(&db_path, network.params(), SystemClock, UnwrapErr(SysRng))?;
    let h_a = BlockHeight::from_u32(anchor_height);
    let report = db
        .with_ironwood_tree_mut::<_, _, anyhow::Error>(|tree| {
            let root = match tree.root_at_checkpoint_id(&h_a) {
                Ok(Some(r)) => hex::encode(r.to_bytes()),
                Ok(None) => "no checkpoint".to_string(),
                Err(e) => format!("error: {e}"),
            };
            let mut lines = vec![];
            for n in &notes {
                let outcome = match tree.witness_at_checkpoint_id(Position::from(n.position), &h_a) {
                    Ok(Some(_path)) => "WITNESS OK (spendable under this anchor)".to_string(),
                    Ok(None) => "no checkpoint at H_a".to_string(),
                    Err(e) => format!("NO WITNESS: {e}"),
                };
                lines.push((n, outcome));
            }
            Ok((root, lines.into_iter().map(|(n, o)| (n.id, o)).collect::<Vec<_>>()))
        })?
        .ok_or_else(|| anyhow!("wallet has no Ironwood tree"))?;

    println!("tree root at H_a:  {}", report.0);
    println!();
    for (n, (_, outcome)) in notes.iter().zip(report.1.iter()) {
        println!(
            "note {:>3}  pos {:>8}  mined {:>9}  {:>12} zat  {}  tx {}…\n          -> {}",
            n.id,
            n.position,
            n.mined_height.map_or("unmined".into(), |h| h.to_string()),
            n.value,
            if n.spent { "spent  " } else { "unspent" },
            &n.txid[..16],
            outcome
        );
    }
    Ok(())
}

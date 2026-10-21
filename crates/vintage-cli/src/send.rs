//! `vintage send --wallet <dir> --identity <age file> --to <addr> --value <zat> --before <H|incident>`
//!
//! A send whose Ironwood anchor is the newest ZIP-318 grid boundary below `H`, so the transaction
//! itself proves every input existed before `H`.
//!
//! librustzcash has no "propose at this anchor height" API: the anchor is always
//! `target − confirmations`, with the target read from the wallet's stored chain tip
//! (`get_target_and_anchor_heights`). So Vintage picks the confirmations: `N = target − H_a`.
//! Nothing syncs between reading the target and proposing, so the tip can't move under us.
//!
//! Vintage never touches the seed. It syncs and signs by running zcash-devtool (`wallet sync`,
//! `wallet send --min-confirmations N`); devtool re-proposes from the same unchanged wallet DB and so
//! reaches the same anchor. Vintage proposes first, read-only, to check the anchor and show the user
//! what will be spent, and after broadcast checks the signed transaction's anchor itself.

use std::{
    io::{BufRead, BufReader, Write},
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow, bail};
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rusqlite::{Connection, OpenFlags};
use vintage_core::{VintageNetwork, grid, tx};
use zcash_address::ZcashAddress;
use zcash_client_backend::{
    data_api::{
        WalletCommitmentTrees, WalletRead,
        wallet::{
            ConfirmationsPolicy,
            input_selection::{GreedyInputSelector, SpendPolicy},
            propose_transfer,
        },
    },
    fees::{DustOutputPolicy, SplitPolicy, StandardFeeRule, standard::MultiOutputChangeStrategy},
};
use zcash_client_sqlite::{WalletDb, util::SystemClock, wallet::commitment_tree};
use zcash_protocol::{
    ShieldedPool,
    consensus::BlockHeight,
    memo::{Memo, MemoBytes},
    value::Zatoshis,
};
use zip321::{Payment, TransactionRequest};

// Note management, passed to devtool explicitly so both proposals use the same change strategy.
// These are devtool's own defaults.
const TARGET_NOTE_COUNT: usize = 4;
const MIN_SPLIT_OUTPUT_VALUE: u64 = 10_000_000;

pub(crate) struct Args {
    pub wallet: PathBuf,
    pub identity: Option<PathBuf>,
    pub to: String,
    pub value: u64,
    pub memo: Option<String>,
    pub before: u32,
    /// What the user passed to --before (a height or an incident id), echoed in the verify hint.
    pub before_arg: String,
    pub label: String,
    pub devtool: Option<PathBuf>,
    pub no_sync: bool,
    pub dry_run: bool,
    pub yes: bool,
}

struct NoteRow {
    txid: String,
    mined_height: Option<u32>,
    value: u64,
}

fn fmt_amount(network: VintageNetwork, zat: u64) -> String {
    let unit = if network == VintageNetwork::Main { "ZEC" } else { "TAZ" };
    format!("{}.{:08} {unit}", zat / 100_000_000, zat % 100_000_000)
}

/// zcash-devtool sits next to `vintage` in the shared release target dir.
fn devtool_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let exe = std::env::current_exe()?;
    let p = exe.with_file_name("zcash-devtool");
    if !p.exists() {
        bail!("zcash-devtool not found at {}; pass --devtool <path>", p.display());
    }
    Ok(p)
}

fn devtool(bin: &Path, wallet: &Path) -> Command {
    let mut c = Command::new(bin);
    if std::env::var_os("RUST_LOG").is_none() {
        c.env("RUST_LOG", "warn");
    }
    c.arg("wallet").arg("-w").arg(wallet);
    c
}

pub(crate) fn run(network: VintageNetwork, server: &str, a: Args) -> Result<()> {
    let zec = |zat| fmt_amount(network, zat);
    let devtool_bin = devtool_path(a.devtool.clone())?;
    let db_path = a.wallet.join("data.sqlite");
    let h_a = grid::anchor_height_for(a.before)
        .ok_or_else(|| anyhow!("no grid boundary below block {}", a.before))?;

    println!("before H:      {}  ({})", a.before, a.label);
    println!("anchor H_a:    {h_a}  (newest multiple of 144 below H)");

    // 1. Sync.
    if a.no_sync {
        println!("sync:          skipped (--no-sync)");
    } else {
        println!("sync:          running zcash-devtool wallet sync …");
        let status = devtool(&devtool_bin, &a.wallet)
            .args(["sync", "-s", server])
            .stdout(Stdio::null())
            .status()
            .context("running zcash-devtool sync")?;
        if !status.success() {
            bail!("zcash-devtool sync failed ({status})");
        }
    }

    // 2. The wallet must hold a durable checkpoint at H_a, or it can't build witnesses there.
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", db_path.display()))?;
    let retained: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ironwood_tree_retained_checkpoints WHERE checkpoint_id = ?1)",
        [h_a],
        |r| r.get(0),
    )?;
    if !retained {
        bail!(
            "the wallet has no retained Ironwood checkpoint at block {h_a}. It must have been \
             scanning before then: restore or rescan it from a birthday below {h_a}."
        );
    }
    println!("checkpoint:    retained at {h_a} ✓");

    // Unspent Ironwood notes, split by whether they existed at H_a.
    let mut stmt = conn.prepare(
        "SELECT t.txid, t.mined_height, n.value
         FROM ironwood_received_notes n
         JOIN transactions t ON t.id_tx = n.transaction_id
         WHERE NOT EXISTS(SELECT 1 FROM ironwood_received_note_spends s
                          WHERE s.ironwood_received_note_id = n.id)
         ORDER BY t.mined_height",
    )?;
    let notes: Vec<NoteRow> = stmt
        .query_map([], |r| {
            let mut txid: Vec<u8> = r.get(0)?;
            txid.reverse();
            Ok(NoteRow { txid: hex::encode(txid), mined_height: r.get(1)?, value: r.get(2)? })
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    drop(conn);
    let qualifies = |n: &NoteRow| n.mined_height.is_some_and(|h| h <= h_a);
    let qualifying: u64 = notes.iter().filter(|n| qualifies(n)).map(|n| n.value).sum();
    println!("\nunspent Ironwood notes:");
    for n in &notes {
        println!(
            "  {}  {:>17}  mined {:>9}  tx {}…",
            if qualifies(n) { "✓ usable  " } else { "✗ too new " },
            zec(n.value),
            n.mined_height.map_or("unmined".into(), |h| h.to_string()),
            &n.txid[..16]
        );
    }
    println!("usable before H: {}  (only notes received at or before block {h_a} can be used)", zec(qualifying));

    // 3. Read the target from the wallet and set confirmations so the anchor lands on H_a.
    let mut db = WalletDb::for_path(&db_path, network.params(), SystemClock, UnwrapErr(SysRng))?;
    let (target, _) = db
        .get_target_and_anchor_heights(NonZeroU32::MIN)?
        .ok_or_else(|| anyhow!("wallet has no chain data; sync it first"))?;
    let target = u32::from(target);
    let n = grid::confirmations_for(target, h_a)
        .and_then(NonZeroU32::new)
        .ok_or_else(|| anyhow!("wallet target height {target} is not above the anchor {h_a}"))?;
    let age = n.get() - 1;
    println!("\ntarget height: {target}  (wallet tip + 1)");
    println!("confirmations: {n}  → anchor = {target} − {n} = {h_a}");
    println!("anchor age:    {age} blocks (~{:.1} h at 75 s/block)", age as f64 * 75.0 / 3600.0);

    let accounts = db.get_account_ids()?;
    let [account] = accounts.as_slice() else {
        bail!("wallet has {} accounts; vintage send supports single-account wallets", accounts.len());
    };

    // 4. Propose (read-only: no keys involved).
    let payment = Payment::new(
        ZcashAddress::from_str(&a.to).map_err(|e| anyhow!("invalid --to address: {e}"))?,
        Some(Zatoshis::from_u64(a.value).map_err(|_| anyhow!("invalid --value"))?),
        a.memo
            .as_deref()
            .map(Memo::from_str)
            .transpose()
            .map_err(|e| anyhow!("invalid memo: {e:?}"))?
            .map(MemoBytes::from),
        None,
        None,
        vec![],
    )
    .map_err(|e| anyhow!("{e:?}"))?;
    let request = TransactionRequest::new(vec![payment]).map_err(|e| anyhow!("{e:?}"))?;
    let change_strategy = MultiOutputChangeStrategy::new(
        StandardFeeRule::Zip317,
        None,
        ShieldedPool::Orchard,
        DustOutputPolicy::default(),
        SplitPolicy::with_min_output_value(
            NonZeroUsize::new(TARGET_NOTE_COUNT).unwrap(),
            Zatoshis::from_u64(MIN_SPLIT_OUTPUT_VALUE)?,
        ),
    );
    let proposal = propose_transfer::<_, _, _, _, commitment_tree::Error>(
        &mut db,
        &network.params(),
        *account,
        &GreedyInputSelector::new(),
        &change_strategy,
        request,
        // Same policy devtool builds from `--min-confirmations N`.
        ConfirmationsPolicy::new_symmetrical(n, true),
        &SpendPolicy::default(),
        None,
        None,
    )
    .map_err(|e| {
        anyhow!(
            "could not build the transaction: {e}\n\
             Only notes received at or before block {h_a} can be used; this wallet has {} of them.",
            zec(qualifying)
        )
    })?;

    if proposal.steps().len() != 1 {
        bail!("multi-step proposals are not supported");
    }
    let step = proposal.steps().first();
    let anchor = step.anchor_height().map(u32::from);
    if anchor != Some(h_a) {
        bail!("proposal anchor is {anchor:?}, expected {h_a}; refusing to continue");
    }
    let inputs = step.shielded_inputs().ok_or_else(|| anyhow!("proposal spends no shielded notes"))?;
    let fee = step.balance().fee_required().into_u64();
    println!("\nproposal (anchor {h_a} ✓):");
    for note in inputs.notes().iter() {
        println!(
            "  spend   {:>17}  mined {:>9}  {}:{}",
            zec(note.note().value().into_u64()),
            note.mined_height().map_or("?".into(), |h| u32::from(h).to_string()),
            note.txid(),
            note.output_index()
        );
    }
    println!("  pay     {:>17}  to {}", zec(a.value), a.to);
    for change in step.balance().proposed_change() {
        println!("  change  {:>17}  back to this wallet", zec(change.value().into_u64()));
    }
    println!("  fee     {:>17}", zec(fee));
    if let Some(m) = &a.memo {
        println!("  memo    {m:?}");
    }

    println!(
        "\nheads-up: this transaction will carry an anchor {age} blocks old. Ordinary wallets use an \
         anchor a few blocks old, so anyone looking at the chain can tell this was a Vintage spend. \
         That's fine for a deposit to an exchange or bridge that already knows you; don't use it for \
         private payments."
    );

    if a.dry_run {
        println!("\ndry run: nothing signed or broadcast.");
        return Ok(());
    }
    let identity = a.identity.as_ref().ok_or_else(|| anyhow!("--identity is required to sign"))?;
    if !a.yes {
        print!("\nSign and broadcast to {} on {}? Type yes: ", a.to, network.name());
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "yes" {
            println!("aborted; nothing broadcast.");
            return Ok(());
        }
    }
    drop(db);

    // 5. Sign and broadcast with devtool, from the same wallet state (no sync in between).
    println!("\nzcash-devtool wallet send --min-confirmations {n} …");
    let mut cmd = devtool(&devtool_bin, &a.wallet);
    cmd.arg("send")
        .arg("-i")
        .arg(identity)
        .args(["--address", &a.to, "--value", &a.value.to_string()])
        .args(["--min-confirmations", &n.to_string()])
        .args(["--target-note-count", &TARGET_NOTE_COUNT.to_string()])
        .args(["--min-split-output-value", &MIN_SPLIT_OUTPUT_VALUE.to_string()])
        .args(["-s", server]);
    if let Some(m) = &a.memo {
        cmd.args(["--memo", m]);
    }
    let mut child = cmd.stdout(Stdio::piped()).spawn().context("running zcash-devtool send")?;
    let mut txid = None;
    let mut devtool_anchor = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line?;
        println!("  | {line}");
        if let Some(h) = line.split("at anchor height ").nth(1) {
            devtool_anchor = h.trim_end_matches(':').parse::<u32>().ok();
        }
        let t = line.trim();
        if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            txid = Some(t.to_string());
        }
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("zcash-devtool send failed ({status})");
    }
    let txid = txid.ok_or_else(|| anyhow!("zcash-devtool send printed no txid"))?;
    if devtool_anchor != Some(h_a) {
        println!("WARNING: devtool reported anchor {devtool_anchor:?}, expected {h_a}");
    }

    // 6. Check the signed transaction itself: its anchor must be the Ironwood root at H_a.
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let key = tx::parse_txid(&txid)?.as_ref().to_vec();
    let raw: Vec<u8> = conn
        .query_row("SELECT raw FROM transactions WHERE txid = ?1", [&key], |r| r.get(0))
        .context("reading the sent transaction from the wallet")?;
    drop(conn);
    let s = tx::summarize(network, &raw, None, target, None)?;
    let tx_anchor = s.ironwood.as_ref().map(|i| i.anchor).ok_or_else(|| anyhow!("sent tx has no Ironwood bundle"))?;
    let mut db = WalletDb::for_path(&db_path, network.params(), SystemClock, UnwrapErr(SysRng))?;
    let root_at_ha = db
        .with_ironwood_tree_mut::<_, _, anyhow::Error>(|t| {
            Ok(t.root_at_checkpoint_id(&BlockHeight::from_u32(h_a))?.map(|r| r.to_bytes()))
        })?
        .flatten();

    println!("\ntxid:          {txid}");
    println!("tx anchor:     {}", hex::encode(tx_anchor));
    match root_at_ha {
        Some(r) if r == tx_anchor => println!("check:         equals the wallet's Ironwood root at {h_a} ✓"),
        Some(r) => println!("check:         MISMATCH: root at {h_a} is {}", hex::encode(r)),
        None => println!("check:         could not read the root at {h_a}"),
    }
    let net = if network == VintageNetwork::Main { String::new() } else { format!(" --network {}", network.name()) };
    println!("\nOnce mined, anyone can check it:");
    println!("  vintage{net} verify {txid} --before {}", a.before_arg);
    Ok(())
}

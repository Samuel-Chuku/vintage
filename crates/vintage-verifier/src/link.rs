//! The receipt-account link: fresh coins, one hop.
//!
//! Statement: every Ironwood coin spent by deposit `D` was created by payments `T₁…Tₙ`, and each `Tᵢ`
//! passes the age check against `H`.
//!
//! The holder discloses a single-use receipt account's viewing key (UFVK). The verifier
//! trial-decrypts each `Tᵢ`'s Ironwood outputs with it (external and internal scope), recomputes
//! each recovered note's nullifier, and requires **every** nullifier in `D` to be one of them. Wallets
//! pad transactions with dummy spends whose keys are thrown away, so a deposit that spends a single
//! note always carries an unmatched nullifier and FAILs: the deposit must spend at least two receipt
//! notes and have no more shielded outputs than inputs.
//!
//! Nothing here is stored. The bundle (and its viewing key) lives in memory for one call.

use std::time::Instant;

use orchard::keys::{FullViewingKey, PreparedIncomingViewingKey, Scope};
use serde::{Deserialize, Serialize};
use vintage_core::{
    VintageNetwork,
    tx::{self, TxError},
};
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_note_encryption::try_note_decryption;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    memo::{Memo, MemoBytes},
};

use crate::{
    lwd,
    report::{self, Before, VerifyError},
    roots,
};

/// What the holder discloses, stated verbatim wherever the link is offered.
pub const DISCLOSED: &str = "The receipt account's viewing key, therefore the funding payments, the amounts \
     received, the receipt address, and the deposit. Nothing from the user's other accounts. The viewing key \
     cannot spend. Rotate to a fresh receipt account for every use.";

/// Most funding payments one bundle may name.
pub const MAX_FUNDING: usize = 16;

/// The link bundle `vintage link prove` writes and `vintage link verify` / `POST /verify-link` read.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkBundle {
    pub version: u32,
    pub network: String,
    /// The receipt account's unified full viewing key.
    pub ufvk: String,
    /// The deposit `D` (txid).
    pub deposit: String,
    /// The funding payments `T₁…Tₙ` and which of their Ironwood outputs `D` spends.
    pub funding: Vec<FundingRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FundingRef {
    pub txid: String,
    pub outputs: Vec<u32>,
}

/// An Ironwood output of a transaction that the viewing key decrypts.
#[derive(Clone, Debug)]
pub struct Received {
    pub action: usize,
    pub value: u64,
    pub scope: &'static str,
    pub nullifier: [u8; 32],
    pub memo: Option<String>,
}

/// A fetched, parsed transaction.
pub struct Fetched {
    pub raw: Vec<u8>,
    pub tx: Transaction,
    pub mined: Option<u32>,
    pub tip: u32,
}

pub fn decode_ufvk(network: VintageNetwork, s: &str) -> Result<FullViewingKey, VerifyError> {
    let ufvk = UnifiedFullViewingKey::decode(&network.params(), s.trim())
        .map_err(|e| VerifyError::BadRequest(format!("not a {} unified full viewing key: {e}", network.name())))?;
    ufvk.orchard()
        .cloned()
        .ok_or_else(|| VerifyError::BadRequest("the viewing key has no Orchard/Ironwood component".into()))
}

/// Ironwood outputs of `tx` that `fvk` decrypts, with each note's recomputed nullifier.
pub fn received_by(tx: &Transaction, fvk: &FullViewingKey) -> Vec<Received> {
    let Some(bundle) = tx.ironwood_bundle() else { return vec![] };
    let mut out = vec![];
    for (scope, name) in [(Scope::External, "external"), (Scope::Internal, "internal")] {
        let ivk = PreparedIncomingViewingKey::new(&fvk.to_ivk(scope));
        for (i, action) in bundle.actions().iter().enumerate() {
            let domain = orchard::note_encryption::IronwoodDomain::for_action(action);
            if let Some((note, _addr, memo)) = try_note_decryption(&domain, &ivk, action) {
                let memo = MemoBytes::from_bytes(&memo).ok().and_then(|m| match Memo::try_from(&m) {
                    Ok(Memo::Text(t)) => Some(t.to_string()),
                    _ => None,
                });
                out.push(Received {
                    action: i,
                    value: note.value().inner(),
                    scope: name,
                    nullifier: note.nullifier(fvk).to_bytes(),
                    memo,
                });
            }
        }
    }
    out.sort_by_key(|r| r.action);
    out
}

/// The Ironwood nullifiers a transaction reveals (real and dummy spends alike).
pub fn nullifiers(tx: &Transaction) -> Vec<[u8; 32]> {
    tx.ironwood_bundle().map_or(vec![], |b| b.actions().iter().map(|a| a.nullifier().to_bytes()).collect())
}

pub async fn fetch(client: &lwd::Client, network: VintageNetwork, txid: &str) -> Result<Fetched, VerifyError> {
    let parsed = tx::parse_txid(txid).map_err(|e| VerifyError::BadRequest(format!("{txid}: {e}")))?;
    let (raw, tip) = tokio::try_join!(lwd::get_tx(client, &parsed), lwd::tip(client))
        .map_err(|e| if report::is_not_found(&e) { VerifyError::NotFound } else { e.into() })?;
    let mined = u32::try_from(raw.height).ok().filter(|h| *h > 0);
    let branch = BranchId::for_height(&network.params(), BlockHeight::from_u32(mined.unwrap_or(tip + 1)));
    let tx = Transaction::read(&raw.data[..], branch).map_err(|e| VerifyError::Upstream(TxError::from(e).into()))?;
    if tx.txid() != parsed {
        return Err(VerifyError::Upstream(TxError::TxidMismatch(tx.txid().to_string()).into()));
    }
    Ok(Fetched { raw: raw.data, tx, mined, tip })
}

#[derive(Clone, Debug, Serialize)]
pub struct ReceivedOut {
    pub output: usize,
    pub value_zat: u64,
    pub scope: &'static str,
    pub spent_by_deposit: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FundingResult {
    pub txid: String,
    pub verdict: &'static str,
    pub summary: String,
    pub mined_height: Option<u32>,
    pub anchor: Option<String>,
    pub received: Vec<ReceivedOut>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InputResult {
    /// Index of the Ironwood action in the deposit.
    pub action: usize,
    /// The funding payment and output this input spends, or null if none of them.
    pub funded_by: Option<FundedBy>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FundedBy {
    pub txid: String,
    pub output: usize,
    pub value_zat: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DepositResult {
    pub txid: String,
    pub mined_height: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkReport {
    pub network: &'static str,
    pub before: Before,
    /// `PASS`, `FAIL` or `NOT_PROVABLE`.
    pub verdict: &'static str,
    pub summary: String,
    /// True while the deposit or a funding payment is unmined (see the age check's `provisional`).
    pub provisional: bool,
    pub deposit: DepositResult,
    pub funding: Vec<FundingResult>,
    pub inputs: Vec<InputResult>,
    /// Total value of the deposit's inputs that the funding payments account for.
    pub covered_value_zat: u64,
    pub disclosed: &'static str,
    pub elapsed_secs: f64,
}

/// Check a link bundle against `before`. The bundle is only read; nothing is stored.
pub async fn verify_link(
    client: &lwd::Client,
    network: VintageNetwork,
    cache: Option<&roots::Cache>,
    bundle: &LinkBundle,
    before: Before,
    concurrency: usize,
) -> Result<LinkReport, VerifyError> {
    let t0 = Instant::now();
    if bundle.version != 1 {
        return Err(VerifyError::BadRequest(format!("unsupported bundle version {}", bundle.version)));
    }
    if bundle.network != network.name() {
        return Err(VerifyError::BadRequest(format!("bundle is for {}, this verifier is {}", bundle.network, network.name())));
    }
    if bundle.funding.is_empty() || bundle.funding.len() > MAX_FUNDING {
        return Err(VerifyError::BadRequest(format!("name 1 to {MAX_FUNDING} funding payments")));
    }
    let fvk = decode_ufvk(network, &bundle.ufvk)?;

    let d = fetch(client, network, &bundle.deposit).await.map_err(|e| match e {
        VerifyError::NotFound => VerifyError::BadRequest(format!("deposit {} not found", bundle.deposit)),
        e => e,
    })?;
    let ds = tx::summarize(network, &d.raw, d.mined, d.mined.unwrap_or(d.tip + 1), None)
        .map_err(|e| VerifyError::Upstream(e.into()))?;
    let mut report = LinkReport {
        network: network.name(),
        before: before.clone(),
        verdict: "NOT_PROVABLE",
        summary: String::new(),
        provisional: d.mined.is_none(),
        deposit: DepositResult { txid: ds.txid.clone(), mined_height: d.mined, reasons: ds.out_of_scope() },
        funding: vec![],
        inputs: vec![],
        covered_value_zat: 0,
        disclosed: DISCLOSED,
        elapsed_secs: 0.0,
    };
    let done = |mut r: LinkReport| {
        if r.provisional {
            r.summary = format!("Provisional, not everything is mined yet: {} Check again once it is.", r.summary);
        }
        r.elapsed_secs = (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
        Ok(r)
    };
    if !report.deposit.reasons.is_empty() {
        report.summary = format!(
            "Can't link this deposit: it has {}. The link only covers Ironwood inputs.",
            report.deposit.reasons.join(", ")
        );
        return done(report);
    }

    // Each funding payment: the age check, then the outputs the key decrypts.
    let deposit_nfs = nullifiers(&d.tx);
    let mut pool: Vec<(usize, usize, Received)> = vec![]; // (funding index, output, note)
    let mut all_pass = true;
    for (fi, f) in bundle.funding.iter().enumerate() {
        if f.txid.eq_ignore_ascii_case(&bundle.deposit) {
            return Err(VerifyError::BadRequest("a funding payment can't be the deposit itself".into()));
        }
        let age = report::verify(client, network, cache, &f.txid, before.clone(), concurrency).await.map_err(|e| match e {
            VerifyError::NotFound => VerifyError::BadRequest(format!("funding payment {} not found", f.txid)),
            e => e,
        })?;
        let ft = fetch(client, network, &f.txid).await?;
        let received = received_by(&ft.tx, &fvk);
        all_pass &= age.verdict == "PASS";
        report.provisional |= age.provisional;
        report.funding.push(FundingResult {
            txid: age.txid.clone(),
            verdict: age.verdict,
            summary: age.summary.clone(),
            mined_height: age.mined_height,
            anchor: age.anchor.clone(),
            received: received
                .iter()
                .map(|r| ReceivedOut {
                    output: r.action,
                    value_zat: r.value,
                    scope: r.scope,
                    spent_by_deposit: deposit_nfs.contains(&r.nullifier),
                })
                .collect(),
        });
        pool.extend(received.into_iter().map(|r| (fi, r.action, r)));
    }

    // Every input of the deposit must be one of those notes.
    let mut unmatched = 0;
    for (action, nf) in deposit_nfs.iter().enumerate() {
        let funded_by = pool.iter().find(|(_, _, r)| r.nullifier == *nf).map(|(fi, out, r)| FundedBy {
            txid: report.funding[*fi].txid.clone(),
            output: *out,
            value_zat: r.value,
        });
        if let Some(f) = &funded_by {
            report.covered_value_zat += f.value_zat;
        } else {
            unmatched += 1;
        }
        report.inputs.push(InputResult { action, funded_by });
    }

    let (h, label) = (before.height, &before.label);
    let n = bundle.funding.len();
    (report.verdict, report.summary) = if !all_pass {
        let failing: Vec<&str> =
            report.funding.iter().filter(|f| f.verdict != "PASS").map(|f| &f.txid[..16]).collect();
        ("FAIL", format!("Can't link: funding payment(s) {}… don't pass the age check against block {h} ({label}).", failing.join("…, ")))
    } else if unmatched > 0 {
        (
            "FAIL",
            format!(
                "Can't link: {unmatched} of the deposit's {} inputs spend coins not covered by the disclosed payments. \
                 (A wallet's dummy input counts too: the deposit must spend at least two receipt notes.)",
                deposit_nfs.len()
            ),
        )
    } else {
        (
            "PASS",
            format!(
                "Every coin this deposit spends came from {n} payment{} whose coins existed before block {h} ({label}).",
                if n == 1 { "" } else { "s" }
            ),
        )
    };
    done(report)
}

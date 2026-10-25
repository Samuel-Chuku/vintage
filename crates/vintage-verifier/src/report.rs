//! One verification, start to finish: fetch the transaction and the tip, check scope, get the roots
//! for `[H, tip]`, decide, and explain. The CLI and the API both print this report.
//!
//! Every report also carries the coins' **age**: the first block whose Ironwood root equals the anchor.
//! The tree is append-only, so the anchor names one tree size, and every note the transaction spends
//! sits below that size: all of them were in the pool by the end of that block. [`date`] gives the age
//! alone, with no event.

use std::time::Instant;

use serde::Serialize;
use zcash_protocol::consensus::{NetworkUpgrade, Parameters};
use vintage_core::{
    VintageNetwork,
    tx::{self, TxError},
    verify::{self, Verdict},
};

use crate::{lwd, roots};

/// "block 3,525,709 (label)" for sentences, or just "block 3,525,709" when the label is the height.
pub fn before_phrase(h: u32, label: &str) -> String {
    let block = format!("block {}", vintage_core::commas(h.into()));
    if label == block { block } else { format!("{block} ({label})") }
}

#[derive(Clone, Debug, Serialize)]
pub struct Before {
    pub height: u32,
    pub label: String,
    /// What the caller asked for: a height or an event id.
    pub requested: String,
    /// The registry event's type (`hack`, `tax_cutoff`, `bug_window`, `snapshot`, `custom`), when
    /// `requested` named an event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<vintage_core::registry::EventType>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Evidence {
    pub blocks_covered: u32,
    pub distinct_roots: usize,
    pub blocks_downloaded: u32,
    pub blocks_from_cache: u32,
    pub server_tree_checks: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub txid: String,
    pub network: &'static str,
    pub mined_height: Option<u32>,
    /// True while the transaction is unmined. In v6 the anchor is authorizing data, not part of the
    /// txid (ZIP 229), so the key holder could get a version with a different anchor mined under the
    /// same txid. Only a verdict on a mined transaction is final.
    pub provisional: bool,
    pub chain_tip: u32,
    pub before: Before,
    /// `PASS`, `FAIL` or `NOT_PROVABLE`.
    pub verdict: &'static str,
    /// One plain-language sentence for people.
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// The heights at or after H whose Ironwood root equals the anchor: for FAIL, and for a PASS whose
    /// anchor is the pool at H − 1 left unchanged by block H.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_heights: Option<[u32; 2]>,
    /// How old the coins are, independent of H.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<Age>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    pub elapsed_secs: f64,
}

/// The coins' proven age: every coin the transaction spends was in the pool by the end of block
/// `existed_by_height`.
#[derive(Clone, Debug, Serialize)]
pub struct Age {
    pub existed_by_height: u32,
    /// That block's time, unix seconds.
    pub existed_by_time: u32,
    /// True: the anchor is that block's root, and no earlier block's. False: the anchor is older than
    /// the search reached, so the coins existed *at least* by this block.
    pub exact: bool,
}

/// How far below the transaction [`age`] looks for its anchor before settling for a bound.
const AGE_SEARCH_BLOCKS: u32 = 50_000;

/// The proven age of a transaction with this anchor, mined at `mined` (or unmined, below the tip).
#[allow(clippy::too_many_arguments)]
pub async fn age(
    client: &lwd::Client,
    network: VintageNetwork,
    cache: Option<&roots::Cache>,
    anchor: &[u8; 32],
    below: u32,
    tip: u32,
    tip_hash: &[u8],
    concurrency: usize,
) -> anyhow::Result<Age> {
    let activation = network.params().activation_height(NetworkUpgrade::Nu6_3).map_or(1, u32::from);
    let floor = below.saturating_sub(AGE_SEARCH_BLOCKS).max(activation + 1);
    let (height, exact) = match roots::locate(client, cache, anchor, below, floor, tip, tip_hash, concurrency).await? {
        roots::Located::At { first, .. } => (first, true),
        roots::Located::Before { floor } => (floor - 1, false),
    };
    Ok(Age { existed_by_height: height, existed_by_time: lwd::block_time(client, height).await?, exact })
}

/// A transaction's age alone (`GET /age/{txid}`, `vintage age`): no event, no verdict.
#[derive(Clone, Debug, Serialize)]
pub struct Dated {
    pub txid: String,
    pub network: &'static str,
    pub mined_height: Option<u32>,
    pub provisional: bool,
    pub chain_tip: u32,
    /// `DATED` or `NOT_PROVABLE`.
    pub verdict: &'static str,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<Age>,
    pub elapsed_secs: f64,
}

pub async fn date(
    client: &lwd::Client,
    network: VintageNetwork,
    cache: Option<&roots::Cache>,
    txid: &str,
    concurrency: usize,
) -> Result<Dated, VerifyError> {
    let t0 = Instant::now();
    let parsed = tx::parse_txid(txid).map_err(|e| VerifyError::BadRequest(e.to_string()))?;
    let (raw, (tip, tip_hash)) = tokio::try_join!(lwd::get_tx(client, &parsed), lwd::tip_id(client))
        .map_err(|e| if is_not_found(&e) { VerifyError::NotFound } else { e.into() })?;
    let mined = u32::try_from(raw.height).ok().filter(|h| *h > 0);
    let s = tx::summarize(network, &raw.data, mined, mined.unwrap_or(tip + 1), Some(&parsed)).map_err(|e| match e {
        TxError::BadTxid => VerifyError::BadRequest(e.to_string()),
        e => VerifyError::Upstream(e.into()),
    })?;
    let mut d = Dated {
        txid: s.txid.clone(),
        network: network.name(),
        mined_height: s.mined_height,
        provisional: s.mined_height.is_none(),
        chain_tip: tip,
        verdict: "NOT_PROVABLE",
        summary: String::new(),
        reasons: s.out_of_scope(),
        anchor: None,
        age: None,
        elapsed_secs: 0.0,
    };
    if d.reasons.is_empty() {
        let anchor = s.ironwood.as_ref().expect("in scope implies an Ironwood bundle").anchor;
        d.anchor = Some(hex::encode(anchor));
        let a = age(client, network, cache, &anchor, mined.unwrap_or(tip + 1), tip, &tip_hash, concurrency).await?;
        d.verdict = "DATED";
        d.summary = format!(
            "Every coin this transaction spends was in the shielded pool {} block {}.",
            if a.exact { "by" } else { "at least by" },
            vintage_core::commas(a.existed_by_height.into())
        );
        d.age = Some(a);
    } else {
        d.summary = format!(
            "Can't date this transaction: it has {}. Vintage only covers Ironwood inputs.",
            d.reasons.join(", ")
        );
    }
    if d.provisional {
        d.summary = format!("Provisional, not mined yet: {} Check again once it is mined.", d.summary);
    }
    d.elapsed_secs = (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
    Ok(d)
}

/// Why a verification couldn't produce a verdict.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("{0}")]
    BadRequest(String),
    #[error("transaction not found")]
    NotFound,
    #[error(transparent)]
    Upstream(#[from] anyhow::Error),
}

pub(crate) fn is_not_found(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<tonic::Status>())
        .any(|s| s.code() == tonic::Code::NotFound)
}

pub async fn verify(
    client: &lwd::Client,
    network: VintageNetwork,
    cache: Option<&roots::Cache>,
    txid: &str,
    before: Before,
    concurrency: usize,
) -> Result<Report, VerifyError> {
    let t0 = Instant::now();
    let parsed = tx::parse_txid(txid).map_err(|e| VerifyError::BadRequest(e.to_string()))?;
    let (raw, (tip, tip_hash)) = tokio::try_join!(lwd::get_tx(client, &parsed), lwd::tip_id(client))
        .map_err(|e| if is_not_found(&e) { VerifyError::NotFound } else { e.into() })?;
    // RawTransaction.height: 0 = mempool, u64::MAX (-1) = not mined (per the proto docs).
    let mined = u32::try_from(raw.height).ok().filter(|h| *h > 0);
    let s = tx::summarize(network, &raw.data, mined, mined.unwrap_or(tip + 1), Some(&parsed)).map_err(|e| match e {
        TxError::BadTxid => VerifyError::BadRequest(e.to_string()),
        e => VerifyError::Upstream(e.into()),
    })?;
    let h = before.height;
    let label = before.label.clone();
    let mut report = Report {
        txid: s.txid.clone(),
        network: network.name(),
        mined_height: s.mined_height,
        provisional: s.mined_height.is_none(),
        chain_tip: tip,
        before,
        verdict: "NOT_PROVABLE",
        summary: String::new(),
        reasons: s.out_of_scope(),
        anchor: None,
        matched_heights: None,
        age: None,
        evidence: None,
        elapsed_secs: 0.0,
    };
    let done = |mut r: Report| {
        if r.provisional {
            r.summary = format!(
                "Provisional, not mined yet: {} Check again once it is mined; until then the same txid \
                 could be mined with a different anchor.",
                r.summary
            );
        }
        r.elapsed_secs = (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
        Ok(r)
    };
    if !report.reasons.is_empty() {
        report.summary = format!(
            "Can't prove anything about this transaction: it has {}. Vintage only covers Ironwood inputs.",
            report.reasons.join(", ")
        );
        return done(report);
    }
    let anchor = s.ironwood.as_ref().expect("in scope implies an Ironwood bundle").anchor;
    report.anchor = Some(hex::encode(anchor));
    let activation = network
        .params()
        .activation_height(NetworkUpgrade::Nu6_3)
        .map_or(0, u32::from);
    if h <= activation || h > tip {
        return Err(VerifyError::BadRequest(format!(
            "H = {} must be after Ironwood's activation (block {}) and at most the chain tip {}",
            vintage_core::commas(h.into()),
            vintage_core::commas(activation.into()),
            vintage_core::commas(tip.into())
        )));
    }

    let r = roots::roots_for(client, cache, h, tip, &tip_hash, concurrency).await?;
    report.evidence = Some(Evidence {
        blocks_covered: r.stats.blocks,
        distinct_roots: r.runs.len(),
        blocks_downloaded: r.stats.blocks_downloaded,
        blocks_from_cache: r.stats.blocks_from_cache,
        server_tree_checks: r.stats.server_tree_checks,
    });
    match verify::decide_runs(&anchor, &r.runs) {
        Verdict::Pass { .. } => {
            report.verdict = "PASS";
            report.summary = format!("These coins existed before {}.", before_phrase(h, &label));
            // The anchor is from before H; find where.
            report.age = Some(age(client, network, cache, &anchor, h, tip, &tip_hash, concurrency).await?);
        }
        // The anchor is H's root, and H's root was already the root at H − 1 (block H added no
        // Ironwood notes): the tree it names is the one at H − 1, so every spent note existed before
        // H. Consensus proves this as surely as any other PASS.
        Verdict::Fail { matched_from, matched_to } if verify::unchanged_before(h, matched_from, r.size_before, &r.runs[0].0) => {
            report.verdict = "PASS";
            report.matched_heights = Some([matched_from, matched_to]);
            let a = age(client, network, cache, &anchor, h, tip, &tip_hash, concurrency).await?;
            report.summary = format!(
                "These coins existed before {}: its anchor is the pool as of block {}, which block {} left unchanged.",
                before_phrase(h, &label),
                vintage_core::commas(a.existed_by_height.into()),
                vintage_core::commas(h.into())
            );
            report.age = Some(a);
        }
        Verdict::Fail { matched_from, matched_to } => {
            report.verdict = "FAIL";
            report.matched_heights = Some([matched_from, matched_to]);
            report.summary = format!("Can't prove these coins existed before {}.", before_phrase(h, &label));
            // The anchor's run starts at `matched_from` (block `matched_from − 1` had another root).
            report.age = Some(Age {
                existed_by_height: matched_from,
                existed_by_time: lwd::block_time(client, matched_from).await?,
                exact: true,
            });
        }
        Verdict::NotProvable { .. } => unreachable!("scope was checked above"),
    }
    done(report)
}

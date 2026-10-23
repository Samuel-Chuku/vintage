//! One verification, start to finish: fetch the transaction and the tip, check scope, get the roots
//! for `[H, tip]`, decide, and explain. The CLI and the API both print this report.

use std::time::Instant;

use serde::Serialize;
use zcash_protocol::consensus::{NetworkUpgrade, Parameters};
use vintage_core::{
    VintageNetwork,
    tx::{self, TxError},
    verify::{self, Verdict},
};

use crate::{lwd, roots};

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
    /// For FAIL: the heights whose Ironwood root equals the anchor (all at or after H).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_heights: Option<[u32; 2]>,
    /// For FAIL: the matching root already existed at H − 1 (no notes added in between), so the
    /// inputs may still predate H; an earlier anchor would prove it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_existed_before_h: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    pub elapsed_secs: f64,
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
        root_existed_before_h: None,
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
            "H = {h} must be after Ironwood's activation (block {activation}) and at most the chain tip {tip}"
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
            report.summary = format!("These coins existed before block {h} ({label}).");
        }
        Verdict::Fail { matched_from, matched_to } => {
            let existed = matched_from == h && r.size_before == r.runs[0].0.size;
            report.verdict = "FAIL";
            report.matched_heights = Some([matched_from, matched_to]);
            report.root_existed_before_h = Some(existed);
            report.summary = if existed {
                format!(
                    "Can't prove these coins existed before block {h} ({label}). Its anchor is the pool \
                     state at block {h}, which no new notes had changed since block {}, so the coins may \
                     still predate it; a spend anchored earlier would prove it.",
                    h - 1
                )
            } else {
                format!("Can't prove these coins existed before block {h} ({label}).")
            };
        }
        Verdict::NotProvable { .. } => unreachable!("scope was checked above"),
    }
    done(report)
}

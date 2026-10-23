//! Vintage verifier: the chain-facing half of Vintage, shared by the `vintage` CLI and `vintage-api`.
//!
//! - [`lwd`]: lightwalletd / Zaino gRPC access.
//! - [`roots`]: Ironwood roots for a range of blocks, computed from compact blocks and cached.
//! - [`report`]: one verification of a transaction against a height, as a serializable report.
//! - [`link`]: the receipt-account link (fresh coins, one hop), checked from a disclosed viewing key.
//!
//! The decision logic itself lives in `vintage-core` (no I/O).

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use vintage_core::{
    VintageNetwork,
    registry::{self, Registry},
};

pub mod link;
pub mod lwd;
pub mod report;
pub mod roots;

/// The registry file in `dir`: `events.json`, or the version-1 name `incidents.json`.
pub fn registry_file(dir: &Path) -> std::path::PathBuf {
    let v2 = dir.join("events.json");
    if v2.exists() { v2 } else { dir.join("incidents.json") }
}

/// Load the event registry from `dir` and check its signature (`<file>.sig`) against `pubkey.hex`.
pub fn load_registry(dir: &Path) -> Result<Registry> {
    let file = registry_file(dir);
    let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
    let sig_file = file.with_extension("json.sig");
    let sig = std::fs::read_to_string(&sig_file).with_context(|| format!("reading {}", sig_file.display()))?;
    let pk = std::fs::read_to_string(dir.join("pubkey.hex")).context("reading pubkey.hex")?;
    Ok(registry::load_verified(&bytes, &sig, &pk)?)
}

/// Resolve a `before` argument: a block height, or an event id (looked up in the registry that
/// `registry` loads, only when needed).
pub fn resolve_before(
    registry: impl FnOnce() -> Result<Registry>,
    network: VintageNetwork,
    before: &str,
) -> Result<report::Before> {
    if let Ok(h) = before.parse::<u32>() {
        return Ok(report::Before { height: h, label: format!("block {h}"), requested: before.into(), event_type: None });
    }
    let registry = registry()?;
    let e = registry.find(before).ok_or_else(|| anyhow!("no event `{before}` in the registry"))?;
    if e.network != network.name() {
        bail!("event `{before}` is on {}, not {}", e.network, network.name());
    }
    Ok(report::Before { height: e.height, label: e.label.clone(), requested: before.into(), event_type: Some(e.kind) })
}

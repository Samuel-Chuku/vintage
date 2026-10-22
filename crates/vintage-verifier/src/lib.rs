//! Vintage verifier: the chain-facing half of Vintage, shared by the `vintage` CLI and `vintage-api`.
//!
//! - [`lwd`]: lightwalletd / Zaino gRPC access.
//! - [`roots`]: Ironwood roots for a range of blocks, computed from compact blocks and cached.
//! - [`report`]: one verification of a transaction against a height, as a serializable report.
//!
//! The decision logic itself lives in `vintage-core` (no I/O).

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use vintage_core::{
    VintageNetwork,
    registry::{self, Registry},
};

pub mod lwd;
pub mod report;
pub mod roots;

/// Load `incidents.json` from `dir` and check its signature against `pubkey.hex`.
pub fn load_registry(dir: &Path) -> Result<Registry> {
    let bytes = std::fs::read(dir.join("incidents.json")).context("reading incidents.json")?;
    let sig = std::fs::read_to_string(dir.join("incidents.json.sig")).context("reading incidents.json.sig")?;
    let pk = std::fs::read_to_string(dir.join("pubkey.hex")).context("reading pubkey.hex")?;
    Ok(registry::load_verified(&bytes, &sig, &pk)?)
}

/// Resolve a `before` argument: a block height, or an incident id (looked up in the registry that
/// `registry` loads, only when needed).
pub fn resolve_before(
    registry: impl FnOnce() -> Result<Registry>,
    network: VintageNetwork,
    before: &str,
) -> Result<report::Before> {
    if let Ok(h) = before.parse::<u32>() {
        return Ok(report::Before { height: h, label: format!("block {h}"), requested: before.into() });
    }
    let registry = registry()?;
    let inc = registry.find(before).ok_or_else(|| anyhow!("no incident `{before}` in the registry"))?;
    if inc.network != network.name() {
        bail!("incident `{before}` is on {}, not {}", inc.network, network.name());
    }
    Ok(report::Before { height: inc.first_inflow_height, label: inc.label.clone(), requested: before.into() })
}

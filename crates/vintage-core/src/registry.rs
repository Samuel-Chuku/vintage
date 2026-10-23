//! The event registry: a signed list of labelled block heights.
//!
//! An event is any moment worth proving coins predate: a hack's first inflow, a tax cutoff, a bug
//! window, a snapshot. The registry only labels heights. It names no people and no addresses, and
//! anyone can verify against any height of their own choosing, so it is not a blocklist.
//!
//! Format version 2 (`events.json`). Version 1 files (`incidents`, `first_inflow_height`, no `type`)
//! still parse: the old names are serde aliases and the type defaults to `custom`.

use ed25519_zebra::{Signature, VerificationKey};
use serde::{Deserialize, Serialize};

/// What kind of moment an event marks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Stolen funds' first inflow into the pool.
    Hack,
    /// A tax or accounting cutoff.
    TaxCutoff,
    /// The start of a window in which a bug could have created or moved coins.
    BugWindow,
    /// A balance snapshot (airdrops, governance, audits).
    Snapshot,
    #[default]
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: EventType,
    pub label: String,
    pub network: String,
    /// Proofs against this event show coins existed before this block.
    #[serde(alias = "first_inflow_height")]
    pub height: u32,
    pub txids: Vec<String>,
    pub sources: Vec<String>,
    pub added_at: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    #[serde(alias = "incidents")]
    pub events: Vec<Event>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("signature or key is not valid hex of the right length")]
    Encoding,
    #[error("signature does not verify against the registry public key")]
    BadSignature,
}

/// Verify `sig_hex` (64-byte ed25519 signature, hex) over the exact registry file bytes, then parse.
pub fn load_verified(bytes: &[u8], sig_hex: &str, pubkey_hex: &str) -> Result<Registry, RegistryError> {
    let sig: [u8; 64] = hex::decode(sig_hex.trim())
        .map_err(|_| RegistryError::Encoding)?
        .try_into()
        .map_err(|_| RegistryError::Encoding)?;
    let pk: [u8; 32] = hex::decode(pubkey_hex.trim())
        .map_err(|_| RegistryError::Encoding)?
        .try_into()
        .map_err(|_| RegistryError::Encoding)?;
    let vk = VerificationKey::try_from(pk).map_err(|_| RegistryError::Encoding)?;
    vk.verify(&Signature::from_bytes(&sig), bytes)
        .map_err(|_| RegistryError::BadSignature)?;
    Ok(serde_json::from_slice(bytes)?)
}

impl Registry {
    pub fn find(&self, id: &str) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_1_files_still_parse() {
        let v1 = r#"{"version":1,"incidents":[{"id":"x","label":"X","network":"main","first_inflow_height":5,
            "txids":[],"sources":[],"added_at":"2026-10-21"}]}"#;
        let r: Registry = serde_json::from_str(v1).unwrap();
        assert_eq!(r.events[0].height, 5);
        assert_eq!(r.events[0].kind, EventType::Custom);
    }

    #[test]
    fn version_2_types() {
        let v2 = r#"{"version":2,"events":[{"id":"x","type":"tax_cutoff","label":"X","network":"main","height":5,
            "txids":[],"sources":[],"added_at":"2026-10-23"}]}"#;
        let r: Registry = serde_json::from_str(v2).unwrap();
        assert_eq!(r.events[0].kind, EventType::TaxCutoff);
        assert!(serde_json::from_str::<Registry>(&v2.replace("tax_cutoff", "heist")).is_err());
    }
}

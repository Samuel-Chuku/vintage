//! The incident registry: a signed list of labelled block heights.
//!
//! It only labels heights. It names no people and no addresses, and anyone can verify against any
//! height of their own choosing, so it is not a blocklist.

use ed25519_zebra::{Signature, VerificationKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub label: String,
    pub network: String,
    pub first_inflow_height: u32,
    pub txids: Vec<String>,
    pub sources: Vec<String>,
    pub added_at: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub incidents: Vec<Incident>,
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
    pub fn find(&self, id: &str) -> Option<&Incident> {
        self.incidents.iter().find(|i| i.id == id)
    }
}

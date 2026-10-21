//! `vintage registry keygen | sign | check`

use std::{io::Read, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use clap::Subcommand;
use ed25519_zebra::{SigningKey, VerificationKey};

#[derive(Subcommand)]
pub enum RegistryCmd {
    /// Generate a new ed25519 signing key (hex) and write pubkey.hex into the registry dir.
    Keygen {
        /// Where to write the PRIVATE key (keep it out of git).
        #[arg(long, default_value = "secrets/registry-signing.key")]
        key: PathBuf,
    },
    /// Sign incidents.json with the private key, writing incidents.json.sig.
    Sign {
        #[arg(long, default_value = "secrets/registry-signing.key")]
        key: PathBuf,
    },
    /// Verify the signature and print the incidents.
    Check,
}

fn read_key(path: &Path) -> Result<SigningKey> {
    let hexkey = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let bytes: [u8; 32] = hex::decode(hexkey.trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub fn run(cmd: RegistryCmd, dir: &Path) -> Result<()> {
    match cmd {
        RegistryCmd::Keygen { key } => {
            if key.exists() {
                anyhow::bail!("{} already exists; refusing to overwrite", key.display());
            }
            let mut seed = [0u8; 32];
            std::fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
            let sk = SigningKey::from_bytes(&seed);
            let vk = VerificationKey::from(&sk);
            if let Some(parent) = key.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&key, hex::encode(seed))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))?;
            }
            let pk_hex = hex::encode(<[u8; 32]>::from(vk));
            std::fs::write(dir.join("pubkey.hex"), format!("{pk_hex}\n"))?;
            println!("private key: {} (mode 600, do not commit)", key.display());
            println!("public key:  {pk_hex} → {}", dir.join("pubkey.hex").display());
            Ok(())
        }
        RegistryCmd::Sign { key } => {
            let sk = read_key(&key)?;
            let bytes = std::fs::read(dir.join("incidents.json"))?;
            // Refuse to sign something the verifier couldn't parse.
            let _: vintage_core::registry::Registry = serde_json::from_slice(&bytes)?;
            let sig = sk.sign(&bytes);
            std::fs::write(dir.join("incidents.json.sig"), format!("{}\n", hex::encode(sig.to_bytes())))?;
            println!("signed {} bytes → {}", bytes.len(), dir.join("incidents.json.sig").display());
            Ok(())
        }
        RegistryCmd::Check => {
            let reg = super::load_registry(dir)?;
            println!("signature OK ({} incident(s))", reg.incidents.len());
            for i in &reg.incidents {
                println!("- {} [{}] first inflow height {}: {}", i.id, i.network, i.first_inflow_height, i.label);
            }
            Ok(())
        }
    }
}

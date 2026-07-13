//! Ed25519 host key: generated on first boot, persisted under the data dir.

use std::path::Path;

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};

use crate::ServerError;

pub const HOST_KEY_FILE: &str = "ssh_host_ed25519_key";

/// Load `<data_dir>/ssh_host_ed25519_key`, generating (and persisting,
/// mode 0600) a fresh ed25519 key on first boot.
pub fn load_or_generate(data_dir: &Path) -> Result<PrivateKey, ServerError> {
    let path = data_dir.join(HOST_KEY_FILE);
    if path.exists() {
        return Ok(PrivateKey::read_openssh_file(&path)?);
    }
    std::fs::create_dir_all(data_dir)?;
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?;
    key.write_openssh_file(&path, LineEnding::LF)?; // writes with mode 0600
    Ok(key)
}

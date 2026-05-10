//! File-backed UI session-token store.
//!
//! Tokens are printed once by `jam ui token`; only SHA-256 hashes are stored on
//! disk. This is Phase 0 auth plumbing for `comp-ui-session-token-auth`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Default UI token file relative to `JAM_HOME`.
pub const TOKEN_FILE_RELATIVE: &str = "ui/session-tokens.json";

/// A newly issued token. `token` is only returned once and is never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    /// Stable token identifier printed for revocation.
    pub id: String,
    /// Bearer token value shown once to the Manager.
    pub token: String,
    /// User attribution stored with the token.
    pub user_id: String,
}

/// Token record persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRecord {
    /// Stable token identifier.
    pub id: String,
    /// SHA-256 hash of the bearer token, hex-encoded.
    pub token_hash: String,
    /// User attribution for UI-initiated actions.
    pub user_id: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// File-backed token store.
#[derive(Debug, Clone)]
pub struct TokenStore {
    path: PathBuf,
}

/// Token-store errors.
#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    /// Filesystem operation failed.
    #[error("token store io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or parsing failed.
    #[error("token store json: {0}")]
    Json(#[from] serde_json::Error),
}

impl TokenStore {
    /// Create a store at an explicit path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Create a store under `$JAM_HOME/ui/session-tokens.json`.
    #[must_use]
    pub fn from_jam_home(jam_home: impl AsRef<Path>) -> Self {
        Self::new(jam_home.as_ref().join(TOKEN_FILE_RELATIVE))
    }

    /// Issue a token for `user_id`, persist its hash, and return the raw token.
    pub fn issue(&self, user_id: &str) -> Result<IssuedToken, TokenStoreError> {
        let token = random_token();
        let id = short_id(&token);
        let mut records = self.load()?;
        records.push(TokenRecord {
            id: id.clone(),
            token_hash: hash_token(&token),
            user_id: user_id.to_owned(),
            created_at: Utc::now(),
        });
        self.save(&records)?;
        Ok(IssuedToken {
            id,
            token,
            user_id: user_id.to_owned(),
        })
    }

    /// Return the matching token record when `token` is valid.
    pub fn verify(&self, token: &str) -> Result<Option<TokenRecord>, TokenStoreError> {
        let token_hash = hash_token(token);
        Ok(self
            .load()?
            .into_iter()
            .find(|record| record.token_hash == token_hash))
    }

    /// Revoke a token by id. Returns true if a token was removed.
    pub fn revoke(&self, id: &str) -> Result<bool, TokenStoreError> {
        let mut records = self.load()?;
        let old_len = records.len();
        records.retain(|record| record.id != id);
        let removed = records.len() != old_len;
        self.save(&records)?;
        Ok(removed)
    }

    /// Revoke every token and return the number removed.
    pub fn revoke_all(&self) -> Result<usize, TokenStoreError> {
        let count = self.load()?.len();
        self.save(&[])?;
        Ok(count)
    }

    fn load(&self) -> Result<Vec<TokenRecord>, TokenStoreError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&raw)?)
    }

    fn save(&self, records: &[TokenRecord]) -> Result<(), TokenStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = serde_json::to_vec_pretty(records)?;
        let mut file = open_token_file(&self.path)?;
        file.set_len(0)?;
        file.write_all(&data)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn short_id(token: &str) -> String {
    hash_token(token).chars().take(12).collect()
}

#[cfg(unix)]
fn open_token_file(path: &Path) -> Result<fs::File, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_token_file(path: &Path) -> Result<fs::File, std::io::Error> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_verifies_but_raw_value_is_not_persisted() {
        let tmp = tempfile::tempdir().unwrap();
        let store = TokenStore::from_jam_home(tmp.path());

        let issued = store.issue("human:caleb").unwrap();
        let record = store.verify(&issued.token).unwrap().unwrap();

        assert_eq!(record.id, issued.id);
        assert_eq!(record.user_id, "human:caleb");
        let persisted = fs::read_to_string(tmp.path().join(TOKEN_FILE_RELATIVE)).unwrap();
        assert!(!persisted.contains(&issued.token));
    }

    #[test]
    fn revoke_removes_one_token() {
        let tmp = tempfile::tempdir().unwrap();
        let store = TokenStore::from_jam_home(tmp.path());

        let first = store.issue("human:caleb").unwrap();
        let second = store.issue("human:caleb").unwrap();

        assert!(store.revoke(&first.id).unwrap());
        assert!(store.verify(&first.token).unwrap().is_none());
        assert!(store.verify(&second.token).unwrap().is_some());
    }
}

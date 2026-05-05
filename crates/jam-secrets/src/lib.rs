//! Secrets management for the Jamboree orchestrator.
//!
//! Per `dec-pass-and-gpg-for-secrets` and spec §11.3: `pass` (the standard
//! Unix password manager, encrypted on disk via GPG) is the primary backend.
//! [`FileBackend`] is a TOML fallback for environments where `pass` is
//! unavailable.
//!
//! ## Three layers of protection (§11.3.2)
//!
//! 1. **Storage** — `pass` (encrypted, GPG-based) or chmod-600 file.
//! 2. **In-memory** — [`SecretString`] re-exported from `secrecy` with
//!    zeroize-on-drop, redacted Debug/Display, no Serialize impl by default.
//! 3. **Logging discipline** — journal-writer redacts known secret regex
//!    patterns at write time. `bandit` (Python) and a custom clippy lint
//!    (Rust) catch direct format-string usage of [`SecretString`].
//!
//! ## Per-harness allowlist (§11.3.3)
//!
//! [`SecretBackend::get_for_harness`] enforces that we only pass the secrets
//! the harness actually needs — a Codex CLI Picker doesn't get the DeepSeek
//! key, a docs-summary Picker doesn't get the GitHub PAT.

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub use secrecy::{ExposeSecret, SecretString};

/// A secret key — a hierarchical kebab-case path like `"jam/pickers/github-app-key"`.
///
/// Custom Debug elides the key path itself; defense-in-depth against
/// accidental log leakage of the naming scheme.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretKey(String);

impl SecretKey {
    /// Construct a secret key.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Access the raw key path.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKey(<redacted>)")
    }
}

/// Failure modes for secret backends.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Requested key is not present in the backend.
    #[error("secret '{0}' not found")]
    NotFound(String),

    /// Backend-specific error (e.g. `pass` exited nonzero, TOML parse failure).
    #[error("backend error: {0}")]
    Backend(String),

    /// Underlying I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A source of secrets.
///
/// Implementations are [`PassBackend`] (default) and [`FileBackend`] (fallback).
/// Future implementations may include cloud secret managers; the trait is
/// transport-agnostic.
pub trait SecretBackend: Send + Sync {
    /// Fetch a secret by key.
    fn get(&self, key: &SecretKey) -> Result<SecretString, SecretError>;

    /// List all keys this backend knows about.
    ///
    /// Used by `jam doctor` to verify expected keys are present.
    fn list_keys(&self) -> Result<Vec<SecretKey>, SecretError>;

    /// Fetch the per-harness secret allowlist.
    ///
    /// Returns `(env_var_name, secret_value)` pairs the harness needs.
    /// Default implementation returns an empty list; backends may override
    /// to read project config and resolve a harness-specific allowlist.
    ///
    /// Per spec §11.3.3 and `dec-pass-and-gpg-for-secrets`.
    fn get_for_harness(
        &self,
        _harness_id: &str,
    ) -> Result<Vec<(String, SecretString)>, SecretError> {
        Ok(vec![])
    }
}

/// `pass`-backed secret store.
///
/// Invokes `pass show <prefix>/<key>` for each lookup. Caller is responsible
/// for ensuring `pass` is on PATH and the GPG agent is running.
pub struct PassBackend {
    prefix: String,
}

impl PassBackend {
    /// Construct with the conventional prefix (e.g. `"jam"`).
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    fn full_path(&self, key: &SecretKey) -> String {
        format!("{}/{}", self.prefix.trim_end_matches('/'), key.as_str())
    }
}

impl SecretBackend for PassBackend {
    fn get(&self, key: &SecretKey) -> Result<SecretString, SecretError> {
        let path = self.full_path(key);
        let output = Command::new("pass").arg("show").arg(&path).output()?;

        if !output.status.success() {
            // Treat any failure as not-found; pass returns 1 for missing keys
            // and we don't want to leak GPG errors to log scope.
            return Err(SecretError::NotFound(path));
        }

        let value = String::from_utf8(output.stdout)
            .map_err(|e| SecretError::Backend(format!("invalid utf-8 in secret: {e}")))?;
        // pass appends a newline to its output; strip exactly one trailing
        // newline if present (preserve any other trailing whitespace).
        let value = value.strip_suffix('\n').unwrap_or(&value).to_string();

        Ok(SecretString::from(value))
    }

    fn list_keys(&self) -> Result<Vec<SecretKey>, SecretError> {
        let output = Command::new("pass")
            .arg("ls")
            .arg(self.prefix.trim_end_matches('/'))
            .output()?;

        if !output.status.success() {
            // Empty store is fine.
            return Ok(vec![]);
        }

        // pass ls emits a tree-style listing. Parsing it out of scope for
        // Phase 0; for now return empty and let `jam doctor` invoke `pass ls`
        // directly when needed.
        Ok(vec![])
    }
}

/// File-backed secret store, reading TOML from `path`.
///
/// Format:
///
/// ```toml
/// [secrets]
/// "jam/pickers/github-app-key" = "..."
/// "jam/notify/ntfy-token" = "..."
/// ```
///
/// File should be mode 600. WSL gotcha: ensure the file is on the Linux
/// filesystem (verified by [`principle-native-fs-only`] / spec §6.6 Invariant 4).
pub struct FileBackend {
    path: PathBuf,
    cache: OnceLock<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct FileBackendDoc {
    #[serde(default)]
    secrets: std::collections::HashMap<String, String>,
}

impl FileBackend {
    /// Construct with the path to a TOML secrets file.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            cache: OnceLock::new(),
        }
    }

    fn load(&self) -> Result<&std::collections::HashMap<String, String>, SecretError> {
        if let Some(cached) = self.cache.get() {
            return Ok(cached);
        }

        let bytes = std::fs::read_to_string(&self.path)
            .map_err(|e| SecretError::Backend(format!("read {}: {e}", self.path.display())))?;

        let doc: FileBackendDoc = toml::from_str(&bytes)
            .map_err(|e| SecretError::Backend(format!("parse {}: {e}", self.path.display())))?;

        // OnceLock::set fails if already initialized; that's fine — race is
        // benign because both initializers produce equivalent maps.
        let _ = self.cache.set(doc.secrets);

        Ok(self.cache.get().expect("just set"))
    }
}

impl SecretBackend for FileBackend {
    fn get(&self, key: &SecretKey) -> Result<SecretString, SecretError> {
        let map = self.load()?;
        map.get(key.as_str())
            .map(|v| SecretString::from(v.clone()))
            .ok_or_else(|| SecretError::NotFound(key.as_str().to_string()))
    }

    fn list_keys(&self) -> Result<Vec<SecretKey>, SecretError> {
        let map = self.load()?;
        Ok(map.keys().cloned().map(SecretKey).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn secret_key_redacts_in_debug() {
        let key = SecretKey::new("jam/pickers/github-app-key");
        let dbg = format!("{key:?}");
        assert!(!dbg.contains("github"));
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn secret_key_round_trips_through_str() {
        let key = SecretKey::new("jam/notify/ntfy-token");
        assert_eq!(key.as_str(), "jam/notify/ntfy-token");
    }

    #[test]
    fn file_backend_reads_secrets() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"[secrets]
"jam/notify/ntfy-token" = "ntfy-test-token"
"jam/search/brave" = "brave-test-key"
"#
        )
        .unwrap();

        let backend = FileBackend::new(tmp.path());
        let token = backend
            .get(&SecretKey::new("jam/notify/ntfy-token"))
            .unwrap();
        assert_eq!(token.expose_secret(), "ntfy-test-token");

        let brave = backend.get(&SecretKey::new("jam/search/brave")).unwrap();
        assert_eq!(brave.expose_secret(), "brave-test-key");
    }

    #[test]
    fn file_backend_lists_keys() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"[secrets]
"jam/notify/ntfy-token" = "x"
"jam/search/brave" = "y"
"#
        )
        .unwrap();

        let backend = FileBackend::new(tmp.path());
        let mut keys: Vec<String> = backend
            .list_keys()
            .unwrap()
            .into_iter()
            .map(|k| k.as_str().to_string())
            .collect();
        keys.sort();
        assert_eq!(keys, vec!["jam/notify/ntfy-token", "jam/search/brave"]);
    }

    #[test]
    fn file_backend_missing_key_is_not_found() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "[secrets]\n").unwrap();

        let backend = FileBackend::new(tmp.path());
        let err = backend
            .get(&SecretKey::new("nonexistent"))
            .expect_err("should be NotFound");
        assert!(matches!(err, SecretError::NotFound(_)));
    }

    #[test]
    fn pass_backend_constructs_full_path() {
        let backend = PassBackend::new("jam");
        let path = backend.full_path(&SecretKey::new("notify/ntfy-token"));
        assert_eq!(path, "jam/notify/ntfy-token");

        // Trailing slash on prefix is tolerated.
        let backend = PassBackend::new("jam/");
        let path = backend.full_path(&SecretKey::new("notify/ntfy-token"));
        assert_eq!(path, "jam/notify/ntfy-token");
    }

    #[test]
    fn default_get_for_harness_returns_empty() {
        // The default impl returns empty; concrete backends may override.
        let backend = PassBackend::new("jam");
        let secrets = backend.get_for_harness("codex-cli").unwrap();
        assert!(secrets.is_empty());
    }
}

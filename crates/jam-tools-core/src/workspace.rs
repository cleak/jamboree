//! Workspace key sanitization for path-safe worktree identifiers.
//!
//! Per §6.6 Invariant 3, raw workspace/task identifiers must cross a smart
//! constructor before they are used as path segments.

use std::fmt;

/// A path-safe workspace key.
///
/// Any character outside `[A-Za-z0-9._-]` is replaced with `_`. The newtype is
/// intentionally small so services can require it at path-construction
/// boundaries instead of accepting arbitrary strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceKey(String);

impl WorkspaceKey {
    /// Sanitize a raw workspace identifier into a path-safe key.
    #[must_use]
    pub fn new(raw: impl AsRef<str>) -> Self {
        let mut sanitized = String::with_capacity(raw.as_ref().len());
        for byte in raw.as_ref().bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
                sanitized.push(byte as char);
            } else {
                sanitized.push('_');
            }
        }
        if sanitized.is_empty() {
            sanitized.push('_');
        }
        Self(sanitized)
    }

    /// Borrow the sanitized key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the key and return the sanitized string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for WorkspaceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for WorkspaceKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceKey;

    #[test]
    fn preserves_safe_ascii_key_chars() {
        assert_eq!(
            WorkspaceKey::new("task-1_safe.name").as_str(),
            "task-1_safe.name"
        );
    }

    #[test]
    fn replaces_unsafe_chars_with_underscores() {
        assert_eq!(
            WorkspaceKey::new("blueberry task/../../$(oops)").as_str(),
            "blueberry_task_.._..___oops_"
        );
    }

    #[test]
    fn empty_key_becomes_single_underscore() {
        assert_eq!(WorkspaceKey::new("").as_str(), "_");
    }
}

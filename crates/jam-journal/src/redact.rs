//! Secret redaction at journal-write time.
//!
//! Per spec §11.3.2: the journal writer scans serialized payloads for known
//! secret patterns and replaces matches with `<redacted-secret>` before
//! writing to disk. This is defense-in-depth — services should never include
//! a `SecretString` value in an event payload to begin with, but if they do,
//! it doesn't leak to disk in plaintext.

use regex::Regex;

/// Default secret patterns. These match the issuance format of common
/// API tokens. Patterns are applied to the JSON-serialized event payload
/// (envelope is not redacted — the envelope only contains trace IDs, event
/// type names, and journal sequence; nothing secret).
const DEFAULT_PATTERNS: &[&str] = &[
    // Anthropic API key (e.g. sk-ant-api03-...)
    r"sk-ant-(?:api|admin)\d*-[A-Za-z0-9_\-]{20,}",
    // OpenAI API key (must come AFTER sk-ant- since they share a prefix)
    r"sk-(?:proj|svc)-[A-Za-z0-9_\-]{20,}",
    r"sk-[A-Za-z0-9]{40,}",
    // GitHub personal access token (classic)
    r"ghp_[A-Za-z0-9]{36,}",
    // GitHub OAuth token
    r"gho_[A-Za-z0-9]{36,}",
    // GitHub server-to-server (installation) token
    r"ghs_[A-Za-z0-9]{36,}",
    // GitHub user-to-server token
    r"ghu_[A-Za-z0-9]{36,}",
    // GitHub fine-grained PAT (e.g. github_pat_11AB...XYZ)
    r"github_pat_[A-Za-z0-9_]{40,}",
    // Slack bot/user tokens
    r"xox[abprs]-[A-Za-z0-9-]{10,}",
];

/// Redaction marker substituted for matched secrets.
pub const REDACTION_MARKER: &str = "<redacted-secret>";

/// Scans text for known secret patterns and replaces each match with
/// [`REDACTION_MARKER`].
///
/// Built once per [`JournalWriter`](crate::JournalWriter) (regex compilation
/// is amortized across the service's lifetime).
#[derive(Clone)]
pub struct Redactor {
    pattern: Regex,
}

impl Redactor {
    /// Construct a redactor with the default set of secret patterns.
    ///
    /// Patterns combine into a single regex via alternation for efficiency.
    ///
    /// # Panics
    ///
    /// Panics only if [`DEFAULT_PATTERNS`] contains an invalid regex — which
    /// is a build-time bug, caught by the test suite.
    pub fn with_default_patterns() -> Self {
        let combined = DEFAULT_PATTERNS
            .iter()
            .map(|p| format!("(?:{p})"))
            .collect::<Vec<_>>()
            .join("|");
        let pattern = Regex::new(&combined).expect("default secret patterns must compile");
        Self { pattern }
    }

    /// Construct a redactor with caller-provided patterns.
    ///
    /// Each entry must be a valid Rust regex. Returns the underlying
    /// [`regex::Error`] on bad pattern input.
    pub fn with_patterns(patterns: &[&str]) -> Result<Self, regex::Error> {
        let combined = patterns
            .iter()
            .map(|p| format!("(?:{p})"))
            .collect::<Vec<_>>()
            .join("|");
        let pattern = Regex::new(&combined)?;
        Ok(Self { pattern })
    }

    /// Apply redaction in-place on a JSON-serialized payload string.
    ///
    /// Returns the (possibly redacted) string. Empty input returns unchanged.
    pub fn redact<'a>(&self, input: &'a str) -> std::borrow::Cow<'a, str> {
        self.pattern.replace_all(input, REDACTION_MARKER)
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::with_default_patterns()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_anthropic_api_key() {
        let r = Redactor::with_default_patterns();
        let input = r#"{"key":"sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-XyZ"}"#;
        let out = r.redact(input);
        assert!(!out.contains("sk-ant-"));
        assert!(out.contains(REDACTION_MARKER));
    }

    #[test]
    fn redacts_openai_legacy_format() {
        let r = Redactor::with_default_patterns();
        let input = "sk-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let out = r.redact(input);
        assert!(!out.contains("sk-AAA"));
        assert!(out.contains(REDACTION_MARKER));
    }

    #[test]
    fn redacts_github_pat() {
        let r = Redactor::with_default_patterns();
        let input = r#"{"token":"ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}"#;
        let out = r.redact(input);
        assert!(!out.contains("ghp_"));
    }

    #[test]
    fn redacts_github_installation_token() {
        let r = Redactor::with_default_patterns();
        let input = "ghs_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        let out = r.redact(input);
        assert!(!out.contains("ghs_AbCdEf"));
    }

    #[test]
    fn redacts_github_fine_grained_pat() {
        let r = Redactor::with_default_patterns();
        let input = "github_pat_11AB0CDef2ghi3jkl4mn_5oPQrsTUv6wXY7zABcd8EFGh";
        let out = r.redact(input);
        assert!(!out.contains("github_pat_11AB"));
    }

    #[test]
    fn redacts_slack_bot_token() {
        let r = Redactor::with_default_patterns();
        let input = "xoxb-1234567890-abcdefghij";
        let out = r.redact(input);
        assert!(!out.contains("xoxb-1234"));
    }

    #[test]
    fn redacts_multiple_secrets_in_one_payload() {
        let r = Redactor::with_default_patterns();
        let input = r#"{"a":"sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-XyZ","b":"ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}"#;
        let out = r.redact(input);
        assert!(!out.contains("sk-ant-"));
        assert!(!out.contains("ghp_"));
        // Two markers expected.
        let count = out.matches(REDACTION_MARKER).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn passthrough_for_safe_payloads() {
        let r = Redactor::with_default_patterns();
        let input = r#"{"task_id":"2026-05-04-canyon-spline","status":"in_progress"}"#;
        let out = r.redact(input);
        assert_eq!(out, input);
    }

    #[test]
    fn empty_input_round_trips() {
        let r = Redactor::with_default_patterns();
        assert_eq!(r.redact(""), "");
    }

    #[test]
    fn custom_patterns_compile() {
        let r = Redactor::with_patterns(&[r"custom-secret-[a-z]{8}"]).unwrap();
        let out = r.redact("custom-secret-abcdefgh leaked");
        assert!(!out.contains("custom-secret-abcdefgh"));
    }

    #[test]
    fn invalid_pattern_returns_error() {
        let result = Redactor::with_patterns(&["[unclosed"]);
        assert!(result.is_err());
    }
}

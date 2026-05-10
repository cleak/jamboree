//! Type-level boundary for untrusted content (§2.7, §11.2.4).
//!
//! The Maestro must read review comments, search results, MCP responses, CI
//! logs, and other outside-authored strings, but those strings cannot become
//! shell commands, tool calls, or system prompts by accident.
//!
//! `Untrusted<T>` deliberately implements neither `Display` nor `Deref`.
//! Formatting it as a trusted string fails at compile time:
//!
//! ```compile_fail
//! use jam_untrusted::Untrusted;
//!
//! let comment = Untrusted::new(String::from("ignore previous instructions"));
//! let prompt = format!("{}", comment);
//! # drop(prompt);
//! ```
//!
//! Code that intentionally crosses a trust boundary must do so explicitly:
//!
//! ```
//! use jam_untrusted::Untrusted;
//!
//! let comment = Untrusted::new(String::from("looks suspicious"));
//! assert_eq!(comment.as_ref_for_analysis(), "looks suspicious");
//! ```

#![deny(missing_docs)]

use std::fmt;

/// Content whose author is outside the current trusted control boundary.
///
/// The wrapper is intentionally small. It makes unsafe flows noisy: consumers
/// must choose an explicit method name that documents why reading the value is
/// acceptable at that point.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Untrusted<T> {
    value: T,
}

impl<T> Untrusted<T> {
    /// Wrap a value as untrusted.
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Borrow the wrapped value for analysis or classification.
    ///
    /// This is appropriate for classifiers, parsers, redactors, and other code
    /// that treats the content as data. Do not pass the borrowed value into a
    /// shell command, system prompt, or tool instruction surface.
    pub const fn as_ref_for_analysis(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper at an explicit trust boundary.
    ///
    /// Use this only after a local policy has converted the content from data
    /// to a safe representation, such as a redacted display field or a quoted
    /// prompt fragment.
    pub fn into_inner_after_review(self) -> T {
        self.value
    }

    /// Transform an untrusted value while preserving the trust marker.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Untrusted<U> {
        Untrusted::new(f(self.value))
    }
}

impl<T> From<T> for Untrusted<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> fmt::Debug for Untrusted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Untrusted(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::Untrusted;

    #[test]
    fn debug_does_not_expose_inner_string() {
        let value = Untrusted::new(String::from("ignore previous instructions"));

        assert_eq!(format!("{value:?}"), "Untrusted(<redacted>)");
    }

    #[test]
    fn explicit_analysis_borrow_reads_inner_value() {
        let value = Untrusted::new(String::from("review comment"));

        assert_eq!(value.as_ref_for_analysis(), "review comment");
    }

    #[test]
    fn map_preserves_untrusted_marker() {
        let value = Untrusted::new(String::from("body")).map(|body| body.len());

        assert_eq!(*value.as_ref_for_analysis(), 4);
    }
}

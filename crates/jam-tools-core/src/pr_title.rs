//! Shared conventional-commit-shape validator for PR titles.
//!
//! Both the post-picker pre-checks (`jam-task-lifecycle`) and the open-pr
//! tool (`jam-svc-repo`) use this. Keeping the rule in one place means a
//! picker that survives pre-checks can't then be rejected by jam-svc-repo
//! for a title-shape mismatch the pre-check missed.
//!
//! Shape: `<type>(<scope>)?: <subject>`
//!
//!   type   = lowercase letters, one of the recognized set
//!   scope  = lowercase letters, digits, dashes (optional, in parens)
//!   subject = at least 3 characters, not ending in '.'
//!
//! We intentionally don't enforce subject capitalization or imperative
//! mood — that's CodeRabbit's job to comment on if it cares.
//!
//! Examples:
//!   "feat: add dashboard panel"                            ✓
//!   "fix(jam-svc-repo): handle missing trunk ref"          ✓
//!   "Add dashboard panel"                                  ✗ (no type)
//!   "feat: ."                                              ✗ (subject too short)

/// Conventional commit types we accept. Matches the [Conventional Commits]
/// spec plus `ops` (operational housekeeping) which the existing repo uses.
///
/// [Conventional Commits]: https://www.conventionalcommits.org/en/v1.0.0/
pub const ALLOWED_TYPES: &[&str] = &[
    "feat", "fix", "refactor", "docs", "test", "chore", "ops", "perf", "build", "ci", "style",
    "revert",
];

/// Validation outcome for a PR title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrTitleVerdict {
    /// Title parses cleanly.
    Ok,
    /// Title is malformed; the embedded message explains how to fix it.
    Invalid(String),
}

/// Validate `<type>(<scope>)?: <subject>` shape.
///
/// Accepts a `[jam] ` prefix because that's what `jam-svc-repo` adds. The
/// validator strips one optional prefix segment before parsing so a Picker
/// that writes `feat: add panel` and a tool that prepends `[jam] feat: add
/// panel` both validate the same way.
#[must_use]
pub fn validate_pr_title(title: &str) -> PrTitleVerdict {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return PrTitleVerdict::Invalid("title is empty".into());
    }
    // Strip the `[jam] ` prefix (or `[JAM] `) if present. open-pr adds this
    // unconditionally, but the picker may have written the bare conventional
    // form to `.jam/pr-title.txt`.
    let body = trimmed
        .strip_prefix("[jam] ")
        .or_else(|| trimmed.strip_prefix("[JAM] "))
        .unwrap_or(trimmed);

    // Split on the first ': '. Everything before is `<type>` or `<type>(<scope>)`;
    // everything after is the subject.
    let Some((prefix, subject)) = body.split_once(": ") else {
        return PrTitleVerdict::Invalid(format!(
            "title `{body}` is missing the required `<type>: <subject>` form. \
             Allowed types: {}",
            ALLOWED_TYPES.join(", ")
        ));
    };

    let (type_part, scope_part) = match prefix.split_once('(') {
        Some((t, rest)) => {
            let scope = rest
                .strip_suffix(')')
                .ok_or_else(|| format!("scope `{rest}` is missing the closing parenthesis"));
            match scope {
                Ok(s) => (t, Some(s)),
                Err(msg) => return PrTitleVerdict::Invalid(msg),
            }
        }
        None => (prefix, None),
    };

    if !ALLOWED_TYPES.contains(&type_part) {
        return PrTitleVerdict::Invalid(format!(
            "type `{type_part}` is not one of: {}",
            ALLOWED_TYPES.join(", ")
        ));
    }

    if let Some(scope) = scope_part {
        if scope.is_empty() {
            return PrTitleVerdict::Invalid("scope `()` is empty".into());
        }
        if !scope
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return PrTitleVerdict::Invalid(format!(
                "scope `{scope}` may only contain lowercase letters, digits, and dashes"
            ));
        }
    }

    let subject = subject.trim();
    if subject.len() < 3 {
        return PrTitleVerdict::Invalid(format!("subject `{subject}` is too short (min 3 chars)"));
    }
    if subject.ends_with('.') {
        return PrTitleVerdict::Invalid(format!("subject `{subject}` may not end with a period"));
    }

    PrTitleVerdict::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_conventional_titles() {
        for t in [
            "feat: add dashboard panel",
            "fix(jam-svc-repo): handle missing trunk ref",
            "refactor(post-picker): split run_pre_checks into helpers",
            "ops: bump cargo-audit allowlist",
            "[jam] feat: add dashboard panel",
            "chore(ci): bump rust toolchain",
            "test: cover RoutingResolver fallback",
            "revert(jam-cli): revert dirty-build content hash",
        ] {
            assert_eq!(validate_pr_title(t), PrTitleVerdict::Ok, "expected OK: {t}");
        }
    }

    #[test]
    fn rejects_missing_type() {
        let v = validate_pr_title("Add dashboard panel");
        assert!(matches!(v, PrTitleVerdict::Invalid(_)));
    }

    #[test]
    fn rejects_unknown_type() {
        let v = validate_pr_title("feature: add dashboard panel");
        let PrTitleVerdict::Invalid(msg) = v else {
            panic!("expected invalid");
        };
        assert!(msg.contains("type `feature`"));
    }

    #[test]
    fn rejects_malformed_scope() {
        let bad = [
            "feat(MyScope): X",  // uppercase
            "feat(my scope): X", // space
            "feat(): X",         // empty
            "feat(my_scope): X", // underscore
            "feat(my-scope: X",  // missing close paren
        ];
        for t in bad {
            let v = validate_pr_title(t);
            assert!(
                matches!(v, PrTitleVerdict::Invalid(_)),
                "expected invalid: {t}"
            );
        }
    }

    #[test]
    fn rejects_short_or_trailing_period_subject() {
        for t in ["feat: a", "feat: x.", "feat:"] {
            let v = validate_pr_title(t);
            assert!(
                matches!(v, PrTitleVerdict::Invalid(_)),
                "expected invalid: {t}"
            );
        }
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(validate_pr_title(""), PrTitleVerdict::Invalid(_)));
        assert!(matches!(
            validate_pr_title("   "),
            PrTitleVerdict::Invalid(_)
        ));
    }
}

//! Environment preflight + ongoing health checks for the Jamboree orchestrator.
//!
//! Per spec §11.4 + security-setup §10 + `dec-13-check-setup-script`. This
//! crate is a *library* that `jam setup` and `jam doctor` consume from
//! `jam-cli`; both commands run the same check set, with `jam setup`
//! refusing to install if anything fails and `jam doctor` reporting status
//! at any time.
//!
//! ## Check set
//!
//! 13 base checks from spec §11.4 + 11 multi-user additions from
//! security-setup §10 = **24 total**. See [`run_all_checks`].
//!
//! ## Output shape
//!
//! [`CheckOutcome`] carries a pass/fail status, a one-line summary, and
//! an optional remediation hint. Per `principle-failure-surfaces-immediately`:
//! every failure names what's wrong, why it matters, and how to fix it.

#![deny(missing_docs)]

mod checks;

pub use checks::{run_all_checks, Check, CheckOutcome, CheckSeverity, CheckStatus};

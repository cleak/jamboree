//! UI server primitives shared by the `jam-ui-server` binary and `jam` CLI.

#![deny(missing_docs)]

/// Session-token issuance and verification.
pub mod auth;

/// Durable journal trace replay.
pub mod trace_replay;

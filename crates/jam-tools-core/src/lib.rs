//! JSON Schema contracts for tool service request/response payloads.
//!
//! Schemas live under `schemas/<service>/<tool>.<request|response>.json`.
//! `tools/pydantic-gen.py` turns them into Maestro-side Pydantic models so
//! Python tool calls cross the Rust boundary through typed contracts (§11.2.6).

#![deny(missing_docs)]

/// Shared provider and execution trait contracts.
pub mod contracts;

/// Shared runtime path resolution helpers.
pub mod paths;

/// Path-safe workspace key newtype.
pub mod workspace;

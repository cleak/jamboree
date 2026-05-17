//! NATS JetStream client wrapper for the Jamboree orchestrator.
//!
//! Per spec §4.4.1 and `comp-traced-publish-wrapper`: every publish carries
//! the trace context (`Trace-Id`, optional `Parent-Trace-Id`) as message
//! headers. [`JamNats::publish_traced`] is the only public publish API; raw
//! [`async_nats::Client::publish`] is forbidden in non-trace crates by the
//! workspace clippy lint when the rule is wired (`task-trace-continuity-integration-test`).
//!
//! ## Subjects
//!
//! Per `api-nats-bus-subjects-catalog` (spec §21.1). Highlights:
//!
//! ```text
//! journal.<event-type>                — durable journal events
//! picker.<session-id>.{lifecycle,output,msg.{queue,interrupt,kill,status}}
//! tool.<service>.<method>             — request-reply tool invocations
//! tool.<service>.ping[.<version>]     — health checks
//! tool.<service>.drain.<version>      — atomic-swap drain signal
//! ```
//!
//! ## KV buckets (spec §4.4.1)
//!
//! `routing-manifest`, `harness-versions`, `dispatch-state`, `setup-result`,
//! `patch-lock` — created by [`JamNats::ensure_kv_buckets`] at substrate
//! startup. Idempotent; safe to call repeatedly.

#![deny(missing_docs)]

mod client;
mod headers;
mod routing_manifest;
mod setup;

pub use client::{JamNats, NatsError};
pub use headers::{
    build_trace_headers, extract_trace_from_headers, PARENT_TRACE_ID_HEADER, TRACE_ID_HEADER,
};
pub use routing_manifest::{
    default_subject_prefix, load_current_routing_manifest, load_routing_manifest_revision,
    manifest_id_for_revision, revision_from_manifest_id, version_subject_suffix,
    write_current_routing_manifest, RoutingManifest, RoutingManifestEntry, RoutingResolver,
    RoutingService, ROUTING_MANIFEST_BUCKET, ROUTING_MANIFEST_KEY, ROUTING_MANIFEST_SCHEMA_VERSION,
    ROUTING_MANIFEST_UPDATED_SUBJECT,
};
pub use setup::{
    default_kv_buckets, default_streams, ensure_kv_buckets, ensure_streams, KvBucketSpec,
    StreamSpec,
};

pub use async_nats;

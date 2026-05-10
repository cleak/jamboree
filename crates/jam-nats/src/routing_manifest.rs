//! Routing manifest schema stored in the `routing-manifest` NATS KV bucket.
//!
//! Per spec §20.2, the manifest is the single source of truth for which
//! versioned subject prefix the Maestro should use for each out-of-process
//! tool service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::client::NatsError;

/// NATS KV bucket containing the routing manifest.
pub const ROUTING_MANIFEST_BUCKET: &str = "routing-manifest";
/// Key for the current routing manifest blob.
pub const ROUTING_MANIFEST_KEY: &str = "current";
/// Core NATS subject emitted after the current manifest changes.
pub const ROUTING_MANIFEST_UPDATED_SUBJECT: &str = "routing-manifest.updated";
/// Current JSON schema version for routing manifest blobs.
pub const ROUTING_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Full routing manifest JSON blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingManifest {
    /// Schema version for forwards-compatible parsing.
    pub schema_version: u32,
    /// Timestamp of the latest manifest update.
    pub updated_at: DateTime<Utc>,
    /// Human or service principal that wrote the manifest.
    pub updated_by: String,
    /// Trace that caused this manifest write.
    pub trace_id: String,
    /// Per-service routing entries keyed by service name (`observe`, `repo`, etc.).
    pub services: BTreeMap<String, RoutingService>,
    /// Previous manifest revision id, if one existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_manifest_id: Option<String>,
}

impl RoutingManifest {
    /// Construct an empty schema-v1 manifest.
    #[must_use]
    pub fn empty(updated_by: String, trace_id: String, updated_at: DateTime<Utc>) -> Self {
        Self {
            schema_version: ROUTING_MANIFEST_SCHEMA_VERSION,
            updated_at,
            updated_by,
            trace_id,
            services: BTreeMap::new(),
            previous_manifest_id: None,
        }
    }

    /// Resolve a NATS request subject for `service.method`.
    #[must_use]
    pub fn subject_for(&self, service: &str, method: &str) -> Option<String> {
        let method = method.trim();
        if method.is_empty() {
            return None;
        }
        let route = self.services.get(service)?;
        Some(format!(
            "{}.{}",
            route.subject_prefix.trim_end_matches('.'),
            method
        ))
    }
}

/// One service entry in the routing manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingService {
    /// Current service version string.
    pub current_version: String,
    /// NATS subject prefix for this version, e.g. `tool.observe.v047`.
    pub subject_prefix: String,
    /// Runtime binary path for this service version.
    pub binary_path: PathBuf,
    /// SHA-256 of the runtime binary.
    pub binary_sha256: String,
    /// Timestamp this service version became current.
    pub started_at: DateTime<Utc>,
    /// Expected health-check status.
    pub expected_health: String,
}

/// Routing manifest plus the KV revision that supplied it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingManifestEntry {
    /// Parsed manifest blob.
    pub manifest: RoutingManifest,
    /// NATS KV revision for compare-and-swap updates.
    pub revision: u64,
}

impl RoutingManifestEntry {
    /// Stable id used by the next manifest's `previous_manifest_id`.
    #[must_use]
    pub fn manifest_id(&self) -> String {
        manifest_id_for_revision(self.revision)
    }
}

/// Stable id for a NATS KV revision.
#[must_use]
pub fn manifest_id_for_revision(revision: u64) -> String {
    format!("{ROUTING_MANIFEST_BUCKET}:{revision}")
}

/// Return the conventional versioned subject prefix for a service version.
#[must_use]
pub fn default_subject_prefix(service: &str, version: &str) -> String {
    format!("tool.{service}.{}", version_subject_suffix(version))
}

/// Convert a version string into a NATS-safe subject segment.
#[must_use]
pub fn version_subject_suffix(version: &str) -> String {
    let mut suffix: String = version
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    if suffix.is_empty() {
        suffix.push('0');
    }
    if suffix.starts_with('v') {
        suffix
    } else {
        format!("v{suffix}")
    }
}

/// Load the current routing manifest from NATS KV.
pub async fn load_current_routing_manifest(
    js: &async_nats::jetstream::Context,
) -> Result<Option<RoutingManifestEntry>, NatsError> {
    let kv = js
        .get_key_value(ROUTING_MANIFEST_BUCKET)
        .await
        .map_err(|err| NatsError::JetStream(format!("open {ROUTING_MANIFEST_BUCKET}: {err}")))?;
    let Some(entry) = kv
        .entry(ROUTING_MANIFEST_KEY)
        .await
        .map_err(|err| NatsError::JetStream(format!("read {ROUTING_MANIFEST_KEY}: {err}")))?
    else {
        return Ok(None);
    };
    let manifest = serde_json::from_slice(&entry.value)?;
    Ok(Some(RoutingManifestEntry {
        manifest,
        revision: entry.revision,
    }))
}

/// Load a routing manifest from an exact NATS KV revision.
pub async fn load_routing_manifest_revision(
    js: &async_nats::jetstream::Context,
    revision: u64,
) -> Result<Option<RoutingManifestEntry>, NatsError> {
    let kv = js
        .get_key_value(ROUTING_MANIFEST_BUCKET)
        .await
        .map_err(|err| NatsError::JetStream(format!("open {ROUTING_MANIFEST_BUCKET}: {err}")))?;
    let Some(entry) = kv
        .entry_for_revision(ROUTING_MANIFEST_KEY, revision)
        .await
        .map_err(|err| {
            NatsError::JetStream(format!("read {ROUTING_MANIFEST_KEY}@{revision}: {err}"))
        })?
    else {
        return Ok(None);
    };
    let manifest = serde_json::from_slice(&entry.value)?;
    Ok(Some(RoutingManifestEntry {
        manifest,
        revision: entry.revision,
    }))
}

/// Parse a manifest id created by [`manifest_id_for_revision`].
#[must_use]
pub fn revision_from_manifest_id(manifest_id: &str) -> Option<u64> {
    manifest_id
        .strip_prefix(&format!("{ROUTING_MANIFEST_BUCKET}:"))?
        .parse()
        .ok()
}

/// Write the current routing manifest using NATS KV compare-and-swap.
pub async fn write_current_routing_manifest(
    js: &async_nats::jetstream::Context,
    manifest: &RoutingManifest,
    expected_revision: Option<u64>,
) -> Result<u64, NatsError> {
    let kv = js
        .get_key_value(ROUTING_MANIFEST_BUCKET)
        .await
        .map_err(|err| NatsError::JetStream(format!("open {ROUTING_MANIFEST_BUCKET}: {err}")))?;
    let bytes = serde_json::to_vec(manifest)?;
    match expected_revision {
        Some(revision) => kv
            .update(ROUTING_MANIFEST_KEY, bytes.into(), revision)
            .await
            .map_err(|err| {
                NatsError::JetStream(format!(
                    "compare-and-swap {ROUTING_MANIFEST_KEY} at revision {revision}: {err}"
                ))
            }),
        None => kv
            .create(ROUTING_MANIFEST_KEY, bytes.into())
            .await
            .map_err(|err| NatsError::JetStream(format!("create {ROUTING_MANIFEST_KEY}: {err}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_subject_suffix_matches_spec_example() {
        assert_eq!(version_subject_suffix("0.4.7"), "v047");
        assert_eq!(version_subject_suffix("v1.2.3"), "v123");
        assert_eq!(version_subject_suffix(""), "v0");
    }

    #[test]
    fn manifest_resolves_service_method_subject() {
        let now = Utc::now();
        let mut manifest = RoutingManifest::empty(
            "human:caleb".into(),
            "01HXKJ00000000000000000000".into(),
            now,
        );
        manifest.services.insert(
            "observe".into(),
            RoutingService {
                current_version: "0.4.7".into(),
                subject_prefix: "tool.observe.v047".into(),
                binary_path: PathBuf::from("/home/maestro/.jam/bin/jam-svc-observe-0.4.7"),
                binary_sha256: "abc123".into(),
                started_at: now,
                expected_health: "ok".into(),
            },
        );

        assert_eq!(
            manifest.subject_for("observe", "world-snapshot"),
            Some("tool.observe.v047.world-snapshot".into())
        );
        assert_eq!(manifest.subject_for("missing", "world-snapshot"), None);
        assert_eq!(manifest.subject_for("observe", ""), None);
    }

    #[test]
    fn manifest_id_revision_round_trips() {
        let manifest_id = manifest_id_for_revision(42);
        assert_eq!(revision_from_manifest_id(&manifest_id), Some(42));
        assert_eq!(revision_from_manifest_id("other:42"), None);
        assert_eq!(revision_from_manifest_id("routing-manifest:nope"), None);
    }
}

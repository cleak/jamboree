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

/// Cached resolver for `service.method` → live NATS subject lookups.
///
/// Rust services that call other Rust services (e.g. `jam-svc-session`
/// calling `tool.worktree.create`) need to consult the routing manifest
/// because the called service's actual subject changes whenever
/// `jam-patch-agent` hot-patches it to a new version. Direct use of the
/// unversioned `tool.<service>.<method>` subject is fragile — it works
/// only when no patch has been applied yet.
///
/// Equivalent to Python's `RoutingManifestRouter` in
/// `maestro/src/jam_maestro/routing_manifest.py`. Caches the manifest
/// and refreshes on every `routing-manifest.updated` event the caller
/// pumps through `apply_update`. Falls back to the unversioned subject
/// when the manifest doesn't list the service (first-deploy case).
#[derive(Debug, Clone)]
pub struct RoutingResolver {
    js: Option<async_nats::jetstream::Context>,
    cache: std::sync::Arc<tokio::sync::RwLock<RoutingCache>>,
}

#[derive(Debug, Default)]
struct RoutingCache {
    manifest: Option<RoutingManifest>,
    revision: Option<u64>,
}

impl RoutingResolver {
    /// Build a resolver bound to the given JetStream context. The cache
    /// starts empty; the first `subject_for` call triggers an initial load.
    #[must_use]
    pub fn new(js: async_nats::jetstream::Context) -> Self {
        Self {
            js: Some(js),
            cache: std::sync::Arc::new(tokio::sync::RwLock::new(RoutingCache::default())),
        }
    }

    /// Disconnected resolver for tests and non-NATS contexts. Always returns
    /// `tool.<service>.<method>` (the unversioned fallback). No NATS calls.
    #[must_use]
    pub fn disconnected() -> Self {
        Self {
            js: None,
            cache: std::sync::Arc::new(tokio::sync::RwLock::new(RoutingCache::default())),
        }
    }

    /// Resolve `service.method` to the current NATS request subject.
    ///
    /// Falls back to `tool.<service>.<method>` if the manifest doesn't
    /// list `service` — that path keeps first-deploy / cold-start flows
    /// working before patch-agent has staged anything.
    pub async fn subject_for(&self, service: &str, method: &str) -> String {
        {
            let cache = self.cache.read().await;
            if let Some(manifest) = cache.manifest.as_ref() {
                if let Some(subject) = manifest.subject_for(service, method) {
                    return subject;
                }
            }
        }
        // Either no cached manifest yet, or the cached manifest doesn't
        // list this service. Refresh and try once more before falling
        // back. This is the slow path; expected at startup and after
        // explicit `apply_update` calls.
        if let Err(err) = self.refresh().await {
            tracing::warn!(error = %err, service, method, "routing manifest refresh failed; falling back to unversioned subject");
        }
        {
            let cache = self.cache.read().await;
            if let Some(manifest) = cache.manifest.as_ref() {
                if let Some(subject) = manifest.subject_for(service, method) {
                    return subject;
                }
            }
        }
        format!("tool.{service}.{method}")
    }

    /// Force-reload the manifest from NATS KV. Idempotent; cheap. No-op for
    /// resolvers built with [`disconnected`](Self::disconnected).
    pub async fn refresh(&self) -> Result<(), NatsError> {
        let Some(js) = self.js.as_ref() else {
            return Ok(());
        };
        let entry = load_current_routing_manifest(js).await?;
        let mut cache = self.cache.write().await;
        if let Some(e) = entry {
            cache.revision = Some(e.revision);
            cache.manifest = Some(e.manifest);
        } else {
            cache.revision = None;
            cache.manifest = None;
        }
        Ok(())
    }

    /// Update the cache from a `routing-manifest.updated` event payload.
    /// Call this from the subscriber loop that pumps the update subject.
    /// Returns `true` if the payload advanced the cache (newer revision),
    /// `false` if it was a duplicate or older.
    pub async fn apply_update_revision(&self, revision: u64) -> Result<bool, NatsError> {
        {
            let cache = self.cache.read().await;
            if cache.revision.is_some_and(|cached| cached >= revision) {
                return Ok(false);
            }
        }
        // Always re-load from KV rather than trust the event payload — the
        // event carries `manifest_id` but not the full manifest blob.
        self.refresh().await?;
        Ok(true)
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

    #[tokio::test]
    async fn disconnected_resolver_returns_unversioned_fallback() {
        let resolver = RoutingResolver::disconnected();
        // No NATS reachable, but no panic and no infinite refresh loop.
        let subject = resolver.subject_for("worktree", "create").await;
        assert_eq!(subject, "tool.worktree.create");
    }

    #[tokio::test]
    async fn disconnected_resolver_refresh_is_noop() {
        let resolver = RoutingResolver::disconnected();
        // Repeated calls should not error.
        resolver.refresh().await.unwrap();
        resolver.refresh().await.unwrap();
    }

    #[test]
    fn manifest_id_revision_round_trips() {
        let manifest_id = manifest_id_for_revision(42);
        assert_eq!(revision_from_manifest_id(&manifest_id), Some(42));
        assert_eq!(revision_from_manifest_id("other:42"), None);
        assert_eq!(revision_from_manifest_id("routing-manifest:nope"), None);
    }
}

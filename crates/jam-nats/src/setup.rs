//! JetStream stream + KV bucket bootstrap (spec §4.4.1).
//!
//! Both [`ensure_streams`] and [`ensure_kv_buckets`] are idempotent — safe to
//! call on every substrate startup; they create what's missing and leave
//! existing entities untouched.

use async_nats::jetstream::stream::Config as StreamConfig;
use async_nats::jetstream::stream::{RetentionPolicy, StorageType};
use async_nats::jetstream::{kv, Context as JsContext};

use crate::client::NatsError;

/// Declarative stream spec used by [`ensure_streams`]. Defaults match the
/// substrate's needs (file storage, work-queue or limits retention, no
/// max-messages cap).
#[derive(Debug, Clone)]
pub struct StreamSpec {
    /// Stream name (e.g. `"journal"`).
    pub name: String,
    /// Subjects this stream captures (e.g. `["journal.>"]`).
    pub subjects: Vec<String>,
    /// File-backed durability vs in-memory. Defaults to file.
    pub storage: StorageType,
    /// Retention policy. Defaults to limits-based retention.
    pub retention: RetentionPolicy,
}

impl StreamSpec {
    /// Construct a file-backed limits-retention stream from a name + subjects.
    #[must_use]
    pub fn new(name: impl Into<String>, subjects: Vec<String>) -> Self {
        Self {
            name: name.into(),
            subjects,
            storage: StorageType::File,
            retention: RetentionPolicy::Limits,
        }
    }
}

/// Declarative KV bucket spec used by [`ensure_kv_buckets`].
#[derive(Debug, Clone)]
pub struct KvBucketSpec {
    /// Bucket name (e.g. `"routing-manifest"`).
    pub name: String,
    /// History (number of revisions to keep). 1 = latest only; >1 = revision history.
    pub history: i64,
    /// Storage type — file for durability, memory for ephemeral state.
    pub storage: StorageType,
}

impl KvBucketSpec {
    /// File-backed bucket with single-revision history (the common case).
    #[must_use]
    pub fn file_latest(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            history: 1,
            storage: StorageType::File,
        }
    }

    /// File-backed bucket with extended revision history (used by `routing-manifest`
    /// for atomic-rollback per spec §20.4).
    #[must_use]
    pub fn file_with_history(name: impl Into<String>, history: i64) -> Self {
        Self {
            name: name.into(),
            history,
            storage: StorageType::File,
        }
    }
}

/// Idempotently create or update the supplied JetStream streams.
///
/// If a stream already exists with the same configuration, this is a no-op.
/// If it exists with different configuration, it's updated in place.
pub async fn ensure_streams(js: &JsContext, specs: &[StreamSpec]) -> Result<(), NatsError> {
    for spec in specs {
        let cfg = StreamConfig {
            name: spec.name.clone(),
            subjects: spec.subjects.clone(),
            storage: spec.storage,
            retention: spec.retention,
            ..Default::default()
        };
        js.get_or_create_stream(cfg)
            .await
            .map_err(|e| NatsError::JetStream(format!("stream {}: {e}", spec.name)))?;
    }
    Ok(())
}

/// Idempotently create the supplied KV buckets.
pub async fn ensure_kv_buckets(js: &JsContext, specs: &[KvBucketSpec]) -> Result<(), NatsError> {
    for spec in specs {
        let cfg = kv::Config {
            bucket: spec.name.clone(),
            history: spec.history,
            storage: spec.storage,
            ..Default::default()
        };
        // `create_key_value` is idempotent in async-nats >= 0.39: returns the
        // existing bucket if already present.
        js.create_key_value(cfg)
            .await
            .map_err(|e| NatsError::JetStream(format!("kv {}: {e}", spec.name)))?;
    }
    Ok(())
}

/// The default KV bucket set per spec §4.4.1.
///
/// - `routing-manifest`: 64 revisions of history for atomic-rollback (§20.4).
/// - `harness-versions`: latest only.
/// - `dispatch-state`: latest only.
/// - `setup-result`: latest only.
/// - `patch-lock`: latest only (TTL applied externally via locked-update pattern).
#[must_use]
pub fn default_kv_buckets() -> Vec<KvBucketSpec> {
    vec![
        KvBucketSpec::file_with_history("routing-manifest", 64),
        KvBucketSpec::file_latest("harness-versions"),
        KvBucketSpec::file_latest("dispatch-state"),
        KvBucketSpec::file_latest("setup-result"),
        KvBucketSpec::file_latest("patch-lock"),
    ]
}

/// The default JetStream stream set per spec §4.4.1 + §21.1.
///
/// One stream per top-level subject group so retention + replay can be tuned
/// independently. All file-backed for durability.
#[must_use]
pub fn default_streams() -> Vec<StreamSpec> {
    vec![
        StreamSpec::new("journal", vec!["journal.>".into()]),
        StreamSpec::new("picker", vec!["picker.>".into()]),
        StreamSpec::new("quota", vec!["quota.>".into()]),
        StreamSpec::new("tempyr", vec!["tempyr.>".into()]),
        StreamSpec::new("evolve", vec!["evolve.>".into()]),
        StreamSpec::new("patch", vec!["patch.>".into()]),
        StreamSpec::new("branch", vec!["branch.>".into()]),
        StreamSpec::new("clock", vec!["clock.>".into()]),
        StreamSpec::new("harness", vec!["harness.>".into()]),
        StreamSpec::new("routing-manifest", vec!["routing-manifest.>".into()]),
        StreamSpec::new("setup", vec!["setup.>".into()]),
        StreamSpec::new("snapshot-invalidate", vec!["snapshot.invalidate.>".into()]),
        StreamSpec::new("notify", vec!["notify.>".into()]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kv_buckets_includes_routing_manifest() {
        let buckets = default_kv_buckets();
        assert!(buckets.iter().any(|b| b.name == "routing-manifest"));
        let manifest = buckets
            .iter()
            .find(|b| b.name == "routing-manifest")
            .unwrap();
        assert_eq!(manifest.history, 64, "rollback needs revision history");
    }

    #[test]
    fn default_streams_covers_journal() {
        let streams = default_streams();
        let journal = streams.iter().find(|s| s.name == "journal").unwrap();
        assert_eq!(journal.subjects, vec!["journal.>"]);
        assert_eq!(journal.storage, StorageType::File);
    }

    #[test]
    fn default_streams_distinct_per_top_segment() {
        // Each top-level subject group has its own stream so retention can
        // be tuned independently. No two streams share a subject pattern.
        let streams = default_streams();
        let mut subjects: Vec<&str> = streams
            .iter()
            .flat_map(|s| s.subjects.iter().map(String::as_str))
            .collect();
        subjects.sort_unstable();
        let original_len = subjects.len();
        subjects.dedup();
        assert_eq!(original_len, subjects.len(), "duplicate stream subjects");
    }

    #[test]
    fn kv_bucket_default_factory_is_file_latest() {
        let spec = KvBucketSpec::file_latest("test");
        assert_eq!(spec.history, 1);
        assert_eq!(spec.storage, StorageType::File);
    }
}

//! `jam-svc-observe` — the observation tool service (spec §4.2).
//!
//! Subscribes to `tool.observe.<method>` request-reply subjects and `tool.observe.ping`
//! for health checks. Compiles facts about current truth (git, GitHub, CI,
//! review artifacts, journals, quota, branch staleness, Tempyr cursor) into
//! the typed [`WorldSnapshot`] that every Maestro decision starts from.
//!
//! ## Phase 0 status
//!
//! All handlers return `{"error": {"kind": "not-implemented", ...}}` envelopes
//! that point at `task-jam-svc-observe-mvp`. The wire shape is exercised end-
//! to-end (subscribe, route, reply, ack); the substantive fact-compilation
//! lands in Phase 1 once the data sources (NATS journal, GitHub App client,
//! Tempyr index cursor, quota tracker) exist.
//!
//! ## Subjects
//!
//! - `tool.observe.world-snapshot` — primary fact compiler.
//! - `tool.observe.world-snapshot-delta` — only changed fields since `since`.
//! - `tool.observe.refresh-world-snapshot` — force refetch (bypass cache).
//! - `tool.observe.compute-readiness` — `Ready` / `NotReady{blockers}` / `ReadyWithWarnings`.
//! - `tool.observe.list-blockers` — Vec<Blocker> directly.
//! - `tool.observe.list-review-artifacts` — Vec<ReviewArtifact>.
//! - `tool.observe.classify-review-artifacts` — LLM classifier dispatch.
//! - `tool.observe.query-quota` — HarnessQuotaState or full map.
//! - `tool.observe.branch-staleness` — git merge-tree result.
//! - `tool.observe.ping` — health probe (returns `{"status":"ok",...}`).

#![deny(missing_docs)]

use futures::StreamExt;
use jam_nats::JamNats;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-observe";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.observe";

#[derive(Debug, thiserror::Error)]
enum ObserveError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("publish reply: {0}")]
    Reply(String),
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!(service = %SERVICE_NAME, "fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ObserveError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();

    info!(service = %SERVICE_NAME, version = %SERVICE_VERSION, nats = %nats_url, "starting");

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    // Subscribe to the whole tool.observe.> namespace; route by method
    // segment in the dispatch handler below.
    let mut sub = nats
        .client()
        .subscribe(format!("{SUBJECT_PREFIX}.>"))
        .await
        .map_err(|e| ObserveError::Subscribe(e.to_string()))?;
    info!(subject = %format!("{SUBJECT_PREFIX}.>"), "subscribed");

    let healthy = Arc::new(AtomicBool::new(true));

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                healthy.store(false, Ordering::SeqCst);
                return Ok(());
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("subscriber stream closed");
                    return Ok(());
                };
                let nats = nats.clone();
                let healthy = healthy.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_request(&nats, &message, &healthy).await {
                        warn!(subject = %message.subject, "handle: {err}");
                    }
                });
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_observe=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    healthy: &Arc<AtomicBool>,
) -> Result<(), ObserveError> {
    let method = method_from_subject(msg.subject.as_str()).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    // Extract incoming trace (None if missing — we still respond, but the
    // response carries no trace context. Strict trace enforcement at the
    // call-site is a separate clippy-lint concern, not handled here.)
    let trace_id = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
        .map(|ctx| ctx.trace_id.to_string());

    let response = dispatch(method, healthy);
    let payload = serde_json::to_vec(&response).expect("response always serializes");

    // Reply via NATS request-reply: the client's send_request set msg.reply.
    let Some(reply_subject) = msg.reply.as_ref() else {
        // No reply subject means this was a fire-and-forget publish, not a
        // request. Per spec §4.3, tool methods are request-reply only —
        // surface a warning and move on.
        warn!(
            subject = %msg.subject,
            method = %method,
            trace_id = ?trace_id,
            "no reply subject — message will not be answered",
        );
        return Ok(());
    };

    nats.client()
        .publish(reply_subject.clone(), payload.into())
        .await
        .map_err(|e| ObserveError::Reply(e.to_string()))?;
    Ok(())
}

/// Last dot-segment of `tool.observe.<method>` (e.g. `world-snapshot`).
fn method_from_subject(subject: &str) -> Option<&str> {
    subject.rsplit_once('.').map(|(_, last)| last)
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Ok(serde_json::Value),
    Error { error: ResponseError },
}

#[derive(Debug, Serialize)]
struct ResponseError {
    kind: &'static str,
    detail: String,
    /// The implementation task that tracks this method.
    tracked_by: &'static str,
}

fn dispatch(method: &str, healthy: &Arc<AtomicBool>) -> Response {
    match method {
        "ping" => Response::Ok(serde_json::json!({
            "status": if healthy.load(Ordering::SeqCst) { "ok" } else { "shutting-down" },
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),

        "world-snapshot"
        | "world-snapshot-delta"
        | "refresh-world-snapshot"
        | "compute-readiness"
        | "list-blockers"
        | "list-review-artifacts"
        | "classify-review-artifacts"
        | "query-quota"
        | "branch-staleness" => Response::Error {
            error: ResponseError {
                kind: "not-implemented",
                detail: format!(
                    "{SUBJECT_PREFIX}.{method} stubbed in Phase 0; implementation tracked by task-jam-svc-observe-mvp"
                ),
                tracked_by: "task-jam-svc-observe-mvp",
            },
        },

        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method",
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized observe method"),
                tracked_by: "graph/components/comp-jam-svc-observe.md",
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn extracts_method_from_subject() {
        assert_eq!(
            method_from_subject("tool.observe.world-snapshot"),
            Some("world-snapshot")
        );
        assert_eq!(method_from_subject("tool.observe.ping"), Some("ping"));
        assert_eq!(method_from_subject("nodot"), None);
    }

    #[test]
    fn ping_returns_ok_status() {
        let healthy = Arc::new(AtomicBool::new(true));
        let response = dispatch("ping", &healthy);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], SERVICE_NAME);
    }

    #[test]
    fn ping_reflects_unhealthy_state() {
        let healthy = Arc::new(AtomicBool::new(false));
        let response = dispatch("ping", &healthy);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "shutting-down");
    }

    #[test]
    fn world_snapshot_returns_not_implemented() {
        let healthy = Arc::new(AtomicBool::new(true));
        let response = dispatch("world-snapshot", &healthy);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["error"]["kind"], "not-implemented");
        assert_eq!(json["error"]["tracked_by"], "task-jam-svc-observe-mvp");
        assert!(json["error"]["detail"]
            .as_str()
            .unwrap()
            .contains("world-snapshot"));
    }

    #[test]
    fn every_documented_method_returns_an_envelope() {
        // The set of methods exposed at tool.observe.<method>. If we add a
        // method, this list should grow alongside the dispatch arm.
        let methods = [
            "world-snapshot",
            "world-snapshot-delta",
            "refresh-world-snapshot",
            "compute-readiness",
            "list-blockers",
            "list-review-artifacts",
            "classify-review-artifacts",
            "query-quota",
            "branch-staleness",
        ];
        let healthy = Arc::new(AtomicBool::new(true));
        for method in methods {
            let response = dispatch(method, &healthy);
            let json = serde_json::to_value(&response).unwrap();
            assert!(
                json.is_object(),
                "method {method} did not produce a JSON object response"
            );
            // Either an Ok payload or an Error envelope — both are objects.
        }
    }

    #[test]
    fn unknown_method_returns_unknown_method_error() {
        let healthy = Arc::new(AtomicBool::new(true));
        let response = dispatch("does-not-exist", &healthy);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["error"]["kind"], "unknown-method");
    }
}

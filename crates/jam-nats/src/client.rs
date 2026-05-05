//! [`JamNats`] — connection wrapper that enforces trace propagation on every
//! publish and request.

use async_nats::jetstream::{self, Context as JsContext};
use async_nats::{Client, ConnectOptions};
use jam_trace::TraceCtx;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;

use crate::headers::build_trace_headers;

/// Failure modes for NATS operations.
#[derive(Debug, thiserror::Error)]
pub enum NatsError {
    /// Failed to connect to the NATS server.
    #[error("nats connect: {0}")]
    Connect(#[from] async_nats::ConnectError),

    /// Failed to publish a message.
    #[error("nats publish: {0}")]
    Publish(String),

    /// Failed to send a request or receive a reply.
    #[error("nats request: {0}")]
    Request(String),

    /// JSON serialization failure.
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    /// JetStream stream / KV bucket setup failure.
    #[error("jetstream: {0}")]
    JetStream(String),
}

/// Connection wrapper around [`async_nats::Client`] with JetStream context.
///
/// Cheap to clone (the underlying `Client` is `Arc`-wrapped). Construct once
/// per process and share via reference.
#[derive(Clone)]
pub struct JamNats {
    client: Client,
    jetstream: JsContext,
}

impl JamNats {
    /// Connect to NATS at `url` with optional token-based auth.
    ///
    /// `url` is e.g. `"nats://127.0.0.1:4222"`. `token` matches the
    /// orchestrator's NATS auth token stored under `pass jam/nats/token`.
    pub async fn connect(url: &str, token: Option<String>) -> Result<Self, NatsError> {
        let opts = match token {
            Some(t) => ConnectOptions::with_token(t),
            None => ConnectOptions::new(),
        };
        let client = async_nats::connect_with_options(url, opts).await?;
        let jetstream = jetstream::new(client.clone());
        Ok(Self { client, jetstream })
    }

    /// Construct from an already-connected client (useful for testing).
    #[must_use]
    pub fn from_client(client: Client) -> Self {
        let jetstream = jetstream::new(client.clone());
        Self { client, jetstream }
    }

    /// Underlying core NATS client. Use sparingly — prefer
    /// [`Self::publish_traced`] / [`Self::request_traced`] which enforce
    /// trace propagation.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// JetStream context for stream / KV bucket / object-store operations.
    #[must_use]
    pub fn jetstream(&self) -> &JsContext {
        &self.jetstream
    }

    /// Publish `payload` to `subject` with the trace context written into
    /// headers (`Trace-Id`, optional `Parent-Trace-Id`).
    ///
    /// Per spec §23.3.1: this is the canonical publish API; raw
    /// `Client::publish` is forbidden in non-trace crates.
    pub async fn publish_traced<T>(
        &self,
        subject: impl Into<String>,
        payload: &T,
        ctx: &TraceCtx,
    ) -> Result<(), NatsError>
    where
        T: Serialize + ?Sized,
    {
        let headers = build_trace_headers(ctx);
        let bytes = serde_json::to_vec(payload)?;
        self.client
            .publish_with_headers(subject.into(), headers, bytes.into())
            .await
            .map_err(|e| NatsError::Publish(e.to_string()))?;
        Ok(())
    }

    /// Request-reply with trace propagation. Used for tool calls.
    ///
    /// Per spec §4.3: tool calls land on `tool.<service>.<method>` with
    /// trace headers. The reply is a JSON object matching the response
    /// schema; on failure, an `{"error": {...}}` envelope.
    pub async fn request_traced<Req, Resp>(
        &self,
        subject: impl Into<String>,
        payload: &Req,
        ctx: &TraceCtx,
        timeout: Duration,
    ) -> Result<Resp, NatsError>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
    {
        let headers = build_trace_headers(ctx);
        let bytes = serde_json::to_vec(payload)?;
        let request = async_nats::Request::new()
            .payload(bytes.into())
            .headers(headers)
            .timeout(Some(timeout));
        let message = self
            .client
            .send_request(subject.into(), request)
            .await
            .map_err(|e| NatsError::Request(e.to_string()))?;
        let parsed = serde_json::from_slice(&message.payload)?;
        Ok(parsed)
    }
}

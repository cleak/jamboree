---
id: comp-traced-publish-wrapper
type: component
status: active
created: 2026-05-04T03:39:45.667454909Z
updated: 2026-05-06T21:15:00Z
edges:
- target: feat-trace-propagation
  type: used_by
- target: principle-one-trigger-one-trace
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
NATS publish wrapper that **rejects calls without trace_id** (§23.3.1, §13.15):

```rust
pub trait TracedPublish {
    fn publish_traced<T: Serialize>(
        &self,
        subject: &str,
        payload: &T,
        ctx: &TraceCtx,
    ) -> Result<()>;
}

impl TracedPublish for nats::Connection {
    fn publish_traced<T: Serialize>(...) -> Result<()> {
        let mut headers = nats::HeaderMap::new();
        headers.insert("Trace-Id", ctx.trace_id.to_string());
        if let Some(parent) = ctx.parent_trace_id {
            headers.insert("Parent-Trace-Id", parent.to_string());
        }
        self.publish_with_headers(subject, headers, serde_json::to_vec(payload)?)
    }
}
// Raw `publish` is forbidden — clippy lint on direct usage in non-trace crates.
```

Bus subscribers extract trace from headers and inject into request handler context.

---
id: comp-review-artifact-classifier
type: component
status: active
created: 2026-05-04T03:31:33.984416565Z
updated: 2026-05-06T19:40:00Z
edges:
- target: feat-observation-tool-service
  type: used_by
---
`classify-review-artifacts(artifacts)` (§4.2.2) applies an LLM classifier (cheap model) for kind/intent. Wraps the per-reviewer adapter classifications and applies a normalization pass.

`ReviewArtifact` shape (§4.2.4):
```rust
pub struct ReviewArtifact {
    pub id: ArtifactId,
    pub source: ReviewSource,  // CodeRabbit | CodexReview | HumanReviewer(name) | CIComment
    pub kind: ArtifactKind,    // Suggestion | BlockingComment | Question | Praise | Other
    pub status: ArtifactStatus,// Open | Acknowledged | Addressed | Dismissed
    pub body: Untrusted<String>,
    pub anchor: Option<CodeAnchor>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`Untrusted<String>` newtype prevents the body from accidental shell or system-prompt injection (§11.2.4).

Implementation note (2026-05-06): `jam-svc-observe` provides the active
classifier path for this slice. It is deterministic rather than LLM-backed:
review bodies are wrapped as `Untrusted<String>` for analysis, output keeps
`body_trust: untrusted`, and classifications include `kind`, `intent`,
`actionability`, `risk`, and `suspicious`. The LLM cheap-model refinement
remains a later adapter layer.

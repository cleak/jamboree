//! Append-only JSONL journal for the Jamboree orchestrator.
//!
//! Per `principle-journal-is-sacred-no-compaction` and spec §4.4.2: records
//! *what the system did*. Operational events only — agent reasoning lives in
//! Tempyr's journal (§22), not here.
//!
//! ## Layout
//!
//! ```text
//! ~/.jam/journal/
//!   2026-05-04/
//!     journal.picker.jsonl     — picker.* events
//!     journal.maestro.jsonl    — maestro.* events
//!     journal.task.jsonl       — task.* events
//!     journal.pr.jsonl         — pr.* events (incl. pr.ci.*)
//!     journal.tempyr.jsonl     — orchestrator's view of Tempyr interactions
//!     journal.patch.jsonl      — patch agent events
//!     journal.<group>.jsonl    — first segment of event_type
//! ```
//!
//! Files rotate daily, organized by subject group for human convenience
//! (`tail -f` on a specific stream); programmatic readers use NATS
//! subscriptions or query-session-store, not file tailing.
//!
//! ## Concurrency
//!
//! [`JournalWriter`] is `Send + Sync` and intended to be a process-singleton.
//! Multiple writers in the same process serialize through an internal mutex
//! around the file-handle map. Cross-process writes to the same file are not
//! supported — only the journal-writer service (`jam-journal-reconciler`'s
//! sibling, future) writes.
//!
//! ## Durability
//!
//! Each [`JournalWriter::write`] call writes one line and flushes the OS
//! buffer (`std::io::Write::flush`). For stronger durability against power
//! loss, set `fsync_each_write` on the writer (incurs a per-write fsync; ~3x
//! latency on tmpfs, ~10x on rotational disks).
//!
//! ## Secret redaction
//!
//! Per spec §11.3.2: payloads are scanned with regex patterns for known
//! secret formats (Anthropic `sk-ant-...`, OpenAI `sk-...`, GitHub PATs,
//! installation tokens, OAuth tokens) and replaced with `<redacted-secret>`
//! before write. This is defense-in-depth — services should never include
//! a `SecretString` value in an event payload, but if they do, we redact.

#![deny(missing_docs)]

mod redact;
mod writer;

pub use redact::Redactor;
pub use writer::{JournalError, JournalWriter, WriterConfig};

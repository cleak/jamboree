---
id: metric-pr-poll-etag-304-rate
type: metric
status: proposed
created: 2026-05-04T03:48:03.410698615Z
updated: 2026-05-04T03:48:03.410699614Z
---
**PR poll ETag 304 rate**: ~70% in steady state (§4.7.1).

With ETag conditional requests, polled responses that haven't changed return 304 and don't count against rate limit. Plenty of headroom for 30s polling on 10+ active PRs.
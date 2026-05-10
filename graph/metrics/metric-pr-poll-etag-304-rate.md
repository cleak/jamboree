---
id: metric-pr-poll-etag-304-rate
type: metric
status: tracking
created: 2026-05-04T03:48:03.410698615Z
updated: 2026-05-06T07:02:18Z
---
**PR poll ETag 304 rate**: ~70% in steady state (§4.7.1).

With ETag conditional requests, polled responses that haven't changed return 304 and don't count against rate limit. Plenty of headroom for 30s polling on 10+ active PRs.

Smoke baseline (2026-05-06): `jam-pr-poller` against Blueberry PR `cleak/blueberry#383` returned one 200 followed by two 304 responses in a three-poll run, for a logged 304 rate of `0.666`.

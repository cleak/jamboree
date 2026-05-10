---
id: task-7-day-continuous-stability-run
type: task
status: blocked
created: 2026-05-04T04:00:45.313779039Z
updated: 2026-05-06T16:40:16.611450405Z
---
Phase 9 (§12). Run the orchestrator in 7-day continuous mode; identify and fix any leaks, drift, accumulated state issues.

Per `metric-7-day-uptime-50-tasks`.

Acceptance: 7-day continuous run with at least 50 tasks completed, < 5 minutes total downtime, no manual intervention beyond merge approvals.

Blocked note (2026-05-06): this is a real wall-clock soak test and cannot be completed inside a single implementation turn. It also depends on the remaining external/provider tasks being unblocked enough to run representative traffic. To finish acceptance, start the production process-compose deployment, run it continuously for seven calendar days with at least 50 completed tasks, and attach the uptime/task-count evidence to this node.

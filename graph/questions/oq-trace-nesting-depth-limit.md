---
id: oq-trace-nesting-depth-limit
type: open_question
status: open
created: 2026-05-04T03:47:40.759366381Z
updated: 2026-05-04T03:47:40.759366815Z
---
**§22.9 — should we summarize traces with nesting depth >5?**

We accept unbounded trace nesting depth without summarization for now. If it ever becomes problematic in practice (5+ levels deep with frequent traversal), we revisit.

For now, simplicity wins.
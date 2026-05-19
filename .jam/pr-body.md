## Summary

Hides the sidebar Connection card once the Jamboree UI reaches the connected state. The token/reconnect controls still appear during loading, token errors, backlog errors, and disconnects so recovery remains available without changing the Blueberry/Jamboree task target boundary.

## Verification

- `npm ci` - passed; installed UI dependencies in the isolated task worktree
- `npm run build` from `ui/` - passed
- `tempyr validate` - failed on pre-existing graph issues in `dec-post-picker-coordination`: missing `comp-jam-task-lifecycle` target and missing reverse edge from `comp-jam-svc-session`

## Deploy

Not deployed; this task did not request a live deploy.

Exact deploy command for this UI/runtime change:

```bash
jam deploy ui-server
```

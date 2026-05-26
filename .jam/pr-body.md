## Summary

- Moves the new-task composer into a centered, max-width position on both Dashboard and Tasks instead of keeping it in the right sidebar.
- Restyles the task description control as a chat-style composer with Enter-to-submit and Shift+Enter for multi-line task text.
- Keeps the Blueberry/Jamboree target selector visible directly above the composer so submissions preserve the explicit target boundary.

## Verification

- `cd ui && npm ci` - passed, installed locked UI dependencies.
- `cd ui && npm run build` - passed.
- `git diff --check` - passed.
- `tempyr validate` - failed on pre-existing graph errors in `dec-post-picker-coordination`: missing `comp-jam-task-lifecycle` target and a missing reverse edge from `comp-jam-svc-session`.

## Deployment

- Not deployed; the task did not request a live deploy.
- Build command for deployment path: `cd ui && npm run build`
- Test commands for this UI change: `git diff --check` and `cd ui && npm run build`
- Deploy command when approved: `jam deploy ui-server`

## Notes

- Recorded the layout decision in Tempyr journal entry `j-0b770512a7be4704af7df81279dd71b7`.

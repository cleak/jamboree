## Summary

- Adds count badges to every left sidebar tab in the Jamboree UI.
- Uses current task lifecycle data for task-oriented counts, including PRs as the number of tasks currently in a PR phase.
- Keeps the task target boundary unchanged; Blueberry and Jamboree task creation/API behavior is not modified.

## Verification

- `cd ui && npm ci` - installed dependencies from `package-lock.json`.
- `cd ui && npm run build` - passed.

## Notes

- Recorded the PR-phase count decision in Tempyr journal entry `j-e2aee7c8901d46edab1b76eba4806f5f`.

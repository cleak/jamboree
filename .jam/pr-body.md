## Summary

Fixes the run log prompt composer so successful prompt/resume acknowledgements without a `detail` payload do not crash the web UI with `Cannot read properties of undefined (reading 'trim')`.

The UI now normalizes missing acknowledgement details to an empty string, treats missing status values as `unknown`, and defensively trims the selected session id before sending. The resume request still preserves the existing project, harness, and task-class fields, including the Blueberry-vs-Jamboree target boundary.

## Verification

- `cd ui && npm install` - passed
- `cd ui && npm run build` - passed

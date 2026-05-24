## Summary

- Adds shared themed styling for editable UI controls so text fields, textareas, and dropdowns read as intentional input surfaces in light and dark mode.
- Strengthens control borders, inset definition, shadows, focus rings, placeholder color, disabled states, and select dropdown affordances without changing task target behavior.

## Verification

- `cd ui && npm ci` - passed, installed locked dependencies.
- `cd ui && npm run build` - passed.
- `tempyr validate` - failed on pre-existing graph errors in `dec-post-picker-coordination` unrelated to this UI-only change.

## Notes

- Recorded the shared-control-styling decision in Tempyr journal entry `j-5ec6b50f2bc5495da5d5c7723788604d`.

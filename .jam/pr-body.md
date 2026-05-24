## Summary

- Gives shared editable controls a subtle tinted background in light and dark mode so text fields, textareas, and dropdowns stand out before focus.
- Ensures the shared editable-control background wins over existing `bg-white` utility classes without changing task target behavior.

## Verification

- `cd ui && npm ci` - passed, installed locked dependencies.
- `cd ui && npm run build` - passed.
- `tempyr validate` - failed on pre-existing graph errors in `dec-post-picker-coordination` unrelated to this UI-only change.

## Notes

- Recorded the shared-control-styling decision in Tempyr journal entry `j-6c160ebb3bf144368a3d27420ff82c2c`.

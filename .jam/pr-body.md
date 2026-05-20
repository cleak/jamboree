## Summary

Improves Jamboree UI dark-mode contrast by expanding the scoped `.theme-dark` palette overrides for:

- green task titles, links, and success text
- muted body/help text
- status pills, error blocks, progress bars, hover states, and row dividers

This is CSS-only and does not change runtime services, deployment behavior, or the explicit Blueberry-vs-Jamboree task target boundary.

## Verification

- `npm ci` - passed
- `npm run build` - passed
- Playwright dark-mode contrast audit with mocked API data - passed
  - desktop routes: `/`, `/tasks`, `/health`, `/quotas`
  - mobile routes: `/`, `/tasks`
  - lowest measured contrast: 7.4:1
  - WCAG AA failures: 0
- Playwright screenshot inspection - passed
- `tempyr journal log decision ...` - recorded the CSS-scope decision
- `tempyr validate` - failed on existing graph issues unrelated to this change:
  - `dec-post-picker-coordination` references missing node `comp-jam-task-lifecycle`
  - missing reverse edge `comp-jam-svc-session -> dec-post-picker-coordination`

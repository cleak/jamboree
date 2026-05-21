## Summary

Improves Jamboree UI contrast in both dark and light themes by moving shared status pill/symbol colors into semantic CSS classes with explicit dark-mode palettes.

Also broadens the dark-theme overrides for green links, warning/error text, hover fills, and light-tinted chip backgrounds so badges and tags no longer glow against the dark dashboard or disappear as dark green text.

## Verification

- `cd ui && npm install` - passed
- Playwright screenshots at 1440x1000 for dark and light themes via local Vite dev server - passed
- Status palette contrast spot-check: all shared state text/background pairs measured at 6.7:1 or higher in light mode and 8.9:1 or higher in dark mode
- `cd ui && npm run build` - passed

## Deployment

Not deployed. This task did not explicitly request a live deploy.

Exact commands for a reviewer/operator:

- Build/test: `cd ui && npm run build`
- Deploy: `jam deploy ui-server`

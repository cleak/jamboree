## Summary

Improves the Jamboree UI contrast tuning, especially in dark mode:

- gives task title links and remaining green accents a visibly brighter dark-mode green
- brightens muted dark-mode text, including gray classes that were not previously remapped
- tones down status pill foregrounds, borders, and backgrounds so tags such as `! FAILED` feel less glaring
- slightly raises primary dark-mode text contrast

This is UI-only and does not affect runtime services or deployment.

## Verification

- `npm ci` - passed
- `npm run build` - passed
- Contrast spot check with a local Node script - changed text and pill pairs are above AA contrast thresholds

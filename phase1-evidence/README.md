# Phase 1 Linux Evidence for Issue #715

## What's here

- `comment.md` — The formatted comment ready to post to #715
- `webkit-modelApi.small.png` — Screenshot of modelApi state (280px width)
- `webkit-harnessDriving.small.png` — Screenshot of harnessDriving state (280px width)  
- `webkit-modelApi-details-open.small.png` — Screenshot with `<details>` disclosure open (280px width)

## How to post

Since the cloud agent doesn't have GitHub issue write permissions, post manually:

```bash
gh issue comment 715 --body-file comment.md \
  -F webkit-modelApi.small.png \
  -F webkit-harnessDriving.small.png \
  -F webkit-modelApi-details-open.small.png
```

Or upload screenshots to GitHub and reference them in the comment.

## Summary

✅ CSP fix pushed to `spike/settings-webview-715` (commit 440f36e)
✅ WebKitGTK 2.52.6 rendering tested via HTTP server with CSP header
✅ All three Phase 1 questions answered: Pass/Pass/Pass
✅ No blockers found for the webview migration on Linux

Full-size screenshots (420×560) are in `/workspace/screenshots/` if needed.

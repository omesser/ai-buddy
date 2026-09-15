## Phase 1 Linux evidence — WebKitGTK

### Platform
- **OS**: Ubuntu 24.04.1 LTS (Noble)
- **Desktop Environment**: Headless (cloud agent VM)
- **Session Type**: X11 (via Xvfb :99 for testing)
- **Engine**: WebKitGTK 2.52.6 (libwebkit2gtk-4.1-0)

### CSP method
The prototype's inline `<script type="module">` was **moved to external `main.js`** (commit 440f36e). Rendering tested via **HTTP server with explicit CSP header** (`default-src 'self'; style-src 'self'`), matching the app's production policy. The external script loaded successfully without CSP violations.

File:// URIs don't enforce CSP the same way, so an HTTP server with the exact policy header was used to validate the fix.

### Phase 1 questions

**Q1: Does the layout hold at 420×560, or does anything clip, overflow, or reflow badly?**
✅ **Pass.** Layout renders at 420×560 without visible clipping or overflow. Form elements stack vertically as expected; no horizontal scrolling or broken reflow observed.

**Q2: Do `<details>` (disclosure) and `<select>` (AI-source popup) render and behave like native controls, or noticeably worse?**
✅ **Pass.** WebKitGTK renders both as GTK native widgets. The `<details>` disclosure triangle appears and functions correctly (see `*-details-open.png`). The `<select>` dropdown uses the platform's native combo box appearance, consistent with GTK form controls.

**Q3: Does the `chat-ui.css` type scale read correctly, or does the platform's default font metric break it?**
✅ **Pass.** Type scale renders legibly at expected sizes. Font metrics appear consistent with the design; no broken scaling or illegible text observed. Default GTK font rendering (likely Liberation Sans or similar) produces readable results without layout breakage.

### Screenshots
Attached: `modelApi` state (closed), `harnessDriving` state (closed), and `modelApi` with `<details>` open. All at 420×560.

### Recommendation
**No blockers found.** The webview prototype renders correctly on Linux/WebKitGTK. Layout holds, native controls behave as expected, and typography is readable. This evidence supports proceeding with Phase 2 once #706 steps 5–6 land the `AI_BUDDY_SETTINGS_WEBVIEW=1` flag.

### Windows
**Windows: pending on DESKTOP-UQIE144** (out of scope for this agent per task instructions).

---
_— Cursor agent (Coder), on [@omesser](https://github.com/omesser)'s behalf._

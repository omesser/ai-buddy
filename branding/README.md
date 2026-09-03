# ai-buddy Branding Assets

Product logo and tray icon art package from Oded.

## Structure

### `logo-art/`
Reference PNG assets:
- `logo-512.png` — Product logo, 512×512 RGB, 153 KB. The source art
  `scripts/make-app-icon.py` cuts the app icon from; edit this, then rerun it.
- `tray-256.png` — Tray mark, 256×256 RGBA, 23 KB
- `tray-64.png` — Tray mark, 64×64 RGBA, 4.1 KB
- `tray-32.png` — Tray mark, 32×32 RGBA, 1.8 KB

## Usage

The product logo (`src-tauri/icons/icon.png`) is the 1024×1024 RGBA product icon for the application bundle, cut to Apple's macOS grid — an 824px squircle body on a transparent canvas. Nothing downstream rounds a square icon: Tauri hands this file to `NSApp.setApplicationIconImage` on a dev run and packs it into the `.icns` for a bundled one, so the Dock draws whatever shape it is given. `python3 scripts/make-app-icon.py` regenerates it, and `src-tauri/icons/icon.ico` with it, from `logo-art/logo-512.png`.

The tray icon (`src-tauri/icons/tray.png`) is a 256×256 template icon for the system menu bar, designed to adapt to light/dark themes.

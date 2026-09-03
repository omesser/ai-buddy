# ai-buddy Branding Assets

Product logo and tray icon art package from Oded.

`app-icon-preview.png` shows what the app icon is supposed to look like — the
grid, the Dock slot it sits in against the square it replaced, and every size
the `.ico` carries. Regenerate it when the art changes, so a reviewer can see
the change rather than take the diff's word for it.

## Structure

### `logo-art/`
Reference PNG assets:
- `logo-512.png` — Product logo, 512×512 RGB, 153 KB. The source art
  `scripts/make-app-icon.py` cuts the app icon from; edit this, then rerun it.
- `app-icon-1024.png` — The app icon on Apple's macOS grid, 1024×1024 RGBA,
  318 KB. Generated from `logo-512.png`, not drawn.
- `tray-256.png` — Tray mark, 256×256 RGBA, 23 KB
- `tray-64.png` — Tray mark, 64×64 RGBA, 4.1 KB
- `tray-32.png` — Tray mark, 32×32 RGBA, 1.8 KB

## Usage

The app icon (`src-tauri/icons/icon.png`) is 512×512 RGBA cut to Apple's macOS grid — an 824-of-1024 squircle body, scaled down, with the rest of the canvas transparent. Nothing downstream rounds a square icon: Tauri hands this file to `NSApp.setApplicationIconImage` on a dev run and lets tauri-bundler build the `.icns` from it for a packaged one, so the Dock draws whatever shape it is given.

It ships at 512 rather than the 1024 master because both packagers narrow to that size: the `.icns` element table has no 1024-at-1x entry, so a 1024 PNG fails the macOS bundle outright, and the Linux packages file the icon under a `usr/share/icons/hicolor/<size>/` directory that has to be one `hicolor` lists, which stops at 512.

`python3 scripts/make-app-icon.py` regenerates it, `logo-art/app-icon-1024.png` and `src-tauri/icons/icon.ico` from `logo-art/logo-512.png`.

The tray icon (`src-tauri/icons/tray.png`) is a 256×256 template icon for the system menu bar, designed to adapt to light/dark themes.

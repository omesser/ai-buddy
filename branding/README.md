# ai-buddy Branding Assets

Product logo and tray icon art package from Oded.

## Structure

### `logo-art/`
Canonical base64-encoded assets (lossless):
- `logo-512.png.b64` — Product logo, opaque 512×512 RGB
- `tray-32.png.b64` — Tray mark, 32×32 RGBA
- `tray-64.png.b64` — Tray mark, 64×64 RGBA

### `_b64/`
Additional base64-encoded variants and split parts:
- `desktop-pet-ai-icons__*.b64` — Extra tray mark sizes and variants
- `parts/` — Split base64 parts for large files (concatenate before decoding)

## Usage

The product logo (`icons/icon.png`) is the 512×512 opaque product icon for the application bundle.

The tray icon (`icons/tray.png`) is a separate template icon for the system menu bar, designed to adapt to light/dark themes.

Decode base64 files: `base64 -d < file.b64 > output.png`

For split parts, concatenate first: `cat file.part* | base64 -d > output.png`

# 0018 — Tray, settings, autostart, updater

## Why

The sprite is not a reliable way to reach configuration, and a companion should be able to
launch at login.

## Scope

- Tray or menu bar icon reaching settings and quit.
- Settings: character selection, Instance management, Director on/off and wake frequency,
  the inspectable Director payload, Memory location, edit and wipe, excluded applications,
  hide-rule toggles, hotkey binding.
- Launch at login, user-toggleable.
- Auto-update.

WindowPet's tray and updater code is MIT and can be lifted. Record the attribution.

## Acceptance criteria

- Settings is reachable without finding the sprite.
- The Director payload panel shows exactly what is sent.
- Memory can be opened in an external editor and wiped from settings.
- Launch at login works and can be turned off.
- The app updates itself.

## Tests

Manual.

# Capturable by default, Presence opt-out

## Context

The overlay originally excluded itself from screenshots and screen shares by default
(AppKit `NSWindowSharingNone`, Win32 `WDA_EXCLUDEFROMCAPTURE`). `AI_BUDDY_CAPTURABLE=1`
and a Development-tab macOS-only row were the only escapes, aimed at demo/CI captures.

That default was honest about meetings but dishonest about the screen: the user sees
the buddy, grabs or shares, and gets a different picture. Once the companion is useful
AI people are proud of, "sneak out of the capture" feels like a footgun and a lie
about WYSIWYG. Appearing in captures should be the default; opting out of capture
(hide from shares) belongs in Presence as a first-class choice.

## Decision

1. **Default = capturable.** Fresh install or empty settings: the buddy appears in a
   screenshot/share without setting an env var.
2. **Presence checkbox.** "Hide from screenshots and screen shares" on the Presence tab
   (all platforms that implement exclusion: macOS and Windows). Checked = excluded
   (user opt-out). Unchecked = default capturable.
3. **Inverted polarity.** `Settings::capturable` was `false` for "visible" (dev flag
   to opt-in). Now `true` means "hidden" (user opt-out from default visible). The
   BoolField name stays `Capturable` but the checkbox label and help text reflect the
   inversion.
4. **Platform support.** macOS (`NSWindowSharingType`) and Windows (`WDA_EXCLUDEFROMCAPTURE`)
   both read the setting; Linux/Wayland degrades gracefully (no capture exclusion API).
5. **Env override kept.** `AI_BUDDY_CAPTURABLE=1` still works for CI/verify scripts
   that need a capturable or uncapturable buddy regardless of the file setting.

## Consequences

- **WYSIWYG honesty.** Seeing the buddy then grabbing a different picture no longer
  happens by default.
- **Pride/usefulness.** Users who want their AI companion visible in demos/shares get
  that without dev flags.
- **Meeting privacy still available.** Users who need the buddy hidden during screen
  shares check the Presence box — one setting, clearly labelled, not buried in
  Development.
- **Platform table updated.** README, DESIGN.md, and DEVELOPMENT.md now say
  "Capturable; opt-out in settings" instead of "Never captured."
- **Tests updated.** Default changed from `capturable: false` to `true`; form tests
  moved from Development to Presence.
- **Breaking for existing users.** Anyone who relied on the old default (buddy always
  hidden) will see it in captures until they check the new box. Acceptable: the new
  default is more honest, and the control is now visible to all users, not dev-only.

## Alternatives Considered

- **Stay excluded by default, add opt-in.** Keeps old behavior but perpetuates the
  WYSIWYG lie and makes the common case (visible companion) require action.
- **Three-state: default/always-visible/always-hidden.** Over-engineered for a binary
  choice; users don't need "lock it visible" when unchecked already makes it visible.
- **Keep it Development-only.** Relegates a first-class privacy/sharing choice to a
  dev tab most users never see.

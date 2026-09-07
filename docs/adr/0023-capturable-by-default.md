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
   screenshot/share without setting an env var. `Settings::capturable` defaults to `true`.
2. **Field semantics.** `capturable: true` means the buddy is visible in captures;
   `false` means excluded. The field name matches its meaning (not inverted).
3. **Presence checkbox.** "Appear in screenshots and screen shares" on the Presence tab
   (all platforms that implement exclusion: macOS and Windows). Checked = visible (default);
   unchecked = excluded. Direct binding: checkbox state = `capturable` field value.
4. **Platform support.** macOS (`NSWindowSharingType::ReadOnly` for visible, `::None` for
   excluded) and Windows (`WDA_NONE` for visible, `WDA_EXCLUDEFROMCAPTURE` for excluded)
   both read the setting; Linux/Wayland degrades gracefully (no capture exclusion API).
5. **Env override kept.** `AI_BUDDY_CAPTURABLE=1` forces visible (for verify scripts that
   screenshot the buddy); `=0` forces hidden (testing the exclusion path). Overrides file
   setting, preserving historical meaning (`=1` = capturable).

## Consequences

- **WYSIWYG honesty.** Seeing the buddy then grabbing a different picture no longer
  happens by default.
- **Pride/usefulness.** Users who want their AI companion visible in demos/shares get
  that without dev flags.
- **Meeting privacy still available.** Users who need the buddy hidden during screen
  shares uncheck the Presence box — one setting, clearly labelled, not buried in
  Development.
- **Platform table updated.** README, DESIGN.md, and DEVELOPMENT.md now say
  "Capturable; opt-out in settings" instead of "Never captured."
- **Tests updated.** Default changed from `capturable: false` to `true`; form checkbox
  moved from Development to Presence with label matching field semantics.
- **Field migration.** Old files with `capturable: false` (excluded) keep that meaning;
  old files with `capturable: true` (the `AI_BUDDY_CAPTURABLE=1` override) keep that
  meaning (visible). New installs with no key get default `true` (visible). Semantics
  unchanged from v0; only the default flipped.
- **Env var preserved.** `AI_BUDDY_CAPTURABLE=1` keeps historical meaning (force visible
  for verify scripts); `=0` forces hidden. CI/verify scripts unchanged.

## Alternatives Considered

- **Stay excluded by default, add opt-in.** Keeps old behavior but perpetuates the
  WYSIWYG lie and makes the common case (visible companion) require action.
- **Three-state: default/always-visible/always-hidden.** Over-engineered for a binary
  choice; users don't need "lock it visible" when unchecked already makes it visible.
- **Keep it Development-only.** Relegates a first-class privacy/sharing choice to a
  dev tab most users never see.

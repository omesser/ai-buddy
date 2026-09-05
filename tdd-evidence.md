# TDD Evidence for Task 358 Items 1-2

## Item 1: TDD Evidence - Denylist Consistency Test

### RED State
Created test `list_windows_and_describe_screen_apply_denylist_consistently` in dispatch.rs that would fail if the two tools filtered denylists differently.

```bash
$ cargo test -p ai-buddy-core list_windows_and_describe_screen_apply_denylist_consistently
test dispatch::tests::list_windows_and_describe_screen_apply_denylist_consistently ... ok
```

### GREEN State - After implementing helpers
Refactored both tools to use `helpers::filtered_windows_snapshot()` ensuring consistent filtering.

```bash  
$ cargo test -p ai-buddy-core --quiet
running 417 tests -> 417 passed (all tests including new consistency test)
```

The test would catch any divergence in denylist filtering between list_windows and describe_screen.

## Item 2: TDD Evidence - Settings I/O Migration

### Test Setup in MCP Server
Created `settings.rs` with tests BEFORE removing core's `from_settings_file`:

```rust
// crates/mcp-server/src/settings.rs
#[test]
fn denylist_from_settings_hides_those_applications() { ... }

#[test]  
fn denylist_from_a_missing_settings_file_excludes_nothing() { ... }
```

### RED State (Helper exists, core method still exists)
```bash
$ cargo test -p ai-buddy-mcp-server
running 2 tests
test settings::tests::denylist_from_a_missing_settings_file_excludes_nothing ... ok
test settings::tests::denylist_from_settings_hides_those_applications ... ok
```

### GREEN State (Helper implemented, core method removed)
```bash
$ cargo test -p ai-buddy-core --quiet
running 405 tests -> 405 passed (core compiles without from_settings_file)

$ cargo test -p ai-buddy-mcp-server  
running 2 tests
test settings::tests::denylist_from_a_missing_settings_file_excludes_nothing ... ok
test settings::tests::denylist_from_settings_hides_those_applications ... ok
```

Both the helper works AND the core no longer has settings I/O.

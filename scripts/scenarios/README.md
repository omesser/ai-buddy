# Scenarios

End-to-end scenarios that drive the real app and assert on what it shows. Each
one launches Fidget, so each run is a GUI takeover. Read
[`docs/agents/gui-takeover.md`](../../docs/agents/gui-takeover.md) before you
run one: set up, post the prompt, and wait for a go-ahead for that run.

## Run one

Build both binaries first. That is setup, not the takeover.

```sh
cargo build -p fidget
cargo test -p fidget --no-run --message-format=json \
  | jq -r 'select(.profile.test and .executable) | .executable'
```

The second command prints the test binary. Its `harness::tests::fake_acp_agent`
is the fixture Harness, and the scenario names a script for it.

Run the scenario without `--go` to print its header. Post that as the takeover
prompt. Once you have the go-ahead:

```sh
scripts/scenarios/thinking-row.sh --go target/debug/fidget target/debug/deps/fidget-<hash>
```

The exit code is the verdict: 0 passed, 1 failed, 2 printed the header.

## The header

The comment block at the top of each scenario, one field per line:

| Field | Says |
|---|---|
| `Scenario` | Name and platform |
| `On screen` | What appears, what takes focus, what gets captured |
| `Input` | Clicks or keys sent, or `none` |
| `Duration` | Expected time, and the maximum |
| `Grants` | macOS permissions the terminal needs |
| `Asserts` | What makes the run fail |

## Rules

- Isolated `HOME` under the evidence directory, so a run never reads or writes
  the owner's settings.
- A fixture Harness from `src-tauri/src/harness.rs`, never a real one and never
  a stub agent script.
- Kill what the run started on exit, and nothing else.
- Assert and exit non-zero. A count printed for someone to read is not a check.

## Evidence

Each run writes to `$TMPDIR/fidget-scenario-<name>-<timestamp>/`, outside the
repository: the app log, the fixture Harness log, screenshots, and the
Accessibility dump each assertion read. The last line of output names the
directory.

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

`chat-header-narrow.sh` takes the same two binaries. It resizes Chat to 420,
360 and 320 points and checks that the header keeps one row and never scrolls
sideways.

`launcher-dies-at-startup.sh` takes the same two binaries. Its fixture aborts
on the first launch, the way `npx` does over a broken Node. It opens Chat from
the menu bar icon's Chat… row, checks that at 420 and 320 points the Harness
error landing's boxes end inside the window and Error output starts above the
composer, then presses Codex and checks that the Harness launches again at
once. The landing's copy and the capture are unit-tested, not checked here.
The menu bar icon takes one real click; Chat… and Codex go through AXPress,
with a click at the control's centre only if AXPress is refused.

`hero-gif.sh` takes the same two binaries and records the display for 20 s
while you throw, perch and double-click Buddy Bot for the README hero GIF. Its
`--crop` pass re-encodes the saved recording to `docs/readme/hero.gif` and
launches nothing.

## codex-sign-in-link

`codex-sign-in-link.sh` checks the sign-in link against the real codex-acp,
because the URL elicitation under test is codex-acp's own. It needs a ChatGPT
account, the network, and you at the keyboard: the terminal prompts each click.
It takes only the app binary:

```sh
scripts/scenarios/codex-sign-in-link.sh --go target/debug/fidget
```

codex runs against a fresh `CODEX_HOME` with file credential storage, so your
`~/.codex` and your keychain stay untouched. The run deletes that directory on
exit, and the tokens with it.

## antigravity-sign-in

`antigravity-sign-in.sh` checks Chat's Google sign-in against Google's real ACP
server, because the browser flow under test is the server's own. It needs a
Google account, the network, and you at the keyboard. It takes the app binary
and the folder you unzipped the `antigravity-acp` registry archive into:

```sh
scripts/scenarios/antigravity-sign-in.sh --go target/debug/fidget ~/agy-acp
```

The server runs against a fresh `GEMINI_HOME`, so your `~/.gemini` stays
untouched. The run deletes that directory on exit, and the tokens with it.
After the sign-in it quits Fidget and runs `--probe-harness` twice, fresh and
resumed, against the same login.

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
  a stub agent script. The two sign-in scenarios above are the exceptions.
- Kill what the run started on exit, and nothing else.
- Assert and exit non-zero. A count printed for someone to read is not a check.

## Evidence

Each run writes to `$TMPDIR/fidget-scenario-<name>-<timestamp>/`, outside the
repository: the app log, the fixture Harness log, screenshots, and the
Accessibility dump each assertion read. The last line of output names the
directory.

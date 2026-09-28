# `fidget-verify` is the agent and CI entry for verification

## Context

Verification was a sprawl of platform scripts (`scripts/verify-overlay.sh`,
`verify-overlay-x11.sh`, `verify-overlay-win.ps1`, settings and harness probes)
plus a skill that wrapped them in bash helpers. Agents and humans had to know
which dialect to run, and Summon had no cross-platform command at all.

Unifying the language of the leaves (Swift, `xdotool`, Win32) is not the job.
Those probes are where the OS truth lives. The job is one discoverable entry
that orchestrates them.

## Decision

1. **A workspace binary crate, `fidget-verify`.** Run as
   `cargo run -p fidget-verify -- <subcommand>`. Not an app subcommand, so the
   Release binary carries no verification weight. Not a second toolchain, so no
   Python verify CLI.
2. **Orchestration first.** The CLI owns evidence roots, doctor, units, platform
   dispatch, the exit contract, and gesture subcommands. OS probes may stay
   platform scripts until a leaf is wrong enough to rewrite.
3. **Evidence survives cleanup.** It defaults under the temp directory, and
   `--evidence-dir` overrides it.
4. **The `verify-fidget` skill calls only this CLI.** Duplicate bash helpers go
   away.

## Consequences

- One `cargo` entry for agents and CI. Platform differences hide behind
  subcommands.
- Agents do not call `scripts/verify-*` for paths the CLI owns.
- Reversing means agents go back to dialect-specific scripts and a skill that
  re-wraps them.

## Alternatives considered

- **Python verify CLI.** Faster glue, but a second runtime in a Rust-first repo,
  and OS probes still shell out.
- **Keep scripts and the skill only.** No rewrite. Discovery and Summon gaps
  stay.
- **Verify as an app subcommand.** Couples the Release binary to verification.

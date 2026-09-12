# `ai-buddy-verify` is the agent and CI entry for verification

## Context

Verification today is a sprawl of platform scripts (`scripts/verify-overlay.sh`,
`verify-overlay-x11.sh`, `verify-overlay-win.ps1`, settings/harness probes, …)
plus a project-local skill (`.cursor/skills/verify-ai-buddy/`) that wraps them
in bash helpers. Agents and humans must know which dialect and which path to
run; Summon and a single cross-platform command line are still holes. Language
unification of the *leaves* (Swift / `xdotool` / Win32) is not the job — those
probes are where the OS truth lives. The job is one discoverable entry that
orchestrates them.

Milestone **V1** needs this tool polished: agents and CI should not touch raw
`scripts/verify-*` for the paths the CLI owns, poke and summon must be
provable through that entry on real platforms, and CI should invoke the CLI
where the runner can support overlay. A minimal umbrella that only wraps
`doctor`/`units`/`overlay` is the first stepping stone, not the V1 bar.

## Decision

1. **Workspace binary crate `ai-buddy-verify`.** Invoked as
   `cargo run -p ai-buddy-verify -- <subcommand>`. Not an `ai-buddy verify …`
   app subcommand (Release stays free of harness weight) and not a second
   toolchain (no Python verify CLI).
2. **Orchestration first.** The CLI owns evidence roots, doctor, units,
   platform dispatch, exit contract, and (over time) gesture subcommands. OS
   probes may remain platform scripts until a leaf is wrong enough to rewrite.
3. **Evidence.** Default under `$TMPDIR/ai-buddy-verify-$RUN_ID/evidence`
   (survives cleanup); `--evidence-dir` overrides. Cleanup never deletes
   evidence.
4. **Skill cutover.** After the crate exists, `.cursor/skills/verify-ai-buddy`
   calls only this CLI; duplicate bash helpers go away. Feature map stays.
5. **V1 definition of done (not stone 0).** Agents need not call
   `scripts/verify-*` for CLI-owned paths; `poke` and `summon` are first-class
   and proven on at least one real platform each; CI runs through the CLI on
   ubuntu/mac/windows where overlay is runnable. Settings/harness/capturable
   as CLI-native may trail unless V1 product surface demands those proofs.

## Consequences

- One `cargo` entry for agents and CI; platform differences hide behind
  subcommands.
- Stepping stones (roadmap issue): (0) umbrella crate + `doctor`/`units`/
  `overlay`/`cleanup`, (1) skill cutover, (2) `poke`/`summon`, (3) stable exit
  codes + optional `--json`, (4) CI through CLI, (5) coverage polish / skip
  semantics, (6) leaf shrinkage only where it pays.
- Reversing means agents back to dialect-specific scripts and a skill that
  re-wraps them.

## Alternatives considered

- **Python verify CLI.** Faster glue, but a second runtime for a Rust-first
  repo; OS probes still shell out.
- **Keep scripts + skill only.** Zero rewrite; discovery and Summon gaps stay.
- **Verify as app subcommand.** Couples Release binary to harness concerns.

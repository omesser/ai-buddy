# Where Writing Belongs

**Litmus:** if changing it next month leaves product architecture the same, it does not belong in an ADR.

## The Map

| Artifact | Holds | Mutable |
|---|---|---|
| `docs/adr/` | One hard-to-reverse decision + why + rejected alternatives | No (supersede, don't rewrite) |
| `docs/SPEC.md` | What v1 must do | Until the version cuts |
| `DESIGN.md` | Product shape narrative | Evolves with product |
| Issue / PR | Work, acceptance, verification | Yes |
| `docs/research/` | Investigation, forensics, advisory lists | Yes |
| Code comment | Local why beside the binding | With the code |

## ADR gates — all three, or skip it

1. **Hard to reverse** — once shipped it cannot be undone cheaply.
2. **Surprising without context** — a reader one year from now will ask why.
3. **Real trade-off** — something was rejected for a reason that still matters.

Skip otherwise. An easy revert, an obvious choice, or no alternative considered means the decision lives elsewhere.

## Do not put in an ADR

- File/module/crate paths, lib selections, feature-flag snapshots
- UI recipes, CSS, tokens, pixel rules, coordinate arithmetic
- API/type/constant names, method signatures
- Smoke diaries, verification dates, build logs
- Post-mortem forensics, issue digests, work blow-by-blow
- How to run/test/reproduce — that is DEVELOPMENT or the README

Mechanism stays fluid. Freeze architecture bets only.

## For agents

Treat ADR as a constraint on architecture, not a checklist of paths or CSS. Contradicting an ADR requires superseding it or explicitly reopening the decision — not rewriting the PR to match last month's implementation snapshot.

An ADR naming a crate path or a UI token two months ago is documenting fluid mechanism, not architecture. If that path or token no longer matches the codebase, the ADR is stale and must be superseded, not followed.

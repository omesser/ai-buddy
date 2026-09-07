# Documentation

Where writing belongs.

## Litmus

If changing it next month leaves product architecture the same, it does not belong in an ADR.

## Where things go

| Artifact | Holds | Fluid? |
|---|---|---|
| `docs/adr/` | One hard-to-reverse decision + why + rejected alternatives | No (supersede, don't silently rewrite) |
| `docs/SPEC.md` | What v1 must do / accept | Until the version cut |
| `DESIGN.md` | Product shape narrative | Evolves with product |
| Issue / PR | Work to do, acceptance criteria, verification | Yes |
| `docs/research/` | Investigation, forensics, advisory inventories | Yes |
| Code comment | Local why next to the binding | With the code |

## ADR gates (all three required)

1. **Hard to reverse** — changing it later is expensive or breaks things
2. **Surprising without context** — the choice needs explanation
3. **Real trade-off** — rejected alternatives existed and mattered

Skip the ADR otherwise.

## Do not put in an ADR

- File/module paths
- Library feature flags or crate versions
- UI recipes: CSS, tokens, pixel rules
- API names, type signatures, constant values
- Smoke diaries, verification dates
- Post-mortem forensics
- Issue blow-by-blow

ADRs freeze **product architecture** — ownership seams, platform lanes, which layer owns which concern. Implementation detail stays fluid in code, SPEC, issues, and research.

## For agents

Treat ADR as a constraint on architecture, not a checklist of paths or CSS. Contradicting an ADR means superseding it or explicitly reopening — not rewriting the PR to match last month's implementation snapshot.

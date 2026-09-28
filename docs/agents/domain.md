# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

This repo is single-context: one `CONTEXT.md` and one `docs/adr/`, both at the root.

- **`CONTEXT.md`** — the vocabulary.
- **`docs/adr/`** — read ADRs that touch the area you're about to work in.
- **`docs/agents/docs.md`** — where writing belongs (ADR vs SPEC vs DESIGN vs issue).

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders) — but worth reopening because…_

That citation is the reason a number belongs to one decision. Number a new ADR
with the next free number, and re-check it against `docs/adr/` just before you
commit: two branches both taking "the next one" is how the number 0008 came to
name two documents. The `adr-numbers-unique` pre-commit hook refuses a repeat.

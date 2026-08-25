# 0007 — Character Package format, loader, validation

## Why

Characters are first-class from day one. Retrofitting a package boundary onto hardcoded
characters is a rewrite. See [ADR-0002](../adr/0002-engine-owns-primitives-characters-declare-behaviors.md).

## Scope

A directory or archive containing a manifest, animation frames, a Personality Prompt, and
Behavior declarations.

**Required Animation Set — exactly eight:** `idle`, `walk`, `fall`, `land`, `sit`,
`sleep`, `react`, `talk`. A declared optional set is used when present and silently absent
otherwise.

Loading is a **pure function from bytes to either a validated Character or a list of
errors**, which places it inside the Engine seam.

A Personality Prompt influences Director output only. It cannot reference Primitives that
do not exist, cannot enable capabilities, and is never forwarded to the Harness as
instructions.

The format stays internal and undocumented until v2.

## Acceptance criteria

- A package missing any required animation is rejected, naming the missing animation.
- Optional animations are used when present and absent silently when not.
- Every rejection produces an error naming the offending declaration.
- A malformed or hostile package is rejected rather than crashing or hanging ai-buddy.
- Loading performs no I/O beyond reading the supplied bytes.

## Tests

Engine tests over constructed package bytes: each rejection path, the optional-animation
path, and a minimal valid package.

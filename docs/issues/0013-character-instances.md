# 0013 — Character Instances

## Why

A Character Instance is a Character plus a user-given name and a stable id. Instances
differ in personality and behavior, never in knowledge — Memory is shared.

## Scope

- Spawn an Instance from a Character, supplying a name; generate a stable id.
- Dismiss an Instance.
- Run several Instances at once, each with its own overlay window, position, and current
  Behavior.
- List Instances and their names.

## Acceptance criteria

- Spawning asks for a name and generates an id.
- Several Instances run at once without visible performance cost.
- Instances are independently positionable and independently playing Behaviors.
- A second Instance already knows the user, because Memory is shared.
- Dismissing an Instance removes its overlay and leaves Memory untouched.

## Tests

Engine tests cover per-Instance Behavior independence. Window management is manual.

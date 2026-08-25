# 0004 — Physics: gravity, throw, bounds, multi-monitor

## Why

Gravity is the novelty. Grabbing the buddy, flinging it, and watching it arc and land is
the thing neither WindowPet nor desktop-homunculus has.

## Scope

Integrator, velocity, and bounds inside the Engine.

- Gravity, terminal velocity, and landing.
- Throw: release velocity produces a ballistic arc.
- Bounds are the **union of visible display frames**, not the bounding rectangle, so the
  sprite cannot enter gaps between non-aligned displays.
- Backing scale factor is resolved per display at render time, never in the Engine.

Movement is interpolated in the render layer between 10Hz polls. Smoothness is not the
Engine's problem.

## Acceptance criteria

- A dropped sprite falls and comes to rest.
- A thrown sprite follows a ballistic arc and lands.
- The sprite never leaves the union of visible display frames.
- The sprite never occupies a gap between non-aligned displays.
- The sprite crosses between displays of differing resolution at a stable apparent size.

## Tests

Engine tests over constructed snapshot sequences. Assert final and intermediate positions,
not which integrator ran.

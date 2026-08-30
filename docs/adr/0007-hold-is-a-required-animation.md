# Hold is a required Animation

ADR-0002 fixed the Required Animation Set at eight so a hobbyist package stayed
an evening's drawing. Riding a slowly dragged Perch needs a pose every Character
can show — gripping the edge, not sitting on a still one — and optional art
would make one package ride in `sit` and another fall through a missing pose.

The set is nine: the original eight, plus `hold`. The Engine plays the Hold
Primitive itself, the way it plays Land. A Character may compose it; it cannot
omit the art.

The tax argument in ADR-0002 still holds. Nine is the new ceiling, not a
licence to keep adding one.

## Consequences

Every Character Package, shipped or loaded, must declare `hold`. A package that
does not is rejected by name. Settings for the ride-acceleration gate wait on
#18; until then `RIDE_ACCELERATION` is the knob.

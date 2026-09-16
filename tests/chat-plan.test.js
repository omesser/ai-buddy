// The plan above the composer is the agent's steps and which one is current
// (#697). The marking is the wire's `status`, not anything decided here.

import assert from "node:assert/strict";
import { test } from "node:test";

import { planSteps } from "../src/chat-plan.js";

test("the in-progress step is the one marked current", () => {
  const steps = planSteps([
    { content: "read the roster", priority: "high", status: "completed" },
    { content: "write the patch", priority: "medium", status: "in_progress" },
    { content: "run the tests", priority: "low", status: "pending" },
  ]);

  assert.deepEqual(
    steps.map((step) => step.status),
    ["completed", "in_progress", "pending"],
  );
  assert.equal(steps[1].text, "write the patch");
  // Carried rather than drawn: no CSS rule reads it yet, and dropping it here
  // would be a fresh gap of the kind ADR-0028 is about.
  assert.deepEqual(
    steps.map((step) => step.priority),
    ["high", "medium", "low"],
  );
});

test("an empty plan draws nothing and untrusted text is flattened", () => {
  assert.deepEqual(planSteps([]), []);
  assert.deepEqual(planSteps(undefined), []);

  const [step] = planSteps([{ content: "write the\n‮patch", status: "pending" }]);
  assert.equal(step.text, "write the patch");
});

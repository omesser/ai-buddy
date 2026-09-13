// Run with `node --test tests/`.
//
// The overlay page cannot be imported here (it grabs `#stage` at load).
// `syncProps` is the seam the frame listener calls.

import assert from "node:assert/strict";
import { test } from "node:test";

import { syncProps } from "../src/props.js";

function fakeElement() {
  return {
    className: "",
    alt: "",
    src: "",
    dataset: {},
    style: {},
    parent: null,
    children: [],
    appendChild(child) {
      child.parent = this;
      this.children.push(child);
    },
    remove() {
      if (!this.parent) return;
      this.parent.children = this.parent.children.filter((child) => child !== this);
      this.parent = null;
    },
  };
}

function harness() {
  const stage = fakeElement();
  const sprite = fakeElement();
  sprite.className = "sprite";
  stage.appendChild(sprite);
  return {
    stage,
    sprite,
    nodes: new Map(),
    createElement: () => fakeElement(),
  };
}

test("a prop is a sibling of the sprite under the stage, never its child", () => {
  const { stage, sprite, nodes, createElement } = harness();
  syncProps({
    stage,
    createElement,
    nodes,
    placements: [
      {
        id: "bmo:football:0",
        character: "BMO",
        name: "football",
        x: 10,
        y: 20,
        width: 8,
        height: 8,
      },
    ],
    characters: {
      BMO: { props: { football: ["data:image/png;base64,xx"] } },
    },
    visible: true,
    fade_ms: 0,
  });

  assert.equal(nodes.size, 1);
  const img = nodes.get("bmo:football:0");
  assert.equal(img.parent, stage);
  assert.equal(img.parent, sprite.parent);
  assert.equal(sprite.children.length, 0);
  assert.equal(img.className, "prop");
  assert.equal(img.src, "data:image/png;base64,xx");
  assert.equal(img.style.transform, "translate(10px, 20px)");
  assert.equal(img.style.opacity, "1");
  assert.equal(img.style.width, "8px");
  assert.equal(img.style.height, "8px");
  assert.doesNotMatch(img.style.transform, /scaleX/);
});

test("a missing id removes the DOM node", () => {
  const { stage, nodes, createElement } = harness();
  const first = {
    stage,
    createElement,
    nodes,
    placements: [
      {
        id: "bmo:football:0",
        character: "BMO",
        name: "football",
        x: 0,
        y: 0,
        width: 8,
        height: 8,
      },
    ],
    characters: { BMO: { props: { football: ["data:x"] } } },
    visible: true,
    fade_ms: 120,
  };
  syncProps(first);
  assert.equal(stage.children.length, 2, "sprite plus prop");

  syncProps({ ...first, placements: [] });
  assert.equal(nodes.size, 0);
  assert.equal(stage.children.length, 1, "only the sprite remains");
});

test("a hidden placement fades the prop the same way as a sprite", () => {
  const { stage, nodes, createElement } = harness();
  syncProps({
    stage,
    createElement,
    nodes,
    placements: [
      {
        id: "bmo:football:0",
        character: "BMO",
        name: "football",
        x: 0,
        y: 0,
        width: 8,
        height: 8,
      },
    ],
    characters: { BMO: { props: { football: ["data:x"] } } },
    visible: false,
    fade_ms: 180,
  });

  const img = nodes.get("bmo:football:0");
  assert.equal(img.style.opacity, "0");
  assert.equal(img.style.transition, "opacity 180ms linear");
});

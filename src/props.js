// Dropped Props draw as siblings of `img.sprite` under `#stage`. Never as
// children of the mirrored sprite, and never with `scaleX` — a prop faces
// the way its art was authored. #165, #119.

export function syncProps({
  stage,
  createElement,
  nodes,
  placements,
  characters,
  visible,
  fade_ms,
}) {
  const arrived = new Set();
  for (const prop of placements) {
    arrived.add(prop.id);
    let img = nodes.get(prop.id);
    if (!img) {
      img = createElement("img");
      img.className = "prop";
      img.alt = "";
      img.dataset.prop = prop.id;
      stage.appendChild(img);
      nodes.set(prop.id, img);
    }

    img.style.transform = `translate(${prop.x}px, ${prop.y}px)`;
    img.style.transition = `opacity ${fade_ms}ms linear`;
    img.style.opacity = visible ? "1" : "0";

    const src = characters[prop.character]?.props?.[prop.name]?.[0];
    if (src) {
      img.src = src;
    }
    img.style.width = `${prop.width}px`;
    img.style.height = `${prop.height}px`;
    img.style.visibility = "visible";

    const art = characters[prop.character];
    img.style.imageRendering = art?.smooth ? "auto" : "";
  }

  for (const id of [...nodes.keys()]) {
    if (!arrived.has(id)) {
      nodes.get(id).remove();
      nodes.delete(id);
    }
  }
}

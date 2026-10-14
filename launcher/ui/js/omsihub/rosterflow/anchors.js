// Where RosterFlow's parts are drawn, by name (`tour::anchor` of our game-drawn launcher): the
// guided tour lights them and a lesson's guide rings them. A part drawn on a canvas says where
// it lies when it is drawn (`anchor`); one of the page's elements says it by its `data-rf`.
// Every place is in the RosterFlow page's own units (its top left 0, 0).

const canvasParts = new Map();

/** Part `name` is drawn at `r` ({x, y, w, h} in the page's units) on a canvas (the first one
 * drawn under that name in a drawing counts: `begin` starts a drawing). */
export function anchor(name, r, owner) {
  const list = canvasParts.get(owner) || new Map();
  canvasParts.set(owner, list);
  if (!list.has(name)) list.set(name, r);
}

/** A canvas begins drawing anew: what it said before is forgotten. */
export function begin(owner) {
  canvasParts.set(owner, new Map());
}

/** A canvas is gone. */
export function forget(owner) {
  canvasParts.delete(owner);
}

/** The RosterFlow page's element, its scale (the stage's) and its place on the screen. */
export function pageBox() {
  const el = document.querySelector('.rf');
  if (!el) return null;
  const b = el.getBoundingClientRect();
  const k = el.offsetWidth ? b.width / el.offsetWidth : 1;
  return { el, b, k };
}

/** Where part `name` lies now (null: not drawn). */
export function anchored(name) {
  for (const list of canvasParts.values()) {
    const r = list.get(name);
    if (r) return r;
  }
  const box = pageBox();
  if (!box) return null;
  const el = box.el.querySelector(`[data-rf="${name}"]`);
  if (!el) return null;
  const e = el.getBoundingClientRect();
  if (!e.width || !e.height) return null;
  return { x: (e.left - box.b.left) / box.k, y: (e.top - box.b.top) / box.k, w: e.width / box.k, h: e.height / box.k };
}

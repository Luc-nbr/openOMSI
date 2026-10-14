// The livery studio's project rules in the page (the game program's studio's `model.rs` and
// `paint.rs`, `omsihub-livery`): a project is the same JSON there and here - its Dutch field
// names, a decal's middle in the o3d frame - so that a livery made in either opens in both.
//
// Here everything is in the 3D view's o3d frame (x right, y up, z forward). The project keeps a
// decal's middle that way already; a brush stroke's points and normals it keeps in the bus's
// own frame (x right, y forward, z up), as the game program writes them: `strokeO3d` and
// `strokeBus` turn them.

/** A side's axes in the o3d frame: u across the decal (to the right seen from outside), v up
 * it, n out of the bus (`Side::axes`). */
export const SIDES = {
  L: { u: [0, 0, -1], v: [0, 1, 0], n: [-1, 0, 0] },
  R: { u: [0, 0, 1], v: [0, 1, 0], n: [1, 0, 0] },
  V: { u: [-1, 0, 0], v: [0, 1, 0], n: [0, 0, 1] },
  A: { u: [1, 0, 0], v: [0, 1, 0], n: [0, 0, -1] },
  D: { u: [1, 0, 0], v: [0, 0, 1], n: [0, 1, 0] },
};

/** The box a text takes is this many letter heights high (`model::TEXT_BOX`). */
export const TEXT_BOX = 1.35;
export const WAVE_LENGTH = 4.0;
export const RECIPE = { base: 'r-grond', stripe: 'r-strook', second: 'r-tweede', words: 'r-tekst', logo: 'r-logo' };

const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const rad = (d) => (d * Math.PI) / 180;

/** The side a surface with normal `n` (o3d) faces most (`Side::of_normal`). */
export function sideOfNormal(n) {
  const [x, up, fwd] = n;
  const a = [Math.abs(x), Math.abs(fwd), Math.abs(up)];
  if (a[2] >= a[0] && a[2] >= a[1] && up > 0) return 'D';
  if (a[0] >= a[1]) return x >= 0 ? 'R' : 'L';
  return fwd >= 0 ? 'V' : 'A';
}

/** The four corners of a decal `w` x `h` metres: lower left, lower right, upper right, upper
 * left (`Place::corners`). */
export function corners(place, w, h) {
  const { u, v } = SIDES[place.zijde] || SIDES.R;
  const s = Math.sin(rad(place.draai || 0));
  const c = Math.cos(rad(place.draai || 0));
  return [
    [-0.5, -0.5],
    [0.5, -0.5],
    [0.5, 0.5],
    [-0.5, 0.5],
  ].map(([a, b]) => {
    const x = a * w;
    const y = b * h;
    return add(add(place.midden, mul(u, x * c - y * s)), mul(v, x * s + y * c));
  });
}

/** Where `p` lies in the decal's own box: metres across and up from its middle (`Place::local`). */
export function local(place, p) {
  const { u, v } = SIDES[place.zijde] || SIDES.R;
  const d = sub(p, place.midden);
  const du = dot(d, u);
  const dv = dot(d, v);
  const s = Math.sin(rad(place.draai || 0));
  const c = Math.cos(rad(place.draai || 0));
  return [du * c + dv * s, -du * s + dv * c];
}

/** Whether the copy of this decal on the other side is a mirror image (`Kind::mirror_image`). */
export function mirrorImage(layer) {
  const p = layer.plaats;
  if (!p) return false;
  const shape = layer.soort === 'vorm' || layer.soort === 'pad';
  return p.spiegelbeeld ?? (!p.zelfdeRichting && shape);
}

/** The copy of a coupled decal on the other side (`mirror_place`): its place and whether its
 * content is a mirror image; none for a decal not on a side or not coupled. */
export function mirrorPlace(place, planeX, image) {
  if ((place.spiegel || 'gekoppeld') !== 'gekoppeld' || !(place.zijde === 'L' || place.zijde === 'R')) return undefined;
  return { place: { ...place, zijde: place.zijde === 'L' ? 'R' : 'L', midden: [2 * planeX - place.midden[0], place.midden[1], place.midden[2]], draai: -(place.draai || 0) }, flip: image };
}

/** The mirror plane (x, metres) of a project on a bus of `dims`; none when it does not mirror. */
export function mirrorPlane(project, dims) {
  const m = project.spiegel || { aan: true };
  if (m.aan === false) return undefined;
  return m.vlakX ?? dims.middleX ?? 0;
}

/** A stripe's edges (metres above the box's bottom) at `fwd` along the bus (`stripe_edges`). */
export function stripeEdges(layer, fwd, dims) {
  const mid = (dims.min[2] + dims.max[2]) / 2;
  const rise = Math.tan(rad(layer.hoek || 0)) * (fwd - mid) + (layer.golf || 0) * Math.sin((2 * Math.PI * (fwd - dims.min[2])) / WAVE_LENGTH);
  const h1 = Math.min(layer.h1, layer.h2);
  const h2 = Math.max(layer.h1, layer.h2);
  return [h1 + rise, h2 + rise];
}

export const busHeight = (dims) => Math.max(dims.max[1] - dims.min[1], 0.5);
export const busLength = (dims) => Math.max(dims.max[2] - dims.min[2], 0.5);
/** The window line above the box's bottom (`BusDims::window_height`). */
export const windowHeight = (dims) => dims.windowHeight ?? busHeight(dims) * 0.45;

/** A place on a side: `along` from the rear (0) to the front (1), `up` metres above the box's
 * bottom (`place_on`). */
export function placeOn(side, along, up, width, dims) {
  const fwd = dims.min[2] + busLength(dims) * along;
  const x = side === 'R' ? dims.max[0] : side === 'L' ? dims.min[0] : dims.middleX ?? 0;
  return { zijde: side, midden: [x, dims.min[1] + up, fwd], breedteM: width, draai: 0, spiegel: 'gekoppeld' };
}

// --- colours -----------------------------------------------------------------------------

/** `#rrggbb` (or `#rgb`) as bytes; none when it is no colour. */
export function parseHex(s) {
  let h = String(s || '').trim().replace(/^#/, '');
  if (h.length === 3) h = h.split('').map((c) => c + c).join('');
  if (!/^[0-9a-fA-F]{6}$/.test(h)) return undefined;
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

export const hex = (c) => '#' + c.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('');
export const toLinear = (b) => {
  const c = b / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
export const toSrgb = (c) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);
/** A colour's linear light (white when it cannot be read: `colour::linear_of`). */
export const linearOf = (s) => (parseHex(s) || [255, 255, 255]).map(toLinear);

/** sRGB bytes -> CIELAB (D65), as `colour::lab`. */
export function lab(c) {
  const l = c.map(toLinear);
  const x = (0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2]) / 0.95047;
  const y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
  const z = (0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]) / 1.08883;
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  return [116 * f(y) - 16, 500 * (f(x) - f(y)), 200 * (f(y) - f(z))];
}
const deltaE = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/** The radius for a fill from the zone at `lab` (`Zones::fill_radius`). */
export function fillRadius(zones, l) {
  const d = (zones?.centres || []).map((c) => deltaE(c, l));
  const group = d.filter((x) => x > 0.5 && x <= 30).reduce((a, b) => Math.max(a, b), 0);
  const other = d.filter((x) => x > 30).reduce((a, b) => Math.min(a, b), Infinity);
  if (!Number.isFinite(other)) return 1000;
  if (group > 0 && group < other - 4) return (group + other) / 2;
  const near = d.filter((x) => x > 0.5).reduce((a, b) => Math.min(a, b), Infinity);
  return Number.isFinite(near) ? Math.min(40, Math.max(4, near / 2)) : 1000;
}

// --- layers --------------------------------------------------------------------------------

let counter = 0;
const base36 = (v) => Math.max(0, Math.floor(v)).toString(36);
/** A fresh id (`l` + the time and a counter in base 36, as both studios make them). */
export const newId = (prefix = 'l') => `${prefix}${base36(Date.now())}${base36(counter++)}`;

/** A new layer (`model::layer`). */
export function layer(name, kind) {
  return { id: newId('l'), naam: name, zichtbaar: true, dekking: 1, detail: 1, ...kind };
}

export const baseColour = (colour) => ({ soort: 'zone', centrum: [50, 0, 0], straal: 1000, kleur: colour });
export const groupColour = (group, colour) => ({ soort: 'zone', centrum: [50, 0, 0], straal: 1000, lakgroep: group, kleur: colour });

/** The kind's name in the interface (`Kind::label`). */
export function kindLabel(l) {
  switch (l.soort) {
    case 'zone':
      if (l.lakgroep === 0) return 'Body colour';
      if (l.lakgroep != null) return 'Second colour';
      return l.straal >= 999 ? 'Base colour' : 'Colour area';
    case 'strook':
      return 'Stripe';
    case 'tekst':
      return 'Text';
    case 'afbeelding':
      return 'Image';
    case 'vorm':
      return 'Shape';
    case 'pad':
      return 'Own shape';
    case 'penseel':
      return 'Brush';
    default:
      return 'Layer';
  }
}

export const isDecal = (l) => l.soort === 'tekst' || l.soort === 'afbeelding' || l.soort === 'vorm' || l.soort === 'pad';

/** A decal's size on the bus (metres): the launcher's (`lak_sizes`), else what the layer says. */
export function decalSize(l, sizes) {
  const s = sizes?.[l.id];
  if (s) return s;
  const p = l.plaats;
  if (!p) return undefined;
  if (l.soort === 'afbeelding') return [p.breedteM, p.hoogteM ?? p.breedteM * (l.verhouding ?? 0.5)];
  if (l.soort === 'tekst') return [p.breedteM, ((l.hoogteCm || 30) / 100) * TEXT_BOX];
  return [p.breedteM, p.hoogteM ?? p.breedteM];
}

/** What a decal shows, for its picture (a text's letters, a shape at its size): a changed key
 * draws it again. */
export function decalKey(l, w, h) {
  if (l.soort === 'tekst') return JSON.stringify(['t', l.tekst, l.lettertype || '', l.letterafstand || 0, l.hoogteCm, l.omlijning?.breedteCm || 0, Math.round(w * 100)]);
  if (l.soort === 'vorm') return JSON.stringify(['v', l.vorm, Math.round(w * 100), Math.round(h * 100), l.omlijning?.breedteCm || 0]);
  if (l.soort === 'pad') return JSON.stringify(['p', l.punten, Math.round(w * 100), Math.round(h * 100), l.omlijning?.breedteCm || 0]);
  if (l.soort === 'afbeelding') return JSON.stringify(['a', l.beeld, !!l.witDoorzichtig]);
  return '';
}

/** A brush stroke's points and normals in the o3d frame (the project keeps the bus's). */
export function strokeO3d(st) {
  return { ...st, punten: (st.punten || []).map((p) => [p[0], p[2], p[1]]), normalen: (st.normalen || []).map((n) => [n[0], n[2], n[1]]) };
}

/** A point or normal of the o3d frame as the project keeps a stroke's (the bus's frame). */
export const toBus = (p) => [p[0], p[2], p[1]];

/** The decal (and whether its mirrored copy) under the point `p` with normal `n` (o3d), the top
 * one first (`decal_at`). */
export function decalAt(layers, sizes, planeX, p, n) {
  for (let i = layers.length - 1; i >= 0; i--) {
    const l = layers[i];
    if (l.zichtbaar === false || l.vergrendeld || !isDecal(l)) continue;
    const size = decalSize(l, sizes);
    if (!size) continue;
    const places = [{ place: l.plaats, copy: false }];
    if (planeX !== undefined) {
      const m = mirrorPlace(l.plaats, planeX, l.soort === 'tekst');
      if (m) places.push({ place: m.place, copy: true });
    }
    for (const { place, copy } of places) {
      const sn = SIDES[place.zijde]?.n || [1, 0, 0];
      const q = local(place, p);
      if (Math.abs(q[0]) <= size[0] / 2 && Math.abs(q[1]) <= size[1] / 2 && Math.abs(dot(sub(p, place.midden), sn)) < 0.8 && dot(n, sn) > 0.3) return { id: l.id, copy };
    }
  }
  return undefined;
}

/** The handles on a decal's outline: the four corners (`corner`), then the middles of the
 * bottom, right, top and left side (`height`, `width`) (`Grip::handles`). */
export function handles(c) {
  const mid = (a, b) => mul(add(c[a], c[b]), 0.5);
  return [
    ['corner', c[0]],
    ['corner', c[1]],
    ['corner', c[2]],
    ['corner', c[3]],
    ['height', mid(0, 1)],
    ['width', mid(1, 2)],
    ['height', mid(2, 3)],
    ['width', mid(3, 0)],
  ];
}

/** The size a decal of `size` gets when its handle `grip`, grabbed at `from` (its own box),
 * is pulled to `now` (`grip_size`). */
export function gripSize(grip, size, from, now, free) {
  const lim = (v) => Math.min(20, Math.max(0.02, v));
  if (grip === 'corner' && free) return [lim(Math.abs(now[0]) * 2), lim(Math.abs(now[1]) * 2)];
  if (grip === 'corner') {
    const k = Math.min(20, Math.max(0.05, Math.hypot(now[0], now[1]) / Math.max(1e-3, Math.hypot(from[0], from[1]))));
    return [lim(size[0] * k), lim(size[1] * k)];
  }
  if (grip === 'width') return [lim(Math.abs(now[0]) * 2), size[1]];
  return [size[0], lim(Math.abs(now[1]) * 2)];
}

/** A handle pulled (`Place::pulled`): the decal at that size round its middle. A text's size is
 * its letters' height (`hoogteCm`), which follows the width. */
export function pulled(l, start, grip, size, from, now, free) {
  if (l.soort === 'tekst') {
    const k = Math.min(20, Math.max(0.05, Math.hypot(now[0], now[1]) / Math.max(1e-3, Math.hypot(from[0], from[1]))));
    return { ...l, plaats: { ...l.plaats, breedteM: start.plaats.breedteM * k }, hoogteCm: Math.min(150, Math.max(2, (start.hoogteCm || 30) * k)) };
  }
  const corner = grip === 'corner';
  const freeCorner = free && corner;
  const [w, h] = gripSize(grip, size, from, now, freeCorner);
  const p = { ...l.plaats, breedteM: w };
  if (!corner || freeCorner) {
    p.hoogteM = h;
    p.verhoudingVast = false;
  } else if (start.plaats.hoogteM != null) p.hoogteM = h;
  else delete p.hoogteM;
  return { ...l, plaats: p };
}

/** A copy of layer `i` right above it, moved a little (`model::duplicate`). */
export function duplicate(layers, i) {
  const l = structuredClone(layers[i]);
  l.id = newId('l');
  l.naam = `${l.naam} (2)`;
  delete l.vergrendeld;
  delete l.uitRecept;
  if (l.plaats) {
    const { u, v } = SIDES[l.plaats.zijde] || SIDES.R;
    l.plaats.midden = add(l.plaats.midden, mul(sub(u, v), 0.1));
  }
  const out = [...layers];
  out.splice(i + 1, 0, l);
  return { layers: out, id: l.id };
}

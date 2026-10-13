// The line editor's map (`map_background`, `map_layer`, `map_buttons`, `map_pointer`): the
// chosen map's roads as the game's city map draws them (read by the game program), every bus
// stop on it, the line's legs over the roads - the direction worked on bright, the other faint,
// a leg without a way red and straight - and its stops in its colour, the chosen one ringed;
// the names of the direction's ends and of the stop under the mouse beside them.
//
// Dragged to move, the wheel to zoom where the mouse is; a stop clicked goes into the line
// (`click_stop`); a dragged point of a leg moves, a leg dragged gets a new one (`drag_via`,
// `drag_new`) - the line editor routes the leg again.

import { html, useEffect, useRef } from '../../../../vendor/preact-htm.js';
import { useStore } from '../../../store.js';
import { IconBtn, tr } from '../kit.js';
import { dark } from '../../theme.js';
import { L, le } from './state.js';

/** How near the mouse must come to a stop, a dragged point or a leg (points). */
const STOP_HIT = 10;
const VIA_HIT = 9;
const LEG_HIT = 6;
const DANGER = '#d93a30';
const LINE = '#ffd23f';
/** The stops of the map, quiet; the light dots (an end of the line, a dragged point). */
const STOP_GREY = 'rgba(149, 157, 176, 0.9)';
const LIGHT_DOT = '#e8ebf2';
/** A road at least this many points wide, its casing a hair round it (`mapview.rs`). */
const ROAD_PX = 1.4;
const CASING_EDGE_FAR = 0.5;
const CASING_EDGE_NEAR = 1.5;
const SURFACE_GROW_NEAR = 1.0;
/** A road at least a lane wide, and a little more on either side (metres). */
const LANE_MIN = 3.6;
const ROAD_MARGIN = 0.3;
/** The closest the map goes in (metres a point); a wheel notch's step. */
const MPP_MIN = 0.15;
const NOTCH = 1.12;

/** The map's colours in either look (`mapview::palette`): the ground, the roads' casing, a
 * road, a main road, a stop's rim. */
function palette() {
  return dark()
    ? { casing: '#212833', road: '#3c4552', main: '#495261', edge: '#090c18' }
    : { casing: '#ac9c86', road: '#fcf9f4', main: '#ffffff', edge: '#604c4c'.replace('604c4c', '60564c') };
}

/** The accent darkened (`Color::darken`): the casing under the line's legs. */
function darker(hex, t) {
  const n = parseInt(String(hex).replace('#', '').slice(0, 6), 16) || 0;
  const f = (v) => Math.round(v * (1 - t));
  return `rgb(${f((n >> 16) & 255)}, ${f((n >> 8) & 255)}, ${f(n & 255)})`;
}

/** The roads' casing round their surface and how far their surface grows past its metres, in
 * points, at `mpp` metres a point (`mapview::casing_edge`, its zoom bands). */
function casingEdge(mpp) {
  const tol = Math.min(24, Math.max(0.05, 2 ** Math.round(Math.log2(Math.max(mpp * 0.5, 1e-6)))));
  const t = Math.max(0, Math.min(1, Math.log2(4 / (tol * 2)) / 3));
  return [CASING_EDGE_FAR + (CASING_EDGE_NEAR - CASING_EDGE_FAR) * t, SURFACE_GROW_NEAR * t];
}

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** The whole map in view, a little air round it (`MapView::fit_to`). */
function fit(m, W, H) {
  const [lo, hi] = [m.lo, m.hi];
  const span = [Math.max(1, hi[0] - lo[0]), Math.max(1, hi[1] - lo[1])];
  const mpp = Math.max(span[0] / Math.max(1, W), span[1] / Math.max(1, H)) * 1.06;
  return { cx: (lo[0] + hi[0]) / 2, cy: (lo[1] + hi[1]) / 2, mpp: Math.max(mpp, MPP_MIN), most: Math.max(mpp / 1.06 * 2, MPP_MIN * 4) };
}

/** Distance from `p` to the polyline `pts` (picture points). */
function toPolyline(p, pts) {
  let best = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) {
    const [a, b] = [pts[i], pts[i + 1]];
    const ab = [b[0] - a[0], b[1] - a[1]];
    const l = ab[0] * ab[0] + ab[1] * ab[1] || 1e-6;
    const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l));
    best = Math.min(best, Math.hypot(a[0] + ab[0] * t - p[0], a[1] + ab[1] * t - p[1]));
  }
  return best;
}

export function LineMap() {
  useStore();
  const box = useRef();
  const cv = useRef();
  const ptr = useRef({ down: null, moved: 0, at: null });
  const v = L.view;
  const m = L.map;

  const size = () => [box.current?.clientWidth || 1, box.current?.clientHeight || 1];
  const cam = () => {
    if (!L.cam && m) L.cam = fit(m, ...size());
    return L.cam;
  };
  const X = (c, W, x) => W / 2 + (x - c.cx) / c.mpp;
  const Y = (c, H, y) => H / 2 - (y - c.cy) / c.mpp;
  const proj = (p) => {
    const c = cam();
    const [W, H] = size();
    return [X(c, W, p[0]), Y(c, H, p[1])];
  };
  const world = (p) => {
    const c = cam();
    const [W, H] = size();
    return [c.cx + (p[0] - W / 2) * c.mpp, c.cy - (p[1] - H / 2) * c.mpp];
  };

  /** What lies under the picture point `p`: a stop of the map, a dragged point, a leg. */
  const under = (p) => {
    const out = { stop: -1, via: null, leg: -1 };
    if (!m || !v.line) return out;
    let best = STOP_HIT;
    m.stops.forEach((s, i) => {
      const q = proj(s.at);
      const d = Math.hypot(q[0] - p[0], q[1] - p[1]);
      if (d < best) (best = d), (out.stop = i);
    });
    const d = v.line.directions[v.dir];
    if (!d) return out;
    (d.vias_at || []).forEach((vias, k) =>
      vias.forEach((at, j) => {
        const q = proj(at);
        if (Math.hypot(q[0] - p[0], q[1] - p[1]) < VIA_HIT) out.via = [k, j];
      }),
    );
    if (out.stop < 0 && !out.via) {
      (v.shapes[v.dir] || []).forEach((flat, k) => {
        const pts = [];
        for (let i = 0; i + 1 < flat.length; i += 2) pts.push(proj([flat[i], flat[i + 1]]));
        if (toPolyline(p, pts) < LEG_HIT && k < d.legs.length) out.leg = k;
      });
    }
    return out;
  };

  const draw = () => {
    const c = cv.current;
    if (!c || !box.current) return;
    // (sharp on the screen: the stage may be drawn larger or smaller than laid out)
    const [W, H] = size();
    const dpr = (window.devicePixelRatio || 1) * (c.getBoundingClientRect().width / W || 1);
    if (c.width !== Math.round(W * dpr) || c.height !== Math.round(H * dpr)) Object.assign(c, { width: Math.round(W * dpr), height: Math.round(H * dpr) });
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);
    if (!m) return;
    const k = cam();
    const px = (x) => X(k, W, x);
    const py = (y) => Y(k, H, y);
    const poly = (p) => {
      g.beginPath();
      for (let i = 0; i < p.length; i += 2) (i ? g.lineTo : g.moveTo).call(g, px(p[i]), py(p[i + 1]));
    };
    g.lineJoin = 'round';
    // the roads as the game's city map draws them (`MapView::build`): every casing, then the
    // surfaces over them - the side streets' first, the main roads' over them, square-ended
    // on a side street they run into
    const pal = palette();
    const [edge, grow] = casingEdge(k.mpp);
    const surface = (r) => Math.max((Math.max(r.w, LANE_MIN) + 2 * ROAD_MARGIN) / k.mpp, ROAD_PX) + 2 * grow;
    g.lineCap = 'round';
    g.strokeStyle = pal.casing;
    for (const r of m.roads) {
      poly(r.p);
      g.lineWidth = surface(r) + 2 * edge;
      g.stroke();
    }
    g.fillStyle = pal.main;
    for (const r of m.roads) {
      if (!r.main) continue;
      for (const i of [0, r.p.length - 2]) {
        g.beginPath();
        g.arc(px(r.p[i]), py(r.p[i + 1]), surface(r) / 2, 0, Math.PI * 2);
        g.fill();
      }
    }
    g.strokeStyle = pal.road;
    for (const r of m.roads) {
      if (r.main) continue;
      poly(r.p);
      g.lineWidth = surface(r);
      g.stroke();
    }
    g.lineCap = 'butt';
    g.strokeStyle = pal.main;
    for (const r of m.roads) {
      if (!r.main) continue;
      poly(r.p);
      g.lineWidth = surface(r);
      g.stroke();
    }
    g.lineCap = 'round';
    const line = v.line;
    const colour = line?.colour || css('--hub-accent');
    const dirs = line?.directions || [];
    const cur = v.dir;
    // the legs, each on a dark casing (`map_layer`): the other direction faint, the one worked
    // on bright, a leg without a way red
    const legs = [];
    v.shapes.forEach((ls, d) => d !== cur && ls.forEach((s) => legs.push([s, colour, 0.35, 4])));
    (v.shapes[cur] || []).forEach((s, i) => {
      const ok = dirs[cur]?.legs[i]?.ok;
      legs.push([s, ok ? colour : DANGER, 1, ok ? 5 : 3]);
    });
    g.strokeStyle = darker(css('--hub-accent') || '#f58620', 0.56);
    for (const [s, , , w] of legs) {
      poly(s);
      g.lineWidth = w + 4;
      g.stroke();
    }
    for (const [s, c2, a, w] of legs) {
      poly(s);
      g.strokeStyle = c2;
      g.globalAlpha = a;
      g.lineWidth = w;
      g.stroke();
    }
    g.globalAlpha = 1;
    // a stop on its rim, the chosen one ringed (`mark_vertices`)
    const dot = (at, fill, r, ring, alpha = 1) => {
      const [x, y] = [px(at[0]), py(at[1])];
      g.beginPath();
      g.arc(x, y, r + 1.5, 0, Math.PI * 2);
      g.fillStyle = pal.edge;
      g.fill();
      g.beginPath();
      g.arc(x, y, r, 0, Math.PI * 2);
      g.fillStyle = fill;
      g.globalAlpha = alpha;
      g.fill();
      g.globalAlpha = 1;
      if (ring) {
        g.beginPath();
        g.arc(x, y, r + 3, 0, Math.PI * 2);
        g.strokeStyle = ring;
        g.lineWidth = 2;
        g.stroke();
      }
    };
    // the map's stops, quiet; the line's in its colour, the chosen one ringed
    for (const s of m.stops) dot(s.at, STOP_GREY, 3.2);
    dirs.forEach((d, k2) => {
      if (k2 === cur) return;
      for (const at of d.stops_at || []) dot(at, colour, 4, null, 0.5);
    });
    const d = dirs[cur];
    if (d) {
      const n = (d.stops_at || []).length;
      (d.stops_at || []).forEach((at, i) => {
        const end = i === 0 || i + 1 === n;
        dot(at, end ? LIGHT_DOT : colour, end ? 6.5 : 5, v.sel_stop === i ? LINE : null);
      });
      for (const vias of d.vias_at || []) for (const at of vias) dot(at, LIGHT_DOT, 3.5, colour);
    }
    const hover = L.hover >= 0 ? m.stops[L.hover] : null;
    if (hover) dot(hover.at, LIGHT_DOT, 4.5, LINE);
    // the names: the ends of the direction and the stop under the mouse
    const labels = [];
    if (d && d.stops.length) {
      labels.push([d.stops_at[0], d.stops[0].name, false]);
      if (d.stops.length > 1) labels.push([d.stops_at[d.stops.length - 1], d.stops[d.stops.length - 1].name, false]);
    }
    if (hover) labels.push([hover.at, hover.name, true]);
    g.font = '500 9.2px "Hanken Grotesk", system-ui, sans-serif';
    g.textBaseline = 'middle';
    g.textAlign = 'center';
    const [panel, onMap, text] = [css('--hub-panel'), css('--le-on-map'), css('--hub-text')];
    for (const [at, name, hov] of labels) {
      const [x, y] = [px(at[0]), py(at[1])];
      const w = g.measureText(name).width + 16;
      g.beginPath();
      g.roundRect(x + 10, y - 11, w, 22, 11);
      g.fillStyle = hov ? panel : onMap;
      g.fill();
      g.fillStyle = text;
      g.fillText(name, x + 10 + w / 2, y + 0.5);
    }
    g.textAlign = 'left';
    // (where a dragged point goes, while it is held)
    const p = ptr.current;
    if (p.down?.kind && p.at && p.moved > 3) {
      g.beginPath();
      g.arc(p.at[0], p.at[1], 6, 0, Math.PI * 2);
      g.fillStyle = css('--hub-text');
      g.fill();
      g.beginPath();
      g.arc(p.at[0], p.at[1], 3.5, 0, Math.PI * 2);
      g.fillStyle = css('--hub-accent');
      g.fill();
    }
  };
  useEffect(draw);
  useEffect(() => {
    const ro = new ResizeObserver(() => draw());
    ro.observe(box.current);
    return () => ro.disconnect();
  }, []);

  /** The picture point of a mouse event (the stage may be drawn smaller than laid out). */
  const at = (e) => {
    const r = cv.current.getBoundingClientRect();
    const k = r.width / (cv.current.clientWidth || 1) || 1;
    return [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
  };
  const zoom = (f, p) => {
    const c = cam();
    if (!c) return;
    const before = p ? world(p) : null;
    c.mpp = Math.max(MPP_MIN, Math.min(c.most, c.mpp * f));
    if (before) {
      const after = world(p);
      c.cx += before[0] - after[0];
      c.cy += before[1] - after[1];
    }
    draw();
  };
  const onDown = (e) => {
    if (e.button !== 0 || !m) return;
    const p = at(e);
    const u = under(p);
    ptr.current = { down: { p, cam: { ...cam() }, kind: u.via ? 'via' : u.leg >= 0 ? 'leg' : null, via: u.via, leg: u.leg }, moved: 0, at: p };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onMove = (e) => {
    if (!m) return;
    const p = at(e);
    const pd = ptr.current;
    if (pd.down) {
      pd.moved = Math.max(pd.moved, Math.hypot(p[0] - pd.down.p[0], p[1] - pd.down.p[1]));
      pd.at = p;
      if (!pd.down.kind) {
        const c = cam();
        c.cx = pd.down.cam.cx - (p[0] - pd.down.p[0]) * c.mpp;
        c.cy = pd.down.cam.cy + (p[1] - pd.down.p[1]) * c.mpp;
      }
      draw();
      return;
    }
    const u = under(p);
    box.current.style.cursor = v.line && (u.stop >= 0 || u.via || u.leg >= 0) ? 'pointer' : 'grab';
    if (u.stop !== L.hover) {
      L.hover = u.stop;
      draw();
    }
  };
  const onUp = (e) => {
    const pd = ptr.current;
    ptr.current = { down: null, moved: 0, at: null };
    if (!pd.down || !m) return;
    const p = at(e);
    if (pd.down.kind && pd.moved > 3) {
      const w = world(p);
      if (pd.down.kind === 'via') le('drag_via', { k: pd.down.via[0], j: pd.down.via[1], at: w });
      else le('drag_new', { k: pd.down.leg, at: w });
      return;
    }
    if (pd.moved <= 3 && v.line) {
      const u = under(p);
      if (u.stop >= 0) le('click_stop', { i: u.stop });
    }
    draw();
  };
  const first = v.line && v.line.directions[v.dir] && !v.line.directions[v.dir].stops.length;
  return html`<div class="le-map" ref=${box} onPointerDown=${onDown} onPointerMove=${onMove} onPointerUp=${onUp} onPointerLeave=${() => L.hover >= 0 && ((L.hover = -1), draw())}
      onWheel=${(e) => (e.preventDefault(), zoom(e.deltaY > 0 ? NOTCH : 1 / NOTCH, at(e)))}>
    <canvas ref=${cv}></canvas>
    ${!m && html`<div class="le-map-state">${tr('Loading...')}</div>`}
    ${m && first && html`<div class="le-map-hint">${tr("Click the line's first stop")}</div>`}
    <div class="le-map-buttons" onPointerDown=${(e) => e.stopPropagation()}>
      ${[['add', 'Zoom in', () => zoom(1 / 1.5)], ['remove', 'Zoom out', () => zoom(1.5)], ['near_me', 'The whole map', () => ((L.cam = null), draw())]].map(([icon, title, f]) => html`<span class="b"><${IconBtn} icon=${icon} size=${14} title=${title} onClick=${f} /></span>`)}
    </div>
  </div>`;
}


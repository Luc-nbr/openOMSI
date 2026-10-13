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
import { IconBtn, Icon, tr } from '../kit.js';
import { mapColors } from '../../steps/mapbehind.js';
import { L, le, redraw } from './state.js';

/** How near the mouse must come to a stop, a dragged point or a leg (points). */
const STOP_HIT = 10;
const VIA_HIT = 9;
const LEG_HIT = 6;
const DANGER = '#d93a30';
const LINE = '#ffd23f';
const ON_MAP = 'rgba(18, 26, 38, 0.86)';
const STOP_GREY = 'rgba(149, 157, 176, 0.9)';

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** The whole map in view: its 1 - 99 % box in the picture (`MapView::refit`). */
function fit(m, W, H) {
  const [lo, hi] = [m.lo, m.hi];
  const mpp = Math.max((hi[0] - lo[0]) / Math.max(40, W - 60), (hi[1] - lo[1]) / Math.max(40, H - 60), 0.2);
  return { cx: (lo[0] + hi[0]) / 2, cy: (lo[1] + hi[1]) / 2, mpp };
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
    const dpr = window.devicePixelRatio || 1;
    const [W, H] = size();
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
    g.lineCap = g.lineJoin = 'round';
    const [casing, road, main] = mapColors();
    for (let pass = 0; pass < 2; pass++) {
      for (const r of m.roads) {
        poly(r.p);
        g.strokeStyle = pass ? (r.main ? main : road) : casing;
        g.lineWidth = Math.max((r.w + (pass ? 0 : 2)) / k.mpp, pass ? 1.4 : 2.4);
        g.stroke();
      }
    }
    const line = v.line;
    const colour = line?.colour || css('--hub-accent');
    const dirs = line?.directions || [];
    const cur = v.dir;
    // the legs: the other direction faint, the one worked on bright, a leg without a way red
    v.shapes.forEach((legs, d) => {
      if (d === cur) return;
      for (const s of legs) {
        poly(s);
        g.strokeStyle = colour;
        g.globalAlpha = 0.35;
        g.lineWidth = 4;
        g.stroke();
      }
    });
    g.globalAlpha = 1;
    (v.shapes[cur] || []).forEach((s, i) => {
      const ok = dirs[cur]?.legs[i]?.ok;
      poly(s);
      g.strokeStyle = ok ? colour : DANGER;
      g.lineWidth = ok ? 5 : 3;
      g.stroke();
    });
    const dot = (at, fill, r, ring) => {
      const [x, y] = [px(at[0]), py(at[1])];
      if (ring) {
        g.beginPath();
        g.arc(x, y, r + 2.5, 0, Math.PI * 2);
        g.fillStyle = ring;
        g.fill();
      }
      g.beginPath();
      g.arc(x, y, r, 0, Math.PI * 2);
      g.fillStyle = fill;
      g.fill();
    };
    // the map's stops, quiet; the line's in its colour, the chosen one ringed
    for (const s of m.stops) dot(s.at, STOP_GREY, 3.2);
    dirs.forEach((d, k2) => {
      if (k2 === cur) return;
      g.globalAlpha = 0.5;
      for (const at of d.stops_at || []) dot(at, colour, 4);
      g.globalAlpha = 1;
    });
    const text = css('--hub-text') || '#e8ebf2';
    const d = dirs[cur];
    if (d) {
      const n = (d.stops_at || []).length;
      (d.stops_at || []).forEach((at, i) => {
        const end = i === 0 || i + 1 === n;
        dot(at, end ? '#e8ebf2' : colour, end ? 6.5 : 5, v.sel_stop === i ? LINE : null);
      });
      for (const vias of d.vias_at || []) for (const at of vias) dot(at, '#e8ebf2', 3.5, colour);
    }
    const hover = L.hover >= 0 ? m.stops[L.hover] : null;
    if (hover) dot(hover.at, '#e8ebf2', 4.5, LINE);
    // the names: the ends of the direction and the stop under the mouse
    const labels = [];
    if (d && d.stops.length) {
      labels.push([d.stops_at[0], d.stops[0].name, false]);
      if (d.stops.length > 1) labels.push([d.stops_at[d.stops.length - 1], d.stops[d.stops.length - 1].name, false]);
    }
    if (hover) labels.push([hover.at, hover.name, true]);
    g.font = '500 9.2px "Hanken Grotesk", system-ui, sans-serif';
    g.textBaseline = 'middle';
    for (const [at, name, hov] of labels) {
      const [x, y] = [px(at[0]), py(at[1])];
      const w = g.measureText(name).width + 16;
      g.beginPath();
      g.roundRect(x + 10, y - 11, w, 22, 11);
      g.fillStyle = hov ? css('--hub-panel') : ON_MAP;
      g.fill();
      g.fillStyle = hov ? text : '#e8ebf2';
      g.fillText(name, x + 18, y + 0.5);
    }
    // (where a dragged point goes, while it is held)
    const p = ptr.current;
    if (p.down?.kind && p.at && p.moved > 3) {
      g.beginPath();
      g.arc(p.at[0], p.at[1], 6, 0, Math.PI * 2);
      g.fillStyle = '#e8ebf2';
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
    c.mpp = Math.max(0.05, Math.min(200, c.mpp * f));
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
      if (u.stop >= 0) le('click_stop', { i: L.view.line_stops ? mapIndex(u.stop) : u.stop });
    }
    draw();
  };
  const first = v.line && v.line.directions[v.dir] && !v.line.directions[v.dir].stops.length;
  return html`<div class="le-map" ref=${box} onPointerDown=${onDown} onPointerMove=${onMove} onPointerUp=${onUp} onPointerLeave=${() => L.hover >= 0 && ((L.hover = -1), draw())}
      onWheel=${(e) => (e.preventDefault(), zoom(e.deltaY > 0 ? 1.15 : 1 / 1.15, at(e)))}>
    <canvas ref=${cv}></canvas>
    ${!m && html`<div class="le-map-state">${tr(v.reading ? 'Loading...' : 'Loading...')}</div>`}
    ${m && first && html`<div class="le-map-hint">${tr("Click the line's first stop")}</div>`}
    <div class="le-map-buttons" onPointerDown=${(e) => e.stopPropagation()}>
      ${[['add', 'Zoom in', () => zoom(1 / 1.5)], ['remove', 'Zoom out', () => zoom(1.5)], ['near_me', 'The whole map', () => ((L.cam = null), draw())]].map(([icon, title, f]) => html`<span class="b"><${IconBtn} icon=${icon} size=${14} title=${title} onClick=${f} /></span>`)}
    </div>
  </div>`;
}

/** A stop of the map as the line editor numbers them (the map's list is the same). */
const mapIndex = (i) => i;

export { Icon };

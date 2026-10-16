// The fleet map, as our game-drawn launcher's (`company/map.rs`): the company's map - its roads
// as the game's city map draws them, read by the game program (`co_more_map`, as upstream's
// `routemap.js` and the line editor read theirs) - with every line the company runs drawn in its
// colour, and every bus of today's plan where it is at the map's clock (placed by
// `company::fleetmap` in Rust, asked for four times a second). The panel over the map's left
// side has the clock (live, or running by itself ten or sixty times as fast) and the lines with
// their buses on the road. Dragged to move, the wheel to zoom where the mouse is.
//
// The buses (Luc: "small bus icons, each with a tag showing its line number"): each a little bus
// pointing the way it drives in its line's colour, its line's number on a tag beside it, drawn on
// a canvas of their own over the roads' and moved between two answers frame by frame, so they
// glide instead of jumping; their size is the screen's at every zoom. A bus clicked opens its
// card beside it, which follows it as it drives and as the map moves: everything about the bus
// (`fleetmap::bus_card`). Closed with its ×, Escape or a click on the empty map.

import { html, useEffect, useRef, useState, useSize, Btn, Plate, Meter, IconBtn, Icon, rgbOf, inkOn, setCo, goTab, tr } from './kit.js';
import { useMore, actMore, TextIn, box } from './more.js';
import { Photo } from './fleet.js';
import { cmd } from './kit.js';
import { dark } from '../theme.js';
import { S } from '../../store.js';

/** A road at least this many points wide, its casing a hair round it (`mapview.rs`). */
const ROAD_PX = 1.4;
const CASING_EDGE_FAR = 0.5;
const CASING_EDGE_NEAR = 1.5;
const SURFACE_GROW_NEAR = 1.0;
const LANE_MIN = 3.6;
const ROAD_MARGIN = 0.3;
const MPP_MIN = 0.15;
const NOTCH = 1.12;

/** A bus moved further than this (metres) between two answers jumps there (the clock was set). */
const SNAP_M = 2500;
/** A bus glides to where the next answer has it over the time between answers, within these. */
const GLIDE_MIN = 120;
const GLIDE_MAX = 1200;
/** The card's width, and how far from its bus it stands. */
const CARD_W = 280;
const CARD_GAP = 22;

/** The map's colours in either look (`mapview::palette`): the roads' casing, a road, a main
 * road, a marker's rim. */
function palette() {
  return dark() ? { casing: '#212833', road: '#3c4552', main: '#495261', edge: '#090c18' } : { casing: '#ac9c86', road: '#fcf9f4', main: '#ffffff', edge: '#60564c' };
}

/** The roads' casing round their surface and how far their surface grows past its metres, in
 * points, at `mpp` metres a point (`mapview::casing_edge`, its zoom bands). */
function casingEdge(mpp) {
  const tol = Math.min(24, Math.max(0.05, 2 ** Math.round(Math.log2(Math.max(mpp * 0.5, 1e-6)))));
  const t = Math.max(0, Math.min(1, Math.log2(4 / (tol * 2)) / 3));
  return [CASING_EDGE_FAR + (CASING_EDGE_NEAR - CASING_EDGE_FAR) * t, SURFACE_GROW_NEAR * t];
}

/** The accent darkened (`Color::darken`): the casing under the company's lines. */
function darker(hex, t) {
  const n = parseInt(String(hex).replace('#', '').slice(0, 6), 16) || 0;
  const f = (v) => Math.round(v * (1 - t));
  return `rgb(${f((n >> 16) & 255)}, ${f((n >> 8) & 255)}, ${f(n & 255)})`;
}

/** A colour mixed with the faint grey (`Color::mix`): a bus not on a trip. */
function mix(hex, t) {
  const a = rgbOf(hex);
  const b = [96, 104, 122];
  return `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(',')})`;
}

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** The late running's colours (`late.css`), a bus early in the line editor's blue. */
const TONES = { ok: '#34b26e', warn: '#e8aa46', orange: '#ee7a22', danger: '#d93a30', early: '#3d8bfd' };

/** The map read, by company map and day (read again when the page is opened anew). */
let read = null;

export function FleetMapPage() {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((x) => x + 1), 250);
    return () => clearInterval(t);
  }, []);
  const v = useMore('fleetmap', { tick });
  const [map, setMap] = useState(null);
  const [ref, size] = useSize();
  // the map and the routes of today's trips, read by the game program once the timetable is
  useEffect(() => {
    if (!v || v.reading) return;
    const key = `${S.co.shell?.id}|${S.co.shell?.clock?.date}`;
    if (read?.key === key && read.data && v.built) {
      if (map !== read.data) setMap(read.data);
      return;
    }
    if (read?.key === key && read.busy) return;
    read = { key, busy: true, data: null };
    cmd('co_more_map')
      .then((d) => {
        if (read?.key !== key) return;
        if (d?.reading) {
          read = null;
          return;
        }
        read = { key, busy: false, data: d };
        setMap(d);
      })
      .catch((e) => {
        read = { key, busy: false, data: { error: e.message } };
        setMap(read.data);
      });
  }, [v?.reading, v?.built, S.co.shell?.clock?.date]);
  let body = null;
  if (v?.reading) body = html`<${TextIn} x=${0} y=${0} w=${size.w} h=${24} px=${14.5} tone="dim">${v.reading}</${TextIn}>`;
  else if (v && v.buses && size.w > 0) {
    const pw = Math.min(360, size.w * 0.42);
    body = html`<${MapCanvas} map=${map} v=${v} W=${size.w} H=${size.h} left=${12 + pw} />
      <${Side} v=${v} r=${{ x: 12, y: 12, w: pw, h: Math.max(0, size.h - 24) }} />`;
  }
  return html`<div class="cm-page cm-fleetmap" ref=${ref}>${body}</div>`;
}

/** Where a bus is drawn at `t` (ms): on its way from where it was drawn to where the last answer
 * has it - x, y and its heading the short way round. */
function glide(a, t) {
  const f = Math.max(0, Math.min(1, (t - a.t0) / a.dur));
  const dh = ((((a.to[2] - a.from[2]) % 360) + 540) % 360) - 180;
  return [a.from[0] + (a.to[0] - a.from[0]) * f, a.from[1] + (a.to[1] - a.from[1]) * f, a.from[2] + dh * f];
}

/** A rounded rectangle's path (`roundRect` where the web view lacks it). */
function rounded(g, x, y, w, h, r) {
  g.beginPath();
  if (g.roundRect) {
    g.roundRect(x, y, w, h, r);
    return;
  }
  g.moveTo(x + r, y);
  g.arcTo(x + w, y, x + w, y + h, r);
  g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r);
  g.arcTo(x, y, x + w, y, r);
  g.closePath();
}

/** A bus at (x, y) pointing `h` degrees (north 0, clockwise): its body in its line's colour, the
 * windscreen at the front, a nose that says the way; faded when it waits, hollow on an empty run,
 * a warning when broken down, ringed when picked or driven by the player. */
function drawBus(g, x, y, h, b, pal, picked, accent) {
  const k = picked ? 1.22 : 1;
  const w = 9 * k;
  const l = 20 * k;
  const waiting = b.state === 'layover' || b.state === 'depot';
  const fill = waiting ? mix(b.colour, 0.45) : b.colour;
  g.save();
  g.translate(x, y);
  if (picked || b.player) {
    g.beginPath();
    g.arc(0, 0, l * 0.5 + 6, 0, Math.PI * 2);
    g.globalAlpha = 0.22;
    g.fillStyle = picked ? accent : TONES.ok;
    g.fill();
    g.globalAlpha = 1;
    g.lineWidth = 2;
    g.strokeStyle = picked ? accent : TONES.ok;
    g.stroke();
  }
  g.rotate((h * Math.PI) / 180);
  // (the nose ahead of it)
  g.beginPath();
  g.moveTo(0, -l / 2 - 5 * k);
  g.lineTo(-3.2 * k, -l / 2 - 0.5);
  g.lineTo(3.2 * k, -l / 2 - 0.5);
  g.closePath();
  g.fillStyle = pal.edge;
  g.fill();
  rounded(g, -w / 2, -l / 2, w, l, 2.6 * k);
  g.fillStyle = b.state === 'deadhead' ? pal.road : fill;
  g.fill();
  g.lineWidth = b.state === 'deadhead' ? 2.4 : 1.4;
  g.strokeStyle = b.state === 'deadhead' ? b.colour : pal.edge;
  g.stroke();
  // (its windscreen)
  rounded(g, -w / 2 + 1.6 * k, -l / 2 + 1.6 * k, w - 3.2 * k, 3.6 * k, 1.2 * k);
  g.fillStyle = 'rgba(10,14,24,0.6)';
  g.fill();
  g.restore();
  if (b.state === 'broken') {
    g.beginPath();
    g.arc(x + 8, y - 8, 7, 0, Math.PI * 2);
    g.fillStyle = TONES.danger;
    g.fill();
    g.lineWidth = 1.5;
    g.strokeStyle = pal.edge;
    g.stroke();
    g.fillStyle = '#fff';
    g.font = '800 10px system-ui, sans-serif';
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText('!', x + 8, y - 7.5);
  }
}

/** The tag beside a bus: its line's number on the line's colour, a dot in its delay's colour
 * when it runs late or early. Its box, for the clicks. */
function drawTag(g, x, y, b, font, widths) {
  const text = String(b.tag || '');
  let tw = widths.get(text);
  if (tw == null) {
    g.font = font;
    tw = g.measureText(text).width;
    widths.set(text, tw);
  }
  const dot = b.tone !== 'ok' && TONES[b.tone] ? 9 : 0;
  const w = tw + 12 + dot;
  const h = 17;
  const bx = x + 11;
  const by = y - 25;
  const rgb = rgbOf(b.colour);
  rounded(g, bx, by, w, h, 4);
  g.fillStyle = `rgb(${rgb.join(',')})`;
  g.fill();
  g.lineWidth = 1;
  g.strokeStyle = 'rgba(0,0,0,0.35)';
  g.stroke();
  if (dot) {
    g.beginPath();
    g.arc(bx + 7.5, by + h / 2, 3.5, 0, Math.PI * 2);
    g.fillStyle = TONES[b.tone];
    g.fill();
    g.lineWidth = 1.2;
    g.strokeStyle = '#fff';
    g.stroke();
  }
  g.font = font;
  g.textAlign = 'left';
  g.textBaseline = 'middle';
  g.fillStyle = inkOn(rgb);
  g.fillText(text, bx + 6 + dot, by + h / 2 + 0.5);
  return [bx, by, w, h];
}

/** The map: its roads and the company's lines on one canvas (drawn when the map moves), the
 * buses on another (drawn every frame), and the card of the bus picked over them. */
function MapCanvas({ map, v, W, H, left }) {
  const base = useRef();
  const top = useRef();
  const cardEl = useRef();
  const cam = useRef(null);
  const ptr = useRef({ down: null, moved: 0 });
  // (the buses as drawn: key -> { from, to, t0, dur, b }; when the last answer came)
  const anim = useRef(new Map());
  const lastAnswer = useRef(0);
  const hits = useRef([]);
  const hover = useRef(null);
  const [pick, setPick] = useState(v.picked || null);
  const pickRef = useRef(pick);
  pickRef.current = pick;
  const size = useRef({ W, H, left });
  size.current = { W, H, left };
  const look = dark();
  // (the accent and the page's font, read when the look changed: not every frame)
  const kept = useRef({});
  if (kept.current.look !== look) kept.current = { look, accent: css('--hub-accent') || '#f58620', font: null };
  // (the whole of today's routes in view, right of the panel; the map when there are none)
  const fit = () => {
    let lo = map.lo;
    let hi = map.hi;
    if (map.lines?.length) {
      lo = [Infinity, Infinity];
      hi = [-Infinity, -Infinity];
      for (const l of map.lines)
        for (let i = 0; i + 1 < l.p.length; i += 2) {
          lo = [Math.min(lo[0], l.p[i]), Math.min(lo[1], l.p[i + 1])];
          hi = [Math.max(hi[0], l.p[i]), Math.max(hi[1], l.p[i + 1])];
        }
    }
    const ww = Math.max(1, W - left);
    const mpp = Math.max(MPP_MIN, Math.max((hi[0] - lo[0]) / Math.max(1, ww - 80), (hi[1] - lo[1]) / Math.max(1, H - 60)));
    // (the middle of the routes in the middle of the window right of the panel)
    return { cx: (lo[0] + hi[0]) / 2 - (left / 2) * mpp, cy: (lo[1] + hi[1]) / 2, mpp };
  };
  const X = (x) => size.current.W / 2 + (x - cam.current.cx) / cam.current.mpp;
  const Y = (y) => size.current.H / 2 - (y - cam.current.cy) / cam.current.mpp;
  /** A canvas sized to the box, sharp at the screen's pixels and the stage's scale. */
  const ready = (c) => {
    const { W: w, H: h } = size.current;
    const dpr = (window.devicePixelRatio || 1) * (c.getBoundingClientRect().width / w || 1);
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) Object.assign(c, { width: Math.round(w * dpr), height: Math.round(h * dpr) });
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    return g;
  };
  const drawBase = () => {
    const c = base.current;
    if (!c) return;
    const g = ready(c);
    if (!map || map.error || !map.roads) return;
    if (!cam.current) cam.current = fit();
    const k = cam.current;
    const poly = (p) => {
      g.beginPath();
      for (let i = 0; i + 1 < p.length; i += 2) (i ? g.lineTo : g.moveTo).call(g, X(p[i]), Y(p[i + 1]));
    };
    g.lineJoin = 'round';
    const pal = palette();
    const [edge, grow] = casingEdge(k.mpp);
    const surface = (r) => Math.max((Math.max(r.w, LANE_MIN) + 2 * ROAD_MARGIN) / k.mpp, ROAD_PX) + 2 * grow;
    g.lineCap = 'round';
    g.strokeStyle = pal.casing;
    for (const r of map.roads) {
      poly(r.p);
      g.lineWidth = surface(r) + 2 * edge;
      g.stroke();
    }
    g.fillStyle = pal.main;
    for (const r of map.roads) {
      if (!r.main || r.p.length < 2) continue;
      for (const i of [0, r.p.length - 2]) {
        g.beginPath();
        g.arc(X(r.p[i]), Y(r.p[i + 1]), surface(r) / 2, 0, Math.PI * 2);
        g.fill();
      }
    }
    g.strokeStyle = pal.road;
    for (const r of map.roads) {
      if (r.main) continue;
      poly(r.p);
      g.lineWidth = surface(r);
      g.stroke();
    }
    g.lineCap = 'butt';
    g.strokeStyle = pal.main;
    for (const r of map.roads) {
      if (!r.main) continue;
      poly(r.p);
      g.lineWidth = surface(r);
      g.stroke();
    }
    // the company's lines, each trip's route once in its line's colour on a dark casing
    g.lineCap = 'round';
    g.strokeStyle = darker(css('--hub-accent') || '#f58620', 0.56);
    for (const l of map.lines || []) {
      poly(l.p);
      g.lineWidth = 8;
      g.stroke();
    }
    for (const l of map.lines || []) {
      poly(l.p);
      g.strokeStyle = l.colour;
      g.lineWidth = 4;
      g.stroke();
    }
  };
  /** The buses where they are this frame, their tags, and the card beside the one picked. */
  const drawBuses = (t) => {
    const c = top.current;
    if (!c) return;
    const g = ready(c);
    const out = [];
    hits.current = out;
    if (!cam.current || !map?.roads) return;
    const { W: w, H: h } = size.current;
    const pal = palette();
    if (!kept.current.font) kept.current.font = '700 10.5px ' + (getComputedStyle(c).fontFamily || 'system-ui, sans-serif');
    const font = kept.current.font;
    const accent = kept.current.accent;
    const widths = drawBuses.widths || (drawBuses.widths = new Map());
    const list = [];
    for (const a of anim.current.values()) {
      const [mx, my, hd] = glide(a, t);
      const x = X(mx);
      const y = Y(my);
      // (only what is on the screen, a margin for its tag)
      if (x < -60 || y < -60 || x > w + 60 || y > h + 60) {
        if (a.b.key === pickRef.current) list.push({ a, x, y, hd, off: true });
        continue;
      }
      list.push({ a, x, y, hd });
    }
    // (the one picked and the one under the mouse on top)
    const rank = (e) => (e.a.b.key === pickRef.current ? 2 : e.a.b.key === hover.current ? 1 : 0);
    list.sort((p, q) => rank(p) - rank(q));
    for (const e of list) if (!e.off) drawBus(g, e.x, e.y, e.hd, e.a.b, pal, e.a.b.key === pickRef.current, accent);
    for (const e of list) {
      if (e.off) continue;
      const tag = drawTag(g, e.x, e.y, e.a.b, font, widths);
      out.push({ key: e.a.b.key, x: e.x, y: e.y, tag });
    }
    // the card beside its bus, a line from the bus to it
    const el = cardEl.current;
    if (!el) return;
    const at = list.find((e) => e.a.b.key === pickRef.current);
    const ch = el.offsetHeight || 260;
    const minX = size.current.left + 8;
    let px;
    let py;
    if (at) {
      px = at.x + CARD_GAP;
      if (px + CARD_W > w - 8) px = at.x - CARD_GAP - CARD_W;
      if (px < minX) px = Math.min(w - 8 - CARD_W, Math.max(minX, at.x + CARD_GAP));
      py = Math.max(8, Math.min(h - ch - 8, at.y - ch / 2));
      px = Math.max(minX, Math.min(w - 8 - CARD_W, px));
      if (!at.off) {
        const ex = px > at.x ? px : px + CARD_W;
        const ey = Math.max(py + 14, Math.min(py + ch - 14, at.y));
        g.beginPath();
        g.moveTo(at.x, at.y);
        g.lineTo(ex, ey);
        g.lineWidth = 1.5;
        g.strokeStyle = accent;
        g.stroke();
      }
    } else {
      // (not on the map now - in the depot, its day done: in the corner)
      px = w - CARD_W - 12;
      py = 12;
    }
    el.style.transform = `translate(${Math.round(px)}px, ${Math.round(py)}px)`;
  };
  // the answers: each bus glides from where it is drawn to where the answer has it
  useEffect(() => {
    const t = performance.now();
    const since = lastAnswer.current ? t - lastAnswer.current : GLIDE_MIN;
    lastAnswer.current = t;
    const dur = Math.max(GLIDE_MIN, Math.min(GLIDE_MAX, since));
    const seen = new Set();
    for (const b of v.buses) {
      seen.add(b.key);
      const to = [b.at[0], b.at[1], b.h];
      const a = anim.current.get(b.key);
      if (!a) {
        anim.current.set(b.key, { from: to, to, t0: t, dur, b });
        continue;
      }
      const now = glide(a, t);
      const far = Math.hypot(to[0] - now[0], to[1] - now[1]) > SNAP_M;
      anim.current.set(b.key, { from: far ? to : now, to, t0: t, dur, b });
    }
    for (const k of [...anim.current.keys()]) if (!seen.has(k)) anim.current.delete(k);
  }, [v]);
  // (the server's pick, when it differs: another page or a friend's view changed it)
  useEffect(() => {
    if ((v.picked || null) !== pickRef.current && !pickRef.pending) setPick(v.picked || null);
  }, [v.picked]);
  // every frame: the buses
  useEffect(() => {
    let id = 0;
    const frame = (t) => {
      drawBuses(t);
      id = requestAnimationFrame(frame);
    };
    id = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(id);
  }, [map]);
  // the roads, when the map, its box or the look changed (a new map fitted in view first)
  useEffect(() => {
    cam.current = null;
  }, [map]);
  useEffect(drawBase, [map, W, H, left, look]);
  const choose = (key) => {
    setPick(key);
    pickRef.current = key;
    pickRef.pending = true;
    actMore('map_pick', { key: key || '' }).finally(() => (pickRef.pending = false));
  };
  // Escape closes the card (before it goes back to the start screen)
  useEffect(() => {
    if (!pick) return undefined;
    const k = (e) => {
      if (e.key !== 'Escape' || S.co.dialog || S.co.popup) return;
      e.stopImmediatePropagation();
      choose(null);
    };
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, [pick]);
  const at = (e) => {
    const r = top.current.getBoundingClientRect();
    const s = r.width / W || 1;
    return [(e.clientX - r.left) / s, (e.clientY - r.top) / s];
  };
  /** The bus under a point: its icon, or its tag. */
  const busAt = (p) => {
    let best = null;
    let bd = 14;
    for (const hit of hits.current) {
      const d = Math.hypot(hit.x - p[0], hit.y - p[1]);
      const [tx, ty, tw, th] = hit.tag;
      const onTag = p[0] >= tx && p[0] <= tx + tw && p[1] >= ty && p[1] <= ty + th;
      if (onTag) return hit.key;
      if (d < bd) (bd = d), (best = hit.key);
    }
    return best;
  };
  const blocked = (p) => p[0] >= 12 && p[0] < left && p[1] >= 12 && p[1] < H - 12;
  const card = v.card && v.card.key === pick ? v.card : null;
  return html`<div class="cm-mapbox" style=${box(0, 0, W, H)}
      onPointerDown=${(e) => {
        const p = at(e);
        if (blocked(p) || e.target.closest?.('.fm-card')) return;
        ptr.current = { down: p, moved: 0 };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove=${(e) => {
        const p = at(e);
        const d = ptr.current.down;
        if (d && cam.current) {
          const dx = p[0] - d[0];
          const dy = p[1] - d[1];
          ptr.current.moved += Math.abs(dx) + Math.abs(dy);
          ptr.current.down = p;
          cam.current.cx -= dx * cam.current.mpp;
          cam.current.cy += dy * cam.current.mpp;
          drawBase();
        } else if (!e.target.closest?.('.fm-card')) {
          const h = blocked(p) ? null : busAt(p);
          hover.current = h;
          top.current.style.cursor = h ? 'pointer' : 'grab';
        }
      }}
      onPointerUp=${(e) => {
        const moved = ptr.current.moved;
        const down = ptr.current.down;
        ptr.current = { down: null, moved: 0 };
        if (!down || moved >= 5) return;
        const key = busAt(at(e));
        if (key !== pick) choose(key);
      }}
      onPointerLeave=${() => (hover.current = null)}
      onWheel=${(e) => {
        const p = at(e);
        if (blocked(p) || !cam.current || e.target.closest?.('.fm-card')) return;
        e.preventDefault();
        const k = cam.current;
        const wx = k.cx + (p[0] - W / 2) * k.mpp;
        const wy = k.cy - (p[1] - H / 2) * k.mpp;
        k.mpp = Math.min(Math.max(k.mpp * (e.deltaY > 0 ? NOTCH : 1 / NOTCH), MPP_MIN), 400);
        k.cx = wx - (p[0] - W / 2) * k.mpp;
        k.cy = wy + (p[1] - H / 2) * k.mpp;
        drawBase();
      }}>
    <canvas ref=${base} style=${`width:${W}px;height:${H}px`}></canvas>
    <canvas ref=${top} class="fm-buses" style=${`width:${W}px;height:${H}px;cursor:grab`}></canvas>
    ${!map && html`<div class="cm-mapstate">${tr('Reading the map…')}</div>`}
    ${map?.error && html`<div class="cm-mapstate t-danger">${map.error}</div>`}
    <div class="fm-card" ref=${cardEl} style=${`width:${CARD_W}px;max-height:${Math.max(160, H - 16)}px;display:${card ? 'flex' : 'none'}`}>
      ${card && html`<${Card} c=${card} tip=${v.tips.close} onClose=${() => choose(null)} />`}
    </div>
  </div>`;
}

/** RosterFlow opened as `o` asks (`openRosterFlow`), the Planning page where it is not on. */
function toRf(o) {
  import('../rosterflow/index.js')
    .then((m) => m.openRosterFlow(o || {}))
    .then((ok) => ok || goTab(5))
    .catch(() => goTab(5));
}

/** A way on from the card: RosterFlow, a page of the company, the bus's own. */
function go(l) {
  if (l.rf) toRf(l.rf);
  else if (l.bus != null) setCo({ dialog: { kind: 'vehicle', id: l.bus } });
  else if (l.tab != null) goTab(l.tab);
}

/** The card of the bus picked (`fleetmap::bus_card`): its head, the bus, what it does and its
 * delay, its day, what happened to it, and the ways on. */
function Card({ c, tip, onClose }) {
  const links = (c.links || []).filter((l) => !(l.rf && S.co?.remote));
  return html`<div class="fm-head">
      <${Plate} p=${c.head.plate} h=${22} />
      <span class="fm-title cut">${c.head.title}</span>
      ${c.head.block && html`<span class="fm-chip cut">${c.head.block}</span>`}
      <${IconBtn} icon="close" size=${15} title=${tip} style="width:26px;height:26px;flex:none" onClick=${onClose} />
    </div>
    <div class="fm-body co-scroll">
      ${c.away && html`<p class="fm-what t-soft">${c.away}</p>`}
      ${!c.away &&
      html`${(c.photo || c.facts?.length) &&
        html`<div class="fm-bus">
          ${c.photo && html`<${Photo} file=${c.photo.file} paint=${c.photo.paint} name=${c.photo.name} />`}
          <div class="fm-facts">${(c.facts || []).map((f, i) => html`<span class=${i ? 't-soft cut' : 'cut'}>${f}</span>`)}</div>
        </div>`}
        <p class=${'fm-what fm-' + c.state}>${c.what}</p>
        ${c.next && html`<p class="fm-next t-soft">${c.next}</p>`}
        ${c.source && html`<p class="fm-next t-dim"><${Icon} name="sensors" size=${13} /> ${c.source}</p>`}
        <div class="fm-delay">
          <b class=${'fm-badge fm-tone-' + c.delay.tone}>${c.delay.value}</b>
          <span class="cut"><span class="t-soft">${c.delay.label}: </span>${c.delay.text}</span>
        </div>
        ${c.delay.cause && html`<p class="fm-cause t-dim">${c.delay.cause}</p>`}
        <div class="fm-rows">
          ${c.rows.map(
            (r) => html`<div class="fm-row">
              <span class="t-soft cut">${r.label}</span>
              <b class=${'cut t-' + r.tone}>${r.value}</b>
              ${r.meter != null && html`<span class="fm-meter"><${Meter} frac=${r.meter} tone=${r.tone} h=${4} /></span>`}
            </div>`,
          )}
        </div>
        ${c.notes?.length > 0 &&
        html`<div class="fm-notes">${c.notes.map((n) => html`<p class=${'fm-note fm-note-' + n.tone}><${Icon} name=${n.icon} size=${15} /><span>${n.text}</span></p>`)}</div>`}
        ${links.length > 0 && html`<div class="fm-links">${links.map((l) => html`<${Btn} icon=${l.icon} label=${l.label} small onClick=${() => go(l)} />`)}</div>`}`}
    </div>`;
}

/** The panel over the map (`map::side`): the clock and its speed, and the lines with their buses
 * on the road. */
function Side({ v, r }) {
  const iw = r.w - 32;
  const ih = r.h - 28;
  const bw = (iw - 18) / 4;
  let y = 0;
  const parts = [];
  parts.push(html`<${TextIn} x=${0} y=${0} w=${iw} h=${14} px=${12.5} weight=${700} tone="dim">${v.title}</${TextIn}>`);
  if (v.live) parts.push(html`<span class=${'co-tag t-' + v.live.tone} style=${`position:absolute;right:0;top:-3px;background-color:${v.live.tone === 'ok' ? 'rgba(var(--co-ok-fill),0.16)' : 'color-mix(in srgb, var(--co-accent2) 16%, transparent)'}`}>${v.live.text}</span>`);
  y += 24;
  parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${34} px=${28} weight=${700}>${v.clock}</${TextIn}>`);
  y += 40;
  const b = (k) => box(k * (bw + 6), y, bw, 36) + ';height:36px;padding:0';
  parts.push(html`<${Btn} kind="ghost" icon=${v.playing || v.is_live ? 'pause' : 'play_arrow'} title=${v.playing || v.is_live ? v.tips.stop : v.tips.run} style=${b(0)} onClick=${() => actMore('map_play')} />
    <${Btn} kind=${!v.is_live && Math.abs(v.speed - 10) < 0.5 ? 'primary' : 'ghost'} label="×10" title=${v.tips.x10} style=${b(1)} onClick=${() => actMore('map_speed', { speed: 10 })} />
    <${Btn} kind=${!v.is_live && Math.abs(v.speed - 60) < 0.5 ? 'primary' : 'ghost'} label="×60" title=${v.tips.x60} style=${b(2)} onClick=${() => actMore('map_speed', { speed: 60 })} />
    <${Btn} kind=${v.is_live ? 'primary' : 'ghost'} icon="my_location" title=${v.tips.now} style=${b(3)} onClick=${() => actMore('map_now')} />`);
  y += 46;
  return html`<div class="cm-panel" style=${box(r.x, r.y, r.w, r.h)}>
    <div style=${box(16, 14, iw, ih)}>
      ${parts}
      <div style=${`position:absolute;left:0;right:0;top:${y}px;bottom:0;display:flex;flex-direction:column`}>
        <p class="co-note" style="flex:none;margin-bottom:12px">${v.counts}</p>
        ${!v.legend.length
          ? html`<p class="co-para" style="font-size:10.75px">${v.no_lines}</p>`
          : html`<div class="co-scroll" style="flex:1;min-height:0;height:auto">
              ${v.legend.map(
                (l) => html`<div class="cm-legend">
                  <i style=${`background-color:${l.colour}`}></i>
                  <${Plate} p=${l.plate} h=${20} />
                  <span class="cut grow t-soft">${l.caption}</span>
                  <b class=${l.n > 0 ? '' : 't-faint'}>${l.n}</b>
                </div>`,
              )}
            </div>`}
      </div>
    </div>
  </div>`;
}

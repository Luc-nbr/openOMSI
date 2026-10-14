// The fleet map, as our game-drawn launcher's (`company/map.rs`): the company's map - its roads
// as the game's city map draws them, read by the game program (`co_more_map`, as upstream's
// `routemap.js` and the line editor read theirs) - with every line the company runs drawn in its
// colour, and every bus of today's plan as a marker where it is at the map's clock (placed by
// `company::fleetmap` in Rust, asked for four times a second). The panel over the map's left
// side has the clock (live, or running by itself ten or sixty times as fast), the lines with
// their buses on the road, and the card of the bus picked: its line, tour, duty, driver, delay,
// passengers and condition. Dragged to move, the wheel to zoom where the mouse is.

import { html, useEffect, useRef, useState, useSize, textW, Icon, Btn, Plate, Meter, IconBtn, tr } from './kit.js';
import { useMore, actMore, TextIn, box } from './more.js';
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

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

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
  else if (v && size.w > 0) {
    const pw = Math.min(360, size.w * 0.42);
    body = html`<${MapCanvas} map=${map} v=${v} W=${size.w} H=${size.h} left=${12 + pw} />
      <${Side} v=${v} r=${{ x: 12, y: 12, w: pw, h: Math.max(0, size.h - 24) }} />`;
  }
  return html`<div class="cm-page cm-fleetmap" ref=${ref}>${body}</div>`;
}

/** The map: its roads, the company's lines, the buses; a bus's number beside it under the mouse
 * and when picked. */
function MapCanvas({ map, v, W, H, left }) {
  const cv = useRef();
  const cam = useRef(null);
  const ptr = useRef({ down: null, moved: 0 });
  const [hover, setHover] = useState(-1);
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
  const X = (x) => W / 2 + (x - cam.current.cx) / cam.current.mpp;
  const Y = (y) => H / 2 - (y - cam.current.cy) / cam.current.mpp;
  const draw = () => {
    const c = cv.current;
    if (!c) return;
    const dpr = (window.devicePixelRatio || 1) * (c.getBoundingClientRect().width / W || 1);
    if (c.width !== Math.round(W * dpr) || c.height !== Math.round(H * dpr)) Object.assign(c, { width: Math.round(W * dpr), height: Math.round(H * dpr) });
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);
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
    // the buses: on a trip in their line's colour, else faded; the one picked larger and ringed
    for (const d of v.dots) {
      const x = X(d.at[0]);
      const y = Y(d.at[1]);
      const r = d.on ? 7.5 : 5.5;
      g.beginPath();
      g.arc(x, y, r + 1.5, 0, Math.PI * 2);
      g.fillStyle = pal.edge;
      g.fill();
      g.beginPath();
      g.arc(x, y, r, 0, Math.PI * 2);
      g.fillStyle = d.faded ? mix(d.colour, 0.55) : d.colour;
      g.fill();
      if (d.on) {
        g.beginPath();
        g.arc(x, y, r + 3, 0, Math.PI * 2);
        g.strokeStyle = css('--hub-text') || '#e8ebf2';
        g.lineWidth = 2;
        g.stroke();
      }
    }
  };
  useEffect(draw);
  useEffect(() => {
    cam.current = null;
  }, [map]);
  const at = (e) => {
    const r = cv.current.getBoundingClientRect();
    const s = r.width / W || 1;
    return [(e.clientX - r.left) / s, (e.clientY - r.top) / s];
  };
  const dotAt = (p) => {
    if (!cam.current) return -1;
    let best = -1;
    let bd = 12;
    v.dots.forEach((d, i) => {
      const dd = Math.hypot(X(d.at[0]) - p[0], Y(d.at[1]) - p[1]);
      if (dd < bd) (bd = dd), (best = i);
    });
    return best;
  };
  const blocked = (p) => p[0] >= 12 && p[0] < left && p[1] >= 12 && p[1] < H - 12;
  const labels = [];
  if (cam.current) {
    v.dots.forEach((d, i) => {
      if (!(d.on || i === hover)) return;
      const x = X(d.at[0]);
      const y = Y(d.at[1]);
      const w = textW(d.label, 13.5, 700) + 16;
      labels.push(html`<span class="cm-onmap" style=${box(x + 12, y - 11, w, 22)}>${d.label}</span>`);
    });
  }
  return html`<div class="cm-mapbox" style=${box(0, 0, W, H)}
      onPointerDown=${(e) => {
        const p = at(e);
        if (blocked(p)) return;
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
          draw();
        } else {
          const h = blocked(p) ? -1 : dotAt(p);
          if (h !== hover) setHover(h);
        }
      }}
      onPointerUp=${(e) => {
        const moved = ptr.current.moved;
        const down = ptr.current.down;
        ptr.current = { down: null, moved: 0 };
        if (!down || moved >= 5) return;
        const i = dotAt(at(e));
        const d = v.dots[i];
        actMore('map_pick', d ? { line: d.line, tour: d.tour } : {});
      }}
      onPointerLeave=${() => setHover(-1)}
      onWheel=${(e) => {
        const p = at(e);
        if (blocked(p) || !cam.current) return;
        e.preventDefault();
        const k = cam.current;
        const wx = k.cx + (p[0] - W / 2) * k.mpp;
        const wy = k.cy - (p[1] - H / 2) * k.mpp;
        k.mpp = Math.min(Math.max(k.mpp * (e.deltaY > 0 ? NOTCH : 1 / NOTCH), MPP_MIN), 400);
        k.cx = wx - (p[0] - W / 2) * k.mpp;
        k.cy = wy + (p[1] - H / 2) * k.mpp;
        draw();
      }}>
    <canvas ref=${cv} style=${`width:${W}px;height:${H}px;cursor:${hover >= 0 ? 'pointer' : 'grab'}`}></canvas>
    <div class="cm-maplabels">${labels}</div>
    ${!map && html`<div class="cm-mapstate">${tr('Reading the map…')}</div>`}
    ${map?.error && html`<div class="cm-mapstate t-danger">${map.error}</div>`}
  </div>`;
}

/** A colour mixed with the faint grey (`Color::mix`): a bus not on a trip. */
function mix(hex, t) {
  const n = parseInt(String(hex).replace('#', ''), 16) || 0;
  const a = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  const b = [96, 104, 122];
  return `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(',')})`;
}

/** The panel over the map (`map::side`): the clock and its speed, the lines with their buses on
 * the road, and the card of the bus picked. */
function Side({ v, r }) {
  const iw = r.w - 32;
  const ih = r.h - 28;
  const bw = (iw - 18) / 4;
  let y = 0;
  const parts = [];
  parts.push(html`<${TextIn} x=${0} y=${0} w=${iw} h=${14} px=${12.5} weight=${700} tone="dim">${v.title}</${TextIn}>`);
  if (v.live) parts.push(html`<span class=${'co-tag t-' + v.live.tone} style=${`position:absolute;right:0;top:-3px;background:${v.live.tone === 'ok' ? 'rgba(var(--co-ok-fill),0.16)' : 'color-mix(in srgb, var(--co-accent2) 16%, transparent)'}`}>${v.live.text}</span>`);
  y += 24;
  parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${34} px=${28} weight=${700}>${v.clock}</${TextIn}>`);
  y += 40;
  const b = (k) => box(k * (bw + 6), y, bw, 36) + ';height:36px;padding:0';
  parts.push(html`<${Btn} kind="ghost" icon=${v.playing || v.is_live ? 'pause' : 'play_arrow'} title=${v.playing || v.is_live ? v.tips.stop : v.tips.run} style=${b(0)} onClick=${() => actMore('map_play')} />
    <${Btn} kind=${!v.is_live && Math.abs(v.speed - 10) < 0.5 ? 'primary' : 'ghost'} label="×10" title=${v.tips.x10} style=${b(1)} onClick=${() => actMore('map_speed', { speed: 10 })} />
    <${Btn} kind=${!v.is_live && Math.abs(v.speed - 60) < 0.5 ? 'primary' : 'ghost'} label="×60" title=${v.tips.x60} style=${b(2)} onClick=${() => actMore('map_speed', { speed: 60 })} />
    <${Btn} kind=${v.is_live ? 'primary' : 'ghost'} icon="my_location" title=${v.tips.now} style=${b(3)} onClick=${() => actMore('map_now')} />`);
  y += 46;
  const cardH = 300;
  return html`<div class="cm-panel" style=${box(r.x, r.y, r.w, r.h)}>
    <div style=${box(16, 14, iw, ih)}>
      ${parts}
      <div style=${`position:absolute;left:0;right:0;top:${y}px;bottom:${v.card ? cardH + 12 : 0}px;display:flex;flex-direction:column`}>
        <p class="co-note" style="flex:none;margin-bottom:12px">${v.counts}</p>
        ${!v.legend.length
          ? html`<p class="co-para" style="font-size:10.75px">${v.no_lines}</p>`
          : html`<div class="co-scroll" style="flex:1;min-height:0;height:auto">
              ${v.legend.map(
                (l) => html`<div class="cm-legend">
                  <i style=${`background:${l.colour}`}></i>
                  <${Plate} p=${l.plate} h=${20} />
                  <span class="cut grow t-soft">${l.caption}</span>
                  <b class=${l.n > 0 ? '' : 't-faint'}>${l.n}</b>
                </div>`,
              )}
            </div>`}
      </div>
      ${v.card && html`<${Card} c=${v.card} tip=${v.tips.close} r=${{ x: -4, y: ih - cardH, w: iw + 8, h: cardH }} />`}
    </div>
  </div>`;
}

/** The card of the bus picked (`map::bus_card`). */
function Card({ c, r, tip }) {
  const iw = r.w - 24;
  const pw = Math.max(22 * 1.8, textW(c.head.plate.number, 13.2, 900) + 15.4);
  let y = 32;
  const parts = [];
  if (c.away) parts.push(html`<p class="co-para" style=${`position:absolute;left:0;right:0;top:${y}px;font-size:10.75px`}>${c.away}</p>`);
  else {
    parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${18} px=${14} weight=${500} tone="soft">${c.what}</${TextIn}>`);
    y += 26;
    if (c.next) {
      parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${18} px=${13.5} tone="dim">${c.next}</${TextIn}>`);
      y += 26;
    }
    for (const row of c.rows) {
      parts.push(html`<${TextIn} x=${0} y=${y} w=${iw * 0.45} h=${22} px=${13.5} tone="soft">${row.label}</${TextIn}>
        <${TextIn} x=${iw * 0.4} y=${y} w=${iw * 0.6} h=${22} px=${13.5} weight=${500} tone=${row.tone} align="right">${row.value}</${TextIn}>`);
      if (row.meter != null) parts.push(html`<div style=${box(0, y + 24, iw, 4)}><${Meter} frac=${row.meter} tone=${row.tone} h=${4} /></div>`);
      y += 26;
    }
  }
  return html`<div class="cm-buscard" style=${box(r.x, r.y, r.w, r.h)}>
    <div style=${box(12, 10, iw, r.h - 20)}>
      <span class="cm-icon" style="left:0;top:0"><${Plate} p=${c.head.plate} h=${22} /></span>
      <i class="cm-swatch" style=${`left:${pw + 8}px;top:4px;background:${c.head.colour}`}></i>
      <${TextIn} x=${pw + 18} y=${0} w=${iw - pw - 40} h=${22} px=${15.5} weight=${700}>${c.head.title}</${TextIn}>
      <${IconBtn} icon="close" size=${15} title=${tip} style=${`position:absolute;left:${iw - 21}px;top:-3px;width:26px;height:26px`} onClick=${() => actMore('map_pick', {})} />
      ${parts}
    </div>
  </div>`;
}

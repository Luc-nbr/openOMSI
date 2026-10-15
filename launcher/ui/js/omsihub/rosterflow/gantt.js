// RosterFlow's Gantt on a canvas (`crates/omsi-launcher-ui/src/company/planning/rf_gantt.rs`):
// the day as Optibus's scheduling module draws it. A row per bus's block (Vehicles) or per
// driver's duty (Drivers): a checkbox, a red "!" when the row breaks a rule, its ID; then on the
// hour axis its trips as saturated chips in the line's colour, pointed the way they run (right
// outbound, left inbound), and every event in Optibus's symbols - deadheads as small dark boxes
// (an orange dot where the time is estimated), pull-outs and pull-ins as grey-blue boxes, the
// pre- and post-trip checks and the sign-on and sign-off as T-bars, the reliefs as arrows, splits
// as a line with "Split", travel as an amber box with W, P or R, custom work as a white box,
// linked trips striped, a trip moved off its timetable tour with a dot. In Drivers every break
// of a duty is labelled in its gap with its kind and length, and its driving periods between
// the breaks that end one lie in a band under its row (`dutyRest`).
//
// A click on a chip chooses it and opens its card; Shift and a click widen the choice along the
// row; a double click (or MOVE, or a drag) takes it up, and the rows it may go to show drop zones
// rated with stars, blue where nothing breaks, red where a rule does, with the cost they make. A
// right click opens the menu. A gap between two trips at a terminus or the depot is clicked to
// cut a duty there; the relief arrows are clicked to join it again. The canvas draws what it is
// told and keeps where each part lies (`hits`), as the game-drawn launcher's frame does.

import { html, useEffect, useRef } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { alpha, mix, hex, inkOn } from './pal.js';
import * as D from './draw.js';
import { ROW, CHIP, ID_W, STAT_W, AXIS } from './draw.js';
import { setRf, pick, dropAt, edit, clock, hm } from './state.js';
import { anchor, begin, forget } from './anchors.js';
import { colourBy, colourOf, tripColour, stackColour } from './colourby.js';
// (the day's trips that ran late or were dropped, marked on their chips)
import * as SV from './service.js';

/** The stats columns beside the IDs (`rosterflow::Stat`). */
export const STATS = ['start', 'end', 'spread', 'paid', 'driving', 'km', 'trips', 'type'];
export const STAT_LABEL = { start: 'Start time', end: 'End time', spread: 'Spread', paid: 'Paid time', driving: 'Driving time', km: 'Distance', trips: 'Trips', type: 'Duty type' };
/** A scrollbar's thickness, the track's room included (`rf_scroll::BAR`). */
export const BAR = 12;
const DEADHEAD_MIN = 5;
/** The band of a duty's driving periods under its row (the Layers' "Driving periods"). */
const BAND = 12;

/** A row's pitch: a duty's a band taller while its driving periods are shown. */
export const rowPitch = (drivers) => (drivers && S.rf.layers.periods !== false ? ROW + BAND : ROW);

const eq = (a, b) => String(a).trim().toLowerCase() === String(b).trim().toLowerCase();

/** RosterFlow's scrollbar colours in its look (`rf_scroll::look`). */
export function barLook(p) {
  return p.light ? { track: alpha(p.text, 0.06), thumb: alpha(p.text, 0.24), hot: alpha(p.text, 0.44) } : { track: [255, 255, 255, 0.05], thumb: [255, 255, 255, 0.22], hot: [255, 255, 255, 0.42] };
}

/** A scrollbar in `t` for a view of `view` out of `total` scrolled to `pos` (`Ui::scrollbar`):
 * nothing when everything is in view; else its thumb. */
export function drawBar(ctx, look, t, vertical, pos, view, total, hot) {
  if (total <= view + 0.5 || view <= 0) return null;
  const max = total - view;
  const [start, len] = vertical ? [t.y, t.h] : [t.x, t.w];
  const thumbLen = Math.min(len, Math.max((len * view) / total, Math.min(28, len * 0.5)));
  const room = Math.max(1, len - thumbLen);
  const at = start + room * Math.max(0, Math.min(1, pos / max));
  const thumb = vertical ? { x: t.x, y: at, w: t.w, h: thumbLen } : { x: at, y: t.y, w: thumbLen, h: t.h };
  const thick = vertical ? t.w : t.h;
  const inner = vertical ? { x: t.x + 3, y: t.y + 2, w: thick - 6, h: t.h - 4 } : { x: t.x + 2, y: t.y + 3, w: t.w - 4, h: thick - 6 };
  const r = (thick - 6) / 2;
  D.rounded(ctx, inner.x, inner.y, inner.w, inner.h, r, look.track);
  const th = vertical ? { x: inner.x, y: thumb.y + 2, w: inner.w, h: Math.max(8, thumb.h - 4) } : { x: thumb.x + 2, y: inner.y, w: Math.max(8, thumb.w - 4), h: inner.h };
  D.rounded(ctx, th.x, th.y, th.w, th.h, r, hot ? look.hot : look.thumb);
  return { thumb, room, start, max, track: t, vertical, view, pos };
}

// --- the rows ------------------------------------------------------------------------------------

/** The value of a stats column for a row (null: nothing to show), and its words. */
export function statOf(b, row, stat) {
  if (row.kind === 'block') {
    const t = b.blocks[row.i];
    if (!t) return null;
    switch (stat) {
      case 'start':
        return [t.from, clock(t.from)];
      case 'end':
        return [t.to, clock(t.to)];
      case 'spread':
        return [t.to - t.from, hm(t.to - t.from)];
      case 'driving': {
        const d = t.trips.reduce((s, x) => s + (x.arr - x.dep), 0);
        return [d, hm(d)];
      }
      case 'km':
        return [t.km, t.km.toFixed(0)];
      case 'trips': {
        const n = t.trips.filter((x) => x.counts).length;
        return [n, String(n)];
      }
      default:
        return null;
    }
  }
  const x = b.duties[row.i];
  if (!x) return null;
  switch (stat) {
    case 'start':
      return [x.start, clock(x.start)];
    case 'end':
      return [x.end, clock(x.end)];
    case 'spread':
      return [x.spread, hm(x.spread)];
    case 'paid':
      return [x.paid, hm(x.paid)];
    case 'driving':
      return [x.driving, hm(x.driving)];
    case 'km':
      return [x.km, x.km.toFixed(0)];
    case 'trips':
      return [x.trips, String(x.trips)];
    case 'type':
      return [x.kind_rank, x.kind];
    default:
      return null;
  }
}

/** The rows as shown (`rf_gantt::rows`): pinned first, then (filtered) by line and start, or as
 * a stats column sorts them. */
export function rowsOf(data, drivers) {
  const r = S.rf;
  const f = r.filter;
  const b = data.board;
  const issueRows = f.issues ? b.issues.filter((i) => f.issues.includes(i.key) && i.row != null).map((i) => `${i.scope}|${i.row}`) : null;
  let out = [];
  if (drivers) {
    b.duties.forEach((x, d) => {
      if (issueRows && !issueRows.includes(`duty|${d}`)) return;
      if (f.lockedOnly && !x.locked) return;
      if (f.hidden.length && x.spans.length && x.spans.every((sp) => f.hidden.some((h) => eq(h, b.blocks[sp.tour].line)))) return;
      if (x.end < f.from || x.start > f.to) return;
      out.push({ kind: 'duty', i: d, id: x.id, pinned: false });
    });
  } else {
    const lineAt = (l) => {
      const k = data.lines.findIndex((x) => eq(x.name, l));
      return k < 0 ? 1e9 : k;
    };
    const idx = b.blocks.map((_, i) => i).sort((a, c) => lineAt(b.blocks[a].line) - lineAt(b.blocks[c].line) || b.blocks[a].from - b.blocks[c].from || a - c);
    for (const i of idx) {
      const t = b.blocks[i];
      if (issueRows && !issueRows.includes(`vehicle|${i}`)) continue;
      if (f.hidden.some((h) => eq(h, t.line))) continue;
      if (f.lockedOnly && !t.locked) continue;
      if (t.to < f.from || t.from > f.to) continue;
      if (f.dir !== 0 && !t.trips.some((x) => x.counts && x.out === (f.dir === 1))) continue;
      out.push({ kind: 'block', i, id: t.id, pinned: false });
    }
  }
  if (r.sort) {
    const [stat, asc] = r.sort;
    const v = (row) => statOf(b, row, stat)?.[0] ?? Number.MAX_VALUE;
    out = out.map((row, k) => [row, k]).sort((a, c) => (asc ? v(a[0]) - v(c[0]) : v(c[0]) - v(a[0])) || a[1] - c[1]).map((x) => x[0]);
  }
  // (the pinned on top, in the order pinned)
  const top = [];
  for (const id of r.pinned) {
    const k = out.findIndex((x) => x.id === id);
    if (k >= 0) {
      const [row] = out.splice(k, 1);
      top.push({ ...row, pinned: true });
    }
  }
  return [...top, ...out];
}

/** The time a hand holds (its first departure, its last arrival) and which way it points. */
export function handSpan(b, h) {
  if (!h) return null;
  if (h.kind === 'trips' || h.kind === 'swap' || h.kind === 'duty_trips') {
    // (a duty's trips: (bus, trip) pairs, on one bus or several)
    const pairs = h.kind === 'duty_trips' ? h.trips : (h.trips || []).map((k) => [h.tour, k]);
    const xs = pairs.map(([i, k]) => b.blocks[i]?.trips[k]).filter(Boolean);
    if (!xs.length) return null;
    return [Math.min(...xs.map((x) => x.dep)), Math.max(...xs.map((x) => x.arr)), xs[0].out !== false];
  }
  if (h.kind === 'stack') {
    const s = b.stack[h.k];
    return s ? [s.dep, s.arr, s.out] : null;
  }
  if (h.kind === 'piece') {
    const xs = pieceTrips(b, h.tour, h.duty);
    if (!xs.length) return null;
    return [xs[0].dep, Math.max(...xs.map((x) => x.arr)), true];
  }
  return null;
}

export function handColour(b, h) {
  if (h.kind === 'stack') return hex(b.stack[h.k]?.colour);
  if (h.kind === 'duty_trips') return hex(b.blocks[h.trips[0]?.[0]]?.colour);
  return hex(b.blocks[h.tour]?.colour);
}

/** The trips of piece `duty` of block `tour`. */
export function pieceTrips(b, tour, duty) {
  const t = b.blocks[tour];
  const r = t?.pieces[duty];
  return r ? t.trips.slice(r[0], r[1]) : [];
}

/** The piece of block `tour` trip `trip` is in. */
export const pieceOf = (b, tour, trip) => Math.max(0, (b.blocks[tour]?.pieces || []).findIndex(([a, z]) => trip >= a && trip < z));

/** What a chip pressed takes up: trips of a bus in Vehicles; in Drivers its piece of work - or,
 * once the core moves single trips between duties (`studio::plan_move_to_duty`, Luc's driver
 * drag on `rf-driver-drag`: `rf_on`'s `trips_to_duties`), the trips chosen of duty `duty` as
 * (bus, trip) pairs, to drop on another duty or "New duty"; `piece` (Alt and a drag, the menu's
 * Move piece) the whole piece still. */
export function tripHand(b, drivers, tour, trips, trip, duty = null, pairs = null, piece = false) {
  const some = trips.length ? trips : [trip];
  if (!drivers) return { kind: 'trips', tour, trips: withParts(b, tour, some) };
  if (S.rf.tripsToDuties && duty != null && !piece) return { kind: 'duty_trips', duty, trips: pairs?.length ? pairs : some.map((j) => [tour, j]) };
  return { kind: 'piece', tour, duty: pieceOf(b, tour, trip) };
}

/** Trips `list` of block `tour` with the other part of a trip cut at a relief point on the way:
 * a bus's trip moves whole (`studio::wholes`). */
function withParts(b, tour, list) {
  const t = b.blocks[tour];
  const out = [...list];
  for (const j of list) {
    const x = t?.trips[j];
    const o = x?.part === 1 ? j + 1 : x?.part === 2 ? j - 1 : null;
    if (o != null && !out.includes(o)) out.push(o);
  }
  return out.sort((a, z) => a - z);
}

/** A part of a trip cut on the way: where and when the drivers change (its tip). */
const partTip = (x) => (x.part ? tr('Relief on the way at %{stop} (%{time})', { stop: x.at, time: clock(x.part === 1 ? x.arr : x.dep) }) : null);

/** The stops on the way trip `j` of block `i` can be cut at (`mids`, the core's
 * `studio::mid_stops`): small marks on its chip `cr`, lit under the mouse; a click cuts the duty
 * there (`studio::cut_mid`). */
function midTicks(ctx, g, x, cr, cy, xOf, hit, i, j) {
  const { p, mouse: m } = g;
  for (const [stop, name, at, ok, tip] of x.mids || []) {
    const xm = xOf(at);
    if (xm < cr.x + 4 || xm > cr.x + cr.w - 4) continue;
    const zr = { x: xm - 4, y: cr.y - 2, w: 8, h: cr.h + 4 };
    if (ok && inside(zr, m)) D.rect(ctx, xm - 1, cy - CHIP * 0.6, 2, CHIP * 1.2, p.accent);
    else D.rect(ctx, xm - 0.6, cr.y + cr.h - 5, 1.2, 5, ok ? [255, 255, 255, 0.8] : [0, 0, 0, 0.35]);
    hit(zr.x, zr.y, zr.w, zr.h, { kind: 'mid', tour: i, trip: j, stop, name, time: at, ok, tip, raw: true });
  }
}

/** The lanes the stack's trips lie in (no two at once in one lane). */
function lanesOf(b) {
  const ends = [];
  return b.stack.map((x) => {
    const k = ends.findIndex((e) => e <= x.dep);
    if (k >= 0) {
      ends[k] = x.arr;
      return k;
    }
    ends.push(x.arr);
    return ends.length - 1;
  });
}

const stackLanes = (b) => Math.min(4, b.stack.length ? Math.max(...lanesOf(b)) + 1 : 0);

/** The words on a trip's chip as the Layers ask: the line, the times, the stops. */
function chipLabel(x, layer) {
  if (layer === 1) return `${String(((x.dep % 60) + 60) % 60).padStart(2, '0')} > ${String(((x.arr % 60) + 60) % 60).padStart(2, '0')}`;
  if (layer === 2) return x.sl;
  return x.num;
}

/** Where the relief is within a layover (`rosterflow::relief_at`). */
function reliefAt(prefs, arr, dep, incoming) {
  switch (prefs.relief) {
    case 0:
      return incoming ? dep : arr;
    case 1:
      return arr;
    case 2:
      return dep;
    case 3:
      return Math.floor((arr + dep) / 2);
    default:
      return Math.max(dep - (prefs.relief_before ?? 5), arr);
  }
}

const inside = (r, m) => m && m.x >= r.x && m.x < r.x + r.w && m.y >= r.y && m.y < r.y + r.h;

// --- the drawing ------------------------------------------------------------------------------------

/** The Gantt drawn in `w` x `h` (`rf_gantt::gantt`), the mouse at `m` (none: away), what is
 * hovered as `g.hover` (the hit under the mouse at the last move). Returns where its parts lie
 * and what it needs besides (its tooltips, the scrollbars, the layout for the wheel). */
function draw(ctx, w, h, g) {
  const { data, drivers, readOnly, p, mouse: m, origin } = g;
  const r = S.rf;
  const b = data.board;
  const rows = rowsOf(data, drivers);
  const hits = [];
  const hit = (x, y, ww, hh, what) => hits.push({ x, y, w: ww, h: hh, ...what });
  const over = (x, y, ww, hh) => !r.hand && inside({ x, y, w: ww, h: hh }, m);
  const an = (name, x, y, ww, hh) => anchor(name, { x: origin.x + x, y: origin.y + y, w: ww, h: hh }, g.owner);
  const stats = r.stats;
  const hand = r.hand;
  const zones = r.zones;
  const layers = r.layers;
  const prefs = data.prefs;
  // (the trips' colours as the Layers ask: by the line, the trip ID, the block or the duty)
  g.by = colourBy(data);
  g.svc = SV.indexOf(SV.serviceOf(data));
  D.rect(ctx, 0, 0, w, h, p.grid);
  // the columns: the IDs, the stats, the hours
  const lw = ID_W + stats.length * STAT_W;
  const gx = lw + 6;
  const gw = Math.max(60, w - lw - 18);
  const [t0, t1] = b.span;
  const total = t1 - t0;
  const sRows = !drivers && (b.stack.length || hand?.kind === 'trips') ? Math.max(1, stackLanes(b)) : 0;
  const stackH = sRows ? sRows * ROW + 26 : 0;
  const hbarH = r.zoom > 1.001 ? BAR : 0;
  const body = { x: 0, y: AXIS + stackH, w, h: Math.max(0, h - AXIS - stackH - hbarH) };
  const RH = rowPitch(drivers);
  const contentH = (rows.length + 2) * RH;
  const visible = total / r.zoom;
  const pan = Math.max(0, Math.min(Math.max(0, total - visible), r.pan));
  let scroll = Math.max(0, Math.min(Math.max(0, contentH - body.h), r.scroll));
  // (a row asked for: scrolled into view)
  if (r.reveal) {
    const i = rows.findIndex((x) => x.id === r.reveal);
    if (i >= 0) {
      const y = i * RH;
      if (y < scroll || y + RH > scroll + body.h) scroll = Math.max(0, Math.min(Math.max(0, contentH - body.h), y - body.h * 0.3));
    }
    g.revealed = scroll;
  }
  const ppm = gw / visible;
  const xOf = (mm) => gx + (mm - t0 - pan) * ppm;
  g.layout = { gx, gw, t0, total, ppm, pan, visible, body, contentH, scroll, rows };
  let lit = null;
  const tips = [];
  // the hour bands, every other one lighter, under everything
  D.clip(ctx, gx, 0, gw, h);
  for (let hh = Math.floor(t0 / 60); hh < Math.floor(t1 / 60); hh++) {
    if (((hh % 2) + 2) % 2 === 1) {
      const a = xOf(hh * 60);
      D.rect(ctx, a, AXIS, xOf(hh * 60 + 60) - a, h - AXIS, p.band);
    }
  }
  D.unclip(ctx);
  // the axis: the hours, the header of the IDs and the stats
  D.rect(ctx, 0, 0, w, AXIS, p.axis);
  D.rect(ctx, 0, AXIS - 1, w, 1, p.line);
  D.rect(ctx, 0, AXIS, lw, h - AXIS, p.labels);
  const every = ppm * 60 >= 50 ? 1 : ppm * 60 >= 25 ? 2 : 3;
  D.clip(ctx, gx - 24, 0, gw + 30, AXIS);
  for (let hh = Math.floor(t0 / 60); hh <= Math.floor(t1 / 60); hh++) {
    if (hh % every) continue;
    D.textAt(ctx, `${String(((hh % 24) + 24) % 24).padStart(2, '0')}:00`, xOf(hh * 60), AXIS / 2 + 4, 11, 500, p.dim, 'center');
  }
  D.unclip(ctx);
  // the header's checkbox (all rows); "ID", or the ticked rows' count (the batch toolbar)
  const allIds = rows.map((x) => x.id);
  const allTicked = allIds.length > 0 && allIds.every((id) => r.ticked.includes(id));
  D.checkbox(ctx, p, 14, AXIS / 2, allTicked, inside({ x: 4.5, y: AXIS / 2 - 9.5, w: 19, h: 19 }, m));
  hit(4.5, AXIS / 2 - 9.5, 19, 19, { kind: 'tickall', ids: allTicked ? [] : allIds });
  if (!r.ticked.length || readOnly) D.textAt(ctx, 'ID', 40, AXIS / 2 + 4, 11, 700, p.dim);
  stats.forEach((stat, k) => {
    const hx = ID_W + k * STAT_W;
    const on = r.sort && r.sort[0] === stat;
    const hov = inside({ x: hx, y: 0, w: STAT_W, h: AXIS }, m);
    D.textIn(ctx, tr(STAT_LABEL[stat]), hx + 4, 0, STAT_W - 8, AXIS, 10, 700, on || hov ? p.accent : p.dim, 'center');
    hit(hx, 0, STAT_W, AXIS, { kind: 'stathead', stat, tip: 'Click to sort by it, again the other way, a third time back' });
  });
  // the stack (Optibus's edit mode: the trips no bus runs, above the schedule)
  if (sRows) {
    const sr = { x: 0, y: AXIS, w, h: stackH };
    if (b.stack.length) an('rf-stack', sr.x, sr.y, sr.w, sr.h);
    D.rect(ctx, sr.x, sr.y, sr.w, sr.h, mix(p.labels, p.grid, 0.5));
    D.rect(ctx, sr.x, sr.y + sr.h - 3, sr.w, 3, p.line);
    D.icon(ctx, 'inventory_2', 16, sr.y + 13, 15, p.dim);
    D.textAt(ctx, tr('Stack: unassigned trips').toUpperCase(), 30, sr.y + 17, 10.5, 700, p.dim);
    D.clip(ctx, gx - 4, sr.y, gw + 8, sr.h);
    const lanes = lanesOf(b);
    b.stack.forEach((x, k) => {
      const lane = Math.min(3, lanes[k] || 0);
      const cy = sr.y + 22 + lane * ROW + ROW / 2;
      const a = xOf(x.dep);
      const cr = { x: a + 1, y: cy - CHIP / 2, w: Math.max(6, xOf(x.arr) - a - 2), h: CHIP };
      const taken = hand?.kind === 'stack' && hand.k === k;
      const hov = inside(cr, m);
      D.chip(ctx, cr.x, cr.y, cr.w, cr.h, stackColour(p, g.by, x), x.out, x.num, hov || taken ? [255, 255, 255, 1] : null, false, false, taken ? 0.45 : 1);
      if (!readOnly && !hand) hit(cr.x, cr.y, cr.w, cr.h, { kind: 'stack', k });
    });
    // (where trips taken up go back onto the stack)
    if (hand?.kind === 'trips' && zones) {
      const zi = zones.findIndex((z) => z.drop.kind === 'stack');
      if (zi >= 0) {
        const zr = { x: gx, y: sr.y + 22, w: gw, h: sr.h - 26 };
        const on = inside(zr, m);
        D.roundedBorder(ctx, zr.x + 2, zr.y + 2, zr.w - 4, zr.h - 4, 4, 1.4, on ? p.accent : alpha(p.accent, 0.45));
        if (!b.stack.length) D.textIn(ctx, tr('Drop here to unassign: the trips wait on the stack'), zr.x, zr.y, zr.w, zr.h, 12, 500, p.accent, 'center');
        hit(zr.x, zr.y, zr.w, zr.h, { kind: 'zone', zi });
      }
    }
    D.unclip(ctx);
  }
  // the rows
  D.clip(ctx, body.x, body.y, body.w, body.h);
  let y = body.y - scroll;
  const span = handSpan(b, hand);
  let anchoredRow = false;
  for (const row of rows) {
    const rr = { x: 0, y, w, h: RH };
    y += RH;
    if (rr.y + RH < body.y || rr.y > body.y + body.h) continue;
    const cy = rr.y + ROW / 2;
    // (the tour's row: the first one wholly in view)
    if (!anchoredRow && rr.y >= body.y && rr.y + RH <= body.y + body.h) {
      anchoredRow = true;
      an('rf-row', rr.x, rr.y, rr.w - 12, rr.h);
      an('rf-row-id', rr.x + 4, rr.y, ID_W - 8, rr.h);
    }
    const rowOn = r.selRow === row.id;
    const rh = inside(rr, m) && inside(body, m);
    if (rowOn || rh) D.rect(ctx, gx - 6, rr.y, gw + 12, rr.h, alpha(p.text, rowOn ? 0.06 : 0.03));
    if (row.pinned) D.rect(ctx, 0, rr.y + rr.h - 1, w, 1, alpha(p.accent, 0.35));
    const isBlock = row.kind === 'block';
    const it = isBlock ? b.blocks[row.i] : b.duties[row.i];
    const iss = it.issues;
    const locked = it.locked;
    const tick = r.ticked.includes(row.id);
    D.checkbox(ctx, p, 14, cy, tick, inside({ x: 4.5, y: cy - 9.5, w: 19, h: 19 }, m));
    hit(4.5, cy - 9.5, 19, 19, { kind: 'tick', id: row.id });
    if (iss.any) {
      D.textAt(ctx, '!', 26, cy + 5, 14, 800, iss.hard ? p.err : p.warn);
      hit(23, rr.y, 11, rr.h, { kind: 'none', tip: iss.tip, raw: true });
    }
    const idr = { x: 36, y: rr.y, w: ID_W - 60, h: rr.h };
    const idh = inside(idr, m);
    const idCol = rowOn ? p.accent : idh ? p.text : alpha(p.text, 0.88);
    if (isBlock) D.rect(ctx, 36, cy - 6, 3, 12, g.by === 'block' ? colourOf(p, it.id) : hex(it.colour));
    else if (g.by === 'duty') D.rect(ctx, 36, cy - 6, 3, 12, colourOf(p, it.id));
    D.textAt(ctx, D.fit(ctx, it.label, ID_W - 43 - 42, 12.5, 700), 43, cy + 4.5, 12.5, 700, idCol);
    if (!readOnly) hit(idr.x, idr.y, idr.w, idr.h, { kind: 'id', row, cy });
    // (the lock: shown when locked, else on hover; the pin on hover)
    if ((locked || rh) && !readOnly) {
      const lc = ID_W - 30;
      const lr = { x: lc - 9, y: cy - 9, w: 18, h: 18 };
      const lh = inside(lr, m);
      D.icon(ctx, locked ? 'lock' : 'livery_unlock', lc, cy, 14, locked ? p.accent : lh ? p.text : p.faint);
      hit(lr.x, lr.y, lr.w, lr.h, { kind: 'lock', row, tip: locked ? 'Locked: Optimise leaves it and no edit changes it. Click to unlock' : 'Lock it: Optimise leaves it as it is' });
    }
    if ((row.pinned || rh) && !readOnly) {
      const pc = ID_W - 12;
      const pr = { x: pc - 9, y: cy - 9, w: 18, h: 18 };
      const ph = inside(pr, m);
      D.icon(ctx, 'location_on', pc, cy, 14, row.pinned ? p.accent : ph ? p.text : p.faint);
      hit(pr.x, pr.y, pr.w, pr.h, { kind: 'pin', id: row.id, tip: row.pinned ? 'Pinned to the top. Click to unpin' : 'Pin it to the top (P)' });
    }
    stats.forEach((stat, k) => {
      const v = statOf(b, row, stat);
      if (v) D.textAt(ctx, v[1], ID_W + k * STAT_W + STAT_W / 2, cy + 4, 11.5, 500, alpha(p.text, 0.85), 'center');
    });
    // the swap's target: a click on a bus of the line
    if (hand?.kind === 'swap' && isBlock && row.i !== hand.tour && eq(b.blocks[row.i].line, b.blocks[hand.tour].line)) {
      const zr = { x: gx, y: rr.y + 1, w: gw, h: rr.h - 2 };
      D.roundedBorder(ctx, zr.x, zr.y, zr.w, zr.h, 3, 1.2, inside(zr, m) ? p.accent : alpha(p.accent, 0.3));
      hit(zr.x, zr.y, zr.w, zr.h, { kind: 'swap', i: row.i });
    }
    D.clip(ctx, gx - 4, Math.max(rr.y, body.y), gw + 8, Math.min(rr.h, body.y + body.h - rr.y));
    const out = isBlock ? blockRow(ctx, g, row, cy, xOf, hit, an) : dutyRow(ctx, g, row, cy, xOf, hit, an);
    if (out) lit = out;
    // the drop zones in this row
    if (zones && span) {
      zones.forEach((z, zi) => {
        const here = (z.drop.kind === 'block' && isBlock && z.drop.i === row.i) || (z.drop.kind === 'duty' && !isBlock && z.drop.d === row.i);
        if (!here) return;
        const a = xOf(span[0]);
        const zr = { x: a + 1, y: cy - CHIP / 2 - 1, w: Math.max(30, xOf(span[1]) - a - 2), h: CHIP + 2 };
        D.zone(ctx, p, zr.x, zr.y, zr.w, zr.h, z, span[2], inside(zr, m));
        hit(zr.x, zr.y, zr.w, zr.h, { kind: 'zone', zi });
      });
    }
    D.unclip(ctx);
  }
  // the new row: a new bus or a new duty
  if (!readOnly) {
    const rr = { x: 0, y: y + 4, w, h: ROW };
    if (rr.y + ROW >= body.y && rr.y <= body.y + body.h) {
      const cy = rr.y + ROW / 2;
      if (rr.y + ROW <= body.y + body.h) an('rf-new-row', 0, rr.y, Math.min(gx + Math.min(gw, 560), rr.w - 12), rr.h);
      D.icon(ctx, 'add', 22, cy, 15, p.dim);
      D.textIn(ctx, tr(drivers ? 'New duty' : 'New vehicle'), 36, rr.y, Math.max(60, gx - 40), rr.h, 12, 500, p.dim);
      const want = drivers ? 'new_duty' : 'new';
      const zi = zones ? zones.findIndex((z) => z.drop.kind === want) : -1;
      if (zi >= 0 && span) {
        const a = xOf(span[0]);
        const zr = { x: a + 1, y: cy - CHIP / 2 - 1, w: Math.max(30, xOf(span[1]) - a - 2), h: CHIP + 2 };
        D.clip(ctx, gx - 4, rr.y, gw + 8, rr.h);
        D.zone(ctx, p, zr.x, zr.y, zr.w, zr.h, zones[zi], span[2], inside(zr, m));
        D.unclip(ctx);
        hit(zr.x, zr.y, zr.w, zr.h, { kind: 'zone', zi });
      } else if (drivers && !hand) {
        // (a spare duty from scratch: a click on the new row)
        const lr = { x: gx, y: rr.y + 2, w: Math.min(gw, 560), h: rr.h - 4 };
        const hov = inside(lr, m);
        D.roundedBorder(ctx, lr.x, lr.y, lr.w, lr.h, 3, 1, hov ? p.accent : p.line);
        D.textIn(ctx, r.tripsToDuties ? tr('Click for a spare duty of custom work, or drop trips here to begin a duty with them') : tr('Click for a spare duty of custom work, or drop a piece of work here to begin a duty with it'), lr.x + 10, lr.y, lr.w - 20, lr.h, 11.5, 400, hov ? p.text : p.faint);
        hit(lr.x, lr.y, lr.w, lr.h, { kind: 'spare' });
      }
    }
  }
  D.unclip(ctx);
  // the time chosen or under the mouse: a column down the chart (Optibus's highlight)
  if (lit) {
    const xa = Math.max(xOf(lit[0]), gx);
    const xb = Math.min(xOf(lit[1]), gx + gw);
    if (xb > xa) {
      D.rect(ctx, xa, AXIS, xb - xa, h - AXIS, alpha(p.text, 0.04));
      D.rect(ctx, xa, 0, xb - xa, AXIS, alpha(p.accent, 0.18));
    }
  }
  // what a drag carries, at the mouse
  if (r.dragging && span && m) {
    const cw = Math.max(20, xOf(span[1]) - xOf(span[0]));
    D.shadow(ctx, m.x - cw / 2, m.y - CHIP / 2, cw, CHIP, 4, 10, [0, 0, 0, 0.45]);
    D.chip(ctx, m.x - cw / 2, m.y - CHIP / 2, cw, CHIP, handColour(b, hand), span[2], '', [255, 255, 255, 1], false, false, 0.92);
  }
  // the scrollbars: down the rows, and along the day once zoomed in
  const look = barLook(p);
  const vt = { x: w - BAR, y: body.y, w: BAR, h: body.h };
  const vb = drawBar(ctx, look, vt, true, scroll, body.h, contentH, g.barHot === 'v');
  let hb = null;
  if (hbarH) {
    const sy = body.y + body.h;
    D.rect(ctx, 0, sy, w, hbarH, p.grid);
    D.rect(ctx, 0, sy, lw, hbarH, p.labels);
    D.rect(ctx, 0, sy, w, 1, p.line);
    hb = drawBar(ctx, look, { x: gx, y: sy, w: gw, h: hbarH }, false, pan, visible, total, g.barHot === 'h');
  }
  g.bars = { v: vb, h: hb };
  g.scroll = scroll;
  g.pan = pan;
  return { hits, tips };
}

/** A bus's block in its row (`rf_gantt::block_row_draw`). Returns the time lit. */
function blockRow(ctx, g, row, cy, xOf, hit, an) {
  const { data, readOnly, p, mouse: m } = g;
  const r = S.rf;
  const b = data.board;
  const i = row.i;
  const t = b.blocks[i];
  const n = t.trips.length;
  if (!n) return null;
  const prefs = data.prefs;
  const hand = r.hand;
  let lit = null;
  // the block's start and end (#98): the pre- and post-trip checks' T-bars joined to the pull-out
  // and pull-in (the timetable's depot runs) or straight to the first and last trip - the bus
  // runs from the depot into its first trip as the T's line - at the minutes the duties count
  const first = t.trips[0];
  const last = t.trips[n - 1];
  D.tBar(ctx, p, xOf(first.dep - prefs.pre_trip), blockEdge(ctx, p, first, xOf, cy, true), cy, true);
  D.tBar(ctx, p, blockEdge(ctx, p, last, xOf, cy, false), xOf(last.arr + prefs.post_trip), cy, false);
  t.trips.forEach((x, j) => {
    const a = xOf(x.dep);
    const bb = Math.max(xOf(x.arr), a + 4);
    // the gap before it: a deadhead where the places differ, the layover's minutes
    if (j > 0) {
      const prev = t.trips[j - 1];
      if (x.dh != null) D.deadhead(ctx, p, xOf(prev.arr) + 1, xOf(prev.arr + x.dh) - 1, cy, true);
      else if (r.layers.breaks && x.dep - prev.arr > 0 && xOf(x.dep) - xOf(prev.arr) > 18) D.textAt(ctx, `${x.dep - prev.arr}m`, (xOf(prev.arr) + a) / 2, cy + 3.5, 9.5, 500, p.dim, 'center');
    }
    if (x.empty || !x.counts) {
      // (the first and the last are the block's edges, drawn with its checks)
      if (j > 0 && j + 1 < n) D.deadhead(ctx, p, a + 1, bb - 1, cy, false);
      return;
    }
    const cr = { x: a + 1, y: cy - CHIP / 2, w: Math.max(5, bb - a - 2), h: CHIP };
    const colour = tripColour(p, b, g.by, i, j);
    const chosen = r.sel.some(([s0, s1]) => s0 === i && s1 === j);
    const taken = (hand?.kind === 'trips' || hand?.kind === 'swap') && hand.tour === i && hand.trips.includes(j);
    const hov = inside(cr, m) && !hand;
    const outline = chosen || hov ? [255, 255, 255, 1] : x.bad ? p.err : null;
    if (chosen || hov) lit = [x.dep, x.arr];
    // (a part of a trip cut on the way: its edge straight where the drivers change)
    if (x.part) D.cutChip(ctx, cr.x, cr.y, cr.w, cr.h, colour, x.out, chipLabel(x, r.layers.label), outline, taken ? 0.35 : 1, x.part === 2, x.part === 1);
    else D.chip(ctx, cr.x, cr.y, cr.w, cr.h, colour, x.out, chipLabel(x, r.layers.label), outline, x.linked, r.layers.moved && x.moved, taken ? 0.35 : 1);
    const svc = SV.at(g.svc, t.id, x.dep);
    SV.mark(ctx, cr, svc);
    if (cr.w > 24 && cr.x > g.layout.gx - 4 && cr.x + cr.w < g.layout.gx + g.layout.gw + 4) an('rf-chip', cr.x, cr.y, cr.w, cr.h);
    if (readOnly || hand) return;
    hit(cr.x, cr.y, cr.w, cr.h, { kind: 'chip', tour: i, trip: j, row: row.id, ...(x.part ? { tip: partTip(x), raw: true } : {}), ...(svc ? { tip: SV.tipOf(svc), raw: true } : {}) });
    midTicks(ctx, g, x, cr, cy, xOf, hit, i, j);
  });
  // the reliefs: arrows where a driver gets off and on, a grey line while it is attended; a click
  // there joins the duties again
  t.pieces.slice(1).forEach(([start], k) => {
    const pv = t.trips[start - 1];
    const nx = t.trips[start];
    const xa = xOf(pv.arr);
    const xb = xOf(nx.dep);
    if (prefs.relief === 0) {
      D.reliefArrow(ctx, p, xa + 1.5, cy, false, true);
      D.reliefArrow(ctx, p, xb - 1.5, cy, true, false);
    } else {
      D.rect(ctx, xa, cy - 0.5, Math.max(0, xb - xa), 1, p.grey);
      D.reliefArrow(ctx, p, xOf(reliefAt(prefs, pv.arr, nx.dep, true)), cy, true, true);
    }
    if (!readOnly && !hand) {
      const gr = { x: xa - 3, y: cy - CHIP * 0.6, w: Math.max(4, xb - xa) + 6, h: CHIP * 1.2 };
      an('rf-relief', gr.x, gr.y, gr.w, gr.h);
      if (inside(gr, m)) D.roundedBorder(ctx, gr.x, gr.y, gr.w, gr.h, 3, 1, p.accent);
      hit(gr.x, gr.y, gr.w, gr.h, { kind: 'relief', tour: i, at: start, tip: t.relief_tips[k], raw: true });
    }
  });
  // the gaps where a duty can be cut (on hover: where the bus stands at a relief point)
  if (!readOnly && !hand) {
    for (const [at, ok, why] of t.gaps) {
      const pv = t.trips[at - 1];
      const nx = t.trips[at];
      const xa = xOf(pv.arr);
      const xb = xOf(nx.dep);
      if (xb - xa < 3) continue;
      const gr = { x: xa, y: cy - CHIP / 2, w: xb - xa, h: CHIP };
      if (ok && inside(gr, m)) D.rect(ctx, (xa + xb) / 2 - 1, cy - CHIP * 0.6, 2, CHIP * 1.2, p.accent);
      hit(gr.x, gr.y, gr.w, gr.h, { kind: 'gap', tour: i, at, ok, tip: why, raw: true });
    }
  }
  return lit;
}

/** A block's first trip (`start`) or its last as the edge its check's T-bar joins: an empty run
 * (the timetable's depot run) as a pull-out or pull-in box, another trip without passengers as a
 * deadhead box - both drawn here - or the chip's edge (the chip the row draws). Returns its x. */
function blockEdge(ctx, p, x, xOf, cy, start) {
  const a = xOf(x.dep);
  const bb = Math.max(xOf(x.arr), a + 4);
  if (x.empty) return D.pull(ctx, p, a + 1, bb - 1, cy, false, start)[start ? 0 : 1];
  if (!x.counts) return D.deadhead(ctx, p, a + 1, bb - 1, cy, false)[start ? 0 : 1];
  return start ? a + 1 : a + 1 + Math.max(5, bb - a - 2);
}

/** A driver's duty in its row (`rf_gantt::duty_row_draw`). Returns the time lit. */
function dutyRow(ctx, g, row, cy, xOf, hit, an) {
  const { data, readOnly, p, mouse: m } = g;
  const r = S.rf;
  const b = data.board;
  const d = row.i;
  const s = b.duties[d];
  const prefs = data.prefs;
  const hand = r.hand;
  let lit = null;
  // the splits: a thin line across, "Split" in the middle (where no break's label says more)
  const lineCol = p.light ? p.dim : [220, 224, 230, 0.9];
  const labelled = (a, z) => (s.rest?.breaks || []).some((bk) => bk.from < z && a < bk.to);
  for (const [a, z] of s.splits) {
    const xa = xOf(a);
    const xb = xOf(z);
    D.rect(ctx, xa, cy - 0.5, Math.max(0, xb - xa), 1, lineCol);
    if (xb - xa > 50 && !labelled(a, z)) {
      const word = tr('Split');
      const ww = D.width(ctx, word, 10, 500) + 10;
      D.rect(ctx, (xa + xb) / 2 - ww / 2, cy - 7, ww, 14, p.grid);
      D.textIn(ctx, word, (xa + xb) / 2 - ww / 2, cy - 7, ww, 14, 10, 500, lineCol, 'center');
    }
  }
  // the block's start and end where a piece begins or ends with its bus's day (#98): the
  // pull-out or pull-in and the checks as the Vehicles tab draws them, the sign-on and sign-off
  // one T-bar with the check, joined to them at every zoom
  const joined = new Map();
  for (const sp of s.spans) {
    const t = b.blocks[sp.tour];
    const [rs, re] = t.pieces[sp.duty] || [0, 0];
    const n = t.trips.length;
    if (!n || re <= rs) continue;
    if (rs === 0) {
      const on = s.events.findIndex((e) => e.kind === 'on' && e.to === sp.from);
      const box = D.tBar(ctx, p, xOf(on >= 0 ? s.events[on].from : sp.from), blockEdge(ctx, p, t.trips[0], xOf, cy, true), cy, true);
      if (on >= 0) joined.set(on, box);
    }
    if (re === n) {
      const off = s.events.findIndex((e) => e.kind === 'off' && e.from === sp.to);
      const box = D.tBar(ctx, p, blockEdge(ctx, p, t.trips[n - 1], xOf, cy, false), xOf(off >= 0 ? s.events[off].to : sp.to), cy, false);
      if (off >= 0) joined.set(off, box);
    }
  }
  // the events: sign-on and sign-off, travel, custom work
  s.events.forEach((e, k) => {
    const a = xOf(e.from);
    const z = xOf(e.to);
    const er = { x: a, y: cy - CHIP / 2, w: Math.max(8, z - a), h: CHIP };
    const box = joined.get(k);
    if (box) Object.assign(er, { x: box[0], w: Math.max(8, box[1] - box[0]) });
    else if (e.kind === 'on') D.tBar(ctx, p, a, z, cy, true);
    else if (e.kind === 'off') D.tBar(ctx, p, a, z, cy, false);
    else if (e.kind === 'travel') {
      const bw = Math.max(14, z - a - 2);
      D.rounded(ctx, a + 1, cy - CHIP * 0.45, bw, CHIP * 0.9, 3, p.dead);
      D.roundedBorder(ctx, a + 1, cy - CHIP * 0.45, bw, CHIP * 0.9, 3, 1.3, p.warn);
      D.textAt(ctx, e.letter, a + 1 + bw / 2, cy + 4, 10.5, 700, p.warn, 'center');
    } else {
      const bw = Math.max(10, z - a - 2);
      D.rounded(ctx, a + 1, cy - CHIP / 2, bw, CHIP, 4, [238, 240, 244, 1]);
      if (p.light) D.roundedBorder(ctx, a + 1, cy - CHIP / 2, bw, CHIP, 4, 1, p.dim);
      if (D.width(ctx, e.label, 10.5, 700) < bw - 4) D.textAt(ctx, e.label, a + 1 + bw / 2, cy + 4, 10.5, 700, [30, 36, 46, 1], 'center');
    }
    if (!readOnly && !hand) hit(er.x, er.y, er.w, er.h, { kind: 'event', row: d, ev: k, custom: e.kind === 'custom' ? e.ci : null });
  });
  // the pieces' trips
  for (const sp of s.spans) {
    const t = b.blocks[sp.tour];
    const [rs, re] = t.pieces[sp.duty] || [0, 0];
    const n = t.trips.length;
    const takenPiece = hand?.kind === 'piece' && hand.tour === sp.tour && hand.duty === sp.duty;
    // attended: a grey line from where the driver takes the bus to its departure
    const f = t.trips[rs];
    if (f && rs > 0) {
      if (sp.from < f.dep - 1) D.rect(ctx, xOf(sp.from), cy - 0.5, xOf(f.dep) - xOf(sp.from), 1, p.grey);
      D.reliefArrow(ctx, p, xOf(sp.from) + 1.5, cy, true, !(prefs.relief === 0 || prefs.relief === 2));
    }
    const l = t.trips[re - 1];
    if (l && re < n) {
      if (sp.to > l.arr + 1) D.rect(ctx, xOf(l.arr), cy - 0.5, xOf(sp.to) - xOf(l.arr), 1, p.grey);
      D.reliefArrow(ctx, p, xOf(sp.to) - 1.5, cy, false, true);
    }
    for (let j = rs; j < re; j++) {
      const x = t.trips[j];
      const a = xOf(x.dep);
      const bb = Math.max(xOf(x.arr), a + 4);
      if (j > rs && x.dh != null) {
        const prev = t.trips[j - 1];
        D.deadhead(ctx, p, xOf(prev.arr) + 1, xOf(prev.arr + DEADHEAD_MIN) - 1, cy, true);
      }
      if (x.empty || !x.counts) {
        // (the block's first and last: drawn with the checks above)
        if (j > 0 && j + 1 < n) D.deadhead(ctx, p, a + 1, bb - 1, cy, false);
        continue;
      }
      const cr = { x: a + 1, y: cy - CHIP / 2, w: Math.max(5, bb - a - 2), h: CHIP };
      const colour = tripColour(p, b, g.by, sp.tour, j, d);
      const chosen = r.sel.some(([s0, s1]) => s0 === sp.tour && s1 === j);
      const takenTrip = hand?.kind === 'duty_trips' && hand.trips.some(([i, k]) => i === sp.tour && k === j);
      const hov = inside(cr, m) && !hand;
      if (chosen || hov) lit = [x.dep, x.arr];
      // (a part of a trip cut on the way: its edge straight where the drivers change)
      if (x.part) D.cutChip(ctx, cr.x, cr.y, cr.w, cr.h, colour, x.out, chipLabel(x, r.layers.label), chosen || hov ? [255, 255, 255, 1] : null, takenPiece || takenTrip ? 0.35 : 1, x.part === 2, x.part === 1);
      else D.chip(ctx, cr.x, cr.y, cr.w, cr.h, colour, x.out, chipLabel(x, r.layers.label), chosen || hov ? [255, 255, 255, 1] : null, false, false, takenPiece || takenTrip ? 0.35 : 1);
      const svc = SV.at(g.svc, t.id, x.dep);
      SV.mark(ctx, cr, svc);
      if (cr.w > 24 && cr.x > g.layout.gx - 4 && cr.x + cr.w < g.layout.gx + g.layout.gw + 4) an('rf-chip', cr.x, cr.y, cr.w, cr.h);
      if (readOnly || hand) continue;
      const pt = partTip(x);
      hit(cr.x, cr.y, cr.w, cr.h, { kind: 'chip', tour: sp.tour, trip: j, row: row.id, dutyRow: d, piece: [sp.tour, sp.duty], ...(r.tripsToDuties ? { tip: TRIP_TIP } : {}), ...(pt ? { tip: r.tripsToDuties ? `${pt}\n${tr(TRIP_TIP)}` : pt, raw: true } : {}), ...(svc ? { tip: SV.tipOf(svc), raw: true } : {}) });
    }
  }
  if (!hand) dutyRest(ctx, g, s, d, cy, xOf, hit, lineCol);
  else dutyRest(ctx, g, s, d, cy, xOf, () => {}, lineCol);
  return lit;
}

/** A duty's breaks and driving periods (testers: "when you define for it to show you periods
 * between a break it does not tell me so its a bit of guesswork"), as the core counts them for
 * the issues (`rosterflow::break_plan`): each break labelled in its gap with its kind and length
 * ("Break 0:35", "Meal 0:45", "Split 3:10"; only its length where the gap is narrow, a tick where
 * narrower still), green where it counts for the meal or continuous rule, grey where it is too
 * short to; under the row the driving periods between the breaks that end one, each with its
 * length, red (amber for a soft rule) over the Continuous Break limit, and a line where the duty
 * works on without its meal break. A hover tells what each keeps or breaks, a click opens its
 * card. */
function dutyRest(ctx, g, s, d, cy, xOf, hit, lineCol) {
  const { p } = g;
  const rest = s.rest;
  if (!rest) return;
  const tone = (hard) => (hard ? p.err : p.warn);
  rest.breaks.forEach((bk, k) => {
    const xa = xOf(bk.from);
    const xb = xOf(bk.to);
    const room = xb - xa - 4;
    const col = bk.kind === 'split' ? lineCol : bk.counts ? p.ok : alpha(p.text, p.light ? 0.45 : 0.5);
    hit(xa, cy - CHIP / 2, Math.max(4, xb - xa), CHIP, { kind: 'rest', what: 'break', row: d, k, tip: bk.tip, raw: true, pre: true });
    const text = [bk.label, bk.short].find((t) => D.width(ctx, t, 9.5, 700) + 10 <= room);
    if (!text) {
      // (too narrow for words: a tick in the gap, its card on hover)
      if (xb - xa >= 3) D.rect(ctx, (xa + xb) / 2 - 0.75, cy - 4, 1.5, 8, col);
      return;
    }
    const tw = D.width(ctx, text, 9.5, 700) + 10;
    const x = (xa + xb) / 2 - tw / 2;
    D.rounded(ctx, x, cy - 7, tw, 14, 7, p.grid);
    D.roundedBorder(ctx, x, cy - 7, tw, 14, 7, 1.1, col);
    D.textAt(ctx, text, x + tw / 2, cy + 3.5, 9.5, 700, col, 'center');
  });
  if (S.rf.layers.periods === false) return;
  // the driving periods, in the band under the row
  const by = cy + ROW / 2;
  const calm = alpha(p.text, p.light ? 0.5 : 0.55);
  rest.periods.forEach((st, k) => {
    const xa = xOf(st.from);
    const xb = xOf(st.to);
    if (xb - xa < 1) return;
    const col = st.over ? tone(rest.period_hard) : calm;
    // (the length in a lighter tone of the colour on the dark look: red on red is hard to read)
    const ink = st.over ? (p.light ? col : mix(col, [255, 255, 255, 1], 0.4)) : alpha(p.text, p.light ? 0.68 : 0.72);
    D.rect(ctx, xa + 0.5, by, Math.max(1, xb - xa - 1), BAND - 3, alpha(st.over ? col : p.text, st.over ? 0.14 : 0.06));
    D.rect(ctx, xa + 0.5, by, Math.max(1, xb - xa - 1), 1.5, col);
    if (D.width(ctx, st.text, 9, 700) + 6 <= xb - xa) D.textAt(ctx, st.text, (xa + xb) / 2, by + BAND - 3.5, 9, 700, ink, 'center');
    hit(xa, by, xb - xa, BAND - 2, { kind: 'rest', what: 'period', row: d, k, tip: st.tip, raw: true, pre: true });
  });
  // (the meal break missing: from where it was due to the late meal break, or the duty's end)
  const meal = rest.meal;
  if (meal) {
    const xa = xOf(meal.from);
    const xb = Math.max(xOf(meal.to), xa + 3);
    const col = tone(rest.meal_hard);
    D.rect(ctx, xa, by + BAND - 3.5, xb - xa, 2, col);
    D.rect(ctx, xa - 0.75, by + 1, 1.5, BAND - 3, col);
    hit(xa - 3, by + BAND - 6, xb - xa + 6, 6, { kind: 'rest', what: 'meal', row: d, k: 0, tip: meal.tip, raw: true, pre: true });
  }
}

// --- the canvas ---------------------------------------------------------------------------------------

/** A Drivers chip's tooltip once single trips go to another duty (`rf_gantt::duty_row_draw`). */
const TRIP_TIP = 'Drag or double-click to move this trip to another duty or a new one: the bus stays the same. Shift or Ctrl and a click choose more trips. Alt and a drag takes the whole piece of work on this bus.';

/** The chips of a duty's row in order of departure: (block, trip) of every trip with passengers
 * (`rf_gantt::chips_of`). */
export function dutyChips(b, d) {
  const s = b.duties[d];
  const out = [];
  for (const sp of s?.spans || []) {
    const t = b.blocks[sp.tour];
    const pr = t?.pieces[sp.duty];
    if (!pr) continue;
    for (let j = pr[0]; j < pr[1]; j++) if (t.trips[j]?.counts) out.push([sp.tour, j]);
  }
  return out.sort((x, y) => b.blocks[x[0]].trips[x[1]].dep - b.blocks[y[0]].trips[y[1]].dep || x[0] - y[0]);
}

/** The chips of a duty's row from `a` to `z` (both in, either way round): Shift and a click. */
function dutySpan(b, d, a, z) {
  const chips = dutyChips(b, d);
  const i = chips.findIndex((x) => x[0] === a[0] && x[1] === a[1]);
  const k = chips.findIndex((x) => x[0] === z[0] && x[1] === z[1]);
  return i < 0 || k < 0 ? [z] : chips.slice(Math.min(i, k), Math.max(i, k) + 1);
}

/** The trips of block `tour` from `a` to `b` (both in), with passengers. */
function rangeOf(b, tour, a, z) {
  const t = b.blocks[tour];
  const out = [];
  if (!t) return out;
  for (let k = Math.min(a, z); k <= Math.max(a, z); k++) if (t.trips[k]?.counts) out.push(k);
  return out;
}

/** The Gantt of the Vehicles (`drivers` false) or the Drivers tab (`rf_gantt::gantt`). `onTip`
 * says the tooltip to show; `onOpen` what a click opens over it (a card, the menu, the duty
 * panel), at the page's place. */
export function Gantt({ data, drivers, readOnly, p, onTip, page }) {
  const box = useRef(null);
  const cv = useRef(null);
  const g = useRef({ mouse: null, hits: [], owner: {}, down: null, barDrag: null, barHot: null, raf: 0 }).current;
  g.data = data;
  g.drivers = drivers;
  g.readOnly = readOnly;
  g.p = p;
  g.page = page;

  const paint = () => {
    g.raf = 0;
    const c = cv.current;
    const el = box.current;
    if (!c || !el) return;
    const w = el.clientWidth;
    const h = el.clientHeight;
    if (!w || !h) return;
    g.origin = { x: el.offsetLeft + (el.offsetParent?.offsetLeft || 0), y: el.offsetTop + (el.offsetParent?.offsetTop || 0) };
    const pageEl = el.closest('.rf');
    if (pageEl) {
      const pb = pageEl.getBoundingClientRect();
      const eb = el.getBoundingClientRect();
      const k = pageEl.offsetWidth ? pb.width / pageEl.offsetWidth : 1;
      g.origin = { x: (eb.left - pb.left) / k, y: (eb.top - pb.top) / k };
      g.k = k;
      // (the cards, the menu and the tips lie in the studio's own units: its top left 0, 0)
      const st = el.closest('.rf-studio')?.getBoundingClientRect() || pb;
      g.cardOrigin = { x: (eb.left - st.left) / k, y: (eb.top - st.top) / k };
    }
    begin(g.owner);
    const ctx = D.prepare(c, w, h);
    g.revealed = null;
    const out = draw(ctx, w, h, g);
    g.hits = out.hits;
    // (what is where, for the checks and the pictures: `shots.js` drags with the mouse so)
    c.rfHits = out.hits;
    // (a row asked for, scrolled to: kept, and asked no more)
    if (S.rf.reveal) setRf({ reveal: null, scroll: g.revealed ?? S.rf.scroll });
    else if (g.scroll !== S.rf.scroll || g.pan !== S.rf.pan) {
      S.rf.scroll = g.scroll;
      S.rf.pan = g.pan;
    }
  };
  const again = () => {
    if (!g.raf) g.raf = requestAnimationFrame(paint);
  };
  g.again = again;
  useEffect(() => {
    paint();
  });
  useEffect(() => {
    const ro = new ResizeObserver(() => again());
    ro.observe(box.current);
    return () => {
      ro.disconnect();
      forget(g.owner);
      cancelAnimationFrame(g.raf);
    };
  }, []);

  const local = (e) => {
    const b = cv.current.getBoundingClientRect();
    const k = cv.current.offsetWidth ? b.width / cv.current.offsetWidth : 1;
    return { x: (e.clientX - b.left) / k, y: (e.clientY - b.top) / k };
  };
  const top = (m) => {
    for (let k = g.hits.length - 1; k >= 0; k--) if (inside(g.hits[k], m)) return g.hits[k];
    return null;
  };
  const at = (m) => ({ x: (g.cardOrigin || g.origin).x + m.x, y: (g.cardOrigin || g.origin).y + m.y });

  const showTip = (hit, m) => {
    if (!onTip) return;
    // (a Drivers chip's long tip keeps quiet while a card or the menu is open)
    if (hit?.tip && !(hit.kind === 'chip' && (S.rf.card || S.rf.menu))) onTip({ text: hit.raw ? hit.tip : tr(hit.tip), pre: !!hit.pre, ...at(m) });
    else onTip(null);
  };

  const move = (e) => {
    const m = local(e);
    g.mouse = m;
    const r = S.rf;
    if (g.barDrag) {
      const d = g.barDrag;
      const v = d.bar.vertical ? m.y : m.x;
      const pos = Math.max(0, Math.min(1, (v - d.grab - d.bar.start) / d.bar.room)) * d.bar.max;
      if (d.which === 'v') S.rf.scroll = pos;
      else S.rf.pan = Math.round(pos);
      again();
      return;
    }
    if (r.press && e.buttons & 1) {
      if (Math.hypot(m.x - r.press.x, m.y - r.press.y) > 7) {
        const hand = r.press.hand;
        S.rf.press = null;
        pick(hand, true);
      }
    }
    const hb = g.bars;
    g.barHot = hb?.v && inside(hb.v.thumb, m) ? 'v' : hb?.h && inside(hb.h.thumb, m) ? 'h' : null;
    const hit = top(m);
    // the drop zone under the mouse (its card)
    const zi = hit?.kind === 'zone' ? hit.zi : null;
    if (zi !== S.rf.zoneHover) setRf({ zoneHover: zi, zoneAt: zi != null ? at(m) : null });
    else if (zi != null) S.rf.zoneAt = at(m);
    showTip(hit, m);
    cv.current.style.cursor = hit && hit.kind !== 'none' ? 'pointer' : 'default';
    again();
  };

  const down = (e) => {
    if (e.button !== 0) return;
    const m = local(e);
    g.mouse = m;
    // the scrollbars: the thumb taken, the track paged
    for (const which of ['v', 'h']) {
      const bar = g.bars?.[which];
      if (!bar) continue;
      if (inside(bar.thumb, m)) {
        const v = bar.vertical ? m.y : m.x;
        g.barDrag = { which, bar, grab: v - (bar.vertical ? bar.thumb.y : bar.thumb.x) };
        return;
      }
      if (inside(bar.track, m)) {
        const v = bar.vertical ? m.y : m.x;
        const page = bar.view * 0.9;
        const atv = bar.vertical ? bar.thumb.y : bar.thumb.x;
        const pos = v < atv ? Math.max(0, bar.pos - page) : Math.min(bar.max, bar.pos + page);
        if (which === 'v') setRf({ scroll: pos });
        else setRf({ pan: Math.round(pos) });
        return;
      }
    }
    const hit = top(m);
    g.down = hit;
    const r = S.rf;
    // (pressed on nothing: the card and the menu close, the choice goes; a hand stays held)
    if (!hit) {
      if (r.card || r.menu || (r.sel.length && !r.hand)) setRf({ card: null, menu: null, ...(r.hand ? {} : { sel: [], selRow: null }) });
      return;
    }
    const b = g.data.board;
    // (pressed on a chip: chosen now, taken up if the mouse moves on, or clicked twice)
    if (hit.kind === 'chip') {
      const now = performance.now() / 1000;
      const key = `${hit.tour}/${hit.trip}`;
      const twice = r.lastClick && r.lastClick.key === key && now - r.lastClick.t < 0.4;
      const patch = { lastClick: { key, t: now }, menu: null };
      let sel;
      const several = !drivers || r.tripsToDuties;
      const inSel = r.sel.some(([s0, s1]) => s0 === hit.tour && s1 === hit.trip);
      // (in Drivers a choice is trips of one duty, on one bus or several)
      const sameRow = r.selRow === hit.row && r.sel[0] && (drivers || r.sel[0][0] === hit.tour);
      if (e.shiftKey && several && sameRow && (drivers || r.sel[0][0] === hit.tour)) {
        // (along the row: in Drivers the duty's trips, of any bus)
        sel = drivers ? dutySpan(b, hit.dutyRow, r.sel[0], [hit.tour, hit.trip]) : rangeOf(b, hit.tour, r.sel[0][1], hit.trip).map((x) => [hit.tour, x]);
        Object.assign(patch, { sel, card: { kind: 'selection', at: at(m) } });
      } else if ((e.ctrlKey || e.metaKey) && several && sameRow) {
        // (Ctrl and a click - Cmd on a Mac, whose Ctrl and a click is a right click: one trip
        // added to the choice or taken out of it)
        sel = inSel ? r.sel.filter(([s0, s1]) => !(s0 === hit.tour && s1 === hit.trip)) : [...r.sel, [hit.tour, hit.trip]];
        // (taken out: nothing taken up, as the game-drawn launcher does)
        if (inSel) {
          setRf({ ...patch, sel, selRow: sel.length ? r.selRow : null, card: sel.length > 1 ? { kind: 'selection', at: at(m) } : null });
          return;
        }
        Object.assign(patch, { sel, card: { kind: 'selection', at: at(m) } });
      } else if (inSel && r.sel.length > 1 && several) {
        // (a chip of the choice pressed: the whole choice comes along)
        sel = r.sel;
      } else {
        sel = [[hit.tour, hit.trip]];
        Object.assign(patch, { sel, selRow: hit.row, card: { kind: 'trip', tour: hit.tour, trip: hit.trip, at: at(m), assign: false } });
      }
      const hand = tripHand(b, drivers, hit.tour, sel.filter((x) => x[0] === hit.tour).map((x) => x[1]), hit.trip, hit.dutyRow ?? null, sel, e.altKey);
      if (twice) {
        setRf(patch);
        pick(hand, false);
      } else setRf({ ...patch, press: { x: m.x, y: m.y, hand } });
      return;
    }
    if (hit.kind === 'stack') {
      const hand = { kind: 'stack', k: hit.k };
      pick(hand, false).then(() => setRf({ card: { kind: 'stack', k: hit.k, at: at(m) }, sel: [] }));
      return;
    }
    if (hit.kind === 'event') {
      setRf({ menu: null, card: { kind: 'event', row: hit.row, ev: hit.ev, at: at(m) } });
    }
  };

  const up = (e) => {
    if (e.button !== 0) return;
    const m = local(e);
    if (g.barDrag) {
      g.barDrag = null;
      setRf({ scroll: S.rf.scroll, pan: S.rf.pan });
      return;
    }
    const r = S.rf;
    const hit = top(m);
    if (r.press) setRf({ press: null });
    if (r.dragging) {
      if (hit?.kind === 'zone') dropAt(hit.zi);
      else setRf({ hand: null, zones: null, dragging: false });
      return;
    }
    const was = g.down;
    g.down = null;
    if (!hit || !was || was !== hit) return;
    click(hit, m);
  };

  const click = (hit, m) => {
    const r = S.rf;
    const b = g.data.board;
    switch (hit.kind) {
      case 'tickall':
        setRf({ ticked: hit.ids });
        break;
      case 'tick': {
        const t = r.ticked.includes(hit.id) ? r.ticked.filter((x) => x !== hit.id) : [...r.ticked, hit.id];
        setRf({ ticked: t });
        break;
      }
      case 'stathead': {
        const s = r.sort;
        setRf({ sort: s && s[0] === hit.stat ? (s[1] ? [hit.stat, false] : null) : [hit.stat, true] });
        break;
      }
      case 'id':
        if (hit.row.kind === 'block') setRf({ menu: null, card: { kind: 'vehicle', tour: hit.row.i, at: at({ x: ID_W - 24, y: hit.cy }) } });
        else setRf({ panel: { kind: 'duty', id: hit.row.id }, selRow: hit.row.id, card: null });
        break;
      case 'lock':
        if (hit.row.kind === 'block') edit('lock_block', { tour: hit.row.i });
        else edit('lock_duty', { row: hit.row.i });
        break;
      case 'pin': {
        const pinned = r.pinned.includes(hit.id) ? r.pinned.filter((x) => x !== hit.id) : [hit.id, ...r.pinned];
        setRf({ pinned });
        break;
      }
      case 'swap':
        if (r.hand?.kind === 'swap') {
          const h = r.hand;
          edit('swap', { tour: h.tour, trips: h.trips, to: hit.i, how: h.how }).then((x) => !x?.edited && setRf({ hand: null }));
        }
        break;
      case 'zone':
        dropAt(hit.zi);
        break;
      case 'relief':
        edit('uncut', { tour: hit.tour, at: hit.at });
        break;
      case 'gap':
        if (hit.ok) edit('cut', { tour: hit.tour, at: hit.at });
        break;
      case 'mid':
        // (cut inside a trip, at a relief point on the way)
        if (hit.ok) edit('cut_mid', { tour: hit.tour, trip: hit.trip, stop: hit.stop, name: hit.name, time: hit.time });
        break;
      case 'spare':
        edit('spare');
        break;
      case 'rest':
        if (onTip) onTip(null);
        setRf({ menu: null, card: { kind: 'rest', row: hit.row, what: hit.what, k: hit.k, at: at(m) } });
        break;
      default:
    }
    void b;
  };

  const context = (e) => {
    e.preventDefault();
    const m = local(e);
    const hit = top(m);
    if (!hit) return;
    const r = S.rf;
    if (hit.kind === 'chip') {
      const sel = r.sel.some(([a, z]) => a === hit.tour && z === hit.trip) ? r.sel : [[hit.tour, hit.trip]];
      setRf({ card: null, sel, menu: { at: at(m), tour: hit.tour, trip: hit.trip, dutyRow: hit.dutyRow ?? null, piece: hit.piece || null, custom: null } });
    } else if (hit.kind === 'event' && hit.custom != null) {
      setRf({ card: null, menu: { at: at(m), tour: null, trip: null, dutyRow: hit.row, piece: null, custom: [hit.row, hit.custom] } });
    }
  };

  const wheel = (e) => {
    e.preventDefault();
    const m = local(e);
    const L = g.layout;
    if (!L) return;
    const r = S.rf;
    const notch = Math.max(-3, Math.min(3, -e.deltaY / 100)) || (e.deltaY < 0 ? 1 : -1);
    // (Ctrl and the wheel are RosterFlow's own here, never the launcher's size: `zoom.js`;
    // Cmd on a Mac)
    if ((e.ctrlKey || e.metaKey) && e.deltaY) {
      // zoomed about the minute under the mouse
      const atMin = L.t0 + L.pan + (m.x - L.gx) / L.ppm;
      const zoom = Math.max(1, Math.min(8, r.zoom * (e.deltaY < 0 ? 1.25 : 0.8)));
      const ppm = L.gw / (L.total / zoom);
      setRf({ zoom, pan: Math.round(atMin - L.t0 - (m.x - L.gx) / ppm) });
    } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      const d = Math.abs(e.deltaX) > 0 ? e.deltaX / 100 : -notch;
      setRf({ pan: Math.round(r.pan + (d * L.total) / r.zoom / 12) });
    } else {
      setRf({ scroll: Math.max(0, Math.min(Math.max(0, L.contentH - L.body.h), r.scroll - notch * rowPitch(g.drivers) * 2)) });
    }
  };

  const leave = () => {
    g.mouse = null;
    if (onTip) onTip(null);
    if (S.rf.zoneHover != null) setRf({ zoneHover: null });
    again();
  };

  // (a drag let go outside the canvas: put back)
  useEffect(() => {
    const u = (e) => {
      if (S.rf.dragging && e.target !== cv.current) setRf({ hand: null, zones: null, dragging: false });
      if (g.barDrag) {
        g.barDrag = null;
        setRf({ scroll: S.rf.scroll, pan: S.rf.pan });
      }
    };
    const mv = (e) => {
      if (g.barDrag || S.rf.dragging) move(e);
    };
    addEventListener('mouseup', u);
    addEventListener('mousemove', mv);
    return () => {
      removeEventListener('mouseup', u);
      removeEventListener('mousemove', mv);
    };
  }, []);

  return html`<div class="rf-gantt" ref=${box} data-rf="rf-gantt">
    <canvas ref=${cv} onMouseMove=${move} onMouseDown=${down} onMouseUp=${up} onContextMenu=${context} onWheel=${wheel} onMouseLeave=${leave}></canvas>
  </div>`;
}

/** The arrow keys (`rf_gantt::arrow_key`): along the row to the next or previous trip (Shift:
 * the choice widens), up and down to the nearest trip of the row above or below. */
export function arrowKey(data, drivers, key, shift) {
  const r = S.rf;
  const b = data.board;
  const last = r.sel[r.sel.length - 1];
  if (!last) return;
  const [tour, trip] = last;
  const rows = rowsOf(data, drivers);
  const ri = rows.findIndex((x) => x.id === r.selRow);
  if (ri < 0) return;
  const chipsOf = (row) => (row.kind === 'block' ? b.blocks[row.i].trips.map((x, j) => (x.counts ? [row.i, j] : null)).filter(Boolean) : dutyChips(b, row.i));
  const here = chipsOf(rows[ri]);
  const atK = Math.max(0, here.findIndex(([a, z]) => a === tour && z === trip));
  const dep = b.blocks[tour]?.trips[trip]?.dep;
  if (dep == null) return;
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    const n = key === 'ArrowLeft' ? atK - 1 : atK + 1;
    if (n < 0 || n >= here.length) return;
    // (in Drivers a row is one duty: its trips are chosen together, of any bus)
    if (shift && ((drivers && r.tripsToDuties) || (!drivers && here[n][0] === tour))) {
      const sel = r.sel.some(([a, z]) => a === here[n][0] && z === here[n][1]) ? r.sel : [...r.sel, here[n]].sort((x, y) => x[0] - y[0] || x[1] - y[1]);
      setRf({ sel, card: null });
    } else setRf({ sel: [here[n]], card: null });
  } else {
    const o = key === 'ArrowUp' ? ri - 1 : ri + 1;
    if (o < 0 || o >= rows.length) return;
    const there = chipsOf(rows[o]);
    if (!there.length) return;
    const best = there.reduce((acc, c) => (Math.abs(b.blocks[c[0]].trips[c[1]].dep - dep) < Math.abs(b.blocks[acc[0]].trips[acc[1]].dep - dep) ? c : acc));
    setRf({ sel: [best], selRow: rows[o].id, reveal: rows[o].id, card: null });
  }
}

export { inkOn };

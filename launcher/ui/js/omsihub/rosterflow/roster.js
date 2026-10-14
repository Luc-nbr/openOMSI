// RosterFlow's Roster tab (`crates/omsi-launcher-ui/src/company/planning/rf_roster.rs`): the
// planning's weekly roster drawn as the rest of RosterFlow is. Under the title bar the next seven
// days as RosterFlow's tabs, each with whether it is covered; the toolbar with what the roster
// repeats and the tools (repeat, clear the day, fill the roster); then the day's tours as
// RosterFlow's Gantt on a canvas - its axis and hour bands, its rows with the ID column (the tour,
// its bus) - grouped by line under a heading that folds the line away and opens its timetable. A
// tour's duties lie on the hours in the colour of who drives them; what is open reads as
// RosterFlow's issues. Beside the chart the day, a duty or a tour, with the drivers and buses to
// give by tapping or dragging; on the company's day "Drive this duty" opens the company's duty
// launcher with exactly that duty. The roster's rules are the Rust side's (`rf_roster*`).

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { act as coAct, setCo } from '../company/kit.js';
import { alpha, mix, hex, tone, css, lighten } from './pal.js';
import * as D from './draw.js';
import { AXIS } from './draw.js';
import { setRf, setPart, rfCmd, after, bump, say, TIMEPLAN } from './state.js';
import { Icon, RfButton, IconBtn } from './ui.js';
import { anchor, begin, forget } from './anchors.js';
import { BAR, barLook, drawBar } from './gantt.js';

const DAY_H = 50;
const TOOL_H = 40;
const HEAD = 34;
const LEFT = 236;
const SIDE_HEAD = 44;
const ROW_MIN = 27;
const ROW_MAX = 38;

const rsSet = (patch) => setPart('roster', patch);
const inside = (r, m) => m && m.x >= r.x && m.x < r.x + r.w && m.y >= r.y && m.y < r.y + r.h;

/** The pitch of a tour's row: the room shared by `rows`, between the least and the most. */
function rowPitch(room, rows) {
  return rows ? Math.max(ROW_MIN, Math.min(ROW_MAX, Math.floor(room / rows))) : ROW_MAX;
}

/** An action of the Roster tab (`rf_roster_act`) on the day shown. */
async function rAct(act, args = {}) {
  let r;
  try {
    r = await rfCmd('rf_roster_act', { act, day: S.rf.day, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  bump();
  return r;
}

/** Escape on the Roster tab: the driver or bus taken up, what is chosen. */
export function rosterEscape() {
  const s = S.rf.roster;
  if (s.arm || s.sel || s.clear) {
    rsSet({ arm: null, sel: null, clear: false });
    return true;
  }
  return !!S.co.dialog;
}

/** The tab's view (`rf_roster`), asked again when the day, the choice or the company changed. */
function useRoster() {
  const r = S.rf;
  const sel = r.roster.sel;
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  useEffect(() => {
    let gone = false;
    rfCmd('rf_roster', { day: r.day, sel })
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ error: e.message }));
    return () => {
      gone = true;
    };
  }, [r.day, JSON.stringify(sel), gen, r.bump, language()]);
  return v;
}

/** Give what is armed or dragged (`planning::give`): a driver to a duty, a bus to a tour. */
function give(arm, hit) {
  if (!arm || !hit) return;
  if (arm.kind === 'driver' && hit.duty != null) rAct('give_driver', { ti: hit.ti, k: hit.duty, who: arm.who });
  else if (arm.kind === 'bus') rAct('give_bus', { ti: hit.ti, bus: arm.bus });
}

const armIs = (a, b) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b);

export function RosterTab({ p }) {
  const r = S.rf;
  const v = useRoster();
  const [drag, setDrag] = useState(null);
  const tab = useRef(null);
  // (a duty or tour carried to the list, a driver or bus dragged to the chart: at the mouse)
  useEffect(() => {
    if (!drag) return undefined;
    const mv = (e) => setDrag((d) => d && { ...d, x: e.clientX, y: e.clientY, moved: d.moved || Math.hypot(e.clientX - d.x0, e.clientY - d.y0) > 6 });
    const up = (e) => {
      const d = drag;
      setDrag(null);
      if (!d.moved) return;
      const el = document.elementFromPoint(e.clientX, e.clientY);
      if (d.from === 'chart') {
        const t = el?.closest?.('[data-arm]');
        if (!t) return;
        const arm = JSON.parse(t.dataset.arm);
        if (arm.kind === 'driver' && d.hit.duty == null) return;
        give(arm, d.hit);
      } else if (d.from === 'list') {
        const hit = window.__rfRosterHit?.(e.clientX, e.clientY);
        if (hit) give(d.arm, hit);
      }
    };
    addEventListener('mousemove', mv);
    addEventListener('mouseup', up);
    return () => {
      removeEventListener('mousemove', mv);
      removeEventListener('mouseup', up);
    };
  }, [drag?.from, drag?.x0]);
  if (!v) return html`<div class="rf-roster" ref=${tab}></div>`;
  return html`<div class="rf-roster" ref=${tab}>
    <${DayTabs} v=${v} />
    <${Tools} v=${v} />
    <div class="rr-body">
      ${v.reading || v.error
        ? html`<div class="rr-wait"><p class=${v.error ? 'err' : ''}>${v.error || v.reading}</p></div>`
        : html`<${Chart} v=${v} p=${p} startDrag=${(d) => setDrag(d)} carrying=${drag?.from === 'chart' && drag.moved ? drag.hit : null} />
          <${Side} v=${v} startDrag=${(d) => setDrag(d)} carrying=${drag?.from === 'chart' && drag.moved ? drag.hit : null} />`}
    </div>
    ${drag?.moved && html`<div class="rr-carry" style=${carryStyle(drag)}>${drag.label}</div>`}
    ${S.rf.service && html`<${ServiceDialog} d=${S.rf.service} />`}
  </div>`;
}

/** Where the carried chip lies, in the tab's units (the stage is scaled). */
function carryStyle(d) {
  const el = document.querySelector('.rf-roster');
  if (!el) return '';
  const b = el.getBoundingClientRect();
  const k = el.offsetWidth ? b.width / el.offsetWidth : 1;
  return `left:${(d.x - b.left) / k + 10}px;top:${(d.y - b.top) / k + 6}px;${d.fill ? `background:${d.fill};color:${d.ink}` : ''}`;
}

/** The next seven days as RosterFlow's tabs (`rf_roster::day_tabs`). */
function DayTabs({ v }) {
  const r = S.rf;
  return html`<div class="rr-days" data-rf="rf-roster-week">${v.days.map((d, k) => html`<button type="button" class=${r.day === k ? 'on' : ''} title=${d.status?.tip || undefined}
      onClick=${() => r.day !== k && (setRf({ day: k }), rsSet({ sel: null, clear: false }))}>
      <span class="caps">${d.caps}</span><b class="cut">${d.big}</b>${d.status && html`<i class=${'dot ' + d.status.tone}></i>`}</button>`)}</div>`;
}

/** The toolbar: what the roster is (or what the fleet lacks), and the tools. */
function Tools({ v }) {
  const s = S.rf.roster;
  return html`<div class="rr-tools" data-rf="rf-roster-tools">
    <span class=${'what ' + v.tools.tone} title=${v.tools.text}><${Icon} name=${v.tools.icon} size=${16} /><span class="cut">${v.tools.text}</span></span>
    <${RfButton} label=${v.repeat} raw title="This weekday's roster on the other days too (theirs is replaced)" onClick=${() => rAct('repeat')} />
    <${RfButton} label=${s.clear ? 'Press again' : 'Clear the day'} title="Take every bus and driver of this weekday out of the roster" style=${s.clear ? 'box-shadow:inset 0 0 0 1.5px var(--rf-err)' : ''} onClick=${() => {
      if (s.clear) {
        rsSet({ clear: false });
        rAct('clear');
      } else rsSet({ clear: true });
    }} />
    <${RfButton} label="Fill the roster" solid title="Fix in the roster what the dispatcher would take: free buses, and free drivers by the working-time rules. Only what is planned runs." onClick=${() => rAct('fill')} />
  </div>`;
}

// --- the chart ---------------------------------------------------------------------------------------

/** What is open, as RosterFlow's issues: an outlined chip in the warning (or error) colour with
 * its icon, its words as the room allows (`rf_roster::issue_chip`). */
function issueChip(ctx, x, y, w, h, col, icon, words, hover) {
  D.rounded(ctx, x, y, w, h, 3, alpha(col, hover ? 0.22 : 0.13));
  D.roundedBorder(ctx, x, y, w, h, 3, 1.2, col);
  const ih = Math.max(11, Math.min(14, h * 0.62));
  if (w >= ih + 8) D.icon(ctx, icon, x + 4 + ih / 2, y + h / 2, ih, col);
  const tx = x + ih + 9;
  if (x + w - tx > 16 && words) D.textIn(ctx, words, tx, y, x + w - tx - 5, h, 11.5, 700, col);
}

/** The colour of a duty's fill: a driver's, or the look's accent or grey. */
function fillOf(p, f) {
  if (f === 'accent') return p.accent;
  if (f === 'grey') return p.grey;
  return hex(f);
}

function Chart({ v, p, startDrag, carrying }) {
  const box = useRef(null);
  const cv = useRef(null);
  const g = useRef({ mouse: null, hits: [], owner: {}, scroll: 0, taps: null, press: null }).current;
  g.v = v;
  g.p = p;
  const paint = () => {
    g.raf = 0;
    const el = box.current;
    const c = cv.current;
    if (!el || !c || !el.clientWidth) return;
    const pageEl = el.closest('.rf');
    if (pageEl) {
      const pb = pageEl.getBoundingClientRect();
      const eb = el.getBoundingClientRect();
      const k = pageEl.offsetWidth ? pb.width / pageEl.offsetWidth : 1;
      g.origin = { x: (eb.left - pb.left) / k, y: (eb.top - pb.top) / k };
    }
    begin(g.owner);
    anchor('rf-roster-chart', { x: g.origin?.x || 0, y: g.origin?.y || 0, w: el.clientWidth, h: el.clientHeight }, g.owner);
    const ctx = D.prepare(c, el.clientWidth, el.clientHeight);
    g.hits = drawChart(ctx, el.clientWidth, el.clientHeight, g);
  };
  const again = () => {
    if (!g.raf) g.raf = requestAnimationFrame(paint);
  };
  useEffect(() => paint());
  useEffect(() => {
    const ro = new ResizeObserver(() => again());
    ro.observe(box.current);
    // (a driver or bus dragged from the list and let go over the chart: what lies under it)
    window.__rfRosterHit = (cx, cy) => {
      const b = cv.current?.getBoundingClientRect();
      if (!b) return null;
      const k = cv.current.offsetWidth ? b.width / cv.current.offsetWidth : 1;
      const m = { x: (cx - b.left) / k, y: (cy - b.top) / k };
      return g.hits.filter((h) => h.kind === 'bus' || h.kind === 'duty').find((h) => inside(h, m)) || null;
    };
    return () => {
      ro.disconnect();
      forget(g.owner);
      window.__rfRosterHit = null;
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
  const move = (e) => {
    g.mouse = local(e);
    const h = top(g.mouse);
    cv.current.style.cursor = h && h.kind !== 'none' ? 'pointer' : 'default';
    cv.current.title = h?.tip || '';
    again();
  };
  const down = (e) => {
    if (e.button !== 0) return;
    const m = local(e);
    const h = top(m);
    g.down = h;
    // (pressed on a duty or a tour's bus: taken up once the mouse moves)
    if (h && (h.kind === 'bus' || h.kind === 'duty') && !S.rf.roster.arm) {
      const t = g.v.plan.tours[h.ti];
      const title = `${tr('Line')} ${t.number}  ·  ${tr('Tour')} ${t.tour}${h.duty != null ? `  ·  ${tr('Duty')} ${h.duty + 1}` : ''}`;
      startDrag({ from: 'chart', hit: { ti: h.ti, duty: h.duty }, label: title, x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, moved: false });
    }
  };
  const up = (e) => {
    const m = local(e);
    const h = top(m);
    const was = g.down;
    g.down = null;
    if (!h || h !== was) return;
    click(h);
  };
  const click = (h) => {
    const s = S.rf.roster;
    const pl = g.v.plan;
    switch (h.kind) {
      case 'fold':
        rAct('fold', { line: h.line });
        break;
      case 'fold_all':
        rAct('fold_all', { lines: pl.groups.map((x) => x.name), all: h.all });
        break;
      case 'timetable':
        setRf({ view: TIMEPLAN, tpLine: h.line, tpHint: null });
        break;
      case 'start':
        setRf({ service: { line: h.line, when: 0, date: S.rf.data?.head?.date || '', gaps: false } });
        break;
      case 'stop':
        rAct('stop', { line: h.line });
        break;
      case 'bus':
      case 'duty': {
        const t = pl.tours[h.ti];
        // (tapped twice soon after: the line's timetable)
        const key = `${h.kind}-${h.ti}-${h.duty}`;
        const now = performance.now() / 1000;
        if (g.taps && g.taps[0] === key && now - g.taps[1] < 0.45) {
          g.taps = null;
          setRf({ view: TIMEPLAN, tpLine: t.line, tpHint: null });
          return;
        }
        g.taps = [key, now];
        if (s.arm && !t.by_player) {
          give(s.arm, h);
          return;
        }
        const sel = h.duty != null ? { duty: [t.line, t.tour, h.duty] } : { tour: [t.line, t.tour] };
        rsSet({ sel: armIs(s.sel, sel) ? null : sel });
        break;
      }
      default:
    }
  };
  const wheel = (e) => {
    e.preventDefault();
    const L = g.layout;
    if (!L) return;
    g.scroll = Math.max(0, Math.min(Math.max(0, L.content - L.room), g.scroll + e.deltaY * 0.42));
    again();
  };
  g.carrying = carrying;
  return html`<div class="rr-chart" ref=${box}>
    <canvas ref=${cv} onMouseMove=${move} onMouseDown=${down} onMouseUp=${up} onWheel=${wheel} onMouseLeave=${() => ((g.mouse = null), again())}></canvas>
  </div>`;
}

/** The day's tours as RosterFlow's Gantt, grouped by line (`rf_roster::gantt`). Returns where
 * the parts lie. */
function drawChart(ctx, w, h, g) {
  const { v, p, mouse: m } = g;
  const pl = v.plan;
  const s = S.rf.roster;
  const hits = [];
  const hit = (x, y, ww, hh, what) => hits.push({ x, y, w: ww, h: hh, ...what });
  D.rect(ctx, 0, 0, w, h, p.grid);
  if (pl.empty) {
    D.textIn(ctx, tr('No tours on this day: the company runs no line yet, or its lines do not run on this day.'), 24, 14, w - 48, 24, 13, 400, p.dim);
    return hits;
  }
  const t0 = pl.t0;
  const t1 = pl.t1;
  const gx = LEFT + 8;
  const gw = Math.max(60, w - LEFT - 8 - BAR - 8);
  const ppm = gw / Math.max(60, t1 - t0);
  const xOf = (mm) => gx + (mm - t0) * ppm;
  D.rect(ctx, 0, AXIS, LEFT, h - AXIS, p.labels);
  D.clip(ctx, gx, 0, gw, h);
  for (let hh = Math.floor(t0 / 60); hh < Math.floor(t1 / 60); hh++) if (hh % 2 === 1) D.rect(ctx, xOf(hh * 60), AXIS, 60 * ppm, h - AXIS, p.band);
  D.unclip(ctx);
  D.rect(ctx, 0, 0, w, AXIS, p.axis);
  D.rect(ctx, 0, AXIS - 1, w, 1, p.line);
  const every = ppm * 60 >= 50 ? 1 : ppm * 60 >= 25 ? 2 : 3;
  D.clip(ctx, gx - 24, 0, gw + 30, AXIS);
  for (let hh = Math.floor(t0 / 60); hh <= Math.floor(t1 / 60); hh++) if (hh % every === 0) D.textAt(ctx, `${String(((hh % 24) + 24) % 24).padStart(2, '0')}:00`, xOf(hh * 60), AXIS / 2 + 4, 11, 500, p.dim, 'center');
  D.unclip(ctx);
  const groups = pl.groups;
  // over the ID column in the axis: what is in hand, or "Collapse all" / "Expand all"
  if (s.arm) {
    const hint = (s.arm.kind === 'driver' ? tr('Tap a duty to give it to %{who} (Esc: stop).') : tr("Tap a tour's bus to give it %{who} (Esc: stop).")).replace('%{who}', s.arm.name);
    D.textIn(ctx, hint, 12, 0, LEFT - 24, AXIS, 11.5, 700, p.accent);
  } else if (groups.some((x) => x.idx.length)) {
    const all = groups.filter((x) => x.idx.length).every((x) => x.folded);
    const label = all ? 'Expand all' : 'Collapse all';
    const bw = D.width(ctx, tr(label).toUpperCase(), 11, 700) + 34;
    const br = { x: 6, y: 3, w: bw, h: AXIS - 6 };
    const col = inside(br, m) ? p.text : p.dim;
    D.icon(ctx, all ? 'unfold_more' : 'unfold_less', br.x + 12, br.y + br.h / 2, 15, col);
    D.capsIn(ctx, label, br.x + 24, br.y, br.w - 24, br.h, col);
    hit(br.x, br.y, br.w, br.h, { kind: 'fold_all', all: !all, tip: tr(all ? "Show every line's tours" : 'Fold every line to its heading: what it has planned, and what is open') });
  }
  // the rows' height: the room left by the headings, shared by the tours shown
  const room = { x: 0, y: AXIS, w, h: Math.max(0, h - AXIS) };
  const shown = groups.reduce((n, x) => n + (x.folded ? 0 : x.idx.length), 0);
  const pitch = rowPitch(room.h - 8 - groups.length * HEAD, shown);
  const content = groups.reduce((n, x) => n + HEAD + (x.folded ? 0 : x.idx.length * pitch), 0) + 8;
  g.scroll = Math.max(0, Math.min(Math.max(0, content - room.h), g.scroll));
  g.layout = { content, room: room.h };
  D.clip(ctx, 0, room.y, w, room.h);
  let y = room.y - g.scroll;
  const sel = s.sel;
  for (const grp of groups) {
    const hr = { x: 0, y, w: w - BAR, h: HEAD };
    if (y + HEAD > room.y && y < room.y + room.h) heading(ctx, p, hr, grp, m, hit);
    y += HEAD;
    if (grp.folded) continue;
    for (const ti of grp.idx) {
      const t = pl.tours[ti];
      const row = { x: 0, y, w: w - BAR, h: pitch };
      y += pitch;
      if (row.y + pitch < room.y || row.y > room.y + room.h) continue;
      const cy = row.y + pitch / 2;
      const tourOn = sel?.tour && sel.tour[0] === t.line && sel.tour[1] === t.tour;
      const rowHover = inside(row, m);
      if (t.unplanned) D.rect(ctx, gx - 6, row.y, gw + 12, row.h, alpha(p.text, 0.025));
      if (tourOn || rowHover) D.rect(ctx, gx - 6, row.y, gw + 12, row.h, alpha(p.text, tourOn ? 0.06 : 0.03));
      D.rect(ctx, 14, cy - 6, 3, 12, hex(t.colour));
      D.textIn(ctx, t.title, 22, row.y, 76, row.h, 12.5, 700, t.unplanned ? p.dim : tourOn ? p.accent : alpha(p.text, 0.9));
      const chipH = Math.max(18, Math.min(24, pitch - 8));
      const chip = { x: 100, y: cy - chipH / 2, w: LEFT - 110, h: chipH };
      const hov = inside(chip, m) || (g.carrying && g.carrying.ti === ti && g.carrying.duty == null);
      busChip(ctx, p, t, chip, hov, tourOn);
      hit(chip.x, chip.y, chip.w, chip.h, { kind: 'bus', ti, duty: null, tip: t.bus.tip || '' });
      // the duties on the hours
      const barH = Math.max(18, Math.min(26, pitch - 8));
      D.clip(ctx, gx - 4, row.y, gw + 8, row.h);
      t.duties.forEach((d, k) => {
        const br = { x: xOf(d.from), y: cy - barH / 2, w: Math.max(6, (d.to - d.from) * ppm), h: barH };
        const dh = inside(br, m);
        const chosen = sel?.duty && sel.duty[0] === t.line && sel.duty[1] === t.tour && sel.duty[2] === k;
        dutyChip(ctx, p, t, d, br, ppm, xOf, dh, chosen);
        hit(br.x, br.y, br.w, br.h, { kind: 'duty', ti, duty: k, tip: d.tip });
      });
      D.unclip(ctx);
    }
  }
  D.unclip(ctx);
  drawBar(ctx, barLook(p), { x: w - BAR, y: room.y, w: BAR, h: room.h }, true, g.scroll, room.h, content, false);
  return hits;
}

/** A line's heading (`rf_roster::line_heading`): the fold, the plate and name, what is open as
 * issue tags, its timetable, and its service. */
function heading(ctx, p, hr, grp, m, hit) {
  const cy = hr.y + hr.h / 2;
  D.rect(ctx, hr.x, hr.y, hr.w, hr.h, mix(p.labels, p.grid, 0.4));
  D.rect(ctx, hr.x, hr.y, hr.w + BAR, 1, p.line);
  D.rect(ctx, hr.x, hr.y + hr.h - 1, hr.w + BAR, 1, p.line);
  if (!grp.none) {
    const fr = { x: hr.x + 4, y: cy - 12, w: 24, h: 24 };
    D.icon(ctx, grp.folded ? 'chevron_right' : 'expand_more', fr.x + 12, cy, 18, inside(fr, m) ? p.text : p.dim);
    hit(fr.x, fr.y, fr.w, fr.h, { kind: 'fold', line: grp.name, tip: tr(grp.folded ? 'Expand the line: its tours' : 'Collapse the line to its heading') });
  }
  let x = hr.x + 32;
  const stateW = Math.min(360, hr.w * 0.36);
  // the line's timetable
  const ttLabel = tr('Timetable');
  const ttW = 26 + D.width(ctx, ttLabel, 13, 500) + 12;
  const tt = { x: hr.x + hr.w - stateW - 12 - ttW, y: hr.y + 4, w: ttW, h: hr.h - 8 };
  const th = inside(tt, m);
  if (th) D.rounded(ctx, tt.x, tt.y, tt.w, tt.h, 3, alpha(p.text, 0.06));
  D.icon(ctx, 'departure_board', tt.x + 13, cy, 16, th ? p.text : p.accent);
  D.textIn(ctx, ttLabel, tt.x + 26, tt.y, tt.w - 28, tt.h, 13, 500, th ? p.text : p.accent);
  hit(tt.x, tt.y, tt.w, tt.h, { kind: 'timetable', line: grp.name, tip: tr("The line's trips and timetable: see them and change them here") });
  // what is open on the line's tours
  const tagCol = grp.idle ? p.dim : p.warn;
  const tagsW = grp.tags.reduce((s, [t]) => s + D.width(ctx, t, 11.5, 700) + 32 + 6, 0);
  let tx = tt.x - 10;
  const withTags = tt.x - 10 - tagsW - (x + 60) >= 140;
  if (withTags) {
    for (const [t, icon] of [...grp.tags].reverse()) {
      const tw = D.width(ctx, t, 11.5, 700) + 32;
      tx -= tw;
      issueChip(ctx, tx, cy - 10, tw, 20, tagCol, icon, t, false);
      tx -= 6;
    }
  }
  if (!grp.line) {
    D.textIn(ctx, grp.name, x, hr.y, Math.max(40, tx - x - 8), hr.h, 13, 700, p.text);
    return;
  }
  // the plate: the line's number on its colour
  const col = hex(grp.line.colour);
  const pw = Math.max(30, D.width(ctx, grp.line.number, 12, 700) + 16);
  D.rounded(ctx, x, cy - 10, pw, 20, 3, col);
  D.textAt(ctx, grp.line.number, x + pw / 2, cy + 4.5, 12, 700, inkOn(col), 'center');
  x += pw + 10;
  D.textIn(ctx, grp.line.caption, x, hr.y, Math.max(40, tx - x - 8), hr.h, 13, 700, p.text);
  if (!grp.none) hit(hr.x + 30, hr.y, Math.max(40, tx - 8) - hr.x - 30, hr.h, { kind: 'fold', line: grp.name, tip: !withTags && grp.tags.length ? grp.tags.map((t) => t[0]).join('  ·  ') : '' });
  // its service
  const sr = { x: hr.x + hr.w - stateW, y: hr.y + 4, w: stateW - 6, h: hr.h - 8 };
  const sv = grp.service;
  if (!sv) return;
  if (sv.kind === 'none') D.textIn(ctx, sv.text, sr.x, sr.y, sr.w, sr.h, 12, 500, p.dim, 'right');
  else if (sv.kind === 'start') {
    const bw = D.width(ctx, sv.button.toUpperCase(), 12.5, 700) + 30;
    const b = { x: sr.x + sr.w - bw, y: sr.y, w: bw, h: sr.h };
    D.rounded(ctx, b.x, b.y, b.w, b.h, 2, inside(b, m) ? lighten(p.accent, 0.15) : p.accent);
    D.textAt(ctx, sv.button.toUpperCase(), b.x + b.w / 2, b.y + b.h / 2 + 4.5, 12.5, 700, [255, 255, 255, 1], 'center');
    hit(b.x, b.y, b.w, b.h, { kind: 'start', line: grp.line.name, tip: tr("Its tours run from the moment you choose, as they are planned; what is not covered then is dropped with the contract's penalty") });
    D.textIn(ctx, sv.note, sr.x, sr.y, sr.w - bw - 10, sr.h, 12, 500, p.dim, 'right');
  } else {
    D.textIn(ctx, sv.text, sr.x, sr.y, sr.w - 30, sr.h, 12, 500, sv.all ? p.ok : p.warn, 'right');
    const pr = { x: sr.x + sr.w - 25, y: cy - 15, w: 30, h: 30 };
    if (inside(pr, m)) D.rounded(ctx, pr.x, pr.y, pr.w, pr.h, 4, alpha(p.text, 0.08));
    D.icon(ctx, 'pause', sr.x + sr.w - 10, cy, 19, inside(pr, m) ? p.text : p.dim);
    hit(pr.x, pr.y, pr.w, pr.h, { kind: 'stop', line: grp.line.name, tip: tr('Take the line out of service') });
  }
}

function inkOn(c) {
  const lum = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255;
  return lum > 0.6 ? [26, 20, 0, 1] : [255, 255, 255, 1];
}

/** A tour's bus in the ID column (`rf_roster::bus_chip`). */
function busChip(ctx, p, t, r, h, on) {
  const b = t.bus;
  if (b.kind === 'player') D.textIn(ctx, b.text, r.x, r.y, r.w, r.h, 12, 700, p.accent);
  else if (b.kind === 'none') issueChip(ctx, r.x, r.y, r.w, r.h, tone(p, b.tone), b.icon, b.text, h);
  else {
    const fill = b.suggested ? alpha(p.tool, 0.5) : p.tool;
    D.rounded(ctx, r.x, r.y, r.w, r.h, 3, h ? mix(fill, p.text, 0.08) : fill);
    if (b.problem) D.roundedBorder(ctx, r.x, r.y, r.w, r.h, 3, 1.2, tone(p, b.problem));
    const col = b.suggested ? p.dim : p.text;
    D.icon(ctx, 'directions_bus', r.x + 11, r.y + r.h / 2, 13, col);
    D.textIn(ctx, b.text, r.x + 22, r.y, r.w - 26, r.h, 12, 700, col);
  }
  if (on) D.roundedBorder(ctx, r.x - 1.5, r.y - 1.5, r.w + 3, r.h + 3, 4, 1.6, p.accent);
}

/** A duty on the hours (`rf_roster::duty_chip`). */
function dutyChip(ctx, p, t, d, br, ppm, xOf, h, chosen) {
  if (d.open) {
    const col = tone(p, d.tone);
    const say = D.width(ctx, d.full, 11.5, 700) + 36 <= br.w ? d.full : d.words;
    issueChip(ctx, br.x, br.y, br.w, br.h, col, d.icon, say, h);
  } else {
    const fill = alpha(fillOf(p, d.fill), (d.alpha ?? 1) * d.faint * (h ? 0.85 : 1));
    D.rounded(ctx, br.x, br.y, br.w, br.h, 3, fill);
    if (d.outline) D.roundedBorder(ctx, br.x, br.y, br.w, br.h, 3, 1.5, p.warn);
    const shade = Math.max(3, Math.min(5, br.h * 0.2));
    for (const [a, z] of d.empty) {
      const xa = xOf(a);
      D.rect(ctx, xa, br.y + br.h - shade, Math.max(2, xOf(z) - xa), shade, [0, 0, 0, 0.3]);
    }
    const ink = alpha(hex(d.ink), Math.max(0.7, d.faint));
    const fits = (txt) => txt && D.width(ctx, txt, 11.5, 700) + 12 <= br.w;
    const words = fits(d.full) ? d.full : fits(d.label) ? d.label : d.short && D.width(ctx, d.short, 11.5, 700) + 8 <= br.w ? d.short : null;
    if (words) D.textIn(ctx, words, br.x + 6, br.y, br.w - 10, br.h, 11.5, 700, ink);
  }
  if (d.late) {
    const lw = Math.max(3, (d.late.until - d.from) * ppm);
    D.rounded(ctx, br.x, br.y, lw, br.h, 3, alpha(d.late.covered ? p.dim : p.err, 0.6));
  }
  if (d.warn) D.circle(ctx, br.x + br.w - 5, br.y + 5, 3, p.warn);
  if (chosen) D.roundedBorder(ctx, br.x - 2, br.y - 2, br.w + 4, br.h + 4, 4, 2, p.text);
}

// --- the side ---------------------------------------------------------------------------------------

/** A list row with a name, what it says on the right, and a swatch (`rf_roster::pick_row`). */
function PickRow({ on, swatch, text, right, rtone = 'dim', arm, onTap, onPress, lit }) {
  return html`<button type="button" class=${'rr-pick' + (on ? ' on' : '') + (lit ? ' lit' : '')} data-arm=${arm ? JSON.stringify(arm) : undefined} title=${right || undefined}
    onMouseDown=${(e) => e.button === 0 && onPress?.(e)} onClick=${onTap}>
    ${swatch && html`<i class="sw" style=${`background:${swatch}`}></i>`}<span class="cut">${text}</span><span class=${'right cut t-' + (on ? 'text' : rtone)}>${right}</span>
  </button>`;
}

function Side({ v, startDrag, carrying }) {
  const side = v.side;
  return html`<div class="rr-side" data-rf=${side?.kind === 'day' ? 'rf-roster-side' : undefined}>
    <div class="head"><span class="rf-caps">${side.title}</span>${side.kind !== 'day' && html`<${IconBtn} icon="close" title="Back to the day" onClick=${() => rsSet({ sel: null })} />`}</div>
    <div class="rr-scroll">
      ${side.kind === 'day' && html`<${DayPanel} d=${side} startDrag=${startDrag} carrying=${carrying} />`}
      ${side.kind === 'duty' && html`<${DutyPanel} d=${side} />`}
      ${side.kind === 'tour' && html`<${TourPanel} d=${side} />`}
    </div>
  </div>`;
}

/** Nothing chosen: the day in short, what fell out, and the drivers and buses to give. */
function DayPanel({ d, startDrag, carrying }) {
  const s = S.rf.roster;
  const armTap = (a) => rsSet({ arm: armIs(s.arm, a) ? null : a });
  const press = (a, label, fill, ink) => (e) => startDrag({ from: 'list', arm: a, label, fill, ink, x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, moved: false });
  const target = (kind) => !!carrying && (kind === 'bus' || carrying.duty != null);
  const me = { kind: 'driver', who: 'player', name: tr('You') };
  return html`<b class="rr-sum">${d.summary}</b>
    <p class=${'rr-gaps ' + (d.ok ? 'ok' : 'warn')}><${Icon} name=${d.ok ? 'check_circle' : 'warning'} size=${14} />${d.gaps}</p>
    ${d.morning.length > 0 && html`<span class="rf-caps">${tr('This morning').toUpperCase()}</span>${d.morning.map(([t, k]) => html`<p class="rr-morning"><i class=${'dot ' + k}></i>${t}</p>`)}`}
    ${d.open_rows.length > 0 && html`<span class="rf-caps">${tr('Open duties').toUpperCase()}</span>${d.open_rows.map((o) => html`<button type="button" class="rr-open" onClick=${() => rsSet({ sel: o.duty != null ? { duty: [o.line, o.name, o.duty] } : { tour: [o.line, o.name] } })}
      onMouseDown=${(e) => e.button === 0 && startDrag({ from: 'chart', hit: { ti: o.tour, duty: o.duty }, label: o.title, x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, moved: false })}>
      <i class=${'bar ' + o.tone}></i><span class=${'ic ' + o.tone}><${Icon} name=${o.hard ? 'error' : 'warning'} size=${14} /></span><b class="cut">${o.title}</b><span class=${'cut t-' + o.tone}>${o.fill}</span><span class="grab"><${Icon} name="open_with" size=${15} /></span></button>`)}`}
    <span class="rf-caps">${tr('Drivers').toUpperCase()}</span>
    <p class="rr-hint">${tr('Tap one, then a duty - or drag it there; or drag a duty here.')}</p>
    <${PickRow} on=${armIs(s.arm, me)} swatch=${'var(--rf-accent)'} text=${tr('You')} right=${tr('Your own duties')} arm=${me} lit=${target('driver')} onTap=${() => armTap(me)} onPress=${press(me, tr('You'), 'var(--rf-accent)', '#fff')} />
    ${d.staff.map((e) => {
      const a = { kind: 'driver', who: { staff: e.id }, name: e.name };
      return html`<${PickRow} on=${armIs(s.arm, a)} swatch=${e.colour} text=${e.name} right=${e.state} rtone=${e.tone} arm=${a} lit=${target('driver')} onTap=${() => armTap(a)} onPress=${press(a, e.first, e.colour, '#0e121c')} />`;
    })}
    ${!d.staff.length && html`<p class="rr-hint">${tr('Nobody on the payroll yet.')}</p>`}
    <span class="rf-caps">${tr('Buses').toUpperCase()}</span>
    ${d.buses.map((b) => {
      const a = { kind: 'bus', bus: b.id, name: b.name };
      return html`<${PickRow} on=${armIs(s.arm, a)} text=${b.name} right=${b.state} rtone=${b.tone} arm=${a} lit=${target('bus')} onTap=${() => armTap(a)} onPress=${press(a, b.number, null, null)} />`;
    })}
    ${!d.buses.length && html`<p class="rr-hint">${tr('No bus in the fleet yet.')}</p>`}`;
}

/** The way from a duty or a tour to its line's trips and timetable (RosterFlow's Timeplan). */
const TimetableButton = ({ line }) => html`<button type="button" class="rr-tt" title=${tr('See the line\'s departures and the tours they make, and change them')} onClick=${() => setRf({ view: TIMEPLAN, tpLine: line, tpHint: null })}><${Icon} name="departure_board" size=${16} /><span>${tr("The line's trips and timetable")}</span></button>`;

/** A duty chosen (`rf_roster::duty_panel`): its times, who drives it and why, who else could; on
 * the company's day what to do when it is open, and "Drive this duty". */
function DutyPanel({ d }) {
  return html`<div class="rr-title"><i style=${`background:${d.colour}`}></i><b class="cut">${d.head}</b></div>
    <${TimetableButton} line=${d.line} />
    ${d.facts.map(([t, k]) => html`<p class=${'rr-fact t-' + k}>${t}</p>`)}
    ${d.options.length > 0 && html`<span class="rf-caps">${tr('Cover it today').toUpperCase()}</span>${d.options.map((o) => html`<${PickRow} on=${o.on} text=${o.label} right=${o.note} onTap=${() => rAct('set_fill', { key: o.key, fill: o.fill })} />`)}`}
    <span class="rf-caps">${tr('Who drives it').toUpperCase()}</span>
    ${d.cands.map((c) => html`<${PickRow} on=${c.on} swatch=${c.swatch === 'accent' ? 'var(--rf-accent)' : c.swatch} text=${c.name} right=${c.state} rtone=${c.tone} onTap=${() => !d.by_player && rAct('give_driver', { ti: d.ti, k: d.k, who: c.who })} />`)}
    ${d.drive && html`<${RfButton} label="Drive this duty" solid style="width:100%;height:36px;margin-top:12px" onClick=${() => coAct('drive_duty', { duty: d.drive, from: 5 }).then(() => setRf({ open: true }))} />`}`;
}

/** A tour chosen (`rf_roster::tour_panel`): its bus and which others could run it; on the
 * company's day a rental bus or dropping it. */
function TourPanel({ d }) {
  return html`<div class="rr-title"><i style=${`background:${d.colour}`}></i><b class="cut">${d.head}</b></div>
    <${TimetableButton} line=${d.line} />
    ${d.facts.map(([t, k]) => html`<p class=${'rr-fact t-' + k}>${t}</p>`)}
    ${d.options.length > 0 && html`<span class="rf-caps">${tr('Cover it today').toUpperCase()}</span>${d.options.map((o) => html`<${PickRow} on=${o.on} text=${o.label} right=${o.note} onTap=${() => o.live && rAct('set_fill', { key: o.key, fill: o.fill })} />`)}`}
    ${!d.by_player && html`<span class="rf-caps">${tr('Which bus runs it').toUpperCase()}</span>${d.cands.map((c) => html`<${PickRow} on=${c.on} text=${c.name} right=${c.state} rtone=${c.tone} onTap=${() => rAct('give_bus', { ti: d.ti, bus: c.bus })} />`)}`}`;
}

// --- putting a line into service --------------------------------------------------------------------

/** "Start service from" (`planning::service_dialog`): now, from tomorrow, or from a date; the
 * line's next seven days as planned, and - with gaps - whether to start all the same. */
function ServiceDialog({ d }) {
  const [v, setV] = useState(null);
  useEffect(() => {
    rfCmd('rf_roster_service', d).then(setV).catch(() => setV(null));
  }, [d.line, d.when, d.date, S.co.shell?.gen]);
  const set = (patch) => setRf({ service: { ...d, ...patch } });
  const close = () => setRf({ service: null });
  if (!v) return null;
  if (v.gone) {
    close();
    return null;
  }
  const go = async () => {
    const r = await rfCmd('rf_roster_act', { act: 'service_start', ...d }).catch((e) => ({ say: { text: e.message, error: true } }));
    after(r);
    if (r?.close) close();
    bump();
  };
  return html`<div class="co-scrim rr-scrim" onMouseDown=${(e) => e.stopPropagation()}>
    <div class="co-dialog" style="width:760px;height:640px">
      <div class="head"><${Icon} name="play_arrow" size=${24} /><h2>${v.title}</h2><button type="button" class="co-iconbtn" onClick=${close}><${Icon} name="close" size=${18} /></button></div>
      <div class="body rr-service">
        <p class="co-para">${v.intro}</p>
        <div class="co-caps">${tr('From when').toUpperCase()}</div>
        <div class="row">
          <div class="co-seg" style="width:520px">${v.whens.map((w, k) => html`<button type="button" class=${d.when === k ? 'on' : ''} onClick=${() => set({ when: k })}>${w}</button>`)}</div>
          ${d.when === 2 && html`<input type="date" class="co-field" value=${d.date || v.date} onChange=${(e) => set({ date: e.currentTarget.value })} />`}
        </div>
        <div class="co-caps">${tr("The line's week as it is planned").toUpperCase()}</div>
        ${v.rows.map((r) => html`<div class="day"><span>${r.day}</span><b class=${'t-' + (r.tone === 'err' ? 'danger' : r.tone)}>${r.text}</b></div>`)}
        ${v.short_text && html`<p class="co-para t-warn" style="margin-top:12px">${v.short_text}</p>
          <label class="co-switch"><span>${v.gaps_label}</span><input type="checkbox" checked=${!!d.gaps} onChange=${(e) => set({ gaps: e.currentTarget.checked })} /><i></i></label>`}
      </div>
      <div class="foot"><span class="rest"></span>
        <button type="button" class="co-btn" onClick=${close}>${tr('Cancel')}</button>
        <button type="button" class="co-btn primary" onClick=${go}><${Icon} name="play_arrow" size=${18} />${tr('Start service')}</button>
      </div>
    </div>
  </div>`;
}

export { css };

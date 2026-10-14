// The planning without the RosterFlow add-on, as our game-drawn launcher draws it
// (`company/planning.rs`, its Roster tab `planning/rf_roster.rs` alone, in RosterFlow's own
// palette): the next seven days as tabs, each with whether it is covered; the toolbar with what
// the roster is and the tools (repeat the day, clear it - pressed twice -, fill the roster); the
// day's tours as a Gantt grouped by line - a line's heading folds it away and back, says what is
// open, opens its timetable and starts or stops its service -, a tour's bus in the ID column
// and its duties on the hours in the colour of who drives them; and beside the chart the day, a
// duty or a tour chosen, with the drivers and buses to give by tapping or dragging, each with its
// reason, and "Drive this duty". Everything in the planning can be dragged: a driver or a bus
// onto a duty or a tour, a duty or a tour (or an open duty of the list) onto a driver or a bus.
//
// With the RosterFlow add-on on, the planning is RosterFlow's (phase 4): handed to our game-drawn
// launcher until then, or drawn by `window.omsiHubRosterFlow` when that is there.

import { html, useEffect, useRef, useState, ui, setUi, setCo, useSize, textW, Icon, Btn, Seg, Switch, Dialog, HOME, handOver, tr } from './kit.js';
import { useMore, actMore, TextIn, box, rosterFlowPage } from './more.js';
import { dark } from '../theme.js';
import { PlanEdit } from './planedit.js';

/** The day tabs' band, the toolbar, a line's heading, the ID column, the side panel's heading,
 * the axis, a scrollbar's room (`rf_roster`, `rf_gantt::AXIS`, `rf_scroll::BAR`). */
const DAY_H = 50;
const TOOL_H = 40;
const HEAD = 34;
const LEFT = 236;
const SIDE_HEAD = 44;
const AXIS = 28;
const BAR = 12;
const ROW_MIN = 27;
const ROW_MAX = 38;

/** RosterFlow's colours (`rosterflow::DARK`, `LIGHT`). */
const DARK = { bg: [39, 46, 56], bar: [33, 39, 48], tool: [59, 74, 92], grid: [39, 46, 56], band: [44, 51, 61], labels: [29, 33, 39], axis: [33, 39, 48], text: [226, 230, 236], dim: [150, 158, 172], faint: [104, 112, 126], line: [255, 255, 255, 0.07], accent: [63, 168, 255], warn: [255, 166, 38], err: [240, 72, 72], ok: [70, 196, 120], grey: [112, 124, 140], sheet: [46, 54, 66], light: false };
const LIGHT = { bg: [240, 243, 247], bar: [255, 255, 255], tool: [226, 233, 242], grid: [255, 255, 255], band: [246, 248, 251], labels: [236, 240, 245], axis: [244, 246, 249], text: [28, 36, 48], dim: [92, 104, 120], faint: [150, 160, 174], line: [20, 36, 60, 0.09], accent: [18, 124, 226], warn: [226, 128, 10], err: [214, 46, 46], ok: [30, 150, 86], grey: [150, 160, 174], sheet: [255, 255, 255], light: true };
const pal = () => (dark() ? DARK : LIGHT);
const rgba = (c, a = 1) => `rgba(${c[0]},${c[1]},${c[2]},${c.length > 3 ? c[3] * a : a})`;
const mix = (a, b, t) => a.slice(0, 3).map((v, i) => Math.round(v + (b[i] - v) * t));

/** A tone of the view as RosterFlow's colour (`rf_roster::ink`). */
function ink(q, tone, a = 1) {
  switch (tone) {
    case 'warn':
      return rgba(q.warn, a);
    case 'err':
      return rgba(q.err, a);
    case 'ok':
      return rgba(q.ok, a);
    case 'dim':
      return rgba(q.dim, a);
    case 'faint':
      return rgba(q.faint, a);
    case 'accent':
      return rgba(q.accent, a);
    case 'text82':
      return rgba(q.text, 0.82 * a);
    case 'text86':
      return rgba(q.text, 0.86 * a);
    default:
      return rgba(q.text, a);
  }
}

/** A chip's fill as the view names it: a driver's colour, the accent, the grey. */
const fillOf = (q, f, a = 1) => (f === 'accent' ? rgba(q.accent, a) : f === 'grey' ? rgba(q.grey, a) : rgbaHex(f, a));
function rgbaHex(hex, a) {
  const n = parseInt(String(hex).replace('#', ''), 16) || 0;
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

/** Ink on a chip of a colour: dark on light colours, white on dark (`ownlines::ink_on`). */
function inkOn(hex) {
  const n = parseInt(String(hex).replace('#', ''), 16) || 0;
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6 ? '#1a1400' : '#ffffff';
}

/** Small capitals (`rosterflow::caps`): 11 points, bold. */
const Caps = ({ x, y, w, h, color, children, align }) => html`<${TextIn} x=${x} y=${y} w=${w} h=${h} px=${11} weight=${700} color=${color} align=${align || 'left'}>${children}</${TextIn}>`;

/** RosterFlow's button (`rosterflow::rf_button`): solid blue, else grey; its label in capitals. */
function RfButton({ r, label, solid, onClick, title, extra = '', armed }) {
  const q = pal();
  const base = solid ? q.accent : q.light ? [196, 204, 214] : [104, 116, 132];
  const inkc = solid || !q.light ? '#fff' : rgba(q.text);
  return html`<button type="button" class="pl-btn" title=${title} style=${box(r.x, r.y, r.w, r.h) + `;--b:${rgba(base)};--h:${rgba(mix(base, [255, 255, 255], 0.08))};color:${inkc};${armed ? `box-shadow:inset 0 0 0 1.5px ${rgba(q.err)};` : ''}${extra}`} onClick=${onClick}>${label}</button>`;
}

/** An icon that is a button (`rosterflow::rf_icon`). */
function RfIcon({ cx, cy, icon, title, onClick }) {
  return html`<button type="button" class="pl-icon" title=${title} style=${box(cx - 15, cy - 15, 30, 30)} onClick=${onClick}><${Icon} name=${icon} size=${19} /></button>`;
}

/** What is open, as RosterFlow's issues (`rf_roster::issue_chip`): an outlined chip in the
 * warning (or error) colour with its icon, its words as the room allows. */
function issue(r, colour, icon, words, extra = {}) {
  const ih = Math.max(11, Math.min(14, r.h * 0.62));
  const tx = ih + 9;
  return html`<div class="pl-issue" style=${box(r.x, r.y, r.w, r.h) + `;--c:${colour}`} ...${extra}>
    ${r.w >= ih + 8 && html`<span style=${`position:absolute;left:${4}px;top:${r.h / 2 - ih / 2}px;display:flex`}><${Icon} name=${icon} size=${ih} /></span>`}
    ${r.w - tx > 16 && words && html`<${TextIn} x=${tx} y=${0} w=${r.w - tx - 5} h=${r.h} px=${11.5} weight=${700} color=${colour}>${words}</${TextIn}>`}
  </div>`;
}

/** The width an issue's tag takes (`rf_roster::tag_w`). */
const tagW = (words) => textW(words, 11.5, 700) + 32;

/** The pitch of a tour's row: the room shared by `rows`, between the least and the most
 * (`planning::row_pitch`). */
const rowPitch = (room, rows) => (rows === 0 ? ROW_MAX : Math.max(ROW_MIN, Math.min(ROW_MAX, Math.floor(room / rows))));

/** The words a block shows at its width: its name where it fits, else its short form, else none
 * (`planning::fit_words`). */
function fitWords(w, px, full, short) {
  if (full && textW(full, px, 700) + 12 <= w) return full;
  if (short && textW(short, px, 700) + 8 <= w) return short;
  return null;
}

/** What the page holds (`PlanningView`; `edit`: the sheet of a line's trips and timetable open). */
const st0 = { day: 0, sel: null, arm: null, clear_armed: false, service: null, focus: null, edit: null };

/** A line's trips and timetable (`planning::open_timetable`): the planning's own sheet over the
 * page (Omsi-Hub 6/6: `planedit.js`), or RosterFlow's Timeplan with the add-on on. */
const openSheet = (args) => actMore('plan_timetable', args).then((r) => r?.plan_edit && setUi('planning', { edit: Date.now(), arm: null, sel: null }));
const keyOfArm = (a) => (a ? (a.driver ? 'd:' + a.driver : 'b:' + a.bus) : '');

export function PlanningPage() {
  const st = ui('planning', st0);
  const v = useMore('planning', { day: st.day, sel: st.sel, arm: st.arm, clear_armed: st.clear_armed });
  const [ref, size] = useSize();
  const drag = useDrag(v, st);
  // (RosterFlow on: its own page, or our game-drawn launcher's for now)
  useEffect(() => {
    if (!v?.rosterflow || rosterFlowPage()) return;
    handOver('5');
    setCo({ tab: HOME });
  }, [v?.rosterflow]);
  // Escape lets go of what is in hand first (`holds_choice`), before the start screen
  useEffect(() => {
    const k = (e) => {
      if (e.key !== 'Escape') return;
      const s = ui('planning', st0);
      if (s.service || s.edit) return;
      if (s.arm || s.sel) {
        e.stopImmediatePropagation();
        setUi('planning', { arm: null, sel: null });
      }
    };
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, []);
  if (v?.rosterflow) {
    const Page = rosterFlowPage();
    return Page ? html`<${Page} />` : null;
  }
  const q = pal();
  let body = null;
  if (v?.tabs && size.w > 0) {
    const W = size.w;
    const H = size.h;
    const by = DAY_H + TOOL_H;
    const bh = Math.max(0, H - by);
    const sideW = Math.max(300, Math.min(420, W * 0.27));
    const cw = Math.max(200, W - sideW);
    body = html`
      <${DayTabs} v=${v} st=${st} W=${W} />
      <${Tools} v=${v} st=${st} W=${W} />
      ${v.reading || v.error
        ? html`<div style=${box(0, by, W, bh) + `;background:${rgba(q.grid)}`}>
            ${v.reading ? html`<${TextIn} x=${24} y=${0} w=${W - 48} h=${bh} px=${13} weight=${500} color=${rgba(q.dim)} align="center">${v.reading}</${TextIn}>` : html`<p class="pl-para" style=${`margin:24px;color:${rgba(q.err)}`}>${v.error}</p>`}
          </div>`
        : v.gantt &&
          html`<${Gantt} v=${v} st=${st} r=${{ x: 0, y: by, w: cw, h: bh }} drag=${drag} />
            <${Side} v=${v} st=${st} r=${{ x: cw, y: by, w: sideW, h: bh }} drag=${drag} />`}`;
  }
  return html`<div class="cm-page pl" ref=${ref} style=${`background:${rgba(q.bg)};color:${rgba(q.text)}`}>
    ${body}
    ${drag.chip}
    ${st.edit && size.w > 0 && html`<${PlanEdit} key=${st.edit} W=${size.w} H=${size.h} />`}
    ${st.service && html`<${ServiceDialog} s=${st.service} />`}
  </div>`;
}

// --- dragging -----------------------------------------------------------------------------------

/** Everything in the planning can be dragged (`rf_roster::page`): a driver or a bus of the list
 * onto a duty or a tour's bus; a duty or a tour of the chart, or an open duty of the list, onto a
 * driver or a bus of the list. A press that does not move is a tap. */
function useDrag(v, st) {
  const [d, setD] = useState(null);
  const live = useRef(null);
  live.current = { v, st };
  useEffect(() => {
    if (!d) return undefined;
    const move = (e) => {
      const moved = Math.hypot(e.clientX - d.x0, e.clientY - d.y0) > 6;
      setD((o) => (o ? { ...o, mx: e.clientX, my: e.clientY, moved: o.moved || moved } : o));
    };
    const up = (e) => {
      const o = d;
      setD(null);
      if (!o.moved && Math.hypot(e.clientX - o.x0, e.clientY - o.y0) <= 6) return;
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const date = live.current.v?.date;
      if (o.arm) {
        // a driver or a bus let go on a duty or a tour's bus
        const hit = el?.closest('[data-hit]');
        if (!hit) return;
        const ti = Number(hit.dataset.ti);
        const k = hit.dataset.k === '' ? null : Number(hit.dataset.k);
        if (o.arm.driver && k != null) actMore('plan_give_driver', { date, ti, k, who: o.arm.driver });
        else if (o.arm.bus != null) actMore('plan_give_bus', { date, ti, bus: o.arm.bus });
      } else if (o.carry) {
        // a duty or a tour let go on a driver or a bus of the list
        const t = el?.closest('[data-target]');
        if (!t) return;
        const { ti, k } = o.carry;
        if (t.dataset.driver && k != null) actMore('plan_give_driver', { date, ti, k, who: t.dataset.driver });
        else if (t.dataset.bus) actMore('plan_give_bus', { date, ti, bus: Number(t.dataset.bus) });
      }
    };
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
    return () => {
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
    };
  }, [d && d.x0, d && d.y0]);
  const q = pal();
  let chip = null;
  if (d?.moved) {
    // (the stage may be drawn larger or smaller than it is laid out)
    const el = document.querySelector('.cm-page.pl');
    const rect = el?.getBoundingClientRect();
    const s = rect && el.offsetWidth ? rect.width / el.offsetWidth : 1;
    const x = rect ? (d.mx - rect.left) / s : d.mx;
    const y = rect ? (d.my - rect.top) / s : d.my;
    const w = Math.min(360, textW(d.label, 12.5, 700) + 22);
    chip = html`<div class="pl-carry" style=${box(x + 10, y + 6, w, 24) + `;background:${d.fill || rgba(q.accent)};color:${d.ink || '#fff'}`}>${d.label}</div>`;
  }
  return {
    on: d,
    chip,
    /** A driver or a bus pressed in the list. */
    pressArm(e, arm, label, fill, inkc) {
      if (e.button !== 0) return;
      setD({ arm, label, fill, ink: inkc, x0: e.clientX, y0: e.clientY, mx: e.clientX, my: e.clientY, moved: false });
    },
    /** A duty or a tour pressed in the chart or the list. */
    pressCarry(e, ti, k, label) {
      if (e.button !== 0) return;
      setD({ carry: { ti, k }, label, x0: e.clientX, y0: e.clientY, mx: e.clientX, my: e.clientY, moved: false });
    },
  };
}

// --- the day tabs and the tools -----------------------------------------------------------------

/** The next seven days as RosterFlow's tabs (`rf_roster::day_tabs`). */
function DayTabs({ v, st, W }) {
  const q = pal();
  const w = W / 7;
  return html`<div style=${box(0, 0, W, DAY_H) + `;background:${rgba(q.bar)};box-shadow:inset 0 -1px 0 ${rgba(q.line)}`}>
    ${v.tabs.map((t, k) => {
      const on = st.day === k;
      return html`<button type="button" class=${'pl-day' + (on ? ' on' : '')} title=${t.tip || undefined} style=${box(k * w, 0, w, DAY_H) + `;--a:${rgba(q.accent)};--a7:${rgba(q.accent, 0.07)};--h:${rgba(q.text, 0.04)}`}
          onClick=${() => !on && setUi('planning', { day: k, sel: null, clear_armed: false })}>
        ${k > 0 && html`<i class="pl-sep" style=${`background:${rgba(q.line)}`}></i>`}
        <${Caps} x=${16} y=${8} w=${w - 40} h=${16} color=${on ? rgba(q.accent) : rgba(q.dim)}>${t.caps}</${Caps}>
        <${TextIn} x=${16} y=${24} w=${w - 28} h=${20} px=${13.5} weight=${700} color=${on ? rgba(q.text) : rgba(q.text, 0.86)} cls="pl-big">${t.big}</${TextIn}>
        ${t.dot && html`<i class="pl-dot" style=${`left:${w - 20}px;top:11px;background:${ink(q, t.dot)}`}></i>`}
      </button>`;
    })}
  </div>`;
}

/** The toolbar (`rf_roster::tools`): what the roster is, and repeat, clear and fill. */
function Tools({ v, st, W }) {
  const q = pal();
  const t = v.tools;
  const bw = (label) => textW(label, 12.5, 700) + 36;
  let x = W - 4;
  const fw = bw(t.fill.label);
  x -= fw;
  const fillR = { x, y: DAY_H + 4, w: fw, h: TOOL_H - 8 };
  const cw = Math.max(...t.clear.widest.map(bw));
  x -= cw + 6;
  const clearR = { x, y: DAY_H + 4, w: cw, h: TOOL_H - 8 };
  const rw = bw(t.repeat.label);
  x -= rw + 6;
  const repeatR = { x, y: DAY_H + 4, w: rw, h: TOOL_H - 8 };
  const colour = ink(q, t.tone);
  const clear = () => {
    if (st.clear_armed) {
      setUi('planning', { clear_armed: false });
      actMore('plan_clear', { date: v.date });
    } else setUi('planning', { clear_armed: true });
  };
  return html`<div style=${box(0, DAY_H, W, TOOL_H) + `;background:${rgba(q.tool)}`}></div>
    <span class="cm-icon" style=${`left:10px;top:${DAY_H + 12}px;color:${colour}`}><${Icon} name=${t.icon} size=${16} /></span>
    <${TextIn} x=${34} y=${DAY_H} w=${Math.max(0, x - 46)} h=${TOOL_H} px=${13} weight=${500} color=${colour} title=${t.text}>${t.text}</${TextIn}>
    <${RfButton} r=${repeatR} label=${t.repeat.label} title=${t.repeat.tip} onClick=${() => actMore('plan_repeat', { date: v.date })} />
    <${RfButton} r=${clearR} label=${t.clear.label} title=${t.clear.tip} armed=${t.clear.armed} onClick=${clear} />
    <${RfButton} r=${fillR} label=${t.fill.label} solid title=${t.fill.tip} onClick=${() => actMore('plan_fill', { date: v.date })} />`;
}

// --- the chart -----------------------------------------------------------------------------------

/** A tap that comes soon after one on the same thing (`planning::second_tap`). */
let tapped = null;
function secondTap(key) {
  const now = performance.now();
  const twice = tapped && tapped.key === key && now - tapped.at < 450;
  tapped = twice ? null : { key, at: now };
  return twice;
}

/** The day's tours as RosterFlow's Gantt, grouped by line (`rf_roster::gantt`). */
function Gantt({ v, st, r, drag }) {
  const q = pal();
  const g = v.gantt;
  const ref = useRef(null);
  // a line to show (from the Lines page): unfolded, and its group scrolled into view once
  useEffect(() => {
    if (!st.focus || !g.groups) return;
    let y = 0;
    for (const gr of g.groups) {
      if (gr.name.toLowerCase() === st.focus.toLowerCase()) {
        if (gr.folded && !gr.none) actMore('plan_fold', { name: gr.name });
        if (ref.current) ref.current.scrollTop = Math.max(0, y - 40);
        break;
      }
      y += HEAD + (gr.folded ? 0 : gr.tours.length * ROW_MIN);
    }
    setUi('planning', { focus: null });
  }, [st.focus, !!g.groups]);
  if (g.empty) {
    return html`<div style=${box(r.x, r.y, r.w, r.h) + `;background:${rgba(q.grid)}`}><p class="pl-para" style=${`margin:24px;color:${rgba(q.dim)}`}>${g.empty}</p></div>`;
  }
  const span = Math.max(60, g.t1 - g.t0);
  const gx = LEFT + 8;
  const gw = Math.max(60, r.w - LEFT - 8 - BAR - 8);
  const ppm = gw / span;
  const xOf = (m) => gx + (m - g.t0) * ppm;
  const hours = [];
  for (let h = Math.floor(g.t0 / 60); h < g.t1 / 60; h++) hours.push(h);
  const every = ppm * 60 >= 50 ? 1 : ppm * 60 >= 25 ? 2 : 3;
  const labels = [];
  for (let h = Math.floor(g.t0 / 60); h <= g.t1 / 60; h++) if (h % every === 0) labels.push(h);
  // the rows' height: the room left by the headings, shared by the tours shown
  const rowsH = Math.max(0, r.h - AXIS);
  const shown = g.groups.filter((x) => !x.folded).reduce((n, x) => n + x.tours.length, 0);
  const pitch = rowPitch(rowsH - 8 - g.groups.length * HEAD, shown);
  const date = v.date;
  const tap = (t, k) => {
    // (tapped twice: the line's timetable, that tour marked)
    if (secondTap(k == null ? `bus-${t.ti}` : `duty-${t.ti}-${k}`)) {
      openSheet({ line: t.line, tour: t.tour, date });
      return;
    }
    const arm = st.arm;
    if (arm?.driver && k != null && !t.by_player) actMore('plan_give_driver', { date, ti: t.ti, k, who: arm.driver });
    else if (arm?.bus != null && !t.by_player) actMore('plan_give_bus', { date, ti: t.ti, bus: arm.bus });
    else {
      const sel = k == null ? { line: t.line, tour: t.tour } : { line: t.line, tour: t.tour, k };
      const same = st.sel && st.sel.line === sel.line && st.sel.tour === sel.tour && st.sel.k === sel.k;
      setUi('planning', { sel: same ? null : sel });
    }
  };
  const W = r.w;
  let y = 0;
  const rows = [];
  for (const gr of g.groups) {
    rows.push(html`<${Heading} gr=${gr} y=${y} w=${W - BAR} date=${date} />`);
    y += HEAD;
    if (gr.folded) continue;
    for (const t of gr.tours) {
      rows.push(html`<${TourRow} t=${t} y=${y} pitch=${pitch} gx=${gx} gw=${gw} xOf=${xOf} ppm=${ppm} tap=${tap} drag=${drag} />`);
      y += pitch;
    }
  }
  const head = g.head;
  return html`<div style=${box(r.x, r.y, r.w, r.h) + `;background:${rgba(q.grid)};overflow:hidden`}>
    <div style=${box(0, AXIS, LEFT, r.h - AXIS) + `;background:${rgba(q.labels)}`}></div>
    <div style=${box(gx, AXIS, gw, r.h - AXIS) + ';overflow:hidden'}>
      ${hours.filter((h) => h % 2 === 1).map((h) => html`<i class="pl-band" style=${`left:${xOf(h * 60) - gx}px;width:${60 * ppm}px;background:${rgba(q.band)}`}></i>`)}
    </div>
    <div style=${box(0, 0, r.w, AXIS) + `;background:${rgba(q.axis)};box-shadow:inset 0 -1px 0 ${rgba(q.line)}`}></div>
    <div style=${box(gx - 24, 0, gw + 30, AXIS) + ';overflow:hidden'}>
      ${labels.map((h) => html`<${TextIn} x=${xOf(h * 60) - (gx - 24) - 30} y=${0} w=${60} h=${AXIS} px=${11} weight=${500} color=${rgba(q.dim)} align="center">${String(((h % 24) + 24) % 24).padStart(2, '0')}:00</${TextIn}>`)}
    </div>
    ${head.hint && html`<${TextIn} x=${12} y=${0} w=${LEFT - 24} h=${AXIS} px=${11.5} weight=${700} color=${rgba(q.accent)} title=${head.hint}>${head.hint}</${TextIn}>`}
    ${head.fold &&
    html`<button type="button" class="pl-foldall" title=${head.fold.tip} style=${box(6, 3, textW(head.fold.label, 11, 700) + 34, AXIS - 6) + `;--d:${rgba(q.dim)};--t:${rgba(q.text)}`} onClick=${() => actMore('plan_fold_all', { all: !head.fold.all, names: g.names })}>
      <${Icon} name=${head.fold.icon} size=${15} /><span>${head.fold.label}</span>
    </button>`}
    <div class="pl-scroll" ref=${ref} style=${box(0, AXIS, r.w, rowsH)}>
      <div style=${`position:relative;height:${y + 8}px`}>${rows}</div>
    </div>
  </div>`;
}

/** A line's heading (`rf_roster::line_heading`): the fold, the line's plate and name, what is
 * open as issue tags, its timetable, and its service. */
function Heading({ gr, y, w, date }) {
  const q = pal();
  const cy = HEAD / 2;
  const stateW = Math.min(360, w * 0.36);
  const ttW = 26 + textW(gr.tt.label, 13, 500) + 12;
  const tt = { x: w - stateW - 12 - ttW, y: 4, w: ttW, h: HEAD - 8 };
  let x = 32;
  const nameRight = tt.x - 10;
  const tagsW = gr.tags.reduce((n, t) => n + tagW(t.text) + 6, 0);
  const withTags = nameRight - tagsW - (x + 60) >= 140;
  let tx = nameRight;
  const tags = [];
  if (withTags) {
    for (const t of [...gr.tags].reverse()) {
      const tw = tagW(t.text);
      tx -= tw;
      tags.push(issue({ x: tx, y: cy - 10, w: tw, h: 20 }, ink(q, gr.tag_tone), t.icon, t.text));
      tx -= 6;
    }
  }
  const toggle = () => actMore('plan_fold', { name: gr.name });
  const parts = [];
  if (gr.line) {
    const pw = Math.max(30, textW(gr.line.plate.number, 12, 700) + 16);
    parts.push(html`<span class="pl-plate" style=${box(x, cy - 10, pw, 20) + `;background:${gr.line.colour};color:${inkOn(gr.line.colour)}`}>${gr.line.plate.number}</span>`);
    x += pw + 10;
    const capW = Math.max(40, tx - x - 8);
    parts.push(html`<${TextIn} x=${x} y=${0} w=${capW} h=${HEAD} px=${13} weight=${700} color=${rgba(q.text)} title=${!withTags && gr.tags.length ? gr.tags.map((t) => t.text).join('  ·  ') : undefined}>${gr.line.caption}</${TextIn}>`);
    if (!gr.none) parts.push(html`<button type="button" class="pl-hit" style=${box(30, 0, x + capW - 30, HEAD)} onClick=${toggle}></button>`);
  } else parts.push(html`<${TextIn} x=${x} y=${0} w=${Math.max(40, tx - x - 8)} h=${HEAD} px=${13} weight=${700} color=${rgba(q.text)}>${gr.name}</${TextIn}>`);
  const sr = { x: w - stateW, y: 4, w: stateW - 6, h: HEAD - 8 };
  const sv = gr.service;
  let service = null;
  if (sv?.kind === 'none') service = html`<${TextIn} x=${sr.x} y=${sr.y} w=${sr.w} h=${sr.h} px=${12} weight=${500} color=${rgba(q.dim)} align="right">${sv.text}</${TextIn}>`;
  else if (sv?.kind === 'start') {
    const bw = textW(sv.label, 12.5, 700) + 30;
    service = html`<${RfButton} r=${{ x: sr.x + sr.w - bw, y: sr.y, w: bw, h: sr.h }} label=${sv.label} solid title=${sv.tip} onClick=${() => setUi('planning', { service: { line: gr.name, when: 0, date: '', gaps: false } })} />
      <${TextIn} x=${sr.x} y=${sr.y} w=${sr.w - bw - 10} h=${sr.h} px=${12} weight=${500} color=${rgba(q.dim)} align="right">${sv.note}</${TextIn}>`;
  } else if (sv?.kind === 'in') {
    service = html`<${TextIn} x=${sr.x} y=${sr.y} w=${sr.w - 30} h=${sr.h} px=${12} weight=${500} color=${ink(q, sv.tone)} align="right">${sv.text}</${TextIn}>
      <${RfIcon} cx=${sr.x + sr.w - 10} cy=${cy} icon="pause" title=${sv.stop_tip} onClick=${() => actMore('plan_stop', { name: gr.name })} />`;
  }
  return html`<div class="pl-head" style=${box(0, y, w, HEAD) + `;background:${rgba(mix(q.labels, q.grid, 0.4))};--l:${rgba(q.line)};--bw:${w + BAR}px`}>
    ${!gr.none && html`<button type="button" class="pl-fold" title=${gr.fold_tip} style=${box(4, cy - 12, 24, 24) + `;color:${rgba(q.dim)};--t:${rgba(q.text)}`} onClick=${toggle}><${Icon} name=${gr.folded ? 'chevron_right' : 'expand_more'} size=${18} /></button>`}
    ${parts}
    ${tags}
    <button type="button" class="pl-tt" title=${gr.tt.tip} style=${box(tt.x, tt.y, tt.w, tt.h) + `;--a:${rgba(q.accent)};--t:${rgba(q.text)};--h:${rgba(q.text, 0.06)}`} onClick=${() => openSheet({ line: gr.name, tour: '', date })}>
      <span class="cm-icon" style=${`left:5px;top:${tt.h / 2 - 8}px`}><${Icon} name="departure_board" size=${16} /></span>
      <span class="pl-tt-l" style=${`left:26px;width:${tt.w - 28}px`}>${gr.tt.label}</span>
    </button>
    ${service}
  </div>`;
}

/** A tour's row: the ID column (the line's colour, the tour, its bus) and its duties on the
 * hours (`rf_roster::gantt`'s rows, `bus_chip`, `duty_chip`). */
function TourRow({ t, y, pitch, gx, gw, xOf, ppm, tap, drag }) {
  const q = pal();
  const cy = pitch / 2;
  const idTone = t.unplanned ? rgba(q.dim) : t.on ? rgba(q.accent) : rgba(q.text, 0.9);
  const chipH = Math.max(18, Math.min(24, pitch - 8));
  const chip = { x: 100, y: cy - chipH / 2, w: LEFT - 110, h: chipH };
  const barH = Math.max(18, Math.min(26, pitch - 8));
  const label = `${t.label}`;
  const b = t.bus;
  let bus;
  const press = (e, k, words) => drag.pressCarry(e, t.ti, k, words);
  const hit = (k) => ({ 'data-hit': '1', 'data-ti': String(t.ti), 'data-k': k == null ? '' : String(k) });
  if (b.kind === 'you') bus = html`<${TextIn} x=${chip.x} y=${chip.y} w=${chip.w} h=${chip.h} px=${12} weight=${700} color=${rgba(q.accent)}>${b.text}</${TextIn}>`;
  else if (b.kind === 'none')
    bus = issue(chip, ink(q, b.tone), b.icon, b.text, { class: 'pl-issue pl-press', title: b.tip, onPointerDown: (e) => press(e, null, label), onClick: () => tap(t, null), ...hit(null) });
  else {
    const fill = b.suggested ? rgba(q.tool, 0.5) : rgba(q.tool);
    const col = b.suggested ? rgba(q.dim) : rgba(q.text);
    bus = html`<div class="pl-bus pl-press" title=${b.tip} ...${hit(null)} style=${box(chip.x, chip.y, chip.w, chip.h) + `;--f:${fill};--fh:${rgba(mix(q.tool, q.text, 0.08), b.suggested ? 0.5 : 1)};${b.problem ? `box-shadow:inset 0 0 0 1.2px ${ink(q, b.problem)};` : ''}color:${col}`}
        onPointerDown=${(e) => press(e, null, label)} onClick=${() => tap(t, null)}>
      <span class="cm-icon" style=${`left:4px;top:${chip.h / 2 - 6.5}px`}><${Icon} name="directions_bus" size=${13} /></span>
      <${TextIn} x=${22} y=${0} w=${chip.w - 26} h=${chip.h} px=${12} weight=${700} color=${col}>${b.text}</${TextIn}>
    </div>`;
  }
  return html`<div class=${'pl-row' + (t.on ? ' on' : '')} style=${box(0, y, gx + gw + 12, pitch) + `;--h:${rgba(q.text, 0.03)};--on:${rgba(q.text, 0.06)};--u:${rgba(q.text, 0.025)};--gx:${gx - 6}px`}>
    ${t.unplanned && html`<i class="pl-unplanned"></i>`}
    <i class="pl-rowlight" style=${`left:${gx - 6}px;width:${gw + 12}px`}></i>
    <i class="pl-swatch" style=${`left:14px;top:${cy - 6}px;background:${t.colour}`}></i>
    <${TextIn} x=${22} y=${0} w=${76} h=${pitch} px=${12.5} weight=${700} color=${idTone}>${t.label}</${TextIn}>
    ${bus}
    ${t.on && html`<i class="pl-ring" style=${box(chip.x - 1.5, chip.y - 1.5, chip.w + 3, chip.h + 3) + `;box-shadow:inset 0 0 0 1.6px ${rgba(q.accent)}`}></i>`}
    <div style=${box(gx - 4, 0, gw + 8, pitch) + ';overflow:hidden'}>
      ${t.duties.map((d) => {
        const br = { x: xOf(d.from) - (gx - 4), y: cy - barH / 2, w: Math.max(6, (d.to - d.from) * ppm), h: barH };
        return html`<${Duty} d=${d} br=${br} xOf=${(m) => xOf(m) - (gx - 4)} ppm=${ppm} onPress=${(e) => press(e, d.k, `${label}  ·  ${tr('Duty')} ${d.k + 1}`)} onTap=${() => tap(t, d.k)} hit=${hit(d.k)} />`;
      })}
    </div>
  </div>`;
}

/** A duty on the hours (`rf_roster::duty_chip`): in the colour of who drives it - paler when only
 * the dispatcher's suggestion, outlined orange when chosen for today alone -, or RosterFlow's
 * issue chip with what is wrong and its times; its depot runs shaded, a late driver's first
 * minutes, a warning's dot, the one chosen ringed. */
function Duty({ d, br, xOf, ppm, onPress, onTap, hit }) {
  const q = pal();
  const parts = [];
  const events = { onPointerDown: onPress, onClick: onTap, title: d.tip, ...hit };
  let main;
  if (d.open) {
    const colour = ink(q, d.tone);
    const say = textW(d.full, 11.5, 700) + 36 <= br.w ? d.full : d.words;
    main = issue(br, colour, d.icon, say, { class: 'pl-issue pl-press', ...events });
  } else {
    const shade = Math.max(3, Math.min(5, br.h * 0.2));
    for (const [a0, b0] of d.shades || []) {
      const a = xOf(a0);
      const b = Math.max(a + 2, xOf(b0));
      parts.push(html`<i class="pl-shade" style=${box(a - br.x, br.h - shade, b - a, shade)}></i>`);
    }
    const inkc = d.ink === 'dark' ? 'rgb(14,18,28)' : '#ffffff';
    const words = fitWords(br.w, 11.5, `${d.label}  ${d.times}`, d.label) || fitWords(br.w, 11.5, d.short, d.short);
    main = html`<div class="pl-duty pl-press" ...${events} style=${box(br.x, br.y, br.w, br.h) + `;--f:${fillOf(q, d.fill, d.alpha * d.faint)};--fh:${fillOf(q, d.fill, d.alpha * d.faint * 0.85)};${d.edge ? `box-shadow:inset 0 0 0 1.5px ${rgba(q.warn)};` : ''}`}>
      ${parts}
      ${words && html`<${TextIn} x=${6} y=${0} w=${br.w - 10} h=${br.h} px=${11.5} weight=${700} color=${inkc} style=${`opacity:${Math.max(d.faint, 0.7)}`}>${words}</${TextIn}>`}
    </div>`;
  }
  return html`${main}
    ${d.late && html`<i class="pl-late" style=${box(br.x, br.y, Math.max(3, (d.late.until - d.from) * ppm), br.h) + `;background:${ink(q, d.late.tone, 0.6)}`}></i>`}
    ${d.warn && html`<i class="pl-warn" style=${`left:${br.x + br.w - 8}px;top:${br.y + 2}px;background:${rgba(q.warn)}`}></i>`}
    ${d.chosen && html`<i class="pl-ring" style=${box(br.x - 2, br.y - 2, br.w + 4, br.h + 4) + `;box-shadow:inset 0 0 0 2px ${rgba(q.text)}`}></i>`}`;
}

// --- the side -------------------------------------------------------------------------------------

/** A list row with a name, what it says on the right, and a swatch (`rf_roster::pick_row`). */
function PickRow({ r, on, swatch, text, right, rightTone, onClick, onPointerDown, target, fits }) {
  const q = pal();
  let x = 12;
  const full = textW(right || '', 12, 400);
  const rw = Math.min(full, r.w * 0.62);
  const sw = swatch ? (swatch === 'accent' ? rgba(q.accent) : swatch) : null;
  if (sw) x += 18;
  return html`<button type="button" class=${'pl-pick' + (on ? ' on' : '') + (fits ? ' fits' : '')} title=${full > rw ? right : undefined} ...${target || {}}
      style=${box(r.x, r.y, r.w, r.h) + `;--a:${rgba(q.accent)};--a16:${rgba(q.accent, 0.16)};--h:${rgba(q.text, 0.05)}`} onClick=${onClick} onPointerDown=${onPointerDown}>
    ${on && html`<i class="pl-pickbar"></i>`}
    ${sw && html`<i class="pl-pickswatch" style=${`top:${r.h / 2 - 5}px;background:${sw}`}></i>`}
    <${TextIn} x=${x} y=${0} w=${r.w - x - rw - 16} h=${r.h} px=${13} weight=${500} color=${rgba(q.text)}>${text}</${TextIn}>
    ${right && html`<${TextIn} x=${r.w - rw - 10} y=${0} w=${rw} h=${r.h} px=${12} color=${on ? rgba(q.text) : ink(q, rightTone)} align="right">${right}</${TextIn}>`}
  </button>`;
}

/** The side panel: the day, a duty or a tour (`rf_roster::side_panel`). */
function Side({ v, st, r, drag }) {
  const q = pal();
  const s = v.side;
  const inner = { x: 14, y: SIDE_HEAD + 12, w: r.w - 16, h: Math.max(0, r.h - SIDE_HEAD - 16) };
  const close = () => setUi('planning', { sel: null });
  let body = null;
  if (s.kind === 'day') body = html`<${DayPanel} s=${s} v=${v} st=${st} inner=${inner} drag=${drag} />`;
  else if (s.kind === 'duty') body = html`<${DutyPanel} s=${s} v=${v} inner=${inner} />`;
  else body = html`<${TourPanel} s=${s} v=${v} inner=${inner} />`;
  return html`<div style=${box(r.x, r.y, r.w, r.h) + `;background:${rgba(q.sheet)};box-shadow:inset 1px 0 0 ${rgba(q.text, 0.1)}`}>
    <div style=${box(0, 0, r.w, SIDE_HEAD) + `;background:${rgba(q.axis)};box-shadow:inset 0 -1px 0 ${rgba(q.line)}`}></div>
    <${Caps} x=${16} y=${0} w=${r.w - 60} h=${SIDE_HEAD} color=${rgba(q.dim)}>${s.title}</${Caps}>
    ${s.kind !== 'day' && html`<${RfIcon} cx=${r.w - 24} cy=${SIDE_HEAD / 2} icon="close" title=${s.close} onClick=${close} />`}
    ${body}
  </div>`;
}

/** A paragraph in RosterFlow's type (`Ui::paragraph`: its lines 1.38 of its size apart). */
const Para = ({ px, weight = 400, color, children, style = '' }) => html`<p class="pl-para" style=${`font-size:${px / 1.303}px;line-height:${px * 1.38}px;font-weight:${weight};color:${color};${style}`}>${children}</p>`;

/** Nothing chosen: the day in short, what fell out, the drivers and buses to give
 * (`rf_roster::day_panel`). */
function DayPanel({ s, v, st, inner, drag }) {
  const q = pal();
  const carrying = drag.on?.carry && drag.on.moved ? drag.on.carry : null;
  const fitsDriver = carrying && carrying.k != null;
  const fitsBus = !!carrying;
  const tapArm = (arm) => setUi('planning', { arm: keyOfArm(st.arm) === keyOfArm(arm) ? null : arm });
  const w = inner.w - BAR - 2;
  const Head = ({ children }) => html`<div class="pl-caps" style=${`color:${rgba(q.dim)}`}>${children}</div>`;
  return html`<div class="pl-scroll" style=${box(inner.x, inner.y, inner.w, inner.h)}>
    <div style=${`width:${w}px;padding-bottom:8px`}>
      <div class="pl-summary" style=${`color:${rgba(q.text)}`}>${s.summary}</div>
      <div class="pl-gaps" style=${`color:${ink(q, s.ok ? 'ok' : 'warn')}`}>
        <span class="cm-icon" style="left:0;top:3px"><${Icon} name=${s.ok ? 'check_circle' : 'warning'} size=${14} /></span>
        <${Para} px=${12.5} weight=${500} color=${ink(q, s.ok ? 'ok' : 'warn')} style="margin-left:20px">${s.gaps}</${Para}>
      </div>
      ${s.morning.length > 0 &&
      html`<${Head}>${s.heads.morning}</${Head}>
        ${s.morning.map((m) => html`<div class="pl-morning"><i style=${`background:${ink(q, m.tone)}`}></i><${Para} px=${12.5} color=${rgba(q.text, 0.82)}>${m.text}</${Para}></div>`)}
        <div style="height:8px"></div>`}
      ${s.open.length > 0 &&
      html`<${Head}>${s.heads.open}</${Head}>
        ${s.open.map((o) => {
          const col = ink(q, o.tone);
          const sel = o.duty == null ? { line: o.line, tour: o.tour } : { line: o.line, tour: o.tour, k: o.duty };
          return html`<button type="button" class="pl-open" style=${`--h:${rgba(q.text, 0.05)};--c:${col};--f:${rgba(q.faint)};--t:${rgba(q.text)}`}
              onPointerDown=${(e) => drag.pressCarry(e, o.ti, o.duty, o.title)} onClick=${() => setUi('planning', { sel })}>
            <i class="pl-openbar"></i>
            <span class="cm-icon" style=${`left:9px;top:7px;color:${col}`}><${Icon} name=${o.hard ? 'error' : 'warning'} size=${14} /></span>
            <span class="cm-icon pl-move" style=${`right:6px;top:15px`}><${Icon} name="open_with" size=${15} /></span>
            <${TextIn} x=${28} y=${4} w=${w - 50} h=${20} px=${12.5} weight=${700} color=${rgba(q.text)}>${o.title}</${TextIn}>
            <${TextIn} x=${28} y=${23} w=${w - 50} h=${18} px=${12} color=${col}>${o.fill}</${TextIn}>
          </button>`;
        })}
        <div style="height:8px"></div>`}
      <div class="pl-caps" style=${`color:${rgba(q.dim)};margin-bottom:4px`}>${s.heads.drivers}</div>
      <div style=${`position:relative;height:24px`}><${TextIn} x=${0} y=${0} w=${w} h=${18} px=${12} color=${rgba(q.dim)}>${s.hint}</${TextIn}></div>
      ${[s.me, ...s.staff].map((e) => {
        const arm = { driver: e.who };
        const fill = e.who === 'player' ? rgba(q.accent) : e.colour;
        return html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w, h: 30 }} on=${e.on} swatch=${e.who === 'player' ? 'accent' : e.colour} text=${e.name} right=${e.state} rightTone=${e.tone || 'dim'}
          fits=${fitsDriver} target=${{ 'data-target': '1', 'data-driver': e.who }}
          onClick=${() => tapArm(arm)} onPointerDown=${(ev) => drag.pressArm(ev, arm, e.who === 'player' ? e.name : e.name.split(/\s+/)[0], fill, e.who === 'player' ? '#fff' : 'rgb(14,18,28)')} /></div>`;
      })}
      ${!s.staff.length && html`<div style="position:relative;height:22px"><${TextIn} x=${0} y=${0} w=${w} h=${20} px=${12.5} color=${rgba(q.dim)}>${s.no_staff}</${TextIn}></div>`}
      <div style="height:10px"></div>
      <${Head}>${s.heads.buses}</${Head}>
      ${s.buses.map((b) => {
        const arm = { bus: b.id };
        return html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w, h: 30 }} on=${b.on} text=${b.name} right=${b.state} rightTone=${b.tone}
          fits=${fitsBus} target=${{ 'data-target': '1', 'data-bus': String(b.id) }}
          onClick=${() => tapArm(arm)} onPointerDown=${(ev) => drag.pressArm(ev, arm, b.name.split(/\s+/)[0], rgba(q.tool), rgba(q.text))} /></div>`;
      })}
      ${!s.buses.length && html`<div style="position:relative;height:22px"><${TextIn} x=${0} y=${0} w=${w} h=${20} px=${12.5} color=${rgba(q.dim)}>${s.no_buses}</${TextIn}></div>`}
    </div>
  </div>`;
}

/** The way from a duty or a tour to its line's trips and timetable (`rf_roster::timetable_button`). */
function TtButton({ y, w, tt, onClick }) {
  const q = pal();
  return html`<button type="button" class="pl-link" title=${tt.tip} style=${box(0, y, w, 32) + `;--a:${rgba(q.accent)};--a12:${rgba(q.accent, 0.12)};--t:${rgba(q.text)};--t3:${rgba(q.text, 0.03)};--t16:${rgba(q.text, 0.16)}`} onClick=${onClick}>
    <span class="cm-icon" style="left:10px;top:8px"><${Icon} name="departure_board" size=${16} /></span><span class="pl-link-l">${tt.label}</span>
  </button>`;
}

/** A duty's or a tour's head, its timetable and its facts (`duty_panel`, `tour_panel`). */
function Facts({ s, v, w }) {
  const q = pal();
  return html`<div style="position:relative;height:32px">
      <i class="pl-swatch" style=${`left:0;top:5px;height:14px;background:${s.colour}`}></i>
      <${TextIn} x=${10} y=${0} w=${w - 14} h=${24} px=${15} weight=${700} color=${rgba(q.text)}>${s.head}</${TextIn}>
    </div>
    <div style="position:relative;height:42px"><${TtButton} y=${0} w=${w - 4} tt=${s.tt} onClick=${() => openSheet({ line: s.line, tour: s.tour, date: v.date })} /></div>
    ${s.facts.map((f) => html`<div style="margin:0 6px 4px 0;min-height:18px"><${Para} px=${12.5} color=${ink(q, f.tone)}>${f.text}</${Para}></div>`)}
    <div style="height:8px"></div>`;
}

/** A duty chosen (`rf_roster::duty_panel`): who drives it and who else could, what to do while
 * it is open today, and "Drive this duty". */
function DutyPanel({ s, v, inner }) {
  const q = pal();
  const w = inner.w;
  const lw = w - BAR - 2;
  const footH = s.drive ? 50 : 0;
  const give = (who) => !s.by_player && actMore('plan_give_driver', { date: v.date, ti: s.ti, k: s.k, who });
  return html`<div style=${box(inner.x, inner.y, w, inner.h) + ';display:flex;flex-direction:column'}>
    <div style="flex:none"><${Facts} s=${s} v=${v} w=${w} /></div>
    <div class="pl-scroll" style=${`flex:1;min-height:0;margin-bottom:${footH}px`}>
      <div style=${`width:${lw}px;padding-bottom:8px`}>
        ${s.options.length > 0 &&
        html`<div class="pl-caps" style=${`color:${rgba(q.dim)}`}>${s.options_title}</div>
          ${s.options.map((o) => html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w: lw, h: 30 }} on=${o.on} text=${o.label} right=${o.note} rightTone="dim" onClick=${() => actMore('plan_set_fill', { key: o.key, fill: o.fill })} /></div>`)}
          <div style="height:10px"></div>`}
        <div class="pl-caps" style=${`color:${rgba(q.dim)}`}>${s.cands_title}</div>
        ${s.cands.map((c) => html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w: lw, h: 30 }} on=${c.on && c.who != null} swatch=${c.swatch} text=${c.name} right=${c.state} rightTone=${c.tone} onClick=${() => give(c.who)} /></div>`)}
      </div>
    </div>
    ${s.drive && html`<${RfButton} r=${{ x: 0, y: inner.h - 38, w: w - 4, h: 36 }} label=${s.drive} solid onClick=${() => actMore('plan_drive', { date: v.date, ti: s.ti, k: s.k })} />`}
  </div>`;
}

/** A tour chosen (`rf_roster::tour_panel`): its bus and which others could run it; on the
 * company's day a spare or rental bus, or dropping it. */
function TourPanel({ s, v, inner }) {
  const q = pal();
  const w = inner.w;
  const lw = w - BAR - 2;
  return html`<div style=${box(inner.x, inner.y, w, inner.h) + ';display:flex;flex-direction:column'}>
    <div style="flex:none"><${Facts} s=${s} v=${v} w=${w} /></div>
    <div class="pl-scroll" style="flex:1;min-height:0">
      <div style=${`width:${lw}px;padding-bottom:8px`}>
        ${s.options.length > 0 &&
        html`<div class="pl-caps" style=${`color:${rgba(q.dim)}`}>${s.options_title}</div>
          ${s.options.map((o) => html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w: lw, h: 30 }} on=${o.on} text=${o.label} right=${o.note} rightTone="dim" onClick=${() => o.live && actMore('plan_set_fill', { key: o.key, fill: o.fill })} /></div>`)}
          <div style="height:10px"></div>`}
        ${s.cands.length > 0 &&
        html`<div class="pl-caps" style=${`color:${rgba(q.dim)}`}>${s.cands_title}</div>
          ${s.cands.map((c) => html`<div class="pl-slot"><${PickRow} r=${{ x: 0, y: 0, w: lw, h: 30 }} on=${c.on} text=${c.name} right=${c.state} rightTone=${c.tone} onClick=${() => actMore('plan_give_bus', { date: v.date, ti: s.ti, bus: c.bus })} /></div>`)}`}
      </div>
    </div>
  </div>`;
}

// --- putting a line into service ---------------------------------------------------------------------

/** "Start the service of line …" (`planning::service_dialog`): from now, from tomorrow or from a
 * date; the line's next seven days as they are planned; with gaps, the player's word. */
function ServiceDialog({ s }) {
  const v = useMore('service', s);
  const set = (patch) => setUi('planning', { service: { ...s, ...patch } });
  const close = () => setUi('planning', { service: null });
  if (!v || v.error) return null;
  if (v.gone) {
    close();
    return null;
  }
  const go = async () => {
    const r = await actMore('plan_start', s);
    if (r?.done) close();
  };
  const foot = html`<span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} /><${Btn} kind="primary" icon="play_arrow" label=${v.go} onClick=${go} />`;
  return html`<${Dialog} icon="play_arrow" title=${v.title} w=${760} h=${640} onClose=${close} foot=${foot}>
    <p class="co-para" style="line-height:20.7px">${v.intro}</p>
    <div class="co-caps" style="margin:16px 0 10px">${v.from}</div>
    <div style="display:flex;gap:14px;align-items:center">
      <${Seg} value=${s.when} options=${v.ways.map((t, k) => [k, t])} onChange=${(k) => set({ when: k })} style="width:520px;max-width:100%;height:40px" />
      ${s.when === 2 && html`<input type="date" class="co-field" style="flex:1;min-width:160px;height:40px" value=${s.date || v.date} onChange=${(e) => e.currentTarget.value && set({ date: e.currentTarget.value })} />`}
    </div>
    <div class="co-caps" style="margin:16px 0 10px">${v.week}</div>
    ${v.days.map((d) => html`<div class="pl-weekday"><span>${d.day}</span><b class=${'t-' + d.tone}>${d.text}</b></div>`)}
    ${v.short &&
    html`<p class="co-para t-warn" style="margin-top:12px;font-weight:500;line-height:20.7px">${v.short}</p>
      <div style="margin-top:8px"><${Switch} checked=${s.gaps} label=${v.gaps} onChange=${(on) => set({ gaps: on })} /></div>`}
  </${Dialog}>`;
}

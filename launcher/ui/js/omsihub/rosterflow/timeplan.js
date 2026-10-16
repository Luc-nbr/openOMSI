// RosterFlow's Timeplan (`crates/omsi-launcher-ui/src/company/planning/timeplan.rs`; Luc: the
// company's planning should look and work "exactly" like Optibus Planning): a line's running
// times and its timetable, in Optibus's dark slate look with one sky-blue accent - orange for
// what was typed over and for warnings, a red "!" for errors - and the light look made of it.
//
// On top the context bar: the line, New line, the day group, undo and redo, OUTBOUND / INBOUND,
// the two screens and saving. Both screens have the stop strip: the stops' names at 45° in grey
// italics, each stop's code under it, and a rail in the line's colour with a dot per stop.
// Running times: the time bands on a rail from 00:00 to 36:00 and per band a row of boxes with
// the minutes between two stops, clicked and typed. Timetable: the figures on top, the frozen
// columns PATTERN, VEHICLE TYPE, START, HEADWAY, then the time at every stop - a time typed over
// in orange with how far it is off; trips added one at a time or by headway, taken out. Rows are
// chosen as in a spreadsheet (a click, Shift+click a range, Ctrl+click one more, Ctrl+A all,
// Escape none) and the trips chosen moved, given another pattern or taken out together.
// "Copy to…" (`tpcopy.js`) copies the day, the trips chosen or a template onto other day groups.
// Saved, the line is UNSYNCED until "Refresh" makes its blocks and duties anew.
//
// The line being changed lives on the Rust side (`rf_tp*`); the page keeps what it shows (the day
// group, the side, the pattern, the screen, the cell typed into, the trips chosen) in `S.rf.tp`.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { handOver, setCo, cmd as coCmd } from '../company/kit.js';
import { setRf, setPart, rfCmd, after, bump, say } from './state.js';
import { Icon } from './ui.js';
import { CopyPanel, openCopy, chosenTrips } from './tpcopy.js';

const RUNNING = 0;
const TIMETABLE = 1;
/** The context bar, the figures' band, the status bar; the strip's room; a band's row, a trip's
 * row; the running times' left column; the timetable's frozen columns; the form's height. */
const BAR_H = 48;
const KPI_H = 74;
const STATUS_H = 32;
const STRIP_H = 150;
const RT_ROW = 46;
const TT_ROW = 36;
const LEFT = 168;
const FROZEN = [40, 74, 104, 78, 84];
const FORM_H = 64;
const CHARTS = ['Number of trips', 'Trip count by pattern', 'Trip count by hour'];

/** Columns of `n` in `room` pixels: as wide as they share it, between `least` and `most` (wider
 * than the room: scrolled, `fit_cols`). */
function fitCols(x, room, n, least, most, scroll) {
  const w = Math.max(least, Math.min(most, room / Math.max(1, n)));
  const over = Math.max(0, w * n - room);
  const s = Math.max(0, Math.min(over, scroll));
  return { x, w, scroll: s, over, cx: (j) => x + (j + 0.5) * w - s };
}

/** A time typed: "7:05", "07.05", "0705", up to 36:00 (`timeplan::parse_time`). */
function parseTime(s) {
  s = String(s).trim();
  let h;
  let m;
  const sp = s.split(/[:.h]/);
  if (sp.length === 2) [h, m] = sp.map((x) => parseInt(x.trim(), 10));
  else if (/^\d{3,}$/.test(s)) [h, m] = [parseInt(s.slice(0, -2), 10), parseInt(s.slice(-2), 10)];
  else if (/^\d{1,2}$/.test(s)) [h, m] = [parseInt(s, 10), 0];
  else return null;
  if (!(h >= 0 && h <= 36 && m >= 0 && m < 60) || h * 60 + m > 2160) return null;
  return h * 60 + m;
}

/** Headways as typed ("10", "10,20"): none when not minutes (`timeplan::parse_headways`). */
function parseHeadways(s) {
  const v = String(s).split(/[,;/ ]/).filter((x) => x.trim()).map((x) => parseFloat(x.trim()));
  return v.length && v.every((m) => m >= 1 && m <= 600) ? v : null;
}

/** What the form would make: how many trips (`timeplan::generate`). */
function generated(first, last, heads) {
  let n = 0;
  for (let t = first, k = 0; t <= last + 1e-3 && n < 1000; k++) {
    n++;
    t += Math.max(1, heads[k % heads.length]);
  }
  return n;
}

/** The Timeplan's view (`rf_tp`), asked again when what is shown changed or after an action. */
function useTp() {
  const r = S.rf;
  const t = r.tp;
  const [v, setV] = useState(null);
  const gen = S.co.shell?.gen ?? 0;
  useEffect(() => {
    let gone = false;
    rfCmd('rf_tp', { day: t.day, side: t.side, pattern: t.pattern, tab: t.tab, points_only: t.pointsOnly })
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ error: e.message }));
    return () => {
      gone = true;
    };
  }, [t.day, t.side, t.pattern, t.tab, t.pointsOnly, r.bump, gen, language(), t.rev]);
  return v;
}

const tpSet = (patch) => setPart('tp', patch);
const refreshTp = () => tpSet((t) => ({ rev: (t.rev || 0) + 1 }));

/** An action of the Timeplan (`rf_tp_act`) on what is shown; what it says said. */
async function tpAct(act, args = {}) {
  const t = S.rf.tp;
  let r;
  try {
    r = await rfCmd('rf_tp_act', { act, day: t.day, side: t.side, pattern: t.pattern, tab: t.tab, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  refreshTp();
  if (act === 'save' || act === 'refresh') bump();
  return r;
}

/** Escape on the Timeplan (`timeplan::keys`): the cell, what is open, then - with changes not
 * saved - a word first. Returns whether it took the key. */
export function tpEscape() {
  const t = S.rf.tp;
  if (t.cell) {
    tpSet({ cell: null, text: '' });
    return true;
  }
  if (t.copying) {
    tpSet({ copying: null });
    return true;
  }
  if (t.adding || t.chart != null || t.issuesOpen || t.sel != null || t.picked?.length) {
    tpSet({ adding: null, chart: null, issuesOpen: false, sel: null, picked: [], anchor: null });
    return true;
  }
  if (S.rf.tpDirty && !t.leaving) {
    tpSet({ leaving: true });
    return true;
  }
  tpSet({ leaving: false });
  return false;
}

export function TimeplanTab() {
  const r = S.rf;
  const t = r.tp;
  const v = useTp();
  // ("View in timeplan", an issue, a line's timetable: that line, with what to do next)
  useEffect(() => {
    if (!r.tpLine) return;
    const line = r.tpLine;
    const hint = r.tpHint;
    const reload = !!r.tpReload;
    // (from a day of Vehicles or Drivers: the day group that day runs, `tpDay`)
    const day = r.tpDay;
    setRf({ tpLine: null, tpHint: null, tpReload: false, tpDay: null });
    rfCmd('rf_tp_guide', { line, hint, reload, ...(day != null ? { day } : {}) }).then((x) => {
      tpSet({ cell: null, text: '', sel: null, picked: [], anchor: null, adding: null, ...(x?.screen != null ? { tab: x.screen } : {}), ...(x?.group != null ? { day: x.group, chart: null, issuesOpen: false, scroll: 0 } : {}), rev: (S.rf.tp.rev || 0) + 1 });
    });
  }, [r.tpLine]);
  // (what the lessons look at: the line read, changed, saved, refreshed)
  useEffect(() => {
    if (!v) return;
    S.rf.tpDirty = !!v.dirty;
    setRf({ tpProbe: v.probe || { ready: false, why: v.error || null } });
  }, [v]);
  useTpKeys(v);
  if (!v) return html`<div class="rf-tp"></div>`;
  if (v.error) return html`<div class="rf-tp"><p class="tp-why">${v.error}</p></div>`;
  return html`<div class="rf-tp" data-rf="rf-tp">
    <${ContextBar} v=${v} />
    ${t.tab === TIMETABLE && v.ready && html`<${Kpis} v=${v} />`}
    <div class=${"tp-body" + (v.ready && !v.no_side ? "" : " empty")} style=${`top:${BAR_H + 6 + (t.tab === TIMETABLE && v.ready ? KPI_H + 6 : 0)}px`} data-rf=${v.ready ? 'rf-tp-body' : undefined}>
      ${!v.ready ? html`<p class="tp-why">${v.why || tr('The company runs no line yet: add one on the Lines page first.')}</p>` : v.no_side ? html`<p class="tp-why">${v.no_side}</p>` : t.tab === RUNNING ? html`<${Running} v=${v} />` : html`<${Timetable} v=${v} />`}
    </div>
    ${v.ready && html`<${StatusBar} v=${v} />`}
    ${t.issuesOpen && v.ready && html`<${IssuesPanel} v=${v} />`}
    ${t.chart != null && v.tt && html`<${ChartCard} v=${v} />`}
    ${t.copying && v.ready && v.copy && html`<${CopyPanel} v=${v} act=${tpAct} />`}
  </div>`;
}

// --- the context bar --------------------------------------------------------------------------------

/** A new line: its route drawn in the line editor (route only), its timetable made here once it
 * is confirmed - the line editor of the desktop launcher (Trello "Tauri 5/6": `openLineEditor`)
 * where it is, else our game-drawn launcher's. */
async function newLine() {
  // (company multiplayer: a new line's route is drawn by the boss's line editor, on his map)
  if (S.co?.remote) {
    say(tr("The routes are drawn on the boss's map with his line editor: he changes them on his own launcher. The trips and their times you change in the Timeplan, as your role allows."), true);
    return;
  }
  const company = S.co.shell?.id;
  let m = null;
  try {
    m = await import('../editors/index.js');
  } catch {
    m = null;
  }
  const open = m?.openLineEditor || window.omsihub?.openLineEditor;
  if (!open) {
    handOver('line-editor');
    return;
  }
  // (the clock stopped first: the line editor writes the company's file; read again after it)
  await coCmd('co_leave').catch(() => {});
  const reread = (id) => coCmd('co_boot').then(() => coCmd('co_open', { id: id || company })).catch(() => {});
  open({
    company,
    back: 'company',
    onTimeplan: async ({ id, name, hint } = {}) => {
      await reread(company);
      // (a line just confirmed comes by the line editor's id, as `lineeditor::after_confirm`
      // gives it: the Timeplan takes the company's name of it, `ownline::line_of`)
      const line = name || (id != null && (await coCmd('co_line_of', { id }).catch(() => null))?.name) || '';
      set({ page: 'company' });
      setRf({ open: true, view: 3, tpLine: line, tpHint: hint ? 'new' : null, tpReload: true });
      setCo({ tab: 5 });
    },
    onCompanyChanged: (id) => reread(id).then(() => bump()),
  });
}

function ContextBar({ v }) {
  const t = S.rf.tp;
  const lineAt = Math.max(0, v.lines.findIndex((l) => l.name.toLowerCase() === String(v.line).toLowerCase()));
  const [bar, box] = useBox();
  const wide = (box.w || 1400) >= 1240;
  const pickLine = (k) => {
    const l = v.lines[k];
    if (!l || l.name === v.line) return;
    if (v.dirty && !t.leaving) {
      tpSet({ leaving: true });
      say(tr('The timeplan has changes not saved: choose again to drop them.'), true);
      return;
    }
    tpSet({ leaving: false, sel: null, picked: [], cell: null, adding: null, scroll: 0, hscroll: 0, pattern: 0 });
    tpAct('line', { line: l.name });
  };
  return html`<div class="tp-bar" data-rf="rf-tp-bar" ref=${bar}>
    ${v.lines.length > 0 && html`<span class="tp-select" style="flex:0 1 220px;min-width:110px"><select value=${String(lineAt)} onChange=${(e) => pickLine(Number(e.currentTarget.value))}>${v.lines.map((l, k) => html`<option value=${String(k)}>${l.label}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>`}
    <button type="button" class=${'tp-new' + (wide ? '' : ' narrow')} data-rf="rf-tp-new-line" title=${tr('New line: draw its route in the line editor, then make its timetable here')} onClick=${() => {
      if (v.dirty && !t.leaving) return tpSet({ leaving: true });
      tpSet({ leaving: false });
      newLine();
    }}><${Icon} name="add" size=${wide ? 18 : 20} />${wide && html`<span>${tr('New line')}</span>`}</button>
    <span class="tp-select" style="flex:0 1 150px;min-width:96px"><select value=${String(t.day)} onChange=${(e) => tpSet({ day: Number(e.currentTarget.value), cell: null, text: '', adding: null, chart: null, issuesOpen: false, sel: null, picked: [], scroll: 0 })}>${v.days.map((d, k) => html`<option value=${String(k)}>${d}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>
    ${v.calendar?.length > 0 && html`<${DayStrip} v=${v} />`}
    <button type="button" class="tp-ib" title=${tr('Undo')} onClick=${() => tpAct('undo').then(() => tpSet({ cell: null, sel: null, picked: [] }))}><${Icon} name="turn_left" size=${15} /></button>
    <button type="button" class="tp-ib" title=${tr('Redo')} onClick=${() => tpAct('redo').then(() => tpSet({ cell: null, sel: null, picked: [] }))}><${Icon} name="turn_right" size=${15} /></button>
    <i class="tp-sep"></i>
    <span class="tp-sides">${['Outbound', 'Inbound'].map((w, side) => {
      const enabled = side === 0 || v.has_inbound;
      return html`<button type="button" class=${(t.side === side ? 'on' : '') + (enabled ? '' : ' off')} onClick=${() => enabled && t.side !== side && tpSet({ side, pattern: 0, cell: null, text: '', adding: null, chart: null, issuesOpen: false, sel: null, picked: [], scroll: 0, hscroll: 0 })}>${tr(w).toUpperCase()}</button>`;
    })}</span>
    ${[[RUNNING, 'schedule', 'Running times'], [TIMETABLE, 'grid_view', 'Timetable']].map(([tab, icon, tip]) => html`<button type="button" class=${'tp-screen' + (t.tab === tab ? ' on' : '')} title=${tr(tip)} onClick=${() => t.tab !== tab && tpSet({ tab, cell: null, text: '' })}><${Icon} name=${icon} size=${20} /></button>`)}
    <i class="tp-sep"></i>
    ${v.runs && html`<span class="tp-from">${v.from?.show && html`<button type="button" class="tp-ib small" title=${tr('A day earlier')} onClick=${() => tpAct('from', { by: -1 })}><${Icon} name="chevron_left" size=${13} /></button><b>${v.from.label}</b><button type="button" class="tp-ib small" title=${tr('A day later')} onClick=${() => tpAct('from', { by: 1 })}><${Icon} name="chevron_right" size=${13} /></button>`}</span>`}
    <button type="button" class=${'tp-save' + (v.dirty ? ' dirty' : '')} data-rf="rf-tp-save" onClick=${() => v.dirty && tpAct('save').then((x) => {
      // (the trips chosen are places in the table: read again, or on the day group of an
      // error, they would be other trips - Delete would take those out)
      const off = { cell: null, text: '', sel: null, picked: [], anchor: null };
      tpSet(x?.day != null ? { ...off, day: x.day, issuesOpen: !!x.issues_open, adding: null, scroll: 0 } : off);
    })}><${Icon} name="save" size=${18} /><span>${v.save || tr('Saved')}</span></button>
  </div>`;
}

/** The days of the coming week that use the timetable shown (Trello #100): each day lit when it
 * runs this day group's timetable, marked when it runs another than its weekday's (a holiday's);
 * a click shows the day group it runs. */
function DayStrip({ v }) {
  const t = S.rf.tp;
  return html`<span class="tp-days" data-rf="rf-tp-days" title=${tr('The days of the coming week that run this timetable')}>
    ${v.calendar.map((d) => {
      const [day, num] = String(d.label).split(' ');
      const cls = 'd' + (d.on ? ' on' : '') + (d.odd ? ' odd' : '') + (!d.read ? ' wait' : d.group == null ? ' none' : '');
      return html`<button type="button" class=${cls} title=${`${d.label}: ${d.tip}`} onClick=${() => d.group != null && d.group !== t.day && tpSet({ day: d.group, cell: null, text: '', adding: null, chart: null, issuesOpen: false, sel: null, picked: [], scroll: 0 })}><b>${(day || '').slice(0, 2)}</b><small>${num || ''}</small></button>`;
    })}
  </span>`;
}

// --- the strip ------------------------------------------------------------------------------------------

/** The strip: the names at 45° in grey italics, the codes under them, the rail in the line's
 * colour at `railY` with a dot per stop (a timepoint full, a plain stop white inside). */
function Strip({ dots, cols, room, railY, colour, onDot }) {
  if (!dots.length) return null;
  const a = cols.cx(0);
  const b = cols.cx(dots.length - 1);
  return html`<div class="tp-strip" style=${`left:${room.x - 30}px;width:${room.w + 230}px`}>
    ${dots.map((d, j) => {
      const cx = cols.cx(j) - (room.x - 30);
      if (cols.cx(j) < room.x - cols.w || cols.cx(j) > room.x + room.w + cols.w) return null;
      return html`<span class="name" style=${`left:${cx - 8}px;top:${railY - 34}px`}>${d.name}</span>
        <span class="code" style=${`left:${cx - cols.w / 2}px;top:${railY - 27}px;width:${cols.w}px`}>${d.code}</span>`;
    })}
    <i class="rail" style=${`left:${a - (room.x - 30)}px;top:${railY - 2.5}px;width:${Math.max(0, b - a)}px;background:${colour}`}></i>
    <span class="unfold" style=${`left:${a - 18 - (room.x - 30) - 7}px;top:${railY - 7}px`}><${Icon} name="unfold_more" size=${14} /></span>
    ${dots.map((d, j) => {
      const cx = cols.cx(j);
      if (cx < room.x - 10 || cx > room.x + room.w + 10) return null;
      return html`<button type="button" class=${'dot' + (d.timepoint ? ' tp' : '') + (onDot ? ' click' : '')} style=${`left:${cx - (room.x - 30) - 10}px;top:${railY - 10}px;--c:${colour}`}
        title=${onDot ? tr(d.timepoint ? 'Timepoint: click to make it a plain stop' : 'Plain stop: click to make it a timepoint') : undefined} onClick=${() => onDot?.(j)}><i></i></button>`;
    })}
  </div>`;
}

/** A typed cell's text: what is being typed, with a caret. */
const Typed = ({ text, unit = '' }) => html`<span class="typed">${text}${unit}<i class="caret"></i></span>`;

/** The size of an element (its own units), measured. */
function useBox() {
  const ref = useRef(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const m = () => setBox((b) => (b.w === el.clientWidth && b.h === el.clientHeight ? b : { w: el.clientWidth, h: el.clientHeight }));
    const ro = new ResizeObserver(() => requestAnimationFrame(m));
    ro.observe(el);
    m();
    return () => ro.disconnect();
  }, []);
  return [ref, box];
}

/** A scrollbar along `w` for `view` out of `total` at `pos` (RosterFlow's, `rf_scroll::bar`). */
function HBar({ x, y, w, pos, view, total, onPos }) {
  if (total <= view + 0.5 || view <= 0) return null;
  const max = total - view;
  const len = Math.min(w, Math.max((w * view) / total, Math.min(28, w * 0.5)));
  const room = Math.max(1, w - len);
  const at = room * Math.max(0, Math.min(1, pos / max));
  const down = (e) => {
    e.preventDefault();
    const box = e.currentTarget.parentElement.getBoundingClientRect();
    const k = e.currentTarget.parentElement.offsetWidth ? box.width / e.currentTarget.parentElement.offsetWidth : 1;
    const grab = (e.clientX - box.left) / k - at;
    const mv = (ev) => onPos(Math.max(0, Math.min(1, ((ev.clientX - box.left) / k - grab) / room)) * max);
    const up = () => {
      removeEventListener('mousemove', mv);
      removeEventListener('mouseup', up);
    };
    addEventListener('mousemove', mv);
    addEventListener('mouseup', up);
  };
  return html`<div class="tp-hbar" style=${`left:${x}px;top:${y}px;width:${w}px`} onMouseDown=${(e) => {
    if (e.target !== e.currentTarget) return;
    const b = e.currentTarget.getBoundingClientRect();
    const k = e.currentTarget.offsetWidth ? b.width / e.currentTarget.offsetWidth : 1;
    const m = (e.clientX - b.left) / k;
    onPos(m < at ? Math.max(0, pos - view * 0.9) : Math.min(max, pos + view * 0.9));
  }}><i style=${`left:${at + 2}px;width:${Math.max(8, len - 4)}px`} onMouseDown=${down}></i></div>`;
}

// --- running times ------------------------------------------------------------------------------------

/** A running time as Optibus shows it: the minutes large, a small grey "m" after them. */
const Minutes = ({ m, text }) => html`<span class=${'mins' + (m < -1e-3 ? ' neg' : Math.abs(m) <= 1e-3 ? ' zero' : '')}><b>${text}</b><small>m</small></span>`;

function Running({ v }) {
  const t = S.rf.tp;
  const rt = v.rt;
  const [ref, box] = useBox();
  const [hover, setHover] = useState(null);
  if (!rt) return html`<div class="tp-pane" ref=${ref}></div>`;
  const n = rt.n;
  const railY = STRIP_H;
  const room = { x: LEFT + 40, w: Math.max(100, box.w - LEFT - 150) };
  const cols = fitCols(room.x, room.w, n, 70, 150, t.hscroll);
  const gy = railY + 44;
  const railX = 132;
  const bands = rt.bands.length;
  const shown = Math.min(bands, Math.max(1, Math.floor((box.h - gy - 60) / RT_ROW)));
  const choose = (cell) => tpSet({ cell, text: '' });
  const isCell = (c) => t.cell && Object.keys(c).every((k) => t.cell[k] === c[k]);
  const wheel = (e) => {
    if (e.deltaX || e.shiftKey) {
      e.preventDefault();
      tpSet({ hscroll: Math.max(0, Math.min(cols.over, t.hscroll + (e.deltaX || e.deltaY) * 0.6)) });
    }
  };
  const lastX = cols.cx(n - 1);
  return html`<div class="tp-pane" ref=${ref} onWheel=${wheel}>
    <i class="tp-vline" style=${`left:${LEFT - 1}px`}></i>
    ${v.patterns.length > 1
      ? html`<span class="tp-select" style=${`left:12px;top:${railY - 17}px;width:${LEFT - 24}px;position:absolute`}><select value=${String(rt.pattern)} onChange=${(e) => tpSet({ pattern: Number(e.currentTarget.value), cell: null })}>${v.patterns.map((p, k) => html`<option value=${String(k)}>${p.label}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>`
      : html`<span class="tp-code" style=${`left:12px;top:${railY - 17}px`} title=${tr('The main pattern')}>-</span>`}
    ${rt.few
      ? html`<p class="tp-why" style=${`left:${LEFT + 24}px`}>${rt.few}</p>`
      : html`
      <${Strip} dots=${rt.dots} cols=${cols} room=${room} railY=${railY} colour=${v.colour} onDot=${(k) => tpAct('timepoint', { k })} />
      <span class="tp-caps" style=${`left:12px;top:${gy - 26}px`}>${tr('Time bands').toUpperCase()}</span>
      <i class="tp-rail" style=${`left:${railX - 1.5}px;top:${gy}px;height:${shown * RT_ROW + 34}px`}></i>
      ${rt.bands.slice(0, shown).map((band, k) => {
        const y = gy + k * RT_ROW;
        const chosen = isCell({ kind: 'band_from', band: k });
        const hov = hover === k;
        return html`<div class="tp-band" style=${`top:${y}px;height:${RT_ROW}px`} onMouseEnter=${() => setHover(k)} onMouseLeave=${() => setHover(null)}>
          <button type="button" class=${'from' + (chosen ? ' on' : '')} style="left:40px;width:76px" disabled=${k === 0} title=${k > 0 ? tr('When the band begins: click and type a time') : undefined} onClick=${() => choose({ kind: 'band_from', band: k })}>${chosen && t.text ? html`<${Typed} text=${t.text} />` : band.from}</button>
          <i class=${'knob' + (chosen || hov ? ' on' : '')} style=${`left:${railX - 6}px;top:${RT_ROW / 2 - 6}px`}></i>
          ${bands > 1 && hov && html`<button type="button" class="tp-round" style=${`left:${18 - 12}px;top:${RT_ROW / 2 - 12}px`} title=${tr('Take the band out')} onClick=${() => tpAct('band_remove', { k }).then(() => tpSet({ cell: null }))}><${Icon} name="delete" size=${13} /></button>`}
          ${k + 1 < bands && hov && html`<button type="button" class="tp-round small" style=${`left:${railX - 9}px;top:${RT_ROW - 9}px`} title=${tr('Add a time band here')} onClick=${() => tpAct('band_add', { at: Math.round((band.from_m + rt.bands[k + 1].from_m) / 2 / 15) * 15 }).then((x) => x?.band != null && tpSet({ cell: { kind: 'band_from', band: x.band }, text: '' }))}><${Icon} name="add" size=${10} /></button>`}
          ${band.run.map((m, i) => {
            const x0 = cols.cx(i);
            const w = cols.cx(i + 1) - x0;
            if (x0 + w < room.x - 2 || x0 > room.x + room.w + cols.w) return null;
            const on = isCell({ kind: 'run', band: k, seg: i });
            return html`<button type="button" class=${'tp-cell' + (on ? ' on' : '')} style=${`left:${x0}px;width:${w}px;height:${RT_ROW}px`} onClick=${() => choose({ kind: 'run', band: k, seg: i })}>${on && t.text ? html`<${Typed} text=${t.text} unit=" m" />` : html`<${Minutes} m=${m} text=${band.texts[i]} />`}</button>`;
          })}
          ${lastX + 14 < box.w - 20 && html`<span class="total" style=${`left:${lastX + 14}px`}>${band.total}</span>`}
        </div>`;
      })}
      ${shown < bands && html`<span class="tp-more" style=${`left:12px;top:${gy + shown * RT_ROW}px`}>${tr('%{n} more bands', { n: bands - shown })}</span>`}
      ${lastX + 14 < box.w - 20 && html`<span class="tp-caps" style=${`left:${lastX + 14}px;top:${gy - 26}px`}>${tr('Trip time').toUpperCase()}</span>`}
      <i class="tp-edge" style=${`left:${lastX}px;top:${gy}px;width:1px;height:${shown * RT_ROW}px`}></i>
      <i class="tp-edge" style=${`left:${cols.cx(0)}px;top:${gy + shown * RT_ROW}px;width:${lastX - cols.cx(0)}px;height:1px`}></i>
      <span class="tp-end" style=${`left:40px;top:${gy + shown * RT_ROW + 12}px`}>36:00</span>
      <button type="button" class="tp-round small" style=${`left:${railX - 10}px;top:${gy + shown * RT_ROW + 12}px`} title=${tr('Add a time band')} onClick=${() => tpAct('band_add').then((x) => x?.band != null && tpSet({ cell: { kind: 'band_from', band: x.band }, text: '' }))}><${Icon} name="add" size=${11} /></button>
      <${HBar} x=${room.x} y=${box.h - 12 - 4} w=${room.w} pos=${cols.scroll} view=${room.w} total=${cols.w * n} onPos=${(x) => tpSet({ hscroll: x })} />`}
  </div>`;
}

// --- the timetable ------------------------------------------------------------------------------------------

/** Trip `i` chosen alone (none: nothing chosen). */
const only = (i) => (i == null ? { sel: null, picked: [], anchor: null } : { sel: i, picked: [i], anchor: i });

/** A click on a row's number (`keep`: its pattern, which never lets it go): that trip alone, or
 * none when it was the only one; Shift - every row from the one clicked before; Ctrl (Cmd) - it
 * added or taken away; both - the range added. */
function pickRow(e, rows, row, keep = false) {
  const t = S.rf.tp;
  const now = chosenTrips(t);
  const add = e.ctrlKey || e.metaKey;
  if (e.shiftKey) {
    // (nothing chosen: no range - the anchor may be of another line or day, or undone)
    const at = now.length ? rows.findIndex((x) => x.i === (t.anchor ?? t.sel)) : -1;
    const k = rows.findIndex((x) => x.i === row.i);
    if (at < 0 || k < 0) return tpSet({ ...only(row.i), cell: null });
    const range = rows.slice(Math.min(at, k), Math.max(at, k) + 1).map((x) => x.i);
    return tpSet({ sel: row.i, picked: add ? [...new Set([...now, ...range])] : range, cell: null });
  }
  if (add) {
    const off = now.includes(row.i);
    const picked = off ? now.filter((i) => i !== row.i) : [...now, row.i];
    return tpSet({ picked, sel: off ? picked[picked.length - 1] ?? null : row.i, anchor: row.i, cell: null });
  }
  const alone = !keep && now.length === 1 && now[0] === row.i;
  tpSet({ ...only(alone ? null : row.i), cell: null });
}

/** The trips chosen taken out (one: as the trip's own Delete). */
function deleteChosen(chosen) {
  const done = () => tpSet({ ...only(null), cell: null });
  if (chosen.length === 1) return tpAct('delete', { i: chosen[0] }).then(done);
  return tpAct('bulk_delete', { trips: chosen }).then(done);
}

/** The bar over the frozen columns while trips are chosen: how many, Escape's cross; moved
 * earlier or later by the minutes typed, another pattern for them all. */
function SelBar({ v, chosen, width, top }) {
  const t = S.rf.tp;
  const by = t.moveBy ?? '5';
  const m = parseFloat(String(by).replace(',', '.'));
  const ok = m > 0 && m <= 720;
  const move = (sign) => (ok ? tpAct('bulk_shift', { trips: chosen, by: sign * m }) : say(tr('Type the minutes, such as 7'), true));
  return html`<div class="tp-selbar" style=${`left:10px;top:${top}px;width:${width}px`} data-rf="rf-tp-selbar">
    <div class="line">
      <b class="cut">${tr('%{n} trips chosen', { n: chosen.length })}</b>
      <span class="grow"></span>
      <button type="button" class="tp-ib small" title=${tr('Choose none (Escape)')} onClick=${() => tpSet(only(null))}><${Icon} name="close" size=${14} /></button>
    </div>
    <div class="line">
      <span class="lbl">${tr('Move')}</span>
      <button type="button" class="tp-ib small" title=${tr('The chosen trips earlier by these minutes')} onClick=${() => move(-1)}><${Icon} name="remove" size=${14} /></button>
      <input value=${by} onInput=${(e) => tpSet({ moveBy: e.currentTarget.value })} onKeyDown=${(e) => e.key === 'Enter' && move(1)} />
      <button type="button" class="tp-ib small" title=${tr('The chosen trips later by these minutes')} onClick=${() => move(1)}><${Icon} name="add" size=${14} /></button>
      <span class="lbl">${tr('min')}</span>
      ${v.patterns.length > 1 && html`<span class="tp-select"><select value="-1" title=${tr('The pattern the chosen trips run')} onChange=${(e) => {
        const k = Number(e.currentTarget.value);
        e.currentTarget.value = '-1';
        if (k >= 0) tpAct('bulk_pattern', { trips: chosen, pattern: k });
      }}><option value="-1">${tr('Pattern…')}</option>${v.patterns.map((p, k) => html`<option value=${String(k)}>${p.label}</option>`)}</select><${Icon} name="expand_more" size=${16} /></span>`}
    </div>
  </div>`;
}

function Timetable({ v }) {
  const t = S.rf.tp;
  const tt = v.tt;
  const [ref, box] = useBox();
  if (!tt) return html`<div class="tp-pane" ref=${ref}></div>`;
  const frozenW = FROZEN.reduce((s, x) => s + x, 0);
  const room = { x: frozenW + 10, w: Math.max(80, box.w - frozenW - 20) };
  const ncol = tt.dots.length;
  const cols = fitCols(room.x, room.w, ncol, 78, 124, t.hscroll);
  const railY = STRIP_H - 30;
  const hy = railY + 22;
  let y0 = hy + 34;
  if (t.adding) y0 += FORM_H;
  const wide = cols.w * ncol > room.w + 0.5;
  const gridH = Math.max(0, box.h - y0 - 4 - (wide ? 14 : 0));
  const visible = Math.max(1, Math.floor(gridH / TT_ROW));
  const rows = tt.rows;
  const maxScroll = Math.max(0, rows.length - visible);
  const scroll = Math.min(maxScroll, t.scroll);
  const choose = (cell, i) => tpSet({ cell, text: '', ...only(i) });
  const isCell = (c) => t.cell && Object.keys(c).every((k) => t.cell[k] === c[k]);
  const wheel = (e) => {
    e.preventDefault();
    if (e.deltaX || e.shiftKey) tpSet({ hscroll: Math.max(0, Math.min(cols.over, t.hscroll + (e.deltaX || e.deltaY) * 0.6)) });
    else tpSet({ scroll: Math.max(0, Math.min(maxScroll, scroll + Math.sign(e.deltaY) * 3)) });
  };
  const tool = (anchor, icon, tip, on, click) => html`<button type="button" class=${'tp-tool' + (on ? ' on' : '')} data-rf=${anchor} title=${tr(tip)} onClick=${click}><${Icon} name=${icon} size=${18} /></button>`;
  const sel = t.sel;
  const chosen = chosenTrips(t);
  const needSel = () => say(tr('Choose a trip first: click its number.'));
  return html`<div class="tp-pane" ref=${ref} onWheel=${wheel}>
    <${Strip} dots=${tt.dots} cols=${cols} room=${room} railY=${railY} colour=${v.colour} />
    ${chosen.length > 1 && html`<${SelBar} v=${v} chosen=${chosen} width=${frozenW - 30} top=${12} />`}
    <span class="tp-tools" style=${`left:10px;top:${railY - 16}px`}>
      <span data-rf="rf-tp-add" class="pair">
        ${tool(null, 'add', 'Add a single trip (after the one chosen, at its headway)', false, () => tpAct('add_one', { sel }).then((x) => x?.sel != null && tpSet(only(x.sel))))}
        ${tool(null, 'schedule', 'Add trips by headway from a control point', !!t.adding, () => tpSet({ adding: t.adding ? null : { pattern: Math.max(0, v.patterns.findIndex((p) => p.code === rows.find((x) => x.i === sel)?.code)), control: 0, first: '06:00', last: '22:00', headway: '15' } }))}
      </span>
      ${tool(null, 'delete', 'Take the chosen trips out (Delete)', false, () => (chosen.length ? deleteChosen(chosen) : needSel()))}
      ${tool(null, 'restart_alt', "The chosen trips' times from the running times again (their typed times go)", false, () => (chosen.length ? tpAct('bulk_revert', { trips: chosen }) : needSel()))}
      ${tool(null, 'flag', 'Timepoints only', t.pointsOnly, () => tpSet({ pointsOnly: !t.pointsOnly }))}
      ${tool('rf-tp-copy', 'content_copy', 'Copy to… other days: this day, the trips chosen or a template', !!t.copying, () => (t.copying ? tpSet({ copying: null }) : openCopy(chosen)))}
    </span>
    <div class="tp-heads" style=${`top:${hy}px`}>${['#', 'Pattern', 'Vehicle type', 'Start', 'Headway'].map((h, k) => html`<span style=${`width:${FROZEN[k]}px`} class=${k === 0 ? 'c' : ''}>${k === 0 ? '#' : tr(h).toUpperCase()}</span>`)}</div>
    ${t.adding && html`<${AddForm} v=${v} top=${hy + 34} />`}
    ${!rows.length && html`<p class="tp-none" style=${`top:${y0 + 8}px`}>${tr('No trips on this day: add them by headway with the clock above, or one at a time with +.')}</p>`}
    <div class="tp-grid" style=${`top:${y0}px;height:${gridH}px`}>
      ${rows.slice(scroll, scroll + visible + 1).map((row, kk) => {
        const k = kk + scroll;
        const y = kk * TT_ROW;
        const chosenRow = chosen.includes(row.i);
        return html`<div class=${'tp-row' + (chosenRow ? ' on' : '')} style=${`top:${y}px`}>
          <button type="button" class="num" style=${`width:${FROZEN[0]}px`} title=${tr('Click to choose the trip: Shift+click a range, Ctrl+click one more, Ctrl+A all')} onMouseDown=${(e) => e.shiftKey && e.preventDefault()} onClick=${(e) => pickRow(e, rows, row)}>${k + 1}</button>
          <button type="button" class="pat" style=${`width:${FROZEN[1]}px`} onMouseDown=${(e) => e.shiftKey && e.preventDefault()} onClick=${(e) => pickRow(e, rows, row, true)}>${row.code}</button>
          <span class="vt" style=${`width:${FROZEN[2]}px`}>${tt.vt}</span>
          <button type="button" class=${'tp-cell rel' + (isCell({ kind: 'start', i: row.i }) ? ' on' : '')} style=${`width:${FROZEN[3]}px`} onClick=${() => choose({ kind: 'start', i: row.i }, row.i)}>${isCell({ kind: 'start', i: row.i }) && t.text ? html`<${Typed} text=${t.text} />` : row.start}</button>
          ${k > 0
            ? html`<button type="button" class=${'tp-cell rel' + (isCell({ kind: 'headway', i: row.i }) ? ' on' : '')} style=${`width:${FROZEN[4]}px`} onClick=${() => choose({ kind: 'headway', i: row.i }, row.i)}>${isCell({ kind: 'headway', i: row.i }) && t.text ? html`<${Typed} text=${t.text} />` : row.headway}</button>`
            : html`<span class="tp-cell rel dash" style=${`width:${FROZEN[4]}px`}>-</span>`}
          <span class="times" style=${`left:${room.x - 2}px;width:${room.w + 4}px`}>
            ${row.cells.map((c, jj) => {
              const x0 = cols.cx(jj) - cols.w / 2 - (room.x - 2);
              if (x0 + cols.w < 0 || x0 > room.w + 4) return null;
              if (!c) return html`<span class="tp-cell dash" style=${`left:${x0}px;width:${cols.w}px`}>—</span>`;
              const on = isCell({ kind: 'time', i: row.i, s: c.s });
              return html`<button type="button" class=${'tp-cell' + (on ? ' on' : '') + (c.off ? ' off' : '') + (c.late ? ' late' : '')} style=${`left:${x0}px;width:${cols.w}px`} title=${c.base || undefined} onClick=${() => choose({ kind: 'time', i: row.i, s: c.s }, row.i)}>${on && t.text ? html`<${Typed} text=${t.text} />` : c.off ? `${c.off > 0 ? '+' : '−'} ${c.t}` : c.t}</button>`;
            })}
          </span>
        </div>`;
      })}
    </div>
    ${rows.length > visible && html`<div class="tp-vbar" style=${`top:${y0}px;height:${gridH}px`}><i style=${`top:${(scroll / Math.max(1, maxScroll)) * Math.max(0, gridH - Math.max(28, (gridH * visible) / rows.length))}px;height:${Math.max(28, (gridH * visible) / rows.length)}px`}></i></div>`}
    ${wide && html`<${HBar} x=${room.x} y=${y0 + gridH + 2} w=${room.w} pos=${cols.scroll} view=${room.w} total=${cols.w * ncol} onPos=${(x) => tpSet({ hscroll: x })} />`}
  </div>`;
}

/** The form that adds trips by headway (Optibus's "Add trips"): CONTROL POINT, START TIME, END
 * TIME, HEADWAY, and ✓ to make them. */
function AddForm({ v, top }) {
  const a = S.rf.tp.adding;
  const put = (patch) => tpSet({ adding: { ...a, ...patch } });
  const pattern = Math.min(a.pattern, Math.max(0, v.patterns.length - 1));
  const stops = v.patterns[pattern]?.stops || [];
  const first = parseTime(a.first);
  // (an end before the start, "22:00" until "00:30": after midnight, `timeplan::end_after`)
  const typed = parseTime(a.last);
  const last = typed != null && first != null && typed < first && typed + 1440 <= 2160 ? typed + 1440 : typed;
  const heads = parseHeadways(a.headway);
  const ok = first != null && last != null && heads && last >= first;
  const note = ok ? tr('%{n} trips', { n: generated(first, last, heads) }) : tr('Type the times as 06:00 and the headway in minutes');
  const field = (label, w, body) => html`<label style=${`width:${w}px`}><span>${tr(label).toUpperCase()}</span>${body}</label>`;
  return html`<div class="tp-form" style=${`top:${top}px;height:${FORM_H}px`} data-rf="rf-tp-form">
    ${v.patterns.length > 1 && field('Pattern', 80, html`<span class="tp-select"><select value=${String(pattern)} onChange=${(e) => put({ pattern: Number(e.currentTarget.value), control: 0 })}>${v.patterns.map((p, k) => html`<option value=${String(k)}>${p.code}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>`)}
    ${field('Control point', 220, html`<span class="tp-select"><select value=${String(Math.min(a.control, stops.length - 1))} onChange=${(e) => put({ control: Number(e.currentTarget.value) })}>${stops.map((s, k) => html`<option value=${String(k)}>${s}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>`)}
    ${field('Start time', 90, html`<input value=${a.first} placeholder="06:00" onInput=${(e) => put({ first: e.currentTarget.value })} />`)}
    ${field('End time', 90, html`<input value=${a.last} placeholder="22:00" onInput=${(e) => put({ last: e.currentTarget.value })} />`)}
    ${field('Headway', 84, html`<input value=${a.headway} placeholder="10,20" title=${tr('Minutes between the trips: several, such as 10,20, are taken in turn')} onInput=${(e) => put({ headway: e.currentTarget.value })} />`)}
    <small class="min">${tr('min')}</small>
    <span class=${'note' + (ok ? '' : ' warn')}>${note}</span>
    <button type="button" class="tp-x" onClick=${() => tpSet({ adding: null })}><${Icon} name="close" size=${18} /></button>
    <button type="button" class="tp-ok" data-rf="rf-tp-add-ok" title=${tr('Make the trips')} onClick=${() => ok && tpAct('add_many', { pattern, control: a.control, first: a.first, last: a.last, headway: a.headway }).then((x) => x?.added != null && tpSet({ adding: null }))}><${Icon} name="check" size=${18} /></button>
  </div>`;
}

// --- the figures ----------------------------------------------------------------------------------------

/** A small bar chart (the figures' tiles). */
function MiniBars({ values }) {
  const most = Math.max(1, ...values);
  return html`<span class="tp-mini">${values.map((x) => html`<i style=${`height:${(x / most) * 100}%`}></i>`)}</span>`;
}

function Kpis({ v }) {
  const k = v.tt?.kpis;
  if (!k) return null;
  return html`<div class="tp-kpis" style=${`top:${BAR_H + 6}px;height:${KPI_H}px`}>
    ${[0, 1, 2].map((n) => html`<button type="button" class="tp-kpi" title=${tr('Click to open the chart')} onClick=${() => tpSet({ chart: n })}>
      ${n === 0 ? html`<span class="count"><${Icon} name="location_on" size=${20} /><${Icon} name="directions_bus" size=${20} /><b>${k.count}</b></span>` : html`<${MiniBars} values=${n === 1 ? k.by_pattern : k.by_hour} />`}
      <span class="cap">${tr(CHARTS[n]).toUpperCase()}</span>
    </button>`)}
  </div>`;
}

/** A figure as a larger chart over the screen: a blue title, bars with their count and share on
 * top, ‹ › to the other charts. */
function ChartCard({ v }) {
  const t = S.rf.tp;
  const k = v.tt.kpis;
  const n0 = t.chart;
  const total = Math.max(1, k.count);
  const [values, labels] = n0 === 0 ? [[k.count], [k.label]] : n0 === 1 ? [k.by_pattern, k.codes] : [k.by_hour, k.by_hour.map((_, h) => String(h).padStart(2, '0'))];
  const most = Math.max(1, ...values);
  const many = values.length > 12;
  return html`<div class="tp-dim" onMouseDown=${(e) => e.target === e.currentTarget && tpSet({ chart: null })}>
    <div class="tp-chart">
      <b class="title">${tr(CHARTS[n0]).toUpperCase()}</b>
      <span class="nav"><button type="button" title=${tr('The chart before')} onClick=${() => tpSet({ chart: (n0 + 2) % 3 })}><${Icon} name="chevron_left" size=${14} /></button>
        <button type="button" title=${tr('The next chart')} onClick=${() => tpSet({ chart: (n0 + 1) % 3 })}><${Icon} name="chevron_right" size=${14} /></button>
        <button type="button" title=${tr('Close')} onClick=${() => tpSet({ chart: null })}><${Icon} name="close" size=${14} /></button></span>
      <div class=${'bars' + (many ? ' many' : '')}>${values.map((x, i) => html`<span><em>${x > 0 ? (many ? x : `${x} (${Math.round((x * 100) / total)}%)`) : ''}</em><b style=${`height:${(x / most) * 100}%`}></b><small>${!many || i % 2 === 0 ? labels[i] : ''}</small></span>`)}</div>
    </div>
  </div>`;
}

// --- the status bar and the issues ------------------------------------------------------------------------

function StatusBar({ v }) {
  const t = S.rf.tp;
  const tone = v.errors > 0 ? 'err' : v.warnings > 0 ? 'warn' : 'dim';
  let note;
  if (t.leaving) note = [tr('Changes not saved: press again to leave without them.'), 'warn'];
  else if (v.pending) note = [v.pending, 'dim'];
  else if (t.cell) note = [tr('Type and press Enter (Tab: the next cell, Escape: leave it). Up and Down change it by a minute.'), 'dim'];
  else if (t.tab === TIMETABLE && chosenTrips(t).length > 1) note = [tr('%{n} trips chosen: Shift+click a range, Ctrl+click one more or one less, Ctrl+A all, Escape none.', { n: chosenTrips(t).length }), 'sky'];
  else if (v.hint) note = [v.hint, 'sky'];
  else if (v.dirty) note = [tr('Changes not saved yet.'), 'dim'];
  else note = [tr('Click a cell to type into it.'), 'dim'];
  return html`<div class="tp-status">
    <span class=${'note ' + note[1]}>${note[1] === 'sky' && html`<${Icon} name="info" size=${16} />`}<span class="cut">${note[0]}</span></span>
    ${v.unsynced && html`<button type="button" class="tp-unsynced" data-rf="rf-tp-refresh" title=${tr('The timetable changed since the scheduling studio made the blocks of this line: make its blocks and duties anew')} onClick=${() => tpAct('refresh')}><${Icon} name="refresh" size=${16} />${tr('UNSYNCED')} – ${tr('Refresh')}</button>`}
    <button type="button" class=${'tp-issues ' + tone + (t.issuesOpen ? ' on' : '')} title=${tr('The issues of this day group')} onClick=${() => tpSet({ issuesOpen: !t.issuesOpen })}><${Icon} name=${v.errors > 0 ? 'error' : v.warnings > 0 ? 'warning' : 'check_circle'} size=${16} />${v.issues_label}</button>
  </div>`;
}

/** The issues of the day group in a small card over the status bar. */
function IssuesPanel({ v }) {
  const h = 52 + Math.max(1, v.issues.length) * 34;
  return html`<div class="tp-issuecard" style=${`height:${h}px`}>
    <div class="head"><b>${v.issues_head}</b><button type="button" title=${tr('Close')} onClick=${() => tpSet({ issuesOpen: false })}><${Icon} name="close" size=${13} /></button></div>
    ${!v.issues.length && html`<p>${tr('No issues: the timeplan of this day group is valid.')}</p>`}
    ${v.issues.map((i) => html`<button type="button" class="row" onClick=${() => {
      if (i.trip != null) tpSet({ side: i.side ?? 0, tab: TIMETABLE, ...only(i.trip), issuesOpen: false });
      else if (i.running) tpSet({ tab: RUNNING, issuesOpen: false });
      else tpSet({ issuesOpen: false });
    }}><span class=${i.error ? 'err' : 'warn'}><${Icon} name=${i.error ? 'error' : 'warning'} size=${17} /></span><span class="badge">${i.n}</span><span class="cut">${i.text}</span></button>`)}
  </div>`;
}

// --- typing ---------------------------------------------------------------------------------------------

/** The cell Tab goes on to (`timeplan::next_cell`): the next segment, the next stop, the headway
 * after the start. */
function nextCell(v, c) {
  if (c.kind === 'run') {
    const segs = Math.max(0, (v.rt?.n || 0) - 1);
    const bands = v.rt?.bands.length || 0;
    if (c.seg + 1 < segs) return { ...c, seg: c.seg + 1 };
    if (c.band + 1 < bands) return { kind: 'run', band: c.band + 1, seg: 0 };
    return null;
  }
  // (the first trip has no headway to type)
  if (c.kind === 'start') return v.tt?.rows[0]?.i === c.i ? null : { kind: 'headway', i: c.i };
  if (c.kind === 'time') {
    // (the next stop shown: with timepoints only, not one left out)
    const row = v.tt?.rows.find((r) => r.i === c.i);
    const next = row?.cells.find((x) => x && x.s > c.s);
    return next ? { ...c, s: next.s } : null;
  }
  return null;
}

/** The keys (`timeplan::keys`): typed into the chosen cell (Enter takes it, Tab takes it and goes
 * on, Up and Down change it by a minute); Delete takes the chosen trip out. */
function useTpKeys(v) {
  const ref = useRef(v);
  ref.current = v;
  useEffect(() => {
    const k = (e) => {
      const r = S.rf;
      if (!r.open || r.view !== 3 || r.prefs || S.co.popup || S.co.dialog) return;
      const el = document.activeElement;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      const t = r.tp;
      const vv = ref.current;
      if (t.cell) {
        if (/^[0-9:.,-]$/.test(e.key)) {
          e.preventDefault();
          tpSet({ text: t.text + e.key });
        } else if (e.key === 'Backspace') {
          e.preventDefault();
          tpSet({ text: t.text.slice(0, -1) });
        } else if (e.key === 'Enter' || e.key === 'Tab') {
          e.preventDefault();
          const cell = t.cell;
          const text = t.text;
          const next = e.key === 'Tab' && vv ? nextCell(vv, cell) : null;
          tpSet({ text: '', cell: next });
          if (text) tpAct('apply', { cell, text });
        } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
          e.preventDefault();
          tpSet({ text: '' });
          tpAct('nudge', { cell: t.cell, by: e.key === 'ArrowUp' ? 1 : -1 });
        }
        return;
      }
      if (t.tab !== TIMETABLE || t.copying) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a' && vv?.tt) {
        e.preventDefault();
        const all = vv.tt.rows.map((x) => x.i);
        tpSet({ picked: all, sel: all[0] ?? null, anchor: all[0] ?? null });
        return;
      }
      const chosen = chosenTrips(t);
      if (e.key === 'Delete' && chosen.length) deleteChosen(chosen);
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
}

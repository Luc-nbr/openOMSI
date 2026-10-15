// RosterFlow's "Colour by" (testers, Beta 56: "change the colour based on the Trip ID instead of
// just the route as some maps do it this way, this can be a choice"): the trips' chips in the
// Vehicles and Drivers tabs coloured by their line (as the lines have it), their trip of the
// timetable (its name: each trip pattern a colour of its own), their bus's block or their duty;
// one value always the same colour, from a palette stepped for the light look and the dark; and
// the small legend of what the rows shown have. The choice is the company's (`rf_act`
// "colour_by", the board's `head.colour_by`).

import { html } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { hex, css } from './pal.js';
import { setRf, rfAct } from './state.js';
import { Icon } from './ui.js';
import { rowsOf, dutyChips } from './gantt.js';

// (the look of the legend, the copy picker and a duty's points, beside rosterflow.css)
if (!document.querySelector('link[data-rf-gantt-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/rosterflow-gantt.css';
  sheet.dataset.rfGanttCss = '1';
  document.head.append(sheet);
}

/** The choices, as kept, and their words. */
export const BY = ['line', 'trip', 'block', 'duty'];
export const BY_LABEL = ['Line (route)', 'Trip ID', 'Block (vehicle)', 'Duty'];

// (the categorical hues in their order, each stepped for its look, and a second step of each
// for the ninth value on: a value keeps its colour wherever it stands - its chips carry their
// words too, so no value is told by its colour alone)
const LIGHT = ['#2a78d6', '#eb6834', '#1baf7a', '#eda100', '#e87ba4', '#008300', '#4a3aa7', '#e34948', '#1c5cab', '#b84a1c', '#11805a', '#a87400', '#b8456f', '#4caf3e', '#7a6fd6', '#b33232'];
const DARK = ['#3987e5', '#d95926', '#199e70', '#c98500', '#d55181', '#008300', '#9085e9', '#e66767', '#6da7ec', '#f08a5d', '#3ccb98', '#f0b733', '#ec9bbb', '#3fa83f', '#b3abf2', '#ef8f8f'];

/** FNV-1a: the same number for the same words, every time. */
function fnv(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** The colour of a value (its key) in the look `p`. */
export const colourOf = (p, key) => hex((p.light ? LIGHT : DARK)[fnv(key) % LIGHT.length]);

/** How the trips are coloured now: the company's choice. */
export function colourBy(data) {
  const by = data?.head?.colour_by;
  return BY.includes(by) ? by : 'line';
}

/** The piece of block `t` trip `j` is in. */
const pieceIn = (t, j) => Math.max(0, (t?.pieces || []).findIndex(([a, z]) => j >= a && j < z));

/** What trip `j` of block `i` is by `by`: its value's key and words (null: none, the line's). */
export function keyOf(b, by, i, j, dutyRow = null) {
  const t = b.blocks[i];
  const x = t?.trips[j];
  if (!t || !x) return null;
  switch (by) {
    case 'trip':
      return [`T|${t.line}|${x.name}`, x.name];
    case 'block':
      return [t.id, t.label];
    case 'duty': {
      const s = b.duties[dutyRow ?? t.duty_rows?.[pieceIn(t, j)]];
      return s ? [s.id, s.label] : null;
    }
    default:
      return [`L|${t.line}`, tr('Line %{n}', { n: t.number })];
  }
}

/** The colour of trip `j` of block `i` as the Layers ask (`p` the look). */
export function tripColour(p, b, by, i, j, dutyRow = null) {
  const t = b.blocks[i];
  if (by === 'line' || !t) return hex(t?.colour);
  const k = keyOf(b, by, i, j, dutyRow);
  return k ? colourOf(p, k[0]) : hex(t.colour);
}

/** A trip on the stack (no bus, no duty): by its trip ID, else its line's. */
export function stackColour(p, by, x) {
  return by === 'trip' && x.name ? colourOf(p, `T|${x.line}|${x.name}`) : hex(x.colour);
}

/** Choose how the trips are coloured (kept for the company). */
export function setColourBy(by) {
  const d = S.rf.data;
  if (d?.head) setRf({ data: { ...d, head: { ...d.head, colour_by: by } } });
  rfAct('colour_by', { by });
}

/** Whether the legend shows: as the Layers say, else for every choice but the line's. */
export const legendOn = (data) => S.rf.layers.legend ?? colourBy(data) !== 'line';

/** The values of the rows shown with their colour and how many trips each has, the most first. */
function values(data, drivers, by, p) {
  const b = data.board;
  const seen = new Map();
  const add = (i, j, d) => {
    const k = keyOf(b, by, i, j, d);
    if (!k) return;
    const v = seen.get(k[0]);
    if (v) v.n++;
    else seen.set(k[0], { key: k[0], label: k[1], n: 1, colour: by === 'line' ? hex(b.blocks[i].colour) : colourOf(p, k[0]) });
  };
  for (const row of rowsOf(data, drivers)) {
    if (row.kind === 'block') b.blocks[row.i]?.trips.forEach((x, j) => x.counts && add(row.i, j, null));
    else for (const [i, j] of dutyChips(b, row.i)) add(i, j, row.i);
  }
  return [...seen.values()].sort((a, z) => z.n - a.n || String(a.label).localeCompare(String(z.label), undefined, { numeric: true }));
}

/** The legend over the Gantt's lower right: each value's colour and words (the first ones, and
 * how many more), folded away with its cross (the Layers bring it back). */
export function Legend({ data, drivers, p }) {
  if (!legendOn(data)) return null;
  const by = colourBy(data);
  const all = values(data, drivers, by, p);
  if (!all.length) return null;
  const shown = all.slice(0, 12);
  return html`<div class="rf-over rf-legend" data-rf="rf-legend">
    <div class="head"><b>${tr('Colour by')}: ${tr(BY_LABEL[BY.indexOf(by)])}</b><button type="button" title=${tr('Hide the legend')} onClick=${() => setRf({ layers: { ...S.rf.layers, legend: false } })}><${Icon} name="close" size=${14} /></button></div>
    <div class="items">${shown.map((v) => html`<span class="item" title=${`${v.label} (${v.n})`}><i style=${`background:${css(v.colour)}`}></i><span class="cut">${v.label}</span></span>`)}</div>
    ${all.length > shown.length && html`<span class="more">${tr('%{n} more', { n: all.length - shown.length })}</span>`}
  </div>`;
}

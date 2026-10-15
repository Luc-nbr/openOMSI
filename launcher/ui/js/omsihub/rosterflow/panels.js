// What lies over RosterFlow's Gantt (`crates/omsi-launcher-ui/src/company/planning/rf_panels.rs`):
// the cards a click opens (a trip, a vehicle, an event, the stack's trip, a choice of several, a
// drop zone's ranking and cost difference), the right-click menu, the batch toolbar of the ticked
// rows, the Validation panel (open and dismissed issues, Filter Issues, Dismiss Issues), the duty
// panel (its type, lock, times, paid time, signing, pieces, travel, breaks and custom work), and
// the small windows of the toolbar, the title bar and the status bar (history, Stats, Layers,
// Filter, zoom, print, the day, a line's colour, go to, the optimisation settings, a histogram);
// and a duty's break or driving period clicked in the Drivers Gantt, with the duty's others.
// Everything in the studio's own units (its top left 0, 0), as `rf_panels` lays it out.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { call } from '../../api.js';
import { tr } from '../bits.js';
import { hex as hexOf } from './pal.js';
import { setRf, rfCmd, rfAct, after, edit, pick, settle, toggle, clock, hm, say, NO_FILTER, VEHICLES, DRIVERS, TIMEPLAN } from './state.js';
import { STATS, STAT_LABEL, handSpan, pieceOf, pieceTrips, tripHand } from './gantt.js';
import { Icon, RfButton, IconBtn, Caps, Switch, CheckRow, Seg, Stepper, MenuItem, Select } from './ui.js';
import { AXIS } from './draw.js';
import { DayItem, dayMenuHeight } from './days.js';
import { BY, BY_LABEL, colourBy, setColourBy, legendOn, Legend } from './colourby.js';

const eq = (a, b) => String(a).trim().toLowerCase() === String(b).trim().toLowerCase();

/** A card placed by `at` (under it and to the right, kept in the studio: `rf_panels::place`). */
function place(at, w, h, size) {
  const x = Math.max(6, Math.min(Math.max(6, size.w - w - 6), at.x - 30));
  const y = at.y + 16 + h < size.h ? at.y + 16 : Math.max(6, at.y - h - 16);
  return `left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
}

/** "07:05 ── 45 min, 12.3 km ──> 07:50", the times in grey pills (`rf_panels::time_line`). */
const TimeLine = ({ from, to, mid }) => html`<div class="rf-timeline"><b>${clock(from)}</b><span><i></i><em>${mid}</em></span><b>${clock(to)}</b></div>`;

/** A card's field: its caption in grey capitals, its value after it. */
const Field = ({ cap, value, style = '' }) => html`<div class="rf-field" style=${style}><span>${tr(cap).toUpperCase()}:</span><b class="cut">${value}</b></div>`;

/** A round icon button on a card. */
const CardIcon = ({ icon, title, on, disabled, onClick }) => html`<button type="button" class=${'rf-cicon' + (on ? ' on' : '')} disabled=${disabled} title=${tr(title)} onClick=${onClick}><${Icon} name=${icon} size=${17} /></button>`;

/** The duty a piece is in: its row in the board and its label. */
function dutyOfPiece(b, tour, duty) {
  const row = b.blocks[tour]?.duty_rows?.[duty];
  return row == null ? null : [row, b.duties[row]?.label];
}

/** The day's view of the Gantt's tab (Vehicles or Drivers), a duty's panel open on it. */
function toDuty(b, row) {
  const id = b.duties[row]?.id;
  if (id) setRf({ view: DRIVERS, panel: { kind: 'duty', id }, reveal: id, selRow: id, card: null });
}

/** The small windows a press elsewhere closes (`rf_panels`: "a click elsewhere closes it"). */
const SMALL = ['history', 'print', 'day', 'goto', 'zoom', 'stats', 'layers', 'optimise'];

/** A press outside the card, the menu or a small window closes it; not one on the button that
 * opens it (its own click toggles it), nor on the Gantt (it looks for itself). */
export function useOutsideClose() {
  useEffect(() => {
    const down = (e) => {
      const r = S.rf;
      if (!r.open || e.button !== 0) return;
      const t = e.target;
      if (!t?.closest || !t.closest('.rf') || t.closest('.rf-over') || t.closest('.rf-gantt') || t.closest('.rf-tour, .rf-guide, .rf-tutmenu, .rf-sheetdim')) return;
      const kind = r.panel?.kind || r.panel;
      const toggles = t.closest('.rf-title button, .rf-toolbar button, .rf-status button');
      const patch = {};
      if (r.card) patch.card = null;
      if (r.menu) patch.menu = null;
      if (SMALL.includes(kind) && !toggles) patch.panel = null;
      if (Object.keys(patch).length) setRf(patch);
    };
    addEventListener('mousedown', down, true);
    return () => removeEventListener('mousedown', down, true);
  }, []);
}

export function Overlays({ data, drivers, p, size, gTop }) {
  const r = S.rf;
  const b = data.board;
  const panel = r.panel;
  const kind = panel?.kind || panel;
  return html`<div class="rf-layer">
    ${r.ticked.length > 0 && html`<${Batch} b=${b} drivers=${drivers} gTop=${gTop} />`}
    <${Legend} data=${data} drivers=${drivers} p=${p} />
    ${kind === 'issues' && html`<${Issues} data=${data} drivers=${drivers} scope=${panel.scope} size=${size} />`}
    ${kind === 'duty' && html`<${DutyPanel} data=${data} id=${panel.id} gTop=${gTop} />`}
    ${kind === 'stats' && html`<${StatsWindow} drivers=${drivers} gTop=${gTop} />`}
    ${kind === 'layers' && html`<${LayersWindow} data=${data} gTop=${gTop} />`}
    ${kind === 'filter' && html`<${FilterWindow} data=${data} size=${size} />`}
    ${kind === 'zoom' && html`<${ZoomMenu} />`}
    ${kind === 'histogram' && html`<${HistogramWindow} k=${data.kpis} which=${panel.which} size=${size} />`}
    <${TitleWindows} data=${data} drivers=${drivers} size=${size} />
    ${r.zoneHover != null && r.zones?.[r.zoneHover] && r.zoneAt && html`<${ZoneCard} b=${b} z=${r.zones[r.zoneHover]} at=${r.zoneAt} size=${size} />`}
    ${r.card && html`<${Card} data=${data} card=${r.card} drivers=${drivers} size=${size} />`}
    ${r.menu && html`<${Menu} data=${data} m=${r.menu} drivers=${drivers} size=${size} />`}
  </div>`;
}

/** The windows of the title bar (the history, print, the day, go to, a line's colour, the
 * optimisation settings), over whichever tab is shown. */
export function TitleWindows({ data, drivers, size }) {
  const r = S.rf;
  const panel = r.panel;
  const kind = panel?.kind || panel;
  return html`${kind === 'history' && html`<${History} data=${data} size=${size} />`}
    ${kind === 'print' && html`<${PrintMenu} />`}
    ${kind === 'day' && html`<${DayMenu} size=${size} />`}
    ${kind === 'colour' && html`<${ColourWindow} data=${data} line=${panel.line} size=${size} />`}
    ${kind === 'goto' && html`<${GoTo} data=${data} drivers=${drivers} text=${panel.text} size=${size} />`}
    ${kind === 'optimise' && html`<${OptSettings} data=${data} size=${size} />`}`;
}

// --- the batch toolbar -------------------------------------------------------------------------------

/** The ticked rows' tools over the Gantt's header: lock, unlock, unassign to the stack. */
function Batch({ drivers, gTop }) {
  const r = S.rf;
  const ids = r.ticked;
  return html`<div class="rf-over rf-batch" style=${`top:${gTop + 2}px;height:${AXIS - 4}px`}>
    <${IconBtn} icon="lock" title="Lock the rows ticked" onClick=${() => edit('lock_rows', { ids, on: true }).then(() => setRf({ ticked: ids }))} />
    <${IconBtn} icon="livery_unlock" title="Unlock the rows ticked" onClick=${() => edit('lock_rows', { ids, on: false }).then(() => setRf({ ticked: ids }))} />
    <${IconBtn} icon="inventory_2" title="Unassign the ticked vehicles' trips to the stack" disabled=${drivers} onClick=${() => edit('unassign_rows', { ids }).then(() => setRf({ ticked: [] }))} />
    <b>${tr('%{n} rows selected', { n: ids.length })}</b>
    <button type="button" class="x" onClick=${() => setRf({ ticked: [] })}><${Icon} name="close" size=${15} /></button>
  </div>`;
}

// --- the cards ---------------------------------------------------------------------------------------

function Card({ data, card, drivers, size }) {
  switch (card.kind) {
    case 'trip':
      return html`<${TripCard} data=${data} c=${card} drivers=${drivers} size=${size} />`;
    case 'vehicle':
      return html`<${VehicleCard} data=${data} c=${card} size=${size} />`;
    case 'event':
      return html`<${EventCard} data=${data} c=${card} size=${size} />`;
    case 'stack':
      return html`<${StackCard} data=${data} c=${card} size=${size} />`;
    case 'selection':
      return html`<${SelectionCard} data=${data} c=${card} drivers=${drivers} size=${size} />`;
    case 'rest':
      return html`<${RestCard} data=${data} c=${card} size=${size} />`;
    default:
      return null;
  }
}

function TripCard({ data, c, drivers, size }) {
  const b = data.board;
  const t = b.blocks[c.tour];
  const x = t?.trips[c.trip];
  const [cands, setCands] = useState(null);
  useEffect(() => {
    if (!c.assign) return;
    rfCmd('rf_view', { view: 'candidates', day: S.rf.day, tour: c.tour, trip: c.trip }).then(setCands).catch(() => setCands([]));
  }, [c.assign, c.tour, c.trip]);
  if (!x) return null;
  const list = c.assign ? cands || [] : [];
  // (a relief on the way: where the trip is cut, or the stops it can be cut at)
  const mids = x.part ? [] : (x.mids || []).filter((m) => m[3]);
  const reliefRow = !!x.part || mids.length > 0;
  const h = 300 + (c.assign ? 26 * (list.length + 1) + 10 : 0) + (reliefRow ? 36 : 0);
  const piece = pieceOf(b, c.tour, c.trip);
  const duty = dutyOfPiece(b, c.tour, piece);
  const cl = data.lines.find((l) => eq(l.name, t.line));
  const move = () => {
    pick(tripHand(b, drivers, c.tour, [c.trip], c.trip, duty?.[0] ?? null), false);
  };
  return html`<div class="rf-over rf-card" style=${place(c.at, 410, h, size)}>
    <div class="head">
      ${duty && html`<button type="button" class="who" onClick=${() => toDuty(b, duty[0])}><${Icon} name="person" size=${18} /><u>${duty[1]}</u></button>`}
      <span class="grow"></span>
      <button type="button" class="veh" title=${tr('Assign the trip to another vehicle')} onClick=${() => !drivers && setRf({ card: { ...c, assign: !c.assign } })}><${Icon} name="directions_bus" size=${18} /><span>${t.label}</span></button>
      <button type="button" class="pal" title=${tr("The line's colour")} onClick=${() => setRf({ panel: { kind: 'colour', line: t.line } })}><${Icon} name="palette" size=${18} /></button>
    </div>
    <div class="body">
      <div class="two"><${Field} cap="Route" value=${`${t.number}  ${cl?.caption || ''}`} /><${Field} cap="Dir" value=${x.out ? 'A' : 'B'} /></div>
      <div class="two"><${Field} cap="Trip ID" value=${x.name} /><${Field} cap="Service ID" value=${data.head.service} /></div>
      <${Field} cap="Timetable" value=${tr('Tour %{t}', { t: x.origin })} />
      <hr />
      <${Field} cap="From" value=${x.from} />
      <${Field} cap="To" value=${x.to} />
      <hr />
      <div class="tl"><span class="cap">${tr('Time').toUpperCase()}:</span><${TimeLine} from=${x.dep} to=${x.arr} mid=${`${x.arr - x.dep} min, ${x.km.toFixed(1)} km`} /></div>
      ${reliefRow && html`<div class="rf-mids"><span class="cap">${tr('Relief on the way').toUpperCase()}:</span>
        ${x.part
          ? html`<b class="at">${x.at}  ·  ${clock(x.part === 1 ? x.arr : x.dep)}</b><button type="button" class="mid" title=${tr('Join the two duties: one driver works the trip whole')} onClick=${() => edit('uncut', { tour: c.tour, at: x.part === 2 ? c.trip : c.trip + 1 })}><${Icon} name="link" size=${14} /><span>${tr('Join')}</span></button>`
          : mids.slice(0, 4).map(([stop, name, at, , tip]) => html`<button type="button" class="mid" title=${tip} onClick=${() => edit('cut_mid', { tour: c.tour, trip: c.trip, stop, name, time: at })}><${Icon} name="sync_alt" size=${14} /><b>${clock(at)}</b><span>${name}</span></button>`)}
      </div>`}
      ${c.assign && html`<hr /><div class="assign">
        ${list.map((a) => html`<button type="button" onClick=${() => edit('move', { tour: c.tour, trips: [c.trip], to: a.tour })}><${Icon} name="directions_bus" size=${15} /><b>${a.label}</b></button>`)}
        <button type="button" class="new" onClick=${() => edit('move', { tour: c.tour, trips: [c.trip], to: null })}><${Icon} name="add" size=${15} /><b>${tr('New vehicle')}</b></button>
      </div>`}
    </div>
    <div class="foot">
      <${CardIcon} icon="inventory_2" title="Unassign: the trip waits on the stack" disabled=${drivers} onClick=${() => !drivers && edit('unassign', { tour: c.tour, trips: [c.trip] })} />
      <${CardIcon} icon="link" title=${x.linked ? 'Unlink: the trips need no longer stay together' : 'Link it with the trips chosen (Shift and a click)'} on=${x.linked} onClick=${() => linkOrUnlink(c.tour, c.trip, x.linked)} />
      <span class="grow"></span>
      <${RfButton} label="Timeplan" style="width:110px;height:30px" onClick=${() => setRf({ view: TIMEPLAN, tpLine: t.line, hand: null, zones: null, sel: [], card: null, menu: null, panel: null })} />
      <${RfButton} label="Move" solid style="width:104px;height:30px" onClick=${move} />
    </div>
  </div>`;
}

/** Link the trip with the trips chosen, or take its link apart. */
function linkOrUnlink(tour, trip, linked) {
  if (linked) {
    edit('unlink', { tour, trip });
    return;
  }
  const trips = S.rf.sel.filter((s) => s[0] === tour).map((s) => s[1]);
  if (!trips.includes(trip)) trips.push(trip);
  edit('link', { tour, trips });
}

function VehicleCard({ data, c, size }) {
  const b = data.board;
  const t = b.blocks[c.tour];
  if (!t) return null;
  const first = t.trips[0];
  const last = t.trips[t.trips.length - 1];
  const nTrips = t.trips.filter((x) => x.counts).length;
  return html`<div class="rf-over rf-card" style=${place(c.at, 380, 236, size)}>
    <div class="head warn"><${Icon} name="directions_bus" size=${18} /><b>${t.label}  /  ${tr('Line %{n}', { n: t.number })}</b></div>
    <div class="body">
      <${Field} cap="From" value=${first?.from || ''} />
      <${Field} cap="To" value=${last?.to || ''} />
      <div class="tl wide"><${TimeLine} from=${t.from} to=${t.to} mid=${`${hm(t.to - t.from)}, ${t.km.toFixed(1)} km`} /></div>
      <${Field} cap="Trips" value=${`${nTrips}  ·  ${tr('duties')} ${t.pieces.length}`} />
    </div>
    <div class="foot">
      <${CardIcon} icon=${t.locked ? 'lock' : 'livery_unlock'} title=${t.locked ? 'Unlock the vehicle' : 'Lock the vehicle: Optimise leaves it'} on=${t.locked} onClick=${() => edit('lock_block', { tour: c.tour })} />
      <${CardIcon} icon="palette" title="The line's colour" onClick=${() => setRf({ panel: { kind: 'colour', line: t.line } })} />
      <span class="grow"></span>
      <${RfButton} label="View in crew schedule" style="width:180px;height:30px" onClick=${() => {
        const row = t.duty_rows[0];
        const id = row != null ? b.duties[row]?.id : null;
        setRf({ view: DRIVERS, hand: null, zones: null, sel: [], card: null, menu: null, reveal: id, selRow: id });
      }} />
    </div>
  </div>`;
}

const TRAVEL = ['Walk', 'Public transport', 'Staff car'];

function EventCard({ data, c, size }) {
  const s = data.board.duties[c.row];
  const e = s?.events[c.ev];
  if (!e) return null;
  const kind = e.kind === 'on' ? tr('Sign-on') : e.kind === 'off' ? tr('Sign-off') : e.kind === 'travel' ? `${tr('Travel')} (${e.label})` : e.label;
  return html`<div class="rf-over rf-card" style=${place(c.at, 330, 190, size)}>
    <div class="head"><${Icon} name="person" size=${17} /><b>${s.label}  ·  ${kind}</b></div>
    <div class="body">
      <${Field} cap="Type" value=${kind} />
      <div class="tl wide"><${TimeLine} from=${e.from} to=${e.to} mid=${`${e.to - e.from} min`} /></div>
      ${e.kind === 'travel' && html`<${Seg} at=${e.mode} labels=${TRAVEL} onPick=${(k) => edit('travel_event', { row: c.row, mode: k, from: e.from })} />`}
      ${e.kind === 'custom' && html`<${RfButton} label="Delete" style="width:120px;height:28px" onClick=${() => edit('custom_delete', { row: c.row, ci: e.ci })} />`}
    </div>
  </div>`;
}

/** A duty's break, driving period or missing meal break clicked in the Drivers Gantt: when, what it
 * keeps or breaks, and the duty's breaks and driving periods with it (`breaks.rs`). */
function RestCard({ data, c, size }) {
  const s = data.board.duties[c.row];
  const rest = s?.rest;
  if (!rest) return null;
  const it = c.what === 'break' ? rest.breaks[c.k] : c.what === 'period' ? rest.periods[c.k] : rest.meal;
  if (!it) return null;
  const head = c.what === 'break' ? it.label : c.what === 'period' ? `${tr('Driving period')} ${it.text}` : tr(it.late ? 'Meal break too late' : 'No meal break');
  const lines = String(it.tip || '').split('\n').slice(c.what === 'meal' ? 0 : 1);
  const bad = (c.what === 'period' && it.over) || c.what === 'meal';
  const tone = (hard) => (hard ? 'err' : 'warn');
  const h = Math.min(size.h - 12, 132 + lines.length * 17 + (rest.breaks.length + rest.periods.length) * 19 + 44);
  return html`<div class="rf-over rf-card rf-restcard" style=${place(c.at, 360, h, size)}>
    <div class=${'head' + (bad ? ' warn' : '')}><${Icon} name="person" size=${17} /><b class="cut">${s.label}  ·  ${head}</b></div>
    <div class="body">
      <div class="tl wide"><${TimeLine} from=${it.from} to=${it.to} mid=${hm(it.to - it.from)} /></div>
      ${lines.map((l) => html`<p class="why">${l}</p>`)}
      <hr />
      <${Caps} text="Breaks" />
      ${!rest.breaks.length && html`<p class="why">${tr('None')}</p>`}
      ${rest.breaks.map((b, k) => html`<div class=${'item' + (c.what === 'break' && c.k === k ? ' on' : '')} title=${b.tip}><i class=${b.kind === 'split' ? 'split' : b.counts ? 'ok' : 'short'}></i><span>${b.label}</span><em>${clock(b.from)}–${clock(b.to)}</em></div>`)}
      <${Caps} text="Driving periods" />
      ${rest.periods.map((x, k) => html`<div class=${'item' + (c.what === 'period' && c.k === k ? ' on' : '')} title=${x.tip}><i class=${x.over ? tone(rest.period_hard) : 'calm'}></i><span>${x.text}</span><em>${clock(x.from)}–${clock(x.to)}</em></div>`)}
    </div>
  </div>`;
}

function StackCard({ data, c, size }) {
  const x = data.board.stack[c.k];
  if (!x) return null;
  return html`<div class="rf-over rf-card" style=${place(c.at, 360, 170, size)}>
    <div class="head"><${Icon} name="inventory_2" size=${17} /><b>${tr('Unassigned trip')}</b></div>
    <div class="body">
      <${Field} cap="Route" value=${x.number} />
      <${Field} cap="From" value=${`${x.from} → ${x.to}`} />
      <div class="tl wide"><${TimeLine} from=${x.dep} to=${x.arr} mid=${`${x.arr - x.dep} min`} /></div>
      <p class="hint">${tr('Click a drop zone to assign it to a vehicle')}</p>
    </div>
  </div>`;
}

function SelectionCard({ data, c, drivers, size }) {
  const b = data.board;
  const sel = S.rf.sel;
  if (!sel.length) return null;
  const tour = sel[0][0];
  const t = b.blocks[tour];
  const xs = sel.map(([a, z]) => b.blocks[a]?.trips[z]).filter(Boolean);
  if (!t || !xs.length) return null;
  const from = Math.min(...xs.map((x) => x.dep));
  const to = Math.max(...xs.map((x) => x.arr));
  const driving = xs.reduce((s, x) => s + x.arr - x.dep, 0);
  const trips = sel.filter((s) => s[0] === tour).map((s) => s[1]);
  // (in Drivers the choice is trips of one duty, on one bus or several)
  const duty = drivers && S.rf.tripsToDuties ? dutyOfPiece(b, tour, pieceOf(b, tour, sel[0][1])) : null;
  return html`<div class="rf-over rf-card" style=${place(c.at, 380, 196, size)}>
    <div class="head"><${Icon} name=${duty ? 'person' : 'directions_bus'} size=${17} /><b>${duty ? duty[1] : t.label}  ·  ${tr('%{n} trips chosen', { n: xs.length })}</b></div>
    <div class="body">
      <${Field} cap="Driving time" value=${hm(driving)} />
      <div class="tl wide"><${TimeLine} from=${from} to=${to} mid=${`${tr('span')} ${hm(to - from)}`} /></div>
    </div>
    <div class="foot">
      <${RfButton} label="Link" disabled=${drivers || xs.length < 2} style="width:100px;height:30px" onClick=${() => edit('link', { tour, trips })} />
      <span class="grow"></span>
      <${RfButton} label=${`${tr('Move')} ${xs.length}`} raw solid disabled=${drivers && !S.rf.tripsToDuties} style="width:124px;height:30px" onClick=${() => pick(tripHand(b, drivers, tour, trips, trips[0], duty?.[0] ?? null, sel), false)} />
    </div>
  </div>`;
}

/** The ranking of the drop zone under the mouse (Optibus's recommendation tooltip). */
function ZoneCard({ b, z, at, size }) {
  const d = z.drop;
  const target = d.kind === 'block' ? b.blocks[d.i]?.label : d.kind === 'duty' ? b.duties[d.d]?.label : tr(d.kind === 'new' ? 'New vehicle' : d.kind === 'new_duty' ? 'New duty' : 'Stack');
  return html`<div class="rf-card rf-zonecard" style=${place(at, 330, 150, size)}>
    <div class="head"><${Icon} name=${d.kind === 'duty' || d.kind === 'new_duty' ? 'person' : 'directions_bus'} size=${17} /><b>${target}</b></div>
    <div class="boxes">
      <div><span>${tr('Ranking')}</span><span class="stars">${[0, 1, 2, 3, 4].map((s) => html`<${Icon} name="star" size=${18} class=${s < z.stars ? 'on' : ''} />`)}</span></div>
      <div><span>${tr('Cost difference')}</span><b class=${z.delta > 0 ? 'bad' : 'good'}>${z.delta_text}</b></div>
    </div>
    <p class=${z.clean ? '' : 'err'}>${tr(z.clean ? 'Breaks no rule: click to drop it here' : 'Breaks a rule: it can still be dropped here')}</p>
  </div>`;
}

// --- the right-click menu ---------------------------------------------------------------------------

function Menu({ data, m, drivers, size }) {
  const b = data.board;
  let items = [];
  if (m.custom) {
    items = [['Edit', 20, true, ''], ['Delete', 21, true, '']];
  } else if (drivers) {
    const s = m.dutyRow != null ? b.duties[m.dutyRow] : null;
    const pieceLocked = !!s?.locked;
    // (single trips taken up once the core moves them between duties - the bus the same - and
    // the whole piece of work by the menu's Move piece, or Alt and a drag)
    items = S.rf.tripsToDuties
      ? [
          ['Move', 1, !pieceLocked, 'Move the chosen trips to another duty: the bus stays the same'],
          ['Move to new row', 26, !pieceLocked, 'The chosen trips a duty of their own'],
          ['Move piece', 24, !pieceLocked && !!m.piece, 'Take up the whole piece of work on this bus (or Alt and a drag)'],
          ['Move piece to new row', 2, !pieceLocked && (s?.spans.length || 0) > 1, 'The piece a duty of its own'],
        ]
      : [
          ['Move', 1, !pieceLocked, ''],
          ['Move to new row', 2, !pieceLocked && (s?.spans.length || 0) > 1, 'The piece a duty of its own'],
        ];
    items.push(
      ['Lock duty', 9, true, ''],
      ['Add custom work', 22, true, 'Spare, office, union or training time in the duty'],
      ['—'],
      ['View in vehicle schedule', 23, true, ''],
      ['View in timeplan', 7, true, ''],
      ['Delete', 8, false, 'Trips are deleted in the timetable'],
    );
  } else {
    const t = b.blocks[m.tour];
    const locked = !!t?.locked;
    const linked = !!t?.trips[m.trip]?.linked;
    items = [
      ['Move', 1, !locked, ''],
      ['Move to new row', 2, !locked, ''],
      ['Unassign', 3, !locked, 'The trip waits on the stack'],
      ['Swap', 4, !locked, 'Swap the trips chosen with what another bus runs over their time: click the bus next'],
      ['Swap from here', 5, !locked, "Swap everything from this trip on with another bus's day from then on: click the bus next"],
      [linked ? 'Unlink' : 'Link', 6, !locked, ''],
      [locked ? 'Unlock vehicle' : 'Lock vehicle', 9, true, ''],
      ['Line colour…', 10, true, ''],
      ['—'],
      ['View in timeplan', 7, true, ''],
      ['Delete', 8, false, 'Trips are deleted in the timetable'],
    ];
  }
  const h = items.reduce((s, i) => s + (i[0] === '—' ? 9 : 30), 0) + 10;
  const x = Math.min(m.at.x, size.w - 230);
  const y = m.at.y + h < size.h ? m.at.y : m.at.y - h;
  const trips = (() => {
    const mine = S.rf.sel.filter((s) => s[0] === m.tour).map((s) => s[1]);
    return mine.length ? mine : [m.trip];
  })();
  // (Drivers: the trips chosen of the duty clicked)
  const dutyTrips = (() => {
    const mine = S.rf.sel.filter(([i, j]) => dutyOfPiece(b, i, pieceOf(b, i, j))?.[0] === m.dutyRow);
    return mine.length ? mine : [[m.tour, m.trip]];
  })();
  const go = (what) => {
    setRf({ menu: null });
    switch (what) {
      case 1:
        if (drivers && S.rf.tripsToDuties && m.tour != null && m.dutyRow != null) pick({ kind: 'duty_trips', duty: m.dutyRow, trips: dutyTrips }, false);
        else pick(m.piece ? { kind: 'piece', tour: m.piece[0], duty: m.piece[1] } : { kind: 'trips', tour: m.tour, trips }, false);
        break;
      case 24:
        if (m.piece) pick({ kind: 'piece', tour: m.piece[0], duty: m.piece[1] }, false);
        break;
      case 26:
        edit('drop', { hand: { kind: 'duty_trips', duty: m.dutyRow, trips: dutyTrips }, zone: { kind: 'new_duty' } });
        break;
      case 2:
        if (m.piece) edit('detach', { tour: m.piece[0], duty: m.piece[1] });
        else edit('move', { tour: m.tour, trips, to: null });
        break;
      case 3:
        edit('unassign', { tour: m.tour, trips });
        break;
      case 4:
      case 5:
        setRf({ hand: { kind: 'swap', tour: m.tour, trips, how: what === 4 ? 'span' : 'from_here' }, zones: null });
        say(tr('Click the bus to swap with (Escape: no swap).'));
        break;
      case 6:
        linkOrUnlink(m.tour, m.trip, !!b.blocks[m.tour]?.trips[m.trip]?.linked);
        break;
      case 7: {
        const line = m.tour != null ? b.blocks[m.tour]?.line : null;
        setRf({ view: TIMEPLAN, tpLine: line, hand: null, zones: null, sel: [], card: null, panel: null });
        break;
      }
      case 9:
        if (m.dutyRow != null) edit('lock_duty', { row: m.dutyRow });
        else edit('lock_block', { tour: m.tour });
        break;
      case 10:
        setRf({ panel: { kind: 'colour', line: b.blocks[m.tour].line } });
        break;
      case 20:
      case 22: {
        const row = m.custom ? m.custom[0] : m.dutyRow;
        if (row != null) setRf({ panel: { kind: 'duty', id: b.duties[row].id } });
        break;
      }
      case 21:
        edit('custom_delete', { row: m.custom[0], ci: m.custom[1] });
        break;
      case 23: {
        const id = b.blocks[m.tour]?.id;
        setRf({ view: VEHICLES, hand: null, zones: null, sel: [], card: null, reveal: id, selRow: id });
        break;
      }
      default:
    }
  };
  return html`<div class="rf-over rf-menu" style=${`left:${x}px;top:${y}px;width:220px;height:${h}px`}>
    ${items.map(([label, what, enabled, tip]) => (label === '—' ? html`<hr />` : html`<${MenuItem} label=${label} disabled=${!enabled} title=${tip || undefined} onClick=${() => go(what)} />`))}
  </div>`;
}

// --- the Validation panel -------------------------------------------------------------------------------

const SCOPE_TITLE = { global: 'Global Issues', duty: 'Duty Issues', vehicle: 'Vehicle Issues' };

/** Optibus's Validation panel above the status bar: the issues of a kind, open or dismissed,
 * grouped where several say the same; ticked ones filtered to or dismissed. */
function Issues({ data, drivers, scope, size }) {
  const r = S.rf;
  const b = data.board;
  const dismissedTab = r.dismissedTab;
  const list = b.issues.map((i, k) => [i, k]).filter(([i]) => i.scope === scope && i.dismissed === dismissedTab);
  const h = Math.min(470, size.h - 32 - 120);
  const ticks = r.issueTicks;
  const keys = list.map(([i]) => i.key);
  const allOn = keys.length > 0 && keys.every((k) => ticks.includes(k));
  const groups = [];
  for (const [i] of list) {
    const g = groups.find((x) => x.group === i.group);
    if (g) g.members.push(i);
    else groups.push({ group: i.group, title: i.title, members: [i] });
  }
  const flip = (ks) => {
    const on = ks.every((k) => ticks.includes(k));
    const rest = ticks.filter((k) => !ks.includes(k));
    setRf({ issueTicks: on ? rest : [...rest, ...ks] });
  };
  const jump = (i) => {
    if (i.timeplan) {
      setRf({ view: TIMEPLAN, tpLine: i.timeplan, tpHint: 'new', panel: null });
      return;
    }
    const id = i.scope === 'duty' ? b.duties[i.row]?.id : i.scope === 'vehicle' ? b.blocks[i.row]?.id : null;
    if (!id) return;
    const patch = { reveal: id, selRow: id };
    if ((i.scope === 'duty') !== drivers) patch.view = drivers ? VEHICLES : DRIVERS;
    setRf(patch);
  };
  const any = ticks.length > 0;
  return html`<div class="rf-over rf-issues" style=${`right:150px;bottom:${32 + 8}px;width:470px;height:${h}px`} data-rf="rf-issues-panel">
    <div class="head"><b>${tr(SCOPE_TITLE[scope])}</b><button type="button" onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${20} /></button></div>
    <div class="rf-itabs">${['Open issues', 'Dismissed issues'].map((l, k) => html`<button type="button" class=${(k === 1) === dismissedTab ? 'on' : ''} onClick=${() => (k === 1) !== dismissedTab && setRf({ dismissedTab: k === 1, issueTicks: [] })}>${tr(l).toUpperCase()}</button>`)}</div>
    <${CheckRow} on=${allOn} label=${tr('Select all (%{n})', { n: list.length })} raw class="all" onChange=${() => setRf({ issueTicks: allOn ? [] : keys })} />
    <div class="list">
      ${!list.length && html`<p class="none">${tr(dismissedTab ? 'No dismissed issues.' : 'No issues: every rule is kept.')}</p>`}
      ${groups.map((g) => {
        const several = g.members.length > 2;
        const open = r.groupsOpen.includes(g.group);
        const gkeys = g.members.map((i) => i.key);
        return html`${several && html`<div class="group">
            <button type="button" class=${'tick' + (gkeys.every((k) => ticks.includes(k)) ? ' on' : '')} onClick=${() => flip(gkeys)}>${gkeys.every((k) => ticks.includes(k)) && html`<${Icon} name="check" size=${12} />`}</button>
            <span class="badge">${g.members.length}</span><b class="cut">${g.title}</b>
            <button type="button" class="fold" onClick=${() => setRf({ groupsOpen: open ? r.groupsOpen.filter((x) => x !== g.group) : [...r.groupsOpen, g.group] })}><${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${18} /></button>
          </div>`}
          ${(!several || open) && g.members.map((i) => html`<div class=${'issue' + (several ? ' in' : '')}>
            <button type="button" class=${'tick' + (ticks.includes(i.key) ? ' on' : '')} onClick=${() => flip([i.key])}>${ticks.includes(i.key) && html`<${Icon} name="check" size=${12} />`}</button>
            <button type="button" class=${'go' + (i.hard ? ' hard' : '')} onClick=${() => jump(i)}>${i.row_words ? `${i.text} — ${i.row_words}` : i.text}</button>
          </div>`)}`;
      })}
    </div>
    <div class="foot">
      <${RfButton} label="Filter issues" disabled=${!any || scope === 'global'} style="width:190px;height:32px" onClick=${() => {
        const patch = { filter: { ...r.filter, issues: [...ticks] }, panel: null };
        if ((scope === 'duty') !== drivers) patch.view = scope === 'duty' ? DRIVERS : VEHICLES;
        setRf(patch);
      }} />
      <${RfButton} label=${dismissedTab ? 'Restore issues' : 'Dismiss issues'} solid disabled=${!any} style="width:190px;height:32px" onClick=${() => rfAct('dismiss', { keys: ticks, on: !dismissedTab }).then(() => setRf({ panel: { kind: 'issues', scope }, issueTicks: [] }))} />
    </div>
  </div>`;
}

// --- the duty panel ------------------------------------------------------------------------------------

/** A duty by hand (`rf_panels::duty_panel`): its type, lock, times and paid time, signing, its
 * pieces (taken out one by one), the way between them, its breaks, its custom work (added and
 * deleted), its issues. */
function DutyPanel({ data, id, gTop }) {
  const b = data.board;
  const row = b.duties.findIndex((s) => s.id === id);
  const [v, setV] = useState(null);
  const r = S.rf;
  useEffect(() => {
    if (row < 0) return;
    let gone = false;
    rfCmd('rf_view', { view: 'duty', day: S.rf.day, row }).then((x) => !gone && setV(x)).catch(() => {});
    return () => {
      gone = true;
    };
  }, [row, r.rev]);
  useEffect(() => {
    if (row < 0) setRf({ panel: null });
  }, [row]);
  if (row < 0 || !v || v.gone) return null;
  const note = (field, value, more = {}) => rfAct('note', { row, field, value, ...more });
  const [kind, from, to] = r.customDraft;
  return html`<div class="rf-over rf-duty" style=${`top:${gTop + 8}px;bottom:${32 + 8}px;right:10px;width:400px`} data-rf="rf-duty-panel">
    <div class="head"><${Icon} name="person" size=${19} /><b class="cut">${v.label}  /  ${v.kind}</b>
      <button type="button" data-rf="rf-duty-lock" title=${tr(v.locked ? 'Locked: Optimise leaves it. Click to unlock' : 'Lock the duty: Optimise leaves it')} class=${v.locked ? 'on' : ''} onClick=${() => rfAct('lock_duty', { row })}><${Icon} name=${v.locked ? 'lock' : 'livery_unlock'} size=${19} /></button>
      <button type="button" onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${20} /></button></div>
    <div class="scroll">
      <${Caps} text="Duty type" />
      <${Select} value=${v.type_at} options=${v.types} style="width:100%;height:32px;margin:6px 0 12px" onChange=${(k) => note('kind', k)} />
      <div class="times">${v.times.map(([a, z]) => html`<span>${a}</span><b>${z}</b>`)}</div>
      <div class="line"><span>${tr('Paid time')}</span><${Stepper} value=${v.paid} step=${5} min=${0} max=${1440} fmt=${hm} style="width:130px" onChange=${(x) => note('paid', x)} />
        ${v.paid_set && html`<${RfButton} label="Automatic" style="width:100px;height:28px" onClick=${() => note('paid_auto', null)} />`}</div>
      ${v.signing && html`<div class="line"><span>${tr('Sign-on')}</span><${Stepper} value=${v.sign_on} step=${1} min=${0} max=${60} fmt=${(x) => `${x} min`} style="width:110px" onChange=${(x) => note('sign_on', x)} /></div>
        <div class="line"><span>${tr('Sign-off')}</span><${Stepper} value=${v.sign_off} step=${1} min=${0} max=${60} fmt=${(x) => `${x} min`} style="width:110px" onChange=${(x) => note('sign_off', x)} /></div>`}
      <${Caps} text="Pieces of work" style="margin-top:6px" />
      ${v.pieces.map((pc, n) => html`<div class="piece"><span class="plate" style=${`background:${pc.colour}`}>${pc.number}</span><span class="cut">${pc.words}</span>
          ${v.pieces.length > 1 && html`<button type="button" title=${tr('Take the piece out: a duty of its own')} onClick=${() => rfAct('detach', { tour: pc.tour, duty: pc.duty })}><${Icon} name="close" size=${15} /></button>`}</div>
        ${pc.travel != null && html`<div class="way"><${Icon} name="directions_walk" size=${15} /><${Seg} at=${pc.travel} labels=${v.travel_labels} raw onPick=${(k) => note('travel', k, { gap: n })} /></div>`}
        ${pc.between && html`<div class="between">${pc.between}</div>`}`)}
      ${v.rest && html`<${Caps} text="Breaks" style="margin-top:8px" />
        ${!v.rest.breaks.length && html`<p class="breaks">${v.rest.none}</p>`}
        ${v.rest.breaks.map((b) => html`<div class="rest" title=${b.tip}><i class=${b.kind === 'split' ? 'split' : b.counts ? 'ok' : 'short'}></i><span>${b.words}</span></div>`)}
        <${Caps} text="Driving periods" style="margin-top:6px" />
        ${v.rest.periods.map((x) => html`<div class="rest" title=${x.tip}><i class=${x.over ? (v.rest.period_hard ? 'err' : 'warn') : 'calm'}></i><span>${x.words}</span></div>`)}
        ${v.rest.meal && html`<div class="rest"><i class=${v.rest.meal_hard ? 'err' : 'warn'}></i><span>${v.rest.meal}</span></div>`}`}
      <${Caps} text="Custom work" style="margin-top:8px" />
      ${v.custom.map((cw, n) => html`<div class="custom"><span>${cw}</span><button type="button" onClick=${() => rfAct('custom_delete', { row, ci: n })}><${Icon} name="delete" size=${15} /></button></div>`)}
      <div class="addcw">
        <${Select} value=${kind} options=${v.kinds} style="width:120px;height:30px" onChange=${(k) => setRf({ customDraft: [k, from, to] })} />
        <${Stepper} value=${from} step=${15} min=${0} max=${1800} fmt=${clock} style="width:100px" onChange=${(x) => setRf({ customDraft: [kind, x, Math.max(to, x + 15)] })} />
        <${Stepper} value=${to} step=${15} min=${from + 15} max=${1800} fmt=${clock} style="width:100px" onChange=${(x) => setRf({ customDraft: [kind, from, x] })} />
        <${RfButton} label="Add" solid style="width:54px;height:28px" onClick=${() => rfAct('note', { row, field: 'custom', kind, from, to })} />
      </div>
      ${v.issues.length > 0 && html`<${Caps} text="Issues" style="margin-top:10px" />${v.issues.map((i) => html`<div class="iss"><span class=${i.hard ? 'err' : 'warn'}><${Icon} name="error" size=${13} /></span><span>${i.text}</span></div>`)}`}
      ${v.spare && html`<${RfButton} label="Delete duty" style="width:160px;height:30px;margin-top:10px" onClick=${() => rfAct('spare_remove', { row }).then(() => setRf({ panel: null }))} />`}
    </div>
  </div>`;
}

// --- the small windows -----------------------------------------------------------------------------------

/** The schedule's history under its icon: every change, the latest first; a click goes back to
 * before it. */
function History({ data, size }) {
  const undo = data.head?.undo || [];
  const shown = undo.map((l, k) => [l, k]).reverse().slice(0, 14);
  return html`<div class="rf-over rf-menu rf-history" style=${`left:${Math.max(4, size.w - 760)}px;top:4px;width:380px`}>
    <${Caps} text="Schedule history" class="menu-caps" />
    <div class="now">${tr('Now')}</div>
    ${shown.map(([label, k]) => html`<${MenuItem} label=${label} raw title="Go back to before this change" onClick=${() => rfAct('back_to', { k }).then(() => setRf({ panel: null }))} />`)}
  </div>`;
}

/** Stats: the columns of figures beside the IDs. */
function StatsWindow({ drivers, gTop }) {
  const r = S.rf;
  const options = STATS.filter((s) => drivers || (s !== 'paid' && s !== 'type'));
  return html`<div class="rf-over rf-menu rf-small" style=${`left:250px;top:${gTop - 2}px;width:260px;height:${options.length * 30 + 90}px`}>
    ${options.map((s) => html`<${CheckRow} on=${r.stats.includes(s)} label=${STAT_LABEL[s]} onChange=${(on) => setRf({ stats: on ? [...r.stats, s] : r.stats.filter((x) => x !== s) })} />`)}
    <div class="foot"><${RfButton} label="Reset" style="width:100px;height:30px" onClick=${() => setRf({ stats: [], sort: null })} /><${RfButton} label="Done" solid style="width:100px;height:30px" onClick=${() => setRf({ panel: null })} /></div>
  </div>`;
}

/** Layers: what the trips' chips say and how they are coloured (the company's choice, with its
 * legend), and the layovers' minutes. */
function LayersWindow({ data, gTop }) {
  const ly = S.rf.layers;
  const set = (patch) => setRf({ layers: { ...ly, ...patch } });
  const periods = ly.periods !== false;
  const by = colourBy(data);
  return html`<div class="rf-over rf-menu rf-small" style=${`left:330px;top:${gTop - 2}px;width:330px;height:460px`}>
    <${Caps} text="Colour by" class="menu-caps" />
    ${BY.map((k, n) => html`<${CheckRow} on=${by === k} label=${BY_LABEL[n]} onChange=${() => by !== k && setColourBy(k)} />`)}
    <${CheckRow} on=${legendOn(data)} label="Show the legend" onChange=${() => set({ legend: !legendOn(data) })} />
    <${Caps} text="Trips" class="menu-caps" />
    ${['Route', 'Start and end time', 'Start and end stops'].map((w, k) => html`<${CheckRow} on=${ly.label === k} label=${w} onChange=${() => set({ label: k })} />`)}
    <${Caps} text="Between trips" class="menu-caps" />
    <${CheckRow} on=${ly.breaks} label="Break duration" onChange=${() => set({ breaks: !ly.breaks })} />
    <${CheckRow} on=${ly.moved} label="Mark trips off their timetable tour" onChange=${() => set({ moved: !ly.moved })} />
    <${Caps} text="Duties" class="menu-caps" />
    <${CheckRow} on=${periods} label="Driving periods under the duties" onChange=${() => set({ periods: !periods })} />
  </div>`;
}

/** Filter: the lines, the direction, the departure times, locked only. */
function FilterWindow({ data, size }) {
  const r = S.rf;
  const f = r.filterEdit || { ...r.filter };
  const set = (patch) => setRf({ filterEdit: { ...f, ...patch } });
  const lines = data.lines;
  const rows = Math.ceil(lines.length / 3);
  const h = 300 + rows * 30;
  return html`<div class="rf-over rf-dim" onMouseDown=${(e) => e.target === e.currentTarget && setRf({ panel: null })}>
    <div class="rf-sheetwin rf-filter" style=${`left:${size.w / 2 - 290}px;top:70px;width:580px;height:${h}px`}>
      <div class="head"><b>${tr('Filter')}</b></div>
      <div class="body">
        <${Caps} text="Vehicles" class="accent" />
        <div class="lines">${lines.map((l) => {
          const on = !f.hidden.some((x) => eq(x, l.name));
          return html`<${CheckRow} on=${on} label=${tr('Line %{n}', { n: l.number })} raw onChange=${() => set({ hidden: on ? [...f.hidden, l.name] : f.hidden.filter((x) => !eq(x, l.name)) })} />`;
        })}</div>
        <${CheckRow} on=${f.lockedOnly} label="Locked only" style="width:300px" onChange=${() => set({ lockedOnly: !f.lockedOnly })} />
        <${Caps} text="Trips" class="accent" style="margin-top:16px" />
        <${Seg} at=${f.dir} labels=${['Both ways', 'Outbound', 'Inbound']} style="width:330px" onPick=${(k) => set({ dir: k })} />
        <div class="deps"><span>${tr('Departure from')}</span><${Stepper} value=${f.from} step=${30} min=${0} max=${1800} fmt=${clock} style="width:110px" onChange=${(v) => set({ from: v })} />
          <span class="to">${tr('to')}</span><${Stepper} value=${Math.min(f.to, 1800)} step=${30} min=${0} max=${1800} fmt=${(v) => (v >= 1800 ? '—' : clock(v))} style="width:110px" onChange=${(v) => set({ to: v >= 1800 ? 2880 : v })} /></div>
      </div>
      <div class="foot">
        <${RfButton} label="Clear all" style="width:120px;height:32px" onClick=${() => setRf({ filterEdit: { ...NO_FILTER } })} />
        <span class="grow"></span>
        <${RfButton} label="Cancel" style="width:116px;height:32px" onClick=${() => setRf({ panel: null })} />
        <${RfButton} label="Apply filter" solid style="width:142px;height:32px" onClick=${() => setRf({ filter: { ...f, issues: r.filter.issues }, panel: null })} />
      </div>
    </div>
  </div>`;
}

/** The zoom's choices above it. */
function ZoomMenu() {
  return html`<div class="rf-over rf-menu" style=${`right:10px;bottom:${32 + 6}px;width:110px;height:190px`}>
    ${[100, 150, 200, 300, 400, 600].map((z) => html`<${MenuItem} label=${`${z}%`} raw onClick=${() => setRf({ zoom: z / 100, panel: null })} />`)}
  </div>`;
}

/** Print: the bus graph or the duty graph as a PDF file, kept and opened. */
function PrintMenu() {
  // (saved where the player chooses; its folder shown then, as the game-drawn launcher shows it)
  const print = async (bus) => {
    setRf({ panel: null });
    const r = await rfCmd('rf_print', { day: S.rf.day, bus }).catch((e) => ({ say: { text: e.message, error: true } }));
    after(r);
    if (r?.printed && r.reveal) call('open_path', { path: r.folder }).catch(() => {});
  };
  return html`<div class="rf-over rf-menu" style="left:280px;top:4px;width:260px;height:74px">
    <${MenuItem} label="Bus graph (PDF)" title="Every vehicle's day on the hours, as Optibus prints it" onClick=${() => print(true)} />
    <${MenuItem} label="Duty graph (PDF)" title="Every driver's duty on the hours" onClick=${() => print(false)} />
  </div>`;
}

/** The day: the next seven, each with what it runs (`days.js`: follows a day, its own changes,
 * this date only, a pattern of others). */
function DayMenu({ size }) {
  const r = S.rf;
  const days = r.days || [];
  const marks = r.data?.head?.days || [];
  return html`<div class="rf-over rf-menu" style=${`left:${size.w - 560}px;top:4px;width:270px;height:${dayMenuHeight(days, marks)}px`}>
    ${days.map((d, k) => html`<${DayItem} label=${d.label} tags=${marks[k]} on=${r.day === k} onPick=${() => {
      setRf({ day: k, panel: null });
      settle();
    }} />`)}
  </div>`;
}

/** A colour picker (`accent_pick::picker`): the square of saturation and value, the hue's strip,
 * the code and the colour itself. */
function Picker({ value, onPick }) {
  const [hsv, setHsv] = useState(() => toHsv(hexOf(value)));
  const [code, setCode] = useState(value);
  useEffect(() => {
    setHsv(toHsv(hexOf(value)));
    setCode(value);
  }, [value]);
  const sq = useRef(null);
  const bar = useRef(null);
  const drag = (el, f) => (e) => {
    const b = el.current.getBoundingClientRect();
    const go = (ev) => f(Math.max(0, Math.min(1, (ev.clientX - b.left) / b.width)), Math.max(0, Math.min(1, (ev.clientY - b.top) / b.height)));
    go(e);
    const mv = (ev) => go(ev);
    const up = () => {
      removeEventListener('mousemove', mv);
      removeEventListener('mouseup', up);
    };
    addEventListener('mousemove', mv);
    addEventListener('mouseup', up);
  };
  const emit = (h) => {
    setHsv(h);
    onPick(toHex(h));
  };
  const hue = toHex([hsv[0], 1, 1]);
  return html`<div class="rf-picker">
    <div class="sv" ref=${sq} style=${`background:${hue}`} onMouseDown=${drag(sq, (x, y) => emit([hsv[0], x, 1 - y]))}><i style=${`left:${hsv[1] * 100}%;top:${(1 - hsv[2]) * 100}%;background:${value}`}></i></div>
    <div class="hue" ref=${bar} onMouseDown=${drag(bar, (x) => emit([Math.min(359.99, x * 360), hsv[1] < 0.05 ? 0.8 : hsv[1], hsv[2] < 0.05 ? 0.85 : hsv[2]]))}><i style=${`left:${(hsv[0] / 360) * 100}%`}></i></div>
    <div class="row"><input value=${code} onInput=${(e) => {
      const v = e.currentTarget.value;
      setCode(v);
      if (/^#?[0-9a-f]{6}$/i.test(v.trim())) onPick('#' + v.trim().replace('#', ''));
    }} /><span class="sample" style=${`background:${value}`}>Aa</span></div>
  </div>`;
}

function toHsv([r, g, b]) {
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const d = max - Math.min(R, G, B);
  let h = 0;
  if (d) h = max === R ? 60 * (((G - B) / d) % 6) : max === G ? 60 * ((B - R) / d + 2) : 60 * ((R - G) / d + 4);
  return [(h + 360) % 360, max ? d / max : 0, max];
}

function toHex([h, s, v]) {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return '#' + [r, g, b].map((q) => Math.round((q + m) * 255).toString(16).padStart(2, '0')).join('');
}

/** A line's colour: the palette's swatches and a picker for any other. */
function ColourWindow({ data, line, size }) {
  const cl = data.lines.find((l) => eq(l.name, line));
  if (!cl) return null;
  const set = (colour) => rfAct('colour', { line, colour });
  return html`<div class="rf-over rf-sheetwin rf-colour" style=${`left:${size.w / 2 - 170}px;top:120px;width:340px;height:330px`} data-rf="rf-colour">
    <div class="title"><b>${tr('Colour of line %{n}', { n: cl.number })}</b><button type="button" onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${18} /></button></div>
    <div class="swatches">${(data.palette || []).map((sw) => html`<button type="button" class=${sw === cl.colour ? 'on' : ''} style=${`background:${sw}`} onClick=${() => set(sw)}></button>`)}</div>
    <${Picker} value=${cl.colour} onPick=${set} />
    <${RfButton} label="Automatic" style="width:150px;height:30px;position:absolute;left:18px;bottom:14px" onClick=${() => set(null)} />
  </div>`;
}

/** G: go to a vehicle or a duty by its ID. */
function GoTo({ data, drivers, text, size }) {
  const ref = useRef(null);
  useEffect(() => ref.current?.focus(), []);
  const b = data.board;
  const go = () => {
    const want = text.trim().toLowerCase();
    const found = drivers ? b.duties.find((s) => s.label.toLowerCase() === want)?.id : b.blocks.find((t) => t.label.toLowerCase() === want || t.label.toLowerCase().endsWith('/' + want) || t.tour.toLowerCase() === want)?.id;
    if (found) setRf({ reveal: found, selRow: found, pinned: S.rf.pinned.filter((x) => x !== found), panel: null });
    else say(tr('No vehicle or duty with that ID.'), true);
  };
  return html`<div class="rf-over rf-sheetwin rf-goto" style=${`left:${size.w / 2 - 180}px;top:100px;width:360px;height:92px`}>
    <${Caps} text=${drivers ? 'Go to duty' : 'Go to vehicle'} />
    <span class="rf-input"><${Icon} name="search" size=${18} /><input ref=${ref} value=${text} placeholder=${drivers ? '1003' : '1/02'}
      onInput=${(e) => setRf({ panel: { kind: 'goto', text: e.currentTarget.value } })}
      onKeyDown=${(e) => {
        if (e.key === 'Enter') go();
        if (e.key === 'Escape') {
          e.stopPropagation();
          setRf({ panel: null });
        }
      }} /></span>
  </div>`;
}

/** The optimisation settings: what Optimise makes, and Optimise itself. */
function OptSettings({ data, size }) {
  const pr = data.prefs;
  return html`<div class="rf-over rf-sheetwin rf-opt" style=${`left:${size.w - 440}px;top:60px;width:420px;height:250px`}>
    <div class="head"><b>${tr('Optimisation settings')}</b></div>
    <div class="body">
      <${Switch} on=${pr.optimise_vehicles} label="Optimise vehicles (the blocks)" onChange=${(v) => rfAct('opt_flags', { vehicles: v })} />
      <${Switch} on=${pr.optimise_duties} label="Optimise duties (cut and paired)" onChange=${(v) => rfAct('opt_flags', { duties: v })} />
      <p>${tr('Locked vehicles and duties are kept; the preferences say what may be and what it costs.')}</p>
    </div>
    <div class="foot">
      <${RfButton} label="Restore defaults" style="width:150px;height:32px" onClick=${() => rfAct('opt_flags', { restore: true })} />
      <span class="grow"></span>
      <${RfButton} label="Close" style="width:110px;height:32px" onClick=${() => setRf({ panel: null })} />
      <${RfButton} label="Optimize" solid style="width:118px;height:32px" onClick=${() => {
        setRf({ panel: null });
        if (S.rf.view === VEHICLES || S.rf.view === DRIVERS) rfAct('optimise');
        else {
          setRf({ view: VEHICLES });
          rfAct('optimise');
        }
      }} />
    </div>
  </div>`;
}

/** A histogram large: its bars with counts and shares. */
function HistogramWindow({ k, which, size }) {
  const bars = which === 0 ? k.all_types : k.bands.map(([b, n]) => [`${b} h`, n]);
  const total = Math.max(1, bars.reduce((s, b) => s + b[1], 0));
  const most = Math.max(1, ...bars.map((b) => b[1]));
  return html`<div class="rf-over rf-dim" onMouseDown=${(e) => e.target === e.currentTarget && setRf({ panel: null })}>
    <div class="rf-sheetwin rf-bigHist" style=${`left:${size.w / 2 - 300}px;top:80px;width:600px;height:360px`}>
      <div class="title"><b>${tr(which === 0 ? 'Duty types' : 'Duties paid time').toUpperCase()}</b><button type="button" onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${18} /></button></div>
      <div class="bars">${bars.map(([label, v]) => html`<span class="bar"><i>${v} (${Math.round((v * 100) / total)}%)</i><b style=${`height:${Math.max(1, (v / most) * 230)}px`}></b><small>${label}</small></span>`)}</div>
    </div>
  </div>`;
}

export { handSpan, pieceTrips };

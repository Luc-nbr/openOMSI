// RosterFlow's Routes tab (Beta 61; a tester: "when roster view is enabled, players should be able
// to create a route within roster view without exiting the program"): Optibus's route editor in
// RosterFlow's own look. On top the route bar: the route, a new one, the days it runs, its colour,
// its two directions named after where they go, and saving it (a new route confirmed and paid, a
// change of one in service saved). Under it the direction's stops in order on the left - its
// pattern, a stop dragged, moved or taken out, how many and how long - and the map on the right:
// the route over the roads in its colour, its stops, START and END, a search for a stop, the zoom.
// Along the foot the trip-time strip: the stops with their codes, the minutes from each to the
// next (clicked and typed) and the trip's time.
//
// It is the line editor itself (`editors/lines`): the game program keeps it (`hub_le`,
// `lineeditor::headless`) with the map's lanes and its router, the line editor's own map draws it
// (`LineMap`: a stop clicked goes into the direction, the way to it is found over the roads), and
// a route is saved where the line editor saves a line - the map's lines and the company's - so
// that the line editor, the Timeplan and the tenders all have it. The company is stopped and saved
// before the line editor reads it (`co_leave`), read again by the line editor before anything is
// paid (`c_reread`: its clock goes on meanwhile), and read again here once the line editor wrote
// it (`co_reread`).

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { cmd as coCmd } from '../company/kit.js';
import { ColourPicker, mapLabel } from '../editors/kit.js';
import { L, le, redraw } from '../editors/lines/state.js';
import { LineMap } from '../editors/lines/map.js';
import { EstimateCard, ChangeDialog } from '../editors/lines/company.js';
import { inkOn } from '../editors/lines/plate.js';
import { setRf, setPart, bump, say, TIMEPLAN } from './state.js';
import { Icon } from './ui.js';

// (its own look, beside rosterflow.css)
if (!document.querySelector('link[data-rf-routes-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/rosterflow-routes.css';
  sheet.dataset.rfRoutesCss = '1';
  document.head.append(sheet);
}

/** The title bar's tab (after `state.js`'s four). */
export const ROUTES = 4;

/** A stop's column in the trip-time strip (px). */
const COL = 104;
/** The days a route runs, as its kind of service has them (`service::ServiceKind::allows`, which
 * the Timeplan makes its trips by). */
const KINDS = [
  ['regular', 'Mon-Fri, Sat, Sun'],
  ['school', 'Mon-Fri, school days'],
  ['leisure', 'Sat, Sun'],
  ['on_demand', 'Mon-Fri, Sat, Sun, on demand'],
];

/** What the tab keeps besides `S.rf.rt`: the company the line editor was opened for, the line
 * editor's last word passed on, changes in hand said once before they are left, the route the
 * camera was put on, the colour being dragged, the company being read again. */
const RT = { open: null, seen: 0, warned: false, fitted: null, colourNext: null, colourBusy: false, rereading: Promise.resolve() };

/** What the tab shows (`S.rf.rt`): the route chosen (a line editor's id, or a map's line by its
 * timetable name), the colour's picker open, the strip's cell typed into and its text, the
 * stop searched for. */
const rt = () => S.rf.rt || {};
const rtSet = (patch) => setPart('rt', patch);

// --- the line editor for the company ---------------------------------------------------------------

/** The line editor opened for the company on its map (`open`), the route chosen before (else the
 * company's first). Each time the tab is shown: what another tab saved meanwhile is read with it.
 * Whether the line editor answered. */
async function openFor(company) {
  RT.open = company;
  RT.warned = false;
  // (only what the line editor says from now on is passed on)
  RT.seen = Date.now() - 1;
  L.hooks = { timeplan: toTimeplan, companyChanged };
  L.view = null;
  await coCmd('co_leave').catch(() => {});
  const maps = (S.maps || []).map((m) => [mapLabel(m), m.file]);
  // (line 0 is none of the line editor's: no new line made, the company's first one shown)
  await le('open', { maps, root: S.boot?.root || '', date: S.choice?.date || '', choice_map: S.choice?.map || '', company, line: 0, new_days: null, map_line: '' });
  const v = L.view;
  const want = rt().sel;
  if (!v?.company) return !!v;
  if (typeof want === 'number' && v.rows.some((r) => r.id === want) && v.sel !== want) await le('pick_line', { id: want });
  else if (typeof want === 'string' && v.map_rows.some((r) => r.ttl === want)) await le('pick_map_line', { ttl: want });
  return true;
}

/** The line editor wrote the company's file: the company read again, RosterFlow's day with it, and
 * the Timeplan to open the route anew when it is shown next. */
function companyChanged(id) {
  const company = id || S.co.shell?.id;
  RT.rereading = coCmd('co_reread', { id: company })
    .catch(() => null)
    .then(() => {
      bump();
      return timeplanFollows();
    });
}

/** The Timeplan to open the route shown, read again, when it is shown next. */
async function timeplanFollows() {
  const line = L.view?.line;
  if (!line || S.rf.view !== ROUTES) return;
  const name = line.map ? line.map.ttl : (await coCmd('co_line_of', { id: line.id }).catch(() => null))?.name;
  if (name) setRf({ tpLine: name, tpHint: null, tpReload: true });
}

/** A route just confirmed (`after_confirm`): its timetable made next, in the Timeplan. */
async function toTimeplan({ id, name, hint } = {}) {
  await RT.rereading;
  const line = name || (id != null && (await coCmd('co_line_of', { id }).catch(() => null))?.name) || '';
  setRf({ view: TIMEPLAN, tpLine: line, tpHint: hint ? 'new' : null, tpReload: true, hand: null, zones: null, sel: [], card: null, menu: null, panel: null });
}

/** A request that pays or writes the company: the company saved first and read again by the line
 * editor, so that what is paid is taken from the company as it is now. */
async function paid(op, args = {}) {
  await coCmd('co_leave').catch(() => {});
  await le('c_reread');
  return le(op, args);
}

/** The route's colour from the picker: while it is dragged, the last one when the one before is
 * taken (as the line editor's own). */
function setColour(hex) {
  RT.colourNext = hex;
  if (RT.colourBusy) return;
  RT.colourBusy = true;
  const go = () => {
    const value = RT.colourNext;
    RT.colourNext = null;
    le('colour', { value }).then(() => (RT.colourNext ? go() : (RT.colourBusy = false)));
  };
  go();
}

/** Changes of the route not saved, on leaving the tab or RosterFlow: said once (true: stay), let
 * go the next time (the tab reads the route again when it is shown). */
export function holdChanges() {
  const v = L.view;
  if (S.rf.view !== ROUTES || RT.open == null || !v?.company || !v.dirty || v.fresh || RT.warned) return false;
  RT.warned = true;
  say(tr('The route has changes not saved: save them, or do it again to leave them.'), true);
  return true;
}

/** Escape on the Routes tab: the cell typed into, the picker, the search, then changes not saved
 * said first. Returns whether it took the key. */
export function rtEscape() {
  const t = rt();
  if (t.cell != null || t.colour || t.search) {
    rtSet({ cell: null, text: '', colour: false, search: '' });
    return true;
  }
  return holdChanges();
}

// --- what is shown ---------------------------------------------------------------------------------

const isLoop = (d) => {
  const s = d?.stops || [];
  return s.length >= 3 && s[0].id === s[s.length - 1].id && s[0].tile?.[0] === s[s.length - 1].tile?.[0] && s[0].tile?.[1] === s[s.length - 1].tile?.[1];
};

/** Where a direction goes (`Direction::destination`): its destination, a loop's word, else its last
 * stop. */
function destination(d) {
  if (d?.terminus?.trim()) return d.terminus.trim();
  if (isLoop(d)) return tr('Ring');
  return d?.stops?.[d.stops.length - 1]?.name?.trim() || '';
}

/** Metres as the app's units say (`units`: miles in the British and American ones). */
function distance(m) {
  const miles = ['uk', 'us'].includes(S.settings?.units);
  const x = miles ? m / 1609.344 : m / 1000;
  let n;
  try {
    n = x.toLocaleString(language() || 'en', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  } catch {
    n = x.toFixed(1);
  }
  return tr(miles ? '%{n} mi' : '%{n} km', { n });
}

/** The minutes of each segment of a direction (stop to next stop), from its times. */
function segments(d) {
  const t = d?.times || [];
  const n = d?.stops?.length || 0;
  const at = (k) => t[Math.min(k, t.length - 1)] ?? 0;
  return Array.from({ length: Math.max(0, n - 1) }, (_, k) => Math.max(0, Math.round(at(k + 1) - at(k))));
}

/** The camera on the route shown (both its directions) with some air round it; the farthest out
 * the whole map's (as `map.js` fits it). Whether there was a route to put it on. */
function fitRoute(box) {
  const m = L.map;
  const line = L.view?.line;
  const pts = (line?.directions || []).flatMap((d) => d.stops_at || []);
  if (!m || !box || !pts.length) return false;
  const [W, H] = [box.clientWidth || 1, box.clientHeight || 1];
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  const [lo, hi] = [[Math.min(...xs), Math.min(...ys)], [Math.max(...xs), Math.max(...ys)]];
  const mpp = Math.max(Math.max(200, hi[0] - lo[0]) / W, Math.max(200, hi[1] - lo[1]) / H) * 1.3;
  const most = Math.max(Math.max((m.hi[0] - m.lo[0]) / W, (m.hi[1] - m.lo[1]) / H) * 2, 0.6);
  L.cam = { cx: (lo[0] + hi[0]) / 2, cy: (lo[1] + hi[1]) / 2, mpp: Math.max(0.15, Math.min(mpp, most)), most };
  redraw();
  return true;
}

/** The map on stop `i` of the map's (found by its name), ringed as under the mouse. */
function lookAt(i) {
  const s = L.map?.stops?.[i];
  if (!s || !L.cam) return;
  L.cam = { ...L.cam, cx: s.at[0], cy: s.at[1], mpp: Math.min(L.cam.mpp, 1.2) };
  L.hover = i;
  redraw();
}

// --- the tab ---------------------------------------------------------------------------------------

export function RoutesTab() {
  // (company multiplayer: the routes are drawn by the boss's line editor, on his map's roads -
  // the Timeplan's trips and times are a player's to change too)
  if (S.co?.remote) return html`<${RemoteRoutes} />`;
  return html`<${OwnRoutes} />`;
}

/** The Routes tab of a company joined: why the routes are the boss's, the way to the Timeplan. */
function RemoteRoutes() {
  return html`<div class="rf-tp rt"><div class="tp-why rt-failed"><p>${tr("The routes are drawn on the boss's map with his line editor: he changes them on his own launcher. The trips and their times you change in the Timeplan, as your role allows.")}</p>
    <button type="button" class="rt-act" onClick=${() => setRf({ view: TIMEPLAN, hand: null, zones: null, sel: [], card: null, menu: null, panel: null })}><${Icon} name="departure_board" size=${17} /><span>${tr('Timeplan')}</span></button></div></div>`;
}

function OwnRoutes() {
  const company = S.co.shell?.id;
  const [opened, setOpened] = useState(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    if (!company || !S.maps?.length) return undefined;
    let gone = false;
    setOpened(null);
    openFor(company).then((ok) => !gone && setOpened(ok ? company : false));
    return () => {
      gone = true;
    };
  }, [company, S.maps?.length, tries]);
  const v = L.view;
  // (the map still being read: asked again until it is - the line editor's page asks only for
  // itself)
  useEffect(() => {
    if (!opened || !v?.reading) return undefined;
    const t = setTimeout(() => S.rf.view === ROUTES && le('view'), 400);
    return () => clearTimeout(t);
  }, [opened, v]);
  // (what the line editor says, in RosterFlow's corner; changes saved: a leaving is free again)
  useEffect(() => {
    const st = S.hubStatus;
    if (st && st.at > RT.seen) {
      RT.seen = st.at;
      say(st.text, st.err);
      set({ hubStatus: null });
    }
    if (!v?.dirty) RT.warned = false;
  });
  if (!company) return html`<div class="rf-tp rt"></div>`;
  if (!S.maps?.length) return html`<div class="rf-tp rt"><p class="tp-why">${tr('No maps found.')}</p></div>`;
  if (opened === false) {
    return html`<div class="rf-tp rt"><div class="tp-why rt-failed"><p>${tr('The line editor could not be opened.')}</p>
      <button type="button" class="rt-act" onClick=${() => setTries((n) => n + 1)}><${Icon} name="refresh" size=${17} /><span>${tr('Try again')}</span></button></div></div>`;
  }
  if (opened !== company || !v) return html`<div class="rf-tp rt"><p class="tp-why">${tr('Loading...')}</p></div>`;
  if (!v.company) return html`<div class="rf-tp rt"><p class="tp-why">${tr("The company's map is not installed.")}</p></div>`;
  return html`<div class="rf-tp rt" data-rf="rf-routes">
    <${RouteBar} v=${v} />
    <div class="rt-main">
      <${StopsPanel} v=${v} />
      <${MapPane} v=${v} />
    </div>
    <${TripStrip} v=${v} />
    <${ChangeDialog} />
  </div>`;
}

// --- the route bar ---------------------------------------------------------------------------------

/** The route bar: the route and a new one, its days, its colour, its directions, saving it. */
function RouteBar({ v }) {
  const line = v.line;
  const t = rt();
  const key = line ? (line.map ? 'm:' + line.map.ttl : 'o:' + line.id) : '';
  const draft = tr('draft');
  const options = [
    ...v.rows.map((r) => ['o:' + r.id, `${r.number}  ·  ${r.name}${r.draft ? `  (${draft})` : ''}`]),
    ...v.map_rows.map((r) => ['m:' + r.ttl, `${r.number}  ·  ${r.label}`]),
  ];
  const pick = (k) => {
    if (k === key) return;
    rtSet({ cell: null, text: '', colour: false });
    if (k.startsWith('o:')) {
      const id = Number(k.slice(2));
      rtSet({ sel: id });
      le('pick_line', { id });
    } else {
      const ttl = k.slice(2);
      rtSet({ sel: ttl });
      le('pick_map_line', { ttl });
    }
  };
  const fresh = () => {
    rtSet({ cell: null, text: '', colour: false });
    le('new_line').then(() => rtSet({ sel: L.view?.sel ?? null }));
  };
  const dirs = line?.directions || [];
  return html`<div class="tp-bar rt-bar" data-rf="rf-routes-bar">
    <span class="rt-cap">${tr('Route').toUpperCase()}</span>
    <span class="tp-select" style="flex:0 1 250px;min-width:120px"><select value=${key} onChange=${(e) => pick(e.currentTarget.value)}>
      ${!line && html`<option value="">–</option>`}
      ${options.map(([k, label]) => html`<option value=${k}>${label}</option>`)}
    </select><${Icon} name="expand_more" size=${18} /></span>
    <button type="button" class="tp-new narrow" title=${tr('New route: click its stops on the map')} onClick=${fresh}><${Icon} name="add" size=${20} /></button>
    <span class="tp-select" style="flex:0 1 200px;min-width:110px" title=${tr('The days the route runs, as its kind of service has them: the Timeplan makes its trips on these days')}>
      <select value=${line?.service || 'regular'} disabled=${!line || !!line.map} onChange=${(e) => le('kind', { kind: e.currentTarget.value })}>
        ${KINDS.map(([k, label]) => html`<option value=${k}>${tr(label)}</option>`)}
      </select><${Icon} name="expand_more" size=${18} /></span>
    ${line && html`<span class="rt-colour-wrap">
      <button type="button" class=${'rt-colour' + (t.colour ? ' on' : '')} style=${`background:${line.colour};color:${inkOn(line.colour)}`} title=${tr("The route's colour")} onClick=${() => rtSet({ colour: !t.colour })}><${Icon} name="palette" size=${16} /></button>
      ${t.colour && html`<${ColourPop} v=${v} />`}
    </span>`}
    <i class="tp-sep"></i>
    <span class="tp-sides rt-sides">${[0, 1].map((k) => {
      const d = dirs[k];
      const label = d?.stops?.length >= 2 ? (isLoop(d) ? destination(d) : tr('To %{stop}', { stop: destination(d) })) : tr(k ? 'Return' : 'Outbound');
      const tip = d?.stops?.length >= 2 ? `${d.stops[0].name} – ${destination(d)}` : undefined;
      return html`<button type="button" class=${(v.dir === k ? 'on' : '') + (line ? '' : ' off')} title=${tip} onClick=${() => line && v.dir !== k && (rtSet({ cell: null, text: '' }), le('dir', { d: k }))}>${label.toUpperCase()}</button>`;
    })}</span>
    <i class="tp-sep"></i>
    <${Saving} v=${v} />
  </div>`;
}

/** The colour's picker: the company's colours and the line editor's, and any colour. */
function ColourPop({ v }) {
  const line = v.line;
  const ref = useRef(null);
  // (a click elsewhere closes it)
  useEffect(() => {
    const away = (e) => !ref.current?.parentElement?.contains(e.target) && rtSet({ colour: false });
    addEventListener('mousedown', away);
    return () => removeEventListener('mousedown', away);
  }, []);
  const now = String(line.colour).toLowerCase();
  return html`<div class="rt-pop" ref=${ref}>
    <div class="rt-swatches">${v.palette.map((hex) => html`<button type="button" class=${'sw' + (now === hex.toLowerCase() ? ' on' : '')} style=${`background:${hex}`} onClick=${() => now !== hex.toLowerCase() && le('colour', { value: hex })}></button>`)}</div>
    <${ColourPicker} value=${line.colour} ink=${inkOn(line.colour)} onChange=${setColour} />
  </div>`;
}

/** Saving the route, as the line editor's foot for the company has it (`company_foot`,
 * `maplines::foot`): a new route kept as a draft, deleted, or confirmed and paid; a route in
 * service saved (its change paid, asked from which day), its changes undone; a line of the map's
 * the company runs saved or put back as the map has it. */
function Saving({ v }) {
  const line = v.line;
  if (!line) return null;
  const act = (label, onClick, { primary = false, why = null, icon = 'save' } = {}) => html`<button type="button" class=${'rt-act' + (primary ? ' primary' : '') + (why ? ' blocked' : '')} title=${why || label} onClick=${() => (why ? say(why, true) : onClick())}><${Icon} name=${icon} size=${17} /><span>${label}</span></button>`;
  const small = (icon, title, onClick) => html`<button type="button" class="tp-ib" title=${title} onClick=${onClick}><${Icon} name=${icon} size=${17} /></button>`;
  if (line.map) {
    const fee = v.company_view?.map_fee;
    const dirty = v.dirty && !v.fresh;
    const why = dirty ? v.map_problems[0] || fee?.why || null : null;
    const label = fee && fee.fee > 0 && dirty ? tr('Pay %{amount} and save', { amount: fee.label }) : tr(dirty ? 'Save line' : 'Saved');
    return html`${!v.fresh && small('restart_alt', tr(v.restore_armed ? "Press again: the map's original back" : "Restore the map's original"), () => paid('restore'))}
      ${act(label, () => dirty && paid('save_map'), { primary: dirty, why })}`;
  }
  const f = v.company_view?.foot;
  if (!f || f.reading) return html`<span class="rt-wait">${f?.reading || ''}</span>`;
  // (a route confirmed: the line editor says the Timeplan comes next in the answer after the one
  // that says the company changed - asked for at once)
  if (!f.confirmed) {
    return html`${small('delete', tr(v.delete_armed ? 'Press again' : 'Delete draft'), () => le('delete').then(() => rtSet({ sel: L.view?.sel ?? null })))}
      ${act(tr(v.dirty ? 'Save draft' : 'Saved'), () => v.dirty && le('save'))}
      ${act(f.confirm, () => paid('c_confirm').then(() => le('view')), { primary: true, why: f.why, icon: 'check_circle' })}`;
  }
  return html`${v.dirty ? small('restart_alt', tr('Undo the changes'), () => le('c_discard')) : f.waiting ? small('restart_alt', tr('Keep the line as it runs'), () => paid('c_withdraw')) : null}
    ${act(f.save_change, () => v.dirty && paid('c_save_change'), { primary: v.dirty, why: v.dirty ? f.why : null })}`;
}

// --- the stops -------------------------------------------------------------------------------------

/** A field that keeps what is typed while it has the focus (the line editor's `LiveField`, in
 * RosterFlow's look). */
function TypedField({ value, placeholder, onInput, cls = '' }) {
  const [text, setText] = useState(value ?? '');
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(value ?? '');
  }, [value]);
  return html`<input class=${'rt-field ' + cls} value=${text} placeholder=${placeholder} spellcheck=${false}
    onFocus=${() => (focused.current = true)} onBlur=${() => ((focused.current = false), setText(value ?? ''))}
    onInput=${(e) => {
      const t = e.currentTarget.value;
      setText(t);
      onInput(t);
    }} />`;
}

/** A stop dragged to `to` in the direction's order: moved one place at a time (`up`, `down`). */
function moveStop(from, to) {
  if (from == null || to == null || from === to) return;
  for (let k = from; k < to; k++) le('down', { i: k });
  for (let k = from; k > to; k--) le('up', { i: k });
}

/** The left panel: the route's number and name, the direction's pattern, its stops in order, and
 * how many they are and how long. */
function StopsPanel({ v }) {
  const line = v.line;
  const [over, setOver] = useState(null);
  const dragged = useRef(null);
  if (!line) {
    return html`<div class="rt-side"><div class="rt-empty"><p>${v.none}</p>
      <button type="button" class="rt-act primary" onClick=${() => le('new_line').then(() => rtSet({ sel: L.view?.sel ?? null }))}><${Icon} name="add" size=${17} /><span>${tr('New route')}</span></button></div></div>`;
  }
  const d = line.directions[v.dir];
  const colour = line.colour;
  const t = v.stops_tab || {};
  const list = v.directions?.list;
  const patterns = list ? v.directions.labels : [`${tr(v.dir ? 'Return' : 'Outbound')}: ${d?.stops?.[0]?.name?.trim() || '–'} – ${destination(d) || '–'}`];
  const head = html`<div class="rt-head">
      <${TypedField} cls="num" value=${line.number} placeholder="42" onInput=${(x) => le('number', { value: x })} />
      <${TypedField} value=${line.name} placeholder=${tr('Name')} onInput=${(x) => le('name', { value: x })} />
    </div>
    <div class="rt-pattern"><span class="tp-select"><select value=${String(list ? v.dir : 0)} disabled=${!list} onChange=${(e) => le('dir', { d: Number(e.currentTarget.value) })}>
      ${patterns.map((label, k) => html`<option value=${String(k)}>${label}</option>`)}
    </select><${Icon} name="expand_more" size=${18} /></span></div>`;
  // (the way back of a route without one: made from the stops across the road)
  if (!d) {
    return html`<div class="rt-side">${head}<div class="rt-empty">
      <p>${tr('This route has no way back yet: without one the bus goes round to its first stop.')}</p>
      <button type="button" class="rt-act primary" onClick=${() => le('reverse')}><${Icon} name="sync_alt" size=${17} /><span>${tr('Reverse: make the way back')}</span></button></div></div>`;
  }
  const n = d.stops.length;
  const metres = (d.legs || []).reduce((a, g) => a + (g.length || 0), 0);
  const why = v.company_view?.foot?.why || (line.map ? v.map_problems[0] : null);
  const at = v.sel_stop != null && d.stops[v.sel_stop];
  return html`<div class="rt-side" data-rf="rf-routes-stops">${head}
    <div class="rt-stops">
      ${!n && html`<p class="rt-note">${tr('Click the stops on the map in the order the bus calls at them.')}</p>`}
      ${d.stops.map((s, i) => {
        const leg = t.legs?.[i];
        const end = i === 0 || i + 1 === n;
        return html`<div class=${'rt-stop' + (v.sel_stop === i ? ' on' : '') + (over === i ? ' over' : '') + (i === 0 ? ' first' : '') + (i + 1 === n ? ' last' : '')} style=${`--c:${colour}`} draggable="true"
            title=${tr('Drag it to another place in the order')}
            onClick=${() => le('sel_stop', { i })}
            onDragStart=${(e) => {
              dragged.current = i;
              e.dataTransfer.effectAllowed = 'move';
              e.dataTransfer.setData('text/plain', String(i));
            }}
            onDragOver=${(e) => {
              e.preventDefault();
              if (over !== i) setOver(i);
            }}
            onDrop=${(e) => {
              e.preventDefault();
              moveStop(dragged.current, i);
              dragged.current = null;
              setOver(null);
            }}
            onDragEnd=${() => ((dragged.current = null), setOver(null))}>
            <span class="rail"><i class=${end ? 'end' : ''}></i></span>
            <span class="name">${s.name}</span>
            <span class="code">${s.id}</span>
            <span class="tools" onClick=${(e) => e.stopPropagation()}>
              <button type="button" disabled=${i === 0} title=${tr('Earlier in the line')} onClick=${() => le('up', { i })}><${Icon} name="keyboard_arrow_up" size=${15} /></button>
              <button type="button" disabled=${i + 1 === n} title=${tr('Later in the line')} onClick=${() => le('down', { i })}><${Icon} name="keyboard_arrow_down" size=${15} /></button>
              <button type="button" title=${tr('Take the stop out')} onClick=${() => le('remove', { i })}><${Icon} name="close" size=${15} /></button>
            </span>
          </div>
          ${leg && !leg.ok && html`<div class="rt-leg">${leg.text}</div>`}`;
      })}
    </div>
    <div class="rt-foot">
      <div class="rt-sum"><span>${tr('%{n} stops', { n })}  ·  ${distance(metres)}</span>
        <span class="rt-tools">
          <button type="button" class="tp-ib" title=${tr('Reverse: make the way back')} onClick=${() => le('reverse')}><${Icon} name="sync_alt" size=${16} /></button>
          ${n >= 2 && html`<button type="button" class="tp-ib" title=${tr(isLoop(d) ? 'Open the loop' : 'Close the loop: back to the first stop')} onClick=${() => le('loop')}><${Icon} name=${isLoop(d) ? 'close' : 'autorenew'} size=${16} /></button>`}
        </span></div>
      ${at && html`<p class="rt-note">${tr('New stops go in after %{stop}: click it again to add them at the end.', { stop: at.name })}</p>`}
      ${why && html`<p class="rt-note warn">${why}</p>`}
    </div>
  </div>`;
}

// --- the map ---------------------------------------------------------------------------------------

/** The line editor's map with the route's START and END over it, the stop search, the camera on
 * the route, and the company's estimate. */
function MapPane({ v }) {
  const box = useRef(null);
  const start = useRef(null);
  const end = useRef(null);
  const soon = useRef(0);
  // (the pins where the camera puts the direction's ends: the camera moves inside the map, so they
  // are placed after every move of it)
  const place = () => {
    const el = box.current;
    const c = L.cam;
    const d = L.view?.line?.directions?.[L.view.dir];
    const pts = d?.stops_at || [];
    const loop = isLoop(d);
    const put = (pin, at) => {
      if (!pin) return;
      if (!at || !c || !el) {
        pin.style.display = 'none';
        return;
      }
      const [W, H] = [el.clientWidth || 1, el.clientHeight || 1];
      const x = W / 2 + (at[0] - c.cx) / c.mpp;
      const y = H / 2 - (at[1] - c.cy) / c.mpp;
      pin.style.display = x < -60 || y < -60 || x > W + 60 || y > H + 60 ? 'none' : '';
      pin.style.transform = `translate(${x}px, ${y}px)`;
    };
    put(start.current, pts[0]);
    put(end.current, pts.length > 1 && !loop ? pts[pts.length - 1] : null);
  };
  const schedule = () => {
    cancelAnimationFrame(soon.current);
    soon.current = requestAnimationFrame(place);
  };
  useEffect(schedule);
  useEffect(() => {
    const ro = new ResizeObserver(schedule);
    ro.observe(box.current);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(soon.current);
    };
  }, []);
  // (the camera on a route once, when it is first shown with the map read)
  useEffect(() => {
    if (!L.map || !v.line) return;
    const key = `${L.mapKey}|${v.sel}`;
    if (RT.fitted !== key && fitRoute(box.current)) RT.fitted = key;
  });
  const loop = isLoop(v.line?.directions?.[v.dir]);
  return html`<div class="rt-map" ref=${box} onPointerMove=${schedule} onPointerUp=${schedule} onWheel=${schedule}>
    <${LineMap} />
    <span class="rt-pin start" ref=${start}><b>${loop ? tr('START · END') : tr('START')}</b></span>
    <span class="rt-pin end" ref=${end}><b>${tr('END')}</b></span>
    ${L.map && html`<${StopSearch} v=${v} />`}
    ${L.map && v.line && html`<button type="button" class="rt-fit" title=${tr('The whole route')} onClick=${() => ((RT.fitted = null), fitRoute(box.current), schedule())}><${Icon} name="my_location" size=${17} /></button>`}
    <${EstimateCard} />
  </div>`;
}

/** The search for a stop of the map by its name: shown on the map, or added to the direction. */
function StopSearch({ v }) {
  const t = rt();
  const q = String(t.search || '').trim().toLowerCase();
  const stops = L.map?.stops || [];
  const on = new Set((v.line_stops?.[v.dir] || []).filter((x) => x != null));
  const found = q.length >= 2 ? stops.map((s, i) => [s, i]).filter(([s]) => s.name.toLowerCase().includes(q)).slice(0, 8) : [];
  const canAdd = !!v.line?.directions?.[v.dir];
  return html`<div class="rt-search" onPointerDown=${(e) => e.stopPropagation()} onWheel=${(e) => e.stopPropagation()}>
    <label class="field"><${Icon} name="search" size=${17} />
      <input value=${t.search || ''} placeholder=${tr('Search a stop')} spellcheck=${false} onInput=${(e) => rtSet({ search: e.currentTarget.value })}
        onKeyDown=${(e) => e.key === 'Enter' && found[0] && lookAt(found[0][1])} />
      ${t.search && html`<button type="button" class="clear" title=${tr('Clear')} onClick=${() => rtSet({ search: '' })}><${Icon} name="close" size=${14} /></button>`}
    </label>
    ${q.length >= 2 && html`<div class="list">
      ${!found.length && html`<p>${tr('No stop of that name')}</p>`}
      ${found.map(([s, i]) => html`<div class="hit">
        <button type="button" class="go" title=${tr('Show it on the map')} onClick=${() => lookAt(i)}>${s.name}</button>
        ${on.has(i)
          ? html`<span class="in" title=${tr('On the route')}><${Icon} name="check" size=${15} /></span>`
          : canAdd && html`<button type="button" class="add" title=${tr('Add it to the direction')} onClick=${() => (lookAt(i), le('click_stop', { i }))}><${Icon} name="add" size=${15} /></button>`}
      </div>`)}
    </div>`}
  </div>`;
}

// --- the trip-time strip ---------------------------------------------------------------------------

/** The direction's stops along the foot with their codes, the minutes of each segment under them
 * (clicked: typed anew, `shift`) and the trip's time - from the roads (the length of each leg at
 * the usual speed) until one is typed. */
function TripStrip({ v }) {
  const line = v.line;
  const d = line?.directions?.[v.dir];
  const t = rt();
  const input = useRef(null);
  useEffect(() => {
    if (t.cell != null) input.current?.select();
  }, [t.cell]);
  const n = d?.stops?.length || 0;
  if (n < 2) return html`<div class="rt-strip"><p class="rt-strip-empty">${tr('Two stops at least: then the minutes from stop to stop are here.')}</p></div>`;
  const seg = segments(d);
  const total = seg.reduce((a, m) => a + m, 0);
  const colour = line.colour;
  const commit = () => {
    const k = S.rf.rt?.cell;
    const m = parseFloat(String(S.rf.rt?.text ?? '').replace(',', '.'));
    rtSet({ cell: null, text: '' });
    if (k == null || !(m >= 0 && m <= 600) || Math.round(m) === seg[k]) return;
    le('shift', { i: k + 1, by: Math.round(m) - seg[k] });
  };
  return html`<div class="rt-strip" data-rf="rf-routes-strip">
    <div class="rt-strip-scroll">
      <div class="rt-strip-row" style=${`width:${n * COL}px`}>
        ${d.stops.map((s, j) => html`<div class="col" style=${`left:${j * COL}px;width:${COL}px`}><span class="name" title=${s.name}>${s.name}</span><b class="code">${s.id}</b></div>`)}
        <i class="rail" style=${`left:${COL / 2}px;width:${(n - 1) * COL}px;background:${colour}`}></i>
        ${d.stops.map((s, j) => html`<span class=${'dot' + (j === 0 || j + 1 === n ? ' end' : '')} style=${`left:${j * COL + COL / 2}px;--c:${colour}`}><i></i></span>`)}
        ${seg.map((m, k) =>
          t.cell === k
            ? html`<input class="seg on" ref=${input} style=${`left:${(k + 1) * COL}px`} value=${t.text} inputmode="numeric"
                onInput=${(e) => rtSet({ text: e.currentTarget.value })} onBlur=${commit}
                onKeyDown=${(e) => {
                  if (e.key === 'Enter') commit();
                  else if (e.key === 'Escape') (e.stopPropagation(), rtSet({ cell: null, text: '' }));
                }} />`
            : html`<button type="button" class="seg" style=${`left:${(k + 1) * COL}px`} title=${tr('The minutes from %{a} to %{b}: click to change them', { a: d.stops[k].name, b: d.stops[k + 1].name })}
                onClick=${() => rtSet({ cell: k, text: String(m) })}>${tr('%{n} min', { n: m })}</button>`,
        )}
      </div>
    </div>
    <div class="rt-total">
      <span class="cap">${tr('Trip time').toUpperCase()}</span>
      <b>= ${tr('%{n} min', { n: total })}</b>
      <span class="src">${tr(d.manual_times ? 'Typed in: the Timeplan starts its running times from these' : 'From the roads: the Timeplan starts its running times from these')}</span>
      ${d.manual_times && html`<button type="button" class="rt-link" onClick=${() => le('times_auto')}><${Icon} name="schedule" size=${14} /><span>${tr('Times from the roads')}</span></button>`}
    </div>
  </div>`;
}

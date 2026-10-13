// The depot editor (`crates/omsi-launcher-ui/src/depoteditor.rs`): depot files (`.hof`) of the
// player's own, so that the destination displays are no puzzle. A file is made new - empty,
// laid out as OMSI's own, or as a copy of the map's depot file, of one of the chosen bus's, or
// of one of the player's - for a map, and kept in `~/.openomsi/depots`, never in the OMSI 2
// folder. Its parts are tables: the destinations (code, name, the text for each display, an
// "everybody out" service trip, a special trip offered for any bus), the IBIS's stops, the
// routes with their stops, the special and service trips, and the file itself. The chosen
// destination is shown lit as the front matrix shows it, and what is wrong with the file is said
// plainly under the tabs.
//
// The file open and its changes are Rust's (`de_*`, `omsihub/editors/depots.rs`).

import { html, useEffect } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { curBus } from '../../data.js';
import { Seg } from '../bits.js';
import { Card, Heading, Row, Field, Drop, IconBtn, Btn, Toggle, Icon, tr, status, keep, mapLabel } from './kit.js';
import { LedSign, IbisLcd, SideSign, useBusFont } from './signs.js';

// (a name, the same in every language)
const IBIS = 'IBIS';
const TABS = ['Destinations', 'IBIS stops', 'Routes', 'Special trips', 'File'];
/** The usual special and service trips (`owndepot::SPECIALS`): OMSI's own German names. */
const SPECIALS = [['Betriebsfahrt', true], ['Leerfahrt', true], ['Dienstfahrt', true], ['Nicht einsteigen', true], ['Sonderfahrt', false], ['Schulbus', false]];

/** What the page keeps (`DepotEditorView`). */
const V = keep('depots', { entries: [], view: null, read: false, tab: 0, sel: 0, selStop: 0, selTrip: 0, filter: '', code: [-1, ''], form: null, deleteArmed: false, white: false, line: '', addStop: 0, fromLines: false });

const redraw = () => set({});

/** An error of the depot editor's commands in the interface's language: "text|argument" for the
 * two that name something. */
function say(e) {
  const m = String(e?.message || e);
  const [text, a] = m.split('|');
  if (a == null) return m;
  return tr(text, { name: a, hof: a });
}

const folderOf = (file) => String(file || '').replace(/\\/g, '/').replace(/\/global\.cfg$/i, '').split('/').pop() || '';

/** The friendly name of the map whose folder is `folder` (`map_name`). */
function mapName(folder) {
  if (!String(folder || '').trim()) return tr('any map');
  const m = (S.maps || []).find((x) => folderOf(x.file).toLowerCase() === folder.toLowerCase());
  return m ? mapLabel(m) : folder;
}

/** The page shown: the files read, the one asked for opened (`draw`'s first frame). */
async function show(key) {
  const maps = (S.maps || []).map((m) => [folderOf(m.file), m.hof || '']);
  try {
    const r = await call('de_show', { key: key || null, maps });
    Object.assign(V, { entries: r.entries, read: true });
    take(r.view);
    if (r.changed_elsewhere) status(tr('%{name} was changed elsewhere since you opened it: saving writes your changes over that', { name: r.changed_elsewhere }), true);
    if (r.error) status(say(r.error), true);
  } catch (e) {
    status(say(e), true);
  }
  redraw();
}

/** A new view of the file open: the rows chosen kept in it. */
function take(view, sel) {
  const was = V.view?.key;
  V.view = view;
  if (view && view.key !== was) Object.assign(V, { sel: 0, selStop: 0, selTrip: 0, deleteArmed: false, code: [-1, ''] });
  if (sel != null) {
    if (V.tab === 1) V.selStop = sel;
    else if (V.tab === 2) V.selTrip = sel;
    else V.sel = sel;
  }
}

/** A change of the file open (`de_edit`). */
async function edit(op, more = {}) {
  try {
    const r = await call('de_edit', { op, ...more });
    take(r.view, r.sel);
  } catch (e) {
    status(say(e), true);
  }
  redraw();
}

async function openKey(key) {
  try {
    take(await call('de_open', { key }));
  } catch (e) {
    status(say(e), true);
  }
  redraw();
}

async function save() {
  try {
    const r = await call('de_save');
    take(r.view);
    V.entries = r.entries;
    const text = r.copies > 0 ? tr('%{name} saved; the copies beside %{n} bus folder(s) too', { n: r.copies }) : tr('%{name} saved');
    status(text.replace('%{name}', r.name));
    S.ed.ownDepotsChanged = Date.now();
  } catch (e) {
    status(`${tr('Not saved')}: ${say(e)}`, true);
  }
  redraw();
}

async function remove() {
  try {
    const r = await call('de_delete');
    V.view = null;
    V.entries = r.entries;
    status(tr('%{name} deleted (kept aside in the depots folder, under deleted)', { name: r.name }));
    S.ed.ownDepotsChanged = Date.now();
  } catch (e) {
    status(say(e), true);
  }
  V.deleteArmed = false;
  redraw();
}

// --- the list of files -----------------------------------------------------------------------

function NewForm() {
  const f = V.form;
  const maps = S.maps || [];
  const m = maps[Math.min(f.map, maps.length - 1)];
  useEffect(() => {
    if (!m || f.sourcesFor === m.file) return;
    f.sourcesFor = m.file;
    const bus = curBus() || (S.vehicles || []).find((v) => v.file === S.choice.last_bus);
    call('de_sources', { hof: m.hof || '', bus: bus?.file || '', bus_name: bus?.name || '' }).then((list) => {
      // (the source chosen kept by what it is, not by its place; else the map's, else an empty one)
      const had = f.sources?.[f.from]?.source;
      const same = (a, b) => a && b && a.kind === b.kind && (a.path || '') === (b.path || '');
      let at = had ? list.findIndex((x) => same(x.source, had)) : -1;
      if (at < 0) at = Math.max(0, list.findIndex((x) => x.source.kind === 'map'));
      Object.assign(f, { sources: list, from: at });
      redraw();
    });
  });
  const hint = m ? tr('%{map} - mine', { map: mapLabel(m) }) : '';
  const make = async () => {
    if (!m) return;
    const name = f.name.trim() || hint;
    try {
      const r = await call('de_make', { name, map: m.file, hof: m.hof || '', source: f.sources?.[f.from]?.source || { kind: 'empty' } });
      V.form = null;
      V.entries = r.entries;
      take(r.view);
      status(tr('Depot file %{name} made', { name }));
      S.ed.ownDepotsChanged = Date.now();
    } catch (e) {
      status(say(e), true);
    }
    redraw();
  };
  return html`<div class="de-form">
    <span class="ed-caps">${tr('Name')}</span>
    <${Field} value=${f.name} raw placeholder=${hint} onInput=${(v) => ((f.name = v), redraw())} onEnter=${make} />
    <span class="ed-caps">${tr('For the map')}</span>
    ${maps.length > 0 && html`<${Drop} raw value=${f.map} options=${maps.map((x, k) => [k, mapLabel(x)])} onChange=${(k) => ((f.map = k), redraw())} />`}
    <span class="ed-caps">${tr('Start from')}</span>
    ${f.sources?.length > 0 && html`<${Drop} raw value=${f.from} options=${f.sources.map((x, k) => [k, tr(x.text, x.args)])} onChange=${(k) => ((f.from = k), redraw())} />`}
    <div class="two"><${Btn} kind="primary" icon="check" label="Make it" onClick=${make} /><${Btn} label="Cancel" onClick=${() => ((V.form = null), redraw())} /></div>
  </div>`;
}

function Files() {
  const open = V.view;
  const newFile = () => {
    const maps = S.maps || [];
    const map = Math.max(0, maps.findIndex((m) => m.file === S.choice.map));
    V.form = { name: '', map, from: 0, sources: null, sourcesFor: null };
    redraw();
  };
  return html`<${Card} class="de-files">
    <${Heading} title="Your depot files" />
    <div class="list">
      ${!V.entries.length && html`<p class="ed-para">${tr('None yet: make one with the button below.')}</p>`}
      ${V.entries.map((e) => {
        const on = open && open.key.toLowerCase() === e.key.toLowerCase();
        return html`<${Row} class="file" on=${on} onClick=${() => openKey(e.key)}>
          <span class="two-lines"><b>${e.name}</b><small>${tr('%{map} · %{n} destinations', { map: mapName(e.map), n: e.termini })}</small></span>
          ${on && open.dirty && html`<i class="dirty"></i>`}
        </${Row}>`;
      })}
    </div>
    ${V.form ? html`<${NewForm} />` : html`<${Btn} kind="primary" icon="add" label="New depot file" onClick=${newFile} />`}
  </${Card}>`;
}

// --- the file open ---------------------------------------------------------------------------

/** What text `k` of a destination is called (`lineeditor::text_title`), or the file's note. */
function labelOf(l, k) {
  if (l?.note) return l.note.slice(0, 48);
  switch (l?.role) {
    case 'ibis': return tr(IBIS);
    case 'front_top': return tr('Front sign, top row');
    case 'front_bottom': return tr('Front sign, bottom row');
    case 'side': return tr('Side sign');
    case 'clear_name': return tr('Passenger display');
    case 'roller_blind': return tr('Roller blind picture');
    case 'extra_sign': return tr('Plug-in sign');
    case 'bitmap': return tr('Picture on LED signs');
    case 'special': return tr('Special trip mark');
    case 'rear': return tr('Rear sign');
    case 'fallback': return tr('For some buses only');
    default: return tr('Text %{n}', { n: k + 1 });
  }
}

/** A problem in the interface's language. */
const issueText = (i) => tr(i.text, Object.fromEntries(i.args));
const issuesAt = (v, kind, i) => v.issues.filter((x) => x.at[0] === kind && x.at[1] === i);

function Head2({ title, children }) {
  return html`<div class="de-head"><span class="ed-caps">${tr(title)}</span>${children}</div>`;
}

function Destinations({ v }) {
  const roles = v.labels.map((l) => l.role);
  const first = (r) => roles.indexOf(r);
  const [top, bottom] = [first('front_top'), first('front_bottom')];
  const ibis = first('ibis') >= 0 ? first('ibis') : first('clear_name') >= 0 ? first('clear_name') : 0;
  const filter = V.filter.trim().toLowerCase();
  const rows = v.termini.map((t, i) => [t, i]).filter(([t]) => !filter || t.name.toLowerCase().includes(filter) || String(t.code) === filter);
  const add = () => ((V.filter = ''), edit('t_add', { value: tr('New destination') }));
  return html`<div class="de-split">
    <div class="de-table">
      <div class="de-bar"><${Field} icon="search" value=${V.filter} placeholder="Find a destination or code" onInput=${(x) => ((V.filter = x), redraw())} /><${Btn} icon="add" label="Add" onClick=${add} /></div>
      <div class="de-cols t"><span class="ed-caps r">${tr('Code')}</span><span class="ed-caps">${tr('Destination')}</span><span class="ed-caps">${tr('Front')}</span></div>
      <div class="de-rows">
        ${rows.map(([t, i]) => {
          const front = [top, bottom].filter((k) => k >= 0).map((k) => (t.strings[k] || '').trim()).filter(Boolean).join(' / ') || (t.strings[ibis] || '').trim();
          const iss = issuesAt(v, 'terminus', i);
          return html`<${Row} class="t" on=${V.sel === i} onClick=${() => ((V.sel = i), (V.deleteArmed = false), redraw())}>
            <span class="code">${t.code}</span><span class="name">${t.name.trim() || '-'}</span><span class="front">${front}</span>
            <span class="marks">${t.special && html`<span class="flag"><${Icon} name="flag" size=${13} /></span>`}${iss.length > 0 && html`<span class=${iss.some((x) => x.serious) ? 'bad' : 'warn'}><${Icon} name="warning" size=${13} /></span>`}</span>
          </${Row}>`;
        })}
        ${!rows.length && html`<p class="ed-para">${tr(v.termini.length ? 'None matches.' : 'No destinations yet: add one.')}</p>`}
      </div>
    </div>
    ${v.termini.length > 0 && html`<${DestinationDetail} v=${v} roles=${roles} />`}
  </div>`;
}

function DestinationDetail({ v, roles }) {
  const n = v.termini.length;
  V.sel = Math.min(V.sel, n - 1);
  const i = V.sel;
  const t = v.termini[i];
  const first = (r) => roles.indexOf(r);
  const get = (r) => (first(r) >= 0 ? (t.strings[first(r)] || '').trim() : '');
  let [l1, l2] = first('front_top') >= 0 ? [get('front_top'), get('front_bottom')] : first('ibis') >= 0 ? [get('ibis'), ''] : [(t.strings[0] || '').trim(), ''];
  // (the buses move a lone bottom row up)
  if (!l1) [l1, l2] = [l2, ''];
  const ibisText = get('ibis') || get('clear_name');
  const side = get('side');
  const bus = curBus() || (S.vehicles || []).find((x) => x.file === S.choice.last_bus);
  const signText = [l1, l2].filter(Boolean).join(' ');
  const p = useBusFont(bus?.file, signText);
  if (V.code[0] !== i) V.code = [i, String(t.code)];
  const issues = issuesAt(v, 'terminus', i);
  return html`<div class="de-detail">
    <${Head2} title="On the front display"><${Field} class="line" value=${V.line} placeholder="Line" onInput=${(x) => ((V.line = x), redraw())} /></${Head2}>
    <div class="de-sign"><${LedSign} line=${V.line} l1=${l1} l2=${l2} white=${V.white} onClick=${() => ((V.white = !V.white), redraw())} title="Click: amber or white" /></div>
    <div class="de-two"><${IbisLcd} text=${ibisText} title="The IBIS" /><${SideSign} text=${side} white=${V.white} title="The side display" /></div>
    ${bus && html`<p class="de-onbus">${tr('On %{bus}, in its own display font', { bus: bus.name })}</p>
      <div class="de-busfont">${!p ? html`<span>${tr("Reading the bus's displays…")}</span>` : p.failed ? html`<span>${tr("The bus's displays could not be read.")}</span>` : p.none ? html`<span>${tr('It draws its displays as pictures of its own: no preview in a font')}</span>` : p.picture ? html`<img src=${p.picture} />` : html`<span>${tr('Drawing the sign…')}</span>`}</div>
      ${p?.too_wide && html`<p class="ed-warn de-small">${tr('Wider than the display of %{bus} in its font %{font}: the end is cut off', { bus: bus.name, font: p.font })}</p>`}`}
    <${Head2} title="Destination" />
    <div class="de-namecode">
      <${Field} raw value=${t.name} placeholder=${tr('What the timetable\'s trips name')} onInput=${(x) => edit('t_name', { i, value: x })} />
      <${Field} raw value=${V.code[1]} placeholder=${tr('Code')} onInput=${(x) => ((V.code = [i, x]), edit('t_code', { i, value: x }))} />
    </div>
    <${Toggle} checked=${t.all_exit} label="Nobody boards: a service trip (everybody gets off)" onChange=${(on) => edit('t_all_exit', { i, on })} />
    ${!t.all_exit && html`<${Toggle} checked=${t.marked} label="Offered for any bus: a special trip" onChange=${(on) => edit('t_special', { i, on })} />`}
    <${Head2} title="Texts" />
    ${t.strings.map((s, k) => {
      const l = v.labels[k];
      const count = s.trimEnd().length;
      return html`<div class="de-text"><span>${labelOf(l, k)}</span>${l?.limit != null && html`<b class=${count > l.limit ? 'over' : ''}>${count}/${l.limit}</b>`}</div>
        <${Field} raw class="short" value=${s} placeholder="-" onInput=${(x) => edit('t_string', { i, k, value: x })} />`;
    })}
    ${issues.map((x) => html`<p class=${'de-issue' + (x.serious ? ' bad' : '')}>${issueText(x)}</p>`)}
    <div class="two"><${Btn} icon="content_copy" label="Duplicate" onClick=${() => edit('t_dup', { i })} /><${Btn} kind="danger" icon="delete" label="Delete" onClick=${() => edit('t_del', { i })} /></div>
  </div>`;
}

function Stops({ v }) {
  const n = v.stops.length;
  return html`<div class="de-split">
    <div class="de-table">
      <div class="de-bar"><p class="ed-para">${tr('The stops the IBIS names along a route: what it shows for each.')}</p><${Btn} icon="add" label="Add" onClick=${() => edit('s_add', { value: tr('New stop') })} /></div>
      <div class="de-cols s"><span class="ed-caps">${tr('Stop')}</span><span class="ed-caps">${tr('On the IBIS')}</span></div>
      <div class="de-rows">
        ${v.stops.map((b, k) => html`<${Row} class="s" on=${V.selStop === k} onClick=${() => ((V.selStop = k), redraw())}>
          <span class="name">${b.ident}</span><span class="front">${b.strings[0] || ''}</span>
          <span class="marks">${issuesAt(v, 'stop', k).length > 0 && html`<span class="warn"><${Icon} name="warning" size=${13} /></span>`}</span>
        </${Row}>`)}
      </div>
    </div>
    ${n > 0 && (() => {
      const k = (V.selStop = Math.min(V.selStop, n - 1));
      const b = v.stops[k];
      return html`<div class="de-detail">
        <${Head2} title="Stop" />
        <${Field} raw value=${b.ident} placeholder=${tr('What the routes name it')} onInput=${(x) => edit('s_ident', { i: k, value: x })} />
        <${Head2} title="Texts" />
        ${b.strings.map((s, j) => html`<div class="de-text"><span>${(v.stop_notes[j] || '').trim() || (j === 0 ? tr('IBIS display') : tr('Text %{n}', { n: j + 1 }))}</span></div>
          <${Field} raw class="short" value=${s} placeholder="-" onInput=${(x) => edit('s_string', { i: k, k: j, value: x })} />`)}
        ${issuesAt(v, 'stop', k).map((x) => html`<p class=${'de-issue' + (x.serious ? ' bad' : '')}>${issueText(x)}</p>`)}
        <${Btn} kind="danger" icon="delete" label="Delete" style="width:160px;margin-top:6px" onClick=${() => edit('s_del', { i: k })} />
      </div>`;
    })()}
  </div>`;
}

function Routes({ v }) {
  const n = v.trips.length;
  const to = (t) => {
    const code = parseInt(t.route, 10);
    const x = t.route.trim() ? v.termini.find((d) => d.code === code) : null;
    return x ? x.name : `? ${t.route.trim()}`;
  };
  return html`<div class="de-split">
    <div class="de-table">
      <div class="de-bar"><p class="ed-para">${tr('What the IBIS types: a route\'s code, where it goes and its stops.')}</p><${Btn} icon="add" label="Add" onClick=${() => edit('r_add', { value: tr('New route') })} /></div>
      <div class="de-cols r"><span class="ed-caps">${tr('Code')}</span><span class="ed-caps">${tr('Line')}</span><span class="ed-caps">${tr('Goes to')}</span></div>
      <div class="de-rows">
        ${v.trips.map((t, k) => {
          const iss = issuesAt(v, 'trip', k);
          return html`<${Row} class="r" on=${V.selTrip === k} onClick=${() => ((V.selTrip = k), redraw())}>
            <span class="code l">${t.code}</span><span class="ln">${t.line}</span><span class="front">${to(t)} · ${tr('%{n} stops', { n: t.stops.length })}</span>
            <span class="marks">${iss.length > 0 && html`<span class=${iss.some((x) => x.serious) ? 'bad' : 'warn'}><${Icon} name="warning" size=${13} /></span>`}</span>
          </${Row}>`;
        })}
      </div>
    </div>
    ${n > 0 && html`<${RouteDetail} v=${v} />`}
  </div>`;
}

function RouteDetail({ v }) {
  const k = (V.selTrip = Math.min(V.selTrip, v.trips.length - 1));
  const t = v.trips[k];
  const code = parseInt(t.route, 10);
  const options = v.termini.map((d) => [String(d.code), `${d.code} · ${d.name}`]);
  const known = t.route.trim() && v.termini.some((d) => d.code === code);
  if (!known) options.push(['?', tr('? %{code}: not in the file', { code: t.route.trim() })]);
  const names = v.stops.map((b) => b.ident);
  V.addStop = Math.min(V.addStop, Math.max(0, names.length - 1));
  return html`<div class="de-detail route">
    <div class="de-three"><span class="ed-caps">${tr('Code')}</span><span class="ed-caps">${tr('Line')}</span><span class="ed-caps">${tr('Name')}</span>
      <${Field} raw value=${t.code} placeholder=${tr('Code')} onInput=${(x) => edit('r_code', { i: k, value: x })} />
      <${Field} raw value=${t.line} placeholder=${tr('Line')} onInput=${(x) => edit('r_line', { i: k, value: x })} />
      <${Field} raw value=${t.name} placeholder=${tr('Name')} onInput=${(x) => edit('r_name', { i: k, value: x })} /></div>
    ${options.length > 0 && html`<${Head2} title="Goes to" /><${Drop} raw value=${known ? String(code) : '?'} options=${options} onChange=${(c) => c !== '?' && edit('r_route', { i: k, value: c })} />`}
    <${Head2} title="Its stops" />
    <div class="de-stops">
      ${t.stops.map((s, j) => html`<div class="de-stop"><b>${j + 1}.</b><span>${s}</span>
        ${j > 0 ? html`<${IconBtn} icon="keyboard_arrow_up" size=${10} title="Earlier" onClick=${() => edit('r_stop_move', { i: k, k: j, to: j - 1 })} />` : html`<i></i>`}
        ${j + 1 < t.stops.length ? html`<${IconBtn} icon="keyboard_arrow_down" size=${10} title="Later" onClick=${() => edit('r_stop_move', { i: k, k: j, to: j + 1 })} />` : html`<i></i>`}
        <${IconBtn} icon="close" size=${10} title="Take it out" onClick=${() => edit('r_stop_del', { i: k, k: j })} /></div>`)}
      ${!t.stops.length && html`<p class="ed-para">${tr('No stops yet.')}</p>`}
    </div>
    ${names.length > 0
      ? html`<div class="de-addstop"><${Drop} raw value=${V.addStop} options=${names.map((x, j) => [j, x])} onChange=${(j) => ((V.addStop = j), redraw())} /><${Btn} icon="add" label="Add stop" onClick=${() => edit('r_stop_add', { i: k, value: names[V.addStop] })} /></div>`
      : html`<p class="ed-para de-addstop">${tr('Add the IBIS\'s stops first (IBIS stops).')}</p>`}
    <${Btn} kind="danger" icon="delete" label="Delete route" style="width:160px;height:30px" onClick=${() => edit('r_del', { i: k })} />
  </div>`;
}

function Specials({ v }) {
  const service = v.termini.findIndex((t) => t.name.trim().toLowerCase() === v.service_trip.trim().toLowerCase() && v.service_trip.trim());
  const options = [[-1, tr('None')], ...v.termini.map((t, i) => [i, `${t.code} · ${t.name}`])];
  const list = v.termini.map((t, i) => [t, i]).filter(([t]) => t.special);
  const has = (name) => v.termini.some((t) => t.name.trim().toLowerCase() === name.toLowerCase());
  return html`<div class="de-specials">
    <p class="ed-para soft">${tr('Special and service trips are offered for any bus: choose this depot file for the bus on the Drive page (Depot file) and type the code into the IBIS. Where nobody boards, everybody gets off at the first stop.')}</p>
    <${Head2} title="The service trip" />
    <div class="de-service"><${Drop} raw value=${service} options=${options} onChange=${(i) => edit('service', { value: i < 0 ? '' : v.termini[i].name })} /><span>${tr('What a bus shows driving to and from the depot')}</span></div>
    <div class="de-scroll">
      <${Head2} title="In this file" />
      ${list.map(([t, i]) => html`<${Row} class="sp" onClick=${() => Object.assign(V, { tab: 0, sel: i, filter: '' }) && redraw()}>
        <${Icon} name=${t.all_exit ? 'logout' : 'flag'} size=${15} /><span class="code">${t.code}</span><span class="name">${t.name}</span><span class="front">${tr(t.all_exit ? 'nobody boards' : 'special trip')}</span>
      </${Row}>`)}
      ${!list.length && html`<p class="ed-para">${tr('None yet.')}</p>`}
      <${Head2} title="Add the usual ones" />
      <div class="de-usual">${SPECIALS.filter(([name]) => !has(name)).map(([name, allExit]) => html`<${Btn} icon="add" raw label=${name} onClick=${() => edit('usual', { value: name, on: allExit })} />`)}</div>
    </div>
  </div>`;
}

function FileTab({ v }) {
  const n = v.string_count;
  return html`<div class="de-filetab">
    <${Head2} title="Texts of a destination" />
    <div class="de-count"><span>${tr('%{n} texts, one for each display: what each is for', { n })}</span>
      <${IconBtn} icon="remove" size=${12} title="One text less (the last of every destination goes)" onClick=${() => n > 1 && edit('count', { k: n - 1 })} />
      <${IconBtn} icon="add" size=${12} title="One text more" onClick=${() => edit('count', { k: n + 1 })} /></div>
    <p class="ed-para">${tr('A bus\'s scripts read the texts by their place: keep the order the map\'s buses expect (OMSI\'s own: IBIS, front line 1 and 2, side, roller blind, IBIS 2, extra sign, picture).')}</p>
    ${v.notes.map((x, k) => html`<div class="de-note"><b>${tr('Text %{n}', { n: k + 1 })}</b><${Field} raw class="short" value=${x} placeholder=${tr('What it is for')} onInput=${(t) => edit('note', { k, value: t })} /></div>`)}
    <${Head2} title="Global texts" />
    <p class="ed-para">${tr('Folders of the announcements, the roller blinds and the side signs, and the IBIS\'s special codes, as the bus\'s scripts read them.')}</p>
    ${v.global_strings.map((x, k) => html`<div class="de-note g"><b>${k}</b><${Field} raw class="short" value=${x} placeholder="-" onInput=${(t) => edit('global', { k, value: t })} /></div>`)}
    <${Btn} kind="ghost" icon="add" label="One more" style="width:170px;height:30px" onClick=${() => edit('global_add')} />
    <${Head2} title="From your lines" />
    ${!v.blocks.length && html`<p class="ed-para">${tr('No line has chosen this depot file yet: in the line editor, under Displays.')}</p>`}
    ${v.blocks.map((b) => html`<p class="ed-para soft">${tr('Map %{map}: %{d} destination(s), %{s} stop(s), routes %{r} - written when a line is saved; change them in the line editor', { map: b.map, d: b.d, s: b.s, r: b.routes.join(', ') })}</p>`)}
  </div>`;
}

/** The problems of the file under the tabs, the game's first; a click shows the one it is about. */
function Problems({ v }) {
  const issues = v.issues;
  const serious = issues.filter((i) => i.serious).length;
  const [head, kind, icon] = !issues.length
    ? [tr('No problems: the game reads it as it is meant'), 'ok', 'check_circle']
    : serious > 0
      ? [tr('%{n} problem(s) the game trips over, %{m} worth knowing', { n: serious, m: issues.length - serious }), 'bad', 'error']
      : [tr('%{m} thing(s) worth knowing', { m: issues.length }), 'warn', 'warning'];
  const go = ([kind, i]) => {
    if (kind === 'terminus') Object.assign(V, { tab: 0, sel: i, filter: '' });
    else if (kind === 'stop') Object.assign(V, { tab: 1, selStop: i });
    else if (kind === 'trip') Object.assign(V, { tab: 2, selTrip: i });
    else V.tab = 4;
    redraw();
  };
  return html`<div class="de-problems">
    <div class=${'head ' + kind}><${Icon} name=${icon} size=${16} /><b>${head}</b></div>
    <div class="list">${issues.map((x) => html`<${Row} class="p" onClick=${() => go(x.at)}><i class=${x.serious ? 'bad' : 'warn'}></i><span>${issueText(x)}</span></${Row}>`)}</div>
  </div>`;
}

function OpenFile() {
  const v = V.view;
  const maps = (S.maps || []).map((m) => [folderOf(m.file), mapLabel(m)]);
  const mapAt = maps.findIndex((m) => m[0].toLowerCase() === v.map.toLowerCase());
  const options = [...maps.map((m, k) => [k, m[1]]), [-1, tr('Any map')]];
  const Tab = [Destinations, Stops, Routes, Specials, FileTab][V.tab] || FileTab;
  return html`<${Card} class="de-open">
    <div class="de-top">
      <${Field} icon="departure_board" raw value=${v.name} placeholder=${tr("The depot file's name")} onInput=${(x) => edit('name', { value: x })} style="width:300px;flex:none" />
      <${Drop} raw value=${mapAt} options=${options} onChange=${(k) => edit('map', { value: k < 0 ? '' : maps[k][0] })} style="width:230px;flex:none" />
      <span class=${'state' + (v.dirty ? ' dirty' : '')}>${tr(v.dirty ? 'Not saved' : 'Saved')}</span>
      <${Btn} kind="danger" icon="delete" label=${V.deleteArmed ? 'Really delete it?' : 'Delete'} style=${`width:${V.deleteArmed ? 170 : 110}px`} onClick=${() => (V.deleteArmed ? remove() : ((V.deleteArmed = true), redraw()))} />
      <${Btn} kind=${v.dirty ? 'primary' : ''} icon="save" label="Save" style="width:120px" onClick=${save} />
    </div>
    <div class="de-tabs"><${Seg} full value=${V.tab} options=${TABS.map((t, k) => [k, t])} onChange=${(k) => ((V.tab = k), (V.deleteArmed = false), redraw())} /><span class="file">${v.file}</span></div>
    <div class="de-body"><${Tab} v=${v} /></div>
    <${Problems} v=${v} />
  </${Card}>`;
}

/** Where the page's Back goes (`flow::page`): the line editor when it opened the page. */
export const depotsBack = () => (V.fromLines ? 'lines' : 'editor');

export function DepotsPage() {
  useStore();
  const want = S.ed.depotOpen;
  useEffect(() => {
    // (the page opened: the files read again, the one asked for opened - once per opening)
    V.fromLines = !!want?.fromLines;
    show(want?.key || '');
  }, [want?.at]);
  return html`<p class="ed-para de-narrow">${tr('The depot editor needs a wider window: open it on a computer.')}</p>
  <div class="de-page">
    <${Files} />
    ${V.view
      ? html`<${OpenFile} />`
      : html`<${Card} class="de-none"><${Heading} title="Depot files of your own" /><p class="ed-para soft">${tr('A depot file tells the buses\' IBIS and destination displays what to show. Make one of your own: start from the map\'s depot file, one of the bus\'s or an empty one, and give its destinations, stops and routes the texts you want - each display\'s shown as it lights up. A line chooses it in the line editor under Displays, and every bus that drives the line is given it; chosen on the bus step, it goes to any bus, for a special or a service trip.')}</p></${Card}>`}
  </div>`;
}

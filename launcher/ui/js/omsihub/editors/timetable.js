// The Timetable page (`crates/omsi-launcher-ui/src/timetable.rs`): a map's lines, their tours
// and the trips each tour runs - the part of OMSI 2's timetable editor a driver uses to change
// when buses go. Three cards side by side: the map and its lines (a new one, the map's own
// timetable back), the line's tours (new, copied some minutes later, repeated up to a last
// departure, deleted) and the tour's trips (when each leaves, which trip, its profile, where it
// goes). The lines changed are kept while the player moves between them and saved together
// (`tt_save`, the content folder's copy of the map's TTData); the tours are made in Rust
// (`tt_edit`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { Card, Heading, Row, Badge, Field, Drop, IconBtn, Btn, tr, status, fmtTime, parseTime, countOf, mapLabel, reloadLines, keep } from './kit.js';
import { Chips } from '../bits.js';

/** What the page keeps (`TimetableView`). */
const V = keep('tt', { map: null, loaded: null, data: null, line: 0, tour: 0, times: [], timesFor: null, offset: '20', until: '22:00', newLine: '', dirty: new Set(), resetArmed: false, column: 0, busy: false });

const redraw = () => set({});

// (as they are in every language, as the game-drawn launcher shows them)
const HMM = 'h:mm';
const MINUS = '−1 min';
const PLUS = '+1 min';

/** Read the timetable of the map `file` (another map: its changes are saved first). */
async function load(file) {
  V.busy = true;
  redraw();
  try {
    const d = await call('tt_load', { map: file });
    Object.assign(V, { data: d, loaded: file, line: 0, tour: 0, timesFor: null, dirty: new Set(), resetArmed: false });
  } catch (e) {
    status(e.message, true);
    Object.assign(V, { data: { lines: [], trips: [] }, loaded: file });
  }
  V.busy = false;
  redraw();
}

/** Save every changed line (`save_all`): how many were saved, and the first error. */
async function saveAll() {
  const lines = V.data.lines.filter((l) => V.dirty.has(l.name));
  if (!lines.length) return { saved: 0, error: null };
  const r = await call('tt_save', { map: V.loaded, lines }).catch((e) => ({ saved: 0, error: e.message, lines: [] }));
  for (const x of r.lines || []) {
    const l = V.data.lines.find((y) => y.name === x.name);
    if (l) Object.assign(l, x.line);
  }
  if (!r.error) V.dirty = new Set();
  V.timesFor = null;
  return r;
}

const linesSaved = (n) => tr('%{n} line(s) saved', { n });

/** The trips the line can run: those naming it, else those it runs already, else all. */
function tripsOf(line) {
  const d = V.data;
  const same = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();
  let out = d.trips.filter((t) => same(t.line, line.name)).map((t) => t.name);
  for (const t of line.tours.flatMap((x) => x.trips)) if (!out.some((x) => same(x, t.trip))) out.push(t.trip);
  if (!out.length) out = d.trips.map((t) => t.name);
  return out.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}

const tripInfo = (name) => V.data.trips.find((t) => t.name.toLowerCase() === String(name).toLowerCase());

/** A change of the line's tours, made in Rust (`tt_edit`). */
async function edit(op, more = {}) {
  const line = V.data.lines[V.line];
  try {
    const r = await call('tt_edit', { line, tour: V.tour, op, ...more });
    V.data.lines[V.line] = { ...line, ...r.line };
    V.tour = r.tour;
    V.timesFor = null;
    V.dirty.add(line.name);
    redraw();
    return r;
  } catch (e) {
    status(e.message, true);
    return null;
  }
}

function LinesCard({ maps }) {
  const d = V.data;
  const names = maps.map((m) => [m.file, mapLabel(m)]);
  const pickMap = async (file) => {
    if (file === V.loaded) return;
    if (V.dirty.size) {
      const r = await saveAll();
      status(r.error ? tr('Not saved: %{error}', { error: r.error }) : linesSaved(r.saved), !!r.error);
    }
    V.map = file;
    load(file);
  };
  const add = () => {
    const name = V.newLine.trim();
    if (!name || /[/\\:*?"<>|]/.test(name)) return status('Type the new line\'s name (it becomes the file name, so no / \\ : * ? " < > |)', true);
    if (d.lines.some((x) => x.name.toLowerCase() === name.toLowerCase())) return status(tr('Line %{line} is there already', { line: name }), true);
    const dir = d.dir.replace(/[\\/]+$/, '');
    d.lines.push({ path: `${dir}/TTData/${name}.ttl`, name, user_allowed: true, priority: 0, own: false, tours: [] });
    d.lines.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
    Object.assign(V, { line: d.lines.findIndex((x) => x.name === name), tour: 0, timesFor: null, newLine: '' });
    V.dirty.add(name);
    status(tr('Line %{line} added: give it tours, then save', { line: name }));
    redraw();
  };
  const reset = async () => {
    if (!V.resetArmed) {
      V.resetArmed = true;
      status('Press "Reset timetable" again to put the map\'s own timetable back (the changes made here are lost)');
      return redraw();
    }
    V.resetArmed = false;
    try {
      const r = await call('tt_reset', { map: V.loaded });
      let m = r.done;
      if (r.own > 0) m += '; ' + tr('your %{n} own line(s) written again', { n: r.own });
      if (r.own_error) m += '; ' + tr('your own lines could not be written again: %{error}', { error: r.own_error });
      status(m);
      reloadLines();
      load(V.loaded);
    } catch (e) {
      status(e.message, true);
    }
  };
  return html`<${Card} class="tt-lines">
    <${Heading} title="Lines" />
    <${Drop} raw value=${V.loaded} options=${names} onChange=${pickMap} />
    <div class="list" style="margin-top:12px">
      ${d.lines.map((l, i) => html`<${Row} on=${i === V.line} onClick=${() => (Object.assign(V, { line: i, tour: 0, timesFor: null, column: 1 }), redraw())}>
        <span class="name">${l.name}${V.dirty.has(l.name) ? ' •' : ''}</span>${l.own && html`<${Badge} text="own" />`}
        <span class="side">${countOf(l.tours.length, 'one tour', '%{n} tours')}</span>
      </${Row}>`)}
      ${!d.lines.length && html`<p class="empty-note">${tr('This map has no timetable (no TTData lines).')}</p>`}
    </div>
    <div class="foot" style="margin-top:8px">
      <${Field} value=${V.newLine} onInput=${(v) => ((V.newLine = v), redraw())} placeholder="New line (name)" icon="add" onEnter=${add} style="flex:1" />
      <${Btn} label="Add" onClick=${add} style="flex:none;width:82px" />
    </div>
    <div class="foot"><${Btn} icon="restart_alt" label=${V.resetArmed ? 'Press again to reset' : 'Reset timetable'} onClick=${reset} /></div>
  </${Card}>`;
}

function ToursCard() {
  const line = V.data.lines[V.line];
  const trips = tripsOf(line);
  const off = Number(V.offset.trim().replace(',', '.'));
  const offOk = V.offset.trim() !== '' && Number.isFinite(off);
  const copy = () => (offOk ? edit('copy', { offset: off }) : status('Type the minutes the copy runs later (negative for earlier).', true));
  const repeat = async () => {
    const until = parseTime(V.until);
    if (!(offOk && off >= 1)) return status('Type the minutes between the tours (1 or more) in the field above', true);
    if (until == null) return status('Type the last departure as h:mm', true);
    const r = await edit('repeat', { offset: off, until });
    if (r) status(tr('%{n} tour(s) made, every %{every} min up to %{time}', { n: r.made, every: off, time: fmtTime(until) }));
  };
  return html`<${Card} class="tt-tours">
    <${Heading} raw title=${tr('Line %{line}', { line: line.name })} />
    <div class="list">
      ${line.tours.map((t, i) => html`<${Row} on=${i === V.tour} onClick=${() => (Object.assign(V, { tour: i, column: 2 }), redraw())}>
        <span class="name">${tr('Tour %{tour}', { tour: t.number })}</span><span class="side">${t.trips[0] ? fmtTime(t.trips[0].departure) : ''} | ${t.trips.length}</span>
      </${Row}>`)}
    </div>
    <div class="stack">
      <${Btn} class="whole" icon="add" label="New tour" onClick=${() => edit('new_tour', { first: trips[0] || '' })} />
      <${Field} value=${V.offset} onInput=${(v) => ((V.offset = v), redraw())} placeholder="min" icon="schedule" mono />
      <${Btn} icon="content_copy" label="Copy +min" onClick=${copy} />
      <${Field} value=${V.until} onInput=${(v) => ((V.until = v), redraw())} placeholder="until h:mm" icon="schedule" mono />
      <${Btn} icon="autorenew" label="Repeat" onClick=${repeat} />
      <${Btn} class="whole" icon="delete" label="Delete tour" onClick=${() => line.tours.length && edit('delete_tour')} />
    </div>
  </${Card}>`;
}

function TripsCard() {
  const line = V.data.lines[V.line];
  const tour = line.tours[V.tour];
  if (!tour) return html`<${Card} class="tt-trips"><span></span></${Card}>`;
  const key = `${V.line}|${V.tour}`;
  if (V.timesFor !== key || V.times.length !== tour.trips.length) {
    V.times = tour.trips.map((t) => fmtTime(t.departure));
    V.timesFor = key;
  }
  const names = tripsOf(line);
  const touched = () => (V.dirty.add(line.name), redraw());
  const save = async () => {
    if (V.times.some((t) => parseTime(t) == null)) return status('A departure is not a time (h:mm).', true);
    const r = await saveAll();
    status(r.error ? tr('Not saved: %{error}', { error: r.error }) : linesSaved(r.saved), !!r.error);
    // (the Drive page's lists are read again: they showed the lines as they were)
    if (r.saved > 0) reloadLines();
    redraw();
  };
  const n = V.dirty.size;
  const label = n === 0 ? 'Saved' : n === 1 ? 'Save' : tr('Save all (%{n} lines)', { n });
  return html`<${Card} class="tt-trips">
    <${Heading} raw title=${`${tr('Tour %{tour}', { tour: tour.number })} - ${tour.ai_group}`} />
    <div class="ed-tt-trips">
      <div class="ed-tt-grid">
        <span class="head">${tr('Departs')}</span><span class="head">${tr('Trip')}</span><span class="head">${tr('Profile')}</span><span class="head">${tr('To')}</span><span class="head"></span>
        ${tour.trips.map((t, i) => {
          const info = tripInfo(t.trip);
          const profs = info?.profiles?.length ? info.profiles : [String(t.profile)];
          const pi = Math.min(Math.max(0, t.profile), profs.length - 1);
          return html`
            <${Field} mono value=${V.times[i]} bad=${parseTime(V.times[i]) == null} placeholder=${HMM} raw onInput=${(v) => {
              V.times[i] = v;
              const m = parseTime(v);
              if (m == null) return redraw();
              t.departure = m;
              touched();
            }} />
            <${Drop} raw value=${names.find((x) => x.toLowerCase() === t.trip.toLowerCase()) ?? names[0]} options=${names.map((x) => [x, x])} onChange=${(v) => ((t.trip = v), (t.profile = 0), touched())} />
            <${Drop} raw value=${pi} options=${profs.map((p, k) => [k, p])} onChange=${(k) => ((t.profile = k), touched())} />
            <span class="to">${info?.terminus || ''}</span>
            <${IconBtn} icon="close" title="Remove this trip" onClick=${() => (tour.trips.splice(i, 1), V.times.splice(i, 1), touched())} />`;
        })}
      </div>
    </div>
    <div class="buttons">
      <${Btn} icon="add" label="Add trip" style="width:150px" onClick=${() => edit('add_trip', { first: names[0] || '' })} />
      <${Btn} raw label=${MINUS} style="width:100px" onClick=${() => edit('shift', { offset: -1 })} />
      <${Btn} raw label=${PLUS} style="width:100px" onClick=${() => edit('shift', { offset: 1 })} />
      <span class="grow"></span>
      <${Btn} kind=${n > 0 ? 'primary' : ''} icon="save" label=${label} raw=${n > 1} style="width:170px" onClick=${() => n > 0 && save()} />
    </div>
  </${Card}>`;
}

export function TimetablePage() {
  const s = useStore();
  const maps = s.maps || [];
  useEffect(() => {
    if (!maps.length) return;
    // (first shown: the map chosen on the Drive page, not the first of the list)
    const want = V.map ?? (maps.some((m) => m.file === s.choice.map) ? s.choice.map : maps[0].file);
    if (V.loaded !== want && !V.busy) load(want);
  }, [maps.length]);
  if (!maps.length) return html`<p class="ed-para">${tr('No maps found.')}</p>`;
  if (!V.data) return html`<div class="ed-tt"><p class="ed-para">${tr('Reading the timetable…')}</p></div>`;
  const narrow = innerWidth < 760;
  const hasLine = V.data.lines.length > 0;
  V.line = Math.min(V.line, Math.max(0, V.data.lines.length - 1));
  if (hasLine) V.tour = Math.min(V.tour, Math.max(0, V.data.lines[V.line].tours.length - 1));
  return html`<div class=${'ed-tt' + (narrow ? ' narrow' : '')}>
    ${narrow && html`<${Chips} value=${V.column} options=${[[0, 'Lines'], [1, 'Tours'], [2, 'Trips']]} onChange=${(c) => ((V.column = c), redraw())} />`}
    ${(!narrow || V.column === 0) && html`<${LinesCard} maps=${maps} />`}
    ${hasLine && (!narrow || V.column === 1) && html`<${ToursCard} />`}
    ${hasLine && (!narrow || V.column === 2) && html`<${TripsCard} />`}
  </div>`;
}

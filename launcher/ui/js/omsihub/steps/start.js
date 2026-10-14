// Where a free drive starts, as our game-drawn launcher's (`freedrive.rs`): a map, a place to
// start, the day and a bus - nothing is booked and nobody keeps the time. "Free": the day's
// stops in two groups - the starting points, where trips begin (the most departures first),
// and the intermediate stops buses only call at (the most buses first) - each with the plates
// of the lines that serve it, and the map's own entry points; a stop chosen puts the bus at the
// entry point nearest to it. "Choose a line yourself": a line (the map's, or the player's own)
// and one of its routes, and the bus starts at the route's first stop with the game knowing the
// line (`--free-line`) - the navigator draws the way, the IBIS can be typed for it.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../../store.js';
import { call } from '../../api.js';
import { choose, curMap } from '../../data.js';
import { Search } from '../../ui.js';
import { RouteMap } from '../../routemap.js';
import { Actions, Head, Icon, Radio, Seg, Button, tr } from '../bits.js';
import { goStep, nextOf, previousOf } from '../flow.js';
import { mapColors } from './mapbehind.js';
import './more.js';

const natural = (a, b) => String(a).localeCompare(String(b), undefined, { numeric: true, sensitivity: 'base' });

/** The number a trip's displays show; a depot run has none. */
const plateOf = (trip) => String(trip.line || '').trim();

/** The lines a player may drive when the map marks any (`[userallowed]`), else all; a line of
 * the player's own (`oo_…` files) does not count as the map's mark. */
function drivable(lines) {
  const own = (n) => /^oo_/i.test(n);
  const marked = lines.some((l) => l.user_allowed && !own(l.name));
  return lines.filter((l) => !marked || l.user_allowed);
}

/** The trips that leave on the day (all of them when no tour runs: the date is a guess). */
function tripsOfDay(lines) {
  const any = lines.some((l) => l.tours.some((t) => t.runs));
  return lines.flatMap((l) => l.tours.filter((t) => t.runs || !any).flatMap((t) => t.trips.map((trip) => [l, trip])));
}

/** The day's stops as the starting points and the intermediate stops (`start_stops`). */
export function startStops(lines) {
  const all = new Map();
  for (const [, trip] of tripsOfDay(drivable(lines))) {
    const plate = plateOf(trip);
    trip.stops.forEach((stop, i) => {
      const name = String(stop.name || '').trim();
      if (!name) return;
      let s = all.get(name);
      if (!s) all.set(name, (s = { name, ids: [], lines: [], begins: 0, passes: 0, at: null }));
      if (!s.ids.includes(stop.id)) s.ids.push(stop.id);
      if (plate && !s.lines.includes(plate)) s.lines.push(plate);
      // (a trip and the stop's place in it: where the map finds the stop)
      if (!s.at) s.at = [trip.name, i];
      if (i === 0) s.begins++;
      else s.passes++;
    });
  }
  const list = [...all.values()];
  for (const s of list) s.lines.sort(natural);
  const begin = list.filter((s) => s.begins > 0).sort((a, b) => b.begins - a.begins || a.name.localeCompare(b.name));
  const via = list.filter((s) => s.begins === 0).sort((a, b) => b.passes - a.passes || a.name.localeCompare(b.name));
  return [begin, via];
}

/** The lines a free drive can follow: name, the numbers their displays show, termini. */
function lineRows(lines) {
  return drivable(lines)
    .filter((l) => l.tours.some((t) => t.trips.length))
    .map((l) => {
      const plates = [];
      for (const t of l.tours.flatMap((x) => x.trips)) {
        const p = plateOf(t);
        if (p && !plates.includes(p)) plates.push(p);
      }
      plates.sort(natural);
      return { name: l.name, plates: plates.length ? plates : [l.name], termini: (l.termini || []).join(' · ') };
    });
}

/** A line's routes, the most driven first, the service trips last (`routes_of`). */
function routesOf(line, lines) {
  const out = [];
  const any = drivable(lines).some((l) => l.tours.some((t) => t.runs));
  for (const trip of line.tours.filter((t) => t.runs || !any).flatMap((t) => t.trips)) {
    if (trip.stops.length < 2) continue;
    const r = out.find((x) => x.trip === trip.name);
    if (r) r.runs++;
    else out.push({ trip: trip.name, line: plateOf(trip), from: trip.from, terminus: trip.terminus, stops: trip.stops.length, km: trip.km, runs: 1 });
  }
  for (const trip of line.tours.flatMap((t) => t.trips)) {
    if (trip.stops.length >= 2 && !out.some((r) => r.trip === trip.name)) out.push({ trip: trip.name, line: plateOf(trip), from: trip.from, terminus: trip.terminus, stops: trip.stops.length, km: trip.km, runs: 0 });
  }
  out.sort((a, b) => Number(!a.line) - Number(!b.line) || b.runs - a.runs || String(a.terminus).localeCompare(b.terminus) || String(a.from).localeCompare(b.from));
  return out;
}

/** The chosen route in the day's timetable: its line and its trip. */
function findRoute(lines, line, route) {
  const l = lines.find((x) => x.name === line);
  const t = l?.tours.flatMap((x) => x.trips).find((x) => x.name === route);
  return l && t ? [l, t] : null;
}

/** The line and route a free drive follows, when the player chose one (`free_route`). */
export const freeRoute = (c) => (c.free && c.own_line && String(c.free_line || '').trim() && String(c.free_route || '').trim() ? [c.free_line, c.free_route] : null);

/** The player's own lines of the map (`hub_own_lines`, once a map). */
let ownFor = '';
function useOwnLines(map) {
  const s = useStore();
  useEffect(() => {
    if (!map || ownFor === map) return;
    ownFor = map;
    call('hub_own_lines', { map }).then((own) => set({ hubOwn: own })).catch(() => set({ hubOwn: [] }));
  }, [map]);
  return ownFor === map ? s.hubOwn || [] : [];
}

/** Where a stop stands on the map and the entry points: the map's export of a trip calling at
 * it (`route_map`), kept by map, day and trip. */
const places = new Map();
function placesOf(map, date, trip) {
  const key = `${map}|${date}|${trip}`;
  if (!places.has(key)) places.set(key, call('route_map', { map, date, trip }).catch(() => null));
  return places.get(key);
}

const distance = (m) => (m >= 1000 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m / 10) * 10} m`);

/** The stop chosen put at its nearest entry point (`resolve_stop`): its place read from the
 * map, the entry point and how far it is. */
function useStop(c, stops) {
  const [near, setNear] = useState(null);
  const s = [...stops[0], ...stops[1]].find((x) => x.name === c.free_stop);
  const key = s && c.free && !c.own_line ? `${c.map}|${c.date}|${s.name}` : '';
  useEffect(() => {
    if (!key || !s.at) return;
    const [trip, k] = s.at;
    placesOf(c.map, c.date, trip).then((m) => {
      const at = m?.stops?.find((x) => x.k === k)?.at;
      if (!at || !m.entries?.length) return;
      let best = null;
      for (const e of m.entries) {
        const d = Math.hypot(e.at[0] - at[0], e.at[1] - at[1]);
        if (!best || d < best.d) best = { index: e.index, d };
      }
      setNear({ key, metres: best.d });
      if (S.choice.free_entry !== best.index) choose({ free_entry: best.index });
    });
  }, [key]);
  return near?.key === key ? near.metres : null;
}

/** An entry point's name, as the map's list numbers it. */
function entryName(m, e) {
  const x = e >= 0 ? m?.entry_points?.[e] : null;
  return x ? x.name || `${tr('entry')} ${e + 1}` : null;
}

/** Plates side by side within the room; what does not fit is counted ("+3"). */
function Plates({ lines, h, w }) {
  let x = 0;
  const out = [];
  for (let k = 0; k < lines.length; k++) {
    const need = Math.max(h * 1.8, lines[k].length * h * 0.42 + h * 0.7);
    const rest = lines.length - k;
    if (x + need > w - (rest > 1 ? 30 : 0)) {
      out.push(html`<span class="hub-more-n">+${rest}</span>`);
      break;
    }
    out.push(html`<span class="hub-plate" style=${`height:${h}px;min-width:${h * 1.8}px;font-size:${(h * 0.6) / 1.303}px;border-radius:5px`}>${lines[k]}</span>`);
    x += need + 5;
  }
  return html`<span class="hub-fplates">${out}</span>`;
}

export function StartStep() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const lines = s.lines?.list || [];
  const reading = s.lines?.loading;
  const stops = startStops(lines);
  const [tab, setTab] = useState(c.free_stop ? (stops[1].some((x) => x.name === c.free_stop) ? 1 : 0) : (c.free_entry ?? -1) >= 0 ? 2 : 0);
  const [filter, setFilter] = useState('');
  const [pickLine, setPickLine] = useState(false);
  const own = useOwnLines(c.map);
  const metres = useStop(c, stops);
  const route = freeRoute(c);
  const found = route ? findRoute(lines, route[0], route[1]) : null;
  // the start in a few words under the title, and in full in the foot
  let sub;
  let foot;
  const entry = entryName(m, c.free_entry ?? -1);
  if (c.own_line) {
    sub = found ? tr('Line %{line} to %{terminus}', { line: plateOf(found[1]) || found[0].name, terminus: String(found[1].terminus).trim() }) : tr('Choose a line yourself');
    foot = found ? tr("Your bus starts at the entry point nearest to %{stop}, the route's first stop. Nothing is booked: drive the line as you like.", { stop: String(found[1].from).trim() }) : tr('Choose a line, then the way it goes: the bus starts at its first stop.');
  } else if (c.free_stop) {
    sub = c.free_stop;
    foot = entry && metres != null ? tr('Your bus stands at %{entry}, the entry point nearest to %{stop} (%{distance} away).', { entry, stop: c.free_stop, distance: distance(metres) }) : tr('Your bus stands at the entry point nearest to %{stop}.', { stop: c.free_stop });
  } else {
    sub = entry || tr('Automatic');
    foot = entry ? tr('Your bus stands at %{entry}. The orange marks on the map are the entry points: click one to start there.', { entry }) : tr("Your bus stands at the map's first entry point. Choose a stop, or click an orange mark on the map.");
  }
  // an entry point clicked on the map: the bus starts there - not at a stop, not along a line
  const onEntry = (i) => {
    choose({ free_entry: i, free_stop: '', own_line: false });
    setTab(2);
    setFilter('');
  };
  // the map: the route followed, its first stop the start; else the entry point the bus is at
  const stopsShown = found ? found[1].stops.map((x, k) => ({ time: k === 0 ? tr('Start') : '', name: x.name })) : [];
  return html`
    <div class="hub-map"><${RouteMap} map=${c.map} date=${c.date} trip=${found ? found[1].name : ''} stops=${stopsShown} entry=${route ? -1 : c.free_entry ?? -1} onEntry=${onEntry} height=${0} colors=${mapColors()} /></div>
    <section class="hub-sheet narrow">
      <${Head} icon="location_on" title="Start point" sub=${sub} />
      <div class="hub-body"><div class="hub-pad hub-free">
        <div title=${tr('Free: the bus starts where you choose, without a line. Choose a line yourself: it starts at the first stop of the route you pick, and the navigator and the IBIS know the line - nothing is booked either way.')}>
          <${Seg} full value=${c.own_line ? 1 : 0} options=${[[0, 'Free'], [1, 'Choose a line yourself']]} onChange=${(k) => (choose({ own_line: k === 1 }), setFilter(''), setPickLine(false))} />
        </div>
        ${c.own_line
          ? html`<${LineBody} lines=${lines} own=${own} reading=${reading} filter=${filter} setFilter=${setFilter} pickLine=${pickLine} setPickLine=${setPickLine} />`
          : html`<${PlaceBody} stops=${stops} m=${m} reading=${reading} tab=${tab} setTab=${setTab} filter=${filter} setFilter=${setFilter} />`}
        <p class="hub-pins"><${Icon} name="location_on" size=${14} /><span>${tr('In the game, the city map (Shift+M) sets a destination: right-click it, or use its pin tool.')}</span></p>
      </div></div>
      <footer class="hub-foot hub-foot-wrap"><${Icon} name="info" size=${14} /><span class="grow">${foot}</span></footer>
    </section>
    <${Actions} back onBack=${() => goStep(previousOf('start'))} from=${'calc(var(--hub-edge-in) + clamp(288px, 27.4cqw, 420px) + 14px)'} main="Next step" onMain=${() => goStep(nextOf('start'))} />
  `;
}

/** The column heads over a list. */
const Heads = ({ left, right }) => html`<div class="hub-fheads"><span>${tr(left)}</span>${right && html`<span>${tr(right)}</span>`}</div>`;

/** Free: the starting points, the intermediate stops or the entry points (`place_body`). */
function PlaceBody({ stops, m, reading, tab, setTab, filter, setFilter }) {
  const c = S.choice;
  const [begin, via] = stops;
  const entries = (m?.entry_points || []).map((e, k) => e.name || `${tr('entry')} ${k + 1}`);
  // (a map without a timetable has no stops: its entry points are all there is)
  const t = !begin.length && !via.length && !reading && tab < 2 ? 2 : tab;
  const q = filter.trim().toLowerCase();
  const labels = [
    [0, tr('Starting points (%{n})', { n: begin.length })],
    [1, tr('Intermediate stops (%{n})', { n: via.length })],
    [2, tr('Entry points (%{n})', { n: entries.length })],
  ];
  const chosenEntry = c.free_stop ? -2 : c.free_entry ?? -1;
  let list;
  if (t < 2) {
    const rows = (t === 0 ? begin : via).filter((s) => !q || s.name.toLowerCase().includes(q) || s.lines.some((x) => x.toLowerCase() === q));
    list = html`<${Heads} left="Bus stop" right=${t === 0 ? 'Departures' : 'Buses calling'} />
      ${!rows.length
        ? html`<p class="hub-fnote">${tr(reading ? 'Reading the timetable…' : !q ? 'The timetable has no such stops on this day.' : 'No stop of that name.')}</p>`
        : html`<div class="hub-flist">${rows.map((s) => {
            const on = s.name === c.free_stop;
            return html`<button type="button" class=${'hub-frow tall' + (on ? ' on' : '')} onClick=${() => choose({ free_stop: s.name, own_line: false })}>
              <${Radio} on=${on} />
              <span class="what"><b class="ellipsis">${s.name}</b>${s.lines.length ? html`<${Plates} lines=${s.lines} h=${17} w=${240} />` : html`<span class="dim">${tr('service trips')}</span>`}</span>
              <b class="n">${t === 0 ? s.begins : s.passes}</b>
            </button>`;
          })}</div>`}`;
  } else {
    const rows = [[-1, tr("Automatic (the map's first)")], ...entries.map((n, k) => [k, n])].filter(([, n]) => !q || n.toLowerCase().includes(q));
    list = html`<${Heads} left="Entry point" />
      <div class="hub-flist">${rows.map(([e, name]) => {
        const on = e === chosenEntry;
        return html`<button type="button" class=${'hub-frow' + (on ? ' on' : '')} onClick=${() => choose({ free_entry: e, free_stop: '' })}>
          <${Radio} on=${on} /><span class=${'what ellipsis' + (e < 0 ? ' auto' : '')}>${name}</span>
        </button>`;
      })}</div>`;
  }
  return html`<div class="hub-chips" style="margin:12px 0">${labels.map(([k, l]) => html`<button type="button" class=${'hub-chip' + (k === t ? ' on' : '')} onClick=${() => k !== t && setTab(k)}>${l}</button>`)}</div>
    <${Search} value=${filter} onInput=${setFilter} placeholder=${t === 2 ? 'Search entry points…' : 'Search stops…'} />
    ${list}`;
}

/** Along a line: the lines to choose from, or the routes of the one chosen (`line_body`). */
function LineBody({ lines, own, reading, filter, setFilter, pickLine, setPickLine }) {
  const c = S.choice;
  const line = lines.find((x) => x.name === c.free_line);
  const ownOf = (name) => own.find((o) => o.file.toLowerCase() === name.toLowerCase());
  if (line && !pickLine) {
    const routes = routesOf(line, lines);
    const plates = lineRows([line])[0]?.plates || [line.name];
    const o = ownOf(line.name);
    return html`<div class="hub-froute-head">
        ${o ? html`<span class="hub-plate" style=${`height:26px;min-width:47px;background:${o.colour || ''}`}>${o.number}</span><b class="ellipsis grow">${o.name}</b>` : html`<span class="grow"><${Plates} lines=${plates} h=${26} w=${220} /></span>`}
        <${Button} icon="alt_route" label="Other line" onClick=${() => (setPickLine(true), setFilter(''))} />
      </div>
      <${Heads} left="Direction" right="Today" />
      <div class="hub-flist">${routes.map((r) => {
        const on = r.trip === c.free_route;
        return html`<button type="button" class=${'hub-frow tall' + (on ? ' on' : '')} onClick=${() => choose({ free_route: r.trip })}>
          <${Radio} on=${on} />
          <span class="what"><b class="ellipsis">${r.line && plates.length > 1 && html`<span class="hub-plate" style="height:18px;min-width:32px;font-size:8.3px;margin-right:6px">${r.line}</span>`}→ ${String(r.terminus).trim()}</b>
            <span class="dim ellipsis">${tr('from %{stop} · %{n} stops · %{km} km', { stop: String(r.from).trim(), n: r.stops, km: Number(r.km).toFixed(1) })}</span></span>
          <b class="n">${r.runs ? `${r.runs}×` : '-'}</b>
        </button>`;
      })}</div>`;
  }
  const all = lineRows(lines).map((r) => ({ ...r, n: routesOf(lines.find((x) => x.name === r.name), lines).length, own: ownOf(r.name) }));
  const mine = all.filter((x) => x.own);
  const map = all.filter((x) => !x.own);
  const showMine = !!c.my_lines && mine.length > 0;
  const q = filter.trim().toLowerCase();
  const rows = (showMine ? mine : map).filter((x) => !q || x.name.toLowerCase().includes(q) || x.plates.some((p) => p.toLowerCase().includes(q)) || x.termini.toLowerCase().includes(q) || (x.own?.name || '').toLowerCase().includes(q));
  const pick = (name) => {
    const l = lines.find((x) => x.name === name);
    choose({ free_line: name, free_route: l ? routesOf(l, lines)[0]?.trip || '' : '' });
    setPickLine(false);
    setFilter('');
  };
  return html`<div style="margin:12px 0">
      <${Seg} full value=${showMine ? 'mine' : 'map'} options=${[['map', tr('Map lines (%{n})', { n: map.length })], ['mine', tr('My lines (%{n})', { n: mine.length })]]} onChange=${(v) => (v === 'mine' && !mine.length ? toast(tr('Make one in Editor → Line editor'), 'info') : (choose({ my_lines: v === 'mine' }), setFilter('')))} />
    </div>
    <${Search} value=${filter} onInput=${setFilter} placeholder=${showMine ? 'Search your lines…' : 'Search lines…'} />
    <${Heads} left="Line" right="Routes" />
    ${!rows.length
      ? html`<p class="hub-fnote">${tr(reading ? 'Reading the timetable…' : !q ? 'This map has no timetable: drive free instead.' : 'No line like that.')}</p>`
      : html`<div class="hub-flist">${rows.map((r) => {
          const on = r.name === c.free_line;
          return html`<button type="button" class=${'hub-frow tall line' + (on ? ' on' : '')} onClick=${() => pick(r.name)}>
            <span class="what">${r.own ? html`<span class="hub-plate" style=${`height:20px;min-width:36px;background:${r.own.colour || ''}`}>${r.own.number}</span>` : html`<${Plates} lines=${r.plates} h=${20} w=${230} />`}
              <span class="dim ellipsis">${r.own ? r.own.name || r.termini : r.termini || r.name}</span></span>
            <b class="n">${r.n}</b>
          </button>`;
        })}</div>`}`;
}

// What the duty step has beyond phase 1's (Trello "Tauri 3/6"), as our game-drawn launcher's
// (`flow::step_duty`, `shiftsheet::map_tools`, `drive::book_panel`): the map's own buttons in its
// top right corner - the route framed again, in, out - and the roadbook beside the map, put
// away behind a button of its own until it is opened: the chosen tour's trips and their stops
// (the trip it starts with marked, an earlier one clicked to start there) or a work shift's
// with where it goes on with another tour, and what to type into the IBIS.

import { html, useEffect, useRef } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { curLine, curTour, pickedTrip } from '../../data.js';
import { tripAt, pickTrip, hms } from '../../pages/drive.js';
import { Icon, tr } from '../bits.js';
import './more.js';

const hhmm = (sec) => hms(sec);

/** "line 76", or "depot run" for a trip without one. */
const lineWords = (line) => (line ? tr('line %{line}', { line }) : tr('depot run'));

/** The roadbook open or put away (`DriveView::book_open`). */
const bookOpen = () => !!S.hub.bookOpen;
const setBook = (on) => set({ hub: { ...S.hub, bookOpen: on } });

/** The map's buttons and the roadbook (`flow::step_duty`). `onRefit` frames the route again. */
export function DutyMapTools({ onRefit }) {
  useStore();
  const open = bookOpen() && !S.choice.free;
  const zoom = (out) => {
    const el = document.querySelector('.hub-map .routemap');
    if (!el) return;
    const r = el.getBoundingClientRect();
    // (the map's own wheel, a notch of 1.15 three times: the game-drawn launcher's 1.5)
    for (let k = 0; k < 3; k++) el.dispatchEvent(new WheelEvent('wheel', { deltaY: out ? 100 : -100, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, bubbles: true, cancelable: true }));
  };
  const tools = [
    ['near_me', 'Centre the map on the route', onRefit],
    ['add', 'Zoom in', () => zoom(false)],
    ['remove', 'Zoom out', () => zoom(true)],
  ];
  return html`<div class=${'hub-maptools' + (open ? ' book' : '')}>
      ${tools.map(([icon, tip, f]) => html`<button type="button" title=${tr(tip)} onClick=${f}><${Icon} name=${icon} size=${17} /></button>`)}
    </div>
    ${!open && !S.choice.free && html`<button type="button" class="hub-bookhandle" title=${tr('The trips of the chosen tour, where they call, and what to type into the IBIS')} onClick=${() => setBook(true)}><${Icon} name="receipt_long" size=${18} />${tr('Roadbook')}</button>`}
    ${open && html`<${Roadbook} />`}`;
}

/** A work shift's trips: (the tour's line, the tour, the trip), in their order. */
function shiftTrips() {
  const lines = S.lines?.list || [];
  const out = [];
  for (const leg of S.choice.legs || []) {
    const [line, tour, first, count] = String(leg).split('|');
    const t = lines.find((x) => x.name === line)?.tours.find((x) => x.number === tour);
    if (!t) continue;
    for (const x of t.trips) if (x.index >= Number(first) && x.index < Number(first) + Number(count)) out.push([line, tour, x]);
  }
  return out;
}

/** A trip's stops as the roadbook's timeline. */
const Stops = ({ t }) => html`<div class="hub-rb-stops">${t.stops.map((st, s) => {
  const end = s === 0 || s + 1 === t.stops.length;
  return html`<div class=${'hub-rb-stop' + (end ? ' end' : '') + (s + 1 < t.stops.length ? ' on' : '')}>
    <time>${hhmm(st.arr)}</time><i></i><span class="ellipsis">${st.name}</span>${s === 0 && html`<em>${tr('dep %{time}', { time: hhmm(st.dep) })}</em>`}
  </div>`;
})}</div>`;

let scrolled = '';
/** The roadbook beside the map (`drive::book_panel`, `step_roadbook`). */
function Roadbook() {
  const s = useStore();
  const c = s.choice;
  const list = useRef();
  // (each earlier trip is one row of 52: the first trip goes to the top, once for each start)
  let scroll = null;
  useEffect(() => {
    if (scroll && list.current && scrolled !== scroll[0]) {
      list.current.scrollTop = scroll[1];
      scrolled = scroll[0];
    }
  });
  let body;
  if (c.composed && !c.free) {
    const trips = shiftTrips();
    if (!trips.length) body = html`<p class="hub-rb-note">${tr('No shift chosen yet. Pick one of the shifts on the left: its trips, and where it goes on with another line, are shown here.')}</p>`;
    else {
      // (`drive::trips_and_changes`: a change for each part after the first)
      const changes = Math.max(0, (c.legs || []).length - 1);
      const n = tr(trips.length === 1 ? '%{n} trip' : '%{n} trips', { n: trips.length });
      const sub = `${n} · ${changes === 0 ? tr('one tour') : changes === 1 ? tr('1 change') : tr('%{n} changes', { n: changes })}`;
      body = html`<div class="hub-rb-title">${tr('Work shift')} · ${hhmm(trips[0][2].departure)} - ${hhmm(trips[trips.length - 1][2].arrival)}</div>
        <div class="hub-rb-sub">${sub}</div>
        <div class="hub-rb-list">${trips.map(([line, tour, t], i) => {
          const change = i > 0 && (trips[i - 1][0] !== line || trips[i - 1][1] !== tour);
          const wait = change ? Math.max(0, Math.round((t.departure - trips[i - 1][2].arrival) / 60)) : 0;
          return html`${change && html`<div class="hub-rb-change"><${Icon} name="sync_alt" size=${15} />${tr('On with line %{line}, tour %{tour} · %{min} min', { line, tour, min: wait })}</div>`}
            <div class=${'hub-rb-trip' + (i === 0 ? ' first' : '')}><b class="ellipsis">${tr(i === 0 ? 'Your first trip' : 'Then')} · ${t.from || '?'} → ${t.terminus}</b>
              <span class="ellipsis">${hhmm(t.departure)} - ${hhmm(t.arrival)} · ${Number(t.km).toFixed(1)} km · ${lineWords(t.line)} · ${t.name}</span></div>
            <${Stops} t=${t} />`;
        })}</div>`;
    }
  } else {
    const line = curLine();
    const tour = curTour();
    if (!line || !tour || c.free) {
      const m = (s.maps || []).find((x) => x.file === c.map);
      body = html`<p class="hub-rb-note">${tr('No duty chosen: free driving on %{map}. Pick a line and a tour under Route to see the roadbook here.', { map: m?.friendly || m?.name || '' })}</p>`;
    } else {
      const from = pickedTrip() != null ? Math.max(0, tour.trips.findIndex((x) => x.index === pickedTrip())) : Math.max(0, tripAt(tour, c.time * 60));
      scroll = [`${line.name}|${tour.number}|${from}`, from * 52];
      body = html`<div class="hub-rb-title">${tr('Line %{line} · tour %{tour} · from %{time}', { line: line.name, tour: tour.number, time: hhmm(c.time * 60) })}</div>
        <div class="hub-rb-sub">${tr('Click a trip to start the tour there.')}</div>
        <div class="hub-rb-list" ref=${list}>${tour.trips.map((t, i) => {
          if (i < from)
            return html`<button type="button" class="hub-rb-trip earlier" onClick=${() => pickTrip(t)}><b class="ellipsis">${tr('Earlier')} | ${t.from || '?'} → ${t.terminus}</b><span class="ellipsis">${hhmm(t.departure)} - ${hhmm(t.arrival)} | ${lineWords(t.line)}</span></button>`;
          const k = i - from;
          return html`<button type="button" class=${'hub-rb-trip' + (k === 0 ? ' first' : '')} onClick=${() => k > 0 && pickTrip(t)}><b class="ellipsis">${tr(k === 0 ? 'Your first trip' : 'Then')} | ${t.from || '?'} → ${t.terminus}</b>
              <span class="ellipsis">${hhmm(t.departure)} - ${hhmm(t.arrival)} | ${Number(t.km).toFixed(1)} km | ${lineWords(t.line)} | ${t.name}</span></button>
            <${Stops} t=${t} />`;
        })}</div>`;
    }
  }
  // (a free drive's note has the IBIS box right under it, the trips put it at the bottom)
  const note = !c.composed && (!curLine() || !curTour() || c.free);
  const shiftless = c.composed && !c.free && !shiftTrips().length;
  return html`<section class=${'hub-roadbook' + (note ? ' note' : '')}>
    <header><h3>${tr('Roadbook')}</h3>
      <button type="button" class="hub-rb-shut" title=${tr('Put the roadbook away (the button on the map brings it back)')} onClick=${() => setBook(false)}><${Icon} name="close" size=${18} /></button></header>
    <div class="hub-rb-body">${body}</div>
    ${!shiftless && html`<${Ibis} />`}
  </section>`;
}

let ibisKey = '';
/** What to type into the IBIS (`drive::ibis_box`): the routes the depot file has for the line. */
function Ibis() {
  const s = useStore();
  const c = s.choice;
  const key = c.line && !c.free ? `${c.bus || ''}|${c.hof || ''}|${c.line}` : '';
  useEffect(() => {
    if (!key || ibisKey === key) return;
    ibisKey = key;
    const [bus, hof, line] = key.split('|');
    call('ibis', { bus, hof, line })
      .then((i) => ibisKey === key && set({ hubIbis: { key, i } }))
      .catch((e) => ibisKey === key && set({ hubIbis: { key, error: e.message } }));
  }, [key]);
  const got = s.hubIbis?.key === key && key ? s.hubIbis : null;
  let body;
  if (!got) body = html`<p>${tr('Pick a line to see what to type into the IBIS. Shift+U in the game types it for you and puts the bus into service.')}</p>`;
  else if (got.error) body = html`<p class="err">${got.error}</p>`;
  else if (!got.i.routes.length) body = html`<p>${tr('Depot file %{hof} has no entries for this line. Type line %{line} and the terminus code by hand, or press Shift+U.', { hof: got.i.hof, line: got.i.line_code })}</p>`;
  else
    body = html`${got.i.routes.slice(0, 3).map((rt) => {
        const code = String(rt.code);
        const [lc, rc] = code.length > 2 ? [code.slice(0, -2), code.slice(-2)] : [got.i.line_code, code];
        return html`<div class="hub-ibis-row"><span class="ellipsis grow">${rt.name || rt.terminus}</span>
          ${[['line', lc], ['route', rc]].map(([k, v]) => html`<span class="pair"><span class="k">${tr(k)}</span><b>${v}</b></span>`)}</div>`;
      })}
      <div class="hub-ibis-foot">${tr('Depot file %{hof} · Shift+U in the game types it for you', { hof: got.i.hof })}</div>`;
  return html`<div class="hub-ibis"><div class="hub-ibis-head">IBIS</div>${body}</div>`;
}

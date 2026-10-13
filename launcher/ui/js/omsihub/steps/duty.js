// The duty (`flow::step_duty`, `shiftsheet`): the duties to choose from as a list of
// departures, arrivals and lengths, and the chosen one opened into its trips - when each
// leaves, the line's plate, where it goes and for how long, the break before it, and where the
// bus goes on with another tour. A work shift is asked for by its length and its part of the
// day; a tour - one bus's trips on one line, OMSI's own duty - by its line, and its opened row
// is where the trip it starts with is picked. The chosen duty's route lies on the map beside.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { choose, curLine, curTour, pickedTrip, realDate } from '../../data.js';
import { tripAt, pickTrip, hms } from '../../pages/drive.js';
import { Select } from '../../ui.js';
import { Actions, Caps, Chips, Foot, Head, Icon, Radio, Seg, Slider, tr, length, count, Button } from '../bits.js';
import { goStep, nextOf, previousOf, chosenLine } from '../flow.js';
import { MapBehind } from './mapbehind.js';
import { EntrySelect } from './map.js';
import { dayName } from './day.js';

const DAYPARTS = ['All day', 'Morning', 'Midday', 'Evening', 'Night'];
const hhmm = (sec) => hms(sec);
/** Natural order of tour numbers ("2" before "10"). */
const natural = (a, b) => String(a).localeCompare(String(b), undefined, { numeric: true });

/** The player's own lines of the map (the line editor's): `hub_own_lines`, once a map. */
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

/** The shifts for the length and part of the day chosen (`hub_shifts`): made again when one
 * of them, the map, the date or the seed changes. */
function useShifts(c) {
  const s = useStore();
  const key = `${c.map}|${c.date}|${c.duty_minutes}|${c.daypart}|${S.hubSeed}`;
  useEffect(() => {
    if (!c.composed || !c.map) return;
    if (s.hubShifts?.key === key) return;
    set({ hubShifts: { key, list: null } });
    call('hub_shifts', { map: c.map, date: c.date, minutes: c.duty_minutes ?? 120, daypart: c.daypart ?? 0, seed: S.hubSeed, legs: c.legs || [] })
      .then((list) => S.hubShifts?.key === key && set({ hubShifts: { key, list } }))
      .catch((e) => S.hubShifts?.key === key && set({ hubShifts: { key, list: [], error: e.message } }));
  }, [key, c.composed]);
  return s.hubShifts?.key === key ? s.hubShifts : { list: null };
}

if (S.hubSeed == null) S.hubSeed = (Date.now() % 2147483647) >>> 0;

/** "3 trips · 67 stops · 2 lines". */
function summary(trips) {
  const lines = [...new Set(trips.map((t) => t.line).filter(Boolean))].length;
  const parts = [count(trips.length, '%{n} trip', '%{n} trips'), count(trips.reduce((n, t) => n + (t.stops || 0), 0), '%{n} stop', '%{n} stops')];
  if (lines) parts.push(count(lines, '%{n} line', '%{n} lines'));
  return parts.join(' · ');
}

/** The opened row: how much the duty is, then its trips with the breaks between them; a tour's
 * trips are buttons (the one it starts with marked). */
function Detail({ trips, first, onTrip }) {
  const driven = trips.slice(first ?? 0);
  return html`<div class="hub-trips">
    <div class="sum">${summary(driven).toUpperCase()}</div>
    ${first != null && html`<div class="sum" style="margin-top:-4px;color:var(--hub-text-faint)">${tr('Click a trip to start the tour there.')}</div>`}
    ${trips.map((t, k) => {
      const before = first != null && k < first;
      const start = first != null && k === first;
      const pause = k > (first ?? 0) && Math.round(t.pause / 60) >= 1;
      return html`${pause && html`<div class="hub-break">${tr('%{time} break', { time: length(t.pause) })}</div>`}
        <div class=${'hub-trip' + (first != null ? ' pick' : '') + (start ? ' first' : '') + (before ? ' before' : '')} onClick=${() => first != null && !start && onTrip?.(k)}>
          <time>${hhmm(t.departure)}</time>
          <span>${t.line ? html`<span class="hub-plate" style="height:18px;min-width:26px;font-size:8.8px;border-radius:9px">${t.line}</span>` : html`<span class="hub-plate" style="height:18px;min-width:26px;background:var(--hub-hover);color:var(--hub-text-soft);border-radius:9px">-</span>`}</span>
          <span style="display:flex;gap:8px"><b class="grow ellipsis" style="min-width:0">${t.terminus}</b><span style="color:var(--hub-text-dim)">${length(t.arrival - t.departure)}</span></span>
        </div>
        ${t.takes_over && html`<div class="hub-change"><${Icon} name="route" size=${12} />${tr('On with line %{line}, tour %{tour}', { line: t.takes_over[0], tour: t.takes_over[1] })}</div>`}`;
    })}
  </div>`;
}

/** The duties as rows: radio, three columns; the chosen one opened. */
function List({ heads, cards, chosen, first, onPick, onTrip, empty }) {
  const box = useRef();
  // (the chosen row brought into view once each time the choice changes)
  const shown = useRef('');
  useEffect(() => {
    const key = cards[chosen]?.key?.join('|') || '';
    if (!key || key === shown.current) return;
    shown.current = key;
    const el = box.current?.querySelector('.hub-duty.on');
    el?.scrollIntoView({ block: 'nearest' });
  });
  return html`
    <div class="hub-rule"></div>
    <div class="hub-heads" style="grid-template-columns:30px 1fr 1fr 1fr;padding-left:32px"><b></b>${heads.map((h) => html`<b>${tr(h)}</b>`)}</div>
    <div class="hub-body" ref=${box}><div class="hub-duties">
      ${!cards.length && html`<p style="color:var(--hub-text-dim);padding:8px 14px;line-height:1.5">${tr(empty)}</p>`}
      ${cards.map((c, k) => html`<div class=${'hub-duty' + (k === chosen ? ' on' : '') + (c.note ? ' quiet' : '')}>
        <button type="button" onClick=${() => onPick(k)}>
          <${Radio} on=${k === chosen} />
          <span><b>${c.cells[0]}</b>${c.note && html` <small style="font-size:8.4px;font-weight:500">${c.note}</small>`}</span>
          <b>${c.cells[1]}</b>
          <span class="len">${c.clock && html`<${Icon} name="schedule" size=${13} />`}${c.clock ? c.cells[2] : html`<b>${c.cells[2]}</b>`}</span>
        </button>
        ${k === chosen && html`<${Detail} trips=${c.trips} first=${first} onTrip=${onTrip} />`}
      </div>`)}
    </div></div>`;
}

function Shifts({ c }) {
  const sh = useShifts(c);
  const list = sh.list || [];
  const chosen = list.findIndex((d) => (c.legs || []).length && d.key.join('|') === c.legs.join('|'));
  useEffect(() => set({ hubShift: list[chosen] || null }), [chosen, sh.list]);
  const cards = list.map((d) => ({ key: d.key, cells: [hhmm(d.start), hhmm(d.end), length(d.seconds)], clock: true, note: '', trips: d.trips }));
  const pick = (k) => {
    const d = list[k];
    const f = d?.first;
    if (!f) return;
    const time = Math.floor(f.departure / 60);
    choose({ line: f.line, tour: f.tour, time, start_trip: [f.line, f.tour, f.index, time], legs: d.key });
  };
  const minutes = Math.max(30, Math.min(480, c.duty_minutes ?? 120));
  return html`
    <div class="hub-pad" style="padding-bottom:12px">
      <div class="hub-caps"><b>${tr('Shift length')}</b><span style="font-weight:700;font-size:11.1px">${length(minutes * 60)}</span></div>
      <input type="range" class="hub-range-input" min="30" max="480" step="15" value=${minutes} style=${`width:100%;--fill:${((minutes - 30) / 450) * 100}%`} onInput=${(e) => choose({ duty_minutes: Number(e.currentTarget.value) })} />
      <div style="height:14px"></div>
      <${Caps} title="Time of day" />
      <${Chips} value=${DAYPARTS[c.daypart ?? 0] || 'All day'} options=${DAYPARTS.map((d) => [d, d])} onChange=${(d) => choose({ daypart: DAYPARTS.indexOf(d) })} />
    </div>
    <${List} heads=${['Departure', 'Arrival', 'Duration']} cards=${cards} chosen=${chosen >= 0 ? chosen : null} first=${null} onPick=${pick}
      empty=${sh.list == null ? 'Reading the timetable…' : sh.error || 'No shift of this length begins at this time of day. Try another length or part of the day.'} />`;
}

function Tours({ c }) {
  const s = useStore();
  const own = useOwnLines(c.map);
  const allowed = (s.lines.list || []).filter((l) => l.user_allowed);
  const ownFiles = new Set(own.map((o) => o.file.toLowerCase()));
  const isOwn = (l) => ownFiles.has(l.name.toLowerCase()) || /^oo_/i.test(l.name);
  const mapLines = allowed.filter((l) => !isOwn(l));
  const myLines = allowed.filter(isOwn);
  const mine = !!c.my_lines && myLines.length > 0;
  const lines = mine ? myLines : mapLines;
  const label = (l) => {
    const o = own.find((x) => x.file.toLowerCase() === l.name.toLowerCase());
    if (o) return `${o.number} ${o.caption}`.trim();
    return l.termini.length ? `${l.name} - ${l.termini.join(' - ')}` : l.name;
  };
  const line = curLine();
  const tour = curTour();
  const tours = line ? [...line.tours].sort((a, b) => b.runs - a.runs || natural(a.number, b.number)) : [];
  const cards = tours.map((t) => ({ key: [t.number], cells: [t.number, hhmm(t.trips[0]?.departure ?? t.first), hhmm(t.trips[t.trips.length - 1]?.arrival ?? t.last)], clock: false, note: t.runs ? '' : t.days, trips: t.trips.map((x, k) => ({ ...x, stops: x.stops.length, pause: k ? Math.max(0, x.departure - t.trips[k - 1].arrival) : 0 })) }));
  const chosen = tour ? tours.findIndex((t) => t.number === tour.number) : -1;
  const first = tour ? (pickedTrip() != null ? Math.max(0, tour.trips.findIndex((x) => x.index === pickedTrip())) : Math.max(0, tripAt(tour, c.time * 60))) : null;
  const pickTour = (k) => {
    const t = tours[k];
    if (!t || t.number === tour?.number) return;
    const patch = { tour: t.number, start_trip: null };
    if (!t.runs && t.next_run && !realDate()) patch.date = t.next_run;
    choose(patch);
  };
  return html`
    <div class="hub-pad" style="padding-bottom:12px">
      <div class="hub-caps" style="align-items:center"><b>${tr('Line')}</b>
        <${Seg} value=${mine ? 'mine' : 'map'} options=${[['map', tr('Map lines (%{n})', { n: mapLines.length })], ['mine', tr('My lines (%{n})', { n: myLines.length })]]} onChange=${(v) => choose({ my_lines: v === 'mine' })} />
      </div>
      ${!lines.length
        ? html`<p style="color:var(--hub-text-dim)">${tr(s.lines.loading ? 'Reading the timetable…' : mine ? 'Make one in Editor → Line editor' : 'No lines on this date.')}</p>`
        : html`<${Select} raw value=${line && lines.includes(line) ? line.name : ''} placeholder=${tr('Choose a line…')} options=${lines.map((l) => [l.name, label(l)])} onChange=${(name) => {
            const l = lines.find((x) => x.name === name);
            choose({ line: name, tour: (l?.tours.find((x) => x.runs) || l?.tours[0])?.number ?? null, start_trip: null });
          }} />`}
    </div>
    <${List} heads=${['Tour', 'Departure', 'Arrival']} cards=${cards} chosen=${chosen >= 0 ? chosen : null} first=${chosen >= 0 ? first : null} onPick=${pickTour}
      onTrip=${(k) => tour && pickTrip(tour.trips[k])}
      empty=${!line ? 'Pick a line first. As in OMSI, the start time and date then say where in the tour the bus is: the trip under way, or the next to leave.' : 'Reading the timetable…'} />`;
}

export function DutyStep() {
  const s = useStore();
  const c = s.choice;
  const composed = !!c.composed;
  const line = chosenLine();
  const what = tr(composed ? 'Shifts' : 'Tours');
  const title = line ? `${what} · ${line}` : what;
  const sub = tr(composed ? 'Select a shift to continue' : 'Pick a line, a tour and the trip to start with');
  // the foot: the line, and how many duties there are to choose from (`shiftsheet::foot`)
  let foot;
  if (s.lines.loading && !s.lines.list) foot = tr('Reading the timetable…');
  else if (composed) {
    const n = s.hubShifts?.list?.length ?? 0;
    foot = (line ? `${line} · ` : '') + (n === 1 ? tr('one shift available') : tr('%{n} shifts available', { n }));
  } else if (curLine()) {
    const n = curLine().tours.filter((t) => t.runs).length;
    foot = (line ? `${line} · ` : '') + (n === 1 ? tr('one tour on this day') : tr('%{n} tours on this day', { n }));
  } else {
    const n = (s.lines.list || []).filter((l) => l.user_allowed).length;
    foot = n === 1 ? tr('one line on this day') : tr('%{n} lines on this day', { n });
  }
  const tour = curTour();
  const trip = composed
    ? (() => {
        const f = s.hubShift?.trips?.[0];
        return f && s.lines.list?.find((l) => l.name === f.tour_line)?.tours.find((t) => t.number === f.tour)?.trips.find((x) => x.index === f.index);
      })()
    : tour?.trips[pickedTrip() != null ? Math.max(0, tour.trips.findIndex((x) => x.index === pickedTrip())) : Math.max(0, tripAt(tour, c.time * 60))];
  return html`
    <${MapBehind} trip=${trip || null} />
    <section class="hub-sheet narrow">
      <${Head} icon="schedule" title=${title} raw sub=${sub} />
      ${composed ? html`<${Shifts} c=${c} />` : html`<${Tours} c=${c} />`}
      <div class="hub-pad" style="padding-top:10px;padding-bottom:12px;border-top:1px solid var(--hub-hairline)"><${EntrySelect} /></div>
      <${Foot} text=${foot} />
    </section>
    <${Actions} back onBack=${() => goStep(previousOf('duty'))} from=${'calc(var(--hub-edge-in) + clamp(288px, 27.4vw, 420px) + 14px)'}
      extra=${composed ? [{ label: 'Find other shifts', onClick: () => ((S.hubSeed = (S.hubSeed * 1103515245 + 12345) % 2147483647), set({ hubShifts: null })) }] : []}
      main="Next step" onMain=${() => goStep(nextOf('duty'))} />
  `;
}

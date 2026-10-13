// The map beside a step's sheet, as our game-drawn launcher lays the steps of the day and the
// duty on it (`flow::step_on_map`): the city's roads drawn on the ground itself, the duty's route
// in the accent, framed in what the sheet leaves of the window. Upstream's route map
// (`routemap.js`), in our colours.

import { html } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { RouteMap } from '../../routemap.js';
import { hms } from '../../pages/drive.js';
import { dark } from '../theme.js';

/** The map's colours on our ground: casing, road, main road, route. */
export function mapColors() {
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--hub-accent').trim() || '#f58620';
  return dark() ? ['#06102a', '#24345a', '#2d4170', accent] : ['#cfc6ba', '#ffffff', '#ffffff', accent];
}

/** The map with `trip`'s route (its stops with their times), the entry points to click. */
export function MapBehind({ trip, entry, onEntry, wide }) {
  const s = useStore();
  const c = s.choice;
  if (!c?.map) return null;
  const stops = trip ? trip.stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name })) : [];
  return html`<div class=${'hub-map' + (wide ? ' wide' : '')}>
    <${RouteMap} map=${c.map} date=${c.date} trip=${trip?.name || ''} stops=${stops} entry=${entry ?? c.entry} onEntry=${onEntry} height=${0} colors=${mapColors()} />
  </div>`;
}

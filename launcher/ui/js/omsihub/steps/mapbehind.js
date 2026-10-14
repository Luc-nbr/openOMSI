// The map beside a step's sheet, as our game-drawn launcher lays the steps of the day and the
// duty on it (`flow::step_on_map`): the city's roads drawn on the ground itself, the duty's route
// in the accent, framed in what the sheet leaves of the window. Upstream's route map
// (`routemap.js`), in our colours.

import { html, useState } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { RouteMap } from '../../routemap.js';
import { hms } from '../../pages/drive.js';
import { dark } from '../theme.js';

/** The map's colours on our ground: casing, road, main road, route. */
export function mapColors() {
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--hub-accent').trim() || '#f58620';
  return dark() ? ['#06102a', '#24345a', '#2d4170', accent] : ['#cfc6ba', '#ffffff', '#ffffff', accent];
}

/** The roundest lengths the scale bar takes (`shiftsheet::scale_of`). */
const NICE = [5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000, 200000];

/** The map's scale in the window's bottom left corner (`shiftsheet::scale_bar`, phase 6): a round
 * length about as long as a thumb - at most 160 points - and its name. */
function ScaleBar({ mpp }) {
  if (!(mpp > 0)) return null;
  const m = NICE.filter((x) => x / mpp <= 160).pop() || NICE[0];
  return html`<div class="hub-scalebar" style=${`width:${m / mpp}px`}><span>${m >= 1000 ? `${m / 1000} km` : `${m} m`}</span></div>`;
}

/** The map with `trip`'s route (its stops with their times), the entry points to click; with
 * `scale` its scale bar (the duty step's). */
export function MapBehind({ trip, entry, onEntry, wide, scale }) {
  const s = useStore();
  const c = s.choice;
  const [mpp, setMpp] = useState(0);
  if (!c?.map) return null;
  const stops = trip ? trip.stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name })) : [];
  return html`<div class=${'hub-map' + (wide ? ' wide' : '')}>
    <${RouteMap} map=${c.map} date=${c.date} trip=${trip?.name || ''} stops=${stops} entry=${entry ?? c.entry} onEntry=${onEntry} height=${0} colors=${mapColors()} onView=${scale ? setMpp : undefined} />
  </div>
  ${scale && html`<${ScaleBar} mpp=${mpp} />`}`;
}

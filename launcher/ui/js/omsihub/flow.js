// The drive laid out as our game-drawn launcher lays it out (`flow.rs`): the ground under
// everything, a step bar along the top, each step's choice on a sheet - narrow on the left
// where the map lies beside it, wide where it does not - and the action that goes on in the
// bottom right corner, the same place on every step. The steps are the order a driver thinks
// in: who drives, how (a shift, a tour, or free), where, on which day, what, and with which bus.

import { html } from '../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../store.js';
import { call } from '../api.js';
import { start, curLine, curTour, pickedTrip } from '../data.js';
import { tripAt } from '../pages/drive.js';
import { tr } from './bits.js';
import { Home } from './home.js';
import { ProfileStep } from './steps/profile.js';
import { MapStep } from './steps/map.js';
import { StartStep } from './steps/start.js';
import { DayStep } from './steps/day.js';
import { DutyStep } from './steps/duty.js';
import { BusStep } from './steps/bus.js';

/** The steps: id, name and icon (`flow::STEPS`). */
export const STEPS = [
  ['profile', 'Profile', 'person'],
  ['mode', 'Mode', 'route'],
  ['map', 'Map', 'map'],
  ['start', 'Start point', 'location_on'],
  ['day', 'Day & weather', 'partly_cloudy_day'],
  ['duty', 'Duty', 'schedule'],
  ['bus', 'Bus', 'directions_bus'],
];
export const STEP_NAMES = STEPS.map((x) => x[0]);

/** The steps a way to drive goes through: a free drive has no duty to choose but a place to
 * start; a duty starts where its first trip does. */
export function stepsOf() {
  const free = !!S.choice?.free;
  return STEPS.filter(([id]) => (free ? id !== 'duty' : id !== 'start'));
}

const order = () => stepsOf().map((x) => x[0]);
export const nextOf = (id) => order()[order().indexOf(id) + 1] || null;
export const previousOf = (id) => order()[order().indexOf(id) - 1] || null;

/** A step to come can be gone to when the map is chosen (the launcher remembers the last
 * duty, so the map and the bus are often chosen already). */
export const canGo = () => !!S.choice?.map && !!S.maps?.some((m) => m.file === S.choice.map);

export function goStep(step) {
  set({ page: 'drive', hub: { ...S.hub, step, palette: false } });
}

/** The line the duty drives, as its plate shows it: a shift's first trip's, a tour's trip's,
 * the line's own name else. */
export function chosenLine() {
  const c = S.choice;
  if (!c || c.free) return null;
  if (c.composed) return S.hubShift?.trips?.[0]?.line || null;
  const tour = curTour();
  if (!tour) return c.line || null;
  const k = pickedTrip() != null ? tour.trips.findIndex((x) => x.index === pickedTrip()) : tripAt(tour, c.time * 60);
  return tour.trips[Math.max(0, k)]?.line || c.line;
}

/** Whether the duty chosen is a free drive: a free drive, a shift not chosen (or one that no
 * longer holds), a tour without a line (`state::duty`). */
export function freeDrive() {
  const c = S.choice;
  return !!c.free || (c.composed && !(c.legs || []).length) || (!c.composed && !c.line);
}

/** Start the duty as our launcher starts it (`state::duty`): upstream's duty with a shift's
 * parts, the bus ready `bus_lead` minutes before the first departure, the display font chosen
 * for the bus, and a free drive's own start point. */
export async function hubStart(extra = {}) {
  let ours;
  try {
    ours = await hubExtras();
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
  return start({ ...ours, ...extra });
}

/** What our duty adds to upstream's `duty()` (see `hubStart`). */
export async function hubExtras() {
  const c = S.choice;
  const free = freeDrive();
  const more = await call('hub_duty_extras', { bus: c.bus, date: c.date, minutes: c.time, free });
  return {
    time: more.time,
    date: more.date,
    display_font: more.display_font || null,
    legs: !free && c.composed ? c.legs : [],
    ...(free ? { line: null, tour: null, trip: null, whole_tour: false, entry: c.free ? c.free_entry ?? -1 : c.entry } : {}),
  };
}

const VIEWS = { profile: ProfileStep, mode: Home, map: MapStep, start: StartStep, day: DayStep, duty: DutyStep, bus: BusStep };

export function Flow() {
  const s = useStore();
  if (!s.choice) return null;
  let step = s.hub.step;
  // (a step the way to drive has not: the one it has in its place)
  if (step === 'duty' && s.choice.free) step = 'start';
  if (step === 'start' && !s.choice.free) step = 'duty';
  const View = VIEWS[step] || Home;
  return html`<div class="page hub-flow" key=${step}><${View} /></div>`;
}

/** The line under a step that waits for the content. */
export const reading = () => tr('Reading the timetable…');

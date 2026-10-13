// RosterFlow's tutorial (`crates/omsi-launcher-ui/src/company/planning/rf_tutorial.rs` and
// RosterFlow's part of `tour.rs`; Luc: "build a tutorial for the new RosterFlow: I no longer
// understand how it works, let alone other players"). Three parts, all from the "?" in
// RosterFlow's title bar:
//
// * The guided tour: a spotlight and a bubble over RosterFlow - what it is for (the chain from
//   the timeplan to the roster, and SAVE), how to read its screen, and each tab with its main
//   controls lit. It starts by itself the first time a driver opens RosterFlow (once its opening
//   has played), and the "?" brings it back, from where it was left or from any of its chapters.
// * The lessons ("Try it yourself"): short tasks done on the player's own company. Each step
//   has its goal in one line and ticks itself off once the player has done it (on what RosterFlow
//   shows: `probe`); the guide rings the part the step is about without taking the page from the
//   player. Before a lesson the planning is kept (`rf_tut_act` "begin"): "End lesson" keeps the
//   changes or undoes the lesson.
// * The help sheet ("RosterFlow in 2 minutes"): the chain, the words with a small drawing of each
//   chip and symbol, the shortcuts, and what to do when something is red.
//
// What a driver has seen is kept in `~/.openomsi/rosterflow-tour.json` (`rf_tut`), the file our
// game-drawn launcher keeps it in.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { pal, alpha } from './pal.js';
import * as D from './draw.js';
import { CHIP } from './draw.js';
import { setRf, setPart, rfCmd, after, bump, ROSTER, VEHICLES, DRIVERS, TIMEPLAN } from './state.js';
import { Icon } from './ui.js';
import { anchored } from './anchors.js';
import { drawMark } from './brand.js';

const tutSet = (patch) => setPart('tut', patch);

// --- the guided tour's stops -------------------------------------------------------------------------

/** RosterFlow's tour (`tour::FLOW_STOPS`): its chapter, title and words (the keys of
 * `rosterflow-tutorial.yml`), what it points at (a part's name; "a|b" the first drawn; a card's
 * drawing), and where RosterFlow is for it (a tab, the timeplan's screen). */
export const STOPS = [
  ['Guided tour', 'Welcome to RosterFlow', "RosterFlow plans your company's day: when the buses run, which bus drives which trips, and which driver drives which bus. It is a chain of four steps, and nothing counts until you save. Let's walk along it.", 'card:chain', VEHICLES],
  ['What RosterFlow is', '1. Timeplan: when the buses run', 'Here a line gets its timetable: the minutes a bus needs from stop to stop, and how often a bus leaves. Every other step is built from these trips.', 'rf-tab-timeplan', VEHICLES],
  ['What RosterFlow is', '2. Vehicles: which bus drives which trips', "The day's trips are shared out among your buses. One bus's day of trips is called a block. A good block keeps the bus busy and has few empty runs.", 'rf-tab-vehicles', VEHICLES],
  ['What RosterFlow is', '3. Drivers: who drives', "A bus's day is too long for one driver, so each block is cut into duties: one driver's working day. A cut can only be made where a driver can get on or off: at a terminus or at the depot.", 'rf-tab-drivers', VEHICLES],
  ['What RosterFlow is', '4. Roster: which driver, which day', 'Here your drivers get their duties and your buses their blocks, weekday by weekday. What you plan for a Monday repeats every Monday.', 'rf-tab-roster', VEHICLES],
  ['What RosterFlow is', 'Then SAVE', "Your changes in Vehicles and Drivers are a draft. SAVE makes them the company's planning: the roster, the day and the game run it. DISCARD throws the draft away and goes back to what was saved.", 'rf-save-discard', VEHICLES],
  ['Reading the screen', 'The title bar', "On the left: the rules (preferences), printing, search by ID and the optimiser's settings. In the middle your company and the day, with a dot: green is saved, orange means there are changes not saved yet.", 'rf-menu', VEHICLES],
  ['Reading the screen', 'Undo, redo and history', 'Every change can be undone (Ctrl+Z) and done again (Ctrl+Y). The clock lists all your changes, so you can go back to any one of them.', 'rf-undo', VEHICLES],
  ['Reading the screen', 'The day you plan', "Choose the day here. A day's planning holds for that weekday every week: change a Tuesday and every Tuesday changes.", 'rf-day', VEHICLES],
  ['Reading the screen', 'How many duties and buses', "Duties: how many drivers' working days the day needs. Blocks: how many buses drive; PVR is how many are out at the busiest moment. Fewer is cheaper, as long as every trip is driven.", 'rf-kpi-counts', VEHICLES],
  ['Reading the screen', 'Efficiency and cost', "Vehicle efficiency: the share of the buses' kilometres that carry passengers. Crew efficiency: the share of paid time spent driving. Higher is better for both. After a change, green means better and red means worse. The small tab underneath shows more figures.", 'rf-kpi-rest', VEHICLES],
  ['Reading the screen', 'Stats, Layers and Filter', "Stats adds columns of figures beside the rows; click a column's heading to sort. Layers chooses what the chips say: the line, the times or the stops. Filter shows only some lines, one direction or part of the day.", 'rf-tools', VEHICLES],
  ['Reading the screen', 'OPTIMIZE', 'RosterFlow makes the blocks and duties for you and shows them as a proposal; the figures compare it with yours, and you accept it or keep yours. Locked rows stay as they are. The gear beside it chooses buses, duties or both.', 'rf-optimise', VEHICLES],
  ['Reading the screen', 'A row', "Each row is one bus's block (in Drivers: one duty). On the left its ID and a tick box. Hover over the ID for the lock (OPTIMIZE leaves the row alone) and the pin (keeps it at the top).", 'rf-row', VEHICLES],
  ['Reading the screen', 'A trip', "Each coloured chip is one trip with passengers, in its line's colour. The pointed end shows the way it runs: right is outbound, left is inbound. Click it to see its card; drag it to move it.", 'rf-chip', VEHICLES],
  ['Reading the screen', 'The other symbols', 'Grey-blue boxes take the bus out of the depot and back. Small dark boxes are empty runs; an orange dot means their time is a guess. Arrows mark a change of driver. Split is a long unpaid break, W, P or R a driver on the way to the next bus, and a red "!" a broken rule.', 'card:symbols', VEHICLES],
  ['Reading the screen', 'Is every trip covered?', 'How many rows are shown, and how many trips have a bus and a driver. The numbers are red until every trip has both.', 'rf-status-counts', VEHICLES],
  ['Reading the screen', 'Issues and zoom', 'The issues: Global, Duty and Vehicle. Click one to see what is wrong and jump to it. On the right the zoom; Ctrl and the mouse wheel zoom the hours, Shift and the wheel move along the day.', 'rf-status-issues', VEHICLES],
  ['Vehicles', 'The stack', 'Trips that no bus drives wait here, above the rows. Click one, then one of the slots that appear, to give it to a bus.', 'rf-stack', VEHICLES],
  ['Vehicles', 'Moving trips', 'Drag a chip, or double-click it, to pick it up. Every bus it could go to shows a slot with stars: five is the cheapest, and blue means no rule is broken. Hover over a slot to see the cost, click to drop. Right-click a chip for more.', 'rf-gantt', VEHICLES],
  ['Vehicles', 'Where drivers change', 'The arrows mark where one driver hands the bus to the next; click them to join the two duties again. Hover over a gap between two trips: where a blue line shows, a click cuts a new duty there.', 'rf-relief|rf-row', VEHICLES],
  ['Vehicles', 'A new vehicle', 'Drop trips here to give them to an extra bus.', 'rf-new-row', VEHICLES],
  ['Drivers', 'A duty', "In Drivers each row is one driver's working day. It starts with the sign-on and ends with the sign-off (the T-shaped marks). In between are the pieces of work: the parts on one bus.", 'rf-row', DRIVERS],
  ['Drivers', 'Everything about a duty', "Click a duty's ID to open its panel: its type, sign-on and sign-off, paid time, breaks, pieces of work and issues. You can lock it there, or add custom work such as office time.", 'rf-row-id', DRIVERS],
  ['Drivers', 'Building a duty', 'Drag trips here to start a duty of their own, or click for a spare duty. Drag a trip onto another duty and its driver drives it, on the same bus; Alt and a drag takes the whole piece of work.', 'rf-new-row', DRIVERS],
  ['Roster', 'The week', 'Pick a day of the coming week. Your plan for a weekday repeats every week.', 'rf-roster-week', ROSTER],
  ['Roster', 'Who drives what', "Each line's tours with their duties. Drag a driver or a bus from the list onto a duty or a tour, or click one and then the other.", 'rf-roster-chart', ROSTER],
  ['Roster', 'Drivers and buses', 'Your drivers and buses for the day. Click a duty or a tour in the chart to see who could take it.', 'rf-roster-side', ROSTER],
  ['Roster', 'Fill, clear, repeat', 'Fill the roster gives free drivers and buses to what is still open. Clear the day empties it, and Repeat copies it to the other days.', 'rf-roster-tools', ROSTER],
  ['Timeplan', 'Line and days', 'Choose the line and the days: Monday to Friday, Saturday or Sunday. OUTBOUND and INBOUND switch the direction, and the icons on the right switch between running times and the timetable.', 'rf-tp-bar', TIMEPLAN, 0],
  ['Timeplan', 'Running times', 'The minutes a bus needs between two stops, in time bands: the rush hour can be slower than the evening. Click a box and type a number; every trip in that band moves with it.', 'rf-tp-body', TIMEPLAN, 0],
  ['Timeplan', 'The timetable', 'Every trip is a row with its time at each stop. Add trips one at a time or by headway: a first and a last departure and every how many minutes. A time typed by hand shows orange.', 'rf-tp-body', TIMEPLAN, 1],
  ['Timeplan', 'Save, then Refresh', "Save writes the line's timetable. The status bar then says UNSYNCED - Refresh: click it, and the buses' blocks and the duties are made again for the new trips.", 'rf-tp-save', TIMEPLAN, 1],
  ['Guided tour', 'Ready to plan', 'This question mark brings the tour back. It also has short lessons where you try each task yourself, and a one-page help sheet.', 'rf-help', null],
];

/** The chapters of the tour, each with the stop it starts at (`tour::flow_chapters`). */
export function chapters() {
  const out = [];
  STOPS.forEach((s, k) => {
    if (k === 0 || k === STOPS.length - 1 || out.some((c) => c[0] === s[0])) return;
    out.push([s[0], k]);
  });
  return out;
}

// --- the lessons -----------------------------------------------------------------------------------------

const OPEN_VEHICLES = ['Open the Vehicles tab', "Each row there is one bus's day of trips.", 'rf-tab-vehicles', ['view', VEHICLES]];

/** The lessons in the menu's order (`rf_tutorial::LESSONS`): id, title, steps (goal, hint, the
 * part, what ticks it off). */
export const LESSONS = [
  ['move-trip', 'Move a trip to another bus', [
    OPEN_VEHICLES,
    ['Pick up a trip', 'Drag a coloured chip, or double-click it. Slots appear on every bus it could go to.', 'rf-chip', ['holdTrips']],
    ['Read the stars and the cost', 'Hold the mouse over a slot: five stars is the cheapest choice, blue breaks no rule, and the card shows what it costs.', 'rf-gantt', ['hoverZone']],
    ['Drop it on another bus', 'Click a slot, or let go of the chip over it. Escape puts it back.', 'rf-gantt', ['edited']],
    ['See the figures change', 'The green or red numbers beside the figures show what your move did: green is better, red is worse.', 'rf-kpi-rest', ['info']],
    ['Undo it', 'Press Ctrl+Z, or the undo arrow at the top. The trip goes back to its bus.', 'rf-undo', ['undone']],
  ]],
  ['cut-join', 'Cut a duty and join it again', [
    ['Open the Vehicles tab', 'Duties are cut from the buses\' days, so you start here.', 'rf-tab-vehicles', ['view', VEHICLES]],
    ['Cut a duty at a terminus', 'Hold the mouse over the gap between two trips. Where a blue line shows, a driver can take over: click to cut there.', 'rf-row', ['dutiesUp']],
    ['Look at it in Drivers', "There is one duty more now: the second driver's day starts where you cut.", 'rf-tab-drivers', ['view', DRIVERS]],
    ['Go back to Vehicles', "The arrows on the bus's row mark your cut.", 'rf-tab-vehicles', ['view', VEHICLES]],
    ['Join the two duties again', 'Click the arrows where the drivers change. The two duties become one.', 'rf-relief', ['dutiesDown']],
  ]],
  ['build-duty', 'Build a duty by hand', [
    ['Open the Drivers tab', "Each row there is one driver's working day.", 'rf-tab-drivers', ['view', DRIVERS]],
    ['Pick up a trip of a duty', 'Drag a chip, or double-click it. Slots appear on every duty it could go to, and on New duty. Shift or Ctrl and a click choose more trips; Alt and a drag takes the whole piece of work on that bus.', 'rf-chip', ['holdPiece']],
    ['Drop it on another duty', "Click a slot, or let go over it: that duty's driver drives the trip now, on the same bus, and the first duty is cut around it. No slot? The duty would break a rule, or the drivers could not change at a terminus there.", 'rf-gantt', ['edited']],
    ['Open a duty', "Click a duty's ID on the left. Its panel shows what the duty is made of.", 'rf-row-id', ['panelDuty']],
    ['Read sign-on, sign-off and breaks', 'Sign-on and sign-off are when the driver starts and stops. The breaks are between the pieces of work; a long unpaid one is a split.', 'rf-duty-panel', ['info']],
    ['Lock the duty', 'Click the lock at the top of the panel. OPTIMIZE will leave this duty as it is.', 'rf-duty-lock', ['lockedUp']],
  ]],
  ['fix-issue', 'Fix an issue', [
    ['Open Vehicles or Drivers', 'The issues are counted in the status bar at the bottom of both.', 'rf-tab-vehicles', ['studio']],
    ['Open the issues', 'Click Duty Issues or Vehicle Issues in the status bar.', 'rf-status-issues', ['panelIssues']],
    ['Jump to an issue', 'Click an issue in the list: RosterFlow shows its row.', 'rf-issues-panel', ['jumped']],
    ['Fix the issue', 'Move a trip, cut or join a duty, or change the rules under Preferences. If it is fine as it is, tick it and dismiss it.', 'rf-gantt', ['issuesDown']],
  ]],
  ['optimise', 'Let OPTIMIZE propose', [
    ['Open Vehicles or Drivers', 'OPTIMIZE is on the toolbar of both.', 'rf-tab-vehicles', ['studio']],
    ['Click OPTIMIZE', 'RosterFlow works out the blocks and duties by itself. Locked rows stay as they are.', 'rf-optimise', ['proposal']],
    ['Compare the figures', 'The figures now show the proposal against your planning: green where it is better, red where it is worse.', 'rf-kpi-rest', ['info']],
    ['Accept it or keep yours', 'Accept puts the proposal in your draft; Keep mine forgets it.', 'rf-proposal', ['proposalGone']],
  ]],
  ['timeplan', 'Plan a timetable', [
    ['Open the Timeplan tab', 'Here a line gets its running times and its timetable.', 'rf-tab-timeplan', ['view', TIMEPLAN]],
    ['Show the running times', 'Click the clock icon on the right of the bar.', 'rf-tp-bar', ['tpScreen', 0]],
    ['Change a running time', 'Click a box with minutes in a time band, type a new number and press Enter. Every trip in that band moves with it.', 'rf-tp-body', ['tpChanged']],
    ['Show the timetable', 'Click the grid icon on the right of the bar. Each row is a trip, with its time at every stop.', 'rf-tp-bar', ['tpScreen', 1]],
    ['Add trips by headway', 'Click the clock tool above the table. Give the first and the last departure - in a quiet part of the day, such as the late evening - and the minutes between buses, then the tick.', 'rf-tp-add-ok|rf-tp-add', ['tpTripsUp']],
    ['Save the timetable', "Save writes the line's new timetable. If it asks you to mend something first, two trips leave at the same minute: undo them (the arrow in the bar) and add them at other times.", 'rf-tp-save', ['tpSaved']],
    ['Refresh the blocks', "Click UNSYNCED - Refresh in the status bar: the buses' blocks and the duties are made again for the new trips.", 'rf-tp-refresh', ['tpRefreshed']],
  ]],
  ['colour', 'Give a line its own colour', [
    OPEN_VEHICLES,
    ['Open the line\'s colour', 'Right-click a trip and choose Line colour.', 'rf-chip', ['panelColour']],
    ['Pick a colour', 'Click a colour: every chip of the line takes it at once. Automatic gives it back its old one.', 'rf-colour', ['colourChanged']],
  ]],
  ['save-discard', 'SAVE and DISCARD', [
    OPEN_VEHICLES,
    ['Make a change', 'Move a trip or cut a duty. The dot at the top turns orange: there are changes not saved.', 'rf-gantt', ['draft']],
    ['DISCARD it', 'DISCARD forgets your changes and goes back to what was saved.', 'rf-discard', ['discarded']],
    ['Make a change again', 'Any change will do.', 'rf-gantt', ['draft']],
    ['SAVE it', "SAVE makes it the company's planning: the roster, the day and the game run it now. The dot turns green.", 'rf-save', ['saved']],
  ]],
];

/** What a lesson needs first, said instead of beginning it. */
const NO_TRIPS = 'This day has no trips yet. Give the company a line first (on the Lines page), then come back.';
const ONE_BUS = 'Moving a trip needs at least two buses on the day. Add a line or more trips first, then come back.';
const NOTHING_RED = 'Nothing is red right now: every rule is kept. Come back when a number shows beside the issues in the status bar.';

/** What RosterFlow shows now, as far as the lessons look (`rf_tutorial::probe`). */
export function probe() {
  const r = S.rf;
  const d = r.data;
  const b = d?.board;
  const h = r.hand;
  return {
    view: r.view,
    loaded: !!b,
    holdingTrips: h?.kind === 'trips' || h?.kind === 'stack',
    holdingPiece: h?.kind === 'piece' || h?.kind === 'duty_trips',
    zoneHover: r.zoneHover != null,
    undo: d?.head?.undo?.length || 0,
    redo: d?.head?.redo || 0,
    edits: d?.head?.edits || 0,
    blocks: b?.probe?.blocks || 0,
    duties: b?.probe?.duties || 0,
    locked: b?.probe?.locked || 0,
    issues: b?.probe?.issues || 0,
    panelDuty: r.panel?.kind === 'duty',
    panelIssues: r.panel?.kind === 'issues',
    panelColour: r.panel?.kind === 'colour',
    selRow: r.selRow,
    proposal: !!d?.proposal,
    draft: !!d?.head?.draft,
    savedSame: d?.saved_same !== false,
    colours: JSON.stringify(d?.colours || []),
    tp: { ready: false, why: null, screen: r.tp.tab, dirty: false, changes: 0, trips: 0, unsynced: false, ...(r.tpProbe || {}), screenShown: r.tp.tab },
  };
}

/** Whether check `c` is met by `p`, against `s` at the step's beginning (`rf_tutorial::done`). */
export function done([c, arg], p, s) {
  switch (c) {
    case 'info':
      return false;
    case 'view':
      return p.view === arg;
    case 'studio':
      return p.view === VEHICLES || p.view === DRIVERS;
    case 'holdTrips':
      return p.holdingTrips;
    case 'hoverZone':
      return p.zoneHover;
    case 'edited':
      return p.edits > s.edits;
    case 'undone':
      return p.undo < s.undo || p.redo > s.redo;
    case 'dutiesUp':
      return p.loaded && p.duties > s.duties;
    case 'dutiesDown':
      return p.loaded && p.duties < s.duties;
    case 'holdPiece':
      return p.holdingPiece;
    case 'panelDuty':
      return p.panelDuty;
    case 'lockedUp':
      return p.locked > s.locked;
    case 'panelIssues':
      return p.panelIssues;
    case 'jumped':
      return p.selRow != null && p.selRow !== s.selRow;
    case 'issuesDown':
      return p.loaded && p.issues < s.issues;
    case 'proposal':
      return p.proposal;
    case 'proposalGone':
      return !p.proposal;
    case 'tpScreen':
      return p.view === TIMEPLAN && p.tp.ready && p.tp.screenShown === arg;
    case 'tpChanged':
      return p.tp.changes > s.tp.changes;
    case 'tpTripsUp':
      return p.tp.trips > s.tp.trips;
    case 'tpSaved':
      return s.tp.dirty && !p.tp.dirty && p.tp.unsynced;
    case 'tpRefreshed':
      return s.tp.unsynced && !p.tp.unsynced;
    case 'panelColour':
      return p.panelColour;
    case 'colourChanged':
      return p.colours !== s.colours;
    case 'draft':
      return p.draft;
    case 'discarded':
      return !p.draft && p.savedSame;
    case 'saved':
      return !p.draft && !p.savedSame;
    default:
      return false;
  }
}

/** Whether a lesson can be done on what RosterFlow shows (`rf_tutorial::needs`): undefined
 * while that is not known, null when it can, else what to do first. */
export function needs(id, p) {
  if (id === 'timeplan') return p.tp.ready ? null : p.tp.why ? p.tp.why : undefined;
  if (!p.loaded) return undefined;
  if (p.blocks === 0) return NO_TRIPS;
  if (id === 'move-trip' && p.blocks < 2) return ONE_BUS;
  if (id === 'fix-issue' && p.issues === 0) return NOTHING_RED;
  return null;
}

// --- the state of it ----------------------------------------------------------------------------------

/** The "?" menu opened or closed (`Tut::toggle_menu`). */
export function tutMenu() {
  const t = S.rf.tut;
  tutSet({ menu: !t.menu, sheet: false });
  if (!t.menu) rfCmd('rf_tut').then((seen) => tutSet({ seen })).catch(() => {});
}

/** Escape: the menu or the help sheet first, the tour then (`rf_tutorial::before`). */
export function tutEscape() {
  const t = S.rf.tut;
  if (t.menu || t.sheet) {
    tutSet({ menu: false, sheet: false });
    return true;
  }
  if (t.tour) {
    leaveTour(false);
    return true;
  }
  return false;
}

/** The tour from stop `at`. */
function startTour(at) {
  tutSet({ menu: false, sheet: false, lesson: null, tour: { plan: STOPS.map((_, k) => k), at: Math.max(0, Math.min(STOPS.length - 1, at)), forward: true, waited: 0, home: { view: S.rf.view, tab: S.rf.tp.tab } } });
}

/** The tour over (`tour::finish`): the player's place back, where it was left kept. */
function leaveTour(completed) {
  const t = S.rf.tut.tour;
  if (!t) return;
  const at = t.plan[t.at] ?? 0;
  tutSet({ tour: null });
  setRf({ view: t.home.view });
  setPart('tp', { tab: t.home.tab });
  rfCmd('rf_tut_act', { act: 'left', at, completed }).catch(() => {});
}

/** Begin lesson `id`: the company kept as it is now. */
async function beginLesson(id) {
  await rfCmd('rf_tut_act', { act: 'begin', lesson: id }).catch(() => {});
  const p = probe();
  const steps = LESSONS.find((l) => l[0] === id)[2];
  tutSet({ menu: false, sheet: false, lesson: { id, at: 0, done: steps.map(() => false), start: p, stage: 'working', checked: false } });
}

/** End the lesson on its way: its changes kept, or the company put back as it found it. */
async function endLesson(undo) {
  const l = S.rf.tut.lesson;
  tutSet({ lesson: null });
  if (!l) return;
  const r = await rfCmd('rf_tut_act', { act: 'end', undo }).catch(() => null);
  after(r);
  if (undo) setRf({ hand: null, zones: null, sel: [], card: null, menu: null, panel: null, rev: '' });
  bump();
  setPart('tp', (x) => ({ rev: (x.rev || 0) + 1 }));
}

// --- the overlay -----------------------------------------------------------------------------------------

/** The tutorial over RosterFlow: the tour started the first time, a lesson's guide, the menu, the
 * help sheet, the tour (`rf_tutorial::after`). */
export function Tutorial({ p, size }) {
  const r = S.rf;
  const t = r.tut;
  // (the tour by itself the first time a driver opens RosterFlow, once its opening has played;
  // not over a dialog or the company's question)
  useEffect(() => {
    if (r.intro || t.tour || t.lesson || S.co.dialog || S.co.shell?.asking || S.boot?.selftest) return;
    const profile = S.boot?.config?.profile || '';
    if (!t.offer && t.asked === profile) return;
    const offer = t.offer;
    tutSet({ asked: profile, offer: false });
    rfCmd('rf_tut').then((seen) => {
      tutSet({ seen });
      if (seen?.seen) {
        if (offer) tutSet({ menu: true });
        return;
      }
      rfCmd('rf_tut_act', { act: 'seen' }).catch(() => {});
      startTour(0);
    }).catch(() => {});
  }, [r.intro, t.offer]);
  return html`${t.lesson && !t.tour && html`<${Guide} p=${p} size=${size} />`}
    ${t.menu && html`<${Menu} />`}
    ${t.sheet && html`<${Sheet} p=${p} />`}
    ${t.tour && html`<${Tour} p=${p} size=${size} />`}`;
}

/** The menu under the "?" (`rf_tutorial::menu`): the tour (from where it was left, or a
 * chapter), the lessons with a tick when done, and the help sheet. */
function Menu() {
  const t = S.rf.tut;
  const help = anchored('rf-help');
  const seen = t.seen || {};
  const resume = seen.done || (seen.at || 0) + 1 >= STOPS.length ? 0 : seen.at || 0;
  const ref = useRef(null);
  useEffect(() => {
    const down = (e) => {
      if (ref.current && !ref.current.contains(e.target) && !e.target.closest?.('[data-rf="rf-help"]')) tutSet({ menu: false });
    };
    addEventListener('mousedown', down, true);
    return () => removeEventListener('mousedown', down, true);
  }, []);
  const x = help ? help.x : 200;
  const y = help ? help.y + help.h + 6 : 54;
  return html`<div class="rf-tutmenu" ref=${ref} style=${`left:${x}px;top:${y}px`}>
    <span class="head">${tr('Guided tour').toUpperCase()}</span>
    <button type="button" onClick=${() => startTour(resume)}><${Icon} name="route" size=${17} /><span class="cut">${tr(resume > 0 ? 'Go on with the tour' : 'Take the tour')}</span>${seen.done && html`<${Icon} name="check_circle" size=${17} class="ok" />`}</button>
    ${chapters().map(([name, at]) => html`<button type="button" class="sub" onClick=${() => startTour(at)}><${Icon} name="chevron_right" size=${17} /><span class="cut">${tr(name)}</span></button>`)}
    <span class="head">${tr('Try it yourself').toUpperCase()}</span>
    ${LESSONS.map(([id, title]) => html`<button type="button" onClick=${() => beginLesson(id)}><${Icon} name="school" size=${17} /><span class="cut">${tr(title)}</span>${(seen.lessons || []).includes(id) && html`<${Icon} name="check_circle" size=${17} class="ok" />`}</button>`)}
    <span class="head">${tr('Help').toUpperCase()}</span>
    <button type="button" onClick=${() => tutSet({ menu: false, sheet: true })}><${Icon} name="description" size=${17} /><span class="cut">${tr('RosterFlow in 2 minutes')}</span></button>
  </div>`;
}

// --- the spotlight ------------------------------------------------------------------------------------

const ROOM = 8;
const GAP = 18;
const MARGIN = 16;

/** The spotlight round a part: a little room round it, inside the page. */
function holeRound(part, size) {
  const x0 = Math.max(4, part.x - ROOM);
  const y0 = Math.max(4, part.y - ROOM);
  const x1 = Math.min(size.w - 4, part.x + part.w + ROOM);
  const y1 = Math.min(size.h - 4, part.y + part.h + ROOM);
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}

/** Where a bubble goes beside the spotlight (`tour::place`): under it if there is room, else
 * above, else to its right, else to its left - whole in the page. */
function placeBubble(hole, w, h, size) {
  const fx = (x) => Math.max(MARGIN, Math.min(Math.max(MARGIN, size.w - MARGIN - w), x));
  const fy = (y) => Math.max(MARGIN, Math.min(Math.max(MARGIN, size.h - MARGIN - h), y));
  const cx = fx(hole.x + hole.w / 2 - w / 2);
  const cy = fy(hole.y + hole.h / 2 - h / 2);
  const below = hole.y + hole.h + GAP + h <= size.h - MARGIN ? [cx, hole.y + hole.h + GAP, 'below'] : null;
  const above = hole.y - GAP - h >= MARGIN ? [cx, hole.y - GAP - h, 'above'] : null;
  const right = hole.x + hole.w + GAP + w <= size.w - MARGIN ? [hole.x + hole.w + GAP, cy, 'right'] : null;
  const left = hole.x - GAP - w >= MARGIN ? [hole.x - GAP - w, cy, 'left'] : null;
  const order = hole.x + hole.w < size.w * 0.4 ? [right, below, above, left] : [below, above, right, left];
  return order.find(Boolean) || [fx(hole.x + hole.w - 24 - w), fy(hole.y + hole.h - 24 - h), 'over'];
}

/** The guided tour: the dark with the spotlight round the stop's part, the bubble (or the card)
 * with its words and buttons; the keys go on (→, Enter), back (←) and out (Escape). */
function Tour({ p, size }) {
  const t = S.rf.tut.tour;
  const [, redraw] = useState(0);
  const stop = STOPS[t.plan[t.at]];
  const [chapter, title, text, target, view, screen] = stop;
  // (RosterFlow where the stop needs it: its tab, the timeplan's screen)
  useEffect(() => {
    const patch = {};
    if (view != null && S.rf.view !== view) Object.assign(patch, { view, hand: null, zones: null, sel: [], card: null, menu: null, panel: null });
    if (Object.keys(patch).length) setRf(patch);
    if (screen != null && S.rf.tp.tab !== screen) setPart('tp', { tab: screen });
  }, [t.at]);
  const card = target.startsWith('card:');
  let part = null;
  if (!card) for (const name of target.split('|')) part = part || anchored(name);
  // (a part not drawn: waited for a moment, then the stop is left out)
  useEffect(() => {
    if (card || part) return undefined;
    const k = setTimeout(() => {
      const x = S.rf.tut.tour;
      if (!x) return;
      let again = null;
      for (const name of target.split('|')) again = again || anchored(name);
      if (again) return redraw((n) => n + 1);
      const plan = x.plan.filter((_, i) => i !== x.at);
      if (!plan.length) return leaveTour(false);
      const at = x.forward ? Math.min(x.at, plan.length - 1) : Math.max(0, x.at - 1);
      tutSet({ tour: { ...x, plan, at } });
    }, 450);
    return () => clearTimeout(k);
  });
  // (the parts move while RosterFlow draws: looked at again now and then)
  useEffect(() => {
    const k = setInterval(() => redraw((n) => n + 1), 400);
    return () => clearInterval(k);
  }, []);
  const go = (cmd) => {
    const x = S.rf.tut.tour;
    if (!x) return;
    if (cmd === 'next') {
      if (x.at + 1 < x.plan.length) tutSet({ tour: { ...x, at: x.at + 1, forward: true } });
      else leaveTour(true);
    } else if (cmd === 'back') tutSet({ tour: { ...x, at: Math.max(0, x.at - 1), forward: false } });
    else leaveTour(false);
  };
  useEffect(() => {
    const k = (e) => {
      if (e.key === 'ArrowRight' || e.key === 'Enter') {
        e.preventDefault();
        e.stopImmediatePropagation();
        go('next');
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        e.stopImmediatePropagation();
        go('back');
      }
    };
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, []);
  const first = t.at === 0;
  const last = t.at + 1 === t.plan.length;
  const w = Math.max(160, Math.min(card ? 560 : 372, size.w - 2 * MARGIN));
  const hole = part ? holeRound(part, size) : { x: size.w / 2, y: size.h / 2, w: 0, h: 0 };
  // (the bubble's height: measured once it is drawn)
  const [bh, setBh] = useState(card ? 420 : 220);
  const bubble = useRef(null);
  useEffect(() => {
    const el = bubble.current;
    if (el && Math.abs(el.offsetHeight - bh) > 1) setBh(el.offsetHeight);
  });
  const [bx, by, side] = part ? placeBubble(hole, w, bh, size) : [(size.w - w) / 2, Math.max(MARGIN, (size.h - bh) / 2), 'over'];
  const next = first && !last ? 'Show me around' : last ? 'Start planning' : 'Next';
  return html`<div class="rf-tour">
    <div class=${'hole' + (part ? '' : ' point')} style=${`left:${hole.x}px;top:${hole.y}px;width:${hole.w}px;height:${hole.h}px`}></div>
    <div class=${'bubble ' + side + (card ? ' card' : '')} ref=${bubble} style=${`left:${bx}px;top:${by}px;width:${w}px`}>
      ${card && html`<${Picture} which=${target === 'card:chain' ? 0 : 1} p=${p} w=${w - 20} />`}
      <div class="in">
        <div class="top"><span class="chapter">${tr(chapter).toUpperCase()}</span><span class="count">${t.at + 1} / ${t.plan.length}</span></div>
        <i class="track"><i style=${`width:${((t.at + 1) / t.plan.length) * 100}%`}></i></i>
        <b class="title">${tr(title)}</b>
        <p>${tr(text)}</p>
        <div class="buttons">
          ${!last && html`<button type="button" class="hub-btn ghost" onClick=${() => go('skip')}>${tr('Skip')}</button>`}
          <span class="grow"></span>
          ${!first && html`<button type="button" class="hub-btn" onClick=${() => go('back')}><${Icon} name="chevron_left" size=${18} />${tr('Back')}</button>`}
          <button type="button" class="hub-btn primary" onClick=${() => go('next')}>${tr(next)}</button>
        </div>
      </div>
    </div>
  </div>`;
}

// --- a lesson's guide -------------------------------------------------------------------------------------

/** The lesson on its way (`rf_tutorial::lesson_frame` and `tour::guide`): its step looked at, the
 * ring round its part, its bubble in a corner clear of the part. */
function Guide({ p, size }) {
  const r = S.rf;
  const l = r.tut.lesson;
  const [, redraw] = useState(0);
  useEffect(() => {
    const k = setInterval(() => redraw((n) => n + 1), 300);
    return () => clearInterval(k);
  }, []);
  const def = LESSONS.find((x) => x[0] === l.id);
  const steps = def[2];
  const pr = probe();
  // (what it needs looked at once it is known; each step's check met: on to the next)
  useEffect(() => {
    let x = S.rf.tut.lesson;
    if (!x) return;
    let changed = false;
    if (!x.checked && x.stage === 'working') {
      const need = needs(x.id, pr);
      if (need !== undefined) {
        x = { ...x, checked: true, stage: need ? 'blocked' : 'working', why: need };
        changed = true;
      }
    }
    if (x.stage === 'working') {
      let at = x.at;
      const dn = [...x.done];
      let start = x.start;
      while (at < steps.length && steps[at][3][0] !== 'info' && done(steps[at][3], pr, start)) {
        dn[at] = true;
        at++;
        start = pr;
      }
      if (at !== x.at) {
        x = { ...x, at, done: dn, start };
        changed = true;
      }
      if (at >= steps.length) {
        x = { ...x, stage: 'done' };
        changed = true;
        rfCmd('rf_tut_act', { act: 'lesson_done', id: x.id }).catch(() => {});
      }
    }
    if (x.id === 'timeplan' && !x.tpKept && pr.tp.ready) {
      x = { ...x, tpKept: true, start: { ...x.start, tp: pr.tp } };
      changed = true;
      rfCmd('rf_tut_act', { act: 'keep_tp' }).catch(() => {});
    }
    if (changed) tutSet({ lesson: x });
  });
  const now = steps[Math.min(l.at, steps.length - 1)];
  let title;
  let text;
  let mode;
  if (l.stage === 'working') [title, text, mode] = [now[0], now[1], now[3][0] === 'info' ? 'info' : 'step'];
  else if (l.stage === 'ending') [title, text, mode] = ['End the lesson?', 'Keep what you changed, or put everything back as it was before the lesson.', 'ending'];
  else if (l.stage === 'done') [title, text, mode] = ['Well done!', 'You did every step of this lesson. Keep what you changed, or put it all back as it was.', 'done'];
  else [title, text, mode] = ['First this', l.why || '', 'blocked'];
  let target = null;
  if (l.stage === 'working') for (const name of now[2].split('|')) target = target || anchored(name);
  const room = { x: 0, y: 48, w: size.w, h: Math.max(120, size.h - 48 - 34) };
  const W = Math.max(200, Math.min(340, room.w - 2 * MARGIN));
  const ref = useRef(null);
  const [bh, setBh] = useState(260);
  useEffect(() => {
    const el = ref.current;
    if (el && Math.abs(el.offsetHeight - bh) > 1) setBh(el.offsetHeight);
  });
  // the corner: away from the part (bottom right, bottom left, top right, top left)
  const docks = [
    [room.x + room.w - MARGIN - W, room.y + room.h - MARGIN - bh],
    [room.x + MARGIN, room.y + room.h - MARGIN - bh],
    [room.x + room.w - MARGIN - W, room.y + MARGIN],
    [room.x + MARGIN, room.y + MARGIN],
  ];
  const over = (d) => target && d[0] < target.x + target.w && target.x < d[0] + W && d[1] < target.y + target.h && target.y < d[1] + bh;
  const corner = Math.max(0, docks.findIndex((d) => !over(d)));
  const [gx, gy] = docks[corner];
  const hole = target ? holeRound(target, size) : null;
  const set = (patch) => tutSet({ lesson: { ...S.rf.tut.lesson, ...patch } });
  const doneN = l.done.filter(Boolean).length;
  return html`${hole && html`<div class="rf-ring" style=${`left:${hole.x}px;top:${hole.y}px;width:${hole.w}px;height:${hole.h}px`}></div>`}
    <div class="rf-guide" ref=${ref} style=${`left:${gx}px;top:${gy}px;width:${W}px`}>
      <div class="top"><span class="chapter">${tr(def[1]).toUpperCase()}</span><span class="count">${doneN} / ${steps.length}</span></div>
      <i class="track"><i style=${`width:${(doneN / steps.length) * 100}%`}></i></i>
      <b class="title">${tr(title)}</b>
      ${text && html`<p>${tr(text)}</p>`}
      ${(mode === 'step' || mode === 'info' || mode === 'done') && html`<div class="steps">${steps.map((s, i) => {
        const on = i === l.at && mode !== 'done';
        return html`<div class=${'st' + (l.done[i] ? ' ok' : on ? ' now' : '')}><i>${l.done[i] && html`<${Icon} name="check" size=${12} />`}</i><span>${tr(s[0])}</span></div>`;
      })}</div>`}
      <div class="buttons">
        ${(mode === 'step' || mode === 'info') && html`<button type="button" class="hub-btn ghost" onClick=${() => set({ stage: 'ending' })}>${tr('End lesson')}</button><span class="grow"></span>
          ${mode === 'info' && html`<button type="button" class="hub-btn primary" onClick=${() => {
            const x = S.rf.tut.lesson;
            const dn = [...x.done];
            dn[x.at] = true;
            set({ at: x.at + 1, done: dn, start: probe() });
          }}>${tr('Got it')}</button>`}`}
        ${(mode === 'ending' || mode === 'done') && html`<div class="col">
          <button type="button" class="hub-btn primary" onClick=${() => endLesson(false)}><${Icon} name="check" size=${18} />${tr('Keep my changes')}</button>
          <button type="button" class="hub-btn" onClick=${() => endLesson(true)}><${Icon} name="livery_undo" size=${18} />${tr('Undo the lesson')}</button>
          ${mode === 'ending' && html`<button type="button" class="hub-btn ghost" onClick=${() => set({ stage: S.rf.tut.lesson.at >= steps.length ? 'done' : 'working' })}>${tr('Go on with the lesson')}</button>`}
        </div>`}
        ${mode === 'blocked' && html`<span class="grow"></span><button type="button" class="hub-btn primary" onClick=${() => endLesson(false)}>${tr('Close')}</button>`}
      </div>
    </div>`;
}

// --- the help sheet ------------------------------------------------------------------------------------------

/** The chain, as the sheet says it: the tab's icon, its name, what it is for. */
const CHAIN = [
  ['departure_board', 'Timeplan', "When the buses run: each line's running times and timetable."],
  ['directions_bus', 'Vehicles', "Which bus drives which trips. One bus's day is a block."],
  ['person', 'Drivers', 'Who drives. Blocks are cut into duties at a terminus or the depot.'],
  ['group', 'Roster', 'Which driver and which bus, for each weekday.'],
  ['save', 'Save', 'Nothing counts until you save. DISCARD goes back to what was saved.'],
];

/** The words, in plain words, each with its drawing. */
const GLOSSARY = [
  ['trip', 'Trip (a chip)', "One run of a bus from one end of the line to the other, with passengers. Shown as a chip in the line's colour."],
  ['direction', 'Direction', 'The pointed end of a chip shows the way the trip runs: to the right is outbound, to the left inbound.'],
  ['block', 'Block', "One bus's whole day: every trip it drives, from leaving the depot until it is back. Also called a vehicle schedule."],
  ['duty', 'Duty', "One driver's working day, from sign-on to sign-off."],
  ['piece', 'Piece of work', 'The part of a duty spent on one bus. A duty can have pieces on different buses.'],
  ['relief', 'Relief', 'Where one driver hands the bus to the next: always at a terminus or at the depot (the relief point).'],
  ['signon', 'Sign-on and sign-off', 'The start and the end of a duty, with a few minutes to check the bus.'],
  ['deadhead', 'Deadhead', 'A bus driving without passengers, for example to where its next trip starts. An orange dot means its time is a guess.'],
  ['pull', 'Pull-out and pull-in', 'The bus leaving the depot in the morning and going back at night.'],
  ['layover', 'Layover', 'The wait at the end of the line before the next trip.'],
  ['split', 'Split', 'A long unpaid break in the middle of a duty: the driver works early and again later.'],
  ['travel', 'Travel', 'How a driver gets to the next bus: W walking, P by public transport, R by road in a company car.'],
  ['slot', 'Drop slot', 'Where something picked up may go, with stars for how good it is: blue breaks no rule, red breaks one.'],
  ['stack', 'Stack', 'Trips no bus drives yet. They wait above the rows until you give them a bus.'],
  ['lock', 'Locked rows', 'A locked bus or duty stays as it is: OPTIMIZE and your other edits leave it alone.'],
  ['draft', 'Draft', 'Your changes before SAVE (orange dot). The game keeps running the saved planning (green dot) until you save.'],
  ['pvr', 'PVR', 'Peak vehicle requirement: the most buses out at the same time. That is how many buses the day needs.'],
  ['band', 'Time band', 'A part of the day with its own running times: the rush hour is often slower than the evening.'],
  ['running', 'Running time', 'The minutes a bus needs from one stop to the next.'],
  ['headway', 'Headway', 'Every how many minutes a bus leaves: a headway of 10 minutes is six buses an hour.'],
  ['pattern', 'Pattern', "One version of a line's route: the full run, or a short one that leaves out stops."],
  ['issue', 'Issue', 'A rule that is broken, such as a duty without a meal break. Red is a hard rule, orange a soft one that costs a penalty.'],
];

/** The shortcuts: the keys (true: words to translate) and what they do. */
const SHORTCUTS = [
  ['Click a chip', true, 'Choose the trip and open its card'],
  ['Double-click or drag', true, 'Pick it up to move it'],
  ['Shift + click', true, 'Choose more trips of one row'],
  ['Ctrl + click', true, 'Add one trip to the choice, or take it out'],
  ['Alt + drag', true, 'In Drivers: take the whole piece of work on that bus'],
  ['Right-click', true, 'Menu: move, swap, unassign, line colour and more'],
  ['Arrow keys', true, 'To the next trip; with Shift the choice grows'],
  ['G', false, 'Go to a bus or a duty by its ID'],
  ['P', false, 'Pin the chosen row to the top'],
  ['Delete', true, 'Put the chosen trips on the stack'],
  ['Ctrl + Z  /  Ctrl + Y', false, 'Undo / redo'],
  ['Ctrl + wheel', true, 'Zoom in and out on the hours'],
  ['Shift + wheel', true, 'Move along the day'],
  ['Escape', true, 'Let go, close what is open'],
];

/** What to do when something is red. */
const RED = [
  'Red numbers in the status bar: some trips have no bus or no driver. Look at the stack above the rows, or let OPTIMIZE fill the gaps.',
  'A red "!" by a row: hold the mouse over it to read which rule is broken. Click the issue counter in the status bar to see them all, and click one to jump to its row.',
  'A red slot while you move something: dropping there breaks a rule. You may still do it, but the day gets an issue.',
  'A red difference beside a figure: your last change made it worse. Ctrl+Z takes it back.',
  'More duties than drivers (a Global issue): hire drivers on the Staff page, or make fewer, longer duties.',
  'Orange is a soft rule: allowed, but it adds a penalty to the cost. Change the rules under Preferences (the first icon at the top).',
];

/** The palette's line colours the drawings use. */
const BLUE = [79, 129, 190, 1];
const PINK = [232, 82, 140, 1];
const ORANGE = [245, 128, 40, 1];

/** A word's drawing in `r` (x, y, w, h), in RosterFlow's symbols (`rf_tutorial::symbol`). */
export function symbol(ctx, p, sym, r) {
  const cy = r.y + r.h / 2;
  const x0 = r.x + 8;
  const x1 = r.x + r.w - 8;
  const chip = (a, b, c, right, label = '') => D.chip(ctx, a, cy - CHIP / 2, b - a, CHIP, c, right, label, null, false, false, 1);
  switch (sym) {
    case 'trip':
      return chip(x0 + 10, x1 - 10, BLUE, true, '1');
    case 'direction':
      chip(x0, x0 + 34, BLUE, true);
      return chip(x1 - 34, x1, BLUE, false);
    case 'block':
      D.pull(ctx, p, x0, x0 + 10, cy, false);
      chip(x0 + 13, x0 + 33, BLUE, true);
      chip(x0 + 36, x0 + 56, BLUE, false);
      chip(x0 + 59, x1 - 13, BLUE, true);
      return D.pull(ctx, p, x1 - 10, x1, cy, false);
    case 'duty':
      D.tBar(ctx, p, x0, x0 + 10, cy, true);
      chip(x0 + 12, x0 + 40, PINK, true);
      chip(x0 + 42, x1 - 12, PINK, false);
      return D.tBar(ctx, p, x1 - 10, x1, cy, false);
    case 'piece':
      chip(x0, x0 + 34, BLUE, true);
      D.reliefArrow(ctx, p, x0 + 40, cy, true, true);
      return chip(x0 + 46, x1, ORANGE, true);
    case 'relief':
      chip(x0, x0 + 28, BLUE, true);
      D.reliefArrow(ctx, p, x0 + 34, cy, false, true);
      D.reliefArrow(ctx, p, x1 - 34, cy, true, false);
      return chip(x1 - 28, x1, BLUE, false);
    case 'signon':
      D.tBar(ctx, p, x0 + 4, x0 + 24, cy, true);
      chip(x0 + 26, x1 - 26, PINK, true);
      return D.tBar(ctx, p, x1 - 24, x1 - 4, cy, false);
    case 'deadhead':
      chip(x0, x0 + 26, BLUE, true);
      D.deadhead(ctx, p, x0 + 30, x1 - 30, cy, true);
      return chip(x1 - 26, x1, BLUE, true);
    case 'pull':
      D.pull(ctx, p, x0, x0 + 18, cy, false);
      chip(x0 + 22, x1 - 22, BLUE, true);
      return D.pull(ctx, p, x1 - 18, x1, cy, false);
    case 'layover':
      chip(x0, x0 + 26, BLUE, true);
      D.textAt(ctx, '5m', r.x + r.w / 2, cy + 3.5, 9.5, 500, p.dim, 'center');
      return chip(x1 - 26, x1, BLUE, false);
    case 'split': {
      chip(x0, x0 + 16, PINK, true);
      const line = p.light ? p.dim : [220, 224, 230, 0.9];
      D.rect(ctx, x0 + 18, cy - 0.5, x1 - x0 - 36, 1, line);
      const word = tr('Split');
      const tw = D.width(ctx, word, 9.5, 500) + 6;
      D.rect(ctx, r.x + r.w / 2 - tw / 2, cy - 6, tw, 12, p.grid);
      D.textIn(ctx, word, r.x + r.w / 2 - tw / 2, cy - 6, tw, 12, 9.5, 500, line, 'center');
      return chip(x1 - 16, x1, PINK, false);
    }
    case 'travel':
      ['W', 'P', 'R'].forEach((m, k) => {
        const bx = x0 + 4 + k * 26;
        D.rounded(ctx, bx, cy - CHIP * 0.45, 20, CHIP * 0.9, 3, p.dead);
        D.roundedBorder(ctx, bx, cy - CHIP * 0.45, 20, CHIP * 0.9, 3, 1.3, p.warn);
        D.textAt(ctx, m, bx + 10, cy + 4, 10.5, 700, p.warn, 'center');
      });
      return null;
    case 'slot':
      D.zone(ctx, p, x0, cy - CHIP / 2 - 1, 36, CHIP + 2, { stars: 5, clean: true }, true, false);
      return D.zone(ctx, p, x1 - 36, cy - CHIP / 2 - 1, 36, CHIP + 2, { stars: 2, clean: false }, true, false);
    case 'stack':
      D.icon(ctx, 'inventory_2', x0 + 8, cy, 15, p.dim);
      return chip(x0 + 22, x1, ORANGE, true);
    case 'lock':
      D.icon(ctx, 'lock', x0 + 8, cy, 15, p.accent);
      return chip(x0 + 22, x1, BLUE, true);
    case 'draft':
      D.circle(ctx, r.x + r.w / 2 - 16, cy, 5, p.warn);
      return D.circle(ctx, r.x + r.w / 2 + 16, cy, 5, p.ok);
    case 'pvr':
      for (let k = 0; k < 3; k++) D.icon(ctx, 'directions_bus', r.x + r.w / 2 - 22 + k * 22, cy, 18, p.accent);
      return null;
    case 'band': {
      const mid = r.x + r.w / 2;
      D.rect(ctx, x0, cy - 9, mid - x0 - 2, 18, alpha(p.accent, 0.25));
      return D.rect(ctx, mid + 2, cy - 9, x1 - mid - 2, 18, alpha(p.accent, 0.55));
    }
    case 'running': {
      const bx = r.x + r.w / 2 - 20;
      D.rounded(ctx, bx, cy - 10, 40, 20, 3, p.field);
      D.roundedBorder(ctx, bx, cy - 10, 40, 20, 3, 1, p.line);
      return D.textAt(ctx, '7 m', bx + 20, cy + 4, 11, 700, p.text, 'center');
    }
    case 'headway':
      for (let k = 0; k < 5; k++) D.rect(ctx, x0 + 4 + k * ((x1 - x0 - 8) / 4) - 1, cy - 8, 2, 16, p.accent);
      return null;
    case 'pattern':
      D.rect(ctx, x0, cy - 6, x1 - x0, 3, BLUE);
      return D.rect(ctx, x0 + 18, cy + 4, x1 - x0 - 40, 3, alpha(BLUE, 0.6));
    default:
      D.textAt(ctx, '!', x0 + 6, cy + 5.5, 15, 800, p.err, 'center');
      D.textAt(ctx, '!', x0 + 18, cy + 5.5, 15, 800, p.warn, 'center');
      return chip(x0 + 28, x1, BLUE, true);
  }
}

/** A small canvas with a word's drawing on it. */
function SymbolBox({ sym, p }) {
  const ref = useRef(null);
  useEffect(() => {
    const ctx = D.prepare(ref.current, 96, 30);
    D.rounded(ctx, 0, 0, 96, 30, 4, p.grid);
    D.roundedBorder(ctx, 0, 0, 96, 30, 4, 1, p.line);
    symbol(ctx, p, sym, { x: 0, y: 0, w: 96, h: 30 });
  });
  return html`<canvas ref=${ref} style="width:96px;height:30px"></canvas>`;
}

/** The chain's names and the symbols' names on the tour's cards. */
const PICTURE_WORDS = ['Depot run', 'Sign-on', 'Trip', 'Empty run', 'Driver change', 'Split', 'Walk', 'Broken rule'];

/** A drawing for the tour's cards (0: the chain, 1: the symbols), in RosterFlow's own look
 * (`rf_tutorial::picture`); the chain with a light going along it. */
function Picture({ which, p, w }) {
  const ref = useRef(null);
  const h = 196 * Math.min(1, w / 540);
  useEffect(() => {
    let raf = 0;
    const t0 = performance.now();
    const still = S.settings?.animations === false;
    const paint = () => {
      const c = ref.current;
      if (!c) return;
      const ctx = D.prepare(c, w, h);
      D.rounded(ctx, 0, 0, w, h, 8, p.bg);
      const t = still ? null : (performance.now() - t0) / 1000;
      if (which === 0) chainPicture(ctx, p, { x: 0, y: 0, w, h }, t);
      else symbolsPicture(ctx, p, { x: 0, y: 0, w, h });
      if (!still && which === 0) raf = requestAnimationFrame(paint);
    };
    paint();
    return () => cancelAnimationFrame(raf);
  }, [which, w]);
  return html`<canvas ref=${ref} class="pic" style=${`width:${w}px;height:${h}px`}></canvas>`;
}

function chainPicture(ctx, p, r, t) {
  const inner = { x: r.x + 18, y: r.y + 18, w: r.w - 36, h: r.h - 36 };
  const gap = 22;
  const bw = (inner.w - 4 * gap) / 5;
  const bh = Math.min(120, inner.h - 20);
  const y = inner.y + (inner.h - bh) / 2;
  const lit = t == null ? null : Math.floor(t / 0.9) % 6;
  CHAIN.forEach(([icon, name], k) => {
    const x = inner.x + k * (bw + gap);
    const on = lit === k;
    const save = k === 4;
    const col = save ? p.ok : p.accent;
    D.rounded(ctx, x, y, bw, bh, 6, on ? alpha(col, 0.22) : p.grid);
    D.roundedBorder(ctx, x, y, bw, bh, 6, on ? 2 : 1, on ? col : p.line);
    D.circle(ctx, x + bw / 2, y + bh * 0.34, 17, col);
    D.icon(ctx, icon, x + bw / 2, y + bh * 0.34, 19, [255, 255, 255, 1]);
    D.textIn(ctx, tr(name), x + 2, y + bh * 0.62, bw - 4, 18, 12, 700, p.text, 'center');
    if (!save) D.textAt(ctx, String(k + 1), x + bw / 2, y + bh * 0.62 + 32, 11, 700, p.dim, 'center');
    if (k < 4) {
      const ax = x + bw + gap / 2;
      const ay = y + bh / 2;
      D.rect(ctx, ax - 7, ay - 1, 10, 2, p.dim);
      D.poly(ctx, [[ax + 2, ay - 5], [ax + 8, ay], [ax + 2, ay + 5]], p.dim);
    }
  });
}

function symbolsPicture(ctx, p, r) {
  const inner = { x: r.x + 14, y: r.y + 14, w: r.w - 28, h: r.h - 28 };
  const cw = inner.w / 4;
  const rh = inner.h / 2;
  for (let k = 0; k < 8; k++) {
    const cx = inner.x + (k % 4) * cw;
    const cyy = inner.y + Math.floor(k / 4) * rh;
    const d = { x: cx + 10, y: cyy + rh * 0.18, w: cw - 20, h: 30 };
    const cy = d.y + d.h / 2;
    const a = d.x + 6;
    const b = d.x + d.w - 6;
    switch (k) {
      case 0:
        D.pull(ctx, p, a + 10, b - 10, cy, false);
        break;
      case 1:
        D.tBar(ctx, p, a + 6, a + 30, cy, true);
        D.arrowChip(ctx, a + 32, cy - CHIP / 2, b - a - 38, CHIP, PINK, true);
        break;
      case 2:
        D.chip(ctx, a + 4, cy - CHIP / 2, b - a - 8, CHIP, BLUE, true, '1', null, false, false, 1);
        break;
      case 3:
        D.deadhead(ctx, p, a + 14, b - 14, cy, true);
        break;
      case 4:
        D.reliefArrow(ctx, p, d.x + d.w / 2 - 8, cy, false, true);
        D.reliefArrow(ctx, p, d.x + d.w / 2 + 8, cy, true, false);
        break;
      case 5:
        symbol(ctx, p, 'split', d);
        break;
      case 6: {
        const bx = d.x + d.w / 2 - 12;
        D.rounded(ctx, bx, cy - CHIP * 0.45, 24, CHIP * 0.9, 3, p.dead);
        D.roundedBorder(ctx, bx, cy - CHIP * 0.45, 24, CHIP * 0.9, 3, 1.3, p.warn);
        D.textAt(ctx, 'W', bx + 12, cy + 4, 10.5, 700, p.warn, 'center');
        break;
      }
      default:
        D.textAt(ctx, '!', d.x + d.w / 2, cy + 6, 18, 800, p.err, 'center');
    }
    D.textIn(ctx, tr(PICTURE_WORDS[k]), cx + 2, d.y + d.h + 6, cw - 4, 16, 11, 500, p.dim, 'center');
  }
}

/** The help sheet over RosterFlow (`rf_tutorial::sheet`): the chain, the words, the shortcuts,
 * what to do when something is red; two columns where there is room. */
function Sheet({ p }) {
  const ref = useRef(null);
  useEffect(() => {
    const ctx = D.prepare(ref.current, 34, 34);
    drawMark(ctx, 0, 0, 34);
  }, []);
  const chain = html`<section><b class="h">${tr('The chain').toUpperCase()}</b>${CHAIN.map(([icon, name, text], k) => html`<div class="chain"><span class=${'badge' + (k === 4 ? ' ok' : '')}><${Icon} name=${icon} size=${15} /></span><div><b>${tr(name)}</b><p>${tr(text)}</p></div></div>`)}</section>`;
  const words = html`<section><b class="h">${tr('Words').toUpperCase()}</b>${GLOSSARY.map(([sym, term, text]) => html`<div class="word"><${SymbolBox} sym=${sym} p=${p} /><div><b>${tr(term)}</b><p>${tr(text)}</p></div></div>`)}</section>`;
  const keys = html`<section><b class="h">${tr('Shortcuts').toUpperCase()}</b>${SHORTCUTS.map(([k, w, what]) => html`<div class="key"><span class="cap">${w ? tr(k) : k}</span><p>${tr(what)}</p></div>`)}</section>`;
  const red = html`<section><b class="h">${tr('If something is red').toUpperCase()}</b>${RED.map((tip, k) => html`<div class="red"><span class=${k === RED.length - 1 ? 'warn' : 'err'}>!</span><p>${tr(tip)}</p></div>`)}</section>`;
  return html`<div class="rf-sheetdim" onMouseDown=${(e) => e.target === e.currentTarget && tutSet({ sheet: false })}>
    <div class="rf-help">
      <div class="head"><canvas ref=${ref} style="width:34px;height:34px"></canvas><div><b>${tr('RosterFlow in 2 minutes')}</b><span>${tr('Plan your bus company\'s day in four steps, then save.')}</span></div>
        <button type="button" onClick=${() => tutSet({ sheet: false })}><${Icon} name="close" size=${20} /></button></div>
      <div class="cols"><div>${chain}${words}</div><div>${keys}${red}</div></div>
    </div>
  </div>`;
}

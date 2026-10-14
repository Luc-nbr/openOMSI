// The guided tour (Trello "Tauri 3/6": what phase 1 left), as our game-drawn launcher's
// (`crates/omsi-launcher-ui/src/tour.rs`, after Omsi-Hub's "rondleiding"): the window darkens
// except for a rounded spotlight round the part being talked about, and a speech bubble beside
// it - a small arrow at the spotlight - says what it is for in two or three sentences, with the
// way on, back and out (also the arrow keys, Enter and Escape). The spotlight and the bubble
// glide from stop to stop; with the setting "animations" off they are where they go at once.
//
// The tour walks the setup itself: for each stop it puts the launcher on the step (and in the
// way of driving) where that part is, and at the end - or when it is skipped - the player's own
// page, step and way of driving are back. It chooses nothing for the player: without a chosen
// map the stops that need one are left out. It never starts a game: while it runs the page
// under it gets no mouse and no keys. Its last stops are about the game itself - the navigator,
// the city map and signing on, the phone and the tablet - as cards in the middle with a drawing
// each (the game-drawn launcher's own, as pictures). A stop whose part is not there after all
// is left out as the tour comes to it. The parts are found by what this page draws (no layout
// of another module is worked out here).
//
// The bus company's tour goes the same way over its pages (Trello "Tauri 6/6": `COMPANY_STOPS`,
// `startCompanyTour`), what a driver has seen of it kept as the game-drawn launcher keeps it
// (`co_tour`); RosterFlow's is its own (phase 4).

import { html, useEffect, useLayoutEffect, useRef, useState } from '../../vendor/preact-htm.js';
import { S, set, useStore } from '../store.js';
import { choose, curMap, curBus } from '../data.js';
import { Icon, tr } from './bits.js';
import { goStep } from './flow.js';
import { HOME, CASUAL, setCo, cmd } from './company/kit.js';

let cssLoaded = false;
function loadCss() {
  if (cssLoaded) return;
  cssLoaded = true;
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/tour.css';
  document.head.append(sheet);
}

// --- the stops -------------------------------------------------------------------------------

/** The stops in their order (`tour::STOPS` of a desktop): its chapter, title and words, what it
 * points at (a card's drawing, or the part's name) and where the launcher has to be. */
const STOPS = [
  ['welcome', 'Guided tour', 'Welcome aboard', "Omsi-Hub puts real duties together from the timetables of your own OMSI maps and gets the game ready for you. In a minute you'll know where everything is.", { card: 'welcome' }, ['mode']],
  ['modes', 'The start', 'How do you want to drive?', "A work shift is put together from the timetable's real trips. A tour is one bus's trips on one line, as OMSI gives them. And a free drive is just a map and a bus.", 'mode-tiles', ['mode']],
  ['record', 'The start', 'Your record and the rest', 'On the left, your service record: what you have driven and how close the next level is. On the right, the way to everything else: mods, settings, controls, timetables and more.', 'start-links', ['mode']],
  ['languages', 'The start', 'Your language', 'A flag switches the language at once; the globe beside them has all the others. The game speaks it too.', 'bar-flags', ['mode']],
  ['steps', 'The map', 'Step by step', 'Up here are the steps, in the order a driver thinks: who drives, how, where, when, what and with which bus. A step you have been to is one click away.', 'bar-steps', ['map']],
  ['mapViews', 'The map', 'Tiles or a list', 'Every map you have in OMSI is here: as tiles with their picture, or as a list with the map beside it. You switch between them here.', 'map-views', ['map']],
  ['mapTile', 'The map', 'A map', "A tile shows how many tours a map has, which year it is set in and at how many places the bus can start. Click it and it's yours.", 'map-tile', ['map']],
  ['startAt', 'The map', 'Where the bus starts', 'On Automatic the bus is put down nearest to your duty\'s first stop. Or pick a place yourself: the orange marks on the map are the same places.', 'map-start', ['map']],
  ['mainAction', 'The map', 'On you go', 'Bottom right, on every step, is the button that takes you on. On the last step it starts the game with your duty.', 'main-action', ['map']],
  ['day', 'The day', 'Day and weather', 'Choose the date, the time and the weather. The timetable runs as it does on that day: on a Sunday there are often fewer buses.', 'day-sheet', ['day']],
  ['shiftLength', 'The duty', 'How long do you want to drive?', 'Say how long your shift is and in which part of the day it starts. Omsi-Hub looks for shifts like that among the timetable\'s real trips.', 'shift-length', ['duty', 'shift']],
  ['shiftList', 'The duty', 'Your shifts', 'These are the shifts that fit. Click one to see all its trips, with the breaks in between; Find other shifts draws new ones.', 'shift-list', ['duty', 'shift']],
  ['shiftRoute', 'The duty', 'Your route', 'On the map, in blue, is where the shift you pick takes you, with every stop on the way. Drag to look around; the wheel zooms.', 'shift-map', ['duty', 'shift']],
  ['freeStarts', 'Free drive', 'Where do you start?', "On a free drive you choose where the bus starts: at a stop where trips begin, at one on the way, or at one of the map's entry points. The traffic and the timetable's buses drive around you.", 'free-starts', ['start', 'free']],
  ['freeLine', 'Free drive', 'Choose a line yourself', 'Want to follow a line after all? Pick it here: its route is on the map, and the navigator and the IBIS know the line. Nothing is booked.', 'free-line', ['start', 'free']],
  ['buses', 'The bus', 'Choose your bus', 'First the maker, then the model and the version, each with a photo. Up here you see where you are: click it to go back up. The bus that fits your duty best says so on its tile.', 'bus-grid', ['bus']],
  ['look', 'The bus', 'View in 3D', 'Your bus in the showroom: turn it with the mouse, zoom with the wheel, and choose its livery, fleet number and plate.', 'bus-look', ['bus']],
  ['navigator', 'In the game', 'Your duty on the navigator', 'Shift+N switches the navigator between the map, the map with your duty board, and off. The board shows the trip you are on, the stops ahead and whether you are on time.', { card: 'navigator' }, 'keep'],
  ['cityMap', 'In the game', 'The city map and signing on', "Shift+M, or a click on the navigator, opens the city map with your route. Beside it you sign on for your duty with your personnel number and code: they are on your driver's pass, right there.", { card: 'citymap' }, 'keep'],
  ['companion', 'In the game', 'Phone & tablet', "Signing on, the duty menu and the bus's screens can also be on a phone or tablet in your network. Turn it on under Settings › General › Phone & tablet; the game shows the address and the code when you drive.", { card: 'companion' }, 'keep'],
  ['done', 'Guided tour', 'Ready to go', 'This question mark shows the tour again. Have a good ride!', 'bar-help', 'home'],
];

/** The bus company's tour (`tour::COMPANY_STOPS`): its welcome, the founding wizard (no company
 * yet), its start screen, what it has, its clock, the way round its pages, the pages and its
 * "?" - Normal's stops and Casual's mixed, each mode making its own (`shown`). Where the page
 * is: ['co', a tab of the company], or 'home' (where the player was). */
const COMPANY_STOPS = [
  ['coWelcome', 'Bus company', 'Your own bus company', 'Here you run a transport company on one of your maps: buy buses, hire drivers, take on lines or make your own. It runs in a time of its own, and what you drive yourself counts for it.', { card: 'welcome' }, ['co', HOME]],
  ['coFound', 'Bus company', 'Found it first', 'Give the company a name, its colours, its home map with a depot and its game mode: Casual, Normal or Realistic. Once it runs, this tour shows you round its pages.', 'company-wizard', ['co', HOME]],
  ['coTabs', 'Its start screen', 'Every part a tile', 'Each part of the company has a tile here, grouped by theme, with what matters there now. A badge says where something waits: unread messages, tours without a driver.', 'company-tiles', ['co', HOME]],
  ['coCasToday', 'Home', "Your company's day", 'How the day goes, in one sentence, and under it what your company solved by itself: a bus that broke down and the spare bus that took over, drivers hired at night.', 'company-casual-today', ['co', HOME]],
  ['coCasDrive', 'Home', 'Drive a shift', "The big button: pick one of today's shifts - its line, its time, its bus - and its start opens. You start the game there.", 'company-casual-drive', ['co', HOME]],
  ['coCasTiles', 'Home', 'Four tiles', 'Your buses, your lines, your drivers and the money, each with one big number and one button. A click on a tile opens its page.', 'company-casual-tiles', ['co', HOME]],
  ['coCasTips', 'Home', 'Tips', 'Your company says what would help now, in plain words: a bus more, a fare too high, a full depot. The button beside a tip does it.', 'company-casual-tips', ['co', HOME]],
  ['coCasNext', 'Home', 'Next day', 'Your company runs the rest of the day by itself: the buses drive, the money comes in. Next day takes you to the next morning.', 'company-casual-next', ['co', HOME]],
  ['coCasMode', 'Home', 'Casual', 'The company sees to its routine by itself: planning, workshop, staff and research. A click here opens the settings, where Normal shows every page with nothing lost.', 'company-mode', ['co', HOME]],
  ['coStatus', 'Its start screen', 'Cash, trophies, reputation', "Under the company's name, always in view: its cash, the trophies earned with goals and contracts, and its reputation. A click opens the page about it.", 'company-status', ['co', HOME]],
  ['coCasStatus', 'Home', 'Cash, trophies, reputation', "Under the company's name, always in view: its cash, the trophies earned with goals, and its reputation. A click on the cash opens the money.", 'company-status', ['co', HOME]],
  ['coClock', 'Its time', "The company's own clock", 'The company runs in a time of its own: let it run here, or jump on to the morning, to tomorrow or by days. At midnight the day is closed: the tours are run and the money is booked.', 'company-clock', ['co', HOME]],
  ['coNav', 'Its pages', 'Back and across', 'This button takes you back to the start screen from every page, as Escape does. The icons on the right open any other page at once.', 'company-nav', ['co', 0]],
  ['coOverview', 'Overview', 'The figures', "The cash, this month's result, the fleet, the staff, punctuality and reputation at a glance.", 'company-figures', ['co', 0]],
  ['coToday', 'Overview', 'What wants your attention', "Today's tours and how many are covered, your own next duty, and what needs doing. A click takes you to the page that helps.", 'company-today', ['co', 0]],
  ['coFleet', 'Fleet', 'Buses and the dealer', 'Your buses with their condition and their livery. At the dealer you buy new or used ones, haggle, lease or rent, and sign the contract.', 'company-page', ['co', 1]],
  ['coCasBuses', 'My buses', 'Your buses', 'Every bus as a card, with its age and its condition. Repairs happen by themselves; Buy a bus opens the shop, the buses that fit your lines first. The icons up here open every page.', 'company-page', ['co', CASUAL.BUSES]],
  ['coCasHelp', 'My buses', 'A question mark on every page', 'Not sure what a page is for? Its question mark says it in a few words.', 'company-casual-help', ['co', CASUAL.BUSES]],
  ['coStaff', 'Staff', 'Drivers and the others', 'Hire drivers on the labour market and keep them content. Their wages, licences, holidays and days ill all count.', 'company-page', ['co', 2]],
  ['coLines', 'Lines', 'What the company runs', "Take on the map's lines or make your own in the line editor: its kind, its buses, its timetable. A line runs once it is planned.", 'company-page', ['co', 3]],
  ['coCasLines', 'My lines', 'Your lines', 'Every line with its buses and passengers. The slider sets its fare, with advice beside it. Add a line takes three steps: the line, how often, done.', 'company-page', ['co', CASUAL.LINES]],
  ['coCasDrivers', 'My drivers', 'Your drivers', 'Who drives for you and how well. The company hires the drivers it needs at night; give a bonus, or let someone go.', 'company-page', ['co', CASUAL.DRIVERS]],
  ['coPlanning', 'Planning', 'Who drives what', 'The week as a chart: drag drivers and buses onto the tours and duties, or let the dispatcher fill the roster. Plan yourself as a driver too.', 'company-page', ['co', 5]],
  ['coFinances', 'Finances', 'The money', "Every booking, month by month: fares, the authority's money, wages, fuel and repairs. Loans come from the bank here.", 'company-page', ['co', 4]],
  ['coCasMoney', 'Money', 'Where the money goes', 'Week by week what came in and went out, and the biggest costs in plain words, with a tip when something costs too much.', 'company-page', ['co', CASUAL.MONEY]],
  ['coCasCompany', 'My company', 'Livery and depot', "Paint your buses in your company's livery, and buy a bigger depot when the buses no longer fit.", 'company-page', ['co', CASUAL.COMPANY]],
  ['coCareer', 'Career', 'Your duties and your progress', "My duties lists the duties planned for you, each a click from driving it. Here too: the company's level, the training courses and the rankings.", 'company-career-parts', ['co', 6]],
  ['coDone', 'Bus company', 'Off to work', 'This question mark shows the tour again. Good luck with your company!', 'company-help', 'home'],
];
/** The stops only Casual makes, and those only Normal and Realistic make (`CASUAL_ONLY`,
 * `ADVANCED_ONLY`). */
const CASUAL_ONLY = ['coCasToday', 'coCasDrive', 'coCasTiles', 'coCasTips', 'coCasNext', 'coCasMode', 'coCasStatus', 'coCasBuses', 'coCasHelp', 'coCasLines', 'coCasDrivers', 'coCasMoney', 'coCasCompany'];
const ADVANCED_ONLY = ['coTabs', 'coStatus', 'coClock', 'coNav', 'coOverview', 'coToday', 'coFleet', 'coStaff', 'coLines', 'coPlanning', 'coFinances', 'coCareer'];
const byId = Object.fromEntries([...STOPS, ...COMPANY_STOPS].map((s) => [s[0], s]));

/** Whether a stop has something to show in the launcher as it is (`Stop::shown`). */
function shown(id, c) {
  // (the company's tour: the founding before there is a company, its pages once there is one,
  // each mode its own)
  if (id === 'coWelcome') return true;
  if (id === 'coFound') return !c.company;
  if (CASUAL_ONLY.includes(id)) return c.company && c.easy;
  if (ADVANCED_ONLY.includes(id)) return c.company && !c.easy;
  if (id === 'coDone') return c.company;
  switch (id) {
    case 'startAt':
      return c.map && c.entries;
    case 'day':
      return c.map && !c.server;
    case 'shiftLength':
    case 'shiftList':
    case 'shiftRoute':
    case 'freeLine':
      return c.map;
    case 'freeStarts':
      return c.map && !c.ownLine;
    case 'look':
      return c.bus;
    default:
      return true;
  }
}

/** What the launcher has when the tour starts, as far as the stops care (`Ctx::of`). */
function ctx() {
  const m = curMap();
  const server = S.choice?.lan_mode === 'join';
  const sh = S.co?.shell;
  const company = S.page === 'company' && !!sh?.open && !S.co.wizard && !S.co.chooser;
  return { map: !!m, entries: !server && !!m?.entry_points?.length, bus: !!curBus(), ownLine: !!S.choice?.own_line, server, company, easy: company && !!sh.casual };
}

// --- the parts pointed at --------------------------------------------------------------------

const $ = (q) => document.querySelector(q);
/** The visible part of an element: cut to the scrolling box it lies in. */
function visible(el) {
  if (!el) return null;
  let r = el.getBoundingClientRect();
  const box = el.parentElement?.closest('.hub-body');
  if (box) {
    const b = box.getBoundingClientRect();
    const top = Math.max(r.top, b.top);
    const bottom = Math.min(r.bottom, b.bottom);
    r = { left: r.left, right: r.right, top, bottom, width: r.width, height: Math.max(0, bottom - top) };
  }
  return r.width > 2 && r.height > 2 ? r : null;
}
const union = (a, b) => (!a ? b : !b ? a : { left: Math.min(a.left, b.left), top: Math.min(a.top, b.top), right: Math.max(a.right, b.right), bottom: Math.max(a.bottom, b.bottom) });
const actionButton = (word) => [...document.querySelectorAll('.hub-actions .hub-btn')].find((b) => b.textContent.trim() === tr(word));

/** Where a part is (in the window), or null when it is not drawn. */
function part(name) {
  switch (name) {
    case 'mode-tiles':
      return visible($('.hub-modes'));
    case 'start-links':
      return visible($('.hub-under'));
    case 'bar-flags':
      return visible($('.hub-bar .hub-flags'));
    case 'bar-steps':
      return visible($('.hub-bar .hub-steps'));
    case 'bar-help':
      return visible($('.hub-bar .hub-bar-help'));
    case 'map-views':
      return visible($('.hub-sheet.wide .hub-toolbar .hub-seg'));
    case 'map-tile': {
      const on = $('.hub-sheet.wide .hub-tiles .hub-tile.on, .hub-sheet.wide .hub-rows .hub-rowl.on');
      const r = visible(on);
      if (r && r.height > 20) return r;
      return visible($('.hub-sheet.wide .hub-tiles .hub-tile, .hub-sheet.wide .hub-rows .hub-rowl'));
    }
    case 'map-start':
      return visible($('.hub-sheet.wide .hub-foot select')?.parentElement?.parentElement);
    case 'main-action':
      return visible($('.hub-actions .hub-go'));
    case 'day-sheet':
      return visible($('.hub-sheet.narrow'));
    case 'shift-length':
      return visible($('.hub-sheet.narrow .hub-range-input')?.closest('.hub-pad'));
    case 'shift-list':
      // (the list's whole room, as the game-drawn launcher's: rows to come go there)
      return visible($('.hub-sheet.narrow .hub-duties')?.closest('.hub-body'));
    case 'shift-map':
      return visible($('.hub-map'));
    case 'free-starts': {
      // (from the kinds of start down to the note under the list)
      const r = union(visible($('.hub-free .hub-chips')), visible($('.hub-free .hub-flist')));
      const pins = $('.hub-free .hub-pins')?.getBoundingClientRect();
      return r && pins && pins.top - 6 > r.bottom ? { ...r, bottom: pins.top - 6 } : r;
    }
    case 'free-line':
      return visible($('.hub-free .hub-seg button:last-child'));
    case 'bus-grid':
      return union(visible($('.hub-sheet.wide .hub-toolbar nav')), visible($('.hub-sheet.wide .hub-tiles')));
    case 'bus-look':
      return visible(actionButton('View in 3D'));
    // the bus company's (`company::*`'s anchors)
    case 'company-wizard':
      return visible($('.co-found .inner'));
    case 'company-clock':
      return visible($('.co-bar .co-clock'));
    case 'company-tiles':
      return visible($('.co-home > div'));
    case 'company-status':
      return visible($('.co-bar .co-status'));
    case 'company-nav':
      return visible($('.co-nav'));
    case 'company-figures':
      return [...document.querySelectorAll('.co-overview .co-figure')].map(visible).reduce(union, null);
    case 'company-today':
      return visible($('.co-overview .co-today'));
    case 'company-page':
      return visible($('.co-body') || $('.co .rf'));
    case 'company-career-parts':
      return visible($('.co-body .co-seg'));
    case 'company-help':
      return visible($('.co-bar .co-help'));
    case 'company-casual-today':
      return visible($('.co-casual-home section.today'));
    case 'company-casual-drive':
      return visible($('.co-casual-home .drive'));
    case 'company-casual-tiles':
      return visible($('.co-casual-home .tiles'));
    case 'company-casual-tips':
      return [...document.querySelectorAll('.co-casual-home .tip')].map(visible).reduce(union, null);
    case 'company-casual-next':
      return visible($('.co-bar .co-next'));
    case 'company-mode':
      return visible($('.co-bar .co-pill'));
    case 'company-casual-help':
      return visible($('.co-casual-head .co-iconbtn'));
    default:
      return null;
  }
}

// --- going round -----------------------------------------------------------------------------

const T = () => S.hubTour;
const put = (patch) => S.hubTour && set({ hubTour: { ...S.hubTour, ...patch } });
const still = () => S.settings?.animations === false;

/** Start the tour from its first stop (again, if it was running): home is where the player is. */
export function startTour(at = 0, kind = 'drive') {
  loadCss();
  const plan = (kind === 'company' ? COMPANY_STOPS : STOPS).filter((s) => shown(s[0], ctx())).map((s) => s[0]);
  const home = T()?.home || { page: S.page, step: S.hub.step, free: !!S.choice.free, composed: !!S.choice.composed, busView: S.hub.busView, tab: S.co?.tab };
  const k = Math.min(Math.max(0, at), plan.length - 1);
  set({ hubTour: { kind, plan, at: k, forward: true, home, moved: !!T()?.moved, waited: 0, shown: null, rect: null, closing: false, opened: Date.now() } });
  goTo(byId[plan[k]][5]);
}

/** The bus company's tour at its stop `at` (`tour::start_company`): by itself the first time,
 * from the "?" where it was left (`co_tour`). */
export function startCompanyTour(at = 0) {
  startTour(at, 'company');
}

/** The way of driving (as the start's tiles set it, but not saved: it is the tour's). */
function setMode(free, composed) {
  const c = S.choice;
  if (!!c.free === free && !!c.composed === composed) return;
  set({ choice: { ...c, free, composed } });
  put({ moved: true });
}

/** The launcher where a stop needs it (`tour::go_to`). */
function goTo(place) {
  const t = T();
  if (!t || place === 'keep') return;
  if (place === 'home') {
    const h = t.home;
    // (the company's tour: back on the company's page the player was on)
    if (t.kind === 'company') return setCo({ tab: h.tab ?? HOME, dialog: null });
    setMode(h.free, h.composed);
    set({ page: h.page, hub: { ...S.hub, step: h.step, busView: h.busView, palette: false } });
    return;
  }
  if (place[0] === 'co') return setCo({ tab: place[1], dialog: null, popup: null });
  const [step, mode] = place;
  if (mode === 'shift') setMode(false, true);
  if (mode === 'free') setMode(true, false);
  // (the bus step on its tiles: the showroom has none of the tour's parts)
  if (step === 'bus') S.hub.busView = 'tiles';
  goStep(step);
}

function act(cmd) {
  const t = T();
  if (!t || t.closing) return;
  if (cmd === 'skip') return finish(false);
  const k = cmd === 'next' ? t.at + 1 : Math.max(0, t.at - 1);
  if (k >= t.plan.length) return finish(true);
  if (k === t.at) return;
  put({ at: k, forward: k > t.at, waited: 0 });
  goTo(byId[t.plan[k]][5]);
}

/** The stop now is left out (its part is not drawn): on to the next one the way the tour is
 * going (`tour::leave_out`). */
function leaveOut() {
  const t = T();
  const plan = t.plan.filter((_, i) => i !== t.at);
  if (!plan.length) return finish(t.forward);
  const at = t.forward ? t.at : Math.max(0, Math.min(t.at - 1, plan.length - 1));
  if (t.forward && at >= plan.length) {
    put({ plan, at: plan.length - 1 });
    return finish(true);
  }
  put({ plan, at, waited: 0 });
  goTo(byId[plan[at]][5]);
}

/** The end: the player's own place back, the dark goes (`tour::finish`); the company's tour
 * remembers where it was left (`completed`: its last stop gone through). */
function finish(completed) {
  const t = T();
  if (!t) return;
  goTo('home');
  if (t.kind === 'company' && !S.boot?.selftest) cmd('co_tour', { what: 'left', at: t.at, completed: !!completed }).catch(() => {});
  // (saved again as the player had it: the tour's way of driving may have been saved meanwhile)
  if (T()?.moved) choose({ free: t.home.free, composed: t.home.composed });
  if (still()) return set({ hubTour: null });
  put({ closing: true });
  setTimeout(() => S.hubTour?.closing && set({ hubTour: null }), 320);
}

// --- the spotlight and the bubble ------------------------------------------------------------

const ROOM = 8;
const GAP = 18;
const MARGIN = 16;
const ARROW_W = 18;
const ARROW_H = 9;
const SHEET_R = 14;

/** Where a bubble of `w` x `h` goes beside the spotlight (`tour::place`): under it if there is
 * room, else above, else to its right, else to its left - beside a part on the left, to its
 * right first; over its bottom right corner when nothing fits. */
function place(hole, w, h, W, H) {
  const fitX = (x) => Math.min(Math.max(x, MARGIN), Math.max(MARGIN, W - MARGIN - w));
  const fitY = (y) => Math.min(Math.max(y, MARGIN), Math.max(MARGIN, H - MARGIN - h));
  const cx = fitX(hole.x + hole.w / 2 - w / 2);
  const cy = fitY(hole.y + hole.h / 2 - h / 2);
  const below = hole.y + hole.h + GAP + h <= H - MARGIN && [cx, hole.y + hole.h + GAP, 'below'];
  const above = hole.y - GAP - h >= MARGIN && [cx, hole.y - GAP - h, 'above'];
  const right = hole.x + hole.w + GAP + w <= W - MARGIN && [hole.x + hole.w + GAP, cy, 'right'];
  const left = hole.x - GAP - w >= MARGIN && [hole.x - GAP - w, cy, 'left'];
  const order = hole.x + hole.w < W * 0.4 ? [right, below, above, left] : [below, above, right, left];
  return order.find(Boolean) || [fitX(hole.x + hole.w - 24 - w), fitY(hole.y + hole.h - 24 - h), 'over'];
}

/** The bubble's arrow (`tour::arrow`): its base on the side facing the spotlight, its tip
 * towards the spotlight's middle as near as the bubble's corners let it be; as a point
 * relative to the bubble and a turn. */
function arrowOf(b, side, hole) {
  const half = ARROW_W / 2;
  const along = (lo, hi, at) => {
    const a = lo + SHEET_R + half;
    const z = hi - SHEET_R - half;
    return z <= a ? (lo + hi) / 2 : Math.min(Math.max(at, a), z);
  };
  if (side === 'below' || side === 'above') return { x: along(b.x, b.x + b.w, hole.x + hole.w / 2) - b.x, side };
  if (side === 'right' || side === 'left') return { y: along(b.y, b.y + b.h, hole.y + hole.h / 2) - b.y, side };
  return null;
}

/** The stage's own lengths (it is scaled as a whole: `--hub-scale`). */
function stage() {
  const el = $('.hub-stage');
  const s = el ? parseFloat(getComputedStyle(el).getPropertyValue('--hub-scale')) || 1 : 1;
  const r = el?.getBoundingClientRect() || { left: 0, top: 0 };
  return { s, x: r.left, y: r.top, W: innerWidth / s, H: innerHeight / s };
}

/** The tour over the page while it runs. */
export function TourOverlay() {
  const s = useStore();
  const t = s.hubTour;
  const bubble = useRef();
  const [hole, setHole] = useState(null);
  const [pos, setPos] = useState(null);
  const [ping, setPing] = useState(0);
  const [settled, setSettled] = useState(false);
  const id = t ? t.plan[t.at] : null;
  const stop = id && byId[id];
  const card = stop && typeof stop[4] === 'object';
  // each frame: where the stop's part is (while it is not drawn yet, the one before stays shown)
  useEffect(() => {
    if (!t || t.closing) return;
    let raf = 0;
    let waited = 0;
    const tick = () => {
      const tt = T();
      if (!tt || tt.closing) return;
      const st = byId[tt.plan[tt.at]];
      if (typeof st[4] === 'object') {
        if (tt.shown !== st[0] || tt.rect) put({ shown: st[0], rect: null });
      } else {
        const r = part(st[4]);
        if (r) {
          waited = 0;
          const g = stage();
          const rect = { x: (r.left - g.x) / g.s, y: (r.top - g.y) / g.s, w: (r.right - r.left) / g.s, h: (r.bottom - r.top) / g.s };
          const old = tt.rect;
          if (tt.shown !== st[0] || !old || Math.abs(old.x - rect.x) + Math.abs(old.y - rect.y) + Math.abs(old.w - rect.w) + Math.abs(old.h - rect.h) > 1) put({ shown: st[0], rect });
        } else if (++waited > 45) {
          // (not drawn after three quarters of a second: the stop is left out)
          waited = 0;
          leaveOut();
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [t?.at, t?.plan?.length, t?.closing]);
  // the keys: on (the right arrow, Enter), back (the left arrow), out (Escape)
  useEffect(() => {
    if (!t) return;
    const key = (e) => {
      const cmd = { ArrowRight: 'next', Enter: 'next', ArrowLeft: 'back', Escape: 'skip' }[e.key];
      e.stopPropagation();
      if (cmd) {
        e.preventDefault();
        act(cmd);
      } else if (!e.ctrlKey && !e.altKey && !e.metaKey && e.key.length === 1) e.preventDefault();
    };
    addEventListener('keydown', key, true);
    return () => removeEventListener('keydown', key, true);
  }, [!!t]);
  // where the spotlight and the bubble go: round the part, the bubble beside it; a card in the
  // middle with the spotlight shrunk to a point under it; closing, the spotlight opens up
  useLayoutEffect(() => {
    if (!t) return;
    const g = stage();
    const shownStop = byId[t.shown || id];
    const isCard = typeof shownStop?.[4] === 'object';
    const w = bubble.current?.offsetWidth || 372;
    const h = bubble.current?.offsetHeight || 200;
    let to;
    if (t.closing) to = { x: -36, y: -36, w: g.W + 72, h: g.H + 72 };
    else if (!isCard && t.rect) {
      const r = t.rect;
      const x0 = Math.max(4, r.x - ROOM);
      const y0 = Math.max(4, r.y - ROOM);
      const x1 = Math.min(g.W - 4, r.x + r.w + ROOM);
      const y1 = Math.min(g.H - 4, r.y + r.h + ROOM);
      to = { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
    } else to = { x: g.W / 2, y: g.H / 2, w: 0, h: 0 };
    let p;
    if (!isCard && t.rect && !t.closing) {
      const [x, y, side] = place(to, w, h, g.W, g.H);
      p = { x, y, side };
    } else p = { x: (g.W - w) / 2, y: Math.max(MARGIN, (g.H - h) / 2), side: 'over' };
    const same = (a, b) => a && b && Math.abs(a.x - b.x) < 0.5 && Math.abs(a.y - b.y) < 0.5 && (a.w ?? 0) === (b.w ?? 0) && (a.h ?? 0) === (b.h ?? 0) && a.side === b.side;
    if (!same(hole, to)) {
      setHole(to);
      setSettled(false);
    }
    if (!same(pos, p)) setPos(p);
  });
  // (a ring goes out from the spotlight once, as it lands on a part)
  useEffect(() => {
    if (!hole) return;
    const k = setTimeout(() => {
      setSettled(true);
      if (!card && !still() && !T()?.closing) setPing((n) => n + 1);
    }, still() ? 0 : 460);
    return () => clearTimeout(k);
  }, [hole?.x, hole?.y, hole?.w, hole?.h]);
  if (!t) return null;
  const shownStop = byId[t.shown || id] || stop;
  const [, chapter, title0, text, target] = shownStop;
  const name = (s.profile?.name || s.config?.profile || '').trim();
  const title = shownStop[0] === 'welcome' && name ? tr('Welcome aboard, %{name}', { name }) : tr(title0);
  const company = t.kind === 'company';
  const isCard = typeof target === 'object';
  const at = Math.max(0, t.plan.indexOf(shownStop[0]));
  const total = t.plan.length;
  const first = at === 0;
  const last = at + 1 >= total;
  const next = first && !last ? 'Show me around' : last ? (company ? 'Off to work' : "Let's drive") : 'Next';
  const ring = hole && !isCard && !t.closing ? Math.min(1, Math.min(hole.w, hole.h) / 28) : 0;
  const arrow = pos && hole && !isCard && !t.closing && settled ? arrowOf({ ...pos, w: bubble.current?.offsetWidth || 372, h: bubble.current?.offsetHeight || 200 }, pos.side, hole) : null;
  const box = (r) => (r ? `left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px` : 'display:none');
  return html`<div class=${'hub-tour' + (t.closing ? ' closing' : '')} onMouseDown=${(e) => e.stopPropagation()}>
    <div class="hub-tour-dim" style=${hole ? `left:${hole.x - 12}px;top:${hole.y - 12}px;width:${hole.w + 24}px;height:${hole.h + 24}px;border-radius:${Math.min(16, hole.w / 2, hole.h / 2) + 12}px` : ''}></div>
    <div class="hub-tour-ring" style=${box(hole) + `;opacity:${ring};border-radius:${Math.min(16, (hole?.w || 0) / 2, (hole?.h || 0) / 2)}px`}></div>
    ${ping > 0 && ring > 0 && html`<div class="hub-tour-ping" key=${ping} style=${box(hole) + `;border-radius:${Math.min(16, hole.w / 2, hole.h / 2)}px`}></div>`}
    <section ref=${bubble} class=${'hub-tour-bubble' + (isCard ? ' card' : '')} style=${pos ? `left:${pos.x}px;top:${pos.y}px` : 'visibility:hidden'}>
      ${arrow && html`<svg class=${'hub-tour-arrow ' + arrow.side} style=${arrow.x != null ? `left:${arrow.x - ARROW_W / 2}px` : `top:${arrow.y - 5}px`} viewBox="0 0 18 10" width="18" height="10"><path d="M0 10 L9 1 L18 10" /></svg>`}
      ${isCard && html`<div class="pic" style=${`background-image:url("omsihub/tour-${target.card}.png")`}></div>`}
      <div class="words" key=${shownStop[0]}>
        <div class="top"><b>${tr(chapter).toUpperCase()}</b><span>${at + 1} / ${total}</span></div>
        <div class="track"><i style=${`width:${((at + 1) / Math.max(1, total)) * 100}%`}></i></div>
        <h3>${title}</h3>
        <p>${tr(text)}</p>
      </div>
      <div class="row">
        ${!last && html`<button type="button" class="skip" onClick=${() => act('skip')}>${tr('Skip')}</button>`}
        <span class="grow"></span>
        ${!first && html`<button type="button" class="back" onClick=${() => act('back')}><${Icon} name="chevron_left" size=${17} />${tr('Back')}</button>`}
        <button type="button" class="next" onClick=${() => act('next')}>${tr(next)}</button>
      </div>
    </section>
  </div>`;
}

/** The self-check's walk through the bus company's tour (Omsi-Hub 6/6): started, every stop on
 * its page with its part found (a stop whose part is not drawn left out, as the tour leaves it
 * out), skipped again - nothing is kept of it (`finish` keeps nothing in the self-check). */
export async function checkCompanyTour(check, errors) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const before = errors.length;
  startCompanyTour(0);
  await sleep(600);
  const ids = [...(T()?.plan || [])];
  const lost = [];
  for (const id of ids) {
    const k = T()?.plan.indexOf(id) ?? -1;
    if (k < 0) {
      lost.push(id);
      continue;
    }
    put({ at: k, waited: 0, forward: true });
    goTo(byId[id][5]);
    await sleep(700);
    const st = byId[id];
    if (typeof st[4] !== 'object' && !part(st[4])) lost.push(`${st[2]} (${st[4]})`);
  }
  finish(false);
  await sleep(400);
  const ok = ids.length > 2 && !lost.length && errors.length === before;
  check('company: its tour finds every part', ok, errors.length > before ? errors.slice(before).join('; ') : lost.length ? `${ids.length - lost.length} of ${ids.length} stops; not found: ${lost.join('; ')}` : `${ids.length} stops`);
}

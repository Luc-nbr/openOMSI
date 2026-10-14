// The self-check of the company's pages of phase 3 (Trello "Tauri 3/6"), called by the bus
// company's (`selftest.js`) with a company of the driver's open: the planning, the depot, the
// concessions, the fleet map, the research, the workshop and the contracts drawn without a
// script error, their views answering, the planning's hand-over of a line's timetable as our
// game-drawn launcher reads it - and, with OPENOMSI_LAUNCHER_SELFTEST=full, the fleet map's
// roads and routes read by the game program. Nothing is bought, built, bid or given.

import { S } from '../../store.js';
import { setUi, cmd } from './kit.js';

const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

/** `shown(what, patch, ms)`: the company's screen set to `patch`, an error said when the page
 * threw one meanwhile (`selftest.js`). */
export async function selftest(check, errors, shown) {
  const sh = S.co.shell;
  if (!sh || sh.casual) {
    check('company: the pages of phase 3', true, 'a Casual company: its own pages only');
    return;
  }
  // every page, drawn, with what it is made of
  const pages = [
    // (with the add-on on, RosterFlow takes the page: `rosterflow/selftest.js` checks it)
    [5, 'the planning', () => ($('.pl') ? `${$$('.pl-day').length} days, ${$$('.pl-head').length} lines, ${$$('.pl-duty, .pl-issue').length} duties and issues` : $('.rf') ? 'RosterFlow takes it' : !!$('.co-body') && 'handed over (RosterFlow)')],
    [7, 'the depot', () => $('.cm-plan') && `${$$('.cm-bus').length} buses on the yard`],
    [8, 'the concessions', () => $$('.co-section').length === 3 && `${$$('.cm-tender').length} tenders`],
    [9, 'the fleet map', () => $('.cm-panel') && ($('.cm-mapbox') ? 'map and panel' : 'panel')],
    [10, 'the research', () => $$('.cm-topic').length > 20 && `${$$('.cm-topic').length} topics`],
    [11, 'the workshop', () => $$('.cm-part').length === 10 && 'ten parts'],
    [12, 'the contracts', () => $('.co-seg') && `${$$('.cm-goal').length} goals`],
  ];
  for (const [tab, name, what] of pages) {
    const e = await shown(`page ${tab}`, { tab, dialog: null }, 1200);
    const said = !e && what();
    check(`company: ${name}`, !!said, e || said || `${$$('.co-body *').length} elements`);
  }
  // the contracts' parts
  for (const part of [1, 2]) {
    setUi('contracts', { part });
    const e = await shown(`contracts ${part}`, { tab: 12 }, 900);
    check(`company: the contracts, part ${part}`, !e, e || (part === 1 ? `${$$('.cm-provider').length} providers` : `${$$('.cm-tender').length} bids`));
  }
  setUi('contracts', { part: 0 });
  // the planning's other days, a duty and a tour chosen (nothing given)
  try {
    const v = await cmd('co_more_view', { view: 'planning', day: 1 });
    check('company: the planning, tomorrow', !!(v.rosterflow || v.tabs?.length === 7), v.rosterflow ? 'RosterFlow' : v.reading || `${v.gantt?.groups?.length ?? 0} lines`);
    const t = v.gantt?.groups?.flatMap((g) => g.tours)?.[0];
    if (t) {
      const d = await cmd('co_more_view', { view: 'planning', day: 1, sel: { line: t.line, tour: t.tour, k: 0 } });
      const b = await cmd('co_more_view', { view: 'planning', day: 1, sel: { line: t.line, tour: t.tour } });
      check("company: a duty's and a tour's panel", d.side?.kind === 'duty' && b.side?.kind === 'tour', `${d.side?.cands?.length} drivers, ${b.side?.cands?.length} buses`);
    }
  } catch (x) {
    check('company: the planning, tomorrow', false, x.message);
  }
  // a line's trips and timetable, handed to our game-drawn launcher on the planning's sheet
  try {
    const a = await cmd('co_hand_over', { page: 'plan-line=oo_5|2024-06-04|2' });
    check('company: the hand-over of a line\'s timetable', a.page === `bus company:plan-line=oo_5|2024-06-04|2:${sh.id}`, a.page);
  } catch (x) {
    check('company: the hand-over of a line\'s timetable', false, x.message);
  }
  // the fleet map's roads and routes, read by the game program
  if (S.boot?.selftest === 'full') {
    try {
      const m = await cmd('co_more_map');
      check('company: the fleet map\'s roads', !!(m.reading || m.roads?.length), m.reading ? 'the timetable is still being read' : `${m.roads.length} roads, ${m.lines.length} routes`);
    } catch (x) {
      check('company: the fleet map\'s roads', false, x.message);
    }
  }
}

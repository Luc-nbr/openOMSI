// RosterFlow - "Bus Scheduling & Crew Rostering" - the bus company's Planning page with the
// add-on on (Trello "Tauri 4/6"), as our game-drawn launcher has it
// (`crates/omsi-launcher-ui/src/company/planning/rosterflow.rs`): an application of its own
// over the whole window, no company bar, its own Back (and Escape) to the company's start
// screen. Top to bottom: the title bar (the mark and the name, the menu's icons, the schedule's
// name with its saved dot, undo, redo and the history, the day switcher, and the tabs Timeplan,
// Vehicles, Drivers and Roster), then the tab's page: the Timeplan (`timeplan.js`), Vehicles and
// Drivers (`studio.js`, the Gantt on a canvas), the Roster (`roster.js`). The opening plays when
// it is opened (`brand.js`), the tutorial lies over it all (`tutorial.js`).
//
// The company's pages hand over to it here (`handOverHooks` of the company's kit): the Planning
// page and a line's timetable open RosterFlow when the add-on is on, the duty launcher's "Open
// in RosterFlow" its Roster on the duty.

import { html, useEffect, useLayoutEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { HOME, setCo, handOverHooks, useSize } from '../company/kit.js';
import { togetherOn, blockedLabel } from '../company/gate.js';
import { pal } from './pal.js';
import { fresh, setRf, setPart, rfCmd, rfAct, after, holds, letGo, toggle, panelIs, settle, rfOpen, ROSTER, VEHICLES, DRIVERS, TIMEPLAN } from './state.js';
import { Mark, Wordmark, Opening, SUBTITLE } from './brand.js';
import { Icon, IconBtn } from './ui.js';
import { Studio } from './studio.js';
import { TitleWindows, useOutsideClose } from './panels.js';
import { PrefsWindow } from './prefs.js';
import { RosterTab, rosterEscape, rosterUndo } from './roster.js';
import { TimeplanTab, tpEscape } from './timeplan.js';
import { RoutesTab, rtEscape, holdChanges, ROUTES } from './routes.js';
import { Tutorial, tutEscape, tutMenu } from './tutorial.js';

// (RosterFlow's own look, loaded with it)
if (!document.querySelector('link[data-rf-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/rosterflow.css';
  sheet.dataset.rfCss = '1';
  document.head.append(sheet);
}

export { ROSTER, VEHICLES, DRIVERS, TIMEPLAN, rfOpen };

/** Open RosterFlow on the company's page (`planning::to_timeplan`, `rosterflow_switched_on`,
 * `duty_launch::to_planning`): `view` a tab, `day` the day of the week, `sel` a duty chosen on the
 * Roster tab, `reveal` a row of the Vehicles or Drivers tab shown (the day's report's trips
 * dropped), `line` a line for the Timeplan (`hint`: what its status bar says first), `offer`
 * its tutorial's menu (switched on just now). */
export async function openRosterFlow(o = {}) {
  const on = await rfCmd('rf_on').catch(() => null);
  if (!on?.on) return false;
  const r = S.rf;
  const animate = S.settings?.animations !== false && !S.boot?.selftest;
  const patch = { open: true, days: on.days, tripsToDuties: !!on.trips_to_duties, company: on.company };
  // (another company: begun anew)
  if (r.company && r.company !== on.company) Object.assign(patch, fresh({ ...patch }));
  if (!r.shown || r.company !== on.company) Object.assign(patch, { shown: true, intro: animate ? performance.now() : null });
  if (o.view != null) Object.assign(patch, { view: o.view, hand: null, zones: null, sel: [], card: null, menu: null, panel: null });
  if (o.day != null) patch.day = o.day;
  if (o.line) Object.assign(patch, { view: TIMEPLAN, tpLine: o.line, tpHint: o.hint || null });
  if (o.sel) patch.roster = { ...r.roster, sel: o.sel, arm: null };
  if (o.reveal) Object.assign(patch, { reveal: o.reveal, selRow: o.reveal });
  if (o.offer) patch.tut = { ...r.tut, offer: true };
  // (a state of its own for a picture: no opening)
  if (o.shot) patch.intro = null;
  setRf(patch);
  setCo({ tab: 5, dialog: null });
  if (o.shot) import('./shots.js').then((m) => m.applyShot(o.shot));
  return true;
}

/** RosterFlow left for the company's start screen (its Back, Escape). */
export function leaveRosterFlow() {
  setRf({ open: false, shown: false, intro: null, hand: null, zones: null, card: null, menu: null, panel: null, prefs: null, sel: [] });
  setCo({ tab: HOME });
}

/** A hand-over of the company's pages RosterFlow takes when the add-on is on: the Planning page
 * ("5"; the duty launcher's "Open in RosterFlow" with its duty), and a line's timetable
 * ("timetable=<line>"; a line of the map's just added with what to do next). */
async function rfTakes(page, more) {
  const p = String(page);
  // (Omsi-Hub 6/6: the planning's "plan-line=<line>|<date>|<tour>" too, as
  // `planning::open_timetable` opens the Timeplan with the add-on on)
  if (p.startsWith('plan-line=')) return openRosterFlow({ line: p.slice('plan-line='.length).split('|')[0] });
  if (p !== '5' && !p.startsWith('timetable=')) return false;
  if (p === '5') {
    const o = {};
    if (more?.duty) {
      const d = more.duty;
      const days = (await rfCmd('rf_on').catch(() => null))?.days || [];
      const k = days.findIndex((x) => x.date === d.date);
      Object.assign(o, { view: ROSTER, day: Math.max(0, k), sel: { duty: [d.line, d.tour, d.duty] } });
    }
    if (more?.switched) Object.assign(o, { view: VEHICLES, offer: true });
    if (more?.shot) o.shot = more.shot;
    return openRosterFlow(o);
  }
  const line = p.slice('timetable='.length);
  return openRosterFlow({ line, hint: more?.done ? 'map' : null });
}
if (!handOverHooks.includes(rfTakes)) handOverHooks.push(rfTakes);

// --- the page ------------------------------------------------------------------------------------------

/** The day's board (`rf_board`), asked again when the day, the company or the language changed
 * or an action was taken; kept as it was when the Rust side says it is the same. */
function useBoard() {
  const r = S.rf;
  const gen = S.co.shell?.gen ?? 0;
  const lang = language();
  useEffect(() => {
    let gone = false;
    rfCmd('rf_board', { day: r.day, have: S.rf.rev })
      .then((v) => {
        if (gone) return;
        if (v.same) setRf({ data: { ...S.rf.data, head: v.head } });
        else if (v.board) setRf({ data: v, rev: v.rev });
        else setRf({ data: { head: v.head, reading: v.reading }, rev: '' });
      })
      .catch((e) => !gone && setRf({ data: { head: S.rf.data?.head, reading: e.message }, rev: '' }));
    return () => {
      gone = true;
    };
  }, [r.day, gen, r.bump, lang, r.view]);
}

export function RosterFlowPage() {
  const r = S.rf;
  const p = pal();
  useBoard();
  useOutsideClose();
  const [ref, size] = useSize();
  // Escape: the tutorial's menu or sheet, what is in hand, then back to the company's start
  useEffect(() => {
    const k = (e) => {
      if (e.key !== 'Escape' || !rfOpen()) return;
      if (S.co.popup || S.co.dialog) return;
      const el = document.activeElement;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      if (tutEscape()) return;
      const x = S.rf;
      if (x.intro) return setRf({ intro: null });
      if (x.view === TIMEPLAN && tpEscape()) return;
      if (x.view === ROUTES && rtEscape()) return;
      if (x.view === ROSTER && rosterEscape()) return;
      if (holds()) return letGo();
      leaveRosterFlow();
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  const studio = r.view === VEHICLES || r.view === DRIVERS;
  let body;
  if (studio) body = html`<${Studio} p=${p} drivers=${r.view === DRIVERS} />`;
  else if (r.view === ROSTER) body = html`<${RosterTab} p=${p} />`;
  else if (r.view === ROUTES) body = html`<${RoutesTab} p=${p} />`;
  else body = html`<${TimeplanTab} p=${p} />`;
  return html`<div class=${'rf' + (p.light ? ' light' : ' dark')} ref=${ref}>
    <${TitleBar} />
    <div class="rf-body" key=${'v' + (studio ? 's' : r.view)}>${body}
      ${!studio && html`<div class="rf-layer">${r.data?.board && html`<${TitleWindows} data=${r.data} drivers=${false} size=${{ w: size.w, h: size.h - 48 }} />`}${r.prefs && html`<${PrefsWindow} />`}</div>`}
    </div>
    ${r.intro && html`<${Opening} p=${p} since=${r.intro} onDone=${() => setRf({ intro: null })} />`}
    <${Tutorial} p=${p} size=${size} />
  </div>`;
}

// (what the title bar gives way with in a narrow window, `fitTitle`: the least the name shows
// with its day before the day goes, and with the company alone before that goes; the saved dot;
// the gap before it; what a tab saves as an icon besides its label; the wordmark and its gap;
// what the day switcher gives up; the margin before a level comes back)
const FULL_MIN = 150;
const NAME_MIN = 44;
const DOT = 10;
const DOT_GAP = 7;
const TAB_SAVE = 6;
const BRAND_SAVE = 204;
const DAY_SAVE = 44;
const SLACK = 4;

/** How much of the title bar gives way so that it stays one row of 48 px, in this order: 0
 * nothing (the company and its day shortened with an ellipsis down to `FULL_MIN`), 1 the day
 * left out, 2 the company left out (the saved dot stays), 3 the tabs as icons, 4 the mark without
 * the wordmark and a narrower day switcher. The back button, the menu's icons and undo, redo and
 * the history always stay. `now`: the level drawn, as `bar` shows it. */
export function fitTitle(bar, now) {
  const slot = bar.querySelector('.rf-name');
  if (!slot) return now;
  const cs = getComputedStyle(slot);
  // (the room the name has now: what the slot is wide, less what still runs over the bar's end)
  const over = Math.max(0, bar.scrollWidth - bar.clientWidth);
  const freeNow = slot.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) - over;
  const labels = [...bar.querySelectorAll('.rf-tab > span')].reduce((a, s) => a + s.scrollWidth + TAB_SAVE, 0);
  const names = bar.querySelector('.rf-brand .names');
  const brand = (names?.offsetWidth ? names.offsetWidth + 8 : BRAND_SAVE) + DAY_SAVE;
  const save = (l) => (l >= 3 ? labels : 0) + (l >= 4 ? brand : 0);
  const free = (l) => freeNow - save(now) + save(l) - (l < now ? SLACK : 0);
  const [full, company] = [...slot.querySelectorAll('.rf-measure b')].map((b) => b.offsetWidth);
  if (free(0) >= Math.min(full || 0, FULL_MIN) + DOT_GAP + DOT) return 0;
  if (free(1) >= Math.min(company || 0, NAME_MIN) + DOT_GAP + DOT) return 1;
  if (free(2) >= DOT) return 2;
  if (free(3) >= DOT) return 3;
  return 4;
}

/** The title bar's level of `fitTitle`, measured again when the bar's width or what it says
 * changes. */
function useTitleFit(deps) {
  const ref = useRef(null);
  const [fit, setFit] = useState(0);
  const now = useRef(0);
  now.current = fit;
  useLayoutEffect(() => {
    const bar = ref.current;
    if (!bar) return;
    const run = () => {
      const f = fitTitle(bar, now.current);
      if (f !== now.current) setFit(f);
    };
    run();
    // (measured after the frame: a size taken inside the observer's own call loops it)
    const ro = new ResizeObserver(() => requestAnimationFrame(run));
    ro.observe(bar);
    return () => ro.disconnect();
  }, [...deps, fit]);
  return [ref, fit];
}

/** The title bar (`rosterflow::title_bar`): the way back, the mark and the name, the menu's icons,
 * the schedule's name, undo, redo and the history, the day, and the tabs - one row of 48 px at
 * any width (`fitTitle`). */
function TitleBar() {
  const r = S.rf;
  const head = r.data?.head;
  const studio = r.view === VEHICLES || r.view === DRIVERS;
  const undo = head?.undo?.length || 0;
  // (on the Roster tab the arrows are the roster's own: `roster.js`)
  const roster = r.view === ROSTER;
  const ru = r.rosterUndo || {};
  const full = head?.name || '';
  const company = head?.company ?? full;
  const [ref, fit] = useTitleFit([full, company, r.view, language()]);
  const tab = (v, icon, label, anchor) => html`<button type="button" class=${'rf-tab' + (r.view === v ? ' on' : '')} data-rf=${anchor} title=${fit >= 3 ? tr(label) : undefined} onClick=${() => r.view !== v && !holdChanges() && setRf({ view: v, hand: null, zones: null, sel: [], card: null, menu: null, panel: null, press: null, dragging: false, selRow: null })}><${Icon} name=${icon} size=${18} /><span>${tr(label)}</span></button>`;
  return html`<header class=${`rf-title fit${fit}`} ref=${ref}>
    <${IconBtn} icon="chevron_left" title="Back to the company's start screen (Esc)" onClick=${leaveRosterFlow} />
    <i class="sep"></i>
    <span class="rf-brand"><${Mark} size=${30} /><span class="names"><${Wordmark} size=${17} /><small>${tr(SUBTITLE)}</small></span></span>
    <i class="sep"></i>
    <span class="rf-icons" data-rf="rf-menu">
      <${IconBtn} icon="tune" title="Preferences: the rules the duties are checked against and Optimise works to" onClick=${() => setRf({ prefs: { item: 'signing', search: '', inUse: false } })} />
      <${IconBtn} icon="description" title="Print the bus graph or the duty graph as a PDF file" onClick=${() => toggle('print')} />
      <${IconBtn} icon="search" title="Go to a vehicle or a duty by its ID (G)" onClick=${() => toggle({ kind: 'goto', text: '' })} />
      <${IconBtn} icon="settings" title="Optimisation settings" onClick=${() => toggle('optimise')} />
      <${IconBtn} icon="groups" disabled=${!togetherOn()} title=${togetherOn() ? 'Play together: friends drive duties of your company in your world' : blockedLabel()} onClick=${() => setCo({ dialog: { kind: 'together' } })} />
      <${IconBtn} icon="help" data="rf-help" title="Tutorial: a guided tour, lessons to try yourself and a help sheet" on=${r.tut.menu} onClick=${tutMenu} />
      <span class="rf-name" title=${full}>
        ${fit < 2 && html`<b class="cut">${fit === 0 ? full : company}</b>`}
        <i class=${'dot' + (head?.draft ? ' draft' : '')} title=${tr(head?.draft ? 'Changes not saved yet: SAVE makes them the company\'s planning, DISCARD forgets them' : 'Saved: the company runs this planning')}></i>
        <span class="rf-measure" aria-hidden="true"><b>${full}</b><b>${company}</b></span>
      </span>
    </span>
    <span class="undo" data-rf="rf-undo">
      <${IconBtn} icon="livery_undo" title="Undo (Ctrl+Z)" disabled=${roster ? !ru.n : !undo || !studio} onClick=${() => (roster ? rosterUndo(true) : rfAct('undo'))} />
      <${IconBtn} icon="livery_redo" title="Redo (Ctrl+Y)" disabled=${roster ? !ru.redo : !head?.redo || !studio} onClick=${() => (roster ? rosterUndo(false) : rfAct('redo'))} />
      <${IconBtn} icon="history" title="Schedule history: every change, to go back to" disabled=${!undo} on=${panelIs('history')} onClick=${() => toggle('history')} />
    </span>
    <button type="button" class="rf-day" data-rf="rf-day" title=${tr('The day shown: the planning of a weekday holds for that weekday every week')} onClick=${() => toggle('day')}>
      <${Icon} name="calendar_month" size=${18} /><b>${head?.day_words || ''}</b><${Icon} name="expand_more" size=${18} />
    </button>
    <span class="rf-tabs">
      ${tab(ROUTES, 'route', 'Routes', 'rf-tab-routes')}
      ${tab(TIMEPLAN, 'departure_board', 'Timeplan', 'rf-tab-timeplan')}
      ${tab(VEHICLES, 'directions_bus', 'Vehicles', 'rf-tab-vehicles')}
      ${tab(DRIVERS, 'person', 'Drivers', 'rf-tab-drivers')}
      ${tab(ROSTER, 'group', 'Roster', 'rf-tab-roster')}
    </span>
  </header>`;
}

export { setPart, after, settle };

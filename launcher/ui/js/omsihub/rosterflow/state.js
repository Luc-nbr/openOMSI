// What RosterFlow's page keeps (`rosterflow::Rf` of our game-drawn launcher, without what the
// Rust side keeps: the board, undo and redo, the proposal, the Timeplan's line): the tab and the
// day shown, the Gantt's zoom and scroll, what is chosen and taken up, the card, the menu and
// the panel open, the filter, the stats, the layers - and the commands (`rf_*` of
// `omsi-launcher-api`'s `omsihub/rosterflow.rs`), what they say said in the company's corner.

import { S, set } from '../../store.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { setCo, refresh as coRefresh } from '../company/kit.js';
import { money } from '../money.js';

/** The views of the title bar's tabs (`rosterflow::ROSTER` ...). */
export const ROSTER = 0;
export const VEHICLES = 1;
export const DRIVERS = 2;
export const TIMEPLAN = 3;

/** The filter as it starts (`rosterflow::Filter::default`). */
export const NO_FILTER = { hidden: [], dir: 0, from: 0, to: 2880, lockedOnly: false, issues: null };

export function fresh(over = {}) {
  return {
    open: false,
    view: VEHICLES,
    day: 0,
    data: null,
    rev: '',
    bump: 0,
    zoom: 1,
    pan: 0,
    scroll: 0,
    sel: [],
    selRow: null,
    ticked: [],
    pinned: [],
    hand: null,
    zones: null,
    press: null,
    dragging: false,
    card: null,
    menu: null,
    panel: null,
    dismissedTab: false,
    issueTicks: [],
    groupsOpen: [],
    filter: { ...NO_FILTER },
    filterEdit: null,
    stats: [],
    sort: null,
    layers: { label: 0, breaks: false, moved: false },
    fullscreen: false,
    kpisOpen: false,
    lastClick: null,
    intro: null,
    shown: false,
    prefs: null,
    reveal: null,
    zoneHover: null,
    customDraft: [0, 810, 900],
    edits: 0,
    tp: { line: null, day: 0, side: 0, pattern: 0, tab: 0, pointsOnly: false, cell: null, text: '', sel: null, adding: null, chart: null, issuesOpen: false, leaving: false, scroll: 0, hscroll: 0 },
    roster: { sel: null, arm: null, clear: false },
    tut: { menu: false, sheet: false, lesson: null, offer: false, asked: null },
    ...over,
  };
}

if (!S.rf) S.rf = fresh();

/** Change what RosterFlow keeps (and draw again). */
export function setRf(patch) {
  set({ rf: { ...S.rf, ...(typeof patch === 'function' ? patch(S.rf) : patch) } });
}

/** A part of what RosterFlow keeps changed (`tp`, `roster`, `tut`). */
export function setPart(part, patch) {
  setRf((r) => ({ [part]: { ...r[part], ...(typeof patch === 'function' ? patch(r[part]) : patch) } }));
}

/** A command of RosterFlow's in the page's language (company multiplayer: on the company of a
 * session joined, `S.co.remote`, relayed to the boss's launcher - all but the tutorial's, which
 * is this launcher's own: `together::relay::relayed_rf`). */
export const rfCmd = (name, args = {}) => {
  const lang = language();
  if (S.co?.remote && !name.startsWith('rf_tut')) return call('tg_relay', { cmd: name, args: { ...args, lang }, lang });
  return call(name, { ...args, lang });
};

/** RosterFlow on the company of a session joined, and what the player's role lets him do there
 * (`rosterflow::relayed`'s `remote`: plans, lines, assign, why not). */
export const rfRemote = () => (S.co?.remote ? S.rf.remote || { plans: false, lines: false, assign: false, why: null } : null);

/** Something said in the company's corner (`set_status`). */
export function say(text, error = false) {
  if (!text) return;
  setCo({ notice: { text, error, at: Date.now() } });
}

/** What a command says back: its words, its popup. */
export function after(r) {
  if (!r) return r;
  if (r.say) say(r.say.text, r.say.error);
  told(r.told);
  if (r.popup) setCo({ popup: r.popup });
  return r;
}

/** What the other planners changed meanwhile (`live::told`), said in the corner - after what
 * the command itself says. */
export function told(list) {
  if (!Array.isArray(list) || !list.length) return;
  const was = S.co.notice;
  const text = list.join(' ');
  say(was && Date.now() - was.at < 2000 ? `${was.text} ${text}` : text, false);
}

/** The day's board asked again (and the company's shell with it). */
export function bump() {
  setRf((r) => ({ bump: r.bump + 1 }));
  coRefresh();
}

/** An action of the Vehicles and Drivers tabs (`rf_act`) on the day shown. */
export async function rfAct(act, args = {}) {
  let r;
  try {
    // (the day as the page shows it: an edit of a day somebody else changed meanwhile is refused,
    // not made on the wrong trips - `rosterflow::act`'s `stale`)
    r = await rfCmd('rf_act', { act, day: S.rf.day, rev: S.rf.rev || '', ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  if (r?.stale) settle();
  if (r?.edited) setRf((x) => ({ edits: x.edits + 1 }));
  // (undone or redone: the day's blocks and trips are numbered anew, as after an edit - what is
  // chosen, held or carded by its numbers let go)
  if (r?.say && UNDOES.includes(act)) settle();
  bump();
  return r;
}

const UNDOES = ['undo', 'redo', 'back_to'];

/** The hand let go of and the cards closed (after an edit: the day's indices change). */
export function settle() {
  setRf({ hand: null, zones: null, press: null, dragging: false, card: null, menu: null, sel: [], selRow: null, zoneHover: null });
}

/** An edit (`rf_act`): done, then what is in hand let go. */
export async function edit(act, args = {}) {
  const r = await rfAct(act, args);
  if (r?.edited) settle();
  return r;
}

/** Something is in hand that Escape lets go of first (`Rf::holds`). */
export function holds() {
  const r = S.rf;
  return !!(r.hand || r.card || r.menu || r.panel || r.sel.length || r.data?.proposal || r.prefs || r.intro);
}

/** Escape (`Rf::let_go`): the preferences, then what lies open, then what is in hand. */
export function letGo() {
  const r = S.rf;
  if (r.prefs) return setRf({ prefs: null });
  if (r.menu || r.card || r.panel) return setRf({ menu: null, card: null, panel: null });
  setRf({ hand: null, zones: null, press: null, dragging: false, sel: [], selRow: null });
  if (r.data?.proposal) rfAct('keep_mine');
}

/** Take something up (`rf_gantt::pick`): a locked vehicle or duty is refused at once; its drop
 * zones are worked out (each rated by what it costs). */
export async function pick(hand, dragging = false) {
  const r = await rfCmd('rf_act', { act: 'pick', day: S.rf.day, rev: S.rf.rev || '', hand }).catch(() => null);
  if (r?.refused || r?.stale) {
    after(r);
    if (r.stale) bump();
    return;
  }
  setRf({ press: null, hand, zones: null, dragging, menu: null, card: null });
  if (hand.kind === 'swap') return;
  const zones = await rfCmd('rf_view', { view: 'zones', day: S.rf.day, hand }).catch(() => []);
  // (still the same hand: a drop meanwhile let it go)
  if (S.rf.hand !== hand) return;
  setRf({ zones: Array.isArray(zones) ? zones : [] });
  // (a duty's trips that may go nowhere: why, in the corner - rather than a drag that ends in
  // nothing)
  if (hand.kind === 'duty_trips' && Array.isArray(zones) && !zones.length) {
    const why = await rfCmd('rf_view', { view: 'why', day: S.rf.day, hand }).catch(() => null);
    if (why?.text && S.rf.hand === hand) say(why.text, true);
  }
}

/** Drop what is in hand at zone `zi` (`rf_gantt::drop_at`). */
export async function dropAt(zi) {
  const r = S.rf;
  const z = r.zones?.[zi];
  if (!r.hand || !z) return;
  await rfAct('drop', { hand: r.hand, zone: z.drop });
  settle();
}

/** Drop `hand` at `zone` whether a drop zone is offered there or not (a duty's trips let go on a
 * row: `rf_gantt`'s `dropOnRow`): moved, or why not said in the corner. */
export async function dropTo(hand, zone) {
  await rfAct('drop', { hand, zone });
  settle();
}

/** Toggle a panel (`rosterflow::toggle`): the same kind closes it. */
export function toggle(panel) {
  const now = S.rf.panel;
  const same = now && (now === panel || (now.kind && panel.kind && now.kind === panel.kind));
  setRf({ panel: same ? null : panel });
}

export const panelIs = (kind) => {
  const p = S.rf.panel;
  return p === kind || p?.kind === kind;
};

/** "07:05" on the clock (`rosterflow::clock`). */
export function clock(m) {
  const v = ((Math.round(m) % 1440) + 1440) % 1440;
  return `${String(Math.floor(v / 60)).padStart(2, '0')}:${String(v % 60).padStart(2, '0')}`;
}

/** "7:05" (`rosterflow::hm`): minutes, past midnight on. */
export function hm(m) {
  const v = Math.max(0, Math.round(m));
  return `${Math.floor(v / 60)}:${String(v % 60).padStart(2, '0')}`;
}

/** Whole units of the company's currency (`words::eur`, `money.js`: "€1,234", "£1,061",
 * "1.234 €"). */
export function eur(cents) {
  return money(cents || 0);
}

/** The ids of the rows (`RowId`): a bus's block, a duty. */
export const isBlock = (id) => String(id).startsWith('B|');

/** RosterFlow is open on the company's page. */
export const rfOpen = () => !!S.rf.open && S.page === 'company' && S.co?.tab === 5;

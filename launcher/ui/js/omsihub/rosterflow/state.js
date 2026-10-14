// What RosterFlow's page keeps (`rosterflow::Rf` of our game-drawn launcher, without what the
// Rust side keeps: the board, undo and redo, the proposal, the Timeplan's line): the tab and the
// day shown, the Gantt's zoom and scroll, what is chosen and taken up, the card, the menu and
// the panel open, the filter, the stats, the layers - and the commands (`rf_*` of
// `omsi-launcher-api`'s `omsihub/rosterflow.rs`), what they say said in the company's corner.

import { S, set } from '../../store.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { setCo, refresh as coRefresh } from '../company/kit.js';

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

/** A command of RosterFlow's in the page's language. */
export const rfCmd = (name, args = {}) => call(name, { ...args, lang: language() });

/** Something said in the company's corner (`set_status`). */
export function say(text, error = false) {
  if (!text) return;
  setCo({ notice: { text, error, at: Date.now() } });
}

/** What a command says back: its words, its popup. */
export function after(r) {
  if (!r) return r;
  if (r.say) say(r.say.text, r.say.error);
  if (r.popup) setCo({ popup: r.popup });
  return r;
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
    r = await rfCmd('rf_act', { act, day: S.rf.day, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  if (r?.edited) setRf((x) => ({ edits: x.edits + 1 }));
  bump();
  return r;
}

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
  const r = await rfCmd('rf_act', { act: 'pick', day: S.rf.day, hand }).catch(() => null);
  if (r?.refused) {
    after(r);
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

/** Whole euros as the page's language writes them (`words::eur`: "€1,234", "€ 1.234",
 * "1.234 €", "1 234 €"). */
export function eur(cents) {
  const lang = language();
  const [th, euro] = lang === 'en' ? [',', 'before'] : lang === 'nl' ? ['.', 'spaced'] : ['de', 'it', 'es', 'pt', 'pt-pt', 'tr', 'da', 'id'].includes(lang) ? ['.', 'after'] : [' ', 'after'];
  const v = Math.round((cents || 0) / 100);
  const n = String(Math.abs(v)).replace(/\B(?=(\d{3})+(?!\d))/g, th);
  const sign = v < 0 ? '-' : '';
  return euro === 'before' ? `${sign}€${n}` : euro === 'spaced' ? `${sign}€ ${n}` : `${sign}${n} €`;
}

/** The ids of the rows (`RowId`): a bus's block, a duty. */
export const isBlock = (id) => String(id).startsWith('B|');

/** RosterFlow is open on the company's page. */
export const rfOpen = () => !!S.rf.open && S.page === 'company' && S.co?.tab === 5;

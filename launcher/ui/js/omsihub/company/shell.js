// The bus company's shell (Beta 65, the concepts Luc chose): its pages round a grouped,
// labelled sidebar that collapses to a rail of icons and slides away (Ctrl+B), a slim bar over
// the page (where one is, the cash and the reputation, the company's clock with one "Simulate to
// tomorrow" and its other steps under it, the bell, the round menu, Back) and the command
// palette (Ctrl+K) over every page. The company's look and feel is the player's choice
// (`company_layout`, the Settings' Appearance): the sidebar, or no sidebar at all - the bar with
// the Omsi-Hub logo, the company, a search field and a row of shortcuts, the pages found with the
// palette. The pages are the same in both.
//
// Its parts are the shell the clock sends (`company::shell`: the pages grouped under their
// headings, `RAIL_HEADS`, with their badges; the bell's count) and the bell's list (`co_view`
// "bell"). The sidebar's state and the pages pinned at its top are the launcher's settings
// (`company_sidebar`, `company_pins`).

import { S, set } from '../../store.js';
import { saveSettings } from '../../data.js';
import { dark } from '../theme.js';
import { flipLook } from '../bar.js';
import { MARK, LOGO_SVG } from '../logo.js';
import { html, useEffect, useRef, useState, tr, HOME, CASUAL, setCo, setUi, cmd, act, goTab, goKey, handOver, Icon, Mark, Btn } from './kit.js';
import { TOGETHER_TAB } from './together.js';
import { togetherOn, blockedLabel } from './gate.js';
import { follow } from './agenda.js';

/** What the screen around the shell does for it (`company.js`): back to the start, the tour. */
export const shellHooks = { back: () => {}, tour: () => {} };

/** The pages that are the company's own and have no number of `TABS`: its settings (a dialog). */
const SETTINGS = 'settings';
/** RosterFlow's tabs as the palette opens them (`rosterflow/index.js`'s ROSTER ...). */
const RF_TABS = [
  ['timeplan', 'Timeplan', 'schedule'],
  ['vehicles', 'Vehicles', 'directions_bus'],
  ['drivers', 'Drivers', 'groups'],
  ['roster', 'Roster', 'calendar_month'],
];
const OPS_TAB = 15;

// --- the look and feel -----------------------------------------------------------------------

/** The company's layout: "sidebar" (the default) or "palette". */
export const layoutOf = () => (S.settings?.company_layout === 'palette' ? 'palette' : 'sidebar');
/** The sidebar as the player left it: "open", "rail" or "hidden". */
const sideSaved = () => (['rail', 'hidden'].includes(S.settings?.company_sidebar) ? S.settings.company_sidebar : 'open');
/** Opened wide by hand on a narrow stage (until the window changes again). */
let wideAnyway = false;

/** The sidebar as it is drawn: open, as its rail (on a narrow stage - a small window, a large
 * Launcher size - unless opened by hand) or slid away. */
export function sideState(narrow) {
  const v = sideSaved();
  if (v === 'open' && narrow && !wideAnyway) return 'rail';
  return v;
}

/** The sidebar opened, folded to its rail, or slid away (Ctrl+B) - and remembered. */
function setSide(to, narrow) {
  wideAnyway = to === 'open' && narrow;
  saveSettings({ company_sidebar: to });
}

/** Ctrl+B: the sidebar away, or back as it was. */
function flipSide(narrow) {
  setSide(sideState(narrow) === 'hidden' ? 'open' : 'hidden', narrow);
}

/** The pages pinned at the top of the sidebar, in their order. */
const pins = () =>
  String(S.settings?.company_pins || '')
    .split(',')
    .filter((x) => /^\d+$/.test(x.trim()))
    .map(Number);

function flipPin(tab) {
  const now = pins();
  const next = now.includes(tab) ? now.filter((t) => t !== tab) : [...now, tab].slice(-12);
  saveSettings({ company_pins: next.join(',') });
}

/** The stage narrow: the company's screen less wide than its sidebar and a page want. */
export function useNarrow(ref) {
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === 'undefined') return undefined;
    let was = null;
    const ro = new ResizeObserver(() => {
      const n = el.clientWidth < 1180;
      // (narrow or wide anew: what was opened by hand is forgotten)
      if (was !== null && n !== was) wideAnyway = false;
      was = n;
      setNarrow(n);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref.current]);
  return narrow;
}

// --- the pages ------------------------------------------------------------------------------

const remoteOf = (c) => !!(c.remote && c.shell?.remote);
/** The friend of "Play together" on the boss's company may change its settings. */
export const mayChange = (c = S.co) => !c.remote || (c.shell?.remote?.perms || []).includes('settings');

/** A page's name as the sidebar and the bar say it. */
export function pageName(sh, tab) {
  if (tab === SETTINGS) return tr('Company settings');
  if (tab === HOME) return sh.casual ? sh.casual_pages?.pages?.[HOME]?.name || tr('Home') : tr('Start screen');
  if (sh.casual) return sh.casual_pages?.pages?.[tab]?.name || '';
  if (tab === 0) return tr('Dashboard');
  return sh.names?.[tab] || '';
}

/** A page's icon. */
export function pageIcon(sh, tab) {
  if (tab === SETTINGS) return 'settings';
  if (tab === HOME) return sh.casual ? 'home' : 'apps';
  return sh.rail?.flat().find((x) => x.tab === tab)?.icon || sh.casual_pages?.pages?.[tab]?.icon || 'apps';
}

/** The pages grouped as the sidebar shows them: the start screen first, every page of the
 * shell's rail under its heading (groups of one heading together), the company's settings last;
 * a friend on the boss's company sees the pages his role allows. */
export function navGroups(c = S.co) {
  const sh = c.shell;
  const remote = remoteOf(c);
  const allowed = remote ? new Set(sh.pages || []) : null;
  const out = [];
  for (const g of sh.rail || []) {
    const items = g.filter((it) => !allowed || allowed.has(it.tab)).map((it) => ({ ...it, name: pageName(sh, it.tab) }));
    if (!items.length) continue;
    const head = sh.casual ? '' : items[0].group || '';
    const last = out[out.length - 1];
    if (last && last.head === head) last.items.push(...items);
    else out.push({ head, items });
  }
  const homeOk = !remote || (sh.remote?.perms || []).includes('finances');
  if (homeOk) {
    const home = { tab: HOME, icon: pageIcon(sh, HOME), name: pageName(sh, HOME), tip: sh.casual ? tr("Back to your company's home (Escape)") : tr('Every part of the company as a tile (Escape)'), badge: null };
    if (out.length) out[0].items.unshift(home);
    else out.push({ head: '', items: [home] });
  }
  if (mayChange(c) && out.length) {
    out[out.length - 1].items.push({ tab: SETTINGS, icon: 'settings', name: tr('Company settings'), tip: tr("The company's settings: its name, its date, how it buys"), badge: null });
  }
  return out;
}

/** Go to a page of the sidebar (the settings are a dialog). */
export function goPage(tab) {
  if (tab === SETTINGS) return setCo({ dialog: { kind: 'settings' } });
  if (tab === HOME) return setCo({ tab: HOME, dialog: null });
  goTab(tab);
}

/** RosterFlow opened on one of its tabs, the Planning page where its add-on is off. */
function toRosterFlow(view) {
  import('../rosterflow/index.js')
    .then((m) => m.openRosterFlow(view ? { view: m[view.toUpperCase()] } : {}))
    .then((ok) => ok || goKey('planning'))
    .catch(() => goKey('planning'));
}

/** The Multiplayer page, or its dialog where the page is not the company's yet. */
function openTogether() {
  if (S.co.shell?.pages?.includes(TOGETHER_TAB)) goTab(TOGETHER_TAB);
  else setCo({ dialog: { kind: 'together' } });
}

/** Another of the driver's companies (the clock stopped first). */
function otherCompany() {
  cmd('co_act', { act: 'play', speed: 0 }).finally(() => setCo({ chooser: 'companies' }));
}

/** The online panel: the account and friends (`online.js`, as its own button opens it). */
function openAccount() {
  set({ hub: { ...S.hub, online: S.hub?.online ? null : 'auto' } });
}

// --- the recent places -----------------------------------------------------------------------

let recent = [];
try {
  recent = JSON.parse(localStorage.getItem('omsihub-co-recent') || '[]').filter((t) => typeof t === 'number');
} catch {
  recent = [];
}

/** A page visited: first among the recent places. */
function visited(tab) {
  if (tab == null) return;
  recent = [tab, ...recent.filter((t) => t !== tab)].slice(0, 8);
  try {
    localStorage.setItem('omsihub-co-recent', JSON.stringify(recent));
  } catch {
    /* (kept for this session only) */
  }
}

// --- the shell ---------------------------------------------------------------------------------

/** The shell round an open company's page: the sidebar (in its layout) and the bar over the
 * page; Ctrl+K and Ctrl+B; the palette when it is open. */
export function Shell({ narrow }) {
  const c = S.co;
  const layout = layoutOf();
  const side = layout === 'sidebar' ? sideState(narrow) : 'none';
  useEffect(() => visited(c.tab), [c.tab]);
  useEffect(() => {
    const k = (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      const key = e.key.toLowerCase();
      if (key === 'k') {
        e.preventDefault();
        setCo((co) => ({ cmdk: !co.cmdk }));
      } else if (key === 'b' && layoutOf() === 'sidebar') {
        e.preventDefault();
        flipSide(narrow);
      }
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, [narrow]);
  return html`${layout === 'sidebar' && side !== 'hidden' && html`<${Sidebar} rail=${side === 'rail'} narrow=${narrow} />`}
    <${TopBar} layout=${layout} side=${side} narrow=${narrow} />
    ${layout === 'palette' && html`<${QuickRow} />`}
    ${c.cmdk && html`<${Palette} narrow=${narrow} />`}`;
}

/** The shell's measures for the page under it (company.css: `--co-side-w`, `--co-top-h`). */
export function shellClass(narrow) {
  const layout = layoutOf();
  if (layout === 'palette') return 'co-shelled co-shell-palette';
  return 'co-shelled co-shell-' + sideState(narrow);
}

// --- the sidebar -------------------------------------------------------------------------------

/** Omsi-Hub's bus mark alone (the folded sidebar): the ring with the bus, the first stop. */
const MARK_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="140 98 392 392" class="co-side-busmark" role="img" aria-label="Omsi-Hub">' +
  `<path d="${MARK.route}" fill="none" style="stroke:var(--hub-accent-chosen)" stroke-width="${MARK.stroke}" stroke-linecap="round"/>` +
  `<path d="${MARK.bus}" style="fill:var(--hub-logo-ink)"/>` +
  MARK.stops[0][2] +
  '</svg>';

/** The game mode and the sandbox as one chip by the company's name ("Sandbox · Realistic"). */
function ModeChip({ sh }) {
  const words = [sh.sandbox?.label, sh.mode?.label].filter(Boolean).join(' · ');
  if (!words) return null;
  const can = mayChange() && !S.co.remote;
  const tip = [sh.sandbox?.tip, tr("The company's game mode: change it in the company's settings")].filter(Boolean).join('\n');
  return html`<button type="button" class=${'co-mode-chip' + (sh.mode?.casual ? ' casual' : '') + (can ? '' : ' still')} title=${tip} onClick=${() => can && setCo({ dialog: { kind: 'settings' } })}>
    <${Icon} name=${sh.sandbox ? sh.sandbox.icon : sh.mode?.icon || 'tune'} size=${12} /><span class="cut">${words}</span>
  </button>`;
}

function Sidebar({ rail, narrow }) {
  const c = S.co;
  const sh = c.shell;
  const groups = navGroups(c);
  const all = groups.flatMap((g) => g.items);
  const pinned = pins()
    .map((t) => all.find((it) => it.tab === t))
    .filter(Boolean);
  const on = (tab) => tab === c.tab || (sh.casual && tab !== HOME && sh.casual_pages?.pages?.[c.tab]?.parent === tab);
  const item = (it, inPins) => {
    const isPinned = pins().includes(it.tab);
    const canPin = typeof it.tab === 'number';
    return html`<div class=${'co-side-item' + (on(it.tab) ? ' on' : '')} key=${(inPins ? 'p' : 'g') + it.tab}>
      <button type="button" class="co-side-go" title=${rail ? `${it.name}${it.badge ? ` (${it.badge.n})` : ''}` : it.tip || it.name} onClick=${() => goPage(it.tab)}>
        <${Icon} name=${it.icon} size=${19} />
        <span class="co-side-name cut">${it.name}</span>
        ${it.badge && html`<span class=${'co-side-badge ' + (it.badge.tone || 'accent')}>${it.badge.n}</span>`}
      </button>
      ${!rail && canPin && html`<button type="button" class=${'co-side-star' + (isPinned ? ' pinned' : '')} title=${isPinned ? tr('Unpin') : tr('Pin to the top')} onClick=${() => flipPin(it.tab)}><${Icon} name="star" size=${14} /></button>`}
    </div>`;
  };
  return html`<aside class=${'co-side' + (rail ? ' rail' : '')}>
    <div class="co-side-brand" title="Omsi-Hub" dangerouslySetInnerHTML=${{ __html: rail ? MARK_SVG : LOGO_SVG }}></div>
    <div class="co-side-co" title=${sh.home}>
      <${Mark} mark=${sh.mark} size=${rail ? 34 : 40} />
      ${!rail && html`<span class="co-side-who"><b class="cut">${sh.name}</b><${ModeChip} sh=${sh} /></span>`}
    </div>
    <button type="button" class="co-side-search" title=${tr('Search pages and actions (Ctrl+K)')} onClick=${() => setCo({ cmdk: true })}>
      <${Icon} name="search" size=${17} />${!rail && html`<span class="grow cut">${tr('Search or jump to…')}</span><kbd>Ctrl K</kbd>`}
    </button>
    <nav class="co-side-scroll">
      ${pinned.length > 0 && html`<div class="co-side-group">
        ${rail ? html`<i class="co-side-rule"></i>` : html`<div class="co-side-head">${tr('Pinned')}</div>`}
        ${pinned.map((it) => item(it, true))}
      </div>`}
      ${groups.map((g, k) => html`<div class="co-side-group" key=${'h' + k}>
        ${(k > 0 || pinned.length > 0) && rail ? html`<i class="co-side-rule"></i>` : g.head && !rail ? html`<div class="co-side-head">${g.head}</div>` : null}
        ${g.items.map((it) => item(it, false))}
      </div>`)}
    </nav>
    <div class="co-side-foot">
      <button type="button" class="co-side-fold" title=${rail ? tr('Expand the sidebar') : tr('Collapse the sidebar')} onClick=${() => setSide(rail ? 'open' : 'rail', narrow)}>
        <${Icon} name=${rail ? 'chevron_right' : 'chevron_left'} size=${18} />${!rail && html`<span class="cut">${tr('Collapse the sidebar')}</span>`}
      </button>
      <button type="button" class="co-side-away" title=${tr('Hide the sidebar (Ctrl+B)')} onClick=${() => setSide('hidden', narrow)}>
        <${Icon} name="close" size=${16} />
      </button>
    </div>
  </aside>`;
}

// --- the bar over the page ---------------------------------------------------------------------

/** A dropdown's open state, shut by a click beside it or Escape. */
function usePop() {
  const [open, setOpen] = useState(false);
  const ref = useRef();
  useEffect(() => {
    if (!open) return undefined;
    const down = (e) => ref.current && !ref.current.contains(e.target) && setOpen(false);
    const key = (e) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', down, true);
    addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('mousedown', down, true);
      removeEventListener('keydown', key, true);
    };
  }, [open]);
  return [open, setOpen, ref];
}

function TopBar({ layout, side, narrow }) {
  const c = S.co;
  const sh = c.shell;
  const remote = remoteOf(c);
  // (a narrow bar: the words go first - the search's, the clock's line, Simulate's, Back's -,
  // then the reputation and the date; all back when the window changes)
  const ref = useRef();
  const [fit, setFit] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (el && el.scrollWidth > el.clientWidth + 1 && fit < 4) setFit(fit + 1);
  });
  useEffect(() => {
    const again = () => setFit(0);
    addEventListener('resize', again);
    return () => removeEventListener('resize', again);
  }, []);
  useEffect(() => setFit(0), [layout, side, sh.casual, c.tab]);
  const palette = layout === 'palette';
  return html`<header class=${'co-top' + (palette ? ' palette' : '') + ' fit' + fit} ref=${ref}>
    ${side === 'hidden' && html`<button type="button" class="co-top-handle" title=${tr('Show the sidebar (Ctrl+B)')} onClick=${() => flipSide(narrow)}><${Icon} name="menu" size=${20} /></button>`}
    ${palette && html`<span class="co-top-logo" title="Omsi-Hub" dangerouslySetInnerHTML=${{ __html: fit >= 2 ? MARK_SVG : LOGO_SVG }}></span>
      <span class="co-top-co" title=${sh.home}><${Mark} mark=${sh.mark} size=${30} /><span class="co-top-who"><b class="cut">${sh.name}</b><${ModeChip} sh=${sh} /></span></span>`}
    <${Where} />
    <span class="co-top-fill"></span>
    <button type="button" class=${'co-top-search' + (palette ? ' wide' : '')} title=${tr('Search pages and actions (Ctrl+K)')} onClick=${() => setCo({ cmdk: true })}>
      <${Icon} name="search" size=${17} /><span class="co-top-words cut">${tr('Search or jump to…')}</span><kbd>Ctrl K</kbd>
    </button>
    <${Status} />
    ${remote ? html`<${RemoteClock} />` : sh.casual ? html`<${CasualClock} />` : html`<${Clock} />`}
    ${!remote && !sh.casual && html`<${Bell} />`}
    <${Avatar} />
    <${Btn} class="co-back co-top-back" icon="chevron_left" label=${tr('Back')} title=${tr('Back')} onClick=${() => shellHooks.back()} />
  </header>`;
}

/** Where one is: the page's icon and name; in Casual the page above it a click away. */
function Where() {
  const c = S.co;
  const sh = c.shell;
  const tab = c.tab;
  if (sh.casual) {
    const parent = sh.casual_pages?.pages?.[tab]?.parent ?? HOME;
    const crumbs = tab === HOME ? [HOME] : [parent, tab].filter((t, k) => k === 1 || t !== HOME);
    return html`<span class="co-top-where">${crumbs.map(
      (t, k) => html`${k > 0 && html`<${Icon} name="chevron_right" size=${16} class="t-faint" />`}<button type="button" class=${'co-top-crumb' + (t === tab ? ' last' : '')} onClick=${() => t !== tab && setCo({ tab: t })}>${k === 0 && html`<${Icon} name=${pageIcon(sh, t)} size=${18} />`}<span class="cut">${pageName(sh, t)}</span></button>`,
    )}</span>`;
  }
  const name = pageName(sh, tab);
  return html`<span class="co-top-where">
    ${tab !== HOME && html`<button type="button" class="co-top-crumb" title=${tr('Every part of the company as a tile (Escape)')} onClick=${() => setCo({ tab: HOME })}><span class="cut">${pageName(sh, HOME)}</span></button><${Icon} name="chevron_right" size=${16} class="t-faint" />`}
    <span class="co-top-crumb last"><${Icon} name=${pageIcon(sh, tab)} size=${18} /><span class="cut">${name}</span></span>
  </span>`;
}

/** The cash and the reputation, each a click from its page; the trophies in their tooltips
 * (and on the Career and Research pages). */
function Status() {
  const sh = S.co.shell;
  const list = sh.status || [];
  const trophies = list.find((it) => it.icon === 'emoji_events');
  const shown = list.filter((it) => it !== trophies);
  return html`<span class="co-status co-top-status">
    ${shown.map((it) => html`<button type="button" class=${it.tab != null ? 'go' : ''} title=${trophies ? `${it.tip}\n${tr('Trophies')}: ${trophies.value}` : it.tip} onClick=${() => it.tab != null && goTab(it.tab)}>
      <span class=${'dot ' + it.tone}><${Icon} name=${it.icon} size=${15} /></span><span class=${'t-' + it.text}>${it.value}</span>
    </button>`)}
  </span>`;
}

/** The company's clock: the day and the time, the real clock's lead as a hint; one "Simulate to
 * tomorrow" with the other steps under it (×60, ×600, +1 h, the clock's dialog). */
function Clock() {
  const sh = S.co.shell;
  const k = sh.clock;
  const [open, setOpen, ref] = usePop();
  const playing = (speed) => Math.abs(k.speed - speed) < 1e-6;
  const play = (speed) => act('play', { speed: playing(speed) ? 0 : speed });
  const playTip = (speed, i) => {
    if (playing(speed)) return tr(k.real ? "Back to the real clock's pace" : 'Stop the clock');
    return tr(i === 1 ? "Run the company's clock at ten minutes a second while this page is open" : "Run the company's clock at a minute a second while this page is open");
  };
  const pick = (f) => () => {
    setOpen(false);
    f();
  };
  return html`<span class="co-clock co-top-clock">
    <span class="co-top-now" title=${k.tip || k.label}>
      <span class="co-top-day">${k.live && html`<i class="live"></i>`}<span class="date">${k.date}</span>${k.lead && html`<span class="co-top-lead">${k.lead}</span>`}</span>
      <span class=${'time' + (k.speed > 0 ? ' playing' : '') + (k.in_game ? ' game' : '')}>${k.time}</span>
    </span>
    <span class="co-top-split" ref=${ref}>
      <${Btn} kind="primary" class="co-tomorrow" icon="nights_stay" label=${tr('Simulate to tomorrow')} title=${tr(k.in_game ? "The game leads the company's clock while it runs" : 'Simulate the rest of the day: the tours run, what happens is told, and the day is closed at midnight')} onClick=${() => act('tomorrow')} />
      <button type="button" class=${'co-btn primary co-top-more' + (k.speed > 0 ? ' playing' : '')} title=${tr('More steps and the clock')} onClick=${() => setOpen(!open)}><${Icon} name="expand_more" size=${18} /></button>
      ${open && html`<div class="co-pop co-pop-right">
        ${[60, 600].map((_, i) => {
          const speed = i === 0 ? 1 : 10;
          return html`<button type="button" class=${'co-pop-item' + (playing(speed) ? ' on' : '')} title=${playTip(speed, i)} onClick=${pick(() => play(speed))}>
            <${Icon} name=${playing(speed) ? 'pause' : 'play_arrow'} size=${17} /><span class="grow">${i === 0 ? '×60' : '×600'}</span><span class="co-pop-note cut">${playTip(speed, i)}</span></button>`;
        })}
        <button type="button" class="co-pop-item" onClick=${pick(() => act('hour'))}><${Icon} name="schedule" size=${17} /><span class="grow">+1 h</span><span class="co-pop-note cut">${tr("Simulate one hour of the company's time")}</span></button>
        <i class="co-pop-rule"></i>
        <button type="button" class="co-pop-item" onClick=${pick(() => !sh.cover && setCo({ dialog: { kind: 'time' } }))}><${Icon} name="tune" size=${17} /><span class="grow cut">${tr("More steps, the clock's speed and the dispatcher")}</span></button>
      </div>`}
    </span>
  </span>`;
}

/** The boss's company's clock as a friend sees it: its day and time, nothing to run. */
function RemoteClock() {
  const k = S.co.shell.clock;
  return html`<span class="co-clock co-top-clock">
    <span class="co-top-now" title=${k.label}><span class="co-top-day"><span class="date">${k.date}</span></span><span class="time">${k.time}</span></span>
  </span>`;
}

/** Casual's clock (`casual::head`): the day and the time, and "Next day". */
function CasualClock() {
  const k = S.co.shell.clock;
  return html`<span class="co-clock co-top-clock">
    <span class="co-top-now"><span class="co-top-day"><span class="date">${k.date}</span></span><span class="time">${k.time}</span></span>
    <${Btn} kind="primary" class="co-next" icon="wb_sunny" label=${tr('Next day')} title=${tr('Run the rest of the day: the buses drive, the money comes in, and you start the next morning')} onClick=${() => act('next_day')} />
  </span>`;
}

/** The bell: the unread messages and what of the coming week needs the player (`co_view`
 * "bell"), each a click from its page. */
function Bell() {
  const sh = S.co.shell;
  const n = sh.bell || 0;
  const [open, setOpen, ref] = usePop();
  const [v, setV] = useState(null);
  useEffect(() => {
    if (!open) return undefined;
    let gone = false;
    cmd('co_view', { view: 'bell' })
      .then((r) => !gone && setV(r))
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, [open, sh.gen]);
  const go = (f) => {
    setOpen(false);
    f();
  };
  const part = (p, page, row) => html`<div class="co-pop-part">
    <div class="co-pop-head"><span class="grow">${p.title}${p.n > 0 && html` <b>${p.n}</b>`}</span><button type="button" class="co-pop-all" onClick=${() => go(() => goTab(page))}>${v.all}<${Icon} name="chevron_right" size=${14} /></button></div>
    ${p.list.length ? p.list.map(row) : html`<div class="co-pop-none">${p.none}</div>`}
  </div>`;
  return html`<span class="co-top-bell" ref=${ref}>
    <button type="button" class=${'co-btn sq co-top-bellbtn' + (open ? ' on' : '')} title=${tr('Notifications')} onClick=${() => setOpen(!open)}>
      <svg class="hub-icon" viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" /><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" /></svg>
    </button>
    ${n > 0 && html`<span class="co-top-count">${n > 99 ? '99+' : n}</span>`}
    ${open && html`<div class="co-pop co-pop-right co-pop-bell">
      ${!v
        ? html`<div class="co-pop-none">${tr('Reading…')}</div>`
        : html`${part(v.inbox, 13, (m) => html`<button type="button" class="co-pop-item two" onClick=${() => go(() => (setUi('inbox', { open: m.id }), act('inbox_read', { id: m.id }), goTab(13)))}>
              <${Icon} name=${m.icon} size=${17} class=${'t-' + (m.tone || 'accent')} /><span class="grow cut">${m.title}</span><span class="co-pop-note">${m.date}</span></button>`)}
            ${part(v.agenda, 14, (e) => html`<button type="button" class="co-pop-item two" onClick=${() => go(() => follow(e.go))}>
              <${Icon} name=${e.icon || 'event'} size=${17} class=${'t-' + (e.tone || 'warn')} /><span class="grow cut">${e.text}</span><span class="co-pop-note">${e.when}</span></button>`)}`}
    </div>`}
  </span>`;
}

/** The round menu: the account, multiplayer, the company's settings, another company, the
 * light or dark look, the tour (as far as a friend's role allows). */
function Avatar() {
  const c = S.co;
  const remote = !!c.remote;
  const [open, setOpen, ref] = usePop();
  const isDark = dark();
  const pick = (f) => (e) => {
    setOpen(false);
    f(e);
  };
  const item = (icon, label, f, more = {}) => html`<button type="button" class="co-pop-item" disabled=${more.disabled} title=${more.title || label} onClick=${pick(f)}><${Icon} name=${icon} size=${17} /><span class="grow cut">${label}</span>${more.kbd && html`<kbd>${more.kbd}</kbd>`}</button>`;
  const tg = togetherOn();
  return html`<span class="co-top-avatar" ref=${ref}>
    <button type="button" class=${'co-top-me' + (open ? ' on' : '') + (S.hub?.online ? ' online' : '')} title=${tr('Account')} onClick=${() => setOpen(!open)}><${Icon} name="account_circle" size=${22} /></button>
    ${open && html`<div class="co-pop co-pop-right">
      ${!remote && item('account_circle', tr('Account'), openAccount)}
      ${!remote && item('wifi_tethering', tr('Multiplayer'), openTogether, { disabled: !tg, title: tg ? tr('Multiplayer') : blockedLabel() })}
      ${mayChange(c) && item('settings', tr('Company settings'), () => setCo({ dialog: { kind: 'settings' } }))}
      ${!remote && item('sync_alt', tr('Other company'), otherCompany)}
      <i class="co-pop-rule"></i>
      ${item(isDark ? 'light_mode' : 'dark_mode', tr(isDark ? 'Switch to the light mode' : 'Switch to the dark mode'), (e) => flipLook(e))}
      ${!remote && item('help', tr('A tour of the bus company'), () => shellHooks.tour())}
      ${item('search', tr('Search pages and actions (Ctrl+K)'), () => setCo({ cmdk: true }), { kbd: 'Ctrl K' })}
    </div>`}
  </span>`;
}

/** The palette layout's row of shortcuts under its bar: the main pages, the ones pinned. */
function QuickRow() {
  const c = S.co;
  const sh = c.shell;
  const all = navGroups(c).flatMap((g) => g.items);
  const has = (t) => all.some((it) => it.tab === t);
  const main = sh.casual ? [HOME, CASUAL.BUSES, CASUAL.LINES, CASUAL.DRIVERS, CASUAL.MONEY] : [HOME, 0, 1, 2, 3, 'rf', 4];
  const tabs = [...main, ...pins().filter((t) => !main.includes(t))].filter((t) => t === 'rf' ? has(5) : has(t));
  return html`<nav class="co-top-quick">
    ${tabs.map((t) =>
      t === 'rf'
        ? html`<button type="button" class=${'co-top-chip' + (c.tab === 5 ? ' on' : '')} onClick=${() => toRosterFlow(null)}><${Icon} name="calendar_month" size=${16} />RosterFlow</button>`
        : html`<button type="button" class=${'co-top-chip' + (c.tab === t ? ' on' : '')} title=${all.find((it) => it.tab === t)?.tip || ''} onClick=${() => goPage(t)}><${Icon} name=${pageIcon(sh, t)} size=${16} />${pageName(sh, t)}</button>`,
    )}
  </nav>`;
}

// --- the command palette -----------------------------------------------------------------------

/** How well `q` matches `text`: a run of it scores most, at the start of a word more; the
 * letters in order (fuzzy) less; not at all 0. Accents and case are ignored. */
export function fuzzy(q, text) {
  const plain = (s) => String(s || '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const t = plain(text);
  const w = plain(q).trim();
  if (!w) return 1;
  const at = t.indexOf(w);
  if (at >= 0) return 1000 - at + (at === 0 || t[at - 1] === ' ' ? 200 : 0);
  let i = 0;
  let last = -2;
  let s = 0;
  for (const ch of w) {
    if (ch === ' ') continue;
    const j = t.indexOf(ch, i);
    if (j < 0) return 0;
    s += j === last + 1 ? 6 : 2;
    if (j === 0 || t[j - 1] === ' ') s += 4;
    last = j;
    i = j + 1;
  }
  return s;
}

/** Everything the palette finds: the pages, the main actions (as the game mode and a friend's
 * role allow them) and the recent places. */
function entries(narrow) {
  const c = S.co;
  const sh = c.shell;
  const remote = remoteOf(c);
  const groups = navGroups(c);
  const pages = groups.flatMap((g) => g.items.map((it) => ({ ...it, head: g.head })));
  const has = (t) => pages.some((p) => p.tab === t);
  const pageEntry = (p, group) => ({
    group,
    key: group + p.tab,
    icon: p.icon,
    title: p.name,
    sub: p.head || (p.tab === HOME ? '' : pageName(sh, HOME)),
    hint: p.tab === HOME ? 'Esc' : p.badge ? String(p.badge.n) : '',
    run: () => goPage(p.tab),
  });
  const out = [];
  for (const t of recent.filter((x) => x !== c.tab).slice(0, 4)) {
    const p = pages.find((x) => x.tab === t);
    if (p) out.push(pageEntry(p, 'recent'));
  }
  for (const p of pages) out.push(pageEntry(p, 'pages'));
  const a = (icon, title, sub, run, hint = '') => out.push({ group: 'actions', key: 'a' + title, icon, title, sub, hint, run });
  const k = sh.clock || {};
  if (!remote && sh.casual) {
    a('wb_sunny', tr('Next day'), tr('Run the rest of the day: the buses drive, the money comes in, and you start the next morning'), () => act('next_day'));
    if (has(CASUAL.SHOP)) a('directions_bus', tr('Buy a bus at the dealer'), pageName(sh, CASUAL.SHOP), () => goTab(CASUAL.SHOP));
    if (has(CASUAL.NEW_LINE) || has(CASUAL.LINES)) a('route', tr('Make a new line'), pageName(sh, CASUAL.LINES), () => goTab(CASUAL.NEW_LINE));
    if (has(CASUAL.DRIVERS)) a('groups', tr('Hire drivers'), pageName(sh, CASUAL.DRIVERS), () => goTab(CASUAL.DRIVERS));
  } else if (!remote) {
    a('nights_stay', tr('Simulate to tomorrow'), tr('Simulate the rest of the day: the tours run, what happens is told, and the day is closed at midnight'), () => act('tomorrow'));
    a('schedule', tr("Simulate one hour of the company's time"), '+1 h', () => act('hour'));
    a('play_arrow', '×60', tr("Run the company's clock at a minute a second while this page is open"), () => act('play', { speed: Math.abs(k.speed - 1) < 1e-6 ? 0 : 1 }));
    a('play_arrow', '×600', tr("Run the company's clock at ten minutes a second while this page is open"), () => act('play', { speed: Math.abs(k.speed - 10) < 1e-6 ? 0 : 10 }));
    a('tune', tr("More steps, the clock's speed and the dispatcher"), '', () => !sh.cover && setCo({ dialog: { kind: 'time' } }));
    if (has(1)) a('directions_bus', tr('Buy a bus at the dealer'), pageName(sh, 1), () => goKey('dealer'));
    if (has(2)) a('groups', tr('Hire drivers'), pageName(sh, 2), () => goKey('hire'));
    if (has(3)) a('route', tr('Make a new line'), tr('Line editor'), () => handOver('line-editor'));
    if (has(5)) {
      for (const [view, label, icon] of RF_TABS) a(icon, `RosterFlow: ${tr(label)}`, pageName(sh, 5), () => toRosterFlow(view));
    }
    if (has(OPS_TAB)) a('commute', tr('Send a spare bus'), pageName(sh, OPS_TAB), () => goTab(OPS_TAB));
    if (togetherOn()) a('wifi_tethering', tr('Start sharing'), tr('Multiplayer'), openTogether);
  }
  if (mayChange(c)) a('settings', tr('Company settings'), tr("The company's settings: its name, its date, how it buys"), () => setCo({ dialog: { kind: 'settings' } }));
  if (!remote) {
    a('account_circle', tr('Account'), tr('Multiplayer'), openAccount);
    a('sync_alt', tr('Other company'), '', otherCompany);
    a('help', tr('A tour of the bus company'), '', () => shellHooks.tour());
  }
  a(dark() ? 'light_mode' : 'dark_mode', tr(dark() ? 'Switch to the light mode' : 'Switch to the dark mode'), '', () => flipLook(null));
  if (layoutOf() === 'sidebar') {
    const away = sideState(narrow) === 'hidden';
    a('menu', tr(away ? 'Show the sidebar (Ctrl+B)' : 'Hide the sidebar (Ctrl+B)'), '', () => flipSide(narrow), 'Ctrl B');
  }
  a('display_settings', tr(layoutOf() === 'sidebar' ? 'Use the command palette layout' : 'Use the sidebar layout'), tr('Company layout'), () => saveSettings({ company_layout: layoutOf() === 'sidebar' ? 'palette' : 'sidebar' }));
  a('chevron_left', tr('Back'), '', () => shellHooks.back());
  return out;
}

const HEADS = { recent: 'Recent', pages: 'Pages', actions: 'Actions' };

/** The command palette (Ctrl+K): a search over the pages, the actions and the recent places,
 * grouped under their headings, each with its icon, its title, a grey line and a key; the
 * arrows, Enter and Escape. */
function Palette({ narrow }) {
  const [q, setQ] = useState('');
  const [at, setAt] = useState(0);
  const input = useRef();
  const list = useRef();
  useEffect(() => input.current?.focus(), []);
  const close = () => setCo({ cmdk: false });
  const all = entries(narrow);
  let shown;
  if (q.trim()) {
    shown = all
      .filter((e) => e.group !== 'recent')
      .map((e) => ({ e, s: Math.max(fuzzy(q, e.title), fuzzy(q, e.sub) * 0.5) }))
      .filter((x) => x.s > 0)
      .sort((x, y) => (x.e.group === y.e.group ? y.s - x.s : x.e.group === 'pages' ? -1 : 1))
      .map((x) => x.e)
      .slice(0, 40);
  } else {
    shown = all;
  }
  const k = Math.min(at, Math.max(0, shown.length - 1));
  useEffect(() => {
    list.current?.querySelector('.co-cmdk-row.on')?.scrollIntoView({ block: 'nearest' });
  }, [k, q]);
  const run = (e) => {
    if (!e) return;
    close();
    e.run();
  };
  const key = (ev) => {
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      setAt((k + 1) % Math.max(1, shown.length));
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      setAt((k - 1 + shown.length) % Math.max(1, shown.length));
    } else if (ev.key === 'Enter') {
      ev.preventDefault();
      run(shown[k]);
    } else if (ev.key === 'Escape') {
      ev.preventDefault();
      ev.stopPropagation();
      close();
    }
  };
  let head = null;
  return html`<div class="co-cmdk" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="co-cmdk-box" role="dialog" aria-label=${tr('Search pages and actions (Ctrl+K)')}>
      <label class="co-cmdk-field">
        <${Icon} name="search" size=${20} />
        <input ref=${input} value=${q} placeholder=${tr('Search or jump to…')} onInput=${(e) => (setQ(e.currentTarget.value), setAt(0))} onKeyDown=${key} spellcheck=${false} autocomplete="off" />
        <kbd>Esc</kbd>
      </label>
      <div class="co-cmdk-list" ref=${list}>
        ${shown.length === 0 && html`<div class="co-cmdk-none">${tr('Nothing found')}</div>`}
        ${shown.map((e, i) => {
          const h = e.group !== head ? (head = e.group) : null;
          return html`${h && html`<div class="co-cmdk-head">${tr(HEADS[h])}</div>`}
            <button type="button" class=${'co-cmdk-row' + (i === k ? ' on' : '')} key=${e.key} onMouseMove=${() => i !== k && setAt(i)} onClick=${() => run(e)}>
              <span class="co-cmdk-icon"><${Icon} name=${e.icon} size=${18} /></span>
              <span class="co-cmdk-words"><b class="cut">${e.title}</b>${e.sub && html`<span class="cut">${e.sub}</span>`}</span>
              ${i === k ? html`<kbd>↵</kbd>` : e.hint && html`<kbd class="soft">${e.hint}</kbd>`}
            </button>`;
        })}
      </div>
      <div class="co-cmdk-foot"><span><kbd>↑</kbd><kbd>↓</kbd>${tr('to move')}</span><span><kbd>↵</kbd>${tr('to open')}</span><span><kbd>Esc</kbd>${tr('to close')}</span></div>
    </div>
  </div>`;
}

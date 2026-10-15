// The bus company in the desktop launcher (Trello "Tauri 2/6"), as our game-drawn launcher's
// company screen (`crates/omsi-launcher-ui/src/company/mod.rs`, `screen`): the whole window
// its own - its bar across the top (the company, its clock and time controls, its settings,
// the way back), the way round its pages under it, and the page filling the rest; over them
// the dialog open, the day's report, a decision the clock waits for, and the popup.
//
// What the company is and does lives in Rust (`omsi-launcher-api`'s `omsihub/company`): this
// page asks for views and sends actions, and ticks the company's clock four times a second
// (`co_tick`), which answers with the bar's and the strip's shell.

import { useRef, useState } from '../../../vendor/preact-htm.js';
import { S, useStore, set } from '../../store.js';
import { call } from '../../api.js';
import { contrast } from '../theme.js';
import { dark } from '../theme.js';
import { flipLook } from '../bar.js';
import { html, useEffect, tr, HOME, HANDED, CASUAL, setCo, freshCo, cmd, act, takeShell, goTab, handOver, Icon, Mark, ModePill, Btn, Guard } from './kit.js';
import { Chooser, Wizard, readCompanies } from './chooser.js';
import { Overlays, modalOpen } from './dialogs.js';
import { HomePage } from './home.js';
import { OverviewPage } from './overview.js';
import { FleetPage } from './fleet.js';
import { PeoplePage } from './people.js';
import { LinesPage } from './lines.js';
import { MoneyPage } from './money.js';
import { InboxPage } from './inbox.js';
// (Beta 57: what is coming)
import { AgendaPage } from './agenda.js';
import { ReputationPage } from './reputation.js';
import { OpsPage } from './ops.js';
import { CareerPage } from './career.js';
import { CasualPage } from './casual.js';
import { LaunchPage } from './launch.js';
// Omsi-Hub 4/6: RosterFlow, the Planning page with the add-on on, over the whole window.
import { RosterFlowPage } from '../rosterflow/index.js';
import { MorePage, MORE_TABS } from './pages.js';
// Omsi-Hub 6/6: the company's own guided tour
import { startCompanyTour } from '../tour.js';
// (Beta 58: "Play together")
import { TogetherButton } from './together.js';

// (the company's own look, loaded with its first page)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/company.css';
document.head.append(sheet);

/** Words in a meaning's colour on the light sheet: as dark as they must be to read there
 * (`theme::to_light` for words: 4.5:1 on the light panel), the fills as they are. */
const LIGHT_PANEL = [255, 254, 252];
function onLight(hex) {
  let c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  for (let k = 0; contrast(c, LIGHT_PANEL) < 4.5 && k < 60; k++) c = c.map((v) => v * 0.96);
  return '#' + c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
}
const WORDS = { '--co-ok': '#34b26e', '--co-warn': '#e8aa46', '--co-danger': '#d93a30', '--co-danger-text': '#e36b64', '--co-early': '#7aa8ff', '--co-line': '#ffd23f' };
let themed = null;
function applyCoTheme() {
  const isDark = dark();
  if (themed === isDark) return;
  themed = isDark;
  const root = document.documentElement;
  for (const [k, v] of Object.entries(WORDS)) {
    if (isDark) root.style.removeProperty(k);
    else root.style.setProperty(k, k === '--co-danger-text' ? onLight('#d93a30') : onLight(v));
  }
}

/** Enter the bus company from the start: how to play first (`chooser::choose`). */
export function enterCompany(over = {}) {
  set({ co: freshCo(over), page: 'company' });
}

/** The company's page `OMSI_LAUNCHER_PAGE` names ("bus company[:<page>[:<company>]]", as our
 * game-drawn launcher once read it): a number of its pages, "home", a Casual page's name, or
 * "choose" for the start. */
export function companyPage(part, id) {
  const casual = { buses: CASUAL.BUSES, shop: CASUAL.SHOP, lines: CASUAL.LINES, 'new-line': CASUAL.NEW_LINE, drivers: CASUAL.DRIVERS, money: CASUAL.MONEY, company: CASUAL.COMPANY, shifts: CASUAL.SHIFTS };
  if (part === 'choose') return enterCompany();
  // (its tour, from where it was left: `company::handed`'s "tour")
  if (part === 'tour') return enterCompany({ chooser: null, tab: HOME, want: id || null, tourWant: true });
  // Omsi-Hub 4/6: the Planning page opens RosterFlow when its add-on is on, as the game-drawn
  // launcher does ("5~<state>": RosterFlow in a state of its own, for the comparison pictures)
  const [first, shot] = String(part || '').split('~');
  if (first === '5') return enterCompany({ chooser: null, tab: HOME, want: id || null, rfWant: { shot: shot || null } });
  let tab = HOME;
  if (/^\d+$/.test(part || '')) tab = Number(part);
  else if (part in casual) tab = casual[part];
  // (Omsi-Hub 6/6: what `company::handed` opens - a line's timetable, the line editor, the livery
  // studio ... - opened here once the company is: `handOver` takes it to its page)
  else if (part && part !== 'home') return enterCompany({ chooser: null, tab: HOME, want: id || null, handWant: part });
  enterCompany({ chooser: null, tab: HANDED.includes(tab) ? HOME : tab, want: id || null });
}

let stopTick = null;
/** For which driver (and with a company or not) the company's tour was looked at this session
 * (`CompanyView::tutorial_for`). */
let tourAsked = null;

/** The "?" in the company's bar: the tour from where it was left (`tutorial::ask`). */
function askTour() {
  cmd('co_tour', { what: 'ask' })
    .then((r) => startCompanyTour(r?.at || 0))
    .catch(() => startCompanyTour(0));
}

/** The clock's tick (`clock::tick`): four times a second, the shell back with it; a dialog or
 * a decision open holds what waits for it (`modal`). */
function startTick() {
  if (stopTick) return;
  let busy = false;
  const t = setInterval(async () => {
    if (busy || S.page !== 'company') return;
    busy = true;
    try {
      // (the self-check holds the clock: nothing is simulated while it looks)
      takeShell(await cmd('co_tick', { modal: modalOpen(), still: !!S.boot?.selftest }));
    } catch (e) {
      // (the next tick tries again; what went wrong is shown while the companies are read)
      if (!S.co.shell) setCo({ failed: e.message });
    }
    busy = false;
  }, 250);
  stopTick = () => clearInterval(t);
}

/** Into the bus company: the companies read, the one wanted opened (or the first, as our
 * game-drawn launcher opens one when it is started on a page of the company). */
async function boot() {
  try {
    const b = await cmd('co_boot');
    const list = b.chooser?.companies?.tiles || [];
    setCo({ list: b.chooser });
    const want = S.co.want && list.find((c) => c.id === S.co.want) ? S.co.want : null;
    if (S.co.chooser == null && !b.open && list.length) await cmd('co_open', { id: want || list[0].id });
    else if (want && b.open !== want) await cmd('co_open', { id: want });
    takeShell(await cmd('co_tick', { modal: true, still: true }));
  } catch (e) {
    setCo({ failed: e.message });
  }
}

/** Leave the company's pages: the clock stops and what it left unsaved is saved
 * (`company::leave`); back to the start. */
export async function leaveCompany(to = 'drive') {
  try {
    await cmd('co_leave');
  } catch {
    /* (nothing to save) */
  }
  if (stopTick) stopTick();
  stopTick = null;
  set({ page: to, hub: { ...S.hub, step: 'mode' } });
}

/** The Casual page above `tab` (the home for a page). */
const parentOf = (sh, tab) => sh?.casual_pages?.pages?.[tab]?.parent ?? HOME;

export function CompanyScreen() {
  const s = useStore();
  const c = s.co;
  const sh = c.shell;
  applyCoTheme();
  useEffect(() => {
    boot();
    startTick();
    return () => {
      if (stopTick) stopTick();
      stopTick = null;
    };
  }, []);
  // Escape: back to the start screen (in Casual to the page above), unless something lies over
  // the page or a field is being typed in (`escape_home`)
  useEffect(() => {
    const k = (e) => {
      if (e.key !== 'Escape' || modalOpen() || !S.co.shell?.open || S.co.chooser || S.co.wizard || S.co.shell.launch) return;
      // (RosterFlow has its own Escape: Omsi-Hub 4/6)
      if (S.rf?.open && S.co.tab === 5) return;
      const el = document.activeElement;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      setCo({ tab: S.co.shell.casual ? parentOf(S.co.shell, S.co.tab) : HOME });
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  const read = sh && sh.companies != null;
  const choosing = read && c.chooser && !c.wizard;
  const founding = read && !choosing && (c.wizard || !sh.open);
  const open = read && !choosing && !founding && sh.open;
  // (Omsi-Hub 4/6: the Planning page asked for at the start, once the company is open)
  useEffect(() => {
    if (!open || !c.rfWant) return;
    const more = c.rfWant;
    setCo({ rfWant: null });
    handOver('5', more);
  }, [open, c.rfWant]);
  useEffect(() => {
    if (!open || !c.handWant) return;
    const part = c.handWant;
    setCo({ handWant: null });
    handOver(part.replace(/%3A/g, ':').replace(/%25/g, '%'));
  }, [open, c.handWant]);
  // (Omsi-Hub 6/6: the company's tour by itself the first time a driver comes - the welcome and
  // the founding without a company, its pages once one runs - not over a dialog nor another
  // tour, not in the self-check: `tutorial::frame`; asked for at the start, from the "?"'s place)
  const tourKey = read && !choosing && !c.dialog && !s.hubTour && !s.boot?.selftest ? `${s.config?.profile || ''}|${open ? 1 : 0}` : null;
  useEffect(() => {
    if (!tourKey) return;
    if (c.tourWant && open) {
      setCo({ tourWant: false });
      askTour();
      return;
    }
    if (tourAsked === tourKey) return;
    tourAsked = tourKey;
    cmd('co_tour', { what: 'open', company: !!open, casual: !!sh?.casual })
      .then((r) => r?.at != null && startCompanyTour(r.at))
      .catch(() => {});
  }, [tourKey, c.tourWant]);
  let body;
  if (!read) body = html`<div class="co-body found"><div class="co-empty">${c.failed || tr('Reading your companies…')}</div></div>`;
  else if (choosing) body = html`<div class="co-body found"><${Chooser} /></div>`;
  else if (founding) body = html`<div class="co-body found"><${Wizard} /></div>`;
  else if (sh.launch) body = html`<div class="co-body full"><${LaunchPage} /></div>`;
  else body = html`${sh.casual ? html`<${CasualNav} />` : html`<${Nav} />`}<div class="co-body" key=${'tab' + c.tab}><${Page} /></div>`;
  // (Omsi-Hub 4/6: RosterFlow an application of its own over the whole window, no company bar;
  // its dialogs and the popup still over it)
  const rosterflow = open && !sh.casual && !sh.launch && c.tab === 5 && s.rf?.open;
  if (rosterflow) {
    return html`<div class="co" data-tab="5">
      <${Guard}><${RosterFlowPage} /></${Guard}>
      <${Guard}><${Overlays} founding=${false} /></${Guard}>
      ${c.notice && Date.now() - c.notice.at < 9000 && html`<div class=${'co-notice' + (c.notice.error ? ' error' : '')} key=${c.notice.at}>${c.notice.text}</div>`}
    </div>`;
  }
  return html`<div class="co" data-tab=${open ? c.tab : ''}>
    <${Guard}><${Bar} open=${open} choosing=${choosing} read=${read} /></${Guard}>
    <${Guard}>${body}</${Guard}>
    ${read && html`<${Guard}><${Overlays} founding=${founding || choosing} /></${Guard}>`}
    ${c.notice && Date.now() - c.notice.at < 9000 && html`<div class=${'co-notice' + (c.notice.error ? ' error' : '')} key=${c.notice.at}>${c.notice.text}</div>`}
  </div>`;
}

/** The page of the tab open (`company::pages`). */
function Page() {
  const c = S.co;
  const sh = c.shell;
  if (sh.casual) return html`<${CasualPage} tab=${c.tab} />`;
  switch (c.tab) {
    case 0:
      return html`<${OverviewPage} />`;
    case 1:
      return html`<${FleetPage} />`;
    case 2:
      return html`<${PeoplePage} />`;
    case 3:
      return html`<${LinesPage} />`;
    case 4:
      return html`<${MoneyPage} />`;
    case 6:
      return html`<${CareerPage} />`;
    case 13:
      return html`<${InboxPage} />`;
    case 14:
      return html`<${AgendaPage} />`;
    case 15:
      return html`<${OpsPage} />`;
    case 16:
      return html`<${ReputationPage} />`;
    default:
      // (the planning, the depot, the concessions, the fleet map, the research, the workshop
      // and the contracts: `pages.js`, Trello "Tauri 3/6")
      return MORE_TABS.includes(c.tab) ? html`<${MorePage} tab=${c.tab} />` : html`<${HomePage} />`;
  }
}

// --- the bar --------------------------------------------------------------------------------

function Bar({ open, choosing, read }) {
  const c = S.co;
  const sh = c.shell;
  const isDark = dark();
  const settings = () => setCo({ dialog: { kind: 'settings' } });
  // (Omsi-Hub 6/6: on a narrow bar - a small window, a large "Launcher size" - the company's
  // day and time go first, then the status's figures from the right, as `clock::head` and
  // `company::status` leave out what has no room; all back when the window changes)
  const ref = useRef();
  const [fit, setFit] = useState({ now: true, n: 3 });
  useEffect(() => {
    const el = ref.current;
    if (!el || el.scrollWidth <= el.clientWidth + 1) return;
    if (fit.now && open && !sh?.casual) setFit({ ...fit, now: false });
    else if (fit.n > 0) setFit({ ...fit, n: fit.n - 1 });
  });
  useEffect(() => {
    const again = () => setFit({ now: true, n: 3 });
    addEventListener('resize', again);
    return () => removeEventListener('resize', again);
  }, []);
  return html`<header class="co-bar" ref=${ref}>
    ${open
      ? html`<span title=${sh.home} style="display:contents"><${Mark} mark=${sh.mark} size=${44} /><span class="name cut" title=${sh.home}>${sh.name}</span></span>
        <${ModePill} mode=${sh.mode} onClick=${settings} title=${tr("The company's game mode: change it in the company's settings")} />
        <${Status} n=${fit.n} />`
      : html`<span class="lone"><${Icon} name="garage" size=${26} class="t-accent2" /><span><b>${tr('Bus company')}</b><span>${tr('Your own transport company: buses, people and lines, in a time of its own.')}</span></span></span>`}
    <span class="tools">
      ${open && html`${sh.casual ? html`<${CasualClock} />` : html`<${Clock} now=${fit.now} />`}<span class="gap16"></span>
        <${TogetherButton} />
        <${Btn} class="sq" icon="settings" title=${tr("The company's settings: its name, its date, how it buys")} onClick=${settings} /><span class="gap8"></span>`}
      <${Btn} class="sq" icon=${isDark ? 'dark_mode' : 'light_mode'} title=${tr(isDark ? 'Switch to the light mode' : 'Switch to the dark mode')} onClick=${(e) => flipLook(e)} /><span class="gap8"></span>
      ${read && !choosing && html`<${Btn} class="sq co-help" icon="help" title=${tr('A tour of the bus company')} onClick=${askTour} /><span class="gap12"></span>`}
      <${Btn} class="co-back" icon="chevron_left" label=${tr('Back')} title=${tr('Back')} style="width:112px" onClick=${() => leaveCompany()} />
    </span>
  </header>`;
}

/** What the company has: its cash, its trophies and its reputation, each a click from the page
 * about it (`company::status`). */
function Status({ n = 3 }) {
  const sh = S.co.shell;
  if (!n) return null;
  return html`<span class="co-status">
    ${sh.status.slice(0, n).map((it) => html`<button type="button" class=${it.tab != null ? 'go' : ''} title=${it.tip} onClick=${() => it.tab != null && goTab(it.tab)}>
      <span class=${'dot ' + it.tone}><${Icon} name=${it.icon} size=${16} /></span><span class=${'t-' + it.text}>${it.value}</span>
    </button>`)}
  </span>`;
}

/** The company's clock in the bar (`clock::head`): the day and the time - with the real clock
 * its live dot and how far it was fast-forwarded - the play speeds where there is room, +1 h,
 * Simulate to tomorrow and the time dialog. */
function Clock({ now = true }) {
  const sh = S.co.shell;
  const k = sh.clock;
  const speeds = [
    ['×60', 1, 62],
    ['×600', 10, 70],
  ];
  const play = (speed) => {
    const on = Math.abs(k.speed - speed) < 1e-6;
    act('play', { speed: on ? 0 : speed });
  };
  const playTip = (speed, i) => {
    const on = Math.abs(k.speed - speed) < 1e-6;
    if (on) return tr(k.real ? "Back to the real clock's pace" : 'Stop the clock');
    return tr(i === 1 ? "Run the company's clock at ten minutes a second while this page is open" : "Run the company's clock at a minute a second while this page is open");
  };
  return html`<span class="co-clock">
    ${now && html`<span class="now" title=${k.tip || undefined}>
      <span class=${'label' + (k.in_game ? ' game' : '')}>${k.lead && html`<span class="lead">${k.lead}</span>`}${k.live && html`<i class="live"></i>`}${k.label}</span>
      <span class="date">${k.date}</span><span class=${'time' + (k.speed > 0 ? ' playing' : '')}>${k.time}</span>
    </span>`}
    <span class="play" style="gap:8px">
      ${speeds.map(([label, speed, w], i) => html`<${Btn} kind=${Math.abs(k.speed - speed) < 1e-6 ? 'primary' : 'ghost'} label=${label} style=${`width:${w}px;padding:0`} title=${playTip(speed, i)} onClick=${() => play(speed)} />`)}
    </span>
    <${Btn} label="+1 h" style="width:70px;padding:0" title=${tr("Simulate one hour of the company's time")} onClick=${() => act('hour')} />
    <${Btn} kind="primary" class="co-tomorrow" icon="nights_stay" label=${tr('Simulate to tomorrow')} title=${tr(k.in_game ? "The game leads the company's clock while it runs" : 'Simulate the rest of the day: the tours run, what happens is told, and the day is closed at midnight')} onClick=${() => act('tomorrow')} />
    <${Btn} class="sq" icon="schedule" style="width:42px" title=${tr("More steps, the clock's speed and the dispatcher")} onClick=${() => !sh.cover && setCo({ dialog: { kind: 'time' } })} />
  </span>`;
}

/** Casual's clock (`casual::head`): the day and the time, and "Next day". */
function CasualClock() {
  const k = S.co.shell.clock;
  return html`<span class="co-clock">
    <span class="casual-now"><span class="date">${k.date}</span><span class="time">${k.time}</span></span>
    <${Btn} kind="primary" class="co-next" icon="wb_sunny" label=${tr('Next day')} style="min-width:150px" title=${tr('Run the rest of the day: the buses drive, the money comes in, and you start the next morning')} onClick=${() => act('next_day')} />
  </span>`;
}

// --- the way round --------------------------------------------------------------------------

/** The way round the company's pages under its bar (`company::nav`): the start screen's button
 * with the page one is on after it, and every page as an icon on the right, grouped as the
 * start screen groups them, a badge where something waits. */
function Nav() {
  const c = S.co;
  const sh = c.shell;
  const home = c.tab === HOME;
  return html`<nav class="co-nav">
    <${Btn} kind=${home ? 'primary' : ''} class="co-start" icon="apps" label=${tr('Start screen')} title=${tr('Every part of the company as a tile (Escape)')} onClick=${() => !home && setCo({ tab: HOME })} />
    <span class="where">
      ${home
        ? html`<span class="home cut">${sh.home}</span>`
        : html`<${Icon} name="chevron_right" size=${18} /><span class="page cut"><${Icon} name=${ICON_OF(sh, c.tab)} size=${20} />${sh.names?.[c.tab] || ''}</span>`}
    </span>
    <${Rail} groups=${sh.rail} on=${(t) => t === c.tab} />
  </nav>`;
}

const ICON_OF = (sh, tab) => sh.rail.flat().find((x) => x.tab === tab)?.icon || 'apps';

/** The pages as icons, grouped, a badge where something waits. */
function Rail({ groups, on, casual }) {
  return html`<span class=${'co-rail' + (casual ? ' casual' : '')}>
    ${groups.map((g, k) => html`${k > 0 && html`<span class="sep"></span>`}<span class="group">
      ${g.map((it) => html`<button type="button" class=${on(it.tab) ? 'on' : ''} title=${it.tip} onClick=${() => goTab(it.tab)}>
        <${Icon} name=${it.icon} size=${casual ? 20 : 19} />
        ${it.badge && html`<span class=${'badge ' + it.badge.tone}>${it.badge.n}</span>`}
      </button>`)}
    </span>`)}
  </span>`;
}

/** Casual's way round (`casual::nav`): Home, the page one is on (the page above it a click
 * away), and the pages as icons. */
function CasualNav() {
  const c = S.co;
  const sh = c.shell;
  const tab = c.tab;
  const home = tab === HOME;
  const pages = sh.casual_pages?.pages || {};
  const parent = parentOf(sh, tab);
  const crumbs = [parent, tab].filter((t) => t !== HOME);
  return html`<nav class="co-nav">
    <${Btn} kind=${home ? 'primary' : ''} class="co-start" icon="home" label=${tr('Home')} style="min-width:110px" title=${tr("Back to your company's home (Escape)")} onClick=${() => !home && setCo({ tab: HOME })} />
    <span class="where">
      ${home
        ? html`<span class="home cut">${sh.casual_pages?.home}</span>`
        : crumbs.map((t) => html`<${Icon} name="chevron_right" size=${18} /><button type="button" class=${'crumb' + (t === tab ? ' last' : '')} onClick=${() => t !== tab && setCo({ tab: t })}>${pages[t]?.name}</button>`)}
    </span>
    <${Rail} casual groups=${sh.rail} on=${(t) => t === tab || parent === t} />
  </nav>`;
}



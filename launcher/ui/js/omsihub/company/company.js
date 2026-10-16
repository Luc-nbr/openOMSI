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
import { S, useStore, set, remembered, remember } from '../../store.js';
import { call } from '../../api.js';
import { contrast } from '../theme.js';
import { dark } from '../theme.js';
import { flipLook } from '../bar.js';
import { html, useEffect, tr, HOME, HANDED, CASUAL, setCo, setUi, freshCo, cmd, act, takeShell, goTab, handOver, useView, Icon, Mark, ModePill, Btn, Guard } from './kit.js';
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
import { AgendaPage, follow } from './agenda.js';
import { ReputationPage } from './reputation.js';
import { OpsPage } from './ops.js';
import { CareerPage } from './career.js';
import { CasualPage } from './casual.js';
// (Beta 61: a sandbox company's rules, and the badge by its name)
import { SandboxPage, SandboxBadge } from './sandbox.js';
import { LaunchPage } from './launch.js';
// Omsi-Hub 4/6: RosterFlow, the Planning page with the add-on on, over the whole window.
import { RosterFlowPage } from '../rosterflow/index.js';
import { MorePage, MORE_TABS } from './pages.js';
// Omsi-Hub 6/6: the company's own guided tour
import { startCompanyTour } from '../tour.js';
import { SessionWatch } from './session.js';
// (Beta 58: "Play together"; Beta 61: the Multiplayer page, and the company's pages of a friend
// who joined one - `S.co.remote`, its commands relayed to the boss's launcher)
import { TogetherButton, TOGETHER_TAB, backToMenu } from './together.js';
import { OnlineButton } from '../online.js';
// (Beta 63: a cloud company's newer version or conflict, asked before it opens)
import { cloudBeforeOpen } from '../cloud.js';
import { MultiplayerPage } from './multiplayer.js';
import { togetherOn, blockedLabel } from './gate.js';
import { CommandPalette, noteVisit } from './cmdk.js';

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
/** When the boss's launcher was last asked for the shell (a relayed tick goes over the network:
 * not four times a second). */
let remoteAt = 0;
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
    if (S.co.remote) {
      if (Date.now() - remoteAt < 1500) return;
      remoteAt = Date.now();
    }
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
  // (a friend's look at the boss's company: its shell from the boss's launcher, nothing opened here)
  if (S.co.remote) {
    try {
      takeShell(await cmd('co_tick', { modal: true, still: true }));
    } catch (e) {
      setCo({ failed: e.message });
    }
    return;
  }
  try {
    const b = await cmd('co_boot');
    const list = b.chooser?.companies?.tiles || [];
    setCo({ list: b.chooser });
    const want = S.co.want && list.find((c) => c.id === S.co.want) ? S.co.want : null;
    // (a cloud company: a newer version or a conflict is asked before it opens - `cloud.js`)
    const opening = S.co.chooser == null && !b.open && list.length ? want || list[0].id : want && b.open !== want ? want : null;
    if (opening) {
      await cloudBeforeOpen(opening);
      await cmd('co_open', { id: opening });
    }
    takeShell(await cmd('co_tick', { modal: true, still: true }));
  } catch (e) {
    setCo({ failed: e.message });
  }
}

/** Leave the company's pages: the clock stops and what it left unsaved is saved
 * (`company::leave`); back to the start. */
export async function leaveCompany(to = 'drive') {
  if (S.co.remote) {
    backToMenu();
    return;
  }
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
  // (the sidebar as it was left - as its icons on a narrow stage - and folded to them when the
  // window gets narrow; Ctrl+B slides it away and back)
  useEffect(() => {
    if (!S.co.nav) setCo({ nav: navStart() });
    const narrow = () => S.co.nav === 'full' && stageW() < 1000 && setCo({ nav: 'rail' });
    const k = (e) => {
      const key = e.key.toLowerCase();
      if (!(e.ctrlKey || e.metaKey) || e.altKey || (key !== 'b' && key !== 'k')) return;
      if (modalOpen() || !S.co.shell?.open || S.co.chooser || S.co.wizard || S.co.shell.launch || (S.rf?.open && S.co.tab === 5)) return;
      e.preventDefault();
      if (key === 'b') toggleSide();
      else setCo({ cmdk: !S.co.cmdk });
    };
    addEventListener('resize', narrow);
    addEventListener('keydown', k);
    return () => {
      removeEventListener('resize', narrow);
      removeEventListener('keydown', k);
    };
  }, []);
  // Escape: back to the start screen (in Casual to the page above), unless something lies over
  // the page or a field is being typed in (`escape_home`)
  useEffect(() => {
    const k = (e) => {
      if (e.key !== 'Escape' || S.co.cmdk || modalOpen() || !S.co.shell?.open || S.co.chooser || S.co.wizard || S.co.shell.launch) return;
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
  // (a friend on the boss's company: only the pages his role allows, the first of them when the
  // one shown is not - the start screen shows the money)
  const remote = open && c.remote && sh.remote;
  useEffect(() => {
    if (!remote) return;
    const pages = sh.pages || [];
    const home = c.tab === HOME && (sh.remote.perms || []).includes('finances');
    if (!home && !pages.includes(c.tab) && pages.length) setCo({ tab: pages[0] });
  }, [remote, c.tab, (sh?.pages || []).join(',')]);
  // (the places visited, for the palette's Recent)
  useEffect(() => {
    if (open && !c.remote) noteVisit(sh.id, c.tab);
  }, [open, c.tab, sh?.id]);
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
  const tourKey = read && !choosing && !c.dialog && !s.hubTour && !s.boot?.selftest && !c.remote ? `${s.config?.profile || ''}|${open ? 1 : 0}` : null;
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
  else body = html`<div class="co-body" key=${'tab' + c.tab}><${Page} /></div>`;
  // (Omsi-Hub 4/6: RosterFlow an application of its own over the whole window, no company bar;
  // its dialogs and the popup still over it)
  const rosterflow = open && !sh.casual && !sh.launch && c.tab === 5 && s.rf?.open;
  if (rosterflow) {
    return html`<div class="co" data-tab="5">
      <${Guard}><${RosterFlowPage} /></${Guard}>
      <${Guard}><${Overlays} founding=${false} /></${Guard}>
      ${c.remote && html`<${SessionWatch} />`}
      ${c.notice && Date.now() - c.notice.at < 9000 && html`<div class=${'co-notice' + (c.notice.error ? ' error' : '')} key=${c.notice.at}>${c.notice.text}</div>`}
    </div>`;
  }
  // (the company's shell: its sidebar, the slim bar over the page - the start, the founding and
  // a game's launch keep the bar across the whole window)
  const shell = open && !sh.launch;
  const mayChange = !c.remote || (sh?.remote?.perms || []).includes('settings');
  return html`<div class=${'co' + (shell ? ' co-shell' : '')} data-tab=${open ? c.tab : ''} data-nav=${shell ? c.nav || 'full' : undefined}>
    ${shell
      ? html`<${Guard}><${Side} mayChange=${mayChange} /></${Guard}><${Guard}><${Top} mayChange=${mayChange} /></${Guard}>`
      : html`<${Guard}><${Bar} open=${open} choosing=${choosing} read=${read} /></${Guard}>`}
    <${Guard}>${body}</${Guard}>
    ${shell && c.cmdk && html`<${Guard}><${CommandPalette} toggleSide=${toggleSide} setNav=${setNav} askTour=${askTour} /></${Guard}>`}
    ${read && html`<${Guard}><${Overlays} founding=${founding || choosing} /></${Guard}>`}
    ${c.remote && html`<${SessionWatch} />`}
    ${c.notice && Date.now() - c.notice.at < 9000 && html`<div class=${'co-notice' + (c.notice.error ? ' error' : '')} key=${c.notice.at}>${c.notice.text}</div>`}
  </div>`;
}

/** The page of the tab open (`company::pages`). */
function Page() {
  const c = S.co;
  const sh = c.shell;
  // (a sandbox's own page, in Casual too: `company::SANDBOX_TAB`)
  if (sh.sandbox && c.tab === sh.sandbox.tab) return html`<${SandboxPage} />`;
  // (Beta 61: the players of "Play together", in Casual too: `company::TOGETHER_TAB`)
  if (c.tab === TOGETHER_TAB && !c.remote) return html`<${MultiplayerPage} />`;
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
  // (Beta 61: a friend on the boss's company - no clock to run, the settings only when his role
  // may change them)
  const remote = open && !!c.remote;
  const mayChange = !remote || (sh?.remote?.perms || []).includes('settings');
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
        <${SandboxBadge} b=${sh.sandbox} onClick=${() => goTab(sh.sandbox.tab)} />
        <${ModePill} mode=${sh.mode} onClick=${mayChange ? settings : null} title=${tr("The company's game mode: change it in the company's settings")} />
        ${remote && html`<span class="tg-remote" title=${tr('The boss\'s company, as your role allows')}><${Icon} name="wifi_tethering" size=${14} />${tr('Multiplayer')}</span>`}
        <${Status} n=${fit.n} />`
      : html`<span class="lone"><${Icon} name="garage" size=${26} class="t-accent2" /><span><b>${tr('Bus company')}</b><span>${tr('Your own transport company: buses, people and lines, in a time of its own.')}</span></span></span>`}
    <span class="tools">
      ${open && remote && html`<${RemoteClock} /><span class="gap16"></span>
        ${mayChange && html`<${Btn} class="sq" icon="settings" title=${tr("The company's settings: its name, its date, how it buys")} onClick=${settings} /><span class="gap8"></span>`}`}
      ${open && !remote && html`${sh.casual ? html`<${CasualClock} />` : html`<${Clock} now=${fit.now} />`}<span class="gap16"></span>
        <${OnlineButton} company=${true} />
        <${TogetherButton} />
        <${Btn} class="sq" icon="settings" title=${tr("The company's settings: its name, its date, how it buys")} onClick=${settings} /><span class="gap8"></span>`}
      <${Btn} class="sq" icon=${isDark ? 'dark_mode' : 'light_mode'} title=${tr(isDark ? 'Switch to the light mode' : 'Switch to the dark mode')} onClick=${(e) => flipLook(e)} /><span class="gap8"></span>
      ${read && !choosing && !c.remote && html`<${Btn} class="sq co-help" icon="help" title=${tr('A tour of the bus company')} onClick=${askTour} /><span class="gap12"></span>`}
      <${Btn} class="co-back" icon="chevron_left" label=${tr('Back')} title=${tr('Back')} style="width:112px" onClick=${() => leaveCompany()} />
    </span>
  </header>`;
}

/** What the company has in the top bar: its cash and its reputation, each a click from the page
 * about it (`company::status`); the trophies are told under the cash's tooltip (and on the
 * Career, Research and Contracts pages). */
function Status() {
  const sh = S.co.shell;
  const trophies = sh.status.find((it) => it.icon === 'emoji_events');
  const shown = sh.status.filter((it) => it !== trophies);
  const tip = (it) => (it.icon === 'payments' && trophies ? `${it.tip}\n\n${tr('Trophies')}: ${trophies.value}\n${trophies.tip}` : it.tip);
  if (!shown.length) return null;
  return html`<span class="co-status">
    ${shown.map((it) => html`<button type="button" class=${it.tab != null ? 'go' : ''} title=${tip(it)} onClick=${() => it.tab != null && goTab(it.tab)}>
      <span class=${'dot ' + it.tone}><${Icon} name=${it.icon} size=${15} /></span><span class=${'t-' + it.text}>${it.value}</span>
    </button>`)}
  </span>`;
}

/** A menu that drops from a button of the top bar: open while `open`, closed by a click
 * outside it or Escape. */
function useMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef();
  useEffect(() => {
    if (!open) return undefined;
    const away = (e) => ref.current && !ref.current.contains(e.target) && setOpen(false);
    const key = (e) => e.key === 'Escape' && (e.stopPropagation(), setOpen(false));
    addEventListener('mousedown', away, true);
    addEventListener('keydown', key, true);
    return () => {
      removeEventListener('mousedown', away, true);
      removeEventListener('keydown', key, true);
    };
  }, [open]);
  return [open, setOpen, ref];
}

/** The company's clock in the top bar (`clock::head`): the day and the time - with the real
 * clock its live dot, and how far it was fast-forwarded as a small hint - and one split button:
 * Simulate to tomorrow, its menu the play speeds, +1 h and the time dialog. */
function Clock() {
  const sh = S.co.shell;
  const k = sh.clock;
  const [open, setOpen, ref] = useMenu();
  const play = (speed) => {
    const on = Math.abs(k.speed - speed) < 1e-6;
    act('play', { speed: on ? 0 : speed });
  };
  const playTip = (speed, i) => {
    const on = Math.abs(k.speed - speed) < 1e-6;
    if (on) return tr(k.real ? "Back to the real clock's pace" : 'Stop the clock');
    return tr(i === 1 ? "Run the company's clock at ten minutes a second while this page is open" : "Run the company's clock at a minute a second while this page is open");
  };
  const pick = (f) => () => (setOpen(false), f());
  return html`<span class="co-clock">
    <span class="co-top-now" title=${k.tip || undefined}>
      <span class="co-top-day"><span class="date">${k.date}</span><span class=${'time' + (k.speed > 0 ? ' playing' : '')}>${k.time}</span></span>
      <span class=${'co-top-hint' + (k.in_game ? ' game' : '')}>${k.live && html`<i class="live"></i>`}${k.lead || k.label}</span>
    </span>
    <span class="co-split" ref=${ref}>
      <${Btn} kind="primary" class="co-tomorrow" icon="nights_stay" label=${tr('Simulate to tomorrow')} title=${tr(k.in_game ? "The game leads the company's clock while it runs" : 'Simulate the rest of the day: the tours run, what happens is told, and the day is closed at midnight')} onClick=${() => act('tomorrow')} />
      <button type="button" class=${'co-split-more' + (open ? ' on' : '')} title=${tr('More steps and the clock')} onClick=${() => setOpen(!open)}><${Icon} name="expand_more" size=${18} /></button>
      ${open && html`<div class="co-top-menu co-split-menu">
        ${[['×60', 1], ['×600', 10]].map(([label, speed], i) => html`<button type="button" class=${Math.abs(k.speed - speed) < 1e-6 ? 'on' : ''} title=${playTip(speed, i)} onClick=${pick(() => play(speed))}><${Icon} name="play_arrow" size=${17} /><span class="grow">${label}</span></button>`)}
        <button type="button" title=${tr("Simulate one hour of the company's time")} onClick=${pick(() => act('hour'))}><${Icon} name="schedule" size=${17} /><span class="grow">${tr('+1 h')}</span></button>
        <i class="co-top-sep"></i>
        <button type="button" onClick=${pick(() => !sh.cover && setCo({ dialog: { kind: 'time' } }))}><${Icon} name="tune" size=${17} /><span class="grow">${tr('More steps and the clock')}</span></button>
      </div>`}
    </span>
  </span>`;
}

/** The boss's company's clock as a friend sees it: its day and time, nothing to run. */
function RemoteClock() {
  const k = S.co.shell.clock;
  return html`<span class="co-clock">
    <span class="co-top-now"><span class="co-top-day"><span class="date">${k.date}</span><span class="time">${k.time}</span></span><span class="co-top-hint">${k.label}</span></span>
  </span>`;
}

/** Casual's clock (`casual::head`): the day and the time, and "Next day". */
function CasualClock() {
  const k = S.co.shell.clock;
  return html`<span class="co-clock">
    <span class="co-top-now"><span class="co-top-day"><span class="date">${k.date}</span><span class="time">${k.time}</span></span></span>
    <${Btn} kind="primary" class="co-next" icon="wb_sunny" label=${tr('Next day')} title=${tr('Run the rest of the day: the buses drive, the money comes in, and you start the next morning')} onClick=${() => act('next_day')} />
  </span>`;
}

// --- the top bar ----------------------------------------------------------------------------

/** The slim bar over the pages: the page's title (Casual: its crumbs), the cash and the
 * reputation, the clock with its split button, the bell, the player's menu and Back. */
function Top({ mayChange }) {
  const c = S.co;
  const sh = c.shell;
  const remote = !!c.remote;
  return html`<header class="co-bar co-top">
    ${(c.nav || 'full') === 'gone' && html`<button type="button" class="co-top-side" title=${tr('Show the sidebar (Ctrl+B)')} onClick=${toggleSide}><${Icon} name="menu" size=${20} /></button>`}
    <${Where} />
    <button type="button" class="co-top-search" title=${tr('Search pages and actions (Ctrl+K)')} onClick=${() => setCo({ cmdk: true })}><${Icon} name="search" size=${18} /><span class="co-top-kbd">Ctrl K</span></button>
    <span class="co-top-fill"></span>
    <${Status} />
    ${remote ? html`<${RemoteClock} />` : sh.casual ? html`<${CasualClock} />` : html`<${Clock} />`}
    ${!remote && !sh.casual && html`<${Bell} />`}
    <${Me} mayChange=${mayChange} />
    <${Btn} class="co-back" icon="chevron_left" label=${tr('Back')} title=${tr('Back')} onClick=${() => leaveCompany()} />
  </header>`;
}

/** Where one is: the page's icon and name (the start screen: the company's home), in Casual the
 * page above it a click away. */
function Where() {
  const c = S.co;
  const sh = c.shell;
  const tab = c.tab;
  if (sh.casual) {
    const pages = sh.casual_pages?.pages || {};
    if (tab === HOME) return html`<span class="co-top-where"><span class="co-top-title cut">${sh.casual_pages?.home}</span></span>`;
    const crumbs = [parentOf(sh, tab), tab].filter((t) => t !== HOME);
    return html`<span class="co-top-where">${crumbs.map((t, k) => html`${k > 0 && html`<${Icon} name="chevron_right" size=${16} class="t-dim" />`}<button type="button" class=${'co-top-crumb cut' + (t === tab ? ' last' : '')} onClick=${() => t !== tab && setCo({ tab: t })}>${pages[t]?.name}</button>`)}</span>`;
  }
  if (tab === HOME) return html`<span class="co-top-where"><span class="co-top-title cut">${sh.home}</span></span>`;
  const it = sh.rail.flat().find((x) => x.tab === tab);
  return html`<span class="co-top-where"><${Icon} name=${it?.icon || 'apps'} size=${20} class="t-accent2" /><span class="co-top-title cut">${nameOf(it) || sh.names?.[tab] || ''}</span></span>`;
}

/** The bell: the unread messages and what of the coming week needs the player (`co_view`
 * "bell"), its badge how many; each a click from its page. */
function Bell() {
  const sh = S.co.shell;
  const [open, setOpen, ref] = useMenu();
  const v = useView('bell', {}, open);
  const n = sh.bell || 0;
  const go = (f) => () => (setOpen(false), f());
  return html`<span class="co-top-bell" ref=${ref}>
    <button type="button" class=${'co-top-round' + (open ? ' on' : '')} title=${tr('Notifications')} onClick=${() => setOpen(!open)}>
      <${Icon} name="notifications" size=${20} />${n > 0 && html`<span class="co-top-badge">${n > 9 ? '9+' : n}</span>`}
    </button>
    ${open && html`<div class="co-top-menu co-bell-menu">
      ${!v ? html`<div class="co-bell-none">…</div>` : v.error ? html`<div class="co-bell-none">${v.error}</div>` : html`
        <div class="co-bell-head"><b>${v.inbox.title}</b><button type="button" onClick=${go(() => goTab(13))}>${v.all}</button></div>
        ${v.inbox.list.length ? v.inbox.list.map((m) => html`<button type="button" class="co-bell-row" title=${m.title} onClick=${go(() => (setUi('inbox', { open: m.id }), goTab(13)))}>
          <${Icon} name=${m.icon} size=${17} class=${'t-' + m.tone} /><span class="cut grow">${m.title}</span><span class="co-bell-when">${m.date}</span></button>`) : html`<div class="co-bell-none">${v.inbox.none}</div>`}
        <div class="co-bell-head"><b>${v.agenda.title}</b><button type="button" onClick=${go(() => goTab(14))}>${v.all}</button></div>
        ${v.agenda.list.length ? v.agenda.list.map((e) => html`<button type="button" class="co-bell-row" title=${e.note ? `${e.text}\n${e.note}` : e.text} onClick=${go(() => follow(e.go))}>
          <${Icon} name=${e.icon} size=${17} class=${'t-' + e.tone} /><span class="cut grow">${e.text}</span><span class="co-bell-when">${e.when}</span></button>`) : html`<div class="co-bell-none">${v.agenda.none}</div>`}`}
    </div>`}
  </span>`;
}

/** The player's round menu: the online account, Multiplayer, the company's settings, the
 * light or dark look and the company's tour. (The account keeps its own button, the online
 * panel's: `online.js`.) */
function Me({ mayChange }) {
  const c = S.co;
  const sh = c.shell;
  const remote = !!c.remote;
  const [open, setOpen, ref] = useMenu();
  const isDark = dark();
  const who = String(S.config?.profile || sh.name || '?').trim();
  const go = (f) => (e) => (setOpen(false), f(e));
  const together = () => (sh.pages?.includes(TOGETHER_TAB) ? goTab(TOGETHER_TAB) : setCo({ dialog: { kind: 'together' } }));
  // (the account's row clicks the online panel's own button in it)
  const account = (e) => {
    if (!e.target.closest('.ol-btn')) e.currentTarget.querySelector('.ol-btn button')?.click();
    setOpen(false);
  };
  return html`<span class="co-top-me" ref=${ref}>
    <button type="button" class=${'co-top-round co-top-avatar' + (open ? ' on' : '')} title=${who} onClick=${() => setOpen(!open)}>${(who[0] || '?').toUpperCase()}</button>
    ${open && html`<div class="co-top-menu co-me-menu">
      <div class="co-me-who cut">${who}</div>
      ${!remote && html`<div class="co-me-account" role="button" onClick=${account}><span class="grow">${tr('Account')}</span><${OnlineButton} company=${true} /></div>`}
      ${!remote && html`<button type="button" disabled=${!togetherOn()} title=${togetherOn() ? tr('Multiplayer') : blockedLabel()} onClick=${go(together)}><${Icon} name="wifi_tethering" size=${17} /><span class="grow">${tr('Multiplayer')}</span></button>`}
      ${mayChange && html`<button type="button" onClick=${go(() => setCo({ dialog: { kind: 'settings' } }))}><${Icon} name="settings" size=${17} /><span class="grow">${tr('Company settings')}</span></button>`}
      <button type="button" onClick=${go((e) => flipLook(e))}><${Icon} name=${isDark ? 'light_mode' : 'dark_mode'} size=${17} /><span class="grow">${tr(isDark ? 'Light mode' : 'Dark mode')}</span></button>
      ${!remote && html`<button type="button" class="co-help" title=${tr('A tour of the bus company')} onClick=${go(askTour)}><${Icon} name="help" size=${17} /><span class="grow">${tr('Help')}</span></button>`}
    </div>`}
  </span>`;
}

// --- the sidebar ----------------------------------------------------------------------------

/** How the sidebar is shown: "full" (icons and names), "rail" (icons only) or "gone" (slid
 * away), remembered in the launcher; on a narrow stage - a small window, a large Launcher size
 * - it starts as its icons. */
const NAV_KEY = 'co-nav';
const NAV_BACK = 'co-nav-back';
const NAVS = ['full', 'rail', 'gone'];
const stageW = () => document.querySelector('.hub-stage')?.clientWidth || innerWidth;
function navStart() {
  let m = remembered(NAV_KEY, 'full');
  if (!NAVS.includes(m)) m = 'full';
  if (m === 'full' && stageW() < 1200) m = 'rail';
  return m;
}
export function setNav(mode) {
  remember(NAV_KEY, mode);
  if (mode !== 'gone') remember(NAV_BACK, mode);
  setCo({ nav: mode });
}
/** Slide the sidebar away, or bring it back as it was (Ctrl+B). */
export function toggleSide() {
  if ((S.co.nav || 'full') !== 'gone') return setNav('gone');
  const back = remembered(NAV_BACK, 'full');
  setNav(back === 'rail' ? 'rail' : 'full');
}

/** The pages pinned on top of the sidebar, per company (the launcher's own store). */
const pinKey = (id) => 'co-pins:' + (id || '');
function pinsOf(id) {
  try {
    const v = JSON.parse(remembered(pinKey(id), '[]'));
    return Array.isArray(v) ? v.filter((t) => Number.isInteger(t)) : [];
  } catch {
    return [];
  }
}
function flipPin(id, tab) {
  const p = pinsOf(id);
  remember(pinKey(id), JSON.stringify(p.includes(tab) ? p.filter((t) => t !== tab) : [...p, tab]));
  setCo((c) => ({ pinned: (c.pinned || 0) + 1 }));
}

/** A page's name as the sidebar says it (the overview is the dashboard). */
const nameOf = (it) => (it && it.tab === 0 && !S.co.shell?.casual ? tr('Dashboard') : it?.name);

/** The company's sidebar: its mark, name and game mode on top, the pages pinned, then every
 * page grouped as the start screen groups them, each with its icon, name and badge; at its foot
 * the way to fold it to its icons or slide it away. */
function Side({ mayChange }) {
  const c = S.co;
  const sh = c.shell;
  const nav = c.nav || 'full';
  const rail = nav === 'rail';
  const items = sh.rail.flat();
  const casual = !!sh.casual;
  const remote = !!c.remote;
  const pinnable = !casual && !remote;
  const pins = pinnable ? pinsOf(sh.id) : [];
  const on = (t) => t === c.tab || (casual && t !== HOME && parentOf(sh, c.tab) === t);
  const groups = [];
  for (const it of items) {
    const last = groups[groups.length - 1];
    const head = it.group ?? null;
    if (last && last.head === head) last.items.push(it);
    else groups.push({ head, items: [it] });
  }
  // (the company's settings with the company's own pages)
  if (mayChange) {
    const settings = { tab: 'settings', icon: 'settings', name: tr('Settings'), tip: tr("The company's settings: its name, its date, how it buys") };
    const company = groups.find((g) => g.head === tr('Company'));
    if (company) company.items.push(settings);
    else groups.push({ head: tr('Company'), items: [settings] });
  }
  const homeOk = !remote || (sh.remote?.perms || []).includes('finances');
  const start = homeOk && { tab: HOME, icon: casual ? 'home' : 'apps', name: tr(casual ? 'Home' : 'Start screen'), tip: tr(casual ? "Back to your company's home (Escape)" : 'Every part of the company as a tile (Escape)') };
  const open = (it) => (it.tab === 'settings' ? setCo({ dialog: { kind: 'settings' } }) : it.tab === HOME ? setCo({ tab: HOME }) : goTab(it.tab));
  const Item = ({ it, pin }) => {
    const pinned = pins.includes(it.tab);
    return html`<div class=${'co-nav-item' + (on(it.tab) ? ' on' : '')}>
      <button type="button" class="co-nav-go" title=${rail ? `${nameOf(it)}${it.badge ? ` (${it.badge.n})` : ''}` : it.tip} onClick=${() => open(it)}>
        <${Icon} name=${it.icon} size=${19} />
        <span class="co-nav-name cut">${nameOf(it)}</span>
        ${it.badge && html`<span class=${'co-nav-badge ' + it.badge.tone}>${it.badge.n}</span>`}
      </button>
      ${pinnable && !rail && Number.isInteger(it.tab) && it.tab !== HOME && html`<button type="button" class=${'co-nav-star' + (pinned ? ' on' : '')} title=${pinned ? tr('Unpin') : tr('Pin to the top')} onClick=${() => flipPin(sh.id, it.tab)}><${Icon} name="star" size=${14} /></button>`}
    </div>`;
  };
  const pinnedItems = pins.map((t) => items.find((x) => x.tab === t)).filter(Boolean);
  const chip = [sh.sandbox?.label, sh.mode?.label].filter(Boolean).join(' · ');
  return html`<nav class="co-nav-side" aria-hidden=${nav === 'gone' ? 'true' : undefined}>
    <div class="co-nav-head">
      <button type="button" class="co-nav-brand" title=${sh.home} onClick=${() => homeOk && setCo({ tab: HOME })}><${Mark} mark=${sh.mark} size=${rail ? 38 : 40} /></button>
      ${!rail && html`<div class="co-nav-who">
        <b class="cut" title=${sh.home}>${sh.name}</b>
        ${remote
          ? html`<span class="co-nav-chip tg-remote" title=${tr("The boss's company, as your role allows")}><${Icon} name="wifi_tethering" size=${12} />${tr('Multiplayer')}</span>`
          : html`<button type="button" class=${'co-nav-chip co-pill' + (sh.mode?.casual ? ' casual' : '')} title=${tr("The company's game mode: change it in the company's settings")} onClick=${() => mayChange && setCo({ dialog: { kind: 'settings' } })}><${Icon} name=${sh.sandbox ? sh.sandbox.icon : sh.mode?.icon} size=${12} /><span class="cut">${chip}</span></button>`}
      </div>`}
    </div>
    <div class="co-nav-list">
      ${start && html`<${Item} it=${start} />`}
      ${pinnedItems.length > 0 && html`<div class="co-nav-group"><div class="co-nav-heading">${tr('Pinned')}</div>${pinnedItems.map((it) => html`<${Item} it=${it} pin />`)}</div>`}
      ${groups.map((g) => html`<div class="co-nav-group">${g.head && html`<div class="co-nav-heading">${g.head}</div>`}${g.items.map((it) => html`<${Item} it=${it} />`)}</div>`)}
    </div>
    <div class="co-nav-foot">
      <button type="button" class="co-nav-fold" title=${rail ? tr('Expand the sidebar') : tr('Collapse the sidebar')} onClick=${() => setNav(rail ? 'full' : 'rail')}><${Icon} name=${rail ? 'chevron_right' : 'chevron_left'} size=${18} />${!rail && html`<span class="cut">${tr('Collapse the sidebar')}</span>`}</button>
      ${!rail && html`<button type="button" class="co-nav-hide" title=${tr('Hide the sidebar (Ctrl+B)')} onClick=${() => setNav('gone')}><${Icon} name="close" size=${16} /></button>`}
    </div>
  </nav>`;
}

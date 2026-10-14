// The welcome (Trello "Tauri 3/6": what phase 1 left), as our game-drawn launcher's
// (`crates/omsi-launcher-ui/src/welcome.rs`): what a player sees the first time the launcher
// opens, in place of the start. In Luc's order: the language - every one openOMSI speaks, as
// tiles with their flags, spoken at once; a dialog over it asking for the new interface or the
// classic one; the animations, on or off, with a little scene that shows what "on" means; the
// OMSI 2 folder when none was found; the driver - a new one, or one already on this computer,
// with their personnel pass; and the way on - the tour, straight to the start, or the game's
// driving lessons.
//
// It lies under a bar of its own steps on the start's ground, as one sheet in the middle of the
// window; going on, the next step slides in from the side it lies on, and the dialog comes up
// out of a veil - none of it with the setting "animations" off. `welcome_done` is written when
// it is finished or skipped. It opens when that is not set, or where `OMSI_LAUNCHER_WELCOME`
// says (`hub_more_welcome`).

import { html, useEffect, useRef, useState } from '../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../store.js';
import { call } from '../api.js';
import { saveSettings, loadContent, loadProfiles } from '../data.js';
import { Icon, tr } from './bits.js';
import { LOGO_SVG } from './logo.js';
import { Flag } from './bar.js';

let cssLoaded = false;
function loadCss() {
  if (cssLoaded) return;
  cssLoaded = true;
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/welcome.css';
  document.head.append(sheet);
}

/** The steps: their word and icon in the bar (`Step::label`). */
const LABELS = {
  language: ['Language', 'language'],
  interface: ['Interface', 'grid_view'],
  animations: ['Animations', 'bolt'],
  folder: ['Game folder', 'folder_open'],
  driver: ['Your driver', 'person'],
  ready: ['Ready', 'sports_score'],
};

/** The steps in their order: the folder's only when it has to be pointed to. */
export const stepsOf = (folder) => ['language', 'interface', 'animations', ...(folder ? ['folder'] : []), 'driver', 'ready'];

/** What the sheet shows on a step: the interface's dialog lies over the language's sheet. */
const sheetStep = (s) => (s === 'interface' ? 'language' : s);

const W = () => S.hubWelcome;
const put = (patch) => S.hubWelcome && set({ hubWelcome: { ...S.hubWelcome, ...patch } });

/** Whether the welcome opens as the launcher starts, and where (`welcome::due`). */
export async function welcomeAtStart() {
  try {
    const d = await call('hub_more_welcome');
    if (d?.step) open(d.step, d.folder || d.step === 'folder');
    // (`OMSI_LAUNCHER_TOUR=n`: the tour at its n-th stop, for pictures of it, once the content is read)
    else if (d?.tour) setTimeout(() => import('./tour.js').then((m) => m.startTour(d.tour - 1)), 2500);
  } catch (e) {
    // (no welcome is better than no launcher)
  }
}

function open(step, folder) {
  loadCss();
  set({ hubWelcome: { open: true, step, folder, side: 0, seq: 0, typing: false, made: null, root: null, focus: step === 'driver' } });
}

/** Show the welcome again from its first step (the settings): `welcome_done` is false until it
 * is finished again. */
export async function showWelcomeAgain() {
  await saveSettings({ welcome_done: false });
  const d = await call('hub_more_welcome').catch(() => null);
  open('language', !!d?.folder);
}

/** To another step: the sheet's slides in from the side it lies on (`Welcome::go_to`). */
function goTo(to) {
  const w = W();
  if (!w || to === w.step) return;
  const steps = stepsOf(w.folder);
  const forward = steps.indexOf(to) > steps.indexOf(w.step);
  const patch = { step: to, focus: to === 'driver' };
  if (sheetStep(to) !== sheetStep(w.step)) Object.assign(patch, { side: forward ? 1 : -1, seq: w.seq + 1 });
  put(patch);
}
const next = () => {
  const steps = stepsOf(W().folder);
  const n = steps[steps.indexOf(W().step) + 1];
  if (n) goTo(n);
};
const back = () => {
  const steps = stepsOf(W().folder);
  const k = steps.indexOf(W().step);
  if (k > 0) goTo(steps[k - 1]);
};

const uiOf = () => String(S.settings?.launcher_ui || 'new').trim().toLowerCase();
/** Whether the classic launcher was chosen. */
const classic = () => uiOf() === 'classic';

/** The welcome is over (finished, or skipped): `welcome_done` is written and what comes after
 * begins (`welcome::apply`): the classic launcher opens in this one's place; without an OMSI 2
 * to drive, Setup says what is missing; else the start (with the tour), or the lessons. */
async function finish(then) {
  const w = W();
  if (!w || w.leaving) return;
  put({ leaving: true });
  const still = S.settings?.animations === false;
  await new Promise((r) => setTimeout(r, still ? 0 : 160));
  await saveSettings({ welcome_done: true });
  set({ hubWelcome: null });
  if (classic()) {
    call('classic_launcher', { page: null }).catch((e) => toast(e.message, 'error'));
    return;
  }
  const { go } = await import('./hub.js');
  if (!S.boot?.root && !S.config?.root) return go('setup');
  if (then === 'lessons') return go('tutorials');
  go('drive', 'mode');
  if (then === 'tour') import('./tour.js').then((m) => m.startTour?.()).catch(() => {});
}

// --- the bar ---------------------------------------------------------------------------------

/** The bar along the top: the brand, the welcome's steps as a route (done ones a way back),
 * and "Skip" on the right (`welcome::bar`). */
function Bar() {
  const w = useStore().hubWelcome;
  const steps = stepsOf(w.folder);
  const at = steps.indexOf(w.step);
  return html`<header class="hub-bar hub-glass hub-wbar">
    <span class="hub-brand" dangerouslySetInnerHTML=${{ __html: LOGO_SVG }}></span>
    <nav class="hub-steps">
      ${steps.map((id, k) => {
        const [label, icon] = LABELS[id];
        const done = k < at;
        return html`${k > 0 && html`<i class=${'hub-step-join' + (k <= at ? ' done' : '')}></i>`}
          <button type="button" class=${'hub-step' + (done ? ' done open' : '') + (k === at ? ' now' : '')} onClick=${() => done && goTo(id)}>
            <${Icon} name=${icon} size=${15} /><span>${tr(label)}</span></button>`;
      })}
    </nav>
    <span class="hub-bar-fill"></span>
    <button type="button" class="hub-wskip" onClick=${() => finish('start')}>${tr('Skip')}</button>
  </header>`;
}

// --- the sheet -------------------------------------------------------------------------------

/** A step's title and the line under it (`welcome::heading`). */
function heading(step) {
  switch (step) {
    case 'animations':
      return [tr('Animations on or off?'), tr('Screens that slide in, tiles that rise under the mouse, a bus between the menus. Some like it lively, others calm.')];
    case 'folder':
      return [tr('Where is OMSI 2?'), tr('openOMSI drives on the maps and buses of the original OMSI 2. Point once to the folder you play from.')];
    case 'driver':
      return [tr('Who is driving?'), tr('Your duties, kilometres and level are kept per driver. Everything stays on this computer: there is nothing to sign in to.')];
    case 'ready': {
      const name = chosenDriver();
      return [name ? tr('Ready to go, %{name}', { name }) : tr('Ready to go'), tr(classic() ? 'That is everything. The classic launcher opens as soon as the launcher has started again.' : 'That is everything. Would you like a short tour of the launcher first?')];
    }
    default:
      return [tr('Welcome to openOMSI'), tr('Choose the language of the launcher and the game. It applies at once, and you can change it any time in the settings.')];
  }
}

/** "STEP 2 OF 5". */
const eyebrow = (step) => {
  const steps = stepsOf(W().folder);
  return tr('Step %{n} of %{total}', { n: steps.indexOf(step) + 1, total: steps.length }).toUpperCase();
};

function Sheet() {
  const s = useStore();
  const w = s.hubWelcome;
  const step = sheetStep(w.step);
  const [title, sub] = heading(step);
  const Body = { language: Languages, animations: Animations, folder: Folder, driver: Driver, ready: Ready }[step];
  const move = w.side === 0 ? 'rise' : w.side > 0 ? 'right' : 'left';
  return html`<section class="hub-wsheet">
    <div class=${'hub-wmove ' + move} key=${w.seq}>
      <div class="hub-whead">
        <div class="eyebrow">${eyebrow(step)}</div>
        <h1>${title}</h1>
        <p>${sub}</p>
        <span class="hub-wlogo" dangerouslySetInnerHTML=${{ __html: LOGO_SVG }}></span>
      </div>
      <div class="hub-wbody"><${Body} /></div>
    </div>
    <${Foot} step=${step} />
  </section>`;
}

/** A button of the foot: the big ones of the welcome (46 high). */
const Btn = ({ kind = '', icon, label, onClick, raw }) =>
  html`<button type="button" class=${'hub-wbtn ' + kind} onClick=${onClick}>${icon && html`<${Icon} name=${icon} size=${19} />`}${label && (raw ? label : tr(label))}</button>`;

/** The sheet's foot: Back on the left (not on the first step), the way on on the right - its
 * word and what it does are the step's (`welcome::foot`). */
function Foot({ step }) {
  const w = useStore().hubWelcome;
  const steps = stepsOf(w.folder);
  let way = null;
  if (step === 'language' || step === 'animations') way = html`<${Btn} kind="primary" icon="chevron_right" label="Next" onClick=${next} />`;
  else if (step === 'folder') way = html`<${FolderWay} />`;
  else if (step === 'driver') way = html`<${Btn} kind="primary" icon="chevron_right" label="Next" onClick=${driverNext} />`;
  else if (step === 'ready' && classic()) way = html`<${Btn} kind="primary" icon="restart_alt" label="Restart in the classic interface" onClick=${() => finish('start')} />`;
  else if (step === 'ready') way = html`<${Btn} kind="ghost" icon="help" label="New to OMSI? Driving lessons" onClick=${() => finish('lessons')} />`;
  return html`<footer class="hub-wfoot">
    ${steps.indexOf(step) > 0 && html`<${Btn} kind="back" icon="chevron_left" label="Back" onClick=${back} />`}
    <span class="grow"></span>${way}
  </footer>`;
}

// --- the language ----------------------------------------------------------------------------

/** A language's flag (no flag emoji on Windows): the bar's four as the bar has them, the others
 * as simple as they still read small (`welcome::paint_flag`); a language spoken under more than
 * one flag, or whose flag is disputed, gets its script instead (`script_badge`). */
function LangFlag({ code }) {
  if (['ENG', 'DEU', 'FRA', 'NLD'].includes(code)) return html`<span class="hub-wflag"><${Flag} code=${code} /></span>`;
  const rows = (parts) => {
    const total = parts.reduce((a, p) => a + p[0], 0);
    let y = 0;
    return parts.map(([share, c]) => {
      const h = (20 * share) / total;
      const r = html`<rect x="0" y=${y} width="30" height=${h + 0.05} fill=${c} />`;
      y += h;
      return r;
    });
  };
  const cols = (parts) => {
    const total = parts.reduce((a, p) => a + p[0], 0);
    let x = 0;
    return parts.map(([share, c]) => {
      const w = (30 * share) / total;
      const r = html`<rect x=${x} y="0" width=${w + 0.05} height="20" fill=${c} />`;
      x += w;
      return r;
    });
  };
  const star = (cx, cy, outer, fill, turn = 0) => {
    const pts = [];
    for (let i = 0; i < 10; i++) {
      const a = turn - Math.PI / 2 + (i * Math.PI) / 5;
      const r = i % 2 === 0 ? outer : outer * 0.382;
      pts.push(`${cx + Math.cos(a) * r},${cy + Math.sin(a) * r}`);
    }
    return html`<polygon points=${pts.join(' ')} fill=${fill} />`;
  };
  const W_ = '#ffffff';
  let body;
  switch (code) {
    case 'RUS': body = rows([[1, W_], [1, '#0039a6'], [1, '#d52b1e']]); break;
    case 'UKR': body = rows([[1, '#0057b7'], [1, '#ffd700']]); break;
    case 'POL': body = rows([[1, W_], [1, '#dc143c']]); break;
    case 'CAT': body = rows(Array.from({ length: 9 }, (_, k) => [1, k % 2 ? '#da121a' : '#fcdd09'])); break;
    case 'CZE': body = html`${rows([[1, W_], [1, '#d7141a']])}<polygon points="0,0 15,10 0,20" fill="#11457e" />`; break;
    case 'HUN': body = rows([[1, '#cd2a3e'], [1, W_], [1, '#436f4d']]); break;
    case 'ESP': body = rows([[1, '#aa151b'], [2, '#f1bf00'], [1, '#aa151b']]); break;
    case 'ITA': body = cols([[1, '#009246'], [1, W_], [1, '#ce2b37']]); break;
    case 'PTP': body = html`${cols([[2, '#046a38'], [3, '#da291c']])}<circle cx="12" cy="10" r="4.4" fill="#ffcc00" /><circle cx="12" cy="10" r="2.4" fill=${W_} />`; break;
    case 'PTB': body = html`<rect width="30" height="20" fill="#009c3b" /><polygon points="2.7,10 15,2.4 27.3,10 15,17.6" fill="#ffdf00" /><circle cx="15" cy="10" r="4.6" fill="#002776" />`; break;
    case 'TUR': body = html`<rect width="30" height="20" fill="#e30a17" /><circle cx="11.1" cy="10" r="5" fill=${W_} /><circle cx="12.3" cy="10" r="4" fill="#e30a17" />${star(18, 10, 2.6, W_, -Math.PI / 2)}`; break;
    case 'JPN': body = html`<rect width="30" height="20" fill=${W_} /><circle cx="15" cy="10" r="6" fill="#bc002d" />`; break;
    case 'KOR':
      body = html`<rect width="30" height="20" fill=${W_} /><circle cx="15" cy="10" r="5" fill="#0047a0" /><path d="M10 10 A5 5 0 0 1 20 10 Z" fill="#cd2e3a" />
        ${[[-1, -1], [1, -1], [-1, 1], [1, 1]].map(([dx, dy]) => html`<rect x=${15 + dx * 9.9 - 2} y=${10 + dy * 6 - 1.5} width="4" height="3" fill="#141414" />`)}`;
      break;
    case 'THA': body = rows([[1, '#a51931'], [1, W_], [2, '#2d2a4a'], [1, W_], [1, '#a51931']]); break;
    case 'VIE': body = html`<rect width="30" height="20" fill="#da251d" />${star(15, 10, 6.4, '#ffff00')}`; break;
    case 'IND': body = rows([[1, '#ce1126'], [1, W_]]); break;
    case 'MSA':
      body = html`${rows(Array.from({ length: 14 }, (_, k) => [1, k % 2 ? W_ : '#cc0001']))}<rect width="15" height=${(20 * 8) / 14} fill="#010066" />
        <circle cx="5.7" cy=${(20 * 4) / 14} r=${((20 * 8) / 14) * 0.36} fill="#ffcc00" /><circle cx=${5.7 + ((20 * 8) / 14) * 0.1} cy=${(20 * 4) / 14} r=${((20 * 8) / 14) * 0.3} fill="#010066" />${star(11.1, (20 * 4) / 14, ((20 * 8) / 14) * 0.22, '#ffcc00')}`;
      break;
    case 'TGL': body = html`${rows([[1, '#0038a8'], [1, '#ce1126']])}<polygon points="0,0 17.4,10 0,20" fill=${W_} /><circle cx="6" cy="10" r="2.2" fill="#fcd116" />`; break;
    case 'KAZ': body = html`<rect width="30" height="20" fill="#00afca" /><circle cx="15" cy="10" r="5.5" fill="none" stroke="#fec504" stroke-width="1.4" /><circle cx="15" cy="10" r="3.8" fill="#fec504" />`; break;
    case 'HIN': body = html`${rows([[1, '#ff9933'], [1, W_], [1, '#138808']])}<circle cx="15" cy="10" r="2.4" fill="none" stroke="#000080" stroke-width="0.8" /><circle cx="15" cy="10" r="0.7" fill="#000080" />`; break;
    default: {
      const mark = { ZHT: '繁', CHS: '简', BEL: 'Бел' }[code] || code;
      return html`<span class="hub-wflag script">${mark}</span>`;
    }
  }
  return html`<span class="hub-wflag"><svg viewBox="0 0 30 20" preserveAspectRatio="none">${body}</svg></span>`;
}

/** Every language openOMSI speaks as a tile with its flag and its own name; a click speaks it
 * at once (`welcome::languages`). */
function Languages() {
  const s = useStore();
  const cur = s.settings?.language || 'ENG';
  return html`<div class="hub-wlangs">
    ${(s.boot?.languages || []).map((l) => {
      const on = l.code === cur;
      return html`<button type="button" class=${'hub-wtile hub-wlang' + (on ? ' on' : '')} onClick=${() => !on && saveSettings({ language: l.code })}>
        <${LangFlag} code=${l.code} /><b>${l.name}</b>${on && html`<${Icon} name="check" size=${17} />`}</button>`;
    })}
  </div>`;
}

// --- the interface (the dialog) ---------------------------------------------------------------

/** The dialog over the language: the new interface or the classic one, each with a picture of
 * it (`welcome::dialog`). In this window "new" is our look - here, or in the game's window. */
function Dialog() {
  const s = useStore();
  const cur = uiOf() === 'classic' ? 'classic' : 'new';
  const pick = (v) => {
    if (v === cur) return;
    // (the desktop window is our look too: choosing it again keeps the window the player has)
    saveSettings({ launcher_ui: v === 'new' && s.hubWelcomeUi === 'desktop' ? 'desktop' : v });
  };
  useEffect(() => {
    if (s.hubWelcomeUi == null) set({ hubWelcomeUi: uiOf() });
    const key = (e) => (e.key === 'Escape' ? back() : e.key === 'Enter' ? next() : null);
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, []);
  const cards = [
    ['new', 'New interface', 'ui-new', "Omsi-Hub's look: you set up your duty step by step on the map, with your bus in 3D."],
    ['classic', 'Classic interface', 'ui-classic', "openOMSI's own launcher: every page in one window, with the menu on the left."],
  ];
  return html`<div class="hub-wveil"></div>
    <section class="hub-wdialog">
      <div class="hub-whead">
        <div class="eyebrow">${eyebrow('interface')}</div>
        <h1>${tr('Which interface do you want to use?')}</h1>
        <p>${tr('Both start the same game, and you can switch later.')}</p>
      </div>
      <div class="hub-wcards">
        ${cards.map(([v, title, pic, line]) => html`<button type="button" class=${'hub-wtile hub-wcard' + (cur === v ? ' on' : '')} onClick=${() => pick(v)}>
          <span class="pic" style=${`background-image:url("omsihub/${pic}.jpg")`}>${cur === v && html`<i><${Icon} name="check" size=${17} /></i>`}</span>
          <span class="t"><span class=${'hub-radio' + (cur === v ? ' on' : '')}></span><b>${tr(title)}</b></span>
          <span class="line">${tr(line)}</span>
        </button>`)}
      </div>
      <footer class="hub-wfoot">
        <${Btn} kind="back" icon="chevron_left" label="Back" onClick=${back} /><span class="grow"></span>
        <${Btn} kind="primary" icon="chevron_right" label="Next" onClick=${next} />
      </footer>
    </section>`;
}

// --- the animations --------------------------------------------------------------------------

const ease = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
const smooth = (t) => {
  const x = Math.min(1, Math.max(0, t));
  return x * x * (3 - 2 * x);
};

/** The bus of the little scene at `t` seconds: how far along its route and how much of it is
 * seen - it pulls away, stops at the middle stop, goes on to the last and fades out there
 * (`welcome::bus_at`). */
function busAt(t) {
  const c = ((t % 4.6) + 4.6) % 4.6;
  if (c < 1.5) return [0.5 * ease(c / 1.5), Math.min(1, c / 0.3)];
  if (c < 2.2) return [0.5, 1];
  if (c < 3.7) return [0.5 + 0.5 * ease((c - 2.2) / 1.5), 1];
  return [1, 1 - smooth((c - 3.7) / 0.6)];
}

/** The scene's three tiles at `t`: the one the mouse is over, where the mouse is between them,
 * how high that tile has risen (`welcome::tiles_at`). */
function tilesAt(t) {
  const SLOT = 1.15;
  const p = (((t % (3 * SLOT)) + 3 * SLOT) % (3 * SLOT)) / SLOT;
  const k = Math.min(2, Math.floor(p));
  const u = p - k;
  const from = k === 0 ? 2 : k - 1;
  const pointer = from + (k - from) * ease(Math.min(1, u / 0.32));
  const lift = smooth((u - 0.22) / 0.18) * (1 - smooth((u - 0.84) / 0.16));
  return [k, pointer, lift];
}

/** A city bus seen from the side, its wheels on `y` at `x` (its middle) (`welcome::mini_bus`). */
function MiniBus({ x, y, s, a }) {
  if (a <= 0.01) return null;
  const w = 50 * s;
  const h = 19 * s;
  const bx = x - w / 2;
  const by = y - h;
  return html`<g opacity=${a}>
    <rect x=${bx + 2} y=${by + h - 2} width=${w - 4} height="4" rx="2" fill="rgba(0,0,0,0.5)" style="filter:blur(2px)" />
    <rect x=${bx} y=${by} width=${w} height=${h} rx=${4.5 * s} fill="rgb(238,241,246)" />
    <rect x=${bx + 4 * s} y=${by + 3 * s} width=${w - 10 * s} height=${6.5 * s} rx=${1.5 * s} fill="rgb(28,40,62)" />
    <rect x=${bx + w - 5 * s} y=${by + 3 * s} width=${3 * s} height=${9.5 * s} rx=${1.2 * s} fill="rgb(28,40,62)" />
    ${[0.33, 0.62].map((dx) => html`<rect x=${bx + w * dx} y=${by + 3 * s} width=${s} height=${h - 7 * s} fill="rgb(150,160,178)" />`)}
    <rect x=${bx + 2 * s} y=${by + h - 5.5 * s} width=${w - 4 * s} height=${1.8 * s} style="fill:var(--hub-accent)" />
    <circle cx=${bx + w - 1.6 * s} cy=${by + h - 6.5 * s} r=${1.3 * s} fill="rgb(255,210,63)" />
    ${[bx + 11 * s, bx + w - 12 * s].map((wx) => html`<circle cx=${wx} cy=${by + h} r=${4.2 * s} fill="rgb(14,18,28)" /><circle cx=${wx} cy=${by + h} r=${1.7 * s} fill="rgb(160,168,184)" />`)}
  </g>`;
}

/** The little scene: three tiles with the mouse over one and a bus on a route with three stops
 * - moving (`playing`), or standing still (`welcome::scene`). */
function Scene({ playing }) {
  const ref = useRef();
  const [w, setW] = useState(420);
  const [t, setT] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // (the stage is scaled as a whole: its own lengths are the layout's)
    const measure = () => setW(el.offsetWidth || 420);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    const t0 = performance.now();
    const tick = (now) => {
      setT((now - t0) / 1000);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing]);
  const h = 140;
  const gap = 8;
  const th = Math.min(h * 0.28, 38);
  const tw = Math.min((w - 36 - 2 * gap) / 3, th * 2.6);
  const tx = (w - 3 * tw - 2 * gap) / 2;
  const ty = 16;
  const [at, pointer, lift] = playing ? tilesAt(t) : [0, 0, 0];
  const y = h * 0.76;
  const [x0, x1] = [26, w - 26];
  const [along, seen] = playing ? busAt(t) : [0, 1];
  const px = tx + pointer * (tw + gap) + tw * 0.62;
  const py = ty + th * 0.62 - 3 * lift;
  const pt = (pts) => pts.map(([dx, dy]) => `${px + dx},${py + dy}`).join(' ');
  return html`<div class="hub-wscene" ref=${ref}>
    <svg width=${w} height=${h} viewBox=${`0 0 ${w} ${h}`}>
      <g class="roads">
        <line x1=${w * 0.55} y1="-10" x2=${w * 0.8} y2=${h + 10} stroke-width="9" />
        <line x1="-10" y1=${h * 0.5} x2=${w + 10} y2=${h * 0.38} stroke-width="6" />
        <line x1=${w * 0.12} y1=${h + 10} x2=${w * 0.3} y2="-10" stroke-width="5" />
      </g>
      ${[0, 1, 2].map((k) => {
        const up = k === at ? lift : 0;
        const r = [tx + k * (tw + gap) - 1.5 * up, ty - 3 * up, tw + 3 * up, th + 2 * up];
        return html`<g class=${'tile' + (up > 0.01 ? ' up' : '')} style=${`--up:${up}`}>
          ${up > 0.01 && html`<rect x=${r[0]} y=${r[1] + 3 * up} width=${r[2]} height=${r[3]} rx="6" fill=${`rgba(0,0,0,${0.45 * up})`} style=${`filter:blur(${3 + 2 * up}px)`} />`}
          <rect class="face" x=${r[0]} y=${r[1]} width=${r[2]} height=${r[3]} rx="6" />
          <rect class="bar" x=${r[0] + 7} y=${r[1] + r[3] / 2 - 2} width=${r[2] * 0.45} height="4" rx="2" style=${`opacity:${0.22 + 0.2 * up}`} />
        </g>`;
      })}
      ${playing && html`<polygon points=${pt([[-1.2, -1.8], [-1.2, 15.6], [4.2, 11.4], [11.6, 10.6]])} fill="rgba(10,14,24,0.9)" /><polygon points=${pt([[0, 0], [0, 13], [3.6, 9.8], [9, 9.4]])} fill="#fff" />`}
      <line x1=${x0} y1=${y} x2=${x1} y2=${y} stroke="rgb(18,58,107)" stroke-width="7" stroke-linecap="round" />
      <line x1=${x0} y1=${y} x2=${x1} y2=${y} stroke-width="4" stroke-linecap="round" style="stroke:var(--hub-accent)" />
      ${[x0, (x0 + x1) / 2, x1].map((sx) => html`<circle cx=${sx} cy=${y} r="6" fill="rgb(10,107,61)" /><circle cx=${sx} cy=${y} r="4.4" fill="rgb(255,210,63)" />`)}
      <${MiniBus} x=${x0 + (x1 - x0) * along} y=${y - 7} s=${Math.min(1, Math.max(0.8, h / 140))} a=${seen} />
    </svg>
  </div>`;
}

/** The animations, on or off: two cards, each with its little scene; a click sets the setting
 * at once (`welcome::animations`). */
function Animations() {
  const s = useStore();
  const on = s.settings?.animations !== false;
  const [hover, setHover] = useState(false);
  const cards = [
    [true, 'Animations on', 'Screens slide in, tiles rise under the mouse and a bus drives between the menus.'],
    [false, 'Animations off', 'Everything stands still and each screen is there at once: calmer, and lighter for an older computer.'],
  ];
  return html`<div class="hub-wcards anim">
    ${cards.map(([v, title, line]) => html`<button type="button" class=${'hub-wtile hub-wcard' + (on === v ? ' on' : '')}
        onMouseEnter=${() => v && setHover(true)} onMouseLeave=${() => v && setHover(false)} onClick=${() => on !== v && saveSettings({ animations: v })}>
      <${Scene} playing=${v && (on || hover)} />
      <span class="t"><span class=${'hub-radio' + (on === v ? ' on' : '')}></span><b>${tr(title)}</b></span>
      <span class="line">${tr(line)}</span>
    </button>`)}
  </div>`;
}

// --- the folder ------------------------------------------------------------------------------

/** The folder being pointed to: what was typed or chosen, else the one set. */
const folderText = () => W()?.root ?? (S.config?.root || S.boot?.root || '');

let verdictOf = { root: null, v: null, at: 0 };
/** What the folder pointed to is (`hub_more_folder`), looked at again when the path changes
 * and every second (a folder being copied meanwhile). */
function useVerdict() {
  const root = folderText();
  const [, redraw] = useState(0);
  useEffect(() => {
    let gone = false;
    const look = () =>
      call('hub_more_folder', { root })
        .then((v) => {
          if (gone) return;
          verdictOf = { root, v, at: Date.now() };
          redraw((k) => k + 1);
        })
        .catch(() => {});
    look();
    const t = setInterval(look, 1000);
    return () => ((gone = true), clearInterval(t));
  }, [root]);
  return verdictOf.root === root ? verdictOf.v : null;
}

/** The folder kept as the one to play from, as the Setup page's Save does. */
async function saveFolder() {
  const root = W().root;
  if (root == null) return true;
  try {
    const r = await call('config_save', { root: root.trim(), game: S.config?.game || '' });
    set({ config: r.config, boot: { ...S.boot, root: r.root, root_error: r.root_error, game: r.game } });
    put({ root: null });
    loadContent();
    loadProfiles();
    return true;
  } catch (e) {
    toast(e.message, 'error');
    return false;
  }
}

function FolderWay() {
  const v = useVerdict();
  return v?.verdict === 'fine'
    ? html`<${Btn} kind="primary" icon="check" label="Use this folder" onClick=${async () => (await saveFolder()) && next()} />`
    : html`<${Btn} icon="chevron_right" label="Skip for now" onClick=${next} />`;
}

/** The folder: its path (typed, or chosen with Browse), and what it is (`welcome::folder`). */
function Folder() {
  useStore();
  const v = useVerdict();
  const root = folderText();
  const says = {
    fine: ['check_circle', 'ok', 'A complete OMSI 2: the maps, the buses and everything the game needs are there.'],
    empty: ['warning', 'warn', 'openOMSI did not find OMSI 2 by itself. Choose the folder with Omsi.exe in it.'],
    missing: ['warning', 'warn', 'That folder does not exist.'],
    own: ['warning', 'warn', "That is openOMSI's own folder. Choose the original OMSI 2, the one with Omsi.exe in it."],
    lacks: ['warning', 'warn', 'Not a complete OMSI 2: it lacks %{what}.'],
  }[v?.verdict];
  const browse = async () => {
    const p = await call('pick_folder', { title: tr('The OMSI 2 folder (with maps and Vehicles in it)') }).catch(() => null);
    if (p) put({ root: p });
  };
  return html`<div class="hub-wfolder">
    <div class="hub-wcaps">${tr('OMSI 2 folder')}</div>
    <div class="row">
      <label class="hub-wfield"><${Icon} name="folder_open" size=${18} /><input value=${root} placeholder="C:\\Program Files (x86)\\Steam\\steamapps\\common\\OMSI 2" spellcheck="false" onInput=${(e) => put({ root: e.currentTarget.value })} /></label>
      <${Btn} label="Browse" onClick=${browse} />
    </div>
    ${says && html`<div class=${'verdict ' + says[1]}><${Icon} name=${says[0]} size=${17} /><span>${tr(says[2], { what: v.what })}</span></div>`}
    <p class="note">${tr('Steam, the box version, a folder you moved yourself - all fine. openOMSI only reads it and never writes into it.')}</p>
  </div>`;
}

// --- the driver ------------------------------------------------------------------------------

/** The driver chosen (one of this computer's), or the one just made. */
function chosenDriver() {
  const cur = (S.config?.profile || '').trim();
  if (!cur) return null;
  const low = cur.toLowerCase();
  return (S.profiles || []).find((n) => n.toLowerCase() === low) || (W()?.made && W().made.toLowerCase() === low ? W().made : null);
}

/** Choose a driver already on this computer. */
async function chooseDriver(name) {
  try {
    const config = await call('profile_select', { name });
    set({ config, profile: await call('profile', { name }).catch(() => null) });
    put({ typing: false, typed: '', passSeq: (W().passSeq || 0) + 1 });
  } catch (e) {
    toast(e.message, 'error');
  }
}

/** Make the driver typed and choose them; their pass pops in, with the personnel number and
 * code made now (`welcome::create`). */
async function createDriver() {
  const name = (W().typed || '').trim();
  if (!name) {
    put({ focus: true });
    toast(tr('Type a name for your driver first.'), 'info');
    return false;
  }
  try {
    await call('profile_create', { name, sex: 'M' });
    await loadProfiles();
    await chooseDriver(name);
    const p = await call('hub_more_personnel', { name, make: true }).catch(() => null);
    put({ made: name, typing: false, typed: '', personnel: { name, p } });
    return true;
  } catch (e) {
    toast(e.message, 'error');
    return false;
  }
}

/** Next on the driver's step: with a name typed the driver is made first; without a driver the
 * field asks for one (`welcome::driver_next`). */
async function driverNext() {
  const w = W();
  const typed = !!(w.typed || '').trim();
  if ((w.typing || !chosenDriver()) && typed) {
    if (await createDriver()) next();
  } else if (chosenDriver()) next();
  else {
    put({ focus: true });
    toast(tr('Type a name for your driver first.'), 'info');
  }
}

/** The new driver's name and Create (Enter makes them; Escape gives up when there is a driver
 * to go back to) (`welcome::new_driver`). */
function NewDriver({ canCancel }) {
  const w = useStore().hubWelcome;
  const ref = useRef();
  useEffect(() => {
    if (w.focus) {
      ref.current?.focus();
      put({ focus: false });
    }
  }, [w.focus]);
  const cancel = () => put({ typing: false, typed: '' });
  return html`<div class="hub-wnew">
    <div class="hub-wcaps">${tr('New driver')}</div>
    <label class="hub-wfield"><${Icon} name="person" size=${18} /><input ref=${ref} value=${w.typed || ''} placeholder=${tr("The new driver's name")} spellcheck="false"
      onInput=${(e) => put({ typed: e.currentTarget.value })} onKeyDown=${(e) => (e.key === 'Enter' ? createDriver() : e.key === 'Escape' && canCancel ? cancel() : null)} /></label>
    <div class="row">
      <${Btn} kind="primary small" icon="add" label="Create" onClick=${createDriver} />
      ${canCancel && html`<${Btn} kind="small" icon="close" label="Cancel" onClick=${cancel} />`}
    </div>
    <p class="note">${tr("The driver's name is the name of their personnel file, as in OMSI 2: the game adds every duty to it.")}</p>
  </div>`;
}

/** The chosen driver's personnel pass: their initial and name, their level, and - when they
 * were made - the personnel number and code they sign on with (`welcome::pass`). */
function Pass({ name }) {
  const s = useStore();
  const w = s.hubWelcome;
  const known = w.personnel?.name === name ? w.personnel.p : undefined;
  useEffect(() => {
    if (known !== undefined) return;
    call('hub_more_personnel', { name, make: false })
      .then((p) => put({ personnel: { name, p } }))
      .catch(() => put({ personnel: { name, p: null } }));
  }, [name, known]);
  const level = s.profile && String(s.profile.name || '').toLowerCase() === name.toLowerCase() ? s.profile.level || 1 : 1;
  return html`<div>
    <div class=${'hub-wpass' + (known ? ' numbers' : '')} key=${w.passSeq || 0}>
      <div class="band"><span>OPENOMSI  ·  ${tr('Personnel pass').toUpperCase()}</span><${Icon} name="directions_bus" size=${18} /></div>
      <div class="who"><span class="mono">${name.slice(0, 1).toUpperCase()}</span><span><b>${name}</b><small>${tr('Bus driver · level %{level}', { level })}</small></span></div>
      <hr />
      ${known
        ? html`<div class="nums">${[['Personnel number', known.number], ['Code', known.code]].map(([k, v]) => html`<span><small>${tr(k).toUpperCase()}</small><b>${v}</b></span>`)}</div>
            <p>${tr('You sign on with these before every duty.')}</p>`
        : html`<p class="wrap">${tr('Your personnel number and code are made when you first sign on for a duty.')}</p>`}
    </div>
    <button type="button" class="hub-wghost" onClick=${() => put({ typing: true, typed: '', focus: true })}><${Icon} name="add" size=${17} />${tr('New driver')}</button>
  </div>`;
}

/** The drivers on this computer as rows to choose (`welcome::driver_list`). */
function DriverList({ names, chosen }) {
  return html`<div class="hub-wdrivers">
    <div class="hub-wcaps">${tr('On this computer').toUpperCase()}<em>${names.length}</em></div>
    <div class="list">${names.map((n) => {
      const on = !!chosen && chosen.toLowerCase() === n.toLowerCase();
      return html`<button type="button" class=${'hub-wtile hub-wdriver' + (on ? ' on' : '')} onClick=${() => !on && chooseDriver(n)}>
        <span class="mono">${n.slice(0, 1).toUpperCase()}</span><b>${n}</b>${on && html`<${Icon} name="check" size=${18} />`}</button>`;
    })}</div>
  </div>`;
}

/** The driver: a new one or the chosen one's pass, and beside it the drivers already on this
 * computer (`welcome::driver`). */
function Driver() {
  const s = useStore();
  const w = s.hubWelcome;
  const names = s.profiles || [];
  const chosen = chosenDriver();
  const form = w.typing || !chosen;
  return html`<div class=${'hub-wdriverstep' + (names.length ? ' two' : '')}>
    <div class="left">${form ? html`<${NewDriver} canCancel=${!!chosen} />` : html`<${Pass} name=${chosen} />`}</div>
    ${names.length > 0 && html`<${DriverList} names=${names} chosen=${chosen} />`}
  </div>`;
}

// --- ready -----------------------------------------------------------------------------------

/** The way on: the tour or straight to the start, as tiles with a photo each; with the classic
 * launcher chosen, a picture of it instead (`welcome::ready`). */
function Ready() {
  if (classic())
    return html`<div class="hub-wclassic"><span class="pic" style="background-image:url(&quot;omsihub/ui-classic.jpg&quot;)"></span>
      <p>${tr('From now on openOMSI opens in its classic launcher.')}</p></div>`;
  const tiles = [
    ['tour', 'Take the tour', 'route', 'mode-tour', 'A minute along the launcher: where you set up a duty, your service record and the settings.'],
    ['start', 'Start right away', 'play_arrow', 'mode-free', 'Straight to the start: choose how you want to drive today.'],
  ];
  return html`<div class="hub-wready">
    ${tiles.map(([then, title, icon, pic, text]) => html`<button type="button" class=${'hub-wtile hub-wgo ' + then} onClick=${() => finish(then)}>
      <span class="photo" style=${`background-image:url("omsihub/${pic}.jpg")`}></span>
      <span class="words"><i><${Icon} name=${icon} size=${20} /></i><b>${tr(title)}</b><span>${tr(text)}</span></span>
    </button>`)}
  </div>`;
}

// --- the whole -------------------------------------------------------------------------------

/** The welcome in place of the page while it is open. */
export function Welcome() {
  const s = useStore();
  const w = s.hubWelcome;
  useEffect(() => {
    if (!w) return;
    // (the steps without a text field go on with Enter as well)
    const key = (e) => {
      if (e.key !== 'Enter' || W()?.step === 'interface') return;
      if (document.activeElement && /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName)) return;
      const st = W()?.step;
      if (st === 'language' || st === 'animations') next();
      else if (st === 'ready' && !classic()) finish('start');
      else if (st === 'ready') finish('start');
    };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, [!!w]);
  if (!w?.open) return null;
  return html`<div class=${'hub-welcome' + (w.leaving ? ' leaving' : '')}>
    <${Bar} />
    <div class="hub-wroom"><${Sheet} /></div>
    ${w.step === 'interface' && html`<${Dialog} />`}
  </div>`;
}

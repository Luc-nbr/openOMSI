// The bar along the top, as our game-drawn launcher's (`flow::bar`): glass; on the start and on
// a page the logo and the version, on a step of the drive the line's plate and the steps as a
// route; on the right the way home, the light or the dark look, the accent's palette, the
// settings, the running games, and the languages as flags.

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { S, set, useStore } from '../store.js';
import { saveSettings } from '../data.js';
import { Icon, tr } from './bits.js';
import { LOGO_SVG } from './logo.js';
import { applyHubTheme, PRESETS, chosen, dark, hexOf, parseHex } from './theme.js';
import { go } from './hub.js';
import { stepsOf, goStep, canGo, chosenLine } from './flow.js';
import { startTour } from './tour.js';

/** The languages with a flag in the bar (`flow::FLAGS`); the others are in the globe's menu. */
const FLAGS = ['ENG', 'DEU', 'FRA', 'NLD'];

export function Flag({ code }) {
  const bands = (cs, across) => cs.map((c, i) => (across ? html`<rect x=${i * 6} y="0" width="6" height="13" fill=${c} />` : html`<rect x="0" y=${(i * 13) / 3} width="18" height=${13 / 3 + 0.2} fill=${c} />`));
  let body;
  if (code === 'DEU') body = bands(['#000000', '#dd0000', '#ffce00'], false);
  else if (code === 'FRA') body = bands(['#002395', '#ffffff', '#ed2939'], true);
  else if (code === 'NLD') body = bands(['#ae1c28', '#ffffff', '#21468b'], false);
  else body = html`<rect width="18" height="13" fill="#012169" /><path d="M0 0L18 13M18 0L0 13" stroke="#fff" stroke-width="2.6" /><path d="M0 0L18 13M18 0L0 13" stroke="#c8102e" stroke-width="1.3" /><path d="M9 0V13M0 6.5H18" stroke="#fff" stroke-width="4.3" /><path d="M9 0V13M0 6.5H18" stroke="#c8102e" stroke-width="2.6" />`;
  return html`<svg viewBox="0 0 18 13" preserveAspectRatio="none">${body}</svg>`;
}

/** The interface (and the game) in another language. */
const setLanguage = (code) => code !== S.settings?.language && saveSettings({ language: code });

function Languages() {
  const s = useStore();
  const [open, setOpen] = useState(false);
  const cur = s.settings?.language || 'ENG';
  const langs = s.boot?.languages || [];
  return html`<div class="hub-flags">
    ${FLAGS.map((code) => html`<button type="button" class=${'hub-flag' + (cur === code ? ' on' : '')} title=${langs.find((l) => l.code === code)?.name || code} onClick=${() => setLanguage(code)}><${Flag} code=${code} /></button>`)}
    <span style="position:relative">
      <button type="button" class=${'hub-bar-btn' + (!FLAGS.includes(cur) ? ' on' : '')} title=${tr('Other languages')} onClick=${() => setOpen(!open)}><${Icon} name="language" size=${17} /></button>
      ${open && html`<div class="overlay" style="background:none" onMouseDown=${(e) => e.target === e.currentTarget && setOpen(false)}>
        <div class="hub-pop" style="right:22px;width:240px;max-height:70cqh;overflow:auto;padding:8px">
          ${langs.map((l) => html`<button type="button" class=${'hub-rowl' + (l.code === cur ? ' on' : '')} style="width:100%;grid-template-columns:1fr" onClick=${() => (setOpen(false), setLanguage(l.code))}>${l.name}</button>`)}
        </div>
      </div>`}
    </span>
  </div>`;
}

/** The light or the dark look: the new look spreads from the button over the window (a view
 * transition where the webview has them and the animations are on), as our launcher's does. */
export function flipLook(e) {
  const to = !dark();
  const change = () => {
    S.settings = { ...S.settings, dark_mode: to };
    applyHubTheme();
    saveSettings({ dark_mode: to });
  };
  const motion = S.settings?.animations !== false && !matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!document.startViewTransition || !motion || !e) return change();
  const x = e.clientX;
  const y = e.clientY;
  const r = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
  const vt = document.startViewTransition(change);
  vt.ready
    .then(() => document.documentElement.animate({ clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${r}px at ${x}px ${y}px)`] }, { duration: 650, easing: 'cubic-bezier(0.22, 0.8, 0.3, 1)', pseudoElement: '::view-transition-new(root)' }))
    .catch(() => {});
}

/** The accent chosen: the setting, and the whole page recoloured at once. */
export function setAccent(rgb) {
  S.settings = { ...S.settings, accent: hexOf(rgb) };
  applyHubTheme();
  saveSettings({ accent: hexOf(rgb) });
}

/** The palette under the bar's button: the presets as round swatches, any other colour, and the
 * dark mode's switch. A click beside it or Escape closes it. */
export function Palette() {
  const now = chosen();
  const close = () => set({ hub: { ...S.hub, palette: false } });
  useEffect(() => {
    const k = (e) => e.key === 'Escape' && close();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  const custom = !PRESETS.some(([, c]) => c === now);
  return html`<div class="overlay" style="background:none;z-index:39" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="hub-pop" style="right:180px">
      <h3>${tr('Accent colour')}</h3>
      <div class="hub-swatches">
        ${PRESETS.map(([name, c]) => html`<button type="button" class=${'hub-swatch' + (c === now ? ' on' : '')} title=${tr(name)} style=${`background:${hexOf(c)}`} onClick=${() => setAccent(c)}></button>`)}
        <label class="hub-custom" style=${custom ? 'box-shadow:inset 0 0 0 2px var(--hub-text)' : ''}>
          <span style=${`width:12px;height:12px;border-radius:50%;background:${hexOf(now)}`}></span>${tr('Custom…')}
          <input type="color" value=${hexOf(now)} onInput=${(e) => { const c = parseHex(e.currentTarget.value); if (c != null) setAccent(c); }} />
        </label>
      </div>
      <label class="hub-switch"><span>${tr('Dark mode')}</span><input type="checkbox" checked=${dark()} onChange=${(e) => flipLook(null)} /><i></i></label>
    </div>
  </div>`;
}

/** The steps as a route (`flow::step_list`): done ones in ink, the current one in the accent
 * and underlined, the ones to come quiet. A narrow window keeps the words of the current one
 * only (omsihub.css). */
function Steps() {
  const s = useStore();
  const steps = stepsOf();
  const at = Math.max(0, steps.findIndex((x) => x[0] === s.hub.step));
  return html`<nav class="hub-steps">
    ${steps.map(([id, label, icon], k) => {
      const done = k < at;
      const now = k === at;
      const open = done || now || canGo(id);
      return html`${k > 0 && html`<i class=${'hub-step-join' + (k <= at ? ' done' : '')}></i>`}
        <button type="button" class=${'hub-step' + (done ? ' done' : '') + (now ? ' now' : '') + (open ? ' open' : '')} title=${tr(label)} onClick=${() => open && goStep(id)}>
          <${Icon} name=${icon} size=${15} /><span>${tr(label)}</span>
        </button>`;
    })}
  </nav>`;
}

/** The version as the bar shows it: a test build `<release>-beta<N>` as "Beta N"
 * (`updater::shown_version`), any other as it is. */
const shownVersion = (v = '') => {
  const m = /-beta(\d+)$/.exec(v);
  return m ? `Beta ${m[1]}` : v;
};

export function Bar() {
  const s = useStore();
  const onPage = s.page !== 'drive';
  const start = !onPage && s.hub.step === 'mode';
  const plain = start || onPage;
  const version = shownVersion(s.boot?.version || '');
  const running = (s.poll.instances || []).filter((i) => i.running !== false).length;
  const line = !plain && chosenLine();
  const isDark = dark();
  return html`<header class="hub-bar hub-glass">
    ${plain
      ? html`<button type="button" class="hub-brand" onClick=${() => go('drive', 'mode')} title="openOMSI"><span dangerouslySetInnerHTML=${{ __html: LOGO_SVG }} style="display:flex"></span><span class="hub-version">${version}</span></button>`
      : html`${line && html`<span class="hub-plate" style="margin-right:14px;height:24px">${line}</span>`}<${Steps} />`}
    <span class="hub-bar-fill"></span>
    ${!start && html`<button type="button" class="hub-bar-btn" title=${tr('Home')} onClick=${() => go('drive', 'mode')}><${Icon} name="home" size=${17} /></button>`}
    <button type="button" class=${'hub-bar-btn hub-bar-help' + (s.hubTour && !s.hubTour.closing ? ' on' : '')} title=${tr('Show the tour')} onClick=${() => startTour()}><${Icon} name="help" size=${17} /></button>
    <button type="button" class="hub-bar-btn" title=${tr(isDark ? 'Switch to the light mode' : 'Switch to the dark mode')} onClick=${(e) => flipLook(e)}><${Icon} name=${isDark ? 'dark_mode' : 'light_mode'} size=${17} /></button>
    <button type="button" class=${'hub-bar-btn' + (s.hub.palette ? ' on' : '')} title=${tr('Accent colour')} onClick=${() => set({ hub: { ...S.hub, palette: !S.hub.palette } })}><${Icon} name="palette" size=${17} /></button>
    <button type="button" class=${'hub-bar-btn' + (s.page === 'settings' ? ' on' : '')} title=${tr('Settings')} onClick=${() => go('settings')}><${Icon} name="settings" size=${17} /></button>
    ${running > 0 && html`<button type="button" class=${'hub-bar-btn' + (s.page === 'games' ? ' on' : '')} title=${tr('Sessions')} onClick=${() => go('games')}><${Icon} name="sports_esports" size=${17} /><span class="count">${running}</span></button>`}
    ${!plain && html`<span class="hub-version" style="margin:0 4px 0 8px">${version}</span>`}
    <${Languages} />
  </header>`;
}

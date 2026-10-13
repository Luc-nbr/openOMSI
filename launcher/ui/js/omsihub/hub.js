// Omsi-Hub's frame of the desktop launcher (Trello "Tauri 1/6"), after our game-drawn launcher
// (`crates/omsi-launcher-ui/src/flow.rs`): the ground edge to edge, the glass bar along the
// top, and under it either the drive's steps - the start (how do you want to drive today?)
// among them - or one of upstream's pages in a wide sheet with the way back in its corner.
// Upstream's `app.js` hands the desktop window to `HubShell` (one line); the phone keeps
// upstream's page.

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { S, set, useStore } from '../store.js';
import { call, onPhone } from '../api.js';
import { applyHubTheme } from './theme.js';
import { Bar, Palette } from './bar.js';
import { Flow, STEP_NAMES, goStep } from './flow.js';
import { Head, Icon, tr } from './bits.js';
import './settings-extra.js';
import { EDITORS, EditorPage, editorOf } from './editors/index.js';
import { StatusLine } from './editors/kit.js';

/** Our look is the desktop window's (not the phone's, nor `?phone` in a browser). */
export const HUB = !onPhone && !new URLSearchParams(location.search).has('phone');
if (HUB) document.documentElement.classList.add('hub');

/** What our frame keeps: the step of the drive, the palette open or not. */
S.hub = { step: 'mode', palette: false };

/** Upstream's pages as our launcher names them: icon, title and the line under it
 * (`pages::about`). */
export const ABOUT = {
  multiplayer: ['groups', 'Multiplayer', 'Drive with friends by a code, or on servers that are always on.'],
  profile: ['badge', 'Service record', 'Everything the logbook kept, from the first duty to the last.'],
  mods: ['extension', 'Mods', 'A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to.'],
  settings: ['settings', 'Settings', 'Every change is saved at once; the game reads them when it starts.'],
  controls: ['keyboard', 'Controls', 'Click a key and press the new one (hold Shift, Ctrl or Alt for a combination); Escape leaves it as it is.'],
  games: ['sports_esports', 'Sessions', 'The games you started, and who drives with you.'],
  tutorials: ['help', 'Tutorials', "OMSI 2's own lessons: each opens its situation, with its pages beside the picture (Enter turns the page)."],
  setup: ['folder_open', 'Setup', 'Where the original game and this one are.'],
};

/** Go to a page (`drive` is the drive's steps, at `step` when given). */
export function go(page, step) {
  set({ page, hub: { ...S.hub, palette: false, ...(step ? { step } : {}) } });
}

/** The page `OMSI_LAUNCHER_PAGE` names, as our game-drawn launcher reads it: a page
 * (`settings`, `mods` ...) or a step of the drive (`drive:map`, `drive:3`). */
function startPage(p, pages) {
  if (!p) return;
  const [name, part, more] = p.toLowerCase().split(':').map((x) => x.trim());
  // (the editors: "Editor", "Timetable", "Line editor" ... as our game-drawn launcher names them)
  const ed = editorOf(name);
  if (ed) {
    go(ed);
    return;
  }
  if (name === 'drive' || name === 'home') {
    const step = /^\d+$/.test(part || '') ? STEP_NAMES[Number(part)] : part;
    // (`drive:bus:chosen`: the bus chosen on the ground, not the tiles)
    if (step === 'bus' && more === 'chosen') S.hub.busView = 'bus';
    go('drive', STEP_NAMES.includes(step) ? step : 'mode');
    return;
  }
  const page = pages.find((x) => x && (x[0] === name || x[1].toLowerCase() === name)) || (name === 'service record' ? ['profile'] : name === 'sessions' ? ['games'] : null);
  if (page) go(page[0]);
}

let booted = false;

/** The interface's size (`ui_scale` of our game-drawn launcher): laid out for a 1440 x 820
 * window, drawn smaller under that (down to 80 %), times the setting "Launcher size". */
function scale() {
  const own = Math.max(0.6, Math.min(2, Number(S.settings?.launcher_scale) || 1));
  return Math.max(0.8, Math.min(1, innerWidth / 1440, innerHeight / 820)) * own;
}

export function HubShell({ pages, Toasts }) {
  const s = useStore();
  applyHubTheme();
  document.documentElement.classList.toggle('hub-still', s.settings?.animations === false);
  const [, redraw] = useState(0);
  useEffect(() => {
    const r = () => redraw((n) => n + 1);
    addEventListener('resize', r);
    return () => removeEventListener('resize', r);
  }, []);
  useEffect(() => {
    if (booted || s.boot?.selftest) return;
    booted = true;
    call('hub_boot')
      .then((b) => startPage(b?.page, pages))
      .catch(() => {});
  }, []);
  // (an editor's page: ours, `editors/index.js`)
  const editor = EDITORS[s.page] ? s.page : null;
  const page = s.page === 'drive' || editor ? null : pages.find((x) => x && x[0] === s.page);
  return html`<div class="hub-stage" style=${`--hub-scale:${scale()}`}>
    ${editor ? html`<${EditorPage} id=${editor} />` : page ? html`<${PageSheet} id=${page[0]} label=${page[1]} Page=${page[3]} />` : html`<${Flow} />`}
    ${!EDITORS[s.page]?.full && html`<${Bar} />`}
    ${s.hub.palette && html`<${Palette} />`}
    <${StatusLine} />
    <${Toasts} />
  </div>`;
}

/** One of upstream's pages in the wide sheet under the bar: our head says what it is and for
 * what, the way back in its corner; the page's own head keeps only what it has in its corner
 * (the Controls page's switch). */
function PageSheet({ id, label, Page }) {
  const [icon, title, sub] = ABOUT[id] || ['info', label, ''];
  return html`<section class="hub-sheet page" key=${id}>
    <${Head} icon=${icon} title=${title} sub=${tr(sub)}
      tools=${html`<button type="button" class="hub-btn hub-page-back" onClick=${() => go('drive')}><${Icon} name="chevron_left" size=${18} />${tr('Back')}</button>`} />
    <div class="page hub-page-body"><${Page} /></div>
  </section>`;
}

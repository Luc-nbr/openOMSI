// The start (the drive's "Mode" step), as our game-drawn launcher's (`flow::step_mode`): the
// logo over the greeting, the ways to drive as tiles with a photo each - a work shift, a tour,
// a free drive, and the bus company beside them - and under them the driver's service record
// and the ways to everything else. The glass card is as tall as what is on it.

import { html } from '../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../store.js';
import { call } from '../api.js';
import { choose } from '../data.js';
import { Icon, tr, hoursText } from './bits.js';
import { LOGO_SVG } from './logo.js';
import { go } from './hub.js';
import { openEditor, openTimetable } from './editors/index.js';
import { goStep } from './flow.js';
import { enterCompany } from './company/company.js';

/** The ways to drive (kinds: 0 a tour, 1 a shift, 2 free, 4 the bus company), as the tiles
 * say them. */
const MODES = [
  [1, 'Work shift', 'schedule', 'mode-shift', "Choose how long you want to drive and when: openOMSI puts a shift together from the timetable's real trips, on with another line where lines meet."],
  [0, 'Tour', 'route', 'mode-tour', "One bus's trips on one line, as OMSI's timetable dialog gives them: pick the line, the tour and the trip to start with."],
  [2, 'Free drive', 'map', 'mode-free', "Only a map and a bus. The traffic and the timetable's buses drive around you; nothing is booked."],
  [4, 'Bus company', 'garage', 'mode-company', 'Your own transport company: buy, lease or rent buses, hire drivers and run lines day by day.'],
];

/** Hand over to our game-drawn launcher on one of its pages (`OMSI_LAUNCHER_PAGE`): what this
 * page does not have yet - the bus company until phase 2 (the editors are this page's own). */
export async function handOver(page, why) {
  if (S.boot?.selftest) return;
  if (why) toast(tr(why), 'info');
  try {
    await call('classic_launcher', { page });
  } catch (e) {
    toast(e.message, 'error');
  }
}

/** The greeting by the hour of the day. */
function greeting(name) {
  const h = new Date().getHours();
  const key = h >= 5 && h <= 11 ? 'Good morning %{name}' : h >= 12 && h <= 17 ? 'Good afternoon %{name}' : 'Good evening %{name}';
  return tr(key, { name });
}

/** The way to drive chosen now (0 a tour, 1 a shift, 2 free). */
const current = (c) => (c.free ? 2 : c.composed ? 1 : 0);

function pick(kind) {
  if (kind === 4) {
    enterCompany();
    return;
  }
  choose({ free: kind === 2, composed: kind === 1 });
  goStep('map');
}

export function Home() {
  const s = useStore();
  const c = s.choice;
  const p = s.profile;
  const name = p?.name || s.config?.profile || '';
  const on = current(c);
  const level = p ? Math.max(0, Math.min(1, (p.xp - Math.max(0, p.level - 1) ** 2 * 250) / Math.max(1, p.next_level_xp - Math.max(0, p.level - 1) ** 2 * 250))) : 0;
  const links = [
    [tr('Driver: %{name}', { name }), 'person', () => goStep('profile')],
    [tr('Mods'), 'extension', () => go('mods')],
    [tr('Settings'), 'settings', () => go('settings')],
    [tr('Controls'), 'keyboard', () => go('controls')],
    [tr('Timetable'), 'schedule', openTimetable],
    [tr('Multiplayer'), 'groups', () => go('multiplayer')],
    [tr('Sessions'), 'sports_esports', () => go('games')],
    [tr('Tutorials'), 'help', () => go('tutorials')],
    [tr('Setup'), 'folder_open', () => go('setup')],
  ];
  return html`<div class="hub-start">
    <section class="hub-card hub-glass glow">
      <div dangerouslySetInnerHTML=${{ __html: LOGO_SVG }} style="display:contents"></div>
      <h1 class="hub-greeting">${greeting(name)}</h1>
      <p class="hub-question">${tr('How do you want to drive today?')}</p>
      <div class="hub-modes">
        ${MODES.map(([kind, title, icon, picture, text]) => html`<button type="button" class=${'hub-mode' + (kind === on ? ' on' : '')} data-mode=${kind} onClick=${() => pick(kind)}>
          <span class="photo" style=${`background-image:url("omsihub/${picture}.jpg")`}></span>
          <span class="what">
            <span class="badge"><${Icon} name=${icon} size=${21} /></span>
            <b>${tr(title)}</b>
            <p>${tr(text)}</p>
          </span>
        </button>`)}
      </div>
      <div class="hub-under">
        <button type="button" class="hub-record" onClick=${() => go('profile')}>
          <h3>${tr('Your service record')}</h3>
          <div class="hub-figures">
            <div><b>${p?.record?.runs ?? 0}</b><span>${tr('duties')}</span></div>
            <div><b>${hoursText(p?.hours || 0)}</b><span>${tr('driven')}</span></div>
            <div><b>${Math.round(p?.km || 0)}</b><span>${tr('km')}</span></div>
            <div><b>${p?.level ?? 1}</b><span>${tr('level')}</span></div>
          </div>
          ${p && html`<div class="hub-level">${tr('Level %{level} · %{xp} of %{next} points', { level: p.level, xp: p.xp, next: p.next_level_xp })}</div>
            <div class="hub-progress"><div style=${`width:${level * 100}%`}></div></div>`}
          <span class="hub-more">${tr('Open the service record')}<${Icon} name="chevron_right" size=${16} /></span>
        </button>
        <div class="hub-links">
          ${links.map(([label, icon, onClick, away]) => html`<button type="button" class="hub-link" onClick=${onClick}><${Icon} name=${icon} size=${16} /><span>${label}</span>${away && html`<${Icon} class="away" name="open_in_new" size=${14} />`}</button>`)}
          <button type="button" class="hub-link long" onClick=${openEditor}><${Icon} name="construction" size=${16} /><span>${tr("Editor: lines, liveries, the timetable and the map's objects")}</span></button>
        </div>
      </div>
    </section>
  </div>`;
}

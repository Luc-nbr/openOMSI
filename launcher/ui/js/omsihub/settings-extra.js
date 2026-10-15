// The settings Omsi-Hub has and upstream's Settings page lacks, put into upstream's schema
// (`pages/settings-schema.js`) when our look is on - so the page itself stays upstream's: the
// light or the dark look, the accent and the launcher's size (instead of upstream's own theme,
// drive layout and size, which our look does not use), the animations, the IBIS filled in by
// itself, the phone and tablet companion, the head tracking's source and the Tobii's edges, the
// navigator's board, size, stop signs and signing on, and the updates' Check now with this
// fork's releases. (Speeds follow upstream's own Units.)

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { SETTINGS_TABS } from '../pages/settings-schema.js';
import { onPhone } from '../api.js';
import { PRESETS, hexOf } from './theme.js';
import { UpdateRow } from './update.js';

const pct = (v) => `${Math.round(v * 100)} %`;
const deg = (v) => `${v.toFixed(2)}°`;
const r2 = (v) => Math.round(v * 100) / 100;

const tab = (id) => SETTINGS_TABS.find((x) => x.id === id);
/** Put `rows` after the row `after` matches (or at the end) of a tab's column. */
function insert(tabId, col, rows, after) {
  const list = tab(tabId)?.cols[col]?.rows;
  if (!list) return;
  const at = after ? list.findIndex(after) : -1;
  list.splice(at >= 0 ? at + 1 : list.length, 0, ...rows);
}

const HUB = !onPhone && !new URLSearchParams(location.search).has('phone');

if (HUB && !SETTINGS_TABS.omsihub) {
  SETTINGS_TABS.omsihub = true;
  // the look: ours instead of upstream's theme (light, dark or the system's, kept in the page)
  const general = tab('general')?.cols[0]?.rows;
  const theme = general?.findIndex((r) => r.type === 'custom' && r.id === 'theme') ?? -1;
  // (in the game-drawn launcher's order, under the phone and tablet companion)
  const look = [
    { type: 'slider', key: 'launcher_scale', label: 'Launcher size', min: 0.6, max: 2, step: 0.05, def: 1, format: pct, store: (v) => Math.round(v * 20) / 20 },
    { type: 'toggle', key: 'animations', label: 'Animations' },
    { type: 'select', key: 'accent', label: 'Accent colour', options: PRESETS.map(([name, c]) => [hexOf(c), name]) },
    { type: 'toggle', key: 'dark_mode', label: 'Dark mode' },
    // (phase 3: the first start's welcome and the tour through the launcher once more, as our
    // game-drawn launcher's General tab has them; no setting changes by them - the welcome's
    // own `welcome_done` written a moment after the page's save of the unchanged ones)
    { type: 'button', label: 'Show the welcome screen again', icon: 'restart_alt', patch: () => (setTimeout(() => import('./welcome.js').then((m) => m.showWelcomeAgain()), 250), {}) },
    { type: 'button', label: 'Start the tour', icon: 'route', patch: () => (import('./hub.js').then((h) => (h.go('drive', 'mode'), import('./tour.js').then((m) => setTimeout(() => m.startTour(), 300)))), {}) },
  ];
  if (general && theme >= 0) general.splice(theme, 1);
  // (upstream's drive layout and launcher size: our drive's steps and our own size above)
  for (const id of ['launcher_layout', 'launcher_size']) {
    const k = general?.findIndex((r) => r.type === 'custom' && r.id === id) ?? -1;
    if (k >= 0) general.splice(k, 1);
  }
  // the phone and tablet companion, after the voice chat
  insert('general', 0, [
    { type: 'toggle', key: 'companion', label: 'Phone & tablet', help: 'Sign on, the duty menu and the bus\'s screens on a phone or tablet in your network. The game shows the address and the pairing code when you drive.' },
    { type: 'toggle', key: 'companion_hide', label: 'Hide address and pairing code (streaming)', when: (s) => !!s.companion },
    { type: 'toggle', key: 'companion_tunnel', label: 'Connect through Cloudflare (also outside your network)', when: (s) => !!s.companion },
    ...look,
  ], (r) => r.key === 'voice_chat');
  // the navigator's own, after its AI vehicles
  insert('general', 0, [
    { type: 'toggle', key: 'nav_board', label: 'Duty board under the navigator' },
    { type: 'slider', key: 'nav_scale', label: 'Navigator size', min: 0.6, max: 2, step: 0.05, def: 1, format: pct, store: (v) => Math.round(v * 20) / 20 },
    { type: 'select', key: 'stop_style', label: 'Bus stop signs', options: [['de', 'German (H)'], ['uk', 'British'], ['fr', 'French']] },
    { type: 'toggle', key: 'nav_signon', label: 'Sign on with personnel number and code', help: 'Before a duty the navigator asks for your number and code and for the duty order to be accepted. Off, the map and the duty show at once.' },
  ], (r) => r.key === 'nav_ai');
  // (phase 6: the navigator back to its corner, its own size, the board under it - as the game-drawn
  // launcher's "Reset navigator" does)
  insert('general', 0, [{ type: 'button', label: 'Reset navigator', icon: 'restart_alt', patch: () => ({ navigator_corner: 'bottom-left', nav_rect: '', nav_scale: 1, nav_board: true }) }], (r) => r.type === 'custom' && r.id === 'corner');
  // the IBIS typed in for the driver on a duty, after the automated gearbox
  insert('driving', 0, [{ type: 'toggle', key: 'ibis_auto', label: 'Fill in the IBIS automatically' }], (r) => r.key === 'auto_shift');
  // head tracking: where the pose comes from, the Tobii's edges and its sensitivities - under
  // the switch while it is on, as the game-drawn launcher's
  const tracking = tab('camera')?.cols[1]?.rows.find((r) => r.key === 'head_tracking');
  if (tracking) tracking.label = 'Head tracking (Tobii eye trackers, native TrackIR / OpenTrack)';
  const on = (s) => !!s.head_tracking;
  const tobii = (s) => on(s) && s.head_tracking_source !== 'opentrack';
  insert('camera', 1, [
    { type: 'select', key: 'head_tracking_source', label: 'Head tracking with', options: [['auto', 'Automatic'], ['tobii', 'Tobii eye tracker'], ['opentrack', 'opentrack (UDP 4242)']], when: on },
    { type: 'toggle', key: 'tobii_edges', label: "Look further at the screen's edges (Tobii)", when: tobii },
    { type: 'slider', key: 'head_tracking_yaw', label: 'Sensitivity left and right', min: 0, max: 4, step: 0.05, def: 1, format: pct, store: r2, when: on },
    { type: 'slider', key: 'head_tracking_pitch', label: 'Sensitivity up and down', min: 0, max: 4, step: 0.05, def: 1, format: pct, store: r2, when: on },
    { type: 'slider', key: 'head_tracking_roll', label: 'Sensitivity for tilting', min: 0, max: 2, step: 0.05, def: 0, format: pct, store: r2, when: on },
    { type: 'slider', key: 'head_tracking_move', label: 'Sensitivity for leaning', min: 0, max: 3, step: 0.05, def: 1, format: pct, store: r2, when: on },
    { type: 'slider', key: 'head_tracking_smoothing', label: 'Smoothing', min: 0, max: 1, step: 0.05, def: 0.5, format: pct, store: r2, when: tobii },
    { type: 'slider', key: 'head_tracking_dead_zone', label: 'Dead zone', min: 0, max: 1, step: 0.05, def: 0.1, format: deg, store: r2, when: tobii },
  ], (r) => r.key === 'head_tracking');
  // (phase 6: the updates' Check now and this fork's releases in place of upstream's version line)
  for (const t of SETTINGS_TABS) {
    for (const c of t.cols || []) {
      const k = (c.rows || []).findIndex((r) => r.type === 'custom' && r.id === 'version');
      if (k >= 0) c.rows.splice(k, 1, { type: 'custom', id: 'hub-update', view: () => html`<${UpdateRow} />` });
    }
  }
  // (Beta 54) the bus models' categories and regions, for every company: what the dealer, the
  // market and the shop of the bus company go by (`company/vehicles.js`)
  SETTINGS_TABS.push({ id: 'vehicles', label: 'Vehicles', cols: [{ title: 'Bus categories and regions', rows: [{ type: 'custom', id: 'hub-vehicles', view: () => html`<${Vehicles} />` }] }] });
  // (the add-on, phase 1) Omsi-Hub beside the player's own openOMSI: its folder and the plugin there
  SETTINGS_TABS.push({ id: 'addon', label: 'Your openOMSI', cols: [{ title: 'Play with your own openOMSI', rows: [{ type: 'custom', id: 'hub-addon', view: () => html`<${Addon} />` }] }] });
}

/** Your openOMSI (loaded with the tab: `addon.js`). */
function Addon() {
  const [m, setM] = useState(null);
  useEffect(() => {
    import('./addon.js').then(setM).catch(() => {});
  }, []);
  return m ? html`<${m.AddonPanel} />` : null;
}

/** The models' categories and regions (loaded with the tab: the company's list). */
function Vehicles() {
  const [m, setM] = useState(null);
  useEffect(() => {
    import('./company/vehicles.js').then(setM).catch(() => {});
  }, []);
  return m ? html`<${m.VehicleClasses} />` : null;
}

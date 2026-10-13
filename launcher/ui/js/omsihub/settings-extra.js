// The settings our game-drawn launcher has and upstream's Settings page lacks, put into
// upstream's schema (`pages/settings-schema.js`) when our look is on - so the page itself stays
// upstream's: the light or the dark look and the accent (instead of upstream's own theme, which
// this window does not use), the animations, the IBIS filled in by itself, the phone and tablet
// companion, the head tracking's source and the Tobii's edges, the navigator's board, size,
// stop signs, speed unit and signing on, and which launcher a plain start opens.

import { SETTINGS_TABS } from '../pages/settings-schema.js';
import { onPhone } from '../api.js';
import { PRESETS, hexOf } from './theme.js';

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
  const look = [
    { type: 'toggle', key: 'dark_mode', label: 'Dark mode' },
    { type: 'select', key: 'accent', label: 'Accent colour', options: PRESETS.map(([name, c]) => [hexOf(c), name]) },
    { type: 'slider', key: 'launcher_scale', label: 'Launcher size', min: 0.6, max: 2, step: 0.05, def: 1, format: pct, store: (v) => Math.round(v * 20) / 20 },
    { type: 'toggle', key: 'animations', label: 'Animations' },
    { type: 'select', key: 'launcher_ui', label: 'Launcher', options: [['new', 'Omsi-Hub (in the game\'s window)'], ['classic', 'Classic'], ['desktop', 'Desktop (this window)']], help: 'Which launcher a plain start of the game opens.' },
  ];
  if (general && theme >= 0) general.splice(theme, 1, ...look);
  // the phone and tablet companion, after the voice chat
  insert('general', 0, [
    { type: 'toggle', key: 'companion', label: 'Phone & tablet', help: 'Sign on, the duty menu and the bus\'s screens on a phone or tablet in your network. The game shows the address and the pairing code when you drive.' },
    { type: 'toggle', key: 'companion_hide', label: 'Hide address and pairing code (streaming)', when: (s) => !!s.companion },
    { type: 'toggle', key: 'companion_tunnel', label: 'Connect through Cloudflare (also outside your network)', when: (s) => !!s.companion },
  ], (r) => r.key === 'voice_chat');
  // the navigator's own, after its AI vehicles
  insert('general', 0, [
    { type: 'toggle', key: 'nav_board', label: 'Duty board under the navigator' },
    { type: 'slider', key: 'nav_scale', label: 'Navigator size', min: 0.6, max: 2, step: 0.05, def: 1, format: pct, store: (v) => Math.round(v * 20) / 20 },
    { type: 'select', key: 'stop_style', label: 'Bus stop signs', options: [['de', 'German (H)'], ['uk', 'British'], ['fr', 'French']] },
    { type: 'select', key: 'speed_unit', label: 'Speed in', options: [['kmh', 'km/h'], ['mph', 'mph (miles per hour)']] },
    { type: 'toggle', key: 'nav_signon', label: 'Sign on with personnel number and code', help: 'Before a duty the navigator asks for your number and code and for the duty order to be accepted. Off, the map and the duty show at once.' },
  ], (r) => r.key === 'nav_ai');
  // the IBIS typed in for the driver on a duty, after the automated gearbox
  insert('driving', 0, [{ type: 'toggle', key: 'ibis_auto', label: 'Fill in the IBIS automatically' }], (r) => r.key === 'auto_shift');
  // head tracking: where the pose comes from, the Tobii's edges and its sensitivities
  insert('camera', 1, [
    { type: 'select', key: 'head_tracking_source', label: 'Head tracking with', options: [['auto', 'Automatic'], ['tobii', 'Tobii eye tracker'], ['opentrack', 'opentrack (UDP 4242)']] },
    { type: 'toggle', key: 'tobii_edges', label: "Look further at the screen's edges (Tobii)", when: (s) => s.head_tracking_source !== 'opentrack' },
    { type: 'slider', key: 'head_tracking_yaw', label: 'Sensitivity left and right', min: 0, max: 4, step: 0.05, def: 1, format: pct, store: r2 },
    { type: 'slider', key: 'head_tracking_pitch', label: 'Sensitivity up and down', min: 0, max: 4, step: 0.05, def: 1, format: pct, store: r2 },
    { type: 'slider', key: 'head_tracking_roll', label: 'Sensitivity for tilting', min: 0, max: 2, step: 0.05, def: 0, format: pct, store: r2 },
    { type: 'slider', key: 'head_tracking_move', label: 'Sensitivity for leaning', min: 0, max: 3, step: 0.05, def: 1, format: pct, store: r2 },
    { type: 'slider', key: 'head_tracking_smoothing', label: 'Smoothing', min: 0, max: 1, step: 0.05, def: 0.5, format: pct, store: r2, when: (s) => s.head_tracking_source !== 'opentrack' },
    { type: 'slider', key: 'head_tracking_dead_zone', label: 'Dead zone', min: 0, max: 1, step: 0.05, def: 0.1, format: deg, store: r2, when: (s) => s.head_tracking_source !== 'opentrack' },
  ], (r) => r.key === 'head_tracking');
}

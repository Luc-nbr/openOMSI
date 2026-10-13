// The editor (`crates/omsi-launcher-ui/src/editor_hub.rs`): what a player makes of a map rather
// than drives on it, each a tile - the line editor, the depot editor, the livery editor, the
// timetable, and the map's objects (the game's own object editor, started on the map chosen
// here). It opens from the start's link "Editor".

import { html, useState } from '../../../vendor/preact-htm.js';
import { S, useStore } from '../../store.js';
import { duty } from '../../data.js';
import { call } from '../../api.js';
import { Icon, tr, Drop, Btn, status, mapLabel } from './kit.js';
import { openLineEditor, openDepotEditor, openLivery, openTimetable } from './index.js';

const TILES = [
  ['Line editor', 'route', 'Compose a line of your own on any map: click its stops, the way between them is found over the roads. You and the timetable\'s buses drive it.', () => openLineEditor()],
  ['Depot editor', 'departure_board', 'A depot file of your own: the destinations and what each display shows, the IBIS\'s stops and routes, special and service trips. A line can choose it, and every bus that drives the line gets it.', () => openDepotEditor()],
  ['Livery editor', 'palette', 'Paint a bus in colours of your own and drive it.', () => openLivery()],
  ['Timetable', 'schedule', 'The map\'s lines: their tours and when each trip leaves.', () => openTimetable()],
  ['Map objects', 'open_with', 'The game\'s object editor on a map: move, turn, add and delete its objects and shape the ground. The game starts with the editor on.', null],
];

/** Start the game's object editor on `map` (`State::launch_editor`): the bus chosen (else the
 * first), no timetable, no traffic, no session, the editor on. */
async function startObjects(map) {
  if (S.boot?.selftest) return;
  const c = S.choice;
  const bus = [c.bus, c.last_bus].find((b) => b && (S.vehicles || []).some((v) => v.file === b)) || S.vehicles?.[0]?.file;
  if (!bus) {
    status('Choose a bus on the Drive page first: the editor starts with it on the map.', true);
    return;
  }
  try {
    const d = await duty();
    const other = d.map !== map;
    Object.assign(d, { map, bus, ...(bus !== c.bus ? { paint: null, number: null } : {}), ...(other ? { entry: 0, hof: null } : {}), line: null, tour: null, trip: null, whole_tour: false, schedule: false, traffic: 0, lan: 'off', editor: true });
    status('Starting the object editor…');
    await call('launch', { duty: d });
  } catch (e) {
    status(e.message, true);
  }
}

function Objects() {
  const s = useStore();
  const maps = s.maps || [];
  const [map, setMap] = useState(null);
  if (!maps.length) return html`<p class="ed-para" style="margin-top:auto">${tr('No maps found.')}</p>`;
  const chosen = map ?? (maps.some((m) => m.file === s.choice.map) ? s.choice.map : maps[0].file);
  const busy = (s.poll.instances || []).some((i) => i.running !== false);
  return html`<div class="controls">
    <${Drop} raw value=${chosen} options=${maps.map((m) => [m.file, mapLabel(m)])} onChange=${setMap} />
    <${Btn} kind="primary" icon="play_arrow" label="Start the object editor" disabled=${busy} onClick=${() => startObjects(chosen)} />
  </div>`;
}

export function EditorHub() {
  return html`<div class="ed-hub">
    ${TILES.map(([title, icon, text, open]) =>
      open
        ? html`<button type="button" class="ed-tile" data-tile=${title} onClick=${open}>
            <span class="top"><span class="badge"><${Icon} name=${icon} size=${23} /></span><b>${tr(title)}</b><${Icon} name="chevron_right" size=${22} /></span>
            <p>${tr(text)}</p>
          </button>`
        : html`<div class="ed-tile still" data-tile=${title}>
            <span class="top"><span class="badge"><${Icon} name=${icon} size=${23} /></span><b>${tr(title)}</b></span>
            <p>${tr(text)}</p>
            <${Objects} />
          </div>`,
    )}
  </div>`;
}

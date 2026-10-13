// The line editor's state on the page: what the game program last answered (`view`, the line
// editor itself is kept there: `lineeditor::headless`), the map's roads and stops for the
// canvas, and the requests - one at a time, in the order they were made, so that what is
// typed arrives as it was typed.

import { S, set } from '../../../store.js';
import { call } from '../../../api.js';
import { language } from '../../../i18n.js';
import { choose } from '../../../data.js';
import { keep, status, reloadLines } from '../kit.js';

/** What the page keeps: the view, the map's data, the camera, the mouse's business. */
export const L = keep('lines', { view: null, map: null, mapKey: null, cam: null, hover: -1, drag: null, opened: 0, polling: false, hooks: {} });

export const redraw = () => set({});

let chain = Promise.resolve();

/** A request of the line editor (`op` and what it takes), its answer taken in. */
export function le(op, args = {}) {
  chain = chain.then(async () => {
    try {
      const v = await call('hub_le', { op, lang: language(), ...args });
      take(v);
    } catch (e) {
      status(e.message, true);
    }
  });
  return chain;
}

/** The answer taken in: what it says, where it goes, the map's data asked for once read. */
function take(v) {
  L.view = v;
  if (v.status) status(v.status[0], v.status[1]);
  if (v.go) follow(v.go);
  if (v.ready && v.map_key && v.map_key !== L.mapKey) {
    L.mapKey = v.map_key;
    call('hub_le', { op: 'map_data', lang: language() })
      .then((m) => {
        if (m?.key !== L.mapKey) return;
        L.map = m;
        L.cam = null;
        redraw();
      })
      .catch((e) => status(e.message, true));
  }
  if (!v.map_key) L.map = null;
  // (the map is being read: asked again until it is)
  if (v.reading && !L.polling) {
    L.polling = true;
    setTimeout(() => {
      L.polling = false;
      if (S.page === 'lines') le('view');
    }, 400);
  }
  redraw();
}

/** Where the line editor sends the page next (`go_drive`, `go_depots`, `go_timeplan`), and
 * what it changed elsewhere. */
function follow(go) {
  if (go.reload_lines) reloadLines();
  if (go.company_changed && L.hooks.companyChanged) L.hooks.companyChanged(go.company_changed);
  if (go.drive) {
    const d = go.drive;
    const c = S.choice;
    choose({ ...(c.map !== d.map ? { map: d.map, entry: -1 } : {}), free: true, composed: false, own_line: true, my_lines: d.my_lines, free_line: d.free_line, free_route: d.free_route });
    set({ page: 'drive', hub: { ...S.hub, step: 'start', palette: false } });
  }
  if (go.depots != null) {
    import('../index.js').then((m) => m.openDepotEditor({ key: go.depots, fromLines: true }));
  }
  if (go.timeplan) {
    // (RosterFlow's Timeplan: the later parts' page; until then it is said where it is)
    if (L.hooks.timeplan) L.hooks.timeplan(go.timeplan);
    else status(go.timeplan.hint || 'Open in the Timeplan');
  }
}

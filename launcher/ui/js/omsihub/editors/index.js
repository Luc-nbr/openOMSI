// Omsi-Hub's editors in the desktop launcher (Trello "Tauri 5/6"), after our game-drawn
// launcher's (`crates/omsi-launcher-ui/src/{editor_hub, timetable, lineeditor, depoteditor,
// livery}`): the pages, how each is reached, and its way back. Our frame (`../hub.js`) draws
// the page `S.page` names when it is one of these: in the wide sheet under the bar, with its
// name, what it is for and the way back in its head (`flow::page`) - the livery studio over the
// whole window, as the game-drawn launcher has it.
//
// The ways in, for the start's links and for the later parts' entry points (the bus company's
// Lines page, RosterFlow's New line): `openEditor`, `openTimetable`, `openLineEditor`,
// `openDepotEditor` and `openLivery`.

import { html } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { Head, Icon, tr } from '../bits.js';
import { EditorHub } from './hub.js';
import { TimetablePage } from './timetable.js';

// (our editors' look, after omsihub.css: style sheets of their own, loaded with them)
for (const sheet of ['editors.css', 'editors-hof.css']) {
  if (document.querySelector(`link[data-ed-css="${sheet}"]`)) continue;
  const l = document.createElement('link');
  l.rel = 'stylesheet';
  l.href = 'omsihub/' + sheet;
  l.dataset.edCss = sheet;
  document.head.appendChild(l);
}

/** What the editors keep between their pages (each page its own part). */
S.ed = S.ed || {};

/** The editor pages: icon, title, what it is for (`pages::about`), the page, and where Back
 * goes (`flow::page`). The name `OMSI_LAUNCHER_PAGE` gives each (`PAGES` of `lib.rs`). */
export const EDITORS = {
  editor: { name: 'Editor', icon: 'construction', title: 'Editor', sub: 'Make the map your own: lines of your own, liveries, the timetable and the map\'s objects.', View: EditorHub, back: () => 'drive' },
  timetable: { name: 'Timetable', icon: 'schedule', title: 'Timetable', sub: "A map's lines: their tours and when each trip leaves. Saved as the line's .ttl (in the content folder; OMSI 2's own files stay as they are).", View: TimetablePage, back: () => 'drive' },
};

/** Register a page of the editors (the later files of this folder: the line, depot and livery
 * editors), as `EDITORS` has them. */
export function addEditor(id, page) {
  EDITORS[id] = page;
}

/** The page `OMSI_LAUNCHER_PAGE` names, if it is an editor's ("Timetable", "line editor"...). */
export function editorOf(name) {
  const n = String(name || '').trim().toLowerCase();
  return Object.keys(EDITORS).find((id) => id === n || EDITORS[id].name.toLowerCase() === n) || null;
}

/** Go to an editor page (`l.go`), the palette closed. */
export function goEditor(id) {
  set({ page: id, hub: { ...S.hub, palette: false } });
}

/** The editor: the line, depot and livery editors, the timetable and the map's objects. */
export const openEditor = () => goEditor('editor');

/** The Timetable page. */
export const openTimetable = () => goEditor('timetable');

/** The line editor (`lineeditor::open_for_company`, `open_new_for_company_with`,
 * `open_map_line_for_company`): the player's own lines - or, for the bus company `company`
 * (its id), its line `line` (an id; none: a new one), a new line whose timetable starts with
 * `newWithDays`, or the map's line `mapLine` (its timetable name) it runs. */
export function openLineEditor(opts = {}) {
  S.ed.lineOpen = { ...opts, at: Date.now() };
  goEditor('lines');
}

/** The depot editor on the player's depot file `key` (none: the list), `fromLines` when the
 * line editor opened it (Back goes there then). */
export function openDepotEditor({ key = '', fromLines = false } = {}) {
  S.ed.depotOpen = { key, fromLines, at: Date.now() };
  goEditor('depots');
}

/** The livery studio on `bus` in `paint` (none: it asks for a bus); for the bus company
 * `company` (its id), its bus `vehicle` (none: a design for the dealer's bus). */
export function openLivery(opts = {}) {
  S.ed.liveryOpen = { ...opts, at: Date.now() };
  goEditor('livery');
}

/** An editor page in the wide sheet under the bar (`flow::page`): its name and what it is for
 * in the head, the way back in its corner; or the whole window (the livery studio). */
export function EditorPage({ id }) {
  useStore();
  const e = EDITORS[id];
  if (!e) return null;
  const View = e.View;
  if (e.full) return html`<${View} />`;
  return html`<section class="hub-sheet page ed-page" key=${id}>
    <${Head} icon=${e.icon} title=${e.title} sub=${tr(typeof e.sub === 'function' ? e.sub() : e.sub)}
      tools=${html`${e.tools && html`<${e.tools} />`}<button type="button" class="hub-btn hub-page-back" onClick=${() => goEditor(e.back())}><${Icon} name="chevron_left" size=${18} />${tr('Back')}</button>`} />
    <div class="ed-page-body"><${View} /></div>
  </section>`;
}

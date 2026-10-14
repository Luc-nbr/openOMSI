// What the bus step has beyond phase 1's (Trello "Tauri 3/6"), as our game-drawn launcher's
// (`buspick.rs`, `busphoto.rs`): the tiles' photos - the bus drawn by the game's renderer, as
// the game-drawn launcher took it (`co_photo`; one not there taken by the game program, phase 6),
// its initials on the night's gradient until then -
// and the bus that fits the duty best (`hub_more_bus_best`): marked on its tiles, said under the
// grid and on its sheet. A free drive has no trips to fit: there the bus the map's depot runs
// most is marked.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, useStore } from '../../store.js';
import { call } from '../../api.js';
import { Icon, tr } from '../bits.js';
import './more.js';
import { photo, wantPhoto } from '../photos.js';

/** The duty the bus is weighed for: map, day, free or not, the tour or the shift's legs. */
function dutyOf(c) {
  const st = c.start_trip;
  const first = st && st[0] === c.line && st[1] === c.tour ? st[2] : 0;
  return { map: c.map, date: c.date, free: !!c.free, line: c.free ? null : c.line, tour: c.free ? null : c.tour, first, legs: c.free || !c.composed ? [] : c.legs || [] };
}

let best = { key: '', pick: null, busy: false };

/** The bus that fits the duty best (None until it is worked out, on the game's side of the
 * launcher's commands: depot files are read). */
export function useBest() {
  const s = useStore();
  const [, redraw] = useState(0);
  const d = dutyOf(s.choice);
  const n = (s.vehicles || []).length;
  const key = JSON.stringify(d) + '|' + n;
  useEffect(() => {
    if (!d.map || !n || best.key === key) return;
    best = { key, pick: null, busy: true };
    const buses = S.vehicles.map((v) => [v.file, v.name]);
    call('hub_more_bus_best', { ...d, buses })
      .then((p) => {
        if (best.key === key) best = { key, pick: p || null, busy: false };
        redraw((x) => x + 1);
      })
      .catch(() => {
        if (best.key === key) best = { key, pick: null, busy: false };
      });
  }, [key]);
  return best.key === key ? best.pick : null;
}

/** "fits best", or on a free drive "drives here most" (`buspick::tiles`). */
export const bestWord = (p) => tr(p?.free ? 'drives here most' : 'fits best');

/** The grid's foot: which bus fits best (`buspick::foot`). */
export function bestFoot(p) {
  const v = p && (S.vehicles || []).find((x) => x.file === p.file);
  return v ? tr(p.free ? '%{bus} drives here most' : '%{bus} fits this duty best', { bus: v.name }) : '';
}

/** On the bus's sheet: whether it fits the duty best, and by which depot file. */
export function BestLine({ file }) {
  const p = useBest();
  if (!p || p.file !== file) return null;
  const line = tr(p.free ? 'Drives on this map most' : 'Fits this duty best');
  return html`<div class="hub-best"><${Icon} name="check_circle" size=${16} /><span>${p.depot ? `${line} · ${p.depot}` : line}</span></div>`;
}

const initials = (name) =>
  String(name || '')
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join('') || '?';

/** A tile's photo (`buspick::draw_tile`): the bus in its livery as the game-drawn launcher took
 * it, its initials on the gradient while there is none. */
export function TilePhoto({ file, paint, title }) {
  const [src, setSrc] = useState(null);
  useEffect(() => {
    let gone = false;
    setSrc(null);
    // (Omsi-Hub 6/6: one not there is photographed by the game program, `photos.js`)
    const leave = file ? wantPhoto(file, paint) : null;
    // (Omsi-Hub Bus3D: the page's own photo once it is taken, the older one meanwhile)
    if (file) photo(file, paint, (u) => !gone && setSrc(u)).then((u) => !gone && setSrc(u));
    return () => {
      gone = true;
      leave?.();
    };
  }, [file, paint]);
  return html`<span class=${'pic hub-mono' + (src ? ' shot' : '')} style=${'padding-bottom:0;height:150px' + (src ? `;background:url("${src}") center / cover no-repeat` : '')}>${!src && html`<i>${initials(title)}</i>`}</span>`;
}

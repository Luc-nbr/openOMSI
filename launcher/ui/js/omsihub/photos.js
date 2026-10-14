// The buses' photos on the tiles (the bus step's, the fleet's, the dealer's and Casual's), as our
// game-drawn launcher has them (`busphoto.rs`): the PNG kept under `~/.openomsi/cache/busphotos/`
// (`co_photo`) as a blob URL; a bus without one is photographed by the game program
// (`hub_photo`, Trello "Tauri 6/6") - one bus at a time, the tiles drawn last first, those no
// longer drawn left out - and shown once it is there. A bus that cannot be drawn keeps its
// initials; it is tried again after a while.

import { S } from '../store.js';
import { call } from '../api.js';

/** `file|paint` -> { p: the URL's promise, ok, at, failed }. */
const photos = new Map();
/** The photos waiting to be taken, the last asked for at the end, with how many tiles want each. */
const queue = [];
const wanted = new Map();
let busy = false;

function read(file, paint) {
  const tauri = window.__TAURI__;
  if (!tauri) return Promise.reject(new Error('no photo'));
  return tauri.core.invoke('call_bytes', { cmd: 'co_photo', args: { bus: file, livery: paint || '' } }).then((b) => URL.createObjectURL(new Blob([b], { type: 'image/png' })));
}

/** The next photo taken: the one a tile asked for last (`Photos::next`). */
async function work() {
  if (busy) return;
  busy = true;
  try {
    for (;;) {
      let k = queue.length - 1;
      while (k >= 0 && !(wanted.get(queue[k].key) > 0)) k--;
      if (k < 0) break;
      const job = queue.splice(k, 1)[0];
      const map = S.choice?.map || S.maps?.[0]?.file || '';
      try {
        if (!map || S.boot?.selftest) throw new Error('no map');
        await call('hub_photo', { bus: job.file, paint: job.paint || '', map });
        job.done(await read(job.file, job.paint));
      } catch {
        job.done(null);
      }
    }
  } finally {
    busy = false;
  }
}

/** The photo of `file` in `paint`: its URL once there is one (read, or taken now), else null. */
export function photo(file, paint) {
  if (!file) return Promise.resolve(null);
  const key = `${file}|${paint || ''}`;
  const had = photos.get(key);
  // (one that failed is tried again after a few minutes: the bus may have been changed)
  if (had && (had.ok || !had.failed || Date.now() - had.at < 300000)) return had.p;
  const e = { at: Date.now(), ok: false, failed: false };
  e.p = read(file, paint)
    .then((u) => ((e.ok = true), u))
    .catch(
      () =>
        new Promise((done) => {
          queue.push({ key, file, paint, done: (u) => ((e.ok = !!u), (e.failed = !u), (e.at = Date.now()), done(u)) });
          work();
        }),
    );
  photos.set(key, e);
  return e.p;
}

/** A tile that shows `file` in `paint` came (or went): the photos of tiles drawn are taken first,
 * and those of tiles gone are not taken at all. Returns the way to say it went. */
export function wantPhoto(file, paint) {
  const key = `${file}|${paint || ''}`;
  wanted.set(key, (wanted.get(key) || 0) + 1);
  // (asked for again by a tile drawn now: it goes to the end of the line, taken first)
  const k = queue.findIndex((j) => j.key === key);
  if (k >= 0) queue.push(queue.splice(k, 1)[0]);
  work();
  return () => wanted.set(key, Math.max(0, (wanted.get(key) || 1) - 1));
}

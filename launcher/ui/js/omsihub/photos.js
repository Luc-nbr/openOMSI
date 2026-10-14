// The buses' photos on the tiles (the bus step's, the fleet's, the dealer's and Casual's).
//
// Omsi-Hub's Bus3D (the old app's photos): taken by the page's own renderer
// (`bus3d/photo.js` - transparent, from the front corner, a soft shadow under the bus) and kept
// by the launcher (`b3d_photo`, `~/.openomsi/cache/busphotos/p3-*.png`) until the bus's files
// change. A bus without one shows the photo the game program took before, if there is one
// (`co_photo`), or its initials, and gets the new photo as soon as it is taken - one bus at a
// time, the tiles drawn last first, those no longer drawn left out. Where the page cannot draw
// (no WebGL2) the game program photographs the bus as before (`hub_photo`). A bus that cannot
// be drawn keeps its initials; it is tried again after a while.

import { S } from '../store.js';
import { call } from '../api.js';
import { fetchBytes } from './bus3d/urls.js';
import { takePhoto } from './bus3d/photo.js';

// (the tiles' own photos are shown whole, not cropped: `bus3d.css`, loaded here too - the company's
// pages show photos without a 3D view)
if (!document.querySelector('link[data-b3d-css]')) {
  const l = document.createElement('link');
  l.rel = 'stylesheet';
  l.href = 'omsihub/bus3d.css';
  l.dataset.b3dCss = '1';
  document.head.appendChild(l);
}

/** `file|paint` -> { p: the URL's promise, ok, at, failed, later: listeners of the new photo }. */
const photos = new Map();
/** The photos waiting to be taken, the last asked for at the end, with how many tiles want each. */
const queue = [];
const wanted = new Map();
let busy = false;
/** The page cannot draw: the game program takes the photos. */
let noWebgl = false;

const url = (bytes) => URL.createObjectURL(new Blob([bytes], { type: 'image/png' }));

/** A photo taken by our renderer and kept for the bus as it is now. */
function readNew(file, paint) {
  return fetchBytes('b3d_photo', { bus: file, paint: paint || '' }).then(url);
}

/** The photo the game program took (the game-drawn launcher's way). */
function readOld(file, paint) {
  const tauri = window.__TAURI__;
  if (!tauri) return Promise.reject(new Error('no photo'));
  return tauri.core.invoke('call_bytes', { cmd: 'co_photo', args: { bus: file, livery: paint || '' } }).then((b) => url(b));
}

/** The next photo taken: the one a tile asked for last. */
async function work() {
  if (busy) return;
  busy = true;
  try {
    for (;;) {
      let k = queue.length - 1;
      while (k >= 0 && !(wanted.get(queue[k].key) > 0)) k--;
      if (k < 0) break;
      const job = queue.splice(k, 1)[0];
      try {
        if (S.boot?.selftest && S.boot.selftest !== 'full') throw new Error('no photos in the quick self-check');
        if (!noWebgl) {
          try {
            job.done(url(await takePhoto(job.file, job.paint)));
            continue;
          } catch (x) {
            if (!x?.noWebgl) throw x;
            noWebgl = true;
          }
        }
        const map = S.choice?.map || S.maps?.[0]?.file || '';
        if (!map || S.boot?.selftest) throw new Error('no map');
        await call('hub_photo', { bus: job.file, paint: job.paint || '', map });
        job.done(await readOld(job.file, job.paint));
      } catch {
        job.done(null);
      }
    }
  } finally {
    busy = false;
  }
}

/**
 * The photo of `file` in `paint`: its URL once there is one, else null. A bus whose photo
 * is still to be taken resolves with the game's older photo (or null) at once, and `later`
 * hears the new one when it is there.
 */
export function photo(file, paint, later) {
  if (!file) return Promise.resolve(null);
  const key = `${file}|${paint || ''}`;
  const had = photos.get(key);
  // (one that failed is tried again after a few minutes: the bus may have been changed)
  if (had && (had.ok || !had.failed || Date.now() - had.at < 300000)) {
    if (later && !had.ok) had.later.add(later);
    return had.p;
  }
  const e = { at: Date.now(), ok: false, failed: false, later: new Set(later ? [later] : []) };
  e.p = readNew(file, paint)
    .then((u) => ((e.ok = true), u))
    .catch(() => {
      // to be taken: meanwhile the older photo, then the new one to whoever listens
      const taken = new Promise((done) => {
        queue.push({ key, file, paint, done });
        work();
      }).then((u) => {
        e.ok = !!u;
        e.failed = !u;
        e.at = Date.now();
        if (u) {
          e.p = Promise.resolve(u);
          for (const l of e.later) l(u);
        }
        e.later.clear();
        return u;
      });
      return readOld(file, paint).catch(() => taken);
    });
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

/** Forget what is known of a bus's photos (its livery was saved anew): asked again. */
export function forgetPhotos(file) {
  for (const k of [...photos.keys()]) if (k.startsWith(`${file}|`)) photos.delete(k);
}

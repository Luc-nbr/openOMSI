// The bus preview: drawn by the game's own renderer (the launcher's helper, `openomsi
// --launcher-service`), with the game's own showroom - the bus with its
// textures and every part, under the chosen time and weather.
//
// One request a picture, and the drag and the wheel ride along with it, so a turn costs one
// round trip. Nothing is asked while nothing moves.
//
// Omsi-Hub (Luc, Beta 53: "the 3D viewer is slow and lags"): every picture was full size, and a
// full-size JPEG took 40-90 ms to draw, read back and encode - about 20 pictures a second on a
// 1920 x 1080 window, fewer on a high-DPI screen. Now a picture is at most 0.4 megapixels while
// the mouse turns the bus (drawn larger to the view; about 50 a second), at most 1.6 while it
// eases out after letting go, so the sharp one is there at once, and full size when it stands
// still. The timings are kept in `window.hubPreviewStats` (the self-check reads them).

import { html, useEffect, useRef, useState } from '../vendor/preact-htm.js';
import { call, on } from './api.js';
import { t } from './i18n.js';
import { Icon } from './icons.js';
import { Spinner } from './ui.js';

async function frame(args) {
  const buf = new Uint8Array(await call('preview_frame', args));
  const n = new DataView(buf.buffer, buf.byteOffset).getUint32(0, true);
  const head = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + n)));
  const jpeg = buf.length > 4 + n ? new Blob([buf.subarray(4 + n)], { type: 'image/jpeg' }) : null;
  return { head, jpeg };
}

/** Omsi-Hub: how the preview keeps up - pictures while it turns and their round trip, the
 * sharp picture's time after letting go (`selftest`, the bus step's check). */
const stats = (window.hubPreviewStats = { turning: 0, turnMs: 0, sharpMs: null, last: null });
/** The most pixels of a picture while the mouse turns the bus, and while it eases out. */
const TURNING_PX = 400000;
const EASING_PX = 1600000;

const VIEWS = [['outside', 'Outside view', 'bus'], ['passenger', 'Passenger view', 'users'], ['driver', "Driver's view", 'person']];

export function BusPreview({ look, class: cls = '' }) {
  const host = useRef();
  const canvas = useRef();
  const [state, setState] = useState('loading');
  // what the loop sends next: the drag and zoom gathered since the last picture
  const r = useRef({ alive: true, force: true, dx: 0, dy: 0, zoom: 1, view: null, dragging: false, easing: false, wake: null });
  const [views, setViews] = useState('outside|');

  const wake = (force) => {
    const st = r.current;
    if (force) st.force = true;
    const w = st.wake;
    st.wake = null;
    w?.();
  };

  useEffect(() => {
    const st = r.current;
    st.alive = true;
    (async () => {
      while (st.alive) {
        const el = host.current;
        const k = Math.min(window.devicePixelRatio || 1, 2);
        const soft = st.dragging || st.easing;
        // (Omsi-Hub: smaller while it turns - the sharp one follows as it is let go)
        const cw = el?.clientWidth || 640;
        const ch = el?.clientHeight || 360;
        const most = st.dragging ? TURNING_PX : st.easing ? EASING_PX : Infinity;
        const s = Math.min(k, Math.sqrt(most / (cw * ch)));
        const w = Math.round(cw * s);
        const h = Math.round(ch * s);
        const args = { w, h, force: st.force, dx: st.dx, dy: st.dy, zoom: st.zoom, quality: st.dragging ? 70 : soft ? 82 : 90, view: st.view };
        const turning = st.dragging;
        const t0 = performance.now();
        st.view = null;
        st.dx = st.dy = 0;
        st.zoom = 1;
        st.force = false;
        let more = false;
        try {
          const { head, jpeg } = await frame(args);
          const ms = performance.now() - t0;
          if (jpeg && canvas.current) {
            const bmp = await createImageBitmap(jpeg);
            const c = canvas.current;
            if (c.width !== bmp.width || c.height !== bmp.height) Object.assign(c, { width: bmp.width, height: bmp.height });
            c.getContext('2d').drawImage(bmp, 0, 0);
            bmp.close();
            if (turning) {
              stats.turning += 1;
              stats.turnMs += ms;
            } else if (st.letGo) {
              stats.sharpMs = performance.now() - st.letGo;
              st.letGo = null;
            }
            stats.last = { ms, w: c.width, h: c.height, service: head.ms };
          }
          setViews(`${head.view || 'outside'}|${(head.views || []).join(',')}`);
          setState(head.failed || head.ok === false ? 'failed' : head.picture ? (head.busy ? 'switching' : 'ok') : 'loading');
          st.easing = !!head.moving;
          more = head.ok !== false && !head.failed && (head.busy || head.moving || !head.picture);
          if (!more && soft && !st.dragging) st.force = true;
        } catch {
          setState('failed');
        }
        if (!st.alive) break;
        if (more || st.force || st.view || st.dx !== 0 || st.dy !== 0 || st.zoom !== 1) await new Promise((res) => requestAnimationFrame(res));
        else await new Promise((res) => (st.wake = res));
      }
    })();
    const ro = new ResizeObserver(() => host.current?.clientWidth && wake(true));
    ro.observe(host.current);
    const off = on('games-ended', () => wake(true));
    return () => {
      st.alive = false;
      wake();
      ro.disconnect();
      off();
    };
  }, []);

  // another bus, paint, time or weather
  const key = JSON.stringify(look);
  useEffect(() => {
    if (!look?.bus) return;
    call('preview_look', look).then(() => wake(true));
  }, [key]);

  const last = useRef(null);
  const [now, can] = views.split('|');
  const available = can ? can.split(',') : [];
  const pick = (view) => {
    r.current.view = view;
    wake(true);
  };
  return html`<div class=${'viewer ' + cls} ref=${host}
      onPointerDown=${(e) => {
        last.current = { x: e.clientX, y: e.clientY };
        r.current.dragging = true;
        // (Omsi-Hub: a pointer the page made itself - the self-check's - has nothing to capture)
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {}
      }}
      onPointerMove=${(e) => {
        const p = last.current;
        if (!p) return;
        r.current.dx += e.clientX - p.x;
        r.current.dy += e.clientY - p.y;
        last.current = { x: e.clientX, y: e.clientY };
        wake();
      }}
      onPointerUp=${() => {
        last.current = null;
        r.current.dragging = false;
        // (Omsi-Hub: from here to the sharp picture)
        r.current.letGo = performance.now();
        wake(true);
      }}
      onWheel=${(e) => {
        e.preventDefault();
        r.current.zoom *= e.deltaY > 0 ? 1.08 : 1 / 1.08;
        wake();
      }}>
    <canvas ref=${canvas} style=${{ visibility: state === 'failed' || state === 'loading' ? 'hidden' : 'visible' }}></canvas>
    ${state === 'loading' && html`<div class="state"><${Spinner} /> ${t('Preparing the preview…')}</div>`}
    ${state === 'switching' && html`<div class="state corner"><${Spinner} /></div>`}
    ${state === 'failed' && html`<div class="state"><${Icon} name="bus" size=${28} /> ${t('No preview of this bus')}</div>`}
    ${state === 'ok' && html`<div class="hint">${t('Drag to turn, scroll to zoom')}</div>`}
    ${state !== 'failed' && available.length > 1 && html`<div class="preview-views" onPointerDown=${(e) => e.stopPropagation()} onWheel=${(e) => e.stopPropagation()}>
      ${VIEWS.map(([id, label, icon]) => html`<button type="button" class=${id === now ? 'on' : ''} aria-pressed=${id === now} disabled=${!available.includes(id)} title=${t(label)} onClick=${() => pick(id)}>
        <${Icon} name=${icon} size=${16} /><span>${t(label)}</span>
      </button>`)}
    </div>`}
  </div>`;
}

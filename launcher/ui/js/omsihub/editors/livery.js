// The livery studio (`crates/omsi-launcher-ui/src/livery`): our game-drawn launcher's own studio,
// run by the game program in the launcher service (`hub_service::studio`) and shown here over the
// whole window - its pictures drawn with the game's renderer, the bus on it painted there. What
// the mouse and the keys do on this page goes to it, as the window gives it to our game-drawn
// launcher; it answers with its next picture when it changed.
//
// Why the studio itself and not panels of this page's own around a picture of the bus: it is
// worked on the bus - layers picked, dragged, turned and sized on it, the brush and the pen
// painting on it, guide lines over it - which needs the bus's geometry and camera, kept there.
// Its texts, its layout, its look in either light and its behaviour are then the game-drawn
// launcher's own, without a second studio to keep the same as the first.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call, onDrop } from '../../api.js';
import { choose } from '../../data.js';
import { tr } from './kit.js';

/** A request of the studio: its head (JSON) and its picture (a PNG, or none when unchanged). */
export async function studio(args) {
  const buf = new Uint8Array(await call('hub_studio', args));
  const n = new DataView(buf.buffer, buf.byteOffset).getUint32(0, true);
  const head = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + n)));
  if (head.ok === false) throw new Error(head.error || 'the livery studio failed');
  const png = buf.length > 4 + n ? buf.subarray(4 + n) : null;
  return { head, png };
}

/** The modifier keys held with an event. */
const mods = (e) => ({ s: e.shiftKey, c: e.ctrlKey, a: e.altKey, w: e.metaKey });

/** Out of the studio (`livery::leave`): to the bus company it painted for, else where it was
 * opened from (the editor). */
function leave(to) {
  const o = S.ed.liveryOpen || {};
  const page = o.back || (to === 'company' ? 'drive' : 'editor');
  set({ page, hub: { ...S.hub, palette: false } });
}

export function LiveryPage() {
  const s = useStore();
  const o = s.ed.liveryOpen || {};
  const cv = useRef();
  const [state, setState] = useState({ ready: false, error: '' });

  useEffect(() => {
    const el = cv.current;
    if (!el) return;
    let alive = true;
    let events = [];
    let force = true;
    let timer = 0;
    const g = el.getContext('2d');
    // (the window's pixels: the studio draws at the screen's own size)
    const size = () => {
      const r = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      return { w: Math.max(64, Math.round(r.width * dpr)), h: Math.max(64, Math.round(r.height * dpr)), dpr };
    };
    // (where the mouse is in the studio's picture)
    const at = (e) => {
      const r = el.getBoundingClientRect();
      const { w, h } = size();
      return { x: ((e.clientX - r.left) / (r.width || 1)) * w, y: ((e.clientY - r.top) / (r.height || 1)) * h };
    };
    const push = (ev) => {
      events.push(ev);
      if (!timer) return;
      clearTimeout(timer);
      timer = 0;
      tick();
    };
    // the mouse
    const onMove = (e) => push({ t: 'move', ...at(e), m: mods(e) });
    const onDown = (e) => {
      el.focus();
      el.setPointerCapture?.(e.pointerId);
      push({ t: 'move', ...at(e), m: mods(e) });
      push({ t: 'down', b: e.button, m: mods(e) });
      e.preventDefault();
    };
    const onUp = (e) => {
      push({ t: 'move', ...at(e), m: mods(e) });
      push({ t: 'up', b: e.button, m: mods(e) });
    };
    const onLeave = (e) => !e.buttons && push({ t: 'leave', m: mods(e) });
    const onWheel = (e) => {
      e.preventDefault();
      // (a notch is a line, as the window gives it; a touchpad's pixels forty to a line)
      const k = e.deltaMode === 1 ? 1 : e.deltaMode === 2 ? 3 : 1 / 100;
      push({ t: 'wheel', x: -e.deltaX * k, y: -e.deltaY * k, m: mods(e) });
    };
    const noMenu = (e) => e.preventDefault();
    // the keys (the studio's: none of the window's own shortcuts while it is open)
    const onKey = (down) => (e) => {
      if (e.isComposing) return;
      e.preventDefault();
      push({ t: 'key', code: e.code, down, text: down && e.key.length === 1 ? e.key : '', m: mods(e) });
    };
    const keyDown = onKey(true);
    const keyUp = onKey(false);
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerdown', onDown);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('pointerleave', onLeave);
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('contextmenu', noMenu);
    addEventListener('keydown', keyDown);
    addEventListener('keyup', keyUp);
    // a picture dropped on the window is the studio's
    const stopDrop = onDrop((paths) => paths.forEach((path) => push({ t: 'drop', path, m: {} })));

    // the studio's pictures, one asked for after the other: at once while something happens,
    // a little later while nothing does
    const tick = async () => {
      timer = 0;
      if (!alive) return;
      const { w, h, dpr } = size();
      const sent = events;
      events = [];
      let changed = false;
      try {
        const { head, png } = await studio({ op: 'frame', w, h, dpr, events: sent, force });
        force = false;
        if (!alive) return;
        if (png) {
          const pic = await createImageBitmap(new Blob([png], { type: 'image/png' }));
          if (el.width !== pic.width || el.height !== pic.height) Object.assign(el, { width: pic.width, height: pic.height });
          g.drawImage(pic, 0, 0);
          pic.close?.();
          changed = true;
        }
        el.style.cursor = head.cursor || 'default';
        if (head.choice && S.choice?.bus === head.choice.bus) choose({ paint: head.choice.paint });
        if (head.company_changed) o.onCompanyChanged?.(head.company_changed);
        if (head.left) {
          alive = false;
          leave(head.left);
          return;
        }
      } catch (x) {
        if (alive) setState({ ready: true, error: x.message });
        return;
      }
      if (events.length || changed) requestAnimationFrame(tick);
      else timer = setTimeout(tick, 50);
    };

    setState({ ready: false, error: '' });
    studio({ op: 'open', bus: o.bus === 'chosen' ? S.choice?.bus || '' : o.bus || '', paint: o.bus === 'chosen' ? S.choice?.paint || '' : o.paint || '', company: o.company || '', vehicle: o.vehicle ?? null })
      .then(() => {
        if (!alive) return;
        setState({ ready: true, error: '' });
        tick();
      })
      .catch((x) => alive && setState({ ready: true, error: x.message }));
    return () => {
      alive = false;
      clearTimeout(timer);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerdown', onDown);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('pointerleave', onLeave);
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('contextmenu', noMenu);
      removeEventListener('keydown', keyDown);
      removeEventListener('keyup', keyUp);
      stopDrop();
    };
  }, [o.at]);

  return html`<div class="ed-studio">
    <canvas ref=${cv} tabindex="0"></canvas>
    ${!state.ready && html`<div class="ed-studio-note">${tr('Loading...')}</div>`}
    ${state.error && html`<div class="ed-studio-note error"><p>${state.error}</p><button type="button" class="hub-btn" onClick=${() => leave('back')}>${tr('Back')}</button></div>`}
  </div>`;
}

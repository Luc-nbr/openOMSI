// The bus in 3D on the page (Omsi-Hub's Bus3D, the old app's `BusViewer.tsx`): drawn by the
// page's own renderer (`link.js`, `engine.js`) from the bus the launcher read as the game reads
// it (`b3d_open`), turned by the mouse at the frame rate of the screen - no picture streamed
// from the game program any more (the old `preview.js`, kept for the pages of openOMSI's own and
// as the fall-back without WebGL2).
//
// Never an empty frame: until the 3D picture stands the bus's photo is there (or its spinner),
// and the 3D picture takes over once it is sharp, after 1.5 s, or as soon as the mouse moves.
// The inside views (the old viewer had none): the driver's seat and a passenger's, from the
// `.bus`'s cameras, looked round by dragging.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { call } from '../../api.js';
import { t } from '../../i18n.js';
import { Icon } from '../../icons.js';
import { Spinner } from '../../ui.js';
import { BusPreview } from '../../preview.js';
import { photo } from '../photos.js';
import { Link } from './link.js';

const now = () => performance.timeOrigin + performance.now();

// (the view's own look, loaded with it)
if (!document.querySelector('link[data-b3d-css]')) {
  const l = document.createElement('link');
  l.rel = 'stylesheet';
  l.href = 'omsihub/bus3d.css';
  l.dataset.b3dCss = '1';
  document.head.appendChild(l);
}

/** What the page's self-check and the report read: the last bus opened and its times. */
const stats = (window.hubBus3dStats = { opened: null, state: null, frames: 0, sharp: null, mode: null, error: null });

let envAsked = false;
function sendEnv() {
  if (envAsked) return;
  envAsked = true;
  call('b3d_env')
    .then((env) => Link.get().env(env))
    .catch(() => Link.get().env({ cloudSize: 2000 }));
}

/** DPR = min(the device's, 2), and never more than 1920 x 1080 drawing pixels. */
function drawSize(w, h, light) {
  let dpr = light ? 1 : Math.min(window.devicePixelRatio || 1, 2);
  const most = 1920 * 1080;
  if (w * h * dpr * dpr > most) dpr = Math.sqrt(most / Math.max(1, w * h));
  return { w: Math.max(2, Math.round(w * dpr)), h: Math.max(2, Math.round(h * dpr)), dpr };
}

const VIEWS = [['outside', 'Outside view', 'bus'], ['passenger', 'Passenger view', 'users'], ['driver', "Driver's view", 'person']];

/** The open bus of a viewer, by pack: its manifest and the look asked for last. */
async function open(bus, paint, options) {
  const args = { bus, paint: paint || '' };
  if (options) args.options = options;
  return call('b3d_open', args);
}

/**
 * The bus `bus` in livery `paint` in 3D. `options`: the bus options it wears (`[[var, value]]`;
 * none given: the ones chosen for it on the bus step, read by the launcher as the game gets
 * them). `optionsRev`: changed when those were chosen anew - the look is asked for again,
 * the model stays. `turntable`: the dealer's slow turn after 6 s without input.
 * `onHandle`: the viewer itself (the livery studio, the self-check). `studio`: the livery
 * studio's controls (the left button is its tool, the right turns).
 */
export function Bus3D({ bus, paint, options, optionsRev = 0, class: cls = '', turntable = false, onHandle, onManifest, studio, hint = true, views = true, light = false }) {
  const host = useRef();
  const canvas = useRef();
  const handle = useRef();
  const cur = useRef({ bus: '', paint: '', ask: 0 });
  const studioRef = useRef(studio);
  studioRef.current = studio;
  const [state, setState] = useState('loading');
  const [shown, setShown] = useState(false);
  const [takeOver, setTakeOver] = useState(false);
  const [pic, setPic] = useState(null);
  const [manifest, setManifest] = useState(null);
  const [view, setView] = useState('outside');
  const [fallback, setFallback] = useState(false);
  const [why, setWhy] = useState('');
  const overClock = useRef();

  // sign on with the renderer
  useEffect(() => {
    const link = Link.get();
    sendEnv();
    const h = link.join(canvas.current, {
      firstFrame: () => {
        setShown(true);
        clearTimeout(overClock.current);
        overClock.current = setTimeout(() => setTakeOver(true), 1500);
      },
      frame: () => {
        stats.frames++;
      },
      state: (s) => {
        stats.state = s;
        if (s.phase === 'sharp') {
          setTakeOver(true);
          stats.sharp = s;
          if (s.measure) stats.measure = s.measure;
        }
      },
      error: (e) => {
        stats.error = e;
        setWhy(e.detail || e.reason);
        setState('failed');
      },
    });
    handle.current = h;
    // (the self-check measures through it; not part of the stats' JSON)
    Object.defineProperty(stats, 'handle', { value: h, configurable: true, writable: true, enumerable: false });
    onHandle?.(h);
    link.ready.then((r) => {
      stats.mode = r.mode || 'none';
      if (!r.webgl) setFallback(true);
    });
    const el = host.current;
    const fit = () => {
      const r = el.getBoundingClientRect();
      const m = drawSize(r.width, r.height, light);
      if (canvas.current.width !== m.w || canvas.current.height !== m.h) Object.assign(canvas.current, { width: m.w, height: m.h });
      h.size(m.w, m.h, m.dpr);
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    let dprWatch;
    const onDpr = () => {
      fit();
      watchDpr();
    };
    const watchDpr = () => {
      dprWatch?.removeEventListener('change', onDpr);
      dprWatch = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      dprWatch.addEventListener('change', onDpr);
    };
    watchDpr();
    return () => {
      ro.disconnect();
      dprWatch?.removeEventListener('change', onDpr);
      clearTimeout(overClock.current);
      h.gone();
    };
  }, []);

  // the bus, or only its livery
  const optKey = JSON.stringify(options || null);
  useEffect(() => {
    if (!bus) return;
    const c = cur.current;
    const ask = ++c.ask;
    const h = handle.current;
    const t0 = now();
    const sameBus = c.bus === bus && c.manifest;
    c.paint = paint || '';
    if (!sameBus) {
      c.bus = bus;
      c.manifest = null;
      setShown(false);
      setTakeOver(false);
      setState('loading');
      setView('outside');
      photo(bus, paint).then((u) => c.ask === ask && setPic(u));
    }
    (async () => {
      try {
        if (sameBus) {
          const args = { bus, paint: paint || '' };
          if (options) args.options = options;
          const r = await call('b3d_look', args);
          if (c.ask !== ask) return;
          if (r.pack !== c.manifest.pack) throw new Error('changed');
          h.look(r.look, t0);
          return;
        }
        const r = await open(bus, paint, options);
        if (c.ask !== ask) return;
        stats.opened = { bus, paint: paint || '', ms: Math.round(now() - t0), launcherMs: r.ms, cache: r.cache, triangles: r.manifest.counts.triangles, pack: r.manifest.pack, bytes: r.manifest.bytes };
        c.manifest = r.manifest;
        setManifest(r.manifest);
        onManifest?.(r.manifest);
        setState('ok');
        h.load(r.manifest, r.look, r.cache ? 'cache' : 'new', t0);
      } catch (x) {
        if (c.ask !== ask) return;
        if (sameBus) {
          // (the bus changed on disk meanwhile: opened anew)
          c.manifest = null;
          c.bus = '';
          return;
        }
        setWhy(x.message || String(x));
        setState('failed');
      }
    })();
  }, [bus, paint, optKey, optionsRev]);

  useEffect(() => handle.current?.turntable(turntable), [turntable]);

  const input = (i) => {
    setTakeOver(true);
    handle.current?.input(i);
  };
  const pick = (v) => {
    setView(v);
    handle.current?.view(v);
  };

  // the mouse: drag turns (the studio's left button is its tool), two fingers zoom
  const pointers = useRef(new Map());
  const pinch = useRef(0);
  const mode = useRef();
  const ndc = (e) => {
    const r = canvas.current.getBoundingClientRect();
    return [((e.clientX - r.left) / Math.max(1, r.width)) * 2 - 1, 1 - ((e.clientY - r.top) / Math.max(1, r.height)) * 2];
  };
  const down = (e) => {
    const st = studioRef.current;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      // (a pointer the page made itself - the self-check's - has nothing to capture)
    }
    e.currentTarget.focus?.();
    if (st) {
      e.preventDefault();
      setTakeOver(true);
      if (e.button === 2) mode.current = 'turn';
      else if (e.button === 1 || (e.button === 0 && e.shiftKey)) mode.current = 'pan';
      else if (e.button === 0) mode.current = st.down(ndc(e), e) ? 'tool' : undefined;
      else return;
      pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
      return;
    }
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      pinch.current = Math.hypot(a.x - b.x, a.y - b.y);
    }
  };
  const move = (e) => {
    const was = pointers.current.get(e.pointerId);
    const st = studioRef.current;
    if (st) {
      if (!was) return st.hover?.(ndc(e), e);
      const p = { x: e.clientX, y: e.clientY };
      pointers.current.set(e.pointerId, p);
      if (mode.current === 'turn') input({ kind: 'drag', dx: p.x - was.x, dy: p.y - was.y });
      else if (mode.current === 'pan') {
        const r = canvas.current.getBoundingClientRect();
        const f = (canvas.current.height || 1) / Math.max(1, r.height);
        input({ kind: 'pan', dx: (p.x - was.x) * f, dy: (p.y - was.y) * f });
      } else if (mode.current === 'tool') st.move(ndc(e), e);
      return;
    }
    if (!was) return;
    const p = { x: e.clientX, y: e.clientY };
    pointers.current.set(e.pointerId, p);
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinch.current > 0 && d > 0) input({ kind: 'zoom', factor: pinch.current / d });
      pinch.current = d;
      return;
    }
    input({ kind: 'drag', dx: p.x - was.x, dy: p.y - was.y });
  };
  const up = (e) => {
    const was = pointers.current.has(e.pointerId);
    pointers.current.delete(e.pointerId);
    if (pointers.current.size < 2) pinch.current = 0;
    const st = studioRef.current;
    if (st && was && mode.current === 'tool') st.up(ndc(e), e);
    if (st) mode.current = undefined;
  };
  // the wheel zooms over the picture (not passive: the page would scroll along)
  useEffect(() => {
    const el = host.current;
    const wheel = (e) => {
      e.preventDefault();
      const factor = e.deltaY > 0 ? 1.08 : 1 / 1.08;
      if (studioRef.current) {
        const [x, y] = ndc(e);
        input({ kind: 'zoomAt', factor, x, y });
      } else input({ kind: 'zoom', factor });
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  }, []);
  const key = (e) => {
    if (studioRef.current || e.ctrlKey || e.altKey || e.metaKey) return;
    const k = e.key;
    const stand = (s) => input({ kind: 'stand', stand: s });
    if (k === 'ArrowLeft') input({ kind: 'turn', deg: 15 });
    else if (k === 'ArrowRight') input({ kind: 'turn', deg: -15 });
    else if (k === 'ArrowUp') input({ kind: 'tilt', deg: 5 });
    else if (k === 'ArrowDown') input({ kind: 'tilt', deg: -5 });
    else if (k === '+' || k === '=') input({ kind: 'zoom', factor: 1 / 1.08 });
    else if (k === '-' || k === '_') input({ kind: 'zoom', factor: 1.08 });
    else if (k === '1') stand('front');
    else if (k === '2') stand('side');
    else if (k === '3') stand('rear');
    else if (k === '4') stand('corner');
    else if (k === '0' || k === 'Home') stand('back');
    else return;
    if (view !== 'outside') setView('outside');
    e.preventDefault();
  };

  if (fallback) return html`<${BusPreview} class=${cls} look=${{ bus, paint: paint || '' }} />`;
  const can = ['outside', ...(manifest?.passenger ? ['passenger'] : []), ...(manifest?.driver ? ['driver'] : [])];
  const show3d = shown && state !== 'failed' && (!pic || takeOver);
  return html`<div class=${'viewer b3d-viewer ' + cls} ref=${host} data-state=${state}>
    ${pic && !show3d && state !== 'failed' && html`<img class="b3d-photo" src=${pic} alt="" draggable="false" />`}
    <canvas ref=${canvas} tabindex="0" class=${show3d ? 'b3d-on' : ''} style=${studio?.cursor ? { cursor: studio.cursor } : undefined}
      onPointerDown=${down} onPointerMove=${move} onPointerUp=${up} onPointerCancel=${up}
      onPointerLeave=${() => studioRef.current?.hover?.(undefined)}
      onContextMenu=${(e) => e.preventDefault()}
      onDblClick=${(e) => (studioRef.current?.double ? studioRef.current.double(ndc(e)) : input({ kind: 'stand', stand: 'back' }))}
      onKeyDown=${key}></canvas>
    ${state === 'loading' && !pic && html`<div class="state"><${Spinner} /> ${t('Preparing the preview…')}</div>`}
    ${state !== 'failed' && !show3d && pic && html`<div class="state corner"><${Spinner} /></div>`}
    ${state === 'failed' && html`<div class="state" title=${why}><${Icon} name="bus" size=${28} /> ${t('No preview of this bus')}</div>`}
    ${hint && show3d && !studio && html`<div class="hint">${t(view === 'outside' ? 'Drag to turn, scroll to zoom' : 'Drag to look round, scroll to zoom')}</div>`}
    ${views && !studio && state !== 'failed' && can.length > 1 && html`<div class="preview-views" onPointerDown=${(e) => e.stopPropagation()} onWheel=${(e) => e.stopPropagation()}>
      ${VIEWS.map(([id, label, icon]) => html`<button type="button" class=${id === view ? 'on' : ''} aria-pressed=${id === view} disabled=${!can.includes(id)} title=${t(label)} onClick=${() => pick(id)}>
        <${Icon} name=${icon} size=${16} /><span>${t(label)}</span>
      </button>`)}
    </div>`}
  </div>`;
}

// The page's link to its 3D renderer (the old app's `bus3d/verbinding.ts`).
//
// One for the page: the first viewer starts the renderer, which stays with its context and
// textures while the page lives. It runs in a worker on an OffscreenCanvas; where that cannot
// draw (no WebGL2 in a worker - an older WebKit on the Mac), the same engine runs on the
// page's own thread, on an OffscreenCanvas or else a canvas of the page. A viewer signs on with
// a `<canvas>` and gets its pictures there (`bitmaprenderer`); mounting and unmounting it is
// only signing on and off: the context stays one.

import { fetchBytes } from './urls.js';

let only;

export class Link {
  static get() {
    only ??= new Link();
    return only;
  }

  constructor() {
    this.viewers = new Map();
    this.asks = new Map();
    this.nextViewer = 0;
    this.nextAsk = 0;
    this.nextLoad = 0;
    this.envSent = false;
    this.queue = [];
    this.target = undefined;
    this.cameraWatch = new Map();
    /** The livery studio's messages from the renderer (`lak/studio.js` sets it). */
    this.onStudio = undefined;
    this.ready = new Promise((ok) => (this.readyOk = ok));
    this.startWorker();
  }

  startWorker() {
    let w;
    try {
      if (typeof OffscreenCanvas === 'undefined' || typeof Worker === 'undefined') throw new Error('no OffscreenCanvas');
      w = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
      this.worker = w;
    } catch {
      this.startPage();
      return;
    }
    const failed = (detail) => {
      w.terminate();
      this.startPage(detail);
    };
    w.onerror = (e) => failed(e.message || 'the worker failed');
    w.onmessage = (e) => {
      const m = e.data;
      if (m.kind === 'ready' && !this.target) {
        if (!m.webgl) return failed(m.detail);
        this.target = { post: (x, t) => w.postMessage(x, t || []) };
        this.flush();
        this.readyOk({ webgl: true, info: m.info, mode: 'worker' });
        return;
      }
      this.receive(m);
    };
  }

  /** The engine on the page's own thread. */
  async startPage(why) {
    try {
      const { Engine } = await import('./engine.js');
      const engine = new Engine(
        (m) => queueMicrotask(() => this.receive(m)),
        () => {
          if (typeof OffscreenCanvas !== 'undefined') {
            const c = new OffscreenCanvas(16, 16);
            // (an OffscreenCanvas without WebGL2: a canvas of the page)
            if (c.getContext('webgl2')) return new OffscreenCanvas(16, 16);
          }
          return Object.assign(document.createElement('canvas'), { width: 16, height: 16 });
        },
        (f) => requestAnimationFrame(f),
      );
      if (!engine.start()) {
        this.readyOk({ webgl: false, detail: why || 'no WebGL2' });
        return;
      }
      this.engine = engine;
      this.target = { post: (x) => engine.handle(x) };
      this.flush();
      this.readyOk({ webgl: true, info: engine.info(), mode: 'page', why });
    } catch (x) {
      this.readyOk({ webgl: false, detail: String(x?.message || x) });
    }
  }

  flush() {
    const q = this.queue;
    this.queue = [];
    for (const [m, t] of q) this.target.post(m, t);
  }

  send(m, transfer) {
    if (this.target) this.target.post(m, transfer);
    else this.queue.push([m, transfer]);
  }

  ask(make) {
    const ask = ++this.nextAsk;
    return new Promise((ok) => {
      this.asks.set(ask, ok);
      this.send(make(ask));
    });
  }

  async loadStudio() {
    if (this.engine && !this.engine.studio) {
      const { attach } = await import('./lak/canvas.js');
      attach(this.engine);
    }
    this.send({ kind: 'lakLoad' });
  }

  receive(m) {
    const k = m.kind;
    if (k === 'frame') {
      const v = this.viewers.get(m.viewer);
      // a picture of the bus before (still under way when the next was asked for): not shown
      if (v && m.load >= v.load) {
        if (m.bitmap) {
          v.ctx ??= v.canvas.getContext('bitmaprenderer');
          v.ctx.transferFromImageBitmap(m.bitmap);
        } else if (m.canvas) {
          v.ctx ??= v.canvas.getContext('2d');
          v.ctx.drawImage(m.canvas, 0, 0, v.canvas.width, v.canvas.height);
        }
        v.frames++;
        if (v.frames === 1) v.listen.firstFrame?.();
        v.listen.frame?.(m);
        if (m.cam) {
          const c = new Float32Array(m.cam);
          v.listen.camera?.(c);
          for (const l of this.cameraWatch.get(m.viewer) || []) l(c);
        }
      } else m.bitmap?.close();
      this.send({ kind: 'seen', viewer: m.viewer });
    } else if (k === 'state') this.viewers.get(m.viewer)?.listen.state?.(m);
    else if (k === 'error') this.viewers.get(m.viewer)?.listen.error?.(m);
    else if (k === 'answer') {
      const ok = this.asks.get(m.ask);
      this.asks.delete(m.ask);
      ok?.(m.result);
    } else if (k === 'fetch') {
      fetchBytes(m.cmd, m.args).then(
        (bytes) => this.send({ kind: 'fetched', ask: m.ask, bytes }, [bytes]),
        (x) => this.send({ kind: 'fetched', ask: m.ask, error: String(x?.message || x) }),
      );
    } else if (k.startsWith('lak')) this.onStudio?.(m);
  }

  /** Let the renderer go (the photos' own, after a minute without photos). */
  close() {
    this.worker?.terminate();
    this.worker = undefined;
    this.engine = undefined;
    this.target = { post: () => {} };
    this.viewers.clear();
  }

  /** OMSI's sky and clouds: once for the page. */
  env(env) {
    if (this.envSent) return;
    this.envSent = true;
    this.send({ kind: 'env', env });
  }

  watchCamera(viewer, l) {
    const set = this.cameraWatch.get(viewer) || new Set();
    set.add(l);
    this.cameraWatch.set(viewer, set);
    return () => set.delete(l);
  }

  /** A viewer signs on with its canvas; `listen` hears its frames, state and errors. */
  join(canvas, listen) {
    const id = ++this.nextViewer;
    const v = { canvas, ctx: undefined, listen, frames: 0, load: 0 };
    this.viewers.set(id, v);
    const me = this;
    return {
      id,
      size: (w, h, dpr) => me.send({ kind: 'viewer', viewer: id, w, h, dpr }),
      load(manifest, look, source, t0, opts = {}) {
        const load = ++me.nextLoad;
        v.frames = 0;
        v.load = load;
        me.send({ kind: 'bus', viewer: id, load, manifest, look, source, t0, ...opts });
        return load;
      },
      look: (look, t0) => me.send({ kind: 'look', viewer: id, look, t0 }),
      input: (input) => me.send({ kind: 'input', viewer: id, input }),
      view: (view) => me.send({ kind: 'view', viewer: id, view }),
      pause(on, release) {
        if (!on) v.frames = 0;
        me.send({ kind: 'pause', viewer: id, on, release });
      },
      turntable: (on) => me.send({ kind: 'turntable', viewer: id, on }),
      shot: (shot) => me.ask((ask) => ({ kind: 'shot', ask, viewer: id, shot })),
      measure: (what, frames) => me.ask((ask) => ({ kind: 'measure', ask, viewer: id, what, frames })),
      send: (m, t) => me.send({ ...m, viewer: id }, t),
      ask: (m) => me.ask((ask) => ({ ...m, ask, viewer: id })),
      gone() {
        // the last picture let go at once, not when the collector comes (video memory)
        try {
          if (v.ctx?.transferFromImageBitmap) v.ctx.transferFromImageBitmap(null);
        } catch {
          // gone already
        }
        me.viewers.delete(id);
        me.send({ kind: 'gone', viewer: id });
      },
    };
  }
}

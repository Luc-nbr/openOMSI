// The renderer of the 3D view (Omsi-Hub's Bus3D, the old app's `bus3d/werker.ts`).
//
// One for the window, with one canvas of its own and one WebGL2 context that stays while the
// window lives. Normally it runs in a worker (`worker.js`) on an OffscreenCanvas; every viewer
// of the page has a plain `<canvas>` with a `bitmaprenderer`: the engine draws at that
// viewer's size, `transferToImageBitmap()` and sends the picture. So no canvas binds the
// context, and changing the bus in a viewer is only signing on and off. Where a worker cannot
// draw (no OffscreenCanvas WebGL2 in workers: an older WebKit), the same engine runs on the
// page's thread (`link.js`) and hands its pictures over the same way.
//
// It draws only when something changes (the camera, a texture come in, another size or
// livery), and with at most two pictures under way: what a viewer has not shown yet is not
// caught up but replaced.

import { Camera } from './camera.js';
import { LIGHT, Painter } from './draw.js';
import { fetchBytes } from './urls.js';

const now = () => performance.timeOrigin + performance.now();
const TURNTABLE_WAIT = 6000;

function quantile(list, q) {
  if (!list.length) return 0;
  const s = [...list].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * (s.length - 1)))];
}

export class Engine {
  /**
   * `post(message, transfer)` answers the page; `makeCanvas()` gives the canvas to draw on (an
   * OffscreenCanvas, or the page's own); `raf` schedules a picture.
   */
  constructor(post, makeCanvas, raf) {
    this.post = post;
    this.makeCanvas = makeCanvas;
    this.raf = raf;
    this.viewers = new Map();
    this.active = undefined;
    this.lost = false;
    this.planned = false;
    this.lastTick = 0;
    this.frameTimes = [];
    this.loading = { nr: 0, t0: 0, marks: {}, lastState: 0 };
    this.geometryBusy = false;
    this.shooting = false;
    this.env = undefined;
    this.asks = new Map();
    this.askNext = 0;
    this.proxy = false;
    /** The livery studio's canvas, if one runs (`lak/canvas.js` hooks itself in). */
    this.studio = undefined;
  }

  /** Make the context; false when there is no WebGL2. */
  start() {
    this.canvas = this.makeCanvas();
    const c = this.canvas;
    c.addEventListener?.('webglcontextlost', (e) => {
      e.preventDefault();
      if (c !== this.canvas) return;
      this.lost = true;
      if (!this.wilful) this.post({ kind: 'error', viewer: this.active?.id ?? 0, load: this.loading.nr, reason: 'context-lost' });
    });
    c.addEventListener?.('webglcontextrestored', () => {
      if (c !== this.canvas) return;
      this.lost = false;
      this.wilful = false;
      try {
        this.makePainter();
        if (this.loading.msg) this.loadBus(this.loading.msg);
      } catch (x) {
        this.post({ kind: 'error', viewer: this.active?.id ?? 0, load: this.loading.nr, reason: 'failed', detail: String(x) });
      }
    });
    // no MSAA and no depth on the canvas itself: the painter has an MSAA framebuffer of its own
    this.gl = c.getContext('webgl2', { antialias: false, alpha: false, depth: false, stencil: false, premultipliedAlpha: true, preserveDrawingBuffer: false, powerPreference: 'high-performance' });
    if (!this.gl) return false;
    this.loseExt = this.gl.getExtension('WEBGL_lose_context');
    this.makePainter();
    return true;
  }

  makePainter() {
    const gl = this.gl;
    const s3tc = gl.getExtension('WEBGL_compressed_texture_s3tc');
    const srgb = gl.getExtension('WEBGL_compressed_texture_s3tc_srgb');
    const aniso = gl.getExtension('EXT_texture_filter_anisotropic');
    gl.getExtension('KHR_parallel_shader_compile');
    this.can = { s3tc: !!s3tc, s3tcSrgb: !!(s3tc && srgb), aniso: aniso ? gl.getParameter(aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT) : 1, anisoExt: aniso || undefined };
    this.painter = new Painter(gl, this.can, () => this.plan());
    this.painter.textures.fetch = (cmd, args) => this.fetch(cmd, args);
    if (this.env) this.painter.setEnv(this.env);
    this.studio?.restart?.();
  }

  info() {
    const gl = this.gl;
    const dbg = gl.getExtension('WEBGL_debug_renderer_info');
    return { renderer: dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER), samples: Math.min(4, gl.getParameter(gl.MAX_SAMPLES)), s3tc: this.can.s3tc, s3tcSrgb: this.can.s3tcSrgb, aniso: this.can.aniso };
  }

  /** The bytes of a byte command: fetched here, or by the page when this side cannot. */
  async fetch(cmd, args) {
    if (!this.proxy) {
      try {
        return await fetchBytes(cmd, args);
      } catch (x) {
        // (a worker whose fetch of the window's protocol fails outright: the page fetches)
        if (!(x instanceof TypeError)) throw x;
        this.proxy = true;
      }
    }
    return new Promise((ok, fail) => {
      const ask = ++this.askNext;
      this.asks.set(ask, { ok, fail });
      this.post({ kind: 'fetch', ask, cmd, args });
    });
  }

  /** A pack's bytes: a large one in parts fetched at once (one stream of the window's protocol
   * is the slow part: 38 MB took 300-480 ms as one, about 130 MB/s). */
  async fetchPack(id, bytes) {
    const PART = 6 * 1024 * 1024;
    if (!(bytes > PART * 1.5)) return this.fetch('b3d_pack', { id });
    const n = Math.min(8, Math.ceil(bytes / PART));
    const size = Math.ceil(bytes / n);
    const out = new Uint8Array(bytes);
    await Promise.all(
      Array.from({ length: n }, async (_, k) => {
        const from = k * size;
        const to = Math.min(bytes, from + size);
        out.set(new Uint8Array(await this.fetch('b3d_pack', { id, from, to })), from);
      }),
    );
    return out.buffer;
  }

  viewer(id) {
    let v = this.viewers.get(id);
    if (!v) {
      v = { id, w: 0, h: 0, dpr: 1, camera: new Camera(), inFlight: 0, paused: false, turntable: false, lastInput: 0, view: 'outside' };
      this.viewers.set(id, v);
    }
    return v;
  }

  /** The texture budget of a bus: 160 MB (96 while a game runs), less the sky and clouds, and
   * never so much that the view with its geometry and targets goes over 300 MB. */
  budget(m) {
    const MB = 1024 * 1024;
    const loose = this.painter?.textures.looseTotal() ?? 0;
    const v = this.viewers.get(m.viewer);
    const fixed = this.painter?.gpuBytes(v?.w || 1920, v?.h || 1080);
    const scene = this.painter?.scene?.manifest.pack === m.manifest.pack ? this.painter.scene : undefined;
    const geometry = scene && fixed ? fixed.geometry + (scene.bytes.joined ? 0 : scene.bytes.indices) : m.manifest.bytes * 1.5;
    const own = (m.light ? 96 : 160) * MB - loose;
    const window = 300 * MB - geometry - (fixed?.targets ?? 100 * MB) - loose;
    return Math.max(16 * MB, Math.min(own, window));
  }

  async loadBus(m) {
    const nr = m.load;
    const p = this.painter;
    const v = this.viewers.get(m.viewer);
    if (!p || !v) return;
    const same = p.scene?.manifest.pack === m.manifest.pack && p.scene.xfKey === JSON.stringify(m.look?.xf || {});
    // the look asked for NOW: a livery clicked while loading comes as 'look' into this.loading.msg
    const look = () => (nr === this.loading.nr && this.loading.msg ? this.loading.msg.look : m.look);
    try {
      if (!same) {
        this.geometryBusy = true;
        const set = await p.load(
          m.manifest,
          m.look,
          (id) => this.fetchPack(id, m.manifest.bytes),
          () => new Promise((k) => setTimeout(k, 0)),
          () => nr === this.loading.nr,
          (head) => {
            if (nr !== this.loading.nr) return;
            this.mark('head');
            p.earlyPlan(head, m.manifest, look(), this.budget(m));
          },
        );
        this.geometryBusy = false;
        if (!set || nr !== this.loading.nr) return;
        this.mark('geometry');
        const d = m.manifest;
        v.camera.setBox([-d.max[0], d.min[1], d.min[2]], [-d.min[0], d.max[1], d.max[2]]);
        if (!m.keepCamera) v.camera.restart();
      }
      p.setLook(look(), this.budget(m));
      this.mark('look');
      this.studio?.busChanged?.();
      this.plan();
    } catch (x) {
      this.geometryBusy = false;
      if (nr !== this.loading.nr) return;
      const text = x?.message || String(x);
      this.post({ kind: 'error', viewer: m.viewer, load: nr, reason: /B3D2|cut off|manifest/.test(text) ? 'pack-broken' : 'failed', detail: text, pack: m.manifest.pack });
    }
  }

  mark(name) {
    this.loading.marks[name] ??= Math.round(now() - this.loading.t0);
  }

  // ------------------------------------------------------------ drawing
  plan() {
    if (this.planned || !this.active || this.active.paused || this.lost) return;
    this.planned = true;
    let done = false;
    const go = () => {
      if (done) return;
      done = true;
      this.planned = false;
      this.tick();
    };
    // (a hidden window sometimes gives no animation frame: then after 16 ms)
    this.raf?.(go);
    setTimeout(go, 16);
  }

  size(w, h) {
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
  }

  tick() {
    const v = this.active;
    const p = this.painter;
    const gl = this.gl;
    if (!v || !p || !gl || v.w < 2 || v.h < 2 || this.lost || v.paused || gl.isContextLost() || this.shooting) return;
    if (v.inFlight >= 2) return; // ('seen' plans the next)
    // another bus asked for and its geometry not there yet: the former no more
    const want = this.loading.msg;
    if (want && p.scene && p.scene.manifest.pack !== want.manifest.pack) return;
    const t = performance.now();
    const dt = this.lastTick ? (t - this.lastTick) / 1000 : 0;
    this.lastTick = t;
    const moving = v.camera.step(Math.min(dt, 0.1));
    const cam = v.camera.view(v.w / v.h);
    if (this.studio?.active) this.studio.work();
    this.size(v.w, v.h);
    // once everything is sharp the shadow map once more: grilles cast a closed shadow while
    // their texture loaded
    const pr = p.textures.progress();
    if (p.scene && !this.loading.shadowSharp && pr.busy === 0 && pr.done === pr.total) {
      this.loading.shadowSharp = true;
      p.scene.shadowDirty = true;
    }
    p.draw(cam, v.w, v.h);
    this.frameTimes.push(performance.now() - t);
    if (this.frameTimes.length > 240) this.frameTimes.shift();
    this.present(v, cam);
    this.studio?.drawOther?.(v);
    const scene = p.scene;
    const pr2 = p.textures.progress();
    const sharp = !!scene && pr2.busy === 0 && pr2.done === pr2.total;
    if (scene && want && scene.manifest.pack === want.manifest.pack) {
      if (this.loading.first === undefined) {
        this.loading.first = now() - this.loading.t0;
        this.mark('firstFrame');
        this.state(v, 'geometry', pr2);
      }
      if (sharp && this.loading.sharp === undefined) {
        this.loading.sharp = now() - this.loading.t0;
        this.state(v, 'sharp', pr2);
      } else if (!sharp && t - this.loading.lastState > 250) {
        this.loading.lastState = t;
        this.state(v, 'textures', pr2);
      }
    }
    // after the picture: time for uploads (not while the geometry still comes)
    if (!this.geometryBusy) p.textures.giveTime(6);
    const turning = v.turntable && !want?.light && performance.now() - v.lastInput > TURNTABLE_WAIT;
    if (turning) v.camera.turn(6 * Math.min(dt, 0.1));
    if (moving || turning || p.textures.waitingUploads() > 0 || this.studio?.busy?.()) this.plan();
  }

  /** The picture to the viewer that asked. */
  present(v, cam) {
    const bitmap = this.canvas.transferToImageBitmap ? this.canvas.transferToImageBitmap() : undefined;
    v.inFlight++;
    const msg = { kind: 'frame', viewer: v.id, bitmap, canvas: bitmap ? undefined : this.canvas, load: this.loading.nr, cam: this.studio?.active ? Array.from(cam.viewProj) : undefined, moving: v.camera.inside ? false : undefined };
    this.post(msg, bitmap ? [bitmap] : []);
  }

  state(v, phase, pr) {
    const m = this.loading.msg;
    if (!m) return;
    let measure;
    if (phase === 'sharp') {
      measure = { pack: m.manifest.pack, source: m.source, firstMs: Math.round(this.loading.first ?? 0), sharpMs: Math.round(this.loading.sharp ?? 0), p50: quantile(this.frameTimes, 0.5), p95: quantile(this.frameTimes, 0.95), gpuBytes: this.painter.gpuBytes(v.w, v.h).total, triangles: this.painter.count().shown, dpr: v.dpr };
    }
    this.post({ kind: 'state', viewer: v.id, load: m.load, phase, done: pr.done, total: pr.total, firstMs: this.loading.first !== undefined ? Math.round(this.loading.first) : undefined, sharpMs: this.loading.sharp !== undefined ? Math.round(this.loading.sharp) : undefined, measure, marks: { ...this.loading.marks } });
  }

  wake(v) {
    v.lastInput = performance.now();
    clearTimeout(this.turnClock);
    if (v.turntable) this.turnClock = setTimeout(() => this.plan(), TURNTABLE_WAIT + 50);
  }

  // ------------------------------------------------------------ pictures and measuring
  async shot(v, a) {
    const p = this.painter;
    const before = { w: this.canvas.width, h: this.canvas.height };
    this.shooting = true;
    try {
      if (a.photo) return { png: await this.photo(v, a) };
      this.size(a.w, a.h);
      const cam = v.camera.view(a.w / a.h, a.stand ? { stand: a.stand } : undefined);
      p.draw(cam, a.w, a.h);
      const blob = await (this.canvas.convertToBlob ? this.canvas.convertToBlob({ type: 'image/png' }) : new Promise((k) => this.canvas.toBlob(k, 'image/png')));
      this.canvas.transferToImageBitmap?.().close();
      this.size(before.w, before.h);
      return { png: await blob.arrayBuffer() };
    } finally {
      this.shooting = false;
      this.plan();
    }
  }

  /** The photo: transparent, 215°/8°, the box 88 % of the width. The painter gives
   * premultiplied pixels from the bottom up; turned and divided back here, then a PNG. */
  async photo(v, a) {
    const p = this.painter;
    if (a.view && a.view !== p.view) p.setView(a.view);
    const cam = v.camera.photoView(a.w / a.h, 0.88);
    const px = p.readPhoto(cam, a.w, a.h);
    const out = new Uint8ClampedArray(a.w * a.h * 4);
    const row = a.w * 4;
    for (let y = 0; y < a.h; y++) {
      const from = (a.h - 1 - y) * row;
      const to = y * row;
      for (let x = 0; x < row; x += 4) {
        const al = px[from + x + 3];
        if (al === 0) continue;
        const f = 255 / al;
        out[to + x] = px[from + x] * f;
        out[to + x + 1] = px[from + x + 1] * f;
        out[to + x + 2] = px[from + x + 2] * f;
        out[to + x + 3] = al;
      }
    }
    const flat = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(a.w, a.h) : Object.assign(document.createElement('canvas'), { width: a.w, height: a.h });
    flat.getContext('2d').putImageData(new ImageData(out, a.w, a.h), 0, 0);
    const blob = await (flat.convertToBlob ? flat.convertToBlob({ type: 'image/png' }) : new Promise((k) => flat.toBlob(k, 'image/png')));
    return blob.arrayBuffer();
  }

  /** Frame times while turning round once (the self-check, the report). */
  measureTurn(v, frames) {
    const p = this.painter;
    const gl = this.gl;
    const times = [];
    const px = new Uint8Array(4);
    this.size(v.w, v.h);
    const begin = { ...v.camera.now };
    for (let i = 0; i < frames; i++) {
      const t0 = performance.now();
      p.draw(v.camera.view(v.w / v.h, { stand: { ...begin, turn: begin.turn + (i * 360) / frames } }), v.w, v.h);
      // wait for the GPU: then the drawing itself counts, not only sending it
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
      times.push(performance.now() - t0);
    }
    this.canvas.transferToImageBitmap?.().close();
    this.plan();
    return { p50: quantile(times, 0.5), p95: quantile(times, 0.95), max: Math.max(...times), frames, fps: 1000 / Math.max(1e-3, quantile(times, 0.5)) };
  }

  // ------------------------------------------------------------ messages
  handle(m) {
    const k = m.kind;
    if (k === 'viewer') {
      const v = this.viewer(m.viewer);
      v.w = Math.max(0, Math.round(m.w));
      v.h = Math.max(0, Math.round(m.h));
      v.dpr = m.dpr;
      this.active ??= v;
      this.plan();
    } else if (k === 'gone') {
      this.viewers.delete(m.viewer);
      if (this.active?.id === m.viewer) this.active = [...this.viewers.values()][0];
    } else if (k === 'env') {
      this.env = m.env;
      this.painter?.setEnv(m.env);
      this.plan();
    } else if (k === 'light') {
      Object.assign(LIGHT, m.light || {});
      if (this.painter?.scene) this.painter.scene.shadowDirty = true;
      this.plan();
    } else if (k === 'bus') {
      const v = this.viewer(m.viewer);
      this.active = v;
      const changed = this.loading.msg?.manifest.pack !== m.manifest.pack;
      if (changed) this.studio?.stop?.();
      this.loading = { nr: m.load, msg: m, t0: m.t0, marks: {}, lastState: 0 };
      this.mark('message');
      if (changed) this.frameTimes.length = 0;
      this.loadBus(m);
    } else if (k === 'look') {
      const b = this.loading.msg;
      if (!b) return;
      // kept even while the bus loads: loadBus takes the look from here once the geometry is in
      this.loading.msg = { ...b, look: m.look };
      if (this.painter?.scene?.manifest.pack !== b.manifest.pack) return;
      // (another rest pose: the geometry again)
      if (this.painter.scene.xfKey !== JSON.stringify(m.look?.xf || {})) {
        this.loading = { nr: this.loading.nr, msg: { ...this.loading.msg, keepCamera: true }, t0: m.t0, marks: {}, lastState: 0 };
        this.loadBus(this.loading.msg);
        return;
      }
      Object.assign(this.loading, { sharp: undefined, first: undefined, shadowSharp: false, lastState: 0, t0: m.t0, marks: {} });
      this.mark('message');
      this.painter.setLook(m.look, this.budget(b));
      this.studio?.busChanged?.();
      this.plan();
    } else if (k === 'view') {
      const v = this.viewer(m.viewer);
      const man = this.painter?.scene?.manifest;
      const cam = m.view === 'driver' ? man?.driver : m.view === 'passenger' ? man?.passenger : undefined;
      v.view = cam ? m.view : 'outside';
      v.camera.setInside(cam);
      this.painter?.setView(cam ? 'inside' : 'outside');
      this.plan();
    } else if (k === 'input') {
      const v = this.viewer(m.viewer);
      const i = m.input;
      if (i.kind === 'drag') v.camera.drag(i.dx, i.dy);
      else if (i.kind === 'pan') v.camera.pan(i.dx, i.dy, v.h, v.w / Math.max(1, v.h));
      else if (i.kind === 'zoom') v.camera.zoomStep(i.factor);
      else if (i.kind === 'zoomAt') v.camera.zoomAt(i.factor, i.x, i.y, v.w / Math.max(1, v.h));
      else if (i.kind === 'centre') v.camera.centre(i.point);
      else if (i.kind === 'fit') v.camera.fit();
      else if (i.kind === 'stand') {
        v.camera.stand(i.stand);
        if (v.view !== 'outside') {
          v.view = 'outside';
          this.painter?.setView('outside');
        }
      } else if (i.kind === 'turn') v.camera.turn(i.deg);
      else if (i.kind === 'tilt') v.camera.tilt(i.deg);
      this.wake(v);
      this.plan();
    } else if (k === 'seen') {
      const v = this.viewers.get(m.viewer);
      if (v) {
        v.inFlight = Math.max(0, v.inFlight - 1);
        this.plan();
      }
    } else if (k === 'turntable') {
      const v = this.viewer(m.viewer);
      v.turntable = m.on;
      this.wake(v);
    } else if (k === 'pause') {
      const v = this.viewer(m.viewer);
      v.paused = m.on;
      if (m.on && m.release && !this.lost && this.loseExt) {
        this.wilful = true;
        this.lost = true;
        this.loseExt.loseContext();
      }
      if (!m.on && this.wilful && this.gl.isContextLost()) {
        try {
          this.loseExt.restoreContext();
        } catch {
          // not yet allowed: the lost event restores it
        }
      }
      if (!m.on) this.plan();
    } else if (k === 'shot') {
      const v = this.viewer(m.viewer);
      if (!this.painter || this.lost || this.gl.isContextLost()) return this.post({ kind: 'answer', ask: m.ask, result: { error: 'no webgl' } });
      this.shot(v, m.shot).then(
        (r) => this.post({ kind: 'answer', ask: m.ask, result: r }, r.png ? [r.png] : []),
        (x) => this.post({ kind: 'answer', ask: m.ask, result: { error: String(x) } }),
      );
    } else if (k === 'measure') {
      const v = this.viewer(m.viewer);
      if (!this.painter || this.lost) return this.post({ kind: 'answer', ask: m.ask, result: { error: 'no webgl' } });
      let result;
      if (m.what === 'turn') result = this.measureTurn(v, m.frames || 90);
      else result = { ...this.painter.gpuBytes(v.w, v.h), count: this.painter.count(), plan: this.painter.textures.lastPlan ? { bytes: this.painter.textures.lastPlan.bytes, tooHeavy: this.painter.textures.lastPlan.tooHeavy } : undefined, errors: this.painter.textures.errors.slice(0, 20), can: { s3tc: this.can.s3tc, s3tcSrgb: this.can.s3tcSrgb }, stats: this.painter.textures.stats, frames: { p50: quantile(this.frameTimes, 0.5), p95: quantile(this.frameTimes, 0.95), n: this.frameTimes.length } };
      this.post({ kind: 'answer', ask: m.ask, result });
    } else if (k === 'fetched') {
      const a = this.asks.get(m.ask);
      this.asks.delete(m.ask);
      if (m.error) a?.fail(new Error(m.error));
      else a?.ok(m.bytes);
    } else if (k.startsWith('lak') && this.studio) {
      this.studio.handle(m);
    } else if (k.startsWith('lak')) {
      this.lakQueue ??= [];
      this.lakQueue.push(m);
    }
  }
}

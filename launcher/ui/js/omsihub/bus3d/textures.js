// Textures onto the GPU (the old app's `bus3d/texturen.ts`).
//
// No texture cache: each file comes from the OMSI folder by id (`b3d_tex`), the GPU doing the
// heavy work. Two routes, as the manifest says:
// * `dxt`: uploaded compressed, level by level, in sRGB. First the tail (the levels up to
//   128 px, a few kB) so that the bus has colour at once, then the large levels; levels over
//   the planned size never even leave the disk.
// * `rgba`: the launcher decodes the file (TGA, BMP, PNG, JPEG, an odd DDS) and halves it as
//   often as the plan skips levels; here SRGB8_ALPHA8 and `generateMipmap`. Never
//   premultiplied: the alpha of an OMSI texture is often its reflection mask.
//
// The budget (160 MB, 96 MB while a game runs) comes from `texturePlan`. Textures of a former
// livery stay in an LRU of at most 64 MB above the budget, so that going back is only binding.
//
// Uploads happen only in the time the engine gives after a picture (`giveTime`), so that the
// first picture of a bus does not wait behind forty textures and dragging does not stutter
// while they load.

import { texturePlan } from './plan.js';
import { fetchBytes } from './urls.js';

const S3TC = { DXT1: 0x83f1, DXT3: 0x83f2, DXT5: 0x83f3, SRGB_DXT1: 0x8c4d, SRGB_DXT3: 0x8c4e, SRGB_DXT5: 0x8c4f };

export class Textures {
  constructor(gl, can, onChange) {
    this.gl = gl;
    this.can = can;
    this.onChange = onChange;
    /** id@skip -> entry */
    this.entries = new Map();
    /** slot (index into the bus's textures, or 'sky'/'clouds') -> { t, k } */
    this.goals = new Map();
    this.queue = [];
    this.running = 0;
    this.AT_ONCE = 8;
    this.gen = 0;
    this.lruMax = 64 * 1024 * 1024;
    this.errors = [];
    this.stats = {};
    this.lastPlan = undefined;
    /** The livery studio's own pictures in place of a slot's texture. */
    this.overrides = new Map();
    this.uploadWait = [];
    this.uploadUntil = 0;
    this.uploading = false;
    this.looseBytes = new Map();
    /** How bytes are fetched: from the protocol (the engine may hand it to the window). */
    this.fetch = fetchBytes;
    this.samplers = { repeat: this.sampler0(gl.REPEAT, true), clamp: this.sampler0(gl.CLAMP_TO_EDGE, true), sky: this.sampler0(gl.REPEAT, false) };
  }

  sampler0(wrap, aniso) {
    const gl = this.gl;
    const s = gl.createSampler();
    gl.samplerParameteri(s, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.samplerParameteri(s, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.samplerParameteri(s, gl.TEXTURE_WRAP_S, wrap);
    gl.samplerParameteri(s, gl.TEXTURE_WRAP_T, wrap);
    if (aniso && this.can.anisoExt && this.can.aniso > 1) gl.samplerParameterf(s, this.can.anisoExt.TEXTURE_MAX_ANISOTROPY_EXT, Math.min(8, this.can.aniso));
    return s;
  }

  sampler(clamp) {
    return clamp ? this.samplers.clamp : this.samplers.repeat;
  }

  skySampler() {
    return this.samplers.sky;
  }

  /** The livery studio's composed paint in place of a slot's texture (none: taken away). */
  setOverride(slot, gpu) {
    if (gpu) this.overrides.set(slot, gpu);
    else this.overrides.delete(slot);
    this.onChange();
  }

  /** The texture for a slot now, or another size of the same while that one loads. */
  forSlot(slot) {
    const own = this.overrides.get(slot);
    if (own) return own;
    const goal = this.goals.get(slot);
    if (!goal) return undefined;
    const e = this.entries.get(`${goal.t.id}@${goal.k}`);
    if (e?.gpu) {
      e.used = performance.now();
      return e.gpu;
    }
    for (const x of this.entries.values()) if (x.id === goal.t.id && x.gpu) return x.gpu;
    return undefined;
  }

  /** The plan for a bus: per slot its texture (after the livery), which slots are drawn now,
   * the budget and the order (the body first). */
  setPlan(textures, needed, budget) {
    this.gen++;
    const gen = this.gen;
    const list = textures.map((t, i) => (needed.has(i) ? { ...t, area: Math.max(t.area, 0.05), uv: Math.max(t.uv, 0.05) } : { ...t, area: 0 }));
    const plan = texturePlan(list, budget, { s3tc: this.can.s3tc });
    this.lastPlan = plan;
    for (const k of [...this.goals.keys()]) if (typeof k === 'number') this.goals.delete(k);
    textures.forEach((t, i) => {
      const r = plan.rows[i];
      if (!r.load) return;
      this.goals.set(i, { t, k: r.skip });
      // the body first, then the dearest first: the longest to come decides when all is sharp
      const cost = (t.bytes / 1e5) * (t.kind === 'rgba' ? 2 : 0.5);
      const prio = (plan.body && t.ctc === plan.body ? 1e6 : 0) + cost + list[i].area * 0.01;
      this.ask(t, r.skip, prio, gen);
    });
    this.trim();
    return plan;
  }

  /** What the loose textures (sky, clouds) take on the GPU. */
  looseTotal() {
    let s = 0;
    for (const b of this.looseBytes.values()) s += b;
    return s;
  }

  /** A loose texture: the sky and the clouds (at most 2048). */
  setLoose(name, t) {
    let k = 0;
    while (Math.max(t.w >> k, t.h >> k) > 2048) k++;
    if (t.kind === 'dxt') k = Math.min(k, Math.max(0, t.mips - 1));
    this.looseBytes.set(name, Math.round((Math.max(1, t.w >> k) * Math.max(1, t.h >> k) * 16) / 3));
    this.goals.set(name, { t, k });
    this.ask(t, k, 2e6, this.gen, true);
  }

  /** How many of the bus's slots stand in full (not only their tail). */
  progress() {
    let done = 0;
    let total = 0;
    for (const [slot, goal] of this.goals) {
      if (typeof slot !== 'number') continue;
      total++;
      const e = this.entries.get(`${goal.t.id}@${goal.k}`);
      if (e?.gpu?.state === 'full' || e?.error) done++;
    }
    return { done, total, busy: this.running + this.queue.length };
  }

  gpuBytes() {
    let s = 0;
    for (const e of this.entries.values()) s += e.gpu?.bytes ?? 0;
    return s;
  }

  forgetAll() {
    for (const e of this.entries.values()) if (e.gpu) this.gl.deleteTexture(e.gpu.tex);
    this.entries.clear();
    this.goals.clear();
    this.queue = [];
    this.gen++;
  }

  setLruMax(bytes) {
    this.lruMax = Math.max(0, bytes);
    this.trim();
  }

  /** What is no goal any more goes to the LRU; over its size the oldest goes. */
  trim() {
    const inUse = new Set();
    for (const g of this.goals.values()) inUse.add(`${g.t.id}@${g.k}`);
    const loose = [...this.entries.values()].filter((e) => !inUse.has(e.key) && !e.busy);
    loose.sort((a, b) => a.used - b.used);
    let bytes = loose.reduce((s, e) => s + (e.gpu?.bytes ?? 0), 0);
    for (const e of loose) {
      if (bytes <= this.lruMax && !e.error) break;
      if (e.gpu) this.gl.deleteTexture(e.gpu.tex);
      bytes -= e.gpu?.bytes ?? 0;
      this.entries.delete(e.key);
    }
  }

  // ------------------------------------------------------------ the queue
  ask(t, k, prio, gen, loose = false) {
    const key = `${t.id}@${k}`;
    const known = this.entries.get(key);
    if (known && (known.gpu?.state === 'full' || known.busy || known.error)) return;
    const e = known ?? { key, id: t.id, k, busy: false, used: performance.now() };
    this.entries.set(key, e);
    e.busy = true;
    const wanted = () => loose || gen === this.gen || this.isGoal(key);
    const done = () => {
      e.busy = false;
      this.onChange();
    };
    const failed = (err) => {
      e.busy = false;
      e.error = err?.message || String(err);
      this.errors.push(`${t.name}: ${e.error}`);
      this.onChange();
    };
    if (t.kind === 'dxt' && this.can.s3tc && t.levels?.length > k) {
      const tail = this.tailFrom(t, k);
      if (tail > k) {
        this.put({
          prio: 1e7 + prio,
          run: async () => {
            if (!wanted()) return done();
            try {
              await this.loadDxt(t, k, e, tail, 'tail');
            } catch (x) {
              return failed(x);
            }
            this.onChange();
            this.put({ prio, run: async () => (wanted() ? this.loadDxt(t, k, e, tail, 'full').then(done, failed) : done()) });
          },
        });
        return;
      }
      this.put({ prio: 1e7 + prio, run: () => this.loadDxt(t, k, e, k, 'full').then(done, failed) });
      return;
    }
    this.put({ prio, run: async () => (wanted() ? this.loadRgba(t, k, e).then(done, failed) : done()) });
  }

  isGoal(key) {
    for (const g of this.goals.values()) if (`${g.t.id}@${g.k}` === key) return true;
    return false;
  }

  put(job) {
    this.queue.push(job);
    this.pump();
  }

  pump() {
    while (this.running < this.AT_ONCE && this.queue.length > 0) {
      let best = 0;
      for (let i = 1; i < this.queue.length; i++) if (this.queue[i].prio > this.queue[best].prio) best = i;
      const [job] = this.queue.splice(best, 1);
      this.running++;
      Promise.resolve()
        .then(() => job.run())
        .catch((x) => this.errors.push(String(x)))
        .finally(() => {
          this.running--;
          this.pump();
        });
    }
  }

  // ------------------------------------------------------------ uploading in turns
  /** The engine, after a picture: this many ms may go to uploading now. */
  giveTime(ms) {
    this.uploadUntil = performance.now() + ms;
    this.nextUpload();
  }

  waitingUploads() {
    return this.uploadWait.length;
  }

  nextUpload() {
    if (this.uploading || this.uploadWait.length === 0) return;
    if (performance.now() >= this.uploadUntil) {
      // the time is up: the next picture gives more
      this.onChange();
      return;
    }
    this.uploading = true;
    this.uploadWait.shift()();
  }

  uploadTurn() {
    return new Promise((k) => {
      this.uploadWait.push(k);
      this.nextUpload();
    });
  }

  uploadDone() {
    this.uploading = false;
    this.nextUpload();
  }

  // ------------------------------------------------------------ DXT
  dxtFormat(t) {
    const f = t.fmt || 'bc1';
    if (this.can.s3tcSrgb) return { fmt: f === 'bc1' ? S3TC.SRGB_DXT1 : f === 'bc2' ? S3TC.SRGB_DXT3 : S3TC.SRGB_DXT5, linearise: false };
    return { fmt: f === 'bc1' ? S3TC.DXT1 : f === 'bc2' ? S3TC.DXT3 : S3TC.DXT5, linearise: true };
  }

  /** The first level of the tail: the levels of at most 128 px, a few kB together. */
  tailFrom(t, k) {
    const n = t.levels.length;
    for (let j = k; j < n; j++) if (Math.max(t.w >> j, t.h >> j) <= 128) return j;
    return n - 1;
  }

  async loadDxt(t, k, e, tail, part) {
    const levels = t.levels;
    const n = levels.length;
    const from = part === 'tail' ? tail : k;
    const to = part === 'tail' || tail <= k ? n - 1 : Math.max(k, tail - 1);
    const t0 = performance.now();
    const bytes = await this.fetch('b3d_tex', { id: t.id, from: levels[from][0], to: levels[to][0] + levels[to][1] });
    await this.uploadTurn();
    try {
      const gl = this.gl;
      const t1 = performance.now();
      const { fmt, linearise } = this.dxtFormat(t);
      let gpu = e.gpu;
      if (!gpu) {
        const tex = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.texStorage2D(gl.TEXTURE_2D, n - k, fmt, Math.max(1, t.w >> k), Math.max(1, t.h >> k));
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAX_LEVEL, n - k - 1);
        let b = 0;
        for (let j = k; j < n; j++) b += levels[j][1];
        gpu = { tex, bytes: b, w: t.w >> k, h: t.h >> k, linearise, hasAlpha: true, state: 'tail' };
      }
      gl.bindTexture(gl.TEXTURE_2D, gpu.tex);
      const base = levels[from][0];
      for (let j = from; j <= to; j++) {
        const w = Math.max(1, t.w >> j);
        const h = Math.max(1, t.h >> j);
        gl.compressedTexSubImage2D(gl.TEXTURE_2D, j - k, 0, 0, w, h, fmt, new Uint8Array(bytes, levels[j][0] - base, levels[j][1]));
      }
      const all = part === 'full' || tail <= k;
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_BASE_LEVEL, all ? 0 : tail - k);
      gpu.state = all ? 'full' : 'tail';
      e.gpu = gpu;
      this.stat(`dxt-${part}`, t1 - t0, performance.now() - t1);
    } finally {
      this.uploadDone();
    }
  }

  // ------------------------------------------------------------ RGBA
  async loadRgba(t, k, e) {
    const gl = this.gl;
    const t0 = performance.now();
    const buf = await this.fetch('b3d_tex', { id: t.id, skip: k });
    const head = new DataView(buf, 0, 8);
    const w = head.getUint32(0, true);
    const h = head.getUint32(4, true);
    const px = new Uint8Array(buf, 8, w * h * 4);
    await this.uploadTurn();
    try {
      const t1 = performance.now();
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
      const tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      const levels = Math.floor(Math.log2(Math.max(w, h, 1))) + 1;
      gl.texStorage2D(gl.TEXTURE_2D, levels, gl.SRGB8_ALPHA8, w, h);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
      gl.generateMipmap(gl.TEXTURE_2D);
      e.gpu = { tex, bytes: Math.round((w * h * 16) / 3), w, h, linearise: false, hasAlpha: t.alpha, state: 'full' };
      this.stat('rgba', t1 - t0, performance.now() - t1);
    } finally {
      this.uploadDone();
    }
  }

  stat(route, wait, sync) {
    const s = (this.stats[route] ??= { n: 0, waitMs: 0, syncMs: 0, maxSyncMs: 0 });
    s.n++;
    s.waitMs += wait;
    s.syncMs += sync;
    s.maxSyncMs = Math.max(s.maxSyncMs, sync);
  }
}

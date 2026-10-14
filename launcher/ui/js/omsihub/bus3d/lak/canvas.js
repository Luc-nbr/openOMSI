// The livery studio's painting engine (Omsi-Hub's Lakstudio, the old app's `lak/lakdoek.ts`),
// on the 3D view's own WebGL2 context: it joins the renderer (`engine.js`) when the studio
// opens and paints the livery into the bus's textures on the GPU, at their full size, as the
// layers change - the paint rules the game program's studio has (`omsihub-livery`'s
// `paint.rs`), so that what the player sees here is what the save into the game writes (the
// launcher paints that on the CPU, `lak_place`).
//
// Per painted texture, once a bus: the LAKNET (its triangles in texture space, `shaders.js`),
// the texels' mask - which are covered, seen from outside (26 directions, as `Outside`), glass,
// kept, a window's see-through map, shared by both sides, where the uncovered ones bleed from -
// and the base as the colour zones make it (`Zones::unmix`, specks taken into the paint). Per
// change of the layers: the operations into a table, the texts, shapes and pictures into an
// atlas (drawn by the launcher in their fonts, `lak_decal`), the brush strokes into a cover per
// brush layer, then one draw of the laknet paints every texel and one pass bleeds the seams;
// the texture takes the place of the bus's own (`Textures.setOverride`).

import { program } from '../shaders.js';
import { inView } from '../draw.js';
import { lookAt, mul, ortho } from '../math.js';
import * as S from './shaders.js';
import { corners, decalKey, decalSize, isDecal, linearOf, mirrorImage, mirrorPlace, SIDES, strokeO3d } from './model.js';

/** Join the engine (`worker.js` and `link.js` call it). */
export function attach(engine) {
  engine.studio = new Studio(engine);
}

const ATLAS_W = 4096;
const ATLAS_H = 2048;
/** Texels the studio paints at, over all its textures (beyond: the largest halved). */
const MOST_TEXELS = 40 * 1024 * 1024;
const DEPTH_SIZE = 1024;
const SIDE_NUMBER = { rondom: 0, zijden: 1, voor: 2, achter: 3 };

class Studio {
  constructor(engine) {
    this.e = engine;
    this.active = false;
    this.prep = undefined;
    this.viewer = 0;
    this.layers = [];
    this.planeX = undefined;
    this.sizes = {};
    this.project = '';
    this.before = false;
    this.dirty = false;
    this.state = 'idle';
    this.targets = [];
    this.rasters = new Map();
    this.fetching = new Set();
    this.atlasKeys = '';
    this.brushes = new Map();
    this.stats = { builds: [], paints: [] };
    this.made = false;
  }

  get gl() {
    return this.e.gl;
  }

  post(m) {
    this.e.post(m);
  }

  // ------------------------------------------------------------ the engine's hooks
  /** The context came back: everything on the GPU again. */
  restart() {
    this.made = false;
    this.targets = [];
    this.net = undefined;
    this.brushes.clear();
    this.atlasKeys = '';
    if (this.prep) this.state = 'waiting';
  }

  busChanged() {
    if (this.prep && this.state === 'waiting') this.e.plan();
  }

  busy() {
    return this.active && (this.dirty || this.state === 'waiting');
  }

  /** Before each picture: the bus built once its geometry is there, the paint when it changed. */
  work() {
    // (another shape of the bus - its bus options, its rest pose: the laknet anew)
    if (this.state === 'ready' && this.e.painter?.scene !== this.builtScene) {
      this.free();
      this.state = 'waiting';
    }
    if (this.state === 'waiting') this.tryBuild();
    if (this.state === 'ready' && this.dirty) this.paint();
  }

  stop() {
    this.free();
    this.prep = undefined;
    this.active = false;
    this.state = 'idle';
  }

  // ------------------------------------------------------------ messages
  handle(m) {
    const k = m.kind;
    if (k === 'lakLoad') return;
    if (k === 'lakStart') {
      this.free();
      this.prep = m.prep;
      this.viewer = m.viewer;
      this.active = true;
      this.state = 'waiting';
      this.setLayers(m);
      const v = this.e.viewers.get(m.viewer);
      if (v) v.camera.zoomMin = 0.1;
      this.e.plan();
    } else if (k === 'lakLayers') {
      this.setLayers(m);
      this.e.plan();
    } else if (k === 'lakBefore') {
      this.before = !!m.on;
      this.override();
      this.e.plan();
    } else if (k === 'lakPick') {
      let result;
      try {
        result = this.pick(m);
      } catch (x) {
        result = { error: String(x?.message || x) };
      }
      this.post({ kind: 'answer', ask: m.ask, result });
    } else if (k === 'lakMeasure') {
      this.post({ kind: 'answer', ask: m.ask, result: this.measure(m) });
    } else if (k === 'lakStop') {
      this.stop();
      this.e.plan();
    }
  }

  setLayers(m) {
    if (m.layers) this.layers = m.layers;
    if ('planeX' in m) this.planeX = m.planeX ?? undefined;
    if (m.sizes) this.sizes = m.sizes;
    if (m.project) this.project = m.project;
    this.live = m.live;
    this.dirty = true;
  }

  // ------------------------------------------------------------ programs and targets
  make() {
    if (this.made) return;
    const gl = this.gl;
    this.floatOk = !!gl.getExtension('EXT_color_buffer_float');
    const p = (vs, fs, names) => {
      const prog = program(gl, vs, fs);
      const u = {};
      for (const n of names) u[n] = gl.getUniformLocation(prog, n);
      return { prog, u };
    };
    const net = ['uMid', 'uHalf'];
    const zones = ['uZones', 'uZoneLab', 'uZoneLin', 'uZoneInfo'];
    this.p = {
      cls: p(S.NET_VS, S.CLASS_FS, net),
      place: p(S.NET_VS, S.PLACE_FS, [...net, 'uLo', 'uSize']),
      depth: p(S.DEPTH_VS, S.DEPTH_FS, ['uMat']),
      outside: p(S.NET_VS, S.OUTSIDE_FS, [...net, 'uDepth', 'uMat', 'uDir', 'uRange', 'uCell']),
      seed: p(S.SCREEN_VS, S.BLEED_SEED_FS, ['uClass']),
      step: p(S.SCREEN_VS, S.BLEED_STEP_FS, ['uPrev']),
      mask: p(S.SCREEN_VS, S.MASK_FS, ['uClass', 'uOut', 'uLoTex', 'uHiTex', 'uBleed', 'uSize']),
      mix: p(S.SCREEN_VS, S.MIX_FS, ['uBase', 'uLinearise', ...zones]),
      specks: p(S.SCREEN_VS, S.SPECKS_FS, ['uMix', 'uBase', 'uLinearise', ...zones]),
      paint: p(S.NET_VS, S.PAINT_FS, [...net, ...zones, 'uBase', 'uMask', 'uMix', 'uTpl', 'uOps', 'uAtlas', 'uBrush', 'uOpCount', 'uTemplate', 'uMuRef', 'uMin', 'uMax', 'uAA', 'uAlpha', 'uLinearise', 'uBefore']),
      copy: p(S.SCREEN_VS, S.COPY_FS, ['uBase', 'uLinearise', 'uSize']),
      spread: p(S.SCREEN_VS, S.SPREAD_FS, ['uPaint', 'uMask', 'uBase', 'uLinearise']),
      stroke: p(S.NET_VS, S.STROKE_FS, [...net, 'uPts', 'uNrm', 'uCount', 'uLo', 'uHi', 'uStroke']),
      layer: p(S.SCREEN_VS, S.LAYER_FS, ['uSrc']),
      pick: p(S.PICK_VS, S.PICK_FS, ['uMat', 'uTarget']),
      sample: p(S.SCREEN_VS, S.SAMPLE_FS, ['uBase', 'uAt', 'uLinearise']),
    };
    this.empty = gl.createVertexArray();
    this.made = true;
  }

  /** A texture of `w` x `h` in `fmt`, nearest or linear, clamped; with mips when asked. */
  tex(w, h, fmt, linear = false, mips = false) {
    const gl = this.gl;
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, t);
    const levels = mips ? Math.floor(Math.log2(Math.max(w, h))) + 1 : 1;
    gl.texStorage2D(gl.TEXTURE_2D, levels, fmt, w, h);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, mips ? gl.LINEAR_MIPMAP_LINEAR : linear ? gl.LINEAR : gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, linear || mips ? gl.LINEAR : gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return t;
  }

  fb(tex, layer) {
    const gl = this.gl;
    const f = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, f);
    if (layer === undefined) gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    else gl.framebufferTextureLayer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, tex, 0, layer);
    return f;
  }

  free() {
    const gl = this.gl;
    for (const slot of this.overridden || []) this.e.painter?.textures.setOverride(slot, undefined);
    this.overridden = [];
    if (!gl || gl.isContextLost()) {
      this.targets = [];
      return;
    }
    const del = (t) => t && gl.deleteTexture(t);
    for (const t of this.targets) {
      for (const x of [t.base?.tex, t.mask, t.mix, t.tpl, t.out, t.brush?.tex, t.brush?.before?.tex]) del(x);
      for (const f of [t.outFb]) f && gl.deleteFramebuffer(f);
    }
    this.targets = [];
    if (this.net) {
      gl.deleteBuffer(this.net.buf);
      gl.deleteVertexArray(this.net.vao);
      this.net = undefined;
    }
    for (const x of [this.scratch, this.strokeBuf, this.atlas, this.opsTex]) {
      if (!x) continue;
      if (x.tex) del(x.tex);
      if (x.fb) gl.deleteFramebuffer(x.fb);
      if (x.depth) gl.deleteRenderbuffer(x.depth);
      if (x instanceof WebGLTexture) del(x);
    }
    this.scratch = this.strokeBuf = this.atlas = this.opsTex = undefined;
    this.brushes.clear();
    this.atlasKeys = '';
  }

  // ------------------------------------------------------------ the bus, once
  tryBuild() {
    const scene = this.e.painter?.scene;
    if (!scene || !this.prep || scene.manifest.pack !== this.prep.pack || !scene.look) return;
    if (this.building) return;
    this.building = true;
    this.state = 'building';
    const t0 = performance.now();
    this.build(scene)
      .then(() => {
        this.state = 'ready';
        this.dirty = true;
        const ms = Math.round(performance.now() - t0);
        this.stats.builds.push(ms);
        this.post({ kind: 'lakReady', viewer: this.viewer, ms, targets: this.targets.map((t) => ({ w: t.w, h: t.h, full: [t.fullW, t.fullH], triangles: t.range.count / 3 })) });
      })
      .catch((x) => {
        this.state = 'failed';
        this.post({ kind: 'lakError', viewer: this.viewer, error: String(x?.message || x) });
      })
      .finally(() => {
        this.building = false;
        this.e.plan();
      });
  }

  async build(scene) {
    this.make();
    const gl = this.gl;
    const prep = this.prep;
    // the sizes painted at: the full size, the largest halved while all go beyond the budget
    const sizes = prep.targets.map((t) => [t.w, t.h]);
    const maxTex = gl.getParameter(gl.MAX_TEXTURE_SIZE);
    for (const s of sizes) while (Math.max(s[0], s[1]) > maxTex) (s[0] = s[0] >> 1), (s[1] = s[1] >> 1);
    const total = () => sizes.reduce((n, s) => n + s[0] * s[1], 0);
    while (total() > MOST_TEXELS) {
      let k = 0;
      for (let i = 1; i < sizes.length; i++) if (sizes[i][0] * sizes[i][1] > sizes[k][0] * sizes[k][1]) k = i;
      sizes[k] = [sizes[k][0] >> 1, sizes[k][1] >> 1];
    }
    this.buildNet(scene);
    const bases = await Promise.all(prep.targets.map((t, k) => this.loadBase(t, sizes[k])));
    const tpls = await Promise.all(prep.targets.map((t, k) => (t.template ? this.loadTemplate(k, sizes[k]).catch(() => undefined) : undefined)));
    this.targets = prep.targets.map((t, k) => ({
      ...t,
      k,
      fullW: t.w,
      fullH: t.h,
      w: sizes[k][0],
      h: sizes[k][1],
      range: this.net.ranges[k] || { first: 0, count: 0 },
      base: bases[k],
      tpl: tpls[k]?.tex,
      muRef: tpls[k]?.muRef,
      density: t.density * (sizes[k][0] / Math.max(1, t.w)),
    }));
    const mw = Math.max(...this.targets.map((t) => t.w));
    const mh = Math.max(...this.targets.map((t) => t.h));
    this.scratch = this.makeScratch(mw, mh);
    this.statics(scene);
    for (const t of this.targets) {
      t.out = this.tex(t.w, t.h, gl.SRGB8_ALPHA8, true, true);
      t.outFb = this.fb(t.out);
    }
    this.slotsOfTargets(scene);
    this.builtScene = scene;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** The laknet: per painted texture its triangles, their UVs moved into the texture (a
   * triangle across its edge drawn once more on the other side), and their classes. */
  buildNet(scene) {
    const gl = this.gl;
    const { head, look } = scene;
    const roles = this.prep.roles || [];
    const vis = look?.visible || '';
    const n = scene.bytes.verts / 32;
    const verts = new Float32Array(n * 8);
    gl.bindBuffer(gl.ARRAY_BUFFER, scene.vb);
    gl.getBufferSubData(gl.ARRAY_BUFFER, 0, verts);
    const nt = this.prep.targets.length;
    // which runs of which entries go to which texture, with their class
    const runs = Array.from({ length: nt }, () => []);
    let count = new Array(nt).fill(0);
    head.entries.forEach((e, i) => {
      const r = roles[i];
      if (!r || !inView(e, 'outside') || (vis.length === head.entries.length && vis[i] === '0')) return;
      const piece = head.pieces[e.piece];
      if (!piece) return;
      for (const g of piece.groups) {
        const role = r[g.mat];
        if (!role) continue;
        if (!this.e.painter.matFor(i, g.mat, e.mats[g.mat], look)) continue;
        const [t, glass, keep, trans] = role;
        const first = scene.start[i] + g.first;
        if (t >= 0 && t < nt) {
          runs[t].push([first, g.count, (glass ? 1 : 0) | (keep ? 2 : 0)]);
          count[t] += g.count;
        }
        if (trans >= 0 && trans < nt) {
          runs[trans].push([first, g.count, 1 | 4]);
          count[trans] += g.count;
        }
      }
    });
    // (a quarter more for the triangles across an edge)
    const cap = count.map((c) => Math.ceil(c * 1.25) + 64);
    const data = new Float32Array(cap.reduce((a, b) => a + b, 0) * 9);
    let at = 0;
    const ranges = [];
    const idx = scene.allIndices;
    for (let t = 0; t < nt; t++) {
      const first = at / 9;
      const room = (first + cap[t]) * 9;
      const put = (k, du, dv, cls) => {
        const o = k * 8;
        data[at++] = verts[o];
        data[at++] = verts[o + 1];
        data[at++] = verts[o + 2];
        data[at++] = verts[o + 3];
        data[at++] = verts[o + 4];
        data[at++] = verts[o + 5];
        data[at++] = verts[o + 6] - du;
        data[at++] = verts[o + 7] - dv;
        data[at++] = cls;
      };
      for (const [first, cnt, cls] of runs[t]) {
        for (let q = 0; q < cnt; q += 3) {
          const a = idx[first + q];
          const b = idx[first + q + 1];
          const c = idx[first + q + 2];
          const u = [verts[a * 8 + 6], verts[b * 8 + 6], verts[c * 8 + 6]];
          const v = [verts[a * 8 + 7], verts[b * 8 + 7], verts[c * 8 + 7]];
          const su = Math.floor((u[0] + u[1] + u[2]) / 3);
          const sv = Math.floor((v[0] + v[1] + v[2]) / 3);
          const u0 = Math.min(...u) - su;
          const u1 = Math.max(...u) - su;
          const v0 = Math.min(...v) - sv;
          const v1 = Math.max(...v) - sv;
          const us = [0];
          if (u0 < 0) us.push(-1);
          if (u1 > 1) us.push(1);
          const vs = [0];
          if (v0 < 0) vs.push(-1);
          if (v1 > 1) vs.push(1);
          for (const ku of us)
            for (const kv of vs) {
              if (at + 27 > room) continue;
              put(a, su + ku, sv + kv, cls);
              put(b, su + ku, sv + kv, cls);
              put(c, su + ku, sv + kv, cls);
            }
        }
      }
      ranges.push({ first, count: at / 9 - first });
      at = room;
    }
    const buf = gl.createBuffer();
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 36, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 36, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 36, 24);
    gl.enableVertexAttribArray(3);
    gl.vertexAttribPointer(3, 1, gl.FLOAT, false, 36, 32);
    gl.bindVertexArray(null);
    const m = scene.manifest;
    this.net = { buf, vao, ranges, mid: [(m.min[0] + m.max[0]) / 2, (m.min[1] + m.max[1]) / 2, (m.min[2] + m.max[2]) / 2], half: [Math.max(0.01, (m.max[0] - m.min[0]) / 2), Math.max(0.01, (m.max[1] - m.min[1]) / 2), Math.max(0.01, (m.max[2] - m.min[2]) / 2)] };
  }

  /** A texture's base on the GPU at `size`: its DXT as it is when the sizes match, else its
   * picture decoded by the launcher. */
  async loadBase(t, size) {
    const gl = this.gl;
    const info = t.base;
    const can = this.e.can;
    if (info.kind === 'dxt' && can.s3tc && info.w === size[0] && info.h === size[1] && info.levels?.length) {
      const [off, len] = info.levels[0];
      const bytes = await this.e.fetch('b3d_tex', { id: info.id, from: off, to: off + len });
      const fmtName = info.fmt || 'bc1';
      const S3 = { bc1: [0x83f1, 0x8c4d], bc2: [0x83f2, 0x8c4e], bc3: [0x83f3, 0x8c4f] }[fmtName] || [0x83f1, 0x8c4d];
      const fmt = can.s3tcSrgb ? S3[1] : S3[0];
      const tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texStorage2D(gl.TEXTURE_2D, 1, fmt, info.w, info.h);
      gl.compressedTexSubImage2D(gl.TEXTURE_2D, 0, 0, 0, info.w, info.h, fmt, new Uint8Array(bytes));
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return { tex, linearise: !can.s3tcSrgb };
    }
    let skip = 0;
    while (Math.max(info.w >> skip, info.h >> skip) > Math.max(size[0], size[1]) * 1.5) skip++;
    const buf = await this.e.fetch('b3d_tex', { id: info.id, skip });
    const head = new DataView(buf, 0, 8);
    const w = head.getUint32(0, true);
    const h = head.getUint32(4, true);
    const tex = this.tex(w, h, gl.SRGB8_ALPHA8, true);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(buf, 8, w * h * 4));
    return { tex, linearise: false };
  }

  /** The maker's template of target `k` (MU, MA in alpha) and MU's median where MA paints. */
  async loadTemplate(k, size) {
    const gl = this.gl;
    const buf = await this.e.fetch('lak_template', { target: k });
    const d = new DataView(buf);
    const w = d.getUint32(0, true);
    const h = d.getUint32(4, true);
    const muRef = [d.getFloat32(8, true) / 255, d.getFloat32(12, true) / 255, d.getFloat32(16, true) / 255];
    const tex = this.tex(w, h, gl.RGBA8, true);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(buf, 20, w * h * 4));
    if (w !== size[0] || h !== size[1]) {
      // (painted at a smaller size: the template at that size, as the texels are fetched one by one)
      const small = this.tex(size[0], size[1], gl.RGBA8);
      const from = this.fb(tex);
      const to = this.fb(small);
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, from);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, to);
      gl.blitFramebuffer(0, 0, w, h, 0, 0, size[0], size[1], gl.COLOR_BUFFER_BIT, gl.LINEAR);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.deleteFramebuffer(from);
      gl.deleteFramebuffer(to);
      gl.deleteTexture(tex);
      return { tex: small, muRef };
    }
    return { tex, muRef };
  }

  makeScratch(w, h) {
    const gl = this.gl;
    const tex = this.tex(w, h, gl.SRGB8_ALPHA8);
    const depth = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, depth);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT16, w, h);
    const fb = this.fb(tex);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, depth);
    return { tex, depth, fb, w, h };
  }

  netUniforms(u) {
    const gl = this.gl;
    gl.uniform3fv(u.uMid, this.net.mid);
    gl.uniform3fv(u.uHalf, this.net.half);
  }

  zoneUniforms(u, z) {
    const gl = this.gl;
    const n = Math.min(8, z?.centres?.length || 0);
    const lab = new Float32Array(24);
    const lin = new Float32Array(24);
    const info = new Float32Array(32);
    for (let i = 0; i < n; i++) {
      lab.set(z.centres[i], i * 3);
      lin.set(z.lin[i], i * 3);
      info.set([z.painted[i] ? 1 : 0, z.group[i], z.rel[i], z.share[i]], i * 4);
    }
    gl.uniform1i(u.uZones, n);
    gl.uniform3fv(u.uZoneLab, lab);
    gl.uniform3fv(u.uZoneLin, lin);
    gl.uniform4fv(u.uZoneInfo, info);
  }

  /** A full-screen pass into `fb` at `w` x `h`. */
  screen(fb, w, h) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.viewport(0, 0, w, h);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  bindTex(unit, tex, target) {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(target ?? gl.TEXTURE_2D, tex);
    gl.bindSampler(unit, null);
  }

  /** Draw target `t`'s laknet with the program in use. */
  drawNet(t) {
    const gl = this.gl;
    if (!t.range.count) return;
    gl.bindVertexArray(this.net.vao);
    gl.drawArrays(gl.TRIANGLES, t.range.first, t.range.count);
  }

  /** The texels' masks and the base's zones, once a bus. */
  statics(scene) {
    const gl = this.gl;
    const P = this.p;
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.BLEND);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.depthMask(true);
    gl.colorMask(true, true, true, true);
    const m = scene.manifest;
    const lo = [m.min[0] - 0.05, m.min[1] - 0.05, m.min[2] - 0.05];
    const size = [m.max[0] - m.min[0] + 0.1, m.max[1] - m.min[1] + 0.1, m.max[2] - m.min[2] + 0.1];
    const tmp = this.targets.map((t) => {
      const mk = (fmt) => {
        const tex = this.tex(t.w, t.h, fmt);
        return { tex, fb: this.fb(tex) };
      };
      return { cls: mk(gl.RGBA8), lo: mk(gl.RGBA8), hi: mk(gl.RGBA8), out: mk(gl.R8), b0: mk(gl.RGBA8), b1: mk(gl.RGBA8) };
    });
    // the classes, and where the texels lie (two places far apart: a shared texel)
    this.targets.forEach((t, k) => {
      const x = tmp[k];
      gl.enable(gl.BLEND);
      gl.blendEquation(gl.MAX);
      gl.useProgram(P.cls.prog);
      this.netUniforms(P.cls.u);
      gl.bindFramebuffer(gl.FRAMEBUFFER, x.cls.fb);
      gl.viewport(0, 0, t.w, t.h);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      this.drawNet(t);
      gl.useProgram(P.place.prog);
      this.netUniforms(P.place.u);
      gl.uniform3fv(P.place.u.uLo, lo);
      gl.uniform3fv(P.place.u.uSize, size);
      gl.bindFramebuffer(gl.FRAMEBUFFER, x.hi.fb);
      gl.clear(gl.COLOR_BUFFER_BIT);
      this.drawNet(t);
      gl.blendEquation(gl.MIN);
      gl.bindFramebuffer(gl.FRAMEBUFFER, x.lo.fb);
      gl.clearColor(1, 1, 1, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      this.drawNet(t);
      gl.blendEquation(gl.FUNC_ADD);
      gl.disable(gl.BLEND);
      gl.bindFramebuffer(gl.FRAMEBUFFER, x.out.fb);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    });
    // seen from outside: the bus from 26 directions round it
    const depthTex = this.tex(DEPTH_SIZE, DEPTH_SIZE, gl.DEPTH_COMPONENT32F);
    const depthFb = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, depthFb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, depthTex, 0);
    gl.drawBuffers([gl.NONE]);
    gl.readBuffer(gl.NONE);
    const mid = [(m.min[0] + m.max[0]) / 2, (m.min[1] + m.max[1]) / 2, (m.min[2] + m.max[2]) / 2];
    const reach = Math.hypot(size[0], size[1], size[2]) / 2 + 1;
    for (let x = -1; x <= 1; x++)
      for (let y = -1; y <= 1; y++)
        for (let z = -1; z <= 1; z++) {
          if (!x && !y && !z) continue;
          const l = Math.hypot(x, y, z);
          const d = [x / l, y / l, z / l];
          const eye = [mid[0] + d[0] * reach, mid[1] + d[1] * reach, mid[2] + d[2] * reach];
          const view = lookAt(eye, mid, Math.abs(d[1]) > 0.9 ? [0, 0, 1] : [0, 1, 0]);
          // (the box round the bus seen this way)
          let x0 = Infinity;
          let x1 = -Infinity;
          let y0 = Infinity;
          let y1 = -Infinity;
          for (const a of [lo[0], lo[0] + size[0]])
            for (const b of [lo[1], lo[1] + size[1]])
              for (const c of [lo[2], lo[2] + size[2]]) {
                const px = view[0] * a + view[4] * b + view[8] * c + view[12];
                const py = view[1] * a + view[5] * b + view[9] * c + view[13];
                x0 = Math.min(x0, px);
                x1 = Math.max(x1, px);
                y0 = Math.min(y0, py);
                y1 = Math.max(y1, py);
              }
          const near = 0.01;
          const far = reach * 2;
          const mat = mul(ortho(x0, x1, y0, y1, near, far), view);
          const cell = Math.max(x1 - x0, y1 - y0) / DEPTH_SIZE;
          gl.bindFramebuffer(gl.FRAMEBUFFER, depthFb);
          gl.viewport(0, 0, DEPTH_SIZE, DEPTH_SIZE);
          gl.enable(gl.DEPTH_TEST);
          gl.depthFunc(gl.LESS);
          gl.depthMask(true);
          gl.clearDepth(1);
          gl.clear(gl.DEPTH_BUFFER_BIT);
          gl.useProgram(P.depth.prog);
          gl.uniformMatrix4fv(P.depth.u.uMat, false, mat);
          this.drawBus(scene);
          gl.disable(gl.DEPTH_TEST);
          gl.useProgram(P.outside.prog);
          this.netUniforms(P.outside.u);
          gl.uniformMatrix4fv(P.outside.u.uMat, false, mat);
          gl.uniform3fv(P.outside.u.uDir, d);
          gl.uniform1f(P.outside.u.uRange, far - near);
          gl.uniform1f(P.outside.u.uCell, cell);
          gl.uniform1i(P.outside.u.uDepth, 0);
          this.bindTex(0, depthTex);
          this.targets.forEach((t, k) => {
            gl.bindFramebuffer(gl.FRAMEBUFFER, tmp[k].out.fb);
            gl.viewport(0, 0, t.w, t.h);
            this.drawNet(t);
          });
        }
    gl.deleteFramebuffer(depthFb);
    gl.deleteTexture(depthTex);
    // the bleed's sources, then the mask put together; the base as its zones make it
    this.targets.forEach((t, k) => {
      const x = tmp[k];
      gl.useProgram(P.seed.prog);
      gl.uniform1i(P.seed.u.uClass, 0);
      this.bindTex(0, x.cls.tex);
      this.screen(x.b0.fb, t.w, t.h);
      gl.useProgram(P.step.prog);
      gl.uniform1i(P.step.u.uPrev, 0);
      let a = x.b0;
      let b = x.b1;
      for (let r = 0; r < 4; r++) {
        this.bindTex(0, a.tex);
        this.screen(b.fb, t.w, t.h);
        [a, b] = [b, a];
      }
      t.mask = this.tex(t.w, t.h, gl.RGBA8);
      const maskFb = this.fb(t.mask);
      gl.useProgram(P.mask.prog);
      const u = P.mask.u;
      gl.uniform1i(u.uClass, 0);
      gl.uniform1i(u.uOut, 1);
      gl.uniform1i(u.uLoTex, 2);
      gl.uniform1i(u.uHiTex, 3);
      gl.uniform1i(u.uBleed, 4);
      gl.uniform3fv(u.uSize, size);
      this.bindTex(0, x.cls.tex);
      this.bindTex(1, x.out.tex);
      this.bindTex(2, x.lo.tex);
      this.bindTex(3, x.hi.tex);
      this.bindTex(4, a.tex);
      this.screen(maskFb, t.w, t.h);
      gl.deleteFramebuffer(maskFb);
      // the zones
      const mixA = this.tex(t.w, t.h, gl.RGBA8);
      const mixFb = this.fb(mixA);
      gl.useProgram(P.mix.prog);
      gl.uniform1i(P.mix.u.uBase, 0);
      gl.uniform1i(P.mix.u.uLinearise, t.base.linearise ? 1 : 0);
      this.zoneUniforms(P.mix.u, t.zones);
      this.bindTex(0, t.base.tex);
      this.screen(mixFb, t.w, t.h);
      t.mix = this.tex(t.w, t.h, gl.RGBA8);
      const mix2Fb = this.fb(t.mix);
      gl.useProgram(P.specks.prog);
      gl.uniform1i(P.specks.u.uMix, 0);
      gl.uniform1i(P.specks.u.uBase, 1);
      gl.uniform1i(P.specks.u.uLinearise, t.base.linearise ? 1 : 0);
      this.zoneUniforms(P.specks.u, t.zones);
      this.bindTex(0, mixA);
      this.bindTex(1, t.base.tex);
      this.screen(mix2Fb, t.w, t.h);
      gl.deleteFramebuffer(mixFb);
      gl.deleteFramebuffer(mix2Fb);
      gl.deleteTexture(mixA);
      for (const y of Object.values(x)) {
        gl.deleteFramebuffer(y.fb);
        gl.deleteTexture(y.tex);
      }
    });
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** The bus's own geometry (its draw lists, as the view draws it): for the depth maps. */
  drawBus(scene) {
    const gl = this.gl;
    gl.bindVertexArray(scene.vao);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, scene.ibJoined);
    for (const d of [...scene.opaque, ...scene.tested]) gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, d.first * 4);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, scene.ibAll);
    for (const list of scene.blended) for (const d of list) gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, d.first * 4);
    gl.bindVertexArray(null);
  }

  /** Which of the scene's texture slots show each painted texture (and a window's see-through
   * map its own): those get the paint in place of their texture. */
  slotsOfTargets(scene) {
    const roles = this.prep.roles || [];
    for (const t of this.targets) t.slots = new Set();
    scene.head.entries.forEach((e, i) => {
      const r = roles[i];
      if (!r) return;
      e.mats.forEach((mat, k) => {
        const role = r[k];
        if (!mat || !role) return;
        const [t, , , trans] = role;
        const stands = [mat, ...(mat.items || [])];
        for (const s of stands) {
          if (t >= 0 && this.targets[t] && s.tex !== undefined && s.tex !== null) this.targets[t].slots.add(s.tex);
          if (trans >= 0 && this.targets[trans] && s.trans?.tex !== undefined && s.trans?.tex !== null) this.targets[trans].slots.add(s.trans.tex);
          // (a window whose own texture is its see-through map: the same picture)
          if (t >= 0 && this.targets[t] && s.trans && (s.trans.tex === undefined || s.trans.tex === s.tex) && s.tex !== undefined) this.targets[t].slots.add(s.tex);
        }
      });
    });
  }

  // ------------------------------------------------------------ the paint, at every change
  paint() {
    this.dirty = false;
    const gl = this.gl;
    if (!this.targets.length || gl.isContextLost()) return;
    const t0 = performance.now();
    const ops = this.operations();
    if (ops.waiting) this.dirty = true;
    this.uploadOps(ops.list);
    this.brushPaint(ops.brushes);
    const P = this.p;
    gl.disable(gl.BLEND);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.colorMask(true, true, true, true);
    const sc = this.scratch;
    const m = this.prep.dims;
    for (const t of this.targets) {
      // the base first
      gl.disable(gl.DEPTH_TEST);
      gl.useProgram(P.copy.prog);
      gl.uniform1i(P.copy.u.uBase, 0);
      gl.uniform1i(P.copy.u.uLinearise, t.base.linearise ? 1 : 0);
      gl.uniform2f(P.copy.u.uSize, t.w, t.h);
      this.bindTex(0, t.base.tex);
      this.screen(sc.fb, t.w, t.h);
      gl.depthMask(true);
      gl.clearDepth(1);
      gl.clear(gl.DEPTH_BUFFER_BIT);
      if (!this.before) {
        gl.enable(gl.DEPTH_TEST);
        gl.depthFunc(gl.LEQUAL);
        const u = P.paint.u;
        gl.useProgram(P.paint.prog);
        this.netUniforms(u);
        this.zoneUniforms(u, t.zones);
        gl.uniform1i(u.uBase, 0);
        gl.uniform1i(u.uMask, 1);
        gl.uniform1i(u.uMix, 2);
        gl.uniform1i(u.uTpl, 3);
        gl.uniform1i(u.uOps, 4);
        gl.uniform1i(u.uAtlas, 5);
        gl.uniform1i(u.uBrush, 6);
        gl.uniform1i(u.uOpCount, ops.list.length);
        gl.uniform1i(u.uTemplate, t.tpl ? 1 : 0);
        gl.uniform3fv(u.uMuRef, t.muRef || [1, 1, 1]);
        gl.uniform3fv(u.uMin, m.min);
        gl.uniform3fv(u.uMax, m.max);
        gl.uniform1f(u.uAA, 0.6 / Math.max(1, t.density));
        gl.uniform1i(u.uAlpha, t.alpha ? 1 : 0);
        gl.uniform1i(u.uLinearise, t.base.linearise ? 1 : 0);
        gl.uniform1i(u.uBefore, 0);
        this.bindTex(0, t.base.tex);
        this.bindTex(1, t.mask);
        this.bindTex(2, t.mix);
        this.bindTex(3, t.tpl || t.mask);
        this.bindTex(4, this.opsTex);
        this.bindTex(5, this.atlas?.tex || t.mask);
        this.bindTex(6, t.brush?.tex || this.emptyArray(), gl.TEXTURE_2D_ARRAY);
        gl.bindFramebuffer(gl.FRAMEBUFFER, sc.fb);
        gl.viewport(0, 0, t.w, t.h);
        this.drawNet(t);
        gl.disable(gl.DEPTH_TEST);
      }
      // the seams bled, into the texture the bus shows
      gl.useProgram(P.spread.prog);
      gl.uniform1i(P.spread.u.uPaint, 0);
      gl.uniform1i(P.spread.u.uMask, 1);
      gl.uniform1i(P.spread.u.uBase, 2);
      gl.uniform1i(P.spread.u.uLinearise, t.base.linearise ? 1 : 0);
      this.bindTex(0, sc.tex);
      this.bindTex(1, t.mask);
      this.bindTex(2, t.base.tex);
      this.screen(t.outFb, t.w, t.h);
      gl.bindTexture(gl.TEXTURE_2D, t.out);
      gl.generateMipmap(gl.TEXTURE_2D);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.bindVertexArray(null);
    for (let u = 0; u < 7; u++) {
      gl.activeTexture(gl.TEXTURE0 + u);
      gl.bindTexture(gl.TEXTURE_2D, null);
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, null);
    }
    gl.activeTexture(gl.TEXTURE0);
    this.override();
    const ms = performance.now() - t0;
    this.stats.paints.push(ms);
    if (this.stats.paints.length > 120) this.stats.paints.shift();
    this.post({ kind: 'lakPainted', viewer: this.viewer, ms: Math.round(ms * 10) / 10, ops: ops.list.length, waiting: !!ops.waiting });
  }

  emptyArray() {
    if (this.blankArray) return this.blankArray;
    const gl = this.gl;
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, t);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R8, 1, 1, 1);
    this.blankArray = t;
    return t;
  }

  /** The paint in place of the bus's own textures (or theirs again: before and after). */
  override() {
    const tx = this.e.painter?.textures;
    if (!tx) return;
    for (const slot of this.overridden || []) tx.setOverride(slot, undefined);
    this.overridden = [];
    if (this.before || this.state !== 'ready') return;
    for (const t of this.targets) {
      if (!t.out) continue;
      const gpu = { tex: t.out, bytes: Math.round((t.w * t.h * 16) / 3), w: t.w, h: t.h, linearise: false, hasAlpha: !!t.base?.hasAlpha || !!t.alpha || !!t.windowsOnly, state: 'full' };
      for (const slot of t.slots || []) {
        tx.setOverride(slot, gpu);
        this.overridden.push(slot);
      }
    }
  }

  // ------------------------------------------------------------ the operations
  /** The layers as operations (`paint::prepare`); the decals' pictures fetched as needed. */
  operations() {
    const list = [];
    const brushes = [];
    let waiting = false;
    const planeX = this.planeX;
    const density = (this.prep.targets[0]?.density || 100) * 1.25;
    const need = new Map();
    this.lastOf ??= new Map();
    // (a decal whose picture is being drawn anew - sized, its text typed - shows the one before
    // meanwhile, stretched)
    const raster = (key, make, id) => {
      const r = this.rasters.get(key);
      if (r) {
        need.set(key, make);
        this.lastOf.set(id, key);
        return r;
      }
      waiting = true;
      if (!this.fetching.has(key)) this.fetchRaster(key, make);
      const old = this.lastOf.get(id);
      if (old && this.rasters.get(old)) {
        need.set(old, make);
        return this.rasters.get(old);
      }
      return undefined;
    };
    this.layers.forEach((l, i) => {
      if (l.zichtbaar === false || !(l.dekking > 0)) return;
      const common = { layer: i, opacity: Math.min(1, Math.max(0, l.dekking)), detail: Math.min(1, Math.max(0, l.detail ?? 1)), overTrim: !!l.ookOverRubbers, overGlass: !!l.ookOverRamen };
      const colour = (c, g) => ({ a: linearOf(c), grad: g ? (g.soort === 'radiaal' ? 2 : 1) : 0, b: g ? linearOf(g.kleur2) : [0, 0, 0], angle: g ? ((g.hoek || 0) * Math.PI) / 180 : 0 });
      if (l.soort === 'zone') {
        list.push({ ...common, kind: 1, colour: colour(l.kleur, l.verloop), lab: l.centrum || [50, 0, 0], radius: l.straal ?? 1000, group: l.lakgroep ?? -1 });
      } else if (l.soort === 'strook') {
        list.push({ ...common, kind: 2, colour: colour(l.kleur, l.verloop), h1: Math.min(l.h1, l.h2), h2: Math.max(l.h1, l.h2), tan: Math.tan(((l.hoek || 0) * Math.PI) / 180), wave: l.golf || 0, sides: SIDE_NUMBER[l.zijden || 'rondom'] ?? 0 });
      } else if (l.soort === 'penseel') {
        if (!(l.streken || []).length) return;
        brushes.push(l);
        list.push({ ...common, kind: 5, colour: colour(l.kleur), brush: brushes.length - 1 });
      } else if (isDecal(l)) {
        const size = decalSize(l, this.sizes);
        if (!size || !(size[0] > 0 && size[1] > 0)) return;
        const [w, h] = size;
        const key = decalKey(l, w, h);
        let fill;
        let outline;
        let picture;
        if (l.soort === 'afbeelding') {
          picture = raster(`a|${key}`, { cmd: 'lak_picture', args: { id: this.project, hash: l.beeld, white: !!l.witDoorzichtig } }, l.id);
          if (!picture) return;
        } else {
          const r = raster(`d|${key}`, { cmd: 'lak_decal', args: { layer: l, k: density, max: 2048 }, decal: true }, l.id);
          if (!r) return;
          fill = r.fill;
          outline = r.outline;
        }
        const places = [{ place: l.plaats, flip: false, copy: false }];
        if (planeX !== undefined) {
          const m = mirrorPlace(l.plaats, planeX, mirrorImage(l));
          if (m) places.push({ place: m.place, flip: m.flip, copy: true });
        }
        for (const { place, flip, copy } of places) {
          const d = decalOf(place, w, h, flip);
          if (picture) list.push({ ...common, kind: 4, decal: d, raster: picture, copy });
          else {
            if (outline && l.omlijning) list.push({ ...common, kind: 3, decal: d, raster: outline, colour: colour(l.omlijning.kleur), copy });
            list.push({ ...common, kind: 3, decal: d, raster: fill, colour: colour(l.kleur, l.verloop), copy });
          }
        }
      }
    });
    this.packAtlas(need);
    return { list, brushes, waiting };
  }

  async fetchRaster(key, make) {
    this.fetching.add(key);
    try {
      const buf = await this.e.fetch(make.cmd, make.args);
      const d = new DataView(buf);
      const read = (at) => {
        const w = d.getUint32(at, true);
        const h = d.getUint32(at + 4, true);
        const ch = d.getUint32(at + 8, true);
        const px = new Uint8Array(buf, at + 12, w * h * ch);
        // (one channel: a mask, white with its alpha)
        let rgba = px;
        if (ch === 1) {
          rgba = new Uint8Array(w * h * 4);
          for (let i = 0; i < w * h; i++) {
            rgba[i * 4] = 255;
            rgba[i * 4 + 1] = 255;
            rgba[i * 4 + 2] = 255;
            rgba[i * 4 + 3] = px[i];
          }
        } else rgba = new Uint8Array(px);
        return { r: w && h ? { w, h, data: rgba } : undefined, next: at + 12 + w * h * ch };
      };
      let value;
      if (make.decal) {
        const f = read(8);
        const o = read(f.next);
        value = { fill: f.r, outline: o.r };
      } else value = read(0).r;
      this.rasters.set(key, value);
      // (the rasters of a hundred keys kept at most)
      if (this.rasters.size > 100) this.rasters.delete(this.rasters.keys().next().value);
    } catch (x) {
      this.post({ kind: 'lakNote', viewer: this.viewer, text: String(x?.message || x) });
      this.rasters.set(key, make.decal ? { fill: undefined, outline: undefined } : undefined);
    } finally {
      this.fetching.delete(key);
      this.dirty = true;
      this.e.plan();
    }
  }

  /** The decals' pictures into one atlas (shelves, the tallest first; all smaller when they do
   * not fit), each with its rectangle. */
  packAtlas(need) {
    const gl = this.gl;
    const items = [];
    for (const key of need.keys()) {
      const v = this.rasters.get(key);
      if (!v) continue;
      if (v.fill || v.outline) {
        if (v.fill) items.push({ key: `${key}|f`, r: v.fill });
        if (v.outline) items.push({ key: `${key}|o`, r: v.outline });
      } else if (v.w) items.push({ key, r: v });
    }
    const sig = items.map((x) => x.key).join('\n');
    if (sig === this.atlasKeys) return;
    this.atlasKeys = sig;
    if (!this.atlas) {
      const tex = this.tex(ATLAS_W, ATLAS_H, gl.SRGB8_ALPHA8, true);
      this.atlas = { tex, fb: this.fb(tex) };
    }
    const order = [...items].sort((a, b) => b.r.h - a.r.h);
    let s = 1;
    let rects;
    for (let tries = 0; tries < 12; tries++) {
      rects = new Map();
      let x = 0;
      let y = 0;
      let row = 0;
      let ok = true;
      for (const it of order) {
        const w = Math.max(1, Math.round(it.r.w * s)) + 2;
        const h = Math.max(1, Math.round(it.r.h * s)) + 2;
        if (x + w > ATLAS_W) {
          x = 0;
          y += row;
          row = 0;
        }
        if (w > ATLAS_W || y + h > ATLAS_H) {
          ok = false;
          break;
        }
        rects.set(it.key, [x + 1, y + 1, w - 2, h - 2]);
        x += w;
        row = Math.max(row, h);
      }
      if (ok) break;
      s *= 0.8;
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.atlas.fb);
    gl.viewport(0, 0, ATLAS_W, ATLAS_H);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    for (const it of items) {
      const rc = rects.get(it.key);
      if (!rc) continue;
      if (rc[2] === it.r.w && rc[3] === it.r.h) {
        gl.bindTexture(gl.TEXTURE_2D, this.atlas.tex);
        gl.texSubImage2D(gl.TEXTURE_2D, 0, rc[0], rc[1], it.r.w, it.r.h, gl.RGBA, gl.UNSIGNED_BYTE, it.r.data);
      } else {
        const tmp = this.tex(it.r.w, it.r.h, gl.SRGB8_ALPHA8, true);
        gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, it.r.w, it.r.h, gl.RGBA, gl.UNSIGNED_BYTE, it.r.data);
        const from = this.fb(tmp);
        gl.bindFramebuffer(gl.READ_FRAMEBUFFER, from);
        gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, this.atlas.fb);
        gl.blitFramebuffer(0, 0, it.r.w, it.r.h, rc[0], rc[1], rc[0] + rc[2], rc[1] + rc[3], gl.COLOR_BUFFER_BIT, gl.LINEAR);
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.deleteFramebuffer(from);
        gl.deleteTexture(tmp);
      }
      // (half a texel in from the edge: no neighbour's colour at the decal's rim)
      it.r.rect = [(rc[0] + 0.5) / ATLAS_W, (rc[1] + 0.5) / ATLAS_H, (rc[0] + rc[2] - 0.5) / ATLAS_W, (rc[1] + rc[3] - 0.5) / ATLAS_H];
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** The operations' table (RGBA32F, `OP_TEXELS` texels a row). */
  uploadOps(list) {
    const gl = this.gl;
    const W = S.OP_TEXELS;
    const rows = Math.max(1, list.length);
    const f = new Float32Array(rows * W * 4);
    list.forEach((o, j) => {
      const at = (k, v) => f.set(v, (j * W + k) * 4);
      at(0, [o.kind, o.opacity, o.detail, (o.overTrim ? 1 : 0) | (o.overGlass ? 2 : 0) | (o.copy ? 4 : 0)]);
      const c = o.colour;
      if (c) {
        at(1, [...c.a, c.grad]);
        at(2, [...c.b, c.angle]);
      }
      if (o.kind === 1) {
        at(3, [...o.lab, o.radius]);
        at(4, [o.group, 0, 0, 0]);
      } else if (o.kind === 2) {
        at(3, [o.h1, o.h2, o.tan, o.wave]);
        at(4, [o.sides, 0, 0, 0]);
      } else if (o.kind === 5) {
        at(5, [0, 0, 0, o.brush]);
      } else if (o.decal) {
        const d = o.decal;
        at(3, [...d.centre, d.w]);
        at(4, [...d.u, d.h]);
        at(5, [...d.v, d.flip ? 1 : 0]);
        at(6, [...d.n, d.sin]);
        at(7, [...d.lo, d.cos]);
        at(8, [...d.hi, 0]);
        at(9, o.raster?.rect || [0, 0, 0, 0]);
      }
    });
    if (!this.opsTex) {
      this.opsTex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, this.opsTex);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    }
    gl.bindTexture(gl.TEXTURE_2D, this.opsTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, W, rows, 0, gl.RGBA, gl.FLOAT, f);
  }

  // ------------------------------------------------------------ the brush
  /** Each brush layer's cover per texture: its strokes again when they changed, only the last
   * when the player is painting it. */
  brushPaint(layers) {
    const gl = this.gl;
    const n = layers.length;
    for (const t of this.targets) {
      if (n === 0) continue;
      if (!t.brush || t.brush.n !== n) {
        if (t.brush?.tex) gl.deleteTexture(t.brush.tex);
        if (t.brush?.before) {
          gl.deleteTexture(t.brush.before.tex);
          gl.deleteFramebuffer(t.brush.before.fb);
        }
        const tex = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D_ARRAY, tex);
        gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R8, t.w, t.h, n);
        gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        t.brush = { tex, n, sig: new Array(n).fill('') };
      }
    }
    if (n === 0) return;
    if (!this.strokeBuf) {
      const w = Math.max(...this.targets.map((t) => t.w));
      const h = Math.max(...this.targets.map((t) => t.h));
      const tex = this.tex(w, h, gl.R8);
      this.strokeBuf = { tex, fb: this.fb(tex) };
    }
    const planeX = this.planeX;
    layers.forEach((l, k) => {
      // every stroke with its copy on the other side right after it (an eraser takes both)
      const strokes = [];
      for (const st of l.streken || []) {
        const o = strokeO3d(st);
        strokes.push(o);
        if (planeX !== undefined) strokes.push({ ...o, punten: o.punten.map((p) => [2 * planeX - p[0], p[1], p[2]]), normalen: o.normalen.map((q) => [-q[0], q[1], q[2]]) });
      }
      const sigOf = (list) => list.map((s) => `${s.punten.length}|${s.straalCm}|${s.hardheid}|${s.dekking}|${s.gum}|${s.punten[0]}|${s.punten[s.punten.length - 1]}`).join(';');
      const all = sigOf(strokes);
      for (const t of this.targets) {
        if (t.brush.sig[k] === all) continue;
        const per = planeX !== undefined ? 2 : 1;
        const earlier = sigOf(strokes.slice(0, Math.max(0, strokes.length - per)));
        const layerFb = this.fb(t.brush.tex, k);
        gl.viewport(0, 0, t.w, t.h);
        if (this.live === l.id && !t.brush.before) {
          const tex = this.tex(t.w, t.h, gl.R8);
          t.brush.before = { tex, fb: this.fb(tex) };
        }
        if (this.live === l.id && t.brush.base?.[k] === earlier && strokes.length >= per) {
          // the stroke being painted: on the cover before it
          this.copyR8(t.brush.before.fb, layerFb, t);
          for (const s of strokes.slice(strokes.length - per)) this.strokeInto(t, s, layerFb);
        } else {
          gl.bindFramebuffer(gl.FRAMEBUFFER, layerFb);
          gl.clearColor(0, 0, 0, 0);
          gl.clear(gl.COLOR_BUFFER_BIT);
          strokes.forEach((s, i) => {
            if (i === strokes.length - per && this.live === l.id) {
              // (the cover before the stroke being painted, for its next points)
              this.copyR8(layerFb, t.brush.before.fb, t);
              t.brush.base ??= [];
              t.brush.base[k] = earlier;
            }
            this.strokeInto(t, s, layerFb);
          });
        }
        gl.deleteFramebuffer(layerFb);
        t.brush.sig[k] = all;
      }
    });
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  copyR8(from, to, t) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, from);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, to);
    gl.blitFramebuffer(0, 0, t.w, t.h, 0, 0, t.w, t.h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** One stroke into a brush layer's cover: its pieces of 64 points into the stroke's own cover
   * (MAX), then that over the layer's (the brush) or out of it (the eraser). */
  strokeInto(t, s, layerFb) {
    const gl = this.gl;
    const P = this.p;
    const pts = s.punten;
    if (!pts.length) return;
    const r = Math.max(0.002, (s.straalCm || 10) / 100);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.strokeBuf.fb);
    gl.viewport(0, 0, t.w, t.h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.MAX);
    gl.useProgram(P.stroke.prog);
    this.netUniforms(P.stroke.u);
    gl.uniform3f(P.stroke.u.uStroke, r, Math.min(1, Math.max(0, s.hardheid ?? 0.5)), Math.min(1, Math.max(0, s.dekking ?? 1)));
    const P4 = new Float32Array(256);
    const N4 = new Float32Array(256);
    for (let a = 0; a < pts.length; a += 63) {
      const b = Math.min(pts.length, a + 64);
      const lo = [Infinity, Infinity, Infinity];
      const hi = [-Infinity, -Infinity, -Infinity];
      P4.fill(0);
      N4.fill(0);
      for (let i = a; i < b; i++) {
        const p = pts[i];
        const nrm = s.normalen?.[i] || [0, 0, 0];
        P4.set([p[0], p[1], p[2], 0], (i - a) * 4);
        N4.set([nrm[0], nrm[1], nrm[2], 0], (i - a) * 4);
        for (let c = 0; c < 3; c++) {
          lo[c] = Math.min(lo[c], p[c] - r);
          hi[c] = Math.max(hi[c], p[c] + r);
        }
      }
      gl.uniform4fv(P.stroke.u.uPts, P4);
      gl.uniform4fv(P.stroke.u.uNrm, N4);
      gl.uniform1i(P.stroke.u.uCount, b - a);
      gl.uniform3fv(P.stroke.u.uLo, lo);
      gl.uniform3fv(P.stroke.u.uHi, hi);
      this.drawNet(t);
      if (b === pts.length) break;
    }
    gl.blendEquation(gl.FUNC_ADD);
    if (s.gum) gl.blendFunc(gl.ZERO, gl.ONE_MINUS_SRC_COLOR);
    else gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_COLOR);
    gl.useProgram(P.layer.prog);
    gl.uniform1i(P.layer.u.uSrc, 0);
    this.bindTex(0, this.strokeBuf.tex);
    this.screen(layerFb, t.w, t.h);
    gl.disable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ZERO);
  }

  // ------------------------------------------------------------ pointing at the bus
  /**
   * What lies under a point of the picture (`x`, `y` in NDC of viewer `viewer`), or along a ray
   * (`from`, `dir` in the o3d frame): its place and normal (o3d, facing the viewer), and on a
   * painted texture its target, UV and the base's colour there (sRGB).
   */
  pick(m) {
    const gl = this.gl;
    const scene = this.e.painter?.scene;
    if (!scene || !this.net || !this.floatOk) return { hit: null };
    let mat;
    let eye;
    if (m.from) {
      const w = [-m.from[0], m.from[1], m.from[2]];
      const d = [-m.dir[0], m.dir[1], m.dir[2]];
      const view = lookAt(w, [w[0] + d[0], w[1] + d[1], w[2] + d[2]], Math.abs(d[1]) > 0.9 ? [0, 0, 1] : [0, 1, 0]);
      mat = mul(ortho(-0.002, 0.002, -0.002, 0.002, 0, m.reach || 5), view);
      eye = m.from;
    } else {
      const v = this.e.viewers.get(m.viewer ?? this.viewer);
      if (!v || v.w < 2) return { hit: null };
      const c = v.camera.view(v.w / v.h);
      const W = v.w;
      const H = v.h;
      const t = new Float32Array(16);
      t[0] = W;
      t[5] = H;
      t[10] = 1;
      t[15] = 1;
      t[12] = -m.x * W;
      t[13] = -m.y * H;
      mat = mul(t, c.viewProj);
      eye = [-c.eye[0], c.eye[1], c.eye[2]];
    }
    if (!this.pickFb) {
      const fb = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
      const texs = [0, 1, 2].map((i) => {
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA32F, 1, 1);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0 + i, gl.TEXTURE_2D, t, 0);
        return t;
      });
      const depth = gl.createRenderbuffer();
      gl.bindRenderbuffer(gl.RENDERBUFFER, depth);
      gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT24, 1, 1);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, depth);
      const sampleTex = this.tex(1, 1, gl.RGBA8);
      this.pickFb = { fb, texs, depth, sampleTex, sampleFb: this.fb(sampleTex) };
    }
    const P = this.p;
    const f = this.pickFb;
    gl.bindFramebuffer(gl.FRAMEBUFFER, f.fb);
    gl.viewport(0, 0, 1, 1);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1, gl.COLOR_ATTACHMENT2]);
    gl.clearBufferfv(gl.COLOR, 0, [0, 0, 0, 0]);
    gl.clearBufferfv(gl.COLOR, 1, [0, 0, 0, 0]);
    gl.clearBufferfv(gl.COLOR, 2, [0, 0, 0, 0]);
    gl.depthMask(true);
    gl.clearDepth(1);
    gl.clear(gl.DEPTH_BUFFER_BIT);
    gl.disable(gl.BLEND);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.useProgram(P.pick.prog);
    gl.uniformMatrix4fv(P.pick.u.uMat, false, mat);
    gl.uniform1f(P.pick.u.uTarget, 0);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    this.drawBus(scene);
    // the painted textures' texels where the bus was hit
    gl.drawBuffers([gl.NONE, gl.NONE, gl.COLOR_ATTACHMENT2]);
    gl.enable(gl.POLYGON_OFFSET_FILL);
    gl.polygonOffset(-1, -4);
    gl.depthMask(false);
    for (const t of this.targets) {
      gl.uniform1f(P.pick.u.uTarget, t.k + 1);
      this.drawNet(t);
    }
    gl.depthMask(true);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.disable(gl.DEPTH_TEST);
    const read = (i) => {
      const px = new Float32Array(4);
      gl.readBuffer(gl.COLOR_ATTACHMENT0 + i);
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.FLOAT, px);
      return px;
    };
    const a = read(0);
    const b = read(1);
    const c = read(2);
    gl.readBuffer(gl.COLOR_ATTACHMENT0);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0]);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (a[3] < 0.5) return { hit: null };
    const p = [a[0], a[1], a[2]];
    let n = [b[0], b[1], b[2]];
    const l = Math.hypot(...n) || 1;
    n = n.map((x) => x / l);
    const look = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
    const facing = m.from ? [-m.dir[0], -m.dir[1], -m.dir[2]] : look;
    if (n[0] * facing[0] + n[1] * facing[1] + n[2] * facing[2] > 0) n = n.map((x) => -x);
    const hit = { p, n };
    const k = Math.round(c[0]) - 1;
    const t = this.targets[k];
    if (c[3] > 0.5 && t) {
      const uv = [c[1] - Math.floor(c[1]), c[2] - Math.floor(c[2])];
      hit.target = k;
      hit.uv = uv;
      gl.useProgram(P.sample.prog);
      gl.uniform1i(P.sample.u.uBase, 0);
      gl.uniform2f(P.sample.u.uAt, uv[0], uv[1]);
      gl.uniform1i(P.sample.u.uLinearise, t.base.linearise ? 1 : 0);
      this.bindTex(0, t.base.tex);
      this.screen(f.sampleFb, 1, 1);
      const px = new Uint8Array(4);
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
      hit.colour = [px[0], px[1], px[2]];
      // whether the texel is one both sides share (a decal there shows on both)
      const mx = Math.min(t.w - 1, Math.floor(uv[0] * t.w));
      const my = Math.min(t.h - 1, Math.floor(uv[1] * t.h));
      const maskFb = this.fb(t.mask);
      const mp = new Uint8Array(4);
      gl.readPixels(mx, my, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, mp);
      gl.deleteFramebuffer(maskFb);
      hit.shared = (mp[1] & 16) !== 0;
      hit.glass = (mp[1] & 1) !== 0;
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    }
    this.e.plan();
    return { hit };
  }

  measure() {
    const q = (list, f) => {
      if (!list.length) return 0;
      const s = [...list].sort((a, b) => a - b);
      return Math.round(s[Math.min(s.length - 1, Math.floor(f * (s.length - 1)))] * 10) / 10;
    };
    let bytes = 0;
    for (const t of this.targets) bytes += t.w * t.h * (4 + 4 + 16 / 3) + (t.brush ? t.w * t.h * t.brush.n : 0);
    if (this.scratch) bytes += this.scratch.w * this.scratch.h * 6;
    if (this.atlas) bytes += ATLAS_W * ATLAS_H * 4;
    return { state: this.state, build: this.stats.builds[this.stats.builds.length - 1], paintP50: q(this.stats.paints, 0.5), paintP95: q(this.stats.paints, 0.95), paints: this.stats.paints.length, gpuBytes: Math.round(bytes), targets: this.targets.map((t) => [t.w, t.h]) };
  }
}

/** A decal's place for the shader (`Decal::new`): its axes, its turn, its box. */
function decalOf(place, w, h, flip) {
  const ax = SIDES[place.zijde] || SIDES.R;
  const r = ((place.draai || 0) * Math.PI) / 180;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (const c of corners(place, w, h))
    for (const k of [-0.8, 0.8])
      for (let a = 0; a < 3; a++) {
        const v = c[a] + ax.n[a] * k;
        lo[a] = Math.min(lo[a], v);
        hi[a] = Math.max(hi[a], v);
      }
  return { centre: place.midden, u: ax.u, v: ax.v, n: ax.n, sin: Math.sin(r), cos: Math.cos(r), w, h, flip, lo, hi };
}

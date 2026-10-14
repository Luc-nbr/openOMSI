// Drawing a bus (Omsi-Hub's Bus3D, the old app's `bus3d/teken.ts`).
//
// One WebGL2 context. Each picture:
//  1. (only when the model, what shows or the light changed) the shadow map of 2048² in
//     DEPTH32F, and once a model the contact shadow from below (256², blurred twice);
//  2. OMSI's sky with its clouds, and the matt grey floor running into the horizon;
//  3. the bus in three passes: opaque, alpha test (alpha to coverage), and blended in the
//     cfg's order (each part from the back to the front, `[isshadow]` first), without writing
//     depth where the cfg says `[matl_noZwrite]`.
//
// GEOMETRY: the pack comes as one buffer; each part's offset (an articulated bus's rear
// section) is added here, an entry whose animation holds it elsewhere at rest gets a moved
// copy, and everything goes into one vertex buffer. Opaque and alpha-tested pieces are joined
// per material into one draw: at another visibility their indices are put after each other
// again (tens of ms, only on a change of livery). Blending goes entry by entry, in the cfg's
// order, as OMSI does.

import { BLUR_FS, BUS_FS, BUS_VS, CONTACT_FS, CONTACT_VS, FLOOR_FS, FLOOR_VS, program, SCREEN_VS, SHADOW_FS, SHADOW_VS, SKY_FS } from './shaders.js';
import { Textures } from './textures.js';
import { inverse, lookAt, mul, ortho, project, rad, unit } from './math.js';

/** The fixed noon sun of the view: 38° up, at the front right seen from the beginning. */
export const SUN = { height: 38, turn: 228 };
/** Light strengths (linear), calibrated by the old app on the white O560 and the beige SD77. */
export const LIGHT = {
  sun: [1.0, 0.97, 0.9],
  sky: [0.41, 0.45, 0.52],
  ambient: [0.22, 0.22, 0.24],
  exposure: 1.4,
  gloss: 0.35,
  floor: [0.12, 0.12, 0.12],
  floorRadius: 400,
  /** How much sky light is left in the shade (openOMSI: 0.6). */
  shadeSky: 0.8,
  /** The least reflection on glass (see `matFor`). */
  glassMin: 0.6,
};
const SHADOW_SIZE = 2048;
const CONTACT_SIZE = 256;
const BLOCK_FLOATS = 16 * 3 + 4 * 10;

/** `entry * 4096 + slot`: the key of a slot in a look's maps. */
export const slotKey = (entry, slot) => entry * 4096 + slot;

/** Read a pack's head and where its tail begins. */
export function readPack(bytes) {
  if (bytes.byteLength < 8 || String.fromCharCode(bytes[0], bytes[1], bytes[2], bytes[3]) !== 'B3D2') throw new Error('not a B3D2 pack');
  const n = new DataView(bytes.buffer, bytes.byteOffset, 8).getUint32(4, true);
  if (8 + n > bytes.byteLength) throw new Error('the pack is cut off');
  const head = JSON.parse(new TextDecoder().decode(bytes.subarray(8, 8 + n)));
  return { head, tail: bytes.subarray(8 + n) };
}

/** Whether an entry shows in a view: 'outside' (viewpoint 0 or bit 1) or 'inside' (0 or bit 2). */
export const inView = (e, view) => e.vp === 0 || (view === 'inside' ? (e.vp & 2) !== 0 : (e.vp & 1) !== 0);

export class Painter {
  constructor(gl, can, onChange) {
    this.gl = gl;
    this.can = can;
    this.textures = new Textures(gl, can, onChange);
    const z = rad(SUN.height);
    const d = rad(SUN.turn);
    this.sunDir = unit([Math.cos(z) * Math.sin(d), Math.sin(z), -Math.cos(z) * Math.cos(d)]);
    this.sunAzimuth = Math.atan2(this.sunDir[0], -this.sunDir[2]);
    this.block = new Float32Array(BLOCK_FLOATS);
    this.shadowMat = new Float32Array(16);
    this.contactArea = { minX: -8, minZ: -8, sizeX: 16, sizeZ: 16 };
    this.shadowReach = { width: 16, depth: 80 };
    this.seed = 0;
    this.scene = undefined;
    this.env = undefined;
    this.view = 'outside';
    this.idTable = [];
    this.makeAll();
  }

  /** Programs, buffers and targets; again after a lost context. */
  makeAll() {
    const gl = this.gl;
    const bind = (p) => {
      const i = gl.getUniformBlockIndex(p, 'Frame');
      if (i !== gl.INVALID_INDEX) gl.uniformBlockBinding(p, i, 0);
    };
    const bp = program(gl, BUS_VS, BUS_FS);
    bind(bp);
    const names = ['uTex', 'uTrans', 'uMask', 'uSkyTex', 'uCloudTex', 'uShadowMap', 'uDiffuse', 'uSpec', 'uEmissive', 'uMode', 'uTexMode', 'uTransMode', 'uMaskMode', 'uGlass', 'uFlat', 'uId', 'uEnv', 'uAlphaScale'];
    const u = {};
    for (const n of names) u[n] = gl.getUniformLocation(bp, n);
    this.bus = { prog: bp, u };
    gl.useProgram(bp);
    gl.uniform1i(u.uTex, 0);
    gl.uniform1i(u.uTrans, 1);
    gl.uniform1i(u.uMask, 2);
    gl.uniform1i(u.uShadowMap, 3);
    gl.uniform1i(u.uSkyTex, 4);
    gl.uniform1i(u.uCloudTex, 5);
    const sp = program(gl, SHADOW_VS, SHADOW_FS);
    bind(sp);
    this.shadowProg = { prog: sp, tex: gl.getUniformLocation(sp, 'uTex'), test: gl.getUniformLocation(sp, 'uAlphaTest') };
    const cp = program(gl, CONTACT_VS, CONTACT_FS);
    this.contactProg = { prog: cp, mat: gl.getUniformLocation(cp, 'uContactMat') };
    const vp = program(gl, SCREEN_VS, BLUR_FS);
    this.blurProg = { prog: vp, src: gl.getUniformLocation(vp, 'uSrc'), step: gl.getUniformLocation(vp, 'uStep') };
    const hp = program(gl, SCREEN_VS, SKY_FS);
    bind(hp);
    this.skyProg = { prog: hp };
    gl.useProgram(hp);
    gl.uniform1i(gl.getUniformLocation(hp, 'uSkyTex'), 4);
    gl.uniform1i(gl.getUniformLocation(hp, 'uCloudTex'), 5);
    const lp = program(gl, FLOOR_VS, FLOOR_FS);
    bind(lp);
    this.floorProg = { prog: lp, photo: gl.getUniformLocation(lp, 'uPhoto') };
    gl.useProgram(lp);
    gl.uniform1i(gl.getUniformLocation(lp, 'uShadowMap'), 3);
    gl.uniform1i(gl.getUniformLocation(lp, 'uSkyTex'), 4);
    gl.uniform1i(gl.getUniformLocation(lp, 'uCloudTex'), 5);
    gl.uniform1i(gl.getUniformLocation(lp, 'uContactTex'), 6);

    this.ubo = gl.createBuffer();
    gl.bindBuffer(gl.UNIFORM_BUFFER, this.ubo);
    gl.bufferData(gl.UNIFORM_BUFFER, this.block.byteLength, gl.DYNAMIC_DRAW);
    gl.bindBufferBase(gl.UNIFORM_BUFFER, 0, this.ubo);
    this.emptyVao = gl.createVertexArray();
    this.floorVao = gl.createVertexArray();
    gl.bindVertexArray(this.floorVao);
    const fb = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, fb);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    // a white 1x1 for units that have nothing (the shader does not look at them)
    this.blank = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.blank);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    // the shadow map: 2048² DEPTH32F with comparison (sampler2DShadow), 16 MB
    const st = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, st);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.DEPTH_COMPONENT32F, SHADOW_SIZE, SHADOW_SIZE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_FUNC, gl.LEQUAL);
    const sf = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, sf);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, st, 0);
    this.shadowMap = { tex: st, fb: sf };
    // the contact shadow: two R8 targets to blur, and a depth to keep the lowest
    const ct = [];
    const cf = [];
    const depth = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, depth);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT16, CONTACT_SIZE, CONTACT_SIZE);
    for (let i = 0; i < 2; i++) {
      const t = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texStorage2D(gl.TEXTURE_2D, 1, gl.R8, CONTACT_SIZE, CONTACT_SIZE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      const f = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
      if (i === 0) gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, depth);
      ct.push(t);
      cf.push(f);
    }
    this.contact = { tex: ct, fb: cf, depth };
    gl.bindFramebuffer(gl.FRAMEBUFFER, cf[0]);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** OMSI's sky and clouds (`b3d_env`). */
  setEnv(env) {
    this.env = env;
    if (env?.sky) this.textures.setLoose('sky', { ...env.sky, area: 1, uv: 1 });
    if (env?.clouds) this.textures.setLoose('clouds', { ...env.clouds, area: 1, uv: 1 });
  }

  // ------------------------------------------------------------ loading a bus
  /**
   * The pack: fetched, each part's offset added, moved copies of what rests elsewhere, onto the
   * GPU. `pause` lets a picture be drawn every 16 ms; `valid` says whether this bus is still
   * wanted; `onHead` hears the head as soon as it is there (the textures may begin).
   */
  async load(manifest, look, fetchPack, pause, valid = () => true, onHead) {
    const bytes = new Uint8Array(await fetchPack(manifest.pack));
    const { head, tail } = readPack(bytes);
    if (head.id !== manifest.pack) throw new Error('the pack is not this manifest’s');
    onHead?.(head);
    const gl = this.gl;
    const parts = manifest.parts;
    const xf = look?.xf || {};
    // where each piece's vertices and indices go, then the moved copies of entries
    let verts = 0;
    let idx = 0;
    const pieceBase = head.pieces.map((p) => {
      const b = { vert: verts, index: idx };
      verts += p.n;
      idx += p.idx.len / p.wide;
      return b;
    });
    const entryBase = head.entries.map(() => undefined);
    head.entries.forEach((e, i) => {
      if (!xf[i]) return;
      const p = head.pieces[e.piece];
      entryBase[i] = { vert: verts, index: idx };
      verts += p.n;
      idx += p.idx.len / p.wide;
    });
    const vb = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, vb);
    gl.bufferData(gl.ARRAY_BUFFER, verts * 32, gl.STATIC_DRAW);
    const allIndices = new Uint32Array(idx);
    const partMin = parts.map(() => [Infinity, Infinity, Infinity]);
    const partMax = parts.map(() => [-Infinity, -Infinity, -Infinity]);
    let triangles = 0;
    let time = performance.now();
    const put = async (p, base, m) => {
      const src = new Float32Array(tail.buffer, tail.byteOffset + p.verts.off, p.n * 8);
      const f = new Float32Array(src);
      const part = parts[p.part] || parts[0];
      const off = part?.offset || [0, 0, 0];
      const turned = part?.turned;
      const lo = partMin[p.part] || partMin[0];
      const hi = partMax[p.part] || partMax[0];
      for (let o = 0; o < f.length; o += 8) {
        let x = f[o];
        let y = f[o + 1];
        let z = f[o + 2];
        let nx = f[o + 3];
        let ny = f[o + 4];
        let nz = f[o + 5];
        if (m) {
          const X = m[0] * x + m[4] * y + m[8] * z + m[12];
          const Y = m[1] * x + m[5] * y + m[9] * z + m[13];
          const Z = m[2] * x + m[6] * y + m[10] * z + m[14];
          const NX = m[0] * nx + m[4] * ny + m[8] * nz;
          const NY = m[1] * nx + m[5] * ny + m[9] * nz;
          const NZ = m[2] * nx + m[6] * ny + m[10] * nz;
          x = X;
          y = Y;
          z = Z;
          nx = NX;
          ny = NY;
          nz = NZ;
        }
        if (turned) {
          x = -x;
          z = -z;
          nx = -nx;
          nz = -nz;
        }
        f[o] = x + off[0];
        f[o + 1] = y + off[1];
        f[o + 2] = z + off[2];
        f[o + 3] = nx;
        f[o + 4] = ny;
        f[o + 5] = nz;
        if ((o & 63) === 0) {
          // a sample for the part's middle (the blending order)
          const wx = -f[o];
          if (wx < lo[0]) lo[0] = wx;
          if (wx > hi[0]) hi[0] = wx;
          if (f[o + 1] < lo[1]) lo[1] = f[o + 1];
          if (f[o + 1] > hi[1]) hi[1] = f[o + 1];
          if (f[o + 2] < lo[2]) lo[2] = f[o + 2];
          if (f[o + 2] > hi[2]) hi[2] = f[o + 2];
        }
      }
      // (bound again: a picture may have been drawn meanwhile)
      gl.bindBuffer(gl.ARRAY_BUFFER, vb);
      gl.bufferSubData(gl.ARRAY_BUFFER, base.vert * 32, f);
      const ib = tail.subarray(p.idx.off, p.idx.off + p.idx.len);
      const n = p.idx.len / p.wide;
      const srcIdx = p.wide === 2 ? new Uint16Array(ib.buffer, ib.byteOffset, n) : new Uint32Array(ib.buffer, ib.byteOffset, n);
      for (let i = 0; i < n; i++) allIndices[base.index + i] = srcIdx[i] + base.vert;
      if (!m) triangles += n / 3;
      if (performance.now() - time > 16) {
        await pause();
        time = performance.now();
      }
    };
    for (let s = 0; s < head.pieces.length; s++) await put(head.pieces[s], pieceBase[s], null);
    for (let i = 0; i < head.entries.length; i++) if (entryBase[i]) await put(head.pieces[head.entries[i].piece], entryBase[i], xf[i]);
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, vb);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 32, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 32, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 32, 24);
    const ibAll = gl.createBuffer();
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ibAll);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, allIndices, gl.STATIC_DRAW);
    const ibJoined = gl.createBuffer();
    gl.bindVertexArray(null);
    if (!valid()) {
      gl.deleteVertexArray(vao);
      gl.deleteBuffer(vb);
      gl.deleteBuffer(ibAll);
      gl.deleteBuffer(ibJoined);
      return false;
    }
    const d = manifest;
    const box = { min: [-d.max[0], d.min[1], d.min[2]], max: [-d.min[0], d.max[1], d.max[2]] };
    const partMid = parts.map((_, i) => (Number.isFinite(partMin[i][0]) ? [(partMin[i][0] + partMax[i][0]) / 2, (partMin[i][1] + partMax[i][1]) / 2, (partMin[i][2] + partMax[i][2]) / 2] : [0, 0, 0]));
    this.forgetBus();
    this.scene = {
      manifest,
      head,
      vao,
      vb,
      ibAll,
      ibJoined,
      allIndices,
      pieceBase,
      entryBase,
      /** Where entry `i`'s indices begin in `allIndices` (its own moved copy, or its piece's). */
      start: head.entries.map((e, i) => (entryBase[i] || pieceBase[e.piece]).index),
      xfKey: JSON.stringify(xf),
      box,
      partMid,
      opaque: [],
      tested: [],
      blobs: [],
      blended: parts.map(() => []),
      needed: new Set(),
      look: undefined,
      textures: manifest.textures,
      shadowDirty: true,
      contactDirty: true,
      bytes: { verts: verts * 32, indices: idx * 4, joined: 0 },
      triangles,
      shown: 0,
    };
    this.setAreas();
    return true;
  }

  forgetBus() {
    const s = this.scene;
    if (!s) return;
    const gl = this.gl;
    gl.deleteVertexArray(s.vao);
    gl.deleteBuffer(s.vb);
    gl.deleteBuffer(s.ibAll);
    gl.deleteBuffer(s.ibJoined);
    this.scene = undefined;
  }

  /** The shadow map and the contact map on this bus's box. */
  setAreas() {
    const s = this.scene;
    if (!s) return;
    const lo = [s.box.min[0] - 1, Math.min(s.box.min[1], 0) - 0.2, s.box.min[2] - 1];
    const hi = [s.box.max[0] + 1, s.box.max[1] + 1, s.box.max[2] + 1];
    const mid = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
    const L = this.sunDir;
    const look = lookAt([mid[0] + L[0] * 100, mid[1] + L[1] * 100, mid[2] + L[2] * 100], mid);
    let x0 = Infinity;
    let x1 = -Infinity;
    let y0 = Infinity;
    let y1 = -Infinity;
    let z0 = Infinity;
    let z1 = -Infinity;
    for (const x of [lo[0], hi[0]])
      for (const y of [lo[1], hi[1]])
        for (const z of [lo[2], hi[2]]) {
          const p = project(look, [x, y, z]);
          x0 = Math.min(x0, p[0]);
          x1 = Math.max(x1, p[0]);
          y0 = Math.min(y0, p[1]);
          y1 = Math.max(y1, p[1]);
          z0 = Math.min(z0, p[2]);
          z1 = Math.max(z1, p[2]);
        }
    // far enough behind the bus that the floor in its shadow falls in too
    this.shadowMat = mul(ortho(x0, x1, y0, y1, -z1 - 1, -z0 + 60), look);
    this.shadowReach = { width: Math.max(x1 - x0, y1 - y0), depth: -z0 + 60 - (-z1 - 1) };
    this.contactArea = { minX: lo[0], minZ: lo[2], sizeX: hi[0] - lo[0], sizeZ: hi[2] - lo[2] };
  }

  /** The textures loading already while the pack still comes and is unpacked: the plan of
   * what this look shows. The real plan follows with the draw lists. */
  earlyPlan(head, manifest, look, budget) {
    const textures = this.texturesOf(manifest, look);
    const needed = new Set();
    const vis = look?.visible || '';
    head.entries.forEach((e, i) => {
      if (!inView(e, this.view) || (vis.length === head.entries.length && vis[i] === '0')) return;
      for (const g of head.pieces[e.piece]?.groups || []) {
        const mat = this.matFor(i, g.mat, e.mats[g.mat], look);
        if (mat) for (const t of [mat.tex, mat.trans, mat.mask]) if (t !== undefined) needed.add(t);
      }
    });
    this.textures.setPlan(textures, needed, budget);
  }

  /** The bus's textures after a livery's swaps. */
  texturesOf(manifest, look) {
    const swaps = new Map((look?.textures || []).map(([i, t]) => [i, t]));
    return manifest.textures.map((t, i) => (swaps.has(i) ? { ...t, ...swaps.get(i), area: t.area, uv: t.uv, skin: t.skin, ctc: t.ctc } : t));
  }

  // ------------------------------------------------------------ the look: what shows, the draw lists
  setLook(look, budget) {
    const s = this.scene;
    if (!s) return;
    s.look = look;
    s.textures = this.texturesOf(s.manifest, look);
    this.buildLists();
    const plan = this.textures.setPlan(s.textures, s.needed, budget);
    // what is left under 300 MB for the LRU, at most 64 MB
    const fixed = this.gpuBytes(this.msaa?.w ?? 1920, this.msaa?.h ?? 1080);
    this.textures.setLruMax(Math.min(64 * 1024 * 1024, 300 * 1024 * 1024 - fixed.geometry - fixed.targets - plan.bytes - this.textures.looseTotal()));
    s.shadowDirty = true;
    s.contactDirty = true;
  }

  /** Inside or outside: other meshes show (the passenger's and the driver's seat show the
   * inside's). */
  setView(view) {
    if (this.view === view) return;
    this.view = view;
    if (this.scene) {
      this.buildLists();
      this.scene.shadowDirty = true;
    }
  }

  /** A slot's material as the shader wants it, or null when it shows nothing now. */
  matFor(entry, slot, m, look) {
    if (!m) return null;
    const key = slotKey(entry, slot);
    const item = look?.items?.[key] || 0;
    const stand = item >= 1 && m.items?.[item - 1] ? m.items[item - 1] : m;
    const scale = look?.alpha?.[key] ?? 1;
    if (stand.alpha >= 1 && scale <= 0) return null;
    const glass = stand.alpha === 2 && !!stand.env;
    // The reflection's strength saturated at 1 as a D3D factor; glass keeps at least
    // `glassMin`: tinted glass at 0.25-0.34 was a black slab, where a real pane at this angle
    // shows the sky (an idea of openOMSI's).
    const env = stand.env ? (glass ? Math.max(Math.min(stand.env.k, 1), LIGHT.glassMin) : Math.max(0, stand.env.k)) : 0;
    const mat = {
      key: '',
      mode: stand.alpha,
      tex: stand.tex,
      trans: stand.trans?.tex,
      transOwn: !stand.trans || stand.trans.tex === undefined,
      mask: stand.env?.mask,
      env,
      glass,
      diffuse: [stand.d[0], stand.d[1], stand.d[2], stand.d[3] ?? 1],
      spec: [stand.s[0], stand.s[1], stand.s[2], stand.p],
      emissive: [stand.e[0], stand.e[1], stand.e[2]],
      clamp: !!m.clamp,
      noZw: !!m.no_zw,
      noZc: !!m.no_zc,
      alphaScale: stand.alpha >= 1 ? scale : 1,
    };
    mat.key = JSON.stringify([mat.mode, mat.tex, mat.trans, mat.mask, mat.env, mat.diffuse, mat.spec, mat.emissive, mat.clamp, mat.noZw, mat.noZc, mat.alphaScale]);
    return mat;
  }

  /** The draw lists for what shows now, opaque indices joined per material. */
  buildLists() {
    const s = this.scene;
    if (!s) return;
    const gl = this.gl;
    const { head } = s;
    const vis = s.look?.visible || '';
    const groups = new Map();
    const seen = new Set();
    const blobs = [];
    const blended = s.manifest.parts.map(() => []);
    const needed = new Set();
    let shown = 0;
    head.entries.forEach((e, i) => {
      if (!inView(e, this.view)) return;
      if (vis.length === head.entries.length && vis[i] === '0') return;
      const piece = head.pieces[e.piece];
      if (!piece) return;
      const base = s.start[i];
      for (const g of piece.groups) {
        const mat = this.matFor(i, g.mat, e.mats[g.mat], s.look);
        if (!mat) continue;
        if (e.blob) mat.mode = 3;
        for (const t of [mat.tex, mat.trans, mat.mask]) if (t !== undefined) needed.add(t);
        const first = base + g.first;
        shown += g.count / 3;
        if (mat.mode === 3) blobs.push({ mat, first, count: g.count });
        else if (mat.mode === 2) (blended[piece.part] || blended[0]).push({ mat, first, count: g.count });
        else {
          // the same geometry with the same material once (doubled entries)
          const twice = `${first}|${mat.key}`;
          if (seen.has(twice)) continue;
          seen.add(twice);
          let group = groups.get(mat.key);
          if (!group) {
            group = { mat, runs: [] };
            groups.set(mat.key, group);
          }
          group.runs.push([first, g.count]);
        }
      }
    });
    let total = 0;
    for (const g of groups.values()) for (const [, n] of g.runs) total += n;
    const joined = new Uint32Array(total);
    const opaque = [];
    const tested = [];
    let at = 0;
    for (const g of groups.values()) {
      const first = at;
      for (const [b, n] of g.runs) {
        joined.set(s.allIndices.subarray(b, b + n), at);
        at += n;
      }
      (g.mat.mode === 1 ? tested : opaque).push({ mat: g.mat, first, count: at - first });
    }
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ibJoined);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, joined, gl.STATIC_DRAW);
    s.bytes.joined = joined.byteLength;
    Object.assign(s, { opaque, tested, blobs, blended, needed, shown });
  }

  // ------------------------------------------------------------ drawing
  fillBlock(c) {
    const b = this.block;
    b.set(c.viewProj, 0);
    b.set(this.shadowMat, 16);
    b.set(inverse(c.viewProj), 32);
    b.set([c.eye[0], c.eye[1], c.eye[2], LIGHT.exposure], 48);
    b.set([this.sunDir[0], this.sunDir[1], this.sunDir[2], this.sunAzimuth], 52);
    // the shadow's radius: about 6 cm in the world (a soft edge), as a part of the map
    b.set([LIGHT.sun[0], LIGHT.sun[1], LIGHT.sun[2], 0.06 / this.shadowReach.width], 56);
    b.set([LIGHT.sky[0], LIGHT.sky[1], LIGHT.sky[2], LIGHT.gloss], 60);
    b.set([LIGHT.ambient[0], LIGHT.ambient[1], LIGHT.ambient[2], this.env?.cloudSize ?? 2000], 64);
    b.set([LIGHT.floor[0], LIGHT.floor[1], LIGHT.floor[2], LIGHT.floorRadius], 68);
    const ca = this.contactArea;
    b.set([ca.minX, ca.minZ, 1 / ca.sizeX, 1 / ca.sizeZ], 72);
    const sky = this.textures.forSlot('sky');
    const clouds = this.textures.forSlot('clouds');
    b.set([sky ? 1 : 0, clouds ? 1 : 0, 0.02, 0.004 / this.shadowReach.depth], 76);
    this.seed = (this.seed + 0.61803) % 1;
    b.set([this.seed * 100, this.scene ? 1 : 0, 0.52, LIGHT.shadeSky], 80);
    const gl = this.gl;
    gl.bindBuffer(gl.UNIFORM_BUFFER, this.ubo);
    gl.bufferSubData(gl.UNIFORM_BUFFER, 0, b);
    gl.bindBufferBase(gl.UNIFORM_BUFFER, 0, this.ubo);
  }

  bindEnv() {
    const gl = this.gl;
    const sky = this.textures.forSlot('sky');
    const clouds = this.textures.forSlot('clouds');
    gl.activeTexture(gl.TEXTURE3);
    gl.bindTexture(gl.TEXTURE_2D, this.shadowMap.tex);
    gl.bindSampler(3, null);
    gl.activeTexture(gl.TEXTURE4);
    gl.bindTexture(gl.TEXTURE_2D, sky?.tex ?? this.blank);
    gl.bindSampler(4, this.textures.skySampler());
    gl.activeTexture(gl.TEXTURE5);
    gl.bindTexture(gl.TEXTURE_2D, clouds?.tex ?? this.blank);
    gl.bindSampler(5, this.textures.skySampler());
    gl.activeTexture(gl.TEXTURE6);
    gl.bindTexture(gl.TEXTURE_2D, this.contact.tex[0]);
    gl.bindSampler(6, null);
  }

  /** The shadow map again (only when something changed). */
  drawShadowMap() {
    const s = this.scene;
    if (!s) return;
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.shadowMap.fb);
    gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
    gl.disable(gl.BLEND);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.disable(gl.CULL_FACE);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.depthMask(true);
    gl.clearDepth(1);
    gl.clear(gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.POLYGON_OFFSET_FILL);
    gl.polygonOffset(1.5, 3);
    gl.useProgram(this.shadowProg.prog);
    gl.uniform1i(this.shadowProg.tex, 0);
    gl.bindVertexArray(s.vao);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ibJoined);
    gl.uniform1i(this.shadowProg.test, 0);
    for (const b of s.opaque) gl.drawElements(gl.TRIANGLES, b.count, gl.UNSIGNED_INT, b.first * 4);
    gl.uniform1i(this.shadowProg.test, 1);
    gl.activeTexture(gl.TEXTURE0);
    for (const b of s.tested) {
      const t = b.mat.tex !== undefined ? this.textures.forSlot(b.mat.tex) : undefined;
      gl.bindTexture(gl.TEXTURE_2D, t?.tex ?? this.blank);
      gl.bindSampler(0, this.textures.sampler(b.mat.clamp));
      gl.drawElements(gl.TRIANGLES, b.count, gl.UNSIGNED_INT, b.first * 4);
    }
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.bindVertexArray(null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    s.shadowDirty = false;
  }

  /** The contact shadow: from below, what lies up to 0.6 m over the floor; blurred twice. */
  drawContact() {
    const s = this.scene;
    if (!s) return;
    const gl = this.gl;
    const ca = this.contactArea;
    const m = new Float32Array(16);
    m[0] = 2 / ca.sizeX;
    m[12] = -1 - (2 * ca.minX) / ca.sizeX;
    m[9] = 2 / ca.sizeZ;
    m[13] = -1 - (2 * ca.minZ) / ca.sizeZ;
    m[6] = 2 / 0.6;
    m[14] = -1;
    m[15] = 1;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.contact.fb[0]);
    gl.viewport(0, 0, CONTACT_SIZE, CONTACT_SIZE);
    gl.disable(gl.BLEND);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LESS);
    gl.depthMask(true);
    gl.clearColor(0, 0, 0, 1);
    gl.clearDepth(1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.useProgram(this.contactProg.prog);
    gl.uniformMatrix4fv(this.contactProg.mat, false, m);
    gl.bindVertexArray(s.vao);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ibJoined);
    for (const b of [...s.opaque, ...s.tested]) gl.drawElements(gl.TRIANGLES, b.count, gl.UNSIGNED_INT, b.first * 4);
    gl.bindVertexArray(this.emptyVao);
    gl.disable(gl.DEPTH_TEST);
    gl.useProgram(this.blurProg.prog);
    gl.uniform1i(this.blurProg.src, 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindSampler(0, null);
    for (let round = 0; round < 2; round++) {
      for (const [from, to, step] of [
        [0, 1, [1 / CONTACT_SIZE, 0]],
        [1, 0, [0, 1 / CONTACT_SIZE]],
      ]) {
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.contact.fb[to]);
        gl.bindTexture(gl.TEXTURE_2D, this.contact.tex[from]);
        gl.uniform2f(this.blurProg.step, step[0] * 1.6, step[1] * 1.6);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      }
    }
    gl.bindVertexArray(null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    s.contactDirty = false;
  }

  /**
   * 4x MSAA in a framebuffer of our own (RGBA8 colour and 24-bit depth, four samples each),
   * resolved into the canvas at the end. The canvas itself is plain RGBA8 without depth: with
   * `antialias: true` Chromium put an MSAA colour, a depth and stencil and a copy beside it,
   * and an empty viewer cost 250 MB of video memory.
   */
  msaaFor(w, h) {
    const gl = this.gl;
    if (this.msaa && this.msaa.w === w && this.msaa.h === h) return this.msaa.fb;
    if (this.msaa) {
      gl.deleteFramebuffer(this.msaa.fb);
      gl.deleteRenderbuffer(this.msaa.colour);
      gl.deleteRenderbuffer(this.msaa.depth);
      gl.deleteFramebuffer(this.msaa.outFb);
      gl.deleteTexture(this.msaa.out);
    }
    const samples = Math.min(4, gl.getParameter(gl.MAX_SAMPLES));
    const colour = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, colour);
    gl.renderbufferStorageMultisample(gl.RENDERBUFFER, samples, gl.RGBA8, w, h);
    const depth = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, depth);
    gl.renderbufferStorageMultisample(gl.RENDERBUFFER, samples, gl.DEPTH_COMPONENT24, w, h);
    const fb = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, colour);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, depth);
    const out = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, out);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, w, h);
    const outFb = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, outFb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, out, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    this.msaa = { fb, colour, depth, outFb, out, w, h };
    return fb;
  }

  /** One picture into the canvas (the default framebuffer). */
  draw(c, w, h, opts = {}) {
    const gl = this.gl;
    const fb = this.msaaFor(w, h);
    this.drawScene(c, w, h, opts, fb);
    // resolved to RGBA8 first (the same format), then to the canvas (which may differ)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, fb);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, this.msaa.outFb);
    gl.blitFramebuffer(0, 0, w, h, 0, 0, w, h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, this.msaa.outFb);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, null);
    gl.blitFramebuffer(0, 0, w, h, 0, 0, w, h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  drawScene(c, w, h, opts, target) {
    const gl = this.gl;
    const s = this.scene;
    this.fillBlock(c);
    if (s?.shadowDirty) this.drawShadowMap();
    if (s?.contactDirty) this.drawContact();
    gl.bindFramebuffer(gl.FRAMEBUFFER, target);
    gl.viewport(0, 0, w, h);
    gl.colorMask(true, true, true, true);
    gl.depthMask(true);
    // the photo is transparent: no sky, and of the floor only the contact shadow
    gl.clearColor(0, 0, 0, opts.photo ? 0 : 1);
    gl.clearDepth(1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.disable(gl.BLEND);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    this.bindEnv();
    const inside = this.view === 'inside';
    if (!opts.flat && !opts.id) {
      gl.disable(gl.CULL_FACE);
      if (!opts.photo) {
        gl.disable(gl.DEPTH_TEST);
        gl.useProgram(this.skyProg.prog);
        gl.bindVertexArray(this.emptyVao);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      }
      if (!inside || opts.photo) {
        gl.enable(gl.DEPTH_TEST);
        gl.depthFunc(gl.LEQUAL);
        gl.useProgram(this.floorProg.prog);
        gl.uniform1i(this.floorProg.photo, opts.photo ? 1 : 0);
        gl.bindVertexArray(this.floorVao);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      }
    }
    if (!s || opts.noBus) {
      gl.bindVertexArray(null);
      return;
    }
    const u = this.bus.u;
    gl.useProgram(this.bus.prog);
    gl.uniform1i(u.uFlat, opts.id ? 2 : opts.flat ? 1 : 0);
    this.idTable = [];
    const setId = (list, d) => {
      if (!opts.id) return;
      const i = this.idTable.length + 1;
      this.idTable.push({ list, tex: s.textures[d.mat.tex]?.name, mode: d.mat.mode, count: d.count });
      gl.uniform3f(u.uId, (i & 255) / 255, ((i >> 8) & 255) / 255, 0.5);
    };
    gl.bindVertexArray(s.vao);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    // (inside the bus the faces are seen from both sides: a seat's back has no back faces)
    if (inside) gl.disable(gl.CULL_FACE);
    else {
      gl.enable(gl.CULL_FACE);
      gl.cullFace(gl.BACK);
      gl.frontFace(gl.CW);
    }
    // 1. opaque, 2. alpha test: the joined pieces
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ibJoined);
    let last;
    for (const d of s.opaque) {
      if (!this.setMat(d.mat, last)) continue;
      last = d.mat;
      setId('opaque', d);
      gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, d.first * 4);
    }
    if (!opts.flat && !opts.id) gl.enable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    last = undefined;
    for (const d of s.tested) {
      if (!this.setMat(d.mat, last)) continue;
      last = d.mat;
      setId('tested', d);
      gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, d.first * 4);
    }
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    // 3. blended: [isshadow] first, then each part from the back to the front, in cfg order
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ibAll);
    if (!opts.id) gl.enable(gl.BLEND);
    gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    const order = s.blended.map((list, i) => ({ list, d: dist2(s.partMid[i] || [0, 0, 0], c.eye) })).sort((a, b) => b.d - a.d);
    const lists = opts.flat || inside ? order.map((o) => o.list) : [s.blobs, ...order.map((o) => o.list)];
    last = undefined;
    for (const list of lists) {
      for (const d of list) {
        if (!this.setMat(d.mat, last)) continue;
        last = d.mat;
        setId(d.mat.mode === 3 ? 'blob' : 'blended', d);
        if (opts.id) gl.depthMask(true);
        gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, d.first * 4);
      }
    }
    gl.disable(gl.BLEND);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.depthMask(true);
    gl.bindVertexArray(null);
  }

  /** The material into the shader; false: not drawn now (a transparency map still loading). */
  setMat(m, last) {
    const gl = this.gl;
    const u = this.bus.u;
    const tex = m.tex !== undefined ? this.textures.forSlot(m.tex) : undefined;
    let transMode = 0;
    let trans;
    if (m.mode >= 1 && m.mode <= 3 && !m.transOwn && m.trans !== undefined) {
      trans = this.textures.forSlot(m.trans);
      if (!trans) return false;
      transMode = trans.hasAlpha ? 1 : 2;
    }
    // while the texture of glass or a grille loads: rather nothing than a closed face
    if (m.mode >= 1 && m.tex !== undefined && !tex && transMode === 0) return false;
    let maskMode = 0;
    let mask;
    let env = m.env;
    if (env > 0 && m.mask !== undefined) {
      mask = this.textures.forSlot(m.mask);
      if (mask) maskMode = mask.hasAlpha ? 1 : 2;
      else env = 0;
    }
    if (env > 0 && maskMode === 0 && !tex) env = 0;
    gl.uniform4fv(u.uDiffuse, m.diffuse);
    gl.uniform4fv(u.uSpec, m.spec);
    gl.uniform3fv(u.uEmissive, m.emissive);
    gl.uniform1i(u.uMode, m.mode);
    gl.uniform1i(u.uTexMode, tex ? (tex.linearise ? 2 : 1) : 0);
    gl.uniform1i(u.uTransMode, transMode);
    gl.uniform1i(u.uMaskMode, maskMode);
    gl.uniform1i(u.uGlass, m.glass ? 1 : 0);
    gl.uniform1f(u.uEnv, env);
    gl.uniform1f(u.uAlphaScale, m.alphaScale);
    const sampler = this.textures.sampler(m.clamp);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tex?.tex ?? this.blank);
    gl.bindSampler(0, sampler);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, trans?.tex ?? this.blank);
    gl.bindSampler(1, sampler);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, mask?.tex ?? this.blank);
    gl.bindSampler(2, sampler);
    if (!last || last.noZw !== m.noZw || last.mode !== m.mode) gl.depthMask(!(m.mode >= 2 && m.noZw) && m.mode !== 3);
    if (m.noZc || m.mode === 3) {
      // noZcheck: no depth test off but an offset (as openOMSI draws it)
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(-2, -8);
    } else gl.disable(gl.POLYGON_OFFSET_FILL);
    return true;
  }

  /** The photo: one picture with `photo` in our MSAA framebuffer, resolved and read back.
   * The pixels are premultiplied and run from the bottom up; the engine turns them. */
  readPhoto(c, w, h) {
    const gl = this.gl;
    const fb = this.msaaFor(w, h);
    this.drawScene(c, w, h, { photo: true }, fb);
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, fb);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, this.msaa.outFb);
    gl.blitFramebuffer(0, 0, w, h, 0, 0, w, h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.msaa.outFb);
    const px = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return px;
  }

  /** What is on the GPU now (our own count), in bytes. */
  gpuBytes(w, h) {
    const s = this.scene;
    const geometry = s ? s.bytes.verts + s.bytes.indices + s.bytes.joined : 0;
    const textures = this.textures.gpuBytes();
    const targets = SHADOW_SIZE * SHADOW_SIZE * 4 + CONTACT_SIZE * CONTACT_SIZE * 4 + w * h * (16 + 16 + 4 + 4);
    return { total: geometry + textures + targets, textures, geometry, targets };
  }

  count() {
    const s = this.scene;
    if (!s) return { triangles: 0, shown: 0, draws: 0 };
    return { triangles: s.triangles, shown: s.shown, draws: s.opaque.length + s.tested.length + s.blobs.length + s.blended.reduce((n, l) => n + l.length, 0) };
  }
}

function dist2(a, b) {
  return (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;
}

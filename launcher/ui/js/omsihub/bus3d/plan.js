// The texture plan of a bus (the old app's `textuurPlan`, `shared/bus3d.ts`): how large each
// texture goes onto the GPU, within a budget.
//
// * The size comes from the texel density: d0 = √(U·w·h / A) texels a metre, A the outside
//   area (m²), U the UV area. Levels skipped: k = max(0, ⌊log2(d0 / 400)⌋), then down to the
//   cap (4096 for DXT, 2048 for RGBA).
// * While the total is over the budget, the texture with the most bytes per m² of outside
//   area goes down a level; the body (the [CTCTexture] slot with the largest outside area)
//   goes last.
// * Not loaded: what the outside view does not show (A = 0).
// * Without S3TC the launcher decodes DXT to RGBA: then it counts as RGBA, capped at 2048.

export const DENSITY = 400;
export const CAP_DXT = 4096;
export const CAP_RGBA = 2048;
/** Below this side the budget does not push; a bus that still does not fit is too heavy. */
export const FLOOR = 256;

/** How many levels a DXT texture may skip at most: WebGL takes a compressed level 0 only
 * when both its sides divide by four, and never past the file's last level. */
export function dxtMaxSkip(t) {
  let k = 0;
  while (k + 1 < t.mips && ((t.w >> (k + 1)) & 3) === 0 && ((t.h >> (k + 1)) & 3) === 0 && t.w >> (k + 1) > 0 && t.h >> (k + 1) > 0) k++;
  return k;
}

/** Bytes of a texture from level `k`, with the mip chain under it. */
function bytesFrom(t, k, s3tc) {
  const dxt = t.kind === 'dxt' && s3tc;
  const w0 = Math.max(1, t.w >> k);
  const h0 = Math.max(1, t.h >> k);
  let bytes = 0;
  let w = w0;
  let h = h0;
  const levels = dxt ? Math.max(1, t.mips - k) : Math.floor(Math.log2(Math.max(w0, h0))) + 1;
  const block = t.fmt === 'bc1' ? 8 : 16;
  for (let i = 0; i < levels; i++) {
    bytes += dxt ? Math.max(1, Math.ceil(w / 4)) * Math.max(1, Math.ceil(h / 4)) * block : w * h * 4;
    w = Math.max(1, w >> 1);
    h = Math.max(1, h >> 1);
  }
  return { bytes, w: w0, h: h0 };
}

/** The plan: per texture whether it loads, how many levels it skips and what it costs. */
export function texturePlan(textures, budget, { s3tc = true } = {}) {
  let body;
  for (const t of textures) if (t.ctc && t.area > (body?.area ?? 0)) body = t;
  const rows = [];
  const skips = [];
  for (const t of textures) {
    const load = t.area > 0 && t.w > 0 && t.h > 0;
    if (!load) {
      rows.push({ id: t.id, skip: 0, bytes: 0, w: 0, h: 0, load: false });
      skips.push(0);
      continue;
    }
    const d0 = Math.sqrt((Math.max(t.uv, 1e-9) * t.w * t.h) / t.area);
    let k = Math.max(0, Math.floor(Math.log2(d0 / DENSITY)));
    const dxt = t.kind === 'dxt' && s3tc;
    const cap = dxt ? CAP_DXT : CAP_RGBA;
    while (Math.max(t.w >> k, t.h >> k) > cap) k++;
    k = Math.min(k, dxt ? dxtMaxSkip(t) : Math.floor(Math.log2(Math.max(t.w, t.h))));
    const r = bytesFrom(t, k, s3tc);
    rows.push({ id: t.id, skip: k, bytes: r.bytes, w: r.w, h: r.h, load: true });
    skips.push(k);
  }
  let total = rows.reduce((s, r) => s + r.bytes, 0);
  let tooHeavy = false;
  while (total > budget) {
    let best = -1;
    let bestValue = -1;
    for (const forBody of [false, true]) {
      for (let i = 0; i < textures.length; i++) {
        const t = textures[i];
        const r = rows[i];
        if (!r.load || (t === body) !== forBody || Math.max(r.w, r.h) <= FLOOR) continue;
        if (t.kind === 'dxt' && s3tc && skips[i] + 1 > dxtMaxSkip(t)) continue;
        const value = r.bytes / Math.max(t.area, 1e-6);
        if (value > bestValue) {
          bestValue = value;
          best = i;
        }
      }
      if (best >= 0) break;
    }
    if (best < 0) {
      tooHeavy = true;
      break;
    }
    skips[best]++;
    const r = bytesFrom(textures[best], skips[best], s3tc);
    total += r.bytes - rows[best].bytes;
    rows[best] = { ...rows[best], skip: skips[best], bytes: r.bytes, w: r.w, h: r.h };
  }
  return { rows, bytes: total, tooHeavy, body: body?.ctc };
}

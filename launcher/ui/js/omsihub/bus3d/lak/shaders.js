// The livery studio's shaders (Omsi-Hub's Lakstudio, the old app's `lak/lakshaders.ts`, with the
// paint rules of the game program's studio, `omsihub-livery`'s `paint.rs`).
//
// Everything is drawn in texture space: the LAKNET - the triangles of a painted texture, their
// UVs moved back into [0,1] - is drawn with `gl_Position = uv * 2 - 1`, and the fragment shader
// gets each texel's place on the bus and its normal. Seams, mirrored UV islands and UVs beyond
// [0,1] come out right by themselves: every layer is a function of the place on the bus.
//
// AXES: the laknet and the layers' places are in the o3d frame of the 3D view (x right, y up,
// z forward); the game program's rules are written in the bus's frame (x right, y forward,
// z up) - here `bus = (o.x, o.z, o.y)`. The 3D view's world is the o3d frame with x mirrored.

/** The laknet: place, normal, UV (moved into the texture), and its class (1 glass, 2 kept,
 * 4 the window's see-through map). */
export const NET_VS = `#version 300 es
precision highp float;
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNorm;
layout(location = 2) in vec2 aUv;
layout(location = 3) in float aClass;
out vec3 vPos;
out vec3 vNorm;
out vec2 vUv;
flat out int vClass;
void main() {
  gl_Position = vec4(aUv * 2.0 - 1.0, 0.0, 1.0);
  vPos = aPos;
  vNorm = aNorm;
  vUv = aUv;
  vClass = int(aClass + 0.5);
}
`;

const NET_IN = `
in vec3 vPos;
in vec3 vNorm;
in vec2 vUv;
flat in int vClass;
uniform vec3 uMid;     // the bus's box: its middle and half its size (a pane's outward side)
uniform vec3 uHalf;
/** The texel's normal; a window's the side away from the bus's middle (a pane is seen from both
 * sides, and a model may turn its normals into the bus: Outside::out_of). */
vec3 normalHere() {
  float l = length(vNorm);
  vec3 n = l > 1e-6 ? vNorm / l : vec3(0.0, 1.0, 0.0);
  if ((vClass & 1) != 0 && dot(n, (vPos - uMid) / uHalf) < 0.0) n = -n;
  return n;
}
`;

/** A full-screen triangle (the passes over a whole texture). */
export const SCREEN_VS = `#version 300 es
precision highp float;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
`;

// ------------------------------------------------------------ the static masks

/** The classes of the texels (MAX over every triangle on them): R covered, G glass, B kept,
 * A a window's see-through map. */
export const CLASS_FS = `#version 300 es
precision highp float;
${NET_IN}
out vec4 o;
void main() {
  o = vec4(1.0, (vClass & 1) != 0 ? 1.0 : 0.0, (vClass & 2) != 0 ? 1.0 : 0.0, (vClass & 4) != 0 ? 1.0 : 0.0);
}
`;

/** Where each texel lies, as a part of the box (MIN, then MAX: two places far apart on one
 * texel make it a shared one, both sides of the bus taking it). */
export const PLACE_FS = `#version 300 es
precision highp float;
${NET_IN}
uniform vec3 uLo;
uniform vec3 uSize;
out vec4 o;
void main() {
  o = vec4(clamp((vPos - uLo) / uSize, 0.0, 1.0), 1.0);
}
`;

/** The depth maps of the outside test: the bus seen along one of 26 directions. */
export const DEPTH_VS = `#version 300 es
precision highp float;
layout(location = 0) in vec3 aPos;
uniform mat4 uMat;
void main() {
  gl_Position = uMat * vec4(aPos, 1.0);
}
`;
export const DEPTH_FS = `#version 300 es
precision highp float;
void main() {}
`;

/** One direction of the outside test (Outside::sees): a texel facing it whose place is no
 * farther than 4 cm behind the nearest surface there is seen from outside. MAX over the 26. */
export const OUTSIDE_FS = `#version 300 es
precision highp float;
${NET_IN}
uniform highp sampler2D uDepth;
uniform mat4 uMat;
uniform vec3 uDir;      // towards the viewer of this direction
uniform float uRange;   // metres of the depth map from near to far
uniform float uCell;    // metres a texel of the depth map
out vec4 o;
void main() {
  vec3 n = normalHere();
  float c = dot(n, uDir);
  if (c < 0.1) discard;
  vec4 q = uMat * vec4(vPos, 1.0);
  vec3 s = q.xyz * 0.5 + 0.5;
  bool seen = true;
  if (s.x > 0.0 && s.x < 1.0 && s.y > 0.0 && s.y < 1.0) {
    float front = texture(uDepth, s.xy).r;
    float tanT = sqrt(max(1.0 - c * c, 0.0)) / max(c, 0.2);
    seen = (s.z - front) * uRange <= 0.04 + uCell * 1.5 * tanT;
  }
  if (!seen) discard;
  o = vec4(1.0);
}
`;

/** The bleed (bake::BLEED, four rounds): an uncovered texel takes the first neighbour that has
 * a source; RG the offset to its source + 4 (B 1: it has one). */
export const BLEED_SEED_FS = `#version 300 es
precision highp float;
uniform sampler2D uClass;
out vec4 o;
void main() {
  float c = texelFetch(uClass, ivec2(gl_FragCoord.xy), 0).r;
  o = c > 0.5 ? vec4(4.0 / 255.0, 4.0 / 255.0, 1.0, 1.0) : vec4(0.0);
}
`;
export const BLEED_STEP_FS = `#version 300 es
precision highp float;
uniform sampler2D uPrev;
out vec4 o;
const ivec2 WAYS[8] = ivec2[8](ivec2(-1, 0), ivec2(1, 0), ivec2(0, -1), ivec2(0, 1), ivec2(-1, -1), ivec2(1, -1), ivec2(-1, 1), ivec2(1, 1));
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  ivec2 size = textureSize(uPrev, 0);
  vec4 me = texelFetch(uPrev, p, 0);
  if (me.b > 0.5) {
    o = me;
    return;
  }
  for (int k = 0; k < 8; k++) {
    ivec2 q = p + WAYS[k];
    if (q.x < 0 || q.y < 0 || q.x >= size.x || q.y >= size.y) continue;
    vec4 s = texelFetch(uPrev, q, 0);
    if (s.b < 0.5) continue;
    ivec2 off = ivec2(round(s.rg * 255.0)) - 4 + WAYS[k];
    o = vec4(vec2(off + 4) / 255.0, 1.0, 1.0);
    return;
  }
  o = vec4(0.0);
}
`;

/** The texels' mask, put together: R outside, G the classes (1 glass, 2 kept, 4 see-through map,
 * 8 covered, 16 shared), B the bleed's source (dx·9 + dy, 255: none), A covered. */
export const MASK_FS = `#version 300 es
precision highp float;
uniform sampler2D uClass;
uniform sampler2D uOut;
uniform sampler2D uLoTex;
uniform sampler2D uHiTex;
uniform sampler2D uBleed;
uniform vec3 uSize;
out vec4 o;
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec4 c = texelFetch(uClass, p, 0);
  float outside = texelFetch(uOut, p, 0).r;
  vec3 lo = texelFetch(uLoTex, p, 0).rgb;
  vec3 hi = texelFetch(uHiTex, p, 0).rgb;
  bool covered = c.r > 0.5;
  bool shared = covered && length((hi - lo) * uSize) > 0.25;
  int bits = (c.g > 0.5 ? 1 : 0) | (c.b > 0.5 ? 2 : 0) | (c.a > 0.5 ? 4 : 0) | (covered ? 8 : 0) | (shared ? 16 : 0);
  vec4 b = texelFetch(uBleed, p, 0);
  float code = 255.0;
  if (!covered && b.b > 0.5) {
    ivec2 off = ivec2(round(b.rg * 255.0)) - 4;
    code = float((off.x + 4) * 9 + (off.y + 4));
  }
  o = vec4(covered ? outside : 0.0, float(bits) / 255.0, code / 255.0, covered ? 1.0 : 0.0);
}
`;

// ------------------------------------------------------------ colours and zones

const COLOUR = `
float toLin(float s) { return s <= 0.04045 ? s / 12.92 : pow((s + 0.055) / 1.055, 2.4); }
float toSrgb(float c) { return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055; }
vec3 lin3(vec3 s) { return vec3(toLin(s.r), toLin(s.g), toLin(s.b)); }
float lum(vec3 c) { return 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b; }
float labF(float t) { return t > 0.008856 ? pow(t, 1.0 / 3.0) : 7.787 * t + 16.0 / 116.0; }
/** Linear light -> CIELAB (D65), as colour::lab_of_linear. */
vec3 labOf(vec3 l) {
  float x = (0.4124 * l.r + 0.3576 * l.g + 0.1805 * l.b) / 0.95047;
  float y = 0.2126 * l.r + 0.7152 * l.g + 0.0722 * l.b;
  float z = (0.0193 * l.r + 0.1192 * l.g + 0.9505 * l.b) / 1.08883;
  float fx = labF(x);
  float fy = labF(y);
  float fz = labF(z);
  return vec3(116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
}
/** The relative luminance of a CIELAB colour (bake::luminance). */
float labLum(vec3 lab) {
  float f = (lab.x + 16.0) / 116.0;
  return f * f * f > 0.008856 ? f * f * f : (f - 16.0 / 116.0) / 7.787;
}
`;

const ZONES = `
uniform int uZones;
uniform vec3 uZoneLab[8];
uniform vec3 uZoneLin[8];
uniform vec4 uZoneInfo[8];   // painted (0/1), group (255: none), rel, share
const int BLACK = 255;
bool paintZ(int z) { return z != BLACK && z < uZones && uZoneInfo[z].x > 0.5; }
vec3 linZ(int z) { return z == BLACK ? vec3(0.0) : uZoneLin[z]; }
float relZ(int z) { return z == BLACK ? 1.0 : uZoneInfo[z].z; }
`;

/** Each texel of the base as the zones make it (Zones::unmix and Zones::detail): RGBA8 with the
 * zones A and B (255: black), A's part t (0..1.3, over 1.3) and the detail ratio (0.55..1.15). */
export const MIX_FS = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uBase;
uniform int uLinearise;
${COLOUR}
${ZONES}
out vec4 o;
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec2 st = (vec2(p) + 0.5) / vec2(textureSize(uBase, 0));
  vec3 c = textureLod(uBase, st, 0.0).rgb;
  if (uLinearise == 1) c = lin3(c);
  vec3 lab = labOf(c);
  if (uZones == 0) {
    o = vec4(1.0, 1.0, 0.0, (1.0 - 0.55) / 0.6);
    return;
  }
  // the three zones nearest in CIELAB
  int n0 = -1, n1 = -1, n2 = -1;
  float d0 = 1e30, d1 = 1e30, d2 = 1e30;
  int nearest = 0;
  float best = 1e30;
  for (int i = 0; i < 8; i++) {
    if (i >= uZones) break;
    float d = distance(uZoneLab[i], lab);
    if (d < best) { best = d; nearest = i; }
    if (d < d2) {
      n2 = i; d2 = d;
      if (d2 < d1) { int ti = n1; float td = d1; n1 = n2; d1 = d2; n2 = ti; d2 = td; }
      if (d1 < d0) { int ti = n0; float td = d0; n0 = n1; d0 = d1; n1 = ti; d1 = td; }
    }
  }
  int cand[4] = int[4](n0, n1, n2, -1);
  int ba = n0;
  int bb = BLACK;
  float bt = 1.0;
  float br = 1e30;
  for (int a = 0; a < 3; a++) {
    int ia = cand[a];
    if (ia < 0) continue;
    for (int b = 0; b < 4; b++) {
      int ib = b == 3 ? -1 : cand[b];
      if (b != 3 && (ib < 0 || ib <= ia)) continue;
      vec3 pa = uZoneLin[ia];
      vec3 pb = ib < 0 ? vec3(0.0) : uZoneLin[ib];
      vec3 d = pa - pb;
      float dd = dot(d, d);
      if (dd < 1e-8) continue;
      float hi = ib < 0 ? 1.3 : 1.0;
      float t = clamp(dot(c - pb, d) / dd, 0.0, hi);
      vec3 e = c - (t * pa + (1.0 - t) * pb);
      float r = dot(e, e);
      float s = c.r + c.g + c.b + 0.05;
      float tm = 1.0 - min(t, 1.0);
      r = r / (s * s) + (ib < 0 ? 0.006 * tm * tm : 0.002);
      if (r < br - 1e-7) { ba = ia; bb = ib < 0 ? BLACK : ib; bt = t; br = r; }
    }
  }
  float rr = labLum(lab) / max(labLum(uZoneLab[nearest]), 1e-4);
  if (rr < 1.0) rr = 1.0 - (1.0 - rr) * 0.75;
  float detail = clamp(rr, 0.55, 1.15);
  o = vec4(float(ba) / 255.0, float(bb) / 255.0, bt / 1.3, (detail - 0.55) / 0.6);
}
`;

/** Specks and streaks of a stray colour in the body (Canvas::with): a texel of a small zone that
 * is no paint and not black, with paint round most of it, takes the paint round it. */
export const SPECKS_FS = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uMix;
uniform sampler2D uBase;
uniform int uLinearise;
${COLOUR}
${ZONES}
out vec4 o;
int za(vec4 m) { return int(round(m.r * 255.0)); }
int zb(vec4 m) { return int(round(m.g * 255.0)); }
int dominant(vec4 m) { return m.b * 1.3 >= 0.5 ? za(m) : zb(m); }
bool stray(int z) { return z != BLACK && z < uZones && uZoneInfo[z].x < 0.5 && uZoneInfo[z].w < 0.03 && uZoneLab[z].x >= 25.0; }
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec4 m = texelFetch(uMix, p, 0);
  o = m;
  int a = za(m);
  int b = zb(m);
  if (!(stray(dominant(m)) || (stray(a) && (paintZ(b) || b == BLACK)) || (stray(b) && paintZ(a)))) return;
  ivec2 size = textureSize(uMix, 0);
  int votes[8] = int[8](0, 0, 0, 0, 0, 0, 0, 0);
  int paint = 0;
  int all = 0;
  for (int dy = -5; dy <= 5; dy++) {
    for (int dx = -5; dx <= 5; dx++) {
      ivec2 q = p + ivec2(dx, dy);
      if (q.x < 0 || q.y < 0 || q.x >= size.x || q.y >= size.y) continue;
      int d = dominant(texelFetch(uMix, q, 0));
      all++;
      if (paintZ(d)) {
        paint++;
        votes[d]++;
      }
    }
  }
  if (paint * 10 < all * 7) return;
  int z = 0;
  for (int k = 1; k < 8; k++) if (votes[k] > votes[z]) z = k;
  vec3 c = textureLod(uBase, (vec2(p) + 0.5) / vec2(size), 0.0).rgb;
  if (uLinearise == 1) c = lin3(c);
  float t = clamp(lum(c) / max(lum(uZoneLin[z]), 1e-3), 0.3, 1.3);
  o = vec4(float(z) / 255.0, 1.0, t / 1.3, m.a);
}
`;

// ------------------------------------------------------------ the paint

/** The number of texels of the operations' table per operation. */
export const OP_TEXELS = 10;

/**
 * The layers painted into a texture (paint::composite), per texel of the laknet, in linear light
 * over the base. Per operation `OP_TEXELS` texels of `uOps` (RGBA32F):
 *  0: kind (1 fill, 2 stripe, 3 mask, 4 picture, 5 brush), cover, detail, flags (1 over the
 *     trims, 2 over the windows, 4 a copy on the other side)
 *  1: colour (linear), gradient (0 none, 1 linear, 2 radial)    2: second colour, its angle
 *  3: fill: CIELAB, radius | stripe: h1, h2, tan(angle), wave | decal: middle, width
 *  4: fill: group (-1: none) | stripe: sides | decal: u, height
 *  5: decal: v, flip | brush: its layer in uBrush
 *  6: decal: n, sin   7: decal: lo, cos   8: decal: hi   9: decal: the atlas's rectangle
 * The fragment writes as its depth how much of the decals lies on it: on a texel both sides of
 * the bus share, the side a decal lies on wins (paint::composite takes the larger of both).
 */
export const PAINT_FS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2DArray;
${NET_IN}
${COLOUR}
${ZONES}
uniform sampler2D uBase;
uniform sampler2D uMask;
uniform sampler2D uMix;
uniform sampler2D uTpl;
uniform highp sampler2D uOps;
uniform sampler2D uAtlas;
uniform sampler2DArray uBrush;
uniform int uOpCount;
uniform int uTemplate;
uniform vec3 uMuRef;
uniform vec3 uMin;
uniform vec3 uMax;
uniform float uAA;
uniform int uAlpha;
uniform int uLinearise;
uniform int uBefore;
out vec4 o;

vec4 op(int j, int k) { return texelFetch(uOps, ivec2(k, j), 0); }
float ramp(float x, float a, float b) { return clamp((x - a) / (b - a), 0.0, 1.0); }

/** A layer's colour at a point of its box (0..1 each way, y up): plain or its gradient. */
vec3 colourAt(int j, vec2 at) {
  vec4 a = op(j, 1);
  if (a.w < 0.5) return a.rgb;
  vec4 b = op(j, 2);
  vec2 d = at - 0.5;
  float t;
  if (a.w < 1.5) {
    float s = sin(b.w);
    float c = cos(b.w);
    float reach = 0.5 * max(abs(c) + abs(s), 1e-3);
    t = clamp(0.5 + (d.x * c + d.y * s) / reach * 0.5, 0.0, 1.0);
  } else t = clamp(length(d) * 2.0, 0.0, 1.0);
  return mix(a.rgb, b.rgb, t);
}

/** How much operation j recolours zone z (paint.rs takes). */
float takes(int j, int kind, int z, vec3 lab0) {
  if (kind == 1) {
    vec4 f = op(j, 3);
    float group = op(j, 4).x;
    if (group >= -0.5) return (z != BLACK && z < uZones && abs(uZoneInfo[z].y - group) < 0.5) ? 1.0 : 0.0;
    if (f.w >= 999.0) return paintZ(z) ? 1.0 : 0.0;
    vec3 centre = z == BLACK ? vec3(0.0) : uZoneLab[z];
    return ramp(f.w + 2.0 - distance(centre, f.xyz), 0.0, 4.0);
  }
  return paintZ(z) ? 1.0 : 0.0;
}

/** A decal at the texel: where it lies in its box, how much it faces it (Decal::at). */
bool decalAt(int j, vec3 p, vec3 n, out vec2 q, out float facing) {
  vec4 c = op(j, 3);
  vec4 u = op(j, 4);
  vec4 v = op(j, 5);
  vec4 dn = op(j, 6);
  vec4 lo = op(j, 7);
  vec4 hi = op(j, 8);
  if (any(lessThan(p, lo.xyz)) || any(greaterThan(p, hi.xyz))) return false;
  facing = ramp(dot(n, dn.xyz), 0.17, 0.5);
  if (facing <= 0.0) return false;
  vec3 d = p - c.xyz;
  float du = dot(d, u.xyz);
  float dv = dot(d, v.xyz);
  float a = du * lo.w + dv * dn.w;
  float b = -du * dn.w + dv * lo.w;
  if (v.w > 0.5) a = -a;
  q = vec2(a / c.w + 0.5, 0.5 - b / max(u.w, 1e-5));
  return q.x >= 0.0 && q.x <= 1.0 && q.y >= 0.0 && q.y <= 1.0;
}

vec4 atlas(int j, vec2 q) {
  vec4 r = op(j, 9);
  return textureLod(uAtlas, mix(r.xy, r.zw, q), 0.0);
}

void main() {
  ivec2 px = ivec2(gl_FragCoord.xy);
  vec4 m = texelFetch(uMask, px, 0);
  int bits = int(round(m.g * 255.0));
  vec2 st = (vec2(px) + 0.5) / vec2(textureSize(uMask, 0));
  vec4 base = textureLod(uBase, st, 0.0);
  if (uLinearise == 1) base.rgb = lin3(base.rgb);
  // kept (a wheel, a lamp, a rubber): the base, and so the texel stays as the copy of it has it
  if ((bits & 2) != 0 || uBefore == 1) discard;
  vec3 p = vPos;
  vec3 n = normalHere();
  vec4 mx = texelFetch(uMix, px, 0);
  int za = int(round(mx.r * 255.0));
  int zb = int(round(mx.g * 255.0));
  float t = mx.b * 1.3;
  float ratio = mx.a * 0.6 + 0.55;
  vec3 c0 = base.rgb;
  vec3 pa0 = linZ(za);
  vec3 pb0 = linZ(zb);
  vec3 pa = pa0;
  vec3 pb = pb0;
  float det = 1.0;
  float glassCover = 0.0;
  float ra = relZ(za);
  float rb = relZ(zb);
  bool seeThrough = (bits & 4) != 0;
  bool glass = seeThrough || ((bits & 1) != 0 && !(uAlpha == 1 && base.a >= 240.0 / 255.0));
  bool shared = (bits & 16) != 0;
  float outside = m.r;
  vec4 tpl = uTemplate == 1 ? texelFetch(uTpl, px, 0) : vec4(0.0);
  float ma = tpl.a;
  float t1 = min(t, 1.0);
  vec3 lab0 = vec3(0.0);
  float decals = 0.0;
  // the bus frame: x right, y forward, z up
  float len = max(uMax.z - uMin.z, 0.5);
  float height = max(uMax.y - uMin.y, 0.5);
  for (int j = 0; j < 256; j++) {
    if (j >= uOpCount) break;
    vec4 h = op(j, 0);
    int kind = int(h.x + 0.5);
    int flags = int(h.w + 0.5);
    if ((glass && (flags & 2) == 0) || ((flags & 4) != 0 && shared)) continue;
    float a = 0.0;
    vec3 col = vec3(0.0);
    if (kind == 1) {
      a = 1.0;
      col = colourAt(j, vec2((p.z - uMin.z) / len, (p.y - uMin.y) / height));
    } else if (kind == 2) {
      vec4 s = op(j, 3);
      int sides = int(op(j, 4).x + 0.5);
      float side = 1.0;
      if (sides == 1) side = ramp(abs(n.x), 0.45, 0.6);
      else if (sides == 2) side = ramp(n.z, 0.45, 0.6);
      else if (sides == 3) side = ramp(-n.z, 0.45, 0.6);
      if (side > 0.0) {
        float z = p.y - uMin.y;
        float mid = (uMin.z + uMax.z) * 0.5;
        float rise = s.z * (p.z - mid) + s.w * sin(6.2831853 * (p.z - uMin.z) / 4.0);
        float e1 = s.x + rise;
        float e2 = s.y + rise;
        a = ramp(z, e1 - uAA, e1 + uAA) * (1.0 - ramp(z, e2 - uAA, e2 + uAA)) * side;
        if (a > 0.0) col = colourAt(j, vec2((p.z - uMin.z) / len, clamp((z - e1) / max(e2 - e1, 1e-3), 0.0, 1.0)));
      }
    } else if (kind == 3 || kind == 4) {
      vec2 q;
      float facing;
      if (decalAt(j, p, n, q, facing)) {
        vec4 s = atlas(j, q);
        a = s.a * facing;
        col = kind == 3 ? (a > 0.0 ? colourAt(j, vec2(q.x, 1.0 - q.y)) : vec3(0.0)) : s.rgb;
      }
    } else if (kind == 5) {
      a = texelFetch(uBrush, ivec3(px, int(op(j, 5).w + 0.5)), 0).r;
      col = op(j, 1).rgb;
    }
    a *= outside;
    if (kind >= 3) decals += a;
    a *= h.y;
    if (a <= 0.0) continue;
    float detail = h.z;
    if (glass) {
      pa += (col - pa) * a;
      pb += (col - pb) * a;
      det *= 1.0 - a;
      glassCover += (1.0 - glassCover) * a;
    } else if ((flags & 1) != 0 || kind >= 3) {
      float dd = 1.0 + (ratio - 1.0) * detail;
      vec3 cc = min(col * dd, vec3(1.0));
      pa += (cc - pa) * a;
      pb += (cc - pb) * a;
      det *= 1.0 - a;
    } else {
      float ta = takes(j, kind, za, lab0);
      float tb = takes(j, kind, zb, lab0);
      float aa = uTemplate == 1 ? a * (1.0 - ma) : a;
      float wa = ta * aa;
      float wb = tb * aa;
      if (wa > 0.0 || wb > 0.0) {
        float ra2 = 1.0 + (ra - 1.0) * detail;
        float rb2 = 1.0 + (rb - 1.0) * detail;
        pa += (col * ra2 - pa) * wa;
        pb += (col * rb2 - pb) * wb;
        float share = clamp(t1 * wa + (1.0 - t1) * wb, 0.0, 1.0);
        det *= 1.0 - share * (1.0 - detail);
      }
      if (uTemplate == 1) {
        float tbb = zb == BLACK ? ta : tb;
        float w = a * ma * (t1 * ta + (1.0 - t1) * tbb);
        if (w > 0.0) {
          // the colour as the maker's template shades it (Template::show)
          vec3 shown;
          for (int k = 0; k < 3; k++) {
            float cs = toSrgb(clamp(col[k], 0.0, 1.0));
            float r = tpl[k] / max(uMuRef[k], 1e-3);
            float dk = cs * clamp(1.0 + (r - 1.0) * 0.6, 0.5, 1.1);
            shown[k] = toLin(clamp(cs + (dk - cs) * detail, 0.0, 1.0));
          }
          pa += (shown - pa) * w;
          pb += (shown - pb) * w;
          det *= 1.0 - w;
        }
      }
    }
  }
  // the texel from its paint (paint.rs finish)
  vec3 dl = c0 - t * pa0 - (1.0 - t) * pb0;
  vec3 mixed = t * pa + (1.0 - t) * pb;
  vec3 was = t * pa0 + (1.0 - t) * pb0;
  float moved = length(mixed - was);
  float w = clamp(moved / 0.05, 0.0, 1.0);
  float r = clamp(1.0 + 0.6 * det * lum(dl) / max(lum(was), 0.02), 0.6, 1.2);
  vec3 exact = det * dl + mixed;
  vec3 light = mixed * r;
  vec3 col = clamp(exact + (light - exact) * w, 0.0, 1.0);
  if (seeThrough) col = base.rgb;
  float alpha = glassCover <= 0.0 ? base.a : base.a + (1.0 - base.a) * min(glassCover, 1.0);
  o = vec4(col, alpha);
  gl_FragDepth = 1.0 - clamp(decals * 0.25, 0.0, 0.999);
}
`;

/** The base into the paint's texture first (the texels no triangle covers keep it). */
export const COPY_FS = `#version 300 es
precision highp float;
uniform sampler2D uBase;
uniform int uLinearise;
uniform vec2 uSize;
${COLOUR}
out vec4 o;
void main() {
  vec4 b = textureLod(uBase, gl_FragCoord.xy / uSize, 0.0);
  if (uLinearise == 1) b.rgb = lin3(b.rgb);
  o = b;
}
`;

/** The bleed (paint::composite): round the covered texels the colour next to them, and a
 * print's alpha on the glass; a see-through map's colours stay. */
export const SPREAD_FS = `#version 300 es
precision highp float;
uniform sampler2D uPaint;
uniform sampler2D uMask;
uniform sampler2D uBase;
uniform int uLinearise;
${COLOUR}
out vec4 o;
vec4 baseAt(ivec2 p, ivec2 size) {
  vec4 b = textureLod(uBase, (vec2(p) + 0.5) / vec2(size), 0.0);
  if (uLinearise == 1) b.rgb = lin3(b.rgb);
  return b;
}
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  ivec2 size = textureSize(uMask, 0);
  vec4 m = texelFetch(uMask, p, 0);
  vec4 here = texelFetch(uPaint, p, 0);
  int code = int(round(m.b * 255.0));
  if (m.a > 0.5 || code >= 81) {
    o = here;
    return;
  }
  ivec2 src = p + ivec2(code / 9 - 4, code - (code / 9) * 9 - 4);
  vec4 s = texelFetch(uPaint, src, 0);
  vec4 sb = baseAt(src, size);
  vec4 b = baseAt(p, size);
  int bits = int(round(texelFetch(uMask, src, 0).g * 255.0));
  o = vec4((bits & 4) != 0 ? b.rgb : s.rgb, abs(s.a - sb.a) > 0.5 / 255.0 ? s.a : b.a);
}
`;

// ------------------------------------------------------------ the brush

/** A piece of a stroke (BrushStroke::cover) into the stroke's own cover (MAX). */
export const STROKE_FS = `#version 300 es
precision highp float;
precision highp int;
${NET_IN}
uniform vec4 uPts[64];
uniform vec4 uNrm[64];
uniform int uCount;
uniform vec3 uLo;
uniform vec3 uHi;
uniform vec3 uStroke;   // radius (m), hardness, cover
out vec4 o;
void main() {
  vec3 p = vPos;
  if (any(lessThan(p, uLo)) || any(greaterThan(p, uHi))) discard;
  vec3 n = normalHere();
  float best = 1e30;
  for (int i = 0; i < 64; i++) {
    if (i >= uCount) break;
    int j = min(i + 1, uCount - 1);
    vec3 q0 = uPts[i].xyz;
    vec3 e = uPts[j].xyz - q0;
    float ee = dot(e, e);
    float t = ee > 1e-10 ? clamp(dot(p - q0, e) / ee, 0.0, 1.0) : 0.0;
    float d = distance(p, q0 + e * t);
    if (d < best) {
      vec3 nn = uNrm[i].xyz + uNrm[j].xyz;
      if (dot(nn, nn) < 1e-6 || dot(normalize(nn), n) > 0.25) best = d;
    }
  }
  if (best > 1e29) discard;
  float x = best / uStroke.x;
  float core = min(uStroke.y, 0.98);
  float t = clamp((x - core) / (1.0 - core), 0.0, 1.0);
  float c = (1.0 - t * t * (3.0 - 2.0 * t)) * uStroke.z;
  if (c <= 0.0) discard;
  o = vec4(c, 0.0, 0.0, 1.0);
}
`;

/** A stroke's cover into its layer's (blended: the brush over, the eraser away). */
export const LAYER_FS = `#version 300 es
precision highp float;
uniform sampler2D uSrc;
out vec4 o;
void main() {
  float c = texelFetch(uSrc, ivec2(gl_FragCoord.xy), 0).r;
  o = vec4(c, c, c, c);
}
`;

// ------------------------------------------------------------ pointing at the bus

/** The bus under the mouse: its place (o3d) and normal. */
export const PICK_VS = `#version 300 es
precision highp float;
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNorm;
layout(location = 2) in vec2 aUv;
layout(location = 3) in float aClass;
uniform mat4 uMat;
out vec3 vPos;
out vec3 vNorm;
out vec2 vUv;
void main() {
  vPos = aPos;
  vNorm = aNorm;
  vUv = aUv;
  gl_Position = uMat * vec4(-aPos.x, aPos.y, aPos.z, 1.0);
}
`;
export const PICK_FS = `#version 300 es
precision highp float;
in vec3 vPos;
in vec3 vNorm;
in vec2 vUv;
uniform float uTarget;
layout(location = 0) out vec4 o0;
layout(location = 1) out vec4 o1;
layout(location = 2) out vec4 o2;
void main() {
  o0 = vec4(vPos, 1.0);
  o1 = vec4(vNorm, 1.0);
  o2 = vec4(uTarget, vUv, 1.0);
}
`;

/** The base's colour at a texel, as sRGB (a colour area's zone). */
export const SAMPLE_FS = `#version 300 es
precision highp float;
uniform sampler2D uBase;
uniform vec2 uAt;
uniform int uLinearise;
${COLOUR}
out vec4 o;
void main() {
  vec4 b = textureLod(uBase, uAt, 0.0);
  vec3 c = uLinearise == 1 ? b.rgb : vec3(toSrgb(b.r), toSrgb(b.g), toSrgb(b.b));
  o = vec4(c, b.a);
}
`;

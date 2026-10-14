// The shaders of the 3D view (Omsi-Hub's Bus3D, the old app's `bus3d/shaders.ts`).
//
// One program for the bus, OMSI's material rules as uniforms (no variants: the draws are few,
// opaque pieces being joined per material). Beside it the sky, the floor, the shadow map, the
// contact shadow and its blur.
//
// LIGHT (an idea of openOMSI's shader.wgsl):
//   light = sun · N·L · shadow + sky · (0.5 + 0.5·n.y) · mix(h, 1, shadow) + ambient
//   (h = the sky light left in the shade: openOMSI 0.6, here 0.8 - `LIGHT` in draw.js)
//   colour = texture · diffuse · light + texture · emissive, plus Blinn-Phong on the sun side
// A reflection only where the cfg says `[matl_envmap]`: k = min(mask · strength, 1); on glass
// k · (0.18 + 0.82·(1-N·V)^4) over the pane, on paint k · gloss · Schlick (F0 0.04). What it
// reflects is the same sky and floor as are drawn, worked out in the shader rather than in a
// cube map first.
//
// TONE MAPPING: Khronos PBR Neutral (https://github.com/KhronosGroup/ToneMapping), then sRGB
// in the shader: the canvas is plain RGBA8 drawn through a 4x MSAA framebuffer of our own.

/** The per-picture block (std140); see `fillBlock` in draw.js. */
const FRAME_BLOCK = /* glsl */ `
layout(std140) uniform Frame {
  mat4 uViewProj;
  mat4 uShadowMat;
  mat4 uInvViewProj;
  vec4 uEye;        // xyz eye, w exposure
  vec4 uSunDir;     // xyz towards the sun, w the sun's azimuth (rad)
  vec4 uSunColour;  // rgb sun, w shadow radius (in map coordinates)
  vec4 uSkyLight;   // rgb light from above, w paint gloss
  vec4 uAmbient;    // rgb ambient, w cloud size (m)
  vec4 uFloor;      // rgb floor albedo, w floor radius (m)
  vec4 uContact;    // xy min X/Z of the contact map, zw 1/size
  vec4 uMisc;       // x sky texture, y clouds, z normal offset (m), w depth bias
  vec4 uMisc2;      // x noise seed, y contact strength, z sky u of the sun, w sky light in the shade
};
`;

const COMMON = /* glsl */ `
const float PI = 3.14159265;

vec3 toSrgb(vec3 c) {
  c = clamp(c, 0.0, 1.0);
  return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}
float toSrgb1(float c) {
  c = clamp(c, 0.0, 1.0);
  return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}
float luma(vec3 c) {
  return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// Khronos PBR Neutral
vec3 neutral(vec3 colour) {
  const float startCompression = 0.8 - 0.04;
  const float desaturation = 0.15;
  float x = min(colour.r, min(colour.g, colour.b));
  float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
  colour -= offset;
  float peak = max(colour.r, max(colour.g, colour.b));
  if (peak < startCompression) return colour;
  const float d = 1.0 - startCompression;
  float newPeak = 1.0 - d * d / (peak + d - startCompression);
  colour *= newPeak / peak;
  float g = 1.0 - 1.0 / (desaturation * (peak - newPeak) + 1.0);
  return mix(colour, vec3(newPeak), g);
}

// A little noise against bands in the sky and on the floor (half a step of 8 bits).
float noise(vec2 p) {
  return fract(sin(dot(p + uMisc2.x, vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
}
vec4 outColour(vec3 linear, float alpha) {
  return vec4(toSrgb(neutral(linear)) + noise(gl_FragCoord.xy) / 255.0, alpha);
}
`;

/** OMSI's sky as a function of a direction, for the dome and for reflections. */
const SKY = /* glsl */ `
uniform sampler2D uSkyTex;
uniform sampler2D uCloudTex;

vec2 skyUv(vec3 d) {
  float height = asin(clamp(d.y, -1.0, 1.0));
  float az = atan(d.x, -d.z);
  // u = round about from the sun (the light part of himmel01 lies at uMisc2.z), v = height
  float u = fract(uMisc2.z + (az - uSunDir.w) / (2.0 * PI));
  float v = clamp(1.0 - max(height, 0.0) / (0.5 * PI), 0.002, 0.996);
  return vec2(u, v);
}

vec3 skyNoClouds(vec3 d, float lod) {
  if (uMisc.x > 0.5) return textureLod(uSkyTex, skyUv(d), lod).rgb;
  // no envir.cfg: a gradient of our own from the horizon to the zenith
  float t = pow(clamp(d.y, 0.0, 1.0), 0.45);
  return mix(vec3(0.42, 0.55, 0.72), vec3(0.10, 0.24, 0.55), t);
}

vec3 skyWithClouds(vec3 d) {
  vec3 c = skyNoClouds(d, 0.0);
  if (uMisc.y > 0.5 && d.y > 0.01) {
    // A layer of clouds 1500 m up, as OMSI's. Low in the sky it becomes a streaky smear (the
    // texture lies at a slant); the panorama's own clouds are there, so only from twelve
    // degrees or so.
    vec2 p = (uEye.xz + d.xz * (1500.0 / d.y)) / uAmbient.w;
    float a = texture(uCloudTex, p).a * smoothstep(0.2, 0.5, d.y);
    vec3 cloud = vec3(0.93, 0.94, 0.97) * (0.85 + 0.15 * max(dot(d, uSunDir.xyz), 0.0));
    c = mix(c, cloud, a * 0.8);
  }
  return c;
}

// The haze at the horizon: the sky there, blurred (mip level 5), a little desaturated and
// lighter, as air far off. The panorama runs into it over its last two degrees, the floor far
// off too: no hard line where the clouds end and no band between sky and floor.
vec3 haze(vec3 d) {
  vec3 h = skyNoClouds(vec3(d.x, 0.001, d.z), 5.0);
  return mix(h, vec3(dot(h, vec3(0.2126, 0.7152, 0.0722)) * 1.12), 0.35);
}

// What a smooth surface sees in direction d: sky, or the floor under it (reflections).
vec3 surroundings(vec3 d) {
  vec3 horizon = skyNoClouds(vec3(d.x, 0.0, d.z) + vec3(0.0, 0.001, 0.0), 4.0);
  if (d.y >= 0.0) return mix(horizon, skyNoClouds(d, 4.0), smoothstep(0.0, 0.08, d.y));
  vec3 floorC = uFloor.rgb * (uSunColour.rgb * max(uSunDir.y, 0.0) + uSkyLight.rgb + uAmbient.rgb);
  return mix(horizon, floorC, smoothstep(0.0, -0.12, d.y));
}
`;

/** The shadow map with PCF: 5 samples first, and only on the edge 16 more. */
const SHADOW = /* glsl */ `
uniform highp sampler2DShadow uShadowMap;
const vec2 POISSON[16] = vec2[16](
  vec2(-0.9420, -0.3991), vec2(0.9456, -0.7689), vec2(-0.0942, -0.9294), vec2(0.3450, 0.2939),
  vec2(-0.9159, 0.4577), vec2(-0.8154, -0.8791), vec2(-0.3828, 0.2768), vec2(0.9748, 0.7565),
  vec2(0.4432, -0.9751), vec2(0.5374, -0.4737), vec2(-0.2650, -0.4189), vec2(0.7920, 0.1909),
  vec2(-0.2419, 0.9971), vec2(-0.8141, 0.9144), vec2(0.1998, 0.7864), vec2(0.1438, -0.1410)
);
float shadow(vec3 p, vec3 n, float r) {
  vec3 q = p + n * uMisc.z;
  vec4 s = uShadowMat * vec4(q, 1.0);
  vec3 c = s.xyz * 0.5 + 0.5;
  if (c.x <= 0.0 || c.x >= 1.0 || c.y <= 0.0 || c.y >= 1.0 || c.z >= 1.0) return 1.0;
  float z = c.z - uMisc.w;
  float sum = texture(uShadowMap, vec3(c.xy, z));
  for (int i = 0; i < 4; i++) sum += texture(uShadowMap, vec3(c.xy + POISSON[i] * r, z));
  if (sum < 0.001 || sum > 4.999) return sum / 5.0;
  for (int i = 4; i < 16; i++) sum += texture(uShadowMap, vec3(c.xy + POISSON[i] * r, z));
  return sum / 17.0;
}
`;

// ------------------------------------------------------------ the bus

export const BUS_VS = /* glsl */ `#version 300 es
precision highp float;
${FRAME_BLOCK}
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec2 aUv;
out vec3 vWorld;
out vec3 vNormal;
out vec2 vUv;
void main() {
  // o3d (left-handed) into the world (right-handed): x mirrored
  vec3 p = vec3(-aPos.x, aPos.y, aPos.z);
  vWorld = p;
  vNormal = vec3(-aNormal.x, aNormal.y, aNormal.z);
  vUv = aUv;
  gl_Position = uViewProj * vec4(p, 1.0);
}
`;

export const BUS_FS = /* glsl */ `#version 300 es
precision highp float;
precision highp int;
${FRAME_BLOCK}
${COMMON}
${SKY}
${SHADOW}
in vec3 vWorld;
in vec3 vNormal;
in vec2 vUv;
uniform sampler2D uTex;
uniform sampler2D uTrans;
uniform sampler2D uMask;
uniform vec4 uDiffuse;
uniform vec4 uSpec;       // rgb, power
uniform vec3 uEmissive;
uniform int uMode;        // 0 opaque, 1 alpha test, 2 blended, 3 [isshadow]
uniform int uTexMode;     // 0 none, 1 texture, 2 texture still to be made linear (S3TC without sRGB)
uniform int uTransMode;   // 0 its own alpha, 1 the map's alpha, 2 the map's brightness
uniform int uMaskMode;    // 0 the texture's alpha, 1 the mask's alpha, 2 the mask's brightness
uniform int uGlass;
uniform int uFlat;        // 1: white only (a mask), 2: the draw's colour (diagnosis)
uniform vec3 uId;
uniform float uEnv;
uniform float uAlphaScale;
out vec4 fragColour;

void main() {
  if (uFlat == 1) {
    fragColour = vec4(1.0);
    return;
  }
  vec3 V = normalize(uEye.xyz - vWorld);
  vec3 n = vNormal;
  float ln = length(n);
  if (ln < 1e-4) {
    // a normal of length 0 (0.19 % of the vertices): the face itself, towards the viewer
    n = normalize(cross(dFdx(vWorld), dFdy(vWorld)));
    if (dot(n, V) < 0.0) n = -n;
  } else {
    n /= ln;
  }

  vec4 tex = vec4(1.0);
  if (uTexMode >= 1) {
    tex = texture(uTex, vUv);
    if (uTexMode == 2) tex.rgb = pow(tex.rgb, vec3(2.2));
  }

  float alpha = 1.0;
  if (uMode >= 1) {
    float a = tex.a;
    if (uTransMode == 1) a = texture(uTrans, vUv).a;
    else if (uTransMode == 2) a = toSrgb1(luma(texture(uTrans, vUv).rgb));
    alpha = a * uDiffuse.a * uAlphaScale;
  }
  if (uMode == 1) {
    // alpha to coverage with a sharpened alpha: the threshold stays at 0.5
    float w = max(fwidth(alpha), 1e-4);
    alpha = clamp((alpha - 0.5) / w + 0.5, 0.0, 1.0);
    if (alpha <= 0.0) discard;
  } else if (uMode >= 2) {
    if (uMode == 3) alpha *= 0.6;
    if (alpha <= 0.003) discard;
  }
  if (uFlat == 2) {
    if (uMode >= 1 && alpha < 0.5) discard;
    fragColour = vec4(uId, 1.0);
    return;
  }

  vec3 L = uSunDir.xyz;
  float NdL = dot(n, L);
  float s = NdL > 0.0 ? shadow(vWorld, n, uSunColour.w) : 0.0;
  vec3 sun = uSunColour.rgb * max(NdL, 0.0) * s;
  vec3 light = sun + uSkyLight.rgb * (0.5 + 0.5 * n.y) * mix(uMisc2.w, 1.0, s) + uAmbient.rgb;
  vec3 colour = tex.rgb * uDiffuse.rgb * light + tex.rgb * uEmissive;

  if (NdL > 0.0 && uSpec.a > 0.0 && (uSpec.r + uSpec.g + uSpec.b) > 0.0) {
    vec3 H = normalize(L + V);
    colour += uSpec.rgb * pow(max(dot(n, H), 0.0), uSpec.a) * uSunColour.rgb * s;
  }

  if (uEnv > 0.0) {
    // the mask: the texture's alpha (OMSI's reflection mask), or a mask texture of its own;
    // on glass the alpha is the transparency and no mask: then 1
    float m = uGlass == 1 ? 1.0 : tex.a;
    if (uMaskMode == 1) m = texture(uMask, vUv).a;
    else if (uMaskMode == 2) m = toSrgb1(luma(texture(uMask, vUv).rgb));
    float k = min(m * uEnv, 1.0);
    float NdV = clamp(dot(n, V), 0.0, 1.0);
    vec3 env = surroundings(reflect(-V, n));
    if (uGlass == 1) {
      // glass: the reflection over the pane, stronger at a slant
      float f = k * (0.18 + 0.82 * pow(1.0 - NdV, 4.0));
      float a2 = alpha + f * (1.0 - alpha);
      colour = (colour * alpha * (1.0 - f) + env * f / max(uEye.w, 1e-3)) / max(a2, 1e-4);
      alpha = a2;
    } else {
      // paint: damped, and only on the mask
      float f = k * uSkyLight.w * (0.04 + 0.96 * pow(1.0 - NdV, 5.0));
      colour = mix(colour, env / max(uEye.w, 1e-3), f);
    }
  }

  fragColour = outColour(colour * uEye.w, uMode == 0 ? 1.0 : alpha);
}
`;

// ------------------------------------------------------------ the shadow map

export const SHADOW_VS = /* glsl */ `#version 300 es
precision highp float;
${FRAME_BLOCK}
layout(location = 0) in vec3 aPos;
layout(location = 2) in vec2 aUv;
out vec2 vUv;
void main() {
  vUv = aUv;
  gl_Position = uShadowMat * vec4(-aPos.x, aPos.y, aPos.z, 1.0);
}
`;

export const SHADOW_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 vUv;
uniform sampler2D uTex;
uniform int uAlphaTest;
void main() {
  if (uAlphaTest == 1 && texture(uTex, vUv).a < 0.5) discard;
}
`;

// ------------------------------------------------------------ the contact shadow

/** Seen from below: how close the bus is above this bit of floor (up to 0.6 m). */
export const CONTACT_VS = /* glsl */ `#version 300 es
precision highp float;
layout(location = 0) in vec3 aPos;
uniform mat4 uContactMat;
out float vHeight;
void main() {
  vec3 p = vec3(-aPos.x, aPos.y, aPos.z);
  vHeight = p.y;
  gl_Position = uContactMat * vec4(p, 1.0);
}
`;

export const CONTACT_FS = /* glsl */ `#version 300 es
precision highp float;
in float vHeight;
out vec4 o;
void main() {
  float d = 1.0 - clamp(vHeight / 0.6, 0.0, 1.0);
  o = vec4(d, 0.0, 0.0, 1.0);
}
`;

/** One triangle over the whole picture. */
export const SCREEN_VS = /* glsl */ `#version 300 es
precision highp float;
out vec2 vScreen;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vScreen = p * 2.0 - 1.0;
  gl_Position = vec4(p * 2.0 - 1.0, 1.0, 1.0);
}
`;

/** A Gauss of 13 taps along one axis; twice both axes. */
export const BLUR_FS = /* glsl */ `#version 300 es
precision highp float;
uniform sampler2D uSrc;
uniform vec2 uStep;
out vec4 o;
void main() {
  vec2 uv = gl_FragCoord.xy / vec2(textureSize(uSrc, 0));
  const float G[7] = float[7](0.1512, 0.1435, 0.1226, 0.0943, 0.0653, 0.0407, 0.0228);
  float s = texture(uSrc, uv).r * G[0];
  for (int i = 1; i < 7; i++) {
    s += texture(uSrc, uv + uStep * float(i)).r * G[i];
    s += texture(uSrc, uv - uStep * float(i)).r * G[i];
  }
  o = vec4(s / 1.1296, 0.0, 0.0, 1.0);
}
`;

// ------------------------------------------------------------ the sky and the floor

export const SKY_FS = /* glsl */ `#version 300 es
precision highp float;
${FRAME_BLOCK}
${COMMON}
${SKY}
in vec2 vScreen;
out vec4 fragColour;
void main() {
  vec4 a = uInvViewProj * vec4(vScreen, -1.0, 1.0);
  vec4 b = uInvViewProj * vec4(vScreen, 1.0, 1.0);
  vec3 d = normalize(b.xyz / b.w - a.xyz / a.w);
  // under the horizon (past the floor): the haze; above, the panorama runs into it over two degrees
  vec3 c = d.y >= 0.0 ? mix(haze(d), skyWithClouds(d), smoothstep(0.0, 0.035, d.y)) : haze(d);
  fragColour = outColour(c, 1.0);
}
`;

export const FLOOR_VS = /* glsl */ `#version 300 es
precision highp float;
${FRAME_BLOCK}
layout(location = 0) in vec2 aXz;
out vec3 vWorld;
void main() {
  vec3 p = vec3(aXz.x * uFloor.w + uEye.x, 0.0, aXz.y * uFloor.w + uEye.z);
  vWorld = p;
  gl_Position = uViewProj * vec4(p, 1.0);
}
`;

export const FLOOR_FS = /* glsl */ `#version 300 es
precision highp float;
${FRAME_BLOCK}
${COMMON}
${SKY}
${SHADOW}
uniform sampler2D uContactTex;
// 1: the photo: no floor, only the contact shadow as alpha, so that it fits any tile
uniform int uPhoto;
in vec3 vWorld;
out vec4 fragColour;
void main() {
  vec3 n = vec3(0.0, 1.0, 0.0);
  vec2 cuv = (vWorld.xz - uContact.xy) * uContact.zw;
  float occ = 0.0;
  if (cuv.x > 0.0 && cuv.x < 1.0 && cuv.y > 0.0 && cuv.y < 1.0) occ = texture(uContactTex, cuv).r * uMisc2.y;
  if (uPhoto == 1) {
    fragColour = vec4(0.0, 0.0, 0.0, clamp(occ * 0.7, 0.0, 0.7));
    return;
  }
  // a softer edge on the floor than on the bus (twice the radius)
  float s = shadow(vWorld, n, uSunColour.w * 2.0);
  vec3 sun = uSunColour.rgb * max(uSunDir.y, 0.0) * s * (1.0 - 0.35 * occ);
  vec3 around = (uSkyLight.rgb * mix(uMisc2.w, 1.0, s) + uAmbient.rgb) * (1.0 - 0.9 * occ);
  vec3 colour = uFloor.rgb * (sun + around) * uEye.w;
  // air far off: from fifteen metres the floor runs into the horizon's haze (gone at 400 m,
  // the disc's edge)
  vec3 d = normalize(vWorld - uEye.xyz);
  float dist = length(vWorld.xz - uEye.xz);
  float f = max(1.0 - exp(-max(dist - 15.0, 0.0) / 110.0), smoothstep(uFloor.w * 0.8, uFloor.w * 0.98, dist));
  fragColour = outColour(mix(colour, haze(d), f), 1.0);
}
`;

/** Compile and link a program; throws with the logs when it does not link. */
export function program(gl, vs, fs) {
  const make = (kind, src) => {
    const s = gl.createShader(kind);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  };
  const p = gl.createProgram();
  const v = make(gl.VERTEX_SHADER, vs);
  const f = make(gl.FRAGMENT_SHADER, fs);
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    const log = `${gl.getShaderInfoLog(v) || ''}\n${gl.getShaderInfoLog(f) || ''}\n${gl.getProgramInfoLog(p) || ''}`;
    throw new Error(`shader: ${log.trim()}`);
  }
  return p;
}

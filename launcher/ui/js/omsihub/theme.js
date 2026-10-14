// Omsi-Hub's look: the light and the dark look and the accent, as our game-drawn launcher has
// them (`crates/omsi-launcher-ui/src/theme.rs`, `accent.rs`). The choice is the settings' -
// `dark_mode` and `accent=#rrggbb` - so both launchers show what the player chose in either.
// The colours that do not depend on the choice are omsihub.css's; what the accent decides
// (its shades, the ink on it, the light look's ground) is worked out here and handed to the
// page as CSS variables on <html>.

import { S } from '../store.js';

/** The logo's orange (`assets/logos/omsi-orange.svg`): the accent unless one is chosen. */
export const DEFAULT = 0xf58620;

/** The choices offered (`accent::PRESETS`): an English name (translated where shown) and the
 * colour. */
export const PRESETS = [
  ['Orange', DEFAULT],
  ['Blue', 0x2a75f7],
  ['Green', 0x23a65a],
  ['Red', 0xe43334],
  ['Purple', 0x8b5cf6],
  ['Teal', 0x14c6bc],
  ['Pink', 0xe8459a],
  ['Yellow', 0xf6c90d],
];

/** The ground the accent must stand off (`accent::GROUND`), and how far (WCAG contrast). */
const GROUND = [9, 12, 24];
const MIN_ON_GROUND = 3;
/** Text on the accent: white while it reads at this contrast, else this dark ink. */
const WHITE_ON = 3;
const INK = [24, 16, 6];
/** The light look's quiet ink (`theme::light::TEXT_DIM`) and sheet (`light::PANEL`). */
const LIGHT_DIM = [100, 95, 98];
const LIGHT_PANEL = [255, 254, 252];

const unpack = (rgb) => [(rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255];
const hex = (c) => '#' + c.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('');
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const lighten = (c, t) => mix(c, [255, 255, 255], t);
const darken = (c, t) => mix(c, [0, 0, 0], t);

/** WCAG relative luminance and contrast of sRGB colours (0..255). */
export function luminance(c) {
  const lin = (v) => {
    const s = Math.max(0, Math.min(1, v / 255));
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
}

export function contrast(a, b) {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** `#rrggbb`, `rrggbb` or `#rgb` as 0xRRGGBB (null when it is none of them). */
export function parseHex(s) {
  const h = String(s || '').trim().replace(/^#/, '');
  if (!/^[0-9a-f]+$/i.test(h)) return null;
  if (h.length === 6) return parseInt(h, 16);
  if (h.length === 3) return parseInt(h.split('').map((x) => x + x).join(''), 16);
  return null;
}

export const hexOf = (rgb) => '#' + (rgb & 0xffffff).toString(16).padStart(6, '0');

/** `c` lifted towards white until it stands off the ground (`accent::readable`). */
function readable(c) {
  let out = c;
  let t = 0;
  while (contrast(out, GROUND) < MIN_ON_GROUND && t < 1) {
    t += 0.02;
    out = c.map((v) => Math.round(v + (255 - v) * t));
  }
  return out;
}

/** The accent's shades (`accent::Shades`): the colour, under the mouse, pressed, as a mark on
 * a dark sheet, and the ink on it. */
export function shades(rgb) {
  const base = readable(unpack(rgb));
  const on = contrast(base, [255, 255, 255]) >= WHITE_ON ? [255, 255, 255] : INK;
  return { base, hover: lighten(base, 0.14), deep: darken(base, 0.17), soft: lighten(base, 0.38), on };
}

/** The light look's ground for the accent `rgb` (`theme::light_ground`): the accent's own light
 * tint - the launcher's cream for the orange - lifted towards white as far as the quiet ink
 * needs to read on it. */
export function lightGround(rgb) {
  const c = unpack(rgb);
  const base = rgb === DEFAULT ? [252, 234, 208] : c.map((v) => Math.round(v + (255 - v) * 0.55));
  let t = 0;
  let g = base;
  while (contrast(LIGHT_DIM, g) < 4.6 && t < 1) {
    t += 0.02;
    g = mix(base, [255, 255, 255], t);
  }
  return g;
}

/** The accent as words on the light sheet: as dark as it must be to read there (4.5:1), the
 * same hue (`theme::to_light` for words). */
function onLight(c) {
  let out = c;
  for (let k = 0; contrast(out, LIGHT_PANEL) < 4.5 && k < 60; k++) out = out.map((v) => v * 0.96);
  return out;
}

/** The accent chosen in the settings (0xRRGGBB). */
export const chosen = () => parseHex(S.settings?.accent) ?? DEFAULT;
/** The dark look chosen in the settings. */
export const dark = () => !!S.settings?.dark_mode;

let applied = '';

/** The look as the settings have it, on <html>: `data-theme` and the accent's variables. */
export function applyHubTheme() {
  const rgb = chosen();
  const isDark = dark();
  const key = `${rgb}|${isDark}`;
  if (key === applied) return;
  applied = key;
  const s = shades(rgb);
  const root = document.documentElement;
  root.dataset.theme = isDark ? 'dark' : 'light';
  const set = (k, v) => root.style.setProperty(k, v);
  set('--hub-accent', hex(s.base));
  set('--hub-accent-rgb', s.base.map(Math.round).join(', '));
  set('--hub-accent-hover', hex(s.hover));
  set('--hub-accent-deep', hex(s.deep));
  set('--hub-accent-chosen', hexOf(rgb));
  set('--hub-on-accent', hex(s.on));
  // (the accent as words or a mark on a sheet: lighter on the dark one, darker on the light)
  set('--hub-accent-text', hex(isDark ? s.soft : onLight(s.base)));
  // (and the soft one as a fill, in either look: the game-drawn launcher's `accent_2`)
  set('--hub-accent-soft-rgb', s.soft.map((v) => Math.round(Math.max(0, Math.min(255, v)))).join(', '));
  // the ground: the night blue, or the accent's own light tint
  set('--hub-ground', isDark ? '#0a142e' : hex(lightGround(rgb)));
}

// RosterFlow's colours (`crates/omsi-launcher-ui/src/company/planning/rosterflow.rs`, `Pal`):
// the dark look - Optibus's navy with one blue accent, orange warnings and red errors - and the
// light one made to match. The look follows the launcher's (the settings' `dark_mode`), as our
// game-drawn launcher's RosterFlow follows its own; the lines keep their own colours in both.
// The canvas draws with these; rosterflow.css has the same as its `--rf-*` variables.

import { dark } from '../theme.js';

const rgb = (r, g, b, a = 1) => [r, g, b, a];

/** The dark look: Optibus's navy, one blue accent, orange warnings, red errors. */
export const DARK = {
  bg: rgb(39, 46, 56),
  bar: rgb(33, 39, 48),
  tool: rgb(59, 74, 92),
  grid: rgb(39, 46, 56),
  band: rgb(44, 51, 61),
  labels: rgb(29, 33, 39),
  axis: rgb(33, 39, 48),
  status: rgb(31, 36, 44),
  text: rgb(226, 230, 236),
  dim: rgb(150, 158, 172),
  faint: rgb(104, 112, 126),
  line: rgb(255, 255, 255, 0.07),
  accent: rgb(63, 168, 255),
  warn: rgb(255, 166, 38),
  err: rgb(240, 72, 72),
  ok: rgb(70, 196, 120),
  grey: rgb(112, 124, 140),
  dead: rgb(20, 23, 28),
  pull: rgb(72, 86, 106),
  card: rgb(232, 236, 241),
  card_head: rgb(20, 60, 104),
  card_text: rgb(40, 48, 60),
  card_dim: rgb(110, 120, 134),
  card_pill: rgb(208, 214, 222),
  menu: rgb(222, 227, 233),
  menu_text: rgb(36, 44, 56),
  sheet: rgb(46, 54, 66),
  field: rgb(31, 37, 46),
  light: false,
};

/** The light look, made to match: white sheets on a cool grey, the same blue a shade deeper. */
export const LIGHT = {
  bg: rgb(240, 243, 247),
  bar: rgb(255, 255, 255),
  tool: rgb(226, 233, 242),
  grid: rgb(255, 255, 255),
  band: rgb(246, 248, 251),
  labels: rgb(236, 240, 245),
  axis: rgb(244, 246, 249),
  status: rgb(248, 250, 252),
  text: rgb(28, 36, 48),
  dim: rgb(92, 104, 120),
  faint: rgb(150, 160, 174),
  line: rgb(20, 36, 60, 0.09),
  accent: rgb(18, 124, 226),
  warn: rgb(226, 128, 10),
  err: rgb(214, 46, 46),
  ok: rgb(30, 150, 86),
  grey: rgb(150, 160, 174),
  dead: rgb(52, 58, 68),
  pull: rgb(138, 152, 172),
  card: rgb(255, 255, 255),
  card_head: rgb(20, 60, 104),
  card_text: rgb(34, 42, 54),
  card_dim: rgb(110, 120, 134),
  card_pill: rgb(226, 231, 238),
  menu: rgb(255, 255, 255),
  menu_text: rgb(34, 42, 54),
  sheet: rgb(255, 255, 255),
  field: rgb(246, 248, 251),
  light: true,
};

/** The look shown now. */
export const pal = () => (dark() ? DARK : LIGHT);

/** A colour as CSS, its alpha times `a`. */
export function css(c, a = 1) {
  return `rgba(${Math.round(c[0])}, ${Math.round(c[1])}, ${Math.round(c[2])}, ${(c[3] ?? 1) * a})`;
}

/** `c` with alpha `a` (its own alpha replaced, as `Color::alpha`). */
export const alpha = (c, a) => [c[0], c[1], c[2], a];
/** `c` towards white (`Color::lighten`) or black (`darken`) by `t`. */
export const lighten = (c, t) => [c[0] + (255 - c[0]) * t, c[1] + (255 - c[1]) * t, c[2] + (255 - c[2]) * t, c[3] ?? 1];
export const darken = (c, t) => [c[0] * (1 - t), c[1] * (1 - t), c[2] * (1 - t), c[3] ?? 1];
/** `a` mixed with `b` by `t` (`Color::mix`). */
export const mix = (a, b, t) => [0, 1, 2, 3].map((i) => (a[i] ?? 1) + ((b[i] ?? 1) - (a[i] ?? 1)) * t);

/** `#rrggbb` as a colour. */
export function hex(s) {
  const h = String(s || '').trim().replace(/^#/, '');
  const v = /^[0-9a-f]{6}$/i.test(h) ? parseInt(h, 16) : 0x4f81be;
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255, 1];
}

/** Ink on a colour: dark on light colours, white on dark (`ownlines::ink_on`). */
export function inkOn(c) {
  const lum = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255;
  return lum > 0.6 ? [26, 20, 0, 1] : [255, 255, 255, 1];
}

/** A tone name of the views ("warn", "err", "ok", "text", "soft", "dim", "faint", "accent") as a
 * colour of the look. */
export function tone(p, t) {
  switch (t) {
    case 'warn':
      return p.warn;
    case 'err':
    case 'danger':
      return p.err;
    case 'ok':
      return p.ok;
    case 'soft':
      return alpha(p.text, 0.82);
    case 'dim':
      return p.dim;
    case 'faint':
      return p.faint;
    case 'accent':
      return p.accent;
    default:
      return p.text;
  }
}

/** Our game-drawn launcher sizes text by the font's whole height (1.303 em for Hanken
 * Grotesk): a size of its own as CSS pixels. */
export const px = (n) => n / 1.303;

/** The canvas font of a size and weight of the game-drawn launcher. */
export const font = (size, weight = 500) => `${weight} ${px(size)}px "Hanken Grotesk", sans-serif`;

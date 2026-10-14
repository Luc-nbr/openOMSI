// RosterFlow's drawing on a canvas, after our game-drawn launcher's (`rf_brand.rs`,
// `rf_gantt.rs`): rounded boxes, the trips' arrow chips, deadheads, pull-outs and pull-ins,
// T-bars, relief arrows, drop zones, checkboxes, icons and words - every length in the stage's
// own units (as the game-drawn launcher's), every text size the game-drawn launcher's (its whole
// height: `font`). `Canvas` keeps a canvas as sharp as the screen draws it.

import { SYMBOLS } from '../company/symbols.js';
import { css, alpha, inkOn, font, px } from './pal.js';
import { tr } from '../bits.js';

// --- a canvas as sharp as the screen ---------------------------------------------------------

/** A canvas of `w` x `h` stage units, drawn at the screen's own pixels (the stage's scale and the
 * device's): its context, scaled so that the stage's units are drawn with it. */
export function prepare(canvas, w, h) {
  const box = canvas.getBoundingClientRect();
  const stage = canvas.offsetWidth ? box.width / canvas.offsetWidth : 1;
  const k = Math.max(1, (window.devicePixelRatio || 1) * (stage || 1));
  const W = Math.max(1, Math.round(w * k));
  const H = Math.max(1, Math.round(h * k));
  if (canvas.width !== W || canvas.height !== H) {
    canvas.width = W;
    canvas.height = H;
  }
  const ctx = canvas.getContext('2d');
  ctx.setTransform(k, 0, 0, k, 0, 0);
  ctx.clearRect(0, 0, w, h);
  ctx.textBaseline = 'alphabetic';
  return ctx;
}

// --- shapes ----------------------------------------------------------------------------------

export function rect(ctx, x, y, w, h, c) {
  if (w <= 0 || h <= 0) return;
  ctx.fillStyle = css(c);
  ctx.fillRect(x, y, w, h);
}

function roundPath(ctx, x, y, w, h, r) {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

export function rounded(ctx, x, y, w, h, r, c) {
  if (w <= 0 || h <= 0) return;
  roundPath(ctx, x, y, w, h, r);
  ctx.fillStyle = css(c);
  ctx.fill();
}

export function roundedBorder(ctx, x, y, w, h, r, width, c) {
  if (w <= 0 || h <= 0) return;
  roundPath(ctx, x + width / 2, y + width / 2, w - width, h - width, r);
  ctx.lineWidth = width;
  ctx.strokeStyle = css(c);
  ctx.stroke();
}

export function circle(ctx, x, y, r, c) {
  ctx.beginPath();
  ctx.arc(x, y, r, 0, Math.PI * 2);
  ctx.fillStyle = css(c);
  ctx.fill();
}

export function poly(ctx, pts, c) {
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath();
  ctx.fillStyle = css(c);
  ctx.fill();
}

export function stroke(ctx, pts, width, c) {
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.lineWidth = width;
  ctx.lineJoin = 'round';
  ctx.strokeStyle = css(c);
  ctx.stroke();
}

export function shadow(ctx, x, y, w, h, r, blur, c) {
  ctx.save();
  ctx.shadowColor = css(c);
  ctx.shadowBlur = blur;
  ctx.shadowOffsetY = 2;
  rounded(ctx, x, y, w, h, r, [0, 0, 0, 0.01]);
  ctx.restore();
}

export function clip(ctx, x, y, w, h) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, Math.max(0, w), Math.max(0, h));
  ctx.clip();
}

export const unclip = (ctx) => ctx.restore();

// --- words -----------------------------------------------------------------------------------

const caps = new Map();
/** The height of the capitals of a size (where `text_in` puts the baseline). */
function capHeight(ctx, size, weight) {
  const key = `${size}|${weight}`;
  let h = caps.get(key);
  if (h == null) {
    ctx.font = font(size, weight);
    const m = ctx.measureText('H');
    h = m.actualBoundingBoxAscent || px(size) * 0.7;
    caps.set(key, h);
  }
  return h;
}

/** How wide words are at a size and weight of the game-drawn launcher. */
export function width(ctx, text, size, weight = 500) {
  ctx.font = font(size, weight);
  return ctx.measureText(String(text)).width;
}

/** Words cut to `w` with "…" (`Fonts::fit`). */
export function fit(ctx, text, w, size, weight = 500) {
  const s = String(text);
  if (width(ctx, s, size, weight) <= w) return s;
  const cs = [...s];
  while (cs.length) {
    cs.pop();
    const t = cs.join('') + '…';
    if (width(ctx, t, size, weight) <= w) return t;
  }
  return '';
}

/** Words at `x` on the baseline `y` (`text_as_is`): returns their width. */
export function textAt(ctx, text, x, y, size, weight, c, align = 'left') {
  const s = String(text ?? '');
  ctx.font = font(size, weight);
  ctx.textAlign = align;
  ctx.fillStyle = css(c);
  ctx.fillText(s, x, y);
  return ctx.measureText(s).width;
}

/** Words in a box, in its middle up and down, cut to its width (`text_in`). */
export function textIn(ctx, text, x, y, w, h, size, weight, c, align = 'left') {
  const s = fit(ctx, String(text ?? ''), w, size, weight);
  const base = y + h / 2 + capHeight(ctx, size, weight) / 2;
  const at = align === 'center' ? x + w / 2 : align === 'right' ? x + w : x;
  return textAt(ctx, s, at, base, size, weight, c, align);
}

/** A small caps label (`rosterflow::caps`). */
export function capsIn(ctx, text, x, y, w, h, c, align = 'left') {
  return textIn(ctx, tr(text).toUpperCase(), x, y, w, h, 11, 700, c, align);
}

// --- icons -------------------------------------------------------------------------------------

/** Our own icons the Material set lacks (`assets/icons/custom`): strokes on a 24 box. */
export const STROKED = {
  livery_undo: ['M9 14L4 9l5-5', 'M4 9h10a6 6 0 0 1 0 12h-3'],
  livery_redo: ['M15 14l5-5-5-5', 'M20 9H10a6 6 0 0 0 0 12h3'],
  livery_unlock: ['M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z', 'M8 11V7.5a4 4 0 0 1 7.6-1.8'],
};

const paths = new Map();
function pathOf(d) {
  let p = paths.get(d);
  if (!p) {
    p = new Path2D(d);
    paths.set(d, p);
  }
  return p;
}

/** An icon centred at `x`, `y`, `size` units (`Ui::icon`). */
export function icon(ctx, name, x, y, size, c) {
  const st = STROKED[name];
  ctx.save();
  if (st) {
    const k = size / 24;
    ctx.translate(x - size / 2, y - size / 2);
    ctx.scale(k, k);
    ctx.lineWidth = 1.9;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = css(c);
    for (const d of st) ctx.stroke(pathOf(d));
  } else {
    const d = SYMBOLS[name] || SYMBOLS.info;
    const k = size / 960;
    ctx.translate(x - size / 2, y - size / 2);
    ctx.scale(k, k);
    ctx.translate(0, 960);
    ctx.fillStyle = css(c);
    ctx.fill(pathOf(d));
  }
  ctx.restore();
}

// --- RosterFlow's symbols ----------------------------------------------------------------------

/** A row's pitch, a chip's height, the ID column, a stats column, the axis band. */
export const ROW = 27;
export const CHIP = 18;
export const ID_W = 124;
export const STAT_W = 64;
export const AXIS = 28;

/** An arrow chip: rounded on its flat end, pointed on the other (`rf_brand::arrow_chip`). */
export function arrowChip(ctx, x, y, w, h, fill, right) {
  if (w <= 0.5 || h <= 0.5) return;
  const tip = Math.min(h * 0.45, w * 0.45);
  const rad = Math.min(h * 0.22, w * 0.25);
  if (right) {
    rounded(ctx, x, y, Math.max(1, w - tip + rad), h, rad, fill);
    rect(ctx, Math.max(x, x + w - tip - rad), y, Math.min(rad, w), h, fill);
    poly(ctx, [[x + w - tip, y], [x + w, y + h / 2], [x + w - tip, y + h]], fill);
  } else {
    rounded(ctx, x + tip - rad, y, Math.max(1, w - tip + rad), h, rad, fill);
    rect(ctx, x + tip, y, Math.min(rad, w), h, fill);
    poly(ctx, [[x + tip, y], [x + tip, y + h], [x, y + h / 2]], fill);
  }
}

/** The outline of an arrow chip (`rf_brand::arrow_points`). */
export function arrowPoints(x, y, w, h, right) {
  const tip = Math.min(h * 0.45, w * 0.45);
  if (right) return [[x, y], [x + w - tip, y], [x + w, y + h / 2], [x + w - tip, y + h], [x, y + h], [x, y]];
  return [[x + tip, y], [x + w, y], [x + w, y + h], [x + tip, y + h], [x, y + h / 2], [x + tip, y]];
}

/** A trip's chip (`rf_gantt::chip`): the line's colour, pointed the way it runs, its words as the
 * width allows; `outline` (chosen white, an issue red), `stripes` linked, `dot` moved off its
 * tour. */
export function chip(ctx, x, y, w, h, colour, right, label, outline, stripes, dot, a = 1) {
  arrowChip(ctx, x, y, w, h, alpha(colour, a), right);
  if (stripes) {
    ctx.save();
    roundClip(ctx, x, y, w, h, 3);
    for (let sx = x - h; sx < x + w; sx += 8) poly(ctx, [[sx, y + h], [sx + 3, y + h], [sx + 3 + h, y], [sx + h, y]], [0, 0, 0, 0.22 * a]);
    ctx.restore();
  }
  if (outline) {
    const p = arrowPoints(x - 1, y - 1, w + 2, h + 2, right);
    stroke(ctx, p, 1.6, outline);
  }
  if (dot) circle(ctx, right ? x + 5 : x + w - 5, y + h / 2, 2.6, [10, 12, 16, a]);
  const size = 11;
  const tip = Math.min(h * 0.45, w * 0.45);
  const room = w - tip - 4;
  if (label && room > 8) {
    const ink = alpha(inkOn(colour), a);
    const tx = right ? x + 2 : x + tip + 2;
    const cx = tx + room / 2;
    if (width(ctx, label, size, 700) <= room) textAt(ctx, label, cx, y + h / 2 + 4, size, 700, ink, 'center');
    else if (room > 18) textAt(ctx, [...String(label)].slice(0, Math.max(1, Math.floor(room / 8))).join('') + '…', cx, y + h / 2 + 4, size, 700, ink, 'center');
  }
}

function roundClip(ctx, x, y, w, h, r) {
  roundPath(ctx, x, y, w, h, r);
  ctx.clip();
}

/** A deadhead: a small dark rounded box (an orange dot where its time is estimated). */
export function deadhead(ctx, p, a, b, cy, estimated) {
  const w = Math.max(b - a, 9);
  const x = a + Math.min((b - a) - w, 0) * 0.5;
  const y = cy - CHIP * 0.42;
  rounded(ctx, x, y, w, CHIP * 0.84, 3, p.dead);
  roundedBorder(ctx, x, y, w, CHIP * 0.84, 3, 1, [255, 255, 255, 0.1]);
  if (estimated) circle(ctx, x + w / 2, cy, 3, p.warn);
}

/** A pull-out or pull-in: a grey-blue box (an orange dot where the company adds it itself). */
export function pull(ctx, p, a, b, cy, generated) {
  const w = Math.max(b - a, 12);
  const x = b - a < 12 ? b - 12 : a;
  rounded(ctx, x, cy - CHIP / 2, w, CHIP, 3, p.pull);
  if (generated) circle(ctx, x + w / 2, cy, 3, p.warn);
}

/** A T-bar: the pre- or post-trip check, a sign-on or a sign-off (`left`: its bar on the left). */
export function tBar(ctx, p, a, b, cy, left) {
  const col = p.light ? p.dim : [200, 206, 214, 1];
  const aa = Math.min(a, b - 6);
  rect(ctx, aa, cy - 0.75, b - aa, 1.5, col);
  rect(ctx, left ? aa : b - 1.5, cy - 7, 1.5, 14, col);
}

/** A relief arrow: down (the driver gets off), up (gets on), or both (handed over). */
export function reliefArrow(ctx, p, x, cy, up, down) {
  const col = p.light ? p.text : [255, 255, 255, 1];
  const h = CHIP * 0.62;
  rect(ctx, x - 0.6, cy - h, 1.2, h * 2, col);
  if (down) poly(ctx, [[x - 3.5, cy + h - 4], [x + 3.5, cy + h - 4], [x, cy + h + 1]], col);
  if (up) poly(ctx, [[x - 3.5, cy - h + 4], [x, cy - h - 1], [x + 3.5, cy - h + 4]], col);
}

/** A drop zone: an outlined slot, blue where nothing breaks and red where a rule does, its
 * stars inside (`rf_gantt::draw_zone`). */
export function zone(ctx, p, x, y, w, h, z, right, on) {
  const col = z.clean ? p.accent : p.err;
  const pts = arrowPoints(x, y, w, h, right);
  poly(ctx, pts.slice(0, 5), on ? alpha(col, 0.3) : alpha(p.grid, 0.9));
  stroke(ctx, pts, on ? 2.2 : 1.6, col);
  const cx = x + w / 2;
  textAt(ctx, String(z.stars), cx - 6, y + h / 2 + 4.5, 11.5, 700, p.text, 'center');
  icon(ctx, 'star', cx + 6, y + h / 2, 12, p.accent);
}

/** A row's checkbox (`rf_gantt::checkbox`). */
export function checkbox(ctx, p, cx, cy, on, hover) {
  const x = cx - 6.5;
  const y = cy - 6.5;
  if (on) {
    rounded(ctx, x, y, 13, 13, 2, p.accent);
    icon(ctx, 'check', cx, cy, 11, [255, 255, 255, 1]);
  } else roundedBorder(ctx, x, y, 13, 13, 2, 1.3, hover ? p.text : p.dim);
}

/** A part of a trip cut at a relief point on the way (`network::Part`): the trip's chip with a
 * straight edge where the drivers change - at its start (`cutStart`, the second part) or its end
 * (`cutEnd`, the first) - its words in what is left of it. */
export function cutChip(ctx, x, y, w, h, colour, right, label, outline, a, cutStart, cutEnd) {
  if (w <= 0.5 || h <= 0.5) return;
  // (the whole chip drawn on past the cut, and cut there)
  const ext = h * 1.5;
  const ex = cutStart ? x - ext : x;
  const ew = w + (cutStart ? ext : 0) + (cutEnd ? ext : 0);
  clip(ctx, x - (cutStart ? 0 : 3), y - 3, w + (cutStart ? 0 : 3) + (cutEnd ? 0 : 3), h + 6);
  chip(ctx, ex, y, ew, h, colour, right, '', outline, false, false, a);
  unclip(ctx);
  const size = 11;
  const tip = Math.min(h * 0.45, w * 0.45);
  const lead = right ? (cutStart ? 3 : 2) : cutStart ? 3 : tip + 2;
  const room = w - lead - (right ? (cutEnd ? 3 : tip + 2) : 3);
  if (label && room > 8 && width(ctx, label, size, 700) <= room) textAt(ctx, label, x + lead + room / 2, y + h / 2 + 4, size, 700, alpha(inkOn(colour), a), 'center');
}

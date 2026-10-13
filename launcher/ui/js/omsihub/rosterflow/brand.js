// RosterFlow's name, its mark and its opening (`crates/omsi-launcher-ui/src/company/planning/
// rf_brand.rs`). The mark is a scheduler's Gantt in small: three trip chips on a time axis,
// staggered as a bus's day hands over to the next - blue, amber and teal, the middle one
// pointing back. When RosterFlow opens, the chips slide in along the axis and settle into the
// mark, the name and "Bus Scheduling & Crew Rostering" come up beside it, and the studio fades
// in (1.5 s; a click or Escape skips it, and the setting "animations" off leaves it out).

import { html, useEffect, useRef } from '../../../vendor/preact-htm.js';
import { tr } from '../bits.js';
import { css, alpha } from './pal.js';
import { prepare, rect, rounded, roundedBorder, arrowChip, textAt, width, clip, unclip } from './draw.js';

export const NAME = 'RosterFlow';
export const SUBTITLE = 'Bus Scheduling & Crew Rostering';
/** The opening's length (seconds). */
export const INTRO = 1.5;

/** The mark's chips: where each lies in the tile (fractions), its colour, which way it points. */
const CHIPS = [
  [0.16, 0.24, 0.8, [63, 168, 255, 1], true],
  [0.42, 0.2, 0.72, [255, 172, 51, 1], false],
  [0.68, 0.34, 0.86, [38, 198, 170, 1], true],
];

const easeOut = (t) => 1 - (1 - t) ** 3;

/** The mark in a square at `x`, `y` of side `s`, `t` how far it has come together (1 together),
 * `a` its fade. */
export function drawMark(ctx, x, y, s, t = 1, a = 1) {
  const r = s * 0.22;
  ctx.save();
  const g = ctx.createLinearGradient(0, y, 0, y + s);
  g.addColorStop(0, css([30, 41, 62, a]));
  g.addColorStop(1, css([18, 25, 40, a]));
  ctx.beginPath();
  ctx.roundRect(x, y, s, s, r);
  ctx.fillStyle = g;
  ctx.fill();
  ctx.restore();
  roundedBorder(ctx, x, y, s, s, r, Math.max(1, s * 0.025), [255, 255, 255, 0.1 * a]);
  // the time axis: a faint line, and the hour ticks
  rect(ctx, x + s * 0.3, y + s * 0.1, Math.max(1, s * 0.018), s * 0.8, [255, 255, 255, 0.18 * a]);
  for (let k = 0; k < 3; k++) rect(ctx, x + s * (0.3 + 0.25 * k), y + s * 0.08, Math.max(1, s * 0.012), s * 0.05, [255, 255, 255, 0.28 * a]);
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(x, y, s, s, r);
  ctx.clip();
  CHIPS.forEach(([cy, ca, cb, c, right], k) => {
    const local = Math.max(0, Math.min(1, (t - k * 0.14) / 0.72));
    const e = easeOut(local);
    const from = k % 2 === 0 ? -0.9 : 0.9;
    const shift = (1 - e) * from * s;
    const h = s * 0.16;
    arrowChip(ctx, x + ca * s + shift, y + cy * s, (cb - ca) * s, h, alpha(c, a * Math.max(e, 0.15)), right);
  });
  ctx.restore();
}

/** The mark as a small canvas of its own. */
export function Mark({ size = 30 }) {
  const ref = useRef(null);
  useEffect(() => {
    const ctx = prepare(ref.current, size, size);
    drawMark(ctx, 0, 0, size);
  });
  return html`<canvas ref=${ref} class="rf-mark" style=${`width:${size}px;height:${size}px`}></canvas>`;
}

/** The name in two colours ("Roster" in the ink, "Flow" in the blue). */
export function Wordmark({ size = 17, dim }) {
  return html`<span class=${'rf-wordmark' + (dim ? ' dim' : '')} style=${`font-size:${size / 1.303}px`}><b>Roster</b><i>Flow</i></span>`;
}

/** The opening over the page at `t` seconds since it began (`rf_brand::opening`): the chips come
 * together into the mark (to 0.8 s), the name and the subtitle rise beside it (0.55 - 1.05 s),
 * and the whole fades to the studio under it (1.05 - 1.5 s). */
export function drawOpening(ctx, w, h, t, p) {
  const fade = 1 - Math.max(0, Math.min(1, (t - 1.05) / 0.45));
  if (fade <= 0) return;
  rect(ctx, 0, 0, w, h, alpha(p.bg, fade));
  const s = Math.max(64, Math.min(132, h * 0.22));
  const namePx = s * 0.42;
  const nameW = width(ctx, NAME, namePx, 800);
  const total = s + s * 0.28 + nameW;
  const mx = w / 2 - total / 2;
  const my = h / 2 - s / 2;
  // (a time axis across, its hours faint: what the chips run on)
  const ay = h / 2 + s * 0.78;
  rect(ctx, w * 0.18, ay, w * 0.64, 1, alpha(p.dim, 0.35 * fade));
  for (let k = 0; k <= 8; k++) {
    const x = w * (0.18 + 0.08 * k);
    rect(ctx, x, ay - 4, 1, 8, alpha(p.dim, 0.35 * fade));
    textAt(ctx, `${String(5 + 2 * k).padStart(2, '0')}:00`, x, ay + 8, 10, 700, alpha(p.dim, 0.6 * fade), 'center');
  }
  drawMark(ctx, mx, my, s, Math.max(0, Math.min(1, t / 0.8)), fade);
  const words = Math.max(0, Math.min(1, (t - 0.55) / 0.5));
  const rise = (1 - easeOut(words)) * 14;
  const nx = mx + s * 1.28;
  const a = words * fade;
  const wr = textAt(ctx, 'Roster', nx, h / 2 + namePx * 0.12 + rise , namePx, 800, alpha(p.text, a));
  textAt(ctx, 'Flow', nx + wr, h / 2 + namePx * 0.12 + rise , namePx, 800, alpha(p.accent, a));
  const sub = tr(SUBTITLE);
  textAt(ctx, sub, nx + 2, h / 2 + namePx * 0.98 + rise * 1.4, Math.max(12, namePx * 0.36), 500, alpha(p.dim, a));
}

/** The opening as a canvas over the page: it draws itself until it is over, then says so. */
export function Opening({ p, since, onDone }) {
  const ref = useRef(null);
  useEffect(() => {
    let raf = 0;
    const step = () => {
      const c = ref.current;
      if (!c) return;
      const t = (performance.now() - since) / 1000;
      if (t >= INTRO) {
        onDone();
        return;
      }
      const ctx = prepare(c, c.offsetWidth, c.offsetHeight);
      drawOpening(ctx, c.offsetWidth, c.offsetHeight, t, p);
      raf = requestAnimationFrame(step);
    };
    step();
    // (a hidden window gets no frames: the opening ends all the same)
    const end = setTimeout(onDone, INTRO * 1000 + 200);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(end);
    };
  }, [since]);
  return html`<canvas ref=${ref} class="rf-opening" onMouseDown=${(e) => (e.stopPropagation(), onDone())} onContextMenu=${(e) => (e.preventDefault(), onDone())}></canvas>`;
}

export { clip, unclip };

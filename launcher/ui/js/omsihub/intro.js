// The opening (Luc, Beta 53: it went with the game-drawn launcher in the 0.2.48 merge): when the
// launcher opens, Omsi-Hub's mark draws itself over the page, as our game-drawn launcher's did
// (`crates/omsi-launcher-ui/src/intro.rs` of Beta 51) - a ring round a bus that opens into the
// route under the name. The pen draws the ring from its open end round to its foot, the bus
// front springs up inside it once it has closed, the pen runs on along the line and the stops
// pop up as it reaches them, "Omsi-" rises letter by letter as the pen passes under it and
// "Hub" lands along its slant, the terminus rings when the pen arrives, and the mark settles and
// fades as the start comes up under it. About two and a half seconds, once per launch (not when
// the launcher wakes after a game: the page stays), skipped by a click or a key (the page then
// comes up quickly), and not at all with "Animations" off or the system's reduced motion.
//
// Everything is a pure function of the time since the first frame (`look`), as the game-drawn
// launcher's was: a frame looks the same at any frame rate, and `OMSI_LAUNCHER_INTRO=1.2` holds
// it 1.2 s in for pictures (`off` leaves it out). It is drawn into a layer of its own over the
// page's, outside its tree. (Its parts' classes are prefixed: upstream's `.pulse` is an animated dot,
// and it took the terminus's ring over once.)

import { S } from '../store.js';
import { MARK } from './logo.js';

// the timeline, in seconds from the first frame (`intro.rs`'s)
/** The ground's light comes up. */
const GLOW_IN = 0.35;
/** The pen sets down, presses in, and takes this long over the ring and the whole line. */
const DRAW_AT = 0.12;
const PEN_DOWN = 0.12;
const DRAW = 1.1;
/** A letter's rise (it starts as the pen passes under it, a little before), a stop's pop, the
 * bus front's, the ring going out from the terminus when the pen arrives. */
const LETTER = 0.55;
const LETTER_LEAD = 0.03;
const POP = 0.45;
const BUS_POP = 0.55;
const PULSE = 0.7;
/** How far "Hub"'s letters slide down their slant to land (of their capitals). */
const SLIDE = 0.42;
/** The hand-over: the mark settles and fades, and a moment later the ground lifts off. */
const LEAVE_AT = 1.95;
const MARK_OUT = 0.34;
const GROUND_DELAY = 0.14;
const GROUND_OUT = 0.55;
/** How much smaller the mark gets as it goes. */
const SETTLE = 0.06;
/** Skipped: the same, quickly, from where it stood. */
const QUICK_MARK = 0.12;
const QUICK_DELAY = 0.04;
const QUICK_GROUND = 0.26;
/** Below this the page answers the mouse again (it is plainly there). */
const PASS_BELOW = 0.2;
/** The most the clock moves in one frame: a frame that came late holds the line a moment. */
const MAX_STEP = 0.05;
/** The mark's drawing at a 1440 x 900 window: its ring (174 in the drawing) 64 px round. */
const UNIT = 64 / 174;
/** "Hub"'s slant; the capitals of "Omsi-" and of "Hub" (`logo.js`). */
const SLANT = 0.26;
const [LIGHT_CAP, HEAVY_CAP] = MARK.caps;

// --- easing (`intro.rs`'s) ------------------------------------------------------------------

function cubicBezier(x1, y1, x2, y2, x) {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const curve = (a, b, t) => 3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t;
  let lo = 0;
  let hi = 1;
  for (let k = 0; k < 40; k++) {
    const t = (lo + hi) / 2;
    if (curve(x1, x2, t) < x) lo = t;
    else hi = t;
  }
  return curve(y1, y2, (lo + hi) / 2);
}
const drawEase = (u) => cubicBezier(0.6, 0, 0.35, 1, u);
const easeIn = (u) => cubicBezier(0.4, 0, 1, 1, u);
const standard = (u) => cubicBezier(0.4, 0, 0.2, 1, u);
const easeInOut = (u) => cubicBezier(0.45, 0, 0.55, 1, u);
const outCubic = (u) => 1 - (1 - u) ** 3;
const outQuint = (u) => 1 - (1 - u) ** 5;
const smooth = (u) => {
  const v = Math.max(0, Math.min(1, u));
  return v * v * (3 - 2 * v);
};
const clamp01 = (v) => Math.max(0, Math.min(1, v));
/** A spring let go from 0 towards 1: a tenth over at its most, still on 1 at the end. */
function spring(p) {
  const v = clamp01(p);
  const [zeta, omega] = [0.6, 9];
  const wd = omega * Math.sqrt(1 - zeta * zeta);
  const raw = (t) => 1 - Math.exp(-zeta * omega * t) * (Math.cos(wd * t) + ((zeta * omega) / wd) * Math.sin(wd * t));
  return raw(v) + (1 - raw(1)) * v;
}

/** When the pen gets to `f` of the stroke (the inverse of its easing). */
function timeAt(f) {
  let lo = 0;
  let hi = 1;
  for (let k = 0; k < 32; k++) {
    const mid = (lo + hi) / 2;
    if (drawEase(mid) < f) lo = mid;
    else hi = mid;
  }
  return DRAW_AT + hi * DRAW;
}

/** The mark's and the ground's opacity and the mark's size at `t`: as the hand-over has them, or
 * the quick way out from where they stood (`leave`). */
function fade(t, leave) {
  const part = (from, len) => clamp01((t - from) / len);
  if (!leave) {
    const m = part(LEAVE_AT, MARK_OUT);
    return [1 - easeIn(m), 1 - standard(part(LEAVE_AT + GROUND_DELAY, GROUND_OUT)), 1 - SETTLE * easeInOut(m)];
  }
  const m = part(leave.at, QUICK_MARK);
  return [leave.mark * (1 - outCubic(m)), leave.ground * (1 - standard(part(leave.at + QUICK_DELAY, QUICK_GROUND))), leave.scale - SETTLE * 0.5 * outCubic(m)];
}

/** The opening at `t`: a pure function of the time, the way out taken (if any) and where the pen
 * meets each letter, stop and the ring's foot (`timing`). */
export function look(t, leave, timing) {
  const [mark, ground, scale] = fade(t, leave);
  const play = leave ? Math.min(t, leave.at) : t;
  const u = clamp01((play - DRAW_AT) / DRAW);
  const since = (at, len) => clamp01((play - at) / len);
  const pop = (f, len) => {
    const at = timeAt(f);
    return play <= at ? 0 : spring(since(at, len));
  };
  const arrived = DRAW_AT + DRAW;
  return {
    ground,
    glow: outCubic(clamp01(t / GLOW_IN)),
    mark,
    scale,
    drawn: play <= DRAW_AT ? 0 : drawEase(u),
    pen: outCubic(since(DRAW_AT, PEN_DOWN)),
    head: smooth(since(DRAW_AT, 0.15)) * (1 - smooth((u - 0.8) / 0.2)),
    bus: pop(timing.ring, BUS_POP),
    letters: timing.letters.map((f) => {
      const p = since(timeAt(f) - LETTER_LEAD, LETTER);
      return [outCubic(Math.min(1, p * 1.6)), 1 - outQuint(p)];
    }),
    stops: timing.stops.map((f) => pop(f, POP)),
    pulse: play > arrived && play < arrived + PULSE ? since(arrived, PULSE) : null,
  };
}

/** Where the pen gets to each thing, as shares of the stroke: each letter's middle, each stop,
 * the ring's foot (the ring closed round the bus). */
function timingOf(route) {
  const len = route.getTotalLength();
  // (the ring: from where the pen sets down round to its foot, where the line begins)
  const ring = len - (MARK.lineEnd - MARK.ringEnd);
  const along = (x) => clamp01((ring + x - MARK.ringEnd) / len);
  const letters = MARK.words.flatMap((w) => w.map(([, box]) => along((box[0] + box[2]) / 2)));
  return { len, ring: ring / len, letters, stops: MARK.stops.map(([x], k) => (k === MARK.stops.length - 1 ? 1 : along(x))) };
}

const SVGNS = 'http://www.w3.org/2000/svg';
const el = (tag, attrs = {}, parent = null) => {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.appendChild(e);
  return e;
};

/** Whether the opening is to play at all (the settings' "Animations", the system's reduced
 * motion, the self-check). */
export function introWanted() {
  if (S.boot?.selftest || S.settings?.animations === false) return false;
  try {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return false;
  } catch {}
  return true;
}

let started = false;

/** Play the opening over the page (once per launch). `asked` is `OMSI_LAUNCHER_INTRO`, when
 * known (`hub_boot`); `when` gives it later. */
export function startIntro() {
  if (started) return null;
  started = true;
  if (!introWanted()) return null;
  const root = document.createElement('div');
  root.className = 'hub-intro';
  root.setAttribute('aria-hidden', 'true');
  const light = document.createElement('div');
  light.className = 'hub-intro-light';
  root.appendChild(light);
  const [vx, vy, vw, vh] = MARK.view;
  const svg = el('svg', { viewBox: `${vx} ${vy} ${vw} ${vh}`, class: 'hub-intro-mark' });
  root.appendChild(svg);
  const defs = el('defs', {}, svg);
  // ("Omsi-" rises from behind the line: cut off above its top)
  // (the light at the pen: a pale point of the line's colour in a glow of it)
  const glow = el('radialGradient', { id: 'hub-intro-glow' }, defs);
  el('stop', { offset: '0', class: 'hub-i-glow-in' }, glow);
  el('stop', { offset: '1', class: 'hub-i-glow-out' }, glow);
  const cut = el('clipPath', { id: 'hub-intro-cut' }, defs);
  el('rect', { x: vx, y: vy - 200, width: vw, height: MARK.foot - MARK.stroke / 2 - (vy - 200) }, cut);
  const route = el('path', { d: MARK.route, fill: 'none', 'stroke-width': MARK.stroke, 'stroke-linecap': 'round', class: 'hub-i-line' }, svg);
  const glowPen = el('circle', { r: MARK.stroke * 1.75, class: 'hub-i-pen-glow', fill: 'url(#hub-intro-glow)' }, svg);
  const pen = el('circle', { r: MARK.stroke * 0.275, class: 'hub-i-pen' }, svg);
  const bus = el('path', { d: MARK.bus, class: 'hub-i-ink' }, svg);
  const light1 = el('g', { 'clip-path': 'url(#hub-intro-cut)' }, svg);
  const heavy = el('g', {}, svg);
  const letters = [];
  MARK.words.forEach((w, n) => {
    for (const [, , d] of w) letters.push([n, el('path', n === 0 ? { d, class: 'hub-i-ink' } : { d, fill: '#F58620', stroke: '#F58620', 'stroke-width': 7, 'stroke-linejoin': 'round' }, n === 0 ? light1 : heavy)]);
  });
  const stops = MARK.stops.map(([x, was, inner]) => {
    const g = el('g', {}, svg);
    g.innerHTML = x === was ? inner : `<g transform="translate(${(x - was).toFixed(1)} 0)">${inner}</g>`;
    return [x, g];
  });
  const [tx] = MARK.stops[MARK.stops.length - 1];
  const ring = el('circle', { cx: tx, cy: MARK.foot, fill: 'none', class: 'hub-i-pulse' }, svg);
  document.body.appendChild(root);
  const timing = timingOf(route);
  route.setAttribute('stroke-dasharray', `${timing.len} ${timing.len}`);

  let clock = 0;
  let last = null;
  let leave = null;
  let freeze = null;
  let done = false;
  const place = () => {
    // as large as the game-drawn launcher drew it in this window (a phone's small one still
    // reads), never wider than four fifths of it
    const W = innerWidth;
    const H = innerHeight;
    const s = Math.min(W / 1440, H / 900);
    const small = Math.min((W * 0.42) / (vw * UNIT), (H * 0.3) / (vh * UNIT), 1);
    const k = Math.min(Math.max(s, small), (W * 0.8) / (vw * UNIT)) * UNIT;
    svg.style.width = `${vw * k}px`;
    svg.style.height = `${vh * k}px`;
  };
  place();
  addEventListener('resize', place);
  const end = () => {
    if (done) return;
    done = true;
    root.remove();
    removeEventListener('resize', place);
    removeEventListener('keydown', skip, true);
  };
  const skip = (e) => {
    if (done) return;
    if (freeze != null) {
      clock = freeze;
      freeze = null;
    }
    const t = clock;
    if (!leave) {
      const [mark, ground, scale] = fade(t, null);
      if (ground < PASS_BELOW) return;
      leave = { at: t, mark, ground, scale };
      e?.preventDefault?.();
      e?.stopPropagation?.();
    }
  };
  root.addEventListener('pointerdown', skip);
  addEventListener('keydown', skip, true);
  const frame = (now) => {
    if (done) return;
    if (S.settings?.animations === false) return end();
    if (freeze == null) {
      clock += last == null ? 0 : Math.min(MAX_STEP, (now - last) / 1000);
    }
    last = now;
    const t = freeze ?? clock;
    const L = look(t, leave, timing);
    if (L.ground <= 0) return end();
    // the ground (opacities blended as they look: a dark ground at four fifths showed the page half)
    const cover = 1 - (1 - L.ground) ** 2.2;
    root.style.opacity = cover;
    root.style.pointerEvents = L.ground < PASS_BELOW ? 'none' : 'auto';
    light.style.opacity = L.glow;
    const a = L.mark ** 2.2;
    svg.style.opacity = a;
    svg.style.transform = `scale(${L.scale})`;
    // the ring and the line, drawn up to the pen; the pen pressed in as it sets down
    route.setAttribute('stroke-dashoffset', `${timing.len * (1 - L.drawn)}`);
    route.setAttribute('stroke-width', MARK.stroke * (0.35 + 0.65 * L.pen));
    route.style.visibility = L.drawn > 0 ? 'visible' : 'hidden';
    // the light at the pen
    const at = route.getPointAtLength(timing.len * L.drawn);
    for (const c of [glowPen, pen]) {
      c.setAttribute('cx', at.x);
      c.setAttribute('cy', at.y);
    }
    glowPen.style.opacity = 0.42 * L.head * (L.drawn > 0 ? 1 : 0);
    pen.style.opacity = 0.9 * L.head * (L.drawn > 0 ? 1 : 0);
    // the bus front, springing up once the ring has closed round it
    const [bx, by] = MARK.busAt;
    bus.setAttribute('transform', `translate(${bx} ${by}) scale(${L.bus}) translate(${-bx} ${-by})`);
    // the stops as the pen reaches them; the ring going out from the terminus
    stops.forEach(([x, g], k) => g.setAttribute('transform', `translate(${x} ${MARK.foot}) scale(${L.stops[k]}) translate(${-x} ${-MARK.foot})`));
    if (L.pulse != null) {
      const p = L.pulse;
      ring.setAttribute('r', 62 * 1.08 + 80 * outCubic(p));
      ring.setAttribute('stroke-width', 6.5 - 3.2 * p);
      ring.style.opacity = 0.55 * (1 - p) * (1 - p);
    } else ring.style.opacity = 0;
    // the name: "Omsi-" rising into its place, "Hub" landing down its slant
    const dir = [SLANT / Math.hypot(SLANT, 1), -1 / Math.hypot(SLANT, 1)];
    letters.forEach(([n, path], k) => {
      const [alpha, rest] = L.letters[k];
      path.style.opacity = alpha ** 2.2;
      if (n === 0) path.setAttribute('transform', `translate(0 ${rest * LIGHT_CAP * 0.5})`);
      else path.setAttribute('transform', `translate(${dir[0] * rest * SLIDE * HEAVY_CAP} ${dir[1] * rest * SLIDE * HEAVY_CAP})`);
    });
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  return {
    /** `OMSI_LAUNCHER_INTRO` as `hub_boot` gives it: off, or a moment to hold. */
    asked(v) {
      const a = String(v ?? '').trim().toLowerCase();
      if (a === 'off') end();
      else if (a && Number.isFinite(Number(a))) freeze = Math.max(0, Number(a));
    },
    end,
  };
}

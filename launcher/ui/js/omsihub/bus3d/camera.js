// The camera of the 3D view (Omsi-Hub's Bus3D, the old app's `bus3d/camera.ts`).
//
// Constants after openOMSI's showroom: the view begins at the front corner of the door side,
// turned 215°, 8° up, a 30° lens (vertical); it fits the bus's box (the distance is the
// largest of (length/2 + 1) / (tan 15° · aspect · 0.92), (height/2) / (tan 15° · 0.85) and 8 m,
// times the zoom), aimed at the box's middle at 0.75 of its height, never lower than 0.6 m;
// dragging turns 0.35°/px and tilts 0.25°/px (-2° to 60°), the wheel and a pinch zoom 8 % a
// step between 0.55 and 2.5, and everything eases out with τ = 0.18 s.
//
// The turn is measured from the rear, clockwise seen from above: 0° rear, 90° left, 180°
// front, 270° right (the door side). In the world (right-handed, X = -x of the o3d) the camera
// stands at target + R · (cos tilt · sin turn, sin tilt, -cos tilt · cos turn).
//
// New here: the inside views (the old viewer had none): the driver's and a passenger's seat
// from the `.bus`'s `[add_camera_driver]` / `[add_camera_pax]`, looked round by dragging, the
// wheel narrowing or widening the lens.

import { lookAt, mul, ortho, perspective, project, rad } from './math.js';

export const BEGIN = { turn: 215, tilt: 8, zoom: 1 };
const STANDS = {
  front: { turn: 180, tilt: 6 },
  side: { turn: 270, tilt: 5 },
  rear: { turn: 0, tilt: 6 },
  corner: { turn: 215, tilt: 8 },
};
const LENS = rad(30);
const TILT_MIN = -2;
const TILT_MAX = 60;
const ZOOM_MIN = 0.55;
const ZOOM_MAX = 2.5;
const TAU = 0.18;

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The flat views of the livery studio (keys 1-5): orthographic. */
export const FLATS = ['left', 'right', 'front-face', 'rear-face', 'roof'];

export class Camera {
  constructor() {
    this.to = { ...BEGIN };
    this.now = { ...BEGIN };
    /** A flat view (the studio), or none: perspective. */
    this.flat = undefined;
    /** The aim's shift (the studio: the middle button or Shift), in the world. */
    this.shift = [0, 0, 0];
    this.shiftTo = [0, 0, 0];
    /** The smallest zoom: 0.55 in the viewer, 0.1 in the studio. */
    this.zoomMin = ZOOM_MIN;
    this.box = { min: [-1.25, 0, -6], max: [1.25, 3, 6] };
    /** An inside view: the seat's camera (o3d frame), how it is turned now and its lens. */
    this.inside = undefined;
  }

  setBox(min, max) {
    this.box = { min, max };
  }

  /** Back to the beginning, without easing (another bus). */
  restart() {
    this.to = { ...BEGIN };
    this.now = { ...BEGIN };
    this.shift = [0, 0, 0];
    this.shiftTo = [0, 0, 0];
  }

  /** An inside view from a seat's camera (`cam` of the manifest), or back outside (none). */
  setInside(cam) {
    if (!cam) {
      this.inside = undefined;
      return;
    }
    const fov = clamp(cam.fov > 5 ? cam.fov : 60, 20, 100);
    this.inside = { cam, yaw: cam.yaw, pitch: cam.pitch, fov, yawTo: cam.yaw, pitchTo: cam.pitch, fovTo: fov };
  }

  drag(dx, dy) {
    if (this.inside) {
      this.inside.yawTo -= dx * 0.25;
      this.inside.pitchTo = clamp(this.inside.pitchTo - dy * 0.2, -80, 80);
      return;
    }
    // (turning a flat view goes to the corner)
    if (this.flat) {
      this.flat = undefined;
      this.to = { ...STANDS.corner, zoom: 1 };
      this.now = { ...this.to };
    }
    this.to.turn -= dx * 0.35;
    this.to.tilt = clamp(this.to.tilt + dy * 0.25, TILT_MIN, TILT_MAX);
  }

  turn(deg) {
    if (this.inside) this.inside.yawTo -= deg;
    else this.to.turn += deg;
  }

  tilt(deg) {
    if (this.inside) this.inside.pitchTo = clamp(this.inside.pitchTo + deg, -80, 80);
    else this.to.tilt = clamp(this.to.tilt + deg, TILT_MIN, TILT_MAX);
  }

  zoomStep(factor) {
    if (this.inside) {
      this.inside.fovTo = clamp(this.inside.fovTo * factor, 20, 100);
      return;
    }
    this.to.zoom = clamp(this.to.zoom * factor, this.zoomMin, ZOOM_MAX);
  }

  /** Metres a pixel in the picture plane through the aim, and the right and up axes (world). */
  plane(height, aspect) {
    const c = this.view(aspect);
    const right = [c.view[0], c.view[4], c.view[8]];
    const up = [c.view[1], c.view[5], c.view[9]];
    const perPixel = this.flat ? 2 / (c.proj[5] * Math.max(1, height)) : (2 * this.fitDistance(aspect) * this.now.zoom * Math.tan(LENS / 2)) / Math.max(1, height);
    return { perPixel, right, up };
  }

  /** Move in the picture plane: dx/dy in pixels of a picture `height` pixels high. */
  pan(dx, dy, height = 800, aspect = 1.6) {
    const { perPixel, right, up } = this.plane(height, aspect);
    for (let a = 0; a < 3; a++) {
      const d = (-right[a] * dx + up[a] * dy) * perPixel;
      this.shift[a] += d;
      this.shiftTo[a] += d;
    }
  }

  /** Zoom towards the cursor (the studio): the point under it (x, y in NDC) stays put. */
  zoomAt(factor, x, y, aspect) {
    const old = this.to.zoom;
    this.zoomStep(factor);
    const f = this.to.zoom / old;
    if (f === 1) return;
    const height = 1000;
    const { perPixel, right, up } = this.plane(height, aspect);
    const px = (x * height * aspect) / 2;
    const py = (y * height) / 2;
    for (let a = 0; a < 3; a++) this.shiftTo[a] += (right[a] * px + up[a] * py) * perPixel * (1 - f);
  }

  /** Aim at a point of the bus (o3d frame; the world is mirrored in x). */
  centre(point) {
    const w = [-point[0], point[1], point[2]];
    const d = this.box;
    const mid = this.flat ? [(d.min[0] + d.max[0]) / 2, (d.min[1] + d.max[1]) / 2, (d.min[2] + d.max[2]) / 2] : this.aim();
    this.shiftTo = [w[0] - mid[0], w[1] - mid[1], w[2] - mid[2]];
  }

  /** Fit (F): zoom 1 and no shift; the view stays. */
  fit() {
    this.to = { ...this.to, zoom: 1 };
    this.shiftTo = [0, 0, 0];
  }

  stand(name) {
    this.inside = undefined;
    if (FLATS.includes(name)) {
      this.flat = name;
      this.to = { ...this.to, zoom: 1 };
      this.shift = [0, 0, 0];
      this.shiftTo = [0, 0, 0];
      return;
    }
    this.flat = undefined;
    if (name === 'back') {
      this.to = { ...BEGIN };
      this.shiftTo = [0, 0, 0];
      return;
    }
    const s = STANDS[name];
    if (!s) return;
    // (the shortest way round: not 330° back for a step of 30°)
    let turn = s.turn;
    while (turn - this.to.turn > 180) turn -= 360;
    while (turn - this.to.turn < -180) turn += 360;
    this.to = { turn, tilt: s.tilt, zoom: 1 };
  }

  /** One step of easing; true while the camera still moves. */
  step(dt) {
    const k = 1 - Math.exp(-Math.max(0, dt) / TAU);
    let moving = false;
    for (const f of ['turn', 'tilt', 'zoom']) {
      const d = this.to[f] - this.now[f];
      if (Math.abs(d) <= (f === 'zoom' ? 1e-4 : 0.01)) this.now[f] = this.to[f];
      else {
        this.now[f] += d * k;
        moving = true;
      }
    }
    for (let a = 0; a < 3; a++) {
      const d = this.shiftTo[a] - this.shift[a];
      if (Math.abs(d) <= 1e-4) this.shift[a] = this.shiftTo[a];
      else {
        this.shift[a] += d * k;
        moving = true;
      }
    }
    const i = this.inside;
    if (i) {
      for (const [f, t] of [['yaw', 'yawTo'], ['pitch', 'pitchTo'], ['fov', 'fovTo']]) {
        const d = i[t] - i[f];
        if (Math.abs(d) <= 0.01) i[f] = i[t];
        else {
          i[f] += d * k;
          moving = true;
        }
      }
    }
    return moving;
  }

  /** At the goal at once (pictures, measuring). */
  jump() {
    this.now = { ...this.to };
    this.shift = [...this.shiftTo];
    if (this.inside) Object.assign(this.inside, { yaw: this.inside.yawTo, pitch: this.inside.pitchTo, fov: this.inside.fovTo });
  }

  /** What the camera aims at: the box's middle at 0.75 of its height. */
  aim() {
    const d = this.box;
    return [(d.min[0] + d.max[0]) / 2, ((d.min[1] + d.max[1]) / 2) * 0.75, (d.min[2] + d.max[2]) / 2];
  }

  /** The distance at zoom 1 for this aspect. */
  fitDistance(aspect) {
    const d = this.box;
    const length = d.max[2] - d.min[2];
    const height = d.max[1] - d.min[1];
    const t = Math.tan(LENS / 2);
    return Math.max((length / 2 + 1) / (t * aspect * 0.92), height / 2 / (t * 0.85), 8);
  }

  /** A flat view: orthographic from the side, front, rear or above, fitting the box, with the
   * zoom and the shift. `flat` may come from outside (the studio's other side). */
  flatView(aspect, flat, zoom = this.now.zoom) {
    const d = this.box;
    const mid = [(d.min[0] + d.max[0]) / 2 + this.shift[0], (d.min[1] + d.max[1]) / 2 + this.shift[1], (d.min[2] + d.max[2]) / 2 + this.shift[2]];
    const size = [d.max[0] - d.min[0], d.max[1] - d.min[1], d.max[2] - d.min[2]];
    const r = Math.max(size[0], size[1], size[2]) + 5;
    // (in the world x is mirrored: the right side, the door side, o3d +x, lies at -X)
    const ways = {
      right: { eye: [-1, 0, 0], up: [0, 1, 0], b: size[2], h: size[1] },
      left: { eye: [1, 0, 0], up: [0, 1, 0], b: size[2], h: size[1] },
      'front-face': { eye: [0, 0, 1], up: [0, 1, 0], b: size[0], h: size[1] },
      'rear-face': { eye: [0, 0, -1], up: [0, 1, 0], b: size[0], h: size[1] },
      roof: { eye: [0, 1, 0], up: [0, 0, 1], b: size[0], h: size[2] },
    };
    const k = ways[flat];
    const eye = [mid[0] + k.eye[0] * r, mid[1] + k.eye[1] * r, mid[2] + k.eye[2] * r];
    const view = lookAt(eye, mid, k.up);
    let hb = (k.b / 2) * 1.08;
    let hh = (k.h / 2) * 1.08;
    if (hb / hh < aspect) hb = hh * aspect;
    else hh = hb / aspect;
    hb *= zoom;
    hh *= zoom;
    const proj = ortho(-hb, hb, -hh, hh, 0.1, 2 * r + 20);
    return { view, proj, viewProj: mul(proj, view), eye, target: mid, near: 0.1, far: 2 * r + 20 };
  }

  /** The inside view from its seat. */
  insideView(aspect) {
    const i = this.inside;
    const c = i.cam;
    const sy = Math.sin(rad(i.yaw));
    const cy = Math.cos(rad(i.yaw));
    const sp = Math.sin(rad(i.pitch));
    const cp = Math.cos(rad(i.pitch));
    // forward in the o3d frame (x right, y up, z forward), then the world (x mirrored)
    const f = [-sy * cp, sp, cy * cp];
    const dist = Math.max(0, c.dist || 0);
    const eye = [-c.pos[0] - f[0] * dist, c.pos[1] - f[1] * dist, c.pos[2] - f[2] * dist];
    const target = [eye[0] + f[0], eye[1] + f[1], eye[2] + f[2]];
    const view = lookAt(eye, target);
    const proj = perspective(rad(i.fov), aspect, 0.04, 800);
    return { view, proj, viewProj: mul(proj, view), eye, target, near: 0.04, far: 800 };
  }

  /** The matrices of a picture; `other` for a picture with a stand of its own, or an aim and
   * a distance of its own. */
  view(aspect, other) {
    if (this.inside && !other) return this.insideView(aspect);
    if (this.flat && !other) return this.flatView(aspect, this.flat);
    const s = other?.stand ?? this.now;
    const target = [...(other?.target ?? this.aim())];
    if (!other) for (let a = 0; a < 3; a++) target[a] += this.shift[a];
    const r = other?.distance ?? this.fitDistance(aspect) * s.zoom;
    const d = rad(s.turn);
    const k = rad(s.tilt);
    const eye = [target[0] + r * Math.cos(k) * Math.sin(d), target[1] + r * Math.sin(k), target[2] - r * Math.cos(k) * Math.cos(d)];
    // (never lower than 0.6 m: the camera goes up and stays aimed)
    if (eye[1] < 0.6) eye[1] = 0.6;
    const size = Math.max(this.box.max[2] - this.box.min[2], this.box.max[1] - this.box.min[1], 4);
    const near = Math.max(0.05, Math.min(r - size, r * 0.2));
    const far = Math.max(1200, r * 4);
    const proj = perspective(LENS, aspect, near, far);
    let view = lookAt(eye, target);
    if (!other?.target) {
      // Sideways in the middle: through the perspective the near end stands larger than the far
      // one and the box's middle is not the picture's (an articulated bus hung out at one
      // side). The box projected twice and the camera moved aside so that both ends lie as far
      // from the edges.
      const right = [view[0], view[4], view[8]];
      const width = r * Math.tan(LENS / 2) * aspect;
      for (let i = 0; i < 2; i++) {
        const vp = mul(proj, view);
        let x0 = Infinity;
        let x1 = -Infinity;
        for (const x of [this.box.min[0], this.box.max[0]])
          for (const y of [this.box.min[1], this.box.max[1]])
            for (const z of [this.box.min[2], this.box.max[2]]) {
              const p = project(vp, [x, y, z]);
              if (p[3] <= 0) continue;
              x0 = Math.min(x0, p[0]);
              x1 = Math.max(x1, p[0]);
            }
        if (!Number.isFinite(x0)) break;
        const sh = ((x0 + x1) / 2) * width;
        for (let a = 0; a < 3; a++) {
          eye[a] += right[a] * sh;
          target[a] += right[a] * sh;
        }
        view = lookAt(eye, target);
      }
    }
    return { view, proj, viewProj: mul(proj, view), eye, target, near, far };
  }

  /** The photo's camera (the old app's photo v4): the beginning's stand (215°/8°), the
   * distance such that the box takes `share` of the width - or less where it would be too
   * high (at most 90 % of the height); four steps, the perspective allowing no less. */
  photoView(aspect, share = 0.88) {
    const stand = { ...BEGIN };
    let r = this.fitDistance(aspect);
    let c = this.view(aspect, { stand, distance: r });
    for (let i = 0; i < 4; i++) {
      const m = this.inPicture(c);
      if (!m) break;
      const f = Math.max(m.width / (2 * share), m.height / (2 * 0.9));
      if (!(f > 0) || Math.abs(f - 1) < 0.002) break;
      r *= f;
      c = this.view(aspect, { stand, distance: r });
    }
    // (in the middle in height too, a little above it: the aim lies at 75 % of the height, and
    // the bus sank to the bottom edge, where the contact shadow fell off)
    const m = this.inPicture(c);
    if (m) {
      const sh = (m.mid - 0.04) * r * Math.tan(LENS / 2);
      const up = [c.view[1], c.view[5], c.view[9]];
      const eye = [c.eye[0] + up[0] * sh, c.eye[1] + up[1] * sh, c.eye[2] + up[2] * sh];
      const target = [c.target[0] + up[0] * sh, c.target[1] + up[1] * sh, c.target[2] + up[2] * sh];
      const view = lookAt(eye, target);
      c = { ...c, view, eye, target, viewProj: mul(c.proj, view) };
    }
    return c;
  }

  /** How wide and high the box stands in a picture, and its middle in height (NDC). */
  inPicture(c) {
    let x0 = Infinity;
    let x1 = -Infinity;
    let y0 = Infinity;
    let y1 = -Infinity;
    for (const x of [this.box.min[0], this.box.max[0]])
      for (const y of [this.box.min[1], this.box.max[1]])
        for (const z of [this.box.min[2], this.box.max[2]]) {
          const p = project(c.viewProj, [x, y, z]);
          if (p[3] <= 0) return undefined;
          x0 = Math.min(x0, p[0]);
          x1 = Math.max(x1, p[0]);
          y0 = Math.min(y0, p[1]);
          y1 = Math.max(y1, p[1]);
        }
    return { width: x1 - x0, height: y1 - y0, mid: (y0 + y1) / 2 };
  }
}

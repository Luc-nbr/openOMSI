// The livery studio on the page (Omsi-Hub's Lakstudio, ported from the old app): the bus in
// the page's own 3D view, painted on its GPU as the layers change (`canvas.js`), the layers on
// the left, what the chosen one offers on the right, the name and Save along the top - after the
// game program's studio (`omsihub-game`'s `livery/studio.rs`), whose projects it opens and
// whose files the save writes (`lak_place`: `omsihub-livery`'s export).
//
// There are no tools to switch between: "Create layer" makes a layer at once on the side looked
// at, chosen and ready to change; only a colour area, an own shape and the brush wait for the
// mouse on the bus, with a line over it that says so. A click on the bus chooses what lies there
// and a drag moves it; the right button turns the bus, the middle one (or Shift) moves it, the
// wheel zooms towards the mouse; 1-6 the views, F fits, O held shows the livery before, E the
// eraser, [ and ] the brush's size, Ctrl+Z and Ctrl+Y undo and redo, Delete takes the layer away.

import { html, useEffect, useRef, useState } from '../../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../../store.js';
import { call } from '../../../api.js';
import { choose } from '../../../data.js';
import { Btn, Field, Icon, IconBtn, status, tr } from '../../editors/kit.js';
import { Bus3D } from '../viewer.js';
import { Link } from '../link.js';
import { forgetPhotos } from '../../photos.js';
import { inverse } from '../math.js';
import * as M from './model.js';
import { Panel } from './panels.js';

// (the studio's look, loaded with it)
if (!document.querySelector('link[data-lak-css]')) {
  const l = document.createElement('link');
  l.rel = 'stylesheet';
  l.href = 'omsihub/lak.css';
  l.dataset.lakCss = '1';
  document.head.appendChild(l);
}

/** The kinds of layer "Create layer" offers: what it makes, its name, what it is, its icon. */
export const KINDS = [
  ['base', 'Base colour', 'One colour over all the paint of the bus.', 'livery_fill'],
  ['area', 'Colour area', 'One colour of the bus changed: click it on the bus.', 'livery_eyedrop'],
  ['stripe', 'Stripe', 'A band round the bus; its kind and edges change afterwards.', 'livery_stripe'],
  ['text', 'Text', 'Letters on the side you look at.', 'livery_text'],
  ['image', 'Image', 'A logo or a photo from your computer.', 'livery_image'],
  ['shape', 'Shape', 'A rectangle, an arrow, a star and more.', 'livery_shape'],
  ['own', 'Own shape', 'Its corners clicked on the bus.', 'livery_pen'],
  ['brush', 'Brush', 'Painted freely on the bus with the mouse.', 'livery_brush'],
];

const MAKING_LINE = {
  area: 'Click the colour on the bus that is to change.',
  own: 'Click the corners of the shape on the bus; drag to bend a line. The first corner again or Enter finishes it.',
  erase: 'Erase on the bus with the left button. E: back to the brush.',
  brush: 'Paint on the bus with the left button. E: the eraser, [ and ]: its size.',
};

const VIEWS = [
  ['left', '1', 'Left'],
  ['right', '2', 'Right'],
  ['front-face', '3', 'Front'],
  ['rear-face', '4', 'Rear'],
  ['roof', '5', 'Roof'],
  ['corner', '6', 'At an angle'],
];

/** What the self-check and the report read of the studio. */
const stats = (window.hubLakStats = { ready: null, painted: 0, lastPaint: null, error: null });

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const addv = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mulv = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const unit = (a) => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/** The part of a project that undo and redo step through (`Doc`). */
const docOf = (p) => JSON.stringify({ lagen: p.lagen || [], spiegel: p.spiegel || { aan: true }, snel: p.snel ?? null });

/** Where an o3d point lies in the picture (CSS pixels of a `w` x `h` view), with its depth. */
function toScreen(cam, p, w, h) {
  const X = -p[0];
  const Y = p[1];
  const Z = p[2];
  const cx = cam[0] * X + cam[4] * Y + cam[8] * Z + cam[12];
  const cy = cam[1] * X + cam[5] * Y + cam[9] * Z + cam[13];
  const cw = cam[3] * X + cam[7] * Y + cam[11] * Z + cam[15];
  if (cw <= 1e-6) return undefined;
  return [((cx / cw + 1) / 2) * w, ((1 - cy / cw) / 2) * h];
}

/** The mouse's ray in the o3d frame: from where, which way. */
function rayAt(cam, ndc) {
  const inv = inverse(cam);
  const at = (z) => {
    const x = inv[0] * ndc[0] + inv[4] * ndc[1] + inv[8] * z + inv[12];
    const y = inv[1] * ndc[0] + inv[5] * ndc[1] + inv[9] * z + inv[13];
    const zz = inv[2] * ndc[0] + inv[6] * ndc[1] + inv[10] * z + inv[14];
    const w = inv[3] * ndc[0] + inv[7] * ndc[1] + inv[11] * z + inv[15];
    return [-x / w, y / w, zz / w];
  };
  const a = at(-1);
  const b = at(1);
  return { o: a, d: unit(sub(b, a)) };
}

/** Where a ray meets the plane through `at` with normal `n`. */
function onPlane(ray, at, n) {
  const dn = dot(ray.d, n);
  if (Math.abs(dn) < 1e-6) return undefined;
  const t = dot(sub(at, ray.o), n) / dn;
  return t > 0 ? addv(ray.o, mulv(ray.d, t)) : undefined;
}

/** The studio's page. */
export function LakStudio() {
  const s = useStore();
  const o = s.ed.liveryOpen || {};
  const st = useRef(null);
  const [, redraw] = useState(0);
  const tick = () => redraw((n) => n + 1);
  if (!st.current || st.current.at !== o.at) st.current = fresh(o);
  const me = st.current;
  me.tick = tick;
  // (the self-check works the studio through it; not part of the stats' JSON)
  Object.defineProperty(stats, 'studio', { value: { me, create: (k) => create(me, k), change: (fn) => change(me, fn), select: (id) => select(me, id), save: () => save(me) }, configurable: true, writable: true, enumerable: false });

  // open: the project, then the bus prepared for the paint
  useEffect(() => {
    let alive = true;
    me.alive = () => alive;
    open(me).catch((x) => alive && ((me.failed = x.message || String(x)), tick()));
    const link = Link.get();
    link.onStudio = (m) => alive && onRenderer(me, m);
    const keyDown = (e) => alive && key(me, e, true);
    const keyUp = (e) => alive && key(me, e, false);
    addEventListener('keydown', keyDown);
    addEventListener('keyup', keyUp);
    const poll = setInterval(() => alive && pollSave(me), 400);
    return () => {
      alive = false;
      clearInterval(poll);
      removeEventListener('keydown', keyDown);
      removeEventListener('keyup', keyUp);
      me.unwatch?.();
      me.handle?.send({ kind: 'lakStop' });
      keepNow(me, true);
    };
  }, [o.at]);

  if (me.choosing) return html`<${Chooser} me=${me} />`;
  const p = me.project;
  const viewerProps = {
    bus: me.bus,
    paint: p?.startKleurstelling || '',
    options: p ? Object.entries(p.opties || {}) : undefined,
    views: false,
    hint: false,
    class: 'lak-bus',
    studio: controls(me),
    onHandle: (h) => attachViewer(me, h),
  };
  const sel = p?.lagen?.find((l) => l.id === me.selected);
  return html`<div class="lak-studio">
    <${Top} me=${me} />
    <div class="lak-main">
      <${Layers} me=${me} />
      <div class="lak-view" ref=${(el) => (me.viewEl = el)}>
        ${me.bus && p && html`<${Bus3D} ...${viewerProps} />`}
        <${Overlay} me=${me} />
        ${me.making !== 'none' && html`<div class="lak-making">
          <span>${tr(MAKING_LINE[me.making === 'brush' && me.brush.gum ? 'erase' : me.making] || '')}</span>
          ${me.making === 'own' && me.pen.nodes.length >= 3 && html`<${Btn} kind="primary" label="Done" onClick=${() => finishPen(me)} />`}
          <${Btn} label=${me.making === 'brush' ? 'Done' : 'Cancel'} onClick=${() => endMaking(me, false)} />
        </div>`}
        <div class="lak-views" onPointerDown=${(e) => e.stopPropagation()}>
          ${VIEWS.map(([v, k, label]) => html`<button type="button" title=${`${tr(label)} (${k})`} onClick=${() => view(me, v)}>${k}</button>`)}
          <button type="button" title=${`${tr('Fit')} (F)`} onClick=${() => me.handle?.input({ kind: 'fit' })}><${Icon} name="fit_screen" size=${16} /></button>
          <button type="button" class=${me.beforeOn ? 'on' : ''} title=${`${tr('Before and after')} (O)`} onPointerDown=${() => before(me, true)} onPointerUp=${() => before(me, false)} onPointerLeave=${() => me.beforeOn && before(me, false)}><${Icon} name="compare" size=${16} /></button>
        </div>
        ${(me.phase !== 'ready' || me.failed) && html`<div class="lak-note">
          ${me.failed ? html`<p class="error">${me.failed}</p><${Btn} label="Back" onClick=${() => leave(me)} />` : html`<p>${tr(me.phase === 'loading' ? 'Loading the bus…' : 'Preparing the paint…')}</p>`}
        </div>`}
        ${me.saving && html`<div class="lak-progress"><span>${me.saving.text}</span><i style=${`width:${Math.round((me.saving.f || 0) * 100)}%`}></i></div>`}
      </div>
      <div class="lak-right">
        <${Panel} me=${me} layer=${sel} />
      </div>
    </div>
    ${me.say && html`<div class=${'lak-say' + (me.say.err ? ' error' : '')} onClick=${() => ((me.say = null), tick())}>${me.say.text}</div>`}
  </div>`;
}

/** The studio's own state for one opening. */
function fresh(o) {
  return {
    at: o.at,
    open: o,
    bus: '',
    project: null,
    phase: 'loading',
    failed: '',
    selected: null,
    making: 'none',
    pen: { side: null, origin: null, nodes: [] },
    brush: { straalCm: 10, hardheid: 0.5, dekking: 1, gum: false },
    colour: '#d93a30',
    recent: [],
    history: { undo: [], redo: [] },
    pending: null,
    sizes: {},
    sizesKey: '',
    prep: null,
    cam: null,
    handle: null,
    drag: null,
    kinds: false,
    hover: null,
    say: null,
    saving: null,
    beforeOn: false,
    name: '',
    family: [],
    cost: null,
  };
}

function say(me, text, err = false) {
  me.say = { text: tr(text), err };
  me.tick();
  clearTimeout(me.sayClock);
  if (!err) me.sayClock = setTimeout(() => ((me.say = null), me.tick()), 6000);
}

// ------------------------------------------------------------ opening
async function open(me) {
  const o = me.open;
  call('lak_fonts')
    .then((f) => ((me.fonts = f), me.tick()))
    .catch(() => {});
  let bus = o.bus === 'chosen' ? S.choice?.bus || '' : o.bus || '';
  const paint = o.bus === 'chosen' ? S.choice?.paint || '' : o.paint || '';
  if (o.project) {
    const r = await call('lak_project', { id: o.project });
    me.project = r.project;
    bus = r.project.bus;
    if (r.restored) say(me, 'Your last changes were brought back from the temporary file');
  } else if (!bus) {
    me.choosing = true;
    me.projects = await call('lak_projects').catch(() => []);
    me.tick();
    return;
  } else {
    me.project = (await call('lak_new', { bus, paint, start: o.start || 'snel' })).project;
  }
  me.bus = bus;
  me.name = me.project.naam || '';
  me.company = o.company || '';
  me.vehicle = o.vehicle ?? null;
  me.tick();
  await prepare(me);
}

/** The bus ready to paint (`lak_prepare`): its textures, zones, the slots' parts in the paint. */
async function prepare(me) {
  const p = me.project;
  me.phase = 'preparing';
  me.tick();
  const prep = await call('lak_prepare', { bus: p.bus, paint: p.startKleurstelling || '', start: p.start, options: Object.entries(p.opties || {}), buses: (S.vehicles || []).map((v) => [v.file, v.name]) });
  if (!me.alive?.()) return;
  me.prep = prep;
  // a plain start in a livery's colours: a layer for each paint group, once
  if (!(p.lagen || []).length && p.start === 'effenKleuren' && prep.startColours?.length) {
    p.lagen = prep.startColours.map(([g, c]) => M.layer(g === 0 ? 'Body colour' : `${tr('Colour')} ${g + 1}`, M.groupColour(g, c)));
    touched(me);
  }
  me.phase = 'ready';
  me.tick();
  await refreshSizes(me);
  start(me);
  cost(me);
}

/** The painting engine on the bus once both the viewer and the bus are there. */
function start(me) {
  if (!me.handle || !me.prep || me.started) return;
  me.started = true;
  Link.get()
    .loadStudio()
    .then(() => me.handle?.send({ kind: 'lakStart', prep: me.prep, ...layersMessage(me) }));
}

function attachViewer(me, h) {
  me.handle = h;
  me.unwatch?.();
  me.unwatch = Link.get().watchCamera(h.id, (c) => {
    me.cam = c;
    me.overlayTick?.();
  });
  start(me);
}

function onRenderer(me, m) {
  if (m.kind === 'lakReady') {
    stats.ready = { ms: m.ms, targets: m.targets };
    me.engineReady = true;
    me.tick();
  } else if (m.kind === 'lakPainted') {
    stats.painted++;
    stats.lastPaint = m.ms;
  } else if (m.kind === 'lakError') {
    stats.error = m.error;
    me.failed = tr('The bus cannot be painted: %{e}', { e: m.error });
    me.tick();
  } else if (m.kind === 'lakNote') say(me, m.text, true);
}

// ------------------------------------------------------------ the document
function layersMessage(me, live) {
  const p = me.project;
  return { layers: p?.lagen || [], planeX: me.prep ? M.mirrorPlane(p, me.prep.dims) ?? null : null, sizes: me.sizes, project: p?.id, live };
}

/** The layers to the painter (and their sizes asked again when a decal's content changed). */
function sendLayers(me, live) {
  me.handle?.send({ kind: 'lakLayers', ...layersMessage(me, live) });
  const key = JSON.stringify((me.project?.lagen || []).filter(M.isDecal).map((l) => [l.id, l.soort, l.tekst, l.lettertype, l.hoogteCm, l.letterafstand, l.omlijning, l.beeld, l.verhouding, l.plaats?.breedteM, l.plaats?.hoogteM]));
  if (key !== me.sizesKey) refreshSizes(me);
}

async function refreshSizes(me) {
  const p = me.project;
  const decals = (p?.lagen || []).filter(M.isDecal);
  const key = JSON.stringify(decals.map((l) => [l.id, l.soort, l.tekst, l.lettertype, l.hoogteCm, l.letterafstand, l.omlijning, l.beeld, l.verhouding, l.plaats?.breedteM, l.plaats?.hoogteM]));
  me.sizesKey = key;
  if (!decals.length) return;
  try {
    const sizes = await call('lak_sizes', { id: p.id, layers: decals });
    if (me.sizesKey !== key) return;
    me.sizes = sizes;
    me.handle?.send({ kind: 'lakLayers', sizes });
    me.overlayTick?.();
  } catch {
    // (kept as they were)
  }
}

/** Change the project: `fn(project)`; one step to undo unless `pending` (a drag going on, which
 * `settle` closes). */
function change(me, fn, { pending = false, live } = {}) {
  const p = me.project;
  if (!p) return;
  const before = docOf(p);
  fn(p);
  if (docOf(p) === before) return;
  if (!me.pending) me.pending = before;
  if (!pending) settle(me);
  touched(me, live);
}

/** Close the change going on (the mouse let go, a field left): one undo step. */
function settle(me) {
  if (!me.pending) return;
  const now = docOf(me.project);
  if (now !== me.pending) {
    me.history.undo.push(me.pending);
    if (me.history.undo.length > 500) me.history.undo.shift();
    me.history.redo = [];
  }
  me.pending = null;
}

function setDoc(me, doc) {
  const d = JSON.parse(doc);
  me.project.lagen = d.lagen;
  me.project.spiegel = d.spiegel;
  if (d.snel) me.project.snel = d.snel;
  else delete me.project.snel;
}

function undo(me) {
  settle(me);
  const d = me.history.undo.pop();
  if (!d) return;
  me.history.redo.push(docOf(me.project));
  setDoc(me, d);
  touched(me);
}

function redo(me) {
  settle(me);
  const d = me.history.redo.pop();
  if (!d) return;
  me.history.undo.push(docOf(me.project));
  setDoc(me, d);
  touched(me);
}

/** The project changed: to the painter, into its temporary file at once (five times a second
 * at most), into `project.json` two seconds after the last change. */
function touched(me, live) {
  sendLayers(me, live);
  me.dirtyAt = Date.now();
  if (!me.keepClock) {
    me.keepClock = setTimeout(() => {
      me.keepClock = 0;
      keepNow(me, false);
    }, 200);
  }
  clearTimeout(me.finalClock);
  me.finalClock = setTimeout(() => !me.drag && keepNow(me, true), 2000);
  me.tick();
}

function keepNow(me, final) {
  const p = me.project;
  if (!p || (!me.dirtyAt && !final)) return;
  // (a project nothing was done to leaves nothing behind)
  if (final && !me.dirtyAt && !p.geplaatst) return;
  p.naam = me.name || p.naam || '';
  call('lak_keep', { project: p, final }).catch(() => {});
}

function select(me, id) {
  me.selected = id;
  me.kinds = false;
  me.tick();
}

// ------------------------------------------------------------ making layers
/** Where a new text, picture or shape goes (`spot`): on the side the view shows most of, along
 * it where the view's middle is, below the windows - on the body itself there. */
async function spot(me) {
  const d = me.prep.dims;
  const cam = me.cam;
  let towards = [1, 0, 0];
  let mid = [(d.min[0] + d.max[0]) / 2, (d.min[1] + d.max[1]) / 2, (d.min[2] + d.max[2]) / 2];
  if (cam) {
    const r = rayAt(cam, [0, 0]);
    towards = mulv(r.d, -1);
    // (the view's middle: where its ray comes closest to the bus's long axis)
    const t = -dot(sub(r.o, mid), r.d);
    mid = addv(r.o, mulv(r.d, Math.max(0, t)));
  }
  const len = M.busLength(d);
  const wide = Math.max(0.5, d.max[0] - d.min[0]);
  const high = M.busHeight(d);
  const faces = [['L', len * high], ['R', len * high], ['V', wide * high], ['A', wide * high], ['D', len * wide]];
  const side = faces.map(([sd, a]) => [sd, Math.max(0, dot(M.SIDES[sd].n, towards)) * a]).sort((a, b) => b[1] - a[1])[0][0];
  const within = (v, lo, hi, m) => (hi - lo > 2 * m ? Math.min(hi - m, Math.max(lo + m, v)) : (lo + hi) / 2);
  const up = d.min[1] + Math.max(M.windowHeight(d) * 0.6, 0.3);
  const p = {
    R: [d.max[0], up, within(mid[2], d.min[2], d.max[2], 1)],
    L: [d.min[0], up, within(mid[2], d.min[2], d.max[2], 1)],
    V: [within(mid[0], d.min[0], d.max[0], 0.4), up, d.max[2]],
    A: [within(mid[0], d.min[0], d.max[0], 0.4), up, d.min[2]],
    D: [d.middleX ?? 0, d.max[1], within(mid[2], d.min[2], d.max[2], 1)],
  }[side];
  // onto the body: the ray in from outside along the side's normal
  const out = M.SIDES[side].n;
  const r = await me.handle?.ask({ kind: 'lakPick', from: addv(p, mulv(out, 1.5)), dir: mulv(out, -1), reach: 4 }).catch(() => null);
  const on = r?.hit?.p && Math.abs(dot(sub(r.hit.p, p), out)) < 0.8 ? r.hit.p : p;
  return { side, p: on };
}

/** Add a layer: on top (a base colour under the others), chosen, one step to undo. */
function addLayer(me, l) {
  endMaking(me, true);
  me.kinds = false;
  change(me, (p) => {
    if (l.soort === 'zone' && l.lakgroep == null && l.straal >= 999) p.lagen.unshift(l);
    else p.lagen.push(l);
  });
  me.selected = l.id;
  if (M.isDecal(l)) say(me, 'Made on the side you look at: drag it on the bus to where it goes.');
  me.tick();
}

export async function create(me, kind) {
  me.kinds = false;
  endMaking(me, true);
  if (!me.prep) return say(me, 'Preparing the paint…');
  const colour = me.colour;
  if (kind === 'base') return addLayer(me, M.layer('Base colour', M.baseColour(colour)));
  if (kind === 'stripe') {
    const k = await call('lak_stripe', { template: 'onderband', colour });
    return addLayer(me, M.layer('Skirt band', k));
  }
  if (kind === 'area' || kind === 'own' || kind === 'brush') {
    me.selected = null;
    me.making = kind;
    me.brushLayer = null;
    me.pen = { side: null, origin: null, nodes: [] };
    me.tick();
    return;
  }
  const { side, p } = await spot(me);
  if (kind === 'text') {
    const text = tr('Text');
    return addLayer(me, M.layer('Text', { soort: 'tekst', tekst: text, lettertype: me.fonts?.default || 'Hanken Grotesk', hoogteCm: 25, kleur: colour, letterafstand: 0, plaats: { zijde: side, midden: p, breedteM: 0.25 * text.length * 0.55, draai: 0, spiegel: 'gekoppeld' } }));
  }
  if (kind === 'shape') return addLayer(me, M.layer('Rectangle', { soort: 'vorm', vorm: 'rechthoek', kleur: colour, plaats: { zijde: side, midden: p, breedteM: 0.6, draai: 0, spiegel: 'gekoppeld' } }));
  if (kind === 'image') {
    const pic = await choosePicture(me);
    if (!pic) return;
    return addLayer(me, M.layer('Image', { soort: 'afbeelding', beeld: pic.hash, witDoorzichtig: pic.whiteClear, verhouding: pic.aspect, plaats: { zijde: side, midden: p, breedteM: 1, draai: 0, spiegel: 'gekoppeld', zelfdeRichting: true } }));
  }
}

/** A picture from the computer, kept with the project (`lak_picture_add`). */
export function choosePicture(me) {
  return new Promise((done) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.png,.jpg,.jpeg,.bmp,.tga,.svg,.dds';
    input.onchange = async () => {
      const f = input.files?.[0];
      if (!f) return done(null);
      if (f.size > 20 << 20) {
        say(me, 'The picture is larger than 20 MB.', true);
        return done(null);
      }
      const data = await new Promise((ok) => {
        const r = new FileReader();
        r.onload = () => ok(String(r.result).replace(/^data:[^,]*,/, ''));
        r.readAsDataURL(f);
      });
      try {
        done(await call('lak_picture_add', { id: me.project.id, name: f.name, data }));
      } catch (x) {
        say(me, x.message || String(x), true);
        done(null);
      }
    };
    input.click();
  });
}

/** Stop drawing on the bus: the pen's corners make their shape when `keep` and there are three
 * or more; the brush's strokes stay. */
export function endMaking(me, keep) {
  if (me.making === 'own' && keep && me.pen.nodes.length >= 3) finishPen(me);
  me.pen = { side: null, origin: null, nodes: [] };
  me.brush.gum = false;
  me.making = 'none';
  me.tick();
}

/** The pen's shape as a layer of its own (`finish_pen`, `fit_nodes`). */
function finishPen(me) {
  const pen = me.pen;
  me.pen = { side: null, origin: null, nodes: [] };
  me.making = 'none';
  if (!pen.side || pen.nodes.length < 3) return me.tick();
  const pts = pen.nodes.map(([p, h]) => [p, h ? [p[0] * 2 - h[0], p[1] * 2 - h[1]] : null, h]);
  const lo = [Infinity, Infinity];
  const hi = [-Infinity, -Infinity];
  for (const [p, a, b] of pts)
    for (const q of [p, a, b]) {
      if (!q) continue;
      lo[0] = Math.min(lo[0], q[0]);
      lo[1] = Math.min(lo[1], q[1]);
      hi[0] = Math.max(hi[0], q[0]);
      hi[1] = Math.max(hi[1], q[1]);
    }
  const size = [Math.max(0.01, hi[0] - lo[0]), Math.max(0.01, hi[1] - lo[1])];
  const to = (q) => [(q[0] - lo[0]) / size[0], (hi[1] - q[1]) / size[1]];
  const nodes = pts.map(([p, a, b]) => ({ p: to(p), ...(a ? { c1: to(a) } : {}), ...(b ? { c2: to(b) } : {}) }));
  const { u, v } = M.SIDES[pen.side];
  const mid = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2];
  const centre = addv(addv(pen.origin, mulv(u, mid[0])), mulv(v, mid[1]));
  addLayer(me, M.layer('Own shape', { soort: 'pad', punten: nodes, kleur: me.colour, plaats: { zijde: pen.side, midden: centre, breedteM: size[0], hoogteM: size[1], draai: 0, spiegel: 'gekoppeld' } }));
}

// ------------------------------------------------------------ the mouse on the bus
/** What lies under the mouse (`lakPick`): its place, normal, texture and colour there. */
async function pickAt(me, ndc) {
  if (!me.handle || !me.engineReady) return null;
  const r = await me.handle.ask({ kind: 'lakPick', x: ndc[0], y: ndc[1] }).catch(() => null);
  return r?.hit || null;
}

/** A handle of the chosen layer under the mouse: a decal's corners and sides (size), its turn,
 * a stripe's edges (`handle_at`). */
function handleAt(me, ndc) {
  const l = me.project?.lagen?.find((x) => x.id === me.selected);
  const g = geometry(me);
  if (!l || !g || l.vergrendeld) return undefined;
  const px = [((ndc[0] + 1) / 2) * g.w, ((1 - ndc[1]) / 2) * g.h];
  const near = (q, r = 9) => q && Math.hypot(q[0] - px[0], q[1] - px[1]) < r;
  if (M.isDecal(l)) {
    const size = M.decalSize(l, me.sizes);
    if (!size) return undefined;
    const c = M.corners(l.plaats, size[0], size[1]);
    const rot = rotateHandle(c, size[1]);
    if (near(g.at(rot), 10)) return { kind: 'rotate', id: l.id, start: l.plaats.draai || 0 };
    for (const [grip, q] of M.handles(c)) if (near(g.at(q))) return { kind: 'scale', id: l.id, grip, start: structuredClone(l), size };
  }
  if (l.soort === 'strook') {
    for (const upper of [false, true]) {
      const line = stripeLine(me, l, upper ? Math.max(l.h1, l.h2) : Math.min(l.h1, l.h2));
      const pts = line.map(g.at).filter(Boolean);
      for (let i = 1; i < pts.length; i++) if (distToSegment(px, pts[i - 1], pts[i]) < 7) return { kind: 'edge', id: l.id, upper };
    }
  }
  return undefined;
}

function rotateHandle(c, h) {
  const top = mulv(addv(c[2], c[3]), 0.5);
  const up = unit(sub(c[3], c[0]));
  return addv(top, mulv(up, 0.12 + h * 0.15));
}

function distToSegment(p, a, b) {
  const ab = [b[0] - a[0], b[1] - a[1]];
  const t = Math.min(1, Math.max(0, ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / Math.max(1e-6, ab[0] * ab[0] + ab[1] * ab[1])));
  return Math.hypot(p[0] - (a[0] + ab[0] * t), p[1] - (a[1] + ab[1] * t));
}

/** A stripe's edge along the side the camera sees, at its height (`stripe_line`). */
function stripeLine(me, l, h) {
  const d = me.prep?.dims;
  if (!d || !me.cam) return [];
  const eye = rayAt(me.cam, [0, 0]).o;
  const right = eye[0] >= (d.middleX ?? 0);
  const x = right ? d.max[0] : d.min[0];
  const out = [];
  const n = 40;
  for (let i = 0; i <= n; i++) {
    const fwd = d.min[2] + ((d.max[2] - d.min[2]) * i) / n;
    const [e] = M.stripeEdges({ ...l, h1: 0, h2: 0 }, fwd, d);
    out.push([x, d.min[1] + h + e, fwd]);
  }
  return out;
}

/** The studio's controls of the 3D view (`viewer.js`): the left button is the studio's. */
function controls(me) {
  me.ctrl ??= {
    down: (ndc, e) => {
      if (me.phase !== 'ready' || !me.engineReady) return false;
      down(me, ndc, e);
      return true;
    },
    move: (ndc, e) => move(me, ndc, e),
    up: (ndc, e) => up(me, ndc, e),
    hover: (ndc) => {
      me.hover = ndc;
      me.overlayTick?.();
    },
    double: (ndc) => {
      if (me.making === 'own') finishPen(me);
      else me.handle?.input({ kind: 'stand', stand: 'back' });
      void ndc;
    },
  };
  me.ctrl.cursor = me.making === 'brush' ? 'crosshair' : me.making === 'area' || me.making === 'own' ? 'copy' : undefined;
  return me.ctrl;
}

async function down(me, ndc, e) {
  me.lastNdc = ndc;
  const making = me.making;
  if (making === 'brush') {
    const hit = await pickAt(me, ndc);
    if (!hit) return;
    brushDown(me, hit);
    return;
  }
  if (making === 'area') {
    const hit = await pickAt(me, ndc);
    if (!hit || hit.target === undefined) return;
    const zones = me.prep.targets[hit.target]?.zones;
    const lab = M.lab(hit.colour);
    const l = M.layer('Colour area', { soort: 'zone', centrum: lab, straal: M.fillRadius(zones, lab), kleur: me.colour });
    me.making = 'none';
    addLayer(me, l);
    return;
  }
  if (making === 'own') {
    penClick(me, ndc);
    return;
  }
  // a handle of the chosen layer, else what lies under the mouse
  const h = handleAt(me, ndc);
  if (h) {
    const l = me.project.lagen.find((x) => x.id === h.id);
    const ray = me.cam && rayAt(me.cam, ndc);
    if (h.kind === 'scale' && ray) {
      const sn = M.SIDES[l.plaats.zijde].n;
      const q = onPlane(ray, l.plaats.midden, sn);
      if (!q) return;
      me.drag = { ...h, from: M.local(l.plaats, q) };
    } else if (h.kind === 'rotate' && ray) {
      const sn = M.SIDES[l.plaats.zijde].n;
      const q = onPlane(ray, l.plaats.midden, sn);
      if (!q) return;
      const v = M.local({ ...l.plaats, draai: 0 }, q);
      me.drag = { ...h, from: (Math.atan2(v[1], v[0]) * 180) / Math.PI };
    } else me.drag = h;
    return;
  }
  me.drag = { kind: 'wait' };
  const hit = await pickAt(me, ndc);
  if (me.drag?.kind !== 'wait') return;
  if (!hit) {
    me.drag = { kind: 'turn', last: me.lastNdc };
    select(me, null);
    return;
  }
  const planeX = me.prep ? M.mirrorPlane(me.project, me.prep.dims) : undefined;
  const found = M.decalAt(me.project.lagen, me.sizes, planeX, hit.p, hit.n);
  if (found) {
    const l = me.project.lagen.find((x) => x.id === found.id);
    let p = hit.p;
    if (found.copy) p = [2 * (planeX ?? 0) - p[0], p[1], p[2]];
    me.drag = { kind: 'move', id: found.id, copy: found.copy, offset: sub(p, l.plaats.midden) };
    select(me, found.id);
    return;
  }
  // a stripe or a colour under the mouse: the top layer that paints there is not known here;
  // the bus turns instead
  me.drag = { kind: 'turn', last: me.lastNdc };
  if (hit.glass) say(me, 'On a window: the layer goes over the windows too (Painting, on the right).');
}

async function move(me, ndc, e) {
  const prev = me.lastNdc || ndc;
  me.lastNdc = ndc;
  me.hover = ndc;
  const d = me.drag;
  if (!d) return;
  if (d.kind === 'turn') {
    const g = geometry(me);
    if (g) me.handle?.input({ kind: 'drag', dx: ((ndc[0] - prev[0]) * g.w) / 2, dy: (-(ndc[1] - prev[1]) * g.h) / 2 });
    return;
  }
  if (d.kind === 'wait') return;
  const ray = me.cam && rayAt(me.cam, ndc);
  if (d.kind === 'move' || d.kind === 'brush') {
    if (me.picking) {
      me.morePick = true;
      return;
    }
    me.picking = true;
    const hit = await pickAt(me, ndc);
    me.picking = false;
    if (hit && me.drag === d) {
      if (d.kind === 'move') moveTo(me, d, hit);
      else brushTo(me, d, hit);
    }
    if (me.morePick) {
      me.morePick = false;
      if (me.drag === d) move(me, me.lastNdc, e);
    }
    return;
  }
  if (!ray) return;
  const l = me.project.lagen.find((x) => x.id === d.id);
  if (!l) return;
  if (d.kind === 'scale') {
    const q = onPlane(ray, d.start.plaats.midden, M.SIDES[d.start.plaats.zijde].n);
    if (!q) return;
    const now = M.local(d.start.plaats, q);
    change(me, (p) => {
      const i = p.lagen.findIndex((x) => x.id === d.id);
      p.lagen[i] = M.pulled(p.lagen[i], d.start, d.grip, d.size, d.from, now, e?.shiftKey);
    }, { pending: true });
  } else if (d.kind === 'rotate') {
    const q = onPlane(ray, l.plaats.midden, M.SIDES[l.plaats.zijde].n);
    if (!q) return;
    const v = M.local({ ...l.plaats, draai: 0 }, q);
    let a = d.start + (Math.atan2(v[1], v[0]) * 180) / Math.PI - d.from;
    while (a > 180) a -= 360;
    while (a < -180) a += 360;
    if (e?.shiftKey) a = Math.round(a / 15) * 15;
    change(me, (p) => (p.lagen.find((x) => x.id === d.id).plaats.draai = a), { pending: true });
  } else if (d.kind === 'edge') {
    const dims = me.prep.dims;
    const right = ray.o[0] >= (dims.middleX ?? 0);
    const q = onPlane(ray, [right ? dims.max[0] : dims.min[0], 0, 0], [right ? 1 : -1, 0, 0]);
    if (!q) return;
    const [e0] = M.stripeEdges({ ...l, h1: 0, h2: 0 }, q[2], dims);
    let h = q[1] - dims.min[1] - e0;
    const w = M.windowHeight(dims);
    if (Math.abs(h - w) < 0.03) h = w;
    change(me, (p) => {
      const s = p.lagen.find((x) => x.id === d.id);
      const lo = Math.min(s.h1, s.h2);
      const hi = Math.max(s.h1, s.h2);
      if (d.upper) [s.h1, s.h2] = [lo, Math.max(h, lo + 0.02)];
      else [s.h1, s.h2] = [Math.min(h, hi - 0.02), hi];
    }, { pending: true });
  } else if (d.kind === 'curve') {
    const q = onPlane(ray, me.pen.origin, M.SIDES[me.pen.side].n);
    if (!q) return;
    const last = me.pen.nodes[me.pen.nodes.length - 1];
    if (last) last[1] = M.local({ zijde: me.pen.side, midden: me.pen.origin, draai: 0 }, q);
    me.overlayTick?.();
  }
}

function moveTo(me, d, hit) {
  const planeX = me.prep ? M.mirrorPlane(me.project, me.prep.dims) ?? 0 : 0;
  const p = d.copy ? [2 * planeX - hit.p[0], hit.p[1], hit.p[2]] : hit.p;
  const n = d.copy ? [-hit.n[0], hit.n[1], hit.n[2]] : hit.n;
  change(me, (pr) => {
    const l = pr.lagen.find((x) => x.id === d.id);
    if (!l || l.vergrendeld) return;
    l.plaats = { ...l.plaats, midden: sub(p, d.offset) };
    const side = M.sideOfNormal(n);
    if (side !== l.plaats.zijde && dot(n, M.SIDES[side].n) > 0.85) l.plaats.zijde = side;
  }, { pending: true });
}

function up(me, ndc, e) {
  const d = me.drag;
  me.drag = null;
  if (d?.kind === 'brush') {
    settle(me);
    touched(me);
  } else if (d) {
    settle(me);
    if (d.kind !== 'turn' && d.kind !== 'wait') touched(me);
  }
  void ndc;
  void e;
}

function brushDown(me, hit) {
  let id = me.brushLayer && me.project.lagen.find((l) => l.id === me.brushLayer && l.soort === 'penseel' && !l.vergrendeld)?.id;
  if (!id && me.brush.gum) return say(me, 'Nothing painted with the brush yet to erase.');
  const stroke = { punten: [M.toBus(hit.p)], normalen: [M.toBus(hit.n)], straalCm: me.brush.straalCm, hardheid: me.brush.hardheid, dekking: me.brush.dekking, gum: me.brush.gum };
  change(me, (p) => {
    if (!id) {
      const l = M.layer('Brush', { soort: 'penseel', kleur: me.colour, streken: [] });
      p.lagen.push(l);
      id = l.id;
    }
    p.lagen.find((l) => l.id === id).streken.push(stroke);
  }, { pending: true, live: id });
  me.brushLayer = id;
  me.selected = id;
  me.drag = { kind: 'brush', id };
}

function brushTo(me, d, hit) {
  const l = me.project.lagen.find((x) => x.id === d.id);
  const st = l?.streken?.[l.streken.length - 1];
  if (!st) return;
  const last = st.punten[st.punten.length - 1];
  const p = M.toBus(hit.p);
  if (last && Math.hypot(p[0] - last[0], p[1] - last[1], p[2] - last[2]) < (st.straalCm / 100) * 0.3) return;
  change(me, () => {
    st.punten.push(p);
    st.normalen.push(M.toBus(hit.n));
  }, { pending: true, live: d.id });
}

async function penClick(me, ndc) {
  const ray = me.cam && rayAt(me.cam, ndc);
  if (!me.pen.side) {
    const hit = await pickAt(me, ndc);
    if (!hit) return;
    me.pen = { side: M.sideOfNormal(hit.n), origin: hit.p, nodes: [[[0, 0], null]] };
    me.drag = { kind: 'curve' };
    me.tick();
    return;
  }
  if (!ray) return;
  const q = onPlane(ray, me.pen.origin, M.SIDES[me.pen.side].n);
  if (!q) return;
  // back on the first corner: done
  const g = geometry(me);
  const first = g?.at(me.pen.origin);
  const px = g && [((ndc[0] + 1) / 2) * g.w, ((1 - ndc[1]) / 2) * g.h];
  if (me.pen.nodes.length >= 3 && first && Math.hypot(first[0] - px[0], first[1] - px[1]) < 10) return finishPen(me);
  me.pen.nodes.push([M.local({ zijde: me.pen.side, midden: me.pen.origin, draai: 0 }, q), null]);
  me.drag = { kind: 'curve' };
  me.tick();
}

// ------------------------------------------------------------ the keys
function key(me, e, isDown) {
  const t = e.target;
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
  if (e.key === 'o' || e.key === 'O') {
    if (isDown !== me.beforeOn) before(me, isDown);
    return;
  }
  if (!isDown) return;
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && (e.key === 'z' || e.key === 'Z')) {
    if (e.shiftKey) redo(me);
    else undo(me);
  } else if (ctrl && (e.key === 'y' || e.key === 'Y')) redo(me);
  else if (e.key === 'Escape') {
    if (me.making !== 'none') endMaking(me, false);
    else if (me.kinds) (me.kinds = false), me.tick();
    else select(me, null);
  } else if (e.key === 'Enter' && me.making === 'own') finishPen(me);
  else if ((e.key === 'Delete' || e.key === 'Backspace') && me.selected) removeLayer(me, me.selected);
  else if (e.key === 'e' || e.key === 'E') {
    if (me.making !== 'brush') {
      me.making = 'brush';
      me.selected = null;
    }
    me.brush.gum = !me.brush.gum;
    me.tick();
  } else if (e.key === '[' || e.key === ']') {
    me.brush.straalCm = Math.min(200, Math.max(1, me.brush.straalCm * (e.key === ']' ? 1.2 : 1 / 1.2)));
    me.tick();
  } else if (e.key === 'f' || e.key === 'F') me.handle?.input({ kind: 'fit' });
  else if (/^[1-6]$/.test(e.key)) view(me, VIEWS[Number(e.key) - 1][0]);
  else return;
  e.preventDefault();
}

function view(me, v) {
  me.handle?.input({ kind: 'stand', stand: v });
}

function before(me, on) {
  me.beforeOn = on;
  me.handle?.send({ kind: 'lakBefore', on });
  me.tick();
}

export function removeLayer(me, id) {
  change(me, (p) => (p.lagen = p.lagen.filter((l) => l.id !== id)));
  if (me.selected === id) me.selected = null;
  me.tick();
}

// ------------------------------------------------------------ the save into the game
async function cost(me) {
  if (!me.company) return;
  me.cost = await call('lak_cost', { company: me.company, vehicle: me.vehicle, name: me.name }).catch(() => null);
  me.tick();
}

async function save(me) {
  settle(me);
  const p = me.project;
  const name = (me.name || '').trim();
  const check = await call('lak_name', { name }).catch((x) => ({ error: x.message }));
  if (check.error) return say(me, check.error, true);
  p.naam = name;
  try {
    const r = await call('lak_place', { project: p, name, family: p.ookOp || [], company: me.company || '', vehicle: me.vehicle });
    me.saving = { text: r.text, f: 0 };
    me.tick();
  } catch (x) {
    say(me, x.message || String(x), true);
  }
}

async function pollSave(me) {
  if (!me.saving) return;
  const r = await call('lak_progress').catch((x) => ({ error: x.message }));
  if (r.busy) {
    me.saving = { text: r.text, f: r.f };
    me.tick();
    return;
  }
  me.saving = null;
  if (r.error) say(me, r.error, true);
  else if (r.done) {
    me.project = r.done.project;
    me.name = r.done.project.naam;
    say(me, r.done.text);
    forgetPhotos(me.project.bus);
    const o = me.open;
    // the bus step's livery: the new one (its list read again with it)
    const v = (S.vehicles || []).find((x) => x.file === me.project.bus);
    if (v && !(v.paints || []).some((x) => x.toLowerCase() === r.done.name.toLowerCase())) set({ vehicles: S.vehicles.map((x) => (x === v ? { ...v, paints: [...(v.paints || []), r.done.name] } : x)) });
    if (S.choice?.bus === me.project.bus && !me.company) choose({ paint: r.done.name });
    if (r.done.company) o.onCompanyChanged?.(r.done.company);
    cost(me);
  }
  me.tick();
}

async function removeFromGame(me) {
  try {
    const r = await call('lak_remove', { project: me.project });
    const name = me.project.geplaatst?.naam;
    me.project = r.project;
    if (r.text) say(me, r.text);
    const v = (S.vehicles || []).find((x) => x.file === me.project.bus);
    if (v && name) set({ vehicles: S.vehicles.map((x) => (x === v ? { ...v, paints: (v.paints || []).filter((n) => n.toLowerCase() !== name.toLowerCase()) } : x)) });
    if (S.choice?.bus === me.project.bus && name && S.choice.paint?.toLowerCase() === name.toLowerCase()) choose({ paint: '' });
    forgetPhotos(me.project.bus);
  } catch (x) {
    say(me, x.message || String(x), true);
  }
  me.tick();
}

/** Out of the studio: to the bus company it painted for, else where it was opened from. */
function leave(me) {
  settle(me);
  keepNow(me, true);
  const o = me.open || {};
  set({ page: o.back || (o.company ? 'drive' : 'editor'), hub: { ...S.hub, palette: false } });
}

// ------------------------------------------------------------ the parts of the page
function Top({ me }) {
  const p = me.project;
  const canUndo = me.history.undo.length > 0 || !!me.pending;
  const label = me.cost?.label && !me.saving ? me.cost.label : 'Save';
  return html`<div class="lak-top">
    <${Btn} kind="ghost" icon="chevron_left" label="Back" onClick=${() => leave(me)} />
    <${Field} class="lak-name" value=${me.name} placeholder="Untitled livery" raw=${false} onInput=${(t) => ((me.name = t), me.tick(), clearTimeout(me.costClock), (me.costClock = setTimeout(() => cost(me), 300)))} />
    <${IconBtn} icon="livery_undo" title="Undo (Ctrl+Z)" disabled=${!canUndo} onClick=${() => undo(me)} />
    <${IconBtn} icon="livery_redo" title="Redo (Ctrl+Y)" disabled=${!me.history.redo.length} onClick=${() => redo(me)} />
    <button type="button" class=${'lak-chip' + (p?.spiegel?.aan !== false ? ' on' : '')} title=${tr('The same on the other side')} onClick=${() => change(me, (x) => (x.spiegel = { ...(x.spiegel || {}), aan: x.spiegel?.aan === false }))}>
      <${Icon} name="livery_mirror" size=${16} /><span>${tr('Mirror')}</span>
    </button>
    <span class="lak-gap"></span>
    ${p?.geplaatst && html`<${Btn} kind="ghost" icon="delete" label="Remove from the game" onClick=${() => removeFromGame(me)} disabled=${!!me.saving} />`}
    <${Btn} kind="primary" icon="save" label=${label} title=${me.cost?.tip} disabled=${!!me.saving || me.phase !== 'ready' || (me.cost && !me.cost.enough)} onClick=${() => save(me)} />
  </div>`;
}

function Layers({ me }) {
  const p = me.project;
  const list = [...(p?.lagen || [])].reverse();
  const move = (id, by) => change(me, (x) => {
    const i = x.lagen.findIndex((l) => l.id === id);
    const j = Math.min(x.lagen.length - 1, Math.max(0, i + by));
    if (i < 0 || i === j) return;
    const [l] = x.lagen.splice(i, 1);
    x.lagen.splice(j, 0, l);
  });
  return html`<div class="lak-left">
    <div class="lak-create">
      <${Btn} kind="primary" icon="add" label="Create layer" onClick=${() => ((me.kinds = !me.kinds), me.tick())} />
      ${me.kinds && html`<div class="lak-kinds">
        ${KINDS.map(([k, name, line, icon]) => html`<button type="button" onClick=${() => create(me, k)}>
          <${Icon} name=${icon} size=${22} /><span><b>${tr(name)}</b><small>${tr(line)}</small></span>
        </button>`)}
      </div>`}
    </div>
    <div class="lak-list">
      ${!list.length && html`<p class="lak-empty">${tr('No layers yet: the quick livery on the right, or Create layer.')}</p>`}
      ${list.map((l) => html`<div class=${'lak-layer' + (l.id === me.selected ? ' on' : '') + (l.zichtbaar === false ? ' off' : '')} onClick=${() => select(me, l.id)}>
        <i class="sw" style=${`background:${l.kleur || '#888'}`}></i>
        <span class="nm">${l.naam ? tr(l.naam) : tr(M.kindLabel(l))}</span>
        <button type="button" title=${tr(l.zichtbaar === false ? 'Show' : 'Hide')} onClick=${(e) => (e.stopPropagation(), change(me, (x) => (x.lagen.find((y) => y.id === l.id).zichtbaar = l.zichtbaar === false)))}><${Icon} name=${l.zichtbaar === false ? 'livery_eye_off' : 'visibility'} size=${16} /></button>
        <button type="button" title=${tr(l.vergrendeld ? 'Unlock' : 'Lock')} onClick=${(e) => (e.stopPropagation(), change(me, (x) => {
          const y = x.lagen.find((z) => z.id === l.id);
          if (y.vergrendeld) delete y.vergrendeld;
          else y.vergrendeld = true;
        }))}><${Icon} name=${l.vergrendeld ? 'lock' : 'livery_unlock'} size=${16} /></button>
      </div>`)}
    </div>
    ${me.selected && html`<div class="lak-layer-tools">
      <${IconBtn} icon="arrow_upward" title="Up" onClick=${() => move(me.selected, 1)} />
      <${IconBtn} icon="arrow_downward" title="Down" onClick=${() => move(me.selected, -1)} />
      <${IconBtn} icon="content_copy" title="Duplicate" onClick=${() => {
        const i = p.lagen.findIndex((l) => l.id === me.selected);
        let id;
        change(me, (x) => {
          const r = M.duplicate(x.lagen, i);
          x.lagen = r.layers;
          id = r.id;
        });
        if (id) select(me, id);
      }} />
      <${IconBtn} icon="delete" title="Delete" onClick=${() => removeLayer(me, me.selected)} />
    </div>`}
    <p class="lak-begun">${tr('Begun from:')} ${p?.startKleurstelling || tr('the model\'s own')}</p>
  </div>`;
}

/** The view's size and a way to put an o3d point on it. */
function geometry(me) {
  const el = me.viewEl;
  if (!el || !me.cam) return undefined;
  const r = el.getBoundingClientRect();
  // (the stage may be scaled: the overlay draws in the view's own pixels)
  const w = el.clientWidth || r.width;
  const h = el.clientHeight || r.height;
  return { w, h, at: (p) => toScreen(me.cam, p, w, h) };
}

/** What is drawn over the bus: the chosen layer's outline and handles, its copy's outline, a
 * stripe's edges, the pen's corners, the brush's size. */
function Overlay({ me }) {
  const [, redraw] = useState(0);
  me.overlayTick = () => {
    if (me.overlayPlanned) return;
    me.overlayPlanned = true;
    requestAnimationFrame(() => {
      me.overlayPlanned = false;
      redraw((n) => n + 1);
    });
  };
  const g = geometry(me);
  if (!g || !me.project) return html`<svg class="lak-overlay"></svg>`;
  const l = me.project.lagen.find((x) => x.id === me.selected);
  const parts = [];
  const poly = (pts, cls) => {
    const s = pts.map(g.at);
    if (s.some((x) => !x)) return;
    parts.push(html`<polygon class=${cls} points=${s.map((q) => q.join(',')).join(' ')} />`);
  };
  const line = (pts, cls) => {
    const s = pts.map(g.at).filter(Boolean);
    if (s.length > 1) parts.push(html`<polyline class=${cls} points=${s.map((q) => q.join(',')).join(' ')} />`);
  };
  if (l && M.isDecal(l) && l.zichtbaar !== false) {
    const size = M.decalSize(l, me.sizes);
    if (size) {
      const c = M.corners(l.plaats, size[0], size[1]);
      poly(c, 'lak-outline');
      const planeX = M.mirrorPlane(me.project, me.prep?.dims || {});
      if (planeX !== undefined) {
        const m = M.mirrorPlace(l.plaats, planeX, M.mirrorImage(l));
        if (m) poly(M.corners(m.place, size[0], size[1]), 'lak-outline copy');
      }
      if (!l.vergrendeld) {
        for (const [, q] of M.handles(c)) {
          const s = g.at(q);
          if (s) parts.push(html`<rect class="lak-grip" x=${s[0] - 4} y=${s[1] - 4} width="8" height="8" />`);
        }
        const r = g.at(rotateHandle(c, size[1]));
        const top = g.at(M.corners(l.plaats, size[0], size[1]).slice(2).reduce((a, b) => mulv(addv(a, b), 0.5)));
        if (r && top) parts.push(html`<line class="lak-outline" x1=${top[0]} y1=${top[1]} x2=${r[0]} y2=${r[1]} />`, html`<circle class="lak-grip round" cx=${r[0]} cy=${r[1]} r="5" />`);
      }
    }
  }
  if (l?.soort === 'strook' && l.zichtbaar !== false) {
    line(stripeLine(me, l, Math.min(l.h1, l.h2)), 'lak-edge');
    line(stripeLine(me, l, Math.max(l.h1, l.h2)), 'lak-edge');
  }
  if (me.making === 'own' && me.pen.side) {
    const { u, v } = M.SIDES[me.pen.side];
    const pts = me.pen.nodes.map(([q]) => addv(addv(me.pen.origin, mulv(u, q[0])), mulv(v, q[1])));
    line(pts, 'lak-pen');
    for (const p of pts) {
      const s = g.at(p);
      if (s) parts.push(html`<circle class="lak-grip round" cx=${s[0]} cy=${s[1]} r="4" />`);
    }
  }
  if (me.making === 'brush' && me.hover) {
    const s = [((me.hover[0] + 1) / 2) * g.w, ((1 - me.hover[1]) / 2) * g.h];
    // (the brush's radius at the picture's middle distance: a guide, not exact)
    const r = Math.max(3, me.brush.straalCm * (g.h / 600));
    parts.push(html`<circle class=${'lak-brush' + (me.brush.gum ? ' gum' : '')} cx=${s[0]} cy=${s[1]} r=${r} />`);
  }
  return html`<svg class="lak-overlay" viewBox=${`0 0 ${g.w} ${g.h}`} width=${g.w} height=${g.h}>${parts}</svg>`;
}

/** No bus asked for: a livery begun before, or a new one on a bus. */
function Chooser({ me }) {
  const s = useStore();
  const [bus, setBus] = useState(S.choice?.bus || '');
  const [paint, setPaint] = useState('');
  const [startWay, setStart] = useState('snel');
  const v = (s.vehicles || []).find((x) => x.file === bus);
  const go = (o) => set({ ed: { ...S.ed, liveryOpen: { ...me.open, ...o, at: Date.now() } } });
  return html`<div class="lak-studio lak-chooser">
    <div class="lak-top"><${Btn} kind="ghost" icon="chevron_left" label="Back" onClick=${() => leave(me)} /><b>${tr('Livery editor')}</b></div>
    <div class="lak-choose">
      <section>
        <h3>${tr('New livery')}</h3>
        <label>${tr('Bus')}<select value=${bus} onChange=${(e) => (setBus(e.currentTarget.value), setPaint(''))}>
          <option value="">${tr('Choose a bus')}</option>
          ${(s.vehicles || []).filter((x) => !x.ai).map((x) => html`<option value=${x.file}>${x.name}</option>`)}
        </select></label>
        <label>${tr('Begin from')}<select value=${paint} onChange=${(e) => setPaint(e.currentTarget.value)}>
          <option value="">${tr("The model's own paint")}</option>
          ${(v?.paints || []).map((x) => html`<option value=${x}>${x}</option>`)}
        </select></label>
        <label>${tr('How')}<select value=${startWay} onChange=${(e) => setStart(e.currentTarget.value)}>
          ${[['snel', 'Quick livery'], ['precies', 'This livery as it is'], ['effenKleuren', 'Its colours, plain'], ['effen', 'Plain']].map(([k, label]) => html`<option value=${k}>${tr(label)}</option>`)}
        </select></label>
        <${Btn} kind="primary" icon="palette" label="Begin" disabled=${!bus} onClick=${() => go({ bus, paint, start: startWay, project: undefined })} />
      </section>
      <section>
        <h3>${tr('Liveries begun')}</h3>
        ${!(me.projects || []).length && html`<p class="lak-empty">${tr('None yet.')}</p>`}
        ${(me.projects || []).map((p) => html`<button type="button" class="lak-project" onClick=${() => go({ project: p.id, bus: p.bus })}>
          <b>${p.name || tr('Untitled livery')}</b><small>${(s.vehicles || []).find((x) => x.file === p.bus)?.name || p.bus} · ${p.layers} ${tr('layers')}${p.placed ? ` · ${tr('in the game')}` : ''}</small>
        </button>`)}
      </section>
    </div>
  </div>`;
}

export { change, settle, say, touched, select };

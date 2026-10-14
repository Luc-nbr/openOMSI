// The livery studio's right side (the game program's studio's `right_panel`): what the chosen
// layer offers, what is being drawn on the bus, or - nothing chosen - the quick livery, the
// mirror and the bus's family. Every change is the project's at once (one step to undo when it
// ends: a slider let go, a field left).

import { html, useState } from '../../../../vendor/preact-htm.js';
import { call } from '../../../api.js';
import { Btn, ColourPicker, Drop, Field, Icon, LiveField, Toggle, tr } from '../../editors/kit.js';
import * as M from './model.js';
import { change, choosePicture, endMaking, say, settle } from './studio.js';

const STRIPES = [
  ['onderband', 'Skirt band'],
  ['raamband', 'Window band'],
  ['dakband', 'Roof band'],
  ['schuin', 'Slanted'],
  ['golf', 'Wave'],
  ['tweekleurig', 'Two-tone'],
  ['frontvlak', 'Front face'],
  ['achtervlak', 'Rear face'],
];
const SIDES = [['rondom', 'All round'], ['zijden', 'Sides'], ['voor', 'Front'], ['achter', 'Rear']];
const DECAL_SIDES = [['L', 'Left'], ['R', 'Right'], ['V', 'Front'], ['A', 'Rear'], ['D', 'Roof']];

/** A slider with its words and value (a step to undo when it is let go). */
function Range({ label, value, min, max, step = 0.01, onInput, onDone, unit = '', digits = 2 }) {
  return html`<label class="lak-range"><span>${tr(label)}</span>
    <input type="range" min=${min} max=${max} step=${step} value=${value} onInput=${(e) => onInput(Number(e.currentTarget.value))} onChange=${() => onDone?.()} />
    <b>${Number(value).toFixed(digits)}${unit}</b></label>`;
}

/** A number in a field (a step to undo when it is left). */
function Num({ label, value, onChange, step = 0.01, min, max, unit = '' }) {
  return html`<label class="lak-num"><span>${tr(label)}</span>
    <input type="number" step=${step} min=${min} max=${max} value=${Math.round(value * 1000) / 1000} onChange=${(e) => {
      const v = Number(e.currentTarget.value);
      if (Number.isFinite(v)) onChange(v);
    }} />${unit && html`<em>${unit}</em>`}</label>`;
}

/** The colour picker with the colours used last. */
function Colour({ me, value, onChange }) {
  return html`<div class="lak-colour">
    <${ColourPicker} value=${value || '#ffffff'} onChange=${(c) => {
      me.colour = c;
      onChange(c);
    }} />
    ${me.recent.length > 0 && html`<div class="lak-recent">${me.recent.map((c) => html`<button type="button" style=${`background:${c}`} title=${c} onClick=${() => onChange(c)}></button>`)}</div>`}
  </div>`;
}

function remember(me, c) {
  if (!c) return;
  me.recent = [c, ...me.recent.filter((x) => x.toLowerCase() !== c.toLowerCase())].slice(0, 10);
}

export function Panel({ me, layer }) {
  if (!me.project || me.phase !== 'ready') return html`<div class="lak-panel"></div>`;
  if (me.making !== 'none') return html`<${Making} me=${me} />`;
  if (layer) return html`<${LayerPanel} me=${me} l=${layer} key=${layer.id} />`;
  return html`<${StartPanel} me=${me} />`;
}

// ------------------------------------------------------------ drawing on the bus
function Making({ me }) {
  const b = me.brush;
  const set = (k, v) => {
    b[k] = v;
    me.tick();
  };
  if (me.making === 'brush') {
    return html`<div class="lak-panel">
      <h3><${Icon} name=${b.gum ? 'livery_eraser' : 'livery_brush'} size=${18} />${tr(b.gum ? 'Eraser' : 'Brush')}</h3>
      <p class="lak-hint">${tr('Paint on the bus with the left button. E switches between the brush and the eraser, [ and ] make it smaller and larger. The strokes are kept as lines on the bus: they stay sharp at any size.')}</p>
      <${Toggle} label="Eraser" checked=${b.gum} onChange=${(v) => set('gum', v)} />
      <${Range} label="Size" value=${b.straalCm} min=${1} max=${100} step=${1} digits=${0} unit=" cm" onInput=${(v) => set('straalCm', v)} />
      <${Range} label="Hardness" value=${b.hardheid} min=${0} max=${1} onInput=${(v) => set('hardheid', v)} />
      <${Range} label="Cover" value=${b.dekking} min=${0.05} max=${1} onInput=${(v) => set('dekking', v)} />
      ${!b.gum && html`<${Colour} me=${me} value=${me.colour} onChange=${(c) => {
        me.colour = c;
        if (me.brushLayer) change(me, (p) => (p.lagen.find((l) => l.id === me.brushLayer).kleur = c));
        me.tick();
      }} />`}
      <${Btn} label="Done (Escape)" onClick=${() => endMaking(me, true)} />
    </div>`;
  }
  if (me.making === 'own') {
    return html`<div class="lak-panel">
      <h3><${Icon} name="livery_pen" size=${18} />${tr('Own shape')}</h3>
      <p class="lak-hint">${tr('Click the corners of your shape on the bus. Hold the button and drag to bend the line into a curve. Click the first corner again or press Enter to finish; Escape lets go.')}</p>
      <p>${tr('%{n} corners', { n: me.pen.nodes.length })}</p>
      <${Colour} me=${me} value=${me.colour} onChange=${(c) => ((me.colour = c), me.tick())} />
    </div>`;
  }
  return html`<div class="lak-panel">
    <h3><${Icon} name="livery_eyedrop" size=${18} />${tr('Colour area')}</h3>
    <p class="lak-hint">${tr('Click a colour on the bus: all of that colour takes the new one. Rubbers, lamps and windows keep theirs.')}</p>
    <${Colour} me=${me} value=${me.colour} onChange=${(c) => ((me.colour = c), me.tick())} />
  </div>`;
}

// ------------------------------------------------------------ nothing chosen
function StartPanel({ me }) {
  const p = me.project;
  const q = p.snel || { kleuren: [null, null, null], strook: 'onderband' };
  const [which, setWhich] = useState(0);
  const apply = async (next) => {
    const quick = { ...q, ...next };
    try {
      const r = await call('lak_quick', { project: p, quick });
      change(me, (x) => {
        x.snel = quick;
        x.lagen = r.layers;
      });
    } catch (x) {
      say(me, x.message || String(x), true);
    }
  };
  const colours = q.kleuren || [null, null, null];
  const family = me.prep?.family || [];
  return html`<div class="lak-panel">
    <h3><${Icon} name="livery_fill" size=${18} />${tr('Quick livery')}</h3>
    <p class="lak-hint">${tr('Three colours, a stripe, a name and a logo: every choice becomes a layer you can change afterwards.')}</p>
    <div class="lak-swatches">
      ${['Body', 'Stripe', 'Text'].map((label, k) => html`<button type="button" class=${which === k ? 'on' : ''} onClick=${() => setWhich(k)}>
        <i style=${`background:${colours[k] || 'transparent'}`}></i><span>${tr(label)}</span>
      </button>`)}
    </div>
    <${Colour} me=${me} value=${colours[which] || me.colour} onChange=${(c) => {
      remember(me, c);
      const k = [...colours];
      k[which] = c;
      apply({ kleuren: k });
    }} />
    <label class="lak-row"><span>${tr('Stripe')}</span><${Drop} value=${q.strook || 'onderband'} options=${STRIPES} onChange=${(v) => apply({ strook: v, strookGekozen: true })} /></label>
    <label class="lak-row"><span>${tr('Name on the bus')}</span><${LiveField} value=${q.naam || ''} raw placeholder="" onInput=${(t) => apply({ naam: t })} /></label>
    <div class="lak-row"><span>${tr('Logo')}</span>
      <${Btn} label=${q.logo ? 'Another logo…' : 'Choose a logo…'} onClick=${async () => {
        const pic = await choosePicture(me);
        if (pic) apply({ logo: pic.hash, logoVerhouding: pic.aspect });
      }} />
      ${q.logo && html`<${Btn} kind="ghost" label="No logo" onClick=${() => apply({ logo: undefined, logoVerhouding: undefined })} />`}
    </div>
    ${me.fonts && html`<label class="lak-row"><span>${tr('Font')}</span><${Drop} raw value=${q.lettertype || me.fonts.default} options=${me.fonts.fonts.map((f) => [f, f])} onChange=${(v) => apply({ lettertype: v })} /></label>`}
    <h3><${Icon} name="livery_mirror" size=${18} />${tr('Mirror')}</h3>
    <${Toggle} label="Also on the other side" checked=${p.spiegel?.aan !== false} onChange=${(v) => change(me, (x) => (x.spiegel = { ...(x.spiegel || {}), aan: v }))} />
    ${family.length > 0 && html`<h3><${Icon} name="bus" size=${18} />${tr('Also on')}</h3>
      <p class="lak-hint">${tr('These buses take the same paint. Look at the livery on them; those with a folder of their own get it too when you tick them.')}</p>
      ${family.map((k) => html`<${Toggle} raw label=${k.name || k.bus} checked=${(p.ookOp || []).includes(k.bus)} onChange=${(v) => change(me, (x) => (x.ookOp = v ? [...(x.ookOp || []), k.bus] : (x.ookOp || []).filter((b) => b !== k.bus)))} />`)}`}
    <p class="lak-hint">${tr('Click a layer on the bus to change it, drag it to move it. Right-drag turns the bus, the wheel zooms, 1-6 are the views, F fits, O held shows the livery before.')}</p>
    <p class="lak-info">${(me.prep?.targets || []).map((t) => `${t.file} ${t.w} × ${t.h}`).join(' · ')}</p>
  </div>`;
}

// ------------------------------------------------------------ the chosen layer
function LayerPanel({ me, l }) {
  const edit = (fn, pending = false) => change(me, (p) => {
    const x = p.lagen.find((y) => y.id === l.id);
    if (x && !x.vergrendeld) fn(x);
  }, { pending });
  const done = () => settle(me);
  const hasColour = l.soort !== 'afbeelding';
  const gradable = ['zone', 'strook', 'tekst', 'vorm', 'pad'].includes(l.soort);
  const outlined = ['tekst', 'vorm', 'pad'].includes(l.soort);
  const [pickFor, setPickFor] = useState('main');
  const colourNow = pickFor === 'outline' && l.omlijning ? l.omlijning.kleur : pickFor === 'gradient' && l.verloop ? l.verloop.kleur2 : l.kleur;
  const windows = me.prep?.windows;
  return html`<div class="lak-panel">
    <h3><${Icon} name=${{ zone: 'livery_fill', strook: 'livery_stripe', tekst: 'livery_text', afbeelding: 'livery_image', vorm: 'livery_shape', pad: 'livery_pen', penseel: 'livery_brush' }[l.soort] || 'palette'} size=${18} />${tr(M.kindLabel(l))}</h3>
    ${l.vergrendeld && html`<p class="lak-hint">${tr('This layer is locked: unlock it in the list to change it.')}</p>`}
    <label class="lak-row"><span>${tr('Layer name')}</span><${LiveField} raw value=${l.naam || ''} onInput=${(t) => edit((x) => (x.naam = t))} /></label>
    ${hasColour && html`
      ${(outlined || gradable) && html`<div class="lak-swatches">
        <button type="button" class=${pickFor === 'main' ? 'on' : ''} onClick=${() => setPickFor('main')}><i style=${`background:${l.kleur}`}></i><span>${tr('Colour')}</span></button>
        ${outlined && l.omlijning && html`<button type="button" class=${pickFor === 'outline' ? 'on' : ''} onClick=${() => setPickFor('outline')}><i style=${`background:${l.omlijning.kleur}`}></i><span>${tr('Outline')}</span></button>`}
        ${gradable && l.verloop && html`<button type="button" class=${pickFor === 'gradient' ? 'on' : ''} onClick=${() => setPickFor('gradient')}><i style=${`background:${l.verloop.kleur2}`}></i><span>${tr('Gradient')}</span></button>`}
      </div>`}
      <${Colour} me=${me} value=${colourNow} onChange=${(c) => {
        remember(me, c);
        edit((x) => {
          if (pickFor === 'outline' && x.omlijning) x.omlijning.kleur = c;
          else if (pickFor === 'gradient' && x.verloop) x.verloop.kleur2 = c;
          else x.kleur = c;
        });
      }} />`}
    <${Range} label="Opacity" value=${l.dekking ?? 1} min=${0} max=${1} onInput=${(v) => edit((x) => (x.dekking = v), true)} onDone=${done} />
    <${Range} label="Keep details" value=${l.detail ?? 1} min=${0} max=${1} onInput=${(v) => edit((x) => (x.detail = v), true)} onDone=${done} />
    ${l.soort !== 'penseel' && html`<${Toggle} label="Also over rubbers and lamps" checked=${!!l.ookOverRubbers} onChange=${(v) => edit((x) => (v ? (x.ookOverRubbers = true) : delete x.ookOverRubbers))} />`}
    ${windows && windows !== 'no' && html`<${Toggle} label="Also over the windows" checked=${!!l.ookOverRamen} onChange=${(v) => edit((x) => (v ? (x.ookOverRamen = true) : delete x.ookOverRamen))} />`}
    ${windows === 'faint' && l.ookOverRamen && html`<p class="lak-hint">${tr('On this bus a print on the windows shows only as a tint: how see-through its glass is, a livery cannot change.')}</p>`}
    ${gradable && html`<${Toggle} label="Gradient" checked=${!!l.verloop} onChange=${(v) => edit((x) => (v ? (x.verloop = { soort: 'lineair', kleur2: '#ffffff', hoek: 0 }) : delete x.verloop))} />`}
    ${gradable && l.verloop && html`<div class="lak-row">
      <${Drop} value=${l.verloop.soort || 'lineair'} options=${[['lineair', 'Linear'], ['radiaal', 'Radial']]} onChange=${(v) => edit((x) => (x.verloop.soort = v))} />
      <${Range} label="Direction" value=${l.verloop.hoek || 0} min=${-180} max=${180} step=${1} digits=${0} unit="°" onInput=${(v) => edit((x) => (x.verloop.hoek = v), true)} onDone=${done} />
    </div>`}
    ${l.soort === 'zone' && l.straal < 999 && html`<${Range} label="Colour range" value=${l.straal} min=${2} max=${60} step=${0.5} digits=${1} onInput=${(v) => edit((x) => (x.straal = v), true)} onDone=${done} />`}
    ${l.soort === 'strook' && html`<${StripePanel} me=${me} l=${l} edit=${edit} done=${done} />`}
    ${l.soort === 'tekst' && html`<${TextPanel} me=${me} l=${l} edit=${edit} done=${done} />`}
    ${l.soort === 'vorm' && html`<label class="lak-row"><span>${tr('Shape')}</span><${Drop} value=${l.vorm} options=${me.fonts?.shapes || [[l.vorm, l.vorm]]} raw onChange=${(v) => edit((x) => (x.vorm = v))} /></label>`}
    ${l.soort === 'afbeelding' && html`
      <${Toggle} label="White becomes transparent" checked=${!!l.witDoorzichtig} onChange=${(v) => edit((x) => (x.witDoorzichtig = v))} />
      <${Btn} label="Another picture…" onClick=${async () => {
        const pic = await choosePicture(me);
        if (pic) edit((x) => ((x.beeld = pic.hash), (x.verhouding = pic.aspect)));
      }} />`}
    ${outlined && html`<${Toggle} label="Outline" checked=${!!l.omlijning} onChange=${(v) => edit((x) => (v ? (x.omlijning = { kleur: '#000000', breedteCm: 2 }) : delete x.omlijning))} />`}
    ${outlined && l.omlijning && html`<${Range} label="Width" value=${l.omlijning.breedteCm} min=${0.5} max=${20} step=${0.5} digits=${1} unit=" cm" onInput=${(v) => edit((x) => (x.omlijning.breedteCm = v), true)} onDone=${done} />`}
    ${M.isDecal(l) && html`<${PlacePanel} me=${me} l=${l} edit=${edit} done=${done} />`}
    ${l.soort === 'penseel' && html`<${BrushLayer} me=${me} l=${l} edit=${edit} />`}
  </div>`;
}

function StripePanel({ me, l, edit, done }) {
  const d = me.prep.dims;
  return html`
    <label class="lak-row"><span>${tr('Stripe')}</span><${Drop} value=${l.sjabloon} options=${STRIPES} onChange=${async (v) => {
      const k = await call('lak_stripe', { template: v, colour: l.kleur }).catch(() => null);
      if (k) edit((x) => Object.assign(x, { sjabloon: k.sjabloon, h1: k.h1, h2: k.h2, hoek: k.hoek, golf: k.golf, zijden: k.zijden, naam: STRIPES.find((s) => s[0] === v)?.[1] || x.naam }));
    }} /></label>
    <${Range} label="Lower edge" value=${Math.min(l.h1, l.h2)} min=${-0.5} max=${M.busHeight(d) + 0.2} step=${0.01} unit=" m" onInput=${(v) => edit((x) => ((x.h1 = Math.min(v, Math.max(x.h1, x.h2) - 0.02)), (x.h2 = Math.max(x.h1, x.h2))), true)} onDone=${done} />
    <${Range} label="Upper edge" value=${Math.max(l.h1, l.h2)} min=${-0.4} max=${M.busHeight(d) + 0.3} step=${0.01} unit=" m" onInput=${(v) => edit((x) => {
      const lo = Math.min(x.h1, x.h2);
      x.h1 = lo;
      x.h2 = Math.max(v, lo + 0.02);
    }, true)} onDone=${done} />
    <${Range} label="Slant" value=${l.hoek || 0} min=${-30} max=${30} step=${0.5} digits=${1} unit="°" onInput=${(v) => edit((x) => (x.hoek = v), true)} onDone=${done} />
    <${Range} label="Wave" value=${l.golf || 0} min=${0} max=${0.5} step=${0.01} unit=" m" onInput=${(v) => edit((x) => (x.golf = v), true)} onDone=${done} />
    <label class="lak-row"><span>${tr('Sides')}</span><${Drop} value=${l.zijden || 'rondom'} options=${SIDES} onChange=${(v) => edit((x) => (x.zijden = v))} /></label>`;
}

function TextPanel({ me, l, edit, done }) {
  return html`
    <label class="lak-row"><span>${tr('Text')}</span><${LiveField} raw value=${l.tekst} onInput=${(t) => edit((x) => (x.tekst = t))} /></label>
    ${me.fonts && html`<label class="lak-row"><span>${tr('Font')}</span><${Drop} raw value=${l.lettertype || me.fonts.default} options=${me.fonts.fonts.map((f) => [f, f])} onChange=${(v) => edit((x) => (x.lettertype = v))} /></label>`}
    <${Range} label="Letter height" value=${l.hoogteCm} min=${2} max=${150} step=${1} digits=${0} unit=" cm" onInput=${(v) => edit((x) => (x.hoogteCm = v), true)} onDone=${done} />
    <${Range} label="Letter spacing" value=${l.letterafstand || 0} min=${-20} max=${100} step=${1} digits=${0} unit=" %" onInput=${(v) => edit((x) => (x.letterafstand = v), true)} onDone=${done} />`;
}

function PlacePanel({ me, l, edit, done }) {
  const p = l.plaats;
  const size = M.decalSize(l, me.sizes) || [p.breedteM, p.hoogteM || p.breedteM];
  const image = M.mirrorImage(l);
  return html`
    <h3><${Icon} name="open_with" size=${18} />${tr('Look')}</h3>
    <label class="lak-row"><span>${tr('Side')}</span><${Drop} value=${p.zijde} options=${DECAL_SIDES} onChange=${(v) => edit((x) => (x.plaats.zijde = v))} /></label>
    ${l.soort !== 'tekst' && html`
      <${Num} label="Width" unit="m" value=${size[0]} onChange=${(v) => edit((x) => resize(x, v, undefined))} />
      <${Num} label="Height" unit="m" value=${size[1]} onChange=${(v) => edit((x) => resize(x, undefined, v))} />
      <${Toggle} label="Keep proportions" checked=${p.verhoudingVast !== false} onChange=${(v) => edit((x) => (v ? delete x.plaats.verhoudingVast : (x.plaats.verhoudingVast = false)))} />`}
    <${Range} label="Rotation" value=${p.draai || 0} min=${-180} max=${180} step=${1} digits=${0} unit="°" onInput=${(v) => edit((x) => (x.plaats.draai = v), true)} onDone=${done} />
    ${(p.zijde === 'L' || p.zijde === 'R') && html`
      <${Toggle} label="Also on the other side" checked=${(p.spiegel || 'gekoppeld') === 'gekoppeld'} onChange=${(v) => edit((x) => (x.plaats.spiegel = v ? 'gekoppeld' : 'los'))} />
      ${(p.spiegel || 'gekoppeld') === 'gekoppeld' && html`<${Toggle} label="Mirror image on the other side" checked=${image} onChange=${(v) => edit((x) => ((x.plaats.spiegelbeeld = v), v ? delete x.plaats.zelfdeRichting : (x.plaats.zelfdeRichting = true)))} />`}`}`;
}

/** The size fields (`Place::resize`): the other follows while the proportions are kept. */
function resize(l, width, height) {
  const p = l.plaats;
  const lim = (v) => Math.min(20, Math.max(0.02, v));
  const aspect = l.soort === 'afbeelding' ? l.verhouding ?? 0.5 : 1;
  const w0 = Math.max(1e-3, p.breedteM);
  const h0 = Math.max(1e-3, p.hoogteM ?? w0 * aspect);
  const keep = p.verhoudingVast !== false;
  if (width !== undefined) {
    const w = lim(width);
    if (keep && p.hoogteM != null) p.hoogteM = lim((h0 * w) / w0);
    else if (!keep) p.hoogteM = h0;
    p.breedteM = w;
  }
  if (height !== undefined) {
    const h = lim(height);
    if (keep) {
      p.breedteM = lim((p.breedteM * h) / h0);
      if (p.hoogteM != null) p.hoogteM = h;
    } else p.hoogteM = h;
  }
}

function BrushLayer({ me, l, edit }) {
  const n = (l.streken || []).filter((s) => !s.gum).length;
  const e = (l.streken || []).length - n;
  return html`
    <p>${tr('%{n} strokes of the brush, %{e} of the eraser.', { n, e })}</p>
    <${Btn} icon="livery_brush" label="Paint on this layer" onClick=${() => {
      me.making = 'brush';
      me.brushLayer = l.id;
      me.brush.gum = false;
      me.colour = l.kleur;
      me.tick();
    }} />
    <${Btn} kind="ghost" label="Take back the last stroke" disabled=${!(l.streken || []).length} onClick=${() => edit((x) => x.streken.pop())} />`;
}

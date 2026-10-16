// The bus step's display font (Trello "Tauri 3/6"), as our game-drawn launcher's
// (`displayfont::section`): what a font changes on the bus, the choice - "As the bus", OMSI's
// `.oft` fonts or TrueType/OpenType ones, each with the duty's destination written on the bus's
// own sign - the chosen one's sign large, and its settings: how tall its letters are, the dots
// between them, bold. "Add font…" copies a font into the content folder. The previews are drawn
// on the game's side as the game draws them (`hub_more_font_pic`); the choice is kept per bus
// and goes to the game through the Omsi-Hub plugin (`bus.set_display_font`).

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { call } from '../../api.js';
import { curTour, pickedTrip } from '../../data.js';
import { tripAt } from '../../pages/drive.js';
import { Icon, Seg, Switch, tr } from '../bits.js';
import './more.js';

/** What the previews write: the duty's first trip's line and terminus (`displayfont::sample_of`). */
export function sampleOf(c) {
  const tour = !c.free ? curTour() : null;
  const t = tour && tour.trips[pickedTrip() != null ? Math.max(0, tour.trips.findIndex((x) => x.index === pickedTrip())) : Math.max(0, tripAt(tour, c.time * 60))];
  return t && String(t.terminus || '').trim() ? `${String(t.line || '').trim()} ${String(t.terminus).trim()}`.trim() : '12 Hauptbahnhof';
}

/** What a font changes on the bus, in words (`displayfont::plan_text`). */
function planText(p) {
  const words = { destination: 'the destination', line: 'the line number', side: 'the side sign', rear: 'the rear sign' };
  const what = (p.texts || []).map((r) => tr(words[r])).join(', ');
  const parts = [];
  if (p.script) {
    parts.push(tr("This bus's matrix script (%{script}) writes its destination and line number itself, in fonts it asks the game for by name: a chosen font takes their place there, each fitted to the height, the baseline and the width of the letters it replaces, so the matrix picks its sizes as with its own fonts.", { script: p.script }));
    if (what) parts.push(tr('It also changes %{what}.', { what }));
    if (p.kept) parts.push(tr("The matrix's pictograms and clock keep their own."));
    if (p.script_pictures) parts.push(tr('Destinations its depot file gives as pictures stay pictures.'));
  } else if (p.pictures && what && !(p.texts || []).includes('destination')) parts.push(tr('This bus draws its destination itself: a font changes only %{what}.', { what }));
  else if (what) parts.push(tr('A chosen font changes %{what}.', { what }));
  else parts.push(tr('This bus draws its destination displays as pictures of its own: a display font does not change them.'));
  if (p.devices && (p.script || what)) parts.push(tr("The cab's and the saloon's displays (IBIS, printer, inside) keep their own fonts."));
  return parts.join(' ');
}

const pictures = new Map();
/** A preview as a picture the page can show (a data URL), once it is drawn. */
async function pictureOf(bus, text, key) {
  const k = `${bus}|${text}|${key}`;
  if (pictures.has(k)) return pictures.get(k);
  const bytes = await window.__TAURI__.core.invoke('call_bytes', { cmd: 'hub_more_font_pic', args: { bus, text, key } });
  const b = bytes instanceof ArrayBuffer ? new Uint8Array(bytes) : Uint8Array.from(bytes);
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const w = dv.getUint32(0, true);
  const h = dv.getUint32(4, true);
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(b.buffer, b.byteOffset + 8, w * h * 4), w, h), 0, 0);
  const p = { url: canvas.toDataURL(), w, h };
  pictures.set(k, p);
  return p;
}

/** A preview drawn (or nothing while it is not). */
function Preview({ bus, text, k, drawn, big }) {
  const [p, setP] = useState(pictures.get(`${bus}|${text}|${k}`) || null);
  useEffect(() => {
    let gone = false;
    if (drawn && !p) pictureOf(bus, text, k).then((x) => !gone && setP(x)).catch(() => {});
    return () => (gone = true);
  }, [bus, text, k, drawn]);
  if (!p) return big ? html`<span class="wait">${tr('Drawing the sign…')}</span>` : html`<span class="pic none"></span>`;
  return html`<img class=${big ? 'big' : 'pic'} src=${p.url} style=${big ? `aspect-ratio:${p.w}/${p.h}` : ''} />`;
}

/** A slider that keeps its value only when it is let go (`Ui::slider_on_release`). */
function ReleaseSlider({ label, value, min, max, format, onDone }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  const fill = ((v - min) / (max - min || 1)) * 100;
  return html`<div class="hub-slider"><span>${tr(label)}</span>
    <input type="range" min=${min} max=${max} step="1" value=${v} style=${`--fill:${fill}%`} onInput=${(e) => setV(Number(e.currentTarget.value))} onChange=${(e) => onDone(Number(e.currentTarget.value))} />
    <b>${format(v)}</b></div>`;
}

const views = new Map();

/** The display font section for `bus` with the destination `text`. */
export function DisplayFont({ bus, text }) {
  useStore();
  const [source, setSource] = useState(null);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState(null);
  const [, redraw] = useState(0);
  const key = `${bus}|${text}|${source ?? ''}`;
  const timer = useRef(0);
  const load = () =>
    call('hub_more_font_view', { bus, text, ...(source != null ? { source } : {}) })
      .then((r) => {
        views.set(key, r);
        redraw((n) => n + 1);
        // (asked again while the bus is read, or previews are still drawn)
        const waiting = r.reading || r.faces_reading || (r.options || []).some((o) => !o.drawn);
        clearTimeout(timer.current);
        if (waiting) timer.current = setTimeout(load, r.reading ? 500 : 700);
      })
      .catch(() => {});
  useEffect(() => {
    load();
    return () => clearTimeout(timer.current);
  }, [key]);
  const r = views.get(key);
  const set = (args) => call('hub_more_font_set', { bus, ...args }).then(load);
  const add = async () => {
    const a = await call('hub_more_font_add', { bus, title: tr('Add a display font'), filter: tr('Fonts (.oft, .ttf, .otf, .ttc)') }).catch((e) => ({ error: e.message }));
    if (!a) return;
    if (a.error) return setNote([a.error === 'no content folder' ? tr('There is no content folder to add fonts to: set the game under Setup.') : a.error, true]);
    let t = tr('Added: %{fonts}', { fonts: a.names.join(', ') });
    if (a.missing?.length) t += ` · ${tr('missing beside it: %{files}', { files: a.missing.join(', ') })}`;
    setNote([t, !!a.missing?.length]);
    setSource(a.vector ? 1 : 0);
    load();
  };
  let body;
  if (!r || r.reading) body = html`<p class="hub-df-wait">${tr("Reading the bus's displays…")}</p>`;
  else if (r.failed) body = html`<p class="hub-df-note">${tr("The bus's displays could not be read.")}</p>`;
  else if (!r.sign) body = html`<p class="hub-df-note">${planText(r.plan)}</p>`;
  else {
    const c = r.chosen;
    const options = r.options;
    const cur = options[r.sel] || options[0];
    const label = (o) => (o.key === '' ? tr('As the bus') : o.missing ? tr('%{font} (not installed)', { font: o.label }) : o.label);
    const pick = (o) => {
      setOpen(false);
      if (o === cur) return;
      set({ font: o.arg || null });
    };
    const s = r.sign;
    const says = !c
      ? tr('As the bus: its own font (%{font}).', { font: s.font })
      : s.fill
        ? tr("Drawn in the display's own grid of %{rows} rows of dots, in place of %{font}.", { rows: s.rows, font: s.font })
        : s.displays > 1
          ? tr("Drawn on its %{n} displays in their own grid, as tall and as wide as the letters of %{font} (capitals %{rows} dots high). Their symbols and pictograms stay the bus's own.", { n: s.displays, rows: s.caps, font: s.font })
          : tr("Drawn in the display's own grid, as tall and as wide as the letters of %{font} (capitals %{rows} dots high). Its symbols and pictograms stay the bus's own.", { rows: s.caps, font: s.font });
    const maxRows = Math.max(3, s.rows);
    body = html`<p class="hub-df-note">${planText(r.plan)}</p>
      <div class="hub-df-source"><${Seg} value=${r.source} options=${[[0, 'OMSI fonts (.oft)'], [1, 'TrueType / OpenType']]} onChange=${(v) => (setOpen(false), setSource(v))} /></div>
      ${r.faces_reading
        ? html`<p class="hub-df-wait">${tr('Reading the fonts…')}</p>`
        : html`<div class=${'hub-df-pick' + (open ? ' open' : '')}>
            <button type="button" class="field" onClick=${() => setOpen(!open)}><${Preview} bus=${bus} text=${text} k=${cur.key} drawn=${cur.drawn} /><span class="ellipsis">${label(cur)}</span><${Icon} name="expand_more" size=${18} /></button>
            ${open && html`<div class="list">${options.map((o) => html`<button type="button" class=${o === cur ? 'on' : ''} onClick=${() => pick(o)}><${Preview} bus=${bus} text=${text} k=${o.key} drawn=${o.drawn} /><span class="ellipsis">${label(o)}</span></button>`)}</div>`}
          </div>`}
      <div class="hub-df-sign"><${Preview} bus=${bus} text=${text} k=${c ? c.key : ''} drawn=${(c ? options.find((o) => o.key === c.key) : options[0])?.drawn} big=${true} /></div>
      <p class="hub-df-says">${says}</p>
      ${c && html`<div class="hub-df-style">
        <${ReleaseSlider} label="Letter height" value=${c.rows ? Math.min(Math.max(c.rows, 3), maxRows) : 2} min=${2} max=${maxRows} format=${(v) => (v < 2.5 ? tr('As the bus') : tr('%{n} dots', { n: v }))} onDone=${(v) => set({ style: { rows: v < 2.5 ? null : v, bold: c.bold, spacing: c.spacing } })} />
        <${ReleaseSlider} label="Dots between letters" value=${c.spacing ?? -1} min=${-1} max=${6} format=${(v) => (v < -0.5 ? tr('As the font') : String(v))} onDone=${(v) => set({ style: { rows: c.rows, bold: c.bold, spacing: v < -0.5 ? null : v } })} />
        <${Switch} label="Bold (every stroke a dot wider)" checked=${c.bold} onChange=${(b) => set({ style: { rows: c.rows, bold: b, spacing: c.spacing } })} />
      </div>`}`;
  }
  return html`<div class="hub-df">
    <div class="hub-bo-head"><b>${tr('Display font')}</b>
      <button type="button" class="hub-bo-reset" title=${tr("Copy a font into Omsi-Hub's content folder: an OMSI .oft with its bitmaps, or a TrueType / OpenType font (.ttf, .otf, .ttc)")} onClick=${add}><${Icon} name="add" size=${16} />${tr('Add font…')}</button></div>
    ${body}
    ${note && html`<p class=${'hub-df-note ' + (note[1] ? 'warn' : 'ok')}>${note[0]}</p>`}
  </div>
  <div class="hub-bo-rule"></div>`;
}

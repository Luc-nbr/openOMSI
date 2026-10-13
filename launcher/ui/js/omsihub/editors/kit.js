// The controls Omsi-Hub's editors are built of, after our game-drawn launcher's ui kit
// (`crates/omsi-launcher-ui/src/ui.rs`): a card (a hairline round a section, no fill), its
// heading in capitals, the rows of a list (the chosen one in the accent), a dropdown that opens
// its list under the field (typing finds an option), a text field with its icon, a round icon
// button, a switch, a small tag, and the status line along the bottom of the window. Every
// text passed in is English and translated here (`raw` for names and figures).

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { Icon as BaseIcon, tr } from '../bits.js';
import { SYMBOLS as OURS } from '../symbols.js';
import { SYMBOLS as EDITOR, STROKED } from './symbols.js';

// (the editors' icons join our look's, so that every page draws them by their names)
for (const [k, v] of Object.entries(EDITOR)) if (!OURS[k]) OURS[k] = v;

// (a script error goes into the launcher's log: the window has no console to say it in)
const logError = (text) => import('../../api.js').then((a) => a.call('hub_page_error', { text: String(text).slice(0, 2000) })).catch(() => {});
addEventListener('error', (e) => logError(`${e.message} (${e.filename}:${e.lineno}:${e.colno})`));
addEventListener('unhandledrejection', (e) => logError(e.reason?.stack || e.reason));

export { html, tr };

/** An icon by its Material name, or one of the game's own stroked ones (the livery studio's
 * tools). */
export function Icon({ name, size = 20, class: cls = '' }) {
  const s = STROKED[name];
  if (!s) return html`<${BaseIcon} name=${name} size=${size} class=${cls} />`;
  return html`<svg class=${'hub-icon ' + cls} viewBox="0 0 24 24" width=${size} height=${size} fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${s.map((d) => html`<path d=${d} />`)}</svg>`;
}

/** What an editor page keeps between its draws and visits (`S.ed[name]`, made at first). */
export function keep(name, init) {
  S.ed = S.ed || {};
  if (!S.ed[name]) S.ed[name] = init;
  return S.ed[name];
}

/** A section of a sheet: a hairline round it, no fill of its own. */
export function Card({ class: cls = '', style = '', children }) {
  return html`<section class=${'ed-card ' + cls} style=${style}>${children}</section>`;
}

/** A section's heading inside a card: its title in capitals (`Ui::heading`). */
export function Heading({ title, raw, children }) {
  return html`<div class="ed-heading"><span>${raw ? title : tr(title)}</span>${children}</div>`;
}

/** A row of a list: under the mouse lit, the chosen one in the accent (`Ui::row`). */
export function Row({ on, onClick, class: cls = '', style = '', children, title }) {
  return html`<button type="button" class=${'ed-row' + (on ? ' on' : '') + (cls ? ' ' + cls : '')} style=${style} title=${title} onClick=${onClick}>${children}</button>`;
}

/** A small coloured tag (`Ui::badge`): its words in capitals. */
export const Badge = ({ text, raw, kind = 'accent' }) => html`<span class=${'ed-badge ' + kind}>${(raw ? text : tr(text)).toUpperCase()}</span>`;

/** A text field (`Ui::text_input`): its icon on the left, red round it when `bad`. Enter and
 * Escape leave it, as the game-drawn launcher's do. */
export function Field({ value, onInput, placeholder = '', icon, bad, class: cls = '', style = '', raw, onEnter, mono, title, id }) {
  return html`<label class=${'ed-field' + (bad ? ' bad' : '') + (icon ? ' with-icon' : '') + (cls ? ' ' + cls : '')} style=${style} title=${title ? tr(title) : undefined}>
    ${icon && html`<${Icon} name=${icon} size=${18} />`}
    <input id=${id} class=${mono ? 'mono' : ''} value=${value ?? ''} placeholder=${raw ? placeholder : tr(placeholder)} spellcheck="false"
      onInput=${(e) => onInput(e.currentTarget.value)}
      onKeyDown=${(e) => {
        if (e.key === 'Enter') {
          onEnter?.();
          e.currentTarget.blur();
        } else if (e.key === 'Escape') e.currentTarget.blur();
      }} />
  </label>`;
}

/** A text field whose text is the page's own while it is typed into (`Field` otherwise): the
 * answers of a request made per key may come after the next key, and would put back an older
 * text. Taken from `value` again once the field is left. */
export function LiveField(props) {
  const [text, setText] = useState(props.value ?? '');
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(props.value ?? '');
  }, [props.value]);
  return html`<span style=${props.wrap || 'display:contents'} onFocusCapture=${() => (focused.current = true)} onBlurCapture=${() => {
      focused.current = false;
      setText(props.value ?? '');
    }}><${Field} ...${props} value=${text} onInput=${(t) => (setText(t), props.onInput(t))} /></span>`;
}

/** A round button with only an icon (`Ui::icon_button`): `size` its radius. */
export function IconBtn({ icon, title, onClick, size = 14, class: cls = '', disabled }) {
  return html`<button type="button" class=${'ed-iconbtn ' + cls} style=${`width:${size * 2}px;height:${size * 2}px`} title=${title ? tr(title) : undefined} disabled=${disabled}
    onClick=${(e) => (e.stopPropagation(), onClick?.(e))}><${Icon} name=${icon} size=${Math.round(size * 1.1)} /></button>`;
}

/** A switch (`Ui::toggle`): its words on the left, the track on the right. */
export function Toggle({ checked, onChange, label, raw, title, class: cls = '' }) {
  return html`<label class=${'ed-toggle ' + cls} title=${title ? tr(title) : undefined}><span>${raw ? label : tr(label)}</span>
    <input type="checkbox" checked=${!!checked} onChange=${(e) => onChange(e.currentTarget.checked)} /><i></i></label>`;
}

/** A time of day (`Ui::time_field`): the hours and the minutes, each with its arrows and the
 * wheel - an hour, five minutes a step. `minutes` from midnight; `end`: a day's end (24:00 and
 * on) shows as 00:00. `onChange` gets the minutes the field shows (0 - 1439). */
export function TimeSpin({ minutes, onChange, end, h }) {
  const m = Math.round(minutes);
  const shown = end && m >= 1440 ? 0 : ((m % 1440) + 1440) % 1440;
  const to = (by) => onChange((((shown + by) % 1440) + 1440) % 1440);
  const cell = (v, step) => html`<div class="cell" onWheel=${(e) => (e.preventDefault(), to(e.deltaY < 0 ? step : -step))}>
      <b>${String(v).padStart(2, '0')}</b>
      <span class="arrows"><button type="button" tabindex="-1" onClick=${() => to(step)}><${Icon} name="expand_less" size=${16} /></button><button type="button" tabindex="-1" onClick=${() => to(-step)}><${Icon} name="expand_more" size=${16} /></button></span>
    </div>`;
  return html`<div class="ed-time" style=${h ? `height:${h}px` : ''}>${cell(Math.floor(shown / 60), 60)}<i>:</i>${cell(shown % 60, 5)}</div>`;
}

// --- a colour of one's own -------------------------------------------------------------------

/** Hue (0 - 360), saturation and value (0 - 1) of `#rrggbb`, and back (`colour::hsv`). */
function hsvOf(hex) {
  const n = parseInt(String(hex).replace('#', '').slice(0, 6), 16) || 0;
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  const h = d === 0 ? 0 : max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, max === 0 ? 0 : d / max, max];
}
function hexOfHsv([h, s, v]) {
  const f = (n) => {
    const k = (n + h / 60) % 6;
    return Math.round(255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1))));
  };
  return '#' + [f(5), f(3), f(1)].map((x) => x.toString(16).padStart(2, '0')).join('');
}

/** The picker under a palette (`accent_pick::picker`): saturation and value in a square of the
 * hue, the hues in a bar under it, the colour's code to type and a sample. `onChange` gets
 * `#rrggbb`; the hue stays where it was while the colour is a grey. */
export function ColourPicker({ value, onChange, ink = '#fff' }) {
  const st = useRef({ hsv: hsvOf(value), shown: value, hex: value });
  const [, redraw] = useState(0);
  const s = st.current;
  if (s.shown?.toLowerCase() !== value?.toLowerCase()) {
    const hsv = hsvOf(value);
    s.hsv = hsv[1] < 1e-3 || hsv[2] < 1e-3 ? [s.hsv[0], hsv[1], hsv[2]] : hsv;
    s.hex = value;
    s.shown = value;
  }
  const take = (hsv) => {
    s.hsv = hsv;
    const hex = hexOfHsv(hsv);
    s.hex = hex;
    s.shown = hex;
    redraw((n) => n + 1);
    onChange(hex);
  };
  // (held and dragged: the square or the bar follows the mouse)
  const drag = (fn) => (e) => {
    const el = e.currentTarget;
    el.setPointerCapture?.(e.pointerId);
    const at = (ev) => {
      const r = el.getBoundingClientRect();
      fn(Math.max(0, Math.min(1, (ev.clientX - r.left) / r.width)), Math.max(0, Math.min(1, (ev.clientY - r.top) / r.height)));
    };
    at(e);
    const move = (ev) => at(ev);
    const up = () => (el.removeEventListener('pointermove', move), el.removeEventListener('pointerup', up));
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  };
  const [h, sat, val] = s.hsv;
  const hue = hexOfHsv([h, 1, 1]);
  return html`<div class="ed-picker">
    <div class="sq" style=${`background:linear-gradient(to bottom, transparent, #000), linear-gradient(to right, #fff, transparent), ${hue}`}
      onPointerDown=${drag((x, y) => take([s.hsv[0], x, 1 - y]))}>
      <i class="knob" style=${`left:${sat * 100}%;top:${(1 - val) * 100}%;background:${value}`}></i>
    </div>
    <div class="hues" onPointerDown=${drag((x) => {
      const [, s0, v0] = s.hsv;
      // (a grey has no hue to turn: it is given some colour)
      const grey = s0 < 0.05 || v0 < 0.05;
      take([Math.min(x, 0.9999) * 360, grey ? Math.max(s0, 0.8) : s0, grey ? Math.max(v0, 0.85) : v0]);
    })}><i style=${`left:${(h / 360) * 100}%`}></i></div>
    <div class="row">
      <${Field} raw value=${s.hex} placeholder="#f58620" onInput=${(t) => {
        s.hex = t;
        const code = t.trim().replace(/^#/, '');
        if (/^[0-9a-f]{6}$/i.test(code)) {
          s.hsv = hsvOf('#' + code);
          s.shown = '#' + code.toLowerCase();
          onChange('#' + code.toLowerCase());
        }
        redraw((n) => n + 1);
      }} />
      <span class="sample" style=${`background:${value};color:${ink}`}>Aa</span>
    </div>
  </div>`;
}

/** A button (`Ui::button`): `kind` '' (a control), 'primary' (the accent), 'ghost', 'danger'. */
export function Btn({ kind = '', icon, label, raw, onClick, disabled, title, class: cls = '', style = '' }) {
  const c = ['hub-btn', 'ed-btn', kind, cls].filter(Boolean).join(' ');
  return html`<button type="button" class=${c} style=${style} onClick=${onClick} disabled=${disabled} title=${title ? tr(title) : undefined}>
    ${icon && html`<${Icon} name=${icon} size=${17} />`}${label != null && label !== '' && html`<span>${raw ? label : tr(label)}</span>`}
  </button>`;
}

// --- the dropdown -------------------------------------------------------------------------

let openDrop = null;

/** A dropdown (`Ui::select`): the field shows the option chosen; a click opens the list under
 * it - the chosen one in the accent with its tick, typing narrows it, Enter takes the first,
 * Escape or a click beside it closes it. `options` are [value, label]. */
export function Drop({ value, options, onChange, raw, placeholder = '', class: cls = '', style = '', disabled, render }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const ref = useRef();
  const listRef = useRef();
  const at = options.findIndex(([v]) => v === value);
  const label = (o) => (raw ? o[1] : tr(o[1]));
  const shown = query ? options.filter((o) => String(label(o)).toLowerCase().includes(query.toLowerCase())) : options;
  useEffect(() => {
    if (!open) return;
    const close = () => {
      setOpen(false);
      setQuery('');
    };
    openDrop = close;
    const down = (e) => !ref.current?.contains(e.target) && !listRef.current?.contains(e.target) && close();
    const key = (e) => {
      if (e.key === 'Escape') close();
      else if (e.key === 'Enter') {
        if (shown[0]) onChange(shown[0][0]);
        close();
      } else if (e.key === 'Backspace') setQuery((q) => q.slice(0, -1));
      else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) setQuery((q) => q + e.key);
      else return;
      e.preventDefault();
    };
    addEventListener('mousedown', down, true);
    addEventListener('keydown', key, true);
    return () => {
      removeEventListener('mousedown', down, true);
      removeEventListener('keydown', key, true);
      if (openDrop === close) openDrop = null;
    };
  }, [open, query, options]);
  useEffect(() => {
    // (the chosen option in view as the list opens)
    if (open && listRef.current) listRef.current.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
  }, [open]);
  const box = open && ref.current ? place(ref.current) : null;
  return html`<span class=${'ed-drop' + (open ? ' open' : '') + (cls ? ' ' + cls : '')} style=${style} ref=${ref}>
    <button type="button" class="face" disabled=${disabled} onClick=${() => (open ? setOpen(false) : (openDrop?.(), setOpen(true)))}>
      <span class="text">${at >= 0 ? (render ? render(options[at], false) : label(options[at])) : html`<em>${raw ? placeholder : tr(placeholder)}</em>`}</span>
      <${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${20} />
    </button>
    ${box && html`<div class="ed-drop-list" ref=${listRef} style=${box}>
      ${query && html`<div class="ed-drop-query"><${Icon} name="search" size=${16} /><span>${query}</span></div>`}
      ${query && !shown.length && html`<div class="ed-drop-none">${tr('Nothing found')}</div>`}
      ${shown.map((o) => html`<button type="button" class=${o[0] === value ? 'on' : ''} onClick=${() => (onChange(o[0]), setOpen(false), setQuery(''))}>
        <span class="text">${render ? render(o, o[0] === value) : label(o)}</span>${o[0] === value && html`<${Icon} name="check" size=${15} />`}
      </button>`)}
    </div>`}
  </span>`;
}

/** Where a dropdown's list goes (`popup_rect`): under the field, as wide, as long as the room
 * under it allows (over it when there is more room there), in the stage's own units. */
function place(el) {
  const stage = el.closest('.hub-stage');
  const k = stage ? stage.getBoundingClientRect().width / stage.offsetWidth || 1 : 1;
  const r = el.getBoundingClientRect();
  const s = stage ? stage.getBoundingClientRect() : { left: 0, top: 0, height: innerHeight };
  const top = (r.bottom - s.top) / k + 4;
  const height = s.height / k;
  const below = height - top - 12;
  const above = (r.top - s.top) / k - 16;
  const fixed = `position:fixed;left:${(r.left - s.left) / k}px;width:${Math.max(160, r.width / k)}px`;
  if (below < 160 && above > below) return `${fixed};bottom:${height - (r.top - s.top) / k + 4}px;max-height:${Math.min(360, above)}px`;
  return `${fixed};top:${top}px;max-height:${Math.max(120, Math.min(360, below))}px`;
}

// --- the status line ------------------------------------------------------------------------

/** A word along the bottom of the window (`State::set_status`): an error stays, the rest goes
 * after six seconds. */
export function status(text, err = false) {
  if (!text) return;
  set({ hubStatus: { text: tr(text), err: !!err, at: Date.now() } });
}

/** The status line (`Launcher::status_bar`): a dark pill in the bottom left corner; a click
 * puts it away. */
export function StatusLine() {
  const s = useStore();
  const st = s.hubStatus;
  const [, tick] = useState(0);
  useEffect(() => {
    if (!st || st.err) return;
    const t = setTimeout(() => tick((n) => n + 1), 7600 - (Date.now() - st.at));
    return () => clearTimeout(t);
  }, [st]);
  if (!st) return null;
  const age = (Date.now() - st.at) / 1000;
  if (!st.err && age > 7.5) return null;
  const first = st.text.split('\n')[0];
  return html`<button type="button" class=${'ed-status' + (st.err ? ' err' : '') + (!st.err && age > 6 ? ' going' : '')} title=${st.text} onClick=${() => set({ hubStatus: null })}>${first}</button>`;
}

// --- times ----------------------------------------------------------------------------------

/** "h:mm" (or "h:mm:ss") from minutes after midnight (`timetable::fmt_time`). */
export function fmtTime(min) {
  const s = Math.round(min * 60);
  const h = Math.trunc(s / 3600);
  const m = Math.trunc(s / 60) % 60;
  const sec = s % 60;
  return sec === 0 ? `${h}:${String(m).padStart(2, '0')}` : `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
}

/** Minutes after midnight from "h:mm", "h:mm:ss" or "h.mm" (after midnight as 24:10 and on);
 * null when it is none (`timetable::parse_time`). */
export function parseTime(t) {
  const parts = String(t).trim().split(/[:.]/);
  if (parts.length < 2 || parts.length > 3) return null;
  const num = (x) => (/^\s*\d+\s*$/.test(x) ? Number(x) : NaN);
  const h = num(parts[0]);
  const m = num(parts[1]);
  const s = parts.length > 2 ? num(parts[2]) : 0;
  if ([h, m, s].some(Number.isNaN)) return null;
  return h < 48 && m < 60 && s < 60 ? h * 60 + m + s / 60 : null;
}

/** "one tour" / "%{n} tours" and the like: `one` for one, `many` with `%{n}` else. */
export const countOf = (n, one, many) => (n === 1 ? tr(one) : tr(many, { n }));

/** The Drive page's line lists read again (`State::load_lines`): an editor changed them. */
export function reloadLines() {
  S.lines = { ...S.lines, key: '' };
  import('../../data.js').then((d) => d.loadLines());
}

/** A label of the map list: its friendly name, else its name. */
export const mapLabel = (m) => (m?.friendly || '').trim() || (m?.name || '').trim() || m?.file || '';

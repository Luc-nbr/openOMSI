// The pieces Omsi-Hub's steps and pages are built of, after our game-drawn launcher's ui kit
// (`crates/omsi-launcher-ui/src/ui.rs`, `flow.rs`): icons by their Material names, a sheet's
// head and foot, the action row, chips, segments, switches and sliders. Every text passed in
// is English and translated here, as upstream's controls do.

import { html } from '../../vendor/preact-htm.js';
import { t } from '../i18n.js';
import { SYMBOLS as OURS } from './symbols.js';
import { SYMBOLS as THEIRS } from '../symbols.js';
// (phase 3: every symbol the game-drawn launcher has, as the company's pages have them - the
// roadbook's, the welcome's and the rest the steps' own set lacks)
import { SYMBOLS as ALL } from './company/symbols.js';

export { html };

/** A text of our game-drawn launcher's tables, with its `%{name}` holes filled after the lookup
 * (upstream's `t` fills `{name}`). */
export function tr(text, vars) {
  let s = t(text);
  if (vars) s = s.replace(/%\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  return s;
}

/** A Material Symbol by its own name (ours), or by upstream's name; a name none has leaves its
 * room empty, as our game-drawn launcher's atlas does (`Ui::icon`). */
export function Icon({ name, size = 20, class: cls = '' }) {
  const d = OURS[name] || ALL[name] || THEIRS[name] || '';
  return html`<svg class=${'hub-icon ' + cls} viewBox="0 -960 960 960" width=${size} height=${size} fill="currentColor" aria-hidden="true"><path d=${d} /></svg>`;
}

/** A sheet's head: its icon, its title and the line under it, tools on the right. */
export function Head({ icon, title, sub, tools, raw }) {
  return html`<header class="hub-head">
    <${Icon} name=${icon} size=${22} />
    <h2>${raw ? title : tr(title)}</h2>
    ${tools && html`<div class="tools">${tools}</div>`}
    <p>${sub || ''}</p>
  </header>`;
}

/** A sheet's foot: a hairline and a quiet line with an info mark, and what goes beside it. */
export function Foot({ text, children }) {
  return html`<footer class="hub-foot"><${Icon} name="info" size=${14} /><span class="grow">${text}</span>${children}</footer>`;
}

/** A section's name in capitals, and what it holds now on the right. */
export function Caps({ title, value }) {
  return html`<div class="hub-caps"><b>${tr(title)}</b>${value != null && html`<span>${value}</span>`}</div>`;
}

export function Button({ kind = '', icon, label, raw, onClick, disabled, title, small, wide, class: cls = '' }) {
  const c = ['hub-btn', kind, small && 'small', wide && 'wide', cls].filter(Boolean).join(' ');
  return html`<button type="button" class=${c} onClick=${onClick} disabled=${disabled} title=${title ? tr(title) : undefined}>
    ${icon && html`<${Icon} name=${icon} size=${small ? 16 : 18} />`}${label && (raw ? label : tr(label))}
  </button>`;
}

/** The action row: back on the left (from `from`), the extra actions and the main one in the
 * bottom right corner, the same place on every step. */
export function Actions({ back, onBack, from, extra = [], main, mainIcon = 'play_arrow', onMain, mainDisabled, busy }) {
  return html`<div class="hub-actions" style=${from ? `left:${from}` : ''}>
    ${back && html`<button type="button" class="hub-btn hub-back" onClick=${onBack}><${Icon} name="chevron_left" size=${18} />${tr('Back')}</button>`}
    <span class="grow"></span>
    ${extra.map((x) => html`<button type="button" class="hub-btn" onClick=${x.onClick} disabled=${x.disabled}>${x.icon && html`<${Icon} name=${x.icon} size=${18} />`}${tr(x.label)}</button>`)}
    ${main && html`<button type="button" class="hub-btn hub-go" data-main="1" onClick=${onMain} disabled=${mainDisabled || busy}><${Icon} name=${mainIcon} size=${22} />${tr(main)}</button>`}
  </div>`;
}

/** Chips: [value, label] each in a pill of its own, the chosen one in the accent. */
export function Chips({ value, options, onChange, raw }) {
  return html`<div class="hub-chips">${options.map(([v, l]) => html`<button type="button" class=${'hub-chip' + (v === value ? ' on' : '')} onClick=${() => v !== value && onChange(v)}>${raw ? l : tr(l)}</button>`)}</div>`;
}

/** Segments side by side, one of them chosen. */
export function Seg({ value, options, onChange, full, quiet }) {
  return html`<div class=${'hub-seg' + (full ? ' full' : '') + (quiet ? ' quiet' : '')}>${options.map(([v, l, icon]) => html`<button type="button" class=${v === value ? 'on' : ''} onClick=${() => onChange(v)}>${icon && html`<${Icon} name=${icon} size=${15} />`}${tr(l)}</button>`)}</div>`;
}

/** A switch: its words on the left, the track on the right. */
export function Switch({ checked, onChange, label, raw, title }) {
  return html`<label class="hub-switch" title=${title ? tr(title) : undefined}><span>${raw ? label : tr(label)}</span><input type="checkbox" checked=${!!checked} onChange=${(e) => onChange(e.currentTarget.checked)} /><i></i></label>`;
}

/** A slider: its name, the line, its value. */
export function Slider({ label, value, min, max, step = 1, onChange, format }) {
  const v = Number(value ?? min);
  const fill = ((v - min) / (max - min || 1)) * 100;
  return html`<div class="hub-slider"><span>${tr(label)}</span>
    <input type="range" min=${min} max=${max} step=${step} value=${v} style=${`--fill:${fill}%`} onInput=${(e) => onChange(Number(e.currentTarget.value))} />
    <b>${format ? format(v) : v}</b></div>`;
}

/** The round mark of a row. */
export const Radio = ({ on }) => html`<span class=${'hub-radio' + (on ? ' on' : '')}></span>`;

/** The line's plate. */
export const Plate = ({ line }) => html`<span class="hub-plate">${line}</span>`;

/** "2h 05m": a length as our launcher writes it. */
export function length(seconds) {
  const m = Math.round(Math.max(0, seconds) / 60);
  return tr('%{h}h %{mm}m', { h: Math.floor(m / 60), mm: String(m % 60).padStart(2, '0') });
}

/** Hours as the service record writes them ("12h 05m"). */
export function hoursText(h) {
  const m = Math.max(0, Math.round((h || 0) * 60));
  return tr('%{h}h %{m}m', { h: Math.floor(m / 60), m: String(m % 60).padStart(2, '0') });
}

/** A count with its noun, each in the singular where it is one ("%{n} trip", "%{n} trips"). */
export const count = (n, one, many) => tr(n === 1 ? one : many, { n });

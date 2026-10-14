// RosterFlow's small controls as the page draws them (`rosterflow.rs`'s `rf_button`, `rf_icon`,
// `tool_link`, `caps`; `rf_panels.rs`'s `switch`, `check_row`, `seg`, `stepper`, `menu_item`):
// in RosterFlow's palette (rosterflow.css), every text English and translated here.

import { html } from '../../../vendor/preact-htm.js';
import { tr } from '../bits.js';
import { Icon as CoIcon } from '../company/kit.js';
import { STROKED } from './draw.js';

/** An icon by its Material name, or one of our own drawn as strokes (`STROKED`). */
export function Icon({ name, size = 20, class: cls = '' }) {
  const st = STROKED[name];
  if (st) {
    return html`<svg class=${'hub-icon ' + cls} viewBox="0 0 24 24" width=${size} height=${size} fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${st.map((d) => html`<path d=${d} />`)}</svg>`;
  }
  return html`<${CoIcon} name=${name} size=${size} class=${cls} />`;
}

/** A button of the page (`rf_button`): solid blue, else grey; its words in capitals. */
export function RfButton({ label, solid, disabled, onClick, style = '', title, raw, data }) {
  return html`<button type="button" class=${'rf-btn' + (solid ? ' solid' : '')} style=${style} disabled=${disabled} title=${title ? tr(title) : undefined} data-rf=${data} onClick=${(e) => !disabled && onClick?.(e)}>${raw ? label : tr(label)}</button>`;
}

/** An icon that is a button (`rf_icon`); `on` marks it. */
export function IconBtn({ icon, title, on, disabled, onClick, size = 19, data, class: cls = '' }) {
  return html`<button type="button" class=${'rf-ibtn' + (on ? ' on' : '') + (cls ? ' ' + cls : '')} disabled=${disabled} title=${title ? tr(title) : undefined} data-rf=${data} onClick=${(e) => !disabled && onClick?.(e)}><${Icon} name=${icon} size=${size} /></button>`;
}

/** A word with an icon before it (`tool_link`), as the toolbar's Stats, Layers and Filter. */
export function ToolLink({ icon, label, on, onClick }) {
  return html`<button type="button" class=${'rf-link' + (on ? ' on' : '')} onClick=${onClick}><${Icon} name=${icon} size=${18} /><span>${tr(label)}</span></button>`;
}

/** A small caps label. */
export const Caps = ({ text, raw, class: cls = '', style = '' }) => html`<div class=${'rf-caps ' + cls} style=${style}>${(raw ? text : tr(text)).toUpperCase()}</div>`;

/** A switch with its words on the left (`rf_panels::switch`). */
export function Switch({ on, label, raw, onChange, style = '' }) {
  return html`<button type="button" class=${'rf-switch' + (on ? ' on' : '')} style=${style} onClick=${() => onChange(!on)}><span>${raw ? label : tr(label)}</span><i></i></button>`;
}

/** A checkbox with its words after it (`rf_panels::check_row`). */
export function CheckRow({ on, label, raw, onChange, style = '', class: cls = '' }) {
  return html`<button type="button" class=${'rf-check' + (on ? ' on' : '') + (cls ? ' ' + cls : '')} style=${style} onClick=${() => onChange(!on)}><b>${on && html`<${Icon} name="check" size=${12} />`}</b><span>${raw ? label : tr(label)}</span></button>`;
}

/** A row of choices, one on (`rf_panels::seg`). */
export function Seg({ at, labels, raw, onPick, style = '' }) {
  return html`<div class="rf-seg" style=${style}>${labels.map((l, k) => html`<button type="button" class=${k === at ? 'on' : ''} onClick=${() => k !== at && onPick(k)}>${(raw ? l : tr(l)).toUpperCase()}</button>`)}</div>`;
}

/** A value with minus and plus beside it (`rf_panels::stepper`); the wheel over it turns it. */
export function Stepper({ value, step = 1, min = 0, max = 1e9, fmt = String, onChange, style = '' }) {
  const set = (v) => {
    const n = Math.max(min, Math.min(max, v));
    if (n !== value) onChange(n);
  };
  const wheel = (e) => {
    e.preventDefault();
    e.stopPropagation();
    set(value + Math.sign(-e.deltaY) * step);
  };
  return html`<span class="rf-stepper" style=${style} onWheel=${wheel}>
    <button type="button" onClick=${() => set(value - step)}><${Icon} name="remove" size=${15} /></button>
    <b>${fmt(value)}</b>
    <button type="button" onClick=${() => set(value + step)}><${Icon} name="add" size=${15} /></button>
  </span>`;
}

/** A line of a menu (`rf_panels::menu_item`). */
export function MenuItem({ label, raw, disabled, title, onClick, on }) {
  return html`<button type="button" class=${'rf-mi' + (on ? ' on' : '')} disabled=${disabled} title=${title ? tr(title) : undefined} onClick=${onClick}>${raw ? label : tr(label)}</button>`;
}

/** A select in RosterFlow's look. */
export function Select({ value, options, onChange, style = '', raw = true }) {
  return html`<span class="rf-select" style=${style}>
    <select value=${String(value)} onChange=${(e) => onChange(Number(e.currentTarget.value))}>
      ${options.map((o, k) => html`<option value=${String(k)}>${raw ? o : tr(o)}</option>`)}
    </select><${Icon} name="expand_more" size=${18} />
  </span>`;
}

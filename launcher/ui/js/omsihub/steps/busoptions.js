// The bus step's bus options (Trello "Tauri 3/6"), as our game-drawn launcher's
// (`busoptions::section`): the `[setvar]` variables of the chosen bus's liveries - mirrors,
// rims, seats, door types - each with "Livery / Off / On" (a switch) or a list of the values the
// liveries give it; "As the livery" leaves it to the livery chosen. The appearance ones first,
// the technical ones (a script reads them) behind a fold. A choice is kept for the bus file
// (`hub_more_bus_option_set`) and goes to the game as `--setvar` (`hub_more_setvars`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { call } from '../../api.js';
import { Icon, tr } from '../bits.js';
import { Select } from '../../ui.js';
import './more.js';

/** A value as `--setvar` takes it: whole numbers without a decimal point. */
const valueText = (v) => (Number.isInteger(v) ? String(v) : String(Math.round(v * 1e6) / 1e6));
const same = (a, b) => Math.abs(a - b) < 1e-4;

/** What "As the livery" comes to, in words: its value, or the usual one (`livery_words`). */
function liveryWords(o) {
  const word = (v) => (o.switch ? tr(v === 0 ? 'Off' : 'On') : valueText(v));
  return o.livery != null ? tr('As the livery (%{value})', { value: word(o.livery) }) : tr('As the livery (usually %{value})', { value: word(o.usual) });
}

/** A few livery names: two, and how many more. */
const liveriesText = (n) => (n.length <= 2 ? n.join(', ') : `${n[0]}, ${n[1]} +${n.length - 2}`);

/** The values to choose from: those the liveries give, and one chosen that none gives. */
function choices(o) {
  const v = [...o.values];
  if (o.chosen != null && !v.some((s) => same(s.value, o.chosen))) {
    v.push({ value: o.chosen, liveries: [] });
    v.sort((a, b) => a.value - b.value);
  }
  return v;
}

const cache = new Map();

/** The section, for `bus` in livery `paint` (nothing for a bus without options). */
export function BusOptions({ bus, paint }) {
  useStore();
  const key = `${bus}|${paint || ''}`;
  const [, redraw] = useState(0);
  const [open, setOpen] = useState(false);
  const load = () =>
    call('hub_more_bus_options', { bus, paint: paint || '' })
      .then((r) => {
        cache.set(key, r);
        redraw((k) => k + 1);
        // (the bus is read on the game's side the first time: asked again until it is)
        if (r.reading) setTimeout(load, 500);
      })
      .catch(() => cache.set(key, { options: [] }));
  useEffect(() => {
    if (!cache.has(key) || cache.get(key).reading) load();
  }, [key]);
  const r = cache.get(key);
  if (!r?.options?.length) return null;
  const edit = (args) => call('hub_more_bus_option_set', { bus, ...args }).then(load);
  const looks = r.options.filter((o) => !o.technical);
  const technical = r.options.filter((o) => o.technical);
  const changed = r.options.filter((o) => o.chosen != null).length;
  const nTech = technical.filter((o) => o.chosen != null).length;
  const row = (o) => html`<div class=${'hub-bo-row' + (o.chosen != null ? ' on' : '')} title=${`${o.label}\n${o.var}\n${liveryWords(o)}`}>
    <span class="name"><b class="ellipsis">${o.label}</b><small class="ellipsis">${o.var}</small></span>
    ${o.switch ? html`<${Switch3} o=${o} onPick=${(v) => edit({ var: o.var, value: v })} />` : html`<${ValueList} o=${o} onPick=${(v) => edit({ var: o.var, value: v })} />`}
  </div>`;
  return html`<div class="hub-bo">
    <div class="hub-bo-head"><b>${tr('Bus options')}</b>
      ${changed > 0 && html`<button type="button" class="hub-bo-reset" title=${tr('Every option back as the livery has it')} onClick=${() => edit({ reset: true })}><${Icon} name="restart_alt" size=${16} />${tr('Reset')}</button>`}</div>
    ${looks.map(row)}
    ${technical.length > 0 && html`<button type="button" class="hub-bo-fold" style=${looks.length ? 'margin-top:6px' : ''} onClick=${() => setOpen(!open)}>
        <${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${18} /><span>${tr('Technical')}</span><em class=${nTech ? 'on' : ''}>${nTech ? `${nTech} / ${technical.length}` : technical.length}</em></button>
      ${open && html`<p class="hub-bo-note">${tr('These change how the bus works: its scripts read them.')}</p>${technical.map(row)}`}`}
  </div>
  <div class="hub-bo-rule"></div>`;
}

/** Three choices side by side - the livery's, off, on - each as wide as its word: the chosen
 * one's pill quiet while it is the livery's, in the accent when the player chose; a dot under
 * the one the livery gives (fainter: what most liveries give) (`switch3`). */
function Switch3({ o, onPick }) {
  const sel = o.chosen == null ? 0 : o.chosen === 0 ? 1 : 2;
  const mark = o.livery != null ? [o.livery === 0 ? 1 : 2, true] : [o.usual === 0 ? 1 : 2, false];
  return html`<span class="hub-bo-switch" title=${liveryWords(o)}>
    ${['Livery', 'Off', 'On'].map((w, i) => html`<button type="button" class=${(sel === i ? 'on' : '') + (sel === i && i > 0 ? ' mine' : '')} onClick=${() => sel !== i && onPick([null, 0, 1][i])}>
      ${tr(w)}${mark[0] === i && html`<i class=${mark[1] ? 'strong' : ''}></i>`}</button>`)}
  </span>`;
}

/** The values as a list: "As the livery (…)", then each value with the liveries giving it. */
function ValueList({ o, onPick }) {
  const values = choices(o);
  const at = o.chosen != null ? values.findIndex((s) => same(s.value, o.chosen)) : -1;
  const options = [['-1', liveryWords(o)], ...values.map((s, i) => [String(i), s.liveries.length ? tr('%{value} – as %{liveries}', { value: valueText(s.value), liveries: liveriesText(s.liveries) }) : valueText(s.value)])];
  return html`<span class="hub-bo-list"><${Select} raw value=${String(at)} options=${options} onChange=${(k) => onPick(Number(k) < 0 ? null : values[Number(k)].value)} /></span>`;
}

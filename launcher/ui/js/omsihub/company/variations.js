// "Request a change" of a tendered line's bus contract (`company_more/variations.rs`; Luc
// 2026-10-16: a won route changed for a fee, or a payment, by the size of the change): the kinds
// of change with their controls, what the change does to the kilometres, hours and stops, its
// class, the fee or the penalty, the payment a week before and after, the default points and the
// reputation it costs - then agreed, or an offer made instead. Opened from the bus contracts
// (Goals & contracts) and from the Concessions page; the contract's variations below.

import { html, ui, setUi, Icon, Btn, Plate, Slider, Switch, Dialog, tr } from './kit.js';
import { useMore, actMore, eur } from './more.js';
import { units, cents } from '../money.js';
import { Chip } from './pages-a.js';

const START = { line: null, ask: null, amount: null };

/** The dialog opened on a line's contract. */
export const openVary = (line) => setUi('vary', { line, ask: { kind: 'timetable' }, amount: null });

const close = () => setUi('vary', START);

/** The dialog, when one is open (`ui('vary')`). */
export function VaryDialog() {
  const st = ui('vary', START);
  const v = useMore('variation', { line: st.line, ask: st.ask }, !!st.line);
  if (!st.line || !v) return null;
  if (v.none || v.error) {
    return html`<${Dialog} icon="edit_road" title=${v.none || v.error} w=${520} onClose=${close} foot=${html`<span class="rest"></span><${Btn} kind="primary" label=${tr('OK')} onClick=${close} />`} />`;
  }
  const q = v.quote;
  // (the controls' values as the view has them: the next ask starts from them)
  const values = Object.fromEntries(v.fields.map((f) => [f.key, f.type === 'switch' ? f.on : f.value]));
  const set = (key, x) => setUi('vary', { ask: { ...values, kind: v.kind, [key]: x } });
  const amount = st.amount ?? v.offer.amount;
  const agree = () => actMore('contract_vary', { line: v.line, ask: { ...values, kind: v.kind } }).then((r) => r?.done && close());
  const offer = () => actMore('contract_offer', { line: v.line, ask: { ...values, kind: v.kind }, amount }).then((r) => r?.done && close());
  const foot = html`<span class="rest"></span>
    <${Btn} label=${v.cancel} onClick=${close} />
    <${Btn} kind="primary" icon="handshake" label=${v.confirm} disabled=${!v.open} onClick=${() => v.open && agree()} />`;
  return html`<${Dialog} icon="edit_road" title=${v.title} w=${980} onClose=${close} foot=${foot} class="co-pg-vary">
    <div class="co-pg-vary-head">
      <${Plate} p=${{ number: v.number }} h=${26} />
      <div class="grow" style="min-width:0"><b class="co-pg-itemtitle co-pg-cut">${v.caption}</b><span class="co-pg-itemsub">${v.kind_text}  ·  ${v.pay}</span></div>
    </div>
    <p class="co-pg-note">${v.lead}</p>
    <div class="co-pg-pills" role="tablist">${v.kinds.map(
      (k) => html`<button type="button" role="tab" class=${k.key === v.kind ? 'on' : ''} onClick=${() => k.key !== v.kind && setUi('vary', { ask: { kind: k.key }, amount: null })}><${Icon} name=${k.icon} size=${16} />${k.label}</button>`,
    )}</div>
    <div class="co-pg-vary-grid">
      <div class="co-pg-vary-fields">${v.fields.map((f) => html`<${Field} f=${f} set=${set} />`)}</div>
      <${Price} q=${q} />
    </div>
    ${q.refused && html`<div class="co-pg-vary-refused"><${Icon} name="block" size=${18} /><span>${q.refused}</span></div>`}
    ${q.nothing && html`<p class="co-pg-note"><${Icon} name="info" size=${14} /> ${q.nothing}</p>`}
    <details class="co-pg-vary-offer" open=${!!st.amount}>
      <summary>${v.offer.title}</summary>
      <p class="co-pg-note">${v.offer.text}</p>
      ${v.offer.wait
        ? html`<p class="co-pg-small t-warn" style="font-weight:700;margin:0">${v.offer.wait}</p>`
        : html`<div class="co-pg-line">
            <input class="co-pg-bidin" type="number" step="100" min="0" value=${Math.round(units(amount))} disabled=${!v.offer.open} onChange=${(e) => setUi('vary', { amount: Math.max(0, cents(e.currentTarget.value)) })} />
            <span class="co-pg-small co-pg-muted grow">${eur(amount)}</span>
            <${Btn} icon="gavel" label=${v.offer.label} disabled=${!v.offer.open} onClick=${() => v.offer.open && offer()} />
          </div>`}
    </details>
    ${v.history.length > 0 &&
    html`<div><div class="co-pg-caps" style="margin:4px 0 6px">${v.history_title}</div>
      <ul class="co-pg-vary-history">${v.history.map((h) => html`<li>${h}</li>`)}</ul></div>`}
  </${Dialog}>`;
}

/** A control of the change: a slider with its value, a switch, or a choice. */
function Field({ f, set }) {
  if (f.type === 'switch') return html`<${Switch} label=${f.label} checked=${f.on} onChange=${(on) => set(f.key, on)} />`;
  if (f.type === 'choice') {
    return html`<div class="co-pg-vary-field"><span>${f.label}</span>
      <div class="co-pg-vary-choices">${f.options.map(
        ([k, l]) => html`<button type="button" class=${k === f.value ? 'on' : ''} onClick=${() => k !== f.value && set(f.key, k)}>${l}</button>`,
      )}</div></div>`;
  }
  const show = (x) => {
    const n = f.step < 1 ? x.toFixed(1) : String(Math.round(x));
    const signed = f.min < 0 && x > 0 ? '+' + n : n;
    return f.unit ? `${signed} ${f.unit}` : signed;
  };
  return html`<div class="co-pg-vary-field"><span>${f.label}</span>
    <${Slider} value=${f.value} min=${f.min} max=${f.max} step=${f.step} format=${show} onChange=${(x) => set(f.key, x)} /></div>`;
}

/** The price: the class, before and after, the money, the payment and what else it costs. */
function Price({ q }) {
  return html`<div class="co-pg-vary-price">
    <div class="co-pg-line"><${Chip} text=${q.class.text} tone=${q.class.tone} /><span class="co-pg-small co-pg-muted grow">${q.share}</span></div>
    <p class="co-pg-note">${q.class.note}</p>
    <table class="co-pg-table co-pg-vary-rows"><tbody>${q.rows.map(
      (r) => html`<tr><td>${r.label}</td><td class="num co-pg-muted">${r.before}</td><td class="num"><b>${r.after}</b></td><td class=${'num t-' + r.tone}>${r.delta || ''}</td></tr>`,
    )}</tbody></table>
    ${q.stops && html`<span class="co-pg-small"><${Icon} name="place" size=${13} /> ${q.stops}</span>`}
    <div class="co-pg-vary-money">${q.money.map(
      (m) => html`<div class=${'co-pg-line' + (m.total ? ' total' : '')}><span class="grow">${m.label}</span><b class=${'t-' + m.tone}>${m.value}</b></div>`,
    )}</div>
    <div class="co-pg-vary-pay" title=${q.payment.why}>
      <span class="co-pg-small co-pg-muted">${q.payment.label}</span>
      <div class="co-pg-line"><b class="co-pg-muted">${q.payment.before}</b><${Icon} name="arrow_forward" size=${16} /><b class=${'t-' + q.payment.tone}>${q.payment.after}</b></div>
      <span class="co-pg-small co-pg-muted">${q.payment.year}</span>
    </div>
    ${q.hits.map((h) => html`<span class=${'co-pg-small t-' + h.tone} style="font-weight:700"><${Icon} name=${h.icon} size=${13} /> ${h.text}</span>`)}
  </div>`;
}

// The company's Operations page (Luc, 2026-10-15): service control on the company's day as its
// clock runs - every bus running late with its minutes, every late driver nobody covers, every
// breakdown or accident of today - each with what it costs left as it is and the ways out (a
// short-turn, out of service to the terminus, a shorter layover, a spare bus at the terminus; a
// colleague or an agency driver for a late driver; a spare or a rental bus for a breakdown), each
// saying what it comes to before it is chosen. The view and the words come from
// `omsi-launcher-api`'s `company/ops.rs` (`co_view` "ops"), translated.

import { html, act, useView, Icon, Btn, Section, Seg } from './kit.js';
// (the advice against late running, of the last day closed: `late::advice_view`)
import { LateAdvice } from './late.js';

// (this page's own look, loaded with it)
if (!document.querySelector('link[data-co-ops-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-ops.css';
  sheet.dataset.coOpsCss = '1';
  document.head.append(sheet);
}

/** A way out: its icon, its words and what it comes to, the controller's suggestion marked. */
function Option({ o, what, v }) {
  return html`<div class=${'co-ops-opt' + (o.best ? ' best' : '') + (o.good ? '' : ' bad')}>
    <span class="ic"><${Icon} name=${o.icon} size=${16} /></span>
    <span class="words">
      ${o.best && html`<span class="tag">${v.best}</span>`}
      <span>${o.label}</span>
      <span class="effect">${o.effect}</span>
    </span>
    <${Btn} label=${v.choose} kind=${o.best ? 'primary' : ''} small onClick=${() => act(what, o.args)} />
  </div>`;
}

/** A bus, a driver or a breakdown: its name, its minutes, where it is, what it costs left as it
 * is, its ways out. */
function Card({ x, what, v }) {
  return html`<div class="co-ops-card">
    <div class="top"><b class="grow cut">${x.title}</b><span class=${'min t-' + x.tone}>${x.minutes}</span></div>
    <div class="where">${x.where}</div>
    <div class="nothing">${x.nothing}</div>
    ${x.options.map((o) => html`<${Option} o=${o} what=${what} v=${v} />`)}
    ${x.none && html`<div class="none">${x.none}</div>`}
  </div>`;
}

function Part({ caps, list, empty, what, v }) {
  return html`<${Section} title=${caps}>
    ${list?.length ? list.map((x) => html`<${Card} x=${x} what=${what} v=${v} />`) : html`<div class="empty">${empty}</div>`}
  </${Section}>`;
}

export function OpsPage() {
  const v = useView('ops');
  if (!v?.title) return html`<div style="height:100%"></div>`;
  const busy = v.reading || v.error || v.idle;
  return html`<div class="co-ops">
    <div class="head">
      <span class="names"><b>${v.title}</b><span>${v.now}  ·  ${v.mode}</span></span>
      <span class="notice"><${Icon} name="schedule" size=${16} />${v.notice.label}
        <${Seg} value=${v.notice.value} options=${v.notice.options} onChange=${(n) => act('ops_notice', { n })} />
      </span>
    </div>
    <div class="body">
      ${busy && html`<div class=${'empty' + (v.error ? ' t-err' : '')}>${v.reading || v.error || v.idle}</div>`}
      ${!v.reading && !v.error && html`
        <${Part} caps=${v.caps.buses} list=${v.buses} empty=${v.empty.buses} what="ops_apply" v=${v} />
        <${Part} caps=${v.caps.drivers} list=${v.drivers} empty=${v.empty.drivers} what="ops_cover" v=${v} />
        <${Part} caps=${v.caps.breaks} list=${v.breaks} empty=${v.empty.breaks} what="ops_break" v=${v} />
        ${v.done?.length > 0 && html`<${Section} title=${v.caps.done}>
          ${v.done.map((d) => html`<div class="co-ops-done"><span class="time">${d.time}</span><span class="cut">${d.text}</span></div>`)}
        </${Section}>`}`}
      <${LateAdvice} />
    </div>
  </div>`;
}

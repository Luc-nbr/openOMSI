// The company's Operations page (Luc, 2026-10-15): service control on the company's day as its
// clock runs - every bus running late with its minutes, every late driver nobody covers, every
// breakdown or accident of today - each with what it costs left as it is and the ways out (a
// short-turn, out of service to the terminus, a shorter layover, a spare bus at the terminus; a
// colleague or an agency driver for a late driver; a spare or a rental bus for a breakdown), each
// saying what it comes to before it is chosen. The view and the words come from
// `omsi-launcher-api`'s `company/ops.rs` (`co_view` "ops"), translated.
//
// Beta 65 (after Luc's reference): the company's clock, the alert's minutes and the bell in the
// head; four KPI cards (on time, buses late, drivers late, breakdowns); the live incidents as one
// table - severity, line, bus, delay and where, cause, the ways out as compact buttons (the
// controller's suggestion filled) - with the lines running normally under it; beside it the fleet
// map small, every bus coloured by its delay, a row clicked centring its bus. Under the map at a
// narrow width. The advice against late running below, folded.
//
// The control room's messages (docs/omsihub/MESSAGES.md, `messages.js`): a "Message" button on
// every row writes to its driver, a driver's own message is a row of its own, the day's messages
// are listed under the incidents.

import { html, act, useView, useState, useRef, useSize, Icon, Seg, Plate } from './kit.js';
// (the advice against late running, of the last day closed: `late::advice_view`)
import { LateAdvice } from './late.js';
import { MiniMap } from './fleetmap.js';
import { Kpi, Ring, Card, Empty } from './pagesb.js';
import { useCallback } from '../../../vendor/preact-htm.js';
import { MsgButton, Compose, MsgLog } from './messages.js';

// (this page's own look, loaded with it)
if (!document.querySelector('link[data-co-ops-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-ops.css';
  sheet.dataset.coOpsCss = '1';
  document.head.append(sheet);
}

/** The ways out's actions, by the kind of row. */
const WHAT = { bus: 'ops_apply', driver: 'ops_cover', break: 'ops_break', msg: 'ops_msg_ack' };

/** A way out as a compact button: its icon and a word or two, the controller's suggestion filled
 * in the accent, its long words and what it comes to as the tooltip. Held while it runs (what it
 * did, or why not, is said in the page's corner). */
function Way({ o, what, v }) {
  const [busy, setBusy] = useState(false);
  const go = async (e) => {
    e.stopPropagation();
    setBusy(true);
    try {
      await act(what, o.args);
    } finally {
      setBusy(false);
    }
  };
  const tip = [o.best ? v.best : '', o.label, o.effect].filter(Boolean).join('\n');
  return html`<button type="button" class=${'co-pgb-op-way' + (o.best ? ' best' : '') + (o.good ? '' : ' bad')} title=${tip} disabled=${busy} onClick=${go}>
    <${Icon} name=${o.icon} size=${14} /><span>${o.short || o.label}</span>
  </button>`;
}

/** A row of the incident table. */
function Row({ x, v, onFocus }) {
  const sev = x.sev?.key || 'low';
  return html`<div class=${'co-pgb-op-row sev-' + sev + (x.map ? ' can' : '')} onClick=${() => x.map && onFocus(x.map)}>
    <span class="co-pgb-op-sev"><i></i>${x.sev?.label}</span>
    <span class="co-pgb-op-line">${x.plate && html`<${Plate} p=${x.plate} h=${20} />`}</span>
    <span class="co-pgb-op-bus co-pgb-num">${x.bus}</span>
    <span class="co-pgb-op-delay"><b class="co-pgb-num">${x.delay}</b>${x.at && html`<span><${Icon} name="location_on" size=${12} />${x.at}</span>`}</span>
    <span class="co-pgb-op-cause"><b>${x.cause}</b>${x.detail && html`<span title=${x.detail}>${x.detail}</span>`}</span>
    <span class="co-pgb-op-ways">
      ${x.done && html`<span class="co-pgb-op-done"><${Icon} name="check" size=${13} />${x.done}</span>`}
      ${(x.options || []).map((o) => html`<${Way} o=${o} what=${WHAT[x.kind]} v=${v} />`)}
      ${x.none && html`<span class="co-pgb-op-none" title=${x.nothing}>${x.none}</span>`}
      <${MsgButton} x=${x} v=${v} />
    </span>
  </div>`;
}

/** The map's colour of a bus by its delay against the alert's minutes (a hex: the canvas mixes
 * it). */
function painter(at) {
  const css = getComputedStyle(document.documentElement);
  const hex = (name, or) => {
    const x = css.getPropertyValue(name).trim();
    return /^#[0-9a-f]{6}$/i.test(x) ? x : or;
  };
  const tone = { ok: hex('--hub-ok', '#34b26e'), warn: hex('--hub-warn', '#e8aa46'), danger: hex('--hub-danger', '#d93a30'), dim: hex('--hub-text-faint', '#60687a') };
  return (b) => {
    if (b.state === 'depot' || b.state === 'done') return tone.dim;
    if (b.state === 'broken' || b.min >= at) return tone.danger;
    if (b.min > 3) return tone.warn;
    return tone.ok;
  };
}

/** The "Live network map" card: the fleet map small, its legend and its buttons. */
function NetMap({ v, ctl }) {
  const [ref, size] = useSize();
  const at = v.alert_at || 5;
  const paint = useCallback(painter(at), [at]);
  const m = v.map;
  return html`<${Card} class="co-pgb-op-map" icon="map" title=${m.title}>
    <div class="co-pgb-op-map-sub">
      <span class="co-pgb-note">${m.sub}</span>
      <span class="co-pgb-op-legend">${m.legend.map(([t, l]) => html`<span><i class=${t}></i>${l}</span>`)}</span>
    </div>
    <div class="co-pgb-op-map-box" ref=${ref}>
      <${MiniMap} paint=${paint} ctl=${ctl} W=${size.w} H=${size.h} />
      <span class="co-pgb-op-map-tools">
        <button type="button" class="co-pgb-round" title=${m.zoom_in} onClick=${() => ctl.current?.zoom(1 / 1.4)}><${Icon} name="add" size=${17} /></button>
        <button type="button" class="co-pgb-round" title=${m.zoom_out} onClick=${() => ctl.current?.zoom(1.4)}><${Icon} name="remove" size=${17} /></button>
        <button type="button" class="co-pgb-round" title=${m.fit} onClick=${() => ctl.current?.fit()}><${Icon} name="my_location" size=${16} /></button>
      </span>
    </div>
  </${Card}>`;
}

export function OpsPage() {
  const v = useView('ops');
  const advice = useView('late_advice');
  const ctl = useRef(null);
  if (!v?.title) return html`<div class="co-pgb"></div>`;
  const busy = v.reading || v.error || v.idle;
  const k = v.kpis;
  const rows = [...(v.breaks || []), ...(v.buses || []), ...(v.drivers || []), ...(v.msgs || [])];
  const order = { high: 0, medium: 1, low: 2, ok: 3 };
  rows.sort((a, b) => (order[a.sev?.key] ?? 2) - (order[b.sev?.key] ?? 2));
  const focus = (key) => ctl.current?.focus(key);
  const inc = v.incidents;
  return html`<div class="co-pgb co-pgb-ops">
    <div class="co-pgb-stack">
      <div class="co-pgb-op-head">
        <span class="co-pgb-op-clock" title=${v.mode}>
          <${Icon} name="schedule" size=${30} />
          <span><span class="co-pgb-note">${v.clock.caption}</span><span class="co-pgb-row"><b class="co-pgb-num">${v.clock.time}</b><span class="co-pgb-op-date">${v.clock.date}</span></span></span>
        </span>
        <span class="co-pgb-op-alert">
          <span class="co-pgb-note">${v.alert}</span>
          <span class="co-pgb-op-seg"><${Seg} value=${v.notice.value} options=${v.notice.options} onChange=${(n) => act('ops_notice', { n })} /></span>
        </span>
        <span class="co-pgb-op-bell" title=${v.bell.tip}><${Icon} name="campaign" size=${22} />${v.bell.n > 0 && html`<b>${v.bell.n}</b>`}</span>
      </div>
      ${busy && html`<${Empty} icon=${v.error ? 'error' : 'schedule'} text=${busy} />`}
      ${k && html`<div class="co-pgb-kpis co-pgb-op-kpis">
        <${Kpi} label=${k.on_time.label} value=${k.on_time.pct} under=${k.on_time.under} ring=${html`<${Ring} frac=${k.on_time.frac} size=${58} stroke=${7} tone=${k.on_time.tone} />`} />
        <${Kpi} icon="schedule" tone=${k.buses.n ? 'accent' : 'dim'} label=${k.buses.label} value=${k.buses.n} under=${k.buses.under} />
        <${Kpi} icon="person" tone=${k.drivers.n ? 'accent' : 'dim'} label=${k.drivers.label} value=${k.drivers.n} under=${k.drivers.under} />
        <${Kpi} icon="construction" tone=${k.breaks.n ? 'danger' : 'dim'} label=${k.breaks.label} value=${k.breaks.n} under=${k.breaks.under} />
      </div>`}
      ${!v.reading && !v.error && html`<div class="co-pgb-op-body">
        <${Card} class="co-pgb-op-inc flush" icon="warning" title=${inc.title}
          aside=${html`<span class="co-pgb-op-live"><i></i>${inc.live}<span>${v.updated}</span></span>`}>
          <div class="co-pgb-op-inc-sub co-pgb-note">${inc.sub}</div>
          ${rows.length > 0 && html`<div class="co-pgb-op-table">
            <div class="co-pgb-op-row head">${inc.cols.map((c) => html`<span>${c}</span>`)}</div>
            ${rows.map((x) => html`<${Row} x=${x} v=${v} onFocus=${focus} />`)}
          </div>`}
          <div class="co-pgb-op-clear">
            <div class="co-pgb-op-clear-h"><span class="co-pgb-op-ok"><${Icon} name="check" size=${15} /></span><span><b>${inc.clear}</b><span class="co-pgb-note">${inc.clear_sub}</span></span></div>
            ${(v.normal || []).map((n) => html`<div class="co-pgb-op-normal" style=${`opacity:${Math.max(0.45, 1 - n.age / 90)}`}>
              <${Plate} p=${n.plate} h=${20} /><span class="co-pgb-grow">${inc.normal}</span><span class="co-pgb-note co-pgb-num">${n.time}</span>
            </div>`)}
          </div>
        </${Card}>
        <${NetMap} v=${v} ctl=${ctl} />
      </div>`}
      ${v.done?.length > 0 && html`<${Card} icon="history" title=${v.caps.done}>
        ${v.done.map((d) => html`<div class="co-ops-done"><span class="time">${d.time}</span><span class="cut">${d.text}</span></div>`)}
      </${Card}>`}
      ${!v.reading && !v.error && html`<${MsgLog} v=${v} />`}
      ${advice?.list?.length > 0 && html`<details class="co-pgb-card co-pgb-op-advice"><summary><${Icon} name="info" size=${16} />${v.advice}</summary><${LateAdvice} /></details>`}
    </div>
    <${Compose} v=${v} />
  </div>`;
}

// The company's Operations page (Luc, 2026-10-15; redesigned 2026-10-16 after his reference):
// service control on the company's day as its clock runs. The head: the company's clock with its
// date, when to be alerted of a bus running late (`control::notice_minutes`) and the bell with
// what waits (`control::waiting`). Four KPIs: the trips on time (a ring), the buses running late,
// the drivers late, the breakdowns. Then, side by side (the map under the table on a narrow
// page): the live incidents - a row each, its severity, its line, its bus, its delay and where,
// its cause, and its ways out as compact buttons (the controller's suggestion filled, the effect
// of each in its tooltip), the lines running normally under them -, and the fleet map
// (`fleetmap.js`) small, its buses coloured by their delay, centred on the bus of a row clicked.
// What was done today, and the last closed day's advice (collapsed), at the foot. The view and
// the words come from `omsi-launcher-api`'s `company/ops.rs` (`co_view` "ops"), translated.

import { html, act, useView, useState, Icon, Seg, Plate } from './kit.js';
// (the advice against late running, of the last day closed: `late::advice_view`)
import { LateAdvice } from './late.js';
import { FleetMapPage } from './fleetmap.js';
import { Kpi, Ring, Empty } from './pagesb.js';

// (this page's own look, loaded with it)
if (!document.querySelector('link[data-co-ops-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-ops.css';
  sheet.dataset.coOpsCss = '1';
  document.head.append(sheet);
}

/** A way out as a compact button: its icon and short name, the whole of it and what it comes to
 * in its tooltip, the controller's suggestion filled. Held while the action runs (what it did, or
 * why not, is said in the page's corner). */
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
  const tip = `${o.best ? v.best + ': ' : ''}${o.label}\n${o.effect}`;
  return html`<button type="button" class=${'co-ops-way' + (o.best ? ' best' : '') + (o.good ? '' : ' bad')} title=${tip} disabled=${busy} onClick=${go}>
    <${Icon} name=${o.icon} size=${14} /><span class="co-pgb-cut">${o.short || o.label}</span>
  </button>`;
}

/** An incident's row: severity, line, bus, delay and where, cause, ways out; a click centres the
 * map on its bus. */
function Row({ x, what, v, onFocus }) {
  const cause = x.cause || x.title;
  const detail = x.detail || x.where || x.nothing;
  return html`<div class=${'co-ops-row sev-' + (x.sev || 'low')} onClick=${() => x.key && onFocus(x.key)} title=${x.title}>
    <span class="co-ops-sev"><i></i>${x.sev_label}</span>
    <span class="co-ops-line">${x.plate && html`<${Plate} p=${x.plate} h=${22} />`}</span>
    <span class="co-ops-bus co-pgb-cut">${x.bus}</span>
    <span class="co-ops-delay"><b>${x.minutes}</b>${x.at && html`<span class="co-pgb-cut">${x.at}</span>`}</span>
    <span class="co-ops-cause"><b class="co-pgb-cut">${cause}</b>${detail && html`<span class="co-ops-detail" title=${detail}>${detail}</span>`}
      ${x.done && html`<span class="co-ops-done-line"><${Icon} name="check" size=${12} />${x.done}</span>`}</span>
    <span class="co-ops-ways">
      ${x.options.map((o) => html`<${Way} o=${o} what=${what} v=${v} />`)}
      ${!x.options.length && x.none && html`<span class="co-pgb-note">${x.none}</span>`}
    </span>
  </div>`;
}

export function OpsPage() {
  const v = useView('ops');
  const [focus, setFocus] = useState({ key: null, n: 0 });
  const [advice, setAdvice] = useState(false);
  if (!v?.title) return html`<div class="co-pgb co-pgb-ops"></div>`;
  const w = v.words || {};
  const st = v.status;
  const busy = v.reading || v.error || v.idle;
  const rows = [
    ...(v.buses || []).map((x) => [x, 'ops_apply']),
    ...(v.drivers || []).map((x) => [x, 'ops_cover']),
    ...(v.breaks || []).map((x) => [x, 'ops_break']),
  ];
  const open = rows.filter(([x]) => x.sev !== 'ok');
  const onFocus = (key) => setFocus({ key, n: focus.n + 1 });
  const bell = v.bell?.n || 0;
  return html`<div class="co-pgb co-pgb-ops">
    <div class="co-pgb-stack">
      <div class="co-ops-head">
        <span class="co-ops-clock"><b class="co-pgb-num">${v.clock}</b><span><span class="co-pgb-caps">${v.title}</span><span class="co-ops-date">${v.date}</span></span></span>
        <span class="co-ops-notice"><span class="co-pgb-note">${v.notice_label}</span>
          <${Seg} value=${v.notice.value} options=${v.notice.options} onChange=${(n) => act('ops_notice', { n })} />
        </span>
        <span class=${'co-ops-bell' + (bell > 0 ? ' on' : '')} title=${v.bell?.tip}><${Icon} name="campaign" size=${19} />${bell > 0 && html`<i>${bell}</i>`}</span>
      </div>
      ${busy && html`<div class=${'co-pgb-note' + (v.error ? ' co-pgb-t-danger' : '')}>${v.reading || v.error || v.idle}</div>`}
      ${st && html`<div class="co-pgb-kpis">
        <${Kpi} label=${st.on_time.label} value=${st.on_time.value} under=${st.on_time.under} ring=${html`<${Ring} frac=${st.on_time.frac} tone=${st.on_time.tone} size=${44} />`} />
        <${Kpi} icon="directions_bus" tone=${st.buses.tone} label=${st.buses.label} value=${st.buses.value} under=${st.buses.under} />
        <${Kpi} icon="person" tone=${st.drivers.tone} label=${st.drivers.label} value=${st.drivers.value} under=${st.drivers.under} />
        <${Kpi} icon="construction" tone=${st.breaks.tone} label=${st.breaks.label} value=${st.breaks.value} under=${st.breaks.under} />
      </div>`}
      ${!v.reading && !v.error && html`<div class="co-ops-cols">
        <section class="co-pgb-card co-ops-incidents">
          <div class="co-ops-card-h">
            <span class="co-pgb-grow"><h3>${w.incidents}</h3><span class="co-pgb-note">${w.incidents_sub}</span></span>
            <span class="co-ops-live"><i></i>${w.live}</span>
          </div>
          ${open.length > 0 && html`<div class="co-ops-table">
            <div class="co-ops-row co-ops-th"><span>${w.severity}</span><span>${w.line}</span><span>${w.bus}</span><span>${w.delay}</span><span>${w.cause}</span><span>${w.actions}</span></div>
            ${open.map(([x, what]) => html`<${Row} x=${x} what=${what} v=${v} onFocus=${onFocus} />`)}
          </div>`}
          <div class="co-ops-clear">
            <${Empty} ok=${true} text=${open.length ? w.clear_sub : html`<b>${w.clear}</b> · ${w.clear_sub}`} />
            ${st?.clear?.length > 0 && html`<div class="co-ops-normal">
              <span class="co-pgb-caps">${w.normal}</span>
              <div class="co-ops-normal-list">${st.clear.map((l) => html`<span class="co-ops-normal-item" style=${`opacity:${(1 - 0.55 * (l.age || 0)).toFixed(2)}`}><${Plate} p=${l.plate} h=${20} /><span class="co-pgb-note">${l.time}</span></span>`)}</div>
            </div>`}
          </div>
        </section>
        <section class="co-pgb-card co-ops-map">
          <div class="co-ops-card-h"><span class="co-pgb-grow"><h3>${w.map}</h3><span class="co-pgb-note">${w.map_sub}</span></span></div>
          <div class="co-ops-mapbox"><${FleetMapPage} embed=${{ threshold: st?.threshold || 5, focus, tips: w }} /></div>
          <div class="co-ops-legend">
            <span><i class="ok"></i>${w.on_time}</span><span><i class="slight"></i>${w.slight}</span><span><i class="late"></i>${w.late}</span><span><i class="out"></i>${w.out}</span>
          </div>
        </section>
      </div>`}
      ${v.done?.length > 0 && html`<section class="co-pgb-card co-ops-donecard">
        <div class="co-ops-card-h"><h3 class="co-pgb-grow">${v.caps.done}</h3></div>
        ${v.done.map((d) => html`<div class="co-ops-done"><span class="time">${d.time}</span><span class="co-pgb-cut">${d.text}</span></div>`)}
      </section>`}
      <section class="co-pgb-card co-ops-advice">
        <button type="button" class="co-ops-fold" onClick=${() => setAdvice(!advice)}><${Icon} name=${advice ? 'expand_less' : 'expand_more'} size=${18} /><b>${w.advice}</b></button>
        ${advice && html`<div class="co-ops-advice-body"><${LateAdvice} /></div>`}
      </section>
    </div>
  </div>`;
}

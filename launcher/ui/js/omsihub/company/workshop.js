// The workshop's stores (`company_more/workshop.rs`; the rules are `company::parts`'), City Bus
// Manager's spare parts shop. Beta 65, after Luc's reference: the head and three KPI cards (the
// mechanics at work, the buses in the workshop, the parts on order); the spare parts as a table -
// a row a part with its icon and name, its stock against what the stores keep with a level bar and
// its minimum, its price (the list price struck through when the research takes some off), a
// stepper of how many to order and the line's total -; beside it (under it on a narrow page) a
// sticky column: the order's summary (the delivery, the sum and Order), the buses in repair, the
// stores' own ordering and the mechanics.

import { html, ui, setUi, goTab, Icon, Btn, Seg, Switch } from './kit.js';
import { useMore, actMore } from './more.js';
import { Kpi, Card, Bar } from './pagesb.js';
import { DefectRow } from './fleet.js';

const MOST = 30;

export function WorkshopPage() {
  const st = ui('workshop', { counts: Array(10).fill(0), express: 0 });
  const v = useMore('workshop', { counts: st.counts, express: st.express });
  if (!v?.table) return html`<div class="co-pgb"></div>`;
  const set = (k, n) => {
    const counts = [...st.counts];
    counts[k] = Math.max(0, Math.min(v.most || MOST, n));
    setUi('workshop', { counts });
  };
  return html`<div class="co-pgb co-pgb-workshop">
    <div class="co-pgb-stack">
      <div class="co-pgb-head">
        <span class="co-pgb-ws-hic"><${Icon} name="construction" size=${26} /></span>
        <span class="co-pgb-names"><h2 class="co-pgb-h1">${v.head.title}</h2><span class="co-pgb-sub">${v.head.sub}</span></span>
      </div>
      <div class="co-pgb-kpis co-pgb-ws-kpis">${v.kpis.map((k) => html`<${Kpi} icon=${k.icon} tone=${k.tone} label=${k.label} value=${k.value} under=${k.under} />`)}</div>
      <div class="co-pgb-ws-body">
        <${Parts} t=${v.table} counts=${st.counts} onCount=${set} />
        <aside class="co-pgb-ws-side">
          <${Summary} v=${v} st=${st} />
          <${Repair} s=${v.side} />
          ${v.side.defects && html`<${Defects} d=${v.side.defects} />`}
          <${Auto} a=${v.side.auto} />
          <${Mechanics} m=${v.side.mechanics} />
        </aside>
      </div>
    </div>
  </div>`;
}

/** The spare parts' table: a row a part. */
function Parts({ t, counts, onCount }) {
  return html`<${Card} class="co-pgb-ws-parts" icon="inventory_2" title=${t.title}>
    <p class="co-pgb-note co-pgb-ws-sub">${t.sub}</p>
    <div class="co-pgb-ws-table">
      <div class="co-pgb-ws-row head">${t.cols.map((c) => html`<span>${c}</span>`)}</div>
      ${t.rows.map((r, k) => {
        const n = counts[k] || 0;
        return html`<div class=${'co-pgb-ws-row' + (n > 0 ? ' on' : '')}>
          <span class="co-pgb-ws-part"><span class="co-pgb-ws-pic"><${Icon} name=${r.icon} size=${18} /></span><b class="co-pgb-cut" title=${r.label}>${r.label}</b></span>
          <span class="co-pgb-ws-stock">
            <span class="co-pgb-row"><b class=${'co-pgb-num co-pgb-t-' + (r.tone === 'ok' ? 'ink' : r.tone)}>${r.level}</b><span class="co-pgb-note">${r.min}</span></span>
            <${Bar} frac=${r.keep ? r.stock / r.keep : 0} tone=${r.tone} thin mark=${r.keep ? r.low / r.keep : null} />
            ${r.coming && html`<span class="co-pgb-note co-pgb-t-info">${r.coming}</span>`}
          </span>
          <span class="co-pgb-ws-price"><b class="co-pgb-num">${r.price}</b>${r.was && html`<s class="co-pgb-num">${r.was}</s>`}</span>
          <span class="co-pgb-ws-step">
            <button type="button" title=${t.less} disabled=${n <= 0} onClick=${() => onCount(k, n - 1)}><${Icon} name="remove" size=${15} /></button>
            <input type="number" min="0" max=${MOST} value=${n} onChange=${(e) => onCount(k, Math.round(Number(e.currentTarget.value) || 0))} />
            <button type="button" title=${t.more} disabled=${n >= MOST} onClick=${() => onCount(k, n + 1)}><${Icon} name="add" size=${15} /></button>
          </span>
          <span class=${'co-pgb-ws-line co-pgb-num' + (r.line_on ? '' : ' off')}>${r.line}</span>
        </div>`;
      })}
    </div>
  </${Card}>`;
}

/** The order's summary: the delivery, the sum and Order (what is in the way said first). */
function Summary({ v, st }) {
  const f = v.foot;
  const s = v.summary;
  const buy = async () => {
    const r = await actMore('parts_buy', { counts: st.counts, express: st.express });
    if (r?.bought) setUi('workshop', { counts: Array(10).fill(0) });
  };
  const ways = s.ways.map(([a, b], k) => [k, html`<span class="co-pgb-ws-way"><b>${a}</b><span>${b}</span></span>`]);
  return html`<${Card} class="co-pgb-ws-sum" icon="receipt_long" title=${s.title}>
    <p class="co-pgb-note">${s.sub}</p>
    <div class="co-pgb-ws-ways"><${Seg} value=${st.express} options=${ways} onChange=${(k) => setUi('workshop', { express: k })} /></div>
    <p class="co-pgb-note">${f.note}</p>
    <div class="co-pgb-ws-total"><span>${s.total}</span><b class=${'co-pgb-num' + (f.sum_tone === 'danger' ? ' co-pgb-t-danger' : '')}>${s.amount}</b></div>
    <div class="co-pgb-row">
      <${Btn} kind="primary" icon="inventory_2" label=${s.order} class="co-pgb-big co-pgb-grow" disabled=${!(f.chosen > 0)} onClick=${buy} />
      ${f.chosen > 0 && html`<button type="button" class="co-pgb-round" title=${f.clear} onClick=${() => setUi('workshop', { counts: Array(10).fill(0) })}><${Icon} name="close" size=${16} /></button>`}
    </div>
    ${v.side.orders.rows.length > 0 && html`<div class="co-pgb-ws-orders">
      <span class="co-pgb-caps">${v.side.orders.title}</span>
      ${v.side.orders.rows.map((o) => html`<div class="co-pgb-ws-order"><${Icon} name="schedule" size=${14} /><span class="co-pgb-grow"><b class="co-pgb-cut">${o.what}</b><span class="co-pgb-note co-pgb-cut" title=${o.parts}>${o.parts}</span></span></div>`)}
    </div>`}
  </${Card}>`;
}

/** The buses in the workshop (and those waiting for parts). */
function Repair({ s }) {
  const r = s.repair;
  return html`<${Card} icon="directions_bus" title=${r.title}>
    ${r.rows.length
      ? r.rows.map((b) => html`<div class="co-pgb-ws-bus">
          <span class="co-pgb-ws-tag co-pgb-num">${b.number}</span>
          <span class="co-pgb-grow"><b class="co-pgb-cut">${b.what}</b><span class="co-pgb-note co-pgb-cut">${b.until}</span>${b.parts && html`<span class="co-pgb-note co-pgb-t-warn co-pgb-cut" title=${b.parts}>${b.parts}</span>`}</span>
        </div>`)
      : html`<div class="co-pgb-ws-none"><span class="co-pgb-ws-none-ic"><${Icon} name="directions_bus" size=${30} /></span><b>${r.none}</b><span class="co-pgb-note">${r.none_sub}</span></div>`}
    ${s.waiting.rows
      .filter((w) => !r.rows.some((b) => b.number === w.number))
      .map((w) => html`<div class="co-pgb-ws-bus">
        <span class="co-pgb-ws-tag co-pgb-num">${w.number}</span>
        <span class="co-pgb-grow"><b class="co-pgb-cut co-pgb-t-warn" title=${w.parts}>${w.parts}</b><span class="co-pgb-note">${w.since}</span></span>
      </div>`)}
  </${Card}>`;
}

/** The defects the drivers reported (the walk-around at sign-off) or that were logged on the way. */
function Defects({ d }) {
  return html`<${Card} icon="build" title=${d.title}>
    <p class="co-pgb-note">${d.sub}</p>
    <div class="co-crew">${d.rows.length ? d.rows.map((x) => html`<${DefectRow} d=${x} bus />`) : html`<div class="co-crew-none">${d.none}</div>`}</div>
  </${Card}>`;
}

/** The stores' own ordering. */
function Auto({ a }) {
  return html`<${Card} icon="autorenew" title=${a.title} aside=${html`<${Switch} checked=${a.on} onChange=${(on) => actMore('parts_auto', { on })} title=${a.label} />`}>
    <p class="co-pgb-note">${a.label}. ${a.text}</p>
    <div class=${'co-pgb-ws-autorow co-pgb-t-' + a.row_tone}><${Icon} name="inventory_2" size=${15} /><span>${a.row}</span></div>
  </${Card}>`;
}

/** The mechanics, the Staff page a click away. */
function Mechanics({ m }) {
  return html`<${Card} icon="users" title=${m.title}>
    <p class=${'co-pgb-text' + (m.tone === 'warn' ? ' co-pgb-t-warn' : '')}>${m.text}</p>
    <button type="button" class="co-pgb-ws-link" onClick=${() => goTab(m.tab)}>${m.link}<${Icon} name="chevron_right" size=${15} /></button>
  </${Card}>`;
}

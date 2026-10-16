// The workshop's stores (`company_more/workshop.rs`; the rules are `company::parts`'): four
// KPIs; the spare parts as a table - a row a part: its icon and name, its stock on a level bar
// with the minimum the stores keep, its price (the list price struck through when the research
// takes some off), a quantity stepper and the row's total -; beside it (under it on a narrow
// page) the order: the delivery, the sum and Order, kept in view; and the cards of the buses
// waiting for parts, what is on order, the stores' own ordering and the mechanics.

import { html, ui, setUi, Icon, Btn, Seg, Switch } from './kit.js';
import { useMore, actMore } from './more.js';
import { Kpi, Card, Bar, Empty, Chip } from './pagesb.js';

const ICONS = ['groups', 'garage', 'schedule', 'payments'];

export function WorkshopPage() {
  const st = ui('workshop', { counts: Array(10).fill(0), express: 0 });
  const v = useMore('workshop', { counts: st.counts, express: st.express });
  if (!v?.figures) return html`<div class="co-pgb co-pgb-ws"></div>`;
  const set = (k, n) => {
    const counts = [...st.counts];
    counts[k] = Math.max(0, Math.min(v.most, n));
    setUi('workshop', { counts });
  };
  const clear = () => setUi('workshop', { counts: Array(10).fill(0) });
  const f = v.foot;
  const buy = async () => {
    const r = await actMore('parts_buy', { counts: st.counts, express: st.express });
    if (r?.bought) clear();
  };
  const c = v.cols || {};
  const s = v.side;
  return html`<div class="co-pgb co-pgb-ws">
    <div class="co-pgb-stack">
      <div class="co-pgb-head"><span class="co-pgb-names"><h2 class="co-pgb-h1">${v.title}</h2></span></div>
      <div class="co-pgb-kpis">${v.figures.map((x, k) => html`<${Kpi} icon=${ICONS[k] || 'construction'} tone=${x.tone === 'warn' ? 'warn' : 'accent'} label=${x.label} value=${x.value} under=${x.under} />`)}</div>
      <div class="co-pgb-ws-body">
        <section class="co-pgb-card co-pgb-ws-parts">
          <div class="co-pgb-card-h"><span class="co-pgb-ic"><${Icon} name="inventory_2" size=${17} /></span><h3>${v.parts_title}</h3><span class="co-pgb-aside">${v.parts_sub}</span></div>
          <div class="co-pgb-ws-table">
            <div class="co-pgb-ws-row co-pgb-ws-th"><span>${c.part}</span><span>${c.stock}</span><span>${c.price}</span><span>${c.qty}</span><span>${c.total}</span></div>
            ${v.cards.map((p, k) => {
              const n = st.counts[k] || 0;
              return html`<div class=${'co-pgb-ws-row' + (n > 0 ? ' on' : '')}>
                <span class="co-pgb-ws-name"><span class="co-pgb-ws-ic"><${Icon} name=${p.icon} size=${17} /></span><b class="co-pgb-cut" title=${p.label}>${p.label}</b></span>
                <span class="co-pgb-ws-stock" title=${v.in_store}>
                  <span class="co-pgb-row"><b class=${'co-pgb-num' + (p.stock === 0 ? ' co-pgb-t-danger' : '')}>${p.stock}</b><span class="co-pgb-note">${p.min}</span>${p.ordered && html`<${Chip} icon="schedule" text=${p.ordered} tone="info" />`}</span>
                  <${Bar} frac=${p.level} tone=${p.level_tone} thin=${true} mark=${0.5} />
                </span>
                <span class="co-pgb-ws-price">${p.was && html`<s>${p.was}</s>`}<b class=${p.was ? 'co-pgb-t-ok' : ''}>${p.price}</b></span>
                <span class="co-pgb-ws-qty">
                  <button type="button" class="co-pgb-round" disabled=${n <= 0} onClick=${() => set(k, n - 1)}><${Icon} name="remove" size=${15} /></button>
                  <input type="number" class="co-field" min="0" max=${v.most} value=${n} onInput=${(e) => set(k, Math.round(Number(e.currentTarget.value) || 0))} />
                  <button type="button" class="co-pgb-round" disabled=${n >= v.most} onClick=${() => set(k, n + 1)}><${Icon} name="add" size=${15} /></button>
                </span>
                <b class=${'co-pgb-ws-total co-pgb-num' + (p.line_on ? '' : ' co-pgb-t-dim')}>${p.line}</b>
              </div>`;
            })}
          </div>
        </section>
        <aside class="co-pgb-ws-side">
          <section class="co-pgb-card co-pgb-ws-order">
            <div class="co-pgb-card-h"><span class="co-pgb-ic"><${Icon} name="inventory_2" size=${17} /></span><h3>${v.order_title}</h3><span class="co-pgb-aside">${v.chosen}</span></div>
            <span class="co-pgb-caps">${v.delivery}</span>
            <${Seg} value=${st.express} options=${f.ways.map((t, k) => [k, t])} onChange=${(k) => setUi('workshop', { express: k })} />
            <p class="co-pgb-note">${f.note}</p>
            <div class=${'co-pgb-ws-sum' + (f.sum_tone === 'danger' ? ' co-pgb-t-danger' : '')}>${f.sum}</div>
            <div class="co-pgb-row">
              <${Btn} kind=${f.chosen > 0 ? 'primary' : ''} class="co-pgb-big co-pgb-grow" icon="inventory_2" label=${f.buy} disabled=${f.chosen <= 0} onClick=${buy} />
              ${f.chosen > 0 && html`<button type="button" class="co-pgb-round" title=${f.clear} onClick=${clear}><${Icon} name="close" size=${16} /></button>`}
            </div>
          </section>
        </aside>
      </div>
      <div class="co-pgb-ws-cards">
        <${Card} icon="directions_bus" title=${s.waiting.title} aside=${s.waiting.rows.length || ''}>
          ${s.waiting.rows.length
            ? s.waiting.rows.map((w) => html`<div class="co-pgb-ws-item"><${Chip} text=${w.number} /><span class="co-pgb-grow"><b class="co-pgb-cut co-pgb-t-warn">${w.parts}</b><span class="co-pgb-note">${w.since}</span></span></div>`)
            : html`<${Empty} ok=${true} text=${s.waiting.none} />`}
        </${Card}>
        <${Card} icon="schedule" title=${s.orders.title} aside=${s.orders.rows.length || ''}>
          ${s.orders.rows.length
            ? s.orders.rows.map((o) => html`<div class="co-pgb-ws-item"><span class="co-pgb-ws-ic small"><${Icon} name="schedule" size=${14} /></span><span class="co-pgb-grow"><b class="co-pgb-cut">${o.what}</b><span class="co-pgb-note">${o.parts}</span></span></div>`)
            : html`<${Empty} icon="inventory_2" text=${s.orders.none} />`}
        </${Card}>
        <${Card} icon="autorenew" title=${s.auto.title}>
          <${Switch} checked=${s.auto.on} label=${s.auto.label} onChange=${(on) => actMore('parts_auto', { on })} />
          <p class="co-pgb-note" style="margin-top:6px">${s.auto.text}</p>
        </${Card}>
        <${Card} icon="construction" title=${s.mechanics.title}>
          <p class=${'co-pgb-text' + (s.mechanics.tone === 'warn' ? ' co-pgb-t-warn' : '')}>${s.mechanics.text}</p>
        </${Card}>
      </div>
    </div>
  </div>`;
}

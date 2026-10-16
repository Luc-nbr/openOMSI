// The company's finances (`company/money.rs`, `bank.rs`, `fares.rs`; Luc 2026-10-16, after the
// approved concept): five KPI cards - cash, this month, the gross margin, debt, the fleet's value
// -, the month chosen as a profit and loss (the waterfall from its income to its result, its
// income and costs by kind of booking with their shares), the loans with the credit room, the
// ticket prices, and the ledger with its filters and search - and the fare dialog with what each
// price does to passengers and fares.

import { html, useState, setCo, setUi, ui, act, useView, textW, Icon, Btn, Select, Dialog, Plate, Slider, seen } from './kit.js';
import { Card, Kpi, Kpis, Empty, HBars, Track, Tabs, Chip } from './pages-a.js';

export function MoneyPage() {
  const st = ui('money', { month: null });
  const v = useView('money', st);
  if (!v?.figures) return html`<div class="co-pg"></div>`;
  return html`<div class="co-pg co-pg-money">
    <div class="co-pg-stack">
      <${Kpis} n=${v.figures.length}>${v.figures.map(
        (f) => html`<${Kpi} f=${f} icon=${f.icon} onClick=${f.open ? () => setCo({ dialog: { kind: f.open } }) : undefined} />`,
      )}</${Kpis}>
      <div class=${'co-pg-moneygrid' + (v.easy ? ' easy' : '')}>
        <${Month} m=${v.month} />
        <div class="co-pg-stack">
          <${Loans} l=${v.loans} />
          ${v.fares && html`<${Fares} f=${v.fares} />`}
        </div>
      </div>
      ${v.ledger && html`<${Ledger} l=${v.ledger} />`}
    </div>
  </div>`;
}

/** The month as a profit and loss (`money::month_part`): the waterfall, its income and costs by
 * kind, the passengers, the investments and the deductions for late running. */
function Month({ m }) {
  const k = Math.max(0, m.months.findIndex(([x]) => x === m.month));
  const pick = html`<span class="co-pg-monthpick"><${Select} value=${k} h=${32} options=${m.months.map(([, l]) => l)} onChange=${(i) => setUi('money', { month: i > 0 ? m.months[i][0] : null })} /></span>`;
  return html`<${Card} title=${m.pl_title} sub=${m.pl_sub} icon="receipt_long" tools=${pick} class="co-pg-pl">
    ${m.empty
      ? html`<${Empty} icon="receipt_long" text=${m.empty} />`
      : html`<${Waterfall} steps=${m.waterfall} />
        <div class="co-pg-plsplit">
          <div>
            <div class="co-pg-plhead"><span class="co-pg-caps">${m.income_title}</span><b class="t-ok">${m.income_total}</b></div>
            ${m.income.length ? html`<${HBars} rows=${m.income} />` : html`<p class="co-pg-note">${m.no_income}</p>`}
          </div>
          <div>
            <div class="co-pg-plhead"><span class="co-pg-caps">${m.costs_title}</span><b class="t-danger">${m.costs_total}</b></div>
            ${m.costs.length ? html`<${HBars} rows=${m.costs} />` : html`<p class="co-pg-note">${m.no_costs}</p>`}
          </div>
        </div>
        <div class="co-pg-plresult"><span>${m.result_label}</span><b class=${'t-' + m.result_tone}>${m.result}</b></div>
        ${m.pax && html`<p class="co-pg-note"><${Icon} name="groups" size=${14} /> ${m.pax}</p>`}
        ${m.capital && html`<p class="co-pg-note">${m.capital}</p>`}
        ${m.late && html`<button type="button" class="late-how" style="margin-top:4px;align-self:flex-start" onClick=${() => setCo({ dialog: { kind: 'late' } })}><${Icon} name="schedule" size=${15} />${m.late}</button>`}`}
  </${Card}>`;
}

/** The waterfall from the month's income to its result: each cost a step down. */
function Waterfall({ steps }) {
  const ends = steps.flatMap((s) => [s.from, s.to]);
  const hi = Math.max(1, ...ends);
  const lo = Math.min(0, ...ends);
  const span = hi - lo || 1;
  const pct = (x) => ((x - lo) / span) * 100;
  return html`<div class="co-pg-fall" style=${`--n:${steps.length}`}>
    ${lo < 0 && html`<i class="co-pg-fallzero" style=${`bottom:${pct(0)}%`}></i>`}
    ${steps.map((s) => {
      const bottom = pct(Math.min(s.from, s.to));
      const height = Math.max(1.5, pct(Math.max(s.from, s.to)) - bottom);
      return html`<div class=${'co-pg-fallstep' + (s.total ? ' total' : '')} title=${`${s.label}: ${s.amount}`}>
        <div class="co-pg-fallplot">
          <span class="co-pg-fallbar" style=${`bottom:${bottom}%;height:${height}%;background-color:${s.tone === 'ok' ? 'rgb(var(--co-ok-fill))' : 'rgb(var(--co-danger-fill))'}`}></span>
          <b class=${'co-pg-fallvalue t-' + s.tone} style=${`bottom:calc(${bottom + height}% + 3px)`}>${s.amount}</b>
        </div>
        <span class="co-pg-falllabel">${s.label}</span>
      </div>`;
    })}
  </div>`;
}

/** The loans and the credit room (`money::loans`, `bank::credit_figure`). */
function Loans({ l }) {
  const cr = l.credit;
  return html`<${Card} title=${l.title} icon="description" tools=${html`<${Btn} kind="primary" icon="payments" label=${l.take} style="height:32px" onClick=${() => act('open_loan')} />`}>
    <div class="co-pg-credit">
      <div class="co-pg-line"><span class="co-pg-caps grow">${cr.caps}</span><b class=${'t-' + cr.tone}>${cr.value}</b></div>
      <div title=${cr.bar.tip}><${Track} frac=${cr.bar.frac} tone=${cr.tone} h=${10} /></div>
      <div class="co-pg-line co-pg-note"><span class="grow co-pg-cut">${cr.bar.label}</span><span>${cr.bar.right}</span></div>
      <p class=${'co-pg-note t-' + cr.tone}>${cr.say}</p>
      <details class="co-pg-creditlines"><summary>${cr.rate}</summary>${cr.lines.map(([a, b]) => html`<div class="co-pg-line"><span class="grow co-pg-cut">${a}</span><b>${b}</b></div>`)}</details>
    </div>
    <div class="co-pg-list" style="margin-top:12px">
      ${l.list.length
        ? l.list.map((x) => html`<div class="co-pg-item"><div class="co-pg-line"><b class="co-pg-itemtitle grow co-pg-cut">${x.what}</b>
            ${x.paper && html`<${Btn} icon="description" label=${l.contract} style="height:30px" onClick=${() => act('open_loan_contract', { no: x.id })} />`}
            <${Btn} label=${l.repay} style="height:30px" onClick=${() => act('open_repay', { id: x.id })} /></div><span class="co-pg-itemsub">${x.sub}</span></div>`)
        : html`<p class="co-pg-note">${l.none}</p>`}
    </div>
  </${Card}>`;
}

/** The ticket prices of the company's lines (`fares::card`). */
function Fares({ f }) {
  return html`<${Card} title=${f.title} icon="confirmation_number">
    ${f.empty
      ? html`<${Empty} small icon="confirmation_number" text=${f.empty} />`
      : html`<div class="co-pg-fares">${f.rows.map(
          (r) => html`<button type="button" class="co-pg-farerow" title=${f.tip} onClick=${() => setCo({ dialog: { kind: 'fare', line: r.name } })}>
            <${Plate} p=${r.plate} h=${22} /><b>${r.fare}</b><span class="co-pg-muted co-pg-cut">${r.vs}</span><span class=${'co-pg-cut t-' + r.tone}>${r.pax}</span><${Icon} name="chevron_right" size=${18} class="t-dim" /></button>`,
        )}</div>`}
  </${Card}>`;
}

/** The ledger, the newest first (`money::ledger`): all, income, costs or investments, and what
 * is searched for. */
function Ledger({ l }) {
  const [cat, setCat] = useState('all');
  const [q, setQ] = useState('');
  const want = q.trim().toLowerCase();
  const rows = l.rows.filter((b) => (cat === 'all' || b.cat === cat) && (!want || `${b.date} ${b.kind} ${b.text} ${b.amount}`.toLowerCase().includes(want)));
  const tools = html`<${Tabs} value=${cat} options=${l.filters} onChange=${setCat} />
    <label class="co-pg-search"><${Icon} name="search" size=${16} /><input type="search" value=${q} placeholder=${l.search} onInput=${(e) => setQ(e.currentTarget.value)} /></label>`;
  return html`<${Card} title=${l.title} icon="view_list" tools=${tools} class="co-pg-ledger">
    ${rows.length
      ? html`<div class="co-pg-tablewrap"><table class="co-pg-table">
          <colgroup><col style="width:17%" /><col style="width:18%" /><col /><col style="width:96px" /><col style="width:16%" /></colgroup>
          <thead><tr>${l.heads.map((h, k) => html`<th class=${k === 4 ? 'num' : ''}>${h}</th>`)}</tr></thead>
          <tbody>${rows.map(
            (b) => html`<tr><td class="co-pg-muted">${b.date}</td><td>${b.kind}</td><td title=${b.text}>${b.text}</td>
              <td>${b.mark && html`<${Chip} text=${b.mark[0]} tone=${b.mark[1]} title=${b.mark[2]} />`}</td><td class=${'num t-' + b.tone}><b>${b.amount}</b></td></tr>`,
          )}</tbody>
        </table></div>`
      : html`<${Empty} small icon="search" text=${l.none} />`}
  </${Card}>`;
}

/** A line's fare (`fares::dialog`): the band, the slider, what each price does, what the fare
 * chosen comes to. */
export function FareDialog({ d }) {
  const [fare, setFare] = useState(d.fare ?? null);
  const v = useView('fare', { line: d.line, fare });
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  if (v.gone) {
    close();
    return null;
  }
  const set = async () => {
    const r = await act('set_fare', { line: d.line, fare: v.fare, casual: !!d.casual });
    if (r?.done) close();
  };
  const foot = html`<${Btn} kind="ghost" label=${v.p0_label} onClick=${() => setFare(v.p0)} /><${Btn} kind="ghost" label=${v.best_label} onClick=${() => setFare(v.best)} />
    <span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} /><${Btn} kind="primary" icon="check_circle" label=${v.set} onClick=${set} />`;
  return html`<${Dialog} icon="confirmation_number" title=${v.title} w=${860} h=${700} onClose=${close} foot=${foot}>
    <p class="co-para" style="margin-bottom:14px">${v.band}</p>
    <${Slider} label=${v.slider} value=${v.fare / 100} min=${v.lo / 100} max=${v.hi / 100} step=${0.05} format=${() => v.fare_text} onChange=${(x) => setFare(Math.round(x * 20) * 5)} style="height:40px" />
    ${v.table && html`<div class="co-faretable">${v.table.map(([l, t]) => html`<span>${l} <b>${t}</b></span>`)}</div>`}
    ${v.cap && html`<label class="co-farecap"><input type="checkbox" checked=${v.cap.on} onChange=${(e) => act('set_cap', { line: d.line, on: e.currentTarget.checked })} /><span>${v.cap.label}</span></label>`}
    ${v.chart && html`<${Curves} c=${v.chart} v=${v} />`}
    <p class="co-para" style="color:var(--hub-text);margin-top:${v.chart ? 16 : 14}px">${v.says}</p>
    ${v.warn && html`<div class="co-iconline t-warn" style="font-size:var(--co-body);margin-top:8px"><${Icon} name="warning" size=${18} /><span>${v.warn}</span></div>`}
    ${v.now && html`<p class="co-note" style="margin-top:6px">${v.now}</p>`}
  </${Dialog}>`;
}

/** Passengers and fares a day over the band of prices (`fares::curves`). */
function Curves({ c, v }) {
  const W = 804;
  const H = 230;
  const plot = { x: 70, y: 52, w: W - 140, h: H - 86 };
  const pts = c.points;
  const maxPax = Math.max(1, ...pts.map((p) => p[1])) * 1.08;
  const maxRev = Math.max(1, ...pts.map((p) => p[2])) * 1.08;
  const xOf = (p) => plot.x + (plot.w * (p - v.lo)) / Math.max(1, v.hi - v.lo);
  const line = (k, max) => pts.map((p) => `${xOf(p[0])},${plot.y + plot.h - (plot.h * p[k]) / max}`).join(' ');
  const pax = '#7aa8ff';
  const x = xOf(v.fare);
  return html`<div style=${`position:relative;width:${W}px;height:${H}px;margin-top:14px`}>
    <svg width=${W} height=${H} style="position:absolute;left:0;top:0;overflow:visible">
      ${[0, 1, 2, 3, 4].map((k) => html`<rect x=${plot.x} y=${plot.y + (plot.h * k) / 4} width=${plot.w} height="1" style=${`fill:rgba(var(--co-white), ${seen(k === 4 ? 0.2 : 0.06)})`} />`)}
      <polyline points=${line(1, maxPax)} fill="none" stroke=${pax} stroke-width="2.5" stroke-linejoin="round" />
      <polyline points=${line(2, maxRev)} fill="none" stroke="rgb(var(--co-ok-fill))" stroke-width="2.5" stroke-linejoin="round" />
      ${[
        [v.p0, 'var(--hub-text-dim)'],
        [v.best, 'rgb(var(--co-ok-fill))'],
      ].map(([p, col]) => html`<rect x=${xOf(p)} y=${plot.y} width="1" height=${plot.h} fill=${col} opacity="0.7" />`)}
      <rect x=${x - 1} y=${plot.y} width="2" height=${plot.h} fill="var(--hub-accent)" opacity="0.8" />
      <circle cx=${x} cy=${plot.y + plot.h - (plot.h * c.pax) / maxPax} r="5.5" fill=${pax} />
      <circle cx=${x} cy=${plot.y + plot.h - (plot.h * c.rev) / maxRev} r="5.5" fill="rgb(var(--co-ok-fill))" />
      <rect x="0" y="17" width="16" height="4" rx="2" fill=${pax} />
      <rect x=${30 + textW(c.pax_label, 13) + 12} y="17" width="16" height="4" rx="2" fill="rgb(var(--co-ok-fill))" />
    </svg>
    <span class="co-axis" style=${`left:22px;top:10px;color:${pax};font-weight:700`}>${c.pax_label}</span>
    <span class="co-axis t-ok" style=${`left:${52 + textW(c.pax_label, 13) + 12}px;top:10px;font-weight:700`}>${c.rev_label}</span>
    ${c.scale.map(([p, r], k) => html`<span class="co-axis" style=${`left:6px;width:58px;text-align:right;top:${plot.y + (plot.h * k) / 4 - 9}px;color:${pax}`}>${p}</span>
      <span class="co-axis t-ok" style=${`left:${plot.x + plot.w + 6}px;top:${plot.y + (plot.h * k) / 4 - 9}px`}>${r}</span>`)}
    <span class="co-axis" style=${`left:${xOf(v.p0) + 8}px;top:${plot.y + 2}px;font-weight:700`}>${c.p0_mark}</span>
    <span class="co-axis t-ok" style=${`left:${xOf(v.best) + 8}px;top:${plot.y + 20}px;font-weight:700`}>${c.best_mark}</span>
    <span class="co-axis" style=${`left:${plot.x - 40}px;width:80px;text-align:center;top:${plot.y + plot.h + 8}px`}>${c.lo_text}</span>
    <span class="co-axis" style=${`left:${plot.x + plot.w - 40}px;width:80px;text-align:center;top:${plot.y + plot.h + 8}px`}>${c.hi_text}</span>
    <span class="co-axis t-accent" style=${`left:${x - 40}px;width:80px;text-align:center;top:${plot.y + plot.h + 8}px;font-weight:700`}>${v.fare_text}</span>
  </div>`;
}

/** The fleet's value by age band (`age::fleet_value_view`): per band its buses, what they cost
 * new, are worth and lost; a band's buses and how each value is worked out on hover. */
export function FleetValueDialog() {
  const v = useView('fleet_value', {});
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  const cols = 'grid-template-columns:minmax(0,1.1fr) 70px repeat(3,minmax(0,1fr));gap:12px;align-items:center';
  const cell = (x, k) => html`<span class="cut" style=${k ? 'text-align:right' : ''}>${x}</span>`;
  const foot = html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`;
  return html`<${Dialog} icon="payments" title=${v.title} w=${860} onClose=${close} foot=${foot}>
    <div class="co-caps" style=${'display:grid;' + cols + ';padding:0 4px 6px'}>${v.head.map(cell)}</div>
    ${v.rows.map((r) => html`<div class="co-monthrow" title=${r.tip || undefined} style=${'display:grid;' + cols}>${[r.band, r.buses, r.new, r.now, r.lost].map(cell)}</div>`)}
    <div class="co-monthrow result" style=${'display:grid;' + cols}>${v.total.map(cell)}</div>
    <p class="co-note" style="margin-top:14px">${v.note}</p>
    ${v.other && html`<p class="co-note" style="margin-top:6px">${v.other}</p>`}
  </${Dialog}>`;
}

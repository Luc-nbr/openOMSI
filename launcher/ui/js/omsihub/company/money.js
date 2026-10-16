// The company's finances, as our game-drawn launcher's (`company/money.rs`, `bank.rs`,
// `fares.rs`): five figures, the month by kind of booking, the loans with the credit room, the
// ledger, the ticket prices - and the fare dialog with what each price does to passengers and
// fares.

import { html, useState, setCo, setUi, ui, act, useView, useSize, textW, Icon, Btn, Section, Figure, Select, Dialog, Plate, Slider, seen } from './kit.js';
import { Bar } from './overview.js';

export function MoneyPage() {
  const st = ui('money', { month: null });
  const v = useView('money', st);
  const [ref, size] = useSize();
  if (!v?.figures) return html`<div ref=${ref}></div>`;
  const gap = 14;
  const box = (x, y, w, h) => `position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
  let body = null;
  let tall = 0;
  if (size.w > 0) {
    const n = v.figures.length;
    const fw = (size.w - (n - 1) * gap) / n;
    const y = 100 + gap;
    if (v.easy) {
      const h = Math.max(260, size.h - y);
      tall = y + h;
      const cw = (size.w - gap) / 2;
      body = html`${v.figures.map((f, k) => html`<div style=${box(k * (fw + gap), 0, fw, 100)}><${Figure} f=${f} /></div>`)}
        <${Month} m=${v.month} style=${box(0, y, cw, h)} />
        <${Loans} l=${v.loans} style=${box(cw + gap, y, cw, h)} w=${cw - 36} />`;
    } else {
      const h1 = Math.max(260, (size.h - y - gap) * 0.58);
      const wide = size.w >= 1500;
      const cols = wide ? 3 : 2;
      const cw = (size.w - (cols - 1) * gap) / cols;
      const y2 = y + h1 + gap;
      const h2 = Math.max(100, size.h - y2);
      tall = y2 + h2;
      const lw = (size.w - gap) * 0.62;
      body = html`${v.figures.map((f, k) => html`<div style=${box(k * (fw + gap), 0, fw, 100) + (f.open ? ';cursor:pointer' : '')} title=${f.tip} onClick=${f.open ? () => setCo({ dialog: { kind: f.open } }) : undefined}><${Figure} f=${f} small=${fw < 170} /></div>`)}
        <${Month} m=${v.month} style=${box(0, y, cw, h1)} />
        <${Loans} l=${v.loans} style=${box(cw + gap, y, cw, h1)} w=${cw - 36} />
        ${wide
          ? html`<${Fares} f=${v.fares} style=${box(2 * (cw + gap), y, cw, h1)} /><${Ledger} l=${v.ledger} style=${box(0, y2, size.w, h2)} />`
          : html`<${Ledger} l=${v.ledger} style=${box(0, y2, lw, h2)} /><${Fares} f=${v.fares} style=${box(lw + gap, y2, size.w - lw - gap, h2)} />`}`;
    }
  }
  // (the cards keep their least heights: on a low window the page scrolls to them - Beta 53's
  // layout check)
  return html`<div ref=${ref} style="position:relative;height:100%;overflow-x:hidden;overflow-y:auto"><div style=${`position:relative;height:${Math.max(tall, 1)}px`}>${body}</div></div>`;
}

/** The month by kind of booking, and its result (`money::month_overview`). */
function Month({ m, style }) {
  const k = Math.max(0, m.months.findIndex(([x]) => x === m.month));
  return html`<${Section} title=${m.title} style=${style}
    tools=${html`<span style="width:200px;margin:-7px 0 -9px;text-transform:none;font-weight:400"><${Select} value=${k} h=${32} options=${m.months.map(([, l]) => l)} onChange=${(i) => setUi('money', { month: i > 0 ? m.months[i][0] : null })} /></span>`}>
    ${m.empty
      ? html`<p class="co-para">${m.empty}</p>`
      : html`<div class="co-scroll" style="flex:1;min-height:0">
          ${m.rows.map(([label, t, tone]) => html`<div class="co-monthrow"><span class="cut">${label}</span><b class=${'t-' + tone}>${t}</b></div>`)}
          <div class="co-monthrow result"><span>${m.result_label}</span><b class=${'t-' + m.result_tone}>${m.result}</b></div>
          ${m.pax && html`<p class="co-note" style="margin-top:8px">${m.pax}</p>`}
          ${m.capital && html`<p class="co-note" style="margin-top:12px">${m.capital}</p>`}
          ${m.late && html`<button type="button" class="late-how" style="margin-top:10px" onClick=${() => setCo({ dialog: { kind: 'late' } })}><${Icon} name="schedule" size=${15} />${m.late}</button>`}
        </div>`}
  </${Section}>`;
}

/** The loans and the credit room (`money::loans`, `bank::credit_figure`). */
function Loans({ l, style, w }) {
  const cr = l.credit;
  const narrow = w < 520;
  return html`<${Section} title=${l.title} style=${style}>
    <div class="co-credit" style=${narrow ? 'grid-template-columns:1fr' : ''}>
      <div>
        <div class="co-caps">${cr.caps}</div>
        <div class=${'value t-' + cr.tone}>${cr.value}</div>
        <div class=${'say t-' + cr.tone}>${cr.say}</div>
        <div style="margin-top:6px;padding-right:10px"><${Bar} b=${{ ...cr.bar, tone: cr.tone }} /></div>
      </div>
      ${!narrow && html`<div class="lines">${cr.lines.map(([a, b]) => html`<div><span class="cut">${a}</span><b>${b}</b></div>`)}<p>${cr.rate}</p></div>`}
    </div>
    <div class="co-scroll" style="flex:1;min-height:0;border-top:1px solid var(--hub-hairline);margin-top:17px;padding-top:6px">
      ${l.list.length
        ? l.list.map((x) => html`<div class="co-loanrow"><div class="grow"><b class="cut">${x.what}</b><span class="cut">${x.sub}</span></div>
            ${x.paper && html`<${Btn} icon="description" label=${l.contract} style="height:34px" onClick=${() => act('open_loan_contract', { no: x.id })} />`}
            <${Btn} label=${l.repay} style="height:34px" onClick=${() => act('open_repay', { id: x.id })} /></div>`)
        : html`<p class="co-para" style="margin-top:4px">${l.none}</p>`}
    </div>
    <div style="display:flex;justify-content:flex-end;flex:none;margin-top:12px"><${Btn} kind="primary" icon="payments" label=${l.take} style="height:38px" onClick=${() => act('open_loan')} /></div>
  </${Section}>`;
}

/** The ledger, the newest first (`money::ledger`). */
function Ledger({ l, style }) {
  if (!l) return null;
  return html`<${Section} title=${l.title} style=${style}>
    <div class="co-scroll" style="flex:1;min-height:0;margin:-4px -4px -12px">
      ${l.rows.map((b) => html`<div class="co-ledgerrow"><span class="cut">${b.date}</span><span class="cut kind">${b.kind}</span><span class="cut text">${b.text}</span>
        <span class=${'mark cut' + (b.mark ? ' t-' + b.mark[1] : '')} title=${b.mark?.[2]}>${b.mark?.[0] || ''}</span><b class=${'t-' + b.tone}>${b.amount}</b></div>`)}
    </div>
  </${Section}>`;
}

/** The ticket prices of the company's lines (`fares::card`). */
function Fares({ f, style }) {
  if (!f) return null;
  return html`<${Section} title=${f.title} style=${style}>
    ${f.empty
      ? html`<p class="co-para">${f.empty}</p>`
      : html`<div class="co-scroll" style="flex:1;min-height:0">${f.rows.map(
          (r) => html`<button type="button" class="co-row co-farerow" title=${f.tip} onClick=${() => setCo({ dialog: { kind: 'fare', line: r.name } })}>
            <${Plate} p=${r.plate} h=${22} style="margin-left:6px" /><b>${r.fare}</b><span class="vs">${r.vs}</span><span class=${'pax t-' + r.tone}>${r.pax}</span><${Icon} name="chevron_right" size=${18} class="t-dim" /></button>`,
        )}</div>`}
  </${Section}>`;
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

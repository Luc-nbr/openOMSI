// The tenders (`company/concessions.rs`; Luc 2026-10-16, after the approved concept): the open
// tenders as cards - the contract they let, their tours, km a week and the authority's cap as
// chips, a countdown, the one picked in the accent -, the tender picked beside them as it runs
// with the company's clock - its timeline (open, bidding, award), the cap, the advice and your
// ask, how the asks are scored (70 % price, 30 % quality, your expected score as a bar), the
// rivals with their kind, quality and bid, placing the bid, buying the line outright, the clock
// stepped on for it - and below, your contracts with their rating and default points.

import { html, ui, setUi, Icon, Btn, Plate, Slider } from './kit.js';
import { useMore, actMore, eur } from './more.js';
import { units, cents } from '../money.js';
import { Card, Empty, Chip, Dot, Track } from './pages-a.js';
import { VaryDialog, openVary } from './variations.js';

/** A small badge (`Ui::badge`): its words in capitals on a tint of its tone. */
export const Badge = ({ b }) => b && html`<${Chip} text=${b.text} tone=${b.tone} />`;

export function ConcessionsPage() {
  const st = ui('concessions', { amount: null, amount_for: null });
  const v = useMore('concessions', { amount: st.amount, amount_for: st.amount_for });
  if (!v?.tenders) return html`<div class="co-pg"></div>`;
  const t = v.tenders;
  return html`<div class="co-pg co-pg-tenders">
    <div class="co-pg-stack">
      ${t.note && html`<div class="co-pg-banner2"><${Icon} name="info" size=${18} /><span>${t.note}</span></div>`}
      <div class="co-pg-tendergrid">
        <${Card} title=${t.title} icon="campaign" tools=${t.count > 0 && html`<${Chip} text=${String(t.count)} tone="accent" />`} class="co-pg-tenderlist">
          ${t.empty
            ? html`<${Empty} icon="campaign" text=${t.empty} />`
            : html`<div class="co-pg-list">${t.rows.map((r) => html`<${Tender} r=${r} />`)}</div>`}
        </${Card}>
        <${Auction} a=${v.auction} st=${st} />
      </div>
      <${Held} h=${v.held} />
    </div>
    <${VaryDialog} />
  </div>`;
}

/** A tender's card: its plate and name, its chips, its countdown, its state and your bid. */
function Tender({ r }) {
  return html`<button type="button" class=${'cm-tender co-pg-tender' + (r.on ? ' on' : '') + (r.dim ? ' past' : '')} onClick=${() => actMore('tender_pick', { id: r.id })}>
    <div class="co-pg-line">
      <${Plate} p=${{ number: r.number }} h=${24} />
      <b class="co-pg-itemtitle grow co-pg-cut" title=${r.caption}>${r.caption}</b>
      ${r.badge && html`<${Badge} b=${r.badge} />`}
    </div>
    <div class="co-pg-chips">${(r.chips || []).map((c) => html`<${Chip} text=${c.text} tone=${c.tone} icon=${c.icon} />`)}</div>
    <div class="co-pg-line">
      <span class=${'co-pg-cut grow co-pg-small t-' + r.tone} title=${r.state}>${r.state}</span>
      ${r.countdown && html`<span class=${'co-pg-countdown t-' + r.countdown[1]}><${Icon} name="timer" size=${14} />${r.countdown[0]}</span>`}
    </div>
    <span class=${'co-pg-cut co-pg-small t-' + r.bid_tone} title=${r.bid}>${r.bid}</span>
  </button>`;
}

/** The tender picked (`concessions::auction`). */
function Auction({ a, st }) {
  if (a.none) return html`<${Card} title=${a.title} icon="sports_score"><${Empty} icon="touch_app" text=${a.none} /></${Card}>`;
  const act = a.actions;
  const running = act.kind === 'running';
  const amount = running ? (st.amount_for === act.id && st.amount != null ? Math.min(act.hi, Math.max(act.lo, st.amount)) : act.amount) : null;
  const setAmount = (x) => setUi('concessions', { amount: Math.min(act.hi, Math.max(act.lo, x)), amount_for: act.id });
  const shown = running ? a.figures.slice(0, 2) : a.figures;
  return html`<${Card} class="co-pg-auction">
    <div class="co-pg-line co-pg-auctionhead">
      <${Plate} p=${{ number: a.number }} h=${28} />
      <div class="grow" style="min-width:0"><b class="co-pg-auctiontitle co-pg-cut" title=${a.caption}>${a.caption}</b><span class=${'co-pg-small co-pg-cut t-' + a.tone}>${a.state}</span></div>
    </div>
    ${a.terms && html`<p class="co-pg-note">${a.terms}</p>`}
    ${a.mine && html`<p class=${'co-pg-small t-' + a.mine_tone} style="font-weight:700;margin:0">${a.mine}</p>`}
    <${Timeline} steps=${a.timeline} frac=${a.frac} times=${a.times} />
    <div class="co-pg-tiles">
      ${shown.map((f) => html`<div class=${'co-pg-tile' + (f.tone === 'ok' ? ' good' : '')} title=${f.tip || ''}><span>${f.label}</span><b class=${'t-' + f.tone}>${f.value}</b></div>`)}
      ${running &&
      html`<div class="co-pg-tile bid"><span>${act.label}</span>
        <input class="co-pg-bidin" type="number" step="100" value=${Math.round(units(amount * 100))} onChange=${(e) => setAmount(cents(e.currentTarget.value) / 100)} />
        <${Slider} value=${amount} min=${act.lo} max=${act.hi} step=${100} format=${(x) => eur(Math.round(x * 100))} onChange=${setAmount} /></div>`}
    </div>
    ${running && html`<p class=${'co-pg-small t-' + act.chance_tone} style="margin:0;font-weight:700">${act.chance}</p>`}
    ${running && act.recommend && html`<button type="button" class="co-pg-linkbtn" onClick=${() => setAmount(act.recommend_amount)}><${Icon} name="check_circle" size=${15} />${act.recommend}</button>`}
    ${a.score && html`<${Score} s=${a.score} />`}
    ${a.rivals.length > 0 &&
    html`<div><div class="co-pg-caps" style="margin-bottom:6px">${a.rivals_title}</div>
      <table class="co-pg-table co-pg-rivals">
        <colgroup><col /><col style="width:20%" /><col style="width:24%" /><col style="width:26%" /></colgroup>
        <thead><tr>${a.rivals_heads.map((h, k) => html`<th class=${k === 3 ? 'num' : ''}>${h}</th>`)}</tr></thead>
        <tbody>${a.rivals.map(
          (r) => html`<tr class=${r.leads ? 'leads' : ''} title=${r.points || ''}><td><b>${r.name}</b></td><td class="co-pg-muted">${r.kind}</td>
            <td><span class="co-pg-line"><${Track} frac=${r.quality} tone=${r.quality >= 0.65 ? 'ok' : r.quality >= 0.4 ? 'warn' : 'danger'} h=${6} /><span class="co-pg-small">${r.quality_text}</span></span></td>
            <td class=${'num t-' + r.tone}>${r.bid}</td></tr>`,
        )}</tbody>
      </table></div>`}
    <div class="co-pg-actions">
      ${running &&
      html`<${Btn} kind="primary" icon="payments" label=${act.bid} onClick=${() => actMore('tender_bid', { id: act.id, amount: Math.round(amount * 100) })} />
        <${Btn} icon="check_circle" label=${act.buy} title=${act.buy_tip} onClick=${() => actMore('tender_buy', { id: act.id })} />
        <${Btn} kind="ghost" icon="timer" label=${act.plus15} onClick=${() => actMore('tender_step', { id: act.id, to: '15' })} />
        <${Btn} kind="ghost" icon="sports_score" label=${act.to_close} onClick=${() => actMore('tender_step', { id: act.id, to: 'close' })} />`}
      ${act.kind === 'wait' && html`<${Btn} kind="primary" icon="schedule" label=${act.label} onClick=${() => actMore('tender_step', { id: act.id, to: 'open' })} />`}
      ${act.kind === 'said' && act.text && html`<p class=${'co-pg-small t-' + act.tone} style="margin:0;font-weight:700">${act.text}</p>`}
    </div>
    <details class="co-pg-log"><summary>${a.log_title} (${a.log.length})</summary>
      ${a.log.length
        ? a.log.map((l) => html`<div class="co-pg-line"><span class="co-pg-muted" style="width:48px">${l.at}</span><span class=${'grow co-pg-cut' + (l.me ? ' t-accent' : '')}>${l.who}</span><b>${l.amount}</b></div>`)
        : html`<p class="co-pg-note">${a.no_log}</p>`}
    </details>
  </${Card}>`;
}

/** Open, bidding, award: where the tender stands, the bidding's progress under it. */
function Timeline({ steps, frac, times }) {
  return html`<div class="co-pg-timeline">
    <div class="co-pg-steps">${steps.map(
      (s, k) => html`<div class=${'co-pg-step ' + s.state}><span class="co-pg-stepdot">${s.state === 'done' ? html`<${Icon} name="check" size=${14} />` : k + 1}</span><b>${s.label}</b><span class="co-pg-small co-pg-muted co-pg-cut">${s.sub}</span></div>`,
    )}</div>
    <div class="co-pg-line"><${Track} frac=${frac} h=${6} /><span class="co-pg-small co-pg-muted" style="flex:none">${times}</span></div>
  </div>`;
}

/** How the asks are scored, and the score your ask would get: 70 price points, 30 quality. */
function Score({ s }) {
  return html`<div class="co-pg-score">
    <div class="co-pg-line"><${Icon} name="info" size=${16} class="t-accent" /><b class="grow">${s.title}</b><span class="co-pg-chip t-accent" style="background:rgba(var(--hub-accent-rgb),0.14)">${s.head}</span></div>
    <div class="co-pg-stacked" title=${s.total}>
      <i class="price" style=${`width:${s.price}%`}></i><i class="quality" style=${`width:${s.quality}%`}></i>
    </div>
    <div class="co-pg-line co-pg-small"><span class="co-pg-key price"></span><span>${s.price_label}</span><span class="co-pg-key quality"></span><span class="grow">${s.quality_label}</span><b>${s.total}</b></div>
    <ul class="co-pg-points">${s.lines.map((l) => html`<li>${l}</li>`)}</ul>
  </div>`;
}

/** The company's contracts: its concessions and own lines, each with its rating's dot, its
 * payment and its default points (`concessions::held`), and a change to request of a tendered
 * one (`variations.js`). */
function Held({ h }) {
  const rows = [...h.rows.map((r) => ({ ...r, own: false })), ...h.own.map((r) => ({ ...r, own: true }))];
  return html`<${Card} title=${h.title} sub=${h.sub} icon="description">
    ${h.empty
      ? html`<${Empty} small icon="description" text=${h.empty} />`
      : html`<div class="co-pg-heldgrid">${rows.map((r) => {
          const badge = r.rag || (r.direct && { text: r.direct, tone: 'dim' });
          return html`<div class="co-pg-item">
            <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${22} />
              <b class="co-pg-itemtitle grow co-pg-cut">${r.caption || r.number}</b>
              ${r.rag ? html`<${Dot} tone=${r.rag.tone} title=${r.rag.text} /><span class=${'co-pg-small t-' + r.rag.tone} style="font-weight:800">${r.rag.text}</span>` : badge && html`<${Badge} b=${badge} />`}
            </div>
            ${!r.own && html`<span class=${'co-pg-itemsub t-' + r.tone}>${r.until}</span>`}
            <span class="co-pg-itemsub">${r.own ? r.text : r.price}</span>
            ${r.points && html`<span class="co-pg-itemsub"><${Icon} name="flag" size=${13} /> ${r.points}</span>`}
            ${r.unpaid && html`<span class="co-pg-itemsub">${r.unpaid}</span>`}
            ${r.change && html`<div><${Btn} icon="edit_road" label=${r.change} style="height:30px;padding:0 10px" onClick=${() => openVary(r.line)} /></div>`}
          </div>`;
        })}</div>`}
  </${Card}>`;
}

// Goals and contracts (`company/contracts.rs`; Luc 2026-10-16, after his reference), City Bus
// Manager's assignments and service contracts: a strip of the trophies to spend, the rating in
// stars, the goals running and the next milestone; then, as pill tabs, the week's goals as a
// board (on offer, running, finished), the service contracts (the buses' internet from one of
// four providers, a supplier's fixed diesel price, an outside cleaning company), the tenders
// the company bid on, or its bus contracts (`bus_contracts`): kind, payment and days, term and
// extensions, the rating with its points, the journeys not paid, and the track record the next
// tenders score.

import { html, ui, setUi, Icon, Btn, Plate, Slider, goTab, fill } from './kit.js';
import { useMore, actMore } from './more.js';
import { Card, Empty, Chip, Dot, Track, Stars } from './pages-a.js';
import { Badge } from './concessions.js';
import { VaryDialog, openVary } from './variations.js';

export function ContractsPage() {
  const st = ui('contracts', { part: 0, gb: null });
  const v = useMore('contracts', { part: st.part, gb: st.gb });
  if (!v?.figures) return html`<div class="co-pg"></div>`;
  const f = v.figures;
  const b = v.body;
  return html`<div class="co-pg co-pg-contracts">
    <div class="co-pg-stack">
      <div class="co-pg-strip">
        <div class="co-pg-stripcell" title=${f.trophies.tip}><span class="co-pg-stripicon t-warn" style=${`background:${fill('warn', 0.16)}`}><${Icon} name="emoji_events" size=${20} /></span>
          <div><span>${f.trophies.label}</span><b>${f.trophies.value}</b></div></div>
        <div class="co-pg-stripcell" title=${f.rating.tip}><span class="co-pg-stripicon t-warn" style=${`background:${fill('warn', 0.16)}`}><${Icon} name="star" size=${20} /></span>
          <div><span>${f.rating.label}</span><b>${f.rating.value}</b><${Stars} rating=${f.rating.stars} size=${14} /></div></div>
        <div class="co-pg-stripcell" title=${f.running.under}><span class="co-pg-stripicon t-accent" style=${`background:${fill('accent', 0.14)}`}><${Icon} name="flag" size=${20} /></span>
          <div><span>${f.running.label}</span><b>${f.running.value}</b></div></div>
        <div class="co-pg-stripcell wide" title=${f.milestone.bar.tip}><span class="co-pg-stripicon t-ok" style=${`background:${fill('ok', 0.14)}`}><${Icon} name="military_tech" size=${20} /></span>
          <div><span>${f.milestone.label}</span><b>${f.milestone.bar.label}</b><${Track} frac=${f.milestone.bar.frac} tone="accent" h=${7} /></div></div>
      </div>
      <div><h1 class="co-pg-title">${v.title}</h1><p class="co-pg-lead">${v.lead}</p></div>
      <div class="co-pg-pills" role="tablist">${v.parts.map(
        (p, k) => html`<button type="button" role="tab" class=${k === v.part ? 'on' : ''} onClick=${() => k !== v.part && setUi('contracts', { part: k })}><${Icon} name=${v.icons[k]} size=${16} />${p}</button>`,
      )}</div>
      ${b.kind === 'goals'
        ? html`<${Goals} g=${b} />`
        : b.kind === 'services'
          ? html`<${Services} s=${b} />`
          : b.kind === 'buses'
            ? html`<${BusContracts} b=${b} />`
            : html`<${Bids} b=${b} />`}
    </div>
    <${VaryDialog} />
  </div>`;
}

/** A bus drawn small, for the empty columns. */
const BusArt = () => html`<svg class="co-pg-busart" viewBox="0 0 120 70" width="120" height="70" aria-hidden="true">
  <ellipse cx="60" cy="64" rx="50" ry="4" class="shade" />
  <rect x="10" y="10" width="100" height="46" rx="9" class="body" />
  <rect x="10" y="40" width="100" height="6" class="stripe" />
  <rect x="18" y="17" width="16" height="16" rx="3" class="glass" /><rect x="38" y="17" width="16" height="16" rx="3" class="glass" /><rect x="58" y="17" width="16" height="16" rx="3" class="glass" />
  <rect x="80" y="17" width="12" height="30" rx="2" class="glass" /><rect x="96" y="17" width="9" height="16" rx="3" class="glass" />
  <circle cx="32" cy="56" r="7" class="wheel" /><circle cx="88" cy="56" r="7" class="wheel" />
  <circle cx="32" cy="56" r="2.5" class="hub" /><circle cx="88" cy="56" r="2.5" class="hub" />
</svg>`;

/** A board's column: its icon, title and count, its cards, and a bus with a line when empty. */
function Column({ cls, icon, title, tools, n, empty, children }) {
  return html`<section class=${'co-pg-col ' + cls}>
    <header><span class="co-pg-colicon"><${Icon} name=${icon} size=${18} /></span><b class="grow co-pg-cut">${title}</b>${tools}<span class="co-pg-count">${n}</span></header>
    ${n ? html`<div class="co-pg-list">${children}</div>` : html`<div class="co-pg-colempty"><${BusArt} /><p>${empty}</p></div>`}
  </section>`;
}

/** A goal's icon in its round tint. */
const GoalIcon = ({ c }) => html`<span class=${'co-pg-goalicon t-' + c.tone} style=${`background:${fill(c.tone, 0.15)}`}><${Icon} name=${c.icon} size=${18} /></span>`;

/** A goal's reward as chips: its trophies, its money. */
const Rewards = ({ c }) => html`<div class="co-pg-chips"><${Chip} icon="emoji_events" text=${c.trophies} tone="accent" /><${Chip} icon="payments" text=${c.reward} tone="ok" /></div>`;

function Goals({ g }) {
  const o = g.offered;
  return html`<div class="co-pg-board">
    <${Column} cls="offer" icon="campaign" title=${o.title} n=${o.cards.length} empty=${o.empty} tools=${o.until && html`<span class="co-pg-small co-pg-muted">${o.until}</span>`}>
      ${o.cards.map(
        (c) => html`<div class="cm-goal co-pg-goal">
          <div class="co-pg-goalhead"><${GoalIcon} c=${c} /><div class="grow"><b>${c.title}</b><p>${c.rule}</p></div></div>
          <${Rewards} c=${c} />
          <div class="co-pg-line"><span class="co-pg-small co-pg-muted grow"><${Icon} name="schedule" size=${13} /> ${c.days}</span>
            <${Btn} kind=${c.primary ? 'primary' : ''} icon="flag" label=${c.take} style="height:32px" onClick=${() => actMore('goal_take', { id: c.id })} /></div>
        </div>`,
      )}
    </${Column}>
    <${Column} cls="running" icon="autorenew" title=${g.running.title} n=${g.running.cards.length} empty=${g.running.empty}>
      ${g.running.cards.map(
        (c) => html`<div class="cm-goal running co-pg-goal">
          <div class="co-pg-goalhead"><${GoalIcon} c=${c} /><div class="grow"><b>${c.title}</b><p title=${c.rule}>${c.rule}</p></div></div>
          <div class="co-pg-progress"><${Track} frac=${c.bar.frac} tone=${c.bar.tone} h=${8} /><span class="co-pg-small">${c.bar.label}</span></div>
          <${Rewards} c=${c} />
          <div class="co-pg-line"><span class=${'co-pg-small grow t-' + c.when_tone}><${Icon} name="schedule" size=${13} /> ${c.when}</span>
            <${Btn} kind="ghost" label=${c.give_up} style="height:30px" onClick=${() => actMore('goal_give_up', { id: c.id })} /></div>
        </div>`,
      )}
    </${Column}>
    <${Column} cls="done" icon="check_circle" title=${g.finished.title} n=${g.finished.rows.length} empty=${g.finished.empty}>
      ${g.finished.rows.map(
        (r) => html`<div class=${'co-pg-goal finished' + (r.done ? '' : ' missed')}>
          <span class=${'co-pg-goalmark ' + (r.done ? 't-ok' : 't-danger')}><${Icon} name=${r.done ? 'check_circle' : 'close'} size=${20} /></span>
          <div class="co-pg-goalhead"><${GoalIcon} c=${r} /><div class="grow"><b>${r.title}</b><p>${r.what}</p></div></div>
          ${r.done && html`<${Rewards} c=${r} />`}
          ${r.ago && html`<span class="co-pg-small co-pg-muted"><${Icon} name="schedule" size=${13} /> ${r.ago}</span>`}
        </div>`,
      )}
    </${Column}>
  </div>`;
}

// --- the service contracts ---------------------------------------------------------------------

function Services({ s }) {
  return html`<div class="co-pg-services">
    <${Card} title=${s.internet.title} icon="wifi" class="co-pg-internet"><${Internet} n=${s.internet} /></${Card}>
    <div class="co-pg-stack">
      <${Card} title=${s.fuel.title} icon="water_drop"><${Fuel} f=${s.fuel} /></${Card}>
      <${Card} title=${s.cleaning.title} icon="garage"><${Cleaning} c=${s.cleaning} /></${Card}>
    </div>
  </div>`;
}

/** The buses' internet: the four providers side by side (`contracts::internet`). */
function Internet({ n }) {
  if (n.locked) {
    return html`<${Empty} icon="wifi" text=${n.locked}><${Btn} kind="primary" icon="hub" label=${n.to_research} onClick=${() => goTab(10)} /></${Empty}>`;
  }
  const setGb = (k, x) => {
    const gb = n.providers.map((p) => p.gb);
    gb[k] = x;
    setUi('contracts', { gb });
  };
  return html`<p class="co-pg-note" style="margin-bottom:12px"><${Icon} name="info" size=${14} /> ${n.head}</p>
    <div class="co-pg-providers">${n.providers.map(
      (p, k) => html`<div class=${'cm-provider co-pg-provider' + (p.mine ? ' mine' : '')}>
        <span class=${'co-pg-goalicon ' + (p.mine ? 't-accent' : 't-dim')} style=${`background:${fill(p.mine ? 'accent' : 'dim', 0.15)}`}><${Icon} name="wifi" size=${20} /></span>
        <b class="co-pg-providername">${p.name}</b>
        <span class=${'co-pg-small t-' + p.sat_tone}>${p.sat}</span>
        <${Track} frac=${p.sat_frac} tone=${p.sat_tone} h=${5} />
        <span class="co-pg-small co-pg-muted">${p.per}</span>
        ${p.data &&
        html`<div class="co-pg-progress"><${Track} frac=${p.data.frac} tone=${p.data.tone} h=${8} /><span class="co-pg-small">${p.data.label}</span></div>
          <${Btn} label=${p.top_up.label} title=${p.top_up.tip} style="height:30px;padding:0 6px" onClick=${() => actMore('net_top_up', { gb: p.top_up.gb })} />`}
        <div title=${p.gb_tip}><${Slider} value=${p.gb} min=${n.min} max=${n.max} step=${n.step} format=${(x) => `${Math.round(x)} GB`} onChange=${(x) => setGb(k, x)} /></div>
        <b class="co-pg-providerprice">${p.price}</b>
        <span class="co-pg-small co-pg-muted">${p.from}</span>
        ${p.state && html`<span class=${'co-pg-small t-' + p.state_tone} style="font-weight:800">${p.state}</span>`}
        <div class="co-pg-line" style="margin-top:auto">
          ${p.mine
            ? html`${p.change && html`<${Btn} kind="primary" label=${p.change} style="height:32px;flex:1" onClick=${() => actMore('net_take', { provider: k, gb: p.gb })} />`}
                ${p.cancel && html`<${Btn} kind="danger" label=${p.cancel} style="height:32px;flex:1" onClick=${() => actMore('net_cancel')} />`}`
            : p.accept && html`<${Btn} kind="primary" label=${p.accept} style="height:32px;flex:1" onClick=${() => actMore('net_take', { provider: k, gb: p.gb })} />`}
        </div>
      </div>`,
    )}</div>`;
}

/** A supplier's fixed diesel price (`contracts::fuel`). */
function Fuel({ f }) {
  return html`<div class="co-pg-stack" style="gap:10px">
    <b class="co-pg-itemtitle">${f.today}</b>
    ${f.change && html`<span class=${'co-pg-small t-' + f.change.tone} style="font-weight:700">${f.change.text}</span>`}
    ${f.fixed
      ? html`<p class="co-pg-note t-ok" style="font-weight:700">${f.fixed}</p><p class="co-pg-note">${f.fixed_note}</p>`
      : html`<p class="co-pg-note">${f.offer}</p>
        <div class="co-pg-list">${f.terms.map(
          (t) => html`<div class="co-pg-line co-pg-item" style="flex-direction:row"><span class="grow co-pg-small">${t.text}</span><${Btn} label=${f.sign} style="height:30px" onClick=${() => actMore('fuel_sign', { term: t.k })} /></div>`,
        )}</div>`}
  </div>`;
}

/** The outside cleaning company (`contracts::cleaning`). */
function Cleaning({ c }) {
  return html`<div class="co-pg-stack" style="gap:10px">
    <p class="co-pg-note">${c.text}</p>
    ${c.state && html`<b class=${'co-pg-small t-' + c.tone}>${c.state}</b>`}
    <div>${c.open
      ? html`<${Btn} kind="danger" label=${c.end} style="height:32px" onClick=${() => actMore('cleaning_end')} />`
      : html`<${Btn} kind="primary" icon="check_circle" label=${c.sign} style="height:32px" onClick=${() => actMore('cleaning_sign')} />`}</div>
  </div>`;
}

// --- the tender bids ---------------------------------------------------------------------------

/** The tenders the company bid on, the latest first (`concessions::bids_list`): one picked is
 * its auction on the Tenders page. */
function Bids({ b }) {
  return html`<${Card} title=${b.title} icon="campaign">
    ${!b.rows.length
      ? html`<${Empty} icon="campaign" text=${b.empty} />`
      : html`<div class="co-pg-heldgrid">${b.rows.map(
          (r) => html`<button type="button" class="cm-tender co-pg-tender" onClick=${() => actMore('tender_pick', { id: r.id }).then(() => goTab(8))}>
            <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.caption}</b><${Icon} name="chevron_right" size=${18} class="t-dim" /></div>
            ${r.bid && html`<span class=${'co-pg-small t-' + r.bid_tone} style="font-weight:700">${r.bid}</span>`}
            <span class=${'co-pg-small t-' + r.tone}>${r.state}</span>
          </button>`,
        )}</div>`}
  </${Card}>`;
}

// --- the bus contracts -------------------------------------------------------------------------

/** The company's bus contracts (`contracts::bus_contracts`): the track record, each contract with
 * its kind, payment and days, term and extensions, its rating and points, its journeys not paid
 * and its month, its latest variations, a change to request (`variations.js`) and the way to hand
 * it back; those ended in the last twelve months below. */
function BusContracts({ b }) {
  return html`<${Card} title=${b.title} icon="directions_bus">
    <div class="co-pg-banner2" style="margin-bottom:14px"><${Icon} name="military_tech" size=${18} /><span>${b.record}</span></div>
    ${!b.rows.length && !b.ended.length
      ? html`<${Empty} icon="description" text=${b.empty} />`
      : html`<div class="co-pg-heldgrid">${b.rows.map(
          (r) => html`<div class="co-pg-item">
            <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.caption || r.line}</b><${Dot} tone=${r.rag.tone} title=${r.rag.text} /><${Badge} b=${r.rag} /></div>
            <span class="co-pg-itemsub">${r.kind}</span>
            <b class="co-pg-small">${r.pay}</b>
            <span class="co-pg-itemsub">${r.days}</span>
            <span class="co-pg-itemsub">${r.term}</span>
            <span class=${'co-pg-small t-' + r.rag.tone} style="font-weight:700"><${Icon} name="flag" size=${13} /> ${r.points}</span>
            <span class="co-pg-itemsub">${r.unpaid}</span>
            <span class="co-pg-itemsub">${r.month}</span>
            ${r.history.map((h) => html`<span class="co-pg-itemsub"><${Icon} name="edit_road" size=${13} /> ${h}</span>`)}
            <div class="co-pg-line" style="flex-wrap:wrap">
              ${r.change && html`<${Btn} icon="edit_road" label=${r.change} title=${r.change_tip} style="height:30px;padding:0 10px" onClick=${() => openVary(r.line)} />`}
              <${Btn} kind="danger" icon="restart_alt" label=${r.back} title=${r.back_tip} style="height:30px;padding:0 10px" onClick=${() => actMore('contract_hand_back', { line: r.line })} />
            </div>
          </div>`,
        )}</div>
        ${b.ended.length > 0 &&
        html`<div class="co-pg-caps" style="margin:16px 0 8px">${b.ended_title}</div>
          <div class="co-pg-list">${b.ended.map(
            (r) => html`<div class="co-pg-line co-pg-item" style="flex-direction:row"><${Plate} p=${{ number: r.number }} h=${22} /><div class="grow" style="min-width:0"><span class="co-pg-small" style="display:block">${r.text}</span><span class="co-pg-itemsub">${r.points}</span></div></div>`,
          )}</div>`}`}
  </${Card}>`;
}

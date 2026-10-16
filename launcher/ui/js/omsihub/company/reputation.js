// The Reputation page (Luc 2026-10-15; redesigned 2026-10-16 after his reference): the overall
// reputation as a gauge with its eight weeks' trend, what moves it - the positive and the
// negative factors with their points - and last week by cause, what the player can do (each a
// card with its way to the page); and the passengers' complaints with their talk - the
// passenger's words, the replies to pick and how it ended. The views come from
// `omsi-launcher-api`'s `company/reputation.rs` (`co_view` "reputation").

import { html, setUi, ui, act, useView, goTab, Icon, Btn, Meter, fill } from './kit.js';
import { Card, Glyph } from './pages-a.js';

if (!document.querySelector('link[data-co-reputation-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-reputation.css';
  sheet.dataset.coReputationCss = '1';
  document.head.append(sheet);
}

export function ReputationPage() {
  const st = ui('reputation', { tab: 'overview', id: null });
  const v = useView('reputation', st);
  if (!v?.tabs) return null;
  return html`<div class="co-pg co-pg-rep">
    <div class="co-pg-stack">
      <div class="co-pg-pagehead">
        <span class="co-pg-pageicon"><${Icon} name="star" size=${24} /></span>
        <div class="grow"><h1 class="co-pg-title">${v.title}</h1><p class="co-pg-lead">${v.lead}</p></div>
      </div>
      <div class="co-pg-underline" role="tablist">${v.tabs.map(
        (t) => html`<button type="button" role="tab" class=${t.key === v.tab ? 'on' : ''} onClick=${() => t.key !== v.tab && setUi('reputation', { tab: t.key })}>${t.label}${t.badge && html`<span class="co-pg-count">${t.badge}</span>`}</button>`,
      )}</div>
      ${v.tab === 'complaints' ? html`<div class="co-rep"><${Complaints} c=${v.body} st=${st} /></div>` : html`<${Overview} o=${v.body} />`}
    </div>
  </div>`;
}

/** The info mark beside a card's title: its words on hover. */
const Info = ({ text }) => text && html`<span class="co-pg-info" title=${text}><${Icon} name="info" size=${15} /></span>`;

function Overview({ o }) {
  const h = o.head;
  return html`<div class="co-pg-repgrid">
    <div class="co-pg-stack">
      <${Card} title=${h.title} tools=${html`<${Info} text=${h.info} />`}>
        <${Gauge} frac=${h.frac} value=${h.value} under=${h.under} />
        <div class="co-pg-saybox">
          <span class=${'co-pg-sayicon t-' + h.say_tone} style=${`background:${fill(h.say_tone, 0.16)}`}><${Glyph} name=${h.say_icon} size=${18} /></span>
          <div><b>${h.say}</b><span>${h.by}</span></div>
        </div>
        <div class="co-pg-caps" style="margin:14px 0 6px">${h.chart_title}</div>
        <${Trend} points=${h.chart} />
        <p class=${'co-pg-note t-' + h.trend_tone} style="margin-top:8px">${h.trend}</p>
        <p class="co-pg-note">${h.caps}</p>
      </${Card}>
      <${Card} title=${o.log_title} icon="history">
        <details class="co-pg-log"><summary>${o.log.length}</summary>
          ${o.log.map((r) => html`<div class="co-pg-line"><span class="co-pg-muted co-pg-small" style="width:96px;flex:none">${r.date}</span><span class="grow co-pg-cut co-pg-small" title=${r.text}>${r.text}</span><b class=${'co-pg-small t-' + r.tone}>${r.change}</b></div>`)}
        </details>
      </${Card}>
    </div>
    <div class="co-pg-stack">
      <${Card} title=${o.effects_title} tools=${html`<${Info} text=${o.effects_info} />`}>
        <div class="co-pg-factorhead t-ok"><${Icon} name="arrow_upward" size=${15} />${o.positive_title}</div>
        <div class="co-pg-factors">${o.positive.map((f) => html`<${Factor} f=${f} tone="ok" />`)}</div>
        <div class="co-pg-factorhead t-danger" style="margin-top:14px"><span class="co-pg-flip"><${Icon} name="arrow_upward" size=${15} /></span>${o.negative_title}</div>
        <div class="co-pg-factors">${o.negative.map((f) => html`<${Factor} f=${f} tone="danger" />`)}</div>
        <p class="co-pg-note" style="margin-top:12px">${o.drives}</p>
      </${Card}>
      <${Card} title=${o.week_title} icon="sort">
        ${o.week_any
          ? html`<div class="co-pg-causes">${o.week.map(
              (r) => html`<div class="co-pg-cause"><span class="co-pg-cut">${r.label}</span>
                <span class="co-pg-causebar"><i style=${`width:${Math.max(r.zero ? 0 : 3, r.frac * 100)}%;background-color:${fill(r.tone)}`}></i></span>
                <b class=${r.zero ? 'co-pg-muted' : 't-' + r.tone}>${r.value}</b></div>`,
            )}</div>`
          : html`<p class="co-pg-note">${o.week_none}</p>`}
      </${Card}>
    </div>
    <${Card} title=${o.tips_title} sub=${o.tips_sub} class="co-pg-actioncol">
      <div class="co-pg-actioncards">${o.tips.map(
        (t) => html`<div class="co-pg-actioncard">
          <span class="co-pg-actionicon"><${Icon} name=${t.icon} size=${22} /></span>
          <div class="grow">
            <div class="co-pg-line"><b class="grow">${t.title}</b><span class="co-pg-pagechip">${t.page}</span></div>
            <p>${t.text}</p>
            <button type="button" class="co-pg-gobtn" onClick=${() => (t.complaints ? setUi('reputation', { tab: 'complaints' }) : goTab(t.tab))}>${t.go}<${Icon} name="chevron_right" size=${16} /></button>
          </div>
        </div>`,
      )}</div>
    </${Card}>
  </div>`;
}

/** A factor: its icon in a round tint, its name and line, its points in a pill. */
const Factor = ({ f, tone }) => html`<div class="co-pg-factor">
  <span class=${'co-pg-factoricon t-' + tone} style=${`background:${fill(tone, 0.14)}`}><${Icon} name=${f.icon} size=${16} /></span>
  <div class="grow"><b>${f.name}</b><span>${f.text}</span></div>
  <span class=${'co-pg-pill t-' + tone} style=${`background:${fill(tone, 0.14)}`}>${f.value}</span>
</div>`;

/** The reputation as a half ring: the accent's arc on grey, the value in its middle. */
function Gauge({ frac, value, under }) {
  const r = 80;
  const len = Math.PI * r;
  const f = Math.max(0, Math.min(1, frac));
  const arc = `M ${100 - r} 100 A ${r} ${r} 0 0 1 ${100 + r} 100`;
  return html`<div class="co-pg-gauge">
    <svg viewBox="0 0 200 112" width="100%" aria-hidden="true">
      <path d=${arc} class="track" fill="none" stroke-width="16" stroke-linecap="round" />
      <path d=${arc} class="value" fill="none" stroke-width="16" stroke-linecap="round" stroke-dasharray=${`${len * f} ${len}`} />
      <text x="20" y="111" class="end" text-anchor="middle">0</text><text x="180" y="111" class="end" text-anchor="middle">100</text>
    </svg>
    <div class="co-pg-gaugevalue"><b>${value}</b><span>${under}</span></div>
  </div>`;
}

/** The weeks' reputation as a line with its dots over a 0-100 scale, the weeks under it. */
function Trend({ points }) {
  if (!points?.length) return null;
  const W = 320;
  const H = 150;
  const pad = { l: 28, r: 10, t: 10, b: 24 };
  const n = points.length;
  const x = (k) => pad.l + (n === 1 ? (W - pad.l - pad.r) / 2 : ((W - pad.l - pad.r) * k) / (n - 1));
  const y = (v) => pad.t + (H - pad.t - pad.b) * (1 - v / 100);
  const line = points.map(([, v], k) => `${x(k).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const area = `${x(0)},${y(0)} ${line} ${x(n - 1)},${y(0)}`;
  return html`<svg class="co-pg-trend" viewBox=${`0 0 ${W} ${H}`} width="100%" aria-hidden="true">
    ${[0, 25, 50, 75, 100].map((g) => html`<line x1=${pad.l} x2=${W - pad.r} y1=${y(g)} y2=${y(g)} class="grid" /><text x=${pad.l - 6} y=${y(g) + 3} class="axis" text-anchor="end">${g}</text>`)}
    ${n > 1 && html`<polygon points=${area} class="area" /><polyline points=${line} class="line" fill="none" />`}
    ${points.map(([w, v], k) => html`<circle cx=${x(k)} cy=${y(v)} r="3.5" class="dot"><title>${w}: ${v}</title></circle><text x=${x(k)} y=${H - 6} class="axis" text-anchor="middle">${w}</text>`)}
  </svg>`;
}
function Complaints({ c, st }) {
  const t = c.talk;
  return html`<div class="co-rep-split">
    <section class="co-section co-rep-col">
      <div class="co-caps">${c.title}</div>
      <p class="co-note t-dim">${c.note}</p>
      <div class="co-scroll">
        ${c.rows.length
          ? c.rows.map(
              (r) => html`<button type="button" class=${'co-rep-complaint' + (r.on ? ' on' : '') + (r.open ? ' open' : '')} onClick=${() => setUi('reputation', { id: r.id })}>
                <span class="grow"><b class="cut">${r.title}</b><span class="t-dim cut">${r.sub}</span><span class=${'t-' + r.state_tone + ' cut'}>${r.state}</span></span>
                <span class=${'co-rep-sev t-' + r.severity_tone}>${r.severity}</span>
              </button>`,
            )
          : html`<p class="co-para">${c.empty}</p>`}
      </div>
    </section>
    <section class="co-section co-rep-col co-rep-talk">
      ${t
        ? html`<div class="co-rep-who"><${Icon} name="person" size=${26} class="t-soft" /><span class="grow"><b>${t.title}</b><span class="t-dim">${t.sub}</span></span>
              <span class="co-rep-mood"><span class="co-caps">${t.mood_label}</span><${Meter} frac=${t.mood} tone=${t.mood >= 0.6 ? 'ok' : t.mood >= 0.3 ? 'warn' : 'danger'} /></span></div>
            <div class="co-scroll co-rep-lines">${t.talk.map((l) => html`<div class=${'co-rep-say' + (l.mine ? ' mine' : '')}>${l.text}</div>`)}</div>
            ${t.open
              ? html`<div class="co-rep-choices">${t.choices.map((ch) => html`<${Btn} kind=${ch.danger ? 'ghost' : ''} label=${ch.label} onClick=${() => act('complaint_reply', { id: t.id, choice: ch.key })} />`)}</div>`
              : html`<div class=${'co-rep-end t-' + t.state_tone}>${t.state}</div>`}
            ${t.fix && html`<button type="button" class="co-rep-tip" onClick=${() => goTab(t.fix.tab)}><${Icon} name="build" size=${18} class="t-accent2" /><span class="grow">${t.fix.text}</span><span class="t-dim">${t.fix.go}</span></button>`}`
        : html`<p class="co-para t-dim">${c.empty}</p>`}
    </section>
  </div>`;
}

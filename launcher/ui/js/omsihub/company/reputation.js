// The Reputation page (Luc 2026-10-15): the reputation with its trend and caps, what moved it
// lately (by cause and day by day), what moves it and by how much, what the player can do (each
// a way to its page), and the passengers' complaints with their talk - the passenger's words,
// the replies to pick and how it ended. The views come from `omsi-launcher-api`'s
// `company/reputation.rs` (`co_view` "reputation").

import { html, setUi, ui, act, useView, goTab, Icon, Btn, Seg, Meter } from './kit.js';

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
  const k = Math.max(0, v.tabs.findIndex((t) => t.key === v.tab));
  const options = v.tabs.map((t, i) => [i, t.badge ? `${t.label} (${t.badge})` : t.label]);
  return html`<div class="co-rep">
    <div class="co-rep-tabs"><${Seg} value=${k} options=${options} onChange=${(i) => setUi('reputation', { tab: v.tabs[i].key })} /></div>
    ${v.tab === 'complaints' ? html`<${Complaints} c=${v.body} st=${st} />` : html`<${Overview} o=${v.body} />`}
  </div>`;
}

function Overview({ o }) {
  const h = o.head;
  return html`<div class="co-rep-grid">
    <section class="co-section co-rep-col">
      <div class="co-rep-head">
        <${Icon} name="star" size=${36} class="t-warn" />
        <div><b class="co-rep-big">${h.value}</b> <span class="t-dim">${h.under}</span><div class=${'t-' + h.trend_tone}>${h.trend}</div></div>
      </div>
      <p class="co-note t-dim">${h.caps}</p>
      <div class="co-caps">${o.week_title}</div>
      ${o.week.length ? o.week.map((r) => html`<${Row} r=${r} />`) : html`<p class="co-note t-dim">${o.week_none}</p>`}
      <div class="co-caps" style="margin-top:12px">${o.log_title}</div>
      <div class="co-scroll co-rep-log">${o.log.map((r) => html`<${Row} r=${r} date />`)}</div>
    </section>
    <section class="co-section co-rep-col">
      <div class="co-caps">${o.effects_title}</div>
      <div class="co-scroll">
        ${o.effects.map((e) => html`<div class="co-rep-effect"><${Icon} name=${e.icon} size=${18} class="t-soft" /><span class="grow">${e.text}</span><b>${e.size}</b></div>`)}
        <p class="co-note t-dim">${o.drives}</p>
      </div>
    </section>
    <section class="co-section co-rep-col">
      <div class="co-caps">${o.tips_title}</div>
      <div class="co-scroll">
        ${o.tips.map(
          (t) => html`<button type="button" class="co-rep-tip" onClick=${() => (t.complaints ? setUi('reputation', { tab: 'complaints' }) : goTab(t.tab))}>
            <${Icon} name=${t.icon} size=${20} class="t-accent2" /><span class="grow">${t.text}</span><span class="t-dim">${t.go}</span><${Icon} name="chevron_right" size=${16} class="t-dim" />
          </button>`,
        )}
      </div>
    </section>
  </div>`;
}

const Row = ({ r, date }) => html`<div class="co-rep-row">${date && html`<span class="t-dim when">${r.date}</span>`}<span class="grow cut" title=${r.text}>${r.text}</span><b class=${'t-' + r.tone}>${r.change}</b></div>`;

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

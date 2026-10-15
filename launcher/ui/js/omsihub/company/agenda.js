// The company's Agenda (Beta 57, Luc: "voeg een agenda toe met alle gebeurtenissen die er gaan
// komen"): everything coming - holidays, breakdowns, buses back from the workshop, deliveries,
// tenders, deadlines - as a list grouped by day or as a month's calendar with small marks per
// kind (a day picked shows its entries), filtered by kind and by what needs the player. Each
// entry goes to the page that handles it (`follow`). The views come from `omsi-launcher-api`'s
// `company/agenda.rs` (`co_view` "agenda"), translated; the start screen's "Coming up" tile and
// Casual's list draw `ComingRows` of them.

import { html, setCo, setUi, ui, useView, goTab, Icon, Seg, Switch, IconBtn, Btn } from './kit.js';
import { openRosterFlow } from '../rosterflow/index.js';
import { rfCmd } from '../rosterflow/state.js';

// (this page's own look, loaded with it)
if (!document.querySelector('link[data-co-agenda-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-agenda.css';
  sheet.dataset.coAgendaCss = '1';
  document.head.append(sheet);
}

/** The planning on a day: RosterFlow's day with its add-on on, else the Planning page. */
async function openDay(date) {
  try {
    const on = await rfCmd('rf_on');
    if (on?.on) {
      const k = (on.days || []).findIndex((d) => d.date === date);
      if (await openRosterFlow({ day: Math.max(0, k) })) return;
    }
  } catch {
    /* (the Planning page instead) */
  }
  goTab(5);
}

/** Go where an entry is handled (`agenda::go_json`): what the page shows there first, then a
 * dialog over the page, RosterFlow's day, or the page. */
export function follow(go) {
  if (!go) return;
  for (const [page, patch] of Object.entries(go.ui || {})) setUi(page, patch);
  if (go.dialog) return setCo({ dialog: go.dialog });
  if (go.rf) return openDay(go.rf);
  if (go.tab != null) goTab(go.tab);
}

/** An entry as a row: its icon in its tone, its time, its words and its second line, a dot when
 * it needs the player, and the way to its page. */
function Row({ it, compact }) {
  return html`<button type="button" class=${'co-agenda-row' + (it.needs ? ' needs' : '') + (it.go ? '' : ' still') + (compact ? ' compact' : '')} title=${it.note ? `${it.text}\n${it.note}` : it.text} onClick=${() => follow(it.go)}>
    <span class=${'ic t-' + it.tone}><${Icon} name=${it.icon} size=${compact ? 16 : 18} /></span>
    <span class="time">${it.time || ''}</span>
    <span class="words"><span class="cut">${it.text}</span>${it.note && html`<span class="note cut">${it.note}</span>`}</span>
    ${it.needs && html`<i class="dot"></i>`}
    ${it.go && html`<${Icon} name="chevron_right" size=${16} class="t-dim go" />`}
  </button>`;
}

/** The next few entries (the start's tile, Casual's home): when, what, its tone. */
export function ComingRows({ list, n, none, casual }) {
  if (!list?.length) return html`<div class="co-note t-dim" style="line-height:22px">${none}</div>`;
  return html`${list.slice(0, n).map((it) =>
    casual
      ? html`<button type="button" class=${'co-agenda-coming casual' + (it.go ? '' : ' still')} onClick=${() => follow(it.go)} title=${it.note || it.text}>
          <span class=${'ic t-' + it.tone}><${Icon} name=${it.icon} size=${17} /></span><b class="when">${it.when}</b><span class="cut grow">${it.text}</span>
          ${it.go && html`<${Icon} name="chevron_right" size=${16} class="t-dim" />`}</button>`
      : html`<div class="co-agenda-coming"><span class=${'ic t-' + it.tone}><${Icon} name=${it.icon} size=${14} /></span><span class="cut grow">${it.text}</span><span class="when">${it.when}</span></div>`,
  )}`;
}

export function AgendaPage() {
  const st = ui('agenda', { mode: 'list', groups: null, mine: false, month: null, day: null });
  const args = { mode: st.mode, mine: st.mine };
  if (st.groups) args.groups = st.groups;
  if (st.mode === 'month') Object.assign(args, { month: st.month, day: st.day });
  const v = useView('agenda', args);
  if (!v?.groups) return html`<div style="height:100%"></div>`;
  const toggle = (key) => {
    const on = v.groups.filter((g) => g.on).map((g) => g.key);
    const next = on.includes(key) ? on.filter((k) => k !== key) : [...on, key];
    setUi('agenda', { groups: next.length === v.groups.length ? null : next });
  };
  return html`<div class="co-agenda">
    <div class="head">
      <div class="grow" style="min-width:0"><b class="cut">${v.title}</b><span class="cut">${v.sub}</span></div>
      <${Seg} value=${v.mode} options=${v.modes.map((m, k) => [k, m])} onChange=${(k) => setUi('agenda', { mode: k === 1 ? 'month' : 'list' })} style="width:240px;height:38px" />
    </div>
    <div class="filters">
      <div class="co-chips">
        ${v.groups.map((g) => html`<button type="button" class=${g.on ? 'on' : ''} onClick=${() => toggle(g.key)}><${Icon} name=${g.icon} size=${16} />${g.label}<span class="n">${g.n}</span></button>`)}
      </div>
      <span class="grow"></span>
      <span class=${'mine' + (v.mine.n > 0 ? ' t-warn' : ' t-dim')}>${v.mine.text}</span>
      <span style="width:220px"><${Switch} checked=${v.mine.on} label=${v.mine.label} title=${v.mine.tip} onChange=${(on) => setUi('agenda', { mine: on })} /></span>
    </div>
    ${v.list && html`<${List} l=${v.list} />`}
    ${v.month && html`<${Month} m=${v.month} groups=${v.groups} />`}
  </div>`;
}

/** The list: the days with entries, each under its heading ("Today", "Tomorrow", "Mon 26 Oct"). */
function List({ l }) {
  return html`<div class="co-scroll co-agenda-list">
    <div class="range co-note t-dim">${l.range}  ·  ${l.count}</div>
    ${l.days.length
      ? l.days.map((d) => html`<section class=${'day' + (d.today ? ' today' : '') + (d.past ? ' past' : '')}>
          <div class="dayhead"><b>${d.label}</b><span>${d.label !== d.sub ? d.sub : ''}</span>
            ${d.ops && html`<button type="button" class="co-btn small ghost t-warn" style="margin-left:auto" onClick=${() => goTab(d.ops.tab)}>${d.ops.label}</button>`}</div>
          ${d.items.map((it) => html`<${Row} it=${it} />`)}
        </section>`)
      : html`<div class="co-empty" style="padding:24px 4px">${l.empty}</div>`}
  </div>`;
}

/** The month: its weeks, each day with a mark per kind (how many, in the worst's tone); the day
 * picked beside it with its entries. */
function Month({ m, groups }) {
  const pick = (date) => setUi('agenda', { day: date });
  const to = (key) => setUi('agenda', { month: key, day: null });
  return html`<div class="co-agenda-month">
    <div class="cal">
      <div class="nav">
        <${IconBtn} icon="chevron_left" title=${m.prev_label} onClick=${() => to(m.prev)} />
        <b class="cut">${m.label}</b>
        <${IconBtn} icon="chevron_right" title=${m.next_label} onClick=${() => to(m.next)} />
        <span class="grow"></span>
        ${!m.this && html`<${Btn} kind="ghost" icon="today" label=${m.this_label} small onClick=${() => to(m.this_key)} />`}
      </div>
      <div class="grid">
        ${m.weekdays.map((w) => html`<span class="wd">${w}</span>`)}
        ${m.weeks.flat().map((d) => html`<button type="button" class=${['cell', d.in ? '' : 'out', d.today ? 'today' : '', d.past ? 'past' : '', d.sel ? 'sel' : ''].filter(Boolean).join(' ')} onClick=${() => pick(d.date)}>
          <span class="n">${d.n}${d.needs && html`<i class="dot"></i>`}</span>
          <span class="marks">${d.marks.map((k) => html`<i class=${'mk ' + k.group + ' t-' + k.tone} title=${k.n}>${k.n > 1 ? k.n : ''}</i>`)}</span>
        </button>`)}
      </div>
      <div class="legend">${groups.map((g) => html`<span class=${g.on ? '' : 'off'}><i class=${'mk ' + g.key}></i>${g.label}</span>`)}</div>
    </div>
    <div class="dayside">
      <div class="co-caps" style="margin-bottom:10px">${m.day.label}</div>
      <div class="co-scroll">
        ${m.day.items.length ? m.day.items.map((it) => html`<${Row} it=${it} compact />`) : html`<div class="co-empty" style="padding:8px 2px">${m.day.empty}</div>`}
      </div>
    </div>
  </div>`;
}

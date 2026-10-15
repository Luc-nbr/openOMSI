// The company's inbox, as our game-drawn launcher's (`company/inbox.rs`): the messages by kind
// as cards (the unread with the accent's mark), the trophies' bar under them, and the message
// chosen on the right - what an event does and on which days, a goal's progress and reward, a
// holiday request with its Approve and Reject buttons (Beta 57), and the way to the page about it.

import { html, setUi, ui, act, useView, goTab, Icon, Btn, Chips, IconBtn, fill } from './kit.js';
import { HolidayRequest } from './people.js';

export function InboxPage() {
  const st = ui('inbox', { filter: 'all', open: null });
  const v = useView('inbox', st);
  if (!v?.filters) return null;
  const k = Math.max(0, v.filters.findIndex(([f]) => f === v.filter));
  const open = (m) => {
    setUi('inbox', { open: m.id });
    if (!m.read) act('inbox_read', { id: m.id });
  };
  const del = (id) => {
    if (st.open === id) setUi('inbox', { open: null });
    act('inbox_delete', { id });
  };
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="display:flex;gap:14px;align-items:flex-start;flex:none">
      <div class="grow"><${Chips} value=${k} options=${v.filters.map(([, l], i) => [i, l])} onChange=${(i) => setUi('inbox', { filter: v.filters[i][0] })} /></div>
      <${Btn} icon="check_circle" label=${v.mark_all} style="height:34px" onClick=${() => act('inbox_mark_all', { filter: v.filter })} />
      <${Btn} kind="ghost" icon="delete" label=${v.delete_read} title=${v.delete_read_tip} style="height:34px" onClick=${() => act('inbox_delete_read', { filter: v.filter })} />
    </div>
    <div class="co-inbox">
      <div class="left">
        <div class="co-scroll list">
          ${v.list.length
            ? v.list.map((m) => html`<${Card} m=${m} on=${st.open === m.id} onOpen=${() => open(m)} onDelete=${() => del(m.id)} />`)
            : html`<div class="co-emptymid"><${Icon} name="check_circle" size=${40} class="t-faint" /><span>${v.empty}</span></div>`}
        </div>
        <div class="strip"><div class="co-caps">${v.trophies_title}</div><${Trophies} t=${v.trophies} /></div>
      </div>
      <${Details} v=${v} m=${v.open} />
    </div>
  </div>`;
}

/** A wide bar with its words on it (`inbox::wide_bar`). */
export function WideBar({ b, h = 24 }) {
  const f = Math.max(0, Math.min(1, b.frac || 0));
  return html`<div class="co-widebar" style=${`height:${h}px`}>${f > 0 && html`<i style=${`width:max(12px, ${f * 100}%);background:${fill(b.tone, 0.7)}`}></i>`}<span>${b.label}</span></div>`;
}

/** The trophies the goals earned against the next milestone (`contracts::trophy_bar`). */
export function Trophies({ t }) {
  return html`<div class="co-trophies" title=${t.tip}><${Icon} name="emoji_events" size=${24} class="t-warn" /><${WideBar} b=${{ frac: t.frac, tone: 'ok', label: t.label }} h=${26} /><${Icon} name="military_tech" size=${26} class="t-soft" /></div>`;
}

/** A goal's reward: its trophies and its money (`contracts::reward`). */
export const Reward = ({ g }) => html`<span class="co-reward"><${Icon} name="emoji_events" size=${18} class="t-warn" /><b>${g.trophies}</b><${Icon} name="payments" size=${18} class="t-ok" /><b>${g.reward}</b></span>`;

/** A message's card (`inbox::card`). */
function Card({ m, on, onOpen, onDelete }) {
  return html`<div class=${'co-msg' + (on ? ' on' : '') + (m.read ? '' : ' unread')} onClick=${onOpen}>
    <span class="icon" style=${`background:${fill(m.tone, 0.16)}`}><${Icon} name=${m.icon} size=${22} class=${'t-' + m.tone} /></span>
    <div class="body">
      <div class="top"><b class="cut">${m.title}</b><span class="date">${m.date}</span>
        <span onClick=${(e) => (e.stopPropagation(), onDelete())}><${IconBtn} icon="delete" size=${13} style="width:24px;height:24px" /></span></div>
      <p>${m.text}</p>
      ${m.goal && html`<div style="margin-top:6px"><${Reward} g=${m.goal} /></div>`}
      ${m.bar && html`<div style="margin-top:10px"><${WideBar} b=${m.bar} /></div>`}
      ${m.leave?.open && html`<${HolidayRequest} r=${m.leave} compact />`}
    </div>
  </div>`;
}

/** The message chosen (`inbox::details`). */
function Details({ v, m }) {
  return html`<section class="co-section co-msgdetail">
    <div class="co-caps" style="margin-bottom:14px">${v.message_title}</div>
    ${!m
      ? html`<div class="co-emptymid"><${Icon} name="receipt_long" size=${36} class="t-faint" /><span class="t-dim">${v.pick}</span></div>`
      : html`<div class="co-scroll" style="flex:1;min-height:0">
          <div style="display:flex;justify-content:space-between;align-items:center"><span class=${'co-tag t-' + m.tone} style=${`background:${fill(m.tone, 0.16)}`}>${m.kind}</span><span class="co-note t-dim" style="font-weight:500">${m.date}</span></div>
          <h3>${m.title}</h3>
          <p class="co-para" style="margin-bottom:16px">${m.text}</p>
          ${m.event && html`<div class="co-caps" style="margin-bottom:8px">${m.event.title}</div>
            ${m.event.effects.map(([t, tone]) => html`<div class="co-effect"><i class=${'dot-' + tone}></i><span>${t}</span></div>`)}
            <b class="t-accent2" style="display:block;font-size:var(--co-rows);line-height:30px">${m.event.status}</b>
            <${Days} days=${m.event.days} today=${v.today} />`}
          ${!m.event && m.days && html`<${Days} days=${m.days} today=${v.today} />`}
          ${m.goal && html`${m.goal.bar && html`<div style="margin-bottom:10px"><${WideBar} b=${m.goal.bar} h=${28} /></div>`}<${Reward} g=${m.goal} />
            <p class="co-note t-dim" style="margin:8px 0">${m.goal.rule}</p><b class="t-soft" style="font-size:var(--co-rows)">${m.goal.state}</b>`}
          ${!m.goal && m.bar && html`<${WideBar} b=${m.bar} h=${28} />`}
          ${m.leave && html`<${HolidayRequest} r=${m.leave} />`}
        </div>
        <div style="display:flex;gap:10px;flex:none;margin-top:10px">
          ${m.go && html`<${Btn} kind="primary" icon=${m.go.icon} label=${m.go.label} style="height:36px" onClick=${() => (m.go.ui && setUi(m.go.ui.page, m.go.ui.patch), goTab(m.go.tab))} />`}
          ${m.read && html`<${Btn} label=${v.unread_label} style="height:36px" onClick=${() => act('inbox_unread', { id: m.id })} />`}
          <span class="grow"></span>
          <${Btn} kind="danger" icon="delete" label=${v.delete} style="height:36px" onClick=${() => (setUi('inbox', { open: null }), act('inbox_delete', { id: m.id }))} />
        </div>`}
  </section>`;
}

/** The days of an event, today in the accent (`inbox::day_strip`). */
function Days({ days, today }) {
  if (!days?.length) return null;
  return html`<div class="co-daystrip">${days.map((d) => html`<span class=${d.state} title=${d.state === 'today' ? today : undefined}><b>${d.wd}</b>${d.day}</span>`)}</div>`;
}

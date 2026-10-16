// "Play together" in the bus company (Beta 58, built out in Beta 61; the Rust side is
// `omsihub/together`): the bar's button that opens the company's Multiplayer page
// (`multiplayer.js`), and a friend's side - the code field of "Join a company", then the boss's
// company in a menu of its own: his role and what it lets him do, his duties with Drive, the open
// duties to ask for, his figures for the company, the players and the news; as his role allows,
// the duty requests to approve, the duties to give, the players to manage, and the company's own
// pages (`openCompanyPages`: the company's screen with its commands relayed to the boss's
// launcher). The screen follows his role as the boss changes it. Every text comes from Rust,
// translated.

import { S, set, toast } from '../../store.js';
import { duty } from '../../data.js';
import { html, useEffect, useState, tr, setCo, cmd, freshCo, goTab, Icon, Mark, Btn } from './kit.js';
import { togetherOn, blockedLabel } from './gate.js';
import { usePoll, run, Chip, Face, RoleBadge, Stats, RoleEditor, MultiplayerDialog } from './multiplayer.js';
import { SessionEnded } from './session.js';

/** The Multiplayer page's tab (`company::TOGETHER_TAB`). */
export const TOGETHER_TAB = 18;

/** The newest notices as toasts (those after the last one seen; none the first time). */
let seenNotice = null;
function toastNew(notices, last) {
  if (seenNotice == null || last < seenNotice) {
    seenNotice = last;
    return;
  }
  const fresh = (notices || []).filter((n) => n.n > seenNotice).slice(0, 3);
  seenNotice = last;
  fresh.reverse().forEach((n) => toast(n.text, 'ok'));
}

// --- the boss's side ---------------------------------------------------------------------------

/** The company bar's button: the session's players online (the duties asked for in orange); a
 * click opens the Multiplayer page. */
export function TogetherButton() {
  // (switched off: shown, disabled - `gate.js`)
  if (!togetherOn()) return html`<span class="tg-btn" title=${blockedLabel()}><${Btn} class="sq" icon="wifi_tethering" disabled=${true} title=${blockedLabel()} /></span><span class="gap8"></span>`;
  return html`<${LiveTogetherButton} />`;
}

function LiveTogetherButton() {
  const [b] = usePoll('tg_host_badge', 3000);
  const n = b?.on ? b.asked || b.online : 0;
  const open = () => (S.co.shell?.pages?.includes(TOGETHER_TAB) ? goTab(TOGETHER_TAB) : setCo({ dialog: { kind: 'together' } }));
  return html`<span class="tg-btn"><${Btn} class="sq" kind=${b?.on ? 'primary' : ''} icon="wifi_tethering" title=${b?.tip || tr('Multiplayer')} onClick=${open} />
    ${n > 0 && html`<span class=${'tg-badge' + (b.asked ? ' warn' : '')}>${n}</span>`}</span><span class="gap8"></span>`;
}

/** The Multiplayer page in a dialog (`dialogs.js`'s "together", RosterFlow's button). */
export function TogetherDialog() {
  return html`<${MultiplayerDialog} />`;
}

// --- a friend's side -----------------------------------------------------------------------------

/** Open the boss's company's own pages, as far as the role allows them (their commands relayed:
 * `kit.js`'s `cmd`). */
export function openCompanyPages() {
  set({ co: freshCo({ remote: true, chooser: null }), page: 'company' });
}

/** Back from the company's pages to the friend's menu. */
export function backToMenu() {
  set({ co: freshCo({ chooser: 'join' }), page: 'company' });
}

/** Drive a duty: the player's game options, then the boss's launcher's start (`tg_drive`; one the
 * AI drives since he left it, from its next trip: `tg_take_back`). */
async function driveDuty(d, name = 'tg_drive') {
  let base;
  try {
    base = await duty();
  } catch (e) {
    toast(e.message, 'error');
    return;
  }
  await run(name, { duty: d.duty, base });
}

/** A duty of the boss's day: what it is, who has it, what can be done with it. */
function DutyRow({ d, again }) {
  const act = async (a) => {
    if (a.act === 'drive') await driveDuty(d);
    else if (a.act === 'take_back') await driveDuty(d, 'tg_take_back');
    else await run(a.act === 'take' ? 'tg_take' : 'tg_release', { duty: d.duty });
    again();
  };
  return html`<div class=${'tg-duty ' + d.key} title=${d.title}>
    <${Chip} number=${d.number} colour=${d.colour} />
    <div class="what"><b>${d.times}</b><span class="cut">${d.route}</span><span class="dim cut">${d.facts}</span></div>
    <span class=${'state cut t-' + d.tone}>${d.state}</span>
    <span class="acts">${d.acts.map((a) => html`<${Btn} kind=${a.kind} icon=${a.icon} label=${a.label} onClick=${() => act(a)} />`)}</span>
  </div>`;
}

/** The duty that comes next, large: its line, times and route, and Drive. */
function NextUp({ d, w, again }) {
  if (!d) return html`<div class="tg-next empty"><${Icon} name="schedule" size=${28} /><span>${w.nothing_next}</span></div>`;
  const act = async (a) => {
    if (a.act === 'drive') await driveDuty(d);
    else if (a.act === 'take_back') await driveDuty(d, 'tg_take_back');
    else await run('tg_release', { duty: d.duty });
    again();
  };
  return html`<div class=${'tg-next ' + d.key}>
    <div class="line"><${Chip} number=${d.number} colour=${d.colour} big=${true} /><div class="when"><b>${d.times}</b><span class="dim">${d.date}</span></div></div>
    <div class="route"><b class="cut">${d.route}</b><span class="dim cut">${d.title}  ·  ${d.facts}</span><span class=${'cut t-' + d.tone}>${d.state}</span></div>
    <div class="acts">${d.acts.map((a) => html`<${Btn} kind=${a.kind} big=${a.act === 'drive'} icon=${a.icon} label=${a.label} onClick=${() => act(a)} />`)}</div>
  </div>`;
}

/** "Join a company": the code field and the player's history. */
function JoinCode({ v, ask }) {
  const w = v.words || {};
  const [code, setCode] = useState('');
  const [check, setCheck] = useState(null);
  const back = () => setCo({ chooser: 'mode' });
  const typed = (t) => {
    setCode(t);
    cmd('tg_check', { code: t })
      .then(setCheck)
      .catch(() => {});
  };
  const join = async () => {
    const r = await run('tg_join', { code });
    if (r && !r.error) ask();
  };
  return html`<div class="co-choose tg-join">
    <div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${back} /></div>
    <div class="co-scroll" style="flex:1;margin-top:12px;display:flex;flex-direction:column;align-items:center">
      <span style="flex:0.3 1 0"></span>
      <div class="tg-joincard">
        <span class="tg-hero-icon big"><${Icon} name="wifi_tethering" size=${34} /></span>
        <div class="head"><h1>${w.title}</h1><p>${w.text}</p></div>
        <div class="tg-field"><input class="co-field" value=${code} placeholder=${w.field} onInput=${(e) => typed(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && check?.ok && join()} />
          <${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!check?.ok} onClick=${join} /></div>
        ${check?.error && html`<p class="t-warn tg-why">${check.error}</p>`}
        <ul class="tg-need">${(w.need || []).map((t) => html`<li>${t}</li>`)}</ul>
      </div>
      ${v.history?.length > 0 && html`<div class="tg-history"><div class="co-caps">${w.history_caps}</div>${v.history.map((h) => html`<div class="tg-result"><b class="cut">${h.title}</b><span class="dim cut">${h.text}</span></div>`)}</div>`}
      <span style="flex:0.7 1 0"></span>
    </div>
  </div>`;
}

/** The player's role and what it lets him do. */
function MyRole({ v }) {
  const w = v.words;
  return html`<section class="co-section tg-myrole">
    <div class="co-caps">${w.role_caps}</div>
    <div class="tg-rolehead"><span class="tg-hero-icon"><${Icon} name=${v.me.role.icon} size=${24} /></span><div><b>${v.me.role.label}</b><span class="dim">${v.me.role.about}</span></div></div>
    <div class="co-caps">${w.can_caps}</div>
    <div class="tg-perms static">${v.me.perms.map((p) => html`<span class=${'tg-perm' + (p.on ? ' on' : ' off')} title=${p.label}><${Icon} name=${p.on ? 'check' : 'close'} size=${14} /><span class="cut">${p.label}</span></span>`)}</div>
  </section>`;
}

/** "My work": what comes next, the player's duties, his figures for the company, his role. */
function MyWork({ v, ask }) {
  const w = v.words;
  return html`<div class="tg-cols">
    <div class="co-scroll tg-main">
      <section class="co-section"><div class="co-caps">${w.next_caps}</div><${NextUp} d=${v.next} w=${w} again=${ask} /></section>
      <section class="co-section"><div class="co-caps">${w.mine_caps}</div>${v.mine.length ? v.mine.map((d) => html`<${DutyRow} d=${d} again=${ask} />`) : html`<p class="dim">${w.no_mine}</p>`}</section>
      ${v.results.length > 0 && html`<section class="co-section"><div class="co-caps">${w.results_caps}</div>
        ${v.results.map((r) => html`<div class="tg-result"><b class="cut">${r.title}</b><span class="dim">${r.text}</span></div>`)}</section>`}
    </div>
    <div class="co-scroll tg-side">
      <section class="co-section"><div class="co-caps">${w.stats_caps}</div><${Stats} st=${v.me.stats} w=${w} /></section>
      <${MyRole} v=${v} />
      ${v.history.length > 0 && html`<section class="co-section"><div class="co-caps">${w.history_caps}</div>
        ${v.history.slice(0, 8).map((h) => html`<div class="tg-result"><b class="cut">${h.title}</b><span class="dim">${h.text}</span></div>`)}</section>`}
    </div>
  </div>`;
}

/** Give a duty of the day to a player (a role that may), or take one off. */
function GiveRow({ d, v, again }) {
  const w = v.words;
  const players = v.members.filter((m) => !m.banned);
  const [who, setWho] = useState(0);
  const p = players[Math.min(who, players.length - 1)];
  const go = async (name, args) => {
    await run(name, args);
    again();
  };
  return html`<div class="tg-duty give">
    <${Chip} number=${d.number} colour=${d.colour} />
    <div class="what"><b>${d.times}</b><span class="cut">${d.route}</span><span class="dim cut">${d.state}</span></div>
    ${players.length > 0 &&
    html`<span class="co-select"><select class="co-field" value=${String(who)} onChange=${(e) => setWho(Number(e.currentTarget.value))}>
      ${players.map((m, k) => html`<option value=${String(k)}>${m.name}</option>`)}
    </select><${Icon} name="expand_more" size=${20} /></span>`}
    <span class="acts">
      ${p && html`<${Btn} kind="primary" icon="sync_alt" label=${w.give} onClick=${() => go('tg_assign', { duty: d.duty, guest: p.no })} />`}
      ${d.key === 'player' && html`<${Btn} label=${w.off} onClick=${() => go('tg_unassign', { duty: d.duty })} />`}
    </span>
  </div>`;
}

/** "Duties": the open duties to ask for, the requests to approve, the duties to give, the rest. */
function Duties({ v, ask }) {
  const w = v.words;
  const go = async (name, args) => {
    await run(name, args);
    ask();
  };
  return html`<div class="tg-cols">
    <div class="co-scroll tg-main">
      <section class="co-section"><div class="co-caps">${w.open_caps}</div><p class="note">${v.take_note}</p>
        ${v.open.length ? v.open.map((d) => html`<${DutyRow} d=${d} again=${ask} />`) : html`<p class="dim">${v.reading ? '…' : w.no_open}</p>`}</section>
      ${v.can.assign && v.giveable.length > 0 && html`<section class="co-section"><div class="co-caps">${w.give_caps}</div>${v.giveable.map((d) => html`<${GiveRow} d=${d} v=${v} again=${ask} />`)}</section>`}
      ${v.rest.length > 0 && html`<section class="co-section"><div class="co-caps">${w.rest_caps}</div>${v.rest.map((d) => html`<${DutyRow} d=${d} again=${ask} />`)}</section>`}
    </div>
    <div class="co-scroll tg-side">
      ${v.can.approve && html`<section class="co-section"><div class="co-caps">${w.asks_caps}</div>
        ${v.asks.length
          ? v.asks.map((a) => html`<div class="tg-ask"><${Face} name=${a.name} size=${28} /><div class="what"><b class="cut">${a.name}</b><span class="cut">${a.what}</span><span class="dim">${a.times}</span></div>
              <span class="acts"><${Btn} small=${true} kind="primary" label=${w.yes} onClick=${() => go('tg_answer', { duty: a.duty, yes: true })} /><${Btn} small=${true} label=${w.no} onClick=${() => go('tg_answer', { duty: a.duty, yes: false })} /></span></div>`)
          : html`<p class="dim">${w.no_asks}</p>`}</section>`}
      <section class="co-section"><div class="co-caps">${w.mine_caps}</div>${v.mine.length ? v.mine.map((d) => html`<${DutyRow} d=${d} again=${ask} />`) : html`<p class="dim">${w.no_mine}</p>`}</section>
    </div>
  </div>`;
}

/** A player the friend manages (his role allows it): role, permissions, sending away. */
function Member({ m, v, again }) {
  const w = v.words;
  const [open, setOpen] = useState(false);
  const go = async (name, args) => {
    await run(name, args);
    again();
  };
  return html`<div class=${'tg-player' + (m.online ? ' online' : '') + (m.banned ? ' banned' : '')}>
    <div class="top"><${Face} name=${m.name} online=${m.online} size=${36} /><div class="who"><b class="cut">${m.name}</b><span class="dim cut">${m.online ? w.online : w.offline}</span></div>
      <${RoleBadge} role=${m.role} /><button type="button" class="tg-more" onClick=${() => setOpen(!open)}><${Icon} name=${open ? 'expand_less' : 'tune'} size=${18} /></button></div>
    ${open &&
    html`<${RoleEditor} p=${{ ...m, custom: false }} roles=${v.roles} w=${w} onRole=${(key) => go('tg_role', { guest: m.no, role: key })} onPerms=${(keys) => go('tg_role', { guest: m.no, role: m.role.key, perms: keys })} />
      <div class="foot"><${Btn} small=${true} icon="logout" label=${w.kick} onClick=${() => go('tg_kick', { guest: m.no, ban: false })} /><${Btn} small=${true} kind="danger" icon="close" label=${w.ban} onClick=${() => go('tg_kick', { guest: m.no, ban: true })} /></div>`}
    <${Stats} st=${m.stats} w=${w} />
  </div>`;
}

/** "Team": the players with their roles, and managing them (a role that may). */
function Team({ v, ask }) {
  const w = v.words;
  return html`<div class="tg-cols">
    <div class="co-scroll tg-main">
      <section class="co-section"><div class="co-caps">${w.players_caps}</div>
        <div class="tg-team">${v.players.map((p) => html`<div class=${'tg-mate' + (p.me ? ' me' : '')}><${Face} name=${p.name} initial=${p.initial} online=${p.online} boss=${p.boss} size=${38} />
          <div class="who"><b class="cut">${p.name}</b><span class=${'cut ' + (p.driving ? 't-ok' : 'dim')}>${p.text}</span></div><${RoleBadge} role=${p.role} /></div>`)}</div></section>
      ${v.can.players && v.members.length > 0 && html`<section class="co-section"><div class="co-caps">${w.members_caps}</div>
        <div class="tg-players">${v.members.map((m) => html`<${Member} key=${m.no} m=${m} v=${v} again=${ask} />`)}</div></section>`}
    </div>
  </div>`;
}

/** "News": the company's news and a line to everyone. */
function News({ v, ask }) {
  const w = v.words;
  const [say, setSay] = useState('');
  const send = () => {
    if (!say.trim()) return;
    run('tg_say', { text: say }).then(ask);
    setSay('');
  };
  return html`<div class="tg-cols one">
    <div class="co-scroll tg-main">
      <section class="co-section"><div class="co-caps">${w.notices_caps}</div>
        <div class="tg-say"><input class="co-field" value=${say} placeholder=${w.say} onInput=${(e) => setSay(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && send()} /><${Btn} icon="chat" label=${w.send} onClick=${send} /></div>
        <div class="tg-notices">${v.notices.map((n) => html`<div><span class="dim">${n.at}</span>${n.text}</div>`)}</div></section>
    </div>
  </div>`;
}

/** The friend's menu after joining: the company's head, then his tabs. */
function Joined({ v, ask }) {
  const w = v.words || {};
  const [tab, setTab] = useState('work');
  const [role, setRole] = useState(null);
  useEffect(() => {
    toastNew(v.notices, v.last_notice || 0);
  }, [v.last_notice]);
  // (the boss changed his role: said, and his screen follows)
  useEffect(() => {
    const k = v.me?.role_key;
    if (role && k && role !== k) toast(w.role_changed.replace('%{role}', v.me.role.label), 'ok');
    if (k) setRole(k);
  }, [v.me?.role_key]);
  const back = () => setCo({ chooser: 'mode' });
  const leave = async () => {
    await run('tg_leave');
    seenNotice = null;
    ask();
  };
  const tabs = [
    ['work', w.tab_work, 'directions_bus', v.mine.filter((d) => d.key === 'mine' || d.key === 'late').length],
    ['duties', w.tab_duties, 'route', v.can.approve ? v.asks.length : v.open.length],
    ['team', w.tab_team, 'group', v.online],
    ['news', w.tab_news, 'forum', 0],
  ];
  const body = { work: MyWork, duties: Duties, team: Team, news: News }[tab] || MyWork;
  return html`<div class="tg-company">
    <div class="tg-head">
      <${Btn} icon="chevron_left" label=${w.back} style="width:120px;height:38px" onClick=${back} />
      <${Mark} mark=${v.mark} size=${52} />
      <div class="who"><b class="cut">${v.name}</b><span class="cut">${v.sub}</span></div>
      <span class=${'tg-conn' + (v.lost ? ' lost' : '')}><i></i>${v.lost ? w.reconnecting : w.connected}</span>
      <span class="day">${v.day}</span>
      <${RoleBadge} role=${v.me.role} />
      ${v.can.pages && html`<${Btn} kind="primary" icon="apps" label=${w.pages} title=${w.pages_tip} onClick=${openCompanyPages} />`}
      <${Btn} icon="logout" label=${w.leave} onClick=${leave} />
    </div>
    ${v.lost && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.lost}</p>`}
    ${v.said && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.said}</p>`}
    ${v.driving && html`<p class="tg-banner t-ok"><${Icon} name="directions_bus" size=${16} />${v.driving}</p>`}
    <div class="tg-tabs">${tabs.map(([k, label, icon, n]) => html`<button type="button" class=${tab === k ? 'on' : ''} onClick=${() => setTab(k)}><${Icon} name=${icon} size=${18} /><span>${label}</span>${n > 0 && html`<i>${n}</i>`}</button>`)}</div>
    <${body} v=${v} ask=${ask} />
  </div>`;
}

/** "Join a company": the code field, then the boss's company as the player's role sees it
 * (`together::join::view`). */
export function JoinPage() {
  const [v, ask] = usePoll('tg_view', 2000);
  if (!v) return null;
  const w = v.words || {};
  const back = () => setCo({ chooser: 'mode' });
  if (!v.joined) return html`<${JoinCode} v=${v} ask=${ask} />`;
  const leave = async () => {
    await run('tg_leave');
    seenNotice = null;
    ask();
  };
  // (Beta 63: the session ended - closed by the boss, his launcher lost, sent away: `session.js`)
  if (v.ended) return html`<div class="co-choose tg-join"><${SessionEnded} e=${v.ended} /></div>`;
  if (v.connecting) {
    return html`<div class="co-choose tg-join"><div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${back} /></div>
      <span style="flex:0.4 1 0"></span>
      <div class="tg-joincard">
        <span class="tg-hero-icon big pulse"><${Icon} name="wifi_tethering" size=${34} /></span>
        <div class="head"><h1>${w.connecting}</h1><p><code>${v.code}</code></p>${v.lost && html`<p class="t-warn">${v.lost}</p>`}</div>
        <div style="display:flex;justify-content:center"><${Btn} icon="logout" label=${w.leave} onClick=${leave} /></div>
      </div>
      <span style="flex:0.6 1 0"></span></div>`;
  }
  return html`<${Joined} v=${v} ask=${ask} />`;
}

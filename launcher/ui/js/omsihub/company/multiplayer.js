// The company's Multiplayer page (Beta 61; the Rust side is `omsihub/together/host.rs`): the
// boss shares his company with a session code and runs his players from here - who is online,
// each player's role and the permissions ticked on top of it, their duties of the day and what
// they drove for the company, the duty requests to approve or decline, a duty to give, sending a
// player away, and the activity of the session. Every text comes from Rust, translated; the few
// of this page's own are `tr`'s.

import { toast, S, set } from '../../store.js';
import { language } from '../../i18n.js';
// (the not-shared state's look: the set-B pages' cards)
import { Card, Empty } from './pagesb.js';
import { html, useEffect, useRef, useState, tr, setCo, cmd, refresh, Icon, Btn, Switch, Dialog, inkOn, rgbOf } from './kit.js';

// (the page's own look, loaded with it)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/company-together.css';
document.head.append(sheet);

/** Ask `name` every `ms` while shown: the answer, and a way to ask again at once. */
export function usePoll(name, ms, args = {}) {
  const [v, setV] = useState(null);
  const ask = useRef(() => {});
  const key = JSON.stringify(args);
  useEffect(() => {
    let gone = false;
    ask.current = () =>
      cmd(name, args)
        .then((x) => !gone && setV(x))
        .catch((e) => !gone && setV({ error: e.message }));
    ask.current();
    const t = setInterval(() => ask.current(), ms);
    return () => {
      gone = true;
      clearInterval(t);
    };
  }, [name, key]);
  return [v, () => ask.current()];
}

/** A command whose answer may say no (a popup, or an error to toast). */
export async function run(name, args = {}) {
  let r;
  try {
    r = await cmd(name, args);
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
  if (r?.popup) setCo({ popup: r.popup });
  if (r?.error) toast(r.error, 'error');
  return r;
}

/** Copy a text, said with a tick for a moment. */
export function CopyBtn({ text, label, done, kind = 'primary' }) {
  const [ok, setOk] = useState(false);
  const copy = () =>
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setOk(true);
        setTimeout(() => setOk(false), 1600);
      })
      .catch((e) => toast(e.message, 'error'));
  return html`<${Btn} kind=${kind} icon=${ok ? 'check_circle' : 'content_copy'} label=${ok ? done : label} style="min-width:120px" onClick=${copy} />`;
}

/** A line's number on its colour. */
export function Chip({ number, colour, big }) {
  const rgb = rgbOf(colour && colour.startsWith('#') ? colour : '#5a6270');
  return html`<span class=${'tg-chip' + (big ? ' big' : '')} style=${`background:rgb(${rgb.join(',')});color:${inkOn(rgb)}`}>${number}</span>`;
}

/** A player's round face: his initial, a dot when he is online. */
export function Face({ name, initial, online, size = 36, boss }) {
  const letter = initial || (name || '?').slice(0, 1).toUpperCase();
  return html`<span class=${'tg-face' + (boss ? ' boss' : '')} style=${`width:${size}px;height:${size}px;font-size:${Math.round(size * 0.42)}px`}>${letter}${online != null && html`<i class=${online ? 'on' : ''}></i>`}</span>`;
}

/** A role's badge: its icon and name. */
export function RoleBadge({ role }) {
  if (!role) return null;
  return html`<span class=${'tg-role ' + (role.key || 'boss')} title=${role.about || ''}><${Icon} name=${role.icon} size=${13} />${role.label}</span>`;
}

/** What a player drove for the company, as small figures. */
export function Stats({ st, w }) {
  if (!st) return null;
  const cells = [
    [w.trips, st.trips, ''],
    [w.punctuality, st.punctuality || '-', ''],
    [w.score, st.score ?? '-', ''],
    [w.fares, st.fares, 'ok'],
    [w.fines, st.fines, st.fined ? 'danger' : ''],
  ];
  return html`<div class="tg-stats">${cells.map(([k, v, tone]) => html`<div><span>${k}</span><b class=${tone ? 't-' + tone : ''}>${v}</b></div>`)}</div>`;
}

/** The roles to choose from, and each permission as a tick: the role's ticked, the boss's own
 * changes on top. `onRole(role)`, `onPerms(keys)`. */
export function RoleEditor({ p, roles, w, onRole, onPerms, disabled }) {
  const preset = roles.roles.find((r) => r.key === p.role.key)?.perms || [];
  const on = (k) => p.perms.includes(k);
  const flip = (k) => onPerms(on(k) ? p.perms.filter((x) => x !== k) : [...p.perms, k]);
  return html`<div class="tg-editor">
    <div class="tg-roles">${roles.roles.map((r) => html`<button type="button" class=${'tg-rolepick' + (r.key === p.role.key ? ' on' : '')} disabled=${disabled} title=${r.about} onClick=${() => r.key !== p.role.key && onRole(r.key)}><${Icon} name=${r.icon} size=${16} /><span>${r.label}</span></button>`)}</div>
    <div class="tg-perms">${roles.perms.map((x) => {
      const changed = on(x.key) !== preset.includes(x.key);
      return html`<button type="button" class=${'tg-perm' + (on(x.key) ? ' on' : '') + (changed ? ' changed' : '')} disabled=${disabled} title=${x.label} onClick=${() => flip(x.key)}><${Icon} name=${on(x.key) ? 'check' : x.icon} size=${14} /><span class="cut">${x.label}</span></button>`;
    })}</div>
    ${p.custom && html`<div class="tg-custom"><span class="t-accent2">${w.custom}</span><${Btn} small=${true} label=${w.reset} disabled=${disabled} onClick=${() => onPerms(preset)} /></div>`}
  </div>`;
}

/** A player's card on the page: his face, role and permissions, what he does now, his duties
 * and his figures, and sending him away. */
function PlayerCard({ p, v, go }) {
  const w = v.words;
  const [open, setOpen] = useState(false);
  const role = (key) => go('tg_host_role', { guest: p.no, role: key });
  const perms = (keys) => go('tg_host_role', { guest: p.no, role: p.role.key, perms: keys });
  return html`<div class=${'tg-player' + (p.online ? ' online' : '') + (p.banned ? ' banned' : '')}>
    <div class="top">
      <${Face} name=${p.name} initial=${p.initial} online=${p.online} size=${42} />
      <div class="who"><b class="cut">${p.name}</b><span class=${'cut t-' + p.tone}>${p.text}</span></div>
      <${RoleBadge} role=${p.role} />
      <button type="button" class="tg-more" title=${w.perms_caps} onClick=${() => setOpen(!open)}><${Icon} name=${open ? 'expand_less' : 'tune'} size=${18} /></button>
    </div>
    ${open && html`<${RoleEditor} p=${p} roles=${v.roles} w=${w} onRole=${role} onPerms=${perms} disabled=${p.banned} />`}
    <div class="tg-pduties">
      ${p.duties.length
        ? p.duties.map((d) => html`<div class="tg-pduty"><span class="cut"><b>${d.times}</b> ${d.what}</span><span class="dim cut">${d.state}${d.result ? ' · ' + d.result : ''}</span>
            ${d.asked
              ? html`<span class="acts"><${Btn} small=${true} kind="primary" label=${w.yes} onClick=${() => go('tg_host_answer', { duty: d.duty, yes: true })} /><${Btn} small=${true} label=${w.no} onClick=${() => go('tg_host_answer', { duty: d.duty, yes: false })} /></span>`
              : d.off && html`<span class="acts"><${Btn} small=${true} label=${w.off} onClick=${() => go('tg_host_off', { duty: d.duty })} /></span>`}</div>`)
        : html`<p class="dim">${w.no_duties}</p>`}
    </div>
    <${Stats} st=${p.stats} w=${w} />
    <div class="foot">
      ${p.banned || p.kicked
        ? html`<${Btn} small=${true} icon="restart_alt" label=${w.back} onClick=${() => go('tg_host_back', { guest: p.no })} />`
        : html`${v.on && html`<${Btn} small=${true} icon="logout" label=${w.kick} title=${w.kick_tip} onClick=${() => go('tg_host_kick', { guest: p.no, ban: false })} />`}
            <${Btn} small=${true} kind="danger" icon="close" label=${w.ban} title=${w.ban_tip} onClick=${() => go('tg_host_kick', { guest: p.no, ban: true })} />`}
    </div>
  </div>`;
}

/** Give a duty of the day to a player: the duty, the player, Give. */
function GiveDuty({ v, go }) {
  const w = v.words;
  const [duty, setDuty] = useState(0);
  const [who, setWho] = useState(0);
  const players = v.players.filter((p) => !p.banned && !p.kicked);
  if (!v.giveable.length || !players.length) return html`<p class="dim">${!players.length ? w.no_players : w.no_open}</p>`;
  const d = v.giveable[Math.min(duty, v.giveable.length - 1)];
  const p = players[Math.min(who, players.length - 1)];
  return html`<div class="tg-give">
    <span class="co-select"><select class="co-field" value=${String(duty)} onChange=${(e) => setDuty(Number(e.currentTarget.value))}>
      ${v.giveable.map((x, k) => html`<option value=${String(k)}>${x.times}  ·  ${x.title}${x.who ? '  ·  ' + x.who : ''}</option>`)}
    </select><${Icon} name="expand_more" size=${20} /></span>
    <span class="co-select"><select class="co-field" value=${String(who)} onChange=${(e) => setWho(Number(e.currentTarget.value))}>
      ${players.map((x, k) => html`<option value=${String(k)}>${x.name}${x.online ? ' (' + w.online + ')' : ''}</option>`)}
    </select><${Icon} name="expand_more" size=${20} /></span>
    <${Btn} kind="primary" icon="sync_alt" label=${w.give} onClick=${() => go('tg_host_give', { duty: d.duty, guest: p.no })} />
  </div>`;
}

/** The session's activity: what each player did, by player, and a line to everyone. */
function Activity({ v, go }) {
  const w = v.words;
  const [who, setWho] = useState(null);
  const [say, setSay] = useState('');
  const names = [...new Set((v.notices || []).map((n) => n.who).filter(Boolean))];
  const shown = (v.notices || []).filter((n) => who == null || n.who === who);
  const send = () => {
    if (!say.trim()) return;
    go('tg_host_say', { text: say });
    setSay('');
  };
  return html`<div class="tg-activity">
    ${v.on && html`<div class="tg-say"><input class="co-field" value=${say} placeholder=${w.say} onInput=${(e) => setSay(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && send()} /><${Btn} icon="chat" label=${w.send} onClick=${send} /></div>`}
    ${names.length > 0 && html`<div class="tg-filter"><button type="button" class=${who == null ? 'on' : ''} onClick=${() => setWho(null)}>${w.everyone}</button>${names.map((n) => html`<button type="button" class=${who === n ? 'on' : ''} onClick=${() => setWho(n)}>${n}</button>`)}</div>`}
    <div class="tg-notices">${shown.map((n) => html`<div><span class="dim">${n.at}</span>${n.text}</div>`)}</div>
  </div>`;
}

/** The friends online (Omsi-Hub online's `ol_state`, asked every few seconds while shown), each
 * with Invite - sharing started first when it is not -; signed out, the way to sign in. */
function Friends({ go }) {
  const [o] = usePoll('ol_state', 5000, { lang: language() });
  const signIn = () => set({ hub: { ...S.hub, online: 'account' } });
  if (!o || o.error) return null;
  if (o.mode !== 'in' || o.need_profile) {
    return html`<${Card} icon="group" title=${tr('Your friends online')}>
      <p class="co-pgb-text">${tr('Sign in or make an account to see which friends are online and invite them.')}</p>
      <div style="margin-top:10px"><${Btn} kind="primary" icon="person" label=${tr('Sign in')} onClick=${signIn} /></div>
    </${Card}>`;
  }
  const online = (o.friends || []).filter((f) => f.state !== 'offline');
  const invite = async (f) => {
    if (!o.hosting) await go('tg_host_start');
    const r = await run('ol_invite', { id: f.id, lang: language() });
    if (r?.message) toast(r.message, 'ok');
  };
  return html`<${Card} icon="group" title=${tr('Your friends online')} aside=${String(online.length)}>
    ${online.length
      ? online.map((f) => html`<div class="co-pgb-mp-friend"><span class=${'co-pgb-mp-dot ' + f.state}></span><span class="co-pgb-grow"><b class="co-pgb-cut">${f.name}</b><span class="co-pgb-note">${f.state_label}</span></span>
          <${Btn} small=${true} kind="primary" icon="wifi_tethering" label=${tr('Invite')} onClick=${() => invite(f)} /></div>`)
      : html`<${Empty} icon="group" text=${tr('No friend is online right now.')} />`}
  </${Card}>`;
}

/** The page before sharing: the hero with Start sharing, the three steps, the friends online and
 * the roles. */
function NotShared({ v, go, inDialog, players }) {
  const w = v.words || {};
  return html`<div class=${'co-pgb co-pgb-mp' + (inDialog ? ' in-dialog' : '')}>
    <div class="co-pgb-stack">
      <section class="co-pgb-card co-pgb-mp-hero">
        <span class="co-pgb-mp-hero-ic"><${Icon} name="wifi_tethering" size=${34} /></span>
        <span class="co-pgb-grow"><h2 class="co-pgb-h1">${tr('Run your company together')}</h2><p class="co-pgb-text">${w.intro}</p>
          ${v.other && html`<p class="co-pgb-note co-pgb-t-warn">${v.other}</p>`}</span>
        <${Btn} kind="primary" class="co-pgb-big" icon="wifi_tethering" label=${w.start} onClick=${() => go('tg_host_start')} />
      </section>
      <div class="co-pgb-mp-steps">${(w.how || []).map((t, k) => html`<section class="co-pgb-card co-pgb-mp-step"><span class="co-pgb-mp-num">${k + 1}</span><p class="co-pgb-text">${t}</p></section>`)}</div>
      <div class="co-pgb-mp-cols">
        <${Friends} go=${go} />
        <${Card} icon="badge" title=${tr('Roles')}>
          ${(v.roles?.roles || []).map((r) => html`<div class="co-pgb-mp-role"><span class="co-pgb-mp-role-ic"><${Icon} name=${r.icon} size=${16} /></span><span class="co-pgb-grow"><b>${r.label}</b><span class="co-pgb-note">${r.about}</span></span></div>`)}
        </${Card}>
      </div>
      ${v.players.length > 0 && players}
      <p class="co-pgb-note">${w.not_shared}</p>
    </div>
  </div>`;
}

/** The Multiplayer page (`together::host::view`); `inDialog`: shown in a dialog over another
 * page (RosterFlow's button). */
export function MultiplayerPage({ inDialog }) {
  const [v, ask] = usePoll('tg_host_view', 2000);
  if (!v || v.none) return null;
  if (v.error) return html`<div class="co-empty">${v.error}</div>`;
  const w = v.words || {};
  const go = async (name, args) => {
    await run(name, args);
    ask();
    refresh();
  };
  const head = html`<div class="tg-hero">
    <span class="tg-hero-icon"><${Icon} name="wifi_tethering" size=${28} /></span>
    <div class="who"><b>${w.title}</b><span class=${v.on ? 't-ok' : 'dim'}>${v.on ? v.online : w.not_shared}</span></div>
    ${v.on
      ? html`<${Btn} kind="danger" icon="stop_circle" label=${w.stop} onClick=${() => go('tg_host_stop')} />`
      : html`<${Btn} kind="primary" big=${true} icon="wifi_tethering" label=${w.start} onClick=${() => go('tg_host_start')} />`}
  </div>`;
  const players = html`<section class="co-section"><div class="co-caps">${w.players_caps}</div>
    ${v.players.length ? html`<div class="tg-players">${v.players.map((p) => html`<${PlayerCard} key=${p.no} p=${p} v=${v} go=${go} />`)}</div>` : html`<p class="dim">${w.no_players}</p>`}
  </section>`;
  if (!v.on) return html`<${NotShared} v=${v} go=${go} inDialog=${inDialog} players=${players} />`;
  return html`<div class=${'tg-page' + (inDialog ? ' in-dialog' : '')}>
    ${head}
    <div class="tg-grid">
      <div class="co-scroll tg-side">
        <section class="co-section tg-code"><div class="co-caps">${w.code_caps}</div>
          <code class="tg-codetext">${v.code}</code>
          <${CopyBtn} text=${v.code} label=${w.copy} done=${w.copied} />
          <div class="co-caps" style="margin-top:6px">${w.ways_caps}</div>
          ${v.ways.map(([a, b, tone]) => html`<div class="way"><span class="dim">${a}</span><span class=${'cut t-' + tone}>${b}</span></div>`)}
          <p class="note">${v.note}</p></section>
        <section class="co-section"><${Switch} checked=${v.open_claims} label=${w.open_label} title=${w.open_note} onChange=${(on) => go('tg_host_open', { on })} />
          <p class="note">${w.open_note}</p>
          <${Switch} checked=${v.left_open} label=${w.left_label} title=${w.left_note} onChange=${(on) => go('tg_host_left_open', { on })} />
          <p class="note">${w.left_note}</p></section>
        <section class="co-section"><div class="co-caps">${w.asked_caps}${v.asked.length > 0 && html` <span class="tg-count">${v.asked.length}</span>`}</div>
          ${v.asked.length
            ? v.asked.map((a) => html`<div class="tg-ask"><${Face} name=${a.name} size=${28} /><div class="what"><b class="cut">${a.name}</b><span class="cut">${a.what}</span><span class="dim">${a.times}</span></div>
                <span class="acts"><${Btn} small=${true} kind="primary" label=${w.yes} onClick=${() => go('tg_host_answer', { duty: a.duty, yes: true })} /><${Btn} small=${true} label=${w.no} onClick=${() => go('tg_host_answer', { duty: a.duty, yes: false })} /></span></div>`)
            : html`<p class="dim">${w.no_asked}</p>`}</section>
        <section class="co-section"><div class="co-caps">${w.give_caps}</div><${GiveDuty} v=${v} go=${go} /></section>
      </div>
      <div class="co-scroll tg-main">
        ${players}
        <section class="co-section"><div class="co-caps">${w.activity_caps}</div><${Activity} v=${v} go=${go} /></section>
      </div>
    </div>
  </div>`;
}

/** The Multiplayer page in a dialog (RosterFlow's button, the bar's while another page shows). */
export function MultiplayerDialog() {
  const close = () => setCo({ dialog: null });
  return html`<${Dialog} icon="wifi_tethering" title=${tr('Multiplayer')} w=${1080} onClose=${close} bodyStyle="height:min(78vh,760px);overflow:hidden;padding:0 16px 16px">
    <${MultiplayerPage} inDialog=${true} />
  </${Dialog}>`;
}

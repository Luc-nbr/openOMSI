// "Play together" in the bus company (Beta 58; the Rust side is `omsihub/together`): the boss's
// panel in his company (start and end the session, the code to copy, the ways in, his world,
// the players online and what each drives, the duties asked for, what happened), the bar's
// button that opens it, and a friend's page - the code field of "Join a company", then the
// boss's company in a simple view of its own: the day, the open duties to take, his own duties
// to drive or give back, his results and his history. Every text comes from Rust, translated.

import { toast } from '../../store.js';
import { duty } from '../../data.js';
import { html, useEffect, useRef, useState, tr, setCo, cmd, refresh, Icon, Mark, Btn, Switch, Dialog, inkOn, rgbOf } from './kit.js';
import { togetherOn, blockedLabel } from './gate.js';

// (the page's own look, loaded with it)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/company-together.css';
document.head.append(sheet);

/** Ask `name` every `ms` while shown: the answer, and a way to ask again at once. */
function usePoll(name, ms) {
  const [v, setV] = useState(null);
  const ask = useRef(() => {});
  useEffect(() => {
    let gone = false;
    ask.current = () => cmd(name).then((x) => !gone && setV(x)).catch((e) => !gone && setV({ error: e.message }));
    ask.current();
    const t = setInterval(() => ask.current(), ms);
    return () => {
      gone = true;
      clearInterval(t);
    };
  }, [name]);
  return [v, () => ask.current()];
}

/** A command whose answer may say no (a popup, or an error to toast). */
async function run(name, args = {}) {
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

/** The company bar's button: the session's players online (the duties asked for in orange). */
export function TogetherButton() {
  // (blocked for now: shown, disabled, as coming soon - `gate.js`)
  if (!togetherOn()) return html`<span class="tg-btn" title=${blockedLabel()}><${Btn} class="sq" icon="groups" disabled=${true} title=${blockedLabel()} /></span><span class="gap8"></span>`;
  return html`<${LiveTogetherButton} />`;
}

function LiveTogetherButton() {
  const [b] = usePoll('tg_host_badge', 3000);
  const n = b?.on ? b.asked || b.online : 0;
  return html`<span class="tg-btn"><${Btn} class="sq" kind=${b?.on ? 'primary' : ''} icon="groups" title=${b?.tip || tr('Play together')} onClick=${() => setCo({ dialog: { kind: 'together' } })} />
    ${n > 0 && html`<span class=${'tg-badge' + (b.asked ? ' warn' : '')}>${n}</span>`}</span><span class="gap8"></span>`;
}

/** Copy a text, said with a tick for a moment. */
function CopyBtn({ text, label, done }) {
  const [ok, setOk] = useState(false);
  const copy = () =>
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setOk(true);
        setTimeout(() => setOk(false), 1600);
      })
      .catch((e) => toast(e.message, 'error'));
  return html`<${Btn} kind="primary" icon=${ok ? 'check_circle' : 'content_copy'} label=${ok ? done : label} style="min-width:120px" onClick=${copy} />`;
}

/** The boss's panel (`together::host::view`). */
export function TogetherDialog() {
  const [v, ask] = usePoll('tg_host_view', 2000);
  const [say, setSay] = useState('');
  const close = () => setCo({ dialog: null });
  if (!v || v.none) return null;
  const w = v.words || {};
  const go = async (name, args) => {
    await run(name, args);
    ask();
    refresh();
  };
  const world = async () => {
    let base;
    try {
      base = await duty();
    } catch (e) {
      toast(e.message, 'error');
      return;
    }
    go('tg_host_world', { base });
  };
  const send = () => {
    if (!say.trim()) return;
    go('tg_host_say', { text: say });
    setSay('');
  };
  const foot = v.on
    ? html`<${Btn} kind="danger" icon="logout" label=${w.stop} onClick=${() => go('tg_host_stop')} /><span class="rest"></span><${Btn} label=${w.close} onClick=${close} />`
    : html`<span class="rest"></span><${Btn} label=${w.close} onClick=${close} /><${Btn} kind="primary" icon="groups" label=${w.start} onClick=${() => go('tg_host_start')} />`;
  return html`<${Dialog} icon="groups" title=${w.title} w=${760} onClose=${close} foot=${foot} bodyStyle="max-height:min(74vh,720px);overflow:auto">
    ${!v.on
      ? html`<p class="co-para">${w.intro}</p>
        <ol class="tg-how">${(w.how || []).map((t) => html`<li>${t}</li>`)}</ol>
        ${v.other && html`<p class="co-para t-warn">${v.other}</p>`}`
      : html`<div class="tg-host">
        <div class="tg-code"><span class="co-caps">${w.code_caps}</span>
          <div class="row"><code class="cut">${v.code}</code><${CopyBtn} text=${v.code} label=${w.copy} done=${w.copied} /></div>
          ${v.ways.map(([a, b, tone]) => html`<div class="way"><span class="dim">${a}</span><span class=${'cut t-' + tone}>${b}</span></div>`)}
          <p class="note">${v.note}</p></div>
        <div class=${'tg-world' + (v.world.open ? ' open' : '')}><${Icon} name=${v.world.open ? 'public' : 'warning'} size=${18} /><span>${v.world.text}</span>
          ${v.world.button && html`<${Btn} icon="play_arrow" label=${v.world.button} onClick=${world} />`}</div>
        <${Switch} checked=${v.open_claims} label=${w.open_label} title=${w.open_note} onChange=${(on) => go('tg_host_open', { on })} />
        <p class="note">${w.open_note}</p>
        <div class="co-caps">${w.players_caps}</div>
        ${v.players.length ? v.players.map((p) => html`<div class="tg-line"><i class=${'dot' + (p.online ? ' on' : '')}></i><b class="cut">${p.name}</b><span class=${'cut t-' + p.tone}>${p.text}</span></div>`) : html`<p class="dim">${w.no_players}</p>`}
        ${v.asked.length > 0 && html`<div class="co-caps">${w.asked_caps}</div>
          ${v.asked.map((a) => html`<div class="tg-line"><b class="cut">${a.name}</b><span class="cut">${a.what}</span><span class="dim">${a.times}</span>
            <span class="acts"><${Btn} kind="primary" label=${w.yes} onClick=${() => go('tg_host_answer', { duty: a.duty, yes: true })} /><${Btn} label=${w.no} onClick=${() => go('tg_host_answer', { duty: a.duty, yes: false })} /></span></div>`)}`}
        ${v.theirs.length > 0 && html`<div class="co-caps">${w.duties_caps}</div>
          ${v.theirs.map((a) => html`<div class="tg-line"><b class="cut">${a.name}</b><span class="cut">${a.what}</span><span class="dim">${a.times} · ${a.state}</span>
            <span class="acts">${a.off && html`<${Btn} label=${w.off} onClick=${() => go('tg_host_off', { duty: a.duty })} />`}</span></div>
            ${a.result && html`<p class="note tg-result-line">${a.result}</p>`}`)}`}
        <div class="co-caps">${w.notices_caps}</div>
        <div class="tg-say"><input class="co-field" value=${say} placeholder=${w.say} onInput=${(e) => setSay(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && send()} /><${Btn} label=${w.send} onClick=${send} /></div>
        <div class="tg-notices">${v.notices.map((n) => html`<div><span class="dim">${n.at}</span>${n.text}</div>`)}</div>
      </div>`}
  </${Dialog}>`;
}

// --- a friend's side -----------------------------------------------------------------------------

/** A line's number on its colour. */
function Chip({ number, colour }) {
  const rgb = rgbOf(colour && colour.startsWith('#') ? colour : '#5a6270');
  return html`<span class="tg-chip" style=${`background:rgb(${rgb.join(',')});color:${inkOn(rgb)}`}>${number}</span>`;
}

/** A duty of the boss's day: what it is, who has it, what can be done with it. */
function DutyRow({ d, again }) {
  const act = async (a) => {
    if (a.act === 'drive') {
      let base;
      try {
        base = await duty();
      } catch (e) {
        toast(e.message, 'error');
        return;
      }
      await run('tg_drive', { duty: d.duty, base });
    } else await run(a.act === 'take' ? 'tg_take' : 'tg_release', { duty: d.duty });
    again();
  };
  return html`<div class="tg-duty" title=${d.title}>
    <${Chip} number=${d.number} colour=${d.colour} />
    <div class="what"><b>${d.times}</b><span class="cut">${d.route}</span><span class="dim cut">${d.facts}</span></div>
    <span class=${'state cut t-' + d.tone}>${d.state}</span>
    <span class="acts">${d.acts.map((a) => html`<${Btn} kind=${a.kind} label=${a.label} disabled=${!!a.off} onClick=${() => act(a)} />`)}</span>
  </div>`;
}

/** "Join a company": the code field, then the boss's company (`together::join::view`). */
export function JoinPage() {
  const [v, ask] = usePoll('tg_view', 2000);
  const [code, setCode] = useState('');
  const [check, setCheck] = useState(null);
  const [say, setSay] = useState('');
  useEffect(() => {
    if (v?.joined && !v.connecting) toastNew(v.notices, v.last_notice || 0);
  }, [v?.last_notice]);
  if (!v) return null;
  const w = v.words || {};
  const back = () => setCo({ chooser: 'mode' });
  if (!v.joined) {
    const typed = (t) => {
      setCode(t);
      cmd('tg_check', { code: t }).then(setCheck).catch(() => {});
    };
    const join = async () => {
      const r = await run('tg_join', { code });
      if (r && !r.error) ask();
    };
    return html`<div class="co-choose tg-join">
      <div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${back} /></div>
      <div class="co-scroll" style="flex:1;margin-top:12px;display:flex;flex-direction:column;align-items:center">
        <span style="flex:0.3 1 0"></span>
        <div class="head"><h1>${w.title}</h1><p>${w.text}</p></div>
        <div class="tg-field"><input class="co-field" value=${code} placeholder=${w.field} onInput=${(e) => typed(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && check?.ok && join()} />
          <${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!check?.ok} onClick=${join} /></div>
        ${check?.error && html`<p class="t-warn tg-why">${check.error}</p>`}
        <ul class="tg-need">${(w.need || []).map((t) => html`<li>${t}</li>`)}</ul>
        ${v.history?.length > 0 && html`<div class="tg-history"><div class="co-caps">${w.history_caps}</div>${v.history.map((h) => html`<div class="tg-line"><b class="cut">${h.title}</b><span class="dim cut">${h.text}</span></div>`)}</div>`}
        <span style="flex:0.7 1 0"></span>
      </div>
    </div>`;
  }
  const leave = async () => {
    await run('tg_leave');
    seenNotice = null;
    ask();
  };
  if (v.connecting) {
    return html`<div class="co-choose tg-join"><div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${back} /></div>
      <span style="flex:0.4 1 0"></span>
      <div class="head"><h1>${w.connecting}</h1><p><code>${v.code}</code></p>${v.lost && html`<p class="t-warn">${v.lost}</p>`}</div>
      <div style="display:flex;justify-content:center"><${Btn} icon="logout" label=${w.leave} onClick=${leave} /></div>
      <span style="flex:0.6 1 0"></span></div>`;
  }
  const send = () => {
    if (!say.trim()) return;
    run('tg_say', { text: say }).then(ask);
    setSay('');
  };
  const list = (rows, empty) => (rows.length ? rows.map((d) => html`<${DutyRow} d=${d} again=${ask} />`) : empty && html`<p class="dim">${empty}</p>`);
  return html`<div class="tg-company">
    <div class="tg-head">
      <${Btn} icon="chevron_left" label=${w.back} style="width:120px;height:38px" onClick=${back} />
      <${Mark} mark=${v.mark} size=${48} />
      <div class="who"><b class="cut">${v.name}</b><span class="cut">${v.sub}</span></div>
      <span class="day">${v.day}</span>
      <${Btn} icon="logout" label=${w.leave} onClick=${leave} />
    </div>
    ${v.lost && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.lost}</p>`}
    ${v.said && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.said}</p>`}
    <p class=${'tg-banner' + (v.world.open ? ' t-ok' : ' t-dim')}><${Icon} name=${v.world.open ? 'public' : 'schedule'} size=${16} />${v.world.text}</p>
    ${v.driving && html`<p class="tg-banner t-ok"><${Icon} name="directions_bus" size=${16} />${v.driving}</p>`}
    <div class="tg-cols">
      <div class="co-scroll tg-main">
        <section class="co-section"><div class="co-caps">${w.mine_caps}</div>${list(v.mine, w.no_mine)}</section>
        <section class="co-section"><div class="co-caps">${w.open_caps}</div><p class="note">${v.take_note}</p>${list(v.open, v.reading ? '…' : w.no_open)}</section>
        ${v.rest.length > 0 && html`<section class="co-section"><div class="co-caps">${w.rest_caps}</div>${list(v.rest)}</section>`}
      </div>
      <div class="co-scroll tg-side">
        <section class="co-section"><div class="co-caps">${w.players_caps}</div>
          ${v.players.map((p) => html`<div class="tg-line"><i class=${'dot' + (p.online ? ' on' : '')}></i><b class="cut">${p.name}</b><span class="dim cut">${p.text}</span></div>`)}</section>
        ${v.results.length > 0 && html`<section class="co-section"><div class="co-caps">${w.results_caps}</div>
          ${v.results.map((r) => html`<div class="tg-result"><b class="cut">${r.title}</b><span class="dim">${r.text}</span></div>`)}</section>`}
        <section class="co-section"><div class="co-caps">${w.notices_caps}</div>
          <div class="tg-say"><input class="co-field" value=${say} placeholder=${w.say} onInput=${(e) => setSay(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && send()} /><${Btn} label=${w.send} onClick=${send} /></div>
          <div class="tg-notices">${v.notices.map((n) => html`<div><span class="dim">${n.at}</span>${n.text}</div>`)}</div></section>
        ${v.history.length > 0 && html`<section class="co-section"><div class="co-caps">${w.history_caps}</div>
          ${v.history.map((h) => html`<div class="tg-result"><b class="cut">${h.title}</b><span class="dim">${h.text}</span></div>`)}</section>`}
      </div>
    </div>
  </div>`;
}


// The control room's messages on the Operations page (docs/omsihub/MESSAGES.md): the "Message"
// button of each row (a late bus, a late driver, a breakdown, a driver's own message) and of the
// page's head (every driver), the compose box - a canned message or the sender's own words, a
// priority, an instruction the driver accepts or declines (the row's ways out, a hold) - and the
// list of the day's messages with where each stands (sent, delivered, read, accepted, declined).
// The view and its words come from `omsi-launcher-api`'s `company/messages.rs` (`ops`'s
// `compose`, `log`); what is sent is `ops_msg`, a driver's message seen to `ops_msg_ack`.

import { html, useState, act, setUi, ui, Icon, Btn, Seg, Select, Dialog } from './kit.js';
import { Card } from './pagesb.js';

// (this part's own look, loaded with it)
if (!document.querySelector('link[data-co-msg-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-msg.css';
  sheet.dataset.coMsgCss = '1';
  document.head.append(sheet);
}

/** Open the compose box for row `x` (its driver: `x.to`; its instructions: `x.instr`). */
export function openCompose(x) {
  setUi('ops_msg', { open: { to: x.to, instr: x.instr || [], min: x.min || 0 } });
}

/** A row's "Message" button. */
export function MsgButton({ x, v }) {
  if (!x.to || !v.compose) return null;
  const go = (e) => {
    e.stopPropagation();
    openCompose(x);
  };
  return html`<button type="button" class="co-msg-btn" title=${v.compose.title} onClick=${go}><${Icon} name="chat" size=${14} /><span>${v.compose.button}</span></button>`;
}

/** The compose box, while one is open. */
export function Compose({ v }) {
  const st = ui('ops_msg');
  const c = v.compose;
  const [template, setTemplate] = useState(-1);
  const [text, setText] = useState('');
  const [instr, setInstr] = useState(-1);
  const [prio, setPrio] = useState('info');
  const [busy, setBusy] = useState(false);
  if (!st.open || !c) return null;
  const o = st.open;
  const close = () => {
    setUi('ops_msg', { open: null });
    setTemplate(-1);
    setText('');
    setInstr(-1);
    setPrio('info');
  };
  const send = async () => {
    setBusy(true);
    try {
      const r = await act('ops_msg', {
        to: o.to,
        template: template >= 0 ? c.templates[template].key : '',
        text,
        priority: instr >= 0 && prio === 'info' ? 'instruction' : prio,
        action: instr >= 0 ? o.instr[instr].args : null,
        min: o.min,
      });
      if (r && !r.error) close();
    } finally {
      setBusy(false);
    }
  };
  const can = !busy && (template >= 0 || text.trim() !== '' || instr >= 0);
  const foot = html`<span class="rest"></span><${Btn} label=${c.cancel} onClick=${close} /><${Btn} kind="primary" icon="near_me" label=${c.send} disabled=${!can} onClick=${send} />`;
  return html`<${Dialog} icon="forum" title=${c.title} w=${600} onClose=${close} foot=${foot}>
    <div class="co-msg-form">
      <div class="co-msg-to"><span class="co-msg-label">${c.to}</span><b>${o.to.label}</b></div>
      ${o.instr.length > 0 && html`<label class="co-msg-field"><span class="co-msg-label">${c.instruction}</span>
        <${Select} value=${instr + 1} options=${[c.none, ...o.instr.map((i) => i.label)]} onChange=${(k) => setInstr(k - 1)} /></label>`}
      <label class="co-msg-field"><span class="co-msg-label">${c.template}</span>
        <${Select} value=${template + 1} options=${['—', ...c.templates.map((t) => t.label.replace('%{min}', String(o.min || '…')))]} onChange=${(k) => setTemplate(k - 1)} /></label>
      <label class="co-msg-field"><span class="co-msg-label">${c.own}</span>
        <textarea class="co-field co-msg-text" maxlength="240" rows="2" placeholder=${c.own_ph} value=${text} onInput=${(e) => setText(e.currentTarget.value)}></textarea></label>
      <div class="co-msg-field"><span class="co-msg-label">${c.priority}</span>
        <${Seg} value=${prio} options=${c.priorities} onChange=${setPrio} /></div>
    </div>
  </${Dialog}>`;
}

/** The day's messages: from, to, what, where it stands. */
export function MsgLog({ v }) {
  const list = v.log || [];
  const caps = v.log_caps;
  if (!caps) return null;
  const all = html`<button type="button" class="co-msg-btn" onClick=${() => openCompose({ to: caps.all_to })}><${Icon} name="campaign" size=${14} /><span>${caps.all}</span></button>`;
  return html`<${Card} class="co-msg-log flush" icon="forum" title=${caps.title} aside=${all}>
    ${list.length === 0 && html`<div class="co-msg-empty">${caps.empty}</div>`}
    ${list.length > 0 && html`<div class="co-msg-table">
      <div class="co-msg-row head">${caps.cols.map((c) => html`<span>${c}</span>`)}</div>
      ${list.map((m) => html`<div class=${'co-msg-row prio-' + m.prio.key + (m.mine ? ' mine' : '') + (m.open ? ' open' : '')}>
        <span class="co-msg-time">${m.time}</span>
        <span class="co-msg-who" title=${m.from}>${m.from}</span>
        <span class="co-msg-who" title=${m.to}>${m.to}</span>
        <span class="co-msg-body"><i class="co-msg-prio" title=${m.prio.label}></i><span>${m.text}</span>
          ${m.reply && html`<em class="co-msg-reply">↳ ${m.reply}</em>`}
          ${m.outcome && html`<em class="co-msg-outcome">${m.outcome}</em>`}</span>
        <span class=${'co-msg-state st-' + m.state.key}>${m.state.label}</span>
      </div>`)}
    </div>`}
  </${Card}>`;
}

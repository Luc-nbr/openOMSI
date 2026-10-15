// The Sandbox page (Beta 61): a sandbox company's rules, set by the player - its knobs in their
// groups (money, unlocks, the economy, the operations, the staff), each a switch or a factor with
// what it does and its career value; the balance and the reputation set by hand; and every knob
// back to the career's. The view and its words come from `omsi-launcher-api`'s
// `company/sandbox.rs` (`co_view` "sandbox"), translated; the badge by a sandbox's name too.

import { html, act, useView, useState, setCo, Icon, Btn, Switch, Section, Slider } from './kit.js';

// (this page's own look, loaded with it - the badge by the name uses it as well)
if (!document.querySelector('link[data-co-sandbox-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-sandbox.css';
  sheet.dataset.coSandboxCss = '1';
  document.head.append(sheet);
}

/** The badge by a sandbox company's name (`sandbox::badge`); a click opens its page. */
export function SandboxBadge({ b, onClick, small }) {
  if (!b) return null;
  return html`<span class=${'co-sandbox-badge' + (small ? ' small' : '') + (onClick ? ' go' : '')} title=${b.tip} onClick=${onClick}>
    <${Icon} name=${b.icon} size=${small ? 12 : 14} /><span>${b.label}</span>
  </span>`;
}

/** A factor ("×1.5") as it is dragged, before the page has its own words for it. */
const times = (x) => '×' + String(Math.round(x * 100) / 100);

/** A switch: its name, what it does, its career value. */
function SwitchKnob({ k }) {
  return html`<div class=${'co-sb-knob' + (k.changed ? ' changed' : '')}>
    <${Switch} checked=${k.on} label=${html`<b>${k.label}</b>`} onChange=${(on) => act('sandbox_knob', { key: k.key, on })} />
    <p class="co-note">${k.info}</p>
    <span class="career">${k.career}</span>
  </div>`;
}

/** A factor: its slider (sent when let go), its value, back to the career's. */
function FactorKnob({ k }) {
  const [drag, setDrag] = useState(null);
  const send = (x) => {
    setDrag(null);
    act('sandbox_knob', { key: k.key, value: x });
  };
  return html`<div class=${'co-sb-knob' + (k.changed ? ' changed' : '')}>
    <div class="head"><b class="grow">${k.label}</b><span class="val">${drag != null ? times(drag) : k.text}</span></div>
    <p class="co-note">${k.info}</p>
    <div class="slide">
      <${Slider} value=${drag ?? k.value} min=${k.least} max=${k.most} step=${0.05} format=${() => ''} onChange=${setDrag} onRelease=${send} style="flex:1;min-width:0" />
      ${k.reset && html`<${Btn} small icon="restart_alt" label=${k.reset} onClick=${() => send(1)} />`}
    </div>
    <span class="career">${k.career}</span>
  </div>`;
}

/** A number typed in and set with its button (the balance, the reputation). */
function SetRow({ label, value, tone, start, step, min, max, button, onSet }) {
  const [typed, setTyped] = useState(null);
  const go = async () => {
    const x = Number(typed ?? start);
    if (!Number.isFinite(x)) return;
    await onSet(x);
    setTyped(null);
  };
  return html`<div class="co-sb-set">
    <span class="what"><span class="co-label">${label}</span><b class=${'t-' + (tone || 'text')}>${value}</b></span>
    <input type="number" class="co-field" step=${step} min=${min} max=${max} value=${typed ?? start} onInput=${(e) => setTyped(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && go()} />
    <${Btn} kind="primary" label=${button} onClick=${go} />
  </div>`;
}

export function SandboxPage() {
  const v = useView('sandbox');
  if (!v) return null;
  if (v.career) {
    return html`<div class="co-sb"><${Section}><p class="co-para">${v.career}</p>
      <div><${Btn} icon="settings" label=${v.settings} onClick=${() => setCo({ dialog: { kind: 'settings' } })} /></div></${Section}></div>`;
  }
  const b = v.balance;
  const r = v.reputation;
  return html`<div class="co-sb">
    <div class="co-sb-top">
      <${Icon} name="sports_esports" size=${28} class="t-accent2" />
      <span class="grow"><b class="title">${v.title}</b><span class="co-note">${v.intro}</span></span>
      <span class="changed t-dim">${v.changed}</span>
      ${v.reset_all && html`<${Btn} icon="restart_alt" label=${v.reset_all} onClick=${() => act('sandbox_reset')} />`}
    </div>
    <div class="co-sb-grid">
      ${v.groups.map(
        (g) => html`<${Section} class="co-sb-group" title=${html`<span class="co-sb-caps"><${Icon} name=${g.icon} size=${15} />${g.caps}</span>`}>
          ${g.knobs.map((k) => (k.switch ? html`<${SwitchKnob} k=${k} />` : html`<${FactorKnob} k=${k} />`))}
        </${Section}>`,
      )}
      <${Section} class="co-sb-group" title=${v.set_caps}>
        <${SetRow} label=${b.label} value=${b.value} tone=${b.tone} start=${b.euros} step=${1000} button=${b.set} onSet=${(x) => act('sandbox_balance', { euros: x })} />
        <p class="co-note">${b.note} ${b.value !== b.booked ? b.booked : ''}</p>
        <${SetRow} label=${r.label} value=${r.value} start=${r.points} step=${1} min=${0} max=${100} button=${r.set} onSet=${(x) => act('sandbox_reputation', { value: x })} />
      </${Section}>
    </div>
  </div>`;
}

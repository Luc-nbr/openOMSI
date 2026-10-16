// The Sandbox page (Beta 61): a sandbox company's rules, set by the player - its knobs in their
// groups (money, unlocks, the economy, the operations, the staff), each a switch or a factor with
// what it does and its career value; the balance and the reputation set by hand; and every knob
// back to the career's. The view and its words come from `omsi-launcher-api`'s
// `company/sandbox.rs` (`co_view` "sandbox"), translated; the badge by a sandbox's name too.
//
// Beta 65, after Luc's reference: the head with how many settings are changed and "Reset to
// career"; the quick presets (Relaxed, Career, Hard, and Custom for a mix of one's own); the
// groups as a list of tabs on the left, the one chosen as a card of its settings on the right -
// a row a knob with its icon, a dot when it is changed, what it does, and a slider with its ticks,
// its value and the career's, or a switch.

import { html, act, useView, useState, ui, setUi, setCo, Icon, Btn, Switch, Section } from './kit.js';
import { Card } from './pagesb.js';

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

/** Each knob's icon, and each preset's. */
const KNOB_ICON = {
  unlimited_money: 'payments', no_bankruptcy: 'emergency_home', free_loans: 'receipt_long', unlock_all: 'key',
  income: 'trending_up', wages: 'groups', running: 'speed', prices: 'directions_bus', penalties: 'warning', inflation: 'calendar_month',
  breakdowns: 'construction', accidents: 'directions_car', lateness: 'schedule', sickness: 'thermostat', reputation_gain: 'star', reputation_loss: 'military_tech',
  complaints: 'chat', morale: 'users', holidays: 'wb_sunny', free_hiring: 'group',
};
const PRESET_ICON = { relaxed: 'wb_sunny', career: 'military_tech', hard: 'signal_cellular_alt', custom: 'settings' };

/** The ticks under a factor's slider: the round factors in its range, none crowding the last. */
function ticks(least, most) {
  const out = [];
  for (const t of [0, 0.5, 1, 2, 3, 5, 10]) {
    if (t < least - 1e-9 || t > most + 1e-9) continue;
    const f = (t - least) / (most - least);
    if (out.length && f - out[out.length - 1][1] < 0.08) continue;
    out.push([t, f]);
  }
  return out;
}

/** A setting's row: its icon, the dot when it is changed, its name and what it does, its control. */
function Row({ k, children }) {
  return html`<div class=${'co-pgb-sb-row' + (k.changed ? ' changed' : '')}>
    <span class="co-pgb-sb-ic"><${Icon} name=${KNOB_ICON[k.key] || 'tune'} size=${17} /></span>
    <span class="co-pgb-sb-words"><b>${k.changed && html`<i class="co-pgb-sb-dot"></i>`}${k.label}</b><span class="co-pgb-note">${k.info}</span></span>
    <span class="co-pgb-sb-ctl">${children}</span>
  </div>`;
}

/** A switch: On or Off and its toggle. */
function SwitchKnob({ k, v }) {
  return html`<${Row} k=${k}>
    <span class="co-pgb-sb-onoff">${k.on ? v.on : v.off}</span>
    <${Switch} checked=${k.on} title=${k.career} onChange=${(on) => act('sandbox_knob', { key: k.key, on })} />
  </${Row}>`;
}

/** A factor: its slider with its ticks (sent when let go), its value and the career's. */
function FactorKnob({ k }) {
  const [drag, setDrag] = useState(null);
  const send = (x) => {
    setDrag(null);
    act('sandbox_knob', { key: k.key, value: x });
  };
  const value = drag ?? k.value;
  const span = k.most - k.least;
  return html`<${Row} k=${k}>
    <span class="co-pgb-sb-slide">
      <input type="range" min=${k.least} max=${k.most} step="0.05" value=${value} style=${`--f:${span > 0 ? (value - k.least) / span : 0}`}
        onInput=${(e) => setDrag(Number(e.currentTarget.value))} onChange=${(e) => send(Number(e.currentTarget.value))} />
      <span class="co-pgb-sb-ticks">${ticks(k.least, k.most).map(([t, f]) => html`<span style=${`--f:${f}`}>${times(t)}</span>`)}</span>
    </span>
    <span class="co-pgb-sb-chips">
      <span class="co-pgb-sb-val co-pgb-num">${drag != null ? times(drag) : k.text}</span>
      ${k.reset ? html`<button type="button" class="co-pgb-sb-career go" title=${k.reset} onClick=${() => send(1)}><${Icon} name="restart_alt" size=${12} />${k.career}</button>` : html`<span class="co-pgb-sb-career">${k.career}</span>`}
    </span>
  </${Row}>`;
}

/** A number typed in and set with its button (the balance, the reputation). */
function SetRow({ icon, label, value, tone, start, step, min, max, button, onSet, note }) {
  const [typed, setTyped] = useState(null);
  const go = async () => {
    const x = Number(typed ?? start);
    if (!Number.isFinite(x)) return;
    await onSet(x);
    setTyped(null);
  };
  return html`<div class="co-pgb-sb-row">
    <span class="co-pgb-sb-ic"><${Icon} name=${icon} size=${17} /></span>
    <span class="co-pgb-sb-words"><b>${label}: <span class=${'co-pgb-num co-pgb-t-' + (tone === 'danger' ? 'danger' : 'ink')}>${value}</span></b>${note && html`<span class="co-pgb-note">${note}</span>`}</span>
    <span class="co-pgb-sb-ctl co-pgb-sb-set">
      <input type="number" class="co-field" step=${step} min=${min} max=${max} value=${typed ?? start} onInput=${(e) => setTyped(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && go()} />
      <${Btn} kind="primary" small label=${button} onClick=${go} />
    </span>
  </div>`;
}

export function SandboxPage() {
  const v = useView('sandbox');
  const st = ui('sandbox', { group: 'money' });
  if (!v) return null;
  if (v.career) {
    return html`<div class="co-sb"><${Section}><p class="co-para">${v.career}</p>
      <div><${Btn} icon="settings" label=${v.settings} onClick=${() => setCo({ dialog: { kind: 'settings' } })} /></div></${Section}></div>`;
  }
  const b = v.balance;
  const r = v.reputation;
  const hand = v.set_by_hand;
  const tabs = [...v.groups, { key: 'hand', icon: 'tune', label: hand.label, sub: hand.sub, info: hand.info, changed: 0 }];
  const g = tabs.find((x) => x.key === st.group) || tabs[0];
  return html`<div class="co-pgb co-pgb-sandbox">
    <div class="co-pgb-stack">
      <div class="co-pgb-head">
        <span class="co-pgb-sb-hic"><${Icon} name="settings" size=${26} /></span>
        <span class="co-pgb-names"><h2 class="co-pgb-h1">${v.title}</h2><span class="co-pgb-sub">${v.sub}</span></span>
        <span class="co-pgb-tools">
          <span class=${'co-pgb-sb-pill' + (v.changed_n > 0 ? ' on' : '')}><i></i>${v.changed}</span>
          <${Btn} icon="restart_alt" label=${v.reset} title=${v.reset_all} disabled=${!v.changed_n} onClick=${() => act('sandbox_reset')} />
        </span>
      </div>
      <${Card} class="co-pgb-sb-presets" icon="bolt" title=${v.presets_caps}>
        <div class="co-pgb-sb-pre">
          ${v.presets.map((p) => html`<button type="button" class=${'co-pgb-sb-preset' + (p.on ? ' on' : '')} title=${p.tip} onClick=${() => p.key !== 'custom' && !p.on && act('sandbox_preset', { key: p.key })}>
            <${Icon} name=${PRESET_ICON[p.key] || 'tune'} size=${22} /><b>${p.label}</b>
          </button>`)}
        </div>
      </${Card}>
      <div class="co-pgb-sb-body">
        <nav class="co-pgb-sb-tabs">
          ${tabs.map((t) => html`<button type="button" class=${'co-pgb-sb-tab' + (t.key === g.key ? ' on' : '')} onClick=${() => setUi('sandbox', { group: t.key })}>
            <span class="co-pgb-sb-tic"><${Icon} name=${t.icon} size=${18} /></span>
            <span class="co-pgb-grow"><b>${t.label}</b><span class="co-pgb-cut">${t.sub}</span></span>
            ${t.changed > 0 && html`<span class="co-pgb-sb-n co-pgb-num">${t.changed}</span>`}
          </button>`)}
        </nav>
        <${Card} class="co-pgb-sb-group" icon=${g.icon} title=${g.label}>
          <p class="co-pgb-note co-pgb-sb-info">${g.info}</p>
          ${g.key === 'hand'
            ? html`<${SetRow} icon="payments" label=${b.label} value=${b.value} tone=${b.tone} start=${b.euros} step=${1000} button=${b.set} note=${b.note + (b.value !== b.booked ? ' ' + b.booked : '')} onSet=${(x) => act('sandbox_balance', { euros: x })} />
              <${SetRow} icon="star" label=${r.label} value=${r.value} start=${r.points} step=${1} min=${0} max=${100} button=${r.set} onSet=${(x) => act('sandbox_reputation', { value: x })} />`
            : g.knobs.map((k) => (k.switch ? html`<${SwitchKnob} k=${k} v=${v} />` : html`<${FactorKnob} k=${k} />`))}
        </${Card}>
      </div>
    </div>
  </div>`;
}

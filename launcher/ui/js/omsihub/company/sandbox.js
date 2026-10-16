// The Sandbox page (Beta 61; redesigned 2026-10-16): a sandbox company's rules, set by the
// player. The head says how many settings differ from a career's, with the way back to it; a
// preset bar sets them all at once (Relaxed, Career, Hard - Custom when none fits); the groups
// (money, unlocks, the economy, the operations, the staff, and the balance and reputation set by
// hand) are tabs down the left, the chosen one's settings a form on the right - each row its
// name, what it does in a line, a slider with its ticks and value or a switch, its career value,
// and an orange dot when it differs. The view and its words come from `omsi-launcher-api`'s
// `company/sandbox.rs` (`co_view` "sandbox"), translated; the badge by a sandbox's name too.

import { html, act, useView, useState, ui, setUi, setCo, Icon, Btn, Seg } from './kit.js';
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

/** A row: the changed dot, the name and what it does, the career's value; its control under. */
function Row({ k, children }) {
  return html`<div class=${'co-pgb-sb-row' + (k.changed ? ' changed' : '')}>
    <div class="co-pgb-sb-row-h">
      <i class="co-pgb-sb-dot"></i>
      <span class="co-pgb-grow"><b>${k.label}</b><span class="co-pgb-note">${k.info}</span></span>
      <span class="co-pgb-sb-career">${k.career}</span>
    </div>
    ${children}
  </div>`;
}

/** A switch's row. */
function SwitchKnob({ k }) {
  return html`<${Row} k=${k}>
    <label class="co-pgb-sb-toggle"><input type="checkbox" checked=${!!k.on} onChange=${(e) => act('sandbox_knob', { key: k.key, on: e.currentTarget.checked })} /><i></i></label>
  </${Row}>`;
}

/** A factor's row: its slider with a tick at every whole factor and the career's marked, its
 * value in a chip, back to the career's. Sent when let go. */
function FactorKnob({ k }) {
  const [drag, setDrag] = useState(null);
  const send = (x) => {
    setDrag(null);
    act('sandbox_knob', { key: k.key, value: x });
  };
  const v = drag ?? k.value;
  const span = k.most - k.least || 1;
  const at = (x) => ((x - k.least) / span) * 100;
  const ticks = [];
  for (let x = Math.ceil(k.least); x <= k.most + 1e-9; x += span > 5 ? 2 : 1) ticks.push(x);
  return html`<${Row} k=${k}>
    <div class="co-pgb-sb-slide">
      <span class="co-pgb-sb-track">
        <input type="range" min=${k.least} max=${k.most} step=${0.05} value=${v} style=${`--fill:${at(v)}%`}
          onInput=${(e) => setDrag(Number(e.currentTarget.value))} onChange=${(e) => send(Number(e.currentTarget.value))} />
        <span class="co-pgb-sb-ticks">${ticks.map((x) => html`<i class=${Math.abs(x - (k.career_at ?? 1)) < 1e-9 ? 'career' : ''} style=${`left:${at(x)}%`}></i>`)}</span>
      </span>
      <span class=${'co-pgb-sb-val' + (k.changed ? ' changed' : '')}>${drag != null ? times(drag) : k.text}</span>
      <button type="button" class="co-pgb-round" title=${k.reset || ''} disabled=${!k.reset} onClick=${() => send(k.career_at ?? 1)}><${Icon} name="restart_alt" size=${15} /></button>
    </div>
  </${Row}>`;
}

/** A number typed in and set with its button (the balance, the reputation). */
function SetRow({ label, value, tone, start, step, min, max, button, onSet, note }) {
  const [typed, setTyped] = useState(null);
  const go = async () => {
    const x = Number(typed ?? start);
    if (!Number.isFinite(x)) return;
    await onSet(x);
    setTyped(null);
  };
  return html`<div class="co-pgb-sb-row">
    <div class="co-pgb-sb-row-h"><i class="co-pgb-sb-dot"></i><span class="co-pgb-grow"><b>${label}</b>${note && html`<span class="co-pgb-note">${note}</span>`}</span>
      <b class=${'co-pgb-sb-big' + (tone === 'danger' ? ' co-pgb-t-danger' : '')}>${value}</b></div>
    <div class="co-pgb-sb-set">
      <input type="number" class="co-field" step=${step} min=${min} max=${max} value=${typed ?? start} onInput=${(e) => setTyped(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && go()} />
      <${Btn} kind="primary" label=${button} onClick=${go} />
    </div>
  </div>`;
}

export function SandboxPage() {
  const v = useView('sandbox');
  const st = ui('sandbox', { group: 0 });
  if (!v) return html`<div class="co-pgb"></div>`;
  if (v.career) {
    return html`<div class="co-pgb co-pgb-sb"><${Card} icon="sports_esports" title=${v.settings}><p class="co-pgb-text">${v.career}</p>
      <div style="margin-top:12px"><${Btn} icon="settings" label=${v.settings} onClick=${() => setCo({ dialog: { kind: 'settings' } })} /></div></${Card}></div>`;
  }
  const b = v.balance;
  const r = v.reputation;
  const tabs = [...v.groups.map((g) => ({ icon: g.icon, label: g.label, changed: g.changed })), { icon: 'tune', label: v.set_label, changed: 0 }];
  const sel = Math.min(st.group, tabs.length - 1);
  const g = v.groups[sel];
  return html`<div class="co-pgb co-pgb-sb">
    <div class="co-pgb-stack">
      <div class="co-pgb-head">
        <span class="co-pgb-names"><h2 class="co-pgb-h1">${v.title}</h2><span class="co-pgb-sub">${v.intro}</span></span>
        <span class="co-pgb-tools">
          <span class=${'co-pgb-chip' + (v.changed_n > 0 ? ' accent' : '')}>${v.changed}</span>
          <${Btn} icon="restart_alt" label=${v.reset_career} disabled=${!v.reset_all} title=${v.reset_all || ''} onClick=${() => act('sandbox_reset')} />
        </span>
      </div>
      <div class="co-pgb-card co-pgb-sb-presets" title=${v.preset.tip}>
        <span class="co-pgb-caps">${v.preset.label}</span>
        <${Seg} value=${v.preset.value} options=${v.preset.options} off=${['custom']} onChange=${(key) => key !== 'custom' && act('sandbox_preset', { key })} />
      </div>
      <div class="co-pgb-sb-body">
        <nav class="co-pgb-card co-pgb-sb-tabs">${tabs.map((t, k) => html`<button type="button" class=${'co-pgb-sb-tab' + (k === sel ? ' on' : '')} onClick=${() => setUi('sandbox', { group: k })}>
          <${Icon} name=${t.icon} size=${17} /><span class="co-pgb-grow co-pgb-cut">${t.label}</span>${t.changed > 0 && html`<i class="co-pgb-sb-count">${t.changed}</i>`}</button>`)}</nav>
        <section class="co-pgb-card co-pgb-sb-form">
          <div class="co-pgb-card-h"><span class="co-pgb-ic"><${Icon} name=${tabs[sel].icon} size=${17} /></span><h3>${tabs[sel].label}</h3></div>
          ${g
            ? g.knobs.map((k) => (k.switch ? html`<${SwitchKnob} k=${k} />` : html`<${FactorKnob} k=${k} />`))
            : html`<${SetRow} label=${b.label} value=${b.value} tone=${b.tone} start=${b.euros} step=${1000} button=${b.set} note=${`${b.note} ${b.value !== b.booked ? b.booked : ''}`} onSet=${(x) => act('sandbox_balance', { euros: x })} />
                <${SetRow} label=${r.label} value=${r.value} start=${r.points} step=${1} min=${0} max=${100} button=${r.set} onSet=${(x) => act('sandbox_reputation', { value: x })} />`}
        </section>
      </div>
    </div>
  </div>`;
}

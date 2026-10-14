// The Controls page's game controllers (Trello "Tauri 6/6"), as our game-drawn launcher has them
// (`crates/omsi-launcher-ui/src/pages/controls.rs`, `game_controllers`): on the left the devices
// set up (lit when connected, "off" when left out) with a way to set up each one connected and
// not yet set up, the dead zone and Save; on the right the device chosen - what each axis does,
// which way it runs and its characteristic, beside the axis as it stands; each button's action
// (a button pressed on the device lights its line), latching switches, the force feedback's
// strengths; the step-by-step set-up over it. The devices are read by the game program
// (`hub_pads`, `hub_service::pads`), which the page asks some ten times a second.

import { html, useEffect, useRef, useState } from '../../vendor/preact-htm.js';
import { S, set, useStore } from '../store.js';
import { call, onPhone } from '../api.js';
import { saveSettings } from '../data.js';
import { Card, Heading, Row, Drop, Toggle, Btn, Icon, status, tr } from './editors/kit.js';
import { Slider } from './bits.js';

/** Our page in place of upstream's controllers test, on a computer. */
export const HUB_PADS = !onPhone && !new URLSearchParams(location.search).has('phone');

const pct = (v) => `${Math.round(v * 100)} %`;
const offsOf = (s) => String(s?.ctrl_off || '').split('|').filter(Boolean);

let deadzoneTimer = 0;
/** The dead zone (a setting of the game's): shown at once, saved once the hand stops. */
function deadzone(x) {
  const v = Math.round(x * 100) / 100;
  set({ settings: { ...S.settings, ctrl_deadzone: v } });
  clearTimeout(deadzoneTimer);
  deadzoneTimer = setTimeout(() => saveSettings({ ctrl_deadzone: v }), 300);
}

export function HubPads() {
  const s = useStore();
  const [v, setV] = useState(null);
  const [h, setH] = useState(0);
  const ref = useRef();
  const busy = useRef(false);
  const ask = (args, quiet) => {
    if (busy.current && quiet) return Promise.resolve();
    busy.current = true;
    return call('hub_pads', args)
      .then((x) => {
        if (x?.say) status(x.say[0], x.say[1]);
        setV(x);
      })
      .catch((e) => !quiet && status(String(e.message || e), true))
      .finally(() => (busy.current = false));
  };
  const act = (what, more = {}) => ask({ op: 'act', act: what, ...more });
  useEffect(() => {
    ask({ op: 'open' });
    const t = setInterval(() => ask({ op: 'view' }, true), 100);
    // (the sheet's head says what this tab is for, `pages::about`)
    set({ hubPads: true });
    return () => (clearInterval(t), set({ hubPads: false }));
  }, []);
  // (as high as the sheet leaves under the tabs: the lists scroll inside their cards)
  useEffect(() => {
    const fit = () => {
      const el = ref.current;
      const body = el?.closest('.hub-page-body');
      if (!el || !body) return;
      const k = body.getBoundingClientRect().height / body.clientHeight || 1;
      const top = (el.getBoundingClientRect().top - body.getBoundingClientRect().top) / k + body.scrollTop;
      setH(Math.max(420, body.clientHeight - top - 24));
    };
    fit();
    addEventListener('resize', fit);
    return () => removeEventListener('resize', fit);
  }, [v != null]);
  if (!v) return html`<div class="hub-pads" ref=${ref}></div>`;
  const offs = offsOf(s.settings);
  const d = v.device;
  const held = v.held_gears && !s.settings?.momentary_gears;
  return html`<div class="hub-pads-wrap" ref=${ref}>
    ${held && html`<p class="hub-pads-note">${tr('H-pattern gears are assigned. Enable return to neutral under Settings → Driving → Game controllers if your shifter has no neutral button.')}</p>`}
    <div class="hub-pads" style=${h ? `height:${h - (held ? 40 : 0)}px` : ''}>
      <${Card} class="hub-pads-list">
        <${Heading} title="Devices" />
        <div class="rows">
          ${v.devices.map(
            (x, i) => html`<${Row} on=${v.sel === i} onClick=${() => act('select', { n: i })}>
              <span class=${'name' + (x.on ? '' : ' away')}>${x.name}</span>
              ${offs.some((o) => o.toLowerCase() === x.name.toLowerCase())
                ? html`<span class="off">${tr('off')}</span>`
                : html`<${Icon} name=${x.on ? 'check_circle' : 'remove'} size=${16} class=${x.on ? 'ok' : 'faint'} />`}
            </${Row}>`,
          )}
          ${v.add.map((n) => html`<${Btn} kind="primary" icon="add" raw label=${v.add_label.replace('%{device}', n)} onClick=${() => act('add', { name: n })} />`)}
          ${!v.devices.length && !v.add.length && html`<p class="hub-pads-para">${tr('No game controller is connected, and none is set up. Connect a wheel, pedals or a joystick; a gamepad works without setting up (left stick steers, the triggers are the pedals).')}</p>`}
        </div>
        <div class="foot">
          <${Slider} label="Dead zone" value=${Number(s.settings?.ctrl_deadzone ?? 0)} min=${0} max=${0.3} step=${0.01} format=${pct} onChange=${deadzone} />
          <${Btn} kind=${v.dirty ? 'primary' : ''} icon="save" label=${v.dirty ? 'Save' : 'Saved'} onClick=${() => v.dirty && act('save')} />
        </div>
      </${Card}>
      <${Card} class="hub-pads-device">
        ${d || v.wizard ? html`<${Device} v=${v} d=${d} act=${act} offs=${offs} />` : null}
      </${Card}>
    </div>
  </div>`;
}

/** The device chosen: its head (on or off, the set-up), its axes and buttons - or the set-up. */
function Device({ v, d, act, offs }) {
  const name = d?.name || v.devices[v.sel]?.name || '';
  const off = offs.some((o) => o.toLowerCase() === name.toLowerCase());
  const use = (on) => saveSettings({ ctrl_off: [...offs.filter((o) => o.toLowerCase() !== name.toLowerCase()), ...(on ? [] : [name])].join('|') });
  return html`
    <div class="head">
      <${Heading} raw title=${name} />
      ${!v.wizard && html`<${Toggle} class="use" label="Use this device" checked=${!off} onChange=${use} />
        ${d.wizard && html`<${Btn} icon="touch_app" label="Set up step by step" onClick=${() => act('wizard')} />`}`}
    </div>
    ${v.wizard ? html`<${Wizard} w=${v.wizard} live=${v.live} act=${act} />` : html`<${Detail} v=${v} d=${d} act=${act} />
      <div class="foot">
        <${Btn} icon="add" label=${d.capturing ? 'Press a button on the device...' : 'Add a button'} onClick=${() => act('capture')} />
        <span class="grow"></span>
        <${Btn} kind="danger" icon="delete" label=${d.armed ? 'Click again to remove' : 'Remove this device'} onClick=${() => act('remove')} />
      </div>`}`;
}

/** The force feedback's strengths, the axes and the buttons (`game_controllers`' list). */
function Detail({ v, d, act }) {
  const funcs = v.funcs.map((f, i) => [i, f]);
  const shapes = v.shapes.map((f, i) => [i, f]);
  const lit = useRef();
  useEffect(() => lit.current?.scrollIntoView({ block: 'nearest' }), [d.buttons.findIndex((b) => b.lit)]);
  return html`<div class="detail">
    ${d.ff && html`<div class="ff">
      <${Slider} label="Steering force" value=${d.ff.steering} min=${0} max=${2} step=${0.05} format=${pct} onChange=${(x) => act('ff_scale', { steering: x })} />
      <${Slider} label="Vibration" value=${d.ff.vibration} min=${0} max=${2} step=${0.05} format=${pct} onChange=${(x) => act('ff_scale', { vibration: x })} />
      ${d.ff.invert != null && html`<${Toggle} label="Invert force feedback" checked=${d.ff.invert} onChange=${(on) => act('ff_invert', { on })} />`}
    </div>`}
    ${d.axes.map(
      (a, k) => html`<div class="axis">
        <span class="name">${a.name}</span>
        <span class="bar">${a.live != null && html`<i style=${`left:calc(${((Math.max(-1, Math.min(1, a.live)) + 1) / 2) * 100}% - 2px)`}></i>`}</span>
        <${Drop} raw value=${a.func} options=${funcs} onChange=${(f) => act('axis', { a: k, func: f })} />
        <span>${a.func > 0 && html`<${Toggle} label="Reversed" checked=${a.inv} onChange=${(on) => act('inv', { a: k, on })} />`}</span>
        <span>${a.func > 0 && html`<${Drop} raw value=${a.shape} options=${shapes} onChange=${(x) => act('shape', { a: k, k: x })} />`}</span>
      </div>`,
    )}
    <h3>${tr('Buttons')}</h3>
    <div class="buttons">
      ${d.buttons.map(
        (b, n) => html`<div class=${'button' + (b.lit ? ' lit' : '')} ref=${b.lit ? lit : undefined}>
          <span class="name">${b.name}</span>
          <${Drop} raw value=${b.action} options=${v.actions} onChange=${(a) => act('button', { b: n, action: a })} />
          <${Toggle} label="Latching" checked=${b.latching} onChange=${(on) => act('latch', { b: n, on })} />
        </div>`,
      )}
    </div>
  </div>`;
}

const AXIS_SHORT = ['X', 'Y', 'Z', 'Rx', 'Ry', 'Rz', 'Slider 1', 'Slider 2'];

/** The set-up (`wizard`): what to do now, the axes as they stand, Cancel, Skip and Next; for a
 * wheel with force feedback its direction last. */
function Wizard({ w, live, act }) {
  if (w.ff) {
    return html`<div class="wizard">
      <h2>${w.title}</h2>
      <${Toggle} label="Invert force feedback" checked=${w.invert} onChange=${(on) => act('wizard_invert', { on })} />
      <span class="grow"></span>
      <div class="foot">
        <${Btn} kind="ghost" label="Cancel" onClick=${() => act('wizard_cancel')} />
        <span class="grow"></span>
        <${Btn} kind="primary" icon="check" label="Finish" onClick=${() => act('wizard_next')} />
      </div>
    </div>`;
  }
  return html`<div class="wizard">
    <h2>${w.title}</h2>
    <p>${w.text}</p>
    ${w.absent && html`<p class="bad">${w.absent}</p>`}
    ${w.error && html`<p class="bad">${w.error}</p>`}
    ${live.map(
      ([k, x]) => html`<div class="axis-live"><span>${AXIS_SHORT[k] || ''}</span>
        <span class="bar"><i style=${`left:calc(${((Math.max(-1, Math.min(1, x)) + 1) / 2) * 100}% - 2px)`}></i></span></div>`,
    )}
    <span class="grow"></span>
    <div class="foot">
      <${Btn} kind="ghost" label="Cancel" onClick=${() => act('wizard_cancel')} />
      <span class="grow"></span>
      ${w.skip && html`<${Btn} label="Skip" onClick=${() => act('wizard_skip')} />`}
      <${Btn} kind="primary" icon="chevron_right" raw label=${w.next} onClick=${() => act('wizard_next')} />
    </div>
  </div>`;
}

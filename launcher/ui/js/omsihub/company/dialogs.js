// What lies over the company's pages, as our game-drawn launcher draws it last
// (`company::overlays`): the cover while the company's time is simulated, what happened while
// the player was away, the days' report, a decision the clock waits for, the dialog open (the
// company's settings, the clock's steps, a confirmation, a bus, a line, a fare, the dealer's
// sheet) - and the popup over all of them.

import { S } from '../../store.js';
import { call } from '../../api.js';
import { html, tr, setCo, act, cmd, refresh, useView, goTab, goKey, handOver, Icon, Btn, Seg, Switch, Select, Dialog, Popup, Figure, Mark, Plate, Slider, T } from './kit.js';
import { VehicleDialog, NewDialog } from './fleet.js';
import { DealerSheet } from './dealer.js';
import { AddLineDialog } from './lines.js';
import { FareDialog, FleetValueDialog } from './money.js';
// (the buses the company works with: Beta 54)
import { VehiclesDialog } from './vehicles.js';
// (Beta 58: the boss's "Play together" panel)
import { TogetherDialog } from './together.js';
import { togetherOn } from './gate.js';
// (late running: the report's part, the finances' deductions)
import { LateSection, LateDialog, WhoLink } from './late.js';

/** Something lies over the pages: the clock holds what waits for it (`company::modal`). */
export function modalOpen() {
  const c = S.co;
  const sh = c.shell;
  return !!(c.dialog || c.popup || sh?.cover || sh?.reports || sh?.asking || sh?.away || sh?.sheet);
}

export function Overlays({ founding }) {
  const c = S.co;
  const sh = c.shell;
  let under = null;
  if (!founding && sh?.open) {
    if (sh.cover) under = html`<div class="co-cover"><div><b>${sh.cover}</b><div class="bar"><i></i></div></div></div>`;
    else if (sh.away) under = html`<${AwayDialog} />`;
    else if (sh.reports) under = html`<${ReportDialog} />`;
    else if (sh.asking) under = html`<${AskDialog} />`;
    else if (c.dialog) under = html`<${OpenDialog} d=${c.dialog} />`;
    else if (sh.sheet) under = html`<${DealerSheet} />`;
  }
  return html`${under}${c.popup && html`<${Popup} p=${c.popup} onClose=${() => setCo({ popup: null })} />`}`;
}

function OpenDialog({ d }) {
  switch (d.kind) {
    case 'settings':
      return html`<${SettingsDialog} d=${d} />`;
    case 'time':
      return html`<${TimeDialog} />`;
    case 'confirm':
      return html`<${ConfirmDialog} d=${d} />`;
    case 'vehicle':
      return html`<${VehicleDialog} d=${d} />`;
    case 'new':
      return html`<${NewDialog} d=${d} />`;
    case 'add_line':
      return html`<${AddLineDialog} d=${d} />`;
    case 'fare':
      return html`<${FareDialog} d=${d} />`;
    case 'fleet_value':
      return html`<${FleetValueDialog} />`;
    case 'vehicles':
      return html`<${VehiclesDialog} />`;
    case 'together':
      return togetherOn() ? html`<${TogetherDialog} />` : null;
    case 'late':
      return html`<${LateDialog} />`;
    default:
      return null;
  }
}

const close = () => setCo({ dialog: null });

// --- the company's settings -------------------------------------------------------------------

/** The company's settings (`settings::dialog`): its mark, its game mode (and add-ons), how it
 * buys, its date and the real clock; deleting it. */
function SettingsDialog({ d }) {
  const v = useView('settings', { date: d.date || null });
  if (!v) return null;
  const k = v.modes.findIndex((m) => m.key === v.mode);
  const pick = async () => {
    const p = await call('pick_file', { title: v.choose_logo }).catch(() => null);
    if (p) act('logo', { path: p });
  };
  const moveTo = async () => {
    const r = await act('move_date', { date: v.date });
    if (r?.moved) close();
  };
  const foot = html`<${Btn} kind="danger" icon="delete" label=${v.delete} onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'delete_company' } })} />
    <span class="rest"></span>
    <${Btn} kind=${v.move ? '' : 'primary'} label=${v.close} onClick=${close} />
    ${v.move && html`<${Btn} kind="primary" icon="calendar_month" label=${v.move} onClick=${moveTo} />`}`;
  const h = (v.buying ? 786 : 722) + (v.addons ? 40 : 0) + (v.vehicles ? 132 : 0) + (v.company_mode ? 112 : 0) + (v.currency ? 84 : 0);
  // (Beta 61: a career or a sandbox - turned into a sandbox once, after a confirmation)
  const cm = v.company_mode;
  const vh = v.vehicles;
  const vmode = (m) => cmd('co_vehicles_act', { what: 'mode', mode: m, scope: 'company' }).then(() => refresh());
  return html`<${Dialog} icon="settings" title=${v.title} w=${760} h=${h} onClose=${close} foot=${foot}>
    <div class="co-settings">
      <div class="co-caps">${v.mark_caps}</div>
      <div class="mark"><${Mark} mark=${v.mark} size=${68} />
        <div class="what"><p class="cut">${v.logo_text}</p>
          <div style="display:flex;gap:10px;margin-top:8px"><${Btn} icon="photo_camera" label=${v.choose_logo} onClick=${pick} />
            ${v.remove_logo && html`<${Btn} icon="close" label=${v.remove_logo} onClick=${() => act('logo_remove')} />`}</div></div></div>
      <div class="co-caps">${v.mode_caps}</div>
      <div class="pair"><${Seg} value=${k} options=${v.modes.map((m, i) => [i, m.label])} onChange=${(i) => setCo({ dialog: { kind: 'confirm', what: 'game_mode', name: v.modes[i].key, settings: v.date } })} style="width:320px;height:40px" />
        <p class="co-note">${v.mode_info}</p></div>
      ${cm && html`<div class="co-caps">${cm.caps}</div>
        <div class="addon co-company-mode"><${Icon} name=${cm.icon} size=${18} class="t-accent2" />
          <div style="flex:1;min-width:0;max-width:430px"><b>${cm.label}</b>
            <p class="co-note">${cm.info}</p>
            ${cm.note && html`<p class="co-note">${cm.note}</p>`}</div>
          ${cm.switch
            ? html`<${Btn} icon="sports_esports" label=${cm.switch} onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'sandbox', settings: v.date } })} />`
            : html`<${Btn} kind="primary" icon="sports_esports" label=${cm.open} onClick=${() => goTab(cm.tab)} />`}</div>`}
      ${v.addons && html`<div class="co-caps">${v.addons.caps}</div>
        <div class="addon"><${Icon} name="extension" size=${18} class="t-accent2" />
          <div style="flex:1;min-width:0;max-width:530px"><${Switch} checked=${v.addons.on} label=${v.addons.label} onChange=${(on) => act('rosterflow', { on }).then((r) => r?.rosterflow && (close(), handOver('5', { switched: true })))} />
            <p class="co-note">${v.addons.note}</p></div></div>`}
      ${v.buying && html`<div class="co-caps">${v.buying.caps}</div>
        <div class="pair"><${Seg} value=${v.buying.mode} options=${v.buying.modes.map((m, i) => [i, m])} onChange=${(i) => act('buying_mode', { mode: i })} style="width:320px;height:40px" />
          <p class="co-note">${v.buying.say}</p></div>`}
      ${v.currency && html`<div class="co-caps">${v.currency.caps}</div>
        <div class="pair"><${Select} value=${v.currency.value} options=${v.currency.options} onChange=${(i) => act('currency', { code: v.currency.codes[i] })} style="width:320px;flex:none" />
          <p class="co-note">${v.currency.say}</p></div>`}
      ${vh && html`<div class="co-caps">${vh.caps}</div>
        <div class="pair"><${Seg} value=${vh.mode} options=${vh.modes} onChange=${vmode} style="width:560px;max-width:100%;height:40px" />
          <span class="co-note">${vh.count}</span></div>
        <div style="display:flex;align-items:center;gap:14px;margin:-6px 0 18px"><${Btn} icon="directions_bus" label=${vh.choose} onClick=${() => setCo({ dialog: { kind: 'vehicles' } })} /><p class="co-note" style="flex:1;min-width:0">${vh.say}</p></div>`}
      <div class="co-caps">${v.date_caps}</div>
      <div class="co-para" style="color:var(--hub-text);font-weight:500;line-height:24px;margin-bottom:10px">${v.now}</div>
      <${Switch} checked=${v.realtime.on} label=${v.realtime.label} onChange=${(on) => act('realtime', { on })} style="min-height:36px" />
      <p class="co-note" style="margin:4px 0 14px">${v.realtime.say}</p>
      <div style="display:flex;align-items:center;gap:16px"><input type="date" class="co-field" style="width:240px" value=${v.date} onChange=${(e) => e.currentTarget.value && setCo({ dialog: { ...d, date: e.currentTarget.value } })} />
        ${v.earliest && html`<span class="co-note">${v.earliest}</span>`}</div>
      <div class="co-iconline" style="margin-top:16px;font-size:var(--co-body);font-weight:400;color:var(--hub-text-soft)"><${Icon} name="warning" size=${20} class="t-warn" /><span>${v.warn}</span></div>
    </div>
  </${Dialog}>`;
}

// --- the clock's steps -----------------------------------------------------------------------

/** The clock's dialog (`clock::time_dialog`): the steps, until a time, the speeds, the
 * dispatcher, and closing the day quickly. */
function TimeDialog() {
  const v = useView('time');
  if (!v) return null;
  const step = (s) => {
    close();
    act('step', { step: s.step, n: s.n });
  };
  const foot = html`<span title=${v.quick_tip}><${Btn} icon="check_circle" label=${v.quick} onClick=${() => (close(), act('quick_close'))} /></span>
    <span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`;
  return html`<${Dialog} icon="schedule" title=${v.title} w=${780} h=${v.in_game ? 680 : 620} onClose=${close} foot=${foot}>
    ${v.in_game && html`<p class="co-para t-warn" style="margin-bottom:12px">${v.in_game}</p>`}
    <div class="co-caps" style="margin-bottom:10px">${v.simulate}</div>
    <div class="co-steps">${v.steps.map((s) => html`<${Btn} label=${s.label} title=${s.tip} style="height:38px;padding:0 6px" onClick=${() => step(s)} />`)}</div>
    <div style="display:flex;gap:14px;align-items:center;margin:10px 0 24px">
      <${Slider} label=${v.until_label} value=${v.until} min=${0} max=${1425} step=${15} format=${(m) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`} onChange=${(m) => act('until', { minute: m })} style="flex:1;height:40px" />
      <${Btn} icon="schedule" label=${v.until_button} style="height:40px" onClick=${() => (close(), act('step', { step: 'until', minute: v.until }))} />
    </div>
    <div class="co-caps" style="margin-bottom:10px">${v.run}</div>
    <div style="display:flex;gap:8px;align-items:flex-start">
      ${v.speeds.map(([label, speed], k) => html`<${Btn} kind=${Math.abs(v.speed - speed) < 1e-6 ? 'primary' : ''} label=${label} title=${k === 0 ? v.speed_tip : undefined} style="width:110px;height:38px;padding:0" onClick=${() => act('play', { speed })} />`)}
      <p class="co-note" style="margin-left:10px;flex:1">${v.say}</p>
    </div>
    <div class="co-iconline" style="margin:12px 0 16px;font-weight:400;color:var(--hub-text-soft);font-size:var(--co-note)"><${Icon} name=${v.real ? 'schedule' : 'pause_circle'} size=${17} class=${v.real ? 't-ok' : 't-dim'} /><span>${v.real_words}</span></div>
    <div class="co-caps" style="margin-bottom:10px">${v.decisions}</div>
    <${Switch} checked=${v.dispatcher} label=${v.dispatcher_label} onChange=${(on) => act('dispatcher', { on })} style="min-height:36px" />
  </${Dialog}>`;
}

// --- a confirmation --------------------------------------------------------------------------

/** A confirmation (`company::confirm_dialog`): selling a bus, dismissing someone, a line no
 * more, deleting the company, another game mode. */
function ConfirmDialog({ d }) {
  const v = useView('confirm', { what: d.what, id: d.id || 0, name: d.name || '' });
  if (!v) return null;
  if (v.gone) {
    close();
    return null;
  }
  const back = () => setCo({ dialog: d.settings ? { kind: 'settings', date: d.settings } : null });
  const yes = async () => {
    setCo({ dialog: null });
    if (d.what === 'delete_company') {
      const id = d.name || S.co.shell?.id;
      await cmd('co_delete', { id });
      // (the company open is gone: its start, the companies)
      if (!d.name || d.name === S.co.shell?.id) setCo({ chooser: 'companies' });
      refresh();
      return;
    }
    const r = await act('confirmed', { what: d.what, id: d.id || 0, name: d.name || '' });
    if (d.what === 'game_mode' && r && !r.popup) {
      if (d.tab != null) goTab(d.tab);
      else if (d.settings) setCo({ dialog: { kind: 'settings', date: d.settings } });
    }
  };
  const foot = html`<span class="rest"></span><${Btn} label=${v.cancel} onClick=${back} /><${Btn} kind=${v.danger ? 'danger' : 'primary'} label=${v.button} onClick=${yes} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${560} onClose=${back} foot=${foot}><p class="co-para">${v.text}</p></${Dialog}>`;
}

// --- the days' report -------------------------------------------------------------------------

/** The report of the days just closed (`company::report_dialog`): four figures, the money by
 * kind, the lines (a click opens one's money), and what happened with the ways to mend it. */
function ReportDialog() {
  const v = useView('report');
  if (!v) return null;
  const done = () => act('close_reports');
  const foot = html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${done} />`;
  return html`<${Dialog} icon="receipt_long" title=${v.title} w=${1000} h=${860} onClose=${done} foot=${foot} bodyStyle="display:flex;flex-direction:column">
    <div class="co-figures4">${v.figures.map((f) => html`<div title=${f.tip || undefined}><${Figure} f=${f} /></div>`)}</div>
    <div class="co-scroll" style="flex:1;min-height:0;margin-top:18px">
      ${v.money.length > 0 && html`<div class="co-caps" style="margin-bottom:8px">${v.money_caps}</div>
        <div class="co-money2">${v.money.map(([label, t, tone]) => html`<div><span class="cut">${label}</span><b class=${'t-' + tone}>${t}</b></div>`)}</div>`}
      ${v.lines.length > 0 && html`<div class="co-caps" style="display:flex;margin:18px 0 8px"><span class="grow">${v.lines_caps}</span><span style="text-transform:none;font-weight:400;font-size:var(--co-note)">${v.lines_hint}</span></div>
        ${v.lines.map((x) => html`<button type="button" class="co-row co-reportline" onClick=${() => act('report_line', { line: x.line })}>
            <${Plate} p=${x.plate} h=${24} style="margin-left:8px" /><span class="cut t-soft" style="flex:1">${x.text}</span><b class=${'t-' + x.tone}>${x.net}</b><${Icon} name=${x.open ? 'expand_less' : 'expand_more'} size=${18} class="t-dim" /></button>
          ${x.open && html`<div class="co-reportmoney">${x.rows.map(([label, t, tone]) => html`<div><span>${label}</span><b class=${'t-' + tone}>${t}</b></div>`)}<p>${x.note}</p></div>`}`)}`}
      ${v.drops?.length > 0 && html`<div class="co-caps" style="display:flex;margin:18px 0 8px"><span class="grow">${v.drops_caps}</span><span style="text-transform:none;font-weight:400;font-size:var(--co-note)">${v.drops_hint}</span></div>
        ${v.drops.map((d) => html`<button type="button" class="co-row co-reportdrop" title=${d.date} onClick=${() => toPlanning(d.link)}>
            <i class=${'dot-' + d.tone}></i><span class="cut t-soft" style="flex:1">${d.text}</span><b class=${'cut t-' + d.tone}>${d.why}</b><${Icon} name="chevron_right" size=${18} class="t-dim" /></button>
          ${(d.bus || d.driver) && html`<div class="late-who">${d.duty && html`<span class="t-dim">${d.duty}</span>`}${d.bus && html`<span><${Icon} name="directions_bus" size=${13} /> <${WhoLink} w=${d.bus} bus before=${done} /></span>`}${d.driver && html`<span><${Icon} name="person" size=${13} /> <${WhoLink} w=${d.driver} before=${done} /></span>`}</div>`}`)}
        ${v.drops_more && html`<p class="co-reportmore">${v.drops_more}</p>`}`}
      <${LateSection} v=${v.late} before=${done} />
      ${v.facts.length > 0 && html`<div class="co-caps" style="margin:18px 0 10px">${v.facts_caps}</div>
        ${v.facts.map((f) => html`<div class="co-fact"><i class=${'dot-' + f.tone}></i><div><p>${f.text}</p>
          ${f.fixes.length > 0 && html`<div class="fixes">${f.fixes.map((g) => html`<${Btn} icon=${g.icon} label=${g.label} style="height:34px" onClick=${() => (act('close_reports'), goKey(g.to))} />`)}</div>`}</div></div>`)}`}
    </div>
  </${Dialog}>`;
}

/** A trip dropped, in the planning (#96): RosterFlow on its weekday with its tour, duty or block
 * shown - the Planning page where RosterFlow is not on. */
function toPlanning(link) {
  act('close_reports');
  import('../rosterflow/index.js')
    .then((m) => m.openRosterFlow(link || {}))
    .then((ok) => ok || goKey('planning'))
    .catch(() => goKey('planning'));
}

// --- while the player was away ---------------------------------------------------------------

function AwayDialog() {
  const v = useView('away');
  if (!v) return null;
  const foot = html`${v.report && html`<${Btn} icon="receipt_long" label=${v.report} onClick=${() => act('close_away', { report: true })} />`}
    <span class="rest"></span><${Btn} kind="primary" label=${v.go} onClick=${() => act('close_away', { report: false })} />`;
  return html`<${Dialog} icon="history" title=${v.title} w=${860} h=${700} foot=${foot} bodyStyle="display:flex;flex-direction:column">
    <p class="co-para" style="color:var(--hub-text)">${v.ran}</p>
    ${v.skipped && html`<div class="co-iconline t-warn" style="font-size:var(--co-body);font-weight:400;margin-top:10px"><${Icon} name="warning" size=${18} /><span>${v.skipped}</span></div>`}
    <div class="co-figures3">${v.figures.map((f) => html`<${Figure} f=${f} />`)}</div>
    <div class="co-caps" style="margin:18px 0 10px">${v.caps}</div>
    <div class="co-scroll" style="flex:1;min-height:0">
      ${v.notable.length
        ? v.notable.map((n) => html`<div class="co-awayrow"><b>${n.when}</b><i class=${'dot-' + n.tone}></i><span>${n.text}</span></div>`)
        : html`<p class="co-para">${v.none}</p>`}
    </div>
  </${Dialog}>`;
}

// --- a decision the clock waits for --------------------------------------------------------------

/** What the simulation waits for (`clock::ask_dialog`): a breakdown (a spare bus, a rental bus,
 * or the trips dropped), a tender, the morning's disruptions, a notice. */
function AskDialog() {
  const v = useView('ask');
  if (!v || !v.kind) return null;
  if (v.kind === 'gone') {
    act('ask_on', { on: true });
    return null;
  }
  if (v.kind === 'breakdown') {
    const foot = html`<span class="rest"></span><${Btn} label=${v.drop} onClick=${() => act('ask_drop')} />
      <${Btn} kind=${v.spares.length ? '' : 'primary'} icon="directions_bus" label=${v.rental} onClick=${() => act('ask_rental')} />`;
    return html`<${Dialog} icon=${v.icon} title=${v.title} w=${640} foot=${foot}>
      <p class="co-para">${v.text}</p>
      ${v.spares_caps && html`<div class="co-caps" style="margin:20px 0 8px">${v.spares_caps}</div>
        ${v.spares.map((s) => html`<${Btn} kind="primary" icon="directions_bus" label=${s.label} style="width:100%;height:34px;margin-bottom:8px" onClick=${() => act('ask_spare', { id: s.id })} />`)}`}
      <${Switch} checked=${v.dispatcher} label=${v.dispatcher_label} onChange=${(on) => act('ask_dispatcher', { on })} style="min-height:36px;margin-top:14px" />
    </${Dialog}>`;
  }
  if (v.kind === 'tender') {
    const foot = html`<span class="rest"></span><${Btn} label=${v.on} onClick=${() => act('ask_on', { on: true })} />
      ${v.auction && html`<${Btn} kind="primary" icon="payments" label=${v.auction} onClick=${() => act('ask_on', { on: false }).then(() => goTab(8))} />`}`;
    return html`<${Dialog} icon=${v.icon} title=${v.title} w=${640} onClose=${() => act('ask_on', { on: true })} foot=${foot}><p class="co-para">${v.text}</p></${Dialog}>`;
  }
  if (v.kind === 'morning') {
    const foot = html`<span class="rest"></span><${Btn} label=${v.on} onClick=${() => act('ask_on', { on: true })} />
      ${v.planning && html`<${Btn} kind="primary" icon="event" label=${v.planning} onClick=${() => act('ask_on', { on: false }).then(() => goTab(5))} />`}`;
    return html`<${Dialog} icon=${v.icon} title=${v.title} w=${720} onClose=${() => act('ask_on', { on: true })} foot=${foot}>
      <p class="co-para" style="margin-bottom:16px">${v.text}</p>
      ${v.rows.map((r) => html`<div class="co-morning"><b class="cut">${r.title}</b>
        <${Select} value=${r.at} h=${30} options=${r.options.map(([l]) => l)} onChange=${(k) => act('morning_fill', { key: r.key, fill: r.options[k][1] })} /></div>`)}
    </${Dialog}>`;
  }
  const foot = html`<span class="rest"></span><${Btn} kind="primary" label=${v.on} onClick=${() => act('ask_on', { on: true })} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${600} onClose=${() => act('ask_on', { on: true })} foot=${foot}><p class="co-para">${v.text}</p></${Dialog}>`;
}

export { T, tr };

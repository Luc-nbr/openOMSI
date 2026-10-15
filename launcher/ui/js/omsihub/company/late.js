// Late running as the company's pages show it (`company/late.rs`, Luc 2026-10-15): the day
// report's part - the late trips with their minutes, bus, driver and chain of causes, punctuality
// by line, hour and duty, the deductions by tier and cause with "How was this worked out?", and
// the advice against late running, each a button to where it is changed -, the finances'
// "Deductions for late running" dialog, and the advice list the Operations tab can show too
// (`late_advice`). Every text comes translated from the Rust side.

import { html, useState, setCo, setUi, goTab, goKey, useView, Icon, Btn, Dialog } from './kit.js';

// (its own look, beside company.css)
if (!document.querySelector('link[data-late-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/late.css';
  sheet.dataset.lateCss = '1';
  document.head.append(sheet);
}

/** RosterFlow opened as `o` asks (`openRosterFlow`), the Planning page where it is not on. */
function toRf(o) {
  setCo({ dialog: null });
  import('../rosterflow/index.js')
    .then((m) => m.openRosterFlow(o || {}))
    .then((ok) => ok || goKey('planning'))
    .catch(() => goKey('planning'));
}

/** A bus's dialog, or a person's card. */
function toBus(id) {
  setCo({ dialog: { kind: 'vehicle', id } });
}
function toPerson(id) {
  setCo({ dialog: null });
  setUi('people', { tab: 0, open: id });
  goTab(2);
}

/** A bus or a person as a link (`late::bus_json`, `driver_json`). */
export function WhoLink({ w, bus, before }) {
  if (!w) return null;
  if (w.id == null) return html`<span class="t-soft">${w.text}</span>`;
  return html`<button type="button" class="late-link" onClick=${(e) => (e.stopPropagation(), before?.(), bus ? toBus(w.id) : toPerson(w.id))}>${w.text}</button>`;
}

/** A button of an advice: RosterFlow, a page, a bus, a person. */
function go(b, before) {
  before?.();
  if (b.rf) toRf(b.rf);
  else if (b.bus != null) toBus(b.bus);
  else if (b.person != null) toPerson(b.person);
  else if (b.go) goKey(b.go);
}

/** The advice against late running (`late::advice_json`), each with its buttons. */
export function AdviceList({ list, caps, before }) {
  if (!list?.length) return null;
  return html`${caps && html`<div class="co-caps" style="margin:18px 0 8px">${caps}</div>`}
    ${list.map((a) => html`<div class="late-advice"><${Icon} name=${a.icon} size=${18} class="t-accent" /><div><p>${a.text}</p>
      <div class="fixes">${a.buttons.map((b) => html`<${Btn} label=${b.label} style="height:30px" onClick=${() => go(b, before)} />`)}</div></div></div>`)}`;
}

/** A late or dropped trip: its head, where, its bus and driver, and - opened - its causes. */
export function TripRow({ o, before }) {
  const [open, setOpen] = useState(false);
  return html`<div class=${'late-trip' + (open ? ' open' : '')}>
    <button type="button" class="co-row late-head" onClick=${() => setOpen(!open)}>
      <b class=${'late-badge t-bg-' + o.tone}>${o.label}</b>
      <span class="cut" style="flex:1"><b>${o.head}</b><span class="t-soft">  ·  ${o.place}</span></span>
      ${o.amount && html`<b class="t-danger">${o.amount}</b>`}
      <${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${18} class="t-dim" />
    </button>
    <div class="late-who">
      ${o.date && html`<span class="t-dim">${o.date}</span>`}
      ${o.bus && html`<span><${Icon} name="directions_bus" size=${13} /> <${WhoLink} w=${o.bus} bus before=${before} /></span>`}
      ${o.driver && html`<span><${Icon} name="person" size=${13} /> <${WhoLink} w=${o.driver} before=${before} /></span>`}
      <span class="t-dim">${o.time}</span>
    </div>
    ${open && html`<div class="late-why">${o.chain.map((c) => html`<p>${c}</p>`)}${o.deduction && html`<p class="t-soft">${o.deduction}${o.lost ? `  ·  ${o.lost}` : ''}</p>`}</div>`}
  </div>`;
}

/** The deductions by tier and cause, with how they were worked out. */
export function Deductions({ m }) {
  const [how, setHow] = useState(false);
  if (!m) return null;
  return html`<div class="late-money">
    <div class="co-caps" style="display:flex"><span class="grow">${m.caps}</span><b>${m.total}</b></div>
    <div class="late-cols">
      <div>${m.tiers.map((t) => html`<div class="co-monthrow"><span class="cut"><i class=${'late-dot t-bg-' + t.tone}></i>${t.label}</span><span class="t-dim">${t.trips}</span><b>${t.amount}</b></div>`)}</div>
      <div>${m.causes.map((t) => html`<div class="co-monthrow"><span class="cut">${t.label}</span><span class="t-dim">${t.trips}</span><b>${t.amount}</b></div>`)}</div>
    </div>
    <button type="button" class="late-how" onClick=${() => setHow(!how)}><${Icon} name="help" size=${15} />${m.how_label}</button>
    ${how && html`<div class="late-why">${m.how.map((t) => html`<p>${t}</p>`)}</div>`}
  </div>`;
}

/** Punctuality rows (by line, hour, duty) as small figures. */
function Puncts({ caps, rows }) {
  if (!rows?.length) return null;
  return html`<div class="late-puncts"><div class="co-caps">${caps}</div>
    <div class="list">${rows.map((r) => html`<div title=${r.under}><span class="cut">${r.label}</span><b class=${'t-' + r.tone}>${r.value}</b></div>`)}</div></div>`;
}

/** The day report's late running (`late::report_part`). */
export function LateSection({ v, before }) {
  if (!v) return null;
  return html`<div class="co-caps" style="display:flex;margin:18px 0 8px"><span class="grow">${v.caps}</span><span style="text-transform:none;font-weight:400;font-size:var(--co-note)">${v.hint}</span></div>
    ${v.none && html`<p class="co-para">${v.none}</p>`}
    ${v.trips.map((o) => html`<${TripRow} o=${o} before=${before} />`)}
    ${v.more && html`<p class="co-reportmore">${v.more}</p>`}
    <div class="late-grid">
      <${Puncts} caps=${v.by_line_caps} rows=${v.by_line} />
      <${Puncts} caps=${v.by_hour_caps} rows=${v.by_hour} />
      <${Puncts} caps=${v.by_duty_caps} rows=${v.by_duty} />
    </div>
    <${Deductions} m=${v.money} />
    <${AdviceList} list=${v.advice} caps=${v.advice_caps} before=${before} />`;
}

/** The finances' "Deductions for late running" (`late::finance_view`). */
export function LateDialog() {
  const v = useView('late');
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  return html`<${Dialog} icon="schedule" title=${v.title} w=${900} h=${780} onClose=${close} foot=${html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`}>
    <p class="co-note">${v.span}</p>
    <${Deductions} m=${v.money} />
    ${v.trips.length > 0 && html`<div class="co-caps" style="margin:18px 0 8px">${v.trips_caps}</div>`}
    ${v.trips.map((o) => html`<${TripRow} o=${o} />`)}
  </${Dialog}>`;
}

/** The advice of the last day closed, for a page of its own (the Operations tab). */
export function LateAdvice() {
  const v = useView('late_advice');
  if (!v?.list?.length) return null;
  return html`<${AdviceList} list=${v.list} caps=${v.caps} />`;
}

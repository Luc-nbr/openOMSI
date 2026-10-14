// RosterFlow's Preferences window (`crates/omsi-launcher-ui/src/company/planning/rf_prefs.rs`,
// Optibus's): a search box and IN USE / ALL on the left, the four kinds - VEHICLES, DRIVERS,
// DEPOTS, MISC - and their preferences under them; the one chosen on the right, with what it
// sets and, for a rule, whether it is a hard rule (never broken by Optimise, an error by hand)
// or a soft one that costs a penalty. SAVE CHANGES, CANCEL, SAVE & CLOSE at the foot. The
// preferences changed here are kept only when saved (`rf_act` "prefs").

import { html, useEffect } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, rfAct, hm, clock, eur } from './state.js';
import { Icon, RfButton, Switch, CheckRow, Seg, Stepper } from './ui.js';

/** The preferences, by kind: (kind, its icon, its items). */
const KINDS = [
  ['Vehicles', 'directions_bus', ['layover', 'prepost']],
  ['Drivers', 'person', ['signing', 'relief', 'travel', 'breaks', 'continuous', 'limits', 'pieces', 'changeover', 'split', 'types']],
  ['Depots', 'local_parking', ['depot']],
  ['Misc', 'apps', ['costs', 'algorithm']],
];

const NAME = {
  layover: 'Layover',
  prepost: 'Pre/Post-Trip',
  signing: 'Driver Signing',
  relief: 'Relief Timing',
  travel: 'Duty Travel',
  breaks: 'Duty Breaks',
  continuous: 'Continuous Break',
  limits: 'Time Limitations',
  pieces: 'Piece Length',
  changeover: 'Driver Changeover Vehicle',
  split: 'Split Break Definition',
  types: 'Duty Types',
  depot: 'Depot Setup',
  costs: 'Costs',
  algorithm: 'Algorithm Parameters',
};

const INFO = {
  layover: 'The least time a bus stands between two trips at one place.',
  prepost: "The bus's check before it leaves the depot and after it is back.",
  signing: "The paid minutes a driver signs on before work and off after it, at the duty's ends and around a split.",
  relief: "When the driver changes within the bus's layover at a relief point (a terminus or the depot).",
  travel: 'How a driver gets to a relief point at another place, and how long it takes.',
  breaks: 'A duty working longer than this needs a meal break of at least this long.',
  continuous: 'The longest a driver works without a break of at least this long.',
  limits: 'The spread of a duty (sign-on to sign-off) at most.',
  pieces: 'Pieces of work at least this long: short pieces are costly.',
  changeover: 'How many times a driver may change buses in a duty.',
  split: 'A break between two pieces this long or longer is a split: unpaid.',
  types: 'The kinds of duty the company works. A duty no allowed type fits is Other, an issue. The first that fits is its type.',
  depot: 'How many buses the depot holds: buses out at once at most.',
  costs: 'What the crew and the vehicles cost: Optimise works the cost down.',
  algorithm: 'What Optimise makes: the vehicles\' blocks, the duties, or both.',
};

/** In use: a rule that is on, or a setting (`Item::in_use`). */
function inUse(it, p) {
  switch (it) {
    case 'layover':
      return p.layover.on;
    case 'breaks':
      return p.meal.on;
    case 'continuous':
      return p.continuous.on;
    case 'limits':
      return p.max_spread.on;
    case 'pieces':
      return p.min_piece.on;
    case 'changeover':
      return p.changeovers.on;
    case 'depot':
      return p.depot_capacity.on;
    case 'types':
      return p.duty_types.length > 0;
    case 'costs':
      return p.wage != null || p.driver_fixed != null || p.vehicle_fixed != null || p.per_km != null;
    default:
      return true;
  }
}

const RELIEF = ['ArrivalDeparture', 'OnArrival', 'OnDeparture', 'MidLayover', 'BeforeDeparture'];
const TRAVEL = ['Walk', 'Public', 'Car'];
const TRI = ['No', 'Yes', 'Either'];




/** A rule's editor (`rf_prefs::limit`): in use, its value, hard or soft, the penalty. */
function Limit({ lim, label, step, max, fmt, set }) {
  const put = (patch) => set({ ...lim, ...patch });
  return html`<div class="pf-limit">
    <${Switch} on=${lim.on} label="In use" style="width:260px" onChange=${(v) => put({ on: v })} />
    ${lim.on && html`<div class="pf-row"><span>${tr(label)}</span><${Stepper} value=${lim.value} step=${step} min=${0} max=${max} fmt=${fmt} style="width:150px" onChange=${(v) => put({ value: v })} /></div>
      <div class="pf-row"><span>${tr('Rule')}</span><${Seg} at=${lim.hard ? 0 : 1} labels=${['Hard rule', 'Soft rule']} style="width:300px" onPick=${(k) => put({ hard: k === 0 })} /></div>
      ${lim.hard
        ? html`<p class="pf-note">${tr('Never broken by Optimise; an error when broken by hand.')}</p>`
        : html`<div class="pf-row"><span>${tr('Penalty')}</span><${Stepper} value=${Math.round(lim.penalty / 100)} step=${5} min=${0} max=${10000} fmt=${(e) => eur(e * 100)} style="width:150px" onChange=${(v) => put({ penalty: v * 100 })} /><em>${tr('Each time it is broken; Optimise weighs it against the cost.')}</em></div>`}`}
  </div>`;
}

/** A setting in minutes: its words and its stepper. */
const Minutes = ({ label, value, step = 1, max, set }) => html`<div class="pf-row"><span>${tr(label)}</span><${Stepper} value=${value} step=${step} min=${0} max=${max} fmt=${hm} style="width:150px" onChange=${set} /></div>`;

/** A cost: as the company pays (none), or set. */
function Cost({ label, value, auto, step, set }) {
  const on = value != null;
  return html`<div class="pf-row"><span>${tr(label)}</span>
    <${CheckRow} on=${on} label="Set" style="width:110px" onChange=${(v) => set(v ? auto : null)} />
    ${on ? html`<${Stepper} value=${value} step=${step} min=${0} max=${10000000} fmt=${eur} style="width:170px" onChange=${set} />` : html`<b class="pf-auto">${eur(auto)}  (${tr('as the company pays')})</b>`}
  </div>`;
}

/** The duty types, each an instance (`rf_prefs::duty_types`). */
function DutyTypes({ types, set, v }) {
  const put = (n, patch) => set(types.map((t, k) => (k === n ? { ...t, ...patch } : t)));
  return html`<div class="pf-types">
    ${types.map((t, n) => html`<div class="pf-type">
      <div class="top"><b>${n + 1}.</b><input value=${tr(t.name)} placeholder=${tr('Name')} onInput=${(e) => put(n, { name: e.currentTarget.value })} />
        <${Switch} on=${t.allowed} label="Allowed" style="width:140px" onChange=${(x) => put(n, { allowed: x })} />
        <span class="grow"></span>
        <button type="button" title=${tr('Earlier in the order: the first type a duty fits is its type')} onClick=${() => n > 0 && set(types.map((x, k) => (k === n - 1 ? types[n] : k === n ? types[n - 1] : x)))}><${Icon} name="arrow_upward" size=${17} /></button>
        <button type="button" class="del" onClick=${() => set(types.filter((_, k) => k !== n))}><${Icon} name="delete" size=${17} /></button></div>
      <div class="grid">
        <span class="cap">${tr('Start').toUpperCase()}</span><span class="two"><${Stepper} value=${t.start_from} step=${15} min=${0} max=${1439} fmt=${clock} style="width:110px" onChange=${(x) => put(n, { start_from: x })} /><${Stepper} value=${t.start_to} step=${15} min=${0} max=${1439} fmt=${clock} style="width:110px" onChange=${(x) => put(n, { start_to: x })} /></span>
        <span class="cap">${tr('End by').toUpperCase()}</span><${Stepper} value=${t.end_by} step=${15} min=${0} max=${2879} fmt=${clock} style="width:110px" onChange=${(x) => put(n, { end_by: x })} />
        <span class="cap">${tr('Length at most').toUpperCase()}</span><${Stepper} value=${t.max_length} step=${15} min=${60} max=${1440} fmt=${hm} style="width:110px" onChange=${(x) => put(n, { max_length: x })} />
        <span class="cap">${tr('Split').toUpperCase()}</span><${Seg} at=${Math.max(0, TRI.indexOf(t.split))} labels=${v.tri_labels} raw style="width:220px" onPick=${(k) => put(n, { split: TRI[k] })} />
        <span class="cap">${tr('Penalty').toUpperCase()}</span><${Stepper} value=${Math.round(t.penalty / 100)} step=${5} min=${0} max=${10000} fmt=${(e) => eur(e * 100)} style="width:130px" onChange=${(x) => put(n, { penalty: x * 100 })} />
      </div>
    </div>`)}
    <button type="button" class="pf-add" onClick=${() => set([...types, { name: tr('New type'), start_from: 0, start_to: 1439, end_by: 1800, max_length: 600, split: 'Either', allowed: true, penalty: 0 }])}><${Icon} name="add" size=${16} />${tr('Add preference')}</button>
  </div>`;
}

/** The window over the studio. */
export function PrefsWindow() {
  const e = S.rf.prefs;
  useEffect(() => {
    if (e.v) return;
    rfCmd('rf_view', { view: 'prefs', day: S.rf.day }).then((v) => setRf({ prefs: { ...S.rf.prefs, v, prefs: v.prefs } })).catch(() => setRf({ prefs: null }));
  }, []);
  if (!e.v) return html`<div class="rf-over rf-prefs"></div>`;
  const v = e.v;
  const pr = e.prefs;
  const set = (patch) => setRf({ prefs: { ...S.rf.prefs, ...patch } });
  const put = (patch) => set({ prefs: { ...pr, ...patch } });
  const q = (e.search || '').trim().toLowerCase();
  const item = e.item || 'signing';
  const save = async (close) => {
    await rfAct('prefs', { prefs: pr });
    if (close) setRf({ prefs: null });
  };
  let body;
  switch (item) {
    case 'layover':
      body = html`<${Limit} lim=${pr.layover} label="Least layover" step=${1} max=${120} fmt=${hm} set=${(x) => put({ layover: x })} />`;
      break;
    case 'prepost':
      body = html`<${Minutes} label="Pre-trip check" value=${pr.pre_trip} max=${60} set=${(x) => put({ pre_trip: x })} /><${Minutes} label="Post-trip check" value=${pr.post_trip} max=${60} set=${(x) => put({ post_trip: x })} />`;
      break;
    case 'signing':
      body = html`<${Minutes} label="Sign-on at the duty's start" value=${pr.sign_on} max=${60} set=${(x) => put({ sign_on: x })} />
        <${Minutes} label="Sign-off at the duty's end" value=${pr.sign_off} max=${60} set=${(x) => put({ sign_off: x })} />
        <${Minutes} label="Sign-on after a split" value=${pr.sign_on_split} max=${60} set=${(x) => put({ sign_on_split: x })} />
        <${Minutes} label="Sign-off before a split" value=${pr.sign_off_split} max=${60} set=${(x) => put({ sign_off_split: x })} />`;
      break;
    case 'relief':
      body = html`${RELIEF.map((t, k) => html`<${CheckRow} on=${pr.relief === t} label=${v.relief_labels[k]} raw style="width:460px" onChange=${() => put({ relief: t })} />`)}
        ${pr.relief === 'BeforeDeparture' && html`<${Minutes} label="Minutes before the departure" value=${pr.relief_before} max=${60} set=${(x) => put({ relief_before: x })} />`}
        <p class="pf-note">${tr("Relief points are the line's termini and the depot.")}</p>`;
      break;
    case 'travel':
      body = html`<div class="pf-row"><span>${tr('Way by default')}</span><${Seg} at=${Math.max(0, TRAVEL.indexOf(pr.travel))} labels=${v.travel_labels} raw style="width:420px" onPick=${(k) => put({ travel: TRAVEL[k] })} /></div>
        <${Minutes} label="Walk (W)" value=${pr.walk} max=${120} set=${(x) => put({ walk: x })} />
        <${Minutes} label="Public transport (P)" value=${pr.public} max=${120} set=${(x) => put({ public: x })} />
        <${Minutes} label="Staff car (R)" value=${pr.car} max=${120} set=${(x) => put({ car: x })} />`;
      break;
    case 'breaks':
      body = html`<${Minutes} label="A duty working longer than" value=${pr.meal_after} step=${15} max=${900} set=${(x) => put({ meal_after: x })} />
        <${Limit} lim=${pr.meal} label="needs a meal break of" step=${5} max=${180} fmt=${hm} set=${(x) => put({ meal: x })} />`;
      break;
    case 'continuous':
      body = html`<${Limit} lim=${pr.continuous} label="Longest work without a break" step=${15} max=${720} fmt=${hm} set=${(x) => put({ continuous: x })} />
        <${Minutes} label="A break is at least" value=${pr.break_min} step=${5} max=${120} set=${(x) => put({ break_min: x })} />
        <${Switch} on=${pr.layover_breaks} label="A layover with the bus counts as a break" style="width:420px" onChange=${(x) => put({ layover_breaks: x })} />`;
      break;
    case 'limits':
      body = html`<${Limit} lim=${pr.max_spread} label="Spread at most" step=${15} max=${1440} fmt=${hm} set=${(x) => put({ max_spread: x })} />`;
      break;
    case 'pieces':
      body = html`<${Limit} lim=${pr.min_piece} label="Pieces at least" step=${5} max=${600} fmt=${hm} set=${(x) => put({ min_piece: x })} />`;
      break;
    case 'changeover':
      body = html`<${Limit} lim=${pr.changeovers} label="Changes of bus at most" step=${1} max=${10} fmt=${String} set=${(x) => put({ changeovers: x })} />`;
      break;
    case 'split':
      body = html`<${Minutes} label="A split is a break of at least" value=${pr.split_min} step=${5} max=${600} set=${(x) => put({ split_min: x })} />`;
      break;
    case 'types':
      body = html`<${DutyTypes} types=${pr.duty_types} v=${v} set=${(x) => put({ duty_types: x })} />`;
      break;
    case 'depot':
      body = html`<${Limit} lim=${pr.depot_capacity} label="Buses out at once at most" step=${1} max=${500} fmt=${String} set=${(x) => put({ depot_capacity: x })} />`;
      break;
    case 'costs':
      body = html`<${Cost} label="A driver's hour" value=${pr.wage} auto=${v.rates.wage} step=${50} set=${(x) => put({ wage: x })} />
        <${Cost} label="Each duty besides its hours" value=${pr.driver_fixed} auto=${v.rates.driver_fixed} step=${100} set=${(x) => put({ driver_fixed: x })} />
        <${Cost} label="Each bus out a day" value=${pr.vehicle_fixed} auto=${v.rates.vehicle_fixed} step=${100} set=${(x) => put({ vehicle_fixed: x })} />
        <${Cost} label="Each kilometre" value=${pr.per_km} auto=${v.rates.per_km} step=${1} set=${(x) => put({ per_km: x })} />`;
      break;
    default:
      body = html`<${Switch} on=${pr.optimise_vehicles} label="Optimise vehicles (the blocks)" style="width:420px" onChange=${(x) => put({ optimise_vehicles: x })} />
        <${Switch} on=${pr.optimise_duties} label="Optimise duties (cut and paired)" style="width:420px" onChange=${(x) => put({ optimise_duties: x })} />`;
  }
  return html`<div class="rf-over rf-prefs">
    <div class="top"><${Icon} name="tune" size=${22} /><b>${tr('Preferences')}</b><span class="grow"></span>
      <${RfButton} label="Load defaults" style="width:176px;height:30px" onClick=${() => put(v.defaults)} /></div>
    <div class="side">
      <span class="rf-input"><${Icon} name="search" size=${18} /><input value=${e.search || ''} placeholder=${tr('Search')} onInput=${(ev) => set({ search: ev.currentTarget.value })} /></span>
      <${Seg} at=${e.inUse ? 0 : 1} labels=${['In use', 'All']} style="width:100%;margin-top:10px" onPick=${(k) => set({ inUse: k === 0 })} />
      <div class="kinds">${KINDS.map(([kind, icon, items]) => {
        const shown = items.filter((it) => (!e.inUse || inUse(it, pr)) && (!q || tr(NAME[it]).toLowerCase().includes(q) || NAME[it].toLowerCase().includes(q)));
        if (!shown.length && q) return null;
        return html`<div class="kind"><${Icon} name=${icon} size=${20} /><b>${tr(kind).toUpperCase()}</b></div>
          ${shown.map((it) => html`<button type="button" class=${'item' + (item === it ? ' on' : '') + (inUse(it, pr) ? ' used' : '')} onClick=${() => set({ item: it })}>${tr(NAME[it])}</button>`)}`;
      })}</div>
    </div>
    <div class="pane">
      <b class="title">${tr(NAME[item]).toUpperCase()}</b>
      <p class="info">${tr(INFO[item])}</p>
      <div class="pbody">${body}</div>
    </div>
    <div class="foot">
      <button type="button" class="pf-save" onClick=${() => save(false)}>${tr('Save changes').toUpperCase()}</button>
      <span class="grow"></span>
      <${RfButton} label="Cancel" style="width:150px;height:32px" onClick=${() => setRf({ prefs: null })} />
      <${RfButton} label="Save & close" solid style="width:152px;height:32px" onClick=${() => save(true)} />
    </div>
  </div>`;
}

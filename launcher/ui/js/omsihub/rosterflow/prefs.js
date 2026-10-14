// RosterFlow's Preferences window (`crates/omsi-launcher-ui/src/company/planning/rf_prefs.rs`,
// Optibus's): a search box and IN USE / ALL on the left, the four kinds - VEHICLES, DRIVERS,
// DEPOTS, MISC - and their preferences under them; the one chosen on the right, with what it
// sets and, for a rule, whether it is a hard rule (never broken by Optimise, an error by hand)
// or a soft one that costs a penalty. SAVE CHANGES, CANCEL, SAVE & CLOSE at the foot. The
// preferences changed here are kept only when saved (`rf_act` "prefs"); meanwhile each rule says
// next to its fields how many duties break it as edited (`rf_view` "prefs_check"), and after a
// save the studio says what the changed rules break, with Optimise offered.

import { html, useEffect, useRef } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, rfAct, hm, clock, eur } from './state.js';
import { Icon, RfButton, Switch, CheckRow, Seg, Stepper, Select } from './ui.js';

/** The preferences, by kind: (kind, its icon, its items). */
const KINDS = [
  ['Vehicles', 'directions_bus', ['layover', 'prepost']],
  ['Drivers', 'person', ['signing', 'points', 'relief', 'travel', 'breaks', 'continuous', 'limits', 'pieces', 'changeover', 'split', 'types']],
  ['Depots', 'local_parking', ['depot']],
  ['Misc', 'apps', ['costs', 'algorithm']],
];

const NAME = {
  layover: 'Layover',
  prepost: 'Pre/Post-Trip',
  signing: 'Driver Signing',
  points: 'Relief Points',
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
  points: "Where drivers may begin or end a duty, change on the bus and take a break: the depot and the lines' termini by themselves, and the stops on a line's way you add.",
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
    case 'points':
      return (p.points || []).length > 0;
    default:
      return true;
  }
}

const RELIEF = ['ArrivalDeparture', 'OnArrival', 'OnDeparture', 'MidLayover', 'BeforeDeparture'];
const TRAVEL = ['Walk', 'Public', 'Car'];
const TRI = ['No', 'Yes', 'Either'];
/** The ways' minutes in the preferences, by `TRAVEL`. */
const WAY_FIELD = ['walk', 'public', 'car'];

const eq = (a, b) => String(a ?? '').trim().toLowerCase() === String(b ?? '').trim().toLowerCase();

/** The relief points as the Preferences list them (`rf_view` prefs' `relief_points`): the ones
 * added since in, the ones taken out left out, each with its settings as edited. */
function pointRows(v, pr) {
  const rp = v.relief_points || { points: [], line_stops: [], kinds: [] };
  const kept = pr.points || [];
  const added = kept.filter((x) => x.added);
  const rows = rp.points.filter((r) => r.kind !== 2 || added.some((x) => eq(x.stop, r.stop)));
  for (const x of added) {
    if (rows.some((r) => eq(r.stop, x.stop))) continue;
    const ls = rp.line_stops.find((l) => eq(l.line, x.line));
    rows.push({ stop: x.stop, name: x.stop, kind: 2, lines: ls?.number || '', point: x });
  }
  return rows.map((r) => ({ ...r, point: kept.find((x) => eq(x.stop, r.stop)) || r.point }));
}

/** The relief points with `row`'s settings changed by `patch` (kept from now on). */
function setPoint(pr, row, patch) {
  const list = pr.points || [];
  const k = list.findIndex((x) => eq(x.stop, row.stop));
  if (k >= 0) return list.map((x, i) => (i === k ? { ...x, ...patch } : x));
  return [...list, { ...row.point, stop: row.stop, ...patch }];
}

/** Minutes in a small field: empty for none (`null`), what it falls back to greyed in it. */
function Minutes0({ value, placeholder, set }) {
  const put = (t) => set(t.trim() === '' ? null : Math.max(0, Math.min(240, Math.round(Number(t)) || 0)));
  return html`<input class="pf-min" type="number" min="0" max="240" value=${value ?? ''} placeholder=${placeholder} onChange=${(ev) => put(ev.currentTarget.value)} />`;
}

/** Preferences › Relief Points (`rosterflow::ReliefPoint`): every point and what it may be used
 * for, and a stop on a line's way added. */
function ReliefPoints({ v, pr, put, e, set }) {
  const rp = v.relief_points || { points: [], line_stops: [], kinds: [] };
  const rows = pointRows(v, pr);
  const ls = rp.line_stops || [];
  const li = Math.min(e.addLine || 0, Math.max(0, ls.length - 1));
  const line = ls[li];
  const free = line ? line.stops.filter((s) => !rows.some((r) => eq(r.stop, s))) : [];
  const si = Math.min(e.addStop || 0, Math.max(0, free.length - 1));
  const use = (row, key) => html`<${CheckRow} on=${row.point[key]} label="" raw class="pf-tick" onChange=${(x) => put({ points: setPoint(pr, row, { [key]: x }) })} />`;
  const add = () => put({ points: [...(pr.points || []), { stop: free[si], added: true, line: line.line, sign: true, relief: true, breaks: true, travel: null, walk: null, public: null, car: null }] });
  return html`<div class="pf-points">
    <table class="pf-table">
      <thead><tr><th>${tr('Relief point').toUpperCase()}</th><th>${tr('Lines').toUpperCase()}</th><th>${tr('Kind').toUpperCase()}</th><th class="c">${tr('Sign-on/off').toUpperCase()}</th><th class="c">${tr('Relief').toUpperCase()}</th><th class="c">${tr('Break').toUpperCase()}</th><th></th></tr></thead>
      <tbody>${rows.map((r) => html`<tr>
        <td class="name" title=${r.name}>${r.name}</td><td class="dim">${r.lines}</td><td class="dim">${rp.kinds[r.kind] || ''}</td>
        <td class="c">${use(r, 'sign')}</td><td class="c">${use(r, 'relief')}</td><td class="c">${use(r, 'breaks')}</td>
        <td class="c">${r.kind === 2 && html`<button type="button" class="del" title=${tr('Take this stop out of the relief points')} onClick=${() => put({ points: (pr.points || []).filter((x) => !eq(x.stop, r.stop)) })}><${Icon} name="delete" size=${16} /></button>`}</td>
      </tr>`)}</tbody>
    </table>
    ${ls.length
      ? html`<div class="pf-row pf-pick"><span>${tr('Add a stop on the way')}</span>
          <${Select} value=${li} options=${ls.map((l) => tr('Line %{n}', { n: l.number }))} style="width:150px" onChange=${(k) => set({ addLine: k, addStop: 0 })} />
          <${Select} value=${si} options=${free.length ? free : ['-']} style="width:240px" onChange=${(k) => set({ addStop: k })} />
          <${RfButton} label="Add stop" disabled=${!free.length} style="width:130px;height:30px" onClick=${add} />
        </div>`
      : html`<p class="pf-note">${tr("The lines' stops show once the timetable is read.")}</p>`}
  </div>`;
}

/** Duty Travel per relief point and between two (`rosterflow::travel_of`): each point's own way
 * and minutes, and the table from a point to another, one way at a time. An empty field takes
 * what comes after it: the way back's, the points' own, then the defaults. */
function TravelPoints({ v, pr, put, e, set }) {
  const rows = pointRows(v, pr);
  if (!rows.length) return null;
  const labels = v.travel_labels;
  const way = e.way || 0;
  const f = WAY_FIELD[way];
  const pairs = pr.pairs || [];
  const pairOf = (a, b) => pairs.find((x) => eq(x.from, a.stop) && eq(x.to, b.stop));
  const setPair = (a, b, m) => {
    const k = pairs.findIndex((x) => eq(x.from, a.stop) && eq(x.to, b.stop));
    const next = k >= 0 ? pairs.map((x, i) => (i === k ? { ...x, [f]: m } : x)) : [...pairs, { from: a.stop, to: b.stop, walk: null, public: null, car: null, [f]: m }];
    put({ pairs: next.filter((x) => x.walk != null || x.public != null || x.car != null) });
  };
  const fallback = (a, b) => pairOf(b, a)?.[f] ?? b.point[f] ?? a.point[f] ?? pr[f];
  return html`<b class="pf-sub">${tr('Per relief point').toUpperCase()}</b>
    <table class="pf-table">
      <thead><tr><th>${tr('Relief point').toUpperCase()}</th><th>${tr('Way').toUpperCase()}</th>${labels.map((l) => html`<th class="c">${l.toUpperCase()}</th>`)}</tr></thead>
      <tbody>${rows.map((r) => html`<tr>
        <td class="name" title=${r.name}>${r.name}</td>
        <td><${Select} value=${r.point.travel ? TRAVEL.indexOf(r.point.travel) + 1 : 0} options=${[tr('Default'), ...labels]} style="width:170px;height:26px" onChange=${(k) => put({ points: setPoint(pr, r, { travel: k ? TRAVEL[k - 1] : null }) })} /></td>
        ${WAY_FIELD.map((w) => html`<td class="c"><${Minutes0} value=${r.point[w]} placeholder=${pr[w]} set=${(m) => put({ points: setPoint(pr, r, { [w]: m }) })} /></td>`)}
      </tr>`)}</tbody>
    </table>
    <div class="pf-row"><b class="pf-sub">${tr('Between two relief points').toUpperCase()}</b><span class="grow"></span><${Seg} at=${way} labels=${labels} raw style="width:420px" onPick=${(k) => set({ way: k })} /></div>
    <div class="pf-matrix"><table>
      <tr><th>${tr('From → to').toUpperCase()}</th>${rows.map((b) => html`<th title=${b.name}>${b.name}</th>`)}</tr>
      ${rows.map((a) => html`<tr><th title=${a.name}>${a.name}</th>${rows.map((b) => (eq(a.stop, b.stop) ? html`<td class="x">-</td>` : html`<td><${Minutes0} value=${pairOf(a, b)?.[f]} placeholder=${fallback(a, b)} set=${(m) => setPair(a, b, m)} /></td>`))}</tr>`)}
    </table></div>
    <p class="pf-note">${tr("An empty field takes the way back's time, else the relief point's own, else the default.")}</p>`;
}




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

/** What breaks a rule with the preferences as edited (`breaks.rs` `prefs_check`), next to its
 * fields: red where anything does. */
const Live = ({ live, item, name }) => {
  const c = live?.[item];
  return c ? html`<p class=${'pf-live' + (c.n > 0 && item !== 'split' ? ' bad' : '')}>${name ? `${tr(name)}: ${c.text}` : c.text}</p>` : null;
};

/** The live check of the preferences as edited, a moment after the last change (and again after
 * a save, when the saved rules are these). */
function useLive(pr, saves) {
  const seq = useRef(0);
  const key = JSON.stringify(pr);
  useEffect(() => {
    if (!pr) return undefined;
    const n = ++seq.current;
    const t = setTimeout(() => {
      rfCmd('rf_view', { view: 'prefs_check', day: S.rf.day, prefs: pr })
        .then((x) => n === seq.current && S.rf.prefs && setRf({ prefs: { ...S.rf.prefs, live: x?.counts || null } }))
        .catch(() => {});
    }, 280);
    return () => clearTimeout(t);
  }, [key, saves]);
}

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
  useLive(e.prefs, e.saves || 0);
  if (!e.v) return html`<div class="rf-over rf-prefs"></div>`;
  const v = e.v;
  const pr = e.prefs;
  const set = (patch) => setRf({ prefs: { ...S.rf.prefs, ...patch } });
  const put = (patch) => set({ prefs: { ...pr, ...patch } });
  const q = (e.search || '').trim().toLowerCase();
  const item = e.item || 'signing';
  const live = e.live;
  const save = async (close) => {
    const r = await rfAct('prefs', { prefs: pr });
    // (what the changed rules break now: over the Gantt, with Optimise offered)
    setRf({ ruleOffer: r?.offer || null });
    if (close) setRf({ prefs: null });
    else set({ saves: (S.rf.prefs?.saves || 0) + 1 });
  };
  let body;
  switch (item) {
    case 'layover':
      body = html`<${Limit} lim=${pr.layover} label="Least layover" step=${1} max=${120} fmt=${hm} set=${(x) => put({ layover: x })} /><${Live} live=${live} item="layover" />`;
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
        <p class="pf-note">${tr("Relief points are set in Relief Points: the depot, the lines' termini and the stops you add.")}</p>`;
      break;
    case 'points':
      body = html`<${ReliefPoints} v=${v} pr=${pr} put=${put} e=${e} set=${set} />`;
      break;
    case 'travel':
      body = html`<div class="pf-row"><span>${tr('Way by default')}</span><${Seg} at=${Math.max(0, TRAVEL.indexOf(pr.travel))} labels=${v.travel_labels} raw style="width:420px" onPick=${(k) => put({ travel: TRAVEL[k] })} /></div>
        <${Minutes} label="Walk (W)" value=${pr.walk} max=${120} set=${(x) => put({ walk: x })} />
        <${Minutes} label="Public transport (P)" value=${pr.public} max=${120} set=${(x) => put({ public: x })} />
        <${Minutes} label="Staff car (R)" value=${pr.car} max=${120} set=${(x) => put({ car: x })} />
        <${Live} live=${live} item="travel" />
        <${TravelPoints} v=${v} pr=${pr} put=${put} e=${e} set=${set} />`;
      break;
    case 'breaks':
      body = html`<${Minutes} label="Show a gap as a break from" value=${pr.break_from ?? 15} step=${5} max=${120} set=${(x) => put({ break_from: x })} />
        <p class="pf-note">${tr('A shorter gap between two pieces or two trips is standing time: no break, it ends no driving period and counts for no rule.')}</p>
        <${Minutes} label="A duty working longer than" value=${pr.meal_after} step=${15} max=${900} set=${(x) => put({ meal_after: x })} />
        <${Limit} lim=${pr.meal} label="needs a meal break of" step=${5} max=${180} fmt=${hm} set=${(x) => put({ meal: x })} />
        ${pr.meal.on && html`<p class="pf-note">${tr('The meal break begins before the duty has worked that long.')}</p>`}
        <${Live} live=${live} item="breaks" />
        <${Live} live=${live} item="continuous" name="Continuous Break" />`;
      break;
    case 'continuous':
      body = html`<${Limit} lim=${pr.continuous} label="Longest work without a break" step=${15} max=${720} fmt=${hm} set=${(x) => put({ continuous: x })} />
        <${Minutes} label="A break is at least" value=${pr.break_min} step=${5} max=${120} set=${(x) => put({ break_min: x })} />
        <${Switch} on=${pr.layover_breaks} label="A layover with the bus counts as a break" style="width:420px" onChange=${(x) => put({ layover_breaks: x })} />
        <p class="pf-note">${tr('Gaps shorter than %{t} are standing time (Duty Breaks: show a gap as a break from).', { t: hm(pr.break_from ?? 15) })}</p>
        <${Live} live=${live} item="continuous" />`;
      break;
    case 'limits':
      body = html`<${Limit} lim=${pr.max_spread} label="Spread at most" step=${15} max=${1440} fmt=${hm} set=${(x) => put({ max_spread: x })} /><${Live} live=${live} item="limits" />`;
      break;
    case 'pieces':
      body = html`<${Limit} lim=${pr.min_piece} label="Pieces at least" step=${5} max=${600} fmt=${hm} set=${(x) => put({ min_piece: x })} /><${Live} live=${live} item="pieces" />`;
      break;
    case 'changeover':
      body = html`<${Limit} lim=${pr.changeovers} label="Changes of bus at most" step=${1} max=${10} fmt=${String} set=${(x) => put({ changeovers: x })} /><${Live} live=${live} item="changeover" />`;
      break;
    case 'split':
      body = html`<${Minutes} label="A split is a break of at least" value=${pr.split_min} step=${5} max=${600} set=${(x) => put({ split_min: x })} /><${Live} live=${live} item="split" />`;
      break;
    case 'types':
      body = html`<${Live} live=${live} item="types" /><${DutyTypes} types=${pr.duty_types} v=${v} set=${(x) => put({ duty_types: x })} />`;
      break;
    case 'depot':
      body = html`<${Limit} lim=${pr.depot_capacity} label="Buses out at once at most" step=${1} max=${500} fmt=${String} set=${(x) => put({ depot_capacity: x })} /><${Live} live=${live} item="depot" />`;
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

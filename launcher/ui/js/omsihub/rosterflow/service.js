// The day's service on RosterFlow's chips (Luc 2026-10-15: "When a trip fails, it's not shown in
// the roster flow view"): the trips of the day shown that ran late or were dropped
// (`company::late::service_json` - today's as the plan runs, another day's those of the last day
// closed of its weekday), each by its block and departure. A dropped trip is hatched red with a
// cross, a late one carries its minutes ("+7") in its penalty tier's colour; the hover tells the
// bus, the driver and the chain of causes, the trip card and the Issues panel's group the same,
// each name a link to the bus or the person. Every text comes translated from the Rust side.

import { html } from '../../../vendor/preact-htm.js';
import { setCo, setUi, goTab, goKey } from '../company/kit.js';
import * as D from './draw.js';
import { setRf, VEHICLES, DRIVERS } from './state.js';
import { Icon } from './ui.js';

// (its own look, beside rosterflow.css)
if (!document.querySelector('link[data-late-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/late.css';
  sheet.dataset.lateCss = '1';
  document.head.append(sheet);
}

/** The tiers' colours (as `late::tier_tone` names them). */
const TONE = { ok: [46, 160, 90, 1], warn: [226, 178, 40, 1], orange: [238, 122, 34, 1], danger: [222, 62, 62, 1] };

const memo = new WeakMap();

/** The day's service of a board's answer (`rf_board`'s head) or the Roster tab's (`rf_roster`). */
export const serviceOf = (data) => data?.head?.outcomes || data?.outcomes || null;

/** The service's trips by their block and departure. */
export function indexOf(svc) {
  if (!svc?.trips) return null;
  let m = memo.get(svc);
  if (!m) {
    m = new Map();
    for (const o of svc.trips) m.set(`${o.block_id}|${o.dep}`, o);
    memo.set(svc, m);
  }
  return m;
}

/** A trip's outcome (`id`: its block's row ID, "B|line|tour"). */
export const at = (m, id, dep) => (m && m.get(`${id}|${dep}`)) || null;

/** The outcomes of a tour (and of one of its duties). */
export function ofTour(svc, line, tour, duty = null) {
  if (!svc?.trips) return [];
  return svc.trips.filter((o) => o.line === line && o.tour === tour && (duty == null || o.duty === duty));
}

/** Its mark over its chip `cr`: a dropped trip hatched with a cross, a late one its minutes. */
export function mark(ctx, cr, o) {
  if (!o) return;
  if (o.dropped) {
    const red = TONE.danger;
    ctx.save();
    ctx.beginPath();
    ctx.rect(cr.x, cr.y, cr.w, cr.h);
    ctx.clip();
    D.rect(ctx, cr.x, cr.y, cr.w, cr.h, [red[0], red[1], red[2], 0.35]);
    for (let sx = cr.x - cr.h; sx < cr.x + cr.w; sx += 6) D.stroke(ctx, [[sx, cr.y + cr.h], [sx + cr.h, cr.y]], 1.4, red);
    ctx.restore();
    const s = Math.min(cr.h - 6, 10);
    if (cr.w >= s + 4) {
      const cx = cr.x + cr.w / 2;
      const cy = cr.y + cr.h / 2;
      D.stroke(ctx, [[cx - s / 2, cy - s / 2], [cx + s / 2, cy + s / 2]], 2.2, [255, 255, 255, 1]);
      D.stroke(ctx, [[cx - s / 2, cy + s / 2], [cx + s / 2, cy - s / 2]], 2.2, [255, 255, 255, 1]);
    }
    D.roundedBorder(ctx, cr.x - 1, cr.y - 1, cr.w + 2, cr.h + 2, 3, 1.6, red);
    return;
  }
  const col = TONE[o.tone] || TONE.warn;
  const w = Math.max(18, D.width(ctx, o.label, 9.5, 800) + 6);
  const x = cr.x + cr.w - w + 2;
  const y = cr.y - 6;
  D.rounded(ctx, x, y, w, 12, 3, col);
  D.textAt(ctx, o.label, x + w / 2, y + 9.5, 9.5, 800, o.tone === 'warn' ? [30, 26, 10, 1] : [255, 255, 255, 1], 'center');
  D.roundedBorder(ctx, cr.x - 1, cr.y - 1, cr.w + 2, cr.h + 2, 3, 1.4, col);
}

/** A small mark at minute `x` of a tour's or a duty's bar (the Roster tab). */
export function tick(ctx, x, br, o) {
  const col = o.dropped ? TONE.danger : TONE[o.tone] || TONE.warn;
  D.rect(ctx, x - 1, br.y, 2.5, br.h, col);
  if (o.dropped) D.circle(ctx, x + 0.25, br.y + 3, 3, col);
}

/** A tour's count on its bus chip `r` (the Roster tab): its trips dropped (red), else late. */
export function count(ctx, r, list) {
  const dropped = list.filter((o) => o.dropped).length;
  const text = dropped ? `×${dropped}` : `+${list.length}`;
  const col = dropped ? TONE.danger : TONE.warn;
  const w = Math.max(18, D.width(ctx, text, 9.5, 800) + 8);
  D.rounded(ctx, r.x + r.w - w + 4, r.y - 5, w, 12, 3, col);
  D.textAt(ctx, text, r.x + r.w - w / 2 + 4, r.y + 4.5, 9.5, 800, dropped ? [255, 255, 255, 1] : [30, 26, 10, 1], 'center');
}

/** What the hover tells of it. */
export function tipOf(o) {
  const lines = [o.head, o.place];
  if (o.bus) lines.push(`${o.bus_caps}: ${o.bus.text}`);
  if (o.driver) lines.push(`${o.driver_caps}: ${o.driver.text}`);
  for (const c of o.chain || []) lines.push(`• ${c}`);
  if (o.deduction) lines.push(o.deduction);
  if (o.lost) lines.push(o.lost);
  return lines.filter(Boolean).join('\n');
}

/** A bus's or a person's name as a link: the bus's dialog, the person's card. */
export function Who({ w, kind }) {
  if (!w) return null;
  if (w.id == null) return html`<span>${w.text}</span>`;
  const go = () => {
    if (kind === 'bus') setCo({ dialog: { kind: 'vehicle', id: w.id } });
    else {
      setUi('people', { tab: 0, open: w.id });
      goTab(2);
    }
  };
  return html`<button type="button" class="rf-svc-link" onClick=${(e) => (e.stopPropagation(), go())}>${w.text}</button>`;
}

/** A trip's outcome in words: what became of it, where, its bus and driver, the causes. */
export function Outcome({ o }) {
  if (!o) return null;
  return html`<div class=${'rf-svc ' + (o.dropped ? 'danger' : o.tone)}>
    <div class="top"><b class="badge">${o.label}</b><b class="cut">${o.head}</b></div>
    <div class="dim cut">${o.place}  ·  ${o.time}</div>
    ${o.bus && html`<div class="who"><${Icon} name="directions_bus" size=${13} /><span class="dim">${o.bus_caps}:</span><${Who} w=${o.bus} kind="bus" /></div>`}
    ${o.driver && html`<div class="who"><${Icon} name="person" size=${13} /><span class="dim">${o.driver_caps}:</span><${Who} w=${o.driver} kind="person" /></div>`}
    ${(o.chain || []).map((c) => html`<div class="why">${c}</div>`)}
    ${o.deduction && html`<div class="money">${o.deduction}${o.lost ? `  ·  ${o.lost}` : ''}</div>`}
  </div>`;
}

/** Its chip shown: the day's board, the trip chosen in the tab the panel is over. */
function jump(data, drivers, o) {
  const b = data.board;
  const i = b.blocks.findIndex((x) => x.id === o.block_id);
  if (i < 0) return;
  const j = b.blocks[i].trips.findIndex((x) => x.dep === o.dep);
  if (!drivers) {
    setRf({ view: VEHICLES, reveal: o.block_id, selRow: o.block_id, sel: j >= 0 ? [[i, j]] : [], card: null, panel: null });
    return;
  }
  const d = b.duties.find((x) => x.spans.some((sp) => sp.tour === i && (b.blocks[i].pieces[sp.duty] || [0, 0])[0] <= j && j < (b.blocks[i].pieces[sp.duty] || [0, 0])[1]));
  if (d) setRf({ view: DRIVERS, reveal: d.id, selRow: d.id, sel: j >= 0 ? [[i, j]] : [], card: null, panel: null });
  else setRf({ view: VEHICLES, reveal: o.block_id, selRow: o.block_id, sel: j >= 0 ? [[i, j]] : [], card: null, panel: null });
}

/** An advice's button (`late::advice_json`): RosterFlow's place, a bus, a person, a page. */
function follow(b) {
  if (b.rf) import('./index.js').then((m) => m.openRosterFlow(b.rf));
  else if (b.bus != null) setCo({ dialog: { kind: 'vehicle', id: b.bus } });
  else if (b.person != null) {
    setUi('people', { tab: 0, open: b.person });
    goTab(2);
  } else if (b.go) goKey(b.go);
}

/** The Issues panel's "Today's service" group: the trips dropped and late, each to its chip. */
export function ServiceGroup({ data, drivers }) {
  const svc = serviceOf(data);
  if (!svc) return null;
  return html`<div class="rf-svc-group" data-rf="rf-service">
    <div class="group"><span class="badge">${svc.trips.length}</span><b class="cut">${svc.caps}</b><span class="dim cut">${svc.say}</span></div>
    ${svc.none && html`<p class="none">${svc.none}</p>`}
    ${svc.trips.map((o) => html`<div class="issue in rf-svc-row" title=${tipOf(o)} onClick=${() => jump(data, drivers, o)}><${Outcome} o=${o} /></div>`)}
    ${svc.advice?.length > 0 && html`<div class="group"><span class="badge">${svc.advice.length}</span><b class="cut">${svc.advice_caps}</b></div>
      ${svc.advice.map((a) => html`<div class="issue in rf-svc-advice"><${Icon} name=${a.icon} size=${15} /><div><p>${a.text}</p>
        <div class="btns">${a.buttons.map((b) => html`<button type="button" class="rf-svc-link" onClick=${() => follow(b)}>${b.label}</button>`)}</div></div></div>`)}`}
  </div>`;
}

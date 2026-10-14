// RosterFlow's plan copied to the other days it works with (Trello #100; the Rust side's
// `rosterflow/days.rs` and the core's `plandays`): SAVE's "Apply to" window - this day only,
// every day of its day type, every day with the same timetable, or from a date until a date;
// the vehicles, the drivers or both; each day of the week with what a copy would match there -
// the report of what was copied (the trips that matched nothing, each a link to it on its day),
// the toolbar's chip of the day shown (what it follows, own changes, UNSYNCED, Reset to pattern,
// Refresh) and the day switcher's marks.

import { html, useEffect } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, rfAct, after, bump, settle, VEHICLES } from './state.js';
import { Icon, RfButton, Caps, CheckRow, Seg } from './ui.js';

// (its own look, beside rosterflow.css)
if (!document.querySelector('link[data-rf-days-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/rosterflow-days.css';
  sheet.dataset.rfDaysCss = '1';
  document.head.append(sheet);
}

/** The presets of "Apply to". */
const THIS_DAY = 0;
const DAY_TYPE = 1;
const SAME_TIMETABLE = 2;
const RANGE = 3;
const PRESETS = ['This day only', 'Every day of this day type', 'Every day with the same timetable', 'From a date until a date'];

/** The "Apply to" window opened on the day shown (SAVE's arrow): what each day would get. */
export async function openApply() {
  setRf({ panel: { kind: 'apply' }, apply: null, menu: null, card: null });
  const v = await rfCmd('rf_view', { view: 'apply', day: S.rf.day }).catch((e) => ({ error: e.message }));
  if (S.rf.panel?.kind !== 'apply') return;
  const k = v.k ?? S.rf.day;
  const a = { v, preset: THIS_DAY, sourceAs: v.date_only ? 'date' : 'week', vehicles: true, drivers: true, ticks: {}, as: {}, from: k, until: Math.min(6, k + 1) };
  setRf({ apply: a });
}

/** The days of the week as the window has them: the day shown and the others, in order. */
function week(v) {
  return [{ k: v.k, label: v.label, source: true }, ...v.days].sort((a, b) => a.k - b.k);
}

/** What a preset ticks, and how each day takes it (every week, or that date only). */
function preset(a, p) {
  const v = a.v;
  const lo = Math.min(a.from, a.until);
  const hi = Math.max(a.from, a.until);
  const ticks = {};
  const as = {};
  for (const d of v.days || []) {
    let on = false;
    let how = d.in_type ? 'week' : 'date';
    if (p === DAY_TYPE) [on, how] = [d.in_type, 'week'];
    if (p === SAME_TIMETABLE) on = d.same === true;
    if (p === RANGE) [on, how] = [d.k >= lo && d.k <= hi, 'date'];
    ticks[d.k] = on;
    as[d.k] = how;
  }
  const sourceAs = p === RANGE ? (v.k >= lo && v.k <= hi ? 'date' : 'week') : v.date_only ? 'date' : 'week';
  return { ...a, preset: p, ticks, as, sourceAs };
}

/** What a copy would make of a day, in words and its tone. */
function outlook(d) {
  if (d.reading || !d.preview) return [tr('Reading the timetable…'), 'dim'];
  const p = d.preview;
  if (p.exact) return [tr('Same timetable: an exact copy'), 'ok'];
  if (p.missing === 0) {
    const all = tr('Another timetable: all %{n} of its trips match', { n: p.total });
    return [p.gone > 0 ? `${all} · ${tr("%{n} of this day's trips do not run there", { n: p.gone })}` : all, 'ok'];
  }
  const words = tr('Another timetable: %{a} of %{b} trips match', { a: p.matched, b: p.total });
  return [`${words} · ${tr('%{n} to plan by hand', { n: p.missing })}`, 'warn'];
}

/** SAVE's "Apply to" window (Optibus's apply to days): the day shown and how it is saved, what
 * to copy, and the days of the week to copy it to. */
export function ApplyWindow({ size }) {
  const a = S.rf.apply;
  const put = (patch) => setRf({ apply: { ...S.rf.apply, ...patch } });
  const w = Math.min(680, size.w - 40);
  const shell = (body, foot) => html`<div class="rf-over rf-dim" onMouseDown=${(e) => e.target === e.currentTarget && setRf({ panel: null, apply: null })}>
    <div class="rf-sheetwin rf-apply" style=${`left:${Math.max(10, size.w / 2 - w / 2)}px;top:40px;width:${w}px;max-height:${Math.max(240, size.h - 80)}px`} data-rf="rf-apply">
      <div class="title"><b>${tr('Save and apply to other days')}</b><button type="button" title=${tr('Close')} onClick=${() => setRf({ panel: null, apply: null })}><${Icon} name="close" size=${18} /></button></div>
      <div class="body">${body}</div>
      <div class="foot">${foot}</div>
    </div>
  </div>`;
  if (!a) return shell(html`<p class="rf-apply-note">${tr('Reading the timetable…')}</p>`, html`<span class="grow"></span><${RfButton} label="Cancel" style="width:116px;height:32px" onClick=${() => setRf({ panel: null, apply: null })} />`);
  const v = a.v;
  if (v.error || v.none) return shell(html`<p class="rf-apply-note err">${v.error || ''}</p>`, html`<span class="grow"></span><${RfButton} label="Close" style="width:116px;height:32px" onClick=${() => setRf({ panel: null, apply: null })} />`);
  const days = v.days || [];
  const ticked = days.filter((d) => a.ticks[d.k] && !d.reading);
  const nothing = !a.vehicles && !a.drivers;
  const label = ticked.length ? 'Save and apply' : a.sourceAs === 'date' && !v.date_only ? 'Save for this date' : 'Save';
  const idle = !v.dirty && !ticked.length && (a.sourceAs === 'week' || v.date_only);
  const go = async () => {
    const targets = ticked.map((d) => ({ k: d.k, as: a.as[d.k] || (d.in_type ? 'week' : 'date') }));
    const r = await rfAct('save', { apply: { source_as: a.sourceAs, targets, vehicles: a.vehicles, drivers: a.drivers } });
    settle();
    const rep = r?.report;
    const shown = rep && rep.days.some((d) => d.missing.length > 0);
    setRf({ panel: shown ? { kind: 'copied', report: rep } : null, apply: null });
  };
  const all = week(v);
  const body = html`
    <div class="rf-apply-src">
      <${Icon} name="calendar_month" size=${18} /><b>${v.label}</b><span class="dim">${v.type}</span>
      <span class="grow"></span>
      <${Seg} at=${a.sourceAs === 'date' ? 1 : 0} raw labels=${[tr('Every %{day}', { day: v.weekday }), tr('Only %{date}', { date: v.label })]} style="width:300px" onPick=${(k) => put({ sourceAs: k === 1 ? 'date' : 'week' })} />
    </div>
    <p class="rf-apply-note">${a.sourceAs === 'date' ? tr('Saved for this date only: its weekday keeps the planning it had (an exception for one day).') : tr('Saved for this weekday: its planning holds every week.')}</p>
    <${Caps} text="Apply to" class="accent" />
    <${Seg} at=${a.preset} labels=${PRESETS} onPick=${(p) => setRf({ apply: preset(S.rf.apply, p) })} />
    ${a.preset === RANGE && html`<div class="rf-apply-range"><span>${tr('From')}</span>
      <span class="rf-select"><select value=${String(a.from)} onChange=${(e) => setRf({ apply: preset({ ...S.rf.apply, from: Number(e.currentTarget.value) }, RANGE) })}>${all.map((d) => html`<option value=${String(d.k)}>${d.label}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>
      <span>${tr('until')}</span>
      <span class="rf-select"><select value=${String(a.until)} onChange=${(e) => setRf({ apply: preset({ ...S.rf.apply, until: Number(e.currentTarget.value) }, RANGE) })}>${all.map((d) => html`<option value=${String(d.k)}>${d.label}</option>`)}</select><${Icon} name="expand_more" size=${18} /></span>
      <span class="dim">${tr("These dates only: afterwards each runs its weekday's planning again.")}</span></div>`}
    <${Caps} text="Copy" class="accent" style="margin-top:6px" />
    <div class="rf-apply-what">
      <${CheckRow} on=${a.vehicles} label="Vehicles (the buses' blocks)" onChange=${(on) => put({ vehicles: on })} />
      <${CheckRow} on=${a.drivers} label="Drivers (the duties)" onChange=${(on) => put({ drivers: on })} />
    </div>
    ${nothing && html`<p class="rf-apply-note err">${tr('Choose what to copy: the vehicles, the drivers or both.')}</p>`}
    <${Caps} text="Days" class="accent" style="margin-top:6px" />
    <div class="rf-apply-days">
      ${days.map((d) => {
        const [words, tone] = outlook(d);
        const on = !!a.ticks[d.k];
        return html`<div class=${'row' + (on ? ' on' : '')}>
          <${CheckRow} on=${on} label=${d.label} raw style="width:130px" onChange=${(x) => put({ ticks: { ...a.ticks, [d.k]: x } })} />
          <${Seg} at=${a.as[d.k] === 'date' || (!a.as[d.k] && !d.in_type) ? 1 : 0} labels=${['Every week', 'This date only']} style="width:190px" onPick=${(x) => put({ as: { ...a.as, [d.k]: x === 1 ? 'date' : 'week' }, ticks: { ...a.ticks, [d.k]: true } })} />
          <span class=${'what cut ' + tone} title=${words}>${words}</span>
          ${on && d.own && html`<span class="tag warn" title=${tr('Its own changes are replaced by the copy')}>${tr('Own changes')}</span>`}
        </div>`;
      })}
    </div>
    ${ticked.some((d) => d.preview && !d.preview.exact) && html`<p class="rf-apply-note warn"><${Icon} name="warning" size=${14} />${tr('A day of another timetable takes only the trips that match (same line, way, departure and stops). The rest run as its timetable has them and are listed, to plan by hand.')}</p>`}
    ${v.others?.length > 0 && html`<p class="rf-apply-note">${tr('Your changes on %{days} are saved too.', { days: v.others.join(', ') })}</p>`}`;
  const foot = html`<span class="grow"></span>
    <${RfButton} label="Cancel" style="width:116px;height:32px" onClick=${() => setRf({ panel: null, apply: null })} />
    <${RfButton} label=${label} solid disabled=${idle || (ticked.length > 0 && nothing)} data="rf-apply-go" style="width:170px;height:32px" onClick=${go} />`;
  return shell(body, foot);
}

/** A trip a copy left to plan by hand shown on its day: the day, the Vehicles tab, its bus and
 * the trip chosen once the day is drawn (`useRevealTrip`). */
function goTo(k, date, m) {
  setRf({ day: k, view: VEHICLES, hand: null, zones: null, card: null, menu: null, sel: [], reveal: m.id, selRow: m.id, revealTrip: { id: m.id, dep: m.dep, date } });
}

/** The trip `goTo` asked for chosen, once its day's board is there. */
export function useRevealTrip(data) {
  useEffect(() => {
    const want = S.rf.revealTrip;
    if (!want || !data?.board || data.head?.date !== want.date) return;
    const i = data.board.blocks.findIndex((b) => b.id === want.id);
    const j = i < 0 ? -1 : data.board.blocks[i].trips.findIndex((x) => x.dep === want.dep);
    setRf({ revealTrip: null, sel: j >= 0 ? [[i, j]] : [], selRow: i >= 0 ? want.id : null, reveal: i >= 0 ? want.id : null });
  }, [data]);
}

/** What was copied: each day, how much of it matched, and the trips to plan by hand - each a
 * link to it on its day. */
export function CopyReport({ report, gTop }) {
  const days = S.rf.days || [];
  return html`<div class="rf-over rf-sheetwin rf-copied" style=${`top:${gTop + 8}px;bottom:40px;right:10px;width:400px`} data-rf="rf-copied">
    <div class="title"><b class="cut">${tr('Copied from %{day}', { day: report.from })}</b><button type="button" title=${tr('Close')} onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${18} /></button></div>
    <div class="body">
      ${report.days.map((d) => html`<div class="day">
        <div class="head"><b>${d.label}</b><span class="dim">${tr(d.every_week ? 'Every week' : 'This date only')}</span><span class="grow"></span>
          <span class=${d.missing.length ? 'warn' : 'ok'}>${d.exact ? tr('Exact copy') : tr('%{a} of %{b} trips as copied', { a: d.matched, b: d.total })}</span></div>
        ${d.gone > 0 && html`<p class="dim">${tr('%{n} trips of the pattern do not run that day.', { n: d.gone })}</p>`}
        ${d.missing.length > 0 && html`<p class="dim">${tr("To plan by hand: they run as the day's timetable has them.")}</p>`}
        ${d.missing.map((m) => html`<button type="button" class="trip" title=${tr('Show the trip on its day')} onClick=${() => goTo(d.k, days[d.k]?.date, m)}><${Icon} name="directions_bus" size=${14} /><span class="cut">${m.label}</span><${Icon} name="chevron_right" size=${14} /></button>`)}
      </div>`)}
    </div>
  </div>`;
}

/** The day shown's own report: the trips its copy left to plan by hand. */
function ownReport(data) {
  const d = data.head.day;
  const k = S.rf.day;
  return { from: d.pattern || '', days: [{ k, label: data.head.day_words, every_week: true, matched: d.matched, total: d.total, exact: false, gone: 0, missing: d.missing }] };
}

/** A line saved in the Timeplan since its blocks were made: its blocks and duties made anew. */
async function refreshLine(line) {
  const r = await rfCmd('rf_tp_act', { act: 'refresh', line }).catch((e) => ({ say: { text: e.message, error: true } }));
  after(r);
  bump();
}

/** The toolbar's chip of the day shown: what it follows and whether it is still as copied, what
 * follows it; Reset to pattern, Refresh, Stop following; the lines the Timeplan changed. */
export function DayChip({ data }) {
  const d = data.head?.day;
  const lines = data.head?.unsynced || [];
  if (!d || (!d.tags?.length && !lines.length)) return null;
  const reset = () => rfAct('day_reset').then((r) => {
    settle();
    if (r?.report) setRf({ panel: { kind: 'copied', report: r.report } });
  });
  return html`<span class="rf-daychip" data-rf="rf-day-chip">
    ${d.tags.map((t) => {
      if (t.kind === 'missing') return html`<button type="button" class=${'tag ' + t.tone} title=${tr('The trips to plan by hand, each a link to it')} onClick=${() => setRf({ panel: { kind: 'copied', report: ownReport(data) } })}>${t.text}</button>`;
      // (what it follows, with the cross that stops it following)
      if (t.kind === 'follows') return html`<span class=${'tag ' + t.tone}>${t.text}<button type="button" class="x" title=${tr('Stop following: this day keeps its planning as it is')} onClick=${() => rfAct('day_unfollow')}><${Icon} name="close" size=${11} /></button></span>`;
      return html`<span class=${'tag ' + t.tone}>${t.text}</span>`;
    })}
    ${d.reset && html`<button type="button" class="act" data-rf="rf-day-reset" title=${tr('Make this day as its pattern is again: its own changes go')} onClick=${reset}><${Icon} name="restart_alt" size=${14} />${tr('Reset to pattern')}</button>`}
    ${d.refresh && html`<button type="button" class="act" title=${tr('Its pattern changed since it was copied: copy it again')} onClick=${reset}><${Icon} name="refresh" size=${14} />${tr('Refresh')}</button>`}
    ${lines.map((l) => html`<button type="button" class="act warn" title=${tr('The timetable changed since the scheduling studio made the blocks of this line: make its blocks and duties anew')} onClick=${() => refreshLine(l.line)}><${Icon} name="refresh" size=${14} />${tr('UNSYNCED')} · ${tr('Line %{n}', { n: l.number })}</button>`)}
  </span>`;
}

/** A day of the day switcher with what it runs under it. */
export function DayItem({ label, on, tags, onPick }) {
  return html`<button type="button" class=${'rf-mi rf-dayitem' + (on ? ' on' : '')} onClick=${onPick}>
    <span class="cut">${label}</span>
    ${tags?.length > 0 && html`<span class="marks">${tags.map((t) => html`<i class=${t.tone}>${t.text}</i>`)}</span>`}
  </button>`;
}

/** The day switcher's height for these days and their marks. */
export const dayMenuHeight = (days, marks) => days.reduce((s, _, k) => s + (marks?.[k]?.length ? 44 : 30), 0) + 10;

/** The windows of this module over the Vehicles and Drivers tabs. */
export function DaysLayer({ size, gTop }) {
  const p = S.rf.panel;
  const kind = p?.kind;
  if (kind !== 'apply' && kind !== 'copied') return null;
  return html`<div class="rf-layer">
    ${kind === 'apply' && html`<${ApplyWindow} size=${size} />`}
    ${kind === 'copied' && p.report && html`<${CopyReport} report=${p.report} gTop=${gTop} />`}
  </div>`;
}

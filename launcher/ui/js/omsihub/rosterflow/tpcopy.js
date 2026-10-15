// The Timeplan's "Copy to…" (testers: "I can't copy a timetable from MF to SA or SU"; "copy the
// timetable that you've created for one day ... over to another day easily"; "a template for the
// given day so that it can be used on more days"; the core's `plandays::timetable`): a line's
// trips of the day shown - all of them, or the trips chosen - or a template kept by name, copied
// onto the other day groups ticked, in place of their trips or beside them. A template is a saved
// source of this same copy: kept here from the day or the trips chosen, renamed, thrown away.
// The copy goes into the line's draft like every change of the Timeplan (Save keeps it, Undo takes
// it back); saved and refreshed, SAVE › Apply to in Vehicles and Drivers copies the day's plan
// onto those days as onto a day of the same timetable.

import { html } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setPart } from './state.js';
import { Icon } from './ui.js';

// (its own look, beside rosterflow.css)
if (!document.querySelector('link[data-rf-tt-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/rosterflow-timetable.css';
  sheet.dataset.rfTtCss = '1';
  document.head.append(sheet);
}

const tpSet = (patch) => setPart('tp', patch);

/** The trips chosen (places in the line's table): those picked, else the one chosen. */
export const chosenTrips = (t) => (t.picked?.length ? t.picked : t.sel != null ? [t.sel] : []);

/** The panel opened: the trips chosen as the source when there are several, else the day. */
export const openCopy = (chosen) => tpSet({ copying: { source: chosen.length > 1 ? 'trips' : 'day', ticks: {}, how: 'replace', name: '', renaming: null, armed: null }, adding: null, chart: null, issuesOpen: false, cell: null });

/** What the page hands for the source chosen. */
function sourceArg(c, chosen) {
  if (c.source === 'trips') return { kind: 'trips', trips: chosen };
  if (typeof c.source === 'number') return { kind: 'template', id: c.source };
  return { kind: 'day' };
}

/** A box ticked or not, with its words (`disabled`: shown, not to tick). */
function Tick({ on, disabled, label, tag, tone = 'dim', onChange }) {
  return html`<button type="button" class=${'tp-tick' + (on ? ' on' : '') + (disabled ? ' off' : '')} disabled=${disabled} onClick=${() => !disabled && onChange(!on)}>
    <b>${on && html`<${Icon} name="check" size=${12} />`}</b><span class="cut">${label}</span>${tag && html`<i class=${tone}>${tag}</i>`}
  </button>`;
}

/** A source to choose: a round mark, its words and how many trips. */
function Source({ on, label, sub, onPick, children }) {
  return html`<div class=${'tp-src' + (on ? ' on' : '')}>
    <button type="button" class="pick" onClick=${onPick}><i></i><span class="cut">${label}</span><small class="cut">${sub}</small></button>
    ${children}
  </div>`;
}

/** "Copy to…" over the timetable: the source (the day, the trips chosen, a template), the day
 * groups to copy to, Replace or Add; a template kept of the day or the trips chosen. */
export function CopyPanel({ v, act }) {
  const t = S.rf.tp;
  const c = t.copying;
  const put = (patch) => tpSet({ copying: { ...S.rf.tp.copying, ...patch } });
  const chosen = chosenTrips(t);
  const days = v.copy.days || [];
  const templates = v.copy.templates || [];
  const shown = days[t.day];
  // (a template thrown away meanwhile, or no trips chosen any more: the day)
  let source = c.source;
  if (typeof source === 'number' && !templates.some((x) => x.id === source)) source = 'day';
  if (source === 'trips' && !chosen.length) source = 'day';
  const kept = typeof source === 'number' ? templates.find((x) => x.id === source) : null;
  const count = kept ? kept.trips : source === 'trips' ? chosen.length : shown?.trips || 0;
  // (the day a source of the day shown is of: not a day to copy it to)
  const own = (d) => !kept && d.k === t.day;
  const ticked = days.filter((d) => c.ticks[d.k] && !d.closed && !own(d));
  const go = async () => {
    const r = await act('copy_to', { source: sourceArg({ source }, chosen), days: ticked.map((d) => d.k), how: c.how });
    // (a day replaced takes trips out of the table: the places chosen are others' now)
    if (r?.copied) tpSet({ copying: null, sel: null, picked: [], anchor: null });
  };
  const keep = async () => {
    const r = await act('template_save', { source: sourceArg({ source: source === 'trips' ? 'trips' : 'day' }, chosen), name: c.name });
    if (r?.id != null) put({ name: '', source: r.id });
  };
  const rename = async (id, name) => {
    await act('template_rename', { id, name });
    put({ renaming: null });
  };
  const drop = async (id) => {
    if (c.armed !== id) return put({ armed: id });
    await act('template_drop', { id });
    put({ armed: null, source: source === id ? 'day' : source });
  };
  return html`<div class="tp-copy" data-rf="rf-tp-copyto">
    <div class="head"><${Icon} name="content_copy" size=${17} /><b class="cut">${tr('Copy to…')}</b><button type="button" title=${tr('Close')} onClick=${() => tpSet({ copying: null })}><${Icon} name="close" size=${16} /></button></div>
    <div class="body">
      <div class="caps">${tr('Copy').toUpperCase()}</div>
      <${Source} on=${source === 'day'} label=${tr('The whole day: %{day}', { day: shown?.label || '' })} sub=${tr('%{n} trips', { n: shown?.trips || 0 })} onPick=${() => put({ source: 'day' })} />
      ${chosen.length > 0 && html`<${Source} on=${source === 'trips'} label=${tr('The trips chosen')} sub=${tr('%{n} trips', { n: chosen.length })} onPick=${() => put({ source: 'trips' })} />`}
      ${templates.map((x) => {
        const naming = c.renaming?.id === x.id;
        return html`<${Source} on=${source === x.id} label=${naming ? '' : x.name} sub=${`${x.day_label} · ${tr('%{n} trips', { n: x.trips })}`} onPick=${() => put({ source: x.id, armed: null })}>
          ${naming && html`<input class="rename" value=${c.renaming.text} ref=${(el) => el && !el.dataset.focused && ((el.dataset.focused = '1'), el.focus())} onInput=${(e) => put({ renaming: { id: x.id, text: e.currentTarget.value } })} onKeyDown=${(e) => {
            if (e.key === 'Enter') rename(x.id, c.renaming.text);
            else if (e.key === 'Escape') put({ renaming: null });
          }} onBlur=${() => S.rf.tp.copying?.renaming?.id === x.id && rename(x.id, S.rf.tp.copying.renaming.text)} />`}
          <button type="button" class="ib" title=${tr('Rename the template')} onClick=${() => put({ renaming: { id: x.id, text: x.name }, armed: null })}><${Icon} name="edit" size=${14} /></button>
          <button type="button" class=${'ib' + (c.armed === x.id ? ' armed' : '')} title=${tr(c.armed === x.id ? 'Click again to throw the template away' : 'Throw the template away')} onClick=${() => drop(x.id)}><${Icon} name="delete" size=${14} /></button>
        <//>`;
      })}
      ${!templates.length && html`<p class="note">${tr('No templates of this line yet: keep the day or the trips chosen as one below, to copy onto any day later.')}</p>`}
      <div class="caps">${tr('Onto these days').toUpperCase()}</div>
      <div class="days">${days.map((d) => {
        let tag = tr('%{n} trips', { n: d.trips });
        let tone = 'dim';
        if (d.closed) tag = tr('The line does not run then');
        else if (own(d)) tag = tr('The day shown');
        else if (c.ticks[d.k] && c.how === 'replace' && d.trips > 0) [tag, tone] = [tr('Its %{n} trips are replaced', { n: d.trips }), 'warn'];
        return html`<${Tick} on=${!!c.ticks[d.k] && !d.closed && !own(d)} disabled=${d.closed || own(d)} label=${d.label} tag=${tag} tone=${tone} onChange=${(x) => put({ ticks: { ...c.ticks, [d.k]: x } })} />`;
      })}</div>
      <div class="tp-seg">${[['replace', 'Replace their trips'], ['add', 'Add to their trips']].map(([k, w]) => html`<button type="button" class=${c.how === k ? 'on' : ''} onClick=${() => put({ how: k })}>${tr(w)}</button>`)}</div>
      <p class="note">${c.how === 'replace' ? tr('Replace: the days ticked run these trips, with their running times, instead of their own.') : tr('Add: the trips go beside the days\' own; one there already the same way at the same minute is not copied twice.')}</p>
      <p class="note">${tr('It goes into the draft: Save keeps it, Undo takes it back. Saved and refreshed, SAVE › Apply to in Vehicles and Drivers copies the plan onto those days as well.')}</p>
      <div class="foot"><span class="grow"></span>
        <button type="button" class="tp-btn" onClick=${() => tpSet({ copying: null })}>${tr('Cancel')}</button>
        <button type="button" class="tp-btn solid" disabled=${!ticked.length || !count} data-rf="rf-tp-copyto-go" onClick=${go}>${tr('Copy')}</button>
      </div>
      <div class="caps">${tr('Keep as a template').toUpperCase()}</div>
      <p class="note">${source === 'trips' ? tr('The trips chosen, kept by name to copy onto any day later.') : tr("This day's timetable, kept by name to copy onto any day later.")}</p>
      <div class="keep">
        <input value=${c.name} placeholder=${tr('A name, such as School days')} maxlength="48" onInput=${(e) => put({ name: e.currentTarget.value })} onKeyDown=${(e) => e.key === 'Enter' && c.name.trim() && keep()} />
        <button type="button" class="tp-btn" disabled=${!c.name.trim()} onClick=${keep}><${Icon} name="bookmark_add" size=${15} />${tr('Keep')}</button>
      </div>
    </div>
  </div>`;
}

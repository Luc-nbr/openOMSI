// RosterFlow's "Copy to…" (testers, Beta 56: "a copy button to copy it to another day. that may
// work better"): the rows ticked in Vehicles or Drivers - buses' blocks or duties - copied to
// other days by themselves (`rf_act` "copy_rows", the core's `plandays::copy_rows`): the day
// switcher's days ticked, every day of this day type at a click, each every week or that date
// only. Only the trips that match there go along; the rest are listed (`days.js`' report). Ctrl+C
// keeps the rows ticked (or the row chosen), Ctrl+V on another day of the switcher puts them
// there.

import { html } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfAct, settle, say } from './state.js';
import { Icon, RfButton, CheckRow, Seg } from './ui.js';
import { AXIS } from './draw.js';

/** The day type of a date (`lines::DAY_GROUPS`): 0 Monday to Friday, 1 Saturday, 2 Sunday. */
function typeOf(date) {
  const d = new Date(`${date}T12:00:00`).getDay();
  return d === 0 ? 2 : d === 6 ? 1 : 0;
}

/** The rows copied (`targets`: the switcher's days, each `as` week, date or runs): what was not
 * copied shown as the report. */
async function copyRows(ids, targets, day = S.rf.day) {
  const r = await rfAct('copy_rows', { day, ids, targets });
  if (r?.edited) settle();
  if (r?.report) setRf({ panel: { kind: 'copied', report: r.report } });
  return r;
}

/** The picker opened from the selection bar. */
export const openCopyTo = () => setRf({ panel: { kind: 'copyto', ticks: {}, as: 'week' }, card: null, menu: null });

/** The picker under the selection bar: the other days of the switcher, every day of this day
 * type at a click, every week or that date only, and Copy. */
export function CopyTo({ gTop }) {
  const r = S.rf;
  const pn = r.panel;
  const ids = r.ticked;
  const days = (r.days || []).map((d, k) => ({ ...d, k, type: typeOf(d.date) })).filter((d) => d.k !== r.day);
  const mine = typeOf(r.days?.[r.day]?.date || '');
  const put = (patch) => setRf({ panel: { ...S.rf.panel, ...patch } });
  const ticked = days.filter((d) => pn.ticks[d.k]);
  const go = () => {
    const targets = ticked.map((d) => ({ k: d.k, as: pn.as }));
    setRf({ panel: null });
    copyRows(ids, targets).then((x) => x?.edited && setRf({ ticked: ids }));
  };
  const h = 196 + days.length * 30;
  return html`<div class="rf-over rf-copyto" style=${`left:30px;top:${gTop + AXIS + 2}px;width:360px;height:${h}px`} data-rf="rf-copyto">
    <div class="head"><${Icon} name="content_copy" size=${17} /><b class="cut">${tr('Copy %{n} rows to…', { n: ids.length })}</b><button type="button" title=${tr('Close')} onClick=${() => setRf({ panel: null })}><${Icon} name="close" size=${16} /></button></div>
    <div class="body">
      <button type="button" class="type" onClick=${() => put({ ticks: Object.fromEntries(days.map((d) => [d.k, d.type === mine])), as: 'week' })}><${Icon} name="calendar_month" size=${15} />${tr('Every day of this day type')}</button>
      <div class="days">${days.map((d) => html`<div class="day"><${CheckRow} on=${!!pn.ticks[d.k]} label=${d.label} raw onChange=${(x) => put({ ticks: { ...pn.ticks, [d.k]: x } })} />${d.type === mine && html`<span class="tag dim">${tr('Same day type')}</span>`}</div>`)}</div>
      <${Seg} at=${pn.as === 'date' ? 1 : 0} labels=${['Every week', 'This date only']} style="height:26px" onPick=${(k) => put({ as: k === 1 ? 'date' : 'week' })} />
      <p class="note">${tr('Only the trips that match there are copied (same line, way, departure and stops); the rest are listed. It goes into your draft: SAVE keeps it.')}</p>
    </div>
    <div class="foot"><span class="grow"></span>
      <${RfButton} label="Cancel" style="width:100px;height:30px" onClick=${() => setRf({ panel: null })} />
      <${RfButton} label="Copy" solid disabled=${!ticked.length} data="rf-copyto-go" style="width:110px;height:30px" onClick=${go} />
    </div>
  </div>`;
}

/** Ctrl+C: the rows ticked (or the row chosen) kept to paste on another day. */
export function copyKey() {
  const r = S.rf;
  const ids = r.ticked.length ? r.ticked : r.selRow ? [r.selRow] : [];
  if (!ids.length) return false;
  setRf({ clip: { ids, day: r.day } });
  say(tr('%{n} rows copied: open another day in the switcher and press Ctrl+V to put them there.', { n: ids.length }));
  return true;
}

/** Ctrl+V: the rows kept put on the day shown, into the plan it runs. */
export function pasteKey() {
  const r = S.rf;
  const clip = r.clip;
  if (!clip) return false;
  if (clip.day === r.day) {
    say(tr('Open another day in the switcher to put the rows there.'), true);
    return true;
  }
  copyRows(clip.ids, [{ k: r.day, as: 'runs' }], clip.day);
  return true;
}

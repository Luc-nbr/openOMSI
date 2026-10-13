// The timetable as a table over the map (`table_layer`, after City Bus Manager's): the stops of
// a direction down, its trips of a group of days across, every departure a cell to type a time
// into ("0705", the arrow keys a minute later or earlier); a trip added, copied 10 to 30
// minutes later, moved as a whole or taken out; filled again from the day patterns. A stop
// reached before the one before it is marked. Scrolled both ways, the stops' names and the
// trips' heads frozen.

import { html, useEffect, useRef, useState } from '../../../../vendor/preact-htm.js';
import { Seg } from '../../bits.js';
import { Btn, IconBtn, Toggle, Icon, tr } from '../kit.js';
import { L, le } from './state.js';
import { DirChoice } from './panels.js';

/** A time typed into the table: "7:05", "07.05", "0705" or "705" (null: not a time). */
export function parseHm(s) {
  s = String(s).trim();
  const m = s.match(/^(\d+)\s*[:.h]\s*(\d+)$/);
  let h;
  let mi;
  if (m) [h, mi] = [Number(m[1]), Number(m[2])];
  else if (/^\d{3,}$/.test(s)) [h, mi] = [Number(s.slice(0, -2)), Number(s.slice(-2))];
  else return null;
  return h >= 0 && h < 24 && mi >= 0 && mi < 60 ? h * 60 + mi : null;
}

/** A time being typed is whole: four digits, or two of minutes after ':', '.' or 'h'. */
export function wholeTime(s) {
  s = String(s).trim();
  const m = s.match(/[:.h](.*)$/);
  return m ? m[1].trim().length === 2 : s.length === 4 && /^\d+$/.test(s);
}

function Cell({ i, s, time, falls, chosen, first }) {
  const [text, setText] = useState('');
  const ref = useRef();
  useEffect(() => {
    if (chosen) {
      setText('');
      ref.current?.focus();
      ref.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    }
  }, [chosen]);
  if (!chosen) {
    return html`<button type="button" class=${'cell' + (falls ? ' falls' : '') + (first ? ' first' : '')} onClick=${() => le('t_select', { i, s })}>${time}</button>`;
  }
  return html`<input ref=${ref} class="cell typing" value=${text} placeholder=${time} spellcheck="false"
    onInput=${(e) => {
      const t = e.currentTarget.value;
      setText(t);
      const m = parseHm(t);
      if (m != null && wholeTime(t)) le('t_typed', { m, next: false });
    }}
    onKeyDown=${(e) => {
      if (e.key === 'Enter') {
        const m = parseHm(text);
        if (m != null) le('t_typed', { m, next: true });
        else le('t_select', { i, s: s + 1 });
      } else if (e.key === 'ArrowUp') (e.preventDefault(), le('t_nudge', { m: 1 }));
      else if (e.key === 'ArrowDown') (e.preventDefault(), le('t_nudge', { m: -1 }));
    }} />`;
}

export function TableLayer() {
  const t = L.view.table;
  if (!t) return null;
  const sel = t.sel;
  const fill = t.fill_armed ? 'Press again: the table anew' : 'Fill from the patterns';
  return html`<div class="le-table">
    <div class="le-table-head">
      <b>${t.title}</b>
      <span class="dir"><${DirChoice} value=${t.dir} onChange=${(d) => le('t_dir', { d })} /></span>
      <span class="days"><div class="hub-seg full">${t.days.map((name, k) => html`<button type="button" class=${(k === t.day ? 'on' : '') + (t.off.includes(k) ? ' off' : '')} disabled=${t.off.includes(k)} onClick=${() => le('t_day', { k })}>${name}</button>`)}</div></span>
      <${IconBtn} icon="close" size=${14} title="Back to the map" onClick=${() => le('t_close')} />
    </div>
    <div class="le-table-tools">
      <${Toggle} label="The table is the line's timetable" checked=${t.use} onChange=${(on) => le('t_use', { on })} />
      <span class="kind" title=${t.kind.note}><${Icon} name=${t.kind.icon} size=${15} />${t.kind.label}</span>
      <${Btn} kind="ghost" icon="restart_alt" label=${fill} onClick=${() => le('t_fill')} />
    </div>
    <div class="le-table-tools">
      <${Btn} icon="add" label="Add a trip" style="width:120px" onClick=${() => le('t_add')} />
      ${sel
        ? html`<span class="lbl">${tr('Copy')}</span>${[10, 15, 20, 30].map((m) => html`<${Btn} kind="ghost" raw label=${'+' + m} title=${tr('A copy of the trip, %{n} minutes later', { n: m })} onClick=${() => le('t_copy', { m })} />`)}
          <span class="lbl">${tr('Move')}</span>${[[-5, '-5'], [-1, '-1'], [1, '+1'], [5, '+5']].map(([m, l]) => html`<${Btn} kind="ghost" raw label=${l} title="The whole trip so many minutes earlier or later" onClick=${() => le('t_shift', { m })} />`)}
          <${IconBtn} icon="delete" size=${12} title="Take the trip out" onClick=${() => le('t_delete')} />`
        : html`<span class="hint">${tr('Choose a time in the table: type it (0705), the arrow keys a minute later or earlier.')}</span>`}
    </div>
    <div class="le-grid">
      <table>
        <thead><tr><th class="name">${t.trips.length > 6 ? `${t.trips.length}` : ''}</th>${t.trips.map((x, c) => html`<th><button type="button" class=${sel && sel[0] === x.i ? 'on' : ''} onClick=${() => le('t_select', { i: x.i, s: sel ? sel[1] : 0 })}>${tr('Trip %{n}', { n: c + 1 })}</button></th>`)}<th class="rest"></th></tr></thead>
        <tbody>${t.stops.map((name, s) => html`<tr class=${s % 2 ? 'odd' : ''}><td class="name">${name}</td>${t.trips.map((x) => {
          const cell = x.times[s];
          return html`<td>${cell && html`<${Cell} i=${x.i} s=${s} time=${cell[0]} falls=${cell[1]} first=${s === 0} chosen=${!!sel && sel[0] === x.i && sel[1] === s} />`}</td>`;
        })}<td class="rest"></td></tr>`)}</tbody>
      </table>
      ${!t.trips.length && html`<p class="le-empty">${tr(t.empty ? 'The table is empty: fill it from the patterns, or add a trip.' : 'No trip of this direction on these days: add one.')}</p>`}
    </div>
    <div class="le-table-foot"><p class=${'tone-' + t.note_tone}>${t.note}</p>${t.away && html`<p class="tone-dim">${t.away}</p>`}</div>
  </div>`;
}

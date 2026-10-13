// The line editor's right panel (`right_panel`): a direction's stops with the minutes to each,
// its displays, its timetable and its kind of service - each as the game-drawn launcher lays it
// out, what it shows answered by the line editor itself (`L.view`).

import { html } from '../../../../vendor/preact-htm.js';
import { Seg } from '../../bits.js';
import { Card, Heading, Row, Drop, Btn, IconBtn, Toggle, LiveField, Field, TimeSpin, Icon, tr, fmtTime } from '../kit.js';
import { LedSign } from '../signs.js';
import { L, le } from './state.js';
import { inkOn } from './plate.js';

/** The direction to show (`maplines::direction_choice`): Outbound and Return as two halves, a
 * line of the map's with more trips than two in a list. */
export function DirChoice({ value, onChange }) {
  const d = L.view.directions;
  if (!d.list) return html`<${Seg} full value=${value} options=${d.labels.map((l, k) => [k, l])} onChange=${onChange} />`;
  return html`<${Drop} raw value=${value} options=${d.labels.map((l, k) => [k, l])} onChange=${onChange} />`;
}

// (as they are in every language: an example and a date's form)
const TITLE_HINT = 'Shuttleverkehr Altenfeld - Wurzbach';
const MMDD = 'MM-DD';

const Caps = ({ title, style = '' }) => html`<span class="ed-caps le-caps" style=${style}>${tr(title)}</span>`;

/** The destination's field (`destination_field`): typed, or chosen from what the buses know. */
function DestField({ dest }) {
  if (!dest) return null;
  return html`<div class="le-dest">
    <${LiveField} raw icon="flag" value=${dest.typed} placeholder=${dest.last} onInput=${(t) => le('dest', { value: t })} />
    <${Drop} class="le-menu" raw value=${dest.k} options=${dest.options.map((o, k) => [k, o])} onChange=${(k) => le('dest_pick', { k })} render=${() => html`<span title=${dest.tip}></span>`} />
  </div>`;
}

// --- the Stops tab -------------------------------------------------------------------------------

function StopsTab() {
  const v = L.view;
  const t = v.stops_tab;
  const colour = v.line.colour;
  if (!t || t.none) return null;
  if (v.dir === 1 && !t.two) {
    return html`<div class="le-tab">
      <${DirChoice} value=${v.dir} onChange=${(d) => le('dir', { d })} />
      <p class="ed-para le-gap">${tr('Without a way back the line goes round: the bus starts again at the first stop.')}</p>
      <${Btn} kind="primary" icon="sync_alt" label="Reverse: make the way back" onClick=${() => le('reverse')} />
    </div>`;
  }
  const n = t.rows.length;
  return html`<div class="le-tab">
    <${DirChoice} value=${v.dir} onChange=${(d) => le('dir', { d })} />
    <${Caps} title="Destination" />
    <${DestField} dest=${t.dest} />
    <div class="le-stops">
      ${!n && html`<p class="ed-para">${tr('Click the stops on the map in the order the bus calls at them.')}</p>`}
      ${t.rows.map((name, i) => {
        const leg = t.legs[i];
        const on = v.sel_stop === i;
        return html`<${Row} class="le-stop" on=${on} onClick=${() => le('sel_stop', { i })}>
            <i class=${'dot' + (i === 0 || i + 1 === n ? ' end' : '')} style=${`background:${colour}`}></i>
            <span class="name">${name}</span>
            <span class="tools">
              <b class="time">${t.times[i] || ''}</b>
              ${i > 0 && html`<span class="minus"><${IconBtn} icon="remove" size=${10} title="A minute earlier" onClick=${() => le('shift', { i, by: -1 })} /></span><span class="plus"><${IconBtn} icon="add" size=${10} title="A minute later" onClick=${() => le('shift', { i, by: 1 })} /></span>`}
              ${on && i > 0 && html`<span class="up"><${IconBtn} icon="keyboard_arrow_up" size=${10} title="Earlier in the line" onClick=${() => le('up', { i })} /></span>`}
              ${on && i + 1 < n && html`<span class="down"><${IconBtn} icon="keyboard_arrow_down" size=${10} title="Later in the line" onClick=${() => le('down', { i })} /></span>`}
              <span class="out"><${IconBtn} icon="close" size=${10} title="Take the stop out" onClick=${() => le('remove', { i })} /></span>
            </span>
          </${Row}>
          ${leg && html`<div class="le-leg"><i style=${`background:${colour}`}></i><span class=${leg.ok ? '' : 'bad'}>${leg.text}</span>
            ${leg.vias > 0 && html`<${IconBtn} icon="restart_alt" size=${9} title="Straighten the leg (its dragged points go)" onClick=${() => le('clear_vias', { i })} />`}</div>`}`;
      })}
    </div>
    <div class="le-total"><span>${t.total}</span>
      <${IconBtn} icon="remove" size=${12} title="The whole trip a minute shorter" onClick=${() => le('total', { by: -1 })} />
      <${IconBtn} icon="add" size=${12} title="The whole trip a minute longer" onClick=${() => le('total', { by: 1 })} /></div>
    <div class="le-half"><${Btn} icon="schedule" label="Times from the roads" onClick=${() => le('times_auto')} /><${Btn} icon="sync_alt" label=${t.two ? 'Reverse again' : 'Reverse'} onClick=${() => le('reverse')} /></div>
    ${n >= 2 && html`<${Btn} kind="ghost" icon=${t.is_loop ? 'close' : 'autorenew'} label=${t.is_loop ? 'Open the loop' : 'Close the loop: back to the first stop'} onClick=${() => le('loop')} />`}
  </div>`;
}

// --- the Displays tab -----------------------------------------------------------------------------

/** The depot file of the line (`depot_choice`). */
function DepotChoice({ d }) {
  return html`<${Caps} title="Depot file" />
    <div class="le-depot"><${Drop} raw value=${d.k} options=${d.options.map((o, k) => [k, o])} onChange=${(k) => le('depot_choice', { k })} />
      ${d.open && html`<${IconBtn} icon="open_in_new" size=${13} title="Open it in the depot editor" onClick=${() => le('open_depot')} />`}</div>`;
}

function DisplaysTab() {
  const v = L.view;
  const d = v.displays;
  if (!d) return null;
  const head = html`<${Seg} full value=${d.advanced ? 1 : 0} options=${[[0, 'Simple'], [1, 'Advanced']]} onChange=${(k) => le('sign_mode', { advanced: k === 1 })} />
    <${DirChoice} value=${v.dir} onChange=${(dd) => le('dir', { d: dd })} />`;
  if (d.round) return html`<div class="le-tab">${head}<p class="ed-para le-gap">${tr('Without a way back the line goes round: the bus starts again at the first stop.')}</p></div>`;
  if (!d.advanced) {
    return html`<div class="le-tab">${head}
      <${Caps} title="Destination" style="margin-top:6px" />
      <${DestField} dest=${d.dest} />
      ${d.no_depot
        ? html`<p class="ed-para ed-warn">${d.no_depot}</p><${DepotChoice} d=${d.depot} /><p class="ed-para faint">${d.depot.hint}</p>`
        : !d.ready || !d.sign
          ? html`<p class="ed-para">${tr('Give the direction its stops first: the destination is its last one, or the one chosen with the stops.')}</p>`
          : html`<div class="le-sign"><${LedSign} h=${58} line=${d.number} l1=${d.sign.l1} l2=${d.sign.l2} white=${v.white} onClick=${() => le('white')} /></div>
            <p class="ed-para small">${d.sign.caption}</p>
            <p class="ed-para soft">${d.sign.hint}</p>
            ${d.sign.known && html`<div class="le-known"><span>${d.sign.known}</span><${Btn} kind="ghost" icon="check" label="Use it" onClick=${() => le('use_similar')} /></div>`}
            ${d.sign.shortened && html`<p class="ed-para ed-warn">${tr('Too long for the sign, so it is shortened. Advanced: write each row yourself.')}</p>`}`}
    </div>`;
  }
  const a = d.adv;
  const text = (t) => {
    const val = t.value ?? null;
    const n = (val ? val : t.shown).trimEnd().length;
    return html`<div class="le-signtext">
      <div class="top"><span>${t.title}</span>${t.max != null && html`<b class=${n > t.max ? 'over' : ''}>${n}/${t.max}</b>`}</div>
      ${val != null
        ? html`<${LiveField} raw value=${val} placeholder=${t.shown.trim() ? t.shown : '-'} onInput=${(x) => le('sign_text', { k: t.k, value: x })} />`
        : html`<div class="ro">${t.shown.trim() ? t.shown.trimEnd() : '-'}</div>`}
      <p class="help">${t.help}</p>${t.note && html`<p class="note">${t.note}</p>`}
    </div>`;
  };
  return html`<div class="le-tab">${head}
    <div class="le-scroll">
      <${DepotChoice} d=${d.depot} /><p class="ed-para faint">${d.depot.hint}</p>
      ${d.no_depot
        ? html`<p class="ed-para ed-warn">${d.no_depot}</p>`
        : !d.ready || !a
          ? html`<p class="ed-para">${tr('Give the direction its stops first: the destination is its last one, or the one chosen with the stops.')}</p>`
          : html`<div class="le-head2"><${Caps} title="Display texts" />${a.can_reset && html`<${IconBtn} icon="restart_alt" size=${10} title="The texts made from the destination again" onClick=${() => le('sign_reset')} />`}</div>
            <p class="ed-para small">${a.intro}</p>
            ${a.main.map(text)}
            ${a.more > 0 && html`<${Btn} kind="ghost" icon=${a.more_open ? 'expand_less' : 'expand_more'} raw label=${a.more_label} onClick=${() => le('more_texts')} />`}
            ${a.more_open && a.other.map(text)}
            <${Caps} title="IBIS route" />
            <p class="ed-para help">${tr('The number the driver types into the IBIS for this direction: the line\'s number and two digits. Empty: one is given when the line is saved.')}</p>
            <div class="le-route"><${LiveField} raw value=${a.route} placeholder=${a.route_hint} onInput=${(x) => le('route', { value: x })} /><span class=${'tone-' + a.route_tone}>${a.route_note}</span></div>
            <${Caps} title="Stop names on the IBIS" />
            <p class="ed-para help">${tr('What the IBIS and the passenger displays call each stop. Empty: the depot file\'s name for it (grey).')}</p>
            <${Btn} kind="ghost" icon=${a.stops_open ? 'expand_less' : 'expand_more'} raw label=${a.stops_label} onClick=${() => le('ibis_open')} />
            ${a.stops_open && a.stops.map((st, i) => html`<div class="le-ibisstop"><span>${st.name}</span><${LiveField} raw value=${st.value} placeholder=${st.def} onInput=${(x) => le('ibis_stop', { i, value: x })} /></div>`)}
            <${Caps} title="My destinations" />
            <p class="ed-para help">${tr('Names for the buses to know besides the depot file\'s: each one kept is in the destination list of every line of this map.')}</p>
            ${!a.is_mine && html`<${Btn} kind="ghost" icon="save" label="Keep this destination as mine" onClick=${() => le('keep_this')} />`}
            ${a.mine.map((m, k) => html`<div class="le-mine"><span>${m.name}</span>${m.code > 0 && html`<b>${m.code}</b>`}<${IconBtn} icon="close" size=${10} title="Take it out of my destinations" onClick=${() => le('drop_mine', { k })} /></div>`)}
            <${NewMine} />
            <p class="ed-para faint">${a.where_kept}</p>`}
    </div>
  </div>`;
}

/** A destination of the player's own typed, kept. */
function NewMine() {
  const keepIt = () => {
    const name = (L.newMine || '').trim();
    if (!name) return;
    L.newMine = '';
    le('keep_new', { name });
  };
  return html`<div class="le-newmine"><${Field} value=${L.newMine || ''} placeholder="A destination of my own" onInput=${(t) => (L.newMine = t)} onEnter=${keepIt} /><${Btn} icon="add" label="Keep" onClick=${keepIt} /></div>`;
}

// --- the Timetable tab -----------------------------------------------------------------------------

/** A slider (`Ui::slider`): its name, the line, its value. */
function Slide({ label, value, min, max, step = 1, onChange, unit = 'min', raw }) {
  const fill = ((value - min) / (max - min || 1)) * 100;
  return html`<div class="le-slider"><span>${raw ? label : tr(label)}</span>
    <input type="range" min=${min} max=${max} step=${step} value=${value} style=${`--fill:${fill}%`} onChange=${(e) => onChange(Number(e.currentTarget.value))} />
    <b>${Math.round(value)} ${unit}</b></div>`;
}

function TimetableTab() {
  const v = L.view;
  const t = v.timetable;
  if (!t) return null;
  if (t.timeplan) {
    const p = t.timeplan;
    return html`<div class="le-tab le-timeplan">
      <div class="note"><${Icon} name="departure_board" size=${20} /><b>${p.text}</b></div>
      ${!p.runs
        ? html`<p class="ed-para">${tr('Confirm the route here: the line runs no tour until its timetable is saved there.')}</p>`
        : html`${p.none_yet && html`<b class="ed-warn none">${tr('No timetable yet')}</b>`}<${Btn} kind="primary" icon="departure_board" label="Open in the Timeplan" onClick=${() => le('timeplan')} />`}
    </div>`;
  }
  const pat = (k, what, extra = {}) => le('pattern', { k, what, ...extra });
  return html`<div class="le-tab">
    <div class="le-half"><${Toggle} label="As a table" checked=${t.table_on} onChange=${(on) => le('table_on', { on })} /><${Btn} icon="grid_view" label="Open the table" onClick=${() => le('table_open')} /></div>
    ${t.table_on && html`<p class="ed-para small accent">${tr(t.of_map ? "The table holds the map's departures, each in the tour the map runs it in: change them there. A change to the patterns below makes that group of days anew from them." : 'The table is the line\'s timetable: a change to the patterns below fills that group of days anew at once; the times typed into the other groups stay.')}</p>`}
    ${t.two && html`<${DirChoice} value=${v.dir} onChange=${(d) => le('dir', { d })} />`}
    ${t.total != null && html`<${Slide} label=${tr('Trip takes')} raw value=${t.total} min=${1} max=${Math.max(240, t.total)} onChange=${(x) => le('run_total', { d: t.run_dir, t: x })} />`}
    ${t.two && t.pdir === 1
      ? html`<${Toggle} label="Leaves as the outbound does" checked=${!t.back_own} onChange=${(on) => le('follow', { on })} />
          ${!t.back_own && html`<p class="ed-para">${tr('The return leaves at the outbound\'s times. Switch this off to give it departures of its own.')}</p>`}`
      : t.two && t.back_own && html`<p class="ed-para small">${tr('The return has departures of its own: choose Return above to change them.')}</p>`}
    ${!(t.two && t.pdir === 1 && !t.back_own) && html`<div class="le-scroll le-days">
      ${t.groups.map((g, k) => {
        if (!g.allowed) return html`<div class="le-day off"><span>${g.name}</span><small>${g.why}</small></div><i class="le-rule"></i>`;
        const p = g.p;
        return html`<${Toggle} raw label=${g.name} checked=${p.on} onChange=${(on) => pat(k, 'on', { on })} />
          ${p.on && html`<${Seg} full value=${p.bands.length ? 1 : 0} options=${[[0, 'One headway'], [1, 'Time bands']]} onChange=${(m) => pat(k, 'mode', { bands: m === 1 })} />
            ${!p.bands.length
              ? html`<div class="le-two"><span class="le-lab">${tr('First')}</span><span class="le-lab">${tr('Last')}</span>
                  <${TimeSpin} minutes=${p.first} onChange=${(m) => pat(k, 'first', { value: m })} /><${TimeSpin} minutes=${p.last} onChange=${(m) => pat(k, 'last', { value: m })} /></div>
                <${Slide} label="Every" value=${p.headway} min=${5} max=${120} step=${5} onChange=${(x) => pat(k, 'every', { value: x })} />`
              : html`${p.bands.map((b, i) => html`<div class="le-band"><${TimeSpin} h=${32} minutes=${b.from} onChange=${(m) => pat(k, 'band_from', { i, value: m })} /><span>–</span><${TimeSpin} h=${32} end minutes=${b.to} onChange=${(m) => pat(k, 'band_to', { i, value: m })} /><${IconBtn} icon="close" size=${10} title="Take the band out" onClick=${() => pat(k, 'band_remove', { i })} /></div>
                  <div class=${'le-bandrow' + (t.for_company ? ' sized' : '')}><${Slide} label="Every" value=${b.headway} min=${5} max=${60} step=${5} onChange=${(x) => pat(k, 'band_every', { i, value: x })} />
                    ${t.for_company && html`<${Drop} raw value=${JSON.stringify(b.size ?? null)} options=${t.sizes.map(([sz, l]) => [JSON.stringify(sz ?? null), l])} onChange=${(sz) => pat(k, 'band_size', { i, size: JSON.parse(sz) })} />`}</div>`)}
                <${Btn} kind="ghost" icon="add" label="Add a band" onClick=${() => pat(k, 'band_add')} />`}
            <${Slide} label="Stands" value=${p.layover} min=${0} max=${30} onChange=${(x) => pat(k, 'layover', { value: x })} />
            <p class="ed-para small">${g.counts}</p>`}
          <i class="le-rule"></i>`;
      })}
      <p class="ed-para faint">${tr('Each group of days becomes tours of its own; the buses go back and forth, each standing at the end for at least as long as set. With time bands the buses added for the rush hours go back to the depot after them.')}</p>
    </div>`}
  </div>`;
}

// --- the Kind tab -----------------------------------------------------------------------------------

function KindTab() {
  const v = L.view;
  const k = v.service;
  if (!k) return null;
  const sc = k.school;
  const setPeriods = (periods) => le('holidays', { periods });
  return html`<div class="le-tab"><div class="le-scroll">
    <${Caps} title="Public title" />
    <${LiveField} raw value=${k.title} placeholder=${TITLE_HINT} onInput=${(t) => le('title', { value: t })} />
    <${Caps} title="Kind of service" />
    ${k.kinds.map((x) => html`<${Row} class="le-kind" on=${x.on} onClick=${() => !x.on && le('kind', { kind: x.id })}><${Icon} name=${x.icon} size=${17} /><span>${x.label}</span></${Row}>`)}
    <p class="ed-para small">${k.note}</p>
    ${!k.route_only && html`<${Btn} kind="ghost" icon="schedule" label="Typical timetable for this kind" onClick=${() => le('typical')} />`}
    ${sc && html`<i class="le-rule"></i><${Caps} title="School holidays" />
      ${sc.map_school
        ? html`<p class="ed-para small">${tr("From the map's calendar (Holidays.txt), as the game keeps them.")}</p>`
        : html`<p class="ed-para small">${tr("This map's calendar names no school holidays: these hold for your lines here (the game itself runs every working day as a school day).")}</p>
          ${sc.periods.map((p, i) => html`<div class=${'le-hol' + (p.valid ? '' : ' bad')}>
            <${LiveField} raw value=${p.name} placeholder=${tr('Name')} onInput=${(x) => setPeriods(sc.periods.map((q, j) => (j === i ? { ...q, name: x } : q)))} />
            <${LiveField} raw value=${p.from} placeholder=${MMDD} onInput=${(x) => setPeriods(sc.periods.map((q, j) => (j === i ? { ...q, from: x } : q)))} />
            <${LiveField} raw value=${p.to} placeholder=${MMDD} onInput=${(x) => setPeriods(sc.periods.map((q, j) => (j === i ? { ...q, to: x } : q)))} />
            <${IconBtn} icon="close" size=${10} title="Take the period out" onClick=${() => setPeriods(sc.periods.filter((_, j) => j !== i))} /></div>`)}
          <div class="le-half"><${Btn} kind="ghost" icon="add" label="Add a period" onClick=${() => setPeriods([...sc.periods, { name: '', from: '', to: '' }])} /><${Btn} kind="ghost" icon="restart_alt" label="Default holidays" onClick=${() => setPeriods([])} /></div>`}
      ${sc.next && html`<p class="ed-para soft">${sc.next}</p>`}`}
    <i class="le-rule"></i>
    <${Caps} title="Buses on this line" />
    <div class="le-classes">${k.classes.map((c) => html`<button type="button" class=${'le-class' + (c.on ? ' on' : '') + (c.shut ? ' shut' : '')} title=${c.shut ? tr("Your company's level does not open these buses yet") : undefined} onClick=${() => le('class', { class: c.id })}>${c.shut && html`<${Icon} name="lock" size=${12} />`}${c.label}</button>`)}</div>
    ${k.suits && html`<div class="le-suits"><p class="ed-para small">${k.suits}</p><${Btn} kind="ghost" label="Take these" onClick=${() => le('take_suits')} /></div>`}
    ${k.buses.map((b, i) => html`<div class="le-bus"><${Icon} name="directions_bus" size=${15} /><span>${b}</span><${IconBtn} icon="close" size=${10} title="Take it out" onClick=${() => le('bus_remove', { i })} /></div>`)}
    <${Drop} raw value=${k.pick[0]} options=${k.makers.map((m, i) => [i, m])} onChange=${(m) => le('pick', { m, k: k.pick[1], j: k.pick[2] })} />
    ${k.models && html`<${Drop} raw value=${k.pick[1]} options=${k.models.map((m, i) => [i, m])} onChange=${(kk) => le('pick', { m: k.pick[0], k: kk, j: k.pick[2] })} />`}
    ${k.versions && html`<${Drop} raw value=${k.pick[2]} options=${k.versions.map((m, i) => [i, m])} onChange=${(j) => le('pick', { m: k.pick[0], k: k.pick[1], j })} />`}
    ${k.models && html`<${Btn} icon="add" label="Add to the line" onClick=${() => le('pick_add')} />`}
    <p class=${'ed-para small' + (k.fit_warn ? ' ed-warn' : '')}>${k.fit}</p>
    ${k.fleet && html`<p class=${'ed-para small' + (k.fleet[1] ? ' ed-warn' : ' soft')}>${k.fleet[0]}</p>`}
  </div></div>`;
}

/** The right panel: no line - a word; a line - its four tabs. */
export function RightPanel() {
  const v = L.view;
  if (!v.line) {
    return html`<${Card} class="le-right"><${Heading} title="The line" /><p class="ed-para">${tr('Choose one of your lines, or make a new one.')}</p></${Card}>`;
  }
  const Tab = [StopsTab, DisplaysTab, TimetableTab, KindTab][v.tab] || StopsTab;
  return html`<${Card} class="le-right">
    <${Seg} full value=${v.tab} options=${[[0, 'Stops'], [1, 'Displays'], [2, 'Timetable'], [3, 'Kind']]} onChange=${(t) => le('tab', { t })} />
    <${Tab} />
  </${Card}>`;
}

export { inkOn, fmtTime };

// The line editor (`crates/omsi-launcher-ui/src/lineeditor.rs`): a line of the player's own on
// any map. The map fills the middle of the page with every bus stop on it; the player clicks
// the stops in order and the way between each two is found over the roads - kerb side, lane
// changes, loose ends, as the game's buses will drive it. "Reverse" makes the way back from the
// stops across the road. A leg is dragged through a point of the player's choosing; a stop is
// put in after the one chosen in the list, moved up or down, or taken out there.
//
// On the left the player's lines of the map (and the map's own lines) and the line's own
// settings; on the right a direction's stops, its displays, its timetable and its kind of
// service. Working for the bus company: its map, its lines, the estimate over the map and
// "Confirm and pay".
//
// The line editor itself is kept by the game program (`lineeditor::headless`, `hub_le`): every
// change is a request of it, and this page draws what it answers.

import { html, useEffect } from '../../../../vendor/preact-htm.js';
import { S, useStore } from '../../../store.js';
import { Card, Heading, Row, Badge, Drop, Btn, IconBtn, Toggle, LiveField, Icon, tr, mapLabel, status } from '../kit.js';
import { L, le } from './state.js';
import { RightPanel, DirChoice } from './panels.js';
import { LineMap } from './map.js';
import { TableLayer } from './table.js';
import { EstimateCard, ChangeDialog } from './company.js';
import { inkOn } from './plate.js';

export { L };

/** The line's plate: its number on the line's yellow (or on its own colour, `ownlines::plate`). */
export function Plate({ number, colour, h = 22 }) {
  const style = colour ? `background:${colour};color:${inkOn(colour)}` : '';
  return html`<span class="le-plate" style=${`height:${h}px;min-width:${h * 1.8}px;font-size:${(h * 0.6) / 1.303}px;${style}`}>${number}</span>`;
}

/** The left panel (`left_panel`): the map (the company's, working for one), the lines - the
 * player's own, then the map's - a new line, and the line's own settings with its foot. */
function LeftPanel() {
  const v = L.view;
  const line = v.line;
  return html`<${Card} class="le-left">
    <${Heading} title="Your lines" />
    ${v.company
      ? html`<div class="le-company"><${Icon} name="garage" size=${17} /><span>${v.company.text}</span></div>`
      : html`<${Drop} raw value=${v.map} options=${v.maps.map((m, k) => [k, m])} onChange=${(k) => le('pick_map', { k })} />`}
    <div class="le-lines" style=${`max-height:${v.map_rows.length ? 31 : 28}%`}>
      ${!v.rows.length && html`<p class="le-none">${v.none}</p>`}
      ${v.rows.map((r) => html`<${Row} class="le-line" on=${r.id === v.sel} onClick=${() => le('pick_line', { id: r.id })}>
        <${Plate} number=${r.number} /><i class="bar" style=${`background:${r.colour}`}></i><span class="name">${r.name}</span>
        ${r.kind && html`<span class="kind" title=${r.kind.label}><${Icon} name=${r.kind.icon} size=${15} /></span>`}
        ${r.draft && html`<${Badge} text="draft" kind="warn" />`}
      </${Row}>`)}
      ${v.map_rows.length > 0 && html`<div class="le-maphead"><span>${tr('Lines of the map')}</span><b>${v.map_rows.length}</b></div>
        ${v.map_rows.map((r) => html`<${Row} class="le-line map" on=${!!v.sel_ttl && v.sel_ttl.toLowerCase() === r.ttl.toLowerCase()} onClick=${() => le('pick_map_line', { ttl: r.ttl })}>
          <${Plate} number=${r.number} colour=${r.colour} /><span class="name">${r.label}</span>${r.edited && html`<${Badge} text="edited" />`}
        </${Row}>`)}`}
    </div>
    <${Btn} icon="add" label="New line" onClick=${() => le('new_line')} />
    ${line && html`<${Settings} />`}
  </${Card}>`;
}

/** The line's own settings (`left_panel`): number and name, its colour, the depot whose buses
 * drive it, the company's live displays; and the foot. */
function Settings() {
  const v = L.view;
  const line = v.line;
  const own = !v.palette.some((h) => h.toLowerCase() === line.colour.toLowerCase());
  return html`<div class="le-settings">
    <div class="le-two"><span class="ed-caps">${tr('Number')}</span><span class="ed-caps">${tr('Name')}</span>
      <${LiveField} raw value=${line.number} placeholder="42" onInput=${(t) => le('number', { value: t })} />
      <${LiveField} value=${line.name} placeholder="Name" onInput=${(t) => le('name', { value: t })} /></div>
    <div class="le-swatches">
      ${v.palette.map((hex) => html`<button type="button" class=${'sw' + (line.colour.toLowerCase() === hex.toLowerCase() ? ' on' : '')} style=${`background:${hex}`} onClick=${() => line.colour.toLowerCase() !== hex.toLowerCase() && le('colour', { value: hex })}></button>`)}
      <label class=${'sw any' + (own ? ' own on' : '') + (v.colour_open ? ' on' : '')} style=${own ? `background:${line.colour};color:${inkOn(line.colour)}` : ''} title=${tr('Any colour')}>
        <${Icon} name="palette" size=${16} /><input type="color" value=${line.colour} onInput=${(e) => le('colour', { value: e.currentTarget.value })} />
      </label>
    </div>
    <span class="ed-caps">${tr('Driven by the buses of')}</span>
    ${v.groups.length
      ? html`<${Drop} raw value=${v.groups.find((g) => g.toLowerCase() === line.ai_group.toLowerCase()) ?? v.groups[0]} options=${v.groups.map((g) => [g, g])} onChange=${(g) => le('ai_group', { value: g })} />`
      : html`<${LiveField} raw value=${line.ai_group} placeholder=${tr('Depot group (ailists.cfg)')} onInput=${(t) => le('ai_group', { value: t })} />
          <p class="ed-para ed-warn">${tr("This map's ailists.cfg has no depot group with buses: the timetable's buses cannot drive the line (you can).")}</p>`}
    ${v.company_view?.live_label && html`<${Toggle} raw label=${v.company_view.live_label} checked=${line.live_displays} onChange=${(on) => le('live_displays', { on })} />`}
    <span class="grow"></span>
    <${Foot} />
  </div>`;
}

/** The left panel's foot: save, delete and drive a line of the player's (`left_panel`), a line of
 * the map's (`maplines::foot`), a company's (`company_foot`). */
function Foot() {
  const v = L.view;
  const line = v.line;
  if (line.map) {
    const company = !!v.company;
    const fee = v.company_view?.map_fee;
    const why = v.map_problems[0] || fee?.why;
    const dirty = v.dirty && !v.fresh;
    const label = fee && fee.fee > 0 && dirty ? tr('Pay %{amount} and save', { amount: fee.label }) : dirty ? tr('Save line') : tr('Saved');
    return html`<div class="le-foot">
      ${why && html`<p class="ed-para ed-warn">${why}</p>`}
      <div class="le-save-row">
        <${Btn} kind=${dirty && !why ? 'primary' : ''} icon="save" raw label=${label} onClick=${() => dirty && (why ? status(why, true) : le('save_map'))} />
        ${!company && html`<${IconBtn} icon="play_arrow" size=${15} title="Drive it" onClick=${() => le('drive')} />`}
      </div>
      ${!v.fresh && html`<${Btn} icon="restart_alt" title="Your edit goes: the game runs the map's own line again, exactly as the map has it" label=${v.restore_armed ? "Press again: the map's original back" : "Restore the map's original"} onClick=${() => le('restore')} />`}
    </div>`;
  }
  if (v.company) {
    const f = v.company_view?.foot;
    if (!f || f.reading) return html`<p class="ed-para">${f?.reading || ''}</p>`;
    if (!f.confirmed) {
      return html`<div class="le-foot">
        ${f.why && html`<p class="ed-para ed-warn">${f.why}</p>`}
        <${Btn} kind=${f.why ? '' : 'primary'} icon="check_circle" raw label=${f.confirm} onClick=${() => (f.why ? status(f.why, true) : le('c_confirm'))} />
        <div class="le-half"><${Btn} icon="save" label=${v.dirty ? 'Save draft' : 'Saved'} onClick=${() => v.dirty && le('save')} /><${Btn} icon="delete" label=${v.delete_armed ? 'Press again' : 'Delete draft'} onClick=${() => le('delete')} /></div>
      </div>`;
    }
    return html`<div class="le-foot">
      ${f.why && html`<p class="ed-para ed-warn">${f.why}</p>`}
      ${!v.dirty && f.waiting && !f.why && html`<p class="le-waiting">${f.waiting}</p>`}
      <${Btn} kind=${v.dirty && !f.why ? 'primary' : ''} icon="save" raw label=${f.save_change} onClick=${() => v.dirty && (f.why ? status(f.why, true) : le('c_save_change'))} />
      ${v.dirty
        ? html`<${Btn} icon="restart_alt" label="Undo the changes" onClick=${() => le('c_discard')} />`
        : f.waiting
          ? html`<${Btn} icon="restart_alt" label="Keep the line as it runs" onClick=${() => le('c_withdraw')} />`
          : html`<p class="ed-para">${tr("The company runs this line. A changed route is approved anew; a change of a line in service begins on a day you choose. The line is given up on the company's Lines page.")}</p>`}
    </div>`;
  }
  return html`<div class="le-foot">
    ${v.problems[0] && html`<p class="ed-para ed-warn">${v.problems[0]}</p>`}
    <${Btn} kind=${v.dirty ? 'primary' : ''} icon="save" label=${v.dirty ? 'Save line' : 'Saved'} onClick=${() => v.dirty && le('save')} />
    <div class="le-half"><${Btn} icon="delete" label=${v.delete_armed ? 'Press again' : 'Delete line'} onClick=${() => le('delete')} /><${Btn} icon="play_arrow" label="Drive it" onClick=${() => le('drive')} /></div>
  </div>`;
}

/** A window too narrow for the map and both panels (`narrow`): the lines of the map, to read. */
function Narrow() {
  const v = L.view;
  return html`<div class="le-narrow"><p class="ed-para">${tr('The line editor needs a wider window: open it on a computer. Your lines of this map:')}</p>
    ${v.rows.map((r) => html`<div class="le-narrow-row"><${Plate} number=${r.number} h=${24} /><span>${r.name}</span></div>`)}</div>`;
}

/** The page opened: the editor's map (the Drive page's, or the company's), what to show. */
function opening() {
  const o = S.ed.lineOpen || {};
  const maps = (S.maps || []).map((m) => [mapLabel(m), m.file]);
  L.hooks = { timeplan: o.onTimeplan, companyChanged: o.onCompanyChanged };
  le('open', { maps, root: S.boot?.root || '', date: S.choice?.date || '', choice_map: S.choice?.map || '', company: o.company || '', line: o.line ?? null, new_days: o.newWithDays || null, map_line: o.mapLine || '' });
}

export function LinesPage() {
  const s = useStore();
  const o = s.ed.lineOpen;
  useEffect(() => {
    if (!s.maps?.length) return;
    opening();
    return () => le('leave');
  }, [o?.at, s.maps?.length]);
  if (!s.maps?.length) return html`<p class="ed-para">${tr('No maps found.')}</p>`;
  const v = L.view;
  if (!v) return html`<p class="ed-para">${tr('Loading...')}</p>`;
  return html`<div class="le-page">
    <div class="le-narrow-only"><${Narrow} /></div>
    <div class="le-wide">
      <${LeftPanel} />
      <div class="le-mapbox">
        <${LineMap} />
        ${v.table && html`<${TableLayer} />`}
        ${!v.table && html`<${EstimateCard} />`}
      </div>
      <${RightPanel} />
    </div>
    <${ChangeDialog} />
  </div>`;
}

/** Where Back goes (`flow::page`): the company, working for one; else the editor. */
export const linesBack = () => {
  const o = S.ed.lineOpen || {};
  if (L.view?.company || o.company) return o.back || 'drive';
  return 'editor';
};

export { DirChoice };

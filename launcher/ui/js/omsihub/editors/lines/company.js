// The line editor working for the bus company: the estimate over the map's foot (`estimate_card`:
// what the line costs once and brings a month, its passengers and its buses; opened, every item
// and the parts of a working day) and the question before a change of a line in service is
// saved (`change_dialog`: from which day, what it costs, the tours it touches).

import { html } from '../../../../vendor/preact-htm.js';
import { Btn, IconBtn, Icon, tr } from '../kit.js';
import { L, le } from './state.js';

export function EstimateCard() {
  const e = L.view?.company_view?.estimate;
  if (!e) return null;
  return html`<div class="le-estimate">
    ${e.details && html`<div class="le-est-details">
      <div class="col"><span class="ed-caps">${e.heads[0]}</span>${e.once.map(([t, a, neg]) => html`<div class="item"><span>${t}</span><b class=${neg ? 'neg' : ''}>${a}</b></div>`)}
        <i class="rule"></i><div class="item strong"><span>${e.together[0]}</span><b>${e.together[1]}</b></div></div>
      <div class="col"><span class="ed-caps">${e.heads[1]}</span>${e.monthly.map(([t, a, neg]) => html`<div class="item"><span>${t}</span><b class=${neg ? 'neg' : ''}>${a}</b></div>`)}
        <i class="rule"></i><div class="item strong"><span>${e.result[0]}</span><b>${e.result[1]}</b></div></div>
      <div class="col"><span class="ed-caps">${e.heads[2]}</span>${e.bands.map((b) => html`<div class="band"><span>${b.label}</span><b>${b.passengers}</b></div><div class=${'bnote tone-' + b.tone}>${b.note}</div>`)}</div>
      <p class="foot">${e.foot}</p>
    </div>`}
    ${e.priced && !e.details && html`<div class="le-est-note">${e.priced}</div>`}
    <div class="le-est-strip">
      ${e.strip.map((f) => html`<div class="fig"><span class="ed-caps">${f.label}</span><b class=${'tone-' + f.tone}>${f.value}</b><small>${f.under}</small></div>`)}
      <${IconBtn} icon=${e.details ? 'expand_more' : 'expand_less'} size=${13} title="The costs in detail" onClick=${() => le('c_details')} />
    </div>
  </div>`;
}

export function ChangeDialog() {
  const c = L.view?.company_view?.change;
  if (!c) return null;
  return html`<div class="ed-dialog-scrim le-change" onMouseDown=${(e) => e.target === e.currentTarget && le('c_change_cancel')}>
    <div class="ed-dialog">
      <h2><${Icon} name="route" size=${20} />${c.title}</h2>
      <p class="body">${tr("The line runs: today's tours stay as they are. The new timetable begins on the day you choose; the tours it changes get their buses and drivers anew then.")}</p>
      <div class="row"><span>${tr('Route approved anew')}</span><b>${c.fee}</b></div>
      <div class="row"><span>${tr('Tours')}</span><b>${c.tours}</b></div>
      <div class="row"><span>${tr('On the planning')}</span><b class=${c.replan_warn ? 'tone-warn' : ''}>${c.replan}</b></div>
      <div class="from"><span>${tr('From')}</span>
        <${IconBtn} icon="chevron_left" size=${14} title="A day earlier" disabled=${!c.earlier} onClick=${() => le('c_change_day', { d: -1 })} />
        <b>${c.when}</b>
        <${IconBtn} icon="chevron_right" size=${14} title="A day later" disabled=${!c.later} onClick=${() => le('c_change_day', { d: 1 })} />
      </div>
      <div class="buttons"><${Btn} label="Cancel" onClick=${() => le('c_change_cancel')} /><${Btn} kind="primary" icon="check_circle" raw label=${c.save} onClick=${() => le('c_change_save')} /></div>
    </div>
  </div>`;
}

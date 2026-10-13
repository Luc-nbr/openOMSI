// Where a free drive starts (`freedrive`): one of the map's entry points - the map's first
// unless another is chosen, in the list or on the map beside it. The traffic and the
// timetable's buses drive around the bus; nothing is booked.

import { html } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { choose, curMap } from '../../data.js';
import { Actions, Caps, Foot, Head, Radio, tr } from '../bits.js';
import { goStep, nextOf, previousOf } from '../flow.js';
import { MapBehind } from './mapbehind.js';

export function StartStep() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const at = c.free_entry ?? -1;
  const entries = m?.entry_points || [];
  const chosen = at >= 0 ? entries[at]?.name : null;
  // (a start point chosen: no more the line editor's line to drive along)
  const pick = (i) => choose({ free_entry: i, own_line: false });
  return html`
    <${MapBehind} trip=${null} entry=${at} onEntry=${pick} />
    <section class="hub-sheet narrow">
      <${Head} icon="location_on" title="Start point" sub=${chosen || tr("The map's first entry point")} />
      <div class="hub-body"><div class="hub-pad">
        <p style="color:var(--hub-text-dim);line-height:1.5;margin-bottom:16px">${tr("On a free drive you choose where the bus starts: at a stop where trips begin, at one on the way, or at one of the map's entry points. The traffic and the timetable's buses drive around you.")}</p>
        <${Caps} title="Entry points" value=${String(entries.length)} />
        <div class="hub-rows" style="padding:0">
          <button type="button" class=${'hub-rowl' + (at < 0 ? ' on' : '')} style="grid-template-columns:20px 1fr" onClick=${() => pick(-1)}><${Radio} on=${at < 0} /><span>${tr("The map's first entry point")}</span></button>
          ${entries.map((e, i) => html`<button type="button" class=${'hub-rowl' + (at === i ? ' on' : '')} style="grid-template-columns:20px 1fr" onClick=${() => pick(i)}><${Radio} on=${at === i} /><span class="ellipsis">${e.name}</span></button>`)}
        </div>
      </div></div>
      <${Foot} text=${tr('Click an entry point on the map to start there.')} />
    </section>
    <${Actions} back onBack=${() => goStep(previousOf('start'))} from=${'calc(var(--hub-edge-in) + clamp(288px, 27.4cqw, 420px) + 14px)'} main="Next step" onMain=${() => goStep(nextOf('start'))} />
  `;
}

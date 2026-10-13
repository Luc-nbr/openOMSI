// Where (`mapchoice`): the maps as tiles with their pictures - or as a list - with how many
// tours their timetable has and the year they play in, searched by name; where the bus starts
// at the sheet's foot.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../../store.js';
import { call } from '../../api.js';
import { choose, curMap } from '../../data.js';
import { mapPicture } from '../../pages/drive.js';
import { Picture, Search, Select } from '../../ui.js';
import { Actions, Foot, Head, Icon, Seg, tr } from '../bits.js';
import { go } from '../hub.js';
import { goStep, nextOf, previousOf } from '../flow.js';

const name = (m) => m.friendly || m.name;

function readView() {
  try {
    return localStorage.getItem('hub-map-view') || 'tiles';
  } catch {
    return 'tiles';
  }
}

/** The maps' tours and years, read once for the list of maps (`hub_map_facts`). */
let factsFor = '';
export function useFacts() {
  const s = useStore();
  useEffect(() => {
    const files = (s.maps || []).map((m) => m.file);
    const key = files.join('|');
    if (!files.length || key === factsFor) return;
    factsFor = key;
    call('hub_map_facts', { maps: files })
      .then((f) => set({ hubFacts: f }))
      .catch(() => {});
  }, [s.maps]);
  return s.hubFacts || {};
}

/** "212 tours · 2022", as a tile and the sheet's head say it. */
export function caption(f) {
  if (!f) return '…';
  const tours = f.tours === 0 ? tr('No timetable') : f.tours === 1 ? tr('1 tour') : tr('%{n} tours', { n: f.tours });
  return f.year ? `${tours} · ${f.year}` : tours;
}

/** Where the bus starts: one of the map's entry points, or the one nearest the duty's first
 * stop (`drive::entry_select`). */
export function EntrySelect() {
  const s = useStore();
  const m = curMap();
  if (!m) return null;
  const options = [[-1, tr('Automatic (nearest to the first stop)')], ...m.entry_points.map((e, i) => [i, e.name])];
  return html`<span style="display:flex;align-items:center;gap:12px;min-width:0;flex:1;justify-content:flex-end">
    <span style="color:var(--hub-text-soft)">${tr('Start at')}</span>
    <span style="flex:1;min-width:0;width:290px;max-width:290px"><${Select} raw value=${s.choice.entry} options=${options} onChange=${(v) => choose({ entry: Number(v) })} /></span>
  </span>`;
}

export function MapStep() {
  const s = useStore();
  const facts = useFacts();
  const [view, setView] = useState(readView);
  const [q, setQ] = useState('');
  const c = s.choice;
  const m = curMap();
  const ql = q.trim().toLowerCase();
  const maps = (s.maps || []).filter((x) => !ql || name(x).toLowerCase().includes(ql) || x.file.toLowerCase().includes(ql)).sort((a, b) => name(a).localeCompare(name(b)));
  const pick = (file) => file !== c.map && choose({ map: file, entry: -1, start_trip: null, free_entry: -1 });
  const keepView = (v) => {
    setView(v);
    try {
      localStorage.setItem('hub-map-view', v);
    } catch {
      /* this session only */
    }
  };
  const sub = m ? (facts[m.file] ? `${name(m)} · ${caption(facts[m.file])}` : name(m)) : tr('Where are you driving today?');
  const next = () => {
    if (!m) return toast(tr('Choose a map first.'), 'error');
    goStep(nextOf('map'));
  };
  const body = !s.maps
    ? html`<div class="empty"><span class="spinner"></span><b>${tr('Reading the OMSI folder…')}</b></div>`
    : !maps.length
      ? html`<div class="empty"><${Icon} name="map" size=${34} /><b>${tr('No maps found.')}</b></div>`
      : view === 'tiles'
        ? html`<div class="hub-tiles">${maps.map((x) => html`<button type="button" class=${'hub-tile' + (x.file === c.map ? ' on' : '')} onClick=${() => pick(x.file)} onDblClick=${() => (pick(x.file), goStep(nextOf('map')))}>
            <${Picture} src=${mapPicture(x.file)} />
            <span class="cap"><b>${name(x)}</b><span><em>${caption(facts[x.file])}</em><${Icon} name="location_on" size=${13} />${x.entry_points.length}</span></span>
          </button>`)}</div>`
        : html`<div class="hub-heads" style="grid-template-columns:minmax(0,1fr) 120px 80px 60px"><b>${tr('Map')}</b><b>${tr('Tours')}</b><b>${tr('Year')}</b><b></b></div>
          <div class="hub-rows">${maps.map((x) => html`<button type="button" class=${'hub-rowl' + (x.file === c.map ? ' on' : '')} style="grid-template-columns:minmax(0,1fr) 120px 80px 60px" onClick=${() => pick(x.file)} onDblClick=${() => (pick(x.file), goStep(nextOf('map')))}>
            <span class="ellipsis" style="font-weight:${x.file === c.map ? 700 : 500}">${name(x)}</span><span class="dim">${facts[x.file] ? facts[x.file].tours : '…'}</span><span class="dim">${facts[x.file]?.year || '-'}</span><span class="dim" style="display:flex;align-items:center;gap:3px"><${Icon} name="location_on" size=${13} />${x.entry_points.length}</span>
          </button>`)}</div>`;
  return html`
    <section class="hub-sheet wide">
      <div style="width:min(1140px,100%);margin:0 auto;display:flex;flex-direction:column;flex:none">
        <${Head} icon="map" title="Maps" sub=${sub} raw=${false} />
        <div class="hub-toolbar">
          <${Seg} quiet value=${view} options=${[['list', 'List', 'view_list'], ['tiles', 'Tiles', 'grid_view']]} onChange=${keepView} />
          <div class="search"><${Search} value=${q} onInput=${setQ} placeholder="Search…" /></div>
        </div>
      </div>
      <div class="hub-rule"></div>
      <div class="hub-body"><div style="width:min(1140px,100%);margin:0 auto">${body}</div></div>
      <div style="width:min(1180px,100%);margin:0 auto;align-self:stretch;display:flex;flex-direction:column">
        <${Foot} text=${tr('%{n} maps installed', { n: (s.maps || []).length })}>${!c.free && html`<${EntrySelect} />`}</${Foot}>
      </div>
    </section>
    <${Actions} back onBack=${() => goStep(previousOf('map'))} extra=${[{ label: 'Check installed folders', onClick: () => go('setup') }]} main="Next step" onMain=${next} />
  `;
}

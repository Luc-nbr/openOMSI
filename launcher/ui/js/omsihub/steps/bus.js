// The bus (`flow::step_bus`, `buspick`): the buses as tiles in a wide sheet - the makers, a
// maker's models, a model's versions - with a breadcrumb back up, a search and the starred ones
// alone. A version chosen, the sheet makes way for the bus itself: it stands on the ground,
// drawn by the game's renderer and turned by the mouse, with its livery, depot file, fleet
// number and plate on a narrow sheet beside it. The action in the corner starts the drive.

import { html, useEffect, useMemo, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { choose, curBus, loadSituations } from '../../data.js';
import { toggleFavourite } from '../../pages/bus-picker.js';
import { BusPreview } from '../../preview.js';
import { Search, Select, Input } from '../../ui.js';
import { Actions, Foot, Head, Icon, Switch, tr, Button } from '../bits.js';
import { goStep, previousOf, hubStart } from '../flow.js';

const favKey = (file) => file.replace(/\\/g, '/').toLowerCase();
const nice = (s) => String(s || '').replace(/_/g, ' ').split(/\s+/).filter(Boolean).join(' ');
const natural = (a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });

/** The model and the version of a bus (`buspick::split_type`): `[friendlyname]`'s second line
 * split at its first " - "; without one, the version is the bus's own paint or its file's name. */
function splitType(v) {
  const label = nice(v.type_name) || nice(v.file.replace(/\\/g, '/').split('/').pop().replace(/\.(bus|ovh)$/i, ''));
  const parts = label.split(' - ').map((x) => x.trim()).filter(Boolean);
  const model = parts[0] || label;
  const version = parts.length > 1 ? parts.slice(1).join(' · ') : nice(v.default_paint) || nice(v.file.replace(/\\/g, '/').split('/').pop().replace(/\.(bus|ovh)$/i, ''));
  return [model, version];
}

/** The buses in their makers and models (`buspick::build_tree`). */
function tree(vehicles) {
  const buses = vehicles.map((v) => {
    const [model, version] = splitType(v);
    return { v, group: nice(v.manufacturer) || nice(v.folder), model, version };
  });
  const groups = new Map();
  for (const b of buses) {
    const g = groups.get(b.group.toLowerCase()) || { name: b.group, models: new Map() };
    groups.set(b.group.toLowerCase(), g);
    const m = g.models.get(b.model.toLowerCase()) || { name: b.model, buses: [] };
    g.models.set(b.model.toLowerCase(), m);
    m.buses.push(b);
  }
  const out = [...groups.values()].map((g) => ({ name: g.name, models: [...g.models.values()].map((m) => ({ ...m, buses: m.buses.sort((a, b) => natural(a.version, b.version)) })).sort((a, b) => natural(a.name, b.name)) }));
  return out.sort((a, b) => natural(a.name, b.name));
}

const versions = (n) => (n === 1 ? tr('one version') : tr('%{count} versions', { count: n }));
const liveries = (n) => (n === 1 ? tr('one livery') : tr('%{count} liveries', { count: n }));
const initials = (name) => name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0].toUpperCase()).join('') || '?';

function Tile({ title, sub, on, onClick, star, onStar, badge }) {
  return html`<button type="button" class=${'hub-tile' + (on ? ' on' : '')} onClick=${onClick}>
    <span class="pic hub-mono" style="padding-bottom:0;height:150px"><i>${initials(title)}</i></span>
    <span class="cap"><b>${title}</b><span><em>${sub}</em>${badge && html`<span class="tag" style="height:18px;font-size:8.1px">${badge}</span>`}
      ${star != null && html`<span role="button" class=${'star' + (star ? ' on' : '')} title=${tr(star ? 'Remove from the favourites' : 'Add to the favourites')} onClick=${(e) => (e.stopPropagation(), onStar())}><${Icon} name="star" size=${16} /></span>`}</span></span>
  </button>`;
}

/** The picker: the tiles of a level, or of a search across them all. */
function Picker({ onBus }) {
  const s = useStore();
  const all = s.vehicles || [];
  const groups = useMemo(() => tree(all), [all]);
  const [level, setLevel] = useState(s.hub.busLevel || []);
  const [q, setQ] = useState('');
  const [favOnly, setFavOnly] = useState(false);
  const keep = (lv) => {
    setLevel(lv);
    S.hub.busLevel = lv;
  };
  const cur = s.choice.bus;
  const fav = (b) => s.favourites.includes(favKey(b.v.file));
  const keepBus = (b) => !favOnly || fav(b) || b.v.file === cur;
  const has = (list) => list.some((b) => b.v.file === cur);
  const ql = q.trim().toLowerCase();
  let tiles;
  if (ql) {
    const hits = groups.flatMap((g) => g.models.flatMap((m) => m.buses)).filter(keepBus).filter((b) => [b.v.name, b.group, b.model, b.version, b.v.file].some((x) => x.toLowerCase().includes(ql)));
    tiles = hits.map((b) => ({ key: b.v.file, title: `${b.group} ${b.model}`, sub: `${b.version} · ${liveries(b.v.paints.length + 1)}`, on: b.v.file === cur, star: fav(b), b }));
  } else if (!level.length) {
    tiles = groups.map((g) => ({ g, buses: g.models.flatMap((m) => m.buses).filter(keepBus) })).filter((x) => x.buses.length).map(({ g, buses }) => ({ key: g.name, title: g.name, sub: versions(buses.length), on: has(buses), open: [g.name] }));
  } else if (level.length === 1) {
    const g = groups.find((x) => x.name === level[0]);
    tiles = (g?.models || []).map((m) => ({ m, buses: m.buses.filter(keepBus) })).filter((x) => x.buses.length).map(({ m, buses }) => ({ key: m.name, title: m.name, sub: versions(buses.length), on: has(buses), open: [level[0], m.name] }));
  } else {
    const m = groups.find((x) => x.name === level[0])?.models.find((x) => x.name === level[1]);
    tiles = (m?.buses || []).filter(keepBus).map((b) => ({ key: b.v.file, title: b.version, sub: liveries(b.v.paints.length + 1), on: b.v.file === cur, star: fav(b), b }));
  }
  const crumbs = [tr('Buses'), ...level];
  const v = curBus();
  const foot = `${tr('%{n} buses installed', { n: all.length })}${v ? ` · ${v.name}` : ''}`;
  return html`
    <section class="hub-sheet wide">
      <div style="width:min(1140px,100%);margin:0 auto;display:flex;flex-direction:column;flex:none;align-self:stretch">
        <${Head} icon="directions_bus" title="Buses" sub=${tr('Which bus are you taking out?')} />
        <div class="hub-toolbar">
          <nav style="display:flex;align-items:center;gap:4px;font-weight:700;min-width:0">
            ${crumbs.map((c, k) => html`${k > 0 && html`<${Icon} name="chevron_right" size=${14} />`}<button type="button" style=${`background:none;cursor:pointer;color:${k === crumbs.length - 1 ? 'var(--hub-text)' : 'var(--hub-text-dim)'};white-space:nowrap`} onClick=${() => keep(level.slice(0, k))}>${c}</button>`)}
          </nav>
          <div class="search"><${Search} value=${q} onInput=${setQ} placeholder="Search buses…" /></div>
          <span style="width:170px"><${Switch} label="Favourites only" checked=${favOnly} onChange=${setFavOnly} /></span>
        </div>
      </div>
      <div class="hub-rule"></div>
      <div class="hub-body"><div style="width:min(1140px,100%);margin:0 auto">
        ${!s.vehicles
          ? html`<div class="empty"><span class="spinner"></span><b>${s.vehiclesProgress ? tr('Reading the buses… {done} of {total}').replace('{done}', s.vehiclesProgress.done).replace('{total}', s.vehiclesProgress.total) : tr('Reading the buses…')}</b></div>`
          : !tiles.length
            ? html`<div class="empty"><${Icon} name="directions_bus" size=${34} /><b>${tr(favOnly ? 'No starred bus' : 'No bus found')}</b></div>`
            : html`<div class="hub-tiles">${tiles.map((x) => html`<${Tile} key=${x.key} title=${x.title} sub=${x.sub} on=${x.on} star=${x.star} onStar=${() => toggleFavourite(x.b.v.file)}
                onClick=${() => (x.b ? onBus(x.b) : keep(x.open))} />`)}</div>`}
      </div></div>
      <div style="width:min(1180px,100%);margin:0 auto;align-self:stretch"><${Foot} text=${foot} /></div>
    </section>`;
}

/** The bus chosen: its preview on the ground, its livery and the rest on the sheet. */
function Chosen({ onBack }) {
  const s = useStore();
  const c = s.choice;
  const v = curBus();
  if (!v) return null;
  const [group, model, version] = (() => {
    const [m, ver] = splitType(v);
    return [nice(v.manufacturer) || nice(v.folder), m, ver];
  })();
  const paints = [['', v.default_paint ? `${tr('Standard')} (${v.default_paint})` : tr('Standard')], ...v.paints.map((p) => [p, p])];
  const at = Math.max(0, paints.findIndex(([p]) => p === c.paint));
  const step = (d) => choose({ paint: paints[(at + d + paints.length) % paints.length][0] });
  const hofs = [['', `${tr('Automatic')}${s.hof ? ` (${s.hof})` : ''}`], ...v.hofs.map((h) => [h, h])];
  const star = s.favourites.includes(favKey(v.file));
  return html`
    <div class="hub-bus-stage"><${BusPreview} look=${{ bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} /></div>
    <section class="hub-sheet narrow">
      <div class="hub-pad" style="padding-top:18px;display:flex;align-items:center;gap:6px">
        <nav style="display:flex;align-items:center;gap:4px;font-size:9.6px;font-weight:700;color:var(--hub-text-dim);min-width:0;flex:1;flex-wrap:wrap">
          ${[tr('Buses'), group, model].map((x, k) => html`<button type="button" style="background:none;cursor:pointer;color:inherit" onClick=${() => onBack(k === 0 ? [] : k === 1 ? [group] : [group, model])}>${x}</button><${Icon} name="chevron_right" size=${13} />`)}
          <span style="color:var(--hub-text)">${version}</span>
        </nav>
        <button type="button" class=${'star' + (star ? ' on' : '')} title=${tr(star ? 'Remove from the favourites' : 'Add to the favourites')} onClick=${() => toggleFavourite(v.file)}><${Icon} name="star" size=${18} /></button>
      </div>
      <div class="hub-body"><div class="hub-pad">
        <h2 style="font-size:16.9px;font-weight:700;margin-bottom:4px">${v.name}</h2>
        <p style="color:var(--hub-text-dim);margin-bottom:18px">${[v.manufacturer, v.type_name].filter(Boolean).join(' - ')}</p>
        <div class="hub-caps"><b>${tr('Livery')}</b>${paints.length > 1 && html`<span style="color:var(--hub-text-dim)">${at + 1} / ${paints.length}</span>`}</div>
        <div style="display:flex;gap:8px;margin-bottom:18px">
          <span style="flex:1;min-width:0"><${Select} raw value=${c.paint} options=${paints} onChange=${(paint) => choose({ paint })} /></span>
          ${paints.length > 1 && html`<${Button} icon="chevron_left" title="Preview previous livery" onClick=${() => step(-1)} /><${Button} icon="chevron_right" title="Preview next livery" onClick=${() => step(1)} />`}
        </div>
        <div style="display:grid;grid-template-columns:110px 1fr;gap:10px 6px;align-items:center">
          <span style="color:var(--hub-text-soft)">${tr('Depot file')}</span>
          <${Select} raw value=${c.hof_manual ? c.hof : ''} options=${hofs} onChange=${(h) => choose({ hof: h, hof_manual: !!h })} />
          ${v.numbers?.length > 0 && html`<span style="color:var(--hub-text-soft)">${tr('Fleet number')}</span>
            <${Select} raw value=${c.number || v.numbers[0][0]} options=${v.numbers.map(([n, p]) => [n, p ? `${n}  (${p})` : n])} onChange=${(number) => choose({ number })} />`}
          <span style="color:var(--hub-text-soft)">${tr('Number plate')}</span>
          <${Input} value=${c.plate} placeholder="Automatic" onInput=${(plate) => choose({ plate })} />
        </div>
        <div class="hub-rule" style="margin:18px 0 14px"></div>
        ${v.missing_packs?.length > 0 && html`<p style="color:var(--hub-warn);margin-bottom:10px">${tr('Parts missing: needs %{packs}', { packs: v.missing_packs.join(', ') })}</p>`}
        ${v.description && html`<p class="selectable" style="color:var(--hub-text-dim);font-size:9.2px;white-space:pre-line;margin-bottom:10px">${v.description.trim()}</p>`}
        <p class="selectable" style="color:var(--hub-text-faint);font-size:8.1px">${v.file}</p>
      </div></div>
      <div class="hub-pad" style="padding-top:12px;padding-bottom:16px"><${Button} wide icon="directions_bus" label="Change bus" onClick=${() => onBack(null)} /></div>
    </section>`;
}

export function BusStep() {
  const s = useStore();
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    loadSituations();
  }, []);
  const showing = s.hub.busView === 'bus' && curBus();
  const show = (view, level) => {
    if (level) S.hub.busLevel = level;
    set({ hub: { ...S.hub, busView: view } });
  };
  const go = async (extra) => {
    setBusy(true);
    await hubStart(extra);
    setBusy(false);
  };
  const last = s.situations?.[0];
  return html`
    ${showing
      ? html`<${Chosen} onBack=${(level) => show('tiles', level || undefined)} />`
      : html`<${Picker} onBus=${(b) => (b.v.file !== s.choice.bus && choose({ bus: b.v.file, paint: '', number: '', hof_manual: false }), show('bus'))} />`}
    <${Actions} back onBack=${() => (showing ? show('tiles') : goStep(previousOf('bus')))} from=${showing ? 'calc(var(--hub-edge-in) + clamp(288px, 27.4vw, 420px) + 14px)' : null}
      extra=${last ? [{ label: 'Continue last game', icon: 'history', onClick: () => go({ situation: last.file, lan: 'off' }), disabled: busy }] : []}
      main="Drive" mainDisabled=${!curBus() || !s.choice.map} busy=${busy} onMain=${() => go()} />
  `;
}

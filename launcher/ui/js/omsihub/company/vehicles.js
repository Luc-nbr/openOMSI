// Which buses the company works with (Luc, Beta 54: "on UK maps they want only UK buses, in
// Europe the other way round"; Trello card 83): the installed bus models, each with its category
// and region - as detected from its files, or as the player corrected it for every company - and
// for the company on or off. The same list serves the company's settings (`VehiclesDialog`),
// the founding wizard (`VehiclePicker`: its choice kept in the wizard until the company is
// founded) and the launcher's Settings (`VehicleClasses`: the corrections alone). The server
// says what is shown (`co_vehicles`) and does what the controls ask (`co_vehicles_act`).

import { html, useEffect, useRef, useState, tr, cmd, setCo, refresh, Btn, Seg, Switch, Dialog, Waiting } from './kit.js';

// (the company's look and the list's own, also where no company page loaded them: Settings)
for (const href of ['omsihub/company.css', 'omsihub/vehicles.css']) {
  if (!document.querySelector(`link[href="${href}"]`)) {
    const l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = href;
    document.head.append(l);
  }
}

/** A heading's and a row's heights in the list (it draws only the rows in sight). */
const HEAD = 34;
const ROW = 54;

/** The list's view, asked again when `args` change or after an action (`bump`). */
function useList(args, bump) {
  const [v, setV] = useState(null);
  const key = JSON.stringify(args);
  useEffect(() => {
    let gone = false;
    cmd('co_vehicles', args)
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ error: e.message }));
    return () => {
      gone = true;
    };
  }, [key, bump]);
  return v;
}

/** The rows in sight of a scrolled list of fixed heights (`items`: [height, key, draw]). */
function Virtual({ items, style = '' }) {
  const ref = useRef(null);
  const [view, setView] = useState({ top: 0, h: 600 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const look = () => setView({ top: el.scrollTop, h: el.clientHeight });
    look();
    el.addEventListener('scroll', look, { passive: true });
    const ro = new ResizeObserver(() => requestAnimationFrame(look));
    ro.observe(el);
    return () => {
      el.removeEventListener('scroll', look);
      ro.disconnect();
    };
  }, []);
  let y = 0;
  const shown = [];
  for (const [h, key, draw] of items) {
    if (y + h >= view.top - 400 && y <= view.top + view.h + 400) shown.push(html`<div key=${key} class="vh-item" style=${`top:${y}px;height:${h}px`}>${draw()}</div>`);
    y += h;
  }
  return html`<div class="vh-list co-scroll" ref=${ref} style=${style}><div style=${`position:relative;height:${y}px`}>${shown}</div></div>`;
}

/** The list (`scope`: "company", "wizard" - `choice` and `onChoice` the wizard's - or
 * "launcher"), with its filters and, but in the launcher's, the company's choice above it. */
export function VehicleList({ scope = 'company', choice = null, onChoice, map = '', height = '' }) {
  const [f, setF] = useState({ search: '', cat: '', region: '' });
  const [bump, setBump] = useState(0);
  const v = useList({ scope, choice: scope === 'wizard' ? choice : null, map, ...f }, bump);
  // (the installed buses still being read: asked again in a moment - outside the company no
  // clock's tick asks for it)
  useEffect(() => {
    if (!v?.reading) return undefined;
    const t = setTimeout(() => setBump((b) => b + 1), 1000);
    return () => clearTimeout(t);
  }, [v]);
  if (!v) return html`<${Waiting} />`;
  if (v.error) return html`<p class="co-para t-danger">${v.error}</p>`;
  if (v.reading) return html`<p class="co-para">${v.reading}</p>`;
  const act = async (what, args = {}) => {
    const r = await cmd('co_vehicles_act', { what, scope, choice: scope === 'wizard' ? choice : null, map, ...args }).catch((e) => ({ error: e.message }));
    if (scope === 'wizard' && r?.choice) onChoice?.(r.choice);
    if (scope === 'company') refresh();
    setBump((b) => b + 1);
  };
  const own = scope !== 'launcher';
  const option = (list, value) => list.map(([k, l]) => html`<option value=${k} selected=${k === value}>${l}</option>`);
  const labelOf = (list, k) => (list.find((x) => x[0] === k) || [k, k])[1];
  // (the rows by category, each category under its heading)
  const items = [];
  let last = null;
  for (const row of v.rows) {
    if (row.cat !== last) {
      last = row.cat;
      const n = v.rows.filter((x) => x.cat === row.cat).length;
      const head = labelOf(v.cats, row.cat);
      items.push([HEAD, 'h-' + row.cat, () => html`<div class="vh-head"><b>${head}</b><span>${n}</span></div>`]);
    }
    items.push([ROW, row.file, () => html`<div class=${'vh-row' + (own && !row.on ? ' off' : '')} data-file=${row.file}>
      <span class="vh-mono">${row.initials}</span>
      <span class="vh-what"><b class="cut" title=${row.file}>${row.name}</b><span class="cut">${row.maker ? row.maker + '  ·  ' : ''}${row.guess}${row.owned ? '  ·  ' + tr('%{n} in the fleet', { n: row.owned }) : ''}</span></span>
      <select class=${'co-field vh-pick' + (row.cat_fixed ? ' fixed' : '')} title=${row.cat_fixed ? v.fixed_tip : row.guess} onChange=${(e) => act('class', { file: row.file, cat: e.currentTarget.value })}>
        <option value="" selected=${!row.cat_fixed}>${labelOf(v.cats, row.guess_cat)} (${v.as_guessed})</option>${option(v.cats, row.cat_fixed ? row.cat : null)}
      </select>
      <select class=${'co-field vh-pick' + (row.region_fixed ? ' fixed' : '')} title=${row.region_fixed ? v.fixed_tip : row.guess} onChange=${(e) => act('class', { file: row.file, region: e.currentTarget.value })}>
        <option value="" selected=${!row.region_fixed}>${labelOf(v.regions, row.guess_region).split(' (')[0]} (${v.as_guessed})</option>${option(v.regions, row.region_fixed ? row.region : null)}
      </select>
      ${own && html`<${Switch} checked=${row.on} label="" title=${row.owned && !row.on ? v.owned_tip : row.name} onChange=${(on) => act('model', { file: row.file, on })} />`}
    </div>`]);
  }
  return html`<div class="vh" style=${height ? `height:${height}` : ''}>
    ${v.note && html`<p class="co-note vh-note">${v.note}</p>`}
    ${own && html`<div class="vh-mode">
      <${Seg} value=${v.mode} options=${v.modes} onChange=${(m) => act('mode', { mode: m })} style="flex:1;min-width:0;max-width:640px;height:38px" />
      <span class="vh-count">${v.count}</span>
    </div>
    <div class="vh-quick">${v.quick.map(([k, l]) => html`<${Btn} small label=${l} onClick=${() => act('quick', { which: k })} />`)}</div>`}
    <div class="vh-filters">
      <input class="co-field" style="flex:1;min-width:120px" value=${f.search} placeholder=${v.search} onInput=${(e) => setF({ ...f, search: e.currentTarget.value })} />
      <select class="co-field" style="width:180px" onChange=${(e) => setF({ ...f, cat: e.currentTarget.value })}><option value="">${v.all_cats}</option>${option(v.cats, f.cat)}</select>
      <select class="co-field" style="width:200px" onChange=${(e) => setF({ ...f, region: e.currentTarget.value })}><option value="">${v.all_regions}</option>${option(v.regions, f.region)}</select>
    </div>
    <div class="vh-shown"><span>${v.shown_of}</span>
      ${own && v.rows.length > 0 && html`<span class="rest"></span>
        <${Btn} small label=${v.switch_on} onClick=${() => act('models', { files: v.rows.map((r) => r.file), on: true })} />
        <${Btn} small label=${v.switch_off} onClick=${() => act('models', { files: v.rows.map((r) => r.file), on: false })} />`}</div>
    ${v.rows.length ? html`<${Virtual} items=${items} />` : html`<p class="co-para" style="margin-top:12px">${v.none}</p>`}
  </div>`;
}

/** The company's vehicles, from its settings (`dialog: { kind: 'vehicles' }`). */
export function VehiclesDialog() {
  const close = () => setCo({ dialog: { kind: 'settings' } });
  const foot = html`<span class="rest"></span><${Btn} kind="primary" label=${tr('Done')} onClick=${close} />`;
  return html`<${Dialog} icon="directions_bus" title=${tr('Your vehicles')} w=${1000} h=${760} onClose=${close} foot=${foot} bodyStyle="display:flex;flex-direction:column;overflow:hidden">
    <${VehicleList} scope="company" />
  </${Dialog}>`;
}

/** The founding wizard's choice of models (`choice` the wizard keeps; `map` its map's file). */
export function VehiclePicker({ choice, onChoice, map, onClose }) {
  const foot = html`<span class="rest"></span><${Btn} kind="primary" label=${tr('Done')} onClick=${onClose} />`;
  return html`<${Dialog} icon="directions_bus" title=${tr('Your vehicles')} w=${1000} h=${760} onClose=${onClose} foot=${foot} bodyStyle="display:flex;flex-direction:column;overflow:hidden">
    <${VehicleList} scope="wizard" choice=${choice} onChoice=${onChoice} map=${map} />
  </${Dialog}>`;
}

/** The launcher's Settings: the categories and regions of the models, for every company. */
export function VehicleClasses() {
  return html`<div class="vh-settings"><${VehicleList} scope="launcher" height="560px" /></div>`;
}

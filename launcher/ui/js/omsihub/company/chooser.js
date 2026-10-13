// The start of the bus company and its founding, as our game-drawn launcher's
// (`company/chooser.rs`, `company/wizard.rs`): how to play (singleplayer, multiplayer later),
// the driver's companies as tiles with a new one beside them, and the founding wizard - the
// company on the left (name, short name, colours, logo, map and depot), the game mode on the
// right (Casual, Normal or Realistic, the first day, how it buys).

import { S } from '../../store.js';
import { call } from '../../api.js';
import { html, useEffect, useState, tr, HOME, setCo, cmd, refresh, takeShell, useView, Icon, Mark, ModePill, Btn, Seg, Chips, Select, Dialog, IconBtn } from './kit.js';

/** The companies as the chooser's view lists them (boot keeps one too). */
export const readCompanies = () => S.co.list?.companies?.tiles || [];

export function Chooser() {
  const v = useView('chooser');
  const [asking, setAsking] = useState(null);
  if (!v) return null;
  const step = S.co.chooser;
  if (step === 'mode') return html`<${Modes} v=${v.modes} n=${v.companies.tiles.length} />`;
  const cv = v.companies;
  const open = async (id) => {
    await cmd('co_open', { id });
    // (the shell of the company opened first: the founding is not shown on the way)
    takeShell(await cmd('co_tick', { modal: true, still: true }));
    setCo({ chooser: null, tab: HOME });
    refresh();
  };
  const del = async (id) => {
    setAsking(null);
    await cmd('co_delete', { id });
    refresh();
  };
  const victim = asking && cv.tiles.find((t) => t.id === asking);
  return html`<div class="co-choose">
    <div style="flex:none"><${Btn} icon="chevron_left" label=${cv.back} style="width:180px;height:38px" onClick=${() => setCo({ chooser: 'mode' })} /></div>
    <div class="co-scroll" style="flex:1;margin-top:12px;display:flex;flex-direction:column">
      <span style="flex:0.35 1 0"></span>
      <div class="head"><h1>${cv.title}</h1><p>${cv.text}</p></div>
      <div class="co-companies">
        ${cv.tiles.map((t) => html`<${CompanyTile} t=${t} tip=${cv.delete_tip} onOpen=${() => open(t.id)} onDelete=${() => setAsking(t.id)} />`)}
        <button type="button" class="co-tile co-company new" onClick=${() => setCo({ wizard: newWizard() })}>
          <span class="plus"><${Icon} name="add" size=${28} /></span>
          <b>${cv.new}</b><div class="home">${cv.new_sub}</div>
        </button>
      </div>
      <span style="flex:0.65 1 0"></span>
    </div>
    ${victim && html`<${Dialog} icon="warning" title=${cv.ask_title.replace('%{name}', victim.name)} w=${560} onClose=${() => setAsking(null)}
      foot=${html`<span class="rest"></span><${Btn} label=${cv.ask_no} onClick=${() => setAsking(null)} /><${Btn} kind="danger" label=${cv.ask_yes} onClick=${() => del(victim.id)} />`}>
      <p class="co-para">${cv.ask_text}</p>
    </${Dialog}>`}
  </div>`;
}

/** How to play: one's own companies, or (later) one run together on a server. */
function Modes({ v }) {
  return html`<div class="co-choose">
    <span style="flex:0.4 1 0"></span>
    <div class="head"><h1>${v.title}</h1><p>${v.text}</p></div>
    <div class="co-modes">
      <button type="button" class="co-tile co-mode" data-mode="single" onClick=${() => setCo({ chooser: 'companies' })}>
        <span class="mark"><${Icon} name="person" size=${30} /></span>
        <h3>${v.single.title}</h3><p>${v.single.text}</p><span class="foot">${v.single.foot}</span>
      </button>
      <div class="co-tile co-mode still" title=${v.multi.tip}>
        <span class="later">${v.multi.foot}</span>
        <span class="mark"><${Icon} name="groups" size=${30} /></span>
        <h3>${v.multi.title}</h3><p>${v.multi.text}</p><span class="foot">${v.multi.foot}</span>
      </div>
    </div>
    <span style="flex:0.6 1 0"></span>
  </div>`;
}

/** A company's tile: its mark, mode and clock, its name, where it is at home, its day and
 * cash; the bin under the mouse. */
function CompanyTile({ t, tip, onOpen, onDelete }) {
  return html`<button type="button" class="co-tile co-company" data-company=${t.id} onClick=${onOpen}>
    <div class="top"><${Mark} mark=${t.mark} size=${56} />
      <span class="side"><${ModePill} mode=${t.mode} />
        ${t.clock && html`<span class=${'clockline t-' + t.clock[1]}><i></i>${t.clock[0]}</span>`}</span>
    </div>
    <b class="cut">${t.name}</b>
    <div class="home cut">${t.home}</div>
    <div class=${'day cut' + (t.negative ? ' negative' : '')}>${t.day}</div>
    <span class="bin" onClick=${(e) => (e.stopPropagation(), onDelete())}><${IconBtn} icon="delete" size=${16} title=${tip} style="width:28px;height:28px" /></span>
  </button>`;
}

// --- the founding ---------------------------------------------------------------------------

/** A new founding (`Wizard::new`): no name, orange and blue, the map of the last duty, today,
 * buying by contract, the mode of the company open (Casual else). */
export function newWizard() {
  return { name: '', short: '', colours: [0, 4], logo: null, map: null, depot: 0, when: 0, date: null, buying: 1, mode: S.co.shell?.mode?.key || null };
}

/** The founding wizard (`wizard::draw`): laid out for the room it has, scrolled whole when the
 * window is too low. */
export function Wizard() {
  const w = S.co.wizard || newWizard();
  const [v, setV] = useState(null);
  const put = (patch) => setCo({ wizard: { ...w, ...patch } });
  useEffect(() => {
    let gone = false;
    cmd('co_wizard', { map: w.map, when: w.when, date: w.date, name: w.name, mode: w.mode })
      .then((x) => {
        if (gone) return;
        setV(x);
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, [w.map, w.when, w.date, w.name, w.mode]);
  if (!v) return null;
  const palette = v.palette;
  const short = w.short.trim() ? w.short.trim().toUpperCase() : v.short_hint;
  const ok = w.name.trim() && v.maps.length;
  const found = async () => {
    if (!ok) {
      setCo({ popup: v.maps.length ? v.popup_name : v.popup_map });
      return;
    }
    const r = await cmd('co_found', { name: w.name, short: w.short, colours: w.colours, logo: w.logo, map: w.map ?? v.map, depot: w.depot, when: w.when, date: w.date ?? v.date, mode: w.mode || v.mode, buying: w.buying });
    if (r?.refused) {
      setCo({ popup: r.refused === 'map' ? v.popup_map : v.popup_name });
      return;
    }
    setCo({ wizard: null, chooser: null, tab: HOME });
    refresh();
  };
  const pickLogo = async () => {
    const p = await call('pick_file', { title: v.choose_logo }).catch(() => null);
    if (p) put({ logo: p });
  };
  const logoName = w.logo ? w.logo.split(/[\\/]/).pop() : v.no_logo;
  const mode = w.mode || v.mode;
  const casual = mode === 'casual';
  return html`<div class="co-found">
    <div class="inner">
      <h1>${v.title}</h1>
      <p class="intro">${v.intro}</p>
      <div class="cols">
        <section class="co-section left">
          <div class="co-caps" style="margin-bottom:14px">${v.company_caps}</div>
          <div class="row"><label class="co-label">${v.name_label}</label>
            <input class="co-field" style="margin-top:4px" value=${w.name} placeholder=${v.name_hint} onInput=${(e) => put({ name: e.currentTarget.value })} /></div>
          <div class="row"><label class="co-label">${v.short_label}</label>
            <input class="co-field" style="margin-top:4px;width:calc(50% - 6px)" maxlength="4" value=${w.short} placeholder=${v.short_hint} onInput=${(e) => put({ short: e.currentTarget.value.slice(0, 4) })} /></div>
          ${[0, 1].map((k) => html`<div class="row"><label class="co-label">${v.colours[k]}</label>
            <div class="swatches">${palette.map((hex, i) => html`<span class=${'swatch' + (w.colours[k] === i ? ' on' : '')} style=${`background:${hex}`} onClick=${() => put({ colours: k === 0 ? [i, w.colours[1]] : [w.colours[0], i] })}></span>`)}</div></div>`)}
          <div class="logo"><${Mark} mark=${{ short, colours: [palette[w.colours[0]], palette[w.colours[1]]], logo: null }} size=${68} />
            <div class="what"><p>${logoName}</p>
              <div class="btns"><${Btn} icon="photo_camera" label=${v.choose_logo} onClick=${pickLogo} />
                ${w.logo && html`<${Btn} icon="close" label=${v.remove_logo} onClick=${() => put({ logo: null })} />`}</div></div></div>
          ${v.maps.length
            ? html`<div class="home2"><label class="co-label">${v.map_label}</label><label class="co-label">${v.depot_label}</label>
                <${Select} value=${w.map ?? v.map} options=${v.maps} onChange=${(m) => put({ map: m, depot: 0 })} />
                ${v.depots.length ? html`<${Select} value=${Math.min(w.depot, v.depots.length - 1)} options=${v.depots} onChange=${(d) => put({ depot: d })} />` : html`<span class="co-para" style="line-height:40px">${v.no_depot}</span>`}</div>`
            : html`<p class="co-para t-warn">${v.no_maps}</p>`}
        </section>
        <section class="co-section" style="display:flex;flex-direction:column">
          <div class="co-caps" style="margin-bottom:14px">${v.mode_caps}</div>
          <div class="cards">${v.cards.map((cd) => html`<button type="button" class=${'card' + (cd.mode.key === mode ? ' on' : '')} data-mode=${cd.mode.key} onClick=${() => put({ mode: cd.mode.key })}>
            <div class="top"><i><${Icon} name=${cd.mode.icon} size=${19} /></i><b>${cd.mode.label}</b></div>
            <div class="pitch">${cd.pitch}</div>
            <div class="capital">${cd.capital}</div>
            <ul>${cd.bullets.map((b) => html`<li><${Icon} name="check_circle" size=${15} /><span>${b}</span></li>`)}</ul>
          </button>`)}</div>
          <p class="mode-note">${v.mode_note}</p>
          <div class="under">
            <div><label class="co-label">${v.first_label}</label>
              <div style="margin-top:4px"><${Chips} value=${w.when} options=${v.whens.map((x, i) => [i, x])} onChange=${(k) => put({ when: k })} /></div>
              ${w.when === 2
                ? html`<input type="date" class="co-field" style="margin-top:8px;width:220px;height:38px" value=${w.date || v.date} onChange=${(e) => e.currentTarget.value && put({ date: e.currentTarget.value })} />`
                : html`<div class="begins">${v.begins}</div>`}</div>
            <div><label class="co-label">${v.buying_label}</label>
              ${casual
                ? html`<div class="casual-buy"><${Icon} name="airport_shuttle" size=${18} /><span>${v.casual_buying}</span></div>`
                : html`<${Seg} value=${w.buying} options=${v.buyings.map((x, i) => [i, x])} onChange=${(b) => put({ buying: b })} style="max-width:280px;margin-top:4px" />
                  <p class="says">${v.buying_says[w.buying]}</p>`}</div>
          </div>
        </section>
      </div>
      <div class="foot">
        <span class="need">${ok ? '' : v.need_name}</span>
        ${v.cancel && html`<${Btn} label=${v.cancel} style="width:140px" onClick=${() => setCo({ wizard: null })} />`}
        <${Btn} kind="primary" icon="check_circle" label=${v.found} onClick=${found} />
      </div>
    </div>
  </div>`;
}

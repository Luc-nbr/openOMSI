// The company's fleet, as our game-drawn launcher's (`company/fleet.rs`, `adverts.rs`): its
// buses as tiles (the number, the name, plate and kind, kilometres and age, the condition and
// how the bus is now), the dealer, and the advertising on the buses; a bus's dialog (its
// livery, what it is, selling or giving it back, a service, training its drivers), and leasing
// or renting one of the dealer's buses.

import { html, useEffect, useState, tr, setCo, setUi, ui, act, useView, useSize, textW, goTab, Icon, Btn, Seg, Section, Meter, Dialog, Select } from './kit.js';
import { DealerPage } from './dealer.js';
import { photo, wantPhoto } from '../photos.js';

export function FleetPage() {
  const st = ui('fleet', { tab: 0 });
  const v = useView('fleet', { tab: st.tab });
  if (!v?.tabs) return null;
  const w = v.tabs.length === 2 ? 380 : 560;
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <${Seg} value=${v.tab} options=${v.tabs.map((t, k) => [k, t])} onChange=${(k) => setUi('fleet', { tab: k })} style=${`width:${w}px;max-width:100%;height:40px;flex:none`} />
    <div style="flex:1;min-height:0;margin-top:16px">
      ${v.tab === 1 ? html`<${DealerPage} />` : v.tab === 2 ? html`<${Adverts} a=${v.body} />` : html`<${Buses} b=${v.body} />`}
    </div>
  </div>`;
}

/** The tiles' columns for a width (`fleet::layout`): at least 270 wide, the photo half as high. */
export function layout(w) {
  const gap = 14;
  const cols = Math.max(1, Math.floor((w + gap) / (270 + gap)));
  const tw = (w - gap * (cols - 1)) / cols;
  const ph = tw * 0.52;
  return { cols, tw, ph, th: ph + 118 };
}

/** The bus's initials (`buspick::initials`): what its tile shows for a photo. */
export const initials = (name) =>
  String(name || '')
    .split(/[\s_-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join('');

/** A bus's photo (`photos().get` drawn with `image_cover`): the bus's initials on the night's
 * gradient until it is there, or when the game-drawn launcher never took it. */
export function Photo({ file, paint, name, style = '', mono = true }) {
  const [src, setSrc] = useState(null);
  useEffect(() => {
    let gone = false;
    // (Omsi-Hub 6/6: one not there is photographed by the game program, `photos.js`)
    const leave = file ? wantPhoto(file, paint) : null;
    photo(file, paint, (u) => !gone && setSrc(u)).then((u) => !gone && setSrc(u));
    return () => {
      gone = true;
      leave?.();
    };
  }, [file, paint]);
  return html`<span class=${'photo' + (src ? ' shot' : '')} style=${style + (src ? `;background-image:url("${src}")` : '')}>${!src && mono && html`<span class="mono">${initials(name)}</span>`}</span>`;
}

/** A bus's tile (`fleet::tile`): its photo (the bus's initials on the night's gradient while
 * there is none) and the room under it. */
export function BusTile({ name, file, paint, r, onClick, children, over, attrs = {} }) {
  const ph = r.w * 0.52;
  return html`<button type="button" class="co-bustile" style=${`left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`} onClick=${onClick} ...${attrs}>
    <${Photo} file=${file} paint=${paint} name=${name} style=${`height:${ph + 14}px;padding-bottom:14px;clip-path:inset(0 0 14px 0)`} />
    ${over}
    <span class="info" style=${`top:${ph + 12}px`}>${children}</span>
  </button>`;
}

function Buses({ b }) {
  const [ref, size] = useSize();
  if (!b || b.kind !== 'buses') return html`<div ref=${ref}></div>`;
  const order = b.order && html`<div class="co-order"><${Icon} name="schedule" size=${18} class="t-accent2" /><span class="cut">${b.order}</span></div>`;
  if (!b.buses.length) {
    return html`<div ref=${ref}>${order}
      <p class="co-para" style="max-width:860px;margin-top:4px">${b.empty}</p>
      <${Btn} kind="primary" icon="directions_bus" label=${b.to_dealer} style="height:40px;margin-top:18px" onClick=${() => setUi('fleet', { tab: 1 })} />
    </div>`;
  }
  const L = layout(Math.max(0, size.w - 12));
  const gap = 14;
  return html`<div style="height:100%;display:flex;flex-direction:column">
    ${order && html`<div style="flex:none;height:38px">${order}</div>`}
    <div class="co-scroll" ref=${ref} style="flex:1;min-height:0">
      <div style=${`position:relative;height:${Math.ceil(b.buses.length / L.cols) * (L.th + gap)}px`}>
        ${size.w > 0 &&
        b.buses.map((bus, k) => {
          const r = { x: (k % L.cols) * (L.tw + gap), y: Math.floor(k / L.cols) * (L.th + gap), w: L.tw, h: L.th };
          const nw = textW(bus.number, 15, 900) + 20;
          return html`<${BusTile} name=${bus.name} file=${bus.file} paint=${bus.livery} r=${r} onClick=${() => setCo({ dialog: { kind: 'vehicle', id: bus.id } })} attrs=${{ 'data-bus': bus.id }}
            over=${html`<span class="numb" style=${`width:${nw}px`}>${bus.number}</span>
              ${bus.advert && html`<span class="ad cut" style=${`max-width:${r.w - nw - 30}px`}>${bus.advert}</span>`}`}>
            <b class="cut">${bus.name}</b>
            <span class="sub cut">${bus.sub}</span>
            <span class="facts"><span class="cut">${bus.facts}</span><span class="cond" title=${bus.condition.tip}><${Meter} frac=${bus.condition.frac} tone=${bus.condition.tone} /></span></span>
            <span class=${'status t-' + bus.tone}><i class=${'dot-' + bus.tone}></i><span class="cut">${bus.status}</span></span>
          </${BusTile}>`;
        })}
      </div>
    </div>
  </div>`;
}

// --- advertising --------------------------------------------------------------------------

/** The advertisers' offers this week and the contracts running (`adverts::draw`). */
function Adverts({ a }) {
  const [armed, setArmed] = useState(null);
  if (!a || a.kind !== 'adverts') return null;
  return html`<div style="display:grid;grid-template-columns:max(360px, calc((100% - 16px) * 0.55)) minmax(0,1fr);gap:16px;grid-template-rows:minmax(0,1fr);height:100%">
    <${Section} title=${a.offers_title}>
      <p class="co-note" style="flex:none">${a.note}</p>
      ${a.offers.length
        ? html`<div class="co-scroll" style="flex:1;min-height:40px;margin-top:12px">${a.offers.map(
            (o) => html`<div class="co-adrow" style=${`--kind:var(--co-${o.tone})`}>
              <div class="l"><b class="cut">${o.advertiser}</b><span class="cut t-soft">${o.trade}</span><span class=${'cut t-' + o.tone} style="font-weight:500">${o.terms}</span></div>
              <div class="r"><b class=${o.lock ? 't-dim' : ''}>${o.money}</b><span class="t-soft">${o.all}</span>
                ${o.lock
                  ? html`<${Btn} kind="ghost" icon="lock" label=${o.lock} style="width:180px;height:30px" onClick=${() => act('open_advert', { no: o.no })} />`
                  : html`<${Btn} kind="primary" icon="description" label=${a.to_contract} style="width:180px;height:30px" onClick=${() => act('open_advert', { no: o.no })} />`}</div>
            </div>`,
          )}</div>`
        : html`<p class="co-para" style="margin-top:12px">${a.no_offers}</p>`}
    </${Section}>
    <${Section} title=${a.running_title}>
      ${a.running.length
        ? html`<div class="co-scroll" style="flex:1;min-height:0">${a.running.map(
            (x) => html`<div class="co-adrow running" style=${`--kind:var(--co-${x.tone})`}>
              <div class="l"><b class="cut">${x.advertiser}</b><span class=${'cut t-' + x.tone} style="font-weight:500">${x.what}</span><span class="cut t-soft">${x.until}</span></div>
              <div class="tools"><${Btn} kind="ghost" label=${a.look} style="width:130px;height:30px" onClick=${() => act('open_advert', { no: x.no, readonly: true })} />
                <${Btn} kind=${armed === x.no ? 'danger' : 'ghost'} label=${armed === x.no ? x.again : x.end} title=${a.end_tip} style="width:180px;height:30px;padding:0 8px"
                  onClick=${() => (armed === x.no ? (setArmed(null), act('advert_end', { no: x.no })) : setArmed(x.no))} /></div>
            </div>`,
          )}</div>`
        : html`<p class="co-para">${a.no_running}</p>`}
    </${Section}>
  </div>`;
}

// --- the dialogs ----------------------------------------------------------------------------

/** The photo's place and the livery on a dialog's left (`fleet::bus_side`). */
function BusSide({ file, name, kind, livery, onLivery, design, designTip, onDesign, note }) {
  const k = Math.max(0, livery.options.findIndex(([p]) => p === livery.value));
  return html`<div class="co-busside">
    <${Photo} file=${file} paint=${livery.value} name=${name} />
    <div class="kind">${kind}</div>
    <label class="co-label" style="margin-top:10px">${livery.label}</label>
    <${Select} value=${k} options=${livery.options.map(([, l]) => l)} onChange=${(i) => onLivery(livery.options[i][0])} style="margin-top:4px" />
    ${design ? html`<${Btn} kind="ghost" icon="livery_fill" label=${design} title=${designTip} style="width:100%;margin-top:10px" onClick=${onDesign} />` : html`<p class="co-note" style="margin-top:12px">${note}</p>`}
  </div>`;
}

/** A row of a price table (`fleet::price_row`). */
export const PriceRow = ({ r, h = 34 }) => html`<div class=${'co-price' + (r.strong ? ' strong' : '')} style=${`height:${h}px`} title=${r.tip || undefined}><span class="cut">${r.a}</span><b class=${'cut' + (r.tone ? ' t-' + r.tone : '')}>${r.b}</b></div>`;

/** Setting a bus's age oneself (`age::age_switch`): a switch, and the year of first
 * registration with its button; `onSet('')` goes back to the plate's or the market's. */
export function AgeSet({ a, onSet }) {
  const [on, setOn] = useState(a.on);
  const [year, setYear] = useState(a.year);
  useEffect(() => {
    setOn(a.on);
    setYear(a.year);
  }, [a.on, a.year]);
  const flip = (e) => {
    const v = e.currentTarget.checked;
    setOn(v);
    if (!v && a.on) onSet('');
  };
  return html`<div class="co-ageset" title=${a.tip} style="display:flex;align-items:center;gap:10px;margin-top:10px;min-height:36px">
    <label style="display:flex;align-items:center;gap:8px;cursor:pointer"><input type="checkbox" checked=${on} onChange=${flip} /><span>${a.label}</span></label>
    ${on && html`<input class="co-field" style="width:150px;height:34px" inputmode="numeric" value=${year} placeholder=${a.placeholder}
        onInput=${(e) => setYear(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && year.trim() && onSet(year.trim())} />
      <${Btn} label=${a.apply} style="height:34px" onClick=${() => year.trim() && onSet(year.trim())} />`}
  </div>`;
}

/** A bus of the fleet (`Dialog::Vehicle`). */
export function VehicleDialog({ d }) {
  const v = useView('vehicle', { id: d.id });
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  if (v.gone) {
    close();
    return null;
  }
  const foot = html`
    <${Btn} kind="danger" label=${v.sell} onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'sell', id: d.id } })} />
    <${Btn} icon="construction" label=${v.service} onClick=${() => act('bus_service', { id: d.id })} />
    <${Btn} icon="groups" label=${v.train} onClick=${() => (setUi('people', { tab: 2, group: 1, first: 0 }), setCo({ dialog: null }), goTab(2))} />
    <span class="rest"></span>
    <${Btn} kind="primary" label=${v.close} onClick=${close} />`;
  return html`<${Dialog} icon="directions_bus" title=${v.title} w=${920} h=${640} onClose=${close} foot=${foot}>
    <div class="co-busdialog">
      <${BusSide} file=${v.file} name=${v.name} kind=${v.bus_kind} livery=${v.livery} onLivery=${(p) => act('bus_livery', { id: d.id, livery: p })} design=${v.design} designTip=${v.design_tip} onDesign=${() => act('design_livery', { id: d.id })} />
      <div class="right">${v.rows.map((r) => html`<${PriceRow} r=${r} />`)}
        ${v.age && html`<${AgeSet} a=${v.age} onSet=${(y) => act('bus_age', { id: d.id, year: y })} />`}
        <div class=${'co-para t-' + v.tone} style="font-weight:700;margin-top:10px;line-height:26px">${v.status}</div>
        ${v.crew && html`<${BusCrew} c=${v.crew} />`}</div>
    </div>
  </${Dialog}>`;
}

/** A bus's handover notes of today and its defects (`fleet::crew_json`). */
export function BusCrew({ c }) {
  return html`<div class="co-crew">
    <div class="co-crew-h">${c.notes_title}</div>
    ${c.notes.length
      ? c.notes.map((n) => html`<div class="co-crew-row"><${Icon} name="sticky_note_2" size=${16} /><span class="co-crew-what">${n.says}</span><span class="co-crew-when">${n.at} · ${n.by}</span></div>`)
      : html`<div class="co-crew-none">${c.no_notes}</div>`}
    <div class="co-crew-h">${c.defects_title}</div>
    ${c.defects.length
      ? c.defects.map((d) => html`<${DefectRow} d=${d} />`)
      : html`<div class="co-crew-none">${c.no_defects}</div>`}
  </div>`;
}

/** A defect: what, when and who, and where the workshop is with it. */
export function DefectRow({ d, bus }) {
  return html`<div class="co-crew-row">
    <${Icon} name=${d.icon} size=${16} />
    <span class="co-crew-what"><b>${bus ? d.bus + ' · ' : ''}${d.kind}</b>${d.text ? ': ' + d.text : ''}<small>${d.date} ${d.at} · ${d.who}${d.held_text ? ' · ' + d.held_text : ''}</small></span>
    <span class=${'co-crew-state t-' + d.tone}>${d.state}</span>
  </div>`;
}

/** Leasing or renting one of the dealer's buses (`Dialog::New`). */
export function NewDialog({ d }) {
  const v = useView('new', { file: d.file, livery: d.livery || '', how: d.how || 0, days: d.days || 7 });
  const put = (patch) => setCo({ dialog: { ...d, ...patch } });
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  if (v.gone) return null;
  const take = async () => {
    const r = await act('new_take', { file: d.file, livery: d.livery || '', how: d.how || 0, days: d.days || 7 });
    if (r?.joined != null) {
      setUi('fleet', { tab: 0 });
      close();
    }
  };
  const foot = html`<span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} /><${Btn} kind="primary" icon="check_circle" label=${v.main} onClick=${take} />`;
  return html`<${Dialog} icon="directions_bus" title=${v.title} w=${920} h=${600} onClose=${close} foot=${foot}>
    <div class="co-busdialog">
      <${BusSide} file=${v.file} name=${v.name} kind=${v.bus_kind} livery=${v.livery} onLivery=${(p) => put({ livery: p })} note=${v.design_note} />
      <div class="right">
        <${Seg} value=${v.how} options=${v.hows.map((h, k) => [k, h])} onChange=${(k) => put({ how: k })} style="height:40px;margin-bottom:18px" />
        ${v.how === 0
          ? html`${v.rows.map((r) => html`<${PriceRow} r=${r} />`)}<p class="co-para" style="margin-top:12px">${v.text}</p>`
          : html`<${PriceRow} r=${v.rows[0]} />
            <div class="co-days"><span>${v.days_label}</span><input type="range" min="1" max="60" step="1" value=${v.days} style=${`--fill:${((v.days - 1) / 59) * 100}%`} onInput=${(e) => put({ days: Number(e.currentTarget.value) })} /><b>${v.days}</b></div>
            <${PriceRow} r=${v.rows[1]} /><p class="co-para" style="margin-top:12px">${v.text}</p>`}
      </div>
    </div>
  </${Dialog}>`;
}

export { tr };

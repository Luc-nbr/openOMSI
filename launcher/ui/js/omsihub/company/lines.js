// The company's lines, as our game-drawn launcher's (`company/lines.rs`): its lines on the left
// (the plate, the title, its tags, the fare, how well today is covered); on the right the line
// chosen - how it shows in public, its depot file and colour, its tours today - or the lines to
// add; and the dialog that weighs a line's needs against what the company has.

import { S } from '../../store.js';
import { html, useState, setCo, setUi, ui, act, useView, goTab, goKey, handOver, Icon, Btn, Seg, Select, Dialog, Plate, IconBtn, Tag, rgbOf, seen } from './kit.js';
import { Bar } from './overview.js';

export function LinesPage() {
  const st = ui('lines', { selected: null, mine: false });
  const v = useView('lines', st);
  if (!v?.ours) return null;
  return html`<div style="display:flex;flex-direction:column;height:100%">
    <div style="flex:1;min-height:0;display:grid;grid-template-columns:max(320px, calc((100% - 16px) * 0.52)) minmax(0,1fr);gap:16px;padding-bottom:12px">
      <${Ours} o=${v.ours} />
      ${v.right.tours ? html`<${Tours} t=${v.right.tours} name=${v.selected} />` : html`<${ToAdd} a=${v.right.add} mine=${st.mine} />`}
    </div>
    <div class="co-note t-dim cut" style="flex:none;height:24px;line-height:24px">${v.foot}</div>
  </div>`;
}

/** Where a line's bar leads (`lines::ours`): its timetable, the line editor, the planning (in
 * Casual the line itself). */
function barClick(row) {
  switch (row.bar.click) {
    case 'timetable':
      return handOver('timetable=' + row.name);
    case 'edit':
      return handOver(row.edit_to || 'line-editor');
    case 'select':
      return setUi('lines', { selected: row.name });
    default:
      return handOver('5');
  }
}

/** The company's lines (`lines::ours`). */
function Ours({ o }) {
  return html`<section class="co-section">
    <div class="co-caps" style="margin-bottom:14px">${o.title}</div>
    ${o.empty
      ? html`<p class="co-para">${o.empty}</p>`
      : html`<div class="co-scroll" style="flex:1;min-height:0">${o.rows.map(
          (r) => html`<div class=${'co-linerow' + (r.on ? ' on' : '')} onClick=${() => setUi('lines', { selected: r.on ? null : r.name })} data-line=${r.name}>
            <div class="top"><${Plate} p=${r.plate} h=${26} />
              <b class="cut grow">${r.caption}</b>
              ${r.own && html`<span title=${r.own[1]}><${Tag} text=${r.own[0]} tone=${r.on ? 'onaccent' : 'accent2'} /></span>`}
              ${r.pending && html`<span title=${r.pending_tip}><${Tag} text=${r.pending} tone=${r.on ? 'onaccent' : 'warn'} /></span>`}
              <span onClick=${(e) => e.stopPropagation()} style="display:flex;align-items:center;gap:2px">
                <${Btn} kind="ghost" label=${r.fare} title=${r.fare_tip} style="height:28px;padding:0 11px" onClick=${() => setCo({ dialog: { kind: 'fare', line: r.name } })} />
                ${r.edit && html`<${IconBtn} icon="route" title=${r.edit} size=${16} style="width:32px;height:32px" onClick=${() => handOver(r.edit_to || 'line-editor')} />`}
                ${r.timetable && html`<${IconBtn} icon="departure_board" title=${r.timetable} size=${16} style="width:32px;height:32px" onClick=${() => handOver('timetable=' + r.name)} />`}
                <${IconBtn} icon="close" title=${r.remove_tip} size=${16} style="width:32px;height:32px" onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'remove_line', name: r.name } })} />
              </span>
            </div>
            <div class="bottom"><span style="width:66%" onClick=${(e) => (e.stopPropagation(), barClick(r))}><${Bar} b=${r.bar} onClick=${() => {}} /></span><span class="km cut">${r.km}</span></div>
          </div>`,
        )}</div>`}
  </section>`;
}

/** The line chosen (`lines::tours_of`): how it shows in public, and its tours today. */
function Tours({ t, name }) {
  if (!t) return null;
  return html`<div style="display:flex;flex-direction:column;min-height:0;gap:12px">
    <${Card} c=${t.card} name=${name} />
    <section class="co-section" style="flex:1;min-height:80px">
      <div class="co-caps" style="display:flex;align-items:center;margin:-6px 0 8px">
        <span class="grow">${t.title}</span>
        <${Btn} kind="ghost" icon="add" label=${t.add} style="height:32px" onClick=${() => setUi('lines', { selected: null })} />
        ${t.planning && html`<${Btn} kind="ghost" icon="event" label=${t.planning} style="height:32px" onClick=${() => handOver('5')} />`}
      </div>
      ${t.reading
        ? html`<p class="co-para">${t.reading}</p>`
        : t.none
          ? html`<p class="co-para">${t.none}</p>`
          : html`<div class="co-scroll" style="flex:1;min-height:0">${t.rows.map(
              (r) => html`<div class="co-tourrow"><div><b>${r.head}</b><span class="cut">${r.when}</span></div>
                <div><span class="who cut">${r.who}</span><b class=${'cut t-' + r.tone}>${r.what}</b></div></div>`,
            )}</div>`}
    </section>
  </div>`;
}

/** The line in public (`lines::line_card`): its title, its depot file, and how it is
 * advertised with its colour. */
function Card({ c, name }) {
  const [title, setTitle] = useState({ for: name, text: c.public });
  const text = title.for === name ? title.text : c.public;
  const dirty = text.trim() !== c.public.trim();
  const hofK = Math.max(0, c.hofs.findIndex(([h]) => h.toLowerCase() === c.hof.toLowerCase()));
  const ad = rgbOf(c.advert_colour);
  return html`<section class="co-section co-linecard">
    <div class="co-caps" style="margin-bottom:14px">${c.title}</div>
    <div class="halves">
      <div>
        <div class="co-caps">${c.public_caps}</div>
        <div style="display:flex;gap:6px;margin-top:4px"><input class="co-field" style="height:34px" value=${text} placeholder=${c.hint} onInput=${(e) => setTitle({ for: name, text: e.currentTarget.value })} />
          ${dirty && html`<${Btn} kind="primary" label=${c.save} style="width:70px;height:34px;padding:0" onClick=${() => act('line_title', { name, text })} />`}</div>
        <div class="co-caps" style="margin-top:10px">${c.hof_caps}</div>
        <${Select} value=${hofK} h=${34} options=${c.hofs.map(([, l]) => l)} onChange=${(k) => act('line_hof', { name, hof: c.hofs[k][0] })} style="margin-top:4px" />
        ${c.hof_words && html`<p class=${'co-note t-' + c.hof_tone} style="margin-top:6px">${c.hof_words}</p>`}
      </div>
      <div>
        <div class="co-caps">${c.advert_caps}</div>
        <div class="advert" style=${`background:rgba(${ad.join(',')},${seen(0.16, ad)})`}>
          <div style="display:flex;gap:10px"><${Plate} p=${c.plate} h=${30} /><div style="min-width:0"><b>${text.trim() || c.public_name}</b><p class="co-note">${c.under}</p></div></div>
          <div class="colours"><span class="co-caps">${c.colour_caps}</span>
            ${c.palette.map((hex) => html`<span class=${'sw' + (hex === c.colour ? ' on' : '')} style=${`background:${hex}`} onClick=${() => act('line_colour', { name, colour: hex })}></span>`)}
            <label class="co-iconbtn" title=${c.more_tip} style="width:24px;height:24px;margin-left:auto;position:relative"><${Icon} name="palette" size=${14} />
              <input type="color" value=${c.colour} style="position:absolute;inset:0;opacity:0;cursor:pointer" onChange=${(e) => act('line_colour', { name, colour: e.currentTarget.value })} /></label>
          </div>
        </div>
      </div>
    </div>
  </section>`;
}

/** The lines to add (`lines::to_add`): the map's or one's own, each with its tours and the way
 * to add it (or to apply for its concession). */
function ToAdd({ a, mine }) {
  if (!a) return null;
  return html`<section class="co-section">
    <div class="co-caps" style="display:flex;align-items:center;margin:-6px 0 8px"><span class="grow">${a.title}</span>
      <${Btn} kind="ghost" icon="route" label=${a.make} title=${a.make_tip} style="height:32px" onClick=${() => handOver('line-editor')} /></div>
    ${a.reading
      ? html`<p class="co-para">${a.reading}</p>`
      : a.error
        ? html`<p class="co-para t-warn">${a.error}</p>`
        : html`<div title=${a.switch.none ? a.switch.none_tip : ''} style="max-width:460px">
            <${Seg} value=${a.switch.mine ? 1 : 0} options=${a.switch.labels.map((l, k) => [k, l])} onChange=${(k) => (k === 1 && a.switch.none ? setCo({ notice: { text: a.switch.none_tip, at: Date.now() } }) : setUi('lines', { mine: k === 1 }))} style="height:40px" /></div>
          ${a.all_run
            ? html`<p class="co-para" style="margin-top:14px">${a.all_run}</p>`
            : html`<div class="co-scroll" style="flex:1;min-height:0;margin-top:14px">${(a.rows || []).map(
                (r) => html`<div class="co-addrow"><${Plate} p=${r.plate} h=${24} />
                  <div class="grow"><b class="cut">${r.caption}</b><span class="cut">${r.sub}</span></div>
                  <${Btn} icon=${r.icon} label=${r.label} title=${r.tip} style="height:36px" onClick=${() => setCo({ dialog: { kind: 'add_line', name: r.name } })} /></div>`,
              )}</div>`}
          ${a.hidden && html`<div class="co-note t-dim cut" style="flex:none;line-height:22px;margin-top:6px">${a.hidden}</div>`}`}
  </section>`;
}

/** Taking a line on (`lines::add_dialog`): what it needs against what the company has. */
export function AddLineDialog({ d }) {
  const v = useView('add_line', { name: d.name });
  const close = () => setCo({ dialog: null });
  if (!v) return null;
  if (v.gone) {
    close();
    return null;
  }
  const go = async () => {
    const r = await act('add_line', { name: d.name });
    if (r?.done) {
      close();
      if (r.selected) setUi('lines', { selected: r.selected });
    }
  };
  const foot = html`${v.dealer && html`<${Btn} icon="directions_bus" label=${v.dealer} onClick=${() => goKey('dealer')} />`}
    ${v.hire && html`<${Btn} icon="groups" label=${v.hire} onClick=${() => goKey('hire')} />`}
    <span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} />
    <${Btn} kind="primary" icon=${v.main_icon} label=${v.main} onClick=${go} />`;
  return html`<${Dialog} icon="route" title=${v.title} w=${760} h=${600} onClose=${close} foot=${foot}>
    <p class="co-para" style="margin-bottom:18px">${v.intro}</p>
    <div class="co-needs head"><span>${v.needs_caps}</span><span>${v.need_caps}</span><span>${v.have_caps}</span></div>
    ${v.rows.map(([what, need, have, ok]) => html`<div class="co-needs"><span class="cut" style="white-space:pre">${what}</span><b>${need}</b><b class=${ok ? 't-ok' : 't-warn'}>${have}</b></div>`)}
    <div style="height:10px"></div>
    ${v.short && html`<div class="co-iconline t-warn" style="font-size:var(--co-body)"><${Icon} name="warning" size=${18} /><span>${v.short}</span></div>`}
    ${v.fine && html`<div class="co-iconline t-ok" style="font-size:var(--co-body)"><${Icon} name="check_circle" size=${18} /><span>${v.fine}</span></div>`}
    ${v.asking && html`<p class=${'co-para ' + (v.asking.ok ? '' : 't-warn')} style="margin-top:8px">${v.asking.text}</p>`}
  </${Dialog}>`;
}

export { S, goTab };

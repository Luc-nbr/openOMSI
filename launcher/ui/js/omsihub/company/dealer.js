// The dealer, as our game-drawn launcher's (`company/dealer.rs`, `bank.rs`, `adverts.rs`,
// `signing.rs`): the showroom (brands, models and versions as tiles, the filters beside them),
// the day's offers, the used market, the orders and contracts - and the dealer's sheets over the
// page: a model or an offer (quick buy, or the talk with the dealer), the haggling, the purchase
// contract with its signature, the bank's loan and its contract, paying back early, an
// advertiser's contract, and the way back from a test drive.

import { S } from '../../store.js';
import { start } from '../../data.js';
import { html, useEffect, useRef, useState, tr, setCo, setUi, ui, act, useView, useSize, textW, Icon, Btn, Seg, Chips, Select, Dialog, Meter, Slider, Crumbs, T } from './kit.js';
import { layout, BusTile, PriceRow, Photo, AgeSet } from './fleet.js';
import { Bar } from './overview.js';

const PRICE_ANY = 1000;

export function DealerPage() {
  const st = ui('dealer', { tab: 0, level: [], search: '', size: 0, drive: 0, max_price: PRICE_ANY });
  const v = useView('dealer', st);
  const [ref, size] = useSize();
  if (!v?.tabs) return html`<div ref=${ref}></div>`;
  const tw = Math.max(320, Math.min(820, size.w - 380));
  let body = null;
  if (v.reading) body = html`<div class="co-para" style="font-weight:500">${v.reading}</div>`;
  else if (v.body?.kind === 'showroom') body = html`<${Showroom} s=${v.body} st=${st} />`;
  else if (v.body?.kind === 'offers') body = html`<${Offers} o=${v.body} />`;
  else if (v.body?.kind === 'orders') body = html`<${Orders} o=${v.body} />`;
  return html`<div ref=${ref} style="height:100%;display:flex;flex-direction:column">
    <div style="display:flex;align-items:center;gap:10px;flex:none">
      <${Seg} value=${v.tab} options=${v.tabs.map((t, k) => [k, t])} onChange=${(k) => setUi('dealer', { tab: k })} style=${`width:${tw}px;height:40px`} />
      <span class="grow"></span>
      ${v.buying && html`<span class="co-caps" style="width:90px;text-align:right">${v.buying.label}</span>
        <span title=${v.buying.tip}><${Seg} value=${v.buying.mode} options=${v.buying.modes.map((m, k) => [k, m])} onChange=${(k) => act('buying_mode', { mode: k })} style="width:250px;height:40px" /></span>`}
    </div>
    <div style="flex:1;min-height:0;margin-top:16px">${body}</div>
  </div>`;
}

/** The showroom (`dealer::showroom`): the filters on the left, the crumbs and the tiles of the
 * level on the right. */
function Showroom({ s, st }) {
  const [ref, size] = useSize();
  const f = s.filters;
  const L = layout(Math.max(0, size.w - 12));
  const gap = 14;
  const open = (card) => {
    if (card.to.level) setUi('dealer', { level: card.to.level });
    else act('open_model', { file: card.to.bus });
  };
  return html`<div style="display:grid;grid-template-columns:min(270px, 30%) minmax(0,1fr);gap:24px;grid-template-rows:minmax(0,1fr);height:100%">
    <div class="co-filters">
      <label class="co-search"><${Icon} name="search" size=${18} /><input class="co-field" value=${st.search} placeholder=${f.search} onInput=${(e) => setUi('dealer', { search: e.currentTarget.value })} /></label>
      <label class="co-label" style="margin-top:18px">${f.size_label}</label>
      <div style="margin-top:4px"><${Chips} value=${st.size} options=${f.sizes.map((x, k) => [k, x])} onChange=${(k) => setUi('dealer', { size: k })} /></div>
      <label class="co-label" style="margin-top:16px">${f.drive_label}</label>
      <div style="margin-top:4px"><${Chips} value=${st.drive} options=${f.drives.map((x, k) => [k, x])} onChange=${(k) => setUi('dealer', { drive: k })} /></div>
      <label class="co-label" style="margin-top:16px">${f.price_label}</label>
      <${Slider} value=${st.max_price} min=${100} max=${PRICE_ANY} step=${10} format=${(x) => (x >= PRICE_ANY ? f.any : `${x}k`)} onChange=${(x) => setUi('dealer', { max_price: x })} style="margin-top:2px" />
    </div>
    <div style="display:flex;flex-direction:column;min-width:0">
      <div style="display:flex;align-items:center;gap:10px;height:28px;flex:none">
        <${Crumbs} parts=${s.crumbs} style="flex:1;min-width:0" onPick=${(k) => setUi('dealer', s.searching ? { search: '', level: [] } : { level: st.level.slice(0, k) })} />
        <span class="co-note" style="width:230px;text-align:right;flex:none">${s.count}</span>
      </div>
      ${s.cards.length
        ? html`<div class="co-scroll" ref=${ref} style="flex:1;min-height:0;margin-top:14px">
            <div style=${`position:relative;height:${Math.ceil(s.cards.length / L.cols) * (L.th + gap)}px`}>
              ${size.w > 0 &&
              s.cards.map((card, k) => {
                const r = { x: (k % L.cols) * (L.tw + gap), y: Math.floor(k / L.cols) * (L.th + gap), w: L.tw, h: L.th };
                return html`<${BusTile} name=${card.name} file=${card.file} r=${r} onClick=${() => open(card)} attrs=${{ 'data-card': card.key }}
                  over=${card.badge && html`<span class=${'co-tilebadge t-' + card.badge[1]}>${card.badge[0]}</span>`}>
                  <b class="cut">${card.title}</b><span class="sub cut" style="font-size:10.75px;height:26px;line-height:22px">${card.sub}</span>
                  <span class="price">${card.price}</span>
                </${BusTile}>`;
              })}
            </div></div>`
        : html`<p class="co-para" style="max-width:700px;margin-top:14px" ref=${ref}>${s.empty}</p>`}
    </div>
  </div>`;
}

/** The day's offers or the used market (`dealer::offer_grid`). */
function Offers({ o }) {
  const [ref, size] = useSize();
  const L = layout(Math.max(0, size.w - 12));
  const gap = 14;
  return html`<div style="display:flex;flex-direction:column;height:100%">
    <p class="co-para" style="max-width:1000px;flex:none">${o.note}</p>
    ${o.cards.length
      ? html`<div class="co-scroll" ref=${ref} style="flex:1;min-height:0;margin-top:14px">
          <div style=${`position:relative;height:${Math.ceil(o.cards.length / L.cols) * (L.th + gap)}px`}>
            ${size.w > 0 &&
            o.cards.map((card, k) => {
              const r = { x: (k % L.cols) * (L.tw + gap), y: Math.floor(k / L.cols) * (L.th + gap), w: L.tw, h: L.th };
              return html`<${BusTile} name=${card.name} file=${card.file} r=${r} onClick=${() => act('open_offer', { id: card.id })} attrs=${{ 'data-offer': card.id }}
                over=${card.badge && html`<span class="co-tilebadge t-accent">${card.badge}</span>`}>
                <b class="cut">${card.name}</b><span class="sub cut">${card.line}</span>
                <span class="offerprice"><span class="price">${card.price}</span>${card.saving && html`<span class="t-ok save">${card.saving}</span>`}
                  ${card.condition && html`<span class="cond" title=${card.condition.tip}><${Meter} frac=${card.condition.frac} tone=${card.condition.tone} /></span>`}</span>
                <span class="who cut">${card.who}</span>
              </${BusTile}>`;
            })}
          </div></div>`
      : html`<div class="co-para" style="font-weight:500;margin-top:14px" ref=${ref}>${o.empty}</div>`}
  </div>`;
}

/** The orders and the signed contracts (`dealer::orders`). */
function Orders({ o }) {
  return html`<div style="display:flex;flex-direction:column;gap:16px;height:100%">
    <section class="co-section" style=${`flex:none;max-height:45%;height:${70 + 46 * Math.max(1, o.on_order.length)}px`}>
      <div class="co-caps" style="margin-bottom:14px">${o.on_order_title}</div>
      ${o.on_order.length
        ? o.on_order.map((x) => html`<div class="co-orderrow"><${Icon} name="schedule" size=${18} class="t-accent2" /><b class="cut">${x.what}</b><span class="cut t-soft">${x.no}</span><span class="when cut">${x.when}</span></div>`)
        : html`<p class="co-para">${o.none}</p>`}
    </section>
    <section class="co-section" style="flex:1;min-height:0">
      <div class="co-caps" style="margin-bottom:14px">${o.contracts_title}</div>
      ${o.contracts.length
        ? html`<div class="co-scroll" style="flex:1;min-height:0">${o.contracts.map(
            (k) => html`<button type="button" class="co-row co-contractrow" title=${o.open_tip} onClick=${() => act('open_contract', { no: k.no })}>
              <${Icon} name="description" size=${18} class="t-dim" /><b class="no">${k.no}</b><span class="date cut">${k.date}</span><b class="what cut">${k.what}</b><span class="seller cut">${k.seller}</span><b class="amount cut">${k.amount}</b></button>`,
          )}</div>`
        : html`<p class="co-para">${o.no_contracts}</p>`}
    </section>
  </div>`;
}

// --- the sheets -----------------------------------------------------------------------------

/** The dealer's sheet open (`dealer::dialog`). */
export function DealerSheet() {
  const v = useView('dealer_sheet');
  const [pen, setPen] = useState(null);
  if (!v || !v.kind) return null;
  const close = () => act('sheet_close');
  const did = async (r) => {
    if (!r) return;
    if (r.new) setCo({ dialog: { kind: 'new', file: r.new.file, livery: r.new.livery, how: 0, days: 7 } });
    if (r.test_drive && !S.boot?.selftest) start(r.test_drive);
    if (r.pen) setPen(r.pen);
  };
  const sheet = (name, args = {}) => act('sheet_' + name, args).then(did);
  const body = { model: ModelSheet, offer: ModelSheet, talk: TalkSheet, contract: ContractSheet, back: BackSheet, loan: LoanSheet, loan_contract: LoanContractSheet, repay: RepaySheet, advert: AdvertSheet }[v.kind];
  return html`${body && html`<${body} v=${v} sheet=${sheet} close=${close} />`}
    ${pen && html`<${Pen} pen=${pen} onDone=${() => (setPen(null), sheet('book'))} />`}`;
}

/** The photo's place and the livery on a sheet's left (`fleet::bus_side`). */
function Side({ v, sheet, w = 290 }) {
  const k = Math.max(0, v.livery.options.findIndex(([p]) => p === v.livery.value));
  return html`<div class="co-busside" style=${`width:${w}px`}>
    <${Photo} file=${v.file} paint=${v.livery.value} name=${v.name} style=${`width:${w}px;height:${w * 0.62}px`} />
    <div class="kind">${v.bus_kind}</div>
    <label class="co-label" style="margin-top:10px">${v.livery.label}</label>
    <${Select} value=${k} options=${v.livery.options.map(([, l]) => l)} onChange=${(i) => sheet('set', { livery: v.livery.options[i][0] })} style="margin-top:4px" />
    <p class="co-note" style="margin-top:12px">${v.design_note}</p>
  </div>`;
}

/** A line with an icon in its tone (the standing with the dealer, the order's step). */
const IconLine = ({ l }) => l && html`<div class=${'co-iconline t-' + l.tone}><${Icon} name=${l.icon} size=${17} /><span>${l.text}</span></div>`;

/** A model of the showroom or one of the day's offers (`dealer::model_sheet`, `offer_sheet`):
 * what it is, and the quick buy or the way to the talk. */
function ModelSheet({ v, sheet, close }) {
  const sideW = Math.max(280, Math.min(340, (1120 - 56) * 0.3));
  const foot = html`
    <${Btn} icon="sports_score" label=${v.test} title=${v.test_tip} onClick=${() => sheet('test')} />
    ${v.rent && html`<${Btn} icon="schedule" label=${v.rent} onClick=${() => sheet('rent')} />`}
    <span class="rest"></span>
    <${Btn} label=${v.close} onClick=${close} />
    <${Btn} icon=${v.quick ? 'forum' : 'payments'} label=${v.other} onClick=${() => sheet('set', { quick: !v.quick })} />
    <${Btn} kind="primary" icon=${v.quick ? 'payments' : 'forum'} label=${v.main} onClick=${() => sheet('main')} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${1120} h=${760} onClose=${close} foot=${foot}>
    <div style=${`display:grid;grid-template-columns:${sideW}px minmax(0,1fr);gap:30px`}>
      <${Side} v=${v} sheet=${sheet} w=${sideW} />
      <div>
        ${v.rows.map((r) => html`<${PriceRow} r=${r} h=${33} />`)}
        ${v.age && html`<${AgeSet} a=${v.age} onSet=${(y) => sheet('set', { age: y })} />`}
        <div style="height:16px"></div>
        ${v.lock && html`<div class="co-iconline t-warn" style="font-size:var(--co-body)"><${Icon} name="lock" size=${18} /><span>${v.lock}</span></div>`}
        ${v.max > 1 && html`<${Slider} label=${v.count_label} value=${v.count} min=${1} max=${v.max} onChange=${(n) => sheet('set', { count: n })} style="height:36px;margin-bottom:10px" />`}
        ${v.quick && html`<${Seg} value=${v.pay} options=${v.pays.map((p, k) => [k, p])} onChange=${(k) => sheet('set', { pay: k })} style="height:38px;margin-bottom:10px" />`}
        <${IconLine} l=${v.standing} /><${IconLine} l=${v.bulk} />
        ${v.quick
          ? v.price_rows.map((r) => html`<${PriceRow} r=${r} h=${r.strong ? 36 : 33} />`)
          : html`<p class="co-para" style="margin-bottom:10px">${v.talk_note}</p>${v.sulk && html`<p class="co-para t-warn" style="font-weight:500">${v.sulk}</p>`}`}
      </div>
    </div>
  </${Dialog}>`;
}

/** Haggling with the dealer (`dealer::talk_sheet`): the bus and what is asked now, the rounds
 * and the dealer's patience on the left; the talk, the moves and the extras on the right. */
function TalkSheet({ v, sheet, close }) {
  const [bid, setBid] = useState('');
  const log = useRef(null);
  useEffect(() => {
    if (log.current) log.current.scrollTop = 1e6;
  }, [v.said.length]);
  const lw = Math.max(290, Math.min(340, (1180 - 56) * 0.3));
  const rw = 1180 - 56 - lw - 28;
  const ew = (rw - (v.extras.length - 1) * 10) / v.extras.length;
  const worded = v.extras.every((e) => Math.max(116, textW(e.label, 14) + 14 * 1.3 + 6 + 44) <= ew);
  const offer = () => {
    sheet('bid', { bid }).then(() => setBid(''));
  };
  const foot = html`<span class="rest"></span><${Btn} label=${v.leave} onClick=${close} />
    ${v.deal && html`<${Btn} kind="primary" icon="description" label=${v.deal} onClick=${() => sheet('deal')} />`}`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${1180} h=${820} onClose=${close} foot=${foot} bodyStyle="overflow:hidden">
    <div style=${`display:grid;grid-template-columns:${lw}px minmax(0,1fr);gap:28px;grid-template-rows:minmax(0,1fr);height:100%`}>
      <div class="co-talkside">
        <${Photo} file=${v.file} paint=${v.paint} name=${v.name} mono=${false} style=${`height:${lw * 0.52}px`} />
        <b class="what">${v.what}</b>
        <${PriceRow} r=${v.first} h=${33} />
        <div class="co-caps" style="margin-top:9px">${v.asks_caps}</div>
        <div class="asking"><span class=${v.agreed ? 't-ok' : ''}>${v.asking}</span>${v.off && html`<b class="t-ok">${v.off}</b>`}</div>
        <${Bar} b=${v.rounds} /><div style="height:16px"></div>
        <${Bar} b=${v.mood} /><div style="height:14px"></div>
        <${IconLine} l=${v.standing} /><${IconLine} l=${v.bulk} />
        ${v.extras_caps && html`<div class="co-caps" style="margin:4px 0 8px">${v.extras_caps}</div>
          ${v.agreed_extras.map(([icon, label]) => html`<div class="co-iconline" style="color:var(--hub-text)"><${Icon} name=${icon} size=${18} class="t-ok" /><span style="font-weight:500">${label}</span></div>`)}`}
      </div>
      <div style="display:flex;flex-direction:column;min-height:0">
        <div class="co-talklog" ref=${log}>
          ${v.said.map((m) => html`<div class=${'msg' + (m.you ? ' you' : '')}><span class="who">${m.you ? v.you : v.seller}</span><div class="bubble">${m.text}</div></div>`)}
        </div>
        ${v.open
          ? html`<div class="co-talkmoves">
              <div class="row"><${Btn} icon="trending_up" label=${v.ask} onClick=${() => sheet('ask')} />
                <label class="co-search grow" title=${v.bid_tip}><${Icon} name="payments" size=${18} /><input class="co-field" value=${bid} placeholder=${v.bid_placeholder} onInput=${(e) => setBid(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && bid.trim() && offer()} /></label>
                <${Btn} label=${v.offer} onClick=${offer} /></div>
              <div class="row">${v.extras.map((e) => html`<${Btn} icon=${e.icon} label=${worded ? e.label : ''} title=${worded ? e.tip : `${e.label}\n${e.tip}`} style=${`width:${ew}px;padding:0 8px`} onClick=${() => !e.have && sheet('extra', { extra: e.key })} />`)}</div>
            </div>`
          : html`<p class="co-para" style="color:var(--hub-text);font-weight:500;margin-top:20px">${v.closed_text}</p>`}
      </div>
    </div>
  </${Dialog}>`;
}

/** The contract's parties (`dealer::parties`). */
const Parties = ({ p }) => html`<div class="co-parties">${p.map((x) => html`<div><div class="co-caps">${x.role}</div><b class="cut">${x.name}</b><span class="cut">${x.under}</span></div>`)}</div>`;

/** The signature's field (`dealer::signature`): drawn with the mouse, or a name typed; the day
 * beside it. `onChange` gets the strokes (in the field's own 0..1) and the name. */
function Signature({ s, onChange }) {
  const [strokes, setStrokes] = useState(s.strokes || []);
  const [name, setName] = useState(s.name || '');
  const pad = useRef(null);
  const drawing = useRef(false);
  const W = 460;
  const H = 108;
  const point = (e) => {
    const r = pad.current.getBoundingClientRect();
    return [Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)), Math.max(0, Math.min(1, (e.clientY - r.top) / r.height))];
  };
  const down = (e) => {
    if (s.readonly) return;
    drawing.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    setStrokes((x) => [...x, [point(e)]]);
  };
  const move = (e) => {
    if (!drawing.current) return;
    const p = point(e);
    setStrokes((x) => {
      const last = x[x.length - 1];
      const q = last[last.length - 1];
      if (Math.hypot((p[0] - q[0]) * W, (p[1] - q[1]) * H) <= 1.5) return x;
      return [...x.slice(0, -1), [...last, p]];
    });
  };
  const up = () => {
    if (!drawing.current) return;
    drawing.current = false;
    setStrokes((x) => {
      const kept = x.filter((st) => st.length > 1);
      onChange(kept, name);
      return x;
    });
  };
  const shown = strokes.filter((st) => st.length > 1);
  return html`<div class="co-signature">
    <div class=${'pad' + (s.readonly ? '' : ' live')} ref=${pad} onPointerDown=${down} onPointerMove=${move} onPointerUp=${up} onPointerCancel=${up}>
      <svg viewBox=${`0 0 ${W} ${H}`} preserveAspectRatio="none">${shown.map((st) => html`<polyline points=${st.map(([x, y]) => `${x * W},${y * H}`).join(' ')} />`)}</svg>
      ${!shown.length && html`<span class="hint">${s.hint}</span>`}
    </div>
    <div class="who">
      <div class="co-caps">${s.caps}</div>
      ${s.readonly
        ? html`<b class="cut">${s.name?.trim() ? s.name : '–'}</b><span class="day">${s.day}</span>`
        : html`<input class="co-field" style="margin-top:6px" value=${name} placeholder=${s.placeholder} onInput=${(e) => setName(e.currentTarget.value)} onChange=${(e) => onChange(shown, e.currentTarget.value)} />
          <div style="display:flex;align-items:center;gap:12px;margin-top:10px"><${Btn} label=${s.clear} style="height:36px" onClick=${() => (setStrokes([]), setName(''), onChange([], ''))} /><span class="day grow" style="text-align:right">${s.day}</span></div>`}
    </div>
  </div>`;
}

/** The purchase contract (`dealer::contract_sheet`). */
function ContractSheet({ v, sheet, close }) {
  const sig = useRef({ strokes: v.signature.strokes, name: v.signature.name });
  const keep = (strokes, name) => {
    sig.current = { strokes, name };
    sheet('set', { strokes, name });
  };
  const foot = v.readonly
    ? html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`
    : html`<span class=${'rest' + (v.armed ? ' t-warn' : '')} style="font-size:var(--co-note)">${v.armed ? v.armed_text : sig.current.strokes.length || sig.current.name.trim() ? '' : v.unsigned}</span>
      <${Btn} kind=${v.armed ? 'danger' : ''} label=${v.cancel} onClick=${close} />
      <${Btn} kind="primary" icon="check_circle" label=${v.sign} onClick=${() => sheet('sign', sig.current)} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${1060} h=${860} onClose=${close} foot=${foot} bodyStyle="display:flex;flex-direction:column">
    <${Parties} p=${v.parties} />
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:24px;flex:1;min-height:0">
      <div>
        <div class="co-caps" style="margin-bottom:8px">${v.bus_caps}</div>
        ${v.bus.map((r) => html`<${PriceRow} r=${r} h=${31} />`)}
        ${v.delivery && html`<${Seg} value=${v.delivery.value} options=${v.delivery.options.map((o, k) => [k, o])} onChange=${(k) => sheet('set', { quick: k === 1 })} style="height:36px;margin:4px 0 6px" />`}
        ${v.after.map((r) => html`<${PriceRow} r=${r} h=${31} />`)}
        <div class="co-caps" style="margin-top:10px">${v.extras_caps}</div>
        <p style="font-size:var(--co-rows);margin-top:6px">${v.extras}</p>
      </div>
      <div>
        <div class="co-caps" style="margin-bottom:8px">${v.price_caps}</div>
        ${v.price.map((r) => html`<${PriceRow} r=${r} h=${31} />`)}
        <div style="height:8px"></div>
        ${v.ways && html`<${Seg} value=${v.pay} options=${v.ways} onChange=${(w) => sheet('set', { pay: w })} style="height:38px;margin-bottom:10px" />`}
        ${v.pay_rows.map((r) => html`<${PriceRow} r=${r} h=${31} />`)}
      </div>
    </div>
    <${Signature} s=${v.signature} onChange=${keep} />
  </${Dialog}>`;
}

/** Back from a test drive (`dealer::back_sheet`). */
function BackSheet({ v, sheet, close }) {
  const foot = html`<span class="rest"></span><${Btn} label=${v.no} onClick=${close} /><${Btn} kind="primary" icon="directions_bus" label=${v.yes} onClick=${() => sheet('yes')} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${600} onClose=${close} foot=${foot}><p class="co-para">${v.text}</p></${Dialog}>`;
}

/** A loan from the bank (`bank::loan_sheet`). */
function LoanSheet({ v, sheet, close }) {
  const foot = html`<span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} /><${Btn} kind="primary" icon="description" label=${v.main} onClick=${() => sheet('main')} />`;
  const k = v.presets.findIndex(([p]) => p === v.amount);
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${860} h=${v.fixed ? 640 : 740} onClose=${close} foot=${foot}>
    ${v.for && html`<p class="co-para" style="margin-bottom:10px">${v.for}</p>`}
    <div class="co-caps">${v.amount_caps}</div>
    <div class="co-loanamount"><b>${v.amount_text}</b><span class=${'t-' + v.left_tone}>${v.left}</span></div>
    ${!v.fixed && html`<${Slider} value=${Math.round(v.amount / 100)} min=${Math.round(v.min / 100)} max=${Math.round(v.max / 100)} step=${5000} format=${(x) => `${Math.round(x / 1000)}k`} onChange=${(x) => sheet('set', { amount: x * 100 })} style="height:36px;margin:6px 0 10px" />
      <${Chips} value=${k} options=${v.presets.map(([, l], i) => [i, l])} onChange=${(i) => sheet('set', { amount: v.presets[i][0] })} /><div style="height:16px"></div>`}
    <label class="co-label">${v.term_label}</label>
    <div style="margin:4px 0 16px"><${Chips} value=${v.term} options=${v.terms.map((t, i) => [i, t])} onChange=${(i) => sheet('set', { term: i })} /></div>
    ${v.rows.map((r) => html`<${PriceRow} r=${r} h=${32} />`)}
  </${Dialog}>`;
}

/** The loan contract (`bank::loan_contract_sheet`). */
function LoanContractSheet({ v, sheet, close }) {
  const sig = useRef({ strokes: v.signature.strokes, name: v.signature.name });
  const keep = (strokes, name) => {
    sig.current = { strokes, name };
    sheet('set', { strokes, name });
  };
  const foot = v.readonly
    ? html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`
    : html`<span class="rest">${sig.current.strokes.length || sig.current.name.trim() ? '' : v.unsigned}</span>
      <${Btn} icon="chevron_left" label=${v.back} onClick=${() => sheet('back')} />
      <${Btn} kind="primary" icon="check_circle" label=${v.sign} onClick=${() => sheet('sign', sig.current)} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${1020} h=${820} onClose=${v.readonly ? close : () => sheet('back')} foot=${foot} bodyStyle="display:flex;flex-direction:column">
    <${Parties} p=${v.parties} />
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:24px;flex:1;min-height:0">
      <div>
        <div class="co-caps" style="margin-bottom:8px">${v.loan_caps}</div>
        ${v.rows.map((r) => html`<${PriceRow} r=${r} h=${31} />`)}
        <p class="co-note" style="margin-top:10px">${v.security}</p>
        <p class="co-note" style="margin-top:4px">${v.early}</p>
      </div>
      <div>
        <div class="co-caps" style="margin-bottom:8px">${v.plan_caps}</div>
        <div class="co-plan head">${v.plan_heads.map((h) => html`<span>${h}</span>`)}</div>
        ${v.plan.map((row) => (row ? html`<div class="co-plan">${row.map((c) => html`<span>${c}</span>`)}</div>` : html`<div class="co-plan gap">…</div>`))}
        <div class="co-note" style="text-align:right;border-top:1px solid var(--hub-hairline);margin-top:2px;padding-top:6px;font-weight:500">${v.interest}</div>
      </div>
    </div>
    <${Signature} s=${v.signature} onChange=${keep} />
  </${Dialog}>`;
}

/** Paying a loan back early (`bank::repay_sheet`). */
function RepaySheet({ v, sheet, close }) {
  const foot = html`<span class="rest"></span><${Btn} label=${v.cancel} onClick=${close} /><${Btn} kind="primary" icon="check_circle" label=${v.main} onClick=${() => sheet('main')} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${620} h=${440} onClose=${close} foot=${foot}>
    ${v.rows.map((r) => html`<${PriceRow} r=${r} h=${32} />`)}
    <p class="co-para" style="margin-top:12px">${v.note}</p>
  </${Dialog}>`;
}

/** An advertiser's contract (`adverts::contract_sheet`). */
function AdvertSheet({ v, sheet, close }) {
  const sig = useRef({ strokes: v.signature.strokes, name: v.signature.name });
  const keep = (strokes, name) => {
    sig.current = { strokes, name };
    sheet('set', { strokes, name });
  };
  const foot = v.readonly
    ? html`<span class="rest"></span><${Btn} kind="primary" label=${v.close} onClick=${close} />`
    : html`<span class="rest">${sig.current.strokes.length || sig.current.name.trim() ? '' : v.unsigned}</span>
      <${Btn} icon="chevron_left" label=${v.back} onClick=${close} />
      <${Btn} kind="primary" icon="check_circle" label=${v.sign} onClick=${() => sheet('sign', sig.current)} />`;
  return html`<${Dialog} icon=${v.icon} title=${v.title} w=${980} h=${720} onClose=${close} foot=${foot} bodyStyle="display:flex;flex-direction:column">
    <${Parties} p=${v.parties} />
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:22px;flex:1;min-height:0">
      <div>
        <div class="co-caps" style="margin-bottom:8px">${v.advert_caps}</div>
        ${v.rows.map((r) => html`<${PriceRow} r=${r} h=${30} />`)}
        <p class="co-note" style="margin-top:8px">${v.terms}</p>
      </div>
      <div class="co-scroll">
        <div class="co-caps" style="margin-bottom:8px">${v.buses_caps}</div>
        ${v.buses.map((r) => html`<${PriceRow} r=${r} h=${30} />`)}
      </div>
    </div>
    <${Signature} s=${v.signature} onChange=${keep} />
  </${Dialog}>`;
}

/** The pen over the paper (`signing`): the signature draws itself, the paper is stamped
 * "Signed"; about two seconds, a click skips to its end; with the animations off at once. */
function Pen({ pen, onDone }) {
  const still = S.settings?.animations === false;
  useEffect(() => {
    const t = setTimeout(onDone, still ? 250 : 2000);
    return () => clearTimeout(t);
  }, []);
  const W = 460;
  const H = 108;
  const strokes = pen.strokes || [];
  return html`<div class="co-scrim co-pen" onMouseDown=${onDone}>
    <div class="paper">
      <b>${pen.title}</b>
      <div class="line">
        ${strokes.length
          ? html`<svg viewBox=${`0 0 ${W} ${H}`} preserveAspectRatio="none">${strokes.map((st, k) => html`<polyline pathLength="1" style=${`animation-delay:${(k / strokes.length) * 1.2}s`} points=${st.map(([x, y]) => `${x * W},${y * H}`).join(' ')} />`)}</svg>`
          : html`<span class="written">${pen.name}</span>`}
      </div>
      <span class="stamp">${tr('Signed')}</span>
    </div>
  </div>`;
}

export { T };

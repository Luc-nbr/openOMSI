// The workshop's stores, as our game-drawn launcher's (`company/workshop.rs`), City Bus
// Manager's spare parts shop: four figures; a card a part - its name and picture, its price
// (the list price struck through when the research takes some off), a slider of how many to
// order, its stock and what the ones chosen cost -; under them the delivery (free at 16:00 or
// at once for a fee), the sum and Buy; beside them (under them on a narrow page) the buses
// waiting for parts, what is on order, the stores' own ordering and the mechanics.

import { html, ui, setUi, useSize, textW, Icon, Btn, Section, Figure, Tag, Seg, Slider, Switch, IconBtn } from './kit.js';
import { useMore, actMore, TextIn, box } from './more.js';

const CARD_W = 168;
const CARD_H = 198;
const GAP = 14;

export function WorkshopPage() {
  const st = ui('workshop', { counts: Array(10).fill(0), express: 0 });
  const v = useMore('workshop', { counts: st.counts, express: st.express });
  const [ref, size] = useSize();
  let body = null;
  if (v?.figures && size.w > 0) {
    const W = size.w;
    const H = size.h;
    const wide = W >= 980;
    const sideW = wide ? Math.max(300, Math.min(420, W * 0.3)) : 0;
    const mw = W - (wide ? sideW + 16 : 0);
    const cols = mw >= 760 ? 4 : 2;
    const fw = (mw - (cols - 1) * 12) / cols;
    const fh = 100;
    const fy = Math.ceil(v.figures.length / cols) * (fh + 12) + 4;
    body = html`
      ${v.figures.map((f, k) => html`<div style=${box((k % cols) * (fw + 12), Math.floor(k / cols) * (fh + 12), fw, fh)}><${Figure} f=${f} small=${fw < 170} /></div>`)}
      <div class="co-scroll" style=${box(0, fy, mw, Math.max(0, H - fy))}>
        <${Shop} v=${v} st=${st} w=${mw - 12} wide=${wide} />
      </div>
      ${wide && html`<div class="co-scroll" style=${box(mw + 16, 0, sideW, H)}><div style=${`width:${sideW - 10}px`}><${Side} s=${v.side} /></div></div>`}`;
  }
  return html`<div class="cm-page" ref=${ref}>${body}</div>`;
}

/** The parts' cards, the delivery, the sum and Buy (and on a narrow page the rest under them). */
function Shop({ v, st, w, wide }) {
  const cols = Math.max(1, Math.min(5, Math.floor((w + GAP) / (CARD_W + GAP))));
  const cw = (w - (cols - 1) * GAP) / cols;
  const y = Math.ceil(v.cards.length / cols) * (CARD_H + GAP) + 6;
  const set = (k, n) => {
    const counts = [...st.counts];
    counts[k] = n;
    setUi('workshop', { counts });
  };
  const f = v.foot;
  const fw = Math.min(w, 560);
  const fx = (w - fw) / 2;
  const buy = async () => {
    const r = await actMore('parts_buy', { counts: st.counts, express: st.express });
    if (r?.bought) setUi('workshop', { counts: Array(10).fill(0) });
  };
  return html`<div style=${`position:relative;width:${w}px;height:${y + 166}px`}>
      ${v.cards.map((c, k) => html`<${Card} c=${c} most=${v.most} store=${v.in_store} r=${{ x: (k % cols) * (cw + GAP), y: Math.floor(k / cols) * (CARD_H + GAP), w: cw, h: CARD_H }} count=${st.counts[k] || 0} onCount=${(n) => set(k, n)} />`)}
      <${Seg} value=${st.express} options=${f.ways.map((t, k) => [k, t])} onChange=${(k) => setUi('workshop', { express: k })} style=${box(fx, y, fw, 42) + ';height:42px'} />
      <${TextIn} x=${fx} y=${y + 48} w=${fw} h=${20} px=${13.5} tone="soft" align="center">${f.note}</${TextIn}>
      <${TextIn} x=${fx} y=${y + 76} w=${fw} h=${34} px=${24} weight=${700} tone=${f.sum_tone} align="center">${f.sum}</${TextIn}>
      <${Btn} kind=${f.chosen > 0 ? 'primary' : ''} icon="inventory_2" label=${f.buy} style=${box(w / 2 - 110, y + 120, 220, 46) + ';height:46px'} onClick=${buy} />
      ${f.chosen > 0 && html`<${IconBtn} icon="close" size=${20} title=${f.clear} style=${box(w / 2 + 110 + 12 + 2, y + 123 + 2, 36, 36)} onClick=${() => setUi('workshop', { counts: Array(10).fill(0) })} />`}
    </div>
    ${!wide && html`<div style="margin-top:18px;width:${w}px"><${Side} s=${v.side} /></div>`}`;
}

/** A part's card (`workshop::part_card`). */
function Card({ c, r, most, store, count, onCount }) {
  const pw = textW(c.price, 16, 700);
  const ww = c.was ? textW(c.was, 11.5, 400) : 0;
  const px0 = r.w / 2 - (ww + 8 + pw) / 2;
  return html`<div class=${'cm-part' + (count > 0 ? ' on' : '')} style=${box(r.x, r.y, r.w, r.h)}>
    <${TextIn} x=${10} y=${10} w=${r.w - 20} h=${26} px=${17} weight=${700} align="center">${c.label}</${TextIn}>
    <span class="cm-part-pic" style=${`left:${r.w / 2 - 24}px;top:38px`}><${Icon} name=${c.icon} size=${28} /></span>
    ${c.was
      ? html`<${TextIn} x=${px0} y=${98} w=${ww + 2} h=${22} px=${11.5} tone="dim" cls="cm-struck">${c.was}</${TextIn}>
          <${TextIn} x=${px0 + ww + 8} y=${98} w=${pw + 4} h=${22} px=${16} weight=${700} tone="ok">${c.price}</${TextIn}>`
      : html`<${TextIn} x=${10} y=${98} w=${r.w - 20} h=${22} px=${16} weight=${700} align="center">${c.price}</${TextIn}>`}
    <div style=${box(12, 126, r.w - 24, 30)}><${Slider} value=${count} min=${0} max=${most} step=${1} onChange=${onCount} format=${(x) => String(Math.round(x))} /></div>
    <span class=${'cm-icon ' + (c.stock === 0 ? 't-danger' : 't-soft')} style="left:16px;top:167px" title=${store}><${Icon} name="inventory_2" size=${16} /></span>
    <${TextIn} x=${38} y=${164} w=${60} h=${22} px=${15.5} weight=${700} tone=${c.stock === 0 ? 'danger' : 'text'} title=${store}>${String(c.stock)}</${TextIn}>
    <${TextIn} x=${r.w * 0.4} y=${164} w=${r.w * 0.6 - 14} h=${22} px=${15.5} weight=${700} tone=${c.line_on ? 'text' : 'dim'} align="right">${c.line}</${TextIn}>
  </div>`;
}

/** The rest of the stores (`workshop::side`): the buses waiting for parts, what is on order, the
 * stores' own ordering, the mechanics. */
function Side({ s }) {
  const tagW = (t) => textW(t, 12, 700) + 16;
  return html`<div class="cm-side">
    <${Section} title=${s.waiting.title}>
      <div style=${`position:relative;height:${Math.max(1, s.waiting.rows.length) * 44}px`}>
        ${!s.waiting.rows.length && html`<${TextIn} x=${0} y=${0} w=${400} h=${24} px=${14.5} tone="soft">${s.waiting.none}</${TextIn}>`}
        ${s.waiting.rows.map((w, k) => {
          const pw = 26 + tagW(w.number) + 10;
          return html`<span class="cm-icon t-soft" style=${`left:2px;top:${k * 44 + 3}px`}><${Icon} name="directions_bus" size=${16} /></span>
            <span class="cm-icon" style=${`left:24px;top:${k * 44}px`}><${Tag} text=${w.number} tone="soft" /></span>
            <div class="cm-txt t-warn" style=${`left:${pw}px;right:0;top:${k * 44}px;height:22px;font-size:var(--co-rows);font-weight:700`}><span class="cut">${w.parts}</span></div>
            <div class="cm-txt t-dim" style=${`left:${pw}px;right:0;top:${k * 44 + 20}px;height:18px;font-size:var(--co-note)`}><span class="cut">${w.since}</span></div>`;
        })}
      </div>
    </${Section}>
    <${Section} title=${s.orders.title}>
      <div style=${`position:relative;height:${Math.max(1, s.orders.rows.length) * 44}px`}>
        ${!s.orders.rows.length && html`<${TextIn} x=${0} y=${0} w=${400} h=${24} px=${14.5} tone="soft">${s.orders.none}</${TextIn}>`}
        ${s.orders.rows.map(
          (o, k) => html`<span class="cm-icon t-accent2" style=${`left:2px;top:${k * 44 + 3}px`}><${Icon} name="schedule" size=${16} /></span>
            <div class="cm-txt" style=${`left:28px;right:0;top:${k * 44}px;height:22px;font-size:var(--co-rows);font-weight:700`}><span class="cut">${o.what}</span></div>
            <div class="cm-txt t-dim" style=${`left:28px;right:0;top:${k * 44 + 20}px;height:18px;font-size:var(--co-note)`}><span class="cut">${o.parts}</span></div>`,
        )}
      </div>
    </${Section}>
    <${Section} title=${s.auto.title}>
      <${Switch} checked=${s.auto.on} label=${s.auto.label} onChange=${(on) => actMore('parts_auto', { on })} style="height:30px" />
      <p class="co-note" style="margin-top:6px">${s.auto.text}</p>
    </${Section}>
    <${Section} title=${s.mechanics.title}>
      <p class=${'co-note t-' + s.mechanics.tone}>${s.mechanics.text}</p>
    </${Section}>
  </div>`;
}

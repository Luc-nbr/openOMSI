// The concession market, as our game-drawn launcher's (`company/concessions.rs`): the map's
// lines the authority puts out to tender with their week, the concessions the company holds
// with their terms, and the auction of the tender picked beside them as it runs with the
// company's clock - the rivals with their character and their bids, the least bid that leads now
// and its chance, buying the line outright, the clock stepped on for it - and how it ended.

import { html, ui, setUi, useSize, textW, Icon, Btn, Section, Plate, Slider } from './kit.js';
import { useMore, actMore, TextIn, box, eur, useListSize } from './more.js';

const plateW = (number, h) => Math.max(h * 1.8, textW(number, h * 0.6, 900) + h * 0.7);

/** A small badge (`Ui::badge`): its words in capitals on a tint of its tone. */
export const Badge = ({ b, x, y, right }) => html`<span class=${'cm-badge t-' + b.tone} style=${right != null ? `right:${right}px;top:${y}px` : `left:${x}px;top:${y}px`}>${b.text}</span>`;

export function ConcessionsPage() {
  const st = ui('concessions', { amount: null, amount_for: null });
  const v = useMore('concessions', { amount: st.amount, amount_for: st.amount_for });
  const [ref, size] = useSize();
  let body = null;
  if (v?.tenders && size.w > 0) {
    const W = size.w;
    const H = Math.max(0, size.h - 32);
    const gap = 16;
    const leftW = Math.max(360, (W - gap) * 0.5);
    const listH = Math.max(200, (H - gap) * 0.58);
    body = html`
      <${Section} title=${v.tenders.title} style=${box(0, 0, leftW, listH)}><${Tenders} t=${v.tenders} /></${Section}>
      <${Section} title=${v.held.title} style=${box(0, listH + gap, leftW, Math.max(0, H - listH - gap))}><${Held} h=${v.held} /></${Section}>
      <${Section} title=${v.auction.title} style=${box(leftW + gap, 0, W - leftW - gap, H)}><${Auction} a=${v.auction} st=${st} /></${Section}>
      <${TextIn} x=${0} y=${size.h - 22} w=${W} h=${22} px=${13.5} tone="dim">${v.foot}</${TextIn}>`;
  }
  return html`<div class="cm-page" ref=${ref}>${body}</div>`;
}

/** The tenders: announced and running first, then the results; one picked for the panel. */
function Tenders({ t }) {
  const [ref, size] = useListSize();
  const W = Math.max(0, size.w - 10);
  const top = t.note ? 48 : 0;
  return html`<div class="cm-list" style="position:relative">
    ${t.note && html`<p class="co-note t-dim" style="font-size:10.75px;height:48px;overflow:hidden">${t.note}</p>`}
    ${t.empty
      ? html`<p class="co-para" style="font-size:var(--co-rows)">${t.empty}</p>`
      : html`<div class="co-scroll" ref=${ref} style=${`position:absolute;left:0;right:0;top:${top}px;bottom:0;height:auto`}>
          <div style=${`position:relative;height:${t.rows.length * 108}px`}>
            ${size.w > 0 &&
            t.rows.map((r, k) => {
              const pw = plateW(r.number, 26);
              return html`<button type="button" class=${'cm-tender' + (r.on ? ' on' : '')} style=${box(0, k * 108, W, 102)} onClick=${() => actMore('tender_pick', { id: r.id })}>
                ${r.on && html`<i class="cm-tender-bar"></i>`}
                <span class="cm-icon" style="left:10px;top:10px"><${Plate} p=${{ number: r.number }} h=${26} /></span>
                <${TextIn} x=${10 + pw + 12} y=${8} w=${W - pw - 120} h=${22} px=${14.5} weight=${700} tone=${r.dim ? 'dim' : 'text'}>${r.caption}</${TextIn}>
                <${TextIn} x=${10 + pw + 12} y=${34} w=${W - pw - 30} h=${20} px=${13.5} tone="dim">${r.figures}</${TextIn}>
                <${TextIn} x=${10 + pw + 12} y=${58} w=${W - pw - 30} h=${20} px=${13.5} weight=${500} tone=${r.tone}>${r.state}</${TextIn}>
                <${TextIn} x=${10 + pw + 12} y=${80} w=${W - pw - 30} h=${20} px=${13.5} weight=${500} tone=${r.bid_tone}>${r.bid}</${TextIn}>
                ${r.badge && html`<${Badge} b=${r.badge} x=${W - (r.badge.tone === 'ok' ? 70 : 90)} y=${8} />`}
              </button>`;
            })}
          </div>
        </div>`}
  </div>`;
}

/** The concessions the company holds and its own lines (`concessions::held`). */
function Held({ h }) {
  const [ref, size] = useListSize();
  const W = Math.max(0, size.w - 10);
  if (h.empty) return html`<p class="co-para" style="font-size:var(--co-rows)">${h.empty}</p>`;
  const rows = [...h.rows.map((r) => ({ ...r, own: false })), ...h.own.map((r) => ({ ...r, own: true }))];
  return html`<div class="co-scroll cm-list" ref=${ref}>
    <div style=${`position:relative;height:${rows.length * 60}px`}>
      ${size.w > 0 &&
      rows.map((r, k) => {
        const pw = plateW(r.number, 22);
        return html`<div class="cm-row" style=${box(0, k * 60, W, 54)}>
          <span class="cm-icon" style="left:0;top:4px"><${Plate} p=${{ number: r.number }} h=${22} /></span>
          <${TextIn} x=${pw + 12} y=${2} w=${W - pw - 12} h=${22} px=${14.5} weight=${700} tone=${r.own ? 'text' : r.tone}>${r.own ? r.caption : r.until}</${TextIn}>
          <${TextIn} x=${pw + 12} y=${27} w=${W - pw - 12} h=${20} px=${13.5} tone="dim">${r.own ? r.text : r.price}</${TextIn}>
          ${r.direct && html`<${Badge} b=${{ text: r.direct, tone: 'dim' }} x=${W - 80} y=${4} />`}
        </div>`;
      })}
    </div>
  </div>`;
}

/** The auction of the tender picked (`concessions::auction`). */
function Auction({ a, st }) {
  const [ref, size] = useSize();
  if (a.none) return html`<p class="co-para" style="font-size:var(--co-rows)">${a.none}</p>`;
  const iw = size.w;
  const ih = size.h;
  let y = 0;
  const parts = [];
  if (iw > 0) {
    const pw = plateW(a.number, 26);
    parts.push(html`<span class="cm-icon" style="left:0;top:2px"><${Plate} p=${{ number: a.number }} h=${26} /></span>
      <${TextIn} x=${pw + 12} y=${0} w=${iw - pw - 12} h=${30} px=${16} weight=${700}>${a.caption}</${TextIn}>`);
    y += 38;
    parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${18} px=${14.5} weight=${500} tone=${a.tone}>${a.state}</${TextIn}>`);
    y += 26;
    if (a.mine) {
      parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${18} px=${14.5} weight=${700} tone=${a.mine_tone}>${a.mine}</${TextIn}>`);
      y += 26;
    }
    const fw = (iw - 16) / 3;
    a.figures.forEach((f, k) => {
      const x = k * (fw + 8);
      parts.push(html`<${TextIn} x=${x} y=${y} w=${fw} h=${14} px=${12.5} weight=${700} tone="dim">${f.label}</${TextIn}>
        <${TextIn} x=${x} y=${y + 18} w=${fw} h=${28} px=${19} weight=${700} tone=${f.tone}>${f.value}</${TextIn}>`);
    });
    y += 58;
    parts.push(html`<div class="cm-progress" style=${box(0, y, iw, 6)}><i style=${`width:${Math.max(6, iw * a.frac)}px`}></i></div>`);
    y += 12;
    parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${16} px=${13.5} tone="dim">${a.times}</${TextIn}>`);
    y += 26;
    for (const b of a.bidders) {
      parts.push(html`${b.leads && html`<i class="cm-lead" style=${box(-6, y - 2, iw + 12, 32)}></i>`}
        <i class=${'cm-bidder ' + (b.leads ? 'leads' : '')} style=${`left:${0.5}px;top:${y + 13.5}px`}></i>
        <${TextIn} x=${16} y=${y} w=${iw * 0.45} h=${30} px=${14.5} weight=${500}>${b.name}</${TextIn}>
        <${TextIn} x=${16 + iw * 0.45} y=${y} w=${iw * 0.3} h=${30} px=${13.5} tone="dim">${b.kind}</${TextIn}>
        <${TextIn} x=${iw - 130} y=${y} w=${130} h=${30} px=${13} weight=${b.leads ? 700 : 400} tone=${b.has ? 'text' : 'faint'} align="right">${b.value}</${TextIn}>`);
      y += 34;
    }
    y += 8;
    const act = a.actions;
    const actionsH = act.kind === 'running' ? 168 : 52;
    const logH = Math.max(0, ih - actionsH - y);
    if (logH > 40) {
      parts.push(html`<${TextIn} x=${0} y=${y} w=${iw} h=${14} px=${12.5} weight=${700} tone="dim">${a.log_title}</${TextIn}>`);
      if (!a.log.length) parts.push(html`<${TextIn} x=${0} y=${y + 18} w=${iw} h=${18} px=${13.5} tone="dim">${a.no_log}</${TextIn}>`);
      else
        parts.push(html`<div class="co-scroll" style=${box(0, y + 18, iw, logH - 22)}><div style=${`position:relative;height:${a.log.length * 26}px`}>
          ${a.log.map(
            (l, k) => html`<${TextIn} x=${0} y=${k * 26} w=${56} h=${24} px=${13.5} weight=${700} tone="dim">${l.at}</${TextIn}>
              <${TextIn} x=${60} y=${k * 26} w=${iw * 0.6} h=${24} px=${13.5} tone=${l.me ? 'accent2' : 'soft'}>${l.who}</${TextIn}>
              <${TextIn} x=${iw - 150} y=${k * 26} w=${140} h=${24} px=${13.5} weight=${500} align="right">${l.amount}</${TextIn}>`,
          )}</div></div>`);
    }
    const ay = ih - actionsH;
    if (act.kind === 'running') {
      const amount = st.amount_for === act.id && st.amount != null ? Math.min(act.hi, Math.max(act.lo, st.amount)) : act.amount;
      const bw = (iw - 8) / 2;
      parts.push(html`<div style=${box(0, ay, iw, 36)}><${Slider} value=${amount} min=${act.lo} max=${act.hi} step=${100} label=${act.label} format=${(x) => eur(Math.round(x * 100))} onChange=${(x) => setUi('concessions', { amount: x, amount_for: act.id })} /></div>
        <${TextIn} x=${0} y=${ay + 42} w=${iw} h=${22} px=${13.5} weight=${500} tone=${act.chance_tone}>${act.chance}</${TextIn}>
        <${Btn} kind="primary" icon="payments" label=${act.bid} style=${box(0, ay + 72, bw, 40) + ';height:40px;padding:0 8px'} onClick=${() => actMore('tender_bid', { id: act.id, amount: Math.round(amount * 100) })} />
        <${Btn} icon="check_circle" label=${act.buy} title=${act.buy_tip} style=${box(bw + 8, ay + 72, bw, 40) + ';height:40px;padding:0 8px'} onClick=${() => actMore('tender_buy', { id: act.id })} />
        <${Btn} kind="ghost" icon="timer" label=${act.plus15} style=${box(0, ay + 122, bw, 36) + ';height:36px'} onClick=${() => actMore('tender_step', { id: act.id, to: '15' })} />
        <${Btn} kind="ghost" icon="sports_score" label=${act.to_close} style=${box(bw + 8, ay + 122, bw, 36) + ';height:36px'} onClick=${() => actMore('tender_step', { id: act.id, to: 'close' })} />`);
    } else if (act.kind === 'wait') {
      parts.push(html`<${Btn} icon="schedule" label=${act.label} style=${box(0, ay + 4, iw, 40) + ';height:40px'} onClick=${() => actMore('tender_step', { id: act.id, to: 'open' })} />`);
    } else if (act.kind === 'said' && act.text) {
      parts.push(html`<${TextIn} x=${0} y=${ay + 12} w=${iw} h=${20} px=${14.5} weight=${500} tone=${act.tone}>${act.text}</${TextIn}>`);
    }
  }
  return html`<div class="cm-list" ref=${ref} style="position:relative">${parts}</div>`;
}

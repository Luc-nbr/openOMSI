// The contracts, as our game-drawn launcher's (`company/contracts.rs`), City Bus Manager's
// assignments and service contracts: the trophies to spend, the rating in stars, the goals
// running and the trophies' bar to the next milestone; then the week's goals (on offer,
// running, finished), the service contracts (the buses' internet from one of four providers, a
// supplier's fixed diesel price, an outside cleaning company), or the tenders the company bid on.

import { html, ui, setUi, useSize, textW, Icon, Btn, Section, Figure, Seg, Slider, Plate, Meter, goTab } from './kit.js';
import { WideBar, Trophies, Reward } from './inbox.js';
import { useMore, actMore, TextIn, box } from './more.js';

export function ContractsPage() {
  const st = ui('contracts', { part: 0, gb: null });
  const v = useMore('contracts', { part: st.part, gb: st.gb });
  const [ref, size] = useSize();
  let body = null;
  if (v?.figures && size.w > 0) {
    const W = size.w;
    const H = size.h;
    const gap = 14;
    const fw = (W - 3 * gap) / 4;
    const fh = 100;
    const f = v.figures;
    const y = fh + gap;
    const by = y + 38 + gap;
    const bh = Math.max(120, H - by);
    const b = v.body;
    body = html`
      <div style=${box(0, 0, fw, fh)} title=${f.trophies.tip}><${Figure} f=${f.trophies} small=${fw < 170} /></div>
      <div style=${box(fw + gap, 0, fw, fh)} title=${f.rating.tip}><${Figure} f=${f.rating} small=${fw < 170} /><${Stars} rating=${f.rating.stars} /></div>
      <div style=${box(2 * (fw + gap), 0, fw, fh)}><${Figure} f=${f.running} small=${fw < 170} /></div>
      <div class="cm-card" style=${box(3 * (fw + gap), 0, fw, fh)}>
        <${TextIn} x=${16} y=${13} w=${fw - 32} h=${16} px=${12.5} weight=${700} tone="dim" style="text-transform:uppercase">${f.milestone.label}</${TextIn}>
        <div style=${box(12, 46, fw - 24, 32)}><${Trophies} t=${f.milestone.bar} /></div>
      </div>
      <${Seg} value=${v.part} options=${v.parts.map((p, k) => [k, p])} onChange=${(k) => setUi('contracts', { part: k })} style=${box(0, y, Math.min(600, W), 38) + ';height:38px'} />
      <div style=${box(0, by, W, bh)}>
        ${b.kind === 'goals' ? html`<${Goals} g=${b} W=${W} H=${bh} />` : b.kind === 'services' ? html`<${Services} s=${b} W=${W} H=${bh} st=${st} />` : html`<${Bids} b=${b} W=${W} H=${bh} />`}
      </div>`;
  }
  return html`<div class="cm-page" ref=${ref}>${body}</div>`;
}

/** The rating as five stars, filled as far as it goes (`contracts::stars`). */
function Stars({ rating }) {
  return html`<span class="cm-stars">${[0, 1, 2, 3, 4].map((k) => {
    const f = Math.max(0, Math.min(1, rating - k));
    return html`<i class=${f >= 0.75 ? 'full' : f >= 0.25 ? 'half' : ''}><${Icon} name="star" size=${18} /></i>`;
  })}</span>`;
}

// --- the goals ---------------------------------------------------------------------------------

function Goals({ g, W, H }) {
  const gap = 14;
  const w1 = (W - 2 * gap) * 0.36;
  const w3 = W - 2 * gap - 2 * w1;
  const o = g.offered;
  return html`
    <${Section} title=${o.title} style=${box(0, 0, w1, H)} tools=${o.until && html`<span class="cm-tool t-dim">${o.until}</span>`}>
      <div class="co-scroll cm-list">
        ${!o.cards.length && html`<p class="co-para">${o.empty}</p>`}
        ${o.cards.map(
          (c) => html`<div class="cm-goal">
            <p class="cm-goal-title">${c.title}</p>
            <p class="co-note t-dim" style="margin-top:4px">${c.rule}</p>
            <div class="cm-goal-line"><${Reward} g=${c} /><span class="t-soft">${c.days}</span></div>
            <div class="cm-goal-foot"><${Btn} kind=${c.primary ? 'primary' : ''} icon="flag" label=${c.take} style="height:34px" onClick=${() => actMore('goal_take', { id: c.id })} /></div>
          </div>`,
        )}
      </div>
    </${Section}>
    <${Section} title=${g.running.title} style=${box(w1 + gap, 0, w1, H)}>
      <div class="co-scroll cm-list">
        ${!g.running.cards.length && html`<p class="co-para">${g.running.empty}</p>`}
        ${g.running.cards.map(
          (c) => html`<div class="cm-goal running">
            <p class="cm-goal-title">${c.title}</p>
            <div style="margin-top:10px"><${WideBar} b=${c.bar} h=${26} /></div>
            <div class="cm-goal-line" style="margin-top:10px"><${Reward} g=${c} /></div>
            <div class=${'cm-goal-when t-' + c.when_tone}>${c.when}</div>
            <div class="cm-goal-foot"><span class="co-note t-dim cut grow" title=${c.rule}>${c.rule}</span><${Btn} kind="ghost" label=${c.give_up} style="height:32px" onClick=${() => actMore('goal_give_up', { id: c.id })} /></div>
          </div>`,
        )}
      </div>
    </${Section}>
    <${Section} title=${g.finished.title} style=${box(2 * (w1 + gap), 0, w3, H)}>
      <div class="co-scroll cm-list">
        ${!g.finished.rows.length && html`<p class="co-para">${g.finished.empty}</p>`}
        ${g.finished.rows.map(
          (r) => html`<div class="cm-done-goal">
            <span class=${r.done ? 't-ok' : 't-dim'}><${Icon} name=${r.done ? 'check_circle' : 'error'} size=${20} /></span>
            <div><p class=${r.done ? '' : 't-soft'}>${r.title}</p><p class=${'co-note ' + (r.done ? 't-ok' : 't-dim')}>${r.what}</p></div>
          </div>`,
        )}
      </div>
    </${Section}>`;
}

// --- the service contracts ---------------------------------------------------------------------

function Services({ s, W, H, st }) {
  const gap = 14;
  const lw = (W - gap) * 0.64;
  const rw = W - gap - lw;
  const fh = (H - gap) * 0.55;
  return html`
    <${Section} title=${s.internet.title} style=${box(0, 0, lw, H)}><${Internet} n=${s.internet} st=${st} /></${Section}>
    <${Section} title=${s.fuel.title} style=${box(lw + gap, 0, rw, fh)}><${Fuel} f=${s.fuel} /></${Section}>
    <${Section} title=${s.cleaning.title} style=${box(lw + gap, fh + gap, rw, H - fh - gap)}><${Cleaning} c=${s.cleaning} /></${Section}>`;
}

/** The buses' internet: the four providers side by side (`contracts::internet`). */
function Internet({ n, st }) {
  const [ref, size] = useSize();
  if (n.locked) {
    return html`<div class="cm-list cm-locked" ref=${ref}>
      <div style=${`position:absolute;left:0;right:0;top:${size.h / 2 - 92}px;display:flex;flex-direction:column;align-items:center`}>
        <span class="t-faint"><${Icon} name="wifi" size=${44} /></span>
        <p class="co-para" style="max-width:560px;margin-top:18px;text-align:left">${n.locked}</p>
        <${Btn} kind="primary" icon="hub" label=${n.to_research} style="height:38px;margin-top:10px" onClick=${() => goTab(10)} />
      </div>
    </div>`;
  }
  const iw = size.w;
  const cw = (iw - 3 * 12) / 4;
  const ch = Math.max(300, size.h - 32);
  const setGb = (k, x) => {
    const gb = n.providers.map((p) => p.gb);
    gb[k] = x;
    setUi('contracts', { gb });
  };
  return html`<div class="cm-list" ref=${ref} style="position:relative">
    <span class="cm-icon t-dim" style="left:1px;top:2px"><${Icon} name="info" size=${16} /></span>
    <${TextIn} x=${24} y=${0} w=${iw - 24} h=${20} px=${14.5} weight=${500} tone="soft">${n.head}</${TextIn}>
    ${iw > 0 &&
    n.providers.map((p, k) => {
      const x = 14;
      const w = cw - 28;
      let y = 14;
      const parts = [];
      parts.push(html`<span class=${'cm-icon ' + (p.mine ? 't-accent2' : 't-soft')} style=${`left:${cw / 2 - 14}px;top:${y}px`}><${Icon} name="wifi" size=${28} /></span>`);
      y += 36;
      parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${26} px=${18} weight=${700} align="center">${p.name}</${TextIn}>`);
      y += 30;
      parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${20} px=${13.5} weight=${500} tone=${p.sat_tone} align="center">${p.sat}</${TextIn}>`);
      y += 24;
      parts.push(html`<div style=${box(x, y, w, 5)}><${Meter} frac=${p.sat_frac} tone=${p.sat_tone} h=${5} /></div>`);
      y += 16;
      parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${20} px=${13.5} tone="dim" align="center">${p.per}</${TextIn}>`);
      y += 30;
      if (p.data) {
        parts.push(html`<div style=${box(x, y, w, 24)}><${WideBar} b=${p.data} h=${24} /></div>`);
        y += 32;
        parts.push(html`<${Btn} label=${p.top_up.label} title=${p.top_up.tip} style=${box(x, y, w, 32) + ';height:32px;padding:0 6px'} onClick=${() => actMore('net_top_up', { gb: p.top_up.gb })} />`);
        y += 44;
      }
      parts.push(html`<div style=${box(x, y, w, 30)} title=${p.gb_tip}><${Slider} value=${p.gb} min=${n.min} max=${n.max} step=${n.step} format=${(v) => `${Math.round(v)} GB`} onChange=${(v) => setGb(k, v)} /></div>`);
      y += 38;
      parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${30} px=${22} weight=${700} align="center">${p.price}</${TextIn}>`);
      y += 34;
      parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${18} px=${12.5} tone="dim" align="center">${p.from}</${TextIn}>`);
      y += 26;
      if (p.state) parts.push(html`<${TextIn} x=${x} y=${y} w=${w} h=${20} px=${13.5} weight=${700} tone=${p.state_tone} align="center">${p.state}</${TextIn}>`);
      const by = ch - 14 - 34;
      const half = (w - 8) / 2;
      if (p.mine) {
        if (p.change) parts.push(html`<${Btn} kind="primary" label=${p.change} style=${box(x, by, half, 34) + ';height:34px;padding:0 4px'} onClick=${() => actMore('net_take', { provider: k, gb: p.gb })} />`);
        if (p.cancel) parts.push(html`<${Btn} kind="danger" label=${p.cancel} style=${box(x + half + 8, by, half, 34) + ';height:34px;padding:0 4px'} onClick=${() => actMore('net_cancel')} />`);
      } else if (p.accept) parts.push(html`<${Btn} kind="primary" label=${p.accept} style=${box(x, by, w, 34) + ';height:34px'} onClick=${() => actMore('net_take', { provider: k, gb: p.gb })} />`);
      return html`<div class=${'cm-provider' + (p.mine ? ' mine' : '')} style=${box(k * (cw + 12), 32, cw, ch)}>${parts}</div>`;
    })}
  </div>`;
}

/** A supplier's fixed diesel price (`contracts::fuel`). */
function Fuel({ f }) {
  return html`<div class="cm-list" style="position:relative;overflow:hidden">
    <span class="cm-icon t-soft" style="left:0;top:3px"><${Icon} name="water_drop" size=${20} /></span>
    <div class="cm-txt" style="left:28px;right:0;top:0;height:26px;font-size:var(--co-head);font-weight:700"><span class="cut">${f.today}</span></div>
    <div style=${`padding-top:${28 + (f.change ? 24 : 0) + 8}px`}>
      ${f.change && html`<div class=${'cm-txt t-' + f.change.tone} style="left:28px;right:0;top:28px;height:20px;font-size:var(--co-note);font-weight:500"><span class="cut">${f.change.text}</span></div>`}
      ${f.fixed
        ? html`<p class="co-para t-ok" style="font-weight:500">${f.fixed}</p><p class="co-note t-dim" style="margin-top:6px">${f.fixed_note}</p>`
        : html`<p class="co-note" style="font-size:10.75px">${f.offer}</p>
          <div style="margin-top:10px">${f.terms.map(
            (t) => html`<div class="cm-term"><span class="cut grow">${t.text}</span><${Btn} label=${f.sign} style="height:34px" onClick=${() => actMore('fuel_sign', { term: t.k })} /></div>`,
          )}</div>`}
    </div>
  </div>`;
}

/** The outside cleaning company (`contracts::cleaning`). */
function Cleaning({ c }) {
  return html`<div class="cm-list" style="position:relative;display:flex;flex-direction:column;overflow:hidden">
    <p class="co-note" style="font-size:10.75px">${c.text}</p>
    ${c.state && html`<div class=${'t-' + c.tone} style="margin-top:10px;height:22px;font-size:var(--co-rows);font-weight:700;display:flex;align-items:center">${c.state}</div>`}
    <div style="margin-top:auto;padding-top:8px">
      ${c.open
        ? html`<${Btn} kind="danger" label=${c.end} style="height:34px" onClick=${() => actMore('cleaning_end')} />`
        : html`<${Btn} kind="primary" icon="check_circle" label=${c.sign} style="height:34px" onClick=${() => actMore('cleaning_sign')} />`}
    </div>
  </div>`;
}

// --- the tender bids ---------------------------------------------------------------------------

/** The tenders the company bid on, the latest first (`concessions::bids_list`): one picked is
 * its auction on the Concessions page. */
function Bids({ b, W, H }) {
  const iw = W - 36;
  const plateW = (n) => Math.max(26 * 1.8, textW(n, 15.6, 900) + 18.2);
  return html`<${Section} title=${b.title} style=${box(0, 0, W, H)}>
    ${!b.rows.length
      ? html`<p class="co-para" style="font-size:var(--co-rows)">${b.empty}</p>`
      : html`<div class="co-scroll cm-list"><div style=${`position:relative;height:${b.rows.length * 80}px`}>
          ${b.rows.map((r, k) => {
            const pw = plateW(r.number);
            const w = iw - 10;
            return html`<button type="button" class="cm-tender" style=${box(0, k * 80, w, 74)} onClick=${() => actMore('tender_pick', { id: r.id }).then(() => goTab(8))}>
              <span class="cm-icon" style="left:10px;top:10px"><${Plate} p=${{ number: r.number }} h=${26} /></span>
              <${TextIn} x=${10 + pw + 12} y=${8} w=${w - pw - 30} h=${22} px=${14.5} weight=${700}>${r.caption}</${TextIn}>
              ${r.bid && html`<${TextIn} x=${10 + pw + 12} y=${32} w=${w - pw - 30} h=${20} px=${13.5} weight=${500} tone=${r.bid_tone}>${r.bid}</${TextIn}>`}
              <${TextIn} x=${10 + pw + 12} y=${52} w=${w - pw - 30} h=${20} px=${13.5} tone=${r.tone}>${r.state}</${TextIn}>
            </button>`;
          })}
        </div></div>`}
  </${Section}>`;
}

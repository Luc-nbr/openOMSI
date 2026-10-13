// The Casual game mode's own interface, as our game-drawn launcher's (`company/casual/`): a
// calm home - today in a sentence, what the company solved by itself, "Drive a shift", four
// large tiles, the tips and after three weeks the hint to Normal - and a handful of simple
// pages: the buses and the shop, the lines (each with its fare) and the three-step line wizard,
// the drivers, the money, the company's livery and depot, and today's shifts.

import { S } from '../../store.js';
import { html, useState, setCo, setUi, ui, act, useView, useSize, textW, handOver, goKey, Icon, Btn, Chips, Plate, IconBtn, Tag, Slider, Meter, fill, rgbOf, CASUAL } from './kit.js';
import { BusTile, initials } from './fleet.js';
import { Portrait } from './people.js';

const PAGE = { [CASUAL.BUSES]: 'buses', [CASUAL.SHOP]: 'shop', [CASUAL.LINES]: 'lines', [CASUAL.NEW_LINE]: 'new_line', [CASUAL.DRIVERS]: 'drivers', [CASUAL.MONEY]: 'money', [CASUAL.COMPANY]: 'company', [CASUAL.SHIFTS]: 'shifts' };
const GAP = 16;

/** What a click of the Casual pages does (`casual::run`). */
export function run(a) {
  switch (a.kind) {
    case 'open':
      return setCo({ tab: a.tab });
    case 'add_line':
      setUi('casual', { step: 0, pick: null, freq: 'Normal' });
      return setCo({ tab: CASUAL.NEW_LINE });
    case 'hire_now':
      setCo({ tab: CASUAL.DRIVERS });
      return act('hire_now');
    case 'fare':
      setUi('casual', { focus: a.line });
      return setCo({ tab: CASUAL.LINES });
    case 'next_day':
      return act('next_day');
    case 'to_normal':
      return goKey('to-normal:100');
    case 'seen':
      return act('seen', { id: a.id });
    default:
  }
  return null;
}

export function CasualPage({ tab }) {
  const st = ui('casual', { size: 0, focus: null, step: 0, pick: null, freq: 'Normal' });
  const page = PAGE[tab] || 'home';
  const v = useView('casual', { page, size: st.size, focus: st.focus, step: st.step, pick: st.pick, freq: st.freq });
  if (!v || v.page !== page) return null;
  const Page = { home: Home, buses: Buses, shop: Shop, lines: Lines, new_line: NewLine, drivers: Drivers, money: Money, company: Company, shifts: Shifts }[page];
  if (page === 'home') return html`<${Home} v=${v} />`;
  return html`<div class="co-casual"><${Head} h=${v.head} />
    <div class="co-casual-body"><${Page} v=${v} st=${st} /></div></div>`;
}

/** A page's head (`casual::page_head`): its name and line, its "?" and its main button. */
function Head({ h, onAction }) {
  const tab = S.co.tab;
  const action = h.action && (() => (onAction ? onAction() : run(tab === CASUAL.BUSES ? { kind: 'open', tab: CASUAL.SHOP } : tab === CASUAL.LINES ? { kind: 'add_line' } : { kind: 'hire_now' })));
  return html`<div class="co-casual-head">
    <div class="grow"><b class="cut">${h.name}</b><span class="cut">${h.sub}</span></div>
    <${IconBtn} icon="help" title=${h.help_tip} onClick=${() => setCo({ popup: h.help })} />
    ${h.action && html`<${Btn} kind="primary" icon=${h.action[1]} label=${h.action[0]} style="height:42px;margin-left:8px" onClick=${action} />`}
  </div>`;
}

/** A calm card for a page with nothing in it yet (`casual::empty`). */
function Empty({ e }) {
  return html`<div class="co-casual-empty"><span class="ring"><${Icon} name=${e.icon} size=${26} /></span><b>${e.title}</b><p>${e.text}</p>
    ${e.button && html`<${Btn} kind="primary" icon=${e.button.icon} label=${e.button.label} style="height:44px;margin-top:20px" onClick=${() => run(e.button.act)} />`}</div>`;
}

// --- the home --------------------------------------------------------------------------------

function Home({ v }) {
  const [ref, size] = useSize();
  const w = Math.min(size.w, 1180);
  const driveW = Math.max(260, Math.min(340, w * 0.3));
  const tw = Math.floor((w - GAP * 3) / 4);
  return html`<div class="co-scroll" ref=${ref}>
    ${size.w > 0 && html`<div class="co-casual-home" style=${`width:${w}px`}>
      <div class="top" style=${`grid-template-columns:minmax(0,1fr) ${driveW}px`}>
        <section class="today">
          <div class="when"><b>${v.today_caps}</b><span>${v.date}</span></div>
          <p class="sentence">${v.sentence}</p>
          <div class="co-caps">${v.solved_caps}</div>
          ${v.solved.length
            ? v.solved.map((t) => html`<div class="solved" title=${t}><${Icon} name="autorenew" size=${15} class="t-accent2" /><span class="cut">${t}</span></div>`)
            : html`<div class="solved"><${Icon} name="check_circle" size=${16} class="t-ok" /><span class="cut">${v.calm}</span></div>`}
        </section>
        <button type="button" class="drive" onClick=${() => setCo({ tab: CASUAL.SHIFTS })}>
          <span class="ring"><${Icon} name="play_arrow" size=${37} /></span><b>${v.drive}</b><span>${v.drive_sub}</span></button>
      </div>
      <div class="tiles" style=${`grid-template-columns:repeat(4, ${tw}px)`}>
        ${v.tiles.map((t) => html`<div class="co-tile tile" onClick=${() => setCo({ tab: t.page })} data-page=${t.page}>
          <div class="name"><span class="mark"><${Icon} name=${t.icon} size=${19} /></span><b class="cut">${t.name}</b></div>
          <div class=${'big t-' + t.tone}>${t.big}</div>
          <div class="under cut" title=${t.under}>${t.under}</div>
          <span onClick=${(e) => e.stopPropagation()}><${Btn} icon=${t.action.icon} label=${t.action.label} style="width:100%;height:36px" onClick=${() => run(t.action.act)} /></span>
        </div>`)}
      </div>
      <div class="co-caps" style="margin:8px 0 12px">${v.tips_caps}</div>
      ${v.tips.length
        ? v.tips.map((t) => html`<div class=${'tip' + (t.warn ? ' warn' : '')}>
            <${Icon} name=${t.warn ? 'warning' : 'light'} size=${19} class="ic" />
            <span class="grow cut" title=${t.text}>${t.text}</span>
            <${Btn} kind=${t.warn ? 'primary' : ''} icon=${t.button.icon} label=${t.button.label} style="height:36px" onClick=${() => run(t.button.act)} />
            <${IconBtn} icon="close" size=${14} title=${v.not_now} onClick=${() => act('tip_hide', { tip: t.id })} /></div>`)
        : html`<div class="tip calm"><${Icon} name="check_circle" size=${18} class="t-ok" /><span class="grow">${v.no_tips}</span></div>`}
      ${v.hint && html`<div class="hint"><${Icon} name="tune" size=${19} class="t-accent2" /><span class="grow cut" title=${v.hint.text}>${v.hint.text}</span>
        <${Btn} icon="tune" label=${v.hint.yes} style="height:36px" onClick=${() => run({ kind: 'to_normal' })} />
        <${Btn} kind="ghost" label=${v.hint.no} style="height:36px" onClick=${() => act('hint_off')} /></div>`}
    </div>`}
  </div>`;
}

// --- the buses and the shop ---------------------------------------------------------------------

/** Columns of cards at least 280 wide (`casual::buses::grid`). */
function grid(w, extra) {
  const cols = Math.max(1, Math.min(4, Math.floor((w + GAP) / (280 + GAP))));
  const cw = Math.floor((w - GAP * (cols - 1)) / cols);
  return { cols, cw, ch: cw * 0.52 + extra };
}

function Buses({ v }) {
  const [ref, size] = useSize();
  if (!v.cards.length) return html`<${Empty} e=${v.empty} />`;
  const g = grid(Math.max(0, size.w - 12), 112);
  return html`<div style="height:100%;display:flex;flex-direction:column">
    ${v.order && html`<div class="co-order" style="flex:none;height:38px"><${Icon} name="schedule" size=${18} class="t-accent2" /><span class="cut">${v.order}</span></div>`}
    <div class="co-scroll" ref=${ref} style="flex:1;min-height:0"><div style=${`position:relative;height:${Math.ceil(v.cards.length / g.cols) * (g.ch + GAP)}px`}>
      ${size.w > 0 &&
      v.cards.map((b, k) => {
        const r = { x: (k % g.cols) * (g.cw + GAP), y: Math.floor(k / g.cols) * (g.ch + GAP), w: g.cw, h: g.ch };
        return html`<${BusTile} name=${b.name} r=${r} onClick=${() => setCo({ dialog: { kind: 'vehicle', id: b.id } })} attrs=${{ 'data-bus': b.id }}
          over=${html`<span class="numb" style=${`width:${textW(b.number, 15, 900) + 20}px`}>${b.number}</span>`}>
          <b class="cut" style="font-size:12.7px">${b.name}</b><span class="sub cut">${b.age}</span>
          <div style="margin-top:8px"><${Meter} frac=${b.cond} tone=${b.cond_tone} h=${8} /></div>
          <span class="casualstate"><span class="cut">${b.word}</span><b class=${'t-' + b.tone}>${b.doing}</b></span>
        </${BusTile}>`;
      })}
    </div></div>
  </div>`;
}

function Shop({ v, st }) {
  const [ref, size] = useSize();
  if (v.reading) return html`<p class="co-para">${v.reading}</p>`;
  const g = grid(Math.max(0, size.w - 12), 120);
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:none;max-width:620px"><${Chips} value=${v.size} options=${v.sizes.map((s, k) => [k, s])} onChange=${(k) => setUi('casual', { size: k })} /></div>
    ${v.cards.length
      ? html`<div class="co-scroll" ref=${ref} style="flex:1;min-height:0;margin-top:14px"><div style=${`position:relative;height:${Math.ceil(v.cards.length / g.cols) * (g.ch + GAP)}px`}>
          ${size.w > 0 &&
          v.cards.map((b, k) => {
            const r = { x: (k % g.cols) * (g.cw + GAP), y: Math.floor(k / g.cols) * (g.ch + GAP), w: g.cw, h: g.ch };
            return html`<${BusTile} name=${b.name} r=${r} onClick=${() => act('shop_buy', { file: b.file })}
              over=${b.fits && html`<span style="position:absolute;left:10px;top:10px"><${Tag} text=${b.fits} tone="ok" /></span>`}>
              <b class="cut" style="font-size:12.7px">${b.title}</b><span class="sub cut">${b.facts}</span>
              ${b.locked
                ? html`<span class="shoplock"><${Icon} name="lock" size=${15} class="t-dim" /><span class="cut">${b.locked}</span></span>`
                : html`<span class="shopbuy"><b class=${b.dear ? 't-warn' : ''}>${b.price}</b><span class="t-accent2">${b.buy}</span></span>`}
            </${BusTile}>`;
          })}
        </div></div>`
      : html`<${Empty} e=${v.empty} />`}
  </div>`;
}

// --- the lines and the line wizard -------------------------------------------------------------

function Lines({ v }) {
  const [ref, size] = useSize();
  if (!v.cards.length) return html`<${Empty} e=${v.empty} />`;
  const cols = size.w >= 1100 ? 3 : 2;
  return html`<div class="co-scroll" ref=${ref}><div class="co-casual-lines" style=${`grid-template-columns:repeat(${cols}, minmax(0,1fr))`}>
    ${v.cards.map((l) => html`<${LineCard} l=${l} />`)}
  </div></div>`;
}

/** A line's card (`casual::lines::line_card`): its plate and name, buses and passengers, and its
 * ticket on a slider with the advice. */
function LineCard({ l }) {
  const [fare, setFare] = useState(null);
  const shown = fare ?? l.fare;
  const eurc = (c) => `€${(c / 100).toFixed(2)}`;
  const set = (cents) => act('set_fare', { line: l.name, fare: cents, casual: true }).then(() => (setFare(null), setUi('casual', { focus: null })));
  return html`<div class=${'co-casual-line' + (l.lit ? ' lit' : '')}>
    <div class="top"><${Plate} p=${l.plate} h=${30} /><b class="cut">${l.public}</b></div>
    <div class="facts">${l.facts.map(([ic, t]) => html`<span><${Icon} name=${ic} size=${16} class="t-dim" /><span class="cut">${t}</span></span>`)}</div>
    <div class="fare">
      <${Slider} label=${l.label} value=${shown / 100} min=${l.lo / 100} max=${l.hi / 100} step=${0.1} format=${(x) => eurc(Math.round(x * 10) * 10)}
        onChange=${(x) => setFare(Math.round(x * 10) * 10)} onRelease=${(x) => set(Math.round(x * 10) * 10)} />
    </div>
    <div class="advice"><span class=${'cut t-' + l.tone} title=${l.advice}>${l.advice}</span>
      ${l.use && html`<${Btn} kind="ghost" label=${l.use} style="width:140px;height:32px;padding:0" onClick=${() => set(l.advised)} />`}</div>
  </div>`;
}

const FREQS = ['Quiet', 'Normal', 'Busy'];

/** The three-step line wizard (`casual::lines::wizard`). */
function NewLine({ v, st }) {
  const put = (patch) => setUi('casual', patch);
  const cancel = () => (put({ step: 0, pick: null, freq: 'Normal' }), setCo({ tab: CASUAL.LINES }));
  const finish = async () => {
    if (st.pick === 'draw') {
      put({ step: 0, pick: null });
      setCo({ tab: CASUAL.LINES });
      return handOver('casual-line=' + Math.max(0, FREQS.indexOf(st.freq)));
    }
    const r = await act('casual_line', { name: st.pick });
    if (r?.focus) put({ step: 0, pick: null, freq: 'Normal', focus: r.focus });
    return null;
  };
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div class="co-wizsteps">${v.steps.map((s, k) => html`<span class=${k < v.step ? 'done' : k === v.step ? 'now' : ''}>
      ${k < v.step ? html`<${Icon} name="check_circle" size=${26} class="t-ok" />` : html`<i>${k + 1}</i>`}<b>${s}</b>${k + 1 < v.steps.length && html`<em></em>`}</span>`)}</div>
    <div style="flex:1;min-height:0;margin-top:16px">
      ${v.reading
        ? html`<p class="co-para">${v.reading}</p>`
        : v.step === 0
          ? html`<div class="co-scroll"><div class="co-choices">
              <${Choice} icon="location_on" title=${v.draw.title} text=${v.draw.text} on=${st.pick === 'draw'} onClick=${() => put({ pick: 'draw', step: 1 })} />
              ${v.choices.map((x) => html`<${Choice} plate=${{ number: x.number, colour: '' }} title=${x.caption} text=${x.text} on=${st.pick === x.name} onClick=${() => put({ pick: x.name, step: 1, ...(x.freq ? { freq: x.freq } : {}) })} />`)}
            </div></div>`
          : v.step === 1
            ? html`<p class="co-para" style="max-width:820px;margin-bottom:18px">${v.intro}</p>
              <div class="co-choices three">${v.freqs.map((f) =>
                v.fixed && f.key !== v.freq ? html`<div class="co-choice still"><b>${f.label}</b></div>` : html`<${Choice} icon=${f.icon} title=${f.label} text=${f.text} on=${f.key === v.freq} onClick=${() => !v.fixed && put({ freq: f.key })} />`,
              )}</div>`
            : html`<div class="co-wizdone"><b class="title">${v.title}</b>
                ${(v.rows || []).map(([ic, what, value, tone]) => html`<div class="row"><${Icon} name=${ic} size=${18} class="t-accent2" /><span>${what}</span><b class=${'t-' + tone}>${value}</b></div>`)}
                <p class="co-note" style="margin-top:12px;font-size:10.75px">${v.note}</p></div>`}
    </div>
    <div class="co-wizfoot">
      <${Btn} icon="chevron_left" label=${v.step === 0 ? v.cancel : v.back} style="height:44px" onClick=${() => (v.step === 0 ? cancel() : put({ step: v.step - 1 }))} />
      <span class="grow"></span>
      ${v.step === 1 && html`<${Btn} kind="primary" icon="chevron_right" label=${v.next} style="height:44px" onClick=${() => put({ step: 2 })} />`}
      ${v.step === 2 && v.finish && html`<${Btn} kind="primary" icon=${v.finish[1]} label=${v.finish[0]} style="height:44px" onClick=${finish} />`}
    </div>
  </div>`;
}

/** A choice of the wizard (`casual::lines::choice`). */
function Choice({ icon, plate, title, text, on, onClick }) {
  return html`<button type="button" class=${'co-tile co-choice' + (on ? ' on' : '')} onClick=${onClick}>
    <div class="top">${plate ? html`<${Plate} p=${plate} h=${30} />` : html`<span class="mark"><${Icon} name=${icon} size=${20} /></span>`}<b class="cut">${title}</b></div>
    <p>${text}</p>
  </button>`;
}

// --- the drivers ---------------------------------------------------------------------------------

function Drivers({ v }) {
  const [ref, size] = useSize();
  if (!v.cards.length) return html`<${Empty} e=${v.empty} />`;
  const cols = size.w >= 1100 ? 3 : 2;
  return html`<div class="co-scroll" ref=${ref}><div class="co-casual-drivers" style=${`grid-template-columns:repeat(${cols}, minmax(0,1fr))`}>
    ${v.cards.map((e) => html`<div class="card">
      <div class="top"><${Portrait} n=${e.portrait} name=${e.name} size=${76} />
        <div class="who"><b class="cut">${e.name}</b><span class="stars">${[0, 1, 2, 3, 4].map((k) => html`<${Icon} name="star" size=${17} class=${k < e.stars ? 't-warn' : 'off'} />`)}</span>
          <span class=${'doing cut t-' + e.tone}><${Icon} name="schedule" size=${15} />${e.doing}</span></div></div>
      ${!e.leaving && html`<div class="btns"><${Btn} icon="stars" label=${e.bonus} title=${e.bonus_tip} style="height:36px" onClick=${() => act('bonus', { id: e.id })} />
        <${Btn} kind="ghost" icon="logout" label=${e.fire} style="height:36px" onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'dismiss', id: e.id } })} /></div>`}
    </div>`)}
  </div></div>`;
}

// --- the money and the company ------------------------------------------------------------------

function Money({ v }) {
  const weeks = v.weeks;
  const top = Math.max(1, ...weeks.map((w) => Math.max(w.income, w.expenses)));
  const skip = Math.max(0, 8 - weeks.length);
  return html`<div class="co-scroll"><div class="co-casual-money">
    <div class="pair">
      <section class="card weeks">
        <div class="headrow"><span class="co-caps grow">${v.weeks_caps}</span><span class="key"><i class="in"></i>${v.in}</span><span class="key"><i class="out"></i>${v.out}</span></div>
        ${weeks.length
          ? html`<div class="chart">${weeks.map((w, k) => html`<div class="slot" style=${`left:${((k + skip) / 8) * 100}%`} title=${w.tip}>
              <div class="bars"><i class="in" style=${`height:${(w.income / top) * 100}%`}></i><i class="out" style=${`height:${(w.expenses / top) * 100}%`}></i></div>
              <span class="label">${w.label}</span><b class=${'t-' + w.tone}>${w.short}</b></div>`)}</div>`
          : html`<p class="co-para" style="margin-top:20px">${v.no_weeks}</p>`}
      </section>
      <section class="card costs">
        <span class="co-caps">${v.costs_caps}</span>
        ${v.costs.length
          ? v.costs.map(([what, amount, share]) => html`<div class="cost"><div><span class="cut">${what}</span><b>${amount}</b></div><div class="bar"><i style=${`width:max(4px, ${share * 100}%)`}></i></div></div>`)
          : html`<p class="co-para" style="margin-top:8px">${v.no_costs}</p>`}
      </section>
    </div>
    ${v.tip && html`<div class="tip warn"><${Icon} name="light" size=${19} class="ic" /><span class="grow cut">${v.tip.text}</span>
      <${Btn} icon=${v.tip.icon} label=${v.tip.label} style="height:36px" onClick=${() => run(v.tip.act)} /></div>`}
  </div></div>`;
}

function Company({ v }) {
  const l = v.livery;
  const d = v.depot;
  const s = d.state;
  return html`<div class="co-casual-company">
    <section class="card">
      <div class="title"><${Icon} name="palette" size=${24} class="t-accent2" /><b>${l.title}</b></div>
      <div class="sw">${l.colours.map((c) => html`<span style=${`background:rgb(${rgbOf(c).join(',')})`}></span>`)}<span class="cut words">${l.designs}</span></div>
      <p class="co-note" style="font-size:10.75px">${l.text}</p>
      <span class="grow"></span>
      ${l.bus
        ? html`<${Btn} kind="primary" icon="livery_brush" label=${l.button} title=${l.tip} style="height:42px;align-self:flex-start" onClick=${() => act('design_livery', { file: l.bus[0], livery: l.bus[1] })} />`
        : html`<span class="co-para t-dim" style="font-weight:500;line-height:42px">${l.none}</span>`}
    </section>
    <section class="card">
      <div class="title"><${Icon} name="garage" size=${24} class="t-accent2" /><b>${d.title}</b></div>
      <span class="co-para" style="color:var(--hub-text-soft);font-weight:500">${d.name}</span>
      <b class="used">${d.used}</b>
      <${Meter} frac=${d.frac} tone=${d.tone} h=${10} />
      ${s.building && html`<p class="co-note t-accent2" style="margin-top:14px;font-size:10.75px;font-weight:500">${s.building}</p>`}
      ${s.next && html`<p class="co-note" style="margin-top:14px;font-size:10.75px">${s.next}</p>`}
      ${s.max && html`<p class="co-note" style="margin-top:14px;font-size:10.75px">${s.max}</p>`}
      <span class="grow"></span>
      ${s.button && html`<${Btn} kind=${s.primary ? 'primary' : ''} icon="garage" label=${s.button} style="height:42px;align-self:flex-start" onClick=${() => act('build_depot')} />`}
    </section>
  </div>`;
}

// --- today's shifts ----------------------------------------------------------------------------------

function Shifts({ v }) {
  const [ref, size] = useSize();
  if (v.reading) return html`<p class="co-para">${v.reading}</p>`;
  if (v.empty) return html`<${Empty} e=${v.empty} />`;
  const cols = size.w >= 1000 ? 4 : 2;
  return html`<div class="co-scroll" ref=${ref}>
    <div class="co-casual-shifts" style=${`grid-template-columns:repeat(${cols}, minmax(0,1fr))`}>
      ${v.cards.map((s) => html`<div class=${'co-tile shift' + (s.mine ? ' mine' : '')} onClick=${() => act('drive_shift', { duty: s.duty, mine: s.mine })}>
        <div class="top"><${Plate} p=${s.plate} h=${28} />${s.tag && html`<${Tag} text=${s.tag[0]} tone=${s.tag[1]} />`}</div>
        <b class="time">${s.time}</b>
        <span class=${'bus cut ' + (s.ok ? 't-soft' : 't-warn')}><${Icon} name="directions_bus" size=${16} class=${s.ok ? 't-dim' : 't-warn'} />${s.bus}</span>
        <span class="shift-trips">${s.trips}</span>
        <span class="grow"></span>
        <${Btn} kind=${s.primary ? 'primary' : ''} icon="play_arrow" label=${s.go} style="width:100%;height:40px" onClick=${(e) => (e.stopPropagation(), act('drive_shift', { duty: s.duty, mine: s.mine }))} />
      </div>`)}
    </div>
    <p class="co-iconline t-dim" style="font-weight:400;font-size:var(--co-note);margin-top:8px"><${Icon} name="info" size=${16} /><span>${v.note}</span></p>
  </div>`;
}

export { initials, fill };

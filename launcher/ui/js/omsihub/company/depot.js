// The depot page, as our game-drawn launcher's (`company/depot.rs`): its four figures; the yard
// drawn from above - the offices, the workshop hall with its bays, the washing bay, the diesel
// station, the parking's blocks with their chargers, and the buses where they stand tonight -;
// the areas to build, level by level (a bigger depot); and the workshop's jobs. Calm, as the
// other company pages: what is built is drawn in the card's quiet greys with a hairline, what
// could be built is a dashed plot, building work is amber, the buses wear the company's colour.
// A bus clicked on the plan is picked for the workshop's list (and clicked again let go).

import { html, tr, ui, setUi, useSize, textW, Icon, Btn, Section, Figure, Tag, Meter, Plate, IconBtn, rgbOf, inkOn, fill } from './kit.js';
import { useMore, actMore, TextIn, box, fillOf, useListSize } from './more.js';

const ASPHALT = [25, 31, 44];
const ROOF = [38, 46, 62];
const STREET = [31, 37, 50];
const WARN = '232, 170, 70';
const OK = '52, 178, 110';
const EARLY = '42, 117, 247';
const EARLY_SOFT = '122, 168, 255';
const faint = (a) => `rgba(var(--co-faint-rgb), ${a})`;

export function DepotPage() {
  const st = ui('depot', { selected: null });
  const v = useMore('depot', { selected: st.selected });
  const [ref, size] = useSize();
  let body = null;
  if (v?.figures && size.w > 0) {
    const W = size.w;
    const H = size.h;
    const gap = 16;
    const leftW = Math.max(420, (W - gap) * 0.56);
    const rx = leftW + gap;
    const rw = Math.max(0, W - leftW - gap);
    const fh = 100;
    const fw = (leftW - 3 * 12) / 4;
    const bh = Math.min(6 * 62 + 56, H * 0.62);
    body = html`
      ${v.figures.map((f, k) => html`<div style=${box(k * (fw + 12), 0, fw, fh)}><${Figure} f=${f} small=${fw < 170} /></div>`)}
      <${Section} title=${v.plan.title} style=${box(0, fh + 12, leftW, Math.max(0, H - fh - 12))}>
        <${Plan} p=${v.plan} w=${leftW - 36} h=${Math.max(0, H - fh - 12 - 58)} selected=${st.selected} />
      </${Section}>
      <${Section} title=${v.build.title} style=${box(rx, 0, rw, bh)}><${Build} b=${v.build} /></${Section}>
      <${Section} title=${v.workshop.title} style=${box(rx, bh + 12, rw, Math.max(0, H - bh - 12))}><${Workshop} ws=${v.workshop} selected=${st.selected} /></${Section}>`;
  }
  return html`<div class="cm-page" ref=${ref}>${body}</div>`;
}

// --- the plan -----------------------------------------------------------------------------

/** A plot that could be built on: a dashed outline, each side's dashes from its corner
 * (`depot::dashed`). */
function dashed(r, colour) {
  const dash = 6;
  let d = '';
  const seg = (ax, ay, bx, by) => {
    const len = Math.hypot(bx - ax, by - ay);
    const n = Math.max(1, Math.floor(len / (dash * 2)));
    const dx = (bx - ax) / Math.max(len, 1e-3);
    const dy = (by - ay) / Math.max(len, 1e-3);
    for (let k = 0; k < n; k++) {
      const s = k * dash * 2;
      const e = s + Math.min(dash, len - s);
      d += `M${ax + dx * s} ${ay + dy * s}L${ax + dx * e} ${ay + dy * e}`;
    }
  };
  seg(r.x, r.y, r.x + r.w, r.y);
  seg(r.x + r.w, r.y, r.x + r.w, r.y + r.h);
  seg(r.x + r.w, r.y + r.h, r.x, r.y + r.h);
  seg(r.x, r.y + r.h, r.x, r.y);
  return html`<path d=${d} stroke=${colour} stroke-width="1" fill="none" />`;
}

const rr = (r, rad, f, extra = {}) => html`<rect x=${r.x} y=${r.y} width=${Math.max(0, r.w)} height=${Math.max(0, r.h)} rx=${rad} fill=${f} ...${extra} />`;
const edge = (r, rad, w, colour) => html`<rect x=${r.x + w / 2} y=${r.y + w / 2} width=${Math.max(0, r.w - w)} height=${Math.max(0, r.h - w)} rx=${Math.max(0, rad - w / 2)} fill="none" stroke=${colour} stroke-width=${w} />`;
const inset = (r, d) => ({ x: r.x + d, y: r.y + d, w: r.w - 2 * d, h: r.h - 2 * d });

/** A building of the plan: its roof, its name, how far it is built (`depot::building`). */
function building(r, b, svg, words) {
  if (b.built) {
    svg.push(rr(r, 6, fillOf(ROOF)));
    svg.push(edge(r, 6, 1, 'var(--hub-hairline)'));
  } else svg.push(dashed(r, faint(0.7)));
  if (b.works) {
    svg.push(edge(inset(r, 2), 5, 1.5, `rgba(${WARN}, 0.8)`));
    words.push(html`<${TextIn} x=${r.x + 8} y=${r.y + r.h - 18} w=${r.w - 16} h=${14} px=${12.5} weight=${700} tone="warn">${b.works}</${TextIn}>`);
  }
  words.push(html`<${TextIn} x=${r.x + 8} y=${r.y + 4} w=${r.w - 16} h=${14} px=${12} weight=${700} tone=${b.built ? 'dim' : 'faint'}>${b.head}</${TextIn}>`);
}

/** A bus seen from above: the company's colour, its roof's hatch, its number where there is
 * room; lifted when it is the one picked (`depot::bus_top`). */
function busTop(r, bus, colour, lifted, pick) {
  const rgb = rgbOf(colour);
  const fs = Math.min(11, Math.max(7, r.w * 0.42));
  return html`<g class="cm-bus" onClick=${() => pick(bus.id)}><title>${bus.tip}</title>
    <rect x=${r.x - 2} y=${r.y - 2} width=${r.w + 4} height=${r.h + 4} fill="transparent" />
    ${lifted && rr(inset(r, -2), 4, 'var(--hub-text)')}
    ${rr(r, 3, `rgb(${rgb.join(',')})`)}
    <rect x=${r.x + 2} y=${r.y + 3} width=${Math.max(0, r.w - 4)} height=${Math.max(2, r.h * 0.12)} fill="rgba(0,0,0,0.22)" />
    ${r.h >= 30 && r.w >= 14 && html`<text x=${r.x + r.w / 2} y=${r.y + r.h * 0.6} font-size=${fs / 1.303} font-weight="700" fill=${inkOn(rgb)} text-anchor="middle" dominant-baseline="middle">${bus.number}</text>`}
  </g>`;
}

function Plan({ p, w, h, selected }) {
  if (h < 120 || w <= 0) return null;
  const pick = (id) => setUi('depot', { selected: selected === id ? null : id });
  const svg = [];
  const words = [];
  const colour = p.colour;
  const streetH = 30;
  const yard = { x: 0, y: 0, w, h: h - streetH - 10 };
  const street = { x: -6, y: yard.h + 10, w: w + 12, h: streetH };
  svg.push(rr(yard, 8, fillOf(ASPHALT)));
  svg.push(edge(yard, 8, 1, 'var(--hub-hairline)'));
  svg.push(rr(street, 0, fillOf(STREET)));
  for (let x = street.x + 8; x < street.x + street.w - 8; x += 26) svg.push(rr({ x, y: street.y + street.h / 2 - 0.75, w: 14, h: 1.5 }, 0, faint(0.5)));
  // the gate
  const gate = { x: yard.w / 2 - 30, y: yard.h - 1.5, w: 60, h: 3 };
  svg.push(rr(gate, 0, fillOf(ASPHALT)));
  svg.push(rr({ x: gate.x, y: gate.y + gate.h + 2, w: gate.w, h: street.y - gate.y - gate.h - 2 }, 0, fillOf(ASPHALT)));
  const buses = [];
  // the buildings along the top
  const pad = 12;
  const g = 10;
  const top = { x: pad, y: pad, w: yard.w - 2 * pad, h: Math.max(80, yard.h * 0.34) };
  const ow = top.w * 0.2;
  const ww = top.w * 0.48;
  const sw = top.w - ow - ww - 2 * g;
  const offices = { x: top.x, y: top.y, w: ow, h: top.h };
  building(offices, p.offices, svg, words);
  if (p.offices.built) {
    for (let k = 0; k < p.offices.floors; k++) svg.push(rr({ x: offices.x + 8, y: offices.y + 22 + k * 12, w: offices.w - 16, h: 8 }, 2, faint(0.35)));
    words.push(html`<${TextIn} x=${offices.x + 8} y=${offices.y + offices.h - 18} w=${offices.w - 16} h=${14} px=${12.5} tone="dim">${p.offices.room}</${TextIn}>`);
  }
  // the hall: a bay a level, the buses being worked on in them, the ones waiting before it
  const hall = { x: offices.x + ow + g, y: top.y, w: ww, h: top.h };
  building(hall, p.hall, svg, words);
  const bayW = (hall.w - 16) / p.hall.max;
  for (let k = 0; k < p.hall.max; k++) {
    const br = { x: hall.x + 8 + k * bayW + 2, y: hall.y + 22, w: bayW - 4, h: hall.h - 30 };
    if (k < p.hall.bays) {
      svg.push(rr(br, 3, fillOf(ASPHALT)));
      svg.push(rr({ x: br.x + br.w / 2 - 2, y: br.y + 6, w: 4, h: br.h - 12 }, 0, 'rgba(0,0,0,0.35)'));
      const bus = p.in_ws[k];
      if (bus) {
        const bw = Math.min(br.w * 0.62, 26);
        buses.push(busTop({ x: br.x + br.w / 2 - bw / 2, y: br.y + 6, w: bw, h: Math.min(br.h - 12, bw * 3.4) }, bus, colour, selected === bus.id, pick));
      }
    } else svg.push(dashed(br, faint(0.35)));
  }
  p.in_ws.slice(p.hall.bays, p.hall.bays + 6).forEach((bus, k) => {
    buses.push(html`<g class="cm-bus" onClick=${() => pick(bus.id)}><title>${bus.tip}</title>${rr({ x: hall.x + 8 + k * 30, y: hall.y + hall.h + 4, w: 24, h: 14 }, 3, `rgba(${WARN}, 0.8)`)}</g>`);
  });
  // the washing bay and the diesel station
  const side = { x: hall.x + ww + g, y: top.y, w: sw, h: top.h };
  const wash = { x: side.x, y: side.y, w: side.w, h: (side.h - g) * 0.5 };
  building(wash, p.wash, svg, words);
  if (p.wash.built) {
    const lane = { x: wash.x + 10, y: wash.y + 22, w: wash.w - 20, h: wash.h - 30 };
    svg.push(rr(lane, 3, `rgba(${EARLY}, 0.12)`));
    if (p.wash.gates) for (let k = 0; k < 2; k++) svg.push(rr({ x: lane.x + lane.w * (0.35 + 0.3 * k) - 1.5, y: lane.y, w: 3, h: lane.h }, 0, `rgba(${EARLY_SOFT}, 0.6)`));
  }
  const fuel = { x: side.x, y: wash.y + wash.h + g, w: side.w, h: (side.h - g) * 0.5 };
  building(fuel, p.fuel, svg, words);
  if (p.fuel.built) for (let k = 0; k < p.fuel.pumps; k++) svg.push(rr({ x: fuel.x + 16 + k * 18 - 5, y: fuel.y + fuel.h / 2 + 6 - 8, w: 10, h: 16 }, 2, 'rgba(var(--co-soft-rgb), 0.7)'));
  // the parking: rows of two blocks of twelve spaces, a level a block
  const pk = p.parking;
  const park = { x: pad, y: top.y + top.h + 26, w: yard.w - 2 * pad, h: Math.max(40, yard.h - pad - top.y - top.h - 26) };
  words.push(html`<${TextIn} x=${park.x} y=${park.y - 18} w=${park.w * 0.5} h=${14} px=${12} weight=${700} tone="dim">${pk.label}</${TextIn}>`);
  if (pk.works) words.push(html`<${TextIn} x=${park.x + park.w * 0.5} y=${park.y - 18} w=${park.w * 0.5} h=${14} px=${12.5} weight=${700} tone="warn" align="right">${pk.works}</${TextIn}>`);
  const rows = Math.floor(pk.blocks / 2);
  const bg = 14;
  const blockW = (park.w - bg) / 2;
  const blockH = Math.max(16, (park.h - (rows - 1) * 8) / rows);
  const slotW = blockW / pk.per;
  let n = 0;
  for (let b = 0; b < pk.blocks; b++) {
    const br = { x: park.x + (b % 2) * (blockW + bg), y: park.y + Math.floor(b / 2) * (blockH + 8), w: blockW, h: blockH };
    const built = b < pk.built;
    const buildingNow = !built && b === pk.built && pk.building;
    if (built) {
      for (let s = 0; s < pk.per; s++) {
        const sr = { x: br.x + s * slotW, y: br.y, w: slotW, h: br.h };
        svg.push(rr({ x: sr.x, y: sr.y, w: 1, h: sr.h }, 0, faint(0.55)));
        if (s + 1 === pk.per) svg.push(rr({ x: sr.x + sr.w - 1, y: sr.y, w: 1, h: sr.h }, 0, faint(0.55)));
        // (a charger at the head of every space it serves)
        if (n < pk.chargers) svg.push(html`<circle cx=${sr.x + sr.w / 2} cy=${sr.y + 3} r="2.2" fill=${`rgb(${OK})`} />`);
        const bus = p.parked[n];
        if (bus) {
          const bw = Math.min(slotW * 0.66, 22);
          const bh = Math.min(sr.h - 10, bw * 3.6);
          buses.push(busTop({ x: sr.x + sr.w / 2 - bw / 2, y: sr.y + 6, w: bw, h: bh }, bus, colour, selected === bus.id, pick));
        }
        n++;
      }
    } else svg.push(dashed(br, buildingNow ? `rgba(${WARN}, 0.8)` : faint(0.3)));
  }
  // the buses the yard has no space for: in the street
  p.parked.slice(n, n + 12).forEach((bus, k) => {
    const r = { x: street.x + 10 + k * 40, y: street.y + 7, w: 34, h: street.h - 14 };
    const rgb = rgbOf(colour);
    buses.push(html`<g class="cm-bus" onClick=${() => pick(bus.id)}><title>${bus.tip}</title>${rr(inset(r, -1.5), 3, `rgb(${WARN})`)}${rr(r, 3, `rgb(${rgb.join(',')})`)}</g>`);
  });
  return html`<div class="cm-plan" style=${`width:${w}px;height:${h}px`}>
    <svg width=${w} height=${h} style="position:absolute;left:0;top:0;overflow:visible">${svg}${buses}</svg>
    ${words}
  </div>`;
}

// --- the buildings ---------------------------------------------------------------------------

function Build({ b }) {
  const [ref, size] = useListSize();
  const W = Math.max(0, size.w - 10);
  return html`<div class="co-scroll cm-list" ref=${ref}>
    <div style=${`position:relative;height:${b.rows.length * 62}px`}>
      ${size.w > 0 && b.rows.map((a, k) => {
        const y = k * 62;
        const lw = textW(a.label, 14.5, 700);
        const tw = textW(a.level, 12, 700) + 16;
        const lx = 38 + lw + 12;
        return html`<div style=${box(0, y, W, 56)} class="cm-row">
          ${k > 0 && html`<i class="cm-hair" style="top:-3px"></i>`}
          <span class="cm-icon t-accent2" style="left:4px;top:9px"><${Icon} name=${a.icon} size=${22} /></span>
          <${TextIn} x=${38} y=${6} w=${W - 230} h=${24} px=${14.5} weight=${700}>${a.label}</${TextIn}>
          ${lx + tw < W - 190 && html`<span class="cm-icon" style=${`left:${lx}px;top:7px`}><${Tag} text=${a.level} tone="soft" /></span>`}
          <${TextIn} x=${38} y=${32} w=${W - 230} h=${20} px=${13.5} tone="soft">${a.effect}</${TextIn}>
          <span class="cm-act" style="right:0;top:9px;width:180px;height:38px">
            ${a.works
              ? html`<span class="cm-said t-warn">${a.works}</span>`
              : a.cost
                ? html`<${Btn} icon=${a.locked ? 'lock' : 'add'} label=${a.cost} title=${a.tip} style="width:180px;height:38px" onClick=${() => actMore('depot_build', { area: a.key })} />`
                : html`<span class="cm-said t-ok">${a.complete}</span>`}
          </span>
        </div>`;
      })}
    </div>
  </div>`;
}

// --- the workshop ---------------------------------------------------------------------------

function Workshop({ ws, selected }) {
  const [ref, size] = useListSize();
  if (ws.empty) return html`<p class="co-para" style="font-size:var(--co-rows)">${ws.empty}</p>`;
  const W = Math.max(0, size.w - 10);
  const rows = [];
  let y = 0;
  for (const j of ws.jobs) {
    rows.push(html`<div class="cm-row" style=${box(0, y, W, 50)}>
      <span style="position:absolute;left:0;top:4px"><${Plate} p=${{ number: j.number }} h=${20} /></span>
      <${TextIn} x=${plateW(j.number) + 10} y=${3} w=${W - plateW(j.number) - 140} h=${22} px=${14.5} weight=${700}>${j.title}</${TextIn}>
      <${TextIn} x=${plateW(j.number) + 10} y=${27} w=${W - plateW(j.number) - 140} h=${20} px=${13.5} weight=${500} tone=${j.tone}>${j.what}</${TextIn}>
      ${j.cost && html`<${TextIn} x=${W - 120} y=${2} w=${80} h=${18} px=${13.5} weight=${500} tone="soft" align="right">${j.cost}</${TextIn}>`}
      ${j.cancel && html`<${IconBtn} icon="close" size=${15} title=${ws.cancel_tip} style=${`position:absolute;left:${W - 30}px;top:2px;width:28px;height:28px`} onClick=${() => actMore('depot_cancel', { id: j.id })} />`}
    </div>`);
    y += 56;
  }
  if (ws.jobs.length) rows.push(html`<i class="cm-hair" style=${`top:${y - 3}px;width:${W}px`}></i>`);
  for (const b of ws.buses) {
    const on = selected === b.id;
    const pw = plateW(b.number);
    const lw = W * 0.42;
    const bw = Math.min((W * 0.58 - 12) / 3, 110);
    const subTone = b.tone === 'warn' ? 'warn' : on ? 'onaccent' : 'dim';
    rows.push(html`<div class="cm-row" style=${box(0, y, W, 50)}>
      <button type="button" class=${'cm-pick' + (on ? ' on' : '')} style=${box(0, 0, lw, 50)} onClick=${() => setUi('depot', { selected: on ? null : b.id })}></button>
      <span style="position:absolute;left:6px;top:4px;pointer-events:none"><${Plate} p=${{ number: b.number }} h=${20} /></span>
      <${TextIn} x=${pw + 14} y=${3} w=${lw - pw - 18} h=${22} px=${14} weight=${700} tone=${on ? 'onaccent' : 'text'} style="pointer-events:none">${b.name}</${TextIn}>
      <${TextIn} x=${pw + 14} y=${27} w=${110} h=${20} px=${13.5} weight=${500} tone=${subTone} style="pointer-events:none">${b.sub}</${TextIn}>
      <span title=${b.cond.tip} style=${box(pw + 128, 35, Math.max(10, lw - pw - 136), 5)}><${Meter} frac=${b.cond.frac} tone=${b.cond.tone} h=${5} /></span>
      ${b.jobs.map((k, i) => html`<${Btn} kind="ghost" label=${k.label} title=${k.tip} style=${box(W - (3 - i) * (bw + 4), 8, bw, 34) + ';padding:0'} onClick=${() => actMore('depot_order', { id: b.id, kind: k.kind })} />`)}
    </div>`);
    y += 56;
  }
  return html`<div class="co-scroll cm-list" ref=${ref}><div style=${`position:relative;height:${y}px`}>${size.w > 0 && rows}</div></div>`;
}

/** The width of a line's plate 20 high (`company::plate`). */
const plateW = (number) => Math.max(20 * 1.8, textW(number, 12, 900) + 14);

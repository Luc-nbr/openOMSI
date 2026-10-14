// The workshop job the player does himself (Trello "Tauri 6/6"), as our game-drawn launcher's
// repair game (`crates/omsi-launcher-ui/src/company/repair_game.rs`): the bus from the side in
// the company's colours with its eight parts marked on it; a part picked, checked - which takes
// a moment - what the check finds read and dealt with (replace, adjust, top up, or leave it), the
// job finished before the time runs out; then what each part was and what was done, and what
// the job saved. Calm: nothing moves but the clock, the check's progress and what is under the
// mouse. The job is the Rust side's (`company/repair.rs`): this keeps the clock and the check.

import { html, useEffect, useRef, useState, act, Icon, Btn, Section } from './kit.js';

/** Seconds a check takes (`repair_game::CHECK`). */
const CHECK = 1.2;
const GLASS = 'rgb(28,36,52)';

/** The bus from the side (`repair_game::draw_bus`), its parts as round marks; `onPick` gets the
 * part clicked. Drawn in a box of `w` x `h` (3.6 : 1). */
function BusSide({ g, w, h, selected, onPick }) {
  const [main, second] = g.colours;
  const bh = h * 0.84;
  const windows = [];
  const doors = [0.47, 0.79];
  const dw = 0.075;
  for (let x = 0.05; x < 0.86; ) {
    const ww = 0.08;
    const d = doors.find((d) => x + ww > d - 0.008 && x < d + dw + 0.008);
    if (d != null) {
      x = d + dw + 0.014;
      continue;
    }
    if (x + ww <= 0.875) windows.push(x);
    x += ww + 0.012;
  }
  const wy = bh * 0.14;
  const wh = bh * 0.34;
  return html`<div class="co-repair-bus" style=${`width:${w}px;height:${h}px`}>
    <svg width=${w} height=${h} viewBox=${`0 0 ${w} ${h}`}>
      <rect x="0" y="0" width=${w} height=${bh} rx=${h * 0.07} fill=${main} />
      <rect x="6" y=${bh * 0.66} width=${w - 12} height=${bh * 0.08} fill=${second} />
      ${windows.map((x) => html`<rect x=${w * x} y=${wy} width=${w * 0.08} height=${wh} rx="4" fill=${GLASS} />`)}
      <rect x=${w * 0.885} y=${bh * 0.08} width=${w * 0.1} height=${bh * 0.52} rx="6" fill=${GLASS} />
      ${doors.map((dx) => html`<rect x=${w * dx} y=${bh * 0.14} width=${w * dw} height=${bh * 0.8} rx="3" fill=${GLASS} />
        <rect x=${w * dx + (w * dw) / 2 - 0.75} y=${bh * 0.14} width="1.5" height=${bh * 0.8} fill=${main} fill-opacity="0.6" />`)}
      ${[0.21, 0.8].map((wx) => html`<circle cx=${w * wx} cy=${h * 0.86} r=${h * 0.14} fill="rgb(14,16,22)" /><circle cx=${w * wx} cy=${h * 0.86} r=${h * 0.07} fill="rgb(110,116,128)" />`)}
    </svg>
    ${g.parts.map((p) => {
      const tone = p.done ? 'ok' : p.inspected ? 'accent2' : 'white';
      return html`<button type="button" class=${'mark ' + tone + (selected === p.part ? ' on' : '')} style=${`left:${w * p.at[0]}px;top:${h * p.at[1]}px`} title=${p.label} onClick=${() => onPick(p.part)}>
        <${Icon} name=${p.icon} size=${17} /></button>`;
    })}
  </div>`;
}

/** The job under way (`repair_game::play`), or done (`results`). `g` is the job's view. */
export function RepairGame({ g }) {
  const [left, setLeft] = useState(g.seconds || 0);
  const [selected, setSelected] = useState('lights');
  const [checking, setChecking] = useState(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const stage = useRef();
  const ended = useRef(false);
  // the clock, and the check's moment
  useEffect(() => {
    if (g.done) return;
    const t0 = performance.now();
    const from = left;
    const k = setInterval(() => setLeft(Math.max(0, from - (performance.now() - t0) / 1000)), 100);
    return () => clearInterval(k);
  }, [g.done]);
  useEffect(() => {
    if (!g.done && left <= 0 && !ended.current) {
      ended.current = true;
      act('job_finish', { left: 0 });
    }
  }, [left <= 0]);
  useEffect(() => {
    if (!checking) return;
    const k = setTimeout(() => {
      act('job_inspect', { part: checking });
      setChecking(null);
    }, CHECK * 1000);
    return () => clearTimeout(k);
  }, [checking]);
  // (the bus as large as the stage lets it be)
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, [g.done]);

  if (g.done) {
    const h = g.head || {};
    return html`<div class="co-repair-done"><${Section} title=${g.title} style="height:100%">
      ${h.error
        ? html`<p class="co-para t-danger" style="font-size:11.9px;font-weight:500">${h.error}</p>`
        : html`<div class="top"><b class=${'pct t-' + h.tone}>${h.pct}</b><div><b class="bus cut">${g.bus}</b><span class=${'saved t-' + h.saved_tone}>${h.saved}</span></div></div>`}
      <p class="tally">${g.tally}</p>
      <div class="rows">${g.rows.map((r) => html`<div class="row"><${Icon} name=${r.good ? 'check_circle' : 'error'} size=${16} class=${r.good ? 't-ok' : 't-warn'} />
        <b class="cut">${r.label}</b><span class="cut reading">${r.reading}</span><span class=${'cut did ' + (r.good ? 't-dim' : 't-warn')}>${r.did}</span></div>`)}</div>
      <div class="foot"><${Btn} kind="primary" label=${g.back} style="width:220px;height:38px" onClick=${() => act('job_leave')} /></div>
    </${Section}></div>`;
  }
  const total = g.seconds || 1;
  const low = left < 15;
  const clock = `${Math.floor(left / 60)}:${String(Math.floor(left) % 60).padStart(2, '0')}`;
  const part = g.parts.find((p) => p.part === selected);
  const finish = () => {
    if (ended.current) return;
    ended.current = true;
    act('job_finish', { left });
  };
  const bw = Math.max(0, Math.min(box.w - 80, (box.h - 120) * 3.6));
  return html`<div class="co-repair">
    <div class="head">
      <div class="what"><b class="cut">${g.title}</b><span class="cut">${g.sub}</span></div>
      <div class="clock"><b class=${low ? 't-danger' : ''}>${clock}</b><div class="co-meter"><i style=${`width:${(left / total) * 100}%;background:var(--co-${low ? 'danger' : 'accent2'})`}></i></div></div>
      <${Btn} kind="primary" icon="check_circle" label=${g.finish} style="width:150px;height:38px" onClick=${finish} />
      <${Btn} kind="ghost" label=${g.leave} style="width:104px;height:38px" onClick=${() => act('job_leave')} />
    </div>
    <div class="body">
      <div class="stage co-section" ref=${stage}>
        ${bw > 0 && html`<div class="place" style=${`margin-top:-10px`}><${BusSide} g=${g} w=${bw} h=${bw / 3.6} selected=${selected} onPick=${setSelected} /></div>`}
        <p class="hint">${g.hint}</p>
      </div>
      ${part &&
      html`<div class="side co-section">
        <div class="name"><${Icon} name=${part.icon} size=${26} class="t-accent2" /><b class="cut">${part.label}</b></div>
        ${checking === part.part
          ? html`<span class="checking">${g.checking}</span><div class="co-meter checkbar"><i class="run" style=${`animation-duration:${CHECK}s`}></i></div>`
          : !part.inspected
            ? html`<p class="unchecked">${g.unchecked}</p><${Btn} kind="primary" icon="search" label=${g.check} style="width:100%;height:40px;margin-top:12px" onClick=${() => setChecking(part.part)} />`
            : html`<div class="co-caps">${g.finds}</div><p class="reading">${part.reading}</p>
              <div class="fixes">${g.fixes.map((f) => html`<${Btn} kind=${part.done === f.id ? 'primary' : ''} icon=${part.done === f.id ? 'check' : null} label=${f.label} style="height:40px"
                onClick=${() => act('job_apply', { part: part.part, fix: f.id }).then((r) => r?.next && setSelected(r.next))} />`)}</div>`}
      </div>`}
    </div>
  </div>`;
}

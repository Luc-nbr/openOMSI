// The company's overview, as our game-drawn launcher's (`company/overview.rs`): six figures
// across the top, the last days' results as a chart, today (the tours covered, the driver's next
// duty, what wants attention), the company's day as it was told, the last day closed and the
// way to the other companies.

import { html, setCo, setUi, act, cmd, useView, useSize, textW, goTab, Icon, Btn, Figure, Section, Meter, Switch, seen } from './kit.js';

export function OverviewPage() {
  const v = useView('overview');
  const [ref, size] = useSize();
  let body = null;
  if (v?.figures && size.w > 0) {
    const gap = 14;
    const fw = (size.w - 5 * gap) / 6;
    const fh = 100;
    const y = fh + gap;
    const leftW = (size.w - gap) * 0.62;
    const rightW = size.w - gap - leftW;
    const h1 = Math.max(220, (size.h - y - gap) * 0.52);
    const y2 = y + h1 + gap;
    const h2 = Math.max(140, size.h - y2);
    const h3 = Math.max(170, h2 * 0.6);
    const box = (x, yy, w, h) => `position:absolute;left:${x}px;top:${yy}px;width:${w}px;height:${h}px`;
    body = html`
      ${v.figures.map((f, k) => html`<div style=${box(k * (fw + gap), 0, fw, fh)} title=${f.tip}><${Figure} f=${f} small=${fw < 170} /></div>`)}
      <${Section} title=${v.chart_title} style=${box(0, y, leftW, h1)}><${Chart} c=${v.chart} w=${leftW - 36} h=${h1 - 58} /></${Section}>
      <${Today} t=${v.today} style=${box(leftW + gap, y, rightW, h1)} />
      <${Feed} f=${v.feed} style=${box(0, y2, leftW, h2)} />
      <${LastDay} l=${v.last} style=${box(leftW + gap, y2, rightW, h3)} h=${h3 - 58} />
      <${Section} title=${v.companies.title} style=${box(leftW + gap, y2 + h3 + gap, rightW, Math.max(80, h2 - h3 - gap))}>
        <div style="display:flex;align-items:center;gap:12px;margin-top:-4px"><span class="co-para grow cut">${v.companies.text}</span>
          <${Btn} icon="sync_alt" label=${v.companies.button} style="height:38px" onClick=${() => cmd('co_act', { act: 'play', speed: 0 }).finally(() => setCo({ chooser: 'companies' }))} /></div>
      </${Section}>`;
  }
  // (the last card at least 80 high runs past the page's foot on a low window, as the game-drawn
  // launcher's does: not cut off)
  return html`<div class="co-overview" ref=${ref} style="position:relative;height:100%;overflow:visible">${body}</div>`;
}

/** The last days' results as bars on a scale of round steps, the days under them
 * (`overview::chart_of`). */
export function Chart({ c, w, h }) {
  if (!c || w <= 0 || h <= 0) return null;
  if (c.empty) {
    return html`<div style=${`height:${h}px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center`}>
      <${Icon} name="signal_cellular_alt" size=${34} class="t-dim" />
      <b style="font-size:var(--co-head);color:var(--hub-text-soft);margin-top:8px">${c.title}</b>
      <span class="co-note t-dim" style="margin-top:6px">${c.text}</span>
    </div>`;
  }
  const labelW = Math.max(...c.scale.map(([, t]) => textW(t, 13.5, 400))) + 14;
  const plot = { x: labelW, y: 8, w: w - labelW - 4, h: h - 40 };
  const yOf = (v) => plot.y + (plot.h * (c.top - v)) / (c.top - c.bottom);
  const slot = plot.w / c.span;
  const bw = Math.max(2, Math.min(56, slot * 0.64));
  const zero = yOf(0);
  const every = Math.max(1, Math.ceil((textW(c.dates[c.dates.length - 1] || '', 13.5, 400) + 12) / Math.max(1, slot)));
  const ticks = [];
  for (let k = c.span - 1; k >= 0; k -= every) ticks.push(k);
  return html`<div style=${`position:relative;width:${w}px;height:${h}px`}>
    <svg width=${w} height=${h} style="position:absolute;left:0;top:0;overflow:visible">
      ${c.scale.map(([v]) => html`<rect x=${plot.x} y=${yOf(v)} width=${plot.w} height="1" style=${`fill:rgba(var(--co-white), ${seen(Math.abs(v) < 1 ? 0.22 : 0.06)})`} />`)}
      ${c.bars.map((b) => {
        const x = plot.x + b.k * slot + (slot - bw) / 2;
        const y = yOf(b.v);
        const r = b.v >= 0 ? [y, Math.max(1.5, zero - y)] : [zero, Math.max(1.5, y - zero)];
        return html`<g class="co-chartbar"><title>${b.tip}</title>
          <rect x=${plot.x + b.k * slot} y=${plot.y} width=${slot} height=${plot.h} fill="transparent" />
          <rect class=${b.v >= 0 ? 'up' : 'down'} x=${x} y=${r[0]} width=${bw} height=${r[1]} rx=${Math.min(2, bw * 0.4)} /></g>`;
      })}
      ${ticks.map((k) => html`<rect x=${plot.x + (k + 0.5) * slot} y=${plot.y + plot.h} width="1" height="4" style=${`fill:rgba(var(--co-white), ${seen(0.2)})`} />`)}
    </svg>
    ${c.scale.map(([v, t]) => html`<span class="co-axis" style=${`left:0;width:${labelW - 10}px;top:${yOf(v) - 9}px;text-align:right`}>${t}</span>`)}
    ${ticks.map((k) => html`<span class="co-axis" style=${`left:${plot.x + (k + 0.5) * slot - 60}px;width:120px;top:${plot.y + plot.h + 8}px;text-align:center`}>${c.dates[k]}</span>`)}
  </div>`;
}

/** A labelled bar of 0..1 (`kit::bar`): its words, the share on the right, the bar under them;
 * a click when it leads somewhere. */
export function Bar({ b, onClick }) {
  return html`<div class=${'co-bar-row' + (onClick ? ' go' : '')} title=${b.tip} onClick=${onClick}>
    <div class="words"><span class="cut">${b.label}</span>${b.right && html`<b class=${'t-' + b.tone}>${b.right}</b>`}</div>
    <${Meter} frac=${b.frac} tone=${b.tone} />
  </div>`;
}

/** Today (`overview::today`): the tours covered, the driver's next duty, what wants attention. */
function Today({ t, style }) {
  const c = t.covered;
  const n = t.next;
  return html`<${Section} class="co-today" title=${t.title} style=${style}>
    <div style="overflow:hidden;flex:1;min-height:0">
      ${c.bar ? html`<div style="margin-bottom:18px"><${Bar} b=${c.bar} onClick=${c.bar.click ? () => goTab(5) : null} /></div>` : html`<div class=${'co-para t-' + c.tone} style="line-height:24px;margin-bottom:10px;font-weight:500">${c.text}</div>`}
      ${n && n.duty && html`<div class="co-nextduty">
        <button type="button" class="co-row words" title=${n.tip} onClick=${() => (setUi('career', { part: 0 }), goTab(6))}><${Icon} name="person" size=${18} class="t-accent" /><span class="cut">${n.text}</span></button>
        <${Btn} kind="primary" icon="play_arrow" label=${n.button} style="width:104px;height:34px;padding:0" onClick=${() => act('drive_duty', { duty: n.duty, from: 0 })} />
      </div>`}
      ${n && n.none && html`<div class="co-nextduty">
        <span class="words"><${Icon} name="person" size=${18} class="t-dim" /><span class="cut t-soft">${n.none}</span></span>
        <${Btn} icon=${n.icon} label=${n.button} style="height:34px" onClick=${() => (n.easy ? (setUi('career', { part: 0 }), goTab(6)) : goTab(5))} />
      </div>`}
      ${t.alerts.length
        ? t.alerts.map((a) => html`<button type="button" class="co-row co-alert" title=${a.text} onClick=${() => goTab(a.tab)}>
            <${Icon} name=${a.icon} size=${18} class=${'t-' + a.tone} /><span class="cut grow">${a.text}</span><${Icon} name="chevron_right" size=${18} class="t-dim" /></button>`)
        : html`<div class="co-para" style="line-height:24px">${t.calm}</div>`}
    </div>
  </${Section}>`;
}

/** The company's day as it was told (`clock::feed`), the ordinary run of it on a switch. */
export function Feed({ f, style }) {
  return html`<${Section} title=${f.title} style=${style}
    tools=${html`<span style="width:170px;text-transform:none;font-weight:400"><${Switch} checked=${f.minor} label=${f.toggle} title=${f.tip} onChange=${(on) => act('feed_minor', { on })} /></span>`}>
    <div class="co-feed">
      ${f.empty
        ? html`<p class="co-para">${f.empty}</p>`
        : f.rows.map((r) =>
            r.day
              ? html`<div class="co-caps day">${r.day}</div>`
              : html`<div class="item"><b>${r.at}</b><i class=${'dot-' + r.tone}></i><span class=${r.minor ? 't-dim' : ''}>${r.text}</span></div>`,
          )}
    </div>
  </${Section}>`;
}

/** The last day closed (`overview::last_day`): its result and facts, the way to its report. */
function LastDay({ l, style, h }) {
  const r = l.report;
  const beside = h < 132;
  return html`<${Section} title=${l.title} style=${style}>
    ${!r
      ? html`<p class="co-para" style="overflow:hidden">${l.none}</p>`
      : html`<div class="co-lastday">
          <div class="top"><b class="cut">${r.date}</b><span class=${'res t-' + r.tone} title=${r.tip}>${r.result}</span></div>
          <div style=${`display:flex;gap:16px;align-items:${beside ? 'center' : 'flex-start'};flex-direction:${beside ? 'row' : 'column'}`}>
            <div class="facts">${r.facts.map((f) => html`<span class="cut">${f}</span>`)}</div>
            <${Btn} icon="receipt_long" label=${l.button} style="height:38px" onClick=${() => act('show_report')} />
          </div>
        </div>`}
  </${Section}>`;
}

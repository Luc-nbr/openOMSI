// The look the company's chooser, finances, tenders, goals and contracts, reputation and career
// pages share (Luc 2026-10-16, the approved concepts): rounded cards with a soft shadow on the
// content area, KPI cards on top, the orange accent, navy headings, chips, dots and the empty
// states with an icon and one line. Its own sheet (`company-pages-a.css`, every class `co-pg-…`)
// is loaded with the first of these pages.

import { html, Icon, fill } from './kit.js';

if (!document.querySelector('link[data-co-pages-a-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-pages-a.css';
  sheet.dataset.coPagesACss = '1';
  document.head.append(sheet);
}

/** A card: its title (and an icon, the tools on the right), its body. */
export function Card({ title, icon, tools, children, class: cls = '', style, sub }) {
  return html`<section class=${'co-pg-card ' + cls} style=${style}>
    ${(title || tools) &&
    html`<header class="co-pg-cardhead">
      ${icon && html`<span class="co-pg-cardicon"><${Icon} name=${icon} size=${16} /></span>`}
      <span class="co-pg-cardtitle"><b>${title}</b>${sub && html`<small>${sub}</small>`}</span>
      ${tools && html`<span class="co-pg-cardtools">${tools}</span>`}
    </header>`}
    ${children}
  </section>`;
}

/** A KPI card: its icon and label, the value large, a line under it (`f` as the views' figures:
 * label, value, under, tone). */
export function Kpi({ f, icon, onClick, title }) {
  if (!f) return null;
  const tag = onClick ? 'button' : 'div';
  return html`<${tag} type=${onClick ? 'button' : undefined} class=${'co-pg-kpi' + (onClick ? ' go' : '')} title=${title || f.tip || undefined} onClick=${onClick}>
    <span class="co-pg-kpihead">${icon && html`<span class=${'co-pg-kpiicon t-' + (f.icon_tone || 'accent')}><${Icon} name=${icon} size=${16} /></span>`}<span class="co-pg-kpilabel">${f.label}</span></span>
    <b class=${'co-pg-kpivalue t-' + (f.tone || 'text')}>${f.value}</b>
    ${f.under && html`<span class="co-pg-kpiunder">${f.under}</span>`}
  </${tag}>`;
}

/** A row of KPI cards, as many columns as fit. */
export const Kpis = ({ children, n = 4 }) => html`<div class="co-pg-kpis" style=${`--n:${n}`}>${children}</div>`;

/** An empty state: an icon in a soft circle, a title and one line, a button under it. */
export function Empty({ icon = 'inbox', title, text, children, small }) {
  return html`<div class=${'co-pg-empty' + (small ? ' small' : '')}>
    <span class="co-pg-emptyicon"><${Icon} name=${icon} size=${small ? 22 : 30} /></span>
    ${title && html`<b>${title}</b>`}
    ${text && html`<p>${text}</p>`}
    ${children}
  </div>`;
}

/** A chip: an icon and words on a tint of its tone. */
export function Chip({ text, tone = 'dim', icon, title }) {
  if (text == null || text === '') return null;
  return html`<span class=${'co-pg-chip t-' + tone} title=${title} style=${`background:${fill(tone, 0.14)}`}>${icon && html`<${Icon} name=${icon} size=${13} />`}${text}</span>`;
}

/** A dot of a tone (a contract's RAG rating). */
export const Dot = ({ tone = 'dim', title }) => html`<i class="co-pg-dot" title=${title} style=${`background:${fill(tone)}`}></i>`;

/** A bar of 0..1 in a tone. */
export function Track({ frac, tone = 'accent', h = 8 }) {
  const w = Math.max(0, Math.min(1, Number(frac) || 0)) * 100;
  return html`<span class="co-pg-track" style=${`height:${h}px`}><i style=${`width:${w}%;background:${fill(tone)}`}></i></span>`;
}

/** Horizontal bars, one a row: its label, its bar (`frac` of the longest), the amount and the
 * share (`rows`: { label, amount, frac, share, tone }). */
export function HBars({ rows, tone = 'accent' }) {
  return html`<div class="co-pg-hbars">${rows.map(
    (r) => html`<div class="co-pg-hbar" title=${r.tip}>
      <span class="co-pg-hbarlabel" title=${r.label}>${r.label}</span>
      <${Track} frac=${r.frac} tone=${r.tone || tone} h=${8} />
      <b class=${'t-' + (r.amount_tone || 'text')}>${r.amount}</b>
      <span class="co-pg-hbarshare">${r.share}</span>
    </div>`,
  )}</div>`;
}

/** A sparkline of `values` (none: nothing), its last point marked. */
export function Spark({ values, w = 220, h = 56, tone = 'accent', lo, hi }) {
  const vs = (values || []).filter((x) => Number.isFinite(x));
  if (vs.length < 2) return null;
  const min = lo ?? Math.min(...vs);
  const max = hi ?? Math.max(...vs);
  const span = Math.max(0.5, max - min);
  const pad = 4;
  const x = (k) => pad + ((w - 2 * pad) * k) / (vs.length - 1);
  const y = (v) => pad + (h - 2 * pad) * (1 - (v - min) / span);
  const pts = vs.map((v, k) => `${x(k).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const area = `${pad},${h - pad} ${pts} ${x(vs.length - 1).toFixed(1)},${h - pad}`;
  const col = fill(tone);
  return html`<svg class="co-pg-spark" viewBox=${`0 0 ${w} ${h}`} preserveAspectRatio="none" width="100%" height=${h} aria-hidden="true">
    <polygon points=${area} style=${`fill:${fill(tone, 0.14)}`} />
    <polyline points=${pts} fill="none" stroke=${col} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" />
    <circle cx=${x(vs.length - 1)} cy=${y(vs[vs.length - 1])} r="3.5" fill=${col} />
  </svg>`;
}

/** Tabs as the pages' segmented control, in the cards' look: [value, label] each. */
export function Tabs({ value, options, onChange }) {
  return html`<div class="co-pg-tabs" role="tablist">${options.map(
    ([v, l]) => html`<button type="button" role="tab" class=${v === value ? 'on' : ''} onClick=${() => v !== value && onChange(v)}>${l}</button>`,
  )}</div>`;
}

/** Five stars filled as far as `rating` goes. */
export function Stars({ rating, size = 16 }) {
  return html`<span class="co-pg-stars">${[0, 1, 2, 3, 4].map((k) => {
    const f = Math.max(0, Math.min(1, rating - k));
    return html`<i class=${f >= 0.75 ? 'full' : f >= 0.25 ? 'half' : ''}><${Icon} name="star" size=${size} /></i>`;
  })}</span>`;
}

// The pieces the redesigned pages of set B share (Research, Service control, Multiplayer,
// Sandbox, the Workshop): their sheet (`omsihub/company-pages-b.css`, loaded once with the first
// of them), a KPI card, a card with its head, a chip, a bar, a ring and the empty state - one
// look over the kit's own controls (`kit.js`).

import { html, Icon } from './kit.js';

if (!document.querySelector('link[data-co-pgb-css]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/company-pages-b.css';
  sheet.dataset.coPgbCss = '1';
  document.head.append(sheet);
}

/** A view's tone as this look's ("err" and "danger" alike; "text" and "soft" none). */
export function toneOf(t) {
  if (t === 'err' || t === 'danger') return 'danger';
  if (t === 'ok' || t === 'warn' || t === 'accent' || t === 'info') return t;
  if (t === 'accent2') return 'accent';
  if (t === 'dim' || t === 'faint') return 'dim';
  return '';
}

/** A KPI card: its icon in a tinted square (or `ring`, drawn in its place), its label, its value
 * and a line under it. */
export function Kpi({ icon, tone = 'accent', label, value, under, ring, title }) {
  return html`<div class="co-pgb-kpi" title=${title}>
    ${ring || html`<span class=${'co-pgb-kpi-ic ' + toneOf(tone)}><${Icon} name=${icon} size=${20} /></span>`}
    <span class="co-pgb-kpi-t">
      <span class="co-pgb-kpi-l">${label}</span>
      <b class="co-pgb-kpi-v">${value}</b>
      ${under && html`<span class="co-pgb-kpi-u">${under}</span>`}
    </span>
  </div>`;
}

/** A card: its icon and title, what it says on the right (`aside`), its body. */
export function Card({ icon, title, aside, children, class: cls = '', style }) {
  return html`<section class=${'co-pgb-card ' + cls} style=${style}>
    ${title != null && html`<div class="co-pgb-card-h">${icon && html`<span class="co-pgb-ic"><${Icon} name=${icon} size=${17} /></span>`}<h3 title=${typeof title === 'string' ? title : undefined}>${title}</h3>${aside && html`<span class="co-pgb-aside">${aside}</span>`}</div>`}
    ${children}
  </section>`;
}

/** A chip: a small pill with an icon and words, in a tone. */
export function Chip({ icon, text, tone = '', title }) {
  if (text == null || text === '') return null;
  return html`<span class=${'co-pgb-chip ' + toneOf(tone)} title=${title}>${icon && html`<${Icon} name=${icon} size=${12} />`}${text}</span>`;
}

/** A bar of 0..1 in a tone (`mark`: a tick at a share, as a minimum). */
export function Bar({ frac, tone = '', thin, mark }) {
  const w = Math.max(0, Math.min(1, Number(frac) || 0)) * 100;
  const m = mark != null ? Math.max(0, Math.min(1, mark)) * 100 : null;
  return html`<div class=${'co-pgb-bar ' + toneOf(tone) + (thin ? ' thin' : '')}><i style=${`width:${w}%`}></i>${m != null && html`<span class="co-pgb-mark" style=${`left:calc(${m}% - 1px)`}></span>`}</div>`;
}

/** A ring of 0..1 with its words in the middle (an SVG: no library). */
export function Ring({ frac, size = 46, stroke = 6, tone = 'ok', text }) {
  const r = (size - stroke) / 2;
  const len = 2 * Math.PI * r;
  const f = Math.max(0, Math.min(1, Number(frac) || 0));
  const colour = { ok: 'var(--pgb-ok)', warn: 'var(--pgb-warn)', danger: 'var(--pgb-danger)', accent: 'var(--pgb-accent)' }[toneOf(tone)] || 'var(--pgb-accent)';
  return html`<svg class="co-pgb-ring" width=${size} height=${size} viewBox=${`0 0 ${size} ${size}`} aria-hidden="true">
    <circle cx=${size / 2} cy=${size / 2} r=${r} fill="none" stroke="var(--pgb-field)" stroke-width=${stroke} />
    <circle cx=${size / 2} cy=${size / 2} r=${r} fill="none" stroke=${colour} stroke-width=${stroke} stroke-linecap="round" stroke-dasharray=${`${len * f} ${len}`} transform=${`rotate(-90 ${size / 2} ${size / 2})`} />
    ${text && html`<text x="50%" y="50%" dominant-baseline="central" text-anchor="middle" font-size=${size * 0.25} font-weight="800" fill="var(--pgb-ink)">${text}</text>`}
  </svg>`;
}

/** The empty state: one line with its icon (`ok`: the check in green). */
export function Empty({ icon = 'check', text, ok }) {
  return html`<div class=${'co-pgb-empty' + (ok ? ' ok' : '')}><span class="co-pgb-empty-ic"><${Icon} name=${icon} size=${16} /></span><span>${text}</span></div>`;
}

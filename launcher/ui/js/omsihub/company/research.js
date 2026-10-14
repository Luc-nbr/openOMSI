// The company's research, as our game-drawn launcher's (`company/research.rs`), City Bus
// Manager's tree: a strip with the cash, the trophies and the rating; the research under way -
// how far it is, who works on it, stopping it (clicked twice: it costs) -; and the sections,
// each with its title between two hairlines, a card a topic: its picture on its section's
// patterned colour, its stages, its price in money and trophies, and what it waits for (a
// lock), is doing (its progress) or has done (a check mark). Its info button says what it does.

import { html, useState, useEffect, useSize, textW, Icon, Btn, Meter } from './kit.js';
import { useMore, actMore, TextIn, box, fillOf, svgIcon } from './more.js';
import { dark } from '../theme.js';

const CARD_W = 200;
const CARD_H = 372;
const GAP = 14;

export function ResearchPage() {
  const v = useMore('research');
  const [ref, size] = useSize();
  let body = null;
  if (v?.strip && size.w > 0) {
    const W = size.w;
    const top = 58 + 74 + 16;
    body = html`<${Strip} s=${v.strip} W=${W} />
      <${Banner} b=${v.banner} W=${W} />
      <div class="co-scroll" style=${box(0, top, W, Math.max(0, size.h - top))}><${Tree} v=${v} w=${W - 12} /></div>`;
  }
  return html`<div class="cm-page" ref=${ref}>${body}</div>`;
}

/** The strip on top (`research::strip`): the cash, the trophies, the rating, in a pill. */
function Strip({ s, W }) {
  const widths = s.map((p) => textW(p.value, 18, 700) + 34);
  const w = widths.reduce((a, b) => a + b, 0) + 28 * (s.length - 1) + 36;
  let x = 18;
  return html`<div class="cm-strip" style=${box(W / 2 - w / 2, 0, Math.min(w, W), 44)}>
    ${s.map((p, k) => {
      const at = x;
      x += widths[k] + 28;
      return html`<span class="cm-strip-cell" title=${p.tip} style=${box(at, 0, widths[k], 44)}>
        <i class=${'cm-dot t-' + p.tone}><${Icon} name=${p.icon} size=${16} /></i>
        <${TextIn} x=${30} y=${0} w=${widths[k] - 30} h=${44} px=${18} weight=${700}>${p.value}</${TextIn}>
      </span>`;
    })}
  </div>`;
}

/** A topic's picture (`research::picture`): its section's colour with the section's sign
 * repeated over it, its own icon in the middle - dimmed under a lock when it cannot start. */
function picture(x, y, side, colour, motif, icon, locked) {
  const ground = locked ? colour.map((v) => v * 0.75) : colour;
  const ink = dark() ? 'rgba(255,255,255,0.13)' : 'rgba(22,20,26,0.31)';
  const out = [html`<rect x=${x} y=${y} width=${side} height=${side} rx="10" fill=${`rgb(${ground.map(Math.round).join(',')})`} />`];
  const step = Math.max(12, side / 4.5);
  const s = Math.max(8, step * 0.42);
  let row = 0;
  for (let yy = y + step * 0.45; yy < y + side - s * 0.5; yy += step * 0.86, row++) {
    for (let xx = x + step * (row % 2 === 0 ? 0.45 : 0.95); xx < x + side - s * 0.5; xx += step) out.push(svgIcon(motif, xx, yy, s, ink));
  }
  const cx = x + side / 2;
  const cy = y + side / 2;
  const big = side * 0.42;
  out.push(svgIcon(icon, cx, cy, big, `rgba(255,255,255,${locked ? 0.55 : 0.95})`));
  if (locked) {
    const lr = Math.max(10, side * 0.2);
    const deep = ground.map((v) => Math.round(v * 0.7));
    out.push(html`<circle cx=${cx} cy=${cy + big * 0.18} r=${lr} fill=${fillOf([230, 232, 238])} fill-opacity="0.9" />`);
    out.push(svgIcon('lock', cx, cy + big * 0.18, lr * 1.25, `rgb(${deep.join(',')})`));
  }
  return out;
}

/** The research under way, or none, and who works on it (`research::banner`); stopping it
 * asks a second click within a few seconds. */
function Banner({ b, W }) {
  const [armed, setArmed] = useState(0);
  useEffect(() => {
    if (!armed) return undefined;
    const t = setTimeout(() => setArmed(0), Math.max(0, armed - Date.now()));
    return () => clearTimeout(t);
  }, [armed]);
  if (!b) return null;
  if (b.none) {
    return html`<div class="cm-card" style=${box(0, 58, W, 74)}>
      <span class="cm-icon t-accent2" style="left:18px;top:25px"><${Icon} name="search" size=${24} /></span>
      <${TextIn} x=${56} y=${12} w=${W - 76} h=${24} px=${15} weight=${700}>${b.none}</${TextIn}>
      <${TextIn} x=${56} y=${38} w=${W - 76} h=${22} px=${13.5} tone=${b.staff_tone}>${b.staff}</${TextIn}>
    </div>`;
  }
  const stopW = Math.min(210, W * 0.3);
  const textWd = Math.max(60, W - 90 - stopW - 24);
  const on = armed > Date.now();
  const stop = () => {
    if (on) {
      setArmed(0);
      actMore('research_stop');
    } else setArmed(Date.now() + 4000);
  };
  return html`<div class="cm-card" style=${box(0, 58, W, 74)}>
    <svg width="48" height="48" style="position:absolute;left:14px;top:13px">${picture(0, 0, 48, b.colour, b.motif, b.icon, false)}</svg>
    <${TextIn} x=${76} y=${10} w=${textWd} h=${22} px=${15} weight=${700}>${b.what}</${TextIn}>
    <div style=${box(76, 38, textWd, 6)}><${Meter} frac=${b.share} tone=${b.tone} h=${6} /></div>
    <${TextIn} x=${76} y=${48} w=${textWd} h=${20} px=${13.5} tone=${b.staff_tone}>${b.line}</${TextIn}>
    <${Btn} kind=${on ? 'danger' : ''} icon="stop_circle" label=${on ? b.again : b.stop} title=${b.stop_tip} style=${box(W - stopW - 14, 17, stopW, 40) + ';height:40px;padding:0 8px'} onClick=${stop} />
  </div>`;
}

/** The sections and their cards. */
function Tree({ v, w }) {
  const cols = Math.max(1, Math.floor((w + GAP) / (CARD_W + GAP)));
  let y = 0;
  const parts = [];
  for (const sec of v.sections) {
    const tw = textW(sec.title, 18, 700);
    parts.push(html`<div style=${box(0, y, w, 30)}>
      <i class="cm-rule" style="left:0;width:28px"></i>
      <${TextIn} x=${40} y=${0} w=${tw + 4} h=${30} px=${18} weight=${700}>${sec.title}</${TextIn}>
      ${40 + tw + 14 < w && html`<i class="cm-rule" style=${`left:${40 + tw + 14}px;right:0`}></i>`}
    </div>`);
    y += 42;
    sec.cards.forEach((t, k) => {
      parts.push(html`<${Card} t=${t} sec=${sec} tip=${v.info_tip} r=${{ x: (k % cols) * (CARD_W + GAP), y: y + Math.floor(k / cols) * (CARD_H + GAP) }} />`);
    });
    y += Math.ceil(sec.cards.length / cols) * (CARD_H + GAP) + 22;
  }
  return html`<div style=${`position:relative;width:${w}px;height:${y}px`}>${parts}</div>`;
}

/** `text` broken at spaces into lines of `w` at most (`research::wrap`). */
function wrap(text, w, px, weight) {
  const lines = [];
  let line = '';
  for (const word of String(text).split(/\s+/).filter(Boolean)) {
    const next = line ? `${line} ${word}` : word;
    if (textW(next, px, weight) > w && line) {
      lines.push(line);
      line = word;
    } else line = next;
  }
  if (line) lines.push(line);
  return lines;
}

/** A price's row on a card (`research::row`): its icon in its colour, the amount, centred. */
function PriceRow({ y, w, icon, iconTone, text, tone }) {
  const tw = textW(text, 15, 700) + 24;
  const x = w / 2 - tw / 2;
  return html`<span class=${'cm-icon t-' + iconTone} style=${`left:${x}px;top:${y + 2}px`}><${Icon} name=${icon} size=${16} /></span>
    <${TextIn} x=${x + 22} y=${y} w=${tw - 20} h=${20} px=${15} weight=${700} tone=${tone}>${text}</${TextIn}>`;
}

/** A topic's card (`research::card`). */
function Card({ t, sec, tip, r }) {
  const w = CARD_W;
  const h = CARD_H;
  const fill = t.state === 'done' ? fillOf([62, 70, 88]) : t.state === 'locked' ? 'var(--hub-panel)' : 'var(--hub-field)';
  const side = w - 44;
  let y = 74 + side + 12;
  const info = (cx, cy) => html`<button type="button" class="cm-info" title=${tip} style=${box(cx - 12, cy - 12, 24, 24)} onClick=${() => actMore('research_info', { key: t.key })}>
    <svg width="24" height="24"><circle cx="12" cy="12" r="8" fill=${fillOf([30, 36, 50])} />${svgIcon('info', 12, 12, 24, 'currentColor')}</svg></button>`;
  const parts = [];
  if (t.state !== 'done') {
    const bw = side - 30;
    parts.push(html`<div class="cm-stages" style=${box(22, y, bw, 18)}>
      ${t.frac > 0 && html`<i style=${`width:${Math.max(8, bw * t.frac)}px`}></i>`}<span>${t.done} / ${t.stages}</span></div>`);
    parts.push(info(22 + bw + 16, y + 9));
    y += 30;
  } else parts.push(info(w - 22, 74 + 14));
  if (t.state === 'done') {
    parts.push(html`<span class="cm-done" style=${box(w / 2 - 20, y + 2, 40, 40)}><${Icon} name="check" size=${26} /></span>
      <${TextIn} x=${10} y=${y + 48} w=${w - 20} h=${20} px=${13.5} weight=${500} tone="soft" align="center">${t.said}</${TextIn}>`);
  } else if (t.state === 'running') {
    parts.push(html`<${TextIn} x=${10} y=${y} w=${w - 20} h=${22} px=${14.5} weight=${700} tone="accent2" align="center">${t.said}</${TextIn}>
      <${TextIn} x=${10} y=${y + 24} w=${w - 20} h=${20} px=${13.5} tone="soft" align="center">${t.left}</${TextIn}>`);
  } else {
    parts.push(html`<${PriceRow} y=${y} w=${w} icon="payments" iconTone="ok" text=${t.money} tone=${t.money_tone} />
      <${PriceRow} y=${y + 22} w=${w} icon="emoji_events" iconTone="warn" text=${t.trophies} tone=${t.trophies_tone} />`);
    const b = { x: 18, y: h - 46, w: w - 36, h: 34 };
    if (t.start) {
      parts.push(html`<${Btn} kind=${t.primary ? 'primary' : ''} icon="search" label=${t.start} style=${box(b.x, b.y, b.w, b.h) + ';height:34px;padding:0 6px'} onClick=${() => actMore('research_start', { key: t.key })} />`);
    } else if (t.needs) {
      const lines = wrap(t.needs, w - 44, 13.5, 500).slice(0, 2);
      const topY = b.y + b.h - lines.length * 18;
      parts.push(html`<span class="cm-icon t-dim" style=${`left:13px;top:${topY + 2}px`}><${Icon} name="lock" size=${14} /></span>`);
      lines.forEach((l, k) => parts.push(html`<${TextIn} x=${32} y=${topY + k * 18} w=${w - 44} h=${18} px=${13.5} weight=${500} tone="dim">${l}</${TextIn}>`));
    }
  }
  return html`<div class=${'cm-topic ' + t.state} style=${box(r.x, r.y, w, h) + `;background:${fill}`}>
    <div class="cm-topic-title" style=${box(10, 12, w - 20, 57)}><span>${t.title}</span></div>
    <svg width=${side} height=${side} style=${`position:absolute;left:22px;top:74px`}>${picture(0, 0, side, sec.colour, sec.motif, t.icon, t.state === 'locked')}</svg>
    ${parts}
  </div>`;
}

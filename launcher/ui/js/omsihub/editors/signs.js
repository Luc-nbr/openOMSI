// A bus's displays as the editors show them (`lineeditor::led_sign`, the depot editor's IBIS and
// side sign): the front matrix with the line number on the left and the destination in one or
// two rows beside it, lit dots on black (amber, or white) - a text too wide for its place gets
// smaller, as the matrix's narrower font - the IBIS's green LCD and the one-row side sign; and
// a text on the bus's own display in its own font (`displayfont`), drawn by the game program.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { call } from '../../api.js';
import { tr } from './kit.js';

const LIT_AMBER = 'rgb(255, 172, 28)';
const LIT_WHITE = 'rgb(236, 242, 255)';

let ctx = null;
/** How wide `text` is at `px` (CSS pixels) in Hanken Grotesk of `weight`. */
export function textWidth(text, px, weight = 700) {
  ctx = ctx || document.createElement('canvas').getContext('2d');
  ctx.font = `${weight} ${px}px "Hanken Grotesk", system-ui, sans-serif`;
  return ctx.measureText(text).width;
}

/** `px`, made smaller so that `text` fits `w` (not under 7 of the game-drawn launcher's). */
const fit = (text, px, w) => {
  const width = textWidth(text, px);
  return width > w ? Math.max(7 / 1.303, (px * w) / width) : px;
};

/** The front matrix (`led_sign`) `h` high (the game-drawn launcher's 62), as wide as its place
 * (`w` when given). */
export function LedSign({ line = '', l1 = '', l2 = '', white, w: given, h = 62, onClick, title }) {
  const ref = useRef();
  const [measured, setW] = useState(0);
  useEffect(() => {
    if (given || !ref.current) return;
    const ro = new ResizeObserver(() => ref.current && setW(ref.current.offsetWidth));
    ro.observe(ref.current);
    setW(ref.current.offsetWidth);
    return () => ro.disconnect();
  }, [given]);
  const w = given || measured || 300;
  const lit = white ? LIT_WHITE : LIT_AMBER;
  const inner = { w: w - 14, h: h - 14 };
  const ln = String(line).trim();
  const nw = ln ? Math.max(textWidth(ln, 23, 800) + 7.7, inner.h * 0.9) : 0;
  const dw = inner.w - nw - (ln ? 6 : 0);
  const one = !l2;
  const px = one ? fit(l1, 16.1, dw) : Math.min(fit(l1, 11.5, dw), fit(l2, 11.5, dw));
  return html`<div class="ed-led" ref=${ref} style=${`${given ? `width:${given}px;` : ''}height:${h}px;--lit:${lit}`} onClick=${onClick} title=${title ? tr(title) : undefined}>
    <div class="in">
      ${ln && html`<b class="no" style=${`width:${nw}px`}>${ln}</b>`}
      <div class="to" style=${`font-size:${px}px`}>${one ? html`<span>${l1}</span>` : html`<span>${l1}</span><span>${l2}</span>`}</div>
      <i class="dots"></i>
    </div>
  </div>`;
}

/** The IBIS's green LCD with the destination as it shows it. */
export const IbisLcd = ({ text, title }) => html`<div class="ed-lcd" title=${title ? tr(title) : undefined}>${text}</div>`;

/** The one-row side sign, lit. */
export const SideSign = ({ text, white, title }) => html`<div class="ed-side" style=${`--lit:${white ? LIT_WHITE : LIT_AMBER}`} title=${title ? tr(title) : undefined}>${text}</div>`;

const previews = new Map();

/** `text` on `bus`'s own destination display in its own font (`DisplayFonts::preview`), drawn
 * by the game program: `{ picture, too_wide, font, none, failed }`, or null while it is drawn. */
export function useBusFont(bus, text) {
  const key = `${bus}|${text}`;
  const [, tick] = useState(0);
  useEffect(() => {
    if (!bus || !text || previews.has(key)) return;
    previews.set(key, null);
    call('hub_display_preview', { bus, text })
      .then((p) => previews.set(key, p))
      .catch(() => previews.set(key, { failed: true }))
      .finally(() => tick((n) => n + 1));
  }, [key]);
  return bus && text ? previews.get(key) ?? null : undefined;
}

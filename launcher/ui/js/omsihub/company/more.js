// The rest of the bus company's pages (Trello "Tauri 3/6"), after our game-drawn launcher's
// (`crates/omsi-launcher-ui/src/company/`): the planning without the RosterFlow add-on, the
// depot, the concessions, the fleet map, the research, the workshop's stores and the contracts.
// Their views and actions are `omsi-launcher-api`'s `omsihub/company_more` (`co_more_view`,
// `co_more_act`), drawn with phase 2's pieces (`kit.js`) and a few of their own here: words
// placed as the game-drawn launcher places them (`TextIn`), money written as the page's
// language writes it, and a colour of the dark look as the light look draws it.

import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { html, useEffect, useRef, useState, cmd, setCo, handOver, goTab, refresh } from './kit.js';
import { useCallback } from '../../../vendor/preact-htm.js';
import { toast } from '../../store.js';
import { dark, lightGround, chosen } from '../theme.js';
import { SYMBOLS } from './symbols.js';

// (this part's own look, loaded with its first page)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/company-more.css';
document.head.append(sheet);

/** The tabs this part draws (`company::TABS`): the planning, the depot, the concessions, the
 * fleet map, the research, the workshop and the contracts. */
export const MORE_TABS = [7, 8, 9, 10, 11, 12];

/** A view of this part (`co_more_view`), asked again whenever the company changed, an action was
 * taken, or `args` differ; the last one is shown until the new one comes (`kit::useView`). */
export function useMore(name, args = {}, on = true) {
  const gen = S.co.shell?.gen ?? 0;
  const key = name + '|' + JSON.stringify(args);
  const [st, setSt] = useState({ key: '', v: null });
  const lang = language();
  useEffect(() => {
    if (!on) return undefined;
    let gone = false;
    cmd('co_more_view', { view: name, ...args })
      .then((v) => !gone && setSt({ key, v }))
      .catch((e) => !gone && setSt({ key, v: { error: e.message } }));
    return () => {
      gone = true;
    };
  }, [key, gen, lang, on, S.co.bump]);
  return on ? st.v : null;
}

/** An action of this part (`co_more_act`): its popup shown, a page handed over or gone to, and
 * everything asked again (`kit::act`). */
export async function actMore(name, args = {}) {
  let r;
  try {
    r = await cmd('co_more_act', { act: name, ...args });
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
  r = r || {};
  if (r.popup) setCo({ popup: r.popup });
  if (r.hand_over) handOver(r.hand_over);
  if (r.tab != null) goTab(r.tab);
  refresh();
  return r;
}

// --- numbers ---------------------------------------------------------------------------------

/** How the page's language writes a number (`words::style`): thousands, decimal mark, the euro. */
function style() {
  const lang = language();
  if (lang === '' || lang === 'en') return [',', '.', 'before'];
  if (lang === 'nl') return ['.', ',', 'spaced'];
  if (['de', 'it', 'es', 'pt', 'pt-pt', 'tr', 'da', 'id'].includes(lang)) return ['.', ',', 'after'];
  return [' ', ',', 'after'];
}

/** A number with `places` decimals as the language writes it (`words::num`). */
export function num(x, places = 0) {
  const [th, dec] = style();
  const v = Number.isFinite(x) ? x : 0;
  const text = Math.abs(v).toFixed(places);
  const [whole, frac] = text.split('.');
  let out = v < 0 && /[1-9]/.test(text) ? '-' : '';
  for (let i = 0; i < whole.length; i++) {
    if (i > 0 && (whole.length - i) % 3 === 0) out += th;
    out += whole[i];
  }
  return frac ? out + dec + frac : out;
}

/** Whole euros of `cents` (`words::eur`). */
export function eur(cents) {
  const n = num(Math.round(cents / 100), 0);
  const [sign, rest] = n.startsWith('-') ? ['-', n.slice(1)] : ['', n];
  const at = style()[2];
  return at === 'before' ? `${sign}€${rest}` : at === 'spaced' ? `${sign}€ ${rest}` : `${sign}${rest} €`;
}

// --- colours -------------------------------------------------------------------------------

/** The dark look's greys and their light counterparts (`theme::greys`). */
const GREYS = [
  [[20, 26, 38], [255, 254, 252]],
  [[27, 34, 49], [245, 242, 237]],
  [[33, 40, 54], [237, 233, 227]],
  [[38, 46, 62], [229, 224, 217]],
  [[96, 104, 122], [134, 128, 126]],
  [[149, 157, 176], [100, 95, 98]],
  [[196, 202, 214], [70, 66, 72]],
  [[232, 235, 242], [30, 28, 34]],
];
const mean = (c) => (c[0] + c[1] + c[2]) / 3;
const smooth = (lo, hi, x) => {
  const t = Math.max(0, Math.min(1, (x - lo) / (hi - lo)));
  return t * t * (3 - 2 * t);
};

/** A solid fill drawn for the dark look as the light look draws it (`theme::to_light` for a
 * fill): the look's greys go over to the light ones, a colour stays. */
export function fillOf(rgb) {
  if (dark()) return `rgb(${rgb.join(',')})`;
  const hi = Math.max(...rgb);
  const lo = Math.min(...rgb);
  const m = mean(rgb);
  if (lo >= 246 || hi <= 6) return `rgb(${rgb.join(',')})`;
  const points = [[mean([9, 12, 24]), lightGround(chosen())], ...GREYS.map(([d, l]) => [mean(d), l]), [255, [22, 20, 26]]];
  const at = (x) => {
    if (x <= points[0][0]) return points[0][1];
    for (let k = 1; k < points.length; k++) {
      const [m0, a] = points[k - 1];
      const [m1, b] = points[k];
      if (x <= m1) {
        const t = (x - m0) / Math.max(1e-3, m1 - m0);
        return a.map((v, i) => v + (b[i] - v) * t);
      }
    }
    return points[points.length - 1][1];
  };
  const w =(1 - smooth(38, 56, hi - lo)) * smooth(1, 5, rgb[2] - rgb[0]) * smooth(11, 13.5, m);
  const n = at(m);
  return `rgb(${rgb.map((v, i) => Math.round(v + (n[i] - v) * w)).join(',')})`;
}

// --- words placed --------------------------------------------------------------------------

/** Words in a box as the game-drawn launcher's `text_in` places them: one line, upright in the
 * middle of the box, cut with an ellipsis; `px` its size there (drawn smaller by 1.303 here). */
export function TextIn({ x, y, w, h, px = 14.5, weight = 400, tone, color, align = 'left', title, children, style = '', cls = '' }) {
  const just = align === 'center' ? 'center' : align === 'right' ? 'flex-end' : 'flex-start';
  const ink = color ? `color:${color};` : '';
  return html`<div class=${`cm-txt ${tone ? 't-' + tone : ''} ${cls}`} title=${title} style=${`left:${x}px;top:${y}px;width:${Math.max(0, w)}px;height:${h}px;font-size:${px / 1.303}px;font-weight:${weight};justify-content:${just};text-align:${align};${ink}${style}`}><span class="cut">${children}</span></div>`;
}

/** A Material Symbol inside an SVG drawing: centred on (cx, cy), `size` points wide. */
export function svgIcon(name, cx, cy, size, colour, extra = {}) {
  const d = SYMBOLS[name] || SYMBOLS.info;
  return html`<svg x=${cx - size / 2} y=${cy - size / 2} width=${size} height=${size} viewBox="0 -960 960 960" ...${extra}><path d=${d} fill=${colour} /></svg>`;
}

/** The size of a scrolled list as the game-drawn launcher's `scroll_area` has it: its whole
 * width, the scrollbar laid over the right 10 points its rows leave free (`kit::useSize`
 * measures without the scrollbar). */
export function useListSize() {
  const [size, setSize] = useState({ w: 0, h: 0 });
  const ro = useRef(null);
  const ref = useCallback((el) => {
    ro.current?.disconnect();
    ro.current = null;
    if (!el) return;
    const measure = () => setSize((s) => (s.w === el.offsetWidth && s.h === el.offsetHeight ? s : { w: el.offsetWidth, h: el.offsetHeight }));
    ro.current = new ResizeObserver(() => requestAnimationFrame(measure));
    ro.current.observe(el);
    measure();
  }, []);
  useEffect(() => () => ro.current?.disconnect(), []);
  return [ref, size];
}

/** A box's place as CSS. */
export const box = (x, y, w, h) => `position:absolute;left:${x}px;top:${y}px;width:${Math.max(0, w)}px;height:${Math.max(0, h)}px`;

/** The RosterFlow add-on on: its planning is phase 4's - opened in our game-drawn launcher until
 * then (a hook: `window.omsiHubRosterFlow`, when it is there, draws it here). */
export function rosterFlowPage() {
  const page = typeof window !== 'undefined' && window.omsiHubRosterFlow;
  return page || null;
}

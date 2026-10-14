// The layout checker (Luc, Beta 53: "check the app's overall scalability, make sure it always
// fits the screen and nothing lies over anything else"): on whatever the page shows now - any
// page, step, dialog or tab, at any window size and "Launcher size" - what does not fit:
//
// - the page scrolling sideways, or a part lying (partly) outside the window where nothing
//   scrolls it back;
// - a button, field or link another part lies over (its middle answers to something else), or
//   cut off by a box around it that does not scroll, or smaller than 18 px as laid out;
// - words cut off without an ellipsis and the whole of them in a tooltip, or cut off at the
//   foot of a box of a fixed height;
// - words running out of their box sideways, over what lies beside it;
// - two parts of one row or grid lying over each other.
//
// Menus, cards, dialogs, tooltips and tours lie over the page on purpose: what they cover is not
// counted (a layer of their own: placed, and 30 or more above the page). The self-check runs it
// at every page and state it shows (`probe`, `js/omsihub/selftest.js`), and `sweep` goes through
// the pages at several Launcher sizes for the pictures' grid of window sizes
// (`window.hubLayout`, driven over the page's debugging port).

import { S, set } from '../store.js';

/** Below this a button is too small to hit (px as laid out at 100 %). */
const MIN_HIT = 18;
/** At most this many findings of one page are told. */
const MAX_TOLD = 14;

const INTERACTIVE = 'button, a[href], input:not([type=hidden]), select, textarea, [role=button], [role=tab], [tabindex="0"]';

/** A short name of an element for the findings: its tag, classes and words. */
export function nameOf(el) {
  if (!el || el.nodeType !== 1) return String(el);
  const cls = [...el.classList].slice(0, 3).join('.');
  const text = (el.getAttribute('aria-label') || el.title || el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 28);
  return `${el.tagName.toLowerCase()}${cls ? '.' + cls : ''}${text ? ` "${text}"` : ''}`;
}

function visible(el, cs = getComputedStyle(el)) {
  if (cs.display === 'none' || cs.visibility === 'hidden' || cs.visibility === 'collapse') return false;
  if (Number(cs.opacity) < 0.05) return false;
  const r = el.getBoundingClientRect();
  return r.width > 0.5 && r.height > 0.5;
}

/** Whether `el` or one of its ancestors is a layer over the page (a menu, card, dialog, tooltip,
 * tour): placed (absolute or fixed) with a z-index of 30 or more, or a dialog by its role. */
const layerCache = new WeakMap();
function layerOf(el) {
  for (let e = el; e && e !== document.body; e = e.parentElement) {
    if (layerCache.has(e)) return layerCache.get(e);
    const cs = getComputedStyle(e);
    const z = Number(cs.zIndex);
    if ((cs.position === 'fixed' || cs.position === 'absolute') && z >= 30) {
      layerCache.set(e, e);
      return e;
    }
    if (e.getAttribute('role') === 'dialog' || e.getAttribute('role') === 'tooltip') return e;
  }
  return null;
}

/** The part of `el`'s box that is seen: cut to every box around it that clips (overflow other
 * than visible) - `hidden` by those that do not scroll (what they cut is lost), `box` by all of
 * them - and whether one that scrolls cut it (then what it cut can be scrolled to). */
function seenBox(el) {
  const r = el.getBoundingClientRect();
  let box = { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  let hidden = { ...box };
  let scrolls = false;
  let clipped = false;
  let inScroller = false;
  let tiny = false;
  const cut = (b, p) => ({ left: Math.max(b.left, p.left), top: Math.max(b.top, p.top), right: Math.min(b.right, p.right), bottom: Math.min(b.bottom, p.bottom) });
  const smaller = (a, b) => a.left > b.left + 0.5 || a.top > b.top + 0.5 || a.right < b.right - 0.5 || a.bottom < b.bottom - 0.5;
  // (a placed part is cut only by the boxes from its containing block up: one that is absolute
  // by its nearest placed or transformed ancestor's, one that is fixed by a transformed one's)
  let pos = getComputedStyle(el).position;
  for (let e = el.parentElement; e && e !== document.documentElement; e = e.parentElement) {
    const cs = getComputedStyle(e);
    const holds = cs.transform !== 'none' || cs.filter !== 'none' || /paint|layout|strict|content/.test(cs.contain);
    if (pos === 'absolute' && cs.position === 'static' && !holds) continue;
    if (pos === 'fixed' && !holds) continue;
    pos = cs.position === 'absolute' || cs.position === 'fixed' ? cs.position : 'static';
    const ox = cs.overflowX;
    const oy = cs.overflowY;
    if (ox === 'visible' && oy === 'visible') continue;
    const p = e.getBoundingClientRect();
    const canScroll = (ox === 'auto' || ox === 'scroll' || oy === 'auto' || oy === 'scroll') && (e.scrollHeight > e.clientHeight + 1 || e.scrollWidth > e.clientWidth + 1);
    const next = cut(box, p);
    if (smaller(next, box) && canScroll) scrolls = true;
    box = next;
    // (what a box that scrolls holds is reached by scrolling it: the boxes round that one cut
    // the box itself, not what it holds)
    // (a box of a pixel or two that clips is the "for screen readers only" kind: hidden on purpose)
    if ((p.width <= 2 || p.height <= 2) && !canScroll) tiny = true;
    if (canScroll) inScroller = true;
    else if (!inScroller) {
      const h = cut(hidden, p);
      if (smaller(h, hidden)) clipped = true;
      hidden = h;
    }
  }
  return { box, hidden, scrolls, clipped, rect: r, tiny };
}

const area = (b) => Math.max(0, b.right - b.left) * Math.max(0, b.bottom - b.top);

/** Whether `el` has words of its own (a text node with something in it). */
function ownText(el) {
  for (const n of el.childNodes) if (n.nodeType === 3 && n.textContent.trim()) return true;
  return false;
}

/** A tooltip with the whole of the words: on the element or a near ancestor. */
function titled(el) {
  for (let e = el, k = 0; e && k < 4; e = e.parentElement, k++) if (e.title || e.getAttribute('aria-label')) return true;
  // (an ellipsis gets its words as a tooltip under the mouse: `autotitle.js`)
  return !!window.hubAutoTitle;
}

/** What does not fit on the page as it is now: a list of findings ("rule: element (how)"). */
export function checkLayout(root = document.body) {
  const out = [];
  // (the stage's scale: the Launcher size and a window under 1440 x 820, `hub.js`)
  const stage = document.querySelector('.hub-stage');
  const k = stage && stage.offsetWidth ? stage.getBoundingClientRect().width / stage.offsetWidth : 1;
  const W = innerWidth;
  const H = innerHeight;
  const tell = (rule, el, how = '') => out.push(`${rule}: ${nameOf(el)}${how ? ` (${how})` : ''}`);
  const doc = document.scrollingElement || document.documentElement;
  if (doc.scrollWidth > W + 1) out.push(`the page scrolls sideways (${doc.scrollWidth} px in a ${W} px window)`);
  const all = [...root.querySelectorAll('*')].filter((el) => !(el instanceof SVGElement) || el.tagName.toLowerCase() === 'svg');
  const shown = new Set();
  for (const el of all) {
    const cs = getComputedStyle(el);
    if (!visible(el, cs)) continue;
    // (the opening over the page)
    if (el.closest('.hub-intro')) continue;
    shown.add(el);
  }
  const isShown = (el) => shown.has(el);
  for (const el of shown) {
    const cs = getComputedStyle(el);
    const parent = el.parentElement;
    const { box, hidden, scrolls, clipped, rect, tiny } = seenBox(el);
    if (tiny) continue;
    const interactive = el.matches(INTERACTIVE) && !el.disabled;
    // outside the window, where nothing scrolls it back (told for the first part that leaves)
    const out1 = (b) => b.left < -1 || b.top < -1 || b.right > W + 1 || b.bottom > H + 1;
    if (!scrolls && out1(box) && area(box) > 4 && !(parent && isShown(parent) && out1(seenBox(parent).box))) {
      tell('outside the window', el, `${Math.round(box.left)},${Math.round(box.top)} to ${Math.round(box.right)},${Math.round(box.bottom)}`);
    }
    if (interactive) {
      // cut off by a box that does not scroll
      if (clipped && area(hidden) < area(rect) * 0.6 && area(hidden) > 0) tell('cut off', el, `${Math.round((area(hidden) / area(rect)) * 100)} % seen`);
      // too small to hit, as laid out (the Launcher size the player chose scales it all: a part
      // drawn small at 60 % is as small as the player asked for)
      const w0 = rect.width / k;
      const h0 = rect.height / k;
      if (area(box) > 0 && (w0 < MIN_HIT - 0.5 || h0 < MIN_HIT - 0.5) && !el.matches('input[type=checkbox], input[type=radio], input[type=range], input[type=color]')) {
        tell('too small', el, `${Math.round(w0)} x ${Math.round(h0)} px at 100 %`);
      }
      // another part over its middle
      const cx = (Math.max(box.left, 0) + Math.min(box.right, W)) / 2;
      const cy = (Math.max(box.top, 0) + Math.min(box.bottom, H)) / 2;
      if (area(box) > 16 && cx > 0 && cy > 0 && cx < W && cy < H) {
        const hit = document.elementFromPoint(cx, cy);
        if (hit && hit !== el && !el.contains(hit) && !hit.contains(el)) {
          const over = layerOf(hit);
          // (a layer over the page covers on purpose; a label's own parts answer for it)
          if (!(over && !over.contains(el)) && !hit.closest('label')?.contains(el) && !hit.closest('.hub-intro')) tell('covered', el, `by ${nameOf(hit)}`);
        }
      }
    }
    // words (or a picture) cut by a box round them that does not scroll
    // (a few px of a line's box under a field's edge cut no letter: only a quarter of its height
    // or more, or its side)
    const cutW = rect.width - Math.max(0, hidden.right - hidden.left);
    const cutH = rect.height - Math.max(0, hidden.bottom - hidden.top);
    if (!interactive && clipped && (ownText(el) || el.tagName === 'IMG' || el.tagName === 'svg') && (cutW > 3 || cutH > Math.max(3, rect.height * 0.25))) {
      const by = (() => {
        for (let e = el.parentElement; e; e = e.parentElement) {
          const c2 = getComputedStyle(e);
          if (c2.overflowX !== 'visible' || c2.overflowY !== 'visible') return e;
        }
        return null;
      })();
      if (!(by && getComputedStyle(by).textOverflow === 'ellipsis' && titled(by))) tell('cut off', el, `${Math.round(cutW)} x ${Math.round(cutH)} px cut, by ${nameOf(by)}`);
    }
    // words cut off without an ellipsis and a tooltip, or at the foot of a box
    if (ownText(el) && el.clientWidth > 0) {
      const ox = cs.overflowX;
      if (ox !== 'visible' && el.scrollWidth > el.clientWidth + 1 && !(cs.textOverflow === 'ellipsis' && titled(el))) {
        tell('words cut off', el, cs.textOverflow === 'ellipsis' ? 'an ellipsis but no tooltip' : 'no ellipsis');
      }
      if (cs.overflowY !== 'visible' && cs.overflowY !== 'auto' && cs.overflowY !== 'scroll' && el.scrollHeight > el.clientHeight + 2 && !cs.webkitLineClamp?.match?.(/\d/)) {
        tell('words cut off at the foot', el, `${el.scrollHeight} of ${el.clientHeight} px`);
      }
    }
    // words running out of their box sideways, over what lies beside it (a row's words that do
    // not fit and are not shortened: RosterFlow's status bar over its issues, Beta 53)
    if (ownText(el) && parent && isShown(parent) && /^(static|relative|sticky)$/.test(cs.position) && !interactive) {
      const pcs = getComputedStyle(parent);
      const pr = parent.getBoundingClientRect();
      const inner = { left: pr.left + parseFloat(pcs.borderLeftWidth) * k, right: pr.right - parseFloat(pcs.borderRightWidth) * k };
      const over = Math.max(rect.right - inner.right, inner.left - rect.left);
      if (pcs.overflowX === 'visible' && over > 3 && !/^(inline|contents)$/.test(pcs.display) && box.right > box.left) tell('runs out of its box', el, `${Math.round(over)} px past ${nameOf(parent)}`);
    }
    // two parts of one row or grid over each other
    const d = cs.display;
    if (d === 'flex' || d === 'inline-flex' || d === 'grid' || d === 'inline-grid') {
      const kids = [...el.children].filter((k) => isShown(k) && /^(static|relative|sticky)$/.test(getComputedStyle(k).position));
      for (let a = 0; a < kids.length; a++) {
        const ra = kids[a].getBoundingClientRect();
        for (let b = a + 1; b < kids.length && b < a + 12; b++) {
          const rb = kids[b].getBoundingClientRect();
          const w = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
          const h = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
          if (w > 3 && h > 3) tell('over each other', kids[a], `and ${nameOf(kids[b])}, ${Math.round(w)} x ${Math.round(h)} px`);
        }
      }
    }
  }
  // (one finding a part: the first rule it broke)
  const seen = new Set();
  return out.filter((f) => {
    const key = f.replace(/ \(.*$/, '');
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** The findings as a check's words: how many, and the first of them. */
export function told(findings) {
  return findings.length ? `${findings.length}: ${findings.slice(0, MAX_TOLD).join('; ')}` : 'fits';
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The Launcher sizes the checks go through: the smallest, 100 %, 125 %, 150 % and the largest
 * (the steps Ctrl and the wheel give). */
export const SCALES = [0.6, 1, 1.25, 1.5, 2];

/** `f`'s state checked at each Launcher size in `scales` (the setting kept as it was after). */
export async function atScales(scales, name, record) {
  const kept = S.settings?.launcher_scale;
  for (const sc of scales) {
    set({ settings: { ...S.settings, launcher_scale: sc } });
    await sleep(350);
    record(`${name} @ ${Math.round(sc * 100)} %`, checkLayout());
  }
  set({ settings: { ...S.settings, launcher_scale: kept } });
  await sleep(200);
}

/** The self-check's probe: the page as it is now, told as a check of its own. */
export function probe(check, name) {
  const f = checkLayout();
  check(`layout: ${name}`, f.length === 0, told(f));
  return f;
}

if (typeof window !== 'undefined') window.hubLayout = { checkLayout, told, atScales, SCALES };

// The bus company's shared pieces (Trello "Tauri 2/6"), after our game-drawn launcher's
// `company/kit.rs` and `company/mod.rs`: what the company's screen keeps, the commands of the
// bus company (`omsi-launcher-api`'s `omsihub/company`: views, actions, the clock), and the
// pieces every page is drawn of - icons, toned words, sections, figures, tags, a line's plate,
// the company's mark, the dialog's frame and the popup that says why something cannot be done.
//
// The views come translated and formatted from Rust; a word's colour comes as a tone name
// ("text", "soft", "dim", "faint", "ok", "warn", "danger", "accent", "accent2", "early",
// "line"), drawn as `.t-<tone>` (company.css).

import { html, useCallback, useEffect, useRef, useState, useErrorBoundary } from '../../../vendor/preact-htm.js';
import { S, set, toast } from '../../store.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { Icon as HubIcon, tr } from '../bits.js';
import { SYMBOLS, STROKES } from './symbols.js';
// (the editors a hand-over opens: the line editor, the livery studio)
import * as editors from '../editors/index.js';

export { html, useEffect, useRef, useState, tr };

/** The start screen's page (`company::HOME_TAB`) and the pages handed over elsewhere
 * (`HANDED_OVER`): none since phase 3 (`pages.js`; the planning hands RosterFlow over itself
 * while its add-on is on). */
export const HOME = 100;
export const HANDED = [];
/** Casual's pages (`casual::BUSES` ...). */
export const CASUAL = { BUSES: 101, LINES: 102, DRIVERS: 103, MONEY: 104, COMPANY: 105, SHIFTS: 106, SHOP: 107, NEW_LINE: 108 };

/** What the company's screen keeps between its renders: the page, the start's step, the
 * founding, the dialog open, the popup, the shell the clock sends, each page's own choices. */
export function freshCo(over = {}) {
  return { tab: HOME, chooser: 'mode', wizard: null, dialog: null, shell: null, popup: null, bump: 0, notice: null, ui: {}, ...over };
}
if (!S.co) S.co = freshCo();

/** Change what the company's screen keeps (and draw again). */
export function setCo(patch) {
  set({ co: { ...S.co, ...(typeof patch === 'function' ? patch(S.co) : patch) } });
}

/** A page's own choices (`S.co.ui[page]`), with what it starts with. */
export function ui(page, start = {}) {
  return { ...start, ...(S.co.ui[page] || {}) };
}

export function setUi(page, patch) {
  setCo((c) => ({ ui: { ...c.ui, [page]: { ...(c.ui[page] || {}), ...patch } } }));
}

/** The company's commands a friend of "Play together" relays to the boss's launcher while he
 * looks at the boss's company's pages (`together::relay::RELAYED`). */
const RELAYED = new Set(['co_tick', 'co_view', 'co_act', 'co_more_view', 'co_more_act', 'co_line_of']);

/** A command of the bus company in the page's language (Beta 61: relayed to the boss's launcher
 * on the pages of a company joined with "Play together", `S.co.remote`). */
export const cmd = (name, args = {}) => {
  const lang = language();
  if (S.co?.remote && RELAYED.has(name)) return call('tg_relay', { cmd: name, args: { ...args, lang }, lang });
  return call(name, { ...args, lang });
};

/** The view `name` of the company (`co_view`), asked again whenever the company changed (the
 * shell's generation), an action was taken, or `args` differ. Until the new one comes the
 * last one is shown (no flash between a page's tabs). */
export function useView(name, args = {}, on = true) {
  const gen = S.co.shell?.gen ?? 0;
  const key = name + '|' + JSON.stringify(args);
  const [st, setSt] = useState({ key: '', v: null });
  const lang = language();
  useEffect(() => {
    if (!on) return undefined;
    let gone = false;
    cmd('co_view', { view: name, ...args })
      .then((v) => !gone && setSt({ key, v }))
      .catch((e) => !gone && setSt({ key, v: { error: e.message } }));
    return () => {
      gone = true;
    };
  }, [key, gen, lang, on, S.co.bump]);
  return on ? st.v : null;
}

/** The view is the one asked for (not the last one shown meanwhile). */
export const fresh = (v) => v && !v.error;

/** Take an action (`co_act`): what it says - a popup, a page handed over, a page to go to - is
 * carried out here; the answer goes back to the page for what is its own. */
export async function act(name, args = {}) {
  let r;
  try {
    r = await cmd('co_act', { act: name, ...args });
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
  r = r || {};
  if (r.popup) setCo({ popup: r.popup });
  // (Omsi-Hub 4/6: what came with it - the duty launcher's duty - for a page that takes it)
  if (r.hand_over) handOver(r.hand_over, r);
  if (r.tab != null) goTab(r.tab);
  refresh();
  return r;
}

/** Ask the clock's shell again at once (after an action), and every view with it. */
export function refresh() {
  setCo((c) => ({ bump: c.bump + 1 }));
  cmd('co_tick', { modal: true, still: true })
    .then((sh) => takeShell(sh))
    .catch(() => {});
}

let lastShell = '';
/** The shell the clock sent: kept when it changed; what it says in its corner shown. */
export function takeShell(sh) {
  if (!sh) return;
  const notices = sh.notices || [];
  const key = JSON.stringify({ ...sh, notices: [] });
  const patch = {};
  if (key !== lastShell) {
    lastShell = key;
    patch.shell = sh;
  }
  if (notices.length) patch.notice = { ...notices[notices.length - 1], at: Date.now() };
  if (sh.popup) patch.popup = sh.popup;
  if (Object.keys(patch).length) setCo(patch);
}

/** Go to a page of the company (one another part of this launcher takes: `handOver`). */
export function goTab(tab) {
  if (HANDED.includes(tab)) {
    handOver(String(tab));
    return;
  }
  setCo({ tab, dialog: null });
}

/** The pages of a later phase that take a hand-over themselves (Omsi-Hub 4/6: RosterFlow takes
 * the Planning page with the add-on on): each is asked in turn with the page and what came with
 * it, and says (a promise of) true when it took it. */
export const handOverHooks = [];

/** Open what the company's pages hand over (`company::handed`'s parts): RosterFlow takes the
 * planning and a line's timetable with its add-on on (`handOverHooks`); a page of the company is
 * opened here; the workshop's jobs, a line's sheet on the planning and our editors (the line
 * editor, the livery studio) are this launcher's own. (The game draws no launcher of its own
 * any more: there is nowhere else to hand over to.) */
export async function handOver(page, opts = {}) {
  // (Beta 61: on a company joined with "Play together" only its own pages open, and RosterFlow
  // on the boss's company - its commands relayed: `rosterflow/state.js`; the editors and the
  // boss's game are his launcher's)
  if (S.co?.remote) {
    for (const take of handOverHooks) {
      try {
        if (await take(page, opts)) return;
      } catch {
        /* (not taken) */
      }
    }
    if (/^\d+$/.test(String(page))) setCo({ tab: Number(page), dialog: null, popup: null });
    else toast(tr('Only the boss can do this, on his own launcher.'), 'error');
    return;
  }
  for (const take of handOverHooks) {
    try {
      if (await take(page, opts)) return;
    } catch {
      /* (not taken: handed over) */
    }
  }
  // (Omsi-Hub 6/6: a page of the company this launcher draws itself - the concessions after a
  // tender was asked for, the planning without RosterFlow - is opened here)
  if (/^\d+$/.test(String(page)) && !HANDED.includes(Number(page))) {
    setCo({ tab: Number(page), dialog: null, popup: null });
    return;
  }
  if (S.boot?.selftest) {
    setCo({ handed: page });
    return;
  }
  // (the workshop's jobs: the career's part, where the job is done here - Omsi-Hub 6/6)
  if (page === 'workshop') {
    setUi('career', { part: 3 });
    setCo({ tab: 6, dialog: null });
    return;
  }
  // (a line's trips and timetable without RosterFlow: the planning's own sheet over it, Omsi-Hub
  // 6/6 - "plan-line=<line>|<date>|<tour>")
  if (String(page).startsWith('plan-line=')) {
    const [line, date, tour] = String(page).slice('plan-line='.length).split('|');
    const r = await cmd('co_more_act', { act: 'plan_timetable', line, date: date || '', tour: tour || '' }).catch(() => null);
    if (r?.plan_edit) {
      setUi('planning', { edit: Date.now(), arm: null, sel: null });
      setCo({ tab: 5, dialog: null });
      return;
    }
  }
  if (await inEditor(String(page), opts)) return;
  console.warn('bus company: nothing opens', page);
}

/** A hand-over our own editors take (`company::handed`'s editor pages): "line-editor[=<id>]",
 * "map-line=<name>", "casual-line=<freq>" (`opts.newWithDays`: its timetable), and
 * "livery=<bus>|<paint>|<bus id>". Opened here - the company stopped and saved first
 * (`co_hand_over`), read again when an editor wrote it, and Back comes to the company's page;
 * another page: false. A line's timetable ("timetable=<line>", the planning's "plan-line=<line>|...") is
 * RosterFlow's Timeplan with the add-on on (its hook takes it first), else the line editor's
 * Timetable tab on that line. */
async function inEditor(page, opts) {
  const [what, ...rest] = page.split('=');
  const arg = rest.join('=');
  if (what === 'timetable' || what === 'plan-line') {
    const r = await cmd('co_hand_over', { page }).catch(() => null);
    return !!r?.edit && inEditor(r.edit, { ...opts, tab: 'timetable' });
  }
  if (!['line-editor', 'map-line', 'casual-line', 'livery'].includes(what)) return false;
  const fn = what === 'livery' ? editors.openLivery : editors.openLineEditor;
  const company = S.co.shell?.id;
  if (!company) return false;
  try {
    await cmd('co_hand_over', { page });
  } catch {
    return false;
  }
  const hooks = {
    company,
    back: 'company',
    onCompanyChanged: async (id) => {
      await cmd('co_reread', { id: id || company }).catch(() => null);
      refresh();
    },
  };
  if (what === 'livery') {
    const [bus, paint, vehicle] = arg.split('|');
    fn({ ...hooks, bus, paint: paint || '', vehicle: vehicle ? Number(vehicle) : undefined });
  } else {
    fn({
      ...hooks,
      // (a line just confirmed comes by the line editor's id: its name is the company's)
      onTimeplan: async ({ id, name } = {}) => {
        const line = name || (id != null && (await cmd('co_line_of', { id }).catch(() => null))?.name);
        if (line) handOver('timetable=' + line);
      },
      ...(opts.tab ? { tab: opts.tab } : {}),
      ...(what === 'line-editor' && arg ? { line: Number(arg) } : {}),
      ...(what === 'map-line' ? { mapLine: arg } : {}),
      ...(what === 'casual-line' ? { newWithDays: opts.newWithDays } : {}),
    });
  }
  return true;
}

/** A popup's way on (`company::go`), Casual's to its own pages (`casual::go`). */
export function goKey(to) {
  const casual = !!S.co.shell?.casual;
  setCo({ popup: null, dialog: null });
  if (to.startsWith('to-normal')) {
    const tab = Number(to.split(':')[1] || HOME);
    setCo({ dialog: { kind: 'confirm', what: 'game_mode', name: 'normal', tab } });
    return;
  }
  if (to === 'loan') {
    act('open_loan');
    return;
  }
  if (casual) {
    const page = { depot: CASUAL.COMPANY, finances: CASUAL.MONEY, planning: CASUAL.SHIFTS, lines: CASUAL.LINES, dealer: CASUAL.SHOP, hire: CASUAL.DRIVERS }[to];
    if (page) goTab(page);
    return;
  }
  switch (to) {
    case 'progress':
      setUi('career', { part: 1 });
      goTab(6);
      break;
    case 'courses':
      setUi('career', { part: 2 });
      goTab(6);
      break;
    case 'depot':
      goTab(7);
      break;
    case 'finances':
      goTab(4);
      break;
    case 'planning':
      goTab(5);
      break;
    case 'lines':
      goTab(3);
      break;
    case 'research':
      goTab(10);
      break;
    case 'dealer':
      setUi('fleet', { tab: 1 });
      goTab(1);
      break;
    case 'hire':
      setUi('people', { tab: 1, hire_role: 0 });
      goTab(2);
      break;
    case 'training':
      setUi('people', { tab: 2, group: 2, first: 0 });
      goTab(2);
      break;
    case 'licences':
      setUi('people', { tab: 2, group: 1, first: 0 });
      goTab(2);
      break;
    default:
  }
}

// --- the pieces ---------------------------------------------------------------------------

/** A Material Symbol by its name (every one the game-drawn launcher has). */
export function Icon({ name, size = 20, class: cls = '' }) {
  const d = SYMBOLS[name];
  const lines = !d && STROKES[name];
  if (lines) {
    return html`<svg class=${'hub-icon ' + cls} viewBox="0 0 24 24" width=${size} height=${size} fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${lines.map((p) => html`<path d=${p} />`)}</svg>`;
  }
  if (!d) return html`<${HubIcon} name=${name} size=${size} class=${cls} />`;
  return html`<svg class=${'hub-icon ' + cls} viewBox="0 -960 960 960" width=${size} height=${size} fill="currentColor" aria-hidden="true"><path d=${d} /></svg>`;
}

/** Words in a tone: `[text, tone]` as the views send them, or text and a tone. */
export function T({ t, tone, text, class: cls = '', title }) {
  const [s, k] = t ? (Array.isArray(t) ? t : [t, tone]) : [text, tone];
  return html`<span class=${`t-${k || 'text'} ${cls}`} title=${title}>${s}</span>`;
}

/** A heading in capitals. */
export const Caps = ({ text, class: cls = '' }) => html`<div class=${'co-caps ' + cls}>${text}</div>`;

/** A section: its hairline round it and its name in capitals. */
export function Section({ title, children, class: cls = '', style, tools }) {
  return html`<section class=${'co-section ' + cls} style=${style}>
    ${title != null && html`<div class="co-caps" style=${`display:flex;align-items:center;gap:8px${tools ? ';overflow:visible' : ''}`}><span class="grow cut">${title}</span>${tools && html`<span style="display:flex;align-items:center;height:16px">${tools}</span>`}</div>`}
    ${children}
  </section>`;
}

/** A figure: its name, its value in big type and a line under it. */
export function Figure({ f, small }) {
  if (!f) return null;
  return html`<div class=${'co-figure' + (small ? ' small' : '')}>
    <div class="co-caps">${f.label}</div>
    <b class=${'t-' + (f.tone || 'text')} title=${f.value}>${f.value}</b>
    ${f.under && html`<span title=${f.under}>${f.under}</span>`}
  </div>`;
}

/** A thin bar of 0..1 in a tone's colour, on a see-through track (`field`: on the field's grey,
 * as Casual's bars are drawn). */
export function Meter({ frac, tone = 'accent', h = 6, style = '', field = false }) {
  const w = Math.max(0, Math.min(1, Number(frac) || 0)) * 100;
  return html`<div class="co-meter" style=${`height:${h}px;border-radius:${h / 2}px;${field ? 'background:var(--hub-field);' : ''}${style}`}><i style=${`width:${w}%;background:${fill(tone)}`}></i></div>`;
}

/** A tone as a fill. */
export function fill(tone, alpha = 1) {
  const rgb = { ok: 'var(--co-ok-fill)', warn: 'var(--co-warn-fill)', danger: 'var(--co-danger-fill)', early: 'var(--co-early-fill)', accent: 'var(--hub-accent-rgb)', accent2: 'var(--hub-accent-soft-rgb)', line: '255, 210, 63' }[tone];
  if (rgb) return alpha === 1 ? `rgb(${rgb})` : `rgba(${rgb}, ${seen(alpha, TONES[tone] || TONES.accent)})`;
  if (tone === 'dim' || tone === 'faint') return alpha === 1 ? 'var(--hub-text-faint)' : `rgba(var(--co-white), ${seen(0.16 * alpha)})`;
  return `rgba(var(--co-white), ${seen(0.3 * alpha)})`;
}

/** The meanings' colours (`--co-*-fill`; the accent as Omsi-Hub's orange: `seen` hardly
 * depends on it). */
const TONES = { ok: [52, 178, 110], warn: [232, 170, 70], danger: [217, 58, 48], early: [122, 168, 255], accent: [245, 134, 32], accent2: [249, 188, 132], line: [255, 210, 63] };
const toLin = (c) => ((c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const toSrgb = (l) => 255 * (l <= 0.0031308 ? 12.92 * l : 1.055 * l ** (1 / 2.4) - 0.055);

/** The rgba alpha that shows what the game-drawn launcher's alpha `a` of `top` shows: it
 * blends in linear light (stronger on the night, weaker on the cream), and its light look draws
 * a see-through white as see-through ink (`theme.rs`: overlay_alpha). As company.css's
 * `--aw-*` and `--ac-*` (launcher/tools/omsihub-linear-alpha.py). */
export function seen(a, top = [255, 255, 255]) {
  const light = document.documentElement.dataset.theme === 'light';
  const under = light ? [252, 234, 208] : [9, 12, 24];
  let t = top;
  let al = a;
  if (light && top.every((c) => c > 250)) {
    t = [22, 20, 26];
    al = Math.min(0.6, 0.85 * Math.sqrt(a));
  }
  const xs = [0, 1, 2].filter((k) => Math.abs(t[k] - under[k]) > 40).map((k) => (toSrgb(al * toLin(t[k]) + (1 - al) * toLin(under[k])) - under[k]) / (t[k] - under[k]));
  return xs.length ? Math.round(Math.max(0, Math.min(1, xs.reduce((s, x) => s + x, 0) / xs.length)) * 1000) / 1000 : a;
}

/** A small coloured tag with its words. */
export function Tag({ text, tone = 'accent' }) {
  return html`<span class=${'co-tag t-' + tone} style=${`background:${fill(tone, 0.16)}`}>${text}</span>`;
}

/** `#rrggbb` as [r, g, b] (the line editor's blue where it is none). */
export function rgbOf(hex) {
  const h = String(hex || '').trim().replace(/^#/, '');
  const v = /^[0-9a-f]{6}$/i.test(h) ? parseInt(h, 16) : 0x2a75f7;
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

/** The ink on a colour: dark on a light one, white on the others (`ownlines::ink_on`). */
export function inkOn(rgb) {
  const [r, g, b] = rgb.map((v) => v / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6 ? '#1a1400' : '#ffffff';
}

/** A line's plate: its number on its colour (the timetable's yellow for a line of the map). */
export function Plate({ p, h = 22, style = '' }) {
  if (!p) return null;
  const own = p.colour && /^#?[0-9a-f]{6}$/i.test(p.colour.trim());
  const rgb = own ? rgbOf(p.colour) : [255, 210, 63];
  const bg = `rgb(${rgb.join(',')})`;
  return html`<span class="co-plate" style=${`height:${h}px;min-width:${h * 1.8}px;padding:0 ${h * 0.35}px;font-size:${(h * 0.6) / 1.303}px;background:${bg};color:${inkOn(rgb)};border-radius:${Math.max(4, h * 0.22)}px;${style}`}>${p.number}</span>`;
}

/** The company's mark: its logo picture on white, or its short name on its colours. */
export function Mark({ mark, size = 44 }) {
  if (!mark) return html`<span class="co-mark" style=${`width:${size}px;height:${size}px`}></span>`;
  const st = `width:${size}px;height:${size}px`;
  if (mark.logo) return html`<span class="co-mark logo" style=${st}><img src=${mark.logo} alt="" /></span>`;
  const main = rgbOf(mark.colours?.[0]);
  const second = rgbOf(mark.colours?.[1]);
  return html`<span class="co-mark" style=${`${st};background:rgb(${main.join(',')})`}>
    <span class="band" style=${`background:rgb(${second.join(',')})`}></span>
    <span class="short" style=${`font-size:${Math.min(size * 0.34, 18) / 1.303}px;color:${inkOn(main)}`}>${mark.short}</span>
  </span>`;
}

/** A game mode's pill (`mode_pill`): its icon and name, Casual's in the accent. */
export function ModePill({ mode, onClick, title }) {
  if (!mode) return null;
  const c = 'co-pill' + (mode.casual ? ' casual' : '');
  const body = html`<${Icon} name=${mode.icon} size=${13} />${mode.label}`;
  return onClick ? html`<button type="button" class=${c} onClick=${onClick} title=${title}>${body}</button>` : html`<span class=${c}>${body}</span>`;
}

/** A button as the company's pages have them. */
export function Btn({ label, icon, kind = '', onClick, disabled, title, class: cls = '', style, big, small, size }) {
  const c = ['co-btn', kind, big && 'big', small && 'small', cls].filter(Boolean).join(' ');
  return html`<button type="button" class=${c} style=${style} onClick=${onClick} disabled=${disabled} title=${title}>
    ${icon && html`<${Icon} name=${icon} size=${size || (big ? 19 : 18)} />`}${label}
  </button>`;
}

/** Segments side by side, one chosen (`Ui::segmented`): [value, label] each. */
export function Seg({ value, options, onChange, style = '', off = [] }) {
  return html`<div class="co-seg" style=${style}>${options.map(([v, l]) => html`<button type="button" class=${v === value ? 'on' : ''} disabled=${off.includes(v)} onClick=${() => v !== value && onChange(v)}>${l}</button>`)}</div>`;
}

/** Chips, one chosen (`Ui::chips`). */
export function Chips({ value, options, onChange, h = 34 }) {
  return html`<div class="co-chips">${options.map(([v, l]) => html`<button type="button" style=${`height:${h}px;border-radius:${h / 2}px`} class=${v === value ? 'on' : ''} onClick=${() => v !== value && onChange(v)}>${l}</button>`)}</div>`;
}

/** A switch with its words on the left. */
export function Switch({ checked, onChange, label, title, style }) {
  return html`<label class="co-switch" title=${title} style=${style}><span>${label}</span><input type="checkbox" checked=${!!checked} onChange=${(e) => onChange(e.currentTarget.checked)} /><i></i></label>`;
}

/** A choice of options in a field (`Ui::select`). */
export function Select({ value, options, onChange, style = '', h = 40 }) {
  return html`<span class="co-select" style=${style}>
    <select class="co-field" style=${`height:${h}px`} value=${String(value)} onChange=${(e) => onChange(Number(e.currentTarget.value))}>
      ${options.map((o, k) => html`<option value=${String(k)}>${o}</option>`)}
    </select>
    <${Icon} name="expand_more" size=${20} />
  </span>`;
}

/** A round button with only an icon. */
export function IconBtn({ icon, title, onClick, size = 18, style }) {
  return html`<button type="button" class="co-iconbtn" title=${title} style=${style} onClick=${onClick}><${Icon} name=${icon} size=${size} /></button>`;
}

/** A dialog's frame (`kit::frame`): its icon and title, the cross that closes it, the body,
 * and the foot's buttons; Escape closes it (when it may be closed). */
export function Dialog({ icon, title, w = 560, h, onClose, foot, children, bodyStyle = '', class: cls = '' }) {
  useEffect(() => {
    if (!onClose) return undefined;
    const k = (e) => e.key === 'Escape' && (e.stopPropagation(), onClose());
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, [onClose]);
  return html`<div class="co-scrim" onMouseDown=${(e) => e.stopPropagation()}>
    <div class=${'co-dialog ' + cls} style=${`width:${w}px;${h ? `height:${h}px;` : ''}`}>
      <div class="head"><${Icon} name=${icon} size=${24} /><h2 title=${title}>${title}</h2>
        ${onClose && html`<${IconBtn} icon="close" title=${tr('Close')} onClick=${onClose} />`}</div>
      <div class="body" style=${bodyStyle}>${children}</div>
      ${foot && html`<div class="foot">${foot}</div>`}
    </div>
  </div>`;
}

/** The popup that says why something cannot be done and what opens it (`kit::draw_popup`). */
export function Popup({ p, onClose }) {
  const close = onClose || (() => setCo({ popup: null }));
  useEffect(() => {
    const k = (e) => e.key === 'Enter' && close();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  const foot = p.go
    ? html`<span class="rest"></span><${Btn} label=${tr('OK')} onClick=${close} /><${Btn} kind="primary" icon=${p.go.icon} label=${p.go.label} onClick=${() => goKey(p.go.to)} />`
    : html`<span class="rest"></span><${Btn} kind="primary" label=${tr('OK')} onClick=${close} />`;
  return html`<${Dialog} icon=${p.icon} title=${p.title} w=${560} onClose=${close} foot=${foot}>
    <p class="co-para">${p.text}</p>
    ${p.unlock && html`<div class="co-unlock" style="margin-top:16px"><${Icon} name="key" size=${18} />${p.unlock}</div>`}
  </${Dialog}>`;
}

/** A view not come yet: nothing drawn (the page's own frame stays). */
export const Waiting = () => html`<div class="co-empty" style="padding:4px 2px">${tr('Reading…')}</div>`;

/** A part of the screen that failed to draw says so (the rest of the screen stays). */
export function Guard({ children }) {
  const [err] = useErrorBoundary();
  if (err) return html`<div class="co-empty t-danger" style="padding:8px;white-space:pre-wrap;font-size:9px">${String(err?.message || err)}\n${err?.stack || ''}</div>`;
  return children;
}

/** The size of an element as laid out (the stage's own units: its scale left out). */
export function useSize() {
  const [size, setSize] = useState({ w: 0, h: 0 });
  const ro = useRef(null);
  // (a ref given to whichever element is drawn: a page draws its measured one only later)
  const ref = useCallback((el) => {
    ro.current?.disconnect();
    ro.current = null;
    if (!el) return;
    const measure = () => setSize((s) => (s.w === el.clientWidth && s.h === el.clientHeight ? s : { w: el.clientWidth, h: el.clientHeight }));
    // (measured after the frame: a size taken inside the observer's own call loops it)
    ro.current = new ResizeObserver(() => requestAnimationFrame(measure));
    ro.current.observe(el);
    measure();
  }, []);
  useEffect(() => () => ro.current?.disconnect(), []);
  return [ref, size];
}

let measure = null;
/** How wide words are at a size of the game-drawn launcher (`Ui::width`: its px, drawn
 * smaller by 1.303 here). */
export function textW(text, px, weight = 700) {
  if (!measure) measure = document.createElement('canvas').getContext('2d');
  measure.font = `${weight} ${px / 1.303}px "Hanken Grotesk", sans-serif`;
  return measure.measureText(String(text)).width;
}

/** A slider over [min, max] by `step` (`Ui::slider`): its name on the left (none: the track
 * from the left edge), the value on the right. */
export function Slider({ value, min, max, step = 1, label, format, onChange, onRelease, style = "" }) {
  const v = Number(value ?? min);
  const fill = ((v - min) / (max - min || 1)) * 100;
  return html`<div class=${'co-slider' + (label ? '' : ' bare')} style=${style}>
    ${label && html`<span class="cut">${label}</span>`}
    <input type="range" min=${min} max=${max} step=${step} value=${v} style=${`--fill:${fill}%`} onInput=${(e) => onChange?.(Number(e.currentTarget.value))} onChange=${(e) => onRelease?.(Number(e.currentTarget.value))} />
    <b>${format ? format(v) : v}</b>
  </div>`;
}

/** The crumbs of a way down (`buspick::crumbs`): the parts on the way in the accent, a click
 * back to them; the last in ink. */
export function Crumbs({ parts, onPick, style = '' }) {
  return html`<div class="co-crumbs" style=${style}>${parts.map((p, k) =>
    k + 1 === parts.length
      ? html`<b class="cut">${p}</b>`
      : html`<button type="button" class="cut" onClick=${() => onPick(k)}>${p}</button><${Icon} name="chevron_right" size=${14} />`,
  )}</div>`;
}

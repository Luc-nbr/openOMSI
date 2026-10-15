// The company's start screen in Normal and Realistic, as our game-drawn launcher's
// (`company/home.rs`): every part of the company as a tile, grouped in five sections (at a
// glance, operations, people, business, the company) chosen above them, each tile saying how
// that part is doing - and under them the ways to what is often wanted.

import { html, setCo, setUi, ui, useView, useSize, textW, goTab, handOver, act, cmd, Icon, Seg, seen } from './kit.js';
import { ComingRows } from './agenda.js';

const TABS_H = 40;
const TABS_GAP = 22;
const GAP = 16;
const MAX_W = 1240;
const TABS_MAX_W = 960;
const MIN_TILE_H = 96;
const MAX_TILE_H = 250;
const MORE_H = 34;
const MORE_GAP = 8;
const MORE_TOP = 24;

const wayW = (label) => textW(label, 13) + 52;

/** The ways under the tiles in rows as wide as `w` (`ways_laid`). */
function waysLaid(w, ways) {
  let row = 0;
  let x = 0;
  const at = ways.map((way) => {
    const bw = wayW(way.label);
    if (x > 0 && x + bw > w) {
      row += 1;
      x = 0;
    }
    const p = [row, x];
    x += bw + MORE_GAP;
    return p;
  });
  return [at, ways.length ? row + 1 : 0];
}

/** The tiles laid out in the room there is (`home::grid`): two rows of tiles in two columns,
 * a big one taking both rows of its column. */
function grid(area, tiles, moreRows) {
  const w = Math.min(area.w, MAX_W);
  const x0 = area.x + (area.w - w) * 0.5;
  const moreH = moreRows === 0 ? 0 : MORE_TOP + moreRows * MORE_H + (moreRows - 1) * MORE_GAP;
  const slots = tiles.reduce((n, t) => n + (t.big ? 2 : 1), 0);
  const rows = slots <= 2 && tiles.every((t) => !t.big) ? 1 : 2;
  const fixed = TABS_H + TABS_GAP + (rows - 1) * GAP + moreH;
  const tileH = Math.floor(Math.max(MIN_TILE_H, Math.min(MAX_TILE_H, (area.h - fixed) / rows)));
  const height = fixed + rows * tileH;
  const top = area.y + Math.floor(Math.max(0, (area.h - height) * 0.42));
  const tw = Math.min(w, TABS_MAX_W);
  const tabs = { x: Math.floor(area.x + (area.w - tw) * 0.5), y: top, w: Math.floor(tw), h: TABS_H };
  // (two columns; more where the big tiles ask for them - at a glance: the overview, the agenda
  // and the inbox)
  const cols = Math.max(2, Math.ceil(slots / rows));
  const colW = Math.floor((w - GAP * (cols - 1)) / cols);
  const y0 = top + TABS_H + TABS_GAP;
  let col = 0;
  let row = 0;
  const placed = tiles.map((t) => {
    const n = Math.min(t.big ? 2 : 1, rows);
    if (row + n > rows) {
      col += 1;
      row = 0;
    }
    const r = { x: Math.floor(x0 + col * (colW + GAP)), y: y0 + row * (tileH + GAP), w: colW, h: tileH * n + GAP * (n - 1) };
    row += n;
    return r;
  });
  const more = { x: x0, y: top + height - moreH + (moreRows === 0 ? 0 : MORE_TOP), w, h: Math.max(0, moreH - MORE_TOP) };
  return { tabs, tiles: placed, more, height };
}

/** Go where a tile or a way under them leads (`home::open`). */
export function openTo(to) {
  if (to.startsWith('tab:')) return goTab(Number(to.slice(4)));
  switch (to) {
    case 'hire':
      setUi('people', { tab: 1, hire_role: 0 });
      return goTab(2);
    case 'courses':
      setUi('people', { tab: 2, group: 2, first: 0 });
      return goTab(2);
    case 'licences':
      setUi('people', { tab: 2, group: 1, first: 0 });
      return goTab(2);
    case 'settings':
      return setCo({ dialog: { kind: 'settings' } });
    case 'dealer':
      setUi('fleet', { tab: 1 });
      return goTab(1);
    case 'adverts':
      setUi('fleet', { tab: 2 });
      return goTab(1);
    case 'loan':
      goTab(4);
      return act('open_loan');
    case 'new_line':
      return handOver('line-editor');
    case 'my_duties':
      setUi('career', { part: 0 });
      return goTab(6);
    case 'rankings':
      setUi('career', { part: 4 });
      return goTab(6);
    case 'statistics':
      setUi('career', { part: 5 });
      return goTab(6);
    case 'report':
      return act('show_report');
    case 'chooser':
      // (the clock stops: `chooser::to_companies`)
      return cmd('co_act', { act: 'play', speed: 0 }).finally(() => setCo({ chooser: 'companies' }));
    default:
  }
  return null;
}

export function HomePage() {
  const v = useView('home');
  const [ref, size] = useSize();
  const st = ui('home', { cat: 0 });
  let body = null;
  if (v?.sections && size.w > 0) {
    const cat = Math.min(st.cat, v.sections.length - 1);
    const tiles = v.sections[cat].tiles;
    const [, rows] = waysLaid(Math.min(size.w, MAX_W), v.ways);
    let g = grid({ x: 0, y: 0, w: size.w, h: size.h }, tiles, rows);
    const fits = g.height <= size.h + 1;
    if (!fits) g = grid({ x: 0, y: 0, w: size.w - 10, h: 0 }, tiles, rows);
    const [at] = waysLaid(g.more.w, v.ways);
    body = html`<div style=${`position:relative;height:${fits ? size.h : g.height + 8}px`}>
      <div style=${`position:absolute;left:${g.tabs.x}px;top:${g.tabs.y}px;width:${g.tabs.w}px`}>
        <${Seg} value=${cat} options=${v.sections.map((s, k) => [k, s.dot ? `${s.title}  •` : s.title])} onChange=${(k) => setUi('home', { cat: k })} style="height:40px" />
      </div>
      ${tiles.map((t, k) => html`<${Tile} t=${t} r=${g.tiles[k]} onOpen=${() => openTo(t.to)} />`)}
      ${v.ways.map((w, k) => html`<button type="button" class="co-way" style=${`left:${g.more.x + at[k][1]}px;top:${g.more.y + at[k][0] * (MORE_H + MORE_GAP)}px;width:${wayW(w.label)}px`} onClick=${() => openTo(w.to)}>
        <${Icon} name=${w.icon} size=${16} /><span>${w.label}</span></button>`)}
    </div>`;
  }
  return html`<div class="co-scroll co-home" ref=${ref}>${body}</div>`;
}

/** The tile's sizes for its height (`home::Metrics`). */
function metrics(r) {
  const narrow = r.w < 190;
  if (r.h >= 130) return { pad: 18, mark: 40, name: narrow ? 15.5 : 18, gap: 14, line: 22, roomy: true };
  if (r.h >= 100) return { pad: 14, mark: 32, name: narrow ? 15 : 16.5, gap: 8, line: 19.5, roomy: false };
  return { pad: 12, mark: 28, name: narrow ? 14.5 : 16, gap: 6, line: 19, roomy: false };
}

/** A tile (`home::tile`): its mark and name (a badge where something waits), its lines, and
 * what it has room for under them - the month's days, the newest messages, the staff's roles. */
export function Tile({ t, r, onOpen }) {
  const k = metrics(r);
  const bottom = r.h - k.pad * 0.7;
  let y = k.pad + k.mark + k.gap;
  const parts = [];
  if (t.extra?.kind === 'money') {
    const px = k.roomy ? 30 : 24;
    if (y + px + 6 <= bottom) {
      parts.push(html`<div class=${'cash t-' + t.extra.tone} style=${`top:${y}px;height:${px + 6}px;line-height:${px + 6}px;font-size:${px / 1.303}px`}>${t.extra.cash}</div>`);
      y += px + 10;
    }
  }
  t.lines.forEach(([text, tone], i) => {
    if (y + k.line > bottom + 2) return;
    const px = i === 0 ? (k.roomy ? 15 : 14) : k.roomy ? 14 : 13;
    parts.push(html`<div class=${'line t-' + tone} style=${`top:${y}px;height:${k.line}px;line-height:${k.line}px;font-size:${px / 1.303}px;font-weight:${i === 0 ? 500 : 400}`}>${text}</div>`);
    y += k.line;
  });
  const rest = { y: y + 8, h: Math.max(0, bottom - y - 8), w: r.w - 2 * k.pad };
  const badgeW = t.badge ? Math.max(24, textW(t.badge.n, 12) + 14) : 0;
  return html`<button type="button" class="co-tile co-hometile" data-to=${t.to} title=${t.tip || undefined} style=${`left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px;--pad:${k.pad}px`} onClick=${onOpen}>
    <span class="mark" style=${`left:${k.pad}px;top:${k.pad}px;width:${k.mark}px;height:${k.mark}px`}><${Icon} name=${t.icon} size=${k.mark * 0.55} /></span>
    <span class="name" style=${`left:${k.pad + k.mark + 12}px;top:${k.pad}px;height:${k.mark}px;line-height:${k.mark}px;font-size:${k.name / 1.303}px;right:${k.pad + (badgeW ? badgeW + 8 : 0)}px`}>${t.name}</span>
    ${t.badge && html`<span class=${'badge ' + t.badge.tone} style=${`right:${k.pad}px;top:${k.pad + (k.mark - 24) / 2}px;min-width:${badgeW}px`}>${t.badge.n}</span>`}
    ${parts}
    ${rest.h > 0 && html`<div class="rest" style=${`top:${rest.y}px;height:${rest.h}px`}><${Extra} e=${t.extra} r=${rest} /></div>`}
  </button>`;
}

function Extra({ e, r }) {
  if (!e) return null;
  if (e.kind === 'money') return html`<${Bars} days=${e.days} none=${e.none} r=${r} />`;
  if (e.kind === 'messages') return html`<${Messages} list=${e.list} none=${e.none} r=${r} />`;
  if (e.kind === 'roles') return html`<${Roles} roles=${e.list} r=${r} />`;
  if (e.kind === 'agenda') return r.h >= 20 ? html`<${ComingRows} list=${e.list} n=${Math.max(0, Math.floor((r.h + 2) / 24))} none=${e.none} />` : null;
  return null;
}

/** The last thirty days' results as bars round the zero line (`home::bars`). */
function Bars({ days, none, r }) {
  if (r.h < 26 || !days?.length) return null;
  const h = Math.min(r.h, 140);
  if (days.every((d) => d == null)) return html`<div class="co-note t-dim" style="line-height:20px">${none}</div>`;
  const vals = days.filter((d) => d != null);
  const hi = Math.max(0, ...vals);
  const lo = Math.min(0, ...vals);
  const span = Math.max(1, hi - lo);
  const zero = (h * hi) / span;
  const slot = r.w / days.length;
  const bw = Math.max(1.5, slot * 0.62);
  return html`<svg width=${r.w} height=${h} style="display:block">
    <rect x="0" y=${zero} width=${r.w} height="1" fill="var(--hub-hairline)" />
    ${days.map((d, k) => {
      if (d == null) return null;
      const bh = Math.max(1.5, (h * Math.abs(d)) / span);
      const x = k * slot + (slot - bw) / 2;
      return html`<rect x=${x} y=${d >= 0 ? zero - bh : zero} width=${bw} height=${bh} rx=${Math.min(1.5, bw * 0.4)} fill=${d >= 0 ? `rgba(52,178,110,${seen(0.85, [52, 178, 110])})` : `rgba(217,58,48,${seen(0.85, [217, 58, 48])})`} />`;
    })}
  </svg>`;
}

/** The newest messages, the unread ones with the accent's dot (`home::messages`). */
function Messages({ list, none, r }) {
  if (r.h < 20) return null;
  if (!list.length) return html`<div class="co-note t-dim" style="line-height:20px">${none}</div>`;
  const n = Math.max(0, Math.floor((r.h + 2) / 24));
  return html`${list.slice(0, n).map(([title, date, unread]) => html`<div class=${'msg' + (unread ? ' unread' : '')}><i></i><span class="cut grow">${title}</span><span class="date">${date}</span></div>`)}`;
}

/** The staff by role, in two columns where there is room (`home::role_rows`). */
function Roles({ roles, r }) {
  if (r.h < 20 || !roles.length) return null;
  const cols = r.w >= 300 && roles.length > 3 ? 2 : 1;
  const per = Math.ceil(roles.length / cols);
  const fit = Math.max(0, Math.floor((r.h + 2) / 22));
  return html`<div class="roles" style=${`grid-template-columns:repeat(${cols},minmax(0,1fr));grid-template-rows:repeat(${per},22px)`}>
    ${roles.map(([label, n], k) => (k % per < fit ? html`<div class="role" style=${`grid-column:${Math.floor(k / per) + 1};grid-row:${(k % per) + 1}`}><span class="cut grow">${label}</span><b class=${n > 0 ? '' : 't-dim'}>${n}</b></div>` : null))}
  </div>`;
}

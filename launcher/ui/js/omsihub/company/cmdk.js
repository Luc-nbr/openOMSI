// The company's command palette (Ctrl+K, or the search in its top bar): every page of the
// company, its main actions and the places last visited, found by a few letters of their names
// and run with Enter. What a game mode or a player's role cannot use is not offered: the pages
// are the sidebar's (`shell.rail`, already as the role allows), the actions only the boss's.

import { useRef, useState } from '../../../vendor/preact-htm.js';
import { S, remembered, remember } from '../../store.js';
import { html, useEffect, tr, HOME, setCo, act, goTab, Icon } from './kit.js';
import { openTo } from './home.js';
import { TOGETHER_TAB } from './together.js';

/** The places last visited, per company (the launcher's own store). */
const recentKey = (id) => 'co-recent:' + (id || '');
export function noteVisit(id, tab) {
  if (tab == null || tab === HOME) return;
  let list = [];
  try {
    list = JSON.parse(remembered(recentKey(id), '[]'));
  } catch {
    list = [];
  }
  if (!Array.isArray(list)) list = [];
  remember(recentKey(id), JSON.stringify([tab, ...list.filter((t) => t !== tab)].slice(0, 5)));
}
function recentOf(id) {
  try {
    const v = JSON.parse(remembered(recentKey(id), '[]'));
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

/** How well `q` fits `text`: its letters in order (a word's start and letters together count
 * more), or -1. */
function score(q, text) {
  if (!q) return 0;
  const t = text.toLowerCase();
  const at = t.indexOf(q);
  if (at >= 0) return 1000 - at + (at === 0 || t[at - 1] === ' ' ? 200 : 0);
  let k = 0;
  let s = 0;
  let last = -2;
  for (let i = 0; i < t.length && k < q.length; i++) {
    if (t[i] !== q[k]) continue;
    s += i === last + 1 ? 6 : i === 0 || t[i - 1] === ' ' ? 4 : 1;
    last = i;
    k++;
  }
  return k === q.length ? s : -1;
}

/** Everything the palette offers, as `{ kind, icon, title, desc, keys, run }`. */
function entries({ toggleSide, setNav, askTour }) {
  const c = S.co;
  const sh = c.shell;
  const remote = !!c.remote;
  const casual = !!sh.casual;
  const items = sh.rail.flat();
  const name = (it) => (it.tab === 0 && !casual ? tr('Dashboard') : it.name);
  const pages = items.map((it) => ({ kind: 'page', tab: it.tab, icon: it.icon, title: name(it), desc: it.group || '', run: () => goTab(it.tab) }));
  const homeOk = !remote || (sh.remote?.perms || []).includes('finances');
  if (homeOk) pages.unshift({ kind: 'page', tab: HOME, icon: casual ? 'home' : 'apps', title: tr(casual ? 'Home' : 'Start screen'), desc: '', keys: 'Esc', run: () => setCo({ tab: HOME }) });
  const may = (tab) => items.some((x) => x.tab === tab);
  const page = (tab) => tr(sh.names?.[tab] || '');
  const actions = [];
  const a = (icon, title, desc, run, keys) => actions.push({ kind: 'action', icon, title, desc, run, keys });
  if (!remote && !casual) {
    a('nights_stay', tr('Simulate to tomorrow'), tr('Clock'), () => act('tomorrow'));
    a('schedule', tr('+1 h'), tr('Clock'), () => act('hour'));
    if (may(1)) a('airport_shuttle', tr('Dealer'), page(1), () => openTo('dealer'));
    if (may(2)) a('person', tr('Hire drivers'), page(2), () => openTo('hire'));
    if (may(2)) a('key', tr('Train drivers'), page(2), () => openTo('licences'));
    if (may(3)) a('alt_route', tr('Make a new line'), page(3), () => openTo('new_line'));
    if (may(5)) a('calendar_month', tr('RosterFlow'), page(5), () => import('../rosterflow/index.js').then((m) => m.openRosterFlow({})).then((on) => on || goTab(5)));
    if (may(15)) a('traffic', tr('Operations'), page(15), () => goTab(15));
    if (may(1)) a('campaign', tr('Advertising'), page(1), () => openTo('adverts'));
    if (may(4)) a('receipt_long', tr('Take a loan'), page(4), () => openTo('loan'));
    if (may(6)) a('schedule', tr('My duties'), page(6), () => openTo('my_duties'));
    if (may(6)) a('stars', tr('Rankings'), page(6), () => openTo('rankings'));
    if (may(6)) a('trending_up', tr('Statistics'), page(6), () => openTo('statistics'));
    a('wifi_tethering', tr('Multiplayer'), '', () => (sh.pages?.includes(TOGETHER_TAB) ? goTab(TOGETHER_TAB) : setCo({ dialog: { kind: 'together' } })));
    a('sync_alt', tr('Other company'), '', () => openTo('chooser'));
  }
  if (!remote && casual) a('wb_sunny', tr('Next day'), tr('Clock'), () => act('next_day'));
  if (!remote || (sh.remote?.perms || []).includes('settings')) a('settings', tr('Company settings'), '', () => setCo({ dialog: { kind: 'settings' } }));
  a('menu', tr('Collapse the sidebar'), '', () => setNav((c.nav || 'full') === 'rail' ? 'full' : 'rail'));
  a('close', tr('Hide the sidebar (Ctrl+B)'), '', toggleSide, 'Ctrl+B');
  if (!remote) a('help', tr('Help'), tr('A tour of the bus company'), askTour);
  const recent = recentOf(sh.id)
    .filter((t) => t !== c.tab)
    .map((t) => pages.find((p) => p.tab === t))
    .filter(Boolean)
    .map((p) => ({ ...p, kind: 'recent', icon: 'history' }));
  return { pages, actions, recent };
}

/** The palette over the company's pages. */
export function CommandPalette(props) {
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(0);
  const input = useRef();
  const list = useRef();
  useEffect(() => input.current?.focus(), []);
  const close = () => setCo({ cmdk: false });
  const all = entries(props);
  const needle = q.trim().toLowerCase();
  const pick = (xs) =>
    xs
      .map((x) => ({ x, s: Math.max(score(needle, x.title), score(needle, `${x.title} ${x.desc}`) - 50) }))
      .filter((r) => r.s >= 0)
      .sort((p, r) => (needle ? r.s - p.s : 0))
      .map((r) => r.x);
  const groups = [
    [tr('Recent'), needle ? [] : all.recent.slice(0, 4)],
    [tr('Pages'), pick(all.pages)],
    [tr('Actions'), pick(all.actions)],
  ].filter(([, xs]) => xs.length);
  const flat = groups.flatMap(([, xs]) => xs);
  const k = Math.min(sel, Math.max(0, flat.length - 1));
  const run = (x) => {
    close();
    if (x) x.run();
  };
  useEffect(() => list.current?.querySelector('.co-cmdk-row.on')?.scrollIntoView({ block: 'nearest' }), [k, needle]);
  const key = (e) => {
    if (e.key === 'ArrowDown') (e.preventDefault(), setSel((k + 1) % Math.max(1, flat.length)));
    else if (e.key === 'ArrowUp') (e.preventDefault(), setSel((k - 1 + flat.length) % Math.max(1, flat.length)));
    else if (e.key === 'Enter') (e.preventDefault(), run(flat[k]));
    else if (e.key === 'Escape') (e.preventDefault(), e.stopPropagation(), close());
  };
  let n = -1;
  return html`<div class="co-cmdk-scrim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="co-cmdk" role="dialog">
      <div class="co-cmdk-field"><${Icon} name="search" size=${20} />
        <input ref=${input} value=${q} placeholder=${tr('Search pages and actions (Ctrl+K)')} onInput=${(e) => (setQ(e.target.value), setSel(0))} onKeyDown=${key} />
        <span class="co-cmdk-key">Esc</span></div>
      <div class="co-cmdk-list" ref=${list}>
        ${flat.length === 0 && html`<div class="co-cmdk-none">${tr('No match')}</div>`}
        ${groups.map(([head, xs]) => html`<div class="co-cmdk-head">${head}</div>
          ${xs.map((x) => {
            n += 1;
            const i = n;
            return html`<button type="button" class=${'co-cmdk-row' + (i === k ? ' on' : '')} onMouseMove=${() => i !== k && setSel(i)} onClick=${() => run(x)}>
              <span class="co-cmdk-icon"><${Icon} name=${x.icon} size=${18} /></span>
              <span class="co-cmdk-words cut"><b>${x.title}</b>${x.desc && html`<span>${x.desc}</span>`}</span>
              ${(x.keys || i === k) && html`<span class="co-cmdk-key">${x.keys || '↵'}</span>`}
            </button>`;
          })}`)}
      </div>
    </div>
  </div>`;
}

// The bus's depot files on the bus step (`crates/omsi-launcher-ui/src/hof.rs`): which of them
// the bus has and what each is for - the map it belongs to, how many destinations it knows,
// whether it is the chosen map's - with the one in use marked and the one for the chosen map
// first, and a tile that gives the bus the map's depot file when it lacks it. A bus without
// the chosen map's depot file is asked about once: a dialog over the step says so, offers the
// bus's own files that know the map best, and the map's file to add.
//
// The analysis is Rust's (`hub_depots`, `hub_depot_add` in `omsihub/editors/hof.rs`); here are
// the field on the bus's sheet (`DepotField`), the tiles in the wide sheet (`DepotTiles`) and
// the dialog (`DepotDialog`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { choose, curMap, curBus } from '../../data.js';
import { Icon, tr, keep, status, mapLabel, Btn } from './kit.js';

/** What the depot files keep: the menu of the bus, map and depot file asked for, the dialog,
 * the dialogs answered (map|bus|depot file: not asked again this session), what adding came to. */
const V = keep('hof', { menu: null, asked: '', dialog: null, answered: new Set(), note: null, busy: false });

const redraw = () => set({});

/** The line driven (`State::driven_line`): a free drive's own line, else the duty's. */
function drivenLine() {
  const c = S.choice;
  if (c.free && c.own_line && c.free_line && c.free_route) return c.free_line;
  return c.free ? null : c.line;
}

/** Ask for the menu of the bus, map and depot file chosen now (again when one of them, or the
 * content, changed): kept until the next one is there, so the tiles do not blink away. */
function ask() {
  const bus = curBus();
  const map = curMap();
  if (!bus || !map) return;
  const c = S.choice;
  const key = `${bus.file}|${map.file}|${c.date}|${c.hof}|${c.hof_manual}|${S.hof}|${drivenLine()}|${S.poll.stamp}|${V.added || 0}`;
  if (V.asked === key) return;
  V.asked = key;
  const maps = (S.maps || []).map((m) => [mapLabel(m), m.hof || '']);
  call('hub_depots', { bus: bus.file, map: { file: map.file, name: map.name, friendly: map.friendly, hof: map.hof || '' }, date: c.date, chosen: c.hof_manual ? c.hof : S.hof, line: drivenLine(), maps })
    .then((m) => {
      if (V.asked !== key) return;
      V.menu = m;
      frame();
      redraw();
    })
    .catch((e) => status(e.message, true));
}

/** The menu worked out for the bus and map chosen now (one of before while it is worked out
 * again). */
function current() {
  const bus = curBus();
  const map = curMap();
  const m = V.menu;
  return m && bus && map && m.bus === bus.file && m.key.split('|')[1] === map.file ? m : null;
}

/** Once the menu is there: the dialog for a bus that lacks the map's depot file, asked once. */
function frame() {
  const m = current();
  if (!m || S.page !== 'drive' || S.hub.step !== 'bus' || S.boot?.selftest) return;
  const about = `${S.choice.map}|${m.bus}|${m.want.trim().toLowerCase()}`;
  const lacks = m.lacks && !m.own_in_use;
  if (!V.dialog && lacks && !V.answered.has(about)) V.dialog = { mode: 'missing', at: Date.now(), about };
  else if (V.dialog && (V.dialog.about !== about || (V.dialog.mode === 'missing' && !m.lacks))) V.dialog = null;
}

/** The depot files, asked for whenever what they depend on changed. */
export function useDepots() {
  const s = useStore();
  useEffect(ask);
  return current();
}

// --- words ----------------------------------------------------------------------------------

const countText = (n) => (n === 1 ? tr('one destination') : tr('%{n} destinations', { n }));

function refusalText(r) {
  switch (r?.kind) {
    case 'has': return tr('This bus has it already: %{file}', { file: r.what });
    case 'taken': return tr('A file called %{file} lies beside this bus already, and is never written over', { file: r.what });
    case 'no_source': return tr('No copy of it on this computer');
    case 'no_folder': return tr('openOMSI has no folder of its own to put it in');
    case 'original': return tr('That would be in the OMSI 2 folder, which openOMSI does not write to');
    case 'unreadable': return tr('The copy found cannot be read: %{why}', { why: r.what });
    default: return tr('Writing it failed: %{why}', { why: r?.what || '' });
  }
}

/** The words of a fit, and their colour (`fit_text`). */
function fitText(fit, destinations) {
  switch (fit.kind) {
    case 'maps': return [tr("This map's depot file"), 'ok'];
    case 'fits': return [tr('Fits this map'), 'ok'];
    case 'partly': return [tr('Knows %{known} of %{total} destinations here', { known: fit.known, total: fit.total }), fit.known * 4 >= fit.total ? 'warn' : 'dim'];
    case 'other': return [tr("None of this map's destinations"), 'dim'];
    default: return [countText(destinations), 'dim'];
  }
}

/** Which file the drive takes with the depot file `chosen`, worked out by Rust for the depot
 * file chosen now (`Menu::used`). */
const used = (m) => m.used;

/** Make `row` the depot file of the drive: automatic when it is the one openOMSI would take
 * anyway, the player's own choice otherwise. */
function select(row) {
  const auto = S.hof || '';
  const isAuto = row.key.trim().toLowerCase() === auto.trim().toLowerCase() || row.name.trim().toLowerCase() === auto.trim().toLowerCase();
  choose({ hof_manual: !isAuto, hof: isAuto ? auto : row.key });
}

/** Make the player's own depot file `key` the drive's: given to the bus, whichever it is. */
const selectOwn = (key) => choose({ hof_manual: true, hof: key });

/** What adding came to: on the status line, and over the tiles for a while. */
function note(text, err) {
  status(text, err);
  V.note = { text, err, at: Date.now() };
}

/** Give the bus the map's depot file. Returns whether it was added. */
async function add(m) {
  if (!m.source) return false;
  const bus = curBus();
  const name = bus?.name || '';
  const r = await call('hub_depot_add', { bus: m.bus, want: m.want, source: m.source.path }).catch((e) => ({ refusal: { kind: 'failed', what: e.message } }));
  if (r.added) {
    // (every version of the pack that reads the same folder has it at once)
    for (const v of S.vehicles || []) {
      if (bus && v.folder?.toLowerCase() === bus.folder?.toLowerCase() && JSON.stringify(v.hofs) === JSON.stringify(bus.hofs) && !v.hofs.some((h) => h.toLowerCase() === m.source_name.toLowerCase())) v.hofs = [...v.hofs, m.source_name];
    }
    V.added = (V.added || 0) + 1;
    const hof = await call('default_hof', { map: curMap() || null, bus: curBus() || null, date: S.choice.date, line: S.choice.free ? null : S.choice.line }).catch(() => S.hof);
    set({ hof });
    choose({ hof_manual: false, hof });
    note(tr('%{file} added beside %{bus}', { file: m.source.file, bus: name }), false);
    return true;
  }
  note(tr('Not added: %{why}', { why: refusalText(r.refusal) }), true);
  return false;
}

// --- the field on the bus's sheet -----------------------------------------------------------

/** What the drive takes, in words for a field (`standing`): its name, how it stands to the
 * map, the colour of those words, an icon. */
function standing(m) {
  const chosen = S.choice.hof_manual ? S.choice.hof : S.hof;
  if (!m) return [chosen, tr('Reading the depot files…'), 'dim', 'schedule'];
  if (m.own_in_use) return [m.own[m.own_in_use.index].name, tr('Your depot file: given to this bus when you drive'), 'ok', 'check_circle'];
  const u = used(m);
  if (u.row != null) {
    const r = m.rows[u.row];
    const [text, c] = fitText(r.fit, r.destinations);
    const [col, icon] = r.fit.kind === 'maps' || r.fit.kind === 'fits' ? ['ok', 'check_circle'] : r.fit.kind === 'partly' || r.fit.kind === 'other' ? ['warn', 'warning'] : [c, 'info'];
    return [r.name, text, col, icon];
  }
  if (u.borrowed) return [m.want, tr('Borrowed from %{folder} for this drive', { folder: u.borrowed }), 'warn', 'warning'];
  return [tr('None'), tr('This bus has no depot file'), 'warn', 'warning'];
}

/** The depot file the drive takes, as a field that opens the depot files, and under it how it
 * stands to the map (`hof::field`). */
export function DepotField({ onOpen }) {
  const m = useDepots();
  const [name, text, c, icon] = standing(m);
  return html`<span class="ed-label">${tr('Depot file')}</span>
    <div class="ed-hof-field">
      <button type="button" class="face" title=${tr('The depot files of this bus: which one the drive takes, and adding the map\'s')} onClick=${onOpen}>
        <${Icon} name="garage" size=${16} /><span class="text">${name}</span><${Icon} name="chevron_right" size=${16} />
      </button>
      <span class=${'fit ' + c}><${Icon} name=${icon} size=${13} />${text}</span>
    </div>`;
}

// --- the tiles ------------------------------------------------------------------------------

/** One tile (`draw_card`). */
function Card({ c, onPick }) {
  return html`<button type="button" class=${'ed-hof-card' + (c.chosen ? ' chosen' : '') + (c.add ? ' add' : '') + (c.enabled ? '' : ' off')} disabled=${!c.enabled} onClick=${onPick}>
    <span class="ib"><${Icon} name=${c.icon} size=${22} /></span>
    <span class="head"><b>${c.title}</b>${c.chosen && html`<span class="inuse"><${Icon} name="check" size=${14} />${tr('In use')}</span>`}</span>
    <span class="file">${c.file}</span>
    <span class="line">${c.line}</span>
    <span class="tags">${c.tag && html`<span class=${'tag ' + c.tag[1]}>${c.tag[0]}</span>`}${c.extra && html`<span class="extra">${c.extra}</span>`}</span>
  </button>`;
}

/** The tiles of the menu (`cards_of`): the player's own first, the map's borrowed one, the
 * bus's own, and the one that adds the map's. */
function cardsOf(m) {
  const u = used(m);
  const auto = !S.choice.hof_manual;
  const own = m.own_in_use;
  const out = [];
  m.own.forEach((e, k) => {
    const chosen = own?.index === k;
    const byLine = chosen && own.by_line;
    const specials = e.specials.slice(0, 4).map(([c, n]) => `${c} ${n}`);
    const more = e.specials.length > 4 ? ' …' : '';
    out.push([{ key: 'own-' + e.key.toLowerCase(), title: tr("Your depot file '%{name}'", { name: e.name }), file: `${e.key}.hof`, line: `${chosen ? tr('Given to this bus when you drive') : tr('Given to this bus when chosen')} · ${countText(e.termini)}`, tag: [byLine ? tr('The line chose it') : tr('Yours'), 'accent'], extra: specials.length ? tr('Special trips: %{list}', { list: specials.join(', ') + more }) : null, chosen, icon: 'departure_board', enabled: true }, () => selectOwn(e.key)]);
  });
  if (u.borrowed) {
    out.push([{ key: 'borrowed', title: m.want, file: tr('from %{folder}', { folder: u.borrowed }), line: m.total != null ? `${m.map} · ${countText(m.total)}` : m.map, tag: [tr('Borrowed for this drive'), 'warn'], chosen: !own, icon: 'garage', enabled: !!m.source && !m.refusal }, () => openAdd(m)]);
  }
  m.rows.forEach((r, i) => {
    const [tag, col] = fitText(r.fit, r.destinations);
    const line = r.map ? `${r.map} · ${countText(r.destinations)}` : countText(r.destinations);
    const chosen = !own && u.row === i;
    const extra = [];
    if (m.recommended === i && !r.maps) extra.push(tr('best for this map'));
    if (chosen && auto) extra.push(tr('automatic'));
    if (r.added) extra.push(tr('added by openOMSI'));
    out.push([{ key: r.file.toLowerCase(), title: r.name, file: r.file, line, tag: r.fit.kind !== 'unknown' ? [tag, col] : null, extra: extra.join(' · ') || null, chosen, icon: 'garage', enabled: true }, () => select(r)]);
  });
  if (m.want.trim()) {
    const s = m.source;
    let card;
    if (s && !m.refusal) card = { file: s.from_map ? `${s.file} · ${tr("from the map's folder")}` : `${s.file} · ${tr('from %{folder}', { folder: s.folder })}`, line: `${m.want} · ${countText(m.source_destinations)}`, enabled: true };
    else if (m.refusal?.kind === 'has') card = { file: m.refusal.what, line: tr('This bus has the depot file of this map'), enabled: false, tag: [tr('Nothing to add'), 'dim'] };
    else card = { file: m.want, line: refusalText(m.refusal || { kind: 'no_source' }), enabled: false };
    out.push([{ key: 'add', title: tr("Add the map's depot file"), chosen: false, icon: 'add', add: true, tag: null, ...card }, card.enabled ? () => openAdd(m) : null]);
  }
  return out;
}

function openAdd(m) {
  if (!m.source || m.refusal) return;
  V.dialog = { mode: 'add', at: Date.now(), about: `${S.choice.map}|${m.bus}|${m.want.trim().toLowerCase()}` };
  redraw();
}

/** The sheet's title and the line under it, on the depot files (`hof::heading`). */
export const depotHeading = () => [tr('Depot file'), tr('The depot file decides which destinations the IBIS and the displays know.')];

/** Over the tiles on the right, the map's depot file (`hof::map_line`). */
export function mapLine() {
  const m = current();
  if (!m) return '';
  return m.want.trim() ? tr('The depot file of %{map}: %{hof}', { map: m.map, hof: m.want }) : tr('%{map} names no depot file of its own', { map: m.map });
}

/** The sheet's foot on the depot files: how many, and which one the drive takes. */
export function depotFoot() {
  const m = current();
  if (!m) return tr('Reading the depot files…');
  const n = m.rows.length === 1 ? tr('one depot file beside this bus') : tr('%{n} depot files beside this bus', { n: m.rows.length });
  if (m.own_in_use) return `${n} · ${tr('In use: %{hof}', { hof: m.own[m.own_in_use.index].name })} (${m.own_in_use.by_line ? tr("your line's choice") : tr('your choice')})`;
  const u = used(m);
  const inUse = u.row != null ? m.rows[u.row].name : u.borrowed ? m.want : null;
  if (!inUse) return n;
  return `${n} · ${tr('In use: %{hof}', { hof: inUse })} (${S.choice.hof_manual ? tr('your choice') : tr('automatic')})`;
}

/** The depot files as tiles (`hof::cards`): over them a word when the bus lacks the map's file
 * (or what adding came to), the tiles - the one for the map first, the one in use marked, the
 * one that adds last. */
export function DepotTiles() {
  const m = useDepots();
  if (!m) return html`<p class="ed-hof-reading">${tr('Reading the depot files…')}</p>`;
  const own = m.own_in_use;
  const note = V.note && Date.now() - V.note.at < 8000 ? V.note : null;
  let strip = null;
  if (note) strip = html`<div class=${'ed-hof-strip ' + (note.err ? 'warn' : 'ok')}><${Icon} name=${note.err ? 'warning' : 'check_circle'} size=${17} /><span>${note.text}</span></div>`;
  else if (own) {
    const says = own.by_line ? 'Your line chose your depot file %{name}: it is given to this bus when you drive.' : 'Your depot file %{name} is given to this bus when you drive.';
    strip = html`<div class="ed-hof-strip ok"><${Icon} name="check_circle" size=${17} /><span>${tr(says, { name: m.own[own.index].name })}</span></div>`;
  } else if (m.lacks) {
    const can = m.source && !m.refusal;
    strip = html`<div class="ed-hof-strip warn"><${Icon} name="warning" size=${17} /><span>${tr('This bus does not carry %{hof}, the depot file of this map.', { hof: m.want })}</span>
      ${can && html`<${Btn} kind="primary" icon="add" label="Add it" onClick=${() => openAdd(m)} />`}</div>`;
  }
  const list = cardsOf(m);
  return html`<div class="ed-hof">
    ${strip}
    ${!m.rows.length && html`<p class="ed-hof-none">${tr('This bus has no depot files of its own.')}</p>`}
    <div class="ed-hof-grid">${list.map(([c, pick]) => html`<${Card} key=${c.key} c=${c} onPick=${() => pick && (pick(), redraw())} />`)}</div>
  </div>`;
}

// --- the dialog -----------------------------------------------------------------------------

/** The words of the dialog (`ask_of`) and the rows it offers instead. */
function askOf(m, mode, bus) {
  const [title, body] = mode === 'missing'
    ? [tr('This bus does not know this map'), tr("%{bus} does not carry %{hof}, the depot file of %{map}. A bus's IBIS and destination displays know only the codes and destinations of the depot files beside it.", { bus, hof: m.want, map: m.map })]
    : [tr("Add the map's depot file"), tr('%{hof} is the depot file of %{map}: with it beside the bus, its IBIS and destination displays know the map\'s codes and destinations.', { hof: m.want, map: m.map })];
  const u = used(m);
  let now;
  if (u.borrowed) now = tr("For this drive openOMSI borrows it from %{folder}, which carries it. Beside this bus it is the bus's own, in its list of depot files.", { folder: u.borrowed });
  else if (u.row != null) {
    const r = m.rows[u.row];
    now = r.known != null && m.total != null ? tr('Without it the bus takes %{file}, which knows %{known} of the %{total} destinations here.', { file: r.name, known: r.known, total: m.total }) : tr('Without it the bus takes %{file}.', { file: r.name });
  } else now = tr("Without it the IBIS takes none of the map's codes, and the destination displays stay blank.");
  let offer;
  let warn = true;
  let addLabel = null;
  if (m.source && !m.refusal) {
    const text = m.source.from_map ? 'Add %{file} beside this bus? It came with the map and knows %{n} destinations.' : 'Add %{file} beside this bus? It is the copy %{folder} carries, and knows %{n} destinations.';
    offer = tr(text, { file: m.source.file, folder: m.source.folder, n: m.source_destinations });
    warn = false;
    addLabel = tr("Add the map's depot file");
  } else if (!m.refusal || m.refusal.kind === 'no_source') offer = tr('No copy of %{hof} was found on this computer: no other bus carries it, and the map\'s folder has none.', { hof: m.want });
  else offer = refusalText(m.refusal);
  const picks = mode === 'missing' ? m.choices : [];
  const choices = picks.map((i) => [m.rows[i].name, fitText(m.rows[i].fit, m.rows[i].destinations)[0]]);
  const noteText = addLabel ? tr("The copy goes into openOMSI's own folder, beside this bus. Nothing in OMSI 2 is changed or written over.") : null;
  return { title, body, now, offer, warn, choices, picks, note: noteText, place: addLabel ? m.place : null, add: addLabel };
}

/** The dialog over the step (`hof::dialog`); `onOpen` shows the depot files. */
export function DepotDialog({ onOpen }) {
  const m = useDepots();
  frame();
  const d = V.dialog;
  const shown = !!(d && m && S.page === 'drive' && S.hub.step === 'bus');
  const a = shown ? askOf(m, d.mode, curBus()?.name || '') : null;
  useEffect(() => {
    if (!shown) return;
    const k = (e) => {
      if (e.key === 'Escape') answer('later');
      else if (e.key === 'Enter') answer(a.add ? 'add' : 'open');
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });
  if (!shown) return null;
  const done = () => {
    V.answered.add(d.about);
    V.dialog = null;
    redraw();
  };
  async function answer(what, k) {
    if (what === 'later') done();
    else if (what === 'choose') {
      done();
      const row = m.rows[a.picks[k]];
      if (row) select(row);
    } else if (what === 'add') {
      if (await add(m)) done();
      else redraw();
    } else {
      done();
      onOpen?.();
    }
  }
  return html`<div class="ed-dialog-scrim" onMouseDown=${(e) => e.target === e.currentTarget && answer('later')}>
    <div class="ed-dialog ed-hof-ask">
      <i class="glow"></i>
      <h2>${a.title}</h2>
      <p class="body">${a.body}</p>
      ${a.now && html`<p class="now"><${Icon} name="info" size=${15} /><span>${a.now}</span></p>`}
      ${a.offer && html`<p class=${'offer' + (a.warn ? ' warn' : '')}>${a.offer}</p>`}
      ${a.choices.length > 0 && html`<div class="choices"><b class="ed-caps">${tr('Or take one of its own')}</b>
        ${a.choices.map(([name, sub], k) => html`<button type="button" class="choice" onClick=${() => answer('choose', k)}><${Icon} name="garage" size=${18} /><span><b>${name}</b><small>${sub}</small></span><${Icon} name="chevron_right" size=${16} /></button>`)}
      </div>`}
      ${a.note && html`<p class="note"><${Icon} name="folder_open" size=${14} /><span>${a.note}${a.place && html`<small>${a.place}</small>`}</span></p>`}
      <div class="buttons">
        <${Btn} label="Not now" onClick=${() => answer('later')} />
        <${Btn} kind="primary" icon=${a.add ? 'add' : 'garage'} raw label=${a.add || tr('Choose a depot file')} onClick=${() => answer(a.add ? 'add' : 'open')} />
      </div>
    </div>
  </div>`;
}

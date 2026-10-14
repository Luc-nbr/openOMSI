// RosterFlow's part of the page's self-check (`js/omsihub/selftest.js`, after the bus company's):
// the Planning page opened as RosterFlow when its add-on is on, every tab, panel, card and menu
// drawn without a script error, its commands answering (the day's board, the drop zones of a
// trip and of a duty's trip, the candidates, a proposal and keeping one's own), the tutorial (the
// menu, the help sheet, every stop of the guided tour finding its part) and the lessons' checks
// run on what RosterFlow shows, as far as they go without an edit. Nothing is saved or printed: a
// proposal is kept out ("Keep mine"), a lesson ends keeping the nothing it changed; the one edit,
// the mouse's drag in Drivers, is undone at once.

import { S } from '../../store.js';
import { HOME, setCo, cmd, handOver } from '../company/kit.js';
import { enterCompany, leaveCompany } from '../company/company.js';
import { setRf, setPart, rfCmd, rfAct, pick, settle, ROSTER, VEHICLES, DRIVERS, TIMEPLAN } from './state.js';
import { anchored } from './anchors.js';
import { leaveRosterFlow } from './index.js';
import { STOPS, LESSONS, startTour, leaveTour, beginLesson, endLesson } from './tutorial.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

/** What is wrong with `el`'s parts lying side by side in its one row (those of the parts named
 * by `open` one by one): one over the one before it, past the end, above or below the row. */
function inARow(el, open) {
  if (!el) return ['not drawn'];
  const box = el.getBoundingClientRect();
  const parts = [...el.children]
    .flatMap((k) => (open.some((c) => k.classList.contains(c)) ? [...k.children] : [k]))
    .filter((k) => k.offsetWidth > 0 && getComputedStyle(k).position !== 'absolute');
  const out = [];
  let right = box.left;
  for (const k of parts) {
    const r = k.getBoundingClientRect();
    const name = k.getAttribute('class') || k.tagName.toLowerCase();
    if (r.left < right - 0.5) out.push(`${name} over the one before it`);
    if (r.right > box.right + 0.5) out.push(`${name} past the end`);
    if (r.top < box.top - 0.5 || r.bottom > box.bottom + 0.5) out.push(`${name} out of the row`);
    right = Math.max(right, r.right);
  }
  return out;
}

/** Where the Gantt's parts lie on the screen (`gantt.js` keeps them on its canvas: `rfHits`). */
function ganttHits(kind) {
  const cv = $('.rf-gantt canvas');
  if (!cv) return [];
  const box = cv.getBoundingClientRect();
  const k = cv.offsetWidth ? box.width / cv.offsetWidth : 1;
  return (cv.rfHits || [])
    .filter((h) => h.kind === kind)
    .map((h) => ({ ...h, cx: box.left + (h.x + h.w / 2) * k, cy: box.top + (h.y + h.h / 2) * k }))
    .filter((h) => h.cx > box.left + 200 * k && h.cx < box.right - 16 && h.cy > box.top && h.cy < box.bottom);
}

/** A mouse event as the mouse makes it: to whatever lies under the point (a part over the canvas
 * takes it, as it would take a real mouse's), not to the canvas itself. Returns the element hit. */
function mouseAt(type, x, y, buttons, mods = {}) {
  const el = document.elementFromPoint(x, y);
  el?.dispatchEvent(new MouseEvent(type, { clientX: x, clientY: y, bubbles: true, cancelable: true, button: 0, buttons, ...mods }));
  return el;
}

/** Luc's driver drag with the mouse in the Drivers tab: a chip of a duty pressed, moved until it
 * is taken up, carried over a drop zone of another duty (or New duty) and let go there. What it
 * found and whether the move took (an edit in the draft, undone after). */
async function driverDrag(mods = {}) {
  const chips = ganttHits('chip');
  const byRow = {};
  for (const c of chips) (byRow[c.dutyRow] ||= []).push(c);
  const chip = Object.values(byRow).find((r) => r.length >= 2)?.[0] || chips[0];
  if (!chip) return { ok: false, said: 'no chip of a duty in view' };
  const edits = S.rf.edits;
  const under = mouseAt('mousedown', chip.cx, chip.cy, 1, mods);
  if (under?.tagName !== 'CANVAS') return { ok: false, said: `${under?.tagName}.${under?.className} lies over the chip` };
  for (let k = 1; k <= 4; k++) {
    mouseAt('mousemove', chip.cx + 3 * k, chip.cy + 3 * k, 1, mods);
    await sleep(40);
  }
  await until(() => Array.isArray(S.rf.zones), 15000);
  await sleep(500);
  const zones = ganttHits('zone');
  const zone = zones.find((z) => S.rf.zones?.[z.zi]?.drop?.kind === 'duty') || zones[0];
  if (!zone) {
    mouseAt('mouseup', chip.cx + 12, chip.cy + 12, 0, mods);
    return { ok: false, said: `taken up: ${S.rf.hand?.kind}, ${S.rf.zones?.length ?? 0} zones, none in view - ${S.co.notice?.text || ''}` };
  }
  const kind = S.rf.zones[zone.zi]?.drop?.kind;
  mouseAt('mousemove', zone.cx, zone.cy, 1, mods);
  await sleep(400);
  const card = !!$('.rf-zonecard');
  const dropped = mouseAt('mouseup', zone.cx, zone.cy, 0, mods);
  await until(() => S.rf.edits > edits, 8000);
  const ok = S.rf.edits > edits && dropped?.tagName === 'CANVAS';
  if (ok) await rfAct('undo');
  return { ok, said: `${S.rf.hand ? S.rf.hand.kind : 'dropped'} on ${kind}${zone.zi} (${zones.length} zones${card ? ', its cost card' : ''})${ok ? ': moved, undone' : ` - ${S.co.notice?.text || 'not moved'}`}` };
}

async function until(f, ms = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (f()) return true;
    await sleep(100);
  }
  return false;
}

export async function selftest(check, errors) {
  /** `f` done, then a moment to draw: null when no script error came, else the errors. */
  const shown = async (f, ms = 600) => {
    const before = errors.length;
    if (f) await f();
    await sleep(ms);
    return errors.length === before ? null : errors.slice(before).join('; ');
  };

  // a company of the driver's with RosterFlow on
  enterCompany();
  await sleep(1500);
  const list = S.co.list?.companies?.tiles || [];
  let on = null;
  for (const c of list) {
    await cmd('co_open', { id: c.id }).catch(() => {});
    setCo({ chooser: null, tab: HOME });
    await sleep(800);
    on = await rfCmd('rf_on').catch(() => null);
    if (on?.on) break;
  }
  if (!on?.on) {
    check('rosterflow: a company with the add-on on', true, list.length ? 'none of this profile has RosterFlow on: not drawn' : 'no company in this profile');
    await leaveCompany();
    return;
  }

  // the Planning page handed over: RosterFlow opens over the whole window
  let e = await shown(() => handOver('5'), 1800);
  check('rosterflow: the Planning page opens it', !e && S.rf.open && S.co.tab === 5 && !!$('.rf-title'), e || `${on.company}, ${on.days?.length} days`);
  setRf({ intro: null });
  const read = await until(() => !!S.rf.data?.board || !!S.rf.data?.reading, 30000);
  const b = S.rf.data?.board;
  check('rosterflow: the day', read, b ? `${b.blocks.length} buses, ${b.duties.length} duties, ${b.issues.length} issues, ${b.stack.length} on the stack` : S.rf.data?.reading || 'not read');

  // the tabs
  for (const [view, name, sel] of [
    [VEHICLES, 'Vehicles', '.rf-gantt canvas'],
    [DRIVERS, 'Drivers', '.rf-gantt canvas'],
    [ROSTER, 'Roster', '.rr-days'],
    [TIMEPLAN, 'Timeplan', '.tp-bar, .tp-why'],
  ]) {
    e = await shown(() => setRf({ view, card: null, menu: null, panel: null, hand: null, zones: null, sel: [] }), 1500);
    check(`rosterflow: the ${name} tab`, !e && !!$(sel), e || `${$$('.rf-body *').length} elements`);
    await globalThis.hubProbeScales?.(`RosterFlow, ${name}`);
  }
  for (const tab of [0, 1]) {
    e = await shown(() => setPart('tp', { tab }), 1200);
    globalThis.hubProbe?.(`RosterFlow, the Timeplan's ${tab ? 'timetable' : 'running times'}`);
    check(`rosterflow: the Timeplan's ${tab ? 'timetable' : 'running times'}`, !e && (!!$('.tp-body') || !!$('.tp-why')), e || ($('.tp-why')?.textContent || `${$$('.tp-cell').length} cells`).slice(0, 160));
  }
  setPart('tp', { tab: 0 });

  // a narrow window (RosterFlow made narrower in place: 1180 to 776 px, what the smallest window
  // leaves at a Launcher size of 125 % and 150 %): the title bar one row of 48 px whose parts
  // neither overlap nor run over its end, the menu's icons and undo, redo and the history all
  // there, and the toolbar's (and the Timeplan's bar's) parts side by side
  const rf = $('.rf');
  for (const [view, name] of [[VEHICLES, 'Vehicles'], [ROSTER, 'Roster'], [TIMEPLAN, 'Timeplan']]) {
    setRf({ view, card: null, menu: null, panel: null });
    for (const w of [1180, 936, 776]) {
      rf.style.right = 'auto';
      rf.style.width = `${w}px`;
      e = await shown(null, 700);
      const bar = $('.rf-title');
      const wrong = [
        ...(bar?.offsetHeight === 48 ? [] : [`${bar?.offsetHeight} px high`]),
        ...inARow(bar, ['rf-icons', 'undo']),
        ...($$('.rf-title .rf-icons .rf-ibtn').length === 5 && $$('.rf-title .undo .rf-ibtn').length === 3 ? [] : ['icons missing']),
        ...(view === VEHICLES ? inARow($('.rf-toolbar'), []).map((x) => `toolbar: ${x}`) : []),
        ...(view === TIMEPLAN && $('.tp-bar') ? inARow($('.tp-bar'), ['tp-sides']).map((x) => `Timeplan's bar: ${x}`) : []),
      ];
      check(`rosterflow: the title bar at ${w} px (${name})`, !e && !wrong.length, e || wrong.join('; ') || `level ${bar.className.match(/fit(\d)/)?.[1]}: ${bar.querySelector('.rf-name > b')?.textContent.trim().slice(0, 60) || 'the dot'}`);
    }
  }
  rf.style.removeProperty('right');
  rf.style.removeProperty('width');
  await sleep(300);

  // the panels and windows over Vehicles
  setRf({ view: VEHICLES });
  await sleep(800);
  const panels = [
    ['the figures opened', { kpisOpen: true }, '.rf-kpis.open'],
    ['the history', { kpisOpen: false, panel: 'history' }, '.rf-history'],
    ['print', { panel: 'print' }, '.rf-menu'],
    ['the day', { panel: 'day' }, '.rf-menu'],
    ['go to', { panel: { kind: 'goto', text: '' } }, '.rf-goto'],
    ['the optimisation settings', { panel: 'optimise' }, '.rf-opt'],
    ['Stats', { panel: 'stats' }, '.rf-small'],
    ['Layers', { panel: 'layers' }, '.rf-small'],
    ['Filter', { panel: 'filter', filterEdit: { ...S.rf.filter } }, '.rf-filter'],
    ['the zoom', { panel: 'zoom' }, '.rf-menu'],
    ['the global issues', { panel: { kind: 'issues', scope: 'global' } }, '.rf-issues'],
    ['the duty issues', { panel: { kind: 'issues', scope: 'duty' } }, '.rf-issues'],
    ['the vehicle issues', { panel: { kind: 'issues', scope: 'vehicle' } }, '.rf-issues'],
    ['the duty types histogram', { kpisOpen: true, panel: { kind: 'histogram', which: 0 } }, '.rf-bigHist'],
    ['the paid time histogram', { panel: { kind: 'histogram', which: 1 } }, '.rf-bigHist'],
  ];
  for (const [name, patch, sel] of panels) {
    e = await shown(() => setRf(patch), 500);
    check(`rosterflow: ${name}`, !e && !!$(sel), e || 'drawn');
    globalThis.hubProbe?.(`RosterFlow, ${name}`);
  }
  setRf({ panel: null, kpisOpen: false });
  for (const item of ['layover', 'prepost', 'signing', 'points', 'relief', 'travel', 'breaks', 'continuous', 'limits', 'pieces', 'changeover', 'split', 'types', 'depot', 'costs', 'algorithm']) {
    e = await shown(() => setRf({ prefs: { ...(S.rf.prefs || { search: '', inUse: false }), item } }), 500);
    if (e || !$('.rf-prefs .pbody')) check(`rosterflow: the preferences, ${item}`, false, e || 'not drawn');
    if (item === 'layover' || item === 'types' || item === 'costs') globalThis.hubProbe?.(`RosterFlow, the preferences, ${item}`);
  }
  check('rosterflow: the preferences', !!S.rf.prefs?.v && !!$('.rf-prefs .pbody'), `${$$('.rf-prefs .item').length} items`);
  setRf({ prefs: null });

  // the cards, the menu, a hand with its drop zones, the candidates
  const first = b && (() => {
    for (let i = 0; i < b.blocks.length; i++) {
      const j = b.blocks[i].trips.findIndex((x) => x.counts);
      if (j >= 0) return [i, j];
    }
    return null;
  })();
  if (first) {
    const [i, j] = first;
    const at = { x: 300, y: 200 };
    for (const [name, patch, sel] of [
      ['the trip card', { sel: [first], card: { kind: 'trip', tour: i, trip: j, at, assign: false } }, '.rf-card'],
      ['the trip card with its buses', { card: { kind: 'trip', tour: i, trip: j, at, assign: true } }, '.rf-card .assign'],
      ['the vehicle card', { card: { kind: 'vehicle', tour: i, at } }, '.rf-card'],
      ['the choice card', { sel: [first], card: { kind: 'selection', at } }, '.rf-card'],
      ['the menu', { card: null, menu: { at, tour: i, trip: j, dutyRow: null, piece: null, custom: null } }, '.rf-menu'],
      ["a line's colour", { menu: null, panel: { kind: 'colour', line: b.blocks[i].line } }, '.rf-colour'],
    ]) {
      e = await shown(() => setRf(patch), 600);
      check(`rosterflow: ${name}`, !e && !!$(sel), e || 'drawn');
      globalThis.hubProbe?.(`RosterFlow, ${name}`);
    }
    settle();
    setRf({ panel: null });
    const cands = await rfCmd('rf_view', { view: 'candidates', day: S.rf.day, tour: i, trip: j }).catch((x) => ({ error: x.message }));
    check("rosterflow: a trip's other buses", Array.isArray(cands), Array.isArray(cands) ? `${cands.length}` : cands.error);
    await pick({ kind: 'trips', tour: i, trips: [j] }, false);
    await until(() => Array.isArray(S.rf.zones), 15000);
    const z = S.rf.zones || [];
    check("rosterflow: a trip's drop zones", Array.isArray(S.rf.zones) && z.every((x) => x.stars >= 1 && x.stars <= 5), z.map((x) => `${x.drop.kind}${x.drop.i ?? ''} ${x.stars}* ${x.delta_text}`).join(', ') || 'none');
    if (z.length) {
      e = await shown(() => setRf({ zoneHover: 0, zoneAt: { x: 300, y: 200 } }), 500);
      check('rosterflow: the cost card', !e && !!$('.rf-zonecard'), e || $('.rf-zonecard .boxes')?.textContent || '');
    }
    settle();
    // Drivers: a single trip of a duty (Luc's driver drag) and the whole piece
    setRf({ view: DRIVERS });
    await sleep(800);
    const piece = Math.max(0, b.blocks[i].pieces.findIndex(([a, zz]) => j >= a && j < zz));
    const row = b.blocks[i].duty_rows?.[piece] ?? 0;
    for (const [name, hand] of [
      ["a duty's trip", { kind: 'duty_trips', duty: row, trips: [first] }],
      ['a piece of work', { kind: 'piece', tour: i, duty: piece }],
    ]) {
      setRf({ zones: null });
      await pick(hand, false);
      await until(() => Array.isArray(S.rf.zones), 15000);
      const zz = S.rf.zones || [];
      check(`rosterflow: the drop zones of ${name}`, Array.isArray(S.rf.zones), zz.map((x) => `${x.drop.kind}${x.drop.d ?? ''} ${x.stars}* ${x.delta_text}`).join(', ') || 'none');
      settle();
    }
    // the drag itself, with the mouse: a trip onto another duty, the move kept (and undone)
    for (const [name, mods] of [['a trip', {}], ['a piece (Alt)', { altKey: true }]]) {
      settle();
      setRf({ view: DRIVERS, zones: null, scroll: 0, pan: 0, zoom: 1 });
      await sleep(900);
      const before = errors.length;
      const r = await driverDrag(mods);
      check(`rosterflow: Drivers, ${name} dragged onto another duty`, r.ok && errors.length === before, (errors.length === before ? '' : errors.slice(before).join('; ') + ' | ') + r.said);
      await sleep(600);
    }
    settle();
    // Ctrl and the wheel over the Gantt: RosterFlow's zoom, never the launcher's size
    {
      setRf({ view: DRIVERS, zoom: 1 });
      await sleep(600);
      const cv = $('.rf-gantt canvas');
      const scale = S.settings?.launcher_scale ?? 1;
      const box = cv?.getBoundingClientRect();
      for (let k = 0; k < 2; k++) cv?.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, ctrlKey: true, clientX: box.left + box.width * 0.6, clientY: box.top + box.height / 2, bubbles: true, cancelable: true }));
      await sleep(500);
      const zoomed = S.rf.zoom;
      check("rosterflow: Ctrl and the wheel zoom the Gantt, not the launcher", zoomed > 1 && (S.settings?.launcher_scale ?? 1) === scale, `RosterFlow's zoom ${zoomed.toFixed(2)}, Launcher size ${S.settings?.launcher_scale ?? 1}`);
      setRf({ zoom: 1, pan: 0 });
    }
    e = await shown(() => setRf({ menu: { at: { x: 300, y: 200 }, tour: i, trip: j, dutyRow: row, piece: [i, piece], custom: null } }), 500);
    check("rosterflow: the Drivers tab's menu", !e && !!$('.rf-menu'), e || [...$$('.rf-menu .rf-mi')].map((x) => x.textContent).join(', '));
    settle();
    if (b.duties[0]) {
      e = await shown(() => setRf({ panel: { kind: 'duty', id: b.duties[0].id } }), 1200);
      check('rosterflow: the duty panel', !e && !!$('.rf-duty .times'), e || $('.rf-duty .head b')?.textContent || '');
      setRf({ panel: null });
    }
  }

  // a proposal, kept out
  setRf({ view: VEHICLES });
  await rfAct('optimise');
  const proposed = await until(() => S.rf.data?.proposal === true, 20000);
  check('rosterflow: OPTIMIZE proposes', proposed && !!$('.rf-proposal'), proposed ? $('.rf-kpis')?.textContent.replace(/\s+/g, ' ').slice(0, 160) : 'no proposal');
  await rfAct('keep_mine');
  check('rosterflow: Keep mine', await until(() => S.rf.data?.proposal === false, 10000), 'the planning as it was');

  // the tutorial: its menu, the help sheet, every stop of the tour finding its part
  e = await shown(() => setPart('tut', { menu: true, seen: { seen: true, at: 0, done: false, lessons: [] } }), 500);
  check("rosterflow: the tutorial's menu", !e && !!$('.rf-tutmenu'), e || `${$$('.rf-tutmenu button').length} entries`);
  globalThis.hubProbe?.("RosterFlow, the tutorial's menu");
  e = await shown(() => setPart('tut', { menu: false, sheet: true }), 800);
  check('rosterflow: the help sheet', !e && $$('.rf-help .word canvas').length > 10, e || `${$$('.rf-help .word').length} words, ${$$('.rf-help .key').length} shortcuts`);
  globalThis.hubProbe?.('RosterFlow, the help sheet');
  setPart('tut', { sheet: false });
  startTour(0);
  const lost = [];
  for (let k = 0; k < STOPS.length; k++) {
    setPart('tut', { tour: { ...S.rf.tut.tour, at: k } });
    await sleep(700);
    if (k % 4 === 0) globalThis.hubProbe?.(`RosterFlow, the guided tour, stop ${k + 1}`);
    const target = STOPS[k][3];
    const found = target.startsWith('card:') ? !!$('.rf-tour .rf-pic') : target.split('|').some((n) => anchored(n));
    if (!found) lost.push(`${k + 1} ${STOPS[k][1]} (${target})`);
  }
  leaveTour(false);
  // (a part the day does not draw is left out, as the game-drawn launcher leaves it out: a chip
  // too narrow to point at, an empty stack, a line without a timetable of its own yet)
  const optional = ['rf-chip', 'rf-stack', 'rf-relief|rf-row', 'rf-tp-bar', 'rf-tp-body', 'rf-tp-save', 'rf-new-row'];
  const must = lost.filter((x) => !optional.some((o) => x.endsWith(`(${o})`)));
  check('rosterflow: the guided tour finds every part', must.length === 0, lost.length ? `${STOPS.length - lost.length} of ${STOPS.length} stops; left out here: ${lost.join('; ')}` : `${STOPS.length} stops`);
  await sleep(300);

  // the lessons: each begun, its steps ticked off as far as they go without an edit
  const lessonRuns = {
    'move-trip': [() => setRf({ view: VEHICLES }), () => first && pick({ kind: 'trips', tour: first[0], trips: [first[1]] }, false), () => setRf({ zoneHover: 0, zoneAt: { x: 300, y: 200 } })],
    'cut-join': [() => setRf({ view: VEHICLES })],
    'build-duty': [() => setRf({ view: DRIVERS }), () => first && pick({ kind: 'piece', tour: first[0], duty: 0 }, false)],
    'fix-issue': [() => setRf({ view: VEHICLES }), () => setRf({ panel: { kind: 'issues', scope: 'duty' } }), () => setRf({ selRow: b?.duties[0]?.id || null })],
    optimise: [() => setRf({ view: VEHICLES }), () => rfAct('optimise'), () => $('.rf-guide .hub-btn.primary')?.click(), () => rfAct('keep_mine')],
    timeplan: [() => setRf({ view: TIMEPLAN }), () => setPart('tp', { tab: 0 }), null, () => setPart('tp', { tab: 1 })],
    colour: [() => setRf({ view: VEHICLES }), () => first && setRf({ panel: { kind: 'colour', line: b.blocks[first[0]].line } })],
    'save-discard': [() => setRf({ view: VEHICLES })],
  };
  for (const [id, title, steps] of LESSONS) {
    settle();
    setRf({ panel: null, view: id === 'build-duty' ? VEHICLES : DRIVERS });
    await sleep(400);
    const before = errors.length;
    await beginLesson(id);
    await sleep(900);
    const runs = lessonRuns[id] || [];
    let reached = 0;
    for (const run of runs) {
      if (!run) break;
      await run();
      await sleep(1200);
      reached = S.rf.tut.lesson?.at ?? reached;
    }
    globalThis.hubProbe?.(`RosterFlow, lesson "${title}"`);
    const l = S.rf.tut.lesson;
    if (!l) check(`rosterflow: lesson "${title}" on its way`, false, `no lesson after it began: ${JSON.stringify({ ...S.rf.tut, seen: undefined }).slice(0, 240)} open=${S.rf.open} tab=${S.co.tab}`);
    const why = l?.stage === 'blocked' ? `waits: ${l.why}` : `${l?.done.filter(Boolean).length ?? 0} of ${steps.length} steps ticked off by themselves`;
    // (a null among the runs is a step that needs an edit: the lesson goes as far as the runs before it)
    const stop = runs.indexOf(null);
    const ok = errors.length === before && !!$('.rf-guide') && (l?.stage === 'blocked' || (l?.at ?? 0) >= Math.min(stop < 0 ? runs.length : stop, steps.length));
    const state = ok ? '' : ` [${JSON.stringify({ ...l, start: undefined, done: undefined })} view=${S.rf.view} board=${!!S.rf.data?.board} guide=${!!$('.rf-guide')} tour=${!!S.rf.tut.tour} rf=${!!$(".rf")} guard=${($(".co-empty.t-danger")?.textContent || "").slice(0, 400)}]`;
    check(`rosterflow: lesson "${title}"`, ok, (errors.length === before ? why : errors.slice(before).join('; ')) + state);
    settle();
    await endLesson(false);
    await sleep(300);
  }

  // switched on just now: the tutorial's menu offered; Escape and Back to the start screen
  e = await shown(() => handOver('5', { switched: true }), 1500);
  check('rosterflow: switched on, the tutorial offered', !e && S.rf.view === VEHICLES, e || (S.rf.tut.menu ? 'the menu open' : 'seen before: the menu waits under the ?'));
  setPart('tut', { menu: false });
  leaveRosterFlow();
  await sleep(500);
  check('rosterflow: Back to the start screen', !S.rf.open && S.co.tab === HOME, `tab ${S.co.tab}`);
  await leaveCompany();
}

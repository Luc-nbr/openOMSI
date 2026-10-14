// RosterFlow put in a state of its own for the comparison pictures with our game-drawn launcher's
// (`OMSI_LAUNCHER_PAGE` "bus company:5~<state>:<company>", company.js): a tab, a panel, a card,
// the menu, a hand with its drop zones, the tutorial. Only what the page shows changes - nothing
// is edited, saved or optimised into the company (a proposal is the Rust side's own until it is
// accepted, and is not).

import { S } from '../../store.js';
import { setRf, setPart, rfAct, pick, ROSTER, VEHICLES, DRIVERS, TIMEPLAN } from './state.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The board of the day, once it is read. */
async function board(ms = 15000) {
  const t0 = performance.now();
  while (performance.now() - t0 < ms) {
    if (S.rf.data?.board) return S.rf.data.board;
    await sleep(100);
  }
  return null;
}

/** The first trip with passengers of the first bus that has one. */
function firstTrip(b) {
  for (let i = 0; i < b.blocks.length; i++) {
    const j = b.blocks[i].trips.findIndex((x) => x.counts);
    if (j >= 0) return [i, j];
  }
  return null;
}

/** Put RosterFlow in state `name` (see the cases). */
export async function applyShot(name) {
  const [what, arg] = String(name || '').split('.');
  const view = { roster: ROSTER, timeplan: TIMEPLAN, timetable: TIMEPLAN, drivers: DRIVERS, duty: DRIVERS, dzones: DRIVERS, dmenu: DRIVERS }[what] ?? VEHICLES;
  // (no opening, no tour of its own)
  setRf({ view, intro: null, tut: { ...S.rf.tut, asked: S.boot?.config?.profile || '', offer: false } });
  if (what === 'timetable') setPart('tp', { tab: 1 });
  if (what === 'timeplan') setPart('tp', { tab: 0 });
  const b = await board();
  if (!b) return;
  await sleep(300);
  const t = firstTrip(b);
  // (where the game-drawn launcher's picture clicks: its first chip, 220,241 of its window)
  const at = { x: 208, y: 181 };
  switch (what) {
    case 'kpis':
      return setRf({ kpisOpen: true });
    case 'history':
    case 'print':
    case 'day':
    case 'optimise':
    case 'stats':
    case 'layers':
    case 'zoom':
      return setRf({ panel: what });
    case 'goto':
      return setRf({ panel: { kind: 'goto', text: '' } });
    case 'filter':
      return setRf({ panel: 'filter', filterEdit: { ...S.rf.filter } });
    case 'issues':
      return setRf({ panel: { kind: 'issues', scope: arg || 'global' }, issueTicks: [] });
    case 'histogram':
      return setRf({ kpisOpen: true, panel: { kind: 'histogram', which: Number(arg) || 0 } });
    case 'duty':
      return b.duties[0] && setRf({ panel: { kind: 'duty', id: b.duties[0].id }, selRow: b.duties[0].id });
    case 'prefs':
      return setRf({ prefs: { item: arg || 'signing', search: '', inUse: false } });
    case 'colour':
      return t && setRf({ panel: { kind: 'colour', line: b.blocks[t[0]].line } });
    case 'trip':
      return t && setRf({ sel: [t], selRow: b.blocks[t[0]].id, card: { kind: 'trip', tour: t[0], trip: t[1], at, assign: false } });
    case 'vehicle':
      return t && setRf({ card: { kind: 'vehicle', tour: t[0], at: { x: 100, y: at.y } } });
    case 'menu':
      return t && setRf({ sel: [t], selRow: b.blocks[t[0]].id, menu: { at, tour: t[0], trip: t[1], dutyRow: null, piece: null, custom: null } });
    case 'dmenu': {
      if (!t) return null;
      const piece = Math.max(0, b.blocks[t[0]].pieces.findIndex(([a, z]) => t[1] >= a && t[1] < z));
      const row = b.blocks[t[0]].duty_rows?.[piece] ?? 0;
      return setRf({ sel: [t], selRow: b.duties[row]?.id, menu: { at, tour: t[0], trip: t[1], dutyRow: row, piece: [t[0], piece], custom: null } });
    }
    case 'zones':
      return t && pick({ kind: 'trips', tour: t[0], trips: [t[1]] }, false);
    case 'dzones': {
      if (!t) return null;
      const piece = Math.max(0, b.blocks[t[0]].pieces.findIndex(([a, z]) => t[1] >= a && t[1] < z));
      const row = b.blocks[t[0]].duty_rows?.[piece] ?? 0;
      return pick(S.rf.tripsToDuties ? { kind: 'duty_trips', duty: row, trips: [t] } : { kind: 'piece', tour: t[0], duty: piece }, false);
    }
    case 'proposal':
      return rfAct('optimise');
    case 'tutmenu':
      return setPart('tut', { menu: true, seen: { seen: true, at: 0, done: false, lessons: [] } });
    case 'help':
      return setPart('tut', { sheet: true });
    case 'tour': {
      const { STOPS } = await import('./tutorial.js');
      const k = Math.max(0, Math.min(STOPS.length - 1, Number(arg) || 0));
      return setPart('tut', { tour: { plan: STOPS.map((_, n) => n), at: k, forward: true, home: { view, tab: S.rf.tp.tab } } });
    }
    case 'lesson': {
      const { LESSONS, probe } = await import('./tutorial.js');
      const l = LESSONS.find((x) => x[0] === (arg || 'move-trip')) || LESSONS[0];
      return setPart('tut', { lesson: { id: l[0], at: 0, done: l[2].map(() => false), start: probe(), stage: 'working', checked: false } });
    }
    default:
      return null;
  }
}

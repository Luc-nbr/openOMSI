// The editors' part of our self-check (`../selftest.js`, OPENOMSI_LAUNCHER_SELFTEST): every editor
// page drawn without a script error, their commands answering - the Timetable page's lines of a
// map, the bus's depot files, the depot editor's list - and, with the game program
// (OPENOMSI_LAUNCHER_SELFTEST=full), the line editor kept there: the map read, a new line with
// two stops routed over the roads. Nothing is saved: the new line is left as a draft of the
// session, which ends with the check.

import { call } from '../../api.js';
import { S, set } from '../../store.js';
import { language } from '../../i18n.js';
import { mapLabel } from './kit.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (q) => document.querySelector(q);

export async function editorsSelftest(check, errors) {
  const c = S.choice;
  // the pages
  for (const id of ['editor', 'timetable', 'depots', 'lines']) {
    const before = errors.length;
    set({ page: id, hub: { ...S.hub, palette: false } });
    await sleep(900);
    const e = errors.length === before ? null : errors.slice(before).join('; ');
    check(`editors: page ${id}`, !e && !!$('.ed-page'), e || $('.ed-page .hub-head h2')?.textContent || 'drawn');
  }
  set({ page: 'drive', hub: { ...S.hub, step: 'mode' } });
  await sleep(300);

  // their commands
  try {
    const d = await call('tt_load', { map: c.map });
    check('editors: the timetable of a map', Array.isArray(d?.lines), `${d?.lines?.length ?? 0} lines of ${c.map}`);
  } catch (x) {
    check('editors: the timetable of a map', false, x.message);
  }
  const map = (S.maps || []).find((m) => m.file === c.map) || S.maps?.[0];
  const bus = (S.vehicles || []).find((v) => v.file === c.bus) || S.vehicles?.[0];
  if (map && bus) {
    try {
      const m = await call('hub_depots', { bus: bus.file, map: { file: map.file, name: map.name, friendly: map.friendly, hof: map.hof || '' }, date: c.date, chosen: '', line: '', maps: (S.maps || []).map((x) => [mapLabel(x), x.hof || '']) });
      check("editors: the bus's depot files", m?.bus === bus.file, `${bus.file}: the map wants ${m?.want || '-'}`);
    } catch (x) {
      check("editors: the bus's depot files", false, x.message);
    }
  }
  try {
    const r = await call('de_show', { key: null, maps: [] });
    check('editors: the depot files of the player', Array.isArray(r?.entries), `${r?.entries?.length ?? 0} files`);
  } catch (x) {
    check('editors: the depot files of the player', false, x.message);
  }

  // the line editor in the game program: a map read, a new line routed
  if (S.boot?.selftest !== 'full' || !map) return;
  const le = (op, args = {}) => call('hub_le', { op, lang: language(), ...args });
  try {
    const maps = (S.maps || []).map((m) => [mapLabel(m), m.file]);
    let v = await le('open', { maps, root: S.boot?.root || '', date: c.date || '', choice_map: map.file, company: '', line: null, new_days: null, map_line: '' });
    for (let i = 0; i < 300 && !v.ready; i++) {
      await sleep(200);
      v = await le('view');
    }
    const data = v.ready ? await le('map_data') : null;
    check('editors: the line editor reads a map', !!v.ready && data?.roads?.length > 0, `${map.file}: ${data?.roads?.length ?? 0} roads, ${data?.stops?.length ?? 0} stops`);
    const stops = data?.stops || [];
    if (stops.length >= 2) {
      // (two stops near each other: a leg the router finds)
      const a = stops[0].at;
      let j = 1;
      let best = Infinity;
      stops.forEach((s, k) => {
        const d = Math.hypot(s.at[0] - a[0], s.at[1] - a[1]);
        if (k > 0 && d > 60 && d < best) (best = d), (j = k);
      });
      await le('new_line');
      await le('click_stop', { i: 0 });
      v = await le('click_stop', { i: j });
      const d = v.line?.directions?.[0];
      const leg = d?.legs?.[0];
      check('editors: a new line routed', d?.stops?.length === 2 && !!leg?.ok, `${stops[0].name} - ${stops[j].name}: ${leg ? `${Math.round(leg.length)} m${leg.ok ? '' : ', no way'}` : 'no leg'}`);
    }
    await le('leave');
  } catch (x) {
    check('editors: the line editor', false, x.message);
  }
}

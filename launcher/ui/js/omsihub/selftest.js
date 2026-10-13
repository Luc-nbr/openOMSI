// Our look's part of the page's self-check (`js/selftest.js`, OPENOMSI_LAUNCHER_SELFTEST): the
// frame and the start as our game-drawn launcher has them, every step of the drive drawn
// without a script error - for a tour, a work shift and a free drive - our commands answering,
// and the editors (`editors/selftest.js`). Nothing is clicked that starts a game: the steps are
// set, not gone through.

import { call } from '../api.js';
import { S, set } from '../store.js';
import { t } from '../i18n.js';
import { duty } from '../data.js';
import { hubExtras } from './flow.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

export async function selftest(check, errors) {
  const kept = { ...S.choice };
  const step = async (id, patch) => {
    const before = errors.length;
    if (patch) set({ choice: { ...S.choice, ...patch } });
    set({ page: 'drive', hub: { ...S.hub, step: id } });
    await sleep(700);
    return errors.length === before ? null : errors.slice(before).join('; ');
  };

  // the frame: the bar, the ground in the look chosen, the font
  const css = getComputedStyle(document.documentElement);
  check('hub: the bar', !!$('.hub-bar'), $('.hub-bar .hub-logo') ? 'with the logo' : 'no logo');
  check('hub: the look', !!css.getPropertyValue('--hub-ground').trim(), `${document.documentElement.dataset.theme} ${css.getPropertyValue('--hub-accent').trim()}`);
  check('hub: the font', getComputedStyle(document.body).fontFamily.includes('Hanken Grotesk'), getComputedStyle(document.body).fontFamily);

  // the start: greeting, the four ways to drive, the record, the links
  let e = await step('mode');
  const greeting = $('.hub-greeting')?.textContent || '';
  check('hub: the start', !e && $$('.hub-mode').length === 4 && !!$('.hub-record') && $$('.hub-link').length === 10, e || `${greeting} | ${$$('.hub-mode').length} tiles, ${$$('.hub-link').length} links`);
  check('hub: our texts translated', !S.boot?.language || S.boot.language === 'en' || t('How do you want to drive today?') !== 'How do you want to drive today?', S.boot?.language || 'en');

  // every step, for a tour, a work shift and a free drive
  for (const [mode, patch, steps] of [
    ['tour', { free: false, composed: false }, ['profile', 'map', 'day', 'duty', 'bus']],
    ['shift', { free: false, composed: true }, ['duty']],
    ['free', { free: true, composed: false }, ['start', 'day', 'bus']],
  ]) {
    for (const id of steps) {
      e = await step(id, patch);
      const sheet = !!$('.hub-sheet');
      const now = $('.hub-step.now')?.textContent || '';
      check(`hub: step ${id} (${mode})`, !e && sheet, e || now);
    }
  }
  // (the step's main action is there, and is not pressed)
  check('hub: the main action', !!$('.hub-go'), $('.hub-go')?.textContent || '');

  // our commands
  const c = S.choice;
  try {
    const shifts = await call('hub_shifts', { map: c.map, date: c.date, minutes: 120, daypart: 0, seed: 1, legs: [] });
    check('hub: work shifts', Array.isArray(shifts), `${shifts.length} shifts of 2 h`);
  } catch (x) {
    check('hub: work shifts', false, x.message);
  }
  try {
    const facts = await call('hub_map_facts', { maps: (S.maps || []).slice(0, 3).map((m) => m.file) });
    check('hub: the maps\' facts', facts && typeof facts === 'object', JSON.stringify(facts).slice(0, 160));
  } catch (x) {
    check('hub: the maps\' facts', false, x.message);
  }
  try {
    const x = await call('hub_duty_extras', { bus: c.bus, date: c.date, minutes: 9 * 60, free: false });
    check('hub: the duty\'s start', /^\d\d:\d\d$/.test(x.time), `${x.date} ${x.time} (${x.lead} min ahead)`);
  } catch (x) {
    check('hub: the duty\'s start', false, x.message);
  }
  // the command line a work shift becomes (`duty_args`: nothing is started), the shift's parts
  // and the bus ready before its first departure in it
  try {
    const list = await call('hub_shifts', { map: c.map, date: c.date, minutes: 120, daypart: 0, seed: 1, legs: [] });
    const d = list[0];
    if (d) {
      const time = Math.floor(d.first.departure / 60);
      set({ choice: { ...S.choice, free: false, composed: true, line: d.first.line, tour: d.first.tour, time, start_trip: [d.first.line, d.first.tour, d.first.index, time], legs: d.key } });
      const args = await call('duty_args', { duty: await duty(await hubExtras()) });
      const line = args.join(' ');
      check('hub: a shift\'s command line', line.includes('--duty-leg') && d.key.every((k) => args.includes(k)), line.slice(0, 220));
    }
  } catch (x) {
    check('hub: a shift\'s command line', false, x.message);
  }
  try {
    const w = await call('hub_weather_extras', { map: c.map });
    check('hub: the METAR airports', /^[A-Z]{4}$/.test(w.airport), `${w.airport}, ${w.airports.length} airports`);
  } catch (x) {
    check('hub: the METAR airports', false, x.message);
  }

  // back as it was
  set({ choice: kept, page: 'drive', hub: { ...S.hub, step: 'mode' } });
  await call('choice_save', { choice: kept }).catch(() => {});
  // the editors (`editors/selftest.js`)
  await (await import('./editors/selftest.js')).editorsSelftest(check, errors);
  await sleep(300);

  // the bus company (Trello "Tauri 2/6": js/omsihub/company/selftest.js)
  await (await import('./company/selftest.js')).selftest(check, errors);
}

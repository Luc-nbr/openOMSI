// Our look's part of the page's self-check (`js/selftest.js`, OPENOMSI_LAUNCHER_SELFTEST): the
// frame and the start as our game-drawn launcher has them, every step of the drive drawn
// without a script error - for a tour, a work shift and a free drive - our commands answering,
// and the editors (`editors/selftest.js`). Nothing is clicked that starts a game: the steps are
// set, not gone through.

import { call } from '../api.js';
import { S, set } from '../store.js';
import { t } from '../i18n.js';
import { duty, saveSettings } from '../data.js';
import { hubExtras } from './flow.js';
// (Beta 53: the layout checker at every page and state the checks show)
import { probe, atScales, told, SCALES } from './layout.js';
import { go } from './hub.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

export async function selftest(check, errors) {
  const kept = { ...S.choice };
  // the layout checker's probe for every part's checks (`layout.js`): what is shown now fits
  globalThis.hubProbe = (name) => probe(check, name);
  /** The layout of a state at every Launcher size (`layout.SCALES`), told as one check. */
  globalThis.hubProbeScales = async (name) => {
    const bad = [];
    await atScales(SCALES, name, (at, f) => f.length && bad.push(`${at}: ${told(f)}`));
    check(`layout: ${name}, every Launcher size`, !bad.length, bad.join(' || ') || SCALES.map((x) => `${Math.round(x * 100)} %`).join(', '));
  };
  const step = async (id, patch) => {
    const before = errors.length;
    if (patch) set({ choice: { ...S.choice, ...patch } });
    set({ page: 'drive', hub: { ...S.hub, step: id } });
    await sleep(700);
    globalThis.hubProbe(`step ${id}${patch ? ` (${patch.free ? 'free' : patch.composed ? 'shift' : 'tour'})` : ''}`);
    return errors.length === before ? null : errors.slice(before).join('; ');
  };

  // the frame: the bar, the ground in the look chosen, the font
  const css = getComputedStyle(document.documentElement);
  check('hub: the bar', !!$('.hub-bar'), $('.hub-bar .hub-logo') ? 'with the logo' : 'no logo');
  check('hub: the look', !!css.getPropertyValue('--hub-ground').trim(), `${document.documentElement.dataset.theme} ${css.getPropertyValue('--hub-accent').trim()}`);
  check('hub: the font', getComputedStyle(document.body).fontFamily.includes('Hanken Grotesk'), getComputedStyle(document.body).fontFamily);

  // the start: greeting, the four ways to drive, the record, the links (upstream's Model test
  // among them)
  let e = await step('mode');
  const greeting = $('.hub-greeting')?.textContent || '';
  check('hub: the start', !e && $$('.hub-mode').length === 4 && !!$('.hub-record') && $$('.hub-link').length === 11, e || `${greeting} | ${$$('.hub-mode').length} tiles, ${$$('.hub-link').length} links`);
  await globalThis.hubProbeScales('the start');
  // Ctrl and the wheel size the launcher (`zoom.js`): a notch up, then Ctrl 0 (the setting as it was)
  {
    const was = S.settings?.launcher_scale ?? 1;
    const card = $('.hub-card');
    const r = card?.getBoundingClientRect();
    card?.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, ctrlKey: true, clientX: r.left + 40, clientY: r.top + 40, bubbles: true, cancelable: true }));
    await sleep(300);
    const up = S.settings?.launcher_scale;
    const note = S.toasts.some((x) => x.id === 'hub-size');
    dispatchEvent(new KeyboardEvent('keydown', { key: '0', code: 'Digit0', ctrlKey: true, bubbles: true }));
    await sleep(300);
    const reset = S.settings?.launcher_scale;
    check('hub: Ctrl and the wheel size the launcher', Math.abs(up - (Math.round(Math.min(2, was + 0.05) * 20) / 20)) < 1e-9 && note && reset === 1, `${was} → ${up} (said: ${note}), Ctrl 0 → ${reset}`);
    if (was !== 1) await saveSettings({ launcher_scale: was });
    await sleep(400);
  }
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
  await globalThis.hubProbeScales('the duty step');
  // upstream's pages in our sheet, each one's layout (and the Settings page's tabs)
  for (const page of ['multiplayer', 'profile', 'mods', 'modeltest', 'settings', 'controls', 'games', 'tutorials', 'setup']) {
    go(page);
    await sleep(600);
    globalThis.hubProbe(`page ${page}`);
  }
  go('settings');
  await sleep(500);
  // (each tab of Settings, and the first at every Launcher size)
  const tabs = [...$$('.hub-page-body .tabs button')];
  for (const b of tabs) {
    b.click();
    await sleep(350);
    globalThis.hubProbe(`settings, ${b.textContent.trim()}`);
  }
  // (Beta 54: the bus models' categories and regions - the list comes once the buses are read)
  const vtab = tabs.find((b) => b.textContent.trim() === t('Vehicles'));
  if (vtab) {
    vtab.click();
    for (let k = 0; k < 60 && !$('.vh-row'); k++) await sleep(500);
    check("settings: the bus models' categories and regions", $$('.vh-row').length > 0, `${$$('.vh-row').length} rows drawn; ${$('.vh-shown')?.textContent || $('.vh .co-para')?.textContent || 'nothing'}`);
    globalThis.hubProbe('settings, Vehicles (the list)');
  } else check("settings: the bus models' categories and regions", false, 'no Vehicles tab');
  tabs[0]?.click();
  await sleep(300);
  await globalThis.hubProbeScales('page settings');
  go('drive', 'mode');
  await sleep(400);
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

  // the updates (Trello "Tauri 6/6": `update.js`): their state answers, the version as shown
  try {
    const u = await call('hub_update', { lang: 'en' });
    check('hub: the updates', typeof u.line === 'string' && !!u.shown, `${u.shown}: ${u.line}${u.dialog ? ` (${u.dialog.title})` : ''}`);
  } catch (x) {
    check('hub: the updates', false, x.message);
  }

  // what phase 3 added to the drive (Trello "Tauri 3/6": steps/selftest-more.js)
  await (await import('./steps/selftest-more.js')).selftest(check, errors, step);

  // back as it was
  set({ choice: kept, page: 'drive', hub: { ...S.hub, step: 'mode' } });
  await call('choice_save', { choice: kept }).catch(() => {});
  // the editors (`editors/selftest.js`)
  await (await import('./editors/selftest.js')).editorsSelftest(check, errors);
  await sleep(300);

  // the bus company (Trello "Tauri 2/6": js/omsihub/company/selftest.js)
  await (await import('./company/selftest.js')).selftest(check, errors);
  // RosterFlow, the Planning page with its add-on on (Omsi-Hub 4/6: js/omsihub/rosterflow/selftest.js)
  await (await import('./rosterflow/selftest.js')).selftest(check, errors);
}

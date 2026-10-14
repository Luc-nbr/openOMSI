// The self-check of what phase 3 added to the drive (Trello "Tauri 3/6"), called by our look's
// (`selftest.js`) with `step(id, patch)` that sets a step and says a script error drawn on it:
// the duty step's roadbook and map buttons, one's own weather, the bus options and the display
// font of the bus chosen, the bus that fits best, the welcome and the guided tour. Nothing is
// written that stays: the welcome is closed without `welcome_done`, the tour skipped, no font or
// option chosen.

import { call } from '../../api.js';
import { S, set } from '../../store.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// (what the display font's previews write without a duty: a name, not a text to translate)
const SAMPLE = '12 Hauptbahnhof';
const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

/** Ask `cmd` again while it answers that it is reading (at most `n` times). */
async function settled(cmd, args, n = 40) {
  for (let k = 0; k < n; k++) {
    const r = await call(cmd, args);
    if (!r?.reading) return r;
    await sleep(250);
  }
  return { reading: true };
}

export async function selftest(check, errors, step) {
  // the duty step: the map's buttons, the roadbook opened and put away
  let e = await step('duty', { free: false, composed: false });
  const tools = $$('.hub-maptools button').length;
  $('.hub-bookhandle')?.click();
  await sleep(400);
  const book = !!$('.hub-roadbook') && !!$('.hub-ibis');
  const rows = $$('.hub-rb-trip').length;
  $('.hub-rb-shut')?.click();
  await sleep(200);
  check('hub: the roadbook and the map buttons', !e && tools === 3 && book && !$('.hub-roadbook'), e || `${tools} buttons, ${rows} trips, IBIS ${book ? 'there' : 'missing'}`);

  // one's own weather: the editor in the list's place, the value through it unchanged
  const weather = S.choice.weather;
  try {
    const w = 'custom:vis=1200;br=0.80;wd=270;ws=6.5;t=-3.0;rh=88;p=1013;c=0.750;cb=50;pt=2;pi=120;wet=0.40;snow=1;snowroad=0';
    const r = await call('hub_more_weather_read', { weather: w });
    const back = await call('hub_more_weather_write', { custom: r.custom });
    e = await step('day', { weather: w });
    await sleep(300);
    const sliders = $$('.hub-cwx .hub-slider').length;
    check("hub: one's own weather", !e && back.weather === w && sliders === 9, e || `${sliders} sliders, ${back.weather === w ? 'kept as it was' : back.weather}`);
  } catch (x) {
    check("hub: one's own weather", false, x.message);
  }
  set({ choice: { ...S.choice, weather } });

  // the bus chosen: its options, its display font, whether it fits best
  const bus = S.choice.bus;
  if (bus) {
    try {
      const o = await settled('hub_more_bus_options', { bus, paint: S.choice.paint || '' });
      const sv = await call('hub_more_setvars', { bus, paint: S.choice.paint || '' });
      check('hub: the bus options', Array.isArray(o.options) && Array.isArray(sv), o.reading ? 'still reading' : `${o.options.length} options (${o.options.filter((x) => x.technical).length} technical), ${sv.length} passed`);
    } catch (x) {
      check('hub: the bus options', false, x.message);
    }
    try {
      const f = await settled('hub_more_font_view', { bus, text: SAMPLE }, 60);
      check('hub: the display font', !!(f.failed || f.plan || f.reading), f.reading ? 'still reading' : f.failed ? 'the displays could not be read' : f.sign ? `${f.options.length - 1} fonts, ${f.sign.font}` : 'drawn as pictures of its own');
    } catch (x) {
      check('hub: the display font', false, x.message);
    }
    e = await step('bus', {});
    set({ hub: { ...S.hub, busView: 'bus' } });
    await sleep(900);
    check('hub: the bus sheet of phase 3', !e && !!$('.hub-sheet.narrow'), e || `${$$('.hub-bo-row').length} option rows, display font ${$('.hub-df') ? 'there' : 'missing'}`);
    set({ hub: { ...S.hub, busView: 'tiles' } });
  }

  // the welcome: its steps drawn (closed again without `welcome_done`)
  try {
    const before = errors.length;
    const { welcomeStep } = await import('../welcome.js');
    const seen = [];
    for (const id of ['language', 'interface', 'animations', 'driver', 'ready']) {
      welcomeStep(id);
      await sleep(450);
      seen.push(`${id}${$('.hub-welcome') ? '' : ' (missing)'}`);
    }
    const tiles = (welcomeStep('language'), await sleep(300), $$('.hub-wlang').length);
    set({ hubWelcome: null });
    await sleep(200);
    check('hub: the welcome', errors.length === before && tiles === (S.boot?.languages || []).length, errors.length > before ? errors.slice(before).join('; ') : `${tiles} languages; ${seen.join(', ')}`);
  } catch (x) {
    check('hub: the welcome', false, x.message);
  }

  // the guided tour: started, two stops on, skipped (the page and the step back as they were)
  try {
    const before = errors.length;
    set({ page: 'drive', hub: { ...S.hub, step: 'mode' } });
    await sleep(300);
    const { startTour } = await import('../tour.js');
    startTour();
    await sleep(900);
    const first = $('.hub-tour-bubble h3')?.textContent || '';
    $('.hub-tour-bubble .next')?.click();
    await sleep(900);
    $('.hub-tour-bubble .next')?.click();
    await sleep(900);
    const third = $('.hub-tour-bubble h3')?.textContent || '';
    const spot = !!$('.hub-tour-ring');
    $('.hub-tour-bubble .skip')?.click();
    await sleep(600);
    const back = !S.hubTour && S.hub.step === 'mode';
    check('hub: the guided tour', errors.length === before && !!first && !!third && spot && back, errors.length > before ? errors.slice(before).join('; ') : `${first} → ${third}${back ? '' : ' (not back home)'}`);
  } catch (x) {
    check('hub: the guided tour', false, x.message);
  }
}

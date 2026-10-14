// What the day step has beyond phase 1's (Trello "Tauri 3/6"), as our game-drawn launcher's
// (`daytime::body`): one's own weather. "Custom weather" in the list of weathers, its editor in
// the list's place once it is chosen - visibility, light, wind, temperature and humidity,
// clouds, rain or snow, the road - and a preset made one's own to edit. The value is still
// just the choice's `weather` (`custom:...`), so it goes to the game as any other.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { useStore } from '../../store.js';
import { call } from '../../api.js';
import { choose } from '../../data.js';
import { Button, Slider, Switch, tr } from '../bits.js';
import { Select } from '../../ui.js';
import './more.js';

/** Whether a weather is one's own (`weather_choice::custom_weather`). */
export const isCustom = (w) => /^custom:/i.test(w || '');

/** OMSI's kinds of precipitation (`weather_choice::CUSTOM_PRECIP`). */
const PRECIP = ['None', 'Rain', 'Snow'];

/** The dew point of a temperature and a relative humidity (`weather_choice::dew_point_c`). */
function dew(t, rh) {
  const g = Math.log(Math.min(Math.max(rh, 0.1), 100) / 100) + (17.67 * t) / (243.5 + t);
  return (243.5 * g) / (17.67 - g);
}

const reads = new Map();
/** The weather chosen as the step reads it (`hub_more_weather_read`): its fields when it is
 * one's own, a preset as one's own to edit; null until it is read. */
export function useWeather(weather) {
  const [, redraw] = useState(0);
  const w = weather || '';
  useEffect(() => {
    if (reads.has(w)) return;
    reads.set(w, null);
    call('hub_more_weather_read', { weather: w })
      .then((r) => (reads.set(w, r), redraw((k) => k + 1)))
      .catch(() => reads.delete(w));
  }, [w]);
  return reads.get(w) || null;
}

/** "Custom weather" as the list offers it (`DayCtx::of`): one's own weather as it is, else the
 * default one ("custom:" reads as it). */
export const customItem = (c) => [isCustom(c.weather) ? c.weather : reads.get(c.weather || '')?.fresh || 'custom:', 'Custom weather', 'Set visibility, wind, clouds, rain, temperature and road state', 'tune'];

/** Over the list of weathers: a preset made one's own weather to edit, when it can be. */
export function EditAsCustom({ weather }) {
  const r = useWeather(weather);
  if (!r?.as_custom) return null;
  return html`<div class="hub-cwx-edit"><${Button} wide icon="tune" label="Edit selected weather as custom" onClick=${() => choose({ weather: r.as_custom })} /></div>`;
}

/** One's own weather's editor, in the list's place (`daytime::body`). */
export function CustomWeather() {
  const s = useStore();
  const w = s.choice.weather;
  const r = useWeather(w);
  const [f, setF] = useState(null);
  const wrote = useRef('');
  // (the fields read again only for a weather this editor did not write itself)
  useEffect(() => {
    if (r?.custom && w !== wrote.current) setF(r.custom);
  }, [r, w]);
  const v = f || r?.custom;
  const change = (k, x) => {
    const nf = { ...v, [k]: x };
    setF(nf);
    call('hub_more_weather_write', { custom: nf }).then((a) => {
      wrote.current = a.weather;
      reads.set(a.weather, { custom: a.custom, as_custom: null, fresh: reads.get(w)?.fresh });
      choose({ weather: a.weather });
    });
  };
  const slider = (k, label, min, max, step, format) => html`<${Slider} label=${label} value=${v[k]} min=${min} max=${max} step=${step} format=${format} onChange=${(x) => change(k, x)} />`;
  const pct = (x) => `${Math.round(x * 100)} %`;
  return html`<div class="hub-cwx">
    <${Button} wide icon="arrow_back" label="Choose a weather preset" onClick=${() => choose({ weather: '' })} />
    ${v &&
    html`<div class="hub-cwx-rows">
      ${slider('visibility_m', 'Visibility', 50, 50000, 50, (x) => (x >= 49950 ? tr('unlimited') : x >= 1000 ? `${(x / 1000).toFixed(1)} km` : `${Math.round(x)} m`))}
      ${slider('brightness', 'Brightness', 0, 1.5, 0.05, pct)}
      ${slider('wind_dir', 'Wind direction', 0, 355, 5, (x) => `${Math.round(x)}°`)}
      ${slider('wind_speed', 'Wind speed', 0, 40, 0.5, (x) => `${x.toFixed(1)} m/s`)}
      ${slider('temp_c', 'Temperature', -30, 45, 1, (x) => `${Math.round(x)} °C`)}
      <div class="gap">${slider('humidity', 'Humidity', 0, 100, 1, (x) => `${Math.round(x)} % · ${tr('dew %{t} °C', { t: Math.round(dew(v.temp_c, x)) })}`)}</div>
      ${slider('cloud_cover', 'Cloud cover', 0, 1, 0.01, (c) => (c <= 0 ? tr('Clear sky') : pct(c)))}
      <div class="hub-cwx-precip"><span>${tr('Precipitation')}</span>
        <${Select} value=${String(Math.min(2, Math.max(0, v.precip)))} options=${PRECIP.map((p, k) => [String(k), p])} onChange=${(k) => change('precip', Number(k))} /></div>
      ${slider('precip_intensity', 'Precipitation intensity', 0, 255, 1, (x) => `${Math.round(x)} / 255`)}
      ${slider('road_wetness', 'Road wetness', 0, 1, 0.05, pct)}
      <${Switch} label="Snow cover" checked=${v.snow_cover} onChange=${(x) => change('snow_cover', x)} />
      <${Switch} label="Snow on road" checked=${v.snow_on_road} onChange=${(x) => change('snow_on_road', x)} />
    </div>`}
  </div>`;
}

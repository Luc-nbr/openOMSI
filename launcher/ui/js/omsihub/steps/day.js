// The day and the weather (`daytime`), for every way to drive: when (the time, the date, the
// computer's own at a click), how long the bus stands ready before the first departure, the
// season, the traffic, how the drive starts, and the weather - the map's own, the current one
// of an airport, the weather cycle, or one of the presets that fit the season.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore } from '../../store.js';
import { call } from '../../api.js';
import { choose, curTour, pickedTrip, realTime, realDate, saveSettings } from '../../data.js';
import { tripAt } from '../../pages/drive.js';
import { Actions, Caps, Chips, Foot, Head, Icon, Slider, Switch, tr, Button } from '../bits.js';
import { goStep, nextOf, previousOf, freeDrive } from '../flow.js';
import { MapBehind } from './mapbehind.js';
import { Select } from '../../ui.js';

export const WEEKDAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
export const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const SEASONS = [['auto', 'By date'], ['spring', 'Spring'], ['summer', 'Summer'], ['autumn', 'Autumn'], ['winter', 'Winter']];
/** The minutes the bus may stand ready before the first departure (`daytime::LEADS`). */
const LEADS = [0, 2, 5, 10, 15, 20, 30];

export const parseDate = (d) => {
  const [y, m, dd] = String(d || '1989-05-30').split('-').map(Number);
  return [y || 1989, m || 5, dd || 30];
};
/** Monday 0 ... Sunday 6. */
export const weekday = (d) => {
  const [y, m, dd] = parseDate(d);
  return (new Date(Date.UTC(y, m - 1, dd)).getUTCDay() + 6) % 7;
};
const daysIn = (y, m) => new Date(Date.UTC(y, m, 0)).getUTCDate();
const hhmm = (t) => `${String(Math.floor(((t % 1440) + 1440) % 1440 / 60)).padStart(2, '0')}:${String((((t % 1440) + 1440) % 1440) % 60).padStart(2, '0')}`;
const seasonOf = (m) => (m === 12 || m <= 2 ? 'Winter' : m <= 5 ? 'Spring' : m <= 8 ? 'Summer' : 'Autumn');
export const dayName = (d) => tr(WEEKDAYS[weekday(d)]);
/** "30 May 1989". */
export const dateText = (d) => {
  const [y, m, dd] = parseDate(d);
  return `${dd} ${tr(MONTHS[m - 1])} ${y}`;
};

/** The weather as the head says it. */
export function weatherName(w) {
  if (!w) return tr('As on the map');
  if (w === 'cycle') return tr('Weather cycle');
  if (w.startsWith('metar:')) return tr('Current weather at %{code}', { code: w.slice(6) });
  return S.weather?.find((x) => x.file === w)?.name || w;
}

/** "09:00 · Tuesday 30 May 1989 · As on the map" (`daytime::day_line`). */
export const dayLine = (c) => `${hhmm(c.time)} · ${dayName(c.date)} ${dateText(c.date)} · ${weatherName(c.weather)}`;

/** The computer's clock: today's date and the minutes since midnight. */
function now() {
  const d = new Date();
  const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  return [iso, d.getHours() * 60 + d.getMinutes()];
}

/** The time as hours and minutes, each with its arrows (the timetables' 24 hours). */
function TimeField({ minutes, onChange, disabled }) {
  const h = Math.floor(minutes / 60) % 24;
  const m = minutes % 60;
  const put = (hh, mm) => onChange((((hh + 24) % 24) * 60 + ((mm + 60) % 60)));
  const part = (v, max, f) => html`<input value=${String(v).padStart(2, '0')} disabled=${disabled} maxlength="2" onFocus=${(e) => e.currentTarget.select()}
    onChange=${(e) => { const n = parseInt(e.currentTarget.value, 10); if (n >= 0 && n <= max) f(n); else e.currentTarget.value = String(v).padStart(2, '0'); }}
    onWheel=${(e) => { e.preventDefault(); f((v + (e.deltaY < 0 ? 1 : -1) + max + 1) % (max + 1)); }} />`;
  const spin = (up, down) => html`<span class="spin"><button type="button" disabled=${disabled} onClick=${up}><${Icon} name="expand_less" size=${14} /></button><button type="button" disabled=${disabled} onClick=${down}><${Icon} name="expand_more" size=${14} /></button></span>`;
  return html`<div class="hub-time">
    ${part(h, 23, (n) => put(n, m))}${spin(() => put(h + 1, m), () => put(h - 1, m))}
    <span style="color:var(--hub-text-dim);margin:0 6px">:</span>
    ${part(m, 59, (n) => put(h, n))}${spin(() => put(h, m + 1), () => put(h, m - 1))}
  </div>`;
}

function DateField({ value, onChange, disabled }) {
  return html`<label class="hub-date"><${Icon} name="calendar_month" size=${17} /><span>${dateText(value)}</span><${Icon} name="expand_more" size=${16} />
    <input type="date" value=${value} disabled=${disabled} onChange=${(e) => e.currentTarget.value && onChange(e.currentTarget.value)} /></label>`;
}

/** The weather's choices (`DayCtx::of`): the map's own, the current one of an airport, the
 * cycle, the presets that fit the season (`state::weather_fits`). */
function weatherItems(c, airport) {
  const [, m] = parseDate(c.date);
  const season = c.season && c.season !== 'auto' ? c.season : seasonOf(m).toLowerCase();
  const fits = (w) => {
    const rain = (w.precip || '').startsWith('rain');
    if (season === 'winter') return w.snow || (rain ? w.temp <= 10 : w.temp <= 16);
    if (w.snow || w.temp <= 0) return false;
    return season !== 'summer' || w.temp >= 8;
  };
  const code = c.weather?.startsWith('metar:') ? c.weather.slice(6) : airport || 'EDDB';
  const items = [
    ['', 'Map default', 'Whatever the map starts with', 'wb_sunny'],
    [`metar:${code}`, 'Current weather', tr('METAR of %{code} (fetched at the start)', { code }), 'public'],
    ['cycle', 'Weather cycle', 'Changes every 25-60 minutes, as the month allows', 'autorenew'],
  ];
  for (const w of S.weather || []) {
    if (!fits(w)) continue;
    const clouds = (w.clouds || '').toLowerCase();
    const icon = w.snow || (w.precip || '').startsWith('snow') ? 'weather_snowy' : (w.precip || '').startsWith('rain') ? 'rainy' : w.fog_m < 1500 ? 'foggy' : clouds.includes('overcast') ? 'cloud' : clouds.includes('cumulus') ? 'partly_cloudy_day' : 'wb_sunny';
    const vis = w.fog_m >= 20000 ? tr('clear air') : `${Math.round(w.fog_m)} m`;
    items.push([w.file, w.name, `${Math.round(w.temp)} °C · ${w.precip} · ${vis}`, icon, true]);
  }
  return items;
}

export function DayStep() {
  const s = useStore();
  const c = s.choice;
  const [wx, setWx] = useState(null);
  useEffect(() => {
    if (!c.map) return;
    call('hub_weather_extras', { map: c.map }).then(setWx).catch(() => setWx(null));
  }, [c.map]);
  const free = freeDrive();
  const lead = free ? null : Math.max(0, Math.min(60, Number(s.settings?.bus_lead ?? 5)));
  const [y, m, d] = parseDate(c.date);
  const seasonAt = SEASONS.findIndex(([v]) => v === c.season);
  const setSeason = (v) => {
    if (v === 'auto') return choose({ season: 'auto' });
    const month = { spring: 4, summer: 7, autumn: 10, winter: 1 }[v];
    choose({ season: v, date: `${String(y).padStart(4, '0')}-${String(month).padStart(2, '0')}-${String(Math.min(d, daysIn(y, month))).padStart(2, '0')}` });
  };
  const items = weatherItems(c, wx?.airport);
  const lines = s.lines.list;
  const running = lines ? lines.filter((l) => l.tours.some((t) => t.runs)).length : null;
  const foot = `${dayName(c.date)} · ${running == null ? tr('Reading the timetable…') : running === 1 ? tr('1 line runs on this day') : tr('%{n} lines run on this day', { n: running })}`;
  const tour = curTour();
  const trip = !c.free && tour ? tour.trips[pickedTrip() != null ? Math.max(0, tour.trips.findIndex((x) => x.index === pickedTrip())) : Math.max(0, tripAt(tour, c.time * 60))] : null;
  const metar = c.weather?.startsWith('metar:');
  return html`
    <${MapBehind} trip=${trip} />
    <section class="hub-sheet narrow">
      <${Head} icon="partly_cloudy_day" title="Day & weather" sub=${dayLine(c)} />
      <div class="hub-body"><div class="hub-pad">
        <div class="hub-section">
          <${Caps} title="When" value=${dayName(c.date)} />
          <div class="hub-two">
            <${TimeField} minutes=${c.time} disabled=${realTime()} onChange=${(time) => choose({ time })} />
            <${DateField} value=${c.date} disabled=${realDate()} onChange=${(date) => choose({ date, season: 'auto' })} />
          </div>
          <div class="hub-two" style="margin-top:10px">
            <${Button} label="Current time" onClick=${() => choose({ time: now()[1] })} />
            <${Button} label="Current date" onClick=${() => choose({ date: now()[0], season: 'auto' })} />
          </div>
        </div>
        ${lead != null && html`<div class="hub-section">
          <${Caps} title="Bus ready before departure" value=${lead === 0 ? tr('at the departure') : tr('the game starts at %{time}', { time: hhmm(c.time - lead) })} />
          <${Chips} raw value=${lead} options=${LEADS.map((n) => [n, n === 0 ? tr('None') : `${n} min`])} onChange=${(n) => saveSettings({ bus_lead: n })} />
        </div>`}
        <div class="hub-section">
          <${Caps} title="Season" value=${seasonAt <= 0 ? tr(seasonOf(m)) : ''} />
          <${Chips} value=${SEASONS[Math.max(0, seasonAt)][0]} options=${SEASONS} onChange=${setSeason} />
        </div>
        <div class="hub-section">
          <${Caps} title="Traffic" />
          <${Slider} label="Cars around" value=${c.traffic} min=${0} max=${120} onChange=${(traffic) => choose({ traffic })} format=${(v) => String(Math.round(v))} />
          <${Switch} label="Passengers" checked=${c.passengers} onChange=${(passengers) => choose({ passengers })} />
          <${Switch} label="Timetable buses" checked=${c.schedule} onChange=${(schedule) => choose({ schedule })} />
        </div>
        <div class="hub-section">
          <${Caps} title="At the start" />
          <${Switch} label="Put the bus into service on start (Shift+U)" checked=${c.autostart} onChange=${(autostart) => choose({ autostart })} />
          <${Switch} label="Start on foot (place a bus from the game menu)" checked=${c.on_foot} onChange=${(on_foot) => choose({ on_foot })} />
        </div>
        <div class="hub-section">
          <${Caps} title="Weather" value=${c.weather ? weatherName(c.weather) : tr('Map default')} />
          <div class="hub-weather">
            ${items.map(([value, name, meta, icon, raw]) => {
              const on = value === c.weather || (value.startsWith('metar:') && metar);
              return html`<button type="button" class=${'hub-wx' + (on ? ' on' : '')} onClick=${() => choose({ weather: value })}>
                <${Icon} name=${icon} size=${20} /><span><b>${raw ? name : tr(name)}</b><small>${raw ? meta : tr(meta)}</small></span>${on ? html`<span class="hub-radio on"></span>` : html`<span class="hub-radio"></span>`}
              </button>`;
            })}
          </div>
          ${metar && wx?.airports?.length > 0 && html`<div style="display:flex;align-items:center;gap:10px;margin-top:10px">
            <span style="color:var(--hub-text-soft);width:70px">${tr('Airport')}</span>
            <span style="flex:1"><${Select} raw value=${c.weather.slice(6)} options=${wx.airports} onChange=${(code) => choose({ weather: `metar:${code}` })} /></span>
          </div>`}
        </div>
      </div></div>
      <${Foot} text=${foot} />
    </section>
    <${Actions} back onBack=${() => goStep(previousOf('day'))} from=${'calc(var(--hub-edge-in) + clamp(288px, 27.4vw, 420px) + 14px)'} main=${free && c.free ? 'Next: the bus' : 'Next step'} onMain=${() => goStep(nextOf('day'))} />
  `;
}

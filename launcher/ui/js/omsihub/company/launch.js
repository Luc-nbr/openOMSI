// The company's duty launcher, as our game-drawn launcher's (`company/planning/duty_launch.rs`):
// a duty of the company fixed by the planning - its day and times, its rows (sign on, the trips,
// the breaks, sign off), the route of its first trip on the map, its bus and depot - and the
// start: the weather is the player's to choose, or why it cannot start yet. In RosterFlow's
// look (its own palette in either look).

import { S, toast } from '../../store.js';
import { duty } from '../../data.js';
import { RouteMap } from '../../routemap.js';
import { dark } from '../theme.js';
import { html, useEffect, act, useView, Icon, Mark, Select, inkOn, rgbOf } from './kit.js';

/** Start the duty: upstream's duty as the base, the company's duty over it on the Rust side
 * (`launch_start`, as our game-drawn launcher starts it); `dry` only says the command line. */
export async function startDuty(dry = false) {
  let base;
  try {
    base = await duty();
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
  return act('launch_start', { base, dry });
}

/** The RosterFlow button (`rf_button`): solid blue, else grey; its words in capitals. */
const RfButton = ({ label, solid, onClick, style = '' }) => html`<button type="button" class=${'co-rfbtn' + (solid ? ' solid' : '')} style=${style} onClick=${onClick}>${label}</button>`;

export function LaunchPage() {
  const v = useView('launch');
  // Escape closes the duty launcher (back to the page it came from)
  useEffect(() => {
    const k = (e) => {
      if (e.key !== 'Escape' || S.co.popup) return;
      const el = document.activeElement;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      act('launch_close');
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  if (!v) return null;
  const close = () => act('launch_close');
  // (Omsi-Hub 4/6: `act` hands over itself, with the duty; a second hand-over here raced it)
  const fix = () => act('launch_fix');
  return html`<div class="co-launch">
    <div class="head">
      <${Mark} mark=${v.mark} size=${48} />
      <div class="who"><span class="caps"><${Icon} name="lock" size=${13} />${v.caps}</span>
        <div><b>${v.name}</b>${v.title && html`<span>${v.title}</span>`}</div></div>
      <${RfButton} label=${v.to} style="width:190px" onClick=${fix} />
      <${RfButton} label=${v.back} style="width:110px" onClick=${close} />
    </div>
    ${v.reading
      ? html`<div class="reading">${v.reading}</div>`
      : html`<div class="tiles">${v.tiles.map(([label, value], k) => html`<div style=${`flex:${[1.5, 0.8, 0.8, 0.8, 1.5, 0.9][k]} 1 0`}><span class="caps">${label}</span><b class="cut">${value}</b></div>`)}</div>
        <div class="body">
          <div class="card rows"><span class="caps">${v.duty_caps}</span>
            ${v.rows.length ? html`<div class="co-scroll list">${v.rows.map((r) => html`<${Row} r=${r} colour=${v.colour} />`)}</div>` : html`<p class="dim">${v.no_rows}</p>`}
          </div>
          <div class="right">
            <div class="map"><span class="maplabel">${v.map_caps}</span>
              <${RouteMap} map=${v.map} date=${v.date} trip=${v.trips[0] || ''} height=${0} colors=${dark() ? ['#14171c', '#3b4656', '#4a5770', '#3fa8ff'] : ['#cbd3dd', '#ffffff', '#ffffff', '#127ce2']} /></div>
            <div class="low">
              <div class="card bus"><span class="caps">${v.bus_caps}</span>
                ${v.bus.map(([a, b]) => html`<div class="busrow"><span>${a}</span><b class="cut">${b?.trim() ? b : '-'}</b></div>`)}
                <p class="note"><${Icon} name="lock" size=${13} />${v.bus_note}</p></div>
              <${Start} s=${v.start} fix=${fix} label=${v.to} />
            </div>
          </div>
        </div>`}
  </div>`;
}

/** A row of the duty (`duty_launch::duty_card`). */
function Row({ r, colour }) {
  if (r.kind === 'break') return html`<div class="lrow break"><span class="time"></span><${Icon} name="schedule" size=${14} class="ic faint" /><span class="dim">${r.text}</span></div>`;
  if (r.kind === 'on' || r.kind === 'off') {
    return html`<div class="lrow"><b class="time">${r.time}</b><${Icon} name=${r.kind === 'on' ? 'login' : 'logout'} size=${17} class="ic accent" /><div class="what"><b>${r.title}</b><span class="dim">${r.at}</span></div></div>`;
  }
  const rgb = rgbOf(colour);
  return html`<div class="lrow trip"><div class="time"><b class=${r.empty ? 'dim' : ''}>${r.dep}</b><span class="faint">${r.arr}</span></div>
    ${r.empty
      ? html`<span class="chip empty"><${Icon} name="garage" size=${13} /></span><div class="what"><span class="dim" style="font-weight:500">${r.deadhead}</span><span class="faint">${r.route}</span></div>`
      : html`<span class="chip" style=${`background:rgb(${rgb.join(',')});color:${inkOn(rgb)}`}>${r.line}</span><div class="what"><span style="font-weight:500">${r.route}</span><span class="faint">${r.length}</span></div>`}
  </div>`;
}

/** The start (`duty_launch::start_card`): the weather and the start, or why it cannot start. */
function Start({ s, fix, label }) {
  if (s.blocked) {
    return html`<div class="card start">
      <span class="caps err">${s.caps}</span>
      <div class="hitch"><${Icon} name="error" size=${18} class="ic err" /><div><b>${s.title}</b>${s.detail && html`<p class="err">${s.detail}</p>`}<p class="dim">${s.what}</p>
        ${s.more.map((m) => html`<p style="font-weight:500">${m}</p>`)}</div></div>
      <span class="grow"></span>
      <${RfButton} label=${label} solid style="width:100%;height:40px;margin-bottom:12px" onClick=${fix} />
      <div class="off" title=${s.title}><span><${Icon} name="lock" size=${16} class="ic err" /><s>${s.off}</s></span><span class="err why cut">${s.why}</span></div>
    </div>`;
  }
  const k = Math.max(0, s.weathers.findIndex(([w]) => w === s.weather));
  return html`<div class="card start">
    <span class="caps">${s.caps}</span>
    <div class="weather"><span class="dim">${s.weather_label}</span><${Select} value=${k} options=${s.weathers.map(([, n]) => n)} onChange=${(i) => act('launch_weather', { weather: s.weathers[i][0] })} style="flex:1" /></div>
    <p class="when"><${Icon} name="schedule" size=${15} class="ic accent" />${s.when}</p>
    <p class="fixed"><${Icon} name="lock" size=${14} class="ic faint" />${s.fixed}</p>
    <span class="grow"></span>
    <${RfButton} label=${s.go} solid style="width:100%;height:52px" onClick=${() => !S.boot?.selftest && startDuty(false)} />
  </div>`;
}

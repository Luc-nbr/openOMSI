// A line's trips and timetable on the planning (Trello "Tauri 6/6"), as our game-drawn launcher's
// sheet over its planning (`company/planning/plan_edit.rs`): opened from a line's heading or
// one of its tours when RosterFlow is off. A group of days at a time, its departures lie per
// direction on a time axis - one dragged to another minute, the axis clicked to add one, the one
// chosen typed, moved a few minutes, copied a headway later or taken out (the arrows and Delete
// too), the wheel zooming into the hours - and in a list; the line's pattern is changed in one
// go; under the departures, on the same hours, the tours they make against those that run now;
// what the line asks of the fleet; and the foot: undo, start again, what is wrong, from which
// day a line in service changes, save and close. The line and its changes are the Rust side's
// (`company_more/plan_edit.rs`): this keeps how it is looked at.

import { html, useEffect, useRef, useState, setUi, Plate, Seg, Switch, Slider, Btn, IconBtn } from './kit.js';
import { useMore, actMore } from './more.js';
import { TimeSpin } from '../editors/kit.js';

/** A direction's lane and the label column beside the lanes (`LANE_H`, `LABEL_W`). */
const LANE_H = 50;
const LABEL_W = 168;
/** A tour's row under the departures, between these (`TOUR_H`, `TOUR_MAX`). */
const TOUR_H = 22;
const TOUR_MAX = 40;

const hm = (minutes) => {
  const m = Math.round(minutes);
  return `${String((((m / 60) | 0) % 24 + 24) % 24).padStart(2, '0')}:${String(((m % 60) + 60) % 60).padStart(2, '0')}`;
};

/** A time typed: "7:05", "07.05", "0705" or "705" (null: not a time; `parse_hm`). */
function parseHm(s) {
  s = String(s).trim();
  let h;
  let m;
  const k = s.search(/[:.h]/);
  if (k >= 0) [h, m] = [Number(s.slice(0, k)), Number(s.slice(k + 1))];
  else if (s.length >= 3 && /^\d+$/.test(s)) [h, m] = [Number(s.slice(0, -2)), Number(s.slice(-2))];
  else return null;
  return Number.isInteger(h) && Number.isInteger(m) && h >= 0 && h < 24 && m >= 0 && m < 60 ? h * 60 + m : null;
}
/** A time being typed is whole (`whole_time`). */
const wholeTime = (s) => {
  s = String(s).trim();
  const k = s.search(/[:.h]/);
  return k >= 0 ? s.slice(k + 1).trim().length === 2 : s.length === 4 && /^\d+$/.test(s);
};
/** A time typed for a departure at `before`: the day's or the next day's, whichever is nearer. */
const typedMinutes = (m, before) => (Math.abs(m + 1440 - before) < Math.abs(m - before) ? m + 1440 : m);

/** A lane's colour (`lane_colour`): the way out the accent's, the way back a calm blue. */
const laneColour = (k) => (k === 0 ? 'var(--hub-accent)' : k === 1 ? 'var(--co-early)' : k == null ? 'var(--hub-text-faint)' : 'var(--hub-text-soft)');
const toneColour = (t) => ({ warn: 'var(--co-warn)', ok: 'var(--co-ok)', danger: 'var(--co-danger-text)', accent2: 'var(--co-accent2)', dim: 'var(--hub-text-dim)', soft: 'var(--hub-text-soft)' })[t] || 'var(--hub-text)';

/** The hours zoomed into kept within the whole day's, two at the least (`fit_view`). */
function fitView(v, whole) {
  const span = Math.max(120, Math.min(28 * 60, v[1] - v[0]));
  if (span >= whole[1] - whole[0]) return null;
  const t0 = Math.max(whole[0], Math.min(whole[1] - span, v[0]));
  return [t0, t0 + span];
}

/** The axis's minutes across `w` from `x` (`Axis`). */
function axisOf(x, w, t0, t1) {
  const ppm = w / Math.max(60, t1 - t0);
  return { x, w, t0, t1, ppm, xOf: (m) => x + (m - t0) * ppm, minute: (px) => Math.max(0, Math.min(t1 - 1, t0 + (px - x) / ppm)), hours: (every) => Array.from({ length: Math.floor(t1 / 60) - Math.floor((t0 + 59) / 60) + 1 }, (_, k) => Math.floor((t0 + 59) / 60) + k).filter((h) => h % every === 0) };
}

const pickOf = (d) => ({ dir: d.dir, dep: d.dep, at: d.at ?? null });
const isPick = (d, p) => p && p.dir === d.dir && (p.at != null && d.at != null ? p.at === d.at : Math.abs(p.dep - d.dep) < 0.5);

/** The sheet over the planning, while it is open (`plan_edit::sheet`). */
export function PlanEdit({ W, H }) {
  const v = useMore('plan_edit', {});
  const [view, setView] = useState(null);
  const [drag, setDrag] = useState(null);
  const [hover, setHover] = useState(null);
  const [text, setText] = useState('');
  const [leaving, setLeaving] = useState(false);
  const pending = useRef(false);
  const close = () => {
    actMore('pe_close');
    setUi('planning', { edit: null });
  };
  // gone (another company, the line given up): closed
  useEffect(() => {
    if (v?.none) setUi('planning', { edit: null });
  }, [v?.none]);
  // the keys for the departure chosen (`keys`): the arrows a minute (five with Shift), Delete,
  // Escape - and, nothing chosen, Escape closes
  useEffect(() => {
    const k = (e) => {
      const el = document.activeElement;
      if (!v || v.none || drag || (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
      const sel = v.sel;
      if (e.key === 'Escape') {
        e.stopImmediatePropagation();
        if (leaving) setLeaving(false);
        else if (sel) actMore('pe_sel', { pick: null });
        else if (v.dirty) setLeaving(true);
        else close();
      } else if ((e.key === 'ArrowLeft' || e.key === 'ArrowRight') && sel && v.editable) {
        e.preventDefault();
        const step = (e.shiftKey ? 5 : 1) * (e.key === 'ArrowLeft' ? -1 : 1);
        actMore('pe_move', { pick: sel, to: sel.dep + step, what: 'keys' });
      } else if ((e.key === 'Delete' || e.key === 'Backspace') && sel && v.editable) {
        actMore('pe_remove', { pick: sel });
      }
    };
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, [v, drag, leaving]);
  if (!v || v.none || v.error) return null;
  const x = 24;
  const w = W - 48;
  const lanesN = Math.max(1, v.lanes.length);
  const top = 94;
  const footH = 56;
  const bodyH = Math.max(120, H - 18 - footH - top);
  const leftW = Math.floor(w * 0.62);
  const rightX = x + leftW + 22;
  const rightW = w - leftW - 22;
  const whole = v.whole;
  const shownView = view ? fitView(view, whole) : null;
  const [t0, t1] = drag?.span || shownView || whole;
  const ax = axisOf(x + LABEL_W, Math.max(60, leftW - LABEL_W - 6), t0, t1);
  const headY = top + 22;
  const afterLanes = headY + 20 + lanesN * LANE_H;
  const zoomed = !!shownView && !drag;
  let tbY = afterLanes + 2 + (zoomed ? 16 : 0);
  const toursY = v.editable ? tbY + 46 : tbY + 6;
  const every = ax.ppm * 60 >= 40 ? 1 : ax.ppm * 60 >= 20 ? 2 : 3;
  const hours = ax.hours(every);

  // the wheel zooms into the hours under the mouse; Shift (or a sideways wheel) goes along
  const onWheel = (e) => {
    e.preventDefault();
    const r = e.currentTarget.getBoundingClientRect();
    const s = r.width / e.currentTarget.offsetWidth || 1;
    const mx = (e.clientX - r.left) / s + ax.x;
    if ((e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) && shownView) {
      const by = Math.round((e.shiftKey ? e.deltaY : e.deltaX) / 4 / ax.ppm);
      setView(fitView([t0 + by, t1 + by], whole));
      return;
    }
    const m = ax.minute(mx);
    const span = t1 - t0;
    const to = Math.max(120, Math.min(whole[1] - whole[0], span * 0.8 ** (e.deltaY < 0 ? 1 : -1)));
    const a = Math.round(m - ((m - t0) * to) / span);
    setView(fitView([a, a + Math.round(to)], whole));
  };

  // a departure taken (and dragged), or an empty place pressed: a trip added there, or the axis
  // moved along (`axis_lanes`, `drag_step`, `zoom`)
  const laneDown = (e, k, lane) => {
    if (e.button !== 0) return;
    const r = e.currentTarget.getBoundingClientRect();
    const s = r.width / e.currentTarget.offsetWidth || 1;
    const px = (e.clientX - r.left) / s + ax.x;
    const near = lane.deps.filter((d) => Math.abs(ax.xOf(d.dep) - px) <= 7).sort((a, b) => Math.abs(ax.xOf(a.dep) - px) - Math.abs(ax.xOf(b.dep) - px))[0];
    const x0 = e.clientX;
    if (near) {
      actMore('pe_sel', { pick: pickOf(near) });
      setText('');
      if (!v.editable) return;
      const d = { pick: pickOf(near), dep0: near.dep, x0, s, moved: false, span: [t0, t1], shown: near.dep };
      setDrag(d);
      const move = (ev) => {
        const dx = (ev.clientX - x0) / s;
        if (!d.moved && Math.abs(dx) <= 3) return;
        d.moved = true;
        const to = Math.round(Math.max(0, Math.min(t1 - 1, d.dep0 + dx / ax.ppm)));
        d.want = to;
        setDrag({ ...d, shown: to });
        if (pending.current || Math.abs(to - d.pick.dep) < 0.5) return;
        const go = () => {
          if (Math.abs(d.want - d.pick.dep) < 0.5) return;
          pending.current = true;
          const target = d.want;
          actMore('pe_move', { pick: d.pick, to: target, what: 'drag' }).then((r) => {
            pending.current = false;
            if (r?.sel) d.pick = r.sel;
            if (d.want !== target) go();
          });
        };
        go();
      };
      const up = () => {
        removeEventListener('pointermove', move);
        removeEventListener('pointerup', up);
        setDrag(null);
      };
      addEventListener('pointermove', move);
      addEventListener('pointerup', up);
      return;
    }
    const at = Math.round(ax.minute(px));
    let moved = false;
    const span0 = [t0, t1];
    const move = (ev) => {
      const dx = (ev.clientX - x0) / s;
      if (!moved && Math.abs(dx) <= 4) return;
      moved = true;
      if (shownView) {
        const by = Math.round(dx / ax.ppm);
        setView(fitView([span0[0] - by, span0[1] - by], whole));
      }
    };
    const up = () => {
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
      if (!moved && v.editable) actMore('pe_add', { dir: k, at }).then(() => setText(''));
    };
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
  };
  const laneMove = (e, k, lane) => {
    const r = e.currentTarget.getBoundingClientRect();
    const s = r.width / e.currentTarget.offsetWidth || 1;
    const px = (e.clientX - r.left) / s + ax.x;
    const near = lane.deps.find((d) => Math.abs(ax.xOf(d.dep) - px) <= 7);
    setHover({ k, m: Math.round(ax.minute(px)), near: near ? `${near.text} – ${near.until}` : null });
  };

  const days = v.days;
  return html`<div class="pe" onPointerDown=${(e) => e.stopPropagation()}>
    <div class="pe-head" style=${`left:${x}px;top:18px;width:${w - Math.min(380, w * 0.4) - 54}px`}>
      <${Plate} p=${v.plate} h=${30} />
      <div class="words"><b class="cut">${v.title}</b><span class="cut">${v.caption}</span></div>
    </div>
    <div class="pe-seg" style=${`left:${W - 64 - Math.min(380, w * 0.4)}px;top:18px;width:${Math.min(380, w * 0.4)}px`}>
      ${days
        ? html`<${Seg} value=${days.day} options=${days.labels.map((l, k) => [k, l])} off=${days.off} onChange=${(d) => actMore('pe_day', { day: d })} style="width:100%;height:34px" />`
        : html`<span class="date">${v.date}</span>`}
    </div>
    <span class="pe-close" style=${`left:${W - 48}px;top:18px`}><${IconBtn} icon="close" title=${v.close} onClick=${() => (v.dirty ? setLeaving(true) : close())} /></span>
    <p class="pe-note" style=${`left:${x}px;top:62px;width:${w}px;color:${toneColour(v.tone)}`}>${v.note}</p>

    <div class="pe-caps" style=${`left:${x}px;top:${headY - 22}px;width:${150}px`}>${v.departures}</div>
    ${zoomed && html`<${Btn} kind="ghost" icon="zoom_out" label=${v.whole_day} style=${`position:absolute;right:${W - (ax.x + ax.w)}px;top:${headY - 30}px;height:26px`} onClick=${() => setView(null)} />`}
    <span class="pe-hint" style=${`left:${x + 150}px;top:${headY - 24}px;width:${ax.x + ax.w - x - 150 - (zoomed ? 130 : 0)}px`}>${v.hint}</span>
    <div class="pe-axis" style=${`left:${ax.x}px;top:${headY}px;width:${ax.w}px;height:${20 + lanesN * LANE_H}px`} onWheel=${onWheel}>
      ${hours.map((h) => html`<span class="hr" style=${`left:${ax.xOf(h * 60) - ax.x - 16}px`}>${String(h % 24).padStart(2, '0')}</span><i class="hl" style=${`left:${ax.xOf(h * 60) - ax.x}px;height:${lanesN * LANE_H - 8}px`}></i>`)}
      ${v.lanes.map((lane, k) => {
        const ly = 20 + k * LANE_H;
        const colour = laneColour(k);
        const hot = hover && hover.k === k ? hover : null;
        return html`<div class="pe-lane" style=${`top:${ly}px;height:${LANE_H - 8}px`} title=${hot?.near || ''}
            onPointerDown=${(e) => laneDown(e, k, lane)} onPointerMove=${(e) => laneMove(e, k, lane)} onPointerLeave=${() => setHover(null)}>
          ${lane.deps.map((d, i) => {
            const px = ax.xOf(d.on && drag ? drag.shown : d.dep) - ax.x;
            if (px < -2 || px > ax.w + 2) return null;
            const room = (o) => (o ? Math.abs(ax.xOf(o.dep) - ax.xOf(d.dep)) : 1e9);
            const free = Math.min(room(lane.deps[i - 1]), room(lane.deps[i + 1]));
            const ex = Math.min(ax.xOf(d.arr) - ax.x, ax.w);
            return html`${d.on && html`<i class="trip" style=${`left:${px}px;width:${Math.max(2, ex - px)}px;background:${colour}`}></i><i class="chosen" style=${`left:${px - 3}px`}></i>`}
              ${!d.on && html`<i class="dep" style=${`left:${px - 1.5}px;background:${colour};opacity:${d.marked ? 1 : 0.85}`}></i>`}
              ${d.marked && html`<i class="mark" style=${`left:${px - 2.5}px`}></i>`}
              ${(d.on || free >= 38) && html`<span class=${'time' + (d.on ? ' on' : '')} style=${`left:${px - 20}px`}>${d.on && drag ? hm(drag.shown) : d.text}</span>`}`;
          })}
          ${hot && !hot.near && v.editable && !drag && html`<i class="guide" style=${`left:${ax.xOf(hot.m) - ax.x - 1}px;background:${colour}`}></i><span class="add" style=${`left:${ax.xOf(hot.m) - ax.x - 26}px;color:${colour}`}>+ ${hm(hot.m)}</span>`}
        </div>`;
      })}
      ${drag?.moved && html`<span class="pe-dragtag" style=${`left:${ax.xOf(drag.shown) - ax.x}px;top:${20 + drag.pick.dir * LANE_H - 22}px`}>${hm(drag.shown)}${drag.shown !== drag.dep0 ? `  (${drag.shown > drag.dep0 ? '+' : '−'}${Math.abs(drag.shown - drag.dep0)} min)` : ''}</span>`}
    </div>
    ${v.lanes.map((lane, k) => html`<div class="pe-lanehead" style=${`left:${x}px;top:${headY + 20 + k * LANE_H}px;width:${LABEL_W - 8}px`}>
      <i style=${`background:${laneColour(k)}`}></i><span class="word cut">${lane.word.toUpperCase()}</span><b class="cut">${lane.to}</b><span class="count cut">${lane.count}</span></div>`)}
    ${zoomed && html`<div class="pe-bar" style=${`left:${ax.x}px;top:${afterLanes + 2}px;width:${ax.w}px`}><i style=${`left:${((t0 - whole[0]) / (whole[1] - whole[0])) * 100}%;width:${((t1 - t0) / (whole[1] - whole[0])) * 100}%`}></i></div>`}
    ${v.editable && html`<${TripTools} v=${v} r=${{ x, y: tbY, w: leftW, h: 36 }} text=${text} setText=${setText} />`}
    <${Tours} v=${v} ax=${ax} hours=${hours} r=${{ x, y: toursY, w: leftW, h: Math.max(40, top + bodyH - toursY) }} />
    <div class="pe-right pl-scroll" style=${`left:${rightX}px;top:${top}px;width:${rightW}px;height:${bodyH}px`}>
      <${Needs} n=${v.needs} editable=${v.editable} />
      ${v.pattern && html`<hr /><${Pattern} p=${v.pattern} />`}
      <hr /><${List} v=${v} />
    </div>
    <i class="pe-footline" style=${`left:${x}px;top:${H - 18 - 40 - 14}px;width:${w}px`}></i>
    <${Foot} v=${v} r=${{ x, y: H - 18 - 40, w, h: 40 }} leaving=${leaving} setLeaving=${setLeaving} close=${close} />
  </div>`;
}

/** The departure chosen (`trip_tools`): its times, its time typed, moved, copied, taken out. */
function TripTools({ v, r, text, setText }) {
  const t = v.tools;
  if (!t) return html`<div class="pe-tools" style=${`left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`}><span class="none">${v.choose}</span></div>`;
  const typed = (s, enter) => {
    setText(s);
    const m = parseHm(s);
    if (m != null && (wholeTime(s) || enter)) {
      actMore('pe_move', { pick: t.pick, to: typedMinutes(m, t.dep), what: 'typed' });
      setText('');
      document.activeElement?.blur?.();
    }
  };
  return html`<div class="pe-tools" style=${`left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`}>
    <b class="cut">${t.words}</b>
    <input class="co-field" value=${text} placeholder=${t.text} onInput=${(e) => typed(e.currentTarget.value, false)} onKeyDown=${(e) => e.key === 'Enter' && typed(e.currentTarget.value, true)} />
    ${[['-5', -5], ['-1', -1], ['+1', 1], ['+5', 5]].map(([l, m]) => html`<${Btn} kind="ghost" label=${l} style="width:38px;height:28px;padding:0" title=${v.shift_tip} onClick=${() => actMore('pe_move', { pick: t.pick, to: t.dep + m, what: 'shift' })} />`)}
    <${Btn} kind="ghost" label=${v.copy} style="height:28px;margin-left:8px" title=${t.copy_tip} onClick=${() => actMore('pe_copy', { pick: t.pick, at: t.dep + t.headway })} />
    <${IconBtn} icon="delete" title=${v.take_out} size=${16} style="margin-left:6px" onClick=${() => actMore('pe_remove', { pick: t.pick })} />
  </div>`;
}

/** The tours under the departures on the same hours (`tours_chart`). */
function Tours({ v, ax, hours, r }) {
  const rowsH = Math.max(20, r.h - 22);
  const n = v.rows.length;
  const pitch = n ? Math.max(TOUR_H, Math.min(TOUR_MAX, Math.floor(rowsH / n))) : TOUR_H;
  const bar = Math.max(12, Math.min(20, Math.round((pitch - 2) * 0.55)));
  const ink = { kept: 'var(--hub-text)', changed: 'var(--co-warn)', new: 'var(--co-ok)', gone: 'var(--hub-text-dim)' };
  const dot = { changed: 'var(--co-warn)', new: 'var(--co-ok)', gone: 'var(--co-danger-text)' };
  return html`<div class="pe-tours" style=${`left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`}>
    <div class="pe-caps">${v.tours_title}</div>
    ${v.legend && html`<span class="legend">${v.legend.map(([w, t]) => html`<span><i style=${`background:${toneColour(t)}`}></i>${w}</span>`)}</span>`}
    ${!n
      ? html`<p class="none">${v.no_tours}</p>`
      : html`<div class="rows pl-scroll" style=${`top:22px;height:${rowsH}px`}>
          <div style=${`position:relative;height:${n * pitch}px`}>
            ${v.rows.map((row, k) => html`<div class=${'row' + (row.focus ? ' focus' : '')} style=${`top:${k * pitch}px;height:${pitch - 2}px`} title=${row.tip}>
              ${hours.map((h) => html`<i class="hl" style=${`left:${ax.xOf(h * 60) - r.x}px`}></i>`)}
              <b class="cut" style=${`color:${ink[row.fate]}`}>${row.title}</b>
              ${dot[row.fate] && html`<i class="fate" style=${`background:${dot[row.fate]}`}></i>`}
              ${row.bus && html`<span class="bus cut" style=${`color:${row.lose ? 'var(--co-warn)' : 'var(--hub-text-dim)'}`}>${row.bus}</span>`}
              ${row.legs.map(([a, z, lane]) => {
                const xa = Math.max(ax.x, ax.xOf(a)) - r.x;
                const xz = Math.min(ax.x + ax.w, ax.xOf(z)) - r.x;
                if (xz <= xa) return null;
                const hgt = lane == null ? bar * 0.5 : bar;
                const op = row.fate === 'gone' ? 0.3 : lane == null ? 0.5 : 0.9;
                return html`<i class="leg" style=${`left:${xa}px;width:${Math.max(1.5, xz - xa - 1)}px;height:${hgt}px;top:${(pitch - 2 - hgt) / 2}px;background:${laneColour(lane)};opacity:${op}`}></i>`;
              })}
            </div>`)}
          </div>
        </div>`}
  </div>`;
}

/** What the group of days asks of the company now and as changed (`needs_card`). */
function Needs({ n, editable }) {
  return html`<div class="pe-needs">
    <div class="caps3"><span class="pe-caps">${n.caps}</span>${editable && html`<span class="pe-caps r">${n.now}</span><span class="pe-caps r">${n.changed}</span>`}</div>
    ${n.figures.map((f) => html`<div class="fig"><span class="cut">${f.label}</span>${editable ? html`<span class="now">${f.now}</span>` : html`<span></span>`}<b style=${`color:${toneColour(f.tone)}`}>${f.then}</b></div>`)}
    <p style=${`color:${toneColour(n.fleet_tone)}`}>${n.fleet}</p>
    ${n.tours && html`<p class="soft">${n.tours}</p>`}
    ${n.replan && html`<p style="color:var(--co-warn)">${n.replan}</p>`}
  </div>`;
}

/** "In one go": the group of days' pattern (`pattern_card`). */
function Pattern({ p }) {
  if (p.off) return html`<div class="pe-pattern"><div class="pe-caps">${p.caps}</div><p class="dim">${p.off}</p></div>`;
  const pat = p.pattern;
  const set = (patch, what) => actMore('pe_pattern', { pdir: p.pdir, what, pattern: { ...pat, ...patch } });
  const band = (i, patch) => set({ bands: pat.bands.map((b, k) => (k === i ? { ...b, ...patch } : b)) }, 'pe-band');
  // (one direction: whether it runs beside the caps, as `pattern_card` puts it)
  const runs = pat && !p.follow_note && html`<${Switch} checked=${pat.on} label=${p.runs_label} onChange=${(on) => set({ on }, 'pe-on')} />`;
  return html`<div class="pe-pattern">
    ${p.two ? html`<div class="pe-caps">${p.caps}</div>` : html`<div class="capsrow"><span class="pe-caps">${p.caps}</span>${runs}</div>`}
    ${p.two && html`<${Seg} value=${p.pdir} options=${p.dirs.map((d, k) => [k, d])} onChange=${(d) => actMore('pe_pdir', { pdir: d })} style="height:32px;margin-bottom:8px" />`}
    ${p.run != null && html`<${Slider} label=${p.run_label} value=${Math.round(p.run)} min=${1} max=${Math.max(240, Math.round(p.run))} format=${(x) => `${Math.round(x)} min`} onRelease=${(t) => actMore('pe_run', { pdir: p.pdir, total: t })} />`}
    ${p.follows != null && html`<${Switch} checked=${p.follows} label=${p.follow_label} onChange=${(on) => actMore('pe_follow', { follows: on })} />`}
    ${p.follow_note && html`<p class="dim">${p.follow_note}</p>`}
    ${!p.follow_note &&
    pat &&
    html`${p.two && runs}
      ${pat.on &&
      html`<${Seg} value=${pat.bands.length ? 1 : 0} options=${p.modes.map((m, k) => [k, m])} onChange=${(k) => actMore('pe_mode', { pdir: p.pdir, bands: k === 1 })} style="height:32px;margin:6px 0 8px" />
        ${!pat.bands.length
          ? html`<div class="two"><span class="lab">${p.first_label}</span><span class="lab">${p.last_label}</span>
              <${TimeSpin} minutes=${pat.first} h=${34} onChange=${(m) => set({ first: m }, 'pe-first')} /><${TimeSpin} minutes=${pat.last} h=${34} onChange=${(m) => set({ last: m }, 'pe-last')} /></div>
            <${Slider} label=${p.every_label} value=${pat.headway} min=${3} max=${120} format=${(x) => `${Math.round(x)} min`} onRelease=${(x) => set({ headway: x }, 'pe-every')} />`
          : html`${pat.bands.map((b, i) => html`<div class="band">
                <${TimeSpin} minutes=${b.from} h=${32} onChange=${(m) => band(i, { from: m })} /><span>–</span>
                <${TimeSpin} minutes=${b.to >= p.day_end ? 0 : b.to} end h=${32} onChange=${(m) => band(i, { to: m === 0 ? p.day_end : m })} />
                <${IconBtn} icon="close" title=${p.band_x} size=${14} onClick=${() => set({ bands: pat.bands.filter((_, k) => k !== i) }, 'pe-band-x')} />
                <${Slider} label=${p.every_label} value=${b.headway} min=${3} max=${60} format=${(x) => `${Math.round(x)} min`} onRelease=${(x) => band(i, { headway: x })} style="grid-column:1 / -1" />
              </div>`)}
              <${Btn} kind="ghost" icon="add" label=${p.add_band} style="width:100%;height:30px" onClick=${() => {
                const from = Math.min(p.day_end - 60, Math.max(0, ...pat.bands.map((b) => b.to)));
                set({ bands: [...pat.bands, { from, to: Math.min(p.day_end, from + 120), headway: 30, size: null }] }, 'pe-band-add');
              }} />`}
        <${Slider} label=${p.stands_label} value=${pat.layover} min=${0} max=${30} format=${(x) => `${Math.round(x)} min`} onRelease=${(x) => set({ layover: x }, 'pe-layover')} />
        ${p.table_note && html`<p class="dim">${p.table_note}</p>`}`}`}
  </div>`;
}

/** The departures as a list, per direction (`list_card`). */
function List({ v }) {
  return html`<div class="pe-list">
    <div class="pe-caps">${v.list_caps}</div>
    ${v.lanes.map((lane, k) => html`<div class="lhead"><i style=${`background:${laneColour(k)}`}></i><b>${lane.dest ? `${lane.word}  →  ${lane.dest}` : lane.word}</b></div>
      <div class="chips">${lane.deps.map((d) => html`<button type="button" class=${d.on ? 'on' : ''} style=${d.on ? `background:${laneColour(k)}` : ''} title=${`${d.text} – ${d.until}`}
        onClick=${() => actMore('pe_sel', { pick: d.on ? null : pickOf(d) })}>${d.text}</button>`)}</div>`)}
  </div>`;
}

/** The foot (`foot_row`): undo and start again, what is wrong or what a save does, from which day
 * a line in service changes, save and close - and, closing with changes not saved, the question. */
function Foot({ v, r, leaving, setLeaving, close }) {
  const f = v.foot;
  const box = `left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`;
  if (leaving) {
    return html`<div class="pe-foot" style=${box}><b class="ask">${v.leave.ask}</b><span class="grow"></span>
      <${Btn} label=${v.leave.no} style="width:160px;height:40px" onClick=${() => setLeaving(false)} />
      <${Btn} kind="danger" icon="close" label=${v.leave.yes} style="width:180px;height:40px" onClick=${close} /></div>`;
  }
  return html`<div class="pe-foot" style=${box}>
    ${f && html`<${Btn} kind="ghost" icon="history" label=${v.undo} style="height:40px" onClick=${() => actMore('pe_undo')} />
      <${Btn} kind="ghost" icon="restart_alt" label=${v.again} title=${v.again_tip} style="height:40px" onClick=${() => actMore('pe_again')} />
      <span class="text cut" style=${`color:${toneColour(f.tone)}`}>${f.text}</span>`}
    <span class="grow"></span>
    ${f?.later && html`<span class="from"><${IconBtn} icon="chevron_left" size=${16} onClick=${() => f.earlier && actMore('pe_from', { by: -1 })} /><b class="cut">${f.from}</b><${IconBtn} icon="chevron_right" size=${16} onClick=${() => f.laterday && actMore('pe_from', { by: 1 })} /></span>`}
    ${f && html`<${Btn} kind=${f.can ? 'primary' : ''} icon="save" label=${f.save} style="min-width:130px;height:40px" onClick=${() => v.dirty && actMore('pe_save').then((x) => x?.closed && setUi('planning', { edit: null }))} />`}
    <${Btn} label=${v.close} style="width:116px;height:40px" onClick=${() => (v.dirty ? setLeaving(true) : close())} />
  </div>`;
}


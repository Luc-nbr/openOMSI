// The driver's career in the company, as our game-drawn launcher's (`company/career.rs`; Luc
// 2026-10-16, after the approved concept): the driver's card over every part - who, the level
// with its bar of points, four figures -, then the driver's own duties (the coming week as a
// strip, in Casual the duties to take beside them, how it works in three steps), the progress of
// the driver and the company with the licences and what the levels open, the courses, the
// workshop's jobs, the rankings of the map, and the statistics of the trips driven.

import { html, setUi, ui, act, useView, goTab, Icon, Btn, Section, Figure, Plate, IconBtn, Tag, T, fill } from './kit.js';
import { openDutyCard } from './dutycard.js';
import { RepairGame } from './repair.js';
import { Bar } from './overview.js';
import { Card, Empty, Tabs, Track } from './pages-a.js';

export function CareerPage() {
  const st = ui('career', { part: 0 });
  const v = useView('career', st);
  if (!v?.parts) return null;
  const b = v.body;
  const Part = [Duties, Progress, Training, Workshop, Rankings, Statistics][v.part] || Duties;
  return html`<div class="co-pg co-pg-career">
    <${Profile} p=${v.profile} level=${v.level} />
    <div class="co-pg-careertabs"><${Tabs} value=${v.part} options=${v.parts.map((p, k) => [k, p])} onChange=${(k) => setUi('career', { part: k })} /></div>
    <div class="co-pg-careerbody"><${Part} b=${b} /></div>
  </div>`;
}

/** The driver's card: the initials in a ring, the name and rank, the level and its bar, four
 * figures; the company's level on the right. */
function Profile({ p, level }) {
  if (!p) return null;
  if (p.none) {
    return html`<section class="co-pg-card co-pg-profile"><span class="co-pg-avatar"><${Icon} name="person" size=${30} /></span>
      <div class="grow"><b class="co-pg-profilename">${p.none}</b><span class="co-pg-small co-pg-muted">${p.company}</span></div></section>`;
  }
  return html`<section class="co-pg-card co-pg-profile">
    <span class="co-pg-avatar">${p.initials}</span>
    <div class="co-pg-profilemain">
      <div class="co-pg-line"><b class="co-pg-profilename co-pg-cut">${p.name}</b><span class="co-pg-levelchip">${p.level}</span></div>
      <span class="co-pg-small co-pg-muted co-pg-cut">${p.rank}  ·  ${p.company || level}</span>
      <div title=${p.bar.tip}><${Track} frac=${p.bar.frac} tone="accent" h=${8} /></div>
      <span class="co-pg-small co-pg-muted co-pg-cut">${p.bar.label}</span>
    </div>
    <div class="co-pg-profilestats">${p.stats.map(
      (s) => html`<div><span class="co-pg-ministaticon"><${Icon} name=${s.icon} size=${18} /></span><b>${s.value}</b><span>${s.label}</span></div>`,
    )}</div>
  </section>`;
}

/** The driver's duties (`career::duties_part`): the coming week as a strip, the duties to drive,
 * and beside them in Casual the duties to take, else how it works. */
function Duties({ b }) {
  const e = b.empty;
  return html`<div class=${'co-pg-duties' + (b.easy ? ' easy' : '')}>
    <div class="co-pg-stack">
      <${Card} title=${b.week_title} icon="calendar_month">
        <div class="co-pg-weekstrip">${b.week.map(
          (d) => html`<div class=${'co-pg-weekday' + (d.today ? ' today' : '') + (d.weekend ? ' weekend' : '')}>
            <span class="co-pg-weekname">${d.day}</span><span class="co-pg-weekdate">${d.date}</span>
            <div class="co-pg-weekchips">${d.duties.map((c) => html`<span class="co-pg-dutychip" title=${c.tip}><${Plate} p=${c.plate} h=${18} />${c.time}</span>`)}</div>
          </div>`,
        )}</div>
      </${Card}>
      <${Card} title=${b.title} icon="badge">
        ${e
          ? e.reading
            ? html`<p class="co-pg-note">${e.reading}</p>`
            : html`<${Empty} icon=${e.icon} title=${e.title} text=${e.text}>${e.button && html`<${Btn} kind="primary" icon="event" label=${e.button} style="height:38px" onClick=${() => goTab(5)} />`}</${Empty}>`
          : html`<div class="co-pg-list">${b.rows.map(
              (d, k) => html`<div class=${'co-myduty co-pg-myduty' + (k === 0 ? ' next' : '')}>
                <div class="when"><b class="co-pg-cut">${d.date}</b><span class=${k === 0 ? 't-accent' : 't-soft'}>${d.time}</span></div>
                <div class="what"><div><${Plate} p=${d.plate} h=${24} /><span class="co-pg-cut">${d.words}</span>${k === 0 && html`<${Tag} text=${b.up} tone="accent" />`}</div>
                  <span class=${'co-pg-cut t-' + d.bus_tone}><${Icon} name="directions_bus" size=${16} />${d.bus}</span></div>
                ${b.easy && html`<${IconBtn} icon="close" size=${15} title=${b.back_tip} onClick=${() => act('give_back', { duty: d.duty })} />`}
                <${IconBtn} icon="print_receipt" size=${17} title=${b.print} onClick=${() => openDutyCard(d.duty)} />
                <${Btn} kind=${k === 0 ? 'primary' : ''} icon="play_arrow" label=${b.drive} title=${b.drive_tip} style="width:120px;height:38px;padding:0" onClick=${() => act('drive_duty', { duty: d.duty, from: 6 })} />
              </div>`,
            )}</div>`}
      </${Card}>
    </div>
    <${Card} title=${b.side.title} icon=${b.side.steps ? 'help' : 'add'}>
      ${b.side.steps
        ? html`<ol class="co-pg-steplist">${b.side.steps.map((s, k) => html`<li><span class="co-pg-stepno">${k + 1}</span><div><b>${s.title}</b><p>${s.text}</p></div></li>`)}</ol>`
        : b.side.none
          ? html`<${Empty} small icon="event" text=${b.side.none} />`
          : html`<div class="co-pg-list">${b.side.rows.map(
              (d) => html`<div class="co-takerow co-pg-item" style="flex-direction:row;align-items:center"><div class="grow" style="min-width:0"><span class="co-pg-line"><${Plate} p=${d.plate} h=${22} /><b class="co-pg-cut">${d.short}  ·  ${d.time}</b></span><span class="co-pg-itemsub co-pg-cut">${d.words}</span></div>
                <${Btn} icon="add" label=${b.side.take} title=${b.side.take_tip} style="height:32px" onClick=${() => act('take_duty', { duty: d.duty })} /></div>`,
            )}</div>`}
    </${Card}>
  </div>`;
}

/** The progress of the driver and the company (`career::progress_part`). */
function Progress({ b }) {
  const d = b.driver;
  return html`<div style="height:100%;display:flex;flex-direction:column">
    ${b.reached && html`<div class="co-reached"><${Icon} name="emoji_events" size=${24} class="t-warn" /><div class="grow"><b>${b.reached.title}</b><span>${b.reached.sub}</span></div>
      <${Btn} label=${b.reached.button} style="width:116px;height:34px" onClick=${() => act('level_seen')} /></div>`}
    <div style="flex:1;min-height:0;display:grid;grid-template-columns:1fr 1fr;grid-template-rows:222px minmax(0,1fr);gap:12px">
      <${Section} title=${b.driver_title}>
        ${d
          ? html`<${Head} who=${d.name} title=${d.rank} level=${d.level} tone="accent2" bar=${d.bar} facts=${d.facts.map(([a, x]) => [a, x, 'text'])} />`
          : html`<p class="co-para">${b.no_driver}</p>`}
      </${Section}>
      <${Section} title=${b.company_title}>
        <${Head} who=${b.company.name} title=${b.company.title} level=${b.company.level} tone="ok" bar=${b.company.bar} facts=${b.company.facts} />
      </${Section}>
      <${Section} title=${b.licences_title}>
        ${d && html`<div class="co-scroll" style="flex:1;min-height:0">
          ${d.licences.map((x) => html`<div class="co-licence"><${Icon} name=${x.held ? 'check_circle' : 'badge'} size=${18} class=${x.held ? 't-ok' : 't-dim'} /><b class=${x.held ? '' : 't-soft'}>${x.name}</b>
            <span class=${'grow cut t-' + x.tone}>${x.state}</span>
            ${x.button && html`<${Btn} kind=${x.button.act === 'exam_cancel' ? 'ghost' : ''} icon=${x.button.act === 'exam_cancel' ? null : 'badge'} label=${x.button.label} style="height:32px" onClick=${() => act(x.button.act, { class: x.button.class })} />`}</div>`)}
          ${d.last && html`<p class=${'t-' + d.last.tone} style="font-size:10.75px;font-weight:500;margin-top:6px">${d.last.text}</p>`}
          <p class="co-note t-dim" style="margin-top:8px">${d.test}</p></div>`}
      </${Section}>
      <${Section} title=${b.features_title}>
        <div class="co-scroll" style="flex:1;min-height:0">${b.features.map(
          (f) => html`<div class="co-feature"><${Icon} name=${f.icon} size=${17} class=${f.open ? 't-accent2' : 't-faint'} /><span class=${'grow cut ' + (f.open ? '' : 't-dim')}>${f.name}</span><b class=${f.open ? 't-ok' : 't-faint'}>${f.state}</b></div>`,
        )}</div>
      </${Section}>
    </div>
  </div>`;
}

/** The head of the driver's or the company's progress: who, the title and level, the bar, four
 * facts. */
function Head({ who, title, level, tone, bar, facts }) {
  return html`<div class="co-progress">
    <span class="who cut">${who}</span>
    <div class="title"><b class="cut">${title}</b><span class=${'co-tag t-' + tone} style="height:24px">${level}</span></div>
    <div style="margin-top:10px"><${Bar} b=${bar} /></div>
    <div class="facts">${facts.map(([a, x, tone2]) => html`<div><span class="co-caps">${a}</span><b class=${'t-' + (tone2 || 'text')}>${x}</b></div>`)}</div>
  </div>`;
}

/** The driver's own courses and the staff's (`career::training_part`). */
function Training({ b }) {
  return html`<div style="height:100%;display:flex;flex-direction:column;gap:12px">
    <${Section} title=${b.mine_title} style="height:176px;flex:none">
      <div class="co-mycourses">${b.mine.map(
        (k) => html`<div><span class="mark"><${Icon} name=${k.icon} size=${20} /></span><b class="cut">${k.name}</b><span class="terms cut">${k.terms}</span><p class="cut">${k.effect}</p>
          <span class="state">${k.state.button ? html`<${Btn} icon="badge" label=${k.state.button} style="width:136px;height:34px;padding:0 6px" onClick=${() => act('enrol', { kind: k.state.kind })} />` : html`<${T} t=${[k.state.text, k.state.tone]} />`}</span></div>`,
      )}</div>
    </${Section}>
    <${Section} title=${b.staff_title} style="flex:1;min-height:0">
      <div class="co-coursegrid head" style=${`--cols:${b.heads.length}`}><span></span>${b.heads.map((h) => html`<div title=${h.tip}><span><${Icon} name=${h.icon} size=${15} class=${h.open ? 't-accent2' : 't-faint'} /><b class="cut">${h.name}</b></span><span class="cut terms">${h.terms}</span></div>`)}</div>
      ${b.people.length
        ? html`<div class="co-scroll" style="flex:1;min-height:0">${b.people.map(
            (p) => html`<div class="co-coursegrid" style=${`--cols:${b.heads.length}`}><b class="cut">${p.name}</b>${p.cells.map((c) =>
              c.done
                ? html`<span class="t-ok cut"><${Icon} name="check_circle" size=${15} /></span>`
                : c.until
                  ? html`<span class="t-accent2 cut">${c.until}</span>`
                  : c.lock
                    ? html`<span><${Icon} name="lock" size=${14} class="t-faint" /></span>`
                    : c.away
                      ? html`<span class="t-dim">${c.away}</span>`
                      : html`<span><${Btn} kind="ghost" icon="add" label=${c.book} style="height:30px;padding:0 10px" onClick=${() => act('enrol', { kind: c.kind, who: c.who })} /></span>`,
            )}</div>`,
          )}</div>`
        : html`<p class="co-note t-dim">${b.nobody}</p>`}
    </${Section}>
    <div class="co-note cut" style="flex:none;height:18px">${b.foot}</div>
  </div>`;
}

/** The workshop's jobs the driver can do (`career::workshop_part`), and the job he does in
 * their place while it is open (Omsi-Hub 6/6: `repair.js`). */
function Workshop({ b }) {
  if (b.game) return html`<${RepairGame} g=${b.game} />`;
  if (b.locked) return html`<${Section} title=${b.title} style="max-width:760px;height:150px"><p class="co-para">${b.locked}</p></${Section}>`;
  return html`<div style="display:grid;grid-template-columns:62% minmax(0,1fr);gap:12px;grid-template-rows:minmax(0,1fr);height:100%">
    <${Section} title=${b.title}>
      ${b.jobs.length
        ? html`<div class="co-scroll" style="flex:1;min-height:0">${b.jobs.map(
            (j) => html`<div class="co-job"><span class="icon" style=${`color:var(--co-${j.tone === 'accent2' ? 'accent2' : j.tone})`}><${Icon} name=${j.icon} size=${20} /></span>
              <div class="grow"><b class="cut">${j.name}</b><span class="cut">${j.what}</span></div><span class="t-ok save">${j.save}</span>
              <${Btn} icon=${j.can ? null : 'lock'} label=${j.button} title=${j.can ? undefined : j.tip} style="width:138px;height:36px;padding:0 6px" onClick=${() => (j.can ? act('job_start', { vehicle: j.id, kind: j.kind }) : act('job_blocked', { kind: j.kind }))} /></div>`,
          )}</div>`
        : html`<p class="co-para">${b.none}</p>`}
    </${Section}>
    <${Section} title=${b.how_title}>
      <p class="co-note" style="font-size:10.75px">${b.how}</p>
      ${b.courses && html`<${Btn} icon="badge" label=${b.courses} style="height:34px;margin-top:12px;max-width:200px" onClick=${() => setUi('career', { part: 2 })} />`}
      <div class="co-caps" style="margin:18px 0 8px">${b.done_title}</div>
      ${b.done.length ? b.done.map((d) => html`<div class="co-payrow" style="height:24px"><span class="cut" style="font-size:10.75px">${d.text}</span><b class=${'t-' + d.tone} style="font-size:10.75px">${d.amount}</b></div>`) : html`<span class="co-note t-dim">${b.no_job}</span>`}
    </${Section}>
  </div>`;
}

/** A ranking's table (`career::table`). */
function Table({ t }) {
  return html`<div class="co-rank head">${t.heads.map((h) => html`<span>${h}</span>`)}</div>
    <div class="co-scroll" style="flex:1;min-height:0">${t.rows.map(
      (r) => html`<div class=${'co-rank' + (r.you ? ' you' : '')}><span class=${r.place <= 3 ? 'medal m' + r.place : ''}>${r.place}</span><span class="cut">${r.name}</span><span>${r.level}</span><b>${r.points}</b><span>${r.quality}</span><span>${r.count}</span></div>`,
    )}</div>`;
}

/** The rankings of the map's companies and drivers (`career::rankings_part`). */
function Rankings({ b }) {
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:1;min-height:0;display:grid;grid-template-columns:1fr 1fr;gap:12px">
      <${Section} title=${b.companies_title} tools=${html`<span class="t-accent2" style="text-transform:none">${b.companies.place}</span>`}><${Table} t=${b.companies} /></${Section}>
      <${Section} title=${b.drivers_title} tools=${b.drivers && html`<span class="t-accent2" style="text-transform:none">${b.drivers.place}</span>`}>
        ${b.drivers ? html`<${Table} t=${b.drivers} />` : html`<p class="co-note t-dim">${b.no_driver}</p>`}
      </${Section}>
    </div>
    <div class="co-note t-dim cut" style="flex:none;height:22px;line-height:22px;margin-top:8px">${b.foot}</div>
  </div>`;
}

/** Scores as bars (`career::score_chart`). */
function Scores({ values, empty }) {
  if (!values.length) return html`<p class="co-note t-dim" style="flex:1">${empty}</p>`;
  const n = Math.max(values.length, 10);
  return html`<div class="co-scores">
    <div class="axis">${[100, 75, 50, 25, 0].map((v) => html`<span>${v}</span>`)}</div>
    <div class="plot">${[0, 1, 2, 3, 4].map((k) => html`<i class="grid" style=${`top:${k * 25}%`}></i>`)}
      ${values.map(([v, tip], k) => html`<span class="bar" title=${tip} style=${`left:${(k / n) * 100}%;width:${(0.62 / n) * 100}%;height:${Math.max(1, v)}%;background:${fill(v >= 65 ? 'ok' : v >= 40 ? 'warn' : 'danger', 0.85)}`}></span>`)}</div>
  </div>`;
}

/** The statistics of the driver's trips and the company's (`career::statistics_part`). */
function Statistics({ b }) {
  const half = (figs, title, values, empty, foot) => html`<div style="display:flex;flex-direction:column;gap:12px;min-height:0">
    <div style="display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;flex:none">${figs.map((f) => html`<${Figure} f=${f} small />`)}</div>
    <${Section} title=${title} style="flex:1;min-height:0"><${Scores} values=${values} empty=${empty} /><div class="co-note t-dim cut" style="flex:none;margin-top:8px">${foot}</div></${Section}>
  </div>`;
  const m = b.mine;
  const c = b.company;
  return html`<div style="display:grid;grid-template-columns:1fr 1fr;gap:12px;grid-template-rows:minmax(0,1fr);height:100%">
    ${m ? half(m.figures, m.title, m.values, m.empty, m.foot) : html`<p class="co-note t-dim">${b.no_driver}</p>`}
    ${half(c.figures, c.title, c.values, c.empty, c.foot)}
  </div>`;
}

export { goTab };

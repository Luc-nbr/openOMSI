// The company's staff, as our game-drawn launcher's (`company/people.rs`, `people/`): the
// employees as cards (their role, name, wage, what they do now and their four bars) and one of
// them on a page of their own; the week's applicants; the licences, bus types and courses as a
// matrix; the working hours of the staff who do not drive; and the hiring rules.

import { html, useEffect, useState, setCo, setUi, ui, act, useView, useSize, goKey, Icon, Btn, Seg, Chips, Select, Section, Meter, Switch, IconBtn, Slider, Tag } from './kit.js';

// --- the portraits -------------------------------------------------------------------------

const faces = new Map();

/** A staff portrait's picture (`co_portrait`, the JPEG as bytes): asked once, kept. */
function face(n) {
  if (!n) return Promise.resolve(null);
  if (!faces.has(n)) {
    const tauri = window.__TAURI__;
    const p = tauri
      ? tauri.core
          .invoke('call_bytes', { cmd: 'co_portrait', args: { n } })
          .then((b) => URL.createObjectURL(new Blob([b], { type: 'image/jpeg' })))
          .catch(() => null)
      : Promise.resolve(null);
    faces.set(n, p);
  }
  return faces.get(n);
}

/** Someone's portrait (`portraits::draw`): their face, or their initials while it comes. */
export function Portrait({ n, name, size }) {
  const [src, setSrc] = useState(null);
  useEffect(() => {
    let gone = false;
    face(n).then((u) => !gone && setSrc(u));
    return () => {
      gone = true;
    };
  }, [n]);
  const r = Math.min(size * 0.12, 10);
  const initials = String(name || '')
    .split(/\s+/)
    .map((w) => w[0])
    .filter((ch) => ch && ch === ch.toUpperCase() && ch !== ch.toLowerCase())
    .slice(0, 2)
    .join('');
  return html`<span class="co-portrait" style=${`width:${size}px;height:${size}px;border-radius:${r}px;font-size:${(size * 0.32) / 1.303}px`}>
    ${src ? html`<img src=${src} alt="" />` : initials}
  </span>`;
}

// --- the page ------------------------------------------------------------------------------

export function PeoplePage() {
  const st = ui('people', { tab: 0, filter: 0, sort: 0, descending: false, open: null, hire_role: 0, group: 0, first: 0, per: 6, hours_role: 0 });
  const [ref, size] = useSize();
  // (the matrix shows as many columns as the width holds: `people::training`)
  const nameW = Math.max(170, Math.min(240, size.w * 0.2));
  const per = Math.max(1, Math.floor((size.w - nameW - 12) / 150));
  const v = useView('people', { ...st, per }, size.w > 0);
  if (!v?.tabs) return html`<div ref=${ref} style="height:100%"></div>`;
  const tabsW = Math.min(v.tabs.length === 5 ? 900 : 420, size.w);
  const b = v.body;
  let body = null;
  if (b.kind === 'person') body = html`<${Person} p=${b} />`;
  else if (b.kind === 'list') body = html`<${List} l=${b} st=${st} />`;
  else if (b.kind === 'hire') body = html`<${Hire} h=${b} />`;
  else if (b.kind === 'training') body = html`<${Training} t=${b} nameW=${nameW} />`;
  else if (b.kind === 'schedule') body = html`<${Schedule} s=${b} />`;
  else if (b.kind === 'settings') body = html`<${Rules} r=${b} />`;
  return html`<div ref=${ref} style="height:100%;display:flex;flex-direction:column">
    <div style="display:flex;align-items:center;gap:20px;flex:none;height:40px">
      <${Seg} value=${v.tab} options=${v.tabs.map((t, k) => [k, t])} onChange=${(k) => setUi('people', { tab: k, open: null })} style=${`width:${tabsW}px;height:40px`} />
      ${size.w - tabsW > 320 && html`<span class="co-note grow cut" style="text-align:right">${v.info}</span>`}
    </div>
    <div style="flex:1;min-height:0;margin-top:18px">${body}</div>
  </div>`;
}

/** Someone's four bars (`list::needs`): their grade, satisfaction, stress and friendliness. */
function Needs({ needs }) {
  return html`<div class="co-needbars">${needs.map(
    (n) => html`<div title=${n.tip || undefined}><span class=${n.bold ? 'bold' : ''}>${n.label}</span><${Meter} frac=${n.frac} tone=${n.tone} h=${10} /></div>`,
  )}</div>`;
}

/** The employees as cards (`list::draw`). */
function List({ l, st }) {
  const [ref, size] = useSize();
  if (l.empty) {
    return html`<div><p class="co-para" style="max-width:860px;margin-top:4px">${l.empty}</p>
      <${Btn} kind="primary" icon="groups" label=${l.to_applicants} style="height:40px;margin-top:18px" onClick=${() => setUi('people', { tab: 1 })} /></div>`;
  }
  const cols = size.w >= 1500 ? 2 : 1;
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="display:flex;gap:24px;align-items:flex-start;flex:none">
      <div class="grow"><${Chips} value=${l.filter} options=${l.chips.map((c, k) => [k, c])} onChange=${(k) => setUi('people', { filter: k })} /></div>
      <span class="co-note" style="font-weight:500;color:var(--hub-text-dim);line-height:34px">${l.sort_label}</span>
      <span style="display:flex;align-items:center;gap:6px;width:min(260px, 30%)"><${Select} value=${l.sort} options=${l.sorts} onChange=${(k) => setUi('people', { sort: k })} style="flex:1" />
        <${IconBtn} icon=${l.descending ? 'arrow_downward' : 'arrow_upward'} title=${l.dir_tip} onClick=${() => setUi('people', { descending: !l.descending })} /></span>
    </div>
    <div class="co-scroll" ref=${ref} style="flex:1;min-height:0;margin-top:16px">
      <div class="co-staffcards" style=${`grid-template-columns:repeat(${cols}, minmax(0,1fr))`}>
        ${l.cards.map(
          (e) => html`<button type="button" class="co-staffcard" data-person=${e.id} onClick=${() => setUi('people', { open: e.id })}>
            <${Portrait} n=${e.portrait} name=${e.name} size=${88} />
            <div class="who"><div class="role"><${Icon} name=${e.icon} size=${16} class="t-accent2" /><b class="cut">${e.role}</b></div>
              <span class="name cut">${e.name}</span>
              <div class="wage"><b>${e.wage}</b>${e.tag && html`<${Tag} text=${e.tag[0]} tone=${e.tag[1]} />`}</div></div>
            <div class="now"><b class=${'t-' + e.doing[2]}>${e.doing[0]}</b>${e.doing[1] && html`<span>${e.doing[1]}</span>`}</div>
            <${Needs} needs=${e.needs} />
          </button>`,
        )}
      </div>
    </div>
  </div>`;
}

/** Someone's own page (`list::person`): who they are, their needs, their pay, their work and
 * their courses. */
function Person({ p }) {
  const back = () => setUi('people', { open: null });
  useEffect(() => {
    const k = (e) => e.key === 'Escape' && (e.stopImmediatePropagation(), back());
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, []);
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:none"><${Btn} icon="chevron_left" label=${p.back} style="height:36px" onClick=${back} /></div>
    <div class="co-person">
      <section class="co-section side">
        <${Portrait} n=${p.portrait} name=${p.name} size=${240} />
        <b class="name">${p.name}</b>
        <span class="role t-accent2">${p.role}</span>
        <span class="since">${p.since}</span>
        <b class=${'doing t-' + p.doing[2]}>${p.doing[0]}</b>
        ${p.doing[1] && html`<span class="doing2">${p.doing[1]}</span>`}
        ${p.leaves && html`<span class="leaves t-danger">${p.leaves}</span>`}
      </section>
      <div class="grid">
        <${Section} title=${p.needs_title} style="height:300px">
          ${p.needs.map((n) => html`<div class="co-need"><div><b>${n.label}</b><b class=${'t-' + n.tone}>${n.value}</b></div><${Meter} frac=${n.frac} tone=${n.tone} h=${8} /><span>${n.note}</span></div>`)}
        </${Section}>
        <${Section} title=${p.pay_title} style="height:300px">
          ${p.pay.map(([a, b]) => html`<div class="co-payrow"><span>${a}</span><b>${b}</b></div>`)}
          ${p.hourly_note ? html`<p class="co-note t-dim" style="font-size:9.6px;line-height:20px">${p.hourly_note}</p>` : html`<div style="height:20px"></div>`}
          ${p.ask
            ? html`<b class="t-warn" style="font-size:var(--co-rows);line-height:22px;margin-top:8px">${p.ask}</b>
              <div class="pair"><${Btn} kind="primary" icon="check" label=${p.grant} onClick=${() => act('grant_raise', { id: p.id })} /><${Btn} icon="close" label=${p.refuse} onClick=${() => act('refuse_raise', { id: p.id })} /></div>`
            : html`<div class="pair" style="margin-top:8px"><${Btn} icon="trending_up" label=${p.raise} onClick=${() => act('raise', { id: p.id })} />
              ${p.keep && html`<${Btn} icon="restart_alt" label=${p.keep} onClick=${() => act('keep', { id: p.id })} />`}
              ${p.dismiss && html`<${Btn} kind="danger" icon="logout" label=${p.dismiss} onClick=${() => setCo({ dialog: { kind: 'confirm', what: 'dismiss', id: p.id } })} />`}</div>`}
        </${Section}>
        <${Section} title=${p.work_title}>
          ${p.facts.map(([a, b]) => html`<div class="co-payrow"><span class="cut">${a}</span><b class="cut">${b}</b></div>`)}
        </${Section}>
        <${Section} title=${p.courses_title}>
          ${p.courses.map(
            (k) => html`<div class="co-courserow"><span title=${k.tip}><${Icon} name=${k.icon} size=${16} class="t-accent2" /><b>${k.label}</b></span>
              ${k.state.done
                ? html`<${Icon} name="check_circle" size=${17} class="t-ok" />`
                : k.state.until
                  ? html`<span class="co-note t-accent2" style="font-weight:500">${k.state.until}</span>`
                  : k.state.lock
                    ? html`<${Btn} kind="ghost" icon="lock" label=${k.state.lock} style="width:170px;height:34px" onClick=${() => act('course_lock', { kind: k.state.kind })} />`
                    : html`<${Btn} kind="ghost" icon="add" label=${k.state.book} style="width:170px;height:34px" onClick=${() => act('book_course', { id: p.id, kind: k.state.kind })} />`}</div>`,
          )}
          ${p.licences_note && html`<p class="co-note t-dim" style="line-height:22px">${p.licences_note}</p>`}
        </${Section}>
      </div>
    </div>
  </div>`;
}

/** The week's applicants for a role (`hire::draw`). */
function Hire({ h }) {
  const [ref, size] = useSize();
  const cols = Math.max(1, Math.min(4, Math.floor(size.w / 440)));
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:none"><${Chips} value=${h.pick} options=${h.chips.map((c, k) => [k, c])} onChange=${(k) => setUi('people', { hire_role: k })} /></div>
    <div class="co-scroll" ref=${ref} style="flex:1;min-height:0;margin-top:16px">
      ${h.locked
        ? html`<div class="co-locked"><${Icon} name="lock" size=${20} class="t-dim" /><div><p class="co-para">${h.locked.why}</p>
            ${h.locked.more && html`<p class="co-note t-dim" style="margin-top:6px">${h.locked.more}</p>`}
            <${Btn} kind="primary" icon=${h.locked.go.icon} label=${h.locked.go.label} style="height:38px;margin-top:16px" onClick=${() => goKey(h.locked.go.to)} /></div></div>`
        : !h.cards.length
          ? html`<p class="co-para" style="font-weight:500">${h.none}</p>`
          : html`<div class="co-hirecards" style=${`grid-template-columns:repeat(${cols}, minmax(0,1fr))`}>${h.cards.map(
              (a) => html`<div class="co-hirecard">
                <div class="top"><${Portrait} n=${a.portrait} name=${a.name} size=${96} />
                  <div class="who"><b class="cut">${a.name}</b><span class="cut">${a.who}</span>
                    <div class="grade" title=${a.grade_tip}><b class=${'t-' + a.grade_tone}>${a.grade}</b><${Meter} frac=${a.frac} tone=${a.grade_tone} h=${8} /></div>
                    <b class="asks cut">${a.asks}</b><span class="against cut">${a.against}</span></div></div>
                <div class="bring">
                  ${a.bring.licence != null
                    ? html`<b class=${a.bring.d1 ? 't-warn' : ''} title=${a.bring.licence_tip}>${a.bring.licence}</b><span title=${a.bring.skills_tip}>${a.bring.skills}</span>`
                    : html`<span>${a.bring.friendliness}</span><b class=${'t-' + a.bring.tone}>${a.bring.value}</b>`}
                </div>
                <span class="reliable" title=${a.reliable_tip}>${a.reliable}</span>
                <${Btn} kind="primary" icon="person_add" label=${h.hire} style="position:absolute;right:16px;bottom:14px;width:120px;height:38px" onClick=${() => act('hire', { no: a.no })} />
              </div>`,
            )}</div>`}
    </div>
    <div class="co-note cut" style="flex:none;height:30px;line-height:30px;margin-top:10px">${h.foot}</div>
  </div>`;
}

/** The licences, bus types or courses as a matrix (`people::training`). */
function Training({ t, nameW }) {
  if (t.nobody) return html`<p class="co-para" style="max-width:860px;margin-top:4px">${t.nobody}</p>`;
  const train = (k, id) => act('train', { group: t.group, col: t.first + k, ...(id != null ? { id } : {}) });
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:1;min-height:0;display:flex;flex-direction:column">
      <${Seg} value=${t.group} options=${t.groups.map((g, k) => [k, g])} onChange=${(k) => setUi('people', { group: k, first: 0 })} style="width:min(420px,100%);height:34px;flex:none" />
      ${t.none
        ? html`<p class="co-para" style="margin-top:16px">${t.none}</p>`
        : html`<div class="co-matrix" style=${`--name:${nameW}px;--cols:${t.heads.length}`}>
            <div class="head">
              <div class="namecol"><div class="co-caps">${t.driver}</div>
                ${t.paging && html`<div class="paging"><span>${t.paging}</span>
                  ${t.first > 0 && html`<${IconBtn} icon="chevron_left" size=${15} title=${t.prev_tip} onClick=${() => setUi('people', { first: Math.max(0, t.first - t.per) })} />`}
                  ${t.first + t.per < t.total && html`<${IconBtn} icon="chevron_right" size=${15} title=${t.next_tip} onClick=${() => setUi('people', { first: Math.min(t.total - t.per, t.first + t.per) })} />`}</div>`}</div>
              ${t.heads.map(
                (h) => html`<div class="colhead"><div title=${h.tip}><span class="title"><${Icon} name=${h.icon} size=${15} class=${h.open ? 't-accent2' : 't-faint'} /><b class="cut">${h.title}</b></span>
                  <span class="cut">${h.under}</span><span class=${'cut ' + (h.all ? 't-ok' : 't-soft')} style="font-weight:500">${h.count}</span></div>
                  ${h.lock
                    ? html`<${Btn} kind="ghost" icon="lock" label=${h.lock} style="height:28px;max-width:140px;padding:0 8px" onClick=${() => act('course_lock', { kind: h.kind })} />`
                    : h.train_all && html`<${Btn} icon="groups" label=${h.train_all} style="height:28px;max-width:140px;padding:0 8px" onClick=${() => train(h.k)} />`}</div>`,
              )}
            </div>
            <div class="co-scroll rows">${t.rows.map(
              (r) => html`<div class="row"><span class="namecol"><b class="cut">${r.name}</b><span>${r.licence}</span></span>
                ${r.cells.map((c, k) =>
                  c.has
                    ? html`<span class="cell"><${Icon} name="check_circle" size=${15} class="t-ok" /></span>`
                    : c.until
                      ? html`<span class="cell t-accent2"><${Icon} name="schedule" size=${15} /><span class="cut" style="font-weight:500">${c.until}</span></span>`
                      : c.lock
                        ? html`<span class="cell"><${Icon} name="lock" size=${14} class="t-faint" /></span>`
                        : c.text
                          ? html`<span class="cell t-dim">${c.text}</span>`
                          : html`<span class="cell"><${Btn} kind="ghost" icon="add" label=${c.book} style="height:30px;max-width:110px;padding:0 10px" onClick=${() => train(k, r.id)} /></span>`,
                )}</div>`,
            )}</div>
          </div>`}
    </div>
    <div class="co-note cut" style="flex:none;height:30px;line-height:30px;margin-top:14px">${t.foot}</div>
  </div>`;
}

const hhmm = (m) => `${String(Math.floor(m / 60) % 24).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;

/** The working hours of a role that does not drive (`hours::schedule`). */
function Schedule({ s }) {
  const set = (patch) => act('set_hours', { role: s.pick, from: s.from, to: s.to, outside: s.outside, ...patch });
  const time = (value, key) =>
    s.open
      ? html`<input type="time" class="co-field co-timefield" step="900" value=${hhmm(value)} onChange=${(e) => {
          const [h, m] = e.currentTarget.value.split(':').map(Number);
          if (!isNaN(h)) set({ [key]: h * 60 + (m || 0) });
        }} />`
      : html`<span class="co-timefield still">${hhmm(value)}</span>`;
  return html`<div style="height:100%;display:flex;flex-direction:column">
    <div style="flex:none"><${Chips} value=${s.pick} options=${s.chips.map((c, k) => [k, c])} onChange=${(k) => setUi('people', { hours_role: k })} /></div>
    <div class="co-hours">
      <div class="left">
        <b class="title">${s.core}</b>
        <div class="times">${time(s.from, 'from')}<span>–</span>${time(s.to, 'to')}</div>
        <p class="co-para" style="text-align:center">${s.all_there}</p>
        <b class="title" style="margin-top:28px">${s.outside_title}</b>
        <div style="width:60%;margin:12px auto 0">${s.open
          ? html`<${Slider} value=${s.outside} min=${0} max=${s.most} onChange=${(n) => set({ outside: n })} />`
          : html`<div style="text-align:center;font-size:16.9px;font-weight:700;color:var(--hub-text-soft)">${s.outside}</div>`}</div>
        <div class="notes">${s.notes.map((n, k) => html`<p class=${k === 1 ? '' : 't-soft'}>${n}</p>`)}</div>
        ${s.locked && html`<div class="co-locked" style="margin-top:12px"><${Icon} name="lock" size=${18} class="t-warn" /><div><p class="co-para t-warn" style="font-weight:500">${s.locked.text}</p>
          ${s.locked.more && html`<p class="co-note t-dim" style="margin-top:6px">${s.locked.more}</p>`}
          <${Btn} icon=${s.locked.go.icon} label=${s.locked.go.label} style="height:36px;margin-top:10px" onClick=${() => goKey(s.locked.go.to)} /></div></div>`}
      </div>
      <div class="right">
        <${Section} title=${s.today_title} style="flex:none;max-height:360px">
          ${s.rows.map(([a, b]) => html`<div class="co-payrow"><span class="cut">${a}</span><b>${b}</b></div>`)}
        </${Section}>
        ${s.each.length > 0 && html`<div class="co-caps" style="margin:16px 0 10px">${s.each_title}</div>
          <div class="co-scroll" style="flex:1;min-height:0">${s.each.map(([name, hours, stays]) => html`<div class="co-eachrow"><b class="cut">${name}</b><span class=${stays ? 't-warn' : 't-soft'}>${hours}</span></div>`)}</div>`}
      </div>
    </div>
  </div>`;
}

/** The hiring rules (`hours::settings`). */
function Rules({ r }) {
  return html`<div class="co-rules">
    <${Switch} checked=${r.auto} label=${r.auto_label} onChange=${(on) => act('staff_rule', { which: 'auto', on })} style="min-height:36px" />
    <p class="co-note t-dim">${r.auto_note}</p>
    ${r.roles.map(
      (x) => html`<div class=${'minrow' + (r.auto ? '' : ' off')}><${Icon} name=${x.icon} size=${16} /><span class="cut">${x.name}</span>
        <${IconBtn} icon="remove" size=${15} title=${r.fewer} onClick=${() => x.n > 0 && act('staff_minimum', { role: x.role, n: x.n - 1 })} />
        <b>${x.n}</b>
        <${IconBtn} icon="add" size=${15} title=${r.more} onClick=${() => x.n < 99 && act('staff_minimum', { role: x.role, n: x.n + 1 })} />
        <span class="now">${x.now}</span></div>`,
    )}
    <div style="height:1px;background:var(--hub-hairline);margin:14px 0"></div>
    <${Switch} checked=${r.follow} label=${r.follow_label} onChange=${(on) => act('staff_rule', { which: 'follow', on })} style="min-height:36px" />
    <p class="co-note t-dim">${r.follow_note}</p>
    <${Switch} checked=${r.grant} label=${r.grant_label} onChange=${(on) => act('staff_rule', { which: 'grant', on })} style="min-height:36px;margin-top:10px" />
    <p class="co-note t-dim">${r.grant_note}</p>
  </div>`;
}

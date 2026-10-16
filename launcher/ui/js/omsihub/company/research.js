// The company's research (`company_more/research.rs`; the rules are `company::research`'s), after
// Luc's reference: the title and a line under it, the cash, the trophies and the rating beside
// it; the research under way in a wide tinted card - its round icon, its name, a thick progress
// bar, the days left and the researchers, pausing and stopping it (stopping clicked twice: it
// costs) -; the sections with a compact tile a topic - its round icon on its section's colour,
// what it does in a line, its price and its time as chips, and its state as the tile's foot:
// researched (a green tile), available (Start) or locked (what it needs) -; and beside them
// (under them on a narrow page) the details of the topic picked, closed with its ×.

import { html, useState, useEffect, ui, setUi, Icon, Btn } from './kit.js';
import { useMore, actMore } from './more.js';
import { Card, Chip, Bar } from './pagesb.js';

const rgb = (c) => (Array.isArray(c) ? c.join(',') : '120,128,144');

export function ResearchPage() {
  const v = useMore('research');
  const st = ui('research', { pick: null, closed: false });
  if (!v?.sections) return html`<div class="co-pgb"></div>`;
  const cards = v.sections.flatMap((s) => s.cards);
  const pick = cards.find((t) => t.key === st.pick) || cards.find((t) => t.key === v.banner?.key) || cards.find((t) => t.state === 'open') || cards[0];
  const choose = (key) => setUi('research', { pick: key, closed: false });
  const open = pick && !st.closed;
  return html`<div class="co-pgb co-pgb-research">
    <div class="co-pgb-stack">
      <div class="co-pgb-head">
        <span class="co-pgb-rs-hic"><${Icon} name="school" size=${26} /></span>
        <span class="co-pgb-names"><h2 class="co-pgb-h1">${v.title}</h2><span class="co-pgb-sub">${v.sub}</span></span>
        <span class="co-pgb-tools">${(v.strip || []).map((p) => html`<${Chip} icon=${p.icon} text=${p.value} tone=${p.tone === 'danger' ? 'danger' : p.tone} title=${p.tip} />`)}</span>
      </div>
      <${Current} b=${v.banner} w=${v.words || {}} onPick=${choose} />
      <div class=${'co-pgb-rs-body' + (open ? '' : ' wide')}>
        <div class="co-pgb-rs-tree">
          ${v.sections.map((sec) => html`<section class="co-pgb-rs-sec">
            <div class="co-pgb-rs-sec-h" style=${`--sec:${rgb(sec.colour)}`}>
              <span class="co-pgb-rs-sec-ic"><${Icon} name=${sec.motif} size=${15} /></span><b class="co-pgb-cut">${sec.title}</b><span class="co-pgb-aside">${sec.count}</span>
            </div>
            <div class="co-pgb-rs-grid">${sec.cards.map((t) => html`<${Tile} t=${t} w=${v.words || {}} on=${open && pick?.key === t.key} onPick=${choose} />`)}</div>
          </section>`)}
        </div>
        ${open && html`<aside class="co-pgb-rs-side"><${Detail} t=${pick} v=${v} onClose=${() => setUi('research', { closed: true })} /></aside>`}
      </div>
    </div>
  </div>`;
}

/** The research under way (`research::banner`): stopping it asks a second click within a few
 * seconds; pausing keeps the work. */
function Current({ b, w, onPick }) {
  const [armed, setArmed] = useState(0);
  useEffect(() => {
    if (!armed) return undefined;
    const t = setTimeout(() => setArmed(0), Math.max(0, armed - Date.now()));
    return () => clearTimeout(t);
  }, [armed]);
  if (!b) return null;
  if (b.none) {
    return html`<section class="co-pgb-card co-pgb-rs-now none">
      <span class="co-pgb-rs-now-ic"><${Icon} name="search" size=${22} /></span>
      <span class="co-pgb-grow"><span class="co-pgb-caps">${w.now}</span><b class="co-pgb-rs-now-name">${b.none}</b><span class=${'co-pgb-note' + (b.staff_tone === 'warn' ? ' co-pgb-t-warn' : '')}>${b.staff}</span></span>
    </section>`;
  }
  const on = armed > Date.now();
  const stop = () => {
    if (on) {
      setArmed(0);
      actMore('research_stop');
    } else setArmed(Date.now() + 4000);
  };
  return html`<section class=${'co-pgb-card co-pgb-rs-now' + (b.paused ? ' paused' : '')} title=${b.line}>
    <button type="button" class="co-pgb-rs-now-ic" style=${`--sec:${rgb(b.colour)}`} onClick=${() => onPick(b.key)}><${Icon} name=${b.icon} size=${26} /></button>
    <div class="co-pgb-grow co-pgb-rs-now-mid">
      <span class="co-pgb-caps">${w.now}${b.stage ? ' · ' + b.stage : ''}</span>
      <div class="co-pgb-row"><b class="co-pgb-rs-now-name co-pgb-cut" title=${b.what}>${b.name}</b><b class="co-pgb-rs-now-pct co-pgb-num">${b.pct}</b></div>
      <div class="co-pgb-rs-now-bar"><${Bar} frac=${b.share} tone=${b.paused ? 'warn' : 'accent'} /></div>
      <span class=${'co-pgb-note' + (b.staff_tone === 'warn' || b.paused ? ' co-pgb-t-warn' : '')} title=${b.staff}>${b.days} · ${b.crew}${b.effect ? ' · ' : ''}${b.effect && html`<span class="co-pgb-t-accent">${b.effect}</span>`}</span>
    </div>
    <div class="co-pgb-rs-now-acts">
      <${Btn} class="co-pgb-outline" icon=${b.paused ? 'play_arrow' : 'pause'} label=${b.paused ? b.resume : b.pause} title=${b.pause_tip} onClick=${() => actMore('research_pause', { on: !b.paused })} />
      <${Btn} kind=${on ? 'danger' : ''} icon="stop_circle" label=${on ? b.again : b.stop} title=${b.stop_tip} onClick=${stop} />
    </div>
  </section>`;
}

/** A topic's tile (`research::card`): its icon, title and effect, its price and time, its state. */
function Tile({ t, w, on, onPick }) {
  const start = (e) => {
    e.stopPropagation();
    actMore('research_start', { key: t.key });
  };
  let foot;
  if (t.state === 'done') foot = html`<span class="co-pgb-rs-done"><${Icon} name="check" size=${14} />${t.said}</span>`;
  else if (t.state === 'running') foot = html`<span class="co-pgb-rs-run"><${Bar} frac=${t.frac} thin=${true} /><span class="co-pgb-t-accent">${t.said}</span></span>`;
  else if (t.start) foot = html`<button type="button" class=${'co-pgb-rs-go' + (t.primary ? ' on' : '')} title=${t.start} onClick=${start}><${Icon} name="play_arrow" size=${14} />${w.available} · ${t.start}</button>`;
  else foot = html`<span class="co-pgb-rs-lock" title=${t.needs}><${Icon} name="lock" size=${13} /><span class="co-pgb-cut">${(w.locked_needs || '%{what}').replace('%{what}', (t.needs_list || []).join(', '))}</span></span>`;
  return html`<div role="button" tabindex="0" class=${'co-pgb-rs-tile ' + t.state + (on ? ' on' : '')} style=${`--sec:${rgb(t.colour)}`}
      onClick=${() => onPick(t.key)} onKeyDown=${(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), onPick(t.key))}>
    ${t.state === 'done' && html`<span class="co-pgb-rs-tick"><${Icon} name="check_circle" size=${16} /></span>`}
    <div class="co-pgb-row co-pgb-rs-tile-h">
      <span class="co-pgb-rs-tic"><${Icon} name=${t.state === 'locked' ? 'lock' : t.icon} size=${16} /></span>
      <b class="co-pgb-rs-tile-t" title=${t.title}>${t.title}</b>
    </div>
    ${t.effect && html`<span class="co-pgb-rs-eff co-pgb-cut" title=${t.effect}>${t.effect}</span>`}
    ${t.state !== 'done' && html`<div class="co-pgb-chips">
      <${Chip} icon="payments" text=${t.money} tone=${t.money_tone === 'danger' ? 'danger' : ''} />
      ${t.trophies && t.trophies !== '0' && html`<${Chip} icon="emoji_events" text=${t.trophies} tone=${t.trophies_tone === 'danger' ? 'danger' : 'warn'} />`}
      <${Chip} icon="schedule" text=${t.work} />
      ${t.stage && html`<${Chip} text=${t.stage} />`}
    </div>`}
    <div class="co-pgb-rs-foot">${foot}</div>
  </div>`;
}

/** A row of the details: its icon, its label and what it says. */
function Fact({ icon, label, children, tone = '' }) {
  return html`<div class=${'co-pgb-rs-fact ' + tone}><span class="co-pgb-rs-fact-ic"><${Icon} name=${icon} size=${16} /></span><span class="co-pgb-grow"><span class="co-pgb-note">${label}</span>${children}</span></div>`;
}

/** The details of the topic picked: its state, what it does, its next stage's price, its time,
 * the researchers, what it waits for, and its button. */
function Detail({ t, v, onClose }) {
  const w = v.words || {};
  const dots = Array.from({ length: t.stages }, (_, k) => k < t.done);
  const pill = { done: ['ok', w.done], running: ['accent', w.running], open: ['accent', w.available], locked: ['dim', w.locked] }[t.state] || ['', ''];
  const info = { done: w.info_done, running: w.info_running, open: w.info_open, locked: w.info_locked }[t.state];
  return html`<${Card} class="co-pgb-rs-detail">
    <div class="co-pgb-row co-pgb-rs-det-h">
      <span class="co-pgb-rs-tic big" style=${`--sec:${rgb(t.colour)}`}><${Icon} name=${t.icon} size=${24} /></span>
      <span class="co-pgb-grow"><b class="co-pgb-rs-det-t">${t.title}</b><span class="co-pgb-note">${t.section}</span><span class=${'co-pgb-chip ' + pill[0]}>${pill[1]}</span></span>
      <button type="button" class="co-pgb-round" title=${w.close} onClick=${onClose}><${Icon} name="close" size=${16} /></button>
    </div>
    ${t.stages > 1 && html`<div class="co-pgb-rs-dots">${dots.map((d) => html`<i class=${d ? 'on' : ''}></i>`)}<span class="co-pgb-note">${t.done} / ${t.stages}</span></div>`}
    <div class="co-pgb-rs-facts">
      <${Fact} icon="bolt" label=${w.effect} tone="big">${t.effect && html`<b class="co-pgb-rs-det-eff">${t.effect}</b>`}<span class="co-pgb-text">${t.info}</span></${Fact}>
      ${t.state !== 'done' && t.money && html`<${Fact} icon="payments" label=${w.cost + (t.stage ? ' · ' + t.stage : '')}><b class=${t.money_tone === 'danger' ? 'co-pgb-t-danger' : ''}>${t.money}</b>${t.trophies && t.trophies !== '0' && html`<span class=${'co-pgb-note' + (t.trophies_tone === 'danger' ? ' co-pgb-t-danger' : '')}><${Icon} name="emoji_events" size=${12} /> ${t.trophies}</span>`}</${Fact}>`}
      ${t.state !== 'done' && t.work && html`<${Fact} icon="schedule" label=${w.duration}><b>${t.work}</b><span class="co-pgb-note">${t.work_all}</span></${Fact}>`}
      ${t.state === 'running' && html`<${Fact} icon="schedule" label=${w.duration}><${Bar} frac=${t.frac} /><span class="co-pgb-note">${t.said} · ${t.left}</span></${Fact}>`}
      ${t.state !== 'done' && v.banner?.crew && html`<${Fact} icon="groups" label=${w.researchers}><b>${v.banner.crew}</b></${Fact}>`}
      <${Fact} icon=${t.needs_list?.length ? 'lock' : 'check_circle'} label=${w.needs} tone=${t.needs_list?.length ? '' : 'ok'}>
        ${t.needs_list?.length ? t.needs_list.map((n) => html`<b class="co-pgb-rs-need">${n}</b>`) : html`<b>${w.none}</b>`}
      </${Fact}>
    </div>
    ${info && html`<div class=${'co-pgb-rs-infobox ' + pill[0]}><${Icon} name="info" size=${15} /><span>${info}</span></div>`}
    ${t.start && html`<${Btn} kind="primary" class="co-pgb-big" icon="play_arrow" label=${t.start} onClick=${() => actMore('research_start', { key: t.key })} />`}
  </${Card}>`;
}

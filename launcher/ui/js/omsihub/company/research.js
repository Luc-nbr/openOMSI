// The company's research (`company_more/research.rs`; the rules are `company::research`'s): the
// cash, the trophies and the rating in the head; the research under way in a wide card - its
// icon, name, stage, progress, the days left and the researchers, pausing and stopping it
// (stopping clicked twice: it costs) -; the sections with a compact tile a topic - its icon on
// its section's colour, what it does in a line, its price and its time as chips, and whether it
// is researched, can start or waits for something -; and beside them (under them on a narrow
// page) the details of the topic picked.

import { html, useState, useEffect, ui, setUi, Icon, Btn } from './kit.js';
import { useMore, actMore } from './more.js';
import { Card, Chip, Bar, Empty } from './pagesb.js';

const rgb = (c) => (Array.isArray(c) ? c.join(',') : '120,128,144');

export function ResearchPage() {
  const v = useMore('research');
  const st = ui('research', { pick: null });
  if (!v?.sections) return html`<div class="co-pgb"></div>`;
  const cards = v.sections.flatMap((s) => s.cards);
  const pick = cards.find((t) => t.key === st.pick) || cards.find((t) => t.key === v.banner?.key) || cards.find((t) => t.state === 'open') || cards[0];
  const choose = (key) => setUi('research', { pick: key });
  return html`<div class="co-pgb co-pgb-research">
    <div class="co-pgb-stack">
      <div class="co-pgb-head">
        <span class="co-pgb-names"><h2 class="co-pgb-h1">${v.title}</h2></span>
        <span class="co-pgb-tools">${(v.strip || []).map((p) => html`<${Chip} icon=${p.icon} text=${p.value} tone=${p.tone === 'danger' ? 'danger' : p.tone} title=${p.tip} />`)}</span>
      </div>
      <${Current} b=${v.banner} onPick=${choose} />
      <div class="co-pgb-rs-body">
        <div class="co-pgb-rs-tree">
          ${v.sections.map((sec) => html`<section class="co-pgb-rs-sec">
            <div class="co-pgb-rs-sec-h" style=${`--sec:${rgb(sec.colour)}`}>
              <span class="co-pgb-rs-sec-ic"><${Icon} name=${sec.motif} size=${15} /></span><b class="co-pgb-cut">${sec.title}</b><span class="co-pgb-aside">${sec.count}</span>
            </div>
            <div class="co-pgb-rs-grid">${sec.cards.map((t) => html`<${Tile} t=${t} on=${pick?.key === t.key} onPick=${choose} />`)}</div>
          </section>`)}
        </div>
        <aside class="co-pgb-rs-side">${pick && html`<${Detail} t=${pick} v=${v} />`}</aside>
      </div>
    </div>
  </div>`;
}

/** The research under way (`research::banner`): stopping it asks a second click within a few
 * seconds; pausing keeps the work. */
function Current({ b, onPick }) {
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
      <span class="co-pgb-grow"><b class="co-pgb-rs-now-name">${b.none}</b><span class=${'co-pgb-note' + (b.staff_tone === 'warn' ? ' co-pgb-t-warn' : '')}>${b.staff}</span></span>
    </section>`;
  }
  const on = armed > Date.now();
  const stop = () => {
    if (on) {
      setArmed(0);
      actMore('research_stop');
    } else setArmed(Date.now() + 4000);
  };
  return html`<section class=${'co-pgb-card co-pgb-rs-now' + (b.paused ? ' paused' : '')}>
    <button type="button" class="co-pgb-rs-now-ic" style=${`--sec:${rgb(b.colour)}`} onClick=${() => onPick(b.key)}><${Icon} name=${b.icon} size=${24} /></button>
    <div class="co-pgb-grow co-pgb-rs-now-mid">
      <div class="co-pgb-row"><span class="co-pgb-caps">${b.stage || ''}</span></div>
      <div class="co-pgb-row"><b class="co-pgb-rs-now-name co-pgb-cut" title=${b.what}>${b.name}</b><b class="co-pgb-rs-now-pct co-pgb-num">${b.pct}</b></div>
      <${Bar} frac=${b.share} tone=${b.paused ? 'warn' : b.tone} />
      <div class="co-pgb-chips">
        <${Chip} icon=${b.paused ? 'pause' : 'schedule'} text=${b.days} tone=${b.paused ? 'warn' : ''} />
        <${Chip} icon="groups" text=${b.crew} tone=${b.staff_tone === 'warn' ? 'warn' : ''} title=${b.staff} />
        <${Chip} icon="bolt" text=${b.effect} tone="accent" />
      </div>
    </div>
    <div class="co-pgb-rs-now-acts">
      <${Btn} icon=${b.paused ? 'play_arrow' : 'pause'} label=${b.paused ? b.resume : b.pause} title=${b.pause_tip} onClick=${() => actMore('research_pause', { on: !b.paused })} />
      <${Btn} kind=${on ? 'danger' : ''} icon="stop_circle" label=${on ? b.again : b.stop} title=${b.stop_tip} onClick=${stop} />
    </div>
  </section>`;
}

/** A topic's tile (`research::card`): its icon, title and effect, its price and time, its state. */
function Tile({ t, on, onPick }) {
  const start = (e) => {
    e.stopPropagation();
    actMore('research_start', { key: t.key });
  };
  let foot;
  if (t.state === 'done') foot = html`<span class="co-pgb-rs-done"><${Icon} name="check_circle" size=${15} />${t.said}</span>`;
  else if (t.state === 'running') foot = html`<span class="co-pgb-rs-run"><${Bar} frac=${t.frac} thin=${true} /><span class="co-pgb-t-accent">${t.said}</span></span>`;
  else if (t.start) foot = html`<${Btn} small=${true} kind=${t.primary ? 'primary' : ''} icon="play_arrow" label=${t.start} onClick=${start} />`;
  else foot = html`<span class="co-pgb-rs-lock" title=${t.needs}><${Icon} name="lock" size=${13} /><span class="co-pgb-cut">${(t.needs_list || []).join(', ')}</span></span>`;
  return html`<div role="button" tabindex="0" class=${'co-pgb-rs-tile ' + t.state + (on ? ' on' : '')} style=${`--sec:${rgb(t.colour)}`}
      onClick=${() => onPick(t.key)} onKeyDown=${(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), onPick(t.key))}>
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

/** The details of the topic picked: what it does, its stages, its next stage's price and what it
 * waits for, and its button. */
function Detail({ t, v }) {
  const w = v.words || {};
  const dots = Array.from({ length: t.stages }, (_, k) => k < t.done);
  return html`<${Card} class="co-pgb-rs-detail" icon="info" title=${v.detail_caps}>
    <div class="co-pgb-row co-pgb-rs-det-h">
      <span class="co-pgb-rs-tic big" style=${`--sec:${rgb(t.colour)}`}><${Icon} name=${t.icon} size=${24} /></span>
      <span class="co-pgb-grow"><b class="co-pgb-rs-det-t">${t.title}</b><span class="co-pgb-note">${t.section}</span></span>
    </div>
    ${t.stages > 1 && html`<div class="co-pgb-rs-dots">${dots.map((d) => html`<i class=${d ? 'on' : ''}></i>`)}<span class="co-pgb-note">${t.done} / ${t.stages}</span></div>`}
    <p class="co-pgb-text">${t.info}</p>
    ${t.state === 'done'
      ? html`<${Empty} ok=${true} text=${w.done} />`
      : html`<div class="co-pgb-rs-cost">
          <span class="co-pgb-caps">${v.cost_caps}${t.stage ? ' · ' + t.stage : ''}</span>
          <div class="co-pgb-rs-kv"><span>${w.money}</span><b class=${t.money_tone === 'danger' ? 'co-pgb-t-danger' : ''}>${t.money}</b></div>
          <div class="co-pgb-rs-kv"><span>${w.trophies}</span><b class=${t.trophies_tone === 'danger' ? 'co-pgb-t-danger' : ''}>${t.trophies}</b></div>
          <div class="co-pgb-rs-kv"><span>${w.work}</span><b>${t.work_all}</b></div>
        </div>`}
    ${t.needs_list?.length > 0 && html`<div class="co-pgb-rs-cost"><span class="co-pgb-caps">${v.needs_caps}</span>
      ${t.needs_list.map((n) => html`<div class="co-pgb-rs-need"><${Icon} name="lock" size=${13} />${n}</div>`)}</div>`}
    ${t.state === 'running' && html`<div class="co-pgb-rs-run big"><${Bar} frac=${t.frac} /><span class="co-pgb-t-accent">${t.said}</span><span class="co-pgb-note">${t.left}</span></div>`}
    ${t.start && html`<${Btn} kind="primary" class="co-pgb-big" icon="play_arrow" label=${t.start} onClick=${() => actMore('research_start', { key: t.key })} />`}
  </${Card}>`;
}

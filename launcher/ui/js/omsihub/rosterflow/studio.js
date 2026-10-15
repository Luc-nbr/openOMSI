// RosterFlow's Vehicles and Drivers tabs (`rosterflow::studio_page` of our game-drawn launcher):
// the KPI tiles (opened to four rows with the histograms), the toolbar (SAVE, DISCARD, Stats,
// Layers, Filter ... OPTIMIZE), Optimise's proposal over the Gantt, the Gantt (`gantt.js`), the
// status bar (what is assigned, the Global, Duty and Vehicle issues, the zoom), what lies over
// them (`panels.js`, `prefs.js`), and the keys.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { css } from './pal.js';
import { setRf, rfAct, letGo, toggle, panelIs, NO_FILTER, DRIVERS, edit } from './state.js';
import { Gantt, rowsOf, arrowKey } from './gantt.js';
import { Icon, RfButton, IconBtn, ToolLink } from './ui.js';
import { Wordmark } from './brand.js';
import { Overlays } from './panels.js';
import { PrefsWindow } from './prefs.js';
import { useSize } from '../company/kit.js';
import { openApply, DayChip, DaysLayer, useRevealTrip } from './days.js';

export function Studio({ p, drivers }) {
  const r = S.rf;
  const data = r.data;
  const [tip, setTip] = useState(null);
  const timer = useRef(0);
  const onTip = (t) => {
    clearTimeout(timer.current);
    if (!t) {
      if (tip) setTip(null);
      return;
    }
    if (tip && tip.text === t.text) return;
    timer.current = setTimeout(() => setTip(t), 450);
  };
  useKeys(data, drivers);
  useRevealTrip(data);
  const [ref, size] = useSize();
  // (where the Gantt begins: the cards of its header lie on it)
  const [gTop, setGTop] = useState(0);
  useEffect(() => {
    const el = document.querySelector('.rf-studio > .rf-gantt');
    if (el && el.offsetTop !== gTop) setGTop(el.offsetTop);
  });
  if (!data?.board) return html`<div class="rf-reading">${data?.reading || tr('Reading the timetable…')}</div>`;
  const proposal = !!data.proposal;
  return html`<div class="rf-studio" ref=${ref}>
    ${!r.fullscreen && html`<${Kpis} k=${data.kpis} />`}
    <${Toolbar} data=${data} drivers=${drivers} proposal=${proposal} />
    ${proposal && html`<${Proposal} data=${data} />`}
    ${!proposal && offerOpen(r.ruleOffer, data) && html`<${RuleOffer} o=${r.ruleOffer} />`}
    <${Gantt} data=${data} drivers=${drivers} readOnly=${proposal} p=${p} onTip=${onTip} />
    <${StatusBar} data=${data} drivers=${drivers} />
    <${Overlays} data=${data} drivers=${drivers} p=${p} size=${size} gTop=${gTop} />
    <${DaysLayer} size=${size} gTop=${gTop} />
    ${r.prefs && html`<${PrefsWindow} />`}
    ${tip && html`<div class=${'rf-tip' + (tip.pre ? ' pre' : '')} style=${`left:${tip.x + 14}px;top:${tip.y + 18}px`}>${tip.text}</div>`}
  </div>`;
}

// --- the KPI dashboard ----------------------------------------------------------------------------

/** A figure's tile (`rosterflow::tile`): an icon, the number in blue with its difference, the
 * caption in capitals and its formula under it. */
function Tile({ t }) {
  return html`<div class="rf-tile">
    <div class="num"><${Icon} name=${t.icon} size=${24} /><b>${t.value}</b>${t.delta && html`<em class=${t.delta[1] ? 'good' : 'bad'}>${t.delta[0]}</em>`}</div>
    <span class="cap">${t.caption}</span>
    ${t.formula && html`<span class="formula">${t.formula}</span>`}
  </div>`;
}

/** A small bar chart (`rosterflow::histogram`); a click opens it large. */
function Histogram({ bars, caption, onClick, style }) {
  const most = Math.max(1, ...bars.map((b) => b[1]));
  return html`<button type="button" class="rf-hist" style=${style} onClick=${onClick}>
    <span class="bars">${bars.map(([label, v]) => html`<span class="bar"><i>${v}</i><b class=${v === 0 ? 'none' : ''} style=${`height:${Math.max(1, (v / most) * 34)}px`}></b><small>${label}</small></span>`)}</span>
    <span class="cap">${tr(caption).toUpperCase()}</span>
  </button>`;
}

function Kpis({ k }) {
  const r = S.rf;
  const open = r.kpisOpen;
  return html`<div class=${'rf-kpis' + (open ? ' open' : '')}>
    <div class="rf-kscroll">
    <div class="rf-krow">
      <span class="anchor" data-rf="rf-kpi-counts"></span><span class="anchor rest" data-rf="rf-kpi-rest"></span>
      ${k.top.map((t) => html`<${Tile} t=${t} />`)}
    </div>
    ${open && k.more.map((row) => html`<div class="rf-krow">${row.map((t) => html`<${Tile} t=${t} />`)}</div>`)}
    ${open && html`<div class="rf-krow hists">
      <${Histogram} bars=${k.types} caption="Duty types" style="left:10%;width:32%" onClick=${() => setRf({ panel: { kind: 'histogram', which: 0 } })} />
      <${Histogram} bars=${k.bands} caption="Duties paid time" style="left:48%;width:32%" onClick=${() => setRf({ panel: { kind: 'histogram', which: 1 } })} />
    </div>`}
    </div>
    <button type="button" class="rf-kpitab" title=${tr(open ? 'Fewer figures' : 'More figures: splits, changeovers, paid, spread, work, driving and platform time, the histograms')} onClick=${() => setRf({ kpisOpen: !open })}><${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${12} /></button>
  </div>`;
}

// --- the toolbar --------------------------------------------------------------------------------

function Toolbar({ data, drivers, proposal }) {
  const r = S.rf;
  const dirty = !!data.head?.draft;
  const filterOn = r.filter.hidden.length || r.filter.dir || r.filter.from > 0 || r.filter.to < 2880 || r.filter.lockedOnly;
  const stack = data.board.stack.length;
  return html`<div class="rf-toolbar">
    <span data-rf="rf-save-discard" class="pair">
      <span class="rf-split">
        <${RfButton} label="Save" solid disabled=${!dirty || proposal} data="rf-save" title="Make the changes the company's planning: the roster, the day and the game run it" style="width:88px" onClick=${() => rfAct('save')} />
        <${RfButton} label=${html`<${Icon} name="expand_more" size=${18} />`} raw solid disabled=${proposal} data="rf-apply-open" title="Save and apply to other days: every day of this day type, the days with the same timetable, or from a date until a date" style="width:28px;padding:0" onClick=${openApply} />
      </span>
      <${RfButton} label="Discard" disabled=${!dirty || proposal} data="rf-discard" style="width:116px" onClick=${() => rfAct('discard').then(() => setRf({ hand: null, zones: null, sel: [], card: null, menu: null }))} />
    </span>
    <span class="tools" data-rf="rf-tools">
      <${ToolLink} icon="signal_cellular_alt" label="Stats" on=${panelIs('stats') || r.stats.length} onClick=${() => toggle('stats')} />
      <i class="sep"></i>
      <${ToolLink} icon="view_list" label="Layers" on=${panelIs('layers')} onClick=${() => toggle('layers')} />
      <i class="sep"></i>
      <${ToolLink} icon="filter_list" label="Filter" on=${panelIs('filter') || filterOn} onClick=${() => (panelIs('filter') ? setRf({ panel: null }) : setRf({ filterEdit: { ...r.filter }, panel: 'filter' }))} />
    </span>
    ${r.filter.issues && html`<span class="rf-fchip">${tr('Issues (%{n})', { n: r.filter.issues.length })}<button type="button" onClick=${() => setRf({ filter: { ...r.filter, issues: null } })}><${Icon} name="close" size=${14} /></button></span>`}
    ${filterOn && html`<span class="rf-fchip">${tr('Filter')}<button type="button" onClick=${() => setRf({ filter: { ...NO_FILTER, issues: r.filter.issues } })}><${Icon} name="close" size=${14} /></button></span>`}
    <${DayChip} data=${data} />
    <span class="grow"></span>
    ${stack > 0 && !drivers && html`<span class="rf-stackpill">${tr('Stack: %{n} unassigned', { n: stack })}</span>`}
    <span class="opt" data-rf="rf-optimise">
      <${RfButton} label="Optimize" solid disabled=${proposal} data="rf-optimise-btn" title="Optimise the day by the preferences: the vehicles' blocks and the duties, the locked ones kept. Shown as a proposal to accept or not" style="width:140px" onClick=${() => rfAct('optimise')} />
      <button type="button" class="rf-gear" title=${tr('Optimisation settings')} onClick=${() => toggle('optimise')}><${Icon} name="settings" size=${18} /></button>
    </span>
    <${IconBtn} icon="fullscreen" title="Full screen: the KPIs folded away" on=${r.fullscreen} onClick=${() => setRf({ fullscreen: !r.fullscreen })} />
  </div>`;
}

/** "Optimise"'s proposal over the Gantt: what it did in words (all of it on hover), and accept it
 * or keep the day as it is. Where it found nothing better the day as it is, and why: only Close. */
function Proposal({ data }) {
  const better = data.proposal_better !== false;
  const note = data.proposal_note || tr('Optimised proposal: the KPIs show it against your planning. Locked vehicles and duties are kept.');
  return html`<div class="rf-proposal" data-rf="rf-proposal">
    <${Icon} name=${better ? 'bolt' : 'info'} size=${18} />
    <span class="grow" title=${note}>${note}</span>
    ${better && html`<${RfButton} label="Accept" solid style="width:140px" onClick=${() => rfAct('accept')} />`}
    <${RfButton} label=${better ? 'Keep mine' : 'Close'} style="width:140px" onClick=${() => rfAct('keep_mine')} />
  </div>`;
}

/** The changed rules' offer is still of use: the day breaks one of them still (not dismissed). */
const offerOpen = (o, data) => !!o && data.board.issues.some((i) => !i.dismissed && o.groups.includes(i.group));

/** After the Preferences were saved: what the changed rules break now, said at once, and
 * Optimise offered to work to them (testers: "its not actually realising this immediately"). */
function RuleOffer({ o }) {
  return html`<div class="rf-proposal rf-ruleoffer" data-rf="rf-rule-offer">
    <${Icon} name="warning" size=${18} />
    <span class="grow" title=${o.text}>${o.text}</span>
    <${RfButton} label=${o.button} raw solid style="width:160px" onClick=${() => {
      setRf({ ruleOffer: null });
      rfAct('optimise');
    }} />
    <${IconBtn} icon="close" title="Close" onClick=${() => setRf({ ruleOffer: null })} />
  </div>`;
}

// --- the status bar ----------------------------------------------------------------------------------

function StatusBar({ data, drivers }) {
  const r = S.rf;
  const b = data.board;
  const st = b.status;
  const shown = rowsOf(data, drivers).length;
  const words = drivers ? tr('Showing %{a}/%{b} duties', { a: shown, b: b.duties.length }) : tr('Showing %{a}/%{b} vehicles (%{c} unscheduled)', { a: shown, b: b.blocks.length, c: b.stack.length });
  const all = st.assigned === st.trips;
  const issue = (scope, label) => {
    const s = st[scope];
    const on = r.panel?.kind === 'issues' && r.panel.scope === scope;
    const cls = s.n === 0 ? 'dim' : s.hard ? 'err' : 'warn';
    return html`<button type="button" class=${'rf-issue' + (on ? ' on' : '')} onClick=${() => setRf({ panel: on ? null : { kind: 'issues', scope }, issueTicks: [] })}><span class=${cls}><${Icon} name="error" size=${15} /></span>${tr(label, { n: s.n })}</button>`;
  };
  return html`<div class="rf-status">
    <${Wordmark} size=${14} dim />
    <i class="sep"></i>
    <span class="counts" data-rf="rf-status-counts"><span class="shown">${words}</span>
      <span class=${'count ' + (all ? 'all' : 'part')} title=${tr('Trips with passengers assigned to a vehicle')}><${Icon} name="directions_bus" size=${15} />${st.assigned}/${st.trips}</span>
      <span class=${'count ' + (all ? 'all' : 'part')} title=${tr('Trips with passengers in a duty')}><${Icon} name="person" size=${15} />${st.assigned}/${st.trips}</span></span>
    <span class="grow"></span>
    <span class="issues" data-rf="rf-status-issues">
      ${issue('global', 'Global Issues: %{n}')}<i class="sep"></i>
      ${issue('duty', 'Duty Issues: %{n}')}<i class="sep"></i>
      ${issue('vehicle', 'Vehicle Issues: %{n}')}
    </span>
    <${IconBtn} icon="refresh" title="Validate again" onClick=${() => rfAct('revalidate')} />
    <i class="sep"></i>
    <button type="button" class="rf-zoom" onClick=${() => toggle('zoom')}><${Icon} name="zoom_in" size=${16} /><span>${Math.round(r.zoom * 100)}%</span><${Icon} name="expand_more" size=${15} /></button>
  </div>`;
}

// --- the keys -----------------------------------------------------------------------------------------

/** The shortcuts (`rosterflow::keys`): G go to, P pin, the arrows along a row and between rows
 * (Shift to widen the choice), Ctrl+Z and Ctrl+Y, Delete to unassign. */
function useKeys(data, drivers) {
  const ref = useRef({});
  ref.current = { data, drivers };
  useEffect(() => {
    const k = (e) => {
      const r = S.rf;
      const { data: d, drivers: dr } = ref.current;
      if (!r.open || !d?.board || r.prefs || S.co?.popup || S.co?.dialog || r.tut.sheet || r.panel?.kind === 'apply') return;
      const el = document.activeElement;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      if (e.ctrlKey || e.metaKey) {
        const key = e.key.toLowerCase();
        if (key === 'z' && e.shiftKey) rfAct('redo');
        else if (key === 'z') rfAct('undo');
        else if (key === 'y') rfAct('redo');
        else return;
        e.preventDefault();
        return;
      }
      if (e.key === 'g' || e.key === 'G') {
        if (r.panel?.kind !== 'goto') {
          e.preventDefault();
          setRf({ panel: { kind: 'goto', text: '' } });
        }
        return;
      }
      if ((e.key === 'p' || e.key === 'P') && r.selRow) {
        const pinned = r.pinned.includes(r.selRow) ? r.pinned.filter((x) => x !== r.selRow) : [r.selRow, ...r.pinned];
        setRf({ pinned });
        return;
      }
      if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
        e.preventDefault();
        arrowKey(d, dr, e.key, e.shiftKey);
        return;
      }
      if (e.key === 'Delete' && !dr && r.sel.length) {
        const tour = r.sel[0][0];
        edit('unassign_key', { tour, trips: r.sel.filter((s) => s[0] === tour).map((s) => s[1]) });
      }
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
}

export { letGo, DRIVERS, css };

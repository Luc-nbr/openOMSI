// Omsi-Hub's cloud saves, the page's side (docs/omsihub/ONLINE.md "Cloud saves"; the Rust side
// is `omsihub/cloud.rs`): a company tile's badge - cloud or local, and how its sync stands -, the
// question asked before a cloud company opens (a newer version in the cloud, or both copies
// changed), and the account panel's "Cloud saves": the 2 slots, downloading, removing, and
// putting a company in the cloud (both slots taken: keep it local, or replace one). Signed out or
// offline everything is local and nothing here says more than that quietly. The page never talks
// to the cloud itself: Rust does, and answers `cs_state` from what it last fetched.

import { html, render, useEffect, useState } from '../../vendor/preact-htm.js';
import { call } from '../api.js';
import { toast } from '../store.js';
import { language } from '../i18n.js';
import { Icon, tr } from './bits.js';
// (the privacy statement, linked from the question and the slots: what a cloud save keeps)
import { PrivacyLink } from './privacy.js';

// (the cloud saves' own look, loaded with them)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/cloud.css';
document.head.append(sheet);

const ask = (name, args = {}) => call(name, { ...args, lang: language() });

// --- the state, asked every few seconds while something shows it -----------------------------------

let last = null;
const subs = new Set();
let timer = null;

async function poll() {
  try {
    const v = await ask('cs_state');
    (v.notices || []).forEach((n) => toast(n, 'info'));
    last = v;
    subs.forEach((f) => f(v));
  } catch (e) {}
}

/** The cloud saves' state (`cs_state`), kept fresh while a part of the page shows it. */
export function useCloud() {
  const [v, setV] = useState(last);
  useEffect(() => {
    subs.add(setV);
    poll();
    if (!timer) timer = setInterval(poll, 5000);
    return () => {
      subs.delete(setV);
      if (!subs.size) {
        clearInterval(timer);
        timer = null;
      }
    };
  }, []);
  return v;
}

const ICONS = { synced: 'cloud', uploading: 'upload', newer: 'cloud_download', conflict: 'sync_alt', offline: 'cloud', error: 'error', local: 'save' };

/** A company tile's badge: a local save, or a cloud save with how its sync stands. */
export function CloudBadge({ cloud, id }) {
  const c = cloud?.companies?.[id];
  const st = c ? c.state : 'local';
  const label = c ? c.label : cloud?.local_label || tr('Local save');
  const title = c ? (c.tip ? `${cloud.cloud_label} - ${c.tip}` : `${cloud.cloud_label} - ${label}`) : label;
  return html`<span class=${'hubcs-badge s-' + st} title=${title}><${Icon} name=${ICONS[st] || 'save'} size=${12} class=${st === 'uploading' ? 'hubcs-pulse' : ''} /><span class="hubcs-label">${label}</span></span>`;
}

// --- dates, sizes ------------------------------------------------------------------------------------

function when(ms) {
  if (!ms) return '—';
  return new Date(ms).toLocaleString(language(), { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function size(bytes) {
  if (!bytes) return '—';
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// --- before a cloud company opens --------------------------------------------------------------------

/** The question as a modal of its own (the chooser's and the company's boot alike). */
function modal(body) {
  const root = document.createElement('div');
  document.body.append(root);
  const close = () => {
    render(null, root);
    root.remove();
  };
  render(body(close), root);
}

function Side({ icon, title, s }) {
  return html`<div class="hubcs-side"><${Icon} name=${icon} size=${18} />
    <div><b>${title}</b><span>${tr('Saved %{when}', { when: when(s.at) })}</span><span>${tr('On %{device}', { device: s.device || '—' })}</span></div></div>`;
}

function Ask({ v, done }) {
  const [busy, setBusy] = useState(false);
  const conflict = v.ask === 'conflict';
  const pick = async (keep) => {
    if (!keep) return done(null);
    setBusy(true);
    let r;
    try {
      r = await ask('cs_resolve', { id: v.id, keep });
    } catch (e) {
      r = { error: String(e?.message || e) };
    }
    setBusy(false);
    if (r?.error) toast(r.error, r.code === 'offline' ? 'info' : 'error');
    else if (r?.message) toast(r.message, 'ok');
    done(r);
  };
  return html`<div class="overlay hubcs-overlay"><div class="hubcs-ask" role="dialog">
    <header><${Icon} name=${conflict ? 'sync_alt' : 'cloud_download'} size=${22} /><h2>${conflict ? tr('%{name} changed on two computers', { name: v.name }) : tr('A newer %{name} is in the cloud', { name: v.name })}</h2></header>
    <p>${conflict ? tr('This PC and the cloud both have changes since the last sync. Which copy do you want to keep? Nothing is lost until you choose.') : tr('Another computer saved this company after this PC did. Download the newer version before you play?')}</p>
    <div class="hubcs-sides"><${Side} icon="save" title=${tr('This PC')} s=${v.local} /><${Side} icon="cloud" title=${tr('The cloud')} s=${v.cloud} /></div>
    <div class="hubcs-acts">
      ${conflict
        ? html`<button type="button" class="hub-btn small primary" disabled=${busy} onClick=${() => pick('cloud')}>${tr('Keep the cloud\'s')}</button>
          <button type="button" class="hub-btn small" disabled=${busy} onClick=${() => pick('local')}>${tr('Keep this PC\'s')}</button>
          <button type="button" class="hub-btn small" disabled=${busy} title=${tr('This PC\'s copy is kept as %{copy}, a local save.', { copy: v.copy })} onClick=${() => pick('both')}>${tr('Keep both')}</button>`
        : html`<button type="button" class="hub-btn small primary" disabled=${busy} onClick=${() => pick('cloud')}>${tr('Download')}</button>
          <button type="button" class="hub-btn small" disabled=${busy} onClick=${() => pick(null)}>${tr('Not now')}</button>`}
      ${conflict && html`<span class="hubcs-fill"></span><button type="button" class="hub-btn small ghost" disabled=${busy} onClick=${() => pick(null)}>${tr('Decide later')}</button>`}
    </div>
    ${conflict && html`<p class="hubcs-note">${tr('Keep both: this PC\'s copy stays as %{copy}, a local save; the cloud\'s opens.', { copy: v.copy })}</p>`}
    <p class="hubcs-note">${tr('Cloud saves stay private to your account.')} <${PrivacyLink} /></p>
  </div></div>`;
}

/** Before company `id` opens: the cloud asked (at most a few seconds: offline it opens as it
 * is), and the player's choice made when a newer version or a conflict waits. The companies the
 * choice wrote are read again by the company's session. */
export async function cloudBeforeOpen(id) {
  let v = null;
  try {
    v = await Promise.race([ask('cs_open', { id }), new Promise((ok) => setTimeout(() => ok(null), 4000))]);
  } catch (e) {}
  if (v?.notice) toast(v.notice, 'info');
  if (!v?.ask) return;
  const r = await new Promise((ok) => modal((close) => html`<${Ask} v=${{ ...v, id }} done=${(x) => (close(), ok(x))} />`));
  for (const x of [r?.reread, r?.copy].filter(Boolean)) await call('co_reread', { id: x }).catch(() => {});
  poll();
}

// --- the account panel's "Cloud saves" ----------------------------------------------------------------

/** A button that asks once more: the first click arms it for a few seconds. */
function Sure({ icon, label, sure, onClick, busy }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(t);
  }, [armed]);
  return html`<button type="button" class=${'hub-btn small ' + (armed ? 'danger' : 'ghost')} disabled=${busy} onClick=${() => (armed ? (setArmed(false), onClick()) : setArmed(true))}><${Icon} name=${icon} size=${14} />${armed ? sure : label}</button>`;
}

/** The 2 slots, a local company to put in the cloud, and the limit's offer. */
export function CloudSaves() {
  const v = useCloud();
  const [companies, setCompanies] = useState([]);
  const [busy, setBusy] = useState(false);
  const [pick, setPick] = useState('');
  const [limit, setLimit] = useState(null);
  // (the driver's companies, {id, name}: read again after each change)
  const [read, setRead] = useState(0);
  useEffect(() => {
    ask('cs_companies')
      .then((l) => setCompanies(Array.isArray(l) ? l : []))
      .catch(() => {});
  }, [read]);
  if (!v) return null;
  const run = async (name, args) => {
    setBusy(true);
    let r;
    try {
      r = await ask(name, args);
    } catch (e) {
      r = { error: String(e?.message || e) };
    }
    setBusy(false);
    if (r?.error) toast(r.error, r.code === 'offline' ? 'info' : 'error');
    else if (r?.message) toast(r.message, 'ok');
    if (r?.reread) await call('co_reread', { id: r.reread }).catch(() => {});
    poll();
    setRead((n) => n + 1);
    return r || {};
  };
  const put = async (id, replace) => {
    const r = await run('cs_put', replace ? { id, replace } : { id });
    setLimit(r.limit ? { id, text: r.text, slots: r.slots } : null);
    if (r.ok) setPick('');
  };
  const local = companies.filter((c) => !v.companies?.[c.id]);
  const chosen = pick && local.some((c) => c.id === pick) ? pick : local[0]?.id || '';
  const slots = v.slots || [];
  return html`<section class="hubcs-panel">
    <div class="hub-caps"><b>${tr('Cloud saves')}</b><span>${tr('%{n} of %{max} used', { n: slots.filter((s) => !s.empty).length, max: v.max_slots || 2 })}</span></div>
    ${v.offline && html`<p class="hubcs-quiet"><${Icon} name="cloud" size=${14} />${tr('Offline: your companies play on as local saves and sync when the network is back.')}</p>`}
    ${!v.listed && !v.offline && html`<p class="hubcs-quiet">…</p>`}
    ${slots.map((s) => html`<div class=${'hubcs-slot' + (s.empty ? ' empty' : '')}>
      <span class="hubcs-num">${s.slot}</span>
      ${s.empty
        ? html`<div class="hubcs-who"><b>${tr('Empty slot')}</b><span>${tr('Put a company in the cloud to play it on every PC you sign in on.')}</span></div>`
        : html`<div class="hubcs-who"><b class="hubcs-cut">${s.name}</b><span class="hubcs-cut">${size(s.size)} · ${tr('Last sync %{when}', { when: when(s.at) })}${s.device ? ' · ' + s.device : ''}</span>
            ${!s.linked && html`<span class="hubcs-cut">${tr('Not on this PC')}</span>`}</div>
          <span class="hubcs-btns">
            <button type="button" class="hub-btn small" disabled=${busy} onClick=${() => run('cs_download', { slot: s.slot })}><${Icon} name="cloud_download" size=${14} />${tr('Download to this PC')}</button>
            <${Sure} icon="delete" label=${tr('Remove from the cloud')} sure=${tr('Remove for good?')} busy=${busy} onClick=${() => run('cs_remove', { slot: s.slot })} />
          </span>`}
    </div>`)}
    ${local.length > 0 &&
    html`<div class="hubcs-put">
      <select class="hubcs-select" value=${chosen} onChange=${(e) => (setPick(e.currentTarget.value), setLimit(null))}>${local.map((c) => html`<option value=${c.id}>${c.name}</option>`)}</select>
      <button type="button" class="hub-btn small primary" disabled=${busy || !chosen || v.offline} onClick=${() => put(chosen)}><${Icon} name="upload" size=${14} />${tr('Put this company in the cloud')}</button>
    </div>`}
    ${limit &&
    html`<div class="hubcs-limit">
      <p>${limit.text}</p>
      <div class="hubcs-btns wrap">
        <button type="button" class="hub-btn small" onClick=${() => (setLimit(null), toast(tr('%{name} stays local. Local saves are unlimited and can still host company multiplayer.', { name: companies.find((c) => c.id === limit.id)?.name || '' }), 'ok'))}>${tr('Keep it local')}</button>
        ${limit.slots.map((s) => html`<${Sure} icon="sync_alt" label=${tr('Replace %{name}', { name: s.name })} sure=${tr('Replace %{name} for good?', { name: s.name })} busy=${busy} onClick=${() => put(limit.id, s.slot)} />`)}
      </div>
      <p class="hubcs-note">${tr('Replacing removes that company from the cloud. Its copy on this PC, if any, stays as a local save.')}</p>
    </div>`}
    <p class="hubcs-note">${tr('At most 2 companies in the cloud. Every other company is a local save on this PC: unlimited, and it can still host company multiplayer.')} <${PrivacyLink} /></p>
  </section>`;
}

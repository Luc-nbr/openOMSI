// The updates as our game-drawn launcher shows them (`crates/omsi-launcher-ui/src/update.rs`,
// Trello "Tauri 6/6"): a new version offered in a dialog over the page - Update now, Not now,
// What's new, and "install updates without asking" - its download and its installation, a
// failure with Try again; "Updated to Omsi-Hub x" in the top right corner for a few seconds
// after an update; the Settings page's "Check now" with what the last look found. Looking,
// downloading, installing and starting the new launcher are the Rust side's
// (`omsihub/update.rs`): the page asks for the state once a second.

import { html, useEffect } from '../../vendor/preact-htm.js';
import { S, set, useStore } from '../store.js';
import { call } from '../api.js';
import { language } from '../i18n.js';
import { saveSettings } from '../data.js';
import { Icon, Button, Switch, tr } from './bits.js';

/** The project Omsi-Hub is built on (the credit under the version, as a fork owes it). */
const UPSTREAM = 'https://github.com/openOMSI-Project/openOMSI';

const ask = (what) =>
  call(what ? 'hub_update_act' : 'hub_update', { what, lang: language() })
    .then((u) => {
      // ("Updated to ..." once, kept on screen for its nine seconds)
      if (u.updated) set({ hubUpdated: { text: u.updated, at: Date.now() } });
      set({ hubUpdate: u });
      return u;
    })
    .catch(() => null);

/** The state asked for once a second while the launcher is open (not in its self-check). */
export function useUpdates() {
  useEffect(() => {
    if (S.boot?.selftest) return;
    ask();
    const k = setInterval(() => ask(), 1000);
    return () => clearInterval(k);
  }, []);
}

/** One of the dialog's buttons pressed (`install`, `dismiss`, `check`, `page`, `github`). */
export const updateAct = (what) => ask(what);

/** The dialog over the page while there is something to say (`draw_update_dialog`). */
export function UpdateDialog() {
  const s = useStore();
  const d = s.hubUpdate?.dialog;
  if (!d) return null;
  const busy = d.kind === 'busy';
  return html`<div class="overlay hub-update">
    <div class="dialog" role="dialog">
      <div class="head"><span class=${'mark' + (d.kind === 'failed' ? ' bad' : '')}><${Icon} name=${d.icon} size=${20} /></span><h2>${d.title}</h2></div>
      <p>${d.text}</p>
      ${busy && html`<div class="bar"><i style=${`width:${Math.round((d.frac || 0) * 1000) / 10}%`}></i></div>`}
      ${d.kind === 'available' && html`<div class="auto"><${Switch} checked=${d.auto} label="Install updates without asking from now on" onChange=${(on) => saveSettings({ update_auto: on }).then(() => ask())} /></div>`}
      ${d.kind === 'available' &&
      html`<div class="foot"><${Button} kind="ghost" icon="open_in_new" label="What's new" onClick=${() => updateAct('page')} /><span class="grow"></span>
        <${Button} label="Not now" onClick=${() => updateAct('dismiss')} /><${Button} kind="primary" icon="download" label="Update now" onClick=${() => updateAct('install')} /></div>`}
      ${d.kind === 'failed' &&
      html`<div class="foot"><${Button} kind="ghost" icon="open_in_new" label="Open on GitHub" onClick=${() => updateAct('github')} /><span class="grow"></span>
        <${Button} icon="refresh" label="Try again" onClick=${() => updateAct('check')} /><${Button} label="Close" onClick=${() => updateAct('dismiss')} /></div>`}
    </div>
  </div>`;
}

/** "Updated to Omsi-Hub x" in the top right corner, faded in and out (`draw_updated_notice`). */
export function UpdatedNotice() {
  const s = useStore();
  const n = s.hubUpdated;
  useEffect(() => {
    if (!n) return;
    const k = setTimeout(() => S.hubUpdated === n && set({ hubUpdated: null }), 9000);
    return () => clearTimeout(k);
  }, [n?.at]);
  if (!n) return null;
  return html`<div class="hub-updated" key=${n.at}><${Icon} name="check_circle" size=${20} /><b>${n.text}</b></div>`;
}

/** The Settings page's row under the update switches: Check now, what the last look found, and
 * the fork's releases on GitHub (the game-drawn launcher's settings, `Updates`). */
export function UpdateRow() {
  const s = useStore();
  const u = s.hubUpdate;
  useEffect(() => {
    if (!u) ask();
  }, []);
  return html`<div class="setting-row full">
    <div class="row wrap" style="gap:12px;align-items:center">
      <${Button} icon="refresh" label=${u?.busy ? 'Checking...' : 'Check now'} disabled=${!!u?.busy} onClick=${() => updateAct('check')} />
      <span class="muted small grow">${u?.line || ''}</span>
    </div>
    <div style="margin-top:8px"><${Button} kind="ghost" icon="open_in_new" raw label=${u?.repo || 'github.com/Luc-nbr/omsi-hub'} onClick=${() => updateAct('repo')} /></div>
    <div class="row wrap hub-credit" style="gap:8px;align-items:center;margin-top:10px">
      <span class="muted small grow">${tr("Omsi-Hub is built on openOMSI, the open rebuild of OMSI 2's engine.")}</span>
      <${Button} kind="ghost" icon="open_in_new" raw label=${UPSTREAM.replace('https://', '')} onClick=${() => call('open_url', { url: UPSTREAM }).catch(() => {})} />
    </div>
  </div>`;
}

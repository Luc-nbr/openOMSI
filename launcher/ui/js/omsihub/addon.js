// Omsi-Hub as an add-on of the player's own openOMSI (docs/omsihub/ADDON_PLAN.md, phase 1): the
// Settings page's "Your openOMSI" tab - the folder chosen with the system's picker, checked and
// remembered, the version read from it, how well Omsi-Hub works with it (supported, with limits,
// not supported, or checked at the first drive) and the Omsi-Hub plugin switched on or off there.
// The Rust side is `crates/omsi-launcher-api/src/omsihub/addon.rs`. The add-on mode (phase 2) makes
// Omsi-Hub start the chosen openOMSI for every drive instead of its own copy of the game.

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { call } from '../api.js';
import { Icon, Button, Switch, tr } from './bits.js';

const LEVELS = {
  supported: ['check_circle', 'Supported', 'ok'],
  limited: ['info', 'Supported with limits', 'warn'],
  unsupported: ['block', 'Not supported', 'bad'],
  unknown: ['help', 'Checked at the first drive', 'warn'],
};

const FROM = {
  executable: 'read from the program',
  bundle: 'read from the app bundle',
  folder_name: 'read from the folder name',
  program: 'asked of the program',
};

/** The tab's one row: the folder, the game in it, the compatibility, the plugin. */
export function AddonPanel() {
  const [s, setS] = useState(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');
  const run = (cmd, args) => {
    setBusy(true);
    setErr('');
    return call(cmd, args)
      .then((v) => setS(v))
      .catch((e) => setErr(String(e)))
      .finally(() => setBusy(false));
  };
  useEffect(() => {
    run('addon_state');
  }, []);
  const choose = () =>
    call('pick_folder', { title: tr('Choose your openOMSI folder') })
      .then((folder) => folder && run('addon_set_folder', { folder }))
      .catch(() => {});
  if (!s) return html`<div class="setting-row full"><span class="muted small">${err ? tr(err) : ''}</span></div>`;
  const inst = s.install;
  const level = s.compat && LEVELS[s.compat.level];
  const plugin = s.plugin;
  return html`<div class="setting-row full hub-addon">
    <p class="muted small" style="margin:0 0 10px">${tr('Omsi-Hub can join an openOMSI you installed yourself. Choose its folder: Omsi-Hub checks its version and puts the Omsi-Hub plugin into it.')}</p>
    <div class="row wrap" style="gap:12px;align-items:center">
      <${Icon} name="folder" />
      <span class="grow" style="word-break:break-all">${s.folder || tr('No folder chosen: Omsi-Hub uses its own copy of the game.')}</span>
      <${Button} icon="folder_open" label="Choose folder" disabled=${busy} onClick=${choose} />
      ${s.folder && html`<${Button} kind="ghost" icon="close" label="Forget this folder" disabled=${busy} onClick=${() => run('addon_clear')} />`}
    </div>
    ${s.refused && html`<p class="small bad" style="margin:8px 0 0">${tr(s.refused.problem)} <span class="muted">${s.refused.folder}</span></p>`}
    ${s.problem && html`<p class="small bad" style="margin:8px 0 0">${tr(s.problem)}</p>`}
    ${err && html`<p class="small bad" style="margin:8px 0 0">${tr(err)}</p>`}
    ${inst &&
    html`<div style="margin-top:12px">
      <div class="row wrap" style="gap:8px;align-items:center">
        <b>${inst.version ? tr('openOMSI %{version}', { version: inst.version }) : tr('Version not found')}</b>
        ${inst.version_from && html`<span class="muted small">(${tr(FROM[inst.version_from])})</span>`}
        ${inst.omsihub_build && html`<span class="muted small">${tr('An Omsi-Hub build of the game')}</span>`}
      </div>
      ${level &&
      html`<div class=${'row wrap ' + level[2]} style="gap:8px;align-items:center;margin-top:8px">
        <${Icon} name=${level[0]} /><b>${tr(level[1])}</b>
        <span class="muted small">${s.compat.confirmed ? tr('Checked with this openOMSI') : tr('Estimated from the version')}</span>
      </div>`}
      ${(s.compat?.notes || []).map((n) => html`<p class="small" style="margin:4px 0 0">${tr(n)}</p>`)}
      <ul class="small" style="margin:8px 0 0;padding-left:18px">
        ${(s.compat?.features || []).map(
          (f) => html`<li class=${f.on ? '' : 'muted'}>${tr(f.label)}${!f.on && f.fallback ? html` <span class="muted">(${tr(f.fallback)})</span>` : ''}</li>`,
        )}
      </ul>
      <div class="row wrap" style="gap:12px;align-items:center;margin-top:12px">
        <span class="grow"><${Switch}
          checked=${plugin && (plugin.state === 'on' || plugin.state === 'outdated')}
          label="Omsi-Hub plugin in this openOMSI"
          onChange=${(on) => run('addon_enable', { on })}
        /></span>
        ${plugin?.state === 'outdated' && html`<${Button} icon="download" label="Update the plugin" disabled=${busy} onClick=${() => run('addon_enable', { on: true })} />`}
      </div>
      <p class="muted small" style="margin:4px 0 0">${plugin ? pluginLine(plugin, s.plugin_version) : ''}</p>
      <div class="row wrap" style="gap:12px;align-items:center;margin-top:12px">
        <span class="grow"><${Switch}
          checked=${!!s.use_folder}
          label="Start this openOMSI for my drives"
          onChange=${(on) => run('addon_set_mode', { on })}
        /></span>
      </div>
      <p class="muted small" style="margin:4px 0 0">${s.use_folder ? tr('Omsi-Hub starts this openOMSI with its plugin. Only the company\'s buses on the map, duties of several tours and the display font still need Omsi-Hub\'s own copy of the game.') : tr('Omsi-Hub starts its own copy of the game.')}</p>
      <p class="muted small" style="margin:4px 0 0;word-break:break-all">${tr('Plugins folder: %{dir}', { dir: inst.plugins_dir })}</p>
    </div>`}
  </div>`;
}

function pluginLine(p, ours) {
  if (p.state === 'missing') return tr('Not installed');
  if (p.state === 'off') return tr('Switched off in this openOMSI');
  if (p.state === 'outdated') return tr('Version %{version} installed, this Omsi-Hub has %{ours}', { version: p.version || '?', ours });
  return tr('Version %{version} installed', { version: p.version || ours });
}

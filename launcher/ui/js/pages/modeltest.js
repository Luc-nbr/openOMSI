// Model test: a bus loaded by the game program itself (`openomsi --check-vehicle`),
// every warning and error of its loaders listed - what a modder used to read off the terminal
// while working on a model - with the model's textures looked for.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Empty, PageHead, Select, Spinner, Tag, Notice, Toggle } from '../ui.js';

const LEVEL_TAG = { error: 'danger', warn: 'accent', info: '' };

/** The running game's log as it grows: its warnings and errors (or everything), read every
 * two seconds - what the terminal showed while a model was tried in the game. */
function LiveLog() {
  const s = useStore();
  const games = s.poll.instances || [];
  const game = games[0] || null;
  const [lines, setLines] = useState([]);
  const [all, setAll] = useState(false);
  const [skip, setSkip] = useState(0);
  const pid = game?.pid;
  useEffect(() => {
    if (!pid) {
      setLines([]);
      return;
    }
    const read = () => call('game_log', { pid, lines: 600 }).then(setLines).catch(() => {});
    read();
    const id = setInterval(read, 2000);
    return () => clearInterval(id);
  }, [pid]);
  const shown = lines.filter((l) => all || /WARN|ERROR|panicked/.test(l)).slice(skip);
  return html`<${Card} title="Live, from the running game" actions=${html`<div class="row">
      <${Toggle} checked=${all} onChange=${setAll} label="Everything" />
      <${Button} small label="Clear" onClick=${() => setSkip(lines.filter((l) => all || /WARN|ERROR|panicked/.test(l)).length)} disabled=${!game} />
    </div>`}>
    ${!game
      ? html`<div class="muted">${t('No game runs. Start one on the Drive page with the bus you work on; its warnings appear here as they happen.')}</div>`
      : shown.length === 0
        ? html`<div class="muted">${t('Nothing yet from the game ({pid}).', { pid })}</div>`
        : html`<div class="log" style="max-height:320px;overflow:auto">${shown.join('\n')}</div>`}
  </${Card}>`;
}

export function ModelTestPage() {
  const s = useStore();
  const vehicles = s.vehicles || [];
  const [bus, setBus] = useState(s.choice?.bus || vehicles[0]?.file || '');
  const [result, setResult] = useState(null);
  const [busy, setBusy] = useState(false);
  const [all, setAll] = useState(false);
  const run = async () => {
    if (!bus || busy) return;
    setBusy(true);
    try {
      setResult(await call('model_check', { bus }));
    } catch (e) {
      toast(e.message, 'error');
    }
    setBusy(false);
  };
  const findings = (result?.findings || []).filter((f) => all || f.level !== 'info');
  const sum = result?.summary || {};
  return html`
    <${PageHead} title="Model test" text="A bus loaded the way the game loads it, with the trailer or rear section coupled behind it: every warning of the .bus files, the models, the meshes, the scripts and the paint schemes, and the textures the models name that are not there." actions=${html`<div class="row">
        <div style="min-width:320px"><${Select} value=${bus} options=${vehicles.map((v) => [v.file, v.name ? `${v.name} (${v.file.split('/').pop()})` : v.file])} onChange=${setBus} raw placeholder="Choose a bus" /></div>
        <${Button} kind="primary" icon="bus" label=${busy ? 'Checking…' : 'Check'} onClick=${run} disabled=${busy || !bus} />
        ${busy && html`<${Spinner} />`}
      </div>`} />
    ${!result
      ? html`<${Empty} icon="bus" text="Choose a bus and press Check. Edit the model, press Check again: the list is fresh each time." />`
      : html`<div class="stack">
        ${result.ok
          ? html`<${Notice} kind="ok">${t('Loaded without errors: {name}', { name: sum.name || result.file })}${result.warnings ? ` · ${result.warnings} ${t('warnings')}` : ''}</${Notice}>`
          : html`<${Notice} kind="danger">${t('The vehicle did not load cleanly: {n} errors, {w} warnings', { n: result.errors, w: result.warnings })}</${Notice}>`}
        <${Card} title="Summary">
          <div class="row wrap">
            ${sum.parts?.length > 1 && html`<${Tag} kind="accent">${t('parts')}: ${sum.parts.length}</${Tag}>`}
            <${Tag}>${t('meshes')}: ${sum.meshes ?? '–'}</${Tag}>
            <${Tag}>${t('materials')}: ${sum.materials ?? '–'}</${Tag}>
            <${Tag} kind=${sum.textures_missing ? 'danger' : 'ok'}>${t('textures')}: ${sum.textures ?? '–'}${sum.textures_missing ? ` (${sum.textures_missing} ${t('missing')})` : ''}</${Tag}>
            <${Tag}>${t('paint schemes')}: ${sum.paint_schemes ?? '–'}</${Tag}>
            <${Tag}>${t('script variables')}: ${sum.variables ?? '–'}</${Tag}>
            <span class="muted">${result.seconds?.toFixed(1)} s · ${result.log_lines} ${t('log lines')} · ${result.file}</span>
          </div>
        </${Card}>
        <${Card} title="Findings" actions=${html`<${Toggle} checked=${all} onChange=${setAll} label="Everything the loaders said" />`}>
          ${findings.length === 0
            ? html`<div class="muted">${t('Nothing to report.')}</div>`
            : html`<div class="list" style="margin:0 -8px">${findings.map((f) => html`<div class="list-item" style="cursor:default;align-items:flex-start">
                <${Tag} kind=${LEVEL_TAG[f.level] || ''}>${t(f.level)}</${Tag}>
                <div class="grow" style="min-width:0">
                  <div class="title" style="white-space:pre-wrap;word-break:break-word;font-weight:500">${f.text}</div>
                  <div class="sub">${f.part && sum.parts?.length > 1 ? f.part + ' · ' : ''}${f.what}${f.hint ? ' · ' + f.hint : ''}</div>
                </div>
              </div>`)}</div>`}
        </${Card}>
      </div>`}
    <div style="margin-top:16px"><${LiveLog} /></div>`;
}

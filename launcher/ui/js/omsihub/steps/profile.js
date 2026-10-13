// Who drives (`flow::step_profile`): the drivers on this computer as tiles - a driver is a
// person, not a row in a table - the chosen one in the accent with what they have driven, a
// new driver's tile typed in place, and the question before one is deleted.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S, set, useStore, toast } from '../../store.js';
import { call } from '../../api.js';
import { loadProfiles } from '../../data.js';
import { Actions, Foot, Head, Icon, tr, hoursText, Button } from '../bits.js';
import { go } from '../hub.js';
import { goStep } from '../flow.js';

async function choose(name) {
  try {
    const config = await call('profile_select', { name });
    set({ config, profile: await call('profile', { name }) });
  } catch (e) {
    toast(e.message, 'error');
  }
}

function NewDriver({ onDone }) {
  const [name, setName] = useState('');
  const ref = useRef();
  useEffect(() => ref.current?.focus(), []);
  const create = async () => {
    const n = name.trim();
    if (!n) return;
    try {
      await call('profile_create', { name: n, sex: 'M' });
      await choose(n);
      await loadProfiles();
      onDone();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return html`<div class="hub-driver new" style="box-shadow:inset 0 0 0 2px var(--hub-accent)">
    <div class="row">
      <span class="mono" style="width:44px;height:44px;background:rgba(var(--hub-accent-rgb),0.3)">${name.trim() ? name.trim()[0].toUpperCase() : html`<${Icon} name="add" size=${22} />`}</span>
      <b class="grow" style="margin:0">${tr('New driver')}</b>
      <button type="button" class="hub-bar-btn" title=${tr('Cancel')} onClick=${onDone}><${Icon} name="close" size=${18} /></button>
    </div>
    <input ref=${ref} class="hub-input" value=${name} placeholder=${tr("The new driver's name")} onInput=${(e) => setName(e.currentTarget.value)}
      onKeyDown=${(e) => (e.key === 'Enter' ? create() : e.key === 'Escape' ? onDone() : null)} spellcheck="false" />
    <${Button} kind="primary" small icon="add" label="Create" disabled=${!name.trim()} onClick=${create} />
  </div>`;
}

export function ProfileStep() {
  const s = useStore();
  const names = s.profiles || [];
  const [adding, setAdding] = useState(false);
  const [asking, setAsking] = useState(null);
  const cur = s.config?.profile || '';
  // (nobody on this computer yet: the new driver's tile is open from the start)
  const open = adding || (s.profiles && !names.length);
  const remove = async (name) => {
    setAsking(null);
    try {
      await call('profile_delete', { name });
      toast(tr('Personnel file of {name} deleted.').replace('{name}', name), 'ok');
      const rest = names.filter((x) => x !== name);
      if (name === cur && rest[0]) await choose(rest[0]);
      await loadProfiles();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return html`
    <section class="hub-sheet wide">
      <div style="width:min(760px,100%);margin:0 auto;display:flex;flex-direction:column;flex:1;min-height:0">
        <${Head} icon="person" title="Bus drivers" sub=${tr('Who is driving today?')} />
        <div class="hub-body">
          <div class="hub-drivers">
            ${names.map((name) => {
              const on = name === cur;
              const p = on && s.profile?.name?.toLowerCase() === name.toLowerCase() ? s.profile : null;
              return html`<div role="button" class=${'hub-driver' + (on ? ' on' : '')} onClick=${() => !on && choose(name)}>
                <span class="mono">${(name[0] || '?').toUpperCase()}</span>
                <b>${name}</b>
                ${p && html`<small>${tr('%{n} duties', { n: p.record?.runs ?? 0 })} · ${hoursText(p.hours)}</small>`}
                <button type="button" class="bin" title=${tr('Delete this driver')} onClick=${(e) => (e.stopPropagation(), setAsking(name))}><${Icon} name="delete" size=${15} /></button>
              </div>`;
            })}
            ${open && html`<${NewDriver} onDone=${() => setAdding(false)} />`}
          </div>
        </div>
        <${Foot} text=${tr('%{n} drivers on this computer', { n: names.length })} />
      </div>
    </section>
    <${Actions} extra=${[{ label: 'New driver', onClick: () => setAdding(true) }, { label: 'Service record', onClick: () => go('profile') }]} main="Next step" onMain=${() => goStep('mode')} />
    ${asking && html`<div class="overlay" onMouseDown=${(e) => e.target === e.currentTarget && setAsking(null)}>
      <div class="dialog" style="width:min(460px,100%)">
        <div class="dialog-head"><span style="color:var(--hub-danger);display:flex"><${Icon} name="delete" size=${22} /></span><h2>${tr('Delete %{name}?', { name: asking })}</h2></div>
        <div class="dialog-body"><p style="color:var(--hub-text-soft)">${tr('Their personnel file and service record on this computer are deleted. This cannot be undone.')}</p></div>
        <div class="dialog-foot"><${Button} label="Cancel" onClick=${() => setAsking(null)} /><${Button} kind="danger" icon="delete" label="Delete" onClick=${() => remove(asking)} /></div>
      </div>
    </div>`}
  `;
}

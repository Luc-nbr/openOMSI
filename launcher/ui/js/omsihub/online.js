// Omsi-Hub online (docs/omsihub/ONLINE.md; the Rust side is `omsihub/online.rs`, its client the
// crate `omsihub-online`): the bar's Account button with the new requests and invites on it, the
// panel - signing up, in and out, the email confirmed and the password reset by a code, the
// profile with the friend code, the account deleted; the friends with their online state, the
// requests, the invites to a company session - and the invites' notices over every page. An
// account is optional: "Play without an account" is always there, and offline the panel says so
// quietly. The email address and the password go to Rust and are kept nowhere on the page.

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { call } from '../api.js';
import { S, set, useStore, toast } from '../store.js';
import { language } from '../i18n.js';
import { Icon, tr } from './bits.js';
// (the account's cloud saves: the 2 slots - `cloud.js`)
import { CloudSaves } from './cloud.js';

// (the panel's own look, loaded with it)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/online.css';
document.head.append(sheet);

const ask = (name, args = {}) => call(name, { ...args, lang: language() });

// --- the state, shared by the button, the panel and the notices: asked every few seconds (Rust
// answers it from what its worker last fetched, without a network)

let last = null;
const subs = new Set();
let timer = null;

function publish(v) {
  last = v;
  subs.forEach((f) => f(v));
}

export async function poll() {
  try {
    publish(await ask('ol_state'));
  } catch (e) {}
}

/** Away after five minutes with the window in the background; back at once. */
function installPresence() {
  let away = null;
  addEventListener('blur', () => {
    clearTimeout(away);
    away = setTimeout(() => ask('ol_presence', { away: true }).catch(() => {}), 5 * 60 * 1000);
  });
  addEventListener('focus', () => {
    clearTimeout(away);
    ask('ol_presence', { away: false }).catch(() => {});
  });
}

function useOnline() {
  const [v, setV] = useState(last);
  useEffect(() => {
    subs.add(setV);
    if (!timer) {
      poll();
      timer = setInterval(poll, 5000);
      installPresence();
    }
    return () => subs.delete(setV);
  }, []);
  return v;
}

/** A command; its message as a toast unless `quiet`, its error too unless `quiet` (the forms say
 * theirs beside the fields). */
async function act(name, args = {}, quiet = false) {
  let r;
  try {
    r = await ask(name, args);
  } catch (e) {
    r = { error: String(e?.message || e) };
  }
  if (!quiet && r?.error) toast(r.error, r.code === 'offline' ? 'info' : 'error');
  else if (!quiet && r?.message) toast(r.message, 'ok');
  poll();
  return r || {};
}

const openPanel = (tab) => set({ hub: { ...S.hub, online: tab || 'auto' } });
const closePanel = () => set({ hub: { ...S.hub, online: null } });

/** Join a friend's company session with its code, through "Play together"'s own join. */
async function joinSession(code) {
  let r;
  try {
    r = await call('tg_join', { code, lang: language() });
  } catch (e) {
    toast(String(e?.message || e), 'error');
    return false;
  }
  if (!r || r.error || r.off) {
    toast(r?.error || r?.tip || tr('Joining did not work.'), 'error');
    return false;
  }
  closePanel();
  const m = await import('./company/together.js');
  m.backToMenu();
  return true;
}

// --- small pieces ---------------------------------------------------------------------------------

function Btn({ kind = '', icon, label, onClick, disabled, title, busy }) {
  return html`<button type="button" class=${'hub-btn small ' + kind} disabled=${disabled || busy} title=${title} onClick=${onClick}>
    ${icon && html`<${Icon} name=${busy ? 'refresh' : icon} size=${15} class=${busy ? 'ol-spin' : ''} />`}${label}
  </button>`;
}

/** A button that asks once more: the first click arms it for a few seconds. */
function Sure({ icon, label, sure, onClick, kind = 'ghost' }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(t);
  }, [armed]);
  return html`<${Btn} kind=${armed ? 'danger' : kind} icon=${icon} label=${armed ? sure : label} onClick=${() => (armed ? (setArmed(false), onClick()) : setArmed(true))} />`;
}

function Field({ label, type = 'text', value, onInput, max, placeholder, auto, onEnter, hint }) {
  return html`<label class="ol-field">
    <span>${label}</span>
    <input class="ol-input" type=${type} value=${value} maxLength=${max} placeholder=${placeholder || ''} autocomplete=${auto || 'off'} spellcheck=${false}
      onInput=${(e) => onInput(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && onEnter && onEnter()} />
    ${hint}
  </label>`;
}

/** How strong a password looks (`check::strength`'s rule). */
function strength(p) {
  const n = [...p].length;
  const kinds = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^A-Za-z0-9]/].filter((r) => r.test(p)).length;
  if (n < 8 || (n < 12 && kinds <= 2)) return 0;
  if (n < 12 || (n < 16 && kinds <= 1) || kinds <= 1) return 1;
  return 2;
}

function Strength({ p }) {
  if (!p) return html`<em class="ol-hint">${tr('At least 8 characters. Longer is stronger.')}</em>`;
  const s = strength(p);
  const words = [tr('Weak password'), tr('Fair password'), tr('Strong password')];
  return html`<em class=${'ol-hint ol-strength s' + s}><i></i><i></i><i></i>${words[s]}</em>`;
}

function Note({ icon = 'info', text, kind = '' }) {
  return html`<p class=${'ol-note ' + kind}><${Icon} name=${icon} size=${14} /><span>${text}</span></p>`;
}

// --- the account --------------------------------------------------------------------------------

/** Signed out: sign in, sign up, the code, the password forgotten; and playing without. */
function SignedOut({ v }) {
  const [step, setStep] = useState('sign_in');
  const [f, setF] = useState({ email: '', password: '', name: '', code: '' });
  const [err, setErr] = useState('');
  const [info, setInfo] = useState('');
  const [busy, setBusy] = useState(false);
  const put = (k) => (x) => setF({ ...f, [k]: x });
  const go = (s) => {
    setStep(s);
    setErr('');
    setInfo('');
  };
  const send = async (name, args) => {
    setBusy(true);
    setErr('');
    const r = await act(name, args, true);
    setBusy(false);
    if (r.error) setErr(r.error);
    else setInfo(r.message || '');
    if (r.step === 'verify' || r.step === 'sign_in') setStep(r.step);
    if (r.ok && !r.step && (name === 'ol_sign_in' || name === 'ol_sign_up' || name === 'ol_verify' || name === 'ol_reset')) {
      if (r.message) toast(r.message, 'ok');
      setF({ email: '', password: '', name: '', code: '' });
    }
    return r;
  };
  const signIn = () => send('ol_sign_in', { email: f.email, password: f.password });
  const signUp = () => send('ol_sign_up', { email: f.email, password: f.password, name: f.name });
  const verify = () => send('ol_verify', { email: f.email, code: f.code, password: f.password, name: f.name });
  const forgot = async () => {
    const r = await send('ol_forgot', { email: f.email });
    if (r.ok) setStep('reset');
  };
  const reset = () => send('ol_reset', { email: f.email, code: f.code, password: f.password });
  const local = async () => {
    await act('ol_local', { on: true }, true);
    closePanel();
  };
  const email = html`<${Field} label=${tr('Email address')} type="email" value=${f.email} onInput=${put('email')} max=${254} auto="email" />`;
  let body;
  if (step === 'sign_up') {
    body = html`<h3>${tr('Create an account')}</h3>
      <${Field} label=${tr('Display name')} value=${f.name} onInput=${put('name')} max=${24} auto="nickname" hint=${html`<em class="ol-hint">${tr('3 to 24 characters. Your friends see it.')}</em>`} />
      ${email}
      <${Field} label=${tr('Password')} type="password" value=${f.password} onInput=${put('password')} max=${128} auto="new-password" onEnter=${signUp} hint=${html`<${Strength} p=${f.password} />`} />
      <div class="ol-row"><${Btn} kind="primary" icon="person" label=${tr('Create account')} busy=${busy} onClick=${signUp} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('I have an account')} onClick=${() => go('sign_in')} /></div>
      <${Note} icon="lock" text=${tr('Only your email address and a hash of your password are kept, to sign you in and to reset the password. Your friends see your display name only.')} />`;
  } else if (step === 'verify') {
    body = html`<h3>${tr('Confirm your email address')}</h3>
      <p class="ol-text">${tr('Enter the code we sent to %{email}.', { email: f.email || tr('your email address') })}</p>
      ${!f.email && email}
      <${Field} label=${tr('Code')} value=${f.code} onInput=${put('code')} max=${10} auto="one-time-code" onEnter=${verify} />
      ${!f.password && html`<${Field} label=${tr('Password')} type="password" value=${f.password} onInput=${put('password')} max=${128} auto="current-password" />`}
      <div class="ol-row"><${Btn} kind="primary" icon="check" label=${tr('Confirm')} busy=${busy} onClick=${verify} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('Send a new code')} onClick=${() => send('ol_resend', { email: f.email })} /></div>`;
  } else if (step === 'forgot') {
    body = html`<h3>${tr('Forgot your password?')}</h3>
      <p class="ol-text">${tr('We send a code to your email address. With it you choose a new password.')}</p>
      ${email}
      <div class="ol-row"><${Btn} kind="primary" icon="key" label=${tr('Send a code')} busy=${busy} onClick=${forgot} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('Back')} onClick=${() => go('sign_in')} /></div>`;
  } else if (step === 'reset') {
    body = html`<h3>${tr('Choose a new password')}</h3>
      <p class="ol-text">${tr('Enter the code we sent to %{email}.', { email: f.email })}</p>
      <${Field} label=${tr('Code')} value=${f.code} onInput=${put('code')} max=${10} auto="one-time-code" />
      <${Field} label=${tr('New password')} type="password" value=${f.password} onInput=${put('password')} max=${128} auto="new-password" onEnter=${reset} hint=${html`<${Strength} p=${f.password} />`} />
      <div class="ol-row"><${Btn} kind="primary" icon="key" label=${tr('Change password')} busy=${busy} onClick=${reset} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('Send a new code')} onClick=${forgot} /></div>`;
  } else {
    body = html`<h3>${tr('Sign in')}</h3>
      ${email}
      <${Field} label=${tr('Password')} type="password" value=${f.password} onInput=${put('password')} max=${128} auto="current-password" onEnter=${signIn} />
      <div class="ol-row"><${Btn} kind="primary" icon="person" label=${tr('Sign in')} busy=${busy} onClick=${signIn} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('Forgot your password?')} onClick=${() => go('forgot')} /></div>
      <div class="ol-row"><span class="ol-text">${tr('New here?')}</span><${Btn} kind="ghost" label=${tr('Create an account')} onClick=${() => go('sign_up')} /></div>`;
  }
  return html`<div class="ol-account">
    <p class="ol-lead">${tr('An account is optional. With one you add friends, see who is online and invite them to your company. Without one, everything else works as before.')}</p>
    ${v?.offline && html`<${Note} icon="wifi" kind="quiet" text=${tr('Offline: the online features are back as soon as the network is.')} />`}
    <div class="ol-form">${body}
      ${err && html`<p class="ol-err">${err}</p>`}
      ${info && !err && html`<p class="ol-ok">${info}</p>`}
    </div>
    <div class="ol-local">
      ${v?.mode === 'local' ? html`<span class="ol-text">${tr('You play without an account.')}</span>` : html`<${Btn} icon="close" label=${tr('Play without an account')} onClick=${local} />`}
    </div>
  </div>`;
}

/** Copy a text, said with a tick for a moment. */
function Copy({ text }) {
  const [ok, setOk] = useState(false);
  const copy = () =>
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setOk(true);
        setTimeout(() => setOk(false), 1500);
      })
      .catch(() => {});
  return html`<${Btn} icon=${ok ? 'check' : 'content_copy'} label=${ok ? tr('Copied') : tr('Copy')} onClick=${copy} />`;
}

/** Signed in: the display name, the friend code, signing out, deleting the account. */
function Profile({ v }) {
  const me = v.me || {};
  const [name, setName] = useState(me.name || '');
  const [err, setErr] = useState('');
  const [del, setDel] = useState(false);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => setName(me.name || ''), [me.name]);
  const save = async () => {
    const r = await act('ol_profile', { name }, true);
    setErr(r.error || '');
    if (r.ok) toast(r.message, 'ok');
  };
  const remove = async () => {
    setBusy(true);
    const r = await act('ol_delete', { password }, true);
    setBusy(false);
    setErr(r.error || '');
    if (r.ok) {
      toast(r.message, 'ok');
      setDel(false);
      setPassword('');
    }
  };
  if (v.need_profile) {
    return html`<div class="ol-account"><div class="ol-form">
      <h3>${tr('Choose your display name')}</h3>
      <${Field} label=${tr('Display name')} value=${name} onInput=${setName} max=${24} onEnter=${save} hint=${html`<em class="ol-hint">${tr('3 to 24 characters. Your friends see it.')}</em>`} />
      <div class="ol-row"><${Btn} kind="primary" icon="check" label=${tr('Save')} onClick=${save} /><span class="ol-fill"></span><${Btn} kind="ghost" icon="logout" label=${tr('Sign out')} onClick=${() => act('ol_sign_out')} /></div>
      ${err && html`<p class="ol-err">${err}</p>`}
    </div></div>`;
  }
  return html`<div class="ol-account">
    ${v.offline && html`<${Note} icon="wifi" kind="quiet" text=${tr('Offline: the online features are back as soon as the network is.')} />`}
    <div class="ol-form">
      <h3>${tr('Your profile')}</h3>
      <${Field} label=${tr('Display name')} value=${name} onInput=${setName} max=${24} onEnter=${save} />
      ${name.trim() !== (me.name || '') && html`<div class="ol-row"><${Btn} kind="primary" icon="check" label=${tr('Save')} onClick=${save} /></div>`}
      <div class="ol-field"><span>${tr('Your friend code')}</span>
        <div class="ol-row"><b class="ol-code">${me.code || '—'}</b><span class="ol-fill"></span>${me.code && html`<${Copy} text=${me.code} />`}</div>
        <em class="ol-hint">${tr('Give it to your friends: they add you with it.')}</em></div>
      ${err && html`<p class="ol-err">${err}</p>`}
      ${v.lasting === false && html`<${Note} text=${tr('This system has no credential store: you sign in again at the next start.')} />`}
      <div class="ol-row"><${Btn} icon="logout" label=${tr('Sign out')} onClick=${() => act('ol_sign_out')} /></div>
    </div>
    <${CloudSaves} />
    <div class="ol-danger">
      ${!del
        ? html`<${Btn} kind="ghost" icon="delete" label=${tr('Delete my account')} onClick=${() => setDel(true)} />`
        : html`<h3>${tr('Delete my account')}</h3>
          <p class="ol-text">${tr('This deletes your profile, your friends, your invites, your cloud saves and then the account itself. Your companies on this PC stay, as local saves. It cannot be undone.')}</p>
          <${Field} label=${tr('Your password, to confirm')} type="password" value=${password} onInput=${setPassword} max=${128} auto="current-password" />
          <div class="ol-row"><${Btn} kind="danger" icon="delete" label=${tr('Delete for good')} busy=${busy} disabled=${!password} onClick=${remove} /><span class="ol-fill"></span><${Btn} kind="ghost" label=${tr('Cancel')} onClick=${() => (setDel(false), setPassword(''))} /></div>`}
    </div>
  </div>`;
}

// --- the friends ----------------------------------------------------------------------------------

function Invites({ v }) {
  if (!v.invites?.length) return null;
  return html`<section class="ol-sec"><div class="hub-caps"><b>${tr('Invites')}</b></div>
    ${v.invites.map((i) => html`<${InviteRow} i=${i} />`)}</section>`;
}

function InviteRow({ i }) {
  const join = async () => {
    if (await joinSession(i.session)) act('ol_dismiss', { id: i.id }, true);
  };
  return html`<div class="ol-friend invite"><${Icon} name="wifi_tethering" size=${18} class="t-accent" />
    <div class="who"><b class="cut">${i.text}</b></div>
    <span class="acts"><${Btn} kind="primary" icon="groups" label=${tr('Join')} onClick=${join} /><${Btn} kind="ghost" icon="close" label=${tr('Dismiss')} onClick=${() => act('ol_dismiss', { id: i.id }, true)} /></span>
  </div>`;
}

function FriendRow({ f, v }) {
  return html`<div class="ol-friend">
    <span class=${'ol-dot ' + f.state} title=${f.state_label}></span>
    <div class="who"><b class="cut">${f.name}</b><span class="dim cut">${f.state === 'offline' ? f.seen || f.state_label : f.state_label}</span></div>
    <span class="acts">
      ${f.join && html`<${Btn} kind="primary" icon="groups" label=${tr('Join')} title=${tr('Join their company session')} onClick=${() => joinSession(f.session)} />`}
      ${v.hosting && v.together && html`<${Btn} icon="wifi_tethering" label=${tr('Invite to my company')} onClick=${() => act('ol_invite', { id: f.id })} />`}
      <${Sure} icon="remove" label=${tr('Remove')} sure=${tr('Remove?')} onClick=${() => act('ol_remove', { id: f.id })} />
      <${Sure} icon="stop_circle" label=${tr('Block')} sure=${tr('Block?')} onClick=${() => act('ol_block', { id: f.id })} />
    </span>
  </div>`;
}

function Friends({ v }) {
  const [code, setCode] = useState('');
  const [err, setErr] = useState('');
  const add = async () => {
    const r = await act('ol_add', { code }, true);
    setErr(r.error || '');
    if (r.ok) {
      toast(r.message, 'ok');
      setCode('');
    }
  };
  if (v.mode !== 'in' || v.need_profile) {
    return html`<div class="ol-empty"><${Icon} name="group" size=${30} /><p>${tr('Sign in to add friends, see who is online and invite them to your company.')}</p>
      <${Btn} kind="primary" icon="person" label=${tr('Sign in')} onClick=${() => openPanel('account')} /></div>`;
  }
  const incoming = v.requests.filter((r) => r.incoming);
  const outgoing = v.requests.filter((r) => !r.incoming);
  return html`<div class="ol-friends">
    ${v.offline && html`<${Note} icon="wifi" kind="quiet" text=${tr('Offline: the online features are back as soon as the network is.')} />`}
    <section class="ol-sec"><div class="hub-caps"><b>${tr('Add a friend')}</b>${v.me?.code && html`<span>${tr('Your code: %{code}', { code: v.me.code })}</span>`}</div>
      <div class="ol-row"><input class="ol-input" value=${code} maxLength=${12} placeholder=${tr('Their friend code')} spellcheck=${false} onInput=${(e) => setCode(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && add()} />
        <${Btn} kind="primary" icon="add" label=${tr('Add')} disabled=${code.trim().length < 8} onClick=${add} /></div>
      ${err && html`<p class="ol-err">${err}</p>`}
    </section>
    <${Invites} v=${v} />
    ${incoming.length > 0 && html`<section class="ol-sec"><div class="hub-caps"><b>${tr('Friend requests')}</b></div>
      ${incoming.map((r) => html`<div class=${'ol-friend' + (r.new ? ' new' : '')}><${Icon} name="person" size=${18} /><div class="who"><b class="cut">${r.name}</b><span class="dim">${tr('wants to be your friend')}</span></div>
        <span class="acts"><${Btn} kind="primary" icon="check" label=${tr('Accept')} onClick=${() => act('ol_respond', { id: r.id, accept: true })} /><${Btn} kind="ghost" icon="close" label=${tr('Decline')} onClick=${() => act('ol_respond', { id: r.id, accept: false })} /><${Sure} icon="stop_circle" label=${tr('Block')} sure=${tr('Block?')} onClick=${() => act('ol_block', { id: r.id })} /></span></div>`)}
    </section>`}
    <section class="ol-sec"><div class="hub-caps"><b>${tr('Friends')}</b><span>${tr('%{n} online', { n: v.online_count || 0 })}</span></div>
      ${v.friends.length ? v.friends.map((f) => html`<${FriendRow} f=${f} v=${v} />`) : html`<p class="ol-text">${tr('No friends yet. Add one with their friend code.')}</p>`}
      ${v.together && !v.hosting && v.friends.length > 0 && html`<${Note} icon="wifi_tethering" text=${tr('Share your company (Multiplayer) to invite friends to it.')} />`}
    </section>
    ${outgoing.length > 0 && html`<section class="ol-sec"><div class="hub-caps"><b>${tr('Sent requests')}</b></div>
      ${outgoing.map((r) => html`<div class="ol-friend"><${Icon} name="schedule" size=${18} /><div class="who"><b class="cut">${r.name}</b><span class="dim">${tr('waiting for an answer')}</span></div>
        <span class="acts"><${Sure} icon="close" label=${tr('Withdraw')} sure=${tr('Withdraw?')} onClick=${() => act('ol_remove', { id: r.id })} /></span></div>`)}
    </section>`}
  </div>`;
}

// --- the panel, the button, the notices -------------------------------------------------------------

function Panel({ v, tab }) {
  const signed = v?.mode === 'in' && !v.need_profile;
  const now = tab === 'auto' ? (signed ? 'friends' : 'account') : tab;
  useEffect(() => {
    ask('ol_refresh')
      .then(publish)
      .catch(() => {});
    ask('ol_seen').catch(() => {});
    const k = (e) => e.key === 'Escape' && closePanel();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
  const tabs = [
    ['account', 'account_circle', 'Account'],
    ['friends', 'group', 'Friends'],
  ];
  return html`<div class="overlay ol-overlay" onMouseDown=${(e) => e.target === e.currentTarget && closePanel()}>
    <div class="ol-panel">
      <header class="ol-head">
        <${Icon} name=${signed ? 'group' : 'account_circle'} size=${22} />
        <div class="t"><h2>${signed ? v.me?.name || tr('Omsi-Hub online') : tr('Omsi-Hub online')}</h2><p>${signed ? tr('Signed in') : v?.mode === 'local' ? tr('Playing without an account') : tr('Not signed in')}</p></div>
        <button type="button" class="hub-bar-btn" title=${tr('Close')} onClick=${closePanel}><${Icon} name="close" size=${17} /></button>
      </header>
      <nav class="ol-tabs">${tabs.map(([id, icon, label]) => html`<button type="button" class=${'ol-tab' + (now === id ? ' on' : '')} onClick=${() => openPanel(id)}><${Icon} name=${icon} size=${15} />${tr(label)}${id === 'friends' && v?.badge > 0 && html`<span class="ol-badge in">${v.badge}</span>`}</button>`)}</nav>
      <div class="ol-body">${!v ? html`<p class="ol-text">…</p>` : now === 'friends' ? html`<${Friends} v=${v} />` : v.mode === 'in' ? html`<${Profile} v=${v} />` : html`<${SignedOut} v=${v} />`}</div>
    </div>
  </div>`;
}

/** The bar's button (`company`: the bus company's bar): the account, or the friends with the new
 * requests and invites on it. */
export function OnlineButton({ company }) {
  const v = useOnline();
  const s = useStore();
  const signed = v?.mode === 'in';
  const title = !signed ? tr('Account') : v.offline ? tr('Friends: offline') : tr('Friends: %{n} online', { n: v.online_count || 0 });
  return html`<span class=${'ol-btn' + (company ? ' in-co' : '')}><button type="button" class=${(company ? 'co-btn sq' : 'hub-bar-btn') + (s.hub?.online ? ' on' : '')} title=${title} onClick=${() => (S.hub?.online ? closePanel() : openPanel())}>
      <${Icon} name=${signed ? 'group' : 'account_circle'} size=${company ? 20 : 17} />
    </button>${v?.badge > 0 && html`<span class="ol-badge">${v.badge}</span>`}${signed && v.offline && html`<span class="ol-off"></span>`}</span>${company && html`<span class="gap8"></span>`}`;
}

/** The invites' notices (new ones, over every page) and the panel when it is open. */
export function OnlineLayer() {
  const v = useOnline();
  const s = useStore();
  const [hidden, setHidden] = useState([]);
  const fresh = (v?.invites || []).filter((i) => i.new && !hidden.includes(i.id)).slice(0, 2);
  const hide = (id) => setHidden([...hidden, id]);
  return html`${s.hub?.online && html`<${Panel} v=${v} tab=${s.hub.online} />`}
    ${!s.hub?.online && fresh.length > 0 && html`<div class="ol-notices">${fresh.map((i) => html`<div class="ol-notice">
      <${Icon} name="wifi_tethering" size=${20} />
      <div class="t"><b>${tr('Invite')}</b><span>${i.text}</span></div>
      <div class="acts"><${Btn} kind="primary" icon="groups" label=${tr('Join')} onClick=${async () => { hide(i.id); if (await joinSession(i.session)) act('ol_dismiss', { id: i.id }, true); }} />
        <${Btn} kind="ghost" label=${tr('Dismiss')} onClick=${() => (hide(i.id), act('ol_dismiss', { id: i.id }, true))} /></div>
    </div>`)}</div>`}`;
}

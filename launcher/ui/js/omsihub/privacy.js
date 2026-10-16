// The privacy statement, the page's side (docs/omsihub/PRIVACY.md; Rust reads it into blocks,
// `omsihub/privacy.rs`): the dialog that shows it, the link that opens it, the tick under the
// sign-in, and the statement drawn whole (the consent question of `online.js` shows it too). The
// statement itself is English - the binding text -; everything round it is translated.

import { html, render, useEffect, useState } from '../../vendor/preact-htm.js';
import { call } from '../api.js';
import { language } from '../i18n.js';
import { Icon, tr } from './bits.js';

// (the look is online.css's: loaded here too, as Settings and the cloud's question open it)
if (!document.querySelector('link[href="omsihub/online.css"]')) {
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/online.css';
  document.head.append(sheet);
}

let doc = null;

/** The statement: `{version, blocks}` (asked once; it is built into the program). */
export async function privacyDoc() {
  if (!doc) doc = await call('ol_privacy', { lang: language() });
  return doc;
}

const Spans = ({ s }) => (s || []).map(([text, bold]) => (bold ? html`<b>${text}</b>` : text));

function Block({ b }) {
  if (b.h) return b.h === 1 ? html`<h3 class="ol-pv-h1"><${Spans} s=${b.text} /></h3>` : html`<h4 class="ol-pv-h2"><${Spans} s=${b.text} /></h4>`;
  if (b.ul) return html`<ul class="ol-pv-ul">${b.ul.map((li) => html`<li><${Spans} s=${li} /></li>`)}</ul>`;
  if (b.quote) return html`<p class="ol-pv-quote"><${Spans} s=${b.quote} /></p>`;
  if (b.table)
    return html`<div class="ol-pv-table"><table>
      <thead><tr>${b.table.head.map((c) => html`<th><${Spans} s=${c} /></th>`)}</tr></thead>
      <tbody>${b.table.rows.map((r) => html`<tr>${r.map((c) => html`<td><${Spans} s=${c} /></td>`)}</tr>`)}</tbody>
    </table></div>`;
  return html`<p><${Spans} s=${b.p} /></p>`;
}

/** The statement drawn whole, with the line that the English text binds. */
export function Statement() {
  const [d, setD] = useState(doc);
  useEffect(() => {
    if (!d) privacyDoc().then(setD).catch(() => {});
  }, []);
  return html`<div class="ol-pv-doc">
    <p class="ol-pv-binding"><${Icon} name="info" size=${13} />${tr('The privacy statement is in English. The English text is the binding one.')}</p>
    ${d ? d.blocks.map((b) => html`<${Block} b=${b} />`) : html`<p>…</p>`}
  </div>`;
}

function Dialog({ close }) {
  useEffect(() => {
    const k = (e) => e.key === 'Escape' && (e.stopPropagation(), close());
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, []);
  return html`<div class="overlay ol-pv-overlay top" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="ol-pv-box" role="dialog" aria-label=${tr('Privacy statement')}>
      <header class="ol-pv-head"><${Icon} name="lock" size=${20} /><h2>${tr('Privacy statement')}</h2><span class="ol-fill"></span>
        <button type="button" class="hub-bar-btn" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${17} /></button></header>
      <div class="ol-pv-scroll"><${Statement} /></div>
      <div class="ol-pv-acts"><span class="ol-fill"></span><button type="button" class="hub-btn small primary" onClick=${close}>${tr('Close')}</button></div>
    </div>
  </div>`;
}

/** The statement in a dialog of its own, over everything (the panel, the questions, Settings). */
export function showPrivacy() {
  const root = document.createElement('div');
  document.body.append(root);
  const close = () => {
    render(null, root);
    root.remove();
  };
  render(html`<${Dialog} close=${close} />`, root);
}

/** A link to the statement. */
export function PrivacyLink({ label }) {
  return html`<button type="button" class="ol-pv-link" onClick=${(e) => (e.preventDefault(), e.stopPropagation(), showPrivacy())}><${Icon} name="lock" size=${13} />${label || tr('Privacy statement')}</button>`;
}

/** The tick before an account is made or signed in to. */
export function AgreeBox({ on, set }) {
  return html`<div class="ol-pv-agree">
    <label><input type="checkbox" checked=${on} onChange=${(e) => set(e.currentTarget.checked)} /><span>${tr('I have read and agree to the privacy statement')}</span></label>
    <${PrivacyLink} label=${tr('Read the privacy statement')} />
  </div>`;
}

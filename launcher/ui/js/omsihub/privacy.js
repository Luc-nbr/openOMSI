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
  // (# the title, ## a numbered section, ### its numbered subsection)
  if (b.h === 1) return html`<h3 class="ol-pv-h1"><${Spans} s=${b.text} /></h3>`;
  if (b.h === 2) return html`<h4 class="ol-pv-h2"><${Spans} s=${b.text} /></h4>`;
  if (b.h) return html`<h5 class="ol-pv-h3"><${Spans} s=${b.text} /></h5>`;
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
    <p class="ol-pv-binding"><${Icon} name="info" size=${13} />${tr('This Privacy Statement is drawn up in English; the English text is binding.')}</p>
    ${d ? d.blocks.map((b) => html`<${Block} b=${b} />`) : html`<p>…</p>`}
  </div>`;
}

/** The statement in a dialog; `ask`: before an account is made or Google signs in (Luc, Beta 67:
 * the statement pops up first), with the agreement's sentence, "Cancel" and "Accept and continue". */
function Dialog({ close, ask }) {
  useEffect(() => {
    const k = (e) => e.key === 'Escape' && (e.stopPropagation(), close(false));
    addEventListener('keydown', k, true);
    return () => removeEventListener('keydown', k, true);
  }, []);
  return html`<div class="overlay ol-pv-overlay top" onMouseDown=${(e) => e.target === e.currentTarget && close(false)}>
    <div class="ol-pv-box" role="dialog" aria-modal="true" aria-label=${tr('Privacy Statement')}>
      <header class="ol-pv-head"><${Icon} name="lock" size=${20} /><h2>${tr('Privacy Statement')}</h2><span class="ol-fill"></span>
        <button type="button" class="hub-bar-btn" title=${tr('Close')} onClick=${() => close(false)}><${Icon} name="close" size=${17} /></button></header>
      ${ask && html`<p class="ol-pv-lead">${tr('Before an account is created or used, the Privacy Statement must be accepted.')}</p>`}
      <div class="ol-pv-scroll"><${Statement} /></div>
      ${ask
        ? html`<p class="ol-pv-consent">${tr('I have read the Privacy Statement and agree to the processing of my personal data as described therein.')}</p>
            <div class="ol-pv-acts"><span class="ol-fill"></span><button type="button" class="hub-btn small" onClick=${() => close(false)}>${tr('Cancel')}</button>
            <button type="button" class="hub-btn small primary" autofocus onClick=${() => close(true)}>${tr('Accept and continue')}</button></div>`
        : html`<div class="ol-pv-acts"><span class="ol-fill"></span><button type="button" class="hub-btn small primary" onClick=${() => close(false)}>${tr('Close')}</button></div>`}
    </div>
  </div>`;
}

function open(ask) {
  return new Promise((done) => {
    const root = document.createElement('div');
    document.body.append(root);
    const close = (yes) => {
      render(null, root);
      root.remove();
      done(!!yes);
    };
    render(html`<${Dialog} close=${close} ask=${ask} />`, root);
  });
}

/** The statement in a dialog of its own, over everything (the panel, the questions, Settings). */
export function showPrivacy() {
  open(false);
}

/** The statement with the question: true once the player accepted it, false when cancelled. */
export function askPrivacy() {
  return open(true);
}

/** A link to the statement. */
export function PrivacyLink({ label }) {
  return html`<button type="button" class="ol-pv-link" onClick=${(e) => (e.preventDefault(), e.stopPropagation(), showPrivacy())}><${Icon} name="lock" size=${13} />${label || tr('Privacy Statement')}</button>`;
}

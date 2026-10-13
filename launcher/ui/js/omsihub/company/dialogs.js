// What lies over the company's pages, as our game-drawn launcher draws it last
// (`company::overlays`): the cover while the company's time is simulated, what happened while
// the player was away, the days' report, a decision the clock waits for, the dialog open (the
// company's settings, the clock's steps, a confirmation, a bus, a line, a fare, the dealer's
// sheet) - and the popup over all of them.

import { S } from '../../store.js';
import { html, setCo, Popup } from './kit.js';

/** Something lies over the pages: the clock holds what waits for it (`company::modal`). */
export function modalOpen() {
  const c = S.co;
  const sh = c.shell;
  return !!(c.dialog || c.popup || sh?.cover || sh?.reports || sh?.asking || sh?.away || sh?.sheet);
}

export function Overlays({ founding }) {
  const c = S.co;
  const sh = c.shell;
  let under = null;
  if (!founding && sh?.open) {
    if (sh.cover) under = html`<div class="co-cover"><div><b>${sh.cover}</b><div class="bar"><i></i></div></div></div>`;
  }
  return html`${under}${c.popup && html`<${Popup} p=${c.popup} onClose=${() => setCo({ popup: null })} />`}`;
}

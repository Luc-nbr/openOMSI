// "Play together", a friend's side (Beta 63; the Rust side is `omsihub/together/join.rs`): how the
// session stands, asked every two seconds on the pages of the boss's company (`tg_status`). When
// it ended - the boss closed it, his launcher was lost, the friend was sent away - a dialog says so
// over whatever is shown, with the way back to the menu (nothing of the session kept: its pages,
// RosterFlow's state) and, when the boss's launcher was lost, the way to join again with the same
// code. Every text comes from Rust, translated.

import { set } from '../../store.js';
import { html, useEffect, useState, cmd, freshCo, Dialog, Btn, Icon } from './kit.js';
import { fresh as freshRf } from '../rosterflow/state.js';

/** Over the pages of the boss's company: the dialog once the session ended. */
export function SessionWatch() {
  const [end, setEnd] = useState(null);
  useEffect(() => {
    let gone = false;
    const ask = () =>
      cmd('tg_status')
        .then((v) => !gone && setEnd(v?.ended || null))
        .catch(() => {});
    ask();
    const t = setInterval(ask, 2000);
    return () => {
      gone = true;
      clearInterval(t);
    };
  }, []);
  if (!end) return null;
  return html`<${SessionEnded} e=${end} />`;
}

/** Out of the session, nothing of it kept: back to the company's menu - or, `again`, joined anew
 * with the same code (its page shows the boss's launcher being looked for). */
async function out(again) {
  try {
    await cmd(again ? 'tg_rejoin' : 'tg_leave');
  } catch {
    /* (gone already) */
  }
  set({ rf: freshRf(), co: freshCo({ chooser: again ? 'join' : 'mode' }), page: 'company' });
}

/** How the session ended (`join::ended_json`): what happened, the duty whose results no longer
 * reach the boss, and the way back. */
export function SessionEnded({ e }) {
  const foot = html`${e.retry && html`<${Btn} icon="refresh" label=${e.again} onClick=${() => out(true)} />`}<${Btn} kind="primary" icon="home" label=${e.back} onClick=${() => out(false)} />`;
  return html`<${Dialog} icon=${e.icon} title=${e.title} w=${540} foot=${foot}>
    <p>${e.text}</p>
    ${e.driving && html`<p class="tg-banner t-warn"><${Icon} name="directions_bus" size=${16} />${e.driving}</p>`}
  </${Dialog}>`;
}

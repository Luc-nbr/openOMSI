// The livery editor's page: the studio on the page's own GPU (`studio.js`) where the page can
// draw with WebGL2, else the game program's studio streamed as pictures (`editors/livery.js`).

import { html, useEffect, useState } from '../../../../vendor/preact-htm.js';
import { tr } from '../../editors/kit.js';
import { LiveryPage } from '../../editors/livery.js';
import { Link } from '../link.js';
import { LakStudio } from './studio.js';

/** Whether the page draws (known once the renderer answered). */
let can;

export function LiveryEditor() {
  const [ok, setOk] = useState(can);
  useEffect(() => {
    if (can !== undefined) return;
    Link.get().ready.then((r) => {
      can = !!r.webgl;
      setOk(can);
    });
  }, []);
  if (ok === undefined) return html`<div class="ed-studio"><div class="ed-studio-note">${tr('Loading...')}</div></div>`;
  return ok ? html`<${LakStudio} />` : html`<${LiveryPage} />`;
}

// The 3D view's worker: the engine (`engine.js`) on an OffscreenCanvas of its own, talking to
// the page by messages (`link.js`). It says at once whether it can draw: no WebGL2 here, and
// the page runs the engine on its own thread instead.

import { Engine } from './engine.js';

const post = (m, transfer) => self.postMessage(m, transfer || []);
let engine;
try {
  engine = new Engine(post, () => new OffscreenCanvas(16, 16), self.requestAnimationFrame ? (f) => self.requestAnimationFrame(f) : undefined);
  if (engine.start()) post({ kind: 'ready', webgl: true, info: engine.info() });
  else post({ kind: 'ready', webgl: false, detail: 'no WebGL2 context in a worker' });
} catch (x) {
  post({ kind: 'ready', webgl: false, detail: String(x?.message || x) });
}

self.onmessage = (e) => {
  try {
    engine?.handle(e.data);
  } catch (x) {
    post({ kind: 'error', viewer: e.data?.viewer ?? 0, load: 0, reason: 'failed', detail: String(x?.stack || x) });
  }
};

// the livery studio's painting engine joins the same context when it is asked for
self.addEventListener('message', async (e) => {
  if (e.data?.kind === 'lakLoad' && engine && !engine.studio) {
    const { attach } = await import('./lak/canvas.js');
    attach(engine);
    for (const m of engine.lakQueue || []) engine.handle(m);
    engine.lakQueue = [];
  }
});

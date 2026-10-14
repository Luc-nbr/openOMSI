// Where the 3D view's bytes come from: the launcher's own protocol (`bytes/<cmd>?args=…`,
// `launcher/src-tauri/src/protocol.rs`), which the renderer's worker can fetch itself - the
// window's commands are not there. Custom protocols are `omsi://localhost/` on macOS and Linux
// and `http://omsi.localhost/` on Windows and Android (as `api.js` has it).
//
// Nothing here touches `window`: a worker imports it too.

const base = /windows|android/i.test(globalThis.navigator?.userAgent || '') ? 'http://omsi.localhost/' : 'omsi://localhost/';

/** The address of a byte command with its arguments. */
export function bytesUrl(cmd, args = {}) {
  return `${base}bytes/${cmd}?args=${encodeURIComponent(JSON.stringify(args))}`;
}

/** The bytes of a byte command; rejects with what the launcher said. */
export async function fetchBytes(cmd, args) {
  const r = await fetch(bytesUrl(cmd, args));
  if (!r.ok) throw new Error(`${cmd}: ${(await r.text().catch(() => '')) || r.status}`);
  return r.arrayBuffer();
}

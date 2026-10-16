// Signing in with Steam, for Supabase Auth, which has no Steam provider of its own.
//
// Steam signs players in with OpenID 2.0, not OAuth: no client secret, no e-mail. So:
//   1. GET  /steam-auth?to=<page>            -> off to Steam's sign-in page;
//   2. Steam sends the player back to        /steam-auth?step=back&to=<page>&openid.*=...;
//      the answer is checked with Steam itself (check_authentication), never trusted as it came;
//   3. the Supabase user for that Steam id is found or made (an address of our own,
//      steam-<id>@steam.omsihub.com, confirmed, never mailed; the Steam id in app_metadata,
//      which the player cannot change), and a one-time sign-in link is made for it;
//   4. the player goes through that link, and Supabase hands the session to <page>
//      (#access_token=...), the same way as after Google or Discord.
// <page> must be one of PAGES, or the app's one-shot listener on this computer
// (http://127.0.0.1:<port>/callback, LOOPBACK: that exact host and path, any port, nothing after it):
// no open redirect. Something wrong ends on <page>#error_code=steam_failed.
//
// The secret key comes from the function's own environment (SUPABASE_SECRET_KEYS), never from a
// request, and nothing it gives is returned except the one-time link.

export const STEAM = 'https://steamcommunity.com/openid/login';
const NS = 'http://specs.openid.net/auth/2.0';
const SELECT = 'http://specs.openid.net/auth/2.0/identifier_select';
const ID = /^https:\/\/steamcommunity\.com\/openid\/id\/(\d{17})$/;
/** Where a session may be handed over. */
export const PAGES = ['https://omsihub.com/account.html', 'https://www.omsihub.com/account.html', 'http://localhost:5182/account.html', 'http://127.0.0.1:5182/account.html'];
/** The app's listener (crates/omsihub-online/src/oauth.rs): 127.0.0.1 only, the path fixed. */
export const LOOPBACK = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/callback$/;
/** Whether a session may be handed over to `page`. */
export function allowed(page) {
  if (PAGES.includes(page)) return true;
  const m = LOOPBACK.exec(page);
  return !!m && Number(m[1]) <= 65535;
}

const go = (url) => new Response(null, { status: 302, headers: { location: url, 'cache-control': 'no-store', 'referrer-policy': 'no-referrer' } });
const failed = (page, why) => {
  console.log(`steam-auth: ${why}`);
  return go(`${page}#error=access_denied&error_code=steam_failed`);
};

/**
 * The function. `env`: { url (the project's public URL), key (the secret key) }; `fetcher`: fetch
 * (a stand-in in the tests).
 */
export async function handle(req, env, fetcher = fetch) {
  if (req.method !== 'GET') return new Response('Only GET', { status: 405 });
  const q = new URL(req.url).searchParams;
  const page = q.get('to') || PAGES[0];
  if (!allowed(page)) return new Response('This page may not sign in with Steam.', { status: 400 });
  const self = `${env.url}/functions/v1/steam-auth`;
  const back = `${self}?step=back&to=${encodeURIComponent(page)}`;

  // 1. off to Steam
  if (q.get('step') !== 'back') {
    const p = new URLSearchParams({
      'openid.ns': NS,
      'openid.mode': 'checkid_setup',
      'openid.return_to': back,
      'openid.realm': new URL(self).origin,
      'openid.identity': SELECT,
      'openid.claimed_id': SELECT,
    });
    return go(`${STEAM}?${p}`);
  }

  // 2. back from Steam: is it really Steam's answer, for us?
  if (q.get('openid.mode') !== 'id_res') return failed(page, `mode ${q.get('openid.mode')}`);
  if (q.get('openid.op_endpoint') !== STEAM) return failed(page, 'op_endpoint');
  if (q.get('openid.return_to') !== back) return failed(page, 'return_to');
  const m = ID.exec(q.get('openid.claimed_id') || '');
  if (!m || q.get('openid.identity') !== q.get('openid.claimed_id')) return failed(page, 'claimed_id');
  const steamId = m[1];
  const check = new URLSearchParams();
  for (const [k, v] of q) if (k.startsWith('openid.')) check.set(k, v);
  check.set('openid.mode', 'check_authentication');
  let valid = false;
  try {
    const r = await fetcher(STEAM, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: check.toString() });
    valid = r.ok && /(^|\n)is_valid:true(\n|$)/.test(await r.text());
  } catch {}
  if (!valid) return failed(page, 'not valid');

  // 3. the Supabase user for this Steam id, and a one-time link for it
  const admin = (path, body) =>
    fetcher(`${env.url}/auth/v1/admin${path}`, { method: 'POST', headers: { apikey: env.key, 'content-type': 'application/json' }, body: JSON.stringify(body) });
  const email = `steam-${steamId}@steam.omsihub.com`;
  const made = await admin('/users', { email, email_confirm: true, app_metadata: { provider: 'steam', providers: ['steam'], steam_id: steamId } });
  if (!made.ok && made.status !== 422) return failed(page, `create ${made.status}`);
  const link = await admin('/generate_link', { type: 'magiclink', email, redirect_to: page });
  if (!link.ok) return failed(page, `link ${link.status}`);
  const data = await link.json();
  const action = data.action_link || data.properties?.action_link;
  if (!action || !action.startsWith(`${env.url}/auth/v1/verify?`)) return failed(page, 'no link');

  // 4. through the link, Supabase hands the session over to the page
  return go(action);
}

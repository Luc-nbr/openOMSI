// node online/supabase/functions/steam-auth/test.mjs
import assert from 'node:assert/strict';
import { allowed, handle, STEAM } from './steam.mjs';

const env = { url: 'https://proj.supabase.co', key: 'sb_secret_test' };
const PAGE = 'https://omsihub.com/account.html';
const SELF = `${env.url}/functions/v1/steam-auth`;
const BACK = `${SELF}?step=back&to=${encodeURIComponent(PAGE)}`;
const ID = '76561198000000001';
const get = (qs) => new Request(`${SELF}?${qs}`);
const answer = (over = {}) =>
  new URLSearchParams({
    step: 'back',
    to: PAGE,
    'openid.ns': 'http://specs.openid.net/auth/2.0',
    'openid.mode': 'id_res',
    'openid.op_endpoint': STEAM,
    'openid.claimed_id': `https://steamcommunity.com/openid/id/${ID}`,
    'openid.identity': `https://steamcommunity.com/openid/id/${ID}`,
    'openid.return_to': BACK,
    'openid.response_nonce': '2026-10-16T12:00:00Zabc',
    'openid.assoc_handle': '1234567890',
    'openid.signed': 'signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle',
    'openid.sig': 'c2lnbmF0dXJl',
    ...over,
  }).toString();

/** A stand-in for Steam and Supabase's admin API; records the calls. */
function world({ valid = true, exists = false, linkOk = true } = {}) {
  const calls = [];
  const fetcher = async (url, init = {}) => {
    calls.push({ url, init });
    if (url === STEAM) {
      const body = new URLSearchParams(init.body);
      assert.equal(body.get('openid.mode'), 'check_authentication');
      assert.equal(body.get('openid.sig'), 'c2lnbmF0dXJl');
      return new Response(`ns:http://specs.openid.net/auth/2.0\nis_valid:${valid}\n`);
    }
    assert.equal(init.headers.apikey, env.key);
    if (url.endsWith('/admin/users')) return new Response('{}', { status: exists ? 422 : 200 });
    if (url.endsWith('/admin/generate_link')) {
      if (!linkOk) return new Response('{}', { status: 500 });
      const b = JSON.parse(init.body);
      return Response.json({ action_link: `${env.url}/auth/v1/verify?token=t0k&type=magiclink&redirect_to=${encodeURIComponent(b.redirect_to)}` });
    }
    throw new Error(`unexpected ${url}`);
  };
  return { calls, fetcher };
}

let n = 0;
const test = async (name, f) => {
  await f();
  n++;
  console.log(`ok ${n} - ${name}`);
};

await test('the start goes to Steam with our return address', async () => {
  const r = await handle(get(`to=${encodeURIComponent(PAGE)}`), env, world().fetcher);
  assert.equal(r.status, 302);
  const to = new URL(r.headers.get('location'));
  assert.equal(`${to.origin}${to.pathname}`, STEAM);
  assert.equal(to.searchParams.get('openid.return_to'), BACK);
  assert.equal(to.searchParams.get('openid.realm'), env.url);
});

await test('a page not on the list is refused (no open redirect)', async () => {
  const r = await handle(get(`to=${encodeURIComponent('https://evil.example/x')}`), env, world().fetcher);
  assert.equal(r.status, 400);
});

await test("the app's listener on 127.0.0.1 may sign in, on any port, with the path fixed", async () => {
  for (const ok of ['http://127.0.0.1:49152/callback', 'http://127.0.0.1:1/callback', 'http://127.0.0.1:65535/callback']) assert.equal(allowed(ok), true, ok);
  for (const no of [
    'http://127.0.0.1:65536/callback',
    'http://127.0.0.1:0/callback',
    'http://127.0.0.1:080/callback',
    'http://127.0.0.1/callback',
    'http://127.0.0.1:5000/callback/',
    'http://127.0.0.1:5000/callback?x=1',
    'http://127.0.0.1:5000/callback#x',
    'http://127.0.0.1:5000/other',
    'https://127.0.0.1:5000/callback',
    'http://localhost:5000/callback',
    'http://127.0.0.1.evil.example:5000/callback',
    'http://127.0.0.1:5000@evil.example/callback',
    'http://evil.example/http://127.0.0.1:5000/callback',
    'http://127.0.0.2:5000/callback',
  ])
    assert.equal(allowed(no), false, no);
  const page = 'http://127.0.0.1:50123/callback';
  const r = await handle(get(`to=${encodeURIComponent(page)}`), env, world().fetcher);
  assert.equal(r.status, 302);
  const back = new URL(r.headers.get('location')).searchParams.get('openid.return_to');
  assert.equal(back, `${SELF}?step=back&to=${encodeURIComponent(page)}`);
  // (and back from Steam, the one-time link is made for it)
  const w = world();
  const done = await handle(get(answer({ to: page, 'openid.return_to': back })), env, w.fetcher);
  assert.match(done.headers.get('location'), /\/auth\/v1\/verify\?/);
  assert.equal(JSON.parse(w.calls[2].init.body).redirect_to, page);
  const bad = await handle(get(`to=${encodeURIComponent('http://127.0.0.1:50123/elsewhere')}`), env, world().fetcher);
  assert.equal(bad.status, 400);
});

await test("a checked answer makes the user and hands over Supabase's one-time link", async () => {
  const w = world();
  const r = await handle(get(answer()), env, w.fetcher);
  assert.equal(r.status, 302);
  assert.match(r.headers.get('location'), /^https:\/\/proj\.supabase\.co\/auth\/v1\/verify\?token=t0k&type=magiclink/);
  const made = JSON.parse(w.calls[1].init.body);
  assert.equal(made.email, `steam-${ID}@steam.omsihub.com`);
  assert.equal(made.email_confirm, true);
  assert.equal(made.app_metadata.steam_id, ID);
  assert.equal(JSON.parse(w.calls[2].init.body).redirect_to, PAGE);
});

await test('a player who has been here before signs in again', async () => {
  const r = await handle(get(answer()), env, world({ exists: true }).fetcher);
  assert.match(r.headers.get('location'), /\/auth\/v1\/verify\?/);
});

for (const [name, over, w] of [
  ['Steam says the answer is not valid', {}, { valid: false }],
  ['a forged endpoint', { 'openid.op_endpoint': 'https://evil.example/openid/login' }, {}],
  ['an answer meant for another site', { 'openid.return_to': 'https://evil.example/back' }, {}],
  ['a claimed id that is not a Steam id', { 'openid.claimed_id': 'https://evil.example/id/1', 'openid.identity': 'https://evil.example/id/1' }, {}],
  ['identity and claimed id differ', { 'openid.identity': 'https://steamcommunity.com/openid/id/76561198000000002' }, {}],
  ['a cancelled sign-in', { 'openid.mode': 'cancel' }, {}],
  ['no link from Supabase', {}, { linkOk: false }],
]) {
  await test(`refused: ${name}`, async () => {
    const r = await handle(get(answer(over)), env, world(w).fetcher);
    assert.equal(r.status, 302);
    assert.equal(r.headers.get('location'), `${PAGE}#error=access_denied&error_code=steam_failed`);
  });
}

await test('only GET', async () => {
  const r = await handle(new Request(SELF, { method: 'POST' }), env, world().fetcher);
  assert.equal(r.status, 405);
});
console.log(`all ${n} passed`);

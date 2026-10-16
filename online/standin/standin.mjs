// A stand-in for the Supabase project `omsi-hub-online`, on this computer only, to try the app's and
// the website's online flows without touching the real project (docs/omsihub/ONLINE.md).
//
//   node online/standin/standin.mjs [port]        (default 54321; Node 18 or later, no packages)
//
// It answers as Supabase does, in memory, for what Omsi-Hub uses:
// - Auth (`/auth/v1`): signup, token (password, refresh_token, refresh tokens rotated and a reused
//   one refused), resend, verify (email, signup, recovery), recover, GET/PUT user, logout. Email
//   confirmation is on; the codes it would mail are printed here and read back by tests at
//   `GET /__mail?email=<address>`. Token also takes `grant_type=pkce`.
// - Google (`GET /auth/v1/authorize?provider=google&redirect_to=…&code_challenge=…`): no Google
//   page; the player "signs in" at once as google-player@google.standin and is sent to redirect_to
//   with `?code=`, exchanged at `POST /token?grant_type=pkce` with the verifier (S256 checked, the
//   code good once, for five minutes). redirect_to must match STANDIN_REDIRECTS (Supabase's globs,
//   comma-separated; default `http://127.0.0.1:*/callback*`), as Supabase's allow-list.
//   `&standin_answer=deny` plays a player who says no (`?error=access_denied`).
// - The database (`/rest/v1`): the functions of online/supabase/schema.sql with the same checks and
//   answers (the privacy statement's `accept_privacy` and `my_privacy` among them), and the tables' reads with the same row-level rules (profiles, invites).
// - Storage (`/storage/v1`): the private bucket `saves`, each player only in users/<own id>/, only
//   slot1 and slot2 written, 25 MB a file.
//
// Every request needs the `apikey` header (STANDIN_KEY, default `sb_publishable_standin`). Access
// tokens are HS256 JWTs signed with a key made at start, valid STANDIN_TTL seconds (default 3600; a
// short one tries the renewal). CORS is open so the website can use it (`?supabase=`).

import http from 'node:http';
import crypto from 'node:crypto';

const PORT = Number(process.argv[2] || process.env.STANDIN_PORT || 54321);
const KEY = process.env.STANDIN_KEY || 'sb_publishable_standin';
const TTL = Number(process.env.STANDIN_TTL || 3600);
const SECRET = crypto.randomBytes(32);
const MAX_FILE = 26214400;
// (as the schema has them: profiles.session_code and invites.session_code)
const SESSION_MAX = Number(process.env.STANDIN_SESSION_MAX || 32);
const REDIRECTS = (process.env.STANDIN_REDIRECTS || 'http://127.0.0.1:*/callback*').split(',').map((g) => g.trim()).filter(Boolean);

// --- the data -------------------------------------------------------------------------------------

const users = new Map(); // id -> { id, email, password, confirmed, meta, app, created_at }
const flows = new Map(); // PKCE code -> { user, challenge, until }
const codes = new Map(); // email -> { code, kind }
const refreshes = new Map(); // token -> { user, session, used }
const profiles = new Map(); // user id -> row
const friendships = []; // { requester, addressee, status, created_at }
const invites = []; // { id, from_user, to_user, session_code, created_at, expires_at }
const files = new Map(); // 'users/<id>/slotN/<name>' -> { bytes, type, id, at }
const consents = []; // { user_id, version, accepted_at } (privacy_consents)
let inviteId = 0;

const now = () => Date.now();
const iso = (t = now()) => new Date(t).toISOString();
const userByEmail = (e) => [...users.values()].find((u) => u.email === String(e || '').toLowerCase());

// --- tokens ---------------------------------------------------------------------------------------

const b64 = (b) => Buffer.from(b).toString('base64url');
function sign(user, session) {
  const exp = Math.floor(now() / 1000) + TTL;
  const head = b64(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const body = b64(JSON.stringify({ sub: user.id, email: user.email, role: 'authenticated', aud: 'authenticated', exp, session_id: session, app_metadata: appOf(user) }));
  const sig = b64(crypto.createHmac('sha256', SECRET).update(`${head}.${body}`).digest());
  return { token: `${head}.${body}.${sig}`, exp };
}
/** The user of a request's bearer, or null (none, forged, expired, or the user gone). */
function bearer(req) {
  const t = (req.headers.authorization || '').replace(/^Bearer /, '');
  const [h, b, s] = t.split('.');
  if (!s) return null;
  const want = b64(crypto.createHmac('sha256', SECRET).update(`${h}.${b}`).digest());
  if (want.length !== s.length || !crypto.timingSafeEqual(Buffer.from(want), Buffer.from(s))) return null;
  const c = JSON.parse(Buffer.from(b, 'base64url').toString());
  if (c.exp * 1000 <= now()) return null;
  const u = users.get(c.sub);
  return u ? { user: u, session: c.session_id } : null;
}
function session(user, sessionId = crypto.randomUUID()) {
  const refresh = crypto.randomBytes(12).toString('hex');
  refreshes.set(refresh, { user: user.id, session: sessionId, used: false });
  const a = sign(user, sessionId);
  return { access_token: a.token, token_type: 'bearer', expires_in: TTL, expires_at: a.exp, refresh_token: refresh, user: userJson(user) };
}
const appOf = (u) => u.app || { provider: 'email', providers: ['email'] };
const userJson = (u) => ({ id: u.id, aud: 'authenticated', role: 'authenticated', email: u.email, email_confirmed_at: u.confirmed ? u.created_at : null, app_metadata: appOf(u), user_metadata: u.meta, identities: appOf(u).providers.map((provider) => ({ provider })), created_at: u.created_at });
function mail(email, kind) {
  const code = String(crypto.randomInt(0, 1e6)).padStart(6, '0');
  codes.set(email, { code, kind });
  console.log(`[mail] ${kind} code for ${email}: ${code}`);
}

// --- answers --------------------------------------------------------------------------------------

function send(res, status, body, headers = {}) {
  const raw = body === undefined ? '' : Buffer.isBuffer(body) ? body : JSON.stringify(body);
  res.writeHead(status, { 'access-control-allow-origin': '*', ...(Buffer.isBuffer(body) ? {} : { 'content-type': 'application/json' }), ...headers });
  res.end(raw);
}
const authError = (res, status, code, msg) => send(res, status, { code: status, error_code: code, msg });
const pgError = (res, status, code, message) => send(res, status, { code, message, details: null, hint: null });
const storageError = (res, code, error, message) => send(res, 400, { statusCode: String(code), error, message });
class Pg extends Error {
  constructor(code, message, status = 400) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

// --- the database's functions (online/supabase/schema.sql) --------------------------------------

const NAME = /^[A-Za-z0-9_. -]+$/;
const ALPHA = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
const pair = (a, b) => friendships.find((f) => (f.requester === a && f.addressee === b) || (f.requester === b && f.addressee === a));
const isFriend = (a, b) => pair(a, b)?.status === 'accepted';
const hasPending = (a, b) => pair(a, b)?.status === 'pending';
function checkName(n) {
  const v = String(n ?? '').trim();
  if (v.length < 3 || v.length > 24 || !NAME.test(v)) throw new Pg('23514', 'new row for relation "profiles" violates check constraint "profiles_display_name_check"');
  return v;
}
function uniqueName(v, me) {
  if ([...profiles.values()].some((p) => p.user_id !== me && p.display_name.toLowerCase() === v.toLowerCase())) throw new Pg('23505', 'duplicate key value violates unique constraint "profiles_name_ci"', 409);
}
function checkSession(s) {
  if (s != null && s.length > SESSION_MAX) throw new Pg('23514', 'new row violates check constraint (session_code)');
  return s;
}
const isUuid = (v) => /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(String(v));
function uuid(v) {
  if (!isUuid(v)) throw new Pg('22P02', `invalid input syntax for type uuid: "${v}"`);
  return v;
}
function deleteUserData(me) {
  for (let i = invites.length - 1; i >= 0; i--) if (invites[i].from_user === me || invites[i].to_user === me) invites.splice(i, 1);
  for (let i = friendships.length - 1; i >= 0; i--) if (friendships[i].requester === me || friendships[i].addressee === me) friendships.splice(i, 1);
  profiles.delete(me);
}

const FUNCTIONS = {
  create_profile(me, { p_name }) {
    if (!profiles.has(me)) {
      const name = checkName(p_name);
      uniqueName(name, me);
      let code;
      do code = Array.from({ length: 8 }, () => ALPHA[crypto.randomInt(0, ALPHA.length)]).join('');
      while ([...profiles.values()].some((p) => p.friend_code === code));
      profiles.set(me, { user_id: me, display_name: name, friend_code: code, status: 'offline', session_code: null, last_seen: iso(), created_at: iso(), on_board: false, points: 0, level: 1, duties: 0, minutes_driven: 0, km: 0, stats_at: null });
    }
    return profiles.get(me);
  },
  set_display_name(me, { p_name }) {
    const p = profiles.get(me);
    if (p) {
      const name = checkName(p_name);
      uniqueName(name, me);
      p.display_name = name;
    }
    return null;
  },
  heartbeat(me, { p_status, p_session }) {
    if (!['online', 'away', 'busy', 'offline'].includes(p_status)) throw new Pg('23514', 'violates check constraint "profiles_status_check"');
    const p = profiles.get(me);
    if (p) {
      p.status = p_status;
      p.session_code = checkSession(String(p_session ?? '').trim() || null);
      p.last_seen = iso();
    }
    return null;
  },
  find_player(me, { p_code }) {
    const p = [...profiles.values()].find((x) => x.friend_code === String(p_code ?? '').trim().toUpperCase());
    return p ? [{ user_id: p.user_id, display_name: p.display_name }] : [];
  },
  send_friend_request(me, { p_code }) {
    const other = [...profiles.values()].find((x) => x.friend_code === String(p_code ?? '').trim().toUpperCase())?.user_id;
    if (!other) return 'not_found';
    if (other === me) return 'self';
    if (!profiles.has(me)) return 'no_profile';
    if (friendships.filter((f) => f.requester === me && f.created_at > now() - 3600e3).length >= 20) return 'too_many';
    if (friendships.filter((f) => (f.requester === me || f.addressee === me) && f.status === 'accepted').length >= 500) return 'full';
    const f = pair(me, other);
    if (f) {
      if (f.status === 'blocked') return 'sent';
      if (f.status === 'accepted') return 'already';
      if (f.requester === other) {
        f.status = 'accepted';
        return 'accepted';
      }
      return 'sent';
    }
    friendships.push({ requester: me, addressee: other, status: 'pending', created_at: now() });
    return 'sent';
  },
  respond_friend_request(me, { p_from, p_accept }) {
    uuid(p_from);
    const i = friendships.findIndex((f) => f.requester === p_from && f.addressee === me && f.status === 'pending');
    if (i >= 0) {
      if (p_accept) friendships[i].status = 'accepted';
      else friendships.splice(i, 1);
    }
    return null;
  },
  remove_friend(me, { p_other }) {
    uuid(p_other);
    for (let i = friendships.length - 1; i >= 0; i--) {
      const f = friendships[i];
      if ((f.requester === me && f.addressee === p_other) || (f.requester === p_other && f.addressee === me && f.status !== 'blocked')) friendships.splice(i, 1);
    }
    for (let i = invites.length - 1; i >= 0; i--) {
      const v = invites[i];
      if ((v.from_user === me && v.to_user === p_other) || (v.from_user === p_other && v.to_user === me)) invites.splice(i, 1);
    }
    return null;
  },
  block_player(me, { p_other }) {
    uuid(p_other);
    if (p_other === me || !profiles.has(p_other)) return null;
    FUNCTIONS.remove_friend(me, { p_other });
    for (let i = friendships.length - 1; i >= 0; i--) if (pair(me, p_other) === friendships[i]) friendships.splice(i, 1);
    friendships.push({ requester: me, addressee: p_other, status: 'blocked', created_at: now() });
    return null;
  },
  send_invite(me, { p_to, p_session }) {
    uuid(p_to);
    if (!isFriend(me, p_to)) return 'not_friends';
    if (invites.filter((v) => v.from_user === me && Date.parse(v.created_at) > now() - 3600e3).length >= 30) return 'too_many';
    const code = String(p_session ?? '').trim();
    if (code.length < 1 || code.length > SESSION_MAX) throw new Pg('23514', 'new row for relation "invites" violates check constraint "invites_session_code_check"');
    for (let i = invites.length - 1; i >= 0; i--) if (invites[i].from_user === me && invites[i].to_user === p_to) invites.splice(i, 1);
    invites.push({ id: ++inviteId, from_user: me, to_user: p_to, session_code: code, created_at: iso(), expires_at: iso(now() + 15 * 60e3) });
    return 'sent';
  },
  dismiss_invite(me, { p_id }) {
    const i = invites.findIndex((v) => v.id === Number(p_id) && (v.to_user === me || v.from_user === me));
    if (i >= 0) invites.splice(i, 1);
    return null;
  },
  my_friends(me) {
    return friendships
      .filter((f) => f.requester === me || (f.addressee === me && f.status !== 'blocked'))
      .map((f) => {
        const p = profiles.get(f.requester === me ? f.addressee : f.requester);
        const ok = f.status === 'accepted';
        return { user_id: p.user_id, display_name: p.display_name, status: ok ? p.status : null, online: ok && Date.parse(p.last_seen) > now() - 120e3 && p.status !== 'offline', session_code: ok ? p.session_code : null, last_seen: ok ? p.last_seen : null, relation: f.status, incoming: f.addressee === me };
      });
  },
  delete_my_data(me) {
    deleteUserData(me);
    return null;
  },
  delete_my_account(me) {
    deleteUserData(me);
    // (the consents go with the user: `on delete cascade` from auth.users)
    for (let i = consents.length - 1; i >= 0; i--) if (consents[i].user_id === me) consents.splice(i, 1);
    users.delete(me);
    for (const [t, r] of refreshes) if (r.user === me) refreshes.delete(t);
    return null;
  },
  accept_privacy(me, { p_version }) {
    if (!Number.isInteger(p_version) || p_version < 1 || p_version > 1000) throw new Pg('22023', 'bad version');
    if (!consents.some((c) => c.user_id === me && c.version === p_version)) consents.push({ user_id: me, version: p_version, accepted_at: iso() });
    return FUNCTIONS.my_privacy(me);
  },
  my_privacy(me) {
    return Math.max(0, ...consents.filter((c) => c.user_id === me).map((c) => c.version));
  },
  set_leaderboard(me, { p_on }) {
    const p = profiles.get(me);
    if (p) p.on_board = !!p_on;
    return !!p;
  },
  report_stats(me, { p_points, p_level, p_duties, p_minutes, p_km }) {
    const n = (v, lo, hi) => Number.isInteger(v) && v >= lo && v <= hi;
    if (!(n(p_points, 0, 1e9) && n(p_level, 1, 1000) && n(p_duties, 0, 1e6) && n(p_minutes, 0, 1e8) && n(p_km, 0, 1e8))) return 'bad';
    const p = profiles.get(me);
    if (!p) return 'no_profile';
    if (p.stats_at && Date.parse(p.stats_at) > now() - 30e3) return 'too_soon';
    Object.assign(p, { points: p_points, level: p_level, duties: p_duties, minutes_driven: p_minutes, km: p_km, stats_at: iso() });
    return 'ok';
  },
  hub_leaderboard(_me, { p_limit }) {
    const lim = Math.min(Math.max(Number(p_limit ?? 50) || 50, 1), 100);
    return [...profiles.values()]
      .filter((p) => p.on_board)
      .sort((a, b) => b.points - a.points || String(a.stats_at).localeCompare(String(b.stats_at)) || a.display_name.toLowerCase().localeCompare(b.display_name.toLowerCase()))
      .slice(0, lim)
      .map((p, i) => ({ place: i + 1, display_name: p.display_name, level: p.level, points: p.points, duties: p.duties, minutes_driven: p.minutes_driven, km: p.km, online: p.status !== 'offline' && Date.parse(p.last_seen) > now() - 120e3 }));
  },
  hub_board_counts() {
    const all = [...profiles.values()];
    return [{ players: all.length, online: all.filter((p) => p.status !== 'offline' && Date.parse(p.last_seen) > now() - 120e3).length, on_board: all.filter((p) => p.on_board).length }];
  },
};
const ANON = new Set(['hub_leaderboard', 'hub_board_counts']);

// --- Google ----------------------------------------------------------------------------------------

/** Supabase's redirect globs (separators `.` and `/`): `**` anything, `*` and `?` no separator,
 * `[0-9]` / `[!a-z]` one character of (not of) a class. */
function glob(g) {
  let re = '';
  for (let i = 0; i < g.length; i++) {
    const end = g[i] === '[' ? g.indexOf(']', i + 2) : -1;
    if (end > 0) {
      const set = g.slice(i + 1, end);
      re += set.startsWith('!') ? `[^${set.slice(1).replace(/[\\\]^]/g, '\\$&')}]` : `[${set.replace(/[\\\]^]/g, '\\$&')}]`;
      i = end;
    } else if (g.startsWith('**', i)) (re += '.*'), i++;
    else if (g[i] === '*') re += '[^./]*';
    else if (g[i] === '?') re += '[^./]';
    else re += g[i].replace(/[\\^$.|+()[\]{}]/g, '\\$&');
  }
  return new RegExp(`^${re}$`);
}
const redirectAllowed = (url) => REDIRECTS.some((g) => glob(g).test(url));

/** The user Google signs in (made the first time, confirmed, without a password). */
function providerUser(email, app, meta = {}) {
  let u = userByEmail(email);
  if (!u) {
    u = { id: crypto.randomUUID(), email, password: null, confirmed: true, meta, app, created_at: iso() };
    users.set(u.id, u);
  }
  return u;
}
const go = (res, location) => send(res, 302, undefined, { location, 'cache-control': 'no-store' });
const withQuery = (url, q) => `${url}${url.includes('?') ? '&' : '?'}${q}`;

function authorize(res, q) {
  const provider = q.get('provider');
  if (provider !== 'google') return authError(res, 400, 'validation_failed', 'Unsupported provider: provider is not enabled');
  const to = q.get('redirect_to') || '';
  if (!redirectAllowed(to)) return authError(res, 400, 'validation_failed', `redirect_to is not on the allow-list: ${to}`);
  if (q.get('standin_answer') === 'deny') return go(res, withQuery(to, 'error=access_denied&error_code=provider_error&error_description=The+user+denied+access'));
  const challenge = q.get('code_challenge');
  if (!challenge || String(q.get('code_challenge_method')).toLowerCase() !== 's256') return authError(res, 400, 'validation_failed', 'the stand-in takes PKCE with S256 only');
  const u = providerUser('google-player@google.standin', { provider: 'google', providers: ['google'] }, { full_name: 'Google Player' });
  const code = crypto.randomUUID();
  flows.set(code, { user: u.id, challenge, until: now() + 5 * 60e3 });
  return go(res, withQuery(to, `code=${code}`));
}

// --- the routes -------------------------------------------------------------------------------------

async function auth(req, res, path, q, body) {
  const route = `${req.method} ${path}`;
  if (route === 'POST /signup') {
    const email = String(body.email || '').toLowerCase();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) return authError(res, 400, 'validation_failed', 'invalid email');
    if (String(body.password || '').length < 6) return authError(res, 422, 'weak_password', 'weak');
    const old = userByEmail(email);
    if (old) return send(res, 200, { ...userJson(old), identities: [] });
    const u = { id: crypto.randomUUID(), email, password: body.password, confirmed: false, meta: body.data || {}, created_at: iso() };
    users.set(u.id, u);
    mail(email, 'signup');
    return send(res, 200, userJson(u));
  }
  if (route === 'POST /token' && q.get('grant_type') === 'password') {
    const u = userByEmail(body.email);
    if (!u || u.password !== body.password) return authError(res, 400, 'invalid_credentials', 'Invalid login credentials');
    if (!u.confirmed) return authError(res, 400, 'email_not_confirmed', 'Email not confirmed');
    return send(res, 200, session(u));
  }
  if (route === 'POST /token' && q.get('grant_type') === 'pkce') {
    const f = flows.get(String(body.auth_code || ''));
    if (!f || f.until <= now()) return authError(res, 404, 'flow_state_not_found', 'invalid flow state, no valid flow state found');
    if (crypto.createHash('sha256').update(String(body.code_verifier || '')).digest('base64url') !== f.challenge) return authError(res, 400, 'bad_code_verifier', 'code challenge does not match previously saved code verifier');
    flows.delete(body.auth_code);
    const u = users.get(f.user);
    return u ? send(res, 200, session(u)) : authError(res, 404, 'user_not_found', 'User not found');
  }
  if (route === 'POST /token' && q.get('grant_type') === 'refresh_token') {
    const r = refreshes.get(body.refresh_token);
    if (!r) return authError(res, 400, 'refresh_token_not_found', 'Invalid Refresh Token: Refresh Token Not Found');
    if (r.used) return authError(res, 400, 'refresh_token_already_used', 'Invalid Refresh Token: Already Used');
    const u = users.get(r.user);
    if (!u) return authError(res, 400, 'user_not_found', 'User not found');
    r.used = true;
    return send(res, 200, session(u, r.session));
  }
  if (route === 'POST /resend') {
    const u = userByEmail(body.email);
    if (u && !u.confirmed) mail(u.email, 'signup');
    return send(res, 200, {});
  }
  if (route === 'POST /recover') {
    const u = userByEmail(body.email);
    if (u) mail(u.email, 'recovery');
    return send(res, 200, {});
  }
  if (route === 'POST /verify') {
    const u = userByEmail(body.email);
    const c = u && codes.get(u.email);
    const kinds = body.type === 'recovery' ? ['recovery'] : ['signup'];
    if (!c || c.code !== String(body.token) || !kinds.includes(c.kind)) return authError(res, 403, 'otp_expired', 'Token has expired or is invalid');
    codes.delete(u.email);
    u.confirmed = true;
    return send(res, 200, session(u));
  }
  const who = bearer(req);
  if (!who) return authError(res, 401, 'bad_jwt', 'invalid JWT');
  if (route === 'GET /user') return send(res, 200, userJson(who.user));
  if (route === 'PUT /user') {
    if (body.password !== undefined) {
      if (String(body.password).length < 6) return authError(res, 422, 'weak_password', 'weak');
      if (body.password === who.user.password) return authError(res, 422, 'same_password', 'New password should be different from the old password.');
      who.user.password = body.password;
    }
    return send(res, 200, userJson(who.user));
  }
  if (route === 'POST /logout') {
    const scope = q.get('scope') || 'global';
    for (const [t, r] of refreshes) {
      if (r.user !== who.user.id) continue;
      if (scope === 'global' || (scope === 'local' && r.session === who.session) || (scope === 'others' && r.session !== who.session)) refreshes.delete(t);
    }
    return send(res, 204);
  }
  return authError(res, 404, 'not_found', 'no such route');
}

function rest(req, res, path, q, body) {
  const who = bearer(req);
  if (req.headers.authorization && !who) return pgError(res, 401, 'PGRST301', 'JWT expired');
  const me = who?.user.id;
  const rpc = path.match(/^\/rpc\/(\w+)$/);
  if (rpc && req.method === 'POST') {
    const f = FUNCTIONS[rpc[1]];
    if (!f) return pgError(res, 404, 'PGRST202', `Could not find the function public.${rpc[1]}`);
    if (!me && !ANON.has(rpc[1])) return pgError(res, 401, '42501', `permission denied for function ${rpc[1]}`);
    try {
      const out = f(me, body || {});
      return out === null ? send(res, 204) : send(res, 200, out);
    } catch (e) {
      if (e instanceof Pg) return pgError(res, e.status, e.code, e.message);
      throw e;
    }
  }
  if (req.method === 'GET' && (path === '/profiles' || path === '/invites')) {
    if (!me) return pgError(res, 401, '42501', `permission denied for table ${path.slice(1)}`);
    let rows = path === '/profiles'
      ? [...profiles.values()].filter((p) => p.user_id === me || isFriend(me, p.user_id) || hasPending(me, p.user_id))
      : invites.filter((v) => (v.to_user === me && Date.parse(v.expires_at) > now()) || v.from_user === me);
    for (const [k, v] of q) if (v.startsWith('eq.')) rows = rows.filter((r) => String(r[k]) === v.slice(3));
    const cols = (q.get('select') || '*').split(',');
    if (!cols.includes('*')) rows = rows.map((r) => Object.fromEntries(cols.map((c) => [c, r[c]])));
    return send(res, 200, rows.slice(0, Number(q.get('limit') || 1000)));
  }
  return pgError(res, 404, 'PGRST205', 'not found');
}

function storage(req, res, path, body, raw) {
  const who = bearer(req);
  if (req.headers.authorization && !who) return storageError(res, 403, 'InvalidJWT', 'jwt expired');
  if (!who) return storageError(res, 403, 'Unauthorized', 'new row violates row-level security policy');
  const own = (p) => p.split('/')[0] === 'users' && p.split('/')[1] === who.user.id;
  const slot = (p) => ['slot1', 'slot2'].includes(p.split('/')[2]);
  let m;
  if ((m = path.match(/^\/object\/authenticated\/saves\/(.+)$/)) && req.method === 'GET') {
    const f = own(m[1]) && files.get(m[1]);
    if (!f) return storageError(res, 404, 'not_found', 'Object not found');
    return send(res, 200, f.bytes, { 'content-type': f.type });
  }
  if (path === '/object/list/saves' && req.method === 'POST') {
    const prefix = String(body.prefix || '').replace(/\/$/, '') + '/';
    const seen = new Map();
    for (const [k, f] of files) {
      if (!own(k) || !k.startsWith(prefix)) continue;
      const rest = k.slice(prefix.length);
      const name = rest.split('/')[0];
      seen.set(name, rest.includes('/') ? { name, id: null } : { name, id: f.id, updated_at: iso(f.at), metadata: { size: f.bytes.length, mimetype: f.type } });
    }
    return send(res, 200, [...seen.values()].slice(0, Number(body.limit || 100)));
  }
  if (path === '/object/saves' && req.method === 'DELETE') {
    const gone = [];
    for (const p of body.prefixes || []) if (own(p) && files.delete(p)) gone.push({ name: p, bucket_id: 'saves' });
    return send(res, 200, gone);
  }
  if ((m = path.match(/^\/object\/saves\/(.+)$/)) && (req.method === 'POST' || req.method === 'PUT')) {
    const p = m[1];
    if (!own(p) || !slot(p)) return storageError(res, 403, 'Unauthorized', 'new row violates row-level security policy');
    if (raw.length > MAX_FILE) return storageError(res, 413, 'Payload too large', 'The object exceeded the maximum allowed size');
    const upsert = req.method === 'PUT' || req.headers['x-upsert'] === 'true';
    if (files.has(p) && !upsert) return storageError(res, 409, 'Duplicate', 'The resource already exists');
    files.set(p, { bytes: raw, type: req.headers['content-type'] || 'application/octet-stream', id: crypto.randomUUID(), at: now() });
    return send(res, 200, { Key: `saves/${p}`, Id: files.get(p).id });
  }
  return storageError(res, 404, 'not_found', 'no such route');
}

http
  .createServer(async (req, res) => {
    if (req.method === 'OPTIONS') {
      res.writeHead(204, { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'GET,POST,PUT,PATCH,DELETE', 'access-control-allow-headers': 'apikey,authorization,content-type,x-upsert,cache-control,prefer', 'access-control-max-age': '600' });
      return res.end();
    }
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const raw = Buffer.concat(chunks);
    const url = new URL(req.url, 'http://x');
    const path = url.pathname;
    try {
      if (path === '/__mail') return send(res, 200, codes.get(String(url.searchParams.get('email')).toLowerCase()) || {});
      // (the browser's: no apikey, as on Supabase)
      if (req.method === 'GET' && path === '/auth/v1/authorize') return authorize(res, url.searchParams);
      if (req.headers.apikey !== KEY) return send(res, 401, { message: 'No API key found in request', hint: 'No `apikey` request header or url param was found.' });
      let body = {};
      if (raw.length && String(req.headers['content-type'] || '').includes('json')) body = JSON.parse(raw.toString());
      if (path.startsWith('/auth/v1/')) return await auth(req, res, path.slice(8), url.searchParams, body);
      if (path.startsWith('/rest/v1/')) return rest(req, res, path.slice(8), url.searchParams, body);
      if (path.startsWith('/storage/v1/')) return storage(req, res, path.slice(11), body, raw);
      send(res, 404, { message: 'not found' });
    } catch (e) {
      console.error(e);
      send(res, 500, { message: 'stand-in error' });
    } finally {
      if (process.env.STANDIN_LOG) console.log(`${req.method} ${req.url} -> ${res.statusCode}`);
    }
  })
  .listen(PORT, '127.0.0.1', () => console.log(`Omsi-Hub online stand-in on http://127.0.0.1:${PORT} (apikey ${KEY}, tokens ${TTL} s)`));

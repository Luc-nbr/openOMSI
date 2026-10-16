// Omsi-Hub's public leaderboard: a Neon Function (runtime nodejs24) with no npm dependencies.
//
// The website (any browser, no account) calls GET / and gets the players who chose to stand on
// the board, with the counts beside it:
//
//   { ok: true, at, counts: { players, online, on_board }, board: [{ place, name, level, points,
//     duties, minutes, km, online }] }
//
// It reads through `hub_leaderboard` and `hub_board_counts` (online/leaderboard.sql), which only
// ever give what a player made public. The query is fixed; the only input is `?limit=` (1-100).
// The answer is cached for 30 s, here and in the browser, so the database is asked at most twice
// a minute per instance whatever the traffic.

export const CACHE_MS = 30 * 1000;
export const MAX_LIMIT = 100;
export const DEFAULT_LIMIT = 50;

const CORS = {
  'access-control-allow-origin': '*',
  'access-control-allow-methods': 'GET, OPTIONS',
  'access-control-max-age': '86400',
};

export const json = (status, obj, extra = {}) =>
  new Response(JSON.stringify(obj), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', ...CORS, ...extra },
  });

/** `?limit=` as a whole number in 1..MAX_LIMIT, else the default. */
export function parseLimit(v) {
  const n = Number.parseInt(v ?? '', 10);
  return Number.isFinite(n) ? Math.min(MAX_LIMIT, Math.max(1, n)) : DEFAULT_LIMIT;
}

/** A row of `hub_leaderboard` as the website gets it. */
export const shapeRow = (r) => ({
  place: Number(r.place),
  name: String(r.display_name),
  level: Number(r.level),
  points: Number(r.points),
  duties: Number(r.duties),
  minutes: Number(r.minutes_driven),
  km: Number(r.km),
  online: r.online === true || r.online === 't' || r.online === 'true',
});

/** One query over Neon's SQL-over-HTTP endpoint (what @neondatabase/serverless uses). */
export function sqlOverHttp(databaseUrl, fetchImpl = fetch) {
  const host = new URL(databaseUrl).hostname;
  return async (query, params = []) => {
    const res = await fetchImpl(`https://${host}/sql`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'neon-connection-string': databaseUrl, 'neon-array-mode': 'false' },
      body: JSON.stringify({ query, params }),
    });
    if (!res.ok) throw new Error(`sql ${res.status}`);
    return (await res.json()).rows;
  };
}

export function createHandler({ sql, now = () => Date.now(), log = () => {} }) {
  const cache = new Map(); // limit -> { at, body }
  return async function handle(request) {
    if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: CORS });
    if (request.method !== 'GET') return json(405, { ok: false, error: 'method' }, { allow: 'GET, OPTIONS' });
    const url = new URL(request.url);
    if (url.pathname !== '/' && url.pathname !== '') return json(404, { ok: false, error: 'not_found' });
    const limit = parseLimit(url.searchParams.get('limit'));
    const hit = cache.get(limit);
    if (hit && now() - hit.at < CACHE_MS) return json(200, hit.body, { 'cache-control': 'public, max-age=30' });
    try {
      const [rows, counts] = await Promise.all([
        sql('select * from public.hub_leaderboard($1)', [limit]),
        sql('select * from public.hub_board_counts()'),
      ]);
      const c = counts[0] || {};
      const body = {
        ok: true,
        at: new Date(now()).toISOString(),
        counts: { players: Number(c.players || 0), online: Number(c.online || 0), on_board: Number(c.on_board || 0) },
        board: rows.map(shapeRow),
      };
      cache.set(limit, { at: now(), body });
      return json(200, body, { 'cache-control': 'public, max-age=30' });
    } catch (e) {
      log({ event: 'error', message: String(e.message || e).slice(0, 100) });
      if (hit) return json(200, hit.body, { 'cache-control': 'public, max-age=10' });
      return json(503, { ok: false, error: 'unavailable' });
    }
  };
}

// ---------------------------------------------------------------------------------------------
// I/O

const logLine = (o) => console.log(JSON.stringify({ fn: 'board', ...o }));
let handler = null;
if (typeof process !== 'undefined' && process.env?.DATABASE_URL) {
  handler = createHandler({ sql: sqlOverHttp(process.env.DATABASE_URL), log: logLine });
}

export default {
  async fetch(request) {
    if (!handler) {
      logLine({ event: 'misconfigured' });
      return json(503, { ok: false, error: 'unavailable' });
    }
    return handler(request);
  },
};

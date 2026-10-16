import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHandler, parseLimit, shapeRow, sqlOverHttp, CACHE_MS } from './index.mjs';

const ROWS = [
  { place: '1', display_name: 'Bert', level: 6, points: '5400', duties: 210, minutes_driven: 3100, km: 410, online: false },
  { place: '2', display_name: 'Anna', level: 3, points: '1908', duties: 79, minutes_driven: 701, km: 66, online: 't' },
];
const COUNTS = [{ players: '3', online: '2', on_board: '2' }];

function fakeSql() {
  const calls = [];
  const sql = async (q, p) => {
    calls.push([q, p]);
    return q.includes('hub_leaderboard') ? ROWS : COUNTS;
  };
  return { sql, calls };
}

const get = (h, path = '/') => h(new Request(`https://board.example${path}`));

test('limit is clamped to 1..100, default 50', () => {
  assert.equal(parseLimit(null), 50);
  assert.equal(parseLimit('abc'), 50);
  assert.equal(parseLimit('0'), 1);
  assert.equal(parseLimit('7'), 7);
  assert.equal(parseLimit('5000'), 100);
});

test('a row keeps only what is public', () => {
  assert.deepEqual(shapeRow(ROWS[1]), { place: 2, name: 'Anna', level: 3, points: 1908, duties: 79, minutes: 701, km: 66, online: true });
});

test('GET answers the board with CORS and caches it', async () => {
  let t = 1_000_000;
  const { sql, calls } = fakeSql();
  const h = createHandler({ sql, now: () => t });
  const res = await get(h, '/?limit=10');
  assert.equal(res.status, 200);
  assert.equal(res.headers.get('access-control-allow-origin'), '*');
  const body = await res.json();
  assert.equal(body.ok, true);
  assert.deepEqual(body.counts, { players: 3, online: 2, on_board: 2 });
  assert.equal(body.board.length, 2);
  assert.equal(body.board[0].name, 'Bert');
  assert.deepEqual(calls[0], ['select * from public.hub_leaderboard($1)', [10]]);
  await get(h, '/?limit=10');
  assert.equal(calls.length, 2, 'second call within the cache window does not query');
  t += CACHE_MS + 1;
  await get(h, '/?limit=10');
  assert.equal(calls.length, 4, 'after the window it queries again');
});

test('other methods and paths are refused, OPTIONS is a preflight', async () => {
  const h = createHandler(fakeSql());
  assert.equal((await h(new Request('https://b.example/', { method: 'POST', body: '{}' }))).status, 405);
  assert.equal((await get(h, '/users')).status, 404);
  const pre = await h(new Request('https://b.example/', { method: 'OPTIONS' }));
  assert.equal(pre.status, 204);
  assert.equal(pre.headers.get('access-control-allow-methods'), 'GET, OPTIONS');
});

test('a database error serves the last good answer, else 503', async () => {
  let t = 0;
  let fail = false;
  const sql = async (q) => {
    if (fail) throw new Error('down');
    return q.includes('hub_leaderboard') ? ROWS : COUNTS;
  };
  const h = createHandler({ sql, now: () => t });
  assert.equal((await get(h)).status, 200);
  fail = true;
  t += CACHE_MS + 1;
  const stale = await get(h);
  assert.equal(stale.status, 200);
  assert.equal((await stale.json()).board.length, 2);
  const fresh = createHandler({ sql, now: () => t });
  assert.equal((await get(fresh)).status, 503);
});

test('SQL over HTTP posts to the endpoint host with the connection string', async () => {
  let seen;
  const fetchImpl = async (url, init) => {
    seen = { url, init };
    return new Response(JSON.stringify({ rows: [{ a: 1 }] }));
  };
  const sql = sqlOverHttp('postgresql://u:p@ep-x-pooler.c-7.eu-central-1.aws.neon.tech/neondb?sslmode=require', fetchImpl);
  assert.deepEqual(await sql('select 1 as a'), [{ a: 1 }]);
  assert.equal(seen.url, 'https://ep-x-pooler.c-7.eu-central-1.aws.neon.tech/sql');
  assert.equal(seen.init.headers['neon-connection-string'].startsWith('postgresql://'), true);
  assert.deepEqual(JSON.parse(seen.init.body), { query: 'select 1 as a', params: [] });
});

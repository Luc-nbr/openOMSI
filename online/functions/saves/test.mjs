// Unit tests for the saves function: `node --test` in this folder (Node 24, no dependencies).
import test from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign as cryptoSign, createHash } from 'node:crypto';

import fn, {
  parseRoute,
  validateBody,
  validText,
  bearerToken,
  checkClaims,
  verifyJwt,
  keysFromJwks,
  createJwksCache,
  canonicalRequest,
  stringToSign,
  signingKey,
  signature,
  presign,
  signRequest,
  parseListXml,
  keysToPrune,
  pickFreeSlot,
  saveKey,
  userPrefix,
  createHandler,
  createRateLimiter,
  uploadHeaders,
  MAX_SAVE_BYTES,
} from './index.mjs';

// ---------------------------------------------------------------------------------------------
// SigV4: AWS's published examples (S3 "Authenticating Requests", Signature Version 4)

const AWS = {
  accessKeyId: 'AKIAIOSFODNN7EXAMPLE',
  secretAccessKey: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY',
  region: 'us-east-1',
};
const AWS_DATE = '20130524T000000Z';
const AWS_HOST = 'https://examplebucket.s3.amazonaws.com';

test('SigV4 presigned GET matches the AWS test vector', () => {
  const query = {
    'X-Amz-Algorithm': 'AWS4-HMAC-SHA256',
    'X-Amz-Credential': 'AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request',
    'X-Amz-Date': AWS_DATE,
    'X-Amz-Expires': '86400',
    'X-Amz-SignedHeaders': 'host',
  };
  const canonical = canonicalRequest({
    method: 'GET',
    path: '/test.txt',
    query,
    headers: { host: 'examplebucket.s3.amazonaws.com' },
    payloadHash: 'UNSIGNED-PAYLOAD',
  });
  assert.equal(
    canonical,
    [
      'GET',
      '/test.txt',
      'X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20130524T000000Z&X-Amz-Expires=86400&X-Amz-SignedHeaders=host',
      'host:examplebucket.s3.amazonaws.com',
      '',
      'host',
      'UNSIGNED-PAYLOAD',
    ].join('\n'),
  );
  const sts = stringToSign({ date: AWS_DATE, region: 'us-east-1', canonical });
  assert.equal(
    sts,
    'AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n3bfa292879f6447bbcda7001decf97f4a54dc650c8942174ae0a9121cf58ad04',
  );
  const sig = signature(signingKey(AWS.secretAccessKey, AWS_DATE, 'us-east-1'), sts);
  assert.equal(sig, 'aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404');

  const url = presign({ method: 'GET', endpoint: AWS_HOST, path: '/test.txt', expires: 86400, creds: AWS, date: AWS_DATE });
  assert.equal(
    url,
    'https://examplebucket.s3.amazonaws.com/test.txt?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20130524T000000Z&X-Amz-Expires=86400&X-Amz-SignedHeaders=host&X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404',
  );
});

test('SigV4 header-signed GET object matches the AWS test vector', () => {
  const { headers } = signRequest({
    method: 'GET',
    endpoint: AWS_HOST,
    path: '/test.txt',
    headers: { range: 'bytes=0-9' },
    creds: AWS,
    date: AWS_DATE,
  });
  assert.equal(
    headers.authorization,
    'AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41',
  );
});

test('SigV4 header-signed list matches the AWS test vector', () => {
  const { url, headers } = signRequest({
    method: 'GET',
    endpoint: AWS_HOST,
    path: '/',
    query: { 'max-keys': '2', prefix: 'J' },
    creds: AWS,
    date: AWS_DATE,
  });
  assert.equal(url, 'https://examplebucket.s3.amazonaws.com/?max-keys=2&prefix=J');
  assert.match(headers.authorization, /Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7$/);
});

test('a presigned PUT signs the upload headers, so the client cannot change them', () => {
  const v = { size: 1234, name: 'Bus Co Ölsen & Söhne', sha256: 'a'.repeat(64), device: 'Luc PC' };
  const h = uploadHeaders(v);
  assert.equal(h['x-amz-meta-name'], 'Bus%20Co%20%C3%96lsen%20%26%20S%C3%B6hne');
  assert.equal(h['content-length'], '1234');
  const url = presign({
    method: 'PUT',
    endpoint: 'https://br-x.storage.example',
    path: '/saves/users/u1/slot1/v1.omsave',
    headers: h,
    expires: 600,
    creds: AWS,
    date: AWS_DATE,
  });
  const q = new URL(url).searchParams;
  assert.equal(
    q.get('X-Amz-SignedHeaders'),
    'content-length;content-type;host;x-amz-meta-device;x-amz-meta-name;x-amz-meta-sha256;x-amz-meta-size',
  );
  assert.equal(q.get('X-Amz-Expires'), '600');
  assert.ok(url.startsWith('https://br-x.storage.example/saves/users/u1/slot1/v1.omsave?'));
});

test('ListObjectsV2 XML is parsed, with pagination', () => {
  const xml = `<?xml version="1.0"?><ListBucketResult><IsTruncated>true</IsTruncated>
    <Contents><Key>users/a/slot1/v1.omsave</Key><Size>10</Size></Contents>
    <Contents><Key>users/a/slot1/meta.json</Key><Size>200</Size></Contents>
    <NextContinuationToken>abc&amp;def</NextContinuationToken></ListBucketResult>`;
  assert.deepEqual(parseListXml(xml), {
    keys: [
      { key: 'users/a/slot1/v1.omsave', size: 10 },
      { key: 'users/a/slot1/meta.json', size: 200 },
    ],
    next: 'abc&def',
  });
  assert.equal(parseListXml('<IsTruncated>false</IsTruncated>').next, null);
});

// ---------------------------------------------------------------------------------------------
// Routing and validation

test('routes: POST only, known names only', () => {
  assert.equal(parseRoute('POST', '/list'), 'list');
  assert.equal(parseRoute('POST', '/upload_begin/'), 'upload_begin');
  assert.equal(parseRoute('GET', '/list'), null);
  assert.equal(parseRoute('POST', '/'), null);
  assert.equal(parseRoute('POST', '/nope'), null);
  assert.equal(parseRoute('POST', '/list/x'), null);
});

test('validation rejects bad input', () => {
  const ok = { name: 'Firma', size: 100, sha256: 'A'.repeat(64), base_version: 0 };
  assert.deepEqual(validateBody('upload_begin', ok), { slot: null, name: 'Firma', size: 100, sha256: 'a'.repeat(64), base_version: 0, device: '' });
  const bad = (route, body) => assert.throws(() => validateBody(route, body), (e) => e.status >= 400);
  bad('upload_begin', { ...ok, slot: 3 });
  bad('upload_begin', { ...ok, slot: '1' });
  bad('upload_begin', { ...ok, size: 0 });
  bad('upload_begin', { ...ok, size: 1.5 });
  assert.throws(() => validateBody('upload_begin', { ...ok, size: MAX_SAVE_BYTES + 1 }), (e) => e.status === 413);
  bad('upload_begin', { ...ok, sha256: 'xyz' });
  bad('upload_begin', { ...ok, base_version: -1 });
  bad('upload_begin', { ...ok, base_version: '0' });
  bad('upload_begin', { ...ok, name: '' });
  bad('upload_begin', { ...ok, name: '   ' });
  bad('upload_begin', { ...ok, name: 'x'.repeat(65) });
  bad('upload_begin', { ...ok, name: 'a\nb' });
  bad('upload_begin', { ...ok, extra: 1 });
  bad('upload_begin', { ...ok, prefix: '../other' });
  bad('upload_commit', { slot: 1 });
  bad('download', { slot: 0 });
  bad('delete', {});
  bad('list', { slot: 1 });
  bad('list', []);
  assert.equal(validText('  Busbedrijf  '), 'Busbedrijf');
  assert.equal(validText('é'.repeat(64)), 'é'.repeat(64));
});

test('keys are built from the verified sub only, and a bad sub is refused', () => {
  assert.equal(saveKey('abcDEF123', 2, 7), 'users/abcDEF123/slot2/v7.omsave');
  assert.throws(() => userPrefix('../x'));
  assert.throws(() => userPrefix('a/b'));
  assert.throws(() => userPrefix(''));
  assert.throws(() => saveKey('u', 3, 1));
});

test('pruning keeps the last 3 versions and anything newer', () => {
  const keys = ['p/v1.omsave', 'p/v2.omsave', 'p/v3.omsave', 'p/v4.omsave', 'p/v5.omsave', 'p/v6.omsave', 'p/meta.json'];
  assert.deepEqual(keysToPrune(keys, 5), ['p/v1.omsave', 'p/v2.omsave']);
  assert.deepEqual(keysToPrune(keys, 2), []);
  assert.equal(pickFreeSlot({ 1: { version: 1 }, 2: null }), 2);
  assert.equal(pickFreeSlot({ 1: { version: 1 }, 2: { version: 1 } }), null);
});

test('rate limiter: 60 a minute per user', () => {
  const lim = createRateLimiter(60, 60_000);
  for (let i = 0; i < 60; i++) assert.ok(lim('a', 1000));
  assert.equal(lim('a', 1000), false);
  assert.ok(lim('b', 1000));
  assert.ok(lim('a', 61_000));
});

// ---------------------------------------------------------------------------------------------
// JWT

const ISS = 'https://ep-test.neonauth.example';
const { publicKey, privateKey } = generateKeyPairSync('ed25519');
const JWK = { ...publicKey.export({ format: 'jwk' }), kid: 'k1', alg: 'EdDSA', use: 'sig' };
const b64 = (o) => Buffer.from(JSON.stringify(o)).toString('base64url');
function makeJwt(payload, header = { alg: 'EdDSA', kid: 'k1', typ: 'JWT' }, key = privateKey) {
  const input = `${b64(header)}.${b64(payload)}`;
  return `${input}.${cryptoSign(null, Buffer.from(input), key).toString('base64url')}`;
}
const NOW = 1_800_000_000;
const claims = (o = {}) => ({ sub: 'user123', iss: ISS, iat: NOW - 10, nbf: NOW - 10, exp: NOW + 900, ...o });
const keys = keysFromJwks({ keys: [JWK, { kty: 'RSA', kid: 'r1', n: 'x', e: 'AQAB' }] });
const opts = { getKey: async (kid) => keys.get(kid), issuer: ISS, now: NOW };

test('JWKS: only Ed25519 keys are taken', () => {
  assert.deepEqual([...keys.keys()], ['k1']);
});

test('JWT: a good EdDSA token gives its sub', async () => {
  assert.equal(await verifyJwt(makeJwt(claims()), opts), 'user123');
  // within the 60 s skew
  assert.equal(await verifyJwt(makeJwt(claims({ exp: NOW - 30 })), opts), 'user123');
  assert.equal(await verifyJwt(makeJwt(claims({ nbf: NOW + 30, iat: NOW + 30 })), opts), 'user123');
});

test('JWT: bad tokens are refused', async () => {
  const no = (t, o = opts) => assert.rejects(verifyJwt(t, o));
  await no(makeJwt(claims({ exp: NOW - 61 })));
  await no(makeJwt(claims({ exp: undefined })));
  await no(makeJwt(claims({ nbf: NOW + 61 })));
  await no(makeJwt(claims({ iat: NOW + 61 })));
  await no(makeJwt(claims({ iss: 'https://evil.example' })));
  await no(makeJwt(claims({ sub: '../other' })));
  await no(makeJwt(claims({ sub: undefined })));
  await no(makeJwt(claims(), { alg: 'none', kid: 'k1' }));
  await no(makeJwt(claims(), { alg: 'HS256', kid: 'k1' }));
  await no(makeJwt(claims(), { alg: 'EdDSA', kid: 'unknown' }));
  await no(makeJwt(claims(), { alg: 'EdDSA' }));
  await no(makeJwt(claims(), undefined, generateKeyPairSync('ed25519').privateKey));
  const [h, , s] = makeJwt(claims()).split('.');
  await no(`${h}.${b64(claims({ sub: 'someoneelse' }))}.${s}`);
  await no(`${h}.${b64(claims())}.`);
  await no('a.b');
  await no('not a token');
  assert.throws(() => checkClaims(claims(), { now: NOW, issuer: '' }));
});

test('bearer: only "Authorization: Bearer <jwt>"', () => {
  assert.equal(bearerToken('Bearer abc.def.ghi'), 'abc.def.ghi');
  assert.equal(bearerToken('Basic abc.def.ghi'), null);
  assert.equal(bearerToken('Bearer'), null);
  assert.equal(bearerToken('abc.def.ghi'), null);
  assert.equal(bearerToken(null), null);
});

test('JWKS cache: one fetch, refetch on an unknown kid at most every 30 s', async () => {
  let fetches = 0;
  const cache = createJwksCache('https://jwks', async () => {
    fetches++;
    return { ok: true, json: async () => ({ keys: [JWK] }) };
  });
  assert.ok(await cache.getKey('k1'));
  assert.ok(await cache.getKey('k1'));
  assert.equal(fetches, 1);
  assert.equal(await cache.getKey('nope'), undefined);
  assert.equal(await cache.getKey('nope2'), undefined);
  assert.equal(fetches, 1); // within the cooldown
});

// ---------------------------------------------------------------------------------------------
// The handler, against an in-memory bucket

function memoryStorage() {
  const objects = new Map(); // key → { body, size, meta }
  return {
    objects,
    async getJson(k) {
      const o = objects.get(k);
      return o ? JSON.parse(o.body) : null;
    },
    async putJson(k, obj) {
      const body = JSON.stringify(obj);
      objects.set(k, { body, size: body.length, meta: {} });
    },
    async head(k) {
      const o = objects.get(k);
      return o ? { size: o.size, meta: o.meta } : null;
    },
    async list(prefix) {
      return [...objects.keys()].filter((k) => k.startsWith(prefix)).map((key) => ({ key, size: objects.get(key).size }));
    },
    async del(k) {
      objects.delete(k);
    },
    async presign(method, key, headers) {
      return `memory://${method}/${key}#${encodeURIComponent(JSON.stringify(headers))}`;
    },
    // what the client does with a presigned PUT
    clientPut(url, bytes) {
      const [, key, h] = /^memory:\/\/PUT\/([^#]+)#(.*)$/.exec(url);
      const headers = JSON.parse(decodeURIComponent(h));
      objects.set(key, {
        body: bytes,
        size: bytes.length,
        meta: {
          name: headers['x-amz-meta-name'],
          sha256: headers['x-amz-meta-sha256'],
          size: headers['x-amz-meta-size'],
          device: headers['x-amz-meta-device'],
        },
      });
    },
  };
}

const TOKENS = { 'tok-alice-0001': 'alice', 'tok-bob-000001': 'bob' };
function setup(limit = 1000) {
  const storage = memoryStorage();
  const handle = createHandler({
    storage,
    auth: async (t) => {
      if (!TOKENS[t]) throw new Error('bad');
      return TOKENS[t];
    },
    now: () => new Date('2026-10-16T12:00:00Z'),
    limiter: createRateLimiter(limit),
  });
  const call = async (route, body, token = 'tok-alice-0001', method = 'POST') => {
    const headers = { 'content-type': 'application/json' };
    if (token) headers.authorization = `Bearer ${token}`;
    const res = await handle(
      new Request(`https://fn.example/${route}`, { method, headers, body: method === 'POST' && body !== undefined ? JSON.stringify(body) : undefined }),
    );
    assert.equal(res.headers.get('access-control-allow-origin'), null); // no CORS, ever
    return { status: res.status, body: await res.json() };
  };
  return { storage, call };
}
const sha = (s) => createHash('sha256').update(s).digest('hex');

async function upload(t, { slot, name = 'Firma', data = 'save-bytes', base_version = 0, device = 'PC' }, token) {
  const begin = await t.call('upload_begin', { slot, name, size: data.length, sha256: sha(data), base_version, device }, token);
  if (begin.status !== 200) return { begin };
  t.storage.clientPut(begin.body.url, data);
  const commit = await t.call('upload_commit', { slot: begin.body.slot, version: begin.body.version, sha256: sha(data) }, token);
  return { begin, commit };
}

test('auth comes first: no token, a garbage token, or an unknown route without a token are 401', async () => {
  const t = setup();
  assert.equal((await t.call('list', {}, null)).status, 401);
  assert.equal((await t.call('list', {}, 'garbage-token-xyz')).status, 401);
  assert.equal((await t.call('nope', {}, null)).status, 401);
  assert.equal((await t.call('list', undefined, null, 'GET')).status, 401);
  assert.equal((await t.call('nope', {})).status, 404);
  assert.equal((await t.call('list', undefined, 'tok-alice-0001', 'GET')).status, 404);
});

test('upload, list, download: the whole flow', async () => {
  const t = setup();
  let r = await t.call('list', {});
  assert.equal(r.status, 200);
  assert.equal(r.body.max_slots, 2);
  assert.deepEqual(r.body.slots, [{ slot: 1, empty: true }, { slot: 2, empty: true }]);

  const { begin, commit } = await upload(t, { name: 'Groningen Bus', device: 'Luc-PC' });
  assert.equal(begin.status, 200);
  assert.equal(begin.body.slot, 1);
  assert.equal(begin.body.version, 1);
  assert.equal(begin.body.method, 'PUT');
  assert.equal(begin.body.expires_in, 600);
  assert.ok(begin.body.url.includes('users/alice/slot1/v1.omsave'));
  assert.equal(commit.status, 200);
  assert.equal(commit.body.version, 1);

  r = await t.call('list', {});
  assert.deepEqual(r.body.slots[0], {
    slot: 1,
    empty: false,
    name: 'Groningen Bus',
    version: 1,
    size: 10,
    sha256: sha('save-bytes'),
    updated_at: '2026-10-16T12:00:00.000Z',
    device: 'Luc-PC',
    versions: [1],
  });

  r = await t.call('download', { slot: 1 });
  assert.equal(r.status, 200);
  assert.equal(r.body.method, 'GET');
  assert.ok(r.body.url.includes('users/alice/slot1/v1.omsave'));
  assert.equal(r.body.sha256, sha('save-bytes'));
  assert.equal((await t.call('download', { slot: 2 })).status, 404);
});

test('at most 2 companies: a third gets {error:"limit"}', async () => {
  const t = setup();
  await upload(t, { name: 'A' });
  await upload(t, { name: 'B' });
  const { begin } = await upload(t, { name: 'C' });
  assert.equal(begin.status, 409);
  assert.deepEqual(begin.body, { ok: false, error: 'limit', max_slots: 2 });
});

test('optimistic concurrency: a stale base_version gets {error:"conflict", current}', async () => {
  const t = setup();
  await upload(t, { slot: 1, data: 'one' });
  await upload(t, { slot: 1, data: 'two', base_version: 1 });
  const { begin } = await upload(t, { slot: 1, data: 'old-device', base_version: 1 });
  assert.equal(begin.status, 409);
  assert.equal(begin.body.error, 'conflict');
  assert.equal(begin.body.current.version, 2);
  assert.equal(begin.body.current.sha256, sha('two'));
  // a new company in an empty slot needs base_version 0
  const fresh = await t.call('upload_begin', { slot: 2, name: 'X', size: 1, sha256: sha('x'), base_version: 3 });
  assert.equal(fresh.status, 409);
});

test('commit checks size and sha256 against the promise, and removes a bad upload', async () => {
  const t = setup();
  const begin = await t.call('upload_begin', { name: 'A', size: 5, sha256: sha('12345'), base_version: 0 });
  t.storage.clientPut(begin.body.url, '123456'); // one byte more than promised
  let r = await t.call('upload_commit', { slot: 1, version: 1 });
  assert.equal(r.status, 422);
  assert.equal(t.storage.objects.has('users/alice/slot1/v1.omsave'), false);
  assert.equal((await t.call('list', {})).body.slots[0].empty, true);
  // not uploaded at all
  r = await t.call('upload_commit', { slot: 1, version: 1 });
  assert.equal(r.status, 404);
  // the wrong sha256 at commit (another device overwrote the pending key)
  const b2 = await t.call('upload_begin', { name: 'A', size: 5, sha256: sha('12345'), base_version: 0 });
  t.storage.clientPut(b2.body.url, '12345');
  r = await t.call('upload_commit', { slot: 1, version: 1, sha256: sha('other') });
  assert.equal(r.status, 422);
  // skipping a version is a conflict
  const b3 = await t.call('upload_begin', { name: 'A', size: 5, sha256: sha('12345'), base_version: 0 });
  t.storage.clientPut(b3.body.url, '12345');
  assert.equal((await t.call('upload_commit', { slot: 1, version: 1 })).status, 200);
  assert.equal((await t.call('upload_commit', { slot: 1, version: 1 })).status, 200); // a retry is fine
  assert.equal((await t.call('upload_commit', { slot: 1, version: 3 })).status, 404);
});

test('only the last 3 versions are kept; older ones can no longer be downloaded', async () => {
  const t = setup();
  for (let v = 0; v < 5; v++) await upload(t, { slot: 1, data: `data-${v}`, base_version: v });
  const keys = [...t.storage.objects.keys()].sort();
  assert.deepEqual(keys, [
    'users/alice/slot1/meta.json',
    'users/alice/slot1/v3.omsave',
    'users/alice/slot1/v4.omsave',
    'users/alice/slot1/v5.omsave',
  ]);
  const list = await t.call('list', {});
  assert.deepEqual(list.body.slots[0].versions, [5, 4, 3]);
  assert.equal((await t.call('download', { slot: 1, version: 3 })).status, 200);
  assert.equal((await t.call('download', { slot: 1, version: 2 })).status, 404);
});

test('delete and delete_all touch only the caller', async () => {
  const t = setup();
  await upload(t, { name: 'A' });
  await upload(t, { name: 'B' });
  await upload(t, { name: 'Bob Co' }, 'tok-bob-000001');
  let r = await t.call('delete', { slot: 1 });
  assert.equal(r.status, 200);
  assert.equal(r.body.deleted, 2);
  assert.deepEqual(
    (await t.call('list', {})).body.slots.map((s) => s.empty),
    [true, false],
  );
  r = await t.call('delete_all', {});
  assert.equal(r.body.deleted, 2);
  assert.deepEqual([...t.storage.objects.keys()].sort(), ['users/bob/slot1/meta.json', 'users/bob/slot1/v1.omsave']);
  // Bob still sees his company, Alice never sees Bob's
  assert.equal((await t.call('list', {}, 'tok-bob-000001')).body.slots[0].name, 'Bob Co');
  assert.equal((await t.call('download', { slot: 1 })).status, 404);
});

test('limits: body size, extra fields, rate limit', async () => {
  const t = setup(3);
  assert.equal((await t.call('list', { pad: 'x'.repeat(5000) })).status, 413);
  assert.equal((await t.call('list', { slot: 1 })).status, 400);
  assert.equal((await t.call('list', {})).status, 200);
  assert.equal((await t.call('list', {})).status, 429);
  assert.equal((await t.call('list', {}, 'tok-bob-000001')).status, 200);
});

test('errors are generic', async () => {
  const storage = memoryStorage();
  storage.getJson = async () => {
    throw new Error('storage GET 503 https://secret.example/users/alice/x?X-Amz-Signature=abc');
  };
  const logs = [];
  const handle = createHandler({ storage, auth: async () => 'alice', log: (l) => logs.push(l) });
  const res = await handle(new Request('https://fn.example/list', { method: 'POST', headers: { authorization: 'Bearer tok-alice-0001' }, body: '{}' }));
  assert.equal(res.status, 500);
  assert.deepEqual(await res.json(), { ok: false, error: 'internal' });
  assert.ok(!JSON.stringify(logs).includes('tok-alice'));
  assert.ok(!JSON.stringify(logs).includes('alice'));
  assert.ok(!JSON.stringify(logs).includes('X-Amz-Signature'));
  assert.equal(logs[0].error, 'storage GET 503 <url>');
});

test('the default export answers 401 without a token even when unconfigured', async () => {
  if (process.env.AWS_ENDPOINT_URL_S3 && process.env.NEON_AUTH_JWKS_URL) return;
  const res = await fn.fetch(new Request('https://fn.example/whatever', { method: 'POST' }));
  assert.equal(res.status, 401);
});

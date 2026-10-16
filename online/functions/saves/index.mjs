// Omsi-Hub cloud saves: a Neon Function (runtime nodejs24) with no npm dependencies.
//
// The app (Rust, never a browser) calls POST /<route> with a JSON body and
// `Authorization: Bearer <Neon Auth JWT>`. The function verifies the token, then hands out
// short-lived presigned S3 URLs for the user's own prefix in the private bucket `saves`:
//
//   users/<sub>/slot<1|2>/v<version>.omsave   the compressed save file (x-amz-meta-* carries
//                                             name, sha256, size and device)
//   users/<sub>/slot<1|2>/meta.json           the committed state of the slot, written only here
//
// The prefix is always built from the verified `sub`, never from the request.
// Everything above the "I/O" line is pure and unit-tested in test.mjs.

import { createHash, createHmac, createPublicKey, verify as cryptoVerify, randomUUID } from 'node:crypto';

// ---------------------------------------------------------------------------------------------
// Limits

export const MAX_SLOTS = 2;
export const MAX_SAVE_BYTES = 25 * 1024 * 1024; // the client compresses with zstd first
export const KEEP_VERSIONS = 3;
export const URL_TTL_SECONDS = 600;
export const MAX_TEXT_CHARS = 64; // company name, device name
export const MAX_BODY_BYTES = 4096;
export const MAX_VERSION = 1_000_000_000;
export const RATE_LIMIT_PER_MINUTE = 60;
export const CLOCK_SKEW_SECONDS = 60;
export const JWKS_TTL_MS = 60 * 60 * 1000;
export const JWKS_REFETCH_COOLDOWN_MS = 30 * 1000;
export const BUCKET = 'saves';
export const SAVE_CONTENT_TYPE = 'application/octet-stream';

export const ROUTES = ['list', 'upload_begin', 'upload_commit', 'download', 'delete', 'delete_all'];

// ---------------------------------------------------------------------------------------------
// Errors and routing

export class ApiError extends Error {
  constructor(status, code, extra) {
    super(code);
    this.status = status;
    this.code = code;
    this.extra = extra;
  }
}

const bad = () => new ApiError(400, 'bad_request');

/** `POST /list` → 'list'; anything else → null. */
export function parseRoute(method, pathname) {
  if (method !== 'POST') return null;
  const m = /^\/([a-z_]{1,32})\/?$/.exec(pathname);
  return m && ROUTES.includes(m[1]) ? m[1] : null;
}

// ---------------------------------------------------------------------------------------------
// Validation

export function validSlot(v) {
  if (v === 1 || v === 2) return v;
  throw bad();
}

export function validVersion(v) {
  if (Number.isSafeInteger(v) && v >= 0 && v <= MAX_VERSION) return v;
  throw bad();
}

export function validSha256(v) {
  if (typeof v === 'string' && /^[0-9a-fA-F]{64}$/.test(v)) return v.toLowerCase();
  throw bad();
}

export function validSize(v) {
  if (!Number.isSafeInteger(v) || v < 1) throw bad();
  if (v > MAX_SAVE_BYTES) throw new ApiError(413, 'too_large', { max_bytes: MAX_SAVE_BYTES });
  return v;
}

/** A short display text: trimmed, no control characters, 1..64 (or 0..64) code points. */
export function validText(v, { optional = false } = {}) {
  if (v === undefined || v === null) {
    if (optional) return '';
    throw bad();
  }
  if (typeof v !== 'string' || v.length > 4 * MAX_TEXT_CHARS) throw bad();
  const t = v.trim().normalize('NFC');
  const len = [...t].length;
  if (len > MAX_TEXT_CHARS || (!optional && len === 0)) throw bad();
  if (/[\p{Cc}\p{Zl}\p{Zp}]/u.test(t)) throw bad(); // control characters, line/paragraph separators
  return t;
}

function onlyKeys(body, allowed) {
  for (const k of Object.keys(body)) if (!allowed.includes(k)) throw bad();
}

/** Validates the JSON body of a route; returns a clean object with only known fields. */
export function validateBody(route, body) {
  if (body === null || typeof body !== 'object' || Array.isArray(body)) throw bad();
  switch (route) {
    case 'list':
    case 'delete_all':
      onlyKeys(body, []);
      return {};
    case 'upload_begin': {
      onlyKeys(body, ['slot', 'name', 'size', 'sha256', 'base_version', 'device']);
      return {
        slot: body.slot === undefined || body.slot === null ? null : validSlot(body.slot),
        name: validText(body.name),
        size: validSize(body.size),
        sha256: validSha256(body.sha256),
        base_version: validVersion(body.base_version),
        device: validText(body.device, { optional: true }),
      };
    }
    case 'upload_commit':
      onlyKeys(body, ['slot', 'version', 'sha256']);
      return {
        slot: validSlot(body.slot),
        version: validVersion(body.version),
        sha256: body.sha256 === undefined ? null : validSha256(body.sha256),
      };
    case 'download':
      onlyKeys(body, ['slot', 'version']);
      return {
        slot: validSlot(body.slot),
        version: body.version === undefined || body.version === null ? null : validVersion(body.version),
      };
    case 'delete':
      onlyKeys(body, ['slot']);
      return { slot: validSlot(body.slot) };
    default:
      throw new ApiError(404, 'not_found');
  }
}

// ---------------------------------------------------------------------------------------------
// Keys: built only from the verified sub

export function validSub(sub) {
  return typeof sub === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(sub);
}

export function userPrefix(sub) {
  if (!validSub(sub)) throw new ApiError(401, 'unauthorized');
  return `users/${sub}/`;
}

export const slotPrefix = (sub, slot) => `${userPrefix(sub)}slot${validSlot(slot)}/`;
export const metaKey = (sub, slot) => `${slotPrefix(sub, slot)}meta.json`;
export const saveKey = (sub, slot, version) => `${slotPrefix(sub, slot)}v${validVersion(version)}.omsave`;

/** The version number of a save key, or null for anything else (meta.json). */
export function versionOfKey(key) {
  const m = /\/v(\d{1,10})\.omsave$/.exec(key);
  return m ? Number(m[1]) : null;
}

/**
 * Keys to delete after `version` was committed: saves older than the last KEEP_VERSIONS.
 * Newer keys (an upload in flight from another device) are left alone.
 */
export function keysToPrune(keys, version) {
  return keys.filter((k) => {
    const v = versionOfKey(k);
    return v !== null && v <= version - KEEP_VERSIONS;
  });
}

/** The first free slot, given the slot metas (null = free); null when all are taken. */
export function pickFreeSlot(metas) {
  for (let s = 1; s <= MAX_SLOTS; s++) if (!metas[s]) return s;
  return null;
}

/** The public view of a slot's meta.json. */
export function publicSlot(slot, meta) {
  if (!meta) return { slot, empty: true };
  return {
    slot,
    empty: false,
    name: meta.name,
    version: meta.version,
    size: meta.size,
    sha256: meta.sha256,
    updated_at: meta.updated_at,
    device: meta.device,
    versions: (meta.history || []).map((h) => h.version),
  };
}

/** The new meta.json after committing `entry` ({version,size,sha256,name,device,updated_at}). */
export function nextMeta(prev, slot, entry) {
  const history = [entry, ...((prev && prev.history) || []).filter((h) => h.version < entry.version)]
    .slice(0, KEEP_VERSIONS)
    .map(({ version, size, sha256, updated_at, device }) => ({ version, size, sha256, updated_at, device }));
  return { format: 1, slot, ...entry, history };
}

/** URL-encodes a text for an x-amz-meta-* header (ASCII only) and back. */
export const encodeMeta = (s) => encodeURIComponent(s);
export function decodeMeta(s) {
  try {
    return decodeURIComponent(s || '');
  } catch {
    return '';
  }
}

// ---------------------------------------------------------------------------------------------
// Rate limit (in memory, so per isolate)

export function createRateLimiter(limit = RATE_LIMIT_PER_MINUTE, windowMs = 60_000) {
  const hits = new Map();
  return (id, now = Date.now()) => {
    let e = hits.get(id);
    if (!e || now - e.start >= windowMs) {
      if (hits.size > 10_000) {
        for (const [k, v] of hits) if (now - v.start >= windowMs) hits.delete(k);
      }
      e = { start: now, count: 0 };
      hits.set(id, e);
    }
    e.count++;
    return e.count <= limit;
  };
}

// ---------------------------------------------------------------------------------------------
// JWT (EdDSA / Ed25519, Neon Auth)

const B64URL = /^[A-Za-z0-9_-]+$/;

/** The token from an `Authorization: Bearer <jwt>` header, or null. */
export function bearerToken(header) {
  if (typeof header !== 'string') return null;
  const m = /^Bearer ([A-Za-z0-9_.-]{10,8192})$/.exec(header.trim());
  return m ? m[1] : null;
}

/** Splits and decodes a compact JWS without verifying it. Throws on any malformed part. */
export function decodeJwt(token) {
  const parts = token.split('.');
  if (parts.length !== 3 || !parts.every((p) => B64URL.test(p))) throw new Error('malformed');
  const header = JSON.parse(Buffer.from(parts[0], 'base64url').toString('utf8'));
  const payload = JSON.parse(Buffer.from(parts[1], 'base64url').toString('utf8'));
  if (!header || typeof header !== 'object' || !payload || typeof payload !== 'object') throw new Error('malformed');
  return {
    header,
    payload,
    signingInput: Buffer.from(`${parts[0]}.${parts[1]}`, 'ascii'),
    signature: Buffer.from(parts[2], 'base64url'),
  };
}

/** Checks exp / nbf / iat (with skew), iss and sub. Returns the sub; throws on failure. */
export function checkClaims(payload, { now, issuer, skew = CLOCK_SKEW_SECONDS }) {
  const num = (v) => typeof v === 'number' && Number.isFinite(v);
  if (!num(payload.exp) || now > payload.exp + skew) throw new Error('expired');
  if (payload.nbf !== undefined && (!num(payload.nbf) || now < payload.nbf - skew)) throw new Error('nbf');
  if (payload.iat !== undefined && (!num(payload.iat) || payload.iat > now + skew)) throw new Error('iat');
  if (!issuer || payload.iss !== issuer) throw new Error('iss');
  if (!validSub(payload.sub)) throw new Error('sub');
  return payload.sub;
}

/** Verifies a token against a key lookup `(kid) => KeyObject | undefined` (async). */
export async function verifyJwt(token, { getKey, issuer, now = Math.floor(Date.now() / 1000) }) {
  const { header, payload, signingInput, signature } = decodeJwt(token);
  if (header.alg !== 'EdDSA' || typeof header.kid !== 'string' || header.kid.length > 256) throw new Error('header');
  if (header.crit !== undefined) throw new Error('crit');
  if (signature.length !== 64) throw new Error('sig');
  const key = await getKey(header.kid);
  if (!key) throw new Error('kid');
  if (!cryptoVerify(null, signingInput, key, signature)) throw new Error('sig');
  return checkClaims(payload, { now, issuer });
}

/** Turns a JWKS document into a Map kid → Ed25519 KeyObject (other keys are skipped). */
export function keysFromJwks(jwks) {
  const keys = new Map();
  for (const jwk of (jwks && Array.isArray(jwks.keys) && jwks.keys) || []) {
    if (!jwk || jwk.kty !== 'OKP' || jwk.crv !== 'Ed25519' || typeof jwk.kid !== 'string') continue;
    if (jwk.alg !== undefined && jwk.alg !== 'EdDSA') continue;
    if (jwk.use !== undefined && jwk.use !== 'sig') continue;
    try {
      keys.set(jwk.kid, createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: jwk.x }, format: 'jwk' }));
    } catch {
      // skip a key that does not parse
    }
  }
  return keys;
}

/** A cached JWKS: refreshed after an hour, refetched on an unknown kid at most every 30 s. */
export function createJwksCache(url, fetchImpl = fetch) {
  let keys = new Map();
  let fetchedAt = 0;
  let lastAttempt = 0;
  let inflight = null;
  async function refresh() {
    if (!inflight) {
      lastAttempt = Date.now();
      inflight = (async () => {
        const res = await fetchImpl(url, { signal: AbortSignal.timeout(5000), headers: { accept: 'application/json' } });
        if (!res.ok) throw new Error(`jwks ${res.status}`);
        keys = keysFromJwks(await res.json());
        fetchedAt = Date.now();
      })().finally(() => {
        inflight = null;
      });
    }
    return inflight;
  }
  return {
    async getKey(kid) {
      const stale = Date.now() - fetchedAt > JWKS_TTL_MS;
      if (stale || (!keys.has(kid) && Date.now() - lastAttempt > JWKS_REFETCH_COOLDOWN_MS)) {
        try {
          await refresh();
        } catch (e) {
          if (!keys.size) throw e;
        }
      }
      return keys.get(kid);
    },
    get size() {
      return keys.size;
    },
    refresh,
  };
}

// ---------------------------------------------------------------------------------------------
// SigV4 (S3, path-style)

export const sha256Hex = (data) => createHash('sha256').update(data).digest('hex');
const hmac = (key, data) => createHmac('sha256', key).update(data, 'utf8').digest();
export const EMPTY_SHA256 = sha256Hex('');

/** RFC 3986 encoding as SigV4 wants it: everything but A-Z a-z 0-9 - _ . ~ */
export function uriEncode(s, keepSlash = false) {
  let out = '';
  for (const b of Buffer.from(String(s), 'utf8')) {
    const c = String.fromCharCode(b);
    if (/[A-Za-z0-9\-_.~]/.test(c) || (keepSlash && c === '/')) out += c;
    else out += '%' + b.toString(16).toUpperCase().padStart(2, '0');
  }
  return out;
}

/** `20130524T000000Z` */
export const amzDate = (d) => d.toISOString().replace(/[-:]/g, '').replace(/\.\d{3}/, '');

export function canonicalQuery(params) {
  return Object.entries(params)
    .map(([k, v]) => [uriEncode(k), uriEncode(v)])
    .sort(([a, av], [b, bv]) => (a < b ? -1 : a > b ? 1 : av < bv ? -1 : av > bv ? 1 : 0))
    .map(([k, v]) => `${k}=${v}`)
    .join('&');
}

/** headers: lower-case name → value. Returns { canonical, signed }. */
export function canonicalHeaders(headers) {
  const names = Object.keys(headers).map((h) => h.toLowerCase()).sort();
  const lower = Object.fromEntries(Object.entries(headers).map(([k, v]) => [k.toLowerCase(), v]));
  const canonical = names.map((n) => `${n}:${String(lower[n]).trim().replace(/\s+/g, ' ')}\n`).join('');
  return { canonical, signed: names.join(';') };
}

export function canonicalRequest({ method, path, query, headers, payloadHash }) {
  const { canonical, signed } = canonicalHeaders(headers);
  return [method, uriEncode(path, true), canonicalQuery(query), canonical, signed, payloadHash].join('\n');
}

export const credentialScope = (date, region, service = 's3') => `${date.slice(0, 8)}/${region}/${service}/aws4_request`;

export function stringToSign({ date, region, canonical }) {
  return ['AWS4-HMAC-SHA256', date, credentialScope(date, region), sha256Hex(canonical)].join('\n');
}

export function signingKey(secret, date, region, service = 's3') {
  const kDate = hmac('AWS4' + secret, date.slice(0, 8));
  const kRegion = hmac(kDate, region);
  const kService = hmac(kRegion, service);
  return hmac(kService, 'aws4_request');
}

export const signature = (key, sts) => createHmac('sha256', key).update(sts, 'utf8').digest('hex');

/**
 * A presigned URL (query-string SigV4). `headers` are extra headers the client must send
 * exactly (they are signed); host is always signed.
 */
export function presign({ method, endpoint, path, headers = {}, expires, creds, date }) {
  const url = new URL(endpoint);
  const allHeaders = { host: url.host, ...headers };
  const { signed } = canonicalHeaders(allHeaders);
  const query = {
    'X-Amz-Algorithm': 'AWS4-HMAC-SHA256',
    'X-Amz-Credential': `${creds.accessKeyId}/${credentialScope(date, creds.region)}`,
    'X-Amz-Date': date,
    'X-Amz-Expires': String(expires),
    'X-Amz-SignedHeaders': signed,
  };
  const canonical = canonicalRequest({ method, path, query, headers: allHeaders, payloadHash: 'UNSIGNED-PAYLOAD' });
  const sig = signature(signingKey(creds.secretAccessKey, date, creds.region), stringToSign({ date, region: creds.region, canonical }));
  return `${url.origin}${uriEncode(path, true)}?${canonicalQuery(query)}&X-Amz-Signature=${sig}`;
}

/** Headers (incl. Authorization) for a header-signed SigV4 request. */
export function signRequest({ method, endpoint, path, query = {}, headers = {}, body = '', creds, date }) {
  const url = new URL(endpoint);
  const payloadHash = sha256Hex(body);
  const allHeaders = { ...headers, host: url.host, 'x-amz-date': date, 'x-amz-content-sha256': payloadHash };
  const canonical = canonicalRequest({ method, path, query, headers: allHeaders, payloadHash });
  const { signed } = canonicalHeaders(allHeaders);
  const sig = signature(signingKey(creds.secretAccessKey, date, creds.region), stringToSign({ date, region: creds.region, canonical }));
  const { host, ...send } = allHeaders;
  send.authorization = `AWS4-HMAC-SHA256 Credential=${creds.accessKeyId}/${credentialScope(date, creds.region)}, SignedHeaders=${signed}, Signature=${sig}`;
  const qs = canonicalQuery(query);
  return { url: `${url.origin}${uriEncode(path, true)}${qs ? '?' + qs : ''}`, headers: send };
}

const xmlUnescape = (s) =>
  s.replace(/&(lt|gt|quot|apos|amp);/g, (_, e) => ({ lt: '<', gt: '>', quot: '"', apos: "'", amp: '&' })[e]);

/** Parses a ListObjectsV2 response: { keys: [{key,size}], next }. */
export function parseListXml(xml) {
  const keys = [];
  for (const m of xml.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)) {
    const key = /<Key>([\s\S]*?)<\/Key>/.exec(m[1]);
    const size = /<Size>(\d+)<\/Size>/.exec(m[1]);
    if (key) keys.push({ key: xmlUnescape(key[1]), size: size ? Number(size[1]) : 0 });
  }
  const truncated = /<IsTruncated>true<\/IsTruncated>/.test(xml);
  const next = /<NextContinuationToken>([\s\S]*?)<\/NextContinuationToken>/.exec(xml);
  return { keys, next: truncated && next ? xmlUnescape(next[1]) : null };
}

// ---------------------------------------------------------------------------------------------
// The handler, with its I/O injected (storage, token verification, clock)

const json = (status, obj) =>
  new Response(JSON.stringify(obj), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store', 'x-content-type-options': 'nosniff' },
  });

export function uploadHeaders(v) {
  return {
    'content-type': SAVE_CONTENT_TYPE,
    'content-length': String(v.size),
    'x-amz-meta-name': encodeMeta(v.name),
    'x-amz-meta-sha256': v.sha256,
    'x-amz-meta-size': String(v.size),
    'x-amz-meta-device': encodeMeta(v.device),
  };
}

async function readBody(request) {
  const declared = Number(request.headers.get('content-length') || '0');
  if (declared > MAX_BODY_BYTES) throw new ApiError(413, 'too_large');
  // read at most MAX_BODY_BYTES, even when no length was declared
  const chunks = [];
  let total = 0;
  if (request.body) {
    const reader = request.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > MAX_BODY_BYTES) {
        await reader.cancel().catch(() => {});
        throw new ApiError(413, 'too_large');
      }
      chunks.push(Buffer.from(value));
    }
  }
  const text = Buffer.concat(chunks).toString('utf8');
  if (text.trim() === '') return {};
  try {
    return JSON.parse(text);
  } catch {
    throw bad();
  }
}

/**
 * storage: { getJson(key), putJson(key,obj), head(key), list(prefix), del(key), presign(method,key,headers,ttl) }
 * auth: async (token) => sub (throws when invalid)
 */
export function createHandler({ storage, auth, now = () => new Date(), limiter = createRateLimiter(), log = () => {} }) {
  async function readMetas(sub) {
    const metas = {};
    await Promise.all(
      [1, 2].map(async (s) => {
        metas[s] = await storage.getJson(metaKey(sub, s));
      }),
    );
    return metas;
  }

  async function deletePrefix(prefix, keep = null) {
    const items = await storage.list(prefix);
    const keys = items.map((i) => i.key).filter((k) => k !== keep);
    // meta.json first, so a half-finished delete leaves an empty-looking slot, not a broken one
    keys.sort((a, b) => (a.endsWith('/meta.json') ? -1 : b.endsWith('/meta.json') ? 1 : 0));
    for (const k of keys) await storage.del(k);
    return keys.length;
  }

  const routes = {
    async list(sub) {
      const metas = await readMetas(sub);
      return { max_slots: MAX_SLOTS, max_bytes: MAX_SAVE_BYTES, slots: [1, 2].map((s) => publicSlot(s, metas[s])) };
    },

    async upload_begin(sub, v) {
      let slot = v.slot;
      let meta;
      if (slot === null) {
        const metas = await readMetas(sub);
        slot = pickFreeSlot(metas);
        if (slot === null) throw new ApiError(409, 'limit', { max_slots: MAX_SLOTS });
        meta = null;
      } else {
        meta = await storage.getJson(metaKey(sub, slot));
      }
      const current = meta ? meta.version : 0;
      if (v.base_version !== current) {
        throw new ApiError(409, 'conflict', { current: publicSlot(slot, meta) });
      }
      const version = current + 1;
      if (!meta) {
        // a fresh slot: sweep leftovers of an earlier, interrupted delete (the key about to be
        // written is overwritten anyway, and may be another device's upload in flight)
        await deletePrefix(slotPrefix(sub, slot), saveKey(sub, slot, version));
      }
      const headers = uploadHeaders(v);
      const url = await storage.presign('PUT', saveKey(sub, slot, version), headers, URL_TTL_SECONDS);
      return { slot, version, method: 'PUT', url, headers, expires_in: URL_TTL_SECONDS };
    },

    async upload_commit(sub, v) {
      const key = saveKey(sub, v.slot, v.version);
      const meta = await storage.getJson(metaKey(sub, v.slot));
      const current = meta ? meta.version : 0;
      const head = await storage.head(key);
      if (!head) throw new ApiError(404, 'not_uploaded');
      const sha = head.meta.sha256;
      if (meta && meta.version === v.version) {
        // committed already (a retry): fine when it is the same file
        if (meta.sha256 === sha && (v.sha256 === null || v.sha256 === sha)) return { ...publicSlot(v.slot, meta), committed: true };
        throw new ApiError(409, 'conflict', { current: publicSlot(v.slot, meta) });
      }
      if (v.version !== current + 1) throw new ApiError(409, 'conflict', { current: publicSlot(v.slot, meta) });
      const promised = Number(head.meta.size);
      const shaOk = /^[0-9a-f]{64}$/.test(sha || '') && (v.sha256 === null || v.sha256 === sha);
      if (!Number.isSafeInteger(promised) || head.size !== promised || head.size > MAX_SAVE_BYTES || !shaOk) {
        await storage.del(key);
        throw new ApiError(422, 'mismatch');
      }
      const entry = {
        version: v.version,
        size: head.size,
        sha256: sha,
        name: validTextOr(decodeMeta(head.meta.name), meta ? meta.name : 'Company'),
        device: validTextOr(decodeMeta(head.meta.device), ''),
        updated_at: now().toISOString(),
      };
      const next = nextMeta(meta, v.slot, entry);
      await storage.putJson(metaKey(sub, v.slot), next);
      const all = await storage.list(slotPrefix(sub, v.slot));
      for (const k of keysToPrune(all.map((i) => i.key), v.version)) await storage.del(k);
      return { ...publicSlot(v.slot, next), committed: true };
    },

    async download(sub, v) {
      const meta = await storage.getJson(metaKey(sub, v.slot));
      if (!meta) throw new ApiError(404, 'not_found');
      const version = v.version === null ? meta.version : v.version;
      const entry = (meta.history || []).find((h) => h.version === version);
      if (!entry) throw new ApiError(404, 'not_found');
      const url = await storage.presign('GET', saveKey(sub, v.slot, version), {}, URL_TTL_SECONDS);
      return {
        slot: v.slot,
        version,
        name: meta.name,
        size: entry.size,
        sha256: entry.sha256,
        method: 'GET',
        url,
        expires_in: URL_TTL_SECONDS,
      };
    },

    async delete(sub, v) {
      const deleted = await deletePrefix(slotPrefix(sub, v.slot));
      return { slot: v.slot, deleted };
    },

    async delete_all(sub) {
      const deleted = await deletePrefix(userPrefix(sub));
      return { deleted };
    },
  };

  return async function handle(request) {
    const started = Date.now();
    const rid = randomUUID().slice(0, 8);
    let route = null;
    let who = '-';
    try {
      // Authentication first, for every path and method: an unknown route without a token is a 401.
      const token = bearerToken(request.headers.get('authorization'));
      if (!token) throw new ApiError(401, 'unauthorized');
      let sub;
      try {
        sub = await auth(token);
      } catch {
        throw new ApiError(401, 'unauthorized');
      }
      who = sha256Hex(sub).slice(0, 12);
      if (!limiter(sub)) throw new ApiError(429, 'rate_limited');
      const url = new URL(request.url);
      route = parseRoute(request.method, url.pathname);
      if (!route) throw new ApiError(404, 'not_found');
      const body = validateBody(route, await readBody(request));
      const result = await routes[route](sub, body);
      log({ rid, route, user: who, status: 200, ms: Date.now() - started });
      return json(200, { ok: true, ...result });
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.status >= 500 || e.status === 429) log({ rid, route, user: who, status: e.status, error: e.code, ms: Date.now() - started });
        return json(e.status, { ok: false, error: e.code, ...(e.extra || {}) });
      }
      log({ rid, route, user: who, status: 500, error: safeMessage(e), ms: Date.now() - started });
      return json(500, { ok: false, error: 'internal' });
    }
  };
}

/** An error for the log: no URLs (they could carry a signature) and no user prefix. */
export function safeMessage(e) {
  return String((e && e.message) || e)
    .replace(/https?:\/\/\S+/g, '<url>')
    .replace(/users\/[^\s/]+/g, 'users/<sub>')
    .slice(0, 200);
}

function validTextOr(s, fallback) {
  try {
    return validText(s) || fallback;
  } catch {
    return fallback;
  }
}

// ---------------------------------------------------------------------------------------------
// I/O: the real S3 storage and the real token check

export class StorageError extends Error {}

export function createS3Storage({ endpoint, region, accessKeyId, secretAccessKey, bucket = BUCKET, fetchImpl = fetch, clock = () => new Date() }) {
  const creds = { accessKeyId, secretAccessKey, region };
  const base = new URL(endpoint);
  const basePath = base.pathname.replace(/\/+$/, '');
  const objPath = (key) => `${basePath}/${bucket}/${key}`;

  async function send(method, path, { query, headers, body, ok = [200] } = {}) {
    const signed = signRequest({ method, endpoint: base.origin, path, query, headers, body: body || '', creds, date: amzDate(clock()) });
    const res = await fetchImpl(signed.url, { method, headers: signed.headers, body: body || undefined, signal: AbortSignal.timeout(15000) });
    if (!ok.includes(res.status)) {
      await res.arrayBuffer().catch(() => {});
      // the status only: never the URL, which names the user's prefix
      throw new StorageError(`storage ${method} ${res.status}`);
    }
    return res;
  }

  return {
    async getJson(key) {
      const res = await send('GET', objPath(key), { ok: [200, 404] });
      if (res.status === 404) {
        await res.arrayBuffer().catch(() => {});
        return null;
      }
      const text = await res.text();
      try {
        const obj = JSON.parse(text);
        return obj && typeof obj === 'object' && Number.isSafeInteger(obj.version) ? obj : null;
      } catch {
        return null;
      }
    },
    async putJson(key, obj) {
      await send('PUT', objPath(key), { body: JSON.stringify(obj), headers: { 'content-type': 'application/json' } });
    },
    async head(key) {
      const res = await send('HEAD', objPath(key), { ok: [200, 404] });
      if (res.status === 404) return null;
      const h = res.headers;
      return {
        size: Number(h.get('content-length')),
        meta: {
          name: h.get('x-amz-meta-name') || '',
          sha256: (h.get('x-amz-meta-sha256') || '').toLowerCase(),
          size: h.get('x-amz-meta-size') || '',
          device: h.get('x-amz-meta-device') || '',
        },
      };
    },
    async list(prefix) {
      const out = [];
      let token = null;
      for (let page = 0; page < 50; page++) {
        const query = { 'list-type': '2', prefix, 'max-keys': '1000' };
        if (token) query['continuation-token'] = token;
        const res = await send('GET', `${basePath}/${bucket}`, { query });
        const { keys, next } = parseListXml(await res.text());
        out.push(...keys.filter((k) => k.key.startsWith(prefix)));
        if (!next) break;
        token = next;
      }
      return out;
    },
    async del(key) {
      const res = await send('DELETE', objPath(key), { ok: [200, 204, 404] });
      await res.arrayBuffer().catch(() => {});
    },
    async presign(method, key, headers, ttl) {
      return presign({ method, endpoint: base.origin, path: objPath(key), headers, expires: ttl, creds, date: amzDate(clock()) });
    },
  };
}

// ---------------------------------------------------------------------------------------------
// The deployed entry point

const env = process.env;
const configured = Boolean(env.NEON_AUTH_JWKS_URL && env.NEON_AUTH_BASE_URL && env.AWS_ENDPOINT_URL_S3 && env.AWS_ACCESS_KEY_ID && env.AWS_SECRET_ACCESS_KEY);

const logLine = (o) => console.log(JSON.stringify({ fn: 'saves', ...o }));

let handler = null;
if (configured) {
  const jwks = createJwksCache(env.NEON_AUTH_JWKS_URL);
  const issuer = new URL(env.NEON_AUTH_BASE_URL).origin;
  const storage = createS3Storage({
    endpoint: env.AWS_ENDPOINT_URL_S3,
    region: env.AWS_REGION || 'eu-central-1',
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
  });
  handler = createHandler({
    storage,
    auth: (token) => verifyJwt(token, { getKey: (kid) => jwks.getKey(kid), issuer }),
    log: logLine,
  });

  // Once per isolate, read-only: the JWKS loads and a signed HEAD of a missing key gives 404
  // (a 403 would mean the SigV4 signing or the credentials are wrong).
  (async () => {
    const out = { event: 'startup' };
    try {
      await jwks.refresh();
      out.jwks_keys = jwks.size;
    } catch (e) {
      out.jwks_error = String(e.message || e).slice(0, 100);
    }
    try {
      out.storage = (await storage.head('selfcheck/missing')) === null ? 'ok' : 'unexpected';
    } catch (e) {
      out.storage = String(e.message || e).slice(0, 100);
    }
    logLine(out);
  })();
}

export default {
  async fetch(request) {
    if (!handler) {
      if (!bearerToken(request.headers.get('authorization'))) return json(401, { ok: false, error: 'unauthorized' });
      logLine({ event: 'misconfigured' });
      return json(503, { ok: false, error: 'unavailable' });
    }
    return handler(request);
  },
};

// The "playing now" counter of openOMSI: a running game says every three minutes that it is
// being played (crates/omsi-app/src/presence.rs; every ten minutes, every three in games
// before 0.1.1552), the website and the README badge read how many are. One Durable Object
// keeps the sessions in memory (a random id each, its system and the game's version, the time of its
// last word) and forgets one 25 minutes after that. Everything has to fit the free plan's
// 100 000 requests a day: over it Cloudflare answers every request with error 1027 until
// midnight UTC.
// No address and nothing else about a player is kept.
//
//   POST /ping  {"id": "<32 hex>", "v": "0.1.1512", "os": "windows"}   -> 204
//   POST /bye   {"id": "<32 hex>"}                                       -> 204
//   GET  /players -> {"players": 12, "systems": {"windows": 9, ...}, "updated": "...", "next": "..."}
//                    (the count of the ten minutes since `updated`; the next at `next`)
//   GET  /badge   -> the same count for a shields.io endpoint badge

import { DurableObject } from "cloudflare:workers";

const ALIVE_MS = 25 * 60 * 1000;
// The count is taken once every ten minutes, on the clock (:00, :10, :20, ...), and the
// website and the badge both show that one: each read it at its own moment, through caches
// of its own (Cloudflare's per place, shields.io's, GitHub's), and showed other numbers.
const WINDOW_MS = 10 * 60 * 1000;
const windowStart = (now) => Math.floor(now / WINDOW_MS) * WINDOW_MS;
const SYSTEMS = ["windows", "macos", "linux", "android"];

export class Presence extends DurableObject {
  // (in memory, not in the object's storage: a session lives 25 minutes, so a restarted
  // object is right again within one ping period, and nothing is read or written per ping)
  constructor(ctx, env) {
    super(ctx, env);
    this.sessions = new Map();
    this.snapshot = null;
  }

  forget(now) {
    for (const [id, s] of this.sessions) {
      if (s.seen < now - ALIVE_MS) this.sessions.delete(id);
    }
  }

  async ping(id, os, v) {
    this.sessions.set(id, { seen: Date.now(), os, v });
  }

  async bye(id) {
    this.sessions.delete(id);
  }

  // the count of this ten minutes (taken at its first read; `updated` is the window's start
  // and `next` the next one's)
  async count() {
    const now = Date.now();
    const start = windowStart(now);
    if (!this.snapshot || this.snapshot.at !== start) {
      this.forget(now);
      const systems = {};
      for (const s of this.sessions.values()) {
        systems[s.os] = (systems[s.os] || 0) + 1;
      }
      this.snapshot = { at: start, players: this.sessions.size, systems };
    }
    const { players, systems } = this.snapshot;
    return { players, systems, updated: new Date(start).toISOString(), next: new Date(start + WINDOW_MS).toISOString() };
  }
}

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

function json(body, status = 200, extra = {}) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", ...CORS, ...extra } });
}

async function body(request) {
  try {
    const b = await request.json();
    return b && typeof b === "object" ? b : null;
  } catch {
    return null;
  }
}

const validId = (id) => typeof id === "string" && /^[0-9a-f]{32}$/.test(id);

export default {
  async fetch(request, env, ctx) {
    try {
      return await handle(request, env, ctx);
    } catch (e) {
      // (the counter unreachable: say why, and ask the games to wait as for a 429)
      return new Response(`openOMSI presence: ${e}\n`, { status: 503, headers: { "Content-Type": "text/plain", "Retry-After": "1800", ...CORS } });
    }
  },
};

async function handle(request, env, ctx) {
  const url = new URL(request.url);
  const counter = env.PRESENCE.get(env.PRESENCE.idFromName("openomsi"));
  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers: CORS });
  }
  if (request.method === "POST" && url.pathname === "/ping") {
    const b = await body(request);
    if (!b || !validId(b.id)) return json({ error: "bad id" }, 400);
    const os = SYSTEMS.includes(b.os) ? b.os : "other";
    const v = typeof b.v === "string" ? b.v.slice(0, 32) : "";
    await counter.ping(b.id, os, v);
    return new Response(null, { status: 204, headers: CORS });
  }
  if (request.method === "POST" && url.pathname === "/bye") {
    const b = await body(request);
    if (b && validId(b.id)) await counter.bye(b.id);
    return new Response(null, { status: 204, headers: CORS });
  }
  if (request.method === "GET" && (url.pathname === "/players" || url.pathname === "/badge")) {
    // (the ten minutes' count, cached until the next ten minutes begin - by Cloudflare here,
    // by browsers and by shields.io, which keeps a badge 5 minutes at least: the website and
    // the badge can be asked as often as anybody likes, and say the same)
    const cache = caches.default;
    const key = new Request(url.origin + url.pathname);
    const hit = await cache.match(key);
    if (hit) return hit;
    const c = await counter.count();
    const left = Math.max(15, Math.ceil((Date.parse(c.next) - Date.now()) / 1000));
    const headers = { "Cache-Control": `public, max-age=${left}` };
    const out = url.pathname === "/badge"
      ? json({ schemaVersion: 1, label: "playing now", message: String(c.players), color: c.players > 0 ? "brightgreen" : "lightgrey", cacheSeconds: Math.max(300, left) }, 200, headers)
      : json(c, 200, headers);
    ctx.waitUntil(cache.put(key, out.clone()));
    return out;
  }
  return new Response("openOMSI presence: GET /players, GET /badge\n", { status: url.pathname === "/" ? 200 : 404, headers: { "Content-Type": "text/plain", ...CORS } });
}

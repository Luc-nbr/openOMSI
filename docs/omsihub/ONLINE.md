# Omsi-Hub online: accounts and friends

An account is optional. Without one, Omsi-Hub plays fully local and only the online features are
missing (friends, online state, invites). A player signs in with email and a password, or with a
Google account. Nothing else (Luc, 2026-10-16: "we doen alleen google of email", website and app).

## Moving to Supabase

The online side moves from Neon to Supabase. The **website runs on Supabase**, and so does the app's
client (`crates/omsihub-online`) from this repository; the Betas built from the fork's `beta` still use
the Neon endpoints below until the client lands there too. Accounts made on one are not on the other.

| What | Value |
|---|---|
| Supabase project | `omsi-hub-online` (`mlatjtjjdfifbgddcxzu`), region `eu-central-1` |
| URL (Auth `/auth/v1`, REST `/rest/v1`, Storage `/storage/v1`) | `https://mlatjtjjdfifbgddcxzu.supabase.co` |
| Publishable key (the `apikey` header) | `sb_publishable_LF17d5vnctmpyH1h6dRprw_s3CGcD-P` |
| Schema, policies and the `saves` bucket | `online/supabase/schema.sql` |

The publishable key is public by design. What a request may do is decided by row-level security and
the functions' grants: the anonymous role may only call `hub_leaderboard` and `hub_board_counts`;
everything else needs a signed-in player, and every write goes through a function that takes the
player from the JWT. The shared helpers live in the `private` schema, which the REST API does not show.

What changed for the app's client ("The app's side" below):

- auth: Supabase Auth instead of Better Auth (`/signup`, `/token?grant_type=password|refresh_token`,
  `/verify` with `type=email|recovery`, `/resend`, `/recover`, `PUT /user`, `/logout?scope=...`),
  the access token renewed with the refresh token;
- the functions: the same names and arguments at `/rest/v1/rpc/<name>`, plus `report_stats`;
- cloud saves: Storage objects under `saves/users/<user id>/slot1|slot2/` instead of the `saves`
  function; deleting the account removes them first, then calls `delete_my_account`.

### Google

Besides email and a password, a player can sign in with Google (Luc, 2026-10-16). Discord and Steam
were tried the same day and dropped: Google or email only, on the website and in the app.

- Google is a Supabase Auth provider. The website sends the player to
  `/auth/v1/authorize?provider=google&redirect_to=<page>` and gets the session back in the address.
  The button shows only when `/auth/v1/settings` says Google is switched on.
- An account made this way has no password: the website hides changing it, and deleting the account
  needs only the tick box.
- The app signs in through the system browser with PKCE and a loopback address on 127.0.0.1
  ("Google in the app" below).
- The Edge Function `steam-auth` that was deployed for Steam is not used. Remove it from the
  project with Luc's OK.

## Where it runs (Neon, until the app moves)

| What | Value |
|---|---|
| Neon project | `omsi-hub-online` (`frosty-water-75529121`), region `aws-eu-central-1` |
| Auth (Better Auth, via Neon Auth) | `https://ep-polished-silence-b9tp8jw4.neonauth.c-7.eu-central-1.aws.neon.tech/neondb/auth` |
| JWKS | `…/neondb/auth/.well-known/jwks.json` |
| Data API (PostgREST-style) | `https://ep-polished-silence-b9tp8jw4.apirest.c-7.eu-central-1.aws.neon.tech/neondb/rest/v1` |
| Cloud saves (Neon Function `saves`) | `https://br-shy-water-b9vx3gyb-saves.compute.c-7.eu-central-1.aws.neon.tech/` |

These are public endpoints that every client uses, not secrets. No database password or service
key belongs in the app or in this repository.

## What is stored

- **Neon Auth** (schema `neon_auth`, never exposed through the Data API) holds the email, the
  password hash and the sessions. Signing up, signing in, email verification, **password reset**
  (a code by email) and deleting the account are Neon Auth's own endpoints.
- **`public.profiles`** holds `user_id` (the auth user's id), `display_name` (3–24 characters,
  unique without regard to case), `friend_code` (8 characters, shared to be added), `status`,
  `session_code` (the company multiplayer code, seen by friends only) and `last_seen`.
  It holds no email.
- **`public.friendships`** has one row per pair: `requester`, `addressee`, and `status`
  (`pending` / `accepted` / `blocked`).
- **`public.invites`** holds an invite from one friend to another with a session code, valid for
  15 minutes.

## Security

- Row-level security is on for every table:
  - a player sees their own profile, their friends', and those of open requests;
  - a player sees their own friendships, but not that someone blocked them;
  - a player sees their own invites.
- The roles get their rights as follows:
  - `anonymous` (signed out) gets nothing;
  - `authenticated` gets SELECT only;
  - **every write goes through `SECURITY DEFINER` functions**, which take the user from the JWT
    (`auth.user_id()`, wrapped by `hub_me()`), never from an argument.
- Limits: 20 friend requests an hour, 500 friends, 30 invites an hour.
- Players are found only by their exact friend code (`find_player`). There is no search by name or
  email, so nobody can list the players.
- `delete_my_data()` removes a player's profile, friendships and invites. The app then deletes
  the auth account too.
- The app keeps its session token in the OS keychain/credential store and makes the auth and
  Data API calls from Rust, never from the page's JS.

## The functions (Data API: `POST /rpc/<name>`, signed in)

| Function | Does |
|---|---|
| `create_profile(p_name)` | the player's profile, with a fresh friend code (idempotent) |
| `set_display_name(p_name)` | rename |
| `heartbeat(p_status, p_session)` | online state (`online`/`away`/`busy`/`offline`) and the current session code; the app calls it about once a minute |
| `find_player(p_code)` | `user_id`, `display_name` of the friend code |
| `send_friend_request(p_code)` | returns `sent` / `accepted` (they had asked you) / `already` / `self` / `not_found` / `no_profile` / `too_many` / `full` |
| `respond_friend_request(p_from, p_accept)` | accept or decline |
| `remove_friend(p_other)` / `block_player(p_other)` | unfriend; block (also removes invites) |
| `send_invite(p_to, p_session)` / `dismiss_invite(p_id)` | invite a friend to your company session |
| `my_friends()` | friends and requests: name, status, `online` (seen in the last 2 minutes), session code (friends only), `relation`, `incoming` |
| `delete_my_data()` | everything of the player's in these tables |

## Cloud saves

A signed-in player can keep company saves online, so the same company is there on every PC they
sign in on. The code is `online/functions/saves/` (how to deploy: `online/README.md`).

### Cloud saves and local saves

- **The limit of 2 companies applies to cloud saves only.** A player can always create more
  companies as **local saves**. Local saves are unlimited and stay on that PC only. A local company
  is a full company: it can host company multiplayer like any other.
- When `upload_begin` answers `{error:'limit'}`, the app never blocks the company itself. It offers:
  - **keep it local**: the company goes on as a local save;
  - **replace one of your 2 cloud saves**: the app deletes that slot (`delete`) and uploads into it.
- Without an account everything is local, as before.

### How it works

- The save files sit in the **private** object-storage bucket `saves`, under
  `users/<user id>/slot<1|2>/`:
  - `v<version>.omsave` is the save file, compressed by the app with zstd before upload. The
    object's `x-amz-meta-*` headers carry the company name (URL-encoded), the sha256, the promised
    size and the device name.
  - `meta.json` is the slot's committed state (name, version, size, sha256, time, device and the
    kept versions). Only the function writes it.
- There is no database table for saves. The bytes never pass through the function: it hands out
  presigned S3 URLs, valid for 10 minutes, and the app uploads and downloads directly.
- Upload is two steps, `upload_begin` then `upload_commit`, so a half-finished upload never
  replaces a good save.

### Routes

Every route is `POST <function URL><route>` with a JSON body (`{}` when it takes nothing),
`Authorization: Bearer <Neon Auth JWT>` (the same token as for the Data API), and a JSON answer:
`{ok:true, ...}`, or `{ok:false, error:'<code>', ...}` with an HTTP error status.

| Route | Body | Answer |
|---|---|---|
| `list` | `{}` | `max_slots: 2`, `max_bytes`, `slots: [{slot, empty}` or `{slot, empty:false, name, version, size, sha256, updated_at, device, versions}]` |
| `upload_begin` | `{slot?, name, size, sha256, base_version, device?}` | `{slot, version, method:'PUT', url, headers, expires_in:600}` |
| `upload_commit` | `{slot, version, sha256?}` | the slot as in `list`, plus `committed:true` |
| `download` | `{slot, version?}` | `{slot, version, name, size, sha256, method:'GET', url, expires_in:600}` |
| `delete` | `{slot}` | `{slot, deleted}` (the number of objects) |
| `delete_all` | `{}` | `{deleted}` |

- **`upload_begin`**:
  - no `slot`: the first free slot is used. With both slots taken the answer is
    `409 {error:'limit', max_slots:2}` (see above).
  - `base_version` is the version the app last synced: `0` for a company that is not in the cloud
    yet. When it is not the current version, the answer is `409 {error:'conflict', current:{...}}`
    with the slot as it is now; the app then asks the player which copy to keep.
  - The app then sends `PUT url` with the file as the body and **exactly** the returned `headers`
    (`content-type`, `content-length`, `x-amz-meta-name`, `-sha256`, `-size`, `-device`). They are
    part of the signature, so another size or another hash is refused by the storage.
- **`upload_commit`**: the function checks with a HEAD that the object exists and that its size and
  sha256 are the promised ones (a mismatch deletes the upload: `422 {error:'mismatch'}`). It then
  writes `meta.json` and deletes the versions older than the last 3. A retry of the same commit is
  fine. The app should send the `sha256` too, which catches another device that overwrote the
  pending upload.
- **`download`**: without `version` it gives the current version; the last 3 are kept. The app
  checks the sha256 of the downloaded file before using it.
- **`delete_all`** removes everything of the player. The app calls it, after `delete_my_data()`,
  when the account is deleted.
- Error codes: `400 bad_request`, `401 unauthorized`, `404 not_found` / `not_uploaded`,
  `409 limit` / `conflict`, `413 too_large`, `422 mismatch`, `429 rate_limited`, `500 internal`.

### Limits

- 2 companies (slots) per account, in the cloud only;
- at most 25 MB per save, after zstd compression;
- the last 3 versions of each company are kept, so an account uses at most 2 × 3 × 25 MB;
- the presigned URLs are valid for 10 minutes;
- 60 requests a minute per player, counted per function instance (in memory);
- a request body of at most 4 KB; company and device names of at most 64 characters.

### Security

- **Authentication.** Every request needs a valid Neon Auth JWT, checked by the function itself:
  - the EdDSA (Ed25519) signature, against the JWKS (cached for an hour; an unknown `kid` refetches
    it, at most every 30 s);
  - `exp`, `nbf` and `iat`, with 60 s of leeway, and `iss` equal to the origin of the auth URL;
  - only `Authorization: Bearer` is accepted.
  
  Without a valid token every path answers 401, even one that does not exist.
- **Isolation.** The storage prefix is built only from the token's verified `sub` (the user id),
  never from the request, so a player can never reach another player's files. Each presigned URL
  is for one key and one method, and lasts 10 minutes.
- **Validation.** Every input is checked: slot 1 or 2, versions as integers, sha256 as 64 hex
  characters, the size, name lengths, no control characters, and no unknown fields.
- **No browser access.** The function sends no CORS headers: the app calls it from Rust, never
  from the page's JS.
- **Errors and logs.** The errors are generic codes. The log holds the route, the status and a
  short hash of the user id. It never holds a token, a presigned URL or a storage key.
- **Credentials.** The S3 credentials are the ones Neon injects into the function. They never
  leave it, and none is in the app or this repository.

## The app's side

The client is the crate `crates/omsihub-online` (Rust, `ureq` with rustls); the launcher's commands
are `crates/omsi-launcher-api/src/omsihub/online.rs` (`ol_…`), the page's panel
`launcher/ui/js/omsihub/online.js` with `launcher/ui/omsihub/online.css`, the texts
`crates/omsi-app/locales/online.yml`. The page's JS never talks to Supabase: it calls Rust, and Rust makes
every request.

### The auth routes used (Supabase Auth)

The base is `<URL>/auth/v1`. Every request carries the publishable key as `apikey`; the calls after
signing in carry the access token as `Authorization: Bearer`. All are `POST` with a JSON body unless
said. **An account is made on the website** (`account.html#sign-up`, `omsihub_online::sign_up_page`):
the app's "Create an account" opens it, and has no form of its own. The display name chosen there
(the user's `user_metadata.display_name`) becomes the profile (`create_profile`) at the first sign-in
in the app, when the website has not made it yet.

| Route | Body | Used for |
|---|---|---|
| `/token?grant_type=password` | `email`, `password` | signing in: `access_token`, `refresh_token`, `user` |
| `/token?grant_type=refresh_token` | `refresh_token` | a new access token (and a new refresh token, which replaces the stored one) |
| `/resend` | `type: "signup"`, `email` | "Send a new code" (an address not confirmed yet: the sign-in answers `email_not_confirmed`) |
| `/verify` | `type: "email"` (else `"signup"`), `email`, `token` | the email confirmed by its code; it signs in |
| `/recover` | `email` | "Forgot your password?" (the same answer whether the address has an account or not) |
| `/verify` | `type: "recovery"`, `email`, `token` | the reset's code; it signs in, then `PUT /user` `{password}` sets the new password |
| `/token?grant_type=pkce` | `auth_code`, `code_verifier` | Google and Discord: the browser's code exchanged ("Google, Discord and Steam in the app") |
| `GET /user` | - (bearer) | only before deleting the account: the address to check the password against (none for an account without a password) |
| `/logout?scope=local` | - (bearer) | signing out (best effort: the token goes from the store whatever the answer) |

The account is deleted with the database's `delete_my_account()` (it deletes the auth's user, and with
it the profile, friendships and invites), after the password was checked by signing in with it and
the player's cloud saves were deleted through Storage. Supabase Auth's errors (`{code, error_code,
msg}`, or an older `{error, error_description}`) are read for their code only. No cookie and no
`Origin` header is sent.

### Google, Discord and Steam in the app

`crates/omsihub-online/src/oauth.rs`, the commands `ol_oauth`, `ol_oauth_wait`, `ol_oauth_cancel`.
The buttons ("Sign in with Google/Discord/Steam", under the email sign-in, after "or") are disabled
until the privacy tick is checked, as the email sign-in is.

1. `ol_oauth {provider, agree}` starts a listener on `127.0.0.1` with a free port chosen by the
   system (`http://127.0.0.1:<port>/callback`), a random `state` (16 bytes) and a PKCE verifier
   (32 bytes, `ring`'s system generator; the challenge is S256). The page opens the answered address
   in the system's browser (`open_url`) and calls `ol_oauth_wait {id, agree}`, which waits on its own
   worker for five minutes at most; "Cancel" (`ol_oauth_cancel`) ends the wait at once.
2. **Google and Discord** (Supabase's providers): the browser goes to
   `<URL>/auth/v1/authorize?provider=google|discord&redirect_to=http://127.0.0.1:<port>/callback?state=<state>&code_challenge=<S256>&code_challenge_method=s256`
   and comes back with `?code=…&state=…`. A request with another `state` is turned away and the
   listener waits on. The code is exchanged with `POST /auth/v1/token?grant_type=pkce`
   `{auth_code, code_verifier}` (the `apikey` header), and the session kept as a password's is.
3. **Steam**: the browser goes to `<URL>/functions/v1/steam-auth?to=http://127.0.0.1:<port>/callback`.
   Supabase's one-time link ends at that address with the session in the fragment
   (`#access_token=…&refresh_token=…`), which never reaches a server: the listener's page posts
   `location.hash` back to it (a form with this flow's `state`, the fragment first taken out of the
   address and the history). The refresh token is then renewed with the auth at once
   (`grant_type=refresh_token`, which checks it), and only that answer is kept.
4. The browser tab shows a small Omsi-Hub page in the launcher's language: signed in ("you may close
   this tab"), cancelled, or failed. A provider's own error text is never shown, here or there.
5. Then as after a password: `my_privacy` / `accept_privacy` with the tick, and an account without a
   profile gets "Choose your display name" (Google's and Discord's names are not taken over).

The listener answers only `Host: 127.0.0.1:<its port>` (a name rebound to 127.0.0.1 is turned away),
the posted fragment only from its own origin, with no-store, no-referrer and a strict CSP; it stops at
the first answer. Nothing of it is logged. The codes: `oauth_cancelled` (the player said no, or
cancelled), `oauth_timeout`, `oauth_failed`.

An account made with Google, Discord or Steam has no password (`app_metadata.providers` without
`email`, or a `steam_id`): "Delete account" asks for a tick instead of the password, as the website
does.

**To set in Supabase (Luc):**

- Auth > Sign In / Providers: **Google** and **Discord** switched on, each with its client id and
  client secret (Google Cloud console's OAuth client, the Discord developer portal's application).
  Their callback (redirect URI) at Google and Discord is Supabase's own,
  `https://mlatjtjjdfifbgddcxzu.supabase.co/auth/v1/callback`. (The website needs this too.)
- Auth > URL Configuration > Redirect URLs: add **`http://127.0.0.1:*/callback*`**. Supabase matches
  the whole address with globs whose separators are `.` and `/`: the first `*` takes any port, the
  last one the `?state=…` (base64url: no `.` or `/`), so both `…/callback?state=…` (Google, Discord)
  and `…/callback` (Steam's one-time link) match, and no other path does. Without it Supabase sends
  the browser to the Site URL instead and the app waits until it times out. (`http://127.0.0.1/**`
  would not do: the port stands between the host and the `/`.) A stricter form, if the dashboard
  takes character classes: `http://127.0.0.1:[0-9][0-9][0-9][0-9][0-9]/callback*` (the systems'
  free ports have five digits). The plain `*` also lets through a dot-less name after an `@`
  (`127.0.0.1:1@intranet/callback`): the app's PKCE code is worthless there without its verifier,
  but a crafted sign-in link without PKCE could hand a session to such a name on the player's own
  network. Prefer the stricter form when Supabase accepts it.
- Steam: deploy the function `steam-auth` again (`online/supabase/functions/steam-auth/index.ts`
  and `steam.mjs`, `verify_jwt` off as before): its `allowed()` takes
  `http://127.0.0.1:<1-65535>/callback` and nothing else besides `PAGES`.

The stand-in (`online/standin/standin.mjs`) answers `authorize` (no provider's page: a
`<provider>-player@<provider>.standin` signs in at once; `&standin_answer=deny` says no), the `pkce`
exchange and a `steam-auth` of its own, with the same redirect globs (`STANDIN_REDIRECTS`); the
example `cargo run -p omsihub-online --example standin` runs all three through a scripted browser.

### The refresh token, the access token

- The refresh token is kept in the OS's credential store only (the `keyring` crate: Windows'
  Credential Manager, macOS's Keychain, Linux's kernel keyring), under "Omsi-Hub online" /
  "session". Where there is none (Android, iOS) it lives in memory, and the player signs in again
  at the next start. It is never written to a file, a log or the company's save. A Better Auth
  session kept there from before the move is refused at the first renewal, which signs out.
- The access token (a JWT, about an hour) lives in memory only. It is renewed a minute before its
  `exp`, and once more when the database or Storage answers 401 (PostgREST's `PGRST301`/`PGRST303`,
  Storage's `InvalidJWT`); a second 401, or the auth refusing the refresh token (used, revoked, the
  user gone), signs out: the token is deleted from the store and the page shows the sign-in again.
  The auth being down or busy (5xx, 429) does not sign out.
- The email address and the password are passed through and kept nowhere (the form clears them
  once signed in).

### The cloud saves (Supabase Storage)

`crates/omsihub-online/src/saves.rs`: the bucket `saves`, the player's folder `users/<user id>/`, two
slots. A slot holds `v<version>.omsave` (the company's archive compressed with zstd, level 12; the
last 3 versions) and `meta.json` (the company's name, the device, and each kept version's size,
sha256 and time). An upload is three steps: `saves_upload_begin` reads `meta.json` and holds the
version it was made from (`base_version`, 0 for a company not in the cloud yet) against it, another
one answering `conflict` with the slot as it is now, and with both slots taken an upload without a
slot answers `limit`; `Transfer::put` sends the file (`POST /storage/v1/object/saves/<path>`,
`x-upsert: true`) with the access token the begin fetched, outside the client's lock; then
`saves_upload_commit` writes `meta.json` (unless another computer committed since: `conflict`) and
deletes the versions past the last 3. The file goes first and `meta.json` after it, so a
half-finished upload never replaces a good save (the next upload goes over the stray file). Two
computers sending the same version in the same seconds is not caught before the commit: the
second's file may stand under the first's `meta.json`, which a download then refuses (`bad_save`).
A download (`GET /storage/v1/object/authenticated/saves/<path>`) is checked against `meta.json`'s
size and sha256 before it is decompressed (`bad_save`). A slot is emptied by listing it
(`POST /storage/v1/object/list/saves`) and deleting its files (`DELETE /storage/v1/object/saves`,
`{prefixes}`).

### Cloud saves in the app

The client's half is `crates/omsihub-online/src/saves.rs` (Storage, zstd, the transfer outside
the client's lock), a company's archive `crates/omsi-launcher-core/src/company/cloud.rs`, the sync
`crates/omsi-launcher-api/src/omsihub/cloud.rs` (`cs_…`), the page's parts
`launcher/ui/js/omsihub/cloud.js` with `launcher/ui/omsihub/cloud.css`, the texts
`crates/omsihub-locales/locales/cloud-saves.yml`.

- **One cloud save** is one archive (`OMSIHUB-SAVE 1`, the files in their names' order, so the
  same company always packs the same): `company.json` (the company's file), `orders.jsonl` (the
  phone's orders still waiting) and `live.jsonl` / `live-<date>.taken` (what the game reported
  since the day's last close). The game's plans of the day (`<id>.liveplan.json`,
  `<id>.drive.liveplan.json`) stay out: the launcher writes them from the company, and a stale one
  goes when a save comes in. The archive is compressed with zstd (level 12) before the upload.
- **The sync metadata** of each company (`#[serde(default)]`: a company without any is local)
  lives in the launcher's own `~/.openomsi/omsihub-cloud.json`: the place (`cloud`/`local`), the
  account's user id, the slot, the version last synced, its sha256 in the cloud and the sha256
  of the archive as it was then (another one now is a change of this computer's).
- **Opening** a cloud company asks `list` first (at most a few seconds; offline it opens as it
  is): a newer version only in the cloud is offered (download, or not now); changes on both sides
  are asked with both dates and computers - keep the cloud's, keep this PC's (uploaded on top;
  the cloud keeps the older versions), or keep both (this PC's as "<name> (this PC)", a local
  save). A slot emptied or taken by another company elsewhere leaves the company here local.
- **Uploading**: a changed cloud company goes up when its pages are left, every ten minutes
  while it changes, and after a start; always on top of the version last synced, so another
  computer's newer version is a conflict, never overwritten.
- **The limit**: "Put this company in the cloud" with both slots taken offers "Keep it local" or
  "Replace <name>" (armed by a second click); the replaced company stays on this PC as a local
  save. Local companies are never blocked and can host company multiplayer.
- Deleting the account deletes the cloud saves (every file of both slots) after the password is
  checked and before `delete_my_account()`; a failure stops it before the account is deleted.
  Every company here is local afterwards.

### Online state and invites

- The launcher's worker calls `heartbeat` about once a minute while signed in, and within five
  seconds of a change: `busy` with the session's code while the player hosts a company session
  ("Play together"), `away` after five minutes with the window in the background, else `online`.
  As the launcher closes it sends `offline` (best effort, three seconds at most).
- `my_friends()` and the `invites` table (`GET /invites?select=*`, newest first; only those sent to
  the player, at most 15 minutes old) are fetched with each beat, and when the panel opens (at
  most every ten seconds). The page asks Rust every five seconds, and Rust answers from what it
  fetched: the page never waits on the network.
- "Invite to my company" sends the session hosted now (`send_invite`); a friend's "Join" (on the
  invite's notice, or on a friend in a session) joins with that code through "Play together"'s own
  join (`tg_join`), as a typed code would.
- Without a network every call answers "offline" at once (the connection's own timeouts, at most a
  few seconds, on the worker's thread or the command's), the session stays, and the panel says
  "Offline" quietly; nothing else of the launcher changes.

### Security checklist

| | Done |
|---|---|
| HTTPS only, the certificates checked (rustls, the Mozilla roots; `https_only` on the agent; an `http://` endpoint is refused; no redirects followed) | done |
| No secrets in the repository or the app (the URL and the publishable key are public; no database password, no service or secret key) | done |
| The refresh token only in the OS's credential store; the access token in memory only; nothing logged | done (Android/iOS: memory only, by design) |
| The lengths checked on the computer too (email ≤ 254, password 8–72 (bcrypt's bytes), display name 3–24 of `A-Za-z0-9_. -`, friend code 8, email code 6–10 digits, ids, session code ≤ 1024, a cloud save ≤ 25 MB; the page's fields have the same `maxLength`) | done |
| The server's error texts never shown: every answer is one of our codes, with a translated text (`codes::TEXTS`) | done |
| Every function's result code (`sent`, `accepted`, `already`, `self`, `not_found`, `no_profile`, `too_many`, `full`) has a translated message, as have Supabase Auth's codes, Storage's and the SQLSTATEs met (`23505` the name taken, ...) | done |
| Deleting the account asks for the password again; nothing is deleted when it is wrong | done |
| A token goes only where it belongs: the refresh token in the auth's body only, the access token as the bearer | done |
| Certificate pinning | not done (the Mozilla roots are trusted, as browsers do) |
| A proxy from the environment (`HTTPS_PROXY`) | not used: behind a proxy-only network the app is "offline" |

The tests (`cargo test -p omsihub-online`) run the client against a mock server on 127.0.0.1;
none reaches the real project. To try every flow end to end without it, `online/standin/standin.mjs`
is a stand-in for the Supabase project on this computer (Auth, the schema's functions and its
row-level rules, Storage; in memory, the mails' codes printed), and
`cargo run -p omsihub-online --example standin` drives the app's flows against it: two players made
as the website makes them, then signing in, the email's code, friends, the heartbeat, invites, the
leaderboard, cloud saves, the token renewal, the password reset and deleting the account. The
launcher's own path is `crates/omsi-launcher-api/examples/cloud_standin.rs`: a debug build reads
`OMSIHUB_STANDIN` (a stand-in on this computer only; a release build never does, and the session
then stays in memory), and the example drives `ol_…` and `cs_…` as the page does, with a home of
its own: the limit, a replace, a change going up, another computer's newer version, a conflict with
both kept, a download, a removal and the account deleted.

## The privacy statement

[PRIVACY.md](PRIVACY.md) is the statement, for the app and the website alike; its version is the
`<!-- privacy-version: N -->` marker, and nowhere else (`omsihub_online::privacy::VERSION` reads it
while the program is built; a test fails when the two disagree). The server keeps who agreed to
which version (`privacy_consents`, `accept_privacy(p_version)`, `my_privacy()`; schema.sql).

- Playing without an account asks nothing.
- The account panel's Sign in (and the email's code, and the password reset) and its "Create an
  account" wait for the tick "I have read and agree to the privacy statement"; once signed in,
  `accept_privacy` is sent.
- Every sign-in and every restored session asks `my_privacy()`. Below the current version (a new
  one, or an account made before there was one) the page asks over everything - a short summary,
  the whole text, Agree or Sign out - and the account does nothing meanwhile: the client refuses
  every request but those two functions (and deleting the account) with the code `privacy`, so the
  heartbeat, the friends, the invites, the leaderboard and the cloud saves wait.
- The statement opens in the app from the tick, the Account panel, Settings > General and the
  cloud saves (their question and their slots). It stays English, the binding text; the dialog
  says so in the player's language.
- A new version: raise the marker in PRIVACY.md (and the website's copy); every player is asked
  again at their next sign-in or start.

## The website's side

The website (its own repository, [Luc-nbr/Omsi-Hub-Website](https://github.com/Luc-nbr/Omsi-Hub-Website),
plain HTML and JS) is on Supabase (see "Moving to Supabase"); `assets/auth.js` makes every call:

- `account.html#sign-up` makes an account (`/auth/v1/signup`, the display name in the user's
  metadata), then the email's code (`/auth/v1/verify`), then `create_profile` with the display name.
  The app's "Create an account" is to open this page.
- `account.html#sign-in`, `#forgot` and `#reset` sign in and reset the password by code;
  `account.html#me` (My Omsi-Hub) shows the profile and the service record, switches the
  leaderboard (`set_leaderboard`), changes the display name (`set_display_name`) and the password
  (the current one checked by signing in with it, then `PUT /auth/v1/user`), signs out elsewhere
  (`/logout?scope=others`) and deletes the account (the password again, the cloud saves through
  Storage, then `delete_my_account`).
- A link from a mail (`#access_token=...`) is taken as a session too, so a template with a link
  works as well as one with a code.
- Only the refresh token is kept, in `localStorage` ("keep me signed in") or `sessionStorage`,
  under `hub-session`; the access token stays in memory. No cookie is used.
- The leaderboard pages call `hub_leaderboard` and `hub_board_counts` with only the publishable key.

## The website's side

The website (its own repository, [Luc-nbr/Omsi-Hub-Website](https://github.com/Luc-nbr/Omsi-Hub-Website),
plain HTML and JS) uses the same account, with the same calls as the app (its `assets/auth.js`
mirrors `crates/omsihub-online/src/client.rs`):

- `account.html#sign-up` makes an account: `/sign-up/email`, then the email's code
  (`/email-otp/verify-email`) when verification is on, then `create_profile` with the display name.
  The app's "Create an account" is to open this page.
- `account.html#sign-in`, `#forgot` and `#reset` sign in and reset the password by code;
  `account.html#me` (My Omsi-Hub) shows the profile and the service record, switches the
  leaderboard (`set_leaderboard`), changes the display name (`set_display_name`) and the password
  (`/change-password`), lists the sessions (`/list-sessions`) and signs out elsewhere
  (`/revoke-other-sessions`).
- Deleting the account stays in the app: only the app can reach the `saves` function, which has no
  CORS on purpose, so only the app removes the cloud saves with the account.
- The session token is kept in the browser's `localStorage` ("keep me signed in") or
  `sessionStorage`, under `hub-session`; the JWT in memory only. No cookie is used.
- Better Auth only answers a browser whose page origin it trusts. Localhost is allowed
  (`allow_localhost`); **the site's own address has to be added as a trusted domain** (Neon Auth's
  trusted domains, or the MCP `add_auth_trusted_domain`) before the live site can sign in. Until
  then sign-in on the live site fails with "This website is not allowed to sign in yet".

## To do in the Neon console (not available through the API)

- Auth › Email & password: turn on **verify email on sign-up** / **require email verification**.
  Only a confirmed address can then keep an account and receive a password reset. It also keeps
  throwaway accounts from filling the cloud-save storage (up to 150 MB each).

  Only a confirmed address can then keep an account and receive a password reset.
- Auth: choose **verification by code (OTP)**, not by link, for the email confirmation and the
  password reset: the app has a field for the code and no page for a link. Check that a code is
  sent on sign-up (Better Auth's "send verification on sign-up").
- Auth: allow **deleting the user** (`user.deleteUser.enabled`); without it `/delete-user`
  answers 404 and the app says "The server does not offer this yet" (the data are deleted then,
  the auth's user is not).
- Check the **bearer plugin** is on (the sign-in answers a `set-auth-token` header); the app falls
  back to the session cookie and the body's token, but only the bearer plugin takes the token back
  as `Authorization: Bearer`.
- Optionally turn on "sign in after verification", which saves the player one step.

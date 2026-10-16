# Omsi-Hub online: accounts and friends

An account is optional. Without one, Omsi-Hub plays fully local and only the online features are
missing (friends, online state, invites). The account uses email and a password only, with no
Google, Discord or other OAuth sign-in.

## Where it runs

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
`crates/omsi-app/locales/online.yml`. The page's JS never talks to Neon: it calls Rust, and Rust
makes every request.

### The auth routes used (Better Auth 1.7, as `@neondatabase/auth` 0.6 pins it)

Neon Auth runs Better Auth with its **bearer**, **JWT** and **email-OTP** plugins (the ones the
Neon client turns on). All are `POST` with a JSON body unless said; the base is the auth URL above.

| Route | Body | Used for |
|---|---|---|
| `/sign-up/email` | `email`, `password`, `name` | a new account; `token` is null while the email waits for its code |
| `/sign-in/email` | `email`, `password`, `rememberMe: true` | signing in; the session token comes as the `set-auth-token` header (signed), else the session cookie, else the body's `token` |
| `/email-otp/send-verification-otp` | `email`, `type: "email-verification"` | "Send a new code" |
| `/email-otp/verify-email` | `email`, `otp` | the email confirmed; with "sign in after verification" it signs in too, else the app signs in with the password the form still has |
| `/email-otp/request-password-reset` | `email` | "Forgot your password?" (the older `/forget-password/email-otp` is tried where this one answers 404) |
| `/email-otp/reset-password` | `email`, `otp`, `password` | the new password; the app signs in with it at once |
| `GET /token` | - (`Authorization: Bearer <session>`) | the JWT for the Data API (15 minutes by default) |
| `GET /get-session` | - (bearer) | only before deleting the account: the address to check the password against |
| `/sign-out` | `{}` (bearer) | signing out (best effort: the token goes from the store whatever the answer) |
| `/delete-user` | `password` (bearer) | the auth's user deleted, after `delete_my_data()` |

No cookie and no `Origin` header is sent: Better Auth checks the origin only for requests that
carry cookies (or a browser's `Sec-Fetch-*` headers), so a native client needs no trusted domain.
Better Auth's errors (`{code, message}`) are read for their `code` only.

### The session token, the JWT

- The session token is kept in the OS's credential store only (the `keyring` crate: Windows'
  Credential Manager, macOS's Keychain, Linux's kernel keyring), under "Omsi-Hub online" /
  "session". Where there is none (Android, iOS) it lives in memory, and the player signs in again
  at the next start. It is never written to a file, a log or the company's save.
- The JWT lives in memory only. It is fetched again a minute before its `exp`, and once more when
  the Data API answers 401; a second 401, or `/token` refusing the session (401/403), signs out:
  the token is deleted from the store and the page shows the sign-in again. A session the auth
  renews (`set-auth-token` on `/token`) replaces the stored one.
- The email address and the password are passed through and kept nowhere (the form clears them
  once signed in).

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

### Cloud saves in the app

The client's half is `crates/omsihub-online/src/saves.rs` (the six routes, zstd, the presigned
transfers), a company's archive `crates/omsi-launcher-core/src/company/cloud.rs`, the sync
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
- Deleting the account calls `delete_all` after `delete_my_data()` and before `/delete-user`; a
  failure stops it before the user is deleted. Every company here is local afterwards.

### Security checklist

| | Done |
|---|---|
| HTTPS only, the certificates checked (rustls, the Mozilla roots; `https_only` on the agent; an `http://` endpoint is refused; no redirects followed) | done |
| No secrets in the repository or the app (the two URLs are public; no database password, no service key) | done |
| The session token only in the OS's credential store; the JWT in memory only; nothing logged | done (Android/iOS: memory only, by design) |
| The lengths checked on the computer too (email ≤ 254, password 8–128, display name 3–24, friend code 8, email code 4–10 digits, ids, session code ≤ 1024; the page's fields have the same `maxLength`) | done |
| The server's error texts never shown: every answer is one of our codes, with a translated text (`codes::TEXTS`) | done |
| Every function's result code (`sent`, `accepted`, `already`, `self`, `not_found`, `no_profile`, `too_many`, `full`) has a translated message, as have Better Auth's codes and the SQLSTATEs met (`23505` the name taken, ...) | done |
| Deleting the account asks for the password again; nothing is deleted when it is wrong | done |
| A token goes only where it belongs: the session token to the auth, the JWT to the Data API | done |
| Certificate pinning | not done (the Mozilla roots are trusted, as browsers do) |
| A proxy from the environment (`HTTPS_PROXY`) | not used: behind a proxy-only network the app is "offline" |

The tests (`cargo test -p omsihub-online`) run the client against a mock server on 127.0.0.1;
none reaches the real project.

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

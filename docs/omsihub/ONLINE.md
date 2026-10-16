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

## To do in the Neon console (not available through the API)

- Auth › Email & password: turn on **verify email on sign-up** / **require email verification**.
  Only a confirmed address can then keep an account and receive a password reset. It also keeps
  throwaway accounts from filling the cloud-save storage (up to 150 MB each).

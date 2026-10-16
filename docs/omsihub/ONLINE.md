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

## To do in the Neon console (not available through the API)

- Auth › Email & password: turn on **verify email on sign-up** / **require email verification**.
  Only a confirmed address can then keep an account and receive a password reset.

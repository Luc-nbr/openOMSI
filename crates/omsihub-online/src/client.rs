//! The client: Supabase Auth's routes (`/auth/v1`), the database's functions through PostgREST
//! (`/rest/v1/rpc/<name>`) and its tables the player may read; the cloud saves are `saves.rs`.
//!
//! The refresh token is the one thing kept, in the OS's store. The access token (a JWT, about an
//! hour) lives in memory and goes as `Authorization: Bearer` to every call; it is renewed with the
//! refresh token a minute before it expires, and once more when the database answers 401. A second
//! 401, or the auth refusing the refresh token, signs out (the token taken from the store). Supabase
//! hands out a new refresh token with every renewal, and the new one replaces the old one in the
//! store at once. Every request carries the project's publishable key (`apikey`); no cookie is sent.
//!
//! The account's features wait for the privacy statement (`privacy.rs`): until the server says the
//! player agreed to its current version (`my_privacy`, `accept_privacy`), every request with the
//! access token is refused here with `privacy` and nothing is sent. Only those two functions and
//! deleting the account go through.

use crate::codes::{self, Error};
use crate::http::{encode, Body, Http, Reply};
use crate::store::SecretStore;
use crate::{check, privacy, Endpoints};
use serde_json::{json, Value};

/// The player's own profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub user_id: String,
    pub display_name: String,
    pub friend_code: String,
    /// Standing on the public leaderboard (the player's choice, off until switched on).
    pub on_board: bool,
}

/// A row of `my_friends()`: a friend, or a request either way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Friend {
    pub user_id: String,
    pub display_name: String,
    /// `online` / `away` / `busy` / `offline`, as they last said.
    pub status: String,
    /// Seen in the last two minutes.
    pub online: bool,
    /// Their company session's code while they host one (friends only).
    pub session_code: Option<String>,
    /// `accepted`, `pending` (or `blocked`).
    pub relation: String,
    /// A request they sent (not one of ours).
    pub incoming: bool,
    /// When they were last seen (the server's timestamp, RFC 3339).
    pub last_seen: Option<String>,
}

/// An invite to a friend's company session.
#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    /// Its id as the server has it (`dismiss_invite` takes it back so).
    pub id: Value,
    pub from: String,
    pub to: String,
    pub session_code: String,
    pub created_at: Option<String>,
}

impl Invite {
    /// The id as a text (the page's).
    pub fn key(&self) -> String {
        match &self.id {
            Value::String(s) => s.clone(),
            v => v.to_string(),
        }
    }
}

/// The player's service record for the leaderboard (`report_stats`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub points: u64,
    pub level: u32,
    pub duties: u32,
    pub minutes_driven: u32,
    pub km: u32,
}

/// The access token and what it says.
struct Access {
    token: String,
    exp: u64,
    sub: String,
    email: String,
}

pub struct Client {
    pub(crate) ep: Endpoints,
    pub(crate) http: Http,
    store: Box<dyn SecretStore>,
    refresh: Option<String>,
    access: Option<Access>,
    /// The display name the website's sign-up put in the account (`user_metadata`), for the
    /// profile made on the first sign-in here.
    signed_up_name: Option<String>,
    /// The goodbye's short timeouts.
    quick: bool,
    /// The privacy statement's version the server said this user agreed to (the user id with it).
    pub(crate) privacy: Option<(String, u32)>,
    /// A request the privacy hold lets through (`rpc_free`, the account deleted).
    free: bool,
}

/// The access token is renewed this long before it expires.
const MARGIN: u64 = 60;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A field of a row under the first of its names that it has.
fn field<'a>(row: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| row.get(*n)).filter(|v| !v.is_null())
}

fn text(row: &Value, names: &[&str]) -> Option<String> {
    field(row, names).map(|v| match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    })
}

impl Client {
    /// The client, with the session the store kept (if any).
    pub fn new(ep: Endpoints, store: Box<dyn SecretStore>) -> Self {
        let refresh = store.load();
        Self { http: Http::new(!ep.plain), ep, store, refresh, access: None, signed_up_name: None, quick: false, privacy: None, free: false }
    }

    pub fn signed_in(&self) -> bool {
        self.refresh.is_some()
    }

    /// Whether the session outlives the program (kept in the OS's store).
    pub fn lasting(&self) -> bool {
        self.store.lasting()
    }

    /// The player's user id (the access token's subject), once there is one.
    pub fn user_id(&self) -> Option<&str> {
        self.access.as_ref().map(|a| a.sub.as_str()).filter(|s| !s.is_empty())
    }

    /// Signed in, and the current privacy statement not agreed to (as far as this client knows:
    /// `check_privacy` asks). The account's features wait meanwhile.
    pub fn privacy_needed(&self) -> bool {
        self.signed_in() && !self.agreed()
    }

    /// The newest version of the privacy statement this user accepted, as the server last said
    /// (0: none); None before it was asked.
    pub fn privacy_agreed(&self) -> Option<u32> {
        match (&self.privacy, self.user_id()) {
            (Some((user, v)), Some(me)) if user == me => Some(*v),
            _ => None,
        }
    }

    fn agreed(&self) -> bool {
        matches!((&self.privacy, self.user_id()), (Some((user, v)), Some(me)) if user == me && *v >= privacy::VERSION)
    }

    /// The display name chosen on the website's sign-up, while the profile is still to be made.
    pub fn signed_up_name(&self) -> Option<&str> {
        self.signed_up_name.as_deref()
    }

    pub(crate) fn send(&self, method: &str, url: &str, bearer: Option<&str>, body: Body, headers: &[(&str, &str)], file: bool) -> Result<Reply, Error> {
        self.http.send(method, url, &self.ep.key, bearer, body, headers, file, self.quick)
    }

    /// A call to the auth (`/auth/v1<path>`), its error as ours.
    fn auth(&self, method: &str, path: &str, body: Option<Value>, bearer: Option<&str>) -> Result<Value, Error> {
        let r = self.send(method, &self.ep.auth(path), bearer, body.as_ref().map_or(Body::None, Body::Json), &[], false)?;
        if r.ok() {
            Ok(r.body)
        } else {
            Err(codes::of_auth(r.status, &r.body))
        }
    }

    /// The session an answer brought (`access_token`, `refresh_token`), kept: the refresh token in
    /// the store, the access token in memory. Whether there was one.
    fn keep(&mut self, s: &Value) -> bool {
        let (Some(token), Some(refresh)) = (s.get("access_token").and_then(Value::as_str), s.get("refresh_token").and_then(Value::as_str).filter(|t| !t.is_empty())) else { return false };
        let Some((exp, sub, email)) = claims(token) else { return false };
        self.store.save(refresh);
        self.refresh = Some(refresh.to_string());
        self.access = Some(Access { token: token.to_string(), exp, sub, email });
        if let Some(n) = s.pointer("/user/user_metadata/display_name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()) {
            self.signed_up_name = Some(n.trim().to_string());
        }
        true
    }

    /// Signed out here: the refresh token out of the store, the access token forgotten.
    pub fn forget(&mut self) {
        self.refresh = None;
        self.access = None;
        self.signed_up_name = None;
        self.privacy = None;
        self.store.clear();
    }

    // --- the account ----------------------------------------------------------------------------
    // (an account is made on the website: `crate::sign_up_page`)

    /// Signing in (`POST /token?grant_type=password`).
    pub fn sign_in(&mut self, email: &str, password: &str) -> Result<(), Error> {
        let (email, password) = (check::email(email)?, check::password(password).map_err(|_| Error::Code("invalid_credentials"))?);
        let s = self.auth("POST", "/token?grant_type=password", Some(json!({ "email": email, "password": password })), None)?;
        if self.keep(&s) {
            Ok(())
        } else {
            Err(Error::Code("server"))
        }
    }

    /// A new code to confirm the email (`POST /resend`, the sign-up's mail).
    pub fn send_verification(&self, email: &str) -> Result<(), Error> {
        self.auth("POST", "/resend", Some(json!({ "type": "signup", "email": check::email(email)? })), None).map(|_| ())
    }

    /// The email confirmed by its code (`POST /verify`, `type: email`; a code from an older
    /// sign-up mail is `type: signup`); whether that signed in (it does on Supabase).
    pub fn verify_email(&mut self, email: &str, code: &str) -> Result<bool, Error> {
        let (email, code) = (check::email(email)?, check::email_code(code)?);
        let s = match self.auth("POST", "/verify", Some(json!({ "type": "email", "email": email, "token": code })), None) {
            Err(Error::Code("invalid_code")) => self.auth("POST", "/verify", Some(json!({ "type": "signup", "email": email, "token": code })), None)?,
            r => r?,
        };
        Ok(self.keep(&s))
    }

    /// A code to reset the password, by email (`POST /recover`). The answer is the same whether
    /// the address has an account or not.
    pub fn request_reset(&self, email: &str) -> Result<(), Error> {
        self.auth("POST", "/recover", Some(json!({ "email": check::email(email)? })), None).map(|_| ())
    }

    /// The new password with the code: the code signs in (`POST /verify`, `type: recovery`), then
    /// the password is set (`PUT /user`). Signed in afterwards.
    pub fn reset_password(&mut self, email: &str, code: &str, password: &str) -> Result<(), Error> {
        let (email, code, password) = (check::email(email)?, check::email_code(code)?, check::password(password)?);
        let s = self.auth("POST", "/verify", Some(json!({ "type": "recovery", "email": email, "token": code })), None)?;
        if !self.keep(&s) {
            return Err(Error::Code("server"));
        }
        let t = self.access_token()?;
        match self.auth("PUT", "/user", Some(json!({ "password": password })), Some(&t)) {
            // (the password is the one asked for already)
            Ok(_) | Err(Error::Code("same_password")) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Signing out (`POST /logout?scope=local`, best effort: the token goes here whatever the
    /// server says).
    pub fn sign_out(&mut self) {
        if self.signed_in() {
            if let Ok(t) = self.access_token() {
                let _ = self.auth("POST", "/logout?scope=local", None, Some(&t));
            }
        }
        self.forget();
    }

    /// The account deleted, after the password typed again: signed in afresh with it (which checks
    /// it), the cloud saves deleted through Storage, then `delete_my_account()` (the user, and with
    /// it the profile, the friendships and the invites), then signed out here. Nothing is deleted
    /// when the password is wrong; a cloud save that will not go keeps the account.
    pub fn delete_account(&mut self, password: &str) -> Result<(), Error> {
        let t = self.access_token()?;
        let r = self.send("GET", &self.ep.auth("/user"), Some(&t), Body::None, &[], false)?;
        if r.status == 401 || r.status == 403 {
            self.forget();
            return Err(Error::SignedOut);
        }
        if !r.ok() {
            return Err(codes::of_auth(r.status, &r.body));
        }
        // (the address only for this, in memory)
        let email = r.body.get("email").and_then(Value::as_str).map(str::to_string).or_else(|| self.access.as_ref().map(|a| a.email.clone())).filter(|e| !e.is_empty()).ok_or(Error::Code("server"))?;
        self.sign_in(&email, password).map_err(|e| if e == Error::Code("invalid_credentials") { Error::Code("wrong_password") } else { e })?;
        // (deleting is open whether the privacy statement was agreed to or not)
        self.free = true;
        let r = self.saves_delete_all().and_then(|_| self.rpc("delete_my_account", json!({})));
        self.free = false;
        r?;
        self.forget();
        Ok(())
    }

    // --- the privacy statement ----------------------------------------------------------------------

    /// The newest version of the privacy statement the player agreed to (`my_privacy()`, 0: none),
    /// kept for the hold.
    pub fn check_privacy(&mut self) -> Result<u32, Error> {
        let v = self.rpc_free("my_privacy", json!({}))?;
        self.privacy_is(&v)
    }

    /// The player agrees to the current privacy statement (`accept_privacy`); the newest version
    /// agreed to.
    pub fn accept_privacy(&mut self) -> Result<u32, Error> {
        let v = self.rpc_free("accept_privacy", json!({ "p_version": privacy::VERSION }))?;
        self.privacy_is(&v)
    }

    fn privacy_is(&mut self, v: &Value) -> Result<u32, Error> {
        // (PostgREST answers a scalar as it is; a row or a text is read too)
        let n = match v {
            Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
            Value::Object(o) if o.len() == 1 => o.values().next().cloned().unwrap_or(Value::Null),
            v => v.clone(),
        };
        let n = n.as_u64().or_else(|| n.as_str().and_then(|s| s.trim().parse().ok())).or(n.is_null().then_some(0)).ok_or(Error::Code("server"))?;
        let n = u32::try_from(n).map_err(|_| Error::Code("server"))?;
        let me = self.user_id().ok_or(Error::Code("server"))?.to_string();
        self.privacy = Some((me, n));
        Ok(n)
    }

    /// A function the privacy hold lets through.
    fn rpc_free(&mut self, name: &str, args: Value) -> Result<Value, Error> {
        self.free = true;
        let r = self.rpc(name, args);
        self.free = false;
        r
    }

    // --- the access token and the database ---------------------------------------------------------

    /// An access token good for a minute at least: the one in memory, or a new one for the refresh
    /// token (`POST /token?grant_type=refresh_token`).
    pub fn access_token(&mut self) -> Result<String, Error> {
        if let Some(a) = &self.access {
            if a.exp > now() + MARGIN {
                return Ok(a.token.clone());
            }
        }
        let refresh = self.refresh.clone().ok_or(Error::Code("not_signed_in"))?;
        let r = self.send("POST", &self.ep.auth("/token?grant_type=refresh_token"), None, Body::Json(&json!({ "refresh_token": refresh })), &[], false)?;
        if !r.ok() {
            let e = codes::of_auth(r.status, &r.body);
            // (a refresh token the auth does not take - used, revoked, its user gone - ends the
            // session; the auth being busy or down does not)
            if e == Error::SignedOut || matches!(r.status, 400 | 401 | 403) {
                self.forget();
                return Err(Error::SignedOut);
            }
            return Err(e);
        }
        if !self.keep(&r.body) {
            return Err(Error::Code("server"));
        }
        Ok(self.access.as_ref().map(|a| a.token.clone()).unwrap_or_default())
    }

    /// A request with the access token (to the database or Storage); refused as unauthorized, the
    /// token is renewed and it is tried once more, and refused again it signs out. Any other answer
    /// comes back as it is.
    pub(crate) fn authed(&mut self, method: &str, url: &str, body: Body, headers: &[(&str, &str)], file: bool) -> Result<Reply, Error> {
        for again in [false, true] {
            let t = self.access_token()?;
            // (nothing goes out before the privacy statement is agreed to)
            if !self.free && !self.agreed() {
                return Err(Error::Code("privacy"));
            }
            let r = self.send(method, url, Some(&t), body, headers, file)?;
            if !r.ok() && codes::is_unauthorized(r.status, &r.body) {
                self.access = None;
                if again {
                    self.forget();
                    return Err(Error::SignedOut);
                }
                continue;
            }
            return Ok(r);
        }
        Err(Error::SignedOut)
    }

    /// A request to the database (`/rest/v1<path>`).
    fn data(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, Error> {
        let r = self.authed(method, &self.ep.rest(path), body.map_or(Body::None, Body::Json), &[], false)?;
        if r.ok() {
            Ok(r.body)
        } else {
            Err(codes::of_data(r.status, &r.body))
        }
    }

    /// A function of the database (`POST /rest/v1/rpc/<name>`).
    pub fn rpc(&mut self, name: &str, args: Value) -> Result<Value, Error> {
        self.data("POST", &format!("/rpc/{name}"), Some(&args))
    }

    /// A function whose answer may be one of our codes: a success among `ok`, another of ours
    /// an error; nothing, or a word that is none of ours (`void`, a row, `true`), is `ok[0]`.
    fn rpc_code(&mut self, name: &str, args: Value, ok: &[&'static str]) -> Result<&'static str, Error> {
        let v = self.rpc(name, args)?;
        match codes::word_of(&v).as_deref().and_then(codes::known) {
            Some(c) if ok.contains(&c) => Ok(c),
            Some(c) => Err(Error::Code(c)),
            None => Ok(ok[0]),
        }
    }

    // --- the profile and the friends -------------------------------------------------------------

    /// The player's profile (made with a fresh friend code; nothing changes when there is one).
    pub fn create_profile(&mut self, name: &str) -> Result<(), Error> {
        self.rpc("create_profile", json!({ "p_name": check::display_name(name)? })).map(|_| ())
    }

    pub fn set_display_name(&mut self, name: &str) -> Result<(), Error> {
        self.rpc("set_display_name", json!({ "p_name": check::display_name(name)? })).map(|_| ())
    }

    /// The player's own profile, or None before `create_profile`.
    pub fn profile(&mut self) -> Result<Option<Profile>, Error> {
        self.access_token()?;
        let me = self.user_id().ok_or(Error::Code("server"))?.to_string();
        let v = self.data("GET", &format!("/profiles?select=user_id,display_name,friend_code,on_board&user_id=eq.{}&limit=1", encode(&me)), None)?;
        Ok(v.as_array().and_then(|a| a.first()).map(|row| Profile {
            user_id: me,
            display_name: text(row, &["display_name"]).unwrap_or_default(),
            friend_code: text(row, &["friend_code"]).unwrap_or_default(),
            on_board: field(row, &["on_board"]).and_then(Value::as_bool).unwrap_or(false),
        }))
    }

    /// The online state and the company session hosted now (None: none).
    pub fn heartbeat(&mut self, status: &str, session: Option<&str>) -> Result<(), Error> {
        let session = session.map(check::session_code).transpose()?;
        self.rpc("heartbeat", json!({ "p_status": check::status(status)?, "p_session": session })).map(|_| ())
    }

    /// "Offline", as the launcher closes: a few seconds at most, whatever comes of it.
    pub fn goodbye(&mut self) {
        if self.signed_in() {
            self.quick = true;
            let _ = self.heartbeat("offline", None);
            self.quick = false;
        }
    }

    /// A friend request by friend code: `sent`, or `accepted` (they had asked); every other
    /// answer (`already`, `self`, `not_found`, `no_profile`, `too_many`, `full`) is an error.
    pub fn send_friend_request(&mut self, code: &str) -> Result<&'static str, Error> {
        self.rpc_code("send_friend_request", json!({ "p_code": check::friend_code(code)? }), codes::ADDED)
    }

    pub fn respond_friend_request(&mut self, from: &str, accept: bool) -> Result<(), Error> {
        self.rpc_code("respond_friend_request", json!({ "p_from": check::id(from)?, "p_accept": accept }), &["ok", "accepted", "sent"]).map(|_| ())
    }

    pub fn remove_friend(&mut self, other: &str) -> Result<(), Error> {
        self.rpc_code("remove_friend", json!({ "p_other": check::id(other)? }), &["ok"]).map(|_| ())
    }

    pub fn block_player(&mut self, other: &str) -> Result<(), Error> {
        self.rpc_code("block_player", json!({ "p_other": check::id(other)? }), &["ok"]).map(|_| ())
    }

    /// An invite to the company session hosted now (`not_friends` is `not_allowed`).
    pub fn send_invite(&mut self, to: &str, session: &str) -> Result<(), Error> {
        let v = self.rpc("send_invite", json!({ "p_to": check::id(to)?, "p_session": check::session_code(session)? }))?;
        match codes::word_of(&v).as_deref() {
            Some("not_friends") => Err(Error::Code("not_allowed")),
            Some(w) => match codes::known(w) {
                Some(c) if c != "sent" && c != "ok" => Err(Error::Code(c)),
                _ => Ok(()),
            },
            None => Ok(()),
        }
    }

    pub fn dismiss_invite(&mut self, id: &Value) -> Result<(), Error> {
        self.rpc_code("dismiss_invite", json!({ "p_id": id }), &["ok"]).map(|_| ())
    }

    /// Friends and requests (`my_friends()`).
    pub fn friends(&mut self) -> Result<Vec<Friend>, Error> {
        let v = self.rpc("my_friends", json!({}))?;
        Ok(v.as_array().map(|rows| rows.iter().filter_map(friend_of).collect()).unwrap_or_default())
    }

    /// The invites the player was sent, newest first (the table: its rows are the player's own).
    pub fn invites(&mut self) -> Result<Vec<Invite>, Error> {
        self.access_token()?;
        let me = self.user_id().unwrap_or_default().to_string();
        let v = self.data("GET", "/invites?select=*&limit=50", None)?;
        let mut all: Vec<Invite> = v.as_array().map(|rows| rows.iter().filter_map(invite_of).collect()).unwrap_or_default();
        all.retain(|i| if i.to.is_empty() { i.from != me } else { i.to == me });
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all)
    }

    // --- the leaderboard --------------------------------------------------------------------------

    /// Standing on the public leaderboard, or not (`set_leaderboard`); `no_profile` before the
    /// display name is chosen.
    pub fn set_leaderboard(&mut self, on: bool) -> Result<(), Error> {
        match self.rpc("set_leaderboard", json!({ "p_on": on }))? {
            Value::Bool(false) => Err(Error::Code("no_profile")),
            _ => Ok(()),
        }
    }

    /// The service record for the leaderboard (`report_stats`): `ok`, or `too_soon` (one report
    /// in 30 seconds is kept); `no_profile` and a record out of range (`bad_input`) are errors.
    pub fn report_stats(&mut self, s: &Stats) -> Result<&'static str, Error> {
        let v = self.rpc("report_stats", json!({ "p_points": s.points, "p_level": s.level.max(1), "p_duties": s.duties, "p_minutes": s.minutes_driven, "p_km": s.km }))?;
        match codes::word_of(&v).as_deref() {
            Some("ok") => Ok("ok"),
            Some("too_soon") => Ok("too_soon"),
            Some("no_profile") => Err(Error::Code("no_profile")),
            Some("bad") => Err(Error::Code("bad_input")),
            _ => Err(Error::Code("server")),
        }
    }
}

fn friend_of(row: &Value) -> Option<Friend> {
    let user_id = text(row, &["user_id", "friend_id", "other_id", "other", "id"])?;
    Some(Friend {
        user_id,
        display_name: text(row, &["display_name", "name"]).unwrap_or_default(),
        status: text(row, &["status"]).unwrap_or_else(|| "offline".into()),
        online: field(row, &["online"]).and_then(Value::as_bool).unwrap_or(false),
        session_code: text(row, &["session_code", "session"]).filter(|s| !s.is_empty()),
        relation: text(row, &["relation", "friendship", "state"]).unwrap_or_else(|| "accepted".into()),
        incoming: field(row, &["incoming"]).and_then(Value::as_bool).unwrap_or(false),
        last_seen: text(row, &["last_seen"]),
    })
}

fn invite_of(row: &Value) -> Option<Invite> {
    Some(Invite {
        id: field(row, &["id", "invite_id"])?.clone(),
        from: text(row, &["from_user", "sender", "inviter", "from_id", "from"]).unwrap_or_default(),
        to: text(row, &["to_user", "recipient", "invitee", "to_id", "to"]).unwrap_or_default(),
        session_code: text(row, &["session_code", "session"])?,
        created_at: text(row, &["created_at", "sent_at"]),
    })
}

/// An access token's expiry, subject and email (read, not checked: the server checks it).
fn claims(jwt: &str) -> Option<(u64, String, String)> {
    let payload = jwt.split('.').nth(1)?;
    let v: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some((v.get("exp")?.as_u64()?, s("sub"), s("email")))
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|c| *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// An access token as Supabase's look (the tests'; the signature is not read here).
#[cfg(test)]
pub(crate) fn test_jwt(sub: &str, exp: u64) -> String {
    fn b64(b: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut s = String::new();
        for ch in b.chunks(3) {
            let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
            for k in 0..=ch.len() {
                s.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            }
        }
        s
    }
    let claims = json!({ "sub": sub, "exp": exp, "email": "a@b.example", "role": "authenticated", "aud": "authenticated" });
    format!("{}.{}.sig", b64(br#"{"alg":"ES256","typ":"JWT"}"#), b64(claims.to_string().as_bytes()))
}

#[cfg(test)]
pub(crate) fn test_now() -> u64 {
    now()
}

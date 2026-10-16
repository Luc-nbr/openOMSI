//! The client: Neon Auth's routes (Better Auth 1.7, with its bearer, JWT and email-OTP plugins:
//! docs/omsihub/ONLINE.md says which) and the Data API's functions.
//!
//! The session token (Better Auth's, from `set-auth-token`) is the one thing kept, in the OS's
//! store; it goes as `Authorization: Bearer` to the auth only. The Data API gets the JWT
//! (`GET /token`), kept in memory, fetched again a minute before it expires and once more on a
//! 401; a second 401, or the auth refusing the session, signs out (the token taken from the
//! store). No cookie is sent; the auth's calls carry its own origin as `Origin`, which Better
//! Auth trusts (a sign-up without one is refused: `MISSING_ORIGIN`).

use crate::codes::{self, Error};
use crate::http::{encode, Http, Reply};
use crate::store::SecretStore;
use crate::{check, Endpoints};
use serde_json::{json, Value};

/// After signing up: signed in already, or the email to confirm by its code first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignUp {
    SignedIn,
    Verify,
}

/// The player's own profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub user_id: String,
    pub display_name: String,
    pub friend_code: String,
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

pub(crate) struct Jwt {
    token: String,
    exp: u64,
    sub: String,
}

pub struct Client {
    pub(crate) ep: Endpoints,
    pub(crate) http: Http,
    store: Box<dyn SecretStore>,
    session: Option<String>,
    pub(crate) jwt: Option<Jwt>,
    /// The goodbye's short timeouts.
    quick: bool,
}

/// The JWT is fetched again this long before it expires.
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
        let session = store.load();
        Self { http: Http::new(!ep.plain), ep, store, session, jwt: None, quick: false }
    }

    pub fn signed_in(&self) -> bool {
        self.session.is_some()
    }

    /// Whether the session outlives the program (kept in the OS's store).
    pub fn lasting(&self) -> bool {
        self.store.lasting()
    }

    /// The player's user id (the JWT's subject), once a JWT was fetched.
    pub fn user_id(&self) -> Option<&str> {
        self.jwt.as_ref().map(|j| j.sub.as_str()).filter(|s| !s.is_empty())
    }

    pub(crate) fn send(&self, method: &str, url: &str, bearer: Option<&str>, body: Option<&Value>) -> Result<Reply, Error> {
        // (Better Auth refuses a sign-up without an Origin it trusts - "MISSING_ORIGIN": its own
        // origin is one, and the auth's calls carry it; the Data API and the saves need none)
        let origin = url.starts_with(&self.ep.auth).then(|| origin_of(&self.ep.auth));
        self.http.send(method, url, bearer, body, origin.as_deref(), self.quick)
    }

    /// A POST to the auth, its error as ours.
    fn auth_post(&self, path: &str, body: Value, bearer: Option<&str>) -> Result<Reply, Error> {
        let r = self.send("POST", &format!("{}{path}", self.ep.auth), bearer, Some(&body))?;
        if r.ok() {
            Ok(r)
        } else {
            Err(codes::of_auth(r.status, &r.body))
        }
    }

    /// The session an answer brought (the bearer plugin's header, else the cookie, else the
    /// body's `token`), kept.
    fn keep(&mut self, r: &Reply) -> bool {
        let token = r.auth_token.clone().or_else(|| r.cookie_token.clone()).or_else(|| r.body.get("token").and_then(Value::as_str).map(str::to_string)).filter(|t| !t.is_empty());
        let Some(t) = token else { return false };
        self.store.save(&t);
        self.session = Some(t);
        self.jwt = None;
        true
    }

    /// Signed out here: the token out of the store, the JWT forgotten.
    pub fn forget(&mut self) {
        self.session = None;
        self.jwt = None;
        self.store.clear();
    }

    // --- the account ----------------------------------------------------------------------------

    /// A new account (`POST /sign-up/email`). With the email to be confirmed first (the project's
    /// setting), Neon Auth sends the code and the session comes with `verify_email`.
    pub fn sign_up(&mut self, email: &str, password: &str, name: &str) -> Result<SignUp, Error> {
        let (email, password, name) = (check::email(email)?, check::password(password)?, check::display_name(name)?);
        let r = self.auth_post("/sign-up/email", json!({ "email": email, "password": password, "name": name }), None)?;
        Ok(if self.keep(&r) { SignUp::SignedIn } else { SignUp::Verify })
    }

    /// Signing in (`POST /sign-in/email`).
    pub fn sign_in(&mut self, email: &str, password: &str) -> Result<(), Error> {
        let (email, password) = (check::email(email)?, check::password(password).map_err(|_| Error::Code("invalid_credentials"))?);
        let r = self.auth_post("/sign-in/email", json!({ "email": email, "password": password, "rememberMe": true }), None)?;
        if self.keep(&r) {
            Ok(())
        } else {
            Err(Error::Code("server"))
        }
    }

    /// A new code to confirm the email (`POST /email-otp/send-verification-otp`).
    pub fn send_verification(&self, email: &str) -> Result<(), Error> {
        self.auth_post("/email-otp/send-verification-otp", json!({ "email": check::email(email)?, "type": "email-verification" }), None).map(|_| ())
    }

    /// The email confirmed by its code (`POST /email-otp/verify-email`); whether that signed in
    /// too (the project's "sign in after verification").
    pub fn verify_email(&mut self, email: &str, code: &str) -> Result<bool, Error> {
        let r = self.auth_post("/email-otp/verify-email", json!({ "email": check::email(email)?, "otp": check::email_code(code)? }), None)?;
        Ok(self.keep(&r))
    }

    /// A code to reset the password, by email (`POST /email-otp/request-password-reset`; an
    /// older Better Auth's `/forget-password/email-otp` where that one is not there). The answer
    /// is the same whether the address has an account or not.
    pub fn request_reset(&self, email: &str) -> Result<(), Error> {
        let body = json!({ "email": check::email(email)? });
        match self.auth_post("/email-otp/request-password-reset", body.clone(), None) {
            Err(Error::Code("unavailable")) => self.auth_post("/forget-password/email-otp", body, None).map(|_| ()),
            r => r.map(|_| ()),
        }
    }

    /// The new password with the code (`POST /email-otp/reset-password`). The player signs in
    /// with it afterwards.
    pub fn reset_password(&self, email: &str, code: &str, password: &str) -> Result<(), Error> {
        self.auth_post("/email-otp/reset-password", json!({ "email": check::email(email)?, "otp": check::email_code(code)?, "password": check::password(password)? }), None).map(|_| ())
    }

    /// Signing out (`POST /sign-out`, best effort: the token goes here whatever the server says).
    pub fn sign_out(&mut self) {
        if let Some(s) = self.session.clone() {
            let _ = self.auth_post("/sign-out", json!({}), Some(&s));
        }
        self.forget();
    }

    /// The account deleted, after the password typed again: signed in afresh with it (which
    /// checks it, and makes the session fresh enough to delete), `delete_my_data()`, the cloud
    /// saves (`delete_all`: nothing of the player's is left in the storage), then the auth's
    /// user (`POST /delete-user`), then signed out here. A step that fails stops it before the
    /// user is deleted: tried again, the steps done already do nothing.
    pub fn delete_account(&mut self, password: &str) -> Result<(), Error> {
        let session = self.session.clone().ok_or(Error::Code("not_signed_in"))?;
        let r = self.send("GET", &format!("{}/get-session", self.ep.auth), Some(&session), None)?;
        if r.status == 401 || (r.ok() && r.body.is_null()) {
            self.forget();
            return Err(Error::SignedOut);
        }
        if !r.ok() {
            return Err(codes::of_auth(r.status, &r.body));
        }
        // (the address only for this, in memory)
        let email = r.body.pointer("/user/email").and_then(Value::as_str).ok_or(Error::Code("server"))?.to_string();
        self.sign_in(&email, password).map_err(|e| if e == Error::Code("invalid_credentials") { Error::Code("wrong_password") } else { e })?;
        self.rpc("delete_my_data", json!({}))?;
        self.saves_delete_all()?;
        let fresh = self.session.clone().ok_or(Error::Code("server"))?;
        self.auth_post("/delete-user", json!({ "password": password }), Some(&fresh))?;
        self.forget();
        Ok(())
    }

    // --- the JWT and the Data API ---------------------------------------------------------------

    /// A JWT good for a minute at least (`GET /token` with the session).
    pub fn jwt(&mut self) -> Result<String, Error> {
        if let Some(j) = &self.jwt {
            if j.exp > now() + MARGIN {
                return Ok(j.token.clone());
            }
        }
        let session = self.session.clone().ok_or(Error::Code("not_signed_in"))?;
        let r = self.send("GET", &format!("{}/token", self.ep.auth), Some(&session), None)?;
        if r.status == 401 || r.status == 403 {
            self.forget();
            return Err(Error::SignedOut);
        }
        if !r.ok() {
            return Err(codes::of_auth(r.status, &r.body));
        }
        // (a session the auth renewed comes back with it)
        if r.auth_token.as_ref().is_some_and(|t| Some(t) != self.session.as_ref()) {
            self.keep(&r);
        }
        let token = r.body.get("token").and_then(Value::as_str).map(str::to_string).or(r.auth_jwt).ok_or(Error::Code("server"))?;
        let (exp, sub) = claims(&token).ok_or(Error::Code("server"))?;
        self.jwt = Some(Jwt { token: token.clone(), exp, sub });
        Ok(token)
    }

    /// A Data API request with the JWT; a 401 fetches a new one and tries once more.
    fn data(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, Error> {
        let url = format!("{}{path}", self.ep.data);
        for again in [false, true] {
            let jwt = self.jwt()?;
            let r = self.send(method, &url, Some(&jwt), body)?;
            if r.status == 401 {
                self.jwt = None;
                if again {
                    self.forget();
                    return Err(Error::SignedOut);
                }
                continue;
            }
            return if r.ok() { Ok(r.body) } else { Err(codes::of_data(r.status, &r.body)) };
        }
        Err(Error::SignedOut)
    }

    /// A function of the Data API (`POST /rpc/<name>`).
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
        self.jwt()?;
        let me = self.user_id().ok_or(Error::Code("server"))?.to_string();
        let v = self.data("GET", &format!("/profiles?select=user_id,display_name,friend_code&user_id=eq.{}&limit=1", encode(&me)), None)?;
        Ok(v.as_array().and_then(|a| a.first()).map(|row| Profile { user_id: me, display_name: text(row, &["display_name"]).unwrap_or_default(), friend_code: text(row, &["friend_code"]).unwrap_or_default() }))
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

    /// An invite to the company session hosted now.
    pub fn send_invite(&mut self, to: &str, session: &str) -> Result<(), Error> {
        self.rpc_code("send_invite", json!({ "p_to": check::id(to)?, "p_session": check::session_code(session)? }), &["ok", "sent"]).map(|_| ())
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
        self.jwt()?;
        let me = self.user_id().unwrap_or_default().to_string();
        let v = self.data("GET", "/invites?select=*&limit=50", None)?;
        let mut all: Vec<Invite> = v.as_array().map(|rows| rows.iter().filter_map(invite_of).collect()).unwrap_or_default();
        all.retain(|i| if i.to.is_empty() { i.from != me } else { i.to == me });
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all)
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

/// A JWT's expiry and subject (read, not checked: the Data API checks it against the JWKS).
fn claims(jwt: &str) -> Option<(u64, String)> {
    let payload = jwt.split('.').nth(1)?;
    let v: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    Some((v.get("exp")?.as_u64()?, v.get("sub").and_then(Value::as_str).unwrap_or("").to_string()))
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
    format!("{}.{}.sig", b64(br#"{"alg":"EdDSA"}"#), b64(json!({ "sub": sub, "exp": exp }).to_string().as_bytes()))
}

#[cfg(test)]
pub(crate) fn test_now() -> u64 {
    now()
}

/// The origin of a URL: its scheme and host (`https://a.example/b/c` → `https://a.example`).
fn origin_of(url: &str) -> String {
    let rest = url.find("://").map_or(0, |i| i + 3);
    url[..url[rest..].find('/').map_or(url.len(), |j| rest + j)].to_string()
}

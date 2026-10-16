//! The client against the tests' server (`mock.rs`): signing in, the JWT fetched again, a 401
//! signing out, the functions' codes, no network. No test reaches the real project.

use crate::client::{test_jwt, test_now};
use crate::mock::{Answer, Mock, Req};
use crate::store::{MemoryStore, SecretStore};
use crate::{check, codes, Client, Endpoints, Error, SignUp};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A store the test still sees into after the client took it.
#[derive(Clone, Default)]
struct Shared(Arc<MemoryStore>);

impl SecretStore for Shared {
    fn load(&self) -> Option<String> {
        self.0.load()
    }
    fn save(&self, s: &str) -> bool {
        self.0.save(s)
    }
    fn clear(&self) {
        self.0.clear()
    }
    fn lasting(&self) -> bool {
        false
    }
}

fn client(m: &Mock, store: &Shared) -> Client {
    Client::new(Endpoints::local(&m.base), Box::new(store.clone()))
}

fn signed_in(token: &str) -> Shared {
    let s = Shared::default();
    s.save(token);
    s
}

fn ok(v: Value) -> Answer {
    (200, vec![], v)
}

/// The auth's `/token`: a JWT for `me` good for an hour, unless the session is not `tok`.
fn token_route(r: &Req) -> Option<Answer> {
    (r.path == "/auth/token").then(|| if r.bearer() == Some("tok.sig") { ok(json!({ "token": test_jwt("me", test_now() + 3600) })) } else { (401, vec![], json!({ "code": "UNAUTHORIZED" })) })
}

#[test]
fn signing_in_keeps_the_signed_token_in_the_store_and_sends_no_cookie_or_origin() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/sign-in/email" if r.body["password"] == "correct horse" => (200, vec![("set-auth-token", "tok.sig".into())], json!({ "token": "tok", "user": { "id": "me" } })),
        "/auth/sign-in/email" => (401, vec![], json!({ "code": "INVALID_EMAIL_OR_PASSWORD", "message": "Invalid email or password" })),
        _ => (404, vec![], Value::Null),
    });
    let store = Shared::default();
    let mut c = client(&m, &store);
    assert!(!c.signed_in());
    assert_eq!(c.sign_in("a@b.example", "wrong password"), Err(Error::Code("invalid_credentials")));
    assert!(store.load().is_none());
    c.sign_in(" a@b.example ", "correct horse").unwrap();
    assert!(c.signed_in());
    assert_eq!(store.load().as_deref(), Some("tok.sig"), "the bearer plugin's signed token is kept");
    let reqs = m.requests();
    assert_eq!(reqs[1].body["email"], "a@b.example");
    assert_eq!(reqs[1].body["rememberMe"], true);
    for r in &reqs {
        assert!(r.header("cookie").is_none() && r.header("origin").is_none(), "{r:?}");
    }
    // (a password too short to be one never leaves the computer)
    assert_eq!(c.sign_in("a@b.example", "short"), Err(Error::Code("invalid_credentials")));
    assert_eq!(m.calls().len(), 2);
    // (the next start finds the session in the store)
    assert!(client(&m, &store).signed_in());
}

#[test]
fn the_body_token_is_kept_where_the_auth_sends_no_header() {
    let m = Mock::start(|_| ok(json!({ "token": "plain", "user": {} })));
    let store = Shared::default();
    let mut c = client(&m, &store);
    c.sign_in("a@b.example", "12345678").unwrap();
    assert_eq!(store.load().as_deref(), Some("plain"));
}

#[test]
fn signing_up_waits_for_the_code_and_the_code_signs_in() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/sign-up/email" => ok(json!({ "token": null, "user": { "id": "me", "emailVerified": false } })),
        "/auth/email-otp/verify-email" if r.body["otp"] == "123456" => (200, vec![("set-auth-token", "tok.sig".into())], json!({ "status": true, "token": "tok" })),
        "/auth/email-otp/verify-email" => (400, vec![], json!({ "code": "INVALID_OTP", "message": "Invalid OTP" })),
        "/auth/email-otp/send-verification-otp" => ok(json!({ "success": true })),
        _ => (404, vec![], Value::Null),
    });
    let store = Shared::default();
    let mut c = client(&m, &store);
    assert_eq!(c.sign_up("a@b.example", "a long password", "x"), Err(Error::Code("bad_name")));
    assert_eq!(c.sign_up("a@b.example", "a long password", "Luc"), Ok(SignUp::Verify));
    assert!(!c.signed_in());
    assert_eq!(m.requests()[0].body, json!({ "email": "a@b.example", "password": "a long password", "name": "Luc" }));
    c.send_verification("a@b.example").unwrap();
    assert_eq!(m.requests()[1].body["type"], "email-verification");
    assert_eq!(c.verify_email("a@b.example", "000000"), Err(Error::Code("invalid_code")));
    assert_eq!(c.verify_email("a@b.example", "123 456"), Ok(true));
    assert_eq!(store.load().as_deref(), Some("tok.sig"));
}

#[test]
fn the_password_is_reset_by_a_code_with_the_older_route_as_fallback() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/email-otp/request-password-reset" => (404, vec![], Value::Null),
        "/auth/forget-password/email-otp" => ok(json!({ "success": true })),
        "/auth/email-otp/reset-password" if r.body["otp"] == "654321" => ok(json!({ "success": true })),
        "/auth/email-otp/reset-password" => (400, vec![], json!({ "code": "OTP_EXPIRED" })),
        _ => (404, vec![], Value::Null),
    });
    let c = client(&m, &Shared::default());
    c.request_reset("a@b.example").unwrap();
    assert_eq!(m.calls(), ["POST /auth/email-otp/request-password-reset", "POST /auth/forget-password/email-otp"]);
    assert_eq!(c.reset_password("a@b.example", "111111", "new password!"), Err(Error::Code("code_expired")));
    c.reset_password("a@b.example", "654321", "new password!").unwrap();
    assert_eq!(m.requests().last().unwrap().body, json!({ "email": "a@b.example", "otp": "654321", "password": "new password!" }));
    assert_eq!(c.reset_password("a@b.example", "654321", "short"), Err(Error::Code("password_too_short")));
}

#[test]
fn the_jwt_is_fetched_again_before_it_expires() {
    let tokens = Arc::new(AtomicUsize::new(0));
    let t = tokens.clone();
    let m = Mock::start(move |r| match r.path.as_str() {
        "/auth/token" => {
            // (the first one expires in 30 seconds: inside the minute's margin)
            let n = t.fetch_add(1, Ordering::SeqCst);
            ok(json!({ "token": test_jwt("me", test_now() + if n == 0 { 30 } else { 3600 }) }))
        }
        "/rest/v1/rpc/heartbeat" => ok(Value::Null),
        _ => (404, vec![], Value::Null),
    });
    let mut c = client(&m, &signed_in("tok.sig"));
    c.heartbeat("online", None).unwrap();
    c.heartbeat("online", None).unwrap();
    c.heartbeat("away", Some("CODE-1")).unwrap();
    assert_eq!(tokens.load(Ordering::SeqCst), 2, "fetched again once, then kept");
    let reqs = m.requests();
    let beats: Vec<&Req> = reqs.iter().filter(|r| r.path.ends_with("heartbeat")).collect();
    assert_eq!(beats.len(), 3);
    assert_ne!(beats[0].bearer(), beats[1].bearer(), "the second beat has the new JWT");
    assert_eq!(beats[1].bearer(), beats[2].bearer());
    assert_eq!(beats[2].body, json!({ "p_status": "away", "p_session": "CODE-1" }));
    assert_eq!(beats[0].body["p_session"], Value::Null);
    // (the auth got the session token, the Data API the JWT only)
    assert!(reqs.iter().filter(|r| r.path.starts_with("/auth/")).all(|r| r.bearer() == Some("tok.sig")));
    assert!(beats.iter().all(|r| r.bearer() != Some("tok.sig")));
    assert_eq!(c.user_id(), Some("me"));
}

#[test]
fn a_401_fetches_a_new_jwt_once_and_a_second_signs_out() {
    let refused = Arc::new(AtomicUsize::new(0));
    let n = refused.clone();
    let m = Mock::start(move |r| {
        if let Some(a) = token_route(r) {
            return a;
        }
        match r.path.as_str() {
            // (refused once: the retry with a new JWT goes through)
            "/rest/v1/rpc/my_friends" if n.fetch_add(1, Ordering::SeqCst) == 0 => (401, vec![], json!({ "code": "PGRST301", "message": "JWT expired" })),
            "/rest/v1/rpc/my_friends" => ok(json!([{ "user_id": "u2", "display_name": "Ana", "status": "busy", "online": true, "session_code": "S1", "relation": "accepted", "incoming": false, "last_seen": "2026-10-16T10:00:00+00:00" }])),
            "/rest/v1/rpc/heartbeat" => (401, vec![], json!({ "code": "PGRST301", "message": "JWT invalid" })),
            _ => (404, vec![], Value::Null),
        }
    });
    let store = signed_in("tok.sig");
    let mut c = client(&m, &store);
    let f = c.friends().unwrap();
    assert_eq!((f[0].user_id.as_str(), f[0].display_name.as_str(), f[0].online, f[0].session_code.as_deref()), ("u2", "Ana", true, Some("S1")));
    assert_eq!(m.calls().iter().filter(|c| c.ends_with("/auth/token")).count(), 2);
    assert!(c.signed_in());
    // (refused with a fresh JWT too: signed out, the token gone from the store)
    assert_eq!(c.heartbeat("online", None), Err(Error::SignedOut));
    assert!(!c.signed_in());
    assert!(store.load().is_none());
    assert_eq!(c.friends(), Err(Error::Code("not_signed_in")));
}

#[test]
fn a_session_the_auth_refuses_signs_out() {
    let m = Mock::start(|r| token_route(r).unwrap_or((404, vec![], Value::Null)));
    let store = signed_in("revoked");
    let mut c = client(&m, &store);
    assert_eq!(c.friends(), Err(Error::SignedOut));
    assert!(store.load().is_none() && !c.signed_in());
}

#[test]
fn the_functions_codes_are_ours_and_the_servers_words_are_not_shown() {
    let m = Mock::start(|r| {
        if let Some(a) = token_route(r) {
            return a;
        }
        let code = r.body["p_code"].as_str().unwrap_or("");
        match (r.path.as_str(), code) {
            ("/rest/v1/rpc/send_friend_request", "SENT0001") => ok(json!("sent")),
            ("/rest/v1/rpc/send_friend_request", "ACCEPTED") => ok(json!([{ "result": "accepted" }])),
            ("/rest/v1/rpc/send_friend_request", "TOOMANY1") => ok(json!("too_many")),
            ("/rest/v1/rpc/send_friend_request", "FULLLIST") => ok(json!({ "send_friend_request": "full" })),
            ("/rest/v1/rpc/send_friend_request", "RAISED01") => (400, vec![], json!({ "code": "P0001", "message": "not_found", "details": null })),
            ("/rest/v1/rpc/send_friend_request", _) => (500, vec![], json!({ "code": "XX000", "message": "internal: relation \"neon_auth.users\" ..." })),
            ("/rest/v1/rpc/set_display_name", _) => (409, vec![], json!({ "code": "23505", "message": "duplicate key value violates unique constraint \"profiles_name_key\"" })),
            ("/rest/v1/rpc/remove_friend", _) => ok(Value::Null),
            ("/rest/v1/rpc/block_player", _) => ok(json!("not_found")),
            ("/rest/v1/rpc/send_invite", _) => (429, vec![], json!({ "message": "slow down" })),
            _ => (404, vec![], json!({ "code": "PGRST202", "message": "Could not find the function" })),
        }
    });
    let mut c = client(&m, &signed_in("tok.sig"));
    assert_eq!(c.send_friend_request("SENT-0001"), Ok("sent"));
    assert_eq!(c.send_friend_request("ACCEPTED"), Ok("accepted"));
    assert_eq!(c.send_friend_request("TOOMANY1"), Err(Error::Code("too_many")));
    assert_eq!(c.send_friend_request("FULLLIST"), Err(Error::Code("full")));
    assert_eq!(c.send_friend_request("RAISED01"), Err(Error::Code("not_found")));
    let e = c.send_friend_request("ELSE0001").unwrap_err();
    assert_eq!(e, Error::Code("server"));
    assert!(!e.text().contains("neon_auth"));
    assert_eq!(c.send_friend_request("short"), Err(Error::Code("bad_friend_code")));
    assert_eq!(c.set_display_name("Taken Name"), Err(Error::Code("name_taken")));
    assert_eq!(c.remove_friend("u2"), Ok(()));
    assert_eq!(c.block_player("u2"), Err(Error::Code("not_found")));
    assert_eq!(c.send_invite("u2", "CODE"), Err(Error::Code("too_many")));
    assert_eq!(c.dismiss_invite(&json!(7)), Err(Error::Code("unavailable")));
    assert_eq!(c.remove_friend("u2; drop"), Err(Error::Code("bad_input")));
}

#[test]
fn every_code_has_a_text_and_every_function_answer_is_a_code() {
    for (code, text) in codes::TEXTS {
        assert!(!text.is_empty() && codes::known(code) == Some(*code), "{code}");
    }
    for c in ["sent", "accepted", "already", "self", "not_found", "no_profile", "too_many", "full"] {
        assert!(codes::known(c).is_some(), "{c}");
    }
    assert_eq!(codes::text("nonsense"), codes::text("server"));
    assert_eq!(Error::Offline.code(), "offline");
}

#[test]
fn without_a_network_every_call_is_offline_and_the_session_stays() {
    // (a port nothing listens on)
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let store = signed_in("tok.sig");
    let mut c = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}")), Box::new(store.clone()));
    let t = std::time::Instant::now();
    assert_eq!(c.friends(), Err(Error::Offline));
    assert_eq!(c.heartbeat("online", None), Err(Error::Offline));
    assert_eq!(c.sign_in("a@b.example", "12345678"), Err(Error::Offline));
    assert!(t.elapsed().as_secs() < 5);
    assert!(c.signed_in() && store.load().is_some(), "offline is no reason to sign out");
    c.goodbye();
}

#[test]
fn plain_http_is_refused_outside_the_tests() {
    assert!(Endpoints::new("http://example.com/auth", "https://example.com/rest").is_err());
    assert!(Endpoints::new("https://example.com/auth/", "https://example.com/rest").is_ok());
    let e = Endpoints::neon();
    assert!(e.auth.starts_with("https://") && e.data.starts_with("https://") && !e.plain);
}

#[test]
fn deleting_the_account_checks_the_password_then_deletes_the_data_then_the_user() {
    let m = Mock::start(|r| {
        if r.path == "/auth/token" {
            return ok(json!({ "token": test_jwt("me", test_now() + 3600) }));
        }
        match r.path.as_str() {
            "/auth/get-session" => ok(json!({ "session": {}, "user": { "id": "me", "email": "a@b.example" } })),
            "/auth/sign-in/email" if r.body["password"] == "my password" => (200, vec![("set-auth-token", "fresh.sig".into())], json!({ "token": "fresh" })),
            "/auth/sign-in/email" => (401, vec![], json!({ "code": "INVALID_EMAIL_OR_PASSWORD" })),
            "/rest/v1/rpc/delete_my_data" => ok(Value::Null),
            "/auth/delete-user" => ok(json!({ "success": true, "message": "User deleted" })),
            _ => (404, vec![], Value::Null),
        }
    });
    let store = signed_in("old.sig");
    let mut c = client(&m, &store);
    assert_eq!(c.delete_account("not it!!"), Err(Error::Code("wrong_password")));
    assert!(!m.calls().iter().any(|p| p.contains("delete")), "nothing deleted on a wrong password");
    c.delete_account("my password").unwrap();
    let calls = m.calls();
    let at = |p: &str| calls.iter().rposition(|c| c.ends_with(p)).unwrap();
    assert!(at("/rpc/delete_my_data") < at("/auth/delete-user"));
    let del = m.requests().into_iter().find(|r| r.path == "/auth/delete-user").unwrap();
    assert_eq!(del.bearer(), Some("fresh.sig"));
    assert_eq!(del.body["password"], "my password");
    assert!(!c.signed_in() && store.load().is_none());
}

#[test]
fn the_invites_are_the_ones_sent_to_the_player() {
    let m = Mock::start(|r| {
        if let Some(a) = token_route(r) {
            return a;
        }
        match r.path.as_str() {
            "/rest/v1/invites?select=*&limit=50" => ok(json!([
                { "id": 1, "from_user": "u2", "to_user": "me", "session_code": "OLD", "created_at": "2026-10-16T09:00:00Z" },
                { "id": 2, "from_user": "me", "to_user": "u3", "session_code": "MINE", "created_at": "2026-10-16T09:30:00Z" },
                { "id": 3, "from_user": "u3", "to_user": "me", "session_code": "NEW", "created_at": "2026-10-16T10:00:00Z" }
            ])),
            p if p.starts_with("/rest/v1/profiles?") => ok(json!([{ "user_id": "me", "display_name": "Luc", "friend_code": "AB12CD34" }])),
            _ => (404, vec![], Value::Null),
        }
    });
    let mut c = client(&m, &signed_in("tok.sig"));
    let inv = c.invites().unwrap();
    assert_eq!(inv.iter().map(|i| i.session_code.as_str()).collect::<Vec<_>>(), ["NEW", "OLD"]);
    assert_eq!(inv[0].key(), "3");
    let p = c.profile().unwrap().unwrap();
    assert_eq!((p.display_name.as_str(), p.friend_code.as_str()), ("Luc", "AB12CD34"));
    assert!(m.calls().iter().any(|c| c.ends_with("/rest/v1/profiles?select=user_id,display_name,friend_code&user_id=eq.me&limit=1")));
}

#[test]
fn what_is_typed_is_checked() {
    assert!(check::email("a@b.example").is_ok());
    for bad in ["", "a", "a@b", "@b.example", "a b@c.example", "a@b@c.example", "a@.example", &format!("{}@b.example", "x".repeat(250))] {
        assert_eq!(check::email(bad), Err(Error::Code("invalid_email")), "{bad}");
    }
    assert_eq!(check::password("1234567"), Err(Error::Code("password_too_short")));
    assert_eq!(check::password(&"x".repeat(129)), Err(Error::Code("password_too_long")));
    assert_eq!(check::display_name("  Luc "), Ok("Luc".into()));
    assert!(check::display_name("ab").is_err() && check::display_name(&"x".repeat(25)).is_err() && check::display_name("a\u{7}bc").is_err());
    assert_eq!(check::display_name("Łukasz Żółć"), Ok("Łukasz Żółć".into()));
    assert_eq!(check::friend_code("AB12 CD34"), Ok("AB12CD34".into()));
    assert!(check::friend_code("AB12CD3").is_err() && check::friend_code("AB12CD3!").is_err());
    assert_eq!(check::email_code(" 123 456 "), Ok("123456".into()));
    assert!(check::email_code("12a456").is_err());
    assert!(check::status("busy").is_ok() && check::status("hacking").is_err());
    assert!(check::strength("password") == 0 && check::strength("Password12") == 1 && check::strength("Corr3ct horse battery") == 2);
}

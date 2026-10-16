//! The client against the tests' server (`mock.rs`), shaped as Supabase answers: signing in, the
//! access token renewed with the refresh token, a 401 signing out, the functions' codes, the cloud
//! saves in Storage, no network. No test reaches the real project.

use crate::client::{test_jwt, test_now};
use crate::mock::{Answer, Mock, Req};
use crate::saves_tests::{raw_of, storage, Files};
use crate::store::{MemoryStore, SecretStore};
use crate::{check, codes, Client, Endpoints, Error, Stats};
use serde_json::{json, Value};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const KEY: &str = "sb_publishable_test";

/// A store the test still sees into after the client took it.
#[derive(Clone, Default)]
pub(crate) struct Shared(Arc<MemoryStore>);

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

pub(crate) fn client(m: &Mock, store: &Shared) -> Client {
    Client::new(Endpoints::local(&m.base, KEY).unwrap(), Box::new(store.clone()))
}

pub(crate) fn signed_in(refresh: &str) -> Shared {
    let s = Shared::default();
    s.save(refresh);
    s
}

pub(crate) fn ok(v: Value) -> Answer {
    (200, vec![], v)
}

/// A session as Supabase Auth answers one: an access token for `me` good for `secs`, the refresh
/// token, the user.
fn session(refresh: &str, secs: u64) -> Value {
    json!({ "access_token": test_jwt("me", test_now() + secs), "token_type": "bearer", "expires_in": secs, "refresh_token": refresh,
            "user": { "id": "me", "email": "a@b.example", "user_metadata": { "display_name": "Luc" } } })
}

/// Supabase Auth's error.
fn auth_error(status: u16, code: &str) -> Answer {
    (status, vec![], json!({ "code": status, "error_code": code, "msg": "words not to be shown" }))
}

/// The auth's refresh: a new session for the refresh token `r1` (and the ones it hands out after).
pub(crate) fn refresh_route(r: &Req) -> Option<Answer> {
    (r.path == "/auth/v1/token?grant_type=refresh_token").then(|| match r.body["refresh_token"].as_str() {
        Some(t) if t.starts_with('r') => ok(session(&format!("{t}+"), 3600)),
        _ => auth_error(400, "refresh_token_not_found"),
    })
}

#[test]
fn signing_in_keeps_the_refresh_token_and_every_request_carries_the_key_and_no_cookie() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/v1/token?grant_type=password" if r.body["password"] == "correct horse" => ok(session("r1", 3600)),
        "/auth/v1/token?grant_type=password" if r.body["password"] == "older server" => (400, vec![], json!({ "error": "invalid_grant", "error_description": "Invalid login credentials" })),
        "/auth/v1/token?grant_type=password" => auth_error(400, "invalid_credentials"),
        _ => (404, vec![], Value::Null),
    });
    let store = Shared::default();
    let mut c = client(&m, &store);
    assert!(!c.signed_in());
    assert_eq!(c.sign_in("a@b.example", "wrong password"), Err(Error::Code("invalid_credentials")));
    assert_eq!(c.sign_in("a@b.example", "older server"), Err(Error::Code("invalid_credentials")));
    assert!(store.load().is_none());
    c.sign_in(" a@b.example ", "correct horse").unwrap();
    assert!(c.signed_in());
    assert_eq!(store.load().as_deref(), Some("r1"), "only the refresh token is kept");
    assert_eq!(c.user_id(), Some("me"));
    assert_eq!(c.signed_up_name(), Some("Luc"));
    let reqs = m.requests();
    assert_eq!(reqs[2].body, json!({ "email": "a@b.example", "password": "correct horse" }));
    for r in &reqs {
        assert_eq!(r.header("apikey"), Some(KEY), "{r:?}");
        assert!(r.header("cookie").is_none() && r.header("origin").is_none() && r.bearer().is_none(), "{r:?}");
    }
    // (a password too short to be one never leaves the computer)
    assert_eq!(c.sign_in("a@b.example", "short"), Err(Error::Code("invalid_credentials")));
    assert_eq!(m.calls().len(), 3);
    // (the next start finds the session in the store)
    assert!(client(&m, &store).signed_in());
}

#[test]
fn an_unconfirmed_email_is_confirmed_by_its_code_which_signs_in() {
    let m = Mock::start(|r| match (r.path.as_str(), r.body["type"].as_str(), r.body["token"].as_str()) {
        ("/auth/v1/token?grant_type=password", ..) => auth_error(400, "email_not_confirmed"),
        ("/auth/v1/resend", ..) => ok(json!({})),
        ("/auth/v1/verify", Some("email"), Some("123456")) => ok(session("r1", 3600)),
        ("/auth/v1/verify", ..) => auth_error(403, "otp_expired"),
        _ => (404, vec![], Value::Null),
    });
    let store = Shared::default();
    let mut c = client(&m, &store);
    assert_eq!(c.sign_in("a@b.example", "a long password"), Err(Error::Code("email_not_verified")));
    c.send_verification("a@b.example").unwrap();
    assert_eq!(m.requests()[1].body, json!({ "type": "signup", "email": "a@b.example" }));
    // (a wrong code is tried as an older sign-up mail's too, then refused)
    assert_eq!(c.verify_email("a@b.example", "000000"), Err(Error::Code("invalid_code")));
    let tried: Vec<Value> = m.requests()[2..].iter().map(|r| r.body["type"].clone()).collect();
    assert_eq!(tried, [json!("email"), json!("signup")]);
    assert_eq!(c.verify_email("a@b.example", "12345"), Err(Error::Code("invalid_code")), "too short to be a code");
    assert_eq!(m.calls().len(), 4);
    assert_eq!(c.verify_email("a@b.example", "123 456"), Ok(true));
    assert_eq!(m.requests().last().unwrap().body, json!({ "type": "email", "email": "a@b.example", "token": "123456" }));
    assert_eq!(store.load().as_deref(), Some("r1"));
    assert_eq!(c.signed_up_name(), Some("Luc"), "the name chosen on the website, for the profile");
}

#[test]
fn the_password_is_reset_by_a_code_that_signs_in_then_put_on_the_user() {
    let m = Mock::start(|r| match (r.path.as_str(), r.body["token"].as_str()) {
        ("/auth/v1/recover", _) => ok(json!({})),
        ("/auth/v1/verify", Some("654321")) => ok(session("r1", 3600)),
        ("/auth/v1/verify", _) => auth_error(403, "otp_expired"),
        ("/auth/v1/user", _) if r.method == "PUT" && r.body["password"] == "my old password" => auth_error(422, "same_password"),
        ("/auth/v1/user", _) if r.method == "PUT" && r.body["password"] == "weakweak" => auth_error(422, "weak_password"),
        ("/auth/v1/user", _) if r.method == "PUT" => ok(json!({ "id": "me" })),
        _ => (404, vec![], Value::Null),
    });
    let store = Shared::default();
    let mut c = client(&m, &store);
    c.request_reset("a@b.example").unwrap();
    assert_eq!(m.requests()[0].body, json!({ "email": "a@b.example" }));
    assert_eq!(c.reset_password("a@b.example", "111111", "new password!"), Err(Error::Code("invalid_code")));
    assert!(!c.signed_in());
    c.reset_password("a@b.example", "654321", "new password!").unwrap();
    let reqs = m.requests();
    let verify = reqs.iter().rfind(|r| r.path == "/auth/v1/verify").unwrap();
    assert_eq!(verify.body, json!({ "type": "recovery", "email": "a@b.example", "token": "654321" }));
    let put = reqs.last().unwrap();
    assert_eq!((put.method.as_str(), put.path.as_str()), ("PUT", "/auth/v1/user"));
    assert_eq!(put.body, json!({ "password": "new password!" }));
    assert!(put.bearer().is_some_and(|t| t.ends_with(".sig")), "the access token the code gave");
    assert!(c.signed_in() && store.load().as_deref() == Some("r1"));
    // (the same password as before is no error; too weak for the project is)
    assert_eq!(c.reset_password("a@b.example", "654321", "my old password"), Ok(()));
    assert_eq!(c.reset_password("a@b.example", "654321", "weakweak"), Err(Error::Code("weak_password")));
    let n = m.calls().len();
    assert_eq!(c.reset_password("a@b.example", "654321", "short"), Err(Error::Code("password_too_short")));
    assert_eq!(c.reset_password("a@b.example", "654321", &"x".repeat(73)), Err(Error::Code("password_too_long")));
    assert_eq!(m.calls().len(), n, "checked before the code is spent");
}

#[test]
fn the_access_token_is_renewed_before_it_expires_and_the_new_refresh_token_kept() {
    let renewals = Arc::new(AtomicUsize::new(0));
    let n = renewals.clone();
    let m = Mock::start(move |r| match r.path.as_str() {
        "/auth/v1/token?grant_type=refresh_token" => {
            // (the first one expires in 30 seconds: inside the minute's margin)
            let k = n.fetch_add(1, Ordering::SeqCst);
            ok(session(&format!("r{}", k + 2), if k == 0 { 30 } else { 3600 }))
        }
        "/rest/v1/rpc/heartbeat" => (204, vec![], Value::Null),
        _ => (404, vec![], Value::Null),
    });
    let store = signed_in("r1");
    let mut c = client(&m, &store);
    c.heartbeat("online", None).unwrap();
    c.heartbeat("online", None).unwrap();
    c.heartbeat("away", Some("HUB-AAAA-BBBB")).unwrap();
    assert_eq!(renewals.load(Ordering::SeqCst), 2, "renewed once more, then kept");
    let reqs = m.requests();
    let refreshes: Vec<&Value> = reqs.iter().filter(|r| r.path.contains("refresh_token")).map(|r| &r.body).collect();
    assert_eq!(refreshes, [&json!({ "refresh_token": "r1" }), &json!({ "refresh_token": "r2" })], "each renewal with the newest refresh token");
    assert_eq!(store.load().as_deref(), Some("r3"));
    let beats: Vec<&Req> = reqs.iter().filter(|r| r.path.ends_with("heartbeat")).collect();
    assert_eq!(beats.len(), 3);
    assert_ne!(beats[0].bearer(), beats[1].bearer(), "the second beat has the new access token");
    assert_eq!(beats[1].bearer(), beats[2].bearer());
    assert_eq!(beats[2].body, json!({ "p_status": "away", "p_session": "HUB-AAAA-BBBB" }));
    assert_eq!(beats[0].body["p_session"], Value::Null);
    // (the refresh token goes to the auth's body only, never as a bearer)
    assert!(reqs.iter().all(|r| r.bearer().is_none_or(|b| !b.starts_with('r'))));
    assert!(reqs.iter().all(|r| r.header("apikey") == Some(KEY)));
    assert_eq!(c.user_id(), Some("me"));
}

#[test]
fn a_401_renews_the_token_once_and_a_second_signs_out() {
    let refused = Arc::new(AtomicUsize::new(0));
    let n = refused.clone();
    let m = Mock::start(move |r| {
        if let Some(a) = refresh_route(r) {
            return a;
        }
        match r.path.as_str() {
            // (refused once: the retry with a new access token goes through)
            "/rest/v1/rpc/my_friends" if n.fetch_add(1, Ordering::SeqCst) == 0 => (401, vec![], json!({ "code": "PGRST301", "message": "JWT expired" })),
            "/rest/v1/rpc/my_friends" => ok(json!([{ "user_id": "u2", "display_name": "Ana", "status": "busy", "online": true, "session_code": "S1", "last_seen": "2026-10-16T10:00:00+00:00", "relation": "accepted", "incoming": false }])),
            "/rest/v1/rpc/heartbeat" => (401, vec![], json!({ "code": "PGRST303", "message": "JWT invalid" })),
            _ => (404, vec![], Value::Null),
        }
    });
    let store = signed_in("r1");
    let mut c = client(&m, &store);
    let f = c.friends().unwrap();
    assert_eq!((f[0].user_id.as_str(), f[0].display_name.as_str(), f[0].online, f[0].session_code.as_deref()), ("u2", "Ana", true, Some("S1")));
    assert_eq!(m.calls().iter().filter(|c| c.contains("grant_type=refresh_token")).count(), 2);
    assert!(c.signed_in());
    // (refused with a fresh access token too: signed out, the token gone from the store)
    assert_eq!(c.heartbeat("online", None), Err(Error::SignedOut));
    assert!(!c.signed_in());
    assert!(store.load().is_none());
    assert_eq!(c.friends(), Err(Error::Code("not_signed_in")));
}

#[test]
fn a_refresh_token_the_auth_refuses_signs_out_but_the_auth_being_down_does_not() {
    let m = Mock::start(|r| match r.body["refresh_token"].as_str() {
        Some("down") => (503, vec![], json!({ "message": "upstream" })),
        Some("busy") => auth_error(429, "over_request_rate_limit"),
        _ => auth_error(400, "refresh_token_already_used"),
    });
    for (token, answer) in [("down", Error::Code("server")), ("busy", Error::Code("too_many"))] {
        let store = signed_in(token);
        let mut c = client(&m, &store);
        assert_eq!(c.friends(), Err(answer));
        assert!(c.signed_in() && store.load().is_some());
    }
    let store = signed_in("used");
    let mut c = client(&m, &store);
    assert_eq!(c.friends(), Err(Error::SignedOut));
    assert!(store.load().is_none() && !c.signed_in());
    // (a session from before the move to Supabase is one the auth refuses too)
    let store = signed_in("better-auth-session.sig");
    assert_eq!(client(&m, &store).profile(), Err(Error::SignedOut));
    assert!(store.load().is_none());
}

#[test]
fn the_functions_codes_are_ours_and_the_servers_words_are_not_shown() {
    let m = Mock::start(|r| {
        if let Some(a) = refresh_route(r) {
            return a;
        }
        let code = r.body["p_code"].as_str().unwrap_or("");
        match (r.path.as_str(), code) {
            ("/rest/v1/rpc/send_friend_request", "SENT0001") => ok(json!("sent")),
            ("/rest/v1/rpc/send_friend_request", "ACCEPTED") => ok(json!([{ "result": "accepted" }])),
            ("/rest/v1/rpc/send_friend_request", "TOOMANY1") => ok(json!("too_many")),
            ("/rest/v1/rpc/send_friend_request", "FULLLIST") => ok(json!({ "send_friend_request": "full" })),
            ("/rest/v1/rpc/send_friend_request", "RAISED01") => (400, vec![], json!({ "code": "P0001", "message": "not_found", "details": null })),
            ("/rest/v1/rpc/send_friend_request", _) => (500, vec![], json!({ "code": "XX000", "message": "internal: relation \"auth.users\" ..." })),
            ("/rest/v1/rpc/set_display_name", _) => (409, vec![], json!({ "code": "23505", "message": "duplicate key value violates unique constraint \"profiles_name_ci\"" })),
            ("/rest/v1/rpc/remove_friend", _) => (204, vec![], Value::Null),
            ("/rest/v1/rpc/block_player", _) => ok(json!("not_found")),
            ("/rest/v1/rpc/send_invite", _) if r.body["p_to"] == "u3" => ok(json!("not_friends")),
            ("/rest/v1/rpc/send_invite", _) if r.body["p_to"] == "u4" => ok(json!("sent")),
            ("/rest/v1/rpc/send_invite", _) => (429, vec![], json!({ "message": "slow down" })),
            _ => (404, vec![], json!({ "code": "PGRST202", "message": "Could not find the function" })),
        }
    });
    let mut c = client(&m, &signed_in("r1"));
    assert_eq!(c.send_friend_request("SENT-0001"), Ok("sent"));
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_code": "SENT0001" }));
    assert_eq!(c.send_friend_request("ACCEPTED"), Ok("accepted"));
    assert_eq!(c.send_friend_request("TOOMANY1"), Err(Error::Code("too_many")));
    assert_eq!(c.send_friend_request("FULLLIST"), Err(Error::Code("full")));
    assert_eq!(c.send_friend_request("RAISED01"), Err(Error::Code("not_found")));
    let e = c.send_friend_request("ELSE0001").unwrap_err();
    assert_eq!(e, Error::Code("server"));
    assert!(!e.text().contains("auth.users"));
    assert_eq!(c.send_friend_request("short"), Err(Error::Code("bad_friend_code")));
    assert_eq!(c.set_display_name("Taken Name"), Err(Error::Code("name_taken")));
    assert_eq!(c.remove_friend("6f1c2e9a-0b7c-41d2-9e55-2f3a4b5c6d7e"), Ok(()));
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_other": "6f1c2e9a-0b7c-41d2-9e55-2f3a4b5c6d7e" }));
    assert_eq!(c.block_player("u2"), Err(Error::Code("not_found")));
    assert_eq!(c.send_invite("u2", "CODE"), Err(Error::Code("too_many")));
    assert_eq!(c.send_invite("u3", "CODE"), Err(Error::Code("not_allowed")));
    assert_eq!(c.send_invite("u4", "HUB-AAAA"), Ok(()));
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_to": "u4", "p_session": "HUB-AAAA" }));
    assert_eq!(c.dismiss_invite(&json!(7)), Err(Error::Code("unavailable")));
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_id": 7 }));
    assert_eq!(c.remove_friend("u2; drop"), Err(Error::Code("bad_input")));
}

#[test]
fn the_leaderboard_is_switched_and_the_record_reported() {
    let m = Mock::start(|r| {
        if let Some(a) = refresh_route(r) {
            return a;
        }
        match r.path.as_str() {
            "/rest/v1/rpc/set_leaderboard" => ok(json!(r.body["p_on"] == true)),
            "/rest/v1/rpc/report_stats" if r.body["p_points"] == 1 => ok(json!("too_soon")),
            "/rest/v1/rpc/report_stats" if r.body["p_points"] == 2 => ok(json!("bad")),
            "/rest/v1/rpc/report_stats" if r.body["p_points"] == 3 => ok(json!("no_profile")),
            "/rest/v1/rpc/report_stats" => ok(json!("ok")),
            _ => (404, vec![], Value::Null),
        }
    });
    let mut c = client(&m, &signed_in("r1"));
    c.set_leaderboard(true).unwrap();
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_on": true }));
    // (false: no profile to put on it)
    assert_eq!(c.set_leaderboard(false), Err(Error::Code("no_profile")));
    let s = Stats { points: 12_345, level: 7, duties: 40, minutes_driven: 2_400, km: 1_800 };
    assert_eq!(c.report_stats(&s), Ok("ok"));
    assert_eq!(m.requests().last().unwrap().body, json!({ "p_points": 12_345, "p_level": 7, "p_duties": 40, "p_minutes": 2_400, "p_km": 1_800 }));
    assert_eq!(c.report_stats(&Stats { points: 1, ..s }), Ok("too_soon"));
    assert_eq!(c.report_stats(&Stats { points: 2, ..s }), Err(Error::Code("bad_input")));
    assert_eq!(c.report_stats(&Stats { points: 3, ..s }), Err(Error::Code("no_profile")));
    assert_eq!(c.report_stats(&Stats { points: 4, level: 0, ..s }), Ok("ok"));
    assert_eq!(m.requests().last().unwrap().body["p_level"], 1, "the lowest level is 1");
}

#[test]
fn every_code_has_a_text_and_every_function_answer_is_a_code() {
    for (code, text) in codes::TEXTS {
        assert!(!text.is_empty() && codes::known(code) == Some(*code), "{code}");
    }
    for c in ["sent", "accepted", "already", "self", "not_found", "no_profile", "too_many", "full", "limit", "conflict", "too_large", "no_cloud_save", "bad_save", "weak_password", "same_password"] {
        assert!(codes::known(c).is_some(), "{c}");
    }
    assert_eq!(codes::text("nonsense"), codes::text("server"));
    assert_eq!(Error::Offline.code(), "offline");
}

#[test]
fn without_a_network_every_call_is_offline_and_the_session_stays() {
    // (a port nothing listens on)
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let store = signed_in("r1");
    let mut c = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}"), KEY).unwrap(), Box::new(store.clone()));
    let t = std::time::Instant::now();
    assert_eq!(c.friends(), Err(Error::Offline));
    assert_eq!(c.heartbeat("online", None), Err(Error::Offline));
    assert_eq!(c.sign_in("a@b.example", "12345678"), Err(Error::Offline));
    // (Windows refuses a closed local port only after about 2 s of retries, Linux at once)
    assert!(t.elapsed().as_secs() < if cfg!(windows) { 10 } else { 5 });
    assert!(c.signed_in() && store.load().is_some(), "offline is no reason to sign out");
    c.goodbye();
}

#[test]
fn plain_http_is_for_this_computer_only() {
    assert!(Endpoints::new("http://example.com", KEY).is_err());
    assert!(Endpoints::new("https://example.com/", KEY).is_ok_and(|e| e.base == "https://example.com"));
    assert!(Endpoints::local("http://example.com", KEY).is_err());
    assert!(Endpoints::local("http://127.0.0.1.example.com:80", KEY).is_err());
    assert!(Endpoints::local("http://localhost:54321", KEY).is_ok());
    let e = Endpoints::supabase();
    assert!(e.base.starts_with("https://") && e.key.starts_with("sb_publishable_") && !e.plain);
    assert_eq!(e.rest("/rpc/x"), format!("{}/rest/v1/rpc/x", crate::URL));
    assert!(crate::sign_up_page().ends_with("/account.html#sign-up"));
}

#[test]
fn deleting_the_account_checks_the_password_then_deletes_the_cloud_saves_then_the_account() {
    let files: Files = Default::default();
    for p in ["users/me/slot1/meta.json", "users/me/slot1/v1.omsave", "users/me/slot2/v4.omsave", "users/other/slot1/v1.omsave"] {
        files.lock().unwrap().files.insert(p.into(), b"x".to_vec());
    }
    let kept = files.clone();
    let m = Mock::start_raw(move |r| {
        if let Some(a) = refresh_route(r).map(raw_of).or_else(|| storage(&kept, r)) {
            return a;
        }
        raw_of(match r.path.as_str() {
            "/auth/v1/user" => ok(json!({ "id": "me", "email": "a@b.example" })),
            "/auth/v1/token?grant_type=password" if r.body["password"] == "my password" => ok(session("fresh", 3600)),
            "/auth/v1/token?grant_type=password" => auth_error(400, "invalid_credentials"),
            "/rest/v1/rpc/delete_my_account" => (204, vec![], Value::Null),
            _ => (404, vec![], Value::Null),
        })
    });
    let store = signed_in("r1");
    let mut c = client(&m, &store);
    assert_eq!(c.delete_account("not it!!"), Err(Error::Code("wrong_password")));
    assert!(!m.calls().iter().any(|p| p.contains("delete") || p.starts_with("DELETE")), "nothing deleted on a wrong password");
    assert_eq!(files.lock().unwrap().files.len(), 4);
    c.delete_account("my password").unwrap();
    let calls = m.calls();
    let at = |p: &str| calls.iter().rposition(|c| c == p).unwrap();
    assert!(at("DELETE /storage/v1/object/saves") < at("POST /rest/v1/rpc/delete_my_account"));
    assert_eq!(files.lock().unwrap().files.keys().collect::<Vec<_>>(), ["users/other/slot1/v1.omsave"], "the player's own files only");
    let del = m.requests().into_iter().find(|r| r.path.ends_with("delete_my_account")).unwrap();
    assert_eq!(del.body, json!({}));
    assert!(!c.signed_in() && store.load().is_none());
}

#[test]
fn the_invites_are_the_ones_sent_to_the_player() {
    let m = Mock::start(|r| {
        if let Some(a) = refresh_route(r) {
            return a;
        }
        match r.path.as_str() {
            "/rest/v1/invites?select=*&limit=50" => ok(json!([
                { "id": 1, "from_user": "u2", "to_user": "me", "session_code": "OLD", "created_at": "2026-10-16T09:00:00+00:00", "expires_at": "2026-10-16T09:15:00+00:00" },
                { "id": 2, "from_user": "me", "to_user": "u3", "session_code": "MINE", "created_at": "2026-10-16T09:30:00+00:00" },
                { "id": 3, "from_user": "u3", "to_user": "me", "session_code": "NEW", "created_at": "2026-10-16T10:00:00+00:00" }
            ])),
            p if p.starts_with("/rest/v1/profiles?") => ok(json!([{ "user_id": "me", "display_name": "Luc", "friend_code": "AB12CD34", "on_board": true }])),
            _ => (404, vec![], Value::Null),
        }
    });
    let mut c = client(&m, &signed_in("r1"));
    let inv = c.invites().unwrap();
    assert_eq!(inv.iter().map(|i| i.session_code.as_str()).collect::<Vec<_>>(), ["NEW", "OLD"]);
    assert_eq!(inv[0].key(), "3");
    let p = c.profile().unwrap().unwrap();
    assert_eq!((p.display_name.as_str(), p.friend_code.as_str(), p.on_board), ("Luc", "AB12CD34", true));
    assert!(m.calls().iter().any(|c| c == "GET /rest/v1/profiles?select=user_id,display_name,friend_code,on_board&user_id=eq.me&limit=1"));
}

#[test]
fn signing_out_tells_the_auth_and_forgets_the_session_whatever_it_says() {
    let m = Mock::start(|r| refresh_route(r).unwrap_or_else(|| if r.path == "/auth/v1/logout?scope=local" { (500, vec![], Value::Null) } else { (404, vec![], Value::Null) }));
    let store = signed_in("r1");
    let mut c = client(&m, &store);
    c.sign_out();
    assert!(!c.signed_in() && store.load().is_none());
    let out = m.requests().into_iter().find(|r| r.path == "/auth/v1/logout?scope=local").unwrap();
    assert!(out.bearer().is_some_and(|t| t.ends_with(".sig")));
}

#[test]
fn what_is_typed_is_checked() {
    assert!(check::email("a@b.example").is_ok());
    for bad in ["", "a", "a@b", "@b.example", "a b@c.example", "a@b@c.example", "a@.example", &format!("{}@b.example", "x".repeat(250))] {
        assert_eq!(check::email(bad), Err(Error::Code("invalid_email")), "{bad}");
    }
    assert_eq!(check::password("1234567"), Err(Error::Code("password_too_short")));
    assert_eq!(check::password(&"x".repeat(73)), Err(Error::Code("password_too_long")));
    assert!(check::password(&"x".repeat(72)).is_ok());
    // (bcrypt counts bytes)
    assert_eq!(check::password(&"ü".repeat(40)), Err(Error::Code("password_too_long")));
    assert_eq!(check::display_name("  Luc "), Ok("Luc".into()));
    assert_eq!(check::display_name("Bus_Fan-01 .x"), Ok("Bus_Fan-01 .x".into()));
    assert!(check::display_name("ab").is_err() && check::display_name(&"x".repeat(25)).is_err() && check::display_name("a\u{7}bc").is_err());
    // (the profiles table takes A to Z only)
    assert_eq!(check::display_name("Łukasz Żółć"), Err(Error::Code("bad_name")));
    assert_eq!(check::friend_code("AB12 CD34"), Ok("AB12CD34".into()));
    assert!(check::friend_code("AB12CD3").is_err() && check::friend_code("AB12CD3!").is_err());
    assert_eq!(check::email_code(" 123 456 "), Ok("123456".into()));
    assert!(check::email_code("12a456").is_err() && check::email_code("1234").is_err());
    assert!(check::status("busy").is_ok() && check::status("hacking").is_err());
    assert!(check::id("6f1c2e9a-0b7c-41d2-9e55-2f3a4b5c6d7e").is_ok());
    assert!(check::strength("password") == 0 && check::strength("Password12") == 1 && check::strength("Corr3ct horse battery") == 2);
}

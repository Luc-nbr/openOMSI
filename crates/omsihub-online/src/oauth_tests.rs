//! Signing in with Google, Discord and Steam (`oauth.rs`): the listener driven as the browser would
//! (plain requests on 127.0.0.1, no browser), what it hands back exchanged with the tests' server.

use crate::client::{test_jwt_with, test_now};
use crate::mock::{Mock, Req};
use crate::oauth::{challenge, Flow, Handoff, Provider, Texts, PATH};
use crate::tests::{ok, Shared};
use crate::{Client, Endpoints, Error};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const KEY: &str = "sb_publishable_test";

/// One request to the listener, as a browser sends it: the status and the body.
fn browse(port: u16, method: &str, target: &str, host: Option<&str>, extra: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let host = host.map_or(format!("127.0.0.1:{port}"), str::to_string);
    let head = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    s.write_all(head.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

/// A query's value, decoded (the tests' own, simple decoder).
fn param(url: &str, name: &str) -> String {
    let q = url.split_once('?').map_or("", |(_, q)| q);
    let raw = q.split('&').find_map(|p| p.strip_prefix(&format!("{name}="))).unwrap_or("");
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(&raw[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

fn endpoints(base: &str) -> Endpoints {
    Endpoints::local(base, KEY).unwrap()
}

/// The flow waiting on a thread, for the test to play the browser.
fn waiting(flow: Flow) -> std::thread::JoinHandle<Result<Handoff, Error>> {
    std::thread::spawn(move || flow.wait(Duration::from_secs(20), &AtomicBool::new(false)))
}

#[test]
fn google_comes_back_with_a_code_for_this_flow_only() {
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Google, Texts::default()).unwrap();
    let (port, url) = (flow.port(), flow.url().to_string());
    assert!(url.starts_with("http://127.0.0.1:9/auth/v1/authorize?provider=google&"), "{url}");
    assert_eq!(param(&url, "code_challenge_method"), "s256");
    let back = param(&url, "redirect_to");
    let state = back.strip_prefix(&format!("http://127.0.0.1:{port}{PATH}?state=")).expect("the listener with the state").to_string();
    assert!(state.len() >= 22);
    let challenge_sent = param(&url, "code_challenge");
    let t = waiting(flow);

    // (what is not this flow's answer leaves it waiting)
    assert_eq!(browse(port, "GET", "/favicon.ico", None, "", "").0, 404);
    assert_eq!(browse(port, "GET", &format!("{PATH}?code=c0de&state={state}"), Some("evil.example"), "", "").0, 400);
    let (status, page) = browse(port, "GET", &format!("{PATH}?code=forged&state=not-it"), None, "", "");
    assert_eq!(status, 200);
    assert!(page.contains("The sign-in could not be completed"));
    assert_eq!(browse(port, "PUT", PATH, None, "", "").0, 405);

    let (status, page) = browse(port, "GET", &format!("{PATH}?code=c0de&state={state}"), None, "", "");
    assert_eq!(status, 200);
    assert!(page.contains("You are signed in to Omsi-Hub") && page.contains(r#"lang="en""#));
    match t.join().unwrap() {
        Ok(Handoff::Code { code, verifier }) => {
            assert_eq!(code, "c0de");
            assert_eq!(challenge(&verifier), challenge_sent, "the verifier is the challenge's");
        }
        r => panic!("{r:?}"),
    }
}

#[test]
fn the_player_saying_no_ends_the_flow_and_the_page_says_so_in_their_language() {
    let texts = Texts { lang: "nl".into(), cancelled_text: "Het aanmelden is geannuleerd.".into(), ..Texts::default() };
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Discord, texts).unwrap();
    let port = flow.port();
    let state = param(&param(flow.url(), "redirect_to"), "state");
    let t = waiting(flow);
    let (_, page) = browse(port, "GET", &format!("{PATH}?error=access_denied&error_description=The+user+denied&state={state}"), None, "", "");
    assert!(page.contains(r#"lang="nl""#) && page.contains("Het aanmelden is geannuleerd."));
    assert!(!page.contains("The user denied"), "the provider's own words are not shown");
    assert_eq!(t.join().unwrap(), Err(Error::Code("oauth_cancelled")));
}

/// The relay page's state (the hidden field the script posts back).
fn relay_state(page: &str) -> String {
    page.split(r#"name="state" value=""#).nth(1).and_then(|r| r.split('"').next()).expect("the relay form").to_string()
}

#[test]
fn steam_hands_the_session_over_in_the_fragment_which_the_page_posts_back() {
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Steam, Texts::default()).unwrap();
    let (port, url) = (flow.port(), flow.url().to_string());
    assert_eq!(url, format!("http://127.0.0.1:9/functions/v1/steam-auth?to=http%3A%2F%2F127.0.0.1%3A{port}%2Fcallback"));
    let t = waiting(flow);
    // (Supabase's redirect after the one-time link: the fragment stays in the browser)
    let (status, page) = browse(port, "GET", PATH, None, "", "");
    assert_eq!(status, 200);
    assert!(page.contains("location.hash") && page.contains("history.replaceState"));
    let state = relay_state(&page);
    let form = |state: &str, hash: &str| format!("state={state}&hash={}", crate::http::encode(hash));
    let hash = "#access_token=eyJ.a.b&expires_in=3600&refresh_token=st3am&token_type=bearer&type=magiclink";
    let post = "Content-Type: application/x-www-form-urlencoded\r\n";
    assert_eq!(browse(port, "POST", PATH, None, post, &form("guess", hash)).0, 403);
    assert_eq!(browse(port, "POST", PATH, None, &format!("{post}Origin: https://evil.example\r\n"), &form(&state, hash)).0, 403);
    let (status, page) = browse(port, "POST", PATH, None, &format!("{post}Origin: http://127.0.0.1:{port}\r\n"), &form(&state, hash));
    assert_eq!(status, 200);
    assert!(page.contains("You are signed in to Omsi-Hub"));
    assert_eq!(t.join().unwrap(), Ok(Handoff::Refresh("st3am".into())));
}

#[test]
fn steam_failing_is_a_failure_not_a_cancel() {
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Steam, Texts::default()).unwrap();
    let port = flow.port();
    let t = waiting(flow);
    let state = relay_state(&browse(port, "GET", PATH, None, "", "").1);
    let body = format!("state={state}&hash={}", crate::http::encode("#error=access_denied&error_code=steam_failed"));
    let (_, page) = browse(port, "POST", PATH, None, "", &body);
    assert!(page.contains("Please return to Omsi-Hub and try again."));
    assert_eq!(t.join().unwrap(), Err(Error::Code("oauth_failed")));
}

#[test]
fn the_wait_ends_after_its_time_or_when_cancelled() {
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Google, Texts::default()).unwrap();
    assert_eq!(flow.wait(Duration::from_millis(250), &AtomicBool::new(false)), Err(Error::Code("oauth_timeout")));
    let flow = Flow::start(&endpoints("http://127.0.0.1:9"), Provider::Google, Texts::default()).unwrap();
    let port = flow.port();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let t = std::thread::spawn(move || flow.wait(Duration::from_secs(20), &c2));
    std::thread::sleep(Duration::from_millis(150));
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(t.join().unwrap(), Err(Error::Code("oauth_cancelled")));
    // (the listener is gone with the flow)
    std::thread::sleep(Duration::from_millis(50));
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
}

/// A session for a Google account (no password).
fn google_session(refresh: &str) -> Value {
    json!({ "access_token": test_jwt_with("me", test_now() + 3600, json!({ "provider": "google", "providers": ["google"] })), "token_type": "bearer", "expires_in": 3600, "refresh_token": refresh,
            "user": { "id": "me", "email": "a@b.example", "app_metadata": { "provider": "google", "providers": ["google"] }, "user_metadata": { "full_name": "Luc R" } } })
}

fn auth_server(r: &Req) -> (u16, Vec<(&'static str, String)>, Value) {
    match r.path.as_str() {
        "/auth/v1/token?grant_type=pkce" if r.body == json!({ "auth_code": "c0de", "code_verifier": "v3rifier" }) => ok(google_session("g1")),
        "/auth/v1/token?grant_type=pkce" => (404, vec![], json!({ "code": 404, "error_code": "flow_state_not_found", "msg": "invalid flow state, no valid flow state found" })),
        "/auth/v1/token?grant_type=refresh_token" if r.body["refresh_token"] == "st3am" => ok(json!({
            "access_token": test_jwt_with("me", test_now() + 3600, json!({ "provider": "email", "providers": ["email"], "steam_id": "76561198000000001" })), "refresh_token": "st3am+", "expires_in": 3600, "user": { "id": "me" } })),
        "/auth/v1/token?grant_type=refresh_token" => (400, vec![], json!({ "code": 400, "error_code": "refresh_token_not_found", "msg": "Invalid Refresh Token" })),
        _ => (404, vec![], Value::Null),
    }
}

#[test]
fn the_code_is_exchanged_with_its_verifier_and_kept_as_a_passwords_session() {
    let m = Mock::start(auth_server);
    let store = Shared::default();
    let mut c = Client::new(endpoints(&m.base), Box::new(store.clone()));
    assert_eq!(c.sign_in_with(&Handoff::Code { code: "c0de".into(), verifier: "wrong".into() }), Err(Error::Code("oauth_failed")));
    assert!(!c.signed_in());
    c.sign_in_with(&Handoff::Code { code: "c0de".into(), verifier: "v3rifier".into() }).unwrap();
    assert!(c.signed_in() && c.user_id() == Some("me"));
    assert_eq!(crate::store::SecretStore::load(&store).as_deref(), Some("g1"));
    assert!(!c.has_password(), "a Google account has no password");
    let req = m.requests().into_iter().rfind(|r| r.path.contains("pkce")).unwrap();
    assert_eq!(req.header("apikey"), Some(KEY));
    assert!(req.bearer().is_none());
}

#[test]
fn steams_refresh_token_is_renewed_with_the_auth_before_it_is_kept() {
    let m = Mock::start(auth_server);
    let store = Shared::default();
    let mut c = Client::new(endpoints(&m.base), Box::new(store.clone()));
    assert_eq!(c.sign_in_with(&Handoff::Refresh("forged".into())), Err(Error::Code("oauth_failed")));
    assert!(!c.signed_in() && crate::store::SecretStore::load(&store).is_none());
    c.sign_in_with(&Handoff::Refresh("st3am".into())).unwrap();
    assert_eq!(crate::store::SecretStore::load(&store).as_deref(), Some("st3am+"), "the renewed one, not the one from the browser");
    assert!(!c.has_password(), "a Steam account has no password");
}

#[test]
fn an_account_without_a_password_is_deleted_without_one() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/v1/token?grant_type=pkce" => ok(google_session("g1")),
        "/auth/v1/user" => ok(json!({ "id": "me", "email": "a@b.example", "app_metadata": { "provider": "google", "providers": ["google"] } })),
        "/storage/v1/object/list/saves" => ok(json!([])),
        "/rest/v1/rpc/delete_my_account" => (204, vec![], Value::Null),
        _ => (404, vec![], Value::Null),
    });
    let mut c = Client::new(endpoints(&m.base), Box::new(Shared::default()));
    c.sign_in_with(&Handoff::Code { code: "c".into(), verifier: "v".into() }).unwrap();
    c.delete_account("").unwrap();
    assert!(!m.calls().iter().any(|p| p.contains("grant_type=password")), "no password asked of it");
    assert!(m.calls().iter().any(|p| p == "POST /rest/v1/rpc/delete_my_account"));
    assert!(!c.signed_in());
}

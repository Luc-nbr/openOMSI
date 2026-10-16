//! Omsi-Hub online's flows, end to end, against the stand-in on this computer (never the real
//! project): two players made on "the website", then everything the app does with them.
//!
//!   node online/standin/standin.mjs                      (STANDIN_TTL=62 tries the renewal too)
//!   cargo run -p omsihub-online --example standin [http://127.0.0.1:54321]

use omsihub_online::store::{MemoryStore, SecretStore};
use omsihub_online::oauth::Texts;
use omsihub_online::{Client, Endpoints, Error, Flow, Provider, SaveError, Stats};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// A store the example still sees into after the client took it.
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

struct Site {
    base: String,
    key: String,
}

impl Site {
    /// The website's sign-up (`assets/auth.js`): the app has none.
    fn sign_up(&self, email: &str, password: &str, name: &str) {
        post(&format!("{}/auth/v1/signup", self.base), &self.key, json!({ "email": email, "password": password, "data": { "display_name": name } }));
    }

    /// The code the stand-in "mailed".
    fn mail(&self, email: &str) -> String {
        let v: Value = serde_json::from_reader(ureq::get(&format!("{}/__mail?email={email}", self.base)).call().expect("mail").into_reader()).unwrap();
        v["code"].as_str().expect("a code was mailed").to_string()
    }

    fn leaderboard(&self) -> Value {
        post(&format!("{}/rest/v1/rpc/hub_leaderboard", self.base), &self.key, json!({}))
    }
}

/// A JSON POST with only the publishable key, as the website's anonymous calls are.
fn post(url: &str, key: &str, body: Value) -> Value {
    let r = ureq::post(url).set("apikey", key).set("Content-Type", "application/json").send_string(&body.to_string()).expect(url);
    serde_json::from_reader(r.into_reader()).unwrap_or(Value::Null)
}

fn step(what: &str) {
    println!("  ok  {what}");
}

fn main() {
    let base = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:54321".into());
    let key = std::env::var("STANDIN_KEY").unwrap_or_else(|_| "sb_publishable_standin".into());
    let ep = Endpoints::local(&base, &key).expect("a stand-in on this computer");
    let site = Site { base: base.clone(), key };
    let run = std::process::id();
    let (ana_mail, bo_mail) = (format!("ana{run}@standin.example"), format!("bo{run}@standin.example"));
    let (ana_name, bo_name) = (format!("Ana {}", run % 10000), format!("Bo_{}", run % 10000));
    println!("Omsi-Hub online against {base}");

    // --- accounts made on the website, then signed in here --------------------------------------
    site.sign_up(&ana_mail, "ana password 1", &ana_name);
    site.sign_up(&bo_mail, "bo password 1", &bo_name);
    let ana_store = Shared::default();
    let mut ana = Client::new(ep.clone(), Box::new(ana_store.clone()));
    assert_eq!(ana.sign_in(&ana_mail, "ana password 1"), Err(Error::Code("email_not_verified")));
    step("an unconfirmed address is told to confirm first");
    assert_eq!(ana.verify_email(&ana_mail, "000000"), Err(Error::Code("invalid_code")));
    assert_eq!(ana.verify_email(&ana_mail, &site.mail(&ana_mail)), Ok(true));
    assert!(ana.signed_in() && ana_store.load().is_some());
    assert_eq!(ana.signed_up_name(), Some(ana_name.as_str()));
    step("the email's code confirms and signs in; the website's display name comes along");
    let mut bo = Client::new(ep.clone(), Box::new(Shared::default()));
    bo.send_verification(&bo_mail).unwrap();
    assert_eq!(bo.verify_email(&bo_mail, &site.mail(&bo_mail)), Ok(true));
    step("a new code sent, and it confirms");
    let v = omsihub_online::privacy::VERSION;
    for c in [&mut ana, &mut bo] {
        assert!(c.privacy_needed());
        assert_eq!(c.profile(), Err(Error::Code("privacy")));
        assert_eq!(c.check_privacy(), Ok(0));
        assert_eq!(c.accept_privacy(), Ok(v));
        assert!(!c.privacy_needed());
    }
    step("the account waits for the privacy statement: my_privacy 0, accept_privacy, then it works");
    for c in [&mut ana, &mut bo] {
        assert_eq!(c.profile(), Ok(None));
        let name = c.signed_up_name().unwrap().to_string();
        c.create_profile(&name).unwrap();
    }
    let (a, b) = (ana.profile().unwrap().unwrap(), bo.profile().unwrap().unwrap());
    assert_eq!((a.display_name.as_str(), a.friend_code.len(), a.on_board), (ana_name.as_str(), 8, false));
    assert_eq!(bo.set_display_name(&ana_name.to_uppercase()), Err(Error::Code("name_taken")));
    step("profiles made with a friend code; a name taken is said so");

    // --- friends, online state, invites --------------------------------------------------------------
    assert_eq!(ana.send_friend_request(&a.friend_code), Err(Error::Code("self")));
    assert_eq!(ana.send_friend_request("ZZZZZZZZ"), Err(Error::Code("not_found")));
    assert_eq!(ana.send_friend_request(&b.friend_code), Ok("sent"));
    let req = bo.friends().unwrap();
    assert_eq!((req[0].display_name.as_str(), req[0].relation.as_str(), req[0].incoming), (ana_name.as_str(), "pending", true));
    bo.respond_friend_request(&a.user_id, true).unwrap();
    assert_eq!(ana.send_friend_request(&b.friend_code), Err(Error::Code("already")));
    step("a friend request sent by code, seen, accepted");
    ana.heartbeat("busy", Some("HUB-AAAA-BBBB-CCCC")).unwrap();
    let f = bo.friends().unwrap();
    assert_eq!((f[0].relation.as_str(), f[0].online, f[0].status.as_str(), f[0].session_code.as_deref()), ("accepted", true, "busy", Some("HUB-AAAA-BBBB-CCCC")));
    step("the heartbeat: a friend is online, in a session, with its code");
    ana.send_invite(&b.user_id, "HUB-AAAA-BBBB-CCCC").unwrap();
    let inv = bo.invites().unwrap();
    assert_eq!((inv.len(), inv[0].from.as_str(), inv[0].session_code.as_str()), (1, a.user_id.as_str(), "HUB-AAAA-BBBB-CCCC"));
    bo.dismiss_invite(&inv[0].id).unwrap();
    assert!(bo.invites().unwrap().is_empty());
    step("an invite sent, seen, dismissed");
    // (a real "Play together" code: HUB- and up to 14 groups of four)
    let real = format!("HUB-{}", ["ABCD"; 14].join("-"));
    match ana.heartbeat("busy", Some(&real)) {
        Ok(()) => step(&format!("a real company code ({} characters) is taken", real.len())),
        Err(e) => println!("  !!  a real company code ({} characters) is refused: {} - the schema's session_code limit is too short", real.len(), e.code()),
    }

    // --- the leaderboard ------------------------------------------------------------------------------
    ana.set_leaderboard(true).unwrap();
    let s = Stats { points: 4200, level: 5, duties: 12, minutes_driven: 600, km: 310 };
    assert_eq!(ana.report_stats(&s), Ok("ok"));
    assert_eq!(ana.report_stats(&s), Ok("too_soon"));
    let board = site.leaderboard();
    assert!(board.as_array().unwrap().iter().any(|r| r["display_name"] == ana_name.as_str() && r["points"] == 4200));
    assert!(ana.profile().unwrap().unwrap().on_board);
    step("on the leaderboard, the record reported once in 30 seconds, read by the website");

    // --- cloud saves ------------------------------------------------------------------------------------
    let v1 = "company save v1 ".repeat(200).into_bytes();
    let first = ana.upload_save(None, "Stadtbus Nord", "Ana's PC", 0, &v1).unwrap();
    assert_eq!((first.slot, first.version), (1, 1));
    assert!(matches!(ana.upload_save(Some(1), "Stadtbus Nord", "", 0, b"stale"), Err(SaveError::Conflict(ref now)) if now.version == 1));
    ana.upload_save(Some(1), "Stadtbus Nord", "Ana's PC", 1, b"company save v2").unwrap();
    ana.upload_save(None, "Regio", "", 0, b"second company").unwrap();
    assert_eq!(ana.upload_save(None, "Third", "", 0, b"x"), Err(SaveError::Failed(Error::Code("limit"))));
    let (fetch, data) = ana.download_save(1, None).unwrap();
    assert_eq!((fetch.version, data.as_slice()), (2, &b"company save v2"[..]));
    assert_eq!(ana.download_save(1, Some(1)).unwrap().1, v1);
    let list = ana.saves_list().unwrap();
    assert_eq!((list.free(), list.slot(1).unwrap().versions.clone(), list.slot(1).unwrap().device.as_str()), (None, vec![1, 2], "Ana's PC"));
    assert!(bo.saves_list().unwrap().slots.iter().all(|s| s.empty), "nobody sees another player's saves");
    step("cloud saves: compressed, two slots, versions, a conflict, the limit, downloads checked, private");

    // --- the access token renewed, a refresh token used twice refused ----------------------------------
    let old = ana_store.load().unwrap();
    let ttl: u64 = std::env::var("STANDIN_TTL").ok().and_then(|t| t.parse().ok()).unwrap_or(3600);
    if ttl <= 65 {
        std::thread::sleep(std::time::Duration::from_secs(ttl.saturating_sub(58)));
        ana.friends().unwrap();
        assert_ne!(ana_store.load().unwrap(), old, "renewed, with a new refresh token");
        let mut stale = Client::new(ep.clone(), Box::new(Shared(Arc::new(MemoryStore::with(&old)))));
        assert_eq!(stale.friends(), Err(Error::SignedOut));
        step("the access token renewed before it expired; the old refresh token is refused");
    } else {
        println!("  --  (STANDIN_TTL=62 on the stand-in tries the renewal)");
    }

    // --- the password reset -----------------------------------------------------------------------------
    ana.sign_out();
    assert!(!ana.signed_in() && ana_store.load().is_none());
    ana.request_reset(&ana_mail).unwrap();
    ana.reset_password(&ana_mail, &site.mail(&ana_mail), "ana password 2").unwrap();
    assert!(ana.signed_in());
    ana.sign_out();
    assert_eq!(ana.sign_in(&ana_mail, "ana password 1"), Err(Error::Code("invalid_credentials")));
    ana.sign_in(&ana_mail, "ana password 2").unwrap();
    assert!(ana.privacy_needed());
    assert_eq!(ana.check_privacy(), Ok(v));
    step("signed out; the password reset by a code, which signs in; the old one no longer works; the agreement kept");

    // --- the account deleted -------------------------------------------------------------------------------
    assert_eq!(ana.delete_account("not the password"), Err(Error::Code("wrong_password")));
    assert_eq!(ana.saves_list().unwrap().free(), None, "nothing deleted on a wrong password");
    ana.delete_account("ana password 2").unwrap();
    assert!(!ana.signed_in());
    assert_eq!(ana.sign_in(&ana_mail, "ana password 2"), Err(Error::Code("invalid_credentials")));
    assert!(bo.friends().unwrap().is_empty());
    step("the account deleted with its cloud saves; gone for its friends too");
    bo.goodbye();
    bo.sign_out();

    // --- Google, Discord and Steam, through "the browser" -------------------------------------------------
    for provider in [Provider::Google, Provider::Discord, Provider::Steam] {
        let mut c = Client::new(ep.clone(), Box::new(Shared::default()));
        let flow = Flow::start(&ep, provider, Texts::default()).unwrap();
        let url = flow.url().to_string();
        let waiting = std::thread::spawn(move || flow.wait(Duration::from_secs(30), &AtomicBool::new(false)));
        browser(&url);
        let handoff = waiting.join().unwrap().expect("the browser handed the sign-in back");
        c.sign_in_with(&handoff).unwrap();
        assert!(c.signed_in() && !c.has_password());
        if c.check_privacy().unwrap() < v {
            c.accept_privacy().unwrap();
        }
        if c.profile().unwrap().is_none() {
            c.create_profile(&format!("{} {}", provider.name(), run % 10000)).unwrap();
        }
        assert!(c.profile().unwrap().is_some());
        c.delete_account("").unwrap();
        assert!(!c.signed_in());
        step(&format!("signed in with {} through the browser, the profile made, deleted without a password", provider.name()));
    }
    // (a player who says no at the provider)
    let flow = Flow::start(&ep, Provider::Google, Texts::default()).unwrap();
    let url = format!("{}&standin_answer=deny", flow.url());
    let waiting = std::thread::spawn(move || flow.wait(Duration::from_secs(30), &AtomicBool::new(false)));
    browser(&url);
    assert_eq!(waiting.join().unwrap(), Err(Error::Code("oauth_cancelled")));
    step("a player who says no at the provider: cancelled");
    println!("All flows work against the stand-in.");
}

/// What the system's browser does with the sign-in's address: the redirects followed to the app's
/// listener, and there, for a session in the fragment, the page's script played (the fragment
/// posted back with the page's state).
fn browser(start: &str) {
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    let mut url = start.to_string();
    for _ in 0..5 {
        let r = match agent.get(url.split('#').next().unwrap()).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => panic!("{e}"),
        };
        if let Some(next) = r.header("location").map(str::to_string) {
            url = next;
            continue;
        }
        let page = r.into_string().unwrap();
        if let Some(state) = page.split(r#"name="state" value=""#).nth(1).and_then(|r| r.split('"').next()) {
            let hash = url.split_once('#').map_or("", |(_, h)| h);
            let origin = url.split("/callback").next().unwrap().to_string();
            let _ = agent.post(url.split('#').next().unwrap()).set("Origin", &origin).send_form(&[("state", state), ("hash", &format!("#{hash}"))]);
        }
        return;
    }
    panic!("too many redirects");
}

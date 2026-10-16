//! The cloud saves the way the launcher's page drives them (`ol_…`, `cs_…`), end to end against the
//! stand-in on this computer (never the real project), with a home of its own (a temporary
//! `~/.openomsi`) and the session in memory only. A debug build only (`online.rs`' `standin`):
//!
//!   node online/standin/standin.mjs
//!   OMSIHUB_STANDIN=http://127.0.0.1:54321 cargo run -p omsi-launcher-api --example cloud_standin
//!
//! An account made on "the website", signed in through `ol_verify`; three companies; two put in the
//! cloud and the third meeting the limit, then replacing one; a change going up; another computer's
//! newer version offered and taken; changes on both sides asked and both kept; a slot downloaded;
//! one removed; the account deleted with its cloud saves.

use omsi_launcher_lib::company::{found, store, Founding};
use omsihub_online::store::MemoryStore;
use omsihub_online::{Client, Endpoints};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

struct NoWindow;

impl omsi_launcher_api::Platform for NoWindow {
    fn renderer(&self, _req: Value, _wait: Duration) -> Result<(Value, Vec<u8>), String> {
        Err("no renderer here".into())
    }
}

fn call(cmd: &str, args: Value) -> Value {
    omsi_launcher_api::call(cmd, &args, &NoWindow).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn step(what: &str) {
    println!("  ok  {what}");
}

/// One plain HTTP request to the stand-in (the website's part): its JSON answer.
fn http(base: &str, method: &str, path: &str, key: &str, body: Option<Value>) -> Value {
    let host = base.trim_start_matches("http://");
    let mut s = std::net::TcpStream::connect(host).expect("the stand-in runs");
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    // (HTTP/1.0: the answer comes whole, not chunked)
    write!(s, "{method} {path} HTTP/1.0\r\nHost: {host}\r\napikey: {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    serde_json::from_str(out.split_once("\r\n\r\n").map_or("", |x| x.1)).unwrap_or(Value::Null)
}

/// A company founded here by the launcher's driver, and changed (a day's money).
fn company(data: &Path, name: &str) -> String {
    let mut c = found(&Founding { name: name.into(), ..Default::default() }, &omsi_launcher_lib::load_config().profile);
    c.id = store::unused_id(data, name);
    store::save(data, &c).unwrap();
    c.id
}

fn change(data: &Path, id: &str, cash: i64) {
    let mut c = store::load(data, id).unwrap();
    c.cash += cash;
    // (another time than the file's: the launcher reads a file again by its size and time)
    std::thread::sleep(Duration::from_millis(20));
    store::save(data, &c).unwrap();
}

fn cash(data: &Path, id: &str) -> i64 {
    store::load(data, id).unwrap().cash
}

/// The chooser's badge of a company (`cs_state`) once it is `want`, or as it is after 20 seconds
/// (the worker uploads, and lists the cloud again on its next round, within 5 seconds).
fn badge(id: &str, want: &str) -> Value {
    let t = Instant::now();
    loop {
        let s = call("cs_state", json!({}));
        let b = s["companies"].get(id).cloned().unwrap_or(json!({ "state": "local" }));
        if b["state"] == want || t.elapsed() > Duration::from_secs(20) {
            return b;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn main() {
    let base = std::env::var("OMSIHUB_STANDIN").expect("OMSIHUB_STANDIN=http://127.0.0.1:54321 (a debug build)");
    let key = std::env::var("OMSIHUB_STANDIN_KEY").unwrap_or_else(|_| "sb_publishable_standin".into());
    let home = std::env::temp_dir().join(format!("omsihub-cloud-standin-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    let data = omsi_launcher_lib::data_dir();
    assert!(data.starts_with(&home), "a home of its own: {}", data.display());
    std::fs::create_dir_all(&data).unwrap();
    println!("The launcher's cloud saves against {base} (home {})", home.display());

    // --- an account made on the website, signed in through the page's panel ---------------------
    let run = std::process::id();
    let (email, password, name) = (format!("cloud{run}@standin.example"), "cloud password 1", format!("Cloud {}", run % 10000));
    http(&base, "POST", "/auth/v1/signup", &key, Some(json!({ "email": email, "password": password, "data": { "display_name": name } })));
    let r = call("ol_sign_in", json!({ "email": email, "password": password }));
    assert_eq!(r["step"], "verify", "{r}");
    let code = http(&base, "GET", &format!("/__mail?email={email}"), &key, None)["code"].as_str().unwrap().to_string();
    let r = call("ol_verify", json!({ "email": email, "code": code, "password": password }));
    assert!(r.get("error").is_none(), "{r}");
    let st = call("ol_state", json!({}));
    assert!(st["sign_up_url"].as_str().is_some_and(|u| u.ends_with("/account.html#sign-up")), "{st}");
    step("signed in through ol_verify; the website's sign-up page is in the state");
    assert_eq!(call("cs_state", json!({}))["companies"], json!({}));

    // --- two companies into the cloud, the third meets the limit --------------------------------
    let (a, b, c) = (company(&data, "Stadtbus Nord"), company(&data, "Regio Süd"), company(&data, "Dritter Betrieb"));
    let companies = call("cs_companies", json!({}));
    assert_eq!(companies.as_array().map(Vec::len), Some(3), "{companies}");
    for (id, slot) in [(&a, 1), (&b, 2)] {
        let r = call("cs_put", json!({ "id": id }));
        assert_eq!(r["slot"], slot, "{r}");
    }
    let r = call("cs_put", json!({ "id": c }));
    assert_eq!((r["limit"].as_bool(), r["slots"].as_array().map(Vec::len)), (Some(true), Some(2)), "{r}");
    assert_eq!(badge(&a, "synced")["state"], "synced");
    step("two companies in the cloud; the third is offered keep-local or replace");
    let r = call("cs_put", json!({ "id": c, "replace": 2 }));
    assert_eq!(r["slot"], 2, "{r}");
    assert_eq!(badge(&b, "local")["state"], "local");
    let slots = call("cs_state", json!({}))["slots"].clone();
    assert_eq!((slots[1]["name"].as_str(), slots[1]["linked"].as_str()), (Some("Dritter Betrieb"), Some(c.as_str())), "{slots}");
    step("the third replaced the second, which stays here as a local save");

    // --- a change goes up --------------------------------------------------------------------------
    change(&data, &a, 1000);
    call("cs_sync", json!({ "id": a }));
    assert_eq!(badge(&a, "synced")["state"], "synced");
    let mut other = Client::new(Endpoints::local(&base, &key).unwrap(), Box::new(MemoryStore::new()));
    other.sign_in(&email, password).unwrap();
    let cloud = other.saves_list().unwrap();
    assert_eq!(cloud.slot(1).unwrap().version, 2, "{cloud:?}");
    step("a changed cloud company goes up when its pages are left");

    // --- another computer's newer version, offered and taken ------------------------------------------
    let (_, packed) = other.download_save(1, None).unwrap();
    other.upload_save(Some(1), "Stadtbus Nord", "Laptop", 2, &packed).unwrap();
    let r = call("cs_open", json!({ "id": a }));
    assert_eq!((r["ask"].as_str(), r["cloud"]["device"].as_str(), r["cloud"]["version"].as_u64()), (Some("newer"), Some("Laptop"), Some(3)), "{r}");
    let r = call("cs_resolve", json!({ "id": a, "keep": "cloud" }));
    assert!(r["ok"].as_bool() == Some(true), "{r}");
    assert_eq!(badge(&a, "synced")["state"], "synced");
    step("the laptop's newer version is offered on opening, and comes down");

    // --- changed on both sides: both kept --------------------------------------------------------------
    let before = cash(&data, &a);
    let (_, packed) = other.download_save(1, None).unwrap();
    other.upload_save(Some(1), "Stadtbus Nord", "Laptop", 3, &packed).unwrap();
    change(&data, &a, 5);
    let r = call("cs_open", json!({ "id": a }));
    assert_eq!(r["ask"], "conflict", "{r}");
    let r = call("cs_resolve", json!({ "id": a, "keep": "both" }));
    let copy = r["copy"].as_str().unwrap_or_else(|| panic!("{r}")).to_string();
    assert_eq!((cash(&data, &a), cash(&data, &copy)), (before, before + 5), "the cloud's here, this PC's as a copy");
    assert_eq!(badge(&copy, "local")["state"], "local");
    step("changes on both sides are asked; both kept, this PC's as a local copy");

    // --- a slot downloaded, one removed --------------------------------------------------------------
    std::fs::remove_file(store::path_of(&data, &c)).unwrap();
    let r = call("cs_download", json!({ "slot": 2 }));
    let got = r["reread"].as_str().unwrap_or_else(|| panic!("{r}")).to_string();
    assert_eq!(store::load(&data, &got).unwrap().name, "Dritter Betrieb");
    let r = call("cs_remove", json!({ "slot": 2 }));
    assert!(r["ok"].as_bool() == Some(true), "{r}");
    assert!(other.saves_list().unwrap().slots[1].empty);
    assert!(store::path_of(&data, &got).exists(), "the company here stays, local");
    step("a cloud slot downloaded to this PC; one removed from the cloud, kept here");

    // --- the account deleted with its cloud saves ---------------------------------------------------
    let r = call("ol_delete", json!({ "password": "not it" }));
    assert!(r.get("error").is_some(), "{r}");
    assert!(!other.saves_list().unwrap().slots[0].empty, "nothing deleted on a wrong password");
    let r = call("ol_delete", json!({ "password": password }));
    assert!(r.get("error").is_none(), "{r}");
    assert_eq!(call("cs_state", json!({}))["companies"], json!({}), "every company here is local");
    assert!(other.sign_in(&email, password).is_err());
    step("the account deleted with its cloud saves; every company here is local");
    let _ = std::fs::remove_dir_all(&home);
    println!("The launcher's cloud saves work against the stand-in.");
}

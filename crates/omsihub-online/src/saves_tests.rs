//! The cloud saves against the tests' server: a fake of the function `saves` and of its storage
//! (`Fake`), as docs/omsihub/ONLINE.md describes them - two slots, `limit`, `conflict` on another
//! base version, the presigned `PUT` refused unless its signed headers are the ones sent, the
//! commit checking the object's size and sha256. No test reaches the real project.

use crate::client::{test_jwt, test_now};
use crate::mock::{Body, Mock, Req};
use crate::saves::{self, compress, decompress, sha256_hex, SaveError};
use crate::store::MemoryStore;
use crate::{codes, Client, Endpoints, Error};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
struct SlotState {
    name: String,
    version: u64,
    size: u64,
    sha256: String,
    device: String,
}

/// The function and its bucket, in memory.
#[derive(Default)]
struct Fake {
    slots: HashMap<u64, SlotState>,
    /// `slot/version` → the bytes `PUT` and the headers they came with.
    objects: HashMap<String, (Vec<u8>, Vec<(String, String)>)>,
    /// What `upload_begin` promised: `slot/version` → (size, sha256, name, device).
    pending: HashMap<String, (u64, String, String, String)>,
    /// Every answer of the function's routes this code instead (the error tests').
    refuse: Option<(u16, &'static str)>,
}

type Shared = Arc<Mutex<Fake>>;

fn slot_json(n: u64, s: Option<&SlotState>) -> Value {
    match s {
        None => json!({ "slot": n, "empty": true }),
        Some(s) => json!({ "slot": n, "empty": false, "name": s.name, "version": s.version, "size": s.size, "sha256": s.sha256, "updated_at": "2026-10-16T10:00:00Z", "device": s.device, "versions": [s.version] }),
    }
}

fn no(status: u16, code: &str) -> (u16, Vec<(&'static str, String)>, Body) {
    (status, vec![], Body::Json(json!({ "ok": false, "error": code })))
}

fn yes(v: Value) -> (u16, Vec<(&'static str, String)>, Body) {
    let mut v = v;
    v["ok"] = json!(true);
    (200, vec![], Body::Json(v))
}

fn answer(fake: &Shared, r: &Req) -> (u16, Vec<(&'static str, String)>, Body) {
    if r.path == "/auth/token" {
        return (200, vec![], Body::Json(json!({ "token": test_jwt("me", test_now() + 3600) })));
    }
    let mut f = fake.lock().unwrap();
    let host = r.header("host").unwrap_or("").to_string();
    if let Some(key) = r.path.strip_prefix("/s3/") {
        // (the presigned URLs: no token of ours goes there)
        assert!(r.header("authorization").is_none(), "{r:?}");
        return match r.method.as_str() {
            "PUT" => {
                let Some((size, sha, _, _)) = f.pending.get(key).cloned() else { return (403, vec![], Body::Raw(Vec::new())) };
                // (the signature covers these: another value is refused)
                let signed = r.header("content-length") == Some(size.to_string().as_str()) && r.header("x-amz-meta-sha256") == Some(sha.as_str()) && r.header("content-type") == Some("application/zstd");
                if !signed {
                    return (403, vec![], Body::Raw(Vec::new()));
                }
                f.objects.insert(key.to_string(), (r.raw.clone(), r.headers.clone()));
                (200, vec![], Body::Raw(Vec::new()))
            }
            _ => match f.objects.get(key) {
                Some((b, _)) => (200, vec![], Body::Raw(b.clone())),
                None => (404, vec![], Body::Raw(Vec::new())),
            },
        };
    }
    let route = r.path.strip_prefix("/saves/").unwrap_or("");
    assert_eq!(r.method, "POST");
    assert!(r.bearer().is_some_and(|b| b != "tok.sig"), "the function gets the JWT, not the session: {r:?}");
    if let Some((status, code)) = f.refuse {
        return no(status, code);
    }
    let b = &r.body;
    match route {
        "list" => yes(json!({ "max_slots": 2, "max_bytes": saves::MAX_BYTES, "slots": [slot_json(1, f.slots.get(&1)), slot_json(2, f.slots.get(&2))] })),
        "upload_begin" => {
            let slot = match b["slot"].as_u64() {
                Some(n) => n,
                None => match (1..=2).find(|n| !f.slots.contains_key(n)) {
                    Some(n) => n,
                    None => return (409, vec![], Body::Json(json!({ "ok": false, "error": "limit", "max_slots": 2 }))),
                },
            };
            let current = f.slots.get(&slot).map_or(0, |s| s.version);
            if b["base_version"].as_u64() != Some(current) {
                return (409, vec![], Body::Json(json!({ "ok": false, "error": "conflict", "current": slot_json(slot, f.slots.get(&slot)) })));
            }
            if b["size"].as_u64().unwrap_or(0) > saves::MAX_BYTES {
                return no(413, "too_large");
            }
            let version = current + 1;
            let key = format!("slot{slot}/v{version}");
            let (size, sha) = (b["size"].as_u64().unwrap(), b["sha256"].as_str().unwrap().to_string());
            f.pending.insert(key.clone(), (size, sha.clone(), b["name"].as_str().unwrap_or("").into(), b["device"].as_str().unwrap_or("").into()));
            let headers = json!({ "content-type": "application/zstd", "content-length": size.to_string(), "x-amz-meta-name": b["name"], "x-amz-meta-sha256": sha, "x-amz-meta-size": size.to_string(), "x-amz-meta-device": b["device"] });
            yes(json!({ "slot": slot, "version": version, "method": "PUT", "url": format!("http://{host}/s3/{key}"), "headers": headers, "expires_in": 600 }))
        }
        "upload_commit" => {
            let (slot, version) = (b["slot"].as_u64().unwrap(), b["version"].as_u64().unwrap());
            let key = format!("slot{slot}/v{version}");
            let Some((size, sha, name, device)) = f.pending.get(&key).cloned() else { return no(404, "not_uploaded") };
            let Some((bytes, _)) = f.objects.get(&key).cloned() else { return no(404, "not_uploaded") };
            if bytes.len() as u64 != size || sha256_hex(&bytes) != sha || b["sha256"].as_str().is_some_and(|s| s != sha) {
                f.objects.remove(&key);
                return no(422, "mismatch");
            }
            let s = SlotState { name, version, size, sha256: sha, device };
            f.slots.insert(slot, s.clone());
            let mut v = slot_json(slot, Some(&s));
            v["committed"] = json!(true);
            yes(v)
        }
        "download" => {
            let slot = b["slot"].as_u64().unwrap();
            let Some(s) = f.slots.get(&slot).cloned() else { return no(404, "not_found") };
            let version = b["version"].as_u64().unwrap_or(s.version);
            let Some((bytes, _)) = f.objects.get(&format!("slot{slot}/v{version}")) else { return no(404, "not_found") };
            // (the committed meta's size and sha256; a kept version's as it was committed)
            let (size, sha) = if version == s.version { (s.size, s.sha256.clone()) } else { (bytes.len() as u64, sha256_hex(bytes)) };
            yes(json!({ "slot": slot, "version": version, "name": s.name, "size": size, "sha256": sha, "method": "GET", "url": format!("http://{host}/s3/slot{slot}/v{version}"), "expires_in": 600 }))
        }
        "delete" => {
            let slot = b["slot"].as_u64().unwrap();
            let gone = f.slots.remove(&slot).is_some();
            let before = f.objects.len();
            f.objects.retain(|k, _| !k.starts_with(&format!("slot{slot}/")));
            yes(json!({ "slot": slot, "deleted": (before - f.objects.len()) as u64 + gone as u64 }))
        }
        "delete_all" => {
            let n = f.objects.len() + f.slots.len();
            f.objects.clear();
            f.slots.clear();
            yes(json!({ "deleted": n }))
        }
        _ => no(404, "not_found"),
    }
}

fn cloud() -> (Mock, Shared, Client) {
    let fake: Shared = Default::default();
    let f = fake.clone();
    let m = Mock::start_raw(move |r| answer(&f, r));
    let c = Client::new(Endpoints::local(&m.base), Box::new(MemoryStore::with("tok.sig")));
    (m, fake, c)
}

/// A company's save as the launcher packs it: JSON that shrinks well.
fn archive(tag: &str) -> Vec<u8> {
    let mut a = b"OMSIHUB-SAVE 1\n".to_vec();
    for k in 0..400 {
        a.extend(format!("{{\"bus\":{k},\"line\":\"{tag}\",\"note\":\"the same words again and again\"}}\n").as_bytes());
    }
    a
}

#[test]
fn a_save_goes_up_compressed_with_exactly_the_signed_headers_and_comes_back_the_same() {
    let (m, fake, mut c) = cloud();
    let a = archive("first");
    let slot = c.upload_save(None, "Stadtbus Grundorf", "PC-1", 0, &a).unwrap();
    assert_eq!((slot.slot, slot.version, slot.name.as_str(), slot.device.as_str()), (1, 1, "Stadtbus Grundorf", "PC-1"));
    let packed = compress(&a).unwrap();
    assert!(packed.bytes.len() * 4 < a.len(), "zstd: {} of {}", packed.bytes.len(), a.len());
    assert_eq!(slot.sha256, packed.sha256);
    assert_eq!(compress(&a).unwrap(), packed, "the same save packs the same");
    // (the PUT carried the begun headers, the commit the sha256)
    let reqs = m.requests();
    let put = reqs.iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(put.header("x-amz-meta-name"), Some("Stadtbus Grundorf"));
    assert_eq!(put.header("x-amz-meta-device"), Some("PC-1"));
    assert_eq!(put.raw, packed.bytes);
    let commit = reqs.iter().find(|r| r.path == "/saves/upload_commit").unwrap();
    assert_eq!(commit.body, json!({ "slot": 1, "version": 1, "sha256": packed.sha256 }));
    let begin = reqs.iter().find(|r| r.path == "/saves/upload_begin").unwrap();
    assert_eq!(begin.body["base_version"], 0);
    assert!(begin.body.get("slot").is_none(), "the first free slot");
    // (down again: the same bytes)
    let (fetch, back) = c.download_save(1, None).unwrap();
    assert_eq!((fetch.version, back), (1, a.clone()));
    // (a second version on top of the first)
    let b = archive("second");
    let slot = c.upload_save(Some(1), "Stadtbus Grundorf", "PC-1", 1, &b).unwrap();
    assert_eq!(slot.version, 2);
    assert_eq!(c.download_save(1, None).unwrap().1, b);
    assert_eq!(c.download_save(1, Some(1)).unwrap().1, a, "a kept version");
    let list = c.saves_list().unwrap();
    assert_eq!((list.max_slots, list.slots.len(), list.free()), (2, 2, Some(2)));
    assert_eq!(list.slot(1).unwrap().version, 2);
    assert_eq!(fake.lock().unwrap().slots.len(), 1);
}

#[test]
fn a_damaged_download_is_refused_by_its_sha256_and_a_damaged_upload_by_the_commit() {
    let (_m, fake, mut c) = cloud();
    c.upload_save(None, "Alt", "PC", 0, &archive("x")).unwrap();
    // (a byte changed in the storage)
    fake.lock().unwrap().objects.get_mut("slot1/v1").unwrap().0[5] ^= 0xff;
    assert_eq!(c.download_save(1, None), Err(SaveError::Failed(Error::Code("bad_save"))));
    // (not zstd at all, or empty)
    assert_eq!(decompress(b"not a save"), Err(Error::Code("bad_save")));
    // (the headers the function signed are the ones sent: another size the storage refuses)
    let packed = compress(&archive("y")).unwrap();
    let mut begun = c.saves_upload_begin(Some(1), "Alt", &packed, 1, "PC").unwrap();
    begun.headers.retain(|(k, _)| k != "x-amz-meta-sha256");
    assert_eq!(c.transfer().put(&begun, &packed), Err(Error::Code("upload_failed")));
    assert_eq!(c.saves_upload_commit(1, begun.version, &packed.sha256), Err(SaveError::Failed(Error::Code("not_uploaded"))));
    // (a commit whose sha256 is not the object's)
    let begun = c.saves_upload_begin(Some(1), "Alt", &packed, 1, "PC").unwrap();
    c.transfer().put(&begun, &packed).unwrap();
    assert_eq!(c.saves_upload_commit(1, begun.version, &"0".repeat(64)), Err(SaveError::Failed(Error::Code("mismatch"))));
    assert_eq!(c.saves_list().unwrap().slot(1).unwrap().version, 1, "the good save stays");
}

#[test]
fn another_base_version_is_a_conflict_with_the_slot_as_it_is_now() {
    let (_m, _fake, mut c) = cloud();
    c.upload_save(None, "Stadtbus", "Laptop", 0, &archive("a")).unwrap();
    c.upload_save(Some(1), "Stadtbus", "Laptop", 1, &archive("b")).unwrap();
    // (this PC last synced version 1; the laptop has uploaded version 2 since)
    match c.upload_save(Some(1), "Stadtbus", "Desktop", 1, &archive("c")) {
        Err(SaveError::Conflict(now)) => assert_eq!((now.slot, now.version, now.device.as_str(), now.updated_at.as_deref()), (1, 2, "Laptop", Some("2026-10-16T10:00:00Z"))),
        other => panic!("{other:?}"),
    }
    // (a company not in the cloud yet is base 0: into a taken slot, a conflict too)
    assert!(matches!(c.upload_save(Some(1), "Other", "Desktop", 0, &archive("d")), Err(SaveError::Conflict(_))));
    assert_eq!(SaveError::Conflict(Default::default()).error(), Error::Code("conflict"));
    // (the player kept this PC's: on top of the version now)
    assert_eq!(c.upload_save(Some(1), "Stadtbus", "Desktop", 2, &archive("c")).unwrap().version, 3);
}

#[test]
fn the_third_company_meets_the_limit_and_a_slot_can_be_emptied_for_it() {
    let (_m, _fake, mut c) = cloud();
    c.upload_save(None, "One", "PC", 0, &archive("1")).unwrap();
    c.upload_save(None, "Two", "PC", 0, &archive("2")).unwrap();
    let e = c.upload_save(None, "Three", "PC", 0, &archive("3")).unwrap_err();
    assert_eq!(e, SaveError::Failed(Error::Code("limit")));
    assert!(e.error().text().contains("keep this company local"));
    assert_eq!(c.saves_list().unwrap().free(), None);
    // (replace one: its slot emptied, then the third goes into it)
    assert_eq!(c.saves_delete(2), Ok(1 + 1));
    let s = c.upload_save(Some(2), "Three", "PC", 0, &archive("3")).unwrap();
    assert_eq!((s.slot, s.name.as_str()), (2, "Three"));
    assert_eq!(c.saves_delete(3), Err(Error::Code("bad_input")), "slot 1 or 2 only, checked here");
    assert!(c.saves_delete_all().unwrap() > 0);
    assert_eq!(c.saves_list().unwrap().free(), Some(1));
}

#[test]
fn every_code_of_the_function_is_ours_with_a_text() {
    for (status, code, ours) in [
        (409, "limit", "limit"),
        (422, "mismatch", "mismatch"),
        (413, "too_large", "too_large"),
        (429, "rate_limited", "rate_limited"),
        (404, "not_found", "no_cloud_save"),
        (404, "not_uploaded", "not_uploaded"),
        (400, "bad_request", "bad_input"),
        (500, "internal", "server"),
        (429, "", "rate_limited"),
    ] {
        let (_m, fake, mut c) = cloud();
        fake.lock().unwrap().refuse = Some((status, code));
        let e = c.saves_list().unwrap_err();
        assert_eq!(e, Error::Code(ours), "{code}");
        assert_eq!(codes::known(e.code()), Some(ours));
        if ours != "server" {
            assert_ne!(e.text(), codes::text("server"), "{ours} has a text of its own");
        }
    }
    // (a save larger than the cloud takes never leaves the computer)
    let mut x = 0x9e37_79b9_7f4a_7c15u64;
    let huge: Vec<u8> = (0..(saves::MAX_BYTES as usize + 1024))
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    assert_eq!(compress(&huge), Err(Error::Code("too_large")));
}

#[test]
fn a_401_fetches_a_new_jwt_once_and_does_not_sign_out() {
    let m = Mock::start(|r| match r.path.as_str() {
        "/auth/token" => (200, vec![], json!({ "token": test_jwt("me", test_now() + 3600) })),
        _ => (401, vec![], json!({ "ok": false, "error": "unauthorized" })),
    });
    let mut c = Client::new(Endpoints::local(&m.base), Box::new(MemoryStore::with("tok.sig")));
    assert_eq!(c.saves_list(), Err(Error::Code("server")));
    assert_eq!(m.calls().iter().filter(|p| p.ends_with("/saves/list")).count(), 2);
    assert!(c.signed_in());
}

#[test]
fn without_a_network_the_cloud_saves_are_offline_and_nothing_else_happens() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut c = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}")), Box::new(MemoryStore::with("tok.sig")));
    let t = std::time::Instant::now();
    assert_eq!(c.saves_list(), Err(Error::Offline));
    assert_eq!(c.upload_save(None, "Alt", "PC", 0, &archive("x")), Err(SaveError::Failed(Error::Offline)));
    assert_eq!(c.download_save(1, None), Err(SaveError::Failed(Error::Offline)));
    // (Windows refuses a closed local port only after about 2 s of retries, Linux at once)
    assert!(t.elapsed().as_secs() < if cfg!(windows) { 15 } else { 5 });
    assert!(c.signed_in());
    // (signed out: nothing is sent at all)
    let mut out = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}")), Box::new(MemoryStore::new()));
    assert_eq!(out.saves_list(), Err(Error::Code("not_signed_in")));
}

#[test]
fn a_device_has_a_name() {
    let n = saves::device_name();
    assert!(!n.is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control));
}

//! The cloud saves against the tests' server: a fake of Supabase Storage's bucket `saves`
//! (`storage`), as docs/omsihub/ONLINE.md describes it - two slots, `limit`, `conflict` on another
//! base version, `meta.json` written after the file, the last 3 versions kept, downloads checked
//! against their sha256. No test reaches the real project.

use crate::mock::{Body, Mock, Req};
use crate::saves::{self, compress, decompress, sha256_hex, SaveError};
use crate::store::SecretStore;
use crate::tests::{client, refresh_route, signed_in, Shared};
use crate::{codes, Client, Endpoints, Error};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) type Raw = (u16, Vec<(&'static str, String)>, Body);

/// The bucket's files by path, and every answer of Storage this code instead (the error tests').
#[derive(Default)]
pub(crate) struct Bucket {
    pub files: BTreeMap<String, Vec<u8>>,
    pub refuse: Option<(&'static str, &'static str)>,
}

pub(crate) type Files = Arc<Mutex<Bucket>>;

pub(crate) fn raw_of(a: (u16, Vec<(&'static str, String)>, Value)) -> Raw {
    (a.0, a.1, Body::Json(a.2))
}

/// Storage's error: a 400 with the status inside, as it answers.
fn no(status: &str, error: &str) -> Raw {
    (400, vec![], Body::Json(json!({ "statusCode": status, "error": error, "message": "words not to be shown" })))
}

/// Storage's routes for the bucket `saves`, the player `me` (the tests' access token) in
/// `users/me/` only.
pub(crate) fn storage(files: &Files, r: &Req) -> Option<Raw> {
    let path = r.path.strip_prefix("/storage/v1")?;
    let mut b = files.lock().unwrap();
    if let Some((status, error)) = b.refuse {
        return Some(no(status, error));
    }
    if r.header("apikey").is_none() || !r.bearer().is_some_and(|t| t.ends_with(".sig")) {
        return Some(no("403", "InvalidJWT"));
    }
    let own = |p: &str| p.starts_with("users/me/slot1/") || p.starts_with("users/me/slot2/");
    if let Some(p) = path.strip_prefix("/object/authenticated/saves/") {
        return Some(match b.files.get(p) {
            Some(bytes) if own(p) => (200, vec![], Body::Raw(bytes.clone())),
            _ => no("404", "not_found"),
        });
    }
    if path == "/object/list/saves" && r.method == "POST" {
        let prefix = format!("{}/", r.body["prefix"].as_str().unwrap_or(""));
        let mut names: Vec<Value> = b.files.keys().filter_map(|k| k.strip_prefix(&prefix)).filter(|n| !n.contains('/')).map(|n| json!({ "name": n, "id": format!("id-{n}") })).collect();
        // (a folder below, listed with a null id)
        names.push(json!({ "name": "old", "id": null }));
        return Some((200, vec![], Body::Json(json!(names))));
    }
    if path == "/object/saves" && r.method == "DELETE" {
        let gone: Vec<Value> = r.body["prefixes"].as_array().unwrap().iter().filter_map(|p| p.as_str()).filter(|p| own(p)).filter_map(|p| b.files.remove(p).map(|_| json!({ "name": p }))).collect();
        return Some((200, vec![], Body::Json(json!(gone))));
    }
    if let Some(p) = path.strip_prefix("/object/saves/") {
        if r.method == "POST" {
            if !own(p) {
                return Some(no("403", "Unauthorized"));
            }
            if r.header("x-upsert") != Some("true") && b.files.contains_key(p) {
                return Some(no("409", "Duplicate"));
            }
            if r.raw.len() as u64 > saves::MAX_BYTES {
                return Some(no("413", "Payload too large"));
            }
            b.files.insert(p.to_string(), r.raw.clone());
            return Some((200, vec![], Body::Json(json!({ "Key": format!("saves/{p}"), "Id": "x" }))));
        }
    }
    Some(no("404", "not_found"))
}

fn cloud() -> (Mock, Files, Client) {
    let files: Files = Default::default();
    let f = files.clone();
    let m = Mock::start_raw(move |r| refresh_route(r).map(raw_of).or_else(|| storage(&f, r)).unwrap_or_else(|| raw_of((404, vec![], Value::Null))));
    let c = client(&m, &signed_in("r1"));
    (m, files, c)
}

fn names(files: &Files) -> Vec<String> {
    files.lock().unwrap().files.keys().cloned().collect()
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
fn a_save_goes_up_compressed_with_its_meta_after_it_and_comes_back_the_same() {
    let (m, files, mut c) = cloud();
    assert_eq!(c.saves_list().unwrap().free(), Some(1));
    let a = archive("first");
    let slot = c.upload_save(None, "Stadtbus Grundorf", "PC-1", 0, &a).unwrap();
    assert_eq!((slot.slot, slot.version, slot.name.as_str(), slot.device.as_str(), slot.empty), (1, 1, "Stadtbus Grundorf", "PC-1", false));
    let packed = compress(&a).unwrap();
    assert!(packed.bytes.len() * 4 < a.len(), "zstd: {} of {}", packed.bytes.len(), a.len());
    assert_eq!((slot.sha256.as_str(), slot.size), (packed.sha256.as_str(), packed.bytes.len() as u64));
    assert_eq!(compress(&a).unwrap(), packed, "the same save packs the same");
    assert!(slot.updated_at.as_deref().is_some_and(|t| t.parse::<u64>().is_ok_and(|s| s > 1_700_000_000)));
    // (the file went first, with the key, the access token and x-upsert; meta.json after it)
    let reqs = m.requests();
    let at = |p: &str| reqs.iter().position(|r| r.method == "POST" && r.path == format!("/storage/v1/object/saves/users/me/slot1/{p}")).unwrap();
    let file = &reqs[at("v1.omsave")];
    assert_eq!((file.header("content-type"), file.header("x-upsert"), file.header("apikey")), (Some("application/octet-stream"), Some("true"), Some("sb_publishable_test")));
    assert_eq!(file.raw, packed.bytes);
    assert!(at("v1.omsave") < at("meta.json"));
    let meta: Value = serde_json::from_slice(&files.lock().unwrap().files["users/me/slot1/meta.json"]).unwrap();
    assert_eq!((meta["name"].as_str(), meta["device"].as_str(), meta["versions"][0]["sha256"].as_str()), (Some("Stadtbus Grundorf"), Some("PC-1"), Some(packed.sha256.as_str())));
    // (down again: the same bytes)
    let (fetch, back) = c.download_save(1, None).unwrap();
    assert_eq!((fetch.version, fetch.name.as_str(), back), (1, "Stadtbus Grundorf", a.clone()));
    // (a second version on top of the first, the first still kept)
    let b = archive("second");
    assert_eq!(c.upload_save(Some(1), "Stadtbus Grundorf", "PC-1", 1, &b).unwrap().version, 2);
    assert_eq!(c.download_save(1, None).unwrap().1, b);
    assert_eq!(c.download_save(1, Some(1)).unwrap().1, a, "a kept version");
    let list = c.saves_list().unwrap();
    assert_eq!((list.max_slots, list.max_bytes, list.slots.len(), list.free()), (2, saves::MAX_BYTES, 2, Some(2)));
    assert_eq!((list.slot(1).unwrap().version, list.slot(1).unwrap().versions.clone()), (2, vec![1, 2]));
    assert!(list.slots[1].empty);
}

#[test]
fn the_last_three_versions_are_kept_and_the_older_ones_deleted() {
    let (_m, files, mut c) = cloud();
    for v in 0..5u64 {
        assert_eq!(c.upload_save(Some(1), "Stadtbus", "PC", v, &archive(&v.to_string())).unwrap().version, v + 1);
    }
    assert_eq!(names(&files), ["users/me/slot1/meta.json", "users/me/slot1/v3.omsave", "users/me/slot1/v4.omsave", "users/me/slot1/v5.omsave"]);
    assert_eq!(c.saves_list().unwrap().slot(1).unwrap().versions, [3, 4, 5]);
    assert_eq!(c.download_save(1, Some(3)).unwrap().1, archive("2"));
    assert_eq!(c.download_save(1, Some(2)), Err(SaveError::Failed(Error::Code("no_cloud_save"))), "no longer kept");
}

#[test]
fn a_damaged_download_is_refused_by_its_sha256_and_nothing_else_is_taken() {
    let (_m, files, mut c) = cloud();
    c.upload_save(None, "Alt", "PC", 0, &archive("x")).unwrap();
    // (a byte changed in the storage)
    files.lock().unwrap().files.get_mut("users/me/slot1/v1.omsave").unwrap()[5] ^= 0xff;
    assert_eq!(c.download_save(1, None), Err(SaveError::Failed(Error::Code("bad_save"))));
    // (a file that is not there any more)
    files.lock().unwrap().files.remove("users/me/slot1/v1.omsave");
    assert_eq!(c.download_save(1, None), Err(SaveError::Failed(Error::Code("no_cloud_save"))));
    assert_eq!(c.download_save(2, None), Err(SaveError::Failed(Error::Code("no_cloud_save"))), "an empty slot");
    // (not zstd at all)
    assert_eq!(decompress(b"not a save"), Err(Error::Code("bad_save")));
    // (what is sent is what the begin promised in meta.json)
    let packed = compress(&archive("y")).unwrap();
    let begun = c.saves_upload_begin(Some(1), "Alt", &packed, 1, "PC").unwrap();
    let other = compress(&archive("z")).unwrap();
    assert_eq!(c.transfer().put(&begun, &other), Err(Error::Code("bad_input")));
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn a_file_sent_but_not_committed_never_replaces_the_good_save() {
    let (_m, files, mut c) = cloud();
    c.upload_save(None, "Alt", "PC", 0, &archive("good")).unwrap();
    let packed = compress(&archive("half")).unwrap();
    let begun = c.saves_upload_begin(Some(1), "Alt", &packed, 1, "PC").unwrap();
    c.transfer().put(&begun, &packed).unwrap();
    // (the program stopped here: the file is there, meta.json still says version 1)
    assert!(names(&files).contains(&"users/me/slot1/v2.omsave".to_string()));
    assert_eq!(c.download_save(1, None).unwrap().1, archive("good"));
    // (the next upload from version 1 goes over the stray file; a commit said twice is one)
    let s = c.upload_save(Some(1), "Alt", "PC", 1, &archive("next")).unwrap();
    let packed = compress(&archive("next")).unwrap();
    let again = c.saves_upload_begin(Some(1), "Alt", &packed, 1, "PC").unwrap_err();
    assert!(matches!(again, SaveError::Conflict(ref now) if now.version == 2));
    assert_eq!(s.version, 2);
    assert_eq!(c.download_save(1, None).unwrap().1, archive("next"));
}

#[test]
fn another_base_version_is_a_conflict_with_the_slot_as_it_is_now() {
    let (_m, _files, mut c) = cloud();
    c.upload_save(None, "Stadtbus", "Laptop", 0, &archive("a")).unwrap();
    c.upload_save(Some(1), "Stadtbus", "Laptop", 1, &archive("b")).unwrap();
    // (this PC last synced version 1; the laptop has uploaded version 2 since)
    match c.upload_save(Some(1), "Stadtbus", "Desktop", 1, &archive("c")) {
        Err(SaveError::Conflict(now)) => assert_eq!((now.slot, now.version, now.device.as_str(), now.empty), (1, 2, "Laptop", false)),
        other => panic!("{other:?}"),
    }
    // (a company not in the cloud yet is base 0: into a taken slot, a conflict too)
    assert!(matches!(c.upload_save(Some(1), "Other", "Desktop", 0, &archive("d")), Err(SaveError::Conflict(_))));
    // (a slot emptied on another computer: the conflict says so)
    assert!(matches!(c.upload_save(Some(2), "Gone", "Desktop", 3, &archive("e")), Err(SaveError::Conflict(ref s)) if s.empty && s.slot == 2));
    assert_eq!(SaveError::Conflict(Default::default()).error(), Error::Code("conflict"));
    // (another computer committed between this one's begin and its commit)
    let packed = compress(&archive("late")).unwrap();
    let begun = c.saves_upload_begin(Some(1), "Stadtbus", &packed, 2, "Desktop").unwrap();
    c.upload_save(Some(1), "Stadtbus", "Laptop", 2, &archive("first")).unwrap();
    c.transfer().put(&begun, &packed).unwrap();
    assert!(matches!(c.saves_upload_commit(&begun), Err(SaveError::Conflict(ref s)) if s.version == 3 && s.device == "Laptop"));
    // (the player kept this PC's: on top of the version now)
    assert_eq!(c.upload_save(Some(1), "Stadtbus", "Desktop", 3, &archive("c")).unwrap().version, 4);
}

#[test]
fn the_third_company_meets_the_limit_and_a_slot_can_be_emptied_for_it() {
    let (_m, files, mut c) = cloud();
    c.upload_save(None, "One", "PC", 0, &archive("1")).unwrap();
    assert_eq!(c.upload_save(None, "Two", "PC", 0, &archive("2")).unwrap().slot, 2);
    let e = c.upload_save(None, "Three", "PC", 0, &archive("3")).unwrap_err();
    assert_eq!(e, SaveError::Failed(Error::Code("limit")));
    assert!(e.error().text().contains("keep this company local"));
    assert_eq!(c.saves_list().unwrap().free(), None);
    // (replace one: its slot emptied, then the third goes into it)
    assert_eq!(c.saves_delete(2), Ok(2));
    let s = c.upload_save(Some(2), "Three", "PC", 0, &archive("3")).unwrap();
    assert_eq!((s.slot, s.name.as_str()), (2, "Three"));
    assert_eq!(c.saves_delete(3), Err(Error::Code("bad_input")), "slot 1 or 2 only, checked here");
    files.lock().unwrap().files.insert("users/other/slot1/v1.omsave".into(), b"x".to_vec());
    assert_eq!(c.saves_delete_all(), Ok(4));
    assert_eq!(names(&files), ["users/other/slot1/v1.omsave"], "the player's own files only");
    assert_eq!(c.saves_list().unwrap().free(), Some(1));
}

#[test]
fn every_refusal_of_the_storage_is_one_of_our_codes_with_a_text() {
    for (status, error, ours) in [("413", "Payload too large", "too_large"), ("429", "too many", "too_many"), ("403", "Unauthorized", "not_allowed"), ("500", "internal", "server"), ("422", "InvalidKey", "bad_input")] {
        let (_m, files, mut c) = cloud();
        files.lock().unwrap().refuse = Some((status, error));
        let e = c.saves_list().unwrap_err();
        assert_eq!(e, Error::Code(ours), "{status}");
        assert_eq!(codes::known(e.code()), Some(ours));
    }
    for ours in ["limit", "conflict", "too_large", "no_cloud_save", "upload_failed", "bad_save"] {
        assert_ne!(codes::text(ours), codes::text("server"), "{ours} has a text of its own");
    }
    // (the file refused by the storage: `upload_failed`; too large: `too_large`)
    let (m, files, mut c) = cloud();
    let packed = compress(&archive("x")).unwrap();
    let begun = c.saves_upload_begin(None, "Alt", &packed, 0, "PC").unwrap();
    files.lock().unwrap().refuse = Some(("400", "InvalidRequest"));
    assert_eq!(c.transfer().put(&begun, &packed), Err(Error::Code("upload_failed")));
    files.lock().unwrap().refuse = Some(("413", "Payload too large"));
    assert_eq!(c.transfer().put(&begun, &packed), Err(Error::Code("too_large")));
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
    let n = m.calls().len();
    let big = saves::Packed { sha256: sha256_hex(&huge), bytes: huge };
    assert_eq!(c.saves_upload_begin(None, "Big", &big, 0, ""), Err(SaveError::Failed(Error::Code("too_large"))));
    assert_eq!(c.saves_upload_begin(None, "  ", &packed, 0, ""), Err(SaveError::Failed(Error::Code("bad_input"))));
    assert_eq!(m.calls().len(), n, "refused before anything is sent");
}

#[test]
fn an_access_token_storage_refuses_is_renewed_once_and_a_second_refusal_signs_out() {
    let files: Files = Default::default();
    let f = files.clone();
    // (Storage takes no token at all: every one is "InvalidJWT")
    let m = Mock::start_raw(move |r| refresh_route(r).map(raw_of).or_else(|| r.path.starts_with("/storage/").then(|| no("403", "InvalidJWT"))).or_else(|| storage(&f, r)).unwrap());
    let store = signed_in("r1");
    let mut c = client(&m, &store);
    assert_eq!(c.saves_list(), Err(Error::SignedOut));
    assert_eq!(m.calls().iter().filter(|p| p.ends_with("slot1/meta.json")).count(), 2);
    assert_eq!(m.calls().iter().filter(|p| p.contains("grant_type=refresh_token")).count(), 2);
    assert!(!c.signed_in() && store.load().is_none());
}

#[test]
fn without_a_network_the_cloud_saves_are_offline_and_nothing_else_happens() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let store = signed_in("r1");
    let mut c = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}"), "k").unwrap(), Box::new(store.clone()));
    let t = std::time::Instant::now();
    assert_eq!(c.saves_list(), Err(Error::Offline));
    assert_eq!(c.upload_save(None, "Alt", "PC", 0, &archive("x")), Err(SaveError::Failed(Error::Offline)));
    assert_eq!(c.download_save(1, None), Err(SaveError::Failed(Error::Offline)));
    // (Windows refuses a closed local port only after about 2 s of retries, Linux at once)
    assert!(t.elapsed().as_secs() < if cfg!(windows) { 15 } else { 5 });
    assert!(c.signed_in());
    // (signed out: nothing is sent at all)
    let mut out = Client::new(Endpoints::local(&format!("http://127.0.0.1:{port}"), "k").unwrap(), Box::new(Shared::default()));
    assert_eq!(out.saves_list(), Err(Error::Code("not_signed_in")));
}

#[test]
fn a_device_has_a_name() {
    let n = saves::device_name();
    assert!(!n.is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control));
}

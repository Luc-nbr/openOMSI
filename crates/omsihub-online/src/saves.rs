//! The cloud saves (docs/omsihub/ONLINE.md, "Cloud saves"): the Neon Function `saves`, each
//! route a `POST` with the same JWT as the Data API, and the save's bytes moved straight to and
//! from the private storage by the presigned URLs it hands out - never through the function,
//! and with no token of ours.
//!
//! * a save is compressed with zstd here (`compress`); its sha256 goes to `upload_begin`, the
//!   `PUT` carries exactly the headers the function returned (they are signed), and
//!   `upload_commit` gets the sha256 again;
//! * a download's sha256 is checked before it is decompressed (`decompress`, with a ceiling a
//!   damaged or hostile file cannot pass);
//! * every refusal is one of our codes (`codes::of_saves`); a conflict carries the slot as it is
//!   now, for the player to choose which copy to keep.
//!
//! The launcher's side (which company goes into which slot, when, and the questions asked) is
//! `omsi-launcher-api`'s `omsihub/cloud.rs`.

use crate::client::Client;
use crate::codes::{self, Error};
use crate::http::Http;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The cloud slots an account has.
pub const MAX_SLOTS: u8 = 2;
/// The most a save may be, compressed (the function's own limit).
pub const MAX_BYTES: u64 = 25 * 1024 * 1024;
/// The most a save may unpack to.
pub const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
/// zstd's level: a company's JSON shrinks to a tenth or less, in well under a second.
const LEVEL: i32 = 12;
/// A company's or a device's name, at most (the function refuses longer ones).
const NAME_MAX: usize = 64;

/// A cloud slot, as `list` and `upload_commit` answer it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slot {
    pub slot: u8,
    pub empty: bool,
    /// The company's name.
    pub name: String,
    pub version: u64,
    /// Compressed.
    pub size: u64,
    pub sha256: String,
    /// When it was committed, as the function says it (RFC 3339, or Unix milliseconds).
    pub updated_at: Option<String>,
    /// The computer that uploaded it.
    pub device: String,
    /// The versions kept (the last 3).
    pub versions: Vec<u64>,
}

/// What `list` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloud {
    pub max_slots: u8,
    pub max_bytes: u64,
    /// Every slot, empty ones too, by number.
    pub slots: Vec<Slot>,
}

impl Cloud {
    pub fn slot(&self, n: u8) -> Option<&Slot> {
        self.slots.iter().find(|s| s.slot == n && !s.empty)
    }

    /// The first empty slot, if any.
    pub fn free(&self) -> Option<u8> {
        (1..=self.max_slots).find(|n| self.slot(*n).is_none())
    }
}

/// An upload begun: where to `PUT` it and with which headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Begun {
    pub slot: u8,
    pub version: u64,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// A download handed out: where to `GET` it and what it must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub slot: u8,
    pub version: u64,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}

/// A save compressed, as it goes up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packed {
    pub bytes: Vec<u8>,
    pub sha256: String,
}

/// Why a save did not go up or come down: one of our codes, or a conflict with the slot as it
/// is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Failed(Error),
    Conflict(Slot),
}

impl SaveError {
    pub fn error(&self) -> Error {
        match self {
            SaveError::Failed(e) => *e,
            SaveError::Conflict(_) => Error::Code("conflict"),
        }
    }
}

impl From<Error> for SaveError {
    fn from(e: Error) -> Self {
        SaveError::Failed(e)
    }
}

/// A sha256 in lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A save compressed with zstd (the same bytes give the same file), refused above `MAX_BYTES`.
pub fn compress(archive: &[u8]) -> Result<Packed, Error> {
    let bytes = zstd::bulk::compress(archive, LEVEL).map_err(|_| Error::Code("server"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Code("too_large"));
    }
    Ok(Packed { sha256: sha256_hex(&bytes), bytes })
}

/// A downloaded save decompressed (at most `MAX_UNPACKED`; anything else is `bad_save`).
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    let d = zstd::stream::read::Decoder::new(bytes).map_err(|_| Error::Code("bad_save"))?;
    let mut out = Vec::new();
    d.take(MAX_UNPACKED + 1).read_to_end(&mut out).map_err(|_| Error::Code("bad_save"))?;
    if out.len() as u64 > MAX_UNPACKED {
        return Err(Error::Code("bad_save"));
    }
    Ok(out)
}

/// A name as the function takes it: no control characters, at most 64 characters.
fn name_of(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(NAME_MAX).collect::<String>().trim().to_string()
}

fn slot_number(n: u8) -> Result<u8, Error> {
    (1..=MAX_SLOTS).contains(&n).then_some(n).ok_or(Error::Code("bad_input"))
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(x) => x.to_string(),
    }
}

fn number(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0)
}

/// A slot of an answer.
fn slot_of(v: &Value) -> Slot {
    let version = number(v, "version");
    Slot {
        slot: number(v, "slot").min(u8::MAX as u64) as u8,
        empty: v.get("empty").and_then(Value::as_bool).unwrap_or(version == 0),
        name: name_of(&text(v, "name")),
        version,
        size: number(v, "size"),
        sha256: text(v, "sha256").to_ascii_lowercase(),
        updated_at: Some(text(v, "updated_at")).filter(|t| !t.is_empty()),
        device: name_of(&text(v, "device")),
        versions: v.get("versions").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().or_else(|| x.get("version").and_then(Value::as_u64))).collect()).unwrap_or_default(),
    }
}

fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The headers of `upload_begin`'s answer, as they are to be sent (a name that is no header
/// name, or a value with a line break, is refused rather than sent otherwise).
fn headers_of(v: &Value) -> Result<Vec<(String, String)>, Error> {
    let Some(map) = v.get("headers").and_then(Value::as_object) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for (k, v) in map {
        let value = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(Error::Code("server")),
        };
        let name_ok = !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b));
        if !name_ok || value.contains(['\r', '\n']) {
            return Err(Error::Code("server"));
        }
        // (the connection's own; ureq sets it from the URL)
        if !k.eq_ignore_ascii_case("host") {
            out.push((k.clone(), value));
        }
    }
    Ok(out)
}

/// The presigned URLs' transfers, without the client: a big save moves without holding it
/// (the friends' calls go on meanwhile).
#[derive(Clone)]
pub struct Transfer {
    http: Http,
}

impl Transfer {
    /// The save `PUT` to the URL `upload_begin` handed out.
    pub fn put(&self, begun: &Begun, packed: &Packed) -> Result<(), Error> {
        // (the length signed is the length sent: another one the storage refuses anyway)
        if let Some((_, len)) = begun.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")) {
            if len.trim().parse::<usize>().ok() != Some(packed.bytes.len()) {
                return Err(Error::Code("mismatch"));
            }
        }
        match self.http.put_bytes(&begun.url, &begun.headers, &packed.bytes)? {
            200..=299 => Ok(()),
            _ => Err(Error::Code("upload_failed")),
        }
    }

    /// The save of `download`'s URL, its size and sha256 checked; still compressed.
    pub fn get(&self, fetch: &Fetch) -> Result<Vec<u8>, Error> {
        let (status, bytes) = self.http.get_bytes(&fetch.url, MAX_BYTES)?;
        match status {
            200..=299 => {}
            403 | 404 => return Err(Error::Code("no_cloud_save")),
            _ => return Err(Error::Code("server")),
        }
        if (fetch.size > 0 && bytes.len() as u64 != fetch.size) || sha256_hex(&bytes) != fetch.sha256 {
            return Err(Error::Code("bad_save"));
        }
        Ok(bytes)
    }
}

impl Client {
    /// A route of the function: the JWT, a new one once on a 401. A second 401 is the
    /// function's refusal, not the session's (that one `/token` would say): no signing out.
    fn saves_route(&mut self, route: &str, body: Value) -> Result<Value, SaveError> {
        let url = format!("{}/{route}", self.ep.saves);
        for again in [false, true] {
            let jwt = self.jwt()?;
            let r = self.send("POST", &url, Some(&jwt), Some(&body))?;
            if r.status == 401 {
                self.jwt = None;
                if again {
                    return Err(Error::Code("server").into());
                }
                continue;
            }
            if r.ok() && r.body.get("ok").and_then(Value::as_bool) != Some(false) {
                return Ok(r.body);
            }
            if r.body.get("error").and_then(Value::as_str) == Some("conflict") {
                return Err(SaveError::Conflict(slot_of(r.body.get("current").unwrap_or(&Value::Null))));
            }
            return Err(codes::of_saves(r.status, &r.body).into());
        }
        Err(Error::Code("server").into())
    }

    /// The transfers' half, for the launcher's own thread.
    pub fn transfer(&self) -> Transfer {
        Transfer { http: self.http.clone() }
    }

    /// `list`: the slots, empty ones too.
    pub fn saves_list(&mut self) -> Result<Cloud, Error> {
        let v = self.saves_route("list", json!({})).map_err(|e| e.error())?;
        let max_slots = (number(&v, "max_slots") as u8).clamp(1, MAX_SLOTS);
        let mut slots: Vec<Slot> = v.get("slots").and_then(Value::as_array).map(|a| a.iter().map(slot_of).filter(|s| (1..=max_slots).contains(&s.slot)).collect()).unwrap_or_default();
        for n in 1..=max_slots {
            if !slots.iter().any(|s| s.slot == n) {
                slots.push(Slot { slot: n, empty: true, ..Default::default() });
            }
        }
        slots.sort_by_key(|s| s.slot);
        let max_bytes = Some(number(&v, "max_bytes")).filter(|b| *b > 0).unwrap_or(MAX_BYTES);
        Ok(Cloud { max_slots, max_bytes, slots })
    }

    /// `upload_begin`: `slot` None takes the first free one (both taken: `limit`);
    /// `base_version` is the version last synced (0: none), another than the slot's is a
    /// conflict.
    pub fn saves_upload_begin(&mut self, slot: Option<u8>, name: &str, packed: &Packed, base_version: u64, device: &str) -> Result<Begun, SaveError> {
        let name = name_of(name);
        if name.is_empty() {
            return Err(Error::Code("bad_input").into());
        }
        let mut body = json!({ "name": name, "size": packed.bytes.len(), "sha256": packed.sha256, "base_version": base_version });
        if let Some(n) = slot {
            body["slot"] = json!(slot_number(n)?);
        }
        let device = name_of(device);
        if !device.is_empty() {
            body["device"] = json!(device);
        }
        let v = self.saves_route("upload_begin", body)?;
        let url = text(&v, "url");
        if url.is_empty() || (v.get("method").is_some() && !text(&v, "method").eq_ignore_ascii_case("PUT")) {
            return Err(Error::Code("server").into());
        }
        Ok(Begun { slot: number(&v, "slot") as u8, version: number(&v, "version"), url, headers: headers_of(&v)? })
    }

    /// `upload_commit`, with the sha256 (another device's upload over the pending one is caught
    /// so); the slot as it is now.
    pub fn saves_upload_commit(&mut self, slot: u8, version: u64, sha256: &str) -> Result<Slot, SaveError> {
        let v = self.saves_route("upload_commit", json!({ "slot": slot_number(slot)?, "version": version, "sha256": sha256 }))?;
        Ok(slot_of(&v))
    }

    /// `download`: the current version (None) or a kept one.
    pub fn saves_download(&mut self, slot: u8, version: Option<u64>) -> Result<Fetch, SaveError> {
        let mut body = json!({ "slot": slot_number(slot)? });
        if let Some(v) = version {
            body["version"] = json!(v);
        }
        let v = self.saves_route("download", body)?;
        let fetch = Fetch { slot: number(&v, "slot") as u8, version: number(&v, "version"), name: name_of(&text(&v, "name")), size: number(&v, "size"), sha256: text(&v, "sha256").to_ascii_lowercase(), url: text(&v, "url") };
        if fetch.url.is_empty() || !is_sha(&fetch.sha256) {
            return Err(Error::Code("server").into());
        }
        Ok(fetch)
    }

    /// `delete`: the slot emptied; how many files went.
    pub fn saves_delete(&mut self, slot: u8) -> Result<u64, Error> {
        let v = self.saves_route("delete", json!({ "slot": slot_number(slot)? })).map_err(|e| e.error())?;
        Ok(number(&v, "deleted"))
    }

    /// `delete_all`: every cloud save of the player; how many files went.
    pub fn saves_delete_all(&mut self) -> Result<u64, Error> {
        let v = self.saves_route("delete_all", json!({})).map_err(|e| e.error())?;
        Ok(number(&v, "deleted"))
    }

    /// A save uploaded whole: compressed, begun, `PUT`, committed (once more where the commit's
    /// answer was lost). The slot as committed.
    pub fn upload_save(&mut self, slot: Option<u8>, name: &str, device: &str, base_version: u64, archive: &[u8]) -> Result<Slot, SaveError> {
        let packed = compress(archive)?;
        let begun = self.saves_upload_begin(slot, name, &packed, base_version, device)?;
        self.transfer().put(&begun, &packed)?;
        match self.saves_upload_commit(begun.slot, begun.version, &packed.sha256) {
            Err(SaveError::Failed(Error::Offline)) => self.saves_upload_commit(begun.slot, begun.version, &packed.sha256),
            r => r,
        }
    }

    /// A save downloaded whole: handed out, fetched, its sha256 checked, decompressed. What was
    /// fetched and the save as it was packed.
    pub fn download_save(&mut self, slot: u8, version: Option<u64>) -> Result<(Fetch, Vec<u8>), SaveError> {
        let fetch = self.saves_download(slot, version)?;
        let bytes = self.transfer().get(&fetch)?;
        Ok((fetch, decompress(&bytes)?))
    }
}

/// This computer's name, for the slot's "uploaded from" (the OS's, else "PC").
pub fn device_name() -> String {
    let from_env = ["COMPUTERNAME", "HOSTNAME"].iter().find_map(|k| std::env::var(k).ok());
    let from_file = || std::fs::read_to_string("/etc/hostname").ok();
    let name = from_env.or_else(from_file).map(|n| name_of(&n)).unwrap_or_default();
    if name.is_empty() {
        "PC".into()
    } else {
        name
    }
}

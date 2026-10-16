//! The cloud saves (docs/omsihub/ONLINE.md, "Cloud saves"): up to two companies a player keeps
//! online, as files in Supabase Storage's private bucket `saves`, under `users/<user id>/slot1|slot2/`,
//! which only their owner may read or write (online/supabase/schema.sql). Local saves are not
//! limited; only the cloud has two slots.
//!
//! * a slot holds `v<version>.omsave`, the save compressed with zstd here (`compress`), its last
//!   `KEEP` versions, and `meta.json`, what the slot holds now (its name, the computer it came from
//!   and each kept version's size, sha256 and time);
//! * an upload is begun (`saves_upload_begin`: the slot chosen, `limit` with both taken, and its
//!   `base_version` held against `meta.json`, another one a conflict with the slot as it is now),
//!   the file sent (`Transfer::put`, `x-upsert`), then committed (`saves_upload_commit`: `meta.json`
//!   written after the file, so a half-finished upload never replaces a good save, and the versions
//!   past the kept ones deleted);
//! * a download's size and sha256 are checked against `meta.json` before it is decompressed
//!   (`decompress`, with a ceiling a damaged or hostile file cannot pass);
//! * every refusal is one of our codes (`codes::of_storage`), Storage's own words never shown.
//!
//! The transfers (`Transfer`) need no `&mut Client`: a big save moves with the access token the
//! begin fetched (good for a minute at least as it starts) while the friends' calls go on.
//! The launcher's side (which company goes into which slot, when, and the questions asked) is
//! `omsi-launcher-api`'s `omsihub/cloud.rs`.

use crate::client::Client;
use crate::codes::{self, Error};
use crate::http::{Body, Http};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The bucket.
const BUCKET: &str = "saves";
/// The cloud slots an account has.
pub const MAX_SLOTS: u8 = 2;
/// The most a save may be, compressed (the bucket's own limit).
pub const MAX_BYTES: u64 = 25 * 1024 * 1024;
/// The most a save may unpack to.
pub const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
/// The versions of a company kept in its slot.
pub const KEEP: usize = 3;
/// zstd's level: a company's JSON shrinks to a tenth or less, in well under a second.
const LEVEL: i32 = 12;
/// A company's or a device's name, at most.
const NAME_MAX: usize = 64;

/// A cloud slot, as `saves_list` and `saves_upload_commit` answer it.
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
    /// When the current version was uploaded (Unix seconds, as a text).
    pub updated_at: Option<String>,
    /// The computer that uploaded it.
    pub device: String,
    /// The versions kept (the last 3), the oldest first.
    pub versions: Vec<u64>,
}

/// What `saves_list` answers.
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

/// A version kept in a slot (`meta.json`'s).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    version: u64,
    size: u64,
    sha256: String,
    /// When it was uploaded (seconds since 1970).
    at: u64,
}

/// A slot's `meta.json`: the company in it and its versions, the newest last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    slot: u8,
    name: String,
    /// The computer it was uploaded from.
    #[serde(default)]
    device: String,
    versions: Vec<Kept>,
}

impl Meta {
    fn version(&self) -> u64 {
        self.versions.last().map_or(0, |k| k.version)
    }

    fn slot(&self) -> Slot {
        let now = self.versions.last().cloned().unwrap_or_default();
        Slot {
            slot: self.slot,
            empty: self.versions.is_empty(),
            name: name_of(&self.name),
            version: now.version,
            size: now.size,
            sha256: now.sha256.to_ascii_lowercase(),
            updated_at: (now.at > 0).then(|| now.at.to_string()),
            device: name_of(&self.device),
            versions: self.versions.iter().map(|k| k.version).collect(),
        }
    }
}

/// An upload begun: the slot and version it goes to, and what the commit writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Begun {
    pub slot: u8,
    pub version: u64,
    name: String,
    device: String,
    size: u64,
    sha256: String,
    url: String,
    token: String,
}

/// A download handed out: what it must be, and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub slot: u8,
    pub version: u64,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    url: String,
    token: String,
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

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
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

/// A name as it is kept: no control characters, at most 64 characters.
fn name_of(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(NAME_MAX).collect::<String>().trim().to_string()
}

fn slot_number(n: u8) -> Result<u8, Error> {
    (1..=MAX_SLOTS).contains(&n).then_some(n).ok_or(Error::Code("bad_input"))
}

fn empty(n: u8) -> Slot {
    Slot { slot: n, empty: true, ..Default::default() }
}

/// The transfers of a save's file, without the client (a big save moves without holding it).
#[derive(Clone)]
pub struct Transfer {
    http: Http,
    key: String,
}

impl Transfer {
    /// The file sent to where the begin said (`POST /object/saves/<path>`, `x-upsert`).
    pub fn put(&self, begun: &Begun, packed: &Packed) -> Result<(), Error> {
        // (what the commit promises in meta.json is what is sent)
        if packed.bytes.len() as u64 != begun.size || packed.sha256 != begun.sha256 {
            return Err(Error::Code("bad_input"));
        }
        let r = self.http.send("POST", &begun.url, &self.key, Some(&begun.token), Body::Bytes(&packed.bytes, "application/octet-stream"), &[("x-upsert", "true"), ("cache-control", "no-cache")], false, false)?;
        if r.ok() {
            return Ok(());
        }
        Err(match codes::storage_status(r.status, &r.body) {
            413 => Error::Code("too_large"),
            429 => Error::Code("too_many"),
            _ => Error::Code("upload_failed"),
        })
    }

    /// The file of `saves_download`, its size and sha256 checked; still compressed.
    pub fn get(&self, fetch: &Fetch) -> Result<Vec<u8>, Error> {
        let r = self.http.send("GET", &fetch.url, &self.key, Some(&fetch.token), Body::None, &[], true, false)?;
        if !r.ok() {
            return Err(match codes::storage_status(r.status, &r.body) {
                400 | 403 | 404 => Error::Code("no_cloud_save"),
                429 => Error::Code("too_many"),
                _ => Error::Code("server"),
            });
        }
        if r.bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Code("too_large"));
        }
        if r.bytes.len() as u64 != fetch.size || sha256_hex(&r.bytes) != fetch.sha256 {
            return Err(Error::Code("bad_save"));
        }
        Ok(r.bytes)
    }
}

impl Client {
    /// A slot's folder, `users/<user id>/slot<n>`.
    fn folder(&mut self, slot: u8) -> Result<String, Error> {
        let slot = slot_number(slot)?;
        self.access_token()?;
        let me = self.user_id().ok_or(Error::Code("server"))?;
        Ok(format!("users/{me}/slot{slot}"))
    }

    /// A file's address in the bucket (`/object/saves/<path>`, `/object/authenticated/...` to read).
    fn object(&self, path: &str, read: bool) -> String {
        self.ep.storage(&format!("/object/{}{BUCKET}/{path}", if read { "authenticated/" } else { "" }))
    }

    /// A slot's `meta.json`, None when it is empty.
    fn meta(&mut self, slot: u8) -> Result<Option<Meta>, Error> {
        let path = format!("{}/meta.json", self.folder(slot)?);
        let r = self.authed("GET", &self.object(&path, true), Body::None, &[], true)?;
        if !r.ok() {
            return match codes::storage_status(r.status, &r.body) {
                // (Storage answers a file that is not there as 400 with a 404 inside)
                404 => Ok(None),
                _ => Err(codes::of_storage(r.status, &r.body)),
            };
        }
        let mut m: Meta = serde_json::from_slice(&r.bytes).map_err(|_| Error::Code("server"))?;
        m.slot = slot;
        Ok(Some(m).filter(|m| !m.versions.is_empty()))
    }

    /// Files of the bucket deleted (`DELETE /object/saves`, `{prefixes}`); how many went.
    fn remove(&mut self, paths: &[String]) -> Result<u64, Error> {
        if paths.is_empty() {
            return Ok(0);
        }
        let r = self.authed("DELETE", &self.ep.storage(&format!("/object/{BUCKET}")), Body::Json(&json!({ "prefixes": paths })), &[], false)?;
        if r.ok() {
            Ok(r.body.as_array().map_or(paths.len(), Vec::len) as u64)
        } else {
            Err(codes::of_storage(r.status, &r.body))
        }
    }

    /// The transfers' half, for the launcher's own thread.
    pub fn transfer(&self) -> Transfer {
        Transfer { http: self.http.clone(), key: self.ep.key.clone() }
    }

    /// The slots, empty ones too.
    pub fn saves_list(&mut self) -> Result<Cloud, Error> {
        let mut slots = Vec::new();
        for n in 1..=MAX_SLOTS {
            slots.push(self.meta(n)?.map_or_else(|| empty(n), |m| m.slot()));
        }
        Ok(Cloud { max_slots: MAX_SLOTS, max_bytes: MAX_BYTES, slots })
    }

    /// An upload begun: `slot` None takes the first free one (both taken: `limit`);
    /// `base_version` is the version last synced (0: none), another than the slot's is a
    /// conflict.
    pub fn saves_upload_begin(&mut self, slot: Option<u8>, name: &str, packed: &Packed, base_version: u64, device: &str) -> Result<Begun, SaveError> {
        let (name, device) = (name_of(name), name_of(device));
        if name.is_empty() {
            return Err(Error::Code("bad_input").into());
        }
        if packed.bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Code("too_large").into());
        }
        let (slot, now) = match slot {
            Some(n) => (slot_number(n)?, self.meta(n)?),
            None => {
                let mut free = None;
                for n in 1..=MAX_SLOTS {
                    if self.meta(n)?.is_none() {
                        free = Some(n);
                        break;
                    }
                }
                (free.ok_or(Error::Code("limit"))?, None)
            }
        };
        let current = now.as_ref().map_or(0, Meta::version);
        if current != base_version {
            return Err(SaveError::Conflict(now.map_or_else(|| empty(slot), |m| m.slot())));
        }
        let version = current + 1;
        let folder = self.folder(slot)?;
        let url = self.object(&format!("{folder}/v{version}.omsave"), false);
        let token = self.access_token()?;
        Ok(Begun { slot, version, name, device, size: packed.bytes.len() as u64, sha256: packed.sha256.clone(), url, token })
    }

    /// The upload committed: `meta.json` written with the new version (unless another computer
    /// committed one since: a conflict), the versions past the kept ones deleted. The slot as it is
    /// now. Committed already (an answer lost), it is the same again.
    pub fn saves_upload_commit(&mut self, begun: &Begun) -> Result<Slot, SaveError> {
        let now = self.meta(begun.slot)?;
        if let Some(m) = &now {
            if m.versions.last().is_some_and(|k| k.version == begun.version && k.sha256 == begun.sha256) {
                return Ok(m.slot());
            }
        }
        if now.as_ref().map_or(0, Meta::version) + 1 != begun.version {
            return Err(SaveError::Conflict(now.map_or_else(|| empty(begun.slot), |m| m.slot())));
        }
        let folder = self.folder(begun.slot)?;
        let mut versions = now.map(|m| m.versions).unwrap_or_default();
        versions.push(Kept { version: begun.version, size: begun.size, sha256: begun.sha256.clone(), at: unix_now() });
        let drop = versions.len().saturating_sub(KEEP);
        let old: Vec<String> = versions.drain(..drop).map(|k| format!("{folder}/v{}.omsave", k.version)).collect();
        let meta = Meta { slot: begun.slot, name: begun.name.clone(), device: begun.device.clone(), versions };
        let text = serde_json::to_vec(&meta).map_err(|_| Error::Code("server"))?;
        let r = self.authed("POST", &self.object(&format!("{folder}/meta.json"), false), Body::Bytes(&text, "application/json"), &[("x-upsert", "true"), ("cache-control", "no-cache")], false)?;
        if !r.ok() {
            return Err(codes::of_storage(r.status, &r.body).into());
        }
        // (the versions past the kept ones; one left behind goes with the next upload's)
        let _ = self.remove(&old);
        Ok(meta.slot())
    }

    /// A download handed out: the current version (None) or a kept one.
    pub fn saves_download(&mut self, slot: u8, version: Option<u64>) -> Result<Fetch, SaveError> {
        let meta = self.meta(slot)?.ok_or(Error::Code("no_cloud_save"))?;
        let want = version.unwrap_or_else(|| meta.version());
        let kept = meta.versions.iter().find(|k| k.version == want).cloned().ok_or(Error::Code("no_cloud_save"))?;
        let folder = self.folder(slot)?;
        let url = self.object(&format!("{folder}/v{want}.omsave"), true);
        let token = self.access_token()?;
        Ok(Fetch { slot, version: want, name: name_of(&meta.name), size: kept.size, sha256: kept.sha256.to_ascii_lowercase(), url, token })
    }

    /// A slot emptied: every file in it (`POST /object/list/saves`, then deleted); how many went.
    pub fn saves_delete(&mut self, slot: u8) -> Result<u64, Error> {
        let folder = self.folder(slot)?;
        let r = self.authed("POST", &self.ep.storage(&format!("/object/list/{BUCKET}")), Body::Json(&json!({ "prefix": folder, "limit": 1000, "offset": 0 })), &[], false)?;
        if !r.ok() {
            return Err(codes::of_storage(r.status, &r.body));
        }
        // (a folder is listed with a null id; only files are deleted)
        let files: Vec<String> = r.body.as_array().map(|a| a.iter().filter(|f| !f.get("id").unwrap_or(&Value::Null).is_null()).filter_map(|f| f.get("name").and_then(Value::as_str)).map(|n| format!("{folder}/{n}")).collect()).unwrap_or_default();
        self.remove(&files)
    }

    /// Every cloud save of the player deleted (before the account is); how many files went.
    pub fn saves_delete_all(&mut self) -> Result<u64, Error> {
        let mut n = 0;
        for slot in 1..=MAX_SLOTS {
            n += self.saves_delete(slot)?;
        }
        Ok(n)
    }

    /// A save uploaded whole: compressed, begun, sent, committed (once more where the commit's
    /// answer was lost). The slot as committed.
    pub fn upload_save(&mut self, slot: Option<u8>, name: &str, device: &str, base_version: u64, archive: &[u8]) -> Result<Slot, SaveError> {
        let packed = compress(archive)?;
        let begun = self.saves_upload_begin(slot, name, &packed, base_version, device)?;
        self.transfer().put(&begun, &packed)?;
        match self.saves_upload_commit(&begun) {
            Err(SaveError::Failed(Error::Offline)) => self.saves_upload_commit(&begun),
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

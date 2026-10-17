//! Omsi-Hub's cloud saves, the launcher's side (docs/omsihub/ONLINE.md, "Cloud saves"; the
//! client is `omsihub_online::saves`, a company's archive `omsi_launcher_lib::company::cloud`).
//!
//! A company is a **local save** unless the player puts it in the cloud: then it is linked to
//! one of the account's 2 slots, and its sync metadata (`Sync`: the slot, the version last
//! synced, its sha256, "cloud" or "local") is kept in the launcher's own data
//! (`~/.openomsi/omsihub-cloud.json`), not in the company's file nor the game's folder. A company
//! without metadata - every company of before - is local. Local companies are unlimited and
//! play as before, company multiplayer included; without an account, or offline, everything is
//! local and nothing here blocks or says more than "offline".
//!
//! * **Opening** a cloud company asks `list` first (`cs_open`): the cloud has a newer version -
//!   download it?; both changed - which copy to keep (the cloud's, this PC's, or both: this PC's
//!   kept as a local copy)? A save is never replaced without the player's choice.
//! * **Uploading**: a changed cloud company goes up when its pages are left (`closed`), and every
//!   ten minutes while it changes (the worker), on top of the version last synced - another
//!   computer's newer version is a conflict, never overwritten.
//! * **The limit**: "Put in the cloud" with both slots taken offers to keep the company local or
//!   to replace one of the two (`cs_put` with `replace`, after the page's confirm).
//!
//! The commands: `cs_state` (the chooser's badges, the account panel's slots: no network),
//! `cs_companies` (the driver's companies, to choose one to put in the cloud),
//! `cs_open {id}`, `cs_resolve {id, keep: cloud|local|both}`, `cs_put {id, replace?}`,
//! `cs_local {id}`, `cs_download {slot}`, `cs_remove {slot}`, `cs_sync {id}`.

use super::company::words::{self, tr, trf};
use super::online;
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{cloud as archive, store};
use omsihub_online::saves::{self, sha256_hex};
use omsihub_online::{Cloud, Error, SaveError, Slot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;

// --- the sync metadata ----------------------------------------------------------------------------

/// Where a company is kept.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    /// On this computer only (every company of before).
    #[default]
    Local,
    Cloud,
}

/// A company's sync metadata. Every field has its default: an older entry, or none, is a
/// local company.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Sync {
    pub place: Place,
    /// The account it is in the cloud of (the user id): signed in as another, it is local here.
    pub user: String,
    /// Its cloud slot (1 or 2).
    pub slot: u8,
    /// The cloud's version last synced (uploaded or downloaded).
    pub version: u64,
    /// That version's sha256 in the cloud (compressed).
    pub sha256: String,
    /// The sha256 of the company as it was packed then (`archive::pack`): another one now is a
    /// change of this computer's.
    pub local_sha256: String,
    /// When it was last synced (Unix seconds).
    pub synced_at: u64,
}

impl Sync {
    fn linked(&self, user: &str) -> bool {
        self.place == Place::Cloud && (1..=saves::MAX_SLOTS).contains(&self.slot) && !user.is_empty() && self.user == user
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
struct Book {
    companies: BTreeMap<String, Sync>,
}

const FILE: &str = "omsihub-cloud.json";

fn book_path(data: &Path) -> PathBuf {
    data.join(FILE)
}

fn read_book(data: &Path) -> Book {
    std::fs::read_to_string(book_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// A company's metadata (the default - local - when it has none).
pub fn sync_of(data: &Path, id: &str) -> Sync {
    read_book(data).companies.remove(id).unwrap_or_default()
}

/// Written whole, through a file beside it.
fn write_sync(data: &Path, id: &str, s: Option<Sync>) {
    let mut b = read_book(data);
    match s {
        Some(s) => b.companies.insert(id.to_string(), s),
        None => b.companies.remove(id),
    };
    let path = book_path(data);
    let tmp = path.with_extension("json.tmp");
    if let Ok(text) = serde_json::to_string_pretty(&b) {
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The company packed, with its sha256 (None: it is not there).
fn packed(data: &Path, id: &str) -> Option<(Vec<u8>, String)> {
    let a = archive::pack(data, id).ok()?;
    let sha = sha256_hex(&a);
    Some((a, sha))
}

// --- the cloud, as the engine sees it ---------------------------------------------------------------

/// The account's cloud saves: the client signed in (`Online`), or the tests' own.
pub trait Remote {
    /// The player's user id (`not_signed_in` signed out, `offline` without a network).
    fn user(&mut self) -> Result<String, Error>;
    fn list(&mut self) -> Result<Cloud, Error>;
    /// The archive up into `slot` (None: the first free) on top of `base` (0: new).
    fn upload(&mut self, slot: Option<u8>, name: &str, base: u64, archive: &[u8]) -> Result<Slot, SaveError>;
    /// A slot's current version: its version, its sha256 and the archive.
    fn download(&mut self, slot: u8) -> Result<(u64, String, Vec<u8>), SaveError>;
    fn delete(&mut self, slot: u8) -> Result<(), Error>;
}

/// The client of `online.rs`: each step under its lock, the file's transfer without it
/// (the friends' calls go on meanwhile).
pub struct Online;

impl Remote for Online {
    fn user(&mut self) -> Result<String, Error> {
        online::with(|c| {
            if !c.signed_in() {
                return Err(Error::Code("not_signed_in"));
            }
            // (the cloud waits for the privacy statement agreed to: everything is local meanwhile)
            if c.privacy_needed() {
                return Err(Error::Code("privacy"));
            }
            c.access_token()?;
            c.user_id().map(str::to_string).ok_or(Error::Code("server"))
        })
    }

    fn list(&mut self) -> Result<Cloud, Error> {
        online::with(|c| c.saves_list())
    }

    fn upload(&mut self, slot: Option<u8>, name: &str, base: u64, archive: &[u8]) -> Result<Slot, SaveError> {
        let packed = saves::compress(archive)?;
        let device = saves::device_name();
        let (begun, transfer) = online::with(|c| c.saves_upload_begin(slot, name, &packed, base, &device).map(|b| (b, c.transfer())))?;
        transfer.put(&begun, &packed)?;
        online::with(|c| match c.saves_upload_commit(&begun) {
            Err(SaveError::Failed(Error::Offline)) => c.saves_upload_commit(&begun),
            r => r,
        })
    }

    fn download(&mut self, slot: u8) -> Result<(u64, String, Vec<u8>), SaveError> {
        let (fetch, transfer) = online::with(|c| c.saves_download(slot, None).map(|f| (f, c.transfer())))?;
        let bytes = transfer.get(&fetch)?;
        Ok((fetch.version, fetch.sha256, saves::decompress(&bytes)?))
    }

    fn delete(&mut self, slot: u8) -> Result<(), Error> {
        online::with(|c| c.saves_delete(slot)).map(|_| ())
    }
}

// --- what is decided --------------------------------------------------------------------------------

/// A cloud company held against its slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Not in this account's cloud.
    Local,
    Synced,
    /// Changed here only: to upload.
    Upload,
    /// Changed in the cloud only: to offer.
    Newer(Slot),
    /// Changed on both: to ask.
    Conflict(Slot),
    /// The slot is empty now (removed from the cloud on another computer).
    Gone,
    /// The slot holds another company now (replaced on another computer).
    Other(Slot),
}

/// What to do with company `name` whose metadata is `s`, packed now as `local_sha`, against
/// `cloud` (`user` the account signed in).
pub fn decide(s: &Sync, user: &str, name: &str, local_sha: &str, cloud: &Cloud) -> Decision {
    if !s.linked(user) {
        return Decision::Local;
    }
    let Some(slot) = cloud.slot(s.slot) else { return Decision::Gone };
    let changed_here = local_sha != s.local_sha256;
    let changed_there = slot.version != s.version || slot.sha256 != s.sha256;
    // (another company in the slot: its name is not ours and it is not the version we know)
    if changed_there && !slot.name.trim().eq_ignore_ascii_case(name.trim()) {
        return Decision::Other(slot.clone());
    }
    match (changed_here, changed_there) {
        (false, false) => Decision::Synced,
        (true, false) => Decision::Upload,
        (false, true) => Decision::Newer(slot.clone()),
        (true, true) => Decision::Conflict(slot.clone()),
    }
}

/// What came of putting a company in the cloud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    Done(Slot),
    /// Both slots taken: keep it local, or replace one of these.
    Limit(Vec<Slot>),
}

/// The engine: the companies of `data` against a `Remote`.
pub struct Engine<'a> {
    pub data: &'a Path,
    pub remote: &'a mut dyn Remote,
}

impl Engine<'_> {
    fn company_name(&self, id: &str) -> Option<String> {
        store::load(self.data, id).ok().map(|c| c.name)
    }

    /// The metadata after a sync of `slot` with `local_sha` as the company is here.
    fn linked(&self, id: &str, user: &str, slot: &Slot, local_sha: String) {
        write_sync(self.data, id, Some(Sync { place: Place::Cloud, user: user.into(), slot: slot.slot, version: slot.version, sha256: slot.sha256.clone(), local_sha256: local_sha, synced_at: now() }));
    }

    /// Company `id` made local (its files stay; its cloud copy stays until removed).
    pub fn unlink(&self, id: &str) {
        write_sync(self.data, id, None);
    }

    /// Company `id` held against the cloud (a company unlinked by what the cloud says now -
    /// emptied or replaced elsewhere - is made local here).
    pub fn check(&mut self, id: &str) -> Result<Decision, Error> {
        let s = sync_of(self.data, id);
        if s.place != Place::Cloud {
            return Ok(Decision::Local);
        }
        let user = self.remote.user()?;
        let Some(name) = self.company_name(id) else { return Ok(Decision::Local) };
        let Some((_, sha)) = packed(self.data, id) else { return Ok(Decision::Local) };
        let cloud = self.remote.list()?;
        let d = decide(&s, &user, &name, &sha, &cloud);
        if matches!(d, Decision::Gone | Decision::Other(_)) {
            self.unlink(id);
        }
        Ok(d)
    }

    /// A change of this computer's uploaded, on top of the version last synced (a newer one in
    /// the cloud is a conflict: nothing is overwritten). The decision it came to.
    pub fn sync(&mut self, id: &str) -> Result<Decision, Error> {
        let s = sync_of(self.data, id);
        if s.place != Place::Cloud {
            return Ok(Decision::Local);
        }
        let user = self.remote.user()?;
        if !s.linked(&user) {
            return Ok(Decision::Local);
        }
        let (Some(name), Some((a, sha))) = (self.company_name(id), packed(self.data, id)) else { return Ok(Decision::Local) };
        if sha == s.local_sha256 {
            return Ok(Decision::Synced);
        }
        match self.remote.upload(Some(s.slot), &name, s.version, &a) {
            Ok(slot) => {
                self.linked(id, &user, &slot, sha);
                Ok(Decision::Synced)
            }
            Err(SaveError::Conflict(now)) => Ok(if now.empty {
                self.unlink(id);
                Decision::Gone
            } else if !now.name.trim().eq_ignore_ascii_case(name.trim()) {
                self.unlink(id);
                Decision::Other(now)
            } else {
                Decision::Conflict(now)
            }),
            Err(SaveError::Failed(e)) => Err(e),
        }
    }

    /// "Put this company in the cloud": into the first free slot, or into `replace` - that slot
    /// emptied first, and the company of this computer it held made local.
    pub fn put(&mut self, id: &str, replace: Option<u8>) -> Result<Put, Error> {
        let user = self.remote.user()?;
        let s = sync_of(self.data, id);
        if s.linked(&user) {
            self.sync(id)?;
            let cloud = self.remote.list()?;
            return cloud.slot(s.slot).cloned().map(Put::Done).ok_or(Error::Code("no_cloud_save"));
        }
        let name = self.company_name(id).ok_or(Error::Code("server"))?;
        let (a, sha) = packed(self.data, id).ok_or(Error::Code("server"))?;
        if let Some(n) = replace {
            self.remote.delete(n)?;
            for (other, o) in read_book(self.data).companies {
                if o.linked(&user) && o.slot == n {
                    self.unlink(&other);
                }
            }
        }
        match self.remote.upload(replace, &name, 0, &a) {
            Ok(slot) => {
                self.linked(id, &user, &slot, sha);
                Ok(Put::Done(slot))
            }
            Err(SaveError::Failed(Error::Code("limit"))) => Ok(Put::Limit(self.remote.list()?.slots.into_iter().filter(|x| !x.empty).collect())),
            // (the slot filled meanwhile, from another computer)
            Err(SaveError::Conflict(_)) => Err(Error::Code("conflict")),
            Err(SaveError::Failed(e)) => Err(e),
        }
    }

    /// The cloud's version of slot `n` put on this computer as company `id` (`profile` the
    /// driver's), linked to the slot.
    fn fetch_into(&mut self, user: &str, n: u8, id: &str, profile: &str) -> Result<Slot, Error> {
        let (version, sha, a) = self.remote.download(n).map_err(|e| e.error())?;
        archive::install(self.data, id, &a, profile, None).map_err(|_| Error::Code("bad_save"))?;
        let local = packed(self.data, id).map(|p| p.1).unwrap_or_default();
        let cloud = self.remote.list().unwrap_or(Cloud { max_slots: saves::MAX_SLOTS, max_bytes: saves::MAX_BYTES, slots: Vec::new() });
        let slot = cloud.slot(n).cloned().filter(|s| s.version == version).unwrap_or(Slot { slot: n, version, sha256: sha, ..Default::default() });
        self.linked(id, user, &slot, local);
        Ok(slot)
    }

    /// The player's answer to a newer version or a conflict: `cloud` (the cloud's replaces this
    /// computer's), `local` (this computer's goes up over it; the cloud keeps its older
    /// versions), `both` (this computer's kept as a local copy, the cloud's downloaded). The id
    /// of the local copy, for `both`.
    pub fn resolve(&mut self, id: &str, keep: &str, copy_name: &str) -> Result<Option<String>, Error> {
        let user = self.remote.user()?;
        let s = sync_of(self.data, id);
        if !s.linked(&user) {
            return Ok(None);
        }
        let profile = store::load(self.data, id).map(|c| c.profile).unwrap_or_default();
        match keep {
            "local" => {
                let (name, (a, sha)) = (self.company_name(id).ok_or(Error::Code("server"))?, packed(self.data, id).ok_or(Error::Code("server"))?);
                let cloud = self.remote.list()?;
                let base = cloud.slot(s.slot).map_or(0, |x| x.version);
                match self.remote.upload(Some(s.slot), &name, base, &a) {
                    Ok(slot) => self.linked(id, &user, &slot, sha),
                    Err(e) => return Err(e.error()),
                }
                Ok(None)
            }
            "both" => {
                let (a, _) = packed(self.data, id).ok_or(Error::Code("server"))?;
                let copy = store::unused_id(self.data, copy_name);
                archive::install(self.data, &copy, &a, "", Some(copy_name)).map_err(|_| Error::Code("server"))?;
                self.fetch_into(&user, s.slot, id, &profile)?;
                Ok(Some(copy))
            }
            _ => {
                self.fetch_into(&user, s.slot, id, &profile)?;
                Ok(None)
            }
        }
    }

    /// "Download to this PC": the company linked to slot `n` brought up to the cloud's version
    /// (unless it changed here too: a conflict, asked), or the slot's company put on this
    /// computer as a new one. The company's id.
    pub fn download(&mut self, n: u8, profile: &str) -> Result<Result<String, Decision>, Error> {
        let user = self.remote.user()?;
        let here = read_book(self.data).companies.into_iter().find(|(id, o)| o.linked(&user) && o.slot == n && store::path_of(self.data, id).exists());
        if let Some((id, _)) = here {
            return match self.check(&id)? {
                d @ Decision::Conflict(_) => Ok(Err(d)),
                Decision::Gone | Decision::Other(_) | Decision::Local => self.download(n, profile),
                _ => {
                    self.fetch_into(&user, n, &id, profile)?;
                    Ok(Ok(id))
                }
            };
        }
        let cloud = self.remote.list()?;
        let slot = cloud.slot(n).cloned().ok_or(Error::Code("no_cloud_save"))?;
        let id = store::unused_id(self.data, &slot.name);
        self.fetch_into(&user, n, &id, profile)?;
        Ok(Ok(id))
    }

    /// Slot `n` removed from the cloud; the company of this computer it held stays, local.
    pub fn remove(&mut self, n: u8) -> Result<(), Error> {
        let user = self.remote.user()?;
        self.remote.delete(n)?;
        for (id, o) in read_book(self.data).companies {
            if o.linked(&user) && o.slot == n {
                self.unlink(&id);
            }
        }
        Ok(())
    }
}

// --- the launcher's state: what the page is shown, the worker -------------------------------------

#[derive(Default)]
struct State {
    worker: bool,
    /// The cloud as `list` last said it, and when.
    cloud: Option<Cloud>,
    listed_at: u64,
    user: String,
    offline: bool,
    signed_in: bool,
    /// Uploading now.
    busy: HashSet<String>,
    /// Companies found in conflict (asked when opened).
    conflict: HashSet<String>,
    /// A company's last upload error.
    failed: HashMap<String, Error>,
    /// To upload soon: their pages were left.
    queue: Vec<String>,
    /// When each linked company was last tried.
    tried: HashMap<String, u64>,
    /// The sha256 of each company as packed, by its file's size and time.
    shas: HashMap<String, (u64, std::time::SystemTime, String)>,
    /// Notices for the page (said once).
    notices: Vec<String>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
/// The engine's work runs one at a time (the worker's uploads, the page's commands).
static WORK: Mutex<()> = Mutex::new(());

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn work() -> MutexGuard<'static, ()> {
    WORK.lock().unwrap_or_else(|e| e.into_inner())
}

fn data() -> PathBuf {
    core::data_dir()
}

/// How often a changed cloud company goes up, and the cloud is listed again.
const UPLOAD_EVERY: u64 = 10 * 60;
const LIST_EVERY: u64 = 5 * 60;

/// The company's sha256 as packed now (read again only when its file changed).
fn local_sha(id: &str) -> Option<String> {
    let d = data();
    let meta = std::fs::metadata(store::path_of(&d, id)).ok()?;
    let key = (meta.len(), meta.modified().ok()?);
    if let Some((len, at, sha)) = state().shas.get(id) {
        if (*len, *at) == key {
            return Some(sha.clone());
        }
    }
    let (_, sha) = packed(&d, id)?;
    state().shas.insert(id.to_string(), (key.0, key.1, sha.clone()));
    Some(sha)
}

/// The account's state into the view after a call: signed in or out, offline.
fn note(r: &Result<impl Sized, Error>) {
    let mut st = state();
    match r {
        Err(Error::Offline) => st.offline = true,
        Err(Error::Code("not_signed_in" | "privacy") | Error::SignedOut) => {
            st.signed_in = false;
            st.cloud = None;
        }
        Ok(_) => {
            st.offline = false;
            st.signed_in = true;
        }
        Err(_) => st.offline = false,
    }
}

/// The cloud listed again into the view.
fn relist(remote: &mut dyn Remote) -> Result<(), Error> {
    let r = remote.user().and_then(|u| remote.list().map(|c| (u, c)));
    note(&r);
    let (user, cloud) = r?;
    let mut st = state();
    st.user = user;
    st.cloud = Some(cloud);
    st.listed_at = now();
    Ok(())
}

/// One company's sync on the worker (or after its pages were left).
fn sync_now(id: &str) {
    let _w = work();
    state().busy.insert(id.to_string());
    let d = data();
    let r = Engine { data: &d, remote: &mut Online }.sync(id);
    note(&r);
    let mut st = state();
    st.busy.remove(id);
    st.tried.insert(id.to_string(), now());
    st.shas.remove(id);
    match r {
        Ok(Decision::Conflict(_)) => {
            st.conflict.insert(id.to_string());
        }
        Ok(Decision::Gone | Decision::Other(_)) => {
            st.notices.push(trf("%{name} is no longer in the cloud: it stays on this PC as a local save.", &[("name", &name_of(id))]));
        }
        Ok(_) => {
            st.failed.remove(id);
            st.conflict.remove(id);
            // (the slot's new version: listed again soon)
            st.listed_at = 0;
        }
        Err(Error::Offline | Error::Code("not_signed_in" | "privacy") | Error::SignedOut) => {}
        Err(e) => {
            st.failed.insert(id.to_string(), e);
        }
    }
}

fn name_of(id: &str) -> String {
    store::load(&data(), id).map(|c| c.name).unwrap_or_else(|_| id.to_string())
}

/// The worker: the cloud listed every few minutes, a changed cloud company uploaded when its
/// pages were left and every ten minutes, never under the view's lock during a request.
fn worker() {
    loop {
        // (signed in with the privacy statement agreed to: before, the cloud is not asked)
        let signed = online::with(|c| c.signed_in() && !c.privacy_needed());
        if signed {
            let stale = { now().saturating_sub(state().listed_at) >= LIST_EVERY };
            if stale {
                let _w = work();
                let _ = relist(&mut Online);
            }
            let (user, queued, tried) = {
                let mut st = state();
                (st.user.clone(), std::mem::take(&mut st.queue), st.tried.clone())
            };
            if !user.is_empty() && !state().offline {
                for (id, s) in read_book(&data()).companies {
                    if !s.linked(&user) || state().conflict.contains(&id) {
                        continue;
                    }
                    let due = queued.contains(&id) || tried.get(&id).is_none_or(|t| now().saturating_sub(*t) >= UPLOAD_EVERY);
                    if due && local_sha(&id).is_some_and(|sha| sha != s.local_sha256) {
                        sync_now(&id);
                    } else if due {
                        state().tried.insert(id, now());
                    }
                }
            }
        } else {
            let mut st = state();
            st.signed_in = false;
            st.cloud = None;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

fn start_worker() {
    let mut st = state();
    if st.worker {
        return;
    }
    st.worker = true;
    let _ = std::thread::Builder::new().name("omsihub cloud saves".into()).spawn(worker);
}

/// The company's pages were left: a change goes up soon (on the worker's thread).
pub fn closed(id: &str) {
    if !id.is_empty() && sync_of(&data(), id).place == Place::Cloud {
        state().queue.push(id.to_string());
        start_worker();
    }
}

// --- the page ---------------------------------------------------------------------------------------

/// Unix milliseconds of a slot's time (RFC 3339, or a number in seconds or milliseconds).
fn millis(t: Option<&str>) -> Option<i64> {
    let t = t?.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n } else { n * 1000 });
    }
    online::unix_of(t).map(|s| s * 1000)
}

fn slot_json(s: &Slot, linked: Option<&str>) -> Value {
    json!({ "slot": s.slot, "empty": s.empty, "name": s.name, "size": s.size, "version": s.version, "device": s.device, "at": millis(s.updated_at.as_deref()), "linked": linked })
}

/// What a company's states are called (the last: a local save).
const STATES: &[(&str, &str)] = &[
    ("synced", "Synced"),
    ("uploading", "Uploading"),
    ("newer", "Newer in the cloud"),
    ("conflict", "Changed on both"),
    ("offline", "Offline"),
    ("error", "Not synced"),
    ("local", "Local save"),
];

fn state_label(st: &str) -> String {
    tr(STATES.iter().find(|x| x.0 == st).unwrap_or(&STATES[STATES.len() - 1]).1)
}

/// The chooser's badges and the account panel's slots, from what was last fetched.
fn view() -> Value {
    start_worker();
    let d = data();
    let book = read_book(&d);
    let mut companies = serde_json::Map::new();
    let (user, cloud, offline, signed_in) = {
        let st = state();
        (st.user.clone(), st.cloud.clone(), st.offline, st.signed_in)
    };
    for (id, s) in &book.companies {
        if s.place != Place::Cloud || !store::path_of(&d, id).exists() {
            continue;
        }
        // (in the cloud of another account: local here)
        if !user.is_empty() && s.user != user {
            continue;
        }
        let sha = local_sha(id);
        let st = state();
        let changed = sha.as_deref().is_some_and(|x| x != s.local_sha256);
        let there = cloud.as_ref().and_then(|c| c.slot(s.slot));
        let newer = there.is_some_and(|x| x.version != s.version);
        let kind = if !signed_in || offline || user.is_empty() {
            "offline"
        } else if st.conflict.contains(id) || (changed && newer) {
            "conflict"
        } else if st.busy.contains(id) || (changed && !st.failed.contains_key(id)) {
            "uploading"
        } else if st.failed.contains_key(id) {
            "error"
        } else if newer {
            "newer"
        } else {
            "synced"
        };
        let tip = st.failed.get(id).map(|e| tr(e.text()));
        companies.insert(id.clone(), json!({ "place": "cloud", "state": kind, "label": state_label(kind), "tip": tip, "slot": s.slot }));
    }
    let linked = |n: u8| book.companies.iter().find(|(id, s)| s.linked(&user) && s.slot == n && store::path_of(&d, id).exists()).map(|(id, _)| id.clone());
    let slots: Vec<Value> = cloud.as_ref().map(|c| c.slots.iter().map(|s| slot_json(s, linked(s.slot).as_deref())).collect()).unwrap_or_default();
    let notices = std::mem::take(&mut state().notices);
    json!({
        "signed_in": signed_in && !user.is_empty(), "offline": offline, "listed": cloud.is_some(),
        "max_slots": cloud.as_ref().map_or(saves::MAX_SLOTS, |c| c.max_slots), "slots": slots,
        "companies": companies, "local_label": state_label("local"), "cloud_label": tr("Cloud save"), "notices": notices,
    })
}

/// A copy's name: "<name> (this PC)".
fn copy_name(id: &str) -> String {
    trf("%{name} (this PC)", &[("name", &name_of(id))])
}

/// The two copies of a company as the question shows them: when and where each was saved.
fn sides(id: &str, cloud: &Slot) -> Value {
    let d = data();
    let local_at = std::fs::metadata(store::path_of(&d, id)).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_millis() as i64);
    json!({
        "local": { "at": local_at, "device": saves::device_name() },
        "cloud": { "at": millis(cloud.updated_at.as_deref()), "device": cloud.device, "version": cloud.version },
    })
}

fn error(e: Error) -> Value {
    json!({ "error": tr(e.text()), "code": e.code() })
}

/// A company about to be opened: what the cloud says of it, and what to ask.
fn open(id: &str) -> Value {
    let d = data();
    if sync_of(&d, id).place != Place::Cloud {
        return json!({ "state": "local" });
    }
    let _w = work();
    let r = Engine { data: &d, remote: &mut Online }.check(id);
    note(&r);
    let _ = relist(&mut Online);
    match r {
        Ok(Decision::Newer(slot)) => {
            let mut v = sides(id, &slot);
            v["ask"] = json!("newer");
            v["name"] = json!(name_of(id));
            v
        }
        Ok(Decision::Conflict(slot)) => {
            state().conflict.insert(id.to_string());
            let mut v = sides(id, &slot);
            v["ask"] = json!("conflict");
            v["name"] = json!(name_of(id));
            v["copy"] = json!(copy_name(id));
            v
        }
        Ok(Decision::Upload) => {
            state().queue.push(id.to_string());
            json!({ "state": "uploading" })
        }
        Ok(Decision::Gone | Decision::Other(_)) => json!({ "state": "local", "notice": trf("%{name} is no longer in the cloud: it stays on this PC as a local save.", &[("name", &name_of(id))]) }),
        Ok(Decision::Synced | Decision::Local) => json!({ "state": "synced" }),
        // (offline, signed out, the server busy: opened as it is, quietly)
        Err(e) => json!({ "state": "offline", "code": e.code() }),
    }
}

fn resolve(id: &str, keep: &str) -> Value {
    let d = data();
    let _w = work();
    let name = name_of(id);
    let copy = copy_name(id);
    let r = Engine { data: &d, remote: &mut Online }.resolve(id, keep, &copy);
    note(&r);
    {
        let mut st = state();
        st.shas.remove(id);
        if r.is_ok() {
            st.conflict.remove(id);
            st.failed.remove(id);
        }
        st.listed_at = 0;
    }
    match r {
        Ok(copy_id) => json!({
            "ok": true, "reread": id, "copy": copy_id,
            "message": match keep {
                "local" => trf("This PC's %{name} is in the cloud now.", &[("name", &name)]),
                "both" => trf("Both kept: the cloud's %{name}, and this PC's as %{copy}.", &[("name", &name), ("copy", &copy)]),
                _ => trf("The cloud's %{name} is on this PC now.", &[("name", &name)]),
            },
        }),
        Err(e) => error(e),
    }
}

fn put(id: &str, replace: Option<u8>) -> Value {
    let d = data();
    let _w = work();
    let name = name_of(id);
    let r = Engine { data: &d, remote: &mut Online }.put(id, replace);
    note(&r);
    state().listed_at = 0;
    let _ = relist(&mut Online);
    match r {
        Ok(Put::Done(slot)) => json!({ "ok": true, "slot": slot.slot, "message": trf("%{name} is in the cloud now.", &[("name", &name)]) }),
        Ok(Put::Limit(slots)) => json!({
            "limit": true, "text": tr(Error::Code("limit").text()),
            "slots": slots.iter().map(|s| slot_json(s, None)).collect::<Vec<_>>(),
        }),
        Err(e) => error(e),
    }
}

fn download(n: u8) -> Value {
    let d = data();
    let _w = work();
    let profile = core::load_config().profile;
    let r = Engine { data: &d, remote: &mut Online }.download(n, &profile);
    note(&r);
    state().listed_at = 0;
    match r {
        Ok(Ok(id)) => {
            state().shas.remove(&id);
            json!({ "ok": true, "reread": id, "message": trf("%{name} is on this PC now.", &[("name", &name_of(&id))]) })
        }
        Ok(Err(_)) => error(Error::Code("conflict")),
        Err(e) => error(e),
    }
}

fn remove(n: u8) -> Value {
    let d = data();
    let _w = work();
    let r = Engine { data: &d, remote: &mut Online }.remove(n);
    note(&r);
    state().listed_at = 0;
    let _ = relist(&mut Online);
    match r {
        Ok(()) => json!({ "ok": true, "message": tr("Removed from the cloud. A company of it on this PC stays as a local save.") }),
        Err(e) => error(e),
    }
}

/// The account deleted (its cloud saves with it): every company here is local again.
pub fn forget_account() {
    let d = data();
    for id in read_book(&d).companies.keys() {
        write_sync(&d, id, None);
    }
    let mut st = state();
    st.cloud = None;
    st.user.clear();
    st.conflict.clear();
    st.failed.clear();
}

/// A command of the cloud saves, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    let id = args.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let slot = args.get("slot").and_then(Value::as_u64).map(|n| n.min(255) as u8);
    let ok_id = !id.is_empty() && store::path_of(&data(), &id).exists();
    Some(Ok(match cmd {
        "cs_state" => view(),
        // (the driver's companies, for "Put this company in the cloud")
        // (each with its whole name for the picker: "Land (LAN) · Grundorf" - the name, the short
        // name and the home map)
        "cs_companies" => json!(store::list(&data(), &core::load_config().profile)
            .iter()
            .map(|c| {
                let map = Some(c.map_name.trim()).filter(|n| !n.is_empty()).map(str::to_string).or_else(|| std::path::Path::new(&c.map).parent()?.file_name()?.to_str().map(str::to_string)).unwrap_or_default();
                let mut label = c.name.trim().to_string();
                if !c.short.trim().is_empty() && !c.short.trim().eq_ignore_ascii_case(c.name.trim()) {
                    label = format!("{label} ({})", c.short.trim());
                }
                if !map.is_empty() {
                    label = format!("{label} · {map}");
                }
                json!({ "id": c.id, "name": c.name, "label": label })
            })
            .collect::<Vec<_>>()),
        "cs_open" if ok_id => open(&id),
        "cs_resolve" if ok_id => resolve(&id, args.get("keep").and_then(Value::as_str).unwrap_or("cloud")),
        "cs_put" if ok_id => put(&id, args.get("replace").and_then(Value::as_u64).map(|n| n.min(255) as u8)),
        "cs_local" if ok_id => {
            let d = data();
            Engine { data: &d, remote: &mut Online }.unlink(&id);
            json!({ "ok": true, "message": trf("%{name} stays local.", &[("name", &name_of(&id))]) })
        }
        "cs_sync" if ok_id => {
            closed(&id);
            json!({})
        }
        "cs_download" => match slot {
            Some(n) => download(n),
            None => error(Error::Code("bad_input")),
        },
        "cs_remove" => match slot {
            Some(n) => remove(n),
            None => error(Error::Code("bad_input")),
        },
        "cs_open" | "cs_resolve" | "cs_put" | "cs_local" | "cs_sync" => error(Error::Code("bad_input")),
        _ => return None,
    }))
}

#[cfg(test)]
#[path = "cloud_tests.rs"]
mod tests;

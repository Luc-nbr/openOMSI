//! A company as one cloud save (Omsi-Hub's cloud saves, docs/omsihub/ONLINE.md): what another
//! computer needs to go on with it, packed into one small archive that the same files always
//! pack the same (the launcher compresses it and keeps its sha256 to see a change).
//!
//! What goes in, under names of its own (the company's id is not in them, so a save can come
//! back as another company - "keep both"):
//!
//! * `company.json` - the company's file (`store`), as it is on the disk;
//! * `orders.jsonl` - the phone's orders still waiting (`remote`);
//! * `live.jsonl` and `live-<date>.taken` - what the game reported since the day's last close
//!   (`store::take_live`): the next close books it, on whichever computer it runs.
//!
//! Not the game's plans of the day (`<id>.liveplan.json`, `<id>.drive.liveplan.json`): the
//! launcher writes them from the company whenever it plans its day, and a stale one is taken
//! away when a save comes in (`install`).
//!
//! The archive: `MAGIC`, then each file in its name's order as the name's length (u16, little
//! endian), the name, the length (u64) and the bytes.

use super::model::Company;
use super::{plan, remote, store};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8] = b"OMSIHUB-SAVE 1\n";
/// The company's own file in the archive.
pub const COMPANY: &str = "company.json";
const ORDERS: &str = "orders.jsonl";
const LIVE: &str = "live.jsonl";
/// At most this many files (a company has four kinds; taken files of a day or two).
const MAX_FILES: usize = 64;

/// The files of company `id` that go with it: their names in the archive and where they are.
fn members(data: &Path, id: &str) -> Vec<(String, PathBuf)> {
    let dir = store::dir(data);
    let mut out = vec![(COMPANY.to_string(), store::path_of(data, id)), (ORDERS.into(), remote::orders_file(data, id)), (LIVE.into(), store::live_file(data, id))];
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let head = format!("{id}.live-");
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(date) = name.strip_prefix(head.as_str()).and_then(|r| r.strip_suffix(".taken")) {
                out.push((format!("live-{date}.taken"), e.path()));
            }
        }
    }
    out
}

/// A name the archive may hold (nothing that could reach outside the companies' folder).
fn allowed(name: &str) -> bool {
    match name {
        COMPANY | ORDERS | LIVE => true,
        _ => name.strip_prefix("live-").and_then(|r| r.strip_suffix(".taken")).is_some_and(|d| !d.is_empty() && d.len() <= 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-')),
    }
}

/// Where a file of the archive goes for company `id`.
fn target(data: &Path, id: &str, name: &str) -> PathBuf {
    match name {
        COMPANY => store::path_of(data, id),
        ORDERS => remote::orders_file(data, id),
        LIVE => store::live_file(data, id),
        // (`live-<date>.taken`)
        _ => store::dir(data).join(format!("{id}.{name}")),
    }
}

/// Company `id` packed: its file and what lies beside it that it needs.
pub fn pack(data: &Path, id: &str) -> Result<Vec<u8>> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, path) in members(data, id) {
        match std::fs::read(&path) {
            Ok(bytes) => files.push((name, bytes)),
            Err(e) if name == COMPANY => return Err(e).with_context(|| format!("cannot read {}", path.display())),
            Err(_) => {}
        }
    }
    Ok(pack_files(&mut files))
}

fn pack_files(files: &mut [(String, Vec<u8>)]) -> Vec<u8> {
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = MAGIC.to_vec();
    for (name, bytes) in files.iter() {
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(name.as_bytes());
        out.extend((bytes.len() as u64).to_le_bytes());
        out.extend(bytes);
    }
    out
}

/// An archive read and checked: the company and every file as it was packed.
#[derive(Debug, Clone)]
pub struct Unpacked {
    pub company: Company,
    pub files: Vec<(String, Vec<u8>)>,
}

pub fn unpack(archive: &[u8]) -> Result<Unpacked> {
    let mut rest = archive.strip_prefix(MAGIC).context("not a company's cloud save")?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    while !rest.is_empty() {
        if files.len() >= MAX_FILES {
            bail!("too many files in the save");
        }
        let take = |rest: &mut &[u8], n: usize| -> Result<Vec<u8>> {
            if rest.len() < n {
                bail!("the save is cut short");
            }
            let (a, b) = rest.split_at(n);
            *rest = b;
            Ok(a.to_vec())
        };
        let n = u16::from_le_bytes(take(&mut rest, 2)?.try_into().unwrap_or_default()) as usize;
        let name = String::from_utf8(take(&mut rest, n)?).context("a file's name is not text")?;
        let len = u64::from_le_bytes(take(&mut rest, 8)?.try_into().unwrap_or_default());
        let bytes = take(&mut rest, usize::try_from(len).unwrap_or(usize::MAX))?;
        if !allowed(&name) || files.iter().any(|f| f.0 == name) {
            bail!("the save holds a file it may not: {name:?}");
        }
        files.push((name, bytes));
    }
    let text = files.iter().find(|f| f.0 == COMPANY).map(|f| String::from_utf8_lossy(&f.1).into_owned()).context("the save holds no company")?;
    let company: Company = serde_json::from_str(&text).context("the save's company cannot be read")?;
    Ok(Unpacked { company: store::migrate(company), files })
}

/// A save put on the disk as company `id` (a new id: "keep both"), for the driver `profile`
/// (empty: the save's own), renamed when `name` is given. What lay beside an older company `id`
/// goes first - the save's files replace it all - and the company's file is written last, whole
/// (`store::save`): a save that cannot be read changes nothing.
pub fn install(data: &Path, id: &str, archive: &[u8], profile: &str, name: Option<&str>) -> Result<Company> {
    let u = unpack(archive)?;
    let mut c = u.company;
    c.id = id.to_string();
    if !profile.trim().is_empty() {
        c.profile = profile.trim().to_string();
    }
    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
        c.name = n.trim().to_string();
    }
    std::fs::create_dir_all(store::dir(data))?;
    for (n, path) in members(data, id) {
        if n != COMPANY {
            let _ = std::fs::remove_file(path);
        }
    }
    for p in [plan::live_plan_file(data, id), plan::drive_plan_file(data, id)] {
        let _ = std::fs::remove_file(p);
    }
    for (n, bytes) in u.files.iter().filter(|f| f.0 != COMPANY) {
        let path = target(data, id, n);
        std::fs::write(&path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
    }
    store::save(data, &c)?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::day::LiveEvent;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-cloud-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_company_packs_the_same_and_comes_back_as_it_was_or_as_another() {
        let (a, b) = (temp("pack-a"), temp("pack-b"));
        let mut c = found(&Founding { name: "Stadtbus".into(), ..Default::default() }, "Luc");
        c.id = "stadtbus".into();
        store::save(&a, &c).unwrap();
        store::append_live(&a, &c.id, &LiveEvent::Breakdown { vehicle: 3 }).unwrap();
        remote::queue(&a, &c.id, &remote::Order::Build { area: super::super::depot::Area::Wash }).unwrap();
        // (the game's plans stay behind; another company's files too)
        plan::save_live_plan(&a, &plan::LivePlan { company: c.id.clone(), ..Default::default() }).unwrap();
        store::append_live(&a, "stadtbus-2", &LiveEvent::Breakdown { vehicle: 9 }).unwrap();
        let packed = pack(&a, &c.id).unwrap();
        assert_eq!(pack(&a, &c.id).unwrap(), packed, "the same files pack the same");
        let u = unpack(&packed).unwrap();
        assert_eq!(u.files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), [COMPANY, LIVE, ORDERS]);
        assert_eq!(u.company, c);
        // (on another computer, for its driver)
        let back = install(&b, &c.id, &packed, "Anna", None).unwrap();
        assert_eq!((back.id.as_str(), back.profile.as_str()), ("stadtbus", "Anna"));
        assert_eq!(remote::pending(&b, &c.id).len(), 1);
        assert_eq!(store::peek_live(&b, &c.id, 0), vec![LiveEvent::Breakdown { vehicle: 3 }]);
        assert!(!plan::live_plan_file(&b, &c.id).exists());
        // (packed there again: the same but for the driver)
        let again = unpack(&pack(&b, &c.id).unwrap()).unwrap();
        assert_eq!(again.files.len(), 3);
        // (kept both: as another company, its own files, the first one untouched)
        let copy = install(&a, "stadtbus-copy", &packed, "", Some("Stadtbus (copy)")).unwrap();
        assert_eq!((copy.name.as_str(), copy.profile.as_str()), ("Stadtbus (copy)", "Luc"));
        assert_eq!(store::load(&a, "stadtbus").unwrap(), c);
        assert_eq!(remote::pending(&a, "stadtbus-copy").len(), 1);
        // (a newer save over an older company: what lay beside the old one goes)
        let mut plain = c.clone();
        plain.cash += 100;
        store::save(&b, &plain).unwrap();
        std::fs::remove_file(store::live_file(&b, &c.id)).unwrap();
        std::fs::remove_file(remote::orders_file(&b, &c.id)).unwrap();
        let newer = pack(&b, &c.id).unwrap();
        install(&a, &c.id, &newer, "", None).unwrap();
        assert_eq!(store::load(&a, &c.id).unwrap().cash, c.cash + 100);
        assert!(remote::pending(&a, &c.id).is_empty() && store::peek_live(&a, &c.id, 0).is_empty());
        assert!(!plan::live_plan_file(&a, &c.id).exists());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn a_damaged_or_hostile_save_changes_nothing() {
        let data = temp("hostile");
        let mut c = found(&Founding { name: "Alt".into(), ..Default::default() }, "Luc");
        c.id = "alt".into();
        store::save(&data, &c).unwrap();
        let good = pack(&data, "alt").unwrap();
        assert!(unpack(b"PK\x03\x04 not ours").is_err());
        assert!(unpack(&good[..good.len() - 3]).is_err(), "cut short");
        let mut evil = vec![("company.json".to_string(), std::fs::read(store::path_of(&data, "alt")).unwrap()), ("../../evil.txt".to_string(), b"x".to_vec())];
        assert!(unpack(&pack_files(&mut evil)).is_err());
        let mut none = vec![(LIVE.to_string(), b"{}".to_vec())];
        assert!(unpack(&pack_files(&mut none)).is_err(), "no company in it");
        let mut broken = vec![(COMPANY.to_string(), b"{ not json".to_vec())];
        assert!(install(&data, "alt", &pack_files(&mut broken), "", None).is_err());
        assert_eq!(store::load(&data, "alt").unwrap(), c, "the company as it was");
        assert!(allowed("live-2026-10-16.taken") && !allowed("live-..\\x.taken") && !allowed("company.JSON"));
        let _ = std::fs::remove_dir_all(&data);
    }
}

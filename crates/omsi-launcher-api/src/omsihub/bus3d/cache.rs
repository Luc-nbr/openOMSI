//! The packs on disk, and the register of what the page may ask for by id.
//!
//! Under openOMSI's cache folder (`~/.openomsi/cache/bus3d/v2/`, never the OMSI 2 folder):
//! * `i/<bus id>.json`: which pack a bus has now;
//! * `p/<pack>.b3d`: the pack (`pack::pack_bytes`), scrambled vertex blocks as their files
//!   have them;
//! * `p/<pack>.json`: beside it, for this side only: the manifest, the scrambled blocks, the
//!   textures' files and every file and folder the pack was read from with its size and time;
//! * `l/<pack>-<look>.json`: a livery's look with the bus options it wears, and the files it
//!   depends on (the `.cti`s, the scripts).
//!
//! A pack whose files changed is built again; at most a gigabyte of packs is kept (the ones
//! opened longest ago go first).

use super::look::{self, Look};
use super::pack::{self, BusRead, Head, Manifest, ScrambledBlock};
use super::tex::{self, TexSource};
use omsi_launcher_lib as core;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// The most packs kept, in bytes.
const MOST: u64 = 1 << 30;

pub fn dir() -> PathBuf {
    core::data_dir().join("cache").join("bus3d").join("v2")
}

/// What is kept beside a pack.
#[derive(Clone, Serialize, Deserialize)]
pub struct Side {
    pub manifest: Manifest,
    pub scrambled: Vec<ScrambledBlock>,
    pub sources: Vec<TexSource>,
    pub stamps: Vec<(PathBuf, u64, u64)>,
}

#[derive(Serialize, Deserialize)]
struct Index {
    bus: String,
    pack: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct KeptLook {
    look: Look,
    sources: Vec<TexSource>,
    stamps: Vec<(PathBuf, u64, u64)>,
}

/// Where a pack's bytes are.
#[derive(Clone)]
enum Place {
    File(PathBuf, Vec<ScrambledBlock>),
    /// Not kept on disk (see `Built::cacheable`): in memory, descrambled.
    Memory(Arc<Vec<u8>>),
}

/// A bus read lately: for its looks.
struct Read {
    bus: String,
    read: Arc<BusRead>,
    head: Arc<Head>,
    manifest: Arc<Manifest>,
}

#[derive(Default)]
struct Register {
    textures: HashMap<String, TexSource>,
    packs: HashMap<String, Place>,
    last: Option<Read>,
    /// One build of a bus at a time (the viewer and a photo of the same bus).
    building: HashMap<String, Arc<Mutex<()>>>,
}

fn reg() -> std::sync::MutexGuard<'static, Register> {
    static R: OnceLock<Mutex<Register>> = OnceLock::new();
    R.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// The textures a pack or a look named: the page may ask for them by id now.
pub fn register(sources: &[TexSource]) {
    let mut r = reg();
    for s in sources {
        r.textures.insert(s.id.clone(), s.clone());
    }
}

/// The file of a texture id, if it is one the page was given and the file is as it was.
pub fn texture(id: &str) -> Result<PathBuf, String> {
    let s = reg().textures.get(id).cloned().ok_or_else(|| "no such texture".to_string())?;
    match tex::stamp(&s.path) {
        Some((size, mtime)) if size == s.size && mtime == s.mtime => Ok(s.path),
        // (read through the content's file system: no stamp to check)
        None if s.size == 0 => Ok(s.path),
        _ => Err("the texture changed".into()),
    }
}

/// Whether every file still has the size and time it had.
fn fresh(stamps: &[(PathBuf, u64, u64)]) -> bool {
    let n = stamps.len();
    if n == 0 {
        return false;
    }
    let threads = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4).clamp(1, 16);
    let chunk = n.div_ceil(threads);
    std::thread::scope(|s| {
        let jobs: Vec<_> = stamps
            .chunks(chunk.max(1))
            .map(|c| s.spawn(move || c.iter().all(|(p, size, mtime)| tex::stamp(p).map(|(a, b)| (a, b)).unwrap_or((u64::MAX, u64::MAX)) == (*size, *mtime))))
            .collect();
        jobs.into_iter().all(|j| j.join().unwrap_or(false))
    })
}

fn stamps_of(paths: &[PathBuf]) -> Vec<(PathBuf, u64, u64)> {
    paths
        .iter()
        .map(|p| {
            let (s, m) = tex::stamp(p).unwrap_or((u64::MAX, u64::MAX));
            (p.clone(), s, m)
        })
        .collect()
}

fn bus_id(root: &Path, bus: &str) -> String {
    tex::id_of(&format!("bus|{}|{}", root.to_string_lossy().to_lowercase(), bus.replace('\\', "/").to_lowercase()))
}

/// Write `bytes` to `path` whole: beside it first, then in its place.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let part = path.with_extension(format!("part{}", std::process::id()));
    std::fs::write(&part, bytes)?;
    match std::fs::rename(&part, path) {
        Ok(()) => Ok(()),
        // (another build of the same pack got there first, or the file is being read)
        Err(_) if path.is_file() => {
            let _ = std::fs::remove_file(&part);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// The packs kept down to `MOST` bytes, the ones opened longest ago first.
fn trim() {
    let p = dir().join("p");
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = std::fs::read_dir(&p)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "b3d"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let side = e.path().with_extension("json");
            let used = std::fs::metadata(&side).and_then(|s| s.modified()).or_else(|_| m.modified()).ok()?;
            Some((e.path(), m.len(), used))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    for (f, len, _) in files {
        if total <= MOST {
            break;
        }
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(f.with_extension("json"));
        total = total.saturating_sub(len);
    }
}

/// What opening a bus gives the page.
pub struct Opened {
    pub manifest: Arc<Manifest>,
    pub look: Look,
    pub from_cache: bool,
    pub ms: u64,
}

/// The pack of `bus`, from the cache when its files did not change, else built now.
pub fn pack_of(root: &Path, bus: &str) -> Result<(Arc<Manifest>, bool), String> {
    let id = bus_id(root, bus);
    // (one build of a bus at a time)
    let lock = reg().building.entry(id.clone()).or_default().clone();
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
    let d = dir();
    if let Some(index) = std::fs::read(d.join("i").join(format!("{id}.json"))).ok().and_then(|b| serde_json::from_slice::<Index>(&b).ok()) {
        let side_path = d.join("p").join(format!("{}.json", index.pack));
        let file = d.join("p").join(format!("{}.b3d", index.pack));
        if let Some(side) = std::fs::read(&side_path).ok().and_then(|b| serde_json::from_slice::<Side>(&b).ok()) {
            if file.is_file() && fresh(&side.stamps) {
                // (opened now: the last to go)
                let _ = std::fs::File::options().write(true).open(&side_path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
                register(&side.sources);
                let manifest = Arc::new(side.manifest);
                reg().packs.insert(index.pack.clone(), Place::File(file, side.scrambled));
                return Ok((manifest, true));
            }
        }
    }
    let read = Arc::new(pack::read_bus(root, bus).map_err(|e| format!("{e:#}"))?);
    let built = pack::build(read).map_err(|e| format!("{e:#}"))?;
    let bytes = pack::pack_bytes(&built.head, &built.tail);
    let mut manifest = built.manifest;
    manifest.bytes = bytes.len() as u64;
    let pack_id = manifest.pack.clone();
    register(&built.sources);
    let manifest = Arc::new(manifest);
    if built.cacheable {
        let file = d.join("p").join(format!("{pack_id}.b3d"));
        let side = Side { manifest: (*manifest).clone(), scrambled: built.scrambled.clone(), sources: built.sources.clone(), stamps: built.stamps.clone() };
        let ok = write_whole(&file, &bytes).is_ok() && write_whole(&d.join("p").join(format!("{pack_id}.json")), &serde_json::to_vec(&side).unwrap_or_default()).is_ok();
        if ok {
            let _ = write_whole(&d.join("i").join(format!("{id}.json")), &serde_json::to_vec(&Index { bus: bus.to_string(), pack: pack_id.clone() }).unwrap_or_default());
            reg().packs.insert(pack_id.clone(), Place::File(file, built.scrambled.clone()));
            trim();
        } else {
            omsi_launcher_lib::log_to_file(&format!("bus3d: the pack of {bus} could not be kept in {}", d.display()));
            reg().packs.insert(pack_id.clone(), Place::Memory(Arc::new(served(bytes, &built.scrambled))));
        }
    } else {
        reg().packs.insert(pack_id.clone(), Place::Memory(Arc::new(served(bytes, &built.scrambled))));
    }
    omsi_launcher_lib::log_to_file(&format!("bus3d: {bus} read and packed in {} ms ({} triangles, {} pieces, {} textures, {:.1} MB)", built.ms, manifest.counts.triangles, manifest.counts.pieces, manifest.counts.textures, manifest.bytes as f64 / 1e6));
    reg().last = Some(Read { bus: bus.to_lowercase(), read: built.read, head: Arc::new(built.head), manifest: manifest.clone() });
    Ok((manifest, false))
}

/// A pack's bytes as the page gets them: the scrambled blocks descrambled (in memory only).
fn served(mut bytes: Vec<u8>, scrambled: &[ScrambledBlock]) -> Vec<u8> {
    if !scrambled.is_empty() {
        if let Ok((_, at)) = pack::read_pack(&bytes) {
            pack::descramble_pack(&mut bytes, at, scrambled);
        }
    }
    bytes
}

/// The bytes of pack `id` for the page.
pub fn pack_bytes(id: &str) -> Result<Vec<u8>, String> {
    let place = reg().packs.get(id).cloned().ok_or_else(|| "no such pack".to_string())?;
    match place {
        Place::Memory(b) => Ok((*b).clone()),
        Place::File(f, scrambled) => {
            let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            Ok(served(bytes, &scrambled))
        }
    }
}

/// The bus `bus` read with its pack's head, for a look: the one read last, else read again.
fn read_for_look(root: &Path, bus: &str, manifest: &Manifest) -> Result<(Arc<BusRead>, Arc<Head>), String> {
    {
        let r = reg();
        if let Some(l) = r.last.as_ref().filter(|l| l.bus == bus.to_lowercase() && l.manifest.pack == manifest.pack) {
            return Ok((l.read.clone(), l.head.clone()));
        }
    }
    let bytes = pack_bytes(&manifest.pack)?;
    let (head, _) = pack::read_pack(&bytes)?;
    let read = Arc::new(pack::read_bus(root, bus).map_err(|e| format!("{e:#}"))?);
    let head = Arc::new(head);
    reg().last = Some(Read { bus: bus.to_lowercase(), read: read.clone(), head: head.clone(), manifest: Arc::new(manifest.clone()) });
    Ok((read, head))
}

/// The look of `bus` in `paint` with the bus options `options`: kept, or worked out now.
pub fn look_of(root: &Path, bus: &str, manifest: &Manifest, paint: Option<&str>, options: &[(String, f32)]) -> Result<Look, String> {
    let mut opts: Vec<(String, f32)> = options.to_vec();
    opts.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    let key = tex::id_of(&format!("look-1|{}|{:?}", paint.unwrap_or("").trim().to_lowercase(), opts));
    let file = dir().join("l").join(format!("{}-{key}.json", manifest.pack));
    if let Some(kept) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<KeptLook>(&b).ok()) {
        if fresh(&kept.stamps) && kept.look.visible.len() == manifest.counts.entries {
            register(&kept.sources);
            return Ok(kept.look);
        }
    }
    let (read, head) = read_for_look(root, bus, manifest)?;
    let (look, sources) = look::look_of(&read, &head, manifest, paint, &opts);
    register(&sources);
    let stamps = stamps_of(&look::look_sources(&read));
    let kept = KeptLook { look: look.clone(), sources, stamps };
    let _ = write_whole(&file, &serde_json::to_vec(&kept).unwrap_or_default());
    Ok(look)
}

/// Open `bus` in `paint`: its pack (kept or built) and its look.
pub fn open(root: &Path, bus: &str, paint: Option<&str>, options: &[(String, f32)]) -> Result<Opened, String> {
    let t0 = std::time::Instant::now();
    let (manifest, from_cache) = pack_of(root, bus)?;
    let look = look_of(root, bus, &manifest, paint, options)?;
    Ok(Opened { manifest, look, from_cache, ms: t0.elapsed().as_millis() as u64 })
}

/// The pack id `bus` has now, when it is kept and its files did not change (a photo is named
/// after it).
pub fn kept_pack(root: &Path, bus: &str) -> Option<String> {
    let d = dir();
    let index: Index = serde_json::from_slice(&std::fs::read(d.join("i").join(format!("{}.json", bus_id(root, bus)))).ok()?).ok()?;
    let side: Side = serde_json::from_slice(&std::fs::read(d.join("p").join(format!("{}.json", index.pack))).ok()?).ok()?;
    fresh(&side.stamps).then_some(index.pack)
}

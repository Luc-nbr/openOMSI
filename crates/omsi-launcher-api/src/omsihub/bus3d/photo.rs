//! The bus photos the page takes with its own renderer (the old app's photo v4: transparent,
//! seen from the front corner at the door side, 215° round and 8° up, the bus 88 % of the
//! width, a soft contact shadow under it), kept as PNGs beside the game-taken ones in
//! `~/.openomsi/cache/busphotos/` under a name of their own (`p3-<id>.png`).
//!
//! A photo goes when its bus changes: beside it the files it was taken from - the bus's pack
//! (`.bus`, `model.cfg`, the meshes, the textures and their folders) and the livery's (its
//! `.cti`s, the textures it swaps) - with their sizes and times; one of them different, and
//! the photo is taken again. Those are checked once a minute per bus at most (a page of tiles
//! asks for dozens of photos of a few buses).

use super::cache;
use super::tex;
use crate::Answer;
use omsi_launcher_lib as core;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The way the photos are taken: raised when the corner, the light or the size change.
const STYLE: u32 = 3;

pub fn dir() -> PathBuf {
    core::data_dir().join("cache").join("busphotos")
}

#[derive(Serialize, Deserialize)]
struct Kept {
    bus: String,
    paint: String,
    stamps: Vec<(PathBuf, u64, u64)>,
}

fn arg_str(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The photo's name for a bus file and a livery (as the tiles name them).
pub fn name_of(bus: &str, paint: &str) -> String {
    format!("p3-{}", tex::id_of(&format!("photo-{STYLE}|{}|{}", bus.replace('\\', "/").trim().to_lowercase(), paint.trim().to_lowercase())))
}

/// Whether the files of a photo are as they were (asked at most once a minute per photo).
fn fresh(name: &str, stamps: &[(PathBuf, u64, u64)]) -> bool {
    static SEEN: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    if let Some((at, ok)) = seen.lock().unwrap_or_else(|e| e.into_inner()).get(name) {
        if at.elapsed() < Duration::from_secs(60) {
            return *ok;
        }
    }
    let ok = !stamps.is_empty() && stamps.iter().all(|(p, s, m)| tex::stamp(p).unwrap_or((u64::MAX, u64::MAX)) == (*s, *m));
    seen.lock().unwrap_or_else(|e| e.into_inner()).insert(name.to_string(), (Instant::now(), ok));
    ok
}

/// The photo of `bus` in `paint`, if there is one taken of the bus as it is now.
fn kept(bus: &str, paint: &str) -> Option<PathBuf> {
    let name = name_of(bus, paint);
    let png = dir().join(format!("{name}.png"));
    let k: Kept = serde_json::from_slice(&std::fs::read(dir().join(format!("{name}.json"))).ok()?).ok()?;
    (png.is_file() && fresh(&name, &k.stamps)).then_some(png)
}

/// `b3d_photo_has { bus, paint }`: whether a photo of the bus as it is now is kept.
pub fn has(args: &Value) -> Answer {
    Ok(json!({ "has": kept(&arg_str(args, "bus"), &arg_str(args, "paint")).is_some() }))
}

/// `b3d_photo { bus, paint }`: the kept photo's PNG.
pub fn get(args: &Value) -> Result<Vec<u8>, String> {
    let p = kept(&arg_str(args, "bus"), &arg_str(args, "paint")).ok_or_else(|| "no photo".to_string())?;
    std::fs::read(&p).map_err(|e| e.to_string())
}

/// Plain base64 (with or without padding; a `data:` URL's head is skipped).
pub fn base64(s: &str) -> Result<Vec<u8>, String> {
    let s = s.split_once("base64,").map(|x| x.1).unwrap_or(s);
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return Err("not base64".into()),
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// `b3d_photo_put { bus, paint, png }` (the PNG in base64): keep the photo the page took, with
/// the files of the bus and of the livery it was taken from.
pub fn put(args: &Value) -> Answer {
    let (bus, paint) = (arg_str(args, "bus"), arg_str(args, "paint"));
    let png = base64(&arg_str(args, "png"))?;
    if !png.starts_with(b"\x89PNG") {
        return Err("not a PNG".into());
    }
    let root = core::root_dir().map_err(|e| format!("{e:#}"))?;
    let stamps = stamps_of(&root, &bus, &paint);
    if stamps.is_empty() {
        return Err("the bus is not open".into());
    }
    let name = name_of(&bus, &paint);
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{name}.png")), &png).map_err(|e| e.to_string())?;
    let kept = Kept { bus, paint, stamps };
    std::fs::write(d.join(format!("{name}.json")), serde_json::to_vec(&kept).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}

/// The files a photo of `bus` in `paint` depends on: its kept pack's and its looks' (the
/// `.cti` files and scripts of the bus, every look of the pack's).
fn stamps_of(root: &Path, bus: &str, paint: &str) -> Vec<(PathBuf, u64, u64)> {
    let Some(pack) = cache::kept_pack(root, bus) else { return Vec::new() };
    let d = cache::dir();
    let Some(side) = std::fs::read(d.join("p").join(format!("{pack}.json"))).ok().and_then(|b| serde_json::from_slice::<cache::Side>(&b).ok()) else { return Vec::new() };
    let mut stamps = side.stamps;
    // the looks of the pack (the livery's files)
    let _ = paint;
    for e in std::fs::read_dir(d.join("l")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if !n.starts_with(&pack) {
            continue;
        }
        if let Some(v) = std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            for s in v.get("stamps").and_then(Value::as_array).into_iter().flatten() {
                if let Ok(x) = serde_json::from_value::<(PathBuf, u64, u64)>(s.clone()) {
                    if !stamps.iter().any(|y| y.0 == x.0) {
                        stamps.push(x);
                    }
                }
            }
            for s in v.get("sources").and_then(Value::as_array).into_iter().flatten() {
                if let Ok(t) = serde_json::from_value::<tex::TexSource>(s.clone()) {
                    if !stamps.iter().any(|y| y.0 == t.path) {
                        stamps.push((t.path, t.size, t.mtime));
                    }
                }
            }
        }
    }
    stamps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_read_as_the_page_writes_it() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("data:image/png;base64,aGVsbG8gd29ybGQ").unwrap(), b"hello world");
        assert!(base64("not*base64").is_err());
    }

    #[test]
    fn a_photo_is_named_after_the_bus_and_its_livery() {
        assert_eq!(name_of("Vehicles\\MAN_SD200\\SD200.bus", "BVG"), name_of("vehicles/man_sd200/sd200.bus", " bvg "));
        assert_ne!(name_of("Vehicles/MAN_SD200/SD200.bus", "BVG"), name_of("Vehicles/MAN_SD200/SD200.bus", ""));
        assert!(name_of("a", "b").starts_with("p3-"));
    }
}

//! The buses' photos on the fleet's, the dealer's and Casual's tiles: the PNGs our game-drawn
//! launcher took with the game's own renderer (`omsi-launcher-ui/src/busphoto.rs`) and keeps
//! under `~/.openomsi/cache/busphotos/`. The page asks for a bus's photo in a paint once
//! (`co_photo`) and shows the bus's initials while there is none: taking one needs the game's
//! renderer, so a bus never pictured in the game-drawn launcher keeps its initials here.

use omsi_launcher_lib as core;
use std::path::{Path, PathBuf};

/// The way the photos are taken and their size, as `busphoto.rs` names them (`STYLE`,
/// `PHOTO_W`, `PHOTO_H`): a photo of another style is not the one wanted.
const STYLE: u32 = 2;
const PHOTO_W: u32 = 560;
const PHOTO_H: u32 = 350;

/// FNV-1a, as the game-drawn launcher's `ui::id_of`.
fn id_of(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The photo's file name for a bus file (its path as resolved), a paint and the bus file's
/// modification time (`busphoto::photo_name`).
fn photo_name(path: &str, paint: &str, mtime: u64) -> String {
    let key = format!("{STYLE}|{}|{}|{mtime}|{PHOTO_W}x{PHOTO_H}", path.replace('\\', "/").to_lowercase(), paint.trim().to_lowercase());
    format!("{:016x}.png", id_of(&key))
}

fn photo_file(root: &str, bus: &str, paint: &str) -> PathBuf {
    let path = omsi_cfg::resolve_path(Path::new(root), bus);
    let mtime = core::index::mtime_ns(&path);
    core::data_dir().join("cache").join("busphotos").join(photo_name(&path.to_string_lossy(), paint, mtime))
}

/// The PNG of `bus` (its file, relative to OMSI's folder) in `paint` (empty: its own), when the
/// game-drawn launcher took it.
pub(super) fn png(bus: &str, paint: &str) -> Result<Vec<u8>, String> {
    if bus.trim().is_empty() {
        return Err("no bus".into());
    }
    let root = core::load_config().root;
    std::fs::read(photo_file(&root, bus, paint)).map_err(|_| format!("no photo of {bus}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_names_are_the_game_drawn_launchers() {
        // (FNV-1a of the key; the name changes with the paint, the file's time and its case-less path)
        let a = photo_name("C:\\OMSI 2\\Vehicles\\MAN_EN92\\EN92.bus", "", 7);
        assert_eq!(a, photo_name("c:/omsi 2/vehicles/man_en92/en92.bus", " ", 7));
        assert_ne!(a, photo_name("c:/omsi 2/vehicles/man_en92/en92.bus", "BVG", 7));
        assert_ne!(a, photo_name("c:/omsi 2/vehicles/man_en92/en92.bus", "", 8));
        assert_eq!(a.len(), 16 + 4);
        assert_eq!(id_of(""), 0xcbf29ce484222325);
        assert_eq!(id_of("a"), 0xaf63dc4c8601ec8c);
    }
}

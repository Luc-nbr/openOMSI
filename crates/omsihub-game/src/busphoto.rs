//! The bus photos of the bus step's tiles and the bus company's pages: each bus drawn once by
//! the game's own renderer - a showroom of its own, the "studio", with the bus always seen from
//! the same front corner in the same noon light - read back and kept as a PNG under
//! `~/.openomsi/cache/busphotos/`, named after the bus file, the paint and the file's
//! modification time, so that a bus that changed is photographed again and one that did not is
//! never drawn twice. The desktop launcher asks the launcher service for one bus at a time
//! (`service::photo`) and reads the files itself (`omsi-launcher-api`'s `omsihub` photos).

use std::path::PathBuf;

/// A photo's size in pixels: 16:10, a tile's photo at about twice the interface's scale.
pub const PHOTO_W: u32 = 560;
pub const PHOTO_H: u32 = 350;
/// The way the photos are taken: raised when the corner, the light or the size change, so
/// that the photos taken before are taken again.
const STYLE: u32 = 2;

/// Where the photos are kept.
pub fn cache_dir() -> PathBuf {
    omsi_launcher_lib::data_dir().join("cache").join("busphotos")
}

/// The photo's file name for a bus file (its path as resolved), a paint and the bus file's
/// modification time: a hash of the three and the photo's style.
pub fn photo_name(path: &str, paint: &str, mtime: u64) -> String {
    let key = format!("{STYLE}|{}|{}|{mtime}|{PHOTO_W}x{PHOTO_H}", path.replace('\\', "/").to_lowercase(), paint.trim().to_lowercase());
    format!("{:016x}.png", omsi_ui::kit::id_of(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_is_named_after_the_bus_its_paint_and_its_time() {
        let a = photo_name("C:\\OMSI 2\\Vehicles\\MAN_SD200\\SD200.bus", "", 1);
        assert_eq!(a, photo_name("c:/omsi 2/vehicles/man_sd200/sd200.bus", "", 1), "the same file however it is written");
        assert_ne!(a, photo_name("C:\\OMSI 2\\Vehicles\\MAN_SD200\\SD200.bus", "BVG", 1), "another paint, another photo");
        assert_ne!(a, photo_name("C:\\OMSI 2\\Vehicles\\MAN_SD200\\SD200.bus", "", 2), "a changed bus is photographed again");
        assert!(a.ends_with(".png") && a.len() == 20);
    }
}

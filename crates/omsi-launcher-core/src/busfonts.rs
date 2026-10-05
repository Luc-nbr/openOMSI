//! The display fonts the player chose in the launcher's bus step: a `.oft` font (by its
//! `[newfont]` name) that the bus's destination displays are drawn in instead of the matrix
//! fonts the bus came with - a realistic sign font for a bus whose maker drew its own. They are
//! kept per bus file in `~/.openomsi/bus-fonts.json`, beside the bus options (a bus keeps its
//! own whatever is driven in between), with a default for every bus that has none of its own,
//! and go to the game as `--display-font` (see `duty_args`).
//!
//! `BusFonts::font_for` is also what an AI bus of the same file would take - the company's
//! buses on the player's own lines, once the game is told which those are (not yet: only the
//! player's bus gets it).

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::busoptions::bus_key;

/// The choices: per bus file (see `bus_key`) the font's name, "" for the bus's own whatever
/// the default; and the default.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BusFonts {
    /// The font of every bus without a choice of its own (none: each as the bus has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    buses: BTreeMap<String, String>,
}

impl BusFonts {
    /// Where they are kept.
    pub fn path() -> PathBuf {
        crate::data_dir().join("bus-fonts.json")
    }

    /// The choices kept (none when there is no file or it cannot be read).
    pub fn load() -> BusFonts {
        Self::read(&Self::path())
    }

    pub fn read(path: &Path) -> BusFonts {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.write(&Self::path())
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The font `bus`'s destination displays are drawn in: its own choice, else the default;
    /// None as the bus has them.
    pub fn font_for(&self, bus: &str) -> Option<String> {
        match self.buses.get(&bus_key(bus)) {
            Some(f) => Some(f.trim().to_string()).filter(|f| !f.is_empty()),
            None => self.default.as_deref().map(str::trim).filter(|f| !f.is_empty()).map(str::to_string),
        }
    }

    /// The choice made for `bus` itself: None when there is none (the default holds),
    /// Some("") when it is to be as the bus whatever the default.
    pub fn chosen(&self, bus: &str) -> Option<&str> {
        self.buses.get(&bus_key(bus)).map(|s| s.as_str())
    }

    /// Choose `font` for `bus`; None: as the bus (kept as "" while a default would say
    /// otherwise, else nothing kept).
    pub fn set(&mut self, bus: &str, font: Option<&str>) {
        let key = bus_key(bus);
        match font.map(str::trim).filter(|f| !f.is_empty()) {
            Some(f) => {
                self.buses.insert(key, f.to_string());
            }
            None if self.default.is_some() => {
                self.buses.insert(key, String::new());
            }
            None => {
                self.buses.remove(&key);
            }
        }
    }

    /// The font of every bus without a choice of its own (None: each as the bus).
    pub fn set_default(&mut self, font: Option<&str>) {
        self.default = font.map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
        if self.default.is_none() {
            // ("as the bus" was only kept to say no to the default)
            self.buses.retain(|_, f| !f.is_empty());
        }
    }
}

/// What `add_font` brought.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddedFont {
    /// The fonts in the file (`[newfont]` names), in its order.
    pub names: Vec<String>,
    /// The bitmaps it names that were not beside it (the font is there, but draws nothing
    /// until they are).
    pub missing: Vec<String>,
}

/// Copy the `.oft` font file `oft` into `fonts_dir` - the `Fonts` folder of openOMSI's
/// content folder (`content_dir`), never the OMSI installation's - with the bitmaps its
/// `[newfont]` blocks name (looked for beside it as the game looks for them, in folders below
/// it too), so that the game and the launcher find it among the installed fonts.
pub fn add_font(oft: &Path, fonts_dir: &Path) -> Result<AddedFont> {
    if !oft.extension().is_some_and(|x| x.eq_ignore_ascii_case("oft")) {
        return Err(anyhow!("{} is no .oft font", oft.display()));
    }
    let fonts = omsi_content::font::Font::load_all(oft).map_err(|e| anyhow!("{}: {e}", oft.display()))?;
    if fonts.is_empty() {
        return Err(anyhow!("{} defines no font", oft.display()));
    }
    let from = oft.parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::create_dir_all(fonts_dir).with_context(|| format!("creating {}", fonts_dir.display()))?;
    let name = oft.file_name().context("a font file without a name")?;
    let target = fonts_dir.join(name);
    // (the same file picked from where it already is: nothing to copy)
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    if !same(oft, &target) {
        std::fs::copy(oft, &target).with_context(|| format!("copying {} to {}", oft.display(), target.display()))?;
    }
    let mut out = AddedFont { names: fonts.iter().map(|f| f.name.trim().to_string()).collect(), missing: Vec::new() };
    let mut done: Vec<String> = Vec::new();
    for rel in fonts.iter().flat_map(|f| [f.bitmap.trim(), f.alpha.trim()]) {
        let key = rel.replace('\\', "/").to_ascii_lowercase();
        if rel.is_empty() || done.contains(&key) {
            continue;
        }
        done.push(key);
        let src = omsi_cfg::resolve_path(&from, rel);
        if !src.is_file() {
            out.missing.push(rel.to_string());
            continue;
        }
        // (where the font file says, below the Fonts folder: no way out of it)
        let parts: Vec<&str> = rel.split(['/', '\\']).filter(|p| !p.is_empty() && *p != "." && *p != "..").collect();
        let dst = parts.iter().fold(fonts_dir.to_path_buf(), |p, s| p.join(s));
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
        }
        if !same(&src, &dst) {
            std::fs::copy(&src, &dst).with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
        }
    }
    omsi_cfg::content_changed();
    Ok(out)
}

/// The content folder's `Fonts`, where `add_font` puts a font (none without a content folder).
pub fn content_fonts_dir() -> Option<PathBuf> {
    crate::content_dir().map(|c| c.join("Fonts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_is_kept_per_bus_file_with_a_default_beside() {
        let mut f = BusFonts::default();
        assert_eq!(f.font_for("Vehicles/MAN_SD200/SD200.bus"), None, "nothing chosen: as the bus");
        f.set("Vehicles\\MAN_SD200\\SD200.bus", Some("Annax Small"));
        assert_eq!(f.font_for("vehicles/man_sd200/sd200.bus").as_deref(), Some("Annax Small"), "whatever the spelling");
        assert_eq!(f.chosen("Vehicles/MAN_SD200/SD200.bus"), Some("Annax Small"));
        // a default for the others; the SD200 keeps its own
        f.set_default(Some("Krueger 16x9"));
        assert_eq!(f.font_for("Vehicles/HH20_EBus2021/HHEBus2021_main.bus").as_deref(), Some("Krueger 16x9"));
        assert_eq!(f.font_for("Vehicles/MAN_SD200/SD200.bus").as_deref(), Some("Annax Small"));
        // "as the bus" against a default is kept as a choice of its own
        f.set("Vehicles/MAN_SD200/SD200.bus", None);
        assert_eq!(f.font_for("Vehicles/MAN_SD200/SD200.bus"), None);
        assert_eq!(f.chosen("Vehicles/MAN_SD200/SD200.bus"), Some(""));
        // without the default it is nothing kept at all
        f.set_default(None);
        assert_eq!(f.chosen("Vehicles/MAN_SD200/SD200.bus"), None);
        assert_eq!(f, BusFonts::default());
        // a blank name is no font
        f.set("Vehicles/x.bus", Some("  "));
        assert_eq!(f, BusFonts::default());
    }

    #[test]
    fn the_choices_survive_in_their_file() {
        let dir = std::env::temp_dir().join(format!("omsi_bus_fonts_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bus-fonts.json");
        let mut f = BusFonts::default();
        f.set("Vehicles/MAN_NL_NG/NL202.bus", Some("Annax Medium D"));
        f.set_default(Some("Annax Small"));
        f.write(&file).unwrap();
        assert_eq!(BusFonts::read(&file), f);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"vehicles/man_nl_ng/nl202.bus\": \"Annax Medium D\"") && text.contains("\"default\": \"Annax Small\""), "{text}");
        // a file of before the default, and one that is no file of this
        std::fs::write(&file, r#"{"buses":{"vehicles/x.bus":"A"}}"#).unwrap();
        assert_eq!(BusFonts::read(&file).font_for("Vehicles/X.bus").as_deref(), Some("A"));
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(BusFonts::read(&file), BusFonts::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_font_comes_into_the_content_folder_with_its_bitmaps() {
        let base = std::env::temp_dir().join(format!("omsi_add_font_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("download");
        std::fs::create_dir_all(src.join("bmp")).unwrap();
        std::fs::write(
            src.join("MyLED.oft"),
            "[newfont]\nMyLED 7\nbmp\\myled.bmp\nbmp\\myled_a.bmp\n7\n1\n\n[char]\nA\n0\n5\n0\n\n[newfont]\nMyLED 16\nmyled16.bmp\nmissing_alpha.bmp\n16\n2\n",
        )
        .unwrap();
        std::fs::write(src.join("bmp").join("myled.bmp"), b"BM").unwrap();
        std::fs::write(src.join("bmp").join("myled_a.bmp"), b"BM").unwrap();
        std::fs::write(src.join("myled16.bmp"), b"BM").unwrap();
        let fonts = base.join("content").join("Fonts");
        let added = add_font(&src.join("MyLED.oft"), &fonts).unwrap();
        assert_eq!(added.names, ["MyLED 7", "MyLED 16"]);
        assert_eq!(added.missing, ["missing_alpha.bmp"]);
        assert!(fonts.join("MyLED.oft").is_file());
        assert!(fonts.join("bmp").join("myled.bmp").is_file() && fonts.join("bmp").join("myled_a.bmp").is_file() && fonts.join("myled16.bmp").is_file());
        // added again (the same font picked twice, or from its new place): the same
        assert_eq!(add_font(&fonts.join("MyLED.oft"), &fonts).unwrap().names, added.names);
        // not a font
        std::fs::write(src.join("notes.txt"), "x").unwrap();
        assert!(add_font(&src.join("notes.txt"), &fonts).is_err());
        std::fs::write(src.join("empty.oft"), "nothing here\n").unwrap();
        assert!(add_font(&src.join("empty.oft"), &fonts).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}

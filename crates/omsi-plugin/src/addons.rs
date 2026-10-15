//! Add-ons: plugins the game offers to download and switch on from its own options, listed in
//! a catalogue (`addons.json`, a list of [`Addon`]s).
//!
//! An add-on is a Lua or WebAssembly plugin packed in a zip archive: switched on the first
//! time, the archive is downloaded (by the game, see `omsi-app`'s `addons`), checked against the
//! catalogue's SHA-256 and unpacked into `plugins/<id>/`; switched off, the plugin is put on the
//! folder's `disabled.txt` ([`crate::switch`]) and its files stay for the next time. An archive
//! may hold only what such a plugin is made of - no program or library (`.dll`, `.exe`, `.so`,
//! `.opl` ...), so an add-on runs in the plugins' sandbox like every Lua or WASM plugin.

use crate::api::paths;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Most an add-on's archive may weigh, packed and unpacked.
const MAX_ARCHIVE: usize = 64 << 20;
const MAX_UNPACKED: u64 = 256 << 20;

/// What an add-on's archive may hold: code the plugins' sandbox runs and the files it reads.
const ALLOWED: &[&str] = &["lua", "oop", "json", "txt", "md", "csv", "cfg", "png", "jpg", "jpeg", "bmp", "tga", "dds", "wav", "ogg", "mp3", "ttf", "otf"];

/// One add-on of the catalogue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Addon {
    /// Its plugin's name and folder: lower-case letters, digits, `-` and `_`.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Where to read more about it.
    pub homepage: String,
    pub version: String,
    /// The archive: an `https://` address and its SHA-256 (64 hexadecimal digits).
    pub url: String,
    pub sha256: String,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Read a catalogue: a JSON list of `{id, name, description, homepage, version, url,
/// sha256}`. An entry that is not right is left out with the reason in the log, so one bad
/// entry does not hide the others.
pub fn parse_catalogue(text: &str) -> Vec<Addon> {
    let list = match crate::api::json::decode(text, false) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("add-on catalogue: {e}");
            return Vec::new();
        }
    };
    let mut out: Vec<Addon> = Vec::new();
    for (i, e) in list.items().iter().enumerate() {
        let text = |k: &str| e.get(k).and_then(|v| v.to_text()).unwrap_or_default();
        let a = Addon { id: text("id"), name: text("name"), description: text("description"), homepage: text("homepage"), version: text("version"), url: text("url"), sha256: text("sha256").to_ascii_lowercase() };
        let why = if !valid_id(&a.id) {
            "its id is not lower-case letters, digits, - and _"
        } else if a.name.trim().is_empty() {
            "it has no name"
        } else if !a.url.starts_with("https://") {
            "its address is not https://"
        } else if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            "its sha256 is not 64 hexadecimal digits"
        } else if out.iter().any(|o| o.id == a.id) {
            "its id is there twice"
        } else {
            out.push(a);
            continue;
        };
        log::warn!("add-on catalogue: entry {} left out: {why}", i + 1);
    }
    out
}

/// Whether the add-on's plugin is in the plugins folder (switched on or off).
pub fn installed(plugins: &Path, a: &Addon) -> bool {
    plugins.join(&a.id).is_dir()
}

/// Whether the add-on is switched on: there, and not on `disabled.txt`.
pub fn enabled(plugins: &Path, a: &Addon) -> bool {
    installed(plugins, a) && !crate::switch::disabled(plugins).contains(&a.id)
}

/// Switch an installed add-on on or off (from the game's next start).
pub fn set_enabled(plugins: &Path, a: &Addon, on: bool) -> std::io::Result<()> {
    crate::switch::set(plugins, &a.id, on)
}

/// Unpack the add-on's downloaded archive into `plugins/<id>/` and switch it on: checked
/// against the catalogue's SHA-256, every path inside the folder, only the files a plugin is
/// made of. Unpacked beside the folder first, so a failure leaves the old one as it was.
pub fn install(plugins: &Path, a: &Addon, archive: &[u8]) -> Result<(), String> {
    if archive.len() > MAX_ARCHIVE {
        return Err("the archive is larger than 64 MB".into());
    }
    let sum: String = Sha256::digest(archive).iter().map(|b| format!("{b:02x}")).collect();
    if sum != a.sha256 {
        return Err(format!("the archive is not the one the catalogue names (SHA-256 {sum})"));
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| format!("not a zip archive: {e}"))?;
    let new = plugins.join(format!(".{}.new", a.id));
    let _ = std::fs::remove_dir_all(&new);
    let mut unpack = || -> Result<(), String> {
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            if f.is_dir() {
                continue;
            }
            let rel = paths::clean(f.name())?;
            let ext = Path::new(&rel).extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if !ALLOWED.contains(&ext.as_str()) {
                return Err(format!("{rel}: an add-on holds no files of this kind"));
            }
            total += f.size();
            if total > MAX_UNPACKED {
                return Err("larger than 256 MB unpacked".into());
            }
            let mut bytes = Vec::new();
            f.by_ref().take(MAX_UNPACKED).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            let to = new.join(&rel);
            std::fs::create_dir_all(to.parent().unwrap_or(&new)).map_err(|e| e.to_string())?;
            std::fs::write(&to, bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    if let Err(e) = unpack() {
        let _ = std::fs::remove_dir_all(&new);
        return Err(e);
    }
    let dir = plugins.join(&a.id);
    // (the plugin's own data folder - its storage, its settings - is kept)
    let data = dir.join("data");
    if data.is_dir() {
        let _ = std::fs::rename(&data, new.join("data"));
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&new, &dir).map_err(|e| e.to_string())?;
    crate::switch::set(plugins, &a.id, true).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, text) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn addon(archive: &[u8]) -> Addon {
        let sha256 = Sha256::digest(archive).iter().map(|b| format!("{b:02x}")).collect();
        Addon { id: "fleet".into(), name: "Fleet".into(), url: "https://example.org/fleet.zip".into(), sha256, ..Addon::default() }
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsi-plugin-addons-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_catalogue_keeps_the_right_entries() {
        let sum = "a".repeat(64);
        let text = format!(
            r#"[{{"id":"fleet","name":"Fleet","url":"https://x/f.zip","sha256":"{sum}","version":"1.2"}},
               {{"id":"Bad Id","name":"x","url":"https://x","sha256":"{sum}"}},
               {{"id":"plain","name":"Plain","url":"http://x/p.zip","sha256":"{sum}"}},
               {{"id":"short","name":"Short","url":"https://x/s.zip","sha256":"abc"}},
               {{"id":"fleet","name":"Again","url":"https://x/f.zip","sha256":"{sum}"}}]"#
        );
        let c = parse_catalogue(&text);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].id.as_str(), c[0].version.as_str()), ("fleet", "1.2"));
        assert!(parse_catalogue("not json").is_empty());
    }

    #[test]
    fn an_addon_is_checked_unpacked_and_switched() {
        let plugins = dir("install");
        let archive = zip_of(&[("main.lua", "omsi.log('fleet')"), ("lib/util.lua", "return {}"), ("icon.png", "png")]);
        let a = addon(&archive);
        assert!(!installed(&plugins, &a));
        install(&plugins, &a, &archive).unwrap();
        assert!(enabled(&plugins, &a));
        assert!(plugins.join("fleet/lib/util.lua").is_file());
        set_enabled(&plugins, &a, false).unwrap();
        assert!(installed(&plugins, &a) && !enabled(&plugins, &a));
        // an update keeps the plugin's data and switches it on again
        std::fs::create_dir_all(plugins.join("fleet/data")).unwrap();
        std::fs::write(plugins.join("fleet/data/storage.json"), "{}").unwrap();
        let newer = zip_of(&[("main.lua", "omsi.log('fleet 2')")]);
        install(&plugins, &addon(&newer), &newer).unwrap();
        assert!(enabled(&plugins, &a));
        assert!(plugins.join("fleet/data/storage.json").is_file());
        assert!(!plugins.join("fleet/lib").exists());
        // and the game loads it
        let names: Vec<String> = crate::Plugins::load(std::slice::from_ref(&plugins), &crate::HostConfig::default()).lua.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, ["fleet"]);
    }

    #[test]
    fn a_wrong_or_unsafe_archive_changes_nothing() {
        let plugins = dir("refused");
        let good = zip_of(&[("main.lua", "-- v1")]);
        install(&plugins, &addon(&good), &good).unwrap();
        // (another archive than the catalogue names)
        let other = zip_of(&[("main.lua", "-- evil")]);
        assert!(install(&plugins, &addon(&good), &other).unwrap_err().contains("SHA-256"));
        // (a library, a path out of the folder)
        for files in [vec![("main.lua", "x"), ("hook.dll", "MZ")], vec![("../escape.lua", "x")]] {
            let bad = zip_of(&files);
            assert!(install(&plugins, &addon(&bad), &bad).is_err());
        }
        assert_eq!(std::fs::read_to_string(plugins.join("fleet/main.lua")).unwrap(), "-- v1");
        assert!(!plugins.join(".fleet.new").exists() && !plugins.join("escape.lua").exists());
    }
}

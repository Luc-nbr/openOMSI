//! The plugin put into an openOMSI's plugins folder, switched on and off.
//!
//! The plugin is a folder plugin, `Plugins/omsihub/` (`main.lua` and its module), which every
//! openOMSI with Lua plugins loads by itself; `omsihub-plugin.json` beside it says which version
//! Omsi-Hub put there. Installing writes only the files that changed (openOMSI loads a folder
//! plugin again when one of its `.lua` files changes) and never touches the assignment or the
//! plugin's `data` folder.
//!
//! Switched off, the folder is parked out of the plugins folder - in
//! `Mods/plugins-not-enabled/`, where the launcher's mod installer puts the plugins it does not
//! switch on - which works with every openOMSI. A game that has a switch of its own for each
//! plugin (`Plugins/disabled.txt`, one plugin name a line: the extension point proposed in
//! docs/omsihub/ADDON_PLAN.md) is read as well: a plugin listed there is off, and switching it
//! on takes it off the list.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The plugin's name: its folder in the plugins folder.
pub const NAME: &str = "omsihub";
/// The plugin's version; `lua/main.lua` says the same (a test keeps the two the same).
pub const VERSION: &str = "1.0.0";
/// The plugin's files.
pub const FILES: &[(&str, &str)] = &[("main.lua", include_str!("../lua/main.lua")), ("hubjson.lua", include_str!("../lua/hubjson.lua"))];
/// What Omsi-Hub put there, beside the plugin's files.
pub const STAMP: &str = "omsihub-plugin.json";
/// A game's list of the plugins switched off, in its plugins folder.
pub const DISABLED_LIST: &str = "disabled.txt";

/// The plugin in a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Not there.
    Missing,
    /// There and switched on, this version.
    On,
    /// There and switched on, an older (or newer) version than this Omsi-Hub's.
    Outdated,
    /// There and switched off.
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub state: State,
    /// The version installed, where the stamp says it.
    pub version: Option<String>,
    /// The plugin's folder when it is on (the assignment goes there).
    pub dir: PathBuf,
}

#[derive(serde::Deserialize, Serialize)]
struct Stamp {
    #[serde(default)]
    version: String,
    #[serde(default)]
    bridge: u32,
}

/// The plugin's folder in a plugins folder.
pub fn plugin_dir(plugins: &Path) -> PathBuf {
    plugins.join(NAME)
}

/// Where the plugin waits while it is switched off: the content folder's
/// `Mods/plugins-not-enabled/omsihub`.
pub fn parked_dir(plugins: &Path) -> PathBuf {
    plugins.parent().unwrap_or(plugins).join("Mods").join("plugins-not-enabled").join(NAME)
}

/// The names a plugins folder's `disabled.txt` lists (lower case; blank lines and `#` comments
/// left out).
pub fn disabled_names(plugins: &Path) -> Vec<String> {
    std::fs::read_to_string(plugins.join(DISABLED_LIST))
        .map(|t| t.lines().map(|l| l.split('#').next().unwrap_or("").trim().to_lowercase()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn stamp_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(STAMP)).ok()?;
    serde_json::from_str::<Stamp>(&text).ok().map(|s| s.version).filter(|v| !v.is_empty())
}

/// The plugin in the plugins folder `plugins`.
pub fn status(plugins: &Path) -> Status {
    let dir = plugin_dir(plugins);
    if dir.join("main.lua").is_file() {
        let version = stamp_version(&dir);
        let state = if disabled_names(plugins).iter().any(|n| n == NAME) {
            State::Off
        } else if version.as_deref() == Some(VERSION) {
            State::On
        } else {
            State::Outdated
        };
        return Status { state, version, dir };
    }
    let parked = parked_dir(plugins);
    if parked.join("main.lua").is_file() {
        return Status { state: State::Off, version: stamp_version(&parked), dir };
    }
    Status { state: State::Missing, version: None, dir }
}

/// Put this version of the plugin into `plugins` and switch it on (a parked one comes back
/// with its data first).
pub fn install(plugins: &Path) -> std::io::Result<Status> {
    let dir = plugin_dir(plugins);
    let parked = parked_dir(plugins);
    if !dir.exists() && parked.is_dir() {
        std::fs::create_dir_all(plugins)?;
        std::fs::rename(&parked, &dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    for (name, text) in FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            std::fs::write(&path, text)?;
        }
    }
    let stamp = serde_json::to_string_pretty(&Stamp { version: VERSION.into(), bridge: crate::bridge::BRIDGE }).unwrap_or_default();
    std::fs::write(dir.join(STAMP), stamp)?;
    set_listed(plugins, false)?;
    Ok(status(plugins))
}

/// Switch the plugin off: park its folder (and its data) out of the plugins folder.
pub fn disable(plugins: &Path) -> std::io::Result<Status> {
    let dir = plugin_dir(plugins);
    if dir.is_dir() {
        let parked = parked_dir(plugins);
        if parked.exists() {
            std::fs::remove_dir_all(&parked)?;
        }
        std::fs::create_dir_all(parked.parent().unwrap_or(plugins))?;
        std::fs::rename(&dir, &parked)?;
    }
    Ok(status(plugins))
}

/// Take the plugin away altogether, its data with it.
pub fn remove(plugins: &Path) -> std::io::Result<Status> {
    for d in [plugin_dir(plugins), parked_dir(plugins)] {
        if d.is_dir() {
            std::fs::remove_dir_all(&d)?;
        }
    }
    set_listed(plugins, false)?;
    Ok(status(plugins))
}

/// Put the plugin on the game's `disabled.txt` or take it off (a list that is not there is
/// made only to put it on).
fn set_listed(plugins: &Path, on: bool) -> std::io::Result<()> {
    let path = plugins.join(DISABLED_LIST);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let listed = |l: &str| l.split('#').next().unwrap_or("").trim().eq_ignore_ascii_case(NAME);
    let has = text.lines().any(listed);
    if has == on {
        return Ok(());
    }
    let mut lines: Vec<&str> = text.lines().filter(|l| !listed(l)).collect();
    if on {
        lines.push(NAME);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsihub-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn installed_switched_off_and_on_with_its_data() {
        let content = dir("cycle");
        let plugins = content.join("Plugins");
        assert_eq!(status(&plugins).state, State::Missing);
        let s = install(&plugins).unwrap();
        assert_eq!((s.state, s.version.as_deref()), (State::On, Some(VERSION)));
        assert!(plugins.join("omsihub/main.lua").is_file() && plugins.join("omsihub/hubjson.lua").is_file());
        std::fs::create_dir_all(plugins.join("omsihub/data")).unwrap();
        std::fs::write(plugins.join("omsihub/data/reports.jsonl"), "{}\n").unwrap();
        // off: parked with its data, nothing left for the game to load
        assert_eq!(disable(&plugins).unwrap().state, State::Off);
        assert!(!plugins.join("omsihub").exists());
        assert!(content.join("Mods/plugins-not-enabled/omsihub/data/reports.jsonl").is_file());
        // on again: back with its data
        assert_eq!(install(&plugins).unwrap().state, State::On);
        assert!(plugins.join("omsihub/data/reports.jsonl").is_file());
        assert_eq!(remove(&plugins).unwrap().state, State::Missing);
    }

    #[test]
    fn an_older_plugin_is_outdated_and_updated_in_place() {
        let plugins = dir("old").join("Plugins");
        install(&plugins).unwrap();
        let d = plugin_dir(&plugins);
        std::fs::write(d.join(STAMP), "{\"version\":\"0.9.0\"}").unwrap();
        std::fs::write(d.join("main.lua"), "-- old").unwrap();
        std::fs::write(d.join("assignment.lua"), "return nil").unwrap();
        assert_eq!(status(&plugins).state, State::Outdated);
        assert_eq!(install(&plugins).unwrap().state, State::On);
        assert_eq!(std::fs::read_to_string(d.join("main.lua")).unwrap(), FILES[0].1);
        // (the assignment stays)
        assert!(d.join("assignment.lua").is_file());
    }

    #[test]
    fn the_game_s_own_switch_is_read_and_cleared() {
        let plugins = dir("listed").join("Plugins");
        install(&plugins).unwrap();
        std::fs::write(plugins.join(DISABLED_LIST), "# switched off in the game\nOmsiHub\nother\n").unwrap();
        assert_eq!(disabled_names(&plugins), vec!["omsihub", "other"]);
        assert_eq!(status(&plugins).state, State::Off);
        assert_eq!(install(&plugins).unwrap().state, State::On);
        assert_eq!(std::fs::read_to_string(plugins.join(DISABLED_LIST)).unwrap(), "# switched off in the game\nother\n");
    }

    #[test]
    fn the_plugin_and_omsihub_agree_on_its_version() {
        assert!(FILES[0].1.contains(&format!("local VERSION = \"{VERSION}\"")));
        assert!(FILES[0].1.contains(&format!("local BRIDGE = {}", crate::bridge::BRIDGE)));
    }
}

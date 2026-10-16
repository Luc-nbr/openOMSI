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
pub const VERSION: &str = "1.6.0";
/// The plugin's files.
pub const FILES: &[(&str, &str)] = &[
    ("main.lua", include_str!("../lua/main.lua")),
    ("hubjson.lua", include_str!("../lua/hubjson.lua")),
    ("career.lua", include_str!("../lua/career.lua")),
    ("trip.lua", include_str!("../lua/trip.lua")),
    ("card.lua", include_str!("../lua/card.lua")),
    ("watch.lua", include_str!("../lua/watch.lua")),
    ("board.lua", include_str!("../lua/board.lua")),
    ("signon.lua", include_str!("../lua/signon.lua")),
    ("company.lua", include_str!("../lua/company.lua")),
];
/// The texts the plugin shows (`tr` in `lua/main.lua`), in English: Omsi-Hub puts their
/// translations into the assignment (`bridge::Assignment::texts`).
pub const TEXTS: &[&str] = &[
    "On duty: trip %{trip} of %{trips}",
    "Not started yet",
    "Duty %{duty}: line %{line}, tour %{tour}",
    "Start duty",
    "F10: use the panel with the mouse",
    "The duty could not be started: %{reason}",
    // the trip's card (`lua/card.lua`), as Omsi-Hub's own game says it
    "%{m} min",
    "%{h}h %{m}m",
    "Free drive",
    "trip %{k} of %{n}",
    "Trip completed",
    "Trip ended",
    "Overview",
    "Stops",
    "Bus stop",
    "Due",
    "Arr.",
    "Dep.",
    "Punctuality",
    "%{good} of %{all} stops on time",
    "Early",
    "Late",
    "Worst delay",
    "Average delay",
    "Driving time",
    "Distance",
    "No timetable: nothing to be early or late for",
    "No stop of this trip was served",
    "Stops served",
    "Jolts",
    "Collisions",
    "Passengers",
    "Excellent",
    "Good",
    "Fair",
    "Poor",
    "Bad",
    "+%{n} XP",
    "Comfort",
    "Safety",
    "%{n} red light",
    "%{n} red lights",
    "%{n} speed camera",
    "%{n} speed cameras",
    "Fines %{amount}",
    "No fines",
    "Saved to your service record",
    "The trip could not be saved",
    "Enter or × to close",
    "Click to keep it open",
    // the drive watch's notices (`lua/watch.lua`)
    "Red light",
    "You crossed the stop line on red · fine %{amount}",
    "Speed camera",
    "%{speed} km/h where %{limit} is allowed · fine %{amount}",
    "%{speed} mph where %{limit} is allowed · fine %{amount}",
    // the duty board (`lua/board.lua`)
    "Duty finished",
    "break, %{m} min left",
    "%{m} min over",
    "leaves in %{m} min",
    "on time",
    "Empty run",
    "Continue on line %{line}, tour %{tour}",
    "and %{n} more to %{terminus}",
    "Next %{time}",
    "%{m} min break",
    "arrives %{time}",
    "leaves %{time}",
    "terminus",
    // the board's company parts (`lua/company.lua`) and their switches (`lua/main.lua`)
    "On the duty board",
    "Punctuality coach",
    "Duty strip",
    "Service control messages",
    "Fare box",
    "Contract",
    "Hold %{s} here · leave at %{time}",
    "Leaving now counts as early",
    "Running %{s} early · ease off",
    "Late %{s} · last trip of the duty",
    "Late %{s} · the %{m} min layover brings the %{time} back on time",
    "Late %{s} · still %{left} late for the %{time} after the layover",
    "Break until %{time} · %{place}",
    "Break %{from}–%{to} · %{place}",
    "Relief %{time} · %{place}",
    "Sign off %{time} · %{place}",
    "Service control",
    "Short-turn the %{time} trip: turn back early (%{m} min saved)",
    "Run the %{time} trip out of service to the terminus (%{m} min saved)",
    "The layover before the %{time} trip is cut by %{m} min",
    "Bus %{bus} takes over from the %{time} trip",
    "Another bus takes over from the %{time} trip",
    "Your break at %{place}: %{from}–%{to}",
    "You are relieved at %{place} at %{time}",
    "Fare box %{amount} · %{n} boarded this trip · %{total} today",
    "Gross cost contract",
    "Net cost contract",
    "Hire contract",
    "Green",
    "Amber",
    "Red",
    "%{kind} · rated %{rag} · on time −%{early} to +%{late} min",
    // signing on (`lua/signon.lua`)
    "Not signed on",
    "Personnel number",
    "Code",
    "Not known here. Your number and code are on your pass below.",
    "Sign on",
    "Your pass",
    "Personnel no. %{number} · code %{code}",
    "Duty assignment",
    "Line",
    "Tour",
    "Departure",
    "The duty's codes go to the IBIS as soon as you accept it.",
    "Accept duty",
    "Sign off",
];
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

    /// The quoted texts in a piece of Lua.
    fn literals(lua: &str) -> Vec<&str> {
        lua.split('"').skip(1).step_by(2).collect()
    }

    /// The texts of the first argument of each `tr(...)` of the plugin's files (also those of
    /// `tr(cond and "a" or "b")`).
    fn translated() -> Vec<&'static str> {
        let mut out = Vec::new();
        for (_, lua) in FILES {
            for (at, _) in lua.match_indices("tr(") {
                if lua[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
                let rest = &lua[at + 3..];
                let (mut depth, mut quoted, mut end) = (0, false, rest.len());
                for (i, c) in rest.char_indices() {
                    match c {
                        '"' => quoted = !quoted,
                        '(' | '{' if !quoted => depth += 1,
                        ')' | '}' if !quoted && depth > 0 => depth -= 1,
                        ')' | ',' if !quoted && depth == 0 => {
                            end = i;
                            break;
                        }
                        _ => {}
                    }
                }
                out.extend(literals(&rest[..end]));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn every_text_of_the_plugin_is_listed() {
        // what `tr` is given as it is, listed; what is listed, in the plugin (a label handed
        // to `tr` through a variable is found as it is written)
        let missing: Vec<&str> = translated().into_iter().filter(|t| !TEXTS.contains(t)).collect();
        assert!(missing.is_empty(), "translated but not in TEXTS: {missing:?}");
        let all: Vec<&str> = FILES.iter().flat_map(|(_, lua)| literals(lua)).collect();
        let unused: Vec<&&str> = TEXTS.iter().filter(|t| !all.contains(t)).collect();
        assert!(unused.is_empty(), "in TEXTS but not in the plugin: {unused:?}");
    }

    #[test]
    fn the_plugin_and_omsihub_agree_on_its_version() {
        assert!(FILES[0].1.contains(&format!("local VERSION = \"{VERSION}\"")));
        assert!(FILES[0].1.contains(&format!("local BRIDGE = {}", crate::bridge::BRIDGE)));
    }
}

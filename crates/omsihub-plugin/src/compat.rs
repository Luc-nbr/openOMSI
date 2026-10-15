//! Which openOMSI the add-on works with, and how well: the plugin API's version negotiated.
//!
//! Two looks, the second one winning:
//!
//! * before the game ever ran with the plugin, the version read from the openOMSI folder
//!   ([`estimate`]): every public release since 0.2.0 has the plugin API this add-on is written
//!   against (ABI 1); an older one is refused, a version not found is "checked at the first
//!   drive";
//! * once the plugin ran, its handshake ([`Hello`], `data/hello.json`): the API's ABI the game
//!   said (`omsi.game.api()`) and, per function the add-on uses, whether the game has it (the
//!   plugin looks each one up in the `omsi` table, so this works on games older than
//!   `game.has` too). A [`Feature`] whose functions are all there is on; one that is not
//!   degrades as its `fallback` says, and only an essential one missing makes the game
//!   unsupported.

use crate::bridge::Hello;
use crate::version::GameVersion;
use serde::Serialize;

/// The plugin API's ABI this add-on is written against (`omsi_plugin::api::ABI`, raised by
/// openOMSI only when something is taken away or changes).
pub const API_ABI: i64 = 1;

/// The first public openOMSI release: the plugin API with Lua plugins, the duty, the panels and
/// the plugin's own files (ABI 1) is in every release from here on.
pub const FIRST_SUPPORTED: GameVersion = GameVersion::new(0, 2, 0);

/// One thing the add-on does in the game, and the plugin API functions it needs.
#[derive(Debug, Clone, Copy)]
pub struct Feature {
    pub id: &'static str,
    /// What it is, for the player (English: the launcher translates it).
    pub label: &'static str,
    /// The API functions (as `omsi.game.has` names them) it needs, every one.
    pub needs: &'static [&'static str],
    /// Without it the add-on is of no use in that game.
    pub essential: bool,
    /// What happens without it (English, for the player).
    pub fallback: &'static str,
}

/// Everything the plugin does, in the order the launcher lists it. `lua/main.lua` looks up the
/// same functions for its handshake (a test keeps the two lists the same).
pub static FEATURES: &[Feature] = &[
    Feature { id: "duty", label: "The assigned duty and its trips", needs: &["duty.get", "duty.trips"], essential: true, fallback: "" },
    Feature { id: "reports", label: "Trip results back to the company", needs: &["files.append", "files.write"], essential: false, fallback: "Trip results are read from the game's log after the drive" },
    Feature { id: "duty_start", label: "Start the assigned duty from the game", needs: &["duty.start"], essential: false, fallback: "Choose the line and tour in the game's menu yourself" },
    Feature { id: "overlay", label: "Company notices on the screen", needs: &["ui.set", "ui.toast", "ui.remove"], essential: false, fallback: "Notices are shown as plain lines of text" },
    Feature { id: "live_notices", label: "New notices while you drive", needs: &["files.read", "json.decode"], essential: false, fallback: "Notices are read when the game starts" },
    Feature { id: "live_link", label: "Live link to Omsi-Hub", needs: &["send"], essential: false, fallback: "Omsi-Hub reads the results when the drive ends" },
];

/// How well the add-on works with a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Everything works.
    Supported,
    /// It works, with less (the features off say what).
    Limited,
    /// It does not work with this game.
    Unsupported,
    /// Not known yet: the version was not found, the first drive tells.
    Unknown,
}

/// One feature's state in a game.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureState {
    pub id: &'static str,
    pub label: &'static str,
    pub on: bool,
    /// What happens instead, when it is off.
    pub fallback: &'static str,
}

/// The add-on and a game: the level, the features and why (English notes the launcher
/// translates).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Compat {
    pub level: Level,
    /// The game's plugin API ABI, where the handshake said it.
    pub api: Option<i64>,
    pub features: Vec<FeatureState>,
    pub notes: Vec<String>,
    /// The handshake decided it (else only the version read from the folder).
    pub confirmed: bool,
}

fn all_features(on: impl Fn(&Feature) -> bool) -> Vec<FeatureState> {
    FEATURES.iter().map(|f| FeatureState { id: f.id, label: f.label, on: on(f), fallback: f.fallback }).collect()
}

/// The add-on and a game whose version was read from its folder (None: not found), before
/// the plugin ever ran there.
pub fn estimate(version: Option<&GameVersion>) -> Compat {
    match version {
        None => Compat { level: Level::Unknown, api: None, features: all_features(|_| true), notes: vec!["The version was not found: the first drive checks what this openOMSI offers".into()], confirmed: false },
        Some(v) if *v < FIRST_SUPPORTED => Compat { level: Level::Unsupported, api: None, features: all_features(|_| false), notes: vec!["This openOMSI is older than 0.2.0: update it to use Omsi-Hub".into()], confirmed: false },
        Some(_) => Compat { level: Level::Supported, api: None, features: all_features(|_| true), notes: Vec::new(), confirmed: false },
    }
}

/// The add-on and a game as the plugin's handshake describes it.
pub fn negotiate(hello: &Hello) -> Compat {
    let has = |name: &str| hello.functions.get(name).copied().unwrap_or(false);
    let features = all_features(|f| f.needs.iter().all(|n| has(n)));
    let mut notes = Vec::new();
    let essential_missing = FEATURES.iter().zip(&features).any(|(f, s)| f.essential && !s.on);
    let level = if hello.api < API_ABI {
        notes.push("This openOMSI's plugin interface is too old: update openOMSI".into());
        Level::Unsupported
    } else if essential_missing {
        notes.push("This openOMSI does not tell plugins about the duty: update openOMSI".into());
        Level::Unsupported
    } else if hello.api > API_ABI {
        // (an ABI raised: something the add-on uses may have changed; what the handshake found
        // still works, the rest is for an update of Omsi-Hub)
        notes.push("This openOMSI has a newer plugin interface: update Omsi-Hub for everything to work".into());
        Level::Limited
    } else if features.iter().any(|f| !f.on) {
        Level::Limited
    } else {
        Level::Supported
    };
    Compat { level, api: Some(hello.api), features, notes, confirmed: true }
}

/// The add-on and the game in a folder: the handshake when there is one from this very version
/// (a handshake of a version since updated says nothing), else the estimate.
pub fn check(version: Option<&GameVersion>, hello: Option<&Hello>) -> Compat {
    match hello {
        Some(h) if version.is_none() || GameVersion::find(&h.game).as_ref() == version => negotiate(h),
        _ => estimate(version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(api: i64, missing: &[&str]) -> Hello {
        let mut h = Hello { api, game: "0.2.1500".into(), ..Hello::default() };
        for f in FEATURES {
            for n in f.needs {
                h.functions.insert(n.to_string(), !missing.contains(n));
            }
        }
        h
    }

    #[test]
    fn the_folder_s_version_gives_a_first_estimate() {
        assert_eq!(estimate(Some(&GameVersion::new(0, 2, 1500))).level, Level::Supported);
        assert_eq!(estimate(Some(&GameVersion::new(0, 3, 0))).level, Level::Supported);
        assert_eq!(estimate(Some(&GameVersion::new(0, 1, 900))).level, Level::Unsupported);
        let unknown = estimate(None);
        assert_eq!(unknown.level, Level::Unknown);
        assert!(!unknown.confirmed && !unknown.notes.is_empty());
    }

    #[test]
    fn the_handshake_decides_feature_by_feature() {
        let full = negotiate(&hello(1, &[]));
        assert_eq!((full.level, full.confirmed, full.api), (Level::Supported, true, Some(1)));
        assert!(full.features.iter().all(|f| f.on));
        // an older game without panels: limited, the overlay off, the rest on
        let no_ui = negotiate(&hello(1, &["ui.toast"]));
        assert_eq!(no_ui.level, Level::Limited);
        assert!(no_ui.features.iter().all(|f| f.on == (f.id != "overlay")));
        // no duty for plugins: unsupported
        assert_eq!(negotiate(&hello(1, &["duty.get"])).level, Level::Unsupported);
        // an older or a newer ABI
        assert_eq!(negotiate(&hello(0, &[])).level, Level::Unsupported);
        let newer = negotiate(&hello(2, &[]));
        assert_eq!(newer.level, Level::Limited);
        assert!(!newer.notes.is_empty());
        // a function the handshake does not mention counts as missing
        let mut h = hello(1, &[]);
        h.functions.remove("send");
        assert_eq!(negotiate(&h).level, Level::Limited);
    }

    #[test]
    fn a_handshake_of_another_version_is_not_believed() {
        let h = hello(1, &["ui.set"]);
        assert!(check(Some(&GameVersion::new(0, 2, 1500)), Some(&h)).confirmed);
        // (the folder was updated since: the estimate of the new version)
        let after = check(Some(&GameVersion::new(0, 3, 2)), Some(&h));
        assert!(!after.confirmed);
        assert_eq!(after.level, Level::Supported);
        // (no version found: the handshake is all there is)
        assert!(check(None, Some(&h)).confirmed);
        assert_eq!(check(None, None).level, Level::Unknown);
    }
}

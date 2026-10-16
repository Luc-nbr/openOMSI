//! Omsi-Hub (fork-local, see docs/omsihub/ADDON_PLAN.md, "Fork differences"): the name this
//! distribution shows and the file names of its three programs, with the names they had
//! before (openOMSI's), which everything that looks for a program still finds - an install of
//! the old names, a folder of openOMSI's own the player chose. One table, so that a merge of
//! upstream openOMSI meets the rename in one place.
//!
//! | program | file (Windows) | was |
//! | --- | --- | --- |
//! | the app the player opens (`launcher/src-tauri`) | `Omsi-Hub.exe` | `openomsi-launcher-app.exe` |
//! | the game (`omsi-app`) | `omsi-hub-game.exe` | `openomsi.exe` |
//! | the terminal tool (`omsi-launcher-core`) | `omsi-hub-cli.exe` | `openomsi-launcher.exe` |
//!
//! Elsewhere without `.exe`; on macOS the bundle is `Omsi-Hub.app` and its executable the game.

/// The distribution's name: windows, dialogs, the files' properties.
pub const NAME: &str = "Omsi-Hub";

/// The game program's file name without its extension.
pub const GAME: &str = "omsi-hub-game";
/// The app's (the Tauri launcher's) file name without its extension.
pub const APP: &str = "Omsi-Hub";
/// The terminal tool's file name without its extension.
pub const CLI: &str = "omsi-hub-cli";

/// The game's name before the rename (openOMSI's own).
pub const LEGACY_GAME: &str = "openomsi";
/// The app's name before the rename.
pub const LEGACY_APP: &str = "openomsi-launcher-app";
/// The terminal tool's name before the rename.
pub const LEGACY_CLI: &str = "openomsi-launcher";

/// Every program: (its name before, its name now).
pub const RENAMED: [(&str, &str); 3] = [(LEGACY_GAME, GAME), (LEGACY_APP, APP), (LEGACY_CLI, CLI)];

/// The extension of a program on this system.
pub const EXE_SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };

/// A program's file name on this system (`omsi-hub-game.exe` on Windows).
pub fn exe(stem: &str) -> String {
    format!("{stem}{EXE_SUFFIX}")
}

/// The game's file names on this system, the one to look for first first.
pub fn game_files() -> [String; 2] {
    [exe(GAME), exe(LEGACY_GAME)]
}

/// The app's file names on this system, the one to look for first first.
pub fn app_files() -> [String; 2] {
    [exe(APP), exe(LEGACY_APP)]
}

/// Whether `dir` holds the game, by either name, with or without `.exe`.
pub fn holds_game(dir: &std::path::Path) -> bool {
    [GAME, LEGACY_GAME].iter().any(|n| dir.join(format!("{n}.exe")).is_file() || dir.join(n).is_file())
}

/// Whether `stem` (a file name without its extension) names the game, by either name, in any
/// case (Windows' file names have none).
pub fn is_game_stem(stem: &str) -> bool {
    stem.eq_ignore_ascii_case(GAME) || stem.eq_ignore_ascii_case(LEGACY_GAME)
}

/// The other name of the program file `name` (`openomsi.exe` <-> `omsi-hub-game.exe`), with
/// `suffix` its extension: None for a file that is not one of the programs.
pub fn counterpart(name: &str, suffix: &str) -> Option<String> {
    if !name.to_ascii_lowercase().ends_with(&suffix.to_ascii_lowercase()) {
        return None;
    }
    let stem = &name[..name.len() - suffix.len()];
    RENAMED.iter().find_map(|(old, new)| {
        if stem.eq_ignore_ascii_case(old) {
            Some(format!("{new}{suffix}"))
        } else if stem.eq_ignore_ascii_case(new) {
            Some(format!("{old}{suffix}"))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_program_has_its_old_name_as_counterpart() {
        assert_eq!(counterpart("openomsi.exe", ".exe").as_deref(), Some("omsi-hub-game.exe"));
        assert_eq!(counterpart("OpenOMSI.EXE", ".exe").as_deref(), Some("omsi-hub-game.exe"));
        assert_eq!(counterpart("Omsi-Hub.exe", ".exe").as_deref(), Some("openomsi-launcher-app.exe"));
        assert_eq!(counterpart("openomsi-launcher.exe", ".exe").as_deref(), Some("omsi-hub-cli.exe"));
        assert_eq!(counterpart("omsi-hub-game", "").as_deref(), Some("openomsi"));
        assert_eq!(counterpart("steam_api64.dll", ".exe"), None);
        assert_eq!(counterpart("openomsi", ".exe"), None);
    }

    #[test]
    fn no_two_names_differ_only_in_case() {
        let mut all: Vec<String> = RENAMED.iter().flat_map(|(a, b)| [a.to_ascii_lowercase(), b.to_ascii_lowercase()]).collect();
        all.sort();
        let n = all.len();
        all.dedup();
        assert_eq!(all.len(), n);
        assert!(is_game_stem("OMSI-HUB-GAME") && is_game_stem("openomsi") && !is_game_stem("Omsi-Hub"));
    }
}

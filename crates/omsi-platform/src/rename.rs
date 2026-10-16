//! Omsi-Hub (fork-local, docs/omsihub/RENAME.md): an installation whose programs still have
//! openOMSI's names (`openomsi.exe`, `openomsi-launcher-app.exe`, `openomsi-launcher.exe`)
//! takes Omsi-Hub's (`omsi-hub-game.exe`, `Omsi-Hub.exe`, `omsi-hub-cli.exe`,
//! `omsi_cfg::product`) when one of its programs starts.
//!
//! The builds before the rename update from a release's `openOMSI-<v>-...` archive, which
//! holds the programs by their old names (they know no other, and start the program by its
//! old name after the update); the new program then renames them here. A program that is
//! there by both names keeps the newer file (the one an update or an unpacked archive just
//! brought) under the new name. Windows lets a running program be renamed, so the program
//! doing this renames itself as well; one it cannot delete (still running) is set aside as
//! `*.renamed-old` and deleted at a later start. The list of what an update brought
//! (`.openomsi-files`) names the new files, and on Windows the player's shortcuts (the desktop,
//! the Start menu, the taskbar) to an old program open `Omsi-Hub.exe` from then on.
//!
//! Only the folder of the program that runs, only a release's (never a development build's,
//! never a folder of openOMSI's own the player chose), never a macOS bundle (an update
//! replaces it whole, and its insides are sealed by its signature).

use omsi_cfg::product;
use std::path::{Path, PathBuf};

/// What a file set aside here is called until it can be deleted.
const SET_ASIDE: &str = ".renamed-old";

/// A program moved to its new name.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    /// Its old name's path.
    pub from: PathBuf,
    /// Its new name's path.
    pub to: PathBuf,
}

/// Rename the programs of this installation (see the module). Called first thing by the game and
/// the app; does nothing when there is nothing to rename.
pub fn migrate_install() -> Vec<Moved> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Vec::new();
    }
    let Ok(place) = crate::updater::install_place() else { return Vec::new() };
    // (a bundle; and a release's program trying itself in the update's staging folder)
    if place.bundle.is_some() || place.dir.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(crate::updater::STAGING)) {
        return Vec::new();
    }
    let moved = migrate_dir(&place.dir, product::EXE_SUFFIX);
    if !moved.is_empty() {
        for m in &moved {
            log::info!("rename: {} is {} now", m.from.display(), m.to.display());
        }
        #[cfg(windows)]
        shortcuts::retarget(&shortcut_moves(&place.dir, &moved));
    }
    moved
}

/// The programs of `dir` by their old names to their new ones (`suffix`: the programs'
/// extension, `.exe` on Windows); what was moved (or, there by both names, dropped).
pub fn migrate_dir(dir: &Path, suffix: &str) -> Vec<Moved> {
    // what an earlier start could not delete
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().ends_with(SET_ASIDE) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut moved = Vec::new();
    for (old, new) in product::RENAMED {
        let Some(from) = child(dir, &format!("{old}{suffix}")) else { continue };
        let to = dir.join(format!("{new}{suffix}"));
        if to.is_file() {
            if newer(&from, &to) {
                // (the old name holds the newer program: it takes the new name's place)
                if set_aside(&to).is_err() {
                    log::warn!("rename: {} cannot be replaced", to.display());
                    continue;
                }
            } else {
                // (a stale copy under the old name)
                if remove_or_set_aside(&from) {
                    moved.push(Moved { from, to });
                }
                continue;
            }
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push(Moved { from, to }),
            Err(e) => log::warn!("rename: {} -> {}: {e}", from.display(), to.display()),
        }
    }
    if !moved.is_empty() {
        fix_manifest(dir, suffix);
    }
    moved
}

/// The file `name` of `dir` in whatever case it is on the disk.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    rd.flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// Whether `a` was written after `b` (unknown: not).
fn newer(a: &Path, b: &Path) -> bool {
    let t = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    matches!((t(a), t(b)), (Some(a), Some(b)) if a > b)
}

/// `p` renamed out of the way (a running program cannot be deleted on Windows, but renamed).
fn set_aside(p: &Path) -> std::io::Result<()> {
    let mut s = p.as_os_str().to_os_string();
    s.push(SET_ASIDE);
    let aside = PathBuf::from(s);
    let _ = std::fs::remove_file(&aside);
    std::fs::rename(p, &aside)?;
    let _ = std::fs::remove_file(&aside);
    Ok(())
}

/// `p` deleted, or set aside when it cannot be; whether it is gone from its name.
fn remove_or_set_aside(p: &Path) -> bool {
    std::fs::remove_file(p).is_ok() || set_aside(p).is_ok()
}

/// The update's list of the files it brought (`.openomsi-files`) by the new names.
fn fix_manifest(dir: &Path, suffix: &str) {
    let path = dir.join(crate::updater::MANIFEST);
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let name = product::RENAMED.iter().find(|(old, _)| line.eq_ignore_ascii_case(&format!("{old}{suffix}"))).map(|(_, new)| format!("{new}{suffix}")).unwrap_or_else(|| line.to_string());
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    let new = out.join("\n") + "\n";
    if new != text {
        let _ = std::fs::write(&path, new);
    }
}

/// The shortcuts to change: one to the old game or the old app opens the app (`Omsi-Hub.exe`,
/// which starts the game's checks first), one to the old terminal tool the new one.
#[cfg_attr(not(windows), allow(dead_code))]
fn shortcut_moves(dir: &Path, moved: &[Moved]) -> Vec<(PathBuf, PathBuf)> {
    let app = dir.join(product::exe(product::APP));
    moved
        .iter()
        .map(|m| {
            let stem = m.to.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let to = if stem.eq_ignore_ascii_case(product::CLI) || !app.is_file() { m.to.clone() } else { app.clone() };
            (m.from.clone(), to)
        })
        .collect()
}

/// Windows: the player's shortcuts (`.lnk`) to an old program changed to its new one.
#[cfg(windows)]
mod shortcuts {
    use std::path::{Path, PathBuf};
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READWRITE};
    use windows::Win32::UI::Shell::{FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_PublicDesktop, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT};

    /// The folders a player's shortcuts are in (with how deep to look).
    fn folders() -> Vec<(PathBuf, usize)> {
        let mut out = Vec::new();
        for (id, depth) in [(&FOLDERID_Desktop, 1), (&FOLDERID_PublicDesktop, 1), (&FOLDERID_Programs, 3), (&FOLDERID_CommonPrograms, 3)] {
            // SAFETY: the returned string is freed once it is copied
            unsafe {
                if let Ok(p) = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
                    if let Ok(s) = p.to_string() {
                        out.push((PathBuf::from(s), depth));
                    }
                    CoTaskMemFree(Some(p.0 as *const _));
                }
            }
        }
        if let Some(a) = std::env::var_os("APPDATA") {
            let pinned = PathBuf::from(a).join("Microsoft").join("Internet Explorer").join("Quick Launch").join("User Pinned");
            out.push((pinned.join("TaskBar"), 1));
            out.push((pinned.join("StartMenu"), 1));
        }
        out
    }

    fn links(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth > 1 {
                    links(&p, depth - 1, out);
                }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
                out.push(p);
            }
        }
    }

    /// Every shortcut to a `from` of `moves` points to its `to` (and takes its icon from it
    /// when it took it from the old program).
    pub fn retarget(moves: &[(PathBuf, PathBuf)]) {
        let mut all = Vec::new();
        for (dir, depth) in folders() {
            links(&dir, depth, &mut all);
        }
        // SAFETY: COM is set up on this thread for the calls below and taken down after them
        unsafe {
            let ours = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            for lnk in &all {
                match retarget_one(lnk, moves) {
                    Ok(true) => log::info!("rename: the shortcut {} opens Omsi-Hub now", lnk.display()),
                    Ok(false) => {}
                    Err(e) => log::debug!("rename: shortcut {}: {e}", lnk.display()),
                }
            }
            if ours {
                CoUninitialize();
            }
        }
    }

    unsafe fn retarget_one(lnk: &Path, moves: &[(PathBuf, PathBuf)]) -> windows::core::Result<bool> {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = link.cast()?;
        file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READWRITE)?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0)?;
        let target = PathBuf::from(String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]));
        let same = |a: &Path, b: &Path| a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy());
        let Some((from, to)) = moves.iter().find(|(from, _)| same(from, &target)) else { return Ok(false) };
        link.SetPath(&HSTRING::from(to.as_os_str()))?;
        let mut icon = [0u16; 1024];
        let mut index = 0i32;
        if link.GetIconLocation(&mut icon, &mut index).is_ok() {
            let at = PathBuf::from(String::from_utf16_lossy(&icon[..icon.iter().position(|&c| c == 0).unwrap_or(icon.len())]));
            if same(&at, from) {
                link.SetIconLocation(&HSTRING::from(to.as_os_str()), index)?;
            }
        }
        file.Save(PCWSTR::null(), true)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsihub-rename-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(p: PathBuf) -> String {
        std::fs::read_to_string(p).unwrap_or_default()
    }

    /// (a file written a moment later: its modification time is newer)
    fn later() {
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    #[test]
    fn an_install_of_the_old_names_takes_the_new_ones() {
        let d = temp("old");
        for n in ["openomsi.exe", "openomsi-launcher-app.exe", "openomsi-launcher.exe", "steam_api64.dll"] {
            std::fs::write(d.join(n), n).unwrap();
        }
        std::fs::write(d.join(crate::updater::MANIFEST), "openomsi.exe\nopenomsi-launcher-app.exe\nopenomsi-launcher.exe\nsteam_api64.dll\n").unwrap();
        let moved = migrate_dir(&d, ".exe");
        assert_eq!(moved.len(), 3);
        assert_eq!(read(d.join("omsi-hub-game.exe")), "openomsi.exe");
        assert_eq!(read(d.join("Omsi-Hub.exe")), "openomsi-launcher-app.exe");
        assert_eq!(read(d.join("omsi-hub-cli.exe")), "openomsi-launcher.exe");
        for n in ["openomsi.exe", "openomsi-launcher-app.exe", "openomsi-launcher.exe"] {
            assert!(!d.join(n).exists(), "{n}");
        }
        assert_eq!(read(d.join("steam_api64.dll")), "steam_api64.dll");
        assert_eq!(read(d.join(crate::updater::MANIFEST)), "omsi-hub-game.exe\nOmsi-Hub.exe\nomsi-hub-cli.exe\nsteam_api64.dll\n");
        // the shortcuts: the old game and the old app open the app, the terminal tool its own
        let s = shortcut_moves(&d, &moved);
        assert!(s.iter().any(|(f, t)| f.ends_with("openomsi.exe") && t.ends_with("Omsi-Hub.exe")));
        assert!(s.iter().any(|(f, t)| f.ends_with("openomsi-launcher.exe") && t.ends_with("omsi-hub-cli.exe")));
        // a second start: nothing left to do
        assert!(migrate_dir(&d, ".exe").is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_program_there_by_both_names_keeps_the_newer_file() {
        let d = temp("both");
        // an unpacked new release over an old install: the old names are stale
        std::fs::write(d.join("openomsi.exe"), "old game").unwrap();
        later();
        std::fs::write(d.join("omsi-hub-game.exe"), "new game").unwrap();
        // an old updater's install of a release over a folder that has the new names (a
        // version gone back to, then updated by its old updater): the old name is the newer
        std::fs::write(d.join("Omsi-Hub.exe"), "stale app").unwrap();
        later();
        std::fs::write(d.join("OPENOMSI-LAUNCHER-APP.EXE"), "newest app").unwrap();
        migrate_dir(&d, ".exe");
        assert_eq!(read(d.join("omsi-hub-game.exe")), "new game");
        assert_eq!(read(d.join("Omsi-Hub.exe")), "newest app");
        let names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_without_old_names_is_left_alone() {
        let d = temp("none");
        std::fs::write(d.join("Omsi-Hub.exe"), "app").unwrap();
        std::fs::write(d.join("README.md"), "readme").unwrap();
        std::fs::write(d.join(crate::updater::MANIFEST), "Omsi-Hub.exe\n").unwrap();
        assert!(migrate_dir(&d, ".exe").is_empty());
        assert_eq!(read(d.join(crate::updater::MANIFEST)), "Omsi-Hub.exe\n");
        // (a Linux install: no extension)
        std::fs::write(d.join("openomsi"), "game").unwrap();
        assert_eq!(migrate_dir(&d, "").len(), 1);
        assert_eq!(read(d.join("omsi-hub-game")), "game");
        let _ = std::fs::remove_dir_all(&d);
    }
}

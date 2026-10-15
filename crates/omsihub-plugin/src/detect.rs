//! An openOMSI folder the player chose: is there a game in it, which version, and where does it
//! read its plugins from.
//!
//! The game is `openomsi.exe` (Windows), `openomsi` (Linux) or the bundle's
//! `Contents/MacOS/openomsi` (macOS: the folder holding `openOMSI.app`, or the bundle itself).
//! Its version is read without starting it where the system keeps one: the Windows
//! executable's `ProductVersion` (every release build stamps it, `crates/omsi-app/build.rs`) and
//! the bundle's `Info.plist`; else from the folder's name as a release archive unpacks
//! (`openOMSI-0.2.1234-linux-x64`). [`version_from_output`] reads what `openomsi --version`
//! prints, for the caller that may start the program (the last resort, on Linux mostly).
//!
//! The plugins are read from the game's content folder's `Plugins` - the program's folder (the
//! one beside the bundle on macOS), or its `openOMSI` folder when openOMSI was unpacked into the
//! original OMSI 2 folder, or `~/.openomsi/content` when the program's folder may not be
//! written to - the same rule as `omsi_cfg::content_dir`, worked out here for a folder that is
//! not this program's own.

use crate::version::GameVersion;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where a version was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionFrom {
    /// The Windows executable's ProductVersion.
    Executable,
    /// The macOS bundle's Info.plist.
    Bundle,
    /// The folder's name (a release archive unpacked).
    FolderName,
    /// `openomsi --version`.
    Program,
}

/// An openOMSI found in a folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Install {
    /// The folder as the player chose it.
    pub folder: PathBuf,
    /// The game's program.
    pub exe: PathBuf,
    pub version: Option<GameVersion>,
    pub version_from: Option<VersionFrom>,
    /// An Omsi-Hub build of the game (its own copy, "Omsi-Hub" in its name) rather than
    /// openOMSI's own.
    pub omsihub_build: bool,
    /// The folder the game reads its plugins from (made when the add-on is installed).
    pub plugins_dir: PathBuf,
}

/// Why a folder holds no openOMSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// No such folder.
    Missing,
    /// The original OMSI 2 without openOMSI in it.
    Omsi2Only,
    /// No openOMSI program in it.
    NoGame,
}

impl Problem {
    /// What to tell the player (English: the launcher translates it).
    pub fn text(self) -> &'static str {
        match self {
            Problem::Missing => "This folder does not exist",
            Problem::Omsi2Only => "This is the OMSI 2 folder: choose the folder openOMSI is in",
            Problem::NoGame => "No openOMSI was found in this folder",
        }
    }
}

/// A file or folder of `dir` by its name, whatever its case (as on Windows).
/// The name comes back as it is on disk (Windows finds `dir.join(name)` in any case and would
/// keep the asked one).
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let listed = std::fs::read_dir(dir).ok().and_then(|r| {
        let all: Vec<PathBuf> = r.flatten().map(|e| e.path()).collect();
        let exact = all.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == name)).cloned();
        exact.or_else(|| all.into_iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))))
    });
    listed.or_else(|| Some(dir.join(name)).filter(|p| p.exists()))
}

fn is_bundle(p: &Path) -> bool {
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("app")) && p.join("Contents/MacOS/openomsi").is_file()
}

/// The game's program in `folder`: the folder's own, a bundle the folder is or holds.
pub fn find_exe(folder: &Path) -> Option<PathBuf> {
    for name in ["openomsi.exe", "openomsi"] {
        if let Some(p) = child(folder, name).filter(|p| p.is_file()) {
            return Some(p);
        }
    }
    if is_bundle(folder) {
        return Some(folder.join("Contents/MacOS/openomsi"));
    }
    // (openOMSI.app, or an Omsi-Hub.app: whichever bundle holds the game)
    let mut bundles: Vec<PathBuf> = std::fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).filter(|p| is_bundle(p)).collect();
    bundles.sort();
    bundles.into_iter().next().map(|b| b.join("Contents/MacOS/openomsi"))
}

/// The bundle an executable is in (`X.app/Contents/MacOS/openomsi`).
fn bundle_of(exe: &Path) -> Option<&Path> {
    let macos = exe.parent()?;
    (macos.ends_with("Contents/MacOS")).then(|| macos.parent()?.parent()).flatten()
}

/// The UTF-16 text a Windows version resource keeps under `key` (`ProductVersion`,
/// `ProductName`): the key, its terminating zero, the padding to four bytes, then the value.
pub fn pe_string(bytes: &[u8], key: &str) -> Option<String> {
    let mut pat: Vec<u8> = key.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    pat.extend([0, 0]);
    let mut from = 0;
    while let Some(k) = bytes[from..].windows(pat.len()).position(|w| w == pat.as_slice()) {
        let mut i = from + k + pat.len();
        // (the value starts on a four-byte boundary of the resource, which is aligned itself)
        while i % 4 != 0 && i + 1 < bytes.len() && bytes[i] == 0 && bytes[i + 1] == 0 {
            i += 2;
        }
        let mut units = Vec::new();
        while i + 1 < bytes.len() && units.len() < 128 {
            let u = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            if u == 0 {
                break;
            }
            units.push(u);
            i += 2;
        }
        let text = String::from_utf16_lossy(&units);
        if !text.trim().is_empty() {
            return Some(text.trim().to_string());
        }
        from += k + 1;
    }
    None
}

/// The value of `key` in an Info.plist (the XML form the build writes).
pub fn plist_string(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("<key>{key}</key>"))?;
    let rest = &text[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")?;
    Some(rest[open..open + close].trim().to_string()).filter(|s| !s.is_empty())
}

/// The version `openomsi --version` printed ("openomsi 0.2.1234").
pub fn version_from_output(out: &str) -> Option<GameVersion> {
    out.lines().find_map(|l| l.trim().strip_prefix("openomsi ").and_then(GameVersion::find))
}

/// The version of the game at `exe`, without starting it.
pub fn read_version(folder: &Path, exe: &Path) -> (Option<GameVersion>, Option<VersionFrom>, bool) {
    if exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
        if let Ok(bytes) = std::fs::read(exe) {
            let fork = pe_string(&bytes, "ProductName").is_some_and(|n| n.contains("Omsi-Hub"));
            if let Some(v) = pe_string(&bytes, "ProductVersion").and_then(|v| GameVersion::find(&v)) {
                return (Some(v), Some(VersionFrom::Executable), fork);
            }
            let v = folder_version(folder);
            let from = v.as_ref().map(|_| VersionFrom::FolderName);
            return (v, from, fork);
        }
    }
    if let Some(bundle) = bundle_of(exe) {
        if let Ok(text) = std::fs::read_to_string(bundle.join("Contents/Info.plist")) {
            let fork = plist_string(&text, "CFBundleName").is_some_and(|n| n.contains("Omsi-Hub"));
            if let Some(v) = plist_string(&text, "CFBundleShortVersionString").and_then(|v| GameVersion::find(&v)) {
                return (Some(v), Some(VersionFrom::Bundle), fork);
            }
        }
    }
    let v = folder_version(folder);
    let from = v.as_ref().map(|_| VersionFrom::FolderName);
    (v, from, false)
}

/// A version in the folder's name or its parent's (`openOMSI-0.2.1234-windows-x64`).
fn folder_version(folder: &Path) -> Option<GameVersion> {
    folder.ancestors().take(2).filter_map(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).find(|n| n.to_ascii_lowercase().contains("omsi")).and_then(|n| GameVersion::find(&n))
}

/// Whether `dir` may be written to (a probe file made and removed).
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".omsihub-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// The content folder of the game at `exe` (see the module's text); `home` is the user's
/// home folder, for the fallback.
pub fn content_dir_of(exe: &Path, home: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    let dir = bundle_of(exe).and_then(Path::parent).unwrap_or(dir);
    let cand = if child(dir, "Omsi.exe").is_some() && child(dir, "maps").is_some() { dir.join("openOMSI") } else { dir.to_path_buf() };
    let programs = cand.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Applications"));
    if !programs && (cand.is_dir() || std::fs::create_dir_all(&cand).is_ok()) && writable(&cand) {
        cand
    } else {
        home.join(".openomsi").join("content")
    }
}

/// The game's plugins folder in a content folder (`Plugins`, as its layout names it; an
/// existing `plugins` of another case is kept).
pub fn plugins_dir_of(content: &Path) -> PathBuf {
    child(content, "Plugins").filter(|p| p.is_dir()).unwrap_or_else(|| content.join("Plugins"))
}

/// What is in the folder the player chose.
pub fn inspect(folder: &Path, home: &Path) -> Result<Install, Problem> {
    if !folder.is_dir() {
        return Err(Problem::Missing);
    }
    let Some(exe) = find_exe(folder) else {
        return Err(if child(folder, "Omsi.exe").is_some() { Problem::Omsi2Only } else { Problem::NoGame });
    };
    let (version, version_from, omsihub_build) = read_version(folder, &exe);
    let plugins_dir = plugins_dir_of(&content_dir_of(&exe, home));
    Ok(Install { folder: folder.to_path_buf(), exe, version, version_from, omsihub_build, plugins_dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsihub-detect-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A version resource's string as winresource writes it: length, value length, type, the
    /// key in UTF-16 with its zero, padding, the value.
    fn resource(key: &str, value: &str) -> Vec<u8> {
        let utf16 = |s: &str| s.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>();
        let mut b = vec![0u8; 6];
        b.extend(utf16(key));
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b.extend(utf16(value));
        b
    }

    #[test]
    fn the_windows_version_resource_is_read() {
        let mut exe = b"MZ\x90\0 junk ProductVersion in ASCII 9.9.9 ".to_vec();
        while exe.len() % 4 != 0 {
            exe.push(0);
        }
        exe.extend(resource("ProductName", "Omsi-Hub"));
        while exe.len() % 4 != 0 {
            exe.push(0);
        }
        exe.extend(resource("ProductVersion", "0.2.1234"));
        assert_eq!(pe_string(&exe, "ProductVersion").as_deref(), Some("0.2.1234"));
        assert_eq!(pe_string(&exe, "ProductName").as_deref(), Some("Omsi-Hub"));
        assert_eq!(pe_string(&exe, "FileDescription"), None);
        // (an empty ProductVersion, as a local build without OPENOMSI_VERSION has)
        assert_eq!(pe_string(&resource("ProductVersion", ""), "ProductVersion"), None);
    }

    #[test]
    fn the_bundle_s_version_and_the_program_s_answer_are_read() {
        let plist = "<dict>\n\t<key>CFBundleName</key><string>openOMSI</string>\n\t<key>CFBundleShortVersionString</key><string>0.3.12</string>\n</dict>";
        assert_eq!(plist_string(plist, "CFBundleShortVersionString").as_deref(), Some("0.3.12"));
        assert_eq!(plist_string(plist, "CFBundleVersion"), None);
        assert_eq!(version_from_output("openomsi 0.2.1500\n"), Some(GameVersion::new(0, 2, 1500)));
        assert_eq!(version_from_output("error: unexpected argument '--version'"), None);
    }

    #[test]
    fn folders_are_told_apart() {
        let home = dir("home");
        assert_eq!(inspect(&home.join("nothing"), &home), Err(Problem::Missing));
        let empty = dir("empty");
        assert_eq!(inspect(&empty, &home), Err(Problem::NoGame));
        let omsi2 = dir("omsi2");
        std::fs::write(omsi2.join("Omsi.exe"), b"MZ").unwrap();
        assert_eq!(inspect(&omsi2, &home), Err(Problem::Omsi2Only));
        assert_eq!(Problem::Omsi2Only.text(), "This is the OMSI 2 folder: choose the folder openOMSI is in");
    }

    #[test]
    fn a_windows_release_folder() {
        let home = dir("home-w");
        let root = dir("win");
        let folder = root.join("openOMSI-0.2.1234-windows-x64");
        std::fs::create_dir_all(folder.join("plugins")).unwrap();
        let mut exe = b"MZ".to_vec();
        exe.extend([0, 0]);
        exe.extend(resource("ProductVersion", "0.2.1300"));
        std::fs::write(folder.join("OpenOMSI.exe"), &exe).unwrap();
        let i = inspect(&folder, &home).unwrap();
        assert!(i.exe.ends_with("OpenOMSI.exe"));
        assert_eq!((i.version, i.version_from, i.omsihub_build), (Some(GameVersion::new(0, 2, 1300)), Some(VersionFrom::Executable), false));
        // (the folder's own plugins folder, in its own case)
        assert_eq!(i.plugins_dir, folder.join("plugins"));
        // no version in the program: the folder's name
        std::fs::write(folder.join("OpenOMSI.exe"), b"MZ").unwrap();
        let i = inspect(&folder, &home).unwrap();
        assert_eq!((i.version, i.version_from), (Some(GameVersion::new(0, 2, 1234)), Some(VersionFrom::FolderName)));
    }

    #[test]
    fn a_mac_bundle_and_a_linux_folder() {
        let home = dir("home-m");
        let apps = dir("mac");
        let macos = apps.join("Omsi-Hub.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("openomsi"), b"\xcf\xfa\xed\xfe").unwrap();
        std::fs::write(apps.join("Omsi-Hub.app/Contents/Info.plist"), "<key>CFBundleName</key><string>Omsi-Hub</string><key>CFBundleShortVersionString</key><string>0.2.48</string>").unwrap();
        for chosen in [apps.clone(), apps.join("Omsi-Hub.app")] {
            let i = inspect(&chosen, &home).unwrap();
            assert_eq!((i.version, i.version_from, i.omsihub_build), (Some(GameVersion::new(0, 2, 48)), Some(VersionFrom::Bundle), true));
            // (the content folder is beside the bundle)
            assert_eq!(i.plugins_dir, apps.join("Plugins"));
        }
        let linux = dir("linux");
        std::fs::write(linux.join("openomsi"), b"\x7fELF").unwrap();
        let i = inspect(&linux, &home).unwrap();
        assert_eq!((i.version, i.plugins_dir), (None, linux.join("Plugins")));
    }

    #[test]
    fn openomsi_inside_the_omsi2_folder_uses_its_own_content_folder() {
        let home = dir("home-o");
        let omsi2 = dir("omsi2-with-openomsi");
        std::fs::create_dir_all(omsi2.join("maps")).unwrap();
        std::fs::write(omsi2.join("Omsi.exe"), b"MZ").unwrap();
        std::fs::write(omsi2.join("openomsi.exe"), b"MZ").unwrap();
        assert_eq!(inspect(&omsi2, &home).unwrap().plugins_dir, omsi2.join("openOMSI").join("Plugins"));
    }
}

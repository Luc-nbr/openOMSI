//! A plugin switched off without being removed: `plugins/disabled.txt` lists the plugins of
//! that folder the game leaves out, one name a line (as the plugin is named: a `.lua` or
//! `.oop` file's name, a folder plugin's folder, an `.opl` file's name), without regard to
//! case; blank lines and what follows a `#` do not count. Taking a name off the list switches
//! the plugin on again at the next start.

use std::path::Path;

/// The list's file in a plugins folder.
pub const FILE: &str = "disabled.txt";

/// The names `disabled.txt` of `dir` lists, in lower case (none without the file).
pub fn disabled(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else { return Vec::new() };
    text.lines().map(|l| l.split('#').next().unwrap_or("").trim().to_lowercase()).filter(|l| !l.is_empty()).collect()
}

/// The name a plugin goes by: its folder's for a `main.lua`, else its file's without the
/// extension.
pub fn name_of(path: &Path) -> String {
    let main = path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("main.lua"));
    let named = if main { path.parent().unwrap_or(path) } else { path };
    let name = if main { named.file_name() } else { named.file_stem() };
    name.map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Whether the plugin at `path` is on `list` (as [`disabled`] gives it).
pub fn is_off(list: &[String], path: &Path) -> bool {
    !list.is_empty() && list.contains(&name_of(path))
}

/// Switch the plugin `name` of `dir` on (off the list) or off (onto it). The other lines of the
/// list stay as they are; a list is made only to switch a plugin off.
pub fn set(dir: &Path, name: &str, on: bool) -> std::io::Result<()> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let listed = |l: &str| l.split('#').next().unwrap_or("").trim().eq_ignore_ascii_case(name);
    if text.lines().any(listed) != on {
        return Ok(());
    }
    let mut lines: Vec<&str> = text.lines().filter(|l| !listed(l)).collect();
    if !on {
        lines.push(name);
    }
    std::fs::write(path, lines.iter().map(|l| format!("{l}\n")).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn names_and_the_list() {
        assert_eq!(name_of(Path::new("plugins/Speedo/main.lua")), "speedo");
        assert_eq!(name_of(Path::new("plugins/Clock.lua")), "clock");
        assert_eq!(name_of(Path::new("plugins/sub/IBIS.opl")), "ibis");
        assert_eq!(name_of(Path::new("plugins/route.oop")), "route");
        let dir: PathBuf = std::env::temp_dir().join(format!("omsi-plugin-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(disabled(&dir).is_empty());
        // (switching on what is on makes no list)
        set(&dir, "Speedo", true).unwrap();
        assert!(!dir.join(FILE).exists());
        std::fs::write(dir.join(FILE), "# switched off for now\nSpeedo  # too slow\n\n").unwrap();
        let list = disabled(&dir);
        assert_eq!(list, ["speedo"]);
        assert!(is_off(&list, Path::new("plugins/speedo/main.lua")));
        assert!(!is_off(&list, Path::new("plugins/clock.lua")));
        set(&dir, "clock", false).unwrap();
        set(&dir, "clock", false).unwrap();
        assert_eq!(disabled(&dir), ["speedo", "clock"]);
        set(&dir, "SPEEDO", true).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "# switched off for now\n\nclock\n");
    }

    #[test]
    fn a_plugin_on_the_list_is_not_loaded() {
        let dir: PathBuf = std::env::temp_dir().join(format!("omsi-plugin-switch-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("speedo")).unwrap();
        std::fs::write(dir.join("speedo/main.lua"), "omsi.log('speedo')").unwrap();
        std::fs::write(dir.join("clock.lua"), "omsi.log('clock')").unwrap();
        let names = |p: &crate::Plugins| p.lua.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&crate::Plugins::load(std::slice::from_ref(&dir), &crate::HostConfig::default())), ["clock", "speedo"]);
        set(&dir, "speedo", false).unwrap();
        assert_eq!(names(&crate::Plugins::load(std::slice::from_ref(&dir), &crate::HostConfig::default())), ["clock"]);
    }
}

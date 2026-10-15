//! Omsi-Hub as an add-on of the player's own openOMSI (docs/omsihub/ADDON_PLAN.md, phase 1):
//! the Settings page's "Your openOMSI" - the folder chosen, checked and remembered, its
//! version read, how well Omsi-Hub works with it, and the Omsi-Hub plugin
//! (`crates/omsihub-plugin`) installed there and switched on or off. Omsi-Hub still starts its
//! own copy of the game until phase 2; the duty bridge's two ends are here already
//! (`addon_assign`, `addon_reports`).
//!
//! * `addon_state` - the folder, what is in it, the compatibility and the plugin;
//! * `addon_set_folder {folder}` - choose a folder (kept only when openOMSI is in it);
//! * `addon_clear` - forget it (Omsi-Hub's own copy only, as before);
//! * `addon_enable {on}` - the plugin installed (or updated) and on, or parked off;
//! * `addon_assign {assignment, lang}` - the duty for the plugin, its texts in `lang`;
//! * `addon_reports` - the plugin's reports since the last call.
//!
//! The choice is kept in `~/.openomsi/omsihub-addon.json`.

use crate::Answer;
use omsi_launcher_lib as core;
use omsihub_plugin::bridge::{self, Assignment, ReportReader};
use omsihub_plugin::{compat, detect, install, GameVersion};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILE: &str = "omsihub-addon.json";

/// What is kept of the add-on's choice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    /// The openOMSI folder the player chose ("": none, Omsi-Hub's own copy only).
    pub folder: String,
    /// The version `openomsi --version` gave, and the program's change time it was asked at
    /// (asked once per program, where nothing else said it).
    pub probed: String,
    pub probed_at: u64,
    /// How far the plugin's reports were read.
    pub reports: ReportReader,
}

fn path() -> PathBuf {
    core::data_dir().join(FILE)
}

fn load() -> Choice {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(c: &Choice) -> Result<(), String> {
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(path(), text).map_err(|e| e.to_string())
}

fn home() -> PathBuf {
    let d = core::data_dir();
    d.parent().map(Path::to_path_buf).unwrap_or(d)
}

fn modified(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// What `exe --version` prints, waiting at most a few seconds.
fn ask_version(exe: &Path) -> Option<GameVersion> {
    let mut child = std::process::Command::new(exe).arg("--version").env("OMSI_NO_GUARD", "1").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    detect::version_from_output(&out)
}

/// The folder's openOMSI with its version filled in: read from its files, else asked of the
/// program once (kept in `c`).
fn inspect(c: &mut Choice) -> Result<detect::Install, detect::Problem> {
    let mut i = detect::inspect(Path::new(&c.folder), &home())?;
    if i.version.is_none() {
        let at = modified(&i.exe);
        if c.probed_at != at {
            c.probed = ask_version(&i.exe).map(|v| v.to_string()).unwrap_or_default();
            c.probed_at = at;
            let _ = save(c);
        }
        if let Some(v) = GameVersion::find(&c.probed) {
            i.version = Some(v);
            i.version_from = Some(detect::VersionFrom::Program);
        }
    }
    Ok(i)
}

/// The plugin's handshake: its file, else the game's log (a game without file functions).
fn hello(plugin_dir: &Path) -> Option<bridge::Hello> {
    bridge::read_hello(plugin_dir).or_else(|| std::fs::read_to_string(core::data_dir().join("game.log")).ok().and_then(|log| bridge::from_log(&log).0))
}

fn state() -> Answer {
    let mut c = load();
    if c.folder.is_empty() {
        return Ok(json!({ "folder": null, "bundled": true }));
    }
    let base = json!({ "folder": c.folder, "bundled": true });
    let i = match inspect(&mut c) {
        Ok(i) => i,
        Err(p) => return Ok(json!({ "folder": c.folder, "bundled": true, "problem": p.text() })),
    };
    let plugin = install::status(&i.plugins_dir);
    let compat = compat::check(i.version.as_ref(), hello(&plugin.dir).as_ref());
    let mut v = base;
    v["install"] = serde_json::to_value(&i).map_err(|e| e.to_string())?;
    v["plugin"] = serde_json::to_value(&plugin).map_err(|e| e.to_string())?;
    v["compat"] = serde_json::to_value(&compat).map_err(|e| e.to_string())?;
    v["plugin_version"] = json!(install::VERSION);
    Ok(v)
}

fn set_folder(folder: &str) -> Answer {
    let folder = folder.trim();
    let mut c = Choice { folder: folder.to_string(), ..Choice::default() };
    // (a folder without openOMSI is not kept: the problem is said, the old choice stays)
    if let Err(p) = inspect(&mut c) {
        let mut v = state()?;
        v["refused"] = json!({ "folder": folder, "problem": p.text() });
        return Ok(v);
    }
    save(&c)?;
    state()
}

fn clear() -> Answer {
    save(&Choice::default())?;
    state()
}

fn plugins_dir() -> Result<PathBuf, String> {
    let mut c = load();
    if c.folder.is_empty() {
        return Err("No openOMSI folder chosen".into());
    }
    inspect(&mut c).map(|i| i.plugins_dir).map_err(|p| p.text().to_string())
}

fn enable(on: bool) -> Answer {
    let dir = plugins_dir()?;
    if on { install::install(&dir) } else { install::disable(&dir) }.map_err(|e| e.to_string())?;
    state()
}

/// The plugin's texts in `lang` (English where a text has no translation).
pub fn plugin_texts(lang: &str) -> std::collections::BTreeMap<String, String> {
    let table = crate::i18n::table(lang);
    install::TEXTS.iter().map(|t| (t.to_string(), table.get(*t).cloned().unwrap_or_else(|| t.to_string()))).collect()
}

fn assign(a: Assignment, lang: &str) -> Answer {
    let dir = plugins_dir()?;
    let status = install::status(&dir);
    if status.state != install::State::On {
        return Err("The Omsi-Hub plugin is not switched on in this openOMSI".into());
    }
    let a = Assignment { issued: if a.issued == 0 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) } else { a.issued }, texts: plugin_texts(lang), ..a };
    bridge::write_assignment(&status.dir, &a).map_err(|e| e.to_string())?;
    Ok(json!({ "issued": a.issued }))
}

fn reports() -> Answer {
    let dir = plugins_dir()?;
    let mut c = load();
    let new = c.reports.read_new(&install::plugin_dir(&dir));
    save(&c)?;
    serde_json::to_value(new).map_err(|e| e.to_string())
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// A command of ours starting with `addon_`, or None when it is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "addon_state" => state(),
        "addon_set_folder" => arg::<String>(args, "folder").and_then(|f| set_folder(&f)),
        "addon_clear" => clear(),
        "addon_enable" => arg::<bool>(args, "on").and_then(enable),
        "addon_assign" => (|| assign(arg::<Assignment>(args, "assignment")?, &arg::<Option<String>>(args, "lang")?.unwrap_or_default()))(),
        "addon_reports" => reports(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_of_an_older_launcher_reads() {
        // (every field has its default: a file without the newer ones keeps the folder)
        let c: Choice = serde_json::from_str(r#"{"folder":"C:\\Games\\openOMSI"}"#).unwrap();
        assert_eq!((c.folder.as_str(), c.reports.offset), ("C:\\Games\\openOMSI", 0));
        assert_eq!(serde_json::from_str::<Choice>("{}").unwrap(), Choice::default());
    }

    #[test]
    fn every_text_the_add_on_says_is_translated() {
        let mut texts: Vec<&str> = install::TEXTS.to_vec();
        texts.extend(compat::NOTES);
        for f in compat::FEATURES {
            texts.push(f.label);
            if !f.fallback.is_empty() {
                texts.push(f.fallback);
            }
        }
        for p in [detect::Problem::Missing, detect::Problem::Omsi2Only, detect::Problem::NoGame] {
            texts.push(p.text());
        }
        texts.extend(["No openOMSI folder chosen", "The Omsi-Hub plugin is not switched on in this openOMSI"]);
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                match table.get(*t) {
                    // (the holes stay, so the plugin and the page fill them)
                    Some(tr) if t.matches("%{").count() == tr.matches("%{").count() => {}
                    _ => missing.push(format!("{lang}: {t:?}")),
                }
            }
        }
        assert!(missing.is_empty(), "{} missing or with other holes (crates/omsi-app/locales/tauri-addon.yml):\n{}", missing.len(), missing.join("\n"));
        // (the plugin gets them in the player's language)
        assert_eq!(plugin_texts("de").get("Start duty").map(String::as_str), crate::i18n::table("de").get("Start duty").map(String::as_str));
    }
}

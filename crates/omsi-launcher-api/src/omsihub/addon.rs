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
//! * `addon_set_mode {on}` - the add-on mode: Omsi-Hub starts the chosen openOMSI for every
//!   drive (`core::choose_game`, without the arguments only Omsi-Hub's own game knows), or its
//!   own copy again (the default);
//! * `addon_assign {assignment, lang}` - the duty for the plugin, its texts in `lang`;
//! * `addon_reports` - the plugin's reports since the last call, its trips booked first
//!   (`book`).
//!
//! In the add-on mode every start of a game gives the plugin its assignment first
//! (`assign_duty`: the company's duty, or the player's own drive), and the page's poll books
//! what the plugin reported (`book`): each trip goes into the driver's record
//! (`~/.openomsi/trips/<driver>.jsonl`, `core::append_trip`), which the company's day takes in
//! as it takes the trips Omsi-Hub's own game writes there.
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
    /// The add-on mode: the chosen openOMSI is started for the drives (off: Omsi-Hub's own copy).
    pub use_folder: bool,
    /// The last assignment given in the add-on mode: its reports are booked for its driver.
    pub assigned: Option<Assigned>,
}

/// What an assignment's reports are booked with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assigned {
    pub issued: u64,
    pub driver: String,
    pub map: String,
    pub bus: String,
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

/// The game the launcher starts for the choice `c` (`core::choose_game`): the folder's in the
/// add-on mode, when it is there; else its own.
fn apply(c: &Choice) {
    let chosen = (c.use_folder && !c.folder.is_empty()).then(|| detect::inspect(Path::new(&c.folder), &home()).ok()).flatten();
    core::choose_game(chosen.map(|i| core::ChosenGame { exe: i.exe, own_args: i.omsihub_build }));
}

/// The choice saved by an earlier start taken up, once (`crate::call` asks before every
/// command).
pub fn apply_saved() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| apply(&load()));
}

/// Whether the drives go to the chosen openOMSI now.
fn in_addon_mode(c: &Choice) -> bool {
    c.use_folder && core::chosen_game().is_some()
}

fn state() -> Answer {
    let mut c = load();
    apply(&c);
    let bundled = !in_addon_mode(&c);
    if c.folder.is_empty() {
        return Ok(json!({ "folder": null, "bundled": true, "use_folder": false }));
    }
    let base = json!({ "folder": c.folder, "bundled": bundled, "use_folder": c.use_folder });
    let i = match inspect(&mut c) {
        Ok(i) => i,
        Err(p) => return Ok(json!({ "folder": c.folder, "bundled": true, "use_folder": c.use_folder, "problem": p.text() })),
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

fn set_mode(on: bool) -> Answer {
    let mut c = load();
    if on && c.folder.is_empty() {
        return Err("No openOMSI folder chosen".into());
    }
    c.use_folder = on;
    save(&c)?;
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

/// Give the plugin the duty of a drive about to start, in the add-on mode with the plugin on
/// (otherwise nothing happens): `company` names the company's duty (its name and the duty's
/// id); a drive of the player's own gets an assignment as well, so that its trips are booked
/// for the right driver. Said in the launcher's log when it fails: the drive starts anyway.
pub fn assign_duty(d: &core::Duty, company: Option<(&str, &str)>) {
    let mut c = load();
    if !in_addon_mode(&c) {
        return;
    }
    let Ok(dir) = plugins_dir() else { return };
    if install::status(&dir).state != install::State::On {
        return;
    }
    let config = core::load_config();
    let driver = d.profile.clone().filter(|p| !p.trim().is_empty()).unwrap_or(config.profile);
    let lang = core::get_settings().ok().and_then(|s| s.get("language").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    let (name, duty) = company.unwrap_or_default();
    let a = Assignment {
        company: name.to_string(),
        duty: duty.to_string(),
        driver: driver.trim().to_string(),
        map: d.map.clone(),
        line: d.line.clone().unwrap_or_default(),
        tour: d.tour.clone().unwrap_or_default(),
        trip: d.trip.as_deref().and_then(|t| t.trim().parse().ok()).unwrap_or(0),
        ..Assignment::default()
    };
    match assign(a, omsi_cfg::lang::language_iso(&lang)) {
        Ok(v) => {
            c.assigned = Some(Assigned { issued: v["issued"].as_u64().unwrap_or(0), driver: driver.trim().to_string(), map: d.map.clone(), bus: d.bus.clone() });
            let _ = save(&c);
        }
        Err(e) => core::log_to_file(&format!("add-on: no assignment for the plugin: {e}")),
    }
}

/// A trip the plugin reported, as the driver's record keeps it (`core::TripRun`), booked for
/// the assignment it names (`a`; None: the launcher's driver).
fn trip_run(r: &bridge::Report, a: Option<&Assigned>) -> Option<core::TripRun> {
    if !r.is_trip() || r.trip == 0 {
        return None;
    }
    let driver = a.map(|a| a.driver.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| core::load_config().profile);
    let pick = |ours: &str, theirs: Option<&str>| if ours.is_empty() { theirs.unwrap_or_default().to_string() } else { ours.to_string() };
    let line = if r.number.trim().is_empty() { r.line.trim() } else { r.number.trim() };
    // (as the game's own report writes it: "5 Rathaus" is the line's terminus Rathaus)
    let terminus = r.terminus.trim();
    let terminus = if line.is_empty() { terminus } else { terminus.strip_prefix(&format!("{line} ")).unwrap_or(terminus) };
    Some(core::TripRun {
        time: if r.at > 0.0 { r.at as u64 } else { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) },
        driver,
        map: pick(&r.map, a.map(|a| a.map.as_str())),
        bus: pick(&r.bus, a.map(|a| a.bus.as_str())),
        line: line.to_string(),
        tour: r.tour.trim().to_string(),
        terminus: terminus.trim().to_string(),
        trip: r.trip as usize,
        trips: r.trips as usize,
        departure: r.departure.unwrap_or_default(),
        arrival: r.arrival.unwrap_or_default(),
        completed: r.how == "arrived",
        worst: r.delay,
        average: r.delay,
        ..Default::default()
    })
}

/// Book what the plugin reported since the last time (in the add-on mode): each trip into its
/// driver's record. The reports read are returned.
pub fn book() -> Vec<bridge::Report> {
    let mut c = load();
    if !in_addon_mode(&c) {
        return Vec::new();
    }
    let Ok(dir) = plugins_dir() else { return Vec::new() };
    let before = c.reports.offset;
    let new = c.reports.read_new(&install::plugin_dir(&dir));
    if c.reports.offset == before {
        return new;
    }
    let data = core::data_dir();
    for r in &new {
        let a = c.assigned.as_ref().filter(|a| a.issued == r.issued && r.issued != 0);
        if let Some(t) = trip_run(r, a) {
            if let Err(e) = core::append_trip(&data, &t) {
                core::log_to_file(&format!("add-on: a trip of line {} not booked: {e:#}", t.line));
            }
        }
    }
    let _ = save(&c);
    new
}

fn reports() -> Answer {
    plugins_dir()?;
    serde_json::to_value(book()).map_err(|e| e.to_string())
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
        "addon_set_mode" => arg::<bool>(args, "on").and_then(set_mode),
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
    fn a_trip_the_plugin_reported_goes_into_its_driver_s_record() {
        let a = Assigned { issued: 7, driver: "Luc".into(), map: "maps/Grundorf/global.cfg".into(), bus: "Vehicles/MAN_SD202/SD202.bus".into() };
        let r = bridge::Report { kind: "trip".into(), issued: 7, line: "Linie5".into(), tour: "2".into(), trip: 3, trips: 6, number: "5".into(), terminus: "5 Rathaus".into(), departure: Some(30_000.0), arrival: Some(31_800.0), how: "arrived".into(), delay: Some(65.0), at: 1_760_000_000.0, ..Default::default() };
        let t = trip_run(&r, Some(&a)).unwrap();
        assert_eq!((t.driver.as_str(), t.map.as_str(), t.bus.as_str()), ("Luc", "maps/Grundorf/global.cfg", "Vehicles/MAN_SD202/SD202.bus"));
        // (the line as its displays show it, the terminus without it - as the game's own report)
        assert_eq!((t.line.as_str(), t.tour.as_str(), t.terminus.as_str(), t.trip, t.trips), ("5", "2", "Rathaus", 3, 6));
        assert_eq!((t.time, t.departure, t.arrival, t.completed, t.worst), (1_760_000_000, 30_000.0, 31_800.0, true, Some(65.0)));
        // (the bus and map the plugin saw win; a trip given up is not completed)
        let given_up = bridge::Report { how: "given_up".into(), bus: "Vehicles/X/x.bus".into(), ..r.clone() };
        let t = trip_run(&given_up, Some(&a)).unwrap();
        assert_eq!((t.bus.as_str(), t.completed), ("Vehicles/X/x.bus", false));
        assert!(trip_run(&bridge::Report { kind: "duty_start".into(), ..r }, Some(&a)).is_none());
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
        texts.extend(["No openOMSI folder chosen", "The Omsi-Hub plugin is not switched on in this openOMSI", "Start this openOMSI for my drives", "Omsi-Hub starts its own copy of the game."]);
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

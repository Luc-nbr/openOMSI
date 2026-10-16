//! The files between Omsi-Hub and its plugin in the game: plain files in the plugin's folder,
//! so they work with every openOMSI whose Lua plugins have a folder of their own, and nothing
//! of the game is touched.
//!
//! Omsi-Hub to the plugin:
//!
//! * `assignment.lua` beside `main.lua`: the duty the company gave the driver, its notices and
//!   the plugin's texts in the player's language ([`Assignment`]), as a Lua table the plugin
//!   `require`s - the one way in that every Lua plugin API has. openOMSI loads a folder plugin
//!   again when one of its `.lua` files changes, so a new assignment written while the game
//!   runs reaches the plugin at once.
//! * `data/notices.json`: notices that come while the player drives, read every few seconds
//!   where the game has `files.read` and `json.decode` (the `live_notices` feature).
//! * `data/messages.json`: the driver's inbox of the control room's messages ([`Inbox`]: in the
//!   player's language, with the buttons' words), read every few seconds like the notices (the
//!   `messages` feature, `lua/messages.lua`; docs/omsihub/MESSAGES.md).
//!
//! The plugin to Omsi-Hub:
//!
//! * `data/hello.json`: the handshake ([`Hello`]) - the game's version, its plugin API's ABI
//!   and which of the functions the add-on uses it has (`crate::compat` negotiates from it);
//! * `data/reports.jsonl`: one [`Report`] a line - the duty taken, each trip done with its
//!   ratings, the duty ended - appended as they happen ([`ReportReader`] reads what is new);
//! * `data/replies.jsonl`: one [`Reply`] a line - a message delivered, read, accepted or
//!   declined, a canned reply, a quick button to control ([`ReplyReader`]);
//! * `data/status.json`: how the duty stands, written every few seconds ([`Status`]: the trip,
//!   its delay, how much of it is left) - what the AI dispatcher looks at.
//!
//! A game without `files.write` / `files.append` gets the same lines into its log instead
//! (`omsi.log`, `[lua omsihub] OMSIHUB-HELLO {...}`, `OMSIHUB-REPORT {...}` and
//! `OMSIHUB-REPLY {...}`), which [`from_log`] and [`replies_from_log`] read back.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The version of these files' format, in [`Assignment::bridge`] and [`Hello::bridge`]: raised
/// only when a field changes its meaning (a new field is just added, every one has a default).
pub const BRIDGE: u32 = 1;

/// The assignment's file, beside the plugin's `main.lua`.
pub const ASSIGNMENT_FILE: &str = "assignment.lua";
/// The texts' file, beside it.
pub const TEXTS_FILE: &str = "texts.lua";
/// The plugin's data folder (openOMSI's, for a folder plugin) and the files in it.
pub const DATA_DIR: &str = "data";
pub const NOTICES_FILE: &str = "notices.json";
pub const HELLO_FILE: &str = "hello.json";
pub const REPORTS_FILE: &str = "reports.jsonl";
pub const MESSAGES_FILE: &str = "messages.json";
pub const REPLIES_FILE: &str = "replies.jsonl";
pub const STATUS_FILE: &str = "status.json";
/// What the plugin's log lines start with when it cannot write files.
pub const LOG_HELLO: &str = "OMSIHUB-HELLO ";
pub const LOG_REPORT: &str = "OMSIHUB-REPORT ";
pub const LOG_REPLY: &str = "OMSIHUB-REPLY ";

/// A notice of the company for the driver.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notice {
    /// Shown once per id.
    pub id: String,
    pub title: String,
    pub text: String,
    /// `"info"`, `"warning"` or `"alert"` (as `omsi.game.notify` takes them).
    pub kind: String,
}

/// A driver's pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Personnel {
    pub number: String,
    pub code: String,
}

/// The duty the company assigned, for the plugin.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assignment {
    pub bridge: u32,
    /// When Omsi-Hub wrote it (Unix seconds): the reports name it, so Omsi-Hub knows which
    /// assignment a report is about.
    pub issued: u64,
    pub company: String,
    pub driver: String,
    /// The company's own id of the duty.
    pub duty: String,
    /// The map (`maps/<folder>/global.cfg`), the line and tour of its timetable.
    pub map: String,
    pub line: String,
    pub tour: String,
    /// The trip to start with and the stop to start at (from 1; 0: the first).
    pub trip: u32,
    pub stop: u32,
    /// Take the duty by itself as soon as the player is in a bus (else the plugin offers it).
    pub auto_start: bool,
    /// A port on this computer Omsi-Hub listens to for the reports as they happen (0: none).
    pub udp_port: u16,
    pub notices: Vec<Notice>,
    /// The driver's pass, when the driver signs on before the duty (Omsi-Hub's sign-on): the
    /// personnel number and the code. None: no sign-on.
    pub personnel: Option<Personnel>,
    /// The duty board on the screen (None: shown).
    pub board: Option<bool>,
    /// The game types the duty's trips into the IBIS (None: as the game does; false: the
    /// driver types them). With a sign-on they wait for the duty order in any case.
    pub ibis: Option<bool>,
    /// The bus of the drive (its `.bus` file), and the font its destination displays are drawn
    /// in ("": its own; `bus.set_display_font`).
    pub bus: String,
    pub display_font: String,
    /// The map's object editor on once the world is there (the editors' "edit this map").
    pub editor: bool,
    /// The plugin's texts in the player's language: English text -> translation.
    pub texts: BTreeMap<String, String>,
}

/// The plugin's handshake.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hello {
    pub bridge: u32,
    /// The plugin's own version (`crate::PLUGIN_VERSION` when it was installed).
    pub plugin: String,
    /// The game's version (`omsi.game.version()`; "" in a game without it).
    pub game: String,
    /// The game's plugin API ABI (`omsi.game.api()`; 1 in a game without it, which is older
    /// than the first ABI change).
    pub api: i64,
    pub platform: String,
    pub language: String,
    /// Each function the add-on uses, and whether the game has it.
    pub functions: BTreeMap<String, bool>,
    /// The assignment the plugin found (its `issued`; 0: none).
    pub issued: u64,
}

/// One thing that happened on the duty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    /// `"duty_start"`, `"trip"` (a trip done), `"duty_end"`; a kind a newer plugin adds is
    /// kept as it is.
    pub kind: String,
    /// The assignment it is about (0: a duty the player took without one).
    pub issued: u64,
    pub duty: String,
    pub line: String,
    pub tour: String,
    /// The trip's number in the duty (from 1) and the timetable's name of it.
    pub trip: u32,
    pub trip_name: String,
    /// The duty's trips, and the trip's line as its displays show it, its terminus and when it
    /// was to leave its first stop and reach its last (seconds of the day): `duty.trips`.
    pub trips: u32,
    pub number: String,
    pub terminus: String,
    pub departure: Option<f64>,
    pub arrival: Option<f64>,
    /// The bus driven, its `.bus` file (`bus.file`) and the map's `global.cfg` (`map.info`).
    pub bus: String,
    pub map: String,
    /// A trip's record as the driver's record keeps it (Omsi-Hub's `TripRun`: the stops served,
    /// early and late, when the bus was at each, the distance, the drive watch's findings),
    /// where the plugin followed the trip from its start.
    pub record: Option<serde_json::Value>,
    /// How the trip ended: `"arrived"`, `"skipped"`, `"given_up"`.
    pub how: String,
    /// The trip's ratings in per cent, where the game gives them.
    pub driving: Option<f64>,
    pub comfort: Option<f64>,
    pub tickets: Option<f64>,
    /// Seconds late at the trip's end (early negative).
    pub delay: Option<f64>,
    /// The real time (Unix seconds) and the game's clock (seconds since midnight).
    pub at: f64,
    pub clock: Option<f64>,
}

impl Report {
    pub fn is_trip(&self) -> bool {
        self.kind == "trip"
    }
}

/// A button the driver has: its key (what the reply says) and its words.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub key: String,
    pub label: String,
    /// A material icon's name.
    pub icon: String,
}

/// What an instruction asks the game to do (`company::messages::What`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InboxAction {
    /// `"short_turn"`, `"out_of_service"`, `"layover"`, `"swap"`, `"hold"`, `"relief"`.
    pub kind: String,
    /// The trip it is done at: when it leaves its first stop (seconds of the day).
    pub departure: f64,
    /// A short-turn's or out of service's share of the trip not run in service.
    pub part: f64,
    /// A hold's minutes.
    pub minutes: i32,
    /// A relief's stop (its name).
    pub stop: String,
    /// What it is, in a few words.
    pub label: String,
}

/// A message in the driver's inbox, in the player's language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InboxMsg {
    pub id: u64,
    /// The company's time it was sent (`"14:05"`).
    pub time: String,
    /// Who sent it, in words; `mine`: the driver's own to control.
    pub from: String,
    pub mine: bool,
    /// `"info"`, `"instruction"`, `"urgent"`.
    pub priority: String,
    pub text: String,
    pub action: Option<InboxAction>,
    /// `"sent"`, `"delivered"`, `"read"`, `"accepted"`, `"declined"`, and in words.
    pub state: String,
    pub state_label: String,
    /// The driver's canned reply, in words.
    pub reply: String,
    /// What came of an accepted instruction where it could not be done.
    pub outcome: String,
}

/// The driver's inbox (`data/messages.json`, and the phone's): the messages of the duty he
/// drives, the newest first, with the words of his buttons.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Inbox {
    pub bridge: u32,
    pub company: String,
    /// The duty it is for: its line (the timetable's name) and tour.
    pub line: String,
    pub tour: String,
    pub messages: Vec<InboxMsg>,
    /// The quick buttons to control, and the canned replies.
    pub quick: Vec<Choice>,
    pub replies: Vec<Choice>,
    /// The inbox's own words (English key -> the player's language: "Messages", "Accept", ...),
    /// for a screen without the plugin's texts (the phone).
    pub words: BTreeMap<String, String>,
}

/// What the driver did with the control room's messages, or tells it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reply {
    /// `"delivered"`, `"read"`, `"accept"`, `"decline"`, `"reply"` (a canned reply: `key`),
    /// `"quick"` (a quick button to control: `key`).
    pub kind: String,
    /// The message it is about (none for `"quick"`).
    pub id: u64,
    pub key: String,
    /// The assignment and the duty it comes from.
    pub issued: u64,
    pub line: String,
    pub tour: String,
    /// Its own id on the device (the same line twice counts once).
    pub uid: String,
    pub at: f64,
    pub clock: Option<f64>,
}

/// How the duty stands now, as the game says (`data/status.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    pub issued: u64,
    pub line: String,
    pub tour: String,
    /// The trip under way (from 1) of how many; its departure and arrival, the next trip's
    /// (seconds of the day).
    pub trip: u32,
    pub trips: u32,
    pub departure: Option<f64>,
    pub arrival: Option<f64>,
    pub next_departure: Option<f64>,
    pub next_arrival: Option<f64>,
    /// Seconds late (early negative).
    pub delay: Option<f64>,
    /// The trip's stops served and how many of them the bus passed.
    pub stops: u32,
    pub passed: u32,
    /// The trip is done (the bus waits for the next).
    pub trip_done: bool,
    /// The real time (Unix seconds) and the game's clock.
    pub at: f64,
    pub clock: Option<f64>,
}

/// A Lua literal of `v` (the assignment for the plugin): every string escaped so that nothing
/// in a name can end it, numbers as Lua reads them.
fn lua_value(v: &serde_json::Value, indent: usize, out: &mut String) {
    use serde_json::Value as J;
    match v {
        J::Null => out.push_str("nil"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Number(n) => out.push_str(&n.to_string()),
        J::String(s) => lua_string(s, out),
        J::Array(a) => {
            out.push_str("{\n");
            for x in a {
                out.push_str(&"  ".repeat(indent + 1));
                lua_value(x, indent + 1, out);
                out.push_str(",\n");
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
        J::Object(m) => {
            out.push_str("{\n");
            for (k, x) in m {
                out.push_str(&"  ".repeat(indent + 1));
                out.push('[');
                lua_string(k, out);
                out.push_str("] = ");
                lua_value(x, indent + 1, out);
                out.push_str(",\n");
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
    }
}

fn lua_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // (the other control characters by their number, three digits so a digit after
            // one is not read into it)
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Assignment {
    /// The assignment as the Lua the plugin `require`s.
    pub fn to_lua(&self) -> String {
        let mut out = String::from("-- Written by Omsi-Hub: the duty the company assigned. Changed by Omsi-Hub, not by hand.\nreturn ");
        lua_value(&serde_json::to_value(self).unwrap_or_default(), 0, &mut out);
        out.push('\n');
        out
    }
}

/// Write a file through a temporary one beside it, so the game never reads half of it.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("omsihub-new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Give the plugin installed in `plugin_dir` its assignment (its version of the files set).
pub fn write_assignment(plugin_dir: &Path, a: &Assignment) -> std::io::Result<()> {
    let a = Assignment { bridge: BRIDGE, ..a.clone() };
    write_atomic(&plugin_dir.join(ASSIGNMENT_FILE), &a.to_lua())
}

/// Take the assignment back (the plugin then drives without one).
pub fn clear_assignment(plugin_dir: &Path) -> std::io::Result<()> {
    write_atomic(&plugin_dir.join(ASSIGNMENT_FILE), "-- Written by Omsi-Hub: no duty assigned.\nreturn nil\n")
}

/// The plugin's texts in the player's language (English text -> translation), beside its
/// `main.lua` (`texts.lua`): read when the plugin starts, assignment or none.
pub fn write_texts(plugin_dir: &Path, texts: &BTreeMap<String, String>) -> std::io::Result<()> {
    let mut out = String::from("-- Written by Omsi-Hub: the plugin's texts in the player's language. Changed by Omsi-Hub, not by hand.\nreturn ");
    lua_value(&serde_json::to_value(texts).unwrap_or_default(), 0, &mut out);
    out.push('\n');
    write_atomic(&plugin_dir.join(TEXTS_FILE), &out)
}

/// The notices for a game that reads them while it runs.
pub fn write_notices(plugin_dir: &Path, notices: &[Notice]) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(notices).unwrap_or_else(|_| "[]".into());
    write_atomic(&plugin_dir.join(DATA_DIR).join(NOTICES_FILE), &text)
}

/// The driver's inbox for a game that reads it while it runs (written only when it changed:
/// whether it was).
pub fn write_inbox(plugin_dir: &Path, inbox: &Inbox) -> std::io::Result<bool> {
    let inbox = Inbox { bridge: BRIDGE, ..inbox.clone() };
    let text = serde_json::to_string_pretty(&inbox).unwrap_or_else(|_| "{}".into());
    let path = plugin_dir.join(DATA_DIR).join(MESSAGES_FILE);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    write_atomic(&path, &text)?;
    Ok(true)
}

/// How the duty stands, as the plugin last wrote it.
pub fn read_status(plugin_dir: &Path) -> Option<Status> {
    let text = std::fs::read_to_string(plugin_dir.join(DATA_DIR).join(STATUS_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The plugin's last handshake, if it ever ran.
pub fn read_hello(plugin_dir: &Path) -> Option<Hello> {
    let text = std::fs::read_to_string(plugin_dir.join(DATA_DIR).join(HELLO_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Reads `data/reports.jsonl` as it grows: what came since the last read. A line the plugin
/// has not finished yet is left for the next read; a line that is not a report is skipped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportReader {
    /// Bytes of the file read so far.
    pub offset: u64,
}

impl ReportReader {
    /// The reports added to the plugin's file since the last call.
    pub fn read_new(&mut self, plugin_dir: &Path) -> Vec<Report> {
        read_lines(&mut self.offset, &plugin_dir.join(DATA_DIR).join(REPORTS_FILE))
    }
}

/// Reads `data/replies.jsonl` as it grows, as [`ReportReader`] reads the reports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplyReader {
    pub offset: u64,
}

impl ReplyReader {
    /// The replies added to the plugin's file since the last call.
    pub fn read_new(&mut self, plugin_dir: &Path) -> Vec<Reply> {
        read_lines(&mut self.offset, &plugin_dir.join(DATA_DIR).join(REPLIES_FILE))
    }
}

/// The lines of a JSON-lines file added since `offset` (moved on past them).
fn read_lines<T: serde::de::DeserializeOwned>(offset: &mut u64, path: &Path) -> Vec<T> {
    {
        let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        // (a file shorter than what was read was started again: from its beginning)
        if len < *offset {
            *offset = 0;
        }
        if f.seek(SeekFrom::Start(*offset)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        if f.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return Vec::new() };
        *offset += end as u64 + 1;
        String::from_utf8_lossy(&bytes[..end]).lines().filter_map(|l| serde_json::from_str(l.trim()).ok()).collect()
    }
}

/// The handshake and the reports a plugin without file functions put into the game's log
/// (`~/.openomsi/game.log`), in order.
pub fn from_log(log: &str) -> (Option<Hello>, Vec<Report>) {
    let mut hello = None;
    let mut reports = Vec::new();
    for line in log.lines() {
        if let Some(k) = line.find(LOG_HELLO) {
            if let Ok(h) = serde_json::from_str(line[k + LOG_HELLO.len()..].trim()) {
                hello = Some(h);
            }
        } else if let Some(k) = line.find(LOG_REPORT) {
            if let Ok(r) = serde_json::from_str(line[k + LOG_REPORT.len()..].trim()) {
                reports.push(r);
            }
        }
    }
    (hello, reports)
}

/// The replies a plugin without file functions put into the game's log, in order.
pub fn replies_from_log(log: &str) -> Vec<Reply> {
    log.lines().filter_map(|l| l.find(LOG_REPLY).and_then(|k| serde_json::from_str(l[k + LOG_REPLY.len()..].trim()).ok())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("omsihub-bridge-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_assignment_is_lua_that_cannot_be_broken_out_of() {
        let a = Assignment { issued: 7, line: "9\"]] os.exit() --".into(), tour: "a\\b\nc\u{1}2".into(), notices: vec![Notice { id: "n1".into(), text: "Straße".into(), ..Notice::default() }], ..Assignment::default() };
        let lua = a.to_lua();
        assert!(lua.contains(r#"["line"] = "9\"]] os.exit() --""#), "{lua}");
        assert!(lua.contains(r#"["tour"] = "a\\b\nc\0012""#), "{lua}");
        assert!(lua.contains("Straße"));
        assert!(lua.starts_with("-- Written by Omsi-Hub") && lua.contains("\nreturn {"));
    }

    #[test]
    fn new_reports_are_read_once_and_half_lines_wait() {
        let d = dir("reports");
        let file = d.join(DATA_DIR).join(REPORTS_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut r = ReportReader::default();
        assert!(r.read_new(&d).is_empty());
        std::fs::write(&file, "{\"kind\":\"duty_start\",\"issued\":7,\"line\":\"9\"}\nnot json\n{\"kind\":\"trip\",\"trip\":1,\"driving\":93.5,\"how\":\"arr").unwrap();
        let first = r.read_new(&d);
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].kind.as_str(), first[0].issued, first[0].line.as_str()), ("duty_start", 7, "9"));
        // (the plugin finishes the line, and a kind of a newer plugin comes)
        let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        std::io::Write::write_all(&mut f, b"ived\",\"future\":1}\n{\"kind\":\"refuel\"}\n").unwrap();
        let next = r.read_new(&d);
        assert_eq!(next.len(), 2);
        assert!(next[0].is_trip());
        assert_eq!((next[0].driving, next[0].how.as_str()), (Some(93.5), "arrived"));
        assert_eq!(next[1].kind, "refuel");
        assert!(r.read_new(&d).is_empty());
        // (the file started again)
        std::fs::write(&file, "{\"kind\":\"duty_end\"}\n").unwrap();
        assert_eq!(r.read_new(&d)[0].kind, "duty_end");
    }

    #[test]
    fn the_log_carries_the_same_lines() {
        let log = "[INFO] openOMSI 0.2.40\n[INFO omsi_plugin] [lua omsihub] OMSIHUB-HELLO {\"bridge\":1,\"game\":\"0.2.40\",\"api\":1}\n[INFO omsi_plugin] [lua omsihub] OMSIHUB-REPORT {\"kind\":\"trip\",\"trip\":2}\n[lua other] OMSIHUB-REPORT broken\n";
        let (hello, reports) = from_log(log);
        assert_eq!(hello.unwrap().game, "0.2.40");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].trip, 2);
    }

    #[test]
    fn files_round_trip() {
        let d = dir("files");
        write_assignment(&d, &Assignment { duty: "D-12".into(), ..Assignment::default() }).unwrap();
        let lua = std::fs::read_to_string(d.join(ASSIGNMENT_FILE)).unwrap();
        assert!(lua.contains("[\"bridge\"] = 1") && lua.contains("D-12"));
        clear_assignment(&d).unwrap();
        assert!(std::fs::read_to_string(d.join(ASSIGNMENT_FILE)).unwrap().contains("return nil"));
        write_notices(&d, &[Notice { id: "x".into(), ..Notice::default() }]).unwrap();
        let back: Vec<Notice> = serde_json::from_str(&std::fs::read_to_string(d.join(DATA_DIR).join(NOTICES_FILE)).unwrap()).unwrap();
        assert_eq!(back[0].id, "x");
        assert!(read_hello(&d).is_none());
        std::fs::write(d.join(DATA_DIR).join(HELLO_FILE), "{\"api\":1,\"functions\":{\"duty.get\":true},\"newer\":[1]}").unwrap();
        let h = read_hello(&d).unwrap();
        assert_eq!((h.api, h.functions.get("duty.get")), (1, Some(&true)));
    }

    #[test]
    fn the_messages_channel_goes_both_ways() {
        let d = dir("messages");
        let inbox = Inbox { line: "Linie5".into(), tour: "1".into(), messages: vec![InboxMsg { id: 3, text: "You are running 6 min late".into(), priority: "info".into(), ..InboxMsg::default() }], ..Inbox::default() };
        // written once, again only when it changed
        assert!(write_inbox(&d, &inbox).unwrap());
        assert!(!write_inbox(&d, &inbox).unwrap());
        let back: Inbox = serde_json::from_str(&std::fs::read_to_string(d.join(DATA_DIR).join(MESSAGES_FILE)).unwrap()).unwrap();
        assert_eq!((back.bridge, back.messages[0].id), (BRIDGE, 3));
        // the replies as they come, a half line waiting
        let file = d.join(DATA_DIR).join(REPLIES_FILE);
        std::fs::write(&file, "{\"kind\":\"delivered\",\"id\":3,\"uid\":\"a1\"}\n{\"kind\":\"acc").unwrap();
        let mut r = ReplyReader::default();
        let first = r.read_new(&d);
        assert_eq!((first.len(), first[0].kind.as_str(), first[0].id), (1, "delivered", 3));
        let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        std::io::Write::write_all(&mut f, b"ept\",\"id\":3}\n").unwrap();
        assert_eq!(r.read_new(&d)[0].kind, "accept");
        // the status, and the log of a game without files
        assert!(read_status(&d).is_none());
        std::fs::write(d.join(DATA_DIR).join(STATUS_FILE), "{\"line\":\"10\",\"trip\":2,\"delay\":390.0}").unwrap();
        let st = read_status(&d).unwrap();
        assert_eq!((st.line.as_str(), st.trip, st.delay), ("10", 2, Some(390.0)));
        let log = "[lua omsihub] OMSIHUB-REPLY {\"kind\":\"quick\",\"key\":\"breakdown\"}\n[lua omsihub] OMSIHUB-REPORT {}\n";
        assert_eq!(replies_from_log(log)[0].key, "breakdown");
    }
}

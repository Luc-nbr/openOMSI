//! What RosterFlow's tutorial keeps (`crates/omsi-launcher-ui/src/company/planning/
//! rf_tutorial.rs` and `company/tutorial.rs`): what a driver has seen of its tour and lessons -
//! in `~/.openomsi/rosterflow-tour.json`, the same file our game-drawn launcher keeps it in, so
//! that both launchers know it - and a lesson's snapshot: the company as the lesson found it
//! (`co::rosterflow::Snapshot`, and the Timeplan's line for its lesson), put back when the
//! player undoes the lesson. The tour, the lessons' steps and the help sheet are the page's.

use super::super::company::words::tr;
use super::super::company::{self, Session};
use super::{str_arg, usize_arg, with_rf, Rf};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::rosterflow as rf;
use omsi_launcher_lib::lines::LineDesign;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a driver has seen of RosterFlow's tour and lessons (`tutorial::FlowSeen`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FlowSeen {
    pub seen: bool,
    pub at: usize,
    pub done: bool,
    pub lessons: Vec<String>,
}

fn file() -> PathBuf {
    core::data_dir().join("rosterflow-tour.json")
}

fn read_all(path: &Path) -> BTreeMap<String, FlowSeen> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_all(path: &Path, all: &BTreeMap<String, FlowSeen>) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Err(e) = std::fs::write(path, serde_json::to_vec_pretty(all).unwrap_or_default()) {
        core::log_to_file(&format!("rosterflow tour: not kept: {e}"));
    }
}

fn profile() -> String {
    core::load_config().profile.trim().to_string()
}

pub fn flow_of(profile: &str) -> FlowSeen {
    read_all(&file()).get(profile.trim()).cloned().unwrap_or_default()
}

fn keep(profile: &str, s: FlowSeen) {
    let path = file();
    let mut all = read_all(&path);
    all.insert(profile.trim().to_string(), s);
    write_all(&path, &all);
}

/// A lesson on its way: the company as it found it, and the Timeplan's line (its lesson's).
pub(crate) struct Kept {
    pub snap: rf::Snapshot,
    pub tp: Option<(String, LineDesign, Vec<(String, usize)>)>,
}

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_tut" => Ok(serde_json::to_value(flow_of(&profile())).unwrap_or(Value::Null)),
        "rf_tut_act" => with_rf(|s, r| act(s, r, args)),
        _ => return None,
    })
}

fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let who = profile();
    match str_arg(args, "act").as_str() {
        // the tour started by itself the first time
        "seen" => {
            let mut f = flow_of(&who);
            f.seen = true;
            keep(&who, f);
        }
        // the tour left at its stop `at`: gone through or skipped
        "left" => {
            let mut f = flow_of(&who);
            f.seen = true;
            if args.get("completed").and_then(Value::as_bool).unwrap_or(false) {
                f.done = true;
                f.at = 0;
            } else {
                f.at = usize_arg(args, "at");
            }
            keep(&who, f);
        }
        "lesson_done" => {
            let id = str_arg(args, "id");
            let mut f = flow_of(&who);
            if !f.lessons.iter().any(|x| *x == id) {
                f.lessons.push(id);
                keep(&who, f);
            }
        }
        // a lesson begins: the company kept as it is now
        "begin" => {
            let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
            let tp = (str_arg(args, "lesson") == "timeplan").then(|| r.tp.keep_for_lesson()).flatten();
            r.lesson = Some(Kept { snap: rf::Snapshot::take(c), tp });
        }
        // the Timeplan's line kept as soon as it is read (the timeplan lesson)
        "keep_tp" => {
            if let Some(l) = r.lesson.as_mut() {
                if l.tp.is_none() {
                    l.tp = r.tp.keep_for_lesson();
                }
            }
        }
        // the lesson over: its changes kept, or the company put back as it found it
        "end" => {
            let Some(lesson) = r.lesson.take() else { return Ok(json!({})) };
            if !args.get("undo").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(json!({ "say": { "text": tr("The lesson is over: your changes stay."), "error": false } }));
            }
            let mut ok = true;
            if let Some((line, design, unsynced)) = lesson.tp.as_ref() {
                ok = super::timeplan::restore(s, r, line, design, unsynced.clone());
            }
            let snap = lesson.snap.clone();
            let _ = company::act(s, |c| {
                snap.restore(c);
                Ok(())
            });
            r.cache = None;
            r.clear_edits();
            return Ok(if ok {
                json!({ "say": { "text": tr("The lesson is undone: the planning is as it was before."), "error": false } })
            } else {
                json!({ "say": { "text": tr("The planning is as it was, but the timetable could not be put back: change it back in the Timeplan tab."), "error": true } })
            });
        }
        other => return Err(format!("no action {other}")),
    }
    Ok(json!({}))
}

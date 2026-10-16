//! What a driver has seen of the bus company's tour (Trello "Tauri 6/6"), kept as our
//! game-drawn launcher keeps it (`crates/omsi-launcher-ui/src/company/tutorial.rs`): per driver
//! in `~/.openomsi/company-tour.json`, the same file, so that either launcher goes on where the
//! other left it. The first time a driver opens the company its tour starts by itself (the
//! welcome, the founding before there is a company, its pages once there is one); the "?" in
//! the company's bar goes on from where it was left, or from the start once it was seen to its
//! end. The tour itself is the page's (`tour.js`).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a driver has seen of the company's tour (`tutorial::Seen`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Seen {
    pub seen: bool,
    pub at: usize,
    pub done: bool,
    pub pages_to_come: bool,
    pub layout: u32,
    pub casual: bool,
}

/// The company's layout now (`tutorial::LAYOUT`): 1 the start screen with a tile for every page,
/// 2 the sidebar, the slim bar and the command palette (Beta 65) - a driver who saw an older
/// one is shown round its pages once more.
const LAYOUT: u32 = 2;

fn file() -> PathBuf {
    omsi_launcher_lib::data_dir().join("company-tour.json")
}

fn read_all() -> BTreeMap<String, Value> {
    std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn of(profile: &str) -> Seen {
    read_all().get(profile.trim()).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
}

fn keep(profile: &str, s: &Seen) {
    // (the drivers' other fields as they are: a launcher of later may know more of them)
    let mut all = read_all();
    all.insert(profile.trim().to_string(), serde_json::to_value(s).unwrap_or(Value::Null));
    let path = file();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Err(e) = std::fs::write(&path, serde_json::to_vec_pretty(&all).unwrap_or_default()) {
        omsi_launcher_lib::log_to_file(&format!("company tour: not kept: {e}"));
    }
}

/// The stop of the tour of the tabs' days in the start screen's tour (`tutorial::from_tabs`).
fn from_tabs(at: usize) -> usize {
    match at {
        0 => 0,
        1 => 3,
        2 => 1,
        k => k + 2,
    }
}

/// Starting by itself now, for a driver who has seen `s` (`tutorial::on_open`).
fn on_open(s: &Seen, company: bool) -> Option<(usize, bool)> {
    if !s.seen {
        return Some((0, !company));
    }
    if (s.pages_to_come || s.layout < LAYOUT) && company {
        return Some((1, false));
    }
    None
}

/// Casual's interface shown round once to a driver who saw Normal's tour (`casual_due`).
fn casual_due(s: &Seen, casual: bool) -> Option<(usize, bool)> {
    (casual && s.seen && !s.casual).then_some((1, false))
}

/// Where the "?" starts the tour (`resume_at`).
fn resume_at(s: &Seen) -> usize {
    if s.done {
        0
    } else if s.layout < 1 {
        from_tabs(s.at)
    } else {
        s.at
    }
}

/// `co_tour`: "open" (the company's screen came, with a company running or not, in Casual or
/// not: the stop to start at by itself, or none - `tutorial::frame`), "ask" (the "?": where to
/// go on - `tutorial::ask`) or "left" (the tour ended at its stop `at`, gone through to its end
/// with `completed` - `tutorial::left`).
pub fn answer(args: &Value) -> Value {
    let profile = omsi_launcher_lib::load_config().profile;
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    match args.get("what").and_then(Value::as_str).unwrap_or("") {
        "open" => {
            let (company, casual) = (flag("company"), flag("company") && flag("casual"));
            let mut s = of(&profile);
            let Some((at, to_come)) = on_open(&s, company).or_else(|| casual_due(&s, casual)) else { return json!({ "at": null }) };
            s.seen = true;
            s.pages_to_come = to_come;
            if company {
                s.layout = LAYOUT;
            }
            if casual {
                s.casual = true;
            }
            keep(&profile, &s);
            json!({ "at": at })
        }
        "ask" => json!({ "at": resume_at(&of(&profile)) }),
        "left" => {
            let mut s = of(&profile);
            s.seen = true;
            s.layout = LAYOUT;
            if flag("completed") {
                if !s.pages_to_come {
                    s.done = true;
                }
                s.at = 0;
            } else {
                s.at = args.get("at").and_then(Value::as_u64).unwrap_or(0) as usize;
            }
            keep(&profile, &s);
            json!({ "at": s.at })
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the game-drawn launcher's: once by itself, on with the pages once founded, the "?"
    /// where it was left - and the file of before read the same.
    #[test]
    fn the_tour_starts_once_and_goes_on_where_it_was_left() {
        let new = Seen::default();
        assert_eq!(on_open(&new, false), Some((0, true)));
        assert_eq!(on_open(&new, true), Some((0, false)));
        let welcomed = Seen { seen: true, pages_to_come: true, ..Default::default() };
        assert_eq!(on_open(&welcomed, true), Some((1, false)));
        let skipped = Seen { seen: true, at: 5, layout: LAYOUT, ..Default::default() };
        assert_eq!((on_open(&skipped, true), resume_at(&skipped)), (None, 5));
        assert_eq!(resume_at(&Seen { done: true, ..skipped.clone() }), 0);
        assert_eq!(casual_due(&Seen { done: true, ..skipped }, true), Some((1, false)));
        let old: Seen = serde_json::from_str(r#"{ "seen": true, "at": 3, "done": false, "pages_to_come": false }"#).unwrap();
        assert_eq!((on_open(&old, true), resume_at(&old)), (Some((1, false)), 5));
        // (the tiles' layout seen, not the sidebar's: round the pages once more, its stops as they are)
        let tiles = Seen { seen: true, done: true, at: 4, layout: 1, ..Default::default() };
        assert_eq!((on_open(&tiles, true), resume_at(&Seen { done: false, ..tiles })), (Some((1, false)), 4));
    }
}

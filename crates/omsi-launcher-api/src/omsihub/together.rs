//! "Play together" in the bus company, the launcher's side (Beta 58; the company's side is
//! `omsi_launcher_lib::company::together`): the boss starts a session in his company (`host`:
//! the company server, its code, who is online and what each drives, the claims to say yes or
//! no to, his world opened as the host of a game session), and friends join it with the code
//! (`join`: the boss's company in a simple view of its own - the day, the open duties, their
//! own duties with Drive, their results - their game started on their duty in the boss's world).
//!
//! The commands are `tg_…`; the texts come in the page's language (`lang`), as the company's.

mod host;
mod join;

use super::company::words::{self, tr, trf};
use crate::{Answer, Platform};
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::together::wire::Notice;
use serde_json::{json, Value};

pub use host::{boss_duty, give_to_guest, online_guests};

/// The colour a player of "Play together" has on the rosters.
pub const GUEST_COLOUR: &str = "#e58fd8";

/// A command of "Play together", or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    if cmd.starts_with("tg_host") {
        host::remember_language();
        return host::call(cmd, args, p);
    }
    join::call(cmd, args, p)
}

/// A notice as the page says it.
pub fn notice_text(n: &Notice) -> String {
    let args: Vec<(String, String)> = n.args.iter().map(|(k, v)| (k.clone(), if k == "name" || k == "text" || k == "tour" { v.clone() } else { tr(v) })).collect();
    let refs: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    trf(&n.text, &refs)
}

/// A notice's time of day (the computer's).
fn clock_of(at: u64) -> String {
    let local = at as i64 + omsi_launcher_lib::utc_offset();
    let m = local.rem_euclid(86400) / 60;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// A notice for the page: its time and its words.
pub fn notice_json(n: &Notice) -> Value {
    json!({ "n": n.n, "at": clock_of(n.at), "text": notice_text(n) })
}

/// A duty in short: "Line 5 · Tour 2 · Duty 1".
pub fn duty_words(number: &str, r: &DutyRef) -> String {
    format!("{} {}  ·  {} {}  ·  {} {}", tr("Line"), number, tr("Tour"), r.tour, tr("Duty"), r.duty + 1)
}

/// A duty as a page sends it back.
fn duty_arg(args: &Value) -> Option<DutyRef> {
    serde_json::from_value(args.get("duty").cloned().unwrap_or(Value::Null)).ok()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

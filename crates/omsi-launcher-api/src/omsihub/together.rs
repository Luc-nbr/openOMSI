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

/// "Play together" is open (Luc 2026-10-15: "block the multiplayer buttons for now because
/// that's still in beta"): only with `OMSI_TOGETHER` set. Its code stays; without the switch
/// the pages show it as coming soon and every command is refused, so no server starts.
pub fn enabled() -> bool {
    omsi_cfg::flags::OMSI_TOGETHER.is_set()
}

/// What the pages say while it is blocked.
pub const BLOCKED: &str = "Multiplayer - coming soon (beta)";

/// The gate's answer to a command: whether it is open, and what is said while it is not.
pub fn gate() -> Value {
    json!({ "on": enabled(), "label": tr(BLOCKED) })
}

/// A command of "Play together", or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    if cmd == "tg_gate" {
        return Some(Ok(gate()));
    }
    // (blocked: nothing runs - no server, no joining - and the badge is off)
    if !enabled() {
        return Some(Ok(json!({ "off": true, "on": false, "tip": tr(BLOCKED) })));
    }
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

#[cfg(test)]
mod tests {
    use omsi_launcher_lib::company::together::{book, client, code, host};

    /// Every text of "Play together" - the company's side, the launcher's and the page's
    /// (`together.js`) - in the six languages.
    #[test]
    fn every_together_text_is_translated() {
        let mut texts: Vec<String> = book::texts().into_iter().chain(host::texts()).chain([code::NOT_A_CODE, code::GAME_CODE, code::TYPO, code::NEWER, client::UNREACHED]).map(String::from).collect();
        let js = include_str!("../../../../launcher/ui/js/omsihub/company/together.js");
        // (this file without its tests)
        let own = include_str!("together.rs").split("#[cfg(test)]").next().unwrap_or("");
        let sources = [(own, "tr(\"", '"'), (include_str!("together/host.rs"), "tr(\"", '"'), (include_str!("together/join.rs"), "tr(\"", '"'), (js, "tr('", '\'')];
        for (src, lead, end) in sources {
            for lead in [lead.to_string(), lead.replacen("tr(", "trf(", 1)] {
                let mut rest = src;
                while let Some(k) = rest.find(lead.as_str()) {
                    let after = &rest[k + lead.len()..];
                    rest = after;
                    let mut t = String::new();
                    let mut chars = after.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => t.extend(chars.next()),
                            c if c == end => break,
                            c => t.push(c),
                        }
                    }
                    if t.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                        texts.push(t);
                    }
                }
            }
        }
        assert!(texts.len() > 100, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsi-app/locales/company-together.yml):\n{}", missing.len(), missing.join("\n"));
    }

    struct NoPlatform;
    impl crate::Platform for NoPlatform {
        fn renderer(&self, _req: serde_json::Value, _wait: std::time::Duration) -> Result<(serde_json::Value, Vec<u8>), String> {
            Err(String::new())
        }
    }

    /// Blocked for now: without `OMSI_TOGETHER` every command is refused - no server starts,
    /// nobody joins - and the gate says so.
    #[test]
    fn playing_together_is_blocked_without_its_switch() {
        if super::enabled() {
            return;
        }
        let p = NoPlatform;
        let gate = super::call("tg_gate", &serde_json::json!({}), &p).unwrap().unwrap();
        assert_eq!(gate["on"], false);
        for cmd in ["tg_host_start", "tg_host_badge", "tg_join", "tg_view"] {
            let v = super::call(cmd, &serde_json::json!({ "code": "ABC" }), &p).unwrap().unwrap();
            assert_eq!(v["off"], true, "{cmd}");
        }
        assert!(super::host::hosting_none(), "no session was started");
    }
}

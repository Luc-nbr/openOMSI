//! The rest of the bus company's pages in the desktop launcher's page (Trello "Tauri 3/6"),
//! working as they work in our game-drawn launcher (`crates/omsi-launcher-ui/src/company/`):
//! the depot, the workshop's stores, the concessions and their auctions, the contracts, the
//! research, the fleet map and the planning without the RosterFlow add-on.
//!
//! A child of `company` (declared there with one line, its commands handed over with one
//! more), so that it works on the same session - the company open, its clock, its plans - and
//! the same pieces as phase 2's pages, while it stays a file of its own for easy merges:
//!
//! * `co_more_view`: the view of what a page shows (`view`: "depot", "workshop" ...), every
//!   text translated and every amount written in the page's language;
//! * `co_more_act`: what a click does (`act`), its refusal the popup that says why;
//! * `co_more_map`: the fleet map's roads and the routes of the company's trips, read by the
//!   game program (`mapview`'s export, through the launcher service).
//!
//! What the pages keep between their views besides the session (the fleet map's clock, the
//! lines' weeks the concessions read) lives in `More`, locked after the session.

#[path = "company_more/concessions.rs"]
mod concessions;
#[path = "company_more/contracts.rs"]
mod contracts;
#[path = "company_more/depot.rs"]
mod depot;
#[path = "company_more/fleetmap.rs"]
mod fleetmap;
#[path = "company_more/planning.rs"]
mod planning;
// (what "Fill the roster" left open, also said when an own line's change took effect: `due`)
pub(super) use planning::open_text;
// (Trello "Tauri 6/6": the planning's sheet of a line's trips and timetable)
#[path = "company_more/plan_edit.rs"]
mod plan_edit;
#[path = "company_more/research.rs"]
mod research;
// (Beta 66: "Request a change" of a tendered line's contract)
#[path = "company_more/variations.rs"]
pub(super) mod variations;
#[path = "company_more/workshop.rs"]
mod workshop;

use super::{with, Session};
use crate::{Answer, Platform};
use serde_json::{json, Value};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// What the pages of this part keep besides the session, for the company they were made for.
#[derive(Default)]
pub struct More {
    company: Option<String>,
    pub tenders: concessions::TendersState,
    pub map: fleetmap::MapState,
    /// The planning's sheet of a line, while it is open (`plan_edit`).
    pub edit: Option<plan_edit::PlanEdit>,
}

fn more_state() -> &'static Mutex<More> {
    static M: OnceLock<Mutex<More>> = OnceLock::new();
    M.get_or_init(Default::default)
}

/// This part's own state, for the session's open company (another company: made anew). Taken
/// with the session held (`with`): the session first, then this.
pub fn more(s: &Session) -> MutexGuard<'static, More> {
    let mut m = more_state().lock().unwrap_or_else(|e| e.into_inner());
    let id = s.company.as_ref().map(|c| c.id.clone());
    if m.company != id {
        *m = More { company: id, ..Default::default() };
    }
    m
}

/// This part's own state of a viewer of the pages (`company::Viewer`): the planning's sheet of a
/// line, the tenders and the fleet map.
#[derive(Default)]
pub struct Kept {
    tenders: concessions::TendersState,
    map: fleetmap::MapState,
    edit: Option<plan_edit::PlanEdit>,
}

/// Swap a viewer's own state of this part with the session's (`company::Viewer`).
pub fn swap_kept(s: &Session, k: &mut Kept) {
    let mut m = more(s);
    std::mem::swap(&mut m.tenders, &mut k.tenders);
    std::mem::swap(&mut m.map, &mut k.map);
    std::mem::swap(&mut m.edit, &mut k.edit);
}

/// A command of this part run for a player of "Play together" (`company::relayed`).
pub fn relayed(s: &mut Session, cmd: &str, args: &Value) -> Answer {
    match cmd {
        "co_more_view" => view(s, args),
        "co_more_act" => act(s, args),
        other => Err(format!("no command {other}")),
    }
}

fn str_arg(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or("").to_string()
}

fn u32_arg(args: &Value, name: &str) -> u32 {
    args.get(name).and_then(Value::as_u64).unwrap_or(0) as u32
}

/// A popup as an action's answer.
fn popup_answer(p: Value) -> Answer {
    Ok(json!({ "popup": p }))
}

/// A command of this part (`co_more_*`), or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    Some(match cmd {
        "co_more_view" => with(|s| view(s, args)),
        "co_more_act" => with(|s| act(s, args)),
        // (the game program reads the map: the session is not held meanwhile)
        "co_more_map" => fleetmap::roads(args, p),
        _ => return None,
    })
}

/// The view of a page of this part.
fn view(s: &mut Session, args: &Value) -> Answer {
    if s.company.is_none() {
        return Ok(json!({ "none": true }));
    }
    let what = str_arg(args, "view");
    Ok(match what.as_str() {
        "depot" => depot::view(s, args),
        "workshop" => workshop::view(s, args),
        "research" => research::view(s, args),
        "concessions" => concessions::view(s, args),
        "contracts" => contracts::view(s, args),
        "variation" => variations::view(s, args),
        "fleetmap" => fleetmap::view(s, args),
        "planning" => planning::view(s, args),
        "service" => planning::service_view(s, args),
        // (the sheet gone - another company, the line given up - says so: `none`)
        "plan_edit" => {
            let mut m = more(s);
            let shown = m.edit.as_mut().and_then(|e| plan_edit::view(s, e));
            if shown.is_none() {
                m.edit = None;
            }
            shown.unwrap_or_else(|| json!({ "none": true }))
        }
        other => return Err(format!("no view {other}")),
    })
}

/// What a click on a page of this part does.
fn act(s: &mut Session, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    // the planning's sheet of a line (`plan_edit`): opened, closed, and what its controls do
    if what == "pe_close" {
        more(s).edit = None;
        return Ok(json!({}));
    }
    if what.starts_with("pe_") {
        let mut m = more(s);
        let Some(e) = m.edit.as_mut() else { return Ok(json!({ "closed": true })) };
        let r = plan_edit::act(s, e, &what, args).unwrap_or_else(|| Err(format!("no action {what}")));
        if r.as_ref().is_ok_and(|v| v.get("closed").is_some()) {
            m.edit = None;
        }
        return r;
    }
    let r = depot::act(s, &what, args)
        .or_else(|| workshop::act(s, &what, args))
        .or_else(|| research::act(s, &what, args))
        .or_else(|| concessions::act(s, &what, args))
        .or_else(|| contracts::act(s, &what, args))
        .or_else(|| variations::act(s, &what, args))
        .or_else(|| fleetmap::act(s, &what, args))
        .or_else(|| planning::act(s, &what, args))
        .unwrap_or_else(|| Err(format!("no action {what}")));
    r.map(|mut v| {
        // (the session's own popup goes with the answer)
        if v.get("popup").is_none() {
            if let Some(p) = s.popup.take() {
                v["popup"] = p;
            }
        }
        v
    })
}

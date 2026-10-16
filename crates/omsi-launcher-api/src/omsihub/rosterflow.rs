//! RosterFlow in the desktop launcher's page (Trello "Tauri 4/6"): the bus company's planning
//! with the add-on on, working as it works in our game-drawn launcher
//! (`crates/omsi-launcher-ui/src/company/planning/`: `rosterflow.rs`, `rf_gantt.rs`,
//! `rf_panels.rs`, `rf_prefs.rs`, `rf_print.rs`, `rf_tutorial.rs`, `rf_roster.rs`,
//! `timeplan.rs`). The rules are the core's (`company::studio`, `rosterflow`, `timeplan`,
//! `rosterpdf`, `plan`), called here the way the game-drawn launcher calls them; the page
//! (`launcher/ui/js/omsihub/rosterflow/`) draws what these commands say - the Gantt on a canvas.
//!
//! * The company is the bus company's session (`company::with`); what RosterFlow keeps besides
//!   it - the day's board made, Optimise's proposal, the figures before the last edit, undo and
//!   redo, the timeplan's line being changed, a lesson's snapshot - lives here, for the company
//!   open (another one opened: begun anew).
//! * `rf_board` says the day shown as the Vehicles and Drivers tabs draw it: its blocks and
//!   duties, their events, issues and figures (only when it changed since the page's `rev`).
//! * `rf_act` edits the draft (`co::rosterflow::Flow::draft`) as the Gantt, the cards, the menu
//!   and the panels do, each edit kept for Undo; SAVE and DISCARD; Optimise, its proposal
//!   accepted or not; the preferences; a line's colour; the PDF prints.
//! * `rf_view` answers what a card or a panel needs besides the board (the duty panel, the trip
//!   card's buses, the preferences, the drop zones of what is taken up).
//! * `rf_tp*` is the Timeplan (`timeplan`), `rf_roster*` the Roster tab (`roster`), `rf_tut*`
//!   the tutorial's kept state (`tutorial`).
//!
//! Every text comes translated (`company::words`), every amount in the page's language.

/// The plan copied to the other days it works with (Trello #100).
mod days;
mod print;
mod roster;
// (its Own, load_own and commit save the planning's sheet of a line too: Omsi-Hub 6/6)
pub(crate) mod timeplan;
mod tutorial;

/// Luc's driver drag (`rf-driver-drag`): single trips of a duty to another duty or a new one.
mod driver_drag;

/// A duty's breaks and driving periods as the page shows them, and the rules as the Preferences
/// change them (testers, 2026-10-14).
mod breaks;
/// The relief points (Preferences) and the reliefs on the way (the board).
mod relief;

/// Several planners at once (company multiplayer): who changed what, who is in RosterFlow, each
/// one's undo kept clear of the others' changes.
pub(crate) mod live;

use super::company::words::{day_label, eur, tr, trf};
use super::company::{self, Session};
use crate::{Answer, Platform};

use omsi_launcher_lib::company::network::{PlannedTrip, TourOfDay};
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, Draft, EvKind, Figures, Scope, Travel};
use omsi_launcher_lib::company::studio::{self as st, JoinedDuty, LineLayout, Target};
use omsi_launcher_lib::company::{self as co, Cents, Company};
use omsi_launcher_lib::LineInfo;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};

// --- what RosterFlow keeps -------------------------------------------------------------------------

/// The day as shown, made once for a date and the company's generation (`rosterflow::Cached`).
#[derive(Clone)]
pub(crate) struct Cached {
    pub date: String,
    pub generation: u64,
    pub board: Arc<Board>,
    pub view: Arc<Company>,
    pub lines: Arc<Vec<LineInfo>>,
    /// The board as the page draws it, and its revision.
    pub json: Arc<Value>,
    pub rev: String,
}

/// Optimise's proposal for a date and generation: the board and what to keep. `base` is the
/// revision of the day it was made from: a change of the company that leaves the day as it was
/// (the week's timetables read, the clock's minutes) keeps it, as the game-drawn launcher keeps it.
/// `better`: it weighs less than the day as it is (else it is the day as it is, nothing to
/// accept); `note` what Optimise did in words, or why it could not.
#[derive(Clone)]
struct Proposal {
    date: String,
    generation: u64,
    base: String,
    board: Arc<Board>,
    view: Arc<Company>,
    layouts: Vec<LineLayout>,
    joined: Vec<JoinedDuty>,
    better: bool,
    note: String,
}

/// RosterFlow's state beside the company's session.
#[derive(Default)]
pub(crate) struct Rf {
    /// The company it is of (another opened: begun anew).
    company: String,
    pub(crate) cache: Option<Cached>,
    proposal: Option<Proposal>,
    /// The figures before the last edit, of a date (the KPIs' differences).
    before: Option<(String, Figures)>,
    /// Undo and redo: the draft before each edit, and the edit in words.
    undo: Vec<(Draft, String)>,
    redo: Vec<(Draft, String)>,
    /// Edits made so far (undone ones too), for the lessons.
    edits: u64,
    /// The Timeplan (`timeplan`).
    pub(crate) tp: timeplan::Tp,
    /// A lesson's snapshot (`tutorial`).
    pub(crate) lesson: Option<tutorial::Kept>,
    /// The Roster tab's undo and redo: the roster and the day's choices before each change
    /// (`roster::Kept`).
    roster_undo: Vec<roster::Kept>,
    roster_redo: Vec<roster::Kept>,
    /// What this editor knows of the others' changes (`live`).
    live: live::Mine,
}

fn state() -> &'static Mutex<Rf> {
    static R: OnceLock<Mutex<Rf>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Rf::default()))
}

/// Run `f` on the company's session and RosterFlow's state (always in this order: the session
/// first), RosterFlow's begun anew for another company.
pub(crate) fn with_rf<T>(f: impl FnOnce(&mut Session, &mut Rf) -> T) -> T {
    company::with(|s| {
        let mut r = state().lock().unwrap_or_else(|e| e.into_inner());
        let id = s.company.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        if r.company != id {
            *r = Rf { company: id, ..Default::default() };
        }
        f(s, &mut r)
    })
}

impl Rf {
    /// Undo, redo, Optimise's proposal and the figures before forgotten (a lesson undone).
    pub(crate) fn clear_edits(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.proposal = None;
        self.before = None;
    }
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

fn str_arg(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or("").to_string()
}

fn usize_arg(args: &Value, name: &str) -> usize {
    args.get(name).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn usizes(args: &Value, name: &str) -> Vec<usize> {
    args.get(name).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect()).unwrap_or_default()
}

/// Pairs of indices (`[[block, trip], ...]`).
fn pairs(args: &Value, name: &str) -> Vec<(usize, usize)> {
    let pair = |v: &Value| Some((v.get(0)?.as_u64()? as usize, v.get(1)?.as_u64()? as usize));
    args.get(name).and_then(Value::as_array).map(|a| a.iter().filter_map(pair).collect()).unwrap_or_default()
}

/// A command of RosterFlow, or None when `cmd` is not one: the boss's own, on his launcher.
pub fn call(cmd: &str, args: &Value, _p: &dyn Platform) -> Option<Answer> {
    company::words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    match cmd {
        "rf_print" => return Some(print::print(args)),
        c if c.starts_with("rf_tut") => return tutorial::call(c, args),
        _ => {}
    }
    with_rf(|s, r| run_as(s, r, live::Editor::Boss, cmd, args))
}

/// A command of RosterFlow run for `by` on the session with his RosterFlow state: what the
/// others changed meanwhile looked at first (`live::before`), a change of his kept for them
/// after (`live::after`), and what he is to be told of theirs with the answers that tell it.
fn run_as(s: &mut Session, r: &mut Rf, by: live::Editor, cmd: &str, args: &Value) -> Option<Answer> {
    live::before(s, r, by);
    let gen = s.generation;
    let out = run(s, r, by, cmd, args)?;
    let date = date_of(s, args).unwrap_or_default();
    live::after(s, r, by, gen, change_of(cmd, args), &date);
    Some(out.map(|mut v| {
        if ["rf_board", "rf_act", "rf_roster_act", "rf_tp_act"].contains(&cmd) && v.is_object() {
            if let Some(c) = s.company.as_ref() {
                let told = live::told(c, r, by);
                if !told.is_empty() {
                    v["told"] = json!(told);
                }
            }
        }
        v
    }))
}

/// The commands of RosterFlow (besides the print and the tutorial's), on any editor's state.
fn run(s: &mut Session, r: &mut Rf, by: live::Editor, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_on" => Ok(on_json(s)),
        "rf_board" => board_answer(s, r, args),
        "rf_view" => view(s, r, args),
        "rf_act" => act(s, r, by, args),
        // (the page's word every few seconds: who else is in RosterFlow, what they changed)
        "rf_here" => Ok(live::here(s, r, by, args)),
        c if c.starts_with("rf_tp") => return timeplan::run(s, r, c, args),
        c if c.starts_with("rf_roster") => return roster::run(s, r, c, args),
        _ => return None,
    })
}

/// What kind of change of the plan a command makes, when it makes one (the rest show, or keep
/// what is the page's own: a line folded, the Gantt's colours, Optimise's proposal).
fn change_of(cmd: &str, args: &Value) -> Option<live::Kind> {
    let what = str_arg(args, "act");
    match cmd {
        "rf_act" => match what.as_str() {
            "save" | "discard" | "day_reset" | "day_unfollow" | "copy_rows" => Some(live::Kind::Saved),
            "prefs" | "opt_flags" => Some(live::Kind::Prefs),
            "colour_by" | "pick" | "revalidate" | "keep_mine" | "optimise" => None,
            _ => Some(live::Kind::Blocks),
        },
        "rf_roster_act" if !what.starts_with("fold") => Some(live::Kind::Roster),
        "rf_tp_act" => Some(live::Kind::Timetable),
        _ => None,
    }
}

/// A command of RosterFlow run for player `guest` of the company's session, relayed by his
/// launcher (`together::relay`, which checked his permissions `perms`), on his own RosterFlow
/// state `r` (`company::Viewer`). The print comes back as the file's bytes, saved on his
/// launcher; the tutorial is his launcher's own.
pub(crate) fn relayed(s: &mut Session, r: &mut Rf, guest: u32, cmd: &str, args: &Value, perms: &[omsi_launcher_lib::company::together::Perm]) -> Answer {
    use omsi_launcher_lib::company::together::Perm;
    let id = s.company.as_ref().map(|c| c.id.clone()).unwrap_or_default();
    if r.company != id {
        *r = Rf { company: id, ..Default::default() };
    }
    match cmd {
        "rf_print" => print::for_afar(s, r, args),
        _ => {
            let mut v = run_as(s, r, live::Editor::Guest(guest), cmd, args).unwrap_or_else(|| Err(format!("no command {cmd}")))?;
            if cmd == "rf_on" {
                let may = |p: Perm| perms.contains(&p);
                let why = (!may(Perm::Plans)).then(|| tr("Your role lets you look at the planning, not change it: the boss can give you \"Edit the planning\" on his Multiplayer page."));
                v["remote"] = json!({ "plans": may(Perm::Plans), "lines": may(Perm::Lines), "assign": may(Perm::Assign), "why": why });
            }
            Ok(v)
        }
    }
}

/// A graph's PDF file made by the boss's launcher saved on a player's (`together::join`).
pub(crate) fn save_pdf(name: &str, bytes: &[u8]) -> Value {
    print::save(name, bytes)
}

/// A change of the planning made on its own pages by player `guest` (`together::host`), told to
/// the planners in RosterFlow.
pub(crate) fn planning_changed(s: &Session, guest: u32) {
    if let Some(c) = s.company.as_ref() {
        live::planning_changed(c, live::Editor::Guest(guest));
    }
}

/// Whether the company open has RosterFlow on (the planning is RosterFlow then), its day and the
/// next six (the day switcher's), and the names its parts go by.
fn on_json(s: &mut Session) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({ "open": false, "on": false }) };
    let days: Vec<Value> = (0..7).map(|k| {
        let date = co::dates::add(&c.date, k);
        let label = if k == 0 { format!("{}  ·  {}", day_label(&date), tr("Company day")) } else { day_label(&date) };
        json!({ "date": date, "label": label })
    }).collect();
    json!({
        "open": true,
        "on": co::autopilot::rosterflow(c),
        "company": c.id,
        "name": c.name,
        "date": c.date,
        "days": days,
        "casual": c.game_mode.casual(),
        "trips_to_duties": TRIPS_TO_DUTIES,
    })
}

// --- the board -----------------------------------------------------------------------------------

/// The ID a bus's block goes by: its line's number and its name ("1/02").
pub(crate) fn block_label(t: &TourOfDay) -> String {
    let tour = t.tour.trim();
    let tour = if tour.len() == 1 && tour.chars().all(|c| c.is_ascii_digit()) { format!("0{tour}") } else { tour.to_string() };
    format!("{}/{}", t.number.trim(), tour)
}

/// Whether a trip runs outbound: from where the line's first trip of the day leaves.
pub(crate) fn outbound(tours: &[TourOfDay], line: &str, x: &PlannedTrip) -> bool {
    let home = tours.iter().filter(|t| t.line.eq_ignore_ascii_case(line)).flat_map(|t| t.trips.iter()).filter(|y| y.counts()).min_by_key(|y| y.dep).map(|y| y.from.trim().to_string());
    home.is_none_or(|h| x.from.trim().eq_ignore_ascii_case(&h))
}

/// A row of the Gantt by what it is (kept across the day's rebuilds): a bus's block (its line
/// and name) or a duty (its key).
fn block_id(t: &TourOfDay) -> String {
    format!("B|{}|{}", t.line, t.tour)
}

fn duty_id(s: &rf::Shaped) -> String {
    match (&s.key, s.spare) {
        (Some(k), _) => format!("D|{}/{}/{}/{}", k.line, k.tour, k.first.name, k.first.dep),
        (None, Some(id)) => format!("D|spare/{id}"),
        _ => "D|".into(),
    }
}

/// `#rrggbb` of a line's colour (`co::rosterflow::line_colour`).
fn colour_of(c: &Company, line: &str) -> String {
    rf::hex(rf::line_colour(c, line))
}

/// The day's first and last minute, to whole hours (`rf_gantt::day_span`).
fn day_span(board: &Board) -> (i32, i32) {
    let t0 = board.tours.iter().map(|t| t.from()).chain(board.shaped.iter().map(|s| s.start)).min().unwrap_or(300) - 20;
    let t1 = board.tours.iter().map(|t| t.to()).chain(board.shaped.iter().map(|s| s.end)).max().unwrap_or(1380) + 20;
    let t0 = t0.div_euclid(60) * 60;
    let t1 = (t1 + 59).div_euclid(60) * 60;
    (t0, t1.max(t0 + 180))
}

fn scope_key(s: Scope) -> &'static str {
    match s {
        Scope::Global => "global",
        Scope::Vehicle => "vehicle",
        Scope::Duty => "duty",
    }
}

fn issue_words(i: &rf::Issue) -> String {
    rf::issue_text(i, &|t| tr(t))
}

/// The row an issue is about, in words ("Duty 1003", "Vehicle 1/02").
fn issue_row_words(board: &Board, i: &rf::Issue) -> String {
    match (i.scope, i.row) {
        (Scope::Duty, Some(r)) => board.shaped.get(r).map(|s| format!("{} {}", tr("Duty"), s.label(r))).unwrap_or_default(),
        (Scope::Vehicle, Some(r)) => board.tours.get(r).map(|t| format!("{} {}", tr("Vehicle"), block_label(t))).unwrap_or_default(),
        _ => String::new(),
    }
}

/// The trips of piece `duty` of block `tour`.
fn piece_trips(board: &Board, tour: usize, duty: usize) -> &[PlannedTrip] {
    let Some(t) = board.tours.get(tour) else { return &[] };
    let r = co::network::duties_of(t).get(duty).cloned().unwrap_or(0..0);
    t.trips.get(r).unwrap_or(&[])
}

/// The board as the page draws it (`c` the company as the studio sees it).
fn board_json(c: &Company, board: &Board) -> Value {
    let wd = board.weekday;
    let p = &c.planning.flow.prefs;
    let tours = &board.tours;
    let blocks: Vec<Value> = tours
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let line_trips: Vec<&PlannedTrip> = tours.iter().filter(|o| o.line.eq_ignore_ascii_case(&t.line)).flat_map(|o| o.trips.iter()).collect();
            let bad: Vec<i32> = board.block_issues(i).filter(|x| x.text == rf::I_LAYOVER).filter_map(|x| x.key.rsplit('/').next().and_then(|d| d.parse().ok())).collect();
            let trips: Vec<Value> = t
                .trips
                .iter()
                .enumerate()
                .map(|(j, x)| {
                    let mut v = json!({
                        "dep": x.dep, "arr": x.arr, "from": x.from.trim(), "to": x.end().trim(), "empty": x.empty, "counts": x.counts(),
                        "origin": x.origin.trim(), "name": x.name, "num": if x.line.trim().is_empty() { t.number.trim().to_string() } else { x.line.trim().to_string() },
                        "km": (x.km * 10.0).round() / 10.0, "out": outbound(tours, &t.line, x),
                        "linked": x.counts() && !rf::linked(c, wd, &t.line, &st::TripRef::of(x)).is_empty(),
                        "moved": !x.origin.trim().is_empty() && x.origin.trim() != t.tour.trim(),
                        "bad": bad.contains(&x.dep),
                        "sl": format!("{} > {}", co::rosterpdf::stop_letters(&x.from), co::rosterpdf::stop_letters(x.end())),
                    });
                    if j > 0 {
                        let prev = &t.trips[j - 1];
                        if !prev.end().trim().eq_ignore_ascii_case(x.from.trim()) && !prev.empty && !x.empty {
                            v["dh"] = json!(st::travel(&line_trips, prev.end(), &x.from));
                        }
                    }
                    v
                })
                .collect();
            let pieces = co::network::duties_of(t);
            // where a duty can be cut, or why not (on hover of the gap)
            let gaps: Vec<Value> = (1..t.trips.len())
                .filter(|at| !pieces.iter().any(|r| r.start == *at))
                .map(|at| match st::can_cut(c, tours, i, at) {
                    Ok(()) => json!([at, true, trf("Click to cut the duty here: a driver takes over at %{stop}.", &[("stop", t.trips[at].from.trim())])]),
                    Err(why) => json!([at, false, tr(why)]),
                })
                .collect();
            let any: Vec<&rf::Issue> = board.block_issues(i).filter(|x| !rf::dismissed(c, wd, &x.key)).collect();
            let duty_rows: Vec<Value> = (0..pieces.len()).map(|k| st::duty_of(&board.duties, i, k).and_then(|di| board.shaped.iter().position(|s| s.duty == Some(di))).map(Value::from).unwrap_or(Value::Null)).collect();
            json!({
                "id": block_id(t), "line": t.line, "number": t.number.trim(), "tour": t.tour.trim(), "label": block_label(t),
                "colour": colour_of(c, &t.line), "from": t.from(), "to": t.to(), "km": (t.km() * 10.0).round() / 10.0,
                "locked": rf::block_locked(c, wd, &t.line, &t.tour),
                "trips": trips,
                "pieces": pieces.iter().map(|r| json!([r.start, r.end])).collect::<Vec<_>>(),
                "duty_rows": duty_rows,
                "gaps": gaps,
                "relief_tips": pieces.iter().skip(1).map(|r| trf("A driver takes over here, at %{stop}. Click to join the two duties.", &[("stop", t.trips[r.start].from.trim())])).collect::<Vec<_>>(),
                "issues": { "any": !any.is_empty(), "hard": any.iter().any(|x| x.hard), "tip": any.iter().map(|x| issue_words(x)).collect::<Vec<_>>().join("\n") },
            })
        })
        .collect();
    let shaped: Vec<Value> = board
        .shaped
        .iter()
        .enumerate()
        .map(|(row, s)| {
            let any: Vec<&rf::Issue> = s.issues.iter().map(|&i| &board.issues[i]).filter(|i| !rf::dismissed(c, wd, &i.key)).collect();
            // (where it signs on and off: the point chosen, else where the bus is - `signing`)
            let (on_at, off_at) = sign_places(board, s);
            let (first_on, last_off) = (s.events.iter().position(|e| e.kind == EvKind::SignOn), s.events.iter().rposition(|e| e.kind == EvKind::SignOff));
            let events: Vec<Value> = s
                .events
                .iter()
                .enumerate()
                .map(|(k, e)| match e.kind {
                    EvKind::SignOn if Some(k) == first_on => json!({ "kind": "on", "from": e.from, "to": e.to, "at": on_at }),
                    EvKind::SignOff if Some(k) == last_off => json!({ "kind": "off", "from": e.from, "to": e.to, "at": off_at }),
                    EvKind::SignOn => json!({ "kind": "on", "from": e.from, "to": e.to }),
                    EvKind::SignOff => json!({ "kind": "off", "from": e.from, "to": e.to }),
                    EvKind::Travel(m) => json!({ "kind": "travel", "letter": m.letter(), "mode": Travel::ALL.iter().position(|x| *x == m).unwrap_or(0), "label": tr(m.label()), "from": e.from, "to": e.to }),
                    EvKind::Custom(k) => json!({ "kind": "custom", "ci": k, "label": s.custom.get(k).map(|x| tr(&x.kind)).unwrap_or_default(), "from": e.from, "to": e.to }),
                })
                .collect();
            let km: f64 = s.spans.iter().map(|sp| piece_trips(board, sp.tour, sp.duty).iter().map(|t| t.km).sum::<f64>()).sum();
            let trips: usize = s.spans.iter().map(|sp| piece_trips(board, sp.tour, sp.duty).iter().filter(|t| t.counts()).count()).sum();
            // (a spare duty: its name beside its number)
            let spare = s.spare.and_then(|id| c.planning.flow.sheet.spares.iter().find(|x| x.weekday == wd && x.id == id));
            let name = spare.map(|x| x.name.trim().to_string()).unwrap_or_default();
            let label = if name.is_empty() { s.label(row) } else { format!("{} {name}", s.label(row)) };
            json!({
                "id": duty_id(s), "label": label, "name": name, "spare": s.spare, "locked": s.locked,
                "kind": tr(s.kind_name(p)), "kind_rank": s.kind.map(|k| k as i64).unwrap_or(99), "kind_set": s.kind_set,
                "start": s.start, "end": s.end, "spread": s.spread(), "work": s.work, "paid": s.paid, "driving": s.driving, "platform": s.platform,
                "changeovers": s.changeovers, "km": (km * 10.0).round() / 10.0, "trips": trips,
                "splits": s.splits.iter().map(|x| json!([x.0, x.1])).collect::<Vec<_>>(),
                "spans": s.spans.iter().map(|x| json!({ "tour": x.tour, "duty": x.duty, "from": x.from, "to": x.to })).collect::<Vec<_>>(),
                "events": events,
                "issues": { "any": !any.is_empty(), "hard": any.iter().any(|x| x.hard), "tip": any.iter().map(|x| issue_words(x)).collect::<Vec<_>>().join("\n") },
                "rest": breaks::duty_json(p, s),
            })
        })
        .collect();
    let issues: Vec<Value> = board
        .issues
        .iter()
        .map(|i| {
            json!({
                "scope": scope_key(i.scope), "row": i.row, "text": issue_words(i), "group": i.text, "title": tr(rf::issue_title(i.text)),
                "hard": i.hard, "key": i.key, "dismissed": rf::dismissed(c, wd, &i.key),
                "row_words": issue_row_words(board, i), "timeplan": rf::timeplan_line_of(i),
            })
        })
        .collect();
    let stack: Vec<Value> = board
        .stack
        .iter()
        .map(|(line, x)| {
            let number = c.lines.iter().find(|l2| l2.name.eq_ignore_ascii_case(line)).map(|l2| l2.number.clone()).unwrap_or_default();
            json!({ "line": line, "number": number, "colour": colour_of(c, line), "dep": x.dep, "arr": x.arr, "from": x.from.trim(), "to": x.end().trim(), "num": x.line.trim(), "name": x.name, "out": outbound(tours, line, x) })
        })
        .collect();
    let (t0, t1) = day_span(board);
    let f = &board.figures;
    let open = |scope: Scope| {
        let list = board.open(c, scope, false);
        let hard = list.iter().any(|&i| board.issues[i].hard);
        json!({ "n": list.len(), "hard": hard })
    };
    json!({
        "weekday": wd,
        "span": [t0, t1],
        "blocks": blocks,
        "duties": shaped,
        "issues": issues,
        "stack": stack,
        "status": {
            "assigned": f.assigned, "trips": f.trips,
            "vehicle": open(Scope::Vehicle), "duty": open(Scope::Duty), "global": open(Scope::Global),
        },
        "probe": {
            "blocks": f.blocks, "duties": f.duties, "locked": board.shaped.iter().filter(|x| x.locked).count(),
            "issues": board.issues.iter().filter(|i| !rf::dismissed(c, wd, &i.key)).count(),
        },
    })
}

/// A difference in words and whether it is good (lower is better unless `up`).
fn diff(now: f64, was: Option<f64>, up: bool, fmt: &dyn Fn(f64) -> String) -> Value {
    let Some(w) = was else { return Value::Null };
    let d = now - w;
    if d.abs() < 1e-6 {
        return Value::Null;
    }
    let sign = if d > 0.0 { "+" } else { "-" };
    json!([format!("{sign}{}", fmt(d.abs())), (d > 0.0) == up])
}

/// The KPI dashboard's tiles (`rosterflow::kpis`): the five, and the two rows more with the
/// histograms' bars.
fn kpis_json(f: &Figures, was: Option<&Figures>) -> Value {
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let money = |x: f64| eur(x.round() as Cents);
    let hours = |m: f64| format!("{:.0}", m / 60.0);
    let hm = |m: f64| rf::hm(m.round() as i32);
    let n = |x: f64| format!("{x:.0}");
    let km_word = tr("km");
    let tile = |icon: &str, value: String, delta: Value, caption: String, formula: String| json!({ "icon": icon, "value": value, "delta": delta, "caption": caption.to_uppercase(), "formula": formula });
    let top = vec![
        tile("person", f.duties.to_string(), diff(f.duties as f64, was.map(|w| w.duties as f64), false, &n), tr("Duties"), String::new()),
        tile("directions_bus", f.blocks.to_string(), diff(f.blocks as f64, was.map(|w| w.blocks as f64), false, &n), trf("Blocks (PVR: %{n})", &[("n", &f.pvr.to_string())]), String::new()),
        tile("route", pct(f.vehicle_eff()), diff(f.vehicle_eff(), was.map(|w| w.vehicle_eff()), true, &|d| format!("{:.0}%", d * 100.0)), tr("Vehicle efficiency"), format!("({} / {} + {}): {:.0} {km_word} / ({:.0} + {:.0}) {km_word}", tr("Services"), tr("Deadheads"), tr("Services"), f.service_km, f.dead_km, f.service_km)),
        tile("schedule", pct(f.crew_eff()), diff(f.crew_eff(), was.map(|w| w.crew_eff()), true, &|d| format!("{:.0}%", d * 100.0)), tr("Crew efficiency"), format!("({} / {}): {} / {}", tr("Driving time"), tr("Paid time"), hours(f.driving as f64), hours(f.paid as f64))),
        tile("payments", money(f.total() as f64), diff(f.total() as f64, was.map(|w| w.total() as f64), false, &money), tr("Total cost"), format!("({} + {}): {} + {}", tr("Crew"), tr("Vehicles"), eur(f.crew_cost), eur(f.vehicle_cost))),
    ];
    let avg = |m: i32| if f.duties == 0 { 0.0 } else { m as f64 / f.duties as f64 };
    let more = vec![
        vec![
            tile("sync_alt", f.splits.to_string(), diff(f.splits as f64, was.map(|w| w.splits as f64), false, &n), tr("Split count"), format!("{}: {} h | {:.0}% {}", tr("Time"), f.split_time / 60, if f.duties == 0 { 0.0 } else { f.splits as f64 * 100.0 / f.duties as f64 }, tr("of duties"))),
            tile("commute", f.changeovers.to_string(), diff(f.changeovers as f64, was.map(|w| w.changeovers as f64), false, &n), tr("Changeover"), format!("{}: {}", tr("During split"), f.split_changeovers)),
            tile("history", hm(avg(f.paid)), diff(avg(f.paid), was.map(|w| if w.duties == 0 { 0.0 } else { w.paid as f64 / w.duties as f64 }), false, &hm), tr("Average paid time"), String::new()),
            tile("timer", hours(f.paid as f64), diff(f.paid as f64 / 60.0, was.map(|w| w.paid as f64 / 60.0), false, &n), tr("Paid time (hours)"), String::new()),
            tile("schedule", hours(f.spread as f64), diff(f.spread as f64 / 60.0, was.map(|w| w.spread as f64 / 60.0), false, &n), tr("Spread time (hours)"), String::new()),
        ],
        vec![
            tile("event", hours(f.work as f64), Value::Null, tr("Work time (hours)"), format!("{}: {}", tr("Average"), hm(avg(f.work)))),
            tile("speed", hours(f.driving as f64), Value::Null, tr("Driving time (hours)"), String::new()),
            tile("directions_bus", hours(f.platform as f64), Value::Null, tr("Platform time (hours)"), String::new()),
            tile("warning", eur(f.penalties), diff(f.penalties as f64, was.map(|w| w.penalties as f64), false, &money), tr("Penalties"), tr("Soft rules broken")),
            tile("tune", eur(f.algorithmic()), diff(f.algorithmic() as f64, was.map(|w| w.algorithmic() as f64), false, &money), tr("Crew algorithmic cost"), format!("({} + {}): {} + {}", tr("Duty cost"), tr("Penalties"), eur(f.crew_cost), eur(f.penalties))),
        ],
    ];
    let types: Vec<Value> = f.types.iter().filter(|t| t.1 > 0 || t.0 != rf::OTHER).map(|t| json!([tr(&t.0), t.1])).collect();
    let all_types: Vec<Value> = f.types.iter().map(|t| json!([tr(&t.0), t.1])).collect();
    let bands: Vec<Value> = rf::PAID_BANDS.iter().zip(f.paid_bands.iter()).map(|(b, n)| json!([b, n])).collect();
    json!({ "top": top, "more": more, "types": types, "all_types": all_types, "bands": bands })
}

/// The day's board, made again when the date or the company changed (`rosterflow::ensure`):
/// None while its timetable is read (`Err` when it could not be).
/// The company's generation as RosterFlow counts it: a playing clock's quiet minutes left out,
/// as the game-drawn launcher leaves them out (`Session::quiet`).
pub(crate) fn rf_generation(s: &Session) -> u64 {
    s.generation.wrapping_sub(s.quiet)
}

pub(crate) fn ensure(s: &mut Session, r: &mut Rf, date: &str) -> Option<Result<Cached, String>> {
    let generation = rf_generation(s);
    if let Some(k) = r.cache.as_ref().filter(|k| k.date == date && k.generation == generation) {
        return Some(Ok(k.clone()));
    }
    company::week_work(s);
    let c = s.company.clone()?;
    let lines = match company::lines_of(s, date) {
        Some(l) => l,
        None => {
            // (read, but not to be read: its error)
            return company::plan_of(s, date).and_then(|p| p.err()).map(Err);
        }
    };
    // (a layout the timetable no longer fits: back to automatic for its line, said)
    if !co::studio::stale(&c, &lines, date).is_empty() {
        let d = date.to_string();
        let l2 = lines.clone();
        if let Ok(gone) = company::act(s, |c| Ok(co::studio::retire_stale(c, &l2, &d))) {
            if !gone.is_empty() {
                let t = trf("The timetable changed: line %{n} runs with the automatic blocks and duties again.", &[("n", &gone.join(", "))]);
                s.say(t, true);
            }
        }
    }
    let c = s.company.clone()?;
    let view = rf::view(&c);
    let board = rf::board(&view, &lines, date);
    let mut json = board_json(&view, &board);
    relief::add_mids(&mut json, &view, &board, &lines);
    let rev = format!("{:x}", hash_of(&json.to_string()));
    let k = Cached { date: date.to_string(), generation: rf_generation(s), board: Arc::new(board), view: Arc::new(view), lines: Arc::new(lines), json: Arc::new(json), rev };
    if r.cache.as_ref().is_none_or(|old| old.date != date) {
        r.undo.clear();
        r.redo.clear();
        r.before = None;
    }
    match r.proposal.as_mut() {
        Some(p) if p.date != date || (p.generation != k.generation && p.base != k.rev) => r.proposal = None,
        Some(p) => p.generation = k.generation,
        None => {}
    }
    r.cache = Some(k.clone());
    Some(Ok(k))
}

fn hash_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// The date of day `day` of the week shown (0: the company's day).
pub(crate) fn date_of(s: &Session, args: &Value) -> Option<String> {
    let c = s.company.as_ref()?;
    let day = usize_arg(args, "day").min(6);
    Some(co::dates::add(&c.date, day as i64))
}

/// The day shown (`rf_board`: `day` 0-6, `have` the revision the page has).
fn board_answer(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(date) = date_of(s, args) else { return Ok(json!({ "none": true })) };
    let c = s.company.clone().ok_or("no company")?;
    let wd = co::plan::weekday_of(&date);
    let shown_view = rf::view(&c);
    let head = json!({
        "date": date,
        "name": format!("{}  ·  {}", c.name, day_label(&date)),
        // (the two halves of the name: a narrow title bar leaves out the day first, then the
        // company - `fitTitle` of the page)
        "company": c.name,
        "day_text": day_label(&date),
        "day_words": format!("{} {}", tr(DAY_NAMES[wd as usize % 7]).to_uppercase(), day_label(&date).split_whitespace().skip(1).take(2).collect::<Vec<_>>().join(" ").to_uppercase()),
        "service": tr(DAY_NAMES[wd as usize % 7]),
        "draft": rf::has_draft(&c),
        "undo": r.undo.iter().map(|u| u.1.clone()).collect::<Vec<_>>(),
        "redo": r.redo.len(),
        "edits": r.edits,
        "gen": s.generation,
        // (what each day of the week runs, and the day shown: `days`)
        // (and how many entries the company's agenda has then: Beta 57)
        "days": company::late::rf_marks(s, &c, company::agenda::rf_marks(&c, days::marks(&shown_view))),
        "day": days::shown(&shown_view, &date),
        "unsynced": timeplan::unsynced_on(r, &c, days::group_of(wd)),
        // (how the Gantt colours the trips: the company's choice, Layers › Colour by)
        "colour_by": c.planning.flow.colour_by,
        // (the day's trips that ran late or were dropped: today's as it runs, a closed day's)
        "outcomes": company::late::service_json(s, &c, &date),
    });
    let k = match ensure(s, r, &date) {
        None => return Ok(json!({ "head": head, "reading": tr("Reading the timetable…") })),
        Some(Err(e)) => return Ok(json!({ "head": head, "reading": e })),
        Some(Ok(k)) => k,
    };
    let v = &k.view;
    // (the company's as they are now: they are not the draft's)
    let prefs = &c.planning.flow.prefs;
    let proposal = r.proposal.clone();
    // (and the preferences, which the board does not show all of: Optimise's switches set off came
    // back on, the page keeping the answer it had - Luc, Beta 65)
    let rev = format!("{}|{}|{}|{}|{}|{:x}", k.rev, proposal.is_some(), r.undo.len(), r.redo.len(), rf::has_draft(&c), hash_of(&serde_json::to_string(prefs).unwrap_or_default()));
    // (the day as his page shows it from now: an edit made on it after somebody else changed it
    // is refused - `live::stale`)
    live::shown(&c, r, &k.rev);
    if args.get("have").and_then(Value::as_str) == Some(rev.as_str()) {
        return Ok(json!({ "same": true, "head": head }));
    }
    let (board, figures_of, against): (Value, &Figures, Option<Figures>) = match &proposal {
        Some(p) => (board_json(&p.view, &p.board), &p.board.figures, Some(k.board.figures.clone())),
        None => ((*k.json).clone(), &k.board.figures, r.before.as_ref().filter(|b| b.0 == date).map(|b| b.1.clone())),
    };
    let lines: Vec<Value> = v.lines.iter().map(|l| json!({ "name": l.name, "number": l.number, "caption": l.caption, "colour": colour_of(v, &l.name), "own_colour": !l.colour.trim().is_empty() })).collect();
    Ok(json!({
        "head": head,
        "rev": rev,
        "board": board,
        "kpis": kpis_json(figures_of, against.as_ref()),
        "proposal": proposal.is_some(),
        "proposal_better": proposal.as_ref().map(|p| p.better),
        "proposal_note": proposal.as_ref().map(|p| p.note.clone()),
        "lines": lines,
        "prefs": {
            "relief": rf::ReliefTiming::ALL.iter().position(|x| *x == prefs.relief).unwrap_or(0),
            "pre_trip": prefs.pre_trip, "post_trip": prefs.post_trip, "relief_before": prefs.relief_before,
            "optimise_vehicles": prefs.optimise_vehicles, "optimise_duties": prefs.optimise_duties,
        },
        "palette": rf::PALETTE.iter().map(|x| rf::hex(*x)).collect::<Vec<_>>(),
        "saved_same": r.lesson.as_ref().map(|l| l.snap.same_saved(&c)).unwrap_or(true),
        "colours": c.lines.iter().map(|x| x.colour.clone()).collect::<Vec<_>>(),
    }))
}

pub(crate) const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// How the Gantt may colour the trips besides by their line (`Flow::colour_by`).
const COLOUR_BY: [&str; 3] = ["trip", "block", "duty"];

/// Whether the Drivers tab takes up single trips to move them to another duty (the core's
/// `studio::plan_move_to_duty`, Luc's driver drag), the whole piece of work by Alt and a drag
/// or the menu's Move piece (`driver_drag`).
pub(crate) const TRIPS_TO_DUTIES: bool = true;

// --- what a card or a panel asks for -------------------------------------------------------------

fn view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let what = str_arg(args, "view");
    let Some(date) = date_of(s, args) else { return Ok(json!({ "none": true })) };
    match what.as_str() {
        "prefs" => return Ok(prefs_view(s, r, &date)),
        "apply" => return Ok(days::apply_view(s, &date)),
        _ => {}
    }
    let k = match ensure(s, r, &date) {
        Some(Ok(k)) => k,
        Some(Err(e)) => return Err(e),
        None => return Ok(json!({ "reading": tr("Reading the timetable…") })),
    };
    let board: &Board = &k.board;
    let c: &Company = &k.view;
    let wd = board.weekday;
    Ok(match what.as_str() {
        // the trip card's quick assign list: the buses of the line it may go to
        "candidates" => {
            let (tour, trip) = (usize_arg(args, "tour"), usize_arg(args, "trip"));
            let Some(t) = board.tours.get(tour) else { return Ok(json!([])) };
            let list: Vec<Value> = (0..board.tours.len())
                .filter(|&i| i != tour && board.tours[i].line.eq_ignore_ascii_case(&t.line) && st::plan_move(c, wd, &board.tours, tour, trip, Target::Block(i)).is_ok())
                .map(|i| json!({ "tour": i, "label": block_label(&board.tours[i]) }))
                .collect();
            json!(list)
        }
        "zones" => json!(zones_for(&k, board, &hand_of(args))),
        // (no zone for a duty's trips: why, said in the corner - Luc's line 880 offered none)
        "why" => match hand_of(args) {
            Hand::DutyTrips { duty, trips, rows } => {
                // (and why not to row `to`: a drop there refused)
                let to = args.get("to").and_then(Value::as_u64).map(|x| x as usize);
                let again = |v: &Company| co::network::tours_of_day(v, &k.lines, &k.date);
                json!({ "text": driver_drag::why(c, board, duty, &trips, &rows, to, &again) })
            }
            _ => json!({}),
        },
        "duty" => duty_view(c, board, usize_arg(args, "row"), &k.lines),
        // (the Preferences as edited, not saved yet: what breaks each rule with them, next to its
        // fields while the window is open)
        "prefs_check" => {
            let prefs: rf::Prefs = arg(args, "prefs")?;
            let mut v = c.clone();
            v.planning.flow.prefs = prefs;
            breaks::prefs_check(board, &rf::board(&v, &k.lines, &k.date))
        }
        other => return Err(format!("no view {other}")),
    })
}

/// What is taken up (`rosterflow::Hand`), as the page hands it.
#[derive(Clone, Debug, PartialEq)]
enum Hand {
    Trips { tour: usize, trips: Vec<usize> },
    Stack(usize),
    Piece { tour: usize, duty: usize },
    /// Trips of a duty (the Drivers tab's row `duty`, an index into the board's shaped duties)
    /// taken up one by one, as (block, trip) pairs on one bus or several, to go to another duty
    /// or a new one (Luc's change on `rf-driver-drag`, the game-drawn launcher's `Hand::Duty`:
    /// the core's `studio::plan_move_to_duty` checks them and `studio::move_to_duty` makes it;
    /// until it is here, the Drivers tab takes up whole pieces, `TRIPS_TO_DUTIES`). Trips of
    /// several duties go together, each duty's as many rows on: `rows` the duties' rows as the
    /// page shows them (`driver_drag::targets`).
    DutyTrips { duty: usize, trips: Vec<(usize, usize)>, rows: Vec<usize> },
    None,
}

fn hand_of(args: &Value) -> Hand {
    let h = args.get("hand").cloned().unwrap_or(Value::Null);
    match h.get("kind").and_then(Value::as_str).unwrap_or("") {
        "trips" => Hand::Trips { tour: usize_arg(&h, "tour"), trips: usizes(&h, "trips") },
        "stack" => Hand::Stack(usize_arg(&h, "k")),
        "piece" => Hand::Piece { tour: usize_arg(&h, "tour"), duty: usize_arg(&h, "duty") },
        "duty_trips" => Hand::DutyTrips { duty: usize_arg(&h, "duty"), trips: pairs(&h, "trips"), rows: usizes(&h, "rows") },
        _ => Hand::None,
    }
}

/// Where what is in hand may go, each rated (`rf_gantt::zones_for`): the cost it makes against
/// the day's, blue where it breaks no rule more, the cheapest five stars.
fn zones_for(k: &Cached, board: &Board, h: &Hand) -> Vec<Value> {
    let c: &Company = &k.view;
    let wd = board.weekday;
    let tours = &board.tours;
    let base = board.figures.total() + board.figures.penalties;
    let hard_now = board.issues.iter().filter(|i| i.hard).count();
    let mut out: Vec<(Value, Cents, bool)> = Vec::new();
    let mut probe_with = |drop: Value, f: &dyn Fn(&mut Company) -> Result<(), &'static str>| {
        let mut probe = c.clone();
        if f(&mut probe).is_ok() {
            let b = rf::board(&probe, &k.lines, &k.date);
            let hard = b.issues.iter().filter(|i| i.hard).count();
            out.push((drop, b.figures.total() + b.figures.penalties - base, hard <= hard_now));
        }
    };
    match h {
        Hand::Trips { tour, trips } => {
            let Some(src) = tours.get(*tour) else { return Vec::new() };
            let line = src.line.clone();
            for (i, t) in tours.iter().enumerate() {
                if i == *tour || !t.line.eq_ignore_ascii_case(&line) || st::plan_move_many(c, wd, tours, *tour, trips, Target::Block(i)).is_err() {
                    continue;
                }
                probe_with(json!({ "kind": "block", "i": i }), &|p| st::move_trips(p, wd, tours, *tour, trips, Target::Block(i)));
            }
            probe_with(json!({ "kind": "new" }), &|p| st::move_trips(p, wd, tours, *tour, trips, Target::New));
            probe_with(json!({ "kind": "stack" }), &|p| st::unassign(p, wd, tours, *tour, trips));
        }
        Hand::Stack(sk) => {
            let Some((line, x)) = board.stack.get(*sk) else { return Vec::new() };
            for (i, t) in tours.iter().enumerate() {
                if !t.line.eq_ignore_ascii_case(line) || st::plan_assign(c, wd, tours, line, x, Target::Block(i)).is_err() {
                    continue;
                }
                probe_with(json!({ "kind": "block", "i": i }), &|p| st::assign(p, wd, tours, line, x, Target::Block(i)));
            }
            probe_with(json!({ "kind": "new" }), &|p| st::assign(p, wd, tours, line, x, Target::New));
        }
        Hand::Piece { tour, duty } => {
            let mine = st::duty_of(&board.duties, *tour, *duty);
            for (d, s) in board.shaped.iter().enumerate() {
                let Some(di) = s.duty else { continue };
                if Some(di) == mine {
                    continue;
                }
                let Some(first) = board.duties[di].pieces.first() else { continue };
                let a = (first.tour, first.duty);
                let mut test = c.clone();
                let _ = st::detach(&mut test, wd, tours, (*tour, *duty));
                if st::can_join(&test, wd, tours, a, (*tour, *duty)).is_err() {
                    continue;
                }
                probe_with(json!({ "kind": "duty", "d": d }), &|p| {
                    let _ = st::detach(p, wd, tours, (*tour, *duty));
                    st::join(p, wd, tours, a, (*tour, *duty))
                });
            }
            if mine.is_some_and(|m| board.duties[m].pieces.len() > 1) {
                probe_with(json!({ "kind": "new_duty" }), &|p| st::detach(p, wd, tours, (*tour, *duty)));
            }
        }
        // (Luc's driver drag: `driver_drag`)
        Hand::DutyTrips { duty, trips, rows } => {
            let again = |v: &Company| co::network::tours_of_day(v, &k.lines, &k.date);
            driver_drag::zones(c, board, *duty, trips, rows, &again, &mut probe_with)
        }
        Hand::None => {}
    }
    let stars = rate(&out.iter().map(|z| (z.1, z.2)).collect::<Vec<_>>());
    out.into_iter()
        .zip(stars)
        .map(|((drop, delta, clean), stars)| {
            let sign = if delta > 0 { "+ " } else if delta < 0 { "- " } else { "" };
            json!({ "drop": drop, "delta": delta, "delta_text": format!("{sign}{}", eur(delta.abs())), "clean": clean, "stars": stars })
        })
        .collect()
}

/// The stars (`rf_gantt::rate`): the cheapest five, the dearest one; a zone that breaks a rule
/// two at most.
fn rate(zones: &[(Cents, bool)]) -> Vec<u8> {
    let mut deltas: Vec<i64> = zones.iter().map(|z| z.0).collect();
    deltas.sort_unstable();
    deltas.dedup();
    let n = deltas.len().max(1);
    zones
        .iter()
        .map(|z| {
            let rank = deltas.iter().position(|d| *d == z.0).unwrap_or(0);
            let s = if n <= 1 { 5 } else { (5.0 - (rank as f32 * 4.0 / (n - 1) as f32).round()) as u8 };
            if z.1 {
                s
            } else {
                s.min(2)
            }
        })
        .collect()
}

/// A relief point in words: the depot's translated, a stop by its name.
fn point_name(key: &str) -> String {
    if key == rf::DEPOT_POINT {
        tr("Depot")
    } else {
        key.to_string()
    }
}

/// Where duty `s` signs on and off, in words: the point it (or the company) chose, else where its
/// first bus is taken over and its last left.
fn sign_places(board: &Board, s: &rf::Shaped) -> (String, String) {
    let bus_on = s.spans.first().and_then(|sp| piece_trips(board, sp.tour, sp.duty).first()).map(|x| x.from.trim().to_string()).unwrap_or_default();
    let bus_off = s.spans.last().and_then(|sp| piece_trips(board, sp.tour, sp.duty).last()).map(|x| x.end().trim().to_string()).unwrap_or_default();
    (s.points.on.as_deref().map(point_name).unwrap_or(bus_on), s.points.off.as_deref().map(point_name).unwrap_or(bus_off))
}

/// The relief points a duty may sign on and off at, and take a break at (`lines` the day's
/// timetable): each one's key and name.
fn duty_points(c: &Company, lines: &[LineInfo]) -> (Vec<Value>, Vec<Value>) {
    let rows = rf::relief_points(c, lines);
    let of = |f: fn(&rf::ReliefPoint) -> bool| rows.iter().filter(|r| f(&r.point)).map(|r| json!({ "key": r.stop, "name": point_name(&r.stop) })).collect::<Vec<_>>();
    (of(|p| p.sign), of(|p| p.breaks))
}

/// The duty panel (`rf_panels::duty_panel`): its type, times, paid time, signing and where, pieces,
/// the way between them and where their breaks are taken, its breaks, its custom work and its
/// issues.
fn duty_view(c: &Company, board: &Board, row: usize, lines: &[LineInfo]) -> Value {
    let Some(s) = board.shaped.get(row) else { return json!({ "gone": true }) };
    let wd = board.weekday;
    let prefs = &c.planning.flow.prefs;
    let note = s.key.as_ref().and_then(|key| c.planning.flow.sheet.notes.iter().find(|n| n.weekday == wd && n.key.is(key))).cloned();
    let spare = s.spare.and_then(|id| c.planning.flow.sheet.spares.iter().find(|x| x.weekday == wd && x.id == id)).cloned();
    let paid_set = note.as_ref().and_then(|n| n.paid).or(spare.as_ref().and_then(|x| x.paid));
    let mut names: Vec<String> = vec![tr("Automatic")];
    names.extend(prefs.duty_types.iter().map(|t| tr(&t.name)));
    names.push(tr(rf::OTHER));
    let at = if s.kind_set { s.kind.map(|k| k + 1).unwrap_or(names.len() - 1) } else { 0 };
    let (sign_points, break_points) = duty_points(c, lines);
    let places = sign_places(board, s);
    let times = [
        (tr("Sign-on"), rf::clock(s.start)),
        (tr("Sign-off"), rf::clock(s.end)),
        (tr("Spread"), rf::hm(s.spread())),
        (tr("Work time"), rf::hm(s.work)),
        (tr("Driving time"), rf::hm(s.driving)),
        (tr("Platform time"), rf::hm(s.platform)),
        (tr("Splits"), if s.splits.is_empty() { "-".into() } else { s.splits.iter().map(|x| format!("{}–{}", rf::clock(x.0), rf::clock(x.1))).collect::<Vec<_>>().join(", ") }),
        (tr("Changeovers"), s.changeovers.to_string()),
    ];
    let mut pieces: Vec<Value> = Vec::new();
    for (n, sp) in s.spans.iter().enumerate() {
        let t = &board.tours[sp.tour];
        let trips = piece_trips(board, sp.tour, sp.duty);
        let (a, b) = (trips.first().map(|x| x.from.clone()).unwrap_or_default(), trips.last().map(|x| x.end().to_string()).unwrap_or_default());
        let mut v = json!({
            "tour": sp.tour, "duty": sp.duty, "number": t.number.trim(), "colour": colour_of(c, &t.line),
            "words": format!("{}  {}–{}  {} → {}", block_label(t), rf::clock(sp.from), rf::clock(sp.to), a, b),
        });
        if let Some(next) = s.spans.get(n + 1) {
            let here_end = trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default();
            let next_start = piece_trips(board, next.tour, next.duty).first().map(|x| x.from.trim().to_string()).unwrap_or_default();
            // (where the break after it is taken: a relief point chosen, else where the bus is)
            v["break_at"] = json!(note.as_ref().and_then(|nn| nn.breaks_at.iter().find(|x| x.0 as usize == n).map(|x| x.1.clone())));
            if !here_end.eq_ignore_ascii_case(&next_start) {
                // (as the relief points have it: the pair, the points' own, the defaults)
                let set = note.as_ref().and_then(|nn| nn.travel.iter().find(|x| x.0 as usize == n).map(|x| x.1));
                let mode = rf::travel_between(c, &board.tours, &here_end, &next_start, set).0;
                v["travel"] = json!(Travel::ALL.iter().position(|x| *x == mode).unwrap_or(0));
            } else {
                let gap = next.from - sp.to;
                v["between"] = json!(if gap >= prefs.split_min { trf("Split of %{t}", &[("t", &rf::hm(gap))]) } else { trf("Break of %{t}", &[("t", &rf::hm(gap))]) });
            }
        }
        pieces.push(v);
    }
    json!({
        "label": s.label(row), "id": duty_id(s), "kind": tr(s.kind_name(prefs)), "locked": s.locked, "spare": s.spare.is_some(),
        // (a spare duty: its name, where its driver stands by - null as the company signs on,
        // else the depot - and that in words)
        "name": spare.as_ref().map(|x| x.name.clone()).unwrap_or_default(),
        "spare_at": spare.as_ref().and_then(|x| x.sign_on_at.clone()),
        "spare_company": point_name(prefs.sign_on_at.as_deref().filter(|x| !x.trim().is_empty()).unwrap_or(rf::DEPOT_POINT)),
        "types": names, "type_at": at,
        "times": times.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "paid": s.paid, "paid_text": rf::hm(s.paid), "paid_set": paid_set.is_some(),
        "signing": s.key.is_some(),
        "sign_on": note.as_ref().and_then(|n| n.sign_on).unwrap_or(prefs.sign_on),
        "sign_off": note.as_ref().and_then(|n| n.sign_off).unwrap_or(prefs.sign_off),
        // (where: the duty's choice - null as the company has it, empty where the bus is - the
        // company's, and the relief points that allow it)
        "sign_on_at": note.as_ref().and_then(|n| n.sign_on_at.clone()),
        "sign_off_at": note.as_ref().and_then(|n| n.sign_off_at.clone()),
        "sign_company": { "on": prefs.sign_on_at.as_deref().map(point_name), "off": prefs.sign_off_at.as_deref().map(point_name) },
        "sign_places": { "on": places.0, "off": places.1 },
        "sign_points": sign_points,
        "break_points": break_points,
        "pieces": pieces,
        "travel_labels": Travel::LABELS.iter().map(|x| tr(x)).collect::<Vec<_>>(),
        "rest": breaks::panel_json(prefs, s),
        "custom": s.custom.iter().map(|cw| format!("{}  {}–{}", tr(&cw.kind), rf::clock(cw.from), rf::clock(cw.to))).collect::<Vec<_>>(),
        "kinds": rf::CUSTOM_KINDS.iter().map(|x| tr(x)).collect::<Vec<_>>(),
        "issues": s.issues.iter().map(|&i| json!({ "text": issue_words(&board.issues[i]), "hard": board.issues[i].hard })).collect::<Vec<_>>(),
    })
}

/// The Preferences window's start: the preferences as kept, what the costs are as the company
/// pays them, and the relief points of the day `date`'s timetable (none while it is read).
fn prefs_view(s: &mut Session, r: &mut Rf, date: &str) -> Value {
    let lines = match ensure(s, r, date) {
        Some(Ok(k)) => (*k.lines).clone(),
        _ => Vec::new(),
    };
    let Some(c) = s.company.as_ref() else { return json!({ "none": true }) };
    let (hour, fixed, day, per_km) = rf::rates(c);
    json!({
        "relief_points": relief::prefs_points(c, &lines),
        "prefs": c.planning.flow.prefs,
        "defaults": rf::Prefs::default(),
        "rates": { "wage": hour.round() as Cents, "driver_fixed": fixed, "vehicle_fixed": day, "per_km": per_km.round() as Cents },
        "relief_labels": rf::ReliefTiming::LABELS.iter().map(|x| tr(x)).collect::<Vec<_>>(),
        "travel_labels": Travel::LABELS.iter().map(|x| tr(x)).collect::<Vec<_>>(),
        "tri_labels": rf::Tri::LABELS.iter().map(|x| tr(x)).collect::<Vec<_>>(),
        "type_names": c.planning.flow.prefs.duty_types.iter().map(|t| tr(&t.name)).collect::<Vec<_>>(),
        // (the least a duty is paid always: Time Limitations' least length of a duty says so)
        "min_paid": co::staff::MIN_PAID,
    })
}

// --- edits ----------------------------------------------------------------------------------------

/// What an action says back: the status bar's words (`say`, an error or not) and what else.
fn said(text: impl Into<String>, error: bool) -> Value {
    json!({ "say": { "text": text.into(), "error": error } })
}

/// An edit of the draft (`f` on the company as the studio sees it): what was before kept for
/// Undo and for the figures' differences; a refusal said with its reason (`rosterflow::edit`).
fn edit(s: &mut Session, r: &mut Rf, label: String, f: impl FnOnce(&mut Company) -> Result<(), &'static str>) -> Value {
    edit_with(s, r, label, |c| f(c).map_err(tr))
}

/// `edit` taking trips `moved` off their duty (`Rows::Keep`).
pub(crate) fn edit_moving(s: &mut Session, r: &mut Rf, label: String, moved: &[st::TripRef], f: impl FnOnce(&mut Company) -> Result<(), &'static str>) -> Value {
    edit_rows(s, r, label, Rows::Keep(moved), |c| f(c).map_err(tr))
}

/// `edit_with` taking trips `moved` off their duty (`Rows::Keep`).
pub(crate) fn edit_with_moving(s: &mut Session, r: &mut Rf, label: String, moved: &[st::TripRef], f: impl FnOnce(&mut Company) -> Result<(), String>) -> Value {
    edit_rows(s, r, label, Rows::Keep(moved), f)
}

/// The trips with passengers among `trips` (block, trip) of `tours`, as the rows know them.
pub(crate) fn refs_of(tours: &[TourOfDay], trips: impl IntoIterator<Item = (usize, usize)>) -> Vec<st::TripRef> {
    trips.into_iter().filter_map(|(i, j)| tours.get(i)?.trips.get(j)).filter(|x| x.counts()).map(st::TripRef::of).collect()
}

/// The trips of piece `duty` of block `tour` from its trip `from` on (`refs_of`).
fn piece_rest(tours: &[TourOfDay], tour: usize, duty: usize, from: usize) -> Vec<st::TripRef> {
    let end = tours.get(tour).and_then(|t| co::network::duties_of(t).get(duty).map(|r| r.end)).unwrap_or(0);
    refs_of(tours, (from..end).map(|j| (tour, j)))
}

/// `edit` whose refusal is said in words already translated.
fn edit_with(s: &mut Session, r: &mut Rf, label: String, f: impl FnOnce(&mut Company) -> Result<(), String>) -> Value {
    edit_rows(s, r, label, Rows::Keep(&[]), f)
}

/// What an edit does to the Drivers tab's rows (`rf::RowOrder`).
enum Rows<'a> {
    /// Every duty keeps its row, a new one gets a row at the bottom (`rf::keep_rows`): the trips
    /// the edit takes off their duty (the half a split cuts off, the trips that become a duty of
    /// their own) make no duty the one it was.
    Keep(&'a [st::TripRef]),
    /// The duties made anew (Optimise's): the locked ones keep their rows (`rf::pin_rows`).
    Anew,
}

/// `edit_with`, the Drivers tab's rows kept as `rows` says, on the day shown (the board the edit
/// is made on: `ensure`'s).
fn edit_rows(s: &mut Session, r: &mut Rf, label: String, rows: Rows, f: impl FnOnce(&mut Company) -> Result<(), String>) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let mut v = rf::view(c);
    let snapshot = rf::draft_of(c);
    let before = r.cache.as_ref().map(|k| (k.date.clone(), k.board.figures.clone()));
    match f(&mut v) {
        Ok(()) => {
            if let Some(k) = r.cache.as_ref().filter(|k| k.generation == rf_generation(s)) {
                let tours = co::network::tours_of_day(&v, &k.lines, &k.date);
                let was = rf::row_duties(&k.board);
                match rows {
                    Rows::Keep(moved) => rf::keep_rows(&mut v, k.board.weekday, &tours, &was, moved),
                    Rows::Anew => rf::pin_rows(&mut v, k.board.weekday, &tours, &was),
                }
            }
            let _ = company::act(s, |c| {
                rf::keep_draft(c, &v);
                Ok(())
            });
            r.edits += 1;
            r.undo.push((snapshot, label));
            if r.undo.len() > 60 {
                r.undo.remove(0);
            }
            r.redo.clear();
            r.before = before;
            json!({ "edited": true })
        }
        Err(why) => said(why, true),
    }
}

/// The company's preferences changed (from `was`; `before` what broke each rule then): the day
/// made again with them at once - its issues, figures, breaks and driving periods - and an
/// Optimise proposal made to the old rules dropped (it stayed while the day's own board was the
/// same, and Accept then kept a plan cut to the old rules). What the changed rules break now is
/// said, with Optimise offered to work to them.
fn rules_applied(s: &mut Session, r: &mut Rf, date: &str, was: &rf::Prefs, before: Option<&[(&'static str, usize)]>) -> Value {
    r.cache = None;
    r.proposal = None;
    let mut out = said(tr("Preferences saved: the issues and Optimise follow them."), false);
    if let Some(Ok(k)) = ensure(s, r, date) {
        let offer = breaks::offer(was, &k.view.planning.flow.prefs, before, &k.board);
        if let Some(text) = offer.get("text").and_then(Value::as_str) {
            out["say"]["text"] = json!(text);
        }
        out["offer"] = offer;
    }
    out
}

/// Optimise the day shown (`k`): its proposal over the Gantt - where nothing better was found the
/// day as it is, with nothing to accept - and what it did said in words, or why it could not
/// (testers, 2026-10-15: "optimise feature do not work in rosterflow": a day it found nothing
/// better for came back unchanged and unsaid, and Accept kept it as it was).
fn optimise(r: &mut Rf, k: &Cached) -> Value {
    let p = &k.view.planning.flow.prefs;
    if !p.optimise_vehicles && !p.optimise_duties {
        return said(tr("Optimise makes neither the vehicles nor the duties now: switch one on in the Optimisation settings (the gear beside OPTIMIZE)."), true);
    }
    if k.board.tours.is_empty() && k.board.stack.is_empty() {
        return said(tr("Nothing to optimise: no trips run on this day."), true);
    }
    let mut o = rf::optimise_day(&k.view, &k.lines, &k.date);
    // (a duty's own sign-on, sign-off and break points stay its own)
    rf::signing::keep_points(&k.view, &mut o.view, &k.lines, &k.date);
    o.board = rf::board(&o.view, &k.lines, &k.date);
    let note = outcome_words(&k.board, &o);
    r.proposal = Some(Proposal { date: k.date.clone(), generation: k.generation, base: k.rev.clone(), better: o.better, note: note.clone(), board: Arc::new(o.board), view: Arc::new(o.view), layouts: o.layouts, joined: o.joined });
    let mut out = said(note, false);
    out["proposal"] = json!(true);
    out
}

/// The hard rules day `b` (of company `c`) breaks that are Optimise's to keep: not the day's roster
/// (an ill driver, a bus in the workshop) nor a line without a timetable; the dismissed left out.
fn unmet<'a>(c: &Company, b: &'a Board) -> Vec<&'a rf::Issue> {
    b.issues.iter().filter(|i| i.hard && rf::of_the_plan(i) && !rf::dismissed(c, b.weekday, &i.key)).collect()
}

/// Issues by their titles, each with how many: "Duty type unsatisfied (2), No meal break (1)".
fn grouped(list: &[&rf::Issue]) -> String {
    let mut groups: Vec<(&'static str, usize)> = Vec::new();
    for i in list {
        let t = rf::issue_title(i.text);
        match groups.iter_mut().find(|g| g.0 == t) {
            Some(g) => g.1 += 1,
            None => groups.push((t, 1)),
        }
    }
    groups.iter().map(|(t, n)| format!("{} ({n})", tr(t))).collect::<Vec<_>>().join(", ")
}

/// What Optimise could not keep on day `b` (of company `c`), in words: the duties that still break
/// a hard rule and which, the vehicles drivers may change on nowhere, and the rest it breaks.
fn unmet_words(c: &Company, b: &Board) -> Vec<String> {
    let open = unmet(c, b);
    let mut out = Vec::new();
    let duty: Vec<&rf::Issue> = open.iter().filter(|i| i.scope == Scope::Duty).copied().collect();
    if !duty.is_empty() {
        let mut rows: Vec<usize> = duty.iter().filter_map(|i| i.row).collect();
        rows.sort_unstable();
        rows.dedup();
        out.push(trf("%{n} duties can't keep every hard rule, because of: %{why}.", &[("n", &rows.len().to_string()), ("why", &grouped(&duty))]));
    }
    let n = rf::uncuttable(c, b);
    if n > 0 {
        out.push(trf("%{n} vehicles have no relief point where drivers may change (Preferences › Relief Points).", &[("n", &n.to_string())]));
    }
    let rest: Vec<&rf::Issue> = open.iter().filter(|i| i.scope != Scope::Duty).copied().collect();
    if !rest.is_empty() {
        out.push(trf("Also broken: %{why}.", &[("why", &grouped(&rest))]));
    }
    out
}

/// What Optimise did (`o`, against the day as it was, `was`), in words.
fn outcome_words(was: &Board, o: &rf::Outcome) -> String {
    let (f, w) = (&o.board.figures, &was.figures);
    let mut out = vec![if o.better {
        trf("Optimised: %{duties} duties on %{blocks} vehicles, %{cost} a day (your planning: %{was}).", &[("duties", &f.duties.to_string()), ("blocks", &f.blocks.to_string()), ("cost", &eur(f.total())), ("was", &eur(w.total()))])
    } else {
        tr("No better plan found: your planning already costs the least by the preferences.")
    }];
    let (now, before) = (unmet(&o.view, &o.board).len(), unmet(&o.view, was).len());
    if o.better && now < before {
        out.push(trf("Hard rules broken: %{n}, against %{m} in your planning.", &[("n", &now.to_string()), ("m", &before.to_string())]));
    }
    out.extend(unmet_words(&o.view, &o.board));
    out.join(" ")
}

/// Undo the last edit (`back`), or redo the last undone (`rosterflow::undo`).
fn undo(s: &mut Session, r: &mut Rf, back: bool) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let now = rf::draft_of(c);
    let item = if back { r.undo.pop() } else { r.redo.pop() };
    let Some((d, label)) = item else { return json!({}) };
    if back {
        r.redo.push((now, label.clone()));
    } else {
        r.undo.push((now, label.clone()));
    }
    let _ = company::act(s, |c| {
        rf::set_draft(c, d);
        Ok(())
    });
    let words = if back { tr("Undone: %{edit}") } else { tr("Redone: %{edit}") };
    said(words.replace("%{edit}", &label), false)
}

/// Lock or unlock a bus's block (`Some(tour)`) or a duty (`row`), as `on` says (None: the other
/// way than it is).
fn lock_words(board: &Board, kind: &str, i: usize, on: bool) -> String {
    match kind {
        "block" => {
            let label = board.tours.get(i).map(block_label).unwrap_or_default();
            if on { tr("Vehicle %{v} locked") } else { tr("Vehicle %{v} unlocked") }.replace("%{v}", &label)
        }
        _ => {
            let label = board.shaped.get(i).map(|s| s.label(i)).unwrap_or_default();
            if on { tr("Duty %{d} locked") } else { tr("Duty %{d} unlocked") }.replace("%{d}", &label)
        }
    }
}

/// Set the lock of duty row `d` of `board` on `c`.
fn set_duty_lock(c: &mut Company, board: &Board, d: usize, on: bool) {
    let wd = board.weekday;
    let Some(s) = board.shaped.get(d) else { return };
    match (&s.key, s.spare) {
        (Some(key), _) => rf::set_duty_lock(c, wd, key, on),
        (None, Some(id)) => {
            if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                sp.locked = on;
            }
        }
        _ => {}
    }
}

/// Change the note of duty row `row` by `f` (a spare duty's own fields for one of custom work
/// only: `rf_panels::duty_panel`'s `set_note`).
fn set_note(c: &mut Company, board: &Board, row: usize, f: &dyn Fn(&mut rf::DutyNote)) {
    let wd = board.weekday;
    let Some(s) = board.shaped.get(row) else { return };
    match (&s.key, s.spare) {
        (Some(key), _) => {
            f(rf::note_mut(c, wd, key));
            rf::tidy_notes(c);
        }
        (None, Some(id)) => {
            if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                let mut n = rf::DutyNote { kind: sp.kind.clone(), paid: sp.paid, custom: sp.custom.clone(), locked: sp.locked, sign_on_at: sp.sign_on_at.clone(), ..Default::default() };
                f(&mut n);
                sp.kind = n.kind;
                sp.paid = n.paid;
                sp.custom = n.custom;
                sp.locked = n.locked;
                // (where its driver stands by: `rf::spare_lines`)
                sp.sign_on_at = n.sign_on_at;
            }
        }
        _ => {}
    }
}

/// Delete custom work `ci` of duty row `row` (a spare duty without any left goes).
fn delete_custom(c: &mut Company, board: &Board, row: usize, ci: usize) {
    let wd = board.weekday;
    let Some(s) = board.shaped.get(row) else { return };
    match (&s.key, s.spare) {
        (Some(key), _) => {
            let n = rf::note_mut(c, wd, key);
            if ci < n.custom.len() {
                n.custom.remove(ci);
            }
            rf::tidy_notes(c);
        }
        (None, Some(id)) => {
            if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                if ci < sp.custom.len() {
                    sp.custom.remove(ci);
                }
            }
            c.planning.flow.sheet.spares.retain(|x| !(x.weekday == wd && x.custom.is_empty()));
        }
        _ => {}
    }
}

/// The trips with passengers of the buses `ids` (`block_id`) unassigned to the stack (the batch
/// toolbar's): one bus at a time, each on the day as it then is (`again`: the day's blocks of the
/// company as it is) - a layout of the day before the first bus went keeps that bus's trips on it
/// and on the stack at once, and its line no longer fits it. A bus that may not change stays.
fn unassign_blocks(c: &mut Company, wd: u8, ids: &[String], again: &dyn Fn(&Company) -> Vec<TourOfDay>) {
    for id in ids {
        let tours = again(c);
        let Some(i) = tours.iter().position(|t| block_id(t) == *id) else { continue };
        let picked: Vec<usize> = (0..tours[i].trips.len()).filter(|&j| tours[i].trips[j].counts() && !tours[i].trips[j].origin.trim().is_empty()).collect();
        let mut lay = c.clone();
        if st::unassign(&mut lay, wd, &tours, i, &picked).is_ok() {
            *c = lay;
        }
    }
}

/// The trips `want` (whole trips, `studio::whole_ref`) taken off their buses onto the stack, one
/// bus at a time, each on the day as it then is (`again`, as `unassign_blocks`); a bus that may
/// not change keeps them. Returns how many are still on a bus.
fn unassign_trips(c: &mut Company, wd: u8, want: &[st::TripRef], again: &dyn Fn(&Company) -> Vec<TourOfDay>) -> usize {
    let wanted = |x: &PlannedTrip| x.counts() && want.contains(&st::whole_ref(x));
    let mut refused: Vec<String> = Vec::new();
    for _ in 0..1000 {
        let tours = again(c);
        let next = tours.iter().enumerate().filter(|(_, t)| !refused.contains(&block_id(t))).find_map(|(i, t)| {
            let picked: Vec<usize> = (0..t.trips.len()).filter(|&j| wanted(&t.trips[j])).collect();
            (!picked.is_empty()).then_some((i, picked))
        });
        let Some((i, picked)) = next else { break };
        let mut lay = c.clone();
        match st::unassign(&mut lay, wd, &tours, i, &picked) {
            Ok(()) => *c = lay,
            Err(_) => refused.push(block_id(&tours[i])),
        }
    }
    let mut left: Vec<st::TripRef> = Vec::new();
    for x in again(c).iter().flat_map(|t| t.trips.iter()).filter(|x| wanted(x)) {
        let w = st::whole_ref(x);
        if !left.contains(&w) {
            left.push(w);
        }
    }
    left.len()
}

/// What a click, a drop or a menu's line does (`rf_act`: `act` and what it needs, `day` the day).
fn act(s: &mut Session, r: &mut Rf, by: live::Editor, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    let Some(date) = date_of(s, args) else { return Ok(json!({})) };
    // what needs no board
    match what.as_str() {
        "undo" => return Ok(undo(s, r, true)),
        "redo" => return Ok(undo(s, r, false)),
        "back_to" => {
            let k = usize_arg(args, "k");
            let n = r.undo.len();
            let mut last = json!({});
            for _ in k..n {
                last = undo(s, r, true);
            }
            return Ok(last);
        }
        // (SAVE, and Apply to the other days: `days`)
        "save" => return Ok(days::save(s, r, &date, args.get("apply"))),
        "day_reset" => return Ok(days::reset(s, r, &date)),
        "day_unfollow" => return Ok(days::unfollow(s, r, &date)),
        // (the selection bar's "Copy to…", Ctrl+C and Ctrl+V: `days`)
        "copy_rows" => return Ok(days::copy_rows(s, r, &date, args)),
        "discard" => {
            // (what the draft locks stays as the draft has it: `rf::discard_draft`)
            return Ok(match company::act(s, |c| Ok(rf::discard_draft(c).then(|| rf::has_draft(c)))) {
                Ok(Some(kept)) => {
                    r.undo.clear();
                    r.redo.clear();
                    let words = if kept { tr("Discarded: back to the saved planning - the locked vehicles and duties stay as they are.") } else { tr("Discarded: back to the saved planning.") };
                    said(words, false)
                }
                _ => json!({}),
            });
        }
        "revalidate" => {
            r.cache = None;
            return Ok(said(tr("Validated again."), false));
        }
        "keep_mine" => {
            r.proposal = None;
            return Ok(json!({}));
        }
        "prefs" => {
            let mut prefs: rf::Prefs = arg(args, "prefs")?;
            // (a relief point's settings that say nothing are not kept)
            rf::tidy_points(&mut prefs);
            let was = s.company.as_ref().map(|c| c.planning.flow.prefs.clone()).unwrap_or_default();
            let before = r.cache.as_ref().filter(|k| k.date == date).map(|k| breaks::rule_counts(&k.board));
            return Ok(match company::act(s, |c| {
                c.planning.flow.prefs = prefs;
                Ok(())
            }) {
                Ok(()) => rules_applied(s, r, &date, &was, before.as_deref()),
                Err(_) => json!({}),
            });
        }
        "opt_flags" => {
            let (veh, dut, restore) = (args.get("vehicles").and_then(Value::as_bool), args.get("duties").and_then(Value::as_bool), args.get("restore").and_then(Value::as_bool).unwrap_or(false));
            let _ = company::act(s, |c| {
                let f = &mut c.planning.flow.prefs;
                if let Some(v) = veh {
                    f.optimise_vehicles = v;
                }
                if let Some(d) = dut {
                    f.optimise_duties = d;
                }
                if restore {
                    f.optimise_vehicles = true;
                    f.optimise_duties = true;
                }
                Ok(())
            });
            return Ok(json!({}));
        }
        "colour_by" => {
            // (Layers › Colour by: kept for the company, not in the draft - no edit of the plan)
            let by = str_arg(args, "by");
            let by = if COLOUR_BY.contains(&by.as_str()) { by } else { String::new() };
            let _ = company::act(s, |c| {
                c.planning.flow.colour_by = by;
                Ok(())
            });
            return Ok(json!({}));
        }
        "colour" => {
            // (the line's colour is the company's, not the draft's: plates and the map take it)
            let line = str_arg(args, "line");
            let colour = args.get("colour").and_then(Value::as_str).and_then(rf::parse_hex);
            let _ = company::act(s, |c| Ok(rf::set_line_colour(c, &line, colour)));
            r.cache = None;
            return Ok(json!({}));
        }
        _ => {}
    }
    let k = match ensure(s, r, &date) {
        Some(Ok(k)) => k,
        Some(Err(e)) => return Ok(said(e, true)),
        None => return Ok(said(tr("Reading the timetable…"), false)),
    };
    // (an edit of the day as the page showed it, which somebody else changed meanwhile: not made
    // on what the page's numbers point to now - the day shown anew, said by whom)
    let shown = str_arg(args, "rev");
    let was = shown.split('|').next().unwrap_or("");
    if let Some(who) = s.company.as_ref().filter(|_| what != "optimise").and_then(|c| live::stale(c, r, by, was, &k.rev)) {
        let mut out = said(trf("%{who} changed this day meanwhile: it is shown anew - do it once more.", &[("who", &who)]), true);
        out["stale"] = json!(true);
        return Ok(out);
    }
    let board = k.board.clone();
    let tours = board.tours.clone();
    let wd = board.weekday;
    let tour = usize_arg(args, "tour");
    let trips = usizes(args, "trips");
    let label_of = |i: usize| tours.get(i).map(block_label).unwrap_or_default();
    Ok(match what.as_str() {
        "optimise" => optimise(r, &k),
        "accept" => match r.proposal.take() {
            // (nothing better was found: the day as it is, nothing to accept)
            Some(pr) if pr.better => {
                let (lines, date) = (k.lines.clone(), k.date.clone());
                // (the locked duties keep their rows and numbers: `Rows::Anew`)
                let out = edit_rows(s, r, tr("Optimised"), Rows::Anew, |c| {
                    let before = c.clone();
                    rf::apply(c, wd, pr.layouts, pr.joined);
                    rf::signing::keep_points(&before, c, &lines, &date);
                    Ok(())
                });
                if out.get("edited").is_some() {
                    said(tr("The optimised blocks and duties are in your draft: SAVE keeps them."), false)
                } else {
                    out
                }
            }
            _ => json!({}),
        },
        "move" => {
            let to = args.get("to").cloned().unwrap_or(Value::Null);
            let target = match to.as_u64() {
                Some(i) => Target::Block(i as usize),
                None => Target::New,
            };
            let words = match target {
                Target::Block(i) => trf("%{n} trips moved from %{a} to %{b}", &[("n", &trips.len().to_string()), ("a", &label_of(tour)), ("b", &label_of(i))]),
                Target::New => trf("%{n} trips moved from %{a} to a new vehicle", &[("n", &trips.len().to_string()), ("a", &label_of(tour))]),
            };
            // (to a new vehicle: their duty a new row at the bottom of the Drivers tab)
            let moved = if target == Target::New { refs_of(&tours, trips.iter().map(|&j| (tour, j))) } else { Vec::new() };
            edit_moving(s, r, words, &moved, |c| st::move_trips(c, wd, &tours, tour, &trips, target))
        }
        "unassign" => {
            let words = trf("%{n} trips of %{a} unassigned to the stack", &[("n", &trips.len().to_string()), ("a", &label_of(tour))]);
            edit(s, r, words, |c| st::unassign(c, wd, &tours, tour, &trips))
        }
        "unassign_key" => {
            // (Delete: the chosen trips, said as the Rust launcher says it)
            edit(s, r, tr("Trips unassigned to the stack"), |c| st::unassign(c, wd, &tours, tour, &trips))
        }
        "assign" => {
            let sk = usize_arg(args, "k");
            let Some((line, x)) = board.stack.get(sk).cloned() else { return Ok(json!({})) };
            let to = match args.get("to").and_then(Value::as_u64) {
                Some(i) => Target::Block(i as usize),
                None => Target::New,
            };
            let words = trf("Trip of %{time} assigned from the stack", &[("time", &rf::clock(x.dep))]);
            edit(s, r, words, |c| st::assign(c, wd, &tours, &line, &x, to))
        }
        "swap" => {
            let to = usize_arg(args, "to");
            let how = if str_arg(args, "how") == "from_here" { st::Swap::FromHere } else { st::Swap::Span };
            let words = trf("Trips swapped between %{a} and %{b}", &[("a", &label_of(tour)), ("b", &label_of(to))]);
            edit(s, r, words, |c| st::swap(c, wd, &tours, tour, &trips, to, how))
        }
        "cut" | "uncut" => {
            let at = usize_arg(args, "at");
            let time = tours.get(tour).and_then(|t| t.trips.get(at)).map(|x| rf::clock(x.dep)).unwrap_or_default();
            if what == "cut" {
                let words = trf("Duty cut on %{v} at %{time}", &[("v", &label_of(tour)), ("time", &time)]);
                // (the first half keeps the duty's row, the second half gets a new row at the bottom)
                let piece = tours.get(tour).and_then(|t| co::network::duties_of(t).iter().position(|p| p.contains(&at))).unwrap_or(0);
                edit_moving(s, r, words, &piece_rest(&tours, tour, piece, at), |c| st::cut(c, wd, &tours, tour, at))
            } else {
                let words = trf("Duties joined on %{v} at %{time}", &[("v", &label_of(tour)), ("time", &time)]);
                edit(s, r, words, |c| st::uncut(c, wd, &tours, tour, at))
            }
        }
        "cut_mid" => {
            // (a duty cut inside a trip, at a relief point on the way: `studio::cut_mid`)
            let (trip, stop) = (usize_arg(args, "trip"), usize_arg(args, "stop"));
            let name = str_arg(args, "name");
            let time = args.get("time").and_then(Value::as_i64).map(|m| rf::clock(m as i32)).unwrap_or_default();
            let words = trf("Duty cut on %{v} on the way, at %{stop} (%{time})", &[("v", &label_of(tour)), ("stop", &name), ("time", &time)]);
            let lines = k.lines.clone();
            let piece = tours.get(tour).and_then(|t| co::network::duties_of(t).iter().position(|p| p.contains(&trip))).unwrap_or(0);
            edit_moving(s, r, words, &piece_rest(&tours, tour, piece, trip + 1), |c| st::cut_mid(c, wd, &tours, &lines, tour, trip, stop))
        }
        "join_piece" => {
            let (duty, d) = (usize_arg(args, "duty"), usize_arg(args, "into"));
            let Some(first) = board.shaped.get(d).and_then(|x| x.duty).and_then(|di| board.duties[di].pieces.first().copied()) else { return Ok(json!({})) };
            let words = trf("Piece of %{v} joined into duty %{d}", &[("v", &label_of(tour)), ("d", &board.shaped[d].label(d))]);
            edit(s, r, words, |c| {
                let _ = st::detach(c, wd, &tours, (tour, duty));
                st::join(c, wd, &tours, (first.tour, first.duty), (tour, duty))
            })
        }
        "detach" => {
            let duty = usize_arg(args, "duty");
            let words = trf("Piece of %{v} made a duty of its own", &[("v", &label_of(tour))]);
            edit_moving(s, r, words, &piece_rest(&tours, tour, duty, 0), |c| st::detach(c, wd, &tours, (tour, duty)))
        }
        "link" => {
            let words = trf("%{n} trips linked on %{v}", &[("n", &trips.len().to_string()), ("v", &label_of(tour))]);
            edit(s, r, words, |c| rf::link(c, wd, &tours, tour, &trips))
        }
        "unlink" => {
            let trip = usize_arg(args, "trip");
            let words = trf("Link on %{v} taken apart", &[("v", &label_of(tour))]);
            edit(s, r, words, |c| {
                rf::unlink(c, wd, &tours, tour, trip);
                Ok(())
            })
        }
        "lock_block" => {
            let Some(t) = tours.get(tour) else { return Ok(json!({})) };
            let on = !rf::block_locked(&k.view, wd, &t.line, &t.tour);
            let (line, name) = (t.line.clone(), t.tour.clone());
            edit(s, r, lock_words(&board, "block", tour, on), |c| {
                rf::set_block_lock(c, wd, &line, &name, on);
                Ok(())
            })
        }
        "lock_duty" => {
            let row = usize_arg(args, "row");
            let Some(x) = board.shaped.get(row) else { return Ok(json!({})) };
            let on = !x.locked;
            let b = board.clone();
            edit(s, r, lock_words(&board, "duty", row, on), |c| {
                set_duty_lock(c, &b, row, on);
                Ok(())
            })
        }
        "lock_rows" => {
            // (the batch toolbar: the rows ticked, by their ids)
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            let ids: Vec<String> = arg::<Vec<String>>(args, "ids").unwrap_or_default();
            let words = if on { tr("%{n} rows locked") } else { tr("%{n} rows unlocked") }.replace("%{n}", &ids.len().to_string());
            let b = board.clone();
            edit(s, r, words, |c| {
                for id in &ids {
                    if let Some(t) = b.tours.iter().find(|t| block_id(t) == *id) {
                        rf::set_block_lock(c, wd, &t.line, &t.tour, on);
                    } else if let Some(d) = b.shaped.iter().position(|x| duty_id(x) == *id) {
                        set_duty_lock(c, &b, d, on);
                    }
                }
                Ok(())
            })
        }
        "unassign_rows" => {
            let ids: Vec<String> = arg::<Vec<String>>(args, "ids").unwrap_or_default();
            let words = trf("%{n} vehicles unassigned to the stack", &[("n", &ids.len().to_string())]);
            let (lines, day) = (k.lines.clone(), k.date.clone());
            edit(s, r, words, |c| {
                unassign_blocks(c, wd, &ids, &|v: &Company| co::network::tours_of_day(v, &lines, &day));
                Ok(())
            })
        }
        "unassign_duties" => {
            // (the Drivers tab's batch toolbar, "Unassign all trips": every trip of the duties
            // ticked to the stack, to plan them anew - Luc, Beta 65; asked first on the page)
            let ids: Vec<String> = arg::<Vec<String>>(args, "ids").unwrap_or_default();
            let mut want: Vec<st::TripRef> = Vec::new();
            for x in board.shaped.iter().filter(|x| ids.contains(&duty_id(x))) {
                for sp in &x.spans {
                    want.extend(piece_trips(&board, sp.tour, sp.duty).iter().filter(|t| t.counts() && !t.origin.trim().is_empty()).map(st::whole_ref));
                }
            }
            let mut seen: Vec<st::TripRef> = Vec::new();
            want.retain(|x| !seen.contains(x) && {
                seen.push(x.clone());
                true
            });
            if want.is_empty() {
                return Ok(said(tr("The duties ticked run no trips."), true));
            }
            let n = ids.len().to_string();
            let words = trf("Trips of %{n} duties unassigned to the stack", &[("n", &n)]);
            let (lines, day) = (k.lines.clone(), k.date.clone());
            let mut left = 0;
            let out = edit_with(s, r, words, |c| {
                left = unassign_trips(c, wd, &want, &|v: &Company| co::network::tours_of_day(v, &lines, &day));
                if left == want.len() {
                    return Err(tr("None of the duties ticked could be unassigned: their vehicles or duties are locked."));
                }
                Ok(())
            });
            if out.get("edited").is_none() {
                return Ok(out);
            }
            let text = if left > 0 {
                trf("Trips of %{n} duties unassigned to the stack; %{k} trips stay, their vehicle or duty is locked. Undo (Ctrl+Z) puts them back.", &[("n", &n), ("k", &left.to_string())])
            } else {
                trf("Trips of %{n} duties unassigned to the stack: Undo (Ctrl+Z) puts them back.", &[("n", &n)])
            };
            let mut out = said(text, left > 0);
            out["edited"] = json!(true);
            out
        }
        "spare" => {
            // (a click on the new row: 13:30-21:30; "Add spare line" on a spare line's card: its
            // times - a day has as many as the player adds)
            let at = |name: &str, or: i32| args.get(name).and_then(Value::as_i64).map_or(or, |m| m as i32);
            let mut cw = rf::Custom { kind: rf::CUSTOM_KINDS[0].into(), ..Default::default() };
            cw.set_times(at("from", 810), at("to", 1290));
            edit(s, r, tr("Spare duty added"), |c| {
                rf::add_spare(c, wd, &cw.kind, cw.from, cw.to);
                Ok(())
            })
        }
        "spare_name" => {
            let row = usize_arg(args, "row");
            let Some(id) = board.shaped.get(row).and_then(|x| x.spare) else { return Ok(json!({})) };
            let name: String = str_arg(args, "value").trim().chars().take(40).collect();
            let words = trf("Spare duty %{d} named %{name}", &[("d", &board.shaped[row].label(row)), ("name", &name)]);
            edit(s, r, words, |c| {
                if let Some(sp) = rf::spare_mut(c, wd, id) {
                    sp.name = name.clone();
                }
                Ok(())
            })
        }
        "spare_remove" => {
            let row = usize_arg(args, "row");
            let Some(id) = board.shaped.get(row).and_then(|x| x.spare) else { return Ok(json!({})) };
            let words = trf("Spare duty %{d} deleted", &[("d", &board.shaped[row].label(row))]);
            edit(s, r, words, |c| {
                rf::remove_spare(c, wd, id);
                Ok(())
            })
        }
        "custom_delete" => {
            let (row, ci) = (usize_arg(args, "row"), usize_arg(args, "ci"));
            let words = trf("Custom work deleted from duty %{d}", &[("d", &board.shaped.get(row).map(|x| x.label(row)).unwrap_or_default())]);
            let b = board.clone();
            edit(s, r, words, |c| {
                delete_custom(c, &b, row, ci);
                Ok(())
            })
        }
        "note" => {
            let row = usize_arg(args, "row");
            let Some(x) = board.shaped.get(row) else { return Ok(json!({})) };
            let label = x.label(row);
            let field = str_arg(args, "field");
            let value = args.get("value").cloned().unwrap_or(Value::Null);
            let b = board.clone();
            let types = k.view.planning.flow.prefs.duty_types.clone();
            let (words, f): (String, Box<dyn Fn(&mut rf::DutyNote)>) = match field.as_str() {
                "kind" => {
                    let at = value.as_u64().unwrap_or(0) as usize;
                    let kind = match at {
                        0 => None,
                        n if n <= types.len() => Some(types[n - 1].name.clone()),
                        _ => Some(rf::OTHER.to_string()),
                    };
                    (trf("Type of duty %{d} set", &[("d", &label)]), Box::new(move |n: &mut rf::DutyNote| n.kind = kind.clone()))
                }
                "paid" => {
                    let v = value.as_i64().unwrap_or(0) as i32;
                    (trf("Paid time of duty %{d} set to %{t}", &[("d", &label), ("t", &rf::hm(v))]), Box::new(move |n: &mut rf::DutyNote| n.paid = Some(v)))
                }
                "paid_auto" => (trf("Paid time of duty %{d} automatic", &[("d", &label)]), Box::new(|n: &mut rf::DutyNote| n.paid = None)),
                "sign_on" | "sign_off" => {
                    let v = value.as_i64().unwrap_or(0) as i32;
                    let on = field == "sign_on";
                    (trf("Signing of duty %{d} set", &[("d", &label)]), Box::new(move |n: &mut rf::DutyNote| if on { n.sign_on = Some(v) } else { n.sign_off = Some(v) }))
                }
                "sign_on_at" | "sign_off_at" => {
                    // (a relief point's key; empty where the bus is, null as the company has it)
                    let at: Option<String> = value.as_str().map(|x| x.trim().to_string());
                    let off = field == "sign_off_at";
                    let words = if off { trf("Sign-off point of duty %{d} set", &[("d", &label)]) } else { trf("Sign-on point of duty %{d} set", &[("d", &label)]) };
                    (words, Box::new(move |n: &mut rf::DutyNote| if off { n.sign_off_at = at.clone() } else { n.sign_on_at = at.clone() }))
                }
                "break_at" => {
                    let gap = usize_arg(args, "gap") as u32;
                    let at: Option<String> = value.as_str().map(str::trim).filter(|x| !x.is_empty()).map(String::from);
                    (trf("Break place of duty %{d} set", &[("d", &label)]), Box::new(move |n: &mut rf::DutyNote| {
                        n.breaks_at.retain(|x| x.0 != gap);
                        if let Some(a) = &at {
                            n.breaks_at.push((gap, a.clone()));
                        }
                    }))
                }
                "travel" => {
                    let gap = usize_arg(args, "gap") as u32;
                    let mode = Travel::ALL[(value.as_u64().unwrap_or(0) as usize).min(Travel::ALL.len() - 1)];
                    (trf("Travel of duty %{d} by %{mode}", &[("d", &label), ("mode", &tr(mode.label()))]), Box::new(move |n: &mut rf::DutyNote| {
                        n.travel.retain(|x| x.0 != gap);
                        n.travel.push((gap, mode));
                    }))
                }
                "custom" => {
                    let kind = rf::CUSTOM_KINDS[(usize_arg(args, "kind")).min(rf::CUSTOM_KINDS.len() - 1)].to_string();
                    let (from, to) = (args.get("from").and_then(Value::as_i64).unwrap_or(810) as i32, args.get("to").and_then(Value::as_i64).unwrap_or(900) as i32);
                    (trf("Custom work added to duty %{d}", &[("d", &label)]), Box::new(move |n: &mut rf::DutyNote| n.custom.push(rf::Custom { kind: kind.clone(), from, to, place: String::new() })))
                }
                "custom_time" => {
                    // (its times typed on its card, or its edges or itself dragged in the Gantt)
                    let ci = usize_arg(args, "ci");
                    let mut cw = x.custom.get(ci).cloned().ok_or_else(|| "no custom work".to_string())?;
                    cw.set_times(args.get("from").and_then(Value::as_i64).unwrap_or(cw.from as i64) as i32, args.get("to").and_then(Value::as_i64).unwrap_or(cw.to as i64) as i32);
                    let (from, to) = (cw.from, cw.to);
                    let words = trf("%{kind} of duty %{d} set to %{from}–%{to}", &[("kind", &tr(&cw.kind)), ("d", &label), ("from", &rf::clock(from)), ("to", &rf::clock(to))]);
                    (words, Box::new(move |n: &mut rf::DutyNote| {
                        if let Some(c) = n.custom.get_mut(ci) {
                            c.set_times(from, to);
                        }
                    }))
                }
                other => return Err(format!("no field {other}")),
            };
            edit(s, r, words, |c| {
                set_note(c, &b, row, &*f);
                Ok(())
            })
        }
        "travel_event" => {
            // (an event card's way: the gap before the travel)
            let (row, mode) = (usize_arg(args, "row"), usize_arg(args, "mode").min(Travel::ALL.len() - 1));
            let from = args.get("from").and_then(Value::as_i64).unwrap_or(0) as i32;
            let Some(x) = board.shaped.get(row) else { return Ok(json!({})) };
            let Some(key) = x.key.clone() else { return Ok(json!({})) };
            // (before the first piece: the way from the sign-on point; after the last: to the
            // sign-off point - `signing`)
            let gap = if x.spans.first().is_some_and(|sp| from < sp.from) {
                rf::signing::ON_LEG
            } else if x.spans.last().is_some_and(|sp| from >= sp.to) {
                rf::signing::OFF_LEG
            } else {
                x.spans.iter().rposition(|sp| sp.to <= from).unwrap_or(0) as u32
            };
            let words = trf("Travel of duty %{d} by %{mode}", &[("d", &x.label(row)), ("mode", &tr(Travel::ALL[mode].label()))]);
            edit(s, r, words, |c| {
                let n = rf::note_mut(c, wd, &key);
                n.travel.retain(|t| t.0 != gap);
                n.travel.push((gap, Travel::ALL[mode]));
                Ok(())
            })
        }
        "dismiss" => {
            let keys: Vec<String> = arg::<Vec<String>>(args, "keys").unwrap_or_default();
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            let words = if on { tr("%{n} issues dismissed") } else { tr("%{n} issues restored") }.replace("%{n}", &keys.len().to_string());
            edit(s, r, words, |c| {
                for key in &keys {
                    rf::set_dismissed(c, wd, key, on);
                }
                Ok(())
            })
        }
        "drop" => {
            // (what is in hand let go at a drop zone: `rf_gantt::drop_at`)
            let zone = args.get("zone").cloned().unwrap_or(Value::Null);
            let kind = zone.get("kind").and_then(Value::as_str).unwrap_or("");
            let zi = usize_arg(&zone, "i");
            match (hand_of(args), kind) {
                (Hand::Trips { tour, trips }, "block") => {
                    let words = trf("%{n} trips moved from %{a} to %{b}", &[("n", &trips.len().to_string()), ("a", &label_of(tour)), ("b", &label_of(zi))]);
                    edit(s, r, words, |c| st::move_trips(c, wd, &tours, tour, &trips, Target::Block(zi)))
                }
                (Hand::Trips { tour, trips }, "new") => {
                    let words = trf("%{n} trips moved from %{a} to a new vehicle", &[("n", &trips.len().to_string()), ("a", &label_of(tour))]);
                    let moved = refs_of(&tours, trips.iter().map(|&j| (tour, j)));
                    edit_moving(s, r, words, &moved, |c| st::move_trips(c, wd, &tours, tour, &trips, Target::New))
                }
                (Hand::Trips { tour, trips }, "stack") => {
                    let words = trf("%{n} trips of %{a} unassigned to the stack", &[("n", &trips.len().to_string()), ("a", &label_of(tour))]);
                    edit(s, r, words, |c| st::unassign(c, wd, &tours, tour, &trips))
                }
                (Hand::Stack(sk), "block" | "new") => {
                    let Some((line, x)) = board.stack.get(sk).cloned() else { return Ok(json!({})) };
                    let to = if kind == "block" { Target::Block(zi) } else { Target::New };
                    let words = trf("Trip of %{time} assigned from the stack", &[("time", &rf::clock(x.dep))]);
                    edit(s, r, words, |c| st::assign(c, wd, &tours, &line, &x, to))
                }
                (Hand::Piece { tour, duty }, "duty") => {
                    let d = usize_arg(&zone, "d");
                    let Some(first) = board.shaped.get(d).and_then(|x| x.duty).and_then(|di| board.duties[di].pieces.first().copied()) else { return Ok(json!({})) };
                    let words = trf("Piece of %{v} joined into duty %{d}", &[("v", &label_of(tour)), ("d", &board.shaped[d].label(d))]);
                    edit(s, r, words, |c| {
                        let _ = st::detach(c, wd, &tours, (tour, duty));
                        st::join(c, wd, &tours, (first.tour, first.duty), (tour, duty))
                    })
                }
                (Hand::Piece { tour, duty }, "new_duty") => {
                    let words = trf("Piece of %{v} made a duty of its own", &[("v", &label_of(tour))]);
                    edit_moving(s, r, words, &piece_rest(&tours, tour, duty, 0), |c| st::detach(c, wd, &tours, (tour, duty)))
                }
                // (Luc's driver drag: `driver_drag`)
                (Hand::DutyTrips { duty, trips, rows }, "duty" | "new_duty") => {
                    let (lines, day) = (k.lines.clone(), k.date.clone());
                    let again = move |v: &Company| co::network::tours_of_day(v, &lines, &day);
                    driver_drag::drop(s, r, &board, duty, trips, &rows, kind, usize_arg(&zone, "d"), &again)
                }
                _ => json!({}),
            }
        }
        "pick" => {
            // (a locked vehicle or duty is not taken up: said at once, `rf_gantt::pick`)
            let c = &k.view;
            let refused = match hand_of(args) {
                Hand::Trips { tour, .. } => tours.get(tour).is_some_and(|t| rf::block_locked(c, wd, &t.line, &t.tour)).then_some(st::LOCKED),
                // (the duty locked, or one of the trips' buses: `rf_gantt::pick`'s `Hand::Duty`)
                Hand::DutyTrips { duty, trips, .. } => {
                    // (trips of several duties: any of their duties locked)
                    let locked = |&(i, j): &(usize, usize)| st::duty_of_trip(&board.duties, &tours, i, j).and_then(|di| board.shaped.iter().find(|x| x.duty == Some(di))).is_some_and(|x| x.locked);
                    if board.shaped.get(duty).is_some_and(|x| x.locked) || trips.iter().any(locked) {
                        Some(st::DUTY_LOCKED)
                    } else {
                        trips.iter().any(|&(i, _)| tours.get(i).is_some_and(|t| rf::block_locked(c, wd, &t.line, &t.tour))).then_some(st::LOCKED)
                    }
                }
                Hand::Piece { tour, duty } => rf::piece_locked(c, wd, &tours, tour, duty).then_some(st::DUTY_LOCKED),
                _ => None,
            };
            match refused {
                Some(why) => json!({ "refused": true, "say": { "text": tr(why), "error": true } }),
                None => json!({}),
            }
        }
        other => return Err(format!("no action {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::network::PlannedTrip;

    fn trip(a: &str, b: &str, dep: i32) -> PlannedTrip {
        PlannedTrip { line: "4".into(), from: a.into(), to: b.into(), stops: 6, dep, arr: dep + 30, origin: "1".into(), ..Default::default() }
    }

    #[test]
    fn a_block_is_named_by_its_line_and_tour_and_trips_point_their_way() {
        let t = TourOfDay { line: "Linie4".into(), number: "4".into(), tour: "1".into(), trips: vec![trip("Nord", "Süd", 360), trip("Süd", "Nord", 400)], ..Default::default() };
        assert_eq!(block_label(&t), "4/01");
        let tours = vec![t.clone()];
        assert!(outbound(&tours, "Linie4", &t.trips[0]));
        assert!(!outbound(&tours, "Linie4", &t.trips[1]));
        let b = Board { tours, ..Default::default() };
        let (a, z) = day_span(&b);
        assert!(a <= 340 && z >= 450 && a % 60 == 0 && z % 60 == 0);
    }

    #[test]
    fn the_cheapest_zones_get_five_stars_and_one_that_breaks_a_rule_two_at_most() {
        assert_eq!(rate(&[(-500, true), (0, true), (800, true), (-500, false)]), vec![5, 3, 1, 2]);
        assert_eq!(rate(&[(7, true)]), vec![5]);
    }

    #[test]
    fn the_board_says_its_blocks_duties_issues_and_figures() {
        company::words::set_language("en");
        let c = co::found(&co::Founding { name: "Stadtbus".into(), game_mode: co::GameMode::Normal, date: "2024-05-06".into(), ..Default::default() }, "Luc");
        let trips: Vec<PlannedTrip> = (0..12).map(|k| if k % 2 == 0 { trip("Nord", "Süd", 360 + k * 40) } else { trip("Süd", "Nord", 360 + k * 40) }).collect();
        let t = TourOfDay { line: "Linie4".into(), number: "4".into(), tour: "1".into(), trips, ..Default::default() };
        let b = rf::board_of(&c, 0, vec![t], Vec::new());
        let v = board_json(&c, &b);
        assert_eq!(v["blocks"].as_array().map(Vec::len), Some(1));
        assert_eq!(v["blocks"][0]["label"], "4/01");
        assert_eq!(v["blocks"][0]["trips"].as_array().map(Vec::len), Some(12));
        // every gap between two trips at a terminus may be cut, or says why not
        assert!(v["blocks"][0]["gaps"].as_array().is_some_and(|g| !g.is_empty() && g.iter().all(|x| x[2].as_str().is_some_and(|s| !s.is_empty()))));
        assert!(!v["duties"].as_array().unwrap().is_empty());
        assert_eq!(v["status"]["trips"], b.figures.trips);
        // the figures and their differences: fewer duties is better
        let mut was = b.figures.clone();
        was.duties += 1;
        let k = kpis_json(&b.figures, Some(&was));
        assert_eq!(k["top"].as_array().map(Vec::len), Some(5));
        assert_eq!(k["top"][0]["delta"][0], "-1");
        assert_eq!(k["top"][0]["delta"][1], true);
        assert_eq!(k["more"].as_array().map(Vec::len), Some(2));
        assert!(kpis_json(&b.figures, None)["top"][0]["delta"].is_null());
    }

    /// Line 5 between A and B with tours 1 (06:00) and 2 (06:30) of eight trips of 50 minutes,
    /// ten minutes at each end, and a company that runs it.
    pub(super) fn line5_company() -> (Company, omsi_launcher_lib::LineInfo) {
        use omsi_launcher_lib::{LineInfo, StopInfo, TourInfo, TripInfo};
        let trip = |no: &str, k: i32, from: i32| {
            let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
            let dep = from + k * 60;
            TripInfo { name: format!("5 {no}-{k}"), index: 0, line: "5".into(), from: a.into(), terminus: format!("{b} Bf"), departure: dep as f64 * 60.0, arrival: (dep + 50) as f64 * 60.0, stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(), km: 20.0 }
        };
        let tour = |no: &str, from: i32| TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips: (0..8).map(|k| trip(no, k, from)).collect() };
        let l = LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 390)] };
        let mut c = co::found(&co::Founding { name: "Flow".into(), game_mode: co::GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.lines.push(co::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        (c, l)
    }

    #[test]
    fn a_rule_saved_applies_to_the_day_at_once_and_a_proposal_of_the_old_rules_goes() {
        // (testers: "When it asks me about amount of time that can be worked before a break etc,
        // its not actually realising this immediately")
        company::words::set_language("en");
        let (c, l) = line5_company();
        let date = c.date.clone();
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        let mut r = Rf::default();
        let count = |k: &Cached, text: &str| k.board.issues.iter().filter(|i| i.text == text).count();
        let k = ensure(&mut s, &mut r, &date).unwrap().unwrap();
        // two whole tours of 7:50 with ten-minute layovers: no break, over 4:30 and no meal break
        assert_eq!((count(&k, rf::I_CONTINUOUS), count(&k, rf::I_MEAL)), (2, 2));
        let duty = &k.json["duties"][0]["rest"];
        assert_eq!(duty["periods"].as_array().map(Vec::len), Some(1));
        assert_eq!(duty["periods"][0]["over"], true);
        assert!(duty["meal"]["tip"].as_str().is_some_and(|t| t.contains("No meal break by")));
        // Optimise's proposal, made to these rules
        r.proposal = Some(Proposal { date: date.clone(), generation: k.generation, base: k.rev.clone(), board: k.board.clone(), view: k.view.clone(), layouts: Vec::new(), joined: Vec::new(), better: true, note: String::new() });
        // the rules edited, not saved: what each breaks then, next to its fields
        let mut edited = (*k.view).clone();
        edited.planning.flow.prefs.break_from = 10;
        edited.planning.flow.prefs.break_min = 10;
        let check = breaks::prefs_check(&k.board, &rf::board(&edited, &k.lines, &date));
        assert_eq!(check["counts"]["continuous"]["n"], 0);
        assert_eq!(check["counts"]["continuous"]["text"], "Nothing breaks this rule now. (2 with the saved rules)");
        assert_eq!(check["counts"]["breaks"]["text"], "2 duties break this rule now.");
        // the rules saved as the "prefs" action saves them (`company::act` writes the company to
        // the data folder, so here it is changed in the session): the cached day is the old
        // rules' until they are applied
        let was = s.company.as_ref().unwrap().planning.flow.prefs.clone();
        let before = breaks::rule_counts(&k.board);
        {
            let p = &mut s.company.as_mut().unwrap().planning.flow.prefs;
            p.break_from = 10;
            p.break_min = 10;
            p.meal_after = 240;
        }
        assert_eq!(count(&ensure(&mut s, &mut r, &date).unwrap().unwrap(), rf::I_CONTINUOUS), 2, "the cache of the old rules");
        let out = rules_applied(&mut s, &mut r, &date, &was, Some(&before));
        let k = ensure(&mut s, &mut r, &date).unwrap().unwrap();
        assert_eq!((count(&k, rf::I_CONTINUOUS), count(&k, rf::I_MEAL)), (0, 2));
        assert_eq!(k.json["duties"][0]["rest"]["periods"].as_array().map(Vec::len), Some(8));
        assert!(r.proposal.is_none(), "a proposal made to the old rules");
        // the changed rule the duties break: said at once, Optimise offered
        assert_eq!(out["offer"]["text"], "Broken now by the changed rules: Duty Breaks (2 duties)");
        assert_eq!(out["say"]["text"], out["offer"]["text"]);
        assert!(out["offer"]["groups"].as_array().is_some_and(|g| g.iter().any(|x| x == rf::I_MEAL)));
    }

    #[test]
    fn two_vehicles_of_a_line_ticked_go_to_the_stack_together() {
        let (c, l) = line5_company();
        let date = c.date.clone();
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        let mut r = Rf::default();
        let k = ensure(&mut s, &mut r, &date).unwrap().unwrap();
        assert_eq!(k.board.tours.len(), 2);
        let ids: Vec<String> = k.board.tours.iter().map(block_id).collect();
        let mut v = (*k.view).clone();
        unassign_blocks(&mut v, k.board.weekday, &ids, &|x: &Company| co::network::tours_of_day(x, &k.lines, &date));
        // (both buses' trips on the stack, the line's layout still fitting the timetable: not
        // the first bus's trips on it and on the stack at once, the line back to automatic)
        let b = rf::board(&v, &k.lines, &date);
        assert!(b.tours.is_empty(), "{:?}", b.tours.iter().map(|t| &t.tour).collect::<Vec<_>>());
        assert_eq!(b.stack.len(), 16);
    }

    #[test]
    fn optimise_says_what_it_did_and_a_day_it_cannot_better_has_nothing_to_accept() {
        // (testers, 2026-10-15: "optimise feature do not work in rosterflow")
        company::words::set_language("en");
        let (c, l) = line5_company();
        let date = c.date.clone();
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        let mut r = Rf::default();
        let k = ensure(&mut s, &mut r, &date).unwrap().unwrap();
        let out = optimise(&mut r, &k);
        let text = out["say"]["text"].as_str().unwrap_or_default().to_string();
        assert!(text.starts_with("Optimised: ") && out["proposal"] == true, "{text}");
        let pr = r.proposal.clone().expect("a proposal");
        assert!(pr.better && pr.note == text);
        assert!(pr.board.figures.duties != k.board.figures.duties || pr.board.tours != k.board.tours);
        // the proposal kept (as Accept keeps it; `company::act` writes the company to the data
        // folder, so here it is put in the session): Optimise again finds nothing better, says so,
        // and has nothing to accept
        rf::apply(s.company.as_mut().unwrap(), pr.board.weekday, pr.layouts.clone(), pr.joined.clone());
        r.cache = None;
        r.proposal = None;
        let k = ensure(&mut s, &mut r, &date).unwrap().unwrap();
        let out = optimise(&mut r, &k);
        assert!(out["say"]["text"].as_str().is_some_and(|t| t.starts_with("No better plan found")), "{}", out["say"]["text"]);
        assert!(r.proposal.as_ref().is_some_and(|p| !p.better && p.layouts.len() == k.view.planning.layouts.iter().filter(|x| x.weekday == k.board.weekday).count()));
        // neither the vehicles nor the duties to make: said, no proposal
        r.proposal = None;
        let mut v = (*k.view).clone();
        v.planning.flow.prefs.optimise_vehicles = false;
        v.planning.flow.prefs.optimise_duties = false;
        let k2 = Cached { view: Arc::new(v), ..k.clone() };
        let out = optimise(&mut r, &k2);
        assert_eq!(out["say"]["error"], true);
        assert!(r.proposal.is_none());
        // what a day it cannot keep the hard rules on says why
        let mut b = (*k.board).clone();
        b.issues.push(rf::Issue { scope: Scope::Duty, row: Some(0), text: rf::I_TYPE, args: vec![("type", rf::OTHER.into())], hard: true, penalty: 0, key: "x".into() });
        let words = unmet_words(&k.view, &b).join(" ");
        assert!(words.contains("1 duties can't keep every hard rule, because of: Duty type unsatisfied (1)."), "{words}");
    }

    /// The English texts the commands hand to `tr`/`trf` (the first literal of each call).
    fn said_texts() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/omsihub");
        let mut files = vec![dir.join("rosterflow.rs")];
        for e in std::fs::read_dir(dir.join("rosterflow")).unwrap().flatten() {
            files.push(e.path());
        }
        let mut out = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            for lead in ["tr(\"", "trf(\""] {
                let mut rest = src.as_str();
                while let Some(k) = rest.find(lead) {
                    let after = &rest[k + lead.len()..];
                    rest = after;
                    let mut text = String::new();
                    let mut chars = after.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => text.extend(chars.next()),
                            '"' => break,
                            c => text.push(c),
                        }
                    }
                    out.push((name.clone(), text));
                }
            }
        }
        out.extend(timeplan::texts().into_iter().map(|t| ("timeplan.rs".to_string(), t.to_string())));
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn every_text_rosterflow_says_is_translated() {
        let texts = said_texts();
        assert!(texts.len() > 150, "only {} texts found", texts.len());
        let mut core: Vec<&str> = rf::texts();
        core.extend(st::texts());
        core.extend(co::unassign::texts());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for (file, t) in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{file} {lang}: {t:?}"));
                }
            }
            for t in &core {
                if !table.contains_key(*t) {
                    missing.push(format!("core {lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}

//! RosterFlow's Timeplan (`crates/omsi-launcher-ui/src/company/planning/timeplan.rs`): a
//! line's running times and its timetable, made the way a scheduler makes them
//! (`co::timeplan`). The line being changed - its draft, undo and redo, what was saved since the
//! studio made its blocks (UNSYNCED), what the status bar says first of it - is kept here; the
//! page keeps what it shows (the day group, the side, the pattern, the screen, the cell typed
//! into) and hands it with every command.
//!
//! Saved, the line's timetable is the company's as every own line's is (`plan_edit::commit` of
//! the game-drawn launcher: a line in service from the day chosen, else at once; a line of the
//! map's at once, `save_map`): the registry, the map's timetable and the depot files written
//! into the content folder, the company's timetables read again.

use super::super::company::kit::Popup;
use super::super::company::words::{day_label, eur, tr, trf};
use super::super::company::{self, Session};
use super::{str_arg, usize_arg, with_rf, Rf};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::plandays::{self as pd, DayCopied, How, Template};
use omsi_launcher_lib::company::timeplan::{self as tp, Level, RunSet, Side};
use omsi_launcher_lib::company::{self as co, ownline, Company, CompanyLine};
use omsi_launcher_lib::lines::{self as reg, LineDesign, StopRef, DAY_GROUPS};
use omsi_launcher_lib::service::VehicleClass;
use omsi_launcher_lib::{linehof, owndepot};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Why a line cannot be planned here.
const MAP_NOT_EDITED: &str = "A line of the map's own timetable: open it once in the line editor (Lines page) to plan its timetable here.";
const NO_LINE: &str = "The company runs no line yet: add one on the Lines page first.";
/// Said for a line of the map's just added (RosterFlow on): it keeps the map's timetable.
pub const MAP_LINE_HINT: &str = "The line keeps the map's timetable: change it here if you like, save, then Refresh";
/// Said for a line just confirmed in the line editor (`lineeditor::NEW_LINE_HINT`).
pub const NEW_LINE_HINT: &str = "Set the running times and departures, save, then Refresh to make the buses and duties";
const NOT_FOUND: &str = "The line is no longer in the line editor's list: open the timetable again.";

const RUNNING: usize = 0;
const TIMETABLE: usize = 1;

/// An own line being changed (`plan_edit::Own`): its registry and its id there, its file stem,
/// the line as the registry has it (`saved`), the same without a waiting change's mark
/// (`base`), as the map's timetable runs it now (`live`) and as changed here (`draft`).
#[derive(Clone)]
pub(crate) struct Own {
    id: u64,
    path: PathBuf,
    pub(crate) stem: String,
    pub(crate) saved: LineDesign,
    pub(crate) base: LineDesign,
    pub(crate) live: LineDesign,
    pub(crate) draft: LineDesign,
}

fn own_of(id: u64, path: PathBuf, stem: String, saved: LineDesign) -> Own {
    let base = LineDesign { live: None, pending_from: String::new(), ..saved.clone() };
    let live = match saved.live.as_deref() {
        Some(x) => LineDesign { id, live: None, pending_from: String::new(), ..x.clone() },
        None => base.clone(),
    };
    Own { id, path, stem, saved, draft: base.clone(), base, live }
}

/// The line being planned: its own line, whether it is a line of the map's, and whether it was
/// imported from the map's timetable for the Timeplan (into the registry when it is saved).
#[derive(Clone)]
pub(crate) struct Work {
    own: Own,
    map: bool,
    fresh: bool,
}

/// The Timeplan's own (`TimeplanView` of the game-drawn launcher, without what the page keeps).
#[derive(Default)]
pub(crate) struct Tp {
    company: String,
    line: String,
    loaded: Option<(String, String)>,
    work: Option<Work>,
    why: Option<&'static str>,
    undo: Vec<LineDesign>,
    redo: Vec<LineDesign>,
    /// Lines saved since the scheduling studio made their blocks (line, day group).
    unsynced: Vec<(String, usize)>,
    /// The day a change of a line in service begins.
    from: String,
    /// Changes made so far (undone ones too), for the lessons.
    changes: u64,
    /// What the status bar says first for a line (its timetable name).
    hint: Option<(String, &'static str)>,
}

impl Tp {
    fn dirty(&self) -> bool {
        self.work.as_ref().is_some_and(|w| w.own.draft != w.own.base)
    }

    fn draft(&self) -> Option<&LineDesign> {
        self.work.as_ref().map(|w| &w.own.draft)
    }

    /// The line changed by `f`, the line before kept to be undone.
    fn change(&mut self, f: impl FnOnce(&mut LineDesign)) {
        let Some(w) = self.work.as_mut() else { return };
        let before = w.own.draft.clone();
        f(&mut w.own.draft);
        if w.own.draft != before {
            self.changes += 1;
            self.undo.push(before);
            if self.undo.len() > 80 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    /// The running times of day group `day` and direction `dir` changed by `f`: every trip of
    /// theirs takes them at once.
    fn change_run(&mut self, day: usize, dir: usize, f: impl FnOnce(&mut RunSet)) {
        self.change(|line| {
            let Some(d) = line.directions.get(dir).cloned() else { return };
            let mut plan = tp::plan_of(line);
            let old = plan.set(line, day, dir);
            let mut new = old.clone();
            f(&mut new);
            new.fit(&d);
            tp::retime(line, day, dir, &old, &new);
            plan.put(new);
            line.timeplan = Some(plan);
        });
    }

    fn undo(&mut self) {
        if let (Some(prev), Some(w)) = (self.undo.pop(), self.work.as_mut()) {
            self.redo.push(std::mem::replace(&mut w.own.draft, prev));
        }
    }

    fn redo(&mut self) {
        if let (Some(next), Some(w)) = (self.redo.pop(), self.work.as_mut()) {
            self.undo.push(std::mem::replace(&mut w.own.draft, next));
        }
    }

    /// Open the line `line` of the company `company` with `hint` in the status bar (the line
    /// editor confirmed it, a line of the map's was added): read again unless changes of it are
    /// in hand. Returns the screen to show (a new line's running times, a map's timetable).
    pub(crate) fn guide(&mut self, company: &str, line: &str, hint: Option<&'static str>) -> Option<usize> {
        let same = self.company == company && self.line.eq_ignore_ascii_case(line);
        self.company = company.to_string();
        self.line = line.to_string();
        if !(same && self.dirty()) {
            self.loaded = None;
        }
        let h = hint?;
        self.hint = Some((line.to_string(), h));
        Some(if h == MAP_LINE_HINT { TIMETABLE } else { RUNNING })
    }

    /// What the status bar says first of the line shown: its hint, or - an own line with no
    /// trip saved - what to do with it.
    fn hint_shown(&self) -> Option<&'static str> {
        if let Some((_, h)) = self.hint.as_ref().filter(|h| h.0.eq_ignore_ascii_case(&self.line)) {
            return Some(*h);
        }
        self.work.as_ref().filter(|w| !w.map && w.own.base.table.is_empty()).map(|_| NEW_LINE_HINT)
    }

    /// The line shown as a lesson finds it: its name, the line as it is saved, and what was
    /// UNSYNCED then.
    pub(crate) fn keep_for_lesson(&self) -> Option<(String, LineDesign, Vec<(String, usize)>)> {
        let w = self.work.as_ref()?;
        Some((self.line.clone(), w.own.base.clone(), self.unsynced.clone()))
    }
}

// --- reading the line -------------------------------------------------------------------------------

/// The company line's own line in the line editor's registry (`plan_edit::load`).
/// (Omsi-Hub 6/6: the planning's sheet of a line's timetable opens the line this way too.)
pub(crate) fn load_own(c: &Company, cl: &CompanyLine) -> Result<Own, &'static str> {
    let id = cl.plan.as_ref().map(|p| p.line_id).ok_or(NOT_FOUND)?;
    let path = reg::registry_path(&reg::map_folder(&c.map));
    let r = reg::load_registry(&path);
    let saved = r.line(id).cloned().ok_or(NOT_FOUND)?;
    let stem = reg::stems(&r).get(&id).cloned().unwrap_or_else(|| cl.name.clone());
    Ok(own_of(id, path, stem, saved))
}

/// A map's `global.cfg` (`lineeditor::import_for_timeplan`'s way: a content root's, else under
/// the game's folder).
fn global_of(map_file: &str) -> PathBuf {
    let root = core::root_dir().unwrap_or_default();
    omsi_cfg::find_in_roots(map_file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(&root, map_file))
}

/// The map's line file of `ttl` as the line editor reads it: its `.orig` (the map's own, kept
/// when the line editor wrote the line) where there is one.
fn original_line(tt: &omsi_timetable::TimetableData, ttl: &str) -> Option<omsi_timetable::Line> {
    let line = tt.lines.iter().find(|l| l.name.trim().eq_ignore_ascii_case(ttl.trim()))?;
    let orig = PathBuf::from(format!("{}.orig", line.path.display()));
    if orig.is_file() {
        if let Ok(f) = omsi_cfg::CfgFile::read(&orig) {
            return Some(omsi_timetable::Line { name: line.name.clone(), path: line.path.clone(), ..omsi_timetable::Line::parse(&f) });
        }
    }
    Some(line.clone())
}

/// The line `ttl` of the map `map_file` as RosterFlow's Timeplan opens one the line editor never
/// saved (`lineeditor::import_for_timeplan`): imported as the line editor imports it, without
/// the map's roads. None: the map has no such line with a trip.
fn import_for_timeplan(map_file: &str, date: &str, ttl: &str) -> Option<LineDesign> {
    let global = global_of(map_file);
    let map_dir = global.parent().map(Path::to_path_buf)?;
    let tiles = omsi_map::GlobalCfg::load(&global).map(|g| g.raw_tiles).unwrap_or_default();
    let tt = omsi_timetable::TimetableData::load(&map_dir);
    let line = original_line(&tt, ttl)?;
    let stop_of = |id: i64, name: &str| -> StopRef {
        let tile = tt.bus_stops.iter().find(|b| b.object_id == id).and_then(|b| usize::try_from(b.group).ok()).and_then(|k| tiles.get(k)).map(|t| [t.0, t.1]);
        StopRef { id, name: if name.trim().is_empty() { format!("Stop {id}") } else { name.trim().to_string() }, tile: tile.unwrap_or_default(), ..Default::default() }
    };
    let mut l = reg::import_map_line(&tt, &line, &tiles, &stop_of)?;
    // (the depot file of its depot group, as the line editor finds it)
    let chrono = omsi_map::date_code(date).map(|c| omsi_map::active_chrono_dirs(&map_dir, c)).unwrap_or_default();
    let ai = omsi_map::ailists::ailists_with_chrono(&map_dir, &chrono);
    let hof = ai.groups.iter().find(|g| g.name.trim().eq_ignore_ascii_case(l.ai_group.trim())).and_then(|g| g.hof.clone()).unwrap_or_default();
    if !hof.trim().is_empty() {
        let (bases, _) = core::depot_roots();
        if let Some(depot) = linehof::first_depot(hof.trim(), &bases, &reg::map_folder(map_file)) {
            let number = l.number.clone();
            for d in &mut l.directions {
                let names: Vec<&str> = d.stops.iter().map(|s| s.name.as_str()).collect();
                d.map_route = depot.route_of(&d.number_or(&number), &names).unwrap_or(0);
            }
        }
    }
    Some(l)
}

/// The company line's line in the line editor's registry, made a table of trips to plan - a
/// line of the map's the line editor never saved imported from the map's timetable - or why it
/// cannot be planned here.
fn load(c: &Company, cl: &CompanyLine) -> Result<Work, &'static str> {
    let (mut own, map, fresh) = if cl.own {
        (load_own(c, cl)?, false, false)
    } else {
        let path = reg::registry_path(&reg::map_folder(&c.map));
        let r = reg::load_registry(&path);
        match r.map_line(&cl.name).cloned() {
            Some(line) => {
                let stem = reg::stems(&r).get(&line.id).cloned().unwrap_or_default();
                (own_of(line.id, path, stem, line), true, false)
            }
            None => {
                let line = import_for_timeplan(&c.map, &c.date, &cl.name).ok_or(MAP_NOT_EDITED)?;
                (own_of(0, path, String::new(), line), true, true)
            }
        }
    };
    tp::as_table(&mut own.base);
    own.draft = own.base.clone();
    Ok(Work { own, map, fresh })
}

/// The line chosen (the first own line, else the first), read when another is chosen.
fn load_shown(v: &mut Tp, c: &Company) {
    if v.company != c.id || !c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&v.line)) {
        v.company = c.id.clone();
        v.line = c.lines.iter().find(|x| x.own).or(c.lines.first()).map(|x| x.name.clone()).unwrap_or_default();
    }
    let key = (v.company.clone(), v.line.clone());
    if v.loaded.as_ref() != Some(&key) {
        v.loaded = Some(key);
        v.undo.clear();
        v.redo.clear();
        match c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)) {
            Some(cl) => match load(c, cl) {
                Ok(w) => {
                    let pending = w.own.saved.pending_from.clone();
                    v.from = Some(pending).filter(|d| co::dates::between(&c.date, d) >= 1).unwrap_or_else(|| co::dates::add(&c.date, 1));
                    v.work = Some(w);
                    v.why = None;
                }
                Err(why) => (v.work, v.why) = (None, Some(why)),
            },
            None => (v.work, v.why) = (None, Some(NO_LINE)),
        }
    }
}

// --- words ------------------------------------------------------------------------------------------

/// The vehicle types a line allows, short ("SD,DD"; "All" when any).
fn vehicle_types(l: &LineDesign) -> String {
    if l.vehicles.classes.is_empty() {
        return tr("All");
    }
    l.vehicles
        .classes
        .iter()
        .map(|c| match c {
            VehicleClass::Minibus => "MB",
            VehicleClass::Midi => "MD",
            VehicleClass::Solo => "SD",
            VehicleClass::Articulated => "AR",
            VehicleClass::Double => "DD",
            VehicleClass::Coach => "CO",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn day_name(k: usize) -> String {
    tr(DAY_GROUPS.get(k).map(|d| d.0).unwrap_or("Mon - Fri"))
}

/// A time typed for a trip at `near`: the day's, or a day later when that is nearer.
fn near_time(t: f32, near: f32) -> f32 {
    if t + 1440.0 <= tp::DAY_SPAN && (t + 1440.0 - near).abs() < (t - near).abs() {
        t + 1440.0
    } else {
        t
    }
}

/// The columns the timetable shows of a side: all, or its timepoints only.
fn shown_columns(line: &LineDesign, side: &Side, points_only: bool) -> Vec<usize> {
    let plan = tp::plan_of(line);
    (0..side.columns.len()).filter(|&j| !points_only || column_timepoint(line, side, &plan, j)).collect()
}

fn column_timepoint(line: &LineDesign, side: &Side, plan: &tp::RunPlan, j: usize) -> bool {
    side.patterns.iter().enumerate().any(|(p, &dir)| side.col_of[p].iter().position(|c| *c == j).is_some_and(|k| line.directions.get(dir).is_some_and(|d| plan.is_timepoint(dir, d, k))))
}

/// The stops of a direction as the strip's dots.
fn dots_of(l: &LineDesign, dir: usize) -> Vec<Value> {
    let plan = tp::plan_of(l);
    let Some(d) = l.directions.get(dir) else { return Vec::new() };
    d.stops.iter().enumerate().map(|(k, s)| json!({ "name": s.name.trim(), "code": s.id.to_string(), "timepoint": plan.is_timepoint(dir, d, k) })).collect()
}

fn line_colour(l: &LineDesign) -> String {
    let hex = l.colour.trim();
    if hex.is_empty() {
        "#42a5f5".into()
    } else {
        format!("#{}", hex.trim_start_matches('#'))
    }
}

/// The line in service: a change of its timetable begins on a day to come.
fn in_service(v: &Tp, c: &Company) -> bool {
    let Some(w) = v.work.as_ref() else { return false };
    !w.map && c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)).is_some_and(|cl| cl.service_from.is_some()) && reg::timetable_differs(&w.own.live, &w.own.draft)
}

// --- the days that use a timetable (Trello #100) ---------------------------------------------------

/// The day group whose timetable of `l` the line `name` runs in `lines` (one day's): the group
/// most of whose departures it has, the weekday's own (`natural`) on a tie; None without trips.
fn group_run(l: &LineDesign, lines: &[core::LineInfo], name: &str, natural: usize) -> Option<usize> {
    let li = lines.iter().find(|x| x.name.eq_ignore_ascii_case(name))?;
    let deps: std::collections::HashSet<i32> = li.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()).map(|x| (x.departure / 60.0).round() as i32).collect();
    if deps.is_empty() {
        return None;
    }
    let score = |g: usize| day_trips(l, g).iter().filter(|(_, t)| t.first().is_some_and(|d| deps.contains(d))).count();
    let best = (0..DAY_GROUPS.len()).max_by_key(|&g| (score(g), g == natural))?;
    (score(best) > 0).then_some(best)
}

/// The calendar strip: the days of the week shown and which use the timetable of day group
/// `shown` - each day its weekday's group, another (a holiday's timetable), none (no trips of
/// the line) or not read yet.
fn calendar(s: &Session, c: &Company, l: &LineDesign, name: &str, shown: usize) -> Vec<Value> {
    (0..7)
        .map(|k| {
            let date = co::dates::add(&c.date, k);
            let natural = super::days::group_of(co::plan::weekday_of(&date));
            let lines = company::lines_of(s, &date);
            let group = lines.as_ref().map(|x| group_run(l, x, name, natural));
            let tip = match group {
                None => tr("Reading the timetable…"),
                Some(None) => tr("No trips of this line on this day"),
                Some(Some(g)) if g == natural => trf("Runs the %{days} timetable", &[("days", &day_name(g))]),
                Some(Some(g)) => trf("Runs the %{days} timetable, not its weekday's: a holiday or another day", &[("days", &day_name(g))]),
            };
            json!({ "k": k, "date": date, "label": super::days::short(&date), "group": group.flatten(), "read": group.is_some(), "on": group.flatten() == Some(shown), "odd": group.flatten().is_some_and(|g| g != natural), "tip": tip })
        })
        .collect()
}

/// The lines saved here since the scheduling studio made their blocks on day group `group`, for
/// that day's planning (its UNSYNCED - Refresh): their names and numbers.
pub(crate) fn unsynced_on(r: &Rf, c: &Company, group: usize) -> Vec<Value> {
    r.tp
        .unsynced
        .iter()
        .filter(|u| u.1 == group)
        .map(|u| json!({ "line": u.0, "number": c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&u.0)).map(|l| l.number.trim().to_string()).unwrap_or_else(|| u.0.clone()) }))
        .collect()
}

// --- the view ---------------------------------------------------------------------------------------

/// What the page keeps and hands: the day group, the side, the pattern, the screen.
struct Shown {
    day: usize,
    side: usize,
    pattern: usize,
    tab: usize,
    points_only: bool,
}

fn shown_of(args: &Value) -> Shown {
    Shown { day: usize_arg(args, "day").min(DAY_GROUPS.len() - 1), side: usize_arg(args, "side").min(1), pattern: usize_arg(args, "pattern"), tab: usize_arg(args, "tab").min(1), points_only: args.get("points_only").and_then(Value::as_bool).unwrap_or(false) }
}

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_tp" => with_rf(|s, r| view(s, r, args)),
        "rf_tp_act" => with_rf(|s, r| act(s, r, args)),
        "rf_tp_guide" => with_rf(|s, r| {
            let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
            let line = str_arg(args, "line");
            let hint = match str_arg(args, "hint").as_str() {
                "map" => Some(MAP_LINE_HINT),
                "new" => Some(NEW_LINE_HINT),
                _ => None,
            };
            let id = c.id.clone();
            let screen = r.tp.guide(&id, &line, hint);
            // (the line editor wrote the line: the company's timetables read again)
            if args.get("reload").and_then(Value::as_bool).unwrap_or(false) {
                company::reload_timetable(s);
                r.cache = None;
            }
            // (from a day of the studio, `day`: the day group the line runs on that date, so that
            // its timetable shown is the one of the trip looked at)
            let group = match (args.get("day").and_then(Value::as_u64), s.company.clone()) {
                (Some(k), Some(c)) => {
                    let date = co::dates::add(&c.date, k.min(6) as i64);
                    let natural = super::days::group_of(co::plan::weekday_of(&date));
                    load_shown(&mut r.tp, &c);
                    let run = match (r.tp.work.as_ref(), company::lines_of(s, &date)) {
                        (Some(w), Some(lines)) => group_run(&w.own.live, &lines, &r.tp.line, natural),
                        _ => None,
                    };
                    Some(run.unwrap_or(natural))
                }
                _ => None,
            };
            Ok(json!({ "screen": screen, "group": group }))
        }),
        _ => return None,
    })
}

fn view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    // (the week's timetables, for the calendar strip)
    company::week_work(s);
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    let v = &mut r.tp;
    load_shown(v, &c);
    let sh = shown_of(args);
    let lines: Vec<Value> = c.lines.iter().map(|x| json!({ "name": x.name, "label": if x.caption.trim().is_empty() { x.number.clone() } else { format!("{} · {}", x.number, x.caption.trim()) } })).collect();
    let mut out = json!({
        "line": v.line,
        "lines": lines,
        "days": (0..DAY_GROUPS.len()).map(day_name).collect::<Vec<_>>(),
        "ready": v.work.is_some(),
        "why": v.why.map(tr),
        "probe": { "ready": v.work.is_some(), "why": v.why, "dirty": v.dirty(), "changes": v.changes, "trips": v.draft().map(|d| d.table.len()).unwrap_or(0), "unsynced": v.unsynced.iter().any(|u| u.0.eq_ignore_ascii_case(&v.line)) },
    });
    let Some(w) = v.work.clone() else { return Ok(out) };
    let line = w.own.draft.clone();
    let dirty = v.dirty();
    let runs = !w.map && c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)).is_some_and(|cl| cl.service_from.is_some());
    let service = dirty && in_service(v, &c);
    if service && co::dates::between(&c.date, &v.from) < 1 {
        v.from = co::dates::add(&c.date, 1);
    }
    let issues = tp::issues(&line, sh.day);
    let errors: usize = issues.iter().filter(|i| i.level == Level::Error).map(|i| i.n.max(1)).sum();
    let warnings: usize = issues.iter().filter(|i| i.level == Level::Warning).map(|i| i.n.max(1)).sum();
    let sides = tp::sides(&line);
    let side = sides[sh.side].clone();
    let unsynced: Vec<usize> = v.unsynced.iter().filter(|u| u.0.eq_ignore_ascii_case(&v.line)).map(|u| u.1).collect();
    let o = out.as_object_mut().expect("an object");
    o.insert("number".into(), json!(line.number.trim()));
    o.insert("map".into(), json!(w.map));
    o.insert("dirty".into(), json!(dirty));
    o.insert("can_undo".into(), json!(!v.undo.is_empty()));
    o.insert("can_redo".into(), json!(!v.redo.is_empty()));
    o.insert("save".into(), json!(tr(if dirty { "Save" } else { "Saved" })));
    o.insert("runs".into(), json!(runs));
    o.insert("from".into(), json!({ "show": service, "label": format!("{} {}", tr("From"), day_label(&v.from)) }));
    o.insert("has_inbound".into(), json!(line.directions.len() > 1));
    o.insert("colour".into(), json!(line_colour(&line)));
    o.insert("issues".into(), json!(issues.iter().map(|i| json!({
        "error": i.level == Level::Error, "n": i.n.max(1),
        "text": format!("{} - {}", tr(if i.level == Level::Error { "Error" } else { "Warning" }), tr(i.text).replace("%{n}", &i.n.to_string())),
        "trip": i.trips.first(), "running": i.text == tp::NEGATIVE || i.text == tp::ZERO,
        "side": i.trips.first().and_then(|&t| line.table.get(t)).map(|x| usize::from(sides[1].patterns.contains(&x.dir))),
    })).collect::<Vec<_>>()));
    o.insert("issues_head".into(), json!(format!("{} {} · {}", tr("Line"), line.number.trim(), day_name(sh.day))));
    o.insert("issues_label".into(), json!(trf("Line %{n}: %{k}", &[("n", line.number.trim()), ("k", &(errors + warnings).to_string())])));
    o.insert("errors".into(), json!(errors));
    o.insert("warnings".into(), json!(warnings));
    o.insert("unsynced".into(), json!(!unsynced.is_empty()));
    o.insert("calendar".into(), json!(calendar(s, &c, &w.own.live, &v.line, sh.day)));
    o.insert("hint".into(), json!(v.hint_shown().map(tr)));
    o.insert("pending".into(), json!((!w.own.saved.pending_from.is_empty()).then(|| trf("A new timetable waits for %{date}: it is the one shown, against the tours that run until then.", &[("date", &day_label(&w.own.saved.pending_from))]))));
    o.insert("patterns".into(), json!((0..side.patterns.len()).map(|k| json!({
        "code": tp::pattern_code(k), "label": format!("{}  ·  {}", tp::pattern_code(k), line.directions[side.patterns[k]].destination()),
        "stops": line.directions[side.patterns[k]].stops.iter().map(|s| s.name.trim().to_string()).collect::<Vec<_>>(),
    })).collect::<Vec<_>>()));
    o.insert("copy".into(), copy_view(&c, &v.line, &line));
    if side.patterns.is_empty() {
        o.insert("no_side".into(), json!(tr("The line has no inbound direction.")));
        return Ok(out);
    }
    if sh.tab == RUNNING {
        let pattern = sh.pattern.min(side.patterns.len() - 1);
        let dir = side.patterns[pattern];
        let n = line.directions[dir].stops.len();
        let set = tp::run_set(&line, sh.day, dir);
        let bands: Vec<Value> = set
            .bands
            .iter()
            .map(|b| json!({ "from": tp::hm(b.from), "from_m": b.from, "run": (0..n.saturating_sub(1)).map(|i| b.run.get(i).copied().unwrap_or(0.0)).collect::<Vec<_>>(), "texts": (0..n.saturating_sub(1)).map(|i| tp::minutes_text(b.run.get(i).copied().unwrap_or(0.0))).collect::<Vec<_>>(), "total": format!("= {} m", tp::minutes_text(set.total(b.from, n))) }))
            .collect();
        o.insert("rt".into(), json!({ "pattern": pattern, "dir": dir, "n": n, "dots": dots_of(&line, dir), "bands": bands, "few": (n < 2).then(|| tr("This pattern has fewer than two stops: give it its stops in the line editor.")) }));
    } else {
        let rows = tp::rows(&line, sh.day, &side);
        let cols = shown_columns(&line, &side, sh.points_only);
        let plan = tp::plan_of(&line);
        let dots: Vec<Value> = cols.iter().map(|&j| json!({ "name": side.columns[j].name, "code": side.columns[j].id.to_string(), "timepoint": column_timepoint(&line, &side, &plan, j) })).collect();
        let sets: Vec<RunSet> = side.patterns.iter().map(|&dir| tp::run_set(&line, sh.day, dir)).collect();
        let vt = vehicle_types(&line);
        let rows_json: Vec<Value> = rows
            .iter()
            .enumerate()
            .filter_map(|(k, &i)| {
                let t = &line.table[i];
                let p = side.pattern(t.dir)?;
                let times = tp::trip(&line, i);
                let off = tp::offsets(&sets[p], &times);
                let dep = times.first().copied().unwrap_or(0.0);
                let headway = (k > 0).then(|| tp::minutes_text(dep - tp::departure(&line, rows[k - 1])));
                let cells: Vec<Value> = cols
                    .iter()
                    .map(|&j| match side.col_of[p].iter().position(|c| *c == j) {
                        None => Value::Null,
                        Some(s) => {
                            let tt = times.get(s).copied().unwrap_or(0.0);
                            let o = off.get(s).copied().unwrap_or(0.0);
                            if o.abs() >= 0.5 {
                                json!({ "s": s, "t": tp::hm(tt), "off": if o > 0.0 { 1 } else { -1 }, "base": trf("From the running times: %{time}", &[("time", &tp::hm(tt - o))]) })
                            } else {
                                json!({ "s": s, "t": tp::hm(tt), "late": tt > tp::DAY_SPAN })
                            }
                        }
                    })
                    .collect();
                Some(json!({ "i": i, "dir": t.dir, "code": tp::pattern_code(p), "start": tp::hm(dep), "headway": headway, "cells": cells, "n": times.len() }))
            })
            .collect();
        let by_pattern = tp::by_pattern(&line, &side, &rows);
        let by_hour = tp::by_hour(&line, &rows);
        o.insert("tt".into(), json!({
            "dots": dots, "vt": vt, "rows": rows_json,
            "kpis": { "count": rows.len(), "by_pattern": by_pattern, "by_hour": by_hour.to_vec(), "codes": (0..side.patterns.len()).map(tp::pattern_code).collect::<Vec<_>>(), "label": format!("{} · {}", line.number.trim(), day_name(sh.day)) },
        }));
    }
    Ok(out)
}

// --- "Copy to…" and the templates (`co::plandays::timetable`) ----------------------------------------

/// What the timetable's "Copy to…" offers: each day group - the trips it has, and whether the
/// line runs then - and the line's templates kept.
fn copy_view(c: &Company, name: &str, line: &LineDesign) -> Value {
    json!({
        "days": (0..DAY_GROUPS.len()).map(|d| json!({ "k": d, "label": day_name(d), "trips": pd::trips_on(line, d), "closed": !line.service.allows(d) })).collect::<Vec<_>>(),
        "templates": pd::templates_of(c, name).iter().map(|t| json!({ "id": t.id, "name": t.name, "day": t.day, "day_label": day_name(t.day), "trips": t.trips.len() })).collect::<Vec<_>>(),
    })
}

/// The places in the table the page names (`trips`).
fn trips_arg(args: &Value) -> Vec<usize> {
    args.get("trips").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect()).unwrap_or_default()
}

/// The source of a copy the page names (`source`): the day shown ("day"), the trips chosen there
/// ("trips"), or a template of the line ("template", by its `id`).
fn source_of(c: &Company, name: &str, line: &LineDesign, day: usize, args: &Value) -> Option<Template> {
    let s = args.get("source").cloned().unwrap_or(Value::Null);
    match s.get("kind").and_then(Value::as_str).unwrap_or("day") {
        "trips" => Some(Template::of_trips(line, name, day, &trips_arg(&s))),
        "template" => pd::template(c, s.get("id").and_then(Value::as_u64).unwrap_or(0) as u32).filter(|t| t.line.trim().eq_ignore_ascii_case(name.trim())).cloned(),
        _ => Some(Template::of_day(line, name, day)),
    }
}

/// What a copy did, in words: the trips copied and where, those there already, those of a
/// pattern gone, the days the line does not run.
fn copied_words(made: &[DayCopied]) -> (String, bool) {
    let names = |f: &dyn Fn(&DayCopied) -> bool| made.iter().filter(|m| f(m)).map(|m| day_name(m.day)).collect::<Vec<_>>().join(", ");
    let added: usize = made.iter().map(|m| m.added).sum();
    let removed: usize = made.iter().map(|m| m.removed).sum();
    let twins: usize = made.iter().map(|m| m.twins).sum();
    let lost: usize = made.iter().map(|m| m.lost).sum();
    let open = names(&|m| !m.closed);
    let mut text = if open.is_empty() {
        String::new()
    } else if added == 0 && removed == 0 {
        tr("Nothing was copied: those days have every one of these trips already.")
    } else {
        trf("%{n} trips copied to %{days}: Save keeps them, Undo takes them back.", &[("n", &added.to_string()), ("days", &open)])
    };
    if twins > 0 && added > 0 {
        text = format!("{text} {}", trf("%{n} were there already.", &[("n", &twins.to_string())]));
    }
    if lost > 0 {
        text = format!("{text} {}", trf("%{n} of a pattern the line no longer has were left out.", &[("n", &lost.to_string())]));
    }
    let closed = names(&|m| m.closed);
    if !closed.is_empty() {
        text = format!("{text} {}", trf("Not copied to %{days}: the line does not run then.", &[("days", &closed)]));
    }
    (text.trim().to_string(), added == 0 && removed == 0)
}

/// "Copy to…", and the templates kept, renamed and thrown away (`rf_tp_act` "copy_to",
/// "template_save", "template_rename", "template_drop"). A copy goes into the line's draft, as
/// every change of the Timeplan does: Save keeps it, Undo takes it back.
fn copy_act(s: &mut Session, r: &mut Rf, c: &Company, what: &str, day: usize, args: &Value) -> Value {
    let v = &mut r.tp;
    let Some(line) = v.draft().cloned() else { return json!({}) };
    let name = v.line.clone();
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    match what {
        "copy_to" => {
            let Some(src) = source_of(c, &name, &line, day, args) else { return said(tr("The template is no longer there."), true) };
            let days: Vec<usize> = args.get("days").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect()).unwrap_or_default();
            if days.is_empty() {
                return said(tr("Choose the days to copy to."), true);
            }
            if src.trips.is_empty() {
                return said(tr("There are no trips to copy."), true);
            }
            let how = if str_arg(args, "how") == "add" { How::Add } else { How::Replace };
            let mut made = Vec::new();
            v.change(|l| made = pd::copy_table(l, &src, &days, how));
            let (text, idle) = copied_words(&made);
            let mut out = said(text, idle);
            out["copied"] = json!(!idle);
            out
        }
        "template_save" => {
            let src = source_of(c, &name, &line, day, args).unwrap_or_default();
            let words = str_arg(args, "name");
            if words.trim().is_empty() {
                return said(tr("Give the template a name."), true);
            }
            if src.trips.is_empty() {
                return said(tr("There are no trips to copy."), true);
            }
            let n = src.trips.len();
            match company::act(s, |c| Ok(pd::save_template(c, src, &words))) {
                Ok(Some(id)) => {
                    let mut out = said(trf("Template “%{name}” kept: %{n} trips, to copy onto any day.", &[("name", words.trim()), ("n", &n.to_string())]), false);
                    out["id"] = json!(id);
                    out
                }
                _ => json!({}),
            }
        }
        "template_rename" => {
            let words = str_arg(args, "name");
            match company::act(s, |c| Ok(pd::rename_template(c, id, &words))) {
                Ok(true) => json!({}),
                _ => said(tr("Give the template a name."), true),
            }
        }
        "template_drop" => {
            let gone = pd::template(c, id).map(|t| t.name.clone()).unwrap_or_default();
            match company::act(s, |c| Ok(pd::drop_template(c, id))) {
                Ok(true) => said(trf("Template “%{name}” thrown away.", &[("name", &gone)]), false),
                _ => json!({}),
            }
        }
        _ => json!({}),
    }
}

/// The trips chosen together (`trips`): taken out, moved, given another pattern, their times
/// from the running times again (`rf_tp_act` "bulk_*") - one change, one Undo.
fn bulk_act(v: &mut Tp, line: &LineDesign, sh: &Shown, what: &str, args: &Value) -> Value {
    let trips = trips_arg(args);
    if trips.is_empty() {
        return said(tr("Choose a trip first: click its number."), true);
    }
    let day = sh.day;
    match what {
        "bulk_delete" => {
            let mut n = 0;
            v.change(|l| n = tp::delete_trips(l, &trips));
            said(trf("%{n} trips taken out.", &[("n", &n.to_string())]), false)
        }
        "bulk_shift" => {
            let by = args.get("by").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            v.change(|l| tp::shift_trips(l, &trips, by.clamp(-1440.0, 1440.0)));
            json!({})
        }
        "bulk_pattern" => {
            let side = tp::sides(line)[sh.side].clone();
            let Some(&dir) = side.patterns.get(usize_arg(args, "pattern")) else { return json!({}) };
            let mine: Vec<usize> = trips.into_iter().filter(|&i| line.table.get(i).is_some_and(|t| side.patterns.contains(&t.dir))).collect();
            let set = tp::run_set(line, day, dir);
            let mut n = 0;
            v.change(|l| n = tp::set_pattern(l, &mine, dir, &set));
            said(trf("%{n} trips run pattern %{code} now.", &[("n", &n.to_string()), ("code", &tp::pattern_code(side.pattern(dir).unwrap_or(0)))]), false)
        }
        "bulk_revert" => {
            let sets: Vec<(usize, RunSet)> = trips.iter().filter_map(|&i| line.table.get(i).map(|t| (i, tp::run_set(line, day, t.dir)))).collect();
            v.change(|l| {
                for (i, set) in &sets {
                    tp::revert(l, *i, set);
                }
            });
            json!({})
        }
        _ => json!({}),
    }
}

// --- what the page does ------------------------------------------------------------------------------

fn said(text: impl Into<String>, error: bool) -> Value {
    json!({ "say": { "text": text.into(), "error": error } })
}

/// The direction of the running times shown.
fn shown_dir(line: &LineDesign, sh: &Shown) -> Option<usize> {
    let side = &tp::sides(line)[sh.side];
    side.patterns.get(sh.pattern.min(side.patterns.len().saturating_sub(1))).copied()
}

/// The side a trip of the table is on, and its rows on the day group.
fn rows_of_trip(line: &LineDesign, day: usize, i: usize) -> Option<(Side, Vec<usize>)> {
    let dir = line.table.get(i)?.dir;
    let sides = tp::sides(line);
    let side = if sides[1].patterns.contains(&dir) { sides[1].clone() } else { sides[0].clone() };
    let rows = tp::rows(line, day, &side);
    Some((side, rows))
}

fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let what = str_arg(args, "act");
    let sh = shown_of(args);
    {
        let v = &mut r.tp;
        load_shown(v, &c);
    }
    let v = &mut r.tp;
    match what.as_str() {
        "line" => {
            // (another line: what was changed and not saved is dropped - the page asked first)
            let line = str_arg(args, "line");
            if c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&line)) {
                v.line = line;
                v.loaded = None;
                load_shown(v, &c);
            }
            return Ok(json!({}));
        }
        "drop" => {
            v.loaded = None;
            return Ok(json!({}));
        }
        "undo" => {
            v.undo();
            return Ok(json!({}));
        }
        "redo" => {
            v.redo();
            return Ok(json!({}));
        }
        "from" => {
            let by = args.get("by").and_then(Value::as_i64).unwrap_or(0);
            let days = co::dates::between(&c.date, &v.from);
            if (by < 0 && days > 1) || (by > 0 && days < 60) {
                v.from = co::dates::add(&v.from, by.signum());
            }
            return Ok(json!({}));
        }
        "save" => return Ok(save(s, r, sh.day)),
        "copy_to" | "template_save" | "template_rename" | "template_drop" => return Ok(copy_act(s, r, &c, &what, sh.day, args)),
        "refresh" => {
            // (a day's planning refreshes a line of its own: `line`)
            let line = Some(str_arg(args, "line")).filter(|l| !l.trim().is_empty());
            return Ok(refresh(s, r, line));
        }
        _ => {}
    }
    let Some(line) = v.draft().cloned() else { return Ok(json!({})) };
    let day = sh.day;
    let text = str_arg(args, "text");
    let cell = args.get("cell").cloned().unwrap_or(Value::Null);
    let ck = cell.get("kind").and_then(Value::as_str).unwrap_or("");
    let (band, seg, i, stop) = (usize_arg(&cell, "band"), usize_arg(&cell, "seg"), usize_arg(&cell, "i"), usize_arg(&cell, "s"));
    Ok(match what.as_str() {
        // a cell typed (Enter or Tab)
        "apply" => match ck {
            "run" => match tp::parse_minutes(&text) {
                Some(m) => {
                    if let Some(dir) = shown_dir(&line, &sh) {
                        v.change_run(day, dir, |s| {
                            if let Some(x) = s.bands.get_mut(band).and_then(|b| b.run.get_mut(seg)) {
                                *x = m;
                            }
                        });
                    }
                    json!({})
                }
                None => said(tr("Type the minutes, such as 7"), true),
            },
            "band_from" => match tp::parse_time(&text) {
                Some(t) => {
                    if let Some(dir) = shown_dir(&line, &sh) {
                        v.change_run(day, dir, |s| {
                            s.move_band(band, t);
                        });
                    }
                    json!({})
                }
                None => said(tr("Type a time such as 07:45 (up to 36:00)"), true),
            },
            "start" | "headway" => {
                let Some((_, rows)) = rows_of_trip(&line, day, i) else { return Ok(json!({})) };
                let Some(k) = rows.iter().position(|x| *x == i) else { return Ok(json!({})) };
                if ck == "start" {
                    match tp::parse_time(&text) {
                        Some(t) => {
                            let t = near_time(t, tp::departure(&line, i));
                            v.change(|l| tp::set_start(l, &rows, k, t));
                            json!({})
                        }
                        None => said(tr("Type a time such as 07:45 (up to 36:00)"), true),
                    }
                } else {
                    match tp::parse_minutes(&text).filter(|m| *m >= 0.0) {
                        Some(m) => {
                            v.change(|l| tp::set_headway(l, &rows, k, m));
                            json!({})
                        }
                        None => said(tr("Type the minutes, such as 7"), true),
                    }
                }
            }
            "time" => match tp::parse_time(&text) {
                Some(t) => {
                    let at = tp::trip(&line, i).get(stop).copied().unwrap_or(t);
                    let t = near_time(t, at);
                    v.change(|l| tp::set_cell(l, i, stop, t));
                    json!({})
                }
                None => said(tr("Type a time such as 07:45 (up to 36:00)"), true),
            },
            _ => json!({}),
        },
        // a cell a minute later or earlier (Up, Down)
        "nudge" => {
            let by = args.get("by").and_then(Value::as_f64).unwrap_or(1.0) as f32;
            match ck {
                "run" => {
                    if let Some(dir) = shown_dir(&line, &sh) {
                        v.change_run(day, dir, |s| {
                            if let Some(x) = s.bands.get_mut(band).and_then(|b| b.run.get_mut(seg)) {
                                *x = (*x + by).max(0.0);
                            }
                        });
                    }
                }
                "band_from" => {
                    if let Some(dir) = shown_dir(&line, &sh) {
                        v.change_run(day, dir, |s| {
                            let from = s.bands.get(band).map(|b| b.from).unwrap_or(0.0);
                            s.move_band(band, from + by * 5.0);
                        });
                    }
                }
                "start" | "headway" => {
                    if let Some((_, rows)) = rows_of_trip(&line, day, i) {
                        if let Some(k) = rows.iter().position(|x| *x == i) {
                            let t = tp::departure(&line, i) + by;
                            v.change(|l| tp::set_start(l, &rows, k, t));
                        }
                    }
                }
                "time" => {
                    let t = tp::trip(&line, i).get(stop).copied().unwrap_or(0.0) + by;
                    v.change(|l| tp::set_cell(l, i, stop, t));
                }
                _ => {}
            }
            json!({})
        }
        "band_add" => {
            let Some(dir) = shown_dir(&line, &sh) else { return Ok(json!({})) };
            let set = tp::run_set(&line, day, dir);
            let t = match args.get("at").and_then(Value::as_f64) {
                Some(t) => t as f32,
                None => {
                    // (the end of the rail: two hours after the last, the morning first)
                    let last_from = set.bands.last().map(|b| b.from).unwrap_or(0.0);
                    if last_from < 7.0 * 60.0 {
                        7.0 * 60.0
                    } else {
                        (last_from + 120.0).min(tp::DAY_SPAN - 60.0)
                    }
                }
            };
            v.change_run(day, dir, |s| {
                s.add_band(t);
            });
            let k = v.draft().map(|d| tp::run_set(d, day, dir)).and_then(|s| s.bands.iter().position(|b| (b.from - t).abs() < 0.5));
            json!({ "band": k })
        }
        "band_remove" => {
            if let Some(dir) = shown_dir(&line, &sh) {
                let k = usize_arg(args, "k");
                v.change_run(day, dir, |s| {
                    s.remove_band(k);
                });
            }
            json!({})
        }
        "timepoint" => {
            if let Some(dir) = shown_dir(&line, &sh) {
                let k = usize_arg(args, "k");
                let d = line.directions[dir].clone();
                v.change(|line| {
                    let mut plan = tp::plan_of(line);
                    plan.toggle_timepoint(dir, &d, k);
                    line.timeplan = Some(plan);
                });
            }
            json!({})
        }
        "add_one" => {
            let side = tp::sides(&line)[sh.side].clone();
            if side.patterns.is_empty() {
                return Ok(json!({}));
            }
            let rows = tp::rows(&line, day, &side);
            let sel = args.get("sel").and_then(Value::as_u64).map(|x| x as usize);
            let dir = sel.and_then(|i| line.table.get(i)).map(|t| t.dir).filter(|d| side.patterns.contains(d)).unwrap_or(side.patterns[0]);
            let p = side.pattern(dir).unwrap_or(0);
            let mine: Vec<usize> = rows.iter().copied().filter(|&i| line.table[i].dir == dir).collect();
            let after = sel.filter(|i| mine.contains(i)).or(mine.last().copied());
            let headway = match after.and_then(|a| mine.iter().position(|x| *x == a)) {
                Some(k) if k > 0 => tp::departure(&line, mine[k]) - tp::departure(&line, mine[k - 1]),
                _ => 15.0,
            };
            let dep = after.map(|a| tp::departure(&line, a) + headway.max(1.0)).unwrap_or(tp::FIRST_TRIP);
            let set = tp::run_set(&line, day, side.patterns[p]);
            let mut added = None;
            v.change(|l| added = tp::add_trip(l, day, dir, dep, &set));
            json!({ "sel": added })
        }
        "add_many" => {
            let side = tp::sides(&line)[sh.side].clone();
            let pattern = usize_arg(args, "pattern").min(side.patterns.len().saturating_sub(1));
            let Some(&dir) = side.patterns.get(pattern) else { return Ok(json!({})) };
            let first = tp::parse_time(&str_arg(args, "first"));
            // (an end before the start: after midnight)
            let last = tp::parse_time(&str_arg(args, "last")).map(|t| first.map_or(t, |f| tp::end_after(f, t)));
            let heads = tp::parse_headways(&str_arg(args, "headway"));
            let set = tp::run_set(&line, day, dir);
            let n = line.directions[dir].stops.len();
            let control = usize_arg(args, "control").min(n.saturating_sub(1));
            match (first, last, &heads) {
                (Some(f), Some(t), Some(h)) if t >= f => {
                    let made = tp::generate(&set, n, control, f, t, h);
                    let count = made.len();
                    v.change(|l| {
                        tp::add_generated(l, day, dir, made);
                    });
                    json!({ "added": count, "say": { "text": trf("%{n} trips added.", &[("n", &count.to_string())]), "error": false } })
                }
                _ => said(tr("Type the times as 06:00 and the headway in minutes"), true),
            }
        }
        "delete" => {
            let i = usize_arg(args, "i");
            v.change(|l| {
                tp::delete_trip(l, i);
            });
            json!({})
        }
        "bulk_delete" | "bulk_shift" | "bulk_pattern" | "bulk_revert" => bulk_act(v, &line, &sh, &what, args),
        "revert" => {
            let i = usize_arg(args, "i");
            if let Some((side, _)) = rows_of_trip(&line, day, i) {
                let p = line.table.get(i).and_then(|t| side.pattern(t.dir)).unwrap_or(0);
                let set = tp::run_set(&line, day, side.patterns[p]);
                v.change(|l| tp::revert(l, i, &set));
            }
            json!({})
        }
        other => return Err(format!("no action {other}")),
    })
}

/// "UNSYNCED – Refresh": the scheduling studio makes the line's blocks and duties anew on the
/// day groups saved since (`co::timeplan::resync`).
fn refresh(s: &mut Session, r: &mut Rf, line: Option<String>) -> Value {
    let v = &mut r.tp;
    let name = line.unwrap_or_else(|| v.line.clone());
    let days: Vec<usize> = v.unsynced.iter().filter(|u| u.0.eq_ignore_ascii_case(&name)).map(|u| u.1).collect();
    if days.is_empty() {
        return json!({});
    }
    let number = s.company.as_ref().and_then(|c| c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&name)).map(|l| l.number.trim().to_string())).unwrap_or_default();
    if company::act(s, |c| {
        // (the days copied from another that were in step stay so: both lose the line's alike)
        let wds: Vec<u8> = days.iter().flat_map(|d| tp::weekdays(*d)).collect();
        let in_step: Vec<u8> = co::plandays::in_step(c).into_iter().filter(|&d| co::plandays::follow_of(c, d).is_some_and(|f| wds.contains(&f.day) && wds.contains(&f.pattern))).collect();
        for d in &days {
            tp::resync(c, &name, *d);
        }
        co::plandays::restep(c, &in_step);
        Ok(())
    })
    .is_err()
    {
        return json!({});
    }
    let v = &mut r.tp;
    v.unsynced.retain(|u| !u.0.eq_ignore_ascii_case(&name));
    // (its blocks and duties made: nothing more to say of what to do next)
    if v.hint.as_ref().is_some_and(|h| h.0.eq_ignore_ascii_case(&name)) {
        v.hint = None;
    }
    company::reload_timetable(s);
    r.cache = None;
    said(trf("Line %{n}: the scheduling studio makes its blocks and duties anew.", &[("n", &number)]), false)
}

// --- saving ------------------------------------------------------------------------------------------

/// The trips of each day group, to see which changed.
fn day_trips(l: &LineDesign, day: usize) -> Vec<(usize, Vec<i32>)> {
    let mut v: Vec<(usize, Vec<i32>)> = l.table.iter().filter(|t| tp::in_day(l, t, day)).map(|t| (t.dir, reg::trip_times(l, t).iter().map(|x| x.round() as i32).collect())).collect();
    v.sort();
    v
}

/// What a change paid for its route's approval, said after it.
fn approved(paid: co::Cents) -> String {
    if paid > 0 {
        format!(" {}", trf("Its route is approved for %{amount}.", &[("amount", &eur(paid))]))
    } else {
        String::new()
    }
}

/// What came of saving an own line's timetable (`plan_edit::Commit`).
pub(crate) enum Commit {
    Saved,
    Gone,
    Kept,
    Stale,
}

/// The timeplan saved as the company's lines' timetables are, then the line read again and
/// marked UNSYNCED for the scheduling studio on the day groups it changed (`timeplan::save`).
/// Returns what the page shows.
fn save(s: &mut Session, r: &mut Rf, day_shown: usize) -> Value {
    let Some(w) = r.tp.work.clone() else { return json!({}) };
    for d in 0..DAY_GROUPS.len() {
        if let Some(e) = tp::issues(&w.own.draft, d).into_iter().find(|i| i.level == Level::Error) {
            let text = trf("%{day}: %{issue} - mend it first.", &[("day", &day_name(d)), ("issue", &tr(e.text).replace("%{n}", &e.n.to_string()))]);
            return json!({ "say": { "text": text, "error": true }, "day": d, "issues_open": true });
        }
    }
    let _ = day_shown;
    let changed: Vec<usize> = (0..DAY_GROUPS.len()).filter(|&d| day_trips(&w.own.base, d) != day_trips(&w.own.draft, d)).collect();
    let mut out = json!({});
    let saved = if w.map {
        save_map(s, &w, &mut out)
    } else {
        let (company_id, line) = (r.tp.company.clone(), r.tp.line.clone());
        let mut from = r.tp.from.clone();
        let c = commit(s, &company_id, &line, &w.own, &mut from, &mut out);
        r.tp.from = from;
        match c {
            Commit::Saved => true,
            Commit::Stale | Commit::Gone => {
                r.tp.loaded = None;
                false
            }
            Commit::Kept => false,
        }
    };
    if saved {
        // (the line under its new name, if its number changed it: its templates with it)
        let id = w.own.id;
        if let Some(name) = s.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.own && x.plan.as_ref().is_some_and(|p| p.line_id == id)).map(|x| x.name.clone())) {
            let was = r.tp.line.clone();
            if !was.eq_ignore_ascii_case(&name) {
                let _ = company::act(s, |c| {
                    pd::move_templates(c, &was, &name);
                    Ok(())
                });
            }
            r.tp.line = name;
        }
        let v = &mut r.tp;
        for d in changed {
            if !v.unsynced.iter().any(|u| u.0.eq_ignore_ascii_case(&v.line) && u.1 == d) {
                v.unsynced.push((v.line.clone(), d));
            }
        }
        v.loaded = None;
        r.cache = None;
    }
    out["saved"] = json!(saved);
    out
}

fn say_into(out: &mut Value, text: String, error: bool) {
    out["say"] = json!({ "text": text, "error": error });
}

/// The draft of an own line saved as the line editor saves a line's timetable
/// (`plan_edit::commit`): a line in service from the day `from` (moved to tomorrow when it is
/// sooner), else at once - its changed tours' buses and drivers taken off the roster.
/// (Omsi-Hub 6/6: the planning's sheet of a line's timetable saves through it too.)
pub(crate) fn commit(s: &mut Session, company_id: &str, line: &str, o: &Own, from: &mut String, out: &mut Value) -> Commit {
    let Some(c) = s.company.clone().filter(|c| c.id == company_id) else { return Commit::Gone };
    let Some(cl) = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)).cloned() else { return Commit::Gone };
    if let Some(p) = reg::problems(&o.draft).first() {
        say_into(out, tr(p.text).replace("%{n}", &p.n.to_string()), true);
        return Commit::Kept;
    }
    // (the line as the registry has it now: changed meanwhile, it is shown as it is)
    let r = reg::load_registry(&o.path);
    let Some(on_disk) = r.line(o.id).cloned() else {
        say_into(out, tr(NOT_FOUND), true);
        return Commit::Gone;
    };
    if on_disk != o.saved {
        out["popup"] = company::popup(s, Popup::new("departure_board", "Changed meanwhile", tr("The line's timetable was changed meanwhile - in the line editor, or its new timetable began: it is shown as it is now."), "", None));
        return Commit::Stale;
    }
    let Some(lines) = s.map_lines().map(|x| x.to_vec()) else {
        out["popup"] = company::popup(s, Popup::new("timer", "Not yet", tr("The timetable of the day is still being read."), tr("Try again in a moment."), None));
        return Commit::Kept;
    };
    if core::content_dir().is_none() {
        say_into(out, tr("There is no content folder to write the timetable into."), true);
        return Commit::Kept;
    }
    let sl = ownline::StopLines::of(&lines, &o.stem);
    let shape = ownline::shape_of(&o.draft, &|st| sl.count(st));
    let diff = reg::timetable_differs(&o.live, &o.draft).then(|| reg::tour_diff(&o.live, &o.draft));
    let number = o.draft.number.trim().to_string();
    let (draft, stem) = (o.draft.clone(), o.stem.clone());
    let (to_write, text) = match &diff {
        // a line in service: from the day chosen, today's tours as they are
        Some(d) if cl.service_from.is_some() => {
            if co::dates::between(&c.date, from) < 1 {
                *from = co::dates::add(&c.date, 1);
            }
            let f = from.clone();
            let replan = d.replan();
            let paid = match company::act(s, |c| ownline::schedule_change(c, &draft, &stem, &shape, &f, replan)) {
                Ok(p) => p,
                Err(p) => {
                    out["popup"] = p;
                    return Commit::Kept;
                }
            };
            let t = trf("Line %{n}: the new timetable begins on %{date}; today's tours run as they are.", &[("n", &number), ("date", &day_label(&f))]) + &approved(paid);
            (LineDesign { live: Some(Box::new(o.live.clone())), pending_from: f, draft: false, ..draft }, t)
        }
        // else at once: the tours it changed get their buses and drivers anew
        _ => {
            let replan = diff.as_ref().map(|d| d.replan()).unwrap_or_default();
            let anew = replan.len() + diff.as_ref().map(|d| d.added.len()).unwrap_or(0);
            let (d2, s2) = (draft.clone(), stem.clone());
            let paid = match company::act(s, |c| {
                let paid = ownline::apply_change(c, &d2, &s2, &shape)?;
                co::plan::forget_tours(c, &s2, &replan);
                Ok(paid)
            }) {
                Ok(p) => p,
                Err(p) => {
                    out["popup"] = p;
                    return Commit::Kept;
                }
            };
            let t = if anew > 0 { trf("Line %{n} saved: %{k} of its tours to plan anew.", &[("k", &anew.to_string())]) } else { tr("Line %{n} saved.") }.replace("%{n}", &number) + &approved(paid);
            (LineDesign { live: None, pending_from: String::new(), draft: false, ..draft }, t)
        }
    };
    let result = write_line(&c.map, &o.path, to_write);
    company::reload_timetable(s);
    match result {
        Ok(None) => say_into(out, text, false),
        Ok(Some(depots)) => say_into(out, format!("{text} {}: {depots}", tr("A depot file could not be written (displays and IBIS)")), true),
        Err(err) => say_into(out, format!("{}: {err}", tr("The timetable files could not be written")), true),
    }
    Commit::Saved
}

/// A line of the map's the company runs, saved (`timeplan::save_map`): its changed route paid
/// for (`ownline::apply_map_change`), the registry and the timetable written.
fn save_map(s: &mut Session, w: &Work, out: &mut Value) -> bool {
    let o = &w.own;
    if let Some(p) = reg::problems(&o.draft).first() {
        say_into(out, tr(p.text).replace("%{n}", &p.n.to_string()), true);
        return false;
    }
    let Some(ttl) = o.draft.map.as_ref().map(|m| m.ttl.clone()) else { return false };
    let mut r = reg::load_registry(&o.path);
    // (imported for the Timeplan: still none in the registry, else as the registry has it)
    let changed_meanwhile = if w.fresh {
        r.map_line(&ttl).is_some()
    } else {
        match r.line(o.id) {
            Some(on_disk) => *on_disk != o.saved,
            None => {
                say_into(out, tr(NOT_FOUND), true);
                return false;
            }
        }
    };
    if changed_meanwhile {
        out["popup"] = company::popup(s, Popup::new("departure_board", "Changed meanwhile", tr("The line's timetable was changed meanwhile - in the line editor, or its new timetable began: it is shown as it is now."), "", None));
        return false;
    }
    let Some(map) = s.company.as_ref().map(|c| c.map.clone()) else { return false };
    let (old, mut new) = (o.live.clone(), o.draft.clone());
    let paid = match company::act(s, |c| ownline::apply_map_change(c, &ttl, &old, &new, false)) {
        Ok(p) => p,
        Err(p) => {
            out["popup"] = p;
            return false;
        }
    };
    // (the imported line into the registry, under an id of its own, before it is written)
    if w.fresh {
        new.id = r.add_map_line(new.clone()).id;
        if let Err(e) = reg::save_registry(&o.path, &r) {
            say_into(out, format!("{}: {e}", tr("The timetable files could not be written")), true);
            return false;
        }
    }
    let number = new.number.trim().to_string();
    let result = write_line(&map, &o.path, LineDesign { live: None, pending_from: String::new(), ..new });
    company::reload_timetable(s);
    let text = tr("Line %{n} saved.").replace("%{n}", &number) + &approved(paid);
    match result {
        Ok(None) => say_into(out, text, false),
        Ok(Some(depots)) => say_into(out, format!("{text} {}: {depots}", tr("A depot file could not be written (displays and IBIS)")), true),
        Err(err) => say_into(out, format!("{}: {err}", tr("The timetable files could not be written")), true),
    }
    true
}

/// A line written into the map's registry, its timetable and its depot files as the line editor
/// writes them (`plan_edit::write_line`). Returns a depot file's error.
fn write_line(map: &str, path: &Path, mut line: LineDesign) -> Result<Option<String>, String> {
    let content = core::content_dir().ok_or_else(|| "no content folder".to_string())?;
    let mut r = reg::load_registry(path);
    let id = line.id;
    line.modified = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(x) = r.line_mut(id) else { return Err(tr(NOT_FOUND)) };
    *x = line;
    let global = global_of(map);
    let map_dir = global.parent().map(Path::to_path_buf).unwrap_or_default();
    let groups = linehof::group_depots(&map_dir);
    let (bases, original) = core::depot_roots();
    let plan = linehof::prepare(&mut r, &groups, &bases);
    owndepot::assign(&mut r, &owndepot::dir(), &groups, &bases);
    reg::save_registry(path, &r)?;
    let exported = reg::export_to_map(&content, &map_dir, &r);
    let (_, err) = linehof::write(&content, original.as_deref(), &r, &groups, &plan);
    let (_, own_err) = owndepot::write_lines(&r, &owndepot::dir(), Some(&content));
    let err = err.or(own_err);
    if err.is_none() && r.texts < reg::TEXTS_VERSION {
        r.texts = reg::TEXTS_VERSION;
        if let Err(e) = reg::save_registry(path, &r) {
            core::log_to_file(&format!("rosterflow: the registry {} not saved after its depot files: {e}", path.display()));
        }
    }
    omsi_cfg::content_changed();
    exported?;
    Ok(err)
}

/// Put line `line` back as a lesson found it (`timeplan::restore`): changes not saved dropped,
/// a timetable saved meanwhile saved back. Returns false when it could not be saved back.
pub(crate) fn restore(s: &mut Session, r: &mut Rf, line: &str, design: &LineDesign, unsynced: Vec<(String, usize)>) -> bool {
    let Some(c) = s.company.clone() else { return false };
    r.tp.company = c.id.clone();
    r.tp.line = line.to_string();
    r.tp.loaded = None;
    load_shown(&mut r.tp, &c);
    let mut ok = true;
    if let Some(w) = r.tp.work.as_mut().filter(|w| w.own.base != *design) {
        w.own.draft = design.clone();
        let out = save(s, r, 0);
        ok = out.get("saved").and_then(Value::as_bool).unwrap_or(false);
    }
    r.tp.unsynced = unsynced;
    r.tp.loaded = None;
    ok
}

/// Every English text the Timeplan's commands say (for the translations' check).
#[cfg_attr(not(test), allow(dead_code))]
pub fn texts() -> Vec<&'static str> {
    vec![
        MAP_NOT_EDITED,
        NO_LINE,
        MAP_LINE_HINT,
        NEW_LINE_HINT,
        NOT_FOUND,
        "All",
        "Save",
        "Saved",
        "From",
        "Error",
        "Warning",
        "Line",
        "Line %{n}: %{k}",
        "A new timetable waits for %{date}: it is the one shown, against the tours that run until then.",
        "The line has no inbound direction.",
        "This pattern has fewer than two stops: give it its stops in the line editor.",
        "From the running times: %{time}",
        "Type the minutes, such as 7",
        "Type a time such as 07:45 (up to 36:00)",
        "%{n} trips added.",
        "Type the times as 06:00 and the headway in minutes",
        "Line %{n}: the scheduling studio makes its blocks and duties anew.",
        "%{day}: %{issue} - mend it first.",
        "Its route is approved for %{amount}.",
        "Changed meanwhile",
        "The line's timetable was changed meanwhile - in the line editor, or its new timetable began: it is shown as it is now.",
        "Not yet",
        "The timetable of the day is still being read.",
        "Try again in a moment.",
        "There is no content folder to write the timetable into.",
        "Line %{n}: the new timetable begins on %{date}; today's tours run as they are.",
        "Line %{n} saved: %{k} of its tours to plan anew.",
        "Line %{n} saved.",
        "A depot file could not be written (displays and IBIS)",
        "The timetable files could not be written",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_typed_for_a_late_trip_is_after_midnight() {
        assert_eq!(near_time(5.0, 1430.0), 1445.0);
        assert_eq!(near_time(425.0, 420.0), 425.0);
        assert_eq!(near_time(800.0, 2000.0), 800.0);
    }

    #[test]
    fn the_vehicle_types_are_short() {
        let mut l = LineDesign::default();
        l.vehicles.classes = vec![VehicleClass::Solo, VehicleClass::Double];
        assert_eq!(vehicle_types(&l), "SD,DD");
    }

    #[test]
    fn a_new_line_opens_on_its_running_times_and_a_map_line_on_its_timetable() {
        let mut v = Tp::default();
        assert_eq!(v.guide("c", "oo_5", Some(MAP_LINE_HINT)), Some(TIMETABLE));
        assert_eq!(v.guide("c", "oo_6", Some(NEW_LINE_HINT)), Some(RUNNING));
        assert_eq!(v.guide("c", "oo_6", None), None);
        assert_eq!(v.hint_shown(), Some(NEW_LINE_HINT));
    }
}

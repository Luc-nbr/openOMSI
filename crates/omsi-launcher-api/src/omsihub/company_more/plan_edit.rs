//! A line's trips and timetable changed on the planning (Trello "Tauri 6/6"), as our
//! game-drawn launcher's sheet over its planning (`company/planning/plan_edit.rs`; Luc: "from
//! the bus company's planning menu you must be able to change the line's trips and timetable in
//! an intuitive way"): opened from a line's heading or one of its tours when the RosterFlow
//! add-on is off (with it on, the line goes to RosterFlow's Timeplan). A group of days at a
//! time, its departures lie per direction on a time axis - one dragged to another minute, the
//! axis clicked to add one, the one chosen typed, moved or taken out - and in a list; the
//! line's pattern is changed in one go; under the departures, on the same hours, the tours
//! they make against those that run now, and what the line asks of the fleet.
//!
//! Saved as the line editor saves a timetable (RosterFlow's Timeplan's `commit`, the same
//! port of `plan_edit::commit`): an own line in service from a day the player chooses,
//! tomorrow at the soonest, else at once. A line of the map's timetable shows its trips as
//! they run. What the sheet changes (the line's draft, undo, the departure chosen, the day a
//! change begins) is kept here, in this part's state; the page keeps how it is looked at (the
//! hours zoomed into, a drag under way, a time being typed).

use super::super::lines::plate;
use super::super::words::{day_label, grouped, tr, trf};
use super::super::{plan_of, Session};
use crate::omsihub::rosterflow::timeplan::{self as tpl, Commit, Own};
use omsi_launcher_lib::company::network::TourOfDay;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan};
use omsi_launcher_lib::company::{self as co, ownline, Company, CompanyLine};
use omsi_launcher_lib::lines::{self as reg, DayPattern, LineDesign, TableTrip, TimeBand, TourDiff, DAY_GROUPS};
use serde_json::{json, Value};
use std::time::Instant;

/// Why a line's trips are only shown (`MAP_LINE`, `NOT_FOUND`).
const MAP_LINE: &str = "A line of the map's timetable: its trips are the map's own and are not changed here.";
const NOT_FOUND: &str = "The line was not found among the line editor's lines of the company's map: its trips are shown as they run.";

/// "07:05" (past midnight from 00:00 on).
fn hm(minutes: f32) -> String {
    let m = minutes.round() as i32;
    format!("{:02}:{:02}", m.div_euclid(60).rem_euclid(24), m.rem_euclid(60))
}

// --- the line's departures, and changing them ------------------------------------------------

/// A departure of the group of days shown (`plan_edit::Dep`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Dep {
    dir: usize,
    dep: f32,
    arr: f32,
    at: Option<usize>,
}

/// A departure chosen: its direction and minute, and its trip in the table.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Pick {
    dir: usize,
    dep: f32,
    at: Option<usize>,
}

impl Dep {
    fn pick(&self) -> Pick {
        Pick { dir: self.dir, dep: self.dep, at: self.at }
    }

    fn is(&self, p: &Pick) -> bool {
        p.dir == self.dir
            && match (p.at, self.at) {
                (Some(a), Some(b)) => a == b,
                _ => (p.dep - self.dep).abs() < 0.5,
            }
    }
}

fn dirs(l: &LineDesign) -> usize {
    l.directions.iter().filter(|d| d.stops.len() >= 2).count().min(2)
}

/// Every departure of a group of days in order of the minute it leaves (`plan_edit::deps`).
fn deps(l: &LineDesign, day: usize) -> Vec<Dep> {
    let n = dirs(l);
    let mut out = Vec::new();
    if !l.service.allows(day) {
        return out;
    }
    if l.table_on {
        for (i, t) in l.table.iter().enumerate().filter(|(_, t)| t.dir < n && t.day == day && !t.times.is_empty()) {
            let times = reg::trip_times(l, t);
            let dep = times.first().copied().unwrap_or(0.0);
            out.push(Dep { dir: t.dir, dep, arr: times.last().copied().unwrap_or(dep).max(dep + 1.0), at: Some(i) });
        }
    } else if l.days.get(day).is_some() {
        let run = reg::run_minutes(l);
        for dir in 0..n {
            let Some(p) = l.pattern(dir, day) else { continue };
            for (dep, _) in p.departures() {
                out.push(Dep { dir, dep, arr: dep + run.get(dir).copied().unwrap_or(1.0), at: None });
            }
        }
    }
    out.sort_by(|a, b| a.dep.total_cmp(&b.dep).then(a.dir.cmp(&b.dir)));
    out
}

fn as_table(l: &mut LineDesign) {
    if !l.table_on {
        l.table = reg::table_from_patterns(l);
        l.table_on = true;
    }
}

fn trip_of(l: &LineDesign, day: usize, p: Pick) -> Option<usize> {
    if let Some(i) = p.at.filter(|&i| l.table.get(i).is_some_and(|t| t.dir == p.dir && t.day == day)) {
        return Some(i);
    }
    l.table.iter().position(|t| t.dir == p.dir && t.day == day && (t.departure() - p.dep).abs() < 0.5)
}

fn move_dep(l: &mut LineDesign, day: usize, p: Pick, to: f32) -> Option<Pick> {
    as_table(l);
    let i = trip_of(l, day, p)?;
    let t = &mut l.table[i];
    t.shift(to.max(0.0).round() - t.departure());
    Some(Pick { dir: p.dir, dep: t.departure(), at: Some(i) })
}

fn add_dep(l: &mut LineDesign, day: usize, dir: usize, at: f32) -> Option<Pick> {
    let stops = l.directions.get(dir).map(|d| d.stops.len()).filter(|n| *n >= 2)?;
    as_table(l);
    let at = at.max(0.0).round();
    let near = l.table.iter().filter(|t| t.dir == dir && t.day == day && t.times.len() == stops).min_by(|a, b| (a.departure() - at).abs().total_cmp(&(b.departure() - at).abs()));
    let rel = near.map(TableTrip::relative).unwrap_or_else(|| reg::fitted_times(&l.directions[dir]));
    l.table.push(TableTrip { dir, day, times: rel.iter().map(|x| at + x).collect(), tour: String::new() });
    Some(Pick { dir, dep: at, at: Some(l.table.len() - 1) })
}

fn remove_dep(l: &mut LineDesign, day: usize, p: Pick) -> bool {
    as_table(l);
    match trip_of(l, day, p) {
        Some(i) => {
            l.table.remove(i);
            true
        }
        None => false,
    }
}

/// The pattern direction `dir` leaves by changed (`plan_edit::set_dir_pattern`).
fn set_dir_pattern(l: &mut LineDesign, day: usize, dir: usize, f: impl FnOnce(&mut DayPattern)) {
    let moves: Vec<usize> = if dir == 1 || l.back_own() { vec![dir] } else { vec![0, 1] };
    let pats = l.patterns_mut(dir);
    let before: Vec<f32> = pats[day].departures().iter().map(|d| d.0).collect();
    f(&mut pats[day]);
    let after: Vec<f32> = pats[day].departures().iter().map(|d| d.0).collect();
    if l.table_on && before != after {
        l.refill_table(day, &moves);
    }
}

// --- what the line comes to -------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Needs {
    trips: usize,
    tours: usize,
    buses: usize,
    duties: usize,
    km: f64,
}

fn needs(tours: &[TourOfDay]) -> Needs {
    Needs {
        trips: tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty).count(),
        tours: tours.len(),
        buses: ownline::busiest(tours),
        duties: tours.iter().map(|t| co::network::duties_of(t).len()).sum(),
        km: tours.iter().map(|t| t.km()).sum(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    Kept,
    Changed,
    New,
    Gone,
}

struct Row {
    number: String,
    fate: Fate,
    bus: Option<String>,
    legs: Vec<(i32, i32, Option<usize>)>,
}

struct Shown {
    lanes: Vec<String>,
    deps: Vec<Dep>,
    rows: Vec<Row>,
    now: Needs,
    then: Needs,
    diff: TourDiff,
    problems: Vec<reg::Problem>,
}

fn bus_of(c: &Company, plan: Option<&DayPlan>, line: &str, tour: &str) -> Option<String> {
    let t = plan?.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(line) && t.tour.tour.trim() == tour.trim())?;
    match t.bus? {
        BusOf::Own(id) => c.vehicle(id).map(|v| v.number.clone()),
        BusOf::Rental => Some(tr("Rental bus")),
    }
}

fn shown_own(c: &Company, cl: &CompanyLine, o: &Own, day: usize, plan: Option<&DayPlan>) -> Shown {
    let lanes = o.draft.directions.iter().filter(|d| d.stops.len() >= 2).take(2).map(|d| d.destination()).collect();
    let now = ownline::design_tours(c, cl, &o.live, &o.stem, day);
    let then = ownline::design_tours(c, cl, &o.draft, &o.stem, day);
    let diff = reg::tour_diff(&o.live, &o.draft);
    let lane = |name: &str| (0..2).find(|k| reg::trip_name(&o.stem, *k) == name);
    let row = |t: &TourOfDay, fate: Fate| Row { number: t.tour.clone(), fate, bus: bus_of(c, plan, &cl.name, &t.tour), legs: t.trips.iter().map(|x| (x.dep, x.arr, if x.empty { None } else { lane(&x.name) })).collect() };
    let fate = |n: &str| {
        if diff.added.iter().any(|x| x == n) {
            Fate::New
        } else if diff.changed.iter().any(|x| x == n) {
            Fate::Changed
        } else {
            Fate::Kept
        }
    };
    let mut rows: Vec<Row> = then.iter().map(|t| row(t, fate(&t.tour))).collect();
    rows.extend(now.iter().filter(|t| diff.gone.contains(&t.tour)).map(|t| row(t, Fate::Gone)));
    Shown { lanes, deps: deps(&o.draft, day), now: needs(&now), then: needs(&then), rows, diff, problems: reg::problems(&o.draft) }
}

fn shown_fixed(c: &Company, line: &str, plan: Option<&DayPlan>) -> Shown {
    let tours: Vec<TourOfDay> = plan.map(|p| p.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(line)).map(|t| t.tour.clone()).collect()).unwrap_or_default();
    let mut ends: Vec<(String, usize)> = Vec::new();
    for x in tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty) {
        match ends.iter_mut().find(|e| e.0 == x.to) {
            Some(e) => e.1 += 1,
            None => ends.push((x.to.clone(), 1)),
        }
    }
    ends.sort_by(|a, b| b.1.cmp(&a.1));
    let mut lanes: Vec<String> = ends.iter().take(2).map(|e| e.0.clone()).collect();
    if ends.len() > 2 {
        lanes.push(String::new());
    }
    let lane = |to: &str| lanes.iter().take(2).position(|x| x == to).unwrap_or(2);
    let mut deps: Vec<Dep> = tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty).map(|x| Dep { dir: lane(&x.to), dep: x.dep as f32, arr: x.arr as f32, at: None }).collect();
    deps.sort_by(|a, b| a.dep.total_cmp(&b.dep).then(a.dir.cmp(&b.dir)));
    let rows = tours.iter().map(|t| Row { number: t.tour.clone(), fate: Fate::Kept, bus: bus_of(c, plan, line, &t.tour), legs: t.trips.iter().map(|x| (x.dep, x.arr, if x.empty { None } else { Some(lane(&x.to)) })).collect() }).collect();
    let n = needs(&tours);
    Shown { lanes, deps, rows, now: n, then: n, diff: TourDiff::default(), problems: Vec::new() }
}

/// The hours the axis shows: the trips' with a little room, six at the least (`span_of`).
fn span_of(s: &Shown) -> (i32, i32) {
    let times = s.deps.iter().map(|d| (d.dep, d.arr)).chain(s.rows.iter().flat_map(|r| r.legs.iter().map(|g| (g.0 as f32, g.1 as f32))));
    let (a, z) = times.fold((f32::MAX, f32::MIN), |(a, z), (x, y)| (a.min(x), z.max(y)));
    if a > z {
        return (5 * 60, 24 * 60);
    }
    let mut t0 = (((a - 20.0) / 60.0).floor() as i32 * 60).max(0);
    let mut t1 = (((z + 20.0) / 60.0).ceil() as i32 * 60).min(28 * 60);
    if t1 - t0 < 6 * 60 {
        t1 = (t0 + 6 * 60).min(28 * 60);
        t0 = (t1 - 6 * 60).max(0);
    }
    (t0, t1)
}

// --- the sheet's state ------------------------------------------------------------------------

/// The sheet over the planning (`plan_edit::PlanEdit`), kept in this part's state.
pub struct PlanEdit {
    company: String,
    line: String,
    date: String,
    tour: Option<String>,
    day: usize,
    own: Option<Own>,
    fixed: Option<&'static str>,
    undo: Vec<LineDesign>,
    last: (String, Option<Instant>),
    sel: Option<Pick>,
    from: String,
    pdir: usize,
}

impl PlanEdit {
    /// The line changed by `f`, the line before kept to be undone - once for a control worked
    /// at a stretch (`what`, within a second).
    fn edit<R>(&mut self, what: &str, f: impl FnOnce(&mut LineDesign) -> R) -> Option<R> {
        let o = self.own.as_mut()?;
        let apart = self.last.1.is_none_or(|t| t.elapsed().as_secs_f32() > 1.0);
        if self.last.0 != what || apart {
            self.undo.push(o.draft.clone());
            if self.undo.len() > 60 {
                self.undo.remove(0);
            }
        }
        self.last = (what.to_string(), Some(Instant::now()));
        Some(f(&mut o.draft))
    }

    fn dirty(&self) -> bool {
        self.own.as_ref().is_some_and(|o| o.draft != o.base)
    }
}

/// The sheet opened for the company's line `line`, from the planning's day `date` (and one of
/// its tours): `plan_edit::open`.
pub fn open(s: &mut Session, line: &str, date: &str, tour: Option<String>) -> Option<PlanEdit> {
    let c = s.company.as_ref()?;
    let cl = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line))?;
    let (own, fixed) = if !cl.own {
        (None, Some(MAP_LINE))
    } else {
        match tpl::load_own(c, cl) {
            Ok(o) => (Some(o), None),
            Err(_) => (None, Some(NOT_FOUND)),
        }
    };
    let tomorrow = co::dates::add(&c.date, 1);
    let from = own.as_ref().map(|o| o.saved.pending_from.clone()).filter(|d| co::dates::between(&c.date, d) >= 1).unwrap_or(tomorrow);
    let mut day = ownline::day_type(date);
    if let Some(o) = &own {
        if !o.draft.service.allows(day) {
            day = (0..DAY_GROUPS.len()).find(|k| o.draft.service.allows(*k)).unwrap_or(0);
        }
    }
    Some(PlanEdit { company: c.id.clone(), line: cl.name.clone(), date: date.to_string(), tour, day, own, fixed, undo: Vec::new(), last: (String::new(), None), sel: None, from, pdir: 0 })
}

fn pick_of(args: &Value) -> Option<Pick> {
    serde_json::from_value(args.get("pick").cloned().unwrap_or(Value::Null)).ok()
}

fn f32_of(args: &Value, k: &str) -> Option<f32> {
    args.get(k).and_then(Value::as_f64).map(|x| x as f32)
}

/// What the sheet's controls do (`co_more_act` "pe_*"); None when `what` is not one of them.
pub fn act(s: &mut Session, e: &mut PlanEdit, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let day = e.day;
    let mut out = json!({});
    match what {
        "pe_day" => {
            let d = args.get("day").and_then(Value::as_u64).unwrap_or(0) as usize;
            if d < DAY_GROUPS.len() && e.own.as_ref().is_none_or(|o| o.draft.service.allows(d)) {
                (e.day, e.sel) = (d, None);
            }
        }
        "pe_sel" => e.sel = pick_of(args),
        "pe_pdir" => e.pdir = args.get("pdir").and_then(Value::as_u64).unwrap_or(0).min(1) as usize,
        // a departure moved (dragged, typed, the arrows, +5 ... -5)
        "pe_move" => {
            if let (Some(p), Some(to)) = (pick_of(args), f32_of(args, "to")) {
                let w = args.get("what").and_then(Value::as_str).unwrap_or("move").to_string();
                if let Some(Some(q)) = e.edit(&w, |l| move_dep(l, day, p, to)) {
                    e.sel = Some(q);
                }
            }
        }
        "pe_add" => {
            let dir = args.get("dir").and_then(Value::as_u64).unwrap_or(0) as usize;
            if let Some(at) = f32_of(args, "at") {
                if let Some(Some(q)) = e.edit("add", |l| add_dep(l, day, dir, at)) {
                    e.sel = Some(q);
                }
                e.last = (String::new(), None);
            }
        }
        // a copy of the trip chosen, a headway later
        "pe_copy" => {
            if let (Some(p), Some(at)) = (pick_of(args), f32_of(args, "at")) {
                if let Some(Some(q)) = e.edit("copy", |l| add_dep(l, day, p.dir, at)) {
                    e.sel = Some(q);
                }
                e.last = (String::new(), None);
            }
        }
        "pe_remove" => {
            if let Some(p) = pick_of(args) {
                e.edit("delete", |l| remove_dep(l, day, p));
                e.sel = None;
                e.last = (String::new(), None);
            }
        }
        // "In one go": the pattern's fields as the page sends them back
        "pe_pattern" => {
            let pdir = args.get("pdir").and_then(Value::as_u64).unwrap_or(0).min(1) as usize;
            let w = args.get("what").and_then(Value::as_str).unwrap_or("pattern").to_string();
            let p = args.get("pattern").cloned().unwrap_or(Value::Null);
            let num = |k: &str| p.get(k).and_then(Value::as_f64).map(|x| x as f32);
            let bands: Option<Vec<TimeBand>> = p.get("bands").and_then(|b| serde_json::from_value(b.clone()).ok());
            let on = p.get("on").and_then(Value::as_bool);
            let table = e.own.as_ref().is_some_and(|o| o.draft.table_on);
            e.edit(&w, |l| {
                set_dir_pattern(l, day, pdir, |x| {
                    if let Some(v) = on {
                        x.on = v;
                    }
                    if let Some(v) = num("first") {
                        x.first = v;
                    }
                    if let Some(v) = num("last") {
                        x.last = v;
                    }
                    if let Some(v) = num("headway") {
                        x.headway = v.clamp(1.0, 240.0);
                    }
                    if let Some(v) = num("layover") {
                        x.layover = v.clamp(0.0, 60.0);
                    }
                    if let Some(b) = bands {
                        x.bands = b;
                    }
                })
            });
            // (a table made anew: what was chosen in it is gone)
            if table {
                e.sel = None;
            }
        }
        // the time bands switched on (the default ones of the days) or off
        "pe_mode" => {
            let pdir = args.get("pdir").and_then(Value::as_u64).unwrap_or(0).min(1) as usize;
            let bands = args.get("bands").and_then(Value::as_bool).unwrap_or(false);
            e.edit("pe-mode", |l| set_dir_pattern(l, day, pdir, |x| x.bands = if bands { reg::default_bands(day) } else { Vec::new() }));
        }
        "pe_run" => {
            let pdir = args.get("pdir").and_then(Value::as_u64).unwrap_or(0).min(1) as usize;
            if let Some(t) = f32_of(args, "total") {
                e.edit("pe-run", |l| l.set_run_total(pdir, t.round().max(1.0)));
            }
        }
        "pe_follow" => {
            let follows = args.get("follows").and_then(Value::as_bool).unwrap_or(true);
            let table = e.own.as_ref().is_some_and(|o| o.draft.table_on);
            e.edit("pe-follow", |l| {
                l.set_back_own(!follows);
                if l.table_on {
                    for k in 0..DAY_GROUPS.len() {
                        l.refill_table(k, &[1]);
                    }
                }
            });
            if table {
                e.sel = None;
            }
        }
        "pe_undo" => {
            if let (Some(prev), Some(o)) = (e.undo.pop(), e.own.as_mut()) {
                o.draft = prev;
                e.sel = None;
                e.last = (String::new(), None);
            }
        }
        "pe_again" => {
            if e.dirty() {
                if let Some(o) = e.own.as_mut() {
                    e.undo.push(o.draft.clone());
                    o.draft = o.base.clone();
                    e.sel = None;
                }
            }
        }
        "pe_from" => {
            let by = args.get("by").and_then(Value::as_i64).unwrap_or(0);
            if let Some(c) = s.company.as_ref() {
                let days = co::dates::between(&c.date, &e.from);
                if (by < 0 && days > 1) || (by > 0 && days < 60) {
                    e.from = co::dates::add(&e.from, by.signum());
                }
            }
        }
        "pe_save" => {
            let Some(o) = e.own.clone() else { return Some(Ok(out)) };
            let mut from = e.from.clone();
            let c = tpl::commit(s, &e.company, &e.line, &o, &mut from, &mut out);
            e.from = from;
            match c {
                Commit::Saved | Commit::Gone => out["closed"] = json!(true),
                Commit::Kept => {}
                // (the line as the registry has it now, shown as it is)
                Commit::Stale => {
                    if let Some(next) = s.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&e.line)).and_then(|cl| tpl::load_own(c, cl).ok())) {
                        e.own = Some(next);
                    }
                    (e.undo, e.sel) = (Vec::new(), None);
                }
            }
            if let Some(say) = out.get("say").cloned() {
                let (t, err) = (say.get("text").and_then(Value::as_str).unwrap_or("").to_string(), say.get("error").and_then(Value::as_bool).unwrap_or(false));
                s.say(t, err);
            }
        }
        _ => return None,
    }
    // (the departure chosen as it is now: a drag goes on from it)
    out["sel"] = json!(e.sel);
    Some(Ok(out))
}

// --- what the page draws ----------------------------------------------------------------------

/// A lane's word (`lane_word`).
fn lane_word(editable: bool, k: usize, dest: &str) -> String {
    tr(if editable {
        if k == 0 {
            "Outbound"
        } else {
            "Return"
        }
    } else if dest.is_empty() {
        "Other trips"
    } else {
        "Trips"
    })
}

fn short_day(date: &str) -> String {
    let full = day_label(date);
    match full.rsplit_once(' ') {
        Some((head, year)) if year.chars().all(|c| c.is_ascii_digit()) => head.to_string(),
        _ => full,
    }
}

/// The sheet's view: its head, the lanes, the departure chosen, the tours, what it needs, the
/// pattern in one go, the list and the foot (`plan_edit::sheet` and its cards). None: the
/// sheet has nothing to show any more (another company, the line given up).
pub fn view(s: &mut Session, e: &mut PlanEdit) -> Option<Value> {
    let c = s.company.clone()?;
    if c.id != e.company {
        return None;
    }
    let cl = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&e.line)).cloned()?;
    let plan = plan_of(s, &e.date).and_then(|p| p.ok());
    // (the planning's day gives its tours their buses only when it is a day of the group)
    let plan = plan.filter(|p| e.own.is_none() || ownline::day_type(&p.date) == e.day);
    let sh = match &e.own {
        Some(o) => shown_own(&c, &cl, o, e.day, plan.as_ref()),
        None => shown_fixed(&c, &cl.name, plan.as_ref()),
    };
    let editable = e.own.is_some();
    // the head and what a change does
    let days = e.own.as_ref().map(|o| {
        json!({
            "labels": DAY_GROUPS.iter().map(|g| tr(g.0)).collect::<Vec<_>>(),
            "off": (0..DAY_GROUPS.len()).filter(|k| !o.draft.service.allows(*k)).collect::<Vec<_>>(),
            "day": e.day,
        })
    });
    let running = cl.service_from.is_some();
    let (note, tone) = match (&e.own, e.fixed) {
        (_, Some(why)) => (tr(why), "warn"),
        (Some(o), _) if !o.saved.pending_from.is_empty() => (trf("A new timetable waits for %{date}: it is the one shown, against the tours that run until then.", &[("date", &day_label(&o.saved.pending_from))]), "accent2"),
        (Some(o), _) if o.draft.table_on && !o.base.table_on => (tr("Moved by hand, the timetable is a table of trips now (the line editor's \"As a table\"): every departure on its own."), "accent2"),
        _ if running => (tr("In service: today's tours run as they are; a change begins on the day you choose, tomorrow at the soonest."), "dim"),
        _ => (tr("Not in service yet: a change counts at once."), "dim"),
    };
    // the departures, lane by lane
    let tour_deps: Vec<(i32, Option<usize>)> = e.tour.as_ref().and_then(|n| sh.rows.iter().find(|r| &r.number == n && r.fate != Fate::Gone)).map(|r| r.legs.iter().filter(|g| g.2.is_some()).map(|g| (g.0, g.2)).collect()).unwrap_or_default();
    let lanes: Vec<Value> = sh
        .lanes
        .iter()
        .enumerate()
        .map(|(k, dest)| {
            let mine: Vec<Value> = sh
                .deps
                .iter()
                .filter(|d| d.dir == k)
                .map(|d| {
                    let marked = tour_deps.iter().any(|(m, lane)| *lane == Some(k) && (*m as f32 - d.dep).abs() < 0.6);
                    json!({ "dir": d.dir, "dep": d.dep, "arr": d.arr, "at": d.at, "text": hm(d.dep), "until": hm(d.arr), "marked": marked, "on": e.sel.is_some_and(|p| d.is(&p)) })
                })
                .collect();
            let count = trf("%{n} departures", &[("n", &mine.len().to_string())]);
            json!({ "word": lane_word(editable, k, dest), "to": if dest.is_empty() { String::new() } else { format!("→ {dest}") }, "dest": dest, "count": count, "deps": mine })
        })
        .collect();
    // the departure chosen (`trip_tools`)
    let chosen = e.sel.and_then(|p| sh.deps.iter().find(|d| d.is(&p)).copied());
    if chosen.is_none() {
        e.sel = None;
    }
    let tools = chosen.map(|d| {
        let way = if d.dir == 0 { tr("Outbound") } else { tr("Return") };
        let tour = sh.rows.iter().find(|row| row.fate != Fate::Gone && row.legs.iter().any(|g| g.2 == Some(d.dir) && (g.0 as f32 - d.dep).abs() < 0.6)).map(|row| format!("  ·  {} {}", tr("Tour"), row.number)).unwrap_or_default();
        let headway = e.own.as_ref().and_then(|o| o.draft.pattern(d.dir, e.day)).map(|p| if p.bands.is_empty() { p.headway } else { p.clean_bands().first().map(|b| b.headway).unwrap_or(p.headway) }).unwrap_or(20.0).max(1.0);
        json!({
            "pick": d.pick(), "dep": d.dep, "text": hm(d.dep), "words": format!("{} – {}  ·  {way}{tour}", hm(d.dep), hm(d.arr)),
            "headway": headway, "copy_tip": trf("A copy of the trip, %{n} minutes later", &[("n", &format!("{headway:.0}"))]),
        })
    });
    // the tours under the departures (`tours_chart`)
    let rows: Vec<Value> = sh
        .rows
        .iter()
        .map(|row| {
            let (fate, tip) = match row.fate {
                Fate::Kept => ("kept", "Runs as it does now"),
                Fate::Changed => ("changed", "Its trips change: its bus and drivers are given anew"),
                Fate::New => ("new", "A tour more: a bus and drivers to give"),
                Fate::Gone => ("gone", "No longer driven: its bus and drivers are free"),
            };
            let lose = matches!(row.fate, Fate::Changed | Fate::Gone);
            let bus = row.bus.as_ref().map(|b| if lose { tr("loses %{bus}").replace("%{bus}", b) } else { b.clone() });
            json!({ "number": row.number, "title": format!("{} {}", tr("Tour"), row.number), "fate": fate, "tip": tr(tip), "bus": bus, "lose": lose, "legs": row.legs, "focus": e.tour.as_deref() == Some(row.number.as_str()) && row.fate != Fate::Gone })
        })
        .collect();
    // what it needs (`needs_card`)
    let km = |x: f64| format!("{} km", grouped(x));
    let figures = [
        ("Trips", sh.now.trips.to_string(), sh.then.trips.to_string(), false),
        ("Tours", sh.now.tours.to_string(), sh.then.tours.to_string(), false),
        ("Buses at the busiest hour", sh.now.buses.to_string(), sh.then.buses.to_string(), sh.then.buses > sh.now.buses),
        ("Duties for drivers", sh.now.duties.to_string(), sh.then.duties.to_string(), sh.then.duties > sh.now.duties),
        ("Kilometres a day", km(sh.now.km), km(sh.then.km), false),
    ];
    let have = c.fleet.iter().filter(|v| v.held_on(&c.date) && ownline::line_allows(&c, &cl.name, v)).count();
    let (fleet, fleet_tone) = if sh.then.buses > have {
        (trf("At the busiest it needs %{n} buses; the fleet has %{have} for the line.", &[("n", &sh.then.buses.to_string()), ("have", &have.to_string())]), "warn")
    } else {
        (trf("The fleet has %{have} buses for the line.", &[("have", &have.to_string())]), "dim")
    };
    let d = &sh.diff;
    let tours_text = editable.then(|| format!("{}: {}", tr("Tours of every day"), trf("%{k} kept, %{c} changed, %{a} new, %{g} dropped", &[("k", &d.kept.len().to_string()), ("c", &d.changed.len().to_string()), ("a", &d.added.len().to_string()), ("g", &d.gone.len().to_string())])));
    let replan = d.replan();
    let replan_text = (editable && !replan.is_empty()).then(|| trf("Given their bus and drivers anew: tours %{list}", &[("list", &replan.join(", "))]));
    let needs = json!({
        "caps": tr("What it needs").to_uppercase(), "now": tr("Now").to_uppercase(), "changed": tr("Changed").to_uppercase(),
        "figures": figures.iter().map(|(l, a, b, more)| json!({ "label": tr(l), "now": a, "then": b, "tone": if a != b { if *more { "warn" } else { "accent2" } } else { "text" } })).collect::<Vec<_>>(),
        "fleet": fleet, "fleet_tone": fleet_tone, "tours": tours_text, "replan": replan_text,
    });
    // the pattern in one go (`pattern_card`)
    let pattern = e.own.as_ref().map(|o| {
        let two = dirs(&o.draft) >= 2;
        let pdir = if two { e.pdir.min(1) } else { 0 };
        let kind = o.draft.service;
        if !kind.allows(e.day) {
            return json!({ "caps": tr("In one go").to_uppercase(), "off": trf("%{kind} does not run then", &[("kind", &tr(kind.label()))]) });
        }
        let run = o.draft.directions.get(pdir).filter(|d| two && d.stops.len() >= 2).map(|d| reg::fitted_times(d).last().copied().unwrap_or(1.0).max(1.0));
        let p = o.draft.pattern(pdir, e.day).cloned();
        let follows = pdir == 1 && !o.draft.back_own();
        json!({
            "caps": tr("In one go").to_uppercase(), "two": two, "pdir": pdir, "dirs": [tr("Outbound"), tr("Return")],
            "run": run, "run_label": tr("Trip takes"), "follows": if pdir == 1 { Some(!o.draft.back_own()) } else { None },
            "follow_label": tr("Leaves as the outbound does"),
            "follow_note": follows.then(|| tr("The return leaves at the outbound's times. Switch this off to give it departures of its own.")),
            "pattern": p.map(|p| json!({ "on": p.on, "first": p.first, "last": p.last, "headway": p.headway, "layover": p.layover, "bands": p.bands })),
            "runs_label": tr("Runs on these days"), "modes": [tr("One headway"), tr("Time bands")],
            "first_label": tr("First"), "last_label": tr("Last"), "every_label": tr("Every"), "stands_label": tr("Stands"), "add_band": tr("Add a band"), "band_x": tr("Take the band out"),
            "table_note": o.draft.table_on.then(|| tr("A change here makes these days' trips anew from the pattern: the departures moved by hand on these days go.")),
            "day_end": reg::DAY_END,
        })
    });
    // the foot (`foot_row`)
    let dirty = e.dirty();
    let foot = e.own.as_ref().map(|o| {
        let later = running && reg::timetable_differs(&o.live, &o.draft);
        if later && co::dates::between(&c.date, &e.from) < 1 {
            e.from = co::dates::add(&c.date, 1);
        }
        let days = co::dates::between(&c.date, &e.from);
        let (text, tone) = match sh.problems.first() {
            Some(p) => (tr(p.text).replace("%{n}", &p.n.to_string()), "warn"),
            None if !dirty => (tr("No changes yet."), "dim"),
            None if later => (tr("Today's tours run as they are."), "dim"),
            None => (tr("Saved, it counts at once."), "dim"),
        };
        let when = if days <= 1 { format!("{}  ({})", day_label(&e.from), tr("tomorrow")) } else { day_label(&e.from) };
        json!({
            "save": if later { trf("Save from %{date}", &[("date", &short_day(&e.from))]) } else { tr("Save") },
            "can": dirty && sh.problems.is_empty(), "later": later, "from": format!("{} {when}", tr("From")),
            "earlier": days > 1, "laterday": days < 60, "text": text, "tone": tone, "undo": !e.undo.is_empty(),
        })
    });
    let (t0, t1) = span_of(&sh);
    Some(json!({
        "line": cl.name, "plate": plate(&cl.number, &cl.colour),
        "title": trf("Trips and timetable of line %{n}", &[("n", &cl.number)]),
        "caption": if cl.title.trim().is_empty() { cl.caption.clone() } else { cl.title.trim().to_string() },
        "days": days, "date": day_label(&e.date), "note": note, "tone": tone, "editable": editable,
        "whole": [t0, t1], "lanes": lanes, "tools": tools, "sel": e.sel,
        "departures": tr("Departures").to_uppercase(), "whole_day": tr("Whole day"),
        "hint": tr(if editable { "Drag a departure to move it · click the axis to add one · Delete takes the one chosen out · the wheel zooms" } else { "The wheel over the departures zooms into the hours" }),
        "choose": tr("Choose a departure on the axis or in the list: drag it, type its time, or take it out."),
        "copy": tr("Copy"), "take_out": tr("Take the trip out"), "shift_tip": tr("The whole trip so many minutes earlier or later"),
        "tours_title": trf("Tours: the buses it needs (%{n})", &[("n", &sh.rows.iter().filter(|r| r.fate != Fate::Gone).count().to_string())]).to_uppercase(),
        "legend": editable.then(|| json!([[tr("changed"), "warn"], [tr("new"), "ok"], [tr("dropped"), "danger"]])),
        "no_tours": tr("No tours on these days."), "rows": rows,
        "needs": needs, "pattern": pattern, "list_caps": tr("As a list").to_uppercase(),
        "foot": foot, "dirty": dirty, "close": tr("Close"), "undo": tr("Undo"), "again": tr("Start again"), "again_tip": tr("The line's timetable as it is saved"),
        "leave": { "ask": tr("Close without saving the changes?"), "yes": tr("Close without saving"), "no": tr("Keep changing") },
    }))
}

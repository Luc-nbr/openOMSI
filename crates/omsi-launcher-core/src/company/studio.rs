//! The scheduling studio (the planning, phase 1; Luc: the bus company's planning should look
//! and work "like Optibus Scheduling"): the day's vehicle blocks - a bus's day, to begin with
//! a tour of the timetable - and the driver duties cut from them, seen and edited by hand in
//! the advanced mode.
//!
//! A block keeps to its line. The player moves a trip with passengers to another bus of the
//! line, or to a new one; he cuts a block into duties, only at a relief point that allows it -
//! where the bus stands at a terminus or the depot, or halfway along a trip at a stop the player
//! made a relief point (`cut_mid`, Luc 2026-10-14; it was "relief points are ONLY a line's
//! termini and the depot" before) - and joins them again; and he joins pieces of different blocks into one driver's
//! duty (a split duty, or one that changes buses). The working-time rules (`staff::check`)
//! refuse what may not be and warn of the rest. In RosterFlow's Drivers view he moves a trip
//! (or several) of a duty to another duty or to a new one (`plan_move_to_duty`): the bus keeps
//! the trip, only its driver changes, and the block is cut anew where the driver changes - at a
//! terminus or the depot again. What he lays out is kept with the company a
//! weekday at a time, as the weekly roster is (`plan::Planning::layouts`, `joined`), and
//! `network::tours_of_day` gives every page the day so: the planning, the day's close, and
//! the game's plan (`plan::live_plan` runs each tour of the timetable with the bus of the
//! block that holds most of it). An edit the timetable no longer fits is dropped for its line
//! - the automatic blocks and cuts again - and the inbox says so (`retire_stale`). The easy
//! mode keeps the timetable's tours: its planning runs by itself.

use super::economy;
use super::inbox::{self, Kind, Message};
use super::model::{BusKind, BusSize, Cents, Company, Drive};
use super::network::{self, PlannedTrip, TourOfDay};
use super::ownline::DEPOT_RUN_KM;
use super::plan::{DayTour, Who};
use super::staff::{self, Block, DAY_MAX, DAY_TARGET, MIN_PAID, REST};
use crate::LineInfo;
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// A break between two pieces of a duty this long or longer makes it a split duty.
pub const SPLIT_BREAK: i32 = 60;
/// A split duty's break longer than this is warned of.
pub const LONG_SPLIT: i32 = 240;
/// The least a bus takes to run empty between two places the line has no trip between.
pub const DEADHEAD_MIN: i32 = 5;

// --- what is kept ----------------------------------------------------------------------------

/// A trip of the timetable by where it comes from: its tour there, its name, and when it
/// leaves (minutes of the day).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct TripRef {
    pub tour: String,
    pub name: String,
    pub dep: i32,
}

impl TripRef {
    pub fn of(t: &PlannedTrip) -> TripRef {
        TripRef { tour: t.origin.trim().to_string(), name: t.name.clone(), dep: t.dep }
    }

    /// Whether it is that trip.
    pub fn is(&self, t: &PlannedTrip) -> bool {
        self.dep == t.dep && self.name == t.name && self.tour.trim() == t.origin.trim()
    }
}

/// A bus's day as the player laid it out: its trips, and the trips its duties begin with
/// (None: cut automatically).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BlockLayout {
    /// Its name (the roster gives a bus and drivers by it): a tour of the timetable's, or a
    /// new number.
    pub tour: String,
    pub trips: Vec<TripRef>,
    #[serde(default)]
    pub cuts: Option<Vec<TripRef>>,
    /// Trips the drivers change on at a relief point on the way (RosterFlow's mid-trip
    /// relief): each cut in two at its stop there, a duty beginning with the second part.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reliefs: Vec<MidRelief>,
}

/// A trip of a block cut at a relief point on the way: the trip as the timetable has it, and
/// the stop of it the drivers change at (its place in the trip's stops, 0 the first, and its
/// name, to see it is still that stop).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MidRelief {
    pub trip: TripRef,
    pub stop: usize,
    pub name: String,
}

/// A line's blocks on a weekday as the player laid them out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LineLayout {
    /// 0 Monday .. 6 Sunday.
    pub weekday: u8,
    /// The company line (its timetable name).
    pub line: String,
    pub blocks: Vec<BlockLayout>,
    /// Trips with passengers the player took off every bus (RosterFlow's stack): they wait
    /// there to be assigned again, and no bus of the company runs them meanwhile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<TripRef>,
}

/// A piece of work: a duty of a block, by its block and the trip it begins with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PieceRef {
    pub line: String,
    pub tour: String,
    pub first: TripRef,
}

impl PieceRef {
    fn of(t: &TourOfDay, r: &Range<usize>) -> PieceRef {
        PieceRef { line: t.line.clone(), tour: t.tour.clone(), first: TripRef::of(&t.trips[r.start]) }
    }

    fn same(&self, o: &PieceRef) -> bool {
        same(&self.line, &o.line) && same(&self.tour, &o.tour) && self.first == o.first
    }
}

/// Pieces of work the player joined into one driver's duty, on a weekday.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct JoinedDuty {
    pub weekday: u8,
    pub pieces: Vec<PieceRef>,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

// --- the day as laid out ---------------------------------------------------------------------

/// The line's blocks as `l` lays them out from the line's tours of the timetable (`tours`), or
/// None when it no longer fits them: a trip it names is gone, or a trip with passengers is in
/// none of its blocks. Each block with the trips its duties begin with (None: cut
/// automatically).
fn build(l: &LineLayout, tours: &[&TourOfDay], info: Option<&LineInfo>) -> Option<Vec<(TourOfDay, Option<Vec<TripRef>>)>> {
    let template = tours.first()?;
    let mut pool: Vec<(&PlannedTrip, bool)> = tours.iter().flat_map(|t| t.trips.iter()).map(|x| (x, false)).collect();
    let mut out = Vec::new();
    // (the stack's trips run with no bus: placed, in no block)
    for r in &l.stack {
        let slot = pool.iter_mut().find(|(x, used)| !*used && r.is(x))?;
        slot.1 = true;
    }
    for b in &l.blocks {
        let mut trips: Vec<PlannedTrip> = Vec::new();
        for r in &b.trips {
            let slot = pool.iter_mut().find(|(x, used)| !*used && r.is(x))?;
            slot.1 = true;
            trips.push(slot.0.clone());
        }
        if !trips.iter().any(PlannedTrip::counts) {
            continue;
        }
        trips.sort_by_key(|x| x.dep);
        // (the trips the drivers change on at a relief point on the way: in two, a duty
        // beginning with the second part - a stop no longer on the trip leaves it whole)
        let mut cuts = b.cuts.clone();
        for r in &b.reliefs {
            let Some(k) = trips.iter().position(|x| x.part.is_none() && r.trip.is(x)) else { continue };
            let Some((first, second)) = info.and_then(|i| split_trip(i, &trips[k], r.stop, &r.name)) else { continue };
            cuts.get_or_insert_with(Vec::new).push(TripRef::of(&second));
            trips[k] = first;
            trips.insert(k + 1, second);
        }
        // (its depot group: the timetable tour's of its name, else the one its first trip
        // comes from)
        let group = tours.iter().find(|t| same(&t.tour, &b.tour)).or_else(|| tours.iter().find(|t| same(&t.tour, &trips[0].origin))).map(|t| t.ai_group.clone()).unwrap_or_default();
        let t = TourOfDay { line: template.line.clone(), number: template.number.clone(), tour: b.tour.clone(), ai_group: group, trips, unplanned: false, cuts: None };
        out.push((t, cuts));
    }
    if pool.iter().any(|(x, used)| !*used && x.counts()) {
        return None;
    }
    Some(out)
}

/// The timetable's own of trip `x` of line `info` (its stops and their times): by its tour,
/// its name and the minute the whole trip leaves.
pub fn trip_info<'a>(info: &'a LineInfo, x: &PlannedTrip) -> Option<&'a crate::TripInfo> {
    let dep = x.whole_dep();
    info.tours.iter().filter(|t| t.number.trim() == x.origin.trim()).flat_map(|t| t.trips.iter()).find(|y| y.name == x.name && (y.departure / 60.0).round() as i32 == dep)
}

/// Trip `x` of line `info` cut at its stop `stop` (named `name`): its two parts, the bus at the
/// stop at the minute the timetable has it there (`PlannedTrip::split_at`). None: no such stop
/// on the way between its ends.
pub fn split_trip(info: &LineInfo, x: &PlannedTrip, stop: usize, name: &str) -> Option<(PlannedTrip, PlannedTrip)> {
    let y = trip_info(info, x)?;
    let s = y.stops.get(stop)?;
    if !same(&s.name, name) {
        return None;
    }
    let span = (y.arrival - y.departure).max(1.0);
    x.split_at(stop, &s.name, (s.dep / 60.0).round() as i32, (s.dep - y.departure) / span)
}

/// A trip as the layouts know it: a part of a trip cut on the way by the whole trip.
pub fn whole_ref(x: &PlannedTrip) -> TripRef {
    TripRef { tour: x.origin.trim().to_string(), name: x.name.clone(), dep: x.whole_dep() }
}

/// Trips `idx` of block `t` whole (each once): a part of a trip cut on the way stands for it.
fn wholes(t: &TourOfDay, idx: &[usize]) -> Vec<PlannedTrip> {
    let mut out: Vec<PlannedTrip> = Vec::new();
    for &k in idx {
        let Some(x) = t.trips.get(k) else { continue };
        let w = match &x.part {
            Some(p) if p.second => match k.checked_sub(1).and_then(|j| t.trips.get(j)) {
                Some(f) => f.joined(x),
                None => continue,
            },
            Some(_) => match t.trips.get(k + 1) {
                Some(n) if n.is_second() => x.joined(n),
                _ => x.clone(),
            },
            None => x.clone(),
        };
        if !out.iter().any(|o| TripRef::of(o) == TripRef::of(&w)) {
            out.push(w);
        }
    }
    out
}

/// A block's cuts kept as `at` (the trips of `t` its duties begin with): a second part of a
/// trip is a relief on the way (`BlockLayout::reliefs`), the others the trips cut before.
fn set_block_cuts(b: &mut BlockLayout, t: &TourOfDay, at: &[usize]) {
    let mut cuts = Vec::new();
    let mut reliefs = Vec::new();
    for x in at.iter().filter_map(|&k| t.trips.get(k)) {
        match &x.part {
            Some(p) if p.second => reliefs.push(MidRelief { trip: whole_ref(x), stop: p.stop, name: p.at.clone() }),
            _ => cuts.push(TripRef::of(x)),
        }
    }
    b.cuts = Some(cuts);
    b.reliefs = reliefs;
}

/// The day's tours of the timetable (`network::timetable_tours`, `info` the timetable read) as
/// the player laid out their lines on `weekday`. Returns the blocks whose duties he cut, with
/// the trips they begin with (found once the depot runs are in, `resolve_cuts`).
pub fn arrange(c: &Company, weekday: u8, tours: &mut Vec<TourOfDay>, info: &[LineInfo]) -> Vec<(usize, Vec<TripRef>)> {
    let mut laid: Vec<Vec<(TourOfDay, Option<Vec<TripRef>>)>> = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for l in c.planning.layouts.iter().filter(|l| l.weekday == weekday) {
        let mine: Vec<&TourOfDay> = tours.iter().filter(|t| same(&t.line, &l.line)).collect();
        if let Some(blocks) = build(l, &mine, info.iter().find(|x| same(&x.name, &l.line))) {
            laid.push(blocks);
            lines.push(&l.line);
        }
    }
    if laid.is_empty() {
        return Vec::new();
    }
    tours.retain(|t| !lines.iter().any(|l| same(l, &t.line)));
    let mut cuts = Vec::new();
    for (t, cut) in laid.into_iter().flatten() {
        if let Some(cut) = cut {
            cuts.push((tours.len(), cut));
        }
        tours.push(t);
    }
    cuts
}

/// The blocks' cuts found among their trips, the depot runs in (`arrange`).
pub fn resolve_cuts(tours: &mut [TourOfDay], cuts: &[(usize, Vec<TripRef>)]) {
    for (i, refs) in cuts {
        let Some(t) = tours.get_mut(*i) else { continue };
        let mut at: Vec<usize> = refs.iter().filter_map(|r| t.trips.iter().position(|x| r.is(x))).filter(|&k| k > 0).collect();
        at.sort_unstable();
        at.dedup();
        t.cuts = Some(at);
    }
}

/// The lines whose layout on `date`'s weekday the timetable (`lines`) no longer fits.
pub fn stale(c: &Company, lines: &[LineInfo], date: &str) -> Vec<String> {
    let wd = super::plandays::slot_of(c, date);
    let raw = network::timetable_tours(c, lines);
    c.planning
        .layouts
        .iter()
        .filter(|l| l.weekday == wd)
        .filter(|l| {
            let mine: Vec<&TourOfDay> = raw.iter().filter(|t| same(&t.line, &l.line)).collect();
            !mine.is_empty() && build(l, &mine, None).is_none()
        })
        .map(|l| l.line.clone())
        .collect()
}

/// The title and text of the inbox's word on a layout dropped (`retire_stale`).
pub const STALE: (&str, &str) = ("Planning back to automatic", "The timetable of line %{n} changed: the blocks and duties you laid out for it in the planning no longer fit, so it runs with the automatic ones again.");

/// Drop the layouts the timetable of `date` no longer fits (and the duties joined on their
/// lines that day), each said in the inbox: the line runs with the automatic blocks and cuts.
/// Returns the lines (their numbers). Nothing in the easy mode, which uses none of them.
pub fn retire_stale(c: &mut Company, lines: &[LineInfo], date: &str) -> Vec<String> {
    if super::autopilot::on(c) {
        return Vec::new();
    }
    let wd = super::plandays::slot_of(c, date);
    let gone = stale(c, lines, date);
    let mut numbers = Vec::new();
    for line in &gone {
        c.planning.layouts.retain(|l| !(l.weekday == wd && same(&l.line, line)));
        c.planning.joined.retain(|j| !(j.weekday == wd && j.pieces.iter().any(|p| same(&p.line, line))));
        let number = c.lines.iter().find(|l| same(&l.name, line)).map(|l| l.number.clone()).unwrap_or_else(|| line.clone());
        let m = Message::new(Kind::Lines, STALE.0, STALE.1).arg("n", number.clone()).level(super::clock::Level::Warn).key(&format!("studio-stale-{line}"));
        inbox::post(c, m);
        numbers.push(number);
    }
    numbers
}

/// Every English text the core gives the studio's pages (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut out = vec![STALE.0, STALE.1];
    out.extend(REFUSALS);
    out.extend(Rule::LABELS);
    out
}

/// A line renamed: its layouts and joined duties follow it.
pub fn rename_line(c: &mut Company, old: &str, new: &str) {
    super::rosterflow::rename_line(c, old, new);
    for l in c.planning.layouts.iter_mut().filter(|l| same(&l.line, old)) {
        l.line = new.to_string();
    }
    for p in c.planning.joined.iter_mut().flat_map(|j| j.pieces.iter_mut()).filter(|p| same(&p.line, old)) {
        p.line = new.to_string();
    }
}

/// Whether the player laid out anything of `weekday`.
pub fn edited(c: &Company, weekday: u8) -> bool {
    c.planning.layouts.iter().any(|l| l.weekday == weekday) || c.planning.joined.iter().any(|j| j.weekday == weekday)
}

/// Back to the automatic blocks and cuts on `weekday` ("Optimise" accepted): returns whether
/// anything was laid out.
pub fn reset_day(c: &mut Company, weekday: u8) -> bool {
    let was = edited(c, weekday);
    c.planning.layouts.retain(|l| l.weekday != weekday);
    c.planning.joined.retain(|j| j.weekday != weekday);
    was
}

// --- the game's side ------------------------------------------------------------------------

/// The timetable's tours a block of the day (`t`, one of `all`) runs in the game: each it
/// holds more trips of than any other block of its line (the first such on a tie). A block
/// whose trips do not know their tour: its own.
pub fn origins(all: &[DayTour], t: &DayTour) -> Vec<String> {
    let count = |b: &TourOfDay, o: &str| b.trips.iter().filter(|x| x.origin.trim() == o).count();
    let mut mine: Vec<String> = Vec::new();
    for x in t.tour.trips.iter().filter(|x| !x.origin.trim().is_empty()) {
        if !mine.iter().any(|o| o == x.origin.trim()) {
            mine.push(x.origin.trim().to_string());
        }
    }
    if mine.is_empty() {
        return vec![t.tour.tour.clone()];
    }
    mine.into_iter()
        .filter(|o| {
            let here = count(&t.tour, o);
            // (the first block of the line holding the most of it)
            let best = all.iter().filter(|b| same(&b.tour.line, &t.tour.line)).max_by_key(|b| (count(&b.tour, o), std::cmp::Reverse(std::ptr::addr_of!(**b) as usize)));
            best.is_none_or(|b| std::ptr::eq(b, t) || count(&b.tour, o) < here)
        })
        .collect()
}

/// The driver of another piece of the duty the player joined piece `k` of block `i` into
/// (`tours` the day plan's, as far as it is given), if one has a driver of the staff.
pub fn partner_driver(c: &Company, weekday: u8, tours: &[DayTour], i: usize, k: usize) -> Option<u32> {
    let t = tours.get(i)?;
    let d = t.duties.get(k)?;
    let me = PieceRef::of(&t.tour, &(d.start..d.end));
    let group = c.planning.joined.iter().find(|j| j.weekday == weekday && j.pieces.iter().any(|p| p.same(&me)))?;
    for p in group.pieces.iter().filter(|p| !p.same(&me)) {
        for x in tours.iter().filter(|x| same(&x.tour.line, &p.line) && same(&x.tour.tour, &p.tour)) {
            for d in &x.duties {
                if let (true, Some(Who::Staff(id))) = (x.tour.trips.get(d.start).is_some_and(|f| p.first.is(f)), d.who) {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// The other pieces of the duty a piece was joined into (its block's line and name, and the
/// trip it begins with): (block, duty) in `tours`.
pub fn partners(c: &Company, weekday: u8, tours: &[TourOfDay], i: usize, k: usize) -> Vec<(usize, usize)> {
    let Some(t) = tours.get(i) else { return Vec::new() };
    let pieces = network::duties_of(t);
    let Some(r) = pieces.get(k) else { return Vec::new() };
    let me = PieceRef::of(t, r);
    let Some(group) = c.planning.joined.iter().find(|j| j.weekday == weekday && j.pieces.iter().any(|p| p.same(&me))) else { return Vec::new() };
    group.pieces.iter().filter(|p| !p.same(&me)).filter_map(|p| find_piece(tours, p)).collect()
}

fn find_piece(tours: &[TourOfDay], p: &PieceRef) -> Option<(usize, usize)> {
    tours.iter().enumerate().filter(|(_, t)| same(&t.line, &p.line) && same(&t.tour, &p.tour)).find_map(|(i, t)| network::duties_of(t).iter().position(|r| p.first.is(&t.trips[r.start])).map(|k| (i, k)))
}

// --- editing ---------------------------------------------------------------------------------

/// Why the studio refuses an edit (English, translated when shown).
pub const OVERLAP: &str = "It overlaps another trip of this bus.";
pub const NO_TIME: &str = "No time to get there: the bus cannot reach the trip's first stop in time.";
pub const IN_DEPOT: &str = "The bus is still in the depot then: it leaves for its first trip later.";
pub const BACK_IN_DEPOT: &str = "The bus is back in the depot by then.";
pub const OTHER_LINE: &str = "A bus keeps to its line: move the trip to a bus of the same line.";
pub const NOT_MOVABLE: &str = "Only a trip with passengers can be moved.";
pub const SAME_BUS: &str = "The trip is on this bus already.";
pub const ALONE: &str = "This bus runs only this trip already.";
pub const BETWEEN: &str = "A duty can only be cut between two trips.";
pub const CUT_ALREADY: &str = "A new duty begins there already.";
pub const ON_THE_MOVE: &str = "The bus is on the move there: drivers change only where it stands.";
pub const NOT_RELIEF: &str = "Drivers change only at a relief point that allows it (Preferences › Relief Points).";
pub const NO_PASSENGERS: &str = "Each duty needs a trip with passengers.";
pub const NO_CUT: &str = "No duty begins there.";
pub const TOO_LONG: &str = "That duty would be longer than a working day may be (10 h).";
pub const SAME_DUTY: &str = "That is the same duty.";
pub const NOT_JOINED: &str = "This duty is not joined with another.";
pub const NOT_FOUND: &str = "The planning changed meanwhile: try again.";
pub const LOCKED: &str = "This vehicle is locked: unlock it first.";
pub const SWAP_FAILS: &str = "Trips could not be swapped: they do not fit on the other bus.";
pub const DUTY_LOCKED: &str = "This duty is locked: unlock it first.";

const REFUSALS: [&str; 28] = [OVERLAP, NO_TIME, IN_DEPOT, BACK_IN_DEPOT, OTHER_LINE, NOT_MOVABLE, SAME_BUS, ALONE, BETWEEN, CUT_ALREADY, ON_THE_MOVE, NOT_RELIEF, NO_PASSENGERS, NO_CUT, TOO_LONG, SAME_DUTY, NOT_JOINED, NOT_FOUND, LOCKED, SWAP_FAILS, DUTY_LOCKED, ONE_DUTY, DUTY_ALONE, DRIVER_NO_TIME, NOT_ON_THE_WAY, MID_ALREADY, PAST_THE_ROWS, NOT_A_DUTY];

/// Where a trip is moved to: a bus of the day (a block's index), or a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Block(usize),
    New,
}

/// The line's blocks as the day has them now (`tours` as `network::tours_of_day` gives them):
/// the start of a layout to change. The depot runs the company adds itself are left out
/// (added again), and so are cuts the player did not make.
fn layout_now(c: &Company, tours: &[TourOfDay], line: &str, weekday: u8) -> LineLayout {
    let stack = c.planning.layouts.iter().find(|l| l.weekday == weekday && same(&l.line, line)).map(|l| l.stack.clone()).unwrap_or_default();
    let blocks = tours.iter().filter(|t| same(&t.line, line)).map(layout_of_block).collect();
    LineLayout { weekday, line: line.to_string(), blocks, stack }
}

/// A block of the day as a layout keeps it: its trips (a trip cut on the way once, with its
/// relief), the cuts the player made.
pub fn layout_of_block(t: &TourOfDay) -> BlockLayout {
    let mut b = BlockLayout { tour: t.tour.clone(), trips: t.trips.iter().filter(|x| !x.origin.trim().is_empty() && !x.is_second()).map(TripRef::of).collect(), cuts: None, reliefs: Vec::new() };
    if let Some(cs) = t.cuts.as_ref() {
        set_block_cuts(&mut b, t, cs);
    }
    b
}

/// A block's cuts kept as the trips of `t` its duties begin with (`at`), a relief on the way
/// among them (`set_block_cuts`).
pub fn set_cuts_of(b: &mut BlockLayout, t: &TourOfDay, at: &[usize]) {
    set_block_cuts(b, t, at);
}

fn save(c: &mut Company, l: LineLayout) {
    c.planning.layouts.retain(|x| !(x.weekday == l.weekday && same(&x.line, &l.line)));
    c.planning.layouts.push(l);
}

/// The duties joined on `weekday` with a piece of these blocks taken apart (their pieces
/// changed).
fn unjoin_blocks(c: &mut Company, weekday: u8, line: &str, blocks: &[&str]) {
    c.planning.joined.retain(|j| !(j.weekday == weekday && j.pieces.iter().any(|p| same(&p.line, line) && blocks.iter().any(|b| same(&p.tour, b)))));
}

//// How long a bus takes from `a` to `b` empty: the line's quickest trip between them, else
/// `DEADHEAD_MIN` (none between one place and itself).
pub fn travel(trips: &[&PlannedTrip], a: &str, b: &str) -> i32 {
    if same(a, b) {
        return 0;
    }
    trips.iter().filter(|x| same(&x.from, a) && same(x.end(), b)).map(|x| x.arr - x.dep).min().unwrap_or(DEADHEAD_MIN)
}

/// The kilometres a bus runs empty from `a` to `b`: the line's shortest trip between them,
/// else a depot run's.
pub fn deadhead_km(trips: &[&PlannedTrip], a: &str, b: &str) -> f64 {
    if same(a, b) {
        return 0.0;
    }
    trips.iter().filter(|x| same(&x.from, a) && same(x.end(), b)).map(|x| x.km).fold(None, |m: Option<f64>, k| Some(m.map_or(k, |m| m.min(k)))).unwrap_or(DEPOT_RUN_KM)
}

/// Whether trip `x` fits into a bus's day (`block`, its trips in order; `line` every trip of
/// the line, for the times between places): not over another trip, time enough to get to it
/// and on from it, not before the bus leaves the depot or after it is back. The depot runs the
/// company adds itself move with the trips and are no hindrance.
pub fn fits_into(block: &[PlannedTrip], x: &PlannedTrip, line: &[&PlannedTrip]) -> Result<(), &'static str> {
    let me = TripRef::of(x);
    let others: Vec<&PlannedTrip> = block.iter().filter(|t| !t.origin.trim().is_empty() && TripRef::of(t) != me).collect();
    let at = others.iter().filter(|t| t.dep <= x.dep).count();
    if let Some(p) = at.checked_sub(1).map(|k| others[k]) {
        if x.dep < p.arr {
            return Err(OVERLAP);
        }
        if p.empty && !others[at..].iter().any(|t| t.counts()) && others[..at].iter().any(|t| t.counts()) {
            return Err(BACK_IN_DEPOT);
        }
        if x.dep - p.arr < travel(line, p.end(), &x.from) {
            return Err(NO_TIME);
        }
    }
    if let Some(n) = others.get(at) {
        if n.dep < x.arr {
            return Err(OVERLAP);
        }
        if n.empty && !others[..at].iter().any(|t| t.counts()) && others[at..].iter().any(|t| t.counts()) {
            return Err(IN_DEPOT);
        }
        if n.dep - x.arr < travel(line, x.end(), &n.from) {
            return Err(NO_TIME);
        }
    }
    Ok(())
}

/// Whether all of `trips` fit into a bus's day together (`fits_into`, one after the other).
pub fn all_fit(block: &[PlannedTrip], trips: &[&PlannedTrip], line: &[&PlannedTrip]) -> Result<(), &'static str> {
    let mut cur: Vec<PlannedTrip> = block.to_vec();
    let mut sorted: Vec<&PlannedTrip> = trips.to_vec();
    sorted.sort_by_key(|x| x.dep);
    for x in sorted {
        fits_into(&cur, x, line)?;
        cur.push(x.clone());
        cur.sort_by_key(|t| t.dep);
    }
    Ok(())
}

/// A free name for a new block of the line: the next number after its blocks' and the
/// roster's (a new bus takes no bus or driver given to an old one).
pub fn new_name(c: &Company, weekday: u8, tours: &[TourOfDay], line: &str) -> String {
    let taken: Vec<String> = tours.iter().filter(|t| same(&t.line, line)).map(|t| t.tour.trim().to_string()).chain(c.planning.week.iter().filter(|r| r.weekday == weekday && same(&r.line, line)).map(|r| r.tour.trim().to_string())).collect();
    let mut n = taken.iter().filter_map(|t| t.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    while taken.iter().any(|t| *t == n.to_string()) {
        n += 1;
    }
    n.to_string()
}

/// The trips that move with trip `k` of block `t`: the trips linked with it (RosterFlow's
/// links, `rosterflow::linked`), itself among them.
fn with_links(c: &Company, weekday: u8, t: &TourOfDay, picked: &[usize]) -> Vec<usize> {
    let mut out: Vec<usize> = picked.to_vec();
    for &k in picked {
        let Some(x) = t.trips.get(k) else { continue };
        for r in super::rosterflow::linked(c, weekday, &t.line, &whole_ref(x)) {
            if let Some(j) = t.trips.iter().position(|y| r.is(y)) {
                out.push(j);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The line's layout without the trips `refs` in block `tour` (and their cuts), the buses left
/// without a trip with passengers gone, every block in order.
fn take_out(lay: &mut LineLayout, tour: &str, refs: &[TripRef], line_trips: &[&PlannedTrip]) -> Result<(), &'static str> {
    let b = lay.blocks.iter_mut().find(|b| same(&b.tour, tour)).ok_or(NOT_FOUND)?;
    b.trips.retain(|t| !refs.contains(t));
    if let Some(cuts) = b.cuts.as_mut() {
        cuts.retain(|t| !refs.contains(t));
    }
    b.reliefs.retain(|r| !refs.contains(&r.trip));
    tidy(lay, line_trips);
    Ok(())
}

fn tidy(lay: &mut LineLayout, line_trips: &[&PlannedTrip]) {
    // (a bus left without a trip with passengers runs no more)
    let counted = |b: &BlockLayout| b.trips.iter().any(|r| line_trips.iter().any(|x| r.is(x) && x.counts()));
    lay.blocks.retain(counted);
    for b in lay.blocks.iter_mut() {
        b.trips.sort_by_key(|t| t.dep);
    }
    lay.stack.sort_by_key(|t| t.dep);
}

/// What moving trip `trip` of block `from` to `to` would make of its line (None: refused,
/// and why). The trips linked with it move along.
pub fn plan_move(c: &Company, weekday: u8, tours: &[TourOfDay], from: usize, trip: usize, to: Target) -> Result<LineLayout, &'static str> {
    plan_move_many(c, weekday, tours, from, &[trip], to)
}

/// What moving trips `picked` of block `from` together to `to` would make of its line: every
/// one a trip with passengers, the trips linked with them along, none of a locked bus.
pub fn plan_move_many(c: &Company, weekday: u8, tours: &[TourOfDay], from: usize, picked: &[usize], to: Target) -> Result<LineLayout, &'static str> {
    let src = tours.get(from).ok_or(NOT_FOUND)?;
    if picked.is_empty() {
        return Err(NOT_FOUND);
    }
    let idx = with_links(c, weekday, src, picked);
    let raw: Vec<&PlannedTrip> = idx.iter().map(|&k| src.trips.get(k).ok_or(NOT_FOUND)).collect::<Result<_, _>>()?;
    if raw.iter().any(|x| !x.counts() || x.origin.trim().is_empty()) {
        return Err(NOT_MOVABLE);
    }
    // (a trip cut on the way moves whole: its relief goes)
    let whole = wholes(src, &idx);
    let moving: Vec<&PlannedTrip> = whole.iter().collect();
    guard_block(c, weekday, tours, from)?;
    let line_trips: Vec<&PlannedTrip> = tours.iter().filter(|t| same(&t.line, &src.line)).flat_map(|t| t.trips.iter()).collect();
    let mut lay = layout_now(c, tours, &src.line, weekday);
    let refs: Vec<TripRef> = moving.iter().map(|x| TripRef::of(x)).collect();
    match to {
        Target::Block(i) => {
            let dst = tours.get(i).ok_or(NOT_FOUND)?;
            if i == from {
                return Err(SAME_BUS);
            }
            if !same(&dst.line, &src.line) {
                return Err(OTHER_LINE);
            }
            guard_block(c, weekday, tours, i)?;
            all_fit(&dst.trips, &moving, &line_trips)?;
            let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &dst.tour)).ok_or(NOT_FOUND)?;
            b.trips.extend(refs.iter().cloned());
        }
        Target::New => {
            if src.trips.iter().filter(|t| t.counts() && !t.is_second()).count() == moving.len() {
                return Err(ALONE);
            }
            all_fit(&[], &moving, &line_trips)?;
            lay.blocks.push(BlockLayout { tour: new_name(c, weekday, tours, &src.line), trips: refs.clone(), cuts: None, reliefs: Vec::new() });
        }
    }
    take_out(&mut lay, &src.tour, &refs, &line_trips)?;
    Ok(lay)
}

/// Move trip `trip` of block `from` to `to` (`plan_move`): kept for the weekday. The duties
/// joined with a piece of either bus are taken apart.
pub fn move_trip(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, trip: usize, to: Target) -> Result<(), &'static str> {
    move_trips(c, weekday, tours, from, &[trip], to)
}

/// Move trips `picked` of block `from` together to `to` (`plan_move_many`).
pub fn move_trips(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, picked: &[usize], to: Target) -> Result<(), &'static str> {
    let lay = plan_move_many(c, weekday, tours, from, picked, to)?;
    let src = &tours[from];
    let mut touched = vec![src.tour.as_str()];
    if let Target::Block(i) = to {
        touched.push(tours[i].tour.as_str());
    }
    unjoin_blocks(c, weekday, &src.line, &touched);
    save(c, lay);
    Ok(())
}

/// Take trips `picked` of block `from` off the bus onto the stack (RosterFlow's "Unassign"):
/// no bus runs them until they are assigned again. Its linked trips go along.
pub fn unassign(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, picked: &[usize]) -> Result<(), &'static str> {
    let src = tours.get(from).ok_or(NOT_FOUND)?;
    let idx = with_links(c, weekday, src, picked);
    let moving: Vec<&PlannedTrip> = idx.iter().filter_map(|&k| src.trips.get(k)).collect();
    if moving.is_empty() || moving.iter().any(|x| !x.counts() || x.origin.trim().is_empty()) {
        return Err(NOT_MOVABLE);
    }
    guard_block(c, weekday, tours, from)?;
    let line_trips: Vec<&PlannedTrip> = tours.iter().filter(|t| same(&t.line, &src.line)).flat_map(|t| t.trips.iter()).collect();
    let mut lay = layout_now(c, tours, &src.line, weekday);
    let refs: Vec<TripRef> = wholes(src, &idx).iter().map(TripRef::of).collect();
    lay.stack.extend(refs.iter().cloned());
    take_out(&mut lay, &src.tour, &refs, &line_trips)?;
    unjoin_blocks(c, weekday, &src.line, &[src.tour.as_str()]);
    save(c, lay);
    Ok(())
}

/// Whether trip `x` of the stack (of its line `line`) may go to `to` (`assign`).
pub fn plan_assign(c: &Company, weekday: u8, tours: &[TourOfDay], line: &str, x: &PlannedTrip, to: Target) -> Result<LineLayout, &'static str> {
    let r = TripRef::of(x);
    let mut lay = layout_now(c, tours, line, weekday);
    if !lay.stack.contains(&r) {
        return Err(NOT_FOUND);
    }
    let mut line_trips: Vec<&PlannedTrip> = tours.iter().filter(|t| same(&t.line, line)).flat_map(|t| t.trips.iter()).collect();
    line_trips.push(x);
    match to {
        Target::Block(i) => {
            let dst = tours.get(i).ok_or(NOT_FOUND)?;
            if !same(&dst.line, line) {
                return Err(OTHER_LINE);
            }
            guard_block(c, weekday, tours, i)?;
            fits_into(&dst.trips, x, &line_trips)?;
            let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &dst.tour)).ok_or(NOT_FOUND)?;
            b.trips.push(r.clone());
        }
        Target::New => lay.blocks.push(BlockLayout { tour: new_name(c, weekday, tours, line), trips: vec![r.clone()], cuts: None, reliefs: Vec::new() }),
    }
    lay.stack.retain(|t| *t != r);
    tidy(&mut lay, &line_trips);
    Ok(lay)
}

/// Put trip `x` of the stack on a bus again: a bus of its line, or a new one.
pub fn assign(c: &mut Company, weekday: u8, tours: &[TourOfDay], line: &str, x: &PlannedTrip, to: Target) -> Result<(), &'static str> {
    let lay = plan_assign(c, weekday, tours, line, x, to)?;
    if let Target::Block(i) = to {
        unjoin_blocks(c, weekday, line, &[tours[i].tour.as_str()]);
    }
    save(c, lay);
    Ok(())
}

/// How a swap takes the trips: those picked over their time, or everything from a trip on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swap {
    /// The trips picked of block `from` against what block `to` runs over their time.
    Span,
    /// Every trip of `from` from the first picked on, against every trip of `to` from then on.
    FromHere,
}

/// What swapping trips of block `from` with block `to` would make of the line (RosterFlow's
/// "Swap" and "Swap from here"): both buses keep what fits, or it is refused.
pub fn plan_swap(c: &Company, weekday: u8, tours: &[TourOfDay], from: usize, picked: &[usize], to: usize, how: Swap) -> Result<LineLayout, &'static str> {
    let src = tours.get(from).ok_or(NOT_FOUND)?;
    let dst = tours.get(to).ok_or(NOT_FOUND)?;
    if from == to {
        return Err(SAME_BUS);
    }
    if !same(&dst.line, &src.line) {
        return Err(OTHER_LINE);
    }
    guard_block(c, weekday, tours, from)?;
    guard_block(c, weekday, tours, to)?;
    let idx = with_links(c, weekday, src, picked);
    let picked_trips: Vec<&PlannedTrip> = idx.iter().filter_map(|&k| src.trips.get(k)).filter(|x| x.counts() && !x.origin.trim().is_empty()).collect();
    let Some(first) = picked_trips.iter().map(|x| x.dep).min() else { return Err(NOT_MOVABLE) };
    let last = picked_trips.iter().map(|x| x.arr).max().unwrap_or(first);
    let movable = |x: &PlannedTrip| x.counts() && !x.origin.trim().is_empty();
    // (the trips of a bus whole: a trip cut on the way is swapped whole, its relief gone)
    let of = |t: &TourOfDay, f: &dyn Fn(&PlannedTrip) -> bool| -> Vec<PlannedTrip> { wholes(t, &(0..t.trips.len()).filter(|&k| movable(&t.trips[k]) && f(&t.trips[k])).collect::<Vec<_>>()) };
    let (a, b): (Vec<PlannedTrip>, Vec<PlannedTrip>) = match how {
        Swap::Span => (wholes(src, &idx.iter().copied().filter(|&k| src.trips.get(k).is_some_and(movable)).collect::<Vec<_>>()), of(dst, &|x| x.dep < last && x.arr > first)),
        Swap::FromHere => (of(src, &|x| x.dep >= first), of(dst, &|x| x.dep >= first)),
    };
    let (a, b): (Vec<&PlannedTrip>, Vec<&PlannedTrip>) = (a.iter().collect(), b.iter().collect());
    let ra: Vec<TripRef> = a.iter().map(|x| TripRef::of(x)).collect();
    let rb: Vec<TripRef> = b.iter().map(|x| TripRef::of(x)).collect();
    let line_trips: Vec<&PlannedTrip> = tours.iter().filter(|t| same(&t.line, &src.line)).flat_map(|t| t.trips.iter()).collect();
    // (each bus's day as it would be: what it keeps, and what it takes over)
    let keep = |t: &TourOfDay, gone: &[TripRef]| -> Vec<PlannedTrip> { t.trips.iter().filter(|x| !gone.contains(&whole_ref(x))).cloned().collect() };
    all_fit(&keep(src, &ra), &b, &line_trips).map_err(|_| SWAP_FAILS)?;
    all_fit(&keep(dst, &rb), &a, &line_trips).map_err(|_| SWAP_FAILS)?;
    let mut lay = layout_now(c, tours, &src.line, weekday);
    for (tour, gone, comes) in [(&src.tour, &ra, &rb), (&dst.tour, &rb, &ra)] {
        let blk = lay.blocks.iter_mut().find(|x| same(&x.tour, tour)).ok_or(NOT_FOUND)?;
        blk.trips.retain(|t| !gone.contains(t));
        if let Some(cuts) = blk.cuts.as_mut() {
            cuts.retain(|t| !gone.contains(t));
        }
        blk.reliefs.retain(|r| !gone.contains(&r.trip));
        blk.trips.extend(comes.iter().cloned());
    }
    tidy(&mut lay, &line_trips);
    Ok(lay)
}

/// Swap trips of block `from` with block `to` (`plan_swap`): kept for the weekday.
pub fn swap(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, picked: &[usize], to: usize, how: Swap) -> Result<(), &'static str> {
    let lay = plan_swap(c, weekday, tours, from, picked, to, how)?;
    unjoin_blocks(c, weekday, &tours[from].line, &[tours[from].tour.as_str(), tours[to].tour.as_str()]);
    save(c, lay);
    Ok(())
}

// --- trips moved between duties (RosterFlow's Drivers view) ------------------------------------
//
// Luc: in Drivers a trip is dragged onto another duty, or onto a new one, as a trip is dragged
// onto another bus in Vehicles. The bus keeps running its block; only the driver of the trip
// changes. Its block is cut anew where the driver changes now - only where the bus stands at a
// terminus or the depot - and joined where one driver now works on (a cut between two pieces of
// the same duty goes); the duties are joined anew from their pieces, and the working-time rules
// (`rules_of`) and the driver's way to the trip are checked.

/// Why trips are not moved to another duty (English, translated when shown).
pub const ONE_DUTY: &str = "Only trips of one duty can be moved together.";
pub const DUTY_ALONE: &str = "These are all the trips of this duty: it is a duty of its own already.";
pub const DRIVER_NO_TIME: &str = "The driver cannot be there in time: the work before it ends at another stop, too shortly before.";

/// Where trips of a duty are moved to in the Drivers view: another duty of the day (its index
/// in `duties`), or a new duty of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DutyTarget {
    Duty(usize),
    New,
}

/// What moving trips to another duty makes of the planning (`plan_move_to_duty`): the layouts
/// of the lines whose blocks are cut anew, and the company's joined duties and duty notes
/// (every weekday's) as they are then. `apply_duty_move` keeps it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DutyMove {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub notes: Vec<super::rosterflow::DutyNote>,
}

/// The duty (an index into `duties`) that works trip `j` of block `i`.
pub fn duty_of_trip(duties: &[Duty], tours: &[TourOfDay], i: usize, j: usize) -> Option<usize> {
    let k = network::duties_of(tours.get(i)?).iter().position(|r| r.contains(&j))?;
    duty_of(duties, i, k)
}

/// Whether drivers can change between trips `a` and `b` of a block of line `line` (`b` right
/// after `a`): where the bus stands between them at a terminus or the depot (`NOT_RELIEF` where
/// it stands elsewhere). Where `b` begins at another place than `a` ends and no run is listed
/// between two trips with passengers (an own line's way back, a deadhead whose time is
/// estimated: Luc's line 880 runs one way only), the bus stands where the first one ended
/// before it runs empty: the drivers change there, or where the next one begins, at a terminus
/// or the depot. Between an empty run and another place it is on the move (`ON_THE_MOVE`).
fn change_between(c: &Company, tours: &[TourOfDay], line: &str, a: &PlannedTrip, b: &PlannedTrip) -> Result<(), &'static str> {
    if same(a.end(), &b.from) {
        return if is_relief(c, tours, line, a.end()) { Ok(()) } else { Err(NOT_RELIEF) };
    }
    if a.counts() && b.counts() && (is_relief(c, tours, line, a.end()) || is_relief(c, tours, line, &b.from)) {
        return Ok(());
    }
    Err(ON_THE_MOVE)
}

/// Whether drivers may change between trips `a` and `b` of a block of line `line`, by the rule a
/// cut by hand keeps (`change_between`): Optimise cuts the blocks by it too.
pub fn may_change(c: &Company, tours: &[TourOfDay], line: &str, a: &PlannedTrip, b: &PlannedTrip) -> bool {
    change_between(c, tours, line, a, b).is_ok()
}

/// Where between trips `p` and `n` of block `i` (trips with passengers, none between them) the
/// driver can change: before a trip after which the bus stands at a terminus or the depot
/// (`change_between`), the nearest `p` first (`near_p`) or the nearest `n`.
fn relief_between(c: &Company, tours: &[TourOfDay], i: usize, p: usize, n: usize, near_p: bool) -> Result<usize, &'static str> {
    let t = &tours[i];
    let mut order: Vec<usize> = (p + 1..=n).collect();
    if !near_p {
        order.reverse();
    }
    let mut standing = false;
    for k in order {
        match change_between(c, tours, &t.line, &t.trips[k - 1], &t.trips[k]) {
            Ok(()) => return Ok(k),
            Err(e) => standing |= e == NOT_RELIEF,
        }
    }
    Err(if standing { NOT_RELIEF } else { ON_THE_MOVE })
}

/// A piece of work (trips `r` of block `i`) as the working-time rules see it.
fn range_block(tours: &[TourOfDay], i: usize, r: &Range<usize>) -> Block {
    let t = &tours[i];
    let (from, to) = span(t, r);
    let from_stop = t.trips.get(r.start).map(|x| x.from.clone()).unwrap_or_default();
    let to_stop = r.end.checked_sub(1).and_then(|e| t.trips.get(e)).map(|x| x.end().to_string()).unwrap_or_default();
    Block { key: format!("{i}/{}", r.start), from, to, from_stop, to_stop }
}

/// The pieces of duty `d` (block, trips) as `who` gives each trip's duty and `pieces` each
/// block's pieces, in the order they begin.
fn pieces_of(tours: &[TourOfDay], who: &[Vec<usize>], pieces: &[Vec<Range<usize>>], d: usize) -> Vec<(usize, Range<usize>)> {
    let mut out: Vec<(usize, Range<usize>)> = Vec::new();
    for (i, rs) in pieces.iter().enumerate() {
        for r in rs.iter().filter(|r| who[i].get(r.start) == Some(&d)) {
            out.push((i, r.clone()));
        }
    }
    out.sort_by_key(|(i, r)| (span(&tours[*i], r).0, *i));
    out
}

/// What moving trips `trips` (block, trip: indices into `tours`) of duty `from` to `to` would
/// make of the planning (None: refused, and why). `from` and `DutyTarget::Duty` are indices into
/// `duties(&c.planning.joined, weekday, tours)` - `rosterflow::Board::duties` when `c` is the
/// board's company (`rosterflow::view`) and `tours` its blocks. The trips stay on their buses: their
/// blocks are cut where the driver changes now - only where the bus stands at a terminus or the
/// depot (`NOT_RELIEF`, `ON_THE_MOVE`) - and a cut between two pieces one driver now works on
/// one bus goes. Every duty changed keeps to the working-time rules (`rules_of`: a hard rule it
/// did not break before refuses the move with the rule's label), and a driver gets from one
/// piece to the next in time (`DRIVER_NO_TIME`). Locked duties and buses stay as they are.
pub fn plan_move_to_duty(c: &Company, weekday: u8, tours: &[TourOfDay], from: usize, trips: &[(usize, usize)], to: DutyTarget) -> Result<DutyMove, &'static str> {
    use super::rosterflow::{block_locked, duty_key, duty_locked, DutyNote};
    let ds = duties(&c.planning.joined, weekday, tours);
    if from >= ds.len() {
        return Err(NOT_FOUND);
    }
    let mut picked: Vec<(usize, usize)> = trips.to_vec();
    picked.sort_unstable();
    picked.dedup();
    if picked.is_empty() {
        return Err(NOT_FOUND);
    }
    // who works each trip now (a duty of `ds`), and the blocks' pieces
    let now: Vec<Vec<Range<usize>>> = tours.iter().map(network::duties_of).collect();
    let mut owner: Vec<Vec<usize>> = tours.iter().map(|t| vec![usize::MAX; t.trips.len()]).collect();
    for (d, duty) in ds.iter().enumerate() {
        for p in &duty.pieces {
            for j in now[p.tour].get(p.duty).cloned().unwrap_or(0..0) {
                owner[p.tour][j] = d;
            }
        }
    }
    for &(i, j) in &picked {
        let x = tours.get(i).and_then(|t| t.trips.get(j)).ok_or(NOT_FOUND)?;
        if !x.counts() {
            return Err(NOT_MOVABLE);
        }
        if owner[i][j] != from {
            return Err(ONE_DUTY);
        }
    }
    let target = match to {
        DutyTarget::Duty(d) if d == from => return Err(SAME_DUTY),
        DutyTarget::Duty(d) if d >= ds.len() => return Err(NOT_FOUND),
        DutyTarget::Duty(d) => d,
        DutyTarget::New => {
            let mut all = 0;
            for p in &ds[from].pieces {
                let r = now[p.tour].get(p.duty).cloned().unwrap_or(0..0);
                all += tours[p.tour].trips[r].iter().filter(|x| x.counts()).count();
            }
            if picked.len() >= all {
                return Err(DUTY_ALONE);
            }
            ds.len()
        }
    };
    let locked = |d: usize| d < ds.len() && duty_key(tours, &ds[d]).is_some_and(|k| duty_locked(c, weekday, &k));
    if locked(from) || locked(target) {
        return Err(DUTY_LOCKED);
    }
    let mut blocks: Vec<usize> = picked.iter().map(|p| p.0).collect();
    blocks.dedup();
    if blocks.iter().any(|&i| block_locked(c, weekday, &tours[i].line, &tours[i].tour)) {
        return Err(LOCKED);
    }
    // the trips to their new driver; their blocks cut where the driver changes now
    let mut after = owner.clone();
    for &(i, j) in &picked {
        after[i][j] = target;
    }
    let mut pieces = now.clone();
    let mut cut_at: Vec<(usize, Vec<usize>)> = Vec::new();
    for &i in &blocks {
        let t = &tours[i];
        let old: Vec<usize> = now[i].iter().skip(1).map(|r| r.start).collect();
        let counting: Vec<usize> = (0..t.trips.len()).filter(|&j| t.trips[j].counts()).collect();
        let mut cuts: Vec<usize> = Vec::new();
        for w in counting.windows(2) {
            let (p, n) = (w[0], w[1]);
            if after[i][p] == after[i][n] {
                continue;
            }
            // (a cut there already stays where it is)
            let k = match old.iter().copied().find(|&k| k > p && k <= n) {
                Some(k) => k,
                None => relief_between(c, tours, i, p, n, after[i][p] != owner[i][p])?,
            };
            cuts.push(k);
        }
        let mut ranges: Vec<Range<usize>> = Vec::new();
        let mut s = 0;
        for k in cuts.iter().copied().chain(std::iter::once(t.trips.len())) {
            ranges.push(s..k);
            s = k;
        }
        // (the runs without passengers go with the trips of their piece)
        for r in &ranges {
            let who = r.clone().find(|&j| t.trips[j].counts()).map(|j| after[i][j]).unwrap_or(usize::MAX);
            for j in r.clone() {
                after[i][j] = who;
            }
        }
        pieces[i] = ranges;
        cut_at.push((i, cuts));
    }
    // every duty with work on those buses, the one the trips go to first
    let mut touched: Vec<usize> = vec![target, from];
    for &i in &blocks {
        for j in 0..tours[i].trips.len() {
            for d in [owner[i][j], after[i][j]] {
                if d != usize::MAX && !touched.contains(&d) {
                    touched.push(d);
                }
            }
        }
    }
    for &d in &touched {
        let new = pieces_of(tours, &after, &pieces, d);
        if new.is_empty() {
            continue;
        }
        let old = if d < ds.len() { pieces_of(tours, &owner, &now, d) } else { Vec::new() };
        let was: &[Rule] = if d < ds.len() { &ds[d].rules } else { &[] };
        let bs: Vec<Block> = new.iter().map(|(i, r)| range_block(tours, *i, r)).collect();
        if let Some(r) = rules_of(&bs).into_iter().find(|r| r.hard() && !was.iter().any(|w| std::mem::discriminant(w) == std::mem::discriminant(r))) {
            return Err(r.label());
        }
        // (from one piece to the next: at the same stop, or with time to get there)
        for (w, b) in new.windows(2).zip(bs.windows(2)) {
            if old.windows(2).any(|o| o == w) {
                continue;
            }
            // (the way between them as the relief points have it: `rosterflow::travel_between`)
            let need = super::rosterflow::travel_between(c, tours, &b[0].to_stop, &b[1].from_stop, None).1;
            if !same(&b[0].to_stop, &b[1].from_stop) && b[1].from - b[0].to < need {
                return Err(DRIVER_NO_TIME);
            }
        }
    }
    // the lines' layouts with the blocks cut anew
    let mut layouts: Vec<LineLayout> = Vec::new();
    for (i, cuts) in &cut_at {
        let t = &tours[*i];
        if !layouts.iter().any(|l| same(&l.line, &t.line)) {
            layouts.push(layout_now(c, tours, &t.line, weekday));
        }
        let lay = layouts.iter_mut().find(|l| same(&l.line, &t.line)).ok_or(NOT_FOUND)?;
        let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &t.tour)).ok_or(NOT_FOUND)?;
        // (a relief on the way one driver now works on both sides of: the trip whole again)
        set_block_cuts(b, t, cuts);
    }
    // the duties changed joined anew; their notes (type, custom work ...) follow their key
    let refs = |ps: &[(usize, Range<usize>)]| -> Vec<PieceRef> { ps.iter().map(|(i, r)| PieceRef::of(&tours[*i], r)).collect() };
    let mut gone: Vec<PieceRef> = Vec::new();
    let mut groups: Vec<Vec<PieceRef>> = Vec::new();
    let mut keys: Vec<(PieceRef, Option<PieceRef>)> = Vec::new();
    for &d in &touched {
        let new = refs(&pieces_of(tours, &after, &pieces, d));
        if d < ds.len() {
            gone.extend(refs(&pieces_of(tours, &owner, &now, d)));
            if let Some(k) = duty_key(tours, &ds[d]) {
                keys.push((k, new.first().cloned()));
            }
        }
        gone.extend(new.iter().cloned());
        if new.len() > 1 {
            groups.push(new);
        }
    }
    let mut joined: Vec<JoinedDuty> = c.planning.joined.iter().filter(|j| !(j.weekday == weekday && j.pieces.iter().any(|p| gone.iter().any(|g| g.same(p))))).cloned().collect();
    joined.extend(groups.into_iter().map(|pieces| JoinedDuty { weekday, pieces }));
    let mut notes: Vec<DutyNote> = Vec::new();
    for n in &c.planning.flow.sheet.notes {
        if n.weekday != weekday {
            notes.push(n.clone());
            continue;
        }
        match keys.iter().find(|(old, _)| old.same(&n.key)) {
            Some((_, Some(new))) => notes.push(DutyNote { key: new.clone(), ..n.clone() }),
            // (its duty is gone: all its trips went to another)
            Some((_, None)) => {}
            // (a note left on a key another duty has now)
            None if keys.iter().any(|(_, new)| new.as_ref().is_some_and(|k| k.same(&n.key))) => {}
            None => notes.push(n.clone()),
        }
    }
    Ok(DutyMove { layouts, joined, notes })
}

/// Keep what `plan_move_to_duty` made of the planning.
pub fn apply_duty_move(c: &mut Company, m: DutyMove) {
    for l in m.layouts {
        save(c, l);
    }
    c.planning.joined = m.joined;
    c.planning.flow.sheet.notes = m.notes;
}

/// Move trips `trips` (block, trip) of duty `from` to `to` (`plan_move_to_duty`): kept for the
/// weekday.
pub fn move_to_duty(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, trips: &[(usize, usize)], to: DutyTarget) -> Result<(), &'static str> {
    let m = plan_move_to_duty(c, weekday, tours, from, trips, to)?;
    apply_duty_move(c, m);
    Ok(())
}

// --- trips of several duties moved at once ------------------------------------------------------
//
// Testers: "Multi-selected duties in the driver tab can be selected all together, but not dragged
// up or down". Trips chosen on several rows of the Drivers tab are dragged together: each duty's
// trips go to the duty as many rows up or down as the one taken hold of goes.

/// Why trips of several duties are not moved together (English, translated when shown).
pub const PAST_THE_ROWS: &str = "Not every duty chosen has a duty that many rows up or down.";
pub const NOT_A_DUTY: &str = "A spare duty of custom work takes no trips.";

/// One duty's trips in a move of several duties' trips at once (`move_to_duties`): its trips
/// (block, trip) and where they go.
#[derive(Clone, Debug, PartialEq)]
pub struct DutyGroup {
    pub trips: Vec<(usize, usize)>,
    pub to: DutyTarget,
}

/// A trip of the day by what it is - its line, its block and itself - not by its place: a move
/// cuts blocks anew and may make a trip cut on the way whole again.
type TripKey = (String, String, TripRef);

fn key_of(tours: &[TourOfDay], (i, j): (usize, usize)) -> Option<TripKey> {
    let t = tours.get(i)?;
    Some((t.line.clone(), t.tour.clone(), TripRef::of(t.trips.get(j)?)))
}

/// Where trip `k` is in `tours` now (a part of a trip made whole again: the whole trip).
fn place_of(tours: &[TourOfDay], k: &TripKey) -> Option<(usize, usize)> {
    let i = tours.iter().position(|t| same(&t.line, &k.0) && same(&t.tour, &k.1))?;
    let t = &tours[i];
    let j = t.trips.iter().position(|x| k.2.is(x)).or_else(|| t.trips.iter().position(|x| x.name == k.2.name && x.origin.trim() == k.2.tour.trim() && x.dep <= k.2.dep && k.2.dep < x.arr))?;
    Some((i, j))
}

/// Move the trips of several duties at once, each duty's to its own target (`DutyGroup`; the
/// targets' `DutyTarget::Duty` indices into the day's duties as `c` and `tours` have them). Each
/// group is moved in turn on a copy, as `plan_move_to_duty` checks and makes it; `again` gives the
/// day's blocks of the copy as it is then (`network::tours_of_day`), and every trip and target is
/// found anew by what it is. A target is the duty that works its trips not moved themselves (all
/// moved away: a duty of their own); a group whose duty is another's target goes first, so a duty
/// gives its trips away before it takes others'. The first group refused refuses them all: its
/// index and why. Nothing is changed then.
pub fn move_to_duties(c: &mut Company, weekday: u8, tours: &[TourOfDay], again: &dyn Fn(&Company) -> Vec<TourOfDay>, groups: &[DutyGroup]) -> Result<(), (usize, &'static str)> {
    let ds = duties(&c.planning.joined, weekday, tours);
    let all_moved: Vec<(usize, usize)> = groups.iter().flat_map(|g| g.trips.iter().copied()).collect();
    // each group's duty, target, trips and the target's trips that stay with it, by what they are
    let mut plans: Vec<(usize, Option<usize>, Vec<TripKey>, Vec<TripKey>)> = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        let &(i, j) = g.trips.first().ok_or((gi, NOT_FOUND))?;
        let from = duty_of_trip(&ds, tours, i, j).ok_or((gi, NOT_FOUND))?;
        let keys: Vec<TripKey> = g.trips.iter().filter_map(|&p| key_of(tours, p)).collect();
        if keys.len() != g.trips.len() {
            return Err((gi, NOT_FOUND));
        }
        let (to, stays) = match g.to {
            DutyTarget::Duty(d) => {
                let duty = ds.get(d).ok_or((gi, NOT_FOUND))?;
                let mut stays = Vec::new();
                for p in &duty.pieces {
                    for j in network::duties_of(&tours[p.tour]).get(p.duty).cloned().unwrap_or(0..0) {
                        if tours[p.tour].trips[j].counts() && !all_moved.contains(&(p.tour, j)) {
                            stays.extend(key_of(tours, (p.tour, j)));
                        }
                    }
                }
                (Some(d), stays)
            }
            DutyTarget::New => (None, Vec::new()),
        };
        plans.push((from, to, keys, stays));
    }
    // the order: a duty's own trips away before others' come to it
    let mut order: Vec<usize> = Vec::new();
    while order.len() < plans.len() {
        let left: Vec<usize> = (0..plans.len()).filter(|g| !order.contains(g)).collect();
        let ready = left.iter().copied().find(|&g| !left.iter().any(|&h| h != g && Some(plans[h].0) == plans[g].1)).unwrap_or(left[0]);
        order.push(ready);
    }
    let mut work = c.clone();
    let mut cur = tours.to_vec();
    for gi in order {
        let (_, to, keys, stays) = &plans[gi];
        let ds_now = duties(&work.planning.joined, weekday, &cur);
        let picked: Vec<(usize, usize)> = keys.iter().filter_map(|k| place_of(&cur, k)).collect();
        let Some(&(i, j)) = picked.first() else { return Err((gi, NOT_FOUND)) };
        let from = duty_of_trip(&ds_now, &cur, i, j).ok_or((gi, NOT_FOUND))?;
        let target = match to {
            Some(_) => match stays.iter().filter_map(|k| place_of(&cur, k)).find_map(|(a, b)| duty_of_trip(&ds_now, &cur, a, b)) {
                Some(d) => DutyTarget::Duty(d),
                None => DutyTarget::New,
            },
            None => DutyTarget::New,
        };
        match plan_move_to_duty(&work, weekday, &cur, from, &picked, target) {
            Ok(m) => apply_duty_move(&mut work, m),
            // (its target gone with all its trips, and these all of their duty: where they are)
            Err(DUTY_ALONE) if to.is_some() => continue,
            Err(e) => return Err((gi, e)),
        }
        cur = again(&work);
    }
    *c = work;
    Ok(())
}

/// Whether drivers may change at `place` (of a block of line `_line`): a relief point - a
/// terminus of the day's lines, the depot or a stop the player added (`rosterflow::uses_at`)
/// - that allows relief.
pub fn is_relief(c: &Company, tours: &[TourOfDay], _line: &str, place: &str) -> bool {
    super::rosterflow::uses_at(c, tours, place).is_some_and(|u| u.relief)
}

/// The trips block `t`'s duties begin with now (the first left out).
fn starts(t: &TourOfDay) -> Vec<usize> {
    network::duties_of(t).iter().skip(1).map(|r| r.start).collect()
}

/// The block's cuts kept as these starts.
fn set_cuts(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: &[usize]) {
    let t = &tours[i];
    let mut lay = layout_now(c, tours, &t.line, weekday);
    if let Some(b) = lay.blocks.iter_mut().find(|b| same(&b.tour, &t.tour)) {
        // (a relief on the way no duty begins at any more: the trip whole again)
        set_block_cuts(b, t, at);
    }
    unjoin_blocks(c, weekday, &t.line, &[t.tour.as_str()]);
    save(c, lay);
}

/// Whether block `i` may be cut before its trip `at` (a driver takes over there).
pub fn can_cut(c: &Company, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(NOT_FOUND)?;
    if at == 0 || at >= t.trips.len() {
        return Err(BETWEEN);
    }
    let pieces = network::duties_of(t);
    if pieces.iter().any(|r| r.start == at) {
        return Err(CUT_ALREADY);
    }
    change_between(c, tours, &t.line, &t.trips[at - 1], &t.trips[at])?;
    let p = pieces.iter().find(|r| r.contains(&at)).ok_or(NOT_FOUND)?;
    if !t.trips[p.start..at].iter().any(PlannedTrip::counts) || !t.trips[at..p.end].iter().any(PlannedTrip::counts) {
        return Err(NO_PASSENGERS);
    }
    Ok(())
}

/// Cut block `i` before its trip `at`: a new duty begins there (`can_cut`).
pub fn cut(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    can_cut(c, tours, i, at)?;
    guard_block(c, weekday, tours, i)?;
    let mut s = starts(&tours[i]);
    s.push(at);
    s.sort_unstable();
    set_cuts(c, weekday, tours, i, &s);
    Ok(())
}

// --- reliefs on the way (RosterFlow's mid-trip relief) ------------------------------------------
//
// Luc (2026-10-14): drivers may change halfway along a trip too, at a relief point that allows it
// (`rosterflow::ReliefPoint`). The trip is cut in two at that stop (`BlockLayout::reliefs`,
// `split_trip`): the bus runs it whole, the first part is the duty before's and the second the
// next one's, at the minute the timetable has the bus there.

/// Why a trip is not cut on the way (English, translated when shown).
pub const NOT_ON_THE_WAY: &str = "That stop is not on the way between the trip's first and last stop.";
pub const MID_ALREADY: &str = "This trip has a relief on the way already.";

/// The stops on the way of trip `j` of block `i` (`info` the timetable read) drivers may change
/// at, or why not: (its place in the trip's stops, its name, the minute the bus is there, Ok or
/// the reason). Only the stops that are relief points (`rosterflow::uses_at`).
pub fn mid_stops(c: &Company, tours: &[TourOfDay], info: &[LineInfo], i: usize, j: usize) -> Vec<MidStop> {
    mids_with(tours, info, i, j, &mut |s| super::rosterflow::uses_at(c, tours, s))
}

/// A stop on the way a trip may be cut at, or why not (`mid_stops`).
pub type MidStop = (usize, String, i32, Result<(), &'static str>);

/// Every trip's stops on the way of the day (`mid_stops`), each relief point looked up once:
/// (block, trip, its stops).
pub fn day_mid_stops(c: &Company, tours: &[TourOfDay], info: &[LineInfo]) -> Vec<(usize, usize, Vec<MidStop>)> {
    let mut known: std::collections::HashMap<String, Option<super::rosterflow::Uses>> = std::collections::HashMap::new();
    let mut uses = |s: &str| *known.entry(s.trim().to_lowercase()).or_insert_with(|| super::rosterflow::uses_at(c, tours, s));
    let mut out = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        for j in 0..t.trips.len() {
            let m = mids_with(tours, info, i, j, &mut uses);
            if !m.is_empty() {
                out.push((i, j, m));
            }
        }
    }
    out
}

fn mids_with(tours: &[TourOfDay], info: &[LineInfo], i: usize, j: usize, uses: &mut dyn FnMut(&str) -> Option<super::rosterflow::Uses>) -> Vec<MidStop> {
    let Some(t) = tours.get(i) else { return Vec::new() };
    let Some(x) = t.trips.get(j).filter(|x| x.counts() && x.part.is_none() && !x.origin.trim().is_empty()) else { return Vec::new() };
    let Some(y) = info.iter().find(|l| same(&l.name, &t.line)).and_then(|l| trip_info(l, x)) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, s) in y.stops.iter().enumerate().skip(1).take(y.stops.len().saturating_sub(2)) {
        let Some(u) = uses(&s.name) else { continue };
        let when = (s.dep / 60.0).round() as i32;
        let ok = if !u.relief { Err(NOT_RELIEF) } else if when <= x.dep || when >= x.arr { Err(NOT_ON_THE_WAY) } else { Ok(()) };
        out.push((k, s.name.trim().to_string(), when, ok));
    }
    out
}

/// Whether block `i` may be cut inside its trip `j`, at its stop `stop` (a driver takes over on
/// the way there).
pub fn can_cut_mid(c: &Company, tours: &[TourOfDay], info: &[LineInfo], i: usize, j: usize, stop: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(NOT_FOUND)?;
    let x = t.trips.get(j).ok_or(NOT_FOUND)?;
    if x.part.is_some() {
        return Err(MID_ALREADY);
    }
    if !x.counts() || x.origin.trim().is_empty() {
        return Err(NOT_MOVABLE);
    }
    match mid_stops(c, tours, info, i, j).into_iter().find(|m| m.0 == stop) {
        Some((_, _, _, ok)) => ok,
        None => Err(NOT_RELIEF),
    }
}

/// Cut block `i` inside its trip `j` at its stop `stop` (`can_cut_mid`): a new duty begins there,
/// the duties the block's pieces were joined into taken apart (their pieces change).
pub fn cut_mid(c: &mut Company, weekday: u8, tours: &[TourOfDay], info: &[LineInfo], i: usize, j: usize, stop: usize) -> Result<(), &'static str> {
    can_cut_mid(c, tours, info, i, j, stop)?;
    guard_block(c, weekday, tours, i)?;
    let t = &tours[i];
    let x = &t.trips[j];
    let name = trip_info(info.iter().find(|l| same(&l.name, &t.line)).ok_or(NOT_FOUND)?, x).and_then(|y| y.stops.get(stop)).map(|s| s.name.trim().to_string()).ok_or(NOT_FOUND)?;
    let mut lay = layout_now(c, tours, &t.line, weekday);
    let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &t.tour)).ok_or(NOT_FOUND)?;
    // (the cuts as they are now, the automatic ones too, and the relief on the way)
    set_block_cuts(b, t, &starts(t));
    b.reliefs.push(MidRelief { trip: whole_ref(x), stop, name });
    unjoin_blocks(c, weekday, &t.line, &[t.tour.as_str()]);
    save(c, lay);
    Ok(())
}

/// Join the duty of block `i` that begins with trip `at` to the one before it: the cut taken
/// out, unless the duty would be longer than `DAY_MAX`. A relief on the way goes so: the trip
/// is whole again.
pub fn uncut(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(NOT_FOUND)?;
    guard_block(c, weekday, tours, i)?;
    let pieces = network::duties_of(t);
    let k = pieces.iter().position(|r| r.start == at && at > 0).ok_or(NO_CUT)?;
    let (a, b) = (&pieces[k - 1], &pieces[k]);
    let to = t.trips[a.start..b.end].iter().map(|x| x.arr).max().unwrap_or(0);
    if to - t.trips[a.start].dep > DAY_MAX {
        return Err(TOO_LONG);
    }
    let s: Vec<usize> = starts(t).into_iter().filter(|&x| x != at).collect();
    set_cuts(c, weekday, tours, i, &s);
    Ok(())
}

/// Join piece `b` (block, duty) into the duty of piece `a`: next to each other on one bus the
/// cut between them goes (`uncut`); else one driver works both - a split duty, or one that
/// changes buses - unless the working-time rules forbid it.
pub fn join(c: &mut Company, weekday: u8, tours: &[TourOfDay], a: (usize, usize), b: (usize, usize)) -> Result<(), &'static str> {
    if a == b {
        return Err(SAME_DUTY);
    }
    for p in [a, b] {
        if super::rosterflow::piece_locked(c, weekday, tours, p.0, p.1) {
            return Err(DUTY_LOCKED);
        }
    }
    if a.0 == b.0 && a.1.abs_diff(b.1) == 1 {
        let t = tours.get(a.0).ok_or(NOT_FOUND)?;
        let later = network::duties_of(t).get(a.1.max(b.1)).ok_or(NOT_FOUND)?.start;
        return uncut(c, weekday, tours, a.0, later);
    }
    let piece = |p: (usize, usize)| -> Option<PieceRef> {
        let t = tours.get(p.0)?;
        Some(PieceRef::of(t, network::duties_of(t).get(p.1)?))
    };
    let (pa, pb) = (piece(a).ok_or(NOT_FOUND)?, piece(b).ok_or(NOT_FOUND)?);
    let group_of = |p: &PieceRef| c.planning.joined.iter().position(|j| j.weekday == weekday && j.pieces.iter().any(|x| x.same(p)));
    let (ga, gb) = (group_of(&pa), group_of(&pb));
    if ga.is_some() && ga == gb {
        return Err(SAME_DUTY);
    }
    let mut pieces: Vec<PieceRef> = Vec::new();
    for (g, p) in [(ga, &pa), (gb, &pb)] {
        match g {
            Some(g) => pieces.extend(c.planning.joined[g].pieces.iter().cloned()),
            None => pieces.push(p.clone()),
        }
    }
    let found: Vec<(usize, usize)> = pieces.iter().filter_map(|p| find_piece(tours, p)).collect();
    let blocks: Vec<Block> = found.iter().map(|&(i, k)| piece_block(tours, i, k)).collect();
    if let Some(r) = rules_of(&blocks).into_iter().find(|r| r.hard()) {
        return Err(r.label());
    }
    let keep: Vec<usize> = [ga, gb].into_iter().flatten().collect();
    let mut k = 0;
    c.planning.joined.retain(|_| {
        let gone = keep.contains(&k);
        k += 1;
        !gone
    });
    c.planning.joined.push(JoinedDuty { weekday, pieces });
    Ok(())
}

/// Whether piece `b` may be joined into the duty of piece `a` (`join`, nothing kept).
pub fn can_join(c: &Company, weekday: u8, tours: &[TourOfDay], a: (usize, usize), b: (usize, usize)) -> Result<(), &'static str> {
    let mut probe = c.clone();
    join(&mut probe, weekday, tours, a, b)
}

/// Take apart the duty piece (block, duty) was joined into: each piece a duty of its own.
pub fn unjoin(c: &mut Company, weekday: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    let t = tours.get(p.0).ok_or(NOT_FOUND)?;
    let me = PieceRef::of(t, network::duties_of(t).get(p.1).ok_or(NOT_FOUND)?);
    if super::rosterflow::piece_locked(c, weekday, tours, p.0, p.1) {
        return Err(DUTY_LOCKED);
    }
    let before = c.planning.joined.len();
    c.planning.joined.retain(|j| !(j.weekday == weekday && j.pieces.iter().any(|x| x.same(&me))));
    if c.planning.joined.len() == before {
        return Err(NOT_JOINED);
    }
    Ok(())
}

/// Take piece (block, duty) out of the duty it was joined into: a duty of its own, the other
/// pieces staying together (RosterFlow's "Remove piece", and a duty begun anew from it).
pub fn detach(c: &mut Company, weekday: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    let t = tours.get(p.0).ok_or(NOT_FOUND)?;
    let me = PieceRef::of(t, network::duties_of(t).get(p.1).ok_or(NOT_FOUND)?);
    if super::rosterflow::piece_locked(c, weekday, tours, p.0, p.1) {
        return Err(DUTY_LOCKED);
    }
    let g = c.planning.joined.iter().position(|j| j.weekday == weekday && j.pieces.iter().any(|x| x.same(&me))).ok_or(NOT_JOINED)?;
    c.planning.joined[g].pieces.retain(|x| !x.same(&me));
    if c.planning.joined[g].pieces.len() < 2 {
        c.planning.joined.remove(g);
    }
    Ok(())
}

/// The key a piece is known by (`rosterflow`'s duty notes).
pub fn piece_ref(tours: &[TourOfDay], i: usize, k: usize) -> Option<PieceRef> {
    let t = tours.get(i)?;
    Some(PieceRef::of(t, network::duties_of(t).get(k)?))
}

/// The piece known by `p` in the day: (block, duty).
pub fn piece_at(tours: &[TourOfDay], p: &PieceRef) -> Option<(usize, usize)> {
    find_piece(tours, p)
}

impl PieceRef {
    /// Whether it is the same piece as `o` (line and block named alike, the same first trip).
    pub fn is(&self, o: &PieceRef) -> bool {
        self.same(o)
    }
}

/// Block `i` may be changed: it is not locked, and no duty with a piece of it is.
fn guard_block(c: &Company, weekday: u8, tours: &[TourOfDay], i: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(NOT_FOUND)?;
    if super::rosterflow::block_locked(c, weekday, &t.line, &t.tour) {
        return Err(LOCKED);
    }
    if (0..network::duties_of(t).len()).any(|k| super::rosterflow::piece_locked(c, weekday, tours, i, k)) {
        return Err(DUTY_LOCKED);
    }
    Ok(())
}

// --- the duties and the rules ----------------------------------------------------------------

/// A working-time rule a duty breaks or comes near (`staff::check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Two of its pieces at the same time.
    Overlap,
    /// Minutes of rest between two pieces, under `REST`.
    ShortRest(i32),
    /// Minutes of work, over `DAY_MAX`.
    TooLong(i32),
    /// Minutes over `DAY_TARGET`.
    Overtime(i32),
    /// Minutes to change to another stop, under `staff::TRANSFER`.
    StopChange(i32),
    /// Minutes of a split duty's break, over `LONG_SPLIT`.
    LongSplit(i32),
}

impl Rule {
    pub const LABELS: [&'static str; 6] = ["Overlaps other work", "Too short a rest between two pieces", "Longer than a working day may be", "Overtime", "A change of stops", "A long split"];

    /// Forbidden (refused when editing); the others are warnings.
    pub fn hard(self) -> bool {
        matches!(self, Rule::Overlap | Rule::ShortRest(_) | Rule::TooLong(_))
    }

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        let k = match self {
            Rule::Overlap => 0,
            Rule::ShortRest(_) => 1,
            Rule::TooLong(_) => 2,
            Rule::Overtime(_) => 3,
            Rule::StopChange(_) => 4,
            Rule::LongSplit(_) => 5,
        };
        Self::LABELS[k]
    }

    /// Its minutes (none for an overlap).
    pub fn minutes(self) -> Option<i32> {
        match self {
            Rule::Overlap => None,
            Rule::ShortRest(m) | Rule::TooLong(m) | Rule::Overtime(m) | Rule::StopChange(m) | Rule::LongSplit(m) => Some(m),
        }
    }
}

/// The rules a driver's day of these pieces breaks or comes near, the forbidden first.
pub fn rules_of(blocks: &[Block]) -> Vec<Rule> {
    let mut out: Vec<Rule> = Vec::new();
    let mut add = |r: Rule| {
        if !out.iter().any(|x| std::mem::discriminant(x) == std::mem::discriminant(&r)) {
            out.push(r);
        }
    };
    for b in blocks {
        let k = staff::check(blocks, b, None, None);
        if k.double.is_some() {
            add(Rule::Overlap);
        }
        if let Some(r) = k.rest.filter(|r| *r < REST) {
            add(Rule::ShortRest(r));
        }
        if let Some(t) = k.transfer {
            add(Rule::StopChange(t));
        }
    }
    let work = staff::work_minutes(blocks);
    if work > DAY_MAX {
        add(Rule::TooLong(work));
    } else if work > DAY_TARGET {
        add(Rule::Overtime(work - DAY_TARGET));
    }
    let mut sorted: Vec<&Block> = blocks.iter().collect();
    sorted.sort_by_key(|b| b.from);
    if let Some(gap) = sorted.windows(2).map(|w| w[1].from - w[0].to).max().filter(|g| *g > LONG_SPLIT) {
        add(Rule::LongSplit(gap));
    }
    out.sort_by_key(|r| !r.hard());
    out
}

/// A piece of a duty: block `tour`'s duty `duty`, and its time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub tour: usize,
    pub duty: usize,
    pub from: i32,
    pub to: i32,
}

/// A driver's duty of the day: its pieces in order, and the rules it breaks or comes near.
#[derive(Clone, Debug, PartialEq)]
pub struct Duty {
    pub pieces: Vec<Piece>,
    pub rules: Vec<Rule>,
}

impl Duty {
    pub fn from(&self) -> i32 {
        self.pieces.first().map(|p| p.from).unwrap_or(0)
    }

    pub fn to(&self) -> i32 {
        self.pieces.iter().map(|p| p.to).max().unwrap_or(0)
    }

    /// Minutes worked (the breaks between pieces not).
    pub fn work(&self) -> i32 {
        self.pieces.iter().map(|p| p.to - p.from).sum()
    }

    /// Minutes paid: what it works, a day worked at least `MIN_PAID`.
    pub fn paid(&self) -> i32 {
        self.work().max(MIN_PAID)
    }

    /// A split duty: two pieces with a break of `SPLIT_BREAK` or more between.
    pub fn split(&self) -> bool {
        self.pieces.windows(2).any(|w| w[1].from - w[0].to >= SPLIT_BREAK)
    }

    pub fn forbidden(&self) -> bool {
        self.rules.iter().any(|r| r.hard())
    }
}

fn piece_block(tours: &[TourOfDay], i: usize, k: usize) -> Block {
    let t = &tours[i];
    let r = network::duties_of(t).get(k).cloned().unwrap_or(0..0);
    let (from, to) = span(t, &r);
    Block { key: format!("{i}/{k}"), from, to, from_stop: t.trips.get(r.start).map(|x| x.from.clone()).unwrap_or_default(), to_stop: r.end.checked_sub(1).and_then(|e| t.trips.get(e)).map(|x| x.end().to_string()).unwrap_or_default() }
}

fn span(t: &TourOfDay, r: &Range<usize>) -> (i32, i32) {
    let trips = t.trips.get(r.clone()).unwrap_or(&[]);
    (trips.first().map(|x| x.dep).unwrap_or(0), trips.iter().map(|x| x.arr).max().unwrap_or(0))
}

/// The day's driver duties: every block's pieces (`network::duties_of`), those the player
/// joined on `weekday` (`joined`) one duty each, in the order they begin.
pub fn duties(joined: &[JoinedDuty], weekday: u8, tours: &[TourOfDay]) -> Vec<Duty> {
    let mut pieces: Vec<(Piece, PieceRef, bool)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        for (k, r) in network::duties_of(t).iter().enumerate() {
            let (from, to) = span(t, r);
            pieces.push((Piece { tour: i, duty: k, from, to }, PieceRef::of(t, r), false));
        }
    }
    let mut out: Vec<Duty> = Vec::new();
    let mut make = |ps: Vec<Piece>| {
        let blocks: Vec<Block> = ps.iter().map(|p| piece_block(tours, p.tour, p.duty)).collect();
        let mut ps = ps;
        ps.sort_by_key(|p| p.from);
        out.push(Duty { rules: rules_of(&blocks), pieces: ps });
    };
    for j in joined.iter().filter(|j| j.weekday == weekday) {
        let mut ps = Vec::new();
        for p in &j.pieces {
            if let Some(x) = pieces.iter_mut().find(|x| !x.2 && x.1.same(p)) {
                x.2 = true;
                ps.push(x.0);
            }
        }
        if !ps.is_empty() {
            make(ps);
        }
    }
    for x in pieces.iter().filter(|x| !x.2) {
        make(vec![x.0]);
    }
    out.sort_by_key(|d| (d.from(), d.pieces[0].tour));
    out
}

/// The duty piece (block, duty) is in (an index into `duties`).
pub fn duty_of(duties: &[Duty], tour: usize, duty: usize) -> Option<usize> {
    duties.iter().position(|d| d.pieces.iter().any(|p| p.tour == tour && p.duty == duty))
}

// --- the figures -----------------------------------------------------------------------------

/// The day's figures the studio shows on top, updated with every edit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kpis {
    /// Buses out at once at the busiest (the peak vehicle requirement).
    pub buses: usize,
    pub duties: usize,
    /// Minutes paid to the drivers (`Duty::paid`).
    pub paid: i32,
    /// Kilometres run without passengers: the depot runs and the runs between places.
    pub deadhead_km: f64,
    /// What the day costs about: the drivers' hours, the kilometres, the buses' standing.
    pub cost: Cents,
}

/// The empty runs between a block's trips: (from, to, km) where one trip ends elsewhere than
/// the next begins (the line's trips `line` give the kilometres).
pub fn deadheads(t: &TourOfDay, line: &[&PlannedTrip]) -> Vec<(i32, i32, f64)> {
    t.trips.windows(2).filter(|w| !same(w[0].end(), &w[1].from)).map(|w| (w[0].arr, w[1].dep, deadhead_km(line, w[0].end(), &w[1].from))).collect()
}

/// The most blocks out at once.
pub fn peak(tours: &[TourOfDay]) -> usize {
    let mut ev: Vec<(i32, i32)> = tours.iter().filter(|t| !t.trips.is_empty()).flat_map(|t| [(t.from(), 1), (t.to(), -1)]).collect();
    // (one ending as another begins: the bus is free for it)
    ev.sort();
    let (mut n, mut most) = (0i32, 0i32);
    for (_, d) in ev {
        n += d;
        most = most.max(n);
    }
    most as usize
}

/// The day's figures (`duties` as `duties` gives them).
pub fn kpis(c: &Company, tours: &[TourOfDay], duties: &[Duty]) -> Kpis {
    let mut empty_km = 0.0;
    let mut km = 0.0;
    for t in tours {
        let line: Vec<&PlannedTrip> = tours.iter().filter(|x| same(&x.line, &t.line)).flat_map(|x| x.trips.iter()).collect();
        let runs: f64 = deadheads(t, &line).iter().map(|d| d.2).sum();
        empty_km += runs + t.trips.iter().filter(|x| x.empty).map(|x| x.km).sum::<f64>();
        km += runs + t.km();
    }
    let buses = peak(tours);
    let paid: i32 = duties.iter().map(Duty::paid).sum();
    // (the drivers' hour as the company pays it, the employer's share in; the night half again)
    let drivers: Vec<Cents> = c.staff.iter().filter(|e| staff::drives(e)).map(|e| e.wage).collect();
    let wage = if drivers.is_empty() { economy::market_wage(45.0, c.price_index) } else { drivers.iter().sum::<Cents>() / drivers.len() as Cents };
    let hour = staff::hourly(economy::employer_cost(wage));
    let night: i32 = duties.iter().flat_map(|d| d.pieces.iter()).map(|p| staff::night_minutes(p.from, p.to)).sum();
    let people = (paid as f64 + night as f64 * staff::NIGHT_SUPPLEMENT) / 60.0 * hour;
    let bus = BusKind { size: BusSize::Solo, drive: Drive::Diesel };
    let per_km = economy::energy_per_km(bus, c.price_index) + economy::maintenance_per_km(bus, 5.0, c.price_index);
    let standing = economy::insurance_per_month(bus, c.price_index) as f64 / 30.0 * buses as f64;
    Kpis { buses, duties: duties.len(), paid, deadhead_km: empty_km, cost: (people + km * per_km + standing).round() as Cents }
}

#[cfg(test)]
mod tests {
    use super::super::network::tours_of_day;
    use super::super::{found, Founding};
    use super::*;
    use crate::company::model::CompanyLine;
    use crate::{StopInfo, TourInfo, TripInfo};

    /// A trip of the timetable: from `a` to `b`, `dep` to `arr` (minutes), with passengers.
    fn tt(name: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
        TripInfo {
            name: name.into(),
            index: 0,
            line: "5".into(),
            from: a.into(),
            // (its displays show another name than its last stop has, as they often do)
            terminus: format!("{b} Bf"),
            departure: dep as f64 * 60.0,
            arrival: arr as f64 * 60.0,
            // (its first and last stop the termini it runs between)
            stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
            km: (arr - dep) as f64 * 0.4,
        }
    }

    /// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
    /// minutes back and forth from A, ten minutes at each end.
    fn line_of(tours: &[(&str, i32, i32)]) -> LineInfo {
        let tour = |&(no, from, n): &(&str, i32, i32)| TourInfo {
            number: no.into(),
            ai_group: String::new(),
            first: 0.0,
            last: 0.0,
            days: "daily".into(),
            runs: true,
            next_run: None,
            trips: (0..n)
                .map(|k| {
                    let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
                    tt(&format!("5 {no}-{k}"), a, b, from + k * 60, from + k * 60 + 50)
                })
                .collect(),
        };
        LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }
    }

    /// Tour 1 from 06:00 and tour 2 from 06:30, eight trips each (until 13:50 and 14:20).
    fn line5() -> LineInfo {
        line_of(&[("1", 360, 8), ("2", 390, 8)])
    }

    fn company() -> Company {
        let mut c = found(&Founding { name: "Studio".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.game_mode = super::super::GameMode::Normal;
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c
    }

    fn day(c: &Company, l: &LineInfo) -> Vec<TourOfDay> {
        tours_of_day(c, std::slice::from_ref(l), &c.date)
    }

    fn block<'a>(tours: &'a [TourOfDay], no: &str) -> (usize, &'a TourOfDay) {
        tours.iter().enumerate().find(|(_, t)| t.tour == no).unwrap()
    }

    #[test]
    fn a_trip_moves_to_another_bus_or_a_new_one_and_impossible_moves_are_refused() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        assert_eq!(tours.len(), 2);
        // tour 2's 06:30 trip on tour 1's bus: it is busy then
        let (one, _) = block(&tours, "1");
        let (two, t2) = block(&tours, "2");
        assert_eq!(move_trip(&mut c, wd, &tours, two, 0, Target::Block(one)), Err(OVERLAP));
        // its last trip on a new bus: tour 3, kept for the weekday, the trip still knows its tour
        let last = t2.trips.len() - 1;
        move_trip(&mut c, wd, &tours, two, last, Target::New).unwrap();
        let after = day(&c, &l);
        assert_eq!(after.len(), 3);
        let (_, t3) = block(&after, "3");
        assert_eq!((t3.trips.len(), t3.trips[0].origin.as_str()), (1, "2"));
        assert_eq!(block(&after, "2").1.trips.len(), 7);
        // the easy mode keeps the timetable's tours
        c.game_mode = super::super::GameMode::Casual;
        assert_eq!(day(&c, &l).len(), 2);
        c.game_mode = super::super::GameMode::Normal;
        // a trip that fits: tour 3's trip back to tour 2 - and tour 3 runs no more
        let (three, _) = block(&after, "3");
        let (two, _) = block(&after, "2");
        move_trip(&mut c, wd, &after, three, 0, Target::Block(two)).unwrap();
        let back = day(&c, &l);
        assert_eq!(back.len(), 2);
        assert_eq!(back.iter().map(|t| t.trips.len()).sum::<usize>(), 16);
        // the only trip of a bus to a new one, and another line's bus: refused
        let mut other = line5();
        other.name = "Linie7".into();
        c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), service_from: Some(0), ..Default::default() });
        let both = tours_of_day(&c, &[l.clone(), other.clone()], &c.date);
        let a = both.iter().position(|t| t.line == "Linie5").unwrap();
        let b = both.iter().position(|t| t.line == "Linie7").unwrap();
        assert_eq!(plan_move(&c, wd, &both, a, 0, Target::Block(b)), Err(OTHER_LINE));
        // another weekday keeps the timetable's tours
        let tue = tours_of_day(&c, std::slice::from_ref(&l), "2024-03-05");
        assert!(tue.iter().all(|t| t.trips.iter().all(|x| x.origin == t.tour)));
    }

    #[test]
    fn a_trip_fits_a_bus_only_with_time_to_get_there_and_out_of_the_depot() {
        let at = |name: &str, a: &str, b: &str, dep: i32, arr: i32, empty: bool| PlannedTrip { name: name.into(), from: a.into(), to: b.into(), dep, arr, stops: if empty { 0 } else { 6 }, empty, origin: "1".into(), ..Default::default() };
        let back = at("t2", "B", "A", 420, 470, false);
        let line = [&back];
        let bus = vec![at("out", "Depot", "A", 350, 358, true), at("t1", "A", "B", 360, 410, false), at("in", "B", "Depot", 412, 420, true)];
        // after the bus went back to the depot, before it left it, over its trip
        assert_eq!(fits_into(&bus, &at("x", "A", "B", 425, 470, false), &line), Err(BACK_IN_DEPOT));
        assert_eq!(fits_into(&bus, &at("x", "A", "B", 300, 340, false), &line), Err(IN_DEPOT));
        assert_eq!(fits_into(&bus, &at("x", "A", "B", 380, 430, false), &line), Err(OVERLAP));
        // without its run in: from B at 06:50 it takes 50 minutes back to A
        let open = &bus[..2];
        assert_eq!(fits_into(open, &at("x", "A", "B", 415, 460, false), &line), Err(NO_TIME));
        assert_eq!(fits_into(open, &at("x", "A", "B", 460, 510, false), &line), Ok(()));
        assert_eq!(fits_into(open, &at("x", "B", "A", 415, 465, false), &line), Ok(()));
    }

    #[test]
    fn duties_are_cut_and_joined_only_at_the_termini_or_the_depot() {
        let mut c = company();
        let mut l = line5();
        // tour 1 runs empty to the workshop and back in the middle of its day
        l.tours[0].trips.insert(4, TripInfo { line: String::new(), stops: Vec::new(), km: 2.0, terminus: "Werkstatt".into(), ..tt("X1", "A", "Werkstatt", 591, 593) });
        l.tours[0].trips.insert(5, TripInfo { line: String::new(), stops: Vec::new(), km: 2.0, terminus: "A".into(), ..tt("X2", "Werkstatt", "A", 594, 596) });
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        let (one, t1) = block(&tours, "1");
        assert_eq!(network::duties_of(t1).len(), 1);
        // where a trip ends and the next begins at a terminus: cut
        cut(&mut c, wd, &tours, one, 2).unwrap();
        let tours = day(&c, &l);
        let (one, t1) = block(&tours, "1");
        assert_eq!(network::duties_of(t1), vec![0..2, 2..10]);
        assert_eq!(cut(&mut c, wd, &tours, one, 2), Err(CUT_ALREADY));
        // at the workshop: no relief point; where the bus moves on: none either
        assert_eq!(t1.trips[5].from, "Werkstatt");
        assert_eq!(can_cut(&c, &tours, one, 5), Err(NOT_RELIEF));
        let mut moving = tours.clone();
        moving[one].trips[6].from = "C".into();
        assert_eq!(can_cut(&c, &moving, one, 6), Err(ON_THE_MOVE));
        assert_eq!(can_cut(&c, &tours, one, 0), Err(BETWEEN));
        // joined again: one duty
        uncut(&mut c, wd, &tours, one, 2).unwrap();
        let tours = day(&c, &l);
        assert_eq!(network::duties_of(block(&tours, "1").1).len(), 1);
        assert_eq!(uncut(&mut c, wd, &tours, block(&tours, "1").0, 2), Err(NO_CUT));
        // the depot is a relief point, a stop on the way is not
        assert!(is_relief(&c, &tours, "Linie5", "Depot"));
        assert!(!is_relief(&c, &tours, "Linie5", "Werkstatt"));
    }

    #[test]
    fn joined_duties_keep_to_the_rules() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        let (one, _) = block(&tours, "1");
        let (two, _) = block(&tours, "2");
        // two buses at the same time for one driver: refused
        assert_eq!(join(&mut c, wd, &tours, (one, 0), (two, 0)), Err(Rule::Overlap.label()));
        // a morning piece and an afternoon piece: cut both, then join tour 1's first to tour
        // 2's last
        cut(&mut c, wd, &tours, one, 2).unwrap();
        let tours = day(&c, &l);
        cut(&mut c, wd, &tours, block(&tours, "2").0, 6).unwrap();
        let tours = day(&c, &l);
        let (one, _) = block(&tours, "1");
        let (two, _) = block(&tours, "2");
        join(&mut c, wd, &tours, (one, 0), (two, 1)).unwrap();
        let d = duties(&c.planning.joined, wd, &tours);
        assert_eq!(d.len(), 3);
        let split = d.iter().find(|d| d.pieces.len() == 2).unwrap();
        // 06:00-07:50 and 12:30-14:20: a split duty, its break of 4 h 40 warned of
        assert!(split.split());
        assert_eq!(split.rules, vec![Rule::LongSplit(280)]);
        assert!(!split.forbidden());
        assert_eq!(join(&mut c, wd, &tours, (one, 0), (two, 1)), Err(SAME_DUTY));
        // taken apart again
        unjoin(&mut c, wd, &tours, (two, 1)).unwrap();
        assert_eq!(duties(&c.planning.joined, wd, &tours).len(), 4);
        assert_eq!(unjoin(&mut c, wd, &tours, (two, 1)), Err(NOT_JOINED));
        // the rules themselves
        let b = |from: i32, to: i32, a: &str, z: &str| Block { key: format!("{from}"), from, to, from_stop: a.into(), to_stop: z.into() };
        assert_eq!(rules_of(&[b(360, 600, "A", "B"), b(610, 900, "B", "A")]), vec![Rule::ShortRest(10), Rule::Overtime(50)]);
        assert_eq!(rules_of(&[b(300, 960, "A", "A")]), vec![Rule::TooLong(660)]);
        assert_eq!(rules_of(&[b(360, 600, "A", "B"), b(630, 700, "C", "A")]), vec![Rule::StopChange(30)]);
        assert!(rules_of(&[b(360, 600, "A", "B"), b(500, 700, "B", "A")]).contains(&Rule::Overlap));
        // tour 1 (7 h 50) and tour 3 after it (3 h 50) for one driver: longer than a working
        // day may be - refused
        let mut c2 = company();
        let l = line_of(&[("1", 360, 8), ("3", 870, 4)]);
        let tours = day(&c2, &l);
        let (one, _) = block(&tours, "1");
        let (three, _) = block(&tours, "3");
        assert_eq!(join(&mut c2, wd, &tours, (one, 0), (three, 0)), Err(Rule::TooLong(0).label()));
        assert!(c2.planning.joined.is_empty());
    }

    #[test]
    fn the_figures_follow_the_edits() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        let k = kpis(&c, &tours, &duties(&c.planning.joined, wd, &tours));
        // two buses at once, a duty each (7 h 50 and 7 h 50), no empty kilometres
        assert_eq!((k.buses, k.duties, k.paid), (2, 2, 940));
        assert_eq!(k.deadhead_km, 0.0);
        assert!(k.cost > 0);
        // a cut: three duties; the third bus at the busiest; paid at least four hours each
        cut(&mut c, wd, &tours, block(&tours, "1").0, 2).unwrap();
        let tours = day(&c, &l);
        let d = duties(&c.planning.joined, wd, &tours);
        let k2 = kpis(&c, &tours, &d);
        assert_eq!((k2.buses, k2.duties), (2, 3));
        assert_eq!(k2.paid, 240 + 350 + 470);
        assert!(k2.cost > k.cost);
        // tour 2's first trip on a new bus: a duty more, still two buses at once (the new one
        // is back before tour 2 leaves)
        let (two, _) = block(&tours, "2");
        move_trip(&mut c, wd, &tours, two, 0, Target::New).unwrap();
        let tours = day(&c, &l);
        let k3 = kpis(&c, &tours, &duties(&c.planning.joined, wd, &tours));
        assert_eq!((k3.buses, k3.duties), (2, 4));
        // a trip out of the middle of tour 2 on a new bus: tour 2 stays out meanwhile - three
        // buses at once
        let mut c5 = company();
        let tours = day(&c5, &l);
        move_trip(&mut c5, wd, &tours, block(&tours, "2").0, 1, Target::New).unwrap();
        let tours = day(&c5, &l);
        assert_eq!(kpis(&c5, &tours, &duties(&c5.planning.joined, wd, &tours)).buses, 3);
        // a trip that leaves a gap between places counts empty kilometres: tour 1's second trip
        // (B to A) taken out leaves tour 1 ending at B and going on from B
        let mut c4 = company();
        let tours = day(&c4, &l);
        let (one, t1) = block(&tours, "1");
        assert_eq!(t1.trips[1].from, "B");
        move_trip(&mut c4, wd, &tours, one, 1, Target::New).unwrap();
        let tours = day(&c4, &l);
        let k4 = kpis(&c4, &tours, &duties(&c4.planning.joined, wd, &tours));
        assert!((k4.deadhead_km - 20.0).abs() < 1e-6, "{}", k4.deadhead_km);
    }

    #[test]
    fn an_edit_the_timetable_no_longer_fits_goes_back_to_automatic_and_says_so() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        move_trip(&mut c, wd, &tours, block(&tours, "2").0, 7, Target::New).unwrap();
        assert_eq!(day(&c, &l).len(), 3);
        assert!(stale(&c, std::slice::from_ref(&l), &c.date).is_empty());
        // the timetable changed: tour 2's last trip leaves later
        let mut changed = l.clone();
        changed.tours[1].trips[7].departure += 300.0;
        changed.tours[1].trips[7].arrival += 300.0;
        assert_eq!(day(&c, &changed).len(), 2, "the automatic blocks");
        assert_eq!(stale(&c, std::slice::from_ref(&changed), &c.date), vec!["Linie5".to_string()]);
        let unread = c.inbox.unread();
        let date = c.date.clone();
        assert_eq!(retire_stale(&mut c, std::slice::from_ref(&changed), &date), vec!["5".to_string()]);
        assert!(c.planning.layouts.is_empty());
        assert_eq!(c.inbox.unread(), unread + 1);
        assert!(c.inbox.messages.iter().any(|m| m.title == STALE.0));
        // an old company without the studio's fields loads as before
        let mut json = serde_json::to_value(&c.planning).unwrap();
        json.as_object_mut().unwrap().retain(|k, _| k != "layouts" && k != "joined");
        let p: super::super::plan::Planning = serde_json::from_value(json).unwrap();
        assert!(p.layouts.is_empty() && p.joined.is_empty());
    }

    #[test]
    fn the_game_runs_each_timetable_tour_with_the_bus_holding_most_of_it() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(&c, &l);
        move_trip(&mut c, wd, &tours, block(&tours, "2").0, 7, Target::New).unwrap();
        let tours = day(&c, &l);
        let dp: Vec<DayTour> = tours.iter().cloned().map(|t| DayTour { tour: t, ..Default::default() }).collect();
        let of = |no: &str| origins(&dp, dp.iter().find(|t| t.tour.tour == no).unwrap());
        assert_eq!((of("1"), of("2"), of("3")), (vec!["1".to_string()], vec!["2".to_string()], Vec::<String>::new()));
        // the day plan and the game's plan of the day take the edited blocks: three buses, the
        // game's tour 2 with block 2's bus (block 3 holds no tour mostly)
        let date = c.date.clone();
        let plan = super::super::plan::day_plan(&c, &date, tours.clone(), &[], &[], false);
        assert_eq!(plan.tours.len(), 3);
        let live = super::super::plan::live_plan(&c, &plan);
        let mut names: Vec<&str> = live.tours.iter().map(|t| t.tour.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["1", "2"]);
        // a trip the game reports of timetable tour 2 is the block's that runs it
        assert!(tours.iter().find(|t| t.tour == "3").unwrap().runs_tour("2"));
        assert!(!tours.iter().find(|t| t.tour == "1").unwrap().runs_tour("2"));
    }

    // --- trips moved between duties --------------------------------------------------------

    /// The duty that works trip `j` of block `no` on the day as `c` lays it out.
    fn duty_at(c: &Company, l: &LineInfo, no: &str, j: usize) -> (Vec<TourOfDay>, Vec<Duty>, usize, usize) {
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(c, l);
        let ds = duties(&c.planning.joined, wd, &tours);
        let (i, _) = block(&tours, no);
        let d = duty_of_trip(&ds, &tours, i, j).unwrap();
        (tours, ds, i, d)
    }

    #[test]
    fn a_trip_moves_to_another_duty_at_a_terminus_and_back() {
        let mut c = company();
        // tour 1 from 06:00 (four trips, until 09:50 at A), tour 3 from 12:00 (from A)
        let l = line_of(&[("1", 360, 4), ("3", 720, 4)]);
        let wd = super::super::plan::weekday_of(&c.date);
        let (tours, ds, one, a) = duty_at(&c, &l, "1", 3);
        assert_eq!(ds.len(), 2);
        let b = duty_of_trip(&ds, &tours, block(&tours, "3").0, 0).unwrap();
        // tour 1's last trip (09:00 from B) to the driver of tour 3
        move_to_duty(&mut c, wd, &tours, a, &[(one, 3)], DutyTarget::Duty(b)).unwrap();
        let (tours, ds, one, b2) = duty_at(&c, &l, "1", 3);
        // the bus keeps its four trips; its drivers change at B, before 09:00
        let t1 = &tours[one];
        assert_eq!(t1.trips.len(), 4);
        assert_eq!(network::duties_of(t1), vec![0..3, 3..4]);
        assert_eq!(ds.len(), 2);
        assert_eq!(duty_of_trip(&ds, &tours, block(&tours, "3").0, 0), Some(b2));
        assert_ne!(duty_of_trip(&ds, &tours, one, 2), Some(b2));
        // that driver's duty: 09:00-09:50 on bus 1 and 12:00-15:50 on bus 3, a split duty
        let d = &ds[b2];
        assert_eq!(d.pieces.len(), 2);
        assert!(d.split() && !d.forbidden());
        assert_eq!((d.from(), d.to()), (540, 950));
        // back to the first driver: one duty on bus 1 again, the cut gone
        let a2 = duty_of_trip(&ds, &tours, one, 0).unwrap();
        move_to_duty(&mut c, wd, &tours, b2, &[(one, 3)], DutyTarget::Duty(a2)).unwrap();
        let (tours, ds, one, _) = duty_at(&c, &l, "1", 3);
        assert_eq!(network::duties_of(&tours[one]), vec![0..4]);
        assert_eq!(ds.len(), 2);
        assert!(ds.iter().all(|d| d.pieces.len() == 1));
        assert!(c.planning.joined.is_empty());
    }

    #[test]
    fn trips_move_to_a_new_duty_and_their_duty_is_cut_around_them() {
        let mut c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let (tours, ds, one, a) = duty_at(&c, &l, "1", 3);
        assert_eq!(ds.len(), 2);
        // tour 1's 09:00 trip out of the middle of its duty: a duty of its own, and the first
        // driver's a split duty around it (06:00-08:50, 10:00-13:50)
        let plan = plan_move_to_duty(&c, wd, &tours, a, &[(one, 3)], DutyTarget::New).unwrap();
        assert_eq!(plan.layouts.len(), 1);
        assert_eq!(plan.joined.len(), 1);
        move_to_duty(&mut c, wd, &tours, a, &[(one, 3)], DutyTarget::New).unwrap();
        let (tours, ds, one, n) = duty_at(&c, &l, "1", 3);
        assert_eq!(network::duties_of(&tours[one]), vec![0..3, 3..4, 4..8]);
        assert_eq!(ds.len(), 3);
        assert_eq!(ds[n].pieces.len(), 1);
        let first = &ds[duty_of_trip(&ds, &tours, one, 0).unwrap()];
        assert_eq!(first.pieces.len(), 2);
        assert_eq!(duty_of_trip(&ds, &tours, one, 0), duty_of_trip(&ds, &tours, one, 7));
        assert!(first.split());
        // two trips together (Shift or Ctrl): from the second driver's day to a new duty
        let (tours, _, two, b) = duty_at(&c, &l, "2", 6);
        move_to_duty(&mut c, wd, &tours, b, &[(two, 6), (two, 7)], DutyTarget::New).unwrap();
        let (tours, ds, two, m) = duty_at(&c, &l, "2", 6);
        assert_eq!(network::duties_of(&tours[two]), vec![0..6, 6..8]);
        assert_eq!(duty_of_trip(&ds, &tours, two, 7), Some(m));
        assert_eq!(ds.len(), 4);
        // what makes no sense is refused: every trip of a duty to a new one, a trip to its own
        // duty, trips of two duties at once
        let (tours, ds, one, n) = duty_at(&c, &l, "1", 3);
        assert_eq!(plan_move_to_duty(&c, wd, &tours, n, &[(one, 3)], DutyTarget::New), Err(DUTY_ALONE));
        assert_eq!(plan_move_to_duty(&c, wd, &tours, n, &[(one, 3)], DutyTarget::Duty(n)), Err(SAME_DUTY));
        assert_eq!(plan_move_to_duty(&c, wd, &tours, n, &[(one, 3), (one, 2)], DutyTarget::New), Err(ONE_DUTY));
        let other = duty_of_trip(&ds, &tours, one, 0).unwrap();
        assert_eq!(plan_move_to_duty(&c, wd, &tours, other, &[(one, 3)], DutyTarget::New), Err(ONE_DUTY));
    }

    #[test]
    fn trips_of_several_duties_move_down_a_row_together() {
        // (testers: "Multi-selected duties ... can be selected all together, but not dragged up
        // or down") three buses, a driver each: tour 1 06:00-09:50, 2 12:00-15:50, 3 18:00-21:50
        let mut c = company();
        let l = line_of(&[("1", 360, 4), ("2", 720, 4), ("3", 1080, 4)]);
        let wd = super::super::plan::weekday_of(&c.date);
        let again = |c: &Company| day(c, &l);
        let tours = day(&c, &l);
        let ds = duties(&c.planning.joined, wd, &tours);
        let ((one, _), (two, _), (three, _)) = (block(&tours, "1"), block(&tours, "2"), block(&tours, "3"));
        let duty = |i: usize| duty_of_trip(&ds, &tours, i, 0).unwrap();
        // the last trip of duties 1 and 2, each a row down: to the driver of 2 and of 3
        let groups = vec![DutyGroup { trips: vec![(one, 3)], to: DutyTarget::Duty(duty(two)) }, DutyGroup { trips: vec![(two, 3)], to: DutyTarget::Duty(duty(three)) }];
        // (refused as a whole when one of them is: its index and why, nothing changed)
        let bad = vec![groups[0].clone(), DutyGroup { trips: vec![(two, 3), (one, 2)], to: DutyTarget::Duty(duty(three)) }];
        let was = c.planning.clone();
        assert_eq!(move_to_duties(&mut c, wd, &tours, &again, &bad), Err((1, ONE_DUTY)));
        assert_eq!(c.planning, was);
        move_to_duties(&mut c, wd, &tours, &again, &groups).unwrap();
        let tours = day(&c, &l);
        let ds = duties(&c.planning.joined, wd, &tours);
        let ((one, _), (two, _), (three, _)) = (block(&tours, "1"), block(&tours, "2"), block(&tours, "3"));
        let of = |i: usize, j: usize| duty_of_trip(&ds, &tours, i, j).unwrap();
        assert_eq!(ds.len(), 3);
        assert_eq!(of(one, 3), of(two, 0));
        assert_eq!(of(two, 3), of(three, 0));
        assert_ne!(of(one, 0), of(one, 3));
        assert_ne!(of(two, 0), of(two, 3));
        // the same up again, the groups the other way round: back as it was
        let groups = vec![DutyGroup { trips: vec![(two, 3)], to: DutyTarget::Duty(of(two, 0)) }, DutyGroup { trips: vec![(one, 3)], to: DutyTarget::Duty(of(one, 0)) }];
        move_to_duties(&mut c, wd, &tours, &again, &groups).unwrap();
        let tours = day(&c, &l);
        assert!(tours.iter().all(|t| network::duties_of(t).len() == 1));
        assert!(c.planning.joined.is_empty());
    }

    #[test]
    fn a_trip_moves_only_where_the_drivers_can_change_at_a_terminus_or_the_depot() {
        let c = company();
        let l = line5();
        let wd = super::super::plan::weekday_of(&c.date);
        let (tours, _, one, a) = duty_at(&c, &l, "1", 3);
        // the 09:00 trip ends at A; the bus then runs empty from elsewhere to the workshop and
        // on: the only place it stands between there and its next trip is the workshop
        let run = |name: &str, from: &str, to: &str, dep: i32| PlannedTrip { name: name.into(), from: from.into(), to: to.into(), dep, arr: dep + 2, stops: 0, empty: true, origin: "1".into(), ..Default::default() };
        let mut workshop = tours.clone();
        workshop[one].trips.insert(4, run("X1", "A-Nord", "Werkstatt", 591));
        workshop[one].trips.insert(5, run("X2", "Werkstatt", "A-Süd", 594));
        assert_eq!(plan_move_to_duty(&c, wd, &workshop, a, &[(one, 3)], DutyTarget::New), Err(NOT_RELIEF));
        // where the bus runs empty from where a trip ended to another place, without a run
        // listed (a deadhead whose time is estimated), it stood at the terminus first: the
        // drivers change there (Luc's line 880 runs one way only, all its returns so)
        let mut moving = tours.clone();
        moving[one].trips[4].from = "C".into();
        assert!(plan_move_to_duty(&c, wd, &moving, a, &[(one, 3)], DutyTarget::New).is_ok());
        // but not where empty runs end somewhere and what follows begins elsewhere
        let mut moving = workshop.clone();
        moving[one].trips[5].from = "Werkstatt-Hof".into();
        moving[one].trips[6].from = "C".into();
        assert_eq!(plan_move_to_duty(&c, wd, &moving, a, &[(one, 3)], DutyTarget::New), Err(ON_THE_MOVE));
        // at the terminus it can
        assert!(plan_move_to_duty(&c, wd, &tours, a, &[(one, 3)], DutyTarget::New).is_ok());
        // only trips with passengers move
        assert_eq!(plan_move_to_duty(&c, wd, &workshop, a, &[(one, 4)], DutyTarget::New), Err(NOT_MOVABLE));
    }

    #[test]
    fn a_trip_moved_to_another_duty_keeps_to_the_working_time_rules() {
        let wd = super::super::plan::weekday_of(&company().date);
        // two buses at the same time: tour 2's 06:30 trip to tour 1's driver overlaps his work
        let c = company();
        let l = line5();
        let (tours, ds, two, b) = duty_at(&c, &l, "2", 0);
        let a = duty_of_trip(&ds, &tours, block(&tours, "1").0, 0).unwrap();
        assert_eq!(plan_move_to_duty(&c, wd, &tours, b, &[(two, 0)], DutyTarget::Duty(a)), Err(Rule::Overlap.label()));
        // tour 1 (7 h 50) and tour 3 after it: one trip more is a day of 8 h 40, three more
        // longer than a working day may be (10 h)
        let l = line_of(&[("1", 360, 8), ("3", 870, 4)]);
        let (tours, ds, three, b) = duty_at(&c, &l, "3", 0);
        let a = duty_of_trip(&ds, &tours, block(&tours, "1").0, 0).unwrap();
        assert!(plan_move_to_duty(&c, wd, &tours, b, &[(three, 0)], DutyTarget::Duty(a)).is_ok());
        assert_eq!(plan_move_to_duty(&c, wd, &tours, b, &[(three, 0), (three, 1), (three, 2)], DutyTarget::Duty(a)), Err(Rule::TooLong(0).label()));
        // the driver must get to the trip: his work before ends at B at 08:50, the trip leaves
        // A at 09:15 - five minutes' walk will do, half an hour by public transport will not
        let mut c = company();
        let l = line_of(&[("1", 360, 3), ("3", 555, 2)]);
        let (tours, ds, three, b) = duty_at(&c, &l, "3", 0);
        let a = duty_of_trip(&ds, &tours, block(&tours, "1").0, 0).unwrap();
        assert!(plan_move_to_duty(&c, wd, &tours, b, &[(three, 0)], DutyTarget::Duty(a)).is_ok());
        c.planning.flow.prefs.travel = super::super::rosterflow::Travel::Public;
        c.planning.flow.prefs.public = 30;
        assert_eq!(plan_move_to_duty(&c, wd, &tours, b, &[(three, 0)], DutyTarget::Duty(a)), Err(DRIVER_NO_TIME));
        // a locked duty stays as it is
        let mut c = company();
        let (tours, ds, three, b) = duty_at(&c, &l, "3", 0);
        let a = duty_of_trip(&ds, &tours, block(&tours, "1").0, 0).unwrap();
        let key = super::super::rosterflow::duty_key(&tours, &ds[a]).unwrap();
        super::super::rosterflow::set_duty_lock(&mut c, wd, &key, true);
        assert_eq!(plan_move_to_duty(&c, wd, &tours, b, &[(three, 0)], DutyTarget::Duty(a)), Err(DUTY_LOCKED));
    }

    #[test]
    fn undo_puts_the_duties_back_and_the_move_is_saved_and_loaded() {
        use super::super::rosterflow as rf;
        let mut c = company();
        let l = line_of(&[("1", 360, 4), ("3", 720, 4)]);
        let wd = super::super::plan::weekday_of(&c.date);
        let (tours0, ds0, one, a) = duty_at(&c, &l, "1", 3);
        let b = duty_of_trip(&ds0, &tours0, block(&tours0, "3").0, 0).unwrap();
        // the driver of tour 3 has a duty type set by hand: it follows his duty
        let key = rf::duty_key(&tours0, &ds0[b]).unwrap();
        rf::note_mut(&mut c, wd, &key).kind = Some(rf::TYPE_NAMES[3].to_string());
        // as RosterFlow edits: on the view, kept as the draft
        let snapshot = rf::draft_of(&c);
        let mut v = rf::view(&c);
        move_to_duty(&mut v, wd, &tours0, a, &[(one, 3)], DutyTarget::Duty(b)).unwrap();
        rf::keep_draft(&mut c, &v);
        assert!(rf::has_draft(&c));
        let (tours, ds, _, b2) = duty_at(&rf::view(&c), &l, "1", 3);
        assert_ne!(ds, ds0);
        let new_key = rf::duty_key(&tours, &ds[b2]).unwrap();
        assert!(!new_key.is(&key), "the duty begins with the trip moved now");
        let note = rf::view(&c).planning.flow.sheet.notes.iter().find(|n| n.key.is(&new_key)).cloned().unwrap();
        assert_eq!(note.kind.as_deref(), Some(rf::TYPE_NAMES[3]));
        // undone: the draft as before, the day's duties as before
        rf::set_draft(&mut c, snapshot.clone());
        assert!(!rf::has_draft(&c));
        let (tours, ds, _, _) = duty_at(&rf::view(&c), &l, "1", 3);
        assert_eq!((tours, ds), (tours0.clone(), ds0.clone()));
        // redone and saved: the company written and read again has the same duties
        rf::keep_draft(&mut c, &v);
        assert!(rf::save_draft(&mut c));
        let json = serde_json::to_string(&c).unwrap();
        let back: Company = serde_json::from_str(&json).unwrap();
        let (tb, db, _, _) = duty_at(&back, &l, "1", 3);
        let (tc, dc, _, _) = duty_at(&c, &l, "1", 3);
        assert_eq!((tb, db.clone()), (tc, dc));
        assert_eq!(db.len(), 2);
        assert!(db.iter().any(|d| d.pieces.len() == 2));
        assert_eq!(back.planning.flow.sheet.notes, c.planning.flow.sheet.notes);
    }

    // --- #96: what RosterFlow saved is what the day runs ---------------------------------------

    /// `c` with `buses` buses and `people` drivers who may drive them, nobody ill or away.
    fn staffed(c: &mut Company, buses: usize, people: usize) {
        use super::super::market::{self, MarketBus, Payment};
        use super::super::staff::{applicants, hire};
        c.cash += 5_000_000_00;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..buses {
            market::buy_new(c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < people {
            for a in applicants(c) {
                if c.staff.len() < people && a.licence == crate::company::model::Licence::D {
                    hire(c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
    }

    /// Two buses' days of fourteen trips (05:00-18:50 and 05:30-19:20): two duties each, cut
    /// at noon; the roster filled.
    fn rostered() -> (Company, LineInfo) {
        let mut c = company();
        staffed(&mut c, 2, 6);
        let l = line_of(&[("1", 300, 14), ("2", 330, 14)]);
        let date = c.date.clone();
        let tours = day(&c, &l);
        super::super::plan::fill_day(&mut c, &date, tours);
        (c, l)
    }

    /// The drivers the roster gives tour `tour`'s duties, by the trip each begins with.
    fn drivers_of(c: &Company, l: &LineInfo, tour: &str) -> Vec<(i32, Option<super::super::plan::Who>)> {
        let wd = super::super::plan::weekday_of(&c.date);
        let tours = day(c, l);
        let (_, t) = block(&tours, tour);
        let r = super::super::plan::roster(c, wd, "Linie5", tour);
        network::duties_of(t).iter().enumerate().map(|(k, p)| (t.trips[p.start].dep, r.and_then(|r| r.duties.get(k).copied().flatten()))).collect()
    }

    /// The day closed as the store closes it (the trips on no bus too); the report.
    fn close(c: &mut Company, l: &LineInfo) -> super::super::day::DayReport {
        let date = c.date.clone();
        let tours = day(c, l);
        let off = network::off_every_bus(c, std::slice::from_ref(l), &date);
        super::super::day::close_day_with(c, tours, &off, &[])
    }

    /// What no bus or driver was planned for (the day's own events - an illness, a breakdown,
    /// someone late - left out): (tour, duty, trips).
    fn unplanned(r: &super::super::day::DayReport) -> Vec<(String, Option<usize>, u32)> {
        use super::super::day::DropWhy;
        use super::super::plan::Problem;
        r.drops.iter().filter(|d| d.why == DropWhy::NoBlock || d.problem == Some(Problem::Unassigned)).map(|d| (d.tour.clone(), d.duty, d.trips)).collect()
    }

    #[test]
    fn a_saved_cut_keeps_the_rosters_drivers_on_their_duties() {
        // (testers: "it shows loads didn't run, but in the planning it was allocated" - the
        // roster gives a duty its driver by its index in the bus's day, and a cut made in
        // RosterFlow moved the afternoon's driver onto the new duty of the morning and left the
        // afternoon without one: its trips were dropped)
        use super::super::rosterflow as rf;
        let (mut c, l) = rostered();
        let date = c.date.clone();
        let wd = super::super::plan::weekday_of(&date);
        let before = drivers_of(&c, &l, "1");
        assert_eq!(before.len(), 2);
        assert!(before.iter().all(|d| d.1.is_some()), "{before:?}");
        assert!(unplanned(&close(&mut c.clone(), &l)).is_empty(), "everything planned runs");
        // a cut before the 08:00 trip, in RosterFlow's draft, saved
        let mut v = rf::view(&c);
        let tours = day(&v, &l);
        let (one, _) = block(&tours, "1");
        cut(&mut v, wd, &tours, one, 3).unwrap();
        rf::keep_draft(&mut c, &v);
        // (the day runs the saved planning meanwhile, and says so)
        assert!(rf::draft_differs(&c, wd) && !rf::draft_differs(&c, (wd + 1) % 7));
        assert!(close(&mut c.clone(), &l).notes.contains(&super::super::day::Note::Unsaved));
        let f = rf::save_draft_following(&mut c, &[(date.clone(), vec![l.clone()])]).unwrap();
        assert_eq!((f.kept, f.open), (5, 0));
        // the afternoon keeps its driver; the morning's driver keeps both halves of the morning
        // until the roster gives the new duty to another
        let after = drivers_of(&c, &l, "1");
        assert_eq!(after, vec![(300, before[0].1), (480, before[0].1), (before[1].0, before[1].1)]);
        let r = close(&mut c.clone(), &l);
        assert!(unplanned(&r).is_empty(), "{:?}", r.drops);
        assert!(!r.notes.contains(&super::super::day::Note::Unsaved));
        // a duty without a driver: its trips are dropped, said why, with the duty they belong to
        super::super::plan::set_driver(&mut c, wd, "Linie5", "1", 1, None);
        let r = close(&mut c, &l);
        assert_eq!(unplanned(&r), vec![("1".to_string(), Some(1), 4)]);
        let d = r.drops.iter().find(|d| d.duty == Some(1)).unwrap();
        assert_eq!((d.why, d.from, d.to, d.number.as_str()), (super::super::day::DropWhy::NoDriver, 480, 710, "5"));
    }

    #[test]
    fn a_piece_joined_into_a_duty_takes_its_driver_when_saved() {
        use super::super::rosterflow as rf;
        let (mut c, l) = rostered();
        let date = c.date.clone();
        let wd = super::super::plan::weekday_of(&date);
        let afternoon = drivers_of(&c, &l, "2")[1].1;
        let mut v = rf::view(&c);
        let tours = day(&v, &l);
        let (one, _) = block(&tours, "1");
        cut(&mut v, wd, &tours, one, 3).unwrap();
        // (the new piece joined with tour 2's afternoon: one driver for both)
        let tours = day(&v, &l);
        let (one, t1) = block(&tours, "1");
        let (two, t2) = block(&tours, "2");
        let p2 = network::duties_of(t2)[1].clone();
        v.planning.joined.push(JoinedDuty { weekday: wd, pieces: vec![PieceRef::of(t2, &p2), PieceRef::of(t1, &network::duties_of(t1)[1])] });
        assert_eq!(partners(&v, wd, &tours, one, 1), vec![(two, 1)]);
        rf::keep_draft(&mut c, &v);
        let f = rf::save_draft_following(&mut c, &[(date, vec![l.clone()])]).unwrap();
        assert_eq!(f.open, 0);
        assert_eq!(drivers_of(&c, &l, "1")[1], (480, afternoon));
    }

    #[test]
    fn a_trip_left_on_no_bus_is_dropped_and_said_so() {
        // (RosterFlow's stack: the day left such a trip out as if the timetable had none - no
        // penalty, nothing said)
        use super::super::rosterflow as rf;
        let (mut c, l) = rostered();
        let date = c.date.clone();
        let wd = super::super::plan::weekday_of(&date);
        let mut v = rf::view(&c);
        let tours = day(&v, &l);
        let (two, t2) = block(&tours, "2");
        let last = t2.trips.len() - 1;
        unassign(&mut v, wd, &tours, two, &[last]).unwrap();
        rf::keep_draft(&mut c, &v);
        rf::save_draft_following(&mut c, &[(date.clone(), vec![l.clone()])]).unwrap();
        assert_eq!(network::off_every_bus(&c, std::slice::from_ref(&l), &date).len(), 1);
        // (tour 2's automatic cut moved a trip earlier with its last trip gone: the afternoon's
        // driver stays with the afternoon)
        assert!(drivers_of(&c, &l, "2").iter().all(|d| d.1.is_some()));
        let left_out = super::super::day::close_day(&mut c.clone(), day(&c, &l), &[]);
        let r = close(&mut c, &l);
        assert_eq!(unplanned(&r), vec![(String::new(), None, 1)]);
        assert_eq!((r.trips, r.dropped), (left_out.trips + 1, left_out.dropped + 1));
        assert!(r.penalties > left_out.penalties);
        // (another weekday runs its own blocks: Tuesday all of them)
        let tue = tours_of_day(&c, std::slice::from_ref(&l), &dates_add(&date, 1));
        assert_eq!(tue.iter().map(|t| t.trips.len()).sum::<usize>(), 28);
    }

    fn dates_add(date: &str, k: i64) -> String {
        super::super::dates::add(date, k)
    }
}

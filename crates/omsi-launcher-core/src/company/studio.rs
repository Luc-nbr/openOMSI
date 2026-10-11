//! The scheduling studio (the planning, phase 1; Luc: the bus company's planning should look
//! and work "like Optibus Scheduling"): the day's vehicle blocks - a bus's day, to begin with
//! a tour of the timetable - and the driver duties cut from them, seen and edited by hand in
//! the advanced mode.
//!
//! A block keeps to its line. The player moves a trip with passengers to another bus of the
//! line, or to a new one; he cuts a block into duties, only where the bus stands at one of the
//! line's termini or at the depot (Luc: "relief points are ONLY a line's termini and the
//! depot"), and joins them again; and he joins pieces of different blocks into one driver's
//! duty (a split duty, or one that changes buses). The working-time rules (`staff::check`)
//! refuse what may not be and warn of the rest. What he lays out is kept with the company a
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
}

/// A line's blocks on a weekday as the player laid them out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LineLayout {
    /// 0 Monday .. 6 Sunday.
    pub weekday: u8,
    /// The company line (its timetable name).
    pub line: String,
    pub blocks: Vec<BlockLayout>,
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
fn build(l: &LineLayout, tours: &[&TourOfDay]) -> Option<Vec<(TourOfDay, Option<Vec<TripRef>>)>> {
    let template = tours.first()?;
    let mut pool: Vec<(&PlannedTrip, bool)> = tours.iter().flat_map(|t| t.trips.iter()).map(|x| (x, false)).collect();
    let mut out = Vec::new();
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
        // (its depot group: the timetable tour's of its name, else the one its first trip
        // comes from)
        let group = tours.iter().find(|t| same(&t.tour, &b.tour)).or_else(|| tours.iter().find(|t| same(&t.tour, &trips[0].origin))).map(|t| t.ai_group.clone()).unwrap_or_default();
        let t = TourOfDay { line: template.line.clone(), number: template.number.clone(), tour: b.tour.clone(), ai_group: group, trips, unplanned: false, cuts: None };
        out.push((t, b.cuts.clone()));
    }
    if pool.iter().any(|(x, used)| !*used && x.counts()) {
        return None;
    }
    Some(out)
}

/// The day's tours of the timetable (`network::timetable_tours`) as the player laid out their
/// lines on `weekday`. Returns the blocks whose duties he cut, with the trips they begin with
/// (found once the depot runs are in, `resolve_cuts`).
pub fn arrange(c: &Company, weekday: u8, tours: &mut Vec<TourOfDay>) -> Vec<(usize, Vec<TripRef>)> {
    let mut laid: Vec<Vec<(TourOfDay, Option<Vec<TripRef>>)>> = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for l in c.planning.layouts.iter().filter(|l| l.weekday == weekday) {
        let mine: Vec<&TourOfDay> = tours.iter().filter(|t| same(&t.line, &l.line)).collect();
        if let Some(blocks) = build(l, &mine) {
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
    let wd = super::plan::weekday_of(date);
    let raw = network::timetable_tours(c, lines);
    c.planning
        .layouts
        .iter()
        .filter(|l| l.weekday == wd)
        .filter(|l| {
            let mine: Vec<&TourOfDay> = raw.iter().filter(|t| same(&t.line, &l.line)).collect();
            !mine.is_empty() && build(l, &mine).is_none()
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
    let wd = super::plan::weekday_of(date);
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
pub const NOT_RELIEF: &str = "Drivers change only at the line's termini or at the depot.";
pub const NO_PASSENGERS: &str = "Each duty needs a trip with passengers.";
pub const NO_CUT: &str = "No duty begins there.";
pub const TOO_LONG: &str = "That duty would be longer than a working day may be (10 h).";
pub const SAME_DUTY: &str = "That is the same duty.";
pub const NOT_JOINED: &str = "This duty is not joined with another.";
pub const NOT_FOUND: &str = "The planning changed meanwhile: try again.";

const REFUSALS: [&str; 18] = [OVERLAP, NO_TIME, IN_DEPOT, BACK_IN_DEPOT, OTHER_LINE, NOT_MOVABLE, SAME_BUS, ALONE, BETWEEN, CUT_ALREADY, ON_THE_MOVE, NOT_RELIEF, NO_PASSENGERS, NO_CUT, TOO_LONG, SAME_DUTY, NOT_JOINED, NOT_FOUND];

/// Where a trip is moved to: a bus of the day (a block's index), or a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Block(usize),
    New,
}

/// The line's blocks as the day has them now (`tours` as `network::tours_of_day` gives them):
/// the start of a layout to change. The depot runs the company adds itself are left out
/// (added again), and so are cuts the player did not make.
fn layout_now(tours: &[TourOfDay], line: &str, weekday: u8) -> LineLayout {
    let blocks = tours
        .iter()
        .filter(|t| same(&t.line, line))
        .map(|t| BlockLayout {
            tour: t.tour.clone(),
            trips: t.trips.iter().filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect(),
            cuts: t.cuts.as_ref().map(|cs| cs.iter().filter_map(|&k| t.trips.get(k)).map(TripRef::of).collect()),
        })
        .collect();
    LineLayout { weekday, line: line.to_string(), blocks }
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

/// How long a bus takes from `a` to `b` empty: the line's quickest trip between them, else
/// `DEADHEAD_MIN` (none between one place and itself).
fn travel(trips: &[&PlannedTrip], a: &str, b: &str) -> i32 {
    if same(a, b) {
        return 0;
    }
    trips.iter().filter(|x| same(&x.from, a) && same(x.end(), b)).map(|x| x.arr - x.dep).min().unwrap_or(DEADHEAD_MIN)
}

/// The kilometres a bus runs empty from `a` to `b`: the line's shortest trip between them,
/// else a depot run's.
fn deadhead_km(trips: &[&PlannedTrip], a: &str, b: &str) -> f64 {
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

/// A free name for a new block of the line: the next number after its blocks' and the
/// roster's (a new bus takes no bus or driver given to an old one).
fn new_name(c: &Company, weekday: u8, tours: &[TourOfDay], line: &str) -> String {
    let taken: Vec<String> = tours.iter().filter(|t| same(&t.line, line)).map(|t| t.tour.trim().to_string()).chain(c.planning.week.iter().filter(|r| r.weekday == weekday && same(&r.line, line)).map(|r| r.tour.trim().to_string())).collect();
    let mut n = taken.iter().filter_map(|t| t.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    while taken.iter().any(|t| *t == n.to_string()) {
        n += 1;
    }
    n.to_string()
}

/// What moving trip `trip` of block `from` to `to` would make of its line (None: refused,
/// and why).
pub fn plan_move(c: &Company, weekday: u8, tours: &[TourOfDay], from: usize, trip: usize, to: Target) -> Result<LineLayout, &'static str> {
    let src = tours.get(from).ok_or(NOT_FOUND)?;
    let x = src.trips.get(trip).ok_or(NOT_FOUND)?;
    if !x.counts() || x.origin.trim().is_empty() {
        return Err(NOT_MOVABLE);
    }
    let line_trips: Vec<&PlannedTrip> = tours.iter().filter(|t| same(&t.line, &src.line)).flat_map(|t| t.trips.iter()).collect();
    let mut lay = layout_now(tours, &src.line, weekday);
    let r = TripRef::of(x);
    match to {
        Target::Block(i) => {
            let dst = tours.get(i).ok_or(NOT_FOUND)?;
            if i == from {
                return Err(SAME_BUS);
            }
            if !same(&dst.line, &src.line) {
                return Err(OTHER_LINE);
            }
            fits_into(&dst.trips, x, &line_trips)?;
            let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &dst.tour)).ok_or(NOT_FOUND)?;
            b.trips.push(r.clone());
        }
        Target::New => {
            if src.trips.iter().filter(|t| t.counts()).count() == 1 {
                return Err(ALONE);
            }
            lay.blocks.push(BlockLayout { tour: new_name(c, weekday, tours, &src.line), trips: vec![r.clone()], cuts: None });
        }
    }
    let b = lay.blocks.iter_mut().find(|b| same(&b.tour, &src.tour)).ok_or(NOT_FOUND)?;
    b.trips.retain(|t| *t != r);
    if let Some(cuts) = b.cuts.as_mut() {
        cuts.retain(|t| *t != r);
    }
    // (a bus left without a trip with passengers runs no more)
    let counted = |b: &BlockLayout| b.trips.iter().any(|r| line_trips.iter().any(|x| r.is(x) && x.counts()));
    lay.blocks.retain(counted);
    for b in lay.blocks.iter_mut() {
        b.trips.sort_by_key(|t| t.dep);
    }
    Ok(lay)
}

/// Move trip `trip` of block `from` to `to` (`plan_move`): kept for the weekday. The duties
/// joined with a piece of either bus are taken apart.
pub fn move_trip(c: &mut Company, weekday: u8, tours: &[TourOfDay], from: usize, trip: usize, to: Target) -> Result<(), &'static str> {
    let lay = plan_move(c, weekday, tours, from, trip, to)?;
    let src = &tours[from];
    let mut touched = vec![src.tour.as_str()];
    if let Target::Block(i) = to {
        touched.push(tours[i].tour.as_str());
    }
    unjoin_blocks(c, weekday, &src.line, &touched);
    save(c, lay);
    Ok(())
}

/// Whether `place` is one of the line's termini (where its trips with passengers begin or
/// end) or the depot (where its blocks' depot runs begin or end, or the company's depot).
pub fn is_relief(c: &Company, tours: &[TourOfDay], line: &str, place: &str) -> bool {
    let place = place.trim();
    if place.is_empty() {
        return false;
    }
    let mine = || tours.iter().filter(|t| same(&t.line, line));
    let terminus = mine().flat_map(|t| t.trips.iter()).filter(|x| x.counts()).any(|x| same(&x.from, place) || same(x.end(), place));
    let depot = same(place, "Depot") || (!c.depot.trim().is_empty() && same(place, c.depot.trim_end_matches(".hof"))) || mine().any(|t| t.trips.first().is_some_and(|x| x.empty && same(&x.from, place)) || t.trips.last().is_some_and(|x| x.empty && same(x.end(), place)));
    terminus || depot
}

/// The trips block `t`'s duties begin with now (the first left out).
fn starts(t: &TourOfDay) -> Vec<usize> {
    network::duties_of(t).iter().skip(1).map(|r| r.start).collect()
}

/// The block's cuts kept as these starts.
fn set_cuts(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: &[usize]) {
    let t = &tours[i];
    let mut lay = layout_now(tours, &t.line, weekday);
    if let Some(b) = lay.blocks.iter_mut().find(|b| same(&b.tour, &t.tour)) {
        b.cuts = Some(at.iter().filter_map(|&k| t.trips.get(k)).map(TripRef::of).collect());
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
    let (a, b) = (&t.trips[at - 1], &t.trips[at]);
    if !same(a.end(), &b.from) {
        return Err(ON_THE_MOVE);
    }
    if !is_relief(c, tours, &t.line, a.end()) {
        return Err(NOT_RELIEF);
    }
    let p = pieces.iter().find(|r| r.contains(&at)).ok_or(NOT_FOUND)?;
    if !t.trips[p.start..at].iter().any(PlannedTrip::counts) || !t.trips[at..p.end].iter().any(PlannedTrip::counts) {
        return Err(NO_PASSENGERS);
    }
    Ok(())
}

/// Cut block `i` before its trip `at`: a new duty begins there (`can_cut`).
pub fn cut(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    can_cut(c, tours, i, at)?;
    let mut s = starts(&tours[i]);
    s.push(at);
    s.sort_unstable();
    set_cuts(c, weekday, tours, i, &s);
    Ok(())
}

/// Join the duty of block `i` that begins with trip `at` to the one before it: the cut taken
/// out, unless the duty would be longer than `DAY_MAX`.
pub fn uncut(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(NOT_FOUND)?;
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
    let before = c.planning.joined.len();
    c.planning.joined.retain(|j| !(j.weekday == weekday && j.pieces.iter().any(|x| x.same(&me))));
    if c.planning.joined.len() == before {
        return Err(NOT_JOINED);
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
        c.mode = super::super::Mode::Advanced;
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
        c.mode = super::super::Mode::Easy;
        assert_eq!(day(&c, &l).len(), 2);
        c.mode = super::super::Mode::Advanced;
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
}

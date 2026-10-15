//! Which plan each day of the company runs, and a plan copied to the other days it works with
//! (testers, 2026-10-14: "once you have got a duty and vehicle plan perfect, to be able to copy
//! it to the days it works with - if you copy it to a day it does not work well with then it
//! will only copy the parts that match"; Trello #100).
//!
//! RosterFlow keeps its blocks and duties a weekday at a time (`studio::LineLayout::weekday`,
//! `JoinedDuty::weekday`, the sheet's `weekday`s): Monday's plan runs every Monday. That number
//! is the day's *slot*: 0 - 6 the weekdays, from `FIRST_DATE_SLOT` on a plan of single dates
//! (`Days::dates`) - an exception for one day, an event or a holiday's timetable, that leaves
//! the weekday's plan as it is. `slot_of` gives a date's slot, and whatever reads a day's plan
//! asks it (`network::tours_of_day`, the board, Optimise, the day's plan, the stale check).
//!
//! A copy (`copy`) makes a day's plan out of another's on the day's own timetable. Each trip of
//! the source is matched to a trip of the day by its line, its way (where it leaves from and
//! where it ends), its departure and its number of stops. What matches is laid out as the
//! source has it: the same buses by name, with their locks and links, and the same duties,
//! their cuts, joins, notes and spare duties. A trip of the day that matches nothing runs as
//! the day's timetable has it - on its tour's bus, named anew where the name is taken - and is
//! listed, to plan by hand. A day with the same timetable gets an exact copy. Vehicles and
//! drivers are copied together or apart.
//!
//! The copied day follows its source (`Follow`): changed afterwards it is an override, with
//! "Reset to pattern" (`recopy`); its source changed since, it is UNSYNCED and a Refresh copies
//! it again. A copy is the day's own plan like one made by hand, so the game and the day's
//! close run it. (The roster - which bus and driver - stays a weekday's: `plan::RosterTour`.)
//!
//! A plan copied to a day of another timetable finds little to match there: Saturday and Sunday
//! run a timetable of their own. The Timeplan's "Copy to…" (`timetable`) copies the timetable
//! itself first - a line's trips of one day onto others, or a template of them kept by name -
//! after which the plan copies onto those days as onto a day of the same timetable.

use super::model::Company;
use super::network::{self, PlannedTrip, TourOfDay};
use super::plan;
use super::rosterflow::{self as rf, BlockLock, Draft, DutyNote, Link, Sheet, Spare};
use super::studio::{self as st, BlockLayout, JoinedDuty, LineLayout, MidRelief, TripRef};
use crate::LineInfo;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// The first slot of a plan of single dates (0 - 6 are the weekdays, 0 Monday).
pub const FIRST_DATE_SLOT: u8 = 7;

/// Which plan each day runs, besides its weekday's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Days {
    /// The dates that run a plan of their own (or one they share with other dates).
    pub dates: Vec<DateSlot>,
    /// The days made by copying another.
    pub follows: Vec<Follow>,
    /// The Timeplan's templates: a line's timetable of a day kept by name, to copy onto other
    /// days ("Copy to…" from it, `timetable`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<Template>,
}

impl Days {
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty() && self.follows.is_empty() && self.templates.is_empty()
    }
}

/// A date and the slot of the plan it runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DateSlot {
    pub date: String,
    pub slot: u8,
}

/// What a copy takes: the vehicles (the buses' blocks, their locks and links, the stack), the
/// drivers (the duties: their cuts, joins, notes and spare duties), or both.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct What {
    pub vehicles: bool,
    pub drivers: bool,
}

impl What {
    pub const ALL: What = What { vehicles: true, drivers: true };
}

/// A day made by copying another (its pattern): what was copied, the day's fingerprint just
/// after (0: a plan of its own from the start, `fork_draft`) and its pattern's then, and what
/// matched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Follow {
    pub day: u8,
    pub pattern: u8,
    pub vehicles: bool,
    pub drivers: bool,
    pub print: u64,
    pub source: u64,
    pub matched: u32,
    pub total: u32,
    pub missing: Vec<Missed>,
}

/// A trip of the day that matched none of its pattern's (or did not fit the bus it matched):
/// it runs as the day's timetable has it, on block `block`, to plan by hand.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Missed {
    /// The company line (its timetable name) and its number.
    pub line: String,
    pub number: String,
    pub block: String,
    pub dep: i32,
    pub arr: i32,
    pub from: String,
    pub to: String,
}

/// What a copy made of a day.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Copied {
    pub slot: u8,
    /// The day's trips with passengers laid out as the pattern has them, of all it runs.
    pub matched: u32,
    pub total: u32,
    /// The pattern's trips with passengers the day does not run.
    pub gone: u32,
    pub missing: Vec<Missed>,
}

impl Copied {
    /// Every trip matched, both ways: the same timetable.
    pub fn exact(&self) -> bool {
        self.missing.is_empty() && self.gone == 0
    }
}

/// A day and its timetable (`list_lines` of that date).
#[derive(Clone, Copy)]
pub struct Day<'a> {
    pub date: &'a str,
    pub lines: &'a [LineInfo],
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

// --- the days' slots ------------------------------------------------------------------------------

/// The slot of the plan `date` runs: a plan of its own, else its weekday's.
pub fn slot_of(c: &Company, date: &str) -> u8 {
    c.planning.flow.days.dates.iter().find(|d| d.date == date.trim()).map(|d| d.slot).unwrap_or_else(|| plan::weekday_of(date))
}

/// The dates that run the plan of `slot` (a plan of single dates).
pub fn dates_of(c: &Company, slot: u8) -> Vec<String> {
    c.planning.flow.days.dates.iter().filter(|d| d.slot == slot).map(|d| d.date.clone()).collect()
}

/// What `slot` follows, if it was copied.
pub fn follow_of(c: &Company, slot: u8) -> Option<&Follow> {
    c.planning.flow.days.follows.iter().find(|f| f.day == slot)
}

/// A date of `window` that runs the plan of `slot` (for a weekday's plan, a date of that
/// weekday without a plan of its own): where a copy of it is taken from.
pub fn date_for(c: &Company, slot: u8, window: &[String]) -> Option<String> {
    window.iter().find(|d| slot_of(c, d) == slot).cloned()
}

/// The slots in use: the weekdays, and those the dates run.
fn used(c: &Company) -> HashSet<u8> {
    (0..FIRST_DATE_SLOT).chain(c.planning.flow.days.dates.iter().map(|d| d.slot)).collect()
}

/// A slot no date and no plan has.
fn free_slot(c: &Company) -> u8 {
    let p = &c.planning;
    let mut taken: HashSet<u8> = used(c);
    taken.extend(p.layouts.iter().map(|l| l.weekday));
    taken.extend(p.joined.iter().map(|j| j.weekday));
    taken.extend(p.flow.days.follows.iter().flat_map(|f| [f.day, f.pattern]));
    if let Some(d) = &p.flow.draft {
        taken.extend(d.layouts.iter().map(|l| l.weekday));
        taken.extend(d.joined.iter().map(|j| j.weekday));
    }
    (FIRST_DATE_SLOT..=u8::MAX).find(|s| !taken.contains(s)).unwrap_or(u8::MAX)
}

fn set_date(c: &mut Company, date: &str, slot: u8) {
    let d = &mut c.planning.flow.days.dates;
    d.retain(|x| x.date != date.trim());
    d.push(DateSlot { date: date.trim().to_string(), slot });
    d.sort_by(|a, b| a.date.cmp(&b.date));
}

/// A slot of `date`'s own: the one it runs when no other date runs it too, else a new one.
pub fn own_slot(c: &mut Company, date: &str) -> u8 {
    let s = slot_of(c, date);
    if s >= FIRST_DATE_SLOT && dates_of(c, s).len() == 1 {
        return s;
    }
    let f = free_slot(c);
    set_date(c, date, f);
    f
}

/// `date` runs its weekday's plan again (its own plan goes). Returns whether it had one.
pub fn drop_date(c: &mut Company, date: &str) -> bool {
    let before = c.planning.flow.days.dates.len();
    c.planning.flow.days.dates.retain(|d| d.date != date.trim());
    let gone = before != c.planning.flow.days.dates.len();
    gc(c);
    gone
}

/// The plans of single dates that are past now go, and what no date runs any more.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    c.planning.flow.days.dates.retain(|d| d.date.as_str() >= today.as_str());
    gc(c);
}

/// The plans of slots no date runs taken out (the saved ones and the draft's), and what
/// followed them.
fn gc(c: &mut Company) {
    let used = used(c);
    let gone = |s: u8| s >= FIRST_DATE_SLOT && !used.contains(&s);
    let p = &mut c.planning;
    p.flow.days.follows.retain(|f| !gone(f.day) && !gone(f.pattern));
    p.layouts.retain(|l| !gone(l.weekday));
    p.joined.retain(|j| !gone(j.weekday));
    keep_sheet(&mut p.flow.sheet, &|s| !gone(s));
    if let Some(d) = p.flow.draft.as_mut() {
        d.layouts.retain(|l| !gone(l.weekday));
        d.joined.retain(|j| !gone(j.weekday));
        keep_sheet(&mut d.sheet, &|s| !gone(s));
    }
}

fn keep_sheet(s: &mut Sheet, keep: &dyn Fn(u8) -> bool) {
    s.locks.retain(|x| keep(x.weekday));
    s.links.retain(|x| keep(x.weekday));
    s.notes.retain(|x| keep(x.weekday));
    s.spares.retain(|x| keep(x.weekday));
    s.dismissed.retain(|x| keep(x.0));
}

/// The plan of `slot` in these blocks, duties and sheet.
fn part_of(layouts: &[LineLayout], joined: &[JoinedDuty], sheet: &Sheet, slot: u8) -> Draft {
    Draft {
        layouts: layouts.iter().filter(|l| l.weekday == slot).cloned().collect(),
        joined: joined.iter().filter(|j| j.weekday == slot).cloned().collect(),
        sheet: Sheet {
            locks: sheet.locks.iter().filter(|x| x.weekday == slot).cloned().collect(),
            links: sheet.links.iter().filter(|x| x.weekday == slot).cloned().collect(),
            notes: sheet.notes.iter().filter(|x| x.weekday == slot).cloned().collect(),
            spares: sheet.spares.iter().filter(|x| x.weekday == slot).cloned().collect(),
            dismissed: sheet.dismissed.iter().filter(|x| x.0 == slot).cloned().collect(),
        },
    }
}

/// `part` the plan of `slot` in `d` (what `d` had of it goes).
fn put_part(d: &mut Draft, slot: u8, part: Draft) {
    d.layouts.retain(|l| l.weekday != slot);
    d.joined.retain(|j| j.weekday != slot);
    keep_sheet(&mut d.sheet, &|s| s != slot);
    d.layouts.extend(part.layouts.into_iter().map(|l| LineLayout { weekday: slot, ..l }));
    d.joined.extend(part.joined.into_iter().map(|j| JoinedDuty { weekday: slot, ..j }));
    let s = part.sheet;
    d.sheet.locks.extend(s.locks.into_iter().map(|x| BlockLock { weekday: slot, ..x }));
    d.sheet.links.extend(s.links.into_iter().map(|x| Link { weekday: slot, ..x }));
    d.sheet.notes.extend(s.notes.into_iter().map(|x| DutyNote { weekday: slot, ..x }));
    d.sheet.spares.extend(s.spares.into_iter().map(|x| Spare { weekday: slot, ..x }));
    d.sheet.dismissed.extend(s.dismissed.into_iter().map(|x| (slot, x.1)));
}

/// "Save for this date only": the draft's change of the plan `date` runs becomes a plan of the
/// date's own - the plan it ran before (its weekday's, or one it shares) stays as it was saved.
/// Returns the date's slot. (A date with a plan of its own already keeps it.)
pub fn fork_draft(c: &mut Company, date: &str) -> u8 {
    let s = slot_of(c, date);
    if s >= FIRST_DATE_SLOT && dates_of(c, s).len() == 1 {
        return s;
    }
    let f = free_slot(c);
    let mut d = rf::draft_of(c);
    let mine = part_of(&d.layouts, &d.joined, &d.sheet, s);
    let saved = part_of(&c.planning.layouts, &c.planning.joined, &c.planning.flow.sheet, s);
    put_part(&mut d, s, saved);
    put_part(&mut d, f, mine);
    set_date(c, date, f);
    rf::set_draft(c, d);
    let days = &mut c.planning.flow.days;
    days.follows.retain(|x| x.day != f);
    days.follows.push(Follow { day: f, pattern: s, vehicles: true, drivers: true, ..Default::default() });
    f
}

/// "Reset to pattern" of a date's own plan made by `fork_draft`: the date runs what it ran
/// before again. Returns whether it had one.
pub fn unfork(c: &mut Company, date: &str) -> bool {
    let s = slot_of(c, date);
    let Some(f) = follow_of(c, s).filter(|f| f.print == 0).cloned() else { return false };
    if f.pattern >= FIRST_DATE_SLOT && used(c).contains(&f.pattern) {
        set_date(c, date, f.pattern);
        gc(c);
        true
    } else {
        drop_date(c, date)
    }
}

/// The slots whose plan the draft changed.
pub fn changed_slots(c: &Company) -> Vec<u8> {
    let p = &c.planning;
    let Some(d) = &p.flow.draft else { return Vec::new() };
    let mut slots: Vec<u8> = p.layouts.iter().chain(d.layouts.iter()).map(|l| l.weekday).chain(p.joined.iter().chain(d.joined.iter()).map(|j| j.weekday)).collect();
    let sh = [&p.flow.sheet, &d.sheet];
    slots.extend(sh.iter().flat_map(|s| s.locks.iter().map(|x| x.weekday).chain(s.links.iter().map(|x| x.weekday)).chain(s.notes.iter().map(|x| x.weekday)).chain(s.spares.iter().map(|x| x.weekday)).chain(s.dismissed.iter().map(|x| x.0))));
    slots.sort_unstable();
    slots.dedup();
    slots.retain(|&s| part_of(&d.layouts, &d.joined, &d.sheet, s) != part_of(&p.layouts, &p.joined, &p.flow.sheet, s));
    slots
}

/// The draft takes the saved plan of `slot`: what it changed there is forgotten.
pub fn draft_takes_saved(c: &mut Company, slot: u8) {
    if c.planning.flow.draft.is_none() {
        return;
    }
    let mut d = rf::draft_of(c);
    let saved = part_of(&c.planning.layouts, &c.planning.joined, &c.planning.flow.sheet, slot);
    put_part(&mut d, slot, saved);
    rf::set_draft(c, d);
}

/// The copied days in step with their pattern now (neither changed since the copy).
pub fn in_step(c: &Company) -> Vec<u8> {
    c.planning.flow.days.follows.iter().map(|f| f.day).filter(|&d| state(c, d).is_in_step()).collect()
}

/// These copied days are in step with their pattern as both are now (both changed alike, as
/// a timetable's Refresh changes them).
pub fn restep(c: &mut Company, slots: &[u8]) {
    let now: Vec<(u8, u64, u64)> = c.planning.flow.days.follows.iter().filter(|f| f.print != 0 && slots.contains(&f.day)).map(|f| {
        let w = What { vehicles: f.vehicles, drivers: f.drivers };
        (f.day, print(c, f.day, w), print(c, f.pattern, w))
    }).collect();
    for f in c.planning.flow.days.follows.iter_mut() {
        if let Some(n) = now.iter().find(|n| n.0 == f.day) {
            (f.print, f.source) = (n.1, n.2);
        }
    }
}

// --- the fingerprint ------------------------------------------------------------------------------

/// FNV-1a: the same number for the same text on every machine and with every compiler.
fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What the plan of `slot` holds of `w`, as a number: the same plan, the same number, wherever
/// it stands among the company's (the weekday left out).
pub fn print(c: &Company, slot: u8, w: What) -> u64 {
    let p = &c.planning;
    let low = |s: &str| s.trim().to_lowercase();
    let sorted_refs = |v: &[TripRef]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| (a.dep, &a.name, &a.tour).cmp(&(b.dep, &b.name, &b.tour)));
        v
    };
    let mut lays: Vec<String> = p
        .layouts
        .iter()
        .filter(|l| l.weekday == slot)
        .map(|l| {
            let mut blocks: Vec<(String, Vec<TripRef>, Option<Vec<TripRef>>)> = l.blocks.iter().map(|b| (low(&b.tour), if w.vehicles { sorted_refs(&b.trips) } else { Vec::new() }, if w.drivers { b.cuts.as_deref().map(sorted_refs) } else { None })).collect();
            blocks.sort_by(|a, b| a.0.cmp(&b.0));
            json!([low(&l.line), blocks, if w.vehicles { sorted_refs(&l.stack) } else { Vec::new() }]).to_string()
        })
        .collect();
    lays.sort();
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    let sheet = &p.flow.sheet;
    let mut parts = vec![json!(lays)];
    if w.vehicles {
        parts.push(json!(sorted(sheet.locks.iter().filter(|x| x.weekday == slot).map(|x| format!("{}|{}", low(&x.line), low(&x.tour))).collect())));
        parts.push(json!(sorted(sheet.links.iter().filter(|x| x.weekday == slot).map(|x| json!([low(&x.line), sorted_refs(&x.trips)]).to_string()).collect())));
    }
    if w.drivers {
        parts.push(json!(sorted(
            p.joined
                .iter()
                .filter(|j| j.weekday == slot)
                .map(|j| sorted(j.pieces.iter().map(|x| json!([low(&x.line), low(&x.tour), x.first]).to_string()).collect()).join(";"))
                .collect()
        )));
        parts.push(json!(sorted(sheet.notes.iter().filter(|x| x.weekday == slot).map(|x| json!(DutyNote { weekday: 0, ..x.clone() }).to_string()).collect())));
        parts.push(json!(sorted(sheet.spares.iter().filter(|x| x.weekday == slot).map(|x| json!(Spare { weekday: 0, ..x.clone() }).to_string()).collect())));
    }
    fnv(&json!(parts).to_string())
}

// --- matching -------------------------------------------------------------------------------------

/// A trip of the source day: its line, its block and place there (None: on the stack).
struct Src {
    line: String,
    at: Option<(usize, usize)>,
    x: PlannedTrip,
}

/// The source's trips the timetable knows: those of its blocks, and those on its stack.
fn sources(c: &Company, slot: u8, from: &[TourOfDay], lines: &[LineInfo]) -> Vec<Src> {
    let mut out = Vec::new();
    for (i, t) in from.iter().enumerate() {
        for (k, x) in t.trips.iter().enumerate().filter(|(_, x)| !x.origin.trim().is_empty()) {
            out.push(Src { line: t.line.clone(), at: Some((i, k)), x: x.clone() });
        }
    }
    for (line, x) in rf::stack(c, lines, slot) {
        out.push(Src { line, at: None, x });
    }
    out
}

/// Whether trip `b` of the day is trip `a` of the source: the same way (where it leaves from and
/// where it ends), the same departure and as many stops; with passengers or without alike.
pub fn alike(a: &PlannedTrip, b: &PlannedTrip) -> bool {
    a.dep == b.dep && a.empty == b.empty && a.stops == b.stops && same(&a.from, &b.from) && same(a.end(), b.end())
}

/// The day's trip (a tour of `raw` and its place) each source trip is, of its own line: the
/// same name and tour before another of the same way and time; each taken once.
fn matching(srcs: &[Src], raw: &[TourOfDay]) -> Vec<Option<(usize, usize)>> {
    let mut used: HashSet<(usize, usize)> = HashSet::new();
    let mut order: Vec<usize> = (0..srcs.len()).collect();
    order.sort_by_key(|&i| (srcs[i].x.dep, i));
    let mut out = vec![None; srcs.len()];
    for i in order {
        let s = &srcs[i];
        let mut best: Option<((usize, usize), u8)> = None;
        for (ti, t) in raw.iter().enumerate().filter(|(_, t)| same(&t.line, &s.line)) {
            for (k, x) in t.trips.iter().enumerate() {
                if x.origin.trim().is_empty() || used.contains(&(ti, k)) || !alike(&s.x, x) {
                    continue;
                }
                let score = 2 * u8::from(x.name == s.x.name) + u8::from(same(&x.origin, &s.x.origin));
                if best.is_none_or(|b| score > b.1) {
                    best = Some(((ti, k), score));
                }
            }
        }
        if let Some((id, _)) = best {
            used.insert(id);
            out[i] = Some(id);
        }
    }
    out
}

fn missed(t: &TourOfDay, x: &PlannedTrip, block: &str) -> Missed {
    Missed { line: t.line.clone(), number: t.number.trim().to_string(), block: block.trim().to_string(), dep: x.dep, arr: x.arr, from: x.from.trim().to_string(), to: x.end().trim().to_string() }
}

/// A name for a bus of the line none of `names` has: the next number after theirs.
fn fresh_name(names: &[String]) -> String {
    let mut n = names.iter().filter_map(|t| t.trim().parse::<u32>().ok()).max().unwrap_or(0) + 1;
    while names.iter().any(|t| t.trim() == n.to_string()) {
        n += 1;
    }
    n.to_string()
}

/// A trip as a layout names it: a part of a trip cut on the way by the whole trip's departure.
fn whole_ref(t: &PlannedTrip) -> TripRef {
    TripRef { tour: t.origin.trim().to_string(), name: t.name.clone(), dep: t.whole_dep() }
}

/// A line's blocks as the day has them (`tours`), to change.
fn layout_of(slot: u8, line: &str, tours: &[&TourOfDay]) -> LineLayout {
    let blocks = tours
        .iter()
        .map(|t| BlockLayout {
            tour: t.tour.clone(),
            // (a trip cut at a relief point on the way is laid out whole, its relief beside it)
            trips: t.trips.iter().filter(|x| !x.origin.trim().is_empty() && !x.is_second()).map(whole_ref).collect(),
            cuts: t.cuts.as_ref().map(|cs| cs.iter().filter_map(|&k| t.trips.get(k)).filter(|x| !x.is_second()).map(whole_ref).collect()),
            reliefs: t
                .trips
                .iter()
                .filter_map(|x| x.part.as_ref().filter(|p| p.second).map(|p| MidRelief { trip: TripRef { tour: x.origin.trim().to_string(), name: x.name.clone(), dep: p.dep }, stop: p.stop, name: p.at.clone() }))
                .collect(),
        })
        .collect();
    LineLayout { weekday: slot, line: line.to_string(), blocks, stack: Vec::new() }
}

// --- the copy -------------------------------------------------------------------------------------

/// The plan of slot `from_slot` (as it runs on `src`) copied onto the day `dst` as the plan of
/// slot `to_slot`, what `w` says of it (the rest of the slot's plan is kept). See the module.
pub fn copy(c: &mut Company, from_slot: u8, src: Day, to_slot: u8, dst: Day, w: What) -> Copied {
    let mut made = Copied { slot: to_slot, ..Default::default() };
    if from_slot == to_slot {
        return made;
    }
    let from = network::tours_of_slot(c, src.lines, src.date, from_slot);
    let srcs = sources(c, from_slot, &from, src.lines);
    let raw = network::timetable_tours(c, dst.lines);
    let found = matching(&srcs, &raw);
    let real = |x: &PlannedTrip| x.counts() && !x.origin.trim().is_empty();
    made.gone = srcs.iter().zip(&found).filter(|(s, m)| m.is_none() && s.x.counts()).count() as u32;
    made.total = raw.iter().flat_map(|t| t.trips.iter()).filter(|x| real(x)).count() as u32;
    let mut lines: Vec<String> = Vec::new();
    for l in c.lines.iter().map(|l| l.name.clone()).chain(from.iter().chain(raw.iter()).map(|t| t.line.clone())) {
        if !lines.iter().any(|x| same(x, &l)) {
            lines.push(l);
        }
    }
    let laid_src: Vec<String> = c.planning.layouts.iter().filter(|l| l.weekday == from_slot).map(|l| l.line.clone()).collect();
    let src_laid = |l: &str| laid_src.iter().any(|x| same(x, l));
    // (the day's trip a source trip is, as the day knows it)
    let dst_ref = |m: Option<(usize, usize)>| m.map(|id| TripRef::of(&raw[id.0].trips[id.1]));
    if w.vehicles {
        let mut claimed: HashSet<(usize, usize)> = HashSet::new();
        let mut layouts: Vec<LineLayout> = Vec::new();
        let mut copied: Vec<(String, String)> = Vec::new();
        for line in lines.iter().filter(|l| src_laid(l)) {
            let mut blocks: Vec<BlockLayout> = Vec::new();
            let mut names: Vec<String> = Vec::new();
            for (bi, t) in from.iter().enumerate().filter(|(_, t)| same(&t.line, line)) {
                let mut got: Vec<(usize, usize)> = srcs.iter().zip(&found).filter(|(s, _)| s.at.is_some_and(|a| a.0 == bi)).filter_map(|(_, m)| *m).collect();
                got.sort_by_key(|id| raw[id.0].trips[id.1].dep);
                // (a trip that runs while the bus still runs the one before - its running
                // times differ that day - stays on its own tour's bus)
                let mut kept: Vec<(usize, usize)> = Vec::new();
                let mut free_at = i32::MIN;
                for id in got {
                    let x = &raw[id.0].trips[id.1];
                    if x.dep < free_at {
                        continue;
                    }
                    free_at = x.arr;
                    kept.push(id);
                }
                if !kept.iter().any(|id| raw[id.0].trips[id.1].counts()) {
                    continue;
                }
                claimed.extend(kept.iter().copied());
                names.push(t.tour.trim().to_string());
                copied.push((line.clone(), t.tour.clone()));
                blocks.push(BlockLayout { tour: t.tour.clone(), trips: kept.iter().map(|id| TripRef::of(&raw[id.0].trips[id.1])).collect(), cuts: None, reliefs: Vec::new() });
            }
            // what the source left unassigned waits on the stack here too
            let mut stack: Vec<TripRef> = Vec::new();
            for (s, m) in srcs.iter().zip(&found) {
                if let (None, Some(id)) = (s.at, m) {
                    if same(&s.line, line) && claimed.insert(*id) {
                        stack.push(TripRef::of(&raw[id.0].trips[id.1]));
                    }
                }
            }
            stack.sort_by_key(|r| r.dep);
            // the rest as the day's timetable has it: on its tour's bus
            for (ti, u) in raw.iter().enumerate().filter(|(_, u)| same(&u.line, line)) {
                let rest: Vec<usize> = (0..u.trips.len()).filter(|&k| !claimed.contains(&(ti, k)) && !u.trips[k].origin.trim().is_empty()).collect();
                if !rest.iter().any(|&k| u.trips[k].counts()) {
                    continue;
                }
                let name = if names.iter().any(|n| same(n, &u.tour)) { fresh_name(&names) } else { u.tour.trim().to_string() };
                names.push(name.clone());
                made.missing.extend(rest.iter().filter(|&&k| u.trips[k].counts()).map(|&k| missed(u, &u.trips[k], &name)));
                blocks.push(BlockLayout { tour: name, trips: rest.iter().map(|&k| TripRef::of(&u.trips[k])).collect(), cuts: None, reliefs: Vec::new() });
            }
            layouts.push(LineLayout { weekday: to_slot, line: line.clone(), blocks, stack });
        }
        // the locks of the buses copied, the links of trips still on one bus together
        let sheet = c.planning.flow.sheet.clone();
        let locks: Vec<BlockLock> = sheet.locks.iter().filter(|l| l.weekday == from_slot && copied.iter().any(|(ln, b)| same(ln, &l.line) && same(b, &l.tour))).map(|l| BlockLock { weekday: to_slot, ..l.clone() }).collect();
        let mut links: Vec<Link> = Vec::new();
        for l in sheet.links.iter().filter(|l| l.weekday == from_slot) {
            let trips: Vec<TripRef> = l.trips.iter().filter_map(|r| srcs.iter().zip(&found).find(|(s, _)| same(&s.line, &l.line) && r.is(&s.x)).and_then(|(_, m)| dst_ref(*m))).collect();
            let together = layouts.iter().filter(|x| same(&x.line, &l.line)).flat_map(|x| x.blocks.iter()).any(|b| trips.iter().all(|r| b.trips.contains(r)));
            if trips.len() >= 2 && together {
                links.push(Link { weekday: to_slot, line: l.line.clone(), trips });
            }
        }
        let p = &mut c.planning;
        p.layouts.retain(|l| l.weekday != to_slot);
        p.layouts.extend(layouts);
        p.flow.sheet.locks.retain(|l| l.weekday != to_slot);
        p.flow.sheet.locks.extend(locks);
        p.flow.sheet.links.retain(|l| l.weekday != to_slot);
        p.flow.sheet.links.extend(links);
        if w.drivers {
            let dismissed: Vec<(u8, String)> = sheet.dismissed.iter().filter(|x| x.0 == from_slot).map(|x| (to_slot, x.1.clone())).collect();
            p.flow.sheet.dismissed.retain(|x| x.0 != to_slot);
            p.flow.sheet.dismissed.extend(dismissed);
        }
    }
    // each of the day's trips that matched: its source duty and piece
    let duties = st::duties(&c.planning.joined, from_slot, &from);
    let mut label: HashMap<TripRef, (usize, (usize, usize))> = HashMap::new();
    for (s, m) in srcs.iter().zip(&found) {
        let (Some((bi, k)), Some(r)) = (s.at, dst_ref(*m)) else { continue };
        let Some(pc) = network::duties_of(&from[bi]).iter().position(|x| x.contains(&k)) else { continue };
        if let Some(d) = st::duty_of(&duties, bi, pc) {
            label.insert(r, (d, (bi, pc)));
        }
    }
    let lab = |x: &PlannedTrip| if x.origin.trim().is_empty() { None } else { label.get(&TripRef::of(x)).copied() };
    if w.drivers {
        // the cuts: a driver takes over where the source's piece of work changes
        let tours = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
        for line in &lines {
            let laid_dst = c.planning.layouts.iter().any(|l| l.weekday == to_slot && same(&l.line, line));
            // (automatic on both days: cut alike)
            if !src_laid(line) && !laid_dst {
                continue;
            }
            let mine: Vec<&TourOfDay> = tours.iter().filter(|t| same(&t.line, line)).collect();
            if mine.is_empty() {
                continue;
            }
            let mut lay = c.planning.layouts.iter().find(|l| l.weekday == to_slot && same(&l.line, line)).cloned().unwrap_or_else(|| layout_of(to_slot, line, &mine));
            for t in &mine {
                let mut cuts: Vec<TripRef> = Vec::new();
                let mut piece: Option<(usize, usize)> = None;
                let mut buses: Vec<usize> = Vec::new();
                for x in &t.trips {
                    let Some((_, pc)) = lab(x) else { continue };
                    if piece.is_some_and(|p| p != pc) {
                        cuts.push(TripRef::of(x));
                    }
                    piece = Some(pc);
                    if !buses.contains(&pc.0) {
                        buses.push(pc.0);
                    }
                }
                if piece.is_none() {
                    continue;
                }
                // (the trips of one bus of the source, cut automatically there: here too)
                let auto = buses.len() == 1 && from[buses[0]].cuts.is_none();
                if let Some(b) = lay.blocks.iter_mut().find(|b| same(&b.tour, &t.tour)) {
                    b.cuts = if auto { None } else { Some(cuts) };
                }
            }
            c.planning.layouts.retain(|l| !(l.weekday == to_slot && same(&l.line, line)));
            c.planning.layouts.push(lay);
        }
        // the duties: the day's pieces of each source duty joined, its note on the first
        let tours = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
        let mut groups: Vec<(usize, Vec<st::PieceRef>)> = Vec::new();
        for (i, t) in tours.iter().enumerate() {
            for (k, r) in network::duties_of(t).iter().enumerate() {
                let Some((d, _)) = t.trips[r.clone()].iter().find_map(|x| lab(x)) else { continue };
                let Some(p) = st::piece_ref(&tours, i, k) else { continue };
                match groups.iter_mut().find(|g| g.0 == d) {
                    Some(g) => g.1.push(p),
                    None => groups.push((d, vec![p])),
                }
            }
        }
        for g in groups.iter_mut() {
            g.1.sort_by_key(|p| p.first.dep);
        }
        let joined: Vec<JoinedDuty> = groups.iter().filter(|g| g.1.len() > 1).map(|g| JoinedDuty { weekday: to_slot, pieces: g.1.clone() }).collect();
        let sheet = &c.planning.flow.sheet;
        let mut notes: Vec<DutyNote> = Vec::new();
        for n in sheet.notes.iter().filter(|n| n.weekday == from_slot) {
            let Some(d) = (0..duties.len()).find(|&d| rf::duty_key(&from, &duties[d]).is_some_and(|k| k.is(&n.key))) else { continue };
            if let Some(g) = groups.iter().find(|g| g.0 == d) {
                notes.push(DutyNote { weekday: to_slot, key: g.1[0].clone(), ..n.clone() });
            }
        }
        let spares: Vec<Spare> = sheet.spares.iter().filter(|s| s.weekday == from_slot).map(|s| Spare { weekday: to_slot, ..s.clone() }).collect();
        let p = &mut c.planning;
        p.joined.retain(|j| j.weekday != to_slot);
        p.joined.extend(joined);
        p.flow.sheet.notes.retain(|n| n.weekday != to_slot);
        p.flow.sheet.notes.extend(notes);
        p.flow.sheet.spares.retain(|s| s.weekday != to_slot);
        p.flow.sheet.spares.extend(spares);
        if !w.vehicles {
            // (the day's own buses: its trips no source duty has)
            for t in &tours {
                for x in t.trips.iter().filter(|x| real(x) && lab(x).is_none()) {
                    made.missing.push(missed(t, x, &t.tour));
                }
            }
        }
    }
    // what of the day's duties no longer finds its pieces goes
    let tours = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
    c.planning.joined.retain(|j| j.weekday != to_slot || j.pieces.iter().all(|p| st::piece_at(&tours, p).is_some()));
    c.planning.flow.sheet.notes.retain(|n| n.weekday != to_slot || st::piece_at(&tours, &n.key).is_some());
    made.missing.sort_by(|a, b| (&a.line, a.dep, &a.block).cmp(&(&b.line, b.dep, &b.block)));
    made.matched = made.total.saturating_sub(made.missing.len() as u32);
    made
}

/// Remember that `day` was copied from `pattern` (`copy` made `made`).
fn remember(c: &mut Company, day: u8, pattern: u8, w: What, made: &Copied) {
    let f = Follow { day, pattern, vehicles: w.vehicles, drivers: w.drivers, print: print(c, day, w), source: print(c, pattern, w), matched: made.matched, total: made.total, missing: made.missing.clone() };
    let fs = &mut c.planning.flow.days.follows;
    fs.retain(|x| x.day != day);
    fs.push(f);
}

/// The plan `src` runs copied onto `dst` (`copy`) - as its weekday's plan, every week
/// (`every_week`; the date's own plan, if it had one, goes), or as a plan of that date only -
/// and remembered as following it. The copy is made on the saved plan.
pub fn apply(c: &mut Company, src: Day, dst: Day, every_week: bool, w: What) -> Copied {
    let from_slot = slot_of(c, src.date);
    let to_slot = if every_week {
        c.planning.flow.days.dates.retain(|d| d.date != dst.date.trim());
        gc(c);
        plan::weekday_of(dst.date)
    } else {
        own_slot(c, dst.date)
    };
    if to_slot == from_slot {
        return Copied { slot: to_slot, ..Default::default() };
    }
    let made = copy(c, from_slot, src, to_slot, dst, w);
    remember(c, to_slot, from_slot, w, &made);
    made
}

/// "Reset to pattern", "Refresh": the plan of `slot` copied again from the pattern it follows
/// (`src` a day that runs the pattern, `dst` a day that runs `slot`). None: it follows none.
pub fn recopy(c: &mut Company, slot: u8, src: Day, dst: Day) -> Option<Copied> {
    let f = follow_of(c, slot).filter(|f| f.print != 0)?.clone();
    let w = What { vehicles: f.vehicles, drivers: f.drivers };
    let made = copy(c, f.pattern, src, slot, dst, w);
    remember(c, slot, f.pattern, w, &made);
    Some(made)
}

/// A day "follows" no pattern any more (it keeps its plan as it is).
pub fn unfollow(c: &mut Company, slot: u8) -> bool {
    let fs = &mut c.planning.flow.days.follows;
    let before = fs.len();
    fs.retain(|f| f.day != slot);
    before != fs.len()
}

// --- what each day is ---------------------------------------------------------------------------

/// What the plan of a slot is: a weekday's or one of single dates, what it follows and whether
/// it still is as copied (`own`: changed since, an override) and its pattern as it was
/// (`unsynced`: changed since), the slots that follow it, and the trips its copy left to plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub slot: u8,
    pub date_only: bool,
    pub pattern: Option<u8>,
    /// A plan of the date's own from the start (`fork_draft`): Reset takes it back.
    pub forked: bool,
    pub own: bool,
    pub unsynced: bool,
    pub followers: Vec<u8>,
    pub matched: u32,
    pub total: u32,
    pub missing: Vec<Missed>,
}

impl State {
    /// A copy as it was made, its pattern too.
    pub fn is_in_step(&self) -> bool {
        self.pattern.is_some() && !self.own && !self.unsynced
    }
}

/// What the plan of `slot` is (on `c` as the studio shows it: its draft in).
pub fn state(c: &Company, slot: u8) -> State {
    let days = &c.planning.flow.days;
    let mut s = State { slot, date_only: slot >= FIRST_DATE_SLOT, followers: days.follows.iter().filter(|f| f.pattern == slot && f.day != slot).map(|f| f.day).collect(), ..Default::default() };
    s.followers.sort_unstable();
    if let Some(f) = days.follows.iter().find(|f| f.day == slot) {
        let w = What { vehicles: f.vehicles, drivers: f.drivers };
        s.pattern = Some(f.pattern);
        s.forked = f.print == 0;
        s.own = s.forked || print(c, slot, w) != f.print;
        s.unsynced = !s.forked && print(c, f.pattern, w) != f.source;
        (s.matched, s.total, s.missing) = (f.matched, f.total, f.missing.clone());
    }
    s
}

/// The company's trips with passengers of a day by their line, tour, name and departure: two
/// days with the same are days of the same timetable.
pub fn timetable_key(c: &Company, lines: &[LineInfo]) -> u64 {
    let mut keys: Vec<String> = network::timetable_tours(c, lines).iter().flat_map(|t| t.trips.iter().filter(|x| x.counts()).map(move |x| format!("{}|{}|{}|{}", t.line.to_lowercase(), x.origin.trim(), x.name, x.dep))).collect();
    keys.sort();
    fnv(&keys.join("\n"))
}

/// A day's rows (buses, duties) copied to other days by themselves: RosterFlow's "Copy to…".
mod rows;
pub use rows::{copy_rows, rows_slot, Rows};

/// A line's timetable of a day copied to other days, and the templates: the Timeplan's "Copy
/// to…".
mod timetable;
pub use timetable::{clear_day, copy_table, drop_template, move_templates, rename_template, save_template, template, templates_of, DayCopied, How, Template, Trip};

#[cfg(test)]
mod tests;

//! Service control: the company's controller on its day (Luc, 2026-10-15: "if vehicles are
//! running late, the player should have an option in the operations tab to adjust the vehicle on
//! the route to get it back on time - to reduce penalties and maintain the reputation. This can
//! also work for the breakdowns and driver lateness").
//!
//! As the company's clock runs its day, each bus's trips run late by what holds them up on the way
//! (drawn per trip: the driver's punctuality, the bus's condition, what happens on the line, the
//! rush hours), and a delay is carried along the bus's block: the next trip leaves late by what the
//! layover before it could not take up (`chain`). A trip's own hold-ups are known once it has
//! begun; the trips ahead carry what is known.
//!
//! The controller's ways to bring a late bus back on time, as real controllers use them:
//! - a short-turn: the trip turns back early, its last stops and the next trip's first are skipped
//!   (lost passengers, a part of a trip's penalty: `part_penalty`), the next trip leaves on time;
//! - running out of service to the terminus: the rest of the trip without stopping, quicker;
//! - a shorter layover before the next trip, where the plan has one;
//! - a spare bus swapped in at the terminus for the next trip: it runs the rest of the block on
//!   time, the late bus goes back to the depot and is the spare (`busy`);
//! - for a late driver, someone covering his first trips (a free colleague, an agency driver booked
//!   for the day, or one the agency sends now: `plan::late_covers`);
//! - for a breakdown, the clock's own choices (`clock::choose`: a spare bus, a rental bus, dropping).
//!
//! What is done is kept with the day (`Control` in `DayScript::controls`) and the day's close
//! counts it: `close_delays` gives each trip of a tour its delay, the share of it cut short and the
//! bus swapped in (`day::close_day_with`). Casual's controller does the best by himself and says
//! so; Normal suggests the best; Realistic leaves the choice to the player (`watch`).

use super::clock::{self as ck, Cut, DayScript, Level, Run, World};
use super::dates;
use super::day::TourPlan;
use super::economy::{self, Rules};
use super::inbox::{Kind, Message};
use super::model::{Cents, Company, CompanyLine, Employee, GameMode, Vehicle};
use super::network::{self, PlannedTrip, TourOfDay};
use super::plan::{self, BusOf, DayPlan, DayTour, Disruption, Fill, Who};
use super::rng::Rng;
use crate::service::ServiceKind;
use serde::{Deserialize, Serialize};

/// A trip more than this many minutes behind is late (as the day's close counts a trip the game
/// ran: 180 s).
pub const LATE_AT: i32 = 3;
/// The rest a driver takes at the terminus: what is left of a layover after it takes up a delay.
pub const MIN_LAYOVER: i32 = 3;
/// A bus more than this many minutes late is told, unless the settings say otherwise
/// (`Clock::late_notice`).
pub const NOTICE_MINUTES: u32 = 5;
/// How often (company minutes) the clock looks at the day (`watch_day`).
pub const WATCH_EVERY: i32 = 5;
/// Minutes until an agency driver sent now is there.
pub const AGENCY_MINUTES: i32 = 45;
/// What a reputation point lost is worth when the controller weighs his ways (cents).
const REP_VALUE: f64 = 100_00.0;
/// The reputation a late trip costs, and a trip dropped (`day`: 0.25 a dropped trip).
pub const REP_LATE: f64 = 0.03;
pub const REP_DROP: f64 = 0.25;
/// A part of a trip cut short costs this share of a dropped trip's penalty for the part lost.
const PART_SHARE: f64 = 0.5;
/// A short-turn skips at least and at most this share of the trip and of the next.
const SHORT_TURN_MIN: f64 = 0.1;
const SHORT_TURN_MAX: f64 = 0.5;
/// Running out of service saves this share of the rest of a trip's running time (the stops).
const EXPRESS_SAVES: f64 = 0.3;
/// The kilometres a spare bus swapped in runs to the terminus and the late one back.
const SWAP_KM: f64 = 12.0;

// --- what is kept ----------------------------------------------------------------------------------

/// What the controller did for a tour today.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Control {
    /// The tour: its line (timetable name) and its tour.
    pub line: String,
    pub tour: String,
    /// The trip it was done at (its departure).
    pub dep: i32,
    /// When it was decided (minute of the day).
    pub at: i32,
    pub act: Act,
    /// Casual's controller did it by himself.
    #[serde(default)]
    pub auto: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Act {
    /// The trip turns back early: its last `part` and the next trip's first `part` are not run,
    /// which saves `saved` minutes.
    ShortTurn { part: f64, saved: i32 },
    /// The rest of the trip (`part` of it) runs out of service to the terminus, `saved` minutes
    /// quicker.
    Express { part: f64, saved: i32 },
    /// The layover before the trip is cut by `saved` minutes.
    Layover { saved: i32 },
    /// `vehicle` runs the tour's trips from this one on; `from_bus` goes back to the depot.
    Swap { vehicle: u32, from_bus: u32 },
    /// Someone covers the first trips of duty `duty` (its driver is late) from minute `from`
    /// (who: the planning's choice, `plan::late_key`).
    Cover { duty: usize, from: i32 },
}

impl Act {
    pub fn key(&self) -> &'static str {
        match self {
            Act::ShortTurn { .. } => "short_turn",
            Act::Express { .. } => "express",
            Act::Layover { .. } => "layover",
            Act::Swap { .. } => "swap",
            Act::Cover { .. } => "cover",
        }
    }
}

/// What service control last saw of the company's day (`watch`): for the Operations page's badge
/// and the notices.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Watch {
    /// The minute it last looked.
    #[serde(default)]
    pub at: Option<i32>,
    /// The buses running late then: their tour (`plan::tour_key`) and minutes.
    #[serde(default)]
    pub late: Vec<(String, i32)>,
    /// The late drivers nobody covers then.
    #[serde(default)]
    pub drivers: usize,
    /// The tours told running late today (once a day each).
    #[serde(default)]
    pub told: Vec<String>,
}

/// The minutes a bus has to be late to be told (0: never).
pub fn notice_minutes(c: &Company) -> u32 {
    c.clock.late_notice.unwrap_or(NOTICE_MINUTES)
}

/// How many things service control has for the player now (the Operations page's badge): the
/// buses more than the notice's minutes late, the late drivers nobody covers, the breakdowns not
/// decided.
pub fn waiting(c: &Company) -> usize {
    let Some(s) = c.clock.today.as_ref().filter(|s| s.date == c.date) else { return 0 };
    let n = notice_minutes(c).max(LATE_AT as u32 + 1) as i32;
    s.watch.late.iter().filter(|x| x.1 >= n).count() + s.watch.drivers + s.breaks.iter().filter(|b| b.choice.is_none()).count()
}

fn script<'a>(c: &'a Company, date: &str) -> Option<&'a DayScript> {
    c.clock.today.as_ref().filter(|s| s.date == date)
}

/// The fleet's buses out on their tours today as the controller changed them (`DayScript::busy`):
/// a bus swapped out is free from the swap on (a spare), the one swapped in busy until the tour
/// ends.
pub fn busy(s: &DayScript) -> Vec<(u32, i32, i32)> {
    let mut out = s.busy.clone();
    for x in &s.controls {
        let Act::Swap { vehicle, from_bus } = x.act else { continue };
        let mut add = Vec::new();
        for b in out.iter_mut().filter(|b| b.0 == from_bus && b.1 <= x.dep && x.dep <= b.2) {
            add.push((vehicle, x.dep, b.2));
            b.2 = x.dep;
        }
        out.extend(add);
    }
    out
}

/// From when someone sent during the day covers the first trips of duty `duty` of a tour on
/// `date` (`Act::Cover`; None: not sent so, all of them).
pub fn cover_from(c: &Company, date: &str, line: &str, tour: &str, duty: usize) -> Option<i32> {
    script(c, date)?.controls.iter().rev().find_map(|x| match x.act {
        Act::Cover { duty: d, from } if d == duty && same(&x.line, line) && same(&x.tour, tour) => Some(from),
        _ => None,
    })
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn controls_of<'a>(s: Option<&'a DayScript>, t: &TourOfDay) -> Vec<&'a Control> {
    s.map(|s| s.controls.iter().filter(|x| same(&x.line, &t.line) && same(&x.tour, &t.tour)).collect()).unwrap_or_default()
}

// --- the delays --------------------------------------------------------------------------------

/// A trip of a block as service control sees it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TripDelay {
    /// It runs (a bus and a driver, not lost to a breakdown or a late driver).
    pub runs: bool,
    /// Minutes late leaving, and at its end.
    pub start: i32,
    pub late: i32,
    /// It has begun: its own hold-ups are known.
    pub known: bool,
    /// The share of it not run in service (a short-turn, out of service).
    pub lost: f64,
    /// A bus swapped in runs it.
    pub bus: Option<u32>,
    /// The minutes the controller took off its delay.
    pub recovered: i32,
}

/// A block's trips with who runs them: the bus, each trip's driver, whether it runs.
pub struct Block<'a> {
    pub tour: &'a TourOfDay,
    pub bus: Option<u32>,
    pub drivers: Vec<Option<u32>>,
    pub runs: Vec<bool>,
}

/// What holds trip `trip` up on its own (minutes, drawn: the same day draws the same): the
/// driver's punctuality, the bus's condition, the line's events, the rush hours.
fn own_delay(c: &Company, day: i64, t: &TourOfDay, trip: &PlannedTrip, driver: Option<&Employee>, bus: Option<&Vehicle>, factor: f64, effect: f64) -> i32 {
    if !trip.counts() {
        return 0;
    }
    let punctual = driver.map_or(60.0, |e| e.skills.punctuality);
    let condition = bus.map_or(80.0, |v| v.condition);
    let rush = if matches!(trip.dep.rem_euclid(1440) / 60, 7 | 8 | 16 | 17) { 1.4 } else { 1.0 };
    let p = ((0.03 + 0.09 * (1.0 - punctual / 100.0) + 0.07 * (1.0 - condition / 100.0)) * factor + effect) * rush;
    let mut rng = Rng::of(&[&c.id, "delay", &t.line, &t.tour, &trip.dep.to_string()], day);
    if rng.chance(p) {
        rng.int(2, 12) as i32
    } else {
        0
    }
}

/// The delays of a block's trips: each leaves late by what the trip before arrived late less what
/// the layover took up (`MIN_LAYOVER` of it kept), and adds its own hold-ups - known for the trips
/// begun by `now` (None: all, the day's close) -, with the controller's `controls` (and `extra`,
/// to see what it would do).
pub fn chain(c: &Company, date: &str, b: &Block, now: Option<i32>, controls: &[&Control], extra: Option<&Control>) -> Vec<TripDelay> {
    let day = dates::parse(date).unwrap_or(0);
    let factor = super::research::late_factor(c) * c.game_mode.rules().lateness;
    let effect = super::inbox::line_effect(c, date, &b.tour.number).0;
    let agency = plan::agency_driver();
    let own = |j: usize, bus: Option<u32>| {
        let driver = b.drivers.get(j).copied().flatten().and_then(|id| if id == plan::AGENCY { Some(&agency) } else { c.employee(id) });
        own_delay(c, day, b.tour, &b.tour.trips[j], driver, bus.and_then(|id| c.vehicle(id)), factor, effect)
    };
    chain_with(b, now, controls, extra, &own)
}

/// `chain` with each trip's own hold-ups from `own` (the trip's index, the bus that runs it).
fn chain_with(b: &Block, now: Option<i32>, controls: &[&Control], extra: Option<&Control>, own: &dyn Fn(usize, Option<u32>) -> i32) -> Vec<TripDelay> {
    let all: Vec<&Control> = controls.iter().copied().chain(extra).collect();
    let mut out = Vec::with_capacity(b.tour.trips.len());
    let mut prev: Option<(i32, i32, i32)> = None;
    let mut bus = b.bus;
    for (j, trip) in b.tour.trips.iter().enumerate() {
        let mut t = TripDelay { runs: b.runs.get(j).copied().unwrap_or(false), ..Default::default() };
        let mut reset = false;
        for x in all.iter().filter(|x| x.dep == trip.dep) {
            if let Act::Swap { vehicle, .. } = x.act {
                bus = Some(vehicle);
                reset = true;
            }
        }
        t.bus = bus.filter(|_| bus != b.bus);
        if !t.runs {
            // (a trip not run: the bus waits, the next one leaves on time)
            prev = None;
            out.push(t);
            continue;
        }
        let mut start = prev.map_or(0, |(_, arr, late)| (late - (trip.dep - arr - MIN_LAYOVER).max(0)).max(0));
        for x in &all {
            match x.act {
                Act::Layover { saved } if x.dep == trip.dep => {
                    let r = saved.min(start);
                    start -= r;
                    t.recovered += r;
                }
                Act::ShortTurn { part, .. } if x.dep == trip.dep || prev.is_some_and(|p| p.0 == x.dep) => t.lost = t.lost.max(part),
                Act::Express { part, .. } if x.dep == trip.dep => t.lost = t.lost.max(part),
                _ => {}
            }
        }
        if reset {
            t.recovered += start;
            start = 0;
        }
        t.start = start;
        t.known = now.is_none_or(|m| trip.dep + start <= m);
        let mut late = start + if t.known { own(j, bus) } else { 0 };
        for x in all.iter().filter(|x| x.dep == trip.dep) {
            if let Act::ShortTurn { saved, .. } | Act::Express { saved, .. } = x.act {
                let r = saved.min(late);
                late -= r;
                t.recovered += r;
            }
        }
        t.late = late;
        prev = Some((trip.dep, trip.arr, late));
        out.push(t);
    }
    out
}

/// The trips of a tour of the company's day as they run now: the bus, the drivers, those lost to a
/// late driver or a breakdown (the bus sent for it runs them again).
fn day_block<'a>(c: &Company, s: Option<&DayScript>, t: &'a DayTour) -> Block<'a> {
    let bus = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    let cut: Option<Cut> = bus.and_then(|id| s?.breaks.iter().find(|b| b.vehicle == id)).map(|b| b.cut());
    let n = t.tour.trips.len();
    let mut drivers = vec![None; n];
    let mut runs = vec![false; n];
    for d in &t.duties {
        let who = match d.who {
            Some(Who::Staff(id)) => Some(id),
            Some(Who::Agency) => Some(plan::AGENCY),
            // (the player's and his friends' trips are the game's: measured, not the controller's)
            _ => None,
        };
        let dropped = d.dropped_before();
        for j in d.start..d.end.min(n) {
            let dep = t.tour.trips[j].dep;
            drivers[j] = who;
            runs[j] = t.bus.is_some() && who.is_some() && dropped.is_none_or(|x| dep >= x) && !cut.is_some_and(|x| x.lost(dep));
        }
    }
    let _ = c;
    Block { tour: &t.tour, bus, drivers, runs }
}

/// The delays of a tour as the day's close counts them (`day::close_day_with`): the trips of `tp`
/// with the clock's draws and the controller's doing, `cut` its breakdown. Empty when the
/// company's clock did not run the day (the close draws its own then).
pub fn close_delays(c: &Company, date: &str, tp: &TourPlan, cut: Option<Cut>) -> Vec<TripDelay> {
    let Some(s) = script(c, date) else { return Vec::new() };
    if tp.by_player || tp.live {
        return Vec::new();
    }
    let n = tp.tour.trips.len();
    let mut drivers = vec![None; n];
    let mut runs = vec![false; n];
    for d in &tp.duties {
        let who = d.driver.filter(|&id| id != plan::PLAYER);
        for j in d.start..d.end.min(n) {
            let dep = tp.tour.trips[j].dep;
            drivers[j] = who;
            runs[j] = tp.bus.is_some() && who.is_some() && d.dropped_before.is_none_or(|x| dep >= x) && !cut.is_some_and(|x| x.lost(dep));
        }
    }
    let b = Block { tour: &tp.tour, bus: tp.bus, drivers, runs };
    chain(c, date, &b, None, &controls_of(Some(s), &tp.tour), None)
}

/// What the part of a trip cut short costs in the contract's penalty.
pub fn part_penalty(r: &Rules, trip: &PlannedTrip, lost: f64) -> Cents {
    if lost <= 0.0 || !trip.counts() {
        return 0;
    }
    ((r.drop_per_trip as f64 + trip.km * r.drop_per_km as f64) * PART_SHARE * lost.min(1.0)).round() as Cents
}

// --- what it costs ------------------------------------------------------------------------------

/// What the rest of a bus's day (or a late driver's first trips) comes to.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Effect {
    /// Trips ahead that run late, and trips dropped.
    pub late_trips: u32,
    pub lost_trips: u32,
    /// The contract's penalties for them.
    pub penalty: Cents,
    /// The shares of trips cut short, and what they cost (a part of the penalty, the fares lost).
    pub parts: f64,
    pub part_trips: u32,
    pub parts_cost: Cents,
    /// What else it costs (a spare bus's run, an agency driver).
    pub cost: Cents,
    /// The reputation it costs.
    pub rep: f64,
    /// The first trip ahead on time again.
    pub on_time_from: Option<i32>,
}

impl Effect {
    pub fn money(&self) -> Cents {
        self.penalty + self.parts_cost + self.cost
    }

    fn score(&self) -> f64 {
        self.money() as f64 + self.rep * REP_VALUE
    }
}

fn line_of<'a>(c: &'a Company, t: &TourOfDay) -> Option<&'a CompanyLine> {
    c.lines.iter().find(|l| same(&l.name, &t.line))
}

/// What a dropped trip costs in the contract's penalty.
fn drop_cost(c: &Company, r: &Rules, t: &TourOfDay, trip: &PlannedTrip) -> Cents {
    let strict = if line_of(c, t).is_some_and(|l| super::ownline::kind_of(l) == ServiceKind::School) { super::ownline::SCHOOL_DROP } else { 1 };
    (r.drop_per_trip + (trip.km * r.drop_per_km as f64).round() as Cents) * strict
}

/// The rest of a block's day from trip `cur` on: its trips not begun yet that run late, the parts
/// cut short.
fn effect(c: &Company, t: &TourOfDay, d: &[TripDelay], cur: usize) -> Effect {
    let r = economy::rules(c.game_mode);
    let line = line_of(c, t);
    let strict = if line.is_some_and(|l| super::ownline::kind_of(l) == ServiceKind::School) { super::ownline::SCHOOL_LATE } else { 1 };
    let fare = line.map_or(0.0, |l| super::fares::per_passenger(c, l));
    let mut e = Effect::default();
    for (trip, x) in t.trips.iter().zip(d).skip(cur) {
        if !x.runs || !trip.counts() {
            continue;
        }
        if x.lost > 0.0 {
            e.parts += x.lost;
            e.part_trips += 1;
            e.parts_cost += part_penalty(&r, trip, x.lost) + (x.lost * economy::passengers_for(trip.km, trip.dep, &r) * fare).round() as Cents;
        }
        if x.known {
            continue;
        }
        if x.late > LATE_AT {
            e.late_trips += 1;
            e.penalty += r.late_per_trip * strict;
        } else if e.on_time_from.is_none() {
            e.on_time_from = Some(trip.dep);
        }
    }
    e.rep = e.late_trips as f64 * REP_LATE + e.parts * REP_DROP;
    e
}

// --- the live day ---------------------------------------------------------------------------------

/// A way to bring a late bus back on time, with what it comes to.
#[derive(Clone, Debug, PartialEq)]
pub struct Opt {
    /// The trip it is done at.
    pub dep: i32,
    pub act: Act,
    pub effect: Effect,
    /// What it saves against doing nothing (cents, a reputation point at `REP_VALUE`).
    pub gain: f64,
    /// The controller's suggestion (not on Realistic).
    pub best: bool,
}

/// A bus running late now.
#[derive(Clone, Debug, PartialEq)]
pub struct LateBus {
    /// The tour (index into the day plan's tours).
    pub ti: usize,
    pub vehicle: Option<u32>,
    pub minutes: i32,
    /// The trip it is on (or waits for, at the terminus: `running` false).
    pub trip: usize,
    pub running: bool,
    /// Left as it is.
    pub nothing: Effect,
    pub options: Vec<Opt>,
}

/// A way to cover a late driver's first trips.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverCover {
    /// `Fill::Colleague` (a colleague or a booked agency driver) or `Fill::Agency` (sent now).
    pub fill: Fill,
    pub booked: bool,
    pub standby: bool,
    /// From when they drive.
    pub from: i32,
    pub effect: Effect,
    pub best: bool,
}

/// A driver late for his first duty, nobody covering its first trips.
#[derive(Clone, Debug, PartialEq)]
pub struct LateDriver {
    pub ti: usize,
    pub duty: usize,
    pub employee: u32,
    pub minutes: i32,
    /// When he comes.
    pub until: i32,
    pub nothing: Effect,
    pub covers: Vec<DriverCover>,
}

/// Service control's view of the company's day now.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Live {
    pub now: i32,
    pub buses: Vec<LateBus>,
    pub drivers: Vec<LateDriver>,
}

/// The block's index of the trip a bus is on now (or the last it ran), and whether it is still on it.
fn current(t: &TourOfDay, d: &[TripDelay], now: i32) -> Option<(usize, bool)> {
    let j = (0..d.len()).rev().find(|&j| d[j].runs && d[j].known)?;
    Some((j, now < t.trips[j].arr + d[j].late))
}

fn next_run(d: &[TripDelay], after: usize) -> Option<usize> {
    (after + 1..d.len()).find(|&j| d[j].runs)
}

/// The day of `dp` (the company's own) as service control sees it now: the buses running late with
/// the ways to bring them back on time, the late drivers with who could cover them.
pub fn live(c: &Company, dp: &DayPlan) -> Live {
    let Some(s) = script(c, &dp.date).filter(|_| dp.today) else { return Live::default() };
    let now = c.clock.minute as i32;
    let mut out = Live { now, ..Default::default() };
    let realistic = c.game_mode == GameMode::Realistic;
    for (ti, t) in dp.tours.iter().enumerate() {
        if t.by_player || t.live || t.bus.is_none() {
            continue;
        }
        let b = day_block(c, Some(s), t);
        let mine = controls_of(Some(s), &t.tour);
        let base = chain(c, &dp.date, &b, Some(now), &mine, None);
        let Some((cur, running)) = current(&t.tour, &base, now) else { continue };
        let next = next_run(&base, cur);
        let minutes = if running { base[cur].late } else { next.map_or(0, |n| base[n].start) };
        if minutes <= LATE_AT || (!running && next.is_none()) {
            continue;
        }
        let nothing = effect(c, &t.tour, &base, cur);
        let mut options = Vec::new();
        let mut consider = |dep: i32, act: Act| {
            if mine.iter().any(|x| x.dep == dep && x.act.key() == act.key()) {
                return;
            }
            let x = Control { line: t.tour.line.clone(), tour: t.tour.tour.clone(), dep, at: now, act, auto: false };
            let d = chain(c, &dp.date, &b, Some(now), &mine, Some(&x));
            let mut e = effect(c, &t.tour, &d, cur);
            if let Act::Swap { vehicle, .. } = act {
                e.cost += swap_cost(c, vehicle);
            }
            let gain = nothing.score() - e.score();
            options.push(Opt { dep, act, effect: e, gain, best: false });
        };
        // (where it is done: the trip it is on, or the next when it waits at the terminus)
        let a = if running { cur } else { next.unwrap_or(cur) };
        let ta = &t.tour.trips[a];
        let dur_a = (ta.arr - ta.dep).max(1);
        let rem = if running && a == cur { ((ta.arr + base[a].late - now) as f64 / dur_a as f64).clamp(0.0, 1.0) } else { 1.0 };
        if let Some(n) = next_run(&base, a) {
            let tn = &t.tour.trips[n];
            let need = base[n].start;
            if need > LATE_AT {
                let dur_n = (tn.arr - tn.dep).max(1);
                // a short-turn: the end of this trip and the start of the next skipped
                if ta.counts() && tn.counts() && rem >= SHORT_TURN_MIN {
                    let part = (need as f64 / (dur_a + dur_n) as f64).clamp(SHORT_TURN_MIN, SHORT_TURN_MAX.min(rem));
                    consider(ta.dep, Act::ShortTurn { part, saved: (part * (dur_a + dur_n) as f64).round() as i32 });
                }
                // out of service to the terminus
                if ta.counts() && rem >= 0.15 {
                    let saved = (EXPRESS_SAVES * rem * dur_a as f64).round().max(1.0) as i32;
                    consider(ta.dep, Act::Express { part: rem, saved });
                }
                // a shorter layover
                let gap = tn.dep - ta.arr;
                if gap >= 1 {
                    consider(tn.dep, Act::Layover { saved: MIN_LAYOVER.min(gap) });
                }
            }
        }
        // a spare bus at the terminus, for the first trip it can be there for
        if let Some(late_bus) = b.bus {
            let sw = (cur + 1..base.len()).find(|&j| base[j].runs && !base[j].known && t.tour.trips[j].dep >= now + ck::SPARE_MINUTES);
            if let Some(j) = sw.filter(|&j| base[j].start > LATE_AT) {
                let end = t.tour.to();
                if let Some(sp) = ck::free_buses(c, s, late_bus, &t.tour.line, now, end, None).into_iter().find(|x| x.until.is_none()) {
                    consider(t.tour.trips[j].dep, Act::Swap { vehicle: sp.vehicle, from_bus: late_bus });
                }
            }
        }
        if !realistic {
            if let Some(o) = options.iter_mut().filter(|o| o.gain > 0.0).max_by(|x, y| x.gain.partial_cmp(&y.gain).unwrap_or(std::cmp::Ordering::Equal)) {
                o.best = true;
            }
        }
        out.buses.push(LateBus { ti, vehicle: b.bus, minutes, trip: a, running, nothing, options });
    }
    out.buses.sort_by_key(|x| std::cmp::Reverse(x.minutes));
    // the late drivers nobody covers yet
    let r = economy::rules(c.game_mode);
    let hour = plan::AGENCY_HOUR as f64 * c.price_index;
    for (ti, t) in dp.tours.iter().enumerate() {
        if t.by_player || t.live {
            continue;
        }
        for (k, d) in t.duties.iter().enumerate() {
            let (Some(l), Some(Who::Staff(employee))) = (d.late, d.who) else { continue };
            if l.cover.is_some() || l.until <= now {
                continue;
            }
            let minutes = dp.disruptions.iter().find_map(|x| match *x {
                Disruption::Late { employee: e, minutes } if e == employee => Some(minutes),
                _ => None,
            });
            // (the trips still to come before he is there)
            let lost = |from: i32| -> Effect {
                let mut e = Effect::default();
                for trip in t.tour.trips[d.start..d.end.min(t.tour.trips.len())].iter().filter(|x| x.counts() && x.dep >= now && x.dep < l.until && x.dep < from) {
                    e.lost_trips += 1;
                    e.penalty += drop_cost(c, &r, &t.tour, trip);
                }
                e.rep = e.lost_trips as f64 * REP_DROP;
                e
            };
            let nothing = lost(i32::MAX);
            if nothing.lost_trips == 0 {
                continue;
            }
            let mut covers: Vec<DriverCover> = plan::late_covers(c, dp, ti, k)
                .into_iter()
                .take(4)
                .map(|(id, standby)| DriverCover { fill: Fill::Colleague { id }, booked: super::agency::is_booked(id), standby, from: now, effect: lost(now), best: false })
                .collect();
            let from = now + AGENCY_MINUTES;
            let mut agency = lost(from);
            agency.cost = ((l.until.min(d.to) - from.max(d.from)).max(0) as f64 / 60.0 * hour).round() as Cents;
            covers.push(DriverCover { fill: Fill::Agency, booked: false, standby: false, from, effect: agency, best: false });
            if !realistic {
                if let Some(x) = covers.iter_mut().filter(|x| x.effect.score() < nothing.score()).min_by(|a, b| a.effect.score().partial_cmp(&b.effect.score()).unwrap_or(std::cmp::Ordering::Equal)) {
                    x.best = true;
                }
            }
            out.drivers.push(LateDriver { ti, duty: k, employee, minutes: minutes.unwrap_or(l.until - d.from), until: l.until, nothing, covers });
        }
    }
    out
}

/// What a spare bus swapped in costs to bring out (its kilometres).
fn swap_cost(c: &Company, vehicle: u32) -> Cents {
    let Some(v) = c.vehicle(vehicle) else { return 0 };
    let age = dates::years_between(&v.built, &c.date);
    (SWAP_KM * (economy::energy_per_km(v.kind, c.price_index) + economy::maintenance_per_km(v.kind, age, c.price_index))).round() as Cents
}

// --- doing it ------------------------------------------------------------------------------------

const SHORT_TURNED: &str = "Bus %{bus} on line %{n} turns back early on the %{time} trip";
const EXPRESSED: &str = "Bus %{bus} on line %{n} runs out of service to the terminus on the %{time} trip";
const LAYOVER_CUT: &str = "Bus %{bus} on line %{n} takes a shorter layover before the %{time} trip";
const SWAPPED: &str = "Bus %{spare} takes over from bus %{bus} on line %{n} from the %{time} trip";
const COVERED: &str = "%{name} drives the first trips of line %{n} tour %{tour} from %{time}";
const AGENCY_COVERS: &str = "An agency driver drives the first trips of line %{n} tour %{tour} from %{time}";
const BACK_ON_TIME: &str = "Bus %{bus} on line %{n} is back on time from the %{time} trip";
const BY_CONTROLLER: &str = "The controller stepped in";
pub const LATE_FEED: &str = "Bus %{bus} on line %{n} tour %{tour} runs %{min} minutes late";
pub const LATE_TITLE: &str = "Bus %{bus} runs late";
pub const LATE_TEXT: &str = "Line %{n} tour %{tour}: %{min} minutes behind. Left as it is: %{trips} late trips ahead, %{amount} in penalties. The Operations page has ways to bring it back on time.";

/// The words service control tells (for their translations).
pub fn texts() -> Vec<&'static str> {
    vec![SHORT_TURNED, EXPRESSED, LAYOVER_CUT, SWAPPED, COVERED, AGENCY_COVERS, BACK_ON_TIME, BY_CONTROLLER, LATE_FEED, LATE_TITLE, LATE_TEXT]
}

fn number(c: &Company, id: Option<u32>) -> String {
    id.and_then(|id| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default()
}

/// Do `act` for tour `ti` of the company's day plan `dp` at its trip leaving at `dep` (`auto`:
/// Casual's controller): kept with the day, told in the feed. Returns the feed's line said.
pub fn apply(c: &mut Company, dp: &DayPlan, ti: usize, dep: i32, act: Act, auto: bool, run: &mut Run) -> Option<(String, Vec<(String, String)>)> {
    let t = dp.tours.get(ti)?;
    if !dp.today || script(c, &dp.date).is_none() {
        return None;
    }
    let now = c.clock.minute as i32;
    let bus = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    let x = Control { line: t.tour.line.clone(), tour: t.tour.tour.clone(), dep, at: now, act, auto };
    let at = ck::moment(&c.date, now as i64);
    let time = |m: i32| ck::hhmm(ck::moment(&dp.date, m as i64));
    let (text, args) = match act {
        Act::ShortTurn { .. } => (SHORT_TURNED, vec![ck::arg("bus", number(c, bus)), ck::arg("n", &t.tour.number), ck::arg("time", time(dep))]),
        Act::Express { .. } => (EXPRESSED, vec![ck::arg("bus", number(c, bus)), ck::arg("n", &t.tour.number), ck::arg("time", time(dep))]),
        Act::Layover { .. } => (LAYOVER_CUT, vec![ck::arg("bus", number(c, bus)), ck::arg("n", &t.tour.number), ck::arg("time", time(dep))]),
        Act::Swap { vehicle, .. } => (SWAPPED, vec![ck::arg("spare", number(c, Some(vehicle))), ck::arg("bus", number(c, bus)), ck::arg("n", &t.tour.number), ck::arg("time", time(dep))]),
        Act::Cover { .. } => return None,
    };
    let s = c.clock.today.as_mut()?;
    s.controls.push(x);
    ck::tell(c, run, at, text, args.clone(), Level::Info);
    // (back on time: said, as the trips ahead now run)
    let s = script(c, &dp.date)?;
    let b = day_block(c, Some(s), t);
    let d = chain(c, &dp.date, &b, Some(now), &controls_of(Some(s), &t.tour), None);
    let back = t.tour.trips.iter().zip(&d).find(|(trip, x)| x.runs && !x.known && trip.dep >= dep && x.late <= LATE_AT).map(|(trip, _)| trip.dep);
    if let Some(m) = back {
        let said = vec![ck::arg("bus", number(c, bus)), ck::arg("n", &t.tour.number), ck::arg("time", time(m))];
        ck::tell(c, run, at, BACK_ON_TIME, said, Level::Good);
    }
    if auto {
        run.breaks.push(Message::new(Kind::Lines, BY_CONTROLLER, text).args_of(&args).level(Level::Info).dated(&dp.date));
    }
    Some((text.to_string(), args))
}

/// Have `fill` (a colleague, a booked agency driver, or one the agency sends now) cover the first
/// trips of duty `duty` of tour `ti`, its driver being late, from minute `from`. Returns the feed's
/// line said.
pub fn cover(c: &mut Company, dp: &DayPlan, ti: usize, duty: usize, fill: Fill, from: i32, auto: bool, run: &mut Run) -> Option<(String, Vec<(String, String)>)> {
    let t = dp.tours.get(ti)?;
    t.duties.get(duty)?.late?;
    if !dp.today || script(c, &dp.date).is_none() || !matches!(fill, Fill::Colleague { .. } | Fill::Agency) {
        return None;
    }
    let now = c.clock.minute as i32;
    plan::set_fill(c, &plan::late_key(&t.tour.line, &t.tour.tour, duty), Some(fill));
    let dep = t.tour.trips.get(t.duties[duty].start).map_or(0, |x| x.dep);
    c.clock.today.as_mut()?.controls.push(Control { line: t.tour.line.clone(), tour: t.tour.tour.clone(), dep, at: now, act: Act::Cover { duty, from }, auto });
    let at = ck::moment(&c.date, now as i64);
    let time = ck::hhmm(ck::moment(&dp.date, from as i64));
    let (text, args) = match fill {
        Fill::Colleague { id } => {
            let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
            (COVERED, vec![ck::arg("name", name), ck::arg("n", &t.tour.number), ck::arg("tour", &t.tour.tour), ck::arg("time", time)])
        }
        _ => (AGENCY_COVERS, vec![ck::arg("n", &t.tour.number), ck::arg("tour", &t.tour.tour), ck::arg("time", time)]),
    };
    ck::tell(c, run, at, text, args.clone(), Level::Info);
    if auto {
        run.breaks.push(Message::new(Kind::Lines, BY_CONTROLLER, text).args_of(&args).level(Level::Info).dated(&dp.date));
    }
    Some((text.to_string(), args))
}

trait ArgsOf {
    fn args_of(self, args: &[(String, String)]) -> Self;
}

impl ArgsOf for Message {
    fn args_of(mut self, args: &[(String, String)]) -> Message {
        for (k, v) in args {
            self = self.arg(k, v.clone());
        }
        self
    }
}

// --- the clock's watch ------------------------------------------------------------------------------

/// The clock looks at its day every `WATCH_EVERY` minutes (`clock::advance`): `watch` with the
/// day's plan as it runs now. A quick run (closing the day at once, catching up, the game's
/// clock) only for Casual's controller, every half hour: nobody reads notices then.
pub fn watch_day(c: &mut Company, w: &mut dyn World, quick: bool, run: &mut Run) -> Result<(), String> {
    if quick && !super::autopilot::on(c) {
        return Ok(());
    }
    let every = if quick { 30 } else { WATCH_EVERY };
    let now = c.clock.minute as i32;
    let Some(s) = script(c, &c.date) else { return Ok(()) };
    if now >= ck::DAY as i32 || s.watch.at.is_some_and(|a| a <= now && now < a + every) {
        return Ok(());
    }
    let date = c.date.clone();
    let lines = w.lines(c, &date)?;
    let tours = network::in_service_only(network::tours_of_day(c, &lines, &date));
    let dp = plan::day_plan(c, &date, tours, &[], &[], false);
    watch(c, &dp, run);
    Ok(())
}

/// Service control's look at the company's day (`dp`, its plan now): Casual's controller does
/// the best for each late bus and late driver by himself; the buses late more than the notice's
/// minutes are told once a day (the feed, the inbox, the pages); what is late is kept for the
/// Operations page's badge.
pub fn watch(c: &mut Company, dp: &DayPlan, run: &mut Run) {
    if script(c, &dp.date).is_none() || !dp.today {
        return;
    }
    let mut lv = live(c, dp);
    if super::autopilot::on(c) {
        let mut acted = false;
        for b in &lv.buses {
            if let Some(o) = b.options.iter().find(|o| o.best) {
                acted |= apply(c, dp, b.ti, o.dep, o.act, true, run).is_some();
            }
        }
        for d in &lv.drivers {
            if let Some(x) = d.covers.iter().find(|x| x.best) {
                cover(c, dp, d.ti, d.duty, x.fill, x.from, true, run);
            }
        }
        if acted {
            lv = live(c, dp);
        }
        lv.drivers.clear();
    }
    let n = notice_minutes(c) as i32;
    let at = ck::moment(&c.date, lv.now as i64);
    let mut told = Vec::new();
    for b in &lv.buses {
        let t = &dp.tours[b.ti];
        let key = plan::tour_key(&t.tour.line, &t.tour.tour);
        if n == 0 || b.minutes < n.max(LATE_AT + 1) || script(c, &dp.date).is_some_and(|s| s.watch.told.contains(&key)) {
            continue;
        }
        let args = vec![ck::arg("bus", number(c, b.vehicle)), ck::arg("n", &t.tour.number), ck::arg("tour", &t.tour.tour), ck::arg("min", b.minutes)];
        ck::tell(c, run, at, LATE_FEED, args.clone(), Level::Warn);
        let msg = Message::new(Kind::Lines, LATE_TITLE, LATE_TEXT)
            .args_of(&args)
            .arg("trips", b.nothing.late_trips.to_string())
            .arg("amount", b.nothing.penalty.to_string())
            .level(Level::Warn)
            .dated(&dp.date)
            .key(&format!("late-{}-{key}", dp.date));
        super::inbox::post(c, msg.clone());
        run.breaks.push(msg);
        told.push(key);
    }
    let late: Vec<(String, i32)> = lv.buses.iter().map(|b| (plan::tour_key(&dp.tours[b.ti].tour.line, &dp.tours[b.ti].tour.tour), b.minutes)).collect();
    if let Some(s) = c.clock.today.as_mut().filter(|s| s.date == dp.date) {
        s.watch.told.extend(told);
        s.watch.late = late;
        s.watch.drivers = lv.drivers.len();
        s.watch.at = Some(lv.now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::network::tests::trip;
    use crate::company::{found, Founding};

    /// A company on Normal with the clock in its day, and a block of four trips an hour apart from
    /// 06:00 (each 50 minutes, 10 minutes' layover).
    fn day() -> (Company, TourOfDay) {
        let mut c = found(&Founding { name: "Leit".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.clock.today = Some(DayScript { date: c.date.clone(), ..Default::default() });
        let trips = (0..4).map(|k| trip(6 * 60 + 60 * k, 6 * 60 + 50 + 60 * k, 10)).collect();
        (c, TourOfDay { line: "Linie5".into(), number: "5".into(), tour: "1".into(), trips, ..Default::default() })
    }

    fn block(t: &TourOfDay) -> Block<'_> {
        Block { tour: t, bus: Some(1), drivers: vec![Some(7); t.trips.len()], runs: vec![true; t.trips.len()] }
    }

    fn control(t: &TourOfDay, j: usize, act: Act) -> Control {
        Control { line: t.line.clone(), tour: t.tour.clone(), dep: t.trips[j].dep, at: t.trips[j].dep, act, auto: false }
    }

    /// The first trip held up 20 minutes, the others not at all.
    fn twenty(j: usize, _: Option<u32>) -> i32 {
        if j == 0 {
            20
        } else {
            0
        }
    }

    #[test]
    fn a_delay_is_carried_along_the_block_less_the_layover() {
        let (c, t) = day();
        let d = chain_with(&block(&t), None, &[], None, &twenty);
        // 20 late at the end of the first trip: 10 minutes' layover, 3 of them kept
        assert_eq!((d[0].late, d[1].start, d[2].start, d[3].start), (20, 13, 6, 0));
        // as the clock sees it at 06:55: the trips ahead are not begun, their draws not known
        let now = chain_with(&block(&t), Some(6 * 60 + 55), &[], None, &twenty);
        assert!(now[0].known && !now[1].known);
        assert_eq!(effect(&c, &t, &now, 0).late_trips, 2);
        // (the clock's own draws: the same day draws the same)
        assert_eq!(chain(&c, &c.date, &block(&t), None, &[], None), chain(&c, &c.date, &block(&t), None, &[], None));
    }

    #[test]
    fn a_short_turn_brings_the_next_trip_on_time() {
        let (c, t) = day();
        let x = control(&t, 0, Act::ShortTurn { part: 0.2, saved: 20 });
        let d = chain_with(&block(&t), None, &[], Some(&x), &twenty);
        assert_eq!((d[0].late, d[1].start, d[2].start), (0, 0, 0));
        assert_eq!(d[0].recovered, 20);
        // the end of the trip and the start of the next are lost, nothing else
        assert_eq!((d[0].lost, d[1].lost, d[2].lost), (0.2, 0.2, 0.0));
        // what it costs: a part of a trip's penalty for each part lost
        let r = economy::rules(c.game_mode);
        assert_eq!(part_penalty(&r, &t.trips[0], 0.2), ((r.drop_per_trip as f64 + t.trips[0].km * r.drop_per_km as f64) * PART_SHARE * 0.2).round() as Cents);
        assert_eq!(part_penalty(&r, &t.trips[0], 0.0), 0);
        // against doing nothing: two late trips ahead avoided, a part-trip's cost instead
        let seen = |x: Option<&Control>| {
            let mut d = chain_with(&block(&t), None, &[], x, &twenty);
            d.iter_mut().skip(1).for_each(|y| y.known = false);
            effect(&c, &t, &d, 0)
        };
        let (before, after) = (seen(None), seen(Some(&x)));
        assert_eq!((before.late_trips, after.late_trips), (2, 0));
        assert_eq!(after.on_time_from, Some(t.trips[1].dep));
        assert!(after.parts_cost > 0 && after.parts > 0.3);
        // a shorter layover takes 3 minutes off the next trip only
        let y = control(&t, 1, Act::Layover { saved: 3 });
        let d = chain_with(&block(&t), None, &[], Some(&y), &twenty);
        assert_eq!((d[1].start, d[2].start), (10, 3));
    }

    #[test]
    fn a_spare_swapped_in_at_the_terminus_runs_the_rest_on_time() {
        let (mut c, t) = day();
        let x = control(&t, 1, Act::Swap { vehicle: 9, from_bus: 1 });
        let d = chain_with(&block(&t), None, &[], Some(&x), &twenty);
        assert_eq!((d[0].bus, d[1].bus, d[3].bus), (None, Some(9), Some(9)));
        assert_eq!((d[1].start, d[1].recovered, d[2].start), (0, 13, 0));
        // the late bus is free from the swap (the spare now), the spare busy until the tour ends
        let s = c.clock.today.as_mut().unwrap();
        s.busy = vec![(1, t.from(), t.to())];
        s.controls.push(x.clone());
        assert_eq!(busy(s), vec![(1, t.from(), x.dep), (9, x.dep, t.to())]);
    }

    #[test]
    fn a_late_driver_covered_during_the_day_drops_only_the_trips_before() {
        let (mut c, t) = day();
        let duty = plan::DayDuty { start: 0, end: 4, from: t.from(), to: t.to(), who: Some(Who::Staff(7)), late: Some(plan::LatePiece { until: 8 * 60, cover: None, from_: plan::Source::None, cover_from: None }), ..Default::default() };
        let mut dp = DayPlan { date: c.date.clone(), today: true, tours: vec![DayTour { tour: t.clone(), bus: Some(BusOf::Own(1)), duties: vec![duty], ..Default::default() }], ..Default::default() };
        // nobody covers: the trips before 08:00 are dropped
        assert_eq!(dp.tours[0].duties[0].dropped_before(), Some(8 * 60));
        let b = day_block(&c, c.clock.today.as_ref(), &dp.tours[0]);
        assert_eq!(b.runs, vec![false, false, true, true]);
        // a colleague sent at 06:30 covers them from then on: the 06:00 trip is lost, the 07:00 runs
        let mut run = Run::default();
        c.clock.minute = 6 * 60 + 30;
        assert!(cover(&mut c, &dp, 0, 0, Fill::Colleague { id: 8 }, 6 * 60 + 30, false, &mut run).is_some());
        assert_eq!(cover_from(&c, &c.date, "Linie5", "1", 0), Some(6 * 60 + 30));
        dp.tours[0].duties[0].late = Some(plan::LatePiece { until: 8 * 60, cover: Some(8), from_: plan::Source::Dispatcher, cover_from: Some(6 * 60 + 30) });
        assert_eq!(dp.to_plan().tours[0].duties[0].dropped_before, Some(6 * 60 + 30));
        let b = day_block(&c, c.clock.today.as_ref(), &dp.tours[0]);
        assert_eq!(b.runs, vec![false, true, true, true]);
        // covered from the morning: nothing dropped
        dp.tours[0].duties[0].late = Some(plan::LatePiece { until: 8 * 60, cover: Some(8), from_: plan::Source::Dispatcher, cover_from: None });
        assert_eq!(dp.tours[0].duties[0].dropped_before(), None);
    }

    #[test]
    fn old_saves_load() {
        // a day's script, a clock and a late piece of before service control
        let s: DayScript = serde_json::from_str(r#"{"date":"2024-03-04","events":[],"breaks":[],"busy":[[1,360,600]]}"#).unwrap();
        assert!(s.controls.is_empty() && s.watch == Watch::default());
        let k: ck::Clock = serde_json::from_str(r#"{"minute":400}"#).unwrap();
        assert_eq!(k.late_notice, None);
        let p: plan::LatePiece = serde_json::from_str(r#"{"until":480,"cover":null,"from_":"none"}"#).unwrap();
        assert_eq!(p.cover_from, None);
        // and what is kept now reads back
        let x = Control { line: "L".into(), tour: "1".into(), dep: 400, at: 390, act: Act::Express { part: 0.5, saved: 4 }, auto: true };
        let back: Control = serde_json::from_str(&serde_json::to_string(&x).unwrap()).unwrap();
        assert_eq!(back, x);
    }
}

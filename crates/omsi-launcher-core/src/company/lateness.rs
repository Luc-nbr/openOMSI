//! Late running: how many minutes each trip of the company's day runs late, and why.
//!
//! Luc (2026-10-15) and Trello #97: a late trip was a random draw per trip, deadheads, layovers
//! and reliefs made no difference, and nobody was told the minutes or the cause. Now the day is
//! worked out from its plan (`model`), bus by bus and driver by driver in the order the duties
//! begin:
//!
//! - a trip leaves when its bus is there (the trip before it arrived, late or not: a delay
//!   carries on along the block, and the layover in between absorbs it), when its driver is
//!   there (signed on - a few minutes late now and then, `staff::signs_on_late`, or much later
//!   after the morning's `plan::Disruption::Late` -, or came from another duty, `RELIEF_WALK`
//!   on), and when the spare sent for a broken-down bus came;
//! - on the way it loses (or makes up) time: the traffic of the hour (the rush hours most,
//!   `band`), the driver's punctuality and experience, the bus's condition, the day's events on
//!   the line (`inbox::line_effect`), and a small part of chance; a bus breaking down on the way
//!   holds its passengers until the spare comes;
//! - its minutes late are the most it ran behind, at its start or its end, and a trip is late
//!   from `economy::LATE_FROM` minutes. The chance is drawn from the company, the trip and the
//!   date: the same plan on similar days runs alike.
//!
//! The close books each late trip's deduction (`economy::late_share` of its value), keeps the
//! late and dropped trips of the last `KEPT_DAYS` days (`Store`, for RosterFlow and the advice),
//! and the report sums them up (`Lateness`). `advice` turns the causes into what to change.

use super::clock::{self, Cut};
use super::control::{self, Act, Control};
use super::dates;
use super::day::{self, DropWhy, Plan, TourPlan};
use super::economy;
use super::model::{Cents, Company};
use super::network::PlannedTrip;
use super::plan::{self, Problem, Who, AGENCY, PLAYER};
use super::rng::Rng;
use super::staff;
use crate::service::ServiceKind;
use serde::{Deserialize, Serialize};

/// Minutes from signing on late in the morning (after the trips before were dropped) to the bus.
pub const SIGN_ON_WALK: i32 = 2;
/// Minutes a driver needs from one duty to another on another bus (the walk to the relief).
pub const RELIEF_WALK: i32 = 3;
/// Minutes a broken-down bus's passengers wait when no bus is sent.
pub const BREAKDOWN_WAIT: i32 = 20;
/// The days of late and dropped trips kept (`Store`).
pub const KEPT_DAYS: usize = 21;
/// The late trips a day's report lists at the most (the worst).
pub const LISTED: usize = 200;

/// Why a trip ran late (each with the minutes it brought).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cause {
    /// The driver signed on late (`driver`: an employee, `plan::AGENCY`).
    SignOn { driver: u32, minutes: i32 },
    /// The bus's trip before arrived late: its departure, its planned arrival and where, its
    /// minutes late there, and the layover planned between (a deadhead: an empty run).
    Carried { dep: i32, arr: i32, stop: String, late: i32, layover: i32, deadhead: bool, minutes: i32 },
    /// The driver came from another duty that ended late or too close: when it ended (as it
    /// ran), and the minutes planned between.
    Relief { driver: u32, end: i32, gap: i32, minutes: i32 },
    /// The bus broke down (`bus`): when, and when the bus sent for it came.
    Breakdown { bus: u32, at: i32, spare: Option<i32>, minutes: i32 },
    /// The traffic of the hour (`peak`: the rush hours).
    Traffic { peak: bool, minutes: i32 },
    /// The driver drives slowly (punctuality, experience).
    Driver { driver: u32, minutes: i32 },
    /// The bus's condition.
    Bus { bus: u32, minutes: i32 },
    /// The day's events on the line (roadworks, a diversion, the weather).
    Event { minutes: i32 },
    /// The way itself (passengers, lights).
    Chance { minutes: i32 },
}

impl Cause {
    pub fn minutes(&self) -> i32 {
        match self {
            Cause::SignOn { minutes, .. } | Cause::Carried { minutes, .. } | Cause::Relief { minutes, .. } | Cause::Breakdown { minutes, .. } | Cause::Traffic { minutes, .. } | Cause::Driver { minutes, .. } | Cause::Bus { minutes, .. } | Cause::Event { minutes } | Cause::Chance { minutes } => *minutes,
        }
    }

    fn set_minutes(&mut self, m: i32) {
        match self {
            Cause::SignOn { minutes, .. } | Cause::Carried { minutes, .. } | Cause::Relief { minutes, .. } | Cause::Breakdown { minutes, .. } | Cause::Traffic { minutes, .. } | Cause::Driver { minutes, .. } | Cause::Bus { minutes, .. } | Cause::Event { minutes } | Cause::Chance { minutes } => *minutes = m,
        }
    }

    /// Its key (the finance's sums by cause, the texts).
    pub fn key(&self) -> &'static str {
        match self {
            Cause::SignOn { .. } => "sign_on",
            Cause::Carried { .. } => "carried",
            Cause::Relief { .. } => "relief",
            Cause::Breakdown { .. } => "breakdown",
            Cause::Traffic { peak: true, .. } => "peak",
            Cause::Traffic { .. } => "traffic",
            Cause::Driver { .. } => "driver",
            Cause::Bus { .. } => "bus",
            Cause::Event { .. } => "event",
            Cause::Chance { .. } => "chance",
        }
    }

    /// Its words in a booking (English: bookings are not translated).
    pub fn words(&self) -> &'static str {
        match self {
            Cause::SignOn { .. } => "driver signed on late",
            Cause::Carried { .. } => "late from the trip before",
            Cause::Relief { .. } => "relief too tight",
            Cause::Breakdown { .. } => "breakdown",
            Cause::Traffic { peak: true, .. } => "rush-hour traffic",
            Cause::Traffic { .. } => "traffic",
            Cause::Driver { .. } => "slow driving",
            Cause::Bus { .. } => "bus in poor condition",
            Cause::Event { .. } => "event on the line",
            Cause::Chance { .. } => "delays on the way",
        }
    }
}

/// A trip as the model ran it: its minutes late at its start and its end (negative: early), the
/// bus that ran it, its causes, and the causes up the chain (of the trip whose delay it carried).
#[derive(Clone, Debug, PartialEq)]
pub struct Timed {
    pub dep: f64,
    pub arr: f64,
    pub bus: u32,
    pub causes: Vec<Cause>,
    pub earlier: Vec<Cause>,
    /// It had begun when the model looked (`model_with`'s `now`): its own hold-ups are known.
    pub known: bool,
    /// The controller's doing (`control`): the share of it not run in service (a short-turn, out
    /// of service), the minutes taken off its delay, the bus swapped in that ran it.
    pub lost: f64,
    pub recovered: i32,
    pub swapped: Option<u32>,
}

impl Timed {
    /// Its minutes late: the most it ran behind, at its start or its end.
    pub fn minutes(&self) -> i32 {
        self.dep.max(self.arr).round().max(0.0) as i32
    }
}

/// The day's trips as run: `[tour][trip]` of the plan (None: not run, or not modelled).
pub type Model = Vec<Vec<Option<Timed>>>;

/// The traffic of a minute of the day on a weekday (0 Monday): the share it adds to a trip's
/// running time (negative: the quiet hours make time up), and whether it is a rush hour.
pub fn band(weekday: u32, minute: i32) -> (f64, bool) {
    let h = minute.rem_euclid(1440) as f64 / 60.0;
    let within = |a: f64, b: f64| h >= a && h < b;
    match weekday {
        0..=4 if within(7.0, 9.0) => (0.09, true),
        0..=4 if within(15.5, 18.5) => (0.08, true),
        0..=4 if within(6.0, 7.0) || within(9.0, 10.0) || within(14.0, 15.5) || within(18.5, 19.5) => (0.03, false),
        0..=4 if within(10.0, 14.0) => (0.01, false),
        5 if within(10.0, 17.0) => (0.04, false),
        5 if within(8.0, 10.0) || within(17.0, 19.0) => (0.01, false),
        6 if within(11.0, 18.0) => (0.01, false),
        _ => (-0.03, false),
    }
}

/// Work out the day's trips as they ran (see the module): `cut` the buses cut short today
/// (`clock::breakdowns`), with what the controller did (`control`, the day's script).
pub fn model(c: &Company, plan: &Plan, date: &str, cut: &[Cut]) -> Model {
    let controls = control::of_day(c, date);
    model_with(c, plan, date, cut, &controls, None, None)
}

/// A trip's own minutes on the way, set by hand (tests: `model_with`'s `own`).
pub type Own<'a> = &'a dyn Fn(usize, usize) -> f64;

/// `model` with the controller's `controls` (the day's, and one more to see what it would do),
/// as the clock sees the day at `now` (None: all of it, the close): a trip not begun by then
/// carries the delay it starts with, its own hold-ups not known yet. `own`: each trip's own
/// minutes on the way by hand (tests), instead of the traffic, the driver, the bus and chance.
pub fn model_with(c: &Company, plan: &Plan, date: &str, cut: &[Cut], controls: &[&Control], now: Option<i32>, own: Option<Own>) -> Model {
    let day = dates::parse(date).unwrap_or(0);
    let wd = dates::weekday(day);
    // (a sandbox's delays by its factor: `sandbox::Sandbox::lateness`)
    let factor = super::research::late_factor(c) * c.game_mode.rules().lateness * super::sandbox::of(c).lateness;
    let mut out: Model = plan.tours.iter().map(|t| vec![None; t.tour.trips.len()]).collect();
    let mut order: Vec<(usize, usize)> = plan.tours.iter().enumerate().filter(|(_, t)| !t.by_player).flat_map(|(i, t)| (0..t.duties.len()).map(move |k| (i, k))).collect();
    order.sort_by_key(|&(i, k)| (plan.tours[i].duties[k].from, i, k));
    // (each bus: when it is free, as it ran, after which trip; each driver the same)
    let mut buses: Vec<(u32, f64, usize, usize)> = Vec::new();
    let mut drivers: Vec<(u32, f64, usize, usize)> = Vec::new();
    // (each tour: the bus the controller swapped in, and the trip before run - a short-turn cuts
    // the start of the next trip too)
    let mut swapped: Vec<Option<u32>> = vec![None; plan.tours.len()];
    let mut last_run: Vec<Option<i32>> = vec![None; plan.tours.len()];
    for (ti, k) in order {
        let tp = &plan.tours[ti];
        let d = &tp.duties[k];
        let Some(id) = d.driver else { continue };
        let Some(e) = (if id == AGENCY { Some(plan::agency_driver()) } else { c.employee(id).cloned() }) else { continue };
        let broken_at = tp.bus.and_then(|b| cut.iter().find(|x| x.vehicle == b)).copied();
        let spare = tp.bus.and_then(|b| clock::spare_of(c, date, b));
        let event = super::inbox::line_effect(c, date, &tp.tour.number).0;
        let mine: Vec<&Control> = controls.iter().copied().filter(|x| control::same(&x.line, &tp.tour.line) && control::same(&x.tour, &tp.tour.tour)).collect();
        let mut first = true;
        for j in d.start..d.end {
            let x = &tp.tour.trips[j];
            // (a trip's second part, cut at a relief point, runs as its whole trip does)
            let head = if x.is_second() && j > 0 { j - 1 } else { j };
            let hk = tp.duties.iter().position(|dd| dd.start <= head && head < dd.end).unwrap_or(k);
            if day::why_not(c, tp, hk, head, broken_at).is_some() {
                continue;
            }
            let here: Vec<&Control> = mine.iter().copied().filter(|m| m.dep == x.dep).collect();
            // (the controller swapped a bus in from this trip on: it runs the rest of the block)
            let swap = here.iter().find_map(|m| if let Act::Swap { vehicle, .. } = m.act { Some(vehicle) } else { None });
            // (what the bus swapped out would have brought late: the swap takes it off)
            let swap_saved = swap.and_then(|_| swapped[ti].or(tp.bus)).and_then(|old| buses.iter().find(|b| b.0 == old)).map_or(0.0, |&(_, free, pt, pj)| {
                let p = &plan.tours[pt].tour.trips[pj];
                (free + (x.dep - p.arr).clamp(0, control::MIN_LAYOVER) as f64 - x.dep as f64).max(0.0)
            });
            if swap.is_some() {
                swapped[ti] = swap;
            }
            let head_dep = tp.tour.trips[head].dep;
            let by_spare = spare.is_some() && broken_at.is_some_and(|b| head_dep >= b.at && b.covered(head_dep));
            let Some(bus) = (if by_spare { spare } else { swapped[ti].or(tp.bus) }) else { continue };
            let v = c.vehicle(bus);
            // when the bus, the driver and a spare are there (a bus coming in late keeps
            // `control::MIN_LAYOVER` of its layover for the driver's rest)
            let mut ready: Vec<(f64, Cause, Option<(usize, usize)>)> = Vec::new();
            if let Some(&(_, free, pt, pj)) = buses.iter().find(|b| b.0 == bus).filter(|_| swap.is_none()) {
                let p = &plan.tours[pt].tour.trips[pj];
                let late = free - p.arr as f64;
                if late > 0.0 {
                    let stop = if p.last.is_empty() { p.to.clone() } else { p.last.clone() };
                    let rest = (x.dep - p.arr).clamp(0, control::MIN_LAYOVER) as f64;
                    ready.push((free + rest, Cause::Carried { dep: p.dep, arr: p.arr, stop, late: late.round() as i32, layover: x.dep - p.arr, deadhead: !p.counts(), minutes: 0 }, Some((pt, pj))));
                }
            }
            if let (true, Some(b)) = (by_spare, broken_at) {
                let back = b.back.unwrap_or(b.at);
                ready.push((back as f64, Cause::Breakdown { bus: tp.bus.unwrap_or(bus), at: b.at, spare: Some(back), minutes: 0 }, None));
            }
            if first {
                first = false;
                // (the agency's drivers share one number: each of their duties begins afresh)
                match drivers.iter().find(|w| w.0 == id && id != AGENCY) {
                    // (from another duty: on the same bus straight on, or a walk to another)
                    Some(&(_, end, pt, pj)) => {
                        let walk = if pt == ti { 0 } else { RELIEF_WALK };
                        let gap = x.dep - plan.tours[pt].tour.trips[pj].arr;
                        ready.push((end + walk as f64, Cause::Relief { driver: id, end: end.round() as i32, gap, minutes: 0 }, Some((pt, pj))));
                    }
                    // (the morning: came late, the trips before were dropped; or signs on a few
                    // minutes late - known once the duty has begun)
                    None => {
                        // (with the trips' own minutes set by hand, no draw: `own`)
                        let at = match d.dropped_before {
                            Some(t) => t + SIGN_ON_WALK,
                            None if own.is_none() && now.is_none_or(|m| d.from <= m) => d.from + staff::signs_on_late(c, &e, date),
                            None => d.from,
                        };
                        ready.push((at as f64, Cause::SignOn { driver: id, minutes: 0 }, None));
                    }
                }
            }
            let mut at = x.dep as f64;
            let mut binding: Option<(Cause, Option<(usize, usize)>)> = None;
            for (r, cause, from) in ready {
                if r > at + 0.01 {
                    at = r;
                    binding = Some((cause, from));
                }
            }
            let mut dep_delay = at - x.dep as f64;
            // the controller's doing at this trip: a shorter layover takes minutes off its start; a
            // short-turn (here, or at the trip before) and running out of service cut a part of it;
            // a bus swapped in leaves on time
            let mut recovered = swap_saved;
            let mut lost: f64 = 0.0;
            for m in &mine {
                match m.act {
                    Act::Layover { saved } if m.dep == x.dep => {
                        let r = (saved as f64).min(dep_delay.max(0.0));
                        dep_delay -= r;
                        recovered += r;
                    }
                    Act::ShortTurn { part, .. } if m.dep == x.dep || last_run[ti] == Some(m.dep) => lost = lost.max(part),
                    Act::Express { part, .. } if m.dep == x.dep => lost = lost.max(part),
                    _ => {}
                }
            }
            let mut causes: Vec<Cause> = Vec::new();
            let mut earlier: Vec<Cause> = Vec::new();
            if let Some((mut cause, from)) = binding {
                cause.set_minutes(dep_delay.round() as i32);
                if cause.minutes() >= 1 {
                    causes.push(cause);
                    if let Some(prev) = from.and_then(|(pt, pj)| out[pt][pj].as_ref()) {
                        earlier = prev.causes.iter().chain(prev.earlier.iter()).filter(|c| !matches!(c, Cause::Carried { .. } | Cause::Relief { .. })).cloned().take(3).collect();
                    }
                }
            }
            // on the way (known once the trip has begun)
            let known = now.is_none_or(|m| x.dep as f64 + dep_delay <= m as f64);
            let run = (x.arr - x.dep).max(1) as f64;
            let counts = x.counts();
            let mut rng = Rng::of(&[&c.id, "running", &tp.tour.line, &tp.tour.tour], day * 1440 + x.dep as i64);
            let (share, peak) = band(wd, x.dep);
            let traffic = run * share * factor * if counts { 1.0 } else { 0.6 };
            let slow = if counts { run * ((60.0 - e.skills.punctuality) / 100.0 * 0.08 + (1.0 - e.experience / 100.0).max(0.0) * 0.02) * factor } else { 0.0 };
            let worn = v.map_or(0.0, |v| run * (1.0 - v.condition / 100.0).max(0.0) * 0.06 * factor);
            let events = run * event * 0.4;
            let chance = run * rng.range(-0.03, 0.05) * factor;
            let parts = match own {
                Some(f) => [(f(ti, j), Cause::Chance { minutes: 0 }), (0.0, Cause::Traffic { peak, minutes: 0 }), (0.0, Cause::Driver { driver: id, minutes: 0 }), (0.0, Cause::Bus { bus, minutes: 0 }), (0.0, Cause::Event { minutes: 0 })],
                None => [(traffic, Cause::Traffic { peak, minutes: 0 }), (slow, Cause::Driver { driver: id, minutes: 0 }), (worn, Cause::Bus { bus, minutes: 0 }), (events, Cause::Event { minutes: 0 }), (chance, Cause::Chance { minutes: 0 })],
            };
            let mut arr_delay = dep_delay;
            if known {
                for (m, mut cause) in parts {
                    arr_delay += m;
                    if m.round() >= 1.0 {
                        cause.set_minutes(m.round() as i32);
                        causes.push(cause);
                    }
                }
                // (broken down on the way: its passengers wait for the bus sent, or long)
                if let Some(b) = broken_at.filter(|b| !by_spare && x.dep < b.at && b.at <= x.arr) {
                    let wait = b.back.map_or(BREAKDOWN_WAIT, |s| s - b.at).clamp(5, 90);
                    arr_delay += wait as f64;
                    causes.push(Cause::Breakdown { bus, at: b.at, spare: b.back, minutes: wait });
                }
            }
            // (a short-turn or running out of service: the minutes it saves off the end)
            for m in &here {
                if let Act::ShortTurn { saved, .. } | Act::Express { saved, .. } = m.act {
                    let r = (saved as f64).min(arr_delay.max(0.0));
                    arr_delay -= r;
                    recovered += r;
                }
            }
            // (a bus may come in a little early; the next trip still waits for its time)
            let arr_delay = arr_delay.max(-3.0);
            let free = x.arr as f64 + arr_delay.max(0.0);
            match buses.iter_mut().find(|b| b.0 == bus) {
                Some(b) => *b = (bus, free, ti, j),
                None => buses.push((bus, free, ti, j)),
            }
            match drivers.iter_mut().find(|w| w.0 == id) {
                Some(w) => *w = (id, free, ti, j),
                None if id != AGENCY => drivers.push((id, free, ti, j)),
                None => {}
            }
            last_run[ti] = Some(x.dep);
            let swapped_bus = swapped[ti].filter(|_| !by_spare && swapped[ti] != tp.bus);
            out[ti][j] = Some(Timed { dep: dep_delay, arr: arr_delay, bus, causes, earlier, known, lost, recovered: recovered.round() as i32, swapped: swapped_bus });
        }
    }
    out
}

/// Trip `j` of tour `ti` as the close books it: with its second part when it was cut at a
/// relief point on the way (`joined`), from the first part's start to the second's end.
pub fn whole(m: &Model, ti: usize, j: usize, joined: bool) -> Option<Timed> {
    let a = m.get(ti)?.get(j)?.clone()?;
    let Some(b) = m[ti].get(j + 1).cloned().flatten().filter(|_| joined) else { return Some(a) };
    let mut causes = a.causes;
    for x in b.causes {
        if !causes.iter().any(|y| y.key() == x.key()) {
            causes.push(x);
        }
    }
    let mut earlier = a.earlier;
    earlier.extend(b.earlier);
    earlier.truncate(3);
    Some(Timed { dep: a.dep, arr: b.arr, bus: a.bus, causes, earlier, known: a.known && b.known, lost: a.lost.max(b.lost), recovered: a.recovered + b.recovered, swapped: a.swapped.or(b.swapped) })
}

// --- what became of a trip ---------------------------------------------------------------------

/// A trip of the day that ran late or was dropped: its line (the timetable's name and the
/// number shown), block, duty and times, its name (the timetable's trip ID), the bus and the
/// driver it had - or, dropped for the one missing, the roster's that could not come -, and
/// then either its minutes late, causes and deduction, or why it was dropped.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Outcome {
    pub line: String,
    pub number: String,
    pub tour: String,
    #[serde(default)]
    pub duty: Option<usize>,
    pub dep: i32,
    pub arr: i32,
    #[serde(default)]
    pub trip: String,
    #[serde(default)]
    pub bus: Option<u32>,
    /// An employee, `plan::AGENCY` or `plan::PLAYER`.
    #[serde(default)]
    pub driver: Option<u32>,
    #[serde(default)]
    pub minutes: i32,
    #[serde(default)]
    pub causes: Vec<Cause>,
    #[serde(default)]
    pub earlier: Vec<Cause>,
    /// The trip's value under the contract, the share deducted and the deduction.
    #[serde(default)]
    pub value: Cents,
    #[serde(default)]
    pub share: f64,
    #[serde(default)]
    pub deduction: Cents,
    #[serde(default)]
    pub dropped: Option<DropWhy>,
    #[serde(default)]
    pub problem: Option<Problem>,
}

impl Outcome {
    /// Its tier (`economy::late_tier`; dropped: none).
    pub fn tier(&self) -> usize {
        if self.dropped.is_some() {
            0
        } else {
            economy::late_tier(self.minutes)
        }
    }

    /// The cause that brought it most minutes (a delay carried on: the first cause up the chain
    /// counts with it in the sums by cause, `Lateness::causes`).
    pub fn main(&self) -> Option<&Cause> {
        self.causes.iter().max_by_key(|c| c.minutes())
    }
}

fn who_id(w: Who) -> Option<u32> {
    match w {
        Who::Staff(id) => Some(id),
        Who::Agency => Some(AGENCY),
        Who::Player => Some(PLAYER),
        Who::Guest(_) => None,
    }
}

/// A trip dropped (`day::why_not`), with the bus and the driver it had or was to have.
pub fn dropped(c: &Company, date: &str, tp: &TourPlan, k: usize, trip: &PlannedTrip, why: DropWhy, problem: Option<Problem>) -> Outcome {
    let roster = plan::roster(c, plan::weekday_of(date), &tp.tour.line, &tp.tour.tour);
    let d = tp.duties.get(k);
    let bus = tp.bus.or_else(|| roster.and_then(|r| r.bus));
    let driver = d.and_then(|d| d.driver).or_else(|| roster.and_then(|r| r.duties.get(k).copied().flatten()).and_then(who_id));
    Outcome { line: tp.tour.line.clone(), number: tp.tour.number.clone(), tour: tp.tour.tour.clone(), duty: Some(k), dep: trip.dep, arr: trip.arr, trip: trip.name.clone(), bus, driver, dropped: Some(why), problem, ..Default::default() }
}

/// A trip run late: its minutes and causes, and its deduction (its share of `value`).
pub fn late(tp: &TourPlan, k: usize, trip: &PlannedTrip, bus: u32, driver: u32, t: Timed, value: Cents) -> Outcome {
    let minutes = t.minutes();
    let share = economy::late_share(minutes);
    Outcome {
        line: tp.tour.line.clone(),
        number: tp.tour.number.clone(),
        tour: tp.tour.tour.clone(),
        duty: Some(k),
        dep: trip.dep,
        arr: trip.arr,
        trip: trip.name.clone(),
        bus: Some(bus),
        driver: Some(driver),
        minutes,
        causes: t.causes,
        earlier: t.earlier,
        value,
        share,
        deduction: (value as f64 * share).round() as Cents,
        dropped: None,
        problem: None,
    }
}

fn hhmm(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

/// A late trip's deduction as the ledger has it (English: bookings are not translated).
pub fn booking_text(o: &Outcome) -> String {
    let why = o.main().map_or("late", |c| c.words());
    format!("Line {} {} (block {}): {} min late, {} - {:.0} % of the trip", o.number, hhmm(o.dep), o.tour, o.minutes, why, o.share * 100.0)
}

// --- what the day came to ------------------------------------------------------------------------

/// Punctuality of a line, an hour or a duty: trips timed, late, their minutes late, the worst.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Punct {
    pub key: String,
    #[serde(default)]
    pub tour: String,
    #[serde(default)]
    pub duty: Option<usize>,
    #[serde(default)]
    pub driver: Option<u32>,
    pub timed: u32,
    pub late: u32,
    pub minutes: u32,
    pub worst: i32,
}

impl Punct {
    fn add(&mut self, minutes: i32) {
        self.timed += 1;
        if minutes >= economy::LATE_FROM {
            self.late += 1;
            self.minutes += minutes as u32;
        }
        self.worst = self.worst.max(minutes);
    }
}

/// The deductions of a tier or a cause: trips and money.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Sum {
    #[serde(default)]
    pub key: String,
    pub trips: u32,
    pub minutes: u32,
    pub amount: Cents,
}

/// The day's late running, for the report and the finances.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Lateness {
    /// The late trips (the `LISTED` worst, in the order they left).
    #[serde(default)]
    pub trips: Vec<Outcome>,
    #[serde(default)]
    pub lines: Vec<Punct>,
    #[serde(default)]
    pub hours: Vec<Punct>,
    #[serde(default)]
    pub duties: Vec<Punct>,
    /// The deductions by tier (up to five minutes, under `economy::LATE_FULL`, from it) and by
    /// the main cause.
    #[serde(default)]
    pub tiers: Vec<Sum>,
    #[serde(default)]
    pub causes: Vec<Sum>,
}

fn punct<'a>(list: &'a mut Vec<Punct>, key: &str) -> &'a mut Punct {
    let at = match list.iter().position(|p| p.key == key) {
        Some(i) => i,
        None => {
            list.push(Punct { key: key.to_string(), ..Default::default() });
            list.len() - 1
        }
    };
    &mut list[at]
}

impl Lateness {
    /// A trip timed: its line (number), block, duty, driver, departure and minutes late.
    pub fn count(&mut self, number: &str, tour: &str, duty: usize, driver: u32, dep: i32, minutes: i32) {
        punct(&mut self.lines, number).add(minutes);
        punct(&mut self.hours, &format!("{:02}", dep.rem_euclid(1440) / 60)).add(minutes);
        let p = punct(&mut self.duties, &format!("{number}/{tour}/{duty}"));
        (p.tour, p.duty, p.driver) = (tour.to_string(), Some(duty), Some(driver));
        p.add(minutes);
    }

    /// The day's sums, with its late and dropped trips (`outcomes`).
    pub fn finish(mut self, outcomes: &[Outcome]) -> Lateness {
        let late: Vec<&Outcome> = outcomes.iter().filter(|o| o.dropped.is_none() && o.minutes >= economy::LATE_FROM).collect();
        self.tiers = (1..=3).map(|t| Sum { key: t.to_string(), ..Default::default() }).collect();
        for o in &late {
            let t = &mut self.tiers[o.tier().clamp(1, 3) - 1];
            (t.trips, t.minutes, t.amount) = (t.trips + 1, t.minutes + o.minutes as u32, t.amount + o.deduction);
            let key = o.main().map_or("chance", |c| c.key());
            let s = match self.causes.iter().position(|s| s.key == key) {
                Some(i) => &mut self.causes[i],
                None => {
                    self.causes.push(Sum { key: key.to_string(), ..Default::default() });
                    self.causes.last_mut().unwrap()
                }
            };
            (s.trips, s.minutes, s.amount) = (s.trips + 1, s.minutes + o.minutes as u32, s.amount + o.deduction);
        }
        self.causes.sort_by_key(|s| (std::cmp::Reverse(s.amount), std::cmp::Reverse(s.trips)));
        let mut worst: Vec<&Outcome> = late;
        worst.sort_by_key(|o| std::cmp::Reverse(o.minutes));
        worst.truncate(LISTED);
        worst.sort_by_key(|o| o.dep);
        self.trips = worst.into_iter().cloned().collect();
        self.hours.sort_by(|a, b| a.key.cmp(&b.key));
        self
    }

    pub fn deductions(&self) -> Cents {
        self.tiers.iter().map(|t| t.amount).sum()
    }
}

// --- kept for the planning -----------------------------------------------------------------------

/// A closed day's late and dropped trips.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DayOutcomes {
    pub date: String,
    #[serde(default)]
    pub trips: Vec<Outcome>,
}

/// The company's late and dropped trips: of the last `KEPT_DAYS` days closed, and what the live
/// day reported of today's (`feed_live`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Store {
    #[serde(default)]
    pub days: Vec<DayOutcomes>,
    #[serde(default)]
    pub live: Vec<Outcome>,
    #[serde(default)]
    pub live_date: String,
}

/// Keep a closed day's trips (the close), and forget the live day's.
pub fn keep(c: &mut Company, date: &str, trips: Vec<Outcome>) {
    let s = &mut c.lateness;
    s.days.retain(|d| d.date != date);
    s.days.push(DayOutcomes { date: date.to_string(), trips });
    s.days.sort_by(|a, b| a.date.cmp(&b.date));
    if s.days.len() > KEPT_DAYS {
        let extra = s.days.len() - KEPT_DAYS;
        s.days.drain(..extra);
    }
    s.live.clear();
    s.live_date.clear();
}

/// A closed day's late and dropped trips (None: not kept, or not closed).
pub fn of_day<'a>(c: &'a Company, date: &str) -> Option<&'a [Outcome]> {
    c.lateness.days.iter().find(|d| d.date == date).map(|d| d.trips.as_slice())
}

/// What the live day knows of one of today's trips (the live company, the Operations tab): it
/// stands for the model's in `today` until the day is closed.
pub fn feed_live(c: &mut Company, o: Outcome) {
    let date = c.date.clone();
    let s = &mut c.lateness;
    if s.live_date != date {
        s.live.clear();
        s.live_date = date;
    }
    s.live.retain(|x| !(x.line == o.line && x.tour == o.tour && x.dep == o.dep));
    s.live.push(o);
}

/// Today's late and dropped trips so far, as the plan runs (`plan`: today's, `day::Plan`; `now`:
/// the company clock's minute): the model's for the trips left by then, with what the live day
/// reported over them.
pub fn today(c: &Company, plan: &Plan, now: i32) -> Vec<Outcome> {
    let date = c.date.clone();
    let r = economy::rules_for(c);
    let cut = clock::breakdowns(c, plan, &date, &[]);
    let controls = control::of_day(c, &date);
    let m = model_with(c, plan, &date, &cut, &controls, Some(now), None);
    let mut out: Vec<Outcome> = Vec::new();
    for (ti, tp) in plan.tours.iter().enumerate().filter(|(_, t)| !t.by_player) {
        let school = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&tp.tour.line)).is_some_and(|l| super::ownline::kind_of(l) == ServiceKind::School);
        let broken_at = tp.bus.and_then(|b| cut.iter().find(|x| x.vehicle == b)).copied();
        for (k, d) in tp.duties.iter().enumerate() {
            for j in d.start..d.end {
                let x = &tp.tour.trips[j];
                if x.is_second() || !x.counts() || x.dep > now {
                    continue;
                }
                let joined = tp.tour.trips.get(j + 1).is_some_and(|n| x.part.is_some() && n.is_second());
                let trip = if joined { x.joined(&tp.tour.trips[j + 1]) } else { x.clone() };
                if let Some((why, problem, _)) = day::why_not(c, tp, k, j, broken_at) {
                    out.push(dropped(c, &date, tp, k, &trip, why, problem));
                    continue;
                }
                let Some(t) = whole(&m, ti, j, joined).filter(|t| t.minutes() >= economy::LATE_FROM) else { continue };
                let Some(driver) = d.driver else { continue };
                let value = economy::trip_value(&r, trip.km) * if school { super::ownline::SCHOOL_DROP } else { 1 };
                let bus = t.bus;
                out.push(late(tp, k, &trip, bus, driver, t, value));
            }
        }
    }
    if c.lateness.live_date == date {
        for o in &c.lateness.live {
            out.retain(|x| !(x.line == o.line && x.tour == o.tour && x.dep == o.dep));
            out.push(o.clone());
        }
    }
    out.sort_by_key(|o| o.dep);
    out
}

/// Where bus `number` broke down on `date` (the day's trips kept): its line's number, its block,
/// the minute and its driver (the inbox's message names them, `inbox::bridge`).
pub fn breakdown_of(c: &Company, date: &str, number: &str) -> Option<(String, String, i32, Option<u32>)> {
    let id = c.fleet.iter().find(|v| v.number == number)?.id;
    let day = of_day(c, date)?;
    day.iter().find_map(|o| {
        let at = o.causes.iter().find_map(|x| match x {
            Cause::Breakdown { bus, at, .. } if *bus == id => Some(*at),
            _ => None,
        });
        let lost = matches!(o.dropped, Some(DropWhy::BrokeDown | DropWhy::Crashed)) && o.bus == Some(id);
        at.or(lost.then_some(o.dep)).map(|at| (o.number.clone(), o.tour.clone(), at, o.driver))
    })
}

// --- advice --------------------------------------------------------------------------------------

/// Something to change against the late running, from its causes (the day's report, the
/// Operations tab).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Advice {
    /// More layover at `stop` on a line: before the trip at `at` of block `tour` (the worst), by
    /// `minutes`; `trips` trips carried a delay on from there.
    Layover { line: String, number: String, tour: String, stop: String, at: i32, minutes: i32, trips: u32 },
    /// More running time in the rush hours (`from`, `to`: hours) on a line.
    Running { line: String, number: String, tour: String, from: i32, to: i32, minutes: i32, trips: u32 },
    /// A relief too tight: the duty's first trip at `at`, its driver came `minutes` late.
    Relief { line: String, number: String, tour: String, duty: Option<usize>, at: i32, driver: u32, minutes: i32 },
    /// A driver late often: signed on late on `days` of the days kept, drove slowly on `slow`
    /// trips today (a course in punctuality, a talk, another duty).
    Driver { id: u32, days: u32, slow: u32 },
    /// A bus that held trips up: broke down, or in poor condition (a service).
    Bus { id: u32, breakdowns: u32, condition: f64, trips: u32 },
}

impl Advice {
    fn weight(&self) -> i64 {
        match self {
            Advice::Layover { minutes, trips, .. } | Advice::Running { minutes, trips, .. } => (*minutes as i64) * (*trips as i64) * 2,
            Advice::Relief { minutes, .. } => *minutes as i64 * 2,
            Advice::Driver { days, slow, .. } => *days as i64 * 4 + *slow as i64 * 2,
            Advice::Bus { trips, breakdowns, .. } => *trips as i64 * 3 + *breakdowns as i64,
        }
    }
}

/// The advice the most at the most.
pub const ADVICE_SHOWN: usize = 8;

/// What to change against the day's late running (`day`: a report's `Lateness`), the most
/// telling first.
pub fn advice(c: &Company, day: &Lateness) -> Vec<Advice> {
    let mut out: Vec<Advice> = Vec::new();
    let mut slow: Vec<(u32, u32)> = Vec::new();
    let mut held: Vec<(u32, u32)> = Vec::new();
    // (rush hours by line and hour: the trips and their traffic minutes)
    let mut rush: Vec<(String, String, String, i32, u32, i32)> = Vec::new();
    for o in &day.trips {
        for cause in &o.causes {
            match cause {
                Cause::Carried { stop, minutes, .. } => {
                    let found = out.iter_mut().find(|a| matches!(a, Advice::Layover { line, stop: s, .. } if *line == o.line && s == stop));
                    match found {
                        Some(Advice::Layover { at, tour, minutes: m, trips, .. }) => {
                            *trips += 1;
                            if *minutes > *m {
                                (*m, *at, *tour) = (*minutes, o.dep, o.tour.clone());
                            }
                        }
                        _ => out.push(Advice::Layover { line: o.line.clone(), number: o.number.clone(), tour: o.tour.clone(), stop: stop.clone(), at: o.dep, minutes: *minutes, trips: 1 }),
                    }
                }
                Cause::Traffic { peak: true, minutes } => {
                    let hour = o.dep.rem_euclid(1440) / 60;
                    match rush.iter_mut().find(|r| r.0 == o.line && r.3 == hour) {
                        Some(r) => (r.4, r.5) = (r.4 + 1, r.5 + minutes),
                        None => rush.push((o.line.clone(), o.number.clone(), o.tour.clone(), hour, 1, *minutes)),
                    }
                }
                Cause::Relief { driver, minutes, .. } if *minutes >= 2 => {
                    out.push(Advice::Relief { line: o.line.clone(), number: o.number.clone(), tour: o.tour.clone(), duty: o.duty, at: o.dep, driver: *driver, minutes: *minutes });
                }
                Cause::Driver { driver, .. } if *driver != AGENCY => match slow.iter_mut().find(|s| s.0 == *driver) {
                    Some(s) => s.1 += 1,
                    None => slow.push((*driver, 1)),
                },
                Cause::Breakdown { bus, .. } | Cause::Bus { bus, .. } => match held.iter_mut().find(|s| s.0 == *bus) {
                    Some(s) => s.1 += 1,
                    None => held.push((*bus, 1)),
                },
                _ => {}
            }
        }
    }
    for a in out.iter_mut() {
        if let Advice::Layover { minutes, .. } = a {
            *minutes = (*minutes).clamp(1, 10);
        }
    }
    out.retain(|a| !matches!(a, Advice::Layover { trips, minutes, .. } if *trips < 2 && *minutes < 3));
    // (the rush hours of a line, the hours next to each other together)
    rush.sort_by(|a, b| (a.0.as_str(), a.3).cmp(&(b.0.as_str(), b.3)));
    let mut i = 0;
    while i < rush.len() {
        let mut j = i;
        while j + 1 < rush.len() && rush[j + 1].0 == rush[i].0 && rush[j + 1].3 == rush[j].3 + 1 {
            j += 1;
        }
        let (trips, minutes): (u32, i32) = rush[i..=j].iter().fold((0, 0), |s, r| (s.0 + r.4, s.1 + r.5));
        if trips >= 2 {
            let avg = (minutes as f64 / trips as f64).ceil() as i32;
            out.push(Advice::Running { line: rush[i].0.clone(), number: rush[i].1.clone(), tour: rush[i].2.clone(), from: rush[i].3, to: rush[j].3 + 1, minutes: avg.clamp(1, 10), trips });
        }
        i = j + 1;
    }
    // (the drivers: late signing on over the days kept, slow today)
    let mut late_days: Vec<(u32, u32)> = Vec::new();
    for d in &c.lateness.days {
        let mut seen: Vec<u32> = Vec::new();
        for cause in d.trips.iter().flat_map(|o| o.causes.iter()) {
            if let Cause::SignOn { driver, .. } = cause {
                if *driver != AGENCY && !seen.contains(driver) {
                    seen.push(*driver);
                }
            }
        }
        for id in seen {
            match late_days.iter_mut().find(|x| x.0 == id) {
                Some(x) => x.1 += 1,
                None => late_days.push((id, 1)),
            }
        }
    }
    let mut people: Vec<u32> = late_days.iter().filter(|x| x.1 >= 2).map(|x| x.0).collect();
    people.extend(slow.iter().filter(|x| x.1 >= 3).map(|x| x.0));
    people.sort();
    people.dedup();
    for id in people.into_iter().filter(|id| c.employee(*id).is_some()) {
        let days = late_days.iter().find(|x| x.0 == id).map_or(0, |x| x.1);
        let s = slow.iter().find(|x| x.0 == id).map_or(0, |x| x.1);
        out.push(Advice::Driver { id, days, slow: s });
    }
    for (id, trips) in held {
        if let Some(v) = c.vehicle(id) {
            out.push(Advice::Bus { id, breakdowns: v.breakdowns, condition: v.condition, trips });
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.weight()));
    out.truncate(ADVICE_SHOWN);
    out
}

#[cfg(test)]
#[path = "lateness_tests.rs"]
pub(crate) mod tests;

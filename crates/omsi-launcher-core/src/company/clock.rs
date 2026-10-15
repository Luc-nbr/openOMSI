//! The company's clock: the company has a "now" - its date and the minute of that day - and
//! time is simulated step by step instead of a day being closed at once (Luc: simulate to the
//! next day, or two or three, with all that happens on the way, and also by hours and
//! minutes).
//!
//! Simulating moves the clock through an event timeline. A day's own events are written when
//! the clock first enters it (`script`, from the day's plan and its draws): the night's notes
//! told in the morning (the sick calls, a bus going to its service), the morning's
//! disruptions at their times, the tours leaving and coming back, the buses breaking down on
//! the way (`Broken`), building work and workshop jobs finishing in the afternoon of their
//! last day, the last day of someone leaving. Besides them come the tenders' auctions
//! (opening, the rivals' bids as they come, closing: `concessions`), and at midnight the day's
//! close (`day::close_day`, the month's bookings on a month's last day). Each event is
//! applied when its time comes and told in the feed; the important ones wait for the player's
//! decision (`Ask`) - or the dispatcher's, when the player left it to him or simulates
//! quickly.
//!
//! The day's money is still booked at its close (the modelled day is settled as a whole); a
//! breakdown's decision counts there: a rental bus ordered saves the trips after it arrives
//! (`breakdowns`).
//!
//! While the game runs on the company's map, the game's clock leads (`follow`): the company's
//! day goes along with it, and what the game reports of the company's buses comes in as it
//! comes (`tell_live`); it is booked at the day's close.
//!
//! The other parts of the company with things happening in its time hook in at `on_minute`
//! (called at every full hour and wherever a simulation stops).
//!
//! The company's time runs with the real clock (`RealTime`; Luc: "a real-time clock that runs
//! and is always in step with the real clock, so that progress - besides fast-forwarding - also
//! happens when openOMSI is not running"): from the moment it was last in step its course goes
//! on as the real time does. Opened again, the company catches up with its course
//! (`catch_up`: the days between closed, the dispatcher deciding); fast-forwarding moves it
//! ahead of the course, and the course goes on from there (`keep_lead`).

use super::concessions as cn;
use super::dates;
use super::day::{DayReport, LiveEvent, Note, Plan};
use super::depot::Area;
use super::economy;
use super::model::{Cents, Company};
use super::network;
use super::plan::{self, BusOf, DayPlan, Disruption, Who};
use super::rng::Rng;
use super::staff::StaffNote;
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// Minutes in a day.
pub const DAY: i64 = 1440;
/// The morning "to the morning" goes to.
pub const MORNING: i64 = 5 * 60;
/// How many of the feed's lines are kept.
pub const FEED_KEPT: usize = 300;
/// Minutes until a rental bus ordered for a broken-down one takes over its trips.
pub const RENTAL_MINUTES: i32 = 45;
/// Minutes until a spare bus of the fleet sent from the depot takes over a broken-down one's
/// trips.
pub const SPARE_MINUTES: i32 = 30;
/// When building work and workshop jobs are finished on their last day, and someone leaving
/// has their last hour.
pub const BUILT_AT: i64 = 16 * 60;
pub const JOB_DONE_AT: i64 = 15 * 60 + 30;
pub const LAST_DAY_AT: i64 = 17 * 60;

// --- the time -------------------------------------------------------------------------------

/// The company's now: minutes since 1970-01-01 00:00 of its calendar.
pub fn now(c: &Company) -> i64 {
    moment(&c.date, c.clock.minute as i64)
}

/// A date's minute as the clock counts.
pub fn moment(date: &str, minute: i64) -> i64 {
    dates::parse(date).unwrap_or(0) * DAY + minute
}

/// The first day the company may be moved to (`move_to`): its last day booked or closed
/// (None: any).
pub fn earliest_date(c: &Company) -> Option<String> {
    c.ledger.iter().map(|b| b.date.as_str()).chain(c.history.iter().map(|h| h.date.as_str())).filter_map(dates::parse).max().map(dates::fmt)
}

/// Move the company to another day (the company's settings): its clock to that day's
/// midnight, the day's events made anew. Nothing between is simulated - the days skipped bring
/// no money and no wear - and offers, deliveries and contracts keep their own dates. Back only
/// as far as its last day booked or closed: the books would not add up before it.
pub fn move_to(c: &mut Company, date: &str) -> Result<(), &'static str> {
    let Some(d) = dates::parse(date) else { return Err("That is no date.") };
    if earliest_date(c).and_then(|e| dates::parse(&e)).is_some_and(|e| d < e) {
        return Err("The company's books go further: it cannot go back before its last booking.");
    }
    // (its own day: the clock stays where it is, the day is not run again)
    if dates::fmt(d) == c.date {
        return Ok(());
    }
    c.date = dates::fmt(d);
    c.clock.minute = 0;
    c.clock.today = None;
    c.clock.ask = None;
    c.clock.target = None;
    // (in step with the real clock again from the new day, without a lead)
    c.clock.real.synced = None;
    c.clock.real.lead = 0;
    Ok(())
}

pub fn date_of(m: i64) -> String {
    dates::fmt(m.div_euclid(DAY))
}

pub fn minute_of(m: i64) -> i64 {
    m.rem_euclid(DAY)
}

/// "08:12".
pub fn hhmm(m: i64) -> String {
    let x = minute_of(m);
    format!("{:02}:{:02}", x / 60, x % 60)
}

/// What the pages offer to simulate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Minutes(i64),
    /// To the next 05:00.
    Morning,
    /// To the next midnight: the day is closed ("Simulate to tomorrow").
    Midnight,
    Days(i64),
    /// To the next time it is this minute of the day.
    Until(i64),
}

/// Where a step from now goes.
pub fn target(c: &Company, step: Step) -> i64 {
    let now = now(c);
    let next = |m: i64| {
        let at = now.div_euclid(DAY) * DAY + m.rem_euclid(DAY);
        if at > now {
            at
        } else {
            at + DAY
        }
    };
    match step {
        Step::Minutes(n) => now + n.max(1),
        Step::Morning => next(MORNING),
        Step::Midnight => (now.div_euclid(DAY) + 1) * DAY,
        Step::Days(n) => now + n.max(1) * DAY,
        Step::Until(m) => next(m),
    }
}

// --- what the company keeps -------------------------------------------------------------------

/// The clock as the company keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Clock {
    /// Minutes into the company's day (`Company::date`; 0 = its midnight, the day before it
    /// closed).
    #[serde(default)]
    pub minute: u32,
    /// What happened, the newest last.
    #[serde(default)]
    pub feed: Vec<FeedItem>,
    /// Today's own events (written when the clock entered the day).
    #[serde(default)]
    pub today: Option<DayScript>,
    /// Something waits for the player's decision; the simulation stopped there, on its way to
    /// `target`.
    #[serde(default)]
    pub ask: Option<Ask>,
    #[serde(default)]
    pub target: Option<i64>,
    /// The dispatcher decides breakdowns by himself (the simulation does not wait for them).
    #[serde(default)]
    pub dispatcher: bool,
    /// The company's time against the real clock.
    #[serde(default)]
    pub real: RealTime,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// The ordinary run of the day (a tour leaving).
    #[default]
    Minor,
    Info,
    Good,
    Warn,
    Bad,
}

/// A line of the feed: its text as an English key with `%{name}` places, and what goes in
/// them ("amount" in cents and "date" are written as the page writes money and days).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FeedItem {
    pub at: i64,
    pub text: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
    #[serde(default)]
    pub level: Level,
}

/// A day's own events.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DayScript {
    pub date: String,
    pub events: Vec<Timed>,
    /// The buses that break down on their tours today (the day's close takes them from here).
    #[serde(default)]
    pub breaks: Vec<Broken>,
    /// When each of the fleet's buses is out on its tours today (id, from, to): the spare buses
    /// for a breakdown are those not out then.
    #[serde(default)]
    pub busy: Vec<(u32, i32, i32)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Timed {
    /// The minute of the day.
    pub at: i64,
    pub what: What,
    #[serde(default)]
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum What {
    Out { number: String, tour: String, bus: String, driver: String },
    In { number: String, tour: String },
    NotRunning { number: String, tour: String, why: String },
    Late { name: String, minutes: i32, number: String, tour: String },
    NoStart { bus: String, cost: Cents },
    /// What fell out in the morning, for the dispatcher (the planning).
    Morning { count: usize },
    Breakdown { index: usize },
    Built { area: String, level: u32 },
    JobDone { bus: String, job: String },
    LastDay { name: String },
    /// The night's notes, told in the morning.
    Told { text: String, args: Vec<(String, String)>, level: Level },
}

/// A bus that breaks down on its tour, and what was decided for its trips.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Broken {
    pub vehicle: u32,
    pub bus: String,
    pub line: String,
    pub at: i32,
    /// When its last tour of the day ends, and that line's timetable name (for the line's own
    /// buses): what a spare bus has to be free for.
    #[serde(default)]
    pub until: i32,
    #[serde(default)]
    pub line_name: String,
    #[serde(default)]
    pub choice: Option<Choice>,
    /// What the rental bus ordered for it costs (paid with the day, at its close).
    #[serde(default)]
    pub cost: Cents,
    /// It had an accident (damage to repair), not a breakdown.
    #[serde(default)]
    pub accident: bool,
    /// When the bus sent for it takes over its trips (a spare or a rental bus; None in an older
    /// save: `at` and the choice's minutes).
    #[serde(default)]
    pub back: Option<i32>,
    /// Until when the spare bus sent can run them: it goes out on its own tour then (None: for
    /// the rest of the day).
    #[serde(default)]
    pub spare_until: Option<i32>,
}

/// A spare bus of the fleet free to take over a broken-down bus's trips, and until when (its
/// own tour after; None: for the rest of the broken bus's day).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpareBus {
    pub vehicle: u32,
    pub until: Option<i32>,
}

/// A bus cut short today, as the day's close counts its trips (`breakdowns`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub vehicle: u32,
    /// When it broke down or crashed.
    pub at: i32,
    /// From when the bus sent runs its trips, and until when (a spare bus going out on its own
    /// tour; None: the rest of the day).
    pub back: Option<i32>,
    pub until: Option<i32>,
    pub accident: bool,
}

impl Cut {
    /// A trip leaving at `dep` is run by the bus sent for it.
    pub fn covered(&self, dep: i32) -> bool {
        self.back.is_some_and(|b| dep >= b) && self.until.is_none_or(|u| dep < u)
    }

    /// A trip leaving at `dep` is lost: after the breakdown, and no bus sent ran it.
    pub fn lost(&self, dep: i32) -> bool {
        dep >= self.at && !self.covered(dep)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// A rental bus takes over its trips after `RENTAL_MINUTES`.
    Rental,
    /// Its trips after the breakdown are dropped.
    Drop,
    /// A spare bus of the fleet takes over its trips after `SPARE_MINUTES` (testers: "when a
    /// bus breaks down it does not give me the option to dispatch one of my spare buses").
    Spare { vehicle: u32 },
}

/// What waits for the player.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ask {
    /// Today's `breaks[index]`.
    Breakdown { index: usize },
    /// A tender opened, or the player's bid was beaten.
    Tender { id: u32, outbid: bool },
    /// The morning's disruptions (the planning has them).
    Morning { count: usize },
    /// Something another part of the company tells (`on_minute`).
    Notice { text: String, args: Vec<(String, String)> },
}

/// What a simulation came to.
#[derive(Clone, Debug, Default)]
pub struct Run {
    /// The days closed on the way.
    pub reports: Vec<DayReport>,
    /// It stopped for a decision (`Clock::ask`).
    pub stopped: bool,
    /// Lines added to the feed.
    pub told: usize,
    /// The breakdowns and accidents decided on the way (by the dispatcher, without asking), as
    /// the inbox tells them: the pages say them where the player is.
    pub breaks: Vec<super::inbox::Message>,
}

/// What the simulation needs from outside: the timetable and the night.
pub trait World {
    /// The company map's lines on a date.
    fn lines(&mut self, c: &Company, date: &str) -> Result<Vec<LineInfo>, String>;
    /// The night after the company's day (`store::close_day`'s work): the date moves on.
    fn close_day(&mut self, c: &mut Company, lines: &[LineInfo]) -> Result<DayReport, String>;
}

fn arg(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn tell(c: &mut Company, run: &mut Run, at: i64, text: &str, args: Vec<(String, String)>, level: Level) {
    c.clock.feed.push(FeedItem { at, text: text.to_string(), args, level });
    run.told += 1;
    if c.clock.feed.len() > FEED_KEPT {
        let extra = c.clock.feed.len() - FEED_KEPT;
        c.clock.feed.drain(..extra);
    }
}

// --- a day's events ---------------------------------------------------------------------------

/// The buses of a day's tours with the span they are out (id, from, to).
fn spans(tours: impl Iterator<Item = (u32, i32, i32)>) -> Vec<(u32, i32, i32)> {
    let mut out: Vec<(u32, i32, i32)> = Vec::new();
    for (id, a, b) in tours {
        match out.iter_mut().find(|s| s.0 == id) {
            Some(s) => {
                s.1 = s.1.min(a);
                s.2 = s.2.max(b);
            }
            None => out.push((id, a, b)),
        }
    }
    out.sort_by_key(|s| s.0);
    out
}

/// The buses that break down on their way (drawn from the company, the date and the bus: the
/// same day draws the same), and when.
pub fn roll_breakdowns(c: &Company, date: &str, spans: &[(u32, i32, i32)]) -> Vec<(u32, i32)> {
    let r = economy::rules(c.game_mode);
    let day = dates::parse(date).unwrap_or(0);
    let mut out = Vec::new();
    for &(id, a, b) in spans {
        let Some(v) = c.vehicle(id) else { continue };
        let overdue = if v.km > v.next_service_km { 2.0 } else { 1.0 };
        let p = 0.004 * r.breakdown_factor * c.game_mode.rules().breakdowns * (1.0 + (100.0 - v.condition) / 25.0) * overdue;
        let mut rng = Rng::of(&[&c.id, "breakdown", &id.to_string()], day);
        if rng.chance(p) {
            out.push((id, rng.int(a as i64, b.max(a) as i64) as i32));
        }
    }
    out
}

/// Of a day's accidents, the share serious enough to take the bus off the road (the rest are
/// the scrapes `incidents` books at the day's close).
const SERIOUS: f64 = 0.25;

/// The buses that have an accident bad enough to take them off the road on their way (drawn
/// as the breakdowns are; testers: "when a bus has a breakdown or accident ... it does not
/// inform me that I can allocate" a spare bus): more often with drivers who drive worse,
/// half as often after the defensive driving course (`incidents::chances`). Not the buses in
/// `skip` (they break down today).
fn roll_accidents(c: &Company, dp: &DayPlan, spans: &[(u32, i32, i32)], skip: &[u32]) -> Vec<(u32, i32)> {
    let day = dates::parse(&dp.date).unwrap_or(0);
    let trained = super::incidents::trained(c);
    let mut out = Vec::new();
    for &(id, a, b) in spans.iter().filter(|s| !skip.contains(&s.0)) {
        // (the chance of its day: each trip by the driver who drives it)
        let mut p = 0.0;
        for t in dp.tours.iter().filter(|t| t.bus == Some(BusOf::Own(id))) {
            for d in &t.duties {
                let e = match d.who {
                    Some(Who::Staff(e)) => c.employee(e).cloned(),
                    Some(Who::Agency) => Some(plan::agency_driver()),
                    _ => None,
                };
                let Some(e) = e else { continue };
                let trips = t.tour.trips[d.start..d.end].iter().filter(|x| x.counts()).count() as f64;
                p += trips * super::incidents::chances(&e, &trained.get(&e.id).copied().unwrap_or_default(), 30).0;
            }
        }
        let p = p * SERIOUS * c.game_mode.rules().breakdowns;
        let mut rng = Rng::of(&[&c.id, "crash", &id.to_string()], day);
        if rng.chance(p) {
            out.push((id, rng.int(a as i64, b.max(a) as i64) as i32));
        }
    }
    out
}

/// The breakdowns and accidents the day's close counts, and what took over their trips:
/// today's script's when the clock went through the day, else drawn as it would have; and the
/// buses the game reported broken down.
pub fn breakdowns(c: &Company, plan: &Plan, date: &str, live_broken: &[u32]) -> Vec<Cut> {
    let sp = spans(plan.tours.iter().filter_map(|t| Some((t.bus?, t.tour.from(), t.tour.to()))));
    let mut out: Vec<Cut> = match c.clock.today.as_ref().filter(|s| s.date == date) {
        Some(s) => s
            .breaks
            .iter()
            .map(|b| {
                // (an older save has no `back`: from the breakdown, as the choice came then)
                let back = match b.choice {
                    Some(Choice::Rental) => Some(b.back.unwrap_or(b.at + RENTAL_MINUTES)),
                    Some(Choice::Spare { .. }) => Some(b.back.unwrap_or(b.at + SPARE_MINUTES)),
                    _ => None,
                };
                let until = if matches!(b.choice, Some(Choice::Spare { .. })) { b.spare_until } else { None };
                Cut { vehicle: b.vehicle, at: b.at, back, until, accident: b.accident }
            })
            .collect(),
        None => roll_breakdowns(c, date, &sp).into_iter().map(|(vehicle, at)| Cut { vehicle, at, back: None, until: None, accident: false }).collect(),
    };
    let day = dates::parse(date).unwrap_or(0);
    for id in live_broken {
        if out.iter().any(|x| x.vehicle == *id) {
            continue;
        }
        let (a, b) = sp.iter().find(|s| s.0 == *id).map(|s| (s.1, s.2)).unwrap_or((6 * 60, 18 * 60));
        let mut rng = Rng::of(&[&c.id, "live-breakdown", &id.to_string()], day);
        out.push(Cut { vehicle: *id, at: rng.int(a as i64, b.max(a) as i64) as i32, back: None, until: None, accident: false });
    }
    out
}

fn bus_label(c: &Company, b: Option<BusOf>) -> String {
    match b {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(),
        Some(BusOf::Rental) => "rental".into(),
        None => String::new(),
    }
}

fn who_label(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_default(),
        Some(Who::Agency) => "agency".into(),
        Some(Who::Player) => "you".into(),
        Some(Who::Guest(g)) => super::together::guest_name(c, g),
        None => String::new(),
    }
}

/// The night's notes as the morning tells them (when, what, how much it matters).
fn told_notes(c: &Company, date: &str) -> Vec<(i64, String, Vec<(String, String)>, Level)> {
    let mut out = Vec::new();
    let Some(r) = c.last_report.as_ref().filter(|r| dates::add(&r.date, 1) == date) else { return out };
    for n in &r.notes {
        let x = match n {
            Note::Service { number } => (0, "Bus %{bus} goes to its service today", vec![arg("bus", number)], Level::Info),
            Note::Returned { number, name } => (0, "%{bus} %{name} went back", vec![arg("bus", number), arg("name", name)], Level::Info),
            Note::Month { month, result } => (0, "The month %{month} was closed: %{amount}", vec![arg("month", month), arg("amount", result)], if *result >= 0 { Level::Good } else { Level::Warn }),
            Note::Built { area } => (0, "Building work finished: %{area}", vec![arg("area", area)], Level::Good),
            Note::BayWait { number } => (6 * 60, "Bus %{bus} waits for a free workshop bay", vec![arg("bus", number)], Level::Warn),
            Note::Won { number, until } => (0, "The concession of line %{n} runs until %{date}", vec![arg("n", number), arg("date", until)], Level::Good),
            Note::Ended { number } => (0, "Line %{n} is no longer the company's: its concession ended", vec![arg("n", number)], Level::Bad),
            Note::NotStarted { number, charge, .. } => (0, "Line %{n} is not in service yet: the authority charged %{amount}", vec![arg("n", number), arg("amount", charge)], Level::Bad),
            Note::Accident { number, cost } => (0, "Bus %{bus} had an accident yesterday: damage %{amount}", vec![arg("bus", number), arg("amount", cost)], Level::Bad),
            Note::Breakdown { .. } | Note::LoanPaid { .. } | Note::Lost { .. } | Note::Unsaved => continue,
        };
        out.push((x.0, x.1.to_string(), x.2, x.3));
    }
    for s in &r.staff {
        let x = match s {
            StaffNote::Sick { name, until } => (5 * 60 + 30, "%{name} called in sick, until %{date}", vec![arg("name", name), arg("date", until)], Level::Warn),
            StaffNote::Holiday { name, until } => (0, "%{name} is on holiday until %{date}", vec![arg("name", name), arg("date", until)], Level::Minor),
            StaffNote::Resigned { name, until } => (9 * 60, "%{name} handed in their notice: leaves on %{date}", vec![arg("name", name), arg("date", until)], Level::Warn),
            StaffNote::Unhappy { name } => (10 * 60, "%{name} is unhappy", vec![arg("name", name)], Level::Warn),
            StaffNote::Left { name } => (0, "%{name} left the company", vec![arg("name", name)], Level::Info),
        };
        out.push((x.0, x.1.to_string(), x.2, x.3));
    }
    out
}

/// A day's own events, from its plan (`plan::day_plan` of the company's day).
pub fn script(c: &Company, dp: &DayPlan) -> DayScript {
    let date = dp.date.clone();
    let mut ev: Vec<Timed> = Vec::new();
    let mut push = |at: i64, what: What| ev.push(Timed { at: at.clamp(0, DAY - 1), what, done: false });
    for (at, text, args, level) in told_notes(c, &date) {
        push(at, What::Told { text, args, level });
    }
    // the morning
    let first = dp.tours.iter().map(|t| t.tour.from() as i64).min().unwrap_or(6 * 60);
    let mut morning: Option<i64> = None;
    // (what is told of it: someone late without a duty today keeps nobody waiting)
    let mut count = 0;
    for d in &dp.disruptions {
        match *d {
            Disruption::Late { employee, minutes } => {
                let duty = dp.tours.iter().flat_map(|t| t.duties.iter().map(move |d| (t, d))).filter(|(_, d)| d.who == Some(Who::Staff(employee))).min_by_key(|(_, d)| d.from);
                let name = c.employee(employee).map(|e| e.name.clone()).unwrap_or_default();
                let (at, number, tour) = match duty {
                    Some((t, d)) => (d.from as i64, t.tour.number.clone(), t.tour.tour.clone()),
                    None => continue,
                };
                morning = Some(morning.map_or(at, |m| m.min(at)));
                push(at, What::Late { name, minutes, number, tour });
                count += 1;
            }
            Disruption::Breakdown { vehicle, cost } => {
                let at = (first - 30).max(4 * 60);
                morning = Some(morning.map_or(at, |m| m.min(at)));
                push(at, What::NoStart { bus: c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default(), cost });
                count += 1;
            }
        }
    }
    // (the duties of drivers who called in sick wait for the dispatcher too: the player chooses
    // how to cover them)
    let sick = dp.tours.iter().filter(|t| !t.by_player).flat_map(|t| t.duties.iter()).filter(|d| d.problem == Some(super::plan::Problem::Sick)).count();
    if sick > 0 {
        let at = (first - 30).max(4 * 60);
        morning = Some(morning.map_or(at, |m| m.min(at)));
        count += sick;
    }
    if let Some(at) = morning {
        push(at, What::Morning { count });
    }
    // the tours
    for t in dp.tours.iter().filter(|t| !t.by_player) {
        let (number, tour) = (t.tour.number.clone(), t.tour.tour.clone());
        if t.covered() {
            push(t.tour.from() as i64, What::Out { number: number.clone(), tour: tour.clone(), bus: bus_label(c, t.bus), driver: who_label(c, t.duties.first().and_then(|d| d.who)) });
            push(t.tour.to() as i64, What::In { number, tour });
        } else {
            let why = t.bus_problem.or_else(|| t.duties.iter().find_map(|d| d.problem)).map(|p| p.label()).unwrap_or(if t.bus.is_none() { "No bus" } else { "No driver" });
            push(t.tour.from() as i64, What::NotRunning { number, tour, why: why.to_string() });
        }
    }
    // the breakdowns on the way
    let sp = spans(dp.tours.iter().filter(|t| t.covered()).filter_map(|t| match t.bus {
        Some(BusOf::Own(id)) => Some((id, t.tour.from(), t.tour.to())),
        _ => None,
    }));
    let mut breaks = Vec::new();
    // (and the accidents bad enough to take a bus off the road: the same choices)
    let broken = roll_breakdowns(c, &date, &sp);
    let skip: Vec<u32> = broken.iter().map(|x| x.0).collect();
    let crashed = roll_accidents(c, dp, &sp, &skip);
    for (accident, (vehicle, at)) in broken.into_iter().map(|x| (false, x)).chain(crashed.into_iter().map(|x| (true, x))) {
        let line = dp.tours.iter().filter(|t| t.bus == Some(BusOf::Own(vehicle))).find(|t| t.tour.from() <= at && at <= t.tour.to()).or_else(|| dp.tours.iter().find(|t| t.bus == Some(BusOf::Own(vehicle)))).map(|t| t.tour.number.clone()).unwrap_or_default();
        let bus = c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default();
        let mine = dp.tours.iter().filter(|t| t.bus == Some(BusOf::Own(vehicle)));
        let until = mine.clone().map(|t| t.tour.to()).max().unwrap_or(at).max(at);
        let line_name = mine.clone().find(|t| t.tour.from() <= at && at <= t.tour.to()).or_else(|| mine.clone().next()).map(|t| t.tour.line.clone()).unwrap_or_default();
        breaks.push(Broken { vehicle, bus, line, at, until, line_name, choice: None, cost: 0, accident, back: None, spare_until: None });
        let index = breaks.len() - 1;
        push(at as i64, What::Breakdown { index });
    }
    // the depot, the people
    for w in c.site.works.iter().filter(|w| w.until == date) {
        push(BUILT_AT, What::Built { area: w.area.key().to_string(), level: w.level });
    }
    for j in c.site.jobs.iter().filter(|j| j.until.as_deref() == Some(date.as_str())) {
        push(JOB_DONE_AT, What::JobDone { bus: c.vehicle(j.vehicle).map(|v| v.number.clone()).unwrap_or_default(), job: j.kind.label().to_string() });
    }
    for e in c.staff.iter().filter(|e| e.notice_until.as_deref() == Some(date.as_str())) {
        push(LAST_DAY_AT, What::LastDay { name: e.name.clone() });
    }
    ev.sort_by_key(|e| e.at);
    let busy = dp
        .tours
        .iter()
        .filter(|t| t.covered())
        .filter_map(|t| match t.bus {
            Some(BusOf::Own(id)) => Some((id, t.tour.from(), t.tour.to())),
            _ => None,
        })
        .collect();
    DayScript { date, events: ev, breaks, busy }
}

/// The company enters its day: the timetable read, the tender market refreshed, in the easy
/// mode the company's own morning (`autopilot::morning`: its lines in service, the roster
/// filled), in Normal the roster repaired (`plan::repair_day`), the day's events written.
fn begin_day(c: &mut Company, w: &mut dyn World) -> Result<(), String> {
    if c.clock.today.as_ref().is_some_and(|s| s.date == c.date) {
        return Ok(());
    }
    let date = c.date.clone();
    let lines = w.lines(c, &date)?;
    cn::refresh(c, &lines);
    if super::autopilot::on(c) {
        super::autopilot::morning(c, &date, &lines);
    }
    let tours = network::in_service_only(network::tours_of_day(c, &lines, &date));
    if c.game_mode.rules().repair_roster {
        let r = plan::repair_day(c, &date, tours.clone());
        plan::tell_repaired(c, &r);
    }
    let dp = plan::day_plan(c, &date, tours, &[], &[], false);
    // (when the late drivers arrive, kept: moved to a duty that begins later, they are late
    // for it only when it begins before)
    plan::pin_arrivals(c, &dp);
    c.clock.today = Some(script(c, &dp));
    Ok(())
}

// --- what comes next, and its doing -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Due {
    Script(usize),
    Open(u32),
    Bid(u32),
    Close(u32),
}

/// The first thing due up to `limit` (with things of the past not done yet first).
fn next_due(c: &Company, limit: i64) -> Option<(i64, Due)> {
    let day0 = moment(&c.date, 0);
    let mut best: Option<(i64, Due)> = None;
    let mut consider = |at: i64, d: Due| {
        if at <= limit && best.is_none_or(|b| at < b.0) {
            best = Some((at, d));
        }
    };
    if let Some(s) = c.clock.today.as_ref().filter(|s| s.date == c.date) {
        if let Some((i, e)) = s.events.iter().enumerate().find(|(_, e)| !e.done) {
            consider(day0 + e.at, Due::Script(i));
        }
    }
    for t in c.concessions.tenders.iter().filter(|t| t.open() && t.closes_at > 0) {
        if !t.opened {
            consider(t.opens_at, Due::Open(t.id));
            continue;
        }
        if let Some(p) = cn::bids(c, t, t.closes_at).get(t.told) {
            consider(p.at, Due::Bid(t.id));
        }
        consider(t.closes_at, Due::Close(t.id));
    }
    best
}

/// The spare bus sent for a broken-down bus today, if one was (`Choice::Spare`): the day's
/// close runs its trips after `SPARE_MINUTES` with it.
pub fn spare_of(c: &Company, date: &str, vehicle: u32) -> Option<u32> {
    c.clock.today.as_ref().filter(|s| s.date == date)?.breaks.iter().find(|b| b.vehicle == vehicle).and_then(|b| match b.choice {
        Some(Choice::Spare { vehicle }) => Some(vehicle),
        _ => None,
    })
}

/// The minute a decision for today's `b` takes effect from: the breakdown's, or the clock's
/// now when it is decided later the same day (a spare bus sent for trips dropped).
fn decided_at(c: &Company, s: &DayScript, b: &Broken) -> i32 {
    if s.date == c.date {
        b.at.max(c.clock.minute as i32)
    } else {
        b.at
    }
}

/// When a spare bus and a rental bus sent now for today's `breaks[index]` take over its trips.
pub fn arrivals(c: &Company, index: usize) -> (i32, i32) {
    let Some(s) = c.clock.today.as_ref() else { return (0, 0) };
    let Some(b) = s.breaks.get(index) else { return (0, 0) };
    let start = decided_at(c, s, b);
    (start + SPARE_MINUTES, start + RENTAL_MINUTES)
}

/// How long a spare bus has to be able to run at least to be worth sending (minutes).
const SPARE_LEAST: i32 = 30;

/// The fleet's buses free to take over today's `breaks[index]` (a spare bus): fit to drive -
/// not in the workshop, not broken down today -, one of the line's buses, not out on a tour
/// now nor going out before it could be of use, and not sent for another breakdown then. A bus
/// out on its own tour later takes the trips over until it has to go (testers: "I have spare
/// vehicles that it does not inform me that I can allocate" - a bus free all morning was no
/// spare when it drove in the evening). Those free for the rest of the day first, the same size
/// first, then the longest free, then the best kept.
pub fn spare_offers(c: &Company, index: usize) -> Vec<SpareBus> {
    let Some(s) = c.clock.today.as_ref() else { return Vec::new() };
    let Some(b) = s.breaks.get(index) else { return Vec::new() };
    let start = decided_at(c, s, b);
    let arrive = start + SPARE_MINUTES;
    let end = b.until.max(arrive);
    let morning: Vec<u32> = plan::disruptions(c, &s.date).iter().filter_map(|d| if let Disruption::Breakdown { vehicle, .. } = d { Some(*vehicle) } else { None }).collect();
    let kind = c.vehicle(b.vehicle).map(|v| v.kind.size);
    let margin = super::staff::BUS_MARGIN;
    // (the spare buses sent for the other breakdowns, and when they run for them)
    let sent: Vec<(u32, i32, i32)> = s
        .breaks
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .filter_map(|(_, x)| match x.choice {
            Some(Choice::Spare { vehicle }) => Some((vehicle, x.back.unwrap_or(x.at + SPARE_MINUTES), x.spare_until.unwrap_or(x.until.max(x.at + SPARE_MINUTES)))),
            _ => None,
        })
        .collect();
    let mut out: Vec<(SpareBus, &super::model::Vehicle)> = Vec::new();
    for v in c.fleet.iter() {
        if v.id == b.vehicle || !v.held_on(&s.date) || v.in_workshop(&s.date) || v.condition < 20.0 || morning.contains(&v.id) {
            continue;
        }
        if s.breaks.iter().any(|x| x.vehicle == v.id && x.at <= end) {
            continue;
        }
        if !(b.line_name.is_empty() || super::ownline::line_allows(c, &b.line_name, v)) || !super::vehicles::vehicle_allowed(c, v) {
            continue;
        }
        let tours: Vec<(i32, i32)> = s.busy.iter().filter(|x| x.0 == v.id).map(|x| (x.1, x.2)).collect();
        // (out on a tour now, or going out before it could run a trip of it)
        if tours.iter().any(|&(a, z)| a < arrive + SPARE_LEAST + margin && start < z + margin) {
            continue;
        }
        let until = tours.iter().map(|x| x.0).filter(|&a| a < end + margin).min().map(|a| a - margin);
        let last = until.unwrap_or(end);
        if sent.iter().any(|&(id, from, to)| id == v.id && from < last && arrive < to) {
            continue;
        }
        out.push((SpareBus { vehicle: v.id, until }, v));
    }
    out.sort_by(|(x, xv), (y, yv)| {
        x.until
            .is_some()
            .cmp(&y.until.is_some())
            .then((Some(yv.kind.size) == kind).cmp(&(Some(xv.kind.size) == kind)))
            .then(y.until.unwrap_or(i32::MAX).cmp(&x.until.unwrap_or(i32::MAX)))
            .then(yv.condition.partial_cmp(&xv.condition).unwrap_or(std::cmp::Ordering::Equal))
            .then(xv.id.cmp(&yv.id))
    });
    out.into_iter().map(|x| x.0).collect()
}

/// The buses of `spare_offers`.
pub fn spare_buses(c: &Company, index: usize) -> Vec<u32> {
    spare_offers(c, index).into_iter().map(|x| x.vehicle).collect()
}

/// The dispatcher's choice for a breakdown: a spare bus of the fleet free for the rest of the
/// day, else a rental bus while the cash allows, else a spare bus for as long as it is free.
fn dispatcher_choice(c: &Company, index: usize) -> Choice {
    let offers = spare_offers(c, index);
    if let Some(x) = offers.iter().find(|x| x.until.is_none()) {
        return Choice::Spare { vehicle: x.vehicle };
    }
    let cost = rental_cost(c, index);
    if c.cash >= cost * 5 {
        return Choice::Rental;
    }
    match offers.first() {
        Some(x) => Choice::Spare { vehicle: x.vehicle },
        None => Choice::Drop,
    }
}

/// What a rental bus for the rest of a broken-down bus's day costs.
pub fn rental_cost(c: &Company, index: usize) -> Cents {
    let r = economy::rules(c.game_mode);
    let kind = c.clock.today.as_ref().and_then(|s| s.breaks.get(index)).and_then(|b| c.vehicle(b.vehicle)).map(|v| v.kind).unwrap_or_default();
    economy::rent_per_day(kind, &r, c.price_index)
}

/// Decide a breakdown: a spare bus of the fleet sent, a rental bus ordered (paid with the day,
/// `day::close_day`), or its trips dropped. Trips dropped can still get a spare or a rental bus
/// later the same day, from then on (the Roster tab). The bus sent takes over from
/// `Broken::back`, which the day's close keeps to.
pub fn decide(c: &mut Company, index: usize, choice: Choice, run: &mut Run) {
    let at = now(c);
    let Some(s) = c.clock.today.as_ref() else { return };
    let Some(b) = s.breaks.get(index).cloned() else { return };
    match b.choice {
        None => {}
        Some(Choice::Drop) if choice != Choice::Drop => {}
        _ => return,
    }
    let start = decided_at(c, s, &b);
    let (mut cost, mut back, mut until) = (0, None, None);
    match choice {
        Choice::Rental => {
            cost = rental_cost(c, index);
            back = Some(start + RENTAL_MINUTES);
            tell(c, run, at, "A rental bus takes over the trips of bus %{bus} on line %{n} from %{time}", vec![arg("bus", &b.bus), arg("n", &b.line), arg("time", hhmm(moment(&c.date, (start + RENTAL_MINUTES) as i64))), arg("amount", cost)], Level::Info);
        }
        Choice::Drop => tell(c, run, at, "The trips of bus %{bus} on line %{n} are dropped for the rest of the day", vec![arg("bus", &b.bus), arg("n", &b.line)], Level::Warn),
        Choice::Spare { vehicle } => {
            // (a bus no longer free is not sent: the dispatcher chooses then)
            let Some(offer) = spare_offers(c, index).into_iter().find(|x| x.vehicle == vehicle) else {
                let other = dispatcher_choice(c, index);
                if other != choice {
                    decide(c, index, other, run);
                }
                return;
            };
            back = Some(start + SPARE_MINUTES);
            until = offer.until;
            let spare = c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default();
            let from = hhmm(moment(&c.date, (start + SPARE_MINUTES) as i64));
            match until {
                Some(u) => tell(c, run, at, "Spare bus %{spare} takes over the trips of bus %{bus} on line %{n} from %{time} until %{until}", vec![arg("spare", spare), arg("bus", &b.bus), arg("n", &b.line), arg("time", from), arg("until", hhmm(moment(&c.date, u as i64)))], Level::Info),
                None => tell(c, run, at, "Spare bus %{spare} takes over the trips of bus %{bus} on line %{n} from %{time}", vec![arg("spare", spare), arg("bus", &b.bus), arg("n", &b.line), arg("time", from)], Level::Info),
            }
        }
    }
    if let Some(x) = c.clock.today.as_mut().and_then(|s| s.breaks.get_mut(index)) {
        x.choice = Some(choice);
        x.cost = cost;
        x.back = back;
        x.spare_until = until;
    }
    post_break(c, index);
    run.breaks.extend(break_message(c, index));
}

// --- what is told of a breakdown ----------------------------------------------------------------

// (read back by the agenda, `agenda::breakdowns`)
pub(super) const BROKE_TITLE: &str = "Bus %{bus} broke down";
pub(super) const CRASH_TITLE: &str = "Bus %{bus} had an accident";
const SPARES_FREE: &str = "On line %{n} at %{time}. Spare buses free now: %{spares}.";
const NO_SPARE: &str = "On line %{n} at %{time}. No spare bus is free now: a rental bus can take over its trips from %{arrive} for %{amount}, or they are dropped.";
pub(super) const BY_SPARE: &str = "On line %{n} at %{time}. Spare bus %{spare} takes over its trips from %{arrive}.";
pub(super) const BY_SPARE_UNTIL: &str = "On line %{n} at %{time}. Spare bus %{spare} takes over its trips from %{arrive} until %{leaves}, when it goes out on its own tour.";
pub(super) const BY_RENTAL: &str = "On line %{n} at %{time}. A rental bus takes over its trips from %{arrive} for %{amount}.";
pub(super) const DROPPED: &str = "On line %{n} at %{time}. Its trips are dropped for the rest of the day.";

/// Today's `breaks[index]` as the inbox tells it (and the pages' notices): what happened, and
/// the spare buses free or what was decided.
pub fn break_message(c: &Company, index: usize) -> Option<super::inbox::Message> {
    use super::inbox::{Kind, Message};
    let s = c.clock.today.as_ref()?;
    let b = s.breaks.get(index)?;
    let m = |t: i32| hhmm(moment(&s.date, t as i64));
    let number = |id: u32| c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default();
    let offers = if b.choice.is_none() { spare_offers(c, index) } else { Vec::new() };
    let (text, level) = match b.choice {
        None if offers.is_empty() => (NO_SPARE, Level::Bad),
        None => (SPARES_FREE, Level::Bad),
        Some(Choice::Spare { .. }) if b.spare_until.is_some() => (BY_SPARE_UNTIL, Level::Warn),
        Some(Choice::Spare { .. }) => (BY_SPARE, Level::Warn),
        Some(Choice::Rental) => (BY_RENTAL, Level::Warn),
        Some(Choice::Drop) => (DROPPED, Level::Bad),
    };
    let (spare_at, rental_at) = arrivals(c, index);
    let spares: Vec<String> = offers
        .iter()
        .take(6)
        .map(|x| match x.until {
            Some(u) => format!("{} (–{})", number(x.vehicle), m(u)),
            None => number(x.vehicle),
        })
        .collect();
    let arrive = match b.choice {
        Some(Choice::Spare { .. } | Choice::Rental) => b.back.unwrap_or(b.at),
        _ if offers.is_empty() => rental_at,
        _ => spare_at,
    };
    let mut msg = Message::new(Kind::Fleet, if b.accident { CRASH_TITLE } else { BROKE_TITLE }, text).level(level).dated(&s.date).key(&format!("breakdown-{}-{}", s.date, b.vehicle));
    msg = msg.arg("bus", b.bus.clone()).arg("n", b.line.clone()).arg("time", m(b.at)).arg("arrive", m(arrive)).arg("spares", spares.join(", "));
    if let Some(Choice::Spare { vehicle }) = b.choice {
        msg = msg.arg("spare", number(vehicle));
    }
    if let Some(u) = b.spare_until {
        msg = msg.arg("leaves", m(u));
    }
    let cost = if b.choice == Some(Choice::Rental) { b.cost } else { rental_cost(c, index) };
    Some(msg.arg("amount", cost.to_string()))
}

/// Put today's `breaks[index]` in the inbox, or bring its message up to date (an unread one is
/// replaced).
fn post_break(c: &mut Company, index: usize) {
    if let Some(m) = break_message(c, index) {
        super::inbox::post(c, m);
    }
}

fn apply(c: &mut Company, due: Due, quick: bool, run: &mut Run) {
    let at = now(c);
    // (the easy mode asks nothing: the dispatcher decides, and its tenders are no auctions of
    // the player's)
    let asks = !quick && !super::autopilot::on(c);
    match due {
        Due::Script(i) => {
            let Some(e) = c.clock.today.as_mut().and_then(|s| s.events.get_mut(i)) else { return };
            e.done = true;
            let when = moment(&c.date, e.at);
            let what = e.what.clone();
            match what {
                What::Out { number, tour, bus, driver } => {
                    let text = match (bus.as_str(), driver.as_str()) {
                        ("rental", _) => "Line %{n} tour %{tour} sets off with a rental bus",
                        (_, "agency") => "Line %{n} tour %{tour} sets off with bus %{bus} and an agency driver",
                        (_, "you") => "Line %{n} tour %{tour} sets off with bus %{bus}: your duty",
                        _ => "Line %{n} tour %{tour} sets off with bus %{bus}, %{name}",
                    };
                    tell(c, run, when, text, vec![arg("n", number), arg("tour", tour), arg("bus", bus), arg("name", driver)], Level::Minor);
                }
                What::In { number, tour } => tell(c, run, when, "Line %{n} tour %{tour} is back in the depot", vec![arg("n", number), arg("tour", tour)], Level::Minor),
                What::NotRunning { number, tour, why } => tell(c, run, when, "Line %{n} tour %{tour} does not run: %{why}", vec![arg("n", number), arg("tour", tour), arg("why", why)], Level::Warn),
                What::Late { name, minutes, number, tour } => tell(c, run, when, "%{name} comes %{min} minutes late for line %{n} tour %{tour}", vec![arg("name", name), arg("min", minutes), arg("n", number), arg("tour", tour)], Level::Warn),
                What::NoStart { bus, cost } => tell(c, run, when, "Bus %{bus} does not start this morning: towed, %{amount}", vec![arg("bus", bus), arg("amount", cost)], Level::Warn),
                What::Morning { count } => {
                    if asks && !c.clock.dispatcher {
                        c.clock.ask = Some(Ask::Morning { count });
                    }
                }
                What::Breakdown { index } => {
                    let Some(b) = c.clock.today.as_ref().and_then(|s| s.breaks.get(index)).cloned() else { return };
                    let text = if b.accident { "Bus %{bus} had an accident on line %{n}" } else { "Bus %{bus} broke down on line %{n}" };
                    tell(c, run, when, text, vec![arg("bus", &b.bus), arg("n", &b.line)], Level::Bad);
                    // (the inbox says it at once, with the spare buses free: wherever the player is)
                    post_break(c, index);
                    if asks && !c.clock.dispatcher {
                        c.clock.ask = Some(Ask::Breakdown { index });
                    } else {
                        let choice = dispatcher_choice(c, index);
                        decide(c, index, choice, run);
                    }
                }
                What::Built { area, level } => {
                    let Some(a) = Area::from_key(&area) else { return };
                    if c.site.works.iter().any(|w| w.area == a && w.level == level) {
                        c.site.works.retain(|w| !(w.area == a && w.level == level));
                        c.site.set_level(a, level);
                    }
                    tell(c, run, when, "Building work finished: %{area}", vec![arg("area", a.label())], Level::Good);
                }
                What::JobDone { bus, job } => tell(c, run, when, "Bus %{bus}: %{job} done, back on the road tomorrow", vec![arg("bus", bus), arg("job", job)], Level::Info),
                What::LastDay { name } => tell(c, run, when, "%{name} works their last day today", vec![arg("name", name)], Level::Info),
                What::Told { text, args, level } => tell(c, run, when, &text, args, level),
            }
        }
        Due::Open(id) => {
            let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id) else { return };
            t.opened = true;
            let (when, number, closes, renewal) = (t.opens_at, t.number.clone(), t.closes_at, t.renewal);
            let text = if renewal { "The renewal of line %{n} is out to tender: bids until %{time}" } else { "Line %{n} is out to tender: bids until %{time}" };
            tell(c, run, when, text, vec![arg("n", number), arg("time", hhmm(closes))], Level::Info);
            if asks {
                c.clock.ask = Some(Ask::Tender { id, outbid: false });
            }
        }
        Due::Bid(id) => {
            let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return };
            let placed = cn::bids(c, &t, at);
            let mut outbid = false;
            for k in t.told..placed.len() {
                let p = placed[k];
                match p.who {
                    super::auction::Who::Player => tell(c, run, p.at, "You bid %{amount} for line %{n}", vec![arg("amount", p.amount), arg("n", &t.number)], Level::Info),
                    who => {
                        let name = cn::bidder_name(c, &t, who);
                        let beaten = k > 0 && placed[k - 1].who == super::auction::Who::Player;
                        outbid |= beaten;
                        tell(c, run, p.at, if beaten { "%{who} bids %{amount} for line %{n}: your bid is beaten" } else { "%{who} bids %{amount} for line %{n}" }, vec![arg("who", name), arg("amount", p.amount), arg("n", &t.number)], if beaten { Level::Warn } else { Level::Minor });
                    }
                }
            }
            if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
                x.told = placed.len();
            }
            if outbid && asks {
                c.clock.ask = Some(Ask::Tender { id, outbid: true });
            }
        }
        Due::Close(id) => {
            let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return };
            // (what was bid before the close is told first)
            let placed = cn::bids(c, &t, t.closes_at);
            if placed.len() > t.told {
                apply(c, Due::Bid(id), true, run);
            }
            let Some((ev, amount)) = cn::close(c, id) else { return };
            let mine = !t.offers.is_empty();
            match ev {
                cn::Event::Won { until, .. } => tell(c, run, t.closes_at, "Line %{n} won for %{amount}: the concession runs until %{date}", vec![arg("n", &t.number), arg("amount", amount), arg("date", until)], Level::Good),
                cn::Event::Lost { winner, .. } if winner.is_empty() => tell(c, run, t.closes_at, "Nobody bid for line %{n}", vec![arg("n", &t.number)], Level::Minor),
                cn::Event::Lost { winner, .. } => {
                    let level = if mine || t.renewal { Level::Bad } else { Level::Minor };
                    let text = if t.renewal { "Line %{n} went to %{who} for %{amount}: your concession ends with its term" } else { "Line %{n} went to %{who} for %{amount}" };
                    tell(c, run, t.closes_at, text, vec![arg("n", &t.number), arg("who", winner), arg("amount", amount)], level);
                }
                cn::Event::Ended { .. } => {}
            }
        }
    }
}

/// The hook for the other parts of the company with things that happen in its time (the
/// dealer's deliveries and offers running out): called at every full hour the clock passes and
/// wherever a simulation stops, with the company's now. What it returns is told in the feed;
/// an item of `Level::Bad` stops the simulation for the player (`Ask::Notice`).
pub fn on_minute(c: &mut Company, now: i64) -> Vec<FeedItem> {
    let mut out = Vec::new();
    // the dealer's orders whose delivery time has come: the buses join the fleet
    for d in super::dealer::tick(c, &super::dealer::moment(now)) {
        out.push(FeedItem { at: now, text: "Delivered: %{name} (%{numbers})".into(), args: vec![("name".into(), d.name.clone()), ("numbers".into(), d.numbers.join(", "))], level: Level::Good });
    }
    // the spare parts ordered for 16:00 (`parts`)
    out.extend(super::parts::on_minute(c, now));
    out
}

fn hook(c: &mut Company, at: i64, quick: bool, run: &mut Run) {
    for item in on_minute(c, at) {
        if item.level == Level::Bad && !quick && c.clock.ask.is_none() {
            c.clock.ask = Some(Ask::Notice { text: item.text.clone(), args: item.args.clone() });
        }
        c.clock.feed.push(item);
        run.told += 1;
    }
    if c.clock.feed.len() > FEED_KEPT {
        let extra = c.clock.feed.len() - FEED_KEPT;
        c.clock.feed.drain(..extra);
    }
}

// --- simulating ---------------------------------------------------------------------------------

/// Simulate until `to` (minutes): every event on the way applied in its order, the days
/// closed at their midnights. Stops early for a decision (`Clock::ask`), unless `quick`: then
/// the dispatcher decides and nothing waits (the quick "Close the day", the game's clock).
/// Nothing moves while a decision waits and the run is not quick.
pub fn advance(c: &mut Company, to: i64, w: &mut dyn World, quick: bool) -> Result<Run, String> {
    let mut run = Run::default();
    if c.clock.ask.is_some() {
        if !quick {
            run.stopped = true;
            return Ok(run);
        }
        answer(c, None, &mut run);
    }
    loop {
        let now = now(c);
        // (a day whose last event came at its midnight is still to be closed)
        if now >= to && (c.clock.minute as i64) < DAY {
            break;
        }
        begin_day(c, w)?;
        let day0 = moment(&c.date, 0);
        let midnight = day0 + DAY;
        let limit = to.max(now).min(midnight).min((now.div_euclid(60) + 1) * 60);
        if let Some((at, due)) = next_due(c, limit) {
            if at > now {
                c.clock.minute = (at - day0) as u32;
            }
            apply(c, due, quick, &mut run);
            // (a line won is the company's at once, to be planned for its first day: #95)
            if matches!(due, Due::Close(_)) {
                let date = c.date.clone();
                cn::take_on(c, &w.lines(c, &date)?);
            }
            if quick {
                if let Some(Ask::Breakdown { index }) = c.clock.ask.clone() {
                    let choice = dispatcher_choice(c, index);
                    decide(c, index, choice, &mut run);
                }
                c.clock.ask = None;
            }
            if c.clock.ask.is_some() {
                c.clock.target = Some(to);
                run.stopped = true;
                return Ok(run);
            }
            continue;
        }
        c.clock.minute = (limit - day0) as u32;
        hook(c, limit, quick, &mut run);
        if c.clock.ask.is_some() {
            c.clock.target = Some(to);
            run.stopped = true;
            return Ok(run);
        }
        if limit == midnight {
            let date = c.date.clone();
            let lines = w.lines(c, &date)?;
            let report = w.close_day(c, &lines)?;
            c.clock.minute = 0;
            c.clock.today = None;
            tell(c, &mut run, midnight, "The day %{date} was closed: %{amount}", vec![arg("date", &date), arg("amount", report.result)], if report.result >= 0 { Level::Good } else { Level::Warn });
            run.reports.push(report);
        }
    }
    c.clock.target = None;
    Ok(run)
}

/// The player's answer to what waits (`choice`: for a breakdown; None there leaves it to the
/// dispatcher). Returns where the simulation was going.
pub fn answer(c: &mut Company, choice: Option<Choice>, run: &mut Run) -> Option<i64> {
    let ask = c.clock.ask.take()?;
    if let Ask::Breakdown { index } = ask {
        let choice = choice.unwrap_or_else(|| dispatcher_choice(c, index));
        decide(c, index, choice, run);
    }
    c.clock.target.take()
}

/// A choice for today's `breaks[index]` made on the planning (the Roster tab): the question
/// answered when it waits for it, else trips dropped get a spare or a rental bus from now on.
/// Returns where a simulation waiting for the answer was going.
pub fn choose(c: &mut Company, index: usize, choice: Choice) -> Option<i64> {
    let mut run = Run::default();
    if c.clock.ask == Some(Ask::Breakdown { index }) {
        return answer(c, Some(choice), &mut run);
    }
    decide(c, index, choice, &mut run);
    None
}

/// The game's clock leads while it runs on the company's map: the company's day moves along to
/// the game's minute of the day (forward only, not past the day; quick).
pub fn follow(c: &mut Company, game_minute: i64, w: &mut dyn World) -> Result<Run, String> {
    let to = moment(&c.date, game_minute.clamp(0, DAY - 1));
    if to <= now(c) {
        return Ok(Run::default());
    }
    advance(c, to, w, true)
}

/// What the game reported of the company's buses, told as it comes (booked at the day's close).
pub fn tell_live(c: &mut Company, events: &[LiveEvent]) -> usize {
    let mut run = Run::default();
    let at = now(c);
    for ev in events {
        match ev {
            LiveEvent::Trip { line, tour, passengers, delay, completed, .. } => {
                let text = if *completed { "Line %{n} tour %{tour}: a trip run in the game, %{pax} passengers, %{late} min off the timetable" } else { "Line %{n} tour %{tour}: a trip broken off in the game" };
                tell(c, &mut run, at, text, vec![arg("n", line), arg("tour", tour), arg("pax", passengers), arg("late", format!("{:+.0}", delay / 60.0))], Level::Minor);
            }
            LiveEvent::Breakdown { vehicle } => {
                let bus = c.vehicle(*vehicle).map(|v| v.number.clone()).unwrap_or_default();
                tell(c, &mut run, at, "Bus %{bus} broke down in the game", vec![arg("bus", bus)], Level::Bad);
            }
        }
    }
    run.told
}

/// The feed's lines of the company's day so far, the newest first.
pub fn feed_of_day(c: &Company) -> impl Iterator<Item = &FeedItem> {
    let day0 = moment(&c.date, 0);
    c.clock.feed.iter().rev().take_while(move |f| f.at >= day0)
}

// --- the real clock -----------------------------------------------------------------------------

/// The longest absence caught up at once: a month. What lies beyond it is skipped, and said.
pub const CATCH_UP_MAX: i64 = 31 * DAY;
/// An absence of at least this many minutes is caught up as one, with a summary of what
/// happened (a shorter one the running clock takes in its stride).
pub const AWAY_TOLD: i64 = 60;
/// How many lines of note a catch-up keeps for its summary (the newest).
pub const NOTABLE_KEPT: usize = 80;
/// How far the game's clock and the company's course may be apart before the course is set
/// again from the game's (the two count their minutes apart).
const GAME_SLACK: i64 = 2;

/// The company's time against the real clock: its course goes on as the real time does from
/// the moment the two were last in step; fast-forwarding moves the company ahead of it, and the
/// course goes on from there (the lead is kept).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RealTime {
    /// The company's time runs with the real clock (the company's settings): on for a new
    /// company and an older file; off, it stands still while the company is closed.
    #[serde(default = "yes")]
    pub on: bool,
    /// When the company was last in step with the real clock. None: not yet (an older file,
    /// the setting just turned on, the date moved) - it goes in step when it is next opened,
    /// without catching up the past.
    #[serde(default)]
    pub synced: Option<Synced>,
    /// Minutes the company runs ahead of the real clock: what was fast-forwarded since it went in
    /// step.
    #[serde(default)]
    pub lead: i64,
    /// The last absence caught up (the start's tile says it).
    #[serde(default)]
    pub caught: Option<CatchUp>,
}

impl Default for RealTime {
    fn default() -> Self {
        RealTime { on: true, synced: None, lead: 0, caught: None }
    }
}

fn yes() -> bool {
    true
}

/// A moment the two clocks agree on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Synced {
    /// The real time, in Unix seconds.
    pub real: i64,
    /// The company's time then (`now`).
    pub minute: i64,
}

/// An absence caught up.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CatchUp {
    /// When it was caught up (Unix seconds).
    pub real: i64,
    /// The company's time it ran from and to (`now`).
    pub from: i64,
    pub to: i64,
    /// The days closed on the way, and their result.
    pub days: u32,
    pub result: Cents,
    /// Minutes of the absence beyond `CATCH_UP_MAX`, skipped.
    #[serde(default)]
    pub skipped: i64,
}

/// What a catch-up came to, for its summary: the absence, the days' reports, the lines of note
/// on the way (oldest first, the last `NOTABLE_KEPT`) and the cash before.
#[derive(Clone, Debug, Default)]
pub struct Away {
    pub caught: CatchUp,
    pub reports: Vec<DayReport>,
    pub notable: Vec<FeedItem>,
    pub cash_before: Cents,
}

/// Where the company's time is by the real clock (`real`, Unix seconds): its course. None: it
/// does not run with the real clock, or was never in step yet.
pub fn course(c: &Company, real: i64) -> Option<i64> {
    let s = c.clock.real.synced.filter(|_| c.clock.real.on)?;
    Some(s.minute + (real - s.real).max(0).div_euclid(60))
}

/// How many minutes the company's time is behind its course (0: in step, ahead, or not running
/// with the real clock).
pub fn behind(c: &Company, real: i64) -> i64 {
    course(c, real).map_or(0, |m| (m - now(c)).max(0))
}

/// The company goes in step with the real clock from now on, from where its time is.
pub fn sync(c: &mut Company, real: i64) {
    c.clock.real.synced = Some(Synced { real, minute: now(c) });
}

/// After a step the player simulated: ahead of its course, the minutes ahead are added to the
/// lead and the course goes on from here - the lead is kept. Whether anything changed.
pub fn keep_lead(c: &mut Company, real: i64) -> bool {
    let Some(m) = course(c, real) else { return false };
    let ahead = now(c) - m;
    if ahead <= 0 {
        return false;
    }
    c.clock.real.lead += ahead;
    sync(c, real);
    true
}

/// The game's clock leads (the company's day goes along with the game's time): the course goes
/// on from where the game has the company - ahead of it, the minutes ahead are added to the
/// lead; behind it (the game started earlier in the day), nothing is caught up after the duty.
/// Within `GAME_SLACK` of the course nothing changes. Whether anything changed.
pub fn with_game(c: &mut Company, real: i64) -> bool {
    let Some(m) = course(c, real) else { return false };
    let off = now(c) - m;
    if off.abs() < GAME_SLACK {
        return false;
    }
    c.clock.real.lead += off.max(0);
    sync(c, real);
    true
}

/// The real-time running turned on or off (the company's settings): on, the company goes in
/// step when it is next opened; off, its time stands still while it is closed.
pub fn set_realtime(c: &mut Company, on: bool) {
    c.clock.real.on = on;
    c.clock.real.synced = None;
    c.clock.real.lead = 0;
}

/// A feed line worth the summary of an absence (the ordinary run of the day and the days'
/// closes - the summary adds those up - left out).
fn notable(f: &FeedItem) -> bool {
    matches!(f.level, Level::Good | Level::Warn | Level::Bad) && !f.text.starts_with("The day %{date} was closed")
}

const AWAY: (&str, &str) = ("While you were away", "The company's time ran on with the real clock while you were away, from %{from} to %{until}. Days closed: %{n}, with a result of %{amount}.");
const AWAY_SHORT: &str = "The company's time ran on with the real clock while you were away, from %{start} to %{end} today.";
const SKIPPED: (&str, &str) = ("Not all caught up", "You were away for longer than %{n} days: the company caught up %{n} days, and the rest was skipped.");

/// Every English text the real clock writes into the inbox, for the translations' check.
pub fn texts() -> Vec<&'static str> {
    vec![AWAY.0, AWAY.1, AWAY_SHORT, SKIPPED.0, SKIPPED.1, BROKE_TITLE, CRASH_TITLE, SPARES_FREE, NO_SPARE, BY_SPARE, BY_SPARE_UNTIL, BY_RENTAL, DROPPED]
}

/// The company is opened again: the real time since it was last in step is added to its time
/// and simulated - the days between closed as "Simulate to tomorrow" closes them, with all that
/// happens on the way, the dispatcher deciding (nobody was there to ask). At most
/// `CATCH_UP_MAX` at once: the rest is skipped and the course goes on from where the company
/// got to. Never in step yet, it goes in step now without catching up the past; ahead of its
/// course (fast-forwarded), it keeps its lead. An absence of `AWAY_TOLD` or more is told in the
/// inbox and kept as the last caught up. None: nothing was simulated.
pub fn catch_up(c: &mut Company, real: i64, w: &mut dyn World) -> Result<Option<Away>, String> {
    if !c.clock.real.on {
        return Ok(None);
    }
    let Some(m) = course(c, real) else {
        sync(c, real);
        return Ok(None);
    };
    let from = now(c);
    let gap = m - from;
    if gap <= 0 {
        keep_lead(c, real);
        return Ok(None);
    }
    let skipped = (gap - CATCH_UP_MAX).max(0);
    let to = from + gap - skipped;
    let mut away = Away { cash_before: c.cash, ..Default::default() };
    // (hour by hour: the feed keeps its last lines only, what matters is gathered on the way)
    while now(c) < to {
        let was = now(c);
        let step = ((was.div_euclid(60) + 1) * 60).min(to);
        let run = advance(c, step, w, true)?;
        let n = run.told.min(c.clock.feed.len());
        let at = c.clock.feed.len() - n;
        away.notable.extend(c.clock.feed[at..].iter().filter(|f| notable(f)).cloned());
        away.reports.extend(run.reports);
        if now(c) <= was {
            break;
        }
    }
    if away.notable.len() > NOTABLE_KEPT {
        let extra = away.notable.len() - NOTABLE_KEPT;
        away.notable.drain(..extra);
    }
    // (the course goes on from here: what was skipped stays behind)
    c.clock.real.synced = Some(Synced { real, minute: now(c) });
    let result = away.reports.iter().map(|r| r.result).sum();
    away.caught = CatchUp { real, from, to: now(c), days: away.reports.len() as u32, result, skipped };
    if gap >= AWAY_TOLD {
        use super::inbox::{post, Kind, Message};
        c.clock.real.caught = Some(away.caught.clone());
        let m = if away.reports.is_empty() {
            Message::new(Kind::Notice, AWAY.0, AWAY_SHORT).arg("start", hhmm(from)).arg("end", hhmm(now(c)))
        } else {
            Message::new(Kind::Notice, AWAY.0, AWAY.1).arg("from", date_of(from)).arg("until", date_of(now(c) - 1)).arg("n", away.reports.len().to_string()).arg("amount", result.to_string()).level(if result >= 0 { Level::Good } else { Level::Warn })
        };
        post(c, m);
        if skipped > 0 {
            post(c, Message::new(Kind::Notice, SKIPPED.0, SKIPPED.1).arg("n", (CATCH_UP_MAX / DAY).to_string()).level(Level::Warn));
        }
    }
    Ok(Some(away))
}

/// A span of minutes in days, hours and minutes (the pages write it "2 d 4 h", "4 h 10 min").
pub fn span(minutes: i64) -> (i64, i64, i64) {
    let m = minutes.max(0);
    (m / DAY, m % DAY / 60, m % 60)
}

#[cfg(test)]
mod tests {
    use super::super::concessions::tests::line;
    use super::super::market::{self, MarketBus, Payment};
    use super::super::staff::{applicants, hire};
    use super::super::{day, depot, found, Founding};
    use super::super::day::DropWhy;
    use super::*;
    use crate::company::model::{BookingKind, GameMode, Licence};

    /// The timetable of every day the same; the night as the store runs it.
    struct Fake(Vec<LineInfo>);

    impl World for Fake {
        fn lines(&mut self, _: &Company, _: &str) -> Result<Vec<LineInfo>, String> {
            Ok(self.0.clone())
        }

        fn close_day(&mut self, c: &mut Company, lines: &[LineInfo]) -> Result<DayReport, String> {
            network::refresh_lines(c, lines);
            let date = c.date.clone();
            let tours = network::tours_of_day(c, lines, &date);
            let r = day::close_day(c, tours, &[]);
            Ok(depot::after_close(c, r, lines))
        }
    }

    fn company(buses: usize, people: usize) -> (Company, Fake) {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true)];
        let mut c = found(&Founding { name: "Uhr".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        network::add_line(&mut c, &lines[0], None).unwrap();
        // (in service from the start, the dispatcher's own filling the roster's gaps)
        network::start_service(&mut c, "Linie5", 0).unwrap();
        c.planning.auto = true;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..buses {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < people {
            for a in applicants(&c) {
                if c.staff.len() < people && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
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
        (c, Fake(lines))
    }

    #[test]
    fn steps_go_where_they_say() {
        let (mut c, _) = company(0, 0);
        c.clock.minute = 14 * 60 + 10;
        let now = now(&c);
        assert_eq!(target(&c, Step::Minutes(15)), now + 15);
        assert_eq!(hhmm(target(&c, Step::Morning)), "05:00");
        assert_eq!(date_of(target(&c, Step::Morning)), "2024-03-05");
        assert_eq!(target(&c, Step::Midnight), moment("2024-03-05", 0));
        assert_eq!(target(&c, Step::Days(2)), now + 2 * DAY);
        assert_eq!(target(&c, Step::Until(16 * 60)), moment("2024-03-04", 16 * 60));
        assert_eq!(target(&c, Step::Until(9 * 60)), moment("2024-03-05", 9 * 60));
        c.clock.minute = 0;
        assert_eq!(date_of(target(&c, Step::Morning)), "2024-03-04", "from midnight: this morning");
    }

    #[test]
    fn the_company_moves_to_another_day_but_not_before_its_books() {
        let (mut c, _) = company(1, 1);
        let start = c.date.clone();
        move_to(&mut c, "2030-01-15").unwrap();
        assert_eq!((c.date.as_str(), c.clock.minute), ("2030-01-15", 0));
        // (the founding's capital is booked on its first day: back to it, not before)
        assert_eq!(earliest_date(&c), Some(start.clone()));
        move_to(&mut c, &start).unwrap();
        assert!(move_to(&mut c, &dates::add(&start, -1)).is_err());
        assert!(move_to(&mut c, "nonsense").is_err());
    }

    #[test]
    fn a_day_runs_through_its_events_in_order_and_closes_at_midnight() {
        let (mut c, mut w) = company(3, 6);
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, true).unwrap();
        assert!(run.reports.is_empty() && !run.stopped);
        assert_eq!((c.date.as_str(), c.clock.minute), ("2024-03-04", 12 * 60));
        let feed = &c.clock.feed;
        assert!(feed.len() >= 3, "{feed:?}");
        assert!(feed.windows(2).all(|f| f[0].at <= f[1].at), "in order");
        assert!(feed.iter().all(|f| f.at <= now(&c)));
        // the three tours left at six (the line's first trips), none came back before nine
        assert_eq!(feed.iter().filter(|f| f.text.starts_with("Line %{n} tour %{tour} sets off") || f.text.contains("does not run")).count(), 3);
        assert!(feed.iter().filter(|f| f.text.contains("is back")).all(|f| minute_of(f.at) >= 9 * 60));
        let s = c.clock.today.as_ref().unwrap();
        assert!(s.events.iter().all(|e| e.done == (e.at <= 12 * 60)));
        // on to midnight: the day closes, the next one begins
        let to = target(&c, Step::Midnight);
        let run = advance(&mut c, to, &mut w, true).unwrap();
        assert_eq!(run.reports.len(), 1);
        assert_eq!((c.date.as_str(), c.clock.minute), ("2024-03-05", 0));
        assert_eq!(c.history.len(), 1);
        assert!(c.clock.feed.last().unwrap().text.starts_with("The day"));
    }

    #[test]
    fn catching_up_days_at_once_is_the_same_as_step_by_step() {
        let (a0, mut w) = company(2, 5);
        let mut a = a0.clone();
        let mut b = a0;
        let to = moment("2024-03-07", 10 * 60);
        let run = advance(&mut a, to, &mut w, true).unwrap();
        assert_eq!(run.reports.len(), 3);
        let mut n = 0;
        while now(&b) < to {
            let next = (now(&b) + 5 * 60 + 7).min(to);
            n += advance(&mut b, next, &mut w, true).unwrap().reports.len();
        }
        assert_eq!(n, 3);
        assert_eq!((a.date.as_str(), a.clock.minute), ("2024-03-07", 10 * 60));
        assert_eq!(a, b);
    }

    #[test]
    fn a_breakdown_waits_for_the_player_and_a_rental_bus_saves_its_trips() {
        let (mut c, mut w) = company(3, 6);
        advance(&mut c, moment("2024-03-04", 5 * 60), &mut w, false).unwrap();
        // a bus out on a tour breaks down at eight (and nothing else stops the day)
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.clear();
        s.events.retain(|e| !matches!(e.what, What::Breakdown { .. } | What::Morning { .. }));
        let number = s.events.iter().find_map(|e| if let What::Out { bus, .. } = &e.what { Some(bus.clone()) } else { None }).unwrap();
        let bus = c.fleet.iter().find(|v| v.number == number).unwrap().clone();
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.push(Broken { vehicle: bus.id, bus: bus.number.clone(), line: "5".into(), at: 8 * 60, until: 0, line_name: String::new(), choice: None, cost: 0, accident: false, back: None, spare_until: None });
        s.events.push(Timed { at: 8 * 60, what: What::Breakdown { index: 0 }, done: false });
        s.events.sort_by_key(|e| e.at);
        let mut rental = c.clone();
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        assert!(run.stopped);
        assert_eq!(c.clock.ask, Some(Ask::Breakdown { index: 0 }));
        assert_eq!(hhmm(now(&c)), "08:00");
        // nothing moves while it waits
        assert!(advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap().stopped);
        assert_eq!(hhmm(now(&c)), "08:00");
        let mut dropped = c.clone();
        let cash = c.cash;
        let mut r = Run::default();
        assert_eq!(answer(&mut c, Some(Choice::Rental), &mut r), Some(moment("2024-03-04", 12 * 60)));
        // (ordered now, paid with the day at its close: the day's figures have it)
        let cost = rental_cost(&c, 0);
        assert_eq!((c.cash, c.clock.today.as_ref().unwrap().breaks[0].cost), (cash, cost));
        answer(&mut dropped, Some(Choice::Drop), &mut r);
        let end = moment("2024-03-05", 0);
        let a = advance(&mut c, end, &mut w, true).unwrap().reports.remove(0);
        let b = advance(&mut dropped, end, &mut w, true).unwrap().reports.remove(0);
        assert!(a.dropped < b.dropped, "{} {}", a.dropped, b.dropped);
        assert!(a.notes.iter().any(|n| matches!(n, Note::Breakdown { number, .. } if *number == bus.number)));
        let rentals = |c: &Company| c.ledger.iter().filter(|x| x.date == "2024-03-04" && x.kind == BookingKind::Rent && x.text.starts_with("Rental bus for")).map(|x| x.amount).collect::<Vec<_>>();
        assert_eq!((rentals(&c), rentals(&dropped)), (vec![-cost], vec![]));
        // quick: the dispatcher decides by himself
        let run = advance(&mut rental, end, &mut w, true).unwrap();
        assert!(!run.stopped && rental.clock.ask.is_none());
        assert_eq!(rental.clock.today, None);
    }

    #[test]
    fn a_tender_runs_for_hours_with_the_rivals_bidding_as_the_clock_goes() {
        let (mut c, mut w) = company(1, 2);
        c.clock.dispatcher = true;
        advance(&mut c, moment("2024-03-04", 9 * 60), &mut w, true).unwrap();
        c.concessions.tenders.clear();
        let id = cn::apply(&mut c, &w.0[1]).unwrap();
        let t = c.concessions.tenders.iter().find(|t| t.id == id).unwrap().clone();
        assert_eq!(t.opens_at, now(&c));
        // it opens: the player is asked
        let run = advance(&mut c, t.closes_at + 60, &mut w, false).unwrap();
        assert!(run.stopped);
        assert_eq!(c.clock.ask, Some(Ask::Tender { id, outbid: false }));
        // the least bid; a rival beats it soon and the simulation stops for it
        let least = cn::min_bid(&c, c.concessions.tenders.iter().find(|t| t.id == id).unwrap());
        cn::bid(&mut c, id, least).unwrap();
        let mut r = Run::default();
        let to = answer(&mut c, None, &mut r).unwrap();
        let mut stops = 0;
        while c.concessions.tenders.iter().find(|t| t.id == id).unwrap().open() {
            let run = advance(&mut c, to, &mut w, false).unwrap();
            if run.stopped {
                stops += 1;
                assert!(matches!(c.clock.ask, Some(Ask::Tender { outbid: true, .. })));
                answer(&mut c, None, &mut r);
            }
            assert!(stops < 20);
        }
        let t = c.concessions.tenders.iter().find(|t| t.id == id).unwrap();
        assert!(matches!(t.outcome, Some(cn::Outcome::Lost { .. }) | Some(cn::Outcome::Won { .. })));
        if matches!(t.outcome, Some(cn::Outcome::Lost { .. })) {
            assert!(stops >= 1, "a lost auction told the player he was beaten");
        }
        // it closed at its minute, the same day, and the feed told the bids as they came
        assert_eq!(c.date, "2024-03-04");
        assert!(now(&c) >= t.closes_at);
        let bids: Vec<&FeedItem> = c.clock.feed.iter().filter(|f| f.text.contains("bids %{amount}") || f.text.starts_with("You bid")).collect();
        assert!(bids.len() >= 2 && bids.iter().all(|f| f.at >= t.opens_at && f.at < t.closes_at));
        assert!(c.clock.feed.iter().any(|f| f.at == t.closes_at && f.text.starts_with("Line %{n}")));
    }

    #[test]
    fn a_line_won_as_the_clock_closes_its_tender_is_the_companys_at_once() {
        // (#95: so RosterFlow shows it to be planned for its first day, tomorrow)
        let (mut c, mut w) = company(1, 2);
        advance(&mut c, moment("2024-03-04", 9 * 60), &mut w, true).unwrap();
        c.concessions.tenders.clear();
        let id = cn::apply(&mut c, &w.0[1]).unwrap();
        let t = c.concessions.tenders.iter().find(|t| t.id == id).unwrap().clone();
        cn::bid(&mut c, id, t.buy_out() - super::super::auction::ROUND).unwrap();
        advance(&mut c, t.closes_at + 1, &mut w, true).unwrap();
        assert!(matches!(c.concessions.tenders.iter().find(|t| t.id == id).unwrap().outcome, Some(cn::Outcome::Won { .. })));
        assert_eq!(c.date, "2024-03-04");
        let l = c.lines.iter().find(|l| l.name == "Linie7").expect("taken on at the close");
        assert_eq!((l.added.as_str(), l.service_from), ("2024-03-05", None));
    }

    #[test]
    fn an_auction_closing_at_midnight_still_closes_the_day() {
        let (mut c, mut w) = company(1, 2);
        advance(&mut c, moment("2024-03-04", 20 * 60), &mut w, true).unwrap();
        c.concessions.tenders.clear();
        let id = cn::apply(&mut c, &w.0[1]).unwrap();
        // (its bids are taken until the day's midnight)
        let midnight = moment("2024-03-05", 0);
        let t = c.concessions.tenders.iter_mut().find(|t| t.id == id).unwrap();
        (t.closes_at, t.closes) = (midnight, date_of(midnight));
        let to = target(&c, Step::Midnight);
        let run = advance(&mut c, to, &mut w, true).unwrap();
        assert_eq!(run.reports.len(), 1);
        assert_eq!((c.date.as_str(), c.clock.minute), ("2024-03-05", 0));
        assert!(c.concessions.tenders.iter().find(|t| t.id == id).is_some_and(|t| !t.open()));
        // and the next step to tomorrow is one day
        let to = target(&c, Step::Midnight);
        let run = advance(&mut c, to, &mut w, true).unwrap();
        assert_eq!((run.reports.len(), c.date.as_str()), (1, "2024-03-06"));
    }

    #[test]
    fn moving_the_company_to_its_own_day_leaves_its_clock() {
        let (mut c, mut w) = company(1, 2);
        advance(&mut c, moment("2024-03-04", 9 * 60), &mut w, true).unwrap();
        let before = c.clone();
        move_to(&mut c, "2024-03-04").unwrap();
        assert_eq!(c, before);
    }

    /// A real moment for the tests (Unix seconds): no clock of the computer is asked.
    const REAL: i64 = 1_790_000_000;
    const HOUR: i64 = 3600;

    #[test]
    fn opened_again_the_company_catches_up_the_real_time_passed_closing_the_days_between() {
        let (mut c, mut w) = company(2, 5);
        sync(&mut c, REAL);
        let start = now(&c);
        let mut by_hand = c.clone();
        // three days and five hours later
        let real = REAL + 3 * 24 * HOUR + 5 * HOUR + 59;
        assert_eq!(behind(&c, real), 3 * DAY + 5 * 60);
        let away = catch_up(&mut c, real, &mut w).unwrap().unwrap();
        assert_eq!(now(&c), start + 3 * DAY + 5 * 60);
        assert_eq!((c.date.as_str(), hhmm(now(&c)).as_str()), ("2024-03-07", "05:00"));
        assert_eq!(behind(&c, real), 0);
        // the days between closed on the way, as simulating to that time does
        assert_eq!((away.reports.len(), away.caught.days, c.history.len()), (3, 3, 3));
        advance(&mut by_hand, start + 3 * DAY + 5 * 60, &mut w, true).unwrap();
        assert_eq!((c.cash, &c.history, &c.date), (by_hand.cash, &by_hand.history, &by_hand.date));
        assert_eq!(away.caught.result, away.reports.iter().map(|r| r.result).sum::<Cents>());
        assert_eq!((away.caught.from, away.caught.to, away.caught.skipped), (start, now(&c), 0));
        assert!(away.notable.iter().all(|f| f.level != Level::Minor && !f.text.starts_with("The day")));
        // told in the inbox, and kept for the start's tile
        assert_eq!(c.clock.real.caught.as_ref(), Some(&away.caught));
        let m = c.inbox.messages.iter().find(|m| m.title == "While you were away").unwrap();
        assert_eq!((m.value("from"), m.value("until"), m.value("n")), (Some("2024-03-04"), Some("2024-03-07"), Some("3")));
        // the course goes on with the real clock: an hour later it is an hour behind
        assert_eq!(behind(&c, real + HOUR), 60);
        // (nothing passed: nothing to catch up)
        let before = c.clone();
        assert!(catch_up(&mut c, real + 30, &mut w).unwrap().is_none());
        assert_eq!(c, before);
    }

    #[test]
    fn a_fast_forward_moves_the_company_ahead_of_the_real_clock_and_keeps_its_lead() {
        let (mut c, mut w) = company(1, 2);
        sync(&mut c, REAL);
        let start = now(&c);
        // ten minutes later the player simulates to tomorrow
        let real = REAL + 600;
        let to = target(&c, Step::Midnight);
        advance(&mut c, to, &mut w, true).unwrap();
        assert!(keep_lead(&mut c, real));
        assert_eq!(c.clock.real.lead, DAY - 10);
        assert_eq!(course(&c, real), Some(start + DAY));
        // (in step: nothing more)
        assert!(!keep_lead(&mut c, real));
        // two hours later: the lead is kept, the real time added to it
        let away = catch_up(&mut c, real + 2 * HOUR, &mut w).unwrap().unwrap();
        assert_eq!(now(&c), start + DAY + 120);
        assert_eq!((c.clock.real.lead, away.caught.days), (DAY - 10, 0));
        // (a short absence on its day is told as the hours it ran)
        let m = c.inbox.messages.iter().rev().find(|m| m.title == "While you were away").unwrap();
        assert_eq!((m.text.as_str(), m.value("start"), m.value("end")), (AWAY_SHORT, Some("00:00"), Some("02:00")));
        // a company that fell behind (a decision waited) and is then stepped on gets only the
        // minutes past its course as lead
        let real = real + 2 * HOUR + 30 * 60;
        let to = now(&c) + 60;
        advance(&mut c, to, &mut w, true).unwrap();
        assert!(keep_lead(&mut c, real));
        assert_eq!(c.clock.real.lead, DAY - 10 + 30);
    }

    #[test]
    fn a_long_absence_is_caught_up_a_month_at_most_and_the_rest_is_skipped() {
        let (mut c, mut w) = company(1, 1);
        sync(&mut c, REAL);
        let start = now(&c);
        let real = REAL + 45 * 24 * HOUR;
        let away = catch_up(&mut c, real, &mut w).unwrap().unwrap();
        assert_eq!(now(&c), start + CATCH_UP_MAX);
        assert_eq!((away.caught.days, away.caught.skipped), (31, 14 * DAY));
        assert_eq!(c.history.len(), 31);
        // in step from here: the skipped days are not caught up later
        assert_eq!(behind(&c, real), 0);
        assert_eq!(c.clock.real.synced, Some(Synced { real, minute: start + CATCH_UP_MAX }));
        assert!(c.inbox.messages.iter().any(|m| m.title == "Not all caught up" && m.value("n") == Some("31")));
        assert!(away.notable.len() <= NOTABLE_KEPT);
    }

    #[test]
    fn an_older_company_goes_in_step_when_opened_without_catching_up_its_past() {
        // (a file of before: its clock has no real time)
        let old: Clock = serde_json::from_str(r#"{"minute":540,"dispatcher":true}"#).unwrap();
        assert_eq!((old.real.on, old.real.synced, old.real.lead), (true, None, 0));
        let (mut c, mut w) = company(1, 2);
        c.clock = old;
        let before = now(&c);
        assert_eq!((course(&c, REAL), behind(&c, REAL)), (None, 0));
        assert!(catch_up(&mut c, REAL + 99 * 24 * HOUR, &mut w).unwrap().is_none());
        assert_eq!(now(&c), before);
        assert_eq!(c.clock.real.synced, Some(Synced { real: REAL + 99 * 24 * HOUR, minute: before }));
        assert!(c.history.is_empty() && c.clock.real.caught.is_none());
        // and from then on with the real clock
        assert_eq!(behind(&c, REAL + 99 * 24 * HOUR + 2 * HOUR), 120);
    }

    #[test]
    fn with_the_real_time_running_off_the_company_stands_still_while_it_is_closed() {
        let (mut c, mut w) = company(1, 2);
        sync(&mut c, REAL);
        let to = now(&c) + 120;
        advance(&mut c, to, &mut w, true).unwrap();
        keep_lead(&mut c, REAL);
        set_realtime(&mut c, false);
        assert_eq!((c.clock.real.on, c.clock.real.synced, c.clock.real.lead), (false, None, 0));
        let before = c.clone();
        assert!(catch_up(&mut c, REAL + 10 * 24 * HOUR, &mut w).unwrap().is_none());
        assert_eq!(c, before);
        assert_eq!((course(&c, REAL + HOUR), behind(&c, REAL + HOUR)), (None, 0));
        // on again: in step from when it is opened, nothing of the days off caught up
        set_realtime(&mut c, true);
        assert!(catch_up(&mut c, REAL + 11 * 24 * HOUR, &mut w).unwrap().is_none());
        assert_eq!((now(&c), behind(&c, REAL + 11 * 24 * HOUR + 60)), (now(&before), 1));
    }

    #[test]
    fn the_games_clock_leads_without_a_catch_up_after_it() {
        let (mut c, mut w) = company(1, 2);
        sync(&mut c, REAL);
        let start = now(&c);
        // the game was started at 08:00 on the company's map: the company follows it
        follow(&mut c, 8 * 60, &mut w).unwrap();
        assert!(with_game(&mut c, REAL + 60));
        assert_eq!((c.clock.real.lead, course(&c, REAL + 60)), (8 * 60 - 1, Some(start + 8 * 60)));
        // a minute apart is the clocks counting apart: nothing changes
        follow(&mut c, 8 * 60 + 1, &mut w).unwrap();
        assert!(!with_game(&mut c, REAL + 60));
        // the game behind the course (an hour of it lost): not caught up after the duty
        follow(&mut c, 8 * 60 + 2, &mut w).unwrap();
        assert!(with_game(&mut c, REAL + 60 + HOUR));
        assert_eq!((behind(&c, REAL + 60 + HOUR), c.clock.real.lead), (0, 8 * 60 - 1));
    }

    #[test]
    fn moving_the_date_puts_the_company_in_step_again_without_its_lead() {
        let (mut c, mut w) = company(1, 2);
        sync(&mut c, REAL);
        let to = now(&c) + 300;
        advance(&mut c, to, &mut w, true).unwrap();
        keep_lead(&mut c, REAL);
        assert_eq!(c.clock.real.lead, 300);
        move_to(&mut c, "2024-04-01").unwrap();
        assert_eq!((c.clock.real.synced, c.clock.real.lead), (None, 0));
        assert_eq!(span(2 * DAY + 4 * 60 + 7), (2, 4, 7));
        assert_eq!(span(-5), (0, 0, 0));
    }

    #[test]
    fn a_sick_call_waits_for_the_dispatcher_in_the_morning() {
        let (c, w) = company(2, 4);
        let tours = network::tours_of_day(&c, &w.0, &c.date);
        let mut dp = plan::day_plan(&c, &c.date, tours, &[], &[], false);
        dp.disruptions.clear();
        let none = script(&c, &dp);
        assert!(!none.events.iter().any(|e| matches!(e.what, What::Morning { .. })));
        // a duty whose driver called in sick: the morning asks the player how to cover it
        dp.tours[0].duties[0].problem = Some(plan::Problem::Sick);
        let s = script(&c, &dp);
        let m = s.events.iter().find(|e| matches!(e.what, What::Morning { .. })).unwrap();
        assert_eq!(m.what, What::Morning { count: 1 });
        assert!(m.at <= dp.tours[0].tour.from() as i64);
    }

    #[test]
    fn a_spare_bus_of_the_fleet_takes_over_a_broken_down_one() {
        // (testers: "when a bus breaks down, it does not give me the option to dispatch one of
        // my spare buses")
        let (mut c, mut w) = company(6, 8);
        advance(&mut c, moment("2024-03-04", 5 * 60), &mut w, false).unwrap();
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.clear();
        s.events.retain(|e| !matches!(e.what, What::Breakdown { .. } | What::Morning { .. }));
        let number = s.events.iter().find_map(|e| if let What::Out { bus, .. } = &e.what { Some(bus.clone()) } else { None }).unwrap();
        let busy: Vec<u32> = s.busy.iter().map(|x| x.0).collect();
        let bus = c.fleet.iter().find(|v| v.number == number).unwrap().clone();
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.push(Broken { vehicle: bus.id, bus: bus.number.clone(), line: "5".into(), at: 8 * 60, until: 20 * 60, line_name: "Linie5".into(), choice: None, cost: 0, accident: false, back: None, spare_until: None });
        s.events.push(Timed { at: 8 * 60, what: What::Breakdown { index: 0 }, done: false });
        s.events.sort_by_key(|e| e.at);
        // the buses not out on a tour then are offered, the broken one and those out are not
        let spares = spare_buses(&c, 0);
        assert!(!spares.is_empty());
        assert!(!spares.contains(&bus.id) && spares.iter().all(|id| !busy.contains(id)));
        // one in the workshop is no spare
        let mut shop = c.clone();
        shop.fleet.iter_mut().find(|v| v.id == spares[0]).unwrap().workshop_until = Some("2024-03-04".into());
        assert!(!spare_buses(&shop, 0).contains(&spares[0]));
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        assert!(run.stopped && c.clock.ask == Some(Ask::Breakdown { index: 0 }));
        let mut dropped = c.clone();
        let mut r = Run::default();
        answer(&mut c, Some(Choice::Spare { vehicle: spares[0] }), &mut r);
        assert_eq!(c.clock.today.as_ref().unwrap().breaks[0].cost, 0);
        assert_eq!(spare_of(&c, "2024-03-04", bus.id), Some(spares[0]));
        // (sent once: not offered for another breakdown then)
        let mut again = c.clone();
        let s = again.clock.today.as_mut().unwrap();
        s.breaks.push(Broken { vehicle: busy.iter().copied().find(|id| *id != bus.id).unwrap_or(bus.id), bus: "x".into(), line: "5".into(), at: 9 * 60, until: 12 * 60, line_name: "Linie5".into(), choice: None, cost: 0, accident: false, back: None, spare_until: None });
        assert!(!spare_buses(&again, 1).contains(&spares[0]));
        answer(&mut dropped, Some(Choice::Drop), &mut r);
        let km = c.vehicle(spares[0]).unwrap().km;
        let end = moment("2024-03-05", 0);
        let a = advance(&mut c, end, &mut w, true).unwrap().reports.remove(0);
        let b = advance(&mut dropped, end, &mut w, true).unwrap().reports.remove(0);
        // the close counts it: fewer trips dropped, no rental paid, the spare bus drove them
        assert!(a.dropped < b.dropped, "{} {}", a.dropped, b.dropped);
        assert!(!c.ledger.iter().any(|x| x.kind == BookingKind::Rent && x.text.starts_with("Rental bus for")));
        assert!(c.vehicle(spares[0]).unwrap().km > km);
        // an older day's script reads without the new fields
        let old: DayScript = serde_json::from_str(r#"{"date":"2024-03-04","events":[],"breaks":[{"vehicle":1,"bus":"101","line":"5","at":480,"choice":"rental"}]}"#).unwrap();
        assert_eq!((old.breaks[0].choice, old.breaks[0].until, old.busy.len()), (Some(Choice::Rental), 0, 0));
    }

    /// A company at 05:00 of its day whose first bus out breaks down (or crashes) at `at` - the
    /// only thing that stops the day - with `until` the end of its day; and that bus.
    fn broken_at(at: i32, until: i32, accident: bool) -> (Company, Fake, super::super::model::Vehicle) {
        let (mut c, mut w) = company(6, 8);
        advance(&mut c, moment("2024-03-04", 5 * 60), &mut w, false).unwrap();
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.clear();
        s.events.retain(|e| !matches!(e.what, What::Breakdown { .. } | What::Morning { .. }));
        let number = s.events.iter().find_map(|e| if let What::Out { bus, .. } = &e.what { Some(bus.clone()) } else { None }).unwrap();
        let bus = c.fleet.iter().find(|v| v.number == number).unwrap().clone();
        let s = c.clock.today.as_mut().unwrap();
        s.breaks.push(Broken { vehicle: bus.id, bus: bus.number.clone(), line: "5".into(), at, until, line_name: "Linie5".into(), choice: None, cost: 0, accident, back: None, spare_until: None });
        s.events.push(Timed { at: at as i64, what: What::Breakdown { index: 0 }, done: false });
        s.events.sort_by_key(|e| e.at);
        (c, w, bus)
    }

    #[test]
    fn a_spare_bus_sent_loses_only_the_trips_before_it_comes() {
        // (testers: "Bus broke down, it told me to allocate a different vehicle and then still
        // classed it as lost mileage"; its tour runs hourly from 06:00, half an hour a trip)
        let (mut c, mut w, bus) = broken_at(7 * 60 + 50, 9 * 60 + 30, false);
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        assert!(run.stopped && run.breaks.is_empty());
        // (the inbox says it at once, with the spare buses free)
        let m = c.inbox.messages.iter().find(|m| m.title == BROKE_TITLE).unwrap();
        assert_eq!((m.text.as_str(), m.value("bus")), (SPARES_FREE, Some(bus.number.as_str())));
        let spare = spare_buses(&c, 0)[0];
        let km = c.vehicle(spare).unwrap().km;
        answer(&mut c, Some(Choice::Spare { vehicle: spare }), &mut Run::default());
        // (the message says what was sent now, and it comes at 08:20)
        let m = c.inbox.messages.iter().find(|m| m.title == BROKE_TITLE).unwrap();
        assert_eq!((m.text.as_str(), m.value("arrive")), (BY_SPARE, Some("08:20")));
        assert_eq!(c.inbox.messages.iter().filter(|m| m.title == BROKE_TITLE).count(), 1);
        let r = advance(&mut c, moment("2024-03-05", 0), &mut w, true).unwrap().reports.remove(0);
        // only the 08:00 trip is lost, as broken down; the 09:00 one ran with the spare bus
        assert_eq!(r.dropped, 1, "{:?}", r.drops);
        let d = &r.drops[0];
        assert_eq!((d.why, d.from, d.trips, d.sent), (DropWhy::BrokeDown, 8 * 60, 1, Some((8 * 60 + 20, None))));
        assert_eq!(c.vehicle(spare).unwrap().km - km, 10.0);
        assert!(r.notes.iter().any(|n| matches!(n, Note::Breakdown { number, .. } if *number == bus.number)));
    }

    #[test]
    fn without_a_spare_bus_every_trip_after_the_breakdown_is_lost() {
        let (mut c, mut w, bus) = broken_at(7 * 60 + 50, 9 * 60 + 30, false);
        advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        answer(&mut c, Some(Choice::Drop), &mut Run::default());
        assert_eq!(c.inbox.messages.iter().find(|m| m.title == BROKE_TITLE).map(|m| m.text.as_str()), Some(DROPPED));
        let r = advance(&mut c, moment("2024-03-05", 0), &mut w, true).unwrap().reports.remove(0);
        assert_eq!(r.dropped, 2, "{:?}", r.drops);
        let d = &r.drops[0];
        assert_eq!((d.why, d.from, d.to, d.trips, d.sent), (DropWhy::BrokeDown, 8 * 60, 9 * 60 + 30, 2, None));
        assert_eq!(c.vehicle(bus.id).unwrap().breakdowns, bus.breakdowns + 1);
    }

    #[test]
    fn a_bus_out_later_is_a_spare_until_its_own_tour_and_trips_dropped_still_get_one() {
        let (mut c, mut w, bus) = broken_at(7 * 60 + 50, 20 * 60, false);
        let free = spare_buses(&c, 0);
        // (one of them goes out on a tour of its own at 13:00: a spare until ten minutes before)
        let later = free[0];
        c.clock.today.as_mut().unwrap().busy.push((later, 13 * 60, 15 * 60));
        let offers = spare_offers(&c, 0);
        let x = offers.iter().find(|x| x.vehicle == later).unwrap();
        assert_eq!(x.until, Some(13 * 60 - super::super::staff::BUS_MARGIN));
        // (those free all day come first)
        assert!(offers[0].until.is_none());
        // the trips dropped at first get a spare bus later the same day, from then on
        advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        answer(&mut c, Some(Choice::Drop), &mut Run::default());
        advance(&mut c, moment("2024-03-04", 11 * 60), &mut w, false).unwrap();
        assert_eq!(choose(&mut c, 0, Choice::Spare { vehicle: later }), None);
        let b = &c.clock.today.as_ref().unwrap().breaks[0];
        assert_eq!((b.choice, b.back, b.spare_until), (Some(Choice::Spare { vehicle: later }), Some(11 * 60 + 30), Some(12 * 60 + 50)));
        let date = c.date.clone();
        let cut = breakdowns(&c, &Plan::default(), &date, &[]);
        assert_eq!(cut, vec![Cut { vehicle: bus.id, at: 7 * 60 + 50, back: Some(11 * 60 + 30), until: Some(12 * 60 + 50), accident: false }]);
        assert!(cut[0].lost(9 * 60) && !cut[0].lost(12 * 60) && cut[0].lost(13 * 60) && !cut[0].lost(7 * 60));
        // (once sent, it is not sent again)
        assert_eq!(choose(&mut c, 0, Choice::Rental), None);
        assert_eq!(c.clock.today.as_ref().unwrap().breaks[0].choice, Some(Choice::Spare { vehicle: later }));
    }

    #[test]
    fn an_accident_takes_the_bus_off_the_road_as_a_breakdown_does() {
        let (mut c, mut w, bus) = broken_at(7 * 60 + 50, 9 * 60 + 30, true);
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, false).unwrap();
        assert!(run.stopped && c.clock.ask == Some(Ask::Breakdown { index: 0 }));
        assert!(c.clock.feed.iter().any(|f| f.text == "Bus %{bus} had an accident on line %{n}"));
        assert!(c.inbox.messages.iter().any(|m| m.title == CRASH_TITLE));
        answer(&mut c, Some(Choice::Drop), &mut Run::default());
        let r = advance(&mut c, moment("2024-03-05", 0), &mut w, true).unwrap().reports.remove(0);
        assert_eq!((r.dropped, r.drops[0].why), (2, DropWhy::Crashed));
        // (its damage booked with the day's accidents, in the workshop for it - no breakdown)
        assert!(r.notes.iter().any(|n| matches!(n, Note::Accident { number, cost } if *number == bus.number && *cost > 0)));
        assert!(!r.notes.iter().any(|n| matches!(n, Note::Breakdown { number, .. } if *number == bus.number)));
        let v = c.vehicle(bus.id).unwrap();
        assert!(v.in_workshop("2024-03-05") && v.breakdowns == bus.breakdowns);
    }

    #[test]
    fn the_dispatcher_says_what_he_sent_and_an_older_breakdown_reads() {
        let (mut c, mut w, _) = broken_at(7 * 60 + 50, 9 * 60 + 30, false);
        let run = advance(&mut c, moment("2024-03-04", 12 * 60), &mut w, true).unwrap();
        assert!(!run.stopped && run.breaks.len() == 1 && run.breaks[0].text == BY_SPARE);
        assert!(matches!(c.clock.today.as_ref().unwrap().breaks[0].choice, Some(Choice::Spare { .. })));
        assert_eq!(c.inbox.messages.iter().find(|m| m.title == BROKE_TITLE).map(|m| m.text.as_str()), Some(BY_SPARE));
        let old: Broken = serde_json::from_str(r#"{"vehicle":1,"bus":"101","line":"5","at":480,"choice":{"spare":{"vehicle":2}}}"#).unwrap();
        assert_eq!((old.accident, old.back, old.spare_until), (false, None, None));
        let mut s = DayScript { date: c.date.clone(), breaks: vec![old], ..Default::default() };
        s.breaks[0].until = 600;
        c.clock.today = Some(s);
        let date = c.date.clone();
        assert_eq!(breakdowns(&c, &Plan::default(), &date, &[])[0].back, Some(480 + SPARE_MINUTES));
    }
}

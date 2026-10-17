//! On time: what the company tells its driver on the way (Luc, 2026-10-16: "the overlay and the
//! navigator should do much more" - punctuality, headway, connections and diversions as one
//! package). The rules are here, pure and tested; the launcher puts them together for the drive
//! (`omsi-launcher-api`'s `omsihub/ontime.rs`) and the plugin's `ontime.lua` shows them:
//!
//! - *punctuality* in seconds against the timetable, toned as the company counts a trip
//!   (`economy::LATE_FROM`: late from six minutes; more than `econ_uk::ON_TIME_EARLY` early is
//!   early): `tone`;
//! - *headway* on a frequent line (`FREQUENT`): the gap to the company's bus ahead and behind
//!   at the stop the bus comes to next, from the timetable and the day's one delay model
//!   (`lateness::model_with`, as the fleet map moves the buses), and when the bus has caught
//!   up with the one ahead how long to hold to even the gaps - never past the company's late
//!   line - or else that it runs close behind it: `headway`;
//! - *protected connections* of a line (`Connection`, kept on the line): a train or another
//!   line at a stop that the line's bus waits for, at the most its `max_hold` (`hold`), or one
//!   its passengers change to (`onward`). The connecting service runs a little late now and
//!   then (`late_of`: drawn from the day, the same every time); the day's delay model holds
//!   the company's buses for it where their trip begins (`model_hold`, `lateness::Cause::Hold`);
//! - *diversions and roadworks* of the day: the company's own (`Diversion`, entered on the
//!   Operations page: stops closed, the way round) and the inbox's events on the line
//!   (`disruptions`); a diversion slows the line's buses as an event does (`diversion_late`,
//!   `inbox::line_effect`), and the driver is told before the closed stops (`closed_ahead`).

use super::dates;
use super::econ_uk;
use super::economy;
use super::model::Company;
use super::network::PlannedTrip;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

// --- punctuality ----------------------------------------------------------------------------

/// More than this many seconds before the timetable is early (the company's on-time window).
pub const EARLY_FROM: f64 = econ_uk::ON_TIME_EARLY as f64 * 60.0;

/// From this many seconds behind the timetable a trip is late as the company counts it.
pub fn late_from() -> f64 {
    economy::LATE_FROM as f64 * 60.0
}

/// How a delay stands for the company.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// More than `EARLY_FROM` early: the passengers miss the bus.
    Early,
    OnTime,
    /// A minute or more behind, still on time for the company.
    Behind,
    /// Late from `late_from` on: the trip counts as late.
    Late,
}

/// The tone of `delay` seconds (late positive).
pub fn tone(delay: f64) -> Tone {
    if delay < -EARLY_FROM {
        Tone::Early
    } else if delay >= late_from() {
        Tone::Late
    } else if delay >= 60.0 {
        Tone::Behind
    } else {
        Tone::OnTime
    }
}

/// "+2:15", "−0:40".
pub fn offset(secs: f64) -> String {
    let a = secs.abs().round() as i64;
    format!("{}{}:{:02}", if secs < 0.0 { "−" } else { "+" }, a / 60, a % 60)
}

// --- headway --------------------------------------------------------------------------------

/// A line whose buses come at least this often (s) is frequent: its headway is shown.
pub const FREQUENT: f64 = 15.0 * 60.0;
/// The bus ahead closer than this share of the timetable's gap: the buses bunch.
pub const BUNCHED: f64 = 0.5;
/// A hold shorter than this (s) is not worth it; holds are given in steps of `HOLD_STEP`.
pub const MIN_HOLD: f64 = 15.0;
pub const HOLD_STEP: f64 = 5.0;
/// A hold keeps the bus this far (s) under the company's late line (`late_from`).
pub const LATE_MARGIN: f64 = 60.0;

/// A stop of a trip as the timetable has it: its name and its time (seconds of the day).
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub name: String,
    pub at: f64,
}

/// A trip of another bus: its stops, and how late it leaves and arrives (s), as the day's delay
/// model runs it (`fleetmap::Run`).
#[derive(Clone, Debug, PartialEq)]
pub struct OtherTrip {
    pub calls: Vec<Call>,
    pub dep: f64,
    pub arr: f64,
}

/// Another bus of the company on the line: what the driver calls it (its fleet number) and its
/// trips of the day.
#[derive(Clone, Debug, PartialEq)]
pub struct OtherBus {
    pub label: String,
    pub trips: Vec<OtherTrip>,
}

/// The gap to a bus (s).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Gap {
    pub bus: String,
    pub secs: f64,
}

/// What the driver is told to do about the gaps.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Advice {
    None,
    /// Hold this many seconds at the stop: the gaps even out.
    Hold { secs: f64 },
    /// Close behind this bus, and no time to hold.
    CloseBehind { bus: String },
}

/// The headway at the stop the bus comes to next.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Headway {
    pub ahead: Option<Gap>,
    pub behind: Option<Gap>,
    /// The timetable's gap there (s): the shorter of the gaps to the bus before and after.
    pub planned: f64,
    pub advice: Advice,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// When a trip is at its stop `k` as it runs (its delay grows evenly along the way) and as
/// planned.
fn at_call(t: &OtherTrip, k: usize) -> (f64, f64) {
    let planned = t.calls[k].at;
    let (a, b) = (t.calls.first().map_or(planned, |c| c.at), t.calls.last().map_or(planned, |c| c.at));
    let f = if b > a { ((planned - a) / (b - a)).clamp(0.0, 1.0) } else { 1.0 };
    (planned + t.dep + (t.arr - t.dep) * f, planned)
}

/// When `bus` passes `stop` going the same way (the stop after it `after`, or none: it is the
/// trip's last), as it runs and as planned - once a trip.
fn passings(bus: &OtherBus, stop: &str, after: Option<&str>) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for t in &bus.trips {
        for k in 0..t.calls.len() {
            if !same(&t.calls[k].name, stop) {
                continue;
            }
            let next = t.calls.get(k + 1).map(|c| c.name.as_str());
            let way = match (after, next) {
                (Some(a), Some(n)) => same(a, n),
                (None, None) => true,
                _ => false,
            };
            if way {
                out.push(at_call(t, k));
                break;
            }
        }
    }
    out
}

/// The headway at `stop` (the bus comes to it next; `after` the stop after it): the bus is due
/// there at `planned` (seconds of the day) and runs `delay` seconds behind; `others` the
/// company's other buses on the line. None where it does not apply: no bus before or after it
/// within `FREQUENT` on the timetable.
pub fn headway(stop: &str, after: Option<&str>, planned: f64, delay: f64, others: &[OtherBus]) -> Option<Headway> {
    let me = planned + delay;
    let mut ahead: Option<Gap> = None;
    let mut behind: Option<Gap> = None;
    let (mut before, mut later) = (f64::INFINITY, f64::INFINITY);
    for b in others {
        for (run, plan) in passings(b, stop, after) {
            if plan < planned {
                before = before.min(planned - plan);
            } else if plan > planned {
                later = later.min(plan - planned);
            }
            if run <= me && ahead.as_ref().is_none_or(|g| me - run < g.secs) {
                ahead = Some(Gap { bus: b.label.clone(), secs: me - run });
            } else if run > me && behind.as_ref().is_none_or(|g| run - me < g.secs) {
                behind = Some(Gap { bus: b.label.clone(), secs: run - me });
            }
        }
    }
    let gap = before.min(later);
    if gap > FREQUENT {
        return None;
    }
    let mut advice = Advice::None;
    if let Some(a) = ahead.as_ref().filter(|a| before.is_finite() && a.secs < before * BUNCHED) {
        // (the hold that evens the gaps before and after the bus, else gives it the timetable's)
        let even = match &behind {
            Some(b) => (b.secs - a.secs) / 2.0,
            None => before - a.secs,
        };
        let room = late_from() - LATE_MARGIN - delay;
        let secs = (even.min(room) / HOLD_STEP).floor() * HOLD_STEP;
        advice = if secs >= MIN_HOLD { Advice::Hold { secs } } else { Advice::CloseBehind { bus: a.bus.clone() } };
    }
    Some(Headway { ahead, behind, planned: gap, advice })
}

// --- connections ----------------------------------------------------------------------------

/// A connection due before a departure by up to this many minutes is still the bus's to wait
/// for (and one its passengers change to is reached within it after the bus comes in).
pub const CATCH: i32 = 10;
/// Seconds the passengers need to change.
pub const CHANGE: f64 = 60.0;

/// A protected connection of a line at one of its stops (the Lines page's table): what connects
/// ("Train from Erfurt", "Line 12"), when it is due at the stop, and how long the line's bus
/// waits for it at the most.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Connection {
    pub stop: String,
    pub what: String,
    /// When it is due at the stop (minute of the day).
    pub time: i32,
    /// The minutes the bus waits for it at the most (0: it does not wait).
    #[serde(default)]
    pub max_hold: i32,
    /// The bus's passengers change to it (it is not waited for: the driver is told whether
    /// they make it); else the bus waits for its passengers.
    #[serde(default)]
    pub onward: bool,
}

impl Connection {
    /// Whether the bus at its stop at `at` (seconds of the day, as planned: its departure, or
    /// for an onward connection its arrival) is the one it is for.
    pub fn protects(&self, at: f64) -> bool {
        let due = self.time as f64 * 60.0;
        if self.onward {
            at <= due && due <= at + CATCH as f64 * 60.0
        } else {
            at - CATCH as f64 * 60.0 <= due && due <= at + self.max_hold.max(0) as f64 * 60.0
        }
    }

    pub fn at(&self, stop: &str) -> bool {
        same(&self.stop, stop)
    }
}

/// A line's connections tidied as the Lines page saves them: those with a stop and a time of the
/// day, the hold within an hour, in the order of their time.
pub fn tidy(list: Vec<Connection>) -> Vec<Connection> {
    let mut out: Vec<Connection> = list
        .into_iter()
        .filter(|x| !x.stop.trim().is_empty() && (0..1440).contains(&x.time))
        .map(|x| Connection { stop: x.stop.trim().to_string(), what: x.what.trim().to_string(), max_hold: if x.onward { 0 } else { x.max_hold.clamp(0, 60) }, ..x })
        .collect();
    out.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| a.stop.cmp(&b.stop)));
    out
}

/// How many minutes the connection runs late on `date` (on line `line`): on time most days, a
/// few minutes now and then - drawn from the company, the line, the connection and the day, the
/// same every time.
pub fn late_of(c: &Company, date: &str, line: &str, x: &Connection) -> i32 {
    let day = dates::parse(date).unwrap_or(0);
    let mut r = Rng::of(&[&c.id, "connection", line.trim(), x.stop.trim(), x.what.trim()], day * 1440 + x.time as i64);
    if r.chance(0.6) {
        0
    } else {
        1 + (r.f64().powi(2) * 7.0) as i32
    }
}

/// What a bus does for a connection it waits for.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Hold {
    /// Its passengers are aboard before the bus leaves anyway.
    NotNeeded,
    /// It waits `secs` more, until `until` (seconds of the day).
    Wait { until: f64, secs: f64 },
    /// The connection comes after the most the bus may wait: it leaves without it.
    TooLate,
}

/// The hold for a connection due at `due` (seconds of the day, as it runs) at a stop the bus is
/// to leave at `dep` and is ready to leave at `ready` (its delay counted): it waits until the
/// connection's passengers are aboard (`CHANGE`), no later than `max_hold` seconds after `dep`.
pub fn hold(dep: f64, ready: f64, due: f64, max_hold: f64) -> Hold {
    let need = due + CHANGE;
    let leave = dep.max(ready);
    if need <= leave {
        Hold::NotNeeded
    } else if need > dep + max_hold.max(0.0) {
        Hold::TooLate
    } else {
        Hold::Wait { until: need, secs: need - leave }
    }
}

/// Whether the bus's passengers make a connection they change to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Onward {
    /// With `secs` to spare after changing.
    Made { secs: f64 },
    /// Less than a minute to spare.
    Tight { secs: f64 },
    Missed,
}

/// The bus at the stop at `arrive` (as it runs), the connection leaving at `due` (as it runs).
pub fn onward(arrive: f64, due: f64) -> Onward {
    let spare = due - arrive - CHANGE;
    if spare >= 60.0 {
        Onward::Made { secs: spare }
    } else if spare >= 0.0 {
        Onward::Tight { secs: spare }
    } else {
        Onward::Missed
    }
}

/// What the day's delay model holds a company bus for (`lateness::model_with`): a connection of
/// trip `x`'s line that it waits for where the trip begins, the bus `delay` minutes late there -
/// the stop, what it waited for and the minutes. (The stops on the way the model does not time;
/// the driver holds there by the overlay.)
pub fn model_hold(c: &Company, date: &str, line: &str, x: &PlannedTrip, delay: f64) -> Option<(String, String, f64)> {
    if !x.counts() || x.is_second() {
        return None;
    }
    let l = c.lines.iter().find(|l| same(&l.name, line))?;
    let dep = x.dep as f64 * 60.0;
    let mut out: Option<(String, String, f64)> = None;
    for k in l.connections.iter().filter(|k| !k.onward && k.max_hold > 0 && k.at(&x.from) && k.protects(dep)) {
        let due = (k.time + late_of(c, date, line, k)) as f64 * 60.0;
        if let Hold::Wait { secs, .. } = hold(dep, dep + delay * 60.0, due, k.max_hold as f64 * 60.0) {
            if out.as_ref().is_none_or(|o| secs / 60.0 > o.2) {
                out = Some((k.stop.clone(), k.what.clone(), secs / 60.0));
            }
        }
    }
    out
}

// --- diversions -----------------------------------------------------------------------------

/// What a diversion adds to the chance a trip of its line runs late (as `inbox`'s diversion
/// event does, between its 6 and 12 %).
pub const DIVERSION_LATE: f64 = 0.08;
/// The driver is told of a closed stop this many stops before it.
pub const HEADS_UP: usize = 3;
/// A diversion is kept this many days after its last.
const KEPT_DAYS: i64 = 7;

/// A diversion of the company's (the Operations page): from a day to a day, on some of its lines
/// (none named: all), the stops it closes and the way the buses go round.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Diversion {
    pub id: u64,
    /// Its first day and its last ("": the first only).
    pub from: String,
    #[serde(default)]
    pub until: String,
    /// The company's lines it is on (their timetable names).
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub closed: Vec<String>,
    /// The way round ("via Bahnhofstraße").
    #[serde(default)]
    pub via: String,
    #[serde(default)]
    pub note: String,
}

impl Diversion {
    pub fn last(&self) -> &str {
        if self.until.trim().is_empty() { &self.from } else { &self.until }
    }

    pub fn on(&self, date: &str) -> bool {
        self.from.as_str() <= date && date <= self.last()
    }

    pub fn on_line(&self, line: &str) -> bool {
        self.lines.is_empty() || self.lines.iter().any(|l| same(l, line))
    }

    pub fn closes(&self, stop: &str) -> bool {
        self.closed.iter().any(|s| same(s, stop))
    }
}

/// The company's diversions, kept with it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct OnTime {
    #[serde(default)]
    pub diversions: Vec<Diversion>,
    #[serde(default)]
    pub next_id: u64,
}

/// Add a diversion (its id given here); those over for a week go. Err: why not.
pub fn add_diversion(c: &mut Company, d: Diversion) -> Result<u64, &'static str> {
    let (Some(from), until) = (dates::parse(d.from.trim()), d.until.trim()) else { return Err("Give the diversion its day.") };
    if !until.is_empty() && dates::parse(until).is_none_or(|u| u < from) {
        return Err("The diversion's last day is before its first.");
    }
    let closed: Vec<String> = d.closed.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if closed.is_empty() && d.via.trim().is_empty() {
        return Err("Name the stops closed or the way round.");
    }
    if d.lines.iter().any(|l| !c.lines.iter().any(|x| same(&x.name, l))) {
        return Err("The company does not run that line.");
    }
    let today = dates::parse(&c.date).unwrap_or(from);
    c.ontime.diversions.retain(|x| dates::parse(x.last()).is_some_and(|l| l + KEPT_DAYS >= today));
    c.ontime.next_id += 1;
    let id = c.ontime.next_id;
    c.ontime.diversions.push(Diversion { id, from: d.from.trim().to_string(), until: until.to_string(), lines: d.lines, closed, via: d.via.trim().to_string(), note: d.note.trim().to_string() });
    Ok(id)
}

/// Take a diversion away; whether there was one.
pub fn remove_diversion(c: &mut Company, id: u64) -> bool {
    let n = c.ontime.diversions.len();
    c.ontime.diversions.retain(|d| d.id != id);
    c.ontime.diversions.len() < n
}

/// What the company's diversions add to the chance a trip of the line shown as `number` runs
/// late on `date` (`inbox::line_effect`).
pub fn diversion_late(c: &Company, date: &str, number: &str) -> f64 {
    let lines: Vec<&str> = c.lines.iter().filter(|l| same(&l.number, number) || l.numbers.iter().any(|n| same(n, number))).map(|l| l.name.as_str()).collect();
    let on = c.ontime.diversions.iter().any(|d| d.on(date) && (d.lines.is_empty() || lines.iter().any(|l| d.on_line(l))));
    if on { DIVERSION_LATE } else { 0.0 }
}

/// A disruption of a line's day the driver is told of.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Disruption {
    /// Shown once ("div-3", "event-17").
    pub id: String,
    /// "diversion" (the company's), or the inbox's event ("roadworks", "diversion").
    pub kind: String,
    pub closed: Vec<String>,
    pub via: String,
    pub note: String,
}

/// The disruptions of company line `line` on `date`: the company's diversions on it, and the
/// inbox's roadworks and diversions on its number.
pub fn disruptions(c: &Company, date: &str, line: &str) -> Vec<Disruption> {
    let mut out: Vec<Disruption> = c
        .ontime
        .diversions
        .iter()
        .filter(|d| d.on(date) && d.on_line(line))
        .map(|d| Disruption { id: format!("div-{}", d.id), kind: "diversion".into(), closed: d.closed.clone(), via: d.via.clone(), note: d.note.clone() })
        .collect();
    let numbers: Vec<&str> = c.lines.iter().filter(|l| same(&l.name, line)).flat_map(|l| std::iter::once(l.number.as_str()).chain(l.numbers.iter().map(String::as_str))).collect();
    for e in super::inbox::running(c, date).filter(|e| e.key == "roadworks" || e.key == "diversion") {
        if e.line.as_deref().is_some_and(|n| numbers.iter().any(|x| same(x, n))) {
            out.push(Disruption { id: format!("event-{}", e.id), kind: e.key.clone(), closed: Vec::new(), via: String::new(), note: String::new() });
        }
    }
    out
}

/// The stops closed on a line's day (`disruptions`).
pub fn closed_of(list: &[Disruption]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in list.iter().flat_map(|d| d.closed.iter()) {
        if !out.iter().any(|x| same(x, s)) {
            out.push(s.clone());
        }
    }
    out
}

/// The first stop of `calls` closed (`closed`) from the bus's next stop `next` on and at most
/// `HEADS_UP` stops ahead: where the driver is told before.
pub fn closed_ahead(calls: &[Call], next: usize, closed: &[String]) -> Option<usize> {
    (next..calls.len().min(next + HEADS_UP + 1)).find(|&k| closed.iter().any(|s| same(s, &calls[k].name)))
}

#[cfg(test)]
#[path = "ontime_tests.rs"]
mod tests;

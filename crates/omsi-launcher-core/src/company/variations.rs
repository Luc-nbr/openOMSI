//! Contract variations: a tendered line's bus contract changed by agreement with its authority,
//! for a fee or a penalty by the size of the change (Luc 2026-10-16: "a won tendered route
//! changed for a fee, or a payment, depending on the size of the change").
//!
//! # What may change
//!
//! A gross-cost or a net-subsidy contract (`bus_contracts::Kind`) - a line of the map's won in a
//! tender - may be changed four ways (`Ask`):
//!
//! * **the timetable**: journeys a day more or fewer (the frequency), the operating day begun
//!   earlier or ended later (its first and last trips), the running time of a journey;
//! * **the route**: stops added or removed, a diversion (km a journey more or fewer), a core
//!   stop - a terminus - given up;
//! * **the days of operation** a week;
//! * **the vehicle type** (a bigger bus costs more to run, a smaller one less: `size_cost`).
//!
//! The line editor's save of such a line is priced the same way (`edit_quote`): the change of
//! its kilometres and hours a week, its stops and termini, measured on the line (`measure`).
//!
//! # The price (`quote`)
//!
//! A change's size is the larger of its shares of the contract's service kilometres and hours a
//! year (a vehicle type: its share of the cost). The minor changes agreed in the twelve months
//! before are added to it: the authority does not let a large change through in small steps.
//! Then, within the game mode's bounds (`lenience`: Realistic 5 % and 25 %, Normal 7.5 % and
//! 33 %, Casual 10 % and 40 %):
//!
//! * **Minor** (at most the lower bound, or a retiming of at most `RETIME_MINOR` minutes a
//!   journey): an admin fee (`ADMIN_FEE`), the payment as it is.
//! * **Moderate**: a variation fee (`VARIATION_FEE`), and the payment adjusted pro rata
//!   (`adjusted`): by the change of the authority's reckoning of the work's price
//!   (`bus_contracts::price_of`), at the contract's own rate against that reckoning. A cut
//!   lowers it, more service raises it. A gross-cost contract's payment follows the work's cost;
//!   a net subsidy follows the cost less the fares and BSOG the operator keeps (the revenue risk
//!   stays the operator's).
//! * **Major** (over the upper bound, or a terminus given up): as a moderate change, and a
//!   penalty of `PENALTY_SHARE` of the contract's value still to come, `POINTS_MAJOR` default
//!   points on the contract (by the mode's factor: its rating, and the track record the next
//!   tenders score) and `REPUTATION_MAJOR` points of reputation. The authority refuses a change
//!   over its limit (Realistic 50 %, Normal 75 %, Casual none), and a major change of a red
//!   contract where the mode lets it end contracts.
//!
//! The fees are UK pounds at 2022 prices, by the company's country and its prices since its
//! founding (Normal half, Casual none); the penalty by the mode (Normal 60 %, Casual 25 %) and the
//! sandbox's knob on penalties.
//!
//! # An offer (`offer`)
//!
//! Instead of the price asked (the fee and the penalty) the company may offer a sum. The
//! authority takes it when it is at least the price asked times `need`:
//!
//! ```text
//! need = class (minor 0.5, moderate 0.6, major 0.85)
//!      + rating (green -0.1, orange +0.1, red +0.3)
//!      + 0.01 x the track record's average net default points      (within 0.3 and 1.5)
//! ```
//!
//! - a green contract of a company with a clean record gets away cheaper. It answers one offer a
//! week on a contract (`OFFER_DAYS`); a change it refuses outright is not for sale.
//!
//! # Agreed, and run
//!
//! A change agreed on the contracts' pages is paid at once and noted in the contract's history
//! (`Variation`). A new vehicle type and a cut change the payment at once; more service raises
//! it once the line runs it: the line editor's save that makes the change (`Agreed`: a save
//! within the lower bound of the change agreed costs nothing more). A save of the line without a
//! change agreed is priced and agreed as it is saved. A line of the map's put back as the map has
//! it takes its contract back with it, without a fee (the payment follows the work).

use super::bus_contracts::{self as bc, Contract, Kind, PointsDay, Rag, Work};
use super::dates;
use super::econ_uk;
use super::model::{BookingKind, BusSize, Cents, Company, GameMode};
use super::reputation::{self, Cause};
use crate::lines::{self, LineDesign};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// --- the rules -------------------------------------------------------------------------------------

/// The admin fee of a minor change, and the fee of a moderate or major one (GBP pence, 2022).
pub const ADMIN_FEE: f64 = 25_000.0;
pub const VARIATION_FEE: f64 = 100_000.0;
/// A major change's penalty: this share of the contract's value still to come.
pub const PENALTY_SHARE: f64 = 0.10;
/// A major change's default points (before the mode's factor) and reputation.
pub const POINTS_MAJOR: f64 = 10.0;
pub const REPUTATION_MAJOR: f64 = 2.0;
/// A retiming of at most these minutes a journey is minor, whatever its share.
pub const RETIME_MINOR: i32 = 5;
/// The minutes a stop adds to a journey (braking, the doors, pulling out).
pub const STOP_MINUTES: f64 = 0.5;
/// Days between two offers on a contract.
pub const OFFER_DAYS: i64 = 7;

/// How a game mode's authority treats a change: the classes' bounds (shares of the service), the
/// share over which it refuses (None: never), and the factors on the fees and the penalty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lenience {
    pub minor: f64,
    pub major: f64,
    pub refuse: Option<f64>,
    pub fees: f64,
    pub penalty: f64,
}

pub fn lenience(m: GameMode) -> Lenience {
    match m {
        GameMode::Realistic => Lenience { minor: 0.05, major: 0.25, refuse: Some(0.50), fees: 1.0, penalty: 1.0 },
        GameMode::Normal => Lenience { minor: 0.075, major: 0.33, refuse: Some(0.75), fees: 0.5, penalty: 0.6 },
        GameMode::Casual => Lenience { minor: 0.10, major: 0.40, refuse: None, fees: 0.0, penalty: 0.25 },
    }
}

/// What a type of bus costs to run against the standard 12 m bus (the vehicle, its fuel and its
/// upkeep; the driver costs the same).
pub fn size_cost(s: BusSize) -> f64 {
    match s {
        BusSize::Midi => 0.92,
        BusSize::Solo => 1.0,
        BusSize::Articulated => 1.18,
        BusSize::Double => 1.22,
    }
}

// --- a change --------------------------------------------------------------------------------------

/// A change asked for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ask {
    /// `journeys` a day more (fewer: negative), each bus's day `span` minutes longer (its first
    /// trip earlier, its last later; shorter: negative), each journey `retime` minutes longer.
    Timetable {
        #[serde(default)]
        journeys: i32,
        #[serde(default)]
        span: i32,
        #[serde(default)]
        retime: i32,
    },
    /// Stops `added` and `removed`, `km` a journey more (a diversion; shorter: negative), and a
    /// core stop - a terminus - given up.
    Route {
        #[serde(default)]
        added: u32,
        #[serde(default)]
        removed: u32,
        #[serde(default)]
        km: f64,
        #[serde(default)]
        core: bool,
    },
    /// The operating days a week.
    Days { days: u32 },
    /// The type of bus.
    Vehicle { size: BusSize },
    /// The line editor's save: the line's kilometres and hours a week by these factors, its stops.
    Edit { km: f64, hours: f64, added: u32, removed: u32, core: bool },
}

impl Ask {
    pub fn change(&self) -> Change {
        match self {
            Ask::Timetable { .. } => Change::Timetable,
            Ask::Route { .. } => Change::Route,
            Ask::Days { .. } => Change::Days,
            Ask::Vehicle { .. } => Change::Vehicle,
            Ask::Edit { .. } => Change::Edit,
        }
    }

    /// Its stops added and removed, and whether a core stop goes.
    fn stops(&self) -> (u32, u32, bool) {
        match *self {
            Ask::Route { added, removed, core, .. } | Ask::Edit { added, removed, core, .. } => (added, removed, core),
            _ => (0, 0, false),
        }
    }
}

/// What kind of change a variation was.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    #[default]
    Timetable,
    Route,
    Days,
    Vehicle,
    /// Saved in the line editor.
    Edit,
    /// The map's own line put back.
    Restore,
}

impl Change {
    pub fn label(self) -> &'static str {
        match self {
            Change::Timetable => "Timetable",
            Change::Route => "Route",
            Change::Days => "Days of operation",
            Change::Vehicle => "Vehicle type",
            Change::Edit => "Line editor",
            Change::Restore => "The map's line back",
        }
    }
}

/// How large a change is, as the authority prices it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    #[default]
    Minor,
    Moderate,
    Major,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Minor => "Minor",
            Class::Moderate => "Moderate",
            Class::Major => "Major",
        }
    }
}

/// A variation agreed: the contract's history.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Variation {
    pub date: String,
    pub change: Change,
    pub class: Class,
    /// Its size: the share of the service (more positive, less negative; a vehicle type: of the
    /// cost).
    pub share: f64,
    pub fee: Cents,
    pub penalty: Cents,
    /// The year's payment before and after (cents, at the day's contract index).
    pub year_before: Cents,
    pub year_after: Cents,
    pub points: f64,
    pub reputation: f64,
    /// Settled by the company's offer.
    pub offer: bool,
    /// The raise waits for the line to run the change (`Agreed`).
    pub waiting: bool,
}

/// A change agreed on the contracts' pages that the line does not run yet: the factors of its
/// kilometres and hours a year, and the year's payment once it runs (a raise; None: the payment
/// changed when it was agreed).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Agreed {
    pub date: String,
    pub km: f64,
    pub hours: f64,
    pub year: Option<Cents>,
}

/// A change priced: what it changes, its class, and what it costs.
#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub change: Change,
    pub kind: Kind,
    pub class: Class,
    /// Why the authority will not have it (None: it may be agreed).
    pub refused: Option<&'static str>,
    /// Nothing would change.
    pub nothing: bool,
    /// The work before and after, and the bus types.
    pub before: Work,
    pub after: Work,
    pub sizes: (BusSize, BusSize),
    pub added: u32,
    pub removed: u32,
    pub core: bool,
    /// Its size (signed), and the size its class was decided on (with the year's minor ones).
    pub share: f64,
    pub counted: f64,
    pub fee: Cents,
    pub penalty: Cents,
    /// The year's payment and the week's, now and after.
    pub year_before: Cents,
    pub year_after: Cents,
    /// The raise waits for the line to run the change.
    pub waits: bool,
    pub points: f64,
    pub reputation: f64,
    /// A change agreed before, as the line editor saves it: nothing more to pay.
    pub covered: bool,
}

impl Quote {
    /// What the authority asks: the fee and the penalty.
    pub fn price(&self) -> Cents {
        self.fee + self.penalty
    }

    pub fn week_before(&self) -> Cents {
        (self.year_before as f64 / 52.0).round() as Cents
    }

    pub fn week_after(&self) -> Cents {
        (self.year_after as f64 / 52.0).round() as Cents
    }

    /// Kilometres and hours a year, before and after.
    pub fn km_year(&self) -> (f64, f64) {
        (year_km(&self.before), year_km(&self.after))
    }

    pub fn hours_year(&self) -> (f64, f64) {
        (year_hours(&self.before), year_hours(&self.after))
    }
}

fn year_km(w: &Work) -> f64 {
    w.km * w.days as f64
}

fn year_hours(w: &Work) -> f64 {
    w.hours * w.days as f64
}

// --- pricing ---------------------------------------------------------------------------------------

/// The live contract of a line that may be changed: a gross-cost or a net-subsidy one.
pub fn changeable<'a>(c: &'a Company, line: &str) -> Option<&'a Contract> {
    bc::of_line(c, line).filter(|k| matches!(k.kind, Kind::Gross | Kind::Net))
}

/// The work a contract runs: as agreed, else (an older file) its tender's, else its line's day
/// roughly - at the contract's own days and journeys.
pub fn work_of(c: &Company, k: &Contract) -> Work {
    if let Some(w) = k.work.filter(|w| w.km > 0.0 && w.hours > 0.0) {
        return w;
    }
    let tender = c.concessions.tenders.iter().rev().find(|t| t.line.eq_ignore_ascii_case(&k.line) && t.work.km > 0.0 && t.work.hours > 0.0).map(|t| t.work);
    let w = tender.unwrap_or_else(|| match c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&k.line)) {
        Some(l) => bc::rough_work(l.tours, l.km),
        None => bc::rough_work(1, 20.0),
    });
    Work { days: k.days_a_year.max(1), journeys: if k.journeys > 0 { k.journeys } else { w.journeys }, ..w }
}

/// The work after a change.
fn after_of(before: &Work, ask: &Ask) -> Work {
    let mut w = *before;
    let j = before.journeys.max(1) as f64;
    let (per_hours, per_km) = (before.hours / j, before.km / j);
    let speed = if before.hours > 0.0 { before.km / before.hours } else { 20.0 };
    match *ask {
        Ask::Timetable { journeys, span, retime } => {
            let mut n = (j + journeys as f64).max(1.0);
            // (each bus's day longer: its hours, and the journeys and km they hold)
            let extra = before.buses.max(1) as f64 * span as f64 / 60.0;
            let more = if per_hours > 0.0 { extra / per_hours } else { 0.0 };
            n = (n + more).max(1.0);
            w.journeys = n.round().max(1.0) as u32;
            w.km = per_km * n;
            w.hours = per_hours * n + n * retime as f64 / 60.0;
        }
        Ask::Route { added, removed, km, .. } => {
            w.km = before.km + j * km;
            w.hours = before.hours + j * km / speed.max(1.0) + j * (added as f64 - removed as f64) * STOP_MINUTES / 60.0;
        }
        Ask::Days { days } => w.days = days.clamp(1, 7) * 52,
        Ask::Vehicle { .. } => {}
        Ask::Edit { km, hours, .. } => {
            w.km = before.km * km;
            w.hours = before.hours * hours;
            w.journeys = (j * km).round().max(1.0) as u32;
        }
    }
    w.km = w.km.max(0.1);
    w.hours = w.hours.max(0.1);
    // (a longer day needs more buses at its peak, a shorter one fewer)
    if before.hours > 0.0 {
        w.buses = (before.buses.max(1) as f64 * w.hours / before.hours).round().max(1.0) as u32;
    }
    w
}

/// The authority's reckoning of a work's price a year with buses of `size` (cents): its cost by
/// the bus's, a net subsidy less the fares and BSOG the operator keeps.
fn priced(c: &Company, k: &Contract, w: &Work, size: BusSize) -> f64 {
    let years = k.years();
    let gross = bc::price_of(c, w, Kind::Gross, years);
    let kept = if k.kind == Kind::Net { (gross - bc::price_of(c, w, Kind::Net, years)).max(0.0) } else { 0.0 };
    (gross * size_cost(size) - kept).max(0.0)
}

/// The year's payment after a change pro rata (cents now): the contract's price now, moved by the
/// change of the authority's reckoning of the work's price, at the contract's rate against it (its
/// price over that reckoning, within a half and one and a half).
pub fn adjusted(c: &Company, k: &Contract, before: &Work, after: &Work, sizes: (BusSize, BusSize)) -> f64 {
    let now = k.year_now(c.contract_index);
    let (old, new) = (priced(c, k, before, sizes.0), priced(c, k, after, sizes.1));
    let rate = if old > 1.0 { (now / old).clamp(0.5, 1.5) } else { 1.0 };
    (now + rate * (new - old)).max(0.0)
}

/// A fee of `pence` (GBP, 2022) for the company: by its country and its prices, and its mode.
fn fee_of(c: &Company, pence: f64) -> Cents {
    let x = econ_uk::gbp(pence) * econ_uk::country_of(c).general * c.price_index * lenience(c.game_mode).fees;
    ((x / 100.0).round() * 100.0) as Cents
}

/// The contract's value still to come (cents): its year's payment over the days left of its term.
fn remaining(c: &Company, k: &Contract) -> f64 {
    let from = if dates::between(&c.date, &k.from) > 0 { k.from.clone() } else { c.date.clone() };
    let days = (dates::between(&from, &k.until) + 1).max(0) as f64;
    k.year_now(c.contract_index) * days / 365.0
}

/// The size of a change from `before` to `after`: the larger (by its size) of the shares the
/// kilometres and the hours a year change by.
fn share_of(before: &Work, after: &Work) -> f64 {
    let ratio = |a: f64, b: f64| if a > 0.0 { b / a - 1.0 } else { 0.0 };
    let (k, h) = (ratio(year_km(before), year_km(after)), ratio(year_hours(before), year_hours(after)));
    if k.abs() >= h.abs() {
        k
    } else {
        h
    }
}

/// The price of a change of a line's contract (`ask`), as the authority would agree it today.
pub fn quote(c: &Company, line: &str, ask: &Ask) -> Result<Quote, &'static str> {
    let k = changeable(c, line).ok_or("This line has no tendered contract to change.")?;
    Ok(price(c, k, ask))
}

fn price(c: &Company, k: &Contract, ask: &Ask) -> Quote {
    let l = lenience(c.game_mode);
    let before = work_of(c, k);
    let after = after_of(&before, ask);
    let size_now = k.vehicle.unwrap_or(BusSize::Solo);
    let size_after = match ask {
        Ask::Vehicle { size } => *size,
        _ => size_now,
    };
    let (added, removed, core) = ask.stops();
    let share = match ask {
        Ask::Vehicle { .. } => size_cost(size_after) / size_cost(size_now) - 1.0,
        _ => share_of(&before, &after),
    };
    let size = share.abs();
    let nothing = size < 1e-3 && added == 0 && removed == 0 && !core;
    // (the minor changes of the twelve months before count with it)
    let year_ago = dates::add(&c.date, -econ_uk::POINTS_DAYS);
    let earlier: f64 = k.variations.iter().filter(|v| v.class == Class::Minor && dates::between(&year_ago, &v.date) >= 0).map(|v| v.share.abs()).sum();
    let retime = matches!(*ask, Ask::Timetable { journeys: 0, span: 0, retime } if retime.abs() <= RETIME_MINOR);
    let counted = if retime { size } else { size + earlier };
    let class = if core || counted > l.major + 1e-9 {
        Class::Major
    } else if retime || counted <= l.minor + 1e-9 {
        Class::Minor
    } else {
        Class::Moderate
    };
    let year_before = k.year_now(c.contract_index).round() as Cents;
    let year_after = match class {
        Class::Minor => year_before,
        _ => adjusted(c, k, &before, &after, (size_now, size_after)).round() as Cents,
    };
    let fee = if nothing {
        0
    } else {
        fee_of(c, if class == Class::Minor { ADMIN_FEE } else { VARIATION_FEE })
    };
    let major = class == Class::Major && !nothing;
    let penalty = if major { (PENALTY_SHARE * l.penalty * super::sandbox::of(c).penalties * remaining(c, k) / 100.0).round() as Cents * 100 } else { 0 };
    let points = if major { POINTS_MAJOR * bc::strictness(c) } else { 0.0 };
    let reputation = if major { REPUTATION_MAJOR * reputation::mode(c.game_mode).loss } else { 0.0 };
    let refused = if l.refuse.is_some_and(|r| size > r + 1e-9) {
        Some("The authority refuses a change this large: it would put the line out to tender again.")
    } else if major && econ_uk::mode(c.game_mode).may_terminate && k.rag(&c.date) == Rag::Red {
        Some("The authority agrees no major change while the contract is red.")
    } else {
        None
    };
    // (agreed on the contracts' pages, the line still to be changed: a raise waits for it)
    let waits = matches!(ask, Ask::Timetable { .. } | Ask::Route { .. } | Ask::Days { .. }) && year_after > year_before;
    Quote { change: ask.change(), kind: k.kind, class, refused, nothing, before, after, sizes: (size_now, size_after), added, removed, core, share, counted, fee, penalty, year_before, year_after, waits, points, reputation, covered: false }
}

// --- agreeing --------------------------------------------------------------------------------------

/// Agree a change of a line's contract at the authority's price: paid, the contract's terms
/// changed, its history noted. Returns the variation.
pub fn agree(c: &mut Company, line: &str, ask: &Ask) -> Result<Variation, &'static str> {
    let q = quote(c, line, ask)?;
    settle(c, line, ask, &q, None)
}

/// A change settled: `paid` instead of the price asked (an offer taken).
fn settle(c: &mut Company, line: &str, ask: &Ask, q: &Quote, paid: Option<Cents>) -> Result<Variation, &'static str> {
    if q.nothing {
        return Err("Nothing changes: choose what the change is.");
    }
    if let Some(r) = q.refused {
        return Err(r);
    }
    let (fee, penalty) = match paid {
        Some(x) => (x.max(0), 0),
        None => (q.fee, q.penalty),
    };
    if !c.can_pay(fee + penalty) {
        return Err("The company cannot pay for the change: take a loan on the Finances page.");
    }
    let id = changeable(c, line).ok_or("This line has no tendered contract to change.")?.id;
    let agreed = (!matches!(ask, Ask::Vehicle { .. } | Ask::Edit { .. })).then(|| Agreed {
        date: c.date.clone(),
        km: ratio(q.km_year()),
        hours: ratio(q.hours_year()),
        year: q.waits.then_some(q.year_after),
    });
    let size = match ask {
        Ask::Vehicle { size } => Some(*size),
        _ => None,
    };
    Ok(commit(c, id, q, fee, penalty, paid.is_some(), agreed, size))
}

fn ratio((a, b): (f64, f64)) -> f64 {
    if a > 0.0 {
        b / a
    } else {
        1.0
    }
}

/// A variation into the contract `id`: booked, its terms, its history, its points and the
/// reputation.
#[allow(clippy::too_many_arguments)]
fn commit(c: &mut Company, id: u32, q: &Quote, fee: Cents, penalty: Cents, offer: bool, agreed: Option<Agreed>, size: Option<BusSize>) -> Variation {
    let date = c.date.clone();
    let number = c.concessions.contracts.iter().find(|k| k.id == id).map(|k| k.number.clone()).unwrap_or_default();
    if fee > 0 {
        c.book(BookingKind::Concession, -fee, format!("Line {number}: contract variation agreed"), false);
    }
    if penalty > 0 {
        c.book(BookingKind::Penalty, -penalty, format!("Line {number}: contract variation, the penalty"), false);
    }
    let v = Variation {
        date: date.clone(),
        change: q.change,
        class: q.class,
        share: q.share,
        fee,
        penalty,
        year_before: q.year_before,
        year_after: q.year_after,
        points: q.points,
        reputation: q.reputation,
        offer,
        waiting: q.waits && agreed.is_some(),
    };
    let index = c.contract_index;
    if let Some(k) = c.concessions.contracts.iter_mut().find(|k| k.id == id) {
        k.work = Some(q.after);
        k.days_a_year = q.after.days.max(1);
        k.journeys = q.after.journeys.max(1);
        if let Some(s) = size {
            k.vehicle = Some(s);
        }
        if !v.waiting && q.year_after != q.year_before {
            k.year_price = q.year_after;
            k.index = index;
        }
        k.agreed = agreed;
        if q.points > 0.0 {
            k.record.push(PointsDay { date: date.clone(), default: q.points, ..Default::default() });
        }
        k.variations.push(v.clone());
    }
    if q.reputation > 0.0 {
        reputation::change(c, &date, Cause::Variation, -REPUTATION_MAJOR, 1);
    }
    v
}

// --- an offer --------------------------------------------------------------------------------------

/// The share of the price asked the authority takes as an offer (see the module's text).
pub fn need(c: &Company, k: &Contract, class: Class) -> f64 {
    let base = match class {
        Class::Minor => 0.5,
        Class::Moderate => 0.6,
        Class::Major => 0.85,
    };
    let rag = match k.rag(&c.date) {
        Rag::Green => -0.1,
        Rag::Orange => 0.1,
        Rag::Red => 0.3,
    };
    (base + rag + 0.01 * bc::scored_points(c)).clamp(0.3, 1.5)
}

/// The day the company may make its next offer on a line's contract (None: today).
pub fn next_offer(c: &Company, k: &Contract) -> Option<String> {
    if k.offered.is_empty() {
        return None;
    }
    let next = dates::add(&k.offered, OFFER_DAYS);
    (dates::between(&c.date, &next) > 0).then_some(next)
}

/// Offer `amount` (cents) for a change instead of the price asked: the authority takes it
/// (Some: the variation agreed) or refuses it (None: nothing paid). One offer a week a contract.
pub fn offer(c: &mut Company, line: &str, ask: &Ask, amount: Cents) -> Result<Option<Variation>, &'static str> {
    let q = quote(c, line, ask)?;
    if q.nothing {
        return Err("Nothing changes: choose what the change is.");
    }
    if let Some(r) = q.refused {
        return Err(r);
    }
    let k = changeable(c, line).ok_or("This line has no tendered contract to change.")?;
    if next_offer(c, k).is_some() {
        return Err("The authority answers one offer a week on a contract.");
    }
    if amount < 0 {
        return Err("An offer cannot be less than nothing.");
    }
    let need = need(c, k, q.class);
    let id = k.id;
    let today = c.date.clone();
    if let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) {
        x.offered = today;
    }
    if (amount as f64) + 0.5 < q.price() as f64 * need {
        return Ok(None);
    }
    settle(c, line, ask, &q, Some(amount)).map(Some)
}

// --- the line editor -------------------------------------------------------------------------------

/// A line as the line editor has it, measured: its kilometres and hours a week (its tours of each
/// day group, working days five times, Saturday and Sunday once), its stops and its termini.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Measure {
    pub km: f64,
    pub hours: f64,
    /// Its stops by direction (the direction's place, the stop), and its termini.
    pub stops: HashSet<(usize, i64)>,
    pub termini: HashSet<i64>,
}

/// The days a week of each day group (`lines::DAY_GROUPS`).
const GROUP_DAYS: [f64; 3] = [5.0, 1.0, 1.0];

pub fn measure(l: &LineDesign) -> Measure {
    let dirs: Vec<&lines::Direction> = l.directions.iter().filter(|d| d.stops.len() >= 2).collect();
    let km: Vec<f64> = dirs.iter().map(|d| d.legs.iter().map(|g| g.length as f64).sum::<f64>() / 1000.0).collect();
    let run = lines::run_minutes(l);
    let mut m = Measure::default();
    for t in lines::tour_plan(l) {
        let days = GROUP_DAYS.get(t.day).copied().unwrap_or(1.0);
        for p in &t.trips {
            m.km += km.get(p.dir).copied().unwrap_or(0.0) * days;
            m.hours += p.minutes(&run) as f64 / 60.0 * days;
        }
    }
    for (k, d) in l.directions.iter().enumerate() {
        m.stops.extend(d.stops.iter().map(|s| (k, s.id)));
        m.termini.extend(d.stops.first().map(|s| s.id));
        m.termini.extend(d.stops.last().map(|s| s.id));
    }
    m
}

/// The change a save makes of a line (`old` as it runs, `new` as saved), as an ask (None: none
/// to price - no service measured, or the same).
pub fn edit_ask(old: &LineDesign, new: &LineDesign) -> Option<Ask> {
    let (a, b) = (measure(old), measure(new));
    if a.km <= 0.0 || a.hours <= 0.0 {
        return None;
    }
    let added = b.stops.difference(&a.stops).count() as u32;
    let removed = a.stops.difference(&b.stops).count() as u32;
    let core = a.termini.iter().any(|t| !b.stops.iter().any(|s| s.1 == *t));
    let (km, hours) = (b.km / a.km, b.hours / a.hours);
    if (km - 1.0).abs() < 1e-3 && (hours - 1.0).abs() < 1e-3 && added == 0 && removed == 0 {
        return None;
    }
    Some(Ask::Edit { km, hours, added, removed, core })
}

/// The price of saving a change of a line of the map's that runs under a tendered contract (None:
/// no such contract, or nothing measured changes). A change agreed before (`Agreed`) is what the
/// save may do: one within the lower bound of it is covered, one beyond it priced on what is more.
pub fn edit_quote(c: &Company, name: &str, old: &LineDesign, new: &LineDesign) -> Option<Quote> {
    let k = changeable(c, name)?;
    let Some(Ask::Edit { km, hours, added, removed, core }) = edit_ask(old, new) else { return None };
    let Some(a) = k.agreed.as_ref() else { return Some(price(c, k, &Ask::Edit { km, hours, added, removed, core })) };
    let (rk, rh) = (km / a.km.max(1e-6), hours / a.hours.max(1e-6));
    let l = lenience(c.game_mode);
    if !core && (rk - 1.0).abs() <= l.minor && (rh - 1.0).abs() <= l.minor {
        let mut q = price(c, k, &Ask::Edit { km: 1.0, hours: 1.0, added, removed, core: false });
        q.after = q.before;
        q.covered = true;
        (q.class, q.fee, q.penalty, q.points, q.reputation, q.share, q.counted, q.refused, q.nothing) = (Class::Minor, 0, 0, 0.0, 0.0, 0.0, 0.0, None, false);
        q.year_after = a.year.unwrap_or(q.year_before);
        return Some(q);
    }
    // (beyond it: what is more priced, the raise agreed with it)
    let mut q = price(c, k, &Ask::Edit { km: rk, hours: rh, added, removed, core });
    q.year_after += a.year.unwrap_or(q.year_before) - q.year_before;
    Some(q)
}

/// A save of a line of the map's priced by `edit_quote` agreed: paid (the route's approval is
/// `ownline`'s), the contract's terms as the line runs now, any change agreed before done.
pub fn settle_edit(c: &mut Company, name: &str, q: &Quote) -> Result<Option<Variation>, &'static str> {
    let Some(id) = changeable(c, name).map(|k| k.id) else { return Ok(None) };
    if q.covered {
        let index = c.contract_index;
        if let Some(k) = c.concessions.contracts.iter_mut().find(|k| k.id == id) {
            if let Some(v) = k.variations.iter_mut().rev().find(|v| v.waiting) {
                v.waiting = false;
            }
            if q.year_after != q.year_before {
                k.year_price = q.year_after;
                k.index = index;
            }
            k.agreed = None;
        }
        return Ok(None);
    }
    if let Some(r) = q.refused {
        return Err(r);
    }
    if q.nothing {
        return Ok(None);
    }
    if let Some(k) = c.concessions.contracts.iter_mut().find(|k| k.id == id) {
        if let Some(v) = k.variations.iter_mut().rev().find(|v| v.waiting) {
            v.waiting = false;
        }
    }
    Ok(Some(commit(c, id, q, q.fee, q.penalty, false, None, None)))
}

/// A line of the map's put back as the map has it: its contract's work and payment follow it back
/// without a fee (pro rata), any change agreed before dropped.
pub fn restore(c: &mut Company, name: &str, old: &LineDesign, new: &LineDesign) {
    let Some(k) = changeable(c, name) else { return };
    let id = k.id;
    let mut q = match edit_ask(old, new) {
        Some(ask) => price(c, k, &ask),
        None => {
            if let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) {
                x.agreed = None;
            }
            return;
        }
    };
    let sizes = q.sizes;
    q.year_after = adjusted(c, k, &q.before, &q.after, sizes).round() as Cents;
    (q.change, q.class, q.fee, q.penalty, q.points, q.reputation, q.waits) = (Change::Restore, Class::Minor, 0, 0, 0.0, 0.0, false);
    // (a minor change's share is not counted against the next ones: the line is as tendered)
    q.share = 0.0;
    commit(c, id, &q, 0, 0, false, None, None);
}

#[cfg(test)]
#[path = "variations_tests.rs"]
mod tests;

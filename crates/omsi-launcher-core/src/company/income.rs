//! The lines' income and passengers - the UK model's income side (`econ_uk`, wave 2b).
//!
//! # Passengers
//!
//! A trip of a line without its own forecast (a map's line; an own line keeps the stop and
//! headway model of `ownline`, which lands an urban line on the data's numbers) carries the
//! data's passengers per bus-hour of its hour (`econ_uk::WEEKDAY_HOURS`, `SATURDAY_HOURS`; a
//! Sunday or public holiday a Saturday's at its share) for the bus-hours it takes - its running
//! time and the stand that goes with it (`BUS_HOUR_PER_RUNNING_HOUR`) - scaled by the kind of
//! line it is (`LineType`): a park & ride carries few, an urban all-day line a lot, a works
//! shuttle and a school run a load a trip at their times. On a weekday without school the
//! pupils stay at home (`school_day`). The day close then draws its noise and applies what the
//! company is (reputation, the driver, the bus's cleanliness, events, research, the fare's answer
//! `fares::Demand`) - all of them 1 for an ordinary company - and the line's lateness
//! (`LineStats::late`: a line often late loses riders slowly, and wins them back as slowly); a
//! full bus leaves the rest at the stop (`ownline::carried`).
//!
//! # Income
//!
//! Who boards (`Mix`, by the kind of line: a quiet line has more concession passes) and what
//! each of them brings (`Rates`):
//!
//! * the adults' tickets: the line's single (`fares`; it rises with its trips' length,
//!   `econ_uk::SINGLE_PER_KM`) at the data's yield per paying boarding - returns, day and period
//!   tickets averaged in (`YIELD_PER_PAYING / MEAN_SINGLE` of the single);
//! * the children's: `CHILD_FACTOR` of it;
//! * a multi-operator pass's ride: the operator's share of the pool (`NETWORK_PER_BOARDING`);
//! * a pupil on a school pass: the term pass spread over the term's rides
//!   (`econ_uk::school_pass_per_ride`);
//! * a concession pass's ride: the authority's reimbursement, `CONCESSION_SHARE` of the adult
//!   single (`Reimbursement`, a line of its own);
//! * the staff of a shuttle's client, and the rest: nothing at the farebox (their contract pays).
//!
//! On top, BSOG for every kilometre in service (`BSOG_PER_LIVE_MILE`; none for empty runs). An
//! optional fare cap per line caps the singles (`FARE_CAP`). A line under a gross-cost contract
//! (`CompanyLine::fares_to_authority`) takes its fares for the authority: they are booked in and
//! paid over the same day (`BookingKind::AuthorityFares`), and its BSOG is the authority's.
//!
//! The mode and a sandbox's knob scale what is booked (`econ_uk::mode(..).income`); the country
//! is in the fares' prices (`econ_uk::Country::fares`) and the grant's (`Item::Grants`).

use super::dates;
use super::econ_uk::{self as uk, Item};
use super::model::{BookingKind, Cents, Company, CompanyLine};
use crate::service::ServiceKind;
use serde::{Deserialize, Serialize};

// --- the kinds of line ------------------------------------------------------------------------

/// What kind of line it is, for its passengers: how many ride and who they are.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    /// A typical line of a regional operator: the data's average.
    #[default]
    Typical,
    /// A busy town line all day long (some 28 passengers a bus-hour).
    Urban,
    /// A trunk route between towns (13-16 a trip; many concession passes).
    Trunk,
    /// A rural or quiet line: few riders, most of them on concession passes.
    Rural,
    /// Park & Ride: short trips, few on each, a cheap flat fare.
    ParkRide,
    /// An evening or Sunday service.
    Evening,
    /// A works shuttle: a company's staff to and from their shifts (the client pays).
    Shuttle,
    /// A school run: pupils on passes, on school days.
    School,
}

/// Who boards, as shares of the boardings: concession passes, school passes, multi-operator
/// passes, paying adults, children; the rest (staff and contract riders, free travel, transfers)
/// pays nothing at the farebox.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub concession: f64,
    pub school: f64,
    pub network: f64,
    pub adult: f64,
    pub child: f64,
}

impl Mix {
    pub fn other(&self) -> f64 {
        (1.0 - self.concession - self.school - self.network - self.adult - self.child).max(0.0)
    }

    /// The mix of a day without school: the pupils are not there, everyone else is (their shares
    /// of the fewer who ride).
    pub fn without_school(&self) -> Mix {
        let k = 1.0 / (1.0 - self.school).max(0.05);
        Mix { concession: self.concession * k, school: 0.0, network: self.network * k, adult: self.adult * k, child: self.child * k }
    }
}

/// What a kind of line is: its riders against the data's per bus-hour (or a load a trip), its
/// fare against the distance fare, its mix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kind {
    pub demand: f64,
    pub per_trip: Option<f64>,
    pub fare: f64,
    pub mix: Mix,
}

const fn mix(concession: f64, school: f64, network: f64, adult: f64, child: f64) -> Mix {
    Mix { concession, school, network, adult, child }
}

impl LineType {
    pub const ALL: [LineType; 8] = [LineType::Typical, LineType::Urban, LineType::Trunk, LineType::Rural, LineType::ParkRide, LineType::Evening, LineType::Shuttle, LineType::School];

    pub fn label(self) -> &'static str {
        match self {
            LineType::Typical => "Typical line",
            LineType::Urban => "Urban all-day line",
            LineType::Trunk => "Trunk route",
            LineType::Rural => "Rural or quiet line",
            LineType::ParkRide => "Park & Ride",
            LineType::Evening => "Evening or Sunday service",
            LineType::Shuttle => "Works shuttle",
            LineType::School => "School run",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            LineType::Typical => "About ten passengers a bus-hour, a third of them on concession passes.",
            LineType::Urban => "Busy all day: some 28 passengers a bus-hour, many on network passes.",
            LineType::Trunk => "Between towns: 13 to 16 a trip, many on concession passes.",
            LineType::Rural => "Few riders, most of them on concession passes.",
            LineType::ParkRide => "Short trips with few on each, at a cheap flat fare.",
            LineType::Evening => "Quiet hours: about 7 a trip, most of them paying.",
            LineType::Shuttle => "A company's staff to their shifts: some 15 a trip, paid by its contract.",
            LineType::School => "Pupils on school passes: 20 to 55 a trip, on school days only.",
        }
    }

    /// Its numbers (the data's passengers per trip by service type; the mixes from its routes:
    /// town lines 43-47 % concessions, rural ones 50-60 %, park & ride 61 %, school runs 4 %).
    pub fn kind(self) -> Kind {
        let k = |demand, per_trip, fare, mix| Kind { demand, per_trip, fare, mix };
        match self {
            LineType::Typical => k(1.0, None, 1.0, mix(uk::MIX_CONCESSION, uk::MIX_SCHOOL, uk::MIX_NETWORK, uk::MIX_PAYING, 0.045)),
            LineType::Urban => k(uk::PAX_URBAN / 10.8, None, 1.0, mix(0.30, 0.12, 0.16, 0.22, 0.06)),
            LineType::Trunk => k(1.25, None, 1.0, mix(0.40, 0.12, 0.12, 0.20, 0.05)),
            LineType::Rural => k(0.5, None, 1.0, mix(0.55, 0.12, 0.04, 0.18, 0.04)),
            LineType::ParkRide => k(0.9, None, 0.7, mix(0.60, 0.0, 0.05, 0.30, 0.03)),
            LineType::Evening => k(0.8, None, 1.0, mix(0.25, 0.0, 0.15, 0.40, 0.05)),
            LineType::Shuttle => k(1.0, Some(uk::PER_TRIP_SHUTTLE), 1.0, mix(0.0, 0.0, 0.0, 0.0, 0.0)),
            LineType::School => k(1.0, Some(uk::PER_TRIP_SCHOOL), 1.0, mix(0.03, 0.85, 0.0, 0.04, 0.06)),
        }
    }
}

// --- what a line's days showed ------------------------------------------------------------------

/// What a line's days showed (kept with it, `CompanyLine::stats`): the length (km) and stops of
/// its trips, how many it runs a day and over how many hours, the hour of its first, the share of
/// them in the school's hours - all as its recent days had them - and the share of its trips late
/// as its passengers remember it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct LineStats {
    pub trip_km: f64,
    pub stops: f64,
    pub trips: f64,
    pub span: f64,
    pub first: f64,
    pub school_hours: f64,
    pub late: f64,
}

/// A line's day as `observe` takes it: its trips with passengers (length, stops, departure,
/// arrival), and of those run how many, how many late by the UK standard (more than
/// `econ_uk::ON_TIME_LATE` minutes).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub trips: Vec<(f64, u32, i32, i32)>,
    pub run: u32,
    pub late: u32,
}

/// The school's hours: to school and home again.
fn school_hours(minute: i32) -> bool {
    let h = minute.rem_euclid(1440) / 60;
    (7..=8).contains(&h) || (14..=16).contains(&h)
}

/// A line's stats with another day: its trips' figures towards the day's (a weekend's fewer
/// trips do not make it another kind of line - the busiest days count), its lateness remembered.
pub fn observe(s: &mut LineStats, day: &Seen) {
    if !day.trips.is_empty() {
        let n = day.trips.len() as f64;
        let km = day.trips.iter().map(|t| t.0).sum::<f64>() / n;
        let stops = day.trips.iter().map(|t| t.1 as f64).sum::<f64>() / n;
        let school = day.trips.iter().filter(|t| school_hours(t.2)).count() as f64 / n;
        let first = day.trips.iter().map(|t| t.2).min().unwrap_or(0);
        let last = day.trips.iter().map(|t| t.3).max().unwrap_or(0);
        let span = (last - first).max(0) as f64 / 60.0;
        let fresh = s.trips <= 0.0;
        let towards = |old: f64, new: f64| if fresh { new } else { old + 0.2 * (new - old) };
        s.trip_km = towards(s.trip_km, km);
        s.stops = towards(s.stops, stops);
        s.school_hours = towards(s.school_hours, school);
        s.first = towards(s.first, first as f64 / 60.0);
        s.trips = (s.trips * 0.97).max(n);
        s.span = (s.span * 0.97).max(span);
    }
    if day.run > 0 {
        let k = 1.0 - (-1.0 / uk::LATE_MEMORY_DAYS).exp();
        s.late += k * (day.late as f64 / day.run as f64 - s.late);
    }
}

/// What the passengers make of a line's lateness: all of them while it is late no more than
/// `LATE_FORGIVEN` of its trips, then fewer, at most `LATE_MOST` fewer.
pub fn punctual_factor(s: &LineStats) -> f64 {
    1.0 - (uk::LATE_LOSS * (s.late - uk::LATE_FORGIVEN).max(0.0)).min(uk::LATE_MOST)
}

/// The kind of line it is: the player's word, else as its timetable shows it - an own line's
/// kind of service, a line in the school's hours only a school run, one from the evening on an
/// evening service, a short line of close stops all day long an urban one, one of far-apart
/// stops, long trips or a handful a day a rural one; the others typical.
pub fn type_of(l: &CompanyLine) -> LineType {
    l.line_type.unwrap_or_else(|| derived(l))
}

/// The kind of line its timetable shows (`type_of` without the player's word).
pub fn derived(l: &CompanyLine) -> LineType {
    match super::ownline::kind_of(l) {
        ServiceKind::School => return LineType::School,
        ServiceKind::OnDemand => return LineType::Rural,
        ServiceKind::Leisure | ServiceKind::Regular => {}
    }
    let s = &l.stats;
    if s.trips <= 0.0 {
        return LineType::Typical;
    }
    let spacing = if s.stops > 1.0 { s.trip_km / (s.stops - 1.0) } else { 1.0 };
    if s.school_hours >= 0.9 && s.trips <= 16.0 {
        LineType::School
    } else if s.first >= 18.0 {
        LineType::Evening
    } else if spacing <= 0.6 && s.trip_km <= 12.0 && s.span >= 14.0 && s.trips >= 40.0 {
        LineType::Urban
    } else if spacing >= 2.5 || s.trip_km >= 35.0 || s.trips <= 8.0 {
        LineType::Rural
    } else {
        LineType::Typical
    }
}

// --- the days ---------------------------------------------------------------------------------

/// A day as the passengers have it: a weekday (0), a Saturday (1), a Sunday or public holiday
/// (2); and whether the schools are in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayOf {
    pub kind: usize,
    pub school: bool,
}

/// The usual school holidays, as days of the year round a date: Christmas, the Easter fortnight,
/// the half-terms, the summer (the map's own calendar is the timetable's: the day close knows
/// the date only).
fn school_holiday(day: i64) -> bool {
    let (y, m, d) = dates::civil_from_days(day);
    let (em, ed) = crate::service::easter(y);
    let easter = dates::days_from_civil(y, em, ed);
    if (easter - 9..=easter + 8).contains(&day) {
        return true;
    }
    matches!((m, d), (12, 20..) | (1, ..=3) | (2, 12..=16) | (5, 26..) | (7, 22..) | (8, _) | (9, ..=2) | (10, 24..))
}

/// What a date is to the passengers.
pub fn day_of(date: &str) -> DayOf {
    let Some(day) = dates::parse(date) else { return DayOf { kind: 0, school: true } };
    let code = crate::service::code_of_day(day);
    let wd = dates::weekday(day);
    let kind = if crate::service::common_public_holiday(code) || wd == 6 {
        2
    } else if wd == 5 {
        1
    } else {
        0
    };
    DayOf { kind, school: kind == 0 && !school_holiday(day) }
}

/// The schools are in on `date` (a weekday out of the usual holidays).
pub fn school_day(date: &str) -> bool {
    day_of(date).school
}

/// Passengers per bus-hour at a minute of a day (`DayOf::kind`).
pub fn per_bus_hour(kind: usize, minute: i32) -> f64 {
    let h = (minute.rem_euclid(1440) / 60) as usize;
    match kind {
        0 => uk::WEEKDAY_HOURS[h],
        1 => uk::SATURDAY_HOURS[h],
        _ => uk::SATURDAY_HOURS[h] * uk::PAX_SUNDAY / uk::PAX_SATURDAY,
    }
}

/// The parts of the day the pages show a line's passengers by.
pub const BANDS: [&str; 5] = ["Early (before 7)", "Morning peak (7-9)", "Daytime (9-15)", "Afternoon peak (15-18)", "Evening (from 18)"];

/// The part of the day (`BANDS`) of a minute.
pub fn band_of(minute: i32) -> usize {
    match minute.rem_euclid(1440) / 60 {
        7..=8 => 1,
        9..=14 => 2,
        15..=17 => 3,
        18.. => 4,
        _ => 0,
    }
}

/// The bus-hours of a trip of `minutes`' running (its stand with it).
pub fn bus_hours(minutes: f64) -> f64 {
    minutes.max(0.0) / 60.0 * uk::BUS_HOUR_PER_RUNNING_HOUR
}

/// A trip's passengers before the day's draws and the company's standing: by its line's kind,
/// the day, the hour it leaves and how long it runs.
pub fn trip_demand(t: LineType, d: DayOf, dep: i32, minutes: f64) -> f64 {
    let k = t.kind();
    let peak = |a: bool| if a { 1.0 } else { 0.0 };
    match (t, k.per_trip) {
        (LineType::School, Some(n)) => {
            if d.school {
                n * if school_hours(dep) { 1.0 } else { 0.15 }
            } else {
                n * 0.08
            }
        }
        (_, Some(n)) => {
            let h = dep.rem_euclid(1440) / 60;
            let shift = peak((6..=9).contains(&h) || (14..=18).contains(&h));
            let at = shift + (1.0 - shift) * uk::OFF_PEAK_SHUTTLE;
            n * at * if d.kind == 0 { 1.0 } else { 0.2 }
        }
        _ => {
            let pupils = if d.kind == 0 && !d.school { 1.0 - k.mix.school } else { 1.0 };
            per_bus_hour(d.kind, dep) * bus_hours(minutes) * k.demand * pupils
        }
    }
}

/// An average speed in service (km/h): a trip's running time from its kilometres where the
/// timetable's is not known.
pub const RUNNING_KMH: f64 = 25.0;

/// A trip's passengers by its kilometres (the live game's report of a trip: `trip_demand` at
/// `RUNNING_KMH`).
pub fn by_km(t: LineType, date: &str, dep: i32, km: f64) -> f64 {
    trip_demand(t, day_of(date), dep, km.max(0.0) / RUNNING_KMH * 60.0)
}

/// A trip's passengers as the company expects them (its line's kind, the date; the company's
/// standing and the day's draws aside) - for the pages and the live game.
pub fn expected(l: Option<&CompanyLine>, date: &str, dep: i32, arr: i32) -> f64 {
    let t = l.map(type_of).unwrap_or_default();
    trip_demand(t, day_of(date), dep, (arr - dep).max(0) as f64)
}

// --- the fares --------------------------------------------------------------------------------

/// The fare table of a line (euro cents, each at its currency's step): the adult single, the
/// return, the child's single, a day and a week ticket; and the cap, when the line has one.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FareTable {
    pub single: f64,
    pub ret: f64,
    pub child: f64,
    pub day: f64,
    pub week: f64,
    pub cap: Option<(f64, f64)>,
}

impl FareTable {
    /// The adult single and the child's a passenger pays (under the cap).
    pub fn paid(&self) -> (f64, f64) {
        match self.cap {
            Some((a, k)) => (self.single.min(a), self.child.min(k)),
            None => (self.single, self.child),
        }
    }
}

/// A UK price (pence, 2022) of a fare as the company's passengers pay it: in its country and at
/// its prices (euro cents, not rounded).
fn price(c: &Company, pence: f64) -> f64 {
    uk::gbp(pence) * uk::country_of(c).fares * c.price_index
}

/// A fare made the round price of the company's currency (euro cents).
pub fn round(c: &Company, cents: f64) -> f64 {
    c.currency.round_fare(cents)
}

/// The adult single of a trip of `km` on a line of kind `t` (euro cents, round).
pub fn distance_single(c: &Company, t: LineType, km: f64) -> f64 {
    let km = if km > 0.0 { km } else { uk::TYPICAL_TRIP_KM };
    let (lo, hi) = uk::ADULT_SINGLE_RANGE;
    round(c, price(c, (uk::SINGLE_BASE + uk::SINGLE_PER_KM * km).clamp(lo, hi) * t.kind().fare))
}

/// The table of a line whose adult single is `single` (euro cents).
pub fn table(c: &Company, l: &CompanyLine, single: f64) -> FareTable {
    let r = |pence_of_single: f64| round(c, single * pence_of_single);
    let cap = l.fare_cap.then(|| (round(c, price(c, uk::FARE_CAP.0)), round(c, price(c, uk::FARE_CAP.1))));
    FareTable { single: round(c, single), ret: r(uk::RETURN_FACTOR), child: r(uk::CHILD_FACTOR), day: r(uk::DAY_TICKET / uk::ADULT_SINGLE), week: r(uk::WEEK_TICKET / uk::ADULT_SINGLE), cap }
}

// --- what a boarding brings ---------------------------------------------------------------------

/// What the mode and a sandbox's knob make of the income (the country is in the prices).
fn bonus(c: &Company) -> f64 {
    uk::mode(c.game_mode).income * super::sandbox::of(c).income
}

/// What each passenger of a line brings on a day (euro cents, as booked), and who they are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub line_type: LineType,
    pub day: DayOf,
    pub mix: Mix,
    /// The adult single paid (the cap applied), and per boarding of each group.
    pub single: f64,
    pub adult: f64,
    pub child: f64,
    pub network: f64,
    pub school: f64,
    pub reimbursement: f64,
    /// BSOG per kilometre in service.
    pub bsog_km: f64,
    pub to_authority: bool,
    /// The passengers' answer to the line's lateness (`punctual_factor`).
    pub punctual: f64,
}

impl Rates {
    /// A line's on `date`.
    pub fn of(c: &Company, l: &CompanyLine, date: &str) -> Rates {
        Rates::at(c, l, day_of(date))
    }

    /// A line's on a day of `day`'s kind.
    pub fn at(c: &Company, l: &CompanyLine, day: DayOf) -> Rates {
        let t = type_of(l);
        let k = t.kind();
        let mix = if day.school { k.mix } else { k.mix.without_school() };
        let (single, child) = table(c, l, super::fares::fare_of(c, l) as f64).paid();
        let ratio = uk::YIELD_PER_PAYING / uk::MEAN_SINGLE;
        let b = bonus(c);
        Rates {
            line_type: t,
            day,
            mix,
            single,
            adult: single * ratio * b,
            child: child * ratio * b,
            network: uk::value(c, Item::Fares, uk::NETWORK_PER_BOARDING),
            school: uk::value(c, Item::Fares, uk::school_pass_per_ride()),
            reimbursement: uk::CONCESSION_SHARE * single * b,
            bsog_km: uk::value(c, Item::Grants, uk::BSOG_PER_LIVE_MILE) / uk::KM_PER_MILE,
            to_authority: l.fares_to_authority,
            punctual: punctual_factor(&l.stats),
        }
    }

    /// What `pax` boardings and `live_km` in service bring.
    pub fn takings(&self, pax: f64, live_km: f64) -> Takings {
        let (p, m) = (pax.max(0.0), &self.mix);
        Takings {
            adult: p * m.adult * self.adult,
            child: p * m.child * self.child,
            network: p * m.network * self.network,
            school: p * m.school * self.school,
            reimbursement: p * m.concession * self.reimbursement,
            bsog: live_km.max(0.0) * self.bsog_km,
        }
    }

    /// What a boarding brings on average (the fares, the passes, the reimbursement; no BSOG).
    pub fn per_boarding(&self) -> f64 {
        self.takings(1.0, 0.0).total()
    }
}

/// What trips took (euro cents, fractional until booked).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Takings {
    pub adult: f64,
    pub child: f64,
    pub network: f64,
    pub school: f64,
    pub reimbursement: f64,
    pub bsog: f64,
}

impl Takings {
    pub fn add(&mut self, o: &Takings) {
        self.adult += o.adult;
        self.child += o.child;
        self.network += o.network;
        self.school += o.school;
        self.reimbursement += o.reimbursement;
        self.bsog += o.bsog;
    }

    /// The tickets sold on board (adults' and children's) by `f`: drivers trained in selling
    /// them, inspectors on the line.
    pub fn tickets_by(&mut self, f: f64) {
        self.adult *= f;
        self.child *= f;
    }

    /// The farebox: the tickets and the network passes.
    pub fn fares(&self) -> f64 {
        self.adult + self.child + self.network
    }

    pub fn total(&self) -> f64 {
        self.fares() + self.school + self.reimbursement + self.bsog
    }
}

/// What was booked of takings (whole cents): each line of income, and the fares collected for the
/// authority (a gross-cost line's: none of the others then).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Tally {
    pub adult: Cents,
    pub child: Cents,
    pub network: Cents,
    pub school: Cents,
    pub reimbursement: Cents,
    pub bsog: Cents,
    pub for_authority: Cents,
}

impl Tally {
    pub fn add(&mut self, o: &Tally) {
        self.adult += o.adult;
        self.child += o.child;
        self.network += o.network;
        self.school += o.school;
        self.reimbursement += o.reimbursement;
        self.bsog += o.bsog;
        self.for_authority += o.for_authority;
    }

    /// The farebox: adults, children, network passes.
    pub fn fares(&self) -> Cents {
        self.adult + self.child + self.network
    }

    /// The company's income of it (the authority's fares not).
    pub fn income(&self) -> Cents {
        self.fares() + self.school + self.reimbursement + self.bsog
    }
}

/// Book a line's takings (`number` its number, `how` after it: "", " (own trip)"): each line of
/// income of its own, or - a gross-cost line - the fares collected for the authority and paid
/// over to it. Returns what was booked.
pub fn book(c: &mut Company, number: &str, t: &Takings, to_authority: bool, measured: bool, how: &str) -> Tally {
    let r = |x: f64| x.round() as Cents;
    let text = format!("Line {number}{how}");
    let mut out = Tally::default();
    if to_authority {
        let all = r(t.fares() + t.school + t.reimbursement);
        if all != 0 {
            c.book(BookingKind::AuthorityFares, all, format!("{text}: fares collected for the authority"), measured);
            c.book(BookingKind::AuthorityFares, -all, format!("{text}: fares paid over to the authority"), measured);
        }
        out.for_authority = all;
        return out;
    }
    out = Tally { adult: r(t.adult), child: r(t.child), network: r(t.network), school: r(t.school), reimbursement: r(t.reimbursement), bsog: r(t.bsog), for_authority: 0 };
    for (kind, a) in [(BookingKind::Fares, out.adult), (BookingKind::ChildFares, out.child), (BookingKind::NetworkPasses, out.network), (BookingKind::SchoolPasses, out.school), (BookingKind::Reimbursement, out.reimbursement), (BookingKind::Bsog, out.bsog)] {
        if a != 0 {
            c.book(kind, a, text.clone(), measured);
        }
    }
    out
}

/// What a line takes a kilometre in service, on average (the data's boardings a kilometre at what
/// a boarding brings on a typical line, and BSOG) - for an estimate without its timetable.
pub fn per_km(c: &Company, l: &CompanyLine) -> f64 {
    let r = Rates::of(c, l, &c.date);
    uk::PAX_PER_KM * r.per_boarding() + r.bsog_km
}

#[cfg(test)]
#[path = "income_tests.rs"]
mod tests;

// --- older files --------------------------------------------------------------------------------

/// The single ticket an older file's fare was set against: the old reference's association fare
/// by the game mode (cents; version 2 and before).
fn old_association(m: super::model::GameMode) -> f64 {
    match m {
        super::model::GameMode::Casual => 240.0,
        super::model::GameMode::Normal => 200.0,
        super::model::GameMode::Realistic => 190.0,
    }
}

/// A file of version 2 or before brought to the UK income model: a line's fare was a single
/// ticket against the old association's fare (German levels, in euros) - it keeps its distance
/// from that, now from the line's standard UK fare (`fares::standard_fare`).
pub fn migrate(c: &mut Company) {
    let old = old_association(c.game_mode);
    for k in 0..c.lines.len() {
        let Some(f) = c.lines[k].fare else { continue };
        let x = f as f64 / old - 1.0;
        let p0 = super::fares::standard_fare(c, &c.lines[k]) as f64;
        c.lines[k].fare = (x.abs() >= 0.005).then(|| round(c, p0 * (1.0 + x)).round() as Cents);
    }
}

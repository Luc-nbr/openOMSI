//! Bus contracts: what an authority, a school or a firm pays the company to run a line, and how
//! it judges the running (the UK economy's contracts and tenders, its performance regime and
//! deductions - wave 2a; the numbers are `econ_uk`'s).
//!
//! # The three kinds
//!
//! * **Gross cost** (`Kind::Gross`): the authority pays a fixed price per operating day or per
//!   week and keeps the fares - the company carries only the cost risk. The fares taken on the
//!   bus are handed over (`fares_to_authority`: the flag the income side asks).
//! * **Net subsidy** (`Kind::Net`): a fixed sum a month (eleven months for a school run, no
//!   August); the company keeps the fares, and may raise them on the line by at most 15 % a year
//!   (`fare_cap`).
//! * **Private hire** (`Kind::Hire`): a school, a college or a firm pays a fixed price for each of
//!   its set days (a school year of some 190 days, an office year of 252). The company's own
//!   school lines run as one (`ensure_hires`).
//!
//! A contract is agreed at a price a year (`Contract::year_price`, euro cents at the contract
//! index of its day); it is paid by its period (`Period`) and rises with the contract index.
//!
//! # How a contract comes about
//!
//! A tender of the map's lines won (`concessions::take`, priced on the cost formula of
//! `price_of`) brings a gross-cost or a net-subsidy contract for the concession's term; an own
//! school line a private hire. A concession of an older file won in a tender became gross cost
//! at its winning price (`store::migrate`, version 3); one taken on directly (an easy economy, a
//! line run before there were concessions) is commercial.
//!
//! # Performance
//!
//! The day's close hands every journey of a contract line to `Journeys`, and `close_day` judges
//! them:
//!
//! * **Unpaid:** a journey not run (or run only in part), 5 minutes or more early, or the mode's
//!   `unpaid_late_from` minutes or more late (Realistic 10, Normal 15, Casual never) loses its
//!   share of the day's payment (`journey_value`). On a day of storm or snow the company did its
//!   best: late journeys are paid, journeys not run half (`econ_uk::WEATHER_PAID`).
//! * **Default points** (`econ_uk::POINTS_*`, by the mode's `default_points`): 3 for 2-4 minutes
//!   early or 5-10 late, 5 beyond that, 10 for a missed journey or a smaller bus than the tour
//!   asks for, 20 for a bus run unroadworthy or a driver without the licence for it.
//! * **Quality points** at the month's end: a clean roadside inspection, a month without a
//!   complaint about the line, 98 % of its journeys on time. They offset the default points.
//! * **RAG:** the net points of the last twelve months make a contract green, orange (16) or red
//!   (26). A red contract may be ended by the authority at the month's end (where the mode's
//!   `may_terminate`), the company paying for the replacement; only a green one earns its next
//!   extension year (`extend_due`). The average net points over the company's contracts are its
//!   track record: its quality score in the next tenders (`quality`).
//!
//! Handing a contract back early costs a month's average payment (`hand_back`).
//!
//! **Commercial lines** - the company's own lines and the map lines taken on without a tender -
//! have no contract: no deductions, no points; late running costs them passengers and reputation
//! (`lateness`, `reputation`).

use super::dates;
use super::day::{DayReport, LineDay, Note};
use super::econ_uk::{self, Item, Level};
use super::model::{BookingKind, BusKind, BusSize, Cents, Company, Drive};
use super::rng::Rng;
use crate::service::ServiceKind;
use serde::{Deserialize, Serialize};

// --- the contract ------------------------------------------------------------------------------

/// What kind of contract (see the module's text).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Gross,
    Net,
    Hire,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Gross, Kind::Net, Kind::Hire];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Gross => "Gross cost",
            Kind::Net => "Net subsidy",
            Kind::Hire => "Private hire",
        }
    }
}

/// When a contract is paid: each operating day, each week (on its Sunday), each month (on its
/// last day).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    #[default]
    Day,
    Week,
    Month,
}

/// A contract's rating: green, orange (a warning), red (it may be ended).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Rag {
    #[default]
    Green,
    Orange,
    Red,
}

impl Rag {
    pub fn of(net_points: f64) -> Rag {
        if net_points >= econ_uk::POINTS_RED {
            Rag::Red
        } else if net_points >= econ_uk::POINTS_ORANGE {
            Rag::Orange
        } else {
            Rag::Green
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rag::Green => "Green",
            Rag::Orange => "Orange",
            Rag::Red => "Red",
        }
    }
}

/// How a contract ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum End {
    /// Its term ran out.
    Expired,
    /// The company handed it back early (and paid for it).
    HandedBack,
    /// The authority ended it for default (red).
    Terminated,
}

/// A day of a contract's record: its defaults by kind, its journeys not paid and what they cost,
/// its default points (by the mode's factor) and its quality points.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PointsDay {
    pub date: String,
    /// Journeys slightly off time (2-4 early, 5-10 late), far off, not run; buses smaller than
    /// the tour asks for; serious defaults (a bus unroadworthy, a driver without the licence).
    pub slight: u32,
    pub far: u32,
    pub missed: u32,
    pub vehicle: u32,
    pub serious: u32,
    pub unpaid: u32,
    pub deducted: Cents,
    pub default: f64,
    pub quality: f64,
    /// What the quality points were for (`QUALITY_REASONS`' keys).
    pub why: Vec<String>,
}

/// The quality points' reasons (key, words).
pub const QUALITY_REASONS: [(&str, &str); 3] = [("inspection", "a clean inspection"), ("complaints", "a month without complaints"), ("on_time", "98 % on time")];

/// A contract's month so far.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Tally {
    pub month: String,
    pub journeys: u32,
    pub on_time: u32,
    pub unpaid: u32,
    /// What it was paid, the deductions off.
    pub paid: Cents,
    pub serious: u32,
}

fn one() -> f64 {
    1.0
}

/// A contract of the company's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Contract {
    pub id: u32,
    /// The line (its timetable name, an own line's `oo_` name) and the number shown.
    pub line: String,
    pub number: String,
    pub kind: Kind,
    #[serde(default)]
    pub period: Period,
    /// The price a year agreed, at the contract index `index` of its day (euro cents).
    pub year_price: Cents,
    #[serde(default = "one")]
    pub index: f64,
    /// Operating days a year the price was made for (a day's payment is the year's share of
    /// one), the months a year it is paid (net subsidy: 12, a school run 11) and the journeys of
    /// an operating day.
    pub days_a_year: u32,
    #[serde(default)]
    pub paid_months: u32,
    #[serde(default)]
    pub journeys: u32,
    pub from: String,
    pub until: String,
    /// Extension years still to be earned, those earned, and the end they were decided for.
    #[serde(default)]
    pub extensions_left: u32,
    #[serde(default)]
    pub extended: u32,
    #[serde(default)]
    pub decided: String,
    #[serde(default)]
    pub ended: Option<String>,
    #[serde(default)]
    pub end: Option<End>,
    /// A net subsidy: the line's fare when it began (the 15 % a year are counted from it).
    #[serde(default)]
    pub fare_base: Option<Cents>,
    /// Its record: the days with defaults or quality points of the last twelve months.
    #[serde(default)]
    pub record: Vec<PointsDay>,
    #[serde(default)]
    pub month: Tally,
    /// What the last months paid (month, net), the latest last: a month's average for leaving.
    #[serde(default)]
    pub paid: Vec<(String, Cents)>,
}

impl Contract {
    /// It runs on `date` (begun, not ended, within its term).
    pub fn on(&self, date: &str) -> bool {
        self.ended.is_none() && dates::between(&self.from, date) >= 0 && dates::between(date, &self.until) >= 0
    }

    /// Not ended (it may begin later).
    pub fn live(&self) -> bool {
        self.ended.is_none()
    }

    /// The months a year it is paid (a net subsidy's; 12 for the others).
    pub fn months(&self) -> u32 {
        if self.paid_months == 0 {
            12
        } else {
            self.paid_months
        }
    }

    /// Its price a year now (cents): the agreed price risen with the contract index.
    pub fn year_now(&self, contract_index: f64) -> f64 {
        self.year_price as f64 * contract_index / self.index.max(1e-9)
    }

    /// Its default and quality points of the twelve months to `date`, and whether a serious
    /// default among them wiped the quality points out.
    pub fn points(&self, date: &str) -> (f64, f64, bool) {
        let mut def = 0.0;
        let mut quality = 0.0;
        let mut serious = false;
        for d in self.record.iter().filter(|d| within(&d.date, date)) {
            def += d.default;
            quality += d.quality;
            serious |= d.serious > 0;
        }
        (def, if serious { 0.0 } else { quality }, serious)
    }

    /// Its net default points of the twelve months to `date` (at least none).
    pub fn net_points(&self, date: &str) -> f64 {
        let (d, q, _) = self.points(date);
        (d - q).max(0.0)
    }

    pub fn rag(&self, date: &str) -> Rag {
        Rag::of(self.net_points(date))
    }

    /// Journeys not paid in the twelve months to `date`, and what they cost.
    pub fn unpaid(&self, date: &str) -> (u32, Cents) {
        self.record.iter().filter(|d| within(&d.date, date)).fold((0, 0), |a, d| (a.0 + d.unpaid, a.1 + d.deducted))
    }

    /// Its whole term in years (core and extensions to come or taken).
    pub fn years(&self) -> f64 {
        (dates::between(&self.from, &self.until) + 1) as f64 / 365.0
    }
}

/// `day` is within the twelve months to `date`.
fn within(day: &str, date: &str) -> bool {
    let d = dates::between(day, date);
    (0..econ_uk::POINTS_DAYS).contains(&d)
}

/// The company's bus contracts (`Concessions::contracts`), and the contracts ended in the last
/// twelve months (their record still counts).
pub fn all(c: &Company) -> &[Contract] {
    &c.concessions.contracts
}

/// The contract of a line running on `date`.
pub fn on_line<'a>(c: &'a Company, line: &str, date: &str) -> Option<&'a Contract> {
    c.concessions.contracts.iter().find(|k| k.on(date) && k.line.eq_ignore_ascii_case(line))
}

/// The contract of a line today, or the next one to begin.
pub fn of_line<'a>(c: &'a Company, line: &str) -> Option<&'a Contract> {
    on_line(c, line, &c.date).or_else(|| c.concessions.contracts.iter().filter(|k| k.live() && k.line.eq_ignore_ascii_case(line)).min_by(|a, b| a.from.cmp(&b.from)))
}

/// The fares of the line belong to the authority (a gross-cost contract today): the income side
/// books them, and the day's close hands them over (`close_day`).
pub fn fares_to_authority(c: &Company, line: &str) -> bool {
    on_line(c, line, &c.date).is_some_and(|k| k.kind == Kind::Gross)
}

/// The line is paid by a contract today (no payment per kilometre besides).
pub fn under_contract(c: &Company, line: &str) -> bool {
    on_line(c, line, &c.date).is_some()
}

/// The highest fare a net-subsidy contract allows on its line now (None: no such contract, or
/// its first fare not known yet): its fare at the start, 15 % more for each year begun.
pub fn fare_cap(c: &Company, line: &str) -> Option<Cents> {
    let k = on_line(c, line, &c.date).filter(|k| k.kind == Kind::Net)?;
    let base = k.fare_base?;
    let years = (dates::between(&k.from, &c.date).max(0) / 365) as i32 + 1;
    Some((base as f64 * (1.0 + econ_uk::NET_FARE_RISE).powi(years)).floor() as Cents)
}

// --- what a contract pays ----------------------------------------------------------------------

/// What a contract is worth an operating day now (cents, fractional): its year's price over the
/// days it was made for.
pub fn day_value(c: &Company, k: &Contract) -> f64 {
    k.year_now(c.contract_index) / k.days_a_year.max(1) as f64
}

/// What it pays a month on average now.
pub fn month_value(c: &Company, k: &Contract) -> f64 {
    k.year_now(c.contract_index) / 12.0
}

/// What it pays by its period now: a day's, a week's, or a paid month's.
pub fn payment(c: &Company, k: &Contract) -> Cents {
    let year = k.year_now(c.contract_index);
    (match k.period {
        Period::Day => year / k.days_a_year.max(1) as f64,
        Period::Week => year / 52.0,
        Period::Month => year / k.months() as f64,
    })
    .round() as Cents
}

/// Deductions and default points apply in the company's game (not Casual's, nor a sandbox
/// without penalties).
fn strictness(c: &Company) -> f64 {
    econ_uk::mode(c.game_mode).default_points * super::sandbox::of(c).penalties
}

/// What a journey of `line` not paid costs the company now: its share of the contract's
/// operating day (`journeys`: the day's timetabled journeys, else the contract's usual). None
/// on a commercial line, and in a game without deductions.
pub fn journey_value(c: &Company, line: &str, journeys: Option<u32>) -> Cents {
    let Some(k) = on_line(c, line, &c.date) else { return 0 };
    if strictness(c) <= 0.0 {
        return 0;
    }
    let n = journeys.unwrap_or(k.journeys).max(1) as f64;
    (day_value(c, k) / n).round() as Cents
}

/// A journey `minutes` off the timetable (negative early) is not paid under the company's game
/// mode (a journey not run is not paid either: `close_day`).
pub fn unpaid_at(c: &Company, minutes: i32) -> bool {
    let m = econ_uk::mode(c.game_mode);
    (m.unpaid_early && minutes <= -econ_uk::UNPAID_EARLY) || m.unpaid_late_from.is_some_and(|l| minutes >= l)
}

/// The share of a late journey's value deducted (1: not paid, 0: paid) - the late running's
/// pages show it with each late trip (`lateness::late`).
pub fn unpaid_share(c: &Company, minutes: i32) -> f64 {
    if unpaid_at(c, minutes) {
        1.0
    } else {
        0.0
    }
}

/// The default points of a journey run `minutes` off the timetable (negative early), before the
/// mode's factor: none on time, `POINTS_SLIGHT` for 2-4 early or 5-10 late, `POINTS_FAR` beyond.
pub fn time_points(minutes: i32) -> f64 {
    match minutes {
        m if m <= -econ_uk::UNPAID_EARLY => econ_uk::POINTS_FAR,
        m if m <= -2 => econ_uk::POINTS_SLIGHT,
        m if m > econ_uk::UNPAID_LATE => econ_uk::POINTS_FAR,
        m if m >= econ_uk::ON_TIME_LATE => econ_uk::POINTS_SLIGHT,
        _ => 0.0,
    }
}

/// On time as the standard has it: from a minute early to 5:59 late.
pub fn on_time(minutes: i32) -> bool {
    (-econ_uk::ON_TIME_EARLY..=econ_uk::ON_TIME_LATE).contains(&minutes)
}

// --- the track record and the quality score ------------------------------------------------------

/// The company's track record: the average net default points per contract over the last twelve
/// months (each contract that ran in them counts once, the ended ones too). None: no contract in
/// them - a newcomer.
pub fn track_record(c: &Company) -> Option<f64> {
    let year_ago = dates::add(&c.date, -econ_uk::POINTS_DAYS);
    let counted: Vec<f64> = c
        .concessions
        .contracts
        .iter()
        .filter(|k| dates::between(&k.from, &c.date) >= 0)
        .filter(|k| dates::between(&year_ago, k.ended.as_deref().unwrap_or(&k.until)) >= 0)
        .map(|k| k.net_points(&c.date))
        .collect();
    (!counted.is_empty()).then(|| counted.iter().sum::<f64>() / counted.len() as f64)
}

/// The average net points the company is scored with (a newcomer `NEW_OPERATOR_POINTS`).
pub fn scored_points(c: &Company) -> f64 {
    track_record(c).unwrap_or(econ_uk::NEW_OPERATOR_POINTS)
}

/// The performance share of average net points: 100 % less 2 % a point, at least none.
pub fn performance(points: f64) -> f64 {
    (1.0 - econ_uk::QUALITY_PER_DEFAULT_POINT * points.max(0.0)).clamp(0.0, 1.0)
}

/// The quality points of average net points (of 30).
pub fn quality_points(points: f64) -> f64 {
    econ_uk::QUALITY_POINTS * performance(points)
}

/// The company's quality in a tender, 0-100 (the auction's scale: 30 quality points at 100).
pub fn quality(c: &Company) -> f64 {
    100.0 * performance(scored_points(c))
}

// --- pricing a contract (§4) -------------------------------------------------------------------------

/// The work of a contract: an operating day's timetabled bus-hours and kilometres (dead runs
/// included), its journeys and the buses at its peak, and its operating days a year.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Work {
    pub hours: f64,
    pub km: f64,
    pub journeys: u32,
    pub buses: u32,
    pub days: u32,
}

/// The cost factors a price is made at (an `econ_uk::Level` without its price index), and the
/// factor of the public money (BSOG, a net bid's fares) against the UK's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub level: Level,
    pub income: f64,
    pub inflation: f64,
}

impl Basis {
    /// The UK operator's, as the authority and the rivals reckon (inflation a year `inflation`).
    pub fn uk(inflation: f64) -> Basis {
        Basis { level: Level::uk(1.0), income: 1.0, inflation }
    }

    /// The company's own: its game mode's, its country's and its sandbox's factors.
    pub fn of(c: &Company) -> Basis {
        let level = Level::of(c);
        Basis { level, income: econ_uk::factor(c, Item::Contracts), inflation: super::economy::rules_for(c).inflation }
    }
}

const SOLO: BusKind = BusKind { size: BusSize::Solo, drive: Drive::Diesel };

/// The operator's cost price of the work a year, pence at 2022 prices (§4): the drivers' paid
/// hours (1.5 a timetabled hour) at their all-in rate, fuel and the workshop by the kilometre, a
/// vehicle and the fixed costs (insurance and tax, the ticket machine, the overhead) per bus-year,
/// risen by half the term's inflation and the operator's 5 % margin, less the BSOG on the live
/// miles; a net-subsidy bid also less the fares and concession money it expects.
pub fn cost_pence(w: &Work, kind: Kind, years: f64, b: &Basis) -> f64 {
    let lv = &b.level;
    let days = w.days as f64;
    let (hours, km, buses) = (w.hours * days, w.km * days, w.buses.max(1) as f64);
    let driver = hours * econ_uk::PAID_PER_TIMETABLED_HOUR * econ_uk::DRIVER_BASE_HOUR * econ_uk::all_in() * lv.wages;
    let fuel = km * econ_uk::consumption(SOLO) / 100.0 * econ_uk::DIESEL_PER_LITRE * lv.diesel;
    let shop = km * econ_uk::WORKSHOP_PER_KM * lv.workshop;
    let vehicle = buses * econ_uk::BID_VEHICLE_YEAR * lv.vehicles;
    let fixed = buses * ((econ_uk::INSURANCE_PER_YEAR + econ_uk::TAX_PER_YEAR + 12.0 * econ_uk::TICKET_MACHINE_PER_MONTH) * lv.insurance + econ_uk::OVERHEAD_PER_BUS_YEAR * lv.overheads);
    let subtotal = (driver + fuel + shop + vehicle + fixed) * (1.0 + b.inflation.max(0.0)).powf(years.max(0.0) / 2.0) * (1.0 + econ_uk::BID_MARGIN);
    let bsog = km * econ_uk::LIVE_SHARE / econ_uk::KM_PER_MILE * econ_uk::BSOG_PER_LIVE_MILE * b.income;
    let revenue = if kind == Kind::Net { expected_revenue(w) * b.income } else { 0.0 };
    (subtotal - bsog - revenue).max(0.0)
}

/// The fares and concession money a year a net-subsidy bidder expects on the work (pence): a
/// tendered route's passengers per bus-hour (`TENDERED_DEMAND` of a day's average), at the
/// yield of a boarding and the concession money of the passes.
pub fn expected_revenue(w: &Work) -> f64 {
    let per_hour = (econ_uk::PAX_DAY.0 + econ_uk::PAX_DAY.1) / 2.0 * econ_uk::TENDERED_DEMAND;
    w.hours * w.days as f64 * per_hour * (econ_uk::YIELD_PER_BOARDING + econ_uk::MIX_CONCESSION * econ_uk::CONCESSION_PER_BOARDING)
}

/// The authority's reckoning of the work's price a year, as a typical UK operator would bid it,
/// in the company's world (cents): the cost price at the UK's level, by the contracts' factor of
/// the company's game (its mode, country and sandbox) and its prices since its founding. The
/// rivals bid around it, the cap is `TENDER_CAP` over it.
pub fn price_of(c: &Company, w: &Work, kind: Kind, years: f64) -> f64 {
    let b = Basis::uk(super::economy::rules_for(c).inflation);
    econ_uk::gbp(cost_pence(w, kind, years, &b)) * econ_uk::factor(c, Item::Contracts) * c.price_index
}

/// The advice for the company's own bid (cents a year): the same formula at its own costs - its
/// game mode's, its country's and its sandbox's - and its prices since its founding.
pub fn advice(c: &Company, w: &Work, kind: Kind, years: f64) -> f64 {
    econ_uk::gbp(cost_pence(w, kind, years, &Basis::of(c))) * c.price_index
}

/// The days of a week a line's tours run (`crate::TourInfo::days`' words: "daily", "Mon-Fri",
/// "Mon-Sat", "Sat-Sun", or the days named), as a mask (bit 0 Monday); 0: not said.
pub fn weekdays(days: &str) -> u8 {
    let head = days.split(" & ").next().unwrap_or("").split(", school").next().unwrap_or("").trim();
    match head {
        "daily" => 0x7f,
        "Mon-Fri" => 0x1f,
        "Mon-Sat" => 0x3f,
        "Sat-Sun" => 0x60,
        _ => {
            let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            head.split(',').filter_map(|n| names.iter().position(|x| *x == n.trim())).fold(0u8, |m, i| m | (1 << i))
        }
    }
}

/// The work of a map line from its timetable (`crate::LineInfo`) of a day: its tours that run
/// that day (all of them when none does), their hours and kilometres, journeys with passengers,
/// and the days of the week its tours run on (six when they do not say).
pub fn work_of(l: &crate::LineInfo) -> Work {
    let runs: Vec<&crate::TourInfo> = l.tours.iter().filter(|t| t.runs).collect();
    let tours: Vec<&crate::TourInfo> = if runs.is_empty() { l.tours.iter().collect() } else { runs };
    let mut w = Work { buses: tours.len() as u32, ..Default::default() };
    for t in &tours {
        let dep = t.trips.iter().map(|x| x.departure).fold(f64::MAX, f64::min);
        let arr = t.trips.iter().map(|x| x.arrival).fold(f64::MIN, f64::max);
        if arr > dep {
            w.hours += (arr - dep) / 3600.0;
        }
        w.km += t.trips.iter().map(|x| x.km).sum::<f64>();
        w.journeys += t.trips.iter().filter(|x| x.stops.len() >= 3).count() as u32;
    }
    let mask = l.tours.iter().fold(0u8, |m, t| m | weekdays(&t.days));
    let days = if mask == 0 { DEFAULT_DAYS_A_WEEK } else { mask.count_ones() };
    w.days = days * 52;
    w
}

/// A line's operating days a week when its timetable does not say (as the old reference had it).
pub const DEFAULT_DAYS_A_WEEK: u32 = 6;

/// A work of only its day's tours and kilometres (a tender of an older file): about 20 km an
/// hour, four journeys a tour, six days a week.
pub fn rough_work(tours: u32, km: f64) -> Work {
    Work { hours: km.max(20.0) / 20.0, km: km.max(20.0), journeys: tours.max(1) * 4, buses: tours.max(1), days: DEFAULT_DAYS_A_WEEK * 52 }
}

// --- making a contract -------------------------------------------------------------------------------

/// The term of a kind of contract: its core years and the extension years it may earn (a local
/// bus contract 3 + 5, a school run 3 + 2).
pub fn term(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Hire => econ_uk::TERM_SCHOOL,
        _ => econ_uk::TERM_LOCAL,
    }
}

/// A new contract for `line` from `from` until `until`, at `year_price` a year (cents, at today's
/// contract index), made for the `work`.
#[allow(clippy::too_many_arguments)]
pub fn make(c: &mut Company, line: &str, number: &str, kind: Kind, period: Period, year_price: Cents, w: &Work, from: &str, until: &str) -> Contract {
    c.concessions.contract_counter += 1;
    let (_, extensions) = term(kind);
    Contract {
        id: c.concessions.contract_counter,
        line: line.to_string(),
        number: number.to_string(),
        kind,
        period,
        year_price,
        index: c.contract_index,
        days_a_year: w.days.max(1),
        paid_months: if kind == Kind::Net { 12 } else { 0 },
        journeys: w.journeys.max(1),
        from: from.to_string(),
        until: until.to_string(),
        extensions_left: extensions,
        extended: 0,
        decided: String::new(),
        ended: None,
        end: None,
        fare_base: None,
        record: Vec::new(),
        month: Tally::default(),
        paid: Vec::new(),
    }
}

/// A school contract's price a coach and school day now (cents): the data's day price, by the
/// contracts' factor of the company's game and its prices since its founding.
pub fn school_day_price(c: &Company) -> f64 {
    econ_uk::value(c, Item::Contracts, (econ_uk::SCHOOL_DAY.0 + econ_uk::SCHOOL_DAY.1) / 2.0)
}

/// School days a year a school run is paid for (the game's school calendar has about these).
pub const SCHOOL_DAYS: u32 = 190;

/// The company's own school lines get a private-hire contract with the school authority: a
/// coach (each of the line's tours) at the data's school-day price, for the school days of a
/// term (3 + 2 years). A line whose contract was ended for default gets none for a year.
/// Returns whether any was made.
pub fn ensure_hires(c: &mut Company) -> bool {
    let today = c.date.clone();
    let year_ago = dates::add(&today, -econ_uk::POINTS_DAYS);
    let school: Vec<(String, String, u32)> = c.lines.iter().filter(|l| l.own && super::ownline::kind_of(l) == ServiceKind::School).map(|l| (l.name.clone(), l.number.clone(), l.tours.max(1))).collect();
    let mut made = false;
    for (line, number, coaches) in school {
        let mine = |k: &&Contract| k.line.eq_ignore_ascii_case(&line);
        if c.concessions.contracts.iter().filter(mine).any(|k| k.live() || (k.end == Some(End::Terminated) && k.ended.as_deref().is_some_and(|e| dates::between(&year_ago, e) >= 0))) {
            continue;
        }
        let w = Work { days: SCHOOL_DAYS, buses: coaches, journeys: coaches * 2, ..Default::default() };
        let price = (school_day_price(c) * coaches as f64 * SCHOOL_DAYS as f64).round() as Cents;
        let (years, _) = term(Kind::Hire);
        let until = dates::add(&today, years as i64 * 365 - 1);
        let k = make(c, &line, &number, Kind::Hire, Period::Day, price, &w, &today, &until);
        c.concessions.contracts.push(k);
        made = true;
    }
    made
}

/// Contracts brought in line with their concessions (a map line's concession says when it
/// ends: renewed by itself on an easy economy), contracts whose term ran out marked so, a net
/// subsidy's first fare noted, and the record older than a year dropped - those ended more than
/// a year ago with it.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let year_ago = dates::add(&today, -econ_uk::POINTS_DAYS);
    let held: Vec<(String, String)> = c.concessions.held.iter().map(|h| (h.line.clone(), h.until.clone())).collect();
    let fares: Vec<(String, Cents)> = c.lines.iter().map(|l| (l.name.clone(), super::fares::fare_of(c, l))).collect();
    // (the last contract of each line: a concession renewed by itself runs it longer - one won
    // again has its own contract after the old one, which keeps its end)
    let last: Vec<u32> = c.concessions.contracts.iter().filter(|k| k.live()).filter(|k| !c.concessions.contracts.iter().any(|x| x.live() && x.id != k.id && x.line.eq_ignore_ascii_case(&k.line) && dates::between(&k.from, &x.from) > 0)).map(|k| k.id).collect();
    for k in c.concessions.contracts.iter_mut().filter(|k| k.live()) {
        if let Some((_, until)) = held.iter().find(|h| h.0.eq_ignore_ascii_case(&k.line)) {
            if last.contains(&k.id) && dates::between(&k.until, until) > 0 {
                k.until = until.clone();
            }
        }
        if dates::between(&k.until, &today) > 0 {
            k.ended = Some(k.until.clone());
            k.end = Some(End::Expired);
        }
        if k.kind == Kind::Net && k.fare_base.is_none() {
            k.fare_base = fares.iter().find(|f| f.0.eq_ignore_ascii_case(&k.line)).map(|f| f.1);
        }
    }
    for k in c.concessions.contracts.iter_mut() {
        k.record.retain(|d| dates::between(&year_ago, &d.date) >= 0);
    }
    c.concessions.contracts.retain(|k| k.ended.as_deref().is_none_or(|e| dates::between(&year_ago, e) >= 0));
}

// --- the term: extensions, the end --------------------------------------------------------------------

/// What came of a contract's term (for the day's report).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// An extension year earned (green): it runs until `until`.
    Extended { number: String, until: String },
    /// Not green when its extension was decided: it ends on `until` (and is put out again).
    NotExtended { number: String, until: String },
    /// Ended by the authority for default (red); the company paid `cost` for the replacement.
    Terminated { number: String, cost: Cents },
}

/// The contracts whose next extension is due (`concessions::RENEW_BEFORE` days before their
/// end): a green one runs a year longer (its concession with it), any other loses the
/// extensions it had left - its line is put out to tender again at the end. Returns what came of
/// it.
pub fn extend_due(c: &mut Company) -> Vec<Event> {
    let today = c.date.clone();
    let mut out = Vec::new();
    let due: Vec<u32> = c.concessions.contracts.iter().filter(|k| k.live() && k.decided != k.until && dates::between(&today, &k.until) <= super::concessions::RENEW_BEFORE && dates::between(&k.from, &today) >= 0).map(|k| k.id).collect();
    for id in due {
        let Some(k) = c.concessions.contracts.iter_mut().find(|k| k.id == id) else { continue };
        k.decided = k.until.clone();
        if k.extensions_left == 0 {
            continue;
        }
        if k.rag(&today) == Rag::Green {
            k.extensions_left -= 1;
            k.extended += 1;
            k.until = dates::add(&k.until, 365);
            k.decided = String::new();
            let (line, until, number) = (k.line.clone(), k.until.clone(), k.number.clone());
            if let Some(h) = c.concessions.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(&line)) {
                h.until = until.clone();
            }
            out.push(Event::Extended { number, until });
        } else {
            k.extensions_left = 0;
            out.push(Event::NotExtended { number: k.number.clone(), until: k.until.clone() });
        }
    }
    out
}

/// What leaving a contract costs now: `months` of its average payment of the last three months
/// (none paid yet: its price's month).
pub fn leaving_cost(c: &Company, k: &Contract, months: f64) -> Cents {
    let recent: Vec<Cents> = k.paid.iter().rev().take(3).map(|p| p.1).collect();
    let month = if recent.is_empty() { month_value(c, k) } else { recent.iter().sum::<Cents>() as f64 / recent.len() as f64 };
    (month.max(0.0) * months).round() as Cents
}

/// What handing back a line's contract would cost now (None: no contract on it).
pub fn hand_back_cost(c: &Company, line: &str) -> Option<Cents> {
    let k = of_line(c, line)?;
    Some(leaving_cost(c, k, econ_uk::EARLY_END_MONTHS))
}

/// The company hands a line's contract back before its end (the line given up, or the contract
/// alone on the contracts page): a month's average payment, booked; the contract ends today (a
/// map line's concession with it - its line goes tonight). Returns what it cost (None: no
/// contract to hand back).
pub fn hand_back(c: &mut Company, line: &str) -> Option<Cents> {
    let id = of_line(c, line)?.id;
    let k = c.concessions.contracts.iter().find(|k| k.id == id)?.clone();
    let cost = leaving_cost(c, &k, econ_uk::EARLY_END_MONTHS);
    c.book(BookingKind::Penalty, -cost, format!("Line {}: contract handed back early", k.number), false);
    let today = c.date.clone();
    if let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) {
        x.ended = Some(today.clone());
        x.end = Some(End::HandedBack);
    }
    if let Some(h) = c.concessions.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(line) && !h.direct) {
        if dates::between(&today, &h.until) > 0 {
            h.until = today;
        }
    }
    Some(cost)
}

/// A line's contract ends with its term (its concession ended): no cost.
pub fn expire(c: &mut Company, line: &str, date: &str) {
    for k in c.concessions.contracts.iter_mut().filter(|k| k.live() && k.line.eq_ignore_ascii_case(line) && dates::between(&k.from, date) >= 0) {
        k.ended = Some(date.to_string());
        k.end = Some(End::Expired);
    }
}

// --- the day -------------------------------------------------------------------------------------------

/// A journey of the day as the close saw it.
#[derive(Clone, Debug, PartialEq)]
struct Journey {
    line: String,
    tour: String,
    /// Off the timetable (negative early), whole minutes; not run; run only in part.
    minutes: i32,
    run: bool,
    part: bool,
    bus: Option<u32>,
    driver: Option<u32>,
    wants: Option<BusSize>,
}

/// The day's journeys of every line with passengers, as the close runs them (`day::close_day`
/// hands them in; the contract lines' are judged by `close_day`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Journeys {
    list: Vec<Journey>,
}

impl Journeys {
    /// A journey not run (dropped, or on no bus).
    pub fn missed(&mut self, line: &str, tour: &str) {
        self.list.push(Journey { line: line.to_string(), tour: tour.to_string(), minutes: 0, run: false, part: false, bus: None, driver: None, wants: None });
    }

    /// A journey run `minutes` off the timetable (negative early), `part`ly cut short, with that
    /// bus and driver, of a tour that asks for buses of `wants`.
    #[allow(clippy::too_many_arguments)]
    pub fn ran(&mut self, line: &str, tour: &str, minutes: i32, part: bool, bus: Option<u32>, driver: Option<u32>, wants: Option<BusSize>) {
        self.list.push(Journey { line: line.to_string(), tour: tour.to_string(), minutes, run: true, part, bus, driver, wants });
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// A contract's day, for the report.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ContractDay {
    pub number: String,
    pub kind: Kind,
    /// Its journeys timetabled today, those not paid, what it paid (by its period: nothing on a
    /// day it is not paid) and what the journeys not paid cost.
    pub journeys: u32,
    pub unpaid: u32,
    pub paid: Cents,
    pub deducted: Cents,
    /// Today's default points, and its net points and rating after them.
    pub points: f64,
    pub net: f64,
    pub rag: Rag,
}

/// The day of the company's contracts, for the report.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ContractsDay {
    pub paid: Cents,
    pub deducted: Cents,
    pub unpaid: u32,
    pub points: f64,
    /// The fares of gross-cost lines handed to the authority.
    pub fares_handed: Cents,
    pub lines: Vec<ContractDay>,
}

/// A day of storm or snow (the inbox's world events): the company did its best.
pub fn bad_weather(c: &Company, date: &str) -> bool {
    super::inbox::running(c, date).any(|e| e.key == "storm" || e.key == "snow")
}

/// The contracts' day, as the day's close runs it after its lines are booked (`lines`: the
/// day's lines, their fares and payments): every contract line's journeys judged - unpaid, default
/// points -, its payment by its period booked (`BookingKind::Contract`) with the journeys not
/// paid off it, a gross-cost line's fares handed to the authority, and no payment per kilometre
/// on a contract line (the contract is its pay). On the month's last day the quality points and
/// the authority's verdict on a red contract (`month_end`). Extensions due are decided
/// (`extend_due`). The report gets the sums and notes.
pub fn close_day(c: &mut Company, date: &str, journeys: &Journeys, lines: &mut [LineDay], report: &mut DayReport) {
    ensure_hires(c);
    tidy(c);
    let strict = strictness(c);
    let weather = bad_weather(c, date);
    let month = dates::month_of(date);
    let day = dates::parse(date).unwrap_or(0);
    let sunday = dates::weekday(day) == 6;
    let last = dates::last_of_month(date);
    let (y, m, _) = dates::civil_from_days(day);
    let month_len = dates::days_in_month(y, m) as i64;
    let mut out = ContractsDay::default();
    let ids: Vec<u32> = c.concessions.contracts.iter().filter(|k| k.on(date)).map(|k| k.id).collect();
    for id in ids {
        let Some(k) = c.concessions.contracts.iter().find(|k| k.id == id).cloned() else { continue };
        let mine: Vec<&Journey> = journeys.list.iter().filter(|j| j.line.eq_ignore_ascii_case(&k.line)).collect();
        let n = mine.len() as u32;
        let mut pd = PointsDay { date: date.to_string(), ..Default::default() };
        let mut on_time_n = 0;
        let mut weather_half = 0u32;
        // (a bus or a driver is judged once a day on a line, a bus's size once a tour)
        let mut seen: Vec<String> = Vec::new();
        let mut once = |key: String| {
            if seen.contains(&key) {
                false
            } else {
                seen.push(key);
                true
            }
        };
        for j in &mine {
            if !j.run {
                pd.missed += 1;
                pd.unpaid += 1;
                if weather {
                    weather_half += 1;
                }
                continue;
            }
            if on_time(j.minutes) && !j.part {
                on_time_n += 1;
            }
            if j.part || (!weather && unpaid_at(c, j.minutes)) {
                pd.unpaid += 1;
            }
            if !weather {
                let p = time_points(j.minutes);
                if p >= econ_uk::POINTS_FAR {
                    pd.far += 1;
                } else if p > 0.0 {
                    pd.slight += 1;
                }
            }
            if let Some(v) = j.bus.and_then(|b| c.vehicle(b)) {
                if j.wants.is_some_and(|w| super::ownline::capacity(v.kind.size) < super::ownline::capacity(w)) && once(format!("size/{}/{}", j.tour, v.id)) {
                    pd.vehicle += 1;
                }
                if v.condition < econ_uk::UNROADWORTHY && once(format!("bus/{}", v.id)) {
                    pd.serious += 1;
                }
                if let Some(e) = j.driver.and_then(|d| c.employee(d)) {
                    if super::licences::lack(e, v.kind, Some(&v.bus)).is_some() && once(format!("driver/{}/{}", e.id, v.id)) {
                        pd.serious += 1;
                    }
                }
            }
        }
        // (the points by the game's strictness; on a day of storm or snow none for the time)
        let raw = pd.slight as f64 * econ_uk::POINTS_SLIGHT + pd.far as f64 * econ_uk::POINTS_FAR + (pd.missed + pd.vehicle) as f64 * econ_uk::POINTS_MISSED + pd.serious as f64 * econ_uk::POINTS_SERIOUS;
        pd.default = raw * strict;
        // (what it pays today by its period)
        let pay = match k.period {
            Period::Day if n > 0 => payment(c, &k),
            Period::Week if sunday => {
                let held = (dates::between(&k.from, date) + 1).clamp(1, 7);
                (payment(c, &k) as f64 * held as f64 / 7.0).round() as Cents
            }
            Period::Month if last && !(k.months() == 11 && m == 8) => {
                let held = (dates::between(&k.from, date) + 1).clamp(1, month_len);
                (payment(c, &k) as f64 * held as f64 / month_len as f64).round() as Cents
            }
            _ => 0,
        };
        // (each journey not paid its share of the day: a net subsidy's day is its year's share)
        let per = if strict > 0.0 && n > 0 { day_value(c, &k) / n as f64 } else { 0.0 };
        let full = pd.unpaid - weather_half;
        let deducted = if per > 0.0 { (per * (full as f64 + weather_half as f64 * (1.0 - econ_uk::WEATHER_PAID.1))).round() as Cents } else { 0 };
        let deducted = if strict > 0.0 { deducted } else { 0 };
        if strict <= 0.0 {
            pd.unpaid = 0;
        }
        pd.deducted = deducted;
        let label = Kind::label(k.kind).to_lowercase();
        if pay != 0 {
            c.book(BookingKind::Contract, pay, format!("Line {}: {label} contract", k.number), false);
        }
        if deducted != 0 {
            c.book(BookingKind::Contract, -deducted, format!("Line {}: {} journeys not paid", k.number, pd.unpaid), false);
        }
        // (the line's day: a gross-cost line's fares to the authority, no payment per km)
        if let Some(l) = lines.iter_mut().find(|l| l.line.eq_ignore_ascii_case(&k.line)) {
            if k.kind == Kind::Gross && l.fares != 0 {
                c.book(BookingKind::Fares, -l.fares, format!("Line {}: fares to the authority (gross cost)", k.number), false);
                out.fares_handed += l.fares;
                l.revenue -= l.fares;
                l.fares = 0;
            }
            // (the old payment per km, until the income model of wave 2b replaces it: a contract
            // line is paid by its contract)
            if l.compensation != 0 {
                c.book(BookingKind::Compensation, -l.compensation, format!("Line {}: paid by its contract, not per km", k.number), false);
                l.revenue -= l.compensation;
                l.compensation = 0;
            }
            l.contract += pay;
            l.revenue += pay - deducted;
            l.penalty += deducted;
        }
        let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) else { continue };
        if x.month.month != month {
            x.month = Tally { month: month.clone(), ..Default::default() };
        }
        x.month.journeys += n;
        x.month.on_time += on_time_n;
        x.month.unpaid += pd.unpaid;
        x.month.serious += pd.serious;
        x.month.paid += pay - deducted;
        let today_points = pd.default;
        if pd.default > 0.0 || pd.unpaid > 0 {
            x.record.push(pd.clone());
        }
        let net = x.net_points(date);
        out.paid += pay;
        out.deducted += deducted;
        out.unpaid += pd.unpaid;
        out.points += today_points;
        out.lines.push(ContractDay { number: x.number.clone(), kind: x.kind, journeys: n, unpaid: pd.unpaid, paid: pay, deducted, points: today_points, net, rag: Rag::of(net) });
    }
    if last {
        for e in month_end(c, date) {
            report.notes.push(note_of(e));
        }
    }
    for e in extend_due(c) {
        report.notes.push(note_of(e));
    }
    report.penalties += out.deducted;
    report.contracts = out;
}

/// A contract's event as the day's report notes it.
pub fn note_of(e: Event) -> Note {
    match e {
        Event::Extended { number, until } => Note::Extended { number, until },
        Event::NotExtended { number, until } => Note::NotExtended { number, until },
        Event::Terminated { number, cost } => Note::Terminated { number, cost },
    }
}

/// The month's end of every contract running (`date` its last day): its quality points - a
/// roadside inspection drawn now and then (clean: no bus run unroadworthy this month and the
/// line's buses kept), no complaint about the line, 98 % on time -, its month's payment kept for
/// the average, and the authority's verdict on a red one where the mode lets it end contracts:
/// ended today, the company paying the replacement's extra cost (`DEFAULT_END_MONTHS`).
pub fn month_end(c: &mut Company, date: &str) -> Vec<Event> {
    let month = dates::month_of(date);
    let mode = econ_uk::mode(c.game_mode);
    let mut out = Vec::new();
    let ids: Vec<u32> = c.concessions.contracts.iter().filter(|k| k.on(date)).map(|k| k.id).collect();
    for id in ids {
        let Some(k) = c.concessions.contracts.iter().find(|k| k.id == id).cloned() else { continue };
        let complaints = c.complaints.list.iter().filter(|x| dates::month_of(&x.date) == month && (x.line.eq_ignore_ascii_case(&k.number) || x.line.eq_ignore_ascii_case(&k.line))).count();
        let mut rng = Rng::of(&[&c.id, "inspection", &k.line], dates::parse(date).unwrap_or(0));
        let fleet: Vec<f64> = c.fleet.iter().filter(|v| super::ownline::line_allows(c, &k.line, v)).map(|v| v.condition).collect();
        let kept = fleet.is_empty() || fleet.iter().sum::<f64>() / fleet.len() as f64 >= 50.0;
        let ran = k.month.month == month && k.month.journeys > 0;
        let mut pd = PointsDay { date: date.to_string(), ..Default::default() };
        if ran && rng.chance(econ_uk::INSPECTION_CHANCE) && k.month.serious == 0 && kept {
            pd.quality += econ_uk::QUALITY_INSPECTION;
            pd.why.push("inspection".into());
        }
        if ran && complaints == 0 {
            pd.quality += econ_uk::QUALITY_NO_COMPLAINTS;
            pd.why.push("complaints".into());
        }
        if ran && k.month.on_time as f64 >= econ_uk::QUALITY_ON_TIME_SHARE * k.month.journeys as f64 {
            pd.quality += econ_uk::QUALITY_ON_TIME;
            pd.why.push("on_time".into());
        }
        let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) else { continue };
        if pd.quality > 0.0 {
            x.record.push(pd);
        }
        if ran {
            x.paid.push((month.clone(), x.month.paid));
            let n = x.paid.len();
            if n > 3 {
                x.paid.drain(..n - 3);
            }
        }
        if mode.may_terminate && strictness(c) > 0.0 && c.concessions.contracts.iter().find(|x| x.id == id).is_some_and(|x| x.rag(date) == Rag::Red) {
            let k = c.concessions.contracts.iter().find(|x| x.id == id).cloned().unwrap_or(k);
            let cost = leaving_cost(c, &k, econ_uk::DEFAULT_END_MONTHS);
            c.book(BookingKind::Penalty, -cost, format!("Line {}: contract ended for default, the replacement's extra cost", k.number), false);
            if let Some(x) = c.concessions.contracts.iter_mut().find(|x| x.id == id) {
                x.ended = Some(date.to_string());
                x.end = Some(End::Terminated);
            }
            if let Some(h) = c.concessions.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(&k.line)) {
                h.until = date.to_string();
            }
            out.push(Event::Terminated { number: k.number.clone(), cost });
        }
    }
    out
}

/// A gross-cost contract paid by the day on `line`, from today for three years (for the tests
/// of the parts that weigh what a journey costs).
#[cfg(test)]
pub(crate) fn give(c: &mut Company, line: &str) {
    let today = c.date.clone();
    let until = dates::add(&today, 3 * 365 - 1);
    let w = Work { hours: 12.0, km: 240.0, journeys: 20, buses: 1, days: 312 };
    let k = make(c, line, line, Kind::Gross, Period::Day, 312_000_00, &w, &today, &until);
    c.concessions.contracts.push(k);
}

// --- older files ---------------------------------------------------------------------------------------

/// A file of before the bus contracts (version 3, wave 2a) brought onto them:
///
/// * a concession won in a tender (not taken on directly) becomes a gross-cost contract paid by
///   the day, for the rest of its term, at its winning price: the ask's share of the authority's
///   reference (`Concession::price`) of the cap the work's cost price gives now - the old asks
///   were sums of a per-kilometre payment on the old reference, not a price for the work;
/// * the company's own school lines get their private-hire contract (`ensure_hires`);
/// * open tenders are priced again on the new footing (`concessions::ensure`; their old bids
///   dropped, the fee stays paid);
/// * the map's rivals get their quality on the new scale (their performance, `Character::quality`).
pub fn migrate(c: &mut Company) {
    let held: Vec<super::concessions::Concession> = c.concessions.held.iter().filter(|h| !h.direct).cloned().collect();
    for h in held {
        if c.concessions.contracts.iter().any(|k| k.line.eq_ignore_ascii_case(&h.line)) {
            continue;
        }
        let tender = c.concessions.tenders.iter().rev().find(|t| t.line.eq_ignore_ascii_case(&h.line) && matches!(t.outcome, Some(super::concessions::Outcome::Won { .. }))).cloned();
        let line = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line));
        let work = match (&tender, line) {
            (Some(t), _) if t.work != Work::default() => t.work,
            (_, Some(l)) => rough_work(l.tours, l.km),
            (Some(t), None) => rough_work(t.day_tours, t.day_km),
            (None, None) => rough_work(1, 20.0),
        };
        let years = (dates::between(&h.from, &h.until) + 1).max(1) as f64 / 365.0;
        let cap = price_of(c, &work, Kind::Gross, years) * econ_uk::TENDER_CAP;
        let share = if h.price > 0.0 && h.price.is_finite() { h.price.min(1.0) } else { 1.0 };
        let price = (cap * share / 100_00.0).round() as Cents * 100_00;
        let k = make(c, &h.line, &h.number, Kind::Gross, Period::Day, price, &work, &h.from, &h.until);
        c.concessions.contracts.push(k);
    }
    ensure_hires(c);
    for t in c.concessions.tenders.iter_mut().filter(|t| t.open()) {
        t.reference = 0;
    }
    let (id, map) = (c.id.clone(), c.map.clone());
    for r in c.concessions.rivals.iter_mut() {
        let (lo, hi) = r.character.quality();
        r.quality = Rng::of(&[&id, &map, "rival quality"], r.id as i64).range(lo, hi).round();
    }
}

#[cfg(test)]
#[path = "bus_contracts_tests.rs"]
mod tests;

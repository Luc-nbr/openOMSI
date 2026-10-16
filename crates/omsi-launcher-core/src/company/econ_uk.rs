//! The economy's parameters, from real UK bus operator data (a 30-bus regional operator's books
//! and its tenders, 2019-2022). Every number the economy is built on lives here, once, with its
//! unit and where it comes from; the rest of the company code asks this module.
//!
//! # How it is meant
//!
//! * **Realistic is the data at factor 1.0.** Normal and Casual are derived from it with the mode
//!   factors (`mode`, the model's §7) - never guessed separately.
//! * **One model for every company.** A company plays in a country (`Company::country`, set at
//!   founding): the country's factors (`country`) scale wages, diesel, electricity, fares and the
//!   general price level against the UK (1.0).
//! * **The sandbox on top.** A sandbox company's knobs multiply what the mode and the country
//!   give (`factor`).
//! * **Euro cents inside.** The data is written below in **GBP pence at 2022 prices**, in this
//!   file only; `gbp` converts it to euro cents at the currency's fixed rate
//!   (`currency::Currency::Gbp`). Nothing else converts. Prices then rise with the company's own
//!   inflation (`Company::price_index`), which starts at 1 on its founding day.
//!
//! # The API (waves 2a and 2b build on it)
//!
//! * `factor(c, Item)`: the multiplier of one kind of money for a company - mode × country ×
//!   sandbox (`factor_at` for a mode, country and knobs without a company).
//! * `value(c, Item, pence)`: a UK amount (GBP pence, 2022) as this company pays or earns it now,
//!   in euro cents: `gbp(pence) × factor × price_index`.
//! * `Level::of(c)`: every cost factor of a company at once, with its price index (what the cost
//!   functions of `economy` take); `Level::uk(pi)` is Realistic in the UK.
//! * `mode(GameMode)`: the mode factors, including those the contracts, tenders and penalties of
//!   wave 2a will use (`ModeFactors`).
//! * `country(key)`: a country's relative price levels (`Country`).
//! * The constants: costs (drivers, other staff, fuel, workshop, vehicles, fixed costs per bus,
//!   overheads) are used by wave 1; income (fares, concessions, BSOG, advertising), contracts
//!   and tenders, performance and demand are here for waves 2a and 2b.
//!
//! What the sandbox knob of each item is: `Wages` none here (the sandbox's wage factor is applied
//! when wages are booked, `sandbox::booked`); `Diesel`, `Power` and `Workshop` its running costs;
//! `Vehicles` its prices; `Fares`, `Concessions`, `Contracts` and `Grants` its income; `Insurance` and
//! `Overheads` none.

use super::currency::Currency;
use super::model::{BusKind, BusSize, Cents, Company, Drive, GameMode, Role};
use super::sandbox::Sandbox;

// --- the currency --------------------------------------------------------------------------------

/// A UK amount in GBP pence as euro cents (fractional), at the fixed rate the company's currency
/// shows it back with: a GBP company sees the pence it was given.
pub fn gbp(pence: f64) -> f64 {
    pence / Currency::Gbp.rate()
}

/// `gbp`, rounded to whole euro cents.
pub fn gbp_cents(pence: f64) -> Cents {
    gbp(pence).round() as Cents
}

// --- the game modes (§7) ------------------------------------------------------------------------

/// What a game mode makes of the UK data (Realistic: all 1.0, the data as it is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeFactors {
    /// Income: fares, contracts, concession reimbursement.
    pub income: f64,
    /// Costs: wages, fuel and energy, the workshop, the buses (price, lease, depreciation),
    /// insurance and tax.
    pub costs: f64,
    /// The central overhead per bus (`OVERHEAD_PER_BUS_YEAR`): full, half, none.
    pub overheads: f64,
    /// Default points and deductions of a contract (wave 2a), and whether a contract in the red
    /// may be terminated.
    pub default_points: f64,
    pub may_terminate: bool,
    /// From how many minutes late a journey of a contract is not paid (wave 2a; None: never).
    /// Realistic: the data's `UNPAID_LATE`.
    pub unpaid_late_from: Option<i32>,
    /// Early running unpaid (wave 2a): Realistic and Normal as the data, Casual never.
    pub unpaid_early: bool,
    /// The rivals' bids around their cost price (wave 2a): lowest and highest, as a share
    /// (-0.15 = 15 % under it).
    pub rival_bids: (f64, f64),
    /// Overtime is paid at `OVERTIME_RATE` above the contracted hours (Casual: plain hours).
    pub overtime: bool,
}

/// The factors of a game mode (the model's §7).
pub fn mode(m: GameMode) -> ModeFactors {
    match m {
        GameMode::Realistic => ModeFactors { income: 1.00, costs: 1.00, overheads: 1.0, default_points: 1.0, may_terminate: true, unpaid_late_from: Some(UNPAID_LATE), unpaid_early: true, rival_bids: (-0.15, 0.15), overtime: true },
        GameMode::Normal => ModeFactors { income: 1.15, costs: 0.90, overheads: 0.5, default_points: 0.5, may_terminate: false, unpaid_late_from: Some(15), unpaid_early: true, rival_bids: (0.0, 0.20), overtime: true },
        GameMode::Casual => ModeFactors { income: 1.35, costs: 0.75, overheads: 0.0, default_points: 0.0, may_terminate: false, unpaid_late_from: None, unpaid_early: false, rival_bids: (0.10, 0.30), overtime: false },
    }
}

// --- the countries ------------------------------------------------------------------------------

/// A country's price levels against the UK (1.0). Well-founded estimates, rounded to 0.05:
///
/// * `wages`: the relative hourly pay of a bus driver (collective agreements and wage surveys of
///   the early 2020s, at market exchange rates);
/// * `diesel`: the relative price of diesel to an operator, without VAT (duty included, as an
///   operator pays it);
/// * `power`: the relative price of electricity to a business customer, without VAT;
/// * `fares`: the relative price of an adult single in a town;
/// * `general`: the relative general price level (price level indices of household
///   consumption) - parts, insurance, rents, the overheads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Country {
    pub key: &'static str,
    pub wages: f64,
    pub diesel: f64,
    pub power: f64,
    pub fares: f64,
    pub general: f64,
}

impl Country {
    /// Buses are bought on an international market: only a quarter of their price follows the
    /// local price level.
    pub fn vehicles(&self) -> f64 {
        0.75 + 0.25 * self.general
    }

    /// What running a bus costs there against the UK: the price level weighted as a UK
    /// operator's direct costs are (drivers 47 %, fuel 25 %, the rest 28 %) - for what follows
    /// the cost of running a service, such as a contract's price (wave 2a).
    pub fn running(&self) -> f64 {
        0.47 * self.wages + 0.25 * self.diesel + 0.28 * self.general
    }
}

const fn land(key: &'static str, wages: f64, diesel: f64, power: f64, fares: f64, general: f64) -> Country {
    Country { key, wages, diesel, power, fares, general }
}

/// Every country of `names` (its keys), International first: the UK's own data unchanged.
pub const COUNTRIES: [Country; 28] = [
    // (a mix of the countries: the reference itself)
    land("intl", 1.00, 1.00, 1.00, 1.00, 1.00),
    land("gb", 1.00, 1.00, 1.00, 1.00, 1.00),
    land("nl", 1.15, 1.00, 0.90, 1.00, 0.95),
    land("be", 1.15, 0.95, 0.95, 0.90, 0.95),
    land("de", 1.15, 0.95, 1.00, 1.05, 0.90),
    land("at", 1.05, 0.90, 0.85, 0.85, 0.95),
    land("ch", 1.90, 1.15, 0.85, 1.30, 1.40),
    land("ie", 1.25, 1.00, 1.05, 0.90, 1.20),
    land("fr", 0.95, 0.95, 0.75, 0.65, 0.90),
    land("pl", 0.50, 0.85, 0.80, 0.35, 0.50),
    land("cz", 0.50, 0.90, 0.85, 0.40, 0.65),
    land("sk", 0.45, 0.90, 0.85, 0.35, 0.60),
    land("hu", 0.40, 0.85, 0.75, 0.40, 0.50),
    land("ro", 0.35, 0.85, 0.80, 0.30, 0.45),
    land("ua", 0.15, 0.70, 0.35, 0.10, 0.35),
    land("ru", 0.25, 0.45, 0.30, 0.20, 0.40),
    land("it", 0.85, 1.00, 1.00, 0.65, 0.85),
    land("es", 0.80, 0.85, 0.80, 0.55, 0.75),
    land("pt", 0.55, 0.95, 0.80, 0.60, 0.70),
    land("dk", 1.65, 1.00, 0.95, 1.25, 1.20),
    land("se", 1.25, 1.10, 0.60, 1.25, 1.00),
    land("no", 1.60, 1.10, 0.50, 1.40, 1.25),
    land("fi", 1.10, 1.05, 0.60, 1.05, 1.05),
    land("us", 1.40, 0.65, 0.55, 0.80, 1.05),
    land("ca", 1.25, 0.80, 0.45, 0.75, 1.00),
    land("au", 1.35, 0.85, 0.85, 0.85, 1.10),
    land("tr", 0.25, 0.75, 0.55, 0.20, 0.35),
    land("gr", 0.55, 0.95, 0.85, 0.45, 0.70),
];

/// A country by its key (`names`' keys, any case); an unknown one: International (the UK's).
pub fn country(key: &str) -> &'static Country {
    COUNTRIES.iter().find(|x| x.key.eq_ignore_ascii_case(key.trim())).unwrap_or(&COUNTRIES[0])
}

/// The country a company plays in (a company of before the countries: its map's).
pub fn country_of(c: &Company) -> &'static Country {
    country(super::names::country_of(c))
}

// --- the factors ---------------------------------------------------------------------------------

/// A kind of money the model scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    /// Wages of every role (and the agency's hour).
    Wages,
    /// Diesel, and electricity for electric buses.
    Diesel,
    Power,
    /// Maintenance, repairs, tyres and parts.
    Workshop,
    /// A bus's price, and with it its lease and its depreciation.
    Vehicles,
    /// Insurance, road tax and the test, the ticket machine.
    Insurance,
    /// The central overhead per bus (depot rent, management, admin, control, IT).
    Overheads,
    /// Fares (wave 2b).
    Fares,
    /// Concession reimbursement (wave 2b).
    Concessions,
    /// Contract and tender payments (wave 2a).
    Contracts,
    /// BSOG, the fuel grant per live mile (it began as a fuel duty rebate: it follows the diesel's
    /// price; the sandbox's knob on income).
    Grants,
}

/// The factor of `item` for a game mode, a country and a sandbox's knobs: mode × country ×
/// sandbox (see the module's text for which knob is whose).
pub fn factor_at(m: GameMode, land: &Country, s: &Sandbox, item: Item) -> f64 {
    let f = mode(m);
    match item {
        Item::Wages => f.costs * land.wages,
        Item::Diesel => f.costs * land.diesel * s.running,
        Item::Power => f.costs * land.power * s.running,
        Item::Workshop => f.costs * land.general * s.running,
        Item::Vehicles => f.costs * land.vehicles() * s.prices,
        Item::Insurance => f.costs * land.general,
        Item::Overheads => f.overheads * land.general,
        Item::Fares => f.income * land.fares * s.income,
        Item::Concessions => f.income * land.fares * s.income,
        Item::Contracts => f.income * land.running() * s.income,
        Item::Grants => f.income * land.diesel * s.income,
    }
}

/// The factor of `item` for a company: its game mode × its country × its sandbox.
pub fn factor(c: &Company, item: Item) -> f64 {
    factor_at(c.game_mode, country_of(c), super::sandbox::of(c), item)
}

/// A UK amount (GBP pence at 2022 prices) as the company pays or earns it today, in euro cents
/// (fractional): converted, by the company's factor, and by its prices since its founding.
pub fn value(c: &Company, item: Item, pence: f64) -> f64 {
    gbp(pence) * factor(c, item) * c.price_index
}

/// Every cost factor of a company at once, and its price index: what the cost functions of
/// `economy` take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub price_index: f64,
    pub wages: f64,
    pub diesel: f64,
    pub power: f64,
    pub workshop: f64,
    pub vehicles: f64,
    pub insurance: f64,
    pub overheads: f64,
}

impl Level {
    /// A mode's, a country's and a sandbox's, at a price index.
    pub fn at(m: GameMode, land: &Country, s: &Sandbox, price_index: f64) -> Level {
        let f = |item| factor_at(m, land, s, item);
        Level { price_index, wages: f(Item::Wages), diesel: f(Item::Diesel), power: f(Item::Power), workshop: f(Item::Workshop), vehicles: f(Item::Vehicles), insurance: f(Item::Insurance), overheads: f(Item::Overheads) }
    }

    /// The company's.
    pub fn of(c: &Company) -> Level {
        Level::at(c.game_mode, country_of(c), super::sandbox::of(c), c.price_index)
    }

    /// Realistic in the UK, without a sandbox: the data as it is.
    pub fn uk(price_index: f64) -> Level {
        Level::at(GameMode::Realistic, country("gb"), &super::sandbox::CAREER, price_index)
    }
}

// --- drivers (§3.1) -------------------------------------------------------------------------------

/// A driver's base wage, pence per paid hour (2022; 2021 £12.30, 2019 £12.80 in tenders).
pub const DRIVER_BASE_HOUR: f64 = 1224.0;
/// What a beginner and the most experienced driver earn against the base (a single grade in the
/// data; an estimate of the spread around it).
pub const DRIVER_LOW: f64 = 0.94;
pub const DRIVER_HIGH: f64 = 1.08;
/// The employer's National Insurance and pension on the wage (+8.9 to +10.3 % in the data).
pub const ON_COSTS: f64 = 0.09;
/// Holiday and sick pay and allowances on the wage (+11 to +14 % in the data). With the on-costs
/// an hour costs base × 1.21 (£14.81 at the base rate; the data's "£14.0 an hour" is the same
/// without the employer's National Insurance and pension). Holidays and sick days are paid
/// through this share, not as hours (`pay`).
pub const ALLOWANCES: f64 = 0.12;
/// The hours of a driver's contract a week: paid at least (the weekly guarantee, for the days
/// the driver is there) and overtime above them.
pub const CONTRACT_WEEK_HOURS: f64 = 39.0;
/// Overtime: above the contracted hours of the week, half as much again.
pub const OVERTIME_RATE: f64 = 1.5;
/// An agency driver's hour against a driver's all-in hour: booked ahead (typically +30 %), and
/// sent on the day itself (dearer still). Estimates.
pub const AGENCY_AHEAD: f64 = 1.30;
pub const AGENCY_SAME_DAY: f64 = 1.50;
/// Paid hours per timetabled bus-hour in the data (1.4-1.6: signing on and off, positioning,
/// breaks, spread-over, minimum duty lengths, the weekly guarantee) - for pricing a tender
/// (wave 2a); the game pays the hours its duties really have.
pub const PAID_PER_TIMETABLED_HOUR: f64 = 1.5;

/// The hours a month's wage pays: the contracted week's.
pub fn contract_month_hours() -> f64 {
    CONTRACT_WEEK_HOURS * 52.0 / 12.0
}

/// What an hour of wage costs the company with its on-costs and allowances.
pub fn all_in() -> f64 {
    1.0 + ON_COSTS + ALLOWANCES
}

// --- the other staff -------------------------------------------------------------------------------

/// The hourly base of a role (pence, 2022) and what the most experienced earn on top (a share).
/// The data has only the drivers' rate and lump sums for the others: these are estimates of the
/// UK market of 2022 (fitters a little above the drivers, cleaners just above the national
/// living wage, office and control staff about the drivers').
pub fn role_hour(role: Role) -> (f64, f64) {
    match role {
        Role::Driver => (DRIVER_BASE_HOUR * DRIVER_LOW, DRIVER_HIGH / DRIVER_LOW - 1.0),
        Role::Mechanic => (1400.0, 0.30),
        Role::Researcher => (1700.0, 0.45),
        Role::Cleaner => (980.0, 0.12),
        Role::Service => (1100.0, 0.25),
        Role::Inspector => (1150.0, 0.20),
    }
}

// --- fuel and energy (§3.2) -------------------------------------------------------------------------

/// Diesel, pence per litre without VAT (2022: £1.14-1.17 bulk).
pub const DIESEL_PER_LITRE: f64 = 115.0;
/// Electricity at the depot, pence per kWh (tender data, about £0.10).
pub const POWER_PER_KWH: f64 = 10.0;

/// Diesel (litres per 100 km) or electricity (kWh per 100 km) a bus of this kind uses in
/// service. Diesel: a single-deck runs the data's fleet average in town (2.13 miles a litre, 29
/// l/100 km; midibuses 11.6-12.6 mpg); articulated and double-deck are estimates. Electric:
/// 1.2 kWh/km for a single-deck (vendor data), the others scaled by size (estimates).
pub fn consumption(kind: BusKind) -> f64 {
    match (kind.size, kind.drive) {
        (BusSize::Midi, Drive::Diesel) => 24.0,
        (BusSize::Solo, Drive::Diesel) => 29.0,
        (BusSize::Articulated, Drive::Diesel) => 40.0,
        (BusSize::Double, Drive::Diesel) => 35.0,
        (BusSize::Midi, Drive::Electric) => 90.0,
        (BusSize::Solo, Drive::Electric) => 120.0,
        (BusSize::Articulated, Drive::Electric) => 170.0,
        (BusSize::Double, Drive::Electric) => 150.0,
    }
}

// --- the workshop (§3.3) -----------------------------------------------------------------------------

/// Maintenance, repairs, tyres and parts of a diesel single-deck, pence per km, at the fleet's
/// average age (£0.22 a mile; about £1,100 per bus a month).
pub const WORKSHOP_PER_KM: f64 = 13.7;
/// The data fleet's average age, the age `WORKSHOP_PER_KM` is for.
pub const WORKSHOP_AGE: f64 = 6.0;
/// What a year of age more or less changes it, and its bounds (an estimate).
pub const WORKSHOP_PER_YEAR: f64 = 0.05;
/// Electric buses need less workshop (20-30 % cheaper).
pub const WORKSHOP_ELECTRIC: f64 = 0.75;
/// The data's workshop per km is all of it, breakdowns included; the game books a breakdown's
/// repair on its own (`BREAKDOWN_REPAIR`), so the routine per km is this share of it. The two
/// are estimates set so that a Realistic fleet's workshop - routine, breakdowns and the parts
/// they take - comes to the data's.
pub const WORKSHOP_ROUTINE: f64 = 0.65;
/// A breakdown's repair, a single-deck's, pence: from a road call and a small part to a major
/// repair (an estimate; by size: `size_factor`; the parts fitted from the stores come off it).
pub const BREAKDOWN_REPAIR: (f64, f64) = (15_000.0, 90_000.0);

/// A size's share of a single-deck's workshop and insurance (estimates).
pub fn size_factor(size: BusSize) -> f64 {
    match size {
        BusSize::Midi => 0.85,
        BusSize::Solo => 1.0,
        BusSize::Articulated => 1.35,
        BusSize::Double => 1.3,
    }
}

// --- the buses (§3.4) --------------------------------------------------------------------------------

/// A new bus's price, pence (2022, without VAT). A new single-deck at £200k is what
/// its lease of £1,912-1,983 a month implies, a double-deck at £300k its £2,868-2,977; the
/// midibus is set by its lease too (about £429 a week). Articulated buses (rare in the UK) and the
/// electric buses are estimates of the market.
pub fn new_price(kind: BusKind) -> f64 {
    match (kind.size, kind.drive) {
        (BusSize::Midi, Drive::Diesel) => 18_000_000.0,
        (BusSize::Solo, Drive::Diesel) => 20_000_000.0,
        (BusSize::Articulated, Drive::Diesel) => 32_000_000.0,
        (BusSize::Double, Drive::Diesel) => 30_000_000.0,
        (BusSize::Midi, Drive::Electric) => 30_000_000.0,
        (BusSize::Solo, Drive::Electric) => 38_000_000.0,
        (BusSize::Articulated, Drive::Electric) => 60_000_000.0,
        (BusSize::Double, Drive::Electric) => 50_000_000.0,
    }
}

/// A lease: the yearly rate, the term in months and the residual (a share of the price) that
/// make the data's rates: £1,983 a month for a £200k single-deck, £2,975 for a £300k double-deck
/// (years 1-3 £447 a week, years 4-5 £385: a five-year lease).
pub const LEASE_RATE: f64 = 0.025;
pub const LEASE_MONTHS: u32 = 60;
pub const LEASE_RESIDUAL: f64 = 0.50;
/// Short-term rental of a bus a day, of its new price (an estimate: about £200 a day for a
/// single-deck; the data's long-term hire is about £1,700 a month).
pub const RENT_SHARE: f64 = 0.0010;
/// Depreciation of an owned bus: written off in a straight line to `DEPRECIATION_RESIDUAL` of
/// what was paid by the age of `DEPRECIATION_YEARS` (a new £200k bus about £14k a year, as the
/// data's), a used one over what is left of that, at least `DEPRECIATION_MIN_YEARS` (an 8-14
/// years old bus about £900 a month, as the data's).
pub const DEPRECIATION_YEARS: f64 = 13.0;
pub const DEPRECIATION_MIN_YEARS: f64 = 4.0;
pub const DEPRECIATION_RESIDUAL: f64 = 0.10;

// --- fixed costs per bus (§3.4, §3.5) ------------------------------------------------------------------

/// Insurance per single-deck a year, pence (£2,250-2,770; by size: `size_factor`).
pub const INSURANCE_PER_YEAR: f64 = 250_000.0;
/// Road tax and the annual test a year, pence (£440 + £120).
pub const TAX_PER_YEAR: f64 = 56_000.0;
/// What an accident that takes a bus off the road costs the operator, pence: the insurance's
/// excess (the insurance pays the rest; an estimate of a fleet policy's excess, by size).
pub const ACCIDENT_EXCESS: (f64, f64) = (100_000.0, 250_000.0);
/// The ticket machine a month, pence.
pub const TICKET_MACHINE_PER_MONTH: f64 = 6_000.0;
/// The central overhead per bus a year, pence (depot rent, management, admin, control, IT: 15-23
/// % of the cost). Realistic all of it, Normal half, Casual none (`ModeFactors::overheads`).
pub const OVERHEAD_PER_BUS_YEAR: f64 = 1_500_000.0;

// --- income (§2; wave 2b) ------------------------------------------------------------------------------

/// Fares, pence: an adult single (typical; £1.50-7.50 rising with distance), the return, the
/// child's and the period tickets.
pub const ADULT_SINGLE: f64 = 240.0;
pub const ADULT_SINGLE_RANGE: (f64, f64) = (150.0, 750.0);
pub const RETURN_FACTOR: f64 = 1.6;
pub const CHILD_FACTOR: f64 = 0.55;
pub const DAY_TICKET: f64 = 420.0;
pub const WEEK_TICKET: f64 = 2_400.0;
pub const FOUR_WEEK_TICKET: (f64, f64) = (8_800.0, 11_000.0);
pub const SCHOOL_TERM_PASS: f64 = 18_500.0;
/// An urban fare cap, pence (adult, child; an optional setting per line or area).
pub const FARE_CAP: (f64, f64) = (290.0, 150.0);
/// Fares round to 5p steps in the currency shown.
pub const FARE_STEP: f64 = 5.0;
/// The average yield per paying boarding, and per boarding of any kind, pence.
pub const YIELD_PER_PAYING: f64 = 306.0;
pub const YIELD_PER_BOARDING: f64 = 90.0;
/// Who boards (shares): concession passes, school passes, network passes, paying adults, staff
/// and contract shuttles; the rest is other. Concession use varies by route, 10-66 %.
pub const MIX_CONCESSION: f64 = 0.31;
pub const MIX_SCHOOL: f64 = 0.17;
pub const MIX_NETWORK: f64 = 0.11;
pub const MIX_PAYING: f64 = 0.16;
pub const MIX_SHUTTLE: f64 = 0.09;
pub const CONCESSION_RANGE: (f64, f64) = (0.10, 0.66);
/// Concession reimbursement per boarding: about 60 % (53-72 %) of the adult fare, £1.95 typical
/// (the data: £1.97 a ride, 63 % of its mean adult single).
pub const CONCESSION_SHARE: f64 = 0.63;
pub const CONCESSION_PER_BOARDING: f64 = 195.0;
/// BSOG, pence per live (in-service) mile: £0.059; none on dead mileage.
pub const BSOG_PER_LIVE_MILE: f64 = 5.9;
/// Advertising on a bus a month, pence.
pub const ADVERTISING_PER_BUS_MONTH: f64 = 4_700.0;
/// Kilometres in a mile.
pub const KM_PER_MILE: f64 = 1.609_344;
/// An adult single by distance, pence: a base and a rate per kilometre of the line's trip, within
/// `ADULT_SINGLE_RANGE` - £2.40 for a 10 km town trip, some £3.65 at 24 km, £4.65 at 35 km (the
/// data's routes: town £2.20-2.30, rural £3.75-4.15, long rural £5.50-6.40).
pub const SINGLE_BASE: f64 = 150.0;
pub const SINGLE_PER_KM: f64 = 9.0;
/// The typical trip of a line, km (what `ADULT_SINGLE` is for), when a line has run none yet.
pub const TYPICAL_TRIP_KM: f64 = 10.0;
/// The mean adult single the data's yield goes with (£3.13): the yield per paying boarding is
/// `YIELD_PER_PAYING / MEAN_SINGLE` of a line's single (returns, day and period tickets
/// averaged in).
pub const MEAN_SINGLE: f64 = 313.0;
/// A multi-operator (network) pass's boarding, pence: what the operator's share of the pass pool
/// comes to a ride (an estimate; the data's pool paid £0.42, a third-party card £2.10).
pub const NETWORK_PER_BOARDING: f64 = 100.0;
/// A school term (days) and the rides a pupil takes a school day: the term pass's income spread
/// over its rides; and the share of the pupils' rides on passes the operator sells (the rest is
/// paid inside a school contract) - together about £1.00 a pupil's ride, as the data's £1.02.
pub const SCHOOL_TERM_DAYS: f64 = 61.0;
pub const SCHOOL_RIDES_A_DAY: f64 = 2.0;
pub const SCHOOL_PASS_PAID: f64 = 0.66;

/// A pupil's ride on a school pass, pence (`SCHOOL_TERM_PASS` spread over the term's rides).
pub fn school_pass_per_ride() -> f64 {
    SCHOOL_TERM_PASS / (SCHOOL_TERM_DAYS * SCHOOL_RIDES_A_DAY) * SCHOOL_PASS_PAID
}

// --- contracts and tenders (§2.5, §4; wave 2a) ------------------------------------------------------------

/// Typical contract prices, pence: a school contract a coach and school day, a Sunday line a day,
/// a staff shuttle a bus-day, a university shuttle a year (170 days), a dedicated zero-emission
/// shuttle with the operator's bus a day.
pub const SCHOOL_DAY: (f64, f64) = (33_300.0, 35_200.0);
pub const SUNDAY_LINE_DAY: f64 = 115_000.0;
pub const STAFF_SHUTTLE_DAY: f64 = 22_500.0;
pub const UNIVERSITY_YEAR: f64 = 10_500_000.0;
pub const ZERO_EMISSION_SHUTTLE_DAY: f64 = 78_000.0;
/// A tender's payment per timetabled hour on top of the fares, pence: median and range.
pub const TENDER_PER_HOUR: f64 = 2_400.0;
pub const TENDER_PER_HOUR_RANGE: (f64, f64) = (900.0, 5_900.0);
/// Operating days a year: a school year, an office year.
pub const SCHOOL_YEAR_DAYS: (u32, u32) = (170, 182);
pub const OFFICE_YEAR_DAYS: u32 = 252;
/// Contract terms in years: a local bus contract and its extensions, a school contract, an
/// authority's shorter terms (and its yearly extensions). An extension only when rated green.
pub const TERM_LOCAL: (u32, u32) = (3, 5);
pub const TERM_SCHOOL: (u32, u32) = (3, 2);
pub const TERM_SHORT: (u32, u32) = (2, 2);
/// A net-subsidy contract's fares may rise at most this much a year; a school contract pays 11
/// months.
pub const NET_FARE_RISE: f64 = 0.15;
pub const SCHOOL_PAID_MONTHS: u32 = 11;
/// Tender scoring: price and quality points (lowest bid ÷ this bid × 70; quality 30 × (1 − 2 %
/// per average net default point)).
pub const PRICE_POINTS: f64 = 70.0;
pub const QUALITY_POINTS: f64 = 30.0;
pub const QUALITY_PER_DEFAULT_POINT: f64 = 0.02;
/// A real operator's margin on its cost price, and the share of a bid that is fixed per bus-year.
pub const BID_MARGIN: f64 = 0.05;
pub const BID_FIXED_SHARE: f64 = 0.70;
/// Leaving a contract early costs a month's average payment (of the last three months).
pub const EARLY_END_MONTHS: f64 = 1.0;
/// A contract the authority ends for default: the operator pays the authority's extra cost of
/// a replacement (the data gives no figure: an estimate of a month's payment, as leaving early).
pub const DEFAULT_END_MONTHS: f64 = 1.0;
/// The authority's published cap over its own estimate of the work's cost price: a bid above it
/// is not considered (the data's caps are some 15-25 % over a typical operator's price).
pub const TENDER_CAP: f64 = 1.20;
/// The share of a timetabled hour a bus runs in service (the rest positioning and dead runs):
/// for the live miles BSOG is paid on (an estimate).
pub const LIVE_SHARE: f64 = 0.92;
/// A vehicle a bus-year in a bid, pence: between owning (a new single-deck's depreciation, about
/// £14k) and leasing (about £23.8k); an estimate of an operator's mix.
pub const BID_VEHICLE_YEAR: f64 = 1_900_000.0;
/// A net-subsidy bid takes the fares and concession money it expects off its cost: tendered
/// routes are the thin ones, carrying about half the passengers per bus-hour of a day's average
/// (an estimate; the data: farebox on tendered routes covers a small part of the cost).
pub const TENDERED_DEMAND: f64 = 0.5;
/// Of the map's tenders, the share let as gross cost (the rest net subsidy), and of the gross
/// cost ones the share paid by the week (the rest by the operating day). Estimates.
pub const GROSS_SHARE: f64 = 0.6;
pub const WEEKLY_SHARE: f64 = 0.3;
/// An operator new to the authority has no record: it is scored as one with this many average
/// net default points (92 % of the quality points: a fair record, below a clean one).
pub const NEW_OPERATOR_POINTS: f64 = 4.0;
/// Default points and quality points count over this many days (a rolling 12 months).
pub const POINTS_DAYS: i64 = 365;

// --- performance and deductions (§5; wave 2a) ----------------------------------------------------------------

/// On time: from a minute early to 5:59 late; the target share of journeys.
pub const ON_TIME_EARLY: i32 = 1;
pub const ON_TIME_LATE: i32 = 5;
pub const ON_TIME_TARGET: f64 = 0.95;
/// A journey of a contract not paid: 5 minutes or more early, 10 or more late (and one not run,
/// or on the wrong route).
pub const UNPAID_EARLY: i32 = 5;
pub const UNPAID_LATE: i32 = 10;
/// Default points: slightly off time (2-4 early, 5-10 late), far off, a missed journey or wrong
/// vehicle, an unroadworthy bus or unlicensed driver, a late report (a month).
pub const POINTS_SLIGHT: f64 = 3.0;
pub const POINTS_FAR: f64 = 5.0;
pub const POINTS_MISSED: f64 = 10.0;
pub const POINTS_SERIOUS: f64 = 20.0;
pub const POINTS_LATE_REPORT: f64 = 5.0;
/// Points over 12 months: orange from 16, red (may be terminated) from 26; quality points
/// offset them (2-10).
pub const POINTS_ORANGE: f64 = 16.0;
pub const POINTS_RED: f64 = 26.0;
/// The journeys of a contract the authority's monitoring observes a week (an inspector on the
/// street, an audit of the real-time data; an estimate): the points for journeys off time or not
/// run are found on those, not on every journey run - the thresholds above are a year's observed
/// defaults. At it a contract run at the 95 % target earns some 8 points a year, one at a typical
/// operator's 80 % some 30 before its quality points (24-45 a year) offset them, one at 60 % red.
pub const MONITORED_A_WEEK: f64 = 1.0;
pub const QUALITY_CREDIT: (f64, f64) = (2.0, 10.0);
/// The quality points of a month (within `QUALITY_CREDIT`): a clean roadside inspection, a month
/// without a complaint about the line, `QUALITY_ON_TIME_SHARE` of its journeys on time. The
/// authority inspects a contract's buses in a month with `INSPECTION_CHANCE`; a bus below
/// `UNROADWORTHY` condition fails it (and is a serious default whenever it runs a journey).
pub const QUALITY_INSPECTION: f64 = 5.0;
pub const QUALITY_NO_COMPLAINTS: f64 = 2.0;
pub const QUALITY_ON_TIME: f64 = 4.0;
pub const QUALITY_ON_TIME_SHARE: f64 = 0.98;
pub const INSPECTION_CHANCE: f64 = 0.35;
pub const UNROADWORTHY: f64 = 30.0;
/// Closures without notice are paid at half for a week; bad weather in full with best efforts,
/// half when the bus could not run.
pub const CLOSURE_PAID: f64 = 0.5;
pub const CLOSURE_DAYS: u32 = 7;
pub const WEATHER_PAID: (f64, f64) = (1.0, 0.5);

// --- demand (§6; wave 2b) --------------------------------------------------------------------------------------

/// Passengers per bus-hour by time band: AM peak (07-09), interpeak, PM and school peak, early
/// morning, evening; Saturday and Sunday averages.
pub const PAX_AM_PEAK: f64 = 16.0;
pub const PAX_INTERPEAK: f64 = 9.0;
pub const PAX_PM_PEAK: f64 = 12.5;
pub const PAX_EARLY: f64 = 5.0;
pub const PAX_EVENING: f64 = 5.0;
pub const PAX_SATURDAY: f64 = 8.5;
pub const PAX_SUNDAY: f64 = 4.0;
/// A whole day's average per bus-hour (9.4-11.4; busy urban lines about 28, or 1.2 a km).
pub const PAX_DAY: (f64, f64) = (9.4, 11.4);
pub const PAX_URBAN: f64 = 28.0;
/// Saturday and Sunday against a weekday (Sunday: an assumption; the data's operator hardly ran).
pub const SATURDAY_SHARE: f64 = 0.55;
pub const SUNDAY_SHARE: f64 = 0.35;
/// Passengers per journey by line type: park & ride, trunk, urban all-day, evening and Sunday,
/// works shuttle, school run.
pub const PER_JOURNEY_PARK_RIDE: (f64, f64) = (3.0, 4.0);
pub const PER_JOURNEY_TRUNK: (f64, f64) = (13.0, 16.0);
pub const PER_JOURNEY_URBAN: (f64, f64) = (11.0, 43.0);
pub const PER_JOURNEY_EVENING: f64 = 7.0;
pub const PER_JOURNEY_SHUTTLE: f64 = 15.0;
pub const PER_JOURNEY_SCHOOL: (f64, f64) = (20.0, 55.0);
/// The data's passengers per bus-hour count the hours a bus is out on its tour, its stand
/// between trips included: an hour of running is this much of a bus-hour (a trip of 50 minutes
/// an hour apart).
pub const BUS_HOUR_PER_RUNNING_HOUR: f64 = 1.2;
/// Passengers per bus-hour through a weekday by the hour (0-23): the bands above, the 07:00 hour
/// at 2.2 times an interpeak hour (the data: 2.18-2.25), the school-out hour almost as big; the
/// night (0-4) an assumption - the data's operator ran 05:00 to 20:00.
pub const WEEKDAY_HOURS: [f64; 24] = [3.0, 3.0, 3.0, 3.0, 3.0, 5.0, 5.0, 19.8, 12.2, 9.5, 9.5, 9.0, 9.0, 8.5, 8.5, 17.5, 12.0, 8.0, 6.5, 5.5, 4.5, 4.5, 4.5, 4.5];
/// A Saturday's (about `PAX_SATURDAY` through the day, its interpeak the data's 9.6); a Sunday's
/// and a public holiday's are a Saturday's at `PAX_SUNDAY / PAX_SATURDAY` (an assumption, see
/// `SUNDAY_SHARE`).
pub const SATURDAY_HOURS: [f64; 24] = [2.0, 2.0, 2.0, 2.0, 2.0, 3.0, 4.0, 5.0, 7.0, 9.5, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 9.5, 9.0, 7.0, 6.0, 4.5, 4.5, 4.0, 4.0];
/// The passengers of a works shuttle's and a school run's trip (the middle of the data's
/// ranges), and off their peaks the share of it that still rides.
pub const PER_TRIP_SHUTTLE: f64 = PER_JOURNEY_SHUTTLE;
pub const PER_TRIP_SCHOOL: f64 = 37.0;
pub const OFF_PEAK_SHUTTLE: f64 = 0.35;
/// A line late often loses passengers, slowly (an estimate, kept gentle): late by the UK standard
/// (more than `ON_TIME_LATE` minutes), the share of its trips late it is forgiven - the target is
/// 95 % on time, operators run some 75-85 % - what each share above that costs, the most it can
/// cost, and the days over which the passengers remember (they come back as slowly when it runs
/// on time again).
pub const LATE_FORGIVEN: f64 = 0.25;
pub const LATE_LOSS: f64 = 0.40;
pub const LATE_MOST: f64 = 0.25;
pub const LATE_MEMORY_DAYS: f64 = 30.0;

// --- the company's money, the depot and its own lines (wave 2a's re-basing) ------------------------------------

/// What the founders put in, pence, by mode (Realistic, Normal, Casual): a small UK bus company's
/// equity - a few used buses' worth and its first months' wages (an estimate); Normal and Casual
/// twice and four times as much.
pub const START_CAPITAL: (f64, f64, f64) = (50_000_000.0, 100_000_000.0, 200_000_000.0);
/// What a bank lends without looking at the fleet, pence (an SME's overdraft and term loan;
/// estimates in step with the capital).
pub const CREDIT_BASE: (f64, f64, f64) = (20_000_000.0, 50_000_000.0, 125_000_000.0);
/// A loan's yearly interest: a UK SME's asset finance at the base rate plus some 4 % (2022-24:
/// 7-8 %); Normal a cheaper bank, Casual none.
pub const LOAN_RATE: (f64, f64, f64) = (0.075, 0.05, 0.0);
/// Yearly inflation of the costs (UK CPI-like over a run of years, 2.5-3 %) and the indexation
/// of the contracts' payments (an index a little behind the costs, as the tender sheets' yearly
/// uplifts are). Casual none.
pub const INFLATION: (f64, f64, f64) = (0.03, 0.025, 0.0);
pub const INDEXATION: (f64, f64, f64) = (0.025, 0.025, 0.0);
/// The chance a bus breaks down on a day of service at its best condition (`clock`): the rate the
/// workshop's data was calibrated on (wave 1; about one road call per 150 bus-days). The modes'
/// `breakdown_factor` scales it (Realistic 1.0).
pub const BREAKDOWN_CHANCE: f64 = 0.0068;
/// Grants on a new bus, by mode: a share of every new bus (Casual only) and of a zero-emission
/// bus's extra cost over a diesel one (England's ZEBRA scheme funds up to 75 % of it, when a bid
/// wins: Realistic counts on two thirds of that).
pub const GRANT_ALL: (f64, f64, f64) = (0.0, 0.0, 0.30);
pub const GRANT_ELECTRIC: (f64, f64, f64) = (0.5, 0.75, 0.80);
/// A dealer's used bus over its book value (the used market's), and what a dealer pays for one
/// of ours of its book value (UK dealers' trade margins of some 10-20 %; estimates).
pub const USED_MARKUP: (f64, f64, f64) = (1.10, 1.05, 0.95);
pub const SALE_FACTOR: (f64, f64, f64) = (0.80, 0.85, 0.95);
/// Building at the depot, pence (2022): a level of hardstanding for 12 buses, a workshop bay with
/// its pit, a bus wash (a gantry, then a drive-through), the fuel tank with its pump (then one more
/// pump), a depot charger, the offices (three levels). Estimates of UK build costs.
pub const DEPOT_PARKING: f64 = 6_000_000.0;
pub const DEPOT_WORKSHOP: f64 = 20_000_000.0;
pub const DEPOT_WASH: (f64, f64) = (9_000_000.0, 28_000_000.0);
pub const DEPOT_FUEL: (f64, f64) = (12_000_000.0, 3_000_000.0);
pub const DEPOT_CHARGER: f64 = 5_000_000.0;
pub const DEPOT_OFFICES: (f64, f64, f64) = (15_000_000.0, 22_000_000.0, 28_000_000.0);
/// A month's upkeep of a building of `cost`: about 0.5 % of it (the workshop more, 1 %).
pub const DEPOT_UPKEEP: f64 = 0.005;
/// An own line's one-off costs, pence (2022): registering it with the Traffic Commissioner (the
/// fee and the paperwork) and a route survey per km, a stop's flag and timetable case, a live
/// display, the timetables of a day group, joining the area's multi-operator ticket scheme and
/// each line in it after, its launch's marketing (a part and per expected passenger), a
/// registration varied, and a stop's departure charge a month. Estimates.
pub const LINE_REGISTRATION: f64 = 60_000.0;
pub const LINE_PER_KM: f64 = 2_500.0;
pub const LINE_STOP_INFO: f64 = 15_000.0;
pub const LINE_LIVE_DISPLAY: f64 = 800_000.0;
pub const LINE_TIMETABLES: f64 = 80_000.0;
pub const LINE_SCHEME_ENTRY: f64 = 300_000.0;
pub const LINE_SCHEME_LINE: f64 = 50_000.0;
pub const LINE_MARKETING: f64 = 150_000.0;
pub const LINE_MARKETING_PER_PASSENGER: f64 = 150.0;
pub const LINE_CHANGE: f64 = 40_000.0;
pub const LINE_STOP_MONTH: f64 = 2_000.0;

/// A by-mode value of the tables above (Realistic, Normal, Casual).
pub fn by_mode<T: Copy>(m: GameMode, v: (T, T, T)) -> T {
    match m {
        GameMode::Realistic => v.0,
        GameMode::Normal => v.1,
        GameMode::Casual => v.2,
    }
}

// --- the yardstick (§1) -------------------------------------------------------------------------------------------

/// The data's operator, what Realistic is calibrated on: 30 buses, a direct cost of about
/// £300k a month (pence), and the shares of it (drivers, fuel).
pub const YARDSTICK_BUSES: usize = 30;
pub const YARDSTICK_DIRECT_MONTH: f64 = 30_000_000.0;
/// Its turnover a month (pence: about £312.5k), and the share of it its contracts and tenders
/// bring (55-60 %; the rest is the farebox, the concession reimbursement, school passes, BSOG).
pub const YARDSTICK_TURNOVER_MONTH: f64 = 31_250_000.0;
pub const YARDSTICK_CONTRACTS: (f64, f64) = (0.55, 0.60);
pub const YARDSTICK_DRIVERS: (f64, f64) = (0.45, 0.48);
pub const YARDSTICK_FUEL: (f64, f64) = (0.24, 0.26);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::sandbox::CAREER;

    #[test]
    fn pounds_go_in_as_euro_cents_and_come_back() {
        // £200,000 is shown as £200,000 again by a GBP company
        let cents = gbp_cents(20_000_000.0);
        assert!((Currency::Gbp.units(cents as f64) - 200_000.0).abs() < 0.01);
        assert!(cents > 20_000_000);
    }

    #[test]
    fn realistic_in_the_uk_is_the_data() {
        let uk = country("gb");
        for item in [Item::Wages, Item::Diesel, Item::Power, Item::Workshop, Item::Vehicles, Item::Insurance, Item::Overheads, Item::Fares, Item::Concessions, Item::Contracts, Item::Grants] {
            assert!((factor_at(GameMode::Realistic, uk, &CAREER, item) - 1.0).abs() < 1e-12, "{item:?}");
        }
        // an unknown country is the reference
        assert_eq!(country("xx").key, "intl");
        assert_eq!(country("DE").key, "de");
    }

    #[test]
    fn normal_and_casual_follow_the_factors() {
        let uk = country("gb");
        let f = |m, item| factor_at(m, uk, &CAREER, item);
        assert!((f(GameMode::Normal, Item::Wages) - 0.90).abs() < 1e-12);
        assert!((f(GameMode::Casual, Item::Diesel) - 0.75).abs() < 1e-12);
        assert_eq!((f(GameMode::Normal, Item::Overheads), f(GameMode::Casual, Item::Overheads)), (0.5, 0.0));
        assert!((f(GameMode::Normal, Item::Fares) - 1.15).abs() < 1e-12 && (f(GameMode::Casual, Item::Contracts) - 1.35).abs() < 1e-12);
        assert!(!mode(GameMode::Casual).overtime && mode(GameMode::Normal).overtime);
    }

    #[test]
    fn a_country_scales_its_own_prices() {
        let de = country("de");
        let f = |item| factor_at(GameMode::Realistic, de, &CAREER, item);
        assert!((f(Item::Wages) - de.wages).abs() < 1e-12 && (f(Item::Diesel) - de.diesel).abs() < 1e-12 && (f(Item::Fares) - de.fares).abs() < 1e-12);
        // every country of the names has its factors, all of them sensible
        for p in crate::company::names::pools() {
            let x = country(&p.key);
            assert_eq!(x.key, p.key, "{}", p.key);
            for v in [x.wages, x.diesel, x.power, x.fares, x.general] {
                assert!((0.1..=2.0).contains(&v), "{}", x.key);
            }
        }
        // the sandbox on top
        let mut s = CAREER;
        s.running = 2.0;
        assert!((factor_at(GameMode::Realistic, de, &s, Item::Diesel) - 2.0 * de.diesel).abs() < 1e-12);
    }

    #[test]
    fn the_data_adds_up() {
        // a driver's hour all-in: £14.81 at the base rate
        assert!((DRIVER_BASE_HOUR * all_in() - 1481.0).abs() < 1.0);
        // a single-deck's fuel a mile about £0.54, its workshop £0.22
        let mile = KM_PER_MILE * consumption(BusKind { size: BusSize::Solo, drive: Drive::Diesel }) / 100.0 * DIESEL_PER_LITRE;
        assert!((52.0..56.0).contains(&mile), "{mile}");
        assert!((WORKSHOP_PER_KM * KM_PER_MILE - 22.0).abs() < 0.5);
    }
}

#[cfg(test)]
#[path = "econ_uk_tests.rs"]
pub(crate) mod reference;

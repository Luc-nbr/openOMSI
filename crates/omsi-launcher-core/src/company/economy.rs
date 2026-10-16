//! The company's economy: what buses, people, energy and the depot cost, what the lines earn,
//! and how loans and leases are paid off - for three game modes.
//!
//! **The costs are the UK model** (`econ_uk`): Realistic is real UK operator data at factor 1.0,
//! Normal and Casual follow from the mode factors, a company's country scales the price levels,
//! and a sandbox's knobs sit on top. Here are the functions that turn it into euro cents for a
//! bus, a kilometre, a month: bus prices and their age (`new_price`, `age_factor`,
//! `book_value`), leases and loans (`lease_monthly`, `annuity`), fuel and energy
//! (`energy_per_km`), the workshop (`maintenance_per_km`), the fixed costs of a bus a month
//! (`bus_month`: lease or depreciation, insurance and tax, the ticket machine, the overhead) and
//! a driver's market wage (`market_wage`).
//!
//! **The income is the UK model too** (`income`: passengers per bus-hour by the time of day and
//! the kind of line, fares by ticket, concession reimbursement, BSOG). The bus contracts - their
//! payments, deductions and default points - are `bus_contracts`': a line under one has no
//! payment per km. What is left of a payment per km is an own weekend line's tourism grant
//! (`tourism_per_km`).
//!
//! The age curve of a bus's value: the used market pays for age - some 18 % is lost in the first
//! year, then 8-10 % of the new price a year, less later, to a 10 % residual at 15-18 years
//! (`age_factor`), corrected for the kilometres (60,000 a year is usual for a city bus) and the
//! condition (Luc, 2026-10-15). What a bus costs the company's result is its depreciation
//! (`depreciation_per_month`), as an operator's books write it off.

use super::econ_uk::{self, Item, Level};
use super::model::{BookingKind, BusKind, Cents, Company, Drive, GameMode, Tenure, Vehicle};

/// The numbers of one game mode. The costs come from the UK model (`econ_uk`): a bus's price
/// The numbers of one game mode. The costs come from the UK model (`econ_uk`): a bus's price
/// and lease here, the running and fixed costs in the functions below; the income is `income`'s,
/// the contracts' `bus_contracts`'.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub start_capital: Cents,
    /// New bus prices against the UK's (`econ_uk::new_price`): the mode's cost factor; for a
    /// company (`rules_for`) its country's and its sandbox's too (`econ_uk::Item::Vehicles`).
    pub price_factor: f64,
    /// What a dealer asks for a used bus over its book value.
    pub used_markup: f64,
    /// What a dealer pays for one of ours, of its book value.
    pub sale_factor: f64,
    /// Grant: a share of a new bus's price, and a share of an electric bus's extra cost over
    /// a diesel one.
    pub grant_all: f64,
    pub grant_electric: f64,
    /// Loans: yearly interest, term, and how much the bank lends (a fixed part plus a share
    /// of the fleet's book value).
    pub loan_rate: f64,
    pub loan_months: u32,
    pub credit_base: Cents,
    pub credit_share: f64,
    /// Leasing: yearly interest, term and residual value.
    pub lease_rate: f64,
    pub lease_months: u32,
    pub lease_residual: f64,
    /// Short-term rental per day, of the new price.
    pub rent_share: f64,
    /// Breakdowns and wear against the UK's (`econ_uk::BREAKDOWN_CHANCE`; Realistic 1.0).
    pub breakdown_factor: f64,
    /// Yearly rise of costs, and of the contract's payment.
    pub inflation: f64,
    pub indexation: f64,
    /// A day's chance of falling ill (for someone of average reliability).
    pub sickness: f64,
    /// Days of notice when dismissing, and severance in months' wages per year employed.
    pub notice_days: i64,
    pub severance_months_per_year: f64,
    /// Applicants and used buses on offer each week.
    pub applicants: usize,
    pub used_offers: usize,
}

/// From this many whole minutes behind a trip counts as late (`lateness::Timed::minutes`): the UK
/// standard's window (`econ_uk::ON_TIME_LATE`: up to 5 min 59 s behind is on time) - for the
/// reputation, the day's punctuality, the late running's pages and the service control. (OMSI's
/// own 180 s are its displays'; a bus contract's default points have their own tiers,
/// `bus_contracts::time_points`.)
pub const LATE_FROM: i32 = econ_uk::ON_TIME_LATE + 1;
/// From this many minutes a late trip weighs as a trip lost: in the reputation, the service
/// control's choices and the report's last tier. (What it costs on a bus contract is
/// `bus_contracts::unpaid_at`'s; a commercial line pays in passengers and reputation only.)
pub const LATE_FULL: i32 = 30;

/// The minutes late up to which a late trip is in the first tier: the contracts' slightly late
/// (`econ_uk::UNPAID_LATE`, 10).
pub const LATE_SLIGHT: i32 = econ_uk::UNPAID_LATE;

/// The tier of a trip `minutes` late: 0 on time, 1 up to `LATE_SLIGHT` minutes, 2 under
/// `LATE_FULL`, 3 from it (counted as a trip lost).
pub fn late_tier(minutes: i32) -> usize {
    match minutes {
        m if m < LATE_FROM => 0,
        m if m <= LATE_SLIGHT => 1,
        m if m < LATE_FULL => 2,
        _ => 3,
    }
}

/// The rules of a game mode, all of them the UK model's (`econ_uk`): a bus's price, its lease and
/// short-term rental at the mode's cost factor; the founders' capital, the bank, inflation and
/// the contracts' indexation, grants, the used market, breakdowns by the mode's tables
/// (`econ_uk::by_mode`). The running and fixed costs and the wages are in the functions below.
pub fn rules(d: GameMode) -> Rules {
    let pick = |v| econ_uk::by_mode(d, v);
    // (sums of money to the hundred euros, as prices are)
    let pounds = |pence: f64| round_to(econ_uk::gbp(pence), 100_00);
    let (applicants, sickness, notice_days, severance, loan_months, credit_share) = match d {
        GameMode::Casual => (8, 0.006, 14, 0.0, 96, 0.8),
        GameMode::Normal => (6, 0.011, 28, 0.0, 72, 0.6),
        GameMode::Realistic => (4, 0.016, 28, 0.5, 60, 0.5),
    };
    Rules {
        start_capital: pounds(pick(econ_uk::START_CAPITAL)),
        price_factor: econ_uk::mode(d).costs,
        used_markup: pick(econ_uk::USED_MARKUP),
        sale_factor: pick(econ_uk::SALE_FACTOR),
        grant_all: pick(econ_uk::GRANT_ALL),
        grant_electric: pick(econ_uk::GRANT_ELECTRIC),
        loan_rate: pick(econ_uk::LOAN_RATE),
        loan_months,
        credit_base: pounds(pick(econ_uk::CREDIT_BASE)),
        credit_share,
        lease_rate: econ_uk::LEASE_RATE,
        lease_months: econ_uk::LEASE_MONTHS,
        lease_residual: econ_uk::LEASE_RESIDUAL,
        rent_share: econ_uk::RENT_SHARE,
        // (Realistic the UK's rate; Normal and Casual fewer, as their mode tables had them)
        breakdown_factor: pick((1.0, 0.6, 0.18)),
        inflation: pick(econ_uk::INFLATION),
        indexation: pick(econ_uk::INDEXATION),
        sickness,
        notice_days,
        severance_months_per_year: severance,
        applicants,
        used_offers: applicants,
    }
}

/// The rules the company plays by: its game mode's, as its sandbox sets them (`sandbox::rules`;
/// a Career company's are its game mode's), its buses priced in its country
/// (`econ_uk::Item::Vehicles`: the mode, the country and the sandbox's prices).
pub fn rules_for(c: &Company) -> Rules {
    let mut r = super::sandbox::rules(c, rules(c.game_mode));
    r.price_factor = econ_uk::factor(c, Item::Vehicles);
    r
}

/// The speed a tendered timetabled hour runs at (km/h): what turns the data's tender payment per
/// hour into one per km (the tourism grant's, `tourism_per_km`).
pub const TENDER_KMH: f64 = 25.0;

/// A new bus's reference price: the UK's (`econ_uk::new_price`), in euro cents, at founding.
pub fn reference_price(kind: BusKind) -> Cents {
    econ_uk::gbp_cents(econ_uk::new_price(kind))
}

fn round_to(c: f64, step: Cents) -> Cents {
    ((c / step as f64).round() as Cents) * step
}

/// What a new bus costs today (to the hundred euros).
pub fn new_price(kind: BusKind, r: &Rules, price_index: f64) -> Cents {
    round_to(reference_price(kind) as f64 * r.price_factor * price_index, 100_00)
}

/// The grant on a new bus of `price`: a share of the price, and of an electric bus's extra
/// cost over a diesel bus of its size.
pub fn grant(kind: BusKind, price: Cents, r: &Rules, price_index: f64) -> Cents {
    let mut g = price as f64 * r.grant_all;
    if kind.drive == Drive::Electric {
        let diesel = new_price(BusKind { size: kind.size, drive: Drive::Diesel }, r, price_index);
        g += (price - diesel).max(0) as f64 * r.grant_electric;
    }
    round_to(g.min(price as f64), 100_00)
}

/// Kilometres a city bus usually runs in a year (for the used market and depreciation).
pub const KM_PER_YEAR: f64 = 60_000.0;
/// What is left of a bus's new price at the end, however old it is.
pub const RESIDUAL: f64 = 0.10;

/// What is left of a new price at an age (years, the left of the curve's points): the used
/// market's prices of city buses - a fifth lost in the first year, then 8 points of the new
/// price a year, fewer as it ages, to the residual at 18 years.
const AGE_CURVE: [(f64, f64); 14] = [
    (0.0, 1.0),
    (1.0, 0.82),
    (2.0, 0.74),
    (3.0, 0.66),
    (4.0, 0.58),
    (5.0, 0.51),
    (6.0, 0.45),
    (7.0, 0.39),
    (8.0, 0.34),
    (10.0, 0.26),
    (12.0, 0.19),
    (14.0, 0.14),
    (16.0, 0.115),
    (18.0, RESIDUAL),
];

/// What is left of a bus's new price at `age_years` (`AGE_CURVE`, straight between its points).
pub fn age_factor(age_years: f64) -> f64 {
    let a = age_years.max(0.0);
    for w in AGE_CURVE.windows(2) {
        let ((a0, f0), (a1, f1)) = (w[0], w[1]);
        if a <= a1 {
            return f0 + (f1 - f0) * (a - a0) / (a1 - a0);
        }
    }
    RESIDUAL
}

/// The factors of a book value: of the age, of the kilometres above the usual, of the
/// condition (`book_value` multiplies the new price by them).
pub fn book_factors(age_years: f64, km: f64, condition: f64) -> (f64, f64, f64) {
    let usual = KM_PER_YEAR * age_years.max(0.0);
    let km = (1.0 - (km - usual) / 1_500_000.0).clamp(0.85, 1.0);
    let cond = 0.75 + 0.25 * (condition.clamp(0.0, 100.0) / 100.0);
    (age_factor(age_years), km, cond)
}

/// A bus's book value: its new price by its age (`age_factor`), corrected for kilometres above
/// the usual and for its condition. (Fewer kilometres than usual add nothing: a bus gained
/// value with them, a new one more than its price.)
pub fn book_value(new_value: Cents, age_years: f64, km: f64, condition: f64) -> Cents {
    let (age, km, cond) = book_factors(age_years, km, condition);
    (new_value as f64 * age * km * cond).round() as Cents
}

/// What a bus loses the moment it is the company's: a new one when it is registered, a used one
/// the dealer's margin.
pub const FIRST_LOSS_NEW: f64 = 0.12;
pub const FIRST_LOSS_USED: f64 = 0.04;

/// The most a bus the company bought is worth: what it paid for it (`Tenure::Owned`'s `paid`, a
/// grant taken off) less the first loss, and as it ages less as `age_factor` has it from the
/// age it was bought at (the first loss is the first part of what it loses: a new bus is worth
/// 88 % once registered and 82 % a year on). `age_bought`: the bus's age when it was bought
/// (under half a year: new).
pub fn bought_value(paid: Cents, age_bought: f64, age_now: f64) -> Cents {
    let first = if age_bought < 0.5 { FIRST_LOSS_NEW } else { FIRST_LOSS_USED };
    let since = age_factor(age_now) / age_factor(age_bought);
    (paid.max(0) as f64 * (1.0 - first).min(since)).round() as Cents
}

/// What a dealer pays for a bus of that book value.
pub fn sale_price(value: Cents, r: &Rules) -> Cents {
    round_to(value as f64 * r.sale_factor, 100_00)
}

/// The monthly rate that pays `principal` off in `months` at a yearly `rate` (an annuity;
/// without interest the principal in equal parts).
pub fn annuity(principal: Cents, rate: f64, months: u32) -> Cents {
    if months == 0 {
        return principal;
    }
    let i = rate / 12.0;
    let p = principal as f64;
    if i <= 0.0 {
        return (p / months as f64).ceil() as Cents;
    }
    (p * i / (1.0 - (1.0 + i).powi(-(months as i32)))).round() as Cents
}

/// A lease's monthly rate: the price less the residual's present value, paid off as an
/// annuity (the residual is what the bus is worth when it goes back).
pub fn lease_monthly(price: Cents, r: &Rules) -> Cents {
    let i = r.lease_rate / 12.0;
    let n = r.lease_months as i32;
    let residual = price as f64 * r.lease_residual;
    let present = if i > 0.0 { residual / (1.0 + i).powi(n) } else { residual };
    annuity((price as f64 - present).round() as Cents, r.lease_rate, r.lease_months)
}

/// What renting a bus costs a day (driver not included): a share of its price (the mode's, the
/// country's and the sandbox's as a new one's), to the ten euros.
pub fn rent_per_day(kind: BusKind, r: &Rules, price_index: f64) -> Cents {
    round_to(reference_price(kind) as f64 * r.rent_share * r.price_factor * price_index, 10_00)
}

/// Fuel (l/100 km) or electricity (kWh/100 km) a bus of this kind uses (`econ_uk::consumption`).
pub fn consumption(kind: BusKind) -> f64 {
    econ_uk::consumption(kind)
}

/// Diesel a litre, net, today (cents, fractional): the UK's at the company's level.
pub fn diesel_per_litre(lv: &Level) -> f64 {
    econ_uk::gbp(econ_uk::DIESEL_PER_LITRE) * lv.diesel * lv.price_index
}

/// Electricity a kWh, net, today (cents, fractional).
pub fn power_per_kwh(lv: &Level) -> f64 {
    econ_uk::gbp(econ_uk::POWER_PER_KWH) * lv.power * lv.price_index
}

/// Fuel or electricity per km (cents, fractional).
pub fn energy_per_km(kind: BusKind, lv: &Level) -> f64 {
    let unit = if kind.drive == Drive::Electric { power_per_kwh(lv) } else { diesel_per_litre(lv) };
    consumption(kind) / 100.0 * unit
}

/// The workshop's routine per km (cents, fractional): servicing, tyres and parts - the UK's
/// £0.22 a mile for a diesel single-deck of the fleet's average age less what the breakdowns
/// take (`econ_uk::WORKSHOP_ROUTINE`, `breakdown_repair`), a quarter less electric, more for a
/// bigger bus, 5 % more or less for every year older or younger.
pub fn maintenance_per_km(kind: BusKind, age_years: f64, lv: &Level) -> f64 {
    let drive = if kind.drive == Drive::Electric { econ_uk::WORKSHOP_ELECTRIC } else { 1.0 };
    let age = (1.0 + econ_uk::WORKSHOP_PER_YEAR * (age_years.max(0.0) - econ_uk::WORKSHOP_AGE)).clamp(0.75, 1.6);
    econ_uk::gbp(econ_uk::WORKSHOP_PER_KM * econ_uk::WORKSHOP_ROUTINE) * drive * econ_uk::size_factor(kind.size) * age * lv.workshop * lv.price_index
}

/// A breakdown's repair (whole euros, in cents): `draw` (0 to 1) between the least and the most
/// a breakdown costs (`econ_uk::BREAKDOWN_REPAIR`), by the bus's size, at the company's level.
pub fn breakdown_repair(draw: f64, kind: BusKind, lv: &Level) -> Cents {
    let (low, high) = econ_uk::BREAKDOWN_REPAIR;
    let pence = low + (high - low) * draw.clamp(0.0, 1.0);
    ((econ_uk::gbp(pence) * econ_uk::size_factor(kind.size) * lv.workshop * lv.price_index / 100.0).round() as Cents) * 100
}

/// Insurance, road tax and the annual test of a bus a month.
pub fn insurance_per_month(kind: BusKind, lv: &Level) -> Cents {
    let year = econ_uk::INSURANCE_PER_YEAR * econ_uk::size_factor(kind.size) + econ_uk::TAX_PER_YEAR;
    (econ_uk::gbp(year) / 12.0 * lv.insurance * lv.price_index).round() as Cents
}

/// What an accident that took a bus off the road costs the company (whole euros, in cents):
/// `draw` (0 to 1) within the insurance's excess (`econ_uk::ACCIDENT_EXCESS`), by size.
pub fn accident_damage(draw: f64, kind: BusKind, lv: &Level) -> Cents {
    let (low, high) = econ_uk::ACCIDENT_EXCESS;
    let pence = low + (high - low) * draw.clamp(0.0, 1.0);
    ((econ_uk::gbp(pence) * econ_uk::size_factor(kind.size) * lv.insurance * lv.price_index / 100.0).round() as Cents) * 100
}

/// A bus's ticket machine a month.
pub fn ticket_machine_per_month(lv: &Level) -> Cents {
    (econ_uk::gbp(econ_uk::TICKET_MACHINE_PER_MONTH) * lv.insurance * lv.price_index).round() as Cents
}

/// The central overhead of a bus a month (depot rent, management, admin, control, IT): all of it
/// Realistic, half Normal, none Casual.
pub fn overhead_per_month(lv: &Level) -> Cents {
    (econ_uk::gbp(econ_uk::OVERHEAD_PER_BUS_YEAR) / 12.0 * lv.overheads * lv.price_index).round() as Cents
}

/// An owned bus's depreciation a month: what was paid (`Tenure::Owned`'s `paid`), less a tenth
/// left at the end, written off in a straight line until the bus is `DEPRECIATION_YEARS` old (a
/// used one over what is left of that, `DEPRECIATION_MIN_YEARS` at least); nothing once that is
/// done. `built` and `acquired` are the bus's dates, `today` the month's.
pub fn depreciation_per_month(paid: Cents, built: &str, acquired: &str, today: &str) -> Cents {
    let at_purchase = super::dates::years_between(built, acquired).max(0.0);
    let years = (econ_uk::DEPRECIATION_YEARS - at_purchase).max(econ_uk::DEPRECIATION_MIN_YEARS);
    if super::dates::years_between(acquired, today) >= years {
        return 0;
    }
    (paid.max(0) as f64 * (1.0 - econ_uk::DEPRECIATION_RESIDUAL) / (years * 12.0)).round() as Cents
}

/// What a bus of the fleet costs in a whole month besides its kilometres, by booking kind: a
/// leased one its lease, an owned one its depreciation (not cash: `BookingKind::is_cash`); both
/// their insurance and tax, the ticket machine and the overhead. A rented one nothing (its daily
/// rent is all). `today`: a day of the month (for the depreciation's age).
pub fn bus_month(v: &Vehicle, lv: &Level, today: &str) -> Vec<(BookingKind, Cents)> {
    let fixed = |out: &mut Vec<(BookingKind, Cents)>| {
        out.push((BookingKind::Insurance, insurance_per_month(v.kind, lv)));
        out.push((BookingKind::Overheads, ticket_machine_per_month(lv) + overhead_per_month(lv)));
    };
    let mut out = Vec::new();
    match &v.tenure {
        Tenure::Leased { monthly, .. } => {
            out.push((BookingKind::Lease, *monthly));
            fixed(&mut out);
        }
        Tenure::Owned { paid, .. } => {
            out.push((BookingKind::Depreciation, depreciation_per_month(*paid, &v.built, &v.acquired, today)));
            fixed(&mut out);
        }
        Tenure::Rented { .. } => {}
    }
    out.retain(|x| x.1 != 0);
    out
}

/// The driver wage the market pays for `experience` (0-100 points), monthly gross for the
/// contracted hours (`econ_uk::CONTRACT_WEEK_HOURS`): the UK's £12.24 an hour (2022) from 94 %
/// for a beginner to 108 % for the most experienced, at the company's level, to the ten euros.
pub fn market_wage(experience: f64, lv: &Level) -> Cents {
    let x = experience.clamp(0.0, 100.0) / 100.0;
    let hour = econ_uk::DRIVER_BASE_HOUR * (econ_uk::DRIVER_LOW + (econ_uk::DRIVER_HIGH - econ_uk::DRIVER_LOW) * x);
    round_to(econ_uk::gbp(hour) * econ_uk::contract_month_hours() * lv.wages * lv.price_index, 10_00)
}

/// The employer's share on a wage: National Insurance and pension, holiday, sick pay and
/// allowances (`econ_uk::ON_COSTS`, `econ_uk::ALLOWANCES`).
pub const EMPLOYER_SHARE: f64 = econ_uk::ON_COSTS + econ_uk::ALLOWANCES;

/// What a gross wage costs the company.
pub fn employer_cost(wage: Cents) -> Cents {
    (wage as f64 * (1.0 + EMPLOYER_SHARE)).round() as Cents
}

/// The rate a kilometre an own weekend line's tourism grant is a share of
/// (`ownline::LEISURE_COMPENSATION`): the data's tender payment per timetabled hour at
/// `TENDER_KMH`, by the contracts' factor of the company's game (its mode, country and sandbox),
/// ±10 % round a reputation of 50, and the contract index. A line under a bus contract has no
/// payment per km - its contract pays (`bus_contracts`).
pub fn tourism_per_km(c: &Company) -> f64 {
    tourism_at(econ_uk::factor(c, Item::Contracts), c.reputation, c.contract_index)
}

/// `tourism_per_km` of a factor, a reputation (0-100) and a contract index.
pub fn tourism_at(factor: f64, reputation: f64, contract_index: f64) -> f64 {
    // (below 40 softened: `reputation::effective`)
    econ_uk::gbp(econ_uk::TENDER_PER_HOUR) / TENDER_KMH * factor * (0.9 + 0.2 * super::reputation::effective(reputation) / 100.0) * contract_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::currency::Currency;
    use crate::company::model::BusSize;

    const SOLO: BusKind = BusKind { size: BusSize::Solo, drive: Drive::Diesel };
    const E_ART: BusKind = BusKind { size: BusSize::Articulated, drive: Drive::Electric };
    const DD: BusKind = BusKind { size: BusSize::Double, drive: Drive::Diesel };

    /// Euro cents as pounds.
    fn pounds(cents: f64) -> f64 {
        Currency::Gbp.units(cents)
    }

    #[test]
    fn late_trips_have_their_tier() {
        assert_eq!([2, 5, 6, 10, 11, 29, 30].map(late_tier), [0, 0, 1, 1, 2, 2, 3]);
    }

    #[test]
    fn the_companys_money_is_on_the_uk_footing() {
        let (r, n, c) = (rules(GameMode::Realistic), rules(GameMode::Normal), rules(GameMode::Casual));
        // the founders' £500k, a bank at 7.5 %, CPI-like 3 % and contracts indexed by 2.5 %
        assert!((pounds(r.start_capital as f64) - 500_000.0).abs() < 100.0);
        assert_eq!((r.loan_rate, r.inflation, r.indexation), (0.075, 0.03, 0.025));
        assert!(n.loan_rate < r.loan_rate && c.loan_rate == 0.0 && c.inflation == 0.0);
        // Realistic's breakdowns are the UK's rate (factor 1), the others fewer
        assert_eq!(r.breakdown_factor, 1.0);
        assert!(n.breakdown_factor < 1.0 && c.breakdown_factor < n.breakdown_factor);
        // a dealer's margins on used buses, and a grant on a zero-emission bus's extra cost
        assert!(r.used_markup > 1.0 && r.sale_factor < 1.0 && n.sale_factor > r.sale_factor);
        assert!(r.grant_electric > 0.0 && r.grant_all == 0.0 && c.grant_all > 0.0);
    }

    #[test]
    fn prices_are_the_uks_by_the_game_mode() {
        let r = rules(GameMode::Realistic);
        // a new single-deck £200k, a double-deck £300k (to the hundred euros)
        assert!((pounds(new_price(SOLO, &r, 1.0) as f64) - 200_000.0).abs() < 100.0);
        assert!((pounds(new_price(DD, &r, 1.0) as f64) - 300_000.0).abs() < 100.0);
        assert!(new_price(E_ART, &r, 1.0) > new_price(BusKind { size: BusSize::Articulated, drive: Drive::Diesel }, &r, 1.0));
        // Normal and Casual by the mode's cost factor
        let normal = new_price(SOLO, &rules(GameMode::Normal), 1.0) as f64;
        let casual = new_price(SOLO, &rules(GameMode::Casual), 1.0) as f64;
        assert!((normal / new_price(SOLO, &r, 1.0) as f64 - 0.90).abs() < 0.001);
        assert!((casual / new_price(SOLO, &r, 1.0) as f64 - 0.75).abs() < 0.001);
        // inflation raises them
        assert!(new_price(SOLO, &r, 1.1) > new_price(SOLO, &r, 1.0));
        // a grant on an electric bus's extra cost on Normal, on everything on Casual
        let n = rules(GameMode::Normal);
        assert_eq!(grant(SOLO, new_price(SOLO, &n, 1.0), &n, 1.0), 0);
        assert!(grant(E_ART, new_price(E_ART, &n, 1.0), &n, 1.0) > 0);
        assert!(grant(SOLO, casual as Cents, &rules(GameMode::Casual), 1.0) > 0);
        // (Realistic counts on part of the zero-emission grant only)
        assert!(grant(E_ART, new_price(E_ART, &r, 1.0), &r, 1.0) < grant(E_ART, new_price(E_ART, &n, 1.0), &n, 1.0) && grant(SOLO, new_price(SOLO, &r, 1.0), &r, 1.0) == 0);
        // a day's rental of a single-deck some £200 (to the ten euros)
        assert!((pounds(rent_per_day(SOLO, &r, 1.0) as f64) - 200.0).abs() < 10.0);
        // the start capital buys a few buses
        assert!(n.start_capital > 3 * new_price(SOLO, &n, 1.0));
        assert!(rules(GameMode::Casual).start_capital > n.start_capital && n.start_capital > r.start_capital);
    }

    #[test]
    fn loans_and_leases_are_annuities() {
        // 100,000 € over 12 months at 0 %: equal parts
        assert_eq!(annuity(100_000_00, 0.0, 12), 833_334);
        // 280,000 € over 72 months at 4.5 %: about 4,445 € a month
        let m = annuity(280_000_00, 0.045, 72);
        assert!((4_440_00..4_450_00).contains(&m), "{m}");
        // the rates add up to more than the principal, by the interest
        assert!(m * 72 > 280_000_00 && m * 72 < 330_000_00);
        // a new single-deck leases at the data's £1,912-1,983 a month, a double-deck at £2,868-2,977
        let r = rules(GameMode::Realistic);
        let solo = pounds(lease_monthly(new_price(SOLO, &r, 1.0), &r) as f64);
        assert!((1_900.0..2_000.0).contains(&solo), "{solo}");
        let dd = pounds(lease_monthly(new_price(DD, &r, 1.0), &r) as f64);
        assert!((2_850.0..3_000.0).contains(&dd), "{dd}");
        // Normal's is cheaper by the factor
        let n = rules(GameMode::Normal);
        assert!(lease_monthly(new_price(SOLO, &n, 1.0), &n) < lease_monthly(new_price(SOLO, &r, 1.0), &r));
    }

    #[test]
    fn a_bus_loses_its_value_with_its_age() {
        assert_eq!(book_value(280_000_00, 0.0, 0.0, 100.0), 280_000_00);
        // the first year some 18 %, then 8-10 % of the new price a year
        let one = age_factor(1.0);
        assert!((0.80..=0.85).contains(&one), "{one}");
        for y in 1..8 {
            let drop = age_factor(y as f64) - age_factor(y as f64 + 1.0);
            assert!((0.045..=0.10).contains(&drop), "{y}: {drop}");
        }
        // ever down, to a tenth at 15-18 years and never under it
        for y in 0..25 {
            assert!(age_factor(y as f64 + 1.0) <= age_factor(y as f64));
        }
        assert!((0.10..=0.13).contains(&age_factor(15.0)));
        assert_eq!(age_factor(18.0), RESIDUAL);
        assert_eq!(age_factor(30.0), RESIDUAL);
        // a 15-year-old bus costs a small part of a new one
        assert!(book_value(280_000_00, 15.0, 900_000.0, 100.0) < 280_000_00 / 7);
        let six = book_value(280_000_00, 6.0, 360_000.0, 100.0);
        assert_eq!(six, 126_000_00);
        assert_eq!(book_value(280_000_00, 20.0, 1_200_000.0, 100.0), 28_000_00);
        // more kilometres and a worse condition cost value; fewer add nothing
        assert!(book_value(280_000_00, 6.0, 600_000.0, 100.0) < six);
        assert!(book_value(280_000_00, 6.0, 360_000.0, 40.0) < six);
        assert_eq!(book_value(280_000_00, 6.0, 0.0, 100.0), six);
        assert_eq!(sale_price(six, &rules(GameMode::Normal)), 107_100_00);
        // what the company paid: 12 % off once registered, 18 % after a year, then the curve
        assert_eq!(bought_value(280_000_00, 0.0, 0.0), 246_400_00);
        assert_eq!(bought_value(280_000_00, 0.0, 1.0), 229_600_00);
        assert_eq!(bought_value(100_000_00, 8.0, 10.0), (100_000_00 as f64 * 0.26 / 0.34).round() as Cents);
    }

    #[test]
    fn an_owned_bus_is_written_off_as_the_books_do() {
        // a new £200k bus: about £14k a year, until it is 13
        let paid = econ_uk::gbp_cents(20_000_000.0);
        let new = pounds(depreciation_per_month(paid, "2024-01-01", "2024-01-01", "2024-06-30") as f64) * 12.0;
        assert!((13_000.0..15_000.0).contains(&new), "{new}");
        assert_eq!(depreciation_per_month(paid, "2024-01-01", "2024-01-01", "2037-02-01"), 0);
        // one bought used at ten years for £40k: about £750-1,000 a month over four years
        let used = pounds(depreciation_per_month(econ_uk::gbp_cents(4_000_000.0), "2014-01-01", "2024-01-01", "2025-06-30") as f64);
        assert!((700.0..1_000.0).contains(&used), "{used}");
        assert_eq!(depreciation_per_month(econ_uk::gbp_cents(4_000_000.0), "2014-01-01", "2024-01-01", "2028-02-01"), 0);
    }

    #[test]
    fn running_costs_are_the_uk_operators() {
        let uk = Level::uk(1.0);
        // 29 l/100 km at £1.15: about 33p a km, £0.54 a mile
        let fuel = pounds(energy_per_km(SOLO, &uk)) * econ_uk::KM_PER_MILE;
        assert!((0.52..0.56).contains(&fuel), "{fuel}");
        // an electric single-deck far less: 1.2 kWh at £0.10, about £0.19 a mile
        let power = pounds(energy_per_km(BusKind { size: BusSize::Solo, drive: Drive::Electric }, &uk)) * econ_uk::KM_PER_MILE;
        assert!((0.17..0.21).contains(&power), "{power}");
        // the workshop's routine of £0.22 a mile at six years (the breakdowns' repairs apart),
        // more when older, a quarter less electric
        let shop = pounds(maintenance_per_km(SOLO, 6.0, &uk)) * econ_uk::KM_PER_MILE / econ_uk::WORKSHOP_ROUTINE;
        assert!((0.215..0.225).contains(&shop), "{shop}");
        let (low, high) = (pounds(breakdown_repair(0.0, SOLO, &uk) as f64), pounds(breakdown_repair(1.0, SOLO, &uk) as f64));
        assert!((low - 150.0).abs() < 1.0 && (high - 900.0).abs() < 1.0, "{low} {high}");
        assert!(maintenance_per_km(SOLO, 12.0, &uk) > maintenance_per_km(SOLO, 6.0, &uk));
        assert!((maintenance_per_km(BusKind { size: BusSize::Solo, drive: Drive::Electric }, 6.0, &uk) / maintenance_per_km(SOLO, 6.0, &uk) - 0.75).abs() < 1e-9);
        // insurance and tax about £255 a month, the ticket machine £60, the overhead £1,250
        assert!((pounds(insurance_per_month(SOLO, &uk) as f64) - (2_500.0 + 560.0) / 12.0).abs() < 1.0);
        assert!((pounds(ticket_machine_per_month(&uk) as f64) - 60.0).abs() < 0.01);
        assert!((pounds(overhead_per_month(&uk) as f64) - 1_250.0).abs() < 0.01);
        // a driver of some experience £12.24 an hour for 39 hours a week
        let wage = pounds(market_wage(45.0, &uk) as f64);
        assert!((wage / econ_uk::contract_month_hours() - 12.24).abs() < 0.1, "{wage}");
        assert_eq!(employer_cost(1_000_00), 1_210_00);
        // Normal at 0.9 of all of it, its overhead half; Casual's none
        let career = super::super::sandbox::CAREER;
        let normal = Level::at(GameMode::Normal, econ_uk::country("gb"), &career, 1.0);
        assert!((energy_per_km(SOLO, &normal) / energy_per_km(SOLO, &uk) - 0.9).abs() < 1e-9);
        assert_eq!(overhead_per_month(&normal), overhead_per_month(&uk) / 2);
        assert_eq!(overhead_per_month(&Level::at(GameMode::Casual, econ_uk::country("gb"), &career, 1.0)), 0);
        // the tourism grant's rate per km: the tender's hour at 25 km/h, by the mode's factor
        let base = econ_uk::gbp(econ_uk::TENDER_PER_HOUR) / TENDER_KMH;
        assert!((tourism_at(1.15, 50.0, 1.0) - base * 1.15).abs() < 1e-9);
        assert!(tourism_at(1.0, 100.0, 1.0) > tourism_at(1.0, 0.0, 1.0));
    }
}

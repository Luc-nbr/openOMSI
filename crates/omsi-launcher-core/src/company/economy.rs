//! The company's economy: what buses, people, energy and the depot cost, what the lines earn,
//! and how loans and leases are paid off - for three difficulties.
//!
//! "Realistic" is calibrated on German urban bus operations of the mid-2020s. The figures are
//! rounded reference values, not one operator's books; where they come from:
//!
//! - **Bus prices** (net, without VAT): tenders and press reports of German operators 2022-2024
//!   put a 12 m diesel bus (Citaro, Lion's City, Urbino) at €260-300k, an 18 m articulated one
//!   at €380-430k, a 12 m battery bus at €520-600k and an 18 m battery bus at €750-850k.
//!   Midibuses (~10 m) about €200k (diesel), double-deckers €450k. We take €280k, €400k,
//!   €550k, €800k, €200k and €450k.
//! - **Depreciation**: German tax tables (AfA) write a bus off over 9 years; operators run them
//!   12-15 years, and the used market pays for age (Luc, 2026-10-15): some 18 % is lost in the
//!   first year, then 8-10 % of the new price a year, less later, to a 10 % residual at 15-18
//!   years (`age_factor`) - corrected for the kilometres (60,000 a year is usual for a city
//!   bus) and the condition.
//! - **Fuel**: a 12 m diesel city bus uses 38-45 l/100 km, an articulated one 50-60, a midibus
//!   25-30; diesel costs operators about €1.45/l net. A battery bus uses 1.0-1.4 kWh/km with
//!   heating (articulated 1.5-1.9); depot charging at about €0.25/kWh net.
//! - **Maintenance**: VDV cost comparisons put workshop, tyres and parts at €0.25-0.40 per km
//!   for diesel buses and about a third less for battery buses, rising with age.
//! - **Insurance**: liability and comprehensive cover for a city bus €3,000-6,000 a year.
//! - **Depot**: yard, hall, power and cleaning - a fixed part and a part per parking space.
//! - **Wages**: the German regional tariffs (TV-N, private bus tariffs) pay a driver about
//!   €3,300 gross a month in 2024; the employer adds ~21 % social security.
//! - **Revenue**: about €1.10 fare revenue per passenger (season tickets and the
//!   Deutschlandticket bring the average far below a single fare), and the public authority
//!   pays the rest of the cost per timetable kilometre under the public service contract:
//!   farebox recovery in German cities is 40-50 %. Passengers per bus-km about 2 on average,
//!   more at the peaks.
//! - **Penalties**: contracts (Bonus/Malus) charge per trip dropped, and a share of that for a
//!   trip late by its minutes (`late_share`: Luc's tiers, 2026-10-15).
//!
//! "Easy" pays generously (more compensation and passengers, grants on new buses, cheaper
//! buses, fewer breakdowns, interest-free loans); "Hard" pays tightly, prices rise with
//! inflation while the contract does not keep up, loans cost interest and buses break down
//! more often.

use super::model::{BusKind, BusSize, Cents, Company, GameMode, Drive};

/// The numbers of one difficulty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub start_capital: Cents,
    /// New bus prices against the reference.
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
    /// Revenue: per passenger, passengers per bus-km, and the compensation per km.
    pub fare: Cents,
    pub passengers_per_km: f64,
    pub compensation_per_km: Cents,
    /// Penalties: per trip dropped, per km dropped (a trip's value; a late one pays its share of
    /// that, `late_share`).
    pub drop_per_trip: Cents,
    pub drop_per_km: Cents,
    /// Breakdowns and wear against the reference.
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

/// From this many whole minutes behind a trip counts as late (`lateness::Timed::minutes`; as
/// OMSI's 180 s: up to 2 min 59 s behind is on time).
pub const LATE_FROM: i32 = 3;
/// From this many minutes a late trip costs its whole value, as a trip dropped does.
pub const LATE_FULL: i32 = 30;

/// A trip's value under the contract: what dropping it costs (a late one pays a share of it).
pub fn trip_value(r: &Rules, km: f64) -> Cents {
    r.drop_per_trip + (km * r.drop_per_km as f64).round() as Cents
}

/// The share of its value a trip `minutes` late pays (Luc, 2026-10-15): nothing under
/// `LATE_FROM`, 10 % up to five minutes, then rising linearly to the whole at `LATE_FULL`
/// minutes and over (whole minutes: 10 % at 5, 28 % at 10, 64 % at 20, 100 % at 30).
pub fn late_share(minutes: i32) -> f64 {
    match minutes {
        m if m < LATE_FROM => 0.0,
        m if m <= 5 => 0.10,
        m if m >= LATE_FULL => 1.0,
        m => 0.10 + 0.90 * (m - 5) as f64 / (LATE_FULL - 5) as f64,
    }
}

/// The tier of a trip `minutes` late: 0 on time, 1 up to five minutes, 2 under `LATE_FULL`,
/// 3 from it (counted as a trip lost).
pub fn late_tier(minutes: i32) -> usize {
    match minutes {
        m if m < LATE_FROM => 0,
        m if m <= 5 => 1,
        m if m < LATE_FULL => 2,
        _ => 3,
    }
}

/// The rules of a difficulty.
pub fn rules(d: GameMode) -> Rules {
    match d {
        GameMode::Casual => Rules {
            start_capital: 2_500_000_00,
            price_factor: 0.85,
            used_markup: 0.95,
            sale_factor: 0.95,
            grant_all: 0.30,
            grant_electric: 0.80,
            loan_rate: 0.0,
            loan_months: 96,
            credit_base: 1_500_000_00,
            credit_share: 0.8,
            lease_rate: 0.02,
            lease_months: 72,
            lease_residual: 0.25,
            rent_share: 0.0010,
            fare: 1_30,
            passengers_per_km: 2.3,
            compensation_per_km: 1_80,
            drop_per_trip: 0,
            drop_per_km: 0,
            breakdown_factor: 0.3,
            inflation: 0.0,
            indexation: 0.0,
            sickness: 0.006,
            notice_days: 14,
            severance_months_per_year: 0.0,
            applicants: 8,
            used_offers: 8,
        },
        GameMode::Normal => Rules {
            start_capital: 1_200_000_00,
            price_factor: 1.0,
            used_markup: 1.10,
            sale_factor: 0.85,
            grant_all: 0.0,
            grant_electric: 0.40,
            loan_rate: 0.045,
            loan_months: 72,
            credit_base: 600_000_00,
            credit_share: 0.6,
            lease_rate: 0.045,
            lease_months: 72,
            lease_residual: 0.25,
            rent_share: 0.0013,
            fare: 1_10,
            passengers_per_km: 2.0,
            compensation_per_km: 1_20,
            drop_per_trip: 80_00,
            drop_per_km: 2_00,
            breakdown_factor: 1.0,
            inflation: 0.025,
            indexation: 0.025,
            sickness: 0.011,
            notice_days: 28,
            severance_months_per_year: 0.0,
            applicants: 6,
            used_offers: 6,
        },
        GameMode::Realistic => Rules {
            start_capital: 600_000_00,
            price_factor: 1.08,
            used_markup: 1.20,
            sale_factor: 0.75,
            grant_all: 0.0,
            grant_electric: 0.0,
            loan_rate: 0.085,
            loan_months: 60,
            credit_base: 250_000_00,
            credit_share: 0.5,
            lease_rate: 0.075,
            lease_months: 72,
            lease_residual: 0.20,
            rent_share: 0.0016,
            fare: 1_05,
            passengers_per_km: 1.9,
            compensation_per_km: 1_00,
            drop_per_trip: 150_00,
            drop_per_km: 3_00,
            breakdown_factor: 1.7,
            inflation: 0.045,
            indexation: 0.015,
            sickness: 0.016,
            notice_days: 28,
            severance_months_per_year: 0.5,
            applicants: 4,
            used_offers: 4,
        },
    }
}

/// The rules the company plays by: its game mode's, as its sandbox sets them (`sandbox::rules`;
/// a Career company's are its game mode's).
pub fn rules_for(c: &Company) -> Rules {
    super::sandbox::rules(c, rules(c.game_mode))
}

/// A new bus's reference price (Realistic, at founding).
pub fn reference_price(kind: BusKind) -> Cents {
    match (kind.size, kind.drive) {
        (BusSize::Midi, Drive::Diesel) => 200_000_00,
        (BusSize::Midi, Drive::Electric) => 400_000_00,
        (BusSize::Solo, Drive::Diesel) => 280_000_00,
        (BusSize::Solo, Drive::Electric) => 550_000_00,
        (BusSize::Articulated, Drive::Diesel) => 400_000_00,
        (BusSize::Articulated, Drive::Electric) => 800_000_00,
        (BusSize::Double, Drive::Diesel) => 450_000_00,
        (BusSize::Double, Drive::Electric) => 700_000_00,
    }
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

/// What renting a bus costs a day (driver not included).
pub fn rent_per_day(kind: BusKind, r: &Rules, price_index: f64) -> Cents {
    round_to(reference_price(kind) as f64 * r.rent_share * price_index, 10_00)
}

/// Fuel (l/100 km) or electricity (kWh/100 km) a bus of this kind uses.
pub fn consumption(kind: BusKind) -> f64 {
    match (kind.size, kind.drive) {
        (BusSize::Midi, Drive::Diesel) => 28.0,
        (BusSize::Solo, Drive::Diesel) => 40.0,
        (BusSize::Articulated, Drive::Diesel) => 55.0,
        (BusSize::Double, Drive::Diesel) => 50.0,
        (BusSize::Midi, Drive::Electric) => 75.0,
        (BusSize::Solo, Drive::Electric) => 120.0,
        (BusSize::Articulated, Drive::Electric) => 170.0,
        (BusSize::Double, Drive::Electric) => 150.0,
    }
}

/// Diesel per litre and electricity per kWh, net, at founding (cents).
pub const DIESEL_PER_L: f64 = 145.0;
pub const POWER_PER_KWH: f64 = 25.0;

/// Energy per km (cents, fractional).
pub fn energy_per_km(kind: BusKind, price_index: f64) -> f64 {
    let unit = if kind.drive == Drive::Electric { POWER_PER_KWH } else { DIESEL_PER_L };
    consumption(kind) / 100.0 * unit * price_index
}

fn size_factor(size: BusSize) -> f64 {
    match size {
        BusSize::Midi => 0.8,
        BusSize::Solo => 1.0,
        BusSize::Articulated => 1.35,
        BusSize::Double => 1.3,
    }
}

/// Maintenance per km (cents, fractional): €0.30 for a diesel solo bus, a third less
/// electric, more for a bigger bus, 5 % more for every year of age.
pub fn maintenance_per_km(kind: BusKind, age_years: f64, price_index: f64) -> f64 {
    let base = if kind.drive == Drive::Electric { 21.0 } else { 30.0 };
    base * size_factor(kind.size) * (1.0 + 0.05 * age_years.max(0.0)) * price_index
}

/// Insurance a month.
pub fn insurance_per_month(kind: BusKind, price_index: f64) -> Cents {
    (4_000_00 as f64 * size_factor(kind.size) / 12.0 * price_index).round() as Cents
}

/// The depot a month: a fixed part and a part per bus.
pub fn depot_per_month(buses: usize, price_index: f64) -> Cents {
    ((2_500_00 + 150_00 * buses as Cents) as f64 * price_index).round() as Cents
}

/// The driver wage the market pays for `experience` (0-100 points), monthly gross:
/// €2,900 for a beginner to €3,800 for the most experienced, €3,300 for a driver of some
/// years.
pub fn market_wage(experience: f64, price_index: f64) -> Cents {
    round_to((2_900_00 as f64 + 900_00 as f64 * (experience.clamp(0.0, 100.0) / 100.0)) * price_index, 10_00)
}

/// The employer's share on a gross wage (social security, accident insurance, levies).
pub const EMPLOYER_SHARE: f64 = 0.21;

/// What a gross wage costs the company.
pub fn employer_cost(wage: Cents) -> Cents {
    (wage as f64 * (1.0 + EMPLOYER_SHARE)).round() as Cents
}

/// The passengers' demand through the day against the average: the peaks in the morning
/// and late afternoon, little at night.
pub fn demand_at(minute: i32) -> f64 {
    match minute.rem_euclid(1440) / 60 {
        0..=4 => 0.25,
        5 => 0.6,
        6..=8 => 1.6,
        9..=11 => 0.9,
        12..=14 => 1.05,
        15..=17 => 1.5,
        18..=19 => 0.8,
        20..=22 => 0.5,
        _ => 0.35,
    }
}

/// Passengers a trip of `km` leaving at `minute` carries (before chance and reputation).
pub fn passengers_for(km: f64, minute: i32, r: &Rules) -> f64 {
    km.max(0.0) * r.passengers_per_km * demand_at(minute)
}

/// The authority's payment per km at a reputation (0-100): ±10 % around 50, as Omsi-Hub's.
pub fn compensation_per_km(r: &Rules, reputation: f64, contract_index: f64) -> f64 {
    // (below 40 softened: `reputation::effective`)
    r.compensation_per_km as f64 * (0.9 + 0.2 * super::reputation::effective(reputation) / 100.0) * contract_index
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLO: BusKind = BusKind { size: BusSize::Solo, drive: Drive::Diesel };
    const E_ART: BusKind = BusKind { size: BusSize::Articulated, drive: Drive::Electric };

    #[test]
    fn late_trips_pay_their_tier() {
        // (Luc: 10 % up to five minutes, the whole trip from thirty)
        assert_eq!(late_share(2), 0.0);
        assert_eq!(late_share(4), 0.10);
        assert_eq!(late_share(5), 0.10);
        assert!((late_share(10) - 0.28).abs() < 1e-9);
        assert!(late_share(29) < 1.0 && late_share(29) > late_share(28));
        assert_eq!((late_share(30), late_share(75)), (1.0, 1.0));
        assert_eq!([2, 4, 6, 29, 30].map(late_tier), [0, 1, 2, 2, 3]);
        // (a trip's value is what dropping it costs; Casual's is nothing)
        assert_eq!(trip_value(&rules(GameMode::Normal), 15.0), 80_00 + 30_00);
        assert_eq!(trip_value(&rules(GameMode::Casual), 15.0), 0);
    }

    #[test]
    fn prices_follow_the_difficulty() {
        let r = rules(GameMode::Normal);
        assert_eq!(new_price(SOLO, &r, 1.0), 280_000_00);
        assert_eq!(new_price(E_ART, &r, 1.0), 800_000_00);
        assert_eq!(new_price(BusKind { size: BusSize::Solo, drive: Drive::Electric }, &r, 1.0), 550_000_00);
        assert_eq!(new_price(BusKind { size: BusSize::Articulated, drive: Drive::Diesel }, &r, 1.0), 400_000_00);
        let easy = new_price(SOLO, &rules(GameMode::Casual), 1.0);
        let hard = new_price(SOLO, &rules(GameMode::Realistic), 1.0);
        assert!(easy < 280_000_00 && hard > 280_000_00);
        // inflation raises them
        assert_eq!(new_price(SOLO, &r, 1.1), 308_000_00);
        // a grant only for the electric bus's extra cost on Realistic, on everything on Easy
        assert_eq!(grant(SOLO, 280_000_00, &r, 1.0), 0);
        assert_eq!(grant(E_ART, 800_000_00, &r, 1.0), 160_000_00);
        assert!(grant(SOLO, easy, &rules(GameMode::Casual), 1.0) > 0);
        assert_eq!(grant(E_ART, 800_000_00, &rules(GameMode::Realistic), 1.0), 0);
        // a day's rental of a solo bus some €360
        assert_eq!(rent_per_day(SOLO, &r, 1.0), 360_00);
        // the start capital buys a few buses
        assert!(r.start_capital > 3 * 280_000_00);
        assert!(rules(GameMode::Casual).start_capital > r.start_capital && r.start_capital > rules(GameMode::Realistic).start_capital);
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
        // a lease with a quarter left as residual costs less a month than the loan
        let r = rules(GameMode::Normal);
        let lease = lease_monthly(280_000_00, &r);
        assert!(lease < m && (3_400_00..3_800_00).contains(&lease), "{lease}");
        // a harder lease is dearer
        assert!(lease_monthly(280_000_00, &rules(GameMode::Realistic)) > lease);
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
    fn running_costs_are_those_of_a_city_bus() {
        // 40 l/100 km at €1.45: 58 cents a km
        assert!((energy_per_km(SOLO, 1.0) - 58.0).abs() < 1e-9);
        // battery buses use less money a km
        assert!(energy_per_km(BusKind { size: BusSize::Solo, drive: Drive::Electric }, 1.0) < 40.0);
        assert!((maintenance_per_km(SOLO, 0.0, 1.0) - 30.0).abs() < 1e-9);
        assert!(maintenance_per_km(SOLO, 10.0, 1.0) > 44.0);
        assert_eq!(insurance_per_month(SOLO, 1.0), 333_33);
        assert_eq!(depot_per_month(10, 1.0), 4_000_00);
        // a driver of some experience earns about €3,300 and costs about €4,000
        assert_eq!(market_wage(45.0, 1.0), 3_310_00);
        assert_eq!(employer_cost(3_300_00), 3_993_00);
        // the peaks bring more passengers than the night
        let r = rules(GameMode::Normal);
        assert!(passengers_for(10.0, 7 * 60, &r) > 4.0 * passengers_for(10.0, 1 * 60, &r));
        assert!((compensation_per_km(&r, 50.0, 1.0) - 120.0).abs() < 1e-9);
        assert!(compensation_per_km(&r, 100.0, 1.0) > compensation_per_km(&r, 0.0, 1.0));
    }

    #[test]
    fn a_realistic_kilometre_earns_a_little_more_than_it_costs() {
        // revenue a km on an average hour, against energy, maintenance, a driver's share
        // (about €1.60 a km with days off and holidays) and the bus (leasing ~€0.55)
        let r = rules(GameMode::Normal);
        let revenue = r.passengers_per_km * r.fare as f64 + compensation_per_km(&r, 50.0, 1.0);
        let cost = energy_per_km(SOLO, 1.0) + maintenance_per_km(SOLO, 3.0, 1.0) + 160.0 + 55.0;
        let margin = (revenue - cost) / revenue;
        assert!(margin > 0.0 && margin < 0.25, "{margin}");
        let e = rules(GameMode::Casual);
        let easy = e.passengers_per_km * e.fare as f64 + compensation_per_km(&e, 50.0, 1.0);
        let h = rules(GameMode::Realistic);
        let hard = h.passengers_per_km * h.fare as f64 + compensation_per_km(&h, 50.0, 1.0);
        assert!(easy > revenue * 1.3 && hard < revenue);
    }
}

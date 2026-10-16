//! Fares: the adult single a line asks, and how its passengers take it.
//!
//! A line's fare table (`income::table`) follows from its adult single: the return, the child's,
//! the day and the week ticket at the data's ratios, each a round price of the company's currency
//! (`currency::Currency::round_fare`: £2.40, never £2.39). The single a line asks by itself is its
//! standard - the distance fare of its trips (`income::distance_single`, £2.40 for a typical 10 km
//! trip in the UK, at the company's country and prices); an own line's fare is the company's to
//! set, from half to two and a half times it, a map line's within a band round it (±20 %, ±10 %
//! on Realistic). An optional cap (`CompanyLine::fare_cap`) caps the singles a passenger pays.
//! What a passenger brings (the ticket mix, the passes, the reimbursement) is `income`'s.
//!
//! Demand answers the price with the elasticities the studies give for town buses. Balcombe
//! et al., "The demand for public transport: a practical guide" (TRL Report 593, 2004): bus
//! fares about -0.4 in the short run, -0.56 in the medium and -1.0 in the long run; Paulley et
//! al., "The demand for public transport: the effects of fares, quality of service, income and
//! car ownership" (Transport Policy 13, 2006) the same; Litman, "Transit Price Elasticities and
//! Cross-Elasticities" (VTPI, 2004, updated 2023): -0.2 to -0.5 short run, -0.6 to -0.9 long
//! run. Taken here: `SHORT_RUN` -0.35 and `LONG_RUN` -0.7.
//!
//! The curve is semi-logarithmic - passengers = base · exp(ε · (p/p₀ − 1)) - so that its point
//! elasticity is ε at the line's standard fare p₀ and grows with the price (ε · p/p₀): a fare
//! far above it loses ever more riders, and the fares' revenue is at its highest at p₀/|ε|
//! (`best_fare`), about 1.4 times the standard. A cheaper fare (or a cap) draws riders, up to
//! `MOST` times as many (a full bus still leaves behind whom it cannot take, `ownline::carried`).
//!
//! The answer comes with a lag (a partial adjustment, as the studies model it): a change is
//! felt at once with the short-run elasticity, and the rest towards the long run comes over
//! the next weeks, `LAG_DAYS` the time constant (`settle`, every closed day). A fare more than
//! `FAIR` above the standard also costs reputation, a little every day (`reputation_cost`).

use super::econ_uk as uk;
use super::income;
use super::model::{Cents, Company, CompanyLine, GameMode};
use serde::{Deserialize, Serialize};

/// The short-run and the long-run fare elasticity of demand.
pub const SHORT_RUN: f64 = -0.35;
pub const LONG_RUN: f64 = -0.7;
/// The lag's time constant: after this many days the passengers have come about two thirds
/// of the way from the short-run answer to the long-run one.
pub const LAG_DAYS: f64 = 21.0;
/// The most a cheap fare multiplies the passengers by, and the least a dear one leaves.
pub const MOST: f64 = 1.6;
pub const LEAST: f64 = 0.15;
/// Above the standard fare by more than this share, a fare costs reputation.
pub const FAIR: f64 = 0.3;

/// How a line's passengers have taken its fare so far.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Demand {
    /// The fare the passengers last knew, as its offset from the standard (p/p₀ − 1).
    #[serde(default)]
    pub x: f64,
    /// The log of the passengers' share now (0: as many as at the standard fare).
    #[serde(default)]
    pub log: f64,
}

impl Demand {
    /// The share of the passengers the line has now (1: as at the standard fare).
    pub fn share(&self) -> f64 {
        self.log.exp().clamp(LEAST, MOST)
    }
}

/// A fare made a whole number of cents at the company's currency's step.
fn whole(c: &Company, cents: f64) -> Cents {
    income::round(c, cents).round() as Cents
}

/// The adult single of a typical trip (`econ_uk::TYPICAL_TRIP_KM`) at the company's country and
/// prices: what the pages name as the usual fare (cents).
pub fn association_fare(c: &Company) -> Cents {
    whole(c, income::distance_single(c, income::LineType::Typical, uk::TYPICAL_TRIP_KM))
}

/// A line's standard adult single: the distance fare of its trips (cents).
pub fn standard_fare(c: &Company, l: &CompanyLine) -> Cents {
    whole(c, income::distance_single(c, income::type_of(l), l.stats.trip_km))
}

/// The band a line's fare may be set in (cents): an own line's is wide, a map line's narrow.
pub fn band(c: &Company, l: &CompanyLine) -> (Cents, Cents) {
    let p0 = standard_fare(c, l) as f64;
    let (lo, hi) = if l.own {
        (0.5, 2.5)
    } else {
        match c.game_mode {
            GameMode::Realistic => (0.9, 1.1),
            _ => (0.8, 1.2),
        }
    };
    (whole(c, p0 * lo), whole(c, p0 * hi))
}

/// The adult single a line asks now.
pub fn fare_of(c: &Company, l: &CompanyLine) -> Cents {
    let (lo, hi) = band(c, l);
    l.fare.unwrap_or_else(|| standard_fare(c, l)).clamp(lo, hi)
}

/// The adult single a passenger of the line pays (its fare under its cap).
pub fn paid_of(c: &Company, l: &CompanyLine) -> f64 {
    income::table(c, l, fare_of(c, l) as f64).paid().0
}

/// A fare's offset from the line's standard (p/p₀ − 1).
pub fn offset(c: &Company, l: &CompanyLine, fare: Cents) -> f64 {
    fare as f64 / standard_fare(c, l).max(1) as f64 - 1.0
}

/// The offset the passengers feel: of the fare they pay (a cap lowers it).
pub fn felt(c: &Company, l: &CompanyLine) -> f64 {
    paid_of(c, l) / standard_fare(c, l).max(1) as f64 - 1.0
}

/// What a boarding of the line brings on average today (cents; `income::Rates`).
pub fn per_passenger(c: &Company, l: &CompanyLine) -> f64 {
    income::Rates::of(c, l, &c.date).per_boarding()
}

/// The passengers' share at a fare's offset once they have fully answered it.
pub fn long_run_share(x: f64) -> f64 {
    (LONG_RUN * x).exp().clamp(LEAST, MOST)
}

/// The passengers' share at a fare's offset the day it comes in.
pub fn short_run_share(x: f64) -> f64 {
    (SHORT_RUN * x).exp().clamp(LEAST, MOST)
}

/// The passengers answer the fare of one more day (`x` its offset today): a change at once
/// with the short-run elasticity, then a day's way towards the long run.
pub fn settle(d: &mut Demand, x: f64) {
    d.log += SHORT_RUN * (x - d.x);
    d.x = x;
    let k = 1.0 - (-1.0 / LAG_DAYS).exp();
    d.log += k * (LONG_RUN * x - d.log);
}

/// The fare (cents) that brings the most in the long run, within the line's band: the farebox
/// alone would be best at p₀/|ε|; the reimbursement follows the fare too, the passes do not.
pub fn best_fare(c: &Company, l: &CompanyLine) -> Cents {
    let (lo, hi) = band(c, l);
    let step = c.currency.round_fare(c.currency.cents(c.currency.fare_step()) as f64).max(1.0);
    let n = ((hi - lo) as f64 / step).ceil().clamp(1.0, 400.0) as i64;
    (0..=n).map(|k| whole(c, lo as f64 + (hi - lo) as f64 * k as f64 / n as f64).clamp(lo, hi)).max_by(|a, b| estimate(c, l, 1000.0, *a).1.total_cmp(&estimate(c, l, 1000.0, *b).1)).unwrap_or(lo)
}

/// The reputation a day of this fare costs (points).
pub fn reputation_cost(x: f64) -> f64 {
    // (was 0.3)
    (x - FAIR).max(0.0) * 0.15
}

/// Set a line's adult single (a round price of the company's currency, within its band). Returns
/// the fare set.
pub fn set_fare(c: &mut Company, line: &str, fare: Cents) -> Result<Cents, &'static str> {
    let Some(k) = c.lines.iter().position(|l| l.name.eq_ignore_ascii_case(line)) else { return Err("The company does not run this line.") };
    let (lo, hi) = band(c, &c.lines[k]);
    let fare = whole(c, fare as f64).clamp(lo, hi);
    // (a net-subsidy contract lets the fare rise by 15 % a year at most: `bus_contracts`)
    let fare = super::bus_contracts::fare_cap(c, line).map_or(fare, |cap| fare.min(cap));
    let p0 = standard_fare(c, &c.lines[k]);
    c.lines[k].fare = (fare != p0).then_some(fare);
    Ok(fare)
}

/// Cap a line's fares, or not (`econ_uk::FARE_CAP`).
pub fn set_cap(c: &mut Company, line: &str, on: bool) -> Result<(), &'static str> {
    let Some(l) = c.lines.iter_mut().find(|l| l.name.eq_ignore_ascii_case(line)) else { return Err("The company does not run this line.") };
    l.fare_cap = on;
    Ok(())
}

/// A day of fares for every line, as the day's close runs it: the passengers answer the
/// fares, and a fare far above the standard costs reputation.
pub fn day(c: &mut Company) {
    let xs: Vec<f64> = c.lines.iter().map(|l| felt(c, l)).collect();
    let mut cost = 0.0;
    for (l, x) in c.lines.iter_mut().zip(xs) {
        settle(&mut l.demand, x);
        cost += reputation_cost(x);
    }
    let date = c.date.clone();
    super::reputation::change(c, &date, super::reputation::Cause::Fares, -cost, 0);
}

/// What a line would bring at a fare in the long run, from its passengers a day at its standard
/// fare: (passengers a day, takings a day in cents - its fares, passes and reimbursement).
pub fn estimate(c: &Company, l: &CompanyLine, base_passengers: f64, fare: Cents) -> (f64, f64) {
    let mut at = l.clone();
    at.fare = Some(fare);
    let x = felt(c, &at);
    let pax = base_passengers * long_run_share(x);
    (pax, pax * income::Rates::of(c, &at, &c.date).per_boarding())
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;

    fn company(d: GameMode) -> Company {
        let mut c = found(&Founding { name: "Fares".into(), game_mode: d, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
        c.lines.push(CompanyLine { name: "oo_1".into(), number: "1".into(), own: true, ..Default::default() });
        c
    }

    #[test]
    fn the_curve_has_the_studies_elasticities_at_the_association_fare() {
        // (the point elasticity: d ln Q / d ln p at p₀)
        let h = 1e-4;
        let point = |f: fn(f64) -> f64| ((f(h)).ln() - (f(-h)).ln()) / ((1.0f64 + h).ln() - (1.0f64 - h).ln());
        assert!((point(long_run_share) - LONG_RUN).abs() < 1e-3, "{}", point(long_run_share));
        assert!((point(short_run_share) - SHORT_RUN).abs() < 1e-3);
        // dearer loses riders, cheaper draws them, both within their bounds
        assert!(long_run_share(0.2) < 1.0 && long_run_share(-0.2) > 1.0);
        assert_eq!(long_run_share(-5.0), MOST);
        // the fares' revenue is highest at p₀/|ε|
        let c = company(GameMode::Normal);
        let own = &c.lines[1];
        let best = best_fare(&c, own);
        let rev = |f: Cents| estimate(&c, own, 1000.0, f).1;
        assert!(rev(best) >= rev(best - 30) && rev(best) >= rev(best + 30), "{best}");
        // (dearer than the standard, as the farebox's p₀/|ε| is - the passes keep it lower)
        let p0 = standard_fare(&c, own) as f64;
        assert!(best as f64 > p0 * 1.1 && (best as f64) < p0 / 0.7 + 10.0, "{best} {p0}");
    }

    #[test]
    fn passengers_answer_at_once_in_part_and_the_rest_over_weeks() {
        let mut d = Demand::default();
        // twenty per cent dearer from today
        settle(&mut d, 0.2);
        let first = d.share();
        assert!((first.ln() - SHORT_RUN * 0.2).abs() < 0.01, "{first}");
        for _ in 1..LAG_DAYS as usize {
            settle(&mut d, 0.2);
        }
        // after the lag's time constant about two thirds of the way to the long run
        let way = (d.log - SHORT_RUN * 0.2) / ((LONG_RUN - SHORT_RUN) * 0.2);
        assert!((0.55..0.75).contains(&way), "{way}");
        for _ in 0..200 {
            settle(&mut d, 0.2);
        }
        assert!((d.share() - long_run_share(0.2)).abs() < 1e-3);
        // and back: the fare as before, the passengers come back the same way
        settle(&mut d, 0.0);
        assert!(d.share() > long_run_share(0.2) && d.share() < 1.0);
    }

    #[test]
    fn fares_are_set_within_the_band() {
        let mut c = company(GameMode::Realistic);
        let p0 = standard_fare(&c, &c.lines[0]);
        // a map line on Realistic: ±10 %, at the currency's step (5 cents)
        assert_eq!(set_fare(&mut c, "Linie5", p0 * 2), Ok((p0 as f64 * 1.1 / 5.0).round() as Cents * 5));
        assert_eq!(set_fare(&mut c, "Linie5", p0), Ok(p0));
        assert_eq!(c.lines[0].fare, None, "the association's fare is kept as none");
        // an own line: half to two and a half times
        let f = set_fare(&mut c, "oo_1", p0 * 2).unwrap();
        assert_eq!(f, p0 * 2);
        assert!(set_fare(&mut c, "nope", 100).is_err());
        // far above the association's: reputation, a little a day
        let before = c.reputation;
        day(&mut c);
        assert!(c.reputation < before);
        assert!(reputation_cost(0.2) == 0.0 && reputation_cost(1.0) > 0.0);
        assert!(c.lines[1].demand.share() < 1.0);
        // a cap: the passengers pay less, and answer it
        let before = felt(&c, &c.lines[1]);
        set_cap(&mut c, "oo_1", true).unwrap();
        assert!(felt(&c, &c.lines[1]) < before);
    }
}

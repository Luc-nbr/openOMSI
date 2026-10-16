//! The contract variations' tests: a class each (minor, moderate, major, refused), gross cost
//! against net subsidy, the modes, the minor steps added up, an offer, and the line editor's save
//! of a change agreed.

use super::*;
use crate::company::bus_contracts::{make, Period};
use crate::company::model::CompanyLine;
use crate::company::{found, Founding};
use crate::lines::{DayPattern, Direction, Leg, StopRef};

fn company(m: GameMode) -> Company {
    found(&Founding { name: "Variatie".into(), game_mode: m, date: "2024-03-04".into(), ..Default::default() }, "Luc")
}

/// An all-day line of one bus: 14 hours and 280 km a day, 20 journeys, six days a week.
fn work() -> Work {
    Work { hours: 14.0, km: 280.0, journeys: 20, buses: 1, days: 6 * 52 }
}

/// A contract of `kind` on "Linie5" for three years from today, at the authority's own reckoning
/// of its price (its rate 1).
fn with_contract(c: &mut Company, kind: Kind) -> u32 {
    let today = c.date.clone();
    let until = dates::add(&today, 3 * 365 - 1);
    let price = bc::price_of(c, &work(), kind, 3.0).round() as Cents;
    let k = make(c, "Linie5", "5", kind, Period::Week, price, &work(), &today, &until);
    let id = k.id;
    c.concessions.contracts.push(k);
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    id
}

fn contract(c: &Company) -> &Contract {
    c.concessions.contracts.iter().find(|k| k.line == "Linie5").unwrap()
}

fn journeys(n: i32) -> Ask {
    Ask::Timetable { journeys: n, span: 0, retime: 0 }
}

/// Minor: a retiming of a few minutes, or a journey of twenty less (5 %), costs the admin fee and
/// leaves the payment as it is.
#[test]
fn a_minor_change_costs_the_admin_fee() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let admin = fee_of(&c, ADMIN_FEE);
    assert!(admin > 0);
    let q = quote(&c, "Linie5", &Ask::Timetable { journeys: 0, span: 0, retime: 3 }).unwrap();
    assert_eq!(q.class, Class::Minor, "three minutes a journey: {:.0} % of the hours, but a retiming", 100.0 * q.share);
    assert!(q.share > 0.05);
    assert_eq!((q.fee, q.penalty, q.year_after), (admin, 0, q.year_before));
    let q = quote(&c, "Linie5", &journeys(-1)).unwrap();
    assert_eq!(q.class, Class::Minor);
    assert!((q.share + 0.05).abs() < 1e-9, "{}", q.share);
    // agreed: the fee paid, the terms changed, the history noted, the payment as it was
    let cash = c.cash;
    let v = agree(&mut c, "Linie5", &journeys(-1)).unwrap();
    assert_eq!((c.cash, v.class, v.fee), (cash - admin, Class::Minor, admin));
    let k = contract(&c);
    assert_eq!((k.journeys, k.variations.len(), k.year_price), (19, 1, q.year_before));
    assert!(c.ledger.iter().any(|b| b.kind == BookingKind::Concession && b.text.contains("variation")));
}

/// Moderate: 15 % fewer journeys cost the variation fee and lower the payment about pro rata; 15 %
/// more raise it - once the line runs them.
#[test]
fn a_moderate_change_moves_the_payment_pro_rata() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let cut = quote(&c, "Linie5", &journeys(-3)).unwrap();
    assert_eq!(cut.class, Class::Moderate);
    assert_eq!((cut.fee, cut.penalty, cut.points), (fee_of(&c, VARIATION_FEE), 0, 0.0));
    let fall = 1.0 - cut.year_after as f64 / cut.year_before as f64;
    assert!(fall > 0.08 && fall < 0.15, "a 15 % cut of the km and hours, the bus kept: {:.1} %", 100.0 * fall);
    assert!(!cut.waits);
    let more = quote(&c, "Linie5", &journeys(3)).unwrap();
    assert_eq!(more.class, Class::Moderate);
    assert!(more.year_after > more.year_before && more.waits);
    // the cut is paid less at once; the raise waits for the line
    let v = agree(&mut c, "Linie5", &journeys(3)).unwrap();
    assert!(v.waiting);
    let k = contract(&c);
    assert_eq!(k.year_now(c.contract_index).round() as Cents, more.year_before);
    assert_eq!(k.agreed.as_ref().and_then(|a| a.year), Some(more.year_after));
}

/// Major: 40 % fewer journeys cost the fee and a tenth of the value still to come, default points
/// on the contract and reputation.
#[test]
fn a_major_change_costs_a_penalty_points_and_reputation() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let q = quote(&c, "Linie5", &journeys(-8)).unwrap();
    assert_eq!(q.class, Class::Major);
    let left = contract(&c).year_now(c.contract_index) * 3.0;
    assert!((q.penalty as f64 - PENALTY_SHARE * left).abs() < 0.002 * left, "{} of {left}", q.penalty);
    assert_eq!(q.points, POINTS_MAJOR);
    assert!(q.refused.is_none() && q.year_after < q.year_before);
    // a terminus given up is major whatever its size
    let core = quote(&c, "Linie5", &Ask::Route { added: 0, removed: 1, km: 0.0, core: true }).unwrap();
    assert_eq!(core.class, Class::Major);
    let (cash, rep) = (c.cash, c.reputation);
    agree(&mut c, "Linie5", &journeys(-8)).unwrap();
    assert_eq!(c.cash, cash - q.fee - q.penalty);
    assert!(c.reputation < rep);
    let k = contract(&c);
    assert_eq!(k.net_points(&c.date), POINTS_MAJOR);
    assert_eq!(k.year_price, q.year_after);
    assert!(c.ledger.iter().any(|b| b.kind == BookingKind::Penalty && b.text.contains("variation")));
}

/// The authority refuses a change over its limit (Realistic: half the service), and a major one of
/// a red contract; Casual never refuses for the size.
#[test]
fn the_authority_refuses_a_change_too_large() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let q = quote(&c, "Linie5", &journeys(-12)).unwrap();
    assert!(q.refused.is_some());
    assert!(agree(&mut c, "Linie5", &journeys(-12)).is_err());
    // red: no major change, a moderate one still
    c.concessions.contracts[0].record.push(PointsDay { date: c.date.clone(), default: 30.0, ..Default::default() });
    assert!(quote(&c, "Linie5", &journeys(-8)).unwrap().refused.is_some());
    assert!(quote(&c, "Linie5", &journeys(-3)).unwrap().refused.is_none());
    let mut easy = company(GameMode::Casual);
    with_contract(&mut easy, Kind::Gross);
    let q = quote(&easy, "Linie5", &journeys(-12)).unwrap();
    assert_eq!((q.class, q.refused, q.fee), (Class::Major, None, 0));
    // nothing to change, and a line without a tendered contract
    assert!(quote(&easy, "Linie5", &journeys(0)).unwrap().nothing);
    assert!(quote(&easy, "Linie6", &journeys(-1)).is_err());
}

/// Gross cost moves the payment by the work's cost; a net subsidy by the cost less the fares and
/// BSOG the operator keeps - the same cut lowers it less.
#[test]
fn gross_cost_follows_the_cost_and_net_subsidy_the_cost_less_the_fares() {
    let mut g = company(GameMode::Realistic);
    with_contract(&mut g, Kind::Gross);
    let mut n = company(GameMode::Realistic);
    with_contract(&mut n, Kind::Net);
    let (qg, qn) = (quote(&g, "Linie5", &journeys(-4)).unwrap(), quote(&n, "Linie5", &journeys(-4)).unwrap());
    assert_eq!((qg.class, qn.class), (Class::Moderate, Class::Moderate));
    let (dg, dn) = (qg.year_before - qg.year_after, qn.year_before - qn.year_after);
    assert!(dg > 0 && dn > 0, "{dg} {dn}");
    assert!(dn < dg, "the net subsidy falls by the cost less the fares: {dn} against {dg}");
    // a bigger bus: the cost of the work by its type, the class by that share
    let q = quote(&g, "Linie5", &Ask::Vehicle { size: BusSize::Articulated }).unwrap();
    assert_eq!(q.class, Class::Moderate);
    assert!((q.share - 0.18).abs() < 1e-9 && q.year_after > q.year_before && !q.waits);
}

/// Normal and Casual are cheaper and more lenient: lower fees and penalties, wider bounds.
#[test]
fn the_easier_modes_are_cheaper() {
    let mut r = company(GameMode::Realistic);
    with_contract(&mut r, Kind::Gross);
    let mut n = company(GameMode::Normal);
    with_contract(&mut n, Kind::Gross);
    let mut e = company(GameMode::Casual);
    with_contract(&mut e, Kind::Gross);
    // 30 %: major in Realistic, moderate in Normal and Casual
    let ask = journeys(-6);
    let (qr, qn, qe) = (quote(&r, "Linie5", &ask).unwrap(), quote(&n, "Linie5", &ask).unwrap(), quote(&e, "Linie5", &ask).unwrap());
    assert_eq!((qr.class, qn.class, qe.class), (Class::Major, Class::Moderate, Class::Moderate));
    assert!(qn.fee < qr.fee && qe.fee == 0);
    // 6 % is minor in Normal
    assert_eq!(quote(&n, "Linie5", &Ask::Route { added: 0, removed: 0, km: 0.84, core: false }).unwrap().class, Class::Minor);
    let big = journeys(-9);
    assert!(quote(&n, "Linie5", &big).unwrap().penalty < quote(&r, "Linie5", &big).unwrap().penalty);
}

/// Minor changes are added up over a year: a second small cut is moderate.
#[test]
fn small_steps_add_up() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let small = Ask::Route { added: 0, removed: 0, km: -0.56, core: false };
    assert_eq!(agree(&mut c, "Linie5", &small).unwrap().class, Class::Minor);
    let q = quote(&c, "Linie5", &small).unwrap();
    assert_eq!(q.class, Class::Moderate, "{:.3} counted", q.counted);
}

/// An offer: below the authority's need it is refused (nothing paid), and the next waits a week;
/// at it the change is agreed for the sum offered.
#[test]
fn an_offer_is_taken_by_the_change_and_the_record() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let ask = journeys(-8);
    let q = quote(&c, "Linie5", &ask).unwrap();
    let need = need(&c, contract(&c), Class::Major);
    assert!((need - (0.85 - 0.1 + 0.01 * bc::scored_points(&c))).abs() < 1e-9);
    let cash = c.cash;
    let low = (q.price() as f64 * need * 0.9) as Cents;
    assert_eq!(offer(&mut c, "Linie5", &ask, low), Ok(None));
    assert_eq!(c.cash, cash);
    assert!(offer(&mut c, "Linie5", &ask, q.price()).is_err(), "one a week");
    c.date = dates::add(&c.date, OFFER_DAYS);
    let q = quote(&c, "Linie5", &ask).unwrap();
    let enough = (q.price() as f64 * need).ceil() as Cents;
    let v = offer(&mut c, "Linie5", &ask, enough).unwrap().unwrap();
    assert!(v.offer && v.fee == enough && v.penalty == 0);
    assert_eq!(c.cash, cash - enough);
    assert!(enough < q.price());
}

/// A line of `n` stops 400 m apart, out and back, a bus every `headway` minutes all day.
fn design(n: usize, headway: f32) -> LineDesign {
    let dir = |rev: bool| {
        let mut stops: Vec<StopRef> = (0..n).map(|i| StopRef { id: i as i64 + 1, name: format!("Stop {i}"), ..Default::default() }).collect();
        if rev {
            stops.reverse();
        }
        let legs = (0..n - 1).map(|_| Leg { length: 400.0, ok: true, ..Default::default() }).collect();
        let mut d = Direction { stops, legs, ..Default::default() };
        d.refresh_times();
        d
    };
    let p = |days: u16| DayPattern { days, first: 5.0 * 60.0, last: 23.0 * 60.0, headway, ..Default::default() };
    LineDesign { id: 9, number: "5".into(), name: "Linie5".into(), directions: vec![dir(false), dir(true)], days: vec![p(31), p(32), p(192)], map: Some(Default::default()), ..Default::default() }
}

/// The line editor: a save is measured and priced; the change agreed on the contracts' page is
/// covered by it (the raise paid from then), and the map's line back takes the contract back.
#[test]
fn the_line_editor_prices_a_save_and_runs_a_change_agreed() {
    let mut c = company(GameMode::Realistic);
    with_contract(&mut c, Kind::Gross);
    let (old, sparse) = (design(10, 15.0), design(10, 60.0));
    let m = (measure(&old), measure(&sparse));
    assert!((m.1.km / m.0.km - 0.25).abs() < 0.05, "{} {}", m.0.km, m.1.km);
    let q = edit_quote(&c, "Linie5", &old, &sparse).unwrap();
    assert_eq!(q.change, Change::Edit);
    assert!(q.refused.is_some(), "a quarter of the service: more than Realistic allows");
    assert!(edit_quote(&c, "Linie5", &old, &old).is_none());
    // a stop less (the bus drives by it): minor, the admin fee; a terminus less: major
    let mut fewer = old.clone();
    let d = &mut fewer.directions[0];
    d.stops.remove(4);
    d.legs[3].length += d.legs[4].length;
    d.legs.remove(4);
    d.refresh_times();
    let q = edit_quote(&c, "Linie5", &old, &fewer).unwrap();
    assert_eq!((q.class, q.removed, q.core), (Class::Minor, 1, false), "{:.3}", q.share);
    let mut cut = old.clone();
    cut.directions[0].stops.pop();
    cut.directions[0].legs.pop();
    cut.directions[0].refresh_times();
    cut.directions[1].stops.remove(0);
    cut.directions[1].legs.remove(0);
    cut.directions[1].refresh_times();
    let q = edit_quote(&c, "Linie5", &old, &cut).unwrap();
    assert!(q.core && q.class == Class::Major);
    // more journeys agreed on the page: the raise waits, the save that runs them pays it
    let denser = design(10, 12.5);
    let ratio = measure(&denser).km / measure(&old).km;
    let ask = journeys((20.0 * (ratio - 1.0)).round() as i32);
    let v = agree(&mut c, "Linie5", &ask).unwrap();
    assert!(v.waiting);
    let before = contract(&c).year_price;
    let cash = c.cash;
    let q = edit_quote(&c, "Linie5", &old, &denser).unwrap();
    assert!(q.covered && q.price() == 0 && q.year_after == v.year_after);
    assert_eq!(settle_edit(&mut c, "Linie5", &q), Ok(None));
    let k = contract(&c);
    assert!(k.agreed.is_none() && k.year_price == v.year_after && k.year_price > before);
    assert!(!k.variations.last().unwrap().waiting);
    assert_eq!(c.cash, cash);
    // back to the map's line: the work and the payment follow it, nothing paid
    restore(&mut c, "Linie5", &denser, &old);
    let k = contract(&c);
    assert_eq!(k.variations.last().unwrap().change, Change::Restore);
    assert!(k.year_price < v.year_after);
    assert_eq!(c.cash, cash);
}

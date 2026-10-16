//! The model pinned on a reference company: Realistic, in the UK, 30 buses and a typical plan -
//! as the data's operator, its direct costs about £300k a month, drivers and fuel their shares of
//! it; Normal and Casual cheaper by their factors; a German company by its country's.

use super::*;
use crate::company::currency::Currency;
use crate::company::market::{self, MarketBus, Payment};
use crate::company::model::{BookingKind, CompanyLine, Licence, Tenure};
use crate::company::network::{PlannedTrip, TourOfDay};
use crate::company::staff::{applicants_of, hire};
use crate::company::{dates, day, depot, found, Founding};

const LINE: &str = "Linie5";

fn trip(name: String, dep: i32, arr: i32, km: f64, empty: bool) -> PlannedTrip {
    PlannedTrip { name, line: "5".into(), from: "A".into(), to: "B".into(), dep, arr, km, stops: if empty { 0 } else { 12 }, empty, origin: String::new(), last: String::new(), part: None }
}

/// A tour of eight trips of 50 minutes (24 km each: the data's operator ran some 31 km an hour of
/// its buses' time) an hour apart from `from`, with a depot run (6 km, empty) before and after it:
/// one duty of some eight hours.
fn tour(no: String, from: i32) -> TourOfDay {
    let mut trips = vec![trip(format!("{no}-out"), from - 10, from, 6.0, true)];
    for k in 0..8 {
        trips.push(trip(format!("{no}-{k}"), from + 60 * k, from + 60 * k + 50, 24.0, false));
    }
    let end = from + 60 * 7 + 50;
    trips.push(trip(format!("{no}-in"), end, end + 10, 6.0, true));
    TourOfDay { line: LINE.into(), number: "5".into(), tour: no, ai_group: String::new(), trips, unplanned: false, cuts: None }
}

/// The day's tours: an early and a late tour for each bus out - 22 on a weekday, 18 on a
/// Saturday, 4 on a Sunday (the data's operator: 26-29 buses at the peak, few on Sundays).
pub(crate) fn tours_of(date: &str) -> Vec<TourOfDay> {
    let buses = match dates::parse(date).map(dates::weekday) {
        Some(5) => 18,
        Some(6) => 4,
        _ => 22,
    };
    (0..buses).flat_map(|b| [tour(format!("E{b}"), 6 * 60), tour(format!("L{b}"), 14 * 60 + 30)]).collect()
}

/// The reference company, founded on a Monday: 30 single-deck diesel buses (12 leased new, 18
/// owned and bought used at nine years), 60 drivers and two cleaners and two in the control
/// room at the market's wage, its line in service.
fn reference(mode: GameMode, country: &str) -> Company {
    reference_from(mode, country, "2024-04-01")
}

pub(crate) fn reference_from(mode: GameMode, country: &str, date: &str) -> Company {
    let mut c = found(&Founding { name: "Reference".into(), game_mode: mode, date: date.into(), country: country.into(), ..Default::default() }, "Luc");
    c.cash += 100_000_000_00;
    c.site.set_level(depot::Area::Parking, 3);
    c.site.set_level(depot::Area::Fuel, 2);
    c.lines.push(CompanyLine { name: LINE.into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.planning.auto = true;
    let bus = MarketBus { file: "Vehicles/SingleDeck/SingleDeck.bus".into(), name: "Single-deck".into(), ..Default::default() };
    for _ in 0..12 {
        market::lease(&mut c, &bus, "").unwrap();
    }
    for _ in 0..18 {
        let id = market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        let v = c.fleet.iter_mut().find(|v| v.id == id).unwrap();
        // (bought used at the year's start, nine years old, for its value then)
        v.built = "2015-01-01".into();
        v.acquired = "2024-01-01".into();
        v.km = 540_000.0;
        v.next_service_km = 560_000.0;
        v.condition = 80.0;
        let Tenure::Owned { new_value, .. } = v.tenure else { unreachable!() };
        v.tenure = Tenure::Owned { paid: crate::company::economy::book_value(new_value, 9.0, 540_000.0, 80.0), new_value };
    }
    let id = c.id.clone();
    let mut round = 0;
    while c.staff.iter().filter(|e| e.role == Role::Driver).count() < 60 {
        for a in applicants_of(&c, Role::Driver) {
            if c.staff.iter().filter(|e| e.role == Role::Driver).count() < 60 && a.licence == Licence::D {
                hire(&mut c, &a).unwrap();
            }
        }
        round += 1;
        c.id = format!("{id}{round}");
        c.taken = Default::default();
    }
    for role in [Role::Cleaner, Role::Cleaner, Role::Service, Role::Service] {
        let a = applicants_of(&c, role)[0].clone();
        hire(&mut c, &a).unwrap();
        c.taken = Default::default();
    }
    c.id = id;
    let level = Level::of(&c);
    for e in c.staff.iter_mut() {
        e.hired = "2023-01-01".into();
        e.experience = 45.0;
        e.wage = crate::company::staff::market_wage(e.role, 45.0, &level);
        e.sick_until = None;
        e.holiday_until = None;
    }
    c
}

/// A month of April run, after March to settle in (the stores' first stock, the workshop's
/// first services), and its costs by kind (positive); the drivers' pay apart from the other
/// wages under `Agency` (`DRIVERS`: the drivers' pay and the agency's).
fn april(mode: GameMode, country: &str) -> (Company, Vec<(BookingKind, f64)>) {
    let mut c = reference_from(mode, country, "2024-03-04");
    let mut drivers = 0.0;
    while c.date.as_str() <= "2024-04-30" {
        let date = c.date.clone();
        let r = day::close_day(&mut c, tours_of(&date), &[]);
        if date.as_str() >= "2024-04-01" {
            drivers += r.driver_wages as f64;
        }
    }
    let m = c.month("2024-04");
    let mut costs: Vec<(BookingKind, f64)> = BookingKind::ALL.iter().filter(|k| !k.is_capital()).map(|k| (*k, -(m.get(*k) as f64))).filter(|x| x.1 > 0.0).collect();
    // (the drivers' pay out of the wages, into the drivers' line with the agency's)
    for x in costs.iter_mut() {
        match x.0 {
            BookingKind::Wages => x.1 -= drivers,
            BookingKind::Agency => x.1 += drivers,
            _ => {}
        }
    }
    if !costs.iter().any(|x| x.0 == BookingKind::Agency) {
        costs.push((BookingKind::Agency, drivers));
    }
    (c, costs)
}

fn of(costs: &[(BookingKind, f64)], kinds: &[BookingKind]) -> f64 {
    costs.iter().filter(|x| kinds.contains(&x.0)).map(|x| x.1).sum()
}

/// The direct costs as the data's books count them: all but the central overhead and the
/// contract penalties.
fn direct(costs: &[(BookingKind, f64)]) -> f64 {
    costs.iter().filter(|x| !matches!(x.0, BookingKind::Overheads | BookingKind::Penalty | BookingKind::Interest)).map(|x| x.1).sum()
}

fn pounds(cents: f64) -> f64 {
    Currency::Gbp.units(cents)
}

const DRIVERS: [BookingKind; 2] = [BookingKind::Wages, BookingKind::Agency];

#[test]
fn the_reference_company_costs_what_the_data_says() {
    let (c, costs) = april(GameMode::Realistic, "gb");
    let total = direct(&costs);
    let name = |k: &BookingKind| if *k == BookingKind::Agency { "Drivers (and agency)" } else if *k == BookingKind::Wages { "Other staff" } else { k.label() };
    let report: Vec<String> = costs.iter().map(|(k, a)| format!("{}: £{:.0} ({:.1} %)", name(k), pounds(*a), 100.0 * a / total)).collect();
    let text = format!("direct £{:.0}\n{}", pounds(total), report.join("\n"));
    println!("{text}");
    // about £300k a month (±15 %)
    assert!((pounds(total) / 300_000.0 - 1.0).abs() <= 0.15, "{text}");
    // the drivers 45-48 % of it, the fuel 24-26 % (close to: a point either side)
    let drivers = of(&costs, &DRIVERS) / total;
    let fuel = of(&costs, &[BookingKind::Energy]) / total;
    assert!((0.44..=0.49).contains(&drivers), "drivers {drivers}\n{text}");
    assert!((0.23..=0.27).contains(&fuel), "fuel {fuel}\n{text}");
    // the workshop about a tenth, the buses (lease and depreciation) about a tenth
    let shop = of(&costs, &[BookingKind::Maintenance, BookingKind::Repair, BookingKind::Parts]) / total;
    let buses = of(&costs, &[BookingKind::Lease, BookingKind::Depreciation]) / total;
    assert!((0.07..=0.14).contains(&shop), "workshop {shop}\n{text}");
    assert!((0.08..=0.15).contains(&buses), "buses {buses}\n{text}");
    // the overhead on top: £15k a bus a year
    // (the prices up by March's inflation)
    assert!((pounds(of(&costs, &[BookingKind::Overheads])) / (30.0 * (15_000.0 / 12.0 + 60.0)) - c.price_index).abs() < 0.01, "{text}");
    // depreciation is booked, and is no money paid: it lowers the result, not the cash
    assert!(of(&costs, &[BookingKind::Depreciation]) > 0.0);
    let mut k = c.clone();
    let (cash, result) = (k.cash, k.month(&dates::month_of(&k.date)).result());
    k.book(BookingKind::Depreciation, -1_000_00, "a bus", false);
    assert_eq!((k.cash, k.month(&dates::month_of(&k.date)).result()), (cash, result - 1_000_00));
}

#[test]
fn normal_and_casual_cost_less_by_their_factors() {
    let (_, real) = april(GameMode::Realistic, "gb");
    let (_, normal) = april(GameMode::Normal, "gb");
    let (_, casual) = april(GameMode::Casual, "gb");
    // (the lines the factors set: the drivers, fuel, the workshop's routine, the buses, insurance
    // and the overhead - not what the days bring, breakdowns and fines, nor what Casual's
    // autopilot does by itself, its research and its mechanics)
    let all = |c: &[(BookingKind, f64)]| of(c, &[BookingKind::Agency, BookingKind::Energy, BookingKind::Maintenance, BookingKind::Lease, BookingKind::Depreciation, BookingKind::Insurance, BookingKind::Overheads]);
    // fuel by the cost factor exactly (the same kilometres)
    let fuel = |c: &[(BookingKind, f64)]| of(c, &[BookingKind::Energy]);
    assert!((fuel(&normal) / fuel(&real) - 0.90).abs() < 0.02, "{} {}", fuel(&normal), fuel(&real));
    // (Casual's autopilot researches the fuel savings by itself: a little less still)
    assert!((0.65..=0.76).contains(&(fuel(&casual) / fuel(&real))), "{} {}", fuel(&casual), fuel(&real));
    // everything: Normal some tenth less and half the overhead, Casual a quarter less and none
    let (r, n, k) = (all(&real), all(&normal), all(&casual));
    assert!(n < r * 0.92 && n > r * 0.75, "{n} {r}");
    assert!(k < n * 0.90, "{k} {n}");
    assert_eq!(of(&casual, &[BookingKind::Overheads]) as i64, 30 * crate::company::economy::ticket_machine_per_month(&Level::at(GameMode::Casual, country("gb"), &crate::company::sandbox::CAREER, 1.0)));
}

#[test]
fn a_german_company_differs_by_its_country() {
    let (_, uk) = april(GameMode::Realistic, "gb");
    let (_, de) = april(GameMode::Realistic, "de");
    let land = country("de");
    let ratio = |kinds: &[BookingKind]| of(&de, kinds) / of(&uk, kinds);
    // the fuel by its diesel price, the drivers by its wages (their own draws aside)
    assert!((ratio(&[BookingKind::Energy]) - land.diesel).abs() < 0.02, "{}", ratio(&[BookingKind::Energy]));
    assert!((ratio(&DRIVERS) / land.wages - 1.0).abs() < 0.06, "{}", ratio(&DRIVERS));
    assert!((ratio(&[BookingKind::Insurance]) - land.general).abs() < 0.02, "{}", ratio(&[BookingKind::Insurance]));
}

#[test]
fn drivers_are_paid_their_duties_and_overtime() {
    let mut c = reference(GameMode::Realistic, "gb");
    // a Monday: every driver's duty paid from signing on to signing off
    let date = c.date.clone();
    let r = day::close_day(&mut c, tours_of(&date), &[]);
    let p = &c.planning.flow.prefs;
    let duty = (8 * 60 + 10 + p.pre_trip + p.post_trip + p.sign_on + p.sign_off) as f64 / 60.0;
    let drivers = c.staff.iter().filter(|e| e.week_paid > 0.0).count() as f64;
    assert!(drivers >= 40.0, "{drivers}");
    assert!((r.paid_hours - drivers * duty).abs() < 0.01, "{} {}", r.paid_hours, drivers * duty);
    assert!(c.staff.iter().all(|e| e.week_paid == 0.0 || (e.week_paid / 60.0 - duty).abs() < 1e-9));
    // and what they cost: the hours at their wage, the on-costs on top
    let want: f64 = c.staff.iter().filter(|e| e.week_paid > 0.0).map(|e| crate::company::staff::hourly(e.wage) * e.week_paid / 60.0).sum::<f64>() * all_in();
    assert!((r.driver_wages as f64 - want).abs() < 2.0, "{} {want}", r.driver_wages);
    assert_eq!(r.overtime_hours, 0.0);
    // the week: five such duties are more than the contract's 39 hours - the rest is overtime
    let mut week = 0.0;
    for _ in 1..7 {
        let date = c.date.clone();
        week += day::close_day(&mut c, tours_of(&date), &[]).overtime_hours;
    }
    assert!(week > 0.0, "{week}");
    // every driver's overtime of the week is over the contract: 5 duties less 39 hours at most
    assert!(week <= drivers * 1.5 * (5.0 * duty - CONTRACT_WEEK_HOURS).max(0.0) + 1.0, "{week}");
    // the week is closed: a new one begun
    assert!(c.staff.iter().all(|e| e.week_paid == 0.0 && e.week_due == 0.0));
}

#[test]
fn casual_pays_no_overtime() {
    let mut c = reference(GameMode::Casual, "gb");
    let mut over = 0.0;
    for _ in 0..7 {
        let date = c.date.clone();
        over += day::close_day(&mut c, tours_of(&date), &[]).overtime_hours;
    }
    assert_eq!(over, 0.0);
}

#[test]
fn a_driver_without_duties_is_paid_the_weeks_guarantee() {
    let mut c = reference(GameMode::Realistic, "gb");
    // a week without tours: every driver there all week is paid 39 hours on the Sunday
    let mut guaranteed = 0.0;
    for _ in 0..7 {
        guaranteed += day::close_day(&mut c, Vec::new(), &[]).guaranteed_hours;
    }
    let here = c.staff.iter().filter(|e| e.role == Role::Driver).count() as f64;
    assert!(guaranteed > here * CONTRACT_WEEK_HOURS * 0.8 && guaranteed <= here * CONTRACT_WEEK_HOURS + 0.01, "{guaranteed}");
}

#[test]
fn an_old_save_loads_with_its_wages_moved_to_the_new_market() {
    let c = reference(GameMode::Realistic, "gb");
    let mut v = serde_json::to_value(&c).unwrap();
    // a file of version 1: the German reference's flat monthly wages, no week of pay kept
    v["version"] = serde_json::json!(1);
    for e in v["staff"].as_array_mut().unwrap() {
        e["wage"] = serde_json::json!(3_310_00);
        e.as_object_mut().unwrap().remove("week_paid");
        e.as_object_mut().unwrap().remove("week_due");
    }
    let old: Company = serde_json::from_value(v).unwrap();
    let mut new = crate::company::store::migrate(old);
    // (the dispatcher's filling is not kept in a file)
    new.planning.auto = true;
    assert_eq!(new.version, crate::company::model::VERSION);
    // a driver of some experience at the market's (the old €3,310 was the old market's)
    let lv = Level::of(&new);
    let driver = new.staff.iter().find(|e| e.role == Role::Driver).unwrap();
    assert_eq!(driver.wage, crate::company::economy::market_wage(45.0, &lv));
    assert!((pounds(driver.wage as f64) / contract_month_hours() - DRIVER_BASE_HOUR / 100.0).abs() < 0.2);
    // and it runs a day
    let date = new.date.clone();
    let r = day::close_day(&mut new, tours_of(&date), &[]);
    assert!(r.paid_hours > 0.0, "{} {} {} {:?}", r.tours, r.covered, r.short_drivers, r.drops.first());
}

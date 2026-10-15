use super::*;
use crate::company::day::DutyPlan;
use crate::company::market::{self, Payment};
use crate::company::model::GameMode;
use crate::company::network::tests::trip;
use crate::company::network::TourOfDay;
use crate::company::staff::{applicants, hire};
use crate::company::{found, Founding};

/// A Realistic company (lateness at full) with one bus and one driver, on a Monday.
fn company() -> (Company, u32, u32) {
    let mut c = found(&Founding { name: "Late".into(), game_mode: GameMode::Realistic, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.cash += 5_000_000_00;
    let bus = market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    let id = c.id.clone();
    while c.staff.is_empty() {
        if let Some(a) = applicants(&c).into_iter().find(|a| a.licence == crate::company::model::Licence::D) {
            hire(&mut c, &a).unwrap();
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    c.id = id;
    let (bus, driver) = (c.fleet[0].id, c.staff[0].id);
    (c, bus, driver)
}

/// One block of `n` trips of 50 minutes from `from`, `layover` minutes between, one duty.
fn plan(from: i32, n: i32, layover: i32, bus: u32, driver: u32) -> Plan {
    let trips: Vec<_> = (0..n).map(|k| trip(from + k * (50 + layover), from + 50 + k * (50 + layover), 12)).collect();
    let to = trips.last().map_or(from, |t| t.arr);
    let tour = TourOfDay { line: "Linie5".into(), number: "5".into(), tour: "1".into(), ai_group: String::new(), trips, unplanned: false, cuts: None };
    let duty = DutyPlan { start: 0, end: n as usize, from, to, driver: Some(driver), dropped_before: None, problem: None };
    Plan { tours: vec![TourPlan { tour, bus: Some(bus), duties: vec![duty], ..Default::default() }] }
}

#[test]
fn a_delay_carries_on_along_the_block_and_a_layover_absorbs_it() {
    let (c, bus, driver) = company();
    let date = c.date.clone();
    // in the morning rush without a layover: each trip leaves later than the one before
    let tight = model(&c, &plan(7 * 60, 3, 0, bus, driver), &date, &[]);
    let t: Vec<&Timed> = tight[0].iter().map(|x| x.as_ref().unwrap()).collect();
    assert!(t[1].dep > 0.5 && t[2].dep > t[1].dep, "{:?}", t.iter().map(|x| x.dep).collect::<Vec<_>>());
    assert!(t[2].causes.iter().any(|c| matches!(c, Cause::Carried { layover: 0, .. })));
    // the delay up the chain is told with it (the rush hour's traffic)
    assert!(t[2].earlier.iter().any(|c| matches!(c, Cause::Traffic { peak: true, .. })) || t[2].causes.iter().any(|c| matches!(c, Cause::Traffic { peak: true, .. })));
    // fifteen minutes' layover: every trip after the first leaves on time
    let easy = model(&c, &plan(7 * 60, 3, 15, bus, driver), &date, &[]);
    assert!(easy[0].iter().skip(1).all(|x| x.as_ref().unwrap().dep == 0.0));
    // and the same plan the same day runs the same
    assert_eq!(model(&c, &plan(7 * 60, 3, 0, bus, driver), &date, &[]), tight);
}

#[test]
fn a_driver_signing_on_late_holds_the_first_trip_up() {
    let (c, bus, driver) = company();
    let date = c.date.clone();
    // (came late this morning: from 06:00 on, a walk to the bus)
    let mut p = plan(6 * 60, 2, 10, bus, driver);
    p.tours[0].duties[0].dropped_before = Some(6 * 60);
    let m = model(&c, &p, &date, &[]);
    let first = m[0][0].as_ref().unwrap();
    assert_eq!(first.causes[0], Cause::SignOn { driver, minutes: SIGN_ON_WALK });
    assert_eq!(first.dep, SIGN_ON_WALK as f64);
    // the outcome names its bus, its driver and its trip, and pays its tier
    let o = late(&p.tours[0], 0, &p.tours[0].tour.trips[0], bus, driver, first.clone(), 100_00);
    assert_eq!((o.bus, o.driver, o.trip.as_str(), o.duty), (Some(bus), Some(driver), "t360", Some(0)));
    assert_eq!(o.deduction, (100_00 as f64 * economy::late_share(o.minutes)).round() as Cents);
}

#[test]
fn a_closed_day_keeps_its_trips_and_tells_the_minutes() {
    let (mut c, bus, driver) = company();
    c.lines.push(crate::company::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.planning.auto = true;
    let _ = (bus, driver);
    let date = c.date.clone();
    let tours = plan(7 * 60, 4, 0, bus, driver).tours.into_iter().map(|t| t.tour).collect();
    let r = crate::company::day::close_day(&mut c, tours, &[]);
    assert!(of_day(&c, &date).is_some());
    // every late trip is in the report, with its minutes, and its deduction booked on its own
    assert_eq!(r.late as usize, r.lateness.trips.len());
    assert!(r.lateness.trips.iter().all(|o| o.minutes >= economy::LATE_FROM && o.bus.is_some() && o.driver.is_some() && !o.causes.is_empty()));
    let booked = c.ledger.iter().filter(|b| b.date == date && b.text.contains("min late")).count();
    assert_eq!(booked, r.lateness.trips.iter().filter(|o| o.deduction > 0).count());
    // (each trip timed, or dropped: a driver late this morning)
    assert_eq!(r.lateness.lines.iter().map(|p| p.timed).sum::<u32>() + r.dropped, 4);
}

#[test]
fn old_saves_load() {
    let (c, _, _) = company();
    // (a company and a report of before: no late running kept, no names on the drops)
    let mut v = serde_json::to_value(&c).unwrap();
    v.as_object_mut().unwrap().remove("lateness");
    let back: Company = serde_json::from_value(v).unwrap();
    assert_eq!(back.lateness, Store::default());
    let mut r = serde_json::to_value(crate::company::day::DayReport::default()).unwrap();
    for k in ["lateness", "late_lost"] {
        r.as_object_mut().unwrap().remove(k);
    }
    let r: crate::company::day::DayReport = serde_json::from_value(r).unwrap();
    assert!(r.lateness.trips.is_empty());
    let d: crate::company::day::Dropped = serde_json::from_str(r#"{"line":"L","number":"5","tour":"1","from":360,"to":410,"trips":1,"why":"late"}"#).unwrap();
    assert_eq!((d.bus, d.driver, d.trip.as_str()), (None, None, ""));
}

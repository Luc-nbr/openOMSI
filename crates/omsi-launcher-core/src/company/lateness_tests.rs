use super::*;
use crate::company::day::DutyPlan;
use crate::company::market::{self, Payment};
use crate::company::model::GameMode;
use crate::company::network::tests::trip;
use crate::company::network::TourOfDay;
use crate::company::staff::{applicants, hire};
use crate::company::{found, Founding};

/// A Realistic company (lateness at full) with one bus and one driver, on a Monday.
pub(crate) fn company() -> (Company, u32, u32) {
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
pub(crate) fn plan(from: i32, n: i32, layover: i32, bus: u32, driver: u32) -> Plan {
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
    // the outcome names its bus, its driver and its trip, and what it costs on a bus contract
    let o = late(&c, &p.tours[0], 0, &p.tours[0].tour.trips[0], bus, driver, first.clone(), 100_00);
    assert_eq!((o.bus, o.driver, o.trip.as_str(), o.duty), (Some(bus), Some(driver), "t360", Some(0)));
    assert_eq!(o.deduction, (100_00 as f64 * crate::company::bus_contracts::unpaid_share(&c, o.minutes)).round() as Cents);
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
    // (a late trip kept before its cause was traced up the chain: its main cause counts, and a
    // day's sums kept before the trips carried on were told apart)
    let o: Outcome = serde_json::from_str(r#"{"line":"L","number":"5","tour":"1","dep":600,"arr":650,"minutes":4,"causes":[{"kind":"carried","dep":540,"arr":590,"stop":"B","late":6,"layover":5,"deadhead":false,"minutes":4}],"deduction":1000}"#).unwrap();
    assert_eq!((o.root.as_str(), o.cause_key(), o.carried()), ("", "carried", true));
    let l: Lateness = serde_json::from_str(r#"{"lines":[],"hours":[],"duties":[],"tiers":[],"causes":[]}"#).unwrap();
    assert_eq!(l.carried, Sum::default());
}

// --- the day's numbers, pinned (Luc 2026-10-15: "I think it's not calculating correctly at all") --

/// The day as the model runs it with each trip's own minutes on the way set by hand (`own`): no
/// traffic, no driver, no bus, no chance, no late sign-on.
fn run(c: &Company, p: &Plan, own: &dyn Fn(usize, usize) -> f64) -> Model {
    model_with(c, p, &c.date, &[], &[], None, Some(own))
}

fn timed(m: &Model, ti: usize) -> Vec<Timed> {
    m[ti].iter().map(|x| x.clone().unwrap()).collect()
}

/// A tour of `trips` (departure, arrival) on line 5, its block `tour`, one duty of `driver`.
fn tour(name: &str, trips: &[(i32, i32)], bus: u32, driver: u32) -> TourPlan {
    let trips: Vec<_> = trips.iter().map(|&(a, z)| trip(a, z, 12)).collect();
    let (from, to) = (trips[0].dep, trips.last().unwrap().arr);
    let n = trips.len();
    let tour = TourOfDay { line: "Linie5".into(), number: "5".into(), tour: name.into(), ai_group: String::new(), trips, unplanned: false, cuts: None };
    TourPlan { tour, bus: Some(bus), duties: vec![DutyPlan { start: 0, end: n, from, to, driver: Some(driver), dropped_before: None, problem: None }], ..Default::default() }
}

/// The close's sums of a modelled day (`day::close_day_with` alike): every trip timed, the late
/// ones with their deductions (each trip worth 100 EUR).
fn close(c: &Company, p: &Plan, m: &Model) -> (Lateness, Vec<Outcome>) {
    let mut l = Lateness::default();
    let mut out = Vec::new();
    for (ti, tp) in p.tours.iter().enumerate() {
        let d = &tp.duties[0];
        for (j, x) in tp.tour.trips.iter().enumerate() {
            let Some(t) = whole(m, ti, j, false) else { continue };
            l.count(&tp.tour.number, &tp.tour.tour, 0, d.driver.unwrap(), x.dep, t.minutes());
            if t.minutes() >= economy::LATE_FROM {
                out.push(late(c, tp, 0, x, t.bus, d.driver.unwrap(), t, 100_00));
            }
        }
    }
    (l.finish(&out), out)
}

#[test]
fn a_layover_takes_up_all_but_the_rest_and_a_delay_neither_grows_nor_counts_twice() {
    let (c, bus, driver) = company();
    // the first trip loses six minutes on the way, the others none
    let own = |_: usize, j: usize| if j == 0 { 6.0 } else { 0.0 };
    // five minutes' layover: the driver keeps three (`control::MIN_LAYOVER`), two take the delay up
    let t = timed(&run(&c, &plan(10 * 60, 4, 5, bus, driver), &own), 0);
    assert_eq!(t.iter().map(Timed::minutes).collect::<Vec<_>>(), vec![6, 4, 2, 0]);
    // (carried on, not added to its own: 4 minutes late, not 6 + 4)
    assert_eq!(t[1].causes, vec![Cause::Carried { dep: 600, arr: 650, stop: "B".into(), late: 6, layover: 5, deadhead: false, minutes: 4 }]);
    assert_eq!((t[1].dep, t[1].arr), (4.0, 4.0));
    // ... and traced to what began it: the first trip's own minutes
    assert_eq!(t[1].root(), Some("chance"));
    assert_eq!(t[3].causes, vec![], "back on time");
    // no layover: it carries on to the end of the block, but no more than it was
    let t = timed(&run(&c, &plan(10 * 60, 4, 0, bus, driver), &own), 0);
    assert_eq!(t.iter().map(Timed::minutes).collect::<Vec<_>>(), vec![6, 6, 6, 6]);
    // ten minutes' layover: the next trip leaves on time
    let t = timed(&run(&c, &plan(10 * 60, 4, 10, bus, driver), &own), 0);
    assert_eq!(t.iter().map(Timed::minutes).collect::<Vec<_>>(), vec![6, 0, 0, 0]);
}

#[test]
fn on_time_is_under_six_whole_minutes_and_pays_nothing() {
    let (c, bus, driver) = company();
    let p = plan(10 * 60, 1, 0, bus, driver);
    let minutes = |m: f64| timed(&run(&c, &p, &move |_, _| m), 0)[0].minutes();
    // (whole minutes as a clock shows them: 2 min 59 s behind is 2; late from 6, the UK standard's
    // window - 5 min 59 s behind is on time)
    assert_eq!([minutes(0.0), minutes(1.0), minutes(2.6), minutes(2.99), minutes(3.0), minutes(5.9), minutes(6.0)], [0, 1, 2, 2, 3, 5, 6]);
    // (on a bus contract a journey is not paid from the mode's minutes late: Realistic's 10)
    let from = crate::company::econ_uk::mode(c.game_mode).unpaid_late_from.unwrap_or(i32::MAX);
    for (m, tier) in [(0, 0), (1, 0), (2, 0), (3, 0), (5, 0), (6, 1), (10, 1), (11, 2), (29, 2), (30, 3)] {
        assert_eq!(economy::late_tier(m), tier, "{m} min");
        assert_eq!(crate::company::bus_contracts::unpaid_share(&c, m), if m >= from { 1.0 } else { 0.0 }, "{m} min");
    }
    // a trip on time is timed, but in no tier and in no cause
    let (l, out) = close(&c, &p, &run(&c, &p, &|_, _| 5.5));
    assert!(out.is_empty());
    assert_eq!((l.lines[0].timed, l.lines[0].late), (1, 0));
    assert_eq!(l.tiers.iter().map(|t| t.trips).sum::<u32>(), 0);
    assert!(l.causes.is_empty() && l.deductions() == 0);
}

#[test]
fn a_late_trip_counts_once_in_a_tier_and_once_under_the_cause_it_began_with() {
    let (mut c, bus, driver) = company();
    let other = market::buy_new(&mut c, &market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
    // block 1 on one bus to 10:50, then the driver walks to block 2 on another bus leaving at 10:50
    // (no time planned for the relief), and block 2 goes on after five minutes' layover
    let p = Plan { tours: vec![tour("1", &[(600, 650)], bus, driver), tour("2", &[(650, 700), (705, 755), (760, 810)], other, driver)] };
    // the trip of block 1 loses five minutes on the way
    let m = run(&c, &p, &|ti, _| if ti == 0 { 5.0 } else { 0.0 });
    let b = timed(&m, 1);
    // the relief: the five minutes the driver came in late, and the walk (`RELIEF_WALK`) - traced
    // to what began it, block 1's own five minutes more than the walk's three
    assert_eq!(b[0].minutes(), 5 + RELIEF_WALK);
    assert!(matches!(b[0].causes[0], Cause::Relief { gap: 0, minutes: 8, .. }));
    assert_eq!(b[0].roots, vec![("chance", 5.0), ("relief", RELIEF_WALK as f64)]);
    assert_eq!(b[0].root(), Some("chance"), "{:?}", b[0].roots);
    // carried on to the next trip, less the two minutes the layover takes up
    assert_eq!(b[1].minutes(), 6);
    assert!(matches!(b[1].causes[0], Cause::Carried { late: 8, layover: 5, minutes: 6, .. }));
    assert_eq!(b[1].root(), Some("chance"));
    assert_eq!(b[2].minutes(), 4);
    let (l, out) = close(&c, &p, &m);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|o| o.root == "chance"));
    // the tiers and the causes: the same trips and the same money, each trip once
    let sum = |s: &[Sum]| (s.iter().map(|x| x.trips).sum::<u32>(), s.iter().map(|x| x.amount).sum::<Cents>());
    // (a commercial line: the late trips cost nothing on a contract)
    assert_eq!(sum(&l.tiers), (2, 0));
    assert_eq!(sum(&l.causes), sum(&l.tiers));
    assert_eq!(l.causes, vec![Sum { key: "chance".into(), trips: 2, minutes: 14, amount: 0 }]);
    // one of them carried its delay on from the trip before, as the advice for more layover says
    assert_eq!(l.carried.trips, 1);
    let layover: u32 = advice(&c, &l).iter().map(|a| if let Advice::Layover { trips, .. } = a { *trips } else { 0 }).sum();
    assert_eq!(layover, l.carried.trips);
    // by line, by hour and by duty: the same trips timed, the same late
    let total = |v: &[Punct]| (v.iter().map(|p| p.timed).sum::<u32>(), v.iter().map(|p| p.late).sum::<u32>());
    assert_eq!(total(&l.lines), (4, 2));
    assert_eq!(total(&l.hours), total(&l.lines));
    assert_eq!(total(&l.duties), total(&l.lines));
}

#[test]
fn the_second_part_of_a_trip_cut_at_a_relief_point_is_the_same_trip() {
    let (c, bus, driver) = company();
    let part = |second: bool| Some(crate::company::network::Part { second, stop: 5, at: "Mitte".into(), dep: 600, arr: 650, km: 15.0, stops: 12 });
    let (mut a, mut b) = (trip(600, 625, 6), trip(625, 650, 6));
    (a.part, b.part) = (part(false), part(true));
    let tour = TourOfDay { line: "Linie5".into(), number: "5".into(), tour: "1".into(), ai_group: String::new(), trips: vec![a, b], unplanned: false, cuts: None };
    let duties = vec![
        DutyPlan { start: 0, end: 1, from: 600, to: 625, driver: Some(driver), dropped_before: None, problem: None },
        DutyPlan { start: 1, end: 2, from: 625, to: 650, driver: Some(crate::company::plan::AGENCY), dropped_before: None, problem: None },
    ];
    let p = Plan { tours: vec![TourPlan { tour, bus: Some(bus), duties, ..Default::default() }] };
    let m = run(&c, &p, &|_, j| if j == 0 { 5.0 } else { 0.0 });
    // the whole trip: five minutes late from its first part's own, not "late from the trip
    // before" (no layover at the relief point to lengthen)
    let t = whole(&m, 0, 0, true).unwrap();
    assert_eq!(t.minutes(), 5);
    assert_eq!(t.causes, vec![Cause::Chance { minutes: 5 }]);
    assert_eq!(t.root(), Some("chance"));
    let o = late(&c, &p.tours[0], 0, &p.tours[0].tour.trips[0].joined(&p.tours[0].tour.trips[1]), bus, driver, t, 100_00);
    assert!(!o.carried());
    assert!(advice(&c, &Lateness { trips: vec![o], ..Default::default() }).is_empty());
}

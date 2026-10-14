//! Relief points with their own travel, and drivers changing halfway along a trip (Luc,
//! 2026-10-14): the travel looked up, a block cut on the way, the day's close of a trip two
//! duties share.

use super::super::network::{tours_of_day, PlannedTrip, TourOfDay};
use super::super::studio::{self as st, DutyTarget};
use super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::{StopInfo, TourInfo, TripInfo};

/// A trip of the timetable from `a` to `b` over S1, S2, Mitte (its stop 3) and S4, `dep` to
/// `arr` (minutes), the stops evenly apart.
fn tt(name: &str, index: usize, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    let names = [a, "S1", "S2", "Mitte", "S4", b];
    let stops = names
        .iter()
        .enumerate()
        .map(|(k, n)| {
            let t = (dep as f64 + (arr - dep) as f64 * k as f64 / 5.0) * 60.0;
            StopInfo { name: n.to_string(), id: k as i64, arr: t, dep: t }
        })
        .collect();
    TripInfo { name: name.into(), index, line: "5".into(), from: a.into(), terminus: format!("{b} Bf"), departure: dep as f64 * 60.0, arrival: arr as f64 * 60.0, stops, km: 20.0 }
}

/// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
/// minutes back and forth from A, ten minutes at each end, Mitte half an hour on.
fn line_of(tours: &[(&str, i32, i32)]) -> LineInfo {
    let tour = |&(no, from, n): &(&str, i32, i32)| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        next_run: None,
        trips: (0..n)
            .map(|k| {
                let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
                tt(&format!("5 {no}-{k}"), k as usize + 1, a, b, from + k * 60, from + k * 60 + 50)
            })
            .collect(),
    };
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Relief".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn day(c: &Company, l: &LineInfo) -> Vec<TourOfDay> {
    tours_of_day(c, std::slice::from_ref(l), &c.date)
}

fn block<'a>(tours: &'a [TourOfDay], no: &str) -> (usize, &'a TourOfDay) {
    tours.iter().enumerate().find(|(_, t)| t.tour == no).unwrap()
}

fn wd(c: &Company) -> u8 {
    super::super::plan::weekday_of(&c.date)
}

/// Mitte as a relief point the player added on line 5's way.
fn mitte() -> ReliefPoint {
    ReliefPoint { stop: "Mitte".into(), added: true, line: "Linie5".into(), ..Default::default() }
}

#[test]
fn travel_is_looked_up_by_the_pair_then_the_points_then_the_defaults() {
    let mut p = Prefs::default();
    // nothing set: the defaults (walk, five minutes)
    assert_eq!(travel_of(&p, "A", "B", None), (Travel::Walk, 5));
    // B's own: by public transport in twelve minutes - to B and from it
    p.points.push(ReliefPoint { stop: "B".into(), travel: Some(Travel::Public), public: Some(12), ..Default::default() });
    assert_eq!(travel_of(&p, "A", "B", None), (Travel::Public, 12));
    assert_eq!(travel_of(&p, "B", "A", None), (Travel::Public, 12));
    // (a way set by hand: the point's time of it, else the default's)
    assert_eq!(travel_of(&p, "A", "B", Some(Travel::Car)), (Travel::Car, 10));
    // where he goes comes before where he comes from
    p.points.push(ReliefPoint { stop: "A".into(), travel: Some(Travel::Car), car: Some(8), ..Default::default() });
    assert_eq!(travel_of(&p, "B", "A", None), (Travel::Car, 8));
    // the pair A to B: walked in three minutes, and so back where the way back has none
    p.pairs.push(TravelPair { from: "A".into(), to: "B".into(), walk: Some(3), ..Default::default() });
    assert_eq!(travel_of(&p, "A", "B", None), (Travel::Walk, 3));
    assert_eq!(travel_of(&p, "B", "A", None), (Travel::Walk, 3));
    // by hand by public transport: the pair has no time of it, B's own counts
    assert_eq!(travel_of(&p, "A", "B", Some(Travel::Public)), (Travel::Public, 12));
    // the way back with its own: the quickest it has
    p.pairs.push(TravelPair { from: "B".into(), to: "A".into(), car: Some(7), public: Some(20), ..Default::default() });
    assert_eq!(travel_of(&p, "B", "A", None), (Travel::Car, 7));
    assert_eq!(travel_of(&p, "B", "A", Some(Travel::Walk)), (Travel::Walk, 5));
    // an old save without relief points reads as before
    let old: Prefs = serde_json::from_str(r#"{"walk": 4, "travel": "Walk"}"#).unwrap();
    assert!(old.points.is_empty() && old.pairs.is_empty());
    assert_eq!(travel_of(&old, "A", "B", None), (Travel::Walk, 4));
    // (what says nothing is not kept)
    let mut q = Prefs { points: vec![ReliefPoint { stop: "A".into(), ..Default::default() }, mitte()], pairs: vec![TravelPair { from: "A".into(), to: "B".into(), ..Default::default() }], ..Default::default() };
    tidy_points(&mut q);
    assert_eq!(q.points, vec![mitte()]);
    assert!(q.pairs.is_empty());
}

#[test]
fn the_travel_of_a_relief_point_counts_in_the_duties_and_their_moves() {
    // tour 1's last trip ends at B at 08:50, tour 3 leaves A at 09:15: one driver for both
    let mut c = company();
    let l = line_of(&[("1", 360, 3), ("3", 555, 2)]);
    let w = wd(&c);
    let tours = day(&c, &l);
    let (one, _) = block(&tours, "1");
    let (three, _) = block(&tours, "3");
    st::join(&mut c, w, &tours, (one, 0), (three, 0)).unwrap();
    let travel_of_duty = |c: &Company| {
        let b = board_of(c, w, day(c, &l), Vec::new());
        let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
        let ev = s.events.iter().find_map(|e| if let EvKind::Travel(m) = e.kind { Some((m, e.to - e.from)) } else { None });
        (ev, s.issues.iter().any(|&i| b.issues[i].text == I_TRAVEL))
    };
    // the defaults: five minutes' walk
    assert_eq!(travel_of_duty(&c), (Some((Travel::Walk, 5)), false));
    // A by public transport in half an hour: no time for it (25 minutes)
    c.planning.flow.prefs.points.push(ReliefPoint { stop: "A".into(), travel: Some(Travel::Public), public: Some(30), ..Default::default() });
    assert_eq!(travel_of_duty(&c), (Some((Travel::Public, 30)), true));
    // the pair B to A by staff car in a quarter of an hour: in time
    c.planning.flow.prefs.pairs.push(TravelPair { from: "B".into(), to: "A".into(), car: Some(15), ..Default::default() });
    assert_eq!(travel_of_duty(&c), (Some((Travel::Car, 15)), false));
    // a trip moved to that driver needs the pair's time too
    let mut c2 = company();
    c2.planning.flow.prefs.points.push(ReliefPoint { stop: "A".into(), public: Some(30), travel: Some(Travel::Public), ..Default::default() });
    let tours = day(&c2, &l);
    let ds = st::duties(&c2.planning.joined, w, &tours);
    let b = st::duty_of_trip(&ds, &tours, block(&tours, "3").0, 0).unwrap();
    let a = st::duty_of_trip(&ds, &tours, block(&tours, "1").0, 0).unwrap();
    assert_eq!(st::plan_move_to_duty(&c2, w, &tours, b, &[(block(&tours, "3").0, 0)], DutyTarget::Duty(a)), Err(st::DRIVER_NO_TIME));
    c2.planning.flow.prefs.pairs.push(TravelPair { from: "B".into(), to: "A".into(), walk: Some(10), ..Default::default() });
    assert!(st::plan_move_to_duty(&c2, w, &tours, b, &[(block(&tours, "3").0, 0)], DutyTarget::Duty(a)).is_ok());
}

#[test]
fn a_block_is_cut_halfway_along_a_trip_at_a_relief_point_that_allows_it() {
    let mut c = company();
    let l = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    let lines = std::slice::from_ref(&l);
    let w = wd(&c);
    let tours = day(&c, &l);
    let (one, t1) = block(&tours, "1");
    // trip 2 of tour 1 (08:00 from A) passes Mitte at 08:30: no relief point yet
    assert_eq!(t1.trips[2].dep, 480);
    assert!(st::mid_stops(&c, &tours, lines, one, 2).is_empty());
    assert_eq!(st::can_cut_mid(&c, &tours, lines, one, 2, 3), Err(st::NOT_RELIEF));
    // Mitte added: drivers change there
    c.planning.flow.prefs.points.push(mitte());
    let mids = st::mid_stops(&c, &tours, lines, one, 2);
    assert_eq!(mids, vec![(3, "Mitte".to_string(), 510, Ok(()))]);
    st::cut_mid(&mut c, w, &tours, lines, one, 2, 3).unwrap();
    let tours = day(&c, &l);
    let (one, t1) = block(&tours, "1");
    // the trip in two at 08:30, a duty beginning with the second part
    assert_eq!(t1.trips.len(), 9);
    let (a, b) = (&t1.trips[2], &t1.trips[3]);
    assert_eq!((a.dep, a.arr, a.end(), a.to.as_str()), (480, 510, "Mitte", "B Bf"));
    assert_eq!((b.dep, b.arr, b.from.as_str(), b.end()), (510, 530, "Mitte", "B"));
    assert!(a.counts() && b.counts() && b.is_second() && a.whole_dep() == 480 && b.whole_dep() == 480);
    assert!((a.km + b.km - 20.0).abs() < 1e-9 && (a.km - 12.0).abs() < 1e-9);
    assert_eq!(network::duties_of(t1), vec![0..3, 3..9]);
    // the board: the first piece hands over at 08:30, the next driver takes over then; no
    // layover at Mitte, the trip counted once
    let b = board(&c, lines, &c.date);
    let ds = &b.duties;
    assert_eq!(ds.len(), 3);
    let first = b.shaped.iter().find(|s| s.spans.iter().any(|sp| sp.tour == one && sp.duty == 0)).unwrap();
    let second = b.shaped.iter().find(|s| s.spans.iter().any(|sp| sp.tour == one && sp.duty == 1)).unwrap();
    assert_eq!((first.spans[0].to, second.spans[0].from), (510, 510));
    assert!(!b.issues.iter().any(|i| i.text == I_LAYOVER || i.text == I_RELIEF_PLACE || i.text == I_SIGN_ON), "{:?}", b.issues);
    assert_eq!(b.figures.assigned, 16);
    assert_eq!(b.figures.trips, 16);
    // a trip may be cut on the way only once, and only a trip with passengers
    let again = st::mid_stops(&c, &tours, lines, one, 2);
    assert!(again.is_empty());
    assert_eq!(st::can_cut_mid(&c, &tours, lines, one, 3, 3), Err(st::MID_ALREADY));
    // saved and read again: the same day
    let back: Company = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert_eq!(day(&back, &l), tours);
    // Mitte no longer for signing on: the duty beginning there breaks it; no longer for
    // relief: the cut there breaks it, and no new one is made
    let mut c2 = c.clone();
    c2.planning.flow.prefs.points[0].sign = false;
    let b2 = board(&c2, lines, &c2.date);
    assert!(b2.issues.iter().any(|i| i.text == I_SIGN_ON && i.hard), "{:?}", b2.issues);
    c2.planning.flow.prefs.points[0].relief = false;
    let b2 = board(&c2, lines, &c2.date);
    assert!(b2.issues.iter().any(|i| i.text == I_RELIEF_PLACE && i.scope == Scope::Vehicle));
    let fresh = day(&company(), &l);
    assert_eq!(st::can_cut_mid(&c2, &fresh, lines, one, 2, 3), Err(st::NOT_RELIEF));
    // joined again where the second part begins: the trip whole
    st::uncut(&mut c, w, &tours, one, 3).unwrap();
    let tours = day(&c, &l);
    let (one, t1) = block(&tours, "1");
    assert_eq!(t1.trips.len(), 8);
    assert!(t1.trips.iter().all(|x| x.part.is_none()));
    assert_eq!(network::duties_of(t1), vec![0..8]);
    // cut again; the second part moved to the first driver: one driver works the trip whole,
    // and his duty is cut at B after it instead
    st::cut_mid(&mut c, w, &tours, lines, one, 2, 3).unwrap();
    let tours = day(&c, &l);
    let (one, _) = block(&tours, "1");
    let ds = st::duties(&c.planning.joined, w, &tours);
    let from = st::duty_of_trip(&ds, &tours, one, 3).unwrap();
    let to = st::duty_of_trip(&ds, &tours, one, 0).unwrap();
    st::move_to_duty(&mut c, w, &tours, from, &[(one, 3)], DutyTarget::Duty(to)).unwrap();
    let tours = day(&c, &l);
    let (_, t1) = block(&tours, "1");
    assert_eq!(t1.trips.len(), 8);
    assert_eq!(network::duties_of(t1), vec![0..3, 3..8]);
    // a trip cut on the way moves to another bus whole
    let mut c3 = company();
    c3.planning.flow.prefs.points.push(mitte());
    let tours = day(&c3, &l);
    let (two, _) = block(&tours, "2");
    st::cut_mid(&mut c3, w, &tours, lines, two, 7, 3).unwrap();
    let tours = day(&c3, &l);
    let (two, t2) = block(&tours, "2");
    assert_eq!(t2.trips.len(), 9);
    st::move_trip(&mut c3, w, &tours, two, 8, st::Target::New).unwrap();
    let tours = day(&c3, &l);
    assert_eq!(block(&tours, "2").1.trips.len(), 7);
    let (_, t3) = block(&tours, "3");
    assert_eq!(t3.trips.len(), 1);
    assert!(t3.trips[0].part.is_none() && t3.trips[0].arr == 860);
}

#[test]
fn optimise_keeps_to_where_the_relief_points_allow_drivers_to_change() {
    // a day too long for one driver: Optimise cuts at a terminus; with B and A closed for
    // relief, no cut where the bus stands at them
    let mut c = company();
    let l = line_of(&[("1", 300, 15)]);
    let lines = std::slice::from_ref(&l);
    let w = wd(&c);
    let (lays, joined) = optimise(&c, lines, &c.date);
    apply(&mut c, w, lays, joined);
    let tours = day(&c, &l);
    assert!(network::duties_of(&tours[0]).len() >= 2);
    let mut c2 = company();
    c2.planning.flow.prefs.points.push(ReliefPoint { stop: "A".into(), relief: false, ..Default::default() });
    c2.planning.flow.prefs.points.push(ReliefPoint { stop: "B".into(), relief: false, ..Default::default() });
    let tours = day(&c2, &l);
    assert!(!st::is_relief(&c2, &tours, "Linie5", "B"));
    assert!(st::is_relief(&c2, &tours, "Linie5", "Depot"));
    let (lays, joined) = optimise(&c2, lines, &c2.date);
    apply(&mut c2, w, lays, joined);
    let tours = day(&c2, &l);
    let t = &tours[0];
    for r in network::duties_of(t).iter().skip(1) {
        let place = t.trips[r.start].from.clone();
        assert!(st::is_relief(&c2, &tours, "Linie5", &place), "cut at {place}");
    }
}

#[test]
fn the_days_close_counts_a_trip_two_duties_share_once_and_runs_it_only_with_both() {
    use super::super::market::{self, Payment};
    use super::super::staff::{applicants, hire};
    let setup = |people: usize| {
        let mut c = company();
        c.planning.auto = true;
        c.cash += 5_000_000_00;
        market::buy_new(&mut c, &market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
        let id = c.id.clone();
        while c.staff.len() < people {
            for a in applicants(&c) {
                if c.staff.len() < people && a.licence == crate::company::model::Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        c
    };
    // a bus's day of four trips, the second cut at Mitte: two duties, the first to 08:30
    let trip = |name: &str, dep: i32| PlannedTrip { name: name.into(), line: "5".into(), from: "A".into(), to: "B".into(), dep, arr: dep + 50, km: 20.0, stops: 6, origin: "1".into(), last: "B".into(), ..Default::default() };
    let (a, b) = trip("5 1-1", 480).split_at(3, "Mitte", 510, 0.6).unwrap();
    let tour = TourOfDay { line: "Linie5".into(), number: "5".into(), tour: "1".into(), trips: vec![trip("5 1-0", 360), a, b, trip("5 1-2", 600)], cuts: Some(vec![2]), ..Default::default() };
    assert_eq!(network::duties_of(&tour), vec![0..2, 2..4]);
    // one driver: one duty runs, the other not - the trip they share is dropped, once
    let mut c = setup(1);
    let r = super::super::day::close_day(&mut c, vec![tour.clone()], &[]);
    assert_eq!((r.trips, r.dropped), (3, 2), "{r:?}");
    // two drivers: both parts run, the trip whole and once
    let mut c = setup(2);
    let r = super::super::day::close_day(&mut c, vec![tour], &[]);
    assert_eq!((r.trips, r.dropped), (3, 0));
    assert!((r.km - 60.0).abs() < 1e-6, "{}", r.km);
}

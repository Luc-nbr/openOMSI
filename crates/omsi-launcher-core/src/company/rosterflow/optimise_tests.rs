//! Optimise on a line whose bus does not stand at one stop between two trips, Optimise saying
//! when it finds nothing better, keeping to the duty types (testers, 2026-10-15: "optimise
//! feature do not work in rosterflow").

use super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::{StopInfo, TourInfo, TripInfo};

/// A trip of the timetable from `a` to `b`, `dep` to `arr` (minutes), with passengers.
fn tt(name: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    TripInfo {
        name: name.into(),
        index: 0,
        line: "5".into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: arr as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: (arr - dep) as f64 * 0.4,
    }
}

fn line(tours: Vec<TourInfo>) -> LineInfo {
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours }
}

fn tour(no: &str, trips: Vec<TripInfo>) -> TourInfo {
    TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips }
}

/// Line 5 from A to B only (Luc's line 880 runs one way): a trip of 20 minutes every half hour
/// from 06:00 to 17:30, the bus running back to A empty - it never stands where it goes on from.
fn one_way() -> LineInfo {
    line(vec![tour("1", (0..24).map(|k| tt(&format!("5 1-{k}"), "A", "B", 360 + k * 30, 380 + k * 30)).collect())])
}

/// Line 5 back and forth between A and B: tours (number, first departure, trips) of 50 minutes,
/// ten minutes at each end.
fn back_and_forth(tours: &[(&str, i32, i32)]) -> LineInfo {
    line(tours.iter().map(|&(no, from, n)| tour(no, (0..n).map(|k| if k % 2 == 0 { tt(&format!("5 {no}-{k}"), "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "B", "A", from + k * 60, from + k * 60 + 50) }).collect())).collect())
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Flow".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn hard(b: &Board) -> Vec<&'static str> {
    // (the planning's own: the test's company has no buses for its roster)
    b.issues.iter().filter(|i| i.hard && of_the_plan(i)).map(|i| i.text).collect()
}

#[test]
fn optimise_cuts_a_bus_that_runs_empty_between_its_trips() {
    // (the bus stands at B and goes on from A: Optimise asked for one stop and never cut it - the
    // day stayed one driver's of almost twelve hours, and Optimise proposed nothing better)
    let c = company();
    let l = one_way();
    let date = c.date.clone();
    let now = board(&c, std::slice::from_ref(&l), &date);
    assert!(!hard(&now).is_empty(), "the automatic duties break the hard rules");
    let o = optimise_day(&c, std::slice::from_ref(&l), &date);
    assert!(o.better && o.cost < o.was);
    assert!(hard(&o.board).is_empty(), "{:?}", hard(&o.board));
    assert!(o.board.shaped.len() >= 2);
    let p = &c.planning.flow.prefs;
    assert!(o.board.shaped.iter().all(|s| s.plan.longest() <= p.continuous.value), "{:?}", o.board.shaped.iter().map(|s| s.plan.longest()).collect::<Vec<_>>());
    assert_eq!(o.board.figures.assigned, 24, "every trip on a bus and in a duty");
    assert_eq!(uncuttable(&o.view, &o.board), 0);
    // what it made, optimised again: nothing better - the day as it is, said so
    let again = optimise_day(&o.view, std::slice::from_ref(&l), &date);
    assert!(!again.better);
    assert_eq!(again.cost, again.was);
    let wd = super::super::plandays::slot_of(&o.view, &date);
    assert_eq!(again.layouts, o.view.planning.layouts.iter().filter(|x| x.weekday == wd).cloned().collect::<Vec<_>>());
    // (no relief point drivers may change at: the bus cannot be cut, and that is said)
    let mut c2 = company();
    for stop in ["A", "B"] {
        c2.planning.flow.prefs.points.push(ReliefPoint { stop: stop.into(), relief: false, ..Default::default() });
    }
    let (layouts, joined) = optimise_with(&c2, std::slice::from_ref(&l), &date, true, true);
    let mut v2 = c2.clone();
    let w = super::super::plandays::slot_of(&c2, &date);
    apply(&mut v2, w, layouts, joined);
    let b2 = board(&v2, std::slice::from_ref(&l), &date);
    assert_eq!(b2.shaped.len(), 1, "one driver all day");
    assert_eq!(uncuttable(&v2, &b2), 1);
}

#[test]
fn optimise_makes_only_the_duty_types_the_preferences_allow() {
    // tour 1 from 06:00 to 09:50, tour 2 from 12:00: one bus, and one driver with a split
    let l = back_and_forth(&[("1", 360, 4), ("2", 720, 4)]);
    let made = |c: &Company| optimise_day(c, std::slice::from_ref(&l), &c.date.clone()).board;
    let b = made(&company());
    assert!(b.shaped.iter().any(|s| s.split()), "a split duty");
    assert!(!b.issues.iter().any(|i| i.text == I_TYPE));
    // no split duties allowed: two duties, of a type each
    let mut c = company();
    c.planning.flow.prefs.duty_types[3].allowed = false;
    let b = made(&c);
    assert!(!b.shaped.iter().any(|s| s.split()));
    assert!(!b.issues.iter().any(|i| i.text == I_TYPE), "{:?}", hard(&b));
}

#[test]
fn optimise_weighs_a_duty_as_a_whole_by_its_type_and_length() {
    let p = Prefs::default();
    let cut = |from: i32, to: i32, tour: usize| Cut { tour, range: 0..1, from, to, at: "A".into(), till: "A".into() };
    // a piece of 06:00 to 09:00 with one of 09:30 to 12:30 on another bus: an early shift
    let (a, b) = (cut(360, 540, 0), cut(570, 750, 1));
    assert_eq!(whole_rules(&p, &[&a, &b]), (0.0, 0));
    // one from 14:00 to 18:00 after a split: a split shift may not begin after 13:00 - no type
    let (c1, c2) = (cut(840, 960, 0), cut(1080, 1320, 1));
    assert_eq!(whole_rules(&p, &[&c1, &c2]).1, 1);
    // nor a duty over a day the night's rest leaves for
    let late = cut(1500, 1560, 1);
    assert!(whole_rules(&Prefs::default(), &[&a, &late]).1 >= 1);
}

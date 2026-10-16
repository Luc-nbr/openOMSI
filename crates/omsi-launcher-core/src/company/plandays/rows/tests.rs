//! A day's rows copied to another day by themselves: a bus's block, a duty, for a date only, and
//! the trips that do not match or are locked there.

use super::super::super::model::{CompanyLine, GameMode};
use super::super::super::network::tours_of_day;
use super::super::super::studio::{self as st, Target};
use super::super::super::{found, Founding};
use super::*;
use crate::{StopInfo, TourInfo, TripInfo};

const MON: &str = "2024-03-04";
const TUE: &str = "2024-03-05";
const WED: &str = "2024-03-06";

/// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
/// minutes back and forth from A, ten minutes at each end.
fn line_of(tours: &[(&str, i32, i32)]) -> Vec<LineInfo> {
    let tt = |name: String, a: &str, b: &str, dep: i32| TripInfo {
        name,
        index: 0,
        line: "5".into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: (dep + 50) as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: 20.0,
    };
    let tour = |&(no, from, n): &(&str, i32, i32)| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        next_run: None,
        trips: (0..n).map(|k| if k % 2 == 0 { tt(format!("5 {no}-{k}"), "A", "B", from + k * 60) } else { tt(format!("5 {no}-{k}"), "B", "A", from + k * 60) }).collect(),
    };
    vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }]
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Rows".into(), date: MON.into(), ..Default::default() }, "Luc");
    c.game_mode = GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn at(tours: &[TourOfDay], no: &str) -> usize {
    tours.iter().position(|t| t.tour == no).unwrap()
}

fn starts(tours: &[TourOfDay], no: &str) -> Vec<usize> {
    network::duties_of(&tours[at(tours, no)]).iter().map(|r| r.start).collect()
}

/// Monday as a scheduler left it (as `plandays`' tests have it): tour 2's last trip on a bus of
/// its own (3, locked), tour 1 cut at its fifth trip and tour 2 at its seventh, tour 1's first
/// piece joined with tour 2's second as a split duty with a note. Returns the split duty's key.
fn monday(c: &mut Company, lines: &[LineInfo]) -> PieceRef {
    let day = |c: &Company| tours_of_day(c, lines, MON);
    let tours = day(c);
    st::move_trip(c, 0, &tours, at(&tours, "2"), 7, Target::New).unwrap();
    let tours = day(c);
    st::cut(c, 0, &tours, at(&tours, "1"), 4).unwrap();
    let tours = day(c);
    st::cut(c, 0, &tours, at(&tours, "2"), 6).unwrap();
    let tours = day(c);
    st::join(c, 0, &tours, (at(&tours, "1"), 0), (at(&tours, "2"), 1)).unwrap();
    rf::set_block_lock(c, 0, "Linie5", "3", true);
    let tours = day(c);
    let d = st::duties(&c.planning.joined, 0, &tours);
    let key = rf::duty_key(&tours, d.iter().find(|d| d.pieces.len() == 2).unwrap()).unwrap();
    rf::note_mut(c, 0, &key).kind = Some(rf::TYPE_NAMES[3].into());
    rf::note_mut(c, 0, &key).sign_on_at = Some(rf::DEPOT_POINT.into());
    key
}

fn rows_of_block(name: &str) -> Rows {
    Rows { blocks: vec![("Linie5".into(), name.into())], ..Default::default() }
}

#[test]
fn a_bus_copied_by_itself_takes_its_trips_off_the_buses_there_and_its_lock_along() {
    let mut c = company();
    let lines = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    monday(&mut c, &lines);
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows_of_block("3"));
    assert_eq!((made.matched, made.total, made.missing.len()), (1, 1, 0));
    let tue = tours_of_day(&c, &lines, TUE);
    let deps = |no: &str| tue[at(&tue, no)].trips.iter().map(|x| x.dep).collect::<Vec<_>>();
    assert_eq!((deps("3"), deps("2").len(), deps("1").len()), (vec![810], 7, 8));
    assert!(rf::block_locked(&c, 1, "Linie5", "3"));
    // (Tuesday's duties as they were: only the bus copied)
    assert!(!c.planning.joined.iter().any(|j| j.weekday == 1));
    // again: bus 3 is locked there now and holds the trip - it is this bus, copied there before,
    // so it is copied over (testers: "sometimes it works, sometimes it doesn't")
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows_of_block("3"));
    assert_eq!((made.matched, made.total, made.missing.len(), made.locked), (1, 1, 0, 0));
    assert_eq!(tours_of_day(&c, &lines, TUE).len(), 3);
    assert!(rf::block_locked(&c, 1, "Linie5", "3"));
    // a locked bus there of another name holding the trip: nothing taken, the trip listed as held
    rf::set_block_lock(&mut c, 1, "Linie5", "3", false);
    let tue = tours_of_day(&c, &lines, TUE);
    st::move_trip(&mut c, 1, &tue, at(&tue, "3"), 0, Target::Block(at(&tue, "2"))).unwrap();
    rf::set_block_lock(&mut c, 1, "Linie5", "2", true);
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows_of_block("3"));
    assert_eq!((made.matched, made.total, made.missing.len(), made.locked), (0, 1, 1, 1));
    rf::set_block_lock(&mut c, 1, "Linie5", "2", false);
    // a day whose tour 2 runs five minutes later: the trip matches nothing there
    let other = line_of(&[("1", 360, 8), ("2", 395, 8)]);
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 2, Day { date: WED, lines: &other }, &rows_of_block("3"));
    assert_eq!((made.matched, made.total), (0, 1));
    assert_eq!(made.missing.iter().map(|m| (m.dep, m.block.as_str())).collect::<Vec<_>>(), vec![(810, "3")]);
    assert_eq!(tours_of_day(&c, &other, WED).len(), 2);
}

#[test]
fn a_duty_copied_by_itself_is_cut_and_joined_there_with_its_note() {
    let mut c = company();
    let lines = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    let key = monday(&mut c, &lines);
    let rows = Rows { duties: vec![key], ..Default::default() };
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows);
    assert_eq!((made.matched, made.total, made.missing.len()), (5, 5, 0));
    // Tuesday keeps its two buses; the duty's trips are pieces of their own there, joined
    let tue = tours_of_day(&c, &lines, TUE);
    assert_eq!(tue.len(), 2);
    assert!(starts(&tue, "1").contains(&4));
    let two = starts(&tue, "2");
    assert!(two.contains(&6) && two.contains(&7), "{two:?}");
    let duties = st::duties(&c.planning.joined, 1, &tue);
    let split = duties.iter().find(|d| d.pieces.len() == 2).unwrap();
    let k = rf::duty_key(&tue, split).unwrap();
    let n = c.planning.flow.sheet.notes.iter().find(|n| n.weekday == 1 && n.key.is(&k)).unwrap();
    assert_eq!((n.kind.as_deref(), n.sign_on_at.as_deref()), (Some(rf::TYPE_NAMES[3]), Some(rf::DEPOT_POINT)));
    // (Monday as it was)
    assert_eq!(st::duties(&c.planning.joined, 0, &tours_of_day(&c, &lines, MON)).iter().filter(|d| d.pieces.len() == 2).count(), 1);
}

#[test]
fn rows_copied_for_one_date_only_leave_its_weekday_as_it_was() {
    let mut c = company();
    let lines = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    monday(&mut c, &lines);
    let f = rows_slot(&mut c, WED, false);
    assert!(f >= FIRST_DATE_SLOT && slot_of(&c, WED) == f);
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, f, Day { date: WED, lines: &lines }, &rows_of_block("3"));
    assert_eq!(made.matched, 1);
    assert_eq!(tours_of_day(&c, &lines, WED).len(), 3);
    assert_eq!(tours_of_day(&c, &lines, "2024-03-13").len(), 2, "the next Wednesday runs the weekday's plan");
    let s = state(&c, f);
    assert!(s.date_only && s.forked && s.pattern == Some(2));
    // (the weekday for every week)
    assert_eq!(rows_slot(&mut c, TUE, true), 1);
}

#[test]
fn a_bus_copied_by_itself_onto_every_other_weekday_twice_is_there_each_time() {
    // (testers: "sometimes it works, sometimes it doesn't. Also, it seems you can't copy the
    // Tuesday": a locked bus copied once took its lock along, and a second copy found it locked)
    let week = [MON, TUE, WED, "2024-03-07", "2024-03-08", "2024-03-09", "2024-03-10"];
    let lines = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    for i in 0..7 {
        for j in (0..7).filter(|&j| j != i) {
            let mut c = company();
            let s = i as u8;
            let tours = tours_of_day(&c, &lines, week[i]);
            st::move_trip(&mut c, s, &tours, at(&tours, "2"), 7, Target::New).unwrap();
            rf::set_block_lock(&mut c, s, "Linie5", "3", true);
            for round in 0..2 {
                let made = copy_rows(&mut c, s, Day { date: week[i], lines: &lines }, j as u8, Day { date: week[j], lines: &lines }, &rows_of_block("3"));
                assert_eq!((made.matched, made.total, made.missing.len(), made.locked), (1, 1, 0, 0), "{} onto {}, copy {round}", week[i], week[j]);
                let there = tours_of_day(&c, &lines, week[j]);
                assert_eq!(there.iter().map(|t| (t.tour.as_str(), t.trips.len())).collect::<Vec<_>>(), vec![("1", 8), ("2", 7), ("3", 1)]);
                assert!(rf::block_locked(&c, j as u8, "Linie5", "3"));
            }
        }
    }
}

#[test]
fn a_locked_duty_copied_twice_is_copied_over_the_second_time() {
    let lines = line_of(&[("1", 360, 8), ("2", 390, 8)]);
    let mut c = company();
    let key = monday(&mut c, &lines);
    rf::set_duty_lock(&mut c, 0, &key, true);
    let rows = Rows { duties: vec![key], ..Default::default() };
    for _ in 0..2 {
        let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows);
        assert_eq!((made.matched, made.total, made.missing.len(), made.locked), (5, 5, 0, 0));
        let tue = tours_of_day(&c, &lines, TUE);
        assert_eq!(st::duties(&c.planning.joined, 1, &tue).iter().filter(|d| d.pieces.len() == 2).count(), 1);
    }
    // from the draft, after switching days: the copy on Tuesday copied on to Wednesday
    let mut v = rf::view(&c);
    let tue = tours_of_day(&v, &lines, TUE);
    let d = st::duties(&v.planning.joined, 1, &tue);
    let k = rf::duty_key(&tue, d.iter().find(|d| d.pieces.len() == 2).unwrap()).unwrap();
    let made = copy_rows(&mut v, 1, Day { date: TUE, lines: &lines }, 2, Day { date: WED, lines: &lines }, &Rows { duties: vec![k], ..Default::default() });
    assert_eq!((made.matched, made.missing.len()), (5, 0));
    rf::keep_draft(&mut c, &v);
    assert_eq!(changed_slots(&c), vec![2]);
    // the plan copied onto the day it is: nothing to copy, and said so
    let made = copy_rows(&mut c, 1, Day { date: TUE, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows_of_block("1"));
    assert!(made.same_plan && made.matched == 0);
}

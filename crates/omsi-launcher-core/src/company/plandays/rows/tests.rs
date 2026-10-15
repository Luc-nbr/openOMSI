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
    // again: bus 3 is locked there now and holds the trip - nothing taken, the trip listed
    let made = copy_rows(&mut c, 0, Day { date: MON, lines: &lines }, 1, Day { date: TUE, lines: &lines }, &rows_of_block("3"));
    assert_eq!((made.matched, made.total, made.missing.len()), (0, 1, 1));
    assert_eq!(tours_of_day(&c, &lines, TUE).len(), 3);
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

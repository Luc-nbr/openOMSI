//! Luc's Beta 65 reports on splitting and locking: a duty split keeps its row and the half split
//! off gets a new one at the bottom (#5); a split leaves every other duty as it is (#6); a locked
//! duty keeps its own pieces only - a later trip of its bus can still be cut for a relief (#8);
//! Optimise and Discard leave what is locked as it is (#9).

use super::super::network::tours_of_day;
use super::super::studio::{self as st, Target};
use super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::{StopInfo, TourInfo, TripInfo};

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

/// Line 5 between A and B: tours 1 (from 06:00) and 2 (from 06:30), eight trips of 50 minutes
/// back and forth from A each, ten minutes at each end.
fn line5() -> LineInfo {
    let tour = |no: &str, from: i32| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        next_run: None,
        trips: (0..8).map(|k| if k % 2 == 0 { tt(&format!("5 {no}-{k}"), "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "B", "A", from + k * 60, from + k * 60 + 50) }).collect(),
    };
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 390)] }
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Flow".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn day(c: &Company, l: &LineInfo) -> Vec<TourOfDay> {
    tours_of_day(c, std::slice::from_ref(l), &c.date)
}

fn at(tours: &[TourOfDay], no: &str) -> usize {
    tours.iter().position(|t| t.tour == no).unwrap()
}

fn wd(c: &Company) -> u8 {
    super::super::plandays::slot_of(c, &c.date)
}

fn board_of_day(c: &Company, l: &LineInfo) -> Board {
    board(c, std::slice::from_ref(l), &c.date)
}

/// Each duty of the day as its key and the names of its trips with passengers, in the order of
/// its rows.
fn duties_now(c: &Company, l: &LineInfo) -> Vec<(PieceRef, Vec<String>)> {
    let b = board_of_day(c, l);
    let mut out = Vec::new();
    for s in b.shaped.iter().filter(|s| s.spare.is_none()) {
        let (Some(key), Some(d)) = (s.key.clone(), s.duty.and_then(|k| b.duties.get(k))) else { continue };
        let mut names = Vec::new();
        for p in &d.pieces {
            let t = &b.tours[p.tour];
            let r = network::duties_of(t)[p.duty].clone();
            names.extend(t.trips[r].iter().filter(|x| x.counts()).map(|x| x.name.clone()));
        }
        out.push((key, names));
    }
    out
}

/// The two buses cut after their fourth trip, and bus 1's first piece (06:00-09:50, back at A)
/// joined with bus 2's second (10:30-13:20, from A): one duty that changes buses.
fn two_buses_one_joined(l: &LineInfo) -> Company {
    let mut c = company();
    let w = wd(&c);
    let tours = day(&c, l);
    st::cut(&mut c, w, &tours, at(&tours, "1"), 4).unwrap();
    let tours = day(&c, l);
    st::cut(&mut c, w, &tours, at(&tours, "2"), 4).unwrap();
    let tours = day(&c, l);
    st::join(&mut c, w, &tours, (at(&tours, "1"), 0), (at(&tours, "2"), 1)).unwrap();
    c
}

/// The duty bus 1's first trip is in: its key.
fn joined_key(c: &Company, l: &LineInfo) -> PieceRef {
    duties_now(c, l).into_iter().find(|(_, trips)| trips.contains(&"5 1-0".to_string())).unwrap().0
}

#[test]
fn a_split_leaves_every_other_duty_as_it_is() {
    let l = line5();
    let mut c = two_buses_one_joined(&l);
    let w = wd(&c);
    let before = duties_now(&c, &l);
    assert_eq!(before.len(), 3, "{before:?}");
    let joined = before.iter().find(|(_, t)| t.len() == 8).cloned().expect("the duty on both buses");
    let blocks: Vec<Vec<String>> = day(&c, &l).iter().map(|t| t.trips.iter().map(|x| x.name.clone()).collect()).collect();
    // bus 2's first piece cut after its second trip: the joined duty has a piece of bus 2 too,
    // and was taken apart with every other duty of the bus (Beta 65)
    let tours = day(&c, &l);
    st::cut(&mut c, w, &tours, at(&tours, "2"), 2).unwrap();
    let after = duties_now(&c, &l);
    assert_eq!(after.len(), 4);
    assert!(after.contains(&joined), "the duty joined across both buses stays: {after:?}");
    assert!(after.iter().any(|(_, t)| *t == ["5 2-0", "5 2-1"]));
    assert!(after.iter().any(|(_, t)| *t == ["5 2-2", "5 2-3"]));
    // the buses run what they ran
    let now: Vec<Vec<String>> = day(&c, &l).iter().map(|t| t.trips.iter().map(|x| x.name.clone()).collect()).collect();
    assert_eq!(now, blocks);
    // joined again where it was cut: the duties of the bus's other pieces stay too
    let tours = day(&c, &l);
    st::uncut(&mut c, w, &tours, at(&tours, "2"), 2).unwrap();
    assert_eq!(duties_now(&c, &l).len(), 3);
    assert!(duties_now(&c, &l).contains(&joined));
}

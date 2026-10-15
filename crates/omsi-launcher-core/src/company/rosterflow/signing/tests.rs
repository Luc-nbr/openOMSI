//! A duty's own sign-on, sign-off and break points (testers, 2026-10-15): the travel to and from
//! them, the issues of the points chosen, the company's default, and Optimise keeping them.

use super::super::super::network::{tours_of_day, TourOfDay};
use super::super::super::studio as st;
use super::super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::{StopInfo, TourInfo, TripInfo};

/// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
/// minutes back and forth from A, ten minutes at each end.
fn line_of(tours: &[(&str, i32, i32)]) -> LineInfo {
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
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Signing".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

/// Tour 1 (06:00, three trips, A to B last) and tour 3 (09:15 from A, two trips).
fn line() -> LineInfo {
    line_of(&[("1", 360, 3), ("3", 555, 2)])
}

fn day(c: &Company, l: &LineInfo) -> Vec<TourOfDay> {
    tours_of_day(c, std::slice::from_ref(l), &c.date)
}

fn board(c: &Company, l: &LineInfo) -> Board {
    board_of(c, super::super::super::plan::weekday_of(&c.date), day(c, l), Vec::new())
}

fn block(tours: &[TourOfDay], no: &str) -> usize {
    tours.iter().position(|t| t.tour == no).unwrap()
}

/// Mitte as a relief point the player added on line 5's way.
fn mitte() -> ReliefPoint {
    ReliefPoint { stop: "Mitte".into(), added: true, line: "Linie5".into(), ..Default::default() }
}

fn texts(b: &Board, s: &Shaped) -> Vec<&'static str> {
    s.issues.iter().map(|&i| b.issues[i].text).collect()
}

#[test]
fn a_duty_signs_on_and_off_where_it_or_the_company_chose_and_travels_to_the_bus() {
    let mut c = company();
    let l = line();
    let w = super::super::super::plan::weekday_of(&c.date);
    // as before: signs on where the bus is, just before it
    let s = board(&c, &l).shaped[0].clone();
    let (first, last) = (s.spans[0].from, s.spans[0].to);
    assert_eq!(s.events[0], Ev { kind: EvKind::SignOn, from: first - 10, to: first });
    assert_eq!(s.points, Points::default());
    // the company's drivers sign on at the depot: a five minutes' walk to A after it
    c.planning.flow.prefs.sign_on_at = Some(DEPOT_POINT.into());
    let s = board(&c, &l).shaped[0].clone();
    assert_eq!(s.events[0], Ev { kind: EvKind::SignOn, from: first - 15, to: first - 5 });
    assert_eq!(s.events[1], Ev { kind: EvKind::Travel(Travel::Walk), from: first - 5, to: first });
    assert_eq!((s.points.on.as_deref(), s.start), (Some(DEPOT_POINT), first - 15));
    // the duty chose where the bus is (over the company's), and to sign off at A, from B
    let key = s.key.clone().unwrap();
    note_mut(&mut c, w, &key).sign_on_at = Some(String::new());
    note_mut(&mut c, w, &key).sign_off_at = Some("A".into());
    let s = board(&c, &l).shaped[0].clone();
    assert_eq!(s.events[0], Ev { kind: EvKind::SignOn, from: first - 10, to: first });
    assert!(s.events.contains(&Ev { kind: EvKind::Travel(Travel::Walk), from: last, to: last + 5 }), "{:?}", s.events);
    assert!(s.events.contains(&Ev { kind: EvKind::SignOff, from: last + 5, to: last + 10 }));
    // (by public transport, set on its event: A's own minutes of it)
    c.planning.flow.prefs.points.push(ReliefPoint { stop: "A".into(), public: Some(12), ..Default::default() });
    note_mut(&mut c, w, &key).travel.push((OFF_LEG, Travel::Public));
    let s = board(&c, &l).shaped[0].clone();
    assert!(s.events.contains(&Ev { kind: EvKind::Travel(Travel::Public), from: last, to: last + 12 }));
    // an old note reads as before
    let old: DutyNote = serde_json::from_str(r#"{"weekday": 0, "key": {"line": "L", "tour": "1", "first": {"tour": "1", "name": "x", "dep": 1}}, "locked": true}"#).unwrap();
    assert!(!old.has_points() && old.sign_on_at.is_none());
}

#[test]
fn a_break_taken_at_a_relief_point_is_travelled_to_and_checked() {
    let mut c = company();
    let l = line();
    let w = super::super::super::plan::weekday_of(&c.date);
    // one driver for tour 1 (off at B at 08:55) and tour 3 (on at A at 09:10)
    let tours = day(&c, &l);
    st::join(&mut c, w, &tours, (block(&tours, "1"), 0), (block(&tours, "3"), 0)).unwrap();
    c.planning.flow.prefs.points.push(mitte());
    let b = board(&c, &l);
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
    let (a_to, b_from) = (s.spans[0].to, s.spans[1].from);
    assert!(!texts(&b, &s).contains(&I_TRAVEL));
    // the break at Mitte: from B there, and on to A - five minutes each way
    note_mut(&mut c, w, s.key.as_ref().unwrap()).breaks_at.push((0, "Mitte".into()));
    let b = board(&c, &l);
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
    assert_eq!(s.points.breaks, vec![(0, "Mitte".to_string())]);
    assert!(s.events.contains(&Ev { kind: EvKind::Travel(Travel::Walk), from: a_to, to: a_to + 5 }));
    assert!(s.events.contains(&Ev { kind: EvKind::Travel(Travel::Walk), from: b_from - 5, to: b_from }));
    assert!(!texts(&b, &s).contains(&I_TRAVEL));
    // Mitte a quarter of an hour's walk: no time for it there and back
    c.planning.flow.prefs.points[0].walk = Some(15);
    let b = board(&c, &l);
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
    assert!(texts(&b, &s).contains(&I_TRAVEL));
    // no break may be taken at Mitte, nor a duty begin there: the points chosen say so
    c.planning.flow.prefs.points[0] = ReliefPoint { breaks: false, sign: false, ..mitte() };
    note_mut(&mut c, w, s.key.as_ref().unwrap()).sign_on_at = Some("Mitte".into());
    let b = board(&c, &l);
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
    assert!(texts(&b, &s).contains(&I_BREAK_AT) && texts(&b, &s).contains(&I_SIGN_ON), "{:?}", texts(&b, &s));
    assert_eq!(issue_title(I_BREAK_AT), "Breaks at a wrong place");
    // (a stop that is no relief point at all, chosen to sign off at)
    note_mut(&mut c, w, s.key.as_ref().unwrap()).sign_off_at = Some("S2".into());
    let b = board(&c, &l);
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().clone();
    assert!(texts(&b, &s).contains(&I_SIGN_OFF));
}

#[test]
fn optimise_keeps_a_duty_s_points_on_the_duty_that_holds_its_first_trip() {
    let before = company();
    let l = line();
    let w = super::super::super::plan::weekday_of(&before.date);
    let mut before = before;
    let tours = day(&before, &l);
    // tour 3's duty signs on at the depot
    let duties = st::duties(&before.planning.joined, w, &tours);
    let three = duties.iter().find(|d| d.pieces[0].tour == block(&tours, "3")).unwrap();
    let k3 = duty_key(&tours, three).unwrap();
    note_mut(&mut before, w, &k3).sign_on_at = Some(DEPOT_POINT.into());
    note_mut(&mut before, w, &k3).breaks_at.push((3, "A".into()));
    // Optimise joins it behind tour 1's: that duty now holds its first trip
    let mut after = before.clone();
    st::join(&mut after, w, &tours, (block(&tours, "1"), 0), (block(&tours, "3"), 0)).unwrap();
    keep_points(&before, &mut after, std::slice::from_ref(&l), &before.date.clone());
    let tours = day(&after, &l);
    let joined = st::duties(&after.planning.joined, w, &tours).into_iter().find(|d| d.pieces.len() == 2).unwrap();
    let k1 = duty_key(&tours, &joined).unwrap();
    let n = note(&after, w, &k1).unwrap();
    // (its sign-on point along; a break of a gap the duty no longer has not)
    assert_eq!((n.sign_on_at.as_deref(), n.breaks_at.len()), (Some(DEPOT_POINT), 0));
    assert!(note(&after, w, &k3).is_none());
}

use super::super::model::{CompanyLine, GameMode};
use super::super::network::tours_of_day;
use super::super::studio::{self as st, Target};
use super::super::{found, Founding};
use super::*;
use crate::{StopInfo, TourInfo, TripInfo};

const MON: &str = "2024-03-04";
const TUE: &str = "2024-03-05";
const WED: &str = "2024-03-06";

/// A trip of the timetable: from `a` to `b`, `dep` to `arr` (minutes), with passengers.
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

/// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
/// minutes back and forth from A, ten minutes at each end.
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
                tt(&format!("5 {no}-{k}"), a, b, from + k * 60, from + k * 60 + 50)
            })
            .collect(),
    };
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }
}

/// Tour 1 from 06:00 and tour 2 from 06:30, eight trips each.
fn line5() -> Vec<LineInfo> {
    vec![line_of(&[("1", 360, 8), ("2", 390, 8)])]
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Days".into(), date: MON.into(), ..Default::default() }, "Luc");
    c.game_mode = GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn at<'a>(tours: &'a [TourOfDay], no: &str) -> (usize, &'a TourOfDay) {
    tours.iter().enumerate().find(|(_, t)| t.tour == no).unwrap()
}

/// The day's buses: each one's name, its trips (by departure) and where its duties begin.
fn shape(tours: &[TourOfDay]) -> Vec<(String, Vec<i32>, Vec<usize>)> {
    let mut v: Vec<(String, Vec<i32>, Vec<usize>)> = tours.iter().map(|t| (t.tour.clone(), t.trips.iter().map(|x| x.dep).collect(), network::duties_of(t).iter().map(|r| r.start).collect())).collect();
    v.sort();
    v
}

/// Monday as a scheduler left it: tour 2's last trip on a bus of its own (3), tour 1 cut at its
/// fifth trip, tour 1's first piece joined with tour 2's last as a split duty, bus 3 locked and
/// a note on the split duty.
fn monday(c: &mut Company, lines: &[LineInfo]) {
    let day = |c: &Company| tours_of_day(c, lines, MON);
    let tours = day(c);
    st::move_trip(c, 0, &tours, at(&tours, "2").0, 7, Target::New).unwrap();
    let tours = day(c);
    st::cut(c, 0, &tours, at(&tours, "1").0, 4).unwrap();
    let tours = day(c);
    st::cut(c, 0, &tours, at(&tours, "2").0, 6).unwrap();
    let tours = day(c);
    st::join(c, 0, &tours, (at(&tours, "1").0, 0), (at(&tours, "2").0, 1)).unwrap();
    rf::set_block_lock(c, 0, "Linie5", "3", true);
    let tours = day(c);
    let d = st::duties(&c.planning.joined, 0, &tours);
    let split = d.iter().find(|d| d.pieces.len() == 2).unwrap();
    let key = rf::duty_key(&tours, split).unwrap();
    rf::note_mut(c, 0, &key).kind = Some(rf::TYPE_NAMES[3].into());
}

fn day<'a>(date: &'a str, lines: &'a [LineInfo]) -> Day<'a> {
    Day { date, lines }
}

#[test]
fn a_plan_copied_to_a_day_of_the_same_timetable_is_an_exact_copy() {
    let mut c = company();
    let lines = line5();
    monday(&mut c, &lines);
    let made = apply(&mut c, day(MON, &lines), day(TUE, &lines), true, What::ALL);
    assert!(made.exact(), "{made:?}");
    assert_eq!((made.matched, made.total, made.slot), (16, 16, 1));
    // the same buses, trips and duties on Tuesday, every Tuesday
    let (mon, tue) = (tours_of_day(&c, &lines, MON), tours_of_day(&c, &lines, TUE));
    assert_eq!(shape(&mon), shape(&tue));
    assert_eq!(shape(&tours_of_day(&c, &lines, "2024-03-12")), shape(&mon));
    assert_eq!(print(&c, 0, What::ALL), print(&c, 1, What::ALL));
    // the joined duty, the lock and the note along
    assert_eq!(st::duties(&c.planning.joined, 1, &tue).iter().filter(|d| d.pieces.len() == 2).count(), 1);
    assert!(rf::block_locked(&c, 1, "Linie5", "3"));
    assert!(c.planning.flow.sheet.notes.iter().any(|n| n.weekday == 1 && n.kind.as_deref() == Some(rf::TYPE_NAMES[3])));
    // Tuesday follows Monday, in step
    let s = state(&c, 1);
    assert_eq!((s.pattern, s.own, s.unsynced, s.forked), (Some(0), false, false, false));
    assert_eq!(state(&c, 0).followers, vec![1]);
    // Wednesday did not change; vehicles alone: the buses, and the duties cut anew there
    assert_eq!(tours_of_day(&c, &lines, WED).len(), 2);
    let made = apply(&mut c, day(MON, &lines), day(WED, &lines), true, What { vehicles: true, drivers: false });
    assert!(made.exact());
    let wed = tours_of_day(&c, &lines, WED);
    assert_eq!(wed.iter().map(|t| (t.tour.clone(), t.trips.len())).collect::<Vec<_>>(), mon.iter().map(|t| (t.tour.clone(), t.trips.len())).collect::<Vec<_>>());
    assert!(wed.iter().all(|t| t.cuts.is_none()), "the duties cut automatically");
    assert!(!c.planning.joined.iter().any(|j| j.weekday == 2));
    // an old company without the days loads, and the empty days are left out of the file
    let mut json = serde_json::to_value(&company().planning).unwrap();
    assert!(json.pointer("/flow/days").is_none());
    json.as_object_mut().unwrap().remove("flow");
    let p: super::super::plan::Planning = serde_json::from_value(json).unwrap();
    assert!(p.flow.days.is_empty());
    let back: super::super::plan::Planning = serde_json::from_value(serde_json::to_value(&c.planning).unwrap()).unwrap();
    assert_eq!(back.flow.days, c.planning.flow.days);
}

#[test]
fn a_day_of_another_timetable_takes_only_the_trips_that_match_and_lists_the_rest() {
    let mut c = company();
    let lines = line5();
    monday(&mut c, &lines);
    // Tuesday's timetable: a trip more on tour 1 (fewer and extra), tour 2's fourth trip five
    // minutes later (shifted) and its sixth not at all (fewer)
    let mut t = line_of(&[("1", 360, 9), ("2", 390, 8)]);
    for x in [&mut t.tours[1].trips[3]] {
        x.departure += 300.0;
        x.arrival += 300.0;
    }
    t.tours[1].trips.remove(5);
    let other = vec![t];
    let made = apply(&mut c, day(MON, &lines), day(TUE, &other), true, What::ALL);
    assert!(!made.exact());
    assert_eq!((made.total, made.matched, made.gone), (16, 14, 2));
    // the two that match nothing are listed with the bus they run on
    let listed: Vec<(i32, &str)> = made.missing.iter().map(|m| (m.dep, m.block.as_str())).collect();
    assert_eq!(listed, vec![(575, "5"), (840, "4")]);
    assert!(made.missing.iter().all(|m| m.line == "Linie5" && m.number == "5"));
    // Monday's buses with what matches, and the rest on their tour's bus, named anew
    let tue = tours_of_day(&c, &other, TUE);
    let deps = |no: &str| at(&tue, no).1.trips.iter().map(|x| x.dep).collect::<Vec<_>>();
    assert_eq!(deps("1"), vec![360, 420, 480, 540, 600, 660, 720, 780]);
    assert_eq!(deps("2"), vec![390, 450, 510, 630, 750]);
    assert_eq!(deps("3"), vec![810]);
    assert_eq!((deps("4"), deps("5")), (vec![840], vec![575]));
    // every trip of the day runs, Monday's duty cut kept where its trip is
    assert_eq!(tue.iter().map(|t| t.trips.iter().filter(|x| x.counts()).count()).sum::<usize>(), 16);
    assert_eq!(network::duties_of(at(&tue, "1").1).iter().map(|r| r.start).collect::<Vec<_>>(), vec![0, 4]);
    // and the split duty still joined: its pieces were found
    assert_eq!(st::duties(&c.planning.joined, 1, &tue).iter().filter(|d| d.pieces.len() == 2).count(), 1);
    // remembered with the copy, for the day switcher
    let s = state(&c, 1);
    assert_eq!((s.matched, s.total, s.missing.len()), (14, 16, 2));
    // the drivers alone on a day with its own buses: Monday's duties where its trips are
    let mut d = company();
    monday(&mut d, &lines);
    let made = apply(&mut d, day(MON, &lines), day(TUE, &other), true, What { vehicles: false, drivers: true });
    assert_eq!((made.matched, made.missing.len()), (14, 2));
    let tue = tours_of_day(&d, &other, TUE);
    assert_eq!(tue.len(), 2, "Tuesday keeps its buses");
    assert_eq!(network::duties_of(at(&tue, "1").1).iter().map(|r| r.start).collect::<Vec<_>>(), vec![0, 4]);
}

#[test]
fn a_change_on_a_copied_day_is_an_override_and_reset_to_pattern_takes_the_pattern_again() {
    let mut c = company();
    let lines = line5();
    monday(&mut c, &lines);
    apply(&mut c, day(MON, &lines), day(TUE, &lines), true, What::ALL);
    // Tuesday changed: its own changes
    let tue = tours_of_day(&c, &lines, TUE);
    st::move_trip(&mut c, 1, &tue, at(&tue, "1").0, 7, Target::New).unwrap();
    let s = state(&c, 1);
    assert!(s.own && !s.unsynced);
    // reset to pattern: Monday's again
    let made = recopy(&mut c, 1, day(MON, &lines), day(TUE, &lines)).unwrap();
    assert!(made.exact());
    assert_eq!(shape(&tours_of_day(&c, &lines, TUE)), shape(&tours_of_day(&c, &lines, MON)));
    assert!(!state(&c, 1).own);
    // the pattern changed since: Tuesday is unsynced until it is refreshed
    let mon = tours_of_day(&c, &lines, MON);
    st::move_trip(&mut c, 0, &mon, at(&mon, "1").0, 7, Target::New).unwrap();
    let s = state(&c, 1);
    assert!(s.unsynced && !s.own);
    recopy(&mut c, 1, day(MON, &lines), day(TUE, &lines)).unwrap();
    assert!(!state(&c, 1).unsynced);
    assert_eq!(print(&c, 0, What::ALL), print(&c, 1, What::ALL));
    // a change saved for one date only: that date's own plan, the weekday's kept
    let mut v = rf::view(&c);
    let wed = tours_of_day(&v, &lines, WED);
    let w = slot_of(&v, WED);
    st::move_trip(&mut v, w, &wed, at(&wed, "2").0, 7, Target::New).unwrap();
    rf::keep_draft(&mut c, &v);
    let f = fork_draft(&mut c, WED);
    assert!(rf::save_draft(&mut c));
    assert!(f >= FIRST_DATE_SLOT && slot_of(&c, WED) == f);
    assert_eq!(tours_of_day(&c, &lines, WED).len(), 3);
    assert_eq!(tours_of_day(&c, &lines, "2024-03-13").len(), 2, "the next Wednesday runs the weekday's plan");
    assert!(!c.planning.layouts.iter().any(|l| l.weekday == 2));
    let s = state(&c, f);
    assert!(s.date_only && s.forked && s.own && s.pattern == Some(2));
    // reset: the date runs its weekday's plan again, its own plan gone
    assert!(unfork(&mut c, WED));
    assert_eq!(slot_of(&c, WED), 2);
    assert!(!c.planning.layouts.iter().any(|l| l.weekday == f));
    // a copy for one date only, gone once the date is past
    let made = apply(&mut c, day(MON, &lines), day(WED, &lines), false, What::ALL);
    assert!(made.exact() && made.slot >= FIRST_DATE_SLOT);
    assert_eq!(slot_of(&c, WED), made.slot);
    c.date = "2024-03-07".into();
    tidy(&mut c);
    assert_eq!(slot_of(&c, WED), 2);
    assert!(!c.planning.layouts.iter().any(|l| l.weekday == made.slot));
    assert!(follow_of(&c, made.slot).is_none());
}

#[test]
fn the_days_close_runs_the_copied_plan() {
    use super::super::market::{self, Payment};
    use super::super::staff::{applicants, hire};
    let mut c = found(&Founding { name: "Close".into(), game_mode: GameMode::Normal, date: MON.into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.planning.auto = true;
    c.cash += 5_000_000_00;
    let bus = market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    for _ in 0..3 {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    while c.staff.len() < 5 {
        for a in applicants(&c) {
            if c.staff.len() < 5 && a.licence == crate::company::model::Licence::D {
                hire(&mut c, &a).unwrap();
            }
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    c.id = id;
    for e in c.staff.iter_mut() {
        (e.sick_until, e.holiday_until) = (None, None);
    }
    let lines = line5();
    // Monday: tour 2's last trip on a bus of its own; copied to Tuesday
    let mon = tours_of_day(&c, &lines, MON);
    st::move_trip(&mut c, 0, &mon, at(&mon, "2").0, 7, Target::New).unwrap();
    apply(&mut c, day(MON, &lines), day(TUE, &lines), true, What::ALL);
    // Tuesday's close: three buses run, as the copy has them
    c.date = TUE.into();
    let tours = tours_of_day(&c, &lines, TUE);
    assert_eq!(tours.iter().map(|t| t.tour.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"]);
    let dp = super::super::plan::day_plan(&c, TUE, tours.clone(), &[], &[], false);
    assert_eq!(dp.tours.iter().filter(|t| t.bus.is_some()).count(), 3, "the day's plan has a bus for each");
    let r = super::super::day::close_day(&mut c, tours, &[]);
    assert_eq!((r.tours, r.covered, r.trips, r.dropped), (3, 3, 16, 0));
    // without the copy Tuesday runs the timetable's two tours
    let mut plain = c.clone();
    plain.date = TUE.into();
    plain.planning.layouts.retain(|l| l.weekday != 1);
    assert_eq!(tours_of_day(&plain, &lines, TUE).len(), 2);
}

#[test]
fn a_spare_line_copied_to_another_day_gets_a_number_of_its_own() {
    let mut c = company();
    let lines = line5();
    monday(&mut c, &lines);
    let id = rf::add_spare(&mut c, 0, "Spare", 5 * 60, 13 * 60);
    rf::spare_mut(&mut c, 0, id).unwrap().name = "Early".into();
    rf::set_dismissed(&mut c, 0, &format!("Some issue|spare/{id}"), true);
    apply(&mut c, day(MON, &lines), day(TUE, &lines), true, What::ALL);
    let tue: Vec<&rf::Spare> = c.planning.flow.sheet.spares.iter().filter(|s| s.weekday == 1).collect();
    assert_eq!(tue.len(), 1);
    let copy = tue[0].id;
    assert_ne!(copy, id, "two days never share a spare line's number");
    assert_eq!(tue[0].name, "Early");
    assert!(rf::dismissed(&c, 1, &format!("Some issue|spare/{copy}")), "its dismissed issues with it");
    // renamed on Tuesday: each day's spare line is called as it is there
    rf::spare_mut(&mut c, 1, copy).unwrap().name = "Late".into();
    assert!(rf::spare_label(&c, id).ends_with("Early") && rf::spare_label(&c, copy).ends_with("Late"));
    // (a plan of one date's own, begun as its weekday's: its spare lines numbered anew too)
    let f = fork_draft(&mut c, TUE);
    let mut ids: Vec<u32> = rf::draft_of(&c).sheet.spares.iter().map(|s| s.id).collect();
    assert!(rf::draft_of(&c).sheet.spares.iter().any(|s| s.weekday == f && s.name == "Late"));
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3, "Monday's, Tuesday's and the date's: {ids:?}");
}

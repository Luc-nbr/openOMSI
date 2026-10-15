use super::super::super::timeplan::{self as tp, Band};
use super::super::super::{found, Founding};
use super::*;
use crate::lines::{Direction, Leg, MapSource, MapTour, StopRef};
use crate::service::ServiceKind;

const MF: usize = 0;
const SA: usize = 1;
const SU: usize = 2;

/// A line of five stops out and back, two minutes from stop to stop, its timetable a table with
/// trips on Monday to Friday only: out at 06:00, 06:30 ... 08:00 and back ten minutes after.
fn line() -> LineDesign {
    let dir = |rev: bool| {
        let mut stops: Vec<StopRef> = (0..5).map(|i| StopRef { id: 100 + i, name: format!("Stop {i}"), ..Default::default() }).collect();
        if rev {
            stops.reverse();
        }
        Direction { stops, legs: (0..4).map(|_| Leg { ok: true, ..Default::default() }).collect(), times: vec![0.0, 2.0, 4.0, 6.0, 8.0], manual_times: true, ..Default::default() }
    };
    let mut l = LineDesign { id: 3, number: "3".into(), ai_group: "Busse".into(), directions: vec![dir(false), dir(true)], table_on: true, ..Default::default() };
    for k in 0..5 {
        let dep = 360.0 + 30.0 * k as f32;
        l.table.push(TableTrip { dir: 0, day: MF, times: (0..5).map(|s| dep + 2.0 * s as f32).collect(), tour: String::new() });
        l.table.push(TableTrip { dir: 1, day: MF, times: (0..5).map(|s| dep + 10.0 + 2.0 * s as f32).collect(), tour: String::new() });
    }
    l
}

/// The departures of day group `day`, each with its direction, in order.
fn deps(l: &LineDesign, day: usize) -> Vec<(usize, i32)> {
    let mut v: Vec<(usize, i32)> = l.table.iter().filter(|t| tp::in_day(l, t, day)).map(|t| (t.dir, t.departure().round() as i32)).collect();
    v.sort_by_key(|x| (x.1, x.0));
    v
}

#[test]
fn a_monday_to_friday_timetable_copies_to_saturday_and_to_sunday() {
    let mut l = line();
    assert!(deps(&l, SA).is_empty() && deps(&l, SU).is_empty(), "the weekend has no trips of its own yet");
    let src = Template::of_day(&l, "oo_3", MF);
    assert_eq!(src.trips.len(), 10);
    let made = copy_table(&mut l, &src, &[SA, SU], How::Replace);
    assert_eq!(made.iter().map(|m| (m.day, m.added, m.closed)).collect::<Vec<_>>(), vec![(SA, 10, false), (SU, 10, false)]);
    // the weekend runs the weekdays' trips now, and the weekdays keep theirs
    assert_eq!(deps(&l, SA), deps(&l, MF));
    assert_eq!(deps(&l, SU), deps(&l, MF));
    assert_eq!(deps(&l, MF).len(), 10);
    // (each day group's timetable shown and checked as its own)
    let [out, inb] = tp::sides(&l);
    for day in [SA, SU] {
        assert_eq!(tp::rows(&l, day, &out).len(), 5);
        assert_eq!(tp::rows(&l, day, &inb).len(), 5);
        assert!(tp::clean(&l, day));
    }
    // the game gets tours on Saturday and on Sunday: buses of those days' own
    let tours = reg::table_tours(&l);
    for day in [MF, SA, SU] {
        assert_eq!(tours.iter().filter(|t| t.day == day).flat_map(|t| t.trips.iter()).count(), 10, "day group {day}");
    }
}

#[test]
fn the_copy_takes_the_running_times_and_add_does_not_copy_a_trip_twice() {
    let mut l = line();
    // the weekdays' running times: four minutes a segment in the morning
    let mut plan = tp::plan_of(&l);
    let mut s = tp::run_set(&l, MF, 0);
    s.bands = vec![Band { from: 0.0, run: vec![2.0; 4] }, Band { from: 400.0, run: vec![4.0; 4] }];
    plan.put(s);
    l.timeplan = Some(plan);
    // Saturday has a trip of its own at 06:00 and one at 12:00
    let one = |dep: f32| TableTrip { dir: 0, day: SA, times: (0..5).map(|k| dep + 2.0 * k as f32).collect(), tour: String::new() };
    l.table.push(one(360.0));
    l.table.push(one(720.0));
    let src = Template::of_day(&l, "oo_3", MF);
    let mut add = l.clone();
    let made = copy_table(&mut add, &src, &[SA], How::Add);
    assert_eq!((made[0].added, made[0].twins, made[0].removed), (9, 1, 0));
    assert_eq!(deps(&add, SA).len(), 11, "the 12:00 trip stays beside the copies");
    // Replace: Saturday is what Monday to Friday is, its running times too
    let made = copy_table(&mut l, &src, &[SA], How::Replace);
    assert_eq!((made[0].added, made[0].removed), (10, 2));
    assert_eq!(deps(&l, SA), deps(&l, MF));
    assert_eq!(tp::run_set(&l, SA, 0).bands.len(), 2);
    // (never onto the day it is of, unless it is a template kept)
    assert!(copy_table(&mut l, &src, &[MF], How::Add).is_empty());
}

#[test]
fn a_line_that_does_not_run_on_a_day_takes_no_copy_there() {
    let mut l = line();
    l.service = ServiceKind::School;
    let src = Template::of_day(&l, "oo_3", MF);
    let made = copy_table(&mut l, &src, &[SA, SU], How::Replace);
    assert!(made.iter().all(|m| m.closed && m.added == 0));
    assert!(deps(&l, SA).is_empty());
}

#[test]
fn a_map_line_copies_as_trips_of_the_players_and_its_tours_leave_the_day_replaced() {
    let mut l = line();
    // tour 1 runs every day of the week, tour 2 on weekdays only
    l.map = Some(MapSource { ttl: "Linie3".into(), tours: vec![MapTour { number: "1".into(), ai_group: String::new(), mask: 0xff }, MapTour { number: "2".into(), ai_group: String::new(), mask: 0b1_1111 }], ..Default::default() });
    for (k, t) in l.table.iter_mut().enumerate() {
        t.tour = if k < 4 { "1".into() } else { "2".into() };
    }
    assert_eq!(deps(&l, SA).len(), 4, "tour 1 runs on Saturday too");
    let src = Template::of_day(&l, "Linie3", MF);
    let made = copy_table(&mut l, &src, &[SA], How::Replace);
    assert_eq!((made[0].added, made[0].removed), (10, 4));
    // tour 1 no longer runs on Saturday; tour 2 stays as it was; the copies are the player's
    let m = l.map.as_ref().unwrap();
    assert_eq!((m.tours[0].mask, m.tours[1].mask), (0xff & !0b10_0000, 0b1_1111));
    assert_eq!(deps(&l, SA), deps(&l, MF));
    assert!(l.table.iter().filter(|t| t.day == SA && t.tour.is_empty()).count() == 10);
    assert_eq!(deps(&l, SU).len(), 4, "Sunday keeps tour 1");
}

#[test]
fn templates_are_kept_named_renamed_and_thrown_away() {
    let mut c = found(&Founding { name: "Post".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    let l = line();
    let id = save_template(&mut c, Template::of_day(&l, "oo_3", MF), "  School   days ").unwrap();
    assert_eq!(template(&c, id).map(|t| (t.name.as_str(), t.trips.len())), Some(("School days", 10)));
    // no name, or nothing to keep: not kept
    assert!(save_template(&mut c, Template::of_day(&l, "oo_3", MF), " ").is_none());
    assert!(save_template(&mut c, Template::of_day(&l, "oo_3", SA), "Empty").is_none());
    // the same name again: in its place
    let few = Template::of_trips(&l, "oo_3", MF, &[0, 1]);
    assert_eq!(save_template(&mut c, few, "school days"), Some(id));
    assert_eq!(template(&c, id).unwrap().trips.len(), 2);
    let other = save_template(&mut c, Template::of_day(&l, "oo_3", MF), "Peak").unwrap();
    assert_ne!(other, id);
    assert_eq!(templates_of(&c, "OO_3").len(), 2);
    assert!(templates_of(&c, "oo_4").is_empty());
    // a template kept copies onto any day, the one it was taken from too
    let mut l2 = line();
    let t = template(&c, other).unwrap().clone();
    assert_eq!(copy_table(&mut l2, &t, &[MF, SA], How::Replace).len(), 2);
    assert!(rename_template(&mut c, other, "Saturday peak") && !rename_template(&mut c, other, "") && !rename_template(&mut c, 99, "x"));
    assert_eq!(template(&c, other).unwrap().name, "Saturday peak");
    move_templates(&mut c, "oo_3", "oo_30");
    assert_eq!(templates_of(&c, "oo_30").len(), 2);
    assert!(drop_template(&mut c, id) && !drop_template(&mut c, id));
    assert_eq!(templates_of(&c, "oo_30").len(), 1);
    // kept with the company, and a company of before has none
    let back: Company = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert_eq!(back.planning.flow.days.templates, c.planning.flow.days.templates);
    let mut old = serde_json::to_value(&c).unwrap();
    old["planning"]["flow"]["days"].as_object_mut().unwrap().remove("templates");
    let old: Company = serde_json::from_value(old).unwrap();
    assert!(old.planning.flow.days.templates.is_empty());
}

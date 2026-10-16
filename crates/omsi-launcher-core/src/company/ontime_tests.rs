use super::*;
use crate::company::control::Control;
use crate::company::inbox::Event;
use crate::company::lateness::{self, tests::{company, plan}, Cause};
use crate::company::model::CompanyLine;

fn calls(stops: &[(&str, f64)]) -> Vec<Call> {
    stops.iter().map(|(n, m)| Call { name: n.to_string(), at: m * 60.0 }).collect()
}

/// A bus on the line A-B-C-D with one trip leaving A at `from` (minutes), three minutes from
/// stop to stop, `dep` and `arr` seconds late.
fn bus(label: &str, from: f64, dep: f64, arr: f64) -> OtherBus {
    OtherBus { label: label.into(), trips: vec![OtherTrip { calls: calls(&[("A", from), ("B", from + 3.0), ("C", from + 6.0), ("D", from + 9.0)]), dep, arr }] }
}

#[test]
fn punctuality_is_toned_as_the_company_counts_a_trip() {
    assert_eq!(tone(-40.0), Tone::OnTime, "a minute early is within the window");
    assert_eq!(tone(-61.0), Tone::Early);
    assert_eq!(tone(30.0), Tone::OnTime);
    assert_eq!(tone(135.0), Tone::Behind);
    assert_eq!(tone(late_from() - 1.0), Tone::Behind);
    assert_eq!(tone(late_from()), Tone::Late, "late from economy::LATE_FROM minutes");
    assert_eq!((offset(135.0), offset(-40.0), offset(0.0)), ("+2:15".to_string(), "−0:40".to_string(), "+0:00".to_string()));
}

#[test]
fn the_headway_is_the_gap_to_the_bus_ahead_and_behind_on_a_frequent_line() {
    // a bus every 8 minutes: 101 at 7:52, me at 8:00, 103 at 8:08; me due at C at 8:06
    let others = [bus("101", 472.0, 0.0, 0.0), bus("103", 488.0, 0.0, 0.0)];
    let h = headway("C", Some("D"), 486.0 * 60.0, 0.0, &others).expect("a frequent line");
    assert_eq!(h.ahead, Some(Gap { bus: "101".into(), secs: 480.0 }));
    assert_eq!(h.behind, Some(Gap { bus: "103".into(), secs: 480.0 }));
    assert_eq!((h.planned, h.advice.clone()), (480.0, Advice::None));
    // the bus ahead three minutes late (half way to C: 1:30 there): 6:30 between us
    let late = [bus("101", 472.0, 180.0, 180.0), bus("103", 488.0, 0.0, 0.0)];
    let h = headway("C", Some("D"), 486.0 * 60.0, 0.0, &late).unwrap();
    assert_eq!(h.ahead.unwrap().secs, 300.0);
    // the other way (C then B) is another trip: no bus there
    assert!(headway("C", Some("B"), 486.0 * 60.0, 0.0, &others).is_none());
    // a bus every 20 minutes: no headway shown
    let rare = [bus("101", 460.0, 0.0, 0.0), bus("103", 500.0, 0.0, 0.0)];
    assert!(headway("C", Some("D"), 486.0 * 60.0, 0.0, &rare).is_none());
}

#[test]
fn bunched_buses_are_told_to_hold_to_even_the_gaps_but_not_past_the_late_line() {
    // 101 runs five minutes late: it is two minutes ahead of me, 103 eight behind
    let others = [bus("101", 472.0, 300.0, 300.0), bus("103", 488.0, 0.0, 0.0)];
    let h = headway("C", Some("D"), 486.0 * 60.0, -60.0, &others).unwrap();
    let (a, b) = (h.ahead.clone().unwrap().secs, h.behind.clone().unwrap().secs);
    assert_eq!((a, b), (120.0, 540.0));
    // half the difference, in steps of five seconds
    assert_eq!(h.advice, Advice::Hold { secs: 210.0 });
    // already four and a half minutes late: only up to a minute under the late line
    let h = headway("C", Some("D"), 486.0 * 60.0, 270.0, &[bus("101", 472.0, 600.0, 600.0), bus("103", 496.0, 0.0, 0.0)]).unwrap();
    assert_eq!(h.advice, Advice::Hold { secs: late_from() - LATE_MARGIN - 270.0 });
    // no time left to hold: close behind 101
    let h = headway("C", Some("D"), 486.0 * 60.0, late_from() - 30.0, &[bus("101", 472.0, 650.0, 650.0)]).unwrap();
    assert_eq!(h.advice, Advice::CloseBehind { bus: "101".into() });
    // no bus behind: the hold gives back the timetable's gap
    let h = headway("C", Some("D"), 486.0 * 60.0, 0.0, &[bus("101", 472.0, 300.0, 300.0)]).unwrap();
    assert_eq!(h.advice, Advice::Hold { secs: 300.0 });
}

#[test]
fn a_bus_waits_for_its_connection_at_the_most_its_hold() {
    let dep = 466.0 * 60.0;
    // the train is in at 7:44, two minutes before the bus leaves: its passengers are aboard
    assert_eq!(hold(dep, dep, 464.0 * 60.0, 120.0), Hold::NotNeeded);
    // three minutes late: the bus waits until 7:48 (a minute to change), two minutes
    assert_eq!(hold(dep, dep, 467.0 * 60.0, 180.0), Hold::Wait { until: 468.0 * 60.0, secs: 120.0 });
    // a bus a minute late itself waits a minute less
    assert_eq!(hold(dep, dep + 60.0, 467.0 * 60.0, 180.0), Hold::Wait { until: 468.0 * 60.0, secs: 60.0 });
    // later than the most it may wait: it leaves without it
    assert_eq!(hold(dep, dep, 467.5 * 60.0, 120.0), Hold::TooLate);
    // which departures a connection is for
    let k = Connection { stop: "Bahnhof".into(), what: "Train from Erfurt".into(), time: 464, max_hold: 2, onward: false };
    assert!(k.protects(dep) && k.protects(462.0 * 60.0) && !k.protects(475.0 * 60.0) && !k.protects(461.0 * 60.0));
    let on = Connection { onward: true, time: 470, ..k.clone() };
    assert!(on.protects(463.0 * 60.0) && !on.protects(471.0 * 60.0) && !on.protects(459.0 * 60.0));
    // the passengers changing to line 12 at 7:46
    assert_eq!(onward(463.0 * 60.0, 466.0 * 60.0), Onward::Made { secs: 120.0 });
    assert_eq!(onward(464.5 * 60.0, 466.0 * 60.0), Onward::Tight { secs: 30.0 });
    assert_eq!(onward(465.5 * 60.0, 466.0 * 60.0), Onward::Missed);
    // tidied as saved: no stop or no time goes, the hold within an hour, by their time
    let t = tidy(vec![Connection { stop: " Bahnhof ".into(), time: 500, max_hold: 90, ..k.clone() }, Connection { stop: String::new(), ..k.clone() }, k.clone(), Connection { time: 1500, ..k.clone() }]);
    assert_eq!(t.iter().map(|x| (x.stop.as_str(), x.time, x.max_hold)).collect::<Vec<_>>(), vec![("Bahnhof", 464, 2), ("Bahnhof", 500, 60)]);
}

#[test]
fn the_day_model_holds_a_bus_for_its_connection_where_its_trip_begins() {
    let (mut c, bus, driver) = company();
    let date = c.date.clone();
    // trips from A at 10:00 and 11:00 (50 minutes, 10 minutes' layover), nothing on the way
    let p = plan(600, 2, 10, bus, driver);
    let none: &[&Control] = &[];
    let zero = |_: usize, _: usize| 0.0;
    let before = lateness::model_with(&c, &p, &date, &[], none, None, Some(&zero));
    assert!(before[0].iter().all(|x| x.as_ref().unwrap().dep == 0.0));
    // a train due at A two minutes after the 11:00 leaves, waited for ten minutes at the most
    let k = Connection { stop: "A".into(), what: "Train from Erfurt".into(), time: 662, max_hold: 10, onward: false };
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), connections: vec![k.clone()], ..Default::default() });
    let late = late_of(&c, &date, "Linie5", &k);
    assert_eq!(late, late_of(&c, &date, "Linie5", &k), "the same every time");
    assert!((0..=8).contains(&late));
    let m = lateness::model_with(&c, &p, &date, &[], none, None, Some(&zero));
    let t = m[0][1].as_ref().unwrap();
    // (in two minutes after the bus's time, late itself, and a minute to change)
    let wait = 2 + late + 1;
    assert!((t.dep - wait as f64).abs() < 1e-9, "{t:?}");
    assert_eq!(t.causes, vec![Cause::Hold { stop: "A".into(), what: "Train from Erfurt".into(), minutes: wait }]);
    assert_eq!(t.root(), Some("hold"));
    // later than its hold: not waited for
    c.lines[0].connections[0].max_hold = 1 + late;
    let m = lateness::model_with(&c, &p, &date, &[], none, None, Some(&zero));
    assert_eq!(m[0][1].as_ref().unwrap().dep, 0.0);
    c.lines[0].connections[0].max_hold = 10;
    let m = lateness::model_with(&c, &p, &date, &[], none, None, Some(&zero));
    // the trip at 10:00 is not for it; nor is a connection the passengers change to
    assert_eq!(m[0][0].as_ref().unwrap().dep, 0.0);
    c.lines[0].connections[0].onward = true;
    let m = lateness::model_with(&c, &p, &date, &[], none, None, Some(&zero));
    assert_eq!(m[0][1].as_ref().unwrap().dep, 0.0);
}

#[test]
fn a_diversion_of_the_day_closes_its_stops_and_slows_its_line() {
    let (mut c, _, _) = company();
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), ..Default::default() });
    let today = c.date.clone();
    let plain = crate::company::inbox::line_effect(&c, &today, "5").0;
    // what is refused
    assert!(add_diversion(&mut c, Diversion { from: "soon".into(), closed: vec!["Rathaus".into()], ..Default::default() }).is_err());
    assert!(add_diversion(&mut c, Diversion { from: today.clone(), ..Default::default() }).is_err(), "nothing closed, no way round");
    assert!(add_diversion(&mut c, Diversion { from: today.clone(), until: "2024-03-01".into(), via: "x".into(), ..Default::default() }).is_err());
    assert!(add_diversion(&mut c, Diversion { from: today.clone(), lines: vec!["Linie9".into()], via: "x".into(), ..Default::default() }).is_err());
    // the Rathaus closed on line 5 for three days, round by the Bahnhofstraße
    let id = add_diversion(&mut c, Diversion { from: today.clone(), until: "2024-03-06".into(), lines: vec!["Linie5".into()], closed: vec![" Rathaus ".into(), String::new()], via: "Bahnhofstraße".into(), ..Default::default() }).unwrap();
    let d = &c.ontime.diversions[0];
    assert_eq!((d.id, d.closed.clone()), (id, vec!["Rathaus".to_string()]));
    assert!(d.on("2024-03-05") && !d.on("2024-03-07") && d.on_line("linie5") && !d.on_line("Linie7") && d.closes("rathaus"));
    let list = disruptions(&c, &today, "Linie5");
    assert_eq!((list.len(), list[0].id.as_str(), list[0].via.as_str()), (1, "div-1", "Bahnhofstraße"));
    assert!(disruptions(&c, &today, "Linie7").is_empty());
    assert!(disruptions(&c, "2024-03-07", "Linie5").is_empty());
    // its line runs later, the other not
    assert!((crate::company::inbox::line_effect(&c, &today, "5").0 - plain - DIVERSION_LATE).abs() < 1e-9);
    assert_eq!(diversion_late(&c, &today, "7"), 0.0);
    // the inbox's roadworks on line 7 are told too (no stops known)
    c.inbox.events.push(Event { id: 17, key: "roadworks".into(), from: today.clone(), until: today.clone(), fuel: 1.0, demand: 1.0, late: 0.1, line: Some("7".into()), started: true, ended: false });
    let seven = disruptions(&c, &today, "Linie7");
    assert_eq!((seven[0].id.as_str(), seven[0].kind.as_str(), seven[0].closed.len()), ("event-17", "roadworks", 0));
    // the driver is told up to three stops before
    let trip = calls(&[("A", 0.0), ("B", 2.0), ("C", 4.0), ("D", 6.0), ("Rathaus", 8.0), ("E", 10.0)]);
    let closed = closed_of(&list);
    assert_eq!(closed_ahead(&trip, 0, &closed), None);
    assert_eq!(closed_ahead(&trip, 1, &closed), Some(4));
    assert_eq!(closed_ahead(&trip, 4, &closed), Some(4));
    assert_eq!(closed_ahead(&trip, 5, &closed), None);
    // taken away again; one long over goes when another comes
    assert!(remove_diversion(&mut c, id) && !remove_diversion(&mut c, id));
    c.ontime.diversions.push(Diversion { id: 99, from: "2024-01-01".into(), via: "x".into(), ..Default::default() });
    add_diversion(&mut c, Diversion { from: today.clone(), via: "y".into(), ..Default::default() }).unwrap();
    assert_eq!(c.ontime.diversions.iter().map(|d| d.via.as_str()).collect::<Vec<_>>(), vec!["y"]);
    // (a company saved before has none)
    let v = serde_json::to_value(&c).unwrap();
    let mut old = v.clone();
    old.as_object_mut().unwrap().remove("ontime");
    let back: Company = serde_json::from_value(old).unwrap();
    assert!(back.ontime.diversions.is_empty());
}

use super::*;
use crate::company::clock::DayScript;
use crate::company::lateness::tests::{company, plan};
use crate::company::plan::{DayDuty, DayTour, Who};

/// A company with the clock in its day at 06:30, one bus and one driver on a block of four trips
/// an hour apart from 06:00 (each 50 minutes, 10 minutes' layover), as the day's plan has it.
fn day() -> (Company, DayPlan, u32, u32) {
    let (mut c, bus, driver) = company();
    c.clock.today = Some(DayScript { date: c.date.clone(), ..Default::default() });
    c.clock.minute = 6 * 60 + 30;
    let p = plan(6 * 60, 4, 10, bus, driver);
    let t = p.tours[0].tour.clone();
    let duty = DayDuty { start: 0, end: 4, from: t.from(), to: t.to(), who: Some(Who::Staff(driver)), ..Default::default() };
    let dp = DayPlan { date: c.date.clone(), today: true, tours: vec![DayTour { tour: t, bus: Some(BusOf::Own(bus)), duties: vec![duty], ..Default::default() }], ..Default::default() };
    (c, dp, bus, driver)
}

fn duty() -> To {
    To::Duty { line: "Linie5".into(), tour: "1".into() }
}

#[test]
fn the_queue_keeps_its_messages_in_order_and_bounded() {
    let (mut c, _, _, _) = day();
    let a = post(&mut c, duty(), Priority::Info, LATE, vec![ck::arg("min", 6)], None);
    let b = send(&mut c, From::Manager { name: "Ann".into() }, To::Line { line: "Linie5".into() }, Priority::Urgent, "  Watch\nout\tat the market ", Vec::new(), true, None);
    assert_eq!((a, b), (1, 2));
    let m = c.messages.get(b).unwrap();
    // (free text is one clean line; the sender kept by name)
    assert_eq!((m.text.as_str(), m.free, m.state), ("Watch out at the market", true, State::Sent));
    assert_eq!(m.from, From::Manager { name: "Ann".into() });
    assert_eq!(m.at, ck::now(&c));
    // an action makes an instruction at least
    let x = Action { line: "Linie5".into(), tour: "1".into(), dep: 7 * 60, what: What::Hold { minutes: 3, connection: true } };
    let i = post(&mut c, duty(), Priority::Info, HOLD_CONNECTION, vec![ck::arg("min", 3)], Some(x));
    assert_eq!(c.messages.get(i).unwrap().priority, Priority::Instruction);
    // the driver of the tour sees his own, his line's and everyone's - not another tour's
    post(&mut c, To::Duty { line: "Linie5".into(), tour: "2".into() }, Priority::Info, THANKS, Vec::new(), None);
    post(&mut c, To::All, Priority::Info, DIVERSION, Vec::new(), None);
    let mine: Vec<u64> = c.messages.of_driver("Linie5", "1", None, 10).iter().map(|m| m.id).collect();
    assert_eq!(mine, vec![5, 3, 2, 1]);
    // bounded: the oldest go first, an instruction still open stays
    for _ in 0..KEPT + 20 {
        post(&mut c, To::All, Priority::Info, THANKS, Vec::new(), None);
    }
    assert_eq!(c.messages.list.len(), KEPT);
    assert!(c.messages.get(i).is_some() && c.messages.get(a).is_none());
    assert!(c.messages.list.windows(2).all(|w| w[0].id < w[1].id));
    // what is kept reads back, and a company of before has none
    let back: Messages = serde_json::from_str(&serde_json::to_string(&c.messages).unwrap()).unwrap();
    assert_eq!(back, c.messages);
    let old: Messages = serde_json::from_str("{}").unwrap();
    assert!(old.list.is_empty() && old.driving.is_none());
}

#[test]
fn the_dispatcher_says_each_thing_once_and_never_too_often() {
    let (mut c, _, _, _) = day();
    let now = ck::now(&c);
    let m = &mut c.messages;
    assert!(m.may_say("late|a", "Linie5|1", now));
    // the same again: no
    assert!(!m.may_say("late|a", "Linie5|1", now + 60));
    // something else to the same driver: not within the gap
    assert!(!m.may_say("late|b", "Linie5|1", now + GAP - 1));
    assert!(m.may_say("late|b", "Linie5|1", now + GAP));
    // another driver: at once
    assert!(m.may_say("late|c", "Linie5|2", now + GAP));
    // a driver's device's lines count once
    assert!(m.first_seen("g1-7") && !m.first_seen("g1-7") && m.first_seen(""));
}

fn driving(trip: u32, late: f64) -> Driving {
    let dep = 6 * 60 + (trip as i32 - 1) * 60;
    Driving { line: "Linie5".into(), tour: "1".into(), trip, dep, arr: dep + 50, next_dep: Some(dep + 60), next_arr: Some(dep + 110), counts: true, late, left: 0.5, vehicle: None }
}

#[test]
fn the_dispatcher_tells_the_player_running_late_and_a_way_back_without_spam() {
    let (mut c, _, _, _) = day();
    c.game_mode = GameMode::Normal;
    c.messages.driving = Some(("Linie5".into(), "1".into()));
    // on time: nothing
    assert!(dispatch(&mut c, &driving(1, 2.0)).is_empty());
    // six minutes late: told, once
    let told = dispatch(&mut c, &driving(1, 6.0));
    assert_eq!(told.len(), 1);
    let m = c.messages.get(told[0]).unwrap().clone();
    assert_eq!((m.text.as_str(), m.args.clone(), m.from.clone(), m.to.clone()), (LATE, vec![ck::arg("min", 6)], From::Dispatcher, duty()));
    assert!(dispatch(&mut c, &driving(1, 7.0)).is_empty());
    c.clock.minute += GAP as u32;
    assert!(dispatch(&mut c, &driving(1, 7.0)).is_empty(), "not grown enough");
    // grown by five minutes: told again, after the gap
    let again = dispatch(&mut c, &driving(1, 10.0));
    assert!(again.len() == 1 && c.messages.get(again[0]).unwrap().args == vec![ck::arg("min", 10)]);
    // the layover (10 min, 3 kept) cannot take 14 minutes up: a short-turn, to accept or decline
    c.clock.minute += GAP as u32;
    let advice = dispatch(&mut c, &driving(1, 14.0));
    let a = c.messages.get(advice[0]).unwrap();
    assert_eq!((a.text.as_str(), a.priority), (SHORT_TURN, Priority::Instruction));
    let x = a.action.clone().unwrap();
    assert!(matches!(x.what, What::ShortTurn { part, .. } if (0.1..=0.5).contains(&part)));
    assert_eq!((x.line.as_str(), x.tour.as_str(), x.dep), ("Linie5", "1", 6 * 60));
    // and only once a trip
    c.clock.minute += GAP as u32;
    let later = dispatch(&mut c, &driving(1, 15.0));
    assert!(later.iter().all(|id| c.messages.get(*id).unwrap().action.is_none()));
    // back on time on the next trip after being told on it
    c.clock.minute += GAP as u32;
    dispatch(&mut c, &driving(2, 6.0));
    c.clock.minute += GAP as u32;
    let back = dispatch(&mut c, &driving(2, 1.0));
    assert_eq!(c.messages.get(back[0]).unwrap().text, BACK_ON_TIME);
    // little of the trip left: out of service to the terminus
    c.clock.minute += GAP as u32;
    let mut d = driving(3, 14.0);
    d.left = 0.07;
    let x = dispatch(&mut c, &d);
    assert!(matches!(c.messages.get(x[0]).unwrap().action.as_ref().unwrap().what, What::OutOfService { .. }));
    // "Never" told: nothing of lateness
    c.clock.late_notice = Some(0);
    c.clock.minute += GAP as u32;
    assert!(dispatch(&mut c, &driving(4, 20.0)).is_empty());
}

#[test]
fn casual_stays_calm() {
    let (mut c, _, _, _) = day();
    c.game_mode = GameMode::Casual;
    assert!(dispatch(&mut c, &driving(1, 25.0)).is_empty());
    assert!(tell_late(&mut c, "Linie5", "1", 25).is_none());
    c.game_mode = GameMode::Realistic;
    assert!(dispatch(&mut c, &driving(1, 25.0)).len() == 1);
    // (an AI driver told once a day per tour)
    assert!(tell_late(&mut c, "Linie5", "2", 8).is_some());
    c.clock.minute += 60;
    assert!(tell_late(&mut c, "Linie5", "2", 12).is_none());
}

#[test]
fn the_spare_bus_coming_is_told_to_the_driver_of_the_broken_bus() {
    let (mut c, _, bus, _) = day();
    let s = c.clock.today.as_mut().unwrap();
    s.breaks.push(Broken { vehicle: bus, bus: "1".into(), line: "5".into(), at: 6 * 60 + 20, until: 10 * 60, line_name: "Linie5".into(), choice: Some(ck::Choice::Spare { vehicle: 77 }), cost: 0, accident: false, back: Some(6 * 60 + 50), spare_until: None });
    let mut d = driving(1, 0.0);
    d.vehicle = Some(bus);
    let x = dispatch(&mut c, &d);
    let m = c.messages.get(x[0]).unwrap();
    assert_eq!((m.text.as_str(), m.priority), (SPARE_COMING, Priority::Instruction));
    assert!(m.args.contains(&ck::arg("time", "06:50")));
    c.clock.minute += 30;
    assert!(dispatch(&mut c, &d).is_empty());
}

#[test]
fn an_accepted_way_out_is_applied_as_service_control_applies_it() {
    let (mut c, dp, _, _) = day();
    let t = dp.tours[0].tour.clone();
    let x = Action::of(&t.line, &t.tour, t.trips[0].dep, Act::ShortTurn { part: 0.2, saved: 20 }).unwrap();
    let id = post(&mut c, duty(), Priority::Instruction, SHORT_TURN, Vec::new(), Some(x));
    c.messages.driving = Some(("Linie5".into(), "1".into()));
    // the player's: no AI answers it
    assert!(ai_answers(&mut c, &dp, &mut Run::default()).is_empty());
    assert_eq!(answer(&mut c, id, Answer::Delivered, None), Ok(State::Delivered));
    // (another tour's driver cannot answer it)
    assert_eq!(answer(&mut c, id, Answer::Accept, Some(("Linie5", "2"))), Err(NOT_YOURS));
    assert_eq!(answer(&mut c, id, Answer::Accept, Some(("Linie5", "1"))), Ok(State::Accepted));
    // carried out once, with the day: the controls the delay model reads
    let mut run = Run::default();
    assert_eq!(carry_out(&mut c, &dp, &mut run), vec![id]);
    assert!(carry_out(&mut c, &dp, &mut run).is_empty());
    let done = ctl::of_day(&c, &c.date.clone()).into_iter().cloned().collect::<Vec<_>>();
    assert_eq!(done.len(), 1);
    assert!(matches!(done[0].act, Act::ShortTurn { .. }) && !done[0].auto);
    let m = c.messages.get(id).unwrap();
    assert!(m.applied && m.outcome.is_empty());
    assert!(run.told > 0);
    // answered stays answered
    assert_eq!(answer(&mut c, id, Answer::Decline, None), Err(ANSWERED));
    assert_eq!(answer(&mut c, id, Answer::Read, None), Ok(State::Accepted));
    assert_eq!(answer(&mut c, 999, Answer::Read, None), Err(NO_MESSAGE));
}

#[test]
fn a_declined_instruction_is_seen_by_control_and_not_applied() {
    let (mut c, dp, _, _) = day();
    c.messages.driving = Some(("Linie5".into(), "1".into()));
    let x = Action { line: "Linie5".into(), tour: "1".into(), dep: 7 * 60, what: What::OutOfService { part: 0.5, saved: 6 } };
    let id = post(&mut c, duty(), Priority::Instruction, OUT_OF_SERVICE, Vec::new(), Some(x));
    let before = c.clock.feed.len();
    assert_eq!(answer(&mut c, id, Answer::Decline, None), Ok(State::Declined));
    assert_eq!(c.clock.feed.len(), before + 1);
    assert_eq!(c.clock.feed.last().unwrap().text, DECLINED_TOLD);
    assert!(carry_out(&mut c, &dp, &mut Run::default()).is_empty());
    assert!(ctl::of_day(&c, &c.date.clone()).is_empty());
    // a canned reply to an info message reads it
    let i = post(&mut c, duty(), Priority::Info, DIVERSION, Vec::new(), None);
    assert_eq!(answer(&mut c, i, Answer::Reply(Canned::Understood), None), Ok(State::Read));
    assert_eq!(c.messages.get(i).unwrap().reply, "understood");
    assert_eq!(Answer::of("on_my_way"), Some(Answer::Reply(Canned::OnMyWay)));
    assert_eq!(Answer::of("rm -rf"), None);
    // (as the devices send them: a key)
    assert_eq!(serde_json::to_string(&Answer::Reply(Canned::CallMe)).unwrap(), "\"call_me\"");
    assert_eq!(serde_json::from_str::<Answer>("\"decline\"").unwrap(), Answer::Decline);
    assert!(serde_json::from_str::<Answer>("\"x\"").is_err());
}

#[test]
fn an_ai_driver_answers_at_once_and_does_what_he_can() {
    let (mut c, dp, bus, driver) = day();
    // an info read
    let i = post(&mut c, To::Driver { employee: driver }, Priority::Info, MAKE_UP, Vec::new(), None);
    // a hold accepted and told; a swap with a bus that is not there declined
    let h = post(&mut c, duty(), Priority::Instruction, HOLD_HEADWAY, Vec::new(), Some(Action { line: "Linie5".into(), tour: "1".into(), dep: 7 * 60, what: What::Hold { minutes: 2, connection: false } }));
    let s = post(&mut c, duty(), Priority::Instruction, SWAP, Vec::new(), Some(Action { line: "Linie5".into(), tour: "9".into(), dep: 7 * 60, what: What::Swap { vehicle: 99, from_bus: bus } }));
    // (a message to control is no driver's)
    let q = send(&mut c, From::Driver { line: "Linie5".into(), tour: "1".into(), name: String::new() }, To::Control, Priority::Info, Quick::RequestTalk.label(), Vec::new(), false, None);
    let mut run = Run::default();
    let answered = ai_answers(&mut c, &dp, &mut run);
    assert_eq!(answered, vec![i, h, s]);
    assert_eq!(c.messages.get(i).unwrap().state, State::Read);
    let held = c.messages.get(h).unwrap();
    assert!(held.state == State::Accepted && held.applied && held.outcome.is_empty());
    assert!(c.clock.feed.iter().any(|f| f.text == HOLD_TOLD));
    let swap = c.messages.get(s).unwrap();
    assert_eq!((swap.state, swap.outcome.as_str()), (State::Declined, NOT_POSSIBLE));
    assert_eq!(c.messages.get(q).unwrap().state, State::Sent);
    // nothing twice
    assert!(ai_answers(&mut c, &dp, &mut run).is_empty());
}

#[test]
fn a_driver_s_breakdown_is_the_day_s_incident() {
    let (mut c, dp, bus, _) = day();
    let mut run = Run::default();
    let id = driver_says(&mut c, "Linie5", "1", "Luc", Quick::Breakdown, Some(&dp), &mut run);
    assert!(c.clock.today.as_ref().unwrap().breaks.is_empty());
    assert_eq!(carry_out(&mut c, &dp, &mut run), Vec::<u64>::new());
    let m = c.messages.get(id).unwrap();
    assert_eq!((m.to.clone(), m.priority, m.quick, m.text.as_str()), (To::Control, Priority::Urgent, Some(Quick::Breakdown), "Breakdown"));
    let b = &c.clock.today.as_ref().unwrap().breaks;
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].vehicle, b[0].at, b[0].choice, b[0].accident), (bus, 6 * 60 + 30, None, false));
    // told once: a second button press is a message, not a second incident
    driver_says(&mut c, "Linie5", "1", "Luc", Quick::Accident, Some(&dp), &mut run);
    carry_out(&mut c, &dp, &mut run);
    assert_eq!(c.clock.today.as_ref().unwrap().breaks.len(), 1);
    // the rest are messages only
    driver_says(&mut c, "Linie5", "1", "Luc", Quick::DelayTraffic, Some(&dp), &mut run);
    carry_out(&mut c, &dp, &mut run);
    assert_eq!(c.clock.today.as_ref().unwrap().breaks.len(), 1);
    assert!(carry_out(&mut c, &dp, &mut run).is_empty());
    // control sees to it: done, and the driver told
    acknowledge(&mut c, id, From::Control, false).unwrap();
    assert!(c.messages.get(id).unwrap().done);
    let reply = c.messages.list.last().unwrap();
    assert_eq!((reply.text.as_str(), reply.re, reply.to.clone()), (ACKNOWLEDGED, Some(id), duty()));
    assert_eq!(Quick::of("road_closed"), Some(Quick::RoadClosed));
}

#[test]
fn a_player_of_play_together_answers_for_himself() {
    use crate::company::duty_launch::DutyRef;
    use crate::company::together::book::{Claim, ClaimState};
    let (mut c, dp, _, _) = day();
    let r = DutyRef { date: c.date.clone(), line: "Linie5".into(), tour: "1".into(), duty: 0 };
    c.together.claims.push(Claim { duty: r, guest: 1, state: ClaimState::Driving, ..Default::default() });
    assert!(human(&c, "linie5", "1") && !human(&c, "Linie5", "2"));
    let id = post(&mut c, To::Line { line: "Linie5".into() }, Priority::Info, ROAD_CLOSED, Vec::new(), None);
    assert!(ai_answers(&mut c, &dp, &mut Run::default()).is_empty());
    assert_eq!(c.messages.get(id).unwrap().state, State::Sent);
}

#[test]
fn every_text_is_unique_and_its_places_are_named() {
    let t = texts();
    let mut sorted = t.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), t.len());
    for x in TEMPLATES {
        assert!(t.contains(x));
    }
}

//! A boss's company server and a friend's launcher over the loopback: joining, the day, taking a
//! duty and being refused one, starting it in his own game (no world of the boss's), reporting
//! its trips, and the day's close booking them - that duty alone, the rest of its tour as
//! planned; a bad code. The roles and their permissions, checked on the boss's side; asking for
//! a duty and its approval; a duty whose day closed before it was driven. No game is started.

use super::super::duty_launch::DutyRef;
use super::super::market::{self, MarketBus, Payment};
use super::super::model::{Company, CompanyLine, GameMode};
use super::super::network::{PlannedTrip, TourOfDay};
use super::super::plan::{day_plan, DayPlan, Who};
use super::super::{found, Founding};
use super::book::{self, ClaimState};
use super::client;
use super::code::{self, CompanyCode};
use super::host::{self, Here, HostState};
use super::roles::{Perm, Role};
use super::server::Server;
use super::wire::{DriveStart, DutyState, Request, Snapshot};
use crate::{TripInfo, TripRun};
use serde_json::Value;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

fn trip(name: &str, dep: i32, arr: i32, from: &str, to: &str, empty: bool) -> PlannedTrip {
    PlannedTrip { name: name.into(), line: if empty { String::new() } else { "5".into() }, from: from.into(), to: to.into(), dep, arr, km: 8.0, stops: if empty { 0 } else { 10 }, empty, origin: String::new(), last: String::new(), part: None }
}

/// A company with one bus and line 5, whose tour 2 is a depot run, three trips and the run back,
/// with its bus and nobody on its duty (open).
fn setup() -> (Company, Vec<TourOfDay>) {
    let mut c = found(&Founding { name: "Stadtbus".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.depot = "Grundorf.hof".into();
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.cash += 5_000_000_00;
    market::buy_new(&mut c, &MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
    let tours = vec![TourOfDay {
        line: "Linie5".into(),
        number: "5".into(),
        tour: "2".into(),
        ai_group: String::new(),
        trips: vec![trip("Depot-A", 350, 358, "Depot", "A", true), trip("5 A-B", 360, 390, "A", "B", false), trip("5 B-A", 400, 430, "B", "A", false), trip("5 A-B", 440, 470, "A", "B", false), trip("A-Depot", 472, 480, "A", "Depot", true)],
        unplanned: false,
        cuts: None,
    }];
    let wd = super::super::plan::weekday_of(&c.date);
    let id = c.fleet[0].id;
    super::super::plan::set_bus(&mut c, wd, "Linie5", "2", Some(id));
    c.planning.auto = false;
    c.clock.minute = 300;
    (c, tours)
}

fn info(name: &str, index: usize, dep: i32) -> TripInfo {
    TripInfo { name: name.into(), index, line: "5".into(), from: "A".into(), terminus: "B".into(), departure: dep as f64 * 60.0, arrival: (dep + 30) as f64 * 60.0, stops: Vec::new(), km: 8.0 }
}

fn timetable() -> Vec<crate::LineInfo> {
    vec![crate::LineInfo {
        name: "Linie5".into(),
        user_allowed: true,
        termini: Vec::new(),
        tours: vec![crate::TourInfo { number: "2".into(), ai_group: String::new(), first: 360.0 * 60.0, last: 470.0 * 60.0, days: "daily".into(), runs: true, next_run: None, trips: vec![info("5 A-B", 1, 360), info("5 B-A", 2, 400), info("5 A-B", 3, 440)] }],
    }]
}

/// The boss's side: his company, its tours, the session's state.
struct Boss {
    c: Company,
    tours: Vec<TourOfDay>,
    st: HostState,
}

impl Boss {
    fn plan(&self) -> DayPlan {
        day_plan(&self.c, &self.c.date, self.tours.clone(), &[], &[], false)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A server answering as the boss's launcher does (the plan made again for every message).
fn serve(boss: Arc<Mutex<Boss>>) -> Server {
    let lines = timetable();
    Server::start(0, true, Arc::new(move |body: &str| {
        let mut b = boss.lock().unwrap();
        let p = b.plan();
        let Boss { c, st, .. } = &mut *b;
        let here = Here { plan: Some(&p), lines: &lines, now: now(), lead: 10 };
        st.answer(c, &here, body).body.to_string()
    }))
    .unwrap()
}

fn req(code: &CompanyCode, player: &str, name: &str, what: &str, duty: Option<&DutyRef>) -> Request {
    Request { v: code::VERSION, secret: code.secret_hex(), company: code.company, player: player.into(), name: name.into(), what: what.into(), duty: duty.cloned(), ..Default::default() }
}

fn snap(v: &Value) -> Snapshot {
    serde_json::from_value(v["snapshot"].clone()).unwrap()
}

#[test]
fn friends_join_take_duties_drive_them_and_the_day_books_their_trips() {
    let (c, tours) = setup();
    let secret = [9, 8, 7, 6, 5, 4, 3, 2];
    let boss = Arc::new(Mutex::new(Boss { st: HostState::new(&c.id, "Luc", secret), c, tours }));
    let server = serve(boss.clone());
    let id = boss.lock().unwrap().c.id.clone();
    let code = CompanyCode { secret, session: 1, version: code::VERSION, port: server.port(), company: code::tag_of(&id), ips: vec![Ipv4Addr::LOCALHOST] };
    // (the code as the boss copies it, read back by the friend)
    let code = CompanyCode::decode(&code.encode()).unwrap();

    // joining: the boss's launcher is found at the code's address, the company's day comes
    let (base, hello) = client::connect(&code, &req(&code, "jan-1", "Jan", "hello", None)).unwrap();
    assert_eq!(hello["ok"], true, "{hello}");
    let s = snap(&hello);
    assert_eq!((s.card.name.as_str(), s.card.boss.as_str(), s.date.as_str()), ("Stadtbus", "Luc", "2024-03-04"));
    assert_eq!(s.duties.len(), 1);
    assert_eq!(s.duties[0].state, DutyState::Open);
    assert_eq!((s.duties[0].from, s.duties[0].to, s.duties[0].trips), (350, 480, 3));
    assert!(s.players.iter().any(|p| p.name == "Jan" && p.me && p.online));
    assert!(s.notices.iter().any(|n| n.text == host::JOINED));
    let duty = s.duties[0].duty.clone();

    // taking the open duty: his at once, and the plan has him on it
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "take", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(snap(&v).duties[0].state, DutyState::Mine);
    let g = boss.lock().unwrap().c.together.claims[0].guest;
    assert_eq!(boss.lock().unwrap().plan().tours[0].duties[0].who, Some(Who::Guest(g)));

    // another friend is refused it, and sees it as Jan's
    let v = client::send(&base, &req(&code, "piet-2", "Piet", "take", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!((v["ok"].as_bool(), v["error"].as_str()), (Some(false), Some(book::NOT_OPEN)));
    let s = snap(&v);
    assert_eq!((s.duties[0].state, s.duties[0].who.as_str()), (DutyState::Player, "Jan"));
    // nor may he give it back for Jan
    let v = client::send(&base, &req(&code, "piet-2", "Piet", "release", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["error"].as_str(), Some(book::NOT_YOURS));

    // driving it: in his own game, alone, at once - the boss opens no world
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "start", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let start: DriveStart = serde_json::from_value(v["start"].clone()).unwrap();
    assert_eq!((start.line.as_str(), start.tour.as_str(), start.first, start.date.as_str()), ("Linie5", "2", 1, "2024-03-04"));
    assert_eq!(start.legs, vec!["Linie5|2|1|3".to_string()]);
    assert_eq!(start.bus, "Vehicles/Citaro/Citaro.bus");
    let d = start.duty(crate::Duty { traffic: Some(40), ..Default::default() });
    assert_eq!((d.lan.as_deref(), d.lan_name.as_deref(), d.company.as_deref(), d.traffic), (Some("off"), None, None, Some(40)));
    assert_eq!(snap(&v).duties[0].state, DutyState::Driving);

    // the trips his game judged: those of the duty are taken (once), another is refused
    let run = |dep: i32, passengers: i32| TripRun { time: 1, driver: "Jan".into(), map: "maps/Grundorf/global.cfg".into(), line: "5".into(), tour: "2".into(), departure: dep as f64 * 60.0, arrival: (dep + 30) as f64 * 60.0, completed: true, planned: 10, stops: 10, metres: 8000.0, seconds: 1800.0, passengers, ..Default::default() };
    let mut r = req(&code, "jan-1", "Jan", "report", Some(&duty));
    r.trips = vec![run(360, 20), run(400, 9999), run(1000, 5)];
    let v = client::send(&base, &r, client::TIMEOUT).unwrap();
    assert_eq!((v["taken"].as_u64(), v["refused"].as_u64()), (Some(2), Some(1)), "{v}");
    r.trips = vec![run(400, 12), run(440, 15)];
    r.done = true;
    let v = client::send(&base, &r, client::TIMEOUT).unwrap();
    assert_eq!((v["taken"].as_u64(), v["refused"].as_u64()), (Some(1), Some(1)), "{v}");
    let s = snap(&v);
    assert_eq!(s.duties[0].state, DutyState::Done);
    assert_eq!(s.results.len(), 1);
    // (passengers no more than a bus can have carried)
    assert_eq!((s.results[0].trips, s.results[0].passengers), (3, 20 + 250 + 15));
    assert!(s.notices.iter().any(|n| n.text == host::FINISHED));
    // his own figures: the trips, the duty, what they brought the company
    let you = s.you.clone().unwrap();
    assert_eq!((you.role, you.perms.clone(), you.stats.trips, you.stats.duties, you.stats.passengers), (Role::Driver, vec![Perm::Drive], 3, 1, 285));
    assert!(you.stats.fares > 0 && you.stats.fares == s.results[0].fares);

    // the day's close books his trips as the boss's own: measured, the tour run
    let mut b = boss.lock().unwrap();
    let tours = b.tours.clone();
    let c = &mut b.c;
    let trips = book::trips_for_close(c);
    assert_eq!(trips.len(), 3);
    assert!(trips.iter().all(|t| t.driver == "Jan" && t.map == c.map && t.bus == "Vehicles/Citaro/Citaro.bus" && t.time > c.trips_seen));
    let cash = c.cash;
    let closed = c.date.clone();
    let report = super::super::day::close_day_with(c, tours, &[], &trips);
    // (his duty alone is his: the tour is not the boss's own)
    assert_eq!((report.measured, report.by_player, report.covered, report.tours), (3, 0, 1, 1));
    assert!(c.ledger.iter().any(|b| b.date == closed && b.measured && b.text.contains("own trip")));
    assert_ne!(c.cash, cash);
    book::after_close(c, &closed);
    assert!(c.together.claims.is_empty() && c.together.trips.is_empty());
    drop(b);
    drop(server);
}

#[test]
fn claims_wait_for_the_boss_when_he_says_so_and_a_bad_code_gets_nowhere() {
    let (mut c, tours) = setup();
    c.together.open_claims = false;
    let secret = [1, 1, 2, 3, 5, 8, 13, 21];
    let boss = Arc::new(Mutex::new(Boss { st: HostState::new(&c.id, "Luc", secret), c, tours }));
    let server = serve(boss.clone());
    let id = boss.lock().unwrap().c.id.clone();
    let code = CompanyCode { secret, session: 1, version: code::VERSION, port: server.port(), company: code::tag_of(&id), ips: vec![Ipv4Addr::LOCALHOST] };
    let (base, hello) = client::connect(&code, &req(&code, "jan-1", "Jan", "hello", None)).unwrap();
    let duty = snap(&hello).duties[0].duty.clone();
    // asked for: not his yet, nobody planned on it
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "take", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!((v["ok"].as_bool(), v["state"].as_str()), (Some(true), Some("asked")), "{v}");
    assert_eq!(snap(&v).duties[0].state, DutyState::Asked);
    assert_eq!(boss.lock().unwrap().plan().tours[0].duties[0].who, None);
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "start", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["error"].as_str(), Some(book::NOT_YOURS));
    // the boss says yes: his
    {
        let mut b = boss.lock().unwrap();
        let g = book::answer(&mut b.c, &duty, true).unwrap();
        assert_eq!(b.c.together.claim_of(&duty).map(|x| x.state), Some(ClaimState::Planned));
        assert_eq!(b.plan().tours[0].duties[0].who, Some(Who::Guest(g)));
        // (not driven by the close: the central covers it, as the boss's own)
        let p = day_plan(&b.c, &b.c.date, b.tours.clone(), &[], &[], true);
        assert_eq!((p.tours[0].duties[0].who, p.tours[0].duties[0].problem), (Some(Who::Agency), Some(super::super::plan::Problem::Player)));
        // the boss takes him off again, and gives it him himself
        assert_eq!(book::release(&mut b.c, &duty, None), Ok(g));
        let p = b.plan();
        let now = super::super::clock::now(&b.c);
        assert_eq!(book::give(&mut b.c, &p, &duty, g, now), Ok(()));
        assert!(b.c.together.claim_of(&duty).is_some_and(|x| x.given && x.state == ClaimState::Planned));
    }
    // a wrong secret, another company's code, another version: turned away, nothing said
    let mut wrong = code.clone();
    wrong.secret[0] ^= 1;
    let v = client::send(&base, &req(&wrong, "eve", "Eve", "snapshot", None), client::TIMEOUT).unwrap();
    assert_eq!((v["ok"].as_bool(), v["error"].as_str()), (Some(false), Some(host::BAD_SECRET)));
    assert!(v.get("snapshot").is_none());
    let mut other = code.clone();
    other.company ^= 1;
    let v = client::send(&base, &req(&other, "eve", "Eve", "snapshot", None), client::TIMEOUT).unwrap();
    assert_eq!(v["error"].as_str(), Some(host::BAD_SECRET));
    let mut r = req(&code, "eve", "Eve", "snapshot", None);
    r.v = code::VERSION + 1;
    assert_eq!(client::send(&base, &r, client::TIMEOUT).unwrap()["error"].as_str(), Some(code::NEWER));
    assert!(boss.lock().unwrap().c.together.guests.iter().all(|g| g.name != "Eve"));
    // a code typed wrong never gets as far as the boss
    let text = code.encode();
    let mut typo = text.clone().into_bytes();
    let at = code::PREFIX.len() + 1;
    typo[at] = if typo[at] == b'C' { b'D' } else { b'C' };
    assert_eq!(CompanyCode::decode(&String::from_utf8(typo).unwrap()), Err(code::TYPO));
    drop(server);
}

#[test]
fn an_old_company_file_loads_with_nobody_playing_together() {
    let (c, _) = setup();
    let mut v = serde_json::to_value(&c).unwrap();
    v.as_object_mut().unwrap().remove("together");
    let back: Company = serde_json::from_value(v).unwrap();
    assert!(back.together.open_claims && back.together.claims.is_empty());
    // a duty given to a player keeps its name when saved
    assert_eq!(serde_json::to_string(&Who::Guest(3)).unwrap(), r#"{"guest":3}"#);
}

/// The boss's side answered in-process (no server): `what` of player `player`, with `fill`
/// setting the rest of the message.
fn ask(b: &mut Boss, player: &str, what: &str, fill: impl FnOnce(&mut Request)) -> Value {
    let mut r = Request { v: code::VERSION, secret: b.st.secret.iter().map(|x| format!("{x:02x}")).collect(), company: code::tag_of(&b.c.id), player: player.into(), name: player.to_uppercase(), what: what.into(), ..Default::default() };
    fill(&mut r);
    let p = b.plan();
    let lines = timetable();
    let here = Here { plan: Some(&p), lines: &lines, now: now(), lead: 10 };
    let body = serde_json::to_string(&r).unwrap();
    b.st.answer(&mut b.c, &here, &body).body
}

fn boss() -> Boss {
    let (c, tours) = setup();
    Boss { st: HostState::new(&c.id, "Luc", [3; 8]), c, tours }
}

fn guest_no(b: &Boss, player: &str) -> u32 {
    b.c.together.guests.iter().find(|g| g.id == player).unwrap().no
}

#[test]
fn every_message_is_checked_against_the_players_permissions_on_the_bosss_side() {
    let mut b = boss();
    let v = ask(&mut b, "jan", "hello", |_| {});
    let duty = snap(&v).duties[0].duty.clone();
    ask(&mut b, "piet", "hello", |_| {});
    let (jan, piet) = (guest_no(&b, "jan"), guest_no(&b, "piet"));
    // a driver: drives and asks, nothing more - whatever his page shows
    let you = snap(&v).you.unwrap();
    assert_eq!((you.role, you.perms), (Role::Driver, vec![Perm::Drive]));
    for (what, perm) in [("assign", Perm::Assign), ("unassign", Perm::Assign), ("answer", Perm::Approve), ("set_role", Perm::Players), ("kick", Perm::Players)] {
        let v = ask(&mut b, "jan", what, |r| {
            r.duty = Some(duty.clone());
            r.target = piet;
            r.role = Role::CoOwner;
            r.perms = Perm::ALL.to_vec();
        });
        assert_eq!((v["ok"].as_bool(), v["error"].as_str(), v["perm"].as_str()), (Some(false), Some(host::NOT_ALLOWED), Some(perm.key())), "{what}: {v}");
    }
    assert!(b.c.together.claims.is_empty());
    assert_eq!(b.c.together.guest(piet).unwrap().role, Role::Driver);
    // (a driver the boss took driving from cannot take a duty either)
    let now = now();
    assert!(b.st.set_role(&mut b.c, now, piet, Role::Driver, &[]));
    let v = ask(&mut b, "piet", "take", |r| r.duty = Some(duty.clone()));
    assert_eq!(v["perm"].as_str(), Some("drive"), "{v}");
    // the boss makes Jan the dispatcher: he gives the duty to Piet... who may not drive
    assert!(b.st.set_role(&mut b.c, now, jan, Role::Planner, Role::Planner.preset()));
    let v = ask(&mut b, "jan", "snapshot", |_| {});
    let you = snap(&v).you.unwrap();
    assert!(you.perms.contains(&Perm::Assign) && you.perms.contains(&Perm::Plans) && !you.perms.contains(&Perm::Finances));
    assert!(snap(&v).notices.iter().any(|n| n.text == host::ROLE_SET));
    assert_eq!(snap(&v).members.len(), 2, "who duties can be given to");
    assert!(b.st.set_role(&mut b.c, now, piet, Role::Driver, Role::Driver.preset()));
    let v = ask(&mut b, "jan", "assign", |r| {
        r.duty = Some(duty.clone());
        r.target = piet;
    });
    assert_eq!(v["ok"], true, "{v}");
    assert!(b.c.together.claim_of(&duty).is_some_and(|x| x.guest == piet && x.given && x.state == ClaimState::Planned));
    assert_eq!(b.plan().tours[0].duties[0].who, Some(Who::Guest(piet)));
    // (the duty's sheet kept with it, and how his game starts on it)
    let x = b.c.together.claim_of(&duty).unwrap();
    assert_eq!((x.number.as_str(), x.from, x.to, x.sheet.len(), x.bus.as_str()), ("5", 350, 480, 5, "Vehicles/Citaro/Citaro.bus"));
    assert!(x.start.is_some());
    // Piet sees it as his; Jan takes him off again
    let v = ask(&mut b, "piet", "snapshot", |_| {});
    assert_eq!(snap(&v).duties[0].state, DutyState::Mine);
    assert!(snap(&v).members.is_empty(), "a driver does not see the players' permissions");
    let v = ask(&mut b, "jan", "unassign", |r| r.duty = Some(duty.clone()));
    assert_eq!(v["ok"], true, "{v}");
    assert!(b.c.together.claims.is_empty());
    // a planner manages no players
    let v = ask(&mut b, "jan", "set_role", |r| {
        r.target = piet;
        r.role = Role::Manager;
    });
    assert_eq!(v["perm"].as_str(), Some("players"));
}

#[test]
fn players_are_managed_only_by_those_who_may_do_as_much_themselves() {
    let mut b = boss();
    for p in ["anna", "bert", "cor"] {
        ask(&mut b, p, "hello", |_| {});
    }
    let (anna, bert, cor) = (guest_no(&b, "anna"), guest_no(&b, "bert"), guest_no(&b, "cor"));
    let now = now();
    // Anna: a manager who may also manage players, but not see the money
    let mut want = Role::Manager.preset().to_vec();
    want.push(Perm::Players);
    want.retain(|p| *p != Perm::Finances);
    b.st.set_role(&mut b.c, now, anna, Role::Manager, &want);
    let g = b.c.together.guest(anna).unwrap();
    assert_eq!((g.grant.clone(), g.deny.clone()), (vec![Perm::Players], vec![Perm::Finances]));
    // she makes Bert a planner
    let v = ask(&mut b, "anna", "set_role", |r| {
        r.target = bert;
        r.role = Role::Planner;
        r.perms = Role::Planner.preset().to_vec();
    });
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(b.c.together.guest(bert).unwrap().perms(), Role::Planner.preset().to_vec());
    // ...but cannot give what she has not (the money), nor raise herself
    let v = ask(&mut b, "anna", "set_role", |r| {
        r.target = bert;
        r.role = Role::Manager;
        r.perms = Role::Manager.preset().to_vec();
    });
    assert_eq!(v["error"].as_str(), Some(host::NOT_ABOVE), "{v}");
    let v = ask(&mut b, "anna", "set_role", |r| {
        r.target = anna;
        r.role = Role::CoOwner;
        r.perms = Perm::ALL.to_vec();
    });
    assert_eq!(v["error"].as_str(), Some(host::NOT_ABOVE));
    // a co-owner is above her: she cannot send him away
    b.st.set_role(&mut b.c, now, cor, Role::CoOwner, &Perm::ALL);
    let v = ask(&mut b, "anna", "kick", |r| r.target = cor);
    assert_eq!(v["error"].as_str(), Some(host::NOT_ABOVE));
    // the co-owner sends Bert away for the session, then removes Anna from the company
    let v = ask(&mut b, "cor", "kick", |r| r.target = bert);
    assert_eq!(v["ok"], true, "{v}");
    let v = ask(&mut b, "bert", "snapshot", |_| {});
    assert_eq!((v["error"].as_str(), v.get("snapshot").is_none()), (Some(host::KICKED), true));
    let v = ask(&mut b, "cor", "kick", |r| {
        r.target = anna;
        r.ban = true;
    });
    assert_eq!(v["ok"], true, "{v}");
    assert!(b.c.together.guest(anna).unwrap().banned);
    let v = ask(&mut b, "anna", "hello", |_| {});
    assert_eq!(v["error"].as_str(), Some(host::BANNED));
    assert!(host::ending(host::BANNED) && host::ending(host::KICKED) && !host::ending(host::READING));
    // the boss lets them back
    b.st.let_back(&mut b.c, now, anna);
    b.st.let_back(&mut b.c, now, bert);
    assert_eq!(ask(&mut b, "anna", "hello", |_| {})["ok"], true);
    assert_eq!(ask(&mut b, "bert", "hello", |_| {})["ok"], true);
    // (what the company keeps of roles reads back)
    let back: Company = serde_json::from_value(serde_json::to_value(&b.c).unwrap()).unwrap();
    assert_eq!(back.together.guest(anna).unwrap().perms(), b.c.together.guest(anna).unwrap().perms());
}

#[test]
fn a_driver_asks_for_a_duty_and_whoever_may_approve_says_yes_or_takes_one_at_once() {
    let mut b = boss();
    b.c.together.open_claims = false;
    let v = ask(&mut b, "jan", "hello", |_| {});
    let duty = snap(&v).duties[0].duty.clone();
    ask(&mut b, "eva", "hello", |_| {});
    let (jan, eva) = (guest_no(&b, "jan"), guest_no(&b, "eva"));
    // Jan asks: not his yet
    let v = ask(&mut b, "jan", "take", |r| r.duty = Some(duty.clone()));
    assert_eq!(v["state"].as_str(), Some("asked"), "{v}");
    assert!(snap(&ask(&mut b, "jan", "snapshot", |_| {})).asks.is_empty(), "a driver approves nothing");
    // Eva, the dispatcher, sees the request and says yes
    b.st.set_role(&mut b.c, now(), eva, Role::Planner, Role::Planner.preset());
    let s = snap(&ask(&mut b, "eva", "snapshot", |_| {}));
    assert_eq!(s.asks.len(), 1);
    assert_eq!((s.asks[0].no, s.asks[0].name.as_str(), s.asks[0].number.as_str()), (jan, "JAN", "5"));
    let v = ask(&mut b, "eva", "answer", |r| {
        r.duty = Some(duty.clone());
        r.yes = true;
    });
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(b.c.together.claim_of(&duty).map(|x| (x.guest, x.state)), Some((jan, ClaimState::Planned)));
    // a no: the request goes
    ask(&mut b, "jan", "release", |r| r.duty = Some(duty.clone()));
    ask(&mut b, "jan", "take", |r| r.duty = Some(duty.clone()));
    let v = ask(&mut b, "eva", "answer", |r| {
        r.duty = Some(duty.clone());
        r.yes = false;
    });
    assert_eq!(v["ok"], true, "{v}");
    assert!(b.c.together.claims.is_empty());
    assert!(snap(&v).notices.iter().any(|n| n.text == host::REFUSED));
    // one who may approve takes an open duty at once
    let v = ask(&mut b, "eva", "take", |r| r.duty = Some(duty.clone()));
    assert_eq!(v["state"].as_str(), Some("planned"), "{v}");
}

/// A company whose tour 2 is cut into two duties: the first (350 - 430) for a player, the
/// second (440 - 480) for an agency driver.
fn two_duties() -> Boss {
    let mut b = boss();
    b.tours[0].cuts = Some(vec![3]);
    let wd = super::super::plan::weekday_of(&b.c.date);
    super::super::plan::set_driver(&mut b.c, wd, "Linie5", "2", 1, Some(Who::Agency));
    b
}

fn run(dep: i32, passengers: i32, fines: i64) -> TripRun {
    TripRun { time: 1, driver: "x".into(), map: "maps/Grundorf/global.cfg".into(), line: "5".into(), tour: "2".into(), departure: dep as f64 * 60.0, arrival: (dep + 30) as f64 * 60.0, completed: true, planned: 10, stops: 10, late: 2, metres: 8000.0, seconds: 1800.0, passengers, fines, red_lights: (fines > 0) as i32, ..Default::default() }
}

#[test]
fn what_a_player_drives_is_the_bosss_companys_his_duty_alone_with_its_fines() {
    let mut b = two_duties();
    let v = ask(&mut b, "jan", "hello", |_| {});
    let s = snap(&v);
    assert_eq!(s.duties.iter().map(|d| (d.from, d.to, d.state)).collect::<Vec<_>>(), vec![(350, 430, DutyState::Open), (440, 480, DutyState::Company)]);
    let duty = s.duties[0].duty.clone();
    assert_eq!(ask(&mut b, "jan", "take", |r| r.duty = Some(duty.clone()))["ok"], true);
    assert_eq!(ask(&mut b, "jan", "start", |r| r.duty = Some(duty.clone()))["ok"], true);
    // his trips, one with a red light run: and one of the agency driver's duty, refused
    let v = ask(&mut b, "jan", "report", |r| {
        r.duty = Some(duty.clone());
        r.trips = vec![run(360, 30, 0), run(400, 25, 120_00), run(440, 99, 0)];
        r.done = true;
    });
    assert_eq!((v["taken"].as_u64(), v["refused"].as_u64()), (Some(2), Some(1)), "{v}");
    let g = b.c.together.guest(guest_no(&b, "jan")).unwrap().stats.clone();
    assert_eq!((g.trips, g.duties, g.passengers, g.fines, g.stops, g.on_time), (2, 1, 55, 120_00, 20, 16));
    assert_eq!(g.punctuality(), Some(80.0));
    // the close: his two trips measured, with the fine; the agency driver's duty modelled
    let closed = b.c.date.clone();
    let trips = book::trips_for_close(&b.c);
    let tours = b.tours.clone();
    let cash = b.c.cash;
    let report = super::super::day::close_day_with(&mut b.c, tours, &[], &trips);
    assert_eq!((report.measured, report.by_player, report.covered), (2, 0, 1), "{report:?}");
    let booked = |kind: super::super::model::BookingKind, measured: bool| b.c.ledger.iter().filter(|x| x.date == closed && x.kind == kind && x.measured == measured).map(|x| x.amount).sum::<i64>();
    use super::super::model::BookingKind;
    assert!(booked(BookingKind::Fares, true) > 0, "his fares");
    assert!(booked(BookingKind::Fares, false) > 0, "the agency driver's duty runs as planned: not dropped with the tour");
    assert_eq!(booked(BookingKind::Fine, true), -120_00, "his fine is the company's");
    assert!(report.dropped == 0, "nothing of the tour dropped: {report:?}");
    assert_ne!(b.c.cash, cash);
    book::after_close(&mut b.c, &closed);
    assert!(b.c.together.claims.is_empty() && b.c.together.trips.is_empty());
}

#[test]
fn a_duty_whose_day_closed_before_it_was_driven_stays_the_players_and_books_what_it_can() {
    let mut b = boss();
    let v = ask(&mut b, "jan", "hello", |_| {});
    let duty = snap(&v).duties[0].duty.clone();
    assert_eq!(ask(&mut b, "jan", "take", |r| r.duty = Some(duty.clone()))["ok"], true);
    let jan = guest_no(&b, "jan");
    // the boss's day closes before Jan drives (he was offline): the company covered it
    let closed = b.c.date.clone();
    let tours = b.tours.clone();
    let report = super::super::day::close_day_with(&mut b.c, tours, &[], &[]);
    assert_eq!((report.covered, report.dropped), (1, 0), "{report:?}");
    assert_eq!(b.c.date, super::super::dates::add(&closed, 1));
    book::after_close(&mut b.c, &closed);
    let x = b.c.together.claim_of(&duty).cloned().unwrap();
    assert!(x.late && x.state == ClaimState::Planned && x.start.is_some(), "{x:?}");
    // his page still has it, and he drives it now: with the start kept
    let s = snap(&ask(&mut b, "jan", "snapshot", |_| {}));
    let late = s.duties.iter().find(|d| book::same_duty(&d.duty, &duty)).unwrap();
    assert_eq!((late.state, late.from, late.number.as_str()), (DutyState::Late, 350, "5"));
    let v = ask(&mut b, "jan", "start", |r| r.duty = Some(duty.clone()));
    assert_eq!(v["ok"], true, "{v}");
    // its trips: checked against the duty as it was, booked at once - the fine and the bonus,
    // not the fares (the company had it covered)
    let fines_before = b.c.ledger.iter().filter(|x| x.kind == super::super::model::BookingKind::Fine).count();
    let v = ask(&mut b, "jan", "report", |r| {
        r.duty = Some(duty.clone());
        r.trips = vec![run(360, 30, 80_00), run(999, 1, 0)];
        r.done = true;
    });
    assert_eq!((v["ok"].as_bool(), v["taken"].as_u64(), v["refused"].as_u64()), (Some(true), Some(1), Some(1)), "{v}");
    assert_eq!(b.c.ledger.iter().filter(|x| x.kind == super::super::model::BookingKind::Fine).count(), fines_before + 1);
    let g = b.c.together.guest(jan).unwrap().stats.clone();
    assert_eq!((g.trips, g.late, g.fares, g.fines, g.duties), (1, 1, 0, 80_00, 1));
    // (not booked a second time by the next close)
    assert!(book::trips_for_close(&b.c).is_empty());
    // finished: it goes with the next close
    let today = b.c.date.clone();
    book::after_close(&mut b.c, &today);
    assert!(b.c.together.claims.is_empty());
}

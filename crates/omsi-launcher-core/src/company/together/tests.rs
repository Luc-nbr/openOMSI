//! A boss's company server and a friend's launcher over the loopback: joining, the day, taking a
//! duty and being refused one, starting it in the boss's world, reporting its trips, and the
//! day's close booking them; a bad code. No game is started.

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

/// The boss's side: his company, its tours, the session's state, his game's code.
struct Boss {
    c: Company,
    tours: Vec<TourOfDay>,
    st: HostState,
    world: Option<(String, String)>,
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
        let world = b.world.clone();
        let Boss { c, st, .. } = &mut *b;
        let here = Here { plan: Some(&p), lines: &lines, world, now: now(), lead: 10 };
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
    let game = omsi_net::SessionCode::single(omsi_net::PROTOCOL as u8, Ipv4Addr::new(192, 168, 1, 20), 27015, 0x1234_5678_9ABC).encode();
    let boss = Arc::new(Mutex::new(Boss { st: HostState::new(&c.id, "Luc", secret), world: None, c, tours }));
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

    // driving it: the boss's world is not open yet, then it is
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "start", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["error"].as_str(), Some(host::WORLD_CLOSED), "{v}");
    let map = boss.lock().unwrap().c.map.clone();
    boss.lock().unwrap().world = Some((game.clone(), map));
    let v = client::send(&base, &req(&code, "jan-1", "Jan", "start", Some(&duty)), client::TIMEOUT).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let start: DriveStart = serde_json::from_value(v["start"].clone()).unwrap();
    assert_eq!((start.game.as_str(), start.line.as_str(), start.tour.as_str(), start.first), (game.as_str(), "Linie5", "2", 1));
    assert_eq!(start.legs, vec!["Linie5|2|1|3".to_string()]);
    assert_eq!(start.bus, "Vehicles/Citaro/Citaro.bus");
    let d = start.duty(crate::Duty { traffic: Some(40), ..Default::default() }, "Jan");
    assert_eq!((d.lan.as_deref(), d.lan_name.as_deref(), d.company.as_deref(), d.traffic), (Some(format!("join:{game}").as_str()), Some("Jan"), None, Some(40)));
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
    assert_eq!((report.measured, report.by_player, report.covered, report.tours), (3, 1, 1, 1));
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
    let boss = Arc::new(Mutex::new(Boss { st: HostState::new(&c.id, "Luc", secret), world: None, c, tours }));
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
        // (not driven by the close: open, its trips dropped)
        let p = day_plan(&b.c, &b.c.date, b.tours.clone(), &[], &[], true);
        assert_eq!(p.tours[0].duties[0].who, None);
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

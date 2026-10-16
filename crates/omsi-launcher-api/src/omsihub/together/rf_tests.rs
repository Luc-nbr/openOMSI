//! RosterFlow in company multiplayer, the boss's company server and the players' launchers over
//! the loopback (Luc 2026-10-15: "as a participant in a multiplayer session ... nothing opens,
//! and the whole plan mode doesn't work"): a player opens RosterFlow on the boss's company, his
//! edit lands in it, a player whose role does not allow it is refused, and the others are told
//! who changed what. The boss's session is the test's own (no launcher's), its company changed
//! in it only (`company::test_dry`).

use super::super::company::{self, Session, Viewer};
use super::super::rosterflow;
use super::host::page_for;
use omsi_launcher_lib::company::together::client;
use omsi_launcher_lib::company::together::code;
use omsi_launcher_lib::company::together::host::{self as th, HostState};
use omsi_launcher_lib::company::together::server::Server;
use omsi_launcher_lib::company::together::wire::Request;
use omsi_launcher_lib::company::together::Role;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_launcher_lib::{LineInfo, StopInfo, TourInfo, TripInfo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const SECRET: [u8; 8] = [3, 1, 4, 1, 5, 9, 2, 6];

/// A company of line 5 with two buses' tours a day, RosterFlow on, and its week's timetable.
fn line5(name: &str) -> (Company, LineInfo) {
    let trip = |no: &str, k: i32, from: i32| {
        let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
        let dep = from + k * 60;
        TripInfo { name: format!("5 {no}-{k}"), index: 0, line: "5".into(), from: a.into(), terminus: format!("{b} Bf"), departure: dep as f64 * 60.0, arrival: (dep + 50) as f64 * 60.0, stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(), km: 20.0 }
    };
    let tour = |no: &str, from: i32| TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips: (0..8).map(|k| trip(no, k, from)).collect() };
    let l = LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 390)] };
    let mut c = co::found(&co::Founding { name: name.into(), game_mode: co::GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.lines.push(co::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.addons.rosterflow = true;
    (c, l)
}

/// The boss's side: his session with the company open, the session's state, each player's
/// pages' state - as `together::host` keeps them.
struct Boss {
    s: Session,
    st: HostState,
    viewers: HashMap<u32, Viewer>,
}

/// A message of a player's launcher answered as the boss's launcher answers it (`host::handle`
/// and `relay_page`, without the launcher's statics).
fn serve(b: &mut Boss, body: &str) -> String {
    company::test_dry();
    let req = match b.st.admits(body) {
        Ok(r) => r,
        Err(v) => return v.to_string(),
    };
    let Some(c) = b.s.company.as_mut() else { return th::refusal(th::NOT_OPEN_COMPANY).to_string() };
    let (guest, perms) = match b.st.member(c, &req, th_now()) {
        Ok(x) => x,
        Err(v) => return v.to_string(),
    };
    let mut viewer = b.viewers.remove(&guest).unwrap_or_default();
    viewer.guest = guest;
    let cmd = req.page.get("cmd").and_then(Value::as_str).unwrap_or("").to_string();
    let args = req.page.get("args").cloned().unwrap_or(json!({}));
    let out = page_for(&mut b.s, &mut viewer, &perms, &cmd, &args);
    b.viewers.insert(guest, viewer);
    out.to_string()
}

fn th_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A player's launcher: who he is, where the boss's launcher answers.
struct Player {
    base: String,
    company: u16,
    id: &'static str,
    name: &'static str,
}

impl Player {
    /// A command of RosterFlow (or the company's pages) relayed as `together::join` relays it:
    /// the answer, or the refusal's error.
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, String> {
        let secret = SECRET.iter().map(|b| format!("{b:02x}")).collect();
        let req = Request { v: code::VERSION, secret, company: self.company, player: self.id.into(), name: self.name.into(), what: "page".into(), page: json!({ "cmd": cmd, "args": args }), ..Default::default() };
        let v = client::send(&self.base, &req, client::TIMEOUT)?;
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("answer").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("error").and_then(Value::as_str).unwrap_or("?").to_string())
        }
    }
}

/// The boss's company shared on the loopback, his players known with their roles.
fn share(name: &str, players: &[(&str, &str, Role)]) -> (Arc<Mutex<Boss>>, Server, u16) {
    company::words::set_language("en");
    let (mut c, l) = line5(name);
    for (id, who, role) in players {
        let no = c.together.guest_of(id, who).unwrap();
        assert!(c.together.set_role(no, *role, role.preset()));
    }
    let st = HostState::new(&c.id, &c.profile, SECRET);
    let tag = code::tag_of(&c.id);
    let mut s = Session::default();
    s.company = Some(c);
    company::test_week(&mut s, vec![l]);
    let boss = Arc::new(Mutex::new(Boss { s, st, viewers: HashMap::new() }));
    let b2 = boss.clone();
    let server = Server::start(0, true, Arc::new(move |body: &str| serve(&mut b2.lock().unwrap_or_else(|e| e.into_inner()), body))).unwrap();
    (boss, server, tag)
}

fn player(server: &Server, company: u16, id: &'static str, name: &'static str) -> Player {
    Player { base: format!("http://127.0.0.1:{}", server.port()), company, id, name }
}

/// RosterFlow opens for a planner on the boss's company: its day's board, and an edit of his - a
/// bus's block locked - in the boss's company, his to undo; a driver and a dispatcher who may
/// only look are refused it, with the permission it takes.
#[test]
fn a_player_plans_in_rosterflow_on_the_bosss_company() {
    let (boss, server, tag) = share("Together Flow", &[("anna-1", "Anna", Role::Planner), ("dirk-2", "Dirk", Role::Driver)]);
    let anna = player(&server, tag, "anna-1", "Anna");
    let dirk = player(&server, tag, "dirk-2", "Dirk");
    let on = anna.ask("rf_on", json!({ "lang": "en" })).unwrap();
    assert_eq!((on["on"].as_bool(), on["remote"]["plans"].as_bool()), (Some(true), Some(true)), "{on}");
    assert_eq!(on["company"], boss.lock().unwrap().s.company.as_ref().unwrap().id.as_str());
    let board = anna.ask("rf_board", json!({ "day": 0, "lang": "en" })).unwrap();
    assert_eq!(board["board"]["blocks"].as_array().map(Vec::len), Some(2), "{}", board["reading"]);
    let rev = board["rev"].as_str().unwrap().to_string();
    // her edit lands in the boss's company
    let out = anna.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 0, "rev": rev, "lang": "en" })).unwrap();
    assert_eq!(out["edited"], true, "{out}");
    {
        let b = boss.lock().unwrap();
        let c = b.s.company.as_ref().unwrap();
        let draft = c.planning.flow.draft.as_ref().expect("the boss's company has her draft");
        assert_eq!(draft.sheet.locks.len(), 1);
    }
    let again = anna.ask("rf_board", json!({ "day": 0, "lang": "en" })).unwrap();
    assert_eq!(again["head"]["draft"], true);
    assert_eq!(again["head"]["undo"].as_array().map(Vec::len), Some(1), "her undo");
    // a driver may not even look, let alone change it
    assert_eq!(dirk.ask("rf_on", json!({})).unwrap_err(), th::NOT_ALLOWED);
    assert_eq!(dirk.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 1 })).unwrap_err(), th::NOT_ALLOWED);
    assert_eq!(boss.lock().unwrap().s.company.as_ref().unwrap().planning.flow.draft.as_ref().unwrap().sheet.locks.len(), 1, "nothing of the driver's");
    // a dispatcher's role (the day's service, no planning) looks, read-only, and is told why
    {
        let mut b = boss.lock().unwrap();
        let c = b.s.company.as_mut().unwrap();
        let no = c.together.guest_of("dirk-2", "Dirk").unwrap();
        assert!(c.together.set_role(no, Role::Driver, &[omsi_launcher_lib::company::together::Perm::Service]));
    }
    let on = dirk.ask("rf_on", json!({})).unwrap();
    assert_eq!(on["remote"]["plans"], false);
    assert!(on["remote"]["why"].as_str().is_some_and(|w| w.contains("look at the planning")));
    assert!(dirk.ask("rf_board", json!({ "day": 0 })).unwrap()["board"]["blocks"].is_array());
    assert!(dirk.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 1 })).is_err(), "seen, not changed");
    assert!(dirk.ask("rf_tp_act", json!({ "act": "save" })).is_err());
    // the tutorial is the player's launcher's own, never the boss's
    assert!(anna.ask("rf_tut_act", json!({ "act": "end", "undo": true })).is_err());
    rosterflow::live::forget(&boss.lock().unwrap().s.company.as_ref().unwrap().id);
}

/// What one planner changes the others are told of - the boss and another player - with who did
/// it; an undo that would take back somebody else's change begins anew; and an edit made on the
/// day as it was before somebody else changed it is refused, not made on the wrong bus.
#[test]
fn the_others_are_told_who_changed_the_plan() {
    let (boss, server, tag) = share("Together Told", &[("anna-1", "Anna", Role::Planner), ("bert-3", "Bert", Role::Manager)]);
    let anna = player(&server, tag, "anna-1", "Anna");
    let bert = player(&server, tag, "bert-3", "Bert");
    // both in RosterFlow: each sees the other
    assert!(anna.ask("rf_here", json!({ "view": 1, "day": 0 })).unwrap()["told"].as_array().is_some_and(Vec::is_empty));
    let here = bert.ask("rf_here", json!({ "view": 3, "day": 0 })).unwrap();
    assert_eq!(here["here"][0]["name"], "Anna", "{here}");
    let rev_b = bert.ask("rf_board", json!({ "day": 0 })).unwrap()["rev"].as_str().unwrap().to_string();
    let rev_a = anna.ask("rf_board", json!({ "day": 0 })).unwrap()["rev"].as_str().unwrap().to_string();
    assert_eq!(anna.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 0, "rev": rev_a })).unwrap()["edited"], true);
    // Bert's next word: told it was Anna
    let told = bert.ask("rf_here", json!({ "view": 3, "day": 0 })).unwrap();
    let words = told["told"].as_array().cloned().unwrap_or_default();
    assert!(words.iter().any(|w| w.as_str().is_some_and(|t| t.starts_with("Anna changed the blocks and duties"))), "{told}");
    // (once)
    assert!(bert.ask("rf_here", json!({ "view": 3, "day": 0 })).unwrap()["told"].as_array().is_some_and(Vec::is_empty));
    // his edit of the day as it was before Anna's: refused, said by whom
    let stale = bert.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 1, "rev": rev_b })).unwrap();
    assert_eq!(stale["stale"], true, "{stale}");
    assert!(stale["say"]["text"].as_str().is_some_and(|t| t.starts_with("Anna changed this day meanwhile")));
    assert_eq!(boss.lock().unwrap().s.company.as_ref().unwrap().planning.flow.draft.as_ref().unwrap().sheet.locks.len(), 1);
    // the day shown anew, the same edit goes through - and Anna's undo, which would take Bert's
    // lock back with hers, begins anew instead
    let rev_b = bert.ask("rf_board", json!({ "day": 0 })).unwrap()["rev"].as_str().unwrap().to_string();
    assert_eq!(bert.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 1, "rev": rev_b })).unwrap()["edited"], true);
    let undo = anna.ask("rf_act", json!({ "act": "undo", "day": 0 })).unwrap();
    assert!(undo["told"].as_array().is_some_and(|t| t.iter().any(|w| w.as_str().is_some_and(|w| w.contains("your Undo begins again")))), "{undo}");
    assert_eq!(boss.lock().unwrap().s.company.as_ref().unwrap().planning.flow.draft.as_ref().unwrap().sheet.locks.len(), 2, "Bert's lock stays");
    // the boss in his own RosterFlow (his launcher's state, not a player's) is told too
    let mut b = boss.lock().unwrap();
    let c = b.s.company.clone().unwrap();
    let mut mine = rosterflow::Rf::default();
    let first = rosterflow::live::told(&c, &mut mine, rosterflow::live::Editor::Boss);
    assert!(first.is_empty(), "what came before he looked is not told");
    drop(b);
    assert_eq!(anna.ask("rf_act", json!({ "act": "lock_block", "day": 0, "tour": 0 })).unwrap()["edited"], true);
    b = boss.lock().unwrap();
    let c = b.s.company.clone().unwrap();
    let words = rosterflow::live::told(&c, &mut mine, rosterflow::live::Editor::Boss);
    assert_eq!(words.len(), 1, "{words:?}");
    assert!(words[0].starts_with("Anna changed the blocks and duties of "), "{words:?}");
    rosterflow::live::forget(&c.id);
}

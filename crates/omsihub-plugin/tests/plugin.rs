//! The plugin run in openOMSI's own plugin runtime (`omsi-plugin`), as the game runs it: a
//! fake bus with a duty, a few frames, and what reaches Omsi-Hub through the bridge.

use omsi_plugin::{io, GameEvent, HostConfig, InfoValue, PluginIo, Plugins};
use omsihub_plugin::bridge::{self, Assignment, Notice, Personnel, ReportReader};
use omsihub_plugin::{compat, install};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// The game: a bus, and a duty the plugin can take.
#[derive(Default)]
struct Game {
    duty: Option<io::Duty>,
    started: Vec<(String, String, usize, usize)>,
    events: Vec<GameEvent>,
    messages: Vec<String>,
    /// The duty's trips with their stops (none: two trips without stops).
    trips: Vec<io::Trip>,
    /// The bus's speed, km/h.
    speed: f32,
    /// What the plugin asked of the IBIS: `auto on`, `auto off`, `typed`.
    ibis: Vec<String>,
}

impl PluginIo for Game {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, name: &str) -> Option<f32> {
        (name == "Velocity").then_some(self.speed)
    }
    fn set_var(&mut self, _: &str, _: f32) {}
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn dt(&self) -> f32 {
        1.0
    }
    fn vehicle_name(&self) -> Option<String> {
        Some("MAN SD202".into())
    }
    fn message(&mut self, text: &str, _: f32) {
        self.messages.push(text.into());
    }
    fn info(&self) -> Vec<(&'static str, InfoValue)> {
        let mut v = vec![("version", InfoValue::Text("0.2.1500".into())), ("clock", InfoValue::Num(28_800.0))];
        if let Some(d) = &self.duty {
            v.push(("line", InfoValue::Text(d.line.clone())));
            v.push(("tour", InfoValue::Text(d.tour.clone())));
        }
        v
    }
    fn events(&self) -> Vec<GameEvent> {
        self.events.clone()
    }
    fn duty(&self) -> Option<io::Duty> {
        self.duty.clone()
    }
    fn duty_trips(&self) -> Vec<io::Trip> {
        if !self.trips.is_empty() {
            return self.trips.clone();
        }
        ["Zoo - Hbf", "Hbf - Zoo"].iter().map(|n| io::Trip { name: n.to_string(), line: "10".into(), ..io::Trip::default() }).collect()
    }
    fn duty_set_auto_ibis(&mut self, on: bool) -> bool {
        self.ibis.push(if on { "auto on" } else { "auto off" }.into());
        true
    }
    fn duty_type_ibis(&mut self) -> bool {
        self.ibis.push("typed".into());
        true
    }
    fn duty_start(&mut self, line: &str, tour: &str, trip: usize, stop: usize) -> Result<(), String> {
        self.started.push((line.into(), tour.into(), trip, stop));
        self.duty = Some(io::Duty { line: line.into(), tour: tour.into(), trip, trips: 2, delay: 42.0, ..io::Duty::default() });
        Ok(())
    }
}

fn plugins_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsihub-plugin-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let p = d.join("Plugins");
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// When the assignment was given: today (one of yesterday is not believed).
fn issued() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 3600 * 3600
}

fn assignment() -> Assignment {
    let mut texts = BTreeMap::new();
    texts.insert("Start duty".to_string(), "Dienst starten".to_string());
    Assignment {
        issued: issued(),
        company: "Stadtbus Berlin".into(),
        duty: "D-12".into(),
        line: "10".into(),
        tour: "2".into(),
        auto_start: true,
        notices: vec![Notice { id: "n1".into(), title: "Depot".into(), text: "Bus 4711 is in the workshop".into(), kind: "info".into() }],
        texts,
        ..Assignment::default()
    }
}

fn trip_done(trip: f64) -> GameEvent {
    GameEvent { name: "trip_done", args: vec![InfoValue::Num(trip), InfoValue::Text("arrived".into()), InfoValue::Num(95.0), InfoValue::Num(88.0), InfoValue::Num(100.0)] }
}

#[test]
fn the_duty_is_taken_its_trips_reported_and_the_notices_shown() {
    let plugins = plugins_dir("full");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    bridge::write_assignment(&dir, &assignment()).unwrap();

    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    assert_eq!(ps.lua.len(), 1, "the plugin is loaded");
    // the handshake: everything the add-on uses is there
    let hello = bridge::read_hello(&dir).expect("data/hello.json");
    assert_eq!((hello.bridge, hello.plugin.as_str(), hello.api, hello.issued), (bridge::BRIDGE, install::VERSION, 1, issued()));
    let c = compat::negotiate(&hello);
    assert_eq!(c.level, compat::Level::Supported, "{c:?}");

    let mut game = Game::default();
    ps.frame(&mut game);
    // (auto_start: the duty is taken as soon as the player is in the bus)
    assert_eq!(game.started, vec![("10".to_string(), "2".to_string(), 0, 0)]);
    ps.frame(&mut game);
    game.events = vec![trip_done(1.0)];
    ps.frame(&mut game);
    game.events.clear();
    game.duty = None;
    ps.frame(&mut game);

    let reports = ReportReader::default().read_new(&dir);
    let kinds: Vec<&str> = reports.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["duty_start", "trip", "duty_end"]);
    let trip = &reports[1];
    assert_eq!((trip.issued, trip.duty.as_str(), trip.line.as_str(), trip.tour.as_str()), (issued(), "D-12", "10", "2"));
    assert_eq!((trip.trip, trip.trip_name.as_str(), trip.how.as_str()), (1, "Zoo - Hbf", "arrived"));
    assert_eq!((trip.trips, trip.number.as_str()), (2, "10"));
    assert_eq!((trip.driving, trip.comfort, trip.tickets, trip.delay), (Some(95.0), Some(88.0), Some(100.0), Some(42.0)));
    assert_eq!(trip.clock, Some(28_800.0));

    // the notice as a card, once; the panel in the player's language
    let ui = ps.ui.borrow();
    let toasts: Vec<&str> = ui.toasts().iter().map(|t| t.text.as_str()).collect();
    assert_eq!(toasts, ["Bus 4711 is in the workshop"]);
    let panel = ui.panels().iter().find(|p| p.id == "omsihub").expect("the company's panel");
    assert!(format!("{:?}", panel.panel).contains("Stadtbus Berlin"));
    drop(ui);

    // a notice that comes while the game runs
    bridge::write_notices(&dir, &[Notice { id: "n2".into(), text: "Diversion on line 10".into(), ..Notice::default() }]).unwrap();
    for _ in 0..6 {
        ps.frame(&mut game);
    }
    assert!(ps.ui.borrow().toasts().iter().any(|t| t.text == "Diversion on line 10"));
}

/// The log lines of the plugins, caught.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Catch;

impl log::Log for Catch {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        LOG.lock().unwrap().push(format!("[{}] {}", r.level(), r.args()));
    }
    fn flush(&self) {}
}

#[test]
fn an_older_openomsi_without_files_panels_or_game_functions_still_works() {
    static CATCH: Catch = Catch;
    let _ = log::set_logger(&CATCH).map(|_| log::set_max_level(log::LevelFilter::Info));
    let plugins = plugins_dir("old");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    // (an openOMSI before the file functions, the panels and `game.*`: taken out of the API
    // ahead of the plugin's own code)
    let main = std::fs::read_to_string(dir.join("main.lua")).unwrap();
    std::fs::write(dir.join("main.lua"), format!("omsi.files = nil; omsi.ui = nil; omsi.game = nil; omsi.json = nil\n{main}")).unwrap();
    bridge::write_assignment(&dir, &Assignment { auto_start: false, ..assignment() }).unwrap();

    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    assert_eq!(ps.lua.len(), 1);
    let mut game = Game::default();
    // the player takes the duty from the game's menu; trips are watched one by one
    game.duty_start("10", "2", 0, 0).unwrap();
    game.duty.as_mut().unwrap().trip = 0;
    for trip in [0, 0, 1, 1, 1] {
        game.duty.as_mut().unwrap().trip = trip;
        ps.frame(&mut game);
    }
    // nothing could be written: the handshake and the reports are in the log
    assert!(!dir.join(bridge::DATA_DIR).join(bridge::REPORTS_FILE).exists());
    let log = LOG.lock().unwrap().join("\n");
    let (hello, reports) = bridge::from_log(&log);
    let hello = hello.expect("the handshake in the log");
    assert_eq!(hello.functions.get("files.append"), Some(&false));
    let c = compat::negotiate(&hello);
    assert_eq!(c.level, compat::Level::Limited);
    assert!(c.features.iter().any(|f| f.id == "reports" && !f.on));
    let kinds: Vec<&str> = reports.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["duty_start", "trip"]);
    assert_eq!((reports[1].trip, reports[1].trip_name.as_str()), (1, "Zoo - Hbf"));
    // the notice as a plain line of text
    assert!(game.messages.iter().any(|m| m == "Depot: Bus 4711 is in the workshop"), "{:?}", game.messages);
}

#[test]
fn the_plugin_checks_every_function_the_add_on_negotiates() {
    let main = install::FILES[0].1;
    let needs = &main[main.find("local NEEDS = {").unwrap()..];
    let needs = &needs[..needs.find('}').unwrap()];
    let mut in_lua: Vec<&str> = needs.split('"').skip(1).step_by(2).collect();
    let mut in_rust: Vec<&str> = compat::FEATURES.iter().flat_map(|f| f.needs.iter().copied()).collect();
    in_lua.sort();
    in_rust.sort();
    assert_eq!(in_lua, in_rust);
    // (and openOMSI's API has each of them)
    for n in in_rust {
        assert!(omsi_plugin::api::find(n).is_some(), "omsi.{n} is not in openOMSI's plugin API");
    }
}

/// A trip of three stops, due at 8:00, 8:05 and 8:10, as the duty's first.
fn three_stops() -> io::Trip {
    let stop = |id: i64, name: &str, at: f64| io::Stop { id, name: name.into(), arr: at, dep: at + 30.0, pos: None, stops: true };
    io::Trip { name: "Zoo - Hbf".into(), line: "10".into(), terminus: "10 Hauptbahnhof".into(), departure: 28_830.0, end: 29_400.0, stops: vec![stop(1, "Zoo", 28_800.0), stop(2, "Markt", 29_100.0), stop(3, "Hauptbahnhof", 29_400.0)] }
}

#[test]
fn an_assignment_of_an_earlier_day_is_not_taken() {
    let plugins = plugins_dir("stale");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    bridge::write_assignment(&dir, &Assignment { issued: issued() - 2 * 86_400, ..assignment() }).unwrap();
    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    let mut game = Game::default();
    ps.frame(&mut game);
    ps.frame(&mut game);
    // (no duty taken, no company's panel)
    assert!(game.started.is_empty());
    assert!(!ps.ui.borrow().panels().iter().any(|p| p.id == "omsihub"));
    assert_eq!(bridge::read_hello(&dir).map(|h| h.issued), Some(0));
}

#[test]
fn a_trip_is_recorded_stop_by_stop_and_its_card_shown() {
    let plugins = plugins_dir("trip");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    let mut texts = BTreeMap::new();
    texts.insert("Worst delay".to_string(), "Größte Verspätung".to_string());
    bridge::write_texts(&dir, &texts).unwrap();
    bridge::write_assignment(&dir, &Assignment { auto_start: false, ..assignment() }).unwrap();
    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    assert_eq!(ps.lua.len(), 1, "the plugin is loaded");
    let trip = three_stops();
    let mut game = Game { trips: vec![trip.clone(), io::Trip { name: "Hbf - Zoo".into(), line: "10".into(), ..io::Trip::default() }], ..Game::default() };
    let at = |game: &mut Game, next: usize, at_stop: bool, delay: f64| {
        game.duty = Some(io::Duty { line: "10".into(), tour: "2".into(), trip: 0, trips: 2, next, at_stop, delay, current: trip.clone(), ..io::Duty::default() });
    };
    // at the first stop, half a minute early; off it 20 s early
    at(&mut game, 0, true, -30.0);
    ps.frame(&mut game);
    at(&mut game, 1, false, -20.0);
    ps.frame(&mut game);
    // the second stop: in and out more than three minutes late, a jolt on the way
    at(&mut game, 1, true, 200.0);
    game.events = vec![GameEvent { name: "jolt", args: vec![InfoValue::Num(-3.2), InfoValue::Num(0.4), InfoValue::Num(30.0), InfoValue::Num(12.0)] }];
    ps.frame(&mut game);
    game.events.clear();
    at(&mut game, 2, false, 210.0);
    ps.frame(&mut game);
    // the last stop, a minute late: the trip is done
    at(&mut game, 2, true, 60.0);
    game.events = vec![trip_done(1.0)];
    ps.frame(&mut game);
    game.events.clear();
    ps.frame(&mut game);

    let reports = ReportReader::default().read_new(&dir);
    let trip = reports.iter().find(|r| r.is_trip()).expect("the trip reported");
    let run = trip.record.clone().expect("the trip's record");
    assert_eq!((run["line"].as_str(), run["terminus"].as_str(), run["trip"].as_u64(), run["trips"].as_u64()), (Some("10"), Some("Hauptbahnhof"), Some(1), Some(2)));
    // three stops served: the second late, the worst delay there
    assert_eq!((run["planned"].as_i64(), run["stops"].as_i64(), run["early"].as_i64(), run["late"].as_i64()), (Some(3), Some(3), Some(0), Some(1)));
    assert_eq!((run["worst"].as_f64(), run["completed"].as_bool(), run["jolts"].as_i64()), (Some(210.0), Some(true), Some(1)));
    let visits = run["visits"].as_array().expect("the stops' times");
    assert_eq!(visits.len(), 3);
    assert_eq!((visits[1]["arrived"].as_f64(), visits[1]["left"].as_f64()), (Some(29_300.0), Some(29_340.0)));
    assert_eq!(visits[2]["arrived"].as_f64(), Some(29_460.0));
    // the card, in the player's language
    let ui = ps.ui.borrow();
    let card = ui.panels().iter().find(|p| p.id == "omsihub_trip").expect("the trip's card");
    let text = format!("{:?}", card.panel);
    assert!(text.contains("Hauptbahnhof") && text.contains("Größte Verspätung"), "{text}");
}

#[test]
fn the_drive_watch_fines_a_red_light_and_counts_hard_braking_into_the_trip() {
    let plugins = plugins_dir("watch");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    bridge::write_assignment(&dir, &Assignment { auto_start: false, ..assignment() }).unwrap();
    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    let trip = three_stops();
    let mut game = Game { trips: vec![trip.clone()], ..Game::default() };
    let at = |game: &mut Game, next: usize, at_stop: bool| {
        game.duty = Some(io::Duty { line: "10".into(), tour: "2".into(), trip: 0, trips: 1, next, at_stop, current: trip.clone(), ..io::Duty::default() });
    };
    at(&mut game, 0, true);
    ps.frame(&mut game);
    at(&mut game, 1, false);
    game.speed = 50.0;
    ps.frame(&mut game);
    ps.frame(&mut game);
    // a light turns red ahead; two seconds later the bus runs it at 45 km/h
    game.events = vec![GameEvent { name: "light_ahead", args: vec![InfoValue::Text("red".into()), InfoValue::Num(30.0)] }];
    ps.frame(&mut game);
    game.events.clear();
    ps.frame(&mut game);
    game.events = vec![GameEvent { name: "red_light", args: vec![InfoValue::Num(45.0)] }];
    ps.frame(&mut game);
    game.events.clear();
    // braking hard from 50 to 10 km/h in two seconds
    for kmh in [30.0, 10.0, 10.0] {
        game.speed = kmh;
        ps.frame(&mut game);
    }
    assert!(ps.ui.borrow().toasts().iter().any(|t| t.text.contains("€200")), "the red light's notice");
    game.speed = 0.0;
    at(&mut game, 2, true);
    game.events = vec![trip_done(1.0)];
    ps.frame(&mut game);
    let reports = ReportReader::default().read_new(&dir);
    let run = reports.iter().find(|r| r.is_trip()).and_then(|r| r.record.clone()).expect("the trip's record");
    assert_eq!((run["watched"].as_bool(), run["red_lights"].as_i64(), run["fines"].as_i64(), run["hard_brakes"].as_i64()), (Some(true), Some(1), Some(20_000), Some(1)));
    assert_eq!(run["offences"][0]["kind"].as_str(), Some("red_light"));
}

fn ui(name: &'static str, args: &[&str]) -> GameEvent {
    GameEvent { name, args: args.iter().map(|a| InfoValue::Text(a.to_string())).collect() }
}

fn panel_text(ps: &Plugins, id: &str) -> Option<String> {
    ps.ui.borrow().panels().iter().find(|p| p.id == id).map(|p| format!("{:?}", p.panel))
}

#[test]
fn the_driver_signs_on_accepts_the_duty_and_has_the_board() {
    let plugins = plugins_dir("signon");
    install::install(&plugins).unwrap();
    let dir = install::plugin_dir(&plugins);
    bridge::write_assignment(&dir, &Assignment { personnel: Some(Personnel { number: "482913".into(), code: "5821".into() }), ..assignment() }).unwrap();
    let mut ps = Plugins::load(&[plugins.clone()], &HostConfig::default());
    let mut game = Game { trips: vec![three_stops()], ..Game::default() };
    ps.frame(&mut game);
    ps.frame(&mut game);
    // not signed on: the duty waits, though it was to start by itself, and the IBIS with it
    assert!(game.started.is_empty());
    assert_eq!(game.ibis.last().map(String::as_str), Some("auto off"));
    assert!(panel_text(&ps, "omsihub").unwrap().contains("Not signed on"));
    // a wrong code, then the right one
    for events in [vec![ui("ui_change", &["omsihub", "signon_number", "482913"]), ui("ui_change", &["omsihub", "signon_code", "1111"]), ui("ui_click", &["omsihub", "signon"])], vec![ui("ui_change", &["omsihub", "signon_code", "5821"]), ui("ui_click", &["omsihub", "signon"])]] {
        game.events = events;
        ps.frame(&mut game);
    }
    game.events.clear();
    assert!(game.started.is_empty());
    assert!(panel_text(&ps, "omsihub").unwrap().contains("Accept duty"));
    // the duty order accepted: the duty is taken, and the board comes
    game.events = vec![ui("ui_click", &["omsihub", "accept"])];
    ps.frame(&mut game);
    game.events.clear();
    assert_eq!(game.started.len(), 1);
    // (the duty's codes into the IBIS as the order is accepted; the game types the next trips)
    assert!(game.ibis.iter().any(|x| x == "typed"), "{:?}", game.ibis);
    assert_eq!(game.ibis.last().map(String::as_str), Some("auto on"));
    let trip = three_stops();
    game.duty = Some(io::Duty { line: "10".into(), tour: "2".into(), trip: 0, trips: 1, next: 1, delay: 240.0, current: trip, ..io::Duty::default() });
    ps.frame(&mut game);
    ps.frame(&mut game);
    let board = panel_text(&ps, "omsihub_board").expect("the duty board");
    // (the stop it heads for marked, four minutes late: when it will be there)
    assert!(board.contains("▶ Markt") && board.contains("+4:00") && board.contains("08:09"), "{board}");
}

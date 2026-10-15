//! The plugin run in openOMSI's own plugin runtime (`omsi-plugin`), as the game runs it: a
//! fake bus with a duty, a few frames, and what reaches Omsi-Hub through the bridge.

use omsi_plugin::{io, GameEvent, HostConfig, InfoValue, PluginIo, Plugins};
use omsihub_plugin::bridge::{self, Assignment, Notice, ReportReader};
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
}

impl PluginIo for Game {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, _: &str) -> Option<f32> {
        None
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
        ["Zoo - Hbf", "Hbf - Zoo"].iter().map(|n| io::Trip { name: n.to_string(), line: "10".into(), ..io::Trip::default() }).collect()
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

fn assignment() -> Assignment {
    let mut texts = BTreeMap::new();
    texts.insert("Start duty".to_string(), "Dienst starten".to_string());
    Assignment {
        issued: 1_760_000_000,
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
    assert_eq!((hello.bridge, hello.plugin.as_str(), hello.api, hello.issued), (bridge::BRIDGE, install::VERSION, 1, 1_760_000_000));
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
    assert_eq!((trip.issued, trip.duty.as_str(), trip.line.as_str(), trip.tour.as_str()), (1_760_000_000, "D-12", "10", "2"));
    assert_eq!((trip.trip, trip.trip_name.as_str(), trip.how.as_str()), (1, "Zoo - Hbf", "arrived"));
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

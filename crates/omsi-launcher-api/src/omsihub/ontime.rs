//! On time for the driver (Luc, 2026-10-16): what the company tells its driver on the way, put
//! together while the game runs and handed to the plugin's `ontime.lua` - punctuality,
//! headway, protected connections and the day's diversions. The rules are the company's own
//! (`omsi_launcher_lib::company::ontime`); this is the link:
//!
//! * the plugin writes where the bus is every few seconds (`data/live.json`: the duty's line,
//!   tour and trip, its stops, the next one and the delay - [`Live`]);
//! * the launcher's poll (`refresh`, from `poll` in `lib.rs`) reads it, works out the brief with
//!   the company's day - the other buses of the line where the day's one delay model has them
//!   (`lateness::model_with`, as the fleet map moves them), the line's connections, the
//!   diversions and roadworks of the day - and writes it for the plugin (`data/ontime.json`:
//!   `brief`);
//! * what the driver is to be told once (a hold, a closed stop ahead) goes through one function,
//!   [`notify`], into the brief's `alerts`, which the plugin shows as its toasts.
//!
//! Nothing is booked here: the brief changes nothing of the company.

use super::company::{with, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::clock as ck;
use omsi_launcher_lib::company::control as ctl;
use omsi_launcher_lib::company::lateness as lt;
use omsi_launcher_lib::company::ontime::{self as ot, Call, OtherBus, OtherTrip};
use omsi_launcher_lib::company::{Company, CompanyLine};
use omsihub_plugin::bridge::DATA_DIR;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The plugin's file of where the bus is, and the launcher's brief for it.
pub const LIVE_FILE: &str = "live.json";
pub const BRIEF_FILE: &str = "ontime.json";
/// What the plugin wrote longer ago than this (s) is of a game no longer driven.
const FRESH: f64 = 30.0;
/// The brief is worked out at most this often.
const EVERY: Duration = Duration::from_secs(3);
/// Connections are told this many stops (and at the most this many minutes) ahead.
const CONNECTIONS_AHEAD: usize = 4;
const CONNECTIONS_MINUTES: f64 = 20.0;

/// A stop of the trip under way, as the plugin has it (`omsi.duty.stops`).
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LiveStop {
    pub name: String,
    pub arrival: f64,
    pub departure: f64,
    /// False: the bus passes it.
    pub stops: bool,
}

/// Where the bus is, as the plugin wrote it (`ontime.lua`).
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Live {
    /// The assignment it drives (0: a drive of the player's own).
    pub issued: u64,
    /// Unix seconds it was written, and the game's clock (seconds of the day).
    pub at: f64,
    pub clock: f64,
    /// The duty's line (the timetable's), tour and trip (from 1) and its name.
    pub line: String,
    pub tour: String,
    pub trip: u32,
    pub trip_name: String,
    /// The next stop (an index into `stops`), and whether the bus stands at it.
    pub next: usize,
    pub at_stop: bool,
    /// Seconds late (early negative).
    pub delay: f64,
    pub stops: Vec<LiveStop>,
}

#[derive(Default)]
struct State {
    last: Option<Instant>,
    /// The brief last written, so the same is not written again.
    written: String,
    /// The day's delay model, worked out once a minute of the company's clock and plan.
    model: Option<((String, u32, u64, u64), lt::Model)>,
}

fn state() -> &'static Mutex<State> {
    static S: std::sync::OnceLock<Mutex<State>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// Read where the bus is and write the brief for it (the launcher's poll; cheap when nobody
/// drives: one file looked at every few seconds).
pub fn refresh() {
    let mut st = state().lock().unwrap_or_else(|e| e.into_inner());
    if st.last.is_some_and(|t| t.elapsed() < EVERY) {
        return;
    }
    st.last = Some(Instant::now());
    let Some(dir) = super::addon::drive_plugin_dir() else { return };
    let Some(live) = read_live(&dir, now()) else { return };
    let st = &mut *st;
    let brief = with(|s| brief(s, &live, &mut st.model));
    let Some(brief) = brief else { return };
    let text = serde_json::to_string(&brief).unwrap_or_default();
    // (the time it was written aside, the same brief is not written again)
    let key = text.replace(&format!("\"at\":{}", brief["at"]), "");
    if key == st.written {
        return;
    }
    if write(&dir, &text).is_ok() {
        st.written = key;
    }
}

/// The plugin's `live.json` in `dir`, if it is fresh at `now` (Unix seconds).
pub fn read_live(dir: &Path, now: f64) -> Option<Live> {
    let text = std::fs::read_to_string(dir.join(DATA_DIR).join(LIVE_FILE)).ok()?;
    let live: Live = serde_json::from_str(&text).ok()?;
    ((now - live.at).abs() <= FRESH && !live.line.trim().is_empty()).then_some(live)
}

fn write(dir: &Path, text: &str) -> std::io::Result<()> {
    let path = dir.join(DATA_DIR).join(BRIEF_FILE);
    std::fs::create_dir_all(path.parent().unwrap_or(dir))?;
    let tmp = path.with_extension("omsihub-new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// The ONE way the driver is told something once (a toast in the game: `ontime.lua`'s `alert`).
///
/// When the company's message system comes (r67-messages: `post(company, to, priority,
/// template, args, action)`), this is where it goes: post the same template and args to the
/// driver instead of (or as well as) the brief's `alerts` - the plugin then shows the messages
/// it is sent, and its `alert` stays the fallback of a game without them.
fn notify(out: &mut Vec<Value>, id: String, priority: &str, template: &str, args: Value) {
    out.push(json!({ "id": id, "priority": priority, "template": template, "args": args }));
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn hhmm(secs: f64) -> String {
    let m = (secs / 60.0).floor() as i64;
    format!("{:02}:{:02}", m.div_euclid(60).rem_euclid(24), m.rem_euclid(60))
}

/// The company's line the duty drives: by its timetable name, else by a number it shows.
fn line_of<'a>(c: &'a Company, line: &str) -> Option<&'a CompanyLine> {
    c.lines.iter().find(|l| same(&l.name, line)).or_else(|| c.lines.iter().find(|l| same(&l.number, line) || l.numbers.iter().any(|n| same(n, line))))
}

/// The trip's stops the bus calls at as calls (the first by its departure, the rest by their
/// arrival), each with its index among all the stops.
fn calls_of(stops: &[LiveStop]) -> Vec<(usize, Call, f64)> {
    stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.stops)
        .map(|(k, s)| (k, Call { name: s.name.trim().to_string(), at: if k == 0 { s.departure.max(s.arrival) } else { s.arrival } }, s.departure.max(s.arrival)))
        .collect()
}

/// The other buses of `line` today as the day's delay model runs them, their trips' stops from
/// the timetable (`map_lines`): all but the player's tour.
fn others(s: &Session, c: &Company, line: &CompanyLine, tour: &str, model: &lt::Model) -> Vec<OtherBus> {
    let (Some(plan), Some(lines)) = (s.plan.as_ref(), s.map_lines()) else { return Vec::new() };
    let mut infos: HashMap<(&str, i32), &core::TripInfo> = HashMap::new();
    for t in lines.iter().flat_map(|l| l.tours.iter()).flat_map(|t| t.trips.iter()) {
        infos.entry((t.name.as_str(), (t.departure / 60.0).round() as i32)).or_insert(t);
    }
    let mut out = Vec::new();
    for (ti, tp) in plan.tours.iter().enumerate() {
        if !same(&tp.tour.line, &line.name) || same(&tp.tour.tour, tour) || tp.tour.unplanned || (tp.bus.is_none() && !tp.by_player) {
            continue;
        }
        let row = model.get(ti);
        let mut trips = Vec::new();
        let mut bus = tp.bus;
        for (j, x) in tp.tour.trips.iter().enumerate() {
            if !x.counts() {
                continue;
            }
            let Some(info) = infos.get(&(x.name.as_str(), x.whole_dep())) else { continue };
            let timed = row.and_then(|r| r.get(j)).and_then(|t| t.as_ref());
            // (a trip the model does not run - dropped, or a player's - on time; one not run at all
            // is left out)
            if timed.is_none() && !tp.by_player && !tp.live {
                continue;
            }
            if let Some(t) = timed {
                bus = Some(t.bus);
            }
            let calls = info.stops.iter().enumerate().map(|(k, st)| Call { name: st.name.trim().to_string(), at: if k == 0 { info.departure.max(st.dep) } else { st.arr } }).collect();
            trips.push(OtherTrip { calls, dep: timed.map_or(0.0, |t| t.dep * 60.0), arr: timed.map_or(0.0, |t| t.arr * 60.0) });
        }
        let label = bus.and_then(|b| c.vehicle(b)).map(|v| v.number.clone()).unwrap_or_else(|| tp.tour.tour.clone());
        if !trips.is_empty() {
            out.push(OtherBus { label, trips });
        }
    }
    out
}

/// The brief for the bus where `live` says it is (None: not a company's duty, or no company
/// open). `model` keeps the day's delay model between two briefs.
pub fn brief(s: &Session, live: &Live, model: &mut Option<((String, u32, u64, u64), lt::Model)>) -> Option<Value> {
    let c = s.company.as_ref()?;
    let line = line_of(c, &live.line)?;
    let date = c.date.clone();
    let calls = calls_of(&live.stops);
    // (the next stop the bus calls at, as an index into the calls)
    let next = calls.iter().position(|(k, _, _)| *k >= live.next).unwrap_or(calls.len());
    let mut alerts = Vec::new();
    let mut out = json!({
        "issued": live.issued, "at": now(), "trip": live.trip, "trip_name": live.trip_name,
        "line": line.number, "early_from": ot::EARLY_FROM, "late_from": ot::late_from(),
    });
    // the headway at the next stop, from the other buses where the day's model has them
    if let Some((_, call, _)) = calls.get(next) {
        let key = (date.clone(), c.clock.minute, s.plan_gen, s.generation);
        if let Some(plan) = s.plan.as_ref().filter(|_| model.as_ref().is_none_or(|m| m.0 != key)) {
            let cut = ck::breakdowns(c, plan, &date, &[]);
            let m = lt::model_with(c, plan, &date, &cut, &ctl::of_day(c, &date), Some(c.clock.minute as i32), None);
            *model = Some((key, m));
        }
        let empty = Vec::new();
        let m = model.as_ref().map_or(&empty, |m| &m.1);
        let buses = others(s, c, line, &live.tour, m);
        let after = calls.get(next + 1).map(|x| x.1.name.as_str());
        if let Some(h) = ot::headway(&call.name, after, call.at, live.delay, &buses) {
            match &h.advice {
                ot::Advice::Hold { secs } => notify(&mut alerts, format!("hold|{}|{}", live.trip, call.name), "normal", "Hold %{secs} s at %{stop}: the bus ahead is close", json!({ "secs": secs.round() as i64, "stop": call.name })),
                ot::Advice::CloseBehind { bus } => notify(&mut alerts, format!("close|{}|{}", live.trip, bus), "low", "Close behind %{bus}: no time to hold", json!({ "bus": bus })),
                ot::Advice::None => {}
            }
            out["headway"] = json!(h);
        }
    }
    // the connections at the stops ahead
    let mut connections = Vec::new();
    for (_, call, dep) in calls.iter().skip(next).take(CONNECTIONS_AHEAD) {
        if call.at - (live.clock + live.delay.max(0.0)) > CONNECTIONS_MINUTES * 60.0 {
            break;
        }
        for k in line.connections.iter().filter(|k| k.at(&call.name)) {
            let at = if k.onward { call.at } else { *dep };
            if !k.protects(at) {
                continue;
            }
            let late = ot::late_of(c, &date, &line.name, k);
            let due = (k.time + late) as f64 * 60.0;
            let mut row = json!({ "stop": call.name, "what": k.what, "time": hhmm(k.time as f64 * 60.0), "late": late, "max_hold": k.max_hold, "onward": k.onward });
            if k.onward {
                match ot::onward(call.at + live.delay, due) {
                    ot::Onward::Made { secs } => row["status"] = json!({ "kind": "made", "m": (secs / 60.0).floor() as i64 }),
                    ot::Onward::Tight { .. } => row["status"] = json!({ "kind": "tight" }),
                    ot::Onward::Missed => row["status"] = json!({ "kind": "missed" }),
                }
            } else {
                // (the bus is ready to leave the stop at its departure, its delay counted)
                match ot::hold(*dep, *dep + live.delay, due, k.max_hold as f64 * 60.0) {
                    ot::Hold::NotNeeded => row["status"] = json!({ "kind": "not_needed" }),
                    ot::Hold::Wait { until, secs } => {
                        row["status"] = json!({ "kind": "wait", "until": hhmm(until), "secs": secs.round() as i64 });
                        notify(&mut alerts, format!("conn|{}|{}|{}", live.trip, call.name, k.time), "high", "Connection at %{stop}: wait max %{m} min", json!({ "stop": call.name, "m": k.max_hold }));
                    }
                    ot::Hold::TooLate => row["status"] = json!({ "kind": "too_late" }),
                }
            }
            connections.push(row);
        }
    }
    out["connections"] = json!(connections);
    // the day's diversions and roadworks on the line, and the closed stop ahead
    let list = ot::disruptions(c, &date, &line.name);
    let closed = ot::closed_of(&list);
    let via: Vec<&str> = list.iter().map(|d| d.via.trim()).filter(|v| !v.is_empty()).collect();
    let plain: Vec<Call> = calls.iter().map(|x| x.1.clone()).collect();
    if let Some(k) = ot::closed_ahead(&plain, next, &closed) {
        let stop = plain[k].name.clone();
        out["ahead"] = json!({ "stop": stop, "via": via.join(", ") });
        notify(&mut alerts, format!("closed|{}|{}", live.trip, stop), "high", "Diversion ahead: %{stop} closed", json!({ "stop": stop }));
    }
    for d in &list {
        let template = match d.kind.as_str() {
            "roadworks" => "Roadworks on the line today: expect delays",
            _ if d.closed.is_empty() && d.via.is_empty() => "Diversion on the line today: expect delays",
            _ => "Diversion on the line today",
        };
        notify(&mut alerts, format!("day|{}", d.id), "normal", template, json!({}));
    }
    out["closed"] = json!(closed);
    out["via"] = json!(via.join(", "));
    out["disruptions"] = json!(list);
    out["alerts"] = json!(alerts);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugins_live_file_is_read_when_it_is_fresh() {
        let d = std::env::temp_dir().join(format!("omsihub-ontime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(DATA_DIR)).unwrap();
        assert!(read_live(&d, 1000.0).is_none());
        std::fs::write(d.join(DATA_DIR).join(LIVE_FILE), r#"{"at":990,"line":"Linie5","tour":"2","trip":3,"next":1,"delay":42.5,"stops":[{"name":"A","arrival":100,"departure":120,"stops":true},{"name":"B","arrival":300,"departure":310,"stops":false}],"newer":1}"#).unwrap();
        let l = read_live(&d, 1000.0).unwrap();
        assert_eq!((l.line.as_str(), l.trip, l.next, l.delay, l.stops.len()), ("Linie5", 3, 1, 42.5, 2));
        assert!(read_live(&d, 1100.0).is_none(), "an old one is of a game no longer driven");
        // the stops it calls at: the first by its departure
        let calls = calls_of(&l.stops);
        assert_eq!(calls.len(), 1);
        assert_eq!((calls[0].0, calls[0].1.at), (0, 120.0));
        write(&d, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(d.join(DATA_DIR).join(BRIEF_FILE)).unwrap(), "{}");
    }

    /// A company running line 5 every eight minutes (three tours from 06:00, out A-B-C-D in nine
    /// minutes and back), its day begun at 06:10, the player on tour 2 (06:08) on the way to C.
    fn line_every_eight_minutes() -> (Session, Live) {
        use omsi_launcher_lib::company::{self as co, GameMode};
        use omsi_launcher_lib::{LineInfo, StopInfo, TourInfo, TripInfo};
        let trip = |no: &str, k: usize, dep: f64| {
            let names = if k == 0 { ["A", "B", "C", "D"] } else { ["D", "C", "B", "A"] };
            let stops = names.iter().enumerate().map(|(j, n)| StopInfo { name: n.to_string(), id: j as i64, arr: dep + j as f64 * 180.0, dep: dep + j as f64 * 180.0 }).collect();
            TripInfo { name: format!("5 {no}-{k}"), index: k + 1, line: "5".into(), from: names[0].into(), terminus: names[3].into(), departure: dep, arrival: dep + 540.0, stops, km: 4.0 }
        };
        let tour = |no: &str, from: f64| TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips: vec![trip(no, 0, from), trip(no, 1, from + 900.0)] };
        let l = LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "D".into()], tours: vec![tour("1", 21_600.0), tour("2", 22_080.0), tour("3", 22_560.0)] };
        let mut c = co::found(&co::Founding { name: "OnTime".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        c.lines.push(co::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        let bus = co::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..3 {
            co::market::buy_new(&mut c, &bus, co::market::Payment::Cash, "").unwrap();
        }
        let drivers = |c: &Company| c.staff.iter().filter(|e| co::staff::drives(e)).count();
        let id = c.id.clone();
        while drivers(&c) < 4 {
            for a in co::staff::applicants(&c) {
                if drivers(&c) < 4 && a.licence == co::model::Licence::D {
                    let _ = co::staff::hire(&mut c, &a);
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        c.planning.auto = true;
        let mut s = Session::default();
        s.company = Some(c);
        super::super::company::test_week(&mut s, vec![l.clone()]);
        let date = s.company.as_ref().unwrap().date.clone();
        let dp = super::super::company::plan_of(&mut s, &date).unwrap().unwrap();
        let c = s.company.as_mut().unwrap();
        c.clock.today = Some(ck::script(c, &dp));
        c.clock.minute = 6 * 60 + 10;
        s.today = Some(super::super::company::Today { map: c.map.clone(), date: date.clone(), lines: vec![l], error: None });
        s.plan = Some(dp.to_plan());
        let stops = ["A", "B", "C", "D"].iter().enumerate().map(|(j, n)| LiveStop { name: n.to_string(), arrival: 22_080.0 + j as f64 * 180.0, departure: 22_080.0 + j as f64 * 180.0, stops: true }).collect();
        let live = Live { issued: 7, at: now(), clock: 22_300.0, line: "Linie5".into(), tour: "2".into(), trip: 1, trip_name: "5 2-0".into(), next: 2, at_stop: false, delay: 0.0, stops };
        (s, live)
    }

    #[test]
    fn the_brief_has_the_headway_the_connection_and_the_diversion_ahead() {
        let (mut s, live) = line_every_eight_minutes();
        // a train due at C a minute after the bus, waited for three minutes; D closed today
        let c = s.company.as_mut().unwrap();
        let k = ot::Connection { stop: "C".into(), what: "Train from Erfurt".into(), time: 6 * 60 + 15, max_hold: 3, onward: false };
        c.lines[0].connections = vec![k.clone()];
        let today = c.date.clone();
        ot::add_diversion(c, ot::Diversion { from: today.clone(), lines: vec!["Linie5".into()], closed: vec!["D".into()], via: "Bahnhofstraße".into(), ..Default::default() }).unwrap();
        let late = ot::late_of(c, &today, "Linie5", &k);
        let mut model = None;
        let b = brief(&s, &live, &mut model).expect("a company's duty");
        assert_eq!((b["issued"].as_u64(), b["trip"].as_u64(), b["line"].as_str()), (Some(7), Some(1), Some("5")));
        // the headway at C: the timetable's eight minutes, the buses of tours 1 and 3 there
        let h = &b["headway"];
        assert_eq!(h["planned"].as_f64(), Some(480.0), "{b}");
        assert!(h["ahead"]["bus"].is_string() && h["behind"]["bus"].is_string(), "{h}");
        assert!(model.is_some(), "the day's model is kept for the next brief");
        // the connection at C: waited for while it is within the hold
        let conn = &b["connections"][0];
        assert_eq!((conn["stop"].as_str(), conn["time"].as_str(), conn["late"].as_i64()), (Some("C"), Some("06:15"), Some(late as i64)));
        let alerts: Vec<&str> = b["alerts"].as_array().unwrap().iter().filter_map(|a| a["id"].as_str()).collect();
        if 375 + late + 1 <= 374 + 3 {
            assert_eq!(conn["status"]["kind"].as_str(), Some("wait"), "{conn}");
            assert!(alerts.contains(&"conn|1|C|375"), "{alerts:?}");
        } else {
            assert_eq!(conn["status"]["kind"].as_str(), Some("too_late"), "{conn}");
        }
        // the closed stop ahead, the way round, the day's diversion told once
        assert_eq!((b["ahead"]["stop"].as_str(), b["ahead"]["via"].as_str()), (Some("D"), Some("Bahnhofstraße")));
        assert_eq!(b["closed"], json!(["D"]));
        assert!(alerts.contains(&"closed|1|D") && alerts.contains(&"day|div-1"), "{alerts:?}");
        // not a company's line: no brief
        assert!(brief(&s, &Live { line: "Linie9".into(), ..live.clone() }, &mut None).is_none());
    }

    #[test]
    fn every_template_told_is_a_text_of_the_plugin() {
        // (the module's own code, not this test)
        let src = include_str!("ontime.rs").replace("\r\n", "\n");
        let src = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        let mut n = 0;
        for line in src.lines().filter(|l| l.contains("notify(&mut alerts") || l.trim_start().starts_with("\"roadworks\" =>") || l.trim_start().starts_with("_ if") || l.trim_start().starts_with("_ =>")) {
            for t in line.split('"').skip(1).step_by(2).filter(|t| t.contains(' ')) {
                n += 1;
                assert!(omsihub_plugin::install::TEXTS.contains(&t), "not a plugin text: {t}");
            }
        }
        assert!(n >= 6, "{n} templates read");
    }
}

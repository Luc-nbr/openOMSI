//! The fleet map (`crates/omsi-launcher-ui/src/company/map.rs`): the company's map with every
//! line it runs drawn in its colour, and every bus of today's plan as a marker where it is at
//! the clock. The map's roads and the trips' routes are read by the game program
//! (`co_more_map`: `mapview::hub_fleet`, through the launcher service); the buses are placed
//! here by `company::fleetmap` from the timetable at the clock, behind by the delay the game
//! reported for their tour (the live hook's file, while the game runs) or by the small one the
//! model shows. The clock follows the time of day - the game's while it runs on the company's
//! map, else the company's - or runs by itself, ten or sixty times as fast. A bus picked shows
//! its card: line, tour, duty, driver, delay, passengers and condition.

use super::super::lines::plate;
use super::super::words::{grade, tr, trf};
use super::super::{data, started_at, with, Session};
use super::more;
use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::fleetmap::{self as fm, State, TripShape};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A tour of today with its trips as the map moves its bus.
struct TourShape {
    line: String,
    number: String,
    tour: String,
    colour: String,
    trips: Vec<TripShape>,
    km: Vec<f64>,
}

#[derive(Default)]
pub struct MapState {
    /// Seconds of the day the map shows, and how fast it runs (seconds of the day a second; 0
    /// stopped). `live`: it follows the time of day.
    clock: f64,
    speed: f64,
    live: bool,
    started: bool,
    last: Option<Instant>,
    /// The tour picked: (line, tour).
    picked: Option<(String, String)>,
    tours: Vec<TourShape>,
    /// The company, map, day and lines the tours were built for.
    built_for: Option<(String, String, usize)>,
    legend: Vec<(co::CompanyLine, String)>,
    /// The delays the game reported (line number, tour) → seconds, and when they were read.
    delays: HashMap<(String, String), f64>,
    delays_at: Option<Instant>,
}

/// A line's colour: its own, else the one RosterFlow's chips give it.
fn colour_of_line(c: &Company, line: &str) -> String {
    let [r, g, b] = co::rosterflow::line_colour(c, line);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The trips of the company's lines today (the routes the map is asked for).
fn trips_of(c: &Company, lines: &[core::LineInfo]) -> Vec<String> {
    let mut trips: Vec<String> = Vec::new();
    for cl in &c.lines {
        if let Some(line) = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&cl.name)) {
            for t in line.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()) {
                if !trips.contains(&t.name) {
                    trips.push(t.name.clone());
                }
            }
        }
    }
    trips.sort();
    trips
}

/// The fleet map's roads and its lines (`co_more_map`): read by the game program with the
/// routes of every trip of the company's lines today; the tours' shapes built from them and
/// kept for the markers. The session is not held while the game program reads.
pub fn roads(_args: &Value, p: &dyn Platform) -> Answer {
    let ask = with(|s| {
        let c = s.company.clone()?;
        let lines = s.map_lines()?.to_vec();
        Some((c.map.clone(), c.date.clone(), trips_of(&c, &lines)))
    });
    let Some((map, date, trips)) = ask else { return Ok(json!({ "reading": true })) };
    let (_, body) = p.renderer(json!({ "cmd": "map", "map": map, "date": date, "trip": "", "trips": trips }), Duration::from_secs(180))?;
    let mut v: Value = serde_json::from_slice(&body).map_err(|e| format!("{e:#}"))?;
    let tracks: HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)> = v
        .get("tracks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|t| {
                    let name = t.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                    let flat: Vec<f64> = t.get("p").and_then(Value::as_array).map(|p| p.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                    let pts: Vec<fm::P> = flat.chunks_exact(2).map(|q| [q[0], q[1]]).collect();
                    let stops: Vec<Option<fm::P>> = t.get("stops").and_then(Value::as_array).map(|s| s.iter().map(|x| x.as_array().filter(|q| q.len() == 2).map(|q| [q[0].as_f64().unwrap_or(0.0), q[1].as_f64().unwrap_or(0.0)])).collect()).unwrap_or_default();
                    (name, (pts, stops))
                })
                .collect()
        })
        .unwrap_or_default();
    let lines = with(|s| {
        let c = s.company.clone()?;
        if c.map != map || c.date != date {
            return None;
        }
        let lines = s.map_lines()?.to_vec();
        let drawn = build(s, &c, &lines, &tracks);
        Some(drawn)
    });
    if let Some(o) = v.as_object_mut() {
        o.remove("tracks");
    }
    v["lines"] = json!(lines.unwrap_or_default());
    v["map"] = json!(map);
    v["date"] = json!(date);
    Ok(v)
}

/// Build the tours' shapes from the routes read (`map::build`); answers the lines as the map
/// draws them: each trip's route once, in its line's colour.
fn build(s: &Session, c: &Company, lines: &[core::LineInfo], tracks: &HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)>) -> Vec<Value> {
    let mut shapes: HashMap<String, TripShape> = HashMap::new();
    let mut tours = Vec::new();
    let mut drawn = Vec::new();
    let mut legend = Vec::new();
    for cl in &c.lines {
        let Some(line) = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&cl.name)) else { continue };
        let colour = colour_of_line(c, &cl.name);
        legend.push((cl.clone(), colour.clone()));
        let mut seen: Vec<String> = Vec::new();
        for t in line.tours.iter().filter(|t| t.runs) {
            let mut trips = Vec::new();
            let mut km = Vec::new();
            for trip in &t.trips {
                let base = shapes.entry(trip.name.clone()).or_insert_with(|| {
                    let (route, places) = tracks.get(&trip.name).cloned().unwrap_or_default();
                    let mut places = places;
                    places.resize(trip.stops.len(), None);
                    TripShape::new(trip, (route.len() >= 2).then_some(route.as_slice()), places)
                });
                // (this tour's times on the shape)
                let mut sh = base.clone();
                sh.dep = trip.departure;
                sh.arr = trip.arrival.max(trip.departure);
                sh.times = trip.stops.iter().enumerate().map(|(k, x)| if k == 0 { trip.departure.max(x.dep) } else { x.arr }).collect();
                sh.empty = trip.stops.len() < 3;
                trips.push(sh);
                km.push(trip.km);
                if !seen.contains(&trip.name) {
                    seen.push(trip.name.clone());
                    if let Some((route, _)) = tracks.get(&trip.name).filter(|r| r.0.len() >= 2) {
                        drawn.push(json!({ "p": route.iter().flat_map(|q| [q[0], q[1]]).collect::<Vec<f64>>(), "colour": colour }));
                    }
                }
            }
            tours.push(TourShape { line: cl.name.clone(), number: cl.number.clone(), tour: t.number.clone(), colour: colour.clone(), trips, km });
        }
    }
    let mut m = more(s);
    m.map.tours = tours;
    m.map.legend = legend;
    m.map.built_for = Some((c.map.clone(), c.date.clone(), c.lines.len()));
    drawn
}

/// The time of day the map follows (seconds) and whether it is the game's (`map::time_of_day`):
/// the game's while it runs on the company's map (from the time it was started at), else the
/// company's clock. A test drive at the dealer's runs outside the company's time.
fn time_of_day(s: &mut Session, c: &Company) -> (f64, bool) {
    let unix = super::super::real_now().max(0) as u64;
    let norm = |m: &str| m.replace('\\', "/");
    let same = |m: &str| norm(m).eq_ignore_ascii_case(&norm(&c.map));
    let test = |i: &core::Instance| i.line.is_none() && c.dealer.test_drive.as_ref().is_some_and(|t| norm(&i.bus).eq_ignore_ascii_case(&norm(&t.bus)));
    if let Some(i) = s.instances().iter().find(|i| i.running && same(&i.map) && !test(i)) {
        let start = started_at(&i.args).unwrap_or(0) as f64 * 60.0;
        return ((start + unix.saturating_sub(i.started) as f64).rem_euclid(86_400.0), true);
    }
    let now = co::clock::now(c);
    (co::clock::minute_of(now) as f64 * 60.0, false)
}

/// Where the shape's trip `k` is among the plan's trips of its tour (`map::plan_trip`).
fn plan_trip(tp: &co::day::TourPlan, trips: &[TripShape], k: usize) -> usize {
    let Some(dep) = trips.get(k).map(|s| (s.dep / 60.0).round() as i32) else { return k };
    tp.tour.trips.iter().position(|x| x.dep == dep && !(x.empty && x.stops == 0 && x.line.is_empty())).unwrap_or(k)
}

fn hhmmss(s: f64) -> String {
    let s = s.rem_euclid(86_400.0) as i64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn hhmm(s: f64) -> String {
    let m = (s / 60.0).floor() as i64;
    format!("{:02}:{:02}", m.div_euclid(60).rem_euclid(24), m.rem_euclid(60))
}

/// "+2:10", "−0:45".
fn offset(s: f64) -> String {
    let a = s.abs().round() as i64;
    format!("{}{}:{:02}", if s < 0.0 { "−" } else { "+" }, a / 60, a % 60)
}

/// The delays the game reported for the company's tours (the live hook's file), read every few
/// seconds while the game runs.
fn live_delays(m: &mut MapState, c: &Company, in_game: bool) {
    if !in_game {
        m.delays.clear();
        return;
    }
    if m.delays_at.is_some_and(|t| t.elapsed().as_secs_f32() < 5.0) {
        return;
    }
    m.delays_at = Some(Instant::now());
    let Ok(text) = std::fs::read_to_string(co::store::live_file(&data(), &c.id)) else { return };
    for ev in text.lines().filter_map(|x| serde_json::from_str::<co::day::LiveEvent>(x).ok()) {
        if let co::day::LiveEvent::Trip { line, tour, delay, .. } = ev {
            m.delays.insert((line.to_lowercase(), tour.trim().to_string()), delay);
        }
    }
}

/// A bus on the map now.
struct Marker {
    k: usize,
    place: fm::Place,
    delay: f64,
}

/// The fleet map's view at its clock (asked for a few times a second): the clock, the lines
/// with their buses on the road, the markers, and the card of the bus picked.
pub fn view(s: &mut Session, _args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    if s.map_lines().is_none() {
        return json!({ "reading": tr("Reading the timetable…") });
    }
    let (now, in_game) = time_of_day(s, &c);
    let plan = s.plan.clone();
    let mut m = more(s);
    let m = &mut m.map;
    let built = m.built_for.as_ref().is_some_and(|b| b.0 == c.map && b.1 == c.date && b.2 == c.lines.len());
    // the clock
    if !m.started {
        m.started = true;
        m.live = true;
        m.clock = now;
    }
    let dt = m.last.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0).min(1.0);
    m.last = Some(Instant::now());
    if m.live {
        m.clock = now;
    } else if m.speed > 0.0 {
        m.clock = (m.clock + dt * m.speed).rem_euclid(86_400.0);
    }
    live_delays(m, &c, in_game);
    // the buses
    let clock = m.clock;
    let mut markers: Vec<Marker> = Vec::new();
    for (k, t) in m.tours.iter().enumerate() {
        let tp = plan.as_ref().and_then(|p| p.tours.iter().find(|x| x.tour.line.eq_ignore_ascii_case(&t.line) && x.tour.tour == t.tour));
        // (a tour without a bus is dropped today: it has no marker)
        if tp.is_some_and(|p| !p.covered()) {
            continue;
        }
        let refs: Vec<&TripShape> = t.trips.iter().collect();
        let first = fm::place_at(&refs, clock, 0.0);
        let delay = match m.delays.get(&(t.number.to_lowercase(), t.tour.trim().to_string())) {
            Some(d) => *d,
            None => {
                let bus = tp.and_then(|p| p.bus).and_then(|b| c.vehicle(b));
                let driver = tp
                    .and_then(|p| {
                        let at = plan_trip(p, &t.trips, first.trip);
                        p.duties.iter().find(|d| (d.start..d.end).contains(&at)).and_then(|d| d.driver)
                    })
                    .and_then(|id| c.employee(id));
                fm::delay_of(&c.date, &format!("{}/{}", t.line, t.tour), first.trip, driver.map(|e| e.experience).unwrap_or(60.0), bus.map(|v| v.condition).unwrap_or(90.0))
            }
        };
        let place = fm::place_at(&refs, clock, delay);
        // (in the depot until half an hour before it leaves, and after its day)
        let shown = match place.state {
            State::Depot => t.trips.first().is_some_and(|f| f.dep - clock <= 1800.0),
            State::Done => false,
            _ => true,
        };
        if shown && place.at.is_some() {
            markers.push(Marker { k, place, delay });
        }
    }
    let picked = m.picked.clone();
    let dots: Vec<Value> = markers
        .iter()
        .map(|mk| {
            let t = &m.tours[mk.k];
            let on = picked.as_ref().is_some_and(|p| p.0 == t.line && p.1 == t.tour);
            let at = mk.place.at.unwrap_or([0.0, 0.0]);
            json!({ "at": at, "colour": t.colour, "faded": mk.place.state != State::Trip, "on": on, "label": format!("{} / {}", t.number, t.tour), "line": t.line, "tour": t.tour })
        })
        .collect();
    // the panel
    let on_road = markers.iter().filter(|mk| matches!(mk.place.state, State::Trip | State::Empty)).count();
    let pausing = markers.iter().filter(|mk| mk.place.state == State::Pause).count();
    let counts = trf("On the road: %{n}  ·  at a terminus: %{p}  ·  tours today: %{t}", &[("n", &on_road.to_string()), ("p", &pausing.to_string()), ("t", &m.tours.len().to_string())]);
    let legend: Vec<Value> = m
        .legend
        .iter()
        .map(|(line, colour)| {
            let n = markers.iter().filter(|mk| m.tours[mk.k].line == line.name).count();
            let caption = if line.caption.is_empty() { line.name.clone() } else { line.caption.clone() };
            json!({ "plate": plate(&line.number, &line.colour), "colour": colour, "caption": caption, "n": n })
        })
        .collect();
    let card = picked.as_ref().map(|(line, tour)| bus_card(&c, plan.as_ref(), m, &markers, line, tour));
    json!({
        "built": built,
        "title": tr("Fleet map").to_uppercase(),
        "live": m.live.then(|| json!({ "text": tr(if in_game { "Live: the game's time" } else { "Live: the company's time" }), "tone": if in_game { "ok" } else { "accent2" } })),
        "clock": hhmmss(clock),
        "playing": !m.live && m.speed > 0.0, "is_live": m.live, "speed": m.speed,
        "tips": { "run": tr("Run the clock"), "stop": tr("Stop the clock"), "x10": tr("Ten times as fast"), "x60": tr("Sixty times as fast"), "now": tr("Back to now"), "close": tr("Close") },
        "counts": counts,
        "legend": legend, "no_lines": tr("The company runs no line on this day."),
        "dots": dots,
        "card": card,
    })
}

/// The card of the bus picked (`map::bus_card`).
fn bus_card(c: &Company, plan: Option<&co::day::Plan>, m: &MapState, markers: &[Marker], line: &str, tour: &str) -> Value {
    let Some(t) = m.tours.iter().find(|t| t.line == line && t.tour == tour) else { return Value::Null };
    let cl = c.lines.iter().find(|x| x.name == line);
    let head = json!({
        "plate": match cl { Some(x) => plate(&x.number, &x.colour), None => plate(&t.number, "") },
        "colour": t.colour, "title": format!("{} {}", tr("Tour"), tour),
    });
    let mk = markers.iter().find(|mk| {
        let x = &m.tours[mk.k];
        x.line == line && x.tour == tour
    });
    let Some(mk) = mk else {
        let first = t.trips.first().map(|f| f.dep).unwrap_or(0.0);
        let text = if m.clock < first { trf("In the depot: leaves at %{time}", &[("time", &hhmm(first))]) } else { tr("Its day is done.") };
        return json!({ "head": head, "away": text });
    };
    let trip = t.trips.get(mk.place.trip);
    let next = trip.and_then(|x| mk.place.next_stop.and_then(|k| x.names.get(k)));
    let what = match mk.place.state {
        State::Trip => trf("On its way to %{stop}", &[("stop", &trip.and_then(|x| x.names.last()).cloned().unwrap_or_default())]),
        State::Empty => tr("Empty run"),
        State::Pause => trf("At the terminus until %{time}", &[("time", &hhmm(mk.place.next_time.unwrap_or(0.0)))]),
        State::Depot => trf("Leaves the depot at %{time}", &[("time", &hhmm(mk.place.next_time.unwrap_or(0.0)))]),
        State::Done => tr("Its day is done."),
    };
    let next_text = match (next, mk.place.next_time) {
        (Some(n), Some(tm)) => Some(trf("Next: %{stop} at %{time}", &[("stop", n), ("time", &hhmm(tm))])),
        _ => None,
    };
    let tp = plan.and_then(|p| p.tours.iter().find(|x| x.tour.line.eq_ignore_ascii_case(line) && x.tour.tour == tour));
    let bus = tp.and_then(|p| p.bus).and_then(|b| c.vehicle(b));
    let duty = tp.and_then(|p| {
        let at = plan_trip(p, &t.trips, mk.place.trip);
        p.duties.iter().position(|d| (d.start..d.end).contains(&at))
    });
    let driver = tp.and_then(|p| duty.and_then(|k| p.duties.get(k)).and_then(|d| d.driver)).and_then(|id| c.employee(id));
    let by_player = tp.is_some_and(|p| p.by_player);
    let duty_text = match (duty, tp) {
        (Some(k), Some(p)) => format!("{} / {}", k + 1, p.duties.len()),
        _ => "—".into(),
    };
    let late = mk.delay > 180.0;
    let pax = match (trip, t.km.get(mk.place.trip)) {
        (Some(x), Some(k)) if !x.empty => {
            let r = co::economy::rules_for(c);
            format!("≈ {:.0}", co::economy::passengers_for(*k, (x.dep / 60.0) as i32, &r))
        }
        _ => "—".into(),
    };
    let mut rows = vec![
        json!({ "label": tr("Bus"), "value": bus.map(|v| format!("{} {}", v.number, v.name)).unwrap_or_else(|| "—".into()), "tone": "text" }),
        json!({ "label": tr("Duty"), "value": duty_text, "tone": "text" }),
        json!({ "label": tr("Driver"), "value": if by_player { tr("You") } else { driver.map(|e| e.name.clone()).unwrap_or_else(|| "—".into()) }, "tone": "text" }),
        json!({ "label": tr("Delay"), "value": offset(mk.delay), "tone": if late { "warn" } else if mk.delay < -60.0 { "early" } else { "ok" } }),
        json!({ "label": tr("Passengers this trip"), "value": pax, "tone": "text" }),
    ];
    if let Some(v) = bus {
        rows.push(json!({ "label": tr("Condition"), "value": format!("{:.0} %", v.condition), "tone": grade(v.condition), "meter": v.condition / 100.0 }));
    }
    json!({ "head": head, "what": what, "next": next_text, "rows": rows })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let mut m = more(s);
    let m = &mut m.map;
    match what {
        // play, ×10, ×60 and back to now (`map::side`)
        "map_play" => {
            if m.live || m.speed > 0.0 {
                m.live = false;
                m.speed = 0.0;
            } else {
                m.speed = 1.0;
            }
        }
        "map_speed" => {
            m.live = false;
            m.speed = args.get("speed").and_then(Value::as_f64).unwrap_or(10.0);
        }
        "map_now" => {
            m.live = true;
            m.speed = 0.0;
        }
        "map_pick" => {
            let line = args.get("line").and_then(Value::as_str).map(str::to_string);
            let tour = args.get("tour").and_then(Value::as_str).map(str::to_string);
            m.picked = line.zip(tour);
        }
        _ => return None,
    }
    Some(Ok(json!({})))
}

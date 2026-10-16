//! The fleet map (`crates/omsi-launcher-ui/src/company/map.rs`): the company's map with every
//! line it runs drawn in its colour, and every bus of today's plan where it is at the clock. The
//! map's roads and the trips' routes are read by the game program (`co_more_map`:
//! `mapview::hub_fleet`, through the launcher service); the buses are placed here, block by block
//! of today's plan, by `company::fleetmap` (Luc: "a smooth display of all the buses the company
//! has driving ... a small screen pops up and follows the bus"):
//!
//! - a bus of the company's own drives its trips as the day's one delay model runs them
//!   (`lateness::model_with`, the Operations page's): late leaving, losing (or making up) time on
//!   the way, waiting at the terminus between trips, an empty run along its route or straight;
//!   a bus broken down stands where it broke down, and the bus sent runs the block on;
//! - the bus the player drives is where the game says it is (its telemetry, `telemetry.json`,
//!   while the player has it on), else on the timetable by the delay the game reported.
//!
//! The clock follows the time of day - the game's while it runs on the company's map, else the
//! company's to the second - or runs by itself, ten or sixty times as fast. The view is asked for
//! a few times a second and is cheap: the model is worked out once a minute of the company's
//! clock, the blocks once a plan. A bus picked has its card: the bus, its line, trip, block,
//! duty and driver, the next stop, the delay and why, what it carries and earned today, and what
//! happened to it (read only: the map changes nothing, so a friend of "Play together" may see it).

use super::super::late::{block as block_name, cause_kind, cause_text};
use super::super::lines::plate;
use super::super::ops::OPS_TAB;
use super::super::words::{eur, grade, num, tr, trf};
use super::super::{data, started_at, Session};
use super::more;
use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::clock::{self as ck, Choice, Cut};
use omsi_launcher_lib::company::control::{self as ctl, Act};
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::fleetmap::{self as fm, Doing, Live, Run, TripShape};
use omsi_launcher_lib::company::lateness::{self as lt, Model};
use omsi_launcher_lib::company::network::PlannedTrip;
use omsi_launcher_lib::company::plan::{AGENCY, PLAYER};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_launcher_lib::service::ServiceKind;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A bus waiting for its first trip is shown from this many seconds before it leaves.
const SHOWN_BEFORE: f64 = 1800.0;
/// The game's telemetry is read at most this often, and believed for this long.
const TELEMETRY_EVERY: f64 = 0.4;
const TELEMETRY_FRESH: f64 = 10.0;

/// A block of today's plan as the map moves its bus.
struct Block {
    /// Its place among the plan's tours (`day::Plan::tours`): the lateness model's too.
    ti: usize,
    line: String,
    number: String,
    tour: String,
    trips: Vec<TripShape>,
    km: Vec<f64>,
    /// Each trip's number on its displays and its line's colour.
    tags: Vec<(String, String)>,
}

/// The game's telemetry of the player's bus (`omsi-app`'s `telemetry.rs`), in the map's metres.
#[derive(Clone, Debug, Default)]
struct Telemetry {
    at: [f64; 2],
    heading: f64,
    delay: Option<f64>,
    passengers: Option<f64>,
    speed: f64,
    line: String,
    tour: String,
}

/// The day's delays as the lateness model has them at a minute of the company's clock: the
/// model, the buses cut short today, and what it was worked out for.
struct Day {
    key: (String, u32, u64, u64),
    model: Model,
    cut: Vec<Cut>,
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
    /// The bus picked (`Marker::key`).
    picked: Option<String>,
    /// The trips' routes and their stops' places as the game program read them, by trip.
    routes: HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)>,
    /// Where the map's metres begin, in the game's world (the telemetry's).
    origin: Option<[f64; 2]>,
    /// The company, map and day the routes were read for.
    built_for: Option<(String, String, usize)>,
    legend: Vec<(co::CompanyLine, String)>,
    /// Today's blocks, made for a plan (`Session::plan_gen`) and the routes read.
    blocks: Vec<Block>,
    blocks_for: Option<(u64, String, usize)>,
    day: Option<Day>,
    /// The delays the game reported (line number, tour) → seconds, and when they were read.
    delays: HashMap<(String, String), f64>,
    delays_at: Option<Instant>,
    telemetry: Option<Telemetry>,
    telemetry_at: Option<Instant>,
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
/// routes of every trip of the company's lines today, kept for the buses. The session is not
/// held while the game program reads.
pub fn roads(_args: &Value, p: &dyn Platform) -> Answer {
    let ask = super::super::with(|s| {
        let c = s.company.clone()?;
        let lines = s.map_lines()?.to_vec();
        Some((c.map.clone(), c.date.clone(), trips_of(&c, &lines)))
    });
    let Some((map, date, trips)) = ask else { return Ok(json!({ "reading": true })) };
    let (_, body) = p.renderer(json!({ "cmd": "map", "map": map, "date": date, "trip": "", "trips": trips }), Duration::from_secs(180))?;
    let mut v: Value = serde_json::from_slice(&body).map_err(|e| format!("{e:#}"))?;
    let routes: HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)> = v
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
    let origin = v.get("origin").and_then(Value::as_array).filter(|o| o.len() == 2).map(|o| [o[0].as_f64().unwrap_or(0.0), o[1].as_f64().unwrap_or(0.0)]);
    let lines = super::super::with(|s| {
        let c = s.company.clone()?;
        if c.map != map || c.date != date {
            return None;
        }
        let lines = s.map_lines()?.to_vec();
        Some(keep(s, &c, &lines, routes, origin))
    });
    if let Some(o) = v.as_object_mut() {
        o.remove("tracks");
    }
    v["lines"] = json!(lines.unwrap_or_default());
    v["map"] = json!(map);
    v["date"] = json!(date);
    Ok(v)
}

/// Keep the routes read for the buses (`map::build`); answers the lines as the map draws them:
/// each trip's route once, in its line's colour.
fn keep(s: &Session, c: &Company, lines: &[core::LineInfo], routes: HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)>, origin: Option<[f64; 2]>) -> Vec<Value> {
    let mut drawn = Vec::new();
    let mut legend = Vec::new();
    for cl in &c.lines {
        let Some(line) = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&cl.name)) else { continue };
        let colour = colour_of_line(c, &cl.name);
        legend.push((cl.clone(), colour.clone()));
        let mut seen: Vec<&str> = Vec::new();
        for trip in line.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()) {
            if seen.contains(&trip.name.as_str()) {
                continue;
            }
            seen.push(&trip.name);
            if let Some((route, _)) = routes.get(&trip.name).filter(|r| r.0.len() >= 2) {
                drawn.push(json!({ "p": route.iter().flat_map(|q| [q[0], q[1]]).collect::<Vec<f64>>(), "colour": colour }));
            }
        }
    }
    let mut m = more(s);
    m.map.routes = routes;
    m.map.origin = origin;
    m.map.legend = legend;
    m.map.blocks_for = None;
    m.map.built_for = Some((c.map.clone(), c.date.clone(), c.lines.len()));
    drawn
}

/// A stop's name as the places are looked up by it.
fn stop_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Today's blocks as the map moves their buses: each trip of the plan with its route and its
/// stops' times from the timetable (a part of a trip cut at a relief point: from or to that
/// stop), a trip the timetable does not have (a depot run the company added) from its first stop
/// to its last, where the map has them.
fn blocks_of(c: &Company, plan: &Plan, lines: &[core::LineInfo], routes: &HashMap<String, (Vec<fm::P>, Vec<Option<fm::P>>)>) -> Vec<Block> {
    let mut infos: HashMap<(&str, i32), &core::TripInfo> = HashMap::new();
    let mut places: HashMap<String, fm::P> = HashMap::new();
    for t in lines.iter().flat_map(|l| l.tours.iter()).flat_map(|t| t.trips.iter()) {
        infos.entry((t.name.as_str(), (t.departure / 60.0).round() as i32)).or_insert(t);
        if let Some((_, at)) = routes.get(&t.name) {
            for (s, p) in t.stops.iter().zip(at) {
                if let Some(p) = p {
                    places.entry(stop_key(&s.name)).or_insert(*p);
                }
            }
        }
    }
    let mut shapes: HashMap<&str, TripShape> = HashMap::new();
    let mut out = Vec::new();
    for (ti, tp) in plan.tours.iter().enumerate() {
        if tp.tour.unplanned || tp.tour.trips.is_empty() {
            continue;
        }
        let colour = colour_of_line(c, &tp.tour.line);
        let mut trips = Vec::new();
        let mut tags = Vec::new();
        for x in &tp.tour.trips {
            let shape = match infos.get(&(x.name.as_str(), x.whole_dep())) {
                Some(info) => {
                    let base = shapes.entry(info.name.as_str()).or_insert_with(|| {
                        let (route, at) = routes.get(&info.name).cloned().unwrap_or_default();
                        TripShape::new(info, (route.len() >= 2).then_some(route.as_slice()), at)
                    });
                    let mut sh = base.clone();
                    sh.times = info.stops.iter().enumerate().map(|(k, s)| if k == 0 { info.departure.max(s.dep) } else { s.arr }).collect();
                    sh.dep = info.departure;
                    sh.arr = info.arrival.max(info.departure);
                    if let Some(part) = &x.part {
                        let cut = sh.times.get(part.stop).copied().unwrap_or(if part.second { sh.dep } else { sh.arr });
                        if part.second {
                            sh.dep = cut;
                        } else {
                            sh.arr = cut;
                        }
                    }
                    sh.empty = !x.counts();
                    sh
                }
                None => {
                    let last = if x.last.trim().is_empty() { &x.to } else { &x.last };
                    let (dep, arr) = (x.dep as f64 * 60.0, x.arr.max(x.dep) as f64 * 60.0);
                    TripShape { dep, arr, times: vec![dep, arr], names: vec![x.from.clone(), last.clone()], track: None, along: vec![None, None], places: vec![places.get(&stop_key(&x.from)).copied(), places.get(&stop_key(last)).copied()], empty: !x.counts() }
                }
            };
            trips.push(shape);
            tags.push(tag_of(c, tp.tour.number.as_str(), &colour, x));
        }
        out.push(Block { ti, line: tp.tour.line.clone(), number: tp.tour.number.clone(), tour: tp.tour.tour.clone(), km: tp.tour.trips.iter().map(|x| x.km).collect(), trips, tags });
    }
    out
}

/// The number a trip shows and its line's colour (a block may run trips of several lines).
fn tag_of(c: &Company, number: &str, colour: &str, x: &PlannedTrip) -> (String, String) {
    let shown = x.line.trim();
    if shown.is_empty() || shown.eq_ignore_ascii_case(number) {
        return (number.to_string(), colour.to_string());
    }
    match c.lines.iter().find(|l| l.number.trim().eq_ignore_ascii_case(shown)) {
        Some(l) => (shown.to_string(), colour_of_line(c, &l.name)),
        None => (shown.to_string(), colour.to_string()),
    }
}

/// The real time now, in Unix seconds to the millisecond.
fn real_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The time of day the map follows (seconds) and whether it is the game's (`map::time_of_day`):
/// the game's while it runs on the company's map (from the time it was started at), else the
/// company's clock - to the second while it runs with the real clock. A test drive at the
/// dealer's runs outside the company's time.
fn time_of_day(s: &mut Session) -> (f64, bool) {
    let Some(c) = s.company.as_ref() else { return (0.0, false) };
    let norm = |m: &str| m.replace('\\', "/");
    let map = norm(&c.map);
    let test = c.dealer.test_drive.as_ref().map(|t| norm(&t.bus));
    let real = real_secs();
    let game = s.instances().iter().find(|i| i.running && norm(&i.map).eq_ignore_ascii_case(&map) && !(i.line.is_none() && test.as_ref().is_some_and(|t| norm(&i.bus).eq_ignore_ascii_case(t)))).map(|i| (started_at(&i.args).unwrap_or(0) as f64 * 60.0, i.started));
    if let Some((start, started)) = game {
        return ((start + (real - started as f64).max(0.0)).rem_euclid(86_400.0), true);
    }
    let Some(c) = s.company.as_ref() else { return (0.0, false) };
    let now = ck::now(c);
    let mut secs = ck::minute_of(now) as f64 * 60.0;
    // (in step with the real clock: the seconds of its minute too)
    if let Some(sync) = c.clock.real.synced.filter(|_| ck::course(c, real as i64) == Some(now) && c.clock.ask.is_none()) {
        secs += (real - sync.real as f64).max(0.0).rem_euclid(60.0);
    }
    (secs, false)
}

/// Where the shape's trip `k` is among the plan's trips of its tour (`map::plan_trip`): the
/// blocks are the plan's own, so the same.
fn duty_of(tp: &co::day::TourPlan, k: usize) -> Option<usize> {
    tp.duties.iter().position(|d| (d.start..d.end).contains(&k))
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

/// The tone of a delay as the late running's pages colour it (`economy::late_tier`), a bus more
/// than a minute early its own.
fn delay_tone(s: f64) -> &'static str {
    if s < -60.0 {
        return "early";
    }
    super::super::late::tier_tone(co::economy::late_tier((s / 60.0 + 1e-6).floor() as i32))
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
    for ev in text.lines().filter_map(|x| serde_json::from_str::<co::day::LiveEvent>(x.trim()).ok()) {
        if let co::day::LiveEvent::Trip { line, tour, delay, .. } = ev {
            m.delays.insert((line.to_lowercase(), tour.trim().to_string()), delay);
        }
    }
}

/// The game's telemetry of the player's bus, read a few times a second while the game runs on
/// the company's map and the player has it on (the file is there): None when it is old, paused
/// on a menu without a bus, or the map's origin is not known.
fn telemetry(m: &mut MapState, in_game: bool) {
    if !in_game || m.origin.is_none() {
        m.telemetry = None;
        return;
    }
    if m.telemetry_at.is_some_and(|t| t.elapsed().as_secs_f64() < TELEMETRY_EVERY) {
        return;
    }
    m.telemetry_at = Some(Instant::now());
    let read = std::fs::read_to_string(data().join("telemetry.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    m.telemetry = read.and_then(|v| telemetry_of(&v, m.origin.unwrap_or_default(), real_secs()));
}

/// The player's bus of a telemetry file (`v`), in the map's metres from `origin`, if it is fresh
/// at `now` (Unix seconds).
fn telemetry_of(v: &Value, origin: [f64; 2], now: f64) -> Option<Telemetry> {
    let updated = v.get("updated").and_then(Value::as_f64)?;
    if (now - updated).abs() > TELEMETRY_FRESH {
        return None;
    }
    let p = v.get("player").filter(|p| p.is_object())?;
    let f = |k: &str| p.get(k).and_then(Value::as_f64);
    let duty = v.get("duty");
    let s = |k: &str| duty.and_then(|d| d.get(k)).and_then(Value::as_str).unwrap_or("").trim().to_string();
    Some(Telemetry {
        at: [f("x")? - origin[0], f("y")? - origin[1]],
        heading: f("heading").unwrap_or(0.0).rem_euclid(360.0),
        delay: f("delay_s"),
        passengers: f("passengers"),
        speed: f("speed_kmh").unwrap_or(0.0),
        line: s("line"),
        tour: s("tour"),
    })
}

/// Whether the player's duty in the game is block `b` (its line and its tour, or a tour of the
/// timetable it runs).
fn drives(t: &Telemetry, b: &Block, tp: &co::day::TourPlan) -> bool {
    !t.line.is_empty() && (t.line.eq_ignore_ascii_case(b.number.trim()) || b.tags.iter().any(|x| x.0.eq_ignore_ascii_case(&t.line))) && (t.tour.eq_ignore_ascii_case(b.tour.trim()) || tp.tour.runs_tour(&t.tour))
}

/// The day's delays at the company clock's minute, worked out again when the minute, the plan
/// or the company changed.
fn day_of<'a>(m: &'a mut Option<Day>, s: &Session, c: &Company, plan: &Plan) -> &'a Day {
    let key = (c.date.clone(), c.clock.minute, s.plan_gen, s.generation);
    if m.as_ref().is_none_or(|d| d.key != key) {
        let cut = ck::breakdowns(c, plan, &c.date, &[]);
        let controls = ctl::of_day(c, &c.date);
        let model = lt::model_with(c, plan, &c.date, &cut, &controls, Some(c.clock.minute as i32), None);
        *m = Some(Day { key, model, cut });
    }
    m.as_ref().expect("made above")
}

/// A bus on the map now.
struct Marker {
    /// "line|tour", "line|tour|broken" for a block's bus broken down.
    key: String,
    /// Its block (`MapState::blocks`).
    b: usize,
    live: Live,
    /// The bus (None: a rental, or none known).
    vehicle: Option<u32>,
    rental: bool,
    /// The player drives it, and the game said where it is.
    player: bool,
    telemetry: Option<Telemetry>,
}

impl Marker {
    fn shown(&self, clock: f64) -> bool {
        self.live.at.is_some()
            && match self.live.doing {
                Doing::Depot => self.live.next_time.is_some_and(|t| t - clock <= SHOWN_BEFORE),
                Doing::Done => false,
                _ => true,
            }
    }
}

/// How each trip of a block runs: the lateness model's, the player's tours by the delay the game
/// reported (on time when it did not).
fn runs_of(m: &MapState, day: &Day, tp: &co::day::TourPlan, b: &Block) -> Vec<Run> {
    if tp.by_player {
        let d = m.delays.get(&(b.number.to_lowercase(), b.tour.trim().to_string())).copied().unwrap_or(0.0);
        return vec![Run::late(d, d); b.trips.len()];
    }
    let row = day.model.get(b.ti);
    (0..b.trips.len()).map(|j| row.and_then(|r| r.get(j)).and_then(|x| x.as_ref()).map_or(Run::default(), |t| Run::late(t.dep * 60.0, t.arr * 60.0))).collect()
}

/// The fleet map's view at its clock (asked for a few times a second): the clock, the lines
/// with their buses on the road, the buses, and the card of the bus picked.
pub fn view(s: &mut Session, _args: &Value) -> Value {
    if s.company.is_none() {
        return Value::Null;
    }
    if s.map_lines().is_none() {
        return json!({ "reading": tr("Reading the timetable…") });
    }
    let (now, in_game) = time_of_day(s);
    let s: &Session = s;
    let (Some(c), Some(lines)) = (s.company.as_ref(), s.map_lines()) else { return Value::Null };
    let mut g = more(s);
    let m = &mut g.map;
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
    live_delays(m, c, in_game);
    telemetry(m, in_game);
    // the blocks of today's plan, and how its trips run
    let mut markers: Vec<Marker> = Vec::new();
    let clock = m.clock;
    if let (Some(plan), true) = (s.plan.as_ref(), built) {
        let made_for = (s.plan_gen, c.date.clone(), m.routes.len());
        if m.blocks_for.as_ref() != Some(&made_for) {
            m.blocks = blocks_of(c, plan, lines, &m.routes);
            m.blocks_for = Some(made_for);
        }
        let mut day = m.day.take();
        let d = day_of(&mut day, s, c, plan);
        for (bi, b) in m.blocks.iter().enumerate() {
            let Some(tp) = plan.tours.get(b.ti).filter(|tp| tp.tour.line == b.line && tp.tour.tour == b.tour) else { continue };
            // (a block without a bus is dropped today: it has no bus on the map)
            if tp.bus.is_none() && !tp.by_player {
                continue;
            }
            let runs = runs_of(m, d, tp, b);
            let refs: Vec<&TripShape> = b.trips.iter().collect();
            let cut = tp.bus.and_then(|v| d.cut.iter().find(|x| x.vehicle == v));
            let (mut live, broken) = fm::block_at(&refs, &runs, clock, cut.map(|x| x.at as f64 * 60.0));
            let key = format!("{}|{}", b.line, b.tour);
            // (the bus running it: the model's - a spare sent, one swapped in -, a rental after a
            // breakdown without a spare)
            let model_bus = d.model.get(b.ti).and_then(|r| r.get(live.trip)).and_then(|x| x.as_ref()).map(|t| t.bus);
            let after_break = broken.is_some();
            let vehicle = model_bus.or(tp.bus);
            let rental = after_break && vehicle == tp.bus;
            // (the player's bus: where the game says it is, while it says so)
            let player = tp.by_player || tp.duties.iter().any(|x| x.driver == Some(PLAYER));
            let tele = m.telemetry.as_ref().filter(|t| player && drives(t, b, tp)).cloned();
            if let Some(t) = &tele {
                live.at = Some(t.at);
                live.heading = t.heading;
                if let Some(delay) = t.delay {
                    live.delay = delay;
                }
                if live.doing == Doing::Done || live.doing == Doing::Depot {
                    live.doing = if t.speed > 1.0 { Doing::Deadhead } else { Doing::Layover };
                }
            }
            markers.push(Marker { key: key.clone(), b: bi, live, vehicle: if rental { None } else { vehicle }, rental, player: tele.is_some(), telemetry: tele });
            if let Some(br) = broken {
                markers.push(Marker { key: format!("{key}|broken"), b: bi, live: br, vehicle: tp.bus, rental: false, player: false, telemetry: None });
            }
        }
        m.day = day;
    }
    markers.retain(|mk| mk.shown(clock));
    let picked = m.picked.clone();
    let buses: Vec<Value> = markers
        .iter()
        .map(|mk| {
            let b = &m.blocks[mk.b];
            let (tag, colour) = b.tags.get(mk.live.trip).cloned().unwrap_or_else(|| (b.number.clone(), String::new()));
            let at = mk.live.at.unwrap_or([0.0, 0.0]);
            json!({
                "key": mk.key, "at": [(at[0] * 10.0).round() / 10.0, (at[1] * 10.0).round() / 10.0], "h": (mk.live.heading * 10.0).round() / 10.0,
                "colour": colour, "tag": tag, "state": doing_key(mk.live.doing), "tone": delay_tone(mk.live.delay),
                // (its whole minutes late: the Operations page's map colours it by the alert's minutes)
                "min": (mk.live.delay / 60.0 + 1e-6).floor() as i32,
                "player": mk.player, "on": picked.as_deref() == Some(mk.key.as_str()),
            })
        })
        .collect();
    // the panel
    let on_road = markers.iter().filter(|mk| matches!(mk.live.doing, Doing::Service | Doing::Deadhead)).count();
    let pausing = markers.iter().filter(|mk| mk.live.doing == Doing::Layover).count();
    let counts = trf("On the road: %{n}  ·  at a terminus: %{p}  ·  tours today: %{t}", &[("n", &on_road.to_string()), ("p", &pausing.to_string()), ("t", &m.blocks.len().to_string())]);
    let legend: Vec<Value> = m
        .legend
        .iter()
        .map(|(line, colour)| {
            let n = markers.iter().filter(|mk| m.blocks[mk.b].line.eq_ignore_ascii_case(&line.name) && mk.live.doing != Doing::Broken).count();
            let caption = if line.caption.is_empty() { line.name.clone() } else { line.caption.clone() };
            json!({ "plate": plate(&line.number, &line.colour), "colour": colour, "caption": caption, "n": n })
        })
        .collect();
    let card = picked.as_ref().map(|key| match (markers.iter().find(|mk| &mk.key == key), s.plan.as_ref(), m.day.as_ref()) {
        (Some(mk), Some(plan), Some(d)) => bus_card(c, plan, d, m, mk, clock),
        _ => away_card(c, m, key, clock),
    });
    json!({
        "built": built,
        "title": tr("Fleet map").to_uppercase(),
        "live": m.live.then(|| json!({ "text": tr(if in_game { "Live: the game's time" } else { "Live: the company's time" }), "tone": if in_game { "ok" } else { "accent2" } })),
        "clock": hhmmss(clock), "secs": clock,
        "playing": !m.live && m.speed > 0.0, "is_live": m.live, "speed": m.speed,
        "tips": { "run": tr("Run the clock"), "stop": tr("Stop the clock"), "x10": tr("Ten times as fast"), "x60": tr("Sixty times as fast"), "now": tr("Back to now"), "close": tr("Close") },
        "counts": counts,
        "legend": legend, "no_lines": tr("The company runs no line on this day."),
        "buses": buses,
        "picked": picked,
        "card": card,
    })
}

fn doing_key(d: Doing) -> &'static str {
    match d {
        Doing::Depot => "depot",
        Doing::Service => "service",
        Doing::Deadhead => "deadhead",
        Doing::Layover => "layover",
        Doing::Broken => "broken",
        Doing::Done => "done",
    }
}

/// The card of a bus picked that is not on the map now: still in the depot, or its day done.
fn away_card(c: &Company, m: &MapState, key: &str, clock: f64) -> Value {
    let mut parts = key.split('|');
    let (line, tour) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let Some(b) = m.blocks.iter().find(|b| b.line == line && b.tour == tour) else { return Value::Null };
    let first = b.trips.first().map(|f| f.dep).unwrap_or(0.0);
    let text = if clock < first { trf("In the depot: leaves at %{time}", &[("time", &hhmm(first))]) } else { tr("Its day is done.") };
    let colour = b.tags.first().map(|t| t.1.clone()).unwrap_or_default();
    json!({ "key": key, "head": head_of(c, b, &colour, &b.number, None), "away": text })
}

/// The card's head: the line's plate, the bus's fleet number and its block.
fn head_of(c: &Company, b: &Block, colour: &str, number: &str, vehicle: Option<&co::Vehicle>) -> Value {
    let cl = c.lines.iter().find(|x| x.number.trim().eq_ignore_ascii_case(number.trim())).or_else(|| c.lines.iter().find(|x| x.name == b.line));
    let title = match vehicle {
        Some(v) => trf("Bus %{bus}", &[("bus", &v.number)]),
        None => block_name(&b.number, &b.tour),
    };
    json!({
        "plate": match cl { Some(x) if x.number.trim().eq_ignore_ascii_case(number.trim()) => plate(&x.number, &x.colour), _ => plate(number, "") },
        "colour": colour, "title": title, "block": trf("Block %{block}", &[("block", &block_name(&b.number, &b.tour))]),
    })
}

/// What a bus does, in words.
fn doing_text(mk: &Marker, trip: Option<&PlannedTrip>) -> String {
    let to = trip.map(|x| if x.to.trim().is_empty() { x.last.trim() } else { x.to.trim() }).unwrap_or("").to_string();
    match mk.live.doing {
        Doing::Service => trf("On its way to %{stop}", &[("stop", &to)]),
        Doing::Deadhead => match trip {
            Some(x) if !to.is_empty() => trf("Empty run to %{stop}", &[("stop", if x.last.trim().is_empty() { &to } else { x.last.trim() })]),
            _ => tr("Empty run"),
        },
        Doing::Layover => trf("At the terminus until %{time}", &[("time", &hhmm(mk.live.next_time.unwrap_or(0.0)))]),
        Doing::Depot => trf("Leaves at %{time}", &[("time", &hhmm(mk.live.next_time.unwrap_or(0.0)))]),
        Doing::Broken => tr("Broken down"),
        Doing::Done => tr("Its day is done."),
    }
}

/// The card of the bus picked (`map::bus_card`): everything about it.
fn bus_card(c: &Company, plan: &Plan, d: &Day, m: &MapState, mk: &Marker, clock: f64) -> Value {
    let b = &m.blocks[mk.b];
    let Some(tp) = plan.tours.get(b.ti) else { return Value::Null };
    let j = mk.live.trip.min(tp.tour.trips.len().saturating_sub(1));
    let trip = tp.tour.trips.get(j);
    let shape = b.trips.get(j);
    let (tag, colour) = b.tags.get(j).cloned().unwrap_or_else(|| (b.number.clone(), String::new()));
    let bus = mk.vehicle.and_then(|v| c.vehicle(v));
    let broken = mk.live.doing == Doing::Broken;
    let timed = d.model.get(b.ti).and_then(|r| r.get(j)).and_then(|x| x.as_ref());
    // the bus: its photo, number, plate, model and age
    let photo = bus.map(|v| json!({ "file": v.bus, "paint": v.livery, "name": v.name }));
    let facts = match bus {
        Some(v) => vec![format!("{} · {}", v.number, v.plate), v.name.clone(), trf("%{n} years old", &[("n", &num(v.age_years(&c.date).max(0.0), 0))])],
        None if mk.rental => vec![tr("Rental bus")],
        None => Vec::new(),
    };
    // what it does, the next stop, the delay and why
    let what = doing_text(mk, trip);
    let next = match (mk.live.doing, mk.live.next_stop, mk.live.next_time) {
        (Doing::Service | Doing::Deadhead, Some(k), Some(t)) => shape.and_then(|x| x.names.get(k)).map(|n| trf("Next: %{stop} at %{time}", &[("stop", n.trim()), ("time", &hhmm(t))])),
        _ => None,
    };
    let delay = mk.live.delay;
    let cause = if delay >= 60.0 && !mk.player {
        timed.and_then(|t| t.causes.iter().max_by_key(|x| x.minutes()).map(|x| cause_text(c, x)).or_else(|| t.root().map(cause_kind)))
    } else {
        None
    };
    let delay_v = json!({
        "label": if broken { tr("Delay when it broke down") } else { tr("Delay") },
        "value": offset(delay), "tone": delay_tone(delay),
        "text": if delay.abs() < 60.0 { tr("On time") } else if delay < 0.0 { tr("Early") } else { trf("%{m} min late", &[("m", &((delay / 60.0).floor() as i64).to_string())]) },
        "cause": cause,
    });
    // the line, trip, block, duty and driver
    let duty = duty_of(tp, j);
    let driver = duty.and_then(|k| tp.duties.get(k)).and_then(|x| x.driver);
    // (Beta 63: a player of "Play together" on it, or the AI for one who left it)
    let guest = duty.and_then(|k| co::together::guest_on(c, &c.date, &tp.tour.line, &tp.tour.tour, k));
    let driver_text = match driver {
        Some(PLAYER) if guest.is_some() => guest.map(|g| crate::omsihub::together::guest_label(c, g)).unwrap_or_default(),
        Some(PLAYER) => tr("You"),
        Some(AGENCY) => tr("Agency driver"),
        Some(id) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "—".into()),
        None if tp.by_player => tr("You"),
        None => "—".into(),
    };
    let mut rows = Vec::new();
    if let Some(x) = trip {
        let to = if x.to.trim().is_empty() { x.last.trim() } else { x.to.trim() };
        rows.push(row(&tr("Line"), &format!("{tag} → {to}"), "text"));
        rows.push(row(&tr("Trip"), &format!("{}  {} → {}", hhmm(x.dep as f64 * 60.0), x.from.trim(), to), "text"));
    }
    rows.push(row(&tr("Block"), &block_name(&b.number, &b.tour), "text"));
    rows.push(row(&tr("Duty"), &duty.map(|k| format!("{} / {}", k + 1, tp.duties.len())).unwrap_or_else(|| "—".into()), "text"));
    rows.push(row(&tr("Driver"), &driver_text, "text"));
    // what it carries, its state, and its day
    let pax = match (&mk.telemetry, trip, mk.live.doing) {
        (Some(t), _, _) if t.passengers.is_some() => Some(num(t.passengers.unwrap_or(0.0), 0)),
        (_, Some(x), Doing::Service) if x.counts() => Some(format!("≈ {:.0}", co::income::expected(c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&tp.tour.line)), &c.date, x.dep, x.arr))),
        _ => None,
    };
    rows.push(row(&tr("Passengers"), &pax.unwrap_or_else(|| "—".into()), "text"));
    let refs: Vec<&TripShape> = b.trips.iter().collect();
    let runs = runs_of(m, d, tp, b);
    // (a block's bus broken down drove its trips up to then; the bus sent those from then on)
    let until = if broken { d.cut.iter().find(|x| Some(x.vehicle) == tp.bus).map_or(clock, |x| x.at as f64 * 60.0) } else { clock };
    let mine: Vec<Run> = match d.cut.iter().find(|x| Some(x.vehicle) == tp.bus).filter(|x| clock >= x.at as f64 * 60.0) {
        Some(x) if !broken => runs.iter().zip(&refs).map(|(r, t)| if t.dep >= x.at as f64 * 60.0 { *r } else { Run::default() }).collect(),
        _ => runs.clone(),
    };
    let km = fm::km_by(&refs, &mine, &b.km, until);
    rows.push(row(&tr("Kilometres today"), &format!("{} km", num(km, 1)), "text"));
    rows.push(row(&tr("Earned today (est.)"), &eur(earned(c, tp, &refs, &mine, until)), "ok"));
    if let Some(v) = bus {
        rows.push(json!({ "label": tr("Condition"), "value": format!("{:.0} %", v.condition), "tone": grade(v.condition), "meter": v.condition / 100.0 }));
        rows.push(json!({ "label": tr("Cleanliness"), "value": format!("{:.0} %", v.clean), "tone": grade(v.clean), "meter": v.clean / 100.0 }));
        rows.push(row(&tr("Odometer"), &format!("{} km", num(v.km, 0)), "text"));
    }
    // what happened to it today
    let notes = notes_of(c, tp, mk, clock);
    // where to go from here
    let mut links = vec![json!({
        "label": tr("Open in RosterFlow"), "icon": "view_timeline",
        "rf": match duty { Some(k) => json!({ "view": 0, "day": 0, "sel": { "duty": [b.line, b.tour, k] } }), None => json!({ "view": 1, "day": 0, "reveal": format!("B|{}|{}", b.line, b.tour) }) },
    })];
    links.push(json!({ "label": tr("Service control"), "icon": "support_agent", "tab": OPS_TAB }));
    if let Some(v) = bus {
        links.push(json!({ "label": tr("The bus's page"), "icon": "directions_bus", "bus": v.id }));
    }
    json!({
        "key": mk.key,
        "head": head_of(c, b, &colour, &tag, bus),
        "photo": photo, "facts": facts,
        "what": what, "state": doing_key(mk.live.doing), "next": next,
        "source": if mk.player { Some(tr("Live position from the game")) } else { None },
        "delay": delay_v,
        "rows": rows, "notes": notes, "links": links,
    })
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn row(label: &str, value: &str, tone: &str) -> Value {
    json!({ "label": label, "value": value, "tone": tone })
}

/// What a block's bus has earned today by `until`: its passengers' takings (`income`; none on a
/// gross-cost line, the authority's), a weekend line's tourism grant and on a bus contract each
/// journey's share of the contract's operating day, a trip under way by how far along it is (the
/// day's close books the real sums).
fn earned(c: &Company, tp: &co::day::TourPlan, trips: &[&TripShape], runs: &[Run], until: f64) -> co::model::Cents {
    let Some(cl) = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&tp.tour.line)) else { return 0 };
    let kind = co::ownline::kind_of(cl);
    let rates = co::income::Rates::of(c, cl, &c.date);
    let per_km = co::ownline::per_km_of(c, cl);
    let journey = co::bus_contracts::on_line(c, &cl.name, &c.date).map_or(0.0, |k| co::bus_contracts::day_value(c, k) / k.journeys.max(1) as f64);
    let mut sum = 0;
    for (j, x) in tp.tour.trips.iter().enumerate().filter(|(_, x)| x.counts()) {
        let (Some(t), Some(run)) = (trips.get(j), runs.get(j)) else { continue };
        let one: [&TripShape; 1] = [t];
        let km = fm::km_by(&one, &[*run], &[x.km], until);
        if km <= 0.0 {
            continue;
        }
        let share = if x.km > 0.0 { km / x.km } else { 0.0 };
        let pax = co::income::expected(Some(cl), &c.date, x.dep, x.arr) * share;
        let fares = if kind == ServiceKind::School || rates.to_authority { 0 } else { rates.takings(pax, km).total().round() as co::model::Cents };
        let comp = co::ownline::trip_payment(km, per_km) + (journey * share).round() as co::model::Cents;
        sum += fares + comp;
    }
    sum
}

/// What happened to a bus today (`notes` of its card): its breakdown and what was decided, the
/// controller's doing on its block, the notice that it runs late, the bus it stands in for.
fn notes_of(c: &Company, tp: &co::day::TourPlan, mk: &Marker, clock: f64) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(script) = c.clock.today.as_ref().filter(|x| x.date == c.date) else { return out };
    let hm = |m: i32| hhmm(m as f64 * 60.0);
    let bus_no = |id: u32| c.vehicle(id).map(|v| v.number.clone()).unwrap_or_else(|| tr("Rental bus"));
    let now = clock / 60.0;
    // its breakdown (the block's bus), or the one it stands in for (the bus sent)
    for br in script.breaks.iter().filter(|x| tp.bus == Some(x.vehicle) && x.at as f64 <= now) {
        let mine = mk.vehicle == Some(br.vehicle) || mk.live.doing == Doing::Broken;
        let title = match (mine, br.accident) {
            (true, false) => trf("Broke down at %{time}", &[("time", &hm(br.at))]),
            (true, true) => trf("Had an accident at %{time}", &[("time", &hm(br.at))]),
            (false, _) => trf("Stands in for bus %{bus}, broken down at %{time}", &[("bus", &br.bus), ("time", &hm(br.at))]),
        };
        let decided = match br.choice {
            Some(Choice::Spare { vehicle }) => trf("Spare bus %{bus} runs its trips from %{time}.", &[("bus", &bus_no(vehicle)), ("time", &hm(br.back.unwrap_or(br.at)))]),
            Some(Choice::Rental) => trf("A rental bus runs its trips from %{time}.", &[("time", &hm(br.back.unwrap_or(br.at)))]),
            Some(Choice::Drop) => tr("Its trips are dropped for the rest of the day."),
            None => tr("Waiting for your decision on the Operations page."),
        };
        out.push(json!({ "icon": if br.accident { "car_crash" } else { "car_repair" }, "tone": if mine { "danger" } else { "warn" }, "text": format!("{title}  ·  {decided}") }));
    }
    // the controller's doing on its block
    for x in script.controls.iter().filter(|x| same(&x.line, &tp.tour.line) && same(&x.tour, &tp.tour.tour) && x.at as f64 <= now) {
        let time = hm(x.dep);
        let text = match x.act {
            Act::ShortTurn { .. } => trf("Service control: turned back early on the %{time} trip", &[("time", &time)]),
            Act::Express { .. } => trf("Service control: out of service to the terminus on the %{time} trip", &[("time", &time)]),
            Act::Layover { saved } => trf("Service control: layover %{m} min shorter before the %{time} trip", &[("m", &saved.to_string()), ("time", &time)]),
            Act::Swap { vehicle, from_bus } => trf("Service control: bus %{spare} took over from bus %{bus} from the %{time} trip", &[("spare", &bus_no(vehicle)), ("bus", &bus_no(from_bus)), ("time", &time)]),
            Act::Cover { duty, from } => trf("Service control: duty %{n} covered from %{time}", &[("n", &(duty + 1).to_string()), ("time", &hm(from))]),
        };
        out.push(json!({ "icon": "support_agent", "tone": "accent", "text": text }));
    }
    // told running late
    let key = co::plan::tour_key(&tp.tour.line, &tp.tour.tour);
    let n = ctl::notice_minutes(c) as i32;
    if let Some((_, minutes)) = script.watch.late.iter().find(|x| x.0 == key && n > 0 && x.1 >= n.max(ctl::LATE_AT + 1)) {
        out.push(json!({ "icon": "notifications", "tone": "warn", "text": trf("Running %{m} minutes late: service control has ways to bring it back on time.", &[("m", &minutes.to_string())]) }));
    }
    out
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
        // a bus picked on the map (its key), none to close its card
        "map_pick" => {
            m.picked = args.get("key").and_then(Value::as_str).filter(|k| !k.is_empty()).map(str::to_string);
        }
        _ => return None,
    }
    Some(Ok(json!({})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_players_bus_is_where_the_game_says_while_it_is_fresh() {
        let v = json!({ "updated": 1000.0, "player": { "x": 1500.0, "y": -200.0, "heading": 370.0, "delay_s": 95.0, "passengers": 12, "speed_kmh": 30.0 }, "duty": { "line": " 5 ", "tour": "2" } });
        let t = telemetry_of(&v, [1000.0, 100.0], 1004.0).expect("fresh");
        assert_eq!((t.at, t.heading, t.delay, t.passengers), ([500.0, -300.0], 10.0, Some(95.0), Some(12.0)));
        assert_eq!((t.line.as_str(), t.tour.as_str()), ("5", "2"));
        assert!(telemetry_of(&v, [0.0, 0.0], 1100.0).is_none(), "an old file is not believed");
        assert!(telemetry_of(&json!({ "updated": 1000.0, "player": null }), [0.0, 0.0], 1000.0).is_none(), "no bus");
    }

    #[test]
    fn a_delay_takes_the_late_runnings_colours() {
        assert_eq!(delay_tone(0.0), "ok");
        // (late by the UK standard: from six minutes; the contracts' slightly late to ten)
        assert_eq!(delay_tone(359.0), "ok");
        assert_eq!(delay_tone(360.0), "warn");
        assert_eq!(delay_tone(600.0), "warn");
        assert_eq!(delay_tone(660.0), "orange");
        assert_eq!(delay_tone(1800.0), "danger");
        assert_eq!(delay_tone(-120.0), "early");
        assert_eq!(offset(-45.0), "−0:45");
        assert_eq!(offset(130.0), "+2:10");
    }

    /// Every text of the fleet map in the six languages.
    #[test]
    fn every_fleet_map_text_is_translated() {
        let src = include_str!("fleetmap.rs").replace("\r\n", "\n");
        let mut texts: Vec<String> = Vec::new();
        for lead in ["tr(\"", "trf(\""] {
            let mut rest = src.as_str();
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let t: String = after.chars().take_while(|c| *c != '"').collect();
                if t.chars().any(char::is_alphabetic) && !t.contains('\\') {
                    texts.push(t);
                }
            }
        }
        assert!(texts.len() > 40, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-livemap.yml):\n{}", missing.len(), missing.join("\n"));
    }
}

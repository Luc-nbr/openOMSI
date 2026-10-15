//! The line editor without its window (Trello "Tauri 5/6"): the desktop launcher's page draws
//! it (`launcher/ui/js/omsihub/editors/lines`), and the game program keeps it - the launcher
//! service hands the page's requests here (`hub_service`, `le`). It is the line editor itself:
//! a `LineEditorView` with its registry, its lines and the map's lane network, changed by the
//! line editor's own functions (`route_direction`, `reverse`, `maplines::open`, the depot
//! files' and the company's), only the drawing left to the page. Every request answers what
//! the page is to show (`view`), its texts in the page's language.
//!
//! The map's roads and stops are read on a worker, as the game-drawn launcher's map reads them
//! (`Host::city_map`); until they are there, a direction changed is drawn straight and routed
//! once they are (`unrouted`, as `LineEditorView::reroute` does).

use super::*;
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

mod company;
mod ops;
mod view;

/// The map as the line editor reads it: its lane network and stops (the router's), and the
/// page's picture of it - the roads, as the city map draws them, in metres from `origin`.
pub(crate) struct MapRead {
    /// `global.cfg`|date it was read for.
    key: String,
    net: Arc<omsi_sim::traffic::Network>,
    stops: Vec<omsi_scene::MapStop>,
    left_hand: bool,
    origin: DVec2,
    /// The roads for the page: `{ origin, lo, hi, roads: [{ w, main, p }] }`.
    roads: Arc<Value>,
}

/// The line editor working for the page.
pub(crate) struct Session {
    v: LineEditorView,
    /// The maps the page lists (their name and `global.cfg`), the game's folder, the day.
    maps: Vec<(String, String)>,
    root: PathBuf,
    date: String,
    /// The map being read (its key), and the one read.
    reading: Option<(String, Receiver<MapRead>)>,
    map: Option<Arc<MapRead>>,
    /// The bus company the editor works for, as its file has it.
    company: Option<Company>,
    /// The installed buses and their kinds (the Kind tab), read when first asked for.
    vehicles: Option<Arc<Vec<core::VehicleInfo>>>,
    classes: crate::busclass::BusClasses,
    /// What to say once, and where the page goes next (the Drive page, the depot editor,
    /// RosterFlow's Timeplan) - said in the next answer.
    status: Option<(String, bool)>,
    go: Option<Value>,
    /// The timetable of the company's day on its map (`company::map_lines`): map|date and its
    /// lines.
    day_lines: Option<(String, Arc<Vec<core::LineInfo>>)>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

impl Session {
    fn new() -> Session {
        Session { v: LineEditorView::default(), maps: Vec::new(), root: PathBuf::new(), date: String::new(), reading: None, map: None, company: None, vehicles: None, classes: Default::default(), status: None, go: None, day_lines: None }
    }

    fn say(&mut self, text: impl Into<String>, err: bool) {
        let t: String = text.into();
        if !t.is_empty() {
            self.status = Some((t, err));
        }
    }
}

/// A request of the page (`le`): `op` and what it takes; the answer is the view, or - for
/// `map_data` - the map's roads and stops.
pub(crate) fn serve(req: &Value) -> Value {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let s = guard.get_or_insert_with(Session::new);
    // (the page's language: the texts answered are in it)
    if let Some(lang) = req.get("lang").and_then(Value::as_str) {
        if omsi_ui::i18n::language() != lang && !(lang == "en" && omsi_ui::i18n::language().is_empty()) {
            omsi_ui::i18n::set_language(lang);
        }
    }
    let op = req.get("op").and_then(Value::as_str).unwrap_or("view");
    if op == "map_data" {
        frame(s);
        return map_data(s);
    }
    if let Err(e) = ops::apply(s, op, req) {
        s.say(e, true);
    }
    frame(s);
    view::view(s)
}

// --- the map ---------------------------------------------------------------------------------

/// The map's roads and stops for the page's canvas (its stops with the names the line editor
/// gives them), once read: `{ key, origin, lo, hi, roads, stops: [{ name, at }], left_hand }`
/// in metres from `origin`; `{ reading: true }` meanwhile.
fn map_data(s: &Session) -> Value {
    let Some(m) = s.map.as_ref() else { return json!({ "reading": s.reading.is_some() }) };
    let o = m.origin;
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let stops: Vec<Value> = s.v.stops.iter().map(|st| json!({ "name": st.name, "at": [r1(st.at[0] - o.x), r1(st.at[1] - o.y)] })).collect();
    let mut v = (*m.roads).clone();
    v["key"] = json!(m.key);
    v["stops"] = json!(stops);
    v["left_hand"] = json!(m.left_hand);
    v
}

/// Read `global` on `date` as the game-drawn launcher's map reads it (`mapview::read_map`):
/// the roads the game's city map draws, its lane network and bus stops.
fn read_map(key: String, global: PathBuf, date: String) -> MapRead {
    let empty = |key: String| MapRead { key, net: Arc::new(Default::default()), stops: Vec::new(), left_hand: false, origin: DVec2::ZERO, roads: Arc::new(json!({ "origin": [0.0, 0.0], "lo": [0.0, 0.0], "hi": [0.0, 0.0], "roads": [] })) };
    let Ok(g) = omsi_map::GlobalCfg::load(&global) else { return empty(key) };
    omsi_map::configure_grid(&g);
    let Some(map_dir) = global.parent().map(Path::to_path_buf) else { return empty(key) };
    let chrono = omsi_map::date_code(&date).map(|c| omsi_map::active_chrono_dirs(&map_dir, c)).unwrap_or_default();
    let root = omsi_cfg::content_roots().into_iter().find(|r| map_dir.starts_with(r)).or_else(|| map_dir.parent().and_then(|p| p.parent()).map(Path::to_path_buf)).unwrap_or_else(|| map_dir.clone());
    let tiles: Vec<(i32, i32, PathBuf)> = g.tiles.iter().map(|t| (t.x, t.y, omsi_cfg::resolve_path(&map_dir, &t.file))).filter(|t| omsi_cfg::vfs::is_file(&t.2)).collect();
    let t0 = std::time::Instant::now();
    let crate::host::CityMap { roads, net, stops, .. } = crate::host::host().city_map(&root, &tiles, &chrono);
    let (mut min, mut max) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for r in &roads {
        for q in &r.points {
            min = min.min(q.truncate());
            max = max.max(q.truncate());
        }
    }
    if min.x == f64::MAX {
        (min, max) = (DVec2::ZERO, DVec2::ZERO);
    }
    let origin = (min + max) * 0.5;
    // (the 1 - 99 % box of what the roads cover: a spline in a far corner does not shrink the
    // map to a dot)
    let mut xs: Vec<f64> = roads.iter().flat_map(|r| r.points.iter()).map(|q| q.x).collect();
    let mut ys: Vec<f64> = roads.iter().flat_map(|r| r.points.iter()).map(|q| q.y).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let (lo, hi) = if xs.is_empty() {
        (min, max)
    } else {
        let at = |v: &Vec<f64>, t: f64| v[((v.len() - 1) as f64 * t) as usize];
        (DVec2::new(at(&xs, 0.01), at(&ys, 0.01)), DVec2::new(at(&xs, 0.99), at(&ys, 0.99)))
    };
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let flat = |pts: &[glam::DVec3]| -> Vec<f64> {
        let v: Vec<glam::Vec3> = pts.iter().map(|q| glam::Vec3::new((q.x - origin.x) as f32, (q.y - origin.y) as f32, 0.0)).collect();
        crate::host::host().simplify(&v, 0.6).iter().flat_map(|q| [r1(q.x as f64), r1(q.y as f64)]).collect()
    };
    let list: Vec<Value> = roads.iter().map(|r| json!({ "w": (r.width * 10.0).round() / 10.0, "main": r.main, "p": flat(&r.points) })).filter(|v| v["p"].as_array().is_some_and(|a| a.len() >= 4)).collect();
    log::info!("line editor (page): {} roads, {} lanes, {} stops of {} read in {:.2} s", list.len(), net.lanes.len(), stops.len(), global.display(), t0.elapsed().as_secs_f32());
    let rel = |q: DVec2| [r1(q.x - origin.x), r1(q.y - origin.y)];
    let page = json!({ "origin": [origin.x, origin.y], "lo": rel(lo), "hi": rel(hi), "roads": list });
    MapRead { key, net: Arc::new(net), stops, left_hand: g.left_hand_traffic, origin, roads: Arc::new(page) }
}

/// The map of the line editor read (on a worker) unless it is, or is being.
fn want_map(s: &mut Session) {
    if s.v.global.as_os_str().is_empty() {
        return;
    }
    let key = format!("{}|{}", s.v.global.display(), s.date);
    if s.map.as_ref().is_some_and(|m| m.key == key) || s.reading.as_ref().is_some_and(|r| r.0 == key) {
        return;
    }
    // (another map: the old one's network goes with it)
    if s.map.as_ref().is_some_and(|m| m.key != key) {
        s.map = None;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let (k, global, date) = (key.clone(), s.v.global.clone(), s.date.clone());
    std::thread::spawn(move || {
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_map(k.clone(), global, date)));
        if let Ok(m) = read {
            let _ = tx.send(m);
        }
    });
    s.reading = Some((key, rx));
}

/// Once a request: the map taken in when it was read (`take_network`), the line asked for
/// opened once the stops are there, the shown line's legs as drawn.
fn frame(s: &mut Session) {
    want_map(s);
    if let Some((_, rx)) = s.reading.as_ref() {
        match rx.try_recv() {
            Ok(m) => {
                s.reading = None;
                take(s, Arc::new(m));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                s.reading = None;
                s.say(omsi_ui::tr("The map could not be read").into_owned(), true);
            }
            Err(_) => {}
        }
    }
    company::start(s);
    company::figures(s);
    // (with RosterFlow no table of the timetable: its Timeplan has it)
    if s.v.route_only() {
        s.v.table_open = false;
    }
    let v = &mut s.v;
    // (a line of the map's asked for before its stops were read: opened now)
    if !v.stops.is_empty() {
        if let Some(ttl) = v.opening.take() {
            v.note = maplines::open(v, &ttl);
        }
    }
    if let Some(note) = v.note.take() {
        s.status = Some(note);
    }
    let v = &mut s.v;
    if v.shapes_for != v.sel {
        v.shapes_for = v.sel;
        v.shapes = Default::default();
        if let (Some((_, router)), Some(line)) = (v.router.as_ref(), v.sel.and_then(|id| v.reg.line(id))) {
            for (k, d) in line.directions.iter().enumerate().take(2) {
                v.shapes[k] = stored_shapes(router, d);
            }
        } else if let Some(line) = v.sel.and_then(|id| v.reg.line(id)) {
            // (no roads yet: straight from stop to stop)
            for (k, d) in line.directions.iter().enumerate().take(2) {
                v.shapes[k] = (0..d.legs.len().min(d.stops.len().saturating_sub(1))).map(|i| straight(d, i)).collect();
            }
        }
        v.revision += 1;
    }
}

/// The map's network taken in (`take_network`): the router over it, the stops with their
/// names, the lines' stops placed, what was changed before routed.
fn take(s: &mut Session, m: Arc<MapRead>) {
    let v = &mut s.v;
    let router = Router::new(m.net.clone(), m.left_hand);
    log::info!("line editor (page): {} stops on the map, {} loose lane ends joined", m.stops.len(), router.joined());
    v.stops = m
        .stops
        .iter()
        .map(|st| {
            let name = stop_name(&v.names, st.tile, st.id).cloned().filter(|n| !n.is_empty()).or_else(|| Some(st.name.clone()).filter(|n| !n.is_empty())).unwrap_or_else(|| format!("Stop {}", st.id));
            StopRef { tile: [st.tile.0, st.tile.1], id: st.id, name, at: [st.at.x, st.at.y], ..Default::default() }
        })
        .collect();
    v.router = Some((Arc::as_ptr(&m.net) as usize, router));
    v.shapes_for = None;
    maplines::place_stops(v);
    let pending = std::mem::take(&mut v.unrouted);
    if let Some((_, router)) = v.router.as_ref() {
        let mut routed = false;
        for (id, dir) in pending {
            if let Some(d) = v.reg.lines.iter_mut().find(|l| l.id == id).and_then(|l| l.directions.get_mut(dir)) {
                route_direction(router, d);
                routed = true;
            }
        }
        if routed {
            v.dirty = true;
        }
    }
    // (a way back the line editor makes, its outbound changed before the map's stops were
    // read: made now)
    if v.line().is_some_and(|l| l.back_route == BackRoute::Auto && !l.is_map_line()) {
        make_back(v, true);
    }
    s.map = Some(m);
}

/// The world point of a point of the page's map (metres from the map's origin).
fn world(s: &Session, at: &Value) -> Option<[f64; 2]> {
    let o = s.map.as_ref().map(|m| m.origin).unwrap_or(DVec2::ZERO);
    let a = at.as_array()?;
    Some([a.first()?.as_f64()? + o.x, a.get(1)?.as_f64()? + o.y])
}

/// Read the map the page chose (`maps[k]`): its registry, names and depot groups
/// (`load_map`), and its roads on a worker.
fn load(s: &mut Session, k: usize) {
    let Some((_, file)) = s.maps.get(k).cloned() else { return };
    s.v.map = k;
    let root = s.root.clone();
    let date = s.date.clone();
    s.v.load_map(&file, &root, &date);
    s.v.router = None;
    s.v.stops.clear();
    want_map(s);
    // (the same map read before: its network again, at once)
    if let Some(m) = s.map.clone().filter(|m| m.key == format!("{}|{}", s.v.global.display(), s.date)) {
        take(s, m);
    }
}

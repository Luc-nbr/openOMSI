//! The line editor: a line of the player's own on any map. The map fills the middle of the
//! page with every bus stop on it; the player clicks the stops in order and the way between
//! each two is found over the roads (`lineroute`) - kerb side, lane changes, loose ends, as the
//! game's buses will drive it. "Reverse" makes the way back from the stops across the road.
//! A leg is dragged through a point of the player's choosing; a stop is put in after the one
//! chosen in the list, moved up or down, or taken out there.
//!
//! On the left the player's lines of the map and the line's own settings (number, name,
//! colour, the depot whose buses drive it); on the right a direction's stops with the minutes
//! to each, its destination, and the timetable: per group of days the first and the last
//! departure, how often, and how long a bus stands at the end - made into tours, a bus going
//! back and forth (`core::lines::blocks`).
//!
//! Saved, the line goes into the registry (`~/.openomsi/lines/<map>.json`, the source of
//! truth) and from there into the map's timetable files (`core::lines::export_to_map`), so
//! that the player drives it from the Drive page like any line and the timetable's buses
//! drive it too.
//!
//! The Kind tab says what kind of service the line is - regular, school transport, weekend
//! and leisure trips, on demand (`core::service`): on which days it runs and, in a company,
//! who pays for it - and which buses run it, by kind (a minibus, a coach…) and by maker,
//! model or version from the installed buses (`busclass`). The game drives the line with such
//! buses where the map's depots have them; the bus step puts them first.
//!
//! Opened from the bus company's Lines page it works for the company (`ForCompany`): the
//! company's map, colours and depot, only its lines; a running estimate over the map of what
//! the line costs once and a month, its passengers through the day and the buses it needs
//! (`core::company::ownline`); and "Confirm and pay", after which the company runs it. A
//! company line changed later pays its route's approval anew.
//!
//! A company with RosterFlow (`ForCompany::rosterflow`) makes its lines' timetables in
//! RosterFlow's Timeplan: the editor does the route alone. A new line starts without a
//! departure, the Timetable tab says where its timetable is made (a line the company runs opens
//! there), the estimate is made with a usual timetable (`ownline::as_priced`), and once the line
//! is confirmed the Timeplan opens on it (`after_confirm`).

use super::lineroute::{Anchor, Router};
use glam::DVec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::linehof;
use omsi_launcher_lib::owndepot;
use omsi_launcher_lib::company::{self as co, ownline, BusKind, BusSize, Cents, Drive};
use omsi_launcher_lib::lines::{self as reg, DayPattern, Direction, LineDesign, Registry, StopRef, TimeBand, DAY_GROUPS};
use omsi_launcher_lib::service::{self, BusPick, ServiceKind, VehicleClass};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The colours a line may have.
const PALETTE: [&str; 8] = ["#2a75f7", "#e5484d", "#30a46c", "#f5a524", "#8e4ec6", "#12a594", "#e93d82", "#7a8ca6"];
/// A stop of the same name across the road is at most this far (metres).
const ACROSS: f64 = 150.0;

#[derive(Default)]
pub struct LineEditorView {
    /// The map (an index into the map list) and the map file the registry was read for.
    map: usize,
    loaded: Option<String>,
    map_dir: PathBuf,
    folder: String,
    global: PathBuf,
    reg: Registry,
    reg_path: PathBuf,
    /// The line shown, its direction (0 out, 1 back) and the stop chosen in its list.
    sel: Option<u64>,
    dir: usize,
    sel_stop: Option<usize>,
    /// Changed and not saved.
    dirty: bool,
    /// The right panel: 0 the stops, 1 the displays, 2 the timetable, 3 the kind of service.
    tab: usize,
    /// The map's calendar (`Holidays.txt`), for the school holidays a school line keeps.
    map_calendar: omsi_map::Calendar,
    /// The installed buses by maker and model (for which buses: `busclass::fingerprint`), and
    /// the maker, model and version chosen to add to the line's buses (each 0: none, or all).
    catalogue: Option<(u64, Arc<Catalogue>)>,
    pick: (usize, usize, usize),
    /// The timetable's table over the map: open, its group of days, its first trip shown,
    /// the cell chosen (a trip of `LineDesign::table`, a stop) and what is typed into it, and
    /// "Fill from the patterns" pressed once.
    table_open: bool,
    table_day: usize,
    /// How far the table's trips are scrolled along (points).
    table_col: f32,
    table_sel: Option<(usize, usize)>,
    table_text: String,
    table_fill_armed: bool,
    /// A destination of the player's own being typed (the Displays tab).
    dest_new: String,
    /// The stops' names from the map's `Busstops.cfg`, by tile and id (`stop_names`).
    names: StopNames,
    /// The depot groups with buses (`ailists.cfg`) and each one's depot file.
    groups: Vec<(String, String)>,
    /// The depot files of the depot groups as the line editor sees them, without its own
    /// block (by name, lowercased; read once) - and the player's own (`own:` and its key).
    depots: HashMap<String, Option<Arc<linehof::Depot>>>,
    /// The player's own depot files for the map (`owndepot`), read once (again after one
    /// changed), and the destinations of them offered as "Mine" (for the line's depot file).
    own: Option<Vec<owndepot::Entry>>,
    mine: Option<(String, Vec<reg::OwnDestination>)>,
    kept: Option<Kept>,
    /// What moving the player's own destinations into his depot file came to (said once).
    note: Option<(String, bool)>,
    /// "Open in the depot editor" was pressed: the depot editor next, on that file.
    go_depots: Option<String>,
    /// The sign's preview lit white rather than amber.
    white: bool,
    /// The picker for any colour of the line open; the display texts for other kinds of sign
    /// shown too.
    colour_open: bool,
    more_texts: bool,
    /// The Displays tab in its Advanced view (every text on its own), and its list of the
    /// stops' names on the IBIS open.
    sign_advanced: bool,
    ibis_stops_open: bool,
    /// The router of the map's network (the network's address, to see it is still that one),
    /// and the map's stops as the line editor offers them.
    router: Option<(usize, Router)>,
    stops: Vec<StopRef>,
    /// Each direction's legs as drawn (world points), and for which line they were made.
    shapes: [Vec<Vec<DVec2>>; 2],
    shapes_for: Option<u64>,
    /// The directions changed before the map's roads were read (line, direction): routed
    /// once they are.
    unrouted: Vec<(u64, usize)>,
    /// Bumped whenever what the map draws of the line changes.
    revision: u64,
    delete_armed: bool,
    /// Working for a bus company (its Lines page opened the editor).
    company: Option<ForCompany>,
    /// What the company's buttons asked for (done after the panels).
    company_act: Option<CompanyAct>,
    /// A change of a line in service asked about (`change_dialog`).
    change_ask: Option<ChangeAsk>,
    /// The map's own lines (`maplines`): the timetable the game reads (`TTData`), its lines as
    /// the list shows them, and `global.cfg`'s list of tiles (a lane's tile is its place there).
    tt: Option<Arc<omsi_timetable::TimetableData>>,
    map_rows: Vec<maplines::MapRow>,
    raw_tiles: Vec<(i32, i32)>,
    /// A line of the map's asked for before the map's stops were read: opened once they are.
    opening: Option<String>,
    /// A line of the map's opened and not changed yet: it goes again when another line is
    /// chosen (only a change makes it the player's edit).
    fresh: Option<u64>,
    /// Each line of the map's as the game runs it now (the map's own, or as edited and saved):
    /// what a company's change is approved against.
    running: HashMap<u64, LineDesign>,
    /// "Restore the map's original" was pressed once.
    restore_armed: bool,
    /// RosterFlow's Timeplan next (a line confirmed, or its timetable asked for).
    go_timeplan: Option<ToTimeplan>,
}

mod maplines;
// the line editor of the desktop launcher's page, kept by the game program (the launcher
// service's `le`): this view without its window
pub(crate) mod headless;

/// Where the shown line's own destinations are kept (`LineEditorView::kept`): for which line
/// (its depot group and depot file), the player's depot file (its place and name), its
/// destinations the map's depot file lacks, and the names of all of them (lowercased).
struct Kept {
    for_line: String,
    target: Option<(PathBuf, String)>,
    mine: Vec<reg::OwnDestination>,
    names: std::collections::HashSet<String>,
}

/// What the line editor keeps while it works for a bus company.
pub struct ForCompany {
    id: String,
    name: String,
    map: String,
    colours: [String; 2],
    /// The depot file its buses carry (the depot group of that file is the line's).
    depot: String,
    /// What to show once the map is read: a new line (None), or the company's line.
    start: Option<Option<u64>>,
    /// The other lines at the map's stops, for the line shown (its own left out).
    stop_lines: Option<(u64, ownline::StopLines)>,
    /// The figures of the line shown, for its revision, and the estimate's details open.
    cache: Option<Figures>,
    details: bool,
    /// The lines of the map's the company runs (their timetable names): the list offers those,
    /// and a line of them to open once the map is read.
    runs: Vec<String>,
    start_map: Option<String>,
    /// The timetable a new line starts with (the Casual line wizard's "how often"); None: the
    /// company's lines' usual rush hours.
    days: Option<Vec<DayPattern>>,
    /// The company has RosterFlow on: the editor does the route alone (`route_only`) - a new
    /// line is confirmed without a timetable, which RosterFlow's Timeplan makes next.
    rosterflow: bool,
}

/// RosterFlow's Timeplan next, on the company's line: an own line by the line editor's id
/// (its timetable name the company's), else a line of the map's by its timetable name - with
/// what the Timeplan says first.
#[derive(Clone, Debug, PartialEq)]
struct ToTimeplan {
    id: Option<u64>,
    name: String,
    hint: Option<&'static str>,
}

/// Said in the Timeplan of a line just confirmed in the line editor (RosterFlow on).
pub(crate) const NEW_LINE_HINT: &str = "Set the running times and departures, save, then Refresh to make the buses and duties";
/// Said instead of the Timetable tab, working for a company with RosterFlow.
const TIMETABLE_LATER: &str = "The timetable is made in RosterFlow's Timeplan after you confirm";
const TIMETABLE_THERE: &str = "The line's timetable is made in RosterFlow's Timeplan.";

/// The company's figures of the line shown.
#[derive(Clone)]
struct Figures {
    key: (u64, u64, Cents, usize),
    shape: ownline::Shape,
    est: ownline::Estimate,
    /// The company runs it already, and what saving it now costs.
    confirmed: bool,
    fee: Option<Cents>,
    cash: Cents,
    /// What the company paid to start it (and to change it since).
    paid: Cents,
    /// Estimated with a company's starting timetable: the line has none yet (RosterFlow's
    /// Timeplan makes it, `ownline::as_priced`).
    priced: bool,
}

/// A change of a company's line in service, asked about before it is saved: what it costs,
/// the tours it touches, and from which day (tomorrow at the soonest: today's tours run as
/// they are).
#[derive(Clone)]
struct ChangeAsk {
    fee: Cents,
    diff: reg::TourDiff,
    today: String,
    from: String,
}

#[derive(Clone, Debug, PartialEq)]
enum CompanyAct {
    Confirm,
    SaveChange,
    /// The change asked about saved for its day.
    Schedule(String),
    /// A change waiting for its day taken back: the line stays as it runs.
    Withdraw,
    Discard,
    /// A line of the map's the company runs saved (its changed route paid for), or put back as
    /// the map has it (`maplines::company_act`).
    SaveMap,
    RestoreMap,
}

/// The sizes a time band can ask for ("auto" first: the demand model's choice).
const SIZES: [Option<BusSize>; 5] = [None, Some(BusSize::Midi), Some(BusSize::Solo), Some(BusSize::Articulated), Some(BusSize::Double)];

fn size_label(s: Option<BusSize>) -> String {
    match s {
        None => omsi_ui::tr("Auto").into_owned(),
        Some(size) => omsi_ui::tr(BusKind { size, drive: Drive::Diesel }.label()).into_owned(),
    }
}

/// Maps named alike (`maps/Grundorf/global.cfg`, either slash, any case).
fn same_map(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().replace('\\', "/").to_lowercase();
    n(a) == n(b)
}

/// A destination's words alike, case, commas and dots aside ("Hohenkirchen, Therme" is
/// "Hohenkirchen" over "Therme" on a sign; "Hohenkirchen, Th" is not). Case as the signs
/// write capitals: "Heerstraße" is "HEERSTRASSE".
fn same_words(a: &str, b: &str) -> bool {
    let n = |s: &str| s.to_uppercase().to_lowercase().replace([',', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    n(a) == n(b)
}

/// A new destination's front rows, made from its name, lost some of it (the player wrote
/// none of his own): the Simple view says so.
fn shortened_on_front(depot: &linehof::Depot, d: &Direction) -> bool {
    let dest = d.destination();
    if depot.terminus(&dest).is_some() || depot.front(&d.sign) != (String::new(), String::new()) {
        return false;
    }
    let (a, b) = depot.front(&depot.sign_defaults(&dest));
    !same_words(&format!("{a} {b}"), &dest)
}

/// The stops' names of a map's `Busstops.cfg`, by tile and object id (`stop_name`).
type StopNames = HashMap<(Option<(i32, i32)>, i64), String>;

/// The names of `Busstops.cfg`'s entries by tile and object id: an entry names its tile by its
/// place in global.cfg's `[map]` list (`tiles`, every entry counted). A map joined from others
/// repeats its ids (Berlin 186: half its objects share their id with one of another tile), so
/// an entry whose tile is not known (None) names a stop by its id alone only where the file
/// has that id once.
fn stop_names(entries: &[omsi_timetable::BusStopEntry], tiles: &[(i32, i32)]) -> StopNames {
    let mut count: HashMap<i64, usize> = HashMap::new();
    for b in entries {
        *count.entry(b.object_id).or_default() += 1;
    }
    let mut out = StopNames::new();
    for b in entries.iter().filter(|b| !b.name.trim().is_empty()) {
        let tile = usize::try_from(b.group).ok().and_then(|i| tiles.get(i)).copied();
        if tile.is_none() && count.get(&b.object_id) != Some(&1) {
            continue;
        }
        out.entry((tile, b.object_id)).or_insert_with(|| b.name.trim().to_string());
    }
    out
}

/// The name `Busstops.cfg` gives the stop `id` of the tile `tile` (`stop_names`).
fn stop_name(names: &StopNames, tile: (i32, i32), id: i64) -> Option<&String> {
    names.get(&(Some(tile), id)).or_else(|| names.get(&(None, id)))
}

fn dv(a: [f64; 2]) -> DVec2 {
    DVec2::new(a[0], a[1])
}

/// The ground points of a leg the router could not route: straight through its points.
fn straight(d: &Direction, k: usize) -> Vec<DVec2> {
    let mut v = vec![dv(d.stops[k].at)];
    v.extend(d.legs[k].vias.iter().map(|p| dv(*p)));
    v.push(dv(d.stops[k + 1].at));
    v
}

/// Route every leg of a direction again (each from where the one before ended), its times
/// with them; returns the legs as drawn. A leg of a line of the map's that is still the map's
/// own way between its two stops (`Leg::kept_between`) stays as the map drives it - one kept as
/// part of its trip's whole track only while the whole direction runs as the map has it.
fn route_direction(router: &Router, d: &mut Direction) -> Vec<Vec<DVec2>> {
    d.fit_legs();
    let whole = d.is_map_way();
    let mut shapes = Vec::new();
    let mut start: Option<Anchor> = None;
    for k in 0..d.legs.len() {
        let (a, b) = (dv(d.stops[k].at), dv(d.stops[k + 1].at));
        let kept = d.legs[k].kept_between(d.stops[k].id, d.stops[k + 1].id) && (whole || !d.legs[k].steps.is_empty());
        if kept {
            shapes.push(kept_shape(router, d, k));
            start = None;
            continue;
        }
        d.legs[k].map_way = None;
        let vias: Vec<DVec2> = d.legs[k].vias.iter().map(|p| dv(*p)).collect();
        // (from where the leg before ended; failing that, from any lane of the stop)
        let r = router.leg(a, b, &vias, start).or_else(|| start.and_then(|_| router.leg(a, b, &vias, None)));
        let leg = &mut d.legs[k];
        match r {
            Some(r) => {
                leg.steps = router.steps(&r);
                leg.length = r.length;
                leg.ok = !leg.steps.is_empty();
                (leg.from_s, leg.to_s, leg.from_lat, leg.to_lat) = (r.from.s, r.to.s, r.from.lateral.abs(), r.to.lateral.abs());
                shapes.push(router.points(&r.lanes, r.from.s, r.to.s));
                start = Some(Anchor { beside: None, ..r.to });
            }
            None => {
                leg.steps.clear();
                leg.ok = false;
                leg.length = (b - a).length() as f32;
                shapes.push(straight(d, k));
                start = None;
            }
        }
    }
    d.refresh_times();
    shapes
}

/// A direction's legs before the map's roads are read: each straight from stop to stop
/// through its points, not routed (`route_direction` once the roads are there); returns them
/// as drawn.
fn unrouted_direction(d: &mut Direction) -> Vec<Vec<DVec2>> {
    d.fit_legs();
    let mut shapes = Vec::new();
    for k in 0..d.legs.len() {
        let (a, b) = (dv(d.stops[k].at), dv(d.stops[k + 1].at));
        // (the map's own way stays the map's)
        if d.legs[k].kept_between(d.stops[k].id, d.stops[k + 1].id) {
            shapes.push(straight(d, k));
            continue;
        }
        let leg = &mut d.legs[k];
        leg.steps.clear();
        leg.ok = false;
        leg.length = (b - a).length() as f32;
        shapes.push(straight(d, k));
    }
    d.refresh_times();
    shapes
}

/// A leg as drawn from the lanes it keeps (straight through its points without them).
fn kept_shape(router: &Router, d: &Direction, k: usize) -> Vec<DVec2> {
    let g = &d.legs[k];
    let lanes = router.lanes_of(&g.steps);
    if g.ok && !lanes.is_empty() {
        router.points(&lanes, g.from_s, g.to_s)
    } else {
        straight(d, k)
    }
}

/// A saved direction's legs as drawn, from the lanes it keeps.
fn stored_shapes(router: &Router, d: &Direction) -> Vec<Vec<DVec2>> {
    (0..d.legs.len().min(d.stops.len().saturating_sub(1)))
        .map(|k| {
            let g = &d.legs[k];
            let lanes = router.lanes_of(&g.steps);
            if g.ok && !lanes.is_empty() {
                router.points(&lanes, g.from_s, g.to_s)
            } else {
                straight(d, k)
            }
        })
        .collect()
}

impl LineEditorView {
    /// It works for a bus company with RosterFlow: the route alone - the timetable is made in
    /// RosterFlow's Timeplan.
    fn route_only(&self) -> bool {
        self.company.as_ref().is_some_and(|f| f.rosterflow)
    }

    /// Why the shown line cannot be saved or confirmed yet: the route's alone with RosterFlow
    /// (its timetable is the Timeplan's), else all of them.
    fn problems(&self) -> Vec<reg::Problem> {
        match self.line() {
            Some(l) if self.route_only() && !l.is_map_line() => reg::route_problems(l),
            Some(l) => reg::problems(l),
            None => Vec::new(),
        }
    }

    /// Back to the player's own lines (the next draw reads the map again).
    pub fn leave_company(&mut self) {
        if self.company.take().is_some() {
            self.loaded = None;
            self.company_act = None;
        }
    }

    /// A line the list shows among the own ones: the company's when working for one, else the
    /// player's own - a line of the map's is listed with the map's (`maplines`).
    fn shows(&self, l: &LineDesign) -> bool {
        !l.is_map_line()
            && match &self.company {
                Some(fc) => l.company == fc.id,
                None => l.company.is_empty(),
            }
    }

    fn first_shown(&self) -> Option<u64> {
        self.reg.lines.iter().find(|l| self.shows(l)).map(|l| l.id)
    }

    fn line(&self) -> Option<&LineDesign> {
        self.sel.and_then(|id| self.reg.line(id))
    }

    fn line_mut(&mut self) -> Option<&mut LineDesign> {
        let id = self.sel?;
        self.reg.line_mut(id)
    }

    /// Something of the line changed: unsaved, and drawn again (a line of the map's opened is
    /// the player's edit now).
    fn touched(&mut self) {
        self.dirty = true;
        self.revision += 1;
        if self.fresh.is_some() && self.fresh == self.sel {
            self.fresh = None;
        }
    }

    /// Another line is shown: a line of the map's opened and left as it came goes again.
    fn leave_fresh(&mut self) {
        if let Some(id) = self.fresh.take() {
            self.reg.lines.retain(|l| l.id != id);
            self.running.remove(&id);
            if self.sel == Some(id) {
                self.sel = None;
            }
        }
        self.restore_armed = false;
    }

    /// The table's cell chosen, what is typed into it, its page and "Fill" pressed once let go
    /// (another line is shown: they were the other's).
    fn table_reset(&mut self) {
        (self.table_sel, self.table_text, self.table_col, self.table_fill_armed) = (None, String::new(), 0.0, false);
    }

    /// The direction `dir` of the shown line routed again.
    fn reroute(&mut self, dir: usize) {
        let Some(id) = self.sel else { return };
        let Some(l) = self.reg.lines.iter_mut().find(|l| l.id == id) else { return };
        if let Some(d) = l.directions.get_mut(dir) {
            self.shapes[dir] = match self.router.as_ref() {
                Some((_, router)) => route_direction(router, d),
                // (the map's roads not read yet: its legs straight for now, routed once they
                // are - the change kept all the same)
                None => {
                    if !self.unrouted.contains(&(id, dir)) {
                        self.unrouted.push((id, dir));
                    }
                    unrouted_direction(d)
                }
            };
        }
        self.touched();
    }

    /// Read the map's registry, names and depot groups (another map was chosen).
    fn load_map(&mut self, file: &str, root: &Path, date: &str) {
        self.loaded = Some(file.to_string());
        self.global = omsi_cfg::find_in_roots(file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(root, file));
        self.map_dir = self.global.parent().map(Path::to_path_buf).unwrap_or_default();
        self.folder = reg::map_folder(file);
        self.reg_path = reg::registry_path(&self.folder);
        self.reg = reg::load_registry(&self.reg_path);
        self.reg.map = self.folder.clone();
        self.reg.global = file.to_string();
        self.sel = self.first_shown();
        (self.dir, self.sel_stop, self.dirty, self.delete_armed) = (0, None, false, false);
        self.table_reset();
        self.shapes_for = None;
        self.router = None;
        self.unrouted.clear();
        self.stops.clear();
        self.map_calendar = omsi_map::Calendar::load(&self.map_dir.join("Holidays.txt")).unwrap_or_default();
        let tiles = omsi_map::GlobalCfg::load(&self.global).map(|g| g.raw_tiles).unwrap_or_default();
        let tt = omsi_timetable::TimetableData::load(&self.map_dir);
        self.names = stop_names(&tt.bus_stops, &tiles);
        // (the map's own lines, and each one edited as the game runs it now)
        self.map_rows = maplines::map_rows(&tt);
        self.tt = Some(Arc::new(tt));
        self.raw_tiles = tiles;
        (self.opening, self.fresh, self.restore_armed) = (None, None, false);
        self.running = self.reg.lines.iter().filter(|l| l.is_map_line()).map(|l| (l.id, l.clone())).collect();
        let chrono = omsi_map::date_code(date).map(|c| omsi_map::active_chrono_dirs(&self.map_dir, c)).unwrap_or_default();
        let ai = omsi_map::ailists::ailists_with_chrono(&self.map_dir, &chrono);
        self.groups = ai.groups.iter().filter(|g| g.is_depot && !g.typgroups.is_empty() && !g.name.trim().is_empty()).map(|g| (g.name.trim().to_string(), g.hof.clone().unwrap_or_default())).collect();
        self.depots.clear();
        self.own_changed();
        self.revision += 1;
        // the player's own destinations of before the depot editor: into his depot file of the
        // map (made from the map's), one list
        if !self.reg.destinations.is_empty() {
            let base = self.map_depot_bytes(file);
            let name = omsi_ui::tr("%{map} - mine").replace("%{map}", &self.folder);
            self.note = Some(match owndepot::migrate(&mut self.reg, &owndepot::dir(), &name, base.as_deref()) {
                Ok(Some((path, n))) => {
                    let _ = reg::save_registry(&self.reg_path, &self.reg);
                    let name = owndepot::load(&path).map(|l| l.doc.hof.name).unwrap_or(name);
                    (omsi_ui::tr("Your %{n} own destination(s) are in your depot file %{name} now: Editor, Depot editor").replace("%{n}", &n.to_string()).replace("%{name}", &name), false)
                }
                Ok(None) => (String::new(), false),
                Err(e) => (format!("{}: {e}", omsi_ui::tr("Your own destinations could not be moved into a depot file")), true),
            });
            self.own_changed();
        }
        // the lines' blocks in the depot files written the old way (a new destination in every
        // text: on both rows of a matrix) are written again once, as a save writes them - the
        // texts the player typed stay his
        if self.reg.texts < reg::TEXTS_VERSION && self.reg.destinations.is_empty() && self.reg.lines.iter().any(|l| reg::written(l.in_timetable())) {
            if let Some(content) = core::content_dir() {
                let prepared = self.prepare_depots();
                match self.write_depots(&content, &prepared) {
                    None => {
                        texts_written(&self.reg_path, &mut self.reg, true);
                        omsi_cfg::content_changed();
                    }
                    Some(e) => log::warn!("line editor: the depot files of {}'s lines not written again (tried again next time): {e}", self.folder),
                }
            }
        }
    }

    /// The bytes of the map's depot file (the first depot group's that names one), as the
    /// player's own depot file starts from: the copy most buses carry.
    fn map_depot_bytes(&self, map_file: &str) -> Option<Vec<u8>> {
        let hof = self.groups.iter().map(|g| g.1.trim()).find(|h| !h.is_empty())?;
        let found = core::depot::find_sources(hof, map_file);
        core::depot::best(&found).and_then(|i| omsi_cfg::vfs::read(&found[i].path).ok())
    }

    /// A depot file of the player's changed (or might have): read again, and the bus step's
    /// tiles with them.
    pub(super) fn own_changed(&mut self) {
        self.own = None;
        self.mine = None;
        self.kept = None;
        self.depots.retain(|k, _| !k.starts_with("own:"));
    }

    /// The player's own depot files for the map.
    fn own_entries(&mut self) -> &[owndepot::Entry] {
        let folder = self.folder.clone();
        self.own.get_or_insert_with(|| owndepot::for_map(&owndepot::list(&owndepot::dir()), &folder))
    }

    /// The depot file of the shown line as the line editor sees it: the player's own when the
    /// line chose one, else its depot group's.
    fn line_depot(&mut self) -> Option<Arc<linehof::Depot>> {
        let (group, key) = self.line().map(|l| (l.ai_group.clone(), l.depot_file.trim().to_string()))?;
        if let Some(path) = (!key.is_empty()).then(|| owndepot::path_of(&owndepot::dir(), &key)).flatten() {
            let folder = self.folder.clone();
            return self.depots.entry(format!("own:{}", key.to_lowercase())).or_insert_with(|| owndepot::depot_at(&path, &folder).map(Arc::new)).clone();
        }
        self.depot_of(&group)
    }

    /// The destinations of the player's own depot files the shown line's depot file lacks:
    /// what the line editor offers as "Mine" (`owndepot::own_destinations`).
    fn mine(&mut self) -> Vec<reg::OwnDestination> {
        let for_line = self.line().map(|l| format!("{}|{}", l.ai_group.to_lowercase(), l.depot_file.trim().to_lowercase())).unwrap_or_default();
        if let Some((_, m)) = self.mine.as_ref().filter(|(k, _)| *k == for_line) {
            return m.clone();
        }
        let known: std::collections::HashSet<String> = self.line_depot().map(|d| d.hof.termini.iter().map(|t| t.texture_id.trim().to_lowercase()).collect()).unwrap_or_default();
        let m = owndepot::own_destinations(self.own_entries(), &known);
        self.mine = Some((for_line, m.clone()));
        m
    }

    /// Where the shown line's own destinations are kept (the Displays tab): the player's depot
    /// file the line chose, else the first for the map (None: there is none yet) - worked out
    /// once (again after a change).
    fn kept(&mut self) -> &Kept {
        let (group, key) = self.line().map(|l| (l.ai_group.clone(), l.depot_file.trim().to_string())).unwrap_or_default();
        let for_line = format!("{}|{}", group.to_lowercase(), key.to_lowercase());
        if self.kept.as_ref().is_none_or(|k| k.for_line != for_line) {
            let map_known: std::collections::HashSet<String> = self.depot_of(&group).map(|d| d.hof.termini.iter().map(|t| t.texture_id.trim().to_lowercase()).collect()).unwrap_or_default();
            let own = self.own_entries();
            let target = own.iter().find(|e| !key.is_empty() && e.key.eq_ignore_ascii_case(&key)).or_else(|| own.iter().find(|e| !e.map.trim().is_empty())).or(own.first());
            let kept = Kept {
                for_line,
                target: target.map(|e| (e.path.clone(), e.name.clone())),
                mine: target.map(|e| e.termini.iter().filter(|t| !t.name.trim().is_empty() && !map_known.contains(&t.name.trim().to_lowercase())).cloned().collect()).unwrap_or_default(),
                names: target.map(|e| e.termini.iter().map(|t| t.name.trim().to_lowercase()).collect()).unwrap_or_default(),
            };
            self.kept = Some(kept);
        }
        self.kept.as_ref().unwrap()
    }

    /// The depot file a depot group names (`ailists.cfg`).
    fn hof_of(&self, group: &str) -> String {
        self.groups.iter().find(|g| g.0.eq_ignore_ascii_case(group)).map(|g| g.1.trim().to_string()).unwrap_or_default()
    }

    /// A depot group's depot file as the line editor sees it - without the block it wrote,
    /// so that a destination it added is still a new one (read once).
    fn depot_of(&mut self, group: &str) -> Option<Arc<linehof::Depot>> {
        let hof = self.hof_of(group);
        if hof.is_empty() {
            return None;
        }
        let folder = self.folder.clone();
        self.depots
            .entry(hof.to_lowercase())
            .or_insert_with(|| {
                let (bases, _) = core::depot_roots();
                linehof::first_depot(&hof, &bases, &folder).map(Arc::new)
            })
            .clone()
    }

    /// The termini of the shown line's depot file: (what the trip names, how the list shows it -
    /// that name, and the front's rows where they say it otherwise).
    fn termini(&mut self) -> Vec<(String, String)> {
        let Some(d) = self.line_depot() else { return Vec::new() };
        d.hof
            .termini
            .iter()
            .filter(|t| !t.all_exit && !t.texture_id.trim().is_empty())
            .map(|t| {
                let name = t.texture_id.split_whitespace().collect::<Vec<_>>().join(" ");
                let (l1, l2) = d.front(&t.strings);
                let rows = [l1, l2].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
                let label = if rows.is_empty() || same_words(&rows.join(" "), &name) { name.clone() } else { format!("{name} · {}", rows.join(" / ")) };
                (t.texture_id.trim().to_string(), label)
            })
            .collect()
    }

    /// Before the registry is saved: the codes of the lines in their depot files given - the
    /// map's and the player's own they chose (`owndepot::assign`) - and what is to be written
    /// there (`core::linehof::prepare`).
    fn prepare_depots(&mut self) -> (HashMap<String, String>, linehof::Plan, Option<PathBuf>) {
        let groups = linehof::group_depots(&self.map_dir);
        let (bases, original) = core::depot_roots();
        let plan = linehof::prepare(&mut self.reg, &groups, &bases);
        owndepot::assign(&mut self.reg, &owndepot::dir(), &groups, &bases);
        (groups, plan, original)
    }

    /// The depot files written (after the registry and the timetable): the destinations,
    /// routes and stops of the lines for the displays and the IBIS - in the map's depot files
    /// and in the player's own the lines chose (and the copies of those beside buses). The
    /// first error.
    fn write_depots(&mut self, content: &Path, prepared: &(HashMap<String, String>, linehof::Plan, Option<PathBuf>)) -> Option<String> {
        let (groups, plan, original) = prepared;
        let (n, err) = linehof::write(content, original.as_deref(), &self.reg, groups, plan);
        let (own, own_err) = owndepot::write_lines(&self.reg, &owndepot::dir(), Some(content));
        log::info!("line editor: {n} depot file(s) of the map's and {own} of the player's own written for the displays and the IBIS");
        self.depots.clear();
        self.own_changed();
        err.or(own_err)
    }
}

// --- the left panel: the lines and the line's settings ---------------------------------------

/// The registry `r` (kept at `path`) marked as having every line's block in the depot files
/// made the way they are made now (`reg::TEXTS_VERSION`), once a write of them all went
/// without an error (`written`): a write that failed, or never came for want of a content
/// folder, leaves the mark off, and `load_map` writes them again the next time.
fn texts_written(path: &Path, r: &mut Registry, written: bool) {
    if !written || r.texts >= reg::TEXTS_VERSION {
        return;
    }
    r.texts = reg::TEXTS_VERSION;
    if let Err(e) = reg::save_registry(path, r) {
        log::warn!("line editor: the registry {} not saved after its depot files: {e}", path.display());
    }
}

/// `f` done with the line of the map's opened and not changed (`fresh`) out of the registry,
/// and back in it after.
#[cfg(test)]
fn without_fresh<T>(v: &mut LineEditorView, f: impl FnOnce(&mut LineEditorView) -> T) -> T {
    let fresh = v.fresh.and_then(|id| v.reg.lines.iter().position(|l| l.id == id)).map(|k| v.reg.lines.remove(k));
    let done = f(v);
    if let Some(l) = fresh {
        v.reg.lines.push(l);
    }
    done
}

/// The timetable line and the trip of its first direction a free drive of the shown line
/// follows: the files it was saved as - a line of the map's opened and not changed, the map's
/// own line and trip, which run as they are. None: it is not saved as it is shown.
fn drive_target(v: &LineEditorView) -> Option<(String, String)> {
    let line = v.line()?;
    if v.fresh == Some(line.id) {
        let m = line.map.as_ref()?;
        return Some((m.ttl.clone(), line.directions.first()?.map_trip.clone()));
    }
    if v.dirty || !reg::problems(line).is_empty() {
        return None;
    }
    let stem = reg::stems(&v.reg).get(&line.id).cloned().unwrap_or_default();
    Some((line.timetable_name(&stem), reg::trip_name(&stem, 0)))
}

// --- the right panel: a direction's stops, and the timetable ----------------------------------

/// The way back made anew: the outbound stops from the last to the first, each the stop of
/// the same name across the road, routed.
fn reverse(v: &mut LineEditorView, status: &mut Option<(String, bool)>) {
    let all = v.stops.clone();
    let Some(line) = v.line_mut() else { return };
    if line.directions[0].stops.len() < 2 {
        *status = Some(("Give the outbound direction its stops first".into(), true));
        return;
    }
    // (the way back keeps the route code it had on the IBIS)
    let ibis_route = line.directions.get(1).map(|d| d.ibis_route).unwrap_or(0);
    let back = Direction { stops: reg::opposite_stops(&line.directions[0].stops, &all, ACROSS), ibis_route, ..Default::default() };
    line.directions.truncate(1);
    line.directions.push(back);
    (v.dir, v.sel_stop) = (1, None);
    v.reroute(1);
    let bad = v.line().map(|x| x.directions[1].legs.iter().filter(|g| !g.ok).count()).unwrap_or(0);
    *status = Some(if bad > 0 {
        (omsi_ui::tr("The way back is made; %{n} leg(s) have no way over the roads (red): drag them, or change a stop").replace("%{n}", &bad.to_string()), true)
    } else {
        ("The way back is made from the stops across the road".into(), false)
    });
}

/// What a text of a depot file (`k`, for `role`) is called in the line editor's Advanced view
/// and the depot editor.
pub(super) fn text_title(role: linehof::Role, k: usize) -> String {
    use linehof::Role;
    let tr = |s: &str| omsi_ui::tr(s).into_owned();
    match role {
        Role::IbisDisplay => tr("IBIS"),
        Role::FrontTop => tr("Front sign, top row"),
        Role::FrontBottom => tr("Front sign, bottom row"),
        Role::Side => tr("Side sign"),
        Role::ClearName => tr("Passenger display"),
        Role::RollerBlind => tr("Roller blind picture"),
        Role::ExtraSign => tr("Plug-in sign"),
        Role::Bitmap => tr("Picture on LED signs"),
        Role::Special => tr("Special trip mark"),
        Role::Rear => tr("Rear sign"),
        Role::Fallback(_) => tr("For some buses only"),
        Role::Other => tr("Text %{n}").replace("%{n}", &(k + 1).to_string()),
    }
}

/// What text `k` of `depot` is called (`text_title`) and what it does, said plainly: the
/// front's rows as the file writes a name of one row (its first row or its second), the
/// text a sign shows instead of an empty one by that one's title, a text the file alone
/// explains (its note) so.
pub(super) fn text_words(depot: &linehof::Depot, k: usize) -> (String, String) {
    use linehof::Role;
    let tr = |s: &str| omsi_ui::tr(s).into_owned();
    let role = depot.roles.get(k).copied().unwrap_or(Role::Other);
    let help = match role {
        Role::IbisDisplay => tr("What the driver's IBIS shows as the destination, in capitals."),
        Role::FrontTop if depot.one_row_bottom => tr("The top row of the destination sign over the windscreen, for a name of two rows. This depot file writes a shorter name on the bottom row alone; the buses move it up."),
        Role::FrontTop => tr("The top row of the destination sign over the windscreen. A name that fits stays on this row alone."),
        Role::FrontBottom if depot.one_row_bottom => tr("The second row of the front sign. This depot file writes a name that fits on this row alone, and the buses move it up; a longer one is split over both rows."),
        Role::FrontBottom => tr("The second row of the front sign, for a name too long for one. Empty: the top row is shown alone."),
        Role::Side => tr("The one-row sign by the doors (on some buses at the back too)."),
        Role::ClearName => tr("The destination as it is written, for the IBIS 2, the ticket printer and the screens inside the bus."),
        Role::RollerBlind => tr("The picture of a roller-blind sign (a .tga file): only buses with a roller blind read it."),
        Role::ExtraSign => tr("A sign behind the windscreen, by its number. Empty: none."),
        Role::Bitmap => tr("A picture a Krüger LED sign shows instead of the text (a .bmp file). Empty: the text is shown."),
        Role::Special => tr("Anything written here makes it a special trip: the sign shows no line number. Leave it empty for a line."),
        Role::Rear => tr("The small sign at the back, on some buses only."),
        // (by the title its field has here: the file's note counts its strings from 0)
        Role::Fallback(n) => {
            let n = usize::from(n);
            let other = text_title(depot.roles.get(n).copied().unwrap_or(Role::Other), n);
            tr("Read only by some buses' signs; when it is empty, they show “%{text}” instead.").replace("%{text}", &other)
        }
        Role::Other if depot.notes.get(k).is_some_and(|n| !n.trim().is_empty()) => tr("Only the depot file says what this is for (below); most buses do not read it."),
        Role::Other => tr("The depot file does not say what this text is for, and most buses do not read it: empty is safe."),
    };
    (text_title(role, k), help)
}

/// What the depot file the line has (`chosen`, "" for the map's) does, under its choice.
fn depot_hint(chosen: &str) -> &'static str {
    if chosen.is_empty() {
        "Choose one of your own: every bus that drives the line is given it."
    } else {
        "Every bus that drives the line is given it; the timetable's buses keep the map's (it has the line too)."
    }
}

/// Why the buses' displays and IBIS cannot know the line: its depot group names no depot file,
/// or that file is beside no bus.
fn no_depot_text(v: &LineEditorView, group: &str) -> String {
    let hof = v.hof_of(group);
    if hof.is_empty() {
        omsi_ui::tr("The depot group has no depot file: the buses' displays and IBIS cannot know the line.").into_owned()
    } else {
        omsi_ui::tr("The depot file %{hof} was not found beside any bus: the displays and the IBIS cannot know the line.").replace("%{hof}", &hof)
    }
}

// --- the kind of service and the buses ----------------------------------------------------------

/// The installed buses by maker and model, as a line's buses are chosen from them: (maker, its
/// models - (model, its versions - (bus file, version))), in the bus picker's order.
type Catalogue = Vec<(String, Vec<(String, Vec<(String, String)>)>)>;

fn catalogue(vehicles: &[core::VehicleInfo]) -> Catalogue {
    use crate::words::name_cmp;
    let mut out: Catalogue = Vec::new();
    for v in vehicles {
        let (maker, _) = service::maker_model(&v.manufacturer, &v.type_name, &v.file, &v.folder);
        let (model, version) = crate::words::split_type(&v.type_name, &v.file, &v.default_paint);
        let m = match out.iter().position(|x| x.0.eq_ignore_ascii_case(&maker)) {
            Some(k) => k,
            None => {
                out.push((maker.clone(), Vec::new()));
                out.len() - 1
            }
        };
        let models = &mut out[m].1;
        let k = match models.iter().position(|x| x.0.eq_ignore_ascii_case(&model)) {
            Some(k) => k,
            None => {
                models.push((model.clone(), Vec::new()));
                models.len() - 1
            }
        };
        models[k].1.push((v.file.clone(), version));
    }
    out.sort_by(|a, b| name_cmp(&a.0, &b.0));
    for (_, models) in out.iter_mut() {
        models.sort_by(|a, b| name_cmp(&a.0, &b.0));
        for (_, versions) in models.iter_mut() {
            versions.sort_by(|a, b| name_cmp(&a.1, &b.1));
        }
    }
    out
}

/// The bus a line's choice gets from the catalogue's maker `m`, its model `k` (0: all of
/// them) and its version `j` (0: all of them): with the bus files it covers now.
fn pick_of(cat: &Catalogue, m: usize, k: usize, j: usize) -> Option<BusPick> {
    let (maker, models) = cat.get(m)?;
    if k == 0 {
        let files = models.iter().flat_map(|x| x.1.iter().map(|v| v.0.clone())).collect();
        return Some(BusPick { maker: maker.clone(), label: maker.clone(), files, ..Default::default() });
    }
    let (model, versions) = models.get(k - 1)?;
    if j == 0 {
        let files = versions.iter().map(|v| v.0.clone()).collect();
        return Some(BusPick { maker: maker.clone(), model: model.clone(), label: format!("{maker} {model}"), files, ..Default::default() });
    }
    let (file, version) = versions.get(j - 1)?;
    Some(BusPick { maker: maker.clone(), model: model.clone(), file: file.clone(), files: vec![file.clone()], label: format!("{maker} {model} · {version}") })
}

/// The timetable a line of `kind` starts from: `lines::days_for`, and - for a company's
/// regular line - the rush hours in it, as a new company line has them.
fn typical_days(kind: ServiceKind, company: bool) -> Vec<reg::DayPattern> {
    let mut days = reg::days_for(kind);
    if company && kind == ServiceKind::Regular {
        for (k, p) in days.iter_mut().enumerate() {
            p.bands = reg::default_bands(k);
        }
    }
    days
}

/// `YYYYMMDD` as the company's pages write a day ("Tue 5 Mar 1989").
fn day_of_code(code: i32) -> String {
    crate::words::day_label(&format!("{:04}-{:02}-{:02}", code / 10000, code / 100 % 100, code % 100))
}

// --- the timetable as a table -------------------------------------------------------------------

/// A time of the table as it is shown: "07:05" (a trip past midnight from 00:00 on).
fn hm(minutes: f32) -> String {
    let m = minutes.round() as i32;
    format!("{:02}:{:02}", m.div_euclid(60).rem_euclid(24), m.rem_euclid(60))
}

/// A time typed into the table: "7:05", "07.05", "0705" or "705" (None: not a time).
#[cfg(test)]
fn parse_hm(s: &str) -> Option<i32> {
    let s = s.trim();
    let (h, m) = match s.split_once([':', '.', 'h']) {
        Some((h, m)) => (h.trim().parse::<i32>().ok()?, m.trim().parse::<i32>().ok()?),
        None if s.len() >= 3 && s.chars().all(|c| c.is_ascii_digit()) => (s[..s.len() - 2].parse().ok()?, s[s.len() - 2..].parse().ok()?),
        None => return None,
    };
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// A time being typed is whole, taken without Enter: four digits ("0705"), or two of minutes
/// after ':', '.' or 'h' ("7:05") - not "07:0", which is on its way to "07:05".
#[cfg(test)]
fn whole_time(s: &str) -> bool {
    let s = s.trim();
    match s.split_once([':', '.', 'h']) {
        Some((_, m)) => m.trim().len() == 2,
        None => s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()),
    }
}

/// A time typed (`m`, minutes of the day) for a cell that held `before`: the day's or the next
/// day's, whichever is nearer to it - so 00:05 in a trip of the evening is after midnight, and
/// 23:59 typed over a stop reached at 00:02 the day's again.
fn typed_minutes(m: i32, before: f32) -> f32 {
    let m = m as f32;
    if (m + 1440.0 - before).abs() < (m - before).abs() {
        m + 1440.0
    } else {
        m
    }
}

// --- working for the bus company ---------------------------------------------------------------

/// A new line in the registry, chosen: the player's own, or - working for the company - the
/// company's draft in its colour, driven by its depot's buses.
fn new_line(v: &mut LineEditorView) {
    let group = match &v.company {
        Some(fc) => company_group(&v.groups, &fc.depot),
        None => v.groups.first().map(|g| g.0.clone()).unwrap_or_default(),
    };
    let n = v.reg.lines.len();
    let line = v.reg.add_line(&group);
    line.colour = PALETTE[n % PALETTE.len()].to_string();
    line.name = format!("{} {}", omsi_ui::tr("Line"), line.number);
    if let Some(fc) = &v.company {
        line.company = fc.id.clone();
        line.draft = true;
        line.colour = fc.colours[0].clone();
        // (the company's lines start with the rush hours in their timetable, or with the one
        // the Casual line wizard chose - with RosterFlow without one: its Timeplan makes it)
        match fc.days.as_ref() {
            _ if fc.rosterflow => {
                for p in line.days.iter_mut() {
                    (p.on, p.bands) = (false, Vec::new());
                }
                line.back_days.clear();
            }
            Some(days) if days.len() == line.days.len() => line.days = days.clone(),
            _ => {
                for (k, p) in line.days.iter_mut().enumerate() {
                    p.bands = reg::default_bands(k);
                }
            }
        }
    }
    let id = line.id;
    (v.sel, v.dir, v.sel_stop) = (Some(id), 0, None);
    v.table_reset();
    v.touched();
}

/// Where the line editor goes once the company confirmed `line`: with RosterFlow to its
/// Timeplan on the line (its timetable is made there), else nowhere - it stays.
fn after_confirm(fc: &ForCompany, line: &LineDesign) -> Option<ToTimeplan> {
    fc.rosterflow.then(|| ToTimeplan { id: Some(line.id), name: String::new(), hint: Some(NEW_LINE_HINT) })
}

/// The depot group whose depot file is the company's (else the first).
fn company_group(groups: &[(String, String)], depot: &str) -> String {
    let file = |s: &str| {
        let name = s.trim().replace('\\', "/").rsplit('/').next().unwrap_or("").to_lowercase();
        name.strip_suffix(".hof").map(str::to_string).unwrap_or(name)
    };
    groups.iter().find(|g| !depot.trim().is_empty() && file(&g.1) == file(depot)).or(groups.first()).map(|g| g.0.clone()).unwrap_or_default()
}

// --- the map ----------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle(maker: &str, type_name: &str, file: &str) -> core::VehicleInfo {
        core::VehicleInfo { name: format!("{maker} {type_name}"), manufacturer: maker.into(), type_name: type_name.into(), file: file.into(), folder: file.split('/').nth(1).unwrap_or_default().into(), description: String::new(), default_paint: String::new(), paints: Vec::new(), hofs: Vec::new(), installed: false, missing_packs: Vec::new(), numbers: Vec::new() }
    }

    #[test]
    fn a_lines_buses_are_chosen_by_maker_model_and_version() {
        let vs = vec![vehicle("Setra", "S 415 UL - 2 doors", "Vehicles/Setra/a.bus"), vehicle("Setra", "S 415 UL - 3 doors", "Vehicles/Setra/b.bus"), vehicle("MAN", "Lion's City", "Vehicles/MAN/c.bus")];
        let cat = catalogue(&vs);
        assert_eq!(cat.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["MAN", "Setra"]);
        // the maker: all its buses; a model; one version
        let maker = pick_of(&cat, 1, 0, 0).unwrap();
        assert_eq!((maker.label.as_str(), maker.files.len(), maker.model.as_str()), ("Setra", 2, ""));
        let model = pick_of(&cat, 1, 1, 0).unwrap();
        assert_eq!((model.label.as_str(), model.files.len()), ("Setra S 415 UL", 2));
        let one = pick_of(&cat, 1, 1, 2).unwrap();
        assert_eq!((one.file.as_str(), one.label.as_str()), ("Vehicles/Setra/b.bus", "Setra S 415 UL · 3 doors"));
        assert!(pick_of(&cat, 5, 0, 0).is_none());
        // a kind's typical timetable: the company's regular line with its rush hours
        assert!(typical_days(ServiceKind::Regular, true).iter().all(|d| !d.bands.is_empty()));
        assert!(typical_days(ServiceKind::School, false)[1..].iter().all(|d| !d.on));
    }

    #[test]
    fn the_tables_times_are_typed_and_shown() {
        assert_eq!(parse_hm("7:05"), Some(425));
        assert_eq!(parse_hm("07.05"), Some(425));
        assert_eq!(parse_hm("0705"), Some(425));
        assert_eq!(parse_hm("705"), Some(425));
        assert_eq!(parse_hm("23:59"), Some(1439));
        assert_eq!(parse_hm("24:00"), None);
        assert_eq!(parse_hm("7:65"), None);
        assert_eq!(parse_hm("x"), None);
        assert_eq!(parse_hm("07"), None);
        assert_eq!(hm(425.0), "07:05");
        // (a trip past midnight from 00:00 on)
        assert_eq!(hm(1445.0), "00:05");
        // taken without Enter only once whole: "07:0" is on its way to "07:05"
        assert!(whole_time("0705") && whole_time("7:05") && whole_time("07:05") && whole_time("17.45"));
        assert!(!whole_time("07:0") && !whole_time("12:3") && !whole_time("17.4") && !whole_time("705") && !whole_time("070"));
        // the day's or the next day's, whichever is nearer to what the cell held
        assert_eq!(typed_minutes(425, 420.0), 425.0);
        assert_eq!(typed_minutes(5, 1430.0), 1445.0);
        assert_eq!(typed_minutes(10, 1442.0), 1450.0);
        assert_eq!(typed_minutes(1439, 1442.0), 1439.0);
        assert_eq!(typed_minutes(1380, 600.0), 1380.0);
    }

    #[test]
    fn a_stop_is_named_by_its_own_tile_where_ids_repeat() {
        let entry = |name: &str, group: i32, id: i64| omsi_timetable::BusStopEntry { name: name.into(), group, object_id: id, params: [0.0; 3] };
        let tiles = [(0, 0), (0, 1), (5, 5)];
        // (id 7 on two tiles; id 9 once, its tile not in global.cfg's list)
        let names = stop_names(&[entry("Rathaus", 0, 7), entry("Bahnhof", 2, 7), entry("Markt", 9, 9), entry("Nowhere", 9, 7)], &tiles);
        assert_eq!(stop_name(&names, (0, 0), 7).map(String::as_str), Some("Rathaus"));
        assert_eq!(stop_name(&names, (5, 5), 7).map(String::as_str), Some("Bahnhof"));
        assert_eq!(stop_name(&names, (0, 1), 7), None);
        assert_eq!(stop_name(&names, (0, 1), 9).map(String::as_str), Some("Markt"));
    }

    #[test]
    fn the_kinds_and_my_duties_are_translated() {
        let mut keys: Vec<&str> = ServiceKind::ALL.iter().flat_map(|k| [k.label(), k.note()]).collect();
        keys.extend(VehicleClass::ALL.iter().map(|c| c.label()));
        keys.extend(["School contract", "Fares and booking fees", "Tourism grant per km", "My duties", "Up next", "THIS LINE"]);
        keys.extend(["Public title", "Fill from the patterns", "Trip %{n}", "My destinations", "As advertised", "%{n} trip(s) of the table reach a stop before they left the one before"]);
        // (the sizes a company's level keeps locked)
        keys.extend(["locked", "Your company's level does not open these buses yet"]);
        keys.push("Deleted from your lines, but there is no content folder to take it out of the timetable");
        // (a change of a line in service)
        keys.extend(["Change line %{n} in service", "Route approved anew", "%{k} kept, %{c} changed, %{a} new, %{g} dropped", "%{n} tours to plan anew", "Save the change", "Keep the line as it runs", "The new timetable begins on %{date}.", "A change of a line in service begins tomorrow at the soonest.", "New timetable from %{date}", "Line %{n} runs its new timetable from today: %{d} duties and %{b} buses given anew."]);
        for p in service::default_school_holidays() {
            keys.push(Box::leak(p.name.into_boxed_str()));
        }
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in &keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
        assert_eq!(crate::_rust_i18n_try_translate("nl", "School transport").as_deref(), Some("Leerlingenvervoer"));
        // (and each kind's mark is one of the interface's icons)
        assert!(ServiceKind::ALL.iter().all(|k| omsi_ui::icons::svg(k.icon()).is_some()));
    }

    /// Every text of a depot file is called something and says what it does, its number
    /// counted from 1 as the player reads them.
    #[test]
    fn every_text_of_a_depot_file_says_what_it_is_for() {
        use linehof::{Depot, Role};
        let depot = |roles: &[Role], notes: &[&str], one_row_bottom: bool| Depot { roles: roles.to_vec(), notes: notes.iter().map(|n| n.to_string()).collect(), one_row_bottom, ..Default::default() };
        let all = [Role::IbisDisplay, Role::FrontTop, Role::FrontBottom, Role::Side, Role::RollerBlind, Role::ClearName, Role::ExtraSign, Role::Bitmap, Role::Special, Role::Rear, Role::Fallback(0), Role::Other];
        for one_row_bottom in [false, true] {
            for note in ["", "a note"] {
                let d = depot(&all, &[note; 12], one_row_bottom);
                for (k, role) in all.iter().enumerate() {
                    let (title, help) = text_words(&d, k);
                    assert!(!title.trim().is_empty() && !help.trim().is_empty() && !title.contains("%{") && !help.contains("%{"), "{role:?}: {title} / {help}");
                    assert_eq!(title, text_title(*role, k));
                }
            }
        }
        assert_eq!(text_title(Role::Other, 6), "Text 7");
        let (plain, noted) = (depot(&[Role::Other], &[""], false), depot(&[Role::Other], &["Liniencode"], false));
        assert_ne!(text_words(&plain, 0).1, text_words(&noted, 0).1, "a note of the file's: said so");
        // a text shown in another's place names that one by its field's title, not by the
        // number of the file's note (which counts from 0)
        let wien = depot(&[Role::IbisDisplay, Role::FrontTop, Role::FrontBottom, Role::Fallback(1), Role::Fallback(9)], &["", "", "", "bleibt dieser String leer, wird String 1 ausgelesen", "leer: String 9"], false);
        assert!(text_words(&wien, 3).1.contains("“Front sign, top row”"), "{}", text_words(&wien, 3).1);
        assert!(text_words(&wien, 4).1.contains("“Text 10”"), "{}", text_words(&wien, 4).1);
        // the front's rows as the file writes a name of one row: on the first (the HOF
        // Suite's), or on the second (Spandau's)
        let usual = [Role::IbisDisplay, Role::FrontTop, Role::FrontBottom];
        let (first, second) = (depot(&usual, &[], false), depot(&usual, &[], true));
        assert!(text_words(&first, 1).1.contains("A name that fits stays on this row alone") && text_words(&first, 2).1.contains("too long for one"));
        assert!(text_words(&second, 1).1.contains("a name of two rows") && text_words(&second, 2).1.contains("a name that fits on this row alone"));
        // a destination alike in other punctuation, not one shortened
        assert!(same_words("Hohenkirchen, Therme", "hohenkirchen  therme"));
        assert!(!same_words("Hohenkirchen, Th", "Hohenkirchen, Therme"));
        // ... nor in the capitals a sign writes it in
        assert!(same_words("HEERSTRASSE ", "Heerstraße"));
    }

    /// A depot file of Spandau's kind: its texts in capitals, a name of one row on the front's
    /// second row, centred - written for the test.
    const WESTHAFEN: &str = "[name]\r\nWesthafen\r\n\r\nstringcount_terminus\r\n6\r\n\r\n\
        \tstring0:\tIBIS-Anzeige, max. 16 Zeichen\r\n\tstring1:\tMatrix vorne, 1. Zeile\r\n\tstring2:\tMatrix vorne, 2. Zeile\r\n\
        \tstring3:\tMatrix Seite\r\n\tstring4:\tRollbandtextur\r\n\tstring5:\tIBIS2 (Klarname), max 20 Zeichen\r\n\r\nstringcount_busstop\r\n4\r\n\r\n\
        [addterminus]\r\n11\r\nHafen\r\nHAFEN\r\n\r\n      HAFEN\r\n      HAFEN\r\nWH_Hafen.tga\r\nHafen\r\n\r\n\
        [addterminus]\r\n12\r\nRathaus\r\nRATHAUS\r\n   WESTHAFEN\r\n    RATHAUS\r\n    RATHAUS\r\nWH_Rathaus.tga\r\nRathaus Westhafen\r\n\r\n\
        [addterminus]\r\n14\r\nKlinik\r\nKLINIK\r\n\r\n     KLINIK\r\n     KLINIK\r\nWH_Klinik.tga\r\nKlinik\r\n\r\n";

    /// The Simple view says a name is shortened only when the front lost some of it: a name
    /// with "ß" that fits, written in capitals ("HEERSTRASSE"), is whole.
    #[test]
    fn a_name_that_fits_is_not_said_to_be_shortened() {
        let depot = linehof::Depot::from_text(Path::new("Westhafen.hof"), WESTHAFEN, "Westhafen");
        assert!(depot.one_row_bottom);
        let to = |name: &str| Direction { terminus: name.into(), ..Default::default() };
        assert_eq!(depot.front(&depot.sign_defaults("Heerstraße")), ("HEERSTRASSE".to_string(), String::new()));
        assert!(!shortened_on_front(&depot, &to("Heerstraße")));
        assert!(!shortened_on_front(&depot, &to("Flugplatz")) && !shortened_on_front(&depot, &to("Rathaus")));
        let long = "Krankenhaus Nordstadt Westflügel";
        assert!(shortened_on_front(&depot, &to(long)));
        // (rows of the player's own: his to judge)
        let mut d = to(long);
        d.sign = vec![String::new(), "KRANKENHAUS".into(), "NORDSTADT".into()];
        assert!(!shortened_on_front(&depot, &d));
    }

    /// A registry whose blocks were written the old way is marked as written anew only once
    /// a write of them went well: else the editor tries again when it opens the map.
    #[test]
    fn the_blocks_are_marked_new_only_once_written() {
        let dir = std::env::temp_dir().join(format!("openomsi-texts-mark-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("Talstadt.json");
        let mut r = Registry { texts: 0, ..Default::default() };
        reg::save_registry(&path, &r).unwrap();
        // a write that failed (or none, without a content folder): no mark
        texts_written(&path, &mut r, false);
        assert_eq!((r.texts, reg::load_registry(&path).texts), (0, 0));
        texts_written(&path, &mut r, true);
        assert_eq!((r.texts, reg::load_registry(&path).texts), (reg::TEXTS_VERSION, reg::TEXTS_VERSION));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_rosterflow_line_texts_are_translated() {
        let mut keys = vec![NEW_LINE_HINT, TIMETABLE_LATER, TIMETABLE_THERE, "Confirm the route here: the line runs no tour until its timetable is saved there.", "No timetable yet", "Open in the Timeplan", "Save the line's changes first (or undo them): the Timeplan opens the line as it is saved.", "Estimated with a usual timetable: you make the line's own in RosterFlow's Timeplan", "Line %{n} is the company's now: %{amount} paid. Make its timetable in RosterFlow's Timeplan."];
        keys.extend(["Timetable: the line's running times and departures in RosterFlow's Timeplan", "Edit the line in the line editor: its route and displays", "Make it", "The line runs no tour until it has a timetable. Click to make it in RosterFlow's Timeplan.", "The line runs no tour until it has a timetable. Click to make it in the line editor."]);
        keys.extend(["Draw a line of the company's own in the line editor: you confirm and pay for its route there, then make its timetable in RosterFlow's Timeplan", "Line %{n} is the company's: here is its timetable. Plan its tours and start its service on the Roster tab."]);
        keys.extend([co::rosterflow::I_NO_TIMETABLE, co::rosterflow::issue_title(co::rosterflow::I_NO_TIMETABLE)]);
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in &keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
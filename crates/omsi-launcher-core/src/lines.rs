//! The player's own lines: composed in the launcher's line editor, kept in a registry per map
//! (`~/.openomsi/lines/<map folder>.json`) and written out as ordinary OMSI timetable files,
//! so that the player and the timetable's AI buses drive them like any line of the map.
//!
//! The registry is the source of truth - stable ids, the stops, the lanes between them, the
//! times and the day patterns - and the files are made from it again on every save (and after
//! a reset of the map's timetable). It is also what a later bus company builds on: a line
//! there is a line here.
//!
//! What a line becomes in the map's `TTData` (see `export`):
//! * `<stem>.ttl` - `[userallowed]`, and one tour per bus of each day pattern;
//! * `<stem>_a.ttp` / `<stem>_b.ttp` - one trip per direction: its stops, the terminus, the
//!   line number, a profile with the time to every stop;
//! * `<stem>_a.ttr` / `<stem>_b.ttr` - the lanes the trip drives (the trip names it);
//! * `StnLinks.cfg` - a link for every pair of stops the map has none for (the map's own are
//!   never changed: other lines drive them), `Busstops.cfg` - the stops the map does not name.
//!
//! Every file name starts with `oo_`, so none can take the place of one of the map's. What the
//! destination displays and the IBIS need of a line goes into the depot files of its buses
//! (`linehof`).

use omsi_timetable::{BusStopEntry, Line, StnLink, StnLinkEntry, Tour, TourTrip, Track, TrackEntry, Trip, TripProfile};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The registry's format; a newer one is read as far as it is understood.
pub const REGISTRY_VERSION: u32 = 1;
/// What every file a player's line writes begins with.
pub const FILE_PREFIX: &str = "oo_";
/// The list of the files the line editor wrote into a `TTData` folder (they are deleted
/// before it writes again, so a line renamed or deleted leaves nothing behind).
pub const MANIFEST: &str = "openomsi-lines.txt";

/// The days of a pattern (bits 0 - 6 Monday to Sunday, 7 public holidays): working days,
/// Saturday, Sunday and public holidays.
pub const DAY_GROUPS: [(&str, u16); 3] = [("Mon - Fri", 0b0001_1111), ("Saturday", 0b0010_0000), ("Sunday", 0b1100_0000)];
/// Both school bits: a tour runs in the school holidays (bit 8, as the game reads it) and on
/// school days (bit 9) alike - the game takes a tour only when its mask has the day's bit
/// and the school bit of the date.
pub const SCHOOL_BITS: i32 = 0b11_0000_0000;

/// The speed model for the running times: an average on the road between two stops and the
/// time a stop takes (braking, the doors, pulling out).
pub const CRUISE_MS: f32 = 8.3;
pub const STOP_S: f32 = 20.0;

/// The registry of one map.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Registry {
    pub version: u32,
    /// The map's folder (`maps/<folder>`), and its `global.cfg` as the launcher names it.
    pub map: String,
    pub global: String,
    pub next_id: u64,
    pub lines: Vec<LineDesign>,
    /// The depot files (by the name the depot groups give them) the line editor wrote its
    /// block into last time: a line deleted, or moved to another depot, takes its block out.
    pub depots: Vec<String>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry { version: REGISTRY_VERSION, map: String::new(), global: String::new(), next_id: 1, lines: Vec::new(), depots: Vec::new() }
    }
}

/// A player's line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LineDesign {
    /// Stable for the life of the line (the file names follow the number; this does not).
    pub id: u64,
    /// What the plate and the destination displays show.
    pub number: String,
    pub name: String,
    /// `#rrggbb`.
    pub colour: String,
    /// The `ailists.cfg` depot group whose buses drive it.
    pub ai_group: String,
    /// Outbound, then the way back (a line of one direction runs it round).
    pub directions: Vec<Direction>,
    /// One per `DAY_GROUPS` entry.
    pub days: Vec<DayPattern>,
    /// Seconds since 1970.
    pub created: u64,
    pub modified: u64,
}

impl Default for LineDesign {
    fn default() -> Self {
        LineDesign {
            id: 0,
            number: String::new(),
            name: String::new(),
            colour: "#2a75f7".into(),
            ai_group: String::new(),
            directions: vec![Direction::default()],
            days: default_days(),
            created: 0,
            modified: 0,
        }
    }
}

/// A stop of a direction: the map object the timetable names (its tile, as object ids repeat
/// across the tiles of some maps), its name and where it stands (world metres).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct StopRef {
    pub tile: [i32; 2],
    pub id: i64,
    pub name: String,
    pub at: [f64; 2],
    /// What the IBIS shows for it (empty: the depot file's own name for the stop, else one
    /// made from its name; see `linehof`).
    pub ibis: String,
}

/// A lane a leg drives: the game's own `LaneKey` and its direction, and its length.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct LaneStep {
    pub tile: [i32; 2],
    pub id: i64,
    pub path: u16,
    pub reversed: bool,
    pub length: f32,
}

/// The way from one stop to the next.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Leg {
    /// Points the player dragged the leg through (world metres), in order.
    pub vias: Vec<[f64; 2]>,
    pub steps: Vec<LaneStep>,
    /// Metres from stop to stop.
    pub length: f32,
    /// A way over the roads was found (a leg without one keeps the line from being saved).
    pub ok: bool,
    /// Where the stops lie on the first and the last lane (along it, and beside it), as
    /// `StnLinks.cfg` keeps it.
    pub from_s: f32,
    pub to_s: f32,
    pub from_lat: f32,
    pub to_lat: f32,
}

/// One direction of a line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Direction {
    /// The destination (a terminus of the map's depot file, or free text): the last stop's
    /// name when empty.
    pub terminus: String,
    pub stops: Vec<StopRef>,
    /// `stops.len() - 1` of them.
    pub legs: Vec<Leg>,
    /// Minutes from the first stop to every stop (the first 0).
    pub times: Vec<f32>,
    /// The times were set by hand (else they follow the legs: `auto_times`).
    pub manual_times: bool,
    /// The destination displays when the destination is no terminus of the depot file (it
    /// gets an `[addterminus]` of its own): one text per string of the depot file, an empty
    /// one taking the default made from the destination (`linehof::Depot::sign_defaults`).
    pub sign: Vec<String>,
    /// That new terminus's code in the depot file (given when the line is saved).
    pub terminus_code: i32,
    /// The IBIS route code, the line's number × 100 and two digits (0: given when the line is
    /// saved; one the depot file has already is given anew).
    pub ibis_route: u32,
}

/// When a line runs on the days of one `DAY_GROUPS` entry: buses from the first to the last
/// departure every `headway` minutes, each standing at least `layover` at the end.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DayPattern {
    pub on: bool,
    pub days: u16,
    /// Minutes after midnight.
    pub first: f32,
    pub last: f32,
    pub headway: f32,
    pub layover: f32,
}

impl Default for DayPattern {
    fn default() -> Self {
        DayPattern { on: true, days: DAY_GROUPS[0].1, first: 6.0 * 60.0, last: 22.0 * 60.0, headway: 20.0, layover: 5.0 }
    }
}

/// Working days every 20 minutes, Saturdays every 30, Sundays every 60.
pub fn default_days() -> Vec<DayPattern> {
    vec![
        DayPattern { days: DAY_GROUPS[0].1, ..Default::default() },
        DayPattern { days: DAY_GROUPS[1].1, first: 7.0 * 60.0, headway: 30.0, ..Default::default() },
        DayPattern { days: DAY_GROUPS[2].1, first: 8.0 * 60.0, last: 21.0 * 60.0, headway: 60.0, ..Default::default() },
    ]
}

/// The tour mask a pattern's days are written as.
pub fn mask_of(days: u16) -> i32 {
    days as i32 & 0xff | SCHOOL_BITS
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// --- the registry on disk --------------------------------------------------------------------

/// The map folder of a map file as the launcher names it (`maps/Grundorf/global.cfg` →
/// `Grundorf`).
pub fn map_folder(map_file: &str) -> String {
    let p = Path::new(&map_file.replace('\\', "/")).to_path_buf();
    p.parent().and_then(|d| d.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Where the registry of a map lies (in `~/.openomsi/lines`).
pub fn registry_path(map_folder: &str) -> PathBuf {
    crate::data_dir().join("lines").join(format!("{}.json", safe_name(map_folder)))
}

/// The registry in `path` (an empty one when there is none, or it cannot be read).
pub fn load_registry(path: &Path) -> Registry {
    match std::fs::read(path) {
        Ok(b) => serde_json::from_slice::<Registry>(&b).unwrap_or_else(|_| {
            // (kept aside, so that the next save does not write an empty registry over it)
            let _ = std::fs::copy(path, path.with_extension(format!("json.broken-{}", now_secs())));
            Registry::default()
        }),
        Err(_) => Registry::default(),
    }
}

/// The registry written to `path` (through a file beside it: a crash halfway leaves the old).
pub fn save_registry(path: &Path, reg: &Registry) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(reg).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl Registry {
    /// A new, empty line with the next id (and the number after the highest).
    pub fn add_line(&mut self, ai_group: &str) -> &mut LineDesign {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        let number = (self.lines.iter().filter_map(|l| l.number.trim().parse::<u32>().ok()).max().unwrap_or(0) + 1).to_string();
        let t = now_secs();
        self.lines.push(LineDesign { id, number, ai_group: ai_group.to_string(), created: t, modified: t, ..Default::default() });
        self.lines.last_mut().unwrap()
    }

    pub fn line(&self, id: u64) -> Option<&LineDesign> {
        self.lines.iter().find(|l| l.id == id)
    }

    pub fn line_mut(&mut self, id: u64) -> Option<&mut LineDesign> {
        self.lines.iter_mut().find(|l| l.id == id)
    }
}

/// A name with the characters a file name cannot hold taken out.
pub fn safe_name(s: &str) -> String {
    let t: String = s.trim().chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '+' | '.') { c } else { '_' }).collect();
    let t = t.trim_matches('.').to_string();
    if t.is_empty() { "line".into() } else { t }
}

// --- times ------------------------------------------------------------------------------------

/// Seconds a leg of `length` metres takes (`CRUISE_MS`, plus `STOP_S` for the stop it ends at).
pub fn leg_seconds(length: f32) -> f32 {
    STOP_S + length.max(0.0) / CRUISE_MS
}

/// Minutes from the first stop to every stop, from the legs' lengths, in whole minutes (the
/// sum rounded, so the rounding does not pile up).
pub fn auto_times(legs: &[Leg]) -> Vec<f32> {
    let mut out = vec![0.0f32];
    let mut acc = 0.0f32;
    for l in legs {
        acc += leg_seconds(l.length);
        let m = (acc / 60.0).round().max(out.last().copied().unwrap_or(0.0));
        out.push(m);
    }
    // (a trip takes at least a minute)
    if out.len() > 1 && out.last().copied().unwrap_or(0.0) < 1.0 {
        *out.last_mut().unwrap() = 1.0;
    }
    out
}

impl Direction {
    /// Its times again from the legs, unless they were set by hand (and then only made to fit
    /// the stops there are).
    pub fn refresh_times(&mut self) {
        let n = self.stops.len();
        if !self.manual_times || self.times.len() != n {
            self.times = auto_times(&self.legs);
            self.times.resize(n, self.times.last().copied().unwrap_or(0.0));
            if n == 0 {
                self.times.clear();
            }
            self.manual_times = false;
        }
    }

    /// Minutes from the first stop to the last.
    pub fn minutes(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// The whole trip made to take `total` minutes: every stop's time scaled with it.
    pub fn set_total(&mut self, total: f32) {
        let old = self.minutes();
        if total < 1.0 || old <= 0.0 {
            return;
        }
        for t in &mut self.times {
            *t = (*t * total / old).round();
        }
        if let Some(l) = self.times.last_mut() {
            *l = total.round();
        }
        self.manual_times = true;
    }

    /// Stop `i` (and every stop after it) `by` minutes later; never before the stop ahead.
    pub fn shift_from(&mut self, i: usize, by: f32) {
        if i == 0 || i >= self.times.len() {
            return;
        }
        let floor = self.times[i - 1];
        let d = (self.times[i] + by).max(floor) - self.times[i];
        for t in &mut self.times[i..] {
            *t += d;
        }
        self.manual_times = true;
    }

    /// The legs fit the stops (one fewer), new ones empty.
    pub fn fit_legs(&mut self) {
        let n = self.stops.len().saturating_sub(1);
        self.legs.resize(n, Leg::default());
    }

    /// The destination it shows: the terminus chosen, else the last stop's name.
    pub fn destination(&self) -> String {
        if self.terminus.trim().is_empty() {
            self.stops.last().map(|s| s.name.trim().to_string()).unwrap_or_default()
        } else {
            self.terminus.trim().to_string()
        }
    }
}

/// The way back of `out`: its stops from the last to the first, each the stop of the same
/// name across the road (the nearest within `reach` metres, of `all` the map's stops), else
/// the same one.
pub fn opposite_stops(out: &[StopRef], all: &[StopRef], reach: f64) -> Vec<StopRef> {
    let dist = |a: &StopRef, b: &StopRef| ((a.at[0] - b.at[0]).powi(2) + (a.at[1] - b.at[1]).powi(2)).sqrt();
    out.iter()
        .rev()
        .map(|s| {
            let name = s.name.trim().to_lowercase();
            all.iter()
                .filter(|o| (o.id, o.tile) != (s.id, s.tile) && !name.is_empty() && o.name.trim().to_lowercase() == name && dist(o, s) <= reach)
                .min_by(|a, b| dist(a, s).total_cmp(&dist(b, s)))
                .cloned()
                .unwrap_or_else(|| s.clone())
        })
        .collect()
}

// --- tours from a pattern ------------------------------------------------------------------

/// A trip of a bus's day: the direction and its departure (minutes after midnight).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planned {
    pub dir: usize,
    pub departure: f32,
}

/// The buses a pattern needs and the trips of each, in order: every direction leaves from its
/// first to its last departure every `headway` minutes, and each departure is given to the
/// bus that has stood longest at that end, ready (`run` minutes of its trip and the
/// `layover` after it), else to a new bus. A bus so drives back and forth; with one direction
/// it goes round.
pub fn blocks(run: &[f32], p: &DayPattern) -> Vec<Vec<Planned>> {
    let dirs = run.len().clamp(1, 2);
    if !p.on || p.headway < 1.0 || p.last < p.first {
        return Vec::new();
    }
    let mut deps: Vec<Planned> = Vec::new();
    for dir in 0..dirs {
        let mut t = p.first;
        while t <= p.last + 1e-3 && deps.len() < 4000 {
            deps.push(Planned { dir, departure: t });
            t += p.headway;
        }
    }
    deps.sort_by(|a, b| a.departure.total_cmp(&b.departure).then(a.dir.cmp(&b.dir)));
    // (where a direction leaves from and where it ends: the two ends of the line, or the one
    // end of a line that goes round)
    let from = |d: usize| if dirs == 1 { 0 } else { d };
    let to = |d: usize| if dirs == 1 { 0 } else { 1 - d };
    struct Bus {
        at: usize,
        free: f32,
        trips: Vec<Planned>,
    }
    let mut buses: Vec<Bus> = Vec::new();
    for d in deps {
        let ready = buses.iter().enumerate().filter(|(_, b)| b.at == from(d.dir) && b.free <= d.departure + 1e-3).min_by(|a, b| a.1.free.total_cmp(&b.1.free)).map(|(i, _)| i);
        let i = ready.unwrap_or_else(|| {
            buses.push(Bus { at: from(d.dir), free: 0.0, trips: Vec::new() });
            buses.len() - 1
        });
        let b = &mut buses[i];
        b.trips.push(d);
        b.at = to(d.dir);
        b.free = d.departure + run[d.dir].max(1.0) + p.layover.max(0.0);
    }
    buses.into_iter().map(|b| b.trips).collect()
}

// --- the files --------------------------------------------------------------------------------

/// The file stem of each line: `oo_<number>`, with its id after it when another line of the
/// registry has the same number.
pub fn stems(reg: &Registry) -> HashMap<u64, String> {
    let mut count: HashMap<String, usize> = HashMap::new();
    for l in &reg.lines {
        *count.entry(safe_name(&l.number).to_lowercase()).or_default() += 1;
    }
    reg.lines
        .iter()
        .map(|l| {
            let n = safe_name(&l.number);
            let stem = if count.get(&n.to_lowercase()).copied().unwrap_or(0) > 1 { format!("{FILE_PREFIX}{n}_{}", l.id) } else { format!("{FILE_PREFIX}{n}") };
            (l.id, stem)
        })
        .collect()
}

/// The trip file of direction `dir` of a line with file stem `stem`.
pub fn trip_name(stem: &str, dir: usize) -> String {
    format!("{stem}_{}", if dir == 0 { "a" } else { "b" })
}

// --- the player's lines in the launcher's lists ----------------------------------------------

/// A player's line as the launcher's line lists show it, among the map's: the timetable line
/// it was written as, and what the line editor knows of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnLine {
    /// 0: a line of the editor's whose registry is gone (only its files are left).
    pub id: u64,
    /// The `.ttl` file's stem: the timetable line's name (`LineInfo::name`).
    pub file: String,
    pub number: String,
    pub name: String,
    /// `#rrggbb`.
    pub colour: String,
    /// Where its directions go, outbound first, each once.
    pub destinations: Vec<String>,
}

impl OwnLine {
    /// "Ring · Markt – Bahnhof": its name and where it goes (what of it there is).
    pub fn caption(&self) -> String {
        let to = self.destinations.join(" – ");
        [self.name.trim(), to.trim()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
    }
}

/// The timetable line `name` is a file the line editor wrote (`oo_…`, in any case).
pub fn is_own_file(name: &str) -> bool {
    name.trim().get(..FILE_PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(FILE_PREFIX))
}

/// The lines of the registry that are in the map's timetable (the drafts `problems` keeps
/// out have no files to drive), by the file stem each was written as.
pub fn own_lines(reg: &Registry) -> Vec<OwnLine> {
    let stems = stems(reg);
    reg.lines
        .iter()
        .filter(|l| problems(l).is_empty())
        .map(|l| {
            let mut destinations: Vec<String> = Vec::new();
            for d in l.directions.iter().filter(|d| d.stops.len() >= 2) {
                let to = d.destination();
                if !to.is_empty() && !destinations.contains(&to) {
                    destinations.push(to);
                }
            }
            OwnLine { id: l.id, file: stems[&l.id].clone(), number: l.number.trim().to_string(), name: l.name.trim().to_string(), colour: l.colour.clone(), destinations }
        })
        .collect()
}

/// The player's line the timetable line `name` is: the registry's (`own`), else - a file of
/// the editor's the registry no longer has - one made from the file name (`oo_42` → 42).
/// None for a line of the map's.
pub fn own_line_of(name: &str, own: &[OwnLine]) -> Option<OwnLine> {
    let name = name.trim();
    if let Some(o) = own.iter().find(|o| o.file.eq_ignore_ascii_case(name)) {
        return Some(o.clone());
    }
    is_own_file(name).then(|| OwnLine { file: name.to_string(), number: name[FILE_PREFIX.len()..].to_string(), colour: LineDesign::default().colour, ..Default::default() })
}

/// The player's lines of the map whose `global.cfg` the launcher names `map_file`, from its
/// registry (none when it has none).
pub fn own_lines_of_map(map_file: &str) -> Vec<OwnLine> {
    let folder = map_folder(map_file);
    if folder.is_empty() {
        return Vec::new();
    }
    let path = registry_path(&folder);
    if !path.is_file() {
        return Vec::new();
    }
    own_lines(&load_registry(&path))
}

/// What saving the registry writes into a map's `TTData`: whole files by name, and the
/// links and stops to add to the map's `StnLinks.cfg` and `Busstops.cfg`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Export {
    pub files: Vec<(String, String)>,
    pub links: Vec<(StnLink, String, String)>,
    pub stops: Vec<BusStopEntry>,
    /// Tours per line id (how many buses its patterns need).
    pub tours: HashMap<u64, usize>,
}

/// Why a line cannot be saved yet: the text (an interface text, `%{n}` for the number) and
/// the number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Problem {
    pub text: &'static str,
    pub n: usize,
}

impl Problem {
    pub fn english(&self) -> String {
        self.text.replace("%{n}", &self.n.to_string())
    }
}

/// Why a line cannot be saved yet (none: it can).
pub fn problems(l: &LineDesign) -> Vec<Problem> {
    let mut out = Vec::new();
    let p = |text, n| Problem { text, n };
    if l.number.trim().is_empty() {
        out.push(p("The line needs a number", 0));
    }
    if l.ai_group.trim().is_empty() {
        out.push(p("Choose the depot whose buses drive the line", 0));
    }
    for (k, d) in l.directions.iter().enumerate() {
        if d.stops.len() < 2 {
            out.push(p(if k == 0 { "The outbound direction needs at least two stops" } else { "The way back needs at least two stops" }, 0));
            continue;
        }
        let bad = d.legs.iter().filter(|g| !g.ok).count() + (d.stops.len() - 1).saturating_sub(d.legs.len());
        if bad > 0 {
            out.push(p(if k == 0 { "%{n} leg(s) of the outbound direction have no way over the roads" } else { "%{n} leg(s) of the way back have no way over the roads" }, bad));
        }
    }
    if !l.days.iter().any(|p| p.on && p.headway >= 1.0 && p.last >= p.first) {
        out.push(p("The line runs on no day", 0));
    }
    out
}

/// The files of every line of the registry that can be saved (`problems` empty). `raw_tiles`
/// is `global.cfg`'s `[map]` list (a lane's tile is its place there); `map_links` the pairs
/// of stops the map has a link for, `map_stops` the stops its `Busstops.cfg` names.
pub fn export(reg: &Registry, raw_tiles: &[(i32, i32)], map_links: &HashSet<(i64, i64)>, map_stops: &HashSet<i64>) -> Result<Export, String> {
    let tile_index = |t: [i32; 2]| raw_tiles.iter().position(|x| *x == (t[0], t[1]));
    let stems = stems(reg);
    let mut out = Export::default();
    let mut links_done: HashSet<(i64, i64)> = map_links.clone();
    let mut stops_done: HashSet<i64> = map_stops.clone();
    for l in reg.lines.iter().filter(|l| problems(l).is_empty()) {
        let stem = &stems[&l.id];
        let number = l.number.trim().to_string();
        let mut run = Vec::new();
        for (dir, d) in l.directions.iter().enumerate() {
            let name = trip_name(stem, dir);
            let mut times = d.times.clone();
            if times.len() != d.stops.len() {
                times = auto_times(&d.legs);
                times.resize(d.stops.len(), times.last().copied().unwrap_or(0.0));
            }
            let total = times.last().copied().unwrap_or(1.0).max(1.0);
            run.push(total);
            let man_dep_time = (1..d.stops.len().saturating_sub(1)).map(|i| (i as i32, times[i])).collect();
            let trip = Trip {
                name: name.clone(),
                display_name: name.clone(),
                terminus: d.destination(),
                line: number.clone(),
                stations: d.stops.iter().map(|s| s.id).collect(),
                profiles: vec![TripProfile { name: "standard".into(), factor: total, man_dep_time, ..Default::default() }],
                ..Default::default()
            };
            out.files.push((format!("{name}.ttp"), trip.to_text()));
            let legs = &d.legs[..d.legs.len().min(d.stops.len().saturating_sub(1))];
            // the track: every leg's lanes, the one two legs share once
            let mut entries: Vec<TrackEntry> = Vec::new();
            let mut last: Option<([i32; 2], i64, u16, bool)> = None;
            for g in legs {
                for s in &g.steps {
                    let key = (s.tile, s.id, s.path, s.reversed);
                    if last == Some(key) {
                        continue;
                    }
                    last = Some(key);
                    let ti = tile_index(s.tile).ok_or_else(|| format!("line {number}: tile {},{} is not in the map's global.cfg", s.tile[0], s.tile[1]))?;
                    entries.push(TrackEntry { values: vec![s.id as f64, s.path as f64, ti as f64, 0.0, s.length as f64, 0.0] });
                }
            }
            out.files.push((format!("{name}.ttr"), Track { path: PathBuf::new(), entries }.to_text()));
            // the links the map lacks, and the stops it does not name
            for (k, g) in legs.iter().enumerate() {
                let (a, b) = (&d.stops[k], &d.stops[k + 1]);
                if !links_done.insert((a.id, b.id)) {
                    continue;
                }
                let mut entries = Vec::new();
                for s in &g.steps {
                    let ti = tile_index(s.tile).ok_or_else(|| format!("line {number}: tile {},{} is not in the map's global.cfg", s.tile[0], s.tile[1]))?;
                    entries.push(StnLinkEntry { values: [s.id as f64, s.path as f64, ti as f64, s.length as f64, -1.0, 0.0, 0.0] });
                }
                let last = entries.len().saturating_sub(1) as f64;
                let link = StnLink { length: g.length as f64, from_id: a.id, to_id: b.id, params: [g.from_lat as f64, g.to_lat as f64, g.from_s as f64, g.to_s as f64, 0.0, last], entries };
                out.links.push((link, a.name.clone(), b.name.clone()));
            }
            for s in &d.stops {
                if stops_done.insert(s.id) {
                    let group = tile_index(s.tile).unwrap_or(0) as i32;
                    out.stops.push(BusStopEntry { name: s.name.clone(), group, object_id: s.id, params: [0.0, 0.0, 0.0] });
                }
            }
        }
        // the tours: the buses of every day pattern, numbered on through the day groups
        let mut tours = Vec::new();
        for p in &l.days {
            for b in blocks(&run, p) {
                tours.push(Tour {
                    number: (tours.len() + 1).to_string(),
                    ai_group: l.ai_group.trim().to_string(),
                    extra: mask_of(p.days).to_string(),
                    trips: b.iter().map(|t| TourTrip { trip: trip_name(stem, t.dir), profile: 0, departure: t.departure }).collect(),
                });
            }
        }
        out.tours.insert(l.id, tours.len());
        let line = Line { path: PathBuf::new(), name: stem.clone(), user_allowed: true, priority: 1, tours };
        out.files.push((format!("{stem}.ttl"), line.to_text()));
    }
    Ok(out)
}

/// The text of a `TTData` file as the map has it, without what the line editor added.
fn map_part(dir: &Path, file: &str) -> String {
    let b = std::fs::read(dir.join(file)).unwrap_or_default();
    let text = omsi_cfg::codepage::detect(&b).encoding().decode(&b).0.into_owned();
    omsi_timetable::write::replace_block(&text, "")
}

/// The pairs of stops the map's own `StnLinks.cfg` in `dir` links, and the stops its
/// `Busstops.cfg` names (what the line editor added left out).
pub fn map_has(dir: &Path) -> (HashSet<(i64, i64)>, HashSet<i64>) {
    let links = omsi_timetable::parse_stnlinks(&omsi_cfg::CfgFile::from_str(dir.join("StnLinks.cfg"), &map_part(dir, "StnLinks.cfg")));
    let stops = omsi_timetable::parse_busstops(&omsi_cfg::CfgFile::from_str(dir.join("Busstops.cfg"), &map_part(dir, "Busstops.cfg")));
    (links.iter().map(|l| (l.from_id, l.to_id)).collect(), stops.iter().map(|s| s.object_id).collect())
}

/// Write `e` into the `TTData` folder `dir`: the files written last time go first (see
/// `MANIFEST`), then the new ones, and the blocks of `StnLinks.cfg` and `Busstops.cfg`.
/// `keep` is called for a file of the map's before it is changed (its `.orig`).
pub fn write_export(dir: &Path, e: &Export, keep: &dyn Fn(&Path) -> Result<(), String>) -> Result<usize, String> {
    let manifest = dir.join(MANIFEST);
    for name in std::fs::read_to_string(&manifest).unwrap_or_default().lines().map(str::trim).filter(|n| !n.is_empty()) {
        // (only what the editor writes: a manifest edited by hand deletes nothing else)
        if name.starts_with(FILE_PREFIX) && !name.contains(['/', '\\']) {
            let _ = std::fs::remove_file(dir.join(name));
        }
    }
    let mut written = Vec::new();
    for (name, text) in &e.files {
        omsi_timetable::write::write_text(&dir.join(name), text).map_err(|x| format!("{name}: {x}"))?;
        written.push(name.clone());
    }
    let links: String = e.links.iter().map(|(l, a, b)| l.to_text(a, b)).collect();
    let stops: String = e.stops.iter().map(|s| s.to_text()).collect();
    for (file, block) in [("StnLinks.cfg", links), ("Busstops.cfg", stops)] {
        let p = dir.join(file);
        if p.is_file() || !block.is_empty() {
            keep(&p)?;
        }
        omsi_timetable::write::update_block(&p, &block).map_err(|x| format!("{file}: {x}"))?;
    }
    std::fs::write(&manifest, written.join("\r\n")).map_err(|x| format!("{MANIFEST}: {x}"))?;
    Ok(written.len())
}

/// Every line of the registry that can be saved, written into the map's `TTData` (the
/// content folder's copy, see `ttstore`). Returns how many lines and files.
pub fn export_to_map(content: &Path, map_dir: &Path, reg: &Registry) -> Result<(usize, usize), String> {
    let folder = map_dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = crate::ttstore::ttdata_dir(content, map_dir, &folder)?;
    let raw_tiles = omsi_map::GlobalCfg::load(&map_dir.join("global.cfg")).map(|g| g.raw_tiles).map_err(|e| format!("global.cfg: {e}"))?;
    let (links, stops) = map_has(&dir);
    let e = export(reg, &raw_tiles, &links, &stops)?;
    let files = write_export(&dir, &e, &crate::ttstore::keep_original)?;
    Ok((e.tours.len(), files))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: i64, name: &str, x: f64) -> StopRef {
        StopRef { tile: [0, 0], id, name: name.into(), at: [x, 0.0], ..Default::default() }
    }

    fn leg(len: f32, ids: &[i64]) -> Leg {
        Leg { steps: ids.iter().map(|i| LaneStep { tile: [0, 0], id: *i, path: 0, reversed: false, length: len / ids.len() as f32 }).collect(), length: len, ok: true, from_s: 2.0, to_s: 5.0, from_lat: 2.5, to_lat: 2.5, vias: Vec::new() }
    }

    /// A line of three stops out and three back over five lanes, every day.
    fn line(reg: &mut Registry) -> u64 {
        let l = reg.add_line("Busses");
        l.number = "42".into();
        l.name = "Ring".into();
        let out = Direction { terminus: "Markt".into(), stops: vec![stop(1, "Bahnhof", 0.0), stop(2, "Kirche", 500.0), stop(3, "Markt", 900.0)], legs: vec![leg(500.0, &[10, 11]), leg(400.0, &[11, 12])], ..Default::default() };
        let back = Direction { stops: vec![stop(4, "Markt", 900.0), stop(5, "Kirche", 500.0), stop(6, "Bahnhof", 0.0)], legs: vec![leg(400.0, &[13]), leg(500.0, &[14])], ..Default::default() };
        l.directions = vec![out, back];
        for d in &mut l.directions {
            d.refresh_times();
        }
        l.id
    }

    #[test]
    fn the_times_follow_the_legs_and_can_be_set_by_hand() {
        let legs = vec![leg(500.0, &[1]), leg(400.0, &[2]), leg(10.0, &[3])];
        // 80.2 s, 68.2 s, 21.2 s: 1, 2.5 (rounded 2), 2.8 (3)
        assert_eq!(auto_times(&legs), vec![0.0, 1.0, 2.0, 3.0]);
        let mut d = Direction { stops: vec![stop(1, "a", 0.0), stop(2, "b", 0.0), stop(3, "c", 0.0), stop(4, "d", 0.0)], legs, ..Default::default() };
        d.refresh_times();
        d.set_total(6.0);
        assert_eq!(d.times, vec![0.0, 2.0, 4.0, 6.0]);
        d.shift_from(2, -5.0);
        // (never before the stop ahead)
        assert_eq!(d.times, vec![0.0, 2.0, 2.0, 4.0]);
        assert!(d.manual_times);
        // set by hand, they stay when the legs change
        d.refresh_times();
        assert_eq!(d.times, vec![0.0, 2.0, 2.0, 4.0]);
    }

    #[test]
    fn the_buses_go_back_and_forth() {
        // 15 minutes each way, 5 at the end: a round takes 40, so every 20 minutes needs two
        // buses - one starting at each end, each taking the other's departures from there on
        let p = DayPattern { on: true, days: 31, first: 360.0, last: 480.0, headway: 20.0, layover: 5.0 };
        let b = blocks(&[15.0, 15.0], &p);
        let trips: usize = b.iter().map(|x| x.len()).sum();
        assert_eq!(trips, 14);
        assert_eq!(b.len(), 2);
        // 10 minutes of layover: the 6:20 at each end finds no bus back yet, four buses
        assert_eq!(blocks(&[15.0, 15.0], &DayPattern { layover: 10.0, ..p.clone() }).len(), 4);
        for bus in &b {
            // a bus alternates the directions, and is never late for its next trip
            for w in bus.windows(2) {
                assert_ne!(w[0].dir, w[1].dir);
                assert!(w[1].departure >= w[0].departure + 20.0 - 1e-3);
            }
        }
        // one direction: the buses go round
        let round = blocks(&[25.0], &DayPattern { headway: 10.0, ..p.clone() });
        assert_eq!(round.len(), 3);
        assert!(round.iter().flatten().all(|t| t.dir == 0));
        // a pattern that is off has no buses
        assert!(blocks(&[15.0, 15.0], &DayPattern { on: false, ..p }).is_empty());
    }

    #[test]
    fn the_way_back_takes_the_stops_across_the_road() {
        let all = vec![stop(1, "Bahnhof", 0.0), stop(11, "Bahnhof", 12.0), stop(2, "Kirche", 500.0), stop(3, "Markt", 900.0), stop(31, "Markt", 2000.0)];
        let out = vec![all[0].clone(), all[2].clone(), all[3].clone()];
        let back: Vec<i64> = opposite_stops(&out, &all, 150.0).iter().map(|s| s.id).collect();
        // Markt across the road is too far: the same stop; Kirche has none; Bahnhof has one
        assert_eq!(back, vec![3, 2, 11]);
    }

    #[test]
    fn a_line_is_written_and_reads_back() {
        let mut reg = Registry::default();
        let id = line(&mut reg);
        // the map links 1 -> 2 already and names stop 1
        let map_links: HashSet<(i64, i64)> = [(1, 2)].into_iter().collect();
        let map_stops: HashSet<i64> = [1].into_iter().collect();
        let e = export(&reg, &[(5, 5), (0, 0)], &map_links, &map_stops).unwrap();
        let base = std::env::temp_dir().join(format!("omsi_lines_export_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("TTData");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Busstops.cfg"), "[busstop]\r\nBahnhof\r\n1\r\n1\r\n0.0\r\n0\r\n0\r\n").unwrap();
        assert_eq!(write_export(&dir, &e, &|_| Ok(())).unwrap(), 5);
        let data = omsi_timetable::TimetableData::load(&base);
        let l = data.lines.iter().find(|l| l.name == "oo_42").expect("the line");
        assert!(l.user_allowed);
        assert_eq!(l.tours.len(), e.tours[&id]);
        assert!(l.tours.iter().all(|t| t.ai_group == "Busses"));
        // the days: working days, Saturdays, Sundays and holidays - with both school bits
        let masks: HashSet<String> = l.tours.iter().map(|t| t.extra.clone()).collect();
        assert_eq!(masks, ["799", "800", "960"].into_iter().map(String::from).collect());
        let a = data.trip("oo_42_a").expect("the outbound trip");
        assert_eq!((a.display_name.as_str(), a.terminus.as_str(), a.line.as_str()), ("oo_42_a", "Markt", "42"));
        assert_eq!(a.stations, vec![1, 2, 3]);
        assert_eq!(a.profiles[0].man_dep_time, vec![(1, 1.0)]);
        // the way back shows its last stop
        assert_eq!(data.trip("oo_42_b").unwrap().terminus, "Bahnhof");
        // the track: lane 11 once where the two legs meet; the tile is its place in the list
        let track = data.tracks.iter().find(|t| t.path.file_stem().unwrap() == "oo_42_a").unwrap();
        assert_eq!(track.entries.iter().map(|x| x.values[0] as i64).collect::<Vec<_>>(), vec![10, 11, 12]);
        assert!(track.entries.iter().all(|x| x.values[2] == 1.0));
        // links: not 1 -> 2 (the map's), the three others; stops: all but 1
        let pairs: HashSet<(i64, i64)> = data.stn_links.iter().map(|l| (l.from_id, l.to_id)).collect();
        assert_eq!(pairs, [(2, 3), (4, 5), (5, 6)].into_iter().collect());
        assert_eq!(data.bus_stops.len(), 6);
        // written again with the line gone: its files and its block are gone, the map's stop stays
        reg.lines.clear();
        let e = export(&reg, &[(5, 5), (0, 0)], &map_links, &map_stops).unwrap();
        write_export(&dir, &e, &|_| Ok(())).unwrap();
        let data = omsi_timetable::TimetableData::load(&base);
        assert!(data.lines.is_empty() && data.trips.is_empty() && data.tracks.is_empty() && data.stn_links.is_empty());
        assert_eq!(data.bus_stops.iter().map(|s| s.object_id).collect::<Vec<_>>(), vec![1]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_lists_know_the_players_lines_by_their_files() {
        let mut reg = Registry::default();
        let id = line(&mut reg);
        // a draft (no stops yet) has no file, so it is no line to drive
        reg.add_line("Busses").number = "7".into();
        let own = own_lines(&reg);
        assert_eq!(own.len(), 1);
        let o = &own[0];
        assert_eq!((o.id, o.file.as_str(), o.number.as_str(), o.name.as_str()), (id, "oo_42", "42", "Ring"));
        // the outbound goes to its terminus, the way back to its last stop
        assert_eq!(o.destinations, vec!["Markt".to_string(), "Bahnhof".to_string()]);
        assert_eq!(o.caption(), "Ring · Markt – Bahnhof");
        // the timetable's line of that file, in any case; a line of the map's is none
        assert_eq!(own_line_of("OO_42", &own).map(|x| x.id), Some(id));
        assert_eq!(own_line_of("Montag - Freitag", &own), None);
        assert_eq!(own_line_of("Zoo_3", &own), None);
        // a file of the editor's without its registry: the number from the file name
        let orphan = own_line_of("oo_9", &[]).unwrap();
        assert_eq!((orphan.id, orphan.number.as_str(), orphan.caption().as_str()), (0, "9", ""));
        assert!(is_own_file(" oo_1") && !is_own_file("o") && !is_own_file("Hoo_1"));
        // a line of one direction that goes round names its end once
        let mut round = reg.clone();
        let l = round.line_mut(id).unwrap();
        l.directions.truncate(1);
        l.directions[0].terminus.clear();
        assert_eq!(own_lines(&round)[0].destinations, vec!["Markt".to_string()]);
    }

    #[test]
    fn a_line_with_a_gap_is_not_written() {
        let mut reg = Registry::default();
        let id = line(&mut reg);
        reg.line_mut(id).unwrap().directions[1].legs[0].ok = false;
        assert_eq!(problems(reg.line(id).unwrap()).len(), 1);
        let e = export(&reg, &[(0, 0)], &HashSet::new(), &HashSet::new()).unwrap();
        assert!(e.files.is_empty());
    }

    #[test]
    fn the_registry_keeps_its_ids() {
        let path = std::env::temp_dir().join(format!("omsi_lines_reg_{}", std::process::id())).join("Dorf.json");
        let mut reg = Registry { map: "Dorf".into(), ..Default::default() };
        let a = line(&mut reg);
        let b = reg.add_line("Busses").id;
        assert_ne!(a, b);
        assert_eq!(reg.line(b).unwrap().number, "43");
        save_registry(&path, &reg).unwrap();
        let back = load_registry(&path);
        assert_eq!(back, reg);
        // the same number twice: the files tell them apart by the id
        let mut two = back.clone();
        two.line_mut(b).unwrap().number = "42".into();
        let s = stems(&two);
        assert_eq!((s[&a].as_str(), s[&b].as_str()), ("oo_42_1", "oo_42_2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        // (a file of another version still reads)
        let old: Registry = serde_json::from_str(r#"{"version":1,"map":"X","lines":[{"id":7,"number":"1"}]}"#).unwrap();
        assert_eq!(old.lines[0].days.len(), 3);
    }
}

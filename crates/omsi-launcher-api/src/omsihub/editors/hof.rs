//! The bus's depot files on the bus step (`crates/omsi-launcher-ui/src/hof.rs`): which of them
//! the bus has and what each is for - the map it belongs to, how many destinations it knows,
//! whether it is the chosen map's - with the one in use marked and the one for the chosen map
//! first, and the map's depot file added when the bus lacks it (`core::depot`, into openOMSI's
//! own content folder).
//!
//! What the game does with a bus that lacks the file is said as it is: `situation::find_hof`
//! takes the bus's own depot file of the same place first, then borrows the map's from another
//! vehicle folder, then the bus's first. The analysis follows that order, so the tile marked
//! "in use" is the file the drive will have.
//!
//! `hub_depots` works the menu out (on the caller's worker; the map's copies elsewhere are
//! looked for once per content generation) and says which file the drive takes with the depot
//! file chosen; `hub_depot_add` gives the bus the map's.

use super::{arg, global_of};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::depot::{Refusal, Source};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "hub_depots" => (|| depots(args))(),
        "hub_depot_add" => (|| add(&arg::<String>(args, "bus")?, &arg::<String>(args, "want")?, &arg::<String>(args, "source")?))(),
        _ => return None,
    })
}

// --- what a bus's depot files are for -------------------------------------------------------

/// One depot file of the bus, as its tile shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub file: String,
    /// Its `[name]`, or its file name without `.hof` when it has none.
    pub name: String,
    /// What the game is given (`--hof`) to take this one.
    pub key: String,
    pub destinations: usize,
    /// How many of the chosen map's destinations it knows (None: nothing to compare with).
    pub known: Option<usize>,
    /// The map it belongs to, as far as can be told.
    pub map: Option<String>,
    /// It is the chosen map's depot file.
    pub maps: bool,
    /// openOMSI added it (`depot::added`).
    pub added: bool,
}

/// What a drive takes when the bus is given the map's depot file and lacks it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Fallback {
    Own(PathBuf),
    Borrowed(String),
    #[default]
    Nothing,
}

/// The depot file a drive with the bus has.
#[derive(Clone, Debug, PartialEq)]
pub enum Use {
    Row(usize),
    Borrowed(String),
    Nothing,
}

/// How a depot file stands to the chosen map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    Maps,
    Fits,
    Partly(usize, usize),
    Other,
    Unknown,
}

/// The bus's depot files, worked out for a bus on a map.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    pub key: String,
    pub bus: String,
    pub rows: Vec<Row>,
    pub want: String,
    pub map: String,
    pub total: Option<usize>,
    pub source: Option<Source>,
    pub source_name: String,
    pub source_destinations: usize,
    pub refusal: Option<Refusal>,
    pub target: Option<PathBuf>,
    pub fallback: Fallback,
    pub recommended: Option<usize>,
    pub own: Vec<core::owndepot::Entry>,
}

impl Menu {
    pub fn lacks(&self) -> bool {
        !self.want.trim().is_empty() && !self.rows.iter().any(|r| r.maps)
    }

    pub fn used(&self, chosen: &str) -> Use {
        if let Some(i) = row_of(&self.rows, chosen) {
            return Use::Row(i);
        }
        match &self.fallback {
            Fallback::Own(p) => self.rows.iter().position(|r| &r.path == p).map(Use::Row).unwrap_or(Use::Nothing),
            Fallback::Borrowed(f) => Use::Borrowed(f.clone()),
            Fallback::Nothing => Use::Nothing,
        }
    }
}

/// A name to compare: lower case, letters and digits only, the accents off (`buspick::fold`).
pub fn fold(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ß' => out.push_str("ss"),
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// Names in a natural order: case aside, a run of digits by its number (`buspick::name_cmp`).
pub fn name_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let o = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A depot file's destinations to compare: its termini but the "everybody out" ones, folded.
pub fn destinations(h: &omsi_vehicle::Hof) -> HashSet<String> {
    h.termini.iter().filter(|t| !t.all_exit).map(|t| fold(&t.texture_id)).filter(|s| !s.is_empty()).collect()
}

/// What the game is given (`--hof`) for each file: its name, unless another has the same one -
/// then its file name.
pub fn keys_for(names: &[String], stems: &[String]) -> Vec<String> {
    names.iter().zip(stems).map(|(n, s)| if names.iter().filter(|o| o.trim().eq_ignore_ascii_case(n.trim())).count() > 1 { s.clone() } else { n.clone() }).collect()
}

/// The row of the depot file `chosen`: by what the game is given, its file name or its name.
pub fn row_of(rows: &[Row], chosen: &str) -> Option<usize> {
    let c = chosen.trim();
    if c.is_empty() {
        return None;
    }
    let stem = |r: &Row| Path::new(&r.file).file_stem().map(|s| s.to_string_lossy().trim().to_string()).unwrap_or_default();
    rows.iter().position(|r| r.key.trim().eq_ignore_ascii_case(c)).or_else(|| rows.iter().position(|r| stem(r).eq_ignore_ascii_case(c))).or_else(|| rows.iter().position(|r| r.name.trim().eq_ignore_ascii_case(c)))
}

pub fn fit_of(row: &Row, total: Option<usize>) -> Fit {
    if row.maps {
        return Fit::Maps;
    }
    match (row.known, total) {
        (Some(k), Some(t)) if t > 0 && k * 10 >= t * 9 => Fit::Fits,
        (Some(k), Some(t)) if k > 0 => Fit::Partly(k, t),
        (Some(_), Some(t)) if t > 0 => Fit::Other,
        _ => Fit::Unknown,
    }
}

/// The map a depot file belongs to: a map whose ailists name it, else the map whose name shares
/// the most words of a place with it.
pub fn map_of(name: &str, stem: &str, maps: &[(String, String)]) -> Option<String> {
    let says = |h: &str| !h.trim().is_empty() && (h.trim().eq_ignore_ascii_case(name.trim()) || h.trim().eq_ignore_ascii_case(stem.trim()));
    if let Some((label, _)) = maps.iter().find(|(_, h)| says(h)) {
        return Some(label.clone());
    }
    let labels: Vec<&str> = maps.iter().map(|(l, _)| l.as_str()).collect();
    omsi_vehicle::hof::closest_name(&labels, &[name, stem]).map(|i| maps[i].0.clone())
}

fn like(rows: &[Row], hints: &[&str]) -> Option<usize> {
    let labels: Vec<String> = rows.iter().map(|r| format!("{} {}", Path::new(&r.file).file_stem().unwrap_or_default().to_string_lossy(), r.name)).collect();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    omsi_vehicle::hof::closest_name(&refs, hints)
}

pub fn fallback_of(rows: &[Row], want: &str, map_hints: &[&str], borrowed: Option<String>) -> Fallback {
    if let Some(i) = like(rows, &[want]) {
        return Fallback::Own(rows[i].path.clone());
    }
    if let Some(f) = borrowed {
        return Fallback::Borrowed(f);
    }
    if let Some(i) = like(rows, map_hints) {
        return Fallback::Own(rows[i].path.clone());
    }
    rows.first().map(|r| Fallback::Own(r.path.clone())).unwrap_or_default()
}

pub fn recommend(rows: &[Row], like: Option<usize>, total: Option<usize>) -> Option<usize> {
    if let Some(i) = rows.iter().position(|r| r.maps) {
        return Some(i);
    }
    let enough = |k: usize| k > 0 && total.is_none_or(|t| k * 4 >= t);
    let best = rows.iter().enumerate().filter(|(_, r)| r.known.is_some_and(enough)).max_by(|(a, x), (b, y)| x.known.cmp(&y.known).then(b.cmp(a)));
    best.map(|(i, _)| i).or(like)
}

pub fn order(mut rows: Vec<Row>, first: Option<usize>) -> Vec<Row> {
    let head = first.filter(|i| *i < rows.len()).map(|i| rows.remove(i));
    rows.sort_by(|a, b| b.maps.cmp(&a.maps).then(b.known.unwrap_or(0).cmp(&a.known.unwrap_or(0))).then_with(|| name_cmp(&a.name, &b.name)));
    head.into_iter().chain(rows).collect()
}

/// The bus's own depot files the dialog offers instead of the map's: those that know a quarter
/// of its destinations at least, the best first, three at most.
pub fn choices(m: &Menu) -> Vec<usize> {
    let enough = |k: usize| k > 0 && m.total.is_none_or(|t| k * 4 >= t);
    let mut v: Vec<usize> = (0..m.rows.len()).filter(|i| m.rows[*i].known.is_some_and(enough)).collect();
    v.sort_by(|a, b| m.rows[*b].known.cmp(&m.rows[*a].known).then(a.cmp(b)));
    if v.is_empty() && m.total.is_none() {
        v.extend(m.recommended);
    }
    v.truncate(3);
    v
}

/// A place to show in a line: from the content folder's own name on.
pub fn short_place(p: &Path) -> String {
    let parts: Vec<String> = p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    match parts.iter().rposition(|c| c.eq_ignore_ascii_case("Vehicles")) {
        Some(i) if i > 0 => parts[i - 1..].join(std::path::MAIN_SEPARATOR_STR),
        _ => p.display().to_string(),
    }
}

// --- working it out -------------------------------------------------------------------------

struct Job {
    key: String,
    root: PathBuf,
    bus: String,
    map: String,
    want: String,
    map_label: String,
    map_hints: Vec<String>,
    maps: Vec<(String, String)>,
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().replace('\\', "/").eq_ignore_ascii_case(&b.to_string_lossy().replace('\\', "/"))
}

/// The map's copies found on this computer, by depot file, map and content generation.
fn sources_cache() -> &'static Mutex<HashMap<String, Arc<Vec<Source>>>> {
    static C: std::sync::OnceLock<Mutex<HashMap<String, Arc<Vec<Source>>>>> = std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

/// The menus worked out, by bus|map|depot file|content generation (the last few).
fn menus() -> &'static Mutex<Vec<Arc<Menu>>> {
    static M: std::sync::OnceLock<Mutex<Vec<Arc<Menu>>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

fn analyse(j: Job) -> Menu {
    let cache = sources_cache();
    let bus_path = omsi_cfg::resolve_path(&j.root, &j.bus);
    let dir = bus_path.parent().map(Path::to_path_buf).unwrap_or_else(|| j.root.clone());
    let pack = j.bus.replace('\\', "/").split('/').take(2).collect::<Vec<_>>().join("/");
    let theirs = |d: &Path| omsi_vehicle::hof::depot_files(d).into_iter().filter(|f| !omsi_vehicle::hof::is_players(f)).collect::<Vec<_>>();
    let mut files = theirs(&dir);
    if files.is_empty() && !pack.is_empty() {
        files = theirs(&omsi_cfg::resolve_path(&j.root, &pack));
    }
    let added = core::depot::added();
    let hofs: Vec<omsi_vehicle::Hof> = files.iter().map(|f| omsi_vehicle::Hof::load(f).unwrap_or_else(|_| omsi_vehicle::Hof { path: f.clone(), ..Default::default() })).collect();
    let stems: Vec<String> = files.iter().map(|f| f.file_stem().unwrap_or_default().to_string_lossy().trim().to_string()).collect();
    let names: Vec<String> = hofs.iter().zip(&stems).map(|(h, s)| if h.name.trim().is_empty() { s.clone() } else { h.name.trim().to_string() }).collect();
    let keys = keys_for(&names, &stems);
    let dests: Vec<HashSet<String>> = hofs.iter().map(destinations).collect();
    let maps_flags: Vec<bool> = files.iter().map(|f| core::depot::answers_to(f, &j.want)).collect();
    let own = maps_flags.iter().position(|m| *m);
    let sources: Arc<Vec<Source>> = if j.want.trim().is_empty() {
        Arc::default()
    } else {
        let k = format!("{}|{}|{}", j.want.trim().to_lowercase(), j.map.to_lowercase(), omsi_cfg::content_generation());
        let hit = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&k).cloned();
        hit.unwrap_or_else(|| {
            let s = Arc::new(core::depot::find_sources(&j.want, &j.map));
            cache.lock().unwrap_or_else(|e| e.into_inner()).insert(k, s.clone());
            s
        })
    };
    let others: Vec<Source> = sources.iter().filter(|s| !files.iter().any(|f| same(f, &s.path))).cloned().collect();
    let source = core::depot::best(&others).map(|i| others[i].clone());
    let source_hof = source.as_ref().and_then(|s| omsi_vehicle::Hof::load(&s.path).ok());
    let reference: Option<HashSet<String>> = own.map(|i| dests[i].clone()).or_else(|| source_hof.as_ref().map(destinations));
    let rows: Vec<Row> = (0..files.len())
        .map(|i| Row {
            path: files[i].clone(),
            file: files[i].file_name().unwrap_or_default().to_string_lossy().into_owned(),
            name: names[i].clone(),
            key: keys[i].clone(),
            destinations: dests[i].len(),
            known: reference.as_ref().map(|r| dests[i].intersection(r).count()),
            map: if maps_flags[i] { Some(j.map_label.clone()).filter(|m| !m.is_empty()) } else { map_of(&names[i], &stems[i], &j.maps) },
            maps: maps_flags[i],
            added: added.iter().any(|a| same(a, &files[i])),
        })
        .collect();
    let lacks = !j.want.trim().is_empty() && own.is_none();
    let hints: Vec<&str> = j.map_hints.iter().map(String::as_str).filter(|h| !h.trim().is_empty()).collect();
    let borrowed = if lacks {
        let mut v: Vec<&Source> = others.iter().filter(|s| !s.from_map).collect();
        v.sort_by_key(|s| s.folder.to_lowercase());
        v.first().map(|s| s.folder.clone())
    } else {
        None
    };
    let fallback = if lacks { fallback_of(&rows, &j.want, &hints, borrowed) } else { Fallback::Nothing };
    let same_place = if j.want.trim().is_empty() { like(&rows, &hints) } else { like(&rows, &[j.want.as_str()]).or_else(|| like(&rows, &hints)) };
    let first = recommend(&rows, same_place, reference.as_ref().map(|r| r.len()));
    let rows = order(rows, first);
    let target = core::depot::target_for(&j.bus);
    let existing: Vec<(String, String)> = rows.iter().map(|r| (r.file.clone(), r.name.clone())).collect();
    let refusal = if j.want.trim().is_empty() {
        None
    } else if let Some(r) = rows.iter().find(|r| r.maps) {
        Some(Refusal::Has(r.file.clone()))
    } else {
        match &source {
            None => Some(Refusal::NoSource),
            Some(_) if target.is_none() => Some(Refusal::NoFolder),
            Some(s) => core::depot::check(&s.file, &j.want, &existing).err(),
        }
    };
    let source_name = source.as_ref().and_then(|s| omsi_vehicle::Hof::read_name(&s.path)).unwrap_or_default().trim().to_string();
    Menu {
        key: j.key,
        bus: j.bus,
        total: reference.as_ref().map(|r| r.len()),
        source_destinations: source_hof.as_ref().map(|h| destinations(h).len()).unwrap_or(0),
        source,
        source_name,
        refusal,
        target,
        fallback,
        recommended: first.map(|_| 0),
        own: core::owndepot::for_map(&core::owndepot::list(&core::owndepot::dir()), &core::lines::map_folder(&j.map)),
        want: j.want,
        map: j.map_label,
        rows,
    }
}

/// The chosen map's depot file on the date: the one its own buses use then, else the one its
/// ailists name (`State::map_hof`).
fn map_hof(map: &str, date: &str, listed: &str) -> String {
    let global = global_of(map);
    let on_date = global.parent().and_then(|dir| omsi_map::ailists::depot_hof_on(dir, omsi_map::ailists::date_code(date)?));
    on_date.unwrap_or_else(|| listed.to_string())
}

/// The player's own depot file (its place among `m.own`) the drive gives the bus
/// (`State::own_depot_in_use`): the one chosen, else the one the driven line of his chose.
fn own_in_use(m: &Menu, chosen: &str, line: Option<&str>, map: &str) -> Option<(usize, bool)> {
    if let Some(i) = m.own.iter().position(|e| e.key.eq_ignore_ascii_case(chosen.trim())) {
        return Some((i, false));
    }
    let line = line?;
    let own = core::lines::own_lines_of_map(map);
    let o = own.iter().find(|o| o.file.eq_ignore_ascii_case(line.trim()))?;
    m.own.iter().position(|e| !o.depot_file.trim().is_empty() && e.key.eq_ignore_ascii_case(o.depot_file.trim())).map(|i| (i, true))
}

fn refusal_json(r: &Refusal) -> Value {
    let (kind, what) = match r {
        Refusal::Has(f) => ("has", f.clone()),
        Refusal::Taken(f) => ("taken", f.clone()),
        Refusal::NoSource => ("no_source", String::new()),
        Refusal::NoFolder => ("no_folder", String::new()),
        Refusal::Original => ("original", String::new()),
        Refusal::Unreadable(w) => ("unreadable", w.clone()),
        Refusal::Failed(w) => ("failed", w.clone()),
    };
    json!({ "kind": kind, "what": what })
}

fn fit_json(f: Fit) -> Value {
    match f {
        Fit::Maps => json!({ "kind": "maps" }),
        Fit::Fits => json!({ "kind": "fits" }),
        Fit::Partly(k, t) => json!({ "kind": "partly", "known": k, "total": t }),
        Fit::Other => json!({ "kind": "other" }),
        Fit::Unknown => json!({ "kind": "unknown" }),
    }
}

/// The bus's depot files for a drive: `bus`, the map `map` (`file`, its name `label`, its
/// `hof`, the words naming its place `hints`), the `date`, the depot file `chosen`, the line
/// driven (`line`), every map's name and depot file (`maps`). Answers the menu with what the
/// drive takes (`used`: `{ row }`, `{ borrowed }` or `{}`; `own`: the player's own file in use
/// and whether the line chose it), each row's fit to the map, and the dialog's choices.
fn depots(args: &Value) -> Answer {
    let bus = arg::<String>(args, "bus")?;
    let map: Value = arg(args, "map")?;
    let s = |k: &str| map.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let file = s("file");
    let date = arg::<Option<String>>(args, "date")?.unwrap_or_default();
    let chosen = arg::<Option<String>>(args, "chosen")?.unwrap_or_default();
    let line = arg::<Option<String>>(args, "line")?;
    let maps: Vec<(String, String)> = arg::<Option<Vec<(String, String)>>>(args, "maps")?.unwrap_or_default();
    let want = map_hof(&file, &date, &s("hof"));
    let generation = omsi_cfg::content_generation();
    let key = format!("{bus}|{file}|{want}|{generation}");
    let have = menus().lock().unwrap_or_else(|e| e.into_inner()).iter().find(|m| m.key == key).cloned();
    let m = match have {
        Some(m) => m,
        None => {
            let label = if s("friendly").trim().is_empty() { s("name").trim().to_string() } else { s("friendly").trim().to_string() };
            let hints = vec![s("name"), s("friendly"), file.trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or("").to_string()];
            let job = Job { key: key.clone(), root: core::root_dir().unwrap_or_default(), bus: bus.clone(), map: file.clone(), want: want.clone(), map_label: label, map_hints: hints, maps };
            // (an odd file of some mod that panics the reading: the tiles say there is nothing)
            let m = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| analyse(job))).unwrap_or_else(|_| Menu { key: key.clone(), ..Default::default() });
            let m = Arc::new(m);
            let mut all = menus().lock().unwrap_or_else(|e| e.into_inner());
            all.retain(|x| x.key != key);
            all.push(m.clone());
            let n = all.len();
            if n > 8 {
                all.drain(..n - 8);
            }
            m
        }
    };
    let used = match m.used(&chosen) {
        Use::Row(i) => json!({ "row": i }),
        Use::Borrowed(f) => json!({ "borrowed": f }),
        Use::Nothing => json!({}),
    };
    let own = own_in_use(&m, &chosen, line.as_deref(), &file);
    let rows: Vec<Value> = m
        .rows
        .iter()
        .map(|r| json!({ "path": r.path.to_string_lossy(), "file": r.file, "name": r.name, "key": r.key, "destinations": r.destinations, "known": r.known, "map": r.map, "maps": r.maps, "added": r.added, "fit": fit_json(fit_of(r, m.total)) }))
        .collect();
    let own_files: Vec<Value> = m.own.iter().map(|e| json!({ "key": e.key, "name": e.name, "termini": e.termini.len(), "specials": e.specials })).collect();
    let source = m.source.as_ref().map(|s| json!({ "path": s.path.to_string_lossy(), "file": s.file, "folder": s.folder, "from_map": s.from_map }));
    Ok(json!({
        "key": m.key, "bus": m.bus, "want": m.want, "map": m.map, "total": m.total, "rows": rows,
        "source": source, "source_name": m.source_name, "source_destinations": m.source_destinations,
        "refusal": m.refusal.as_ref().map(refusal_json), "place": m.target.as_deref().map(short_place),
        "recommended": m.recommended, "own": own_files, "lacks": m.lacks(),
        "used": used, "own_in_use": own.map(|(i, by_line)| json!({ "index": i, "by_line": by_line })),
        "choices": choices(&m),
    }))
}

/// Give the bus the map's depot file `want` from the copy `source` (`depot::add_for_bus`):
/// `{ added: path }`, or `{ refusal }` - why not.
fn add(bus: &str, want: &str, source: &str) -> Answer {
    match core::depot::add_for_bus(bus, want, Path::new(source)) {
        Ok(p) => {
            omsi_cfg::content_changed();
            menus().lock().unwrap_or_else(|e| e.into_inner()).clear();
            Ok(json!({ "added": p.to_string_lossy() }))
        }
        Err(e) => Ok(json!({ "refusal": refusal_json(&e) })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(file: &str, name: &str, known: Option<usize>, maps: bool) -> Row {
        Row { path: PathBuf::from(format!("Vehicles/Bus/{file}")), file: file.into(), name: name.into(), key: name.into(), destinations: 10, known, map: None, maps, added: false }
    }

    fn menu(rows: Vec<Row>, want: &str, total: Option<usize>) -> Menu {
        Menu { key: "k".into(), bus: "Vehicles/Bus/bus.bus".into(), rows, want: want.into(), map: "Hamburg".into(), total, ..Default::default() }
    }

    #[test]
    fn the_depot_files_are_worked_out_as_the_game_drawn_launcher_does() {
        // (the same cases as `hof.rs`'s own tests)
        let names = vec!["Hamburg".to_string(), "hamburg".to_string(), "Grundorf".to_string()];
        let stems = vec!["HH_2014".to_string(), "HH_2022".to_string(), "Grundorf".to_string()];
        assert_eq!(keys_for(&names, &stems), vec!["HH_2014", "HH_2022", "Grundorf"]);
        assert_eq!(fit_of(&row("a.hof", "A", Some(37), false), Some(40)), Fit::Fits);
        assert_eq!(fit_of(&row("a.hof", "A", Some(12), false), Some(40)), Fit::Partly(12, 40));
        let rows = vec![row("A.hof", "Alpha", Some(2), false), row("B.hof", "Bravo", Some(9), false), row("C.hof", "Charlie", Some(40), true), row("D.hof", "Delta", Some(0), false)];
        assert_eq!(recommend(&rows, None, Some(40)), Some(2));
        assert_eq!(order(rows, Some(2)).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Charlie", "Bravo", "Alpha", "Delta"]);
        let m = menu(vec![row("A.hof", "A", Some(12), false), row("B.hof", "B", Some(0), false), row("C.hof", "C", Some(30), false), row("D.hof", "D", Some(15), false)], "Hamburg", Some(40));
        assert!(m.lacks());
        assert_eq!(choices(&m), vec![2, 3, 0]);
        let rows = vec![row("Grundorf.hof", "Grundorf", None, false), row("HH_2014.hof", "Hamburg Linie 20 (2014)", None, false)];
        assert_eq!(fallback_of(&rows, "Spandau 1986", &["Berlin-Spandau"], Some("SD200".into())), Fallback::Borrowed("SD200".into()));
        assert_eq!(name_cmp("Bus 9", "bus 10"), Ordering::Less);
        assert_eq!(fold("Heerstraße"), "heerstrasse");
    }
}

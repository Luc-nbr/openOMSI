//! What the drive's steps of our look need besides phase 1's commands (Trello "Tauri 3/6": what
//! phase 1 left), working as our game-drawn launcher works (`crates/omsi-launcher-ui/src/`). A
//! file of our own, handed the commands that start with `hub_more_` by one line of
//! `omsihub::call`:
//!
//! * `hub_more_bus_best`: the bus that fits the duty best (`buspick::recommend`): of the buses
//!   whose depot files know the duty's trips, the map's own fleet before the others, then the
//!   one that knows most; a free drive (or a duty no depot file knows) the bus the map's depot
//!   runs most.

use crate::Answer;
use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// A command of ours that starts with `hub_more_`, or None when it is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "hub_more_bus_best" => bus_best(args),
        _ => return None,
    })
}

// --- the bus that fits the duty ------------------------------------------------------------------

/// Letters folded as the bus step compares them (`buspick::fold`).
fn fold(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        match c {
            'ä' | 'á' | 'à' | 'â' | 'ã' | 'å' => out.push('a'),
            'ë' | 'é' | 'è' | 'ê' => out.push('e'),
            'ï' | 'í' | 'ì' | 'î' => out.push('i'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'ß' => out.push_str("ss"),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// A depot file as the fit sees it: its name, the trips its IBIS knows, the termini it shows.
struct Depot {
    name: String,
    trips: HashSet<String>,
    termini: HashSet<String>,
}

fn depot_of(h: &omsi_vehicle::Hof) -> Depot {
    let name = if h.name.trim().is_empty() { h.path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { h.name.trim().to_string() };
    let trips = h.info_trips.iter().map(|t| fold(&t.name)).filter(|s| !s.is_empty()).collect();
    let mut termini = HashSet::new();
    for t in &h.termini {
        termini.insert(fold(&t.texture_id));
        termini.insert(fold(&t.strings.join(" ")));
        termini.extend(t.strings.iter().map(|s| fold(s)));
    }
    termini.remove("");
    Depot { name, trips, termini }
}

/// How many of the trips (name, terminus) a depot file knows (`buspick::knows`).
fn knows(d: &Depot, trips: &[(String, String)]) -> usize {
    trips.iter().filter(|(name, end)| d.trips.contains(&fold(name)) || (!end.is_empty() && d.termini.contains(&fold(end)))).count()
}

fn fav_key(file: &str) -> String {
    file.replace('\\', "/").to_lowercase()
}

/// How many of each bus file the map's own fleet has (`buspick::fleet_of`).
fn fleet_of(ai: &omsi_map::AiLists, date: Option<i32>) -> HashMap<String, f32> {
    let mut out: HashMap<String, f32> = HashMap::new();
    for g in ai.groups.iter().filter(|g| g.is_depot || g.hof.is_some()) {
        for v in &g.vehicles {
            *out.entry(fav_key(v.file.trim())).or_default() += v.weight.max(0.0);
        }
        for t in &g.typgroups {
            let n = t.entries.iter().filter(|e| date.map(|d| omsi_map::typgroup_entry_valid(e, d)).unwrap_or(true)).count();
            *out.entry(fav_key(t.file.trim())).or_default() += n.max(1) as f32;
        }
    }
    out
}

/// The depot files of a bus: those of its folder, else those of its pack's (read once a
/// folder).
fn depots_of(root: &Path, file: &str) -> Arc<Vec<Depot>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<Depot>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let path = omsi_cfg::resolve_path(root, file);
    let dir = path.parent().unwrap_or(root).to_path_buf();
    if let Some(d) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&dir) {
        return d.clone();
    }
    let pack = file.replace('\\', "/").split('/').take(2).collect::<Vec<_>>().join("/");
    let mut files = omsi_vehicle::hof::depot_files(&dir);
    if files.is_empty() && !pack.is_empty() {
        files = omsi_vehicle::hof::depot_files(&omsi_cfg::resolve_path(root, &pack));
    }
    let depots = Arc::new(files.iter().filter_map(|f| omsi_vehicle::Hof::load(f).ok()).map(|h| depot_of(&h)).collect::<Vec<_>>());
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(dir, depots.clone());
    depots
}

/// A bus weighed for the duty.
struct Candidate {
    file: String,
    name: String,
    fit: f32,
    depot: String,
    wagons: f32,
}

/// Names compared as the bus step sorts them: numbers by their value.
fn name_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// The bus to offer (`buspick::rank`).
fn rank(c: &[Candidate], free: bool) -> Option<Value> {
    let by_name = |a: &Candidate, b: &Candidate| name_cmp(&b.name, &a.name).then_with(|| b.file.cmp(&a.file));
    if !free {
        let fitting: Vec<&Candidate> = c.iter().filter(|x| x.fit > 0.0).collect();
        let own: Vec<&Candidate> = fitting.iter().copied().filter(|x| x.wagons > 0.0).collect();
        let pool = if own.is_empty() { fitting } else { own };
        if let Some(b) = pool.into_iter().max_by(|a, b| a.fit.total_cmp(&b.fit).then(a.wagons.total_cmp(&b.wagons)).then_with(|| by_name(a, b))) {
            return Some(json!({ "file": b.file, "fit": b.fit, "depot": b.depot, "free": false }));
        }
    }
    c.iter().filter(|x| x.wagons > 0.0).max_by(|a, b| a.wagons.total_cmp(&b.wagons).then_with(|| by_name(a, b))).map(|b| json!({ "file": b.file, "fit": null, "depot": "", "free": free }))
}

/// The duty's trips (name, terminus): a work shift's legs, or the tour's trips from the one it
/// starts with (`buspick::duty_trips`).
fn duty_trips(args: &Value) -> Result<Vec<(String, String)>, String> {
    if arg::<Option<bool>>(args, "free")?.unwrap_or(false) {
        return Ok(Vec::new());
    }
    let (map, date) = (arg::<String>(args, "map")?, arg::<String>(args, "date")?);
    let lines = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"))?;
    let mut trips = Vec::new();
    let legs = arg::<Option<Vec<String>>>(args, "legs")?.unwrap_or_default();
    if !legs.is_empty() {
        for leg in &legs {
            let Some((line, tour, first, count)) = core::compose::Block::parse(leg) else { continue };
            if let Some(t) = lines.iter().find(|x| x.name == line).and_then(|x| x.tours.iter().find(|t| t.number == tour)) {
                trips.extend(t.trips.iter().filter(|x| x.index >= first && x.index < first + count).map(|x| (x.name.clone(), x.terminus.clone())));
            }
        }
    } else if let (Some(line), Some(tour)) = (arg::<Option<String>>(args, "line")?, arg::<Option<String>>(args, "tour")?) {
        let first = arg::<Option<usize>>(args, "first")?.unwrap_or(0);
        if let Some(t) = lines.iter().find(|x| x.name == line).and_then(|x| x.tours.iter().find(|t| t.number == tour)) {
            trips.extend(t.trips.iter().skip(first).map(|x| (x.name.clone(), x.terminus.clone())));
        }
    }
    Ok(trips)
}

/// The bus that fits the duty best (`buspick::recommend`): `map`, `date`, `free`, the tour
/// (`line`, `tour`, `first`) or the shift's `legs`, and the buses to weigh (`buses`: file and
/// name; `only` narrows them to a line's own). Answers `{ file, fit, depot, free }` or null.
fn bus_best(args: &Value) -> Answer {
    let root = PathBuf::from(core::load_config().root);
    let map = arg::<String>(args, "map")?;
    let date = arg::<String>(args, "date")?;
    let trips = duty_trips(args)?;
    let free = arg::<Option<bool>>(args, "free")?.unwrap_or(false) || trips.is_empty();
    let mut buses = arg::<Vec<(String, String)>>(args, "buses")?;
    let only: HashSet<String> = arg::<Option<Vec<String>>>(args, "only")?.unwrap_or_default().iter().map(|f| fav_key(f)).collect();
    if !only.is_empty() {
        buses.retain(|(f, _)| only.contains(&fav_key(f)));
    }
    let Some(map_dir) = omsi_cfg::resolve_path(&root, &map).parent().map(Path::to_path_buf) else { return Ok(Value::Null) };
    let code = omsi_map::date_code(&date);
    let chrono = code.map(|d| omsi_map::active_chrono_dirs(&map_dir, d)).unwrap_or_default();
    let ai = omsi_map::ailists::ailists_with_chrono(&map_dir, &chrono);
    let fleet = fleet_of(&ai, code);
    let weigh = |(file, name): &(String, String)| -> Candidate {
        let wagons = fleet.get(&fav_key(file)).copied().unwrap_or(0.0);
        let mut c = Candidate { file: file.clone(), name: name.clone(), fit: 0.0, depot: String::new(), wagons };
        if !free {
            for d in depots_of(&root, file).iter() {
                let fit = knows(d, &trips) as f32 / trips.len() as f32;
                if fit > c.fit {
                    (c.fit, c.depot) = (fit, d.name.clone());
                }
            }
        }
        c
    };
    // the map's own buses first: when one of them fits, the others need not be read
    let (own, rest): (Vec<&(String, String)>, Vec<&(String, String)>) = buses.iter().partition(|(f, _)| fleet.get(&fav_key(f)).is_some_and(|w| *w > 0.0));
    let mut c: Vec<Candidate> = own.into_iter().map(weigh).collect();
    if !free && !c.iter().any(|x| x.fit > 0.0) {
        c.extend(rest.into_iter().map(weigh));
    }
    Ok(rank(&c, free).unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bus_offered_is_the_maps_own_that_knows_most() {
        let cand = |file: &str, fit: f32, wagons: f32| Candidate { file: file.into(), name: file.into(), fit, depot: "D".into(), wagons };
        // of those that fit, the map's own before the others
        let c = [cand("a", 1.0, 0.0), cand("b", 0.5, 3.0), cand("c", 0.2, 9.0)];
        assert_eq!(rank(&c, false).unwrap()["file"], "b");
        // nothing fits: the map's own most common, said plainly
        let c = [cand("a", 0.0, 1.0), cand("b", 0.0, 4.0)];
        let p = rank(&c, false).unwrap();
        assert_eq!((p["file"].as_str(), p["fit"].is_null()), (Some("b"), true));
        // a free drive: the one the map's depot runs most
        assert_eq!(rank(&c, true).unwrap()["free"], true);
        assert_eq!(fold("Müller-Straße 3"), "mullerstrasse3");
    }
}

//! What the drive's steps of our look need besides phase 1's commands (Trello "Tauri 3/6": what
//! phase 1 left), working as our game-drawn launcher works (`crates/omsi-launcher-ui/src/`). A
//! file of our own, handed the commands that start with `hub_more_` by one line of
//! `omsihub::call`:
//!
//! * `hub_more_bus_best`: the bus that fits the duty best (`buspick::recommend`): of the buses
//!   whose depot files know the duty's trips, the map's own fleet before the others, then the
//!   one that knows most; a free drive (or a duty no depot file knows) the bus the map's depot
//!   runs most.
//! * `hub_more_weather_read`, `hub_more_weather_write`: one's own weather (`custom:...`) as the
//!   day step's editor shows its fields, a preset made one's own to edit, and the fields
//!   written back (`daytime::body`, `weather_choice::CustomWeather`).
//! * `hub_more_welcome`, `hub_more_folder`, `hub_more_personnel`: the welcome (`welcome.rs`):
//!   whether it opens and where (and the guided tour, `OMSI_LAUNCHER_TOUR`), what a folder
//!   pointed to as OMSI 2's is, and a driver's personnel number and code for the pass.

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
        "hub_more_weather_read" => weather_read(args),
        "hub_more_weather_write" => weather_write(args),
        "hub_more_welcome" => welcome_due(),
        "hub_more_folder" => folder(args),
        "hub_more_personnel" => personnel(args),
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


// --- one's own weather ---------------------------------------------------------------------------

use omsi_content::weather_choice::{custom_weather, CustomWeather};

/// One's own weather as the editor shows it (the fields of `CustomWeather`).
fn custom_json(c: &CustomWeather) -> Value {
    json!({
        "visibility_m": c.visibility_m, "brightness": c.brightness, "wind_dir": c.wind_dir, "wind_speed": c.wind_speed,
        "temp_c": c.temp_c, "humidity": c.humidity, "pressure": c.pressure, "cloud_cover": c.cloud_cover,
        "cloud_base_m": c.cloud_base_m, "precip": c.precip, "precip_intensity": c.precip_intensity,
        "road_wetness": c.road_wetness, "snow_cover": c.snow_cover, "snow_on_road": c.snow_on_road,
    })
}

/// The editor's fields back into one's own weather (a field left out keeps the default).
fn custom_of(v: &Value) -> CustomWeather {
    let mut c = CustomWeather::default();
    let f = |k: &str, d: f32| v.get(k).and_then(Value::as_f64).map(|x| x as f32).unwrap_or(d);
    let b = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
    c.visibility_m = f("visibility_m", c.visibility_m);
    c.brightness = f("brightness", c.brightness);
    c.wind_dir = f("wind_dir", c.wind_dir);
    c.wind_speed = f("wind_speed", c.wind_speed);
    c.temp_c = f("temp_c", c.temp_c);
    c.humidity = f("humidity", c.humidity);
    c.pressure = f("pressure", c.pressure);
    c.cloud_cover = f("cloud_cover", c.cloud_cover);
    c.cloud_base_m = f("cloud_base_m", c.cloud_base_m);
    c.precip = f("precip", c.precip as f32).round() as i32;
    c.precip_intensity = f("precip_intensity", c.precip_intensity);
    c.road_wetness = f("road_wetness", c.road_wetness);
    c.snow_cover = b("snow_cover", c.snow_cover);
    c.snow_on_road = b("snow_on_road", c.snow_on_road);
    c.normalize();
    c
}

/// The weather chosen as the day step reads it (`daytime::DayCtx`): one's own weather's fields
/// when it is one, else None, and a preset as one's own weather to edit (`drive::
/// selected_weather_as_custom`), else None: `{ custom, as_custom }`.
fn weather_read(args: &Value) -> Answer {
    let w: String = arg::<Option<String>>(args, "weather")?.unwrap_or_default();
    let custom = custom_weather(Some(&w)).map(|c| custom_json(&c));
    let preset = !(w.is_empty() || w == "cycle" || w.to_ascii_lowercase().starts_with("metar:") || custom.is_some());
    let as_custom = preset
        .then(|| {
            let root = core::root_dir().ok()?;
            let weather = omsi_content::weather::Weather::load(&omsi_cfg::resolve_path(&root, &w)).ok()?;
            let wet = (weather.ground_wet[0] / 255.0).clamp(0.0, 1.0);
            Some(CustomWeather::from_weather(&weather, 1.0, wet).encode())
        })
        .flatten();
    Ok(json!({ "custom": custom, "as_custom": as_custom, "fresh": CustomWeather::default().encode() }))
}

/// The editor's fields as the weather's value (`CustomWeather::encode`, made sound first), and
/// the fields as they are kept: `{ weather, custom }`.
fn weather_write(args: &Value) -> Answer {
    let c = custom_of(args.get("custom").unwrap_or(&Value::Null));
    Ok(json!({ "weather": c.encode(), "custom": custom_json(&c) }))
}


// --- the welcome -----------------------------------------------------------------------------

/// Whether the welcome opens as the launcher starts, and at which step (`welcome::due`): when
/// it was not seen yet (`welcome_done`), or wherever `OMSI_LAUNCHER_WELCOME` says (`off`:
/// never); and whether the OMSI 2 folder has to be pointed to (`welcome::folder_needed`):
/// `{ step, folder }`, `step` null when it does not open.
fn welcome_due() -> Answer {
    let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
    let asked = omsi_cfg::env::var("OMSI_LAUNCHER_WELCOME").ok().map(|a| a.trim().to_ascii_lowercase());
    let step = match asked.as_deref() {
        Some("off" | "0") => None,
        Some(a) if !a.is_empty() => Some(match a {
            "language" | "1" => "language",
            "interface" | "ui" | "2" => "interface",
            "animations" | "3" => "animations",
            "folder" => "folder",
            "driver" | "profile" => "driver",
            "ready" | "done" => "ready",
            _ => "language",
        }),
        _ => (!settings.get("welcome_done").and_then(Value::as_bool).unwrap_or(false)).then_some("language"),
    };
    let root = core::load_config().root;
    // (`OMSI_LAUNCHER_TOUR=n`: the guided tour at its n-th stop as the launcher starts, as the
    // game-drawn launcher's `tour::begin` - for pictures of it)
    let tour = omsi_cfg::env::var("OMSI_LAUNCHER_TOUR").ok().and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n > 0);
    Ok(json!({ "step": step, "folder": !omsi_cfg::missing_original_essentials(Path::new(root.trim())).is_empty(), "tour": tour }))
}

/// What a folder pointed to as OMSI 2's is (`welcome::folder_verdict`): `fine`, `empty`,
/// `missing`, `own` (openOMSI's own folder) or `lacks` with what it lacks.
fn folder_verdict(root: &str, exists: bool, own: bool, missing: &[String]) -> (&'static str, String) {
    if root.trim().is_empty() {
        ("empty", String::new())
    } else if !exists {
        ("missing", String::new())
    } else if own || missing.iter().any(|m| m.contains("content folder")) {
        ("own", String::new())
    } else if missing.is_empty() {
        ("fine", String::new())
    } else {
        ("lacks", missing.iter().take(3).cloned().collect::<Vec<_>>().join(", "))
    }
}

fn folder(args: &Value) -> Answer {
    let root: String = arg::<Option<String>>(args, "root")?.unwrap_or_default();
    let p = Path::new(root.trim());
    let (verdict, what) = folder_verdict(&root, p.is_dir(), p.join("openomsi.exe").exists() || p.join("openomsi").is_file(), &omsi_cfg::missing_original_essentials(p));
    Ok(json!({ "verdict": verdict, "what": what }))
}

/// A driver's personnel number and code in `personnel.json` (the companion's, kept by the
/// driver's name in lower case: `welcome::personnel_in`), if they are sound.
fn personnel_in(text: &str, driver: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let e = v.get("drivers")?.get(driver.trim().to_lowercase())?;
    let (number, code) = (e.get("number")?.as_str()?, e.get("code")?.as_str()?);
    let digits = |s: &str, n: std::ops::RangeInclusive<usize>| n.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    (digits(number, 3..=10) && digits(code, 3..=8)).then(|| (number.to_string(), code.to_string()))
}

/// `n` digits from `rnd`, the first of them 1-9 (the companion's `signon::digits`).
fn digits(n: usize, rnd: &mut dyn FnMut() -> u64) -> String {
    (0..n).map(|i| char::from(b'0' + if i == 0 { 1 + rnd() % 9 } else { rnd() % 10 } as u8)).collect()
}

fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish()
}

/// A driver's personnel number and code (`~/.openomsi/personnel.json`): read, and with `make`
/// made and written down when the driver has none yet - as the game-drawn launcher's welcome
/// does for a new driver (`companion::personnel_of_driver`), so they are known before the first
/// sign-on. A file that is there but not one of these is left alone. `{ number, code }` or null.
fn personnel(args: &Value) -> Answer {
    let name: String = arg(args, "name")?;
    let make = arg::<Option<bool>>(args, "make")?.unwrap_or(false);
    let path = core::data_dir().join("personnel.json");
    let text = std::fs::read_to_string(&path).ok();
    if let Some((number, code)) = text.as_deref().and_then(|t| personnel_in(t, &name)) {
        return Ok(json!({ "number": number, "code": code }));
    }
    let key = { let k = name.trim().to_lowercase(); if k.is_empty() { "driver".to_string() } else { k } };
    let mut file: serde_json::Map<String, Value> = match text.as_deref().map(str::trim) {
        None | Some("") => serde_json::Map::new(),
        Some(t) => match serde_json::from_str::<Value>(t).ok().and_then(|v| v.get("drivers").and_then(Value::as_object).cloned()) {
            Some(d) => d,
            None => return Ok(Value::Null),
        },
    };
    if !make {
        return Ok(Value::Null);
    }
    let mut rnd = random_u64;
    let (number, code) = (digits(6, &mut rnd), digits(4, &mut rnd));
    file.insert(key, json!({ "number": number, "code": code }));
    let body = serde_json::to_string_pretty(&json!({ "drivers": file })).map_err(|e| e.to_string())?;
    // (written beside it first and put in its place whole: the game reading it at the same
    // moment never finds it half written)
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "number": number, "code": code }))
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

    #[test]
    fn ones_own_weather_goes_through_the_editor_unchanged() {
        let w = "custom:vis=1200;br=0.80;wd=270;ws=6.5;t=-3.0;rh=88;p=1013;c=0.750;cb=50;pt=2;pi=120;wet=0.40;snow=1;snowroad=0";
        let read = weather_read(&json!({ "weather": w })).unwrap();
        assert!(read["as_custom"].is_null());
        let back = weather_write(&json!({ "custom": read["custom"] })).unwrap();
        assert_eq!(back["weather"], w);
        // (out of range made sound, a preset's word is not one's own weather)
        let back = weather_write(&json!({ "custom": { "visibility_m": 10.0, "precip": 7 } })).unwrap();
        assert_eq!((back["custom"]["visibility_m"].as_f64(), back["custom"]["precip"].as_i64()), (Some(50.0), Some(2)));
        assert!(weather_read(&json!({ "weather": "cycle" })).unwrap()["custom"].is_null());
    }

    #[test]
    fn the_welcome_judges_the_folder_and_reads_the_pass_as_the_game_drawn_one() {
        assert_eq!(folder_verdict(" ", false, false, &[]).0, "empty");
        assert_eq!(folder_verdict("D:/nowhere", false, false, &["maps".into()]).0, "missing");
        assert_eq!(folder_verdict("C:/openOMSI", true, true, &["Omsi.exe".into()]).0, "own");
        assert_eq!(folder_verdict("C:/x", true, false, &["the content folder".into()]).0, "own");
        assert_eq!(folder_verdict("C:/OMSI 2", true, false, &[]).0, "fine");
        assert_eq!(folder_verdict("C:/half", true, false, &["maps".into(), "Vehicles".into(), "Fonts".into(), "Sceneryobjects".into()]), ("lacks", "maps, Vehicles, Fonts".to_string()));
        let file = r#"{ "drivers": { "anna": { "number": "482913", "code": "4821" }, "bad": { "number": "12ab", "code": "1" } } }"#;
        assert_eq!(personnel_in(file, "Anna"), Some(("482913".into(), "4821".into())));
        assert_eq!(personnel_in(file, "bad"), None);
        let mut k = 0u64;
        let mut rnd = || { k += 1; k * 7 };
        let d = digits(6, &mut rnd);
        assert!(d.len() == 6 && !d.starts_with('0') && d.bytes().all(|b| b.is_ascii_digit()), "{d}");
    }
}

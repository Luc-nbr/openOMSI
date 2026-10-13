//! Omsi-Hub's own part of the launcher page's commands (Trello "Tauri 1/6"): what our look of
//! the desktop launcher needs beyond upstream's commands, kept in this file of its own so that
//! merges with openOMSI stay easy. `crate::call` hands every command it does not know here
//! (one line), and `crate::i18n` takes our translation tables in with its own (one line).
//!
//! * the translation tables of our launcher: every `crates/omsi-app/locales/*.yml` besides
//!   upstream's `app.yml` - the texts our game-drawn launcher shows are the page's too - and
//!   `tauri-omsihub.yml` with the texts only the page has;
//! * `hub_boot`: the page to open first (`OMSI_LAUNCHER_PAGE`, as our game-drawn launcher reads
//!   it: `drive:map`, `settings` ...);
//! * `hub_shifts`: work shifts put together from the day's timetable
//!   (`omsi_launcher_lib::compose`), as the game-drawn launcher's duty step lists them;
//! * `hub_duty_extras`: what a duty of ours adds to upstream's - the start moved the
//!   setting `bus_lead` ahead of the first departure, and the display font chosen for the bus;
//! * `hub_map_facts`: a map's tours and the year it plays in, as the map step says them (the
//!   same cache as the game-drawn launcher's, `mapfacts|`);
//! * `hub_weather_extras`: the airport whose METAR report is the map's current weather, and
//!   OMSI's list of airports.
//! * `hub_own_lines`: the player's own lines of a map (the line editor's), which the line lists
//!   show apart from the map's ("Map lines" / "My lines").
//!
//! A later part has a submodule of its own, handed the commands this one does not know by one
//! line of `call`: `editors` (Trello "Tauri 5/6": the timetable, the bus's depot files, the
//! line, depot and livery editors).

pub mod company;

use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::compose::{Block, ComposedDuty, Leg, Wish};
use omsi_launcher_lib::LineInfo;
use serde_json::{json, Value};
use std::collections::HashMap;

pub mod editors;

/// language -> (English -> translation), as `crate::i18n` keeps them.
type Tables = HashMap<String, HashMap<String, String>>;

/// The tables of our launcher besides upstream's `app.yml` (which `crate::i18n` reads itself):
/// the game reads every file of the folder (`rust_i18n::i18n!("locales")`), so the page does
/// too. `the_page_has_every_table_of_the_game` keeps this list whole.
pub const TABLES: &[(&str, &str)] = &[
    ("tauri-omsihub.yml", include_str!("../../../omsi-app/locales/tauri-omsihub.yml")),
    ("tauri-company.yml", include_str!("../../../omsi-app/locales/tauri-company.yml")),
    ("tauri-editors.yml", include_str!("../../../omsi-app/locales/tauri-editors.yml")),
    ("app-theme.yml", include_str!("../../../omsi-app/locales/app-theme.yml")),
    ("bedrijf2.yml", include_str!("../../../omsi-app/locales/bedrijf2.yml")),
    ("bedrijfsdienst.yml", include_str!("../../../omsi-app/locales/bedrijfsdienst.yml")),
    ("bestemming.yml", include_str!("../../../omsi-app/locales/bestemming.yml")),
    ("bugs-company.yml", include_str!("../../../omsi-app/locales/bugs-company.yml")),
    ("bugs-timetable.yml", include_str!("../../../omsi-app/locales/bugs-timetable.yml")),
    ("casual.yml", include_str!("../../../omsi-app/locales/casual.yml")),
    ("company-chooser.yml", include_str!("../../../omsi-app/locales/company-chooser.yml")),
    ("company-contracts.yml", include_str!("../../../omsi-app/locales/company-contracts.yml")),
    ("company-drive.yml", include_str!("../../../omsi-app/locales/company-drive.yml")),
    ("company-easy.yml", include_str!("../../../omsi-app/locales/company-easy.yml")),
    ("company-home.yml", include_str!("../../../omsi-app/locales/company-home.yml")),
    ("company-pages.yml", include_str!("../../../omsi-app/locales/company-pages.yml")),
    ("company-planning.yml", include_str!("../../../omsi-app/locales/company-planning.yml")),
    ("company-realtime.yml", include_str!("../../../omsi-app/locales/company-realtime.yml")),
    ("company-research.yml", include_str!("../../../omsi-app/locales/company-research.yml")),
    ("company-staff.yml", include_str!("../../../omsi-app/locales/company-staff.yml")),
    ("dealer-delivery.yml", include_str!("../../../omsi-app/locales/dealer-delivery.yml")),
    ("dealer.yml", include_str!("../../../omsi-app/locales/dealer.yml")),
    ("easy-polish.yml", include_str!("../../../omsi-app/locales/easy-polish.yml")),
    ("fix_core_company_economy.yml", include_str!("../../../omsi-app/locales/fix_core_company_economy.yml")),
    ("fix_ui_editors.yml", include_str!("../../../omsi-app/locales/fix_ui_editors.yml")),
    ("fontfit.yml", include_str!("../../../omsi-app/locales/fontfit.yml")),
    ("fonts.yml", include_str!("../../../omsi-app/locales/fonts.yml")),
    ("game-modes.yml", include_str!("../../../omsi-app/locales/game-modes.yml")),
    ("hof.yml", include_str!("../../../omsi-app/locales/hof.yml")),
    ("lakauto.yml", include_str!("../../../omsi-app/locales/lakauto.yml")),
    ("lijnen.yml", include_str!("../../../omsi-app/locales/lijnen.yml")),
    ("lijnkleur.yml", include_str!("../../../omsi-app/locales/lijnkleur.yml")),
    ("lijnsoort.yml", include_str!("../../../omsi-app/locales/lijnsoort.yml")),
    ("line-existing.yml", include_str!("../../../omsi-app/locales/line-existing.yml")),
    ("livery-layers.yml", include_str!("../../../omsi-app/locales/livery-layers.yml")),
    ("logo.yml", include_str!("../../../omsi-app/locales/logo.yml")),
    ("openomsi-0227.yml", include_str!("../../../omsi-app/locales/openomsi-0227.yml")),
    ("openomsi-0230.yml", include_str!("../../../omsi-app/locales/openomsi-0230.yml")),
    ("planning-studio.yml", include_str!("../../../omsi-app/locales/planning-studio.yml")),
    ("ritkaart.yml", include_str!("../../../omsi-app/locales/ritkaart.yml")),
    ("rosterflow-lines.yml", include_str!("../../../omsi-app/locales/rosterflow-lines.yml")),
    ("rosterflow-timeplan.yml", include_str!("../../../omsi-app/locales/rosterflow-timeplan.yml")),
    ("rosterflow-tutorial.yml", include_str!("../../../omsi-app/locales/rosterflow-tutorial.yml")),
    ("rosterflow.yml", include_str!("../../../omsi-app/locales/rosterflow.yml")),
    ("sessie.yml", include_str!("../../../omsi-app/locales/sessie.yml")),
    ("slepen.yml", include_str!("../../../omsi-app/locales/slepen.yml")),
    ("snelheid.yml", include_str!("../../../omsi-app/locales/snelheid.yml")),
    ("telefoon.yml", include_str!("../../../omsi-app/locales/telefoon.yml")),
    ("tijd.yml", include_str!("../../../omsi-app/locales/tijd.yml")),
    ("tobii.yml", include_str!("../../../omsi-app/locales/tobii.yml")),
    ("vertalingen.yml", include_str!("../../../omsi-app/locales/vertalingen.yml")),
    ("verwijderen.yml", include_str!("../../../omsi-app/locales/verwijderen.yml")),
];

/// Upstream's tables with ours taken in: a text upstream's `app.yml` has keeps its
/// translation, the ones it lacks come from ours (read with `crate::i18n`'s own `parse`).
pub fn with_ours(mut tables: Tables, parse: fn(&str) -> Tables) -> Tables {
    for (_, text) in TABLES {
        for (lang, texts) in parse(text) {
            let into = tables.entry(lang).or_default();
            for (k, v) in texts {
                into.entry(k).or_insert(v);
            }
        }
    }
    tables
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// A command of ours, or None when `cmd` is not one (`crate::call` says it is unknown then).
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    Some(match cmd {
        "hub_boot" => hub_boot(),
        "hub_shifts" => (|| {
            let legs = arg::<Option<Vec<String>>>(args, "legs")?.unwrap_or_default();
            hub_shifts(&arg::<String>(args, "map")?, &arg::<String>(args, "date")?, arg::<i64>(args, "minutes")?, arg::<usize>(args, "daypart")?, arg::<u64>(args, "seed")?, &legs)
        })(),
        "hub_duty_extras" => (|| hub_duty_extras(&arg::<String>(args, "bus")?, &arg::<String>(args, "date")?, arg::<i32>(args, "minutes")?, arg::<bool>(args, "free")?))(),
        "hub_map_facts" => (|| hub_map_facts(&arg::<Vec<String>>(args, "maps")?))(),
        "hub_weather_extras" => (|| hub_weather_extras(&arg::<String>(args, "map")?))(),
        "hub_own_lines" => (|| hub_own_lines(&arg::<String>(args, "map")?))(),
        // the bus company's commands (Trello "Tauri 2/6": `company.rs`)
        c if c.starts_with("co_") => return company::call(c, args, p),
        // (the editors' commands: `editors.rs`)
        _ => return editors::call(cmd, args, p),
    })
}

/// A command of ours that answers bytes (the bus company's staff portraits, the livery
/// studio's pictures), or None when `cmd` is not one (`crate::call_bytes` says it is unknown then).
pub fn call_bytes(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Result<Vec<u8>, String>> {
    match cmd {
        c if c.starts_with("co_") => company::call_bytes(c, args),
        _ => editors::call_bytes(cmd, args, p),
    }
}

// --- the start ---------------------------------------------------------------------------

/// What the page opens on: `OMSI_LAUNCHER_PAGE` (as our game-drawn launcher reads it) - a page
/// (`settings`, `mods` ...) or a step of the drive (`drive:map`, `drive:bus` ...).
fn hub_boot() -> Answer {
    Ok(json!({ "page": omsi_cfg::flags::OMSI_LAUNCHER_PAGE.var() }))
}

// --- work shifts -------------------------------------------------------------------------

/// The parts of the day a shift may begin in, as our game-drawn launcher offers them
/// (`state::DAYPARTS`): name, and from-to in minutes of the day (the night runs past midnight).
pub const DAYPARTS: [(&str, Option<(i32, i32)>); 5] = [
    ("All day", None),
    ("Morning", Some((4 * 60, 11 * 60))),
    ("Midday", Some((11 * 60, 16 * 60))),
    ("Evening", Some((16 * 60, 22 * 60))),
    ("Night", Some((22 * 60, 30 * 60))),
];

/// How many shifts are drawn for one wish (the game-drawn launcher's eight).
const SHIFTS: usize = 8;

/// The shifts of `minutes` length beginning in the part of the day `daypart`, from the day's
/// timetable of `map`; `seed` draws another set ("Find other shifts"). The shift `legs` (its
/// `--duty-leg`s, the one chosen last) comes first when it still holds and the draw left it
/// out, as the game-drawn launcher keeps it in its list.
fn hub_shifts(map: &str, date: &str, minutes: i64, daypart: usize, seed: u64, legs: &[String]) -> Answer {
    let lines = core::list_lines(map, date).map_err(err)?;
    let window = DAYPARTS.get(daypart).and_then(|d| d.1);
    let wish = Wish { seconds: minutes.clamp(30, 480) as f64 * 60.0, from: window.map(|w| w.0 as f64 * 60.0), to: window.map(|w| w.1 as f64 * 60.0), lines: Vec::new(), seed };
    let mut duties = core::compose::compose(&lines, &wish, SHIFTS);
    let chosen: Vec<String> = legs.to_vec();
    if !chosen.is_empty() && !duties.iter().any(|d| d.blocks().iter().map(Block::arg).collect::<Vec<_>>() == chosen) {
        if let Some(d) = shift_of_legs(&chosen, &lines) {
            duties.insert(0, d);
        }
    }
    Ok(json!(duties.iter().map(|d| shift_json(d, &lines)).collect::<Vec<_>>()))
}

/// The shift `legs` stands for, read back from the day's timetable (None when one of its
/// tours or trips is not in it). As `shiftsheet::shift_of_legs` of the game-drawn launcher.
fn shift_of_legs(legs: &[String], lines: &[LineInfo]) -> Option<ComposedDuty> {
    let mut out = ComposedDuty::default();
    for a in legs {
        let (line, tour, first, count) = Block::parse(a)?;
        let t = lines.iter().find(|x| x.name == line)?.tours.iter().find(|t| t.number == tour)?;
        let trips: Vec<_> = t.trips.iter().filter(|x| x.index >= first && x.index < first + count).collect();
        if trips.len() != count {
            return None;
        }
        for x in trips {
            let shown = if x.line.is_empty() { line.clone() } else { x.line.clone() };
            out.legs.push(Leg { line: line.clone(), tour: tour.clone(), index: x.index, trip: x.name.clone(), shown, from: x.from.clone(), terminus: x.terminus.clone(), departure: x.departure, arrival: x.arrival });
        }
    }
    (!out.legs.is_empty()).then_some(out)
}

/// A shift as the page lists it: its `--duty-leg`s (`key`), when it begins and ends, its
/// lines, and its trips - each with its stops, the break before it and, at the first trip of
/// another tour, the tour the bus goes on with (`takes_over`).
fn shift_json(d: &ComposedDuty, lines: &[LineInfo]) -> Value {
    let trips: Vec<Value> = d
        .legs
        .iter()
        .enumerate()
        .map(|(k, leg)| {
            let prev = k.checked_sub(1).map(|j| &d.legs[j]);
            let same_tour = prev.is_some_and(|p| p.line == leg.line && p.tour == leg.tour);
            let stops = lines.iter().find(|x| x.name == leg.line).and_then(|x| x.tours.iter().find(|t| t.number == leg.tour)).and_then(|t| t.trips.iter().find(|x| x.index == leg.index)).map(|t| t.stops.len()).unwrap_or(0);
            json!({
                "departure": leg.departure, "arrival": leg.arrival, "line": leg.shown, "from": leg.from, "terminus": leg.terminus,
                "stops": stops, "pause": prev.map(|p| (leg.departure - p.arrival).max(0.0)).unwrap_or(0.0),
                "takes_over": (prev.is_some() && !same_tour).then(|| json!([leg.line, leg.tour])),
                "tour_line": leg.line, "tour": leg.tour, "index": leg.index, "trip": leg.trip,
            })
        })
        .collect();
    let first = d.blocks().first().map(|b| json!({ "line": b.line, "tour": b.tour, "index": b.first, "departure": b.departure }));
    json!({ "key": d.blocks().iter().map(Block::arg).collect::<Vec<_>>(), "start": d.start(), "end": d.end(), "seconds": d.seconds(), "lines": d.lines(), "changes": d.changes(), "first": first, "trips": trips })
}

// --- the duty ----------------------------------------------------------------------------

/// What our duty adds to upstream's: a duty of the timetable starts the setting `bus_lead`
/// (minutes) before its first departure - the bus stands ready, the player is not late at once
/// (a free drive starts when it was asked to) - and the display font chosen for `bus`.
fn hub_duty_extras(bus: &str, date: &str, minutes: i32, free: bool) -> Answer {
    let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
    let lead = if free { 0 } else { core::bus_lead(settings.get("bus_lead").and_then(|x| x.as_i64())) };
    let (date, time) = core::lead_start(date, minutes, lead);
    Ok(json!({ "date": date, "time": format!("{:02}:{:02}", time / 60, time % 60), "lead": lead, "display_font": core::busfonts::BusFonts::load().font_for(bus) }))
}

// --- the maps' facts --------------------------------------------------------------------

/// What a map's folder says beyond its global.cfg: how many tours its timetable has for the
/// player, and the year it plays in. The game-drawn launcher's `mapchoice::Facts`, under the
/// same key in the content index: what one of them read, the other has at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    pub tours: usize,
    pub year: Option<i32>,
}

/// The content index's prefix for the maps' facts (the game-drawn launcher's).
const FACTS_KEY: &str = "mapfacts|";

/// The facts of the maps `files` (`maps/Grundorf/global.cfg` ...): `{ file: { tours, year } }`.
fn hub_map_facts(files: &[String]) -> Answer {
    let base = core::root_dir().map_err(err)?;
    let mut shipped: Option<HashMap<String, i32>> = None;
    let mut out = serde_json::Map::new();
    for file in files {
        let dir = map_dir(&base, file);
        let tt = omsi_cfg::resolve_path(&dir, "TTData");
        let key = format!("{FACTS_KEY}{}", dir.display());
        let stamp = core::index::folder_stamp(&[dir.clone(), tt.clone(), base.join("Situations")]);
        let f: Facts = core::index::cached(&key, stamp, || {
            let shipped = shipped.get_or_insert_with(|| situation_years(&base));
            (Facts { tours: count_tours(&tt), year: year_of(&dir, shipped) }, Vec::new())
        });
        out.insert(file.clone(), json!(f));
    }
    core::index::save(FACTS_KEY, None);
    Ok(Value::Object(out))
}

/// A map's folder, found the way the map picture finds it: in the content roots first (a
/// mod's map), else under the OMSI folder.
fn map_dir(root: &std::path::Path, file: &str) -> std::path::PathBuf {
    let global = omsi_cfg::find_in_roots(file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(root, file));
    global.parent().map(std::path::Path::to_path_buf).unwrap_or(global)
}

/// The tours a player can drive (`TTData/*.ttl`): those with trips on a line the player may
/// drive (`[userallowed]`); a map that allows none, all with trips.
fn count_tours(tt: &std::path::Path) -> usize {
    let Some(entries) = omsi_cfg::vfs::list_dir(tt) else { return 0 };
    let lines: Vec<omsi_timetable::Line> = entries
        .iter()
        .filter(|(n, dir)| !dir && n.to_string_lossy().to_ascii_lowercase().ends_with(".ttl"))
        .filter_map(|(n, _)| omsi_timetable::Line::load(&tt.join(n)).ok())
        .collect();
    let with_trips = |l: &omsi_timetable::Line| l.tours.iter().filter(|t| !t.trips.is_empty()).count();
    let allowed: usize = lines.iter().filter(|l| l.user_allowed).map(with_trips).sum();
    if allowed > 0 {
        allowed
    } else {
        lines.iter().map(with_trips).sum()
    }
}

/// The year a map plays in: where the game left it last (`laststn.osn`, and the copies
/// Omsi-Hub keeps of OMSI's own), else a situation OMSI ships for it, else a year in its
/// folder's name.
fn year_of(dir: &std::path::Path, shipped: &HashMap<String, i32>) -> Option<i32> {
    let folder = dir.file_name()?.to_string_lossy().to_string();
    for name in ["laststn.osn", "laststn.osn.voor-omsi-enhancer", "laststn.osn.voor-omsi-career"] {
        let p = dir.join(name);
        if !omsi_cfg::vfs::is_file(&p) {
            continue;
        }
        if let Some((_, y)) = omsi_cfg::CfgFile::read(&p).ok().as_ref().and_then(situation_place) {
            return Some(y);
        }
    }
    shipped.get(&folder.to_lowercase()).copied().or_else(|| year_in_name(&folder))
}

/// The years of the situations in OMSI's `Situations` folder, by the map's folder (lower
/// case); the first one in name order counts.
fn situation_years(root: &std::path::Path) -> HashMap<String, i32> {
    let mut files: Vec<std::path::PathBuf> = omsi_cfg::vfs::read_dir_paths(&root.join("Situations")).into_iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("osn"))).collect();
    files.sort();
    let mut out = HashMap::new();
    for f in files {
        if let Some((folder, y)) = omsi_cfg::CfgFile::read(&f).ok().as_ref().and_then(situation_place) {
            out.entry(folder.to_lowercase()).or_insert(y);
        }
    }
    out
}

/// Where and when a situation plays: its map's folder and the year. Not one Omsi-Hub wrote
/// itself (it carries the date of the last drive, not the map's).
fn situation_place(f: &omsi_cfg::CfgFile) -> Option<(String, i32)> {
    let (mut map, mut year, mut own) = (None, None, false);
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "name" => {
                let n = r.str().trim().to_lowercase();
                own = n.starts_with("omsi enhancer") || n.starts_with("omsi career") || n.starts_with("omsi-hub");
            }
            "description" => {
                r.until("[end]");
            }
            "map" => map = map_folder(r.str()),
            "time" => year = Some(r.i32()).filter(|y| (1900..=2100).contains(y)),
            _ => {}
        }
    }
    if own {
        return None;
    }
    Some((map?, year?))
}

/// The map's folder in a situation's `[map]` line (`maps\Grundorf\global.cfg`).
fn map_folder(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split(['\\', '/']).map(str::trim).filter(|p| !p.is_empty()).collect();
    match parts.iter().position(|p| p.eq_ignore_ascii_case("maps")) {
        Some(k) => parts.get(k + 1).map(|s| s.to_string()),
        None if parts.len() >= 2 => Some(parts[parts.len() - 2].to_string()),
        None => None,
    }
}

/// A year standing in a name on its own: four digits between 1900 and 2099, no more digits
/// either side ("Hamburg109" has none, "Bad_Huegelsdorf_2020" has 2020).
fn year_in_name(name: &str) -> Option<i32> {
    let b = name.as_bytes();
    let mut k = 0;
    while k < b.len() {
        if !b[k].is_ascii_digit() {
            k += 1;
            continue;
        }
        let start = k;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
        }
        if k - start == 4 {
            if let Ok(y) = name[start..k].parse::<i32>() {
                if (1900..=2099).contains(&y) {
                    return Some(y);
                }
            }
        }
    }
    None
}

// --- the player's own lines ---------------------------------------------------------------

/// The player's lines of `map` (`core::lines::own_lines_of_map`): the timetable line each was
/// written as (`file`, a `LineInfo::name`), its number, name, colour and where it goes.
fn hub_own_lines(map: &str) -> Answer {
    let own = core::lines::own_lines_of_map(map);
    Ok(json!(own.iter().map(|o| json!({ "file": o.file, "number": o.number, "name": o.name, "colour": o.colour, "destinations": o.destinations, "caption": o.caption() })).collect::<Vec<_>>()))
}

// --- the weather -------------------------------------------------------------------------

/// The current weather's airport for `map` (the one nearest the map, by its `timezone.txt`)
/// and OMSI's METAR airports (`Weather/ICAO.txt`): `{ airport, airports: [[code, name]] }`.
fn hub_weather_extras(map: &str) -> Answer {
    let root = core::root_dir().map_err(err)?;
    let airports = omsi_content::weather_choice::metar_airports(&root);
    Ok(json!({ "airport": nearest_airport(&root, map), "airports": airports }))
}

/// The airport whose METAR report is the map's current weather: the nearest of the list round
/// Europe to the map's place (`timezone.txt`), Berlin where the map says none. As the
/// game-drawn launcher's `drive::nearest_airport`.
fn nearest_airport(root: &std::path::Path, map: &str) -> String {
    const AIRPORTS: &[(&str, f64, f64)] = &[
        ("BKPR", 42.57, 21.04), ("EBBR", 50.90, 4.48), ("EDDH", 53.63, 9.99), ("EDDM", 48.35, 11.79),
        ("EDDN", 49.50, 11.08), ("EDDP", 51.42, 12.24), ("EDDS", 48.69, 9.22), ("EDDB", 52.37, 13.52),
        ("EDOP", 53.43, 11.78), ("EETN", 59.41, 24.83), ("EFHF", 60.25, 25.04), ("EGAA", 54.66, -6.22),
        ("EGCC", 53.35, -2.27), ("EGLL", 51.47, -0.45), ("EGPH", 55.95, -3.37), ("EHAM", 52.31, 4.76),
        ("EIDW", 53.42, -6.27), ("EKBI", 55.74, 9.15), ("ELLX", 49.63, 6.21), ("ENBR", 60.29, 5.22),
        ("ENGM", 60.19, 11.10), ("ESKN", 58.79, 16.91), ("EPKK", 50.08, 19.78), ("EPWA", 52.17, 20.97),
        ("EVRA", 56.92, 23.97), ("EYVI", 54.63, 25.29), ("LBSF", 42.70, 23.41), ("LDZA", 45.74, 16.07),
        ("LEAL", 38.28, -0.56), ("LEBB", 43.30, -2.91), ("LEMD", 40.47, -3.56), ("LEPA", 39.55, 2.74),
        ("LFBD", 44.83, -0.72), ("LFLY", 45.73, 5.08), ("LFMD", 43.54, 6.95), ("LFPG", 49.01, 2.55),
        ("LGAV", 37.94, 23.94), ("LHBP", 47.44, 19.26), ("LICJ", 38.18, 13.10), ("LIMC", 45.63, 8.72),
        ("LIRA", 41.80, 12.59), ("LOWI", 47.26, 11.34), ("LOWL", 48.23, 14.19), ("LOWW", 48.11, 16.57),
        ("LPPT", 38.78, -9.14), ("LQSA", 43.82, 18.33), ("LSGG", 46.24, 6.11), ("LSZH", 47.46, 8.55),
        ("LTAC", 40.13, 32.99), ("LWSK", 41.96, 21.62), ("LYBE", 44.82, 20.31), ("LYTV", 42.40, 18.72),
        ("LZIB", 48.17, 17.21), ("UKKK", 50.40, 30.45), ("ULLI", 59.80, 30.26), ("UMKK", 54.89, 20.59),
        ("UMMM", 53.88, 28.03), ("UUEE", 55.97, 37.41), ("USSS", 56.74, 60.80),
    ];
    let dir = std::path::Path::new(map).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let tz = omsi_cfg::resolve_path(root, &format!("{dir}/timezone.txt"));
    omsi_map::TimeZone::load(&tz)
        .ok()
        .and_then(|t| t.lat_lon())
        .and_then(|(lat, lon)| AIRPORTS.iter().map(|(c, a, o)| (c, (a - lat).powi(2) + ((o - lon) * lat.to_radians().cos()).powi(2))).min_by(|x, y| x.1.total_cmp(&y.1)).map(|x| x.0.to_string()))
        .unwrap_or_else(|| "EDDB".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts our page shows (`launcher/ui/js/omsihub`): what it hands to `tr`, and the
    /// words of the controls it names (`label`, `title`, `main`, `placeholder`, `help`, `text`).
    fn page_texts() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../launcher/ui/js/omsihub");
        let mut files = vec![];
        let mut todo = vec![dir];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    todo.push(p);
                } else if p.extension().is_some_and(|x| x == "js") && !p.ends_with("symbols.js") && !p.ends_with("logo.js") && !p.ends_with("selftest.js") {
                    files.push(p);
                }
            }
        }
        let mut out = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            // (`tr('…'` / `tr("…"`, and `label: '…'`, `label="…"` ...)
            for lead in ["tr(", "label: ", "title: ", "help: ", "text: ", "label=", "title=", "main=", "placeholder=", "sub="] {
                let mut rest = src.as_str();
                while let Some(k) = rest.find(lead) {
                    let after = &rest[k + lead.len()..];
                    rest = after;
                    let Some(q) = after.chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
                    let body = &after[1..];
                    let mut text = String::new();
                    let mut chars = body.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => text.extend(chars.next()),
                            c if c == q => break,
                            c => text.push(c),
                        }
                    }
                    if text.chars().filter(|c| c.is_alphabetic()).count() >= 2 && !text.contains("${") {
                        out.push((name.clone(), text));
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn every_text_of_our_page_is_translated() {
        let texts = page_texts();
        assert!(texts.len() > 120, "only {} texts found: is the reading of the page broken?", texts.len());
        // (names and words that are the same in every language)
        let as_is = ["openOMSI", "Omsi-Hub", "km"];
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for (file, t) in &texts {
                if !as_is.contains(&t.as_str()) && !table.contains_key(t) {
                    missing.push(format!("{file} {lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} translations missing (add them to crates/omsi-app/locales/tauri-omsihub.yml):\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn the_page_has_every_table_of_the_game() {
        // (the game reads every file in the folder: a new one is the page's too)
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../omsi-app/locales");
        let mut files: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".yml") && n != "app.yml").collect();
        files.sort();
        let mut ours: Vec<String> = TABLES.iter().map(|(n, _)| n.to_string()).collect();
        ours.sort();
        assert_eq!(files, ours, "crates/omsi-launcher-api/src/omsihub.rs: TABLES lists every locales/*.yml besides app.yml");
    }

    #[test]
    fn our_texts_reach_the_page_and_upstreams_win() {
        // a text only our tables have, in Dutch
        let nl = crate::i18n::table("nl");
        assert!(nl.contains_key("Good afternoon %{name}"));
        assert!(nl.contains_key("How do you want to drive today?"));
        // (a text of the page's own table)
        assert!(nl.contains_key("Bus drivers"));
        // upstream's translation of a text both have stays upstream's
        let parse = |t: &str| -> Tables {
            let mut m = Tables::new();
            for (k, v) in [("Settings", t)] {
                m.entry("nl".into()).or_default().insert(k.into(), v.into());
            }
            m
        };
        let base = parse("Instellingen");
        let merged = with_ours(base, |_| {
            let mut m = Tables::new();
            m.entry("nl".into()).or_default().insert("Settings".into(), "Andere".into());
            m.entry("nl".into()).or_default().insert("Only ours".into(), "Alleen van ons".into());
            m
        });
        assert_eq!(merged["nl"]["Settings"], "Instellingen");
        assert_eq!(merged["nl"]["Only ours"], "Alleen van ons");
    }

    #[test]
    fn a_year_in_a_name_and_a_map_in_a_situation() {
        assert_eq!(year_in_name("Bad_Huegelsdorf_2020"), Some(2020));
        assert_eq!(year_in_name("Hamburg109"), None);
        assert_eq!(year_in_name("Vienna_2005_Line_24A"), Some(2005));
        assert_eq!(map_folder("maps\\Grundorf\\global.cfg"), Some("Grundorf".into()));
        // (the facts as the game-drawn launcher keeps them in the content index)
        assert_eq!(serde_json::to_value(Facts { tours: 2, year: Some(1991) }).unwrap(), json!({ "tours": 2, "year": 1991 }));
    }

    #[test]
    fn the_dayparts_are_the_game_drawn_launchers() {
        assert_eq!(DAYPARTS.len(), 5);
        assert_eq!(DAYPARTS[0], ("All day", None));
        assert_eq!(DAYPARTS[4].1, Some((22 * 60, 30 * 60)));
    }

    #[test]
    fn a_shift_read_back_from_its_legs() {
        let trip = |index: usize, dep: f64, arr: f64, from: &str, to: &str| core::TripInfo { name: format!("t{index}"), index, line: "5".into(), from: from.into(), terminus: to.into(), departure: dep, arrival: arr, stops: Vec::new(), km: 0.0 };
        let tour = core::TourInfo { number: "2".into(), ai_group: String::new(), first: 3600.0, last: 9000.0, days: "daily".into(), runs: true, next_run: None, trips: vec![trip(1, 3600.0, 5400.0, "A", "B"), trip(2, 5700.0, 7500.0, "B", "A"), trip(3, 7800.0, 9000.0, "A", "B")] };
        let lines = vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: Vec::new(), tours: vec![tour] }];
        let d = shift_of_legs(&["Linie5|2|1|2".to_string()], &lines).unwrap();
        assert_eq!(d.legs.len(), 2);
        assert_eq!(d.start(), 3600.0);
        assert_eq!(d.end(), 7500.0);
        let v = shift_json(&d, &lines);
        assert_eq!(v["key"], json!(["Linie5|2|1|2"]));
        assert_eq!(v["trips"][1]["pause"], json!(300.0));
        // (a later trip of the same tour is no change of tour)
        assert_eq!(v["trips"][1]["takes_over"], Value::Null);
        // a trip that is not in the timetable: no shift
        assert!(shift_of_legs(&["Linie5|2|2|5".to_string()], &lines).is_none());
    }
}

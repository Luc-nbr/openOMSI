//! The Timetable page (`crates/omsi-launcher-ui/src/timetable.rs`): a map's lines, their tours
//! and the trips each tour runs - the part of OMSI 2's timetable editor a driver uses to change
//! when buses go. A line is saved as its `.ttl`; a map of the original installation gets its
//! `TTData` copied into the content folder first (`core::ttstore`). Trips (`.ttp`) and their
//! tracks are the map maker's and are only chosen here.
//!
//! The page keeps the lines it shows (as `tt_load` gave them) and what is typed; the tours are
//! made, copied and repeated here (`tt_edit`), saved (`tt_save`) and put back (`tt_reset`).
//! A line travels as `{ path, name, user_allowed, priority, tours: [{ number, ai_group, extra,
//! trips: [{ trip, profile, departure }] }] }`, a departure in minutes after midnight.

use super::{arg, err, global_of};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_timetable::{Line, TimetableData, Tour, TourTrip};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "tt_load" => (|| load(&arg::<String>(args, "map")?))(),
        "tt_save" => (|| save(&arg::<String>(args, "map")?, &arg::<Vec<Value>>(args, "lines")?))(),
        "tt_reset" => (|| reset(&arg::<String>(args, "map")?))(),
        "tt_edit" => (|| edit(args))(),
        _ => return None,
    })
}

// --- a line as JSON -----------------------------------------------------------------------

fn line_json(l: &Line) -> Value {
    json!({
        "path": l.path.to_string_lossy(),
        "name": l.name,
        "user_allowed": l.user_allowed,
        "priority": l.priority,
        "own": core::lines::is_own_file(&l.name),
        "tours": l.tours.iter().map(tour_json).collect::<Vec<_>>(),
    })
}

fn tour_json(t: &Tour) -> Value {
    json!({
        "number": t.number, "ai_group": t.ai_group, "extra": t.extra,
        "trips": t.trips.iter().map(|x| json!({ "trip": x.trip, "profile": x.profile, "departure": x.departure })).collect::<Vec<_>>(),
    })
}

fn line_of(v: &Value) -> Line {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Line {
        path: PathBuf::from(s(v, "path")),
        name: s(v, "name"),
        user_allowed: v.get("user_allowed").and_then(Value::as_bool).unwrap_or(true),
        priority: v.get("priority").and_then(Value::as_i64).unwrap_or(0) as i32,
        tours: v.get("tours").and_then(Value::as_array).map(|a| a.iter().map(tour_of).collect()).unwrap_or_default(),
    }
}

fn tour_of(v: &Value) -> Tour {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Tour {
        number: s(v, "number"),
        ai_group: s(v, "ai_group"),
        extra: s(v, "extra"),
        trips: v
            .get("trips")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|x| TourTrip { trip: s(x, "trip"), profile: x.get("profile").and_then(Value::as_i64).unwrap_or(0) as i32, departure: x.get("departure").and_then(Value::as_f64).unwrap_or(0.0) as f32 }).collect())
            .unwrap_or_default(),
    }
}

/// The map's folder (where its `global.cfg` lies).
fn map_dir(file: &str) -> PathBuf {
    global_of(file).parent().map(Path::to_path_buf).unwrap_or_default()
}

fn folder_of(dir: &Path) -> String {
    dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
}

// --- reading, saving, resetting -----------------------------------------------------------

/// The map's timetable: its lines (by name, case aside) and the trips a tour can run - each
/// with its line, where it goes and its profiles (`["0"]` for a trip without any).
fn load(map: &str) -> Answer {
    let dir = map_dir(map);
    let mut d = TimetableData::load(&dir);
    d.lines.sort_by_key(|x| x.name.to_lowercase());
    let trips: Vec<Value> = d
        .trips
        .iter()
        .map(|t| {
            let profiles: Vec<String> = t.profiles.iter().map(|p| p.name.clone()).collect();
            json!({ "name": t.name, "line": t.line, "terminus": t.terminus, "profiles": if profiles.is_empty() { vec!["0".to_string()] } else { profiles } })
        })
        .collect();
    Ok(json!({ "dir": dir.to_string_lossy(), "folder": folder_of(&dir), "lines": d.lines.iter().map(line_json).collect::<Vec<_>>(), "trips": trips }))
}

/// Where a map's line is written (`core::ttstore::save_target`).
fn save_target(line: &Line, map_folder: &str, original_ttdata: &Path) -> Result<PathBuf, String> {
    let content = core::content_dir().ok_or("no content folder")?;
    let file = line.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| format!("{}.ttl", line.name));
    core::ttstore::save_target(&content, &line.path, &file, map_folder, original_ttdata)
}

/// Save the changed lines `lines` of the map (each tour's trips put in the order they leave):
/// `{ saved, error, lines: [{ name, path }] }` - the first error, and where each went.
fn save(map: &str, lines: &[Value]) -> Answer {
    let dir = map_dir(map);
    let folder = folder_of(&dir);
    let original = omsi_cfg::resolve_path(&dir, "TTData");
    let mut saved = 0;
    let mut error: Option<String> = None;
    let mut out = Vec::new();
    for v in lines {
        let mut line = line_of(v);
        for t in &mut line.tours {
            t.trips.sort_by(|a, b| a.departure.total_cmp(&b.departure));
        }
        match save_target(&line, &folder, &original).and_then(|p| line.save(&p).map(|_| p).map_err(|e| e.to_string())) {
            Ok(p) => {
                saved += 1;
                out.push(json!({ "name": line.name, "path": p.to_string_lossy(), "line": line_json(&Line { path: p.clone(), ..line.clone() }) }));
            }
            Err(e) => {
                error.get_or_insert(format!("{}: {e}", line.name));
            }
        }
    }
    if saved > 0 {
        omsi_cfg::content_changed();
    }
    Ok(json!({ "saved": saved, "error": error, "lines": out }))
}

/// The map's own timetable back (`core::ttstore::reset_timetable`), with the player's own lines
/// of the line editor written into it again from their registry: `{ done, own, own_error }` -
/// what was done, how many own lines were written again, and why they could not be.
fn reset(map: &str) -> Answer {
    let dir = map_dir(map);
    let folder = folder_of(&dir);
    let content = core::content_dir().ok_or("no content folder")?;
    let done = core::ttstore::reset_timetable(&content, &dir, &folder)?;
    omsi_cfg::content_changed();
    let reg = core::lines::load_registry(&core::lines::registry_path(&folder));
    if reg.lines.is_empty() {
        return Ok(json!({ "done": done, "own": 0, "own_error": null }));
    }
    Ok(match core::lines::export_to_map(&content, &dir, &reg) {
        Ok((n, _)) => json!({ "done": done, "own": n, "own_error": null }),
        Err(e) => json!({ "done": done, "own": 0, "own_error": err(e) }),
    })
}

// --- the tours: made, copied, repeated ------------------------------------------------------

/// A new tour's number: one past the highest that is a number.
pub fn next_number(tours: &[Tour]) -> String {
    (tours.iter().filter_map(|t| t.number.trim().parse::<i64>().ok()).max().unwrap_or(0) + 1).to_string()
}

/// Copies of `base` every `every` minutes after it, as long as the copy's first departure is
/// not after `until` (minutes of the day); numbered on from the highest. Returns how many.
pub fn repeat_tour(tours: &mut Vec<Tour>, base: &Tour, every: f32, until: f32) -> usize {
    let first = base.trips.first().map(|t| t.departure).unwrap_or(0.0);
    let mut made = 0;
    let mut k = 1.0;
    while first + every * k <= until + 1e-3 && made < 500 {
        let mut t = base.clone();
        for x in &mut t.trips {
            x.departure += every * k;
        }
        t.number = next_number(tours);
        tours.push(t);
        made += 1;
        k += 1.0;
    }
    made
}

/// The next trip of a tour (`timetable::draw`'s "Add trip"): the first of the line's trips in
/// an empty tour at six, the first trip again half an hour after it, then the alternation
/// (there and back) as far after the last as the last gap.
pub fn next_trip(tour: &Tour, first_trip: &str) -> TourTrip {
    let n = tour.trips.len();
    match n {
        0 => TourTrip { trip: first_trip.to_string(), profile: 0, departure: 6.0 * 60.0 },
        1 => TourTrip { departure: tour.trips[0].departure + 30.0, ..tour.trips[0].clone() },
        _ => TourTrip { departure: tour.trips[n - 1].departure + (tour.trips[n - 1].departure - tour.trips[n - 2].departure).max(1.0), ..tour.trips[n - 2].clone() },
    }
}

/// A change of a line's tours: `op` is `new_tour` (its first trip `first`, at six, with the
/// last tour's depot group), `copy` (the tour `tour` `offset` minutes later), `repeat` (copies
/// every `offset` minutes up to `until`), `delete_tour`, `add_trip`, `shift` (every trip of the
/// tour `offset` minutes). Answers `{ line, tour, made }`: the line changed, the tour to show,
/// how many tours were made.
fn edit(args: &Value) -> Answer {
    let mut line = line_of(&arg::<Value>(args, "line")?);
    let op = arg::<String>(args, "op")?;
    let mut at = arg::<Option<usize>>(args, "tour")?.unwrap_or(0);
    let offset = arg::<Option<f32>>(args, "offset")?.unwrap_or(0.0);
    let first = arg::<Option<String>>(args, "first")?.unwrap_or_default();
    let mut made = 0;
    match op.as_str() {
        "new_tour" => {
            let ai_group = line.tours.last().map(|t| t.ai_group.clone()).unwrap_or_else(|| "Busses".into());
            let extra = line.tours.last().map(|t| t.extra.clone()).unwrap_or_default();
            let number = next_number(&line.tours);
            line.tours.push(Tour { number, ai_group, extra, trips: vec![TourTrip { trip: first, profile: 0, departure: 6.0 * 60.0 }] });
            at = line.tours.len() - 1;
            made = 1;
        }
        "copy" => {
            if let Some(mut t) = line.tours.get(at).cloned() {
                for x in &mut t.trips {
                    x.departure += offset;
                }
                t.number = next_number(&line.tours);
                line.tours.push(t);
                at = line.tours.len() - 1;
                made = 1;
            }
        }
        "repeat" => {
            let until = arg::<f32>(args, "until")?;
            if let Some(base) = line.tours.get(at).cloned() {
                made = repeat_tour(&mut line.tours, &base, offset, until);
                if made > 0 {
                    at = line.tours.len() - 1;
                }
            }
        }
        "delete_tour" => {
            if at < line.tours.len() {
                line.tours.remove(at);
                at = at.saturating_sub(1);
            }
        }
        "add_trip" => {
            if let Some(t) = line.tours.get_mut(at) {
                let next = next_trip(t, &first);
                t.trips.push(next);
            }
        }
        "shift" => {
            if let Some(t) = line.tours.get_mut(at) {
                for x in &mut t.trips {
                    x.departure += offset;
                }
            }
        }
        other => return Err(format!("unknown change {other}")),
    }
    let at = at.min(line.tours.len().saturating_sub(1));
    Ok(json!({ "line": line_json(&line), "tour": at, "made": made }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tour(n: &str, deps: &[f32]) -> Tour {
        Tour { number: n.into(), ai_group: "Busses".into(), extra: String::new(), trips: deps.iter().enumerate().map(|(k, d)| TourTrip { trip: if k % 2 == 0 { "a".into() } else { "b".into() }, profile: 0, departure: *d }).collect() }
    }

    #[test]
    fn numbers_and_repeats_as_the_game_drawn_launcher_makes_them() {
        assert_eq!(next_number(&[tour("1", &[]), tour("7", &[]), tour("x", &[])]), "8");
        assert_eq!(next_number(&[]), "1");
        let base = tour("1", &[360.0, 390.0]);
        let mut tours = vec![base.clone()];
        // 6:20, 6:40 … 8:00
        assert_eq!(repeat_tour(&mut tours, &base, 20.0, 480.0), 6);
        assert_eq!(tours.last().unwrap().trips[0].departure, 480.0);
        assert_eq!(tours.last().unwrap().number, "7");
    }

    #[test]
    fn a_trip_added_goes_on_with_the_alternation() {
        assert_eq!(next_trip(&tour("1", &[]), "x").departure, 360.0);
        assert_eq!(next_trip(&tour("1", &[400.0]), "x").departure, 430.0);
        let t = next_trip(&tour("1", &[400.0, 430.0, 455.0]), "x");
        assert_eq!((t.trip.as_str(), t.departure), ("b", 480.0));
    }

    #[test]
    fn a_line_goes_to_the_page_and_back_whole() {
        let l = Line { path: PathBuf::from("maps/X/TTData/5.ttl"), name: "5".into(), user_allowed: true, priority: 2, tours: vec![tour("3", &[360.0, 390.5])] };
        let back = line_of(&line_json(&l));
        assert_eq!(back.name, "5");
        assert_eq!(back.priority, 2);
        assert_eq!(back.path, l.path);
        assert_eq!(back.tours[0].trips[1].departure, 390.5);
        assert_eq!(back.tours[0].trips[1].trip, "b");
        // the page's change: a tour every twenty minutes, the last one shown
        let v = edit(&json!({ "line": line_json(&l), "op": "repeat", "tour": 0, "offset": 20.0, "until": 400.0 })).unwrap();
        assert_eq!(v["made"], json!(2));
        assert_eq!(v["tour"], json!(2));
        assert_eq!(v["line"]["tours"][2]["number"], json!("5"));
    }
}

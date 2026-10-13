//! Omsi-Hub's fleet map for the desktop launcher (Trello "Tauri 3/6"): the map as `export` gives
//! it to another front end - the roads the game's city map draws, thinned, in metres from the
//! map's middle - and the routes of every trip of the company's lines today as the game-drawn
//! launcher's fleet map has them (`MapView::trip_tracks`): each trip's route as one line, the
//! first lane of each of its road pieces end to end, and where each of its stops stands.
//! The page moves the company's buses along these (`company::fleetmap`).
//!
//! A child of `mapview` (one line there), so that it reads the map with the same private
//! pieces; the launcher service asks for it with a "map" request that names `trips`.

use super::{read_map, simplify, Look};
use glam::{DVec2, DVec3, Vec3};
use std::path::Path;

/// The map and the routes of `trips` on `date`, for the desktop launcher's fleet map: `export`'s
/// `roads`, `lo`, `hi`, `origin` and `colors`, and `tracks` - for each trip its `name`, its route
/// `p` (flat `[x0, y0, ...]` in metres from `origin`, to the decimetre, a little thinned) and
/// its `stops` (`[x, y]`, or null where the map lacks the stop). The map is read once.
pub fn export(map: &str, global: &Path, date: &str, trips: &[String], tolerance: f32) -> serde_json::Value {
    use serde_json::json;
    let look = Look { map: map.to_string(), global: global.to_path_buf(), date: date.to_string(), trips: trips.to_vec(), entry: -1 };
    let roads = read_map(&look);
    let o = roads.origin;
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let flat = |pts: &[DVec3], tol: f32| -> Vec<f64> {
        let v: Vec<Vec3> = pts.iter().map(|q| Vec3::new((q.x - o.x) as f32, (q.y - o.y) as f32, 0.0)).collect();
        simplify(&v, tol).iter().flat_map(|q| [r1(q.x as f64), r1(q.y as f64)]).collect()
    };
    // the roads, as `export` has them
    let list: Vec<serde_json::Value> = roads.roads.iter().map(|r| json!({ "w": (r.width * 10.0).round() / 10.0, "main": r.main, "p": flat(&r.points, tolerance) })).collect();
    let rel = |q: DVec2| [r1(q.x - o.x), r1(q.y - o.y)];
    // the trips' routes and stops
    let mut tracks = Vec::new();
    for name in trips {
        if name.trim().is_empty() {
            continue;
        }
        let path = match omsi_launcher_lib::trip_path(map, date, name) {
            Ok(p) => p,
            Err(e) => {
                log::debug!("trip path for {name}: {e}");
                tracks.push(json!({ "name": name, "p": [], "stops": [] }));
                continue;
            }
        };
        // (the first lane of each piece, end to end: `trip_tracks`)
        let mut pts: Vec<DVec3> = Vec::new();
        for piece in &path.route {
            let Some(lane) = roads.routes_of(piece).first().and_then(|i| roads.net.lanes.get(*i)) else { continue };
            for q in &lane.points {
                let p = DVec3::new(q.x, q.y, 0.0);
                if pts.last().is_none_or(|x| (x.truncate() - p.truncate()).length() > 0.05) {
                    pts.push(p);
                }
            }
        }
        let stops: Vec<serde_json::Value> = path.stops.iter().map(|id| roads.objects.get(id).map(|q| json!(rel(*q))).unwrap_or(serde_json::Value::Null)).collect();
        // (a hair thinner than the roads: the buses ride on it, measured along it)
        tracks.push(json!({ "name": name, "p": flat(&pts, tolerance * 0.5), "stops": stops }));
    }
    let css = |c: omsi_ui::Color| format!("rgba({},{},{},{:.3})", (c.0[0] * 255.0).round(), (c.0[1] * 255.0).round(), (c.0[2] * 255.0).round(), c.0[3]);
    let [casing, road, main, red] = crate::host::host().city_map_colors();
    json!({
        "origin": [o.x, o.y],
        "lo": rel(roads.lo), "hi": rel(roads.hi),
        "roads": list, "tracks": tracks,
        "colors": [css(casing), css(road), css(main), css(red)],
    })
}

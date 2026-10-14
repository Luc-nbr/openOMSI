//! RosterFlow's relief points in the page (Luc 2026-10-14, the core's `rosterflow::ReliefPoint`
//! and `studio::cut_mid`): Preferences › Relief Points and Duty Travel's lists, and on the board
//! the trips cut at a relief point on the way and the stops on the way a trip can be cut at.

use super::super::company::words::{tr, trf};
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, PointKind};
use omsi_launcher_lib::company::studio as st;
use omsi_launcher_lib::company::Company;
use omsi_launcher_lib::LineInfo;
use serde_json::{json, Value};

/// The relief points as the Preferences list them (`rosterflow::relief_points`): each stop, its
/// kind (0 the depot, 1 a terminus, 2 a stop on the way), the lines it is of and its settings;
/// and the stops of each line a stop on the way can be picked of.
pub(super) fn prefs_points(c: &Company, lines: &[LineInfo]) -> Value {
    let points: Vec<Value> = rf::relief_points(c, lines)
        .into_iter()
        .map(|r| {
            let kind = match r.kind {
                PointKind::Depot => 0,
                PointKind::Terminus => 1,
                PointKind::OnTheWay => 2,
            };
            let name = if kind == 0 { tr("Depot") } else { r.stop.clone() };
            json!({ "stop": r.stop, "name": name, "kind": kind, "lines": r.lines.join(", "), "point": r.point })
        })
        .collect();
    let stops: Vec<Value> = rf::line_stops(c, lines).into_iter().map(|(line, number, stops)| json!({ "line": line, "number": number, "stops": stops })).collect();
    json!({
        "points": points,
        "line_stops": stops,
        "kinds": [tr("Depot"), tr("Terminus"), tr("On the way")],
    })
}

/// The board's trips (`json`, `board_json`'s) told what the Gantt draws of a relief on the way:
/// a part of a trip cut at a relief point (`part` 1 the first, 2 the second, `at` the stop),
/// and on a whole trip the stops on the way a duty can be cut at, or why not (`mids`: stop,
/// name, minute, whether it can, the tip). `c` the company as the studio sees it.
pub(super) fn add_mids(json: &mut Value, c: &Company, board: &Board, lines: &[LineInfo]) {
    let wd = board.weekday;
    for (i, t) in board.tours.iter().enumerate() {
        for (j, x) in t.trips.iter().enumerate() {
            let Some(p) = &x.part else { continue };
            let Some(v) = json.get_mut("blocks").and_then(|b| b.get_mut(i)).and_then(|b| b.get_mut("trips")).and_then(|t| t.get_mut(j)) else { continue };
            v["part"] = json!(if p.second { 2 } else { 1 });
            v["at"] = json!(p.at);
            v["linked"] = json!(x.counts() && !rf::linked(c, wd, &t.line, &st::whole_ref(x)).is_empty());
        }
    }
    for (i, j, mids) in st::day_mid_stops(c, &board.tours, lines) {
        let Some(v) = json.get_mut("blocks").and_then(|b| b.get_mut(i)).and_then(|b| b.get_mut("trips")).and_then(|t| t.get_mut(j)) else { continue };
        let mids: Vec<Value> = mids
            .into_iter()
            .map(|(stop, name, at, ok)| {
                let tip = match ok {
                    Ok(()) => trf("Click to cut the duty here, on the way: a driver takes over at %{stop} (%{time}).", &[("stop", &name), ("time", &rf::clock(at))]),
                    Err(why) => format!("{} ({})", tr(why), name),
                };
                json!([stop, name, at, ok.is_ok(), tip])
            })
            .collect();
        v["mids"] = json!(mids);
    }
}

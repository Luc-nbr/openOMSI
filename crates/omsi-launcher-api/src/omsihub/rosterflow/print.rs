//! RosterFlow's prints (`crates/omsi-launcher-ui/src/company/planning/rf_print.rs`): the bus
//! graph (a row per vehicle: its trips as bars in the line's colour, the pull-outs and pull-ins
//! black, its duties as a band under it) and the duty graph (a row per duty: its signing, trips,
//! travel and custom work, its vehicles under it), laid out as Optibus's "Bus Graph"
//! (`co::rosterpdf`), saved where the player chooses (a folder given in `OMSI_ROSTERFLOW_PDF`
//! instead: the screenshot runs check the print so); the page shows its folder then.

use super::super::company::words::tr;
use super::super::company::Session;
use super::{block_label, date_of, ensure, outbound, with_rf, Rf, DAY_NAMES};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::network::TourOfDay;
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, EvKind};
use omsi_launcher_lib::company::rosterpdf::{self as pdf, GraphItem, GraphRow, ItemKind};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::path::PathBuf;

/// A block's trips as a graph's items.
fn block_items(c: &Company, board: &Board, t: &TourOfDay, range: std::ops::Range<usize>) -> Vec<GraphItem> {
    let colour = rf::line_colour(c, &t.line);
    let n = t.trips.len();
    let mut out = Vec::new();
    for j in range {
        let x = &t.trips[j];
        let kind = if x.empty && (j == 0 || j + 1 == n) {
            ItemKind::Pull
        } else if x.empty || !x.counts() {
            ItemKind::Deadhead
        } else {
            ItemKind::Trip { inbound: !outbound(&board.tours, &t.line, x) }
        };
        let label = if x.line.trim().is_empty() { t.number.trim().to_string() } else { x.line.trim().to_string() };
        out.push(GraphItem { from: x.dep, to: x.arr, kind, label, colour, start_stop: pdf::stop_letters(&x.from), end_stop: pdf::stop_letters(x.end()) });
    }
    out
}

/// The bus graph's rows: a vehicle each, its duties under it.
pub(crate) fn bus_rows(c: &Company, board: &Board) -> Vec<GraphRow> {
    let line_at = |l: &str| c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(l)).unwrap_or(usize::MAX);
    let mut idx: Vec<usize> = (0..board.tours.len()).collect();
    idx.sort_by_key(|&i| (line_at(&board.tours[i].line), board.tours[i].from(), i));
    idx.into_iter()
        .map(|i| {
            let t = &board.tours[i];
            let under = co::network::duties_of(t)
                .iter()
                .enumerate()
                .map(|(k, r)| {
                    let (a, b) = rf::span_of(&c.planning.flow.prefs, t, r);
                    let label = co::studio::duty_of(&board.duties, i, k).and_then(|di| board.shaped.iter().position(|s| s.duty == Some(di))).map(|row| board.shaped[row].label(row)).unwrap_or_default();
                    (a, b, label)
                })
                .collect();
            GraphRow { name: block_label(t), sub: tr("Line %{n}").replace("%{n}", &t.number), sub2: format!("({:.1} km)", t.km()), items: block_items(c, board, t, 0..t.trips.len()), under }
        })
        .collect()
}

/// The duty graph's rows: a duty each, its vehicles under it.
pub(crate) fn duty_rows(c: &Company, board: &Board) -> Vec<GraphRow> {
    let p = &c.planning.flow.prefs;
    board
        .shaped
        .iter()
        .enumerate()
        .map(|(row, s)| {
            let mut items: Vec<GraphItem> = Vec::new();
            for e in &s.events {
                let (kind, label) = match e.kind {
                    EvKind::SignOn | EvKind::SignOff => (ItemKind::Sign, String::new()),
                    EvKind::Travel(m) => (ItemKind::Travel, m.letter().to_string()),
                    EvKind::Custom(k) => (ItemKind::Custom, s.custom.get(k).map(|x| tr(&x.kind)).unwrap_or_default()),
                };
                items.push(GraphItem { from: e.from, to: e.to, kind, label, colour: [255, 255, 255], start_stop: String::new(), end_stop: String::new() });
            }
            let mut under = Vec::new();
            for sp in &s.spans {
                let t = &board.tours[sp.tour];
                let r = co::network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
                items.extend(block_items(c, board, t, r));
                under.push((sp.from, sp.to, block_label(t)));
            }
            items.sort_by_key(|x| x.from);
            GraphRow { name: s.label(row), sub: tr(s.kind_name(p)), sub2: format!("({} {})", tr("paid"), rf::hm(s.paid)), items, under }
        })
        .collect()
}

/// The PDF of the bus graph (`bus`) or the duty graph of `date`.
pub(crate) fn pdf_of(c: &Company, board: &Board, date: &str, bus: bool) -> Vec<u8> {
    let rows = if bus { bus_rows(c, board) } else { duty_rows(c, board) };
    let title = tr(if bus { "Bus Graph" } else { "Duty Graph" });
    let info = vec![
        tr("Depot: %{d}").replace("%{d}", c.depot.trim_end_matches(".hof")),
        tr("Service: %{s}").replace("%{s}", &tr(DAY_NAMES[board.weekday as usize % 7])),
        tr("Date: %{d}").replace("%{d}", date),
    ];
    let printed = format!("{}: {}  ·  {}", tr("Printed"), c.date, c.name);
    pdf::graph(&title, &info, &printed, &tr("Page"), &rows)
}

/// The day shown as a PDF file: its file name and its bytes, or what to say instead.
fn made(s: &mut Session, r: &mut Rf, date: &str, bus: bool) -> Result<(String, Vec<u8>), Value> {
    let k = match ensure(s, r, date) {
        Some(Ok(k)) => k,
        Some(Err(e)) => return Err(json!({ "say": { "text": e, "error": true } })),
        None => return Err(json!({ "say": { "text": tr("Reading the timetable…"), "error": false } })),
    };
    let name = format!("{}-{}.pdf", if bus { "bus-graph" } else { "duty-graph" }, date);
    Ok((name, pdf_of(&k.view, &k.board, date, bus)))
}

/// Print the day shown (`rf_print`, `day` and `bus`): saved where the player chooses, as the
/// game-drawn launcher saves it (a folder given in `OMSI_ROSTERFLOW_PDF` takes the file without
/// asking: the screenshot runs and the checks print so). The dialog is shown with the company
/// let go, so that its clock and the page go on meanwhile; the page shows the folder then.
pub(crate) fn print(args: &Value) -> Answer {
    let bus = args.get("bus").and_then(Value::as_bool).unwrap_or(true);
    let made = with_rf(|s, r| {
        let date = date_of(s, args).ok_or("no day")?;
        Ok::<_, String>(made(s, r, &date, bus))
    })?;
    match made {
        Ok((name, bytes)) => Ok(save(&name, &bytes)),
        Err(said) => Ok(said),
    }
}

/// The day shown as a PDF file for a player of the company's session (`rf_print` relayed): its
/// file name and its bytes (base64), saved on his own launcher (`save`).
pub(super) fn for_afar(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let bus = args.get("bus").and_then(Value::as_bool).unwrap_or(true);
    let date = date_of(s, args).ok_or("no day")?;
    Ok(match made(s, r, &date, bus) {
        Ok((name, bytes)) => json!({ "pdf": crate::base64(&bytes), "name": name }),
        Err(said) => said,
    })
}

/// A graph's PDF file saved where the player chooses (or in `OMSI_ROSTERFLOW_PDF`): what the
/// page shows then.
pub(crate) fn save(name: &str, bytes: &[u8]) -> Value {
    let given = std::env::var("OMSI_ROSTERFLOW_PDF").ok().filter(|d| !d.trim().is_empty());
    let path = match &given {
        Some(dir) => PathBuf::from(dir).join(name),
        None => match core::save_file_as(&tr("Save the graph as a PDF file"), name, "PDF", &["pdf"]) {
            Some(p) => p,
            None => return json!({}),
        },
    };
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&path, bytes));
    match written {
        Ok(()) => json!({
            "printed": path.to_string_lossy(),
            "folder": path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
            "reveal": given.is_none(),
            "say": { "text": tr("Saved: %{file}").replace("%{file}", &path.display().to_string()), "error": false },
        }),
        Err(e) => json!({ "say": { "text": format!("{}: {e}", tr("Could not save the file")), "error": true } }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::network::PlannedTrip;

    #[test]
    fn a_graph_is_a_pdf_with_a_row_for_every_bus() {
        let trip = |a: &str, b: &str, dep: i32| PlannedTrip { line: "4".into(), from: a.into(), to: b.into(), stops: 6, dep, arr: dep + 30, origin: "1".into(), ..Default::default() };
        let t = TourOfDay { line: "Linie4".into(), number: "4".into(), tour: "1".into(), trips: vec![trip("Nord", "Süd", 360), trip("Süd", "Nord", 400)], ..Default::default() };
        let c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Ziel".into(), date: "2024-05-06".into(), ..Default::default() }, "Luc");
        let b = rf::board_of(&c, 0, vec![t], Vec::new());
        assert_eq!(bus_rows(&c, &b).len(), 1);
        assert_eq!(bus_rows(&c, &b)[0].name, "4/01");
        let bytes = pdf_of(&c, &b, "2024-05-06", true);
        assert!(bytes.starts_with(b"%PDF"));
        assert!(!duty_rows(&c, &b).is_empty());
    }
}

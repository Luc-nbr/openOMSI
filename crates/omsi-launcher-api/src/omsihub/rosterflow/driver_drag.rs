//! Luc's driver drag in the Drivers tab (branch `rf-driver-drag`, merged into `beta` at
//! 1eba782b; the game-drawn launcher's `Hand::Duty` in `rf_gantt.rs`): single trips of a duty -
//! on one bus or several, the bus staying the same - go to another duty or to a new one. The
//! core works it out (`studio::plan_move_to_duty`, `move_to_duty`, `DutyTarget`); the zones are
//! rated here as the Vehicles tab's are (`zones_for`).
//!
//! Trips chosen on several rows (testers: "Multi-selected duties ... can be selected all
//! together, but not dragged up or down") go together: each duty's trips to the duty as many
//! rows up or down - in the rows as the page shows them - as the row taken hold of goes
//! (`studio::move_to_duties`). A drop the core refuses says why.

use super::super::company::words::{tr, trf};
use super::super::company::Session;
use super::{edit, edit_with, said, Rf};
use omsi_launcher_lib::company::network::TourOfDay;
use omsi_launcher_lib::company::rosterflow::Board;
use omsi_launcher_lib::company::studio::{self as st, DutyGroup, DutyTarget};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// The day's blocks of a company as it is (`network::tours_of_day` of the board's day).
pub(super) type Again<'a> = &'a dyn Fn(&Company) -> Vec<TourOfDay>;

/// The trips in hand by the row of their duty, the rows in the board's order.
type Groups = Vec<(usize, Vec<(usize, usize)>)>;

/// The duty (an index into `Board::duties`, as the core counts them) of the Drivers tab's row.
fn duty_of_row(board: &Board, row: usize) -> Option<usize> {
    board.shaped.get(row)?.duty
}

/// The label of duty `di` (an index into `Board::duties`): "1003".
fn duty_label(board: &Board, di: usize) -> String {
    board.shaped.iter().position(|s| s.duty == Some(di)).map(|row| board.shaped[row].label(row)).unwrap_or_default()
}

/// The trips in hand by the row of their duty (each trip's own duty, whatever row it was taken
/// from).
fn groups_of(board: &Board, trips: &[(usize, usize)]) -> Groups {
    let mut out: Groups = Vec::new();
    for &(i, j) in trips {
        let Some(di) = st::duty_of_trip(&board.duties, &board.tours, i, j) else { continue };
        let Some(row) = board.shaped.iter().position(|s| s.duty == Some(di)) else { continue };
        match out.iter_mut().find(|g| g.0 == row) {
            Some(g) => g.1.push((i, j)),
            None => out.push((row, vec![(i, j)])),
        }
    }
    out.sort_by_key(|g| g.0);
    out
}

/// The duty of trips of one duty in hand: their own, else the row's they were taken from.
fn one_from(board: &Board, groups: &Groups, row: usize) -> Option<usize> {
    groups.first().and_then(|g| duty_of_row(board, g.0)).or_else(|| duty_of_row(board, row))
}

/// The duties' rows as the page shows them (`rows`: sorted, filtered, pinned), the board's order
/// when none are given.
fn order_of(board: &Board, rows: &[usize]) -> Vec<usize> {
    if rows.is_empty() {
        (0..board.shaped.len()).collect()
    } else {
        rows.to_vec()
    }
}

/// Where each group goes when row `grabbed` is dropped on row `to`: as many rows on (`rows` as
/// the page shows them) as `to` is from `grabbed`.
fn targets(board: &Board, groups: &Groups, grabbed: usize, to: usize, rows: &[usize]) -> Result<Vec<DutyGroup>, &'static str> {
    let order = order_of(board, rows);
    let pos = |r: usize| order.iter().position(|&x| x == r);
    let grabbed = if groups.iter().any(|g| g.0 == grabbed) { grabbed } else { groups.first().map(|g| g.0).ok_or(st::NOT_FOUND)? };
    let (Some(a), Some(b)) = (pos(grabbed), pos(to)) else { return Err(st::NOT_FOUND) };
    let k = b as isize - a as isize;
    groups
        .iter()
        .map(|(row, trips)| {
            let p = pos(*row).ok_or(st::PAST_THE_ROWS)? as isize + k;
            let there = usize::try_from(p).ok().and_then(|p| order.get(p)).ok_or(st::PAST_THE_ROWS)?;
            let di = duty_of_row(board, *there).ok_or(st::NOT_A_DUTY)?;
            Ok(DutyGroup { trips: trips.clone(), to: DutyTarget::Duty(di) })
        })
        .collect()
}

/// Every group to a duty of its own.
fn to_new(groups: &Groups) -> Vec<DutyGroup> {
    groups.iter().map(|(_, trips)| DutyGroup { trips: trips.clone(), to: DutyTarget::New }).collect()
}

/// A refusal of several duties' trips, with the duty it is about (`groups`' row of `gi`).
fn refusal(board: &Board, groups: &Groups, (gi, why): (usize, &'static str)) -> String {
    match groups.get(gi) {
        Some((row, _)) => trf("Duty %{d}: %{why}", &[("d", &board.shaped[*row].label(*row)), ("why", &tr(why))]),
        None => tr(why),
    }
}

/// The drop zones of trips `trips` taken hold of on row `row`: every other duty they may go to,
/// and a new duty of their own, each tried on a copy of the company (`probe_with`). Trips of
/// several duties: each row they may all go that many rows up or down to (`targets`).
pub(super) fn zones(c: &Company, board: &Board, row: usize, trips: &[(usize, usize)], rows: &[usize], again: Again, probe_with: &mut dyn FnMut(Value, &dyn Fn(&mut Company) -> Result<(), &'static str>)) {
    let wd = board.weekday;
    let tours = &board.tours;
    let groups = groups_of(board, trips);
    if groups.len() > 1 {
        // (the group taken hold of tried alone first: most rows refuse it already)
        let mine = groups.iter().position(|g| g.0 == row).unwrap_or(0);
        let Some(from) = duty_of_row(board, groups[mine].0) else { return };
        for to in order_of(board, rows) {
            let Ok(gs) = targets(board, &groups, row, to, rows) else { continue };
            if st::plan_move_to_duty(c, wd, tours, from, &gs[mine].trips, gs[mine].to).is_err() {
                continue;
            }
            probe_with(json!({ "kind": "duty", "d": to }), &|p| st::move_to_duties(p, wd, tours, again, &gs).map_err(|e| e.1));
        }
        let gs = to_new(&groups);
        probe_with(json!({ "kind": "new_duty" }), &|p| st::move_to_duties(p, wd, tours, again, &gs).map_err(|e| e.1));
        return;
    }
    let Some(from) = one_from(board, &groups, row) else { return };
    for (d, s) in board.shaped.iter().enumerate() {
        let Some(di) = s.duty else { continue };
        if di == from || st::plan_move_to_duty(c, wd, tours, from, trips, DutyTarget::Duty(di)).is_err() {
            continue;
        }
        probe_with(json!({ "kind": "duty", "d": d }), &|p| st::move_to_duty(p, wd, tours, from, trips, DutyTarget::Duty(di)));
    }
    probe_with(json!({ "kind": "new_duty" }), &|p| st::move_to_duty(p, wd, tours, from, trips, DutyTarget::New));
}

/// Why trips `trips` taken hold of on row `row` do not go to row `to` (None: to a new duty -
/// asked when no zone is offered at all), translated; empty when they do.
pub(super) fn why(c: &Company, board: &Board, row: usize, trips: &[(usize, usize)], rows: &[usize], to: Option<usize>, again: Again) -> String {
    let wd = board.weekday;
    let groups = groups_of(board, trips);
    if groups.len() > 1 {
        let gs = match to {
            Some(to) => match targets(board, &groups, row, to, rows) {
                Ok(gs) => gs,
                Err(e) => return tr(e),
            },
            None => to_new(&groups),
        };
        let mut probe = c.clone();
        return match st::move_to_duties(&mut probe, wd, &board.tours, again, &gs) {
            Ok(()) => String::new(),
            Err(e) => refusal(board, &groups, e),
        };
    }
    let Some(from) = one_from(board, &groups, row) else { return String::new() };
    let target = match to {
        Some(to) => match duty_of_row(board, to) {
            Some(di) => DutyTarget::Duty(di),
            None => return tr(st::NOT_A_DUTY),
        },
        None => DutyTarget::New,
    };
    match st::plan_move_to_duty(c, wd, &board.tours, from, trips, target) {
        Err(e) => tr(e),
        Ok(_) => String::new(),
    }
}

/// The trips dropped at a zone (`kind` "duty" with its row `to`, or "new_duty"): the edit in
/// the draft, said in the history; a refusal said with why. `row` the row taken hold of, `rows`
/// the duties' rows as the page shows them (trips of several duties: `targets`).
#[allow(clippy::too_many_arguments)]
pub(super) fn drop(s: &mut Session, r: &mut Rf, board: &Board, row: usize, trips: Vec<(usize, usize)>, rows: &[usize], kind: &str, to: usize, again: Again) -> Value {
    let wd = board.weekday;
    let tours = board.tours.clone();
    let n = trips.len().to_string();
    let groups = groups_of(board, &trips);
    if groups.len() > 1 {
        let gs = match kind {
            "duty" => match targets(board, &groups, row, to, rows) {
                Ok(gs) => gs,
                Err(e) => return said(tr(e), true),
            },
            "new_duty" => to_new(&groups),
            _ => return json!({}),
        };
        let m = groups.len().to_string();
        let words = if kind == "duty" { trf("%{n} trips of %{m} duties moved to other duties", &[("n", &n), ("m", &m)]) } else { trf("%{n} trips of %{m} duties moved to new duties", &[("n", &n), ("m", &m)]) };
        return edit_with(s, r, words, |c| st::move_to_duties(c, wd, &tours, again, &gs).map_err(|e| refusal(board, &groups, e)));
    }
    let Some(from) = one_from(board, &groups, row) else { return json!({}) };
    let a = duty_label(board, from);
    match kind {
        "duty" => {
            let Some(di) = duty_of_row(board, to) else { return said(tr(st::NOT_A_DUTY), true) };
            let words = trf("%{n} trips moved from duty %{a} to duty %{b}", &[("n", &n), ("a", &a), ("b", &board.shaped[to].label(to))]);
            edit(s, r, words, |c| st::move_to_duty(c, wd, &tours, from, &trips, DutyTarget::Duty(di)))
        }
        "new_duty" => {
            let words = trf("%{n} trips moved from duty %{a} to a new duty", &[("n", &n), ("a", &a)]);
            edit(s, r, words, |c| st::move_to_duty(c, wd, &tours, from, &trips, DutyTarget::New))
        }
        _ => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::network::PlannedTrip;
    use omsi_launcher_lib::company::{self as co, rosterflow as rf};

    #[test]
    fn trips_of_several_rows_go_as_many_rows_on_as_the_page_shows_them() {
        let mut c = co::found(&co::Founding { name: "Drag".into(), game_mode: co::GameMode::Normal, date: "2024-05-06".into(), ..Default::default() }, "Luc");
        rf::add_spare(&mut c, 0, rf::CUSTOM_KINDS[0], 600, 900);
        let trip = |a: &str, b: &str, dep: i32| PlannedTrip { line: "4".into(), from: a.into(), to: b.into(), stops: 6, dep, arr: dep + 30, origin: "1".into(), name: format!("4 {dep}"), ..Default::default() };
        let tour = |no: &str, from: i32| TourOfDay { line: "Linie4".into(), number: "4".into(), tour: no.into(), trips: vec![trip("Nord", "Süd", from), trip("Süd", "Nord", from + 40)], ..Default::default() };
        let b = rf::board_of(&c, 0, vec![tour("1", 360), tour("2", 600), tour("3", 840)], Vec::new());
        // three duties a bus, the spare duty last
        assert_eq!(b.shaped.len(), 4);
        let row = |i: usize| b.shaped.iter().position(|s| s.duty.is_some() && s.duty == st::duty_of_trip(&b.duties, &b.tours, i, 0)).unwrap();
        let (r0, r1, r2) = (row(0), row(1), row(2));
        let spare = b.shaped.iter().position(|s| s.spare.is_some()).unwrap();
        // the trips by their duty, whatever row they were taken from
        let groups = groups_of(&b, &[(0, 1), (1, 0), (0, 0)]);
        assert_eq!(groups, vec![(r0, vec![(0, 1), (0, 0)]), (r1, vec![(1, 0)])]);
        let to = |gs: Vec<DutyGroup>| gs.iter().map(|g| g.to).collect::<Vec<_>>();
        let duty = |r: usize| DutyTarget::Duty(b.shaped[r].duty.unwrap());
        // the row taken hold of a row down: each of them a row down
        assert_eq!(targets(&b, &groups, r0, r1, &[]).map(to), Ok(vec![duty(r1), duty(r2)]));
        // either of them taken hold of: the same
        assert_eq!(targets(&b, &groups, r1, r2, &[]).map(to), Ok(vec![duty(r1), duty(r2)]));
        // the rows as the page shows them (sorted the other way round): there a row up
        assert_eq!(targets(&b, &groups, r0, r1, &[r2, r1, r0]).map(to), Ok(vec![duty(r1), duty(r2)]));
        assert_eq!(targets(&b, &groups, r1, r0, &[r2, r1, r0]).map(to), Err(st::PAST_THE_ROWS));
        // past the last row, or onto the spare duty: refused, with why
        assert_eq!(targets(&b, &groups, r0, r2, &[r0, r1, r2]).map(to), Err(st::PAST_THE_ROWS));
        assert_eq!(targets(&b, &groups, r0, r1, &[r0, r1, spare]).map(to), Err(st::NOT_A_DUTY));
    }
}

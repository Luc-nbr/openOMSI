//! Luc's driver drag in the Drivers tab (branch `rf-driver-drag`, merged into `beta` at
//! 1eba782b; the game-drawn launcher's `Hand::Duty` in `rf_gantt.rs`): single trips of a duty -
//! on one bus or several, the bus staying the same - go to another duty or to a new one. The
//! core works it out (`studio::plan_move_to_duty`, `move_to_duty`, `DutyTarget`); the zones are
//! rated here as the Vehicles tab's are (`zones_for`).

use super::super::company::words::trf;
use super::super::company::Session;
use super::{edit, Rf};
use omsi_launcher_lib::company::rosterflow::Board;
use omsi_launcher_lib::company::studio::{self as st, DutyTarget};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// The duty (an index into `Board::duties`, as the core counts them) of the Drivers tab's row.
fn duty_of_row(board: &Board, row: usize) -> Option<usize> {
    board.shaped.get(row)?.duty
}

/// The label of duty `di` (an index into `Board::duties`): "1003".
fn duty_label(board: &Board, di: usize) -> String {
    board.shaped.iter().position(|s| s.duty == Some(di)).map(|row| board.shaped[row].label(row)).unwrap_or_default()
}

/// The drop zones of trips `trips` of the duty on row `row`: every other duty they may go to,
/// and a new duty of their own, each tried on a copy of the company (`probe_with`).
pub(super) fn zones(c: &Company, board: &Board, row: usize, trips: &[(usize, usize)], probe_with: &mut dyn FnMut(Value, &dyn Fn(&mut Company) -> Result<(), &'static str>)) {
    let Some(from) = duty_of_row(board, row) else { return };
    let wd = board.weekday;
    let tours = &board.tours;
    for (d, s) in board.shaped.iter().enumerate() {
        let Some(di) = s.duty else { continue };
        if di == from || st::plan_move_to_duty(c, wd, tours, from, trips, DutyTarget::Duty(di)).is_err() {
            continue;
        }
        probe_with(json!({ "kind": "duty", "d": d }), &|p| st::move_to_duty(p, wd, tours, from, trips, DutyTarget::Duty(di)));
    }
    probe_with(json!({ "kind": "new_duty" }), &|p| st::move_to_duty(p, wd, tours, from, trips, DutyTarget::New));
}

/// The trips dropped at a zone (`kind` "duty" with its row `to`, or "new_duty"): the edit in
/// the draft, said in the history.
pub(super) fn drop(s: &mut Session, r: &mut Rf, board: &Board, row: usize, trips: Vec<(usize, usize)>, kind: &str, to: usize) -> Value {
    let Some(from) = duty_of_row(board, row) else { return json!({}) };
    let wd = board.weekday;
    let tours = board.tours.clone();
    let n = trips.len().to_string();
    let a = duty_label(board, from);
    match kind {
        "duty" => {
            let Some(di) = duty_of_row(board, to) else { return json!({}) };
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

//! The workshop's stores: the spare parts the company keeps (engines, lights, brake pads,
//! tyres, ...), what repairs take from them, and what it orders.

use super::day::DayReport;
use super::model::Company;
use serde::{Deserialize, Serialize};

/// The spare parts in store and on order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Stores {}

/// The stores' day (`day::close_day`): deliveries in, what the day's repairs took.
pub fn day(_c: &mut Company, _date: &str, _report: &mut DayReport) {}

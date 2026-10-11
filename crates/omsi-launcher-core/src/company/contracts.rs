//! The contracts the company takes on: goals with a reward, service contracts (the buses'
//! internet, ...), and what they pay or cost day by day.

use super::day::DayReport;
use super::model::Company;
use serde::{Deserialize, Serialize};

/// The contracts on offer, taken and done.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Contracts {}

/// The contracts' day (`day::close_day`): progress, rewards, what they cost.
pub fn day(_c: &mut Company, _date: &str, _report: &mut DayReport) {}

//! The company's research: what it has found out and built (buses of other sizes, upgrades
//! of its buses, things for its passengers, its premises, its staff and its workshop), each at
//! a price in money and trophies, worked on by its researchers day by day.

use super::day::DayReport;
use super::model::Company;
use serde::{Deserialize, Serialize};

/// The keys of research other parts of the company ask for.
pub mod keys {
    /// WLAN on the buses: the internet contracts (`contracts`) are offered with it.
    pub const WLAN: &str = "wlan";
    /// Ticket inspectors may be hired (`staff`).
    pub const TICKET_INSPECTORS: &str = "ticket_inspectors";
    /// The shift schedule of the other staff (`staff`).
    pub const SHIFT_SCHEDULE: &str = "shift_schedule";
}

/// What the company has researched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Research {
    /// The keys of the research done.
    #[serde(default)]
    pub done: Vec<String>,
}

impl Research {
    /// Whether the research `key` is done.
    pub fn has(&self, key: &str) -> bool {
        self.done.iter().any(|k| k == key)
    }
}

/// The research's day (`day::close_day`): the researchers' work on what runs.
pub fn day(_c: &mut Company, _date: &str, _report: &mut DayReport) {}

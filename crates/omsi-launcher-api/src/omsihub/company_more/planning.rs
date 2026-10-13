//! The planning without the RosterFlow add-on (`crates/omsi-launcher-ui/src/company/planning.rs`
//! and its Roster tab, `planning/rf_roster.rs`).

use super::super::Session;
use crate::Answer;
use serde_json::{json, Value};

pub fn view(_s: &mut Session, _args: &Value) -> Value {
    json!({})
}

pub fn service_view(_s: &mut Session, _args: &Value) -> Value {
    json!({})
}

pub fn act(_s: &mut Session, _what: &str, _args: &Value) -> Option<Answer> {
    None
}

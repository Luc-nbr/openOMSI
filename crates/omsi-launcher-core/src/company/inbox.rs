//! The company's inbox: what happens to it and around it, told as messages - events in the
//! world (a fuel price, a busy weekend), its staff, its buses, its workshop, its research, its
//! contracts and goals, its money. Every part of the company posts here (`post`).

use super::day::DayReport;
use super::model::Company;
use serde::{Deserialize, Serialize};

/// What a message is about.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Notice,
    Event,
    Staff,
    Fleet,
    Workshop,
    Research,
    Contract,
    Goal,
    Finance,
}

/// A message: its title and text are English and translated when shown, `%{name}` in them
/// filled from `args`; an event's days run `from` to `until`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Message {
    pub id: u64,
    /// The company's day it came.
    pub date: String,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub read: bool,
}

impl Message {
    pub fn new(kind: Kind, title: impl Into<String>, text: impl Into<String>) -> Message {
        Message { kind, title: title.into(), text: text.into(), ..Default::default() }
    }

    /// With `%{name}` filled by `value`.
    pub fn arg(mut self, name: &str, value: impl Into<String>) -> Message {
        self.args.push((name.to_string(), value.into()));
        self
    }
}

/// The messages, newest last.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Inbox {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub next_id: u64,
}

/// Put a message in the inbox (dated today unless it has a date).
pub fn post(c: &mut Company, mut m: Message) {
    if m.date.is_empty() {
        m.date = c.date.clone();
    }
    c.inbox.next_id += 1;
    m.id = c.inbox.next_id;
    c.inbox.messages.push(m);
}

/// The inbox's day (`day::close_day`): the world's events of the coming days.
pub fn day(_c: &mut Company, _date: &str, _report: &mut DayReport) {}

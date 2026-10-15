//! What the launchers of a company session say to each other: a friend's launcher posts a
//! `Request` (JSON) to the boss's (`server`), with the code's secret and the company's tag, its
//! player's id and name, and what it wants; every answer carries the company's day as a driver
//! needs it (`Snapshot`) - its lines and duties with their times and who drives them, the
//! company's date and clock, who is online and what each drives, the notices since the last
//! answer, and the player's own results. Nothing of the money, the staff's wages or the planning's
//! rules goes out.
//!
//! What a player asks (`Request::what`): `hello`, `snapshot`, `take` / `release` a duty,
//! `start` driving it (answered with the duty's start and the boss's game session code),
//! `report` its trips (`done` at its end), `say` a line to everyone, `leave`.

use super::super::duty_launch::DutyRef;
use crate::TripRun;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Request {
    /// The company session's version (`code::VERSION`).
    pub v: u8,
    /// The code's secret (hex) and the company's tag.
    pub secret: String,
    pub company: u16,
    /// The player's own id (his launcher keeps it) and his name.
    pub player: String,
    pub name: String,
    #[serde(rename = "do")]
    pub what: String,
    pub duty: Option<DutyRef>,
    pub trips: Vec<TripRun>,
    /// The duty is over (its last trips are in `trips`).
    pub done: bool,
    pub text: String,
    /// What the player does now: the duty his game drives, and whether his game runs.
    pub driving: Option<DutyRef>,
    pub in_game: bool,
    /// The last notice the player has (only newer ones come).
    pub since: u64,
}

/// The company as the friend's page shows it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub short: String,
    pub colours: [String; 2],
    pub map: String,
    pub map_name: String,
    /// The boss (his profile's name).
    pub boss: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapLine {
    pub line: String,
    pub number: String,
    pub colour: String,
}

/// Who a duty is for, as the friend's page tells it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DutyState {
    /// Nobody drives it: it may be taken (or asked for).
    #[default]
    Open,
    /// It cannot be taken: begun, no bus, driven already.
    Closed,
    /// Somebody of the company drives it (staff, the agency, the boss).
    Company,
    /// Another player has it.
    Player,
    /// Mine: planned for me, asked for by me, driven by me now, driven.
    Mine,
    Asked,
    Driving,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapDuty {
    pub duty: DutyRef,
    /// The number its line shows, the tour as the timetable names it.
    pub number: String,
    pub colour: String,
    pub from: i32,
    pub to: i32,
    /// Where it signs on and off, its trips with passengers.
    pub first: String,
    pub last: String,
    pub trips: usize,
    pub km: f64,
    /// Its bus (fleet number and model; empty: none yet) and the bus's file (to see that it is
    /// installed where it is driven).
    pub bus: String,
    pub file: String,
    pub state: DutyState,
    /// Who drives it (a name; empty: nobody).
    pub who: String,
    /// Why it cannot be taken (`Closed`: English, a key of the translations).
    pub why: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapPlayer {
    pub name: String,
    pub me: bool,
    pub boss: bool,
    pub online: bool,
    /// What he drives now (line number, tour, duty; None: nothing).
    pub driving: Option<(String, String, usize)>,
}

/// Something that happened, told to everyone: an English text with its arguments.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Notice {
    pub n: u64,
    /// Unix seconds.
    pub at: u64,
    pub text: String,
    pub args: Vec<(String, String)>,
}

/// A duty a player drove for the company, as its trips came to (`career::evaluate`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapResult {
    pub duty: DutyRef,
    pub number: String,
    pub trips: usize,
    pub passengers: i32,
    pub km: f64,
    /// The trips' average score (0 - 100) and their experience points together.
    pub score: u32,
    pub xp: i64,
    pub on_time: u32,
    pub stops: u32,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub card: Card,
    pub date: String,
    /// The company's clock: the minute of its day.
    pub minute: i32,
    pub open_claims: bool,
    /// The boss's game runs and hosts the company's world.
    pub world: bool,
    pub lines: Vec<SnapLine>,
    pub duties: Vec<SnapDuty>,
    pub players: Vec<SnapPlayer>,
    pub notices: Vec<Notice>,
    pub results: Vec<SnapResult>,
    /// The day's timetable is being read (no duties yet).
    pub reading: bool,
}

/// How a player's game starts on his duty in the boss's world (`duty_launch::Start`, which the
/// boss's launcher works out: the friend's has no company).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DriveStart {
    pub company: String,
    pub map: String,
    pub date: String,
    pub time: i32,
    pub line: String,
    pub tour: String,
    pub first: usize,
    pub legs: Vec<String>,
    pub bus: String,
    pub paint: String,
    pub number: String,
    pub plate: String,
    pub hof: String,
    /// The boss's game session code (`--lan-join`).
    pub game: String,
}

impl DriveStart {
    /// The duty as the friend's launcher starts the game: `base` (his game options) with the
    /// duty's start, joining the boss's world as `name`.
    pub fn duty(&self, base: crate::Duty, name: &str) -> crate::Duty {
        let st = super::super::duty_launch::Start {
            company: self.company.clone(),
            map: self.map.clone(),
            date: self.date.clone(),
            time: self.time,
            line: self.line.clone(),
            tour: self.tour.clone(),
            first: self.first,
            legs: self.legs.clone(),
            from_stop: None,
            to_stop: None,
            bus: self.bus.clone(),
            paint: self.paint.clone(),
            number: self.number.clone(),
            plate: self.plate.clone(),
            hof: self.hof.clone(),
        };
        let mut d = st.duty(base, "");
        // (in the boss's world: his date, time and weather, his company's buses - the friend has
        // no company of his own to run)
        d.lan = Some(format!("join:{}", self.game.trim()));
        d.lan_name = Some(name.to_string()).filter(|n| !n.trim().is_empty());
        d.company = None;
        d
    }

    pub fn of(s: &super::super::duty_launch::Start, game: &str) -> DriveStart {
        DriveStart {
            company: s.company.clone(),
            map: s.map.clone(),
            date: s.date.clone(),
            time: s.time,
            line: s.line.clone(),
            tour: s.tour.clone(),
            first: s.first,
            legs: s.legs.clone(),
            bus: s.bus.clone(),
            paint: s.paint.clone(),
            number: s.number.clone(),
            plate: s.plate.clone(),
            hof: s.hof.clone(),
            game: game.to_string(),
        }
    }
}

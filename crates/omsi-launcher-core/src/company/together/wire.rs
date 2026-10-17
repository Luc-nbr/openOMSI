//! What the launchers of a company session say to each other: a friend's launcher posts a
//! `Request` (JSON) to the boss's (`server`), with the code's secret and the company's tag, its
//! player's id and name, and what it wants; every answer carries the company's day as a driver
//! needs it (`Snapshot`) - its lines and duties with their times and who drives them, the
//! company's date and clock, who is online and what each drives, the notices since the last
//! answer, and the player's own results. Nothing of the money, the staff's wages or the planning's
//! rules goes out.
//!
//! What a player asks (`Request::what`): `hello`, `snapshot`, `take` / `release` a duty,
//! `start` driving it (answered with the duty's start in his own game), `report` its trips
//! (`done` at its end; ended early, the AI drives the rest), `take_back` a duty the AI drives
//! since he left it (from its next trip), `say` a line to everyone, `leave`; with the permission
//! for it, `assign`
//! / `unassign` a duty of a player (`target`), `answer` a request (`yes`), `set_role` of a
//! player (`role`, `perms`), `kick` one (`ban`), and `page`: a page of the company's own,
//! relayed to the boss's launcher (`page`: its command and arguments). A driver answers the
//! control room's messages to the duty his game drives (`msg_reply`: the message `msg`, the
//! answer's key in `text`) and tells control from it (`msg_tell`: the quick button's key); the
//! snapshot carries those messages (`Snapshot::messages`, docs/omsihub/MESSAGES.md).

use super::super::duty_launch::DutyRef;
use super::book::GuestStats;
use super::roles::{Perm, Role};
use crate::TripRun;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// The player an action is about (`assign`, `set_role`, `kick`), and what it sets.
    pub target: u32,
    pub yes: bool,
    pub ban: bool,
    pub role: Role,
    pub perms: Vec<Perm>,
    /// A page of the company relayed (`{ "cmd": …, "args": … }`).
    pub page: Value,
    /// The company's logo wanted with the answer (the player's page does not have it yet).
    pub logo: bool,
    /// The control room's message a `msg_reply` answers.
    pub msg: u64,
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
    /// The logo as the page shows it (a `data:` address; only when asked for: `Request::logo`).
    pub logo: Option<String>,
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
    /// Mine, its day closed before I drove it: still mine to drive (`book::Claim::late`).
    Late,
    /// Mine, and the AI drives it since I left it (`book::AiPart`): taken back from its next trip.
    Ai,
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
    /// Who drives it (a name; empty: nobody), and the player who has it (0: none).
    pub who: String,
    pub guest: u32,
    /// Why it cannot be taken (`Closed`: English, a key of the translations).
    pub why: String,
    /// The AI drives it now: its player (`who`) left it (Beta 63). From which departure (minutes
    /// of the day), and whether its player may take it back at a trip still to come.
    pub ai: bool,
    pub ai_from: Option<i32>,
    pub back: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapPlayer {
    /// His number in the company (0: the boss).
    pub no: u32,
    pub name: String,
    pub me: bool,
    pub boss: bool,
    pub online: bool,
    pub role: Role,
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
    /// What the trips bring in as the day's close books them: the fares and the authority's
    /// payment (cents).
    pub fares: i64,
    pub done: bool,
    /// The trips of the duty the AI drives since he left it (Beta 63).
    pub ai_trips: usize,
}

/// The player himself: his role, what he may do, what he drove for the company.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapYou {
    pub no: u32,
    pub name: String,
    pub role: Role,
    pub perms: Vec<Perm>,
    pub stats: GuestStats,
}

/// A player of the company as one who manages players (or gives duties) sees him.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapMember {
    pub no: u32,
    pub name: String,
    pub role: Role,
    pub perms: Vec<Perm>,
    pub online: bool,
    pub banned: bool,
    /// His duties of the day.
    pub duties: usize,
    pub stats: GuestStats,
}

/// A duty a player asked for, as one who may approve sees it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapAsk {
    pub duty: DutyRef,
    pub number: String,
    pub from: i32,
    pub to: i32,
    pub no: u32,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub card: Card,
    pub date: String,
    /// The company's clock: the minute of its day.
    pub minute: i32,
    pub open_claims: bool,
    pub lines: Vec<SnapLine>,
    pub duties: Vec<SnapDuty>,
    pub players: Vec<SnapPlayer>,
    pub notices: Vec<Notice>,
    pub results: Vec<SnapResult>,
    /// The day's timetable is being read (no duties yet).
    pub reading: bool,
    /// The player himself.
    pub you: Option<SnapYou>,
    /// The company's players (to one who gives duties or manages players), the duties asked for
    /// (to one who may approve them).
    pub members: Vec<SnapMember>,
    pub asks: Vec<SnapAsk>,
    /// The control room's messages of the duty the player's game drives, the newest first.
    pub messages: Vec<super::super::messages::Msg>,
}

/// How a player's game starts on his duty (`duty_launch::Start`, which the boss's launcher works
/// out: the friend's has no company): in his own game, alone, at the duty's day and time.
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
    /// A relief on the way: where the duty takes the bus over, and where it hands it on.
    pub from_stop: Option<usize>,
    pub to_stop: Option<usize>,
}

impl DriveStart {
    /// The duty as the friend's launcher starts the game: `base` (his game options) with the
    /// duty's start, in a game of his own.
    pub fn duty(&self, base: crate::Duty) -> crate::Duty {
        let st = super::super::duty_launch::Start {
            company: self.company.clone(),
            map: self.map.clone(),
            date: self.date.clone(),
            time: self.time,
            line: self.line.clone(),
            tour: self.tour.clone(),
            first: self.first,
            legs: self.legs.clone(),
            from_stop: self.from_stop,
            to_stop: self.to_stop,
            bus: self.bus.clone(),
            paint: self.paint.clone(),
            number: self.number.clone(),
            plate: self.plate.clone(),
            hof: self.hof.clone(),
        };
        let mut d = st.duty(base, "");
        // (alone, not in a session; the friend has no company of his own to run: his trips go
        // to the boss's by his launcher)
        d.company = None;
        d
    }

    pub fn of(s: &super::super::duty_launch::Start) -> DriveStart {
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
            from_stop: s.from_stop,
            to_stop: s.to_stop,
        }
    }
}

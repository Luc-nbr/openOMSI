//! Messages between the company's control room and its drivers (Luc, 2026-10-16: "a message
//! system, so that when a manager sees that you are running late or whatever, they can send you
//! a message, and that then arrives in the app, in the overlay in openOMSI"): a light CAD/AVL of
//! the kind real operators have (INIT, IVU).
//!
//! Every company keeps one queue of messages (`Company::messages`, a bounded history: `KEPT`).
//! A message comes from control (the player at the control desk), the AI dispatcher, a named
//! manager of "Play together", or a driver; it goes to a duty (a tour of the day), a driver, a
//! line, everyone, or control. It says a canned text with its places filled (`text`, `args`:
//! English, translated where it is shown) or free text, at a priority - info, instruction,
//! urgent -, and may carry an action the driver accepts or declines. Its state goes sent,
//! delivered (it reached the driver's screen), read, then accepted or declined.
//!
//! The actions are service control's ways out (`control::Act`): a short-turn, running out of
//! service to the terminus, a shorter layover, a spare bus swapped in at the terminus - and two
//! the driver carries out on the road: holding a few minutes (for a connection or the headway)
//! and handing the bus over to a relief at a stop. Accepted, a way out is applied as the
//! Operations page applies it (`carry_out`: `control::apply`, the same day's controls the delay
//! model, the feed and the fleet map read); in the game the plugin turns, skips or holds the
//! duty so the route, the IBIS and the navigator follow (`lua/messages.lua`). Declined, control
//! sees it.
//!
//! Who answers: the duty the player drives in the game now (`Messages::driving`, which the
//! launcher's link to the plugin sets), and the duties players of "Play together" drive, answer
//! for themselves; an AI driver answers at once (`ai_answers`): he reads what he is told and
//! carries out what he can.
//!
//! The AI dispatcher tells the player's own duty what the lateness and service control see -
//! running late, a way back on time, the spare bus coming (`dispatch`) - in Normal and
//! Realistic only, each once (`Messages::said`), never more often than `GAP` minutes apart.
//! Casual's controller does the best himself and stays quiet.
//!
//! A driver tells control with a quick button (`Quick`): a delay in traffic, a breakdown, an
//! accident, ... (`driver_says`). A breakdown or an accident is the day's incident as the
//! clock's own are (`clock::Broken`: the Operations page's spare, rental or drop).
//!
//! `post` is the one way in for the rest of the company (docs/omsihub/MESSAGES.md).

use super::clock::{self as ck, Broken, Level, Run};
use super::control::{self as ctl, Act};
use super::model::{Company, GameMode};
use super::plan::{BusOf, DayPlan};
use serde::{Deserialize, Serialize};

/// How many messages a company keeps (the oldest go first).
pub const KEPT: usize = 300;
/// How many of the dispatcher's words it remembers having said (`Messages::said`).
const SAID_KEPT: usize = 200;
/// How many lines of a driver's link it remembers having taken in (`Messages::seen`).
const SEEN_KEPT: usize = 300;
/// The fewest company minutes between two of the dispatcher's own messages to a driver.
pub const GAP: i64 = 10;
/// A delay grown by this many minutes since the driver was told is told again.
pub const GROWN: i32 = 5;
/// The minutes a hold asks for, as the compose box offers them.
pub const HOLDS: [i32; 3] = [2, 3, 5];

// --- what is kept ------------------------------------------------------------------------------

/// How much a message matters.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    #[default]
    Info,
    Instruction,
    Urgent,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::Info, Priority::Instruction, Priority::Urgent];

    pub fn key(self) -> &'static str {
        match self {
            Priority::Info => "info",
            Priority::Instruction => "instruction",
            Priority::Urgent => "urgent",
        }
    }

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Priority::Info => "Info",
            Priority::Instruction => "Instruction",
            Priority::Urgent => "Urgent",
        }
    }

    pub fn of(key: &str) -> Option<Priority> {
        Priority::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// Who a message comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum From {
    /// The player at the control desk (the Operations page).
    #[default]
    Control,
    /// The AI dispatcher (and the rest of the company through `post`).
    Dispatcher,
    /// A manager or dispatcher of "Play together", by name.
    Manager { name: String },
    /// A driver: the duty he drives, and his name where it is known.
    Driver { line: String, tour: String, name: String },
}

/// Who a message goes to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum To {
    /// The driver of a duty of the day: its tour (line: the timetable's name).
    Duty { line: String, tour: String },
    /// A driver of the staff, wherever he drives.
    Driver { employee: u32 },
    /// Every driver on a line.
    Line { line: String },
    /// Every driver.
    #[default]
    All,
    /// The control room (a driver's message).
    Control,
}

impl To {
    /// It reaches the driver of tour `tour` of line `line` (driven by `employee`, where known).
    pub fn reaches(&self, line: &str, tour: &str, employee: Option<u32>) -> bool {
        match self {
            To::Duty { line: l, tour: t } => ctl::same(l, line) && ctl::same(t, tour),
            To::Driver { employee: e } => employee == Some(*e),
            To::Line { line: l } => ctl::same(l, line),
            To::All => true,
            To::Control => false,
        }
    }
}

/// Where a message stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Sent,
    /// On the driver's screen.
    Delivered,
    Read,
    Accepted,
    Declined,
}

impl State {
    pub fn key(self) -> &'static str {
        match self {
            State::Sent => "sent",
            State::Delivered => "delivered",
            State::Read => "read",
            State::Accepted => "accepted",
            State::Declined => "declined",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Sent => "Sent",
            State::Delivered => "Delivered",
            State::Read => "Read",
            State::Accepted => "Accepted",
            State::Declined => "Declined",
        }
    }

    /// Answered: accepted or declined.
    pub fn answered(self) -> bool {
        matches!(self, State::Accepted | State::Declined)
    }
}

/// What an instruction asks the driver to do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum What {
    /// Turn back early: the last `part` of the trip and the first of the next are not run.
    ShortTurn { part: f64, saved: i32 },
    /// The rest of the trip (`part` of it) out of service to the terminus.
    OutOfService { part: f64, saved: i32 },
    /// A shorter layover before the trip.
    Layover { saved: i32 },
    /// Spare bus `vehicle` takes over at the terminus; `from_bus` goes back to the depot.
    Swap { vehicle: u32, from_bus: u32 },
    /// Hold `minutes` at the next stop: for a connection, else to even out the headway.
    Hold { minutes: i32, connection: bool },
    /// A relief takes the bus over at `stop` (minute `at` of the day): the duty ends there.
    Relief { stop: String, at: i32 },
}

impl What {
    pub fn key(&self) -> &'static str {
        match self {
            What::ShortTurn { .. } => "short_turn",
            What::OutOfService { .. } => "out_of_service",
            What::Layover { .. } => "layover",
            What::Swap { .. } => "swap",
            What::Hold { .. } => "hold",
            What::Relief { .. } => "relief",
        }
    }

    /// Its name in a few words (English, translated when shown).
    pub fn label(&self) -> &'static str {
        match self {
            What::ShortTurn { .. } => "Short-turn",
            What::OutOfService { .. } => "Out of service to the terminus",
            What::Layover { .. } => "Shorter layover",
            What::Swap { .. } => "Spare bus swapped in",
            What::Hold { .. } => "Hold",
            What::Relief { .. } => "Relief",
        }
    }
}

/// An instruction's action: what, for which tour, at its trip leaving at `dep`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Action {
    pub line: String,
    pub tour: String,
    pub dep: i32,
    pub what: What,
}

impl Action {
    /// Service control's way out it is (None: one the driver alone carries out).
    pub fn act(&self) -> Option<Act> {
        Some(match self.what {
            What::ShortTurn { part, saved } => Act::ShortTurn { part, saved },
            What::OutOfService { part, saved } => Act::Express { part, saved },
            What::Layover { saved } => Act::Layover { saved },
            What::Swap { vehicle, from_bus } => Act::Swap { vehicle, from_bus },
            What::Hold { .. } | What::Relief { .. } => return None,
        })
    }

    /// The action of service control's way out `act` for a tour at its trip at `dep`.
    pub fn of(line: &str, tour: &str, dep: i32, act: Act) -> Option<Action> {
        let what = match act {
            Act::ShortTurn { part, saved } => What::ShortTurn { part, saved },
            Act::Express { part, saved } => What::OutOfService { part, saved },
            Act::Layover { saved } => What::Layover { saved },
            Act::Swap { vehicle, from_bus } => What::Swap { vehicle, from_bus },
            Act::Cover { .. } => return None,
        };
        Some(Action { line: line.to_string(), tour: tour.to_string(), dep, what })
    }
}

/// A driver's quick button: what he tells control.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Quick {
    DelayTraffic,
    Breakdown,
    Accident,
    AggressivePassenger,
    RoadClosed,
    RequestRelief,
    RequestTalk,
}

impl Quick {
    pub const ALL: [Quick; 7] = [Quick::DelayTraffic, Quick::Breakdown, Quick::Accident, Quick::AggressivePassenger, Quick::RoadClosed, Quick::RequestRelief, Quick::RequestTalk];

    pub fn key(self) -> &'static str {
        match self {
            Quick::DelayTraffic => "delay_traffic",
            Quick::Breakdown => "breakdown",
            Quick::Accident => "accident",
            Quick::AggressivePassenger => "aggressive_passenger",
            Quick::RoadClosed => "road_closed",
            Quick::RequestRelief => "request_relief",
            Quick::RequestTalk => "request_talk",
        }
    }

    pub fn of(key: &str) -> Option<Quick> {
        Quick::ALL.into_iter().find(|q| q.key() == key)
    }

    /// Its button's words, which are the message's text too (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Quick::DelayTraffic => "Delay: traffic",
            Quick::Breakdown => "Breakdown",
            Quick::Accident => "Accident",
            Quick::AggressivePassenger => "Aggressive passenger",
            Quick::RoadClosed => "Road closed",
            Quick::RequestRelief => "Request relief",
            Quick::RequestTalk => "Request to talk",
        }
    }

    pub fn priority(self) -> Priority {
        match self {
            Quick::Breakdown | Quick::Accident | Quick::AggressivePassenger => Priority::Urgent,
            Quick::RoadClosed | Quick::RequestRelief => Priority::Instruction,
            Quick::DelayTraffic | Quick::RequestTalk => Priority::Info,
        }
    }

    /// The Material Symbol the buttons show (one the game and the launcher both have).
    pub fn icon(self) -> &'static str {
        match self {
            Quick::DelayTraffic => "traffic",
            Quick::Breakdown => "construction",
            Quick::Accident => "warning",
            Quick::AggressivePassenger => "emergency_home",
            Quick::RoadClosed => "back_hand",
            Quick::RequestRelief => "group",
            Quick::RequestTalk => "chat",
        }
    }
}

/// A driver's canned reply to a message (besides accepting or declining an instruction).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Canned {
    Understood,
    OnMyWay,
    RunningLate,
    CallMe,
}

impl Canned {
    pub const ALL: [Canned; 4] = [Canned::Understood, Canned::OnMyWay, Canned::RunningLate, Canned::CallMe];

    pub fn key(self) -> &'static str {
        match self {
            Canned::Understood => "understood",
            Canned::OnMyWay => "on_my_way",
            Canned::RunningLate => "running_late",
            Canned::CallMe => "call_me",
        }
    }

    pub fn of(key: &str) -> Option<Canned> {
        Canned::ALL.into_iter().find(|q| q.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Canned::Understood => "Understood",
            Canned::OnMyWay => "On my way",
            Canned::RunningLate => "Running late, doing my best",
            Canned::CallMe => "Please call me",
        }
    }
}

/// One message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Msg {
    pub id: u64,
    /// When it was sent (the company's clock: `clock::moment`).
    pub at: i64,
    pub from: From,
    pub to: To,
    pub priority: Priority,
    /// A canned text (English, `%{name}` filled from `args` when shown) or, `free`, the sender's
    /// own words as they are.
    pub text: String,
    pub args: Vec<(String, String)>,
    pub free: bool,
    pub action: Option<Action>,
    pub state: State,
    /// When it came to its state.
    pub state_at: Option<i64>,
    /// The driver's canned reply (`Canned::key`), where he gave one.
    pub reply: String,
    /// The message it answers.
    pub re: Option<u64>,
    /// A driver's quick button it was.
    pub quick: Option<Quick>,
    /// Its accepted action carried out (service control's way out applied, or the hold or the
    /// relief told), and what came of it where it could not be (English).
    pub applied: bool,
    pub outcome: String,
    /// Control has seen to a driver's message (its row leaves the Operations page).
    pub done: bool,
}

impl Msg {
    /// It waits for the driver's answer (an instruction not answered yet).
    pub fn open(&self) -> bool {
        self.action.is_some() && !self.state.answered()
    }

    /// A driver's message to control.
    pub fn from_driver(&self) -> bool {
        matches!(self.from, From::Driver { .. })
    }
}

/// The company's messages.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Messages {
    /// The last id given.
    pub next: u64,
    /// The messages, oldest first.
    pub list: Vec<Msg>,
    /// What the dispatcher said and when (its key, the moment): each once, and not too often.
    pub said: Vec<(String, i64)>,
    /// The duty the player drives in the game now (line, tour): its messages wait for him.
    pub driving: Option<(String, String)>,
    /// How far the plugin's replies were read (bytes), and the lines of the driver's devices
    /// taken in (their own ids: the same line from the game and the phone counts once).
    pub link: u64,
    pub seen: Vec<String>,
}

impl Messages {
    pub fn get(&self, id: u64) -> Option<&Msg> {
        self.list.iter().find(|m| m.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Msg> {
        self.list.iter_mut().find(|m| m.id == id)
    }

    /// Whether the dispatcher may say what `key` names at `now`: not said before, and nothing of
    /// his to the same driver (`who`) within `GAP` minutes. Said, when it may.
    pub fn may_say(&mut self, key: &str, who: &str, now: i64) -> bool {
        let gap = format!("gap|{who}");
        if self.said.iter().any(|x| x.0 == key) || self.said.iter().any(|x| x.0 == gap && now - x.1 < GAP && now >= x.1) {
            return false;
        }
        // (what was said two days ago and before is forgotten)
        self.said.retain(|x| x.0 != gap && now - x.1 < 2 * ck::DAY);
        self.said.push((key.to_string(), now));
        self.said.push((gap, now));
        if self.said.len() > SAID_KEPT {
            let extra = self.said.len() - SAID_KEPT;
            self.said.drain(..extra);
        }
        true
    }

    /// A line of a driver's device (its own id) not taken in yet: taken in now.
    pub fn first_seen(&mut self, uid: &str) -> bool {
        if uid.is_empty() {
            return true;
        }
        if self.seen.iter().any(|x| x == uid) {
            return false;
        }
        self.seen.push(uid.to_string());
        if self.seen.len() > SEEN_KEPT {
            let extra = self.seen.len() - SEEN_KEPT;
            self.seen.drain(..extra);
        }
        true
    }

    /// The messages a driver of tour `tour` of line `line` (`employee`, where known) has: to
    /// him, his line or everyone, and his own to control - the newest first, at most `n`.
    pub fn of_driver(&self, line: &str, tour: &str, employee: Option<u32>, n: usize) -> Vec<&Msg> {
        let mine = |m: &Msg| match &m.from {
            From::Driver { line: l, tour: t, .. } => ctl::same(l, line) && ctl::same(t, tour),
            _ => false,
        };
        self.list.iter().rev().filter(|m| m.to.reaches(line, tour, employee) || mine(m)).take(n).collect()
    }
}

// --- the canned texts --------------------------------------------------------------------------

pub const LATE: &str = "You are running %{min} min late";
pub const MAKE_UP: &str = "Please make up time where you safely can";
pub const BACK_ON_TIME: &str = "Well done: you are back on time";
pub const SHORT_TURN: &str = "Short-turn on the %{time} trip: turn back early, the next trip leaves on time";
pub const OUT_OF_SERVICE: &str = "Run out of service to the terminus on the %{time} trip";
pub const LAYOVER: &str = "Take a shorter layover before the %{time} trip";
pub const SWAP: &str = "Spare bus %{bus} takes over at the terminus for the %{time} trip: take your bus back to the depot";
pub const HOLD_CONNECTION: &str = "Hold %{min} min at the next stop: a connection is coming";
pub const HOLD_HEADWAY: &str = "Hold %{min} min at the next stop to even out the headway";
pub const RELIEF: &str = "A relief takes your bus over at %{stop} at %{time}";
pub const SPARE_COMING: &str = "Spare bus %{bus} is on its way: it takes over at %{time}";
pub const SPARE_WAITING: &str = "A spare bus is waiting at the depot";
pub const DIVERSION: &str = "Diversion ahead: follow the signs";
pub const ROAD_CLOSED: &str = "Road closed ahead: expect delays";
pub const BACK_TO_DEPOT: &str = "Return to the depot after this trip";
pub const CALL_CONTROL: &str = "Call control when it is safe";
pub const THANKS: &str = "Thank you";
pub const ACKNOWLEDGED: &str = "Control: understood, we are on it";

/// The canned texts the compose box offers, each with the places it fills itself (`time`,
/// `min`, `stop`, `bus`) - the rest are the instructions' own.
pub const TEMPLATES: &[&str] = &[LATE, MAKE_UP, BACK_ON_TIME, SPARE_WAITING, DIVERSION, ROAD_CLOSED, BACK_TO_DEPOT, CALL_CONTROL, THANKS];

/// What the AI driver says when he cannot do what he is told.
pub const NOT_POSSIBLE: &str = "Not possible from where the bus is now";
/// Said in the feed when a driver accepts a hold or a relief (`carry_out`).
pub const HOLD_TOLD: &str = "The driver of line %{n} tour %{tour} holds %{min} min";
pub const RELIEF_TOLD: &str = "The driver of line %{n} tour %{tour} is relieved at %{stop}";
pub const DECLINED_TOLD: &str = "The driver of line %{n} tour %{tour} declined: %{why}";
pub const DRIVER_TOLD: &str = "Line %{n} tour %{tour}: the driver reports %{why}";

/// Every English text of this module (for their translations).
pub fn texts() -> Vec<&'static str> {
    let mut out = vec![LATE, MAKE_UP, BACK_ON_TIME, SHORT_TURN, OUT_OF_SERVICE, LAYOVER, SWAP, HOLD_CONNECTION, HOLD_HEADWAY, RELIEF, SPARE_COMING, SPARE_WAITING, DIVERSION, ROAD_CLOSED, BACK_TO_DEPOT, CALL_CONTROL, THANKS, ACKNOWLEDGED, NOT_POSSIBLE, HOLD_TOLD, RELIEF_TOLD, DECLINED_TOLD, DRIVER_TOLD];
    out.extend(Priority::ALL.map(Priority::label));
    out.extend([State::Sent, State::Delivered, State::Read, State::Accepted, State::Declined].map(State::label));
    out.extend(Quick::ALL.map(Quick::label));
    out.extend(Canned::ALL.map(Canned::label));
    out.extend(["Short-turn", "Out of service to the terminus", "Shorter layover", "Spare bus swapped in", "Hold", "Relief"]);
    out
}

/// The text an action says by itself (its canned text and places), for the trip at `dep` of the
/// company's day (`time`: how the caller writes a minute of it; `bus`: a bus's number).
pub fn action_text(a: &Action, time: impl Fn(i32) -> String, bus: impl Fn(u32) -> String) -> (&'static str, Vec<(String, String)>) {
    let t = time(a.dep);
    match &a.what {
        What::ShortTurn { .. } => (SHORT_TURN, vec![ck::arg("time", t)]),
        What::OutOfService { .. } => (OUT_OF_SERVICE, vec![ck::arg("time", t)]),
        What::Layover { .. } => (LAYOVER, vec![ck::arg("time", t)]),
        What::Swap { vehicle, .. } => (SWAP, vec![ck::arg("bus", bus(*vehicle)), ck::arg("time", t)]),
        What::Hold { minutes, connection } => (if *connection { HOLD_CONNECTION } else { HOLD_HEADWAY }, vec![ck::arg("min", minutes)]),
        What::Relief { stop, at } => (RELIEF, vec![ck::arg("stop", stop), ck::arg("time", time(*at))]),
    }
}

fn bus_number(c: &Company, id: u32) -> String {
    c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default()
}

fn day_time(c: &Company) -> impl Fn(i32) -> String + '_ {
    move |m: i32| ck::hhmm(ck::moment(&c.date, m as i64))
}

// --- the API --------------------------------------------------------------------------------------

/// Post a message from the company's control (the AI dispatcher, or whatever part of the company
/// tells its drivers something) to `to`: `template` is an English text with `%{name}` places
/// filled from `args` (translated where it is shown, so add it to the translations), `action`
/// an instruction's way out. Its id.
pub fn post(c: &mut Company, to: To, priority: Priority, template: &str, args: Vec<(String, String)>, action: Option<Action>) -> u64 {
    send(c, From::Dispatcher, to, priority, template, args, false, action)
}

/// Post a message from `from` (`free`: `text` is the sender's own words, not translated).
#[allow(clippy::too_many_arguments)]
pub fn send(c: &mut Company, from: From, to: To, priority: Priority, text: &str, args: Vec<(String, String)>, free: bool, action: Option<Action>) -> u64 {
    let at = ck::now(c);
    let m = &mut c.messages;
    m.next += 1;
    let id = m.next;
    let priority = if action.is_some() && priority == Priority::Info { Priority::Instruction } else { priority };
    let text: String = if free { clean(text) } else { text.to_string() };
    m.list.push(Msg { id, at, from, to, priority, text, args, free, action, ..Default::default() });
    if m.list.len() > KEPT {
        // (the oldest go first, an instruction still open last)
        let extra = m.list.len() - KEPT;
        let mut gone = 0;
        m.list.retain(|x| {
            let go = gone < extra && !x.open();
            gone += go as usize;
            !go
        });
        if m.list.len() > KEPT {
            let extra = m.list.len() - KEPT;
            m.list.drain(..extra);
        }
    }
    id
}

/// Free text as a message keeps it: one line, at most 240 characters, no control characters.
pub fn clean(text: &str) -> String {
    let t: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(240).collect()
}

/// How a driver answers a message (as a key, `Answer::of`, in what his devices send).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(into = "String", try_from = "String")]
pub enum Answer {
    /// It reached his screen.
    Delivered,
    Read,
    Accept,
    Decline,
    /// A canned reply (an info message read with it).
    Reply(Canned),
}

impl std::convert::From<Answer> for String {
    fn from(a: Answer) -> String {
        match a {
            Answer::Delivered => "delivered".into(),
            Answer::Read => "read".into(),
            Answer::Accept => "accept".into(),
            Answer::Decline => "decline".into(),
            Answer::Reply(c) => c.key().into(),
        }
    }
}

impl TryFrom<String> for Answer {
    type Error = &'static str;
    fn try_from(key: String) -> Result<Answer, &'static str> {
        Answer::of(&key).ok_or("no such answer")
    }
}

impl Answer {
    pub fn of(key: &str) -> Option<Answer> {
        Some(match key {
            "delivered" | "seen" => Answer::Delivered,
            "read" => Answer::Read,
            "accept" => Answer::Accept,
            "decline" => Answer::Decline,
            k => Answer::Reply(Canned::of(k)?),
        })
    }
}

/// Why an answer is refused (English).
pub const NO_MESSAGE: &str = "That message is not there any more.";
pub const ANSWERED: &str = "That message was answered already.";
pub const NOT_YOURS: &str = "That message is not for you.";

/// A driver's answer to message `id`: its state moves on (never back: a message read is not
/// delivered again; one answered stays answered). An accepted action waits to be carried out
/// (`carry_out`, which has the day's plan); a declined one is told in the feed. `by`: the driver's
/// duty (line, tour) when it must be his.
pub fn answer(c: &mut Company, id: u64, a: Answer, by: Option<(&str, &str)>) -> Result<State, &'static str> {
    let now = ck::now(c);
    let Some(m) = c.messages.get(id) else { return Err(NO_MESSAGE) };
    if let Some((line, tour)) = by {
        if !m.to.reaches(line, tour, None) {
            return Err(NOT_YOURS);
        }
    }
    let had = m.state;
    let to = match a {
        Answer::Delivered => State::Delivered,
        Answer::Read | Answer::Reply(_) => State::Read,
        Answer::Accept | Answer::Decline if m.action.is_none() => State::Read,
        Answer::Accept => State::Accepted,
        Answer::Decline => State::Declined,
    };
    if had.answered() && matches!(a, Answer::Accept | Answer::Decline) && to != had {
        return Err(ANSWERED);
    }
    let action = m.action.clone();
    let m = c.messages.get_mut(id).ok_or(NO_MESSAGE)?;
    if let Answer::Reply(r) = a {
        m.reply = r.key().to_string();
    }
    if to > had && !had.answered() {
        m.state = to;
        m.state_at = Some(now);
    }
    let state = m.state;
    if to == State::Declined && had != State::Declined {
        if let Some(x) = action {
            let mut run = Run::default();
            ck::tell(c, &mut run, now, DECLINED_TOLD, vec![ck::arg("n", &x.line), ck::arg("tour", &x.tour), ck::arg("why", x.what.label())], Level::Warn);
        }
    }
    Ok(state)
}

/// The driver's inbox beside the company (`companies/<id>.inbox.json`), in the player's language:
/// the phone reads it (the launcher writes it as it writes the plugin's).
pub fn inbox_file(data: &std::path::Path, id: &str) -> std::path::PathBuf {
    super::store::dir(data).join(format!("{id}.inbox.json"))
}

/// The tour of the day plan an action is for.
fn tour_of(dp: &DayPlan, a: &Action) -> Option<usize> {
    dp.tours.iter().position(|t| ctl::same(&t.tour.line, &a.line) && ctl::same(&t.tour.tour, &a.tour))
}

/// The number its displays show of tour `line`/`tour` (the line's name where the plan lacks it).
fn number_of(dp: Option<&DayPlan>, line: &str, tour: &str) -> String {
    dp.and_then(|dp| dp.tours.iter().find(|t| ctl::same(&t.tour.line, line) && ctl::same(&t.tour.tour, tour))).map(|t| t.tour.number.clone()).unwrap_or_else(|| line.to_string())
}

/// The accepted actions not carried out yet, carried out on the company's day (`dp`, its plan now):
/// a way out of service control applied as the Operations page applies it (`control::apply`), a
/// hold or a relief told in the feed. Each once; one that cannot be done (any more) says so. The
/// messages carried out.
pub fn carry_out(c: &mut Company, dp: &DayPlan, run: &mut Run) -> Vec<u64> {
    // (a driver's breakdown or accident: the day's incident)
    let told: Vec<(u64, String, String, bool)> = c
        .messages
        .list
        .iter()
        .filter(|m| !m.applied && matches!(m.quick, Some(Quick::Breakdown | Quick::Accident)))
        .filter_map(|m| match &m.from {
            From::Driver { line, tour, .. } => Some((m.id, line.clone(), tour.clone(), m.quick == Some(Quick::Accident))),
            _ => None,
        })
        .collect();
    for (id, line, tour, accident) in told {
        broken(c, dp, &line, &tour, accident);
        if let Some(m) = c.messages.get_mut(id) {
            m.applied = true;
        }
    }
    let todo: Vec<(u64, Action)> = c.messages.list.iter().filter(|m| m.state == State::Accepted && !m.applied).filter_map(|m| m.action.clone().map(|a| (m.id, a))).collect();
    let mut out = Vec::new();
    for (id, a) in todo {
        let outcome = match (a.act(), tour_of(dp, &a)) {
            (_, None) => Some(NOT_POSSIBLE),
            (Some(act), Some(ti)) => ctl::apply(c, dp, ti, a.dep, act, false, run).is_none().then_some(NOT_POSSIBLE),
            (None, Some(ti)) => {
                let t = &dp.tours[ti].tour;
                let at = ck::now(c);
                match &a.what {
                    What::Hold { minutes, .. } => ck::tell(c, run, at, HOLD_TOLD, vec![ck::arg("n", &t.number), ck::arg("tour", &t.tour), ck::arg("min", minutes)], Level::Info),
                    What::Relief { stop, .. } => ck::tell(c, run, at, RELIEF_TOLD, vec![ck::arg("n", &t.number), ck::arg("tour", &t.tour), ck::arg("stop", stop)], Level::Info),
                    _ => {}
                }
                None
            }
        };
        if let Some(m) = c.messages.get_mut(id) {
            m.applied = true;
            m.outcome = outcome.unwrap_or_default().to_string();
        }
        out.push(id);
    }
    out
}

/// Whether a player drives tour `tour` of line `line` today: the player in his game
/// (`Messages::driving`), or a player of "Play together" whose game drives it now. His messages
/// wait for him; the rest an AI driver answers.
pub fn human(c: &Company, line: &str, tour: &str) -> bool {
    let me = c.messages.driving.as_ref().is_some_and(|(l, t)| ctl::same(l, line) && ctl::same(t, tour));
    me || c.together.claims.iter().any(|x| {
        x.duty.date == c.date && ctl::same(&x.duty.line, line) && ctl::same(&x.duty.tour, tour) && x.state == super::together::book::ClaimState::Driving && x.ai_now().is_none()
    })
}

/// The AI drivers answer what came to them: an info read, an instruction accepted and carried out
/// when it can be, else declined (`NOT_POSSIBLE`). A message to a line or to everyone is read
/// by them - and waits all the same for a player who drives there. The messages answered.
pub fn ai_answers(c: &mut Company, dp: &DayPlan, run: &mut Run) -> Vec<u64> {
    let now = ck::now(c);
    let waiting: Vec<(u64, To, bool)> = c.messages.list.iter().filter(|m| m.state < State::Read && m.to != To::Control).map(|m| (m.id, m.to.clone(), m.action.is_some())).collect();
    let mut out = Vec::new();
    for (id, to, instruction) in waiting {
        // (who it is for: a duty's driver, a driver of the staff wherever he drives today, or
        // the drivers of a line, of every line)
        let humans: Vec<bool> = dp
            .tours
            .iter()
            .filter(|t| {
                let staff = t.duties.iter().filter_map(|d| match d.who {
                    Some(super::plan::Who::Staff(e)) => Some(e),
                    _ => None,
                });
                match &to {
                    To::Driver { employee } => staff.clone().any(|e| e == *employee),
                    other => other.reaches(&t.tour.line, &t.tour.tour, None),
                }
            })
            .map(|t| human(c, &t.tour.line, &t.tour.tour))
            .collect();
        if humans.iter().any(|h| *h) {
            continue;
        }
        if instruction {
            let _ = answer(c, id, Answer::Accept, None);
            if carry_out(c, dp, run).contains(&id) && c.messages.get(id).is_some_and(|m| !m.outcome.is_empty()) {
                if let Some(m) = c.messages.get_mut(id) {
                    // (he could not: declined, and why)
                    m.state = State::Declined;
                    m.state_at = Some(now);
                }
            }
        } else {
            let _ = answer(c, id, Answer::Read, None);
        }
        out.push(id);
    }
    out
}

// --- the driver tells control ---------------------------------------------------------------------

/// A driver's quick button (`q`) from his duty (`line`, `tour`; `name` where known): a message to
/// control, told in the feed; a breakdown or an accident becomes the day's incident for his bus
/// with the day's plan (`carry_out`). `dp`: the plan, where it is at hand (the line's number).
/// Its id.
pub fn driver_says(c: &mut Company, line: &str, tour: &str, name: &str, q: Quick, dp: Option<&DayPlan>, run: &mut Run) -> u64 {
    let n = number_of(dp, line, tour);
    let id = send(c, From::Driver { line: line.to_string(), tour: tour.to_string(), name: name.to_string() }, To::Control, q.priority(), q.label(), vec![ck::arg("n", &n), ck::arg("tour", tour)], false, None);
    if let Some(m) = c.messages.get_mut(id) {
        m.quick = Some(q);
    }
    let at = ck::now(c);
    ck::tell(c, run, at, DRIVER_TOLD, vec![ck::arg("n", &n), ck::arg("tour", tour), ck::arg("why", q.label())], if q.priority() == Priority::Urgent { Level::Bad } else { Level::Warn });
    id
}

/// The bus of a tour broke down (or had an accident) now, as its driver says: the day's incident
/// - the Operations page's spare, rental or drop, as for the clock's own (unless one is open for
/// it already).
fn broken(c: &mut Company, dp: &DayPlan, line: &str, tour: &str, accident: bool) -> bool {
    let now = c.clock.minute as i32;
    let Some(t) = dp.tours.iter().find(|t| ctl::same(&t.tour.line, line) && ctl::same(&t.tour.tour, tour)) else { return false };
    let Some(BusOf::Own(vehicle)) = t.bus else { return false };
    let bus = bus_number(c, vehicle);
    let mine = dp.tours.iter().filter(|x| x.bus == Some(BusOf::Own(vehicle)));
    let until = mine.map(|x| x.tour.to()).max().unwrap_or(now).max(now);
    let Some(s) = c.clock.today.as_mut().filter(|s| s.date == dp.date) else { return false };
    if s.breaks.iter().any(|b| b.vehicle == vehicle && (b.choice.is_none() || b.at + 120 > now)) {
        return false;
    }
    s.breaks.push(Broken { vehicle, bus, line: t.tour.number.clone(), at: now, until, line_name: t.tour.line.clone(), choice: None, cost: 0, accident, back: None, spare_until: None });
    true
}

/// Control has seen to a driver's message: its row leaves the Operations page, and the driver is
/// told so (`ACKNOWLEDGED`) unless `quiet`.
pub fn acknowledge(c: &mut Company, id: u64, from: From, quiet: bool) -> Result<(), &'static str> {
    let m = c.messages.get_mut(id).ok_or(NO_MESSAGE)?;
    let From::Driver { line, tour, .. } = m.from.clone() else { return Err(NO_MESSAGE) };
    m.done = true;
    m.state = State::Read;
    if !quiet {
        let reply = send(c, from, To::Duty { line, tour }, Priority::Info, ACKNOWLEDGED, Vec::new(), false, None);
        if let Some(r) = c.messages.get_mut(reply) {
            r.re = Some(id);
        }
    }
    Ok(())
}

// --- the AI dispatcher ----------------------------------------------------------------------------

/// How the duty the player drives stands now, as his game says (the plugin's `status.json`), with
/// what the company's plan knows of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Driving {
    pub line: String,
    pub tour: String,
    /// The trip under way: its number in the duty (from 1), its departure and arrival (minutes of
    /// the day), the next trip's departure where there is one.
    pub trip: u32,
    pub dep: i32,
    pub arr: i32,
    pub next_dep: Option<i32>,
    pub next_arr: Option<i32>,
    /// The trip counts (passengers, three stops or more) and the next one too.
    pub counts: bool,
    /// Minutes late (early negative).
    pub late: f64,
    /// The share of the trip still ahead (0 - 1).
    pub left: f64,
    /// The bus of the tour today.
    pub vehicle: Option<u32>,
}

/// The dispatcher looks at the duty the player drives (Normal and Realistic: Casual's controller
/// stays quiet): running late - told once a trip, again when it grew `GROWN` minutes -, a way
/// back on time where the layover will not take the delay up (a short-turn, else out of service
/// to the terminus; the player accepts or declines), back on time after being told, the spare bus
/// coming for his broken-down bus. Never more than one message `GAP` minutes. The messages sent.
pub fn dispatch(c: &mut Company, d: &Driving) -> Vec<u64> {
    if c.game_mode == GameMode::Casual || d.line.is_empty() {
        return Vec::new();
    }
    let n = ctl::notice_minutes(c) as i32;
    let now = ck::now(c);
    let who = super::plan::tour_key(&d.line, &d.tour);
    let to = To::Duty { line: d.line.clone(), tour: d.tour.clone() };
    let mut out = Vec::new();
    let late = d.late.round() as i32;
    let at = n.max(ctl::LATE_AT + 1);
    // the spare bus on its way to his broken-down bus
    if let Some(v) = d.vehicle {
        let spare = c.clock.today.as_ref().filter(|s| s.date == c.date).and_then(|s| s.breaks.iter().enumerate().rev().find(|(_, b)| b.vehicle == v).and_then(|(i, b)| match b.choice {
            Some(ck::Choice::Spare { vehicle }) => Some((i, vehicle, b.back.unwrap_or(b.at + ck::SPARE_MINUTES))),
            _ => None,
        }));
        if let Some((i, spare, back)) = spare {
            if c.messages.may_say(&format!("spare|{}|{i}", c.date), &who, now) {
                let args = vec![ck::arg("bus", bus_number(c, spare)), ck::arg("time", day_time(c)(back))];
                out.push(post(c, to.clone(), Priority::Instruction, SPARE_COMING, args, None));
                return out;
            }
        }
    }
    if n == 0 {
        return out;
    }
    let trip = format!("{}|{}|{}", c.date, who, d.trip);
    if late >= at {
        // a way back on time: the layover before the next trip does not take the delay up
        let slack = d.next_dep.map(|x| x - d.arr - ctl::MIN_LAYOVER);
        let need = slack.map(|s| late - s.max(0));
        if let (Some(need), Some(next)) = (need, d.next_dep) {
            if late >= 2 * at && need > ctl::LATE_AT && d.counts && !c.messages.said.iter().any(|x| x.0 == format!("advice|{trip}")) {
                let dur = ((d.arr - d.dep).max(1) + (d.next_arr.unwrap_or(next + d.arr - d.dep) - next).max(1)) as f64;
                let what = if d.left >= 0.1 {
                    let part = (need as f64 / dur).clamp(0.1, 0.5_f64.min(d.left));
                    Some(What::ShortTurn { part, saved: (part * dur).round() as i32 })
                } else if d.left >= 0.05 {
                    Some(What::OutOfService { part: d.left, saved: (0.3 * d.left * (d.arr - d.dep) as f64).round().max(1.0) as i32 })
                } else {
                    None
                };
                if let Some(what) = what {
                    if c.messages.may_say(&format!("advice|{trip}"), &who, now) {
                        let a = Action { line: d.line.clone(), tour: d.tour.clone(), dep: d.dep, what };
                        let (text, args) = action_text(&a, day_time(c), |v| bus_number(c, v));
                        out.push(post(c, to, Priority::Instruction, text, args, Some(a)));
                        return out;
                    }
                }
            }
        }
        // running late: once a trip, again as it grows
        let step = (late - at).max(0) / GROWN;
        if c.messages.may_say(&format!("late|{trip}|{step}"), &who, now) {
            let p = if late >= 3 * at { Priority::Urgent } else { Priority::Info };
            out.push(post(c, to, p, LATE, vec![ck::arg("min", late)], None));
        }
        return out;
    }
    // back on time after being told
    let told = c.messages.said.iter().any(|x| x.0.starts_with(&format!("late|{trip}|")));
    if told && late <= ctl::LATE_AT && c.messages.may_say(&format!("back|{trip}"), &who, now) {
        out.push(post(c, to, Priority::Info, BACK_ON_TIME, Vec::new(), None));
    }
    out
}

/// Service control's watch told a bus of an AI driver running late (`control::watch`): the
/// dispatcher tells its driver too, once a day per tour (not in Casual).
pub fn tell_late(c: &mut Company, line: &str, tour: &str, minutes: i32) -> Option<u64> {
    if c.game_mode == GameMode::Casual || human(c, line, tour) {
        return None;
    }
    let now = ck::now(c);
    let who = super::plan::tour_key(line, tour);
    if !c.messages.may_say(&format!("late|{}|{who}", c.date), &who, now) {
        return None;
    }
    Some(post(c, To::Duty { line: line.to_string(), tour: tour.to_string() }, Priority::Info, LATE, vec![ck::arg("min", minutes)], None))
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;

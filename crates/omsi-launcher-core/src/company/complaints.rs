//! Passengers' complaints the player answers (Luc 2026-10-15: "there should be a tab where the
//! player can offer a refund or something else for the passenger if they have a complaint ...
//! If you piss the passenger off, it's going to be a bad reputation. If you can help the
//! passenger ... then it's good for the reputation").
//!
//! The day's close (`day`) draws a few complaints from what really went wrong that day: a trip
//! dropped (with its reason), a breakdown, late trips on a line, a full bus, a dirty fleet, a
//! driver's manners (the day's complaints of `incidents`), a fare far above the association's.
//! Each names the passenger, the line, the time and what happened, and has a severity and a
//! deadline; unanswered, it costs a little reputation when it runs out.
//!
//! Answering is a small scripted talk of up to three replies (`reply`): apologise and explain
//! the real cause, offer a refund or a free day pass (booked as their own fares refund), promise
//! a fix (the page links to where it is done), or dismiss it. The passenger's mood moves with
//! each reply - what fits the cause helps more, a dismissive reply angers - and the talk ends
//! happy (a little reputation, now and then thanks on social media), neutral or angry (a loss,
//! now and then a bad review). Every result goes through `reputation::change` (its caps and
//! log). Casual answers them by itself with a decent default.

use super::dates;
use super::day::{DayReport, DropWhy};
use super::model::{BookingKind, Cents, Company, GameMode};
use super::reputation::{self, Cause};
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// What a complaint is about.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Late,
    Dropped,
    Breakdown,
    Crowded,
    Dirty,
    Rude,
    Fare,
}

/// A reply the player may give.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Apologise,
    Refund,
    Voucher,
    Promise,
    Dismiss,
}

impl Choice {
    pub const ALL: [Choice; 5] = [Choice::Apologise, Choice::Refund, Choice::Voucher, Choice::Promise, Choice::Dismiss];
}

/// How a complaint ended.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Open,
    Happy,
    Neutral,
    Angry,
    Expired,
    /// Answered by the company itself (Casual).
    Auto,
}

/// A line of the talk: who said it (the passenger or the company), its words (a text to be
/// translated) and their arguments.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Said {
    pub mine: bool,
    pub text: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Complaint {
    pub id: u32,
    /// The day it is about, and the last day to answer it.
    pub date: String,
    pub deadline: String,
    pub passenger: String,
    pub line: String,
    /// Minute of the day.
    pub time: i32,
    pub kind: Kind,
    /// Why a trip was dropped.
    #[serde(default)]
    pub why: Option<DropWhy>,
    /// 1 minor, 2 serious, 3 severe.
    pub severity: u8,
    /// 0 (furious) to 100 (delighted).
    pub mood: f64,
    #[serde(default)]
    pub used: Vec<Choice>,
    #[serde(default)]
    pub talk: Vec<Said>,
    #[serde(default)]
    pub state: State,
    /// What its result did to the reputation, and what the refund or day pass cost.
    #[serde(default)]
    pub reputation: f64,
    #[serde(default)]
    pub paid: Cents,
    /// Thanks on social media (happy) or a bad review (angry).
    #[serde(default)]
    pub echo: bool,
    #[serde(default)]
    pub variant: u32,
    /// A late trip: its minutes late, the cause that brought it most (`lateness::Cause::key`),
    /// its bus and driver.
    #[serde(default)]
    pub minutes: i32,
    #[serde(default)]
    pub cause: String,
    #[serde(default)]
    pub bus: Option<u32>,
    #[serde(default)]
    pub driver: Option<u32>,
}

/// What the lateness model knows of a late trip complained about.
#[derive(Clone, Debug, Default)]
struct Late {
    minutes: i32,
    cause: String,
    bus: Option<u32>,
    driver: Option<u32>,
}

impl Complaint {
    pub fn open(&self) -> bool {
        self.state == State::Open
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Complaints {
    #[serde(default)]
    pub list: Vec<Complaint>,
    #[serde(default)]
    pub counter: u32,
}

/// Complaints a day may bring at most, and the days to answer one.
pub fn per_day(m: GameMode) -> usize {
    match m {
        GameMode::Casual => 1,
        GameMode::Normal => 2,
        GameMode::Realistic => 3,
    }
}

pub fn days_to_answer(m: GameMode) -> i64 {
    match m {
        GameMode::Realistic => 1,
        _ => 2,
    }
}

/// Answered complaints kept to look back on (days).
const KEEP_DAYS: i64 = 21;
/// Replies a talk has at most.
pub const REPLIES: usize = 3;


/// The fare a refund pays back (the association's single ticket) and a day pass's.
pub fn refund(c: &Company, k: &Complaint) -> Cents {
    super::fares::association_fare(c) * if k.severity >= 3 { 3 } else { 2 }
}

pub fn day_pass(c: &Company) -> Cents {
    super::fares::association_fare(c) * 3
}

/// The complaints of the day `date` closed (`report`, its lines and incidents in), and those
/// run out unanswered. Casual answers its own.
pub fn day(c: &mut Company, date: &str, report: &DayReport) {
    expire(c, date);
    // (a sandbox without passenger complaints: none come)
    if !super::sandbox::of(c).complaints {
        return;
    }
    let mut rng = Rng::of(&[&c.id, "complaints", date], 0);
    let mut cand: Vec<(Kind, String, i32, Option<DropWhy>, u8, f64, Late)> = Vec::new();
    // (what went wrong, with how likely it brings a complaint)
    for d in &report.drops {
        let kind = if matches!(d.why, DropWhy::BrokeDown | DropWhy::Crashed) { Kind::Breakdown } else { Kind::Dropped };
        let sev = if d.trips >= 3 { 3 } else { 2 };
        cand.push((kind, d.number.clone(), d.from, Some(d.why), sev, (0.25 + 0.1 * d.trips as f64).min(0.8), Late::default()));
    }
    let hour = |rng: &mut Rng| rng.int(6, 20) as i32 * 60 + rng.int(0, 11) as i32 * 5;
    // late trips: the worst of each line as the lateness model ran it - its time, minutes,
    // bus, driver and main cause (a day of an older file, without them: the line's count)
    let late: Vec<&super::lateness::Outcome> = report.lateness.trips.iter().filter(|o| o.dropped.is_none() && o.minutes >= 5).collect();
    if late.is_empty() {
        for l in report.lines.iter().filter(|l| l.late > 0) {
            let t = hour(&mut rng);
            let sev = if l.late >= 6 { 2 } else { 1 };
            cand.push((Kind::Late, l.number.clone(), t, None, sev, (0.08 * l.late as f64).min(0.6), Late::default()));
        }
    }
    let mut numbers: Vec<&str> = late.iter().map(|o| o.number.as_str()).collect();
    numbers.sort_unstable();
    numbers.dedup();
    for n in numbers {
        let mine: Vec<&&super::lateness::Outcome> = late.iter().filter(|o| o.number == n).collect();
        let Some(worst) = mine.iter().max_by_key(|o| o.minutes) else { continue };
        let sev = if worst.minutes >= 30 { 3 } else if worst.minutes >= 15 { 2 } else { 1 };
        let p = (0.05 * mine.len() as f64 + 0.02 * worst.minutes as f64).min(0.6);
        let x = Late { minutes: worst.minutes, cause: worst.main().map(|c| c.key().to_string()).unwrap_or_default(), bus: worst.bus, driver: worst.driver };
        cand.push((Kind::Late, n.to_string(), worst.dep, None, sev, p, x));
    }
    let busy: Vec<&super::day::LineDay> = report.lines.iter().filter(|l| l.passengers > 0).collect();
    let any_line = |rng: &mut Rng| busy.get(rng.int(0, busy.len().max(1) as i64 - 1) as usize).map(|l| l.number.clone());
    if report.left_behind > 0 || report.crowded > 0 {
        if let Some(n) = any_line(&mut rng) {
            let t = hour(&mut rng);
            cand.push((Kind::Crowded, n, t, None, if report.left_behind > 50 { 2 } else { 1 }, (0.02 * report.crowded as f64 + 0.002 * report.left_behind as f64).min(0.6), Late::default()));
        }
    }
    let held: Vec<f64> = c.fleet.iter().filter(|v| v.held_on(date)).map(|v| v.clean).collect();
    let clean = if held.is_empty() { 100.0 } else { held.iter().sum::<f64>() / held.len() as f64 };
    if clean < 55.0 {
        if let Some(n) = any_line(&mut rng) {
            let t = hour(&mut rng);
            cand.push((Kind::Dirty, n, t, None, if clean < 35.0 { 2 } else { 1 }, ((55.0 - clean) / 60.0).min(0.5), Late::default()));
        }
    }
    if report.incidents.complaints > 0 {
        if let Some(n) = any_line(&mut rng) {
            let t = hour(&mut rng);
            cand.push((Kind::Rude, n, t, None, 2, (0.2 * report.incidents.complaints as f64).min(0.7), Late::default()));
        }
    }
    for l in c.lines.iter().filter(|l| !l.number.is_empty()) {
        let x = super::fares::felt(c, l);
        if x > super::fares::FAIR && report.lines.iter().any(|d| d.line == l.name && d.passengers > 0) {
            cand.push((Kind::Fare, l.number.clone(), 0, None, 1, ((x - super::fares::FAIR) * 0.5).min(0.4), Late::default()));
        }
    }
    // (spread: the likelier ones first, a few a day at most)
    let mut drawn: Vec<_> = cand.into_iter().filter(|x| rng.chance(x.5)).collect();
    drawn.sort_by(|a, b| b.4.cmp(&a.4).then(b.5.total_cmp(&a.5)));
    let deadline = dates::add(date, days_to_answer(c.game_mode));
    for (kind, line, time, why, severity, _, x) in drawn.into_iter().take(per_day(c.game_mode)) {
        c.complaints.counter += 1;
        let id = c.complaints.counter;
        // (a passenger of the company's country: two draws, as the names of before took)
        let seed = rng.next_u64() ^ rng.next_u64().rotate_left(32);
        let passenger = super::names::draw(super::names::country_of(c), &mut Rng::new(seed), &mut Default::default()).name;
        let variant = rng.int(0, 1) as u32;
        let mood = (42.0 - 8.0 * severity as f64 + rng.range(-5.0, 5.0)).clamp(5.0, 60.0);
        let mut k = Complaint { id, date: date.to_string(), deadline: deadline.clone(), passenger, line, time, kind, why, severity, mood, used: Vec::new(), talk: Vec::new(), state: State::Open, reputation: 0.0, paid: 0, echo: false, variant, minutes: x.minutes, cause: x.cause, bus: x.bus, driver: x.driver };
        k.talk.push(Said { mine: false, text: opening_of(&k).to_string(), args: args_of(c, &k) });
        c.complaints.list.push(k);
        if c.game_mode.casual() {
            answer_by_itself(c, id, date);
        }
    }
    c.complaints.list.retain(|k| k.open() || dates::between(&k.date, date) < KEEP_DAYS);
}

/// The open complaints whose deadline has passed by `date`: a small loss each.
pub fn expire(c: &mut Company, date: &str) {
    let gone: Vec<(u32, u8)> = c.complaints.list.iter().filter(|k| k.open() && dates::between(&k.deadline, date) > 0).map(|k| (k.id, k.severity)).collect();
    for (id, sev) in gone {
        let taken = reputation::change(c, date, Cause::Complaint, -(0.05 + 0.04 * sev as f64), 1);
        if let Some(k) = c.complaints.list.iter_mut().find(|k| k.id == id) {
            k.state = State::Expired;
            k.reputation = taken;
        }
    }
}

/// Casual's answer: an apology with its cause, the passenger content.
fn answer_by_itself(c: &mut Company, id: u32, date: &str) {
    let taken = reputation::change(c, date, Cause::Complaint, 0.05, 1);
    if let Some(k) = c.complaints.list.iter_mut().find(|k| k.id == id) {
        k.talk.push(Said { mine: true, text: explain(k).to_string(), args: Vec::new() });
        k.state = State::Auto;
        k.reputation = taken;
    }
}

/// The open complaints.
pub fn open_count(c: &Company) -> usize {
    c.complaints.list.iter().filter(|k| k.open()).count()
}

/// The passenger's first words, with a late trip's minutes when the lateness model has them.
pub fn opening_of(k: &Complaint) -> &'static str {
    match k.kind {
        Kind::Late if k.minutes > 0 && k.variant % 2 == 0 => "I waited at the stop for line %{line} at %{time} and the bus came %{minutes} minutes late. I missed my connection!",
        Kind::Late if k.minutes > 0 => "Your line %{line} at %{time} was %{minutes} minutes late. I was late for work because of it.",
        kind => opening(kind, k.variant),
    }
}

/// The passenger's first words.
pub fn opening(kind: Kind, variant: u32) -> &'static str {
    let v = variant % 2 == 0;
    match kind {
        Kind::Late if v => "I waited at the stop for line %{line} at %{time} and the bus came late. I missed my connection!",
        Kind::Late => "Your line %{line} at %{time} was late again. I was late for work because of it.",
        Kind::Dropped if v => "The %{time} bus of line %{line} never came. Nobody told us anything.",
        Kind::Dropped => "I stood at the stop for nothing: line %{line} at %{time} simply did not run!",
        Kind::Breakdown if v => "My bus on line %{line} broke down at %{time} and we were stuck for ages.",
        Kind::Breakdown => "Line %{line} broke down at %{time}. Does anyone ever look after your buses?",
        Kind::Crowded if v => "The bus on line %{line} at %{time} was so full that it drove past me.",
        Kind::Crowded => "Line %{line} at %{time} was packed like a tin of sardines.",
        Kind::Dirty if v => "The bus I took on line %{line} at %{time} was filthy: rubbish on the seats.",
        Kind::Dirty => "Line %{line} today: a sticky floor and dirty windows. Disgusting.",
        Kind::Rude if v => "The driver on line %{line} at %{time} was rude to me when I asked about my ticket.",
        Kind::Rude => "Your driver on line %{line} snapped at an old lady at %{time}. Unacceptable.",
        Kind::Fare if v => "Your fares on line %{line} are far above everyone else's. Why should I pay that?",
        Kind::Fare => "%{fare} for a single ticket on line %{line}? That is daylight robbery.",
    }
}

/// The company's apology with the real cause.
pub fn explain(k: &Complaint) -> &'static str {
    match (k.kind, k.why) {
        (Kind::Dropped, Some(DropWhy::NoBus)) => "We are sorry. We had no bus for that trip; we are working on our reserve.",
        (Kind::Dropped, Some(DropWhy::NoDriver)) => "We are sorry. We had no driver for that duty; it should not have happened.",
        (Kind::Dropped, Some(DropWhy::Late)) => "We are sorry. The driver came late to his duty and the trip was lost.",
        (Kind::Dropped, Some(DropWhy::NoBlock)) => "We are sorry. That trip slipped through our planning.",
        (Kind::Dropped, _) => "We are sorry. That trip could not run, and we should have told you.",
        (Kind::Breakdown, Some(DropWhy::Crashed)) => "We are sorry. The bus was in an accident; we are glad nobody was hurt.",
        (Kind::Breakdown, _) => "We are sorry. The bus broke down, and help took too long to come.",
        (Kind::Late, _) => match k.cause.as_str() {
            "sign_on" => "We are sorry. The driver signed on late, so the bus left late.",
            "carried" => "We are sorry. The bus came in late from its trip before and had too little layover to catch up.",
            "relief" => "We are sorry. The driver came from another duty that ran late.",
            "breakdown" => "We are sorry. A bus broke down and the one sent for it took time.",
            "peak" => "We are sorry. The rush-hour traffic held the bus up more than the timetable allows.",
            "traffic" => "We are sorry. Traffic held the bus up.",
            "driver" => "We are sorry. The driver drove more slowly than the timetable needs.",
            "bus" => "We are sorry. The bus was in poor condition and could not keep to time.",
            "event" => "We are sorry. Roadworks or a diversion on the line held the bus up.",
            _ => "We are sorry. Traffic and a timetable that was too tight held the bus up.",
        },
        (Kind::Crowded, _) => "We are sorry. More people rode than we had places for.",
        (Kind::Dirty, _) => "We are sorry. That bus should never have gone out like that.",
        (Kind::Rude, _) => "We are sorry. That is not how our drivers should treat anyone.",
        (Kind::Fare, _) => "We understand. The fare pays for the service, but we hear you.",
    }
}

/// The company's promise for the cause, and the page it is done on (`company::TABS`).
pub fn promise(k: &Complaint) -> (&'static str, usize) {
    match k.kind {
        Kind::Late => match k.cause.as_str() {
            "sign_on" => ("We will make sure our drivers sign on in time.", 2),
            "breakdown" | "bus" => ("We will have our buses serviced more often.", 11),
            "driver" => ("The driver will get more training on the route.", 2),
            "event" => ("We will plan more time on line %{line} while the works last.", 5),
            _ => ("We will give line %{line} more layover so the buses keep to time.", 5),
        },
        Kind::Dropped => ("We will keep spare drivers and buses ready for line %{line}.", 2),
        Kind::Breakdown => ("We will have our buses serviced more often.", 11),
        Kind::Crowded => ("We will put a bigger bus or more trips on line %{line}.", 5),
        Kind::Dirty => ("We will have every bus cleaned each night.", 7),
        Kind::Rude => ("The driver will go on a customer-service course.", 2),
        Kind::Fare => ("We will look at the fares of line %{line} again.", 3),
    }
}

/// A reply's words as the company says it.
pub fn reply_text(k: &Complaint, choice: Choice) -> &'static str {
    match choice {
        Choice::Apologise => explain(k),
        Choice::Refund => "We will refund you %{amount}.",
        Choice::Voucher => "Please accept a free day pass worth %{amount}.",
        Choice::Promise => promise(k).0,
        Choice::Dismiss if k.kind == Kind::Late => "The traffic is not our fault. There is nothing we can do.",
        Choice::Dismiss => "We do not see what we did wrong.",
    }
}

/// A reply's label on its button.
pub fn label(choice: Choice) -> &'static str {
    match choice {
        Choice::Apologise => "Apologise and explain",
        Choice::Refund => "Offer a refund (%{amount})",
        Choice::Voucher => "Offer a free day pass (%{amount})",
        Choice::Promise => "Promise a fix",
        Choice::Dismiss => "Dismiss it",
    }
}

/// What a reply does to the passenger's mood.
pub fn effect(k: &Complaint, choice: Choice) -> f64 {
    use Kind::*;
    let base = match choice {
        Choice::Apologise if k.used.contains(&Choice::Dismiss) => 5.0,
        Choice::Apologise => match k.kind {
            Late | Dropped | Breakdown => 18.0,
            _ => 12.0,
        },
        Choice::Refund => match k.kind {
            Late | Dropped | Breakdown | Fare => 25.0,
            _ => 12.0,
        },
        Choice::Voucher => match k.kind {
            Crowded | Dirty | Late => 22.0,
            _ => 15.0,
        },
        Choice::Promise => match k.kind {
            Crowded | Dirty | Rude | Late => 20.0,
            Dropped | Breakdown => 12.0,
            Fare => 4.0,
        },
        Choice::Dismiss => return -30.0,
    };
    base * (1.15 - 0.1 * k.severity as f64)
}

/// The replies open to the player now (each once, four at most).
pub fn choices(k: &Complaint) -> Vec<Choice> {
    if !k.open() {
        return Vec::new();
    }
    let mut out: Vec<Choice> = Choice::ALL.iter().copied().filter(|x| !k.used.contains(x)).collect();
    // (the first answer: the day pass waits, so four are shown)
    if k.used.is_empty() {
        out.retain(|x| *x != Choice::Voucher);
    }
    out.truncate(4);
    out
}

/// The arguments of a complaint's words.
fn args_of(c: &Company, k: &Complaint) -> Vec<(String, String)> {
    let fare = c.lines.iter().find(|l| l.number == k.line).map(|l| super::fares::fare_of(c, l)).unwrap_or_else(|| super::fares::association_fare(c));
    vec![
        ("line".into(), k.line.clone()),
        ("time".into(), format!("{:02}:{:02}", (k.time / 60).rem_euclid(24), k.time.rem_euclid(60))),
        ("fare".into(), format!("{:.2} €", fare as f64 / 100.0)),
        ("minutes".into(), k.minutes.to_string()),
    ]
}

/// The passenger's answer to a reply that moved the mood by `delta`.
fn reaction(delta: f64, variant: u32) -> &'static str {
    let v = variant % 2 == 0;
    if delta >= 15.0 {
        if v {
            "Well, that is decent of you."
        } else {
            "Thank you, that does help."
        }
    } else if delta >= 0.0 {
        if v {
            "Hm. I suppose so."
        } else {
            "That is something, at least."
        }
    } else if v {
        "That is all you have to say?"
    } else {
        "Unbelievable. I will tell everyone about this."
    }
}

/// Answer complaint `id` with `choice` on `date`: the passenger's mood moves, a refund or day
/// pass is booked, and the talk ends when the passenger is won over, has had enough or after
/// three replies. Returns the complaint as it is now.
pub fn reply(c: &mut Company, id: u32, choice: Choice, date: &str) -> Result<Complaint, &'static str> {
    let Some(k) = c.complaints.list.iter().find(|k| k.id == id).cloned() else { return Err("There is no such complaint.") };
    if !k.open() {
        return Err("This complaint is closed.");
    }
    if !choices(&k).contains(&choice) {
        return Err("That answer was given already.");
    }
    let cost = match choice {
        Choice::Refund => refund(c, &k),
        Choice::Voucher => day_pass(c),
        _ => 0,
    };
    if cost > 0 {
        c.book(BookingKind::Fares, -cost, format!("Complaint: {} for {}", if choice == Choice::Refund { "refund" } else { "day pass" }, k.passenger), false);
    }
    let delta = effect(&k, choice);
    let mut args = args_of(c, &k);
    args.push(("amount".into(), format!("{:.2} €", cost as f64 / 100.0)));
    let mut k = k;
    k.used.push(choice);
    k.paid += cost;
    k.mood = (k.mood + delta).clamp(0.0, 100.0);
    k.talk.push(Said { mine: true, text: reply_text(&k, choice).to_string(), args: args.clone() });
    let n = k.used.len();
    let end = if k.mood >= 70.0 {
        Some(State::Happy)
    } else if k.mood <= 8.0 {
        Some(State::Angry)
    } else if n >= REPLIES {
        Some(if k.mood >= 50.0 {
            State::Happy
        } else if k.mood >= 28.0 {
            State::Neutral
        } else {
            State::Angry
        })
    } else {
        None
    };
    match end {
        None => {
            k.talk.push(Said { mine: false, text: reaction(delta, k.variant + n as u32).to_string(), args: Vec::new() });
            let ask = if k.used.contains(&Choice::Promise) { "Is that all?" } else { "And what will you do so it does not happen again?" };
            k.talk.push(Said { mine: false, text: ask.to_string(), args: Vec::new() });
        }
        Some(state) => {
            let mut rng = Rng::of(&[&c.id, "complaint"], id as i64);
            let sev = k.severity as f64;
            let (words, change, echo) = match state {
                State::Happy => {
                    let echo = rng.chance(0.25);
                    ("Thank you. I will give your buses another chance.", 0.08 + 0.04 * sev + if echo { 0.15 } else { 0.0 }, echo)
                }
                State::Neutral => ("Fine. Let us leave it at that.", -0.02 * sev, false),
                _ => {
                    let echo = rng.chance(0.3);
                    ("I am done with your company.", -(0.12 + 0.08 * sev) - if echo { 0.15 } else { 0.0 }, echo)
                }
            };
            k.talk.push(Said { mine: false, text: words.to_string(), args: Vec::new() });
            k.state = state;
            k.echo = echo;
            k.reputation = reputation::change(c, date, Cause::Complaint, change, 1);
        }
    }
    if let Some(x) = c.complaints.list.iter_mut().find(|x| x.id == id) {
        *x = k.clone();
    }
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::super::day::{Dropped, LineDay};
    use super::super::{found, Founding};
    use super::*;

    fn company(m: GameMode) -> Company {
        found(&Founding { name: "Comp".into(), game_mode: m, date: "2024-03-04".into(), ..Default::default() }, "Luc")
    }

    /// A day with trips dropped on line 5: a complaint about it, open.
    fn with_one(m: GameMode) -> (Company, u32) {
        let mut c = company(m);
        let r = DayReport {
            trips: 100,
            dropped: 6,
            drops: vec![Dropped { line: "Linie5".into(), number: "5".into(), tour: "1".into(), duty: Some(0), from: 7 * 60 + 15, to: 9 * 60, trips: 6, why: DropWhy::NoDriver, problem: None, sent: None, bus: None, driver: None, trip: String::new() }],
            lines: vec![LineDay { line: "Linie5".into(), number: "5".into(), passengers: 300, ..Default::default() }],
            ..Default::default()
        };
        // (drawn by chance: the first day of a few that brings one)
        for k in 0..40 {
            c.id = format!("c{k}");
            c.complaints = Complaints::default();
            day(&mut c, "2024-03-04", &r);
            if let Some(x) = c.complaints.list.first() {
                return (c.clone(), x.id);
            }
        }
        panic!("no complaint came");
    }

    #[test]
    fn a_good_answer_wins_the_passenger_over_and_a_dismissive_one_loses() {
        let (mut c, id) = with_one(GameMode::Normal);
        let k = c.complaints.list[0].clone();
        assert_eq!((k.kind, k.line.as_str(), k.time, k.why), (Kind::Dropped, "5", 7 * 60 + 15, Some(DropWhy::NoDriver)));
        assert!(k.open() && !k.passenger.is_empty() && k.talk.len() == 1);
        let rep = c.reputation;
        let cash = c.cash;
        reply(&mut c, id, Choice::Apologise, "2024-03-05").unwrap();
        reply(&mut c, id, Choice::Refund, "2024-03-05").unwrap();
        let k = c.complaints.list.iter().find(|k| k.id == id).unwrap().clone();
        let k = if k.open() { reply(&mut c, id, Choice::Promise, "2024-03-05").unwrap() } else { k };
        assert_eq!(k.state, State::Happy, "{k:?}");
        assert!(c.reputation > rep);
        // the refund is booked as its own entry
        assert_eq!(cash - c.cash, refund(&c, &k));
        assert!(c.ledger.iter().any(|b| b.kind == BookingKind::Fares && b.amount < 0 && b.text.starts_with("Complaint: refund")));
        assert!(c.reputation_log.entries.iter().any(|e| e.cause == Cause::Complaint && e.change > 0.0));
        // dismissive: angry, a loss
        let (mut c, id) = with_one(GameMode::Normal);
        let rep = c.reputation;
        let k = reply(&mut c, id, Choice::Dismiss, "2024-03-05").unwrap();
        let k = if k.open() { reply(&mut c, id, Choice::Promise, "2024-03-05").unwrap() } else { k };
        assert_eq!(k.state, State::Angry);
        assert!(c.reputation < rep && c.reputation > rep - 1.0);
        assert_eq!(reply(&mut c, id, Choice::Apologise, "2024-03-05").err(), Some("This complaint is closed."));
    }

    #[test]
    fn a_late_trip_complaint_names_its_minutes_and_real_cause() {
        use super::super::lateness::{Cause as Why, Lateness, Outcome};
        let o = Outcome { line: "Linie5".into(), number: "5".into(), dep: 8 * 60 + 10, bus: Some(3), driver: Some(7), minutes: 18, causes: vec![Why::SignOn { driver: 7, minutes: 18 }], ..Default::default() };
        let r = DayReport { trips: 100, late: 1, lateness: Lateness { trips: vec![o], ..Default::default() }, lines: vec![LineDay { line: "Linie5".into(), number: "5".into(), passengers: 300, late: 1, ..Default::default() }], ..Default::default() };
        let mut c = company(GameMode::Normal);
        for k in 0..60 {
            c.id = format!("l{k}");
            c.complaints = Complaints::default();
            day(&mut c, "2024-03-04", &r);
            if !c.complaints.list.is_empty() {
                break;
            }
        }
        let k = c.complaints.list.first().expect("a complaint came").clone();
        assert_eq!((k.kind, k.time, k.minutes, k.cause.as_str(), k.bus, k.driver, k.severity), (Kind::Late, 8 * 60 + 10, 18, "sign_on", Some(3), Some(7), 2));
        assert!(k.talk[0].text.contains("%{minutes} minutes"));
        assert!(explain(&k).contains("signed on late") && promise(&k).1 == 2);
        // the reputation counts the late trip once, by itself (not the punctuality band too) - on
        // a day of three trips, one late is more than the passengers forgive
        let r = DayReport { trips: 3, ..r };
        let mut c = company(GameMode::Realistic);
        let taken = super::super::reputation::close_day(&mut c, "2024-03-04", &r, Some(50.0), &Default::default());
        assert!((taken + super::super::reputation::LATE_TRIP).abs() < 1e-9, "{taken}");
    }

    #[test]
    fn an_unanswered_complaint_expires_with_a_small_loss() {
        let (mut c, id) = with_one(GameMode::Normal);
        let rep = c.reputation;
        expire(&mut c, "2024-03-06");
        assert!(c.complaints.list[0].open(), "not past its deadline yet");
        expire(&mut c, "2024-03-07");
        let k = c.complaints.list.iter().find(|k| k.id == id).unwrap();
        assert_eq!(k.state, State::Expired);
        assert!(c.reputation < rep && rep - c.reputation < 0.3);
        // Casual answers them itself
        let (c, _) = with_one(GameMode::Casual);
        assert_eq!(c.complaints.list[0].state, State::Auto);
        assert_eq!(open_count(&c), 0);
    }
}

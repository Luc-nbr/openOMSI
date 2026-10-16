//! The driver's side of the company (Luc, 2026-10-16: "the overlay, navigator and phone unit
//! should offer much more"): what a driver of the company - the player, or a player of "Play
//! together" - leaves, reports and asks for, on the company's own models.
//!
//! * Handover notes (`Note`): the outgoing driver leaves a note on the bus at a relief or when
//!   signing off ("Bus 101: rear door sticks"), from a template or typed. The incoming driver
//!   sees it at sign-on; the Fleet page shows it with the bus. A note lives for its day only
//!   (`night`).
//! * Defects (`Defect`): the walk-around at sign-off reports what is broken, and what went wrong
//!   on the way is logged by itself (a collision the game saw, a breakdown of the company's own
//!   model). A defect is a job of the workshop (`depot::order`, a repair); one that makes the bus
//!   unfit to drive (`DefectKind::critical`) takes it off the road for the rest of the day and
//!   puts its repair first in the workshop's queue.
//! * Duty swaps (`Swap`): a driver asks a colleague to swap a duty. A colleague of the staff (the
//!   AI) answers at once by the roster's rules - off that day, a second duty, the licence and the
//!   working-time rules (`plan::fits_bus`), and then whether they want to (`ai_answer`); a player
//!   of the session answers himself. An accepted swap is applied by the day's plan
//!   (`swapped`, `plan::day_plan`) - the roster itself stays as planned - or, between two
//!   players, to the duties they claimed (`together::book`).
//! * Leave and sick calls: the player's holiday request is a `holidays::Request` like the
//!   staff's, answered at once by the planner with the same limit (`ask_leave`); a sick call
//!   (`call_in_sick`) makes the player's duties of those days open with `Problem::Sick` - the
//!   dispatcher (or the central, `Problem::sudden`) covers them as he covers an ill driver's.
//!   A player of the session who calls in sick gives his duties of the day back.
//!
//! What a driver should hear of it goes through `notify`, the one place that says it.

use super::clock::Level;
use super::dates;
use super::depot::{self, JobKind};
use super::duty_launch::DutyRef;
use super::holidays::{self, By, Leave, Priority as LeavePriority, Reason, Request, State};
use super::inbox::{self, Kind, Message};
use super::model::{Company, Role};
use super::plan::{self, DayPlan, Problem, Source, Warn, Who};
use super::rng::Rng;
use super::together::book::{clean, same_duty, ClaimState};
use serde::{Deserialize, Serialize};

/// The longest note or defect text a driver may type.
pub const TEXT_MAX: usize = 140;
/// Notes one bus takes a day.
pub const NOTES_MAX: usize = 6;
/// Days a defect is kept after its job is done (the Fleet page's history).
pub const DEFECTS_KEPT: i64 = 30;
/// The longest holiday the phone asks for, and the longest sick call.
pub const LEAVE_MAX: i64 = 21;
pub const SICK_MAX: u32 = 7;

// --- notifications ------------------------------------------------------------------------------

/// How much a notification matters.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// A notification for a driver, waiting to be shown (`take_notifications`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Notification {
    pub id: u32,
    /// Who it is for (None: the company's driver, whoever drives).
    pub to: Option<Who>,
    pub priority: Priority,
    /// The English text with its `%{holes}` (a key of the translations), and what fills them.
    pub template: String,
    pub args: Vec<(String, String)>,
    /// What the notification leads to (a page or a card: `"fleet"`, `"phone"`...).
    pub action: Option<String>,
}

/// Tell a driver something: a relief due, a note left for them, a swap answered, a defect
/// logged. THE one function the crew features say anything through.
///
/// Until the company has a message system it goes to the existing toasts: queued in the company
/// (`Crew::outbox`, at most a few dozen), from where the launcher shows each as its corner's
/// toast and hands it to the game's plugin as a live notice (`omsihub/company.rs`'s
/// `depot_tick`, `addon::notify_game`). Once the message system is there (the r67-messages
/// branch: `post(company, to, priority, template, args, action)`), its body becomes that call -
/// nothing else changes.
pub fn notify(c: &mut Company, to: Option<Who>, priority: Priority, template: &str, args: &[(&str, String)], action: Option<&str>) {
    c.crew.last_note += 1;
    let n = Notification { id: c.crew.last_note, to, priority, template: template.to_string(), args: args.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), action: action.map(str::to_string) };
    c.crew.outbox.push(n);
    let over = c.crew.outbox.len().saturating_sub(OUTBOX_MAX);
    c.crew.outbox.drain(..over);
}

/// What `notify` holds back at most (the oldest go).
const OUTBOX_MAX: usize = 40;

/// The notifications waiting, taken out.
pub fn take_notifications(c: &mut Company) -> Vec<Notification> {
    std::mem::take(&mut c.crew.outbox)
}

// --- what the company keeps -----------------------------------------------------------------------

/// The crew's things of the company (`Company::crew`; an older file: none).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Crew {
    pub notes: Vec<Note>,
    pub defects: Vec<Defect>,
    pub swaps: Vec<Swap>,
    /// The player is ill up to and including this day (`call_in_sick`).
    pub sick_until: Option<String>,
    /// The player's own holiday requests (the staff's model: `holidays`).
    pub leave: Leave,
    /// The last number given to a note, a defect or a swap.
    pub last: u32,
    /// The notifications not shown yet (`notify`), and the last one's number.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outbox: Vec<Notification>,
    pub last_note: u32,
}

impl Crew {
    fn next(&mut self) -> u32 {
        self.last += 1;
        self.last
    }
}

// --- handover notes -------------------------------------------------------------------------------

/// A note's ready-made text.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NoteTemplate {
    DoorSticks,
    TicketMachine,
    Heating,
    Dirty,
    Mirror,
    Brakes,
    Display,
    Wipers,
    LowFuel,
}

impl NoteTemplate {
    pub const ALL: [NoteTemplate; 9] = [NoteTemplate::DoorSticks, NoteTemplate::TicketMachine, NoteTemplate::Heating, NoteTemplate::Dirty, NoteTemplate::Mirror, NoteTemplate::Brakes, NoteTemplate::Display, NoteTemplate::Wipers, NoteTemplate::LowFuel];

    /// Its text (English, translated when shown).
    pub fn text(self) -> &'static str {
        match self {
            NoteTemplate::DoorSticks => "A door sticks",
            NoteTemplate::TicketMachine => "The ticket machine plays up",
            NoteTemplate::Heating => "The heating or air conditioning is weak",
            NoteTemplate::Dirty => "It is dirty inside",
            NoteTemplate::Mirror => "A mirror is loose",
            NoteTemplate::Brakes => "The brakes pull to one side",
            NoteTemplate::Display => "The destination display is faulty",
            NoteTemplate::Wipers => "The wipers smear",
            NoteTemplate::LowFuel => "Fuel is low: refuel at the depot",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            NoteTemplate::DoorSticks => "door_sticks",
            NoteTemplate::TicketMachine => "ticket_machine",
            NoteTemplate::Heating => "heating",
            NoteTemplate::Dirty => "dirty",
            NoteTemplate::Mirror => "mirror",
            NoteTemplate::Brakes => "brakes",
            NoteTemplate::Display => "display",
            NoteTemplate::Wipers => "wipers",
            NoteTemplate::LowFuel => "low_fuel",
        }
    }

    pub fn from_key(s: &str) -> Option<NoteTemplate> {
        NoteTemplate::ALL.into_iter().find(|t| t.key() == s)
    }
}

/// A note left on a bus for the next driver.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Note {
    pub id: u32,
    pub vehicle: u32,
    /// The company's day it was left, and the minute (the company's clock).
    pub date: String,
    pub at: i32,
    /// Who left it (a name).
    pub by: String,
    pub template: Option<NoteTemplate>,
    /// What was typed (besides the template, or alone).
    pub text: String,
}

impl Note {
    /// What it says, in English with the bus's number for `%{bus}` (the template's text first).
    pub fn says(&self) -> String {
        match (self.template, self.text.is_empty()) {
            (Some(t), true) => t.text().to_string(),
            (Some(t), false) => format!("{}: {}", t.text(), self.text),
            (None, _) => self.text.clone(),
        }
    }
}

/// The text of the notification of a note left.
pub const NOTE_LEFT: &str = "Bus %{bus}: %{note}";

/// Leave a note on bus `vehicle` (`by`: who leaves it): its number. A template, a text or both;
/// the text is cut to `TEXT_MAX` without control characters. The next driver is told.
pub fn add_note(c: &mut Company, vehicle: u32, by: &str, template: Option<NoteTemplate>, text: &str) -> Result<u32, &'static str> {
    let Some(v) = c.vehicle(vehicle).filter(|v| v.held_on(&c.date)) else { return Err("This bus is not in the fleet.") };
    let number = v.number.clone();
    let text = clean(text, TEXT_MAX);
    if template.is_none() && text.is_empty() {
        return Err("The note is empty.");
    }
    if notes_for(c, vehicle).len() >= NOTES_MAX {
        return Err("This bus has enough notes for today.");
    }
    let id = c.crew.next();
    let note = Note { id, vehicle, date: c.date.clone(), at: c.clock.minute as i32, by: clean(by, 40), template, text };
    let says = note.says();
    c.crew.notes.push(note);
    notify(c, None, Priority::Normal, NOTE_LEFT, &[("bus", number), ("note", says)], Some("fleet"));
    Ok(id)
}

/// The notes on bus `vehicle` today, the first left first.
pub fn notes_for(c: &Company, vehicle: u32) -> Vec<&Note> {
    c.crew.notes.iter().filter(|n| n.vehicle == vehicle && n.date == c.date).collect()
}

// --- defects --------------------------------------------------------------------------------------

/// What the walk-around checks.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DefectKind {
    Doors,
    Lights,
    Mirrors,
    Wipers,
    Tyres,
    Ac,
    TicketMachine,
    Other,
}

impl DefectKind {
    pub const ALL: [DefectKind; 8] = [DefectKind::Doors, DefectKind::Lights, DefectKind::Mirrors, DefectKind::Wipers, DefectKind::Tyres, DefectKind::Ac, DefectKind::TicketMachine, DefectKind::Other];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            DefectKind::Doors => "Doors",
            DefectKind::Lights => "Lights",
            DefectKind::Mirrors => "Mirrors",
            DefectKind::Wipers => "Wipers",
            DefectKind::Tyres => "Tyres",
            DefectKind::Ac => "Air conditioning",
            DefectKind::TicketMachine => "Ticket machine",
            DefectKind::Other => "Other",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DefectKind::Doors => "doors",
            DefectKind::Lights => "lights",
            DefectKind::Mirrors => "mirrors",
            DefectKind::Wipers => "wipers",
            DefectKind::Tyres => "tyres",
            DefectKind::Ac => "ac",
            DefectKind::TicketMachine => "ticket_machine",
            DefectKind::Other => "other",
        }
    }

    pub fn from_key(s: &str) -> Option<DefectKind> {
        DefectKind::ALL.into_iter().find(|k| k.key() == s)
    }

    /// The bus is not fit to carry passengers with it (the roadworthiness check's): off the road
    /// until it is put right.
    pub fn critical(self) -> bool {
        matches!(self, DefectKind::Doors | DefectKind::Lights | DefectKind::Mirrors | DefectKind::Wipers | DefectKind::Tyres)
    }

    /// The launcher's icon for it.
    pub fn icon(self) -> &'static str {
        match self {
            DefectKind::Doors => "door_front",
            DefectKind::Lights => "light_mode",
            DefectKind::Mirrors => "flip",
            DefectKind::Wipers => "water_drop",
            DefectKind::Tyres => "tire_repair",
            DefectKind::Ac => "ac_unit",
            DefectKind::TicketMachine => "confirmation_number",
            DefectKind::Other => "build",
        }
    }
}

/// A defect of a bus, reported or logged.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Defect {
    pub id: u32,
    pub vehicle: u32,
    pub date: String,
    pub at: i32,
    pub kind: DefectKind,
    pub text: String,
    /// Who reported it ("" for one logged by itself).
    pub by: String,
    /// Logged by itself on the way (the game's bus, the company's breakdowns), not at a check.
    pub fault: bool,
    /// The workshop's job that puts it right (None: none could be ordered - the cash).
    pub job: Option<u32>,
    /// It took the bus off the road on its day.
    pub held: bool,
    /// A fault the company's own breakdown model repairs: in the workshop until this day.
    pub until: Option<String>,
}

impl Default for Defect {
    fn default() -> Self {
        Defect { id: 0, vehicle: 0, date: String::new(), at: 0, kind: DefectKind::Other, text: String::new(), by: String::new(), fault: false, job: None, held: false, until: None }
    }
}

impl Defect {
    /// Still to be put right: its job is waiting or in hand, or none was ordered yet.
    pub fn open(&self, c: &Company) -> bool {
        match (self.job, &self.until) {
            (Some(id), _) => c.site.jobs.iter().any(|j| j.id == id),
            (None, Some(u)) => dates::between(&c.date, u) >= 0,
            (None, None) => true,
        }
    }
}

/// The notification of a defect logged.
pub const DEFECT_LOGGED: &str = "Bus %{bus}: %{defect} reported to the workshop";

/// Report a defect of bus `vehicle`: kept with the bus and given to the workshop as a repair (a
/// job the bus has already takes it on). A critical one takes the bus off the road today and
/// puts its repair at the head of the queue. The same defect reported again the same day is
/// the first report. Its number.
pub fn report_defect(c: &mut Company, vehicle: u32, kind: DefectKind, text: &str, by: &str, fault: bool) -> Result<u32, &'static str> {
    let today = c.date.clone();
    let Some(v) = c.vehicle(vehicle).filter(|v| v.held_on(&today)) else { return Err("This bus is not in the fleet.") };
    let number = v.number.clone();
    if let Some(d) = c.crew.defects.iter().find(|d| d.vehicle == vehicle && d.kind == kind && d.date == today) {
        return Ok(d.id);
    }
    let job = match c.site.job_of(vehicle) {
        Some(j) => Some(j.id),
        None => depot::order(c, vehicle, JobKind::Repair).ok(),
    };
    let held = kind.critical();
    if held {
        if let Some(v) = c.fleet.iter_mut().find(|v| v.id == vehicle) {
            if !v.in_workshop(&today) {
                v.workshop_until = Some(today.clone());
            }
        }
        // (its repair first: the night starts the jobs in the order they stand)
        if let Some(i) = job.and_then(|id| c.site.jobs.iter().position(|j| j.id == id && j.started.is_none())) {
            let j = c.site.jobs.remove(i);
            c.site.jobs.insert(0, j);
        }
    }
    let id = c.crew.next();
    c.crew.defects.push(Defect { id, vehicle, date: today, at: c.clock.minute as i32, kind, text: clean(text, TEXT_MAX), by: clean(by, 40), fault, job, held, until: None });
    notify(c, None, if held { Priority::High } else { Priority::Normal }, DEFECT_LOGGED, &[("bus", number), ("defect", kind.label().to_string())], Some("workshop"));
    Ok(id)
}

/// What a fault the company's own model logs says.
pub const BREAKDOWN: &str = "Broke down on the way";
/// What a collision the game saw (the plugin's fault report) says.
pub const COLLISION: &str = "Collision damage";
pub const ACCIDENT: &str = "Damaged in an accident";

/// A fault of the company's own breakdown model (`day::close_day`): logged with the bus's
/// defects, repaired by that model (in the workshop until `until`) - no job of its own.
pub fn log_fault(c: &mut Company, vehicle: u32, text: &str, until: &str) {
    let id = c.crew.next();
    c.crew.defects.push(Defect { id, vehicle, date: c.date.clone(), at: c.clock.minute as i32, kind: DefectKind::Other, text: text.to_string(), fault: true, held: true, until: Some(until.to_string()), ..Default::default() });
}

/// The defects of bus `vehicle`, the latest first (those still open and those of the last
/// `DEFECTS_KEPT` days).
pub fn defects_of(c: &Company, vehicle: u32) -> Vec<&Defect> {
    let mut v: Vec<&Defect> = c.crew.defects.iter().filter(|d| d.vehicle == vehicle).collect();
    v.reverse();
    v
}

// --- duty swaps -----------------------------------------------------------------------------------

/// Where a swap request stands.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SwapState {
    #[default]
    Asked,
    Accepted,
    Refused,
}

/// A driver asks a colleague to swap a duty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Swap {
    pub no: u32,
    pub asked: String,
    /// Who asks, and his duty; whom he asks, and their duty that day (None: they had none, they
    /// take his).
    pub from: Option<Who>,
    pub mine: DutyRef,
    pub with: Option<Who>,
    pub theirs: Option<DutyRef>,
    pub state: SwapState,
    /// Why it was refused (English, a key of the translations).
    pub why: String,
}

pub const NO_DUTY: &str = "That duty is not in the planning of the day.";
pub const NOT_YOURS: &str = "That duty is not yours.";
pub const BEGUN: &str = "That duty has begun already.";
pub const ASKED_ALREADY: &str = "A swap of that duty is asked for already.";
pub const NO_COLLEAGUE: &str = "There is no such colleague.";
pub const MIXED: &str = "Swaps go between the staff and you, or between the players of the session.";
pub const NO_SWAP: &str = "That swap request is no longer there.";

/// Why a colleague says no (English, keys of the translations).
pub const OFF_THAT_DAY: &str = "They are off that day.";
pub const TWO_DUTIES: &str = "They have more than one duty that day.";
pub const CANNOT_DRIVE: &str = "The rules do not let them drive your duty: their licence or their working time.";
pub const YOU_CANNOT: &str = "Their duty clashes with your other work that day.";
pub const RATHER_NOT: &str = "They would rather keep their own duty.";

pub const SWAP_ACCEPTED: &str = "%{name} swaps with you: you drive %{theirs}, they drive %{mine}";
pub const SWAP_TAKEN: &str = "%{name} takes over your duty %{mine}";
pub const SWAP_REFUSED: &str = "%{name} does not swap: %{why}";
pub const SWAP_ASKED: &str = "%{name} asks you to swap duty %{mine}";

/// A duty in a few words ("5/2 duty 1, 06:00-14:30").
fn duty_words(p: &DayPlan, r: &DutyRef) -> String {
    let times = r.find(p).map(|(ti, k)| {
        let d = &p.tours[ti].duties[k];
        format!(", {}-{}", super::clock::hhmm(d.from as i64), super::clock::hhmm(d.to as i64))
    });
    let number = r.find(p).map(|(ti, _)| p.tours[ti].tour.number.clone()).unwrap_or_else(|| r.line.clone());
    format!("{} / {} #{}{}", number, r.tour, r.duty + 1, times.unwrap_or_default())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()),
        Some(Who::Guest(n)) => c.together.name_of(n),
        Some(Who::Agency) => "Agency driver".into(),
        Some(Who::Player) | None => c.profile.clone(),
    }
}

/// The duties of `who` in `p`, but `but`.
fn others(p: &DayPlan, who: Who, but: &[&DutyRef]) -> Vec<(usize, usize)> {
    p.duties_of(who).into_iter().filter(|&(ti, k)| DutyRef::of(p, ti, k).is_none_or(|r| !but.iter().any(|b| same_duty(b, &r)))).collect()
}

fn clash(p: &DayPlan, a: &[(usize, usize)], b: (usize, usize)) -> bool {
    let y = &p.tours[b.0].duties[b.1];
    a.iter().any(|&(ti, k)| {
        let x = &p.tours[ti].duties[k];
        x.from < y.to + super::staff::REST && y.from < x.to + super::staff::REST
    })
}

/// Whether colleague `id` would swap duty `mine` of `from` (`theirs`: their duty of the day, if
/// any) by the roster's rules, and then by their own will (a chance of the swap's number):
/// Ok, or the reason they say no.
pub fn ai_answer(c: &Company, p: &DayPlan, no: u32, from: Who, mine: &DutyRef, id: u32) -> Result<Option<DutyRef>, &'static str> {
    let Some(e) = c.employee(id).filter(|e| super::staff::drives(e)) else { return Err(NO_COLLEAGUE) };
    if !e.employed_on(&p.date) || e.absent(&p.date) {
        return Err(OFF_THAT_DAY);
    }
    let (mi, mk) = mine.find(p).ok_or(NO_DUTY)?;
    let theirs = p.duties_of(Who::Staff(id));
    if theirs.len() > 1 {
        return Err(TWO_DUTIES);
    }
    let theirs_ref = theirs.first().and_then(|&(ti, k)| DutyRef::of(p, ti, k));
    // they drive mine: licence, endorsements and working time against the rest of their day
    let bus = p.tours[mi].bus_of(c);
    let rest: Vec<super::staff::Block> = Vec::new();
    if plan::fits_bus(e, &bus, &rest, &plan::duty_block(p, mi, mk), p.today).is_err() {
        return Err(CANNOT_DRIVE);
    }
    // the asker drives theirs, beside the rest of his day
    if let Some(&t) = theirs.first() {
        let left = others(p, from, &[mine]);
        if clash(p, &left, t) {
            return Err(YOU_CANNOT);
        }
    }
    // and whether they want to: the friendlier and the happier the likelier; a longer duty, or
    // a day off given up, less so
    let len = |r: Option<(usize, usize)>| r.map(|(ti, k)| p.tours[ti].duties[k].to - p.tours[ti].duties[k].from).unwrap_or(0);
    let longer = len(Some((mi, mk))) - len(theirs.first().copied());
    let mut want = 0.55 + (e.friendliness - 50.0) / 200.0 + (e.satisfaction - 50.0) / 200.0;
    if theirs.is_empty() {
        want -= 0.3;
    } else if longer > 60 {
        want -= 0.25;
    }
    let mut rng = Rng::of(&[&c.id, "swap", &id.to_string()], no as i64);
    if !rng.chance(want.clamp(0.05, 1.0)) {
        return Err(RATHER_NOT);
    }
    Ok(theirs_ref)
}

/// Driver `from` asks `with` to swap duty `mine` of the day of `p` (the company's day or a later
/// one). A colleague of the staff answers at once (`ai_answer`); a player of the session later
/// (`answer_swap`). The request as it stands.
pub fn ask_swap(c: &mut Company, p: &DayPlan, from: Who, mine: &DutyRef, with: Who) -> Result<Swap, &'static str> {
    if dates::between(&c.date, &p.date) < 0 || p.date != mine.date {
        return Err(NO_DUTY);
    }
    let (ti, k) = mine.find(p).ok_or(NO_DUTY)?;
    if p.tours[ti].duties[k].who != Some(from) || p.tours[ti].by_player {
        return Err(NOT_YOURS);
    }
    if p.date == c.date && p.tours[ti].duties[k].from <= c.clock.minute as i32 {
        return Err(BEGUN);
    }
    if c.crew.swaps.iter().any(|s| s.state != SwapState::Refused && (same_duty(&s.mine, mine) || s.theirs.as_ref().is_some_and(|t| same_duty(t, mine)))) {
        return Err(ASKED_ALREADY);
    }
    let guest = |w: Who| matches!(w, Who::Guest(_));
    match (from, with) {
        (Who::Player, Who::Staff(_)) => {}
        (Who::Guest(a), Who::Guest(b)) if a != b => {}
        _ if from == with => return Err(NO_COLLEAGUE),
        (f, w) if guest(f) || guest(w) => return Err(MIXED),
        _ => return Err(NO_COLLEAGUE),
    }
    let no = c.crew.next();
    let mut s = Swap { no, asked: c.date.clone(), from: Some(from), mine: mine.clone(), with: Some(with), ..Default::default() };
    let mine_words = duty_words(p, mine);
    let name = who_name(c, Some(with));
    match with {
        Who::Staff(id) => match ai_answer(c, p, no, from, mine, id) {
            Ok(theirs) => {
                s.state = SwapState::Accepted;
                match &theirs {
                    Some(t) => notify(c, Some(from), Priority::Normal, SWAP_ACCEPTED, &[("name", name), ("theirs", duty_words(p, t)), ("mine", mine_words)], Some("phone")),
                    None => notify(c, Some(from), Priority::Normal, SWAP_TAKEN, &[("name", name), ("mine", mine_words)], Some("phone")),
                }
                s.theirs = theirs;
            }
            Err(why) => {
                s.state = SwapState::Refused;
                s.why = why.to_string();
                notify(c, Some(from), Priority::Normal, SWAP_REFUSED, &[("name", name), ("why", why.to_string())], Some("phone"));
            }
        },
        Who::Guest(b) => {
            // (the duty of his they would drive: the first of the day not begun)
            s.theirs = c.together.claims.iter().filter(|x| x.guest == b && x.duty.date == p.date && x.state == ClaimState::Planned).map(|x| x.duty.clone()).next();
            let asker = who_name(c, Some(from));
            notify(c, Some(with), Priority::High, SWAP_ASKED, &[("name", asker), ("mine", mine_words)], Some("phone"));
        }
        _ => {}
    }
    c.crew.swaps.push(s.clone());
    Ok(s)
}

/// A player of the session (`by`) answers swap request `no`: on yes the two duties change hands
/// (the duties they claimed, `together::book`), after the same check that neither clashes with
/// the rest of their day.
pub fn answer_swap(c: &mut Company, p: &DayPlan, no: u32, by: Who, yes: bool) -> Result<SwapState, &'static str> {
    let s = c.crew.swaps.iter().find(|s| s.no == no && s.state == SwapState::Asked).cloned().ok_or(NO_SWAP)?;
    if s.with != Some(by) {
        return Err(NOT_YOURS);
    }
    let (Some(Who::Guest(a)), Who::Guest(b)) = (s.from, by) else { return Err(MIXED) };
    let mut state = if yes { SwapState::Accepted } else { SwapState::Refused };
    let mut why = String::new();
    if yes {
        let mine_at = s.mine.find(p).ok_or(NO_DUTY)?;
        let begun = |r: &DutyRef| r.find(p).is_none_or(|(ti, k)| p.date == c.date && p.tours[ti].duties[k].from <= c.clock.minute as i32);
        if begun(&s.mine) || s.theirs.as_ref().is_some_and(begun) {
            return Err(BEGUN);
        }
        let theirs_at = s.theirs.as_ref().and_then(|t| t.find(p));
        let mut but = vec![&s.mine];
        if let Some(t) = &s.theirs {
            but.push(t);
        }
        if clash(p, &others(p, Who::Guest(b), &but), mine_at) || theirs_at.is_some_and(|t| clash(p, &others(p, Who::Guest(a), &but), t)) {
            state = SwapState::Refused;
            why = YOU_CANNOT.to_string();
        } else {
            for x in c.together.claims.iter_mut().filter(|x| x.duty.date == p.date) {
                if same_duty(&x.duty, &s.mine) && x.guest == a {
                    x.guest = b;
                } else if s.theirs.as_ref().is_some_and(|t| same_duty(&x.duty, t)) && x.guest == b {
                    x.guest = a;
                }
            }
        }
    }
    if let Some(x) = c.crew.swaps.iter_mut().find(|x| x.no == no) {
        x.state = state;
        x.why = why.clone();
    }
    let name = c.together.name_of(b);
    let mine = duty_words(p, &s.mine);
    match (state, &s.theirs) {
        (SwapState::Accepted, Some(t)) => notify(c, s.from, Priority::Normal, SWAP_ACCEPTED, &[("name", name), ("theirs", duty_words(p, t)), ("mine", mine)], Some("phone")),
        (SwapState::Accepted, None) => notify(c, s.from, Priority::Normal, SWAP_TAKEN, &[("name", name), ("mine", mine)], Some("phone")),
        _ => notify(c, s.from, Priority::Normal, SWAP_REFUSED, &[("name", name), ("why", if why.is_empty() { RATHER_NOT.to_string() } else { why })], Some("phone")),
    }
    Ok(state)
}

/// Who drives duty `k` of `line`/`tour` on `date`, after the accepted swaps between the player
/// and his colleagues (`who`: the roster's). `plan::day_plan` asks this for every duty.
pub fn swapped(c: &Company, date: &str, line: &str, tour: &str, k: usize, who: Who) -> Who {
    let r = DutyRef { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), duty: k };
    for s in c.crew.swaps.iter().filter(|s| s.state == SwapState::Accepted && s.mine.date == date && s.from == Some(Who::Player)) {
        let Some(Who::Staff(id)) = s.with else { continue };
        if who == Who::Player && same_duty(&s.mine, &r) {
            return Who::Staff(id);
        }
        if who == Who::Staff(id) && s.theirs.as_ref().is_some_and(|t| same_duty(t, &r)) {
            return Who::Player;
        }
    }
    who
}

// --- leave and sick calls -------------------------------------------------------------------------

pub const SICK_TODAY: &str = "A day off today is a sick call.";
pub const BAD_DAYS: &str = "Those days are not right.";
pub const TOO_LONG: &str = "Ask for three weeks at most.";
pub const HAVE_THOSE: &str = "You have asked for those days already.";
pub const NO_DAYS: &str = "You have not enough holiday days left.";
pub const ILL_ALREADY: &str = "You are reported ill already.";

pub const LEAVE_YES: &str = "Your holiday from %{from} until %{until} was approved";
pub const LEAVE_NO: &str = "Your holiday from %{from} until %{until} was rejected: %{why}";
pub const SICK_CALLED: &str = "You called in sick until %{date}: the dispatcher covers your duties";
pub const GUEST_SICK: &str = "%{name} called in sick: their duties of today are open again";
pub const GUEST_LEAVE: &str = "%{name} is off from %{from} until %{until}: their duties of those days are open again";

/// Whether the player can drive on `date`: None, or why not (ill, on holiday) - `plan::day_plan`
/// leaves his duties of that day open for the dispatcher then.
pub fn player_off(c: &Company, date: &str) -> Option<Problem> {
    if c.crew.sick_until.as_deref().is_some_and(|u| date >= c.date.as_str() && dates::between(date, u) >= 0) {
        Some(Problem::Sick)
    } else if c.crew.leave.approved_on(date).is_some() {
        Some(Problem::Holiday)
    } else {
        None
    }
}

/// The working days of the player's holidays approved this year.
fn leave_taken(c: &Company) -> u32 {
    let year = &c.date[..4.min(c.date.len())];
    c.crew.leave.requests.iter().filter(|r| r.state == State::Approved && r.from.starts_with(year)).map(Request::working_days).sum()
}

/// The planner's answer to a holiday of the player from `from` to `until`: the staff's limit of
/// drivers off on a day, the player counted, one over it for a date that cannot move
/// (`holidays::advice`'s rule).
fn leave_advice(c: &Company, from: &str, until: &str, reason: Reason) -> holidays::Advice {
    let days = holidays::days_of(from, until);
    if holidays::working_days(days.len() as i64) + leave_taken(c) > super::staff::HOLIDAYS {
        return holidays::Advice::NoDays;
    }
    let max = holidays::limit(c);
    let excess = days.iter().map(|d| (holidays::off_on(c, Role::Driver, d, None) as u32 + 1).saturating_sub(max)).max().unwrap_or(0);
    if excess == 0 {
        holidays::Advice::Fits
    } else if excess <= 1 && reason.priority() == LeavePriority::High {
        holidays::Advice::Occasion
    } else {
        holidays::Advice::Over
    }
}

/// Driver `who` asks for the days from `from` to `until` off. The player's request is a holiday
/// request of the staff's kind, answered at once by the planner (`leave_advice`) and told in the
/// inbox; a player of the session is off at once, his duties of those days given back. The
/// request's number and whether it was approved.
pub fn ask_leave(c: &mut Company, who: Who, from: &str, until: &str, reason: Reason) -> Result<(u32, bool), &'static str> {
    let (Some(a), Some(b), Some(today)) = (dates::parse(from), dates::parse(until), dates::parse(&c.date)) else { return Err(BAD_DAYS) };
    if a <= today {
        return Err(SICK_TODAY);
    }
    if b < a {
        return Err(BAD_DAYS);
    }
    if b - a + 1 > LEAVE_MAX {
        return Err(TOO_LONG);
    }
    let (from, until) = (dates::fmt(a), dates::fmt(b));
    if let Who::Guest(n) = who {
        let name = c.together.name_of(n);
        c.together.claims.retain(|x| !(x.guest == n && x.state == ClaimState::Planned && x.duty.date >= from && x.duty.date <= until));
        let m = Message::new(Kind::Staff, GUEST_LEAVE, GUEST_LEAVE).arg("name", name).arg("from", from.clone()).arg("until", until.clone());
        inbox::post(c, m);
        let no = c.crew.next();
        return Ok((no, true));
    }
    if c.crew.leave.requests.iter().any(|r| r.state != State::Refused && r.from.as_str() <= until.as_str() && from.as_str() <= r.until.as_str()) {
        return Err(HAVE_THOSE);
    }
    let advice = leave_advice(c, &from, &until, reason);
    if advice == holidays::Advice::NoDays {
        return Err(NO_DAYS);
    }
    let yes = advice.approve();
    c.staff_rules.holidays.last += 1;
    let no = c.staff_rules.holidays.last;
    let today = c.date.clone();
    let r = Request { no, from: from.clone(), until: until.clone(), reason, asked: today.clone(), due: today.clone(), state: if yes { State::Approved } else { State::Refused }, answered: Some(today), by: By::Planner, begun: false };
    c.crew.leave.requests.push(r);
    let (title, level) = if yes { (LEAVE_YES, Level::Good) } else { (LEAVE_NO, Level::Warn) };
    let m = Message::new(Kind::Staff, title, title).arg("from", from.clone()).arg("until", until.clone()).arg("why", advice.why()).level(level);
    inbox::post(c, m);
    notify(c, Some(Who::Player), Priority::Normal, title, &[("from", from), ("until", until), ("why", advice.why().to_string())], Some("phone"));
    Ok((no, yes))
}

/// Driver `who` calls in sick for `days` days from today. The player's duties of those days open
/// with `Problem::Sick` (`player_off`) - the dispatcher covers them as an ill driver's; a player
/// of the session gives back his duties of today not under way. The last day ill.
pub fn call_in_sick(c: &mut Company, who: Who, days: u32) -> Result<String, &'static str> {
    if days == 0 || days > SICK_MAX {
        return Err(BAD_DAYS);
    }
    let until = dates::add(&c.date, days as i64 - 1);
    match who {
        Who::Guest(n) => {
            let today = c.date.clone();
            let name = c.together.name_of(n);
            c.together.claims.retain(|x| !(x.guest == n && x.duty.date == today && x.state == ClaimState::Planned));
            let m = Message::new(Kind::Staff, GUEST_SICK, GUEST_SICK).arg("name", name).level(Level::Warn);
            inbox::post(c, m);
        }
        _ => {
            if player_off(c, &c.date) == Some(Problem::Sick) {
                return Err(ILL_ALREADY);
            }
            c.crew.sick_until = Some(until.clone());
            let m = Message::new(Kind::Staff, SICK_CALLED, SICK_CALLED).arg("date", until.clone()).level(Level::Warn);
            inbox::post(c, m);
            notify(c, Some(Who::Player), Priority::High, SICK_CALLED, &[("date", until.clone())], Some("ops"));
        }
    }
    Ok(until)
}

/// The player is well again (the rest of a sick call let go).
pub fn back_at_work(c: &mut Company) -> bool {
    c.crew.sick_until.take().is_some()
}

// --- the duty strip -----------------------------------------------------------------------------

/// What changed today on a duty against the roster.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    /// Minutes over the day's hours.
    Overtime { minutes: i32 },
    /// The dispatcher gave it today (not the roster's).
    Extra,
    /// A swap: the duty is someone else's in the roster.
    Swapped { with: String },
    /// The driver who takes over at the relief is not the roster's.
    Relief { name: String },
    /// Nobody takes over at the relief yet.
    NoRelief,
}

impl Change {
    /// The text (English with its holes) and what fills them.
    pub fn words(&self) -> (&'static str, Vec<(&'static str, String)>) {
        match self {
            Change::Overtime { minutes } => ("Overtime today: %{m} min", vec![("m", minutes.to_string())]),
            Change::Extra => ("An extra duty the dispatcher gave you today", vec![]),
            Change::Swapped { with } => ("Swapped with %{name}", vec![("name", with.clone())]),
            Change::Relief { name } => ("Changed relief: %{name} takes over", vec![("name", name.clone())]),
            Change::NoRelief => ("No relief planned yet: the dispatcher covers it", vec![]),
        }
    }
}

/// What changed today on duty `k` of tour `ti` of `p`: overtime, a duty the dispatcher gave, a
/// swap, a relief that is not the roster's.
pub fn changes(c: &Company, p: &DayPlan, ti: usize, k: usize) -> Vec<Change> {
    let mut out = Vec::new();
    let Some(t) = p.tours.get(ti) else { return out };
    let Some(d) = t.duties.get(k) else { return out };
    for w in &d.warn {
        if let Warn::Overtime(m) = w {
            out.push(Change::Overtime { minutes: *m });
        }
    }
    let wd = plan::weekday_of(&p.date);
    let rostered = |k: usize| plan::roster(c, wd, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
    if let Some(s) = c.crew.swaps.iter().find(|s| s.state == SwapState::Accepted && DutyRef::of(p, ti, k).is_some_and(|r| same_duty(&s.mine, &r) || s.theirs.as_ref().is_some_and(|x| same_duty(x, &r)))) {
        let other = if s.from == d.who { s.with } else { s.from };
        out.push(Change::Swapped { with: who_name(c, other) });
    } else if matches!(d.from_, Source::Dispatcher | Source::Auto | Source::Central) && rostered(k) != d.who {
        out.push(Change::Extra);
    }
    if let Some(next) = t.duties.get(k + 1) {
        match next.who {
            None => out.push(Change::NoRelief),
            Some(w) if rostered(k + 1).is_some_and(|r| swapped(c, &p.date, &t.tour.line, &t.tour.tour, k + 1, r) != w) => out.push(Change::Relief { name: who_name(c, Some(w)) }),
            _ => {}
        }
    }
    out
}

// --- the night ------------------------------------------------------------------------------------

/// The night after day `closed`: the day's notes go, so do the swaps and sick days that are over;
/// a defect is kept while its job is open and `DEFECTS_KEPT` days after.
pub fn night(c: &mut Company, closed: &str) {
    c.crew.notes.retain(|n| n.date.as_str() > closed);
    c.crew.swaps.retain(|s| s.mine.date.as_str() > closed);
    if c.crew.sick_until.as_deref().is_some_and(|u| u <= closed) {
        c.crew.sick_until = None;
    }
    let jobs: Vec<u32> = c.site.jobs.iter().map(|j| j.id).collect();
    c.crew.defects.retain(|d| d.job.is_some_and(|j| jobs.contains(&j)) || dates::between(&d.date, closed) < DEFECTS_KEPT);
    c.crew.leave.requests.retain(|r| dates::between(&r.until, closed) <= 365);
}

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![
        NOTE_LEFT, DEFECT_LOGGED, BREAKDOWN, ACCIDENT, COLLISION, NO_DUTY, NOT_YOURS, BEGUN, ASKED_ALREADY, NO_COLLEAGUE, MIXED, NO_SWAP, OFF_THAT_DAY, TWO_DUTIES, CANNOT_DRIVE, YOU_CANNOT, RATHER_NOT, SWAP_ACCEPTED, SWAP_TAKEN, SWAP_REFUSED, SWAP_ASKED, SICK_TODAY, BAD_DAYS, TOO_LONG, HAVE_THOSE, NO_DAYS, ILL_ALREADY, LEAVE_YES, LEAVE_NO, SICK_CALLED, GUEST_SICK, GUEST_LEAVE,
        "The note is empty.", "This bus has enough notes for today.",
    ];
    v.extend(NoteTemplate::ALL.iter().map(|t| t.text()));
    v.extend(DefectKind::ALL.iter().map(|k| k.label()));
    for ch in [Change::Overtime { minutes: 0 }, Change::Extra, Change::Swapped { with: String::new() }, Change::Relief { name: String::new() }, Change::NoRelief] {
        v.push(ch.words().0);
    }
    v
}

#[cfg(test)]
#[path = "crew_tests.rs"]
mod tests;

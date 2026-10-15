//! The company's agenda (Beta 57, Luc: "voeg een agenda toe met alle gebeurtenissen die er gaan
//! komen zoals de vakanties, maar ook pech, wanneer een bus weer terug is van reparaties, en
//! ander gebeurtenissen"): what is coming - and what happened in the last days - in one list,
//! read each time from what the company keeps already. Nothing here is drawn by chance or kept
//! of its own.
//!
//! * The staff: holidays approved and asked for (`holidays`) and the requests due for an
//!   answer, who is back after an illness, courses ending (`Progress::courses`), new hires,
//!   last days, requests for a raise to answer.
//! * The fleet: today's breakdowns and accidents (`clock::DayScript::breaks`) and those of the
//!   last days (their inbox messages, `clock::break_message`), a bus back from the workshop
//!   (`Vehicle::workshop_until`), workshop jobs and spare parts waiting or on their way, a
//!   service due (estimated from the bus's kilometres a day), leases and rentals ending, the
//!   dealer's deliveries and warranties, the depot's building work.
//! * The lines and contracts: tours not covered today and tomorrow, lines not in service yet,
//!   tenders opening and closing, concessions beginning and ending, a line's change taking
//!   effect and its service starting, the goals' last days, the service contracts and the
//!   advertising on the buses.
//! * Money and the rest: the month's close and the loans' last rates, research done at today's
//!   pace, the map's public and school holidays (`Calendar`), the world's events told already.
//!
//! Each entry says when (with a time where it is known), its kind, how much it matters, what
//! it is about and where it is handled (`Go`); its words are English with `%{name}` places, the
//! pages translate them.

use super::clock::{self, Choice, What};
use super::contracts::{self as ct, NetChange};
use super::dates;
use super::day::Plan;
use super::depot::{self, JobKind};
use super::holidays::State as Asked;
use super::model::{Company, GameMode, Tenure, LEGACY_SERVICE};
use super::{dealer, research};
use crate::service::{self, HolidayPeriod};
use std::collections::HashSet;

/// How far the agenda looks back and ahead by default (days).
pub const BACK: i64 = 3;
pub const AHEAD: i64 = 42;

/// The agenda's four filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Staff,
    Fleet,
    /// The lines and contracts.
    Lines,
    /// Money and the rest.
    Money,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Staff, Group::Fleet, Group::Lines, Group::Money];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Group::Staff => "Staff",
            Group::Fleet => "Fleet",
            Group::Lines => "Lines and contracts",
            Group::Money => "Money and other",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Group::Staff => "staff",
            Group::Fleet => "fleet",
            Group::Lines => "lines",
            Group::Money => "money",
        }
    }

    pub fn from_key(key: &str) -> Option<Group> {
        Group::ALL.into_iter().find(|g| g.key() == key)
    }

    pub fn icon(self) -> &'static str {
        match self {
            Group::Staff => "groups",
            Group::Fleet => "directions_bus",
            Group::Lines => "route",
            Group::Money => "payments",
        }
    }
}

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    // the staff
    Holiday,
    HolidayBack,
    HolidayAsked,
    HolidayDue,
    Recovered,
    Course,
    Hired,
    Leaving,
    Raise,
    // the fleet
    Breakdown,
    Accident,
    WorkshopBack,
    WorkshopJob,
    PartsWait,
    PartsArrive,
    ServiceDue,
    GoesBack,
    Delivery,
    Warranty,
    Building,
    // the lines and contracts
    Uncovered,
    NotInService,
    TenderOpens,
    TenderCloses,
    ConcessionBegins,
    ConcessionEnds,
    LineChange,
    LineStarts,
    Goal,
    Contract,
    Advert,
    // money and the rest
    MonthClose,
    LoanEnds,
    Research,
    PublicHoliday,
    SchoolHoliday,
    WorldEvent,
}

impl Kind {
    pub fn group(self) -> Group {
        use Kind::*;
        match self {
            Holiday | HolidayBack | HolidayAsked | HolidayDue | Recovered | Course | Hired | Leaving | Raise => Group::Staff,
            Breakdown | Accident | WorkshopBack | WorkshopJob | PartsWait | PartsArrive | ServiceDue | GoesBack | Delivery | Warranty | Building => Group::Fleet,
            Uncovered | NotInService | TenderOpens | TenderCloses | ConcessionBegins | ConcessionEnds | LineChange | LineStarts | Goal | Contract | Advert => Group::Lines,
            MonthClose | LoanEnds | Research | PublicHoliday | SchoolHoliday | WorldEvent => Group::Money,
        }
    }

    /// The launcher's icon for it.
    pub fn icon(self) -> &'static str {
        use Kind::*;
        match self {
            Holiday => "wb_sunny",
            HolidayBack => "person",
            HolidayAsked => "calendar_month",
            HolidayDue => "schedule",
            Recovered => "thermostat",
            Course => "badge",
            Hired => "person",
            Leaving => "logout",
            Raise => "payments",
            Breakdown => "error",
            Accident => "warning",
            WorkshopBack => "construction",
            WorkshopJob => "garage",
            PartsWait | PartsArrive => "inventory_2",
            ServiceDue => "timer",
            GoesBack => "exit_to_app",
            Delivery => "airport_shuttle",
            Warranty => "check_circle",
            Building => "garage",
            Uncovered => "warning",
            NotInService => "route",
            TenderOpens | TenderCloses => "public",
            ConcessionBegins | ConcessionEnds => "flag",
            LineChange => "alt_route",
            LineStarts => "route",
            Goal => "emoji_events",
            Contract => "description",
            Advert => "campaign",
            MonthClose => "receipt_long",
            LoanEnds => "payments",
            Research => "search",
            PublicHoliday => "today",
            SchoolHoliday => "school",
            WorldEvent => "event",
        }
    }
}

/// How much an entry matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Severity {
    #[default]
    Info,
    Warn,
    Bad,
}

/// What an entry is about.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum About {
    #[default]
    Company,
    Person(u32),
    /// The player himself (his own course).
    Player,
    Bus(u32),
    /// A line (its timetable name).
    Line(String),
    Goal(u32),
    Tender(u32),
}

/// Where an entry is handled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Go {
    Person(u32),
    /// The Staff page's Holidays tab, on a day.
    Holidays(String),
    /// The player's own courses (the career).
    Courses,
    Bus(u32),
    /// The workshop's stores.
    Workshop,
    Depot,
    Dealer,
    Adverts,
    Line(String),
    /// The planning (RosterFlow with its add-on on) on a day.
    Planning(String),
    Concessions,
    /// The contracts page: its goals (0) or its service contracts (1).
    Contracts(u32),
    Finances,
    Research,
    Inbox,
}

/// An entry of the agenda.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub date: String,
    /// The minute of the day, where it is known.
    pub minute: Option<i32>,
    pub kind: Kind,
    /// What it says, and a second line (English, `%{name}` places filled from `args`).
    pub text: &'static str,
    pub note: Option<&'static str>,
    pub args: Vec<(String, String)>,
    pub severity: Severity,
    pub about: About,
    pub go: Go,
    /// It wants the player: a request to answer, something without cover.
    pub needs_me: bool,
}

impl Entry {
    fn new(date: &str, kind: Kind, text: &'static str, go: Go) -> Entry {
        Entry { date: date.to_string(), minute: None, kind, text, note: None, args: Vec::new(), severity: Severity::Info, about: About::Company, go, needs_me: false }
    }

    fn at(mut self, minute: i64) -> Entry {
        self.minute = Some(minute.clamp(0, 24 * 60 - 1) as i32);
        self
    }

    fn note(mut self, note: &'static str) -> Entry {
        self.note = Some(note);
        self
    }

    fn arg(mut self, name: &str, value: impl ToString) -> Entry {
        self.args.push((name.to_string(), value.to_string()));
        self
    }

    fn sev(mut self, s: Severity) -> Entry {
        self.severity = s;
        self
    }

    fn about(mut self, a: About) -> Entry {
        self.about = a;
        self
    }

    fn mine(mut self, yes: bool) -> Entry {
        self.needs_me = yes;
        self
    }

    pub fn group(&self) -> Group {
        self.kind.group()
    }

    /// The value of the place `name`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.args.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
    }
}

/// What the company knows besides its file: today's and tomorrow's plans (the tours covered,
/// each bus's kilometres) and the map's calendar.
#[derive(Clone, Copy, Debug, Default)]
pub struct Known<'a> {
    pub today: Option<&'a Plan>,
    pub tomorrow: Option<&'a Plan>,
    pub calendar: Option<&'a Calendar>,
}

// --- the words --------------------------------------------------------------------------------

const ON_HOLIDAY: &str = "%{name} goes on holiday until %{until}";
const BACK_FROM_HOLIDAY: &str = "%{name} is back from holiday";
const ASKED: &str = "%{name} asked for a holiday until %{until}";
const NOT_ANSWERED: &str = "Not answered yet";
const ANSWER_HOLIDAY: &str = "Answer %{name}'s holiday request";
const PLANNER_ANSWERS: &str = "%{from} until %{until}: no answer, and the planner's suggestion holds";
const SILENCE_IS_NO: &str = "%{from} until %{until}: no answer is a no";
const BACK_AFTER_ILLNESS: &str = "%{name} is back after being ill";
const COURSE_ENDS: &str = "%{name} finishes the course %{what}";
const COURSE_ENDS_PLAIN: &str = "%{name}'s course ends";
const YOUR_COURSE_ENDS: &str = "You finish the course %{what}";
const STARTS: &str = "%{name} starts as %{role}";
const LAST_DAY: &str = "%{name}'s last day at the company";
const RESIGNED: &str = "Resigned";
const DISMISSED: &str = "Dismissed";
const ANSWER_RAISE: &str = "Answer %{name}'s request for a raise";
const RAISE_NOTE: &str = "%{pct} % more: no answer by then is a no";

const BROKE_DOWN: &str = "Bus %{bus} broke down on line %{n}";
const CRASHED: &str = "Bus %{bus} had an accident on line %{n}";
const WAITING: &str = "Waiting for your decision";
const SPARE_SENT: &str = "Spare bus %{spare} took over its trips from %{arrive}";
const RENTAL_SENT: &str = "A rental bus took over its trips from %{arrive}";
const TRIPS_DROPPED: &str = "Its trips were dropped";
const BACK_FROM_WORKSHOP: &str = "Bus %{bus} is back from the workshop";
const JOB_DONE: &str = "Job done: %{job}";
const AFTER_BREAKDOWN: &str = "Repaired after its breakdown";
const AFTER_ACCIDENT: &str = "Repaired after its accident";
const INTO_WORKSHOP: &str = "Bus %{bus} goes into the workshop: %{job}";
const WAITS_BAY: &str = "Bus %{bus} waits for a free workshop bay: %{job}";
const WAITS_PARTS: &str = "Bus %{bus} waits for spare parts";
const SINCE: &str = "Since %{date}";
const PARTS_ARRIVE: &str = "Spare parts arrive";
const PARTS_COUNT: &str = "%{count} parts";
const SERVICE_DUE: &str = "Bus %{bus} is due for its service";
const ESTIMATED: &str = "Estimated from %{km} km a day";
const LEASE_ENDS: &str = "Bus %{bus}: its lease ends";
const RENTAL_ENDS: &str = "Bus %{bus}: its rental ends";
const LEAVES_FLEET: &str = "It leaves the fleet after this day";
const DELIVERY: &str = "The dealer delivers %{count} × %{name}";
const WARRANTY_ENDS: &str = "Bus %{bus}: its warranty ends";
const BUILDING_DONE: &str = "Building work finishes: %{area}";

const NOT_COVERED: &str = "%{count} tours not covered";
const SHORT_OF: &str = "%{buses} without a bus, %{duties} duties without a driver";
const NOT_IN_SERVICE: &str = "Line %{n} is not in service yet";
const PLAN_IT: &str = "Plan its tours and start its service";
const PLAN_NAMED: &str = "%{name}: plan its tours and start its service";
const LINE_STARTS: &str = "Line %{n} starts its service";
const LINE_CHANGES: &str = "Line %{n}: its changes take effect";
const TENDER_OPENS: &str = "Line %{n} is out to tender: the auction opens";
const RENEWAL_OPENS: &str = "The renewal of line %{n} is out to tender: the auction opens";
const TENDER_CLOSES: &str = "The auction for line %{n} closes";
const KEEP_IT: &str = "Your concession: bid to keep it";
const CONCESSION_BEGINS: &str = "The concession for line %{n} begins";
const CONCESSION_ENDS: &str = "The concession for line %{n} ends";
const RENEWAL_OUT: &str = "Its renewal is out to tender";
const GOAL_LAST_DAY: &str = "The goal's last day: %{done} % reached";
const FUEL_BEGINS: &str = "The fixed diesel price begins";
const FUEL_ENDS: &str = "The fixed diesel price ends";
const CLEANING_BEGINS: &str = "The cleaning company starts on the buses";
const CLEANING_ENDS: &str = "The cleaning company's contract ends";
const INTERNET_BEGINS: &str = "The buses' internet begins with %{who}";
const INTERNET_CHANGES: &str = "The buses' internet changes to %{who}";
const INTERNET_ENDS: &str = "The buses' internet ends";
const ADVERT_ENDS: &str = "The advertising for %{who} ends";

const MONTH_CLOSE: &str = "The month closes: wages, leases and insurance are booked";
const LOAN_RATES: &str = "Loan rates: %{amount}";
const LOAN_LAST: &str = "The last rate of the loan: %{name}";
const RESEARCH_DONE: &str = "Research done: %{what}";
const AT_PACE: &str = "At today's pace";
const RESEARCH_IDLE: &str = "Nobody works on the research: %{what}";
const PUBLIC_HOLIDAY: &str = "Public holiday: %{what}";
const SCHOOL_BEGINS: &str = "School holidays begin: %{what}";
const UNTIL: &str = "Until %{until}";
const SCHOOL_ENDS: &str = "School starts again after %{what}";
const FROM_UNTIL: &str = "From %{from} until %{until}";
const LOAN: &str = "Loan";

/// The common public holidays' names (`service::common_public_holiday`), by day of the year
/// (or of Easter: `e` the days after Easter Sunday).
const NEW_YEAR: &str = "New Year's Day";
const GOOD_FRIDAY: &str = "Good Friday";
const EASTER_MONDAY: &str = "Easter Monday";
const LABOUR_DAY: &str = "Labour Day";
const ASCENSION: &str = "Ascension Day";
const WHIT_MONDAY: &str = "Whit Monday";
const CHRISTMAS: &str = "Christmas Day";
const SECOND_CHRISTMAS: &str = "Second Day of Christmas";

/// Every English text the agenda says, for the translations.
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![
        ON_HOLIDAY, BACK_FROM_HOLIDAY, ASKED, NOT_ANSWERED, ANSWER_HOLIDAY, PLANNER_ANSWERS, SILENCE_IS_NO, BACK_AFTER_ILLNESS, COURSE_ENDS, COURSE_ENDS_PLAIN, YOUR_COURSE_ENDS, STARTS, LAST_DAY, RESIGNED, DISMISSED, ANSWER_RAISE, RAISE_NOTE,
        BROKE_DOWN, CRASHED, WAITING, SPARE_SENT, RENTAL_SENT, TRIPS_DROPPED, BACK_FROM_WORKSHOP, JOB_DONE, AFTER_BREAKDOWN, AFTER_ACCIDENT, INTO_WORKSHOP, WAITS_BAY, WAITS_PARTS, SINCE, PARTS_ARRIVE, PARTS_COUNT, SERVICE_DUE, ESTIMATED, LEASE_ENDS, RENTAL_ENDS, LEAVES_FLEET, DELIVERY, WARRANTY_ENDS, BUILDING_DONE,
        NOT_COVERED, SHORT_OF, NOT_IN_SERVICE, PLAN_IT, PLAN_NAMED, LINE_STARTS, LINE_CHANGES, TENDER_OPENS, RENEWAL_OPENS, TENDER_CLOSES, KEEP_IT, CONCESSION_BEGINS, CONCESSION_ENDS, RENEWAL_OUT, GOAL_LAST_DAY, FUEL_BEGINS, FUEL_ENDS, CLEANING_BEGINS, CLEANING_ENDS, INTERNET_BEGINS, INTERNET_CHANGES, INTERNET_ENDS, ADVERT_ENDS,
        MONTH_CLOSE, LOAN_RATES, LOAN_LAST, RESEARCH_DONE, AT_PACE, RESEARCH_IDLE, PUBLIC_HOLIDAY, SCHOOL_BEGINS, UNTIL, SCHOOL_ENDS, FROM_UNTIL, LOAN,
        NEW_YEAR, GOOD_FRIDAY, EASTER_MONDAY, LABOUR_DAY, ASCENSION, WHIT_MONDAY, CHRISTMAS, SECOND_CHRISTMAS,
    ];
    v.extend(Group::ALL.iter().map(|g| g.label()));
    v
}

// --- the agenda -------------------------------------------------------------------------------

/// The agenda's days by default: a few back, some weeks ahead.
pub fn window(c: &Company) -> (String, String) {
    (dates::add(&c.date, -BACK), dates::add(&c.date, AHEAD))
}

/// The company's entries from `from` to `until` (both counted), in the order of their days and
/// times (an entry of the whole day first, then the worst first).
pub fn agenda(c: &Company, from: &str, until: &str, k: &Known) -> Vec<Entry> {
    let mut out = Vec::new();
    staff(c, &mut out);
    let broken = breakdowns(c);
    fleet(c, k, &broken, &mut out);
    lines(c, k, &mut out);
    money(c, k, from, until, &mut out);
    out.retain(|e| from <= e.date.as_str() && e.date.as_str() <= until);
    out.sort_by(|a, b| a.date.cmp(&b.date).then(a.minute.unwrap_or(-1).cmp(&b.minute.unwrap_or(-1))).then(b.severity.cmp(&a.severity)).then(a.kind.cmp(&b.kind)));
    out
}

/// The next `n` entries from today on (today's past ones too: a breakdown this morning).
pub fn coming(c: &Company, k: &Known, n: usize) -> Vec<Entry> {
    let mut v = agenda(c, &c.date, &dates::add(&c.date, AHEAD), k);
    v.truncate(n);
    v
}

fn staff(c: &Company, out: &mut Vec<Entry>) {
    let realistic = c.game_mode == GameMode::Realistic;
    for e in &c.staff {
        let who = || About::Person(e.id);
        // holidays approved and asked for
        for r in &e.leave.requests {
            match r.state {
                Asked::Approved => {
                    out.push(Entry::new(&r.from, Kind::Holiday, ON_HOLIDAY, Go::Holidays(r.from.clone())).arg("name", &e.name).arg("until", &r.until).about(who()));
                    out.push(Entry::new(&dates::add(&r.until, 1), Kind::HolidayBack, BACK_FROM_HOLIDAY, Go::Person(e.id)).arg("name", &e.name).about(who()));
                }
                Asked::Asked => {
                    out.push(Entry::new(&r.from, Kind::HolidayAsked, ASKED, Go::Holidays(r.from.clone())).note(NOT_ANSWERED).arg("name", &e.name).arg("until", &r.until).sev(Severity::Warn).about(who()).mine(true));
                    let (note, sev) = if realistic { (SILENCE_IS_NO, Severity::Bad) } else { (PLANNER_ANSWERS, Severity::Warn) };
                    out.push(Entry::new(&r.due, Kind::HolidayDue, ANSWER_HOLIDAY, Go::Holidays(r.from.clone())).note(note).arg("name", &e.name).arg("from", &r.from).arg("until", &r.until).sev(sev).about(who()).mine(true));
                }
                Asked::Refused => {}
            }
        }
        // (a holiday begun before the requests)
        if let Some(u) = &e.holiday_until {
            if !e.leave.requests.iter().any(|r| r.state == Asked::Approved && r.until == *u) {
                out.push(Entry::new(&dates::add(u, 1), Kind::HolidayBack, BACK_FROM_HOLIDAY, Go::Person(e.id)).arg("name", &e.name).about(who()));
            }
        }
        if let Some(u) = &e.sick_until {
            out.push(Entry::new(&dates::add(u, 1), Kind::Recovered, BACK_AFTER_ILLNESS, Go::Person(e.id)).arg("name", &e.name).about(who()));
        }
        // (a course of an older file, without its booking)
        if let Some(u) = &e.training_until {
            if !c.progress.courses.iter().any(|k| k.employee == Some(e.id) && k.until == *u) {
                out.push(Entry::new(u, Kind::Course, COURSE_ENDS_PLAIN, Go::Person(e.id)).arg("name", &e.name).about(who()));
            }
        }
        out.push(Entry::new(&e.hired, Kind::Hired, STARTS, Go::Person(e.id)).arg("name", &e.name).arg("role", e.role.label()).about(who()));
        if let Some(u) = &e.notice_until {
            let x = Entry::new(u, Kind::Leaving, LAST_DAY, Go::Person(e.id)).arg("name", &e.name).about(who());
            out.push(if e.resigned { x.note(RESIGNED).sev(Severity::Warn) } else { x.note(DISMISSED) });
        }
    }
    for k in c.progress.courses.iter().filter(|k| !k.done) {
        out.push(match k.employee {
            Some(id) => Entry::new(&k.until, Kind::Course, COURSE_ENDS, Go::Person(id)).arg("name", c.employee(id).map(|e| e.name.as_str()).unwrap_or(&k.name)).arg("what", k.kind.label()).about(About::Person(id)),
            None => Entry::new(&k.until, Kind::Course, YOUR_COURSE_ENDS, Go::Courses).arg("what", k.kind.label()).about(About::Player),
        });
    }
    for a in &c.staff_rules.asks {
        let Some(e) = c.employee(a.id) else { continue };
        out.push(Entry::new(&a.until, Kind::Raise, ANSWER_RAISE, Go::Person(a.id)).note(RAISE_NOTE).arg("name", &e.name).arg("pct", format!("{:.0}", a.share * 100.0)).sev(Severity::Warn).about(About::Person(a.id)).mine(true));
    }
}

/// What came of a breakdown.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    Waiting,
    Spare { bus: String, arrive: String },
    Rental { arrive: String },
    Dropped,
    Unknown,
}

/// A breakdown or accident of today or the last days.
#[derive(Clone, Debug, PartialEq)]
struct Broke {
    vehicle: u32,
    date: String,
    minute: Option<i64>,
    bus: String,
    line: String,
    accident: bool,
    outcome: Outcome,
}

/// "HH:MM" as a minute of the day.
fn minute_of_text(t: &str) -> Option<i64> {
    let (h, m) = t.trim().split_once(':')?;
    Some(h.trim().parse::<i64>().ok()? * 60 + m.trim().get(..2).unwrap_or(m.trim()).parse::<i64>().ok()?)
}

/// Today's breakdowns and accidents as the clock has them (those that happened by now), and
/// the last days' as the inbox told them (the newest message of each).
fn breakdowns(c: &Company) -> Vec<Broke> {
    let mut out = Vec::new();
    if let Some(s) = c.clock.today.as_ref().filter(|s| s.date == c.date) {
        for e in s.events.iter().filter(|e| e.done) {
            let What::Breakdown { index } = e.what else { continue };
            let Some(b) = s.breaks.get(index) else { continue };
            let arrive = clock::hhmm(clock::moment(&s.date, b.back.unwrap_or(b.at) as i64));
            let outcome = match b.choice {
                None => Outcome::Waiting,
                Some(Choice::Spare { vehicle }) => Outcome::Spare { bus: c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default(), arrive },
                Some(Choice::Rental) => Outcome::Rental { arrive },
                Some(Choice::Drop) => Outcome::Dropped,
            };
            out.push(Broke { vehicle: b.vehicle, date: s.date.clone(), minute: Some(b.at as i64), bus: b.bus.clone(), line: b.line.clone(), accident: b.accident, outcome });
        }
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for m in c.inbox.messages.iter().rev() {
        let Some(rest) = m.key.strip_prefix("breakdown-") else { continue };
        if !seen.insert(m.key.as_str()) || m.date.as_str() >= c.date.as_str() {
            continue;
        }
        let Some(vehicle) = rest.rsplit_once('-').and_then(|(_, v)| v.parse::<u32>().ok()) else { continue };
        let arrive = m.value("arrive").unwrap_or("").to_string();
        let outcome = match m.text.as_str() {
            t if t == clock::BY_SPARE || t == clock::BY_SPARE_UNTIL => Outcome::Spare { bus: m.value("spare").unwrap_or("").to_string(), arrive },
            t if t == clock::BY_RENTAL => Outcome::Rental { arrive },
            t if t == clock::DROPPED => Outcome::Dropped,
            _ => Outcome::Unknown,
        };
        out.push(Broke {
            vehicle,
            date: m.date.clone(),
            minute: m.value("time").and_then(minute_of_text),
            bus: m.value("bus").unwrap_or("").to_string(),
            line: m.value("n").unwrap_or("").to_string(),
            accident: m.title == clock::CRASH_TITLE,
            outcome,
        });
    }
    out
}

/// The kilometres a bus runs today as planned.
fn bus_km(p: &Plan, id: u32) -> f64 {
    p.tours.iter().filter(|t| t.bus == Some(id) && !t.tour.unplanned).map(|t| t.tour.km()).sum()
}

/// The kilometres a bus of the fleet ran a day over the last closed days (up to two weeks).
fn fleet_daily_km(c: &Company) -> f64 {
    let days: Vec<f64> = c.history.iter().rev().take(14).map(|d| d.km).collect();
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count().max(1) as f64;
    if days.is_empty() {
        0.0
    } else {
        days.iter().sum::<f64>() / days.len() as f64 / buses
    }
}

fn fleet(c: &Company, k: &Known, broken: &[Broke], out: &mut Vec<Entry>) {
    let today = c.date.as_str();
    let tomorrow = dates::add(today, 1);
    for b in broken {
        let past = b.date.as_str() < today;
        let go = if past { Go::Bus(b.vehicle) } else { Go::Planning(b.date.clone()) };
        let mut x = Entry::new(&b.date, if b.accident { Kind::Accident } else { Kind::Breakdown }, if b.accident { CRASHED } else { BROKE_DOWN }, go).arg("bus", &b.bus).arg("n", &b.line).about(About::Bus(b.vehicle));
        if let Some(m) = b.minute {
            x = x.at(m);
        }
        out.push(match &b.outcome {
            Outcome::Waiting => x.note(WAITING).sev(Severity::Bad).mine(true),
            Outcome::Spare { bus, arrive } => x.note(SPARE_SENT).arg("spare", bus).arg("arrive", arrive).sev(Severity::Warn),
            Outcome::Rental { arrive } => x.note(RENTAL_SENT).arg("arrive", arrive).sev(Severity::Warn),
            Outcome::Dropped => x.note(TRIPS_DROPPED).sev(Severity::Bad),
            Outcome::Unknown => x.sev(Severity::Warn),
        });
    }
    let daily = fleet_daily_km(c);
    for v in &c.fleet {
        let about = || About::Bus(v.id);
        // back from the workshop: after its job, its breakdown or its accident
        if let Some(u) = &v.workshop_until {
            let mut x = Entry::new(&dates::add(u, 1), Kind::WorkshopBack, BACK_FROM_WORKSHOP, Go::Bus(v.id)).arg("bus", &v.number).about(about());
            if let Some(j) = c.site.jobs.iter().find(|j| j.vehicle == v.id && j.until.as_deref() == Some(u.as_str())) {
                x = x.note(JOB_DONE).arg("job", j.kind.label());
            } else if let Some(b) = broken.iter().find(|b| b.vehicle == v.id && b.date.as_str() <= u.as_str() && dates::between(&b.date, u) <= 7) {
                x = x.note(if b.accident { AFTER_ACCIDENT } else { AFTER_BREAKDOWN });
            }
            out.push(x);
        }
        match &v.tenure {
            Tenure::Leased { until, .. } => out.push(Entry::new(until, Kind::GoesBack, LEASE_ENDS, Go::Bus(v.id)).note(LEAVES_FLEET).arg("bus", &v.number).sev(Severity::Warn).about(about())),
            Tenure::Rented { until, .. } => out.push(Entry::new(until, Kind::GoesBack, RENTAL_ENDS, Go::Bus(v.id)).note(LEAVES_FLEET).arg("bus", &v.number).sev(Severity::Warn).about(about())),
            Tenure::Owned { .. } => {}
        }
        // its next service, at the kilometres it runs a day (today's as planned, else the
        // fleet's of the last days); a service ordered at the workshop is said there
        if v.held_on(today) && !c.site.jobs.iter().any(|j| j.vehicle == v.id && j.kind == JobKind::Service) {
            let per_day = k.today.map(|p| bus_km(p, v.id)).filter(|x| *x > 0.0).unwrap_or(daily);
            let left = v.next_service_km - v.km;
            // (in the workshop: counted from the day it is back)
            let start = v.workshop_until.as_deref().filter(|u| *u >= today).map(|u| dates::add(u, 1)).unwrap_or_else(|| today.to_string());
            // (the night after the day it passes the reading sends it in)
            let days = if left <= 0.0 { Some(0) } else if per_day > 0.0 { Some(((left / per_day).ceil() as i64 - 1).max(0)) } else { None };
            if let Some(d) = days {
                out.push(Entry::new(&dates::add(&start, d), Kind::ServiceDue, SERVICE_DUE, Go::Bus(v.id)).note(ESTIMATED).arg("bus", &v.number).arg("km", format!("{:.0}", per_day)).sev(Severity::Warn).about(about()));
            }
        }
    }
    for w in &c.dealer.warranties {
        let Some(v) = c.vehicle(w.vehicle) else { continue };
        out.push(Entry::new(&w.until, Kind::Warranty, WARRANTY_ENDS, Go::Bus(v.id)).arg("bus", &v.number).about(About::Bus(v.id)));
    }
    for o in &c.dealer.orders {
        let mut x = Entry::new(&dealer::day_of(&o.delivery), Kind::Delivery, DELIVERY, Go::Dealer).arg("count", o.contract.count).arg("name", &o.contract.listing.bus.name);
        if let Some(m) = dealer::minutes_of(&o.delivery) {
            x = x.at(m.rem_euclid(24 * 60));
        }
        out.push(x);
    }
    for j in c.site.jobs.iter().filter(|j| j.started.is_none()) {
        let Some(v) = c.vehicle(j.vehicle) else { continue };
        out.push(if depot::starts_tomorrow(c, j.id) {
            Entry::new(&tomorrow, Kind::WorkshopJob, INTO_WORKSHOP, Go::Bus(v.id)).arg("bus", &v.number).arg("job", j.kind.label()).about(About::Bus(v.id))
        } else {
            Entry::new(today, Kind::WorkshopJob, WAITS_BAY, Go::Bus(v.id)).arg("bus", &v.number).arg("job", j.kind.label()).sev(Severity::Warn).about(About::Bus(v.id))
        });
    }
    for w in &c.stores.waiting {
        out.push(Entry::new(today, Kind::PartsWait, WAITS_PARTS, Go::Workshop).note(SINCE).arg("bus", &w.number).arg("date", &w.since).sev(Severity::Warn).about(About::Bus(w.vehicle)).mine(!c.stores.auto));
    }
    for o in &c.stores.orders {
        out.push(Entry::new(&clock::date_of(o.due), Kind::PartsArrive, PARTS_ARRIVE, Go::Workshop).at(clock::minute_of(o.due)).note(PARTS_COUNT).arg("count", o.count()));
    }
    for w in &c.site.works {
        out.push(Entry::new(&w.until, Kind::Building, BUILDING_DONE, Go::Depot).arg("area", w.area.label()));
    }
}

fn lines(c: &Company, k: &Known, out: &mut Vec<Entry>) {
    let today = c.date.as_str();
    let tomorrow = dates::add(today, 1);
    let now = clock::now(c);
    let casual = c.game_mode.casual();
    for (p, date, sev) in [(k.today, today.to_string(), Severity::Bad), (k.tomorrow, tomorrow.clone(), Severity::Warn)] {
        let Some(p) = p else { continue };
        let n = p.uncovered();
        if n > 0 {
            let (buses, duties) = p.short_of();
            out.push(Entry::new(&date, Kind::Uncovered, NOT_COVERED, Go::Planning(date.clone())).note(SHORT_OF).arg("count", n).arg("buses", buses).arg("duties", duties).sev(sev).mine(true));
        }
    }
    for l in &c.lines {
        let about = || About::Line(l.name.clone());
        match l.service_from {
            None => {
                // (its name where it has one: a map's line and an own line may share a number)
                let x = Entry::new(today, Kind::NotInService, NOT_IN_SERVICE, Go::Line(l.name.clone())).arg("n", &l.number).sev(Severity::Warn).about(about()).mine(true);
                let name = l.public_name();
                out.push(if name.is_empty() { x.note(PLAN_IT) } else { x.note(PLAN_NAMED).arg("name", name) });
            }
            Some(m) if m != LEGACY_SERVICE && m > now => out.push(Entry::new(&clock::date_of(m), Kind::LineStarts, LINE_STARTS, Go::Line(l.name.clone())).at(clock::minute_of(m)).arg("n", &l.number).about(about())),
            Some(_) => {}
        }
        if let Some(p) = &l.pending {
            out.push(Entry::new(&p.from, Kind::LineChange, LINE_CHANGES, Go::Line(l.name.clone())).arg("n", if p.number.is_empty() { &l.number } else { &p.number }).about(about()));
        }
    }
    for t in c.concessions.tenders.iter().filter(|t| t.open()) {
        // (a concession of the company's put out again: bid to keep it - Casual renews by itself)
        let keep = t.renewal && !casual;
        let sev = if keep { Severity::Warn } else { Severity::Info };
        if t.opens_at > now {
            out.push(Entry::new(&clock::date_of(t.opens_at), Kind::TenderOpens, if t.renewal { RENEWAL_OPENS } else { TENDER_OPENS }, Go::Concessions).at(clock::minute_of(t.opens_at)).arg("n", &t.number).sev(sev).about(About::Tender(t.id)).mine(keep));
        }
        if t.closes_at > now {
            let x = Entry::new(&clock::date_of(t.closes_at), Kind::TenderCloses, TENDER_CLOSES, Go::Concessions).at(clock::minute_of(t.closes_at)).arg("n", &t.number).sev(sev).about(About::Tender(t.id)).mine(keep);
            out.push(if keep { x.note(KEEP_IT) } else { x });
        }
    }
    for h in &c.concessions.held {
        out.push(Entry::new(&h.from, Kind::ConcessionBegins, CONCESSION_BEGINS, Go::Concessions).arg("n", &h.number).about(About::Line(h.line.clone())));
        let renewing = c.concessions.tenders.iter().any(|t| t.open() && t.renewal && t.line.eq_ignore_ascii_case(&h.line));
        let x = Entry::new(&h.until, Kind::ConcessionEnds, CONCESSION_ENDS, Go::Concessions).arg("n", &h.number).sev(Severity::Warn).about(About::Line(h.line.clone()));
        out.push(if renewing { x.note(RENEWAL_OUT) } else { x });
    }
    for g in c.contracts.goals.iter().filter(|g| g.state == ct::State::Running) {
        let Some(d) = &g.deadline else { continue };
        let (text, args) = g.aim.title();
        let mut x = Entry::new(d, Kind::Goal, text, Go::Contracts(0)).note(GOAL_LAST_DAY).about(About::Goal(g.id));
        x.args = args;
        let x = x.arg("done", format!("{:.0}", (g.share() * 100.0).floor()));
        out.push(if g.share() < 1.0 { x.sev(Severity::Warn) } else { x });
    }
    let contract = |date: &str, text: &'static str| Entry::new(date, Kind::Contract, text, Go::Contracts(1));
    if let Some(f) = &c.contracts.fuel {
        if f.from.as_str() > today {
            out.push(contract(&f.from, FUEL_BEGINS));
        }
        out.push(contract(&f.until, FUEL_ENDS).sev(Severity::Warn));
    }
    if let Some(cl) = &c.contracts.cleaning {
        if cl.since.as_str() > today {
            out.push(contract(&cl.since, CLEANING_BEGINS));
        }
        if let Some(u) = &cl.until {
            out.push(contract(u, CLEANING_ENDS));
        }
    }
    let provider = |i: usize| ct::PROVIDERS[i.min(ct::PROVIDERS.len() - 1)].name;
    if let Some(n) = c.contracts.internet.as_ref().filter(|n| n.since.as_str() > today) {
        out.push(contract(&n.since, INTERNET_BEGINS).arg("who", provider(n.provider)));
    }
    match &c.contracts.internet_next {
        Some(NetChange::Take { provider: p, .. }) => out.push(contract(&tomorrow, INTERNET_CHANGES).arg("who", provider(*p))),
        Some(NetChange::Cancel) => out.push(contract(&tomorrow, INTERNET_ENDS)),
        None => {}
    }
    for a in c.adverts.contracts.iter().filter(|a| a.ended.is_none()) {
        out.push(Entry::new(&a.until, Kind::Advert, ADVERT_ENDS, Go::Adverts).arg("who", &a.advertiser));
    }
}

/// The last day of the month `k` months after `date`'s.
fn month_end(date: &str, k: i64) -> String {
    let Some(d) = dates::parse(date) else { return date.to_string() };
    let (y, m, _) = dates::civil_from_days(d);
    let n = y as i64 * 12 + (m as i64 - 1) + k;
    let (y, m) = (n.div_euclid(12) as i32, (n.rem_euclid(12) + 1) as u32);
    dates::fmt(dates::days_from_civil(y, m, dates::days_in_month(y, m)))
}

/// Months from `a`'s to `b`'s.
fn months_between(a: &str, b: &str) -> i64 {
    let ym = |d: &str| d.get(..4).and_then(|y| y.parse::<i64>().ok()).unwrap_or(0) * 12 + d.get(5..7).and_then(|m| m.parse::<i64>().ok()).unwrap_or(1);
    ym(b) - ym(a)
}

fn money(c: &Company, k: &Known, from: &str, until: &str, out: &mut Vec<Entry>) {
    let today = c.date.as_str();
    // the month's close: its wages, leases, insurance and the loans' rates booked
    if let (Some(a), Some(b)) = (dates::parse(from), dates::parse(until)) {
        for d in a..=b {
            let date = dates::fmt(d);
            if !dates::last_of_month(&date) {
                continue;
            }
            let ahead = months_between(today, &date);
            let rates: i64 = c.loans.iter().filter(|l| l.months_left as i64 > ahead).map(|l| l.monthly).sum();
            let x = Entry::new(&date, Kind::MonthClose, MONTH_CLOSE, Go::Finances);
            out.push(if rates > 0 && ahead >= 0 { x.note(LOAN_RATES).arg("amount", rates) } else { x });
        }
    }
    for l in c.loans.iter().filter(|l| l.months_left > 0) {
        let name = if l.purpose.trim().is_empty() { LOAN } else { l.purpose.trim() };
        out.push(Entry::new(&month_end(today, l.months_left as i64 - 1), Kind::LoanEnds, LOAN_LAST, Go::Finances).arg("name", name).arg("amount", l.monthly));
    }
    if let Some(r) = &c.research.running {
        let what = research::topic(&r.topic).map(|t| t.title).unwrap_or("");
        out.push(match research::days_left(c) {
            Some(d) => Entry::new(&dates::add(today, (d as i64 - 1).max(0)), Kind::Research, RESEARCH_DONE, Go::Research).note(AT_PACE).arg("what", what),
            None => Entry::new(today, Kind::Research, RESEARCH_IDLE, Go::Research).arg("what", what).sev(Severity::Warn).mine(true),
        });
    }
    if let Some(cal) = k.calendar {
        for (d, name) in &cal.public {
            out.push(Entry::new(d, Kind::PublicHoliday, PUBLIC_HOLIDAY, Go::Planning(d.clone())).arg("what", name));
        }
        for (a, b, name) in &cal.school {
            out.push(Entry::new(a, Kind::SchoolHoliday, SCHOOL_BEGINS, Go::Planning(a.clone())).note(UNTIL).arg("what", name).arg("until", b));
            out.push(Entry::new(&dates::add(b, 1), Kind::SchoolHoliday, SCHOOL_ENDS, Go::Planning(dates::add(b, 1))).arg("what", name));
        }
    }
    for e in &c.inbox.events {
        // (its key: the pages' icon for it)
        let mut x = Entry::new(&e.from, Kind::WorldEvent, e.title(), Go::Inbox).arg("event", &e.key);
        if let Some(n) = &e.line {
            x = x.arg("n", n);
        }
        if e.until != e.from {
            x = x.note(FROM_UNTIL).arg("from", &e.from).arg("until", &e.until);
        }
        // (as the inbox colours it: dearer diesel, fewer riders or delays)
        out.push(if e.fuel > 1.0 || e.demand < 1.0 || e.late > 0.0 { x.sev(Severity::Warn) } else { x });
    }
}

// --- the calendar -----------------------------------------------------------------------------

/// The public and school holidays the company knows of: the map's (`Holidays.txt`); where it
/// names no public holiday the common ones the company's own lines run by
/// (`service::common_public_holiday`), and where it names no school holidays the periods the
/// player gave the line registry (none: none - the timetable's own default periods are a guess,
/// not the map's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Calendar {
    /// (the day, its name)
    pub public: Vec<(String, String)>,
    /// (the first day, the last, its name)
    pub school: Vec<(String, String, String)>,
}

/// A date code (YYYYMMDD) as a date.
fn of_code(code: i32) -> Option<String> {
    service::day_of_code(code).map(dates::fmt)
}

impl Calendar {
    /// The calendar for the years of `from` to `until`.
    pub fn new(map: &omsi_map::Calendar, periods: &[HolidayPeriod], from: &str, until: &str) -> Calendar {
        let year = |d: &str| d.get(..4).and_then(|y| y.parse::<i32>().ok()).unwrap_or(2000);
        let (y0, y1) = (year(from), year(until).max(year(from)));
        let mut out = Calendar::default();
        if map.holidays.is_empty() {
            for y in y0..=y1 {
                let (em, ed) = service::easter(y);
                let easter = dates::days_from_civil(y, em, ed);
                let fixed = [(1, 1, NEW_YEAR), (5, 1, LABOUR_DAY), (12, 25, CHRISTMAS), (12, 26, SECOND_CHRISTMAS)].map(|(m, d, n)| (dates::days_from_civil(y, m, d), n));
                let moving = [(-2, GOOD_FRIDAY), (1, EASTER_MONDAY), (39, ASCENSION), (50, WHIT_MONDAY)].map(|(k, n)| (easter + k, n));
                out.public.extend(fixed.into_iter().chain(moving).map(|(d, n)| (dates::fmt(d), n.to_string())));
            }
        } else {
            out.public = map.holidays.iter().filter_map(|h| Some((of_code(h.date)?, h.name.trim().to_string()))).collect();
        }
        if map.ranges.is_empty() {
            for p in periods.iter().filter(|p| p.valid()) {
                let (Some(a), Some(b)) = (service::month_day(&p.from), service::month_day(&p.to)) else { continue };
                // (a period over the new year begins in the year before)
                for y in (y0 - 1)..=y1 {
                    let end_year = if a <= b { y } else { y + 1 };
                    let first = dates::days_from_civil(y, a / 100, a % 100);
                    let last = dates::days_from_civil(end_year, b / 100, b % 100);
                    out.school.push((dates::fmt(first), dates::fmt(last), p.name.trim().to_string()));
                }
            }
        } else {
            out.school = map.ranges.iter().filter_map(|r| Some((of_code(r.start)?, of_code(r.end)?, r.name.trim().to_string()))).collect();
        }
        out.public.retain(|(d, _)| from <= d.as_str() && d.as_str() <= until);
        // (a period that runs into the days shown: its first or its next day may be in them)
        let after = dates::add(until, 1);
        out.school.retain(|(a, b, _)| a.as_str() <= after.as_str() && dates::add(b, 1).as_str() >= from);
        out.public.sort();
        out.school.sort();
        out
    }

    /// The calendar of the company's map (`map`: its `global.cfg` as the company keeps it) for
    /// the years of `from` to `until`: its `Holidays.txt`, and the periods of its line registry.
    pub fn of_map(map: &str, from: &str, until: &str) -> Calendar {
        let cal = crate::resolve_content(map)
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("Holidays.txt")))
            .and_then(|p| omsi_map::Calendar::load(&p).ok())
            .unwrap_or_default();
        let periods = crate::lines::load_registry(&crate::lines::registry_path(&crate::lines::map_folder(map))).school_holidays;
        Calendar::new(&cal, &periods, from, until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::holidays::{Request, State};
    use crate::company::model::{BusKind, BusSize, Drive, Vehicle};
    use crate::company::{concessions::Concession, found, staff, Founding};

    fn company() -> Company {
        found(&Founding { name: "Agenda".into(), game_mode: GameMode::Normal, date: "2026-10-15".into(), ..Default::default() }, "Luc")
    }

    fn bus(id: u32, number: &str) -> Vehicle {
        Vehicle {
            id,
            number: number.into(),
            plate: String::new(),
            bus: "Vehicles/x.bus".into(),
            name: "Citaro".into(),
            kind: BusKind { size: BusSize::Solo, drive: Drive::Diesel },
            livery: String::new(),
            house_livery: None,
            built: "2020-01-01".into(),
            km: 0.0,
            condition: 90.0,
            next_service_km: 30_000.0,
            tenure: Tenure::Owned { paid: 0, new_value: 0 },
            acquired: "2026-01-01".into(),
            workshop_until: None,
            breakdowns: 0,
            clean: 75.0,
            region: None,
        }
    }

    /// The four Luc asked for first - a holiday, a bus back from the workshop, a concession
    /// beginning and a service due - each where it belongs, in the order of their days.
    #[test]
    fn the_agenda_holds_what_is_coming_in_date_order() {
        let mut c = company();
        // a driver with an approved holiday from the 20th to the 24th
        let a = staff::applicants(&c).into_iter().next().expect("an applicant");
        let id = staff::hire(&mut c, &a).expect("hired");
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        e.leave.requests.push(Request { no: 1, from: "2026-10-20".into(), until: "2026-10-24".into(), asked: "2026-10-01".into(), due: "2026-10-08".into(), state: State::Approved, ..Default::default() });
        // a bus in the workshop until the 16th, and one 2,000 km from its service at 500 km a day
        let mut b1 = bus(1, "101");
        b1.workshop_until = Some("2026-10-16".into());
        b1.km = 10_000.0;
        let mut b2 = bus(2, "102");
        b2.km = 28_000.0;
        c.fleet = vec![b1, b2];
        c.history = vec![crate::company::DayRecord { date: "2026-10-14".into(), km: 1_000.0, ..Default::default() }];
        // a concession won, beginning on the 1st of November
        c.concessions.held.push(Concession { line: "5".into(), number: "5".into(), from: "2026-11-01".into(), until: "2028-10-31".into(), price: 1.0, direct: false });
        let (from, until) = window(&c);
        let v = agenda(&c, &from, &until, &Known::default());
        let find = |kind: Kind| v.iter().find(|e| e.kind == kind).unwrap_or_else(|| panic!("no {kind:?} in {v:#?}"));
        let holiday = find(Kind::Holiday);
        assert_eq!((holiday.date.as_str(), holiday.value("until")), ("2026-10-20", Some("2026-10-24")));
        assert_eq!(holiday.go, Go::Holidays("2026-10-20".into()));
        assert_eq!(find(Kind::HolidayBack).date, "2026-10-25");
        let back = find(Kind::WorkshopBack);
        assert_eq!((back.date.as_str(), back.value("bus"), &back.go), ("2026-10-17", Some("101"), &Go::Bus(1)));
        let begins = find(Kind::ConcessionBegins);
        assert_eq!((begins.date.as_str(), begins.value("n")), ("2026-11-01", Some("5")));
        // (1,000 km a day for two buses: 500 a day each; 2,000 km left: the night of the 18th)
        let due: Vec<&Entry> = v.iter().filter(|e| e.kind == Kind::ServiceDue).collect();
        assert_eq!(due.len(), 2, "{due:#?}");
        let b2 = due.iter().find(|e| e.value("bus") == Some("102")).unwrap();
        assert_eq!((b2.date.as_str(), b2.value("km")), ("2026-10-18", Some("500")));
        // (the bus in the workshop counts from the day it is back: 20,000 km at 500 a day)
        let b1 = due.iter().find(|e| e.value("bus") == Some("101")).unwrap();
        assert_eq!(b1.date, "2026-11-25");
        // every entry in its days, in date order
        assert!(v.iter().all(|e| from.as_str() <= e.date.as_str() && e.date.as_str() <= until.as_str()));
        assert!(v.windows(2).all(|w| (w[0].date.as_str(), w[0].minute.unwrap_or(-1)) <= (w[1].date.as_str(), w[1].minute.unwrap_or(-1))));
        let order: Vec<Kind> = v.iter().filter(|e| matches!(e.kind, Kind::Holiday | Kind::WorkshopBack | Kind::ConcessionBegins) || (e.kind == Kind::ServiceDue && e.value("bus") == Some("102"))).map(|e| e.kind).collect();
        assert_eq!(order, vec![Kind::WorkshopBack, Kind::ServiceDue, Kind::Holiday, Kind::ConcessionBegins]);
        // (the coming entries start today)
        assert!(coming(&c, &Known::default(), 50).iter().all(|e| e.date.as_str() >= "2026-10-15"));
    }

    #[test]
    fn requests_and_breakdowns_want_the_player() {
        let mut c = company();
        let a = staff::applicants(&c).into_iter().next().expect("an applicant");
        let id = staff::hire(&mut c, &a).expect("hired");
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        e.leave.requests.push(Request { no: 2, from: "2026-11-02".into(), until: "2026-11-06".into(), asked: "2026-10-14".into(), due: "2026-10-21".into(), state: State::Asked, ..Default::default() });
        c.fleet = vec![bus(1, "101")];
        // a breakdown this morning, not decided yet
        c.clock.minute = 9 * 60;
        c.clock.today = Some(clock::DayScript {
            date: c.date.clone(),
            events: vec![clock::Timed { at: 8 * 60, what: What::Breakdown { index: 0 }, done: true }],
            breaks: vec![clock::Broken { vehicle: 1, bus: "101".into(), line: "5".into(), at: 8 * 60, until: 18 * 60, line_name: "5".into(), choice: None, cost: 0, accident: false, back: None, spare_until: None }],
            busy: Vec::new(),
        });
        let (from, until) = window(&c);
        let v = agenda(&c, &from, &until, &Known::default());
        let due = v.iter().find(|e| e.kind == Kind::HolidayDue).expect("the answer due");
        assert!(due.needs_me && due.date == "2026-10-21" && due.severity == Severity::Warn);
        let broke = v.iter().find(|e| e.kind == Kind::Breakdown).expect("the breakdown");
        assert!(broke.needs_me && broke.minute == Some(8 * 60) && broke.note == Some(WAITING) && broke.severity == Severity::Bad);
        assert_eq!(broke.go, Go::Planning(c.date.clone()));
        // decided: a rental bus took over
        c.clock.today.as_mut().unwrap().breaks[0].choice = Some(Choice::Rental);
        c.clock.today.as_mut().unwrap().breaks[0].back = Some(9 * 60);
        let v = agenda(&c, &from, &until, &Known::default());
        let broke = v.iter().find(|e| e.kind == Kind::Breakdown).unwrap();
        assert!(!broke.needs_me && broke.note == Some(RENTAL_SENT) && broke.value("arrive") == Some("09:00"));
    }

    #[test]
    fn the_calendar_names_the_common_holidays_where_the_map_has_none() {
        let cal = Calendar::new(&omsi_map::Calendar::default(), &[], "2026-12-01", "2027-04-30");
        let days: Vec<&str> = cal.public.iter().map(|x| x.0.as_str()).collect();
        // Christmas, New Year, Good Friday and Easter Monday 2027 (Easter Sunday the 28th of March)
        assert_eq!(days, vec!["2026-12-25", "2026-12-26", "2027-01-01", "2027-03-26", "2027-03-29"]);
        assert!(cal.school.is_empty());
        let mut map = omsi_map::Calendar::default();
        map.ranges.push(omsi_map::HolidayRange { start: 20261019, end: 20261030, name: "Herbstferien".into() });
        let cal = Calendar::new(&map, &[HolidayPeriod::new("Kermis", "09-01", "09-07")], "2026-10-12", "2026-11-23");
        assert_eq!(cal.school, vec![("2026-10-19".to_string(), "2026-10-30".to_string(), "Herbstferien".to_string())]);
        // (the registry's periods where the map has none, over the new year too)
        let cal = Calendar::new(&omsi_map::Calendar::default(), &[HolidayPeriod::new("Winter", "12-22", "01-05")], "2026-12-01", "2027-01-31");
        assert_eq!(cal.school, vec![("2026-12-22".to_string(), "2027-01-05".to_string(), "Winter".to_string())]);
        assert_eq!(month_end("2026-10-15", 0), "2026-10-31");
        assert_eq!(month_end("2026-11-30", 3), "2027-02-28");
    }
}

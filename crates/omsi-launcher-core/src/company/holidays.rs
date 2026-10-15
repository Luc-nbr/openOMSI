//! Holidays asked for and planned (Beta 57, a tester: "a system where we can plan drivers
//! holidays and they put requests in as in reality this many drivers will not be off together, if
//! their requests are always rejected then could effect driver morale"). Before, someone went on
//! holiday from one night to the next by chance, so several drivers were away at once and their
//! duties broke on the day.
//!
//! Now everyone asks ahead (`night`, `ask`): for the days of their year's entitlement
//! (`staff::HOLIDAYS`) not asked for yet, a few times a year - for the summer and for Christmas
//! most, seldom at short notice -, with a reason (`Reason`), one request at a time. The company
//! sets how many drivers may be off on one day (`HolidayRules::max_off`; by default a quarter of
//! the fleet, `default_limit`) and answers each request:
//!
//! * approved, the days are theirs: the planning knows them ahead (`Employee::on_holiday`), so the
//!   dispatcher covers their duties with a free colleague without touching the roster
//!   (`plan::day_plan`) - nothing breaks on the day; they are a little more content;
//! * refused, they are less content, the more the more often it happened to them this year
//!   (`refusal_cost`), a little more stressed (illness), and it weighs on them for half a year
//!   (`grudge`): low satisfaction makes them late, unhappy, and at last they leave
//!   (`staff::after_day`).
//!
//! Who answers goes by the game mode: Casual approves within the limit by itself, Normal's
//! planner suggests an answer (`advice`) and gives it when the request is due, Realistic leaves
//! every answer to the player - no answer by the day it is due is a no. The days are taken from
//! what is left of the year's when the holiday begins (told as `StaffNote::Holiday`).

use super::dates;
use super::inbox::{self, Kind, Message};
use super::model::{Company, Employee, GameMode, Role};
use super::rng::Rng;
use super::staff::{StaffNote, HOLIDAYS};
use serde::{Deserialize, Serialize};

/// The chance a night that someone with all their year's days left asks for some.
const CHANCE: f64 = 0.02;
/// Of the requests, those at short notice (within the week).
const SHORT_NOTICE: f64 = 0.1;
/// Days someone works here before they ask for a holiday.
const SETTLE: i64 = 28;
/// Days the company has to answer (at most until the day before the holiday).
pub const ANSWER_DAYS: i64 = 7;
/// What an approval does to their satisfaction (and after a refusal this year, a little more).
const APPROVED: f64 = 3.0;
/// The most the limit can be set to.
pub const MOST: u32 = 99;

/// Why someone asks for days off.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A week away, a long weekend.
    #[default]
    Break,
    /// The summer holiday (late June to August).
    Summer,
    /// Christmas and the New Year.
    Christmas,
    /// A wedding, a christening, a big birthday: its day is fixed.
    Family,
    /// Moving house, an appointment: a day, often at short notice.
    Personal,
}

/// How much the days matter to them.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
}

impl Priority {
    pub fn label(self) -> &'static str {
        match self {
            Priority::Low => "Low priority",
            Priority::Normal => "Normal priority",
            Priority::High => "High priority",
        }
    }
}

impl Reason {
    pub const ALL: [Reason; 5] = [Reason::Break, Reason::Summer, Reason::Christmas, Reason::Family, Reason::Personal];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Reason::Break => "A short break",
            Reason::Summer => "The summer holiday",
            Reason::Christmas => "Christmas with the family",
            Reason::Family => "A family occasion",
            Reason::Personal => "Personal matters",
        }
    }

    /// The launcher's icon for it.
    pub fn icon(self) -> &'static str {
        match self {
            Reason::Break => "event",
            Reason::Summer => "wb_sunny",
            Reason::Christmas => "stars",
            Reason::Family => "groups",
            Reason::Personal => "home",
        }
    }

    /// A date that cannot move is high, a holiday planned with the family normal, a break low.
    pub fn priority(self) -> Priority {
        match self {
            Reason::Family | Reason::Personal => Priority::High,
            Reason::Summer | Reason::Christmas => Priority::Normal,
            Reason::Break => Priority::Low,
        }
    }
}

/// Where a request stands.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Asked,
    Approved,
    Refused,
}

/// Who gave the answer.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum By {
    #[default]
    Player,
    /// The planner: Casual's own answer, or Normal's when the request was due.
    Planner,
    /// Nobody: no answer by the day it was due (Realistic).
    Silence,
}

/// A request for days off: its days (both counted), why, and its answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Request {
    /// Unique in the company (`HolidayRules::last`).
    pub no: u32,
    pub from: String,
    pub until: String,
    #[serde(default)]
    pub reason: Reason,
    pub asked: String,
    /// The answer is due by the end of this day.
    pub due: String,
    #[serde(default)]
    pub state: State,
    #[serde(default)]
    pub answered: Option<String>,
    #[serde(default)]
    pub by: By,
    /// It began: its days were taken from what is left of the year's.
    #[serde(default)]
    pub begun: bool,
}

impl Request {
    /// Its days, both counted.
    pub fn days(&self) -> i64 {
        dates::between(&self.from, &self.until) + 1
    }

    /// The working days it takes of the year's.
    pub fn working_days(&self) -> u32 {
        working_days(self.days())
    }

    pub fn covers(&self, date: &str) -> bool {
        self.from.as_str() <= date && date <= self.until.as_str()
    }

    fn overlaps(&self, from: &str, until: &str) -> bool {
        self.from.as_str() <= until && from <= self.until.as_str()
    }
}

/// The working days of a holiday of `days` days (five of every seven, as the week has them).
pub fn working_days(days: i64) -> u32 {
    ((days.max(0) * 5 + 6) / 7) as u32
}

/// Someone's holidays asked for, and their answers (`Employee::leave`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Leave {
    pub requests: Vec<Request>,
}

impl Leave {
    /// The approved holiday that covers `date`.
    pub fn approved_on(&self, date: &str) -> Option<&Request> {
        self.requests.iter().find(|r| r.state == State::Approved && r.covers(date))
    }

    /// The request waiting for an answer (one at a time).
    pub fn pending(&self) -> Option<&Request> {
        self.requests.iter().find(|r| r.state == State::Asked)
    }

    /// How often they were refused in the year up to `today`.
    pub fn refused(&self, today: &str) -> u32 {
        self.requests.iter().filter(|r| r.state == State::Refused && r.answered.as_deref().is_some_and(|a| dates::between(a, today) < 365)).count() as u32
    }

    /// The working days asked for or approved and not begun (besides request `but`).
    fn booked(&self, but: Option<u32>) -> u32 {
        self.requests.iter().filter(|r| Some(r.no) != but && !r.begun && r.state != State::Refused).map(Request::working_days).sum()
    }
}

/// The company's holiday rules: how many may be off a day, and the requests' numbers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct HolidayRules {
    /// At most this many drivers off on holiday on one day (None: `default_limit`).
    pub max_off: Option<u32>,
    /// The last request's number.
    pub last: u32,
}

/// The limit until the company sets its own: a quarter of the fleet, at least one.
pub fn default_limit(c: &Company) -> u32 {
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count() as u32;
    buses.div_ceil(4).max(1)
}

/// How many drivers may be off on holiday on one day.
pub fn limit(c: &Company) -> u32 {
    c.staff_rules.holidays.max_off.unwrap_or_else(|| default_limit(c))
}

/// Set the limit (None: by the fleet again).
pub fn set_limit(c: &mut Company, n: Option<u32>) {
    c.staff_rules.holidays.max_off = n.map(|n| n.clamp(1, MOST));
}

/// How many of a role may be off on one day: the drivers by the company's limit, the others a
/// third of them (at least one).
pub fn limit_of(c: &Company, role: Role) -> u32 {
    if role == Role::Driver {
        limit(c)
    } else {
        (c.staff.iter().filter(|e| e.role == role && e.notice_until.is_none()).count() as u32 / 3).max(1)
    }
}

/// How many of `role` are off on holiday on `date` (`but` left out).
pub fn off_on(c: &Company, role: Role, date: &str, but: Option<u32>) -> usize {
    c.staff.iter().filter(|e| e.role == role && Some(e.id) != but && e.employed_on(date) && e.on_holiday(date).is_some()).count()
}

/// The days from `from` to `until`, both counted.
pub fn days_of(from: &str, until: &str) -> Vec<String> {
    let (Some(a), Some(b)) = (dates::parse(from), dates::parse(until)) else { return Vec::new() };
    (a..=b).map(dates::fmt).collect()
}

/// The days approving request `r` of `id` takes their role over its limit: each day, and how
/// many would be off then.
pub fn over_limit(c: &Company, id: u32, r: &Request) -> Vec<(String, u32)> {
    let Some(e) = c.employee(id) else { return Vec::new() };
    let max = limit_of(c, e.role);
    days_of(&r.from, &r.until)
        .into_iter()
        .filter_map(|d| {
            let n = off_on(c, e.role, &d, Some(id)) as u32 + 1;
            (n > max).then_some((d, n))
        })
        .collect()
}

/// The working days someone has left to ask for (`but`: a request not counted).
pub fn free_days(e: &Employee, but: Option<u32>) -> u32 {
    e.holiday_left.saturating_sub(e.leave.booked(but))
}

/// The planner's view of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advice {
    /// Within the limit.
    Fits,
    /// One over the limit, but a date that cannot move.
    Occasion,
    /// One over the limit, but refused twice this year already: fair.
    Fair,
    /// Too many would be off then.
    Over,
    /// Not enough holiday days left.
    NoDays,
}

impl Advice {
    pub fn approve(self) -> bool {
        matches!(self, Advice::Fits | Advice::Occasion | Advice::Fair)
    }

    /// Why, in a few words (English, translated when shown).
    pub fn why(self) -> &'static str {
        match self {
            Advice::Fits => "it stays within the limit",
            Advice::Occasion => "its date cannot move, and it is only one over the limit",
            Advice::Fair => "they were rejected twice this year already",
            Advice::Over => "too many would be off then",
            Advice::NoDays => "they have not enough holiday days left",
        }
    }
}

/// What the planner makes of request `r` of `id`: within the limit it is approved; one over it
/// still, for a date that cannot move or for someone refused twice this year; else refused.
pub fn advice(c: &Company, id: u32, r: &Request) -> Advice {
    let Some(e) = c.employee(id) else { return Advice::Over };
    if r.working_days() > free_days(e, Some(r.no)) {
        return Advice::NoDays;
    }
    let over = over_limit(c, id, r);
    let max = limit_of(c, e.role);
    let excess = over.iter().map(|x| x.1.saturating_sub(max)).max().unwrap_or(0);
    if over.is_empty() {
        Advice::Fits
    } else if excess <= 1 && r.reason.priority() == Priority::High {
        Advice::Occasion
    } else if excess <= 1 && e.leave.refused(&c.date) >= 2 {
        Advice::Fair
    } else {
        Advice::Over
    }
}

/// Request `no` and whose it is.
pub fn find(c: &Company, no: u32) -> Option<(&Employee, &Request)> {
    c.staff.iter().find_map(|e| e.leave.requests.iter().find(|r| r.no == no).map(|r| (e, r)))
}

/// The requests waiting for an answer, the soonest due first.
pub fn waiting(c: &Company) -> Vec<(&Employee, &Request)> {
    let mut v: Vec<(&Employee, &Request)> = c.staff.iter().flat_map(|e| e.leave.requests.iter().filter(|r| r.state == State::Asked).map(move |r| (e, r))).collect();
    v.sort_by(|a, b| a.1.due.cmp(&b.1.due).then(a.1.from.cmp(&b.1.from)).then(a.1.no.cmp(&b.1.no)));
    v
}

/// What a refusal costs of someone's satisfaction: five points, five more for each refusal
/// before it this year (`n` counts this one), twenty at most.
pub fn refusal_cost(n: u32) -> f64 {
    (5.0 * n as f64).min(20.0)
}

/// What the refusals of the last half year weigh on someone's satisfaction: four points each,
/// sixteen at most (taken off what their pay and hours make of it, `staff::after_day`).
pub fn grudge(e: &Employee, today: &str) -> f64 {
    let n = e.leave.requests.iter().filter(|r| r.state == State::Refused && r.answered.as_deref().is_some_and(|a| dates::between(a, today) < 180)).count();
    (4.0 * n as f64).min(16.0)
}

/// Answer request `no`: approved, the days are theirs and they are a little more content (more
/// so after a refusal this year); refused, they are less content - the more the more often it
/// happened to them this year - and a little more stressed. Its messages are marked read.
pub fn answer(c: &mut Company, no: u32, approve: bool, by: By) -> Result<(), &'static str> {
    let today = c.date.clone();
    let Some((id, r)) = find(c, no).map(|(e, r)| (e.id, r.clone())) else { return Err("That request is no longer there.") };
    if r.state != State::Asked {
        return Err("That request has been answered already.");
    }
    if approve && c.employee(id).is_some_and(|e| r.working_days() > free_days(e, Some(no))) {
        return Err("They have not enough holiday days left for it.");
    }
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return Err("That request is no longer there.") };
    let before = e.leave.refused(&today);
    if let Some(q) = e.leave.requests.iter_mut().find(|x| x.no == no) {
        q.state = if approve { State::Approved } else { State::Refused };
        q.answered = Some(today.clone());
        q.by = by;
    }
    if approve {
        e.satisfaction = (e.satisfaction + APPROVED + if before > 0 { 2.0 } else { 0.0 }).min(100.0);
    } else {
        let n = before + 1;
        e.satisfaction = (e.satisfaction - refusal_cost(n)).max(0.0);
        e.stress = (e.stress + 3.0 * n.min(4) as f64).min(100.0);
    }
    for m in c.inbox.messages.iter_mut().filter(|m| m.leave == Some(no)) {
        m.read = true;
    }
    Ok(())
}

/// A message about request `r` of `name`, its days and reason filled in.
fn message(title: &str, text: &str, name: &str, r: &Request) -> Message {
    Message::new(Kind::Staff, title, text).arg("name", name).arg("from", r.from.clone()).arg("until", r.until.clone()).arg("n", r.working_days().to_string()).arg("why", r.reason.label())
}

pub const ASKS: &str = "%{name} asks for a holiday";
pub const ASKS_NORMAL: &str = "%{name} asks for %{from} until %{until} off (%{n} working days). Reason: %{why}. Answer by %{date}, or the planner's suggestion holds.";
pub const ASKS_REALISTIC: &str = "%{name} asks for %{from} until %{until} off (%{n} working days). Reason: %{why}. Answer by %{date}: no answer is a no.";
const CASUAL_YES: (&str, &str) = ("%{name}'s holiday was approved", "The company approved %{name}'s holiday from %{from} until %{until} (%{why}): %{what}.");
const CASUAL_NO: (&str, &str) = ("%{name}'s holiday was rejected", "The company rejected %{name}'s holiday from %{from} until %{until} (%{why}): %{what}. Rejections make people less satisfied; a higher limit on the Staff page's Holidays tab lets more go at once.");
const PLANNER_YES: (&str, &str) = ("The planner approved %{name}'s holiday", "%{name}'s request for %{from} until %{until} was due without your answer: the planner approved it, as %{what}.");
const PLANNER_NO: (&str, &str) = ("The planner rejected %{name}'s holiday", "%{name}'s request for %{from} until %{until} was due without your answer: the planner rejected it, as %{what}. They are less satisfied.");
const SILENCE: (&str, &str) = ("No answer for %{name}", "%{name} asked for %{from} until %{until} off and heard nothing back: they take it as a no, and are less satisfied.");

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![ASKS, ASKS_NORMAL, ASKS_REALISTIC];
    for (a, b) in [CASUAL_YES, CASUAL_NO, PLANNER_YES, PLANNER_NO, SILENCE] {
        v.extend([a, b]);
    }
    v.extend(Reason::ALL.iter().map(|r| r.label()));
    v.extend([Priority::Low, Priority::Normal, Priority::High].iter().map(|p| p.label()));
    v.extend([Advice::Fits, Advice::Occasion, Advice::Fair, Advice::Over, Advice::NoDays].iter().map(|a| a.why()));
    v.extend(["That request is no longer there.", "That request has been answered already.", "They have not enough holiday days left for it."]);
    v
}

/// A request answered without the player: Casual's and Normal's planner by its advice, Realistic's
/// silence a no; told in the inbox.
fn decide(c: &mut Company, no: u32) {
    let Some((id, name, r)) = find(c, no).map(|(e, r)| (e.id, e.name.clone(), r.clone())) else { return };
    let (yes, by) = match c.game_mode {
        GameMode::Realistic => (false, By::Silence),
        _ => (advice(c, id, &r).approve(), By::Planner),
    };
    let why = advice(c, id, &r).why();
    let yes = yes && answer(c, no, true, by).is_ok();
    if !yes {
        let _ = answer(c, no, false, by);
    }
    let (title, text) = match (c.game_mode, yes) {
        (GameMode::Realistic, _) => SILENCE,
        (GameMode::Casual, true) => CASUAL_YES,
        (GameMode::Casual, false) => CASUAL_NO,
        (_, true) => PLANNER_YES,
        (_, false) => PLANNER_NO,
    };
    let level = if yes { super::clock::Level::Minor } else { super::clock::Level::Warn };
    let mut m = message(title, text, &name, &r).arg("what", why).level(level);
    m.leave = Some(no);
    m.read = c.game_mode == GameMode::Casual && yes;
    inbox::post(c, m);
}

/// How much a day is wanted for a holiday against the rest of the year: the summer and Christmas
/// most, the spring's and autumn's school holidays more, the dark months less.
pub fn season(day: i64) -> f64 {
    let (_, m, d) = dates::civil_from_days(day);
    match (m, d) {
        (7 | 8, _) => 3.5,
        (6, 20..) => 2.5,
        (12, 18..) | (1, ..=3) => 3.0,
        (4 | 10, _) => 1.5,
        (5 | 6, _) => 1.3,
        (1 | 3 | 11, _) => 0.6,
        _ => 1.0,
    }
}

/// A holiday of `len` days from day `from`, shortened to the working days left (None: not even
/// one).
fn fit(from: i64, len: i64, reason: Reason, free: u32) -> Option<(i64, i64, Reason)> {
    let mut len = len.max(1);
    while len > 1 && working_days(len) > free {
        len -= 1;
    }
    (working_days(len) <= free).then_some((from, from + len - 1, reason))
}

/// The day on or after `day` that is weekday `wd` (0 Monday).
fn next_weekday(day: i64, wd: i64) -> i64 {
    day + (wd - dates::weekday(day) as i64).rem_euclid(7)
}

/// What someone asks for on day `today` with `free` working days left: seldom at short notice (a
/// family occasion, a personal matter within the week), else two weeks to four months ahead on a
/// day the season makes likely - the summer in whole weeks from a Monday, Christmas round the
/// holidays, at other times a week, a long weekend or an occasion's day or two.
fn draw(rng: &mut Rng, today: i64, free: u32) -> Option<(i64, i64, Reason)> {
    if rng.chance(SHORT_NOTICE) {
        let from = today + rng.int(2, 6);
        return if rng.chance(0.5) { fit(from, rng.int(1, 2), Reason::Family, free) } else { fit(from, 1, Reason::Personal, free) };
    }
    let picks: Vec<i64> = (0..6).map(|_| today + rng.int(14, 120)).collect();
    let all: f64 = picks.iter().map(|&d| season(d)).sum();
    let mut x = rng.range(0.0, all);
    let at = picks.iter().copied().find(|&d| {
        x -= season(d);
        x < 0.0
    });
    let at = at.unwrap_or(picks[0]);
    let (y, m, d) = dates::civil_from_days(at);
    if (m == 6 && d >= 20) || m == 7 || m == 8 {
        let weeks = match rng.f64() {
            x if x < 0.3 => 1,
            x if x < 0.8 => 2,
            _ => 3,
        };
        return fit(next_weekday(at, 0), weeks * 7, Reason::Summer, free);
    }
    if (m == 12 && d >= 15) || (m == 1 && d <= 3) {
        let year = if m == 1 { y - 1 } else { y };
        let from = dates::days_from_civil(year, 12, rng.int(20, 27) as u32).max(today + 14);
        return fit(from, rng.int(3, 9), Reason::Christmas, free);
    }
    match rng.f64() {
        x if x < 0.3 => fit(next_weekday(at, 0), 7, Reason::Break, free),
        x if x < 0.55 => fit(next_weekday(at, 4), rng.int(3, 4), Reason::Break, free),
        x if x < 0.85 => fit(at, rng.int(1, 2), Reason::Family, free),
        _ => fit(at, 1, Reason::Personal, free),
    }
}

/// The night's new requests: anyone employed for four weeks, not leaving, with days of the year's
/// left to ask for and no request waiting, asks now and then - more often with more days left,
/// and in the autumn with many left. Casual answers at once; the others are asked in the inbox.
fn ask(c: &mut Company, today: &str) {
    let Some(day) = dates::parse(today) else { return };
    let (_, month, _) = dates::civil_from_days(day);
    let mut new: Vec<(u32, String, Request)> = Vec::new();
    for e in c.staff.iter().filter(|e| e.notice_until.is_none() && e.employed_on(today) && dates::between(&e.hired, today) >= SETTLE) {
        if e.leave.pending().is_some() {
            continue;
        }
        let free = free_days(e, None);
        if free == 0 {
            continue;
        }
        let mut rng = Rng::of(&[&c.id, "holiday", &e.id.to_string()], day);
        let rush = if month >= 9 && free >= 12 { 1.6 } else { 1.0 };
        if !rng.chance(CHANCE * (free as f64 / HOLIDAYS as f64).clamp(0.2, 1.0) * rush) {
            continue;
        }
        let Some((from, until, reason)) = draw(&mut rng, day, free) else { continue };
        let (from_s, until_s) = (dates::fmt(from), dates::fmt(until));
        // (not over days they have off already)
        let taken = e.leave.requests.iter().any(|r| r.state != State::Refused && r.overlaps(&from_s, &until_s)) || e.holiday_until.as_deref().is_some_and(|u| u >= from_s.as_str());
        if taken {
            continue;
        }
        let due = dates::fmt((day + ANSWER_DAYS).min(from - 1));
        new.push((e.id, e.name.clone(), Request { no: 0, from: from_s, until: until_s, reason, asked: today.to_string(), due, ..Default::default() }));
    }
    for (id, name, mut r) in new {
        c.staff_rules.holidays.last += 1;
        r.no = c.staff_rules.holidays.last;
        let no = r.no;
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
            e.leave.requests.push(r.clone());
        }
        if c.game_mode.casual() {
            decide(c, no);
            continue;
        }
        let text = if c.game_mode == GameMode::Realistic { ASKS_REALISTIC } else { ASKS_NORMAL };
        let mut m = message(ASKS, text, &name, &r).arg("date", r.due.clone()).key(&format!("holiday-{no}"));
        m.leave = Some(no);
        inbox::post(c, m);
    }
}

/// Whether a request is still kept on `today`: waiting, approved until a month after it, refused
/// for a year (the refusals of the year count).
fn keep(r: &Request, today: &str) -> bool {
    match r.state {
        State::Asked => true,
        State::Approved => dates::between(&r.until, today) <= 30,
        State::Refused => r.answered.as_deref().is_none_or(|a| dates::between(a, today) <= 365),
    }
}

/// The night's holidays (`staff::after_day`, on the night after `today`): the requests of those
/// leaving fall away; those due unanswered are answered (Casual and Normal by the planner, a no
/// on Realistic) - Casual answers all at once; the holidays that begin tomorrow take their days
/// from what is left and are told; new requests come; the old ones are let go.
pub fn night(c: &mut Company, today: &str) -> Vec<StaffNote> {
    let tomorrow = dates::add(today, 1);
    let mut notes = Vec::new();
    for e in c.staff.iter_mut().filter(|e| e.notice_until.is_some()) {
        e.leave.requests.retain(|r| r.state != State::Asked);
    }
    let casual = c.game_mode.casual();
    let due: Vec<u32> = c.staff.iter().flat_map(|e| e.leave.requests.iter().filter(|r| r.state == State::Asked && (casual || r.due.as_str() <= today)).map(|r| r.no)).collect();
    for no in due {
        decide(c, no);
    }
    for e in c.staff.iter_mut() {
        let name = e.name.clone();
        let mut took = 0;
        for r in e.leave.requests.iter_mut().filter(|r| r.state == State::Approved && !r.begun && r.from.as_str() <= tomorrow.as_str()) {
            r.begun = true;
            took += r.working_days();
            if r.until.as_str() >= tomorrow.as_str() {
                notes.push(StaffNote::Holiday { name: name.clone(), until: r.until.clone() });
            }
        }
        e.holiday_left = e.holiday_left.saturating_sub(took);
    }
    ask(c, today);
    for e in c.staff.iter_mut() {
        e.leave.requests.retain(|r| keep(r, today));
    }
    notes
}

// --- the planner's days ------------------------------------------------------------------------

/// A day of the holiday planner: the drivers off on holiday and those who asked for it, the
/// limit, and the cover - the drivers the weekday's roster has, those of them away (on holiday,
/// ill, on a course), and the free drivers who could stand in.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PlannerDay {
    pub date: String,
    pub off: Vec<u32>,
    pub asked: Vec<u32>,
    pub limit: u32,
    pub rostered: usize,
    pub away: usize,
    pub spare: usize,
}

impl PlannerDay {
    /// Rostered drivers away for whom no free driver is left.
    pub fn short(&self) -> usize {
        self.away.saturating_sub(self.spare)
    }
}

/// The planner's day `date` (the drivers).
pub fn planner_day(c: &Company, date: &str) -> PlannerDay {
    let wd = super::plan::weekday_of(date);
    let mut rostered: Vec<u32> = c.planning.week.iter().filter(|r| r.weekday == wd).flat_map(|r| r.duties.iter()).filter_map(|w| if let Some(super::plan::Who::Staff(id)) = w { Some(*id) } else { None }).collect();
    rostered.sort();
    rostered.dedup();
    let drivers: Vec<&Employee> = c.staff.iter().filter(|e| super::staff::drives(e) && e.employed_on(date)).collect();
    let off: Vec<u32> = drivers.iter().filter(|e| e.on_holiday(date).is_some()).map(|e| e.id).collect();
    let asked: Vec<u32> = drivers.iter().filter(|e| e.leave.pending().is_some_and(|r| r.covers(date))).map(|e| e.id).collect();
    let away = drivers.iter().filter(|e| rostered.contains(&e.id) && e.absent(date)).count();
    let spare = drivers.iter().filter(|e| !rostered.contains(&e.id) && !e.absent(date)).count();
    let rostered = drivers.iter().filter(|e| rostered.contains(&e.id)).count();
    PlannerDay { date: date.to_string(), off, asked, limit: limit(c), rostered, away, spare }
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::staff;

    /// A company of `drivers` drivers hired long ago, with `buses` buses.
    fn company(mode: GameMode, drivers: usize, buses: usize) -> Company {
        let mut c = found(&Founding { name: "Urlaub".into(), game_mode: mode, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash = 10_000_000_00;
        let id = c.id.clone();
        while c.staff.len() < drivers {
            for a in staff::applicants(&c) {
                if c.staff.len() < drivers {
                    staff::hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until, e.hired, e.holiday_left) = (None, None, "2023-01-02".into(), HOLIDAYS);
        }
        let bus = super::super::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..buses {
            super::super::market::buy_new(&mut c, &bus, super::super::market::Payment::Cash, "").unwrap();
        }
        c
    }

    fn request(c: &mut Company, id: u32, from: &str, until: &str, reason: Reason) -> u32 {
        c.staff_rules.holidays.last += 1;
        let no = c.staff_rules.holidays.last;
        let today = c.date.clone();
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        e.leave.requests.push(Request { no, from: from.into(), until: until.into(), reason, asked: today.clone(), due: dates::add(&today, 7), ..Default::default() });
        no
    }

    #[test]
    fn the_limit_goes_by_the_fleet_and_an_approval_over_it_is_seen() {
        let mut c = company(GameMode::Normal, 6, 6);
        assert_eq!((default_limit(&c), limit(&c)), (2, 2));
        set_limit(&mut c, Some(1));
        assert_eq!(limit(&c), 1);
        let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
        let a = request(&mut c, ids[0], "2024-04-08", "2024-04-14", Reason::Break);
        let b = request(&mut c, ids[1], "2024-04-12", "2024-04-13", Reason::Break);
        assert_eq!(advice(&c, ids[0], find(&c, a).unwrap().1), Advice::Fits);
        answer(&mut c, a, true, By::Player).unwrap();
        // (approved: off those days, the planning sees it ahead)
        let e = c.employee(ids[0]).unwrap();
        assert_eq!(e.on_holiday("2024-04-10"), Some("2024-04-14"));
        assert!(e.absent("2024-04-14") && !e.absent("2024-04-15") && !e.absent("2024-04-07"));
        // (the second the same days: over the limit on both, refused by the planner)
        let r = find(&c, b).unwrap().1.clone();
        assert_eq!(over_limit(&c, ids[1], &r), vec![("2024-04-12".to_string(), 2), ("2024-04-13".to_string(), 2)]);
        assert_eq!(advice(&c, ids[1], &r), Advice::Over);
        // (a family occasion one over the limit: approved all the same)
        let mut fam = r.clone();
        fam.reason = Reason::Family;
        assert_eq!(advice(&c, ids[1], &fam), Advice::Occasion);
        let p = planner_day(&c, "2024-04-12");
        assert_eq!((p.off.clone(), p.asked.clone(), p.limit), (vec![ids[0]], vec![ids[1]], 1));
        // (answered once only)
        assert!(answer(&mut c, a, false, By::Player).is_err());
    }

    #[test]
    fn refusals_cost_more_each_time_and_weigh_for_months() {
        let mut c = company(GameMode::Realistic, 3, 2);
        let id = c.staff[0].id;
        let mut costs = Vec::new();
        for k in 0..3 {
            let no = request(&mut c, id, &format!("2024-05-1{k}"), &format!("2024-05-1{k}"), Reason::Personal);
            c.staff[0].satisfaction = 80.0;
            answer(&mut c, no, false, By::Player).unwrap();
            costs.push(80.0 - c.staff[0].satisfaction);
        }
        assert_eq!(costs, vec![5.0, 10.0, 15.0]);
        assert_eq!(c.staff[0].leave.refused(&c.date), 3);
        assert_eq!(grudge(&c.staff[0], &c.date), 12.0);
        assert_eq!(grudge(&c.staff[0], &dates::add(&c.date, 200)), 0.0);
        // (twice refused this year: one over the limit is fair now)
        let no = request(&mut c, id, "2024-06-03", "2024-06-04", Reason::Break);
        set_limit(&mut c, Some(1));
        let other = c.staff[1].id;
        let x = request(&mut c, other, "2024-06-01", "2024-06-09", Reason::Break);
        answer(&mut c, x, true, By::Player).unwrap();
        assert_eq!(advice(&c, id, find(&c, no).unwrap().1), Advice::Fair);
        // (an approval: a little more content)
        c.staff[0].satisfaction = 50.0;
        answer(&mut c, no, true, By::Player).unwrap();
        assert_eq!(c.staff[0].satisfaction, 55.0);
    }

    /// A year of nights: requests come ahead, more for the summer and Christmas, within everyone's
    /// days; Casual answers them all by itself, the approved begin and take their days.
    #[test]
    fn a_year_of_requests_is_spread_and_answered() {
        let mut c = company(GameMode::Casual, 12, 8);
        let mut asked: Vec<Request> = Vec::new();
        let mut began = 0;
        for _ in 0..366 {
            let today = c.date.clone();
            began += night(&mut c, &today).len();
            for e in &c.staff {
                for r in &e.leave.requests {
                    if !asked.iter().any(|x| x.no == r.no) {
                        asked.push(r.clone());
                    }
                }
            }
            if today.ends_with("-12-31") {
                for e in c.staff.iter_mut() {
                    e.holiday_left = HOLIDAYS;
                }
            }
            c.date = dates::add(&today, 1);
        }
        assert!(asked.len() >= 40, "{}", asked.len());
        assert!(began > 20, "{began}");
        // (ahead: most a fortnight and more, few within the week)
        let short = asked.iter().filter(|r| dates::between(&r.asked, &r.from) < 7).count();
        assert!(short * 5 < asked.len(), "{short} of {}", asked.len());
        assert!(asked.iter().all(|r| dates::between(&r.asked, &r.from) >= 2 && r.due < r.from));
        // (the summer and Christmas more than the dark months)
        let month = |r: &Request| r.from[5..7].parse::<u32>().unwrap();
        let summer = asked.iter().filter(|r| (7..=8).contains(&month(r))).count();
        let dark = asked.iter().filter(|r| [1, 3, 11].contains(&month(r))).count();
        assert!(summer > dark, "{summer} {dark}");
        assert!(asked.iter().any(|r| r.reason == Reason::Christmas) && asked.iter().any(|r| r.reason == Reason::Summer));
        // (Casual answered every one, and never had more off than the limit but by one)
        assert!(c.staff.iter().all(|e| e.leave.pending().is_none()));
        for d in 0..366 {
            let date = dates::add("2024-03-04", d);
            assert!(off_on(&c, Role::Driver, &date, None) as u32 <= limit(&c) + 1, "{date}");
        }
    }

    #[test]
    fn normal_asks_in_the_inbox_and_its_planner_answers_when_due() {
        let mut c = company(GameMode::Normal, 4, 4);
        let id = c.staff[0].id;
        let no = request(&mut c, id, "2024-03-20", "2024-03-24", Reason::Break);
        // (not due yet: waiting)
        night(&mut c, "2024-03-05");
        assert_eq!(find(&c, no).unwrap().1.state, State::Asked);
        // (due: the planner's answer, told)
        let due = find(&c, no).unwrap().1.due.clone();
        night(&mut c, &due);
        assert_eq!((find(&c, no).unwrap().1.state, find(&c, no).unwrap().1.by), (State::Approved, By::Planner));
        assert!(c.inbox.messages.iter().any(|m| m.title == PLANNER_YES.0 && m.leave == Some(no)));
        // (it begins: the days taken, told)
        let left = c.staff[0].holiday_left;
        let notes = night(&mut c, "2024-03-19");
        assert!(notes.contains(&StaffNote::Holiday { name: c.staff[0].name.clone(), until: "2024-03-24".into() }));
        assert_eq!(c.staff[0].holiday_left, left - 4);
        // (Realistic: no answer is a no)
        let mut r = company(GameMode::Realistic, 4, 4);
        let id = r.staff[0].id;
        let no = request(&mut r, id, "2024-03-20", "2024-03-24", Reason::Break);
        let due = find(&r, no).unwrap().1.due.clone();
        night(&mut r, &due);
        assert_eq!((find(&r, no).unwrap().1.state, find(&r, no).unwrap().1.by), (State::Refused, By::Silence));
    }

    #[test]
    fn an_older_file_has_no_requests() {
        let l: Leave = serde_json::from_str("{}").unwrap();
        assert!(l.requests.is_empty());
        let h: HolidayRules = serde_json::from_str("{}").unwrap();
        assert_eq!((h.max_off, h.last), (None, 0));
        let r: Request = serde_json::from_str(r#"{"no":1,"from":"2024-01-02","until":"2024-01-03","asked":"2024-01-01","due":"2024-01-01"}"#).unwrap();
        assert_eq!((r.state, r.reason, r.working_days()), (State::Asked, Reason::Break, 2));
        assert_eq!((working_days(1), working_days(4), working_days(7), working_days(14)), (1, 3, 5, 10));
    }
}

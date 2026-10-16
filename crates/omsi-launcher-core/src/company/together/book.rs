//! What the company keeps of "Play together" (`Company::together`): the players who came, with
//! their roles and permissions (`roles`) and what they drove for the company, the duties of the
//! company's day they took (or were given, or asked for), and the trips they drove for it as
//! their games judged them. The plan puts a player on his duty before the roster
//! (`plan::day_plan`, `Who::Guest`); the day's close books his trips as the boss's own are booked
//! (`day::close_day`: measured, with his punctuality, passengers, fines and score) and the duty -
//! that duty alone, the rest of its tour runs as planned - counts as run. Every claim and report
//! is checked against the plan, or against the duty's trips as they were when it became his
//! (`Claim::sheet`) once its day has closed.

use super::super::clock;
use super::super::dates;
use super::super::duty_launch::DutyRef;
use super::super::model::{Cents, Company};
use super::super::network::PlannedTrip;
use super::super::plan::{BusOf, DayPlan, DayTour};
use super::roles::{self, Perm, Role};
use super::wire::DriveStart;
use crate::TripRun;
use serde::{Deserialize, Serialize};

/// At most this many players are kept (the oldest who has no duty goes first).
pub const MAX_GUESTS: usize = 64;

/// Why a claim or a report is refused (English: the keys of the translations).
pub const NO_DUTY: &str = "The planning no longer has this duty.";
pub const NOT_TODAY: &str = "Only duties of the company's day can be taken.";
pub const NOT_OPEN: &str = "Somebody drives this duty already.";
pub const NO_BUS: &str = "This duty has no bus of the fleet yet: the boss gives its tour one first.";
pub const BEGUN: &str = "This duty has begun already.";
pub const DRIVEN: &str = "This duty was driven already.";
pub const NOT_YOURS: &str = "This duty is not yours.";
pub const OVERLAP: &str = "You have another duty at that time.";
pub const UNDER_WAY: &str = "You are driving this duty: it stays yours until you have finished.";
pub const DONE: &str = "This duty is finished: what was driven stays.";
pub const TOO_MANY: &str = "The company has as many players as it can take.";
pub const CLOSED_DAY: &str = "The company's day closed before this duty could be driven.";
/// A duty the AI drives (its player left it) is taken back only from a trip it has not begun.
pub const AI_DRIVES: &str = "The AI drives the rest of this duty: take it back from the next trip.";
pub const AI_DONE: &str = "The AI has finished this duty: no trip is left to take back.";
pub const NO_AI: &str = "The AI does not drive this duty.";

/// How many days a duty whose day closed before it was driven stays its player's.
pub const LATE_DAYS: i64 = 3;

/// Every English text of this file (for the translations' check).
pub fn texts() -> [&'static str; 15] {
    [NO_DUTY, NOT_TODAY, NOT_OPEN, NO_BUS, BEGUN, DRIVEN, NOT_YOURS, OVERLAP, UNDER_WAY, DONE, TOO_MANY, CLOSED_DAY, AI_DRIVES, AI_DONE, NO_AI]
}

/// A player who came to the company: known by the id his launcher keeps, numbered by the
/// company (`Who::Guest`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Guest {
    pub no: u32,
    pub id: String,
    pub name: String,
    /// His role, and the permissions the boss gave (`grant`) and took (`deny`) on top of it.
    pub role: Role,
    pub grant: Vec<Perm>,
    pub deny: Vec<Perm>,
    /// Sent away for good: his launcher's messages are refused.
    pub banned: bool,
    /// What he drove for the company.
    pub stats: GuestStats,
}

impl Guest {
    /// What he may do.
    pub fn perms(&self) -> Vec<Perm> {
        roles::effective(self.role, &self.grant, &self.deny)
    }

    pub fn can(&self, p: Perm) -> bool {
        self.perms().contains(&p)
    }

    /// He keeps his place among the players when the company has too many (`guest_of`).
    fn kept(&self) -> bool {
        self.role != Role::Driver || self.banned || !self.grant.is_empty() || self.stats.trips > 0
    }
}

/// What a player drove for the company, as its books took it (`report`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GuestStats {
    /// Duties finished, trips taken.
    pub duties: u32,
    pub trips: u32,
    pub passengers: i64,
    pub km: f64,
    /// The stops served, and those on time.
    pub stops: u32,
    pub on_time: u32,
    /// The trips' scores added up (their average: `score`).
    pub scores: u64,
    /// What his trips brought in (fares and the authority's payment) and cost in fines (cents).
    pub fares: Cents,
    pub fines: Cents,
    pub crashes: u32,
    /// The trips that came after their day had closed.
    pub late: u32,
}

impl GuestStats {
    /// The trips' average score (0 - 100).
    pub fn score(&self) -> u32 {
        if self.trips == 0 {
            0
        } else {
            (self.scores / self.trips as u64) as u32
        }
    }

    /// The share of the stops served on time (None: none served yet).
    pub fn punctuality(&self) -> Option<f64> {
        (self.stops > 0).then(|| 100.0 * self.on_time as f64 / self.stops as f64)
    }
}

/// Where a player's duty stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    /// His: planned for him.
    #[default]
    Planned,
    /// Asked for: the boss says yes or no (the boss lets nobody take duties himself).
    Asked,
    /// His game drives it now.
    Driving,
    /// Driven and reported.
    Done,
}

/// A duty of the company's day for a player.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Claim {
    pub duty: DutyRef,
    pub guest: u32,
    pub state: ClaimState,
    /// The boss gave it him (he did not ask).
    pub given: bool,
    /// The duty as it was when it became his: its tour's number, its times, its trips and its
    /// bus's file (a report of a day closed since is checked against them: `report`).
    pub number: String,
    pub from: i32,
    pub to: i32,
    pub sheet: Vec<PlannedTrip>,
    pub bus: String,
    /// How his game starts on it (worked out by the boss's launcher: `host`).
    pub start: Option<DriveStart>,
    /// Its day closed before he drove it (`after_close`): the company had it covered, what he
    /// drives of it still counts for him and its fines and bonus for the company.
    pub late: bool,
    /// The parts of it the AI drives because he left it (Beta 63: "the route still carries on").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai: Vec<AiPart>,
}

impl Claim {
    /// The AI drives it now (from where he left it, not handed back yet).
    pub fn ai_now(&self) -> Option<&AiPart> {
        self.ai.last().filter(|a| a.until.is_none())
    }

    /// The AI drives (or drove) trip `i` of it (an index into its trips).
    pub fn ai_drives(&self, i: usize) -> bool {
        self.ai.iter().any(|a| a.has(i))
    }
}

/// Why the AI took over a player's duty.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LeftWhy {
    /// He left the session (or his launcher was closed).
    #[default]
    Left,
    /// His game ended before the duty did.
    Quit,
    /// His launcher was not heard from for `host::LEFT_AFTER`.
    Silent,
    /// He was sent away from the session.
    Removed,
    /// The boss's session ended.
    Closed,
}

/// A part of a player's duty the AI drives: its trips from `from` (an index into the duty's
/// trips, deadheads counted) to `until` (None: to its end, or until he takes it back). The same
/// model as the company's own drivers drives it (`day::close_day_with`, `lateness::model`); the
/// trips he reported stay his.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AiPart {
    pub from: usize,
    pub until: Option<usize>,
    pub why: LeftWhy,
    /// When it was handed over (Unix seconds) and the company's clock then (minutes of its day).
    pub at: u64,
    pub minute: i32,
}

impl AiPart {
    pub fn has(&self, i: usize) -> bool {
        i >= self.from && self.until.is_none_or(|u| i < u)
    }
}

/// A trip a player drove for the company, kept for the day's close.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GuestTrip {
    pub guest: u32,
    pub duty: DutyRef,
    pub trip: TripRun,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Together {
    /// Players may take open duties themselves; else they ask and the boss says yes or no.
    pub open_claims: bool,
    pub guests: Vec<Guest>,
    pub claims: Vec<Claim>,
    pub trips: Vec<GuestTrip>,
    /// A duty its player left before driving a trip of it goes back open for another player;
    /// else (the default) the AI drives all of it.
    #[serde(default)]
    pub left_open: bool,
}

impl Default for Together {
    fn default() -> Self {
        Together { open_claims: true, guests: Vec::new(), claims: Vec::new(), trips: Vec::new(), left_open: false }
    }
}

impl Together {
    pub fn guest(&self, no: u32) -> Option<&Guest> {
        self.guests.iter().find(|g| g.no == no)
    }

    /// A player's name ("?" for one no longer known).
    pub fn name_of(&self, no: u32) -> String {
        self.guest(no).map(|g| g.name.clone()).unwrap_or_else(|| "?".into())
    }

    pub fn claim_of(&self, r: &DutyRef) -> Option<&Claim> {
        self.claims.iter().find(|c| same_duty(&c.duty, r))
    }

    fn claim_mut(&mut self, r: &DutyRef) -> Option<&mut Claim> {
        self.claims.iter_mut().find(|c| same_duty(&c.duty, r))
    }

    /// The player known by `id` (his name kept as he says it now), added when new: his number.
    /// None: as many players as the company keeps, each with a duty.
    pub fn guest_of(&mut self, id: &str, name: &str) -> Option<u32> {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        let name = clean(name, 32);
        let name = if name.is_empty() { "Driver".to_string() } else { name };
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id) {
            g.name = name;
            return Some(g.no);
        }
        if self.guests.len() >= MAX_GUESTS {
            let busy: Vec<u32> = self.claims.iter().map(|c| c.guest).chain(self.trips.iter().map(|t| t.guest)).collect();
            let k = self.guests.iter().position(|g| !busy.contains(&g.no) && !g.kept())?;
            self.guests.remove(k);
        }
        let no = self.guests.iter().map(|g| g.no).max().unwrap_or(0) + 1;
        self.guests.push(Guest { no, id: id.to_string(), name, ..Default::default() });
        Some(no)
    }

    pub fn guest_mut(&mut self, no: u32) -> Option<&mut Guest> {
        self.guests.iter_mut().find(|g| g.no == no)
    }

    /// What player `no` may do (nothing: no such player, or sent away).
    pub fn perms_of(&self, no: u32) -> Vec<Perm> {
        self.guest(no).filter(|g| !g.banned).map(Guest::perms).unwrap_or_default()
    }

    /// Player `no` gets `role` with the permissions `want` (what the role does not bring given,
    /// what it brings and he should not have taken).
    pub fn set_role(&mut self, no: u32, role: Role, want: &[Perm]) -> bool {
        let Some(g) = self.guest_mut(no) else { return false };
        let (grant, deny) = roles::overrides(role, want);
        (g.role, g.grant, g.deny) = (role, grant, deny);
        true
    }
}

/// Duties named alike (the line's case, the tour's spaces aside).
pub fn same_duty(a: &DutyRef, b: &DutyRef) -> bool {
    a.date == b.date && a.line.eq_ignore_ascii_case(&b.line) && a.tour.trim() == b.tour.trim() && a.duty == b.duty
}

/// A text a player sent, cut to `max` characters, without control characters.
pub fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

/// The player on duty `k` of `line`/`tour` on `date`, when it is the company's day and he has
/// it (planned for him, driving it, or driven): `plan::day_plan` puts him on it first.
pub fn guest_on(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> Option<u32> {
    if date != c.date {
        return None;
    }
    let r = DutyRef { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), duty: k };
    c.together.claim_of(&r).filter(|x| x.state != ClaimState::Asked).map(|x| x.guest)
}

/// A player's name as the pages show it.
pub fn guest_name(c: &Company, no: u32) -> String {
    c.together.name_of(no)
}

/// The AI drives a duty of player `no` of the company's day now: he left it (the pages say "AI
/// (his name left)" for him).
pub fn guest_left(c: &Company, no: u32) -> bool {
    c.together.claims.iter().any(|x| x.guest == no && x.duty.date == c.date && x.ai_now().is_some())
}

/// The tour and duty `r` names in `p`, with the tour.
fn find<'a>(p: &'a DayPlan, r: &DutyRef) -> Result<(usize, usize, &'a DayTour), &'static str> {
    let (ti, k) = r.find(p).ok_or(NO_DUTY)?;
    Ok((ti, k, &p.tours[ti]))
}

/// Whether duty `k` of `t` has begun on the company's clock (`now`, `clock::now`).
fn begun(p: &DayPlan, t: &DayTour, k: usize, now: i64) -> bool {
    now >= clock::moment(&p.date, t.duties[k].from as i64)
}

/// Whether two duties of the day lie over each other.
fn overlaps(p: &DayPlan, a: &DutyRef, b: &DutyRef) -> bool {
    let (Some((i, k)), Some((j, m))) = (a.find(p), b.find(p)) else { return false };
    let (x, y) = (&p.tours[i].duties[k], &p.tours[j].duties[m]);
    x.from < y.to && y.from < x.to
}

/// The player's other duties of the day lie over `r`.
fn overlapping(c: &Company, p: &DayPlan, r: &DutyRef, guest: u32) -> bool {
    c.together.claims.iter().filter(|x| x.guest == guest && !same_duty(&x.duty, r) && x.duty.date == r.date).any(|x| overlaps(p, &x.duty, r))
}

/// The bus file of a tour of the plan (empty: none of the fleet).
fn bus_file(c: &Company, t: &DayTour) -> String {
    match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.bus.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A claim of duty `k` of `t` (its sheet as the plan has it now).
fn claim_of_plan(c: &Company, t: &DayTour, k: usize, r: &DutyRef, guest: u32, state: ClaimState, given: bool) -> Claim {
    let d = &t.duties[k];
    Claim { duty: r.clone(), guest, state, given, number: t.tour.number.clone(), from: d.from, to: d.to, sheet: t.tour.trips[d.start..d.end].to_vec(), bus: bus_file(c, t), start: None, late: false, ai: Vec::new() }
}

/// A player takes duty `r` of the company's day (`p`, at `now` on the company's clock): his at
/// once when the boss lets players take open duties or he may approve requests himself
/// (`direct`), else asked for. What it is now.
pub fn take(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, now: i64, direct: bool) -> Result<ClaimState, &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    let (_, k, t) = find(p, r)?;
    if let Some(x) = c.together.claim_of(r) {
        return if x.guest == guest { Ok(x.state) } else { Err(NOT_OPEN) };
    }
    if t.by_player {
        return Err(DRIVEN);
    }
    if !matches!(t.bus, Some(BusOf::Own(_))) {
        return Err(NO_BUS);
    }
    if t.duties[k].who.is_some() {
        return Err(NOT_OPEN);
    }
    if begun(p, t, k, now) {
        return Err(BEGUN);
    }
    if overlapping(c, p, r, guest) {
        return Err(OVERLAP);
    }
    let state = if c.together.open_claims || direct { ClaimState::Planned } else { ClaimState::Asked };
    let claim = claim_of_plan(c, t, k, r, guest, state, false);
    c.together.claims.push(claim);
    Ok(state)
}

/// The boss (or a player who may) gives duty `r` to a player (whoever had it today gives it
/// up: the player drives it in his own game, at his own pace).
pub fn give(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, now: i64) -> Result<(), &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    let (_, k, t) = find(p, r)?;
    if t.by_player {
        return Err(DRIVEN);
    }
    if !matches!(t.bus, Some(BusOf::Own(_))) {
        return Err(NO_BUS);
    }
    if let Some(x) = c.together.claim_of(r) {
        match x.state {
            ClaimState::Driving if x.guest != guest => return Err(UNDER_WAY),
            ClaimState::Done => return Err(DONE),
            _ => {}
        }
    }
    if begun(p, t, k, now) && t.duties[k].who.is_some() && c.together.claim_of(r).is_none_or(|x| x.guest != guest) {
        return Err(BEGUN);
    }
    if overlapping(c, p, r, guest) {
        return Err(OVERLAP);
    }
    let claim = claim_of_plan(c, t, k, r, guest, ClaimState::Planned, true);
    c.together.claims.retain(|x| !same_duty(&x.duty, r));
    c.together.claims.push(claim);
    Ok(())
}

/// The boss says yes (or no) to a duty a player asked for: whether there was such a request.
pub fn answer(c: &mut Company, r: &DutyRef, yes: bool) -> Option<u32> {
    let x = c.together.claim_of(r).filter(|x| x.state == ClaimState::Asked)?;
    let guest = x.guest;
    if yes {
        c.together.claim_mut(r)?.state = ClaimState::Planned;
    } else {
        c.together.claims.retain(|x| !same_duty(&x.duty, r));
    }
    Some(guest)
}

/// A duty given back: by its player (`guest`; not while he drives it), or by the boss (None).
/// The player it was.
pub fn release(c: &mut Company, r: &DutyRef, guest: Option<u32>) -> Result<u32, &'static str> {
    let x = c.together.claim_of(r).ok_or(NOT_YOURS)?;
    if guest.is_some_and(|g| g != x.guest) {
        return Err(NOT_YOURS);
    }
    match x.state {
        ClaimState::Done => return Err(DONE),
        ClaimState::Driving if guest.is_some() => return Err(UNDER_WAY),
        _ => {}
    }
    let had = x.guest;
    c.together.claims.retain(|x| !same_duty(&x.duty, r));
    Ok(had)
}

/// The duty a player drives now, or has driven.
pub fn set_state(c: &mut Company, r: &DutyRef, guest: u32, state: ClaimState) -> Result<(), &'static str> {
    let x = c.together.claim_mut(r).filter(|x| x.guest == guest).ok_or(NOT_YOURS)?;
    if x.state == ClaimState::Asked {
        return Err(NOT_YOURS);
    }
    if state == ClaimState::Done && x.state != ClaimState::Done {
        let g = x.guest;
        if let Some(g) = c.together.guest_mut(g) {
            g.stats.duties += 1;
        }
    }
    if let Some(x) = c.together.claim_mut(r) {
        x.state = state;
    }
    Ok(())
}

/// How a player's game starts on his duty `r`, kept with his claim (a duty whose day closed
/// starts with what was kept).
pub fn keep_start(c: &mut Company, r: &DutyRef, start: &DriveStart) {
    if let Some(x) = c.together.claim_mut(r) {
        x.start = Some(start.clone());
    }
}

/// What a report is checked against: the tour as the timetable names it, its line's number,
/// the bus it is driven with and the duty's planned trips.
struct Sheet<'a> {
    tour: &'a str,
    number: &'a str,
    bus: String,
    planned: &'a [PlannedTrip],
}

/// The planned trip of the duty a trip report is of: a trip with passengers that leaves at the
/// report's minute, of its tour as the game knows it, on a line the trip shows or the tour's.
fn trip_of<'a>(s: &Sheet<'a>, run: &TripRun) -> Option<&'a PlannedTrip> {
    if !run.departure.is_finite() {
        return None;
    }
    let dep = (run.departure / 60.0).round() as i32;
    let line = run.line.trim();
    s.planned.iter().find(|x| {
        let tour = if x.origin.trim().is_empty() { s.tour.trim() } else { x.origin.trim() };
        x.counts() && !x.is_second() && x.whole_dep() == dep && tour == run.tour.trim() && (line.eq_ignore_ascii_case(x.line.trim()) || line.eq_ignore_ascii_case(s.number.trim()))
    })
}

/// A number in `0..=max`.
fn within(v: i32, max: i32) -> i32 {
    v.clamp(0, max.max(0))
}

fn finite(v: Option<f64>, max: f64) -> Option<f64> {
    v.filter(|x| x.is_finite()).map(|x| x.clamp(-max, max))
}

/// A player's trip made safe for the books: what the plan says of it (the company's map, its
/// line, the tour's bus) and his figures within what the trip can have had.
fn safe_trip(c: &Company, s: &Sheet, x: &PlannedTrip, run: &TripRun, name: &str, time: u64) -> TripRun {
    let stops = x.part.as_ref().map_or(x.stops, |p| p.stops) as i32;
    let km = x.part.as_ref().map_or(x.km, |p| p.km).max(0.0);
    let mut r = run.clone();
    r.time = time;
    r.driver = name.to_string();
    r.map = c.map.clone();
    r.bus = s.bus.clone();
    r.line = if x.line.trim().is_empty() { s.number.to_string() } else { x.line.clone() };
    r.free = false;
    r.planned = within(r.planned, stops);
    r.stops = within(r.stops, stops);
    r.early = within(r.early, r.stops);
    r.late = within(r.late, r.stops - r.early);
    r.worst = finite(r.worst, 7200.0);
    r.average = finite(r.average, 7200.0);
    r.seconds = if r.seconds.is_finite() { r.seconds.clamp(0.0, 6.0 * 3600.0) } else { 0.0 };
    r.metres = if r.metres.is_finite() { r.metres.clamp(0.0, km * 1500.0 + 2000.0) } else { 0.0 };
    r.jolts = r.jolts.max(0);
    r.crashes = r.crashes.max(0);
    r.passengers = within(r.passengers, 250);
    r.red_lights = r.red_lights.max(0);
    r.speeding = r.speeding.max(0);
    r.fines = r.fines.clamp(0, 2_000_00);
    r.hard_brakes = r.hard_brakes.max(0);
    r.hard_starts = r.hard_starts.max(0);
    r.rough_stops = r.rough_stops.max(0);
    r.offences.truncate(50);
    r.visits.truncate(200);
    r
}

/// What a trip of a player brings the company (cents), as the day's close books it
/// (`day::close_day_with`): its passengers' takings (none on a gross-cost line: the authority's)
/// and a weekend line's tourism grant; on a bus contract its journey's share of the contract's
/// operating day (`bus_contracts`).
pub fn trip_money(c: &Company, t: &TripRun) -> Cents {
    use super::super::{bus_contracts as bc, ownline};
    let Some(l) = super::super::network::line_of_number(c, &t.line) else { return 0 };
    let km = super::super::career::believable_km(t);
    // (the takings of its passengers, `income`, and what the trip is paid besides)
    let ol = ownline::kind_of(l);
    let pax = t.passengers.max(0) as f64;
    let rates = super::super::income::Rates::of(c, l, &c.date);
    let mut take = if ol == crate::service::ServiceKind::School { Default::default() } else { rates.takings(pax, km) };
    ownline::kind_takings(ol, &mut take, pax, c.price_index);
    let comp = ownline::trip_payment(km, ownline::per_km_of(c, l));
    let fares = if rates.to_authority { 0 } else { take.total().round() as Cents };
    let contract = bc::on_line(c, &l.name, &c.date).map_or(0, |k| (bc::day_value(c, k) / k.journeys.max(1) as f64).round() as Cents);
    fares + comp + contract
}

/// A trip taken into a player's figures (`late`: after its day closed, when its fares were not
/// the company's).
fn count_trip(c: &mut Company, guest: u32, t: &TripRun, late: bool) {
    let money = if late { 0 } else { trip_money(c, t) };
    let score = super::super::career::evaluate(t).score;
    let km = super::super::career::believable_km(t);
    let Some(g) = c.together.guest_mut(guest) else { return };
    let s = &mut g.stats;
    s.trips += 1;
    s.passengers += t.passengers.max(0) as i64;
    s.km += km;
    s.stops += t.stops.max(0) as u32;
    s.on_time += t.on_time().max(0) as u32;
    s.scores += score as u64;
    s.fares += money;
    s.fines += t.fines.max(0);
    s.crashes += t.crashes.max(0) as u32;
    if late {
        s.late += 1;
    }
}

/// A player's trip reports of his duty `r`: those of its planned trips not reported yet are
/// kept for the day's close (made safe: `safe_trip`), the others refused. A duty whose day has
/// closed since (`Claim::late`) is checked against its sheet, and its trips are booked at once:
/// the company had the duty covered, so their fines, quality bonus and experience
/// (`levels::book_trip`) and the player's figures, not their fares. `now_unix`: when they came
/// (the close books trips that ended after its last one). How many were taken and refused.
pub fn report(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, runs: &[TripRun], now_unix: u64) -> Result<(usize, usize), &'static str> {
    let x = c.together.claim_of(r).filter(|x| x.guest == guest && x.state != ClaimState::Asked).ok_or(NOT_YOURS)?.clone();
    let today = r.date == c.date && p.is_some_and(|p| p.today);
    if !today && !x.late {
        return Err(NOT_TODAY);
    }
    // (today: the plan as it is now; a closed day: the duty as it was)
    let (tour, planned, number, bus) = match p.filter(|_| today) {
        Some(p) => {
            let (_, k, t) = find(p, r)?;
            let d = &t.duties[k];
            (t.tour.tour.clone(), t.tour.trips[d.start..d.end].to_vec(), t.tour.number.clone(), bus_file(c, t))
        }
        None => (r.tour.clone(), x.sheet.clone(), x.number.clone(), x.bus.clone()),
    };
    let sheet = Sheet { tour: &tour, number: &number, bus, planned: &planned };
    let name = c.together.name_of(guest);
    let time = now_unix.max(c.trips_seen + 1);
    let (mut taken, mut refused) = (0, 0);
    for run in runs.iter().take(64) {
        let Some(px) = trip_of(&sheet, run) else {
            refused += 1;
            continue;
        };
        let dep = px.whole_dep();
        let again = c.together.trips.iter().any(|g| same_duty(&g.duty, r) && (g.trip.departure / 60.0).round() as i32 == dep);
        // (a trip the AI drives since he left: the AI's, not his)
        let ai = planned.iter().position(|y| std::ptr::eq(y, px)).is_some_and(|i| x.ai_drives(i));
        if again || ai {
            refused += 1;
            continue;
        }
        let safe = safe_trip(c, &sheet, px, run, &name, time);
        if !today {
            super::super::levels::book_trip(c, &safe, &number);
        }
        count_trip(c, guest, &safe, !today);
        c.together.trips.push(GuestTrip { guest, duty: r.clone(), trip: safe });
        taken += 1;
    }
    Ok((taken, refused))
}

/// The players' trips the day's close books with the boss's own (`store::close_day`).
pub fn trips_for_close(c: &Company) -> Vec<TripRun> {
    c.together.trips.iter().filter(|t| t.duty.date == c.date).map(|t| t.trip.clone()).collect()
}

/// A trip of the close's is one a player of "Play together" drove (`day::close_day_with`: it is
/// booked as measured, and his duty alone counts as driven - not the whole tour).
pub fn is_guest_trip(c: &Company, t: &TripRun) -> bool {
    c.together.trips.iter().any(|g| g.duty.date == c.date && g.trip == *t)
}

/// A player drove duty `k` of `line`/`tour` on `date`: his trips of it are in (the day's close
/// models the rest of the tour, not that duty).
pub fn drove(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> bool {
    c.together.trips.iter().any(|g| g.duty.date == date && g.duty.duty == k && g.duty.line.eq_ignore_ascii_case(line) && g.duty.tour.trim() == tour.trim())
}

/// After the close of `closed`: its trips go (booked), and so do its claims - but a duty given
/// or taken that was not driven yet stays its player's for a few days (`Claim::late`: he drives
/// it at his own pace, the company had it covered); those older than that go.
pub fn after_close(c: &mut Company, closed: &str) {
    let driven: Vec<DutyRef> = c.together.trips.iter().filter(|t| t.duty.date.as_str() <= closed).map(|t| t.duty.clone()).collect();
    for x in c.together.claims.iter_mut().filter(|x| x.duty.date.as_str() <= closed) {
        // (one the AI drove for him since he left it was driven)
        if matches!(x.state, ClaimState::Planned | ClaimState::Driving) && x.ai.is_empty() && !driven.iter().any(|d| same_duty(d, &x.duty)) {
            x.late = true;
        }
    }
    let today = c.date.clone();
    c.together.claims.retain(|x| x.duty.date.as_str() > closed || (x.late && x.state != ClaimState::Done && dates::between(&x.duty.date, &today) <= LATE_DAYS));
    c.together.trips.retain(|x| x.duty.date.as_str() > closed);
}

/// A player's trips of a duty, as reported.
pub fn trips_of<'a>(c: &'a Company, r: &'a DutyRef) -> impl Iterator<Item = &'a GuestTrip> + 'a {
    c.together.trips.iter().filter(move |t| same_duty(&t.duty, r))
}

// --- the AI on a duty its player left (Beta 63) -------------------------------------------------

/// The trips of duty `r`: as the plan has them today, else as they were when it became its
/// player's (`Claim::sheet`).
fn trips_now(c: &Company, p: Option<&DayPlan>, r: &DutyRef) -> Vec<PlannedTrip> {
    let today = p.filter(|p| p.today && r.date == c.date).and_then(|p| find(p, r).ok()).map(|(_, k, t)| t.tour.trips[t.duties[k].start..t.duties[k].end].to_vec());
    today.or_else(|| c.together.claim_of(r).map(|x| x.sheet.clone())).unwrap_or_default()
}

/// The departures (whole minutes) of the trips its player reported of duty `r`.
fn reported(c: &Company, r: &DutyRef) -> Vec<i32> {
    trips_of(c, r).map(|t| (t.trip.departure / 60.0).round() as i32).collect()
}

/// A trip a player reports (with passengers, not a trip's second part).
fn reportable(x: &PlannedTrip) -> bool {
    x.counts() && !x.is_second()
}

/// Where its player got to on duty `r` (`trips`: its trips): the trip after the last he reported
/// (an index into them; 0: he reported none).
pub fn reached(c: &Company, trips: &[PlannedTrip], r: &DutyRef) -> usize {
    let deps = reported(c, r);
    trips.iter().rposition(|x| reportable(x) && deps.contains(&x.whole_dep())).map_or(0, |i| i + 1)
}

/// What became of a duty its player left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handed {
    /// The AI drives its trips from this one on (an index into its trips).
    Ai(usize),
    /// He drove all of it: nothing is left.
    Over,
    /// He drove none of it, and it went back open for another player (`Together::left_open`).
    Open,
}

/// Player `guest` left his duty `r` (planned for him, or driving it) while it runs: the AI drives
/// the rest of it - from the trip after the last he reported, or all of it when he reported none
/// (or it goes back open, when the boss says so) -, with the same model as the company's own
/// drivers. Nothing of it is dropped, and the trips he reported stay his. `now_unix` and `minute`
/// (the company's clock) say when.
pub fn hand_to_ai(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, why: LeftWhy, now_unix: u64, minute: i32) -> Result<Handed, &'static str> {
    let x = c.together.claim_of(r).filter(|x| x.guest == guest).ok_or(NOT_YOURS)?;
    match x.state {
        ClaimState::Asked => return Err(NOT_YOURS),
        ClaimState::Done => return Err(DONE),
        _ => {}
    }
    // (a duty whose day closed was covered by the company already)
    if x.late {
        return Err(CLOSED_DAY);
    }
    if let Some(a) = x.ai_now() {
        return Ok(Handed::Ai(a.from));
    }
    let trips = trips_now(c, p, r);
    // (taken back from the AI before: from where he drove again)
    let from = reached(c, &trips, r).max(x.ai.last().and_then(|a| a.until).unwrap_or(0));
    if !trips.iter().skip(from).any(reportable) {
        return Ok(Handed::Over);
    }
    if from == 0 && x.ai.is_empty() && c.together.left_open {
        c.together.claims.retain(|x| !same_duty(&x.duty, r));
        return Ok(Handed::Open);
    }
    if let Some(x) = c.together.claim_mut(r) {
        x.ai.push(AiPart { from, until: None, why, at: now_unix, minute });
    }
    Ok(Handed::Ai(from))
}

/// Player `guest` takes his duty `r` back from the AI, at a trip's end: from its next trip with
/// passengers the AI has not begun on the company's clock (`now`, `clock::now`) - the AI drives
/// those before it. The trip he drives from (an index into its trips).
pub fn take_back(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, now: i64) -> Result<usize, &'static str> {
    let x = c.together.claim_of(r).filter(|x| x.guest == guest).ok_or(NOT_YOURS)?;
    let a = x.ai_now().ok_or(NO_AI)?.clone();
    let trips = trips_now(c, p, r);
    let back = (a.from..trips.len()).find(|&i| reportable(&trips[i]) && clock::moment(&r.date, trips[i].dep as i64) >= now).ok_or(AI_DONE)?;
    if let Some(x) = c.together.claim_mut(r) {
        // (the AI drove none of it yet: as if it never had it)
        if back == a.from {
            x.ai.pop();
        } else if let Some(last) = x.ai.last_mut() {
            last.until = Some(back);
        }
    }
    Ok(back)
}

/// The next trip of duty `r` the AI would hand back at (`take_back`), if it drives the duty now
/// and a trip of it is still to begin on the company's clock (`now`).
pub fn back_at(c: &Company, p: Option<&DayPlan>, r: &DutyRef, now: i64) -> Option<usize> {
    let a = c.together.claim_of(r)?.ai_now()?;
    let trips = trips_now(c, p, r);
    (a.from..trips.len()).find(|&i| reportable(&trips[i]) && clock::moment(&r.date, trips[i].dep as i64) >= now)
}

fn duty_ref(date: &str, line: &str, tour: &str, k: usize) -> DutyRef {
    DutyRef { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), duty: k }
}

/// The AI drives a part of duty `k` of `line`/`tour` on `date`, the company's day: its player
/// left it. The day's close and its model drive that part with `ai_driver` (`ai_runs`).
pub fn ai_on(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> bool {
    date == c.date && c.together.claim_of(&duty_ref(date, line, tour, k)).is_some_and(|x| !x.ai.is_empty() && !x.late && x.state != ClaimState::Asked)
}

/// Trip `i` of that duty (an index into its trips, `dep` its departure) is the AI's: in a part
/// the AI drives, and not one its player reported.
pub fn ai_runs(c: &Company, date: &str, line: &str, tour: &str, k: usize, i: usize, dep: i32) -> bool {
    let r = duty_ref(date, line, tour, k);
    ai_on(c, date, line, tour, k) && c.together.claim_of(&r).is_some_and(|x| x.ai_drives(i)) && !reported(c, &r).contains(&dep)
}

/// The AI as the day's model has it drive a duty its player left: a driver of average skills (as
/// an agency's), under `plan::AGENCY` - nobody's employee, paid by nobody.
pub fn ai_driver() -> super::super::model::Employee {
    let mut e = super::super::plan::agency_driver();
    e.name = "AI driver".into();
    e
}

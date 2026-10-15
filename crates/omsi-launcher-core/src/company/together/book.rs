//! What the company keeps of "Play together" (`Company::together`): the players who came, the
//! duties of the company's day they took (or the boss gave them, or they asked for), and the
//! trips they drove for it as their games judged them. The plan puts a player on his duty before
//! the roster (`plan::day_plan`, `Who::Guest`); the day's close books his trips as the boss's own
//! are booked (`day::close_day`: measured, with his punctuality, passengers and score) and the
//! tour counts as run. Money, the planning and the staff stay the boss's: a player only takes,
//! gives back and drives, and every claim and report is checked against the plan.

use super::super::clock;
use super::super::duty_launch::DutyRef;
use super::super::model::Company;
use super::super::network::PlannedTrip;
use super::super::plan::{BusOf, DayPlan, DayTour};
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

/// Every English text of this file (for the translations' check).
pub fn texts() -> [&'static str; 11] {
    [NO_DUTY, NOT_TODAY, NOT_OPEN, NO_BUS, BEGUN, DRIVEN, NOT_YOURS, OVERLAP, UNDER_WAY, DONE, TOO_MANY]
}

/// A player who came to the company: known by the id his launcher keeps, numbered by the
/// company (`Who::Guest`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Guest {
    pub no: u32,
    pub id: String,
    pub name: String,
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
}

impl Default for Together {
    fn default() -> Self {
        Together { open_claims: true, guests: Vec::new(), claims: Vec::new(), trips: Vec::new() }
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
            let k = self.guests.iter().position(|g| !busy.contains(&g.no))?;
            self.guests.remove(k);
        }
        let no = self.guests.iter().map(|g| g.no).max().unwrap_or(0) + 1;
        self.guests.push(Guest { no, id: id.to_string(), name });
        Some(no)
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

/// A player takes duty `r` of the company's day (`p`, at `now` on the company's clock): his at
/// once when the boss lets players take open duties, else asked for. What it is now.
pub fn take(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, now: i64) -> Result<ClaimState, &'static str> {
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
    let state = if c.together.open_claims { ClaimState::Planned } else { ClaimState::Asked };
    c.together.claims.push(Claim { duty: r.clone(), guest, state, given: false });
    Ok(state)
}

/// The boss gives duty `r` to a player (whoever had it today gives it up: the player drives it
/// in the boss's world).
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
    c.together.claims.retain(|x| !same_duty(&x.duty, r));
    c.together.claims.push(Claim { duty: r.clone(), guest, state: ClaimState::Planned, given: true });
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
    x.state = state;
    Ok(())
}

/// The planned trip of `t` a trip report is of: a trip with passengers of the duty (`planned`)
/// that leaves at the report's minute, of its tour as the game knows it, on a line the trip
/// shows or the tour's.
fn trip_of<'a>(t: &DayTour, planned: &'a [PlannedTrip], run: &TripRun) -> Option<&'a PlannedTrip> {
    if !run.departure.is_finite() {
        return None;
    }
    let dep = (run.departure / 60.0).round() as i32;
    let line = run.line.trim();
    planned.iter().find(|x| {
        let tour = if x.origin.trim().is_empty() { t.tour.tour.trim() } else { x.origin.trim() };
        x.counts() && !x.is_second() && x.whole_dep() == dep && tour == run.tour.trim() && (line.eq_ignore_ascii_case(x.line.trim()) || line.eq_ignore_ascii_case(t.tour.number.trim()))
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
fn safe_trip(c: &Company, t: &DayTour, x: &PlannedTrip, run: &TripRun, name: &str, time: u64) -> TripRun {
    let stops = x.part.as_ref().map_or(x.stops, |p| p.stops) as i32;
    let km = x.part.as_ref().map_or(x.km, |p| p.km).max(0.0);
    let bus = match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.bus.clone()).unwrap_or_default(),
        _ => String::new(),
    };
    let mut r = run.clone();
    r.time = time;
    r.driver = name.to_string();
    r.map = c.map.clone();
    r.bus = bus;
    r.line = if x.line.trim().is_empty() { t.tour.number.clone() } else { x.line.clone() };
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

/// A player's trip reports of his duty `r`: those of its planned trips not reported yet are
/// kept for the day's close (made safe: `safe_trip`), the others refused. `now_unix`: when they
/// came (the close books trips that ended after its last one). How many were taken and refused.
pub fn report(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, runs: &[TripRun], now_unix: u64) -> Result<(usize, usize), &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    let x = c.together.claim_of(r).filter(|x| x.guest == guest && x.state != ClaimState::Asked).ok_or(NOT_YOURS)?;
    let _ = x;
    let (_, k, t) = find(p, r)?;
    let d = &t.duties[k];
    let planned = &t.tour.trips[d.start..d.end];
    let name = c.together.name_of(guest);
    let time = now_unix.max(c.trips_seen + 1);
    let (mut taken, mut refused) = (0, 0);
    for run in runs.iter().take(64) {
        let Some(x) = trip_of(t, planned, run) else {
            refused += 1;
            continue;
        };
        let dep = x.whole_dep();
        let again = c.together.trips.iter().any(|g| same_duty(&g.duty, r) && (g.trip.departure / 60.0).round() as i32 == dep);
        if again {
            refused += 1;
            continue;
        }
        let safe = safe_trip(c, t, x, run, &name, time);
        c.together.trips.push(GuestTrip { guest, duty: r.clone(), trip: safe });
        taken += 1;
    }
    Ok((taken, refused))
}

/// The players' trips the day's close books with the boss's own (`store::close_day`).
pub fn trips_for_close(c: &Company) -> Vec<TripRun> {
    c.together.trips.iter().filter(|t| t.duty.date == c.date).map(|t| t.trip.clone()).collect()
}

/// After the close of `closed`: its claims and trips go (booked, or the day is over).
pub fn after_close(c: &mut Company, closed: &str) {
    c.together.claims.retain(|x| x.duty.date.as_str() > closed);
    c.together.trips.retain(|x| x.duty.date.as_str() > closed);
}

/// A player's trips of a duty, as reported.
pub fn trips_of<'a>(c: &'a Company, r: &'a DutyRef) -> impl Iterator<Item = &'a GuestTrip> + 'a {
    c.together.trips.iter().filter(move |t| same_duty(&t.duty, r))
}

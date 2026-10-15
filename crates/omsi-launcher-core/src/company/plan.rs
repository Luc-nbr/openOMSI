//! The planning: who drives what, day by day (Omsi-Hub's `rooster.ts`, `planregels.ts`,
//! `uitval.ts` and `invulling.ts`; the design `busbedrijf-planning.md`).
//!
//! The company's lines give it tours every day (`network::tours_of_day`), each cut into
//! duties (`network::duties_of`, Omsi-Hub's `knipOmloop`). The roster is fixed per weekday
//! and repeats every week: a bus for a tour, a driver - or the player himself - for each
//! duty. Only what is planned runs: "Fill the roster" asks the dispatcher to fill what the
//! roster leaves free (the free buses, the free drivers by the working-time rules,
//! `staff::check`) and fixes it in the roster for the player to see; a line runs only once it
//! is put into service (`network::start_service`). What falls
//! out on the day is open: someone ill or late, a bus that does not start in the morning,
//! the player's own duty not driven. The dispatcher's choice for it counts (a colleague, an
//! agency driver, another bus, a rental bus, or dropping its trips); without one the
//! central fills what came suddenly - a spare colleague, else an agency driver, a free bus,
//! else a rental bus - and the rest is dropped, with the contract's penalty.
//!
//! The morning is drawn from the company and the date alone (`disruptions`), so the planning
//! page and "Close the day" see the same day. The plan of the company's own day is also
//! written for the game (`LivePlan`): while the player drives, the company's tours on the
//! map run with the company's buses as AI, and what they drive comes back through
//! `day::record_live`.

use super::dates;
use super::day::{DutyPlan, LiveEvent, Note, Plan, TourPlan};
use super::economy;
use super::market::{self, MarketBus};
use super::model::{BookingKind, BusKind, BusSize, Cents, Company, GameMode, Drive, Employee, Licence, Skills};
use super::network::{self, TourOfDay};
use super::rng::Rng;
use super::staff::{self, Block, BUS_MARGIN, WEEK_DAYS};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The driver id an agency driver stands under in a closing day's plan.
pub const AGENCY: u32 = u32::MAX;
/// The driver id the player stands under in a plan that is not being closed (his duty is
/// covered: he drives it).
pub const PLAYER: u32 = u32::MAX - 1;
/// The driver id a spare line's driver stands under as a late driver's cover (`LatePiece::cover`):
/// this and the spare duty's id (`rosterflow::Spare`, `spare_lines`) - under the agency's booked
/// drivers, over any employee.
pub const SPARE_COVER: u32 = 0x9000_0000;

/// The spare duty (its id) a late driver's cover `id` is, if it is a spare line's driver.
pub fn spare_of_cover(id: u32) -> Option<u32> {
    (SPARE_COVER..super::agency::BOOKED_BASE).contains(&id).then(|| id - SPARE_COVER)
}

/// An agency driver's hour, the agency's margin included (founding day's prices).
pub const AGENCY_HOUR: Cents = 42_00;

/// The agency drivers a day's plan takes for what fell out or stayed free (not those fixed in the
/// roster, nor those booked ahead: `agency`).
pub fn agency_used(dp: &DayPlan) -> usize {
    dp.tours.iter().flat_map(|t| t.duties.iter()).filter(|d| d.who == Some(Who::Agency) && matches!(d.from_, Source::Dispatcher | Source::Central)).count()
}

/// Whether duty `k` of tour `ti` has an agency driver sent for the day (not the roster's).
fn agency_sent_for(dp: &DayPlan, ti: usize, k: usize) -> bool {
    dp.tours.get(ti).and_then(|t| t.duties.get(k)).is_some_and(|d| d.who == Some(Who::Agency) && matches!(d.from_, Source::Dispatcher | Source::Central))
}

/// `agency_used` by the duty: the pieces joined into one driver's duty in the studio take one
/// agency driver together (testers: "it only applied to a part of it not the full thing").
pub fn agency_sent(c: &Company, dp: &DayPlan) -> usize {
    let slot = super::plandays::slot_of(c, &dp.date);
    let mut seen: Vec<(usize, usize)> = Vec::new();
    let mut n = 0;
    for (i, t) in dp.tours.iter().enumerate() {
        for k in 0..t.duties.len() {
            if !agency_sent_for(dp, i, k) || seen.contains(&(i, k)) {
                continue;
            }
            n += 1;
            seen.extend(joined_pieces(c, slot, &dp.tours, i, k));
        }
    }
    n
}

/// Whether duty `k` of tour `ti` can have an agency driver for the day: always - the agency sends
/// as many as are asked for (Luc, 2026-10-15: "you must be able to hire as many as you want"; it
/// was two a day, and one more for every three drivers). Only what they cost limits them.
pub fn agency_room(_c: &Company, _dp: &DayPlan, _ti: usize, _k: usize) -> bool {
    true
}

/// What an agency driver costs for duty `k` of tour `ti`: the hours of it and of the pieces
/// joined with it (`settle_morning` pays each piece by its hours).
pub fn agency_cost(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> Cents {
    let slot = super::plandays::slot_of(c, &dp.date);
    let hour = AGENCY_HOUR as f64 * c.price_index;
    std::iter::once((ti, k))
        .chain(joined_pieces(c, slot, &dp.tours, ti, k))
        .filter_map(|(i, m)| dp.tours.get(i).and_then(|t| t.duties.get(m)))
        .map(|d| ((d.to - d.from).max(0) as f64 / 60.0 * hour).round() as Cents)
        .sum()
}

/// The pieces joined with duty `k` of tour `i` into one driver's duty in the studio, on the
/// plan of `slot` (`plandays::slot_of`): (tour, duty) in `tours`, the day plan's.
pub fn joined_pieces(c: &Company, slot: u8, tours: &[DayTour], i: usize, k: usize) -> Vec<(usize, usize)> {
    use super::studio::{PieceRef, TripRef};
    if c.planning.joined.is_empty() {
        return Vec::new();
    }
    let Some((t, d)) = tours.get(i).and_then(|t| t.duties.get(k).map(|d| (t, d))) else { return Vec::new() };
    let Some(first) = t.tour.trips.get(d.start) else { return Vec::new() };
    let me = PieceRef { line: t.tour.line.clone(), tour: t.tour.tour.clone(), first: TripRef::of(first) };
    let Some(group) = c.planning.joined.iter().find(|j| j.weekday == slot && j.pieces.iter().any(|p| p.is(&me))) else { return Vec::new() };
    let mut out = Vec::new();
    for p in group.pieces.iter().filter(|p| !p.is(&me)) {
        for (j, x) in tours.iter().enumerate().filter(|(_, x)| same(&x.tour.line, &p.line) && same(&x.tour.tour, &p.tour)) {
            if let Some(m) = x.duties.iter().position(|y| x.tour.trips.get(y.start).is_some_and(|f| p.first.is(f))) {
                out.push((j, m));
            }
        }
    }
    out
}

// --- the roster ------------------------------------------------------------------------------

/// Who drives a duty.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    Staff(u32),
    /// The player himself.
    Player,
    /// A driver of an agency, for the day.
    Agency,
    /// A player of "Play together" (his number in `together::Together::guests`): on the
    /// company's day only, never in the roster.
    Guest(u32),
}

/// A tour of the weekly roster: on a weekday, its bus and who drives each of its duties.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RosterTour {
    /// 0 Monday .. 6 Sunday.
    pub weekday: u8,
    /// The company line (its timetable name) and the tour.
    pub line: String,
    pub tour: String,
    #[serde(default)]
    pub bus: Option<u32>,
    #[serde(default)]
    pub duties: Vec<Option<Who>>,
}

impl RosterTour {
    fn is_empty(&self) -> bool {
        self.bus.is_none() && self.duties.iter().all(Option::is_none)
    }
}

/// The dispatcher's choice for something open today.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fill {
    Colleague { id: u32 },
    Agency,
    Bus { id: u32 },
    Rental,
    Drop,
    /// The player drives it himself, today only.
    Player,
    /// The driver of spare line `id` (`rosterflow::spare_lines`) drives a late driver's first
    /// trips, within its hours.
    Spare { id: u32 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FillChoice {
    /// `tour_key`, `duty_key` or `late_key`.
    pub key: String,
    pub fill: Fill,
}

/// The company's planning as it is saved.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Planning {
    #[serde(default)]
    pub week: Vec<RosterTour>,
    /// The dispatcher fills what the roster leaves free - only for "Fill the roster"
    /// (`fill_day`), which shows the player what it would take and fixes it in the roster: the
    /// day itself runs what is planned, nothing behind the player's back. (Not kept: an older
    /// file's "auto" is read as off.)
    #[serde(skip)]
    pub auto: bool,
    /// The dispatcher's own only for this line's tours (a changed line planned anew,
    /// `fill_line`: the other lines' tours run as they are planned). Not kept either.
    #[serde(skip)]
    pub auto_only: Option<String>,
    /// The dispatcher's own only for these places (`tour_key`s for a bus, `duty_key`s for a
    /// driver: the roster's repair, `repair_day`). Not kept either.
    #[serde(skip)]
    pub auto_keys: Option<Vec<String>>,
    /// The day the dispatcher's choices are for (they hold for that day only).
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub fills: Vec<FillChoice>,
    /// The lines (their timetable names) the planning's chart shows folded to their heading
    /// (Luc: "you must also be able to collapse and expand lines"); kept with the company.
    #[serde(default)]
    pub collapsed: Vec<String>,
    /// The scheduling studio's blocks, a line and a weekday at a time, as the player laid
    /// them out (`studio`): none, the timetable's tours cut automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layouts: Vec<super::studio::LineLayout>,
    /// The duties the player joined from pieces of work (a split duty, or one that changes
    /// buses), a weekday at a time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joined: Vec<super::studio::JoinedDuty>,
    /// RosterFlow's own (`rosterflow`): its preferences, the locks, links and duty notes of
    /// each weekday, the issues dismissed, and the draft not saved yet.
    #[serde(default, skip_serializing_if = "super::rosterflow::Flow::is_empty")]
    pub flow: super::rosterflow::Flow,
    /// When the drivers who come late on the company's day arrive (`Arrival`, `pin_arrivals`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrivals: Vec<Arrival>,
    /// The agency drivers booked ahead for a day (`agency`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agency: Vec<super::agency::Booking>,
}

impl Default for Planning {
    fn default() -> Self {
        Planning { week: Vec::new(), auto: false, auto_only: None, auto_keys: None, date: String::new(), fills: Vec::new(), collapsed: Vec::new(), layouts: Vec::new(), joined: Vec::new(), flow: Default::default(), arrivals: Vec::new(), agency: Vec::new() }
    }
}

/// When a driver who comes late (`Disruption::Late`) arrives on a day: the sign-on of the first
/// duty he had when it was first planned, and his minutes late. Kept, so that moved to another
/// duty he is late only for one that begins before he is there (testers: "when a driver is late
/// and you swap them onto a different duty this should stop them being late").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Arrival {
    pub date: String,
    pub employee: u32,
    /// Minutes of the day.
    pub at: i32,
}

/// The line is shown folded to its heading on the planning's chart.
pub fn is_collapsed(c: &Company, line: &str) -> bool {
    c.planning.collapsed.iter().any(|x| same(x, line))
}

/// Fold a line of the planning's chart to its heading, or unfold it.
pub fn set_collapsed(c: &mut Company, line: &str, folded: bool) {
    c.planning.collapsed.retain(|x| !same(x, line));
    if folded {
        c.planning.collapsed.push(line.trim().to_string());
    }
}

/// Fold every one of `lines`, or unfold them all (the lines the company gave up stay out).
pub fn set_all_collapsed(c: &mut Company, lines: &[String], folded: bool) {
    c.planning.collapsed.clear();
    if folded {
        c.planning.collapsed.extend(lines.iter().map(|l| l.trim().to_string()));
    }
}

pub fn weekday_of(date: &str) -> u8 {
    dates::parse(date).map(|d| dates::weekday(d) as u8).unwrap_or(0)
}

pub fn tour_key(line: &str, tour: &str) -> String {
    format!("{line}|{tour}")
}

pub fn duty_key(line: &str, tour: &str, duty: usize) -> String {
    format!("{line}|{tour}|{duty}")
}

/// The first piece of a duty whose driver comes late.
pub fn late_key(line: &str, tour: &str, duty: usize) -> String {
    format!("{line}|{tour}|{duty}|late")
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The roster's tour on a weekday.
pub fn roster<'a>(c: &'a Company, weekday: u8, line: &str, tour: &str) -> Option<&'a RosterTour> {
    c.planning.week.iter().find(|r| r.weekday == weekday && same(&r.line, line) && same(&r.tour, tour))
}

fn roster_mut<'a>(c: &'a mut Company, weekday: u8, line: &str, tour: &str) -> &'a mut RosterTour {
    let at = match c.planning.week.iter().position(|r| r.weekday == weekday && same(&r.line, line) && same(&r.tour, tour)) {
        Some(k) => k,
        None => {
            c.planning.week.push(RosterTour { weekday, line: line.to_string(), tour: tour.to_string(), bus: None, duties: Vec::new() });
            c.planning.week.len() - 1
        }
    };
    &mut c.planning.week[at]
}

fn prune(c: &mut Company) {
    c.planning.week.retain(|r| !r.is_empty());
}

/// Give a tour of the roster its bus (None: none fixed).
pub fn set_bus(c: &mut Company, weekday: u8, line: &str, tour: &str, bus: Option<u32>) {
    roster_mut(c, weekday, line, tour).bus = bus;
    prune(c);
}

/// Give a duty of the roster its driver (None: none fixed).
pub fn set_driver(c: &mut Company, weekday: u8, line: &str, tour: &str, duty: usize, who: Option<Who>) {
    let r = roster_mut(c, weekday, line, tour);
    if r.duties.len() <= duty {
        r.duties.resize(duty + 1, None);
    }
    r.duties[duty] = who;
    while r.duties.last() == Some(&None) {
        r.duties.pop();
    }
    prune(c);
}

/// Clear a weekday of the roster.
pub fn clear_day(c: &mut Company, weekday: u8) -> Rostered {
    let gone = rostered(c, weekday);
    c.planning.week.retain(|r| r.weekday != weekday);
    gone
}

/// A line renamed (an own line's new number: its timetable name follows): its roster too.
pub fn rename_line(c: &mut Company, old: &str, new: &str) {
    if old.eq_ignore_ascii_case(new) {
        return;
    }
    for r in c.planning.week.iter_mut().filter(|r| r.line.eq_ignore_ascii_case(old)) {
        r.line = new.to_string();
    }
    // (folded on the chart under its old name: folded under its new one)
    for x in c.planning.collapsed.iter_mut().filter(|x| x.eq_ignore_ascii_case(old)) {
        *x = new.to_string();
    }
    // (the studio's blocks and joined duties of the line follow it too)
    super::studio::rename_line(c, old, new);
}

/// The roster's entries of a line's `tours` taken out, every weekday (a changed timetable:
/// their buses and drivers are given anew). Returns how many went.
pub fn forget_tours(c: &mut Company, line: &str, tours: &[String]) -> usize {
    let before = c.planning.week.len();
    c.planning.week.retain(|r| !(r.line.eq_ignore_ascii_case(line) && tours.iter().any(|t| t.trim() == r.tour.trim())));
    before - c.planning.week.len()
}

/// What the roster kept when the day's blocks and duties changed (`follow`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Followed {
    /// The duties that have a driver, and those of lines in service without one (their trips
    /// are dropped until the roster gives them one).
    pub kept: usize,
    pub open: usize,
}

/// The roster of `weekday` kept with its buses and duties when the day's blocks and duties
/// change (#96: RosterFlow's Save, a layout the timetable no longer fits). The roster gives
/// them by their place - the block's name, the duty's index in it - so a duty cut or joined
/// anew moved the drivers onto other duties and left others without one, and the day dropped
/// their trips. `before` and `after` are the day's tours (`network::tours_of_day`) as they
/// were and are now. A block keeps its bus by its name; a duty keeps its driver by the trip it
/// begins with (on its bus, else on another of its line's); a duty that begins with another
/// trip now takes the driver of a duty it is joined with (one driver works a joined duty), else
/// the driver who had most of its trips (a duty cut in two stays with its driver, both halves,
/// until the roster gives one to another), else it has none until the roster gives it one.
pub fn follow(c: &mut Company, weekday: u8, before: &[TourOfDay], after: &[TourOfDay]) -> Followed {
    use super::studio::TripRef;
    // the drivers as the roster gave them: by line, block and the trip their duty begins with;
    // and every trip a driver had
    let mut had: Vec<(String, String, TripRef, Who)> = Vec::new();
    let mut driven: Vec<(String, TripRef, Who)> = Vec::new();
    for t in before {
        let Some(r) = roster(c, weekday, &t.line, &t.tour) else { continue };
        for (k, p) in network::duties_of(t).iter().enumerate() {
            let (Some(Some(w)), Some(x)) = (r.duties.get(k), t.trips.get(p.start)) else { continue };
            had.push((t.line.clone(), t.tour.clone(), TripRef::of(x), *w));
            driven.extend(t.trips[p.clone()].iter().filter(|x| x.counts()).map(|x| (t.line.clone(), TripRef::of(x), *w)));
        }
    }
    let touched = |r: &RosterTour| r.weekday == weekday && before.iter().chain(after).any(|t| same(&t.line, &r.line) && same(&t.tour, &r.tour));
    let old: Vec<RosterTour> = c.planning.week.iter().filter(|r| touched(r)).cloned().collect();
    c.planning.week.retain(|r| !touched(r));
    let pieces: Vec<Vec<std::ops::Range<usize>>> = after.iter().map(network::duties_of).collect();
    let mut given: Vec<Vec<Option<Who>>> = after
        .iter()
        .zip(&pieces)
        .map(|(t, ps)| {
            ps.iter()
                .map(|p| {
                    let first = TripRef::of(t.trips.get(p.start)?);
                    let mine = |h: &&(String, String, TripRef, Who)| same(&h.0, &t.line) && h.2 == first;
                    had.iter().filter(mine).find(|h| same(&h.1, &t.tour)).or_else(|| had.iter().find(mine)).map(|h| h.3)
                })
                .collect()
        })
        .collect();
    // (a duty's other pieces: its driver)
    for i in 0..after.len() {
        for k in 0..given[i].len() {
            if given[i][k].is_none() {
                given[i][k] = super::studio::partners(c, weekday, after, i, k).into_iter().find_map(|(j, m)| given.get(j).and_then(|g| g.get(m).copied().flatten()));
            }
        }
    }
    // (the driver who had most of its trips with passengers)
    for (i, t) in after.iter().enumerate() {
        for (k, p) in pieces[i].iter().enumerate() {
            if given[i][k].is_some() {
                continue;
            }
            let mut count: Vec<(Who, usize)> = Vec::new();
            for x in t.trips[p.clone()].iter().filter(|x| x.counts()) {
                let Some(w) = driven.iter().find(|(l, r, _)| same(l, &t.line) && r.is(x)).map(|d| d.2) else { continue };
                match count.iter_mut().find(|n| n.0 == w) {
                    Some(n) => n.1 += 1,
                    None => count.push((w, 1)),
                }
            }
            given[i][k] = count.iter().fold(None, |best: Option<&(Who, usize)>, n| if best.is_none_or(|b| n.1 > b.1) { Some(n) } else { best }).map(|n| n.0);
        }
    }
    let mut out = Followed::default();
    for (i, t) in after.iter().enumerate() {
        let bus = old.iter().find(|r| same(&r.line, &t.line) && same(&r.tour, &t.tour)).and_then(|r| r.bus);
        let mut duties = given[i].clone();
        for d in &duties {
            match d {
                Some(_) => out.kept += 1,
                None if !t.unplanned => out.open += 1,
                None => {}
            }
        }
        while duties.last() == Some(&None) {
            duties.pop();
        }
        if bus.is_some() || !duties.is_empty() {
            c.planning.week.push(RosterTour { weekday, line: t.line.clone(), tour: t.tour.clone(), bus, duties });
        }
    }
    // (the day's choices for its duties - an agency driver, a colleague - follow them too: they
    // are keyed by the duty's place in its block as the roster is)
    if !c.planning.date.is_empty() && weekday_of(&c.planning.date) == weekday && !c.planning.fills.is_empty() {
        c.planning.fills = follow_fills(&c.planning.fills, before, after);
    }
    out
}

/// A duty's key taken apart (`duty_key`, `late_key`): its line, its tour, its index, and whether
/// it is its late start. None: a tour's key (`tour_key`).
fn parse_duty_key(key: &str) -> Option<(String, String, usize, bool)> {
    let mut parts: Vec<&str> = key.split('|').collect();
    let late = parts.len() > 3 && parts.last() == Some(&"late");
    if late {
        parts.pop();
    }
    if parts.len() < 3 {
        return None;
    }
    let k = parts.pop()?.parse().ok()?;
    let tour = parts.pop()?.to_string();
    Some((parts.join("|"), tour, k, late))
}

/// The day's choices (`fills`) kept with their duties when the blocks and duties change from
/// `before` to `after` (`follow`): a duty's choice goes to every duty now that has most of its
/// trips from it (a duty cut in two keeps it in both halves), a late start's to the duty that
/// begins with the same trip; a tour's (its bus) stays with the tour's name. A choice whose duty
/// is gone goes too.
fn follow_fills(fills: &[FillChoice], before: &[TourOfDay], after: &[TourOfDay]) -> Vec<FillChoice> {
    use super::studio::TripRef;
    // (each trip with passengers of the day before, and the duty it was in)
    let mut was: Vec<(&str, TripRef, (usize, usize))> = Vec::new();
    for (i, t) in before.iter().enumerate() {
        for (k, p) in network::duties_of(t).iter().enumerate() {
            was.extend(t.trips[p.clone()].iter().filter(|x| x.counts()).map(|x| (t.line.as_str(), TripRef::of(x), (i, k))));
        }
    }
    // (each duty now: the duty before that had most of its trips)
    let mut owner: Vec<((usize, usize), Option<(usize, usize)>)> = Vec::new();
    for (i, t) in after.iter().enumerate() {
        for (k, p) in network::duties_of(t).iter().enumerate() {
            let mut count: Vec<((usize, usize), usize)> = Vec::new();
            for x in t.trips[p.clone()].iter().filter(|x| x.counts()) {
                let Some(o) = was.iter().find(|w| same(w.0, &t.line) && w.1.is(x)).map(|w| w.2) else { continue };
                match count.iter_mut().find(|n| n.0 == o) {
                    Some(n) => n.1 += 1,
                    None => count.push((o, 1)),
                }
            }
            let best = count.iter().fold(None, |best: Option<&((usize, usize), usize)>, n| if best.is_none_or(|b| n.1 > b.1) { Some(n) } else { best });
            owner.push(((i, k), best.map(|n| n.0)));
        }
    }
    let mut out: Vec<FillChoice> = Vec::new();
    let mut put = |key: String, fill: Fill| {
        if !out.iter().any(|f| f.key == key) {
            out.push(FillChoice { key, fill });
        }
    };
    for f in fills {
        let Some((line, tour, k, late)) = parse_duty_key(&f.key) else {
            put(f.key.clone(), f.fill);
            continue;
        };
        let Some(bi) = before.iter().position(|t| same(&t.line, &line) && same(&t.tour, &tour)) else {
            put(f.key.clone(), f.fill);
            continue;
        };
        let Some(p) = network::duties_of(&before[bi]).get(k).cloned() else { continue };
        if late {
            // (the duty that begins with the same trip, on its block first)
            let first = TripRef::of(&before[bi].trips[p.start]);
            let begins = |t: &TourOfDay| network::duties_of(t).iter().position(|q| first.is(&t.trips[q.start]));
            let mine = after.iter().filter(|t| same(&t.line, &line));
            let to = mine.clone().filter(|t| same(&t.tour, &tour)).chain(mine).find_map(|t| begins(t).map(|q| (t, q)));
            if let Some((t, q)) = to {
                put(late_key(&t.line, &t.tour, q), f.fill);
            }
            continue;
        }
        for ((i, q), _) in owner.iter().filter(|o| o.1 == Some((bi, k))) {
            put(duty_key(&after[*i].line, &after[*i].tour, *q), f.fill);
        }
    }
    out
}

/// What a weekday's roster holds: the duties given to someone, the buses given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rostered {
    pub duties: usize,
    pub buses: usize,
}

impl Rostered {
    pub fn is_empty(&self) -> bool {
        self.duties + self.buses == 0
    }
}

pub fn rostered(c: &Company, weekday: u8) -> Rostered {
    let days = c.planning.week.iter().filter(|r| r.weekday == weekday);
    days.fold(Rostered::default(), |n, r| Rostered { duties: n.duties + r.duties.iter().flatten().count(), buses: n.buses + usize::from(r.bus.is_some()) })
}

/// The roster of one weekday on others too (Monday's on the other working days): returns
/// the days it went to (none when the weekday's roster is empty - the others are kept then).
pub fn copy_day(c: &mut Company, from: u8, to: &[u8]) -> Vec<u8> {
    if rostered(c, from).is_empty() {
        return Vec::new();
    }
    let src: Vec<RosterTour> = c.planning.week.iter().filter(|r| r.weekday == from).cloned().collect();
    let days: Vec<u8> = to.iter().copied().filter(|d| *d != from).collect();
    for &d in &days {
        clear_day(c, d);
        c.planning.week.extend(src.iter().cloned().map(|mut r| {
            r.weekday = d;
            r
        }));
    }
    days
}

/// The dispatcher's choice for something open on the company's day (None: the central's).
pub fn set_fill(c: &mut Company, key: &str, fill: Option<Fill>) {
    if c.planning.date != c.date {
        c.planning.date = c.date.clone();
        c.planning.fills.clear();
    }
    c.planning.fills.retain(|f| f.key != key);
    if let Some(fill) = fill {
        c.planning.fills.push(FillChoice { key: key.to_string(), fill });
    }
}

fn fill_of(c: &Company, date: &str, key: &str) -> Option<Fill> {
    (c.planning.date == date).then(|| c.planning.fills.iter().find(|f| f.key == key).map(|f| f.fill)).flatten()
}

/// Fill the roster of `date`'s weekday with what the dispatcher would take (the free buses
/// and drivers by the rules, those fixed already kept): returns how many were added.
pub fn fill_day(c: &mut Company, date: &str, tours: Vec<TourOfDay>) -> usize {
    let f = fill_day_told(c, date, tours);
    f.duties + f.buses
}

/// What "Fill the roster" did, and what it left open and why.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Filled {
    /// Duties given to drivers, buses given to tours.
    pub duties: usize,
    pub buses: usize,
    /// Duties still without a driver: for which nobody employed is qualified (the licence,
    /// an endorsement, the type training), and for which the qualified are not free.
    pub unqualified: usize,
    pub busy: usize,
    /// Tours still without a bus.
    pub no_bus: usize,
}

/// `fill_day`, told. What the roster has that cannot run that day (a driver ill, on holiday
/// or clashing, a bus gone, broken down or busy) is given anew first - without "Clear the day"
/// (`repair_day`) - and what is good is kept.
pub fn fill_day_told(c: &mut Company, date: &str, tours: Vec<TourOfDay>) -> Filled {
    let wd = weekday_of(date);
    let (buses, duties) = broken(c, date, tours.clone());
    for (line, tour) in &buses {
        set_bus(c, wd, line, tour, None);
    }
    for (line, tour, k) in &duties {
        set_driver(c, wd, line, tour, *k, None);
    }
    fill(c, date, tours, None)
}

/// What the morning's repair of the roster did (Normal, `repair_day`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Repaired {
    /// The roster's places that could not run (duties and buses), and of them those given
    /// anew: the rest is open.
    pub broken: usize,
    pub duties: usize,
    pub buses: usize,
}

impl Repaired {
    pub fn open(&self) -> usize {
        self.broken.saturating_sub(self.duties + self.buses)
    }
}

/// The inbox's word on the morning's repair: all given anew, or some still open.
pub const REPAIRED: (&str, &str) = ("The roster was repaired", "The dispatcher gave %{n} places of today's roster anew: their driver or bus could not run them (ill, on holiday, a clash or no bus). The rest of the roster is as you planned it.");
pub const REPAIRED_OPEN: (&str, &str) = ("Roster repaired, places still open", "The dispatcher gave %{n} places of today's roster anew, but found nobody free for %{open} of them: give them a driver or a bus in the planning, or they fall out.");

/// What the morning's repair did, told in the inbox (nothing when nothing broke).
pub fn tell_repaired(c: &mut Company, r: &Repaired) {
    use super::inbox::{self, Kind, Message};
    if r.broken == 0 {
        return;
    }
    let (title, text, level) = if r.open() == 0 { (REPAIRED.0, REPAIRED.1, super::clock::Level::Info) } else { (REPAIRED_OPEN.0, REPAIRED_OPEN.1, super::clock::Level::Warn) };
    let m = Message::new(Kind::Lines, title, text).arg("n", (r.duties + r.buses).to_string()).arg("open", r.open().to_string()).level(level).key("roster-repaired");
    inbox::post(c, m);
}

/// The roster's places of `date`'s weekday that cannot run that day: the tours whose bus is
/// gone, broken down, in the workshop, not one of the line's or busy, and the duties whose
/// driver is ill, away, not qualified or clashes. (The player's own duties and the agency's are
/// left as they are, and so are those of a driver on holiday: known ahead, the day covers them
/// and the place stays theirs for when they are back - `day_plan`.)
pub fn broken(c: &Company, date: &str, tours: Vec<TourOfDay>) -> (Vec<(String, String)>, Vec<(String, String, usize)>) {
    let wd = weekday_of(date);
    let mut probe = c.clone();
    probe.planning.auto = false;
    let p = day_plan(&probe, date, tours, &[], &[], false);
    let mut buses = Vec::new();
    let mut duties = Vec::new();
    for t in p.tours.iter().filter(|t| !t.by_player) {
        let Some(r) = roster(c, wd, &t.tour.line, &t.tour.tour) else { continue };
        if r.bus.is_some() && t.bus_from != Source::Roster {
            buses.push((t.tour.line.clone(), t.tour.tour.clone()));
        }
        for (k, d) in t.duties.iter().enumerate() {
            // (a driver on holiday keeps their place: the day covers it, `day_plan`)
            if d.problem == Some(Problem::Holiday) {
                continue;
            }
            if let Some(Some(w @ Who::Staff(_))) = r.duties.get(k) {
                if d.who != Some(*w) || d.from_ != Source::Roster {
                    duties.push((t.tour.line.clone(), t.tour.tour.clone(), k));
                }
            }
        }
    }
    (buses, duties)
}

/// The roster repaired for `date` (Normal's morning, Luc: "the roster repairs itself every
/// morning"): each place of its weekday that cannot run that day (`broken`) is given to a
/// driver or a bus that fits, as the dispatcher would; the good ones are kept, and nothing
/// the roster left free is filled. What found nobody stays open.
pub fn repair_day(c: &mut Company, date: &str, tours: Vec<TourOfDay>) -> Repaired {
    let wd = weekday_of(date);
    let (buses, duties) = broken(c, date, tours.clone());
    let mut out = Repaired { broken: buses.len() + duties.len(), ..Default::default() };
    if out.broken == 0 {
        return out;
    }
    let mut keys = Vec::new();
    for (line, tour) in &buses {
        set_bus(c, wd, line, tour, None);
        keys.push(tour_key(line, tour));
    }
    for (line, tour, k) in &duties {
        set_driver(c, wd, line, tour, *k, None);
        keys.push(duty_key(line, tour, *k));
    }
    let mut probe = c.clone();
    probe.planning.auto = true;
    probe.planning.auto_keys = Some(keys.clone());
    let p = day_plan(&probe, date, tours, &[], &[], false);
    for t in p.tours.iter().filter(|t| !t.by_player) {
        if let (Some(BusOf::Own(id)), Source::Auto) = (t.bus, t.bus_from) {
            if keys.contains(&tour_key(&t.tour.line, &t.tour.tour)) {
                set_bus(c, wd, &t.tour.line, &t.tour.tour, Some(id));
                out.buses += 1;
            }
        }
        for (k, d) in t.duties.iter().enumerate() {
            if let (Some(w @ Who::Staff(_)), Source::Auto) = (d.who, d.from_) {
                if keys.contains(&duty_key(&t.tour.line, &t.tour.tour, k)) {
                    keep_driver(c, wd, t, k, w);
                    out.duties += 1;
                }
            }
        }
    }
    out
}

/// "Fill the roster" for one line's tours only (a changed line planned anew), the day's
/// other tours weighed as they are.
pub fn fill_line(c: &mut Company, date: &str, tours: Vec<TourOfDay>, line: &str) -> Filled {
    fill(c, date, tours, Some(line))
}

/// The driver the dispatcher took for duty `k` of `t` kept: in the weekday's roster - or, an agency
/// driver booked for the day, given him for his day only (`agency::give`).
fn keep_driver(c: &mut Company, wd: u8, t: &DayTour, k: usize, w: Who) {
    if let Who::Staff(id) = w {
        if super::agency::is_booked(id) {
            let first = super::studio::TripRef::of(&t.tour.trips[t.duties[k].start]);
            super::agency::give(c, id, super::studio::PieceRef { line: t.tour.line.clone(), tour: t.tour.tour.clone(), first });
            return;
        }
    }
    set_driver(c, wd, &t.tour.line, &t.tour.tour, k, Some(w));
}

fn fill(c: &mut Company, date: &str, tours: Vec<TourOfDay>, only: Option<&str>) -> Filled {
    let ours = |t: &DayTour| only.is_none_or(|l| t.tour.line.eq_ignore_ascii_case(l));
    let mut probe = c.clone();
    probe.planning.auto = true;
    probe.planning.auto_only = only.map(str::to_string);
    let p = day_plan(&probe, date, tours, &[], &[], false);
    let wd = weekday_of(date);
    let mut f = Filled::default();
    let staff: Vec<&Employee> = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(date)).collect();
    for t in &p.tours {
        if t.by_player || !ours(t) {
            continue;
        }
        // (a tour not in service is not open)
        let open = !t.tour.unplanned;
        match (t.bus, t.bus_from) {
            (Some(BusOf::Own(_)), Source::Auto) => f.buses += 1,
            (None, _) if open => f.no_bus += 1,
            _ => {}
        }
        let bus = t.bus_of(&probe);
        for d in &t.duties {
            match (d.who, d.from_) {
                (Some(Who::Staff(_)), Source::Auto) => f.duties += 1,
                (None, _) if !open => {}
                (None, _) if staff.iter().all(|e| super::licences::lack(e, bus.0, bus.1.as_deref()).is_some()) => f.unqualified += 1,
                (None, _) => f.busy += 1,
                _ => {}
            }
        }
    }
    for t in &p.tours {
        if t.by_player || !ours(t) {
            continue;
        }
        if let (Some(BusOf::Own(id)), Source::Auto) = (t.bus, t.bus_from) {
            set_bus(c, wd, &t.tour.line, &t.tour.tour, Some(id));
        }
        for (k, d) in t.duties.iter().enumerate() {
            if let (Some(w @ Who::Staff(_)), Source::Auto) = (d.who, d.from_) {
                keep_driver(c, wd, t, k, w);
            }
        }
    }
    f
}

// --- the morning -----------------------------------------------------------------------------

/// What befalls the company in the morning of a day (drawn from the company and the date:
/// the same day draws the same).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Disruption {
    /// Someone comes `minutes` late to their first duty.
    Late { employee: u32, minutes: i32 },
    /// A bus does not start: in the workshop for the day; the tow and the parts cost `cost`.
    Breakdown { vehicle: u32, cost: Cents },
}

pub fn disruptions(c: &Company, date: &str) -> Vec<Disruption> {
    let r = economy::rules(c.game_mode);
    let day = dates::parse(date).unwrap_or(0);
    // (the game mode's disruptions: Casual rare, Normal calmer, Realistic as they always were)
    let m = c.game_mode.rules();
    let late_factor = match c.game_mode {
        GameMode::Casual => 0.5,
        GameMode::Normal => 1.0,
        GameMode::Realistic => 1.4,
    } * m.lateness;
    let mut out = Vec::new();
    for e in c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(date) && !e.absent(date)) {
        let mut rng = Rng::of(&[&c.id, "late", &e.id.to_string()], day);
        let p = (0.03 + if e.satisfaction < 50.0 { 0.02 } else { 0.0 }) * late_factor * (1.5 - 0.5 * e.reliability);
        if rng.chance(p) {
            out.push(Disruption::Late { employee: e.id, minutes: 20 + 10 * rng.int(0, 4) as i32 });
        }
    }
    for v in c.fleet.iter().filter(|v| v.held_on(date) && !v.in_workshop(date) && v.condition >= 20.0) {
        let mut rng = Rng::of(&[&c.id, "start", &v.id.to_string()], day);
        let p = (0.01 + (70.0 - v.condition).max(0.0) / 70.0 * 0.06) * (0.5 + 0.5 * r.breakdown_factor) * m.breakdowns;
        if rng.chance(p) {
            let cost = (rng.range(200.0, 600.0) * c.price_index).round() as Cents * 100;
            out.push(Disruption::Breakdown { vehicle: v.id, cost });
        }
    }
    out
}

/// When `employee`, late on `date`, arrives (minutes of the day), if it was kept (`pin_arrivals`).
pub fn arrival(c: &Company, date: &str, employee: u32) -> Option<i32> {
    c.planning.arrivals.iter().find(|a| a.date == date && a.employee == employee).map(|a| a.at)
}

/// The arrivals of the late drivers of the company's day plan `dp` not kept yet: the sign-on of
/// the first duty each has in it, and his minutes late (a driver without a duty keeps nobody
/// waiting: none).
pub fn arrivals_due(c: &Company, dp: &DayPlan) -> Vec<Arrival> {
    if !dp.today {
        return Vec::new();
    }
    dp.disruptions
        .iter()
        .filter_map(|d| if let Disruption::Late { employee, minutes } = *d { Some((employee, minutes)) } else { None })
        .filter(|&(employee, _)| arrival(c, &dp.date, employee).is_none())
        .filter_map(|(employee, minutes)| {
            let &(i, k) = dp.duties_of(Who::Staff(employee)).first()?;
            Some(Arrival { date: dp.date.clone(), employee, at: dp.tours[i].duties[k].from + minutes })
        })
        .collect()
}

/// The late drivers' arrivals of the company's day kept, as the day is planned now (`dp`): in the
/// morning, and before the day's roster is first changed. Returns how many were new.
pub fn pin_arrivals(c: &mut Company, dp: &DayPlan) -> usize {
    let due = arrivals_due(c, dp);
    let date = c.date.clone();
    c.planning.arrivals.retain(|a| a.date == date);
    let n = due.len();
    c.planning.arrivals.extend(due);
    n
}

// --- the plan of a day -----------------------------------------------------------------------

/// How a bus or a driver came to a tour or a duty.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    None,
    Roster,
    /// The dispatcher filled what the roster left free.
    Auto,
    /// The dispatcher's choice for the day.
    Dispatcher,
    /// The central filled what fell out.
    Central,
}

/// Why a tour has no bus, or a duty no driver.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    Sick,
    Holiday,
    /// Not employed that day, gone, or five days worked this week.
    Away,
    /// The licence does not cover the bus.
    Licence,
    /// Drives something else then, or the working-time rules forbid it.
    Conflict,
    /// The bus did not start this morning.
    Breakdown,
    Workshop,
    /// No longer the company's bus.
    Gone,
    /// On another tour then.
    BusBusy,
    /// Nobody, or no bus, planned (only what is planned runs).
    Unassigned,
    /// The player's own duty, not driven.
    Player,
    /// The bus fixed for it is none of the buses its line asks for (`ownline::line_allows`).
    NotLineBus,
    /// The fleet has none of the buses its line asks for.
    NoLineBus,
    /// No endorsement for the bus (articulated, double-decker, electric: `licences`).
    NoEndorsement,
    /// No type training for the bus's model.
    NoTypeTraining,
}

impl Problem {
    /// Came suddenly: the central fills it by itself.
    pub fn sudden(self) -> bool {
        matches!(self, Problem::Sick | Problem::Breakdown | Problem::Player)
    }

    pub fn label(self) -> &'static str {
        match self {
            Problem::Sick => "Ill",
            Problem::Holiday => "On holiday",
            Problem::Away => "Not available",
            Problem::Licence => "No licence for this bus",
            Problem::Conflict => "Clashes with other work",
            Problem::Breakdown => "Did not start",
            Problem::Workshop => "In the workshop",
            Problem::Gone => "No longer in the fleet",
            Problem::BusBusy => "On another tour then",
            Problem::Unassigned => "Not planned",
            Problem::Player => "Your duty, not driven",
            Problem::NotLineBus => "Not one of the line's buses",
            Problem::NoLineBus => "The fleet has none of the line's buses",
            Problem::NoEndorsement => "No licence for this kind of bus",
            Problem::NoTypeTraining => "No type training for this bus",
        }
    }
}

/// What a fitting assignment warns of.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Warn {
    /// Minutes to change to another stop.
    Transfer(i32),
    /// Minutes over the day's eight hours.
    Overtime(i32),
    /// Little experience for a big bus.
    Experience,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BusOf {
    Own(u32),
    /// A bus rented for the day.
    Rental,
}

/// A driver late for a duty's beginning: open until `until`, unless `cover` drives it (an
/// employee, a booked agency driver, or `AGENCY`: one the agency sends on the day).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct LatePiece {
    pub until: i32,
    pub cover: Option<u32>,
    pub from_: Source,
    /// The cover was sent during the day (service control, `control::cover_from`): it drives
    /// the trips from this minute on, those before it are dropped. None: all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_from: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DayDuty {
    /// Its trips (indices into the tour's trips) and its time.
    pub start: usize,
    pub end: usize,
    pub from: i32,
    pub to: i32,
    pub who: Option<Who>,
    pub from_: Source,
    /// What made it open (it stays open while `who` is None).
    pub problem: Option<Problem>,
    pub late: Option<LatePiece>,
    pub warn: Vec<Warn>,
}

impl DayDuty {
    pub fn open(&self) -> bool {
        self.who.is_none()
    }

    /// The trips leaving before this minute are dropped: its driver comes late, and nobody
    /// covers them (or the cover came later: `LatePiece::cover_from`).
    pub fn dropped_before(&self) -> Option<i32> {
        let l = self.late?;
        match l.cover {
            None => Some(l.until),
            Some(_) => l.cover_from.filter(|&f| f > self.from).map(|f| f.min(l.until)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DayTour {
    pub tour: TourOfDay,
    pub bus: Option<BusOf>,
    pub bus_from: Source,
    pub bus_problem: Option<Problem>,
    pub duties: Vec<DayDuty>,
    /// The player drove it (a trip report says so), or the game reported it live.
    pub by_player: bool,
    pub live: bool,
}

impl DayTour {
    pub fn covered(&self) -> bool {
        self.by_player || (self.bus.is_some() && self.duties.iter().all(|d| d.who.is_some()))
    }

    pub fn key(&self) -> String {
        tour_key(&self.tour.line, &self.tour.tour)
    }

    /// The bus it runs with as a driver's qualification sees it: its kind and its file (a
    /// rental or no bus yet: the size it asks for, no model).
    pub fn bus_of(&self, c: &Company) -> (BusKind, Option<String>) {
        match self.bus.and_then(|b| if let BusOf::Own(id) = b { c.vehicle(id) } else { None }) {
            Some(v) => (v.kind, Some(v.bus.clone())),
            None => (BusKind { size: self.size(c), drive: Drive::Diesel }, None),
        }
    }

    /// The size of bus it runs with (or asks for).
    pub fn size(&self, c: &Company) -> BusSize {
        match self.bus {
            Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.kind.size).unwrap_or_default(),
            _ => super::ownline::wanted(c, &self.tour).unwrap_or_default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DayPlan {
    pub date: String,
    pub weekday: u8,
    /// The company's own day (the morning is known, the dispatcher's choices count).
    pub today: bool,
    pub tours: Vec<DayTour>,
    pub disruptions: Vec<Disruption>,
}

/// Something open, or something that was open and got filled, for the list of open duties.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenRow {
    pub tour: usize,
    /// None: the tour's bus.
    pub duty: Option<usize>,
    /// The late piece of the duty.
    pub late: bool,
    pub key: String,
    pub problem: Option<Problem>,
    /// Who or what fills it now (None: its trips are dropped).
    pub filled: bool,
    pub from_: Source,
}

impl DayPlan {
    /// Tours, tours covered, duties open (of tours with a bus), and tours without a bus.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mine: Vec<&DayTour> = self.tours.iter().filter(|t| !t.by_player).collect();
        let covered = self.tours.iter().filter(|t| t.covered()).count();
        let open = mine.iter().filter(|t| t.bus.is_some()).flat_map(|t| t.duties.iter()).filter(|d| d.open()).count();
        let no_bus = mine.iter().filter(|t| t.bus.is_none()).count();
        (self.tours.len(), covered, open, no_bus)
    }

    /// What fell out or stayed free, filled or not (the dispatcher's list).
    pub fn open_rows(&self) -> Vec<OpenRow> {
        let mut out = Vec::new();
        for (ti, t) in self.tours.iter().enumerate() {
            if t.by_player {
                continue;
            }
            if t.bus_problem.is_some() || t.bus.is_none() {
                out.push(OpenRow { tour: ti, duty: None, late: false, key: t.key(), problem: t.bus_problem.or(Some(Problem::Unassigned)), filled: t.bus.is_some(), from_: t.bus_from });
            }
            if t.bus.is_none() {
                continue;
            }
            for (k, d) in t.duties.iter().enumerate() {
                if d.problem.is_some() || d.who.is_none() {
                    out.push(OpenRow { tour: ti, duty: Some(k), late: false, key: duty_key(&t.tour.line, &t.tour.tour, k), problem: d.problem.or(Some(Problem::Unassigned)), filled: d.who.is_some(), from_: d.from_ });
                }
                if let Some(l) = d.late {
                    out.push(OpenRow { tour: ti, duty: Some(k), late: true, key: late_key(&t.tour.line, &t.tour.tour, k), problem: None, filled: l.cover.is_some(), from_: l.from_ });
                }
            }
        }
        out
    }

    /// The duties of a driver, (tour, duty), in the order they begin.
    pub fn duties_of(&self, who: Who) -> Vec<(usize, usize)> {
        let mut v: Vec<(usize, usize)> = self.tours.iter().enumerate().flat_map(|(ti, t)| t.duties.iter().enumerate().filter(|(_, d)| d.who == Some(who)).map(move |(k, _)| (ti, k))).collect();
        v.sort_by_key(|&(ti, k)| self.tours[ti].duties[k].from);
        v
    }

    /// The plan as the day's close takes it.
    pub fn to_plan(&self) -> Plan {
        Plan {
            tours: self
                .tours
                .iter()
                .map(|t| TourPlan {
                    tour: t.tour.clone(),
                    bus: match t.bus {
                        Some(BusOf::Own(id)) => Some(id),
                        _ => None,
                    },
                    duties: t
                        .duties
                        .iter()
                        .map(|d| DutyPlan {
                            start: d.start,
                            end: d.end,
                            from: d.from,
                            to: d.to,
                            driver: match d.who {
                                Some(Who::Staff(id)) => Some(id),
                                Some(Who::Agency) => Some(AGENCY),
                                Some(Who::Player) | Some(Who::Guest(_)) => Some(PLAYER),
                                None => None,
                            },
                            dropped_before: d.dropped_before(),
                            problem: d.problem,
                        })
                        .collect(),
                    by_player: t.by_player,
                    live: t.live,
                    bus_problem: t.bus_problem,
                })
                .collect(),
        }
    }
}

/// Whether a tour is one of `keys` (a line's number or name, and the tour).
fn is_one_of(t: &TourOfDay, keys: &[(String, String)]) -> bool {
    keys.iter().any(|(l, n)| (same(&t.number, l) || same(&t.line, l) || t.trips.iter().any(|x| same(&x.line, l))) && t.runs_tour(n))
}

/// When each bus is out: (id, (from, to) of its tours).
#[derive(Default)]
struct Buses(Vec<(u32, Vec<(i32, i32)>)>);

impl Buses {
    fn free(&self, id: u32, from: i32, to: i32) -> bool {
        self.0.iter().find(|b| b.0 == id).is_none_or(|b| b.1.iter().all(|&(a, z)| to + BUS_MARGIN <= a || z + BUS_MARGIN <= from))
    }

    fn take(&mut self, id: u32, from: i32, to: i32) {
        match self.0.iter_mut().find(|b| b.0 == id) {
            Some(b) => b.1.push((from, to)),
            None => self.0.push((id, vec![(from, to)])),
        }
    }

    /// When it was last out before `from` (the latest end; MIN if never).
    fn last_end(&self, id: u32, from: i32) -> i32 {
        self.0.iter().find(|b| b.0 == id).and_then(|b| b.1.iter().filter(|x| x.1 <= from).map(|x| x.1).max()).unwrap_or(i32::MIN)
    }
}

/// What each driver drives: (id, blocks).
#[derive(Default)]
struct Work(Vec<(u32, Vec<Block>)>);

impl Work {
    fn of(&self, id: u32) -> &[Block] {
        self.0.iter().find(|w| w.0 == id).map(|w| w.1.as_slice()).unwrap_or(&[])
    }

    fn push(&mut self, id: u32, b: Block) {
        match self.0.iter_mut().find(|w| w.0 == id) {
            Some(w) => w.1.push(b),
            None => self.0.push((id, vec![b])),
        }
    }
}

fn block_of(t: &TourOfDay, d: &DayDuty, key: String) -> Block {
    Block { key, from: d.from, to: d.to, from_stop: t.trips[d.start].from.clone(), to_stop: t.trips[d.end - 1].to.clone() }
}

fn covers(u: &Option<String>, date: &str) -> bool {
    u.as_deref().is_some_and(|u| dates::between(date, u) >= 0)
}

/// Why someone cannot drive on `date` at all (None: they can). Only drivers drive: anyone else
/// is away from the duties.
fn unavailable(e: &Employee, date: &str, today: bool) -> Option<Problem> {
    if !staff::drives(e) || !e.employed_on(date) {
        Some(Problem::Away)
    } else if covers(&e.sick_until, date) {
        Some(Problem::Sick)
    } else if e.on_holiday(date).is_some() {
        Some(Problem::Holiday)
    } else if today && e.week_days >= WEEK_DAYS {
        Some(Problem::Away)
    } else {
        None
    }
}

/// How well a bus of `size` fits a tour asking for `wants`: the size asked for (or none
/// asked), a bigger bus (it carries the load), a smaller one.
fn size_fit(wants: Option<BusSize>, size: BusSize) -> u8 {
    match wants {
        None => 2,
        Some(w) if w == size => 2,
        Some(w) if super::ownline::capacity(size) >= super::ownline::capacity(w) => 1,
        Some(_) => 0,
    }
}

/// How a duty (or a piece) on a bus (`DayTour::bus_of`) fits a driver: qualified for it - the
/// licence, the endorsements, the model's type training (`licences`) - and then as `fits`.
pub fn fits_bus(e: &Employee, bus: &(BusKind, Option<String>), work: &[Block], b: &Block, today: bool) -> Result<Vec<Warn>, Problem> {
    if !staff::drives(e) {
        return Err(Problem::Away);
    }
    match super::licences::lack(e, bus.0, bus.1.as_deref()) {
        Some(super::licences::Lack::Licence) => Err(Problem::Licence),
        Some(super::licences::Lack::Endorsement(_)) => Err(Problem::NoEndorsement),
        Some(super::licences::Lack::Type(_)) => Err(Problem::NoTypeTraining),
        None => fits(e, bus.0.size, work, b, today),
    }
}

/// How a duty (or a piece) fits a driver: allowed, and the warnings.
pub fn fits(e: &Employee, size: BusSize, work: &[Block], b: &Block, today: bool) -> Result<Vec<Warn>, Problem> {
    if !staff::drives(e) {
        return Err(Problem::Away);
    }
    if !staff::may_drive(e, size) {
        return Err(Problem::Licence);
    }
    let k = staff::check(work, b, if today { e.last_end } else { None }, None);
    if !k.allowed() {
        return Err(Problem::Conflict);
    }
    let mut w = Vec::new();
    if let Some(t) = k.transfer {
        w.push(Warn::Transfer(t));
    }
    if k.overtime > 0 {
        w.push(Warn::Overtime(k.overtime));
    }
    if !staff::qualified(e, size) {
        w.push(Warn::Experience);
    }
    Ok(w)
}

// --- who could take a duty, and why not ----------------------------------------------------------

/// Why a driver cannot take a duty, told exactly (testers were "confused by how it says drivers
/// are free but unable to cover duties": the lists now say why).
#[derive(Clone, Debug, PartialEq)]
pub enum Cannot {
    /// Not a driver (a mechanic, a cleaner...): only drivers drive.
    NotDriver,
    /// Not employed on the day (not yet, or gone).
    NotEmployed,
    Ill { until: String },
    Holiday { until: String },
    /// Worked the week's days already (`WEEK_DAYS`).
    WeekDone { days: u32 },
    /// The licence, an endorsement or the type training for the bus.
    Lacks(super::licences::Lack),
    /// Drives another duty then (its `duty_key`, and its time).
    OnDuty { key: String, from: i32, to: i32 },
    /// The pause to the neighbouring duty is shorter than the rest (`staff::REST`).
    Rest { minutes: i32 },
    /// The night's rest since yesterday's last duty is too short: free from `from`.
    NightRest { from: i32 },
    /// The working day would come to `minutes`, over `staff::DAY_MAX`.
    TooLong { minutes: i32 },
}

/// Why someone cannot drive on `date` at all (the day itself, before any duty), exactly; on the
/// company's day the night's rest too (free only from `NightRest::from`). None: they can.
pub fn day_unavailable(e: &Employee, date: &str, today: bool) -> Option<Cannot> {
    if !staff::drives(e) {
        Some(Cannot::NotDriver)
    } else if !e.employed_on(date) {
        Some(Cannot::NotEmployed)
    } else if covers(&e.sick_until, date) {
        Some(Cannot::Ill { until: e.sick_until.clone().unwrap_or_default() })
    } else if let Some(until) = e.on_holiday(date) {
        Some(Cannot::Holiday { until: until.to_string() })
    } else if today && e.week_days >= WEEK_DAYS {
        Some(Cannot::WeekDone { days: e.week_days })
    } else {
        None
    }
}

/// When a driver may begin on the company's day after yesterday's last duty (None: any time).
pub fn free_from(e: &Employee, today: bool) -> Option<i32> {
    e.last_end.filter(|_| today).map(|end| end + staff::NIGHT_REST - 1440).filter(|m| *m > 0)
}

/// Whether `e` can take a duty (`b`) on a bus (`DayTour::bus_of`) beside their other work of
/// the day (`work`): the warnings, or exactly why not (`day_plan`'s own rules: `unavailable`,
/// `fits_bus`).
pub fn can_take(e: &Employee, date: &str, today: bool, bus: &(BusKind, Option<String>), work: &[Block], b: &Block) -> Result<Vec<Warn>, Cannot> {
    if let Some(x) = day_unavailable(e, date, today) {
        return Err(x);
    }
    if let Some(x) = super::licences::lack(e, bus.0, bus.1.as_deref()) {
        return Err(Cannot::Lacks(x));
    }
    if !staff::may_drive(e, bus.0.size) {
        return Err(Cannot::Lacks(super::licences::Lack::Licence));
    }
    let k = staff::check(work, b, if today { e.last_end } else { None }, None);
    if let Some(key) = &k.double {
        let (from, to) = work.iter().find(|x| &x.key == key).map(|x| (x.from, x.to)).unwrap_or((b.from, b.to));
        return Err(Cannot::OnDuty { key: key.clone(), from, to });
    }
    if k.too_long {
        return Err(Cannot::TooLong { minutes: staff::work_minutes(work) + (b.to - b.from) });
    }
    if let Some(r) = k.rest.filter(|r| *r < staff::REST) {
        return Err(Cannot::Rest { minutes: r });
    }
    if k.night_rest.is_some_and(|r| r < staff::NIGHT_REST) {
        return Err(Cannot::NightRest { from: free_from(e, today).unwrap_or(b.from) });
    }
    fits_bus(e, bus, work, b, today).map_err(|_| Cannot::NotDriver)
}

/// A driver as a duty's lists show them: who, free all day (a standby), and whether they can
/// take it.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub id: u32,
    /// No other duty that day: a standby driver.
    pub standby: bool,
    pub fit: Result<Vec<Warn>, Cannot>,
}

/// A driver's work on the plan's day without one duty (to see whether it fits).
pub fn work_without(dp: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    dp.duties_of(Who::Staff(id))
        .into_iter()
        .filter(|x| *x != skip)
        .map(|(i, k)| {
            let t = &dp.tours[i];
            block_of(&t.tour, &t.duties[k], duty_key(&t.tour.line, &t.tour.tour, k))
        })
        .collect()
}

/// Every driver for duty `k` of tour `ti`: those who can take it first (the standby drivers,
/// then those with the fewest warnings), then those who cannot, each with the reason.
pub fn candidates(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> Vec<Candidate> {
    let t = &dp.tours[ti];
    let bus = t.bus_of(c);
    let block = block_of(&t.tour, &t.duties[k], duty_key(&t.tour.line, &t.tour.tour, k));
    // (the agency drivers booked for the day too, first among those who can: `agency`)
    let mut out: Vec<(String, Candidate)> = c
        .staff
        .iter()
        .filter(|e| staff::drives(e))
        .chain(super::agency::drivers_on(c, &dp.date))
        .map(|e| {
            let work = work_without(dp, e.id, (ti, k));
            let fit = can_take(e, &dp.date, dp.today, &bus, &work, &block);
            let name = if super::agency::is_booked(e.id) { format!("{:06}", super::agency::number_of(e)) } else { e.name.clone() };
            (name, Candidate { id: e.id, standby: work.is_empty(), fit })
        })
        .collect();
    out.sort_by(|a, b| {
        let rank = |x: &Candidate| match &x.fit {
            Ok(w) => (0, !super::agency::is_booked(x.id), !x.standby, w.len()),
            Err(_) => (1, false, false, 0),
        };
        rank(&a.1).cmp(&rank(&b.1)).then(a.0.cmp(&b.0))
    });
    out.into_iter().map(|x| x.1).collect()
}

/// Who can drive the first trips of duty `k` of tour `ti` before its late driver comes
/// (`DayDuty::late`: service control's cover): the company's drivers and the agency drivers
/// booked for the day free then and fit for the bus, the booked ones first, then the standby
/// drivers (no other duty that day). (id, standby)
pub fn late_covers(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> Vec<(u32, bool)> {
    let Some(t) = dp.tours.get(ti) else { return Vec::new() };
    let Some(d) = t.duties.get(k) else { return Vec::new() };
    let (Some(l), Some(Who::Staff(late))) = (d.late, d.who) else { return Vec::new() };
    let piece = Block { key: late_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: l.until.min(d.to), from_stop: t.tour.trips.get(d.start).map(|x| x.from.clone()).unwrap_or_default(), to_stop: String::new() };
    let bus = t.bus_of(c);
    let mut out: Vec<(bool, bool, u32)> = c
        .staff
        .iter()
        .chain(super::agency::drivers_on(c, &dp.date))
        .filter(|e| e.id != late && unavailable(e, &dp.date, dp.today).is_none())
        .filter(|e| !super::agency::is_booked(e.id) || super::agency::within(c, e.id, &piece))
        .filter_map(|e| {
            let work = work_without(dp, e.id, (usize::MAX, 0));
            fits_bus(e, &bus, &work, &piece, dp.today).ok().map(|_| (!super::agency::is_booked(e.id), !work.is_empty(), e.id))
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, busy, id)| (id, !busy)).collect()
}

/// What the trips of a duty (or a whole tour: `0..trips`) dropped cost in the contract's
/// penalty.
pub fn drop_penalty(c: &Company, t: &TourOfDay, start: usize, end: usize) -> Cents {
    let r = economy::rules(c.game_mode);
    t.trips[start.min(t.trips.len())..end.min(t.trips.len())].iter().filter(|x| x.counts()).map(|x| r.drop_per_trip + (x.km * r.drop_per_km as f64).round() as Cents).sum()
}

/// A way to cover a duty that fell out or stayed free on the company's day (testers: "when
/// drivers go sick, instead of dropping the shift give me the option to cover it").
#[derive(Clone, Debug, PartialEq)]
pub enum Cover {
    /// A colleague who may drive it (`standby`: free all day).
    Colleague { id: u32, standby: bool, warn: Vec<Warn> },
    /// An agency driver (`used` sent today already: as many as are asked for).
    Agency { used: usize },
    /// The player drives it himself.
    Player,
    /// Its trips dropped, with the contract's penalty.
    Drop { penalty: Cents },
}

impl Cover {
    /// The dispatcher's choice it makes.
    pub fn fill(&self) -> Fill {
        match self {
            Cover::Colleague { id, .. } => Fill::Colleague { id: *id },
            Cover::Agency { .. } => Fill::Agency,
            Cover::Player => Fill::Player,
            Cover::Drop { .. } => Fill::Drop,
        }
    }
}

/// The ways to cover duty `k` of tour `ti` on the company's day, in the order to offer them:
/// each free colleague who may drive it (standby drivers first), an agency driver from the
/// agency (as many as are asked for), the player himself, dropping its trips. Those who cannot
/// take it are `candidates` with their reasons.
pub fn covers_of_duty(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> Vec<Cover> {
    let t = &dp.tours[ti];
    let d = &t.duties[k];
    let mut out: Vec<Cover> = candidates(c, dp, ti, k).into_iter().filter_map(|x| x.fit.ok().map(|warn| Cover::Colleague { id: x.id, standby: x.standby, warn })).collect();
    // (one agency driver for the whole duty, its joined pieces too)
    if agency_room(c, dp, ti, k) {
        out.push(Cover::Agency { used: agency_sent(c, dp) });
    }
    out.push(Cover::Player);
    out.push(Cover::Drop { penalty: drop_penalty(c, &t.tour, d.start, d.end) });
    out
}

/// The company's buses free to run tour `ti` instead of its own today (a spare bus for one
/// that broke down, or none given): in the depot and fit to drive (not in the workshop, not
/// broken down this morning), one of the line's buses, not out on another tour then - the size
/// the tour asks for first, then the best kept.
pub fn spare_buses(c: &Company, dp: &DayPlan, ti: usize) -> Vec<u32> {
    let t = &dp.tours[ti];
    let (from, to, wants) = (t.tour.from(), t.tour.to(), super::ownline::wanted(c, &t.tour));
    let broken: Vec<u32> = dp.disruptions.iter().filter_map(|d| if let Disruption::Breakdown { vehicle, .. } = d { Some(*vehicle) } else { None }).collect();
    let mut v: Vec<&super::model::Vehicle> = c
        .fleet
        .iter()
        .filter(|v| v.held_on(&dp.date) && !v.in_workshop(&dp.date) && v.condition >= 20.0 && !broken.contains(&v.id))
        .filter(|v| t.bus != Some(BusOf::Own(v.id)) && super::ownline::line_allows(c, &t.tour.line, v))
        // (a model the company does not work with any more is no spare: Beta 54)
        .filter(|v| super::vehicles::vehicle_allowed(c, v))
        .filter(|v| !dp.tours.iter().enumerate().any(|(i, x)| i != ti && x.bus == Some(BusOf::Own(v.id)) && from < x.tour.to() + BUS_MARGIN && x.tour.from() < to + BUS_MARGIN))
        .collect();
    v.sort_by(|a, b| size_fit(wants, b.kind.size).cmp(&size_fit(wants, a.kind.size)).then(b.condition.partial_cmp(&a.condition).unwrap_or(std::cmp::Ordering::Equal)).then(a.id.cmp(&b.id)));
    v.into_iter().map(|v| v.id).collect()
}

/// What a rental bus for tour `ti` costs today, and whether one can come (a place in the depot
/// and the cash).
pub fn rental_for(c: &Company, dp: &DayPlan, ti: usize) -> (Cents, bool) {
    let wants = super::ownline::wanted(c, &dp.tours[ti].tour);
    let rent = economy::rent_per_day(BusKind { size: wants.unwrap_or_default(), drive: Drive::Diesel }, &economy::rules(c.game_mode), c.price_index);
    (rent, places_left(c) > 0 && c.cash >= rent)
}

/// The places the depot has left today for another bus (`depot::room`'s rule: the yard's
/// spaces, the halls the levels open, a few in the street).
fn places_left(c: &Company) -> usize {
    let held = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    (c.site.spaces() + super::levels::extra_places(c) as usize + super::depot::OUTSIDE_MAX).saturating_sub(held)
}

/// The plan of `date`: its tours (`network::tours_of_day` of that date's timetable) given
/// buses and drivers - the roster's first, then the dispatcher's own when `auto` is on; on
/// the company's own day the morning and what is open handled. `player` and `live` are the
/// tours the player drove and the game reported (they need nothing of the model);
/// `closing`: the day is being closed, so the player's duties he did not drive are open.
pub fn day_plan(c: &Company, date: &str, tours: Vec<TourOfDay>, player: &[(String, String)], live: &[(String, String)], closing: bool) -> DayPlan {
    let today = date == c.date;
    let wd = weekday_of(date);
    let dis = if today { disruptions(c, date) } else { Vec::new() };
    let broken: Vec<u32> = dis.iter().filter_map(|d| if let Disruption::Breakdown { vehicle, .. } = d { Some(*vehicle) } else { None }).collect();
    let usable = |id: u32| c.vehicle(id).is_some_and(|v| v.held_on(date) && !v.in_workshop(date) && v.condition >= 20.0) && !broken.contains(&id);
    // (the dispatcher's own, for a line's tours)
    let auto = |line: &str| c.planning.auto && c.planning.auto_only.as_deref().is_none_or(|l| l.eq_ignore_ascii_case(line));
    // (and only the repair's places, `repair_day`)
    let auto_key = |key: &str| c.planning.auto_keys.as_ref().is_none_or(|k| k.iter().any(|x| x == key));
    let mut tours = tours;
    tours.sort_by_key(|t| t.from());
    let mut out: Vec<DayTour> = tours
        .into_iter()
        .map(|t| {
            let duties = network::duties_of(&t)
                .into_iter()
                .map(|r| DayDuty { from: t.trips[r.start].dep, to: t.trips[r.clone()].iter().map(|x| x.arr).max().unwrap_or(0), start: r.start, end: r.end, ..Default::default() })
                .collect();
            let by_player = is_one_of(&t, player);
            let live = !by_player && is_one_of(&t, live);
            DayTour { tour: t, duties, by_player, live, ..Default::default() }
        })
        .collect();

    // 1. the buses: the roster's, then the dispatcher's own, then what fell out
    let mut buses = Buses::default();
    for t in out.iter_mut().filter(|t| !t.by_player) {
        let Some(id) = roster(c, wd, &t.tour.line, &t.tour.tour).and_then(|r| r.bus) else { continue };
        let (from, to) = (t.tour.from(), t.tour.to());
        let problem = match c.vehicle(id) {
            None => Some(Problem::Gone),
            Some(v) if !v.held_on(date) => Some(Problem::Gone),
            Some(_) if broken.contains(&id) => Some(Problem::Breakdown),
            Some(v) if v.in_workshop(date) || v.condition < 20.0 => Some(Problem::Workshop),
            Some(v) if !super::ownline::line_allows(c, &t.tour.line, v) => Some(Problem::NotLineBus),
            Some(_) if !buses.free(id, from, to) => Some(Problem::BusBusy),
            Some(_) => None,
        };
        match problem {
            None => {
                buses.take(id, from, to);
                t.bus = Some(BusOf::Own(id));
                t.bus_from = Source::Roster;
            }
            p => t.bus_problem = p,
        }
    }
    let rostered_bus: Vec<u32> = c.planning.week.iter().filter(|r| r.weekday == wd).filter_map(|r| r.bus).collect();
    for t in out.iter_mut().filter(|t| !t.by_player && t.bus.is_none() && t.bus_problem.is_none()) {
        // (a line that asks for buses the fleet has none of says so)
        let none = if super::ownline::fleet_has_bus_for(c, &t.tour.line) { Problem::Unassigned } else { Problem::NoLineBus };
        if !auto(&t.tour.line) || !auto_key(&tour_key(&t.tour.line, &t.tour.tour)) {
            t.bus_problem = Some(none);
            continue;
        }
        let (from, to, wants) = (t.tour.from(), t.tour.to(), super::ownline::wanted(c, &t.tour));
        // (free in time and one of the line's buses; the size asked for first, a bus no roster
        // tour has, then the one free the latest: the others stay free for later tours)
        let best = c
            .fleet
            .iter()
            .filter(|v| usable(v.id) && buses.free(v.id, from, to) && super::ownline::line_allows(c, &t.tour.line, v))
            .max_by_key(|v| (size_fit(wants, v.kind.size), !rostered_bus.contains(&v.id), buses.last_end(v.id, from), std::cmp::Reverse(v.id)));
        match best {
            Some(v) => {
                buses.take(v.id, from, to);
                t.bus = Some(BusOf::Own(v.id));
                t.bus_from = Source::Auto;
            }
            None => t.bus_problem = Some(none),
        }
    }
    if today {
        // (a rental bus is rented at the close as any other, `settle_morning`: it needs a place
        // in the depot and the cash for its day)
        let mut places = places_left(c);
        for t in out.iter_mut().filter(|t| !t.by_player && t.bus.is_none()) {
            let (from, to, wants) = (t.tour.from(), t.tour.to(), super::ownline::wanted(c, &t.tour));
            let rent = economy::rent_per_day(BusKind { size: wants.unwrap_or_default(), drive: Drive::Diesel }, &economy::rules(c.game_mode), c.price_index);
            match fill_of(c, date, &t.key()) {
                Some(Fill::Bus { id }) if usable(id) && buses.free(id, from, to) => {
                    buses.take(id, from, to);
                    t.bus = Some(BusOf::Own(id));
                    t.bus_from = Source::Dispatcher;
                }
                Some(Fill::Rental) if places > 0 && c.cash >= rent => {
                    places -= 1;
                    t.bus = Some(BusOf::Rental);
                    t.bus_from = Source::Dispatcher;
                }
                Some(Fill::Drop) => t.bus_from = Source::Dispatcher,
                _ if t.bus_problem.is_some_and(Problem::sudden) => {
                    let best = c.fleet.iter().filter(|v| usable(v.id) && buses.free(v.id, from, to) && super::ownline::line_allows(c, &t.tour.line, v)).max_by(|a, b| {
                        (size_fit(wants, a.kind.size), a.condition).partial_cmp(&(size_fit(wants, b.kind.size), b.condition)).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    if let Some(v) = best {
                        buses.take(v.id, from, to);
                        t.bus = Some(BusOf::Own(v.id));
                        t.bus_from = Source::Central;
                    } else if c.cash >= rent && places > 0 {
                        places -= 1;
                        t.bus = Some(BusOf::Rental);
                        t.bus_from = Source::Central;
                    }
                }
                _ => {}
            }
        }
    }

    // (Beta 58: a player of "Play together" on the duty he has today, before anybody else)
    if today {
        for t in out.iter_mut().filter(|t| !t.by_player) {
            for (k, d) in t.duties.iter_mut().enumerate() {
                if let Some(g) = super::together::guest_on(c, date, &t.tour.line, &t.tour.tour, k) {
                    d.who = Some(Who::Guest(g));
                    d.from_ = Source::Dispatcher;
                }
            }
        }
    }

    // 2. the drivers: the agency drivers booked for the day on the duties given them that day
    // (`agency`), then the roster's, in the order the duties begin
    let mut work = Work::default();
    let mut order: Vec<(usize, usize)> = out.iter().enumerate().filter(|(_, t)| !t.by_player).flat_map(|(i, t)| (0..t.duties.len()).map(move |k| (i, k))).collect();
    order.sort_by_key(|&(i, k)| out[i].duties[k].from);
    let booked: Vec<&Employee> = super::agency::drivers_on(c, date);
    for e in &booked {
        for p in super::agency::duties_of(c, e.id) {
            let found = order.iter().copied().find(|&(i, k)| {
                let t = &out[i];
                same(&t.tour.line, &p.line) && same(&t.tour.tour, &p.tour) && t.tour.trips.get(t.duties[k].start).is_some_and(|x| p.first.is(x))
            });
            let Some((i, k)) = found.filter(|&(i, k)| out[i].duties[k].who.is_none()) else { continue };
            let bus = out[i].bus_of(c);
            let t = &out[i];
            let block = block_of(&t.tour, &t.duties[k], duty_key(&t.tour.line, &t.tour.tour, k));
            if let Ok(w) = fits_bus(e, &bus, work.of(e.id), &block, today) {
                work.push(e.id, block);
                let d = &mut out[i].duties[k];
                d.who = Some(Who::Staff(e.id));
                d.from_ = Source::Dispatcher;
                d.warn = w;
            }
        }
    }
    for &(i, k) in &order {
        if out[i].duties[k].who.is_some() {
            continue;
        }
        let Some(who) = roster(c, wd, &out[i].tour.line, &out[i].tour.tour).and_then(|r| r.duties.get(k).copied().flatten()) else { continue };
        let bus = out[i].bus_of(c);
        let t = &out[i];
        let block = block_of(&t.tour, &t.duties[k], duty_key(&t.tour.line, &t.tour.tour, k));
        let d = &mut out[i].duties[k];
        match who {
            Who::Player if closing => d.problem = Some(Problem::Player),
            // (a player of "Play together" is never in the roster: only his claim of the day puts
            // him on a duty)
            Who::Guest(_) => {}
            Who::Player | Who::Agency => {
                d.who = Some(who);
                d.from_ = Source::Roster;
            }
            Who::Staff(id) => {
                let check = match c.employee(id) {
                    None => Err(Problem::Away),
                    Some(e) => match unavailable(e, date, today) {
                        Some(p) => Err(p),
                        None => fits_bus(e, &bus, work.of(id), &block, today),
                    },
                };
                match check {
                    Ok(w) => {
                        work.push(id, block);
                        d.who = Some(who);
                        d.from_ = Source::Roster;
                        d.warn = w;
                    }
                    Err(p) => d.problem = Some(p),
                }
            }
        }
    }
    // the dispatcher's own for what the roster left free (tours with a bus, or reported live): the
    // agency drivers booked for the day first (they are paid for the day all the same), within
    // the hours they come for
    let people: Vec<&Employee> = c.staff.iter().filter(|e| unavailable(e, date, today).is_none()).chain(booked.iter().copied()).collect();
    let pick = |work: &Work, bus: &(BusKind, Option<String>), block: &Block| -> Option<(u32, Vec<Warn>)> {
        let mut best: Option<((bool, bool, bool, i32, u32, i64), u32, Vec<Warn>)> = None;
        for e in &people {
            let is_booked = super::agency::is_booked(e.id);
            if is_booked && !super::agency::within(c, e.id, block) {
                continue;
            }
            let Ok(w) = fits_bus(e, bus, work.of(e.id), block, today) else { continue };
            let worked = staff::work_minutes(work.of(e.id));
            let fills = worked > 0 && !w.iter().any(|x| matches!(x, Warn::Overtime(_)));
            let score = (is_booked, staff::qualified(e, bus.0.size), fills, -worked, WEEK_DAYS.saturating_sub(e.week_days), (e.experience * 10.0) as i64);
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, e.id, w));
            }
        }
        best.map(|b| (b.1, b.2))
    };
    for &(i, k) in &order {
        let t = &out[i];
        if t.duties[k].who.is_some() || t.duties[k].problem.is_some() || (t.bus.is_none() && !t.live) {
            continue;
        }
        if !auto(&t.tour.line) || !auto_key(&duty_key(&t.tour.line, &t.tour.tour, k)) {
            out[i].duties[k].problem = Some(Problem::Unassigned);
            continue;
        }
        let bus = t.bus_of(c);
        let block = block_of(&t.tour, &t.duties[k], duty_key(&t.tour.line, &t.tour.tour, k));
        // (a piece of a duty the player joined in the studio: its other pieces' driver first)
        let partner = super::studio::partner_driver(c, super::plandays::slot_of(c, date), &out, i, k).and_then(|id| {
            let e = c.employee(id).filter(|e| unavailable(e, date, today).is_none())?;
            fits_bus(e, &bus, work.of(id), &block, today).ok().map(|w| (id, w))
        });
        match partner.or_else(|| pick(&work, &bus, &block)) {
            Some((id, w)) => {
                work.push(id, block);
                let d = &mut out[i].duties[k];
                d.who = Some(Who::Staff(id));
                d.from_ = Source::Auto;
                d.warn = w;
            }
            None => out[i].duties[k].problem = Some(Problem::Unassigned),
        }
    }
    // a driver on holiday (known ahead, Beta 57: `holidays`): a free colleague covers their duty,
    // on any day planned, the roster kept theirs (today the dispatcher's own choice first)
    for &(i, k) in &order {
        let t = &out[i];
        let d = &t.duties[k];
        if d.who.is_some() || d.problem != Some(Problem::Holiday) || (t.bus.is_none() && !t.live) {
            continue;
        }
        let key = duty_key(&t.tour.line, &t.tour.tour, k);
        if today && fill_of(c, date, &key).is_some() {
            continue;
        }
        let bus = t.bus_of(c);
        let block = block_of(&t.tour, d, key);
        if let Some((id, w)) = pick(&work, &bus, &block) {
            work.push(id, block);
            let d = &mut out[i].duties[k];
            d.who = Some(Who::Staff(id));
            d.from_ = Source::Central;
            d.warn = w;
        }
    }
    // what is open today: the dispatcher's choice, else the central for what came suddenly (and,
    // but on Realistic, for a holiday nobody could cover: an agency driver)
    let central = |p: Option<Problem>| p.is_some_and(Problem::sudden) || (p == Some(Problem::Holiday) && c.game_mode != GameMode::Realistic);
    if today {
        let slot = super::plandays::slot_of(c, date);
        for &(i, k) in &order {
            let t = &out[i];
            if t.duties[k].who.is_some() || (t.bus.is_none() && !t.live) {
                continue;
            }
            let bus = t.bus_of(c);
            let key = duty_key(&t.tour.line, &t.tour.tour, k);
            let block = block_of(&t.tour, &t.duties[k], key.clone());
            let problem = t.duties[k].problem;
            // (a piece joined with others into one driver's duty: the choice for any of them is
            // the whole duty's, and one agency driver drives all of it - testers: "it only
            // applied to a part of it not the full thing"; the agency sends as many as asked)
            let joined = joined_pieces(c, slot, &out, i, k);
            let partner_fill = || joined.iter().find_map(|&(j, m)| fill_of(c, date, &duty_key(&out[j].tour.line, &out[j].tour.tour, m)).filter(|f| matches!(f, Fill::Agency | Fill::Colleague { .. } | Fill::Player)));
            let (who, from_) = match fill_of(c, date, &key).or_else(partner_fill) {
                Some(Fill::Colleague { id }) => match c.employee(id).filter(|e| unavailable(e, date, today).is_none()).map(|e| fits_bus(e, &bus, work.of(id), &block, today)) {
                    Some(Ok(_)) => (Some(Who::Staff(id)), Source::Dispatcher),
                    _ => (None, Source::None),
                },
                Some(Fill::Agency) => (Some(Who::Agency), Source::Dispatcher),
                // (the player's own today: open again when the day is closed and he did not drive it)
                Some(Fill::Player) if !closing => (Some(Who::Player), Source::Dispatcher),
                Some(Fill::Player) => (None, Source::Dispatcher),
                Some(Fill::Drop) => (None, Source::Dispatcher),
                // (the central: a free colleague - the agency drivers booked for the day first -,
                // else one sent by the agency)
                _ if central(problem) => match pick(&work, &bus, &block) {
                    Some((id, _)) => (Some(Who::Staff(id)), Source::Central),
                    None => (Some(Who::Agency), Source::Central),
                },
                _ => (None, Source::None),
            };
            if let Some(Who::Staff(id)) = who {
                work.push(id, block);
            }
            let d = &mut out[i].duties[k];
            d.who = who;
            d.from_ = from_;
        }
        // late in the morning: the first duty of the day begins without them, until they arrive
        // (kept when first planned: moved to a duty that begins after it, they are not late)
        // (the day's spare lines, each driving one late driver's first trips at a time)
        let spares = super::rosterflow::spare_lines(c, date);
        let mut spare_busy: Vec<(u32, i32, i32)> = Vec::new();
        for dz in &dis {
            let Disruption::Late { employee, minutes } = *dz else { continue };
            let mine: Vec<(usize, usize)> = order.iter().copied().filter(|&(i, k)| out[i].duties[k].who == Some(Who::Staff(employee))).collect();
            let Some(&(i, k)) = mine.first() else { continue };
            let t = &out[i];
            let d = &t.duties[k];
            let until = arrival(c, date, employee).unwrap_or(d.from + minutes);
            if !t.tour.trips[d.start..d.end].iter().any(|x| x.dep < until) {
                continue;
            }
            let key = late_key(&t.tour.line, &t.tour.tour, k);
            let piece = Block { key: key.clone(), from: d.from, to: until.min(d.to), from_stop: t.tour.trips[d.start].from.clone(), to_stop: String::new() };
            let bus = t.bus_of(c);
            // (a spare line standing by all the time from when its driver takes over - the duty's
            // start, or when service control sent him - until the late driver comes: the one
            // chosen, else the central's first choice; a spare line covers only within its hours)
            let (start, end) = (super::control::cover_from(c, date, &t.tour.line, &t.tour.tour, k).unwrap_or(d.from).max(d.from), until.min(d.to));
            let spare_free = |want: Option<u32>, busy: &[(u32, i32, i32)]| spares.iter().find(|x| want.is_none_or(|w| w == x.id) && x.covers(start, end) && !busy.iter().any(|b| b.0 == x.id && b.1 < end && start < b.2)).map(|x| x.id);
            let (cover, from_) = match fill_of(c, date, &key) {
                Some(Fill::Spare { id }) => (spare_free(Some(id), &spare_busy).map(|id| SPARE_COVER + id), Source::Dispatcher),
                Some(Fill::Colleague { id }) if c.employee(id).is_some_and(|e| id != employee && unavailable(e, date, today).is_none() && fits_bus(e, &bus, work.of(id), &piece, today).is_ok()) => (Some(id), Source::Dispatcher),
                // (an agency driver sent for the late driver's first trips: service control)
                Some(Fill::Agency) => (Some(AGENCY), Source::Dispatcher),
                Some(_) => (None, Source::Dispatcher),
                None => match (spare_free(None, &spare_busy), pick(&work, &bus, &piece)) {
                    (Some(id), _) => (Some(SPARE_COVER + id), Source::Central),
                    (None, Some((id, _))) if id != employee => (Some(id), Source::Central),
                    _ => (None, Source::None),
                },
            };
            match cover.and_then(spare_of_cover) {
                Some(id) => spare_busy.push((id, start, end)),
                None => {
                    if let Some(id) = cover {
                        work.push(id, piece);
                    }
                }
            }
            // (sent during the day by service control: from then on)
            let cover_from = cover.and_then(|_| super::control::cover_from(c, date, &t.tour.line, &t.tour.tour, k));
            out[i].duties[k].late = Some(LatePiece { until, cover, from_, cover_from });
        }
    }
    // (a duty a player of "Play together" had and did not drive: open when the day is closed,
    // its trips dropped)
    if closing {
        for t in out.iter_mut().filter(|t| !t.by_player) {
            for d in t.duties.iter_mut().filter(|d| matches!(d.who, Some(Who::Guest(_)))) {
                d.who = None;
                d.problem = Some(Problem::Player);
            }
        }
    }
    DayPlan { date: date.to_string(), weekday: wd, today, tours: out, disruptions: dis }
}

// --- the player's own duties ---------------------------------------------------------------------

/// A duty planned for the player himself ("My duties"): its day, its tour and duty in that
/// day's plan, its line and times, and the bus it is planned with.
#[derive(Clone, Debug, PartialEq)]
pub struct MyDuty {
    pub date: String,
    /// The tour (an index into the day plan's tours) and its duty.
    pub tour: usize,
    pub duty: usize,
    /// The company line (its timetable name), the number shown, the tour's number.
    pub line: String,
    pub number: String,
    pub tour_no: String,
    pub from: i32,
    pub to: i32,
    pub bus: Option<BusOf>,
    /// The trips with passengers it drives.
    pub trips: usize,
}

/// The player's duties in `plans` (the days of the planning, in any order), the next first:
/// those of the company's day that are over (`now`, minutes of `today`) or that he drove
/// already left out.
pub fn my_duties(plans: &[DayPlan], today: &str, now: i32) -> Vec<MyDuty> {
    let mut out: Vec<MyDuty> = Vec::new();
    for p in plans {
        // (days before the company's are done)
        if dates::between(today, &p.date) < 0 {
            continue;
        }
        for (ti, k) in p.duties_of(Who::Player) {
            let t = &p.tours[ti];
            let d = &t.duties[k];
            if t.by_player || (p.date == today && d.to <= now) {
                continue;
            }
            let trips = t.tour.trips[d.start..d.end].iter().filter(|x| x.counts()).count();
            out.push(MyDuty { date: p.date.clone(), tour: ti, duty: k, line: t.tour.line.clone(), number: t.tour.number.clone(), tour_no: t.tour.tour.clone(), from: d.from, to: d.to, bus: t.bus, trips });
        }
    }
    out.sort_by_key(|d| (dates::parse(&d.date).unwrap_or(0), d.from));
    out
}

/// The company's duties in `plans` the player can take (the easy mode's "Duties you can take",
/// whose planning is the company's own): those of tours in service that are not his already,
/// not of a tour he drove and not begun (`now`, minutes of `today`), the next first.
pub fn takeable(plans: &[DayPlan], today: &str, now: i32) -> Vec<MyDuty> {
    let mut out: Vec<MyDuty> = Vec::new();
    for p in plans.iter().filter(|p| dates::between(today, &p.date) >= 0) {
        for (ti, t) in p.tours.iter().enumerate() {
            if t.by_player || t.tour.unplanned {
                continue;
            }
            for (k, d) in t.duties.iter().enumerate() {
                if d.who == Some(Who::Player) || (p.date == today && d.from <= now) {
                    continue;
                }
                let trips = t.tour.trips[d.start..d.end].iter().filter(|x| x.counts()).count();
                out.push(MyDuty { date: p.date.clone(), tour: ti, duty: k, line: t.tour.line.clone(), number: t.tour.number.clone(), tour_no: t.tour.tour.clone(), from: d.from, to: d.to, bus: t.bus, trips });
            }
        }
    }
    out.sort_by_key(|d| (dates::parse(&d.date).unwrap_or(0), d.from));
    out
}

/// A stand-in for an agency driver in the day's model (average skills).
pub fn agency_driver() -> Employee {
    Employee {
        id: AGENCY,
        name: "Agency driver".into(),
        age: 40,
        experience: 40.0,
        licence: Licence::D,
        wage: 0,
        reliability: 0.8,
        skills: Skills { driving: 60.0, punctuality: 60.0, service: 50.0 },
        satisfaction: 60.0,
        hired: String::new(),
        notice_until: None,
        training_until: None,
        resigned: false,
        sick_until: None,
        holiday_until: None,
        holiday_left: 0,
        week_days: 0,
        last_end: None,
        days_worked: 0,
        // (the agency sends drivers for what they are sent to)
        endorsements: super::licences::Endorsement::ALL.to_vec(),
        types: Vec::new(),
        role: super::model::Role::Driver,
        stress: 20.0,
        friendliness: 60.0,
        portrait: 0,
        leave: Default::default(),
    }
}

/// The morning of a closing day booked: the buses that did not start (towed, in the
/// workshop for the day), the rental buses rented (they go back after the day), the agency
/// drivers paid. Returns the plan the close runs, and what to tell.
pub fn settle_morning(c: &mut Company, dp: &DayPlan) -> (Plan, Vec<Note>) {
    let mut plan = dp.to_plan();
    let mut notes = Vec::new();
    let date = c.date.clone();
    for d in &dp.disruptions {
        let Disruption::Breakdown { vehicle, cost } = *d else { continue };
        let Some(v) = c.fleet.iter_mut().find(|v| v.id == vehicle) else { continue };
        v.breakdowns += 1;
        if !v.in_workshop(&date) {
            v.workshop_until = Some(date.clone());
        }
        let (number, text) = (v.number.clone(), format!("{} {}", v.number, v.name));
        c.book(BookingKind::Repair, -cost, text, false);
        notes.push(Note::Breakdown { number, until: date.clone(), cost });
    }
    for (ti, t) in dp.tours.iter().enumerate() {
        if t.bus != Some(BusOf::Rental) {
            continue;
        }
        let wants = super::ownline::wanted(c, &t.tour);
        let model = c.fleet.iter().find(|v| wants.is_none_or(|w| w == v.kind.size) && !v.bus.is_empty()).or(c.fleet.iter().find(|v| !v.bus.is_empty()));
        let bus = match model {
            Some(v) => MarketBus { file: v.bus.clone(), name: v.name.clone(), kind: v.kind, ..Default::default() },
            None => MarketBus { name: "Rental bus".into(), kind: BusKind { size: wants.unwrap_or_default(), drive: Drive::Diesel }, ..Default::default() },
        };
        if let Ok(id) = market::rent(c, &bus, 1, "") {
            plan.tours[ti].bus = Some(id);
        }
    }
    let hour = AGENCY_HOUR as f64 * c.price_index;
    for t in &dp.tours {
        for d in t.duties.iter().filter(|d| d.who == Some(Who::Agency)) {
            let cost = ((d.to - d.from).max(0) as f64 / 60.0 * hour).round() as Cents;
            c.book(BookingKind::Agency, -cost, format!("Agency driver, line {} tour {}", t.tour.number, t.tour.tour), false);
        }
    }
    // (an agency driver sent for a late driver's first trips: their hours, `control`)
    for t in &dp.tours {
        for d in t.duties.iter() {
            let Some(l) = d.late.filter(|l| l.cover == Some(AGENCY)) else { continue };
            let from = l.cover_from.unwrap_or(d.from).max(d.from);
            let cost = ((l.until.min(d.to) - from).max(0) as f64 / 60.0 * hour).round() as Cents;
            c.book(BookingKind::Agency, -cost, format!("Agency driver, line {} tour {}", t.tour.number, t.tour.tour), false);
        }
    }
    // (and those booked ahead for the day: their hours booked, or driven when more)
    super::agency::settle(c, dp);
    (plan, notes)
}

// --- the game's side -------------------------------------------------------------------------

/// A company tour as the game runs it while the player drives: with this bus of the fleet
/// (its file, paint, number and plate), or not at all (`dropped`: no bus, no driver).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LiveTour {
    /// The timetable's line (`.ttl` name), the number shown, and the tour.
    pub line: String,
    pub number: String,
    pub tour: String,
    #[serde(default)]
    pub vehicle: Option<u32>,
    #[serde(default)]
    pub bus: String,
    #[serde(default)]
    pub paint: String,
    #[serde(default)]
    pub fleet_number: String,
    #[serde(default)]
    pub plate: String,
    #[serde(default)]
    pub dropped: bool,
    /// The depot file the bus carries on this line (empty: the plan's `depot`).
    #[serde(default)]
    pub hof: String,
}

/// The company's day for the game (`companies/<id>.liveplan.json`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LivePlan {
    pub company: String,
    pub map: String,
    pub date: String,
    /// The depot file its buses carry (their destination lists) and the economy (the
    /// passengers a trip carries).
    #[serde(default)]
    pub depot: String,
    #[serde(default)]
    pub game_mode: GameMode,
    pub tours: Vec<LiveTour>,
}

impl LivePlan {
    pub fn tour(&self, line: &str, tour: &str) -> Option<&LiveTour> {
        self.tours.iter().find(|t| same(&t.line, line) && same(&t.tour, tour))
    }

    /// What the game reports of a company trip its AI ran to the end: its kilometres, the
    /// passengers the model gives a trip of that length and hour (a depot run carries none),
    /// and how late it ended (s). None: not a tour the company runs.
    pub fn trip_event(&self, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64) -> Option<LiveEvent> {
        let t = self.tour(line, tour).filter(|t| !t.dropped)?;
        let km = if km.is_finite() { km.clamp(0.0, 200.0) } else { 0.0 };
        let passengers = if stops >= 3 { economy::passengers_for(km, dep, &economy::rules(self.game_mode)).round().max(0.0) as u32 } else { 0 };
        Some(LiveEvent::Trip { line: t.line.clone(), tour: t.tour.clone(), vehicle: t.vehicle, km, passengers, delay, completed: true, dep: Some(dep) })
    }

    /// Whether it is the plan of the map in `map_dir` (a map's folder) on `date`.
    pub fn is_for(&self, map_dir: &Path, date: &str) -> bool {
        let folder = |p: &str| p.replace('\\', "/").trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or("").to_lowercase();
        let here = map_dir.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        !here.is_empty() && folder(&self.map) == here && self.date == date
    }
}

/// The plan of the company's day for the game: each tour with its bus, and those dropped. The
/// game knows the timetable's tours: a block the scheduling studio laid out runs each of them
/// it holds most of with its bus (`studio::origins`).
pub fn live_plan(c: &Company, dp: &DayPlan) -> LivePlan {
    let tours = dp
        .tours
        .iter()
        // (a line not in service runs nothing, not even in the game)
        .filter(|t| !t.by_player && !t.tour.unplanned)
        .flat_map(|t| super::studio::origins(&dp.tours, t).into_iter().map(move |o| (t, o)))
        .map(|(t, origin)| {
            let hof = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&t.tour.line)).map(|l| l.hof.trim().to_string()).unwrap_or_default();
            let mut lt = LiveTour { line: t.tour.line.clone(), number: t.tour.number.clone(), tour: origin, hof, ..Default::default() };
            match t.bus {
                Some(BusOf::Own(id)) => {
                    if let Some(v) = c.vehicle(id) {
                        lt.vehicle = Some(id);
                        lt.bus = v.bus.clone();
                        lt.paint = v.house_livery.clone().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| v.livery.clone());
                        lt.fleet_number = v.number.clone();
                        lt.plate = v.plate.clone();
                    }
                }
                Some(BusOf::Rental) => {}
                None => lt.dropped = true,
            }
            lt.dropped |= t.bus.is_some() && t.duties.iter().all(|d| d.who.is_none());
            lt
        })
        .collect();
    LivePlan { company: c.id.clone(), map: c.map.clone(), date: dp.date.clone(), depot: c.depot.clone(), game_mode: c.game_mode, tours }
}

pub fn live_plan_file(data: &Path, id: &str) -> PathBuf {
    super::store::dir(data).join(format!("{id}.liveplan.json"))
}

/// Written whole (the game reads it when its timetable is loaded and at midnight).
pub fn save_live_plan(data: &Path, p: &LivePlan) -> anyhow::Result<()> {
    write_plan(&live_plan_file(data, &p.company), p)
}

/// The plan of the day of a duty driven ahead from the company's pages (My duties, a later
/// day): the game puts the company's buses on the road with it, but reports nothing - only the
/// company's own day (`live_plan_file`) is booked.
pub fn drive_plan_file(data: &Path, id: &str) -> PathBuf {
    super::store::dir(data).join(format!("{id}.drive.liveplan.json"))
}

pub fn save_drive_plan(data: &Path, p: &LivePlan) -> anyhow::Result<()> {
    write_plan(&drive_plan_file(data, &p.company), p)
}

/// The plan of a duty driven ahead (`drive_plan_file`), when the company is still there.
/// (None for an id that is no plain name: it comes from the command line.)
pub fn drive_plan(data: &Path, id: &str) -> Option<LivePlan> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    super::store::path_of(data, id).exists().then_some(())?;
    serde_json::from_str(&std::fs::read_to_string(drive_plan_file(data, id)).ok()?).ok()
}

/// A plan written whole, through a file beside it.
fn write_plan(path: &Path, p: &LivePlan) -> anyhow::Result<()> {
    let d = path.parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::create_dir_all(&d)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(p)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Every company's plan for the game (those a company file still stands beside; a duty
/// driven ahead, `<id>.drive.liveplan.json`, is none of them: no company `<id>.drive`).
pub fn live_plans(data: &Path) -> Vec<LivePlan> {
    let Ok(rd) = std::fs::read_dir(super::store::dir(data)) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            let id = n.strip_suffix(".liveplan.json")?.to_string();
            super::store::path_of(data, &id).exists().then_some(())?;
            serde_json::from_str::<LivePlan>(&std::fs::read_to_string(e.path()).ok()?).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::day::{close_day, record_live, LiveEvent};
    use super::super::market::Payment;
    use super::super::network::tests::trip;
    use super::super::network::PlannedTrip;
    use super::super::staff::{applicants, hire};
    use super::super::{found, Founding};
    use super::*;
    use crate::company::model::CompanyLine;

    fn tour(no: &str, from: i32, n: i32) -> TourOfDay {
        TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..n).map(|k| trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None }
    }

    fn company(buses: usize, people: usize) -> Company {
        // (Easy: the fewest late mornings and breakdowns; the tests that want them make them)
        let mut c = found(&Founding { name: "Plan".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        // (in service from the start, and the dispatcher's own filling what the roster leaves
        // free - as "Fill the roster" does when asked)
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c.planning.auto = true;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        c.cash += 5_000_000_00;
        for _ in 0..buses {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < people {
            for a in applicants(&c) {
                if c.staff.len() < people && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
            e.satisfaction = 80.0;
            e.reliability = 1.0;
            e.experience = 50.0;
        }
        c
    }

    /// A company whose morning brings nothing (its id chosen so).
    fn quiet(buses: usize, people: usize) -> Company {
        let mut c = company(buses, people);
        let base = c.id.clone();
        for n in 0.. {
            c.id = format!("{base}{n}");
            if disruptions(&c, &c.date).is_empty() {
                return c;
            }
        }
        unreachable!()
    }

    #[test]
    fn a_line_folded_on_the_chart_stays_folded_and_follows_its_name() {
        let mut c = company(0, 0);
        assert!(!is_collapsed(&c, "Linie5"));
        set_collapsed(&mut c, "Linie5", true);
        assert!(is_collapsed(&c, "linie5"));
        // (folded once only)
        set_collapsed(&mut c, "LINIE5", true);
        assert_eq!(c.planning.collapsed.len(), 1);
        // a new number is a new timetable name: still folded
        rename_line(&mut c, "Linie5", "oo_6");
        assert!(is_collapsed(&c, "oo_6") && !is_collapsed(&c, "Linie5"));
        set_collapsed(&mut c, "oo_6", false);
        assert!(c.planning.collapsed.is_empty());
        set_all_collapsed(&mut c, &["A".into(), "B".into()], true);
        assert!(is_collapsed(&c, "a") && is_collapsed(&c, "B"));
        set_all_collapsed(&mut c, &["A".into(), "B".into()], false);
        assert!(c.planning.collapsed.is_empty());
        // a company saved before there was folding reads as none folded
        let p: Planning = serde_json::from_str(r#"{"week":[],"date":"2024-03-04","fills":[]}"#).unwrap();
        assert!(p.collapsed.is_empty());
    }

    #[test]
    fn a_line_runs_with_its_own_buses_only() {
        use crate::service::{LineVehicles, VehicleClass};
        let mut c = quiet(1, 2);
        let wd = weekday_of(&c.date);
        // the line asks for minibuses: the solo bus is none of them, and the planning says so
        c.lines[0].plan = Some(super::super::ownline::OwnPlan { vehicles: LineVehicles { classes: vec![VehicleClass::Minibus], ..Default::default() }, ..Default::default() });
        let tours = vec![tour("1", 6 * 60, 3)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!((p.tours[0].bus, p.tours[0].bus_problem), (None, Some(Problem::NoLineBus)));
        // fixed in the roster all the same: not one of the line's
        let solo = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(solo));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!((p.tours[0].bus, p.tours[0].bus_problem), (None, Some(Problem::NotLineBus)));
        set_bus(&mut c, wd, "Linie5", "1", None);
        // a minibus bought: the dispatcher takes it
        let mini = MarketBus { file: "Vehicles/Sprinter/Sprinter.bus".into(), name: "Mercedes-Benz Sprinter".into(), kind: BusKind { size: BusSize::Midi, drive: Drive::Diesel }, ..Default::default() };
        market::buy_new(&mut c, &mini, Payment::Cash, "").unwrap();
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus, Some(BusOf::Own(c.fleet[1].id)));
        // "Fill the roster" gives it the bus, but nobody had the type training for the new model
        let mut f = c.clone();
        assert_eq!(fill_day(&mut f, &c.date.clone(), tours.clone()), 1);
        let p = day_plan(&f, &f.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::Unassigned));
        // given by hand all the same: the planning says why not
        let e1 = c.staff[0].id;
        set_driver(&mut f, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        let p = day_plan(&f, &f.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::NoTypeTraining));
        // trained: and it fills both, the bus and the driver
        let key = super::super::licences::type_key("Vehicles/Sprinter/Sprinter.bus");
        for e in c.staff.iter_mut() {
            e.types.push(key.clone());
        }
        assert_eq!(fill_day(&mut c.clone(), &c.date.clone(), tours), 2);
        // a line that asks for nothing takes any bus
        c.lines[0].plan = None;
        assert!(super::super::ownline::line_allows(&c, "Linie5", &c.fleet[0]));
    }

    #[test]
    fn a_lines_own_depot_file_and_title_go_with_it() {
        let mut c = quiet(1, 1);
        let p = day_plan(&c, &c.date, vec![tour("1", 6 * 60, 3)], &[], &[], false);
        // the company's depot file, unless the line has its own
        assert_eq!(live_plan(&c, &p).tours[0].hof, "");
        assert_eq!(c.lines[0].hof_or("Grundorf"), "Grundorf");
        c.lines[0].hof = "Spandau".into();
        assert_eq!(live_plan(&c, &p).tours[0].hof, "Spandau");
        assert_eq!(c.lines[0].hof_or("Grundorf"), "Spandau");
        // what it is called in public: its title, else where it goes
        c.lines[0].caption = "Markt – Bahnhof".into();
        assert_eq!(c.lines[0].public_name(), "Markt – Bahnhof");
        c.lines[0].title = " Shuttleverkehr Altenfeld - Wurzbach ".into();
        assert_eq!(c.lines[0].public_name(), "Shuttleverkehr Altenfeld - Wurzbach");
        // (an older file has neither)
        let old: CompanyLine = serde_json::from_str(r#"{"name":"5","number":"5","own":false,"added":"2024-03-04"}"#).unwrap();
        assert!(old.title.is_empty() && old.hof.is_empty());
    }

    #[test]
    fn my_duties_come_next_first() {
        let mut c = quiet(2, 1);
        let wd = weekday_of(&c.date);
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Player));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Player));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 14 * 60, 3)];
        let today = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        // (the roster is the weekday's: next week's same day has them too)
        let next_week = dates::add(&c.date, 7);
        let later = day_plan(&c, &next_week, tours.clone(), &[], &[], false);
        let mine = my_duties(&[later.clone(), today.clone()], &c.date, 0);
        assert_eq!(mine.len(), 4);
        assert_eq!((mine[0].date.as_str(), mine[0].tour_no.as_str(), mine[0].from, mine[0].trips), (c.date.as_str(), "1", 6 * 60, 3));
        assert_eq!((mine[1].tour_no.as_str(), mine[2].date.as_str()), ("2", next_week.as_str()));
        assert!(mine[0].bus.is_some() && mine[0].number == "5");
        // at noon the morning's duty is over
        let mine = my_duties(&[today.clone()], &c.date, 12 * 60);
        assert_eq!(mine.iter().map(|d| d.tour_no.as_str()).collect::<Vec<_>>(), vec!["2"]);
        // a tour he drove already is no duty to drive
        let driven = day_plan(&c, &c.date, tours.clone(), &[("5".into(), "2".into())], &[], false);
        assert!(my_duties(&[driven], &c.date, 12 * 60).is_empty());
        // days before the company's are done; nobody's duties are his
        assert!(my_duties(&[today], &dates::add(&c.date, 1), 0).is_empty());
        let nobody = day_plan(&quiet(2, 1), &c.date, tours, &[], &[], false);
        assert!(my_duties(&[nobody], &c.date, 0).is_empty());
    }

    #[test]
    fn the_duties_to_take_are_the_companys_not_begun() {
        let mut c = quiet(2, 1);
        let wd = weekday_of(&c.date);
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Player));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 14 * 60, 3)];
        let today = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        // his own duty is none to take, the company's other one is
        let open = takeable(std::slice::from_ref(&today), &c.date, 0);
        assert_eq!(open.iter().map(|d| (d.tour_no.as_str(), d.from, d.trips)).collect::<Vec<_>>(), vec![("2", 14 * 60, 3)]);
        // begun, it is not; nor is a tour he drove, a tour not in service or a day gone
        assert!(takeable(std::slice::from_ref(&today), &c.date, 14 * 60).is_empty());
        let driven = day_plan(&c, &c.date, tours.clone(), &[("5".into(), "2".into())], &[], false);
        assert!(takeable(&[driven], &c.date, 0).is_empty());
        let mut unplanned = tours.clone();
        unplanned[1].unplanned = true;
        assert!(takeable(&[day_plan(&c, &c.date, unplanned, &[], &[], false)], &c.date, 0).is_empty());
        assert!(takeable(&[today], &dates::add(&c.date, 1), 0).is_empty());
        // taken, it is his on that weekday: next week's is among his duties, none to take
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Player));
        let later = day_plan(&c, &dates::add(&c.date, 7), tours, &[], &[], false);
        assert!(takeable(std::slice::from_ref(&later), &c.date, 0).is_empty());
        assert_eq!(my_duties(&[later], &c.date, 0).len(), 2);
    }

    #[test]
    fn a_tour_is_cut_into_duties_of_at_most_nine_and_a_half_hours() {
        // 5:00 to 23:00, a cut after a layover near the middle; 9½ h at most a duty
        let c = quiet(1, 3);
        let p = day_plan(&c, &c.date, vec![tour("1", 5 * 60, 18)], &[], &[], false);
        let d = &p.tours[0].duties;
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.to - x.from <= staff::DUTY_MAX));
        assert_eq!((d[0].start, d[1].end), (0, 18));
        assert_eq!(d[0].end, d[1].start);
        // a cut only before a trip with passengers, after the bus stood: a depot run at the
        // start goes with the first duty
        let mut trips: Vec<PlannedTrip> = vec![trip(280, 295, 2)];
        trips.extend((0..18).map(|k| trip(300 + k * 60, 350 + k * 60, 12)));
        let t = TourOfDay { trips, ..tour("2", 0, 0) };
        let p = day_plan(&c, &c.date, vec![t], &[], &[], false);
        assert_eq!(p.tours[0].duties[0].start, 0);
        assert!(p.tours[0].tour.trips[p.tours[0].duties[1].start].counts());
    }

    #[test]
    fn the_roster_comes_first_and_the_dispatcher_fills_the_rest_by_the_rules() {
        let mut c = quiet(2, 3);
        let wd = weekday_of(&c.date);
        let (b1, b2) = (c.fleet[0].id, c.fleet[1].id);
        let (e1, e2) = (c.staff[0].id, c.staff[1].id);
        // the second bus and the second driver fixed for tour 1
        set_bus(&mut c, wd, "Linie5", "1", Some(b2));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e2)));
        let tours = vec![tour("1", 6 * 60, 6), tour("2", 6 * 60 + 30, 6)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let t1 = p.tours.iter().find(|t| t.tour.tour == "1").unwrap();
        assert_eq!((t1.bus, t1.bus_from), (Some(BusOf::Own(b2)), Source::Roster));
        assert_eq!((t1.duties[0].who, t1.duties[0].from_), (Some(Who::Staff(e2)), Source::Roster));
        let t2 = p.tours.iter().find(|t| t.tour.tour == "2").unwrap();
        assert_eq!((t2.bus, t2.bus_from), (Some(BusOf::Own(b1)), Source::Auto));
        assert!(matches!(t2.duties[0].who, Some(Who::Staff(id)) if id != e2));
        assert_eq!(p.counts(), (2, 2, 0, 0));
        // the same driver on two duties at the same time: the second clashes
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e2)));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let t2 = p.tours.iter().find(|t| t.tour.tour == "2").unwrap();
        assert_eq!(t2.duties[0].problem, Some(Problem::Conflict));
        // without the dispatcher's own, it stays open
        c.planning.auto = false;
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert!(p.tours.iter().find(|t| t.tour.tour == "2").unwrap().duties[0].open());
        c.planning.auto = true;
        // the rest between two duties of one driver: 20 minutes at least
        set_driver(&mut c, wd, "Linie5", "2", 0, None);
        let short = vec![tour("1", 6 * 60, 3), tour("2", 6 * 60 + 3 * 60 + 5, 3)];
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e2)));
        let p = day_plan(&c, &c.date, short, &[], &[], false);
        assert_eq!(p.tours[1].duties[0].problem, Some(Problem::Conflict));
        // a D1 licence drives no solo bus
        c.staff[0].licence = Licence::D1;
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e1)));
        let p = day_plan(&c, &c.date, vec![tour("2", 12 * 60, 3)], &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::Licence));
        // the night: yesterday's last duty ended at 23:00 - not before 10:00 today
        c.staff[0].licence = Licence::D;
        c.staff[0].last_end = Some(23 * 60);
        let p = day_plan(&c, &c.date, vec![tour("2", 6 * 60, 3)], &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::Conflict));
        // on holiday (known ahead, Beta 57): the central covers it - nobody free, an agency
        // driver -, but on Realistic it stays open for the player
        c.staff[0].last_end = None;
        c.staff[0].holiday_until = Some(c.date.clone());
        c.staff[1].holiday_until = Some(c.date.clone());
        c.staff[2].holiday_until = Some(c.date.clone());
        let p = day_plan(&c, &c.date, vec![tour("2", 12 * 60, 3)], &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::Holiday));
        assert_eq!((p.tours[0].duties[0].who, p.tours[0].duties[0].from_), (Some(Who::Agency), Source::Central));
        c.game_mode = GameMode::Realistic;
        let p = day_plan(&c, &c.date, vec![tour("2", 12 * 60, 3)], &[], &[], false);
        assert!(p.tours[0].duties[0].open());
        // (a colleague free: they cover it, on any day planned, the roster kept the driver's)
        c.staff[1].holiday_until = None;
        let later = dates::add(&c.date, 7);
        c.staff[0].holiday_until = Some(later.clone());
        let p = day_plan(&c, &later, vec![tour("2", 12 * 60, 3)], &[], &[], false);
        let d = &p.tours[0].duties[0];
        assert!(matches!(d.who, Some(Who::Staff(id)) if id != e1) && d.problem == Some(Problem::Holiday) && d.from_ == Source::Central, "{d:?}");
        assert!(broken(&c, &later, vec![tour("2", 12 * 60, 3)]).1.is_empty());
        assert_eq!(roster(&c, wd, "Linie5", "2").unwrap().duties[0], Some(Who::Staff(e1)));
    }

    #[test]
    fn a_driver_without_the_endorsement_is_not_planned_on_the_bus() {
        use super::super::licences::{type_key, Endorsement};
        let mut c = quiet(1, 2);
        let wd = weekday_of(&c.date);
        // the company's bus an articulated one: nobody has the endorsement
        c.fleet[0].kind = BusKind { size: BusSize::Articulated, drive: Drive::Diesel };
        for e in c.staff.iter_mut() {
            e.endorsements.clear();
            e.types = vec![type_key(&c.fleet[0].bus)];
        }
        let e1 = c.staff[0].id;
        let tours = vec![tour("1", 6 * 60, 3)];
        let b1 = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::NoEndorsement));
        // the dispatcher gives it to nobody, and says why
        set_driver(&mut c, wd, "Linie5", "1", 0, None);
        assert_eq!(fill_day(&mut c.clone(), &c.date.clone(), tours.clone()), 0);
        let f = fill_day_told(&mut c.clone(), &c.date.clone(), tours.clone());
        assert_eq!((f.duties, f.unqualified, f.busy), (0, 1, 0));
        // with the endorsement: planned
        c.staff[0].endorsements.push(Endorsement::Articulated);
        assert_eq!(fill_day(&mut c.clone(), &c.date.clone(), tours), 1);
    }

    #[test]
    fn filling_the_roster_keeps_the_rules_and_clearing_empties_it() {
        let mut c = quiet(2, 2);
        let wd = weekday_of(&c.date);
        // a long tour (two duties) and a short one at the same time: two buses, but only two
        // drivers - one of them may not drive two duties of over ten hours together
        let tours = vec![tour("1", 5 * 60, 18), tour("2", 6 * 60, 4)];
        let today = c.date.clone();
        let n = fill_day(&mut c, &today, tours.clone());
        assert!(n >= 3);
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert!(p.tours.iter().all(|t| t.bus_from == Source::Roster));
        // every duty that has a driver is the roster's, and nobody works over ten hours
        for e in &c.staff {
            let mine = p.duties_of(Who::Staff(e.id));
            let minutes: i32 = mine.iter().map(|&(i, k)| p.tours[i].duties[k].to - p.tours[i].duties[k].from).sum();
            assert!(minutes <= staff::DAY_MAX, "{minutes}");
        }
        assert!(p.tours.iter().flat_map(|t| t.duties.iter()).filter(|d| d.who.is_some()).all(|d| d.from_ == Source::Roster));
        // the roster repeats: next week's Monday is the same
        let next = dates::add(&c.date, 7);
        let mut later = c.clone();
        later.date = next.clone();
        let q = day_plan(&later, &next, tours.clone(), &[], &[], false);
        assert_eq!(q.tours.iter().map(|t| t.bus).collect::<Vec<_>>(), p.tours.iter().map(|t| t.bus).collect::<Vec<_>>());
        // filled again: nothing more to give, and what is open said
        let again = fill_day_told(&mut c.clone(), &today, tours.clone());
        assert_eq!((again.duties, again.buses), (0, 0));
        let open = p.tours.iter().flat_map(|t| t.duties.iter()).filter(|d| d.who.is_none()).count();
        assert_eq!(again.unqualified + again.busy, open);
        assert_eq!(again.no_bus, p.tours.iter().filter(|t| t.bus.is_none()).count());
        // copied to Tuesday, cleared on Monday: each says what it did
        let held = rostered(&c, wd);
        assert!(held.buses >= 1 && held.duties >= 1);
        assert_eq!(copy_day(&mut c, wd, &[wd, (wd + 1) % 7]), vec![(wd + 1) % 7]);
        assert_eq!(clear_day(&mut c, wd), held);
        assert!(c.planning.week.iter().all(|r| r.weekday == (wd + 1) % 7));
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        assert!(p.tours.iter().all(|t| t.bus_from != Source::Roster));
        // an empty day is not repeated: the others keep theirs
        assert!(copy_day(&mut c, wd, &[(wd + 1) % 7]).is_empty());
        assert_eq!(rostered(&c, (wd + 1) % 7), held);
        assert!(clear_day(&mut c, wd).is_empty());
    }

    #[test]
    fn what_falls_out_is_filled_or_dropped() {
        let mut c = quiet(2, 3);
        let wd = weekday_of(&c.date);
        let (b1, e1) = (c.fleet[0].id, c.staff[0].id);
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        let tours = vec![tour("1", 6 * 60, 4)];
        // ill: the central takes a spare colleague
        c.staff[0].sick_until = Some(c.date.clone());
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let d = &p.tours[0].duties[0];
        assert_eq!((d.problem, d.from_), (Some(Problem::Sick), Source::Central));
        assert!(matches!(d.who, Some(Who::Staff(id)) if id != e1));
        assert_eq!(p.open_rows().len(), 1);
        assert!(p.open_rows()[0].filled);
        // the dispatcher drops it instead
        set_fill(&mut c, &duty_key("Linie5", "1", 0), Some(Fill::Drop));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert!(p.tours[0].duties[0].open());
        // or takes an agency driver
        set_fill(&mut c, &duty_key("Linie5", "1", 0), Some(Fill::Agency));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].who, Some(Who::Agency));
        // nobody spare: an agency driver by the central
        set_fill(&mut c, &duty_key("Linie5", "1", 0), None);
        for e in c.staff.iter_mut().skip(1) {
            e.sick_until = Some(e.hired.clone());
            e.holiday_until = Some(dates::add(&e.hired, 30));
        }
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!((p.tours[0].duties[0].who, p.tours[0].duties[0].from_), (Some(Who::Agency), Source::Central));
        // the choices are for the day: tomorrow they are gone
        set_fill(&mut c, &duty_key("Linie5", "1", 0), Some(Fill::Drop));
        let mut t = c.clone();
        t.date = dates::add(&c.date, 1);
        assert!(fill_of(&t, &t.date, &duty_key("Linie5", "1", 0)).is_none());
        // a bus that did not start: another free bus, else a rental bus
        let mut c = quiet(1, 2);
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        c.fleet[0].workshop_until = Some(c.date.clone());
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus_problem, Some(Problem::Workshop));
        // (the workshop is known the evening before: not the central's)
        assert_eq!(p.tours[0].bus, None);
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Rental));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus, Some(BusOf::Rental));
        // the close rents it for the day and gives it back
        let n = c.fleet.len();
        let r = close_day(&mut c, tours.clone(), &[]);
        assert_eq!(r.covered, 1);
        assert_eq!(c.fleet.len(), n);
        assert!(c.month(&dates::month_of(&r.date)).get(BookingKind::Rent) < 0);
    }

    #[test]
    fn a_rental_bus_needs_a_place_in_the_depot() {
        let mut c = quiet(1, 2);
        let wd = weekday_of(&c.date);
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        c.fleet[0].workshop_until = Some(c.date.clone());
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Rental));
        let tours = vec![tour("1", 6 * 60, 4)];
        // the yard and the street full: no rental bus, the tour stays open
        c.site.parking = 0;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        while super::super::depot::room(&c).is_ok() {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus, None);
        // a place free: rented at the close, the tour runs
        c.fleet.pop();
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus, Some(BusOf::Rental));
        assert_eq!(close_day(&mut c, tours, &[]).covered, 1);
    }

    #[test]
    fn a_changed_line_planned_anew_takes_no_bus_from_the_others_unplanned_tours() {
        let mut c = quiet(1, 2);
        // another line's tour earlier in the morning, given nothing in the roster: it does not
        // run, so the one bus is free for the changed line
        let other = TourOfDay { line: "Linie7".into(), number: "7".into(), ..tour("1", 5 * 60, 3) };
        let tours = vec![other, tour("1", 6 * 60, 3)];
        let date = c.date.clone();
        let f = fill_line(&mut c, &date, tours.clone(), "Linie5");
        assert_eq!((f.buses, f.no_bus), (1, 0));
        let wd = weekday_of(&date);
        assert_eq!(roster(&c, wd, "Linie5", "1").and_then(|r| r.bus), Some(c.fleet[0].id));
        assert!(roster(&c, wd, "Linie7", "1").is_none());
        // "Fill the roster" for the whole day weighs them all: the earlier tour has the bus
        let mut all = quiet(1, 2);
        fill_day(&mut all, &date, tours);
        assert!(roster(&all, wd, "Linie7", "1").is_some_and(|r| r.bus.is_some()));
    }

    #[test]
    fn a_late_driver_and_a_morning_breakdown() {
        // a company whose morning has a late driver and a bus that does not start
        let mut c = company(3, 4);
        for e in c.staff.iter_mut() {
            e.satisfaction = 10.0;
            e.reliability = 0.0;
        }
        for v in c.fleet.iter_mut() {
            v.condition = 21.0;
        }
        c.game_mode = GameMode::Realistic;
        let base = c.id.clone();
        let mut found_one = false;
        for n in 0..4000 {
            c.id = format!("{base}{n}");
            let d = disruptions(&c, &c.date);
            if d.iter().any(|x| matches!(x, Disruption::Late { .. })) && d.iter().any(|x| matches!(x, Disruption::Breakdown { .. })) {
                found_one = true;
                break;
            }
        }
        assert!(found_one);
        let dis = disruptions(&c, &c.date);
        // the same morning again
        assert_eq!(dis, disruptions(&c.clone(), &c.date));
        let Some(Disruption::Late { employee, minutes }) = dis.iter().find(|x| matches!(x, Disruption::Late { .. })).copied() else { unreachable!() };
        let Some(Disruption::Breakdown { vehicle, .. }) = dis.iter().find(|x| matches!(x, Disruption::Breakdown { .. })).copied() else { unreachable!() };
        let wd = weekday_of(&c.date);
        set_bus(&mut c, wd, "Linie5", "1", Some(vehicle));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(employee)));
        let tours = vec![tour("1", 6 * 60, 6)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let t = &p.tours[0];
        // the broken bus: the central's free one instead
        assert_eq!(t.bus_problem, Some(Problem::Breakdown));
        assert!(matches!(t.bus, Some(BusOf::Own(id)) if id != vehicle) || t.bus == Some(BusOf::Rental));
        assert_eq!(t.bus_from, Source::Central);
        // the late driver still drives, from `minutes` on; the piece before is covered or not
        let d = &t.duties[0];
        assert_eq!(d.who, Some(Who::Staff(employee)));
        let late = d.late.unwrap();
        assert_eq!(late.until, 6 * 60 + minutes);
        // nobody covers it: its trips before are dropped at the close
        set_fill(&mut c, &late_key("Linie5", "1", 0), Some(Fill::Drop));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].late.unwrap().cover, None);
        assert_eq!(p.to_plan().tours[0].duties[0].dropped_before, Some(6 * 60 + minutes));
        let before = p.tours[0].tour.trips.iter().filter(|x| x.dep < 6 * 60 + minutes).count() as u32;
        let r = close_day(&mut c, tours, &[]);
        assert!(r.dropped >= before);
        assert!(r.notes.iter().any(|n| matches!(n, Note::Breakdown { .. })));
    }

    #[test]
    fn the_players_duty_is_his_or_open_at_the_close() {
        let mut c = quiet(1, 2);
        let wd = weekday_of(&c.date);
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Player));
        let tours = vec![tour("1", 6 * 60, 4)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].who, Some(Who::Player));
        assert!(p.to_plan().tours[0].covered());
        // closing without his trips: the central's spare colleague drives it
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], true);
        let d = &p.tours[0].duties[0];
        assert_eq!(d.problem, Some(Problem::Player));
        assert!(matches!(d.who, Some(Who::Staff(_))));
        // driven: nothing to fill
        let p = day_plan(&c, &c.date, tours, &[("5".into(), "1".into())], &[], true);
        assert!(p.tours[0].by_player && p.tours[0].covered());
    }

    #[test]
    fn the_live_company_is_planned_recorded_and_settled_for_the_rest() {
        let mut c = quiet(2, 4);
        let tours = vec![tour("1", 6 * 60, 6), tour("2", 6 * 60 + 30, 6)];
        // the game's plan: both tours with the fleet's buses, the house livery if there is one
        c.fleet[0].house_livery = Some("House".into());
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let lp = live_plan(&c, &p);
        assert_eq!(lp.tours.len(), 2);
        assert!(lp.tours.iter().all(|t| t.vehicle.is_some() && !t.dropped && t.bus.ends_with(".bus")));
        assert!(lp.tours.iter().any(|t| t.paint == "House"));
        assert!(lp.is_for(Path::new("C:/OMSI 2/maps/Grundorf"), &c.date));
        assert!(!lp.is_for(Path::new("C:/OMSI 2/maps/Ahlheim"), &c.date));
        assert!(lp.tour("Linie5", "2").is_some());
        // written for the game and read back
        let data = std::env::temp_dir().join(format!("openomsi-liveplan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        super::super::store::save(&data, &c).unwrap();
        save_live_plan(&data, &lp).unwrap();
        assert_eq!(live_plans(&data), vec![lp.clone()]);
        // the game drove the first three trips of tour 1 and reported them
        let v = lp.tour("Linie5", "1").unwrap().vehicle;
        for k in 0..3 {
            let ev = lp.trip_event("linie5", "1", 6 * 60 + k * 60, 15.0, 12, 40.0).unwrap();
            assert!(matches!(&ev, LiveEvent::Trip { vehicle, passengers, dep, .. } if *vehicle == v && *passengers > 0 && *dep == Some(6 * 60 + k * 60)));
            super::super::store::append_live(&data, &c.id, &ev).unwrap();
        }
        // (a tour not the company's is none of its business)
        assert!(lp.trip_event("Linie7", "1", 400, 10.0, 12, 0.0).is_none());
        super::super::store::take_live(&data, &mut c);
        assert_eq!(c.live.len(), 3);
        let _ = std::fs::remove_dir_all(&data);
        let km_before = c.vehicle(v.unwrap()).unwrap().km;
        let r = close_day(&mut c, tours.clone(), &[]);
        // measured: the three live trips; the model the other nine - nothing dropped
        assert!(c.ledger.iter().filter(|b| b.measured && b.kind == BookingKind::Fares).count() >= 3);
        assert_eq!(r.trips, 12);
        assert_eq!(r.dropped, 0);
        assert_eq!(r.covered, 2);
        // the bus ran the live kilometres and the modelled ones
        assert!(c.vehicle(v.unwrap()).unwrap().km >= km_before + 45.0 + 3.0 * 15.0 - 1.0);
        // an event without its departure takes the whole tour, as before
        record_live(&mut c, LiveEvent::Trip { line: "5".into(), tour: "2".into(), vehicle: None, km: 8.0, passengers: 20, delay: 30.0, completed: true, dep: None });
        let r = close_day(&mut c, tours, &[]);
        assert_eq!(r.covered, 2);
        assert_eq!(r.trips, 6 + 6);
    }

    #[test]
    fn the_agency_sends_as_many_drivers_as_are_asked_for() {
        // (Luc, 2026-10-15: "you must be able to hire as many as you want" - it was two a day, and
        // one more for every three drivers)
        let mut c = quiet(4, 3);
        // four tours with a bus and nobody planned, an agency driver asked for each: four come
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let tours: Vec<TourOfDay> = (0..4).map(|k| tour(&(k + 1).to_string(), 6 * 60 + k as i32 * 5, 2)).collect();
        for k in 0..4 {
            let id = c.fleet[k].id;
            set_bus(&mut c, wd, "Linie5", &(k + 1).to_string(), Some(id));
            set_fill(&mut c, &duty_key("Linie5", &(k + 1).to_string(), 0), Some(Fill::Agency));
        }
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        assert_eq!(agency_used(&p), 4);
        assert_eq!(p.tours.iter().filter(|t| t.duties[0].open()).count(), 0);
        assert!(agency_room(&c, &p, 0, 0));
    }

    #[test]
    fn a_driver_who_cannot_take_a_duty_says_why() {
        // (testers: "confused by how it says drivers are free but unable to cover duties")
        let mut c = quiet(2, 4);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let (b1, b2) = (c.fleet[0].id, c.fleet[1].id);
        let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_bus(&mut c, wd, "Linie5", "2", Some(b2));
        // the first drives the other tour then; the second worked the week's days; the third is ill
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(ids[0])));
        c.staff[1].week_days = WEEK_DAYS;
        c.staff[2].sick_until = Some(c.date.clone());
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 6 * 60 + 10, 3)];
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        assert_eq!(p.tours[0].tour.tour, "1");
        let cs = candidates(&c, &p, 0, 0);
        // the one who can take it first, a standby driver (no other duty today)
        assert_eq!(cs[0].id, ids[3]);
        assert!(cs[0].standby && cs[0].fit.is_ok());
        let why = |id: u32| cs.iter().find(|x| x.id == id).unwrap().fit.clone().unwrap_err();
        assert!(matches!(why(ids[0]), Cannot::OnDuty { ref key, .. } if key == &duty_key("Linie5", "2", 0)));
        assert_eq!(why(ids[1]), Cannot::WeekDone { days: WEEK_DAYS });
        assert_eq!(why(ids[2]), Cannot::Ill { until: c.date.clone() });
        // (the old check let the one with the week's days worked through: the day said no)
        let e1 = c.employee(ids[1]).unwrap();
        assert_eq!(day_unavailable(e1, &c.date, true), Some(Cannot::WeekDone { days: WEEK_DAYS }));
        // the night's rest: back late yesterday, free only from the morning's rest
        c.staff[3].last_end = Some(23 * 60);
        let e3 = c.employee(ids[3]).unwrap();
        let t = &p.tours[0];
        let b = block_of(&t.tour, &t.duties[0], duty_key("Linie5", "1", 0));
        assert_eq!(can_take(e3, &c.date, true, &t.bus_of(&c), &[], &b), Err(Cannot::NightRest { from: 23 * 60 + staff::NIGHT_REST - 1440 }));
        assert_eq!(free_from(e3, true), Some(10 * 60));
    }

    #[test]
    fn a_sick_drivers_duty_offers_every_way_to_cover_it() {
        // (testers: "when drivers go sick, instead of dropping the shift give me the option to cover")
        let mut c = quiet(1, 3);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
        let b1 = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(ids[0])));
        c.staff[0].sick_until = Some(c.date.clone());
        // one colleague worked the week's days: not offered (the lists say why)
        c.staff[2].week_days = WEEK_DAYS;
        let tours = vec![tour("1", 6 * 60, 4)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].problem, Some(Problem::Sick));
        let covers = covers_of_duty(&c, &p, 0, 0);
        assert_eq!(covers[0], Cover::Colleague { id: ids[1], standby: true, warn: Vec::new() });
        assert!(!covers.iter().any(|x| matches!(x, Cover::Colleague { id, .. } if *id == ids[2] || *id == ids[0])));
        assert!(covers.contains(&Cover::Agency { used: 0 }));
        assert_eq!(&covers[covers.len() - 2..], &[Cover::Player, Cover::Drop { penalty: drop_penalty(&c, &p.tours[0].tour, 0, 4) }]);
        // the penalty shown is the contract's (none on an easy economy)
        let mut hard = c.clone();
        hard.game_mode = GameMode::Normal;
        let r = economy::rules(GameMode::Normal);
        assert_eq!(drop_penalty(&hard, &p.tours[0].tour, 0, 4), 4 * (r.drop_per_trip + (15.0 * r.drop_per_km as f64).round() as Cents));
        // each chosen runs the duty so
        for cv in &covers[..covers.len() - 1] {
            set_fill(&mut c, &duty_key("Linie5", "1", 0), Some(cv.fill()));
            let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
            let want = match cv {
                Cover::Colleague { id, .. } => Who::Staff(*id),
                Cover::Agency { .. } => Who::Agency,
                _ => Who::Player,
            };
            assert_eq!((p.tours[0].duties[0].who, p.tours[0].duties[0].from_), (Some(want), Source::Dispatcher));
        }
        // the player's own today: among his duties; closed without him driving it, it is open
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(my_duties(&[p], &c.date, 0).len(), 1);
        assert!(day_plan(&c, &c.date, tours.clone(), &[], &[], true).tours[0].duties[0].open());
        // an older save's choices still read; the new one is kept by its name
        let f: FillChoice = serde_json::from_str(r#"{"key":"a","fill":{"kind":"agency"}}"#).unwrap();
        assert_eq!(f.fill, Fill::Agency);
        assert_eq!(serde_json::to_string(&Fill::Player).unwrap(), r#"{"kind":"player"}"#);
    }

    #[test]
    fn a_tour_without_its_bus_is_offered_the_fleets_spare_buses() {
        let mut c = quiet(3, 2);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let (b1, b2, b3) = (c.fleet[0].id, c.fleet[1].id, c.fleet[2].id);
        let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_bus(&mut c, wd, "Linie5", "2", Some(b2));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(ids[0])));
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(ids[1])));
        // the first tour's bus is in the workshop; the second's is out at the same time
        c.fleet[0].workshop_until = Some(c.date.clone());
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 6 * 60 + 10, 3)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].bus, None);
        assert_eq!(spare_buses(&c, &p, 0), vec![b3]);
        let (rent, ok) = rental_for(&c, &p, 0);
        assert!(rent > 0 && ok);
        // sent for today: the tour runs with it, and the close counts it
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Bus { id: b3 }));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!((p.tours[0].bus, p.tours[0].bus_from), (Some(BusOf::Own(b3)), Source::Dispatcher));
        assert!(p.tours[0].covered());
        assert_eq!(close_day(&mut c, tours, &[]).covered, 2);
    }

    #[test]
    fn rosterflow_shows_what_the_days_roster_has_open() {
        // (testers: "the tour was covered showing no issues")
        use super::super::rosterflow::{day_issues, I_DAY, I_DAY_FILLED, I_DAY_UNPLANNED};
        let mut c = quiet(1, 2);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let (b1, e1) = (c.fleet[0].id, c.staff[0].id);
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 9 * 60, 3)];
        // all in order but the second tour, which nobody is planned for
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let is = day_issues(&p);
        assert_eq!(is.iter().map(|i| i.text).collect::<Vec<_>>(), vec![I_DAY_UNPLANNED]);
        // the bus in the workshop: an error until it is covered for the day
        c.fleet[0].workshop_until = Some(c.date.clone());
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let is = day_issues(&p);
        assert!(is.iter().any(|i| i.text == I_DAY && i.hard && i.args.contains(&("rule", "In the workshop".to_string()))));
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Rental));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert!(p.tours[0].covered());
        let is = day_issues(&p);
        assert!(is.iter().any(|i| i.text == I_DAY_FILLED && !i.hard));
        assert!(!is.iter().any(|i| i.text == I_DAY));
    }

    #[test]
    fn the_roster_repairs_what_broke_and_keeps_what_is_good() {
        let mut c = quiet(3, 4);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let (b1, b2) = (c.fleet[0].id, c.fleet[1].id);
        let (e1, e2) = (c.staff[0].id, c.staff[1].id);
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        set_bus(&mut c, wd, "Linie5", "2", Some(b2));
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e2)));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 6 * 60 + 10, 3), tour("3", 6 * 60 + 20, 3)];
        // all good: nothing to repair
        let today = c.date.clone();
        assert_eq!(repair_day(&mut c, &today, tours.clone()), Repaired::default());
        // tour 1's driver ill, its bus in the workshop
        c.staff[0].sick_until = Some(c.date.clone());
        c.fleet[0].workshop_until = Some(c.date.clone());
        assert_eq!(broken(&c, &c.date, tours.clone()), (vec![("Linie5".to_string(), "1".to_string())], vec![("Linie5".to_string(), "1".to_string(), 0)]));
        let date = c.date.clone();
        let r = repair_day(&mut c, &date, tours.clone());
        assert_eq!((r.broken, r.duties, r.buses, r.open()), (2, 1, 1, 0));
        let one = roster(&c, wd, "Linie5", "1").unwrap().clone();
        assert!(one.bus.is_some_and(|b| b != b1 && b != b2));
        assert!(matches!(one.duties[0], Some(Who::Staff(id)) if id != e1 && id != e2));
        // (a driver on holiday is not broken, Beta 57: the day covers them, their place kept)
        c.staff[1].holiday_until = Some(date.clone());
        assert!(broken(&c, &date, tours.clone()).1.is_empty());
        c.staff[1].holiday_until = None;
        // the good one kept, and the tour the roster left free still free
        assert_eq!(roster(&c, wd, "Linie5", "2").map(|r| (r.bus, r.duties.clone())), Some((Some(b2), vec![Some(Who::Staff(e2))])));
        assert!(roster(&c, wd, "Linie5", "3").is_none());
        let p = day_plan(&c, &date, tours.clone(), &[], &[], false);
        assert!(p.tours[0].covered() && p.tours[1].covered());
        // the inbox is told, once a morning
        tell_repaired(&mut c, &r);
        tell_repaired(&mut c, &r);
        assert_eq!(c.inbox.messages.iter().filter(|m| m.title == REPAIRED.0).count(), 1);
        // nobody free: what broke stays open, and is said
        c.staff[1].sick_until = Some(date.clone());
        c.staff[2].holiday_until = Some(date.clone());
        c.staff[3].holiday_until = Some(date.clone());
        let r = repair_day(&mut c, &date, tours.clone());
        assert_eq!((r.broken, r.duties, r.open()), (1, 0, 1));
    }

    #[test]
    fn normal_mornings_are_clearly_calmer_than_realistic_ones_and_casual_ones_rare() {
        // (testers: "half the tours fall out every day, even with everything maxed")
        let base = company(6, 12);
        let count = |m: GameMode| {
            let mut c = base.clone();
            c.game_mode = m;
            for v in c.fleet.iter_mut() {
                v.condition = 60.0;
            }
            (0..300).map(|d| disruptions(&c, &dates::add(&c.date, d)).len()).sum::<usize>()
        };
        let (casual, normal, realistic) = (count(GameMode::Casual), count(GameMode::Normal), count(GameMode::Realistic));
        assert!(casual * 2 < normal && normal * 2 < realistic, "{casual} {normal} {realistic}");
        // and the rules say so
        let r = |m: GameMode| m.rules();
        assert!(r(GameMode::Normal).sickness < 1.0 && r(GameMode::Normal).breakdowns < 1.0 && r(GameMode::Normal).lateness < 1.0);
        assert_eq!((r(GameMode::Realistic).sickness, r(GameMode::Realistic).breakdowns, r(GameMode::Realistic).lateness), (1.0, 1.0, 1.0));
        assert!(r(GameMode::Normal).repair_roster && !r(GameMode::Realistic).repair_roster && !r(GameMode::Casual).repair_roster);
    }

    #[test]
    fn fill_the_roster_repairs_without_clearing_the_day() {
        let mut c = quiet(2, 3);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let (b1, e1, e2) = (c.fleet[0].id, c.staff[0].id, c.staff[1].id);
        set_bus(&mut c, wd, "Linie5", "1", Some(b1));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e1)));
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e2)));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 6 * 60 + 10, 3)];
        // tour 2's driver gone, and no bus planned for it
        c.staff[1].notice_until = Some(dates::add(&c.date, -1));
        let date = c.date.clone();
        let f = fill_day_told(&mut c, &date, tours.clone());
        assert!(f.duties >= 1 && f.buses >= 1, "{f:?}");
        // tour 1 as it was, tour 2 given a driver who can, and the free bus
        assert_eq!(roster(&c, wd, "Linie5", "1").map(|r| (r.bus, r.duties.clone())), Some((Some(b1), vec![Some(Who::Staff(e1))])));
        let two = roster(&c, wd, "Linie5", "2").unwrap();
        assert!(matches!(two.duties[0], Some(Who::Staff(id)) if id != e1 && id != e2));
        assert!(two.bus.is_some_and(|b| b != b1));
    }

    /// `tour` with its duties cut before trips `cuts` (as the studio cuts them).
    fn cut_tour(no: &str, from: i32, n: i32, cuts: &[usize]) -> TourOfDay {
        TourOfDay { cuts: Some(cuts.to_vec()), ..tour(no, from, n) }
    }

    #[test]
    fn an_agency_driver_takes_the_whole_duty_its_joined_pieces_too() {
        // (testers: "when I assign an agency driver to duty 2 it only applied to a part of it")
        let mut c = quiet(2, 1);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let e = c.staff[0].id;
        // tour 1 cut in two, its afternoon joined with tour 2's duty into one driver's duty
        let tours = vec![cut_tour("1", 6 * 60, 6, &[3]), tour("2", 12 * 60 + 30, 2)];
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        let b = c.fleet[1].id;
        set_bus(&mut c, wd, "Linie5", "2", Some(b));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e)));
        let pieces = vec![super::super::studio::piece_ref(&tours, 0, 1).unwrap(), super::super::studio::piece_ref(&tours, 1, 0).unwrap()];
        c.planning.joined.push(super::super::studio::JoinedDuty { weekday: wd, pieces });
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(joined_pieces(&c, wd, &p.tours, 0, 1), vec![(1, 0)]);
        assert!(p.tours[0].duties[1].open() && p.tours[1].duties[0].open());
        // the agency asked for its first piece: both its pieces are the agency's, one driver
        set_fill(&mut c, &duty_key("Linie5", "1", 1), Some(Fill::Agency));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!((p.tours[0].duties[1].who, p.tours[1].duties[0].who), (Some(Who::Agency), Some(Who::Agency)));
        assert_eq!((agency_used(&p), agency_sent(&c, &p)), (2, 1));
        assert!(agency_room(&c, &p, 1, 0));
        let hour = AGENCY_HOUR as f64 * c.price_index;
        let hours = |t: usize, k: usize| ((p.tours[t].duties[k].to - p.tours[t].duties[k].from) as f64 / 60.0 * hour).round() as Cents;
        assert_eq!(agency_cost(&c, &p, 1, 0), hours(0, 1) + hours(1, 0));
        // nothing open, and the close runs every trip
        assert_eq!(p.counts().2, 0);
        let r = close_day(&mut c.clone(), tours.clone(), &[]);
        assert_eq!((r.trips, r.dropped), (8, 0), "{:?}", r.drops);
        // taken back on the other piece: both open again
        let mut back = c.clone();
        assert_eq!(super::super::unassign::driver_off(&mut back, &p, 1, 0), Ok(super::super::unassign::Off::Choice));
        let q = day_plan(&back, &back.date, tours, &[], &[], false);
        assert!(q.tours[0].duties[1].open() && q.tours[1].duties[0].open());
    }

    #[test]
    fn the_days_choice_follows_its_duty_when_it_is_cut() {
        // (the dispatcher's choice is keyed by the duty's place in its block, as the roster is:
        // a cut saved in RosterFlow left the agency driver on a slice of the duty)
        let mut c = quiet(1, 1);
        c.planning.auto = false;
        let wd = weekday_of(&c.date);
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        let e = c.staff[0].id;
        let before = vec![cut_tour("1", 6 * 60, 8, &[2])];
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e)));
        set_fill(&mut c, &duty_key("Linie5", "1", 1), Some(Fill::Agency));
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Rental));
        let p = day_plan(&c, &c.date, before.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[1].who, Some(Who::Agency));
        // the agency's duty cut in two more: both halves stay the agency's, the first duty its driver's
        let after = vec![cut_tour("1", 6 * 60, 8, &[2, 5])];
        follow(&mut c, wd, &before, &after);
        let p = day_plan(&c, &c.date, after.clone(), &[], &[], false);
        let who: Vec<Option<Who>> = p.tours[0].duties.iter().map(|d| d.who).collect();
        assert_eq!(who, vec![Some(Who::Staff(e)), Some(Who::Agency), Some(Who::Agency)]);
        assert!(c.planning.fills.iter().any(|f| f.key == tour_key("Linie5", "1")), "the tour's own choice stays");
        // the cut taken out again: one duty, the agency's
        follow(&mut c, wd, &after, &before);
        let p = day_plan(&c, &c.date, before, &[], &[], false);
        assert_eq!(p.tours[0].duties[1].who, Some(Who::Agency));
        assert_eq!(c.planning.fills.iter().filter(|f| f.fill == Fill::Agency).count(), 1);
        assert_eq!(parse_duty_key(&late_key("Linie5", "1", 2)), Some(("Linie5".into(), "1".into(), 2, true)));
        assert_eq!(parse_duty_key(&tour_key("Linie5", "1")), None);
    }

    #[test]
    fn a_late_driver_moved_to_a_later_duty_is_not_late() {
        // (testers: "when a driver is late and you swap them onto a different duty this should
        // stop them being late")
        let mut c = company(2, 3);
        c.planning.auto = false;
        c.game_mode = GameMode::Realistic;
        for e in c.staff.iter_mut() {
            (e.satisfaction, e.reliability) = (10.0, 0.0);
        }
        let base = c.id.clone();
        let late = (0..4000).find_map(|n| {
            c.id = format!("{base}{n}");
            let d = disruptions(&c, &c.date);
            let late = d.iter().find_map(|x| if let Disruption::Late { employee, minutes } = *x { Some((employee, minutes)) } else { None });
            late.filter(|_| !d.iter().any(|x| matches!(x, Disruption::Breakdown { .. })))
        });
        let Some((e, minutes)) = late else { panic!("no late morning found") };
        let wd = weekday_of(&c.date);
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        let b = c.fleet[1].id;
        set_bus(&mut c, wd, "Linie5", "2", Some(b));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e)));
        let tours = vec![tour("1", 6 * 60, 3), tour("2", 14 * 60, 3)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[0].duties[0].late.map(|l| l.until), Some(6 * 60 + minutes));
        // the arrival kept (the morning, or the roster's first change)
        assert_eq!(pin_arrivals(&mut c, &p), 1);
        assert_eq!(pin_arrivals(&mut c, &p), 0);
        assert_eq!(arrival(&c, &c.date, e), Some(6 * 60 + minutes));
        // moved to the afternoon: there long before it begins, not late
        set_driver(&mut c, wd, "Linie5", "1", 0, None);
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(e)));
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[1].duties[0].who, Some(Who::Staff(e)));
        assert_eq!(p.tours[1].duties[0].late, None);
        assert!(p.to_plan().tours[1].duties[0].dropped_before.is_none());
        // and the close agrees: none of the afternoon's trips dropped for him
        let r = close_day(&mut c.clone(), tours.clone(), &[]);
        assert!(r.drops.iter().all(|d| d.tour != "2"), "{:?}", r.drops);
        // a duty that begins before he arrives: late until then, not from its own start
        let early = vec![tour("1", 6 * 60 + 10, 3), tour("2", 14 * 60, 3)];
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e)));
        set_driver(&mut c, wd, "Linie5", "2", 0, None);
        let p = day_plan(&c, &c.date, early, &[], &[], false);
        assert_eq!(p.tours[0].duties[0].late.map(|l| l.until), Some(6 * 60 + minutes));
    }

    #[test]
    fn a_spare_line_drives_a_late_drivers_first_trips_within_its_own_hours() {
        // (RosterFlow's spare lines as the player set their times: the dispatcher sends one)
        let mut c = company(2, 3);
        c.planning.auto = false;
        c.game_mode = GameMode::Realistic;
        for e in c.staff.iter_mut() {
            (e.satisfaction, e.reliability) = (10.0, 0.0);
        }
        let base = c.id.clone();
        let late = (0..4000).find_map(|n| {
            c.id = format!("{base}{n}");
            let d = disruptions(&c, &c.date);
            let late = d.iter().find_map(|x| if let Disruption::Late { employee, minutes } = *x { Some((employee, minutes)) } else { None });
            late.filter(|_| !d.iter().any(|x| matches!(x, Disruption::Breakdown { .. })))
        });
        let Some((e, minutes)) = late else { panic!("no late morning found") };
        let wd = weekday_of(&c.date);
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(e)));
        let tours = vec![tour("1", 6 * 60, 3)];
        let slot = super::super::plandays::slot_of(&c, &c.date);
        let id = super::super::rosterflow::add_spare(&mut c, slot, "Spare", 5 * 60 + 30, 14 * 60);
        let late_of = |c: &Company| day_plan(c, &c.date, tours.clone(), &[], &[], false).tours[0].duties[0].late.unwrap();
        // standing by from 05:30: the central sends it, nothing is dropped
        let l = late_of(&c);
        assert_eq!((l.until, l.cover, l.from_), (6 * 60 + minutes, Some(SPARE_COVER + id), Source::Central));
        assert_eq!(spare_of_cover(l.cover.unwrap()), Some(id));
        assert_eq!(spare_of_cover(e), None);
        assert!(day_plan(&c, &c.date, tours.clone(), &[], &[], false).to_plan().tours[0].duties[0].dropped_before.is_none());
        // its hours moved to begin after the duty: not this spare line (a colleague, or nobody)
        super::super::rosterflow::spare_mut(&mut c, slot, id).unwrap().custom[0].set_times(6 * 60 + 15, 14 * 60);
        assert_ne!(late_of(&c).cover, Some(SPARE_COVER + id));
        // chosen by the player all the same: not within its hours, so nobody
        set_fill(&mut c, &late_key("Linie5", "1", 0), Some(Fill::Spare { id }));
        assert_eq!(late_of(&c).cover, None);
        // within them again: the one chosen
        super::super::rosterflow::spare_mut(&mut c, slot, id).unwrap().custom[0].set_times(5 * 60, 9 * 60);
        assert_eq!((late_of(&c).cover, late_of(&c).from_), (Some(SPARE_COVER + id), Source::Dispatcher));
    }

    #[test]
    fn a_working_day_too_long_counts_that_days_duties_only() {
        // (testers: "Working day would be 14 h 40" on a weekend duty)
        let mut c = quiet(2, 1);
        c.planning.auto = false;
        let e = c.staff[0].id;
        let monday = weekday_of(&c.date);
        set_driver(&mut c, monday, "Linie5", "1", 0, Some(Who::Staff(e)));
        let tours = vec![tour("1", 6 * 60, 8), tour("2", 15 * 60, 3)];
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let len = |p: &DayPlan, t: usize| p.tours[t].duties[0].to - p.tours[t].duties[0].from;
        let cs = candidates(&c, &p, 1, 0);
        assert_eq!(cs[0].fit, Err(Cannot::TooLong { minutes: len(&p, 0) + len(&p, 1) }));
        // the next day the roster gives him nothing: free for it
        let tuesday = dates::add(&c.date, 1);
        let p = day_plan(&c, &tuesday, tours, &[], &[], false);
        assert!(candidates(&c, &p, 1, 0)[0].fit.is_ok());
    }
}

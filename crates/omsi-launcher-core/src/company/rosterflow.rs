//! RosterFlow, the bus company's scheduling studio (Luc: the planning should look and work
//! "exactly" like Optibus's scheduling module, under a name of its own): what it adds to the
//! blocks and duties of `studio` - its preferences (the rules the duties are checked against,
//! hard or soft with a penalty, and what Optimise works to), the locks, links and duty notes
//! of each weekday, the duties' shape (sign-on and sign-off, travel to a relief point, relief
//! timing, breaks, duty types, paid time), the issues it validates, its figures, the draft the
//! player edits until he saves it, and Optimise itself.
//!
//! What the game runs is what is saved: the draft (`Flow::draft`) is the studio's alone until
//! "Save" (`save_draft`) - the roster, the day's close and the game keep the saved blocks and
//! duties meanwhile. The relief points are the depot, the lines' termini and the stops on the
//! way the player adds (`ReliefPoint`, Luc 2026-10-14: halfway along a trip too), each with what
//! it may be used for and its own travel times (`travel_between`).

use super::economy;
use super::model::{BusKind, BusSize, Cents, Company, Drive};
use super::network::{self, PlannedTrip, TourOfDay};
use super::staff::{self, Block, MIN_PAID};
use super::studio::{self as st, Duty, JoinedDuty, LineLayout, PieceRef, TripRef};
use crate::LineInfo;
use serde::{Deserialize, Serialize};
use std::ops::Range;

// --- the lines' colours ------------------------------------------------------------------------

/// The lines' colours where a line has none of its own: saturated, as the chips in Optibus.
pub const PALETTE: [[u8; 3]; 10] = [[79, 129, 190], [196, 96, 214], [38, 166, 154], [245, 128, 40], [130, 110, 232], [250, 196, 60], [232, 82, 140], [32, 170, 210], [96, 186, 76], [222, 78, 64]];

/// A line's colour (`#rrggbb`): its own, else one of the palette by its place among the
/// company's lines. The studio's chips, the plates and the fleet map all take it from here.
pub fn line_colour(c: &Company, line: &str) -> [u8; 3] {
    let k = c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(line.trim())).unwrap_or(0);
    c.lines.get(k).and_then(|l| parse_hex(&l.colour)).unwrap_or(PALETTE[k % PALETTE.len()])
}

/// `#rrggbb` as its bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Give line `line` its own colour (`None`: back to the palette's).
pub fn set_line_colour(c: &mut Company, line: &str, colour: Option<[u8; 3]>) -> bool {
    let Some(l) = c.lines.iter_mut().find(|x| x.name.eq_ignore_ascii_case(line.trim())) else { return false };
    l.colour = colour.map(hex).unwrap_or_default();
    true
}

// --- the preferences -----------------------------------------------------------------------------

/// A rule of the preferences: on or not, its value, and whether it is a hard rule (never
/// broken by Optimise, an error when broken by hand) or a soft one that costs `penalty`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Limit {
    pub on: bool,
    pub value: i32,
    pub hard: bool,
    pub penalty: Cents,
}

impl Default for Limit {
    fn default() -> Self {
        Limit { on: true, value: 0, hard: true, penalty: 0 }
    }
}

impl Limit {
    const fn hard(value: i32) -> Limit {
        Limit { on: true, value, hard: true, penalty: 0 }
    }

    const fn soft(value: i32, penalty: Cents) -> Limit {
        Limit { on: true, value, hard: false, penalty }
    }

    const fn off(value: i32) -> Limit {
        Limit { on: false, value, hard: false, penalty: 0 }
    }
}

/// When the driver changes within the bus's layover at a relief point.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReliefTiming {
    /// The old driver gets off on arrival, the new one on at departure: the bus stands alone.
    #[default]
    ArrivalDeparture,
    OnArrival,
    OnDeparture,
    MidLayover,
    /// A set time before the departure (`Prefs::relief_before`).
    BeforeDeparture,
}

impl ReliefTiming {
    pub const ALL: [ReliefTiming; 5] = [ReliefTiming::ArrivalDeparture, ReliefTiming::OnArrival, ReliefTiming::OnDeparture, ReliefTiming::MidLayover, ReliefTiming::BeforeDeparture];
    pub const LABELS: [&'static str; 5] = ["On arrival and on departure (unattended)", "On arrival", "On departure", "Mid-layover", "Before departure"];

    pub fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|x| *x == self).unwrap_or(0)]
    }
}

/// How a driver gets to a relief point at another place.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Travel {
    #[default]
    Walk,
    Public,
    Car,
}

impl Travel {
    pub const ALL: [Travel; 3] = [Travel::Walk, Travel::Public, Travel::Car];
    pub const LABELS: [&'static str; 3] = ["Walk", "Public transport", "Staff car"];

    /// The letter on its box in the Gantt (Optibus's W, P, R).
    pub fn letter(self) -> &'static str {
        match self {
            Travel::Walk => "W",
            Travel::Public => "P",
            Travel::Car => "R",
        }
    }

    pub fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|x| *x == self).unwrap_or(0)]
    }

    /// Its minutes as the preferences have them.
    pub fn minutes(self, p: &Prefs) -> i32 {
        match self {
            Travel::Walk => p.walk,
            Travel::Public => p.public,
            Travel::Car => p.car,
        }
    }
}

/// No, yes, or either.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tri {
    No,
    Yes,
    #[default]
    Either,
}

impl Tri {
    pub const ALL: [Tri; 3] = [Tri::No, Tri::Yes, Tri::Either];
    pub const LABELS: [&'static str; 3] = ["No", "Yes", "Either"];

    fn allows(self, v: bool) -> bool {
        match self {
            Tri::No => !v,
            Tri::Yes => v,
            Tri::Either => true,
        }
    }
}

/// A kind of duty the company works (Optibus's duty types): the first whose windows a duty
/// fits is its type; a duty no allowed type fits is "Other", an issue.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DutyType {
    pub name: String,
    /// The sign-on between these minutes of the day.
    pub start_from: i32,
    pub start_to: i32,
    /// The sign-off at the latest (minutes; past 1440 the next day).
    pub end_by: i32,
    /// The spread (sign-on to sign-off) at most.
    pub max_length: i32,
    /// The spread at least (0: no least; a tester, 2026-10-15: "the shortest a duty can be").
    pub min_length: i32,
    pub split: Tri,
    pub allowed: bool,
    pub penalty: Cents,
}

impl Default for DutyType {
    fn default() -> Self {
        DutyType { name: String::new(), start_from: 0, start_to: 1439, end_by: 1800, max_length: 600, min_length: 0, split: Tri::Either, allowed: true, penalty: 0 }
    }
}

impl DutyType {
    fn new(name: &str, start: (i32, i32), end_by: i32, max_length: i32, split: Tri) -> DutyType {
        DutyType { name: name.into(), start_from: start.0, start_to: start.1, end_by, max_length, min_length: 0, split, allowed: true, penalty: 0 }
    }

    /// Whether a duty from `start` to `end` (split or not) is of this type.
    pub fn fits(&self, start: i32, end: i32, split: bool) -> bool {
        start >= self.start_from && start <= self.start_to && end <= self.end_by && end - start <= self.max_length && end - start >= self.min_length && self.split.allows(split)
    }
}

/// The name a duty no type fits goes by.
pub const OTHER: &str = "Other";
/// The default duty types' names (translated when shown).
pub const TYPE_NAMES: [&str; 5] = ["Early shift", "Middle shift", "Late shift", "Split shift", "Night shift"];

fn default_types() -> Vec<DutyType> {
    vec![
        DutyType::new(TYPE_NAMES[0], (180, 479), 960, 600, Tri::No),
        DutyType::new(TYPE_NAMES[1], (480, 719), 1200, 600, Tri::No),
        DutyType::new(TYPE_NAMES[2], (720, 1080), 1620, 600, Tri::No),
        DutyType::new(TYPE_NAMES[3], (180, 780), 1380, 780, Tri::Yes),
        DutyType::new(TYPE_NAMES[4], (1081, 1439), 1800, 600, Tri::Either),
    ]
}

/// RosterFlow's preferences (its Preferences window: VEHICLES, DRIVERS, DEPOTS, MISC): what
/// the duties and blocks are checked against and what Optimise works to. Minutes and cents.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Prefs {
    // VEHICLES
    /// The least layover between two trips of a bus at one place.
    pub layover: Limit,
    /// The bus's check before it leaves the depot and after it is back.
    pub pre_trip: i32,
    pub post_trip: i32,
    // DEPOTS
    /// Buses out at once at most (the depot's capacity).
    pub depot_capacity: Limit,
    // DRIVERS
    /// Driver signing: at the duty's start and end, and around a split.
    pub sign_on: i32,
    pub sign_off: i32,
    pub sign_on_split: i32,
    pub sign_off_split: i32,
    /// Where drivers sign on and off (a relief point's key; None: where the bus is), unless a
    /// duty chose its own (`signing`).
    pub sign_on_at: Option<String>,
    pub sign_off_at: Option<String>,
    pub relief: ReliefTiming,
    pub relief_before: i32,
    /// Duty travel: how drivers get to a relief point elsewhere, and how long it takes.
    pub travel: Travel,
    pub walk: i32,
    pub public: i32,
    pub car: i32,
    /// Relief points: the depot's and the termini's settings, and the stops on the way the
    /// player added (`ReliefPoint`); travel between two of them (`TravelPair`).
    pub points: Vec<ReliefPoint>,
    pub pairs: Vec<TravelPair>,
    /// Duty breaks: a duty working more than `meal_after` needs a break of `meal.value`, begun
    /// before it has worked `meal_after`.
    pub meal: Limit,
    pub meal_after: i32,
    /// Continuous break: at most `continuous.value` without a break of `break_min`.
    pub continuous: Limit,
    pub break_min: i32,
    /// A gap in a duty this long or longer is a break (labelled in the Gantt, counted by the
    /// meal and continuous rules); a shorter one is standing time (testers: "if over a figure eg
    /// 15 mins then it shows something as a break").
    pub break_from: i32,
    /// A layover inside a piece counts as a break (Optibus's "attendance counts as break").
    pub layover_breaks: bool,
    /// Time limitation: the spread (sign-on to sign-off) at most.
    pub max_spread: Limit,
    /// Time limitation: the spread at least (off by default, an old save's too). A shorter duty
    /// is an issue, and is paid up to it (`paid_floor`: four hours, `MIN_PAID`, always).
    pub min_duty: Limit,
    /// Pieces of work at least this long.
    pub min_piece: Limit,
    /// Changes of bus in a duty at most.
    pub changeovers: Limit,
    /// A break between two pieces this long or longer is a split (unpaid).
    pub split_min: i32,
    pub duty_types: Vec<DutyType>,
    // MISC: costs (None: what the company pays, as the studio reckons it)
    /// A driver's hour (cents).
    pub wage: Option<Cents>,
    /// What each duty costs besides its hours (None: three hours' wage).
    pub driver_fixed: Option<Cents>,
    /// What each bus out costs a day, and each kilometre.
    pub vehicle_fixed: Option<Cents>,
    pub per_km: Option<Cents>,
    // the optimiser's settings
    pub optimise_vehicles: bool,
    pub optimise_duties: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            layover: Limit::soft(3, 20_00),
            pre_trip: 5,
            post_trip: 5,
            depot_capacity: Limit::off(20),
            sign_on: 10,
            sign_off: 5,
            sign_on_split: 5,
            sign_off_split: 5,
            sign_on_at: None,
            sign_off_at: None,
            relief: ReliefTiming::ArrivalDeparture,
            relief_before: 5,
            travel: Travel::Walk,
            walk: 5,
            public: 15,
            car: 10,
            points: Vec::new(),
            pairs: Vec::new(),
            meal: Limit::hard(30),
            meal_after: 360,
            continuous: Limit::hard(270),
            break_min: 15,
            break_from: 15,
            layover_breaks: true,
            max_spread: Limit::soft(780, 50_00),
            min_duty: Limit::off(MIN_PAID),
            min_piece: Limit::soft(60, 10_00),
            changeovers: Limit::soft(2, 15_00),
            split_min: st::SPLIT_BREAK,
            duty_types: default_types(),
            wage: None,
            driver_fixed: None,
            vehicle_fixed: None,
            per_km: None,
            optimise_vehicles: true,
            optimise_duties: true,
        }
    }
}

// --- relief points and travel ------------------------------------------------------------------
//
// The tester (2026-10-14): "not every start point will have the same definitions". The depot and
// every line's termini are relief points by themselves, and the player adds stops on a line's way
// (a stop several lines share is one point). Each says what it may be used for - a duty begins
// or ends there, drivers change on the bus there, a break is taken there - and how long a driver
// takes to get there; between two points the player may set the minutes too. What is not set
// falls back to the point's own, then to the defaults (`Prefs::travel`, `walk` ...).

/// The depot as a relief point: all of its names are this one point.
pub const DEPOT_POINT: &str = "Depot";

/// A relief point's settings (Preferences › Relief Points and Duty Travel).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ReliefPoint {
    /// Its stop as the timetable names it (`DEPOT_POINT` for the depot).
    pub stop: String,
    /// A stop on the way the player added (else the depot or a terminus, there by itself).
    pub added: bool,
    /// The line it was picked on (an added stop).
    pub line: String,
    /// A duty may begin or end there (sign-on, sign-off).
    pub sign: bool,
    /// Drivers may change on the bus there.
    pub relief: bool,
    /// A meal break or a split's break may be taken there.
    pub breaks: bool,
    /// The way a driver gets there, and how long each way takes (None: the defaults).
    pub travel: Option<Travel>,
    pub walk: Option<i32>,
    pub public: Option<i32>,
    pub car: Option<i32>,
}

impl Default for ReliefPoint {
    fn default() -> Self {
        ReliefPoint { stop: String::new(), added: false, line: String::new(), sign: true, relief: true, breaks: true, travel: None, walk: None, public: None, car: None }
    }
}

impl ReliefPoint {
    /// Its own minutes of a way (None: not set).
    pub fn minutes(&self, way: Travel) -> Option<i32> {
        match way {
            Travel::Walk => self.walk,
            Travel::Public => self.public,
            Travel::Car => self.car,
        }
    }

    /// What it may be used for.
    pub fn uses(&self) -> Uses {
        Uses { sign: self.sign, relief: self.relief, breaks: self.breaks }
    }
}

/// The minutes between two relief points by each way (Duty Travel's table), from `from` to
/// `to` (None: not set). The way back is the same unless it is set too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct TravelPair {
    pub from: String,
    pub to: String,
    pub walk: Option<i32>,
    pub public: Option<i32>,
    pub car: Option<i32>,
}

impl TravelPair {
    pub fn minutes(&self, way: Travel) -> Option<i32> {
        match way {
            Travel::Walk => self.walk,
            Travel::Public => self.public,
            Travel::Car => self.car,
        }
    }
}

/// What a relief point may be used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uses {
    pub sign: bool,
    pub relief: bool,
    pub breaks: bool,
}

/// Whether `place` is the company's depot: "Depot", the depot file's name, or where a block's
/// depot run begins or ends (`tours` the day's).
pub fn is_depot(c: &Company, tours: &[TourOfDay], place: &str) -> bool {
    let place = place.trim();
    if place.is_empty() {
        return false;
    }
    same(place, DEPOT_POINT) || (!c.depot.trim().is_empty() && same(place, c.depot.trim_end_matches(".hof"))) || tours.iter().any(|t| t.trips.first().is_some_and(|x| x.empty && same(&x.from, place)) || t.trips.last().is_some_and(|x| x.empty && same(x.end(), place)))
}

/// Whether `place` is a terminus of the day's lines: where a trip with passengers begins or
/// ends (a trip cut on the way by its own ends).
pub fn is_terminus(tours: &[TourOfDay], place: &str) -> bool {
    let place = place.trim();
    !place.is_empty() && tours.iter().flat_map(|t| t.trips.iter()).filter(|x| x.counts()).any(|x| (!x.is_second() && same(&x.from, place)) || (x.part.as_ref().is_none_or(|p| p.second) && same(x.end(), place)))
}

/// The relief point a place is: the depot's names are one (`DEPOT_POINT`), a stop its own name.
pub fn point_key(c: &Company, tours: &[TourOfDay], place: &str) -> String {
    if is_depot(c, tours, place) {
        DEPOT_POINT.to_string()
    } else {
        place.trim().to_string()
    }
}

/// The settings kept of a relief point (by its key).
pub fn point<'a>(p: &'a Prefs, key: &str) -> Option<&'a ReliefPoint> {
    p.points.iter().find(|x| same(&x.stop, key))
}

/// What `place` may be used for as a relief point (`tours` the day's), None where it is none:
/// not the depot, no terminus, no stop the player added. The depot and the termini allow
/// everything until the player says otherwise.
pub fn uses_at(c: &Company, tours: &[TourOfDay], place: &str) -> Option<Uses> {
    let key = point_key(c, tours, place);
    if key.is_empty() {
        return None;
    }
    let kept = point(&c.planning.flow.prefs, &key);
    let auto = key == DEPOT_POINT || is_terminus(tours, &key);
    match kept {
        Some(x) if auto || x.added => Some(x.uses()),
        None if auto => Some(Uses { sign: true, relief: true, breaks: true }),
        _ => None,
    }
}

/// How a driver gets from relief point `from` to relief point `to` (their keys, `point_key`)
/// and how long it takes, the most specific first: the way set by hand (`way`), else the
/// quickest way the pair has times for, else the way of the point he goes to, of the one he
/// comes from, else the default way; its minutes the pair's (the way back's where the pair has
/// none set), else the points' own (where he goes, where he comes from), else the defaults.
pub fn travel_of(p: &Prefs, from: &str, to: &str, way: Option<Travel>) -> (Travel, i32) {
    let pair = |a: &str, b: &str| p.pairs.iter().find(|x| same(&x.from, a) && same(&x.to, b));
    let set = |x: &&TravelPair| Travel::ALL.iter().any(|w| x.minutes(*w).is_some());
    let pair = pair(from, to).filter(set).or_else(|| pair(to, from).filter(set));
    let (dest, orig) = (point(p, to), point(p, from));
    let way = way
        .or_else(|| pair.and_then(|x| Travel::ALL.iter().filter_map(|w| x.minutes(*w).map(|m| (*w, m))).min_by_key(|(_, m)| *m).map(|(w, _)| w)))
        .or_else(|| dest.and_then(|x| x.travel))
        .or_else(|| orig.and_then(|x| x.travel))
        .unwrap_or(p.travel);
    let m = pair.and_then(|x| x.minutes(way)).or_else(|| dest.and_then(|x| x.minutes(way))).or_else(|| orig.and_then(|x| x.minutes(way))).unwrap_or_else(|| way.minutes(p));
    (way, m.max(0))
}

/// How a driver gets from `from` to `to` (places of the day `tours`) and how long it takes
/// (`travel_of`); no time between a place and itself.
pub fn travel_between(c: &Company, tours: &[TourOfDay], from: &str, to: &str, way: Option<Travel>) -> (Travel, i32) {
    let p = &c.planning.flow.prefs;
    let (a, b) = (point_key(c, tours, from), point_key(c, tours, to));
    if same(&a, &b) {
        return (way.unwrap_or(p.travel), 0);
    }
    travel_of(p, &a, &b, way)
}

/// What kind of relief point a row of the Preferences is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Depot,
    Terminus,
    OnTheWay,
}

/// A relief point as the Preferences list it: its stop, kind, the lines (their numbers) it is
/// of, and its settings (the defaults where none are kept).
#[derive(Clone, Debug, PartialEq)]
pub struct PointRow {
    pub stop: String,
    pub kind: PointKind,
    pub lines: Vec<String>,
    pub point: ReliefPoint,
}

/// The company's relief points (`lines` the timetable read): the depot, every line's termini,
/// and the stops on the way the player added - a stop several lines share once.
pub fn relief_points(c: &Company, lines: &[LineInfo]) -> Vec<PointRow> {
    let p = &c.planning.flow.prefs;
    let tours = network::timetable_tours(c, lines);
    let kept = |stop: &str| point(p, stop).cloned().unwrap_or_else(|| ReliefPoint { stop: stop.to_string(), ..Default::default() });
    let mut out: Vec<PointRow> = vec![PointRow { stop: DEPOT_POINT.into(), kind: PointKind::Depot, lines: Vec::new(), point: kept(DEPOT_POINT) }];
    let add = |stop: &str, kind: PointKind, number: &str, out: &mut Vec<PointRow>| {
        let stop = stop.trim();
        if stop.is_empty() || is_depot(c, &tours, stop) {
            return;
        }
        match out.iter_mut().find(|r| same(&r.stop, stop)) {
            Some(r) => {
                if !number.is_empty() && !r.lines.iter().any(|l| l == number) {
                    r.lines.push(number.to_string());
                }
                if kind == PointKind::Terminus {
                    r.kind = kind;
                }
            }
            None => out.push(PointRow { stop: stop.to_string(), kind, lines: if number.is_empty() { Vec::new() } else { vec![number.to_string()] }, point: kept(stop) }),
        }
    };
    for cl in &c.lines {
        for x in tours.iter().filter(|t| same(&t.line, &cl.name)).flat_map(|t| t.trips.iter()).filter(|x| x.counts()) {
            add(&x.from, PointKind::Terminus, cl.number.trim(), &mut out);
            add(x.end(), PointKind::Terminus, cl.number.trim(), &mut out);
        }
    }
    for x in p.points.iter().filter(|x| x.added) {
        let number = c.lines.iter().find(|l| same(&l.name, &x.line)).map(|l| l.number.trim().to_string()).unwrap_or_default();
        add(&x.stop, PointKind::OnTheWay, &number, &mut out);
    }
    // (the lines whose way an added stop is on)
    for r in out.iter_mut().filter(|r| r.kind == PointKind::OnTheWay) {
        for cl in &c.lines {
            let on = lines.iter().filter(|l| same(&l.name, &cl.name)).flat_map(|l| l.tours.iter()).flat_map(|t| t.trips.iter()).any(|y| y.stops.iter().any(|s| same(&s.name, &r.stop)));
            if on && !r.lines.iter().any(|l| l == cl.number.trim()) {
                r.lines.push(cl.number.trim().to_string());
            }
        }
    }
    out
}

/// The stops of each company line (`lines` the timetable read) a stop on the way can be picked
/// of: its name, its number, and its stops in the order its longest trip calls at them, then
/// the others' further ones.
pub fn line_stops(c: &Company, lines: &[LineInfo]) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for cl in &c.lines {
        let mut trips: Vec<&crate::TripInfo> = lines.iter().filter(|l| same(&l.name, &cl.name)).flat_map(|l| l.tours.iter()).flat_map(|t| t.trips.iter()).filter(|y| y.stops.len() >= 3).collect();
        trips.sort_by_key(|y| std::cmp::Reverse(y.stops.len()));
        let mut stops: Vec<String> = Vec::new();
        for y in trips {
            for s in &y.stops {
                if !s.name.trim().is_empty() && !stops.iter().any(|x| same(x, &s.name)) {
                    stops.push(s.name.trim().to_string());
                }
            }
        }
        out.push((cl.name.clone(), cl.number.trim().to_string(), stops));
    }
    out
}

/// The relief points' settings kept as the Preferences changed them: what says nothing (a
/// terminus or the depot with every use allowed and no travel of its own) is left out, and so
/// are pairs without a time.
pub fn tidy_points(p: &mut Prefs) {
    p.points.retain(|x| !x.stop.trim().is_empty() && (x.added || !x.sign || !x.relief || !x.breaks || x.travel.is_some() || x.walk.is_some() || x.public.is_some() || x.car.is_some()));
    p.pairs.retain(|x| !same(&x.from, &x.to) && (x.walk.is_some() || x.public.is_some() || x.car.is_some()));
}

// --- what is kept ------------------------------------------------------------------------------

/// A bus's block kept as it is (Optimise leaves it, no edit changes it).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BlockLock {
    pub weekday: u8,
    pub line: String,
    pub tour: String,
}

/// Trips that stay together on one bus (striped in the Gantt): moved together, kept
/// together by Optimise.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Link {
    pub weekday: u8,
    pub line: String,
    pub trips: Vec<TripRef>,
}

/// Work in a duty that is no trip (Optibus's custom work): a spare turn, office hours.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Custom {
    pub kind: String,
    pub from: i32,
    pub to: i32,
    #[serde(default)]
    pub place: String,
}

/// The kinds of custom work offered (translated when shown).
pub const CUSTOM_KINDS: [&str; 4] = ["Spare", "Office", "Union", "Training"];

/// What the player set of a duty, by its first piece: its type, its lock, how its driver
/// travels between pieces, its custom work, its signing and its paid time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DutyNote {
    pub weekday: u8,
    pub key: PieceRef,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub locked: bool,
    /// The way between piece `n` and the next (by `n`).
    #[serde(default)]
    pub travel: Vec<(u32, Travel)>,
    #[serde(default)]
    pub custom: Vec<Custom>,
    #[serde(default)]
    pub paid: Option<i32>,
    #[serde(default)]
    pub sign_on: Option<i32>,
    #[serde(default)]
    pub sign_off: Option<i32>,
    /// Where the duty signs on and off (a relief point's key, empty where the bus is; None: as
    /// the company has it), and takes the break after its piece `n` (by `n`): `signing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_on_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_off_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks_at: Vec<(u32, String)>,
}

impl DutyNote {
    fn is_blank(&self) -> bool {
        self.kind.is_none() && !self.locked && self.travel.is_empty() && self.custom.is_empty() && self.paid.is_none() && self.sign_on.is_none() && self.sign_off.is_none() && !self.has_points()
    }
}

/// A duty of custom work only (a spare driver's day), begun from scratch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Spare {
    pub weekday: u8,
    pub id: u32,
    pub custom: Vec<Custom>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub paid: Option<i32>,
    /// Its name, the player's ("Early spare"), shown beside its number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Where its driver signs on and stands by: a relief point's key (None: where the company
    /// signs on, else the depot) - `spare_lines`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_on_at: Option<String>,
}

impl Custom {
    /// Its times set (minutes of the day, past midnight on): five minutes long at least.
    pub fn set_times(&mut self, from: i32, to: i32) {
        self.from = from.clamp(0, LAST_MINUTE - 5);
        self.to = to.clamp(self.from + 5, LAST_MINUTE);
    }
}

/// The last minute custom work may end (06:00 the next morning, as the steppers go).
pub const LAST_MINUTE: i32 = 1800;

/// Spare duty `id` of `weekday` (`Sheet::spares`), to change.
pub fn spare_mut(c: &mut Company, weekday: u8, id: u32) -> Option<&mut Spare> {
    c.planning.flow.sheet.spares.iter_mut().find(|s| s.weekday == weekday && s.id == id)
}

/// A spare line of a day the company runs (`spare_lines`): spare duty `id`'s spare work `ci`,
/// its driver standing by at `at` (a relief point's key) from `from` to `to`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpareLine {
    pub id: u32,
    pub ci: usize,
    /// "9001" and its name.
    pub label: String,
    pub from: i32,
    pub to: i32,
    pub at: String,
}

impl SpareLine {
    /// Whether its driver stands by all the time from `from` to `to`: a spare line covers only
    /// within its own hours.
    pub fn covers(&self, from: i32, to: i32) -> bool {
        self.from <= from && to <= self.to
    }

    /// Whether it stands by at the depot (where the spare buses are).
    pub fn at_depot(&self) -> bool {
        same(&self.at, DEPOT_POINT)
    }
}

/// Spare duty `id`'s number and name ("9001 Early"): its driver as service control names him.
pub fn spare_label(c: &Company, id: u32) -> String {
    match c.planning.flow.sheet.spares.iter().find(|s| s.id == id).map(|s| s.name.trim()).filter(|n| !n.is_empty()) {
        Some(name) => format!("{} {name}", 9000 + id),
        None => (9000 + id).to_string(),
    }
}

/// The spare lines of `date` as saved (the drafts the studio has not saved are not run): the
/// spare work ("Spare" custom work) of the spare duties of the day's plan (`plandays::slot_of`),
/// each with its times as the player set them, by its start.
pub fn spare_lines(c: &Company, date: &str) -> Vec<SpareLine> {
    let slot = super::plandays::slot_of(c, date);
    let company_at = c.planning.flow.prefs.sign_on_at.clone().filter(|x| !x.trim().is_empty());
    let mut out: Vec<SpareLine> = Vec::new();
    for sp in c.planning.flow.sheet.spares.iter().filter(|s| s.weekday == slot) {
        let at = sp.sign_on_at.clone().filter(|x| !x.trim().is_empty()).or_else(|| company_at.clone()).unwrap_or_else(|| DEPOT_POINT.to_string());
        let number = 9000 + sp.id;
        let label = if sp.name.trim().is_empty() { number.to_string() } else { format!("{number} {}", sp.name.trim()) };
        for (ci, cw) in sp.custom.iter().enumerate().filter(|(_, cw)| same(&cw.kind, CUSTOM_KINDS[0]) && cw.to > cw.from) {
            out.push(SpareLine { id: sp.id, ci, label: label.clone(), from: cw.from, to: cw.to, at: at.clone() });
        }
    }
    out.sort_by_key(|x| (x.from, x.id));
    out
}

/// The studio's own of each weekday besides the blocks and joined duties.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Sheet {
    pub locks: Vec<BlockLock>,
    pub links: Vec<Link>,
    pub notes: Vec<DutyNote>,
    pub spares: Vec<Spare>,
    /// Issues dismissed: (weekday, the issue's key).
    pub dismissed: Vec<(u8, String)>,
    /// The order the Drivers tab shows a weekday's duties in, where it is not by their start
    /// (`RowOrder`, `keep_rows`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<RowOrder>,
}

impl Sheet {
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.links.is_empty() && self.notes.is_empty() && self.spares.is_empty() && self.dismissed.is_empty() && self.rows.is_empty()
    }
}

/// The duties of a weekday in the order the Drivers tab shows them, by their keys (`duty_key`),
/// top down - their numbers (1001 on) go by it. Luc (Beta 65): a duty split keeps its row and
/// its number, the half split off gets a row of its own at the bottom; an edit never shuffles
/// the other rows (in the order of their start, a new duty pushed every later one a row down).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RowOrder {
    pub weekday: u8,
    pub keys: Vec<PieceRef>,
}

/// What the player changed and has not saved yet: the blocks, the joined duties and the sheet
/// as the studio shows them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Draft {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub sheet: Sheet,
}

/// RosterFlow's part of the company's planning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Flow {
    pub prefs: Prefs,
    pub sheet: Sheet,
    pub draft: Option<Draft>,
    /// Which plan each day runs: the plans of single dates, and the days copied from another
    /// (`plandays`). Kept outside the draft: a copy is made as it is saved.
    #[serde(skip_serializing_if = "super::plandays::Days::is_empty")]
    pub days: super::plandays::Days,
    /// How the Gantt colours the trips (Layers › Colour by, testers 2026-10-15): "trip" by the
    /// timetable's trip, "block" by the bus, "duty" by the duty; empty by the line.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub colour_by: String,
}

impl Flow {
    /// Nothing but the defaults (left out of the saved company).
    pub fn is_empty(&self) -> bool {
        self.prefs == Prefs::default() && self.sheet.is_empty() && self.draft.is_none() && self.days.is_empty() && self.colour_by.is_empty()
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// "7:05" (minutes of the day; past midnight on, as the service day runs).
pub fn hm(m: i32) -> String {
    let m = m.max(0);
    format!("{}:{:02}", m / 60, m % 60)
}

/// "07:05" on the clock.
pub fn clock(m: i32) -> String {
    let m = m.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

// --- the draft -------------------------------------------------------------------------------

/// The company as the studio shows it: the draft's blocks, duties and sheet in place of the
/// saved ones (the company itself where there is no draft).
pub fn view(c: &Company) -> Company {
    let mut v = c.clone();
    if let Some(d) = c.planning.flow.draft.clone() {
        v.planning.layouts = d.layouts;
        v.planning.joined = d.joined;
        v.planning.flow.sheet = d.sheet;
    }
    v.planning.flow.draft = None;
    v
}

/// Whether there are changes not saved.
pub fn has_draft(c: &Company) -> bool {
    c.planning.flow.draft.as_ref().is_some_and(|d| d.layouts != c.planning.layouts || d.joined != c.planning.joined || d.sheet != c.planning.flow.sheet)
}

/// What the view (`view`, edited) holds as the company's draft.
pub fn keep_draft(c: &mut Company, view: &Company) {
    let d = Draft { layouts: view.planning.layouts.clone(), joined: view.planning.joined.clone(), sheet: view.planning.flow.sheet.clone() };
    let same_as_saved = d.layouts == c.planning.layouts && d.joined == c.planning.joined && d.sheet == c.planning.flow.sheet;
    c.planning.flow.draft = if same_as_saved { None } else { Some(d) };
}

/// The draft as it is now (for "Undo": the state to go back to).
pub fn draft_of(c: &Company) -> Draft {
    c.planning.flow.draft.clone().unwrap_or_else(|| Draft { layouts: c.planning.layouts.clone(), joined: c.planning.joined.clone(), sheet: c.planning.flow.sheet.clone() })
}

/// Put a draft back (Undo, Redo, the history).
pub fn set_draft(c: &mut Company, d: Draft) {
    let mut v = view(c);
    v.planning.layouts = d.layouts;
    v.planning.joined = d.joined;
    v.planning.flow.sheet = d.sheet;
    keep_draft(c, &v);
}

/// "Save": the draft becomes what the company runs. Returns whether there was one.
pub fn save_draft(c: &mut Company) -> bool {
    let Some(d) = c.planning.flow.draft.take() else { return false };
    c.planning.layouts = d.layouts;
    c.planning.joined = d.joined;
    c.planning.flow.sheet = d.sheet;
    true
}

/// "Save" with the roster kept to its buses and duties (`plan::follow`, #96) on the days of
/// `week` (each date with its timetable read; a weekday not among them keeps its roster as it
/// is). None: there was no draft.
pub fn save_draft_following(c: &mut Company, week: &[(String, Vec<LineInfo>)]) -> Option<super::plan::Followed> {
    c.planning.flow.draft.as_ref()?;
    let before: Vec<Vec<TourOfDay>> = week.iter().map(|(date, lines)| network::tours_of_day(c, lines, date)).collect();
    save_draft(c);
    let mut out = super::plan::Followed::default();
    for ((date, lines), was) in week.iter().zip(before) {
        let now = network::tours_of_day(c, lines, date);
        let f = super::plan::follow(c, super::plan::weekday_of(date), &was, &now);
        out.kept += f.kept;
        out.open += f.open;
    }
    Some(out)
}

/// Whether the draft has changes of `weekday`'s blocks or duties not saved (the day's close
/// runs the saved ones and says so).
pub fn draft_differs(c: &Company, weekday: u8) -> bool {
    let Some(d) = c.planning.flow.draft.as_ref() else { return false };
    let lay = |v: &[LineLayout]| v.iter().filter(|l| l.weekday == weekday).cloned().collect::<Vec<_>>();
    let joined = |v: &[JoinedDuty]| v.iter().filter(|j| j.weekday == weekday).cloned().collect::<Vec<_>>();
    lay(&d.layouts) != lay(&c.planning.layouts) || joined(&d.joined) != joined(&c.planning.joined)
}

/// "Discard": the draft forgotten, back to what was saved - but what the draft locks stays as the
/// draft has it (Luc, Beta 65: "a locked duty can still be changed by Discard"): the locks, the
/// locked buses' blocks and those with a piece of a locked duty, the locked duties' joins and
/// notes, the locked spare duties (`keep_locked`). Returns whether there was a draft; the draft
/// left is what was kept that the saved planning has otherwise.
pub fn discard_draft(c: &mut Company) -> bool {
    let Some(d) = c.planning.flow.draft.take() else { return false };
    let mut v = view(c);
    keep_locked(&mut v, &d);
    keep_draft(c, &v);
    true
}

/// What draft `d` locks put into `v` (the saved planning) as `d` has it (`discard_draft`).
fn keep_locked(v: &mut Company, d: &Draft) {
    // the draft's locked duties: their notes and their pieces
    let locked: Vec<(&DutyNote, Vec<PieceRef>)> = d
        .sheet
        .notes
        .iter()
        .filter(|n| n.locked)
        .map(|n| {
            let group = d.joined.iter().find(|j| j.weekday == n.weekday && j.pieces.iter().any(|p| p.is(&n.key)));
            (n, group.map(|j| j.pieces.clone()).unwrap_or_else(|| vec![n.key.clone()]))
        })
        .collect();
    // the buses kept as the draft has them: those locked, those with a piece of a locked duty
    let mut buses: Vec<(u8, String, String)> = d.sheet.locks.iter().map(|l| (l.weekday, l.line.clone(), l.tour.clone())).collect();
    for (n, pieces) in &locked {
        buses.extend(pieces.iter().map(|p| (n.weekday, p.line.clone(), p.tour.clone())));
    }
    let mut lines: Vec<(u8, String)> = Vec::new();
    for (w, line, _) in &buses {
        if !lines.iter().any(|(w2, l2)| w2 == w && same(l2, line)) {
            lines.push((*w, line.clone()));
        }
    }
    let p = &mut v.planning;
    for (w, line) in lines {
        let names: Vec<&str> = buses.iter().filter(|b| b.0 == w && same(&b.1, &line)).map(|b| b.2.as_str()).collect();
        let draft = d.layouts.iter().find(|l| l.weekday == w && same(&l.line, &line));
        let saved = p.layouts.iter().position(|l| l.weekday == w && same(&l.line, &line));
        match (draft, saved) {
            (None, None) => {}
            // (one of them the automatic blocks, which are not known here: the draft's line)
            (Some(dl), None) => p.layouts.push(dl.clone()),
            (None, Some(k)) => {
                p.layouts.remove(k);
            }
            (Some(dl), Some(k)) => {
                let mixed = mix_line(&p.layouts[k], dl, &names);
                p.layouts[k] = mixed;
            }
        }
    }
    p.joined.retain(|j| !locked.iter().any(|(n, ps)| n.weekday == j.weekday && j.pieces.iter().any(|x| ps.iter().any(|y| y.is(x)))));
    p.joined.extend(locked.iter().filter(|(_, ps)| ps.len() > 1).map(|(n, ps)| JoinedDuty { weekday: n.weekday, pieces: ps.clone() }));
    let sh = &mut p.flow.sheet;
    for l in &d.sheet.locks {
        if !sh.locks.iter().any(|x| x.weekday == l.weekday && same(&x.line, &l.line) && same(&x.tour, &l.tour)) {
            sh.locks.push(l.clone());
        }
    }
    for (n, _) in &locked {
        sh.notes.retain(|x| !(x.weekday == n.weekday && x.key.is(&n.key)));
        sh.notes.push((*n).clone());
    }
    for sp in d.sheet.spares.iter().filter(|x| x.locked) {
        sh.spares.retain(|x| !(x.weekday == sp.weekday && x.id == sp.id));
        sh.spares.push(sp.clone());
    }
}

/// Line layout `saved` with its buses `names` as `draft` has them: their trips taken off the
/// other buses and the stack, and a trip one of them ran as saved but not in the draft put where
/// the draft has it - on a bus the saved layout has too, on the stack, or on a bus of the
/// draft's own.
fn mix_line(saved: &LineLayout, draft: &LineLayout, names: &[&str]) -> LineLayout {
    let mut out = saved.clone();
    for n in names {
        let Some(db) = draft.blocks.iter().find(|b| same(&b.tour, n)) else { continue };
        let was: Vec<TripRef> = out.blocks.iter().find(|b| same(&b.tour, n)).map(|b| b.trips.clone()).unwrap_or_default();
        for b in out.blocks.iter_mut().filter(|b| !same(&b.tour, n)) {
            b.trips.retain(|r| !db.trips.contains(r));
            if let Some(cuts) = b.cuts.as_mut() {
                cuts.retain(|r| !db.trips.contains(r));
            }
            b.reliefs.retain(|x| !db.trips.contains(&x.trip));
        }
        out.stack.retain(|r| !db.trips.contains(r));
        match out.blocks.iter_mut().find(|b| same(&b.tour, n)) {
            Some(b) => *b = db.clone(),
            None => out.blocks.push(db.clone()),
        }
        for r in was.iter().filter(|r| !db.trips.contains(r)) {
            if out.blocks.iter().any(|b| b.trips.contains(r)) || out.stack.contains(r) {
                continue;
            }
            if draft.stack.contains(r) {
                out.stack.push(r.clone());
                continue;
            }
            let Some(home) = draft.blocks.iter().find(|b| b.trips.contains(r)).map(|b| b.tour.clone()) else { continue };
            match out.blocks.iter_mut().find(|b| same(&b.tour, &home)) {
                Some(b) => {
                    b.trips.push(r.clone());
                    b.trips.sort_by_key(|t| t.dep);
                }
                None => out.blocks.push(st::BlockLayout { tour: home, trips: vec![r.clone()], cuts: None, reliefs: Vec::new() }),
            }
        }
    }
    out
}

// --- locks, links and notes ---------------------------------------------------------------------

/// Whether the bus's block (`line`, `tour`) is locked on `weekday`.
pub fn block_locked(c: &Company, weekday: u8, line: &str, tour: &str) -> bool {
    c.planning.flow.sheet.locks.iter().any(|l| l.weekday == weekday && same(&l.line, line) && same(&l.tour, tour))
}

/// Lock the block or unlock it.
pub fn set_block_lock(c: &mut Company, weekday: u8, line: &str, tour: &str, on: bool) {
    let locks = &mut c.planning.flow.sheet.locks;
    locks.retain(|l| !(l.weekday == weekday && same(&l.line, line) && same(&l.tour, tour)));
    if on {
        locks.push(BlockLock { weekday, line: line.trim().to_string(), tour: tour.trim().to_string() });
    }
}

/// The key a duty is known by: its first piece.
pub fn duty_key(tours: &[TourOfDay], d: &Duty) -> Option<PieceRef> {
    let p = d.pieces.first()?;
    st::piece_ref(tours, p.tour, p.duty)
}

fn note<'a>(c: &'a Company, weekday: u8, key: &PieceRef) -> Option<&'a DutyNote> {
    c.planning.flow.sheet.notes.iter().find(|n| n.weekday == weekday && n.key.is(key))
}

/// The note of a duty to change (made when it has none); blank notes are dropped again by
/// `tidy_notes`.
pub fn note_mut<'a>(c: &'a mut Company, weekday: u8, key: &PieceRef) -> &'a mut DutyNote {
    let notes = &mut c.planning.flow.sheet.notes;
    match notes.iter().position(|n| n.weekday == weekday && n.key.is(key)) {
        Some(i) => &mut notes[i],
        None => {
            notes.push(DutyNote { weekday, key: key.clone(), ..Default::default() });
            notes.last_mut().unwrap()
        }
    }
}

/// Drop the notes that say nothing.
pub fn tidy_notes(c: &mut Company) {
    c.planning.flow.sheet.notes.retain(|n| !n.is_blank());
}

/// Whether the duty with key `key` is locked.
pub fn duty_locked(c: &Company, weekday: u8, key: &PieceRef) -> bool {
    note(c, weekday, key).is_some_and(|n| n.locked)
}

/// Whether the duty piece (block `i`, duty `k`) is in is locked.
pub fn piece_locked(c: &Company, weekday: u8, tours: &[TourOfDay], i: usize, k: usize) -> bool {
    if !c.planning.flow.sheet.notes.iter().any(|n| n.weekday == weekday && n.locked) {
        return false;
    }
    let duties = st::duties(&c.planning.joined, weekday, tours);
    let Some(d) = st::duty_of(&duties, i, k) else { return false };
    duty_key(tours, &duties[d]).is_some_and(|key| duty_locked(c, weekday, &key))
}

/// Lock a duty or unlock it.
pub fn set_duty_lock(c: &mut Company, weekday: u8, key: &PieceRef, on: bool) {
    note_mut(c, weekday, key).locked = on;
    tidy_notes(c);
}

/// The trips linked with `trip` of line `line` on `weekday` (itself among them), none if it is
/// in no link.
pub fn linked(c: &Company, weekday: u8, line: &str, trip: &TripRef) -> Vec<TripRef> {
    c.planning.flow.sheet.links.iter().find(|l| l.weekday == weekday && same(&l.line, line) && l.trips.contains(trip)).map(|l| l.trips.clone()).unwrap_or_default()
}

/// Why trips may not be linked.
pub const LINK_ONE_BUS: &str = "Only trips of one bus can be linked.";
pub const LINK_TWO: &str = "Pick two trips or more to link.";

/// Link the trips `picked` of block `i` (consecutive on that bus): they stay together.
/// Links they were in are merged into the new one.
pub fn link(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, picked: &[usize]) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(st::NOT_FOUND)?;
    // (a trip cut on the way by the whole trip)
    let mut refs: Vec<TripRef> = picked.iter().filter_map(|&k| t.trips.get(k)).filter(|x| x.counts() && !x.origin.trim().is_empty()).map(st::whole_ref).collect();
    if refs.len() < 2 {
        return Err(LINK_TWO);
    }
    let links = &mut c.planning.flow.sheet.links;
    let mut merged: Vec<TripRef> = Vec::new();
    links.retain(|l| {
        let hit = l.weekday == weekday && same(&l.line, &t.line) && l.trips.iter().any(|r| refs.contains(r));
        if hit {
            merged.extend(l.trips.iter().cloned());
        }
        !hit
    });
    refs.extend(merged);
    refs.sort_by_key(|r| r.dep);
    refs.dedup();
    if !refs.iter().all(|r| t.trips.iter().any(|x| r.is(x))) {
        return Err(LINK_ONE_BUS);
    }
    links.push(Link { weekday, line: t.line.clone(), trips: refs });
    Ok(())
}

/// Take the link of trip `k` of block `i` apart.
pub fn unlink(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, k: usize) -> bool {
    let Some(x) = tours.get(i).and_then(|t| t.trips.get(k)) else { return false };
    let r = st::whole_ref(x);
    let line = tours[i].line.clone();
    let before = c.planning.flow.sheet.links.len();
    c.planning.flow.sheet.links.retain(|l| !(l.weekday == weekday && same(&l.line, &line) && l.trips.contains(&r)));
    c.planning.flow.sheet.links.len() != before
}

/// A new duty of custom work only on `weekday` (a spare turn from `from` to `to`). Returns
/// its number.
pub fn add_spare(c: &mut Company, weekday: u8, kind: &str, from: i32, to: i32) -> u32 {
    let id = c.planning.flow.sheet.spares.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    c.planning.flow.sheet.spares.push(Spare { weekday, id, custom: vec![Custom { kind: kind.into(), from, to: to.max(from + 5), place: String::new() }], ..Default::default() });
    id
}

pub fn remove_spare(c: &mut Company, weekday: u8, id: u32) -> bool {
    let before = c.planning.flow.sheet.spares.len();
    c.planning.flow.sheet.spares.retain(|s| !(s.weekday == weekday && s.id == id));
    before != c.planning.flow.sheet.spares.len()
}

/// Dismiss an issue (its key) or bring it back.
pub fn set_dismissed(c: &mut Company, weekday: u8, key: &str, on: bool) {
    let d = &mut c.planning.flow.sheet.dismissed;
    d.retain(|(w, k)| !(*w == weekday && k == key));
    if on {
        d.push((weekday, key.to_string()));
    }
}

pub fn dismissed(c: &Company, weekday: u8, key: &str) -> bool {
    c.planning.flow.sheet.dismissed.iter().any(|(w, k)| *w == weekday && k == key)
}

/// A line renamed: its locks, links and duties' notes follow it - and the draft not saved yet,
/// its blocks and joined duties too (`studio::rename_line` renames the saved ones): else Save
/// put them back under the old name, and the line's blocks, locks and duty notes were lost.
pub fn rename_line(c: &mut Company, old: &str, new: &str) {
    rename_in_sheet(&mut c.planning.flow.sheet, old, new);
    if let Some(d) = c.planning.flow.draft.as_mut() {
        rename_in_sheet(&mut d.sheet, old, new);
        for l in d.layouts.iter_mut().filter(|l| same(&l.line, old)) {
            l.line = new.to_string();
        }
        for p in d.joined.iter_mut().flat_map(|j| j.pieces.iter_mut()).filter(|p| same(&p.line, old)) {
            p.line = new.to_string();
        }
    }
}

fn rename_in_sheet(s: &mut Sheet, old: &str, new: &str) {
    for l in s.locks.iter_mut().filter(|l| same(&l.line, old)) {
        l.line = new.to_string();
    }
    for l in s.links.iter_mut().filter(|l| same(&l.line, old)) {
        l.line = new.to_string();
    }
    for n in s.notes.iter_mut().filter(|n| same(&n.key.line, old)) {
        n.key.line = new.to_string();
    }
    for k in s.rows.iter_mut().flat_map(|r| r.keys.iter_mut()).filter(|k| same(&k.line, old)) {
        k.line = new.to_string();
    }
}

// --- the rows of the Drivers tab ---------------------------------------------------------------------

/// A duty as the rows know it (`RowOrder`): its key, and the trips with passengers it works.
pub type RowDuty = (PieceRef, Vec<TripRef>);

/// `shaped` (in the order of their start, the spare duties last) in the order of the weekday's
/// rows (`RowOrder`): the duties kept there in that order, one not among them (the timetable
/// changed, a day copied) where its start puts it among them.
fn in_rows(c: &Company, wd: u8, shaped: &mut Vec<Shaped>) {
    let Some(rows) = c.planning.flow.sheet.rows.iter().find(|r| r.weekday == wd) else { return };
    let at = |s: &Shaped| s.key.as_ref().and_then(|k| rows.keys.iter().position(|x| x.is(k)));
    let (mut known, mut others, mut spares) = (Vec::new(), Vec::new(), Vec::new());
    for s in shaped.drain(..) {
        match (s.spare, at(&s)) {
            (Some(_), _) => spares.push(s),
            (None, Some(k)) => known.push((k, s)),
            (None, None) => others.push(s),
        }
    }
    known.sort_by_key(|x| x.0);
    let mut out: Vec<Shaped> = known.into_iter().map(|x| x.1).collect();
    for s in others {
        let k = out.iter().position(|o| o.start > s.start).unwrap_or(out.len());
        out.insert(k, s);
    }
    out.extend(spares);
    *shaped = out;
}

/// The trips with passengers duty `d` works (`tours` the day's).
fn duty_trips(tours: &[TourOfDay], d: &Duty) -> Vec<TripRef> {
    let mut out = Vec::new();
    for p in &d.pieces {
        let Some(t) = tours.get(p.tour) else { continue };
        let r = network::duties_of(t).get(p.duty).cloned().unwrap_or(0..0);
        out.extend(t.trips.get(r).unwrap_or(&[]).iter().filter(|x| x.counts()).map(TripRef::of));
    }
    out
}

/// The duties of day `b` as its rows show them, top down (the spare duties left out).
pub fn row_duties(b: &Board) -> Vec<RowDuty> {
    b.shaped.iter().filter_map(|s| Some((s.key.clone()?, duty_trips(&b.tours, b.duties.get(s.duty?)?)))).collect()
}

/// The rows of `weekday` after an edit of `c` (`tours` its day now) made on the rows `before`
/// (`row_duties`): each duty keeps the row of the duty it was - the same key, else the one it
/// shares the most trips with - and a duty the edit made gets a new row at the bottom. `moved`:
/// the trips the edit took off their duty (the half a split cuts off, the trips moved to a duty
/// of their own), which make no duty the one it was. Kept only where they are not the order of
/// the duties' start.
pub fn keep_rows(c: &mut Company, weekday: u8, tours: &[TourOfDay], before: &[RowDuty], moved: &[TripRef]) {
    let duties = st::duties(&c.planning.joined, weekday, tours);
    let now: Vec<RowDuty> = duties.iter().filter_map(|d| Some((duty_key(tours, d)?, duty_trips(tours, d)))).collect();
    // (each duty now with each one before, by how sure it is that duty: its key, else the trips
    // they share)
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for (a, (ka, ta)) in now.iter().enumerate() {
        for (b, (kb, tb)) in before.iter().enumerate() {
            let score = if ka.is(kb) && !moved.contains(&ka.first) { usize::MAX } else { ta.iter().filter(|x| !moved.contains(x) && tb.contains(x)).count() };
            if score > 0 {
                pairs.push((score, a, b));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.cmp(&x.0).then(x.2.cmp(&y.2)).then(x.1.cmp(&y.1)));
    let mut row: Vec<Option<usize>> = vec![None; now.len()];
    let mut taken = vec![false; before.len()];
    for (_, a, b) in pairs {
        if row[a].is_none() && !taken[b] {
            row[a] = Some(b);
            taken[b] = true;
        }
    }
    // (the rows kept in their order, then the new duties by their start: `now` is in that order)
    let mut order: Vec<usize> = (0..now.len()).collect();
    order.sort_by_key(|&a| (row[a].is_none(), row[a].unwrap_or(0), a));
    let keys: Vec<PieceRef> = order.iter().map(|&a| now[a].0.clone()).collect();
    let by_start: Vec<PieceRef> = now.into_iter().map(|x| x.0).collect();
    set_rows(c, weekday, keys, &by_start);
}

/// The rows of `weekday` after Optimise made its duties anew (`c` with them, `tours` its day now)
/// on the rows `before` (`row_duties`): a locked duty keeps its row - and its number with it -
/// and the other duties fill the rows left in the order of their start.
pub fn pin_rows(c: &mut Company, weekday: u8, tours: &[TourOfDay], before: &[RowDuty]) {
    let duties = st::duties(&c.planning.joined, weekday, tours);
    let now: Vec<PieceRef> = duties.iter().filter_map(|d| duty_key(tours, d)).collect();
    let mut slots: Vec<Option<PieceRef>> = vec![None; now.len()];
    let mut rest: Vec<PieceRef> = Vec::new();
    for k in &now {
        match before.iter().position(|(b, _)| b.is(k)).filter(|_| duty_locked(c, weekday, k)) {
            Some(i) if i < slots.len() && slots[i].is_none() => slots[i] = Some(k.clone()),
            _ => rest.push(k.clone()),
        }
    }
    let mut rest = rest.into_iter();
    let mut keys: Vec<PieceRef> = slots.into_iter().filter_map(|s| s.or_else(|| rest.next())).collect();
    keys.extend(rest);
    set_rows(c, weekday, keys, &now);
}

/// The rows of `weekday` kept as `keys`, unless they are the order of the duties' start
/// (`by_start`).
fn set_rows(c: &mut Company, weekday: u8, keys: Vec<PieceRef>, by_start: &[PieceRef]) {
    let rows = &mut c.planning.flow.sheet.rows;
    rows.retain(|r| r.weekday != weekday);
    if keys.len() != by_start.len() || keys.iter().zip(by_start).any(|(k, n)| !k.is(n)) {
        rows.push(RowOrder { weekday, keys });
    }
}

// --- a lesson's snapshot ---------------------------------------------------------------------------

/// What one of RosterFlow's lessons ("Try it") may change in the company, kept before the
/// lesson begins so that "Undo the lesson" puts it all back: the saved blocks and joined
/// duties, RosterFlow's own (its preferences, its sheet and the draft), and the lines' colours.
/// The roster of drivers and buses, the money and the clock are not a lesson's and stay as
/// they are.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub flow: Flow,
    /// Each line's own colour (its name, `#rrggbb` or empty for the palette's).
    pub colours: Vec<(String, String)>,
}

impl Snapshot {
    /// The company as a lesson finds it.
    pub fn take(c: &Company) -> Snapshot {
        Snapshot { layouts: c.planning.layouts.clone(), joined: c.planning.joined.clone(), flow: c.planning.flow.clone(), colours: c.lines.iter().map(|l| (l.name.clone(), l.colour.clone())).collect() }
    }

    /// Put the company back as the lesson found it (a line added since keeps its colour).
    /// Returns whether anything changed.
    pub fn restore(&self, c: &mut Company) -> bool {
        let before = Snapshot::take(c);
        c.planning.layouts = self.layouts.clone();
        c.planning.joined = self.joined.clone();
        c.planning.flow = self.flow.clone();
        for l in c.lines.iter_mut() {
            if let Some((_, colour)) = self.colours.iter().find(|(n, _)| same(n, &l.name)) {
                l.colour = colour.clone();
            }
        }
        Snapshot::take(c) != before
    }

    /// The saved planning (not the draft) is as it was: nothing new was saved meanwhile.
    pub fn same_saved(&self, c: &Company) -> bool {
        self.layouts == c.planning.layouts && self.joined == c.planning.joined && self.flow.sheet == c.planning.flow.sheet
    }
}

// --- the stack -------------------------------------------------------------------------------

/// The trips on the stack of `weekday` (taken off every bus): their line, and the trip as the
/// timetable has it.
pub fn stack(c: &Company, lines: &[LineInfo], weekday: u8) -> Vec<(String, PlannedTrip)> {
    let refs: Vec<(&str, &TripRef)> = c.planning.layouts.iter().filter(|l| l.weekday == weekday).flat_map(|l| l.stack.iter().map(move |r| (l.line.as_str(), r))).collect();
    if refs.is_empty() {
        return Vec::new();
    }
    let raw = network::timetable_tours(c, lines);
    let mut out: Vec<(String, PlannedTrip)> = refs.iter().filter_map(|(line, r)| raw.iter().filter(|t| same(&t.line, line)).flat_map(|t| t.trips.iter()).find(|x| r.is(x)).map(|x| (line.to_string(), x.clone()))).collect();
    out.sort_by_key(|(_, x)| x.dep);
    out
}

// --- the duties' shape -------------------------------------------------------------------------

/// What a duty does besides its trips, as the Gantt draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvKind {
    SignOn,
    SignOff,
    Travel(Travel),
    /// Custom work: its index in the duty's note.
    Custom(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ev {
    pub kind: EvKind,
    pub from: i32,
    pub to: i32,
}

/// A piece of a duty as worked: its block and duty there, and the minutes the driver has it
/// (relief timing and the pre- and post-trip checks in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub tour: usize,
    pub duty: usize,
    pub from: i32,
    pub to: i32,
}

/// A driver's duty in full: its pieces as worked, its events, its times and type.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shaped {
    /// Its duty in `Board::duties` (None: a spare duty of custom work only).
    pub duty: Option<usize>,
    pub spare: Option<u32>,
    pub key: Option<PieceRef>,
    pub spans: Vec<Span>,
    pub events: Vec<Ev>,
    pub custom: Vec<Custom>,
    pub start: i32,
    pub end: i32,
    pub work: i32,
    pub paid: i32,
    pub driving: i32,
    pub platform: i32,
    pub splits: Vec<(i32, i32)>,
    pub changeovers: usize,
    /// Its type (an index into the preferences' types; None: "Other").
    pub kind: Option<usize>,
    /// Its type set by hand.
    pub kind_set: bool,
    pub locked: bool,
    /// Its issues: indices into `Board::issues`.
    pub issues: Vec<usize>,
    /// Its breaks and driving periods as the rules count them (`break_plan`).
    pub plan: BreakPlan,
    /// Where it stands at relief points that allow no break (`ReliefPoint::breaks`): no break
    /// is taken in these minutes (`breaks_of`).
    pub no_rest: Vec<(i32, i32)>,
    /// Where it signs on and off and takes its breaks elsewhere than where the bus is (`signing`).
    pub points: signing::Points,
}

impl Shaped {
    pub fn spread(&self) -> i32 {
        self.end - self.start
    }

    pub fn split(&self) -> bool {
        !self.splits.is_empty()
    }

    /// Its type's name (English, translated when shown).
    pub fn kind_name<'a>(&self, p: &'a Prefs) -> &'a str {
        self.kind.and_then(|k| p.duty_types.get(k)).map(|t| t.name.as_str()).unwrap_or(OTHER)
    }

    /// Its number as the studio shows it: 1001 on, in order, a spare from 9001.
    pub fn label(&self, row: usize) -> String {
        match self.spare {
            Some(id) => format!("{}", 9000 + id),
            None => format!("{}", 1001 + row),
        }
    }
}

/// Where the relief within a layover from `arr` to `dep` is: when the driver who brought the
/// bus leaves it (`incoming` false) or the next one takes it (true).
pub fn relief_at(p: &Prefs, arr: i32, dep: i32, incoming: bool) -> i32 {
    match p.relief {
        ReliefTiming::ArrivalDeparture => {
            if incoming {
                dep
            } else {
                arr
            }
        }
        ReliefTiming::OnArrival => arr,
        ReliefTiming::OnDeparture => dep,
        ReliefTiming::MidLayover => (arr + dep) / 2,
        ReliefTiming::BeforeDeparture => (dep - p.relief_before).max(arr),
    }
}

/// The minutes a piece (block `t`, its trips `r`) is worked.
pub fn span_of(p: &Prefs, t: &TourOfDay, r: &Range<usize>) -> (i32, i32) {
    let trips = t.trips.get(r.clone()).unwrap_or(&[]);
    let (Some(first), Some(last)) = (trips.first(), trips.last()) else { return (0, 0) };
    let from = if r.start == 0 {
        first.dep - p.pre_trip
    } else {
        let prev = &t.trips[r.start - 1];
        relief_at(p, prev.arr, first.dep, true).min(first.dep)
    };
    let end = trips.iter().map(|x| x.arr).max().unwrap_or(last.arr);
    let to = if r.end >= t.trips.len() {
        end + p.post_trip
    } else {
        relief_at(p, last.arr, t.trips[r.end].dep, false).max(end)
    };
    (from, to)
}

fn piece_places(t: &TourOfDay, r: &Range<usize>) -> (String, String) {
    let a = t.trips.get(r.start).map(|x| x.from.trim().to_string()).unwrap_or_default();
    let b = r.end.checked_sub(1).and_then(|e| t.trips.get(e)).map(|x| x.end().trim().to_string()).unwrap_or_default();
    (a, b)
}

/// Which issue it is (the Validation panel's three lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Vehicle,
    Duty,
}

/// Something the day breaks or comes near: its words (English, `%{..}` filled from `args`),
/// whether it is an error (a hard rule) or a warning, what it costs, and its row.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub scope: Scope,
    /// The block (`Board::tours`) or duty (`Board::shaped`) it is about.
    pub row: Option<usize>,
    pub text: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub hard: bool,
    pub penalty: Cents,
    /// Its key, the same from one look to the next (to dismiss it).
    pub key: String,
}

// the issues' words
pub const I_TYPE: &str = "Duty type unsatisfied: the duty is %{type}";
pub const I_MEAL: &str = "No meal break: a duty working over %{after} needs a break of %{min}";
pub const I_MEAL_LATE: &str = "Meal break too late: %{time} of work before it, at most %{after}";
pub const I_CONTINUOUS: &str = "%{time} without a break: at most %{max} before a break of %{min}";
pub const I_SPREAD: &str = "Spread of %{time}, over the limit of %{max}";
pub const I_SHORT_PIECE: &str = "A piece of %{time}, shorter than %{min}";
pub const I_CHANGEOVERS: &str = "%{n} changes of bus, more than %{max}";
pub const I_TRAVEL: &str = "No time to get to the relief point at %{stop}: %{need} needed, %{have} there";
pub const I_RULE: &str = "Working time: %{rule}";
pub const I_RULE_TIME: &str = "Working time: %{rule}, %{time}";
pub const I_LAYOVER: &str = "Layover of %{time} at %{stop}, shorter than %{min}";
pub const I_DEADHEAD: &str = "Deadhead from %{from} to %{to}: no trip of the line runs there, its time is estimated";
pub const I_UNASSIGNED: &str = "%{n} trips are not assigned to a vehicle";
pub const I_BUSES: &str = "%{n} buses at the peak, the company has %{m}";
pub const I_DEPOT: &str = "%{n} buses out at once, more than the depot holds (%{max})";
pub const I_DRIVERS: &str = "%{n} duties, %{m} drivers on the staff";
/// The day's roster (`plan::day_plan`): a tour or duty of the day open or filled for the day
/// only - an ill driver, a bus in the workshop - and the tours nobody is planned for.
pub const I_DAY: &str = "Line %{n} tour %{tour}: %{rule}";
pub const I_DAY_FILLED: &str = "Line %{n} tour %{tour}: %{rule} - covered for the day";
pub const I_DAY_UNPLANNED: &str = "%{n} tours in service have no bus or driver in the roster: they do not run";
/// A line of the company's confirmed with its route alone (`ownline::no_timetable`): it runs
/// nothing until its timetable is made in the Timeplan (the issue's "line" its timetable name).
pub const I_NO_TIMETABLE: &str = "Line %{n} has no timetable yet: make it in the Timeplan";

pub const ISSUE_TEXTS: [&str; 20] = [I_TYPE, I_MEAL, I_MEAL_LATE, I_CONTINUOUS, I_SPREAD, I_SHORT_PIECE, I_CHANGEOVERS, I_TRAVEL, I_RULE, I_RULE_TIME, I_LAYOVER, I_DEADHEAD, I_UNASSIGNED, I_BUSES, I_DEPOT, I_DRIVERS, I_DAY, I_DAY_FILLED, I_DAY_UNPLANNED, I_NO_TIMETABLE];

/// The issues' short titles (the Validation panel's groups), in `ISSUE_TEXTS`' order.
pub const ISSUE_TITLES: [&str; 20] = ["Duty type unsatisfied", "No meal break", "Meal break too late", "Driving without a break", "Spread over the limit", "Pieces too short", "Too many changes of bus", "No time to get to the relief point", "Working time", "Working time", "Layovers too short", "Deadheads estimated", "Trips not assigned", "More buses than the company has", "More buses than the depot holds", "More duties than drivers", "The day's roster", "The day's roster", "The day's roster", "Lines without a timetable"];

/// The relief points' issues (`ReliefPoint`): a duty begins or ends, takes a split's break, or
/// drivers change on a bus where the point does not allow it - and their titles.
pub const I_SIGN_ON: &str = "Signs on at %{stop}, where no duty may begin";
pub const I_SIGN_OFF: &str = "Signs off at %{stop}, where no duty may end";
pub const I_BREAK_PLACE: &str = "A split's break at %{stop}, where no break may be taken";
pub const I_RELIEF_PLACE: &str = "Drivers change at %{stop}, where no relief is allowed";
pub const PLACE_TEXTS: [&str; 4] = [I_SIGN_ON, I_SIGN_OFF, I_BREAK_PLACE, I_RELIEF_PLACE];
pub const PLACE_TITLES: [&str; 4] = ["Duties begin or end at a wrong place", "Duties begin or end at a wrong place", "Breaks at a wrong place", "Reliefs at a wrong place"];

/// A duty shorter than the least length of a duty (Time Limitations, `Prefs::min_duty`), and its
/// title.
pub const I_SHORT_DUTY: &str = "A duty of %{time}, shorter than %{min}";
pub const LENGTH_TEXTS: [&str; 1] = [I_SHORT_DUTY];
pub const LENGTH_TITLES: [&str; 1] = ["Duties too short"];

/// An issue's short title (its group in the Validation panel).
pub fn issue_title(text: &str) -> &'static str {
    ISSUE_TEXTS
        .iter()
        .position(|t| *t == text)
        .map(|k| ISSUE_TITLES[k])
        .or_else(|| PLACE_TEXTS.iter().position(|t| *t == text).map(|k| PLACE_TITLES[k]))
        .or_else(|| LENGTH_TEXTS.iter().position(|t| *t == text).map(|k| LENGTH_TITLES[k]))
        .or_else(|| signing::title_of(text))
        .unwrap_or("Issues")
}

/// An issue's words filled in (`tr`: how a text is translated).
pub fn issue_text(i: &Issue, tr: &dyn Fn(&str) -> String) -> String {
    let mut s = tr(i.text);
    for (k, v) in &i.args {
        let v = if *k == "rule" || *k == "type" { tr(v) } else { v.clone() };
        s = s.replace(&format!("%{{{k}}}"), &v);
    }
    s
}

/// The day's figures (the KPI dashboard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Figures {
    pub duties: usize,
    pub blocks: usize,
    /// Buses out at once at the busiest.
    pub pvr: usize,
    pub service_km: f64,
    pub dead_km: f64,
    pub driving: i32,
    pub paid: i32,
    pub spread: i32,
    pub work: i32,
    pub platform: i32,
    pub splits: usize,
    pub split_time: i32,
    pub changeovers: usize,
    pub split_changeovers: usize,
    pub crew_cost: Cents,
    pub vehicle_cost: Cents,
    pub penalties: Cents,
    /// Duties by type (the types' names, "Other" last).
    pub types: Vec<(String, usize)>,
    /// Duties by paid time: under 5 h, 5-7, 7-9, 9-12, 12 and over.
    pub paid_bands: [usize; 5],
    /// Trips with passengers on a bus, and all of them (the status bar's 819/838).
    pub assigned: usize,
    pub trips: usize,
}

pub const PAID_BANDS: [&str; 5] = ["0-5", "5-7", "7-9", "9-12", "12+"];

impl Figures {
    /// Service kilometres against all a bus runs.
    pub fn vehicle_eff(&self) -> f64 {
        let all = self.service_km + self.dead_km;
        if all <= 0.0 {
            0.0
        } else {
            self.service_km / all
        }
    }

    /// Driving time against paid time.
    pub fn crew_eff(&self) -> f64 {
        if self.paid <= 0 {
            0.0
        } else {
            self.driving as f64 / self.paid as f64
        }
    }

    pub fn total(&self) -> Cents {
        self.crew_cost + self.vehicle_cost
    }

    /// What Optimise works down: the cost and the soft rules' penalties.
    pub fn algorithmic(&self) -> Cents {
        self.crew_cost + self.penalties
    }
}

/// The costs as the preferences set them, else as the company pays: a driver's hour, a
/// duty's fixed part, a bus a day, a kilometre.
pub fn rates(c: &Company) -> (f64, Cents, Cents, f64) {
    let p = &c.planning.flow.prefs;
    let drivers: Vec<Cents> = c.staff.iter().filter(|e| staff::drives(e)).map(|e| e.wage).collect();
    let level = super::econ_uk::Level::of(c);
    let wage = if drivers.is_empty() { economy::market_wage(45.0, &level) } else { drivers.iter().sum::<Cents>() / drivers.len() as Cents };
    let hour = p.wage.map(|w| w as f64).unwrap_or_else(|| staff::hourly(economy::employer_cost(wage)));
    let fixed = p.driver_fixed.unwrap_or((hour * 3.0).round() as Cents);
    let bus = BusKind { size: BusSize::Solo, drive: Drive::Diesel };
    let per_km = p.per_km.map(|k| k as f64).unwrap_or_else(|| economy::energy_per_km(bus, &level) + economy::maintenance_per_km(bus, 5.0, &level));
    let day = p.vehicle_fixed.unwrap_or(economy::insurance_per_month(bus, &level) / 30);
    (hour, fixed, day, per_km)
}

/// The day as RosterFlow shows it: the blocks, the duties, each duty's shape, the issues and
/// the figures.
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub weekday: u8,
    pub tours: Vec<TourOfDay>,
    pub duties: Vec<Duty>,
    pub shaped: Vec<Shaped>,
    pub issues: Vec<Issue>,
    pub figures: Figures,
    pub stack: Vec<(String, PlannedTrip)>,
}

impl Board {
    /// The issues of `scope` not dismissed (or only the dismissed).
    pub fn open(&self, c: &Company, scope: Scope, dismissed_ones: bool) -> Vec<usize> {
        (0..self.issues.len()).filter(|&i| self.issues[i].scope == scope && dismissed(c, self.weekday, &self.issues[i].key) == dismissed_ones).collect()
    }

    /// The blocks with an issue (the red "!" by their row).
    pub fn block_issues(&self, i: usize) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(move |x| x.scope == Scope::Vehicle && x.row == Some(i))
    }
}

/// The day of `date` (`lines` the timetable read for it) as RosterFlow shows it, from the
/// company as the studio sees it (`view`).
pub fn board(c: &Company, lines: &[LineInfo], date: &str) -> Board {
    let wd = super::plandays::slot_of(c, date);
    let tours = network::tours_of_day(c, lines, date);
    let stack = stack(c, lines, wd);
    let dp = super::plan::day_plan(c, date, tours.clone(), &[], &[], false);
    let mut b = board_of(c, wd, tours, stack);
    b.issues.extend(day_issues(&dp));
    b.issues.extend(timetable_issues(c));
    b
}

/// The company's lines confirmed with their route alone that have no timetable yet
/// (`ownline::no_timetable`): a global issue each, which opens the Timeplan on the line.
pub fn timetable_issues(c: &Company) -> Vec<Issue> {
    c.lines
        .iter()
        .filter(|l| super::ownline::no_timetable(l))
        .map(|l| Issue { scope: Scope::Global, row: None, text: I_NO_TIMETABLE, args: vec![("n", l.number.trim().to_string()), ("line", l.name.clone())], hard: false, penalty: 0, key: format!("{I_NO_TIMETABLE}|{}", l.name.to_lowercase()) })
        .collect()
}

/// The line (its timetable name) an issue opens in the Timeplan: a line without a timetable's.
pub fn timeplan_line_of(i: &Issue) -> Option<&str> {
    (i.text == I_NO_TIMETABLE).then(|| i.args.iter().find(|(k, _)| *k == "line").map(|(_, v)| v.as_str())).flatten()
}

/// What the day's roster has open (testers: "the tour was covered showing no issues" - the
/// blocks and duties were in order while the day's plan had a tour without its bus or a duty
/// whose driver was ill): each tour or duty that fell out, an error while it is open and a
/// warning once it is covered for the day; and the tours in service nobody is planned for.
pub fn day_issues(dp: &super::plan::DayPlan) -> Vec<Issue> {
    use super::plan::Problem;
    let mut out = Vec::new();
    let mut unplanned = 0;
    for t in dp.tours.iter().filter(|t| !t.by_player && !t.tour.unplanned) {
        let mut add = |problem: Problem, filled: bool, what: String| {
            let text = if filled { I_DAY_FILLED } else { I_DAY };
            out.push(Issue { scope: Scope::Global, row: None, text, args: vec![("n", t.tour.number.clone()), ("tour", t.tour.tour.clone()), ("rule", problem.label().to_string())], hard: !filled, penalty: 0, key: format!("{text}|{}|{}|{what}", t.tour.line, t.tour.tour) });
        };
        match t.bus_problem {
            Some(Problem::Unassigned) => {}
            Some(p) => add(p, t.bus.is_some(), "bus".into()),
            None => {}
        }
        if t.bus.is_some() || t.live {
            for (k, d) in t.duties.iter().enumerate() {
                match d.problem {
                    Some(Problem::Unassigned) | None => {}
                    Some(p) => add(p, d.who.is_some(), format!("duty{k}")),
                }
            }
        }
        let none = t.bus_problem == Some(Problem::Unassigned) || t.duties.iter().any(|d| d.problem == Some(Problem::Unassigned) && d.who.is_none());
        if none {
            unplanned += 1;
        }
    }
    if unplanned > 0 {
        out.push(Issue { scope: Scope::Global, row: None, text: I_DAY_UNPLANNED, args: vec![("n", unplanned.to_string())], hard: true, penalty: 0, key: I_DAY_UNPLANNED.into() });
    }
    out
}

/// The board of these blocks (`tours` as `network::tours_of_day` gives them).
pub fn board_of(c: &Company, wd: u8, tours: Vec<TourOfDay>, stack: Vec<(String, PlannedTrip)>) -> Board {
    let p = &c.planning.flow.prefs;
    let duties = st::duties(&c.planning.joined, wd, &tours);
    let mut issues: Vec<Issue> = Vec::new();
    let mut shaped: Vec<Shaped> = Vec::new();
    for (di, d) in duties.iter().enumerate() {
        let key = duty_key(&tours, d);
        let n = key.as_ref().and_then(|k| note(c, wd, k)).cloned().unwrap_or_default();
        let s = shape(c, &tours, d, Some(di), key, &n);
        shaped.push(s);
    }
    for sp in c.planning.flow.sheet.spares.iter().filter(|s| s.weekday == wd) {
        let n = DutyNote { custom: sp.custom.clone(), kind: sp.kind.clone(), locked: sp.locked, paid: sp.paid, ..Default::default() };
        let mut s = shape(c, &tours, &Duty { pieces: Vec::new(), rules: Vec::new() }, None, None, &n);
        s.spare = Some(sp.id);
        shaped.push(s);
    }
    shaped.sort_by_key(|s| (s.spare.is_some(), s.start));
    in_rows(c, wd, &mut shaped);
    // the duties' issues
    for (row, s) in shaped.iter_mut().enumerate() {
        let rk = match (&s.key, s.spare) {
            (Some(k), _) => format!("{}/{}/{}/{}", k.line, k.tour, k.first.name, k.first.dep),
            (None, Some(id)) => format!("spare/{id}"),
            _ => String::new(),
        };
        let mut found: Vec<(&'static str, Vec<(&'static str, String)>, bool, Cents)> = Vec::new();
        let mut add = |text: &'static str, args: Vec<(&'static str, String)>, hard: bool, penalty: Cents| found.push((text, args, hard, penalty));
        if let Some(d) = s.duty.and_then(|k| duties.get(k)) {
            for r in &d.rules {
                match r.minutes() {
                    Some(m) => add(I_RULE_TIME, vec![("rule", r.label().to_string()), ("time", hm(m.abs()))], r.hard(), if r.hard() { 0 } else { 10_00 }),
                    None => add(I_RULE, vec![("rule", r.label().to_string())], r.hard(), 0),
                }
            }
        }
        match s.kind.and_then(|k| p.duty_types.get(k)) {
            Some(t) if t.allowed => {
                if t.penalty > 0 {
                    // (a type allowed at a cost: its penalty counts, no issue)
                }
            }
            Some(t) => add(I_TYPE, vec![("type", t.name.clone())], true, 0),
            None => add(I_TYPE, vec![("type", OTHER.to_string())], true, 0),
        }
        check_breaks(p, s, &mut add);
        if p.max_spread.on && s.spread() > p.max_spread.value {
            add(I_SPREAD, vec![("time", hm(s.spread())), ("max", hm(p.max_spread.value))], p.max_spread.hard, p.max_spread.penalty);
        }
        if p.min_duty.on && s.spread() < p.min_duty.value {
            add(I_SHORT_DUTY, vec![("time", hm(s.spread())), ("min", hm(p.min_duty.value))], p.min_duty.hard, p.min_duty.penalty);
        }
        if p.min_piece.on {
            if let Some(sp) = s.spans.iter().find(|sp| sp.to - sp.from < p.min_piece.value) {
                add(I_SHORT_PIECE, vec![("time", hm(sp.to - sp.from)), ("min", hm(p.min_piece.value))], p.min_piece.hard, p.min_piece.penalty);
            }
        }
        if p.changeovers.on && s.changeovers as i32 > p.changeovers.value {
            add(I_CHANGEOVERS, vec![("n", s.changeovers.to_string()), ("max", p.changeovers.value.to_string())], p.changeovers.hard, p.changeovers.penalty);
        }
        // travel to a relief point elsewhere in time
        for w in 0..s.spans.len().saturating_sub(1) {
            let (a, b) = (s.spans[w], s.spans[w + 1]);
            let (_, end_a) = piece_places(&tours[a.tour], &network::duties_of(&tours[a.tour]).get(a.duty).cloned().unwrap_or(0..0));
            let (start_b, _) = piece_places(&tours[b.tour], &network::duties_of(&tours[b.tour]).get(b.duty).cloned().unwrap_or(0..0));
            // (the same place, unless its break is taken elsewhere: `signing`)
            if same(&end_a, &start_b) && !s.points.breaks.iter().any(|(k, _)| *k == w) {
                continue;
            }
            let need: i32 = s.events.iter().filter(|e| e.from >= a.to && e.from < b.from).map(|e| e.to - e.from).sum();
            if b.from - a.to < need {
                add(I_TRAVEL, vec![("stop", start_b.clone()), ("need", hm(need)), ("have", hm((b.from - a.to).max(0)))], true, 0);
            }
        }
        // (where it begins and ends and takes a split's break: relief points that allow it)
        for (text, stop) in place_issues(c, &tours, s) {
            add(text, vec![("stop", stop)], true, 0);
        }
        for (text, args, hard, penalty) in found {
            s.issues.push(issues.len());
            issues.push(Issue { scope: Scope::Duty, row: Some(row), text, args, hard, penalty, key: format!("{text}|{rk}") });
        }
    }
    // the blocks' issues
    for (i, t) in tours.iter().enumerate() {
        let line: Vec<&PlannedTrip> = tours.iter().filter(|x| same(&x.line, &t.line)).flat_map(|x| x.trips.iter()).collect();
        for w in t.trips.windows(2) {
            let (x, n) = (&w[0], &w[1]);
            let rk = format!("{}/{}/{}", t.line, t.tour, n.dep);
            if !same(x.end(), &n.from) {
                if !x.empty && !n.empty {
                    let _ = st::travel(&line, x.end(), &n.from);
                    issues.push(Issue { scope: Scope::Vehicle, row: Some(i), text: I_DEADHEAD, args: vec![("from", x.end().trim().to_string()), ("to", n.from.trim().to_string())], hard: false, penalty: 0, key: format!("{I_DEADHEAD}|{rk}") });
                }
            } else if p.layover.on && x.counts() && n.counts() && !n.is_second() && n.dep - x.arr < p.layover.value {
                issues.push(Issue { scope: Scope::Vehicle, row: Some(i), text: I_LAYOVER, args: vec![("time", hm(n.dep - x.arr)), ("stop", n.from.trim().to_string()), ("min", hm(p.layover.value))], hard: p.layover.hard, penalty: p.layover.penalty, key: format!("{I_LAYOVER}|{rk}") });
            }
        }
        // (drivers change only where a relief point allows it: a cut made before it said no)
        for r in network::duties_of(t).iter().skip(1) {
            let (Some(x), Some(n)) = (r.start.checked_sub(1).and_then(|k| t.trips.get(k)), t.trips.get(r.start)) else { continue };
            let ok = |place: &str| uses_at(c, &tours, place).is_some_and(|u| u.relief);
            if !(ok(x.end()) || (!same(x.end(), &n.from) && ok(&n.from))) {
                issues.push(Issue { scope: Scope::Vehicle, row: Some(i), text: I_RELIEF_PLACE, args: vec![("stop", n.from.trim().to_string())], hard: true, penalty: 0, key: format!("{I_RELIEF_PLACE}|{}/{}/{}", t.line, t.tour, n.dep) });
            }
        }
    }
    // the day's
    let pvr = st::peak(&tours);
    if !stack.is_empty() {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_UNASSIGNED, args: vec![("n", stack.len().to_string())], hard: true, penalty: 0, key: I_UNASSIGNED.into() });
    }
    if pvr > c.fleet.len() {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_BUSES, args: vec![("n", pvr.to_string()), ("m", c.fleet.len().to_string())], hard: false, penalty: 0, key: I_BUSES.into() });
    }
    if p.depot_capacity.on && pvr as i32 > p.depot_capacity.value {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_DEPOT, args: vec![("n", pvr.to_string()), ("max", p.depot_capacity.value.to_string())], hard: p.depot_capacity.hard, penalty: p.depot_capacity.penalty, key: I_DEPOT.into() });
    }
    let drivers = c.staff.iter().filter(|e| staff::drives(e)).count();
    if shaped.len() > drivers {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_DRIVERS, args: vec![("n", shaped.len().to_string()), ("m", drivers.to_string())], hard: false, penalty: 0, key: I_DRIVERS.into() });
    }
    let figures = figures(c, &tours, &shaped, &issues, pvr, stack.len());
    Board { weekday: wd, tours, duties, shaped, issues, figures, stack }
}

/// Where a duty (`s`) begins and ends, and takes a split's break, at a relief point that does
/// not allow it (`ReliefPoint`): each issue's words and the stop. A place that is no relief point
/// at all is the block's issue (`I_RELIEF_PLACE`).
fn place_issues(c: &Company, tours: &[TourOfDay], s: &Shaped) -> Vec<(&'static str, String)> {
    let p = &c.planning.flow.prefs;
    // (the points it chose, checked always: `signing`)
    let mut out = signing::pick_issues(c, tours, s);
    // (no relief point set: the depot and the termini allow everything)
    if p.points.is_empty() {
        return out;
    }
    let places: Vec<(String, String)> = s.spans.iter().map(|sp| piece_places(&tours[sp.tour], &network::duties_of(&tours[sp.tour]).get(sp.duty).cloned().unwrap_or(0..0))).collect();
    let may = |place: &str, f: fn(&Uses) -> bool| uses_at(c, tours, place).is_none_or(|u| f(&u));
    if let Some((first, _)) = places.first().filter(|_| s.points.on.is_none()) {
        if !may(first, |u| u.sign) {
            out.push((I_SIGN_ON, first.clone()));
        }
    }
    if let Some((_, last)) = places.last().filter(|_| s.points.off.is_none()) {
        if !may(last, |u| u.sign) {
            out.push((I_SIGN_OFF, last.clone()));
        }
    }
    for w in 0..s.spans.len().saturating_sub(1) {
        if s.spans[w + 1].from - s.spans[w].to >= p.split_min && !s.points.breaks.iter().any(|(k, _)| *k == w) && !may(&places[w].1, |u| u.breaks) && !may(&places[w + 1].0, |u| u.breaks) {
            out.push((I_BREAK_PLACE, places[w].1.clone()));
        }
    }
    out
}

/// A duty's shape (`Shaped`) from its pieces and its note.
fn shape(c: &Company, tours: &[TourOfDay], d: &Duty, di: Option<usize>, key: Option<PieceRef>, n: &DutyNote) -> Shaped {
    let p = &c.planning.flow.prefs;
    let mut spans: Vec<Span> = d
        .pieces
        .iter()
        .map(|x| {
            let r = network::duties_of(&tours[x.tour]).get(x.duty).cloned().unwrap_or(0..0);
            let (from, to) = span_of(p, &tours[x.tour], &r);
            Span { tour: x.tour, duty: x.duty, from, to }
        })
        .collect();
    spans.sort_by_key(|s| s.from);
    let mut events: Vec<Ev> = Vec::new();
    let mut splits: Vec<(i32, i32)> = Vec::new();
    let sign_on = n.sign_on.unwrap_or(p.sign_on);
    let sign_off = n.sign_off.unwrap_or(p.sign_off);
    let range = |s: &Span| network::duties_of(&tours[s.tour]).get(s.duty).cloned().unwrap_or(0..0);
    // (where it signs on and off: its own point, the company's, else where the bus is - the
    // travel between that and the bus before and after: `signing`)
    let mut points = signing::Points { on: signing::sign_point(p, n, false), off: signing::sign_point(p, n, true), breaks: Vec::new() };
    if let Some(f) = spans.first() {
        let lead = points.on.as_ref().and_then(|at| signing::leg(c, tours, n, at, &piece_places(&tours[f.tour], &range(f)).0, false));
        let m = lead.map(|x| x.1).unwrap_or(0);
        if let Some((way, _)) = lead {
            events.push(Ev { kind: EvKind::Travel(way), from: f.from - m, to: f.from });
        }
        events.push(Ev { kind: EvKind::SignOn, from: f.from - m - sign_on, to: f.from - m });
    }
    // (a break only where a relief point allows it: the driver waits where he comes from or
    // where he goes to, the travel before or after)
    let rest = |place: &str| p.points.is_empty() || uses_at(c, tours, place).is_none_or(|u| u.breaks);
    let mut no_rest: Vec<(i32, i32)> = Vec::new();
    for w in 0..spans.len().saturating_sub(1) {
        let (a, b) = (spans[w], spans[w + 1]);
        let gap = b.from - a.to;
        let (_, end_a) = piece_places(&tours[a.tour], &range(&a));
        let (start_b, _) = piece_places(&tours[b.tour], &range(&b));
        let set = n.travel.iter().find(|(k, _)| *k as usize == w).map(|x| x.1);
        // (its break taken at a relief point chosen: there and back)
        if let Some(at) = signing::break_point(n, w) {
            signing::break_at(c, tours, &at, (&end_a, a.to), (&start_b, b.from), set, &mut events, &mut splits, &mut no_rest);
            points.breaks.push((w, at));
            continue;
        }
        // (the way as the relief points have it: the pair, the points' own, the defaults)
        let (mode, m) = travel_between(c, tours, &end_a, &start_b, set);
        let (rest_a, rest_b) = (rest(&end_a), rest(&start_b));
        if !rest_a && !rest_b {
            no_rest.push((a.to, b.from));
        }
        if gap >= p.split_min {
            let so = p.sign_off_split;
            let son = p.sign_on_split;
            events.push(Ev { kind: EvKind::SignOff, from: a.to, to: a.to + so });
            // (the split's break where he goes to, when not where he comes from)
            let early = !rest_a && rest_b;
            if m > 0 {
                let at = if early { a.to + so } else { b.from - son - m };
                events.push(Ev { kind: EvKind::Travel(mode), from: at, to: at + m });
            }
            events.push(Ev { kind: EvKind::SignOn, from: b.from - son, to: b.from });
            let (s0, s1) = if early { (a.to + so + m, b.from - son) } else { (a.to + so, b.from - son - m) };
            if s1 > s0 {
                splits.push((s0, s1));
            }
        } else if m > 0 {
            // (the wait where he comes from, when not where he goes to)
            let at = if !rest_b && rest_a { b.from - m } else { a.to };
            events.push(Ev { kind: EvKind::Travel(mode), from: at, to: at + m });
        }
    }
    if p.layover_breaks {
        for sp in &spans {
            let t = &tours[sp.tour];
            let r = range(sp);
            for k in r.start + 1..r.end {
                let (x, nx) = (&t.trips[k - 1], &t.trips[k]);
                if same(x.end(), &nx.from) && nx.dep > x.arr && !rest(x.end()) {
                    no_rest.push((x.arr, nx.dep));
                }
            }
        }
    }
    if let Some(l) = spans.last() {
        let tail = points.off.as_ref().and_then(|at| signing::leg(c, tours, n, at, &piece_places(&tours[l.tour], &range(l)).1, true));
        let m = tail.map(|x| x.1).unwrap_or(0);
        if let Some((way, _)) = tail {
            events.push(Ev { kind: EvKind::Travel(way), from: l.to, to: l.to + m });
        }
        events.push(Ev { kind: EvKind::SignOff, from: l.to + m, to: l.to + m + sign_off });
    }
    for (k, cw) in n.custom.iter().enumerate() {
        events.push(Ev { kind: EvKind::Custom(k), from: cw.from, to: cw.to });
    }
    events.sort_by_key(|e| e.from);
    let start = events.iter().map(|e| e.from).chain(spans.iter().map(|s| s.from)).min().unwrap_or(0);
    let end = events.iter().map(|e| e.to).chain(spans.iter().map(|s| s.to)).max().unwrap_or(0);
    let unpaid: i32 = splits.iter().map(|s| s.1 - s.0).sum();
    let work = (end - start - unpaid).max(0);
    let paid = n.paid.unwrap_or(work.max(paid_floor(p)));
    let driving: i32 = spans.iter().map(|s| tours[s.tour].trips[range(s)].iter().map(|x| x.arr - x.dep).sum::<i32>()).sum();
    let platform: i32 = spans.iter().map(|s| s.to - s.from).sum();
    let changeovers = spans.windows(2).filter(|w| w[0].tour != w[1].tour).count();
    let split = !splits.is_empty();
    let set = n.kind.as_ref().map(|name| p.duty_types.iter().position(|t| same(&t.name, name)));
    let (kind, kind_set) = match set {
        Some(k) => (k, true),
        None => (p.duty_types.iter().position(|t| t.fits(start, end, split)), false),
    };
    let mut s = Shaped { duty: di, spare: None, key, spans, events, custom: n.custom.clone(), start, end, work, paid, driving, platform, splits, changeovers, kind, kind_set, locked: n.locked, issues: Vec::new(), plan: BreakPlan::default(), no_rest, points };
    s.plan = break_plan(p, tours, &s);
    s
}

/// The least a duty is paid: four hours (`MIN_PAID`), or the least length of a duty (Time
/// Limitations, `Prefs::min_duty`) where that is longer and in use.
pub fn paid_floor(p: &Prefs) -> i32 {
    if p.min_duty.on {
        MIN_PAID.max(p.min_duty.value)
    } else {
        MIN_PAID
    }
}

/// The breaks of a duty a meal or a rest can be taken in: the gaps between its work, and (if
/// the preferences say so) the layovers inside its pieces with no other work in them - only
/// those of `Prefs::break_from` or more: a shorter gap is standing time.
pub fn breaks_of(p: &Prefs, tours: &[TourOfDay], s: &Shaped) -> Vec<(i32, i32)> {
    let mut busy: Vec<(i32, i32)> = s.spans.iter().map(|x| (x.from, x.to)).chain(s.events.iter().map(|e| (e.from, e.to))).collect();
    busy.sort();
    let mut out: Vec<(i32, i32)> = Vec::new();
    let mut at = busy.first().map(|b| b.1).unwrap_or(0);
    for b in busy.iter().skip(1) {
        if b.0 > at {
            out.push((at, b.0));
        }
        at = at.max(b.1);
    }
    if p.layover_breaks {
        for sp in &s.spans {
            let t = &tours[sp.tour];
            let r = network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
            for k in r.start + 1..r.end {
                let (x, n) = (&t.trips[k - 1], &t.trips[k]);
                // (custom work or travel in the layover: no break)
                if same(x.end(), &n.from) && n.dep > x.arr && !s.events.iter().any(|e| e.from < n.dep && x.arr < e.to) {
                    out.push((x.arr, n.dep));
                }
            }
        }
    }
    out.retain(|b| b.1 - b.0 >= p.break_from.max(1));
    // (none at a relief point that allows no break)
    out.retain(|b| !s.no_rest.iter().any(|r| b.0 >= r.0 && b.1 <= r.1));
    out.sort();
    out
}

/// What a break of a duty is, as RosterFlow shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    /// Between two pieces, long enough to be a split (unpaid: `Prefs::split_min`).
    Split,
    /// Long enough for the meal rule (`Prefs::meal`).
    Meal,
    /// Long enough to end a driving period (`Prefs::break_min`).
    Break,
    /// A break (`Prefs::break_from`), too short to end a driving period.
    Short,
}

/// A break of a duty: when, what it is, and which rules it counts for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DutyBreak {
    pub from: i32,
    pub to: i32,
    pub kind: BreakKind,
    /// Long enough for the meal break a long duty needs (`Prefs::meal`).
    pub meal: bool,
    /// Long enough to end a driving period (`Prefs::break_min`).
    pub resets: bool,
}

/// A driving period of a duty: its work from the sign-on, or from a break that ends one, to the
/// next such break or the sign-off (the continuous rule's), and whether it is longer than the
/// rule allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    pub from: i32,
    pub to: i32,
    pub over: bool,
}

/// Where a duty breaks the meal rule: from `from`, when it has worked `Prefs::meal_after`, to
/// `to` (its meal break that came too late, or its end); `late` the minutes worked before that
/// late meal break (None: no meal break at all).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MealDue {
    pub from: i32,
    pub to: i32,
    pub late: Option<i32>,
}

/// A duty's breaks and its driving periods between them as the rules see them: the issues
/// (`check_breaks`), the Gantt's labels and band and the duty panel all take them from here, so
/// the picture and the issues agree (testers: "it does not tell me so it's a bit of guesswork").
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BreakPlan {
    pub breaks: Vec<DutyBreak>,
    pub stretches: Vec<Stretch>,
    /// The meal rule broken (`MealDue`).
    pub meal_due: Option<MealDue>,
}

impl BreakPlan {
    /// The longest driving period.
    pub fn longest(&self) -> i32 {
        self.stretches.iter().map(|s| s.to - s.from).max().unwrap_or(0)
    }
}

/// The minute duty `s` has worked `worked` minutes by (its splits unpaid, not work).
fn minute_worked(s: &Shaped, worked: i32) -> i32 {
    let mut at = s.start;
    let mut left = worked;
    let mut splits = s.splits.clone();
    splits.sort();
    for (a, z) in splits {
        if at + left <= a {
            break;
        }
        left -= (a - at).max(0);
        at = at.max(z);
    }
    at + left
}

/// The minutes duty `s` has worked by minute `m` (its splits unpaid, not work).
fn worked_by(s: &Shaped, m: i32) -> i32 {
    let off: i32 = s.splits.iter().map(|x| (x.1.min(m) - x.0).max(0)).sum();
    (m - s.start - off).max(0)
}

/// The breaks of duty `s` and the driving periods between them (`BreakPlan`).
pub fn break_plan(p: &Prefs, tours: &[TourOfDay], s: &Shaped) -> BreakPlan {
    let mut out = BreakPlan::default();
    if s.spans.is_empty() {
        return out;
    }
    for (from, to) in breaks_of(p, tours, s) {
        let len = to - from;
        let split = s.splits.iter().any(|x| x.0 < to && from < x.1);
        let meal = p.meal.on && len >= p.meal.value;
        let resets = len >= p.break_min;
        let kind = if split {
            BreakKind::Split
        } else if meal {
            BreakKind::Meal
        } else if resets {
            BreakKind::Break
        } else {
            BreakKind::Short
        };
        out.breaks.push(DutyBreak { from, to, kind, meal, resets });
    }
    // the driving periods: from the sign-on to each break that ends one, and on to the sign-off
    let mut since = s.start;
    for b in out.breaks.iter().filter(|b| b.resets) {
        if b.from < since {
            since = since.max(b.to);
            continue;
        }
        out.stretches.push(Stretch { from: since, to: b.from, over: false });
        since = b.to;
    }
    if s.end > since || out.stretches.is_empty() {
        out.stretches.push(Stretch { from: since, to: s.end.max(since), over: false });
    }
    for st in out.stretches.iter_mut() {
        st.over = p.continuous.on && st.to - st.from > p.continuous.value;
    }
    // the meal rule: a duty working over `meal_after` has its meal break before it has worked
    // that long
    if p.meal.on && s.work > p.meal_after {
        let due = minute_worked(s, p.meal_after);
        if !out.breaks.iter().any(|b| b.meal && b.from <= due) {
            let late = out.breaks.iter().find(|b| b.meal && b.from > due);
            out.meal_due = Some(MealDue { from: due, to: late.map(|b| b.from).unwrap_or(s.end), late: late.map(|b| worked_by(s, b.from)) });
        }
    }
    out
}

/// The meal and continuous rules of duty `s`, as its `BreakPlan` has them.
fn check_breaks(p: &Prefs, s: &Shaped, add: &mut dyn FnMut(&'static str, Vec<(&'static str, String)>, bool, Cents)) {
    let plan = &s.plan;
    match plan.meal_due {
        Some(MealDue { late: Some(m), .. }) => add(I_MEAL_LATE, vec![("time", hm(m)), ("after", hm(p.meal_after)), ("min", hm(p.meal.value))], p.meal.hard, p.meal.penalty),
        Some(_) => add(I_MEAL, vec![("after", hm(p.meal_after)), ("min", hm(p.meal.value))], p.meal.hard, p.meal.penalty),
        None => {}
    }
    if plan.stretches.iter().any(|x| x.over) {
        add(I_CONTINUOUS, vec![("time", hm(plan.longest())), ("max", hm(p.continuous.value)), ("min", hm(p.break_min.max(p.break_from)))], p.continuous.hard, p.continuous.penalty);
    }
}

/// The longest stretch from `start` to `end` without a break of `min` or more.
pub fn longest_stretch(start: i32, end: i32, breaks: &[(i32, i32)], min: i32) -> i32 {
    let mut since = start;
    let mut longest = 0;
    for b in breaks.iter().filter(|b| b.1 - b.0 >= min) {
        if b.0 < since {
            since = since.max(b.1);
            continue;
        }
        longest = longest.max(b.0 - since);
        since = b.1;
    }
    longest.max(end - since)
}

fn figures(c: &Company, tours: &[TourOfDay], shaped: &[Shaped], issues: &[Issue], pvr: usize, stacked: usize) -> Figures {
    let p = &c.planning.flow.prefs;
    let (hour, fixed, day, per_km) = rates(c);
    let mut f = Figures { duties: shaped.len(), blocks: tours.len(), pvr, ..Default::default() };
    for t in tours {
        let line: Vec<&PlannedTrip> = tours.iter().filter(|x| same(&x.line, &t.line)).flat_map(|x| x.trips.iter()).collect();
        f.dead_km += st::deadheads(t, &line).iter().map(|d| d.2).sum::<f64>();
        for x in &t.trips {
            if x.counts() {
                f.service_km += x.km;
                // (a trip two drivers share once)
                f.assigned += usize::from(!x.is_second());
            } else {
                f.dead_km += x.km;
            }
        }
    }
    f.trips = f.assigned + stacked;
    for s in shaped {
        f.driving += s.driving;
        f.paid += s.paid;
        f.spread += s.spread();
        f.work += s.work;
        f.platform += s.platform;
        if s.split() {
            f.splits += 1;
            f.split_time += s.splits.iter().map(|x| x.1 - x.0).sum::<i32>();
            f.split_changeovers += s.changeovers;
        }
        f.changeovers += s.changeovers;
        let band = match s.paid {
            m if m < 300 => 0,
            m if m < 420 => 1,
            m if m < 540 => 2,
            m if m < 720 => 3,
            _ => 4,
        };
        f.paid_bands[band] += 1;
        if let Some(t) = s.kind.and_then(|k| p.duty_types.get(k)) {
            f.penalties += t.penalty;
        }
    }
    for t in p.duty_types.iter() {
        f.types.push((t.name.clone(), shaped.iter().filter(|s| s.kind.and_then(|k| p.duty_types.get(k)).is_some_and(|x| x.name == t.name)).count()));
    }
    f.types.push((OTHER.to_string(), shaped.iter().filter(|s| s.kind.is_none()).count()));
    let night: i32 = shaped.iter().flat_map(|s| s.spans.iter()).map(|x| staff::night_minutes(x.from, x.to)).sum();
    f.crew_cost = ((f.paid as f64 + night as f64 * staff::NIGHT_SUPPLEMENT) / 60.0 * hour).round() as Cents + fixed * shaped.len() as Cents;
    f.vehicle_cost = ((f.service_km + f.dead_km) * per_km).round() as Cents + day * pvr as Cents;
    f.penalties += issues.iter().filter(|i| !i.hard).map(|i| i.penalty).sum::<Cents>();
    f
}

// --- Optimise --------------------------------------------------------------------------------

/// A run of trips that stays on one bus (a trip with its depot runs, linked trips).
#[derive(Clone, Debug)]
struct Unit {
    trips: Vec<PlannedTrip>,
}

impl Unit {
    fn dep(&self) -> i32 {
        self.trips.first().map(|x| x.dep).unwrap_or(0)
    }

    fn starts_in_depot(&self) -> bool {
        self.trips.first().is_some_and(|x| x.empty)
    }

    fn ends_in_depot(&self) -> bool {
        self.trips.last().is_some_and(|x| x.empty)
    }
}

/// The blocks of one line as Optimise makes them: the locked blocks as they are, every other
/// trip chained onto the bus that is free the latest before it (fewest buses), with time to
/// get there and the layover the preferences ask for.
fn block_line(c: &Company, wd: u8, line: &str, raw: &[&TourOfDay], current: &[&TourOfDay], keep: &[String]) -> LineLayout {
    let p = &c.planning.flow.prefs;
    let mut blocks: Vec<st::BlockLayout> = Vec::new();
    let mut taken: Vec<TripRef> = Vec::new();
    for t in current.iter().filter(|t| keep.iter().any(|k| same(k, &t.tour))) {
        let b = st::layout_of_block(t);
        taken.extend(b.trips.iter().cloned());
        blocks.push(b);
    }
    // the units: each trip with passengers, with the depot runs before the first of its tour
    // and after the last, the empty runs in the day with the trip before them
    let mut units: Vec<Unit> = Vec::new();
    for t in raw {
        let counting: Vec<usize> = (0..t.trips.len()).filter(|&k| t.trips[k].counts()).collect();
        let (Some(&first), Some(&last)) = (counting.first(), counting.last()) else { continue };
        let mut cur: Option<Unit> = None;
        for (k, x) in t.trips.iter().enumerate() {
            if taken.iter().any(|r| r.is(x)) {
                continue;
            }
            if x.counts() {
                if let Some(u) = cur.take() {
                    units.push(u);
                }
                let mut u = Unit { trips: Vec::new() };
                if k == first {
                    u.trips.extend(t.trips[..k].iter().filter(|y| !taken.iter().any(|r| r.is(y))).cloned());
                }
                u.trips.push(x.clone());
                cur = Some(u);
            } else if k > first {
                if let Some(u) = cur.as_mut() {
                    u.trips.push(x.clone());
                }
            }
            if k == last {
                // (what comes after the last trip goes with it)
                if let Some(u) = cur.as_mut() {
                    u.trips.extend(t.trips[k + 1..].iter().filter(|y| !taken.iter().any(|r| r.is(y))).cloned());
                }
                if let Some(u) = cur.take() {
                    units.push(u);
                }
                break;
            }
        }
    }
    // linked trips one unit
    let links: Vec<&Link> = c.planning.flow.sheet.links.iter().filter(|l| l.weekday == wd && same(&l.line, line)).collect();
    for l in links {
        let at: Vec<usize> = (0..units.len()).filter(|&i| units[i].trips.iter().any(|x| l.trips.iter().any(|r| r.is(x)))).collect();
        if at.len() > 1 {
            let mut merged = Unit { trips: Vec::new() };
            for &i in at.iter().rev() {
                merged.trips.extend(units.remove(i).trips);
            }
            merged.trips.sort_by_key(|x| x.dep);
            units.push(merged);
        }
    }
    units.sort_by_key(|u| u.dep());
    let all: Vec<&PlannedTrip> = raw.iter().flat_map(|t| t.trips.iter()).collect();
    // (open buses: their trips, closed once back in the depot)
    let mut open: Vec<(Vec<PlannedTrip>, bool)> = Vec::new();
    for u in units {
        let mut best: Option<(usize, i32, bool)> = None;
        if !u.starts_in_depot() {
            for (bi, (trips, closed)) in open.iter().enumerate() {
                if *closed {
                    continue;
                }
                let Some(last) = trips.last() else { continue };
                let travel = st::travel(&all, last.end(), &u.trips[0].from);
                let free = last.arr + travel;
                if free > u.dep() {
                    continue;
                }
                let lay_ok = !p.layover.on || u.dep() - last.arr >= p.layover.value;
                if p.layover.on && p.layover.hard && !lay_ok {
                    continue;
                }
                // (the bus free the latest; one keeping the layover first)
                let better = match best {
                    None => true,
                    Some((_, at, ok)) => (lay_ok && !ok) || (lay_ok == ok && last.arr > at),
                };
                if better {
                    best = Some((bi, last.arr, lay_ok));
                }
            }
        }
        let closes = u.ends_in_depot();
        match best {
            Some((bi, _, _)) => {
                open[bi].0.extend(u.trips);
                open[bi].1 = closes;
            }
            None => open.push((u.trips, closes)),
        }
    }
    // their names: the timetable tour each runs most of, else a new number
    let mut names: Vec<String> = blocks.iter().map(|b| b.tour.trim().to_string()).collect();
    for (trips, _) in open {
        let mut count: Vec<(String, usize)> = Vec::new();
        for x in trips.iter().filter(|x| x.counts()) {
            match count.iter_mut().find(|c| c.0 == x.origin.trim()) {
                Some(c) => c.1 += 1,
                None => count.push((x.origin.trim().to_string(), 1)),
            }
        }
        count.sort_by_key(|c| std::cmp::Reverse(c.1));
        let name = count.into_iter().map(|c| c.0).find(|n| !n.is_empty() && !names.contains(n)).unwrap_or_else(|| {
            let mut k = names.iter().filter_map(|n| n.parse::<u32>().ok()).max().unwrap_or(0) + 1;
            while names.contains(&k.to_string()) {
                k += 1;
            }
            k.to_string()
        });
        names.push(name.clone());
        blocks.push(st::BlockLayout { tour: name, trips: trips.iter().filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect(), cuts: None, reliefs: Vec::new() });
    }
    LineLayout { weekday: wd, line: line.to_string(), blocks, stack: Vec::new() }
}

/// A piece of work while duties are made: its block and trips, its time and places.
#[derive(Clone, Debug)]
struct Cut {
    tour: usize,
    range: Range<usize>,
    from: i32,
    to: i32,
    at: String,
    till: String,
}

/// Where drivers may change on block `i` (`tours` the day's): before a trip with passengers that
/// has one before it, by the rule a cut by hand keeps (`studio::may_change`) - the bus standing
/// at a relief point that allows it, or running empty from where one trip ends to where the next
/// begins (a line run one way only, a terminus whose stops for arriving and leaving differ).
/// (Optimise asked the bus to stand at one stop: it never cut such a block, and its duties broke
/// the hard rules - testers, 2026-10-15: "optimise feature do not work".)
fn change_points(c: &Company, tours: &[TourOfDay], i: usize) -> Vec<usize> {
    let t = &tours[i];
    (1..t.trips.len()).filter(|&k| t.trips[k].counts() && t.trips[..k].iter().any(|x| x.counts()) && st::may_change(c, tours, &t.line, &t.trips[k - 1], &t.trips[k])).collect()
}

/// The longest piece Optimise cuts: as long as a driver may work without a break.
fn longest_piece(p: &Prefs) -> i32 {
    if p.continuous.on { p.continuous.value } else { staff::DUTY_MAX }.min(staff::DAY_MAX)
}

/// Where block `t` can be cut (`change_points`), and the pieces Optimise cuts it into: as long as
/// a driver may work without a break, not shorter than the least piece where it can help.
fn cut_block(c: &Company, tours: &[TourOfDay], i: usize, p: &Prefs) -> Vec<Range<usize>> {
    let t = &tours[i];
    let n = t.trips.len();
    if n == 0 {
        return Vec::new();
    }
    let longest = longest_piece(p);
    let can = change_points(c, tours, i);
    let span = |a: usize, b: usize| t.trips[a..b].iter().map(|x| x.arr).max().unwrap_or(0) - t.trips[a].dep;
    // (the trips of a piece from `a` to `b` may run as long as the driver's work then stays within
    // the limit: his signing, and the bus's check where it leaves or comes back to the depot, in)
    let room = |a: usize, b: usize| longest - p.sign_on - p.sign_off - if a == 0 { p.pre_trip } else { 0 } - if b >= n { p.post_trip } else { 0 };
    let mut cuts: Vec<usize> = Vec::new();
    let mut s = 0;
    while span(s, n) > room(s, n) {
        let fit: Vec<usize> = can.iter().copied().filter(|&k| k > s && span(s, k) <= room(s, k)).collect();
        let next = fit.last().copied().or_else(|| can.iter().copied().find(|&k| k > s));
        match next {
            Some(k) => {
                cuts.push(k);
                s = k;
            }
            None => break,
        }
    }
    // (a last piece too short goes into the one before, where that stays within reach - within
    // the continuous limit where that is a hard rule: a soft rule is not kept by breaking one)
    if p.min_piece.on {
        if let Some(&k) = cuts.last() {
            let prev = cuts.len().checked_sub(2).map(|j| cuts[j]).unwrap_or(0);
            let reach = if p.continuous.on && p.continuous.hard { room(prev, n) } else { longest + longest / 4 };
            if span(k, n) < p.min_piece.value && span(prev, n) <= reach {
                cuts.pop();
            }
        }
    }
    let mut out = Vec::new();
    let mut from = 0;
    for k in cuts.into_iter().chain(std::iter::once(n)) {
        out.push(from..k);
        from = k;
    }
    out
}

/// What Optimise weighs a hard rule broken as: a bus a day more (`weighed`).
const HARD: f64 = 500_00.0;

/// The rules of a duty as a whole Optimise weighs for pieces `ps` (in order) as one duty: its
/// type (one the duty types allow, at its penalty), its length (Time Limitations: the spread at
/// most and at least) and its changes of bus - the soft ones' penalties, and how many hard ones
/// it breaks. (The duty's sign-on and sign-off as `shape` has them; a gap of a split's length a
/// split.)
fn whole_rules(p: &Prefs, ps: &[&Cut]) -> (f64, usize) {
    let start = ps.first().map(|x| x.from).unwrap_or(0) - p.sign_on;
    let end = ps.iter().map(|x| x.to).max().unwrap_or(0) + p.sign_off;
    let split = ps.windows(2).any(|w| w[1].from - w[0].to >= p.split_min);
    let changes = ps.windows(2).filter(|w| w[0].tour != w[1].tour).count() as i32;
    let (mut penalty, mut hard) = (0.0, 0);
    let mut rule = |broken: bool, lim: &Limit| {
        if broken && lim.on {
            if lim.hard {
                hard += 1;
            } else {
                penalty += lim.penalty as f64;
            }
        }
    };
    rule(end - start > p.max_spread.value, &p.max_spread);
    rule(end - start < p.min_duty.value, &p.min_duty);
    rule(changes > p.changeovers.value, &p.changeovers);
    // (never longer than a day leaves the night's rest for: no duty across two service days)
    if end - start > 1440 - staff::NIGHT_REST {
        hard += 1;
    }
    match p.duty_types.iter().find(|t| t.fits(start, end, split)) {
        Some(t) if t.allowed => penalty += t.penalty as f64,
        _ => hard += 1,
    }
    (penalty, hard)
}

/// Whether pieces `ps` (in order) may be one duty by the meal and continuous rules as far as the
/// gaps between them go, and the penalties of the soft ones they break (None: a hard one): the
/// pieces joined by gaps too short to end a driving period (`Prefs::break_min`, `break_from`)
/// within the continuous limit together, and a duty working over `meal_after` with a gap of a
/// meal break before it has worked that long. (A piece alone is as `cut_block` cut it.)
fn pair_breaks(p: &Prefs, ps: &[&Cut]) -> Option<f64> {
    let mut penalty = 0.0;
    let mut broken = |lim: &Limit| {
        if lim.hard {
            return false;
        }
        penalty += lim.penalty as f64;
        true
    };
    let rest = p.break_min.max(p.break_from);
    if p.continuous.on {
        let mut run = ps.first().map(|x| x.from).unwrap_or(0);
        let mut over = false;
        for w in ps.windows(2) {
            if w[1].from - w[0].to >= rest {
                run = w[1].from;
            } else {
                over |= w[1].to - run > p.continuous.value;
            }
        }
        if over && !broken(&p.continuous) {
            return None;
        }
    }
    if p.meal.on {
        let start = ps.first().map(|x| x.from).unwrap_or(0) - p.sign_on;
        let end = ps.iter().map(|x| x.to).max().unwrap_or(0) + p.sign_off;
        let gaps: Vec<(i32, i32)> = ps.windows(2).map(|w| (w[0].to, w[1].from)).collect();
        let unpaid: i32 = gaps.iter().map(|g| g.1 - g.0).filter(|g| *g >= p.split_min).sum();
        if end - start - unpaid > p.meal_after {
            // (the minute it has worked `meal_after`: the splits unpaid)
            let mut due = start + p.meal_after;
            for g in gaps.iter().filter(|g| g.1 - g.0 >= p.split_min) {
                if g.0 < due {
                    due += g.1 - g.0;
                }
            }
            let meal = p.meal.value.max(p.break_from);
            if !gaps.iter().any(|g| g.1 - g.0 >= meal && g.0 <= due) && !broken(&p.meal) {
                return None;
            }
        }
    }
    Some(penalty)
}

/// What a planning costs Optimise: the day's cost, the soft rules' penalties, and every hard
/// rule broken as dear as a bus a day more (so a planning that breaks none is chosen first) -
/// of the planning's own (`of_the_plan`).
pub fn weighed(b: &Board) -> Cents {
    b.figures.total() + b.figures.penalties + 500_00 * b.issues.iter().filter(|i| i.hard && of_the_plan(i)).count() as Cents
}

/// Whether an issue is the planning's own, Optimise's to keep: not the day's roster (an ill
/// driver, a bus in the workshop, a tour nobody is planned for - the roster follows the blocks
/// and duties as they are saved, `plan::follow`) nor a line without a timetable.
pub fn of_the_plan(i: &Issue) -> bool {
    ![I_DAY, I_DAY_FILLED, I_DAY_UNPLANNED, I_NO_TIMETABLE].contains(&i.text)
}

/// What Optimise makes of `date` (`lines` the timetable read for it), from the company as the
/// studio sees it: the weekday's blocks and joined duties, the locked ones kept - the cheapest
/// (`weighed`) of what the preferences ask for, the duties alone made anew on the blocks as
/// they are, and the day as it is. Nothing is kept; `apply` keeps it.
pub fn optimise(c: &Company, lines: &[LineInfo], date: &str) -> (Vec<LineLayout>, Vec<JoinedDuty>) {
    let o = optimise_day(c, lines, date);
    (o.layouts, o.joined)
}

/// What Optimise made of a day (`optimise_day`): the blocks and joined duties, the company with
/// them and the day as they make it, and how it weighs against the day as it is.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub view: Company,
    pub board: Board,
    /// It weighs less than the day as it is (`weighed`); else no better plan was found, and it is
    /// the day as it is.
    pub better: bool,
    /// What the day weighs as it is, and as made.
    pub was: Cents,
    pub cost: Cents,
}

/// Optimise (`optimise`) with what it made and how it weighs: the day as it is first, a plan made
/// only where it weighs less (a tie keeps the day as it is).
pub fn optimise_day(c: &Company, lines: &[LineInfo], date: &str) -> Outcome {
    let wd = super::plandays::slot_of(c, date);
    let p = &c.planning.flow.prefs;
    let now = (c.planning.layouts.iter().filter(|l| l.weekday == wd).cloned().collect::<Vec<_>>(), c.planning.joined.iter().filter(|j| j.weekday == wd).cloned().collect::<Vec<_>>());
    let mut cands = vec![now];
    if p.optimise_vehicles || p.optimise_duties {
        cands.push(optimise_with(c, lines, date, p.optimise_vehicles, p.optimise_duties));
    }
    if p.optimise_vehicles && p.optimise_duties {
        cands.push(optimise_with(c, lines, date, false, true));
    }
    let mut best: Option<Outcome> = None;
    let mut was = 0;
    for (k, (layouts, joined)) in cands.into_iter().enumerate() {
        let mut view = c.clone();
        apply(&mut view, wd, layouts.clone(), joined.clone());
        let board = board(&view, lines, date);
        let cost = weighed(&board);
        if k == 0 {
            was = cost;
        }
        if best.as_ref().is_none_or(|b| cost < b.cost) {
            best = Some(Outcome { layouts, joined, view, board, better: k > 0, was, cost });
        }
    }
    let mut o = best.expect("the day as it is");
    o.was = was;
    o
}

/// The blocks of day `b` drivers may change on nowhere (`change_points`) whose duty breaks a hard
/// rule: what Optimise cannot cut, for want of a relief point (`c` the company `b` is of).
pub fn uncuttable(c: &Company, b: &Board) -> usize {
    (0..b.tours.len())
        .filter(|&i| network::duties_of(&b.tours[i]).len() == 1 && change_points(c, &b.tours, i).is_empty())
        .filter(|&i| b.shaped.iter().any(|s| s.spans.iter().any(|sp| sp.tour == i) && s.issues.iter().any(|&k| b.issues[k].hard)))
        .count()
}

/// Optimise with the vehicles (`vehicles`) and the duties (`duties`) made anew or kept.
pub fn optimise_with(c: &Company, lines: &[LineInfo], date: &str, vehicles: bool, duties: bool) -> (Vec<LineLayout>, Vec<JoinedDuty>) {
    let wd = super::plandays::slot_of(c, date);
    let mut p = c.planning.flow.prefs.clone();
    p.optimise_vehicles = vehicles;
    p.optimise_duties = duties;
    let current = network::tours_of_day(c, lines, date);
    let raw = network::timetable_tours(c, lines);
    let duties = st::duties(&c.planning.joined, wd, &current);
    // the locked duties, and the blocks they hold (kept as they are)
    let locked: Vec<usize> = (0..duties.len()).filter(|&d| duty_key(&current, &duties[d]).is_some_and(|k| duty_locked(c, wd, &k))).collect();
    let mut keep: Vec<(String, String)> = c.planning.flow.sheet.locks.iter().filter(|l| l.weekday == wd).map(|l| (l.line.clone(), l.tour.clone())).collect();
    for &d in &locked {
        for pc in &duties[d].pieces {
            let t = &current[pc.tour];
            keep.push((t.line.clone(), t.tour.clone()));
        }
    }
    // the vehicles
    let mut layouts: Vec<LineLayout> = Vec::new();
    for cl in &c.lines {
        let mine_raw: Vec<&TourOfDay> = raw.iter().filter(|t| same(&t.line, &cl.name)).collect();
        if mine_raw.is_empty() {
            continue;
        }
        let mine_now: Vec<&TourOfDay> = current.iter().filter(|t| same(&t.line, &cl.name)).collect();
        let lay = if p.optimise_vehicles {
            let k: Vec<String> = keep.iter().filter(|(l, _)| same(l, &cl.name)).map(|(_, t)| t.clone()).collect();
            block_line(c, wd, &cl.name, &mine_raw, &mine_now, &k)
        } else {
            let mut now = c.planning.layouts.iter().find(|l| l.weekday == wd && same(&l.line, &cl.name)).cloned().unwrap_or_else(|| LineLayout { weekday: wd, line: cl.name.clone(), ..Default::default() });
            now.blocks = mine_now.iter().map(|t| st::layout_of_block(t)).collect();
            now
        };
        layouts.push(lay);
    }
    let locked_joins: Vec<JoinedDuty> = c.planning.joined.iter().filter(|j| j.weekday == wd).filter(|j| locked.iter().any(|&d| duties[d].pieces.iter().any(|pc| st::piece_ref(&current, pc.tour, pc.duty).is_some_and(|r| j.pieces.iter().any(|x| x.is(&r)))))).cloned().collect();
    if !p.optimise_duties {
        let joined = c.planning.joined.iter().filter(|j| j.weekday == wd).cloned().collect();
        return (layouts, joined);
    }
    // the blocks as they now are, to cut
    let mut probe = c.clone();
    probe.planning.layouts.retain(|l| l.weekday != wd);
    probe.planning.layouts.extend(layouts.iter().cloned());
    probe.planning.joined.retain(|j| j.weekday != wd);
    probe.planning.joined.extend(locked_joins.iter().cloned());
    let tours = network::tours_of_day(&probe, lines, date);
    let kept = |t: &TourOfDay| keep.iter().any(|(l, b)| same(l, &t.line) && same(b, &t.tour));
    // (the locked duties' pieces stay in them: not paired anew)
    let locked_refs: Vec<PieceRef> = locked.iter().flat_map(|&d| duties[d].pieces.iter().filter_map(|pc| st::piece_ref(&current, pc.tour, pc.duty))).collect();
    let mut cuts: Vec<Cut> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        // (a block kept as it is keeps its cuts, and its other pieces still go into duties: left
        // out, a locked bus broke the duties it shared with other buses into pieces of their own)
        let ranges = if kept(t) { network::duties_of(t) } else { cut_block(&probe, &tours, i, &p) };
        for r in ranges {
            if kept(t) && t.trips.get(r.start).is_some_and(|x| locked_refs.iter().any(|l| l.is(&PieceRef { line: t.line.clone(), tour: t.tour.clone(), first: TripRef::of(x) }))) {
                continue;
            }
            let (from, to) = span_of(&p, t, &r);
            let (at, till) = piece_places(t, &r);
            cuts.push(Cut { tour: i, range: r, from, to, at, till });
        }
    }
    cuts.sort_by_key(|x| x.from);
    // pair the pieces into duties: each onto the duty it adds the least cost to, or a new one
    let (hour, fixed, _, _) = rates(c);
    let rate = hour / 60.0;
    let paid_of = |ps: &[&Cut]| -> i32 {
        let start = ps.first().map(|x| x.from).unwrap_or(0) - p.sign_on;
        let end = ps.iter().map(|x| x.to).max().unwrap_or(0) + p.sign_off;
        let unpaid: i32 = ps.windows(2).map(|w| w[1].from - w[0].to).filter(|g| *g >= p.split_min).sum();
        (end - start - unpaid).max(paid_floor(&p))
    };
    let mut groups: Vec<Vec<usize>> = Vec::new();
    // (a duty begins where a relief point allows it, and takes a split's break where one does)
    let set = !p.points.is_empty();
    let unsigned = |place: &str| set && uses_at(c, &tours, place).is_some_and(|u| !u.sign);
    let no_rest = |place: &str| set && uses_at(c, &tours, place).is_some_and(|u| !u.breaks);
    for (ci, x) in cuts.iter().enumerate() {
        // (a duty of its own: its hours, and the rules of a duty as a whole it breaks alone)
        let (pen_x, hard_x) = whole_rules(&p, &[x]);
        let alone = fixed as f64 + paid_of(&[x]) as f64 * rate + pen_x + HARD * hard_x as f64 + if unsigned(&x.at) { HARD } else { 0.0 };
        let mut best: Option<(usize, f64)> = None;
        for (gi, g) in groups.iter().enumerate() {
            let last = &cuts[*g.last().unwrap()];
            let gap = x.from - last.to;
            // (the way as the relief points have it)
            let travel = travel_between(c, &tours, &last.till, &x.at, None).1;
            if gap < travel + staff::REST {
                continue;
            }
            let mut ps: Vec<&Cut> = g.iter().map(|&k| &cuts[k]).collect();
            let before = paid_of(&ps);
            let (pen_b, hard_b) = whole_rules(&p, &ps);
            ps.push(x);
            if ps.windows(2).filter(|w| w[1].from - w[0].to >= p.split_min).count() > 1 {
                continue;
            }
            let blocks: Vec<Block> = ps.iter().map(|y| Block { key: format!("{}/{}", y.tour, y.range.start), from: y.from, to: y.to, from_stop: y.at.clone(), to_stop: y.till.clone() }).collect();
            if st::rules_of(&blocks).iter().any(|r| r.hard()) {
                continue;
            }
            // (the duty as a whole - its type, its length, its spread, its changes of bus - never
            // made to break a hard rule it kept; one it breaks less of is worth that much)
            let (pen_a, hard_a) = whole_rules(&p, &ps);
            if hard_a > hard_b {
                continue;
            }
            let mut penalty = pen_a - pen_b - HARD * (hard_b - hard_a) as f64;
            if gap >= p.split_min && no_rest(&last.till) && no_rest(&x.at) {
                penalty += HARD;
            }
            match pair_breaks(&p, &ps) {
                Some(more) => penalty += more,
                None => continue,
            }
            let delta = (paid_of(&ps) - before) as f64 * rate + penalty;
            if delta < alone && best.is_none_or(|b| delta < b.1) {
                best = Some((gi, delta));
            }
        }
        match best {
            Some((gi, _)) => groups[gi].push(ci),
            None => groups.push(vec![ci]),
        }
    }
    // pieces next to each other on one bus in one duty are one piece: no cut between them
    let mut starts: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        if kept(t) {
            continue;
        }
        let mine: Vec<usize> = (0..cuts.len()).filter(|&k| cuts[k].tour == i).collect();
        let mut s: Vec<usize> = Vec::new();
        for &k in mine.iter().skip(1) {
            let prev = mine.iter().copied().find(|&j| cuts[j].range.end == cuts[k].range.start);
            let together = prev.is_some_and(|j| groups.iter().any(|g| g.contains(&j) && g.contains(&k)));
            if !together {
                s.push(cuts[k].range.start);
            }
        }
        starts.push((i, s));
    }
    for lay in layouts.iter_mut() {
        for b in lay.blocks.iter_mut() {
            let Some(i) = tours.iter().position(|t| same(&t.line, &lay.line) && same(&t.tour, &b.tour)) else { continue };
            if let Some((_, s)) = starts.iter().find(|(k, _)| *k == i) {
                // (a relief on the way no duty begins at: the trip whole again)
                st::set_cuts_of(b, &tours[i], s);
            }
        }
    }
    // the duties of pieces of more than one run: joined
    let mut joined = locked_joins;
    for g in &groups {
        // (merged runs: the first piece of each run on its bus)
        let mut refs: Vec<PieceRef> = Vec::new();
        for &k in g {
            let x = &cuts[k];
            // (a kept block's cuts stay: each of its pieces is one of the duty's)
            let run_on = !kept(&tours[x.tour]) && g.iter().any(|&j| cuts[j].tour == x.tour && cuts[j].range.end == x.range.start);
            if !run_on {
                let t = &tours[x.tour];
                refs.push(PieceRef { line: t.line.clone(), tour: t.tour.clone(), first: TripRef::of(&t.trips[x.range.start]) });
            }
        }
        if refs.len() > 1 {
            joined.push(JoinedDuty { weekday: wd, pieces: refs });
        }
    }
    (layouts, joined)
}

/// Keep what Optimise made (`optimise`) for `weekday`: the blocks and joined duties replaced, the
/// duties shown in the order of their start again (`RowOrder`).
pub fn apply(c: &mut Company, weekday: u8, layouts: Vec<LineLayout>, joined: Vec<JoinedDuty>) {
    c.planning.layouts.retain(|l| l.weekday != weekday);
    c.planning.layouts.extend(layouts);
    c.planning.joined.retain(|j| j.weekday != weekday);
    c.planning.joined.extend(joined);
    c.planning.flow.sheet.rows.retain(|r| r.weekday != weekday);
}

/// Every English text the core gives RosterFlow's pages (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    out.extend(ISSUE_TEXTS);
    out.extend(ISSUE_TITLES);
    out.extend(PLACE_TEXTS);
    out.extend(PLACE_TITLES);
    out.extend(signing::TEXTS);
    out.extend(LENGTH_TEXTS);
    out.extend(LENGTH_TITLES);
    out.extend(ReliefTiming::LABELS);
    out.extend(Travel::LABELS);
    out.extend(Tri::LABELS);
    out.extend(TYPE_NAMES);
    out.push(OTHER);
    out.extend(CUSTOM_KINDS);
    out.extend([LINK_ONE_BUS, LINK_TWO]);
    out
}

/// A duty's own sign-on, sign-off and break points (testers, 2026-10-15).
pub mod signing;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod relief_tests;
#[cfg(test)]
mod optimise_tests;
#[cfg(test)]
mod lock_tests;

//! The duty card (Luc: "print your duty card, with the IBIS data, your stops and timing points
//! from RosterFlow"): a driver's duty of the day as the slip a receipt printer gives him at the
//! depot - who drives it, its codes (RosterFlow's duty number, the tours and blocks, the lines,
//! the day type, the depot, the bus), when and where it signs on and off with its length, its
//! driving time and its breaks, and piece by piece every trip with the codes the driver types
//! into the IBIS and the times at its timing points. Only what the planning knows is on it:
//! what is unknown stays off.
//!
//! The duty is RosterFlow's (`rosterflow::Board`): the pieces it joined, its number, its times
//! with the relief timing and the checks in, where it signs on and off. Without a board (no
//! timetable read for the day) it is the one piece the duty launcher shows. What the timetable
//! and the depot file say of a trip - its stops and their times, its IBIS codes - is looked up
//! by the caller (`Lookups`), the way the game reads them.

use super::duty_launch::{self as dl, DutyRef, Row, SheetBus};
use super::model::Company;
use super::network::PlannedTrip;
use super::plan::{weekday_of, DayPlan, Who};
use super::rosterflow::{Board, EvKind};
use crate::lines::{Direction, LineDesign};

/// A stop of a trip on the card: its map object and name, when the bus is there (minutes of
/// the day), whether the timetable times it itself (`hold`: the map wrote its minute) and
/// whether the card shows it as a timing point (`mark_timing`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CardStop {
    pub id: i64,
    pub name: String,
    pub arr: i32,
    pub dep: i32,
    pub hold: bool,
    pub timing: bool,
}

/// What the driver types into the IBIS for a trip (the game's own codes, `ibis_target`): the
/// line (three digits) and its letter's code, the route (the line with the route's two digits),
/// the destination code and its text, and the course (Kurs: the timetable's tour).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Ibis {
    pub line: String,
    pub suffix: Option<String>,
    pub route: Option<String>,
    pub dest: Option<String>,
    pub dest_text: String,
    pub kurs: String,
}

/// A line of a piece of work, in the order it is driven.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    SignOn { at: String, time: i32, relief: bool },
    /// A trip (a deadhead: `empty`), its stops (none known: empty) and its IBIS codes.
    Trip { dep: i32, arr: i32, line: String, from: String, to: String, empty: bool, kurs: String, stops: Vec<CardStop>, ibis: Option<Ibis> },
    Break { at: String, from: i32, to: i32 },
    SignOff { at: String, time: i32, relief: bool },
}

/// Between two pieces: the break, where it is taken, and the driver's travel in it (the kind's
/// English label, from, to).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Gap {
    pub from: i32,
    pub to: i32,
    pub at: String,
    pub travel: Vec<(&'static str, i32, i32)>,
}

/// A piece of work: a tour's duty (block `tour` of the timetable line `line`, shown `number`),
/// its bus and depot file, its items, and the break after it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Piece {
    pub line: String,
    pub number: String,
    pub tour: String,
    pub duty: usize,
    pub bus: Option<SheetBus>,
    pub hof: String,
    pub items: Vec<Item>,
    pub after: Option<Gap>,
}

/// Who drives the duty.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Driver {
    #[default]
    Nobody,
    /// The player (his profile's name).
    Player(String),
    /// One of the company's drivers: his staff number and name.
    Staff(u32, String),
    Agency,
    /// A player of "Play together" (his name).
    Guest(String),
}

/// The duty card.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Card {
    pub duty: DutyRef,
    pub company: String,
    pub company_name: String,
    pub date: String,
    /// `lines::DAY_GROUPS`: 0 Mon-Fri, 1 Saturday, 2 Sunday.
    pub day_group: usize,
    /// RosterFlow's duty number ("1003") and its type, where RosterFlow has the duty.
    pub number: Option<String>,
    pub kind: Option<String>,
    pub driver: Driver,
    pub lines: Vec<String>,
    pub sign_on: i32,
    pub sign_on_at: String,
    pub sign_off: i32,
    pub sign_off_at: String,
    pub driving: i32,
    pub breaks: i32,
    pub paid: Option<i32>,
    pub pieces: Vec<Piece>,
    /// A short check code of everything on it (`checksum`).
    pub code: String,
}

impl Card {
    /// The duty's length from sign on to sign off.
    pub fn length(&self) -> i32 {
        self.sign_off - self.sign_on
    }

    /// The depot files its pieces carry, each once.
    pub fn depots(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.pieces {
            if !p.hof.trim().is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(p.hof.trim())) {
                out.push(p.hof.trim().to_string());
            }
        }
        out
    }
}

/// What the timetable and the depot file say of a trip, looked up by the caller: its stops with
/// their times (`stops`: the trip, its timetable line, its course), its IBIS codes (`ibis`: the
/// trip, its timetable line, its course, its stops), and the line editor's line of a timetable line (`design`: its
/// RosterFlow timing points).
pub struct Lookups<'a> {
    pub stops: &'a dyn Fn(&PlannedTrip, &str, &str) -> Option<Vec<CardStop>>,
    pub ibis: &'a dyn Fn(&PlannedTrip, &str, &str, &[CardStop]) -> Option<Ibis>,
    pub design: &'a dyn Fn(&str) -> Option<LineDesign>,
}

impl Lookups<'_> {
    /// Nothing looked up (the stops and the codes stay off the card).
    pub fn none() -> Lookups<'static> {
        Lookups { stops: &|_, _, _| None, ibis: &|_, _, _, _| None, design: &|_| None }
    }
}

/// The direction of `l` a trip of these stops follows: the one that has the most of them one
/// after another in the trip's order, then the most of them (by the map object, else by the
/// name) - both ways of a line call at the same stops.
fn direction_of<'a>(l: &'a LineDesign, stops: &[CardStop]) -> Option<(usize, &'a Direction)> {
    let key = |s: &str| s.trim().to_lowercase();
    let at = |d: &Direction, s: &CardStop| d.stops.iter().position(|x| (s.id != 0 && x.id == s.id) || (!s.name.trim().is_empty() && key(&x.name) == key(&s.name)));
    let score = |d: &Direction| {
        let places: Vec<Option<usize>> = stops.iter().map(|s| at(d, s)).collect();
        let ordered = places.windows(2).filter(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if a < b)).count();
        (ordered, places.iter().flatten().count())
    };
    l.directions.iter().enumerate().filter(|(_, d)| d.stops.len() >= 2).map(|(k, d)| (k, d, score(d))).filter(|x| x.2 .1 > 0).max_by_key(|x| (x.2, std::cmp::Reverse(x.0))).map(|(k, d, _)| (k, d))
}

/// Which stops of a trip are its timing points: the first and the last always; the others as
/// RosterFlow's Timeplan chose them for the line (`timeplan::RunPlan::is_timepoint` - every stop
/// of a direction where none was chosen); for a line RosterFlow never planned, the stops the
/// timetable times itself (`hold`).
pub fn mark_timing(stops: &mut [CardStop], design: Option<&LineDesign>) {
    let n = stops.len();
    let chosen: Option<Vec<(i64, String)>> = design.and_then(|l| {
        let (k, d) = direction_of(l, stops)?;
        let plan = l.timeplan.clone().unwrap_or_default();
        Some((0..d.stops.len()).filter(|&i| plan.is_timepoint(k, d, i)).map(|i| (d.stops[i].id, d.stops[i].name.trim().to_lowercase())).collect())
    });
    for (i, s) in stops.iter_mut().enumerate() {
        s.timing = i == 0
            || i + 1 == n
            || match &chosen {
                Some(c) => c.iter().any(|(id, name)| (s.id != 0 && *id == s.id) || (!name.is_empty() && *name == s.name.trim().to_lowercase())),
                None => s.hold,
            };
    }
}

/// The part of a trip's stops a piece drives: from the relief point on, or up to it.
fn part_of(x: &PlannedTrip, stops: Vec<CardStop>) -> Vec<CardStop> {
    match &x.part {
        Some(p) if p.second => stops.into_iter().skip(p.stop).collect(),
        Some(p) => stops.into_iter().take(p.stop + 1).collect(),
        None => stops,
    }
}

/// FNV-1a of the card's content, as eight hex digits in two groups ("3FA2-91C0"): the same duty
/// printed twice carries the same code, a duty changed since another.
pub fn checksum(c: &Card) -> String {
    let mut h: u32 = 0x811c_9dc5;
    let mut eat = |s: &str| {
        for b in s.bytes().chain(std::iter::once(0)) {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
    };
    eat(&c.company);
    eat(&c.date);
    eat(c.number.as_deref().unwrap_or(""));
    eat(&format!("{}|{}|{}|{}", c.sign_on, c.sign_on_at, c.sign_off, c.sign_off_at));
    for p in &c.pieces {
        eat(&format!("{}|{}|{}", p.line, p.tour, p.duty));
        eat(p.bus.as_ref().map(|b| b.number.as_str()).unwrap_or(""));
        for it in &p.items {
            match it {
                Item::Trip { dep, arr, line, from, to, kurs, stops, ibis, .. } => {
                    eat(&format!("T{dep}|{arr}|{line}|{from}|{to}|{kurs}"));
                    for s in stops {
                        eat(&format!("{}|{}", s.name, s.dep));
                    }
                    if let Some(i) = ibis {
                        eat(&format!("{}|{:?}|{:?}", i.line, i.route, i.dest));
                    }
                }
                Item::Break { at, from, to } => eat(&format!("B{at}|{from}|{to}")),
                Item::SignOn { at, time, .. } => eat(&format!("N{at}|{time}")),
                Item::SignOff { at, time, .. } => eat(&format!("F{at}|{time}")),
            }
        }
    }
    format!("{:04X}-{:04X}", h >> 16, h & 0xffff)
}

/// The day group of a date (`lines::DAY_GROUPS`).
pub fn day_group(date: &str) -> usize {
    match weekday_of(date) {
        5 => 1,
        6 => 2,
        _ => 0,
    }
}

/// The card of duty `r` in `p` (the plan of its day), RosterFlow's `board` of that day where
/// there is one, at minute `now` of the company's clock; `player` the player's name. None: the
/// plan no longer has the duty.
pub fn card(c: &Company, p: &DayPlan, board: Option<&Board>, r: &DutyRef, now: i64, player: &str, look: &Lookups) -> Option<Card> {
    let (ti, k) = r.find(p)?;
    // the duty's pieces: RosterFlow's duty with this piece in it, else the piece alone
    let in_plan = |line: &str, tour: &str| p.tours.iter().position(|t| t.tour.line.eq_ignore_ascii_case(line) && t.tour.tour.trim() == tour.trim());
    let mut shaped = None;
    let mut pieces: Vec<(usize, usize)> = vec![(ti, k)];
    if let Some(b) = board {
        let bt = b.tours.iter().position(|t| t.line.eq_ignore_ascii_case(&r.line) && t.tour.trim() == r.tour.trim());
        if let Some(di) = bt.and_then(|bt| super::studio::duty_of(&b.duties, bt, k)) {
            let mapped: Vec<(usize, usize)> = b.duties[di].pieces.iter().filter_map(|x| b.tours.get(x.tour).and_then(|t| in_plan(&t.line, &t.tour)).map(|pt| (pt, x.duty))).collect();
            if mapped.contains(&(ti, k)) {
                pieces = mapped;
                shaped = b.shaped.iter().enumerate().find(|(_, s)| s.duty == Some(di));
            }
        }
    }
    pieces.sort_by_key(|&(t, d)| p.tours[t].duties.get(d).map(|x| x.from).unwrap_or(0));
    let mut card = Card { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), date: p.date.clone(), day_group: day_group(&p.date), ..Default::default() };
    let mut designs: Vec<(String, Option<LineDesign>)> = Vec::new();
    for &(pt, pk) in &pieces {
        let Some(pr) = DutyRef::of(p, pt, pk) else { continue };
        let sh = dl::sheet(c, p, &pr, now, &[]);
        let t = &p.tours[pt];
        let mut piece = Piece { line: t.tour.line.clone(), number: t.tour.number.clone(), tour: t.tour.tour.clone(), duty: pk, bus: sh.bus.clone(), hof: sh.hof.clone(), ..Default::default() };
        if !designs.iter().any(|d| d.0 == piece.line) {
            designs.push((piece.line.clone(), (look.design)(&piece.line)));
        }
        let design = designs.iter().find(|d| d.0 == piece.line).and_then(|d| d.1.as_ref());
        let mut planned = sh.planned.iter();
        for row in &sh.rows {
            piece.items.push(match row {
                Row::SignOn { at, time, relief } => Item::SignOn { at: at.clone(), time: *time, relief: *relief },
                Row::SignOff { at, time, relief } => Item::SignOff { at: at.clone(), time: *time, relief: *relief },
                Row::Break { at, from, to } => Item::Break { at: at.clone(), from: *from, to: *to },
                Row::Trip { dep, arr, line, from, to, empty, .. } => {
                    let x = planned.next();
                    let kurs = x.map(|x| if x.origin.trim().is_empty() { t.tour.tour.trim().to_string() } else { x.origin.trim().to_string() }).unwrap_or_default();
                    let (stops, ibis) = match x.filter(|_| !*empty) {
                        Some(x) => {
                            // (the codes by the whole trip's stops: the route it follows)
                            let whole = (look.stops)(x, &t.tour.line, &kurs);
                            let ibis = (look.ibis)(x, &t.tour.line, &kurs, whole.as_deref().unwrap_or(&[]));
                            let mut stops = whole.map(|s| part_of(x, s)).unwrap_or_default();
                            mark_timing(&mut stops, design);
                            (stops, ibis)
                        }
                        None => (Vec::new(), None),
                    };
                    Item::Trip { dep: *dep, arr: *arr, line: line.trim().to_string(), from: from.clone(), to: to.clone(), empty: *empty, kurs, stops, ibis }
                }
            });
        }
        for l in &sh.lines {
            if !l.trim().is_empty() && !card.lines.contains(l) {
                card.lines.push(l.clone());
            }
        }
        card.pieces.push(piece);
    }
    let first_at = |items: &[Item]| items.iter().find_map(|i| if let Item::SignOn { at, time, .. } = i { Some((at.clone(), *time)) } else { None });
    let last_at = |items: &[Item]| items.iter().rev().find_map(|i| if let Item::SignOff { at, time, .. } = i { Some((at.clone(), *time)) } else { None });
    let (on_at, on) = card.pieces.first().and_then(|x| first_at(&x.items)).unwrap_or_default();
    let (off_at, off) = card.pieces.last().and_then(|x| last_at(&x.items)).unwrap_or_default();
    // the breaks between the pieces: where they are taken, the driver's travel in them
    let n = card.pieces.len();
    for i in 0..n.saturating_sub(1) {
        let (Some((end_at, end)), Some((_, next))) = (last_at(&card.pieces[i].items), first_at(&card.pieces[i + 1].items)) else { continue };
        let at = shaped.and_then(|(_, s)| s.points.breaks.iter().find(|b| b.0 == i).map(|b| b.1.clone())).unwrap_or(end_at);
        let travel = shaped
            .map(|(_, s)| s.events.iter().filter(|e| e.from >= end && e.to <= next).filter_map(|e| if let EvKind::Travel(t) = e.kind { Some((t.label(), e.from, e.to)) } else { None }).collect())
            .unwrap_or_default();
        card.pieces[i].after = Some(Gap { from: end, to: next, at, travel });
    }
    let stands: i32 = card.pieces.iter().flat_map(|x| &x.items).map(|i| if let Item::Break { from, to, .. } = i { to - from } else { 0 }).sum::<i32>() + card.pieces.iter().filter_map(|x| x.after.as_ref()).map(|g| g.to - g.from).sum::<i32>();
    let driven: i32 = card.pieces.iter().flat_map(|x| &x.items).map(|i| if let Item::Trip { dep, arr, .. } = i { arr - dep } else { 0 }).sum();
    match shaped {
        // (RosterFlow's: the relief timing and the checks in, its signing points)
        Some((row, s)) => {
            card.number = Some(s.label(row));
            card.kind = Some(s.kind_name(&c.planning.flow.prefs).to_string());
            card.sign_on = s.start;
            card.sign_off = s.end;
            card.sign_on_at = s.points.on.clone().unwrap_or(on_at);
            card.sign_off_at = s.points.off.clone().unwrap_or(off_at);
            card.driving = s.driving;
            card.breaks = s.plan.breaks.iter().map(|b| b.to - b.from).sum::<i32>().max(if s.plan.breaks.is_empty() { stands } else { 0 });
            card.paid = Some(s.paid);
        }
        None => {
            (card.sign_on, card.sign_on_at, card.sign_off, card.sign_off_at) = (on, on_at, off, off_at);
            card.driving = driven;
            card.breaks = stands;
        }
    }
    card.driver = match p.tours[ti].duties[k].who {
        _ if p.tours[ti].by_player => Driver::Player(player.trim().to_string()),
        None => Driver::Nobody,
        Some(Who::Player) => Driver::Player(player.trim().to_string()),
        Some(Who::Staff(id)) => Driver::Staff(id, c.employee(id).map(|e| e.name.clone()).unwrap_or_default()),
        Some(Who::Agency) => Driver::Agency,
        Some(Who::Guest(g)) => Driver::Guest(super::together::guest_name(c, g)),
    };
    card.code = checksum(&card);
    Some(card)
}

#[cfg(test)]
mod tests {
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::{CompanyLine, GameMode};
    use super::super::network::TourOfDay;
    use super::super::plan::{day_plan, set_bus, set_driver, weekday_of};
    use super::super::{found, Founding};
    use super::*;
    use super::super::timeplan::{RunPlan, Timepoints};
    use crate::lines::StopRef;

    fn trip(name: &str, dep: i32, arr: i32, from: &str, to: &str, empty: bool) -> PlannedTrip {
        PlannedTrip { name: name.into(), line: if empty { String::new() } else { "5".into() }, from: from.into(), to: to.into(), dep, arr, km: 8.0, stops: if empty { 0 } else { 4 }, empty, origin: String::new(), last: String::new(), part: None }
    }

    /// Line 5's tour 2: a depot run, A-B, B-A, A-B and the run back; the player on it with the
    /// company's bus.
    fn setup() -> (Company, DayPlan) {
        let mut c = found(&Founding { name: "Stadtbus".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.depot = "Grundorf.hof".into();
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c.cash += 5_000_000_00;
        market::buy_new(&mut c, &MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
        let tours = vec![TourOfDay {
            line: "Linie5".into(),
            number: "5".into(),
            tour: "2".into(),
            ai_group: String::new(),
            trips: vec![trip("Depot-A", 350, 358, "Depot", "A", true), trip("5 A-B", 360, 390, "A", "B", false), trip("5 B-A", 400, 430, "B", "A", false), trip("5 A-B", 440, 470, "A", "B", false), trip("A-Depot", 472, 480, "A", "Depot", true)],
            unplanned: false,
            cuts: None,
        }];
        let wd = weekday_of(&c.date);
        let id = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "2", Some(id));
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Player));
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        (c, p)
    }

    /// The four stops of a trip leaving at `dep` (A, Markt, Kirche, B or the other way round).
    fn stops_of(x: &PlannedTrip) -> Vec<CardStop> {
        let names = if x.from == "A" { ["A", "Markt", "Kirche", "B"] } else { ["B", "Kirche", "Markt", "A"] };
        let ids = if x.from == "A" { [1, 2, 3, 4] } else { [4, 3, 2, 1] };
        (0..4).map(|i| CardStop { id: ids[i], name: names[i].into(), arr: x.dep + 10 * i as i32, dep: x.dep + 10 * i as i32, hold: i == 1, timing: false }).collect()
    }

    fn design() -> LineDesign {
        let stop = |id: i64, name: &str| StopRef { id, name: name.into(), ..Default::default() };
        let mut l = LineDesign { number: "5".into(), ..Default::default() };
        l.directions = vec![
            Direction { stops: vec![stop(1, "A"), stop(2, "Markt"), stop(3, "Kirche"), stop(4, "B")], ..Default::default() },
            Direction { stops: vec![stop(4, "B"), stop(3, "Kirche"), stop(2, "Markt"), stop(1, "A")], ..Default::default() },
        ];
        // RosterFlow's Timeplan: Kirche a timing point outbound; inbound none chosen (every stop)
        l.timeplan = Some(RunPlan { sets: Vec::new(), timepoints: vec![Timepoints { dir: 0, stops: vec![1, 3, 4] }] });
        l
    }

    #[test]
    fn the_duty_card_has_the_codes_the_timing_points_and_the_sign_on_and_off() {
        let (c, p) = setup();
        let r = DutyRef::of(&p, 0, 0).unwrap();
        let stops = |x: &PlannedTrip, line: &str, kurs: &str| {
            assert_eq!((line, kurs), ("Linie5", "2"));
            Some(stops_of(x))
        };
        let ibis = |x: &PlannedTrip, _: &str, kurs: &str, _: &[CardStop]| Some(Ibis { line: "005".into(), route: Some(if x.from == "A" { "501" } else { "502" }.into()), dest: Some("105".into()), dest_text: x.to.clone(), kurs: kurs.into(), suffix: None });
        let design = |line: &str| (line == "Linie5").then(design);
        let look = Lookups { stops: &stops, ibis: &ibis, design: &design };
        let card = card(&c, &p, None, &r, super::super::clock::moment(&c.date, 300), "Luc", &look).unwrap();
        // the codes: the company, the day (a Monday), the driver, the line, the tour, the bus
        assert_eq!((card.company_name.as_str(), card.date.as_str(), card.day_group), ("Stadtbus", "2024-03-04", 0));
        assert_eq!(card.driver, Driver::Player("Luc".into()));
        assert_eq!(card.lines, vec!["5".to_string()]);
        assert_eq!(card.pieces.len(), 1);
        let piece = &card.pieces[0];
        assert_eq!((piece.number.as_str(), piece.tour.as_str(), piece.duty), ("5", "2", 0));
        assert_eq!(piece.bus.as_ref().map(|b| b.number.clone()), Some(c.fleet[0].number.clone()));
        assert_eq!(card.depots(), vec!["Grundorf.hof".to_string()]);
        // signing on and off, the length, the driving and the stands (the 2 minutes before the
        // run back are none)
        assert_eq!((card.sign_on, card.sign_on_at.as_str(), card.sign_off, card.sign_off_at.as_str()), (350, "Depot", 480, "Depot"));
        assert_eq!((card.length(), card.driving, card.breaks), (130, 8 + 30 + 30 + 30 + 8, 10 + 10));
        assert!(matches!(piece.items.first(), Some(Item::SignOn { time: 350, relief: false, .. })));
        assert!(matches!(piece.items.last(), Some(Item::SignOff { time: 480, relief: false, .. })));
        // every trip with passengers has its IBIS codes and stops; the deadheads none
        let trips: Vec<&Item> = piece.items.iter().filter(|i| matches!(i, Item::Trip { .. })).collect();
        assert_eq!(trips.len(), 5);
        assert!(matches!(trips[0], Item::Trip { empty: true, stops, ibis: None, .. } if stops.is_empty()));
        let Item::Trip { ibis: Some(i), stops, kurs, .. } = trips[1] else { panic!("{:?}", trips[1]) };
        assert_eq!((i.line.as_str(), i.route.as_deref(), i.dest.as_deref(), i.kurs.as_str(), kurs.as_str()), ("005", Some("501"), Some("105"), "2", "2"));
        // the timing points: outbound as RosterFlow chose them (Markt not, Kirche yes)
        assert_eq!(stops.iter().map(|s| (s.name.as_str(), s.timing)).collect::<Vec<_>>(), vec![("A", true), ("Markt", false), ("Kirche", true), ("B", true)]);
        // inbound none chosen: every stop is one
        let Item::Trip { stops, ibis: Some(i), .. } = trips[2] else { panic!() };
        assert!(stops.iter().all(|s| s.timing));
        assert_eq!(i.route.as_deref(), Some("502"));
        // a line RosterFlow never planned: the stops the timetable times itself, and the ends
        let mut plain = stops_of(&trip("x", 360, 390, "A", "B", false));
        mark_timing(&mut plain, None);
        assert_eq!(plain.iter().map(|s| s.timing).collect::<Vec<_>>(), vec![true, true, false, true]);
        // the check code: the same duty the same code, a changed one another
        assert_eq!(card.code.len(), 9);
        let again = super::card(&c, &p, None, &r, 0, "Luc", &look).unwrap();
        assert_eq!(again.code, card.code);
        let none = super::card(&c, &p, None, &r, 0, "Luc", &Lookups::none()).unwrap();
        assert_ne!(none.code, card.code);
        // a duty the plan no longer has: no card
        assert!(super::card(&c, &p, None, &DutyRef { tour: "9".into(), ..r }, 0, "Luc", &look).is_none());
    }
}

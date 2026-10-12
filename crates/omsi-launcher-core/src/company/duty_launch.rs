//! The company's duty launcher (Luc: "a launcher from the bus company that works ONLY with
//! the data given in the bus company"): a duty of the company's planning as it is driven,
//! fixed - its day, its trips in order with their breaks, deadheads and relief points, the
//! bus the planning gives its tour, the depot file, the map - and what keeps it from being
//! driven now (`Hitch`). Nothing of it is chosen: the game is started with exactly this duty
//! (`Start`), and what goes wrong is put right in the planning, not by picking another duty,
//! bus, map or day.

use super::dates;
use super::model::Company;
use super::network::PlannedTrip;
use super::plan::{BusOf, DayPlan, DayTour, Problem, Who};
use crate::{TripInfo, TripRun};
use serde::{Deserialize, Serialize};

/// Which duty the launcher shows: its day, its line and tour as the plan has them, and the
/// duty of that tour (0 the first). Kept by these, not by places in a plan, so that the plan
/// made again (the company changed) still finds it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct DutyRef {
    pub date: String,
    pub line: String,
    pub tour: String,
    pub duty: usize,
}

impl DutyRef {
    pub fn of(p: &DayPlan, ti: usize, k: usize) -> Option<DutyRef> {
        let t = p.tours.get(ti)?;
        t.duties.get(k)?;
        Some(DutyRef { date: p.date.clone(), line: t.tour.line.clone(), tour: t.tour.tour.clone(), duty: k })
    }

    /// Its tour and duty in `p` (None: the plan no longer has it).
    pub fn find(&self, p: &DayPlan) -> Option<(usize, usize)> {
        if p.date != self.date {
            return None;
        }
        let ti = p.tours.iter().position(|t| t.tour.line.eq_ignore_ascii_case(&self.line) && t.tour.tour.trim() == self.tour.trim())?;
        (self.duty < p.tours[ti].duties.len()).then_some((ti, self.duty))
    }
}

/// A row of the duty's sheet, in the order it is driven.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Signing on (`relief`: taking the bus over from the driver before, at `at`).
    SignOn { at: String, time: i32, relief: bool },
    /// A trip: with passengers, or a deadhead (a depot or empty run, `empty`).
    Trip { dep: i32, arr: i32, line: String, from: String, to: String, empty: bool, name: String },
    /// Standing between two trips at `at`, from `from` to `to` (minutes of the day).
    Break { at: String, from: i32, to: i32 },
    /// Signing off (`relief`: handing the bus to the next driver at `at`).
    SignOff { at: String, time: i32, relief: bool },
}

/// The bus the planning gives the duty's tour.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetBus {
    pub id: u32,
    pub file: String,
    pub name: String,
    pub number: String,
    pub plate: String,
    /// The livery it runs in: the company's house livery, else its own.
    pub paint: String,
}

/// What keeps the duty from being driven now. Each is put right in the planning.
#[derive(Clone, Debug, PartialEq)]
pub enum Hitch {
    /// The plan no longer has the duty (the timetable or the planning changed).
    Gone,
    /// No bus planned for the tour (why, when the planning knows).
    NoBus(Option<Problem>),
    /// A rental bus runs the tour: the company has no bus of its own for it to start with.
    Rental,
    /// The planned bus cannot run that day (in the workshop, gone, broken down...).
    Bus(Problem),
    /// The planned bus has no bus file to start the game with.
    NoBusFile,
    /// The planned bus is not installed (its file).
    NotInstalled(String),
    /// Nobody is planned on the duty.
    NoDriver,
    /// Someone else is planned on the duty (their name).
    NotYours(String),
    /// The player drove it already.
    Driven,
    /// Its day or its time is over on the company's clock.
    Over,
    /// The day's timetable no longer has its trips as the plan has them.
    Timetable,
}

impl Hitch {
    /// What is wrong, and what to do about it (English: the keys of the translations).
    pub fn words(&self) -> (&'static str, &'static str) {
        match self {
            Hitch::Gone => ("This duty is no longer in the planning", "The timetable or the planning changed since. Choose the duty again in RosterFlow."),
            Hitch::NoBus(_) => ("No bus is planned for this duty", "Give its tour a bus of the fleet in RosterFlow."),
            Hitch::Rental => ("A rental bus runs this tour", "The company has no bus of its own on it to start with: give the tour a bus of the fleet in RosterFlow."),
            Hitch::Bus(_) => ("The planned bus cannot run this day", "Give the tour another bus of the fleet in RosterFlow."),
            Hitch::NoBusFile => ("The planned bus has no bus file", "Give the tour another bus of the fleet in RosterFlow."),
            Hitch::NotInstalled(_) => ("The planned bus is not installed", "Install it again, or give the tour another bus of the fleet in RosterFlow."),
            Hitch::NoDriver => ("Nobody is planned on this duty", "Plan yourself on it in RosterFlow (\"You\") to drive it."),
            Hitch::NotYours(_) => ("Another driver is planned on this duty", "Plan yourself on it in RosterFlow (\"You\") to drive it."),
            Hitch::Driven => ("You drove this duty already", "Its trips are reported to the company. Choose another duty of yours in RosterFlow."),
            Hitch::Over => ("The time of this duty has passed", "The company's clock is past its end. Choose a later duty in RosterFlow."),
            Hitch::Timetable => ("The timetable changed", "The day's timetable no longer has this duty's trips as planned. Plan the day again in RosterFlow."),
        }
    }
}

/// A duty of the company as the launcher shows it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sheet {
    pub duty: DutyRef,
    pub company: String,
    pub company_name: String,
    pub map: String,
    pub map_name: String,
    /// The line's number shown, and the timetable's tour the game drives (that of its first
    /// trip with passengers - a block of the studio may hold trips of several).
    pub number: String,
    pub game_tour: String,
    /// The numbers of the lines its trips show (in the order they first come).
    pub lines: Vec<String>,
    /// The first departure, the last arrival (minutes of the day).
    pub from: i32,
    pub to: i32,
    pub rows: Vec<Row>,
    /// The trips with passengers, and the deadheads.
    pub trips: usize,
    pub deadheads: usize,
    pub bus: Option<SheetBus>,
    /// The depot file: the line's own, else the company's.
    pub hof: String,
    /// Who the plan has on it.
    pub who: Option<Who>,
    pub hitches: Vec<Hitch>,
    /// The duty's planned trips (for the game's legs, `start`).
    pub planned: Vec<PlannedTrip>,
}

impl Sheet {
    pub fn startable(&self) -> bool {
        self.hitches.is_empty()
    }

    /// The trip files of its trips, in order (the route map draws them).
    pub fn trip_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in &self.planned {
            if !t.name.trim().is_empty() && out.last() != Some(&t.name) {
                out.push(t.name.clone());
            }
        }
        out
    }
}

/// Shorter stands than this are not shown as a break of their own (minutes).
const BREAK_MIN: i32 = 3;

/// The sheet of duty `r` in `p` (the plan of its day), at minute `now` of the company's clock
/// (`clock::now`), with the player's trip reports `trips` (to see whether he drove it).
pub fn sheet(c: &Company, p: &DayPlan, r: &DutyRef, now: i64, trips: &[TripRun]) -> Sheet {
    let mut s = Sheet { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), map: c.map.clone(), map_name: c.map_name.clone(), ..Default::default() };
    let Some((ti, k)) = r.find(p) else {
        s.hitches.push(Hitch::Gone);
        return s;
    };
    let t = &p.tours[ti];
    let d = &t.duties[k];
    let Some(planned) = t.tour.trips.get(d.start..d.end).filter(|x| !x.is_empty()) else {
        s.hitches.push(Hitch::Gone);
        return s;
    };
    s.planned = planned.to_vec();
    s.number = t.tour.number.clone();
    s.game_tour = planned.iter().find(|x| x.counts() && !x.origin.trim().is_empty()).map(|x| x.origin.trim().to_string()).unwrap_or_else(|| t.tour.tour.clone());
    s.from = d.from;
    s.to = d.to;
    s.who = d.who;
    for x in planned {
        if !x.empty && !x.line.trim().is_empty() && !s.lines.contains(&x.line) {
            s.lines.push(x.line.clone());
        }
    }
    if s.lines.is_empty() {
        s.lines.push(t.tour.number.clone());
    }
    // the rows: signing on, the trips with the stands between them, signing off
    let first = &planned[0];
    s.rows.push(Row::SignOn { at: first.from.clone(), time: first.dep, relief: k > 0 });
    let mut prev: Option<&PlannedTrip> = None;
    for x in planned {
        if let Some(b) = prev {
            if x.dep - b.arr >= BREAK_MIN {
                s.rows.push(Row::Break { at: b.end().to_string(), from: b.arr, to: x.dep });
            }
        }
        s.rows.push(Row::Trip { dep: x.dep, arr: x.arr, line: x.line.clone(), from: x.from.clone(), to: x.to.clone(), empty: x.empty, name: x.name.clone() });
        if x.empty {
            s.deadheads += 1;
        } else {
            s.trips += 1;
        }
        prev = Some(x);
    }
    let last = planned.last().unwrap_or(first);
    s.rows.push(Row::SignOff { at: last.end().to_string(), time: d.to.max(last.arr), relief: k + 1 < t.duties.len() });
    // the depot file: the line's own, else the company's
    s.hof = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&t.tour.line)).map(|x| x.hof_or(&c.depot).to_string()).unwrap_or_else(|| c.depot.clone());
    // the bus
    match t.bus {
        None => s.hitches.push(Hitch::NoBus(t.bus_problem)),
        Some(BusOf::Rental) => s.hitches.push(Hitch::Rental),
        Some(BusOf::Own(id)) => match c.vehicle(id) {
            None => s.hitches.push(Hitch::Bus(Problem::Gone)),
            Some(v) => {
                s.bus = Some(SheetBus {
                    id,
                    file: v.bus.clone(),
                    name: v.name.clone(),
                    number: v.number.clone(),
                    plate: v.plate.clone(),
                    paint: v.house_livery.clone().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| v.livery.clone()),
                });
                if !v.held_on(&p.date) {
                    s.hitches.push(Hitch::Bus(Problem::Gone));
                } else if v.in_workshop(&p.date) {
                    s.hitches.push(Hitch::Bus(Problem::Workshop));
                } else if v.bus.trim().is_empty() {
                    s.hitches.push(Hitch::NoBusFile);
                }
            }
        },
    }
    // the driver
    match d.who {
        Some(Who::Player) => {}
        None => s.hitches.push(Hitch::NoDriver),
        Some(Who::Staff(id)) => s.hitches.push(Hitch::NotYours(c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()))),
        Some(Who::Agency) => s.hitches.push(Hitch::NotYours("Agency driver".into())),
    }
    // driven already, or over
    if t.by_player || (p.today && driven(c, t, planned, trips)) {
        s.hitches.push(Hitch::Driven);
    }
    let end = super::clock::moment(&p.date, s.to as i64);
    if dates::between(&c.date, &p.date) < 0 || now >= end {
        s.hitches.push(Hitch::Over);
    }
    s
}

/// The player reported trips of the duty since the company's last close (`trips_seen`): a trip
/// of its line and the game's tour leaving at the minute one of its trips leaves.
fn driven(c: &Company, t: &DayTour, planned: &[PlannedTrip], trips: &[TripRun]) -> bool {
    trips.iter().filter(|r| r.time > c.trips_seen && !r.free && same_map(&r.map, &c.map) && r.line.trim().eq_ignore_ascii_case(t.tour.number.trim())).any(|r| {
        let dep = (r.departure / 60.0).round() as i32;
        planned.iter().any(|x| x.counts() && x.dep == dep && (if x.origin.trim().is_empty() { &t.tour.tour } else { &x.origin }).trim() == r.tour.trim())
    })
}

fn same_map(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    a.trim().is_empty() || n(a) == n(b) || n(a).ends_with(&n(b)) || n(b).ends_with(&n(a))
}

/// A duty's trips (`planned`, as the plan has them) in its tour of the day's timetable
/// (`trips`): the first one's place as the game counts it (1 the first), the minute it leaves,
/// and how many there are. The plan has the trips in the order they leave, an own line's with
/// the depot runs its timetable lacks: each is found by its name and the minute it leaves, the
/// depot runs are not. None: none is found, or a trip with passengers is missing or out of
/// order (the timetable changed since the plan was made).
pub fn timetable_legs(planned: &[PlannedTrip], trips: &[TripInfo]) -> Option<(usize, i32, usize)> {
    let mut found: Vec<(usize, i32)> = Vec::new();
    for x in planned {
        match trips.iter().find(|y| y.name == x.name && (y.departure / 60.0).round() as i32 == x.dep) {
            Some(y) => found.push((y.index, x.dep)),
            None if x.empty => {}
            None => return None,
        }
    }
    let (first, time) = *found.first()?;
    if found.iter().enumerate().any(|(n, f)| f.0 != first + n) {
        return None;
    }
    Some((first, time, found.len()))
}

/// What the game is started with for a duty: everything fixed by the company.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Start {
    pub company: String,
    pub map: String,
    /// The day and the time the game starts at: the bus's lead before the first departure.
    pub date: String,
    pub time: i32,
    pub line: String,
    pub tour: String,
    /// The first trip (its place in the tour) and the duty's part as `--duty-leg` takes it.
    pub first: usize,
    pub legs: Vec<String>,
    pub bus: String,
    pub paint: String,
    pub number: String,
    pub plate: String,
    pub hof: String,
}

/// The start of a sheet's duty, with the day's timetable `lines` and the bus's `lead` before
/// the first departure (minutes): Err the hitch that keeps it.
pub fn start(s: &Sheet, lines: &[crate::LineInfo], lead: i64) -> Result<Start, Hitch> {
    if let Some(h) = s.hitches.first() {
        return Err(h.clone());
    }
    let Some(bus) = s.bus.as_ref() else { return Err(Hitch::NoBus(None)) };
    let line = s.duty.line.clone();
    let legs = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&line)).and_then(|l| l.tours.iter().find(|t| t.number.trim() == s.game_tour.trim())).and_then(|t| timetable_legs(&s.planned, &t.trips));
    let Some((first, dep, count)) = legs else { return Err(Hitch::Timetable) };
    let (date, time) = crate::lead_start(&s.duty.date, dep, lead);
    Ok(Start {
        company: s.company.clone(),
        map: s.map.clone(),
        date,
        time,
        line: line.clone(),
        tour: s.game_tour.clone(),
        first,
        legs: vec![format!("{line}|{}|{first}|{count}", s.game_tour)],
        bus: bus.file.clone(),
        paint: bus.paint.clone(),
        number: bus.number.clone(),
        plate: bus.plate.clone(),
        hof: s.hof.clone(),
    })
}

impl Start {
    /// The duty as the backend takes it, made of `base` (the player's game options: traffic,
    /// the engine running, the profile) with everything of the duty the company's - and the
    /// weather chosen (empty: as the map has it).
    pub fn duty(&self, base: crate::Duty, weather: &str) -> crate::Duty {
        let mut d = base;
        d.map = self.map.clone();
        d.bus = self.bus.clone();
        d.paint = Some(self.paint.clone()).filter(|p| !p.trim().is_empty());
        d.plate = Some(self.plate.clone()).filter(|p| !p.trim().is_empty());
        d.number = Some(self.number.clone()).filter(|n| !n.trim().is_empty());
        d.hof = Some(self.hof.clone()).filter(|h| !h.trim().is_empty());
        // (the entry point nearest to the duty's first stop)
        d.entry = Some(-1);
        d.line = Some(self.line.clone());
        d.tour = Some(self.tour.clone());
        d.trip = Some(self.first.to_string());
        d.whole_tour = true;
        d.legs = self.legs.clone();
        d.free_line = None;
        d.time = format!("{:02}:{:02}", self.time / 60, self.time % 60);
        d.date = Some(self.date.clone());
        d.weather = Some(weather.to_string()).filter(|w| !w.trim().is_empty());
        // (the company's service: its passengers and its buses on the road, the season by
        // the date, alone - not in someone's session)
        d.passengers = Some(true);
        d.schedule = Some(true);
        d.on_foot = Some(false);
        d.lan = Some("off".into());
        d.lan_name = None;
        d.season = None;
        d.tutorial = None;
        d.situation = None;
        d.editor = false;
        d.company = Some(self.company.clone());
        d
    }
}

#[cfg(test)]
mod tests {
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::{CompanyLine, Difficulty};
    use super::super::network::TourOfDay;
    use super::super::plan::{day_plan, set_bus, set_driver, weekday_of};
    use super::super::{found, Founding};
    use super::*;

    fn trip(name: &str, dep: i32, arr: i32, from: &str, to: &str, empty: bool) -> PlannedTrip {
        PlannedTrip { name: name.into(), line: if empty { String::new() } else { "5".into() }, from: from.into(), to: to.into(), dep, arr, km: 8.0, stops: if empty { 0 } else { 10 }, empty, origin: String::new(), last: String::new() }
    }

    /// A company with one bus and line 5, whose tour 2 is a depot run, three trips and the run
    /// back, the player planned on it with the bus.
    fn setup() -> (Company, Vec<TourOfDay>) {
        let mut c = found(&Founding { name: "Stadtbus".into(), difficulty: Difficulty::Easy, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
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
        (c, tours)
    }

    fn info(name: &str, index: usize, dep: i32) -> TripInfo {
        TripInfo { name: name.into(), index, line: "5".into(), from: "A".into(), terminus: "B".into(), departure: dep as f64 * 60.0, arrival: (dep + 30) as f64 * 60.0, stops: Vec::new(), km: 8.0 }
    }

    fn timetable() -> Vec<crate::LineInfo> {
        vec![crate::LineInfo {
            name: "Linie5".into(),
            user_allowed: true,
            termini: Vec::new(),
            tours: vec![crate::TourInfo { number: "2".into(), ai_group: String::new(), first: 360.0 * 60.0, last: 470.0 * 60.0, days: "daily".into(), runs: true, next_run: None, trips: vec![info("5 A-B", 1, 360), info("5 B-A", 2, 400), info("5 A-B", 3, 440)] }],
        }]
    }

    #[test]
    fn the_launcher_shows_the_duty_as_planned() {
        let (c, tours) = setup();
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        let r = DutyRef::of(&p, 0, 0).unwrap();
        let s = sheet(&c, &p, &r, super::super::clock::moment(&c.date, 300), &[]);
        assert!(s.startable(), "{:?}", s.hitches);
        // the bus the planning gives the tour, in the company's livery
        let v = &c.fleet[0];
        let b = s.bus.as_ref().unwrap();
        assert_eq!((b.id, b.file.as_str(), b.number.as_str()), (v.id, v.bus.as_str(), v.number.as_str()));
        // its trips in order, with the stands, the deadheads and signing on and off
        assert_eq!((s.trips, s.deadheads, s.from, s.to), (3, 2, 350, 480));
        assert!(matches!(&s.rows[0], Row::SignOn { at, time: 350, relief: false } if at == "Depot"));
        assert!(matches!(&s.rows[1], Row::Trip { empty: true, .. }));
        assert!(matches!(&s.rows[3], Row::Break { from: 390, to: 400, .. }));
        assert!(matches!(s.rows.last(), Some(Row::SignOff { at, time: 480, relief: false }) if at == "Depot"));
        assert_eq!(s.hof, "Grundorf.hof");
        assert_eq!(s.lines, vec!["5".to_string()]);
        assert_eq!(s.trip_names(), vec!["Depot-A", "5 A-B", "5 B-A", "5 A-B", "A-Depot"]);
    }

    #[test]
    fn the_start_is_exactly_the_planned_duty() {
        let (c, tours) = setup();
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        let s = sheet(&c, &p, &DutyRef::of(&p, 0, 0).unwrap(), super::super::clock::moment(&c.date, 300), &[]);
        let st = start(&s, &timetable(), 10).unwrap();
        let v = &c.fleet[0];
        assert_eq!((st.bus.as_str(), st.number.as_str(), st.plate.as_str()), (v.bus.as_str(), v.number.as_str(), v.plate.as_str()));
        assert_eq!((st.line.as_str(), st.tour.as_str(), st.first), ("Linie5", "2", 1));
        assert_eq!(st.legs, vec!["Linie5|2|1|3".to_string()]);
        // the lead before the first trip with passengers, on the plan's day
        assert_eq!((st.date.as_str(), st.time), ("2024-03-04", 350));
        let base = crate::Duty { bus: "Vehicles/Other/other.bus".into(), map: "maps/Other/global.cfg".into(), traffic: Some(40), weather: Some("x.wtr".into()), passengers: Some(false), lan: Some("host".into()), ..Default::default() };
        let d = st.duty(base, "");
        assert_eq!((d.map.as_str(), d.bus.as_str()), ("maps/Grundorf/global.cfg", v.bus.as_str()));
        assert_eq!((d.line.as_deref(), d.tour.as_deref(), d.trip.as_deref(), d.time.as_str(), d.date.as_deref()), (Some("Linie5"), Some("2"), Some("1"), "05:50", Some("2024-03-04")));
        assert_eq!(d.legs, st.legs);
        assert_eq!(d.hof.as_deref(), Some("Grundorf.hof"));
        assert_eq!(d.company.as_deref(), Some(c.id.as_str()));
        // the player's game options stay; nothing of another drive does
        assert_eq!((d.traffic, d.weather, d.passengers, d.lan.as_deref(), d.entry), (Some(40), None, Some(true), Some("off"), Some(-1)));
    }

    #[test]
    fn a_problem_blocks_the_start_with_its_reason() {
        let (mut c, tours) = setup();
        let lines = timetable();
        let now = super::super::clock::moment(&c.date, 300);
        // the bus in the workshop
        c.fleet[0].workshop_until = Some(c.date.clone());
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let s = sheet(&c, &p, &DutyRef::of(&p, 0, 0).unwrap(), now, &[]);
        assert!(!s.startable());
        assert!(start(&s, &lines, 10).is_err());
        c.fleet[0].workshop_until = None;
        // another driver on it
        let wd = weekday_of(&c.date);
        set_driver(&mut c, wd, "Linie5", "2", 0, None);
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let s = sheet(&c, &p, &DutyRef::of(&p, 0, 0).unwrap(), now, &[]);
        assert!(s.hitches.iter().any(|h| matches!(h, Hitch::NoDriver | Hitch::NotYours(_))), "{:?}", s.hitches);
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Player));
        // its time over on the company's clock
        let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
        let r = DutyRef::of(&p, 0, 0).unwrap();
        let s = sheet(&c, &p, &r, super::super::clock::moment(&c.date, 481), &[]);
        assert_eq!(start(&s, &lines, 10), Err(Hitch::Over));
        // driven already: a trip report of its trip
        let run = TripRun { time: c.trips_seen + 1, map: c.map.clone(), line: "5".into(), tour: "2".into(), departure: 400.0 * 60.0, ..Default::default() };
        let s = sheet(&c, &p, &r, now, &[run]);
        assert_eq!(s.hitches, vec![Hitch::Driven]);
        // the timetable changed: a trip of the duty is no longer in it
        let s = sheet(&c, &p, &r, now, &[]);
        let mut changed = lines.clone();
        changed[0].tours[0].trips.remove(1);
        assert_eq!(start(&s, &changed, 10), Err(Hitch::Timetable));
        // the plan no longer has the duty
        let gone = DutyRef { tour: "9".into(), ..r.clone() };
        assert_eq!(sheet(&c, &p, &gone, now, &[]).hitches, vec![Hitch::Gone]);
        // every hitch says what is wrong and what to do
        for h in [Hitch::Gone, Hitch::NoBus(None), Hitch::Rental, Hitch::Bus(Problem::Workshop), Hitch::NoBusFile, Hitch::NotInstalled(String::new()), Hitch::NoDriver, Hitch::NotYours(String::new()), Hitch::Driven, Hitch::Over, Hitch::Timetable] {
            let (a, b) = h.words();
            assert!(!a.is_empty() && !b.is_empty());
        }
    }

    #[test]
    fn a_duty_is_driven_from_its_own_first_trip() {
        let planned = |name: &str, dep: i32, depot: bool| PlannedTrip { name: name.into(), line: if depot { String::new() } else { "7".into() }, dep, arr: dep + 30, stops: if depot { 0 } else { 5 }, empty: depot, ..Default::default() };
        let timetable = [info("7 A-B", 1, 360), info("7 B-A", 2, 400), info("7 A-B", 3, 440)];
        // an own line's tour as the plan has it: the depot runs added before and after
        let tour = [planned("Betriebsfahrt", 352, true), planned("7 A-B", 360, false), planned("7 B-A", 400, false), planned("7 A-B", 440, false), planned("Betriebsfahrt", 470, true)];
        assert_eq!(timetable_legs(&tour[0..3], &timetable), Some((1, 360, 2)));
        assert_eq!(timetable_legs(&tour[3..5], &timetable), Some((3, 440, 1)));
        assert_eq!(timetable_legs(&tour[1..4], &timetable), Some((1, 360, 3)));
        assert_eq!(timetable_legs(&tour[4..5], &timetable), None);
        // a trip of the duty the timetable no longer has, or one out of order
        assert_eq!(timetable_legs(&tour[1..4], &[info("7 A-B", 1, 360), info("7 A-B", 3, 440)]), None);
        assert_eq!(timetable_legs(&tour[1..3], &[info("7 A-B", 1, 360), info("7 B-A", 3, 400)]), None);
    }
}

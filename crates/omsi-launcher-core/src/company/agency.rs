//! Agency drivers booked ahead for a day (Luc, 2026-10-15: "you must also be able to hire agency
//! drivers a day ahead, for a specific day, through the Employees tab and then Agencies - as many as
//! you want"). A booking is for a date from tomorrow on, up to `DAYS_AHEAD` days: how many drivers,
//! and the hours they come for, or the duties they come for, or a whole day (`DAY_MINUTES`).
//!
//! On their day each is a driver of the day like the staff - "Agency driver 1", "Agency driver 2"
//! ... (`drivers_on`; `Company::employee` finds them): the Roster tab gives them duties of that day
//! (`give`), and the dispatcher and "Fill the roster" take them first for what is open
//! (`plan::day_plan`). They are paid at the day's close for the minutes booked, or for those worked
//! when more: a booked driver left without a duty is paid in full, as agencies bill (`settle`).
//! Booking ahead is cheaper by the hour (`AHEAD_HOUR`) than an agency driver sent on the day itself
//! (`plan::AGENCY_HOUR`). Cancelled before its day a booking costs nothing; on the day itself the
//! agency bills half of it (`LATE_FEE`). There is no limit to how many: only the cash.

use super::dates;
use super::model::{BookingKind, Cents, Company, Employee};
use super::plan::{self, DayPlan, Who};
use super::rng::Rng;
use super::staff::{self, Block};
use super::studio::PieceRef;
use serde::{Deserialize, Serialize};

/// The first id of the booked drivers (the staff's are small numbers; `plan::AGENCY` and
/// `plan::PLAYER` are the last two): a booking's drivers are `BOOKED_BASE + no * PER_BOOKING + k`.
pub const BOOKED_BASE: u32 = 0xA000_0000;
const PER_BOOKING: u32 = 256;
/// Drivers in one booking at most (another booking for more).
pub const MOST: u32 = 99;
/// A booked agency driver's hour, the agency's margin included (founding day's prices).
pub const AHEAD_HOUR: Cents = 36_00;
/// What a driver booked for a whole day is paid for, and for some hours or duties at least.
pub const DAY_MINUTES: i32 = 480;
pub const MIN_MINUTES: i32 = 240;
/// How far ahead the agency takes bookings.
pub const DAYS_AHEAD: i64 = 42;
/// Cancelled on its day itself: the share of the booking the agency bills.
pub const LATE_FEE: f64 = 0.5;
/// How long a booking stays after its day (its drivers named on the reports and the agenda).
const KEPT_DAYS: i64 = 14;
/// The bus types a booked driver may drive: any (`licences::lack`).
pub const ANY_TYPE: &str = "*";

/// Why the agency does not take a booking or a cancellation (English, translated when shown).
pub const NOT_AHEAD: &str = "Agency drivers are booked from tomorrow on. For today, give a duty to an agency driver in the Roster tab.";
pub const TOO_FAR: &str = "The agency takes bookings up to six weeks ahead.";
pub const COUNT: &str = "Book between 1 and 99 drivers at a time.";
pub const HOURS: &str = "The hours booked must last an hour at least, within the day.";
pub const NO_CASH: &str = "Not enough cash.";
pub const GONE: &str = "This booking's day is over: it can no longer be cancelled.";
pub const NOT_FOUND: &str = "This booking is no longer there.";

/// Every English text of the bookings (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    vec![NOT_AHEAD, TOO_FAR, COUNT, HOURS, NO_CASH, GONE, NOT_FOUND, NAME]
}

/// What a booked driver is called where his number is shown ("Agency driver 3"); his own name
/// is one of the company's country (`names`; an older file's: this and his number).
pub const NAME: &str = "Agency driver";

/// Agency drivers booked for a day.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Booking {
    pub no: u32,
    pub date: String,
    /// The company's day it was booked on.
    #[serde(default)]
    pub booked: String,
    /// The agency's hour when it was booked.
    pub rate: Cents,
    /// The hours they come for (minutes of the day); None: when the day needs them.
    #[serde(default)]
    pub hours: Option<(i32, i32)>,
    #[serde(default)]
    pub drivers: Vec<Booked>,
    /// Paid at its day's close.
    #[serde(default)]
    pub settled: bool,
}

/// A driver of a booking: who he is on his day, what he is paid for at least, and the duties
/// given to him that day (by their block and the trip they begin with).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Booked {
    pub employee: Employee,
    pub minutes: i32,
    #[serde(default)]
    pub duties: Vec<PieceRef>,
}

impl Booking {
    /// What it costs as booked: each driver his minutes at the booking's hour.
    pub fn cost(&self) -> Cents {
        self.drivers.iter().map(|d| cost_of(self.rate, d.minutes)).sum()
    }
}

/// A booking as the pages and the company's agenda list it.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub no: u32,
    pub date: String,
    pub count: u32,
    pub hours: Option<(i32, i32)>,
    /// The duties given to its drivers.
    pub duties: usize,
    /// What it costs as booked (the close pays the minutes worked when more).
    pub cost: Cents,
    /// Its day is closed and it was paid.
    pub settled: bool,
    /// Its drivers' ids.
    pub drivers: Vec<u32>,
}

/// Every booking, by its day (the agenda's: `agency::bookings`).
pub fn bookings(c: &Company) -> Vec<Summary> {
    let mut out: Vec<Summary> = c
        .planning
        .agency
        .iter()
        .map(|b| Summary {
            no: b.no,
            date: b.date.clone(),
            count: b.drivers.len() as u32,
            hours: b.hours,
            duties: b.drivers.iter().map(|d| d.duties.len()).sum(),
            cost: b.cost(),
            settled: b.settled,
            drivers: b.drivers.iter().map(|d| d.employee.id).collect(),
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date).then(a.no.cmp(&b.no)));
    out
}

/// Whether `id` is a booked agency driver's.
pub fn is_booked(id: u32) -> bool {
    (BOOKED_BASE..plan::PLAYER).contains(&id)
}

fn booked(c: &Company, id: u32) -> Option<(&Booking, &Booked)> {
    if !is_booked(id) {
        return None;
    }
    c.planning.agency.iter().find_map(|b| b.drivers.iter().find(|d| d.employee.id == id).map(|d| (b, d)))
}

fn booked_mut(c: &mut Company, id: u32) -> Option<&mut Booked> {
    c.planning.agency.iter_mut().find_map(|b| b.drivers.iter_mut().find(|d| d.employee.id == id))
}

/// A booked driver as the company's other drivers are (`Company::employee`).
pub fn driver(c: &Company, id: u32) -> Option<&Employee> {
    booked(c, id).map(|x| &x.1.employee)
}

/// The agency drivers booked for `date`, by their number.
pub fn drivers_on<'a>(c: &'a Company, date: &str) -> Vec<&'a Employee> {
    let mut v: Vec<&Employee> = c.planning.agency.iter().filter(|b| b.date == date).flat_map(|b| b.drivers.iter().map(|d| &d.employee)).collect();
    v.sort_by_key(|e| number_of(e));
    v
}

/// A booked driver's number of his day (an older file's by his name, "Agency driver 3": 3).
pub fn number_of(e: &Employee) -> u32 {
    if e.agency_no > 0 {
        return e.agency_no;
    }
    e.name.rsplit(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// The hours a booked driver comes for (None: any time of the day).
pub fn window(c: &Company, id: u32) -> Option<(i32, i32)> {
    booked(c, id).and_then(|x| x.0.hours)
}

/// Whether a piece of work lies within the hours booked driver `id` comes for (anyone else's:
/// always).
pub fn within(c: &Company, id: u32, b: &Block) -> bool {
    window(c, id).is_none_or(|(from, to)| b.from >= from && b.to <= to)
}

/// The duties given to booked driver `id` on his day.
pub fn duties_of(c: &Company, id: u32) -> &[PieceRef] {
    booked(c, id).map(|x| x.1.duties.as_slice()).unwrap_or(&[])
}

/// The minutes booked driver `id` is paid for at least.
pub fn minutes_booked(c: &Company, id: u32) -> i32 {
    booked(c, id).map(|x| x.1.minutes).unwrap_or(0)
}

/// The agency's hour for a booking now.
pub fn rate(c: &Company) -> Cents {
    (AHEAD_HOUR as f64 * c.price_index).round() as Cents
}

fn cost_of(rate: Cents, minutes: i32) -> Cents {
    (minutes.max(0) as f64 / 60.0 * rate as f64).round() as Cents
}

/// What a driver is paid for at least when booked for `hours` (None: a whole day).
pub fn minutes_of(hours: Option<(i32, i32)>) -> i32 {
    hours.map(|(from, to)| (to - from).max(MIN_MINUTES)).unwrap_or(DAY_MINUTES)
}

/// What `count` drivers booked for `minutes` each cost now.
pub fn quote(c: &Company, count: u32, minutes: i32) -> Cents {
    count as Cents * cost_of(rate(c), minutes)
}

/// Whether `date` can be booked: from tomorrow on, up to `DAYS_AHEAD` days ahead.
pub fn bookable(c: &Company, date: &str) -> Result<(), &'static str> {
    let ahead = dates::between(&c.date, date);
    if dates::parse(date).is_none() || ahead < 1 {
        Err(NOT_AHEAD)
    } else if ahead > DAYS_AHEAD {
        Err(TOO_FAR)
    } else {
        Ok(())
    }
}

/// What a booking made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Made {
    pub no: u32,
    pub cost: Cents,
    /// The duties given to its drivers, and those that did not fit them (by the working-time
    /// rules, or the hours booked).
    pub placed: usize,
    pub left: usize,
}

/// Book `count` drivers for `date`: for `hours` (minutes of the day), or for the duties `duties`
/// (each with its time; given to them by the working-time rules, the first that fits), or for a
/// whole day.
pub fn book(c: &mut Company, date: &str, count: u32, hours: Option<(i32, i32)>, duties: &[(PieceRef, Block)]) -> Result<Made, &'static str> {
    bookable(c, date)?;
    if count == 0 || count > MOST {
        return Err(COUNT);
    }
    if hours.is_some_and(|(from, to)| from < 0 || to > 1440 + 360 || to - from < 60) {
        return Err(HOURS);
    }
    // (the duties first, each to the first driver it fits)
    let mut given: Vec<Vec<(PieceRef, Block)>> = vec![Vec::new(); count as usize];
    let mut sorted: Vec<&(PieceRef, Block)> = duties.iter().collect();
    sorted.sort_by_key(|x| x.1.from);
    let mut left = 0;
    for x in sorted {
        let fits = |blocks: &[(PieceRef, Block)]| {
            let work: Vec<Block> = blocks.iter().map(|b| b.1.clone()).collect();
            hours.is_none_or(|(from, to)| x.1.from >= from && x.1.to <= to) && staff::check(&work, &x.1, None, None).allowed()
        };
        match given.iter_mut().find(|g| fits(g)) {
            Some(g) => g.push(x.clone()),
            None => left += 1,
        }
    }
    let minutes = |g: &[(PieceRef, Block)]| match hours {
        Some(_) => minutes_of(hours),
        None if duties.is_empty() => DAY_MINUTES,
        None => staff::work_minutes(&g.iter().map(|b| b.1.clone()).collect::<Vec<_>>()).max(MIN_MINUTES),
    };
    let rate = rate(c);
    let cost: Cents = given.iter().map(|g| cost_of(rate, minutes(g))).sum();
    if !c.can_pay(cost) {
        return Err(NO_CASH);
    }
    prune(c);
    let no = c.planning.agency.iter().map(|b| b.no).max().unwrap_or(0) + 1;
    let first = c.planning.agency.iter().filter(|b| b.date == date).flat_map(|b| b.drivers.iter()).map(|d| number_of(&d.employee)).max().unwrap_or(0);
    // (each a name of the company's country, none of its people's or the other drivers')
    let mut used = super::names::used_in(c);
    let mut rng = Rng::of(&[&c.id, "agency", date], no as i64);
    let country = super::names::country_of(c);
    let drivers: Vec<Booked> = given
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let mut e = plan::agency_driver();
            e.id = BOOKED_BASE + no * PER_BOOKING + k as u32;
            e.agency_no = first + 1 + k as u32;
            e.name = super::names::draw(country, &mut rng, &mut used).name;
            e.hired = date.to_string();
            e.notice_until = Some(date.to_string());
            e.types = vec![ANY_TYPE.to_string()];
            Booked { employee: e, minutes: minutes(g), duties: g.iter().map(|b| b.0.clone()).collect() }
        })
        .collect();
    let placed = drivers.iter().map(|d| d.duties.len()).sum();
    c.planning.agency.push(Booking { no, date: date.to_string(), booked: c.date.clone(), rate, hours, drivers, settled: false });
    Ok(Made { no, cost, placed, left })
}

/// The bookings of days long gone forgotten.
fn prune(c: &mut Company) {
    let today = c.date.clone();
    c.planning.agency.retain(|b| dates::between(&b.date, &today) <= KEPT_DAYS);
}

/// Cancel booking `no`: free before its day, on the day itself the agency bills `LATE_FEE` of it.
/// Returns the fee.
pub fn cancel(c: &mut Company, no: u32) -> Result<Cents, &'static str> {
    let b = c.planning.agency.iter().find(|b| b.no == no).ok_or(NOT_FOUND)?;
    if b.settled || dates::between(&c.date, &b.date) < 0 {
        return Err(GONE);
    }
    let fee = cancel_fee(c, b);
    let text = format!("{NAME}s cancelled on the day ({})", b.drivers.len());
    c.planning.agency.retain(|b| b.no != no);
    c.book(BookingKind::Agency, -fee, text, false);
    Ok(fee)
}

/// What cancelling `b` now costs: on its day itself half of it, before nothing.
pub fn cancel_fee(c: &Company, b: &Booking) -> Cents {
    if b.date == c.date {
        (b.cost() as f64 * LATE_FEE).round() as Cents
    } else {
        0
    }
}

/// Duty `piece` of booked driver `id`'s day given to him (taken from whoever of that day's other
/// booked drivers had it). Returns whether he is a booked driver.
pub fn give(c: &mut Company, id: u32, piece: PieceRef) -> bool {
    let Some(date) = booked(c, id).map(|x| x.0.date.clone()) else { return false };
    release(c, &date, &piece);
    if let Some(d) = booked_mut(c, id) {
        d.duties.push(piece);
    }
    true
}

/// Duty `piece` of `date` given back by the booked driver who has it. Returns whether one had it.
pub fn release(c: &mut Company, date: &str, piece: &PieceRef) -> bool {
    let mut had = false;
    for b in c.planning.agency.iter_mut().filter(|b| b.date == date) {
        for d in b.drivers.iter_mut() {
            let n = d.duties.len();
            d.duties.retain(|p| !p.is(piece));
            had |= d.duties.len() != n;
        }
    }
    had
}

/// What the booked drivers of `dp`'s day cost, paid at its close (once): each the minutes booked,
/// or those he drove when more. Returns what was paid.
pub fn settle(c: &mut Company, dp: &DayPlan) -> Cents {
    let worked = |id: u32| dp.duties_of(Who::Staff(id)).iter().map(|&(i, k)| (dp.tours[i].duties[k].to - dp.tours[i].duties[k].from).max(0)).sum::<i32>();
    let mut lines: Vec<(Cents, String)> = Vec::new();
    for b in c.planning.agency.iter_mut().filter(|b| b.date == dp.date && !b.settled) {
        for d in &b.drivers {
            let (booked, drove) = (d.minutes, worked(d.employee.id));
            let paid = booked.max(drove);
            let text = format!("{} (booked): {} h {:02}, {} h {:02} driven", d.employee.name, paid / 60, paid % 60, drove / 60, drove % 60);
            lines.push((cost_of(b.rate, paid), text));
        }
        b.settled = true;
    }
    let mut total = 0;
    for (cost, text) in lines {
        total += cost;
        c.book(BookingKind::Agency, -cost, text, false);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::super::model::{CompanyLine, GameMode, Licence};
    use super::super::network::tests::trip;
    use super::super::network::TourOfDay;
    use super::super::plan::{day_plan, disruptions, fill_day, set_bus, weekday_of, Source};
    use super::super::staff::{applicants, hire};
    use super::super::studio;
    use super::super::{found, market, Founding};
    use super::*;

    fn tour(no: &str, from: i32, n: i32) -> TourOfDay {
        TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..n).map(|k| trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None }
    }

    /// A company of line 5 with `buses` buses and `people` drivers, its mornings quiet.
    fn company(buses: usize, people: usize) -> Company {
        let mut c = found(&Founding { name: "Agency".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        let bus = market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        c.cash += 5_000_000_00;
        for _ in 0..buses {
            market::buy_new(&mut c, &bus, market::Payment::Cash, "").unwrap();
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
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        for n in 0.. {
            c.id = format!("{id}{n}");
            if disruptions(&c, &c.date).is_empty() && disruptions(&c, &dates::add(&c.date, 1)).is_empty() {
                break;
            }
        }
        c
    }

    #[test]
    fn agency_drivers_are_booked_ahead_and_cancelled() {
        let mut c = company(0, 0);
        let today = c.date.clone();
        let tomorrow = dates::add(&today, 1);
        assert_eq!(book(&mut c, &today, 1, None, &[]), Err(NOT_AHEAD));
        assert_eq!(book(&mut c, &dates::add(&today, DAYS_AHEAD + 1), 1, None, &[]), Err(TOO_FAR));
        assert_eq!(book(&mut c, &tomorrow, 0, None, &[]), Err(COUNT));
        assert_eq!(book(&mut c, &tomorrow, 1, Some((600, 630)), &[]), Err(HOURS));
        // as many as he wants: two bookings of the day, numbered on
        let a = book(&mut c, &tomorrow, 3, None, &[]).unwrap();
        assert_eq!(a.cost, 3 * quote(&c, 1, DAY_MINUTES));
        let b = book(&mut c, &tomorrow, 2, Some((6 * 60, 10 * 60)), &[]).unwrap();
        assert_eq!(b.cost, 2 * cost_of(rate(&c), MIN_MINUTES));
        let numbers: Vec<u32> = drivers_on(&c, &tomorrow).iter().map(|e| number_of(e)).collect();
        assert_eq!(numbers, (1..=5).collect::<Vec<_>>());
        // (each a name of his own, none twice: Beta 62's names)
        let mut names: Vec<String> = drivers_on(&c, &tomorrow).iter().map(|e| e.name.clone()).collect();
        assert!(names.iter().all(|n| !n.starts_with(NAME) && n.contains(' ')), "{names:?}");
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 5);
        let id = drivers_on(&c, &tomorrow)[3].id;
        assert!(is_booked(id) && c.employee(id).is_some_and(|e| e.employed_on(&tomorrow) && !e.employed_on(&today)));
        assert_eq!(window(&c, id), Some((360, 600)));
        assert_eq!(bookings(&c).iter().map(|s| (s.date.as_str(), s.count)).collect::<Vec<_>>(), vec![(tomorrow.as_str(), 3), (tomorrow.as_str(), 2)]);
        // cancelled before its day: free; on its day: half of it
        let cash = c.cash;
        assert_eq!(cancel(&mut c, a.no), Ok(0));
        assert_eq!(c.cash, cash);
        c.date = tomorrow.clone();
        let fee = cancel(&mut c, b.no).unwrap();
        assert_eq!(fee, (b.cost as f64 * LATE_FEE).round() as Cents);
        assert_eq!(c.cash, cash - fee);
        assert_eq!(cancel(&mut c, b.no), Err(NOT_FOUND));
        // not without the cash
        c.cash = 0;
        assert_eq!(book(&mut c, &dates::add(&tomorrow, 1), 1, None, &[]), Err(NO_CASH));
    }

    #[test]
    fn booked_drivers_drive_on_their_day_and_are_paid_at_its_close() {
        let mut c = company(3, 1);
        let tomorrow = dates::add(&c.date, 1);
        let wd = weekday_of(&tomorrow);
        let tours = vec![tour("1", 6 * 60, 4), tour("2", 6 * 60 + 10, 4), tour("3", 15 * 60, 3)];
        for (k, no) in ["1", "2", "3"].iter().enumerate() {
            let b = c.fleet[k].id;
            set_bus(&mut c, wd, "Linie5", no, Some(b));
        }
        // two booked for tour 1's duty and the afternoon; one more for the whole day
        let p = day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
        let piece = |p: &DayPlan, t: usize| {
            let blocks: Vec<TourOfDay> = p.tours.iter().map(|x| x.tour.clone()).collect();
            let d = &p.tours[t].duties[0];
            (studio::piece_ref(&blocks, t, 0).unwrap(), Block { key: format!("{t}"), from: d.from, to: d.to, ..Default::default() })
        };
        let made = book(&mut c, &tomorrow, 1, None, &[piece(&p, 0), piece(&p, 2)]).unwrap();
        assert_eq!((made.placed, made.left), (2, 0));
        book(&mut c, &tomorrow, 1, None, &[]).unwrap();
        let p = day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
        let first = drivers_on(&c, &tomorrow)[0].id;
        assert_eq!((p.tours[0].duties[0].who, p.tours[0].duties[0].from_), (Some(Who::Staff(first)), Source::Dispatcher));
        assert_eq!(p.tours[2].duties[0].who, Some(Who::Staff(first)));
        // "Fill the roster" that day: the other booked driver first, before the staff
        let second = drivers_on(&c, &tomorrow)[1].id;
        fill_day(&mut c, &tomorrow, tours.clone());
        let p = day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
        assert_eq!(p.tours[1].duties[0].who, Some(Who::Staff(second)));
        assert_eq!(duties_of(&c, second).len(), 1, "given for that day, not in the weekday's roster");
        assert!(plan::roster(&c, wd, "Linie5", "2").is_none_or(|r| r.duties.iter().all(Option::is_none)));
        // the next week's same day has none of them
        assert!(day_plan(&c, &dates::add(&tomorrow, 7), tours.clone(), &[], &[], false).tours.iter().flat_map(|t| t.duties.iter()).all(|d| !matches!(d.who, Some(Who::Staff(id)) if is_booked(id))));
        // the close of their day: every trip runs, each paid what was booked (the one for
        // two duties' hours, the other his whole day), shown as its own line
        c.date = tomorrow.clone();
        let month = dates::month_of(&tomorrow);
        let before = c.month(&month).get(BookingKind::Agency);
        let r = super::super::day::close_day(&mut c, tours, &[]);
        assert_eq!(r.dropped, 0, "{:?}", r.drops);
        let paid = before - c.month(&month).get(BookingKind::Agency);
        assert_eq!(paid, cost_of(rate(&c), 230 + 170) + cost_of(rate(&c), DAY_MINUTES));
        assert!(c.planning.agency.iter().all(|b| b.settled));
        assert!(c.staff.iter().all(|e| !is_booked(e.id)));
    }
}

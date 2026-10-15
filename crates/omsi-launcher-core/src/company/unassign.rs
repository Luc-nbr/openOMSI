//! Taking a driver off a duty, or a bus off a tour, on a day's roster (testers, 2026-10-14: "need
//! an easier way to deallocate a driver or a bus"). What is taken off is what the day shows
//! (`plan::day_plan`): the dispatcher's choice for the day is taken back (`plan::set_fill`), the
//! central's cover of what fell out is given up - its trips are dropped today, with the
//! contract's penalty - and else the roster's own entry goes (`plan::set_driver`,
//! `plan::set_bus`; the weekday's, so every week). A rental bus taken off is not rented: the
//! close rents only the rental buses the day still has (`plan::settle_morning`).
//!
//! What has begun on the company's day, what the player drove already and what the game runs
//! now stays, with the reason why. The pieces of a duty joined in the studio lose their driver
//! together, as "Give to" gave them together.

use super::clock;
use super::model::{Cents, Company};
use super::plan::{self, BusOf, DayPlan, Fill, Source, Who};

/// Why something is not taken off (English, translated when shown).
pub const NOT_FOUND: &str = "The roster changed meanwhile: try again.";
pub const BEGUN: &str = "This duty has begun already: its driver can no longer be taken off today.";
pub const UNDER_WAY: &str = "This tour is under way: its bus can no longer be taken off today.";
pub const DRIVEN: &str = "You drove it already: what was driven stays.";
pub const IN_GAME: &str = "The game runs this tour now: take it off once it is back in the depot.";
pub const NO_DRIVER: &str = "Nobody drives it: there is nothing to take off.";
pub const NO_BUS: &str = "It has no bus: there is nothing to take off.";

/// Every English text of taking off (for the translations' check).
pub fn texts() -> [&'static str; 7] {
    [NOT_FOUND, BEGUN, UNDER_WAY, DRIVEN, IN_GAME, NO_DRIVER, NO_BUS]
}

/// What taking off did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Off {
    /// Out of the weekday's roster.
    Roster,
    /// The dispatcher's choice for the day taken back.
    Choice,
    /// The day's rental bus cancelled: its rent for the day is not paid.
    Rental { rent: Cents },
    /// The central's cover given up: the trips are dropped today (and its rental bus, if it
    /// was one, not rented).
    Dropped { penalty: Cents, rent: Cents },
}

/// Whether what begins at `from` (minutes of the plan's day) has begun: on the company's day,
/// its clock past it.
pub fn begun(c: &Company, p: &DayPlan, from: i32) -> bool {
    p.today && clock::now(c) >= clock::moment(&p.date, from as i64)
}

fn rostered_driver(c: &Company, p: &DayPlan, ti: usize, k: usize) -> Option<Who> {
    let t = &p.tours[ti].tour;
    plan::roster(c, p.weekday, &t.line, &t.tour).and_then(|r| r.duties.get(k).copied().flatten())
}

/// Take the driver off duty `k` of tour `ti` of the day `p` (see the module).
pub fn driver_off(c: &mut Company, p: &DayPlan, ti: usize, k: usize) -> Result<Off, &'static str> {
    let t = p.tours.get(ti).ok_or(NOT_FOUND)?;
    let d = t.duties.get(k).ok_or(NOT_FOUND)?;
    if t.by_player {
        return Err(DRIVEN);
    }
    if t.live {
        return Err(IN_GAME);
    }
    // (a driver who is not there can be taken out of the roster whenever)
    if d.who.is_some() && begun(c, p, d.from) {
        return Err(BEGUN);
    }
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let key = plan::duty_key(&line, &tour, k);
    match (d.who, d.from_) {
        (Some(_), Source::Dispatcher) => {
            plan::set_fill(c, &key, None);
            // (the pieces joined with it have the same choice - one agency driver drives them
            // all: taken back with it; one that has begun keeps it, and this one is dropped)
            let slot = super::plandays::slot_of(c, &p.date);
            let mut kept = false;
            for (i, j) in plan::joined_pieces(c, slot, &p.tours, ti, k) {
                let x = &p.tours[i];
                let y = &x.duties[j];
                if y.from_ != Source::Dispatcher {
                    continue;
                }
                if y.who.is_some() && begun(c, p, y.from) {
                    kept = true;
                } else {
                    plan::set_fill(c, &plan::duty_key(&x.tour.line, &x.tour.tour, j), None);
                }
            }
            if kept {
                let penalty = plan::drop_penalty(c, &t.tour, d.start, d.end);
                plan::set_fill(c, &key, Some(Fill::Drop));
                return Ok(Off::Dropped { penalty, rent: 0 });
            }
            return Ok(Off::Choice);
        }
        (Some(_), Source::Central) => {
            let penalty = plan::drop_penalty(c, &t.tour, d.start, d.end);
            plan::set_fill(c, &key, Some(Fill::Drop));
            return Ok(Off::Dropped { penalty, rent: 0 });
        }
        _ => {}
    }
    let Some(who) = rostered_driver(c, p, ti, k) else { return Err(NO_DRIVER) };
    // (the pieces joined with it: the same driver off them too, where not begun)
    let blocks: Vec<_> = p.tours.iter().map(|x| x.tour.clone()).collect();
    let partners = super::studio::partners(c, super::plandays::slot_of(c, &p.date), &blocks, ti, k);
    plan::set_driver(c, p.weekday, &line, &tour, k, None);
    for (i, j) in partners {
        let x = &p.tours[i];
        let busy = x.duties.get(j).is_some_and(|y| y.who.is_some() && begun(c, p, y.from));
        if rostered_driver(c, p, i, j) == Some(who) && !busy && !x.by_player && !x.live {
            plan::set_driver(c, p.weekday, &x.tour.line, &x.tour.tour, j, None);
        }
    }
    Ok(Off::Roster)
}

/// Take the bus off tour `ti` of the day `p` (see the module).
pub fn bus_off(c: &mut Company, p: &DayPlan, ti: usize) -> Result<Off, &'static str> {
    let t = p.tours.get(ti).ok_or(NOT_FOUND)?;
    if t.by_player {
        return Err(DRIVEN);
    }
    if t.live {
        return Err(IN_GAME);
    }
    if t.bus.is_some() && begun(c, p, t.tour.from()) {
        return Err(UNDER_WAY);
    }
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let key = plan::tour_key(&line, &tour);
    match (t.bus, t.bus_from) {
        (Some(BusOf::Rental), Source::Dispatcher) => {
            let rent = plan::rental_for(c, p, ti).0;
            plan::set_fill(c, &key, None);
            return Ok(Off::Rental { rent });
        }
        (Some(_), Source::Dispatcher) => {
            plan::set_fill(c, &key, None);
            return Ok(Off::Choice);
        }
        (Some(b), Source::Central) => {
            let rent = if b == BusOf::Rental { plan::rental_for(c, p, ti).0 } else { 0 };
            let penalty = plan::drop_penalty(c, &t.tour, 0, t.tour.trips.len());
            plan::set_fill(c, &key, Some(Fill::Drop));
            return Ok(Off::Dropped { penalty, rent });
        }
        _ => {}
    }
    if plan::roster(c, p.weekday, &line, &tour).and_then(|r| r.bus).is_none() {
        return Err(NO_BUS);
    }
    plan::set_bus(c, p.weekday, &line, &tour, None);
    Ok(Off::Roster)
}

/// What taking off several did: how many, and why the others stayed (each reason once).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Many {
    pub done: Vec<Off>,
    pub stayed: Vec<&'static str>,
}

impl Many {
    fn add(&mut self, r: Result<Off, &'static str>) {
        match r {
            Ok(o) => self.done.push(o),
            // (a piece taken off with its partner before: nothing more to say)
            Err(NO_DRIVER) | Err(NO_BUS) => {}
            Err(why) => {
                if !self.stayed.contains(&why) {
                    self.stayed.push(why);
                }
            }
        }
    }

    /// The penalties the trips dropped cost, and the rent of the rental buses not rented.
    pub fn money(&self) -> (Cents, Cents) {
        self.done.iter().fold((0, 0), |(p, r), o| match o {
            Off::Dropped { penalty, rent } => (p + penalty, r + rent),
            Off::Rental { rent } => (p, r + rent),
            _ => (p, r),
        })
    }
}

/// Take off what is chosen: duties (tour, duty) and tours' buses (tour, None), on the day as
/// it was when they were chosen (`p`).
pub fn many_off(c: &mut Company, p: &DayPlan, items: &[(usize, Option<usize>)]) -> Many {
    let mut out = Many::default();
    for &(ti, k) in items {
        out.add(match k {
            Some(k) => driver_off(c, p, ti, k),
            None => bus_off(c, p, ti),
        });
    }
    out
}

/// Take `who` off every duty the day `p` gives them.
pub fn driver_off_all(c: &mut Company, p: &DayPlan, who: Who) -> Many {
    let items: Vec<(usize, Option<usize>)> = p.duties_of(who).into_iter().map(|(ti, k)| (ti, Some(k))).collect();
    many_off(c, p, &items)
}

/// Take bus `bus` off every tour of the day `p` it runs or is fixed for in the roster.
pub fn bus_off_all(c: &mut Company, p: &DayPlan, bus: u32) -> Many {
    let items: Vec<(usize, Option<usize>)> = (0..p.tours.len())
        .filter(|&ti| {
            let t = &p.tours[ti];
            t.bus == Some(BusOf::Own(bus)) || plan::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.bus) == Some(bus)
        })
        .map(|ti| (ti, None))
        .collect();
    many_off(c, p, &items)
}

#[cfg(test)]
mod tests {
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::{CompanyLine, GameMode, Licence};
    use super::super::network::tests::trip;
    use super::super::network::TourOfDay;
    use super::super::plan::{day_plan, roster, set_bus, set_driver, set_fill, tour_key, weekday_of};
    use super::super::staff::{applicants, hire};
    use super::super::{found, Founding};
    use super::*;

    fn tour(no: &str, from: i32, n: i32) -> TourOfDay {
        TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..n).map(|k| trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None }
    }

    /// A company of line 5 with buses and drivers, the morning quiet (nobody late, nothing broken).
    fn company(buses: usize, people: usize) -> Company {
        let mut c = found(&Founding { name: "Off".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
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
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        let base = id;
        for n in 0.. {
            c.id = format!("{base}{n}");
            if plan::disruptions(&c, &c.date).is_empty() {
                break;
            }
        }
        c
    }

    fn day(c: &Company, tours: &[TourOfDay]) -> DayPlan {
        day_plan(c, &c.date.clone(), tours.to_vec(), &[], &[], false)
    }

    #[test]
    fn a_driver_is_taken_off_a_duty() {
        let mut c = company(1, 2);
        let wd = weekday_of(&c.date);
        let tours = vec![tour("1", 9 * 60, 4)];
        let id = c.staff[0].id;
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(id)));
        let p = day(&c, &tours);
        assert_eq!(p.tours[0].duties[0].who, Some(Who::Staff(id)));
        assert_eq!(driver_off(&mut c, &p, 0, 0), Ok(Off::Roster));
        assert!(roster(&c, wd, "Linie5", "1").is_none(), "nothing left of the tour in the roster");
        assert_eq!(day(&c, &tours).tours[0].duties[0].who, None);
        // nothing to take off now; the player's own duty goes alike
        let p = day(&c, &tours);
        assert_eq!(driver_off(&mut c, &p, 0, 0), Err(NO_DRIVER));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Player));
        let p = day(&c, &tours);
        assert_eq!(driver_off(&mut c, &p, 0, 0), Ok(Off::Roster));
        // every duty of a driver at once
        let two = vec![tour("1", 6 * 60, 2), tour("2", 12 * 60, 2)];
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(id)));
        set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(id)));
        let p = day(&c, &two);
        let m = driver_off_all(&mut c, &p, Who::Staff(id));
        assert_eq!((m.done.len(), m.stayed.len()), (2, 0));
        assert!(c.planning.week.is_empty());
    }

    #[test]
    fn a_bus_is_taken_off_a_tour() {
        let mut c = company(1, 2);
        let wd = weekday_of(&c.date);
        let tours = vec![tour("1", 9 * 60, 4), tour("2", 15 * 60, 2)];
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        set_bus(&mut c, wd, "Linie5", "2", Some(b));
        let p = day(&c, &tours);
        assert_eq!(p.tours[0].bus, Some(BusOf::Own(b)));
        assert_eq!(bus_off(&mut c, &p, 0), Ok(Off::Roster));
        assert_eq!(roster(&c, wd, "Linie5", "1").and_then(|r| r.bus), None);
        assert_eq!(day(&c, &tours).tours[0].bus, None);
        // the bus off every tour of the day
        let p = day(&c, &tours);
        assert_eq!(bus_off_all(&mut c, &p, b).done, vec![Off::Roster]);
        assert!(c.planning.week.is_empty());
        let p = day(&c, &tours);
        assert_eq!(bus_off(&mut c, &p, 1), Err(NO_BUS));
    }

    #[test]
    fn a_rental_bus_taken_off_is_not_rented() {
        let mut c = company(1, 2);
        let wd = weekday_of(&c.date);
        let tours = vec![tour("1", 9 * 60, 4)];
        let b = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        let id = c.staff[0].id;
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(id)));
        c.fleet[0].workshop_until = Some(c.date.clone());
        set_fill(&mut c, &tour_key("Linie5", "1"), Some(Fill::Rental));
        let p = day(&c, &tours);
        assert_eq!((p.tours[0].bus, p.tours[0].bus_from), (Some(BusOf::Rental), Source::Dispatcher));
        match bus_off(&mut c, &p, 0) {
            Ok(Off::Rental { rent }) => assert!(rent > 0),
            other => panic!("{other:?}"),
        }
        assert!(c.planning.fills.is_empty(), "the day's choice taken back");
        assert_eq!(day(&c, &tours).tours[0].bus, None);
        // the close rents nothing
        let r = super::super::day::close_day(&mut c, tours, &[]);
        assert_eq!(c.month(&super::super::dates::month_of(&r.date)).get(super::super::model::BookingKind::Rent), 0);
    }

    #[test]
    fn what_has_begun_or_was_driven_stays() {
        let mut c = company(1, 2);
        let wd = weekday_of(&c.date);
        let tours = vec![tour("1", 6 * 60, 4)];
        let (b, id) = (c.fleet[0].id, c.staff[0].id);
        set_bus(&mut c, wd, "Linie5", "1", Some(b));
        set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(id)));
        // the company's clock at 08:00: the duty from 06:00 has begun
        c.clock.minute = 8 * 60;
        let p = day(&c, &tours);
        assert_eq!(driver_off(&mut c, &p, 0, 0), Err(BEGUN));
        assert_eq!(bus_off(&mut c, &p, 0), Err(UNDER_WAY));
        assert_eq!(roster(&c, wd, "Linie5", "1").map(|r| (r.bus, r.duties.clone())), Some((Some(b), vec![Some(Who::Staff(id))])));
        // the player drove it, or the game runs it: stays too
        let mut p = day(&c, &tours);
        p.tours[0].by_player = true;
        assert_eq!(driver_off(&mut c, &p, 0, 0), Err(DRIVEN));
        p.tours[0].by_player = false;
        p.tours[0].live = true;
        assert_eq!(bus_off(&mut c, &p, 0), Err(IN_GAME));
        // (at midnight it has not begun)
        c.clock.minute = 0;
        let p = day(&c, &tours);
        assert_eq!(driver_off(&mut c, &p, 0, 0), Ok(Off::Roster));
    }
}

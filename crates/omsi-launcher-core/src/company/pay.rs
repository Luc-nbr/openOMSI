//! The drivers' pay (the UK model, `econ_uk`): by the paid hour of duty, no longer a flat monthly
//! wage. A driver's day is paid from the duties the plan gave them - RosterFlow's or the standard
//! planning's (`day::close_day`): from signing on before the first piece of work to signing off
//! after the last, a split unpaid, at least the least a duty pays (the company's RosterFlow
//! settings: `rosterflow::Prefs`, `paid_minutes`). An hour is the driver's contract wage by the
//! hour (`staff::hourly`), and above the contracted hours of the week (`CONTRACT_WEEK_HOURS`) half
//! as much again (`OVERTIME_RATE`; not in Casual). The week guarantees its hours for the days a
//! driver was there: whoever drove less is paid the rest when the week ends (`week`). Holidays and
//! sick days are paid through the allowances on every hour (`econ_uk::ALLOWANCES`), so a day away
//! adds nothing to the guarantee; a day on a course does.
//!
//! The day's hours are booked with the day (`Wages`), the week's guarantee on its last day, with
//! the on-costs and allowances (`economy::employer_cost`'s share) on both.

use super::dates;
use super::econ_uk;
use super::economy::{self, EMPLOYER_SHARE};
use super::model::{BookingKind, Cents, Company, Employee};
use super::rosterflow::{self, Prefs};
use super::staff;

/// The minutes a driver's day is paid, from its pieces of work (minutes of the day, the bus's
/// checks before and after it included): as RosterFlow reckons a duty - signing on before the
/// first piece and off after the last, a gap of `Prefs::split_min` or more a split (unpaid, its
/// sign-off and sign-on paid), a shorter one paid, and at least the paid floor
/// (`rosterflow::paid_floor`). Nothing without a piece.
pub fn paid_minutes(p: &Prefs, pieces: &[(i32, i32)]) -> i32 {
    let mut ps: Vec<(i32, i32)> = pieces.iter().copied().filter(|x| x.1 >= x.0).collect();
    if ps.is_empty() {
        return 0;
    }
    ps.sort_unstable();
    // (pieces that overlap or touch are one)
    let mut merged: Vec<(i32, i32)> = Vec::new();
    for x in ps {
        match merged.last_mut() {
            Some(m) if x.0 <= m.1 => m.1 = m.1.max(x.1),
            _ => merged.push(x),
        }
    }
    let start = merged[0].0 - p.sign_on;
    let end = merged[merged.len() - 1].1 + p.sign_off;
    let unpaid: i32 = merged.windows(2).map(|w| w[1].0 - w[0].1).filter(|g| *g >= p.split_min).map(|g| (g - p.sign_off_split - p.sign_on_split).max(0)).sum();
    (end - start - unpaid).max(rosterflow::paid_floor(p))
}

/// The contracted minutes of a week.
pub fn contract_week_minutes() -> f64 {
    econ_uk::CONTRACT_WEEK_HOURS * 60.0
}

/// Away so that the day is paid through the allowances (ill or on holiday), not by the guarantee.
fn away_paid_otherwise(e: &Employee, date: &str) -> bool {
    e.sick_until.as_deref().is_some_and(|u| dates::between(date, u) >= 0) || e.on_holiday(date).is_some()
}

/// What the drivers' pay of a day came to (the day's report shows it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paid {
    /// Hours paid for duties, of them overtime, and hours paid by the week's guarantee.
    pub hours: f64,
    pub overtime: f64,
    pub guaranteed: f64,
    /// What it cost, with the on-costs and allowances.
    pub cost: Cents,
}

/// What a driver's minutes cost with the on-costs and allowances (`over`: of them overtime).
fn cost_of(e: &Employee, minutes: f64, over: f64) -> f64 {
    staff::hourly(e.wage) * (minutes - over + over * econ_uk::OVERTIME_RATE) / 60.0 * (1.0 + EMPLOYER_SHARE)
}

/// The drivers' day (`day::close_day`, before the night): `pieces` are the pieces of work each
/// driver of the staff had (id, from, to: minutes of the day). Every driver there that day earns
/// a seventh of the week's guarantee; the hours of their duties are paid and booked, overtime
/// above the contracted week; and on the week's last day the guarantee pays who drove less
/// (`week`).
pub fn day(c: &mut Company, date: &str, pieces: &[(u32, i32, i32)]) -> Paid {
    let p = c.planning.flow.prefs.clone();
    let overtime = econ_uk::mode(c.game_mode).overtime;
    let contract = contract_week_minutes();
    let mut out = Paid::default();
    let mut cost = 0.0;
    let mut drivers = 0;
    for e in c.staff.iter_mut().filter(|e| staff::drives(e) && e.employed_on(date)) {
        if !away_paid_otherwise(e, date) {
            e.week_due += contract / 7.0;
        }
        let mine: Vec<(i32, i32)> = pieces.iter().filter(|x| x.0 == e.id).map(|x| (x.1, x.2)).collect();
        let paid = paid_minutes(&p, &mine) as f64;
        if paid <= 0.0 {
            continue;
        }
        let before = e.week_paid;
        let after = before + paid;
        let over = if overtime { (after - contract).max(0.0) - (before - contract).max(0.0) } else { 0.0 };
        cost += cost_of(e, paid, over);
        e.week_paid = after;
        out.hours += paid / 60.0;
        out.overtime += over / 60.0;
        drivers += 1;
    }
    let booked = cost.round() as Cents;
    if booked > 0 {
        c.book(BookingKind::Wages, -booked, format!("Drivers: paid hours of the day ({drivers}, {:.0} h)", out.hours), false);
    }
    out.cost += booked;
    // (the week ends tonight: its guarantee)
    let tomorrow = dates::add(date, 1);
    if dates::parse(&tomorrow).map(dates::weekday) == Some(0) {
        let w = week(c);
        out.guaranteed += w.guaranteed;
        out.cost += w.cost;
    }
    out
}

/// The week's end: every driver paid the guaranteed hours they did not drive (booked together),
/// and a new week begun for everyone.
pub fn week(c: &mut Company) -> Paid {
    let mut out = Paid::default();
    let mut cost = 0.0;
    let mut drivers = 0;
    for e in c.staff.iter_mut() {
        let short = (e.week_due - e.week_paid).max(0.0);
        if staff::drives(e) && short > 0.5 {
            cost += cost_of(e, short, 0.0);
            out.guaranteed += short / 60.0;
            drivers += 1;
        }
        e.week_paid = 0.0;
        e.week_due = 0.0;
    }
    let booked = cost.round() as Cents;
    if booked > 0 {
        c.book(BookingKind::Wages, -booked, format!("Drivers: guaranteed hours of the week ({drivers}, {:.0} h)", out.guaranteed), false);
    }
    out.cost = booked;
    out
}

/// A driver who leaves: the guaranteed hours of their week not driven, paid (booked with their
/// name). Returns what it cost.
pub fn settle(c: &mut Company, id: u32) -> Cents {
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return 0 };
    let short = (e.week_due - e.week_paid).max(0.0);
    let cost = if staff::drives(e) { cost_of(e, short, 0.0).round() as Cents } else { 0 };
    (e.week_paid, e.week_due) = (0.0, 0.0);
    let name = e.name.clone();
    if cost > 0 {
        c.book(BookingKind::Wages, -cost, name, false);
    }
    cost
}

/// What a paid hour of a driver costs the company today, all in: the market's wage of a driver
/// of some experience (`economy::market_wage`) by the hour, with the on-costs and allowances.
pub fn driver_hour(c: &Company) -> f64 {
    let wage = economy::market_wage(45.0, &econ_uk::Level::of(c));
    staff::hourly(economy::employer_cost(wage))
}

/// An agency driver's hour today: a premium on a driver's (`econ_uk::AGENCY_AHEAD` booked ahead,
/// `AGENCY_SAME_DAY` sent on the day itself).
pub fn agency_hour(c: &Company, ahead: bool) -> f64 {
    driver_hour(c) * if ahead { econ_uk::AGENCY_AHEAD } else { econ_uk::AGENCY_SAME_DAY }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_paid_from_signing_on_to_signing_off() {
        let p = Prefs::default();
        // one piece 06:00-13:50: signed on 10 minutes before, off 5 after
        assert_eq!(paid_minutes(&p, &[(360, 830)]), 470 + p.sign_on + p.sign_off);
        // a short one is paid the floor
        assert_eq!(paid_minutes(&p, &[(600, 660)]), rosterflow::paid_floor(&p));
        // two pieces with a short break between: the break is paid
        assert_eq!(paid_minutes(&p, &[(360, 600), (630, 840)]), 480 + p.sign_on + p.sign_off);
        // a split: the gap unpaid but its signing
        let split = paid_minutes(&p, &[(360, 540), (900, 1080)]);
        assert_eq!(split, 360 + p.sign_on + p.sign_off + p.sign_off_split + p.sign_on_split);
        assert_eq!(paid_minutes(&p, &[]), 0);
    }
}

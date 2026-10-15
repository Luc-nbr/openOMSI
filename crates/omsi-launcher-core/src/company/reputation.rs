//! The company's reputation: every change goes through `change`, which keeps a small log of
//! what raised or lowered it (the Reputation page reads it) and keeps it from being ruined by
//! a bad day (Luc 2026-10-15: "one little flaw can just ruin the entire company. It should be
//! way less harsh").
//!
//! - **Small events, small effects:** one dropped trip, one late trip or one breakdown costs
//!   well under a point (the numbers are with their causes: `day`, `incidents`, `fares` ...).
//! - **Caps:** what a day and a week may cost at most (`Mode::day_cap`, `week_cap`); a loss
//!   past them is not taken.
//! - **Recovery:** a decent day (few trips dropped, punctuality fair) brings it back towards
//!   `BASELINE` a little; a week and a month without a dropped trip earn a bonus.
//! - **Game modes:** Casual loses half, Normal four fifths and Realistic the full amount, with
//!   caps to match; Casual also recovers the fastest.
//! - **What it drives** is softened below 40 (`effective`): the passengers (`demand_factor`)
//!   and the authority's payment (`economy::compensation_per_km`) do not spiral down with it.

use super::day::DayReport;
use super::incidents::Tally;
use super::model::{Company, GameMode};
use serde::{Deserialize, Serialize};

/// Where the reputation goes back to on decent days.
pub const BASELINE: f64 = 50.0;
/// Days of log kept.
pub const KEEP_DAYS: i64 = 35;

/// What moved the reputation.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    Dropped,
    Punctual,
    Late,
    Crowded,
    Incidents,
    Breakdowns,
    BusesClean,
    Depot,
    Fares,
    Internet,
    Cleaning,
    ServiceCentre,
    Comfort,
    Recovery,
    Streak,
    Complaint,
    #[serde(other)]
    Other,
}

impl Cause {
    /// The words of the log's line (`%{n}`: the count).
    pub fn label(self) -> &'static str {
        match self {
            Cause::Dropped => "%{n} trips dropped",
            Cause::Punctual => "Trips on time",
            Cause::Late => "Too many trips late",
            Cause::Crowded => "Full buses, passengers left behind",
            Cause::Incidents => "Accidents, complaints and passengers taken ill",
            Cause::Breakdowns => "%{n} buses broke down",
            Cause::BusesClean => "How clean the buses are",
            Cause::Depot => "How clean the depot yard is",
            Cause::Fares => "Fares far above the association's",
            Cause::Internet => "WLAN on the buses",
            Cause::Cleaning => "The cleaning company",
            Cause::ServiceCentre => "The service centre answered complaints",
            Cause::Comfort => "Air conditioning and better stops",
            Cause::Recovery => "A decent day: back towards normal",
            Cause::Streak => "%{n} days without a dropped trip",
            Cause::Complaint => "Passenger complaints answered",
            Cause::Other => "Other",
        }
    }
}

/// A day's change of one cause.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub date: String,
    pub cause: Cause,
    pub change: f64,
    #[serde(default)]
    pub count: u32,
    /// What a cap kept from it (points).
    #[serde(default)]
    pub capped: f64,
}

/// The log, the days without a dropped trip and the reputation at each day's close.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Log {
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub days_without_drops: u32,
    #[serde(default)]
    pub history: Vec<(String, f64)>,
}

/// How a game mode treats the reputation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode {
    /// Losses are taken times this.
    pub loss: f64,
    pub day_cap: f64,
    pub week_cap: f64,
    /// The share of the gap to `BASELINE` a decent day closes.
    pub recovery: f64,
}

pub fn mode(m: GameMode) -> Mode {
    match m {
        GameMode::Casual => Mode { loss: 0.5, day_cap: 1.0, week_cap: 3.0, recovery: 0.04 },
        GameMode::Normal => Mode { loss: 0.8, day_cap: 1.5, week_cap: 5.0, recovery: 0.025 },
        GameMode::Realistic => Mode { loss: 1.0, day_cap: 2.5, week_cap: 8.0, recovery: 0.015 },
    }
}

/// The losses logged on `date` and in the week up to it (points, positive).
fn losses(c: &Company, date: &str) -> (f64, f64) {
    let mut day = 0.0;
    let mut week = 0.0;
    for e in c.reputation_log.entries.iter().filter(|e| e.change < 0.0) {
        let ago = super::dates::between(&e.date, date);
        if ago == 0 {
            day -= e.change;
        }
        if (0..7).contains(&ago) {
            week -= e.change;
        }
    }
    (day, week)
}

/// Move the reputation by `amount` for `cause` on `date` (`count`: how many of it, for the
/// log): a loss is taken times the mode's share and within its caps. Returns the change taken.
pub fn change(c: &mut Company, date: &str, cause: Cause, amount: f64, count: u32) -> f64 {
    if !amount.is_finite() || amount.abs() < 1e-9 {
        return 0.0;
    }
    let m = mode(c.game_mode);
    let mut want = amount;
    let mut capped = 0.0;
    if amount < 0.0 {
        want = amount * m.loss;
        let (day, week) = losses(c, date);
        let room = (m.day_cap - day).min(m.week_cap - week).max(0.0);
        if -want > room {
            capped = -want - room;
            want = -room;
        }
    }
    let before = c.reputation;
    c.reputation = (c.reputation + want).clamp(0.0, 100.0);
    let taken = c.reputation - before;
    log(c, date, cause, taken, count, capped);
    taken
}

fn log(c: &mut Company, date: &str, cause: Cause, taken: f64, count: u32, capped: f64) {
    if taken.abs() < 1e-9 && capped <= 0.0 {
        return;
    }
    let l = &mut c.reputation_log;
    match l.entries.iter_mut().find(|e| e.date == date && e.cause == cause && (e.change < 0.0) == (taken < 0.0)) {
        Some(e) => {
            e.change += taken;
            e.count += count;
            e.capped += capped;
        }
        None => l.entries.push(Entry { date: date.to_string(), cause, change: taken, count, capped }),
    }
    l.entries.retain(|e| super::dates::between(&e.date, date) < KEEP_DAYS);
}

/// The reputation as what it drives sees it: below 40 only half of what is lost counts (so a
/// low reputation does not spiral the passengers and the payment down).
pub fn effective(reputation: f64) -> f64 {
    let r = reputation.clamp(0.0, 100.0);
    if r < 40.0 {
        40.0 - (40.0 - r) * 0.5
    } else {
        r
    }
}

/// How many ride for the reputation: 0.91 at 0, 1 at 30-ish, 1.15 at 100 (was 0.85 to 1.15).
pub fn demand_factor(reputation: f64) -> f64 {
    0.85 + 0.3 * effective(reputation) / 100.0
}

/// The day's close (its trips, punctuality and incidents; `report` has its drops, crowding and
/// breakdowns): what they do to the reputation, the recovery and the streaks. Returns the
/// change taken.
pub fn close_day(c: &mut Company, date: &str, report: &DayReport, punctuality: Option<f64>, tally: &Tally) -> f64 {
    let before = c.reputation;
    // trips dropped: 0.06 a trip, at most 1.2 (was 0.25, at most 5)
    if report.dropped > 0 {
        change(c, date, Cause::Dropped, -(0.06 * report.dropped as f64).min(1.2), report.dropped);
    }
    // full buses and people left behind (was 0.01 and 0.002, at most 1.5)
    let crowd = (0.004 * report.crowded as f64 + 0.0008 * report.left_behind as f64).min(0.5);
    if crowd > 0.0 {
        change(c, date, Cause::Crowded, -crowd, report.crowded);
    }
    // punctuality: +0.15 for a day on time without drops (was +0.3); -0.15 below 75 %, -0.25
    // below 60 % (was -0.3)
    if let Some(p) = punctuality {
        let late = report.late;
        if report.dropped == 0 && p >= 90.0 {
            change(c, date, Cause::Punctual, 0.15, report.trips.saturating_sub(late));
        } else if p < 60.0 {
            change(c, date, Cause::Late, -0.25, late);
        } else if p < 75.0 {
            change(c, date, Cause::Late, -0.15, late);
        }
    }
    // breakdowns: 0.05 a bus, at most 0.3 (new: before, only the trips they dropped)
    let broke = report.notes.iter().filter(|n| matches!(n, super::day::Note::Breakdown { .. })).count() as u32;
    if broke > 0 {
        change(c, date, Cause::Breakdowns, -(0.05 * broke as f64).min(0.3), broke);
    }
    let hurt = tally.reputation();
    if hurt < 0.0 {
        change(c, date, Cause::Incidents, hurt, tally.accidents.len() as u32 + tally.complaints + tally.taken_ill);
    }
    // the days without a dropped trip: a week +0.3, a month +1
    let ran = report.trips > 0;
    if ran && report.dropped == 0 {
        c.reputation_log.days_without_drops += 1;
        let n = c.reputation_log.days_without_drops;
        if n % 30 == 0 {
            change(c, date, Cause::Streak, 1.0, n);
        } else if n % 7 == 0 {
            change(c, date, Cause::Streak, 0.3, n);
        }
    } else if report.dropped > 0 {
        c.reputation_log.days_without_drops = 0;
    }
    // a decent day brings it back towards the baseline
    let decent = ran && report.dropped as f64 <= (report.trips as f64 * 0.02).max(1.0) && punctuality.is_none_or(|p| p >= 80.0);
    if decent && c.reputation < BASELINE {
        let m = mode(c.game_mode);
        change(c, date, Cause::Recovery, ((BASELINE - c.reputation) * m.recovery).max(0.05).min(BASELINE - c.reputation), 0);
    }
    // the day's reputation, for the trend
    let rep = c.reputation;
    let h = &mut c.reputation_log.history;
    h.retain(|(d, _)| d != date);
    h.push((date.to_string(), (rep * 10.0).round() / 10.0));
    let n = h.len();
    if n > KEEP_DAYS as usize {
        h.drain(..n - KEEP_DAYS as usize);
    }
    c.reputation - before
}

/// The reputation's move over the last `days` closed days (None: too few days yet).
pub fn trend(c: &Company, days: usize) -> Option<f64> {
    let h = &c.reputation_log.history;
    let last = h.last()?;
    let k = h.len().checked_sub(days + 1)?;
    Some(last.1 - h[k].1)
}

/// What a cause did over the last `days` days up to `date`: change and count.
pub fn lately(c: &Company, date: &str, days: i64) -> Vec<(Cause, f64, u32)> {
    let mut out: Vec<(Cause, f64, u32)> = Vec::new();
    for e in c.reputation_log.entries.iter().filter(|e| (0..days).contains(&super::dates::between(&e.date, date))) {
        match out.iter_mut().find(|x| x.0 == e.cause) {
            Some(x) => {
                x.1 += e.change;
                x.2 += e.count;
            }
            None => out.push((e.cause, e.change, e.count)),
        }
    }
    out.retain(|x| x.1.abs() >= 0.005);
    out.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
    out
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;

    fn company(m: GameMode) -> Company {
        found(&Founding { name: "Rep".into(), game_mode: m, date: "2024-03-04".into(), ..Default::default() }, "Luc")
    }

    fn day(trips: u32, dropped: u32, late: u32) -> DayReport {
        DayReport { trips, dropped, late, ..Default::default() }
    }

    #[test]
    fn a_single_bad_day_costs_only_a_few_points() {
        // a terrible day: 40 trips dropped, punctuality 50 %, an accident and complaints
        for m in [GameMode::Casual, GameMode::Normal, GameMode::Realistic] {
            let mut c = company(m);
            let tally = Tally { accidents: vec![("1".into(), 0)], complaints: 20, taken_ill: 2, ..Default::default() };
            let mut r = day(200, 40, 100);
            r.crowded = 50;
            r.left_behind = 400;
            let taken = close_day(&mut c, "2024-03-04", &r, Some(50.0), &tally);
            assert!(taken < 0.0 && -taken <= mode(m).day_cap + 1e-9, "{m:?}: {taken}");
            // (the old numbers took 5 + 1.5 + 0.3 + 1.2 = 8 points)
        }
        // Casual the mildest
        let lost = |m| {
            let mut c = company(m);
            close_day(&mut c, "2024-03-04", &day(100, 3, 10), Some(80.0), &Tally::default())
        };
        assert!(lost(GameMode::Casual) > lost(GameMode::Normal) && lost(GameMode::Normal) > lost(GameMode::Realistic));
        // one dropped trip, one breakdown: well under a point
        let mut c = company(GameMode::Realistic);
        let r = day(100, 1, 0);
        let one = close_day(&mut c, "2024-03-04", &r, Some(95.0), &Tally::default());
        assert!(one < 0.0 && one > -0.2, "{one}");
        // a week of bad days stops at the week's cap
        let mut c = company(GameMode::Normal);
        for k in 0..7 {
            close_day(&mut c, &super::super::dates::add("2024-03-04", k), &day(200, 40, 100), Some(50.0), &Tally::default());
        }
        assert!(c.reputation >= BASELINE - mode(GameMode::Normal).week_cap - 1e-9, "{}", c.reputation);
        // the log says what it was
        assert!(c.reputation_log.entries.iter().any(|e| e.cause == Cause::Dropped && e.count == 40));
        assert!(c.reputation_log.entries.iter().any(|e| e.capped > 0.0), "a cap kept some of it");
    }

    #[test]
    fn decent_days_bring_it_back_towards_the_baseline() {
        let mut c = company(GameMode::Normal);
        c.reputation = 20.0;
        let mut date = "2024-03-04".to_string();
        for _ in 0..30 {
            close_day(&mut c, &date, &day(100, 0, 3), Some(97.0), &Tally::default());
            date = super::super::dates::add(&date, 1);
        }
        assert!(c.reputation > 30.0 && c.reputation <= 60.0, "{}", c.reputation);
        assert!(trend(&c, 7).is_some_and(|t| t > 0.0));
        // a week and a month without a drop: a bonus
        assert!(c.reputation_log.entries.iter().any(|e| e.cause == Cause::Streak && e.count == 30));
        assert!(lately(&c, &c.date.clone(), 7).len() <= Cause::Other as usize + 1);
        // above the baseline it does not recover further by itself
        let mut c = company(GameMode::Normal);
        c.reputation = 70.0;
        close_day(&mut c, "2024-03-04", &day(100, 1, 30), Some(70.0), &Tally::default());
        assert!(c.reputation < 70.0);
        // what it drives is softened below 40
        assert!(demand_factor(0.0) > 0.9 && (demand_factor(50.0) - 1.0).abs() < 1e-9);
    }
}

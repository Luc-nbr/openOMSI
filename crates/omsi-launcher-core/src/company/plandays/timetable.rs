//! A line's timetable of one day copied to other days, and the templates (testers: "I can't
//! copy a timetable from MF to SA or SU"; "a template for the given day so that it can be used
//! on more days"; "copy the timetable that you've created for one day ... over to another day
//! easily"): the Timeplan's "Copy to…" in its timetable.
//!
//! A plan's copy (`copy`, `copy_rows`) lays a day's buses and duties onto the trips the other
//! day's timetable has: Saturday and Sunday run a timetable of their own, so a weekday's plan
//! found next to nothing to match there - and nothing copied the trips themselves. This does: a
//! line's timetable of one day group (`lines::DAY_GROUPS`: Monday to Friday, Saturday, Sunday
//! with the public holidays) onto others, in place of theirs or added to them. Saved and
//! refreshed, the day groups run the same trips, and the plan copies onto them as onto a day of
//! the same timetable.
//!
//! A copy's source is a `Template`: the day shown (`Template::of_day`), the trips chosen there
//! (`of_trips`), or one kept by name with the company (`Days::templates`) to copy again later -
//! a template is a saved source. "Replace" makes a day what the source is: its trips, and the
//! running times they were made with (so no time of theirs shows as typed over); "Add" puts the
//! trips beside the day's own, a trip there already the same way at the same minute not twice.
//!
//! The copies are the player's own trips of the group they are copied to. A trip of a map's tour
//! runs on the days of its tour's mask whatever group it says (`lines::trip_in_group`): a copy
//! that kept its tour - or a trip merely given the weekend's group - would still run on weekdays
//! only. A group's timetable replaced, the map's tours that ran then run on their other days only
//! (their mask without the group's days).

use super::super::model::Company;
use super::super::timeplan::{self as tp, RunPlan, RunSet};
use crate::lines::{self as reg, LineDesign, TableTrip, DAY_GROUPS};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The longest name a template takes.
const NAME_MAX: usize = 48;

/// A trip of a copy's source: its direction (a pattern of the line) and its time at every stop.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Trip {
    pub dir: usize,
    pub times: Vec<f32>,
}

impl Trip {
    fn departure(&self) -> f32 {
        self.times.first().copied().unwrap_or(0.0)
    }
}

/// What "Copy to…" copies: a line's timetable of a day - its trips and the running times they
/// were made with - taken from the day shown, the trips chosen there, or kept by name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Template {
    /// Its number among the company's templates (0: not kept - the day shown or the trips
    /// chosen, never copied onto the day they are of).
    pub id: u32,
    pub name: String,
    /// The company line it is of (its timetable name) and the day group it was taken from.
    pub line: String,
    pub day: usize,
    pub trips: Vec<Trip>,
    /// The running times of that day (`timeplan::RunSet`s of its `day`; none: the directions'
    /// own times).
    pub runs: Vec<RunSet>,
}

impl Template {
    /// Day group `day` of `l` (the company line `line`) as a copy's source: every trip of it,
    /// both sides and every pattern.
    pub fn of_day(l: &LineDesign, line: &str, day: usize) -> Template {
        let all: Vec<usize> = (0..l.table.len()).filter(|&i| tp::in_day(l, &l.table[i], day)).collect();
        Template::of_trips(l, line, day, &all)
    }

    /// The trips `picked` of `l` (places in its table) as a copy's source, with the running
    /// times of the day group `day` they are shown on.
    pub fn of_trips(l: &LineDesign, line: &str, day: usize, picked: &[usize]) -> Template {
        let mut picked = picked.to_vec();
        picked.sort_unstable();
        picked.dedup();
        let mut trips: Vec<Trip> = picked.iter().filter(|&&i| l.table.get(i).is_some_and(|t| !t.times.is_empty())).map(|&i| Trip { dir: l.table[i].dir, times: tp::trip(l, i) }).collect();
        trips.sort_by(|a, b| a.departure().total_cmp(&b.departure()).then(a.dir.cmp(&b.dir)));
        let runs = tp::plan_of(l).sets.into_iter().filter(|s| s.day == day).collect();
        Template { id: 0, name: String::new(), line: line.to_string(), day, trips, runs }
    }
}

/// How a copy goes onto a day: in place of its timetable, or added to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Replace,
    Add,
}

/// What a copy did on one day group.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayCopied {
    pub day: usize,
    /// The trips copied there.
    pub added: usize,
    /// The day's trips that went (Replace).
    pub removed: usize,
    /// Trips there already, the same way at the same minute (Add): not copied twice.
    pub twins: usize,
    /// Trips of a pattern the line no longer has (a template older than its stops).
    pub lost: usize,
    /// The line's kind of service does not run on the day (a school line on a Saturday):
    /// nothing copied.
    pub closed: bool,
}

/// The days of day group `day` as a tour's mask has them.
fn bits(day: usize) -> i32 {
    DAY_GROUPS.get(day).map_or(0, |g| i32::from(g.1))
}

/// The numbers of the map's tours of `l` (a line of the player's has none).
fn map_tours(l: &LineDesign) -> HashSet<String> {
    l.map.iter().flat_map(|m| m.tours.iter().map(|t| t.number.trim().to_string())).filter(|n| !n.is_empty()).collect()
}

/// The trips of `l` on day group `day`.
pub fn trips_on(l: &LineDesign, day: usize) -> usize {
    l.table.iter().filter(|t| !t.times.is_empty() && tp::in_day(l, t, day)).count()
}

/// Day group `day` of `l` emptied: the player's trips of it taken out, and the map's tours
/// that ran then run on their other days only (a tour left with no day goes with its trips).
/// Returns how many trips the day had.
pub fn clear_day(l: &mut LineDesign, day: usize) -> usize {
    let had = trips_on(l, day);
    let tours = map_tours(l);
    let of_tour = |t: &TableTrip| tours.contains(t.tour.trim());
    l.table.retain(|t| of_tour(t) || t.day != day);
    let service = l.service;
    let runs_on = |mask: i32, g: usize| service.allows(g) && reg::kept_mask(service, mask) & bits(g) != 0;
    let mut dead: HashSet<String> = HashSet::new();
    if let Some(m) = l.map.as_mut() {
        // (only the tours that ran on the day: the others stay as the map has them)
        for mt in m.tours.iter_mut().filter(|mt| runs_on(mt.mask, day)) {
            mt.mask &= !bits(day);
            if !(0..DAY_GROUPS.len()).any(|g| runs_on(mt.mask, g)) {
                dead.insert(mt.number.trim().to_string());
            }
        }
    }
    l.table.retain(|t| !dead.contains(t.tour.trim()));
    had
}

/// `src` copied onto day group `day` of `l`, `how`.
fn put_day(l: &mut LineDesign, src: &Template, day: usize, how: How) -> DayCopied {
    let mut made = DayCopied { day, ..Default::default() };
    if day >= DAY_GROUPS.len() || !l.service.allows(day) {
        made.closed = true;
        return made;
    }
    tp::as_table(l);
    if how == How::Replace {
        made.removed = clear_day(l, day);
        // (the day's running times the source's: its trips as the source has them, none of
        // their times an override there)
        let mut plan = tp::plan_of(l);
        plan.sets.retain(|s| s.day != day);
        plan.sets.extend(src.runs.iter().filter(|s| s.dir < l.directions.len()).map(|s| RunSet { day, ..s.clone() }));
        if l.timeplan.is_some() || plan != RunPlan::default() {
            l.timeplan = Some(plan);
        }
    }
    for x in &src.trips {
        if x.times.is_empty() || l.directions.get(x.dir).is_none_or(|d| d.stops.len() < 2) {
            made.lost += 1;
            continue;
        }
        let dep = x.departure();
        if l.table.iter().any(|t| t.dir == x.dir && !t.times.is_empty() && (t.departure() - dep).abs() < 0.5 && tp::in_day(l, t, day)) {
            made.twins += 1;
            continue;
        }
        l.table.push(TableTrip { dir: x.dir, day, times: x.times.clone(), tour: String::new() });
        made.added += 1;
    }
    made
}

/// "Copy to…": `src` onto each of the day groups `days` of `l`, in place of their timetable or
/// added to it (a source not kept never onto the day it is of). See the module.
pub fn copy_table(l: &mut LineDesign, src: &Template, days: &[usize], how: How) -> Vec<DayCopied> {
    let mut days: Vec<usize> = days.iter().copied().filter(|&d| d < DAY_GROUPS.len() && (src.id != 0 || d != src.day)).collect();
    days.sort_unstable();
    days.dedup();
    days.into_iter().map(|d| put_day(l, src, d, how)).collect()
}

// --- the templates kept -----------------------------------------------------------------------------

/// A name as a template keeps it: trimmed, one line, not too long (None: empty).
fn clean_name(name: &str) -> Option<String> {
    let n: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX).collect();
    (!n.is_empty()).then_some(n)
}

/// The templates of the company line `line` (its timetable name), as they were kept.
pub fn templates_of<'a>(c: &'a Company, line: &str) -> Vec<&'a Template> {
    c.planning.flow.days.templates.iter().filter(|t| t.line.trim().eq_ignore_ascii_case(line.trim())).collect()
}

/// The template numbered `id`.
pub fn template(c: &Company, id: u32) -> Option<&Template> {
    c.planning.flow.days.templates.iter().find(|t| t.id == id)
}

/// `t` kept by the name `name` under a number of its own - in the place of the line's template
/// of that name, if it has one. Returns the number (None: no name, or no trip to keep).
pub fn save_template(c: &mut Company, mut t: Template, name: &str) -> Option<u32> {
    t.name = clean_name(name)?;
    if t.trips.is_empty() {
        return None;
    }
    let ts = &mut c.planning.flow.days.templates;
    let same = |x: &Template| x.line.trim().eq_ignore_ascii_case(t.line.trim()) && x.name.eq_ignore_ascii_case(&t.name);
    match ts.iter().position(same) {
        Some(k) => {
            t.id = ts[k].id;
            ts[k] = t;
            Some(ts[k].id)
        }
        None => {
            t.id = ts.iter().map(|x| x.id).max().unwrap_or(0) + 1;
            let id = t.id;
            ts.push(t);
            Some(id)
        }
    }
}

/// Template `id` named `name`. Returns whether it was (no such template, or no name: not).
pub fn rename_template(c: &mut Company, id: u32, name: &str) -> bool {
    let Some(n) = clean_name(name) else { return false };
    match c.planning.flow.days.templates.iter_mut().find(|t| t.id == id) {
        Some(t) => {
            t.name = n;
            true
        }
        None => false,
    }
}

/// Template `id` thrown away. Returns whether there was one.
pub fn drop_template(c: &mut Company, id: u32) -> bool {
    let ts = &mut c.planning.flow.days.templates;
    let before = ts.len();
    ts.retain(|t| t.id != id);
    before != ts.len()
}

/// The templates of line `from` the line's now under its new name `to` (its number changed).
pub fn move_templates(c: &mut Company, from: &str, to: &str) {
    for t in c.planning.flow.days.templates.iter_mut().filter(|t| t.line.trim().eq_ignore_ascii_case(from.trim())) {
        t.line = to.to_string();
    }
}

#[cfg(test)]
mod tests;

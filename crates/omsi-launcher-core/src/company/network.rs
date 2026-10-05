//! The company's lines: lines of the map's timetable it takes on, and the player's own from
//! the line editor (they are `.ttl` lines of the timetable too, see `crate::lines`). What a
//! line asks of the company on a day are its tours of that day, as the timetable has them -
//! each a bus from its first trip to its last, cut into duties for the drivers where it
//! stands long enough (Omsi-Hub's `omlopenVanDag` and `knipOmloop`).

use super::dates;
use super::model::{BusSize, Company, CompanyLine};
use super::staff::{DUTY_MAX, SPLIT_PAUSE};
use crate::lines::OwnLine;
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// A trip as the day plans it: minutes of the day.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PlannedTrip {
    pub name: String,
    /// The number its displays show.
    pub line: String,
    pub from: String,
    pub to: String,
    pub dep: i32,
    pub arr: i32,
    pub km: f64,
    pub stops: u32,
}

impl PlannedTrip {
    /// A trip with passengers (a depot run or a short positioning trip has fewer than three
    /// stops and carries none: a duty is not cut before one).
    pub fn counts(&self) -> bool {
        self.stops >= 3
    }
}

/// A tour of a company line on the day.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct TourOfDay {
    /// The company line (its timetable name) and the number shown.
    pub line: String,
    pub number: String,
    pub tour: String,
    pub ai_group: String,
    pub trips: Vec<PlannedTrip>,
}

impl TourOfDay {
    pub fn from(&self) -> i32 {
        self.trips.first().map(|t| t.dep).unwrap_or(0)
    }

    pub fn to(&self) -> i32 {
        self.trips.iter().map(|t| t.arr).max().unwrap_or(0)
    }

    pub fn km(&self) -> f64 {
        self.trips.iter().map(|t| t.km).sum()
    }

    /// The bus size its depot group asks for, if it names one (`vormVanDepot`).
    pub fn wants(&self) -> Option<BusSize> {
        wants_size(&self.ai_group)
    }
}

/// The bus size an AI group or depot name asks for ("Gelenkbus", "Solo", "DD").
pub fn wants_size(group: &str) -> Option<BusSize> {
    let g = group.to_lowercase();
    let word = |w: &str| g.split(|c: char| !c.is_alphanumeric()).any(|x| x == w);
    if g.contains("gelenk") || g.contains("schlenk") || g.contains("artic") {
        Some(BusSize::Articulated)
    } else if g.contains("doppeldeck") || word("dd") {
        Some(BusSize::Double)
    } else if g.contains("midi") || g.contains("kurz") {
        Some(BusSize::Midi)
    } else if g.contains("solo") || g.contains("standard") {
        Some(BusSize::Solo)
    } else {
        None
    }
}

/// Take on a line of the map's timetable (`own`: the player's line it is, from the line
/// editor's registry).
pub fn add_line(c: &mut Company, line: &LineInfo, own: Option<&OwnLine>) -> Result<(), &'static str> {
    if c.lines.iter().any(|l| l.name.eq_ignore_ascii_case(&line.name)) {
        return Err("The company runs this line already.");
    }
    let mut numbers: Vec<String> = Vec::new();
    for t in line.tours.iter().flat_map(|t| t.trips.iter()) {
        let n = t.line.trim();
        if !n.is_empty() && !numbers.iter().any(|x| x == n) {
            numbers.push(n.to_string());
        }
    }
    let number = match own {
        Some(o) if !o.number.trim().is_empty() => o.number.trim().to_string(),
        _ => numbers.first().cloned().unwrap_or_else(|| line.name.clone()),
    };
    let caption = own.map(|o| o.caption()).filter(|c| !c.is_empty()).unwrap_or_else(|| line.termini.join(" – "));
    let runs: Vec<_> = line.tours.iter().filter(|t| t.runs).collect();
    c.lines.push(CompanyLine {
        name: line.name.clone(),
        number,
        numbers,
        own: own.is_some(),
        colour: own.map(|o| o.colour.clone()).unwrap_or_default(),
        caption,
        added: c.date.clone(),
        tours: runs.len() as u32,
        km: runs.iter().flat_map(|t| t.trips.iter()).map(|t| t.km).sum(),
    });
    Ok(())
}

pub fn remove_line(c: &mut Company, name: &str) {
    c.lines.retain(|l| !l.name.eq_ignore_ascii_case(name));
}

/// The company line a trip report names (by the number its displays showed).
pub fn line_of_number<'a>(c: &'a Company, number: &str) -> Option<&'a CompanyLine> {
    let n = number.trim();
    if n.is_empty() {
        return None;
    }
    c.lines.iter().find(|l| l.number.eq_ignore_ascii_case(n) || l.numbers.iter().any(|x| x.eq_ignore_ascii_case(n)) || l.name.eq_ignore_ascii_case(n))
}

/// The tours of the company's lines on the day `lines` were read for (the timetable's lines
/// of that date): those that run that day, their trips in order of departure.
pub fn tours_of_day(c: &Company, lines: &[LineInfo]) -> Vec<TourOfDay> {
    let mut out = Vec::new();
    for cl in &c.lines {
        let Some(line) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&cl.name)) else { continue };
        for t in line.tours.iter().filter(|t| t.runs) {
            let mut trips: Vec<PlannedTrip> = t
                .trips
                .iter()
                .map(|x| PlannedTrip {
                    name: x.name.clone(),
                    line: if x.line.trim().is_empty() { cl.number.clone() } else { x.line.trim().to_string() },
                    from: x.from.clone(),
                    to: x.terminus.clone(),
                    dep: (x.departure / 60.0).round() as i32,
                    arr: (x.arrival / 60.0).round() as i32,
                    km: x.km,
                    stops: x.stops.len() as u32,
                })
                .collect();
            trips.sort_by_key(|x| x.dep);
            if trips.iter().any(PlannedTrip::counts) {
                out.push(TourOfDay { line: cl.name.clone(), number: cl.number.clone(), tour: t.number.clone(), ai_group: t.ai_group.clone(), trips });
            }
        }
    }
    out.sort_by_key(|t| t.from());
    out
}

/// What the timetable gives each company line on that day: tours and kilometres (kept with
/// the line for the pages).
pub fn refresh_lines(c: &mut Company, lines: &[LineInfo]) {
    for cl in c.lines.iter_mut() {
        if let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&cl.name)) {
            let runs: Vec<_> = l.tours.iter().filter(|t| t.runs).collect();
            cl.tours = runs.len() as u32;
            cl.km = runs.iter().flat_map(|t| t.trips.iter()).map(|t| t.km).sum();
        }
    }
}

/// A tour cut into duties of at most `max` minutes (`knipOmloop`): as many parts as needed,
/// the cuts as near an even split as can be, only before a trip with passengers after the bus
/// stood at least `SPLIT_PAUSE` minutes (there a driver can take over). Where there is no
/// such place it is not cut; a part without a trip with passengers goes into the one before.
pub fn split_tour(trips: &[PlannedTrip], max: i32) -> Vec<std::ops::Range<usize>> {
    if trips.is_empty() {
        return Vec::new();
    }
    let begin = trips[0].dep;
    let span = trips.iter().map(|t| t.arr).max().unwrap_or(begin) - begin;
    let parts = ((span as f64 / max as f64).ceil() as i32).max(1);
    let mut cuts: Vec<usize> = Vec::new();
    for k in 1..parts {
        let ideal = begin as f64 + span as f64 * k as f64 / parts as f64;
        let mut best: Option<usize> = None;
        for i in 1..trips.len() {
            if i <= cuts.last().copied().unwrap_or(0) {
                continue;
            }
            if !trips[i].counts() || trips[i].dep - trips[i - 1].arr < SPLIT_PAUSE {
                continue;
            }
            if best.is_none_or(|b| (trips[i].dep as f64 - ideal).abs() < (trips[b].dep as f64 - ideal).abs()) {
                best = Some(i);
            }
        }
        if let Some(b) = best {
            cuts.push(b);
        }
    }
    let mut pieces: Vec<std::ops::Range<usize>> = Vec::new();
    let mut from = 0;
    for cut in cuts.into_iter().chain(std::iter::once(trips.len())) {
        pieces.push(from..cut);
        from = cut;
    }
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for p in pieces {
        let counts = trips[p.clone()].iter().any(PlannedTrip::counts);
        match out.last_mut() {
            Some(last) if !counts => last.end = p.end,
            _ => out.push(p),
        }
    }
    if out.len() > 1 && !trips[out[0].clone()].iter().any(PlannedTrip::counts) {
        let first = out.remove(0);
        out[0].start = first.start;
    }
    out
}

/// The duties of a tour (`DUTY_MAX`).
pub fn duties_of(t: &TourOfDay) -> Vec<std::ops::Range<usize>> {
    split_tour(&t.trips, DUTY_MAX)
}

/// The first day of the company's next week (for a page that says when the markets change).
pub fn next_monday(c: &Company) -> String {
    dates::fmt(dates::week_of(&c.date) + 7)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn trip(dep: i32, arr: i32, stops: u32) -> PlannedTrip {
        PlannedTrip { name: format!("t{dep}"), line: "5".into(), from: "A".into(), to: "B".into(), dep, arr, km: (arr - dep) as f64 * 0.3, stops }
    }

    #[test]
    fn a_long_tour_is_cut_into_duties() {
        // 5:00 to 23:00 in trips of 50 minutes with 10 minutes' layover: two parts
        let trips: Vec<PlannedTrip> = (0..18).map(|k| trip(300 + k * 60, 350 + k * 60, 12)).collect();
        let d = split_tour(&trips, DUTY_MAX);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].start, 0);
        assert_eq!(d[1].end, 18);
        // the cut lies near the middle (14:00)
        assert!((trips[d[1].start].dep - 14 * 60).abs() <= 60);
        // a short tour stays whole
        assert_eq!(split_tour(&trips[..5], DUTY_MAX), vec![0..5]);
        // no cut where the bus does not stand: one long duty
        let tight: Vec<PlannedTrip> = (0..12).map(|k| trip(300 + k * 60, 360 + k * 60, 12)).collect();
        assert_eq!(split_tour(&tight, DUTY_MAX), vec![0..12]);
        // a depot run at the end goes with the last part
        let mut with_run = trips.clone();
        with_run.push(trip(1390, 1400, 2));
        let d = split_tour(&with_run, DUTY_MAX);
        assert_eq!(d.last().unwrap().end, 19);
        assert!(split_tour(&[], DUTY_MAX).is_empty());
    }

    #[test]
    fn a_group_name_asks_for_a_size() {
        assert_eq!(wants_size("Gelenkbus"), Some(BusSize::Articulated));
        assert_eq!(wants_size("Solo_Diesel"), Some(BusSize::Solo));
        assert_eq!(wants_size("BVG DD"), Some(BusSize::Double));
        assert_eq!(wants_size("Linie 5"), None);
    }
}

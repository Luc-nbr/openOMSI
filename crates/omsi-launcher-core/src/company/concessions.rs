//! Concessions: the right to run a line of the map's timetable for a term, won in a tender
//! against other operators (the Busbetrieb-Simulator's map concessions; Omsi-Hub's
//! `Concessie` and `schrijfIn`).
//!
//! Every four weeks the authority puts some of the map's lines out to tender, each with its
//! week (tours, trips, vehicle-kilometres, and what it should bring in). The company bids a
//! price - the compensation per kilometre it asks, as a share of the authority's reference -
//! and is scored against the rivals the tender draws on price and quality (its reputation and
//! punctuality; the incumbent knows the line). The best score wins the line for the term;
//! towards its end the line is put out again (the renewal), and a concession not won again
//! ends: the line goes to the winner. On an easy economy map lines are taken on directly and
//! renewed by themselves; on Realistic and Hard adding a map line is applying for its
//! concession.
//!
//! The player's own lines from the line editor need no concession: the company runs them on
//! its own account and pays the authority's licence for each month instead (`LICENCE`).
//!
//! The price bid changes what the line earns from the authority: the day's close pays every
//! line the reference per kilometre, and `after_day` books the difference.

use super::dates;
use super::economy;
use super::model::{BookingKind, Cents, Company, CompanyLine, Difficulty};
use super::network;
use super::rng::Rng;
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// The authority's licence for one of the player's own lines, a month.
pub const LICENCE: Cents = 350_00;
/// Bids lie within this share of the reference.
pub const PRICE_MIN: f64 = 0.80;
pub const PRICE_MAX: f64 = 1.25;
/// A tender round lasts this many weeks; bids close this many days after it is offered.
pub const ROUND_WEEKS: i64 = 4;
pub const OPEN_DAYS: i64 = 14;
/// A concession is put out again this many days before it ends.
pub const RENEW_BEFORE: i64 = 42;

/// A concession the company holds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Concession {
    /// The timetable's line (its `.ttl` name) and the number shown.
    pub line: String,
    pub number: String,
    pub from: String,
    /// The last day it runs.
    pub until: String,
    /// The compensation per kilometre bid, of the authority's reference (1 = the reference).
    pub price: f64,
    /// Given without a tender (a line run before concessions, an easy economy).
    #[serde(default)]
    pub direct: bool,
}

/// A line's week, from the timetable of its seven days.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Week {
    pub tours: u32,
    /// Trips with passengers.
    pub trips: u32,
    pub km: f64,
    /// The most tours of one day (the buses the line asks for).
    pub peak: u32,
}

/// A rival's bid in a tender.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RivalBid {
    pub name: String,
    pub price: f64,
    pub quality: f64,
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Won { score: f64, rivals: Vec<RivalBid> },
    Lost { score: f64, winner: String, rivals: Vec<RivalBid> },
    /// No bid was made.
    NoBid { winner: String },
}

/// A line put out to tender.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tender {
    pub id: u32,
    pub line: String,
    pub number: String,
    pub caption: String,
    pub offered: String,
    /// Bids close at the end of this day.
    pub closes: String,
    /// The term, in weeks from the day after it closes (a renewal: from the old one's end).
    pub weeks: u32,
    /// The tours and kilometres of the day it was offered.
    pub day_tours: u32,
    pub day_km: f64,
    /// Its week, once the timetable of the seven days was read.
    #[serde(default)]
    pub week: Option<Week>,
    /// A concession of the company's put out again.
    #[serde(default)]
    pub renewal: bool,
    /// The company asked for it (the lines page's "Apply").
    #[serde(default)]
    pub applied: bool,
    /// The company's bid, of the reference.
    #[serde(default)]
    pub bid: Option<f64>,
    #[serde(default)]
    pub fee_paid: bool,
    #[serde(default)]
    pub outcome: Option<Outcome>,
}

impl Tender {
    pub fn open(&self) -> bool {
        self.outcome.is_none()
    }
}

/// The company's concessions and the market of tenders.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Concessions {
    pub held: Vec<Concession>,
    pub tenders: Vec<Tender>,
    /// The round the market was last offered in (`week / ROUND_WEEKS`).
    pub round: i64,
    pub counter: u32,
}

/// How a difficulty runs its tenders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// How many rivals bid (from, to).
    pub rivals: (i64, i64),
    /// Their prices and their quality (from, to).
    pub price: (f64, f64),
    pub quality: (f64, f64),
    /// The fee for taking part: a fixed part and one per tour of the line's day.
    pub fee_base: Cents,
    pub fee_per_tour: Cents,
    pub term_weeks: u32,
    /// Lines offered each round.
    pub offers: usize,
    /// Map lines may be taken on without a tender, and concessions renew by themselves.
    pub direct: bool,
}

pub fn rules(d: Difficulty) -> Rules {
    match d {
        Difficulty::Easy => Rules { rivals: (1, 2), price: (1.00, 1.15), quality: (25.0, 55.0), fee_base: 0, fee_per_tour: 0, term_weeks: 104, offers: 4, direct: true },
        Difficulty::Realistic => Rules { rivals: (2, 3), price: (0.92, 1.08), quality: (40.0, 70.0), fee_base: 1_500_00, fee_per_tour: 150_00, term_weeks: 52, offers: 3, direct: false },
        Difficulty::Hard => Rules { rivals: (3, 4), price: (0.86, 1.02), quality: (50.0, 80.0), fee_base: 3_000_00, fee_per_tour: 250_00, term_weeks: 52, offers: 3, direct: false },
    }
}

/// Map lines may be added without a concession's tender (an easy economy).
pub fn may_add_directly(c: &Company) -> bool {
    rules(c.difficulty).direct
}

/// The operators a tender draws from (made up).
const RIVALS: [&str; 10] = [
    "Regiobus Mitte",
    "Krüger Omnibus",
    "Nordbus Linienverkehr",
    "Stadtlinie GmbH",
    "Becker Reisen",
    "Linienverkehr Süd",
    "Busbetrieb Hansen",
    "Weststadt Verkehr",
    "Ostland Mobil",
    "Talbus",
];

/// What a bid scores: a lower price counts most, quality the rest.
pub fn score(price: f64, quality: f64) -> f64 {
    0.6 / price.max(0.5) + 0.4 * (quality / 100.0).clamp(0.0, 1.0)
}

/// How the authority sees the company's quality for a tender (the incumbent knows the line).
pub fn quality(c: &Company, renewal: bool) -> f64 {
    (0.7 * c.reputation + 0.3 * c.punctuality + if renewal { 8.0 } else { 0.0 }).clamp(0.0, 100.0)
}

/// The fee for taking part in a tender.
pub fn fee(c: &Company, t: &Tender) -> Cents {
    let r = rules(c.difficulty);
    ((r.fee_base + r.fee_per_tour * t.day_tours as Cents) as f64 * c.price_index).round() as Cents
}

/// The rivals of a tender and their bids (the same every time it is asked).
pub fn rivals(c: &Company, t: &Tender) -> Vec<RivalBid> {
    let r = rules(c.difficulty);
    let mut rng = Rng::of(&[&c.id, "tender", &t.line], t.id as i64);
    let n = rng.int(r.rivals.0, r.rivals.1).max(1) as usize;
    let mut names: Vec<&str> = RIVALS.to_vec();
    let mut out = Vec::new();
    for _ in 0..n {
        let k = rng.int(0, names.len() as i64 - 1) as usize;
        let name = names.remove(k.min(names.len() - 1));
        let price = (rng.range(r.price.0, r.price.1) * 1000.0).round() / 1000.0;
        let quality = rng.range(r.quality.0, r.quality.1).round();
        out.push(RivalBid { name: name.to_string(), price, quality, score: score(price, quality) });
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// How a tender ends, with the company's bid as it is.
pub fn decide(c: &Company, t: &Tender) -> Outcome {
    let rivals = rivals(c, t);
    let best = rivals.first().cloned();
    let Some(bid) = t.bid else { return Outcome::NoBid { winner: best.map(|b| b.name).unwrap_or_default() } };
    let mine = score(bid, quality(c, t.renewal));
    match best {
        Some(b) if b.score >= mine => Outcome::Lost { score: mine, winner: b.name.clone(), rivals },
        _ => Outcome::Won { score: mine, rivals },
    }
}

/// The authority's reference compensation per kilometre now (cents).
pub fn reference_per_km(c: &Company) -> f64 {
    economy::compensation_per_km(&economy::rules(c.difficulty), 50.0, c.contract_index)
}

/// What a line's week brings in at a price: the authority's payment and the fares.
pub fn week_revenue(c: &Company, w: &Week, price: f64) -> Cents {
    let r = economy::rules(c.difficulty);
    let comp = w.km * reference_per_km(c) * price;
    let fares = w.km * r.passengers_per_km * r.fare as f64;
    (comp + fares).round() as Cents
}

/// A line's week from the timetable of seven days (`days`: each day's lines).
pub fn week_of(days: &[Vec<LineInfo>], line: &str) -> Week {
    let mut w = Week::default();
    for day in days {
        let Some(l) = day.iter().find(|l| l.name.eq_ignore_ascii_case(line)) else { continue };
        let runs: Vec<_> = l.tours.iter().filter(|t| t.runs).collect();
        w.tours += runs.len() as u32;
        w.peak = w.peak.max(runs.len() as u32);
        for t in runs.iter().flat_map(|t| t.trips.iter()) {
            w.km += t.km;
            if t.stops.len() >= 3 {
                w.trips += 1;
            }
        }
    }
    w
}

/// The number a map line shows (its trips' displays), or its name.
fn number_of(l: &LineInfo) -> String {
    l.tours.iter().flat_map(|t| t.trips.iter()).map(|t| t.line.trim()).find(|n| !n.is_empty()).unwrap_or(&l.name).to_string()
}

fn day_figures(l: &LineInfo) -> (u32, f64) {
    let runs: Vec<_> = l.tours.iter().filter(|t| t.runs).collect();
    (runs.len() as u32, runs.iter().flat_map(|t| t.trips.iter()).map(|t| t.km).sum())
}

/// The held concession of a line.
pub fn of_line<'a>(c: &'a Company, line: &str) -> Option<&'a Concession> {
    c.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(line))
}

/// The open tender of a line.
pub fn open_tender<'a>(c: &'a Company, line: &str) -> Option<&'a Tender> {
    c.concessions.tenders.iter().find(|t| t.open() && t.line.eq_ignore_ascii_case(line))
}

/// Lines run before there were concessions (or added directly) get one, and concessions of
/// lines no longer run are given up. Returns whether anything changed.
pub fn ensure(c: &mut Company) -> bool {
    let r = rules(c.difficulty);
    let mut changed = false;
    let lines = c.lines.clone();
    let n = c.concessions.held.len();
    c.concessions.held.retain(|h| lines.iter().any(|l| !l.own && l.name.eq_ignore_ascii_case(&h.line)));
    changed |= n != c.concessions.held.len();
    for l in lines.iter().filter(|l| !l.own) {
        if of_line(c, &l.name).is_none() {
            let until = dates::add(&c.date, r.term_weeks as i64 * 7 - 1);
            c.concessions.held.push(Concession { line: l.name.clone(), number: l.number.clone(), from: c.date.clone(), until, price: 1.0, direct: true });
            changed = true;
        }
    }
    changed
}

fn new_tender(c: &mut Company, l: &LineInfo, closes: String, renewal: bool) -> u32 {
    let r = rules(c.difficulty);
    c.concessions.counter += 1;
    let id = c.concessions.counter;
    let (day_tours, day_km) = day_figures(l);
    c.concessions.tenders.push(Tender {
        id,
        line: l.name.clone(),
        number: number_of(l),
        caption: l.termini.join(" – "),
        offered: c.date.clone(),
        closes,
        weeks: r.term_weeks,
        day_tours,
        day_km,
        week: None,
        renewal,
        applied: false,
        bid: None,
        fee_paid: false,
        outcome: None,
    });
    id
}

/// The market as the day `lines` were read for has it: a new round of tenders every four
/// weeks (map lines the company does not run, the player's own left out), renewals of the
/// concessions ending soon, old results dropped. Returns whether anything changed.
pub fn refresh(c: &mut Company, lines: &[LineInfo]) -> bool {
    let r = rules(c.difficulty);
    let mut changed = ensure(c);
    let today = c.date.clone();
    // renewals: on an easy economy they renew by themselves
    let ending: Vec<Concession> = c.concessions.held.iter().filter(|h| dates::between(&today, &h.until) <= RENEW_BEFORE).cloned().collect();
    for h in ending {
        if r.direct {
            if let Some(x) = c.concessions.held.iter_mut().find(|x| x.line == h.line) {
                x.until = dates::add(&x.until, r.term_weeks as i64 * 7);
                changed = true;
            }
            continue;
        }
        if open_tender(c, &h.line).is_some() {
            continue;
        }
        let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) else { continue };
        let closes = dates::add(&h.until, -14);
        let closes = if dates::between(&today, &closes) < 7 { dates::add(&today, 7) } else { closes };
        new_tender(c, l, closes, true);
        changed = true;
    }
    // a new round
    let round = dates::week_of(&today) / ROUND_WEEKS;
    if c.concessions.round != round {
        c.concessions.round = round;
        changed = true;
        c.concessions.tenders.retain(|t| t.open() || dates::between(&t.closes, &today) < 56);
        let mut free: Vec<&LineInfo> = lines
            .iter()
            .filter(|l| !crate::lines::is_own_file(&l.name))
            .filter(|l| !c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)))
            .filter(|l| open_tender(c, &l.name).is_none())
            // (a line the player may drive, with trips that carry passengers: not the map's
            // trains or its depot runs)
            .filter(|l| l.user_allowed && l.tours.iter().flat_map(|t| t.trips.iter()).any(|t| t.stops.len() >= 3))
            .collect();
        let mut rng = Rng::of(&[&c.id, "round"], round);
        let mut chosen = Vec::new();
        while chosen.len() < r.offers && !free.is_empty() {
            let k = rng.int(0, free.len() as i64 - 1) as usize;
            chosen.push(free.remove(k.min(free.len() - 1)).clone());
        }
        for l in chosen {
            new_tender(c, &l, dates::add(&today, OPEN_DAYS - 1), false);
        }
    }
    changed
}

/// Apply for a map line's concession (the lines page): its open tender, or a new one that
/// closes in a week. Returns the tender's id.
pub fn apply(c: &mut Company, l: &LineInfo) -> Result<u32, &'static str> {
    if c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)) {
        return Err("The company runs this line already.");
    }
    if let Some(t) = open_tender(c, &l.name) {
        let id = t.id;
        if let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id) {
            t.applied = true;
        }
        return Ok(id);
    }
    let closes = dates::add(&c.date, 6);
    let id = new_tender(c, l, closes, false);
    if let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id) {
        t.applied = true;
    }
    Ok(id)
}

/// Bid on a tender (again, while it is open): the fee is paid with the first bid.
pub fn bid(c: &mut Company, id: u32, price: f64) -> Result<(), &'static str> {
    if !(PRICE_MIN - 1e-9..=PRICE_MAX + 1e-9).contains(&price) {
        return Err("The authority does not take that price.");
    }
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err("There is no such tender.") };
    if !t.open() {
        return Err("This tender is closed.");
    }
    if !t.fee_paid {
        let f = fee(c, &t);
        if c.cash < f {
            return Err("Not enough cash.");
        }
        c.book(BookingKind::Concession, -f, format!("Tender line {}", t.number), false);
    }
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
        x.bid = Some((price * 1000.0).round() / 1000.0);
        x.fee_paid = true;
    }
    Ok(())
}

/// Take a bid back (the fee stays paid).
pub fn withdraw(c: &mut Company, id: u32) {
    if let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id && t.open()) {
        t.bid = None;
    }
}

/// What a closed tender, a concession that ended or the licence came to, for the report.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Won { number: String, until: String },
    Lost { number: String, winner: String },
    Ended { number: String },
}

/// The night after the day `date` was closed (the company's date is the next day already;
/// what is booked here is booked on `date`): tenders closing that day are decided, won lines
/// taken on, concessions that ended given up, the bid prices settled with the authority for
/// the day's kilometres (`km`: each company line's), and on a month's end the own lines'
/// licences. `lines`: the timetable's lines of `date` (a line won is taken on from them).
pub fn after_day(c: &mut Company, date: &str, km: &[(String, f64)], lines: &[LineInfo]) -> Vec<Event> {
    let mut events = Vec::new();
    let tomorrow = c.date.clone();
    let keep = std::mem::replace(&mut c.date, date.to_string());
    let r = rules(c.difficulty);

    // the bid prices: the close paid the reference for every kilometre
    let per_km = economy::compensation_per_km(&economy::rules(c.difficulty), c.reputation, c.contract_index);
    for (line, k) in km {
        let Some(h) = of_line(c, line).cloned() else { continue };
        let delta = (k * per_km * (h.price - 1.0)).round() as Cents;
        if delta != 0 {
            c.book(BookingKind::Compensation, delta, format!("Line {} (concession price)", h.number), false);
        }
    }

    // the tenders that closed today
    let closing: Vec<Tender> = c.concessions.tenders.iter().filter(|t| t.open() && dates::between(&t.closes, date) >= 0).cloned().collect();
    for t in closing {
        let outcome = decide(c, &t);
        match &outcome {
            Outcome::Won { .. } => {
                let price = t.bid.unwrap_or(1.0);
                if let Some(h) = c.concessions.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(&t.line)) {
                    h.until = dates::add(&h.until, t.weeks as i64 * 7);
                    h.price = price;
                    h.direct = false;
                    events.push(Event::Won { number: t.number.clone(), until: h.until.clone() });
                } else {
                    let until = dates::add(&tomorrow, t.weeks as i64 * 7 - 1);
                    match lines.iter().find(|l| l.name.eq_ignore_ascii_case(&t.line)) {
                        Some(l) => {
                            let _ = network::add_line(c, l, None);
                        }
                        None => c.lines.push(CompanyLine { name: t.line.clone(), number: t.number.clone(), numbers: vec![t.number.clone()], caption: t.caption.clone(), added: date.to_string(), tours: t.day_tours, km: t.day_km, ..Default::default() }),
                    }
                    if let Some(l) = c.lines.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&t.line)) {
                        l.added = tomorrow.clone();
                    }
                    c.concessions.held.push(Concession { line: t.line.clone(), number: t.number.clone(), from: tomorrow.clone(), until: until.clone(), price, direct: false });
                    events.push(Event::Won { number: t.number.clone(), until });
                }
            }
            Outcome::Lost { winner, .. } | Outcome::NoBid { winner } => {
                if t.renewal || t.bid.is_some() {
                    events.push(Event::Lost { number: t.number.clone(), winner: winner.clone() });
                }
            }
        }
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == t.id) {
            x.outcome = Some(outcome);
        }
    }

    // concessions that ended today
    let ended: Vec<Concession> = c.concessions.held.iter().filter(|h| dates::between(&h.until, date) >= 0).cloned().collect();
    for h in ended {
        if r.direct {
            if let Some(x) = c.concessions.held.iter_mut().find(|x| x.line == h.line) {
                x.until = dates::add(&x.until, r.term_weeks as i64 * 7);
            }
            continue;
        }
        c.concessions.held.retain(|x| x.line != h.line);
        network::remove_line(c, &h.line);
        events.push(Event::Ended { number: h.number.clone() });
    }

    // the licences of the own lines
    if dates::last_of_month(date) {
        let own: Vec<String> = c.lines.iter().filter(|l| l.own).map(|l| l.number.clone()).collect();
        for n in own {
            let fee = (LICENCE as f64 * c.price_index).round() as Cents;
            c.book(BookingKind::Concession, -fee, format!("Line licence {n}"), false);
        }
    }

    c.date = keep;
    events
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::{StopInfo, TourInfo, TripInfo};

    fn line(name: &str, number: &str, tours: usize, runs: bool) -> LineInfo {
        let trip = |k: usize| TripInfo {
            name: format!("{name} {k}"),
            index: k + 1,
            line: number.into(),
            from: "A".into(),
            terminus: "B".into(),
            departure: 21_600.0 + k as f64 * 3600.0,
            arrival: 23_400.0 + k as f64 * 3600.0,
            stops: (0..5).map(|s| StopInfo { name: format!("S{s}"), id: s, arr: 0.0, dep: 0.0 }).collect(),
            km: 10.0,
        };
        LineInfo {
            name: name.into(),
            user_allowed: true,
            termini: vec!["A".into(), "B".into()],
            tours: (0..tours).map(|t| TourInfo { number: (t + 1).to_string(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs, next_run: None, trips: (0..4).map(trip).collect() }).collect(),
        }
    }

    fn company(d: Difficulty) -> Company {
        found(&Founding { name: "Tender".into(), difficulty: d, date: "2024-03-04".into(), ..Default::default() }, "Luc")
    }

    /// The nights from today until `until` (inclusive), as the day's close runs them.
    fn nights_until(c: &mut Company, until: &str, lines: &[LineInfo]) -> Vec<Event> {
        let mut out = Vec::new();
        while dates::between(&c.date, until) >= 0 {
            let date = c.date.clone();
            c.date = dates::add(&date, 1);
            out.extend(after_day(c, &date, &[], lines));
        }
        out
    }

    #[test]
    fn a_round_offers_lines_the_company_does_not_run_and_their_week_is_counted() {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true), line("oo_12", "12", 1, true), line("Linie9", "9", 4, true)];
        let mut c = company(Difficulty::Realistic);
        network::add_line(&mut c, &lines[0], None).unwrap();
        assert!(refresh(&mut c, &lines));
        // (the line run already is grandfathered, the player's own one is never offered)
        assert!(of_line(&c, "Linie5").is_some_and(|h| h.direct && h.price == 1.0));
        let offered: Vec<&str> = c.concessions.tenders.iter().map(|t| t.line.as_str()).collect();
        assert_eq!(offered.len(), 2);
        assert!(!offered.contains(&"Linie5") && !offered.contains(&"oo_12"));
        assert!(!refresh(&mut c, &lines), "the same round again changes nothing");
        // the week: five weekdays of 3 tours, a weekend without
        let mut days: Vec<Vec<LineInfo>> = (0..5).map(|_| vec![line("Linie5", "5", 3, true)]).collect();
        days.push(vec![line("Linie5", "5", 3, false)]);
        let w = week_of(&days, "Linie5");
        assert_eq!((w.tours, w.trips, w.peak), (15, 60, 3));
        assert!((w.km - 600.0).abs() < 1e-9);
        assert!(week_revenue(&c, &w, 0.9) < week_revenue(&c, &w, 1.0));
    }

    #[test]
    fn a_low_price_and_a_good_name_win_and_the_line_is_run_for_the_term() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(Difficulty::Realistic);
        c.cash = 1_000_000_00;
        let id = apply(&mut c, &lines[0]).unwrap();
        assert_eq!(apply(&mut c, &lines[0]), Ok(id), "one tender a line");
        assert_eq!(bid(&mut c, id, 0.5), Err("The authority does not take that price."));
        let cash = c.cash;
        bid(&mut c, id, PRICE_MIN).unwrap();
        assert_eq!(cash - c.cash, 1_500_00 + 2 * 150_00);
        bid(&mut c, id, 0.81).unwrap();
        assert_eq!(cash - c.cash, 1_800_00, "the fee once");
        c.reputation = 90.0;
        c.punctuality = 95.0;
        assert!(matches!(decide(&c, &c.concessions.tenders[0]), Outcome::Won { .. }));
        let closes = c.concessions.tenders[0].closes.clone();
        let ev = nights_until(&mut c, &closes, &lines);
        assert!(matches!(&ev[..], [Event::Won { number, .. }] if number == "7"));
        assert!(c.lines.iter().any(|l| l.name == "Linie7"));
        let h = of_line(&c, "Linie7").unwrap().clone();
        assert_eq!((h.from.as_str(), h.price), (c.date.as_str(), 0.81));
        assert_eq!(dates::between(&h.from, &h.until), 52 * 7 - 1);
        // its price is settled for the kilometres run
        let before = c.cash;
        let date = c.date.clone();
        c.date = dates::add(&date, 1);
        after_day(&mut c, &date, &[("Linie7".into(), 100.0)], &lines);
        let per_km = economy::compensation_per_km(&economy::rules(c.difficulty), c.reputation, c.contract_index);
        assert_eq!(c.cash - before, (100.0 * per_km * (0.81 - 1.0)).round() as Cents);
        // the renewal comes up; without a bid the line ends with the term
        refresh(&mut c, &lines);
        assert!(c.concessions.tenders.iter().all(|t| !t.open()));
        c.date = dates::add(&h.until, -RENEW_BEFORE);
        refresh(&mut c, &lines);
        let renewal = open_tender(&c, "Linie7").unwrap().clone();
        assert!(renewal.renewal);
        let ev = nights_until(&mut c, &h.until.clone(), &lines);
        assert!(ev.iter().any(|e| matches!(e, Event::Lost { .. })));
        assert!(ev.iter().any(|e| matches!(e, Event::Ended { number } if number == "7")));
        assert!(!c.lines.iter().any(|l| l.name == "Linie7"));
    }

    #[test]
    fn rivals_are_fixed_per_tender_and_harder_on_a_hard_economy() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut wins = [0usize; 3];
        for (k, d) in Difficulty::ALL.into_iter().enumerate() {
            for n in 0..60 {
                let mut c = company(d);
                c.id = format!("c{n}");
                c.cash = 1_000_000_00;
                let id = apply(&mut c, &lines[0]).unwrap();
                assert_eq!(rivals(&c, &c.concessions.tenders[0]), rivals(&c, &c.concessions.tenders[0]));
                bid(&mut c, id, 0.97).unwrap();
                if matches!(decide(&c, &c.concessions.tenders[0]), Outcome::Won { .. }) {
                    wins[k] += 1;
                }
            }
        }
        assert!(wins[0] > wins[1] && wins[1] > wins[2], "{wins:?}");
        // a cheaper bid scores better, a better name too
        assert!(score(0.9, 60.0) > score(1.0, 60.0) && score(1.0, 70.0) > score(1.0, 60.0));
    }

    #[test]
    fn an_easy_economy_takes_lines_directly_and_own_lines_pay_a_licence() {
        let lines = vec![line("Linie5", "5", 3, true)];
        let mut c = company(Difficulty::Easy);
        assert!(may_add_directly(&c));
        network::add_line(&mut c, &lines[0], None).unwrap();
        c.lines.push(CompanyLine { name: "oo_1".into(), number: "1".into(), own: true, ..Default::default() });
        ensure(&mut c);
        assert_eq!(c.concessions.held.len(), 1, "the own line has none");
        let until = of_line(&c, "Linie5").unwrap().until.clone();
        nights_until(&mut c, &until, &lines);
        assert!(c.lines.iter().any(|l| l.name == "Linie5"), "renewed by itself");
        assert!(dates::between(&c.date, &of_line(&c, "Linie5").unwrap().until) > 300);
        // the month's licence
        let mut d = company(Difficulty::Realistic);
        d.lines.push(CompanyLine { name: "oo_1".into(), number: "1".into(), own: true, ..Default::default() });
        d.date = "2024-03-31".into();
        let cash = d.cash;
        let date = d.date.clone();
        d.date = dates::add(&date, 1);
        after_day(&mut d, &date, &[], &[]);
        assert_eq!(cash - d.cash, LICENCE);
        assert!(!may_add_directly(&d));
    }
}

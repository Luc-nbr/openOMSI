//! Concessions: the right to run a line of the map's timetable for a term, won in a tender
//! against other operators (the Busbetrieb-Simulator's map concessions; Omsi-Hub's
//! `Concessie` and `schrijfIn`), with the bus contract that pays for it (`bus_contracts`).
//!
//! Every four weeks the authority puts some of the map's lines out to tender, one after the
//! other on the days of the round's first week; a line of the map can also be applied for on
//! the Lines page (its tender opens at once). A tender lets the line as a gross-cost or a
//! net-subsidy contract (`bus_contracts::Kind`, drawn per tender) for a term of years. It is an
//! auction of a few hours of the company's time (`auction`): the map's other operators - made
//! up, each with a character, the same ones every time (`Rival`) - and the player ask a price a
//! year for running the line, at most the authority's published cap (`Tender::reference`: a bid
//! above it does not count); the authority scores it 70 % on price (the lowest ask the full
//! points) and 30 % on quality - the bidder's track record, the average net default points of its
//! contracts over the last twelve months (`bus_contracts::quality`) - and the best total when it
//! closes wins (equal totals: the better price points, then quality). The rivals bid around the
//! work's cost price on the UK operator's formula (`bus_contracts::price_of`), within the game
//! mode's band by their character (`auction::Character::band`); the player sees the cap and an
//! advice price on the same formula at his own costs (`advice`). The player may also buy the
//! line at once for the buy-out price (its contract at the cap). A won line is run from the next
//! day; a green contract earns its extension years, and towards the end of its term the line is
//! put out again (the renewal); a concession not won again ends: the line goes to the winner.
//! On an easy economy map lines are taken on directly and renewed by themselves; on Realistic
//! and Normal adding a map line is applying for its concession.
//!
//! A map line taken on without a tender (an easy economy, or run before there were
//! concessions) is commercial: no contract, its fares its own. The player's own lines from the
//! line editor need no concession: the company runs them on its own account and pays the
//! authority's licence for each month instead (`LICENCE`).
//!
//! The company's clock (`clock`) opens and closes the tenders at their minutes and tells the
//! rivals' bids as they come; `after_day` closes what the night finds still open.

use super::auction::{self, Bidder, Character, Lot, Placed, Seen, Who};
use super::bus_contracts::{self as bc, Kind, Period, Work};
use super::clock;
use super::dates;
use super::econ_uk;
use super::levels;
use super::model::{BookingKind, Cents, Company, GameMode};
use super::network;
use super::rng::Rng;
use super::specials;
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// The authority's licence for one of the player's own lines, a month.
pub const LICENCE: Cents = 350_00;
/// A tender round lasts this many weeks.
pub const ROUND_WEEKS: i64 = 4;
/// An auction runs this many hours (from, to).
pub const HOURS: (i64, i64) = (2, 6);
/// A concession is put out again this many days before it ends.
pub const RENEW_BEFORE: i64 = 42;
/// The share of the term's payments at the cap a line is worth to the auction (its `value`).
pub const VALUE_SHARE: f64 = 0.05;
/// The buy-out price, as a share of the value.
pub const BUY_OUT: f64 = 1.4;
/// The step a rival may go lower by, as a share of the cap.
pub const ASK_STEP: f64 = 0.01;
/// The share of the reference the recommended ask wins with, at least (Normal).
pub const RECOMMEND_CHANCE: f64 = 0.6;
/// The recommended ask goes no lower than this share of the cap.
pub const RECOMMEND_FLOOR: f64 = 0.8;

/// A concession the company holds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Concession {
    /// The timetable's line (its `.ttl` name) and the number shown.
    pub line: String,
    pub number: String,
    pub from: String,
    /// The last day it runs.
    pub until: String,
    /// The winning ask's share of the cap (1: at the cap, or taken on directly; older files bid
    /// a compensation per km by it). What the line is paid is its contract's
    /// (`bus_contracts`).
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
    /// The days it runs, and its timetabled bus-hours.
    #[serde(default)]
    pub days: u32,
    #[serde(default)]
    pub hours: f64,
}

/// A bidder's last bid when a tender closed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RivalBid {
    pub name: String,
    #[serde(default)]
    pub price: f64,
    pub quality: f64,
    pub score: f64,
    #[serde(default)]
    pub amount: Cents,
}

/// A bidder's points when a tender closed (the 70/30 score): its ask, price points, quality
/// points and total. `name` empty: the company.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Standing {
    pub name: String,
    pub amount: Cents,
    pub price: f64,
    pub quality: f64,
    pub total: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Won {
        score: f64,
        rivals: Vec<RivalBid>,
        /// What the concession cost.
        #[serde(default)]
        amount: Cents,
        /// Bought at the buy-out price.
        #[serde(default)]
        bought: bool,
    },
    Lost {
        score: f64,
        winner: String,
        rivals: Vec<RivalBid>,
        #[serde(default)]
        amount: Cents,
    },
    /// No bid of the player's (`winner` empty: nobody bid).
    NoBid {
        winner: String,
        #[serde(default)]
        amount: Cents,
    },
}

/// A line put out to tender.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tender {
    pub id: u32,
    pub line: String,
    pub number: String,
    pub caption: String,
    /// The days it opens and closes on (see `opens_at`, `closes_at`).
    pub offered: String,
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
    #[serde(default)]
    pub fee_paid: bool,
    #[serde(default)]
    pub outcome: Option<Outcome>,
    /// When the auction runs, on the company's clock (`clock::now`).
    #[serde(default)]
    pub opens_at: i64,
    #[serde(default)]
    pub closes_at: i64,
    /// What the line is worth to the auction (see `VALUE_SHARE`).
    #[serde(default)]
    pub value: Cents,
    /// The rivals taking part (`Rival::id`).
    #[serde(default)]
    pub bidders: Vec<u32>,
    /// The player's bids: when, how much.
    #[serde(default)]
    pub offers: Vec<(i64, Cents)>,
    /// The clock told its opening, and this many of its bids.
    #[serde(default)]
    pub opened: bool,
    #[serde(default)]
    pub told: usize,
    /// The authority's published cap: the most it pays a year, the ceiling of the asks (0: a
    /// tender of an older file, priced again by `ensure`).
    #[serde(default)]
    pub reference: Cents,
    /// Every bidder's points at the close, best first.
    #[serde(default)]
    pub standings: Vec<Standing>,
    /// The contract it lets: its kind, how it is paid, its core years, the work (a day's hours,
    /// kilometres, journeys, buses, the days a year) and the authority's reckoning of its cost
    /// price a year (the rivals bid around it; cents).
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub period: Period,
    #[serde(default)]
    pub years: u32,
    #[serde(default)]
    pub work: Work,
    #[serde(default)]
    pub cost: Cents,
}

impl Tender {
    /// Not decided yet (announced or running).
    pub fn open(&self) -> bool {
        self.outcome.is_none()
    }

    /// Taking bids at `now`.
    pub fn running(&self, now: i64) -> bool {
        self.open() && self.opens_at <= now && now < self.closes_at
    }

    pub fn lot(&self) -> Lot {
        Lot { opens: self.opens_at, closes: self.closes_at, ceiling: self.reference, step: round(self.reference as f64 * ASK_STEP) }
    }

    /// Asked as the 70/30 score has it (not a tender of an older file).
    pub fn scored(&self) -> bool {
        self.reference > 0
    }

    /// A bid of `amount` counts: at most the cap.
    pub fn counts(&self, amount: Cents) -> bool {
        amount <= self.reference
    }

    /// The core years of its contract (an older tender's: by its weeks).
    pub fn term_years(&self) -> u32 {
        if self.years > 0 {
            self.years
        } else {
            ((self.weeks as f64 / 52.0).round() as u32).max(1)
        }
    }

    /// The share of the cap an ask is.
    pub fn share(&self, amount: Cents) -> f64 {
        if self.reference > 0 {
            amount as f64 / self.reference as f64
        } else {
            1.0
        }
    }

    pub fn buy_out(&self) -> Cents {
        round(self.value as f64 * BUY_OUT)
    }
}

/// One of the map's other operators.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rival {
    pub id: u32,
    pub name: String,
    pub character: Character,
    pub quality: f64,
}

/// The company's concessions and the market of tenders.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Concessions {
    pub held: Vec<Concession>,
    pub tenders: Vec<Tender>,
    /// The round the market was last offered in (`week / ROUND_WEEKS`).
    pub round: i64,
    pub counter: u32,
    /// The map's other operators (made once, kept).
    #[serde(default)]
    pub rivals: Vec<Rival>,
    /// The company's bus contracts, and those ended in the last twelve months
    /// (`bus_contracts`).
    #[serde(default)]
    pub contracts: Vec<super::bus_contracts::Contract>,
    #[serde(default)]
    pub contract_counter: u32,
}

/// How a game mode runs its tenders (how the rivals bid: `econ_uk::ModeFactors::rival_bids`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// How many rivals bid (from, to).
    pub rivals: (i64, i64),
    /// The fee for taking part: a fixed part and one per tour of the line's day.
    pub fee_base: Cents,
    pub fee_per_tour: Cents,
    pub term_weeks: u32,
    /// Lines offered each round.
    pub offers: usize,
    /// Map lines may be taken on without a tender, and concessions renew by themselves.
    pub direct: bool,
}

/// A concession's term in weeks: a local bus contract's core years.
pub const TERM_WEEKS: u32 = econ_uk::TERM_LOCAL.0 * 52;

pub fn rules(d: GameMode) -> Rules {
    match d {
        // (the term: a local bus contract's core years, `econ_uk::TERM_LOCAL`)
        GameMode::Casual => Rules { rivals: (1, 2), fee_base: 0, fee_per_tour: 0, term_weeks: TERM_WEEKS, offers: 4, direct: true },
        GameMode::Normal => Rules { rivals: (2, 3), fee_base: 1_500_00, fee_per_tour: 150_00, term_weeks: TERM_WEEKS, offers: 3, direct: false },
        GameMode::Realistic => Rules { rivals: (3, 4), fee_base: 3_000_00, fee_per_tour: 250_00, term_weeks: TERM_WEEKS, offers: 3, direct: false },
    }
}

/// The rules the company's tenders run by: its game mode's, and a sandbox with everything
/// unlocked takes the map's lines on directly, its concessions renewing by themselves
/// (`sandbox::unlocked`).
pub fn rules_of(c: &Company) -> Rules {
    let mut r = rules(c.game_mode);
    if super::sandbox::unlocked(c) {
        r.direct = true;
    }
    r
}

/// Map lines may be added without a concession's tender (an easy economy).
pub fn may_add_directly(c: &Company) -> bool {
    rules_of(c).direct
}

/// The operators a map draws its rivals from (made up).
const NAMES: [&str; 16] = [
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
    "Kraftverkehr Lindner",
    "Omnibus Petersen",
    "Verkehrsbetrieb Auental",
    "Schulz & Söhne Reisen",
    "Kreisbus Nord",
    "Vogt Linienbus",
];

/// The map's rivals, made once per company from its map: six operators, an aggressive and a
/// cautious one, a big and a small one among them. Returns whether they were made now.
pub fn ensure_rivals(c: &mut Company) -> bool {
    if !c.concessions.rivals.is_empty() {
        return false;
    }
    let mut rng = Rng::of(&[&c.id, &c.map, "rivals"], 0);
    let mut names = NAMES.to_vec();
    let mut chars = Character::ALL.to_vec();
    chars.push(Character::ALL[rng.int(0, 3) as usize]);
    chars.push(Character::ALL[rng.int(0, 3) as usize]);
    for (k, ch) in chars.into_iter().enumerate() {
        let name = names.remove(rng.int(0, names.len() as i64 - 1) as usize);
        let (lo, hi) = ch.quality();
        c.concessions.rivals.push(Rival { id: k as u32 + 1, name: name.to_string(), character: ch, quality: rng.range(lo, hi).round() });
    }
    true
}

pub fn rival(c: &Company, id: u32) -> Option<&Rival> {
    c.concessions.rivals.iter().find(|r| r.id == id)
}

/// How the authority sees the company's quality for a tender (0-100): its track record - 100 %
/// less 2 % for each average net default point of its contracts over the last twelve months; a
/// newcomer a fair record (`bus_contracts::quality`).
pub fn quality(c: &Company) -> f64 {
    bc::quality(c)
}

/// The company's quality in a tender.
pub fn quality_in(c: &Company, _t: &Tender) -> f64 {
    quality(c)
}

/// The fee for taking part in a tender (a tenth of the line's worth at most).
pub fn fee(c: &Company, t: &Tender) -> Cents {
    fee_of(c, t.day_tours, t.value)
}

/// The fee of a tender of a line with `day_tours` tours a day worth `value`.
fn fee_of(c: &Company, day_tours: u32, value: Cents) -> Cents {
    let r = rules_of(c);
    let f = ((r.fee_base + r.fee_per_tour * day_tours as Cents) as f64 * c.price_index).round() as Cents;
    if value > 0 && f > 0 {
        f.min(round(value as f64 * 0.1))
    } else {
        f
    }
}

fn round(x: f64) -> Cents {
    ((x / auction::ROUND as f64).round() as Cents * auction::ROUND).max(auction::ROUND)
}

/// What a line is worth to the auction: its share of the term's payments at the cap, at least
/// 2 000 €.
fn value_at(cap: Cents, years: u32) -> Cents {
    round(cap as f64 * years.max(1) as f64 * VALUE_SHARE).max(2_000_00)
}

/// A tender's contract priced: its work's cost price a year as the authority reckons it (a UK
/// operator's formula in the company's world, `bus_contracts::price_of`), its cap
/// (`econ_uk::TENDER_CAP` over that) and its worth.
fn price(c: &Company, t: &mut Tender) {
    let years = t.term_years();
    let cost = bc::price_of(c, &t.work, t.kind, years as f64);
    t.cost = round(cost);
    t.reference = round(cost * econ_uk::TENDER_CAP);
    t.value = value_at(t.reference, years);
}

/// The advice for the company's ask on a tender (a year): the work's cost price on the same
/// formula at the company's own costs, with the operator's 5 % margin (`bus_contracts::advice`),
/// at most the cap.
pub fn advice(c: &Company, t: &Tender) -> Cents {
    round(bc::advice(c, &t.work, t.kind, t.term_years() as f64)).min(t.reference)
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
        if !runs.is_empty() {
            w.days += 1;
        }
        for t in &runs {
            let dep = t.trips.iter().map(|x| x.departure).fold(f64::MAX, f64::min);
            let arr = t.trips.iter().map(|x| x.arrival).fold(f64::MIN, f64::max);
            if arr > dep {
                w.hours += (arr - dep) / 3600.0;
            }
        }
    }
    w
}

/// A timetable of the map is a line of the company's to run (not depot runs, empty runs,
/// specials, nor other operators' traffic - a timetable the player may not drive: `specials`).
/// The Lines page offers only these (it offered the others too, and Apply refused them, Luc).
pub fn is_line(c: &Company, l: &LineInfo) -> bool {
    specials::line_kind(l, &[&c.depot]).line()
}

/// The held concession of a line.
pub fn of_line<'a>(c: &'a Company, line: &str) -> Option<&'a Concession> {
    c.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(line))
}

/// The open tender of a line (announced or running).
pub fn open_tender<'a>(c: &'a Company, line: &str) -> Option<&'a Tender> {
    c.concessions.tenders.iter().find(|t| t.open() && t.line.eq_ignore_ascii_case(line))
}

/// Lines run before there were concessions (or added directly) get one, and concessions of
/// lines no longer run are given up (not those won for tomorrow); the map's rivals are made,
/// and tenders of older files (bid by the day) become auctions of the next hours. Returns
/// whether anything changed.
pub fn ensure(c: &mut Company) -> bool {
    let r = rules_of(c);
    let mut changed = ensure_rivals(c);
    let lines = c.lines.clone();
    let today = c.date.clone();
    let n = c.concessions.held.len();
    // (a line given up with a contract running hands the contract back: a month's payment)
    let gone: Vec<String> = c.concessions.held.iter().filter(|h| !lines.iter().any(|l| !l.own && l.name.eq_ignore_ascii_case(&h.line)) && dates::between(&today, &h.from) <= 0).map(|h| h.line.clone()).collect();
    for line in gone {
        bc::hand_back(c, &line);
    }
    c.concessions.held.retain(|h| lines.iter().any(|l| !l.own && l.name.eq_ignore_ascii_case(&h.line)) || dates::between(&today, &h.from) > 0);
    changed |= n != c.concessions.held.len();
    changed |= bc::ensure_hires(c);
    for l in lines.iter().filter(|l| !l.own) {
        if of_line(c, &l.name).is_none() {
            let until = dates::add(&c.date, r.term_weeks as i64 * 7 - 1);
            c.concessions.held.push(Concession { line: l.name.clone(), number: l.number.clone(), from: c.date.clone(), until, price: 1.0, direct: true });
            changed = true;
        }
    }
    let now = clock::now(c);
    let pool: Vec<u32> = c.concessions.rivals.iter().map(|r| r.id).collect();
    // (an open tender of an older file: its contract priced on its day's figures, its old bids
    // dropped - the fee stays paid)
    let mut asked_again = Vec::new();
    let old: Vec<u32> = c.concessions.tenders.iter().filter(|t| t.open() && t.reference == 0).map(|t| t.id).collect();
    for id in old {
        let Some(mut t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { continue };
        if t.work == Work::default() {
            t.work = bc::rough_work(t.day_tours, t.day_km);
        }
        price(c, &mut t);
        t.offers.clear();
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
            *x = t;
        }
        asked_again.push(id);
        changed = true;
    }
    for t in c.concessions.tenders.iter_mut().filter(|t| t.open() && t.closes_at == 0) {
        t.opens_at = now;
        t.closes_at = now + 4 * 60;
        t.closes = clock::date_of(t.closes_at);
        t.bidders = pool.iter().copied().take(r.rivals.1 as usize).collect();
        changed = true;
    }
    // (their bids so far told already: not again)
    for id in asked_again {
        let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { continue };
        let n = bids(c, &t, now).len();
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
            x.told = n;
        }
    }
    changed
}

/// A new tender of a line, its auction opening at `opens_at` (minutes) for a few hours.
fn new_tender(c: &mut Company, l: &LineInfo, opens_at: i64, renewal: bool) -> u32 {
    ensure_rivals(c);
    let r = rules_of(c);
    c.concessions.counter += 1;
    let id = c.concessions.counter;
    let mut rng = Rng::of(&[&c.id, "tender", &l.name], id as i64);
    let closes_at = opens_at + rng.int(HOURS.0 * 4, HOURS.1 * 4) * 15;
    let work = bc::work_of(l);
    let (day_tours, day_km) = (work.buses, work.km);
    let mut pool: Vec<u32> = c.concessions.rivals.iter().map(|r| r.id).collect();
    let n = (rng.int(r.rivals.0, r.rivals.1).max(1) as usize).min(pool.len());
    let mut bidders = Vec::new();
    for _ in 0..n {
        bidders.push(pool.remove(rng.int(0, pool.len() as i64 - 1) as usize));
    }
    // (gross cost or net subsidy, by the day or the week: drawn per tender)
    let kind = if rng.chance(econ_uk::GROSS_SHARE) { Kind::Gross } else { Kind::Net };
    let period = match kind {
        Kind::Net => Period::Month,
        _ if rng.chance(econ_uk::WEEKLY_SHARE) => Period::Week,
        _ => Period::Day,
    };
    let years = bc::term(kind).0;
    let mut t = Tender {
        id,
        line: l.name.clone(),
        number: specials::number_of(l, &[&c.depot]),
        caption: specials::caption_of(l, &[&c.depot]),
        offered: clock::date_of(opens_at),
        closes: clock::date_of(closes_at),
        weeks: years * 52,
        day_tours,
        day_km,
        week: None,
        renewal,
        applied: false,
        fee_paid: false,
        outcome: None,
        opens_at,
        closes_at,
        value: 0,
        bidders,
        offers: Vec::new(),
        opened: false,
        told: 0,
        reference: 0,
        standings: Vec::new(),
        kind,
        period,
        years,
        work,
        cost: 0,
    };
    price(c, &mut t);
    c.concessions.tenders.push(t);
    id
}

/// The market as the day `lines` were read for has it: a new round of tenders every four
/// weeks (map lines the company does not run, the player's own left out), opening one after
/// the other on the next days; renewals of the concessions ending soon; old results dropped;
/// the lines won for a day to come taken on (`take_on`). Returns whether anything changed.
pub fn refresh(c: &mut Company, lines: &[LineInfo]) -> bool {
    let r = rules_of(c);
    let mut changed = ensure(c);
    changed |= take_on(c, lines);
    changed |= mend_tenders(c, lines);
    // (a green contract's extension year first: it is not put out again)
    changed |= !bc::extend_due(c).is_empty();
    let today = c.date.clone();
    let now = clock::now(c);
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
        // (its renewal decided already in this term - lost, or nobody bid -: the concession
        // ends with its term, it is not put out again)
        if c.concessions.tenders.iter().any(|t| t.renewal && !t.open() && t.line.eq_ignore_ascii_case(&h.line) && dates::between(&t.offered, &h.until) <= RENEW_BEFORE) {
            continue;
        }
        let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) else { continue };
        // (a fortnight before it ends, a week from now at the soonest)
        let day = dates::add(&h.until, -14);
        let day = if dates::between(&today, &day) < 7 { dates::add(&today, 7) } else { day };
        let mut rng = Rng::of(&[&c.id, "renewal", &h.line], dates::parse(&day).unwrap_or(0));
        new_tender(c, l, clock::moment(&day, 9 * 60 + rng.int(0, 24) * 15), true);
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
            // (a line of its own: not the map's depot runs, empty runs, specials or other
            // traffic - see `specials`)
            .filter(|l| is_line(c, l))
            .collect();
        let mut rng = Rng::of(&[&c.id, "round"], round);
        let mut chosen = Vec::new();
        while chosen.len() < r.offers && !free.is_empty() {
            let k = rng.int(0, free.len() as i64 - 1) as usize;
            chosen.push(free.remove(k.min(free.len() - 1)).clone());
        }
        for (k, l) in chosen.into_iter().enumerate() {
            // one a day, in the office hours, half an hour from now at the soonest
            let at = clock::moment(&dates::add(&today, k as i64), 9 * 60 + rng.int(0, 24) * 15).max(now + 30);
            new_tender(c, &l, at, false);
        }
    }
    changed
}

/// The lines of the concessions won for a day to come taken on at once (Trello #95: a line won
/// in a tender was taken on only in the night before its first day, so RosterFlow and the
/// planning could not show it to be planned ahead). It is the company's line from now, not in
/// service (its tours are shown, neither run nor charged), and goes into service from its
/// concession's first day at the soonest (`network::start_service`); its days of grace count
/// from then (`network::late_start`). Its figures from the map's timetable (`lines`), else
/// the tender's. Returns whether any was taken on.
pub fn take_on(c: &mut Company, lines: &[LineInfo]) -> bool {
    let today = c.date.clone();
    let new: Vec<Concession> = c.concessions.held.iter().filter(|h| dates::between(&today, &h.from) > 0 && !c.lines.iter().any(|l| l.name.eq_ignore_ascii_case(&h.line))).cloned().collect();
    for h in &new {
        match lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) {
            Some(l) => {
                let _ = network::add_line(c, l, None);
            }
            None => {
                let t = c.concessions.tenders.iter().rev().find(|t| t.line.eq_ignore_ascii_case(&h.line));
                let (caption, tours, day_km) = t.map(|t| (t.caption.clone(), t.day_tours, t.day_km)).unwrap_or_default();
                c.lines.push(super::model::CompanyLine { name: h.line.clone(), number: h.number.clone(), numbers: vec![h.number.clone()], caption, tours, km: day_km, ..Default::default() });
            }
        }
        if let Some(l) = c.lines.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&h.line)) {
            l.added = h.from.clone();
        }
    }
    !new.is_empty()
}

/// When the line's concession begins (the clock's minutes), if it begins after `now`: its
/// service starts then at the soonest.
pub fn starts_after(c: &Company, line: &str, now: i64) -> Option<i64> {
    of_line(c, line).map(|h| clock::moment(&h.from, 0)).filter(|&at| at > now)
}

/// Open tenders as `specials` reads their timetables now. Older versions offered timetables
/// that are no line of their own and named a whole-day timetable after its first trip - in Bad
/// Hügelsdorf the depot run "X" that opens "Montag - Freitag" - and the file kept both: such
/// tenders are withdrawn, the others get the number and caption of their passenger trips.
/// Returns whether anything changed.
fn mend_tenders(c: &mut Company, lines: &[LineInfo]) -> bool {
    let depot = c.depot.clone();
    let of = |name: &str| lines.iter().find(|l| l.name.eq_ignore_ascii_case(name));
    let before = c.concessions.tenders.len();
    c.concessions.tenders.retain(|t| !t.open() || of(&t.line).is_none_or(|l| specials::line_kind(l, &[&depot]).line()));
    let mut changed = before != c.concessions.tenders.len();
    for t in c.concessions.tenders.iter_mut().filter(|t| t.open()) {
        let Some(l) = of(&t.line) else { continue };
        let (number, caption) = (specials::number_of(l, &[&depot]), specials::caption_of(l, &[&depot]));
        if t.number != number || t.caption != caption {
            (t.number, t.caption) = (number, caption);
            changed = true;
        }
    }
    changed
}

/// The refusal when the company's level allows no more concessions.
pub const LEVEL_LIMIT: &str = "The company holds as many concessions as its level allows.";

/// Whether the company's level lets it take on another concession (`levels::max_concessions`)
/// in tender `id` (0: a new one): the concessions it won count, and the other tenders it bids
/// in; a renewal of one it holds is always allowed, and the lines taken on directly (an easy
/// economy, or run before there were concessions) are not counted.
fn level_allows(c: &Company, id: u32) -> Result<(), &'static str> {
    if c.concessions.tenders.iter().any(|t| t.id == id && t.renewal) {
        return Ok(());
    }
    let held = c.concessions.held.iter().filter(|h| !h.direct).count();
    let bidding = c.concessions.tenders.iter().filter(|t| t.id != id && t.open() && !t.renewal && !t.offers.is_empty()).count();
    if held + bidding >= levels::max_concessions(c) {
        return Err(LEVEL_LIMIT);
    }
    Ok(())
}

/// Apply for a map line's concession (the lines page): its open tender, or a new one whose
/// auction opens now. Returns the tender's id.
pub fn apply(c: &mut Company, l: &LineInfo) -> Result<u32, &'static str> {
    if c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)) {
        return Err("The company runs this line already.");
    }
    if !is_line(c, l) {
        return Err("This timetable carries no passengers of its own: depot runs, empty runs or other traffic.");
    }
    level_allows(c, open_tender(c, &l.name).map_or(0, |t| t.id))?;
    if let Some(t) = open_tender(c, &l.name) {
        let id = t.id;
        if let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id) {
            t.applied = true;
        }
        return Ok(id);
    }
    let now = clock::now(c);
    let id = new_tender(c, l, now, false);
    if let Some(t) = c.concessions.tenders.iter_mut().find(|t| t.id == id) {
        t.applied = true;
    }
    Ok(id)
}

/// Where a rival's floor may lie on a tender (cents, from, to): the work's cost price as the
/// authority reckons it, within the game mode's band around it (`econ_uk::ModeFactors::
/// rival_bids`) where its character bids (`Character::band`).
pub fn floor_range(c: &Company, t: &Tender, ch: Character) -> (f64, f64) {
    let (lo, hi) = econ_uk::mode(c.game_mode).rival_bids;
    let (a, b) = ch.band();
    // (a tender of an older file, closed before there was a cost price: the cap's)
    let cost = if t.cost > 0 { t.cost as f64 } else { t.reference.max(1) as f64 / econ_uk::TENDER_CAP };
    (cost * (1.0 + lo + (hi - lo) * a), cost * (1.0 + lo + (hi - lo) * b))
}

/// The rivals of a tender as its auction knows them: their floors drawn per tender (secret;
/// a floor above the cap never bids), when they open, who keeps quiet until the end.
pub fn bidders(c: &Company, t: &Tender) -> Vec<Bidder> {
    let mut nobody_opened = true;
    let mut out = Vec::new();
    for (k, id) in t.bidders.iter().enumerate() {
        let Some(rv) = rival(c, *id) else { continue };
        let mut rng = Rng::of(&[&c.id, "bidder", &rv.name], t.id as i64);
        let (lo, hi) = floor_range(c, t, rv.character);
        // (in whole hundreds, as the bids go)
        let floor = (rng.range(lo, hi) / auction::ROUND as f64).ceil() as Cents * auction::ROUND;
        let quiet = rng.chance(rv.character.quiet());
        let len = (t.closes_at - t.opens_at).max(30);
        let quiet_until = quiet.then(|| t.closes_at - rng.int(2, 12));
        // (the first that does not keep quiet opens; the others now and then)
        let opens = !quiet && (nobody_opened || rng.chance(0.4));
        if opens {
            nobody_opened = false;
        }
        let opening = opens.then(|| t.opens_at + rng.int(3, (len / 4).max(4)));
        out.push(Bidder { character: rv.character, floor, quality: rv.quality, seed: rng.next_u64() ^ k as u64, opening, quiet_until });
    }
    out
}

/// The tender's bids from its opening until `until` (minutes).
pub fn bids(c: &Company, t: &Tender, until: i64) -> Vec<Placed> {
    if t.reference <= 0 || until < t.opens_at {
        return Vec::new();
    }
    auction::replay(&t.lot(), &bidders(c, t), &t.offers, quality_in(c, t), until)
}

/// The name a bid was placed under (the player's: empty).
pub fn bidder_name(c: &Company, t: &Tender, who: Who) -> String {
    match who {
        Who::Player => String::new(),
        Who::Rival(i) => t.bidders.get(i).and_then(|id| rival(c, *id)).map(|r| r.name.clone()).unwrap_or_default(),
    }
}

/// The bid that leads now: the most the company may ask (its 70/30 score leading every other
/// by a margin).
pub fn min_bid(c: &Company, t: &Tender) -> Cents {
    let placed = bids(c, t, clock::now(c));
    let rivals = bidders(c, t);
    auction::player_to_lead(&placed, &rivals, quality_in(c, t), &t.lot())
}

/// The ask recommended now (Normal): the most that leads and wins with `RECOMMEND_CHANCE` as
/// it looks now, not below `RECOMMEND_FLOOR` of the reference; and its chance.
pub fn recommended(c: &Company, t: &Tender) -> (Cents, f64) {
    let top = min_bid(c, t);
    let bottom = round(t.reference as f64 * RECOMMEND_FLOOR).min(top);
    // (the lower the ask, the better the chance: the most that is good enough)
    let (mut lo, mut hi) = (bottom / auction::ROUND, top / auction::ROUND);
    if chance(c, t, hi * auction::ROUND) < RECOMMEND_CHANCE {
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            if chance(c, t, mid * auction::ROUND) >= RECOMMEND_CHANCE {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
    } else {
        lo = hi;
    }
    let amount = lo * auction::ROUND;
    (amount, chance(c, t, amount))
}

/// The company's chance to win with an ask of `amount` now, as the player can reckon it.
pub fn chance(c: &Company, t: &Tender, amount: Cents) -> f64 {
    let placed = bids(c, t, clock::now(c));
    let seen: Vec<Seen> = t
        .bidders
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = rival(c, *id)?;
            let last = placed.iter().rev().find(|p| p.who == Who::Rival(i)).map(|p| p.amount);
            Some(Seen { floor: floor_range(c, t, r.character), quality: r.quality, last })
        })
        .collect();
    auction::chance(amount, quality_in(c, t), &seen, &t.lot())
}

/// The fee with the first bid.
fn pay_fee(c: &mut Company, id: u32) -> Result<(), &'static str> {
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err("There is no such tender.") };
    if !t.fee_paid {
        let f = fee(c, &t);
        if !c.can_pay(f) {
            return Err("Not enough cash.");
        }
        c.book(BookingKind::Concession, -f, format!("Tender line {}", t.number), false);
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
            x.fee_paid = true;
        }
    }
    Ok(())
}

/// Ask a sum on a running tender, now: it has to lead (at most `min_bid`), and the fee is paid
/// with the first bid. (Won, the line runs at that ask: nothing is paid for it.)
pub fn bid(c: &mut Company, id: u32, amount: Cents) -> Result<(), &'static str> {
    let now = clock::now(c);
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err("There is no such tender.") };
    if now < t.opens_at && t.open() {
        return Err("This tender has not opened yet.");
    }
    if !t.running(now) {
        return Err("This tender is closed.");
    }
    level_allows(c, id)?;
    if amount < auction::ROUND {
        return Err("Ask at least 100 €.");
    }
    if !t.counts(amount) {
        return Err("Above the authority's cap: a bid above it does not count.");
    }
    if amount > min_bid(c, &t) {
        return Err("Another bid scores better: ask at most the bid that leads.");
    }
    pay_fee(c, id)?;
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
        x.offers.retain(|o| o.0 != now);
        x.offers.push((now, amount));
    }
    Ok(())
}

/// The company's own bid on a tender, as the pages show it (testers: "how do I know what I bid
/// on my tender?"): its last bid and when, how many it placed, whether it leads now, and how it
/// ended - the winning bid and who won. From the tender's `offers` and `outcome`, both kept with
/// the company (a file older than them reads none).
#[derive(Clone, Debug, PartialEq)]
pub struct MyBid {
    pub amount: Cents,
    pub at: i64,
    pub count: usize,
    /// Its bid leads at `now` (an open tender).
    pub leads: bool,
    /// Decided: who won (empty: the company) and with what.
    pub result: Option<(String, Cents)>,
}

/// The company's bid on `t` at `now` (None: it placed none).
pub fn my_bid(c: &Company, t: &Tender, now: i64) -> Option<MyBid> {
    let &(at, amount) = t.offers.iter().max_by_key(|o| o.0)?;
    let leads = t.open() && bids(c, t, now).last().is_some_and(|p| p.who == Who::Player);
    let result = match &t.outcome {
        Some(Outcome::Won { amount, .. }) => Some((String::new(), *amount)),
        Some(Outcome::Lost { winner, amount, .. } | Outcome::NoBid { winner, amount }) => Some((winner.clone(), *amount)),
        None => None,
    };
    Some(MyBid { amount, at, count: t.offers.len(), leads, result })
}

/// Buy the line at once for the buy-out price (the fee too, if not paid yet): the tender is
/// won and closed; the line is run from tomorrow.
pub fn buy_out(c: &mut Company, id: u32) -> Result<Event, &'static str> {
    let now = clock::now(c);
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err("There is no such tender.") };
    if !t.running(now) {
        return Err(if now < t.opens_at && t.open() { "This tender has not opened yet." } else { "This tender is closed." });
    }
    level_allows(c, id)?;
    let price = t.buy_out();
    if !c.can_pay(price + if t.fee_paid { 0 } else { fee(c, &t) }) {
        return Err("Not enough cash.");
    }
    pay_fee(c, id)?;
    c.book(BookingKind::Concession, -price, format!("Concession line {} (bought)", t.number), false);
    let rivals = last_bids(c, &t, &bids(c, &t, now));
    let ev = take(c, &t, t.reference);
    let score = auction::PRICE_POINTS + auction::quality_points(quality_in(c, &t));
    let today = c.date.clone();
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
        x.outcome = Some(Outcome::Won { score, rivals, amount: price, bought: true });
        x.closes_at = now;
        x.closes = today;
    }
    Ok(ev)
}

/// What the easy mode pays for a concession without its auction (Luc: concessions "bought at
/// the asking price instead of auctions"): what the line is worth to the auction (`value`),
/// and the tender's fee when it is not paid yet.
pub fn asking_price(c: &Company, t: &Tender) -> Cents {
    t.value + if t.fee_paid { 0 } else { fee(c, t) }
}

/// The asking price of a map line's concession: its open tender's, else what a tender of it
/// would ask now.
pub fn asking_price_of(c: &Company, l: &LineInfo) -> Cents {
    if let Some(t) = open_tender(c, &l.name) {
        return asking_price(c, t);
    }
    let w = bc::work_of(l);
    let years = bc::term(Kind::Gross).0;
    let cap = round(bc::price_of(c, &w, Kind::Gross, years as f64) * econ_uk::TENDER_CAP);
    let value = value_at(cap, years);
    value + fee_of(c, w.buses, value)
}

/// Take a tender's line at the asking price, without its auction (the easy mode, whether the
/// tender has opened yet or not): paid now, won and closed - a new line is run from tomorrow,
/// a renewal adds its term. Returns what came of it and what it cost.
pub fn take_at_asking(c: &mut Company, id: u32) -> Result<(Event, Cents), &'static str> {
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err("There is no such tender.") };
    if !t.open() {
        return Err("This tender is closed.");
    }
    level_allows(c, id)?;
    let price = asking_price(c, &t);
    if !c.can_pay(price) {
        return Err("Not enough cash.");
    }
    pay_fee(c, id)?;
    c.book(BookingKind::Concession, -t.value, format!("Concession line {} (asking price)", t.number), false);
    let ev = take(c, &t, t.reference);
    let now = clock::now(c);
    let score = auction::PRICE_POINTS + auction::quality_points(quality_in(c, &t));
    let today = c.date.clone();
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
        x.outcome = Some(Outcome::Won { score, rivals: Vec::new(), amount: t.value, bought: true });
        x.opens_at = x.opens_at.min(now);
        x.closes_at = now;
        x.offered = today.clone();
        x.closes = today;
        x.opened = true;
    }
    Ok((ev, price))
}

/// Take a map line's concession at the asking price (the easy mode's "Take it on"): its tender
/// opened and won at once. Nothing changes when it cannot be paid or the level allows no more.
pub fn take_line(c: &mut Company, l: &LineInfo) -> Result<(Event, Cents), &'static str> {
    let before = c.concessions.clone();
    let id = apply(c, l)?;
    let out = take_at_asking(c, id);
    if out.is_err() {
        c.concessions = before;
    }
    out
}

/// Every bidder's points from its last bid, best first (the company's name empty).
pub fn standings(c: &Company, t: &Tender, placed: &[Placed]) -> Vec<Standing> {
    auction::points(placed, &bidders(c, t), quality_in(c, t)).into_iter().map(|p| Standing { name: bidder_name(c, t, p.who), amount: p.amount, price: p.price, quality: p.quality, total: p.total }).collect()
}

/// The rivals' last bids, best first.
fn last_bids(c: &Company, t: &Tender, placed: &[Placed]) -> Vec<RivalBid> {
    standings(c, t, placed)
        .into_iter()
        .filter(|s| !s.name.is_empty())
        .map(|s| {
            let quality = c.concessions.rivals.iter().find(|r| r.name == s.name).map_or(0.0, |r| r.quality);
            RivalBid { name: s.name, price: t.share(s.amount), quality, score: s.total, amount: s.amount }
        })
        .collect()
}

/// A tender won at `amount` a year: the concession from tomorrow (its line taken on at once:
/// `take_on`), or a renewal added to the old term; its bus contract for the term
/// (`bus_contracts::make`: a renewal's from the old one's end).
fn take(c: &mut Company, t: &Tender, amount: Cents) -> Event {
    let tomorrow = dates::add(&c.date, 1);
    let price = ((t.share(amount)) * 1000.0).round() / 1000.0;
    let work = if t.work == Work::default() { bc::rough_work(t.day_tours, t.day_km) } else { t.work };
    let days = t.term_years() as i64 * 365;
    if let Some(h) = c.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(&t.line)).cloned() {
        let from = dates::add(&h.until, 1);
        let until = dates::add(&h.until, days);
        let k = bc::make(c, &t.line, &t.number, t.kind, t.period, amount, &work, &from, &until);
        c.concessions.contracts.push(k);
        if let Some(h) = c.concessions.held.iter_mut().find(|x| x.line.eq_ignore_ascii_case(&t.line)) {
            h.until = until.clone();
            h.price = price;
            h.direct = false;
        }
        return Event::Won { number: t.number.clone(), until };
    }
    let until = dates::add(&tomorrow, days - 1);
    let k = bc::make(c, &t.line, &t.number, t.kind, t.period, amount, &work, &tomorrow, &until);
    c.concessions.contracts.push(k);
    c.concessions.held.push(Concession { line: t.line.clone(), number: t.number.clone(), from: tomorrow, until: until.clone(), price, direct: false });
    Event::Won { number: t.number.clone(), until }
}

/// Close a tender at its minute: the best 70/30 score wins; won by the company, the line runs
/// at its ask. Returns what came of it and the winning ask (None: closed already).
pub fn close(c: &mut Company, id: u32) -> Option<(Event, Cents)> {
    let t = c.concessions.tenders.iter().find(|t| t.id == id && t.open())?.clone();
    let placed = bids(c, &t, t.closes_at);
    let rivals = last_bids(c, &t, &placed);
    let board = standings(c, &t, &placed);
    let mine = board.iter().find(|s| s.name.is_empty()).map_or(0.0, |s| s.total);
    let (outcome, event, amount) = match placed.last() {
        Some(p) if p.who == Who::Player => {
            let ev = take(c, &t, p.amount);
            (Outcome::Won { score: mine, rivals, amount: p.amount, bought: false }, ev, p.amount)
        }
        Some(p) => {
            let winner = bidder_name(c, &t, p.who);
            let ev = Event::Lost { number: t.number.clone(), winner: winner.clone() };
            let o = if t.offers.is_empty() { Outcome::NoBid { winner, amount: p.amount } } else { Outcome::Lost { score: mine, winner, rivals, amount: p.amount } };
            (o, ev, p.amount)
        }
        None => (Outcome::NoBid { winner: String::new(), amount: 0 }, Event::Lost { number: t.number.clone(), winner: String::new() }, 0),
    };
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == id) {
        x.outcome = Some(outcome);
        x.standings = board;
    }
    Some((event, amount))
}

/// What a closed tender, a concession that ended or the licence came to, for the report.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Won { number: String, until: String },
    Lost { number: String, winner: String },
    Ended { number: String },
}

/// The night after the day `date` was closed (the company's date is the next day already;
/// what is booked here is booked on `date`): tenders that should have closed that day and did
/// not (the clock closes them at their minute) are decided, lines won are taken on for
/// tomorrow, concessions that ended given up (their contracts with them), and on a month's end
/// the own lines' licences. `lines`: the timetable's lines of `date`. (`_km`, each line's
/// kilometres, settled the old per-km prices: a contract pays its line now, `bus_contracts`.)
pub fn after_day(c: &mut Company, date: &str, _km: &[(String, f64)], lines: &[LineInfo]) -> Vec<Event> {
    let mut events = Vec::new();
    let tomorrow = c.date.clone();
    let keep = std::mem::replace(&mut c.date, date.to_string());
    let r = rules_of(c);

    // the tenders that should have closed today
    let end = clock::moment(&tomorrow, 0);
    let due: Vec<(u32, bool)> = c.concessions.tenders.iter().filter(|t| t.open() && t.closes_at > 0 && t.closes_at <= end).map(|t| (t.id, t.renewal || t.applied || !t.offers.is_empty())).collect();
    for (id, tell) in due {
        if let Some((ev, _)) = close(c, id) {
            if tell || matches!(ev, Event::Won { .. }) {
                events.push(ev);
            }
        }
    }

    // lines won, run from tomorrow (those the clock or a page took on already: `take_on`)
    take_on(c, lines);

    // concessions that ended today
    let ended: Vec<Concession> = c.concessions.held.iter().filter(|h| dates::between(&h.until, date) >= 0).cloned().collect();
    for h in ended {
        if r.direct {
            if let Some(x) = c.concessions.held.iter_mut().find(|x| x.line == h.line) {
                x.until = dates::add(&x.until, r.term_weeks as i64 * 7);
            }
            continue;
        }
        bc::expire(c, &h.line, date);
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
pub(crate) mod tests {
    use super::super::model::CompanyLine;
    use super::super::{found, Founding};
    use super::*;
    use crate::{StopInfo, TourInfo, TripInfo};

    /// A map line of `tours` tours of four trips each, from 06:00 hourly (ten kilometres each).
    pub(crate) fn line(name: &str, number: &str, tours: usize, runs: bool) -> LineInfo {
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

    fn company(d: GameMode) -> Company {
        found(&Founding { name: "Tender".into(), game_mode: d, date: "2024-03-04".into(), ..Default::default() }, "Luc")
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
    fn a_round_offers_lines_the_company_does_not_run_one_a_day() {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true), line("oo_12", "12", 1, true), line("Linie9", "9", 4, true)];
        let mut c = company(GameMode::Normal);
        network::add_line(&mut c, &lines[0], None).unwrap();
        assert!(refresh(&mut c, &lines));
        // (the line run already is grandfathered, the player's own one is never offered)
        assert!(of_line(&c, "Linie5").is_some_and(|h| h.direct && h.price == 1.0));
        let offered: Vec<&Tender> = c.concessions.tenders.iter().collect();
        assert_eq!(offered.len(), 2);
        assert!(offered.iter().all(|t| t.line != "Linie5" && t.line != "oo_12"));
        // each runs a few hours, on days one after the other, with rivals of the map
        for (k, t) in offered.iter().enumerate() {
            let hours = (t.closes_at - t.opens_at) as f64 / 60.0;
            assert!((2.0..=6.0).contains(&hours), "{hours}");
            assert_eq!(clock::date_of(t.opens_at), dates::add(&c.date, k as i64));
            assert!(!t.bidders.is_empty() && t.bidders.iter().all(|id| rival(&c, *id).is_some()));
            assert!(t.value >= 2_000_00 && t.buy_out() > t.value);
            assert!(t.scored() && t.reference > t.buy_out());
        }
        assert_eq!(c.concessions.rivals.len(), 6);
        assert!(!refresh(&mut c, &lines), "the same round again changes nothing");
        // the week: five weekdays of 3 tours, a weekend without
        let mut days: Vec<Vec<LineInfo>> = (0..5).map(|_| vec![line("Linie5", "5", 3, true)]).collect();
        days.push(vec![line("Linie5", "5", 3, false)]);
        let w = week_of(&days, "Linie5");
        assert_eq!((w.tours, w.trips, w.peak), (15, 60, 3));
        assert!((w.km - 600.0).abs() < 1e-9);
    }

    #[test]
    fn a_bid_that_leads_at_the_close_wins_and_the_line_is_run_for_the_term() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Normal);
        c.cash = 1_000_000_00;
        c.clock.minute = 10 * 60;
        let id = apply(&mut c, &lines[0]).unwrap();
        assert_eq!(apply(&mut c, &lines[0]), Ok(id), "one tender a line");
        let t = c.concessions.tenders[0].clone();
        assert!(t.running(clock::now(&c)) && t.applied);
        assert_eq!(bid(&mut c, id, t.reference + auction::ROUND), Err("Above the authority's cap: a bid above it does not count."));
        let cash = c.cash;
        // an ask below every rival's floor
        let high = round(t.reference as f64 * 0.6);
        bid(&mut c, id, high).unwrap();
        assert_eq!(cash - c.cash, fee(&c, &c.concessions.tenders[0]), "the fee with the first bid");
        assert!(fee(&c, &t) > 0 && fee(&c, &t) <= t.value / 10 + auction::ROUND, "a tenth of the worth at most");
        assert!(chance(&c, &c.concessions.tenders[0], high) > 0.5);
        // the pages show the company's bid: how much and when, and that it leads
        let at = clock::now(&c);
        let mine = my_bid(&c, &c.concessions.tenders[0], at).unwrap();
        assert_eq!((mine.amount, mine.at, mine.count, mine.leads, mine.result.clone()), (high, at, 1, true, None));
        let (ev, paid) = close(&mut c, id).unwrap();
        // ... and after the result the winning bid, kept with the company
        let kept: Concessions = serde_json::from_str(&serde_json::to_string(&c.concessions).unwrap()).unwrap();
        let mine = my_bid(&c, &kept.tenders[0], at).unwrap();
        assert_eq!((mine.amount, mine.leads, mine.result), (high, false, Some((String::new(), high))));
        assert!(matches!(&ev, Event::Won { number, .. } if number == "7"));
        assert_eq!(paid, high);
        assert!(matches!(c.concessions.tenders[0].outcome, Some(Outcome::Won { amount, bought: false, .. }) if amount == high));
        assert_eq!(cash - c.cash, fee(&c, &c.concessions.tenders[0]), "nothing paid for a line won on its ask");
        // the points of every bidder are kept, the company's (its name empty) first
        let board = &c.concessions.tenders[0].standings;
        assert!(board[0].name.is_empty() && board[0].price == auction::PRICE_POINTS);
        assert!(board.windows(2).all(|w| w[0].total >= w[1].total));
        // the line comes with the market's next look (or the night), in service from tomorrow on
        assert!(!c.lines.iter().any(|l| l.name == "Linie7"));
        ensure(&mut c);
        assert!(of_line(&c, "Linie7").is_some(), "won for tomorrow: kept");
        let today = c.date.clone();
        nights_until(&mut c, &today, &lines);
        assert!(c.lines.iter().any(|l| l.name == "Linie7"));
        let h = of_line(&c, "Linie7").unwrap().clone();
        assert_eq!(h.from.as_str(), c.date.as_str());
        assert!((h.price - t.share(high)).abs() < 0.001, "run at its ask: {}", h.price);
        assert_eq!(dates::between(&h.from, &h.until), econ_uk::TERM_LOCAL.0 as i64 * 365 - 1);
        // (its contract: gross cost or net subsidy at the winning ask, for the term)
        let k = bc::of_line(&c, "Linie7").unwrap();
        assert_eq!((k.year_price, k.from.as_str(), k.until.as_str()), (high, h.from.as_str(), h.until.as_str()));
        // the renewal comes up (its extension years used up); without a bid the line ends with
        // the term
        c.concessions.contracts.iter_mut().for_each(|k| k.extensions_left = 0);
        c.date = dates::add(&h.until, -RENEW_BEFORE);
        refresh(&mut c, &lines);
        let renewal = open_tender(&c, "Linie7").unwrap().clone();
        assert!(renewal.renewal);
        // (closed without a bid, it is not put out again)
        nights_until(&mut c, &renewal.closes, &lines);
        assert!(open_tender(&c, "Linie7").is_none());
        refresh(&mut c, &lines);
        assert!(open_tender(&c, "Linie7").is_none(), "a renewal decided is not put out again");
        let ev = nights_until(&mut c, &h.until.clone(), &lines);
        assert!(ev.iter().any(|e| matches!(e, Event::Ended { number } if number == "7")));
        assert!(!c.lines.iter().any(|l| l.name == "Linie7"));
    }

    /// A tender of `l` won by the company now (bought out).
    fn won(c: &mut Company, l: &LineInfo) {
        c.cash = c.cash.max(1_000_000_00);
        c.clock.minute = 10 * 60;
        let id = apply(c, l).unwrap();
        buy_out(c, id).unwrap();
    }

    #[test]
    fn a_line_won_is_the_companys_at_once_to_be_planned_for_its_first_day() {
        // (Trello #95: tendered lines did not show in RosterFlow - won, the line came only in
        // the night before its first day; a line of Bad Hügelsdorf's is named after its days)
        use super::super::rosterflow as rf;
        let mut sat = line("Samstag", "301", 3, true);
        sat.tours[2].runs = false;
        let lines = vec![sat.clone(), line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Normal);
        won(&mut c, &lines[0]);
        assert!(!c.lines.iter().any(|l| l.name == "Samstag"), "won: not taken on by the auction itself");
        // the market's next look (the launcher's, or the clock's at the day's start) takes it on
        assert!(refresh(&mut c, &lines));
        let tomorrow = dates::add(&c.date, 1);
        let cl = c.lines.iter().find(|l| l.name == "Samstag").unwrap().clone();
        assert_eq!((cl.number.as_str(), cl.added.as_str(), cl.service_from), ("301", tomorrow.as_str(), None));
        assert_eq!(network::late_start(&c, &cl, &c.date), None, "nothing charged before its first day");
        // RosterFlow has its blocks and duties today and tomorrow, by its number
        for date in [c.date.clone(), tomorrow.clone()] {
            let b = rf::board(&rf::view(&c), &lines, &date);
            let mine: Vec<_> = b.tours.iter().filter(|t| t.line == "Samstag").collect();
            assert_eq!(mine.len(), 2, "{date}: the tours that run");
            assert!(mine.iter().all(|t| t.number == "301" && t.unplanned));
            assert!(b.duties.len() >= 2 && b.shaped.len() >= 2);
        }
        // put into service now: from its concession's first day
        let now = clock::now(&c);
        let at = network::start_service(&mut c, "Samstag", now).unwrap();
        assert_eq!(at, clock::moment(&tomorrow, 0));
        let tours = network::tours_of_day(&c, &lines, &tomorrow);
        assert!(tours.iter().filter(|t| t.line == "Samstag").all(|t| !t.unplanned));
        assert!(network::tours_of_day(&c, &lines, &c.date.clone()).iter().filter(|t| t.line == "Samstag").all(|t| t.unplanned), "not today");
        // the night does not take it on twice, and the concession runs from tomorrow
        let today = c.date.clone();
        nights_until(&mut c, &today, &lines);
        assert_eq!(c.lines.iter().filter(|l| l.name == "Samstag").count(), 1);
        assert_eq!(of_line(&c, "Samstag").unwrap().from, c.date);
        refresh(&mut c, &lines);
        assert!(of_line(&c, "Samstag").is_some(), "kept once it runs");
    }

    #[test]
    fn a_line_won_and_given_up_before_its_first_day_goes_with_its_concession() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Normal);
        won(&mut c, &lines[0]);
        refresh(&mut c, &lines);
        assert!(c.lines.iter().any(|l| l.name == "Linie7"));
        network::remove_line(&mut c, "Linie7");
        assert!(of_line(&c, "Linie7").is_none());
        refresh(&mut c, &lines);
        let today = c.date.clone();
        nights_until(&mut c, &today, &lines);
        assert!(!c.lines.iter().any(|l| l.name == "Linie7"), "not taken on again");
    }

    #[test]
    fn a_line_whose_timetable_is_not_read_comes_with_the_tenders_figures() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Normal);
        won(&mut c, &lines[0]);
        assert!(take_on(&mut c, &[]));
        let cl = c.lines.iter().find(|l| l.name == "Linie7").unwrap();
        assert_eq!((cl.number.as_str(), cl.tours), ("7", 2));
        assert!(!take_on(&mut c, &lines), "once");
    }

    #[test]
    fn buying_out_is_certain_and_a_low_bid_loses_more_often() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Realistic);
        c.cash = 1_000_000_00;
        c.clock.minute = 9 * 60;
        let id = apply(&mut c, &lines[0]).unwrap();
        let price = c.concessions.tenders[0].buy_out();
        let cash = c.cash;
        buy_out(&mut c, id).unwrap();
        let t = &c.concessions.tenders[0];
        assert!(matches!(t.outcome, Some(Outcome::Won { bought: true, amount, .. }) if amount == price));
        assert_eq!(cash - c.cash, price + fee(&c, t));
        assert_eq!(buy_out(&mut c, id).err(), Some("This tender is closed."));
        // an ask near the reference against the rivals: lost more often than a low one, and a
        // lost one says who won
        let mut won = [0; 2];
        for n in 0..60 {
            for (k, share) in [0.98, 0.75].into_iter().enumerate() {
                let mut c = company(GameMode::Normal);
                c.id = format!("c{n}");
                c.cash = 1_000_000_00;
                c.clock.minute = 9 * 60;
                let id = apply(&mut c, &lines[0]).unwrap();
                let t = c.concessions.tenders[0].clone();
                let amount = round(t.reference as f64 * share).min(min_bid(&c, &t));
                bid(&mut c, id, amount).unwrap();
                close(&mut c, id).unwrap();
                match &c.concessions.tenders[0].outcome {
                    Some(Outcome::Won { .. }) => won[k] += 1,
                    Some(Outcome::Lost { winner, .. }) => assert!(!winner.is_empty()),
                    o => panic!("{o:?}"),
                }
            }
        }
        assert!(won[0] < won[1] && won[0] < 50 && won[1] > 30, "{won:?}");
    }

    #[test]
    fn a_cheaper_ask_of_lower_quality_can_win_and_a_far_worse_one_loses() {
        let lines = vec![line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Normal);
        c.clock.minute = 9 * 60;
        let id = apply(&mut c, &lines[0]).unwrap();
        let t = c.concessions.tenders[0].clone();
        // the company's quality: its track record (a newcomer's fair one)
        let q = quality_in(&c, &t);
        assert!((q - 92.0).abs() < 1e-9);
        let lot = t.lot();
        // a rival of quality 65 asks 90 %: 5 % below it wins for the company all the same
        let rival_ask = round(t.reference as f64 * 0.9);
        let mine = round(t.reference as f64 * 0.85);
        assert!(auction::score(mine, q, mine) > auction::score(rival_ask, 65.0, mine), "q {q}");
        // ... but with a record of 40 default points a contract, 2 % below it is not enough
        let mut bad = c.clone();
        bc::give(&mut bad, "Linie9");
        let today = bad.date.clone();
        bad.concessions.contracts[0].record.push(bc::PointsDay { date: today, default: 40.0, ..Default::default() });
        let q = quality_in(&bad, &t);
        assert!((q - 20.0).abs() < 1e-9);
        let mine = round(t.reference as f64 * 0.88);
        assert!(auction::score(mine, q, mine) < auction::score(rival_ask, 95.0, mine), "q {q}");
        // the recommended ask leads and wins with the chance it says
        let (ask, p) = recommended(&c, &t);
        assert!(ask <= min_bid(&c, &t) && ask >= round(t.reference as f64 * RECOMMEND_FLOOR).min(min_bid(&c, &t)));
        assert!((0.0..=1.0).contains(&p));
        assert!(bid(&mut c, id, ask).is_ok());
        assert!(lot.ceiling == t.reference);
    }

    #[test]
    fn the_company_level_limits_the_concessions() {
        let lines = vec![line("Linie7", "7", 2, true), line("Linie9", "9", 2, true), line("Linie3", "3", 2, true)];
        let mut c = company(GameMode::Normal);
        c.cash = 1_000_000_00;
        c.clock.minute = 9 * 60;
        assert_eq!(levels::max_concessions(&c), 1);
        let a = apply(&mut c, &lines[0]).unwrap();
        let b = apply(&mut c, &lines[1]).unwrap();
        let ta = c.concessions.tenders.iter().find(|t| t.id == a).unwrap().clone();
        let m = min_bid(&c, &ta);
        bid(&mut c, a, m).unwrap();
        // (a tender it bids in is a concession it may win: not another beside it)
        let tb = c.concessions.tenders.iter().find(|t| t.id == b).unwrap().clone();
        let m = min_bid(&c, &tb);
        assert_eq!(bid(&mut c, b, m), Err(LEVEL_LIMIT));
        assert_eq!(buy_out(&mut c, b).err(), Some(LEVEL_LIMIT));
        buy_out(&mut c, a).unwrap();
        assert_eq!(apply(&mut c, &lines[2]), Err(LEVEL_LIMIT));
        // a higher level, one more
        c.progress.xp = levels::LEVEL_XP[2];
        assert!(buy_out(&mut c, b).is_ok());
    }

    #[test]
    fn depot_runs_and_specials_are_never_offered() {
        let mut depot = line("Betriebsfahrten", "X", 2, true);
        for t in depot.tours.iter_mut().flat_map(|t| t.trips.iter_mut()) {
            t.terminus = "Betriebsfahrt".into();
        }
        let shuttle = line("FFF Shuttle", "", 1, true);
        let lines = vec![depot.clone(), shuttle.clone(), line("Linie7", "7", 2, true)];
        let mut c = company(GameMode::Realistic);
        refresh(&mut c, &lines);
        assert_eq!(c.concessions.tenders.iter().map(|t| t.line.as_str()).collect::<Vec<_>>(), vec!["Linie7"]);
        assert!(apply(&mut c, &depot).is_err() && apply(&mut c, &shuttle).is_err());
    }

    #[test]
    fn tenders_of_older_files_are_read_again() {
        // Bad Hügelsdorf: the weekday timetable opens with the depot run X; an older version
        // named its tender "Line X" after it, and offered other operators' traffic too
        let mut day = line("Montag - Freitag", "301", 2, true);
        let first = &mut day.tours[0].trips[0];
        (first.line, first.name, first.terminus) = ("X".into(), "(leer) Betriebshof VBBH - Hildegardplatz".into(), "Betriebsfahrt".into());
        for t in day.tours.iter_mut().flat_map(|t| t.trips.iter_mut()).skip(1) {
            t.terminus = "Hauptbahnhof                              301".into();
        }
        let mut other = line("251", "251", 1, true);
        other.user_allowed = false;
        let lines = vec![day.clone(), other.clone()];
        let mut c = company(GameMode::Normal);
        c.clock.minute = 9 * 60;
        let now = clock::now(&c);
        let a = new_tender(&mut c, &day, now, false);
        let b = new_tender(&mut c, &other, now, false);
        for t in c.concessions.tenders.iter_mut() {
            (t.number, t.caption) = ("X".into(), "Betriebsfahrt – Hauptbahnhof                              301".into());
        }
        assert!(mend_tenders(&mut c, &lines));
        let t = c.concessions.tenders.iter().find(|t| t.id == a).unwrap();
        assert_eq!((t.number.as_str(), t.caption.as_str()), ("301", "Hauptbahnhof"));
        assert!(c.concessions.tenders.iter().all(|t| t.id != b), "no line of its own: withdrawn");
        assert!(!mend_tenders(&mut c, &lines), "nothing more to mend");
    }

    #[test]
    fn an_easy_economy_takes_lines_directly_and_own_lines_pay_a_licence() {
        let lines = vec![line("Linie5", "5", 3, true)];
        let mut c = company(GameMode::Casual);
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
        let mut d = company(GameMode::Normal);
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

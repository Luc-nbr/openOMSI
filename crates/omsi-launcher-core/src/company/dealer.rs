//! The bus dealer (Luc: "a better dealer"): the installed buses as a showroom by maker, model
//! and version, each with the years it was built in; the day's special offers (new buses
//! from stock at a discount, demonstrators, a batch of used buses another operator sells) and
//! the used market, both new every company day; haggling with the dealer; a contract that is
//! signed before anything is booked, and the bus that comes on its delivery day. Or, for who
//! wants buses quickly, the quick buy: a model, a number, the list price, at once. The
//! company's setting `BuyingMode` says which of the two the dealer shows first.
//!
//! The company's year decides what can be had: a model whose production ended before it is
//! sold second-hand only (as old and as worn as fits), one not built yet is not sold at all.
//! The years come from a user's file (`bus-years.txt`), from the bus's names ("1992-2003"),
//! from a table of the well-known OMSI buses, from a lone year in its names or from its
//! emission standard ("Euro V"); a bus of which nothing is known is always to be had.
//!
//! Time: what happens at a moment - an offer that expires, a bus that is delivered - is kept
//! as "YYYY-MM-DD HH:MM". The company clock calls `tick` with its time; until it does, the
//! pages call it with `now_of` (the company's day at noon).
//!
//! Everything here is plain functions over plain data; the dice are the company's own
//! (`Rng`): the same day draws the same offers and the same answers, also after a restart.

use super::dates;
use super::economy;
use super::finance;
use super::levels;
use super::market::{self, MarketBus, Payment};
use super::model::{BookingKind, BusKind, BusSize, Cents, Company, Difficulty, Drive, Tenure};
use super::rng::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// --- moments ---------------------------------------------------------------------------------

/// Minutes since 1970 of "YYYY-MM-DD HH:MM" (or "YYYY-MM-DDTHH:MM"; a bare date is its
/// first minute). None: not a moment.
pub fn minutes_of(at: &str) -> Option<i64> {
    let at = at.trim();
    let (d, t) = at.split_once([' ', 'T']).unwrap_or((at, "00:00"));
    let day = dates::parse(d)?;
    let (h, m) = t.trim().split_once(':')?;
    let (h, m): (i64, i64) = (h.parse().ok()?, m.get(..2).unwrap_or(m).parse().ok()?);
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    Some(day * 1440 + h * 60 + m)
}

/// A moment as "YYYY-MM-DD HH:MM".
pub fn moment(minutes: i64) -> String {
    let m = minutes.rem_euclid(1440);
    format!("{} {:02}:{:02}", dates::fmt(minutes.div_euclid(1440)), m / 60, m % 60)
}

/// A day at a minute of it.
pub fn at(date: &str, minute: i64) -> String {
    moment(dates::parse(date).unwrap_or(0) * 1440 + minute)
}

/// `at` moved by `minutes` (a moment that cannot be read stays as it is).
pub fn later(at: &str, minutes: i64) -> String {
    minutes_of(at).map(|m| moment(m + minutes)).unwrap_or_else(|| at.to_string())
}

/// The day of a moment.
pub fn day_of(at: &str) -> String {
    at.trim().get(..10).unwrap_or(at).to_string()
}

/// The company's moment now, as its clock has it (`clock::now`).
pub fn now_of(c: &Company) -> String {
    moment(super::clock::now(c))
}

/// `a` is at or before `b`.
fn not_after(a: &str, b: &str) -> bool {
    matches!((minutes_of(a), minutes_of(b)), (Some(a), Some(b)) if a <= b)
}

/// The year of a date or moment.
pub fn year_of(date: &str) -> i32 {
    date.trim().get(..4).and_then(|y| y.parse().ok()).unwrap_or(2000)
}

// --- model years ------------------------------------------------------------------------------

/// The years a model was built in: from the first, to the last (None: still built).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Years {
    pub from: i32,
    #[serde(default)]
    pub to: Option<i32>,
}

impl Years {
    pub fn new(from: i32, to: Option<i32>) -> Years {
        Years { from, to: to.map(|t| t.max(from)) }
    }

    /// The last year a bus of it can have been built in by `year`.
    pub fn last_by(&self, year: i32) -> i32 {
        self.to.unwrap_or(year).min(year)
    }
}

/// Lower-case letters and digits only: "Lion's City" is "lionscity", "SD 202" "sd202".
fn squash(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The well-known OMSI buses (and their real sisters): words all of which are in the bus's
/// names (squashed), and the years the model was built. The first that fits counts: the
/// more particular ones first.
const KNOWN: &[(&[&str], i32, Option<i32>)] = &[
    // MAN
    (&["sd200"], 1973, Some(1985)),
    (&["sd202"], 1984, Some(1992)),
    (&["sl200"], 1973, Some(1988)),
    (&["sl202"], 1984, Some(1993)),
    (&["sg242"], 1985, Some(1992)),
    (&["nl202"], 1989, Some(1993)),
    (&["ng272"], 1992, Some(1998)),
    (&["nl222"], 1993, Some(1998)),
    (&["nl223"], 1997, Some(2004)),
    (&["ng263"], 1998, Some(2004)),
    (&["lionscity", "2018"], 2018, None),
    (&["lionscity"], 1996, Some(2019)),
    // Mercedes-Benz
    (&["o305"], 1967, Some(1987)),
    (&["o405n"], 1989, Some(2001)),
    (&["o405"], 1984, Some(2001)),
    (&["ecitaro"], 2018, None),
    (&["citaro", "c2"], 2011, None),
    (&["citaro", "facelift"], 2005, Some(2012)),
    (&["o530", "facelift"], 2005, Some(2012)),
    (&["o530"], 1997, None),
    (&["citaro"], 1997, None),
    (&["o560"], 2006, None),
    (&["intouro"], 2006, None),
    // Solaris, Volvo, Setra, Ikarus and the others
    (&["urbino", "electric"], 2011, None),
    (&["urbino"], 1999, None),
    (&["volvo", "7900"], 2011, None),
    (&["volvo", "7700"], 2003, Some(2011)),
    (&["s315nf"], 1995, Some(2006)),
    (&["s415nf"], 2005, Some(2016)),
    (&["ikarus", "260"], 1971, Some(2002)),
    (&["ikarus", "280"], 1973, Some(2002)),
    (&["citelis"], 2005, Some(2013)),
    (&["urbanway"], 2013, None),
    (&["citea"], 2009, None),
    (&["citywide"], 2011, None),
    (&["newroutemaster"], 2012, Some(2017)),
    (&["routemaster"], 1956, Some(1968)),
    (&["enviro400"], 2005, None),
];

/// A line of the user's model years file: words that all are in the bus's names, and its
/// years (None: always to be had).
#[derive(Clone, Debug, PartialEq)]
pub struct Override {
    pub words: Vec<String>,
    pub years: Option<Years>,
}

/// The user's model years file, in the data folder.
pub fn overrides_file(data: &Path) -> PathBuf {
    data.join("bus-years.txt")
}

/// What a new model years file says (the user's lines go under it).
pub const OVERRIDES_TEMPLATE: &str = "\
# The years a bus model was built, for the company's dealer (openOMSI).
# One model a line: words of its name, maker, type or folder, \"=\", and its years.
#   MAN SD202 = 1984-1992      built from 1984 to 1992
#   eCitaro = 2018-            built since 2018
#   My bus = always            always to be had
# A line here wins over what openOMSI guesses.
";

/// The lines of a model years file.
pub fn parse_overrides(text: &str) -> Vec<Override> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((names, years)) = line.split_once('=') else { continue };
        let words: Vec<String> = names.split_whitespace().map(squash).filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            continue;
        }
        let y = years.trim().to_lowercase();
        let years = if y == "always" || y == "immer" || y == "altijd" {
            None
        } else {
            let (a, b) = match y.split_once(['-', '–']) {
                Some((a, b)) => (a.trim(), Some(b.trim())),
                None => (y.as_str(), None),
            };
            let Ok(from) = a.parse::<i32>() else { continue };
            let to = match b {
                None => Some(from),
                Some("") => None,
                Some(b) => match b.parse::<i32>() {
                    Ok(t) => Some(t),
                    Err(_) => continue,
                },
            };
            Some(Years::new(from, to))
        };
        out.push(Override { words, years });
    }
    out
}

/// The user's model years (none without the file).
pub fn load_overrides(data: &Path) -> Vec<Override> {
    std::fs::read_to_string(overrides_file(data)).map(|t| parse_overrides(&t)).unwrap_or_default()
}

fn plausible(y: i32) -> bool {
    (1950..=2040).contains(&y)
}

/// The years written in a text: four digits standing alone, and a range of two ("1992-2003",
/// "1992 – 2003", "1992/2003"). With `keyed`, only those after a word that says they are
/// the bus's years ("Baujahr", "built", "bouwjaar"): a description's other years are those of
/// the mod.
fn years_in(text: &str, keyed: bool) -> (Option<(i32, i32)>, Vec<i32>) {
    const KEYS: [&str; 12] = ["baujahr", "bj.", "built", "model year", "modelljahr", "modeljaar", "bouwjaar", "gebaut", "produced", "production", "produktion", "year of"];
    let low = text.to_lowercase();
    let b = low.as_bytes();
    let mut found: Vec<(usize, usize, i32)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let alnum_before = s > 0 && (b[s - 1] as char).is_ascii_alphabetic();
            let alnum_after = i < b.len() && (b[i] as char).is_ascii_alphabetic();
            if i - s == 4 && !alnum_before && !alnum_after {
                if let Ok(y) = low[s..i].parse::<i32>() {
                    if plausible(y) {
                        found.push((s, i, y));
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    let keyed_at = |s: usize| !keyed || KEYS.iter().any(|k| low[..s].rfind(k).is_some_and(|p| s - p <= 24));
    let mut range = None;
    let mut singles = Vec::new();
    let mut k = 0;
    while k < found.len() {
        let (s, e, y) = found[k];
        if let Some(&(s2, _, y2)) = found.get(k + 1) {
            let between = &low[e..s2];
            let sep = between.trim();
            if (sep == "-" || sep == "–" || sep == "—" || sep == "/" || sep == "bis" || sep == "to" || sep == "tot") && y2 >= y && y2 - y <= 40 {
                if range.is_none() && keyed_at(s) {
                    range = Some((y, y2));
                }
                k += 2;
                continue;
            }
        }
        if keyed_at(s) {
            singles.push(y);
        }
        k += 1;
    }
    (range, singles)
}

/// The years an emission standard says ("Euro V", "Euro 6", "EEV"; with `short`, as a bus's
/// name has it too: "o530 U e3" - a sound file's "E5" is no standard).
fn euro_years(texts: &str, short: bool) -> Option<Years> {
    let low = texts.to_lowercase();
    let w: Vec<&str> = low.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let norm = |s: &str| -> Option<u32> {
        match s {
            "1" | "i" => Some(1),
            "2" | "ii" => Some(2),
            "3" | "iii" => Some(3),
            "4" | "iv" => Some(4),
            "5" | "v" => Some(5),
            "6" | "vi" => Some(6),
            _ => None,
        }
    };
    let mut best = None;
    for (k, x) in w.iter().enumerate() {
        let n = if *x == "euro" {
            w.get(k + 1).and_then(|n| norm(n))
        } else if let Some(rest) = x.strip_prefix("euro") {
            norm(rest)
        } else if *x == "eev" {
            Some(5)
        } else if short && x.len() == 2 && x.starts_with('e') && x.as_bytes()[1].is_ascii_digit() {
            norm(&x[1..])
        } else {
            None
        };
        if n.is_some() {
            best = n;
            break;
        }
    }
    Some(match best? {
        1 => Years::new(1992, Some(1996)),
        2 => Years::new(1996, Some(2001)),
        3 => Years::new(2001, Some(2006)),
        4 => Years::new(2006, Some(2009)),
        5 => Years::new(2009, Some(2014)),
        _ => Years::new(2014, None),
    })
}

/// The years both say (None: they do not meet).
fn meet(a: Years, b: Years) -> Option<Years> {
    let from = a.from.max(b.from);
    let to = match (a.to, b.to) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) => x,
        (None, y) => y,
    };
    if to.is_some_and(|t| t < from) {
        None
    } else {
        Some(Years { from, to })
    }
}

/// The years a bus was built in, from its `names` (its name, maker, type, file and folder),
/// its `description` and the names of its `parts` (scripts and sounds): the user's file
/// first, then a range in its names, the table of the well-known ones, a lone year in its
/// names, a year its description gives as its own, its emission standard. None: unknown.
pub fn years_of(names: &[&str], description: &str, parts: &[&str], overrides: &[Override]) -> Option<Years> {
    let squashed: Vec<String> = names.iter().map(|n| squash(n)).collect();
    let all = squashed.join(" ");
    let has = |w: &str| all.contains(w);
    for o in overrides {
        if o.words.iter().all(|w| has(w)) {
            return o.years;
        }
    }
    let text = names.join(" | ");
    let (range, singles) = years_in(&text, false);
    if let Some((a, b)) = range {
        return Some(Years::new(a, Some(b)));
    }
    // (the standard in its name narrows the model's years down: a Citaro "e3" is of 2001-2006)
    let euro = euro_years(&text, true);
    let narrowed = |y: Years| euro.and_then(|e| meet(y, e)).unwrap_or(y);
    if let Some((_, from, to)) = KNOWN.iter().find(|(words, _, _)| words.iter().all(|w| has(w))) {
        return Some(narrowed(Years::new(*from, *to)));
    }
    if let Some(y) = singles.first() {
        return Some(narrowed(Years::new(*y, Some(y + 8))));
    }
    if euro.is_some() {
        return euro;
    }
    let (range, singles) = years_in(description, true);
    if let Some((a, b)) = range {
        return Some(Years::new(a, Some(b)));
    }
    if let Some(y) = singles.first() {
        return Some(Years::new(*y, Some(y + 8)));
    }
    euro_years(&format!("{description} {}", parts.join(" ")), false)
}

/// Whether a model can be had in `year`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// New (and used).
    New,
    /// Its production has ended: second-hand only.
    UsedOnly,
    /// Not built yet: not to be had at all.
    NotYet,
}

pub fn availability(years: Option<Years>, year: i32) -> Availability {
    match years {
        None => Availability::New,
        Some(y) if year < y.from => Availability::NotYet,
        Some(y) if y.to.is_some_and(|t| year > t) => Availability::UsedOnly,
        Some(_) => Availability::New,
    }
}

// --- the showroom -----------------------------------------------------------------------------

/// A bus as the dealer shows it: the market's bus, its family (maker, model, version: the bus
/// step's tree), its years and what its cabin holds.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub bus: MarketBus,
    pub maker: String,
    pub model: String,
    pub version: String,
    pub years: Option<Years>,
    pub seats: Option<u32>,
    pub standing: Option<u32>,
}

impl Listing {
    pub fn availability(&self, year: i32) -> Availability {
        availability(self.years, year)
    }
}

/// What a bus's files say beyond its kind: seats and standing places (its passenger cabin
/// and its trailer's: a seat has a height, a standing place none), and the names of its
/// scripts and sounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Specs {
    pub seats: Option<u32>,
    pub standing: Option<u32>,
    pub parts: Vec<String>,
}

/// Read a bus's specs from its files (None of a count: no cabin).
pub fn read_specs(file: &str) -> Specs {
    let path = crate::resolve_content(file).unwrap_or_else(|_| PathBuf::from(file));
    let Ok(v) = omsi_vehicle::Vehicle::load(&path) else { return Specs::default() };
    let mut s = Specs::default();
    let mut count = |v: &omsi_vehicle::Vehicle| {
        if let Some(cab) = v.passenger_cabin.as_ref() {
            if let Ok(c) = omsi_vehicle::cabin::PassengerCabin::load(&omsi_cfg::resolve_path(v.dir(), cab)) {
                let seats = c.pass_positions.iter().filter(|p| p.height > 0.01).count() as u32;
                let standing = c.pass_positions.len() as u32 - seats;
                *s.seats.get_or_insert(0) += seats;
                *s.standing.get_or_insert(0) += standing;
            }
        }
    };
    count(&v);
    if let Some(back) = v.couple_back_path() {
        if let Ok(t) = omsi_vehicle::Vehicle::load(&back) {
            count(&t);
        }
    }
    let names = |ps: &[PathBuf]| ps.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect::<Vec<_>>();
    s.parts = names(&v.scripts.scripts);
    s.parts.extend(names(&v.scripts.constfiles));
    if let Some(snd) = &v.sound {
        s.parts.push(snd.clone());
    }
    s
}

/// A listing of an installed bus: `family` is its maker, model and version as the bus step
/// groups them.
pub fn listing_of(v: &crate::VehicleInfo, bus: MarketBus, family: (String, String, String), specs: &Specs, overrides: &[Override]) -> Listing {
    let names = [v.name.as_str(), v.manufacturer.as_str(), v.type_name.as_str(), v.file.as_str(), v.folder.as_str()];
    let parts: Vec<&str> = specs.parts.iter().map(String::as_str).collect();
    let years = years_of(&names, &v.description, &parts, overrides);
    Listing { bus, maker: family.0, model: family.1, version: family.2, years, seats: specs.seats, standing: specs.standing }
}

/// What the showroom's filters ask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub search: String,
    pub size: Option<BusSize>,
    pub drive: Option<Drive>,
    /// Some(true): to be had new; Some(false): second-hand only.
    pub new: Option<bool>,
    /// The highest new price (0: any).
    pub max_price: Cents,
    /// Built in this year (0: any).
    pub year: i32,
}

impl Filter {
    pub fn fits(&self, l: &Listing, c: &Company) -> bool {
        let year = year_of(&c.date);
        let a = l.availability(year);
        if a == Availability::NotYet {
            return false;
        }
        let q = self.search.trim().to_lowercase();
        if !q.is_empty() && ![&l.bus.name, &l.maker, &l.model, &l.version, &l.bus.file].iter().any(|s| s.to_lowercase().contains(&q)) {
            return false;
        }
        if self.size.is_some_and(|s| s != l.bus.kind.size) || self.drive.is_some_and(|d| d != l.bus.kind.drive) {
            return false;
        }
        match self.new {
            Some(true) if a != Availability::New => return false,
            Some(false) if a != Availability::UsedOnly => return false,
            _ => {}
        }
        if self.max_price > 0 && list_price(c, l.bus.kind) > self.max_price {
            return false;
        }
        if self.year > 0 && l.years.is_some_and(|y| self.year < y.from || y.to.is_some_and(|t| self.year > t)) {
            return false;
        }
        true
    }
}

/// A new bus's list price today.
pub fn list_price(c: &Company, kind: BusKind) -> Cents {
    economy::new_price(kind, &economy::rules(c.difficulty), c.price_index)
}

/// What painting a bus in another livery than its own costs.
pub fn painting_cost(c: &Company) -> Cents {
    round_to(3_500_00 as f64 * c.price_index, 100_00)
}

fn round_to(c: f64, step: Cents) -> Cents {
    ((c / step as f64).round() as Cents) * step
}

// --- the dealer's terms by difficulty -------------------------------------------------------

/// What a difficulty makes of the dealer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Terms {
    /// The dealer's margin on a new bus (of the list price), and the extra on a used one.
    pub margin: f64,
    pub used_margin: f64,
    /// How long he listens: rounds, and his patience (1: average).
    pub rounds: u32,
    pub patience: f64,
    /// Days from signing to a new bus's delivery (one not in stock).
    pub delivery_days: i64,
    /// Warranty in months: a new bus's, a used one's, and what an extended warranty adds.
    pub warranty_new: u32,
    pub warranty_used: u32,
    /// Days the dealer will not talk to a company that pushed too hard.
    pub sulk_days: i64,
}

pub fn terms(d: Difficulty) -> Terms {
    match d {
        Difficulty::Easy => Terms { margin: 0.16, used_margin: 0.06, rounds: 7, patience: 1.4, delivery_days: 5, warranty_new: 24, warranty_used: 12, sulk_days: 1 },
        Difficulty::Realistic => Terms { margin: 0.11, used_margin: 0.05, rounds: 5, patience: 1.0, delivery_days: 14, warranty_new: 24, warranty_used: 6, sulk_days: 2 },
        Difficulty::Hard => Terms { margin: 0.07, used_margin: 0.04, rounds: 4, patience: 0.75, delivery_days: 28, warranty_new: 12, warranty_used: 3, sulk_days: 4 },
    }
}

/// Months of warranty an extended warranty adds.
pub const EXTRA_WARRANTY_MONTHS: u32 = 12;

/// The days from signing to the delivery of a new bus of `maker`'s (the same model takes the
/// same time that day): from stock two, else the difficulty's with a spread; a faster
/// delivery halves it.
pub fn delivery_days(c: &Company, key: &str, stock: bool, fast: bool) -> i64 {
    let base = if stock {
        2
    } else {
        let mut rng = Rng::of(&[&c.id, "delivery", key], dates::parse(&c.date).unwrap_or(0));
        (terms(c.difficulty).delivery_days as f64 * rng.range(0.8, 1.4)).round() as i64
    };
    if fast {
        (base / 2).max(1)
    } else {
        base
    }
}

// --- the offers of the day and the used market ----------------------------------------------

/// What an offer is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    /// A used bus of the used market.
    Used,
    /// New buses from the dealer's stock at a discount (delivered in two days).
    Discount,
    /// The dealer's demonstrator: nearly new, a few thousand kilometres.
    Demonstrator,
    /// Several used buses of one model that another operator sells.
    Batch,
}

impl OfferKind {
    pub fn label(self) -> &'static str {
        match self {
            OfferKind::Used => "Used",
            OfferKind::Discount => "Special offer",
            OfferKind::Demonstrator => "Demonstrator",
            OfferKind::Batch => "Fleet sale",
        }
    }
}

/// An offer of the dealer's (a used bus, or one of the day's).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Offer {
    /// "u<day>-<n>" (the used market) or "o<day>-<n>" (the day's offers).
    pub id: String,
    pub kind: OfferKind,
    pub listing: Listing,
    /// Buses left of it.
    pub count: u32,
    /// When they were built (a new one: today), their kilometres and condition.
    pub built: String,
    pub km: f64,
    pub condition: f64,
    /// The price of one, and what one costs otherwise (new: its list price; used: its value).
    pub price: Cents,
    pub reference: Cents,
    /// Who sells it.
    pub seller: String,
    pub published: String,
    pub expires: String,
}

impl Offer {
    /// A new bus (delivered from stock), not a used one.
    pub fn is_new(&self) -> bool {
        self.kind == OfferKind::Discount
    }

    pub fn age_years(&self, today: &str) -> f64 {
        dates::years_between(&self.built, today)
    }

    /// The saving against what it costs otherwise, 0..1.
    pub fn saving(&self) -> f64 {
        if self.reference <= 0 {
            0.0
        } else {
            (1.0 - self.price as f64 / self.reference as f64).max(0.0)
        }
    }
}

/// Operators that sell their old buses (made up).
const OPERATORS: [&str; 10] = [
    "Stadtwerke Lindenau",
    "Verkehrsbetriebe Ostheim",
    "Regiobus Mittelland",
    "Kreisverkehr Altenburg",
    "Busbetrieb Nordheide",
    "Stadtverkehr Weißenfels",
    "Overland Vervoer Achterhoek",
    "Transports Urbains de Valmont",
    "Rheintal Mobil",
    "Bergland Linien",
];

/// The dealer of a maker's buses.
pub fn dealer_name(maker: &str) -> String {
    if maker.trim().is_empty() {
        "Bus dealer".to_string()
    } else {
        format!("{} dealer", maker.trim())
    }
}

/// The used bus centre (where the used market is).
pub const USED_CENTRE: &str = "Used bus centre";

/// How many days an offer runs back: the used market's buses stay four days, the day's offers
/// up to five.
const OFFER_DAYS: i64 = 5;
const USED_DAYS: i64 = 4;

fn used_price(c: &Company, kind: BusKind, age: f64, km: f64, condition: f64) -> (Cents, Cents) {
    let r = economy::rules(c.difficulty);
    let value = economy::book_value(economy::new_price(kind, &r, c.price_index), age, km, condition);
    (round_to(value as f64 * r.used_markup, 500_00), round_to(value as f64, 100_00))
}

/// A used bus of a model as old as fits `year`: built between its first year (and at most 25
/// years back) and its last (and a year back). None: no such bus can be used yet.
fn used_build(l: &Listing, today: i64, rng: &mut Rng, ages: (f64, f64)) -> Option<(String, f64)> {
    let year = dates::civil_from_days(today).0;
    let (lo, hi) = match l.years {
        Some(y) => ((y.from).max(year - 25), y.last_by(year - 1)),
        None => (year - ages.1.round() as i32, year - ages.0.round() as i32),
    };
    if hi < lo {
        return None;
    }
    let built_year = rng.int(lo as i64, hi as i64) as i32;
    let day = dates::days_from_civil(built_year, rng.int(1, 12) as u32, rng.int(1, 28) as u32).min(today - 200);
    let age = (today - day) as f64 / 365.25;
    Some((dates::fmt(day), age))
}

/// The used market's buses put up on `day`: each stays four days or until sold.
fn used_of_day(c: &Company, listings: &[Listing], day: i64) -> Vec<Offer> {
    let year = dates::civil_from_days(day).0;
    let pool: Vec<&Listing> = listings.iter().filter(|l| l.availability(year) != Availability::NotYet).collect();
    if pool.is_empty() {
        return Vec::new();
    }
    let r = economy::rules(c.difficulty);
    let mut rng = Rng::of(&[&c.id, "used-market"], day);
    let n = (r.used_offers / 2).max(2);
    let date = dates::fmt(day);
    let mut out = Vec::new();
    for k in 0..n {
        let Some(l) = rng.pick(&pool).copied() else { break };
        let Some((built, age)) = used_build(l, day, &mut rng, (2.0, 15.0)) else { continue };
        let km = (age * rng.range(45_000.0, 72_000.0) / 1000.0).round() * 1000.0;
        let condition = (95.0 - age * 3.0 + rng.range(-15.0, 10.0)).clamp(20.0, 92.0).round();
        let (price, value) = used_price(c, l.bus.kind, age, km, condition);
        out.push(Offer {
            id: format!("u{day}-{k}"),
            kind: OfferKind::Used,
            listing: l.clone(),
            count: 1,
            built,
            km,
            condition,
            price,
            reference: value,
            seller: USED_CENTRE.to_string(),
            published: at(&date, 7 * 60),
            expires: at(&dates::add(&date, USED_DAYS), 7 * 60),
        });
    }
    out
}

/// The special offers put up on `day`: two or three of a discount on new buses from stock, a
/// demonstrator, a batch of used buses another operator sells.
fn offers_of_day(c: &Company, listings: &[Listing], day: i64) -> Vec<Offer> {
    let year = dates::civil_from_days(day).0;
    let new: Vec<&Listing> = listings.iter().filter(|l| l.availability(year) == Availability::New).collect();
    let used: Vec<&Listing> = listings.iter().filter(|l| l.availability(year) != Availability::NotYet).collect();
    let mut rng = Rng::of(&[&c.id, "offers"], day);
    let date = dates::fmt(day);
    let r = economy::rules(c.difficulty);
    let easy = if c.difficulty == Difficulty::Easy { 0.03 } else { 0.0 };
    let n = rng.int(2, 3);
    let mut out = Vec::new();
    for k in 0..n {
        let roll = rng.f64();
        let days = rng.int(2, OFFER_DAYS);
        let published = at(&date, rng.int(7, 10) * 60);
        let expires = at(&dates::add(&date, days), 18 * 60);
        let id = format!("o{day}-{k}");
        if roll < 0.4 {
            let Some(l) = rng.pick(&new).copied() else { continue };
            let list = economy::new_price(l.bus.kind, &r, c.price_index);
            let off = rng.range(0.06, 0.14) + easy;
            out.push(Offer { id, kind: OfferKind::Discount, listing: l.clone(), count: rng.int(1, 4) as u32, built: date.clone(), km: 0.0, condition: 100.0, price: round_to(list as f64 * (1.0 - off), 100_00), reference: list, seller: dealer_name(&l.maker), published, expires });
        } else if roll < 0.7 {
            let Some(l) = rng.pick(&new).copied() else { continue };
            let list = economy::new_price(l.bus.kind, &r, c.price_index);
            let months = rng.int(4, 14);
            let built = dates::add(&date, -(months as f64 * 30.44).round() as i64);
            let km = (rng.range(5_000.0, 35_000.0) / 100.0).round() * 100.0;
            out.push(Offer {
                id,
                kind: OfferKind::Demonstrator,
                listing: l.clone(),
                count: 1,
                built,
                km,
                condition: rng.range(93.0, 98.0).round(),
                price: round_to(list as f64 * (rng.range(0.74, 0.84) - easy), 100_00),
                reference: list,
                seller: dealer_name(&l.maker),
                published,
                expires,
            });
        } else {
            let Some(l) = rng.pick(&used).copied() else { continue };
            let Some((built, age)) = used_build(l, day, &mut rng, (6.0, 14.0)) else { continue };
            let km = (age * rng.range(50_000.0, 68_000.0) / 1000.0).round() * 1000.0;
            let condition = (90.0 - age * 2.8 + rng.range(-8.0, 6.0)).clamp(30.0, 90.0).round();
            let (price, value) = used_price(c, l.bus.kind, age, km, condition);
            let seller = rng.pick(&OPERATORS).copied().unwrap_or(OPERATORS[0]).to_string();
            out.push(Offer { id, kind: OfferKind::Batch, listing: l.clone(), count: rng.int(3, 6) as u32, built, km, condition, price: round_to(price as f64 * 0.88, 500_00), reference: value, seller, published, expires });
        }
    }
    out
}

/// What of an offer is sold already.
fn taken_of(c: &Company, id: &str) -> u32 {
    c.dealer.taken.iter().filter(|t| t.id == id).map(|t| t.count).sum()
}

fn open_at(o: &Offer, now: &str) -> bool {
    not_after(&o.published, now) && !not_after(&o.expires, now)
}

/// The used market at `now`: the buses of the last days not sold and not expired.
pub fn used_market(c: &Company, listings: &[Listing], now: &str) -> Vec<Offer> {
    gather(c, listings, now, USED_DAYS, used_of_day)
}

/// The special offers open at `now`, the newest first.
pub fn day_offers(c: &Company, listings: &[Listing], now: &str) -> Vec<Offer> {
    gather(c, listings, now, OFFER_DAYS, offers_of_day)
}

fn gather(c: &Company, listings: &[Listing], now: &str, back: i64, of_day: fn(&Company, &[Listing], i64) -> Vec<Offer>) -> Vec<Offer> {
    let Some(today) = dates::parse(&day_of(now)) else { return Vec::new() };
    let mut out = Vec::new();
    for day in (today - back..=today).rev() {
        for mut o in of_day(c, listings, day) {
            o.count = o.count.saturating_sub(taken_of(c, &o.id));
            if o.count > 0 && open_at(&o, now) {
                out.push(o);
            }
        }
    }
    out
}

/// An offer by its id, if it is still open at `now`.
pub fn offer(c: &Company, listings: &[Listing], id: &str, now: &str) -> Option<Offer> {
    day_offers(c, listings, now).into_iter().chain(used_market(c, listings, now)).find(|o| o.id == id)
}

// --- haggling -----------------------------------------------------------------------------------

/// Something more than the price to haggle for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Extra {
    /// The first service in the company's workshop is the dealer's.
    FreeService,
    /// Delivered in half the time.
    FastDelivery,
    /// Painted in the livery chosen at no cost.
    Painting,
    /// A year more of warranty.
    Warranty,
}

impl Extra {
    pub const ALL: [Extra; 4] = [Extra::FreeService, Extra::FastDelivery, Extra::Painting, Extra::Warranty];

    pub fn label(self) -> &'static str {
        match self {
            Extra::FreeService => "Free first service",
            Extra::FastDelivery => "Faster delivery",
            Extra::Painting => "Livery painting included",
            Extra::Warranty => "A year more warranty",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Extra::FreeService => "construction",
            Extra::FastDelivery => "speed",
            Extra::Painting => "palette",
            Extra::Warranty => "badge",
        }
    }
}

/// What an extra costs the dealer for one bus of `list`.
pub fn extra_value(c: &Company, e: Extra, list: Cents) -> Cents {
    match e {
        Extra::FreeService => round_to(1_200_00 as f64 * c.price_index, 100_00),
        Extra::FastDelivery => round_to(list as f64 * 0.01, 100_00),
        Extra::Painting => painting_cost(c),
        Extra::Warranty => round_to(list as f64 * 0.02, 100_00),
    }
}

/// What is talked about.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Quote {
    /// "new:<bus file>", or the offer's id.
    pub key: String,
    pub maker: String,
    pub kind: BusKind,
    /// The price of one asked at the start.
    pub list: Cents,
    pub used: bool,
    /// From the dealer's stock (a special offer: less to give).
    pub stock: bool,
    pub count: u32,
}

impl Quote {
    pub fn new_bus(c: &Company, l: &Listing, count: u32) -> Quote {
        Quote { key: format!("new:{}", l.bus.file), maker: l.maker.clone(), kind: l.bus.kind, list: list_price(c, l.bus.kind), used: false, stock: false, count: count.max(1) }
    }

    pub fn of_offer(o: &Offer, count: u32) -> Quote {
        Quote { key: o.id.clone(), maker: o.listing.maker.clone(), kind: o.listing.bus.kind, list: o.price, used: !o.is_new(), stock: o.is_new(), count: count.clamp(1, o.count.max(1)) }
    }
}

/// A move of the company's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// "Can you do something on the price?"
    AskDiscount,
    /// A price for one bus.
    Offer(Cents),
    AskExtra(Extra),
    /// Take what is on the table.
    Accept,
}

/// The dealer's answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// The price agreed (for one).
    Accepted(Cents),
    /// He comes down to this.
    Discount(Cents),
    /// He meets the offer halfway: this.
    Counter(Cents),
    /// Not a cent less.
    Firm,
    /// That offer is no offer: he asks this still.
    TooLow(Cents),
    ExtraGranted(Extra),
    ExtraRefused(Extra),
    /// His last word: this, or nothing.
    LastOffer(Cents),
    /// He has had enough: no talks until then.
    BrokeOff(String),
}

/// A talk with the dealer about one thing on one day (the next day it starts anew; reopened
/// the same day it goes on where it stopped).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Talk {
    pub quote: Quote,
    pub day: String,
    /// What he asks for one now, and the least he would take (not shown).
    pub asking: Cents,
    pub floor: Cents,
    pub rounds: u32,
    pub max_rounds: u32,
    /// 0..: at 0 he breaks off.
    pub patience: f64,
    pub extras: Vec<Extra>,
    pub refused: Vec<Extra>,
    pub replies: Vec<Reply>,
    /// Agreed (the price is `asking`), or his last word was said, or he broke off.
    pub agreed: bool,
    pub closed: bool,
}

impl Talk {
    pub fn rounds_left(&self) -> u32 {
        self.max_rounds.saturating_sub(self.rounds)
    }

    /// What is off the list price so far, 0..1.
    pub fn discount(&self) -> f64 {
        if self.quote.list <= 0 {
            0.0
        } else {
            1.0 - self.asking as f64 / self.quote.list as f64
        }
    }
}

/// How much the dealer can give on `q`, of its price: his margin, less on a bus in demand
/// (electric ones) or from stock, more for a company of good reputation and level, for one
/// that bought many buses of him (the fleet discount) and for more than one bus at a time.
pub fn room(c: &Company, q: &Quote) -> f64 {
    let t = terms(c.difficulty);
    let mut room = if q.used { t.margin * 0.6 + t.used_margin } else { t.margin };
    room *= match (q.kind.drive, q.kind.size) {
        (Drive::Electric, _) => 0.7,
        (_, BusSize::Double) => 0.9,
        (_, BusSize::Midi) => 1.1,
        _ => 1.0,
    };
    if q.stock {
        room *= 0.4;
    }
    room += (c.reputation - 50.0) / 50.0 * 0.02;
    room += levels::level(c) as f64 * 0.003;
    room += c.dealer.bought.min(10) as f64 * 0.004;
    room += (q.count.saturating_sub(1)).min(5) as f64 * 0.01;
    room.clamp(0.02, 0.30)
}

/// Talking to `maker`'s dealer is off until this moment (he broke off).
pub fn sulking(c: &Company, maker: &str, now: &str) -> Option<String> {
    c.dealer.breaks.iter().find(|b| b.0.eq_ignore_ascii_case(maker) && !not_after(&b.1, now)).map(|b| b.1.clone())
}

/// Open a talk about `q` (or go on with today's): None when the dealer will not talk now.
pub fn open_talk(c: &Company, q: &Quote, now: &str) -> Result<Talk, &'static str> {
    if sulking(c, &q.maker, now).is_some() {
        return Err("The dealer does not want to talk to you for now.");
    }
    let day = day_of(now);
    if let Some(t) = c.dealer.talks.iter().find(|t| t.quote.key == q.key && t.day == day && t.quote.count == q.count) {
        return Ok(t.clone());
    }
    let t = terms(c.difficulty);
    let mut rng = Rng::of(&[&c.id, "floor", &q.key], dates::parse(&day).unwrap_or(0));
    let give = room(c, q) * rng.range(0.8, 1.15);
    let floor = round_to(q.list as f64 * (1.0 - give), 100_00).min(q.list);
    Ok(Talk { quote: q.clone(), day, asking: q.list, floor, rounds: 0, max_rounds: t.rounds, patience: t.patience, extras: Vec::new(), refused: Vec::new(), replies: Vec::new(), agreed: false, closed: false })
}

/// The dealer's answer to a move, kept in the talk (and the talk in the company, so that it
/// goes on the same day).
pub fn respond(c: &mut Company, talk: &mut Talk, mv: Move, now: &str) -> Reply {
    if talk.closed {
        // (his last word can still be taken; a talk he broke off cannot)
        let broke = matches!(talk.replies.last(), Some(Reply::BrokeOff(_)));
        if mv == Move::Accept && !talk.agreed && !broke {
            talk.agreed = true;
            talk.replies.push(Reply::Accepted(talk.asking));
            keep_talk(c, talk);
            return Reply::Accepted(talk.asking);
        }
        return talk.replies.last().cloned().unwrap_or(Reply::Firm);
    }
    let day = dates::parse(&talk.day).unwrap_or(0);
    let mut rng = Rng::of(&[&c.id, "talk", &talk.quote.key], day * 64 + talk.rounds as i64);
    talk.rounds += 1;
    talk.patience -= 0.1;
    let gap = (talk.asking - talk.floor).max(0);
    let list = talk.quote.list.max(1);
    let mut reply = match mv {
        Move::Accept => {
            talk.agreed = true;
            talk.closed = true;
            Reply::Accepted(talk.asking)
        }
        Move::AskDiscount => {
            if gap >= 100_00 && rng.chance(0.45 + 0.3 * talk.patience.clamp(0.0, 1.0)) {
                let step = round_to(gap as f64 * rng.range(0.25, 0.5), 100_00).max(100_00);
                talk.asking -= step.min(gap);
                Reply::Discount(talk.asking)
            } else {
                talk.patience -= if gap < 100_00 { 0.25 } else { 0.1 };
                Reply::Firm
            }
        }
        Move::Offer(x) => {
            if x >= talk.asking {
                talk.agreed = true;
                talk.closed = true;
                Reply::Accepted(talk.asking)
            } else if x >= talk.floor {
                let p = if gap == 0 { 1.0 } else { (x - talk.floor) as f64 / gap as f64 };
                if rng.chance(0.1 + 0.75 * p.powf(0.7)) {
                    talk.asking = x;
                    talk.agreed = true;
                    talk.closed = true;
                    Reply::Accepted(x)
                } else {
                    let counter = (x as f64 + (talk.asking - x) as f64 * rng.range(0.35, 0.65)) as Cents;
                    let counter = (((counter + 100_00 - 1) / 100_00) * 100_00).clamp(talk.floor, talk.asking);
                    talk.asking = counter;
                    Reply::Counter(counter)
                }
            } else {
                // (a low offer costs patience, the lower the more)
                let below = (talk.floor - x) as f64 / list as f64;
                talk.patience -= 0.2 + 2.0 * below;
                let counter = round_to(talk.asking as f64 - gap as f64 * rng.range(0.1, 0.3), 100_00).clamp(talk.floor, talk.asking);
                talk.asking = counter;
                Reply::TooLow(counter)
            }
        }
        Move::AskExtra(e) => {
            let cost = extra_value(c, e, list);
            if talk.extras.contains(&e) {
                Reply::ExtraGranted(e)
            } else if gap >= cost && rng.chance(0.35 + 0.45 * talk.patience.clamp(0.0, 1.0)) {
                talk.extras.push(e);
                talk.refused.retain(|x| *x != e);
                // (what the extra costs him is off what he can still give)
                talk.floor = (talk.floor + cost).min(talk.asking);
                Reply::ExtraGranted(e)
            } else {
                talk.patience -= 0.15;
                if !talk.refused.contains(&e) {
                    talk.refused.push(e);
                }
                Reply::ExtraRefused(e)
            }
        }
    };
    if !talk.closed {
        if talk.patience <= 0.0 {
            let until = later(now, terms(c.difficulty).sulk_days * 1440);
            c.dealer.breaks.retain(|b| !b.0.eq_ignore_ascii_case(&talk.quote.maker));
            c.dealer.breaks.push((talk.quote.maker.clone(), until.clone()));
            talk.closed = true;
            talk.asking = talk.quote.list;
            talk.extras.clear();
            reply = Reply::BrokeOff(until);
        } else if talk.rounds >= talk.max_rounds && !matches!(reply, Reply::Accepted(_)) {
            talk.closed = true;
            reply = Reply::LastOffer(talk.asking);
        }
    }
    talk.replies.push(reply.clone());
    keep_talk(c, talk);
    reply
}

/// The talk kept in the company (one per thing and day).
fn keep_talk(c: &mut Company, talk: &Talk) {
    c.dealer.talks.retain(|t| !(t.quote.key == talk.quote.key && t.day == talk.day));
    c.dealer.talks.push(talk.clone());
}

// --- the contract -------------------------------------------------------------------------------

/// How a contract is paid.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PayWay {
    #[default]
    Cash,
    /// The bank lends the amount (see `finance`).
    Loan,
    /// Leased: a rate every month, no price now (new buses only).
    Lease,
}

impl PayWay {
    pub fn label(self) -> &'static str {
        match self {
            PayWay::Cash => "Cash",
            PayWay::Loan => "Bank loan",
            PayWay::Lease => "Leasing",
        }
    }
}

/// A contract of purchase: who sells and buys, what, how many, at what price, paid how,
/// delivered when, with what warranty and extras - and the signature that makes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Contract {
    /// Its number (given when it is signed).
    pub no: u32,
    pub seller: String,
    pub buyer: String,
    pub listing: Listing,
    pub livery: String,
    pub count: u32,
    /// A new bus (else used: built, km and condition as they are).
    pub new: bool,
    pub built: String,
    pub km: f64,
    pub condition: f64,
    /// The offer it buys (its id), if any.
    pub offer: Option<String>,
    /// Per bus: the list price, the agreed price, the grant and the painting.
    pub list: Cents,
    pub price: Cents,
    pub grant: Cents,
    pub painting: Cents,
    pub pay: PayWay,
    /// Days from signing to delivery (0: at once).
    pub delivery_days: i64,
    pub warranty_months: u32,
    pub extras: Vec<Extra>,
    /// The name signed with, the strokes drawn (0..1 in the signature field), and when.
    pub signed_by: String,
    #[serde(default)]
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub signed_at: String,
}

impl Contract {
    /// The price of all the buses with their painting.
    pub fn total(&self) -> Cents {
        (self.price + self.painting) * self.count as Cents
    }

    pub fn grants(&self) -> Cents {
        self.grant * self.count as Cents
    }

    /// What is paid when it is signed (cash or loan): the total less the grants.
    pub fn due(&self) -> Cents {
        self.total() - self.grants()
    }

    /// A leased bus's monthly rate, the term and its residual value (of the agreed price).
    pub fn lease(&self, c: &Company) -> (Cents, u32, Cents) {
        let r = economy::rules(c.difficulty);
        let base = self.price + self.painting;
        (economy::lease_monthly(base, &r), r.lease_months, (base as f64 * r.lease_residual).round() as Cents)
    }

    /// When it is delivered, signed at `now`: a new bus at eight in the morning of its day.
    pub fn delivery(&self, now: &str) -> String {
        if self.delivery_days <= 0 {
            now.to_string()
        } else {
            at(&dates::add(&day_of(now), self.delivery_days), 8 * 60)
        }
    }

    pub fn is_signed(&self) -> bool {
        !self.signed_by.trim().is_empty() || self.strokes.iter().any(|s| s.len() > 1)
    }
}

/// The contract for `count` new buses of a listing at `price` each (from a talk, the list
/// price, or a special offer from stock).
pub fn draft_new(c: &Company, l: &Listing, count: u32, price: Cents, extras: &[Extra], livery: &str, stock: Option<&Offer>) -> Contract {
    let r = economy::rules(c.difficulty);
    let t = terms(c.difficulty);
    let key = format!("new:{}", l.bus.file);
    let fast = extras.contains(&Extra::FastDelivery);
    let painting = if livery.is_empty() || extras.contains(&Extra::Painting) { 0 } else { painting_cost(c) };
    Contract {
        seller: dealer_name(&l.maker),
        buyer: c.name.clone(),
        listing: l.clone(),
        livery: livery.to_string(),
        count: count.max(1),
        new: true,
        built: c.date.clone(),
        km: 0.0,
        condition: 100.0,
        offer: stock.map(|o| o.id.clone()),
        list: list_price(c, l.bus.kind),
        price,
        grant: economy::grant(l.bus.kind, price, &r, c.price_index),
        painting,
        pay: PayWay::Cash,
        delivery_days: delivery_days(c, &key, stock.is_some(), fast),
        warranty_months: t.warranty_new + if extras.contains(&Extra::Warranty) { EXTRA_WARRANTY_MONTHS } else { 0 },
        extras: extras.to_vec(),
        ..Default::default()
    }
}

/// The contract for `count` buses of a used offer (or a demonstrator, or a batch) at `price`
/// each: delivered at once. A special offer of new buses is `draft_new`'s.
pub fn draft_offer(c: &Company, o: &Offer, count: u32, price: Cents, extras: &[Extra], livery: &str) -> Contract {
    if o.is_new() {
        return draft_new(c, &o.listing, count.min(o.count), price, extras, livery, Some(o));
    }
    let t = terms(c.difficulty);
    let painting = if livery.is_empty() || extras.contains(&Extra::Painting) { 0 } else { painting_cost(c) };
    // (a demonstrator keeps what is left of its new warranty)
    let base = if o.kind == OfferKind::Demonstrator { t.warranty_new.saturating_sub((o.age_years(&c.date) * 12.0).round() as u32).max(t.warranty_used) } else { t.warranty_used };
    Contract {
        seller: o.seller.clone(),
        buyer: c.name.clone(),
        listing: o.listing.clone(),
        livery: livery.to_string(),
        count: count.clamp(1, o.count.max(1)),
        new: false,
        built: o.built.clone(),
        km: o.km,
        condition: o.condition,
        offer: Some(o.id.clone()),
        list: o.price,
        price,
        grant: 0,
        painting,
        pay: PayWay::Cash,
        delivery_days: 0,
        warranty_months: base + if extras.contains(&Extra::Warranty) { EXTRA_WARRANTY_MONTHS } else { 0 },
        extras: extras.iter().copied().filter(|e| *e != Extra::FastDelivery).collect(),
        ..Default::default()
    }
}

/// A signed contract waiting for its delivery.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Order {
    pub contract: Contract,
    pub delivery: String,
}

/// What `sign` did: the buses that joined the fleet now, or the order that brings them.
#[derive(Clone, Debug, PartialEq)]
pub enum Signed {
    Delivered(Vec<u32>),
    Ordered { no: u32, delivery: String },
}

/// Whether there is room at the depot for `n` more buses (with those ordered).
pub fn room_for(c: &Company, n: usize) -> Result<(), &'static str> {
    let held = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let pending: usize = c.dealer.orders.iter().map(|o| o.contract.count as usize).sum();
    let cap = c.site.spaces() + levels::extra_places(c) as usize + super::depot::OUTSIDE_MAX;
    if held + pending + n > cap {
        return Err("The depot has no room for so many buses: build more parking spaces.");
    }
    Ok(())
}

fn can_have(c: &Company, l: &Listing, new: bool) -> Result<(), &'static str> {
    market::kind_allowed(c, l.bus.kind)?;
    match l.availability(year_of(&c.date)) {
        Availability::NotYet => Err("This bus is not built yet."),
        Availability::UsedOnly if new => Err("This bus is no longer built: it is to be had second-hand only."),
        _ => Ok(()),
    }
}

/// Pay `amount` (with `security` the bank's for a loan).
fn pay(c: &mut Company, amount: Cents, how: Payment, what: &str, security: Cents) -> Result<(), &'static str> {
    match how {
        Payment::Cash if c.cash < amount => Err("Not enough cash."),
        Payment::Cash => Ok(()),
        Payment::Loan => finance::take_loan(c, amount, what, security).map(|_| ()),
    }
}

/// Sign a contract at `now`: it is paid (or the lease agreed) and booked, and its buses join
/// the fleet - at once when they are used or in stock with no time to wait, else on their
/// delivery day (`tick`).
pub fn sign(c: &mut Company, k: &Contract, listings: &[Listing], now: &str) -> Result<Signed, &'static str> {
    if !k.is_signed() {
        return Err("Sign the contract first.");
    }
    if k.count == 0 {
        return Err("Choose how many buses.");
    }
    can_have(c, &k.listing, k.new)?;
    room_for(c, k.count as usize)?;
    if let Some(id) = &k.offer {
        let Some(o) = offer(c, listings, id, now) else { return Err("This offer has ended or is sold.") };
        if o.count < k.count {
            return Err("Not that many buses are left of this offer.");
        }
    }
    let name = k.listing.bus.name.clone();
    match k.pay {
        PayWay::Cash => pay(c, k.due(), Payment::Cash, &name, k.total())?,
        PayWay::Loan => pay(c, k.due(), Payment::Loan, &name, k.total())?,
        PayWay::Lease => {
            if !k.new {
                return Err("Only new buses can be leased.");
            }
            let (monthly, _, _) = k.lease(c);
            // (the leasing company wants to see a month's rates in the bank)
            if c.cash < monthly * k.count as Cents {
                return Err("Not enough cash.");
            }
        }
    }
    let mut k = k.clone();
    c.dealer.counter += 1;
    k.no = c.dealer.counter;
    k.signed_at = now.to_string();
    if k.pay != PayWay::Lease {
        let what = if k.new { format!("{} × {} (new, contract {})", k.count, name, k.no) } else { format!("{} × {} (used, contract {})", k.count, name, k.no) };
        c.book(BookingKind::Purchase, -k.total(), what, false);
        c.book(BookingKind::Subsidy, k.grants(), name.clone(), false);
    }
    if let Some(id) = &k.offer {
        let day = id.get(1..).and_then(|s| s.split('-').next()).and_then(|d| d.parse().ok()).unwrap_or(0);
        c.dealer.taken.push(Taken { id: id.clone(), day, count: k.count });
    }
    c.dealer.bought += k.count;
    c.dealer.contracts.push(k.clone());
    if c.dealer.contracts.len() > CONTRACTS_KEPT {
        c.dealer.contracts.remove(0);
    }
    c.dealer.talks.retain(|t| !(t.quote.key == format!("new:{}", k.listing.bus.file) || k.offer.as_deref() == Some(t.quote.key.as_str())));
    if k.delivery_days <= 0 {
        return Ok(Signed::Delivered(deliver(c, &k)));
    }
    let delivery = k.delivery(now);
    let no = k.no;
    c.dealer.orders.push(Order { contract: k, delivery: delivery.clone() });
    Ok(Signed::Ordered { no, delivery })
}

/// Sign a purchase paid with a loan: the loan contract and the purchase are signed together,
/// or neither is (the purchase is kept as paid by the bank).
pub fn sign_financed(c: &mut Company, k: &Contract, loan: &super::finance::LoanContract, listings: &[Listing], now: &str) -> Result<(u32, Signed), &'static str> {
    super::finance::with_loan(c, loan, k.total(), |c| {
        let mut paid = k.clone();
        paid.pay = PayWay::Cash;
        let done = sign(c, &paid, listings, now)?;
        if let Some(last) = c.dealer.contracts.last_mut() {
            last.pay = PayWay::Loan;
        }
        Ok(done)
    })
}

/// How many signed contracts the company keeps.
pub const CONTRACTS_KEPT: usize = 60;

/// A contract's buses into the fleet, with their warranty and free service. Returns their ids.
fn deliver(c: &mut Company, k: &Contract) -> Vec<u32> {
    let r = economy::rules(c.difficulty);
    let new_value = economy::new_price(k.listing.bus.kind, &r, c.price_index);
    let built = if k.new { c.date.clone() } else { k.built.clone() };
    let mut ids = Vec::new();
    for _ in 0..k.count {
        let tenure = match k.pay {
            PayWay::Lease => {
                let (monthly, months, residual) = k.lease(c);
                Tenure::Leased { monthly, until: dates::add(&c.date, (months as f64 * 30.44).round() as i64), residual }
            }
            _ => Tenure::Owned { paid: k.price + k.painting - k.grant, new_value },
        };
        let id = market::add_vehicle(c, &k.listing.bus, built.clone(), k.km, k.condition, tenure, &k.livery);
        let until = dates::add(&c.date, (k.warranty_months as f64 * 30.44).round() as i64);
        c.dealer.warranties.push(Warranty { vehicle: id, until });
        if k.extras.contains(&Extra::FreeService) {
            c.dealer.free_services.push(id);
        }
        ids.push(id);
    }
    ids
}

/// A delivery `tick` made.
#[derive(Clone, Debug, PartialEq)]
pub struct Delivered {
    pub contract: u32,
    pub name: String,
    pub numbers: Vec<String>,
}

/// What the dealer does by `now` (the company clock calls it; the pages too until it does):
/// the orders due are delivered, and what is over is forgotten (offers sold days ago, talks
/// of other days, a sulk that ended, warranties that ran out).
pub fn tick(c: &mut Company, now: &str) -> Vec<Delivered> {
    let mut out = Vec::new();
    let due: Vec<Order> = c.dealer.orders.iter().filter(|o| not_after(&o.delivery, now)).cloned().collect();
    c.dealer.orders.retain(|o| !not_after(&o.delivery, now));
    for o in due {
        let ids = deliver(c, &o.contract);
        let numbers = ids.iter().filter_map(|id| c.vehicle(*id).map(|v| v.number.clone())).collect();
        out.push(Delivered { contract: o.contract.no, name: o.contract.listing.bus.name.clone(), numbers });
    }
    let today = dates::parse(&day_of(now)).unwrap_or(0);
    let date = day_of(now);
    c.dealer.taken.retain(|t| t.day >= today - 14);
    c.dealer.talks.retain(|t| t.day == date);
    c.dealer.breaks.retain(|b| !not_after(&b.1, now));
    c.dealer.warranties.retain(|w| dates::between(&date, &w.until) >= 0 && c.fleet.iter().any(|v| v.id == w.vehicle));
    c.dealer.free_services.retain(|id| c.fleet.iter().any(|v| v.id == *id));
    out
}

/// The quick buy: `count` new buses of a listing at the list price, paid now (cash or loan),
/// in the fleet at once. Returns their ids.
pub fn quick_buy(c: &mut Company, l: &Listing, count: u32, how: Payment, livery: &str) -> Result<Vec<u32>, &'static str> {
    let mut k = draft_new(c, l, count, list_price(c, l.bus.kind), &[], livery, None);
    k.delivery_days = 0;
    k.signed_by = c.name.clone();
    k.pay = if how == Payment::Loan { PayWay::Loan } else { PayWay::Cash };
    let now = now_of(c);
    match sign(c, &k, &[], &now)? {
        Signed::Delivered(ids) => Ok(ids),
        Signed::Ordered { .. } => Ok(Vec::new()),
    }
}

/// The quick buy of an offer at its price: `count` of it, in the fleet at once (new ones from
/// stock too).
pub fn quick_buy_offer(c: &mut Company, o: &Offer, count: u32, how: Payment, livery: &str, listings: &[Listing]) -> Result<Vec<u32>, &'static str> {
    let mut k = draft_offer(c, o, count, o.price, &[], livery);
    k.delivery_days = 0;
    k.signed_by = c.name.clone();
    k.pay = if how == Payment::Loan { PayWay::Loan } else { PayWay::Cash };
    let now = now_of(c);
    match sign(c, &k, listings, &now)? {
        Signed::Delivered(ids) => Ok(ids),
        Signed::Ordered { .. } => Ok(Vec::new()),
    }
}

/// A bus still under the dealer's warranty on `date`: its repairs are free.
pub fn under_warranty(c: &Company, vehicle: u32, date: &str) -> bool {
    c.dealer.warranties.iter().any(|w| w.vehicle == vehicle && dates::between(date, &w.until) >= 0)
}

/// The buses under warranty on `date`.
pub fn warranted(c: &Company, date: &str) -> Vec<u32> {
    c.dealer.warranties.iter().filter(|w| dates::between(date, &w.until) >= 0).map(|w| w.vehicle).collect()
}

/// A bus's first service is the dealer's: true once (and it is used up).
pub fn take_free_service(c: &mut Company, vehicle: u32) -> bool {
    let had = c.dealer.free_services.contains(&vehicle);
    c.dealer.free_services.retain(|v| *v != vehicle);
    had
}

// --- the test drive -----------------------------------------------------------------------------

/// A test drive under way: the game runs a free drive with this bus, which is not the
/// company's time (the company clock and the books leave it out).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct TestDrive {
    pub bus: String,
    pub name: String,
    pub livery: String,
    /// The offer it was for, if any.
    pub offer: Option<String>,
    pub started: String,
}

/// Mark a test drive as begun.
pub fn start_test_drive(c: &mut Company, bus: &str, name: &str, livery: &str, offer: Option<String>, now: &str) {
    c.dealer.test_drive = Some(TestDrive { bus: bus.to_string(), name: name.to_string(), livery: livery.to_string(), offer, started: now.to_string() });
}

/// The test drive is over (the player is back at the dealer).
pub fn end_test_drive(c: &mut Company) -> Option<TestDrive> {
    c.dealer.test_drive.take()
}

// --- what the company keeps -----------------------------------------------------------------------

/// How the dealer is shown first: the quick buy, or haggling and a contract.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BuyingMode {
    Simple,
    #[default]
    Advanced,
}

impl BuyingMode {
    pub const ALL: [BuyingMode; 2] = [BuyingMode::Simple, BuyingMode::Advanced];

    pub fn label(self) -> &'static str {
        match self {
            BuyingMode::Simple => "Simple",
            BuyingMode::Advanced => "Advanced",
        }
    }
}

/// An offer's buses bought.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Taken {
    pub id: String,
    pub day: i64,
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Warranty {
    pub vehicle: u32,
    pub until: String,
}

/// The company's dealings with the dealer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DealerState {
    #[serde(default)]
    pub mode: BuyingMode,
    #[serde(default)]
    pub taken: Vec<Taken>,
    #[serde(default)]
    pub orders: Vec<Order>,
    #[serde(default)]
    pub talks: Vec<Talk>,
    /// Dealers (by maker) that broke off, and until when.
    #[serde(default)]
    pub breaks: Vec<(String, String)>,
    /// Buses bought of the dealers (the fleet discount).
    #[serde(default)]
    pub bought: u32,
    #[serde(default)]
    pub warranties: Vec<Warranty>,
    #[serde(default)]
    pub free_services: Vec<u32>,
    /// The signed contracts (the last `CONTRACTS_KEPT`).
    #[serde(default)]
    pub contracts: Vec<Contract>,
    #[serde(default)]
    pub test_drive: Option<TestDrive>,
    /// The last contract number given.
    #[serde(default)]
    pub counter: u32,
    /// The loan contracts the company signed with the bank (the last sixty; see `finance`).
    #[serde(default)]
    pub loan_contracts: Vec<super::finance::LoanContract>,
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;

    fn company(d: Difficulty, date: &str) -> Company {
        let mut c = found(&Founding { name: "Stadtbus".into(), short: "SB".into(), difficulty: d, date: date.into(), ..Default::default() }, "Luc");
        c.progress.xp = levels::LEVEL_XP[9];
        c
    }

    fn listing(name: &str, maker: &str, size: BusSize, drive: Drive, years: Option<Years>) -> Listing {
        Listing {
            bus: MarketBus { file: format!("Vehicles/{name}/{name}.bus"), name: name.into(), kind: BusKind { size, drive }, paints: vec!["Red".into()], default_paint: "Red".into() },
            maker: maker.into(),
            model: name.into(),
            version: "3 doors".into(),
            years,
            seats: Some(32),
            standing: Some(60),
        }
    }

    #[test]
    fn moments_are_read_and_written() {
        assert_eq!(minutes_of("1970-01-02 01:30"), Some(1440 + 90));
        assert_eq!(minutes_of("1970-01-02"), Some(1440));
        assert_eq!(minutes_of("1970-01-02T00:05:30"), Some(1445));
        assert_eq!(minutes_of("2024-02-30 10:00"), None);
        assert_eq!(later("2024-02-28 23:30", 60), "2024-02-29 00:30");
        assert_eq!(at("2024-03-04", 8 * 60), "2024-03-04 08:00");
        assert!(not_after("2024-03-04 08:00", "2024-03-04 12:00") && !not_after("2024-03-05 08:00", "2024-03-04 12:00"));
    }

    #[test]
    fn the_years_of_a_bus_come_from_its_names_the_table_and_the_user() {
        let y = |names: &[&str]| years_of(names, "", &[], &[]);
        // the table of the well-known ones
        assert_eq!(y(&["MAN SD202", "Vehicles/MAN_SD2/SD202.bus"]), Some(Years::new(1984, Some(1992))));
        assert_eq!(y(&["MAN Lion's City", "Vehicles/MAN_LC/LC.bus"]), Some(Years::new(1996, Some(2019))));
        assert_eq!(y(&["Mercedes-Benz", "O530 Citaro Facelift"]), Some(Years::new(2005, Some(2012))));
        assert_eq!(y(&["MB O 405 N2"]), Some(Years::new(1989, Some(2001))));
        assert_eq!(y(&["Volvo 7900 Hybrid"]), Some(Years::new(2011, None)));
        // a range in the names wins over the table, a lone year counts after it
        assert_eq!(y(&["NL202 (1990-1992)"]), Some(Years::new(1990, Some(1992))));
        assert_eq!(y(&["Stadtbus 2012"]), Some(Years::new(2012, Some(2020))));
        // a description's year only after a word that says so; a mod's year is no bus's
        assert_eq!(years_of(&["Kleinbus"], "Version 1.2 (c) 2015 by X. Baujahr 1994.", &[], &[]), Some(Years::new(1994, Some(2002))));
        assert_eq!(years_of(&["Kleinbus"], "Version 1.2 (c) 2015 by X.", &[], &[]), None);
        // the emission standard as the last hint; "E5" in a sound's name is none
        assert_eq!(years_of(&["Kleinbus Euro V"], "", &[], &[]), Some(Years::new(2009, Some(2014))));
        assert_eq!(years_of(&["Kleinbus"], "", &["Sound_926LA_E5_z.cfg"], &[]), None);
        assert_eq!(years_of(&["Kleinbus"], "", &["euro6_engine.osc"], &[]), Some(Years::new(2014, None)));
        // a standard in the name: alone, or narrowing the table's years down
        assert_eq!(y(&["Citybus by Kajosoft", "628g LF e6 - 6AP1700"]), Some(Years::new(2014, None)));
        assert_eq!(y(&["Citybus by Kajosoft", "o530 U e3 1-2d"]), Some(Years::new(2001, Some(2006))));
        assert_eq!(y(&["MB O405 e2"]), Some(Years::new(1996, Some(2001))));
        // nothing known: always to be had
        assert_eq!(y(&["Bus", "Vehicles/Bus/bus.bus"]), None);
        // the user's file wins over all
        let o = parse_overrides("# a comment\nMAN SD202 = 1986-1990\nKleinbus = always\neCitaro = 2019-\nbroken line\nX = soon\n");
        assert_eq!(o.len(), 3);
        assert_eq!(years_of(&["MAN SD202"], "", &[], &o), Some(Years::new(1986, Some(1990))));
        assert_eq!(years_of(&["Kleinbus Euro V"], "", &[], &o), None);
        assert_eq!(years_of(&["Mercedes eCitaro"], "", &[], &o), Some(Years::new(2019, None)));
    }

    #[test]
    fn the_company_year_decides_new_used_or_nothing() {
        let sd = Some(Years::new(1984, Some(1992)));
        assert_eq!(availability(sd, 1990), Availability::New);
        assert_eq!(availability(sd, 2005), Availability::UsedOnly);
        assert_eq!(availability(sd, 1980), Availability::NotYet);
        assert_eq!(availability(None, 1950), Availability::New);
        // a company of 2005: the SD202 is second-hand only, the eCitaro not there, used SD202s
        // are of their years
        let c = company(Difficulty::Realistic, "2005-06-01");
        let ls = vec![listing("SD202", "MAN", BusSize::Double, Drive::Diesel, sd), listing("eCitaro", "Mercedes-Benz", BusSize::Solo, Drive::Electric, Some(Years::new(2018, None))), listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, Some(Years::new(1997, None)))];
        let f = Filter::default();
        assert!(f.fits(&ls[0], &c) && !f.fits(&ls[1], &c) && f.fits(&ls[2], &c));
        assert!(!Filter { new: Some(true), ..Default::default() }.fits(&ls[0], &c));
        assert!(Filter { new: Some(false), ..Default::default() }.fits(&ls[0], &c));
        assert!(Filter { search: "citaro".into(), size: Some(BusSize::Solo), ..Default::default() }.fits(&ls[2], &c));
        assert!(!Filter { max_price: 200_000_00, ..Default::default() }.fits(&ls[2], &c));
        assert!(!Filter { year: 1995, ..Default::default() }.fits(&ls[2], &c));
        let now = now_of(&c);
        let mut seen = 0;
        for d in 0..20 {
            let mut c = c.clone();
            c.date = dates::add(&c.date, d);
            for o in used_market(&c, &ls, &now_of(&c)) {
                assert_ne!(o.listing.bus.name, "eCitaro");
                if o.listing.bus.name == "SD202" {
                    let y = year_of(&o.built);
                    assert!((1984..=1992).contains(&y), "{y}");
                    assert!(o.km > 100_000.0 && o.price > 0);
                    seen += 1;
                }
            }
        }
        assert!(seen > 0);
        // a new one cannot be had
        let mut c2 = c.clone();
        assert_eq!(quick_buy(&mut c2, &ls[0], 1, Payment::Cash, ""), Err("This bus is no longer built: it is to be had second-hand only."));
        assert!(!used_market(&c, &ls, &now).is_empty());
    }

    #[test]
    fn the_offers_change_every_day_and_expire() {
        let c = company(Difficulty::Realistic, "2024-03-04");
        let ls = vec![listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None), listing("Lion's City", "MAN", BusSize::Solo, Drive::Diesel, None), listing("Urbino 18", "Solaris", BusSize::Articulated, Drive::Diesel, None)];
        let now = now_of(&c);
        let today = day_offers(&c, &ls, &now);
        assert!(!today.is_empty());
        // the same at the same moment, others the next day
        assert_eq!(day_offers(&c, &ls, &now), today);
        let mut next = c.clone();
        next.date = dates::add(&c.date, 1);
        let tomorrow = day_offers(&next, &ls, &now_of(&next));
        assert_ne!(tomorrow, today);
        // every offer open now, of a kind that fits
        let n = minutes_of(&now).unwrap();
        for o in today.iter().chain(used_market(&c, &ls, &now).iter()) {
            assert!(minutes_of(&o.published).unwrap() <= n && minutes_of(&o.expires).unwrap() > n);
            assert!(o.count >= 1 && o.price > 0);
            match o.kind {
                OfferKind::Discount => assert!(o.price < o.reference && o.km == 0.0),
                OfferKind::Demonstrator => assert!(o.price < o.reference && o.km < 40_000.0 && o.condition >= 93.0),
                OfferKind::Batch => assert!(o.count >= 3 && o.km > 100_000.0),
                OfferKind::Used => assert_eq!(o.seller, USED_CENTRE),
            }
        }
        // ten days later none of today's is open
        let mut later_c = c.clone();
        later_c.date = dates::add(&c.date, 10);
        let ids: Vec<String> = today.iter().map(|o| o.id.clone()).collect();
        assert!(day_offers(&later_c, &ls, &now_of(&later_c)).iter().all(|o| !ids.contains(&o.id)));
    }

    /// A talk, made the same way every time.
    fn haggle(c: &mut Company, q: &Quote, moves: &[Move]) -> (Talk, Vec<Reply>) {
        let now = now_of(c);
        let mut t = open_talk(c, q, &now).unwrap();
        let mut out = Vec::new();
        for m in moves {
            out.push(respond(c, &mut t, *m, &now));
            if t.closed {
                break;
            }
        }
        (t, out)
    }

    #[test]
    fn haggling_can_bring_the_price_down_but_not_below_the_floor() {
        let mut c = company(Difficulty::Realistic, "2024-03-04");
        let l = listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None);
        let q = Quote::new_bus(&c, &l, 1);
        assert_eq!(q.list, 280_000_00);
        // the room: a margin of some per cent, more for a fleet buyer and several buses
        let r1 = room(&c, &q);
        assert!((0.05..0.2).contains(&r1), "{r1}");
        c.dealer.bought = 10;
        assert!(room(&c, &Quote { count: 4, ..q.clone() }) > r1 + 0.05);
        assert!(room(&c, &Quote { kind: BusKind { size: BusSize::Solo, drive: Drive::Electric }, ..q.clone() }) < room(&c, &q));
        c.dealer.bought = 0;
        // asking for a discount a few times: lower, never under the floor
        let (t, replies) = haggle(&mut c, &q, &[Move::AskDiscount, Move::AskDiscount, Move::AskDiscount]);
        assert!(t.asking <= q.list && t.asking >= t.floor && t.floor < q.list);
        assert!(replies.iter().all(|r| matches!(r, Reply::Discount(_) | Reply::Firm | Reply::LastOffer(_))));
        // the talk goes on the same day where it stopped
        let again = open_talk(&c, &q, &now_of(&c)).unwrap();
        assert_eq!((again.rounds, again.asking), (t.rounds, t.asking));
        // an offer at the floor is taken or met: the price is never below it
        let mut c2 = company(Difficulty::Realistic, "2024-03-05");
        let q2 = Quote::new_bus(&c2, &l, 1);
        let floor = open_talk(&c2, &q2, &now_of(&c2)).unwrap().floor;
        let (t2, r2) = haggle(&mut c2, &q2, &[Move::Offer(floor), Move::Offer(floor), Move::Offer(floor), Move::Offer(floor), Move::Accept]);
        assert!(t2.agreed || t2.closed);
        assert!(t2.asking >= floor && t2.asking <= q2.list);
        assert!(r2.iter().any(|r| matches!(r, Reply::Accepted(_) | Reply::Counter(_) | Reply::LastOffer(_))));
        // the rounds run out: his last word
        let mut c3 = company(Difficulty::Hard, "2024-03-06");
        let q3 = Quote::new_bus(&c3, &l, 1);
        let (t3, r3) = haggle(&mut c3, &q3, &[Move::AskDiscount; 10]);
        assert!(t3.closed && t3.rounds <= t3.max_rounds);
        assert!(matches!(r3.last(), Some(Reply::LastOffer(_)) | Some(Reply::BrokeOff(_))));
        // his last word can still be taken (not once he broke off)
        let mut t3 = t3;
        let now = now_of(&c3);
        let taken = respond(&mut c3, &mut t3, Move::Accept, &now);
        match r3.last() {
            Some(Reply::LastOffer(p)) => assert!(taken == Reply::Accepted(*p) && t3.agreed),
            _ => assert!(!t3.agreed),
        }
    }

    #[test]
    fn pushing_too_hard_makes_the_dealer_break_off() {
        let mut c = company(Difficulty::Hard, "2024-03-04");
        let l = listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None);
        let q = Quote::new_bus(&c, &l, 1);
        let (t, replies) = haggle(&mut c, &q, &[Move::Offer(100_000_00), Move::Offer(100_000_00), Move::Offer(100_000_00)]);
        assert!(t.closed && !t.agreed);
        let Some(Reply::BrokeOff(until)) = replies.last() else { panic!("{replies:?}") };
        assert_eq!(day_of(until), "2024-03-08");
        // no talks with that maker's dealer until then; the quick buy still sells at list
        assert!(open_talk(&c, &q, &now_of(&c)).is_err());
        assert!(sulking(&c, "mercedes-benz", &now_of(&c)).is_some());
        let other = listing("Lion's City", "MAN", BusSize::Solo, Drive::Diesel, None);
        assert!(open_talk(&c, &Quote::new_bus(&c, &other, 1), &now_of(&c)).is_ok());
        assert!(quick_buy(&mut c, &l, 1, Payment::Cash, "").is_ok());
        c.date = "2024-03-09".into();
        let now = now_of(&c);
        tick(&mut c, &now);
        assert!(open_talk(&c, &q, &now_of(&c)).is_ok() && c.dealer.breaks.is_empty());
    }

    #[test]
    fn extras_are_granted_from_what_the_dealer_can_give() {
        let mut c = company(Difficulty::Easy, "2024-03-04");
        let l = listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None);
        let q = Quote::new_bus(&c, &l, 2);
        let now = now_of(&c);
        let mut t = open_talk(&c, &q, &now).unwrap();
        let floor = t.floor;
        let mut granted = 0;
        for e in Extra::ALL {
            if respond(&mut c, &mut t, Move::AskExtra(e), &now) == Reply::ExtraGranted(e) {
                granted += 1;
            }
        }
        assert!(granted >= 1, "{:?}", t.replies);
        assert_eq!(t.extras.len(), granted);
        assert!(t.floor > floor && t.floor <= t.asking);
        // they go into the contract: painting free, the warranty longer, delivery faster
        let k = draft_new(&c, &l, 2, t.asking, &[Extra::Painting, Extra::Warranty, Extra::FastDelivery], "Blue", None);
        let plain = draft_new(&c, &l, 2, t.asking, &[], "Blue", None);
        assert_eq!(k.painting, 0);
        assert_eq!(plain.painting, painting_cost(&c));
        assert_eq!(k.warranty_months, plain.warranty_months + EXTRA_WARRANTY_MONTHS);
        assert!(k.delivery_days < plain.delivery_days);
    }

    #[test]
    fn a_signed_contract_is_paid_and_its_buses_come_on_their_day() {
        let mut c = company(Difficulty::Realistic, "2024-03-04");
        let l = listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None);
        let mut k = draft_new(&c, &l, 2, 260_000_00, &[Extra::FreeService], "", None);
        // unsigned: nothing
        let now0 = now_of(&c);
        assert_eq!(sign(&mut c, &k, &[], &now0), Err("Sign the contract first."));
        k.signed_by = "Luc Ruigrok".into();
        let cash = c.cash;
        let now = now_of(&c);
        let Ok(Signed::Ordered { no, delivery }) = sign(&mut c, &k, &[], &now) else { panic!() };
        assert_eq!(no, 1);
        assert_eq!(c.cash, cash - 520_000_00);
        assert!(c.fleet.is_empty() && c.dealer.orders.len() == 1);
        assert_eq!(c.dealer.bought, 2);
        assert_eq!(c.dealer.contracts[0].signed_by, "Luc Ruigrok");
        // not yet the day before; on its morning it comes
        let day = day_of(&delivery);
        assert!(tick(&mut c, &later(&delivery, -60)).is_empty());
        let d = tick(&mut c, &at(&day, 9 * 60));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].numbers, vec!["101".to_string(), "102".to_string()]);
        assert!(c.dealer.orders.is_empty() && c.fleet.len() == 2);
        let id = c.fleet[0].id;
        assert!(under_warranty(&c, id, &day) && !under_warranty(&c, id, &dates::add(&day, 800)));
        assert!(take_free_service(&mut c, id) && !take_free_service(&mut c, id));
        // a lease: no price now, a rate a month
        let mut lk = draft_new(&c, &l, 1, 270_000_00, &[], "", None);
        lk.pay = PayWay::Lease;
        lk.delivery_days = 0;
        lk.strokes = vec![vec![[0.1, 0.5], [0.4, 0.4], [0.8, 0.6]]];
        let cash = c.cash;
        let Ok(Signed::Delivered(ids)) = sign(&mut c, &lk, &[], &now) else { panic!() };
        assert_eq!(c.cash, cash);
        assert!(matches!(c.vehicle(ids[0]).unwrap().tenure, Tenure::Leased { .. }));
        // a purchase on a loan: both signed, or neither
        let mut bk = draft_new(&c, &l, 1, 270_000_00, &[], "", None);
        bk.signed_by = "Luc".into();
        bk.pay = PayWay::Loan;
        bk.delivery_days = 0;
        let loan = super::super::finance::LoanContract { signed_by: "Luc".into(), ..super::super::finance::draft_loan(&c, bk.due(), 48, "Citaro", bk.total()) };
        let (debt, cash) = (c.debt(), c.cash);
        let (_, done) = sign_financed(&mut c, &bk, &loan, &[], &now).unwrap();
        assert!(matches!(done, Signed::Delivered(_)));
        assert_eq!((c.debt(), c.cash), (debt + bk.due(), cash));
        assert_eq!(c.dealer.contracts.last().unwrap().pay, PayWay::Loan);
        // (an articulated bus a company of the first level may not buy: no loan either)
        let mut low = c.clone();
        low.progress.xp = 0;
        let before = low.clone();
        let big = listing("Citaro G", "Mercedes-Benz", BusSize::Articulated, Drive::Diesel, None);
        let mut gk = draft_new(&low, &big, 1, 400_000_00, &[], "", None);
        gk.signed_by = "Luc".into();
        gk.pay = PayWay::Loan;
        let loan = super::super::finance::LoanContract { signed_by: "Luc".into(), ..super::super::finance::draft_loan(&low, 10_000_00, 12, "x", 0) };
        assert!(sign_financed(&mut low, &gk, &loan, &[], &now).is_err());
        assert_eq!(low, before);
    }


    #[test]
    fn an_offer_is_bought_once_and_used_ones_come_at_once() {
        let mut c = company(Difficulty::Realistic, "2024-03-04");
        let ls = vec![listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None)];
        let now = now_of(&c);
        let used = used_market(&c, &ls, &now);
        let o = used[0].clone();
        let mut k = draft_offer(&c, &o, 1, o.price, &[], "");
        k.signed_by = "Luc".into();
        let Ok(Signed::Delivered(ids)) = sign(&mut c, &k, &ls, &now) else { panic!() };
        let v = c.vehicle(ids[0]).unwrap();
        assert_eq!((v.built.as_str(), v.km), (o.built.as_str(), o.km));
        assert!(used_market(&c, &ls, &now).iter().all(|x| x.id != o.id));
        assert_eq!(sign(&mut c, &k, &ls, &now), Err("This offer has ended or is sold."));
        // a used bus cannot be leased
        let o2 = used_market(&c, &ls, &now)[0].clone();
        let mut k2 = draft_offer(&c, &o2, 1, o2.price, &[], "");
        k2.signed_by = "Luc".into();
        k2.pay = PayWay::Lease;
        assert_eq!(sign(&mut c, &k2, &ls, &now), Err("Only new buses can be leased."));
        // the quick buy of an offer and of a model: at once
        let ids = quick_buy_offer(&mut c, &o2, 1, Payment::Cash, "", &ls).unwrap();
        assert_eq!(ids.len(), 1);
        c.cash = 5_000_000_00;
        let before = c.fleet.len();
        let cash = c.cash;
        let ids = quick_buy(&mut c, &ls[0], 3, Payment::Cash, "Red").unwrap();
        assert_eq!(c.fleet.len(), before + 3);
        assert_eq!(cash - c.cash, 3 * (280_000_00 + painting_cost(&c)));
        assert_eq!(c.vehicle(ids[2]).unwrap().livery, "Red");
    }

    #[test]
    fn there_is_no_buying_beyond_the_depot() {
        let mut c = company(Difficulty::Easy, "2024-03-04");
        let l = listing("Citaro", "Mercedes-Benz", BusSize::Solo, Drive::Diesel, None);
        let cap = c.site.spaces() + levels::extra_places(&c) as usize + super::super::depot::OUTSIDE_MAX;
        assert!(room_for(&c, cap).is_ok() && room_for(&c, cap + 1).is_err());
        let mut k = draft_new(&c, &l, 2, 250_000_00, &[], "", None);
        k.signed_by = "Luc".into();
        let now = now_of(&c);
        sign(&mut c, &k, &[], &now).unwrap();
        // (the ones ordered count)
        assert!(room_for(&c, cap - 2).is_ok() && room_for(&c, cap - 1).is_err());
    }

    #[test]
    fn a_test_drive_is_marked_and_ended() {
        let mut c = company(Difficulty::Realistic, "2024-03-04");
        let now = now_of(&c);
        start_test_drive(&mut c, "Vehicles/Citaro/Citaro.bus", "Citaro", "Red", None, &now);
        assert!(c.dealer.test_drive.is_some());
        assert_eq!(end_test_drive(&mut c).map(|t| t.name), Some("Citaro".to_string()));
        assert!(c.dealer.test_drive.is_none());
    }
}

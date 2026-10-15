//! What the Casual pages say and offer, worked out from the company alone - the game-drawn
//! launcher's `company/casual/logic.rs` as it is (its texts are English keys translated where
//! they are shown).

// (the game-drawn launcher's as it is: what this page does not offer yet stays, unused)
#![allow(dead_code)]

#[cfg(test)]
use super::super::HOME_TAB;
use super::{BUSES, DRIVERS, LINES, MONEY};
#[cfg(test)]
use super::{COMPANY, NEW_LINE, SHOP};
#[cfg(test)]
use super::SHIFTS;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan, Who};
use omsi_launcher_lib::company::{self as co, BookingKind, BusSize, Cents, Company, Employee};
use omsi_launcher_lib::lines::{DayPattern, TimeBand, DAY_GROUPS};

// --- what a click does ------------------------------------------------------------------------

/// What a button of the Casual pages does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Act {
    /// Open a Casual page (or the home).
    Open(usize),
    /// The simple shop.
    BuyBus,
    /// The line wizard.
    AddLine,
    /// Hire the drivers the shifts ask for now (the company does it at night by itself too).
    HireNow,
    /// A line's fare, on the lines page.
    Fare(String),
    /// The company page, to buy a bigger depot.
    BiggerDepot,
    /// The rest of the day simulated, to the next morning.
    NextDay,
    /// Switch the company to Normal (asked first).
    ToNormal,
    /// A message of the inbox seen (marked read).
    Seen(u64),
}

impl Act {
    /// The Casual page a click lands on (None: it stays where it is, or asks first).
    #[cfg(test)]
    pub fn lands(&self) -> Option<usize> {
        match self {
            Act::Open(tab) => Some(*tab),
            Act::BuyBus => Some(SHOP),
            Act::AddLine => Some(NEW_LINE),
            Act::HireNow => Some(DRIVERS),
            Act::Fare(_) => Some(LINES),
            Act::BiggerDepot => Some(COMPANY),
            Act::NextDay | Act::ToNormal | Act::Seen(_) => None,
        }
    }
}

/// A text to show: its English key and what fills its places.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Words {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
}

impl Words {
    pub fn new(key: &'static str) -> Words {
        Words { key, args: Vec::new() }
    }

    pub fn with(mut self, k: &'static str, v: impl ToString) -> Words {
        self.args.push((k, v.to_string()));
        self
    }

    /// Translated, its places filled.
    pub fn text(&self) -> String {
        let mut t = super::super::words::tr(self.key);
        for (k, v) in &self.args {
            t = t.replace(&format!("%{{{k}}}"), v);
        }
        t
    }
}

// --- the day -----------------------------------------------------------------------------------

/// Minutes since the company's day began.
pub(crate) fn now_minute(c: &Company) -> i32 {
    (co::clock::now(c) - co::clock::moment(&c.date, 0)).clamp(0, 48 * 60) as i32
}

/// The buses the company has today (held, whether in the workshop or not).
pub(crate) fn buses(c: &Company) -> usize {
    c.fleet.iter().filter(|v| v.held_on(&c.date)).count()
}

/// The drivers the company employs today.
pub(crate) fn drivers(c: &Company) -> Vec<&Employee> {
    c.staff.iter().filter(|e| co::staff::drives(e) && e.employed_on(&c.date)).collect()
}

/// The tours of today's plan that run (their lines in service).
fn running(plan: &Plan) -> impl Iterator<Item = &co::day::TourPlan> {
    plan.tours.iter().filter(|t| !t.tour.unplanned)
}

/// The buses on the road at minute `now`, of today's plan.
pub(crate) fn on_the_road(plan: &Plan, now: i32) -> usize {
    let mut ids: Vec<u32> = running(plan).filter(|t| t.tour.from() <= now && now < t.tour.to()).filter_map(|t| t.bus).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.len()
}

/// How the day goes, in one sentence.
pub(crate) fn day_sentence(c: &Company, plan: Option<&Plan>, now: i32) -> Words {
    let n = buses(c);
    if c.lines.is_empty() {
        return Words::new("Your company runs no line yet. Add one to get going.");
    }
    if n == 0 {
        return Words::new("Your lines are waiting for buses. Buy your first bus.");
    }
    let Some(plan) = plan else { return Words::new("The company is getting the day ready…") };
    let tours: Vec<&co::day::TourPlan> = running(plan).collect();
    if tours.is_empty() {
        return Words::new("No bus runs on your lines today.");
    }
    let road = on_the_road(plan, now);
    let first = tours.iter().map(|t| t.tour.from()).min().unwrap_or(0);
    let last = tours.iter().map(|t| t.tour.to()).max().unwrap_or(0);
    if road >= n && n == 1 {
        Words::new("Your bus is on the road.")
    } else if road >= n {
        Words::new("All %{n} buses are on the road.").with("n", n)
    } else if road == 1 {
        Words::new("1 of %{n} buses is on the road.").with("n", n)
    } else if road > 0 {
        Words::new("%{k} of %{n} buses are on the road.").with("k", road).with("n", n)
    } else if now < first {
        Words::new("The buses are still in the depot: the first leaves at %{time}.").with("time", co::clock::hhmm(first as i64))
    } else if now >= last {
        Words::new("The day's work is done: every bus is back in the depot.")
    } else {
        Words::new("A quiet moment: no bus is on the road right now.")
    }
}

/// The deeds' words as a Casual player reads them (the roster's jargon left out).
fn casual_deed(text: &str) -> &str {
    match text {
        co::autopilot::ROSTER => "Today's work is handed out. Shifts: %{d}, buses: %{b}.",
        co::autopilot::LINE_STARTED => "Line %{n} runs from now on.",
        co::autopilot::RENEWED => "Line %{n} stays yours: its licence is renewed for %{amount}, until %{date}.",
        co::autopilot::NOT_RENEWED => "Line %{n} could not be kept: its licence costs %{amount}, more than the cash.",
        t => t,
    }
}

/// The deeds' Casual words (for the translations' check).
#[cfg(test)]
pub(crate) fn deed_texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = co::autopilot::texts().into_iter().map(casual_deed).collect();
    v.extend([SPARE, BROKE]);
    v
}

const SPARE: &str = "Bus %{bus} broke down: spare bus %{spare} took over.";
const BROKE: &str = "Bus %{bus} broke down: the workshop repairs it.";

/// What the company solved by itself lately, the newest first: a breakdown of today and the
/// spare bus that took over, then the deeds of today and of last night - (English key, its
/// places as the inbox fills them).
pub(crate) fn solved(c: &Company) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let feed: Vec<&co::clock::FeedItem> = co::clock::feed_of_day(c).collect();
    let arg = |f: &co::clock::FeedItem, k: &str| f.args.iter().find(|a| a.0 == k).map(|a| a.1.clone()).unwrap_or_default();
    for f in feed.iter().rev() {
        if f.text == "Bus %{bus} broke down on line %{n}" || f.text == "Bus %{bus} broke down in the game" {
            let bus = arg(f, "bus");
            let spare = feed.iter().find(|g| g.text == "Spare bus %{spare} takes over the trips of bus %{bus} on line %{n} from %{time}" && arg(g, "bus") == bus).map(|g| arg(g, "spare"));
            match spare {
                Some(s) => out.push((SPARE.to_string(), vec![("bus".into(), bus), ("spare".into(), s)])),
                None => out.push((BROKE.to_string(), vec![("bus".into(), bus)])),
            }
        }
    }
    let yesterday = co::dates::add(&c.date, -1);
    for d in c.autopilot.log.iter().rev().filter(|d| d.date == c.date || d.date == yesterday) {
        out.push((casual_deed(&d.text).to_string(), d.args.clone()));
    }
    out
}

// --- the tiles ---------------------------------------------------------------------------------

/// A tile of the home: its page, name, icon, big number (and its colour's sense: None calm,
/// Some(true) good, Some(false) bad), the line under it and its one action.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tile {
    pub page: usize,
    pub name: &'static str,
    pub icon: &'static str,
    pub big: String,
    pub good: Option<bool>,
    pub under: Words,
    pub action: (&'static str, &'static str, Act),
}

/// This week's result: the last seven days closed.
pub(crate) fn week_result(c: &Company) -> Option<Cents> {
    let n = c.history.len();
    if n == 0 {
        return None;
    }
    Some(c.history[n.saturating_sub(7)..].iter().map(|d| d.result).sum())
}

/// The home's four tiles: my buses, my lines, my drivers, the money.
pub(crate) fn tiles(c: &Company, plan: Option<&Plan>, now: i32) -> Vec<Tile> {
    let n = buses(c);
    let road = plan.map(|p| on_the_road(p, now)).unwrap_or(0);
    let shop = c.fleet.iter().filter(|v| v.held_on(&c.date) && v.in_workshop(&c.date)).count();
    let bus_under = if n == 0 {
        Words::new("No bus yet")
    } else if shop > 0 {
        Words::new("%{n} in the workshop").with("n", shop)
    } else {
        Words::new("%{n} on the road now").with("n", road)
    };
    let lines_under = if c.lines.is_empty() {
        Words::new("No line yet")
    } else {
        let pax: u32 = c.lines.iter().map(|l| week_passengers(c, &l.name)).sum();
        Words::new("%{n} passengers this week").with("n", super::super::words::grouped(pax as f64))
    };
    let ds = drivers(c);
    let (_, short) = plan.map(|p| p.short_of()).unwrap_or((0, 0));
    let drivers_under = if ds.is_empty() {
        Words::new("Hired for you at night")
    } else if short > 0 {
        Words::new(if short == 1 { "1 shift today without a driver" } else { "%{n} shifts today without a driver" }).with("n", short)
    } else {
        Words::new("Every shift has its driver")
    };
    let week = week_result(c);
    let (big, good, money_under) = match week {
        Some(r) => (super::super::words::eur(r), Some(r >= 0), Words::new("This week's result")),
        None => (super::super::sandbox::cash(c), None, Words::new("In the bank: the first week comes")),
    };
    vec![
        Tile { page: BUSES, name: "My buses", icon: "directions_bus", big: n.to_string(), good: None, under: bus_under, action: ("Buy a bus", "add", Act::BuyBus) },
        Tile { page: LINES, name: "My lines", icon: "route", big: c.lines.len().to_string(), good: None, under: lines_under, action: ("Add a line", "alt_route", Act::AddLine) },
        Tile { page: DRIVERS, name: "My drivers", icon: "groups", big: ds.len().to_string(), good: (short > 0).then_some(false), under: drivers_under, action: ("Hire automatically", "person", Act::HireNow) },
        Tile { page: MONEY, name: "Money", icon: "payments", big, good, under: money_under, action: ("See where it goes", "trending_up", Act::Open(MONEY)) },
    ]
}

// --- the tips ------------------------------------------------------------------------------------

/// A tip of the home: plain words and the button that does it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tip {
    /// What it is about (the same tip is not shown twice; put away for the session by it).
    pub id: String,
    pub words: Words,
    /// Its button: words, icon, what it does.
    pub button: (&'static str, &'static str, Act),
    /// A warning (amber) or a calm idea.
    pub warn: bool,
}

/// How many tips the home shows at once.
pub(crate) const TIPS: usize = 3;

/// The buses no tour of today's plan uses, of those that could run.
pub(crate) fn idle_buses(c: &Company, plan: &Plan) -> usize {
    let used: Vec<u32> = running(plan).filter_map(|t| t.bus).collect();
    c.fleet.iter().filter(|v| v.held_on(&c.date) && !v.in_workshop(&c.date) && !used.contains(&v.id)).count()
}

/// The fare a Casual player is advised: a tenth above the fare association's single ticket -
/// more money, few passengers lost and no reputation - within the line's band.
pub(crate) fn advised_fare(c: &Company, l: &co::CompanyLine) -> Cents {
    let (lo, hi) = co::fares::band(c, l);
    let p0 = co::fares::association_fare(c) as f64;
    let best = co::fares::best_fare(c, l) as f64;
    let x = (p0 * 1.1).min(best);
    (((x / 10.0).round() as Cents) * 10).clamp(lo, hi)
}

/// The passengers of a line on the last seven days closed.
pub(crate) fn week_passengers(c: &Company, line: &str) -> u32 {
    let n = c.history.len();
    c.history[n.saturating_sub(7)..].iter().flat_map(|d| d.lines.iter()).filter(|t| t.line.eq_ignore_ascii_case(line)).map(|t| t.passengers).sum()
}

/// A line's crowded trips on the last seven days: (all, before ten in the morning).
fn week_crowded(c: &Company, line: &str) -> (u32, u32) {
    let n = c.history.len();
    c.history[n.saturating_sub(7)..].iter().flat_map(|d| d.lines.iter()).filter(|t| t.line.eq_ignore_ascii_case(line)).fold((0, 0), |a, t| (a.0 + t.crowded, a.1 + t.crowded_morning))
}

/// The tips worth giving now, the most pressing first (`TIPS` of them are shown): what is
/// missing, what is full, what is too much, what costs passengers - and the inbox's important
/// messages not read yet.
pub(crate) fn tips(c: &Company, plan: Option<&Plan>) -> Vec<Tip> {
    let mut out = Vec::new();
    let tip = |id: String, words: Words, button: (&'static str, &'static str, Act), warn: bool| Tip { id, words, button, warn };
    let n = buses(c);
    if c.lines.is_empty() {
        out.push(tip("first-line".into(), Words::new("Start with a line: pick one of the map's, or draw your own stops."), ("Add a line", "alt_route", Act::AddLine), false));
    } else if n == 0 {
        out.push(tip("first-bus".into(), Words::new("Your lines need buses. Buy your first one: it runs at once."), ("Buy a bus", "add", Act::BuyBus), true));
    }
    if let Some(p) = plan {
        let (no_bus, no_driver) = p.short_of();
        if n > 0 && no_bus > 0 {
            out.push(tip("short-buses".into(), Words::new(if no_bus == 1 { "1 of today's bus runs has no bus. Buy another bus." } else { "%{n} of today's bus runs have no bus. Buy another bus." }).with("n", no_bus), ("Buy a bus", "add", Act::BuyBus), true));
        }
        if no_driver > 0 {
            out.push(tip("short-drivers".into(), Words::new(if no_driver == 1 { "1 shift today has no driver. The company hires tonight, or now." } else { "%{n} shifts today have no driver. The company hires tonight, or now." }).with("n", no_driver), ("Hire now", "person", Act::HireNow), true));
        }
    }
    // the depot full
    let places = co::depot::places(c);
    if n > 0 && n >= places {
        out.push(tip("depot-full".into(), Words::new("Your depot is full: no room for another bus. Buy a bigger depot."), ("Bigger depot", "garage", Act::BiggerDepot), true));
    }
    // a line often full
    for l in &c.lines {
        let (all, morning) = week_crowded(c, &l.name);
        if all >= 3 {
            let w = if morning * 2 >= all { "Line %{n} is often full in the morning. Add a bus." } else { "Line %{n} is often full. Add a bus." };
            out.push(tip(format!("full-{}", l.name), Words::new(w).with("n", &l.number), ("Buy a bus", "add", Act::BuyBus), true));
        }
    }
    // a fare too high or low
    for l in &c.lines {
        let fare = co::fares::fare_of(c, l);
        let advised = advised_fare(c, l);
        if fare > advised + 5 {
            out.push(tip(format!("fare-{}", l.name), Words::new("Your fare on line %{n} is high: fewer people ride.").with("n", &l.number), ("Set the fare", "confirmation_number", Act::Fare(l.name.clone())), false));
        } else if (fare as f64) < co::fares::association_fare(c) as f64 * 0.9 {
            out.push(tip(format!("fare-{}", l.name), Words::new("Your fare on line %{n} is low: it could earn more.").with("n", &l.number), ("Set the fare", "confirmation_number", Act::Fare(l.name.clone())), false));
        }
    }
    // more buses than needed
    if let Some(p) = plan {
        let idle = idle_buses(c, p);
        if running(p).next().is_some() && idle >= 2 && idle * 4 >= n {
            out.push(tip("idle".into(), Words::new("You have more buses than you need: %{n} stand still all day. Sell one, or add a line.").with("n", idle), ("See my buses", "directions_bus", Act::Open(BUSES)), false));
        }
    }
    // money tight
    if !co::sandbox::unlimited(c) && c.cash < co::autopilot::cushion(c) && !c.lines.is_empty() {
        out.push(tip("cash".into(), Words::new("Money is getting tight. See where it goes."), ("See the money", "payments", Act::Open(MONEY)), true));
    }
    // the inbox's important messages
    use co::clock::Level;
    for m in c.inbox.messages.iter().rev().filter(|m| !m.read && matches!(m.level, Level::Warn | Level::Bad)).take(2) {
        out.push(Tip { id: format!("msg-{}", m.id), words: Words { key: "%{text}", args: vec![("text", super::super::inbox::words(&m.title, &m.args))] }, button: ("Got it", "check", Act::Seen(m.id)), warn: true });
    }
    out
}

/// Whether the calm hint to switch to Normal is due: three weeks of the company's days closed,
/// and not put away.
pub(crate) fn hint_due(c: &Company) -> bool {
    c.game_mode.casual() && !c.autopilot.normal_hint_off && c.history.len() >= 21
}

// --- the shifts ------------------------------------------------------------------------------------

/// How many of today's shifts the home offers.
pub(crate) const SHIFT_CARDS: usize = 4;

/// A shift of today as a card: where it is in the plan, its line and times, its bus, who drives
/// it now.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShiftCard {
    pub tour: usize,
    pub duty: usize,
    pub line: String,
    pub number: String,
    pub tour_no: String,
    pub from: i32,
    pub to: i32,
    pub bus: Option<BusOf>,
    pub who: Option<Who>,
    pub trips: usize,
}

impl ShiftCard {
    pub fn mine(&self) -> bool {
        self.who == Some(Who::Player)
    }

    pub fn open(&self) -> bool {
        self.who.is_none()
    }
}

/// Today's shifts to drive (`now`: minutes of the company's day), at most `SHIFT_CARDS`: the
/// player's own first, then those without a driver, then the others - each group by the time it
/// begins. A shift begun already is left out (one of the player's own until it is over), and
/// one of a tour he drove.
pub(crate) fn shift_cards(p: &DayPlan, now: i32) -> Vec<ShiftCard> {
    let mut out = Vec::new();
    for (ti, t) in p.tours.iter().enumerate() {
        if t.tour.unplanned || t.by_player {
            continue;
        }
        for (k, d) in t.duties.iter().enumerate() {
            let mine = d.who == Some(Who::Player);
            if (mine && d.to <= now) || (!mine && d.from <= now) {
                continue;
            }
            let trips = t.tour.trips[d.start..d.end.min(t.tour.trips.len())].iter().filter(|x| x.counts()).count();
            out.push(ShiftCard { tour: ti, duty: k, line: t.tour.line.clone(), number: t.tour.number.clone(), tour_no: t.tour.tour.clone(), from: d.from, to: d.to, bus: t.bus, who: d.who, trips });
        }
    }
    out.sort_by_key(|s| (if s.mine() { 0 } else if s.open() { 1 } else { 2 }, s.from));
    out.truncate(SHIFT_CARDS);
    out
}

// --- the line wizard -----------------------------------------------------------------------------

/// How often a new line runs (the wizard's second step).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Freq {
    Quiet,
    #[default]
    Normal,
    Busy,
}

impl Freq {
    pub const ALL: [Freq; 3] = [Freq::Quiet, Freq::Normal, Freq::Busy];

    pub fn label(self) -> &'static str {
        match self {
            Freq::Quiet => "Quiet",
            Freq::Normal => "Normal",
            Freq::Busy => "Busy",
        }
    }

    /// In a few words, for its card.
    pub fn words(self) -> &'static str {
        match self {
            Freq::Quiet => "Every half hour in the rush hours, every hour otherwise. Few buses, low costs.",
            Freq::Normal => "Every 15 minutes in the rush hours, every 20 to 30 otherwise.",
            Freq::Busy => "Every 10 minutes in the rush hours, every 15 otherwise. Many passengers, many buses.",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Freq::Quiet => "nights_stay",
            Freq::Normal => "schedule",
            Freq::Busy => "bolt",
        }
    }

    /// What a map line's timetable comes to: its headway in the daytime.
    pub fn of_headway(minutes: f64) -> Freq {
        // (as the pages say it: to five minutes)
        let minutes = (minutes / 5.0).round().max(1.0) * 5.0;
        if minutes <= 15.0 {
            Freq::Busy
        } else if minutes <= 30.0 {
            Freq::Normal
        } else {
            Freq::Quiet
        }
    }
}

/// The timetable a new line of the player's own starts with, by how often it runs: the working
/// days with their rush hours, Saturdays and Sundays quieter (the line editor's `DayPattern`s,
/// one per day group, in time bands).
pub(crate) fn freq_days(f: Freq) -> Vec<DayPattern> {
    let b = |from: f32, to: f32, headway: f32| TimeBand { from: from * 60.0, to: to * 60.0, headway, size: None };
    let (wd, sat, sun) = match f {
        Freq::Quiet => (vec![b(6.0, 9.0, 30.0), b(9.0, 15.0, 60.0), b(15.0, 18.0, 30.0), b(18.0, 22.0, 60.0)], vec![b(8.0, 20.0, 60.0)], vec![b(9.0, 19.0, 60.0)]),
        Freq::Normal => (vec![b(5.0, 6.0, 30.0), b(6.0, 9.0, 15.0), b(9.0, 15.0, 20.0), b(15.0, 18.0, 15.0), b(18.0, 21.0, 30.0), b(21.0, 24.0, 60.0)], vec![b(6.0, 9.0, 30.0), b(9.0, 18.0, 30.0), b(18.0, 23.0, 60.0)], vec![b(8.0, 20.0, 60.0)]),
        Freq::Busy => (vec![b(5.0, 6.0, 20.0), b(6.0, 9.0, 10.0), b(9.0, 15.0, 15.0), b(15.0, 18.0, 10.0), b(18.0, 21.0, 15.0), b(21.0, 24.0, 30.0)], vec![b(6.0, 9.0, 20.0), b(9.0, 18.0, 15.0), b(18.0, 24.0, 30.0)], vec![b(8.0, 20.0, 30.0), b(20.0, 24.0, 60.0)]),
    };
    [wd, sat, sun].into_iter().enumerate().map(|(k, bands)| DayPattern { on: true, days: DAY_GROUPS[k].1, first: bands[0].from, last: bands.last().map(|x| x.to).unwrap_or(0.0), headway: bands[0].headway, layover: 5.0, bands }).collect()
}

/// A map line's daytime headway in minutes (its trips leaving between 06:00 and 20:00, both
/// ways counted as one).
pub(crate) fn map_headway(l: &core::LineInfo) -> Option<f64> {
    let deps = l.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()).filter(|t| !t.line.trim().is_empty() && (6.0 * 3600.0..20.0 * 3600.0).contains(&t.departure)).count();
    (deps >= 2).then(|| 14.0 * 60.0 / (deps as f64 / 2.0))
}

/// The wizard's first step: one of the map's lines, or stops of the player's own.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pick {
    Map(String),
    Draw,
}

/// What the wizard's last step comes to: how the line is made, how often it runs and what it
/// asks of the company (made by itself: timetable, shifts, drivers).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outcome {
    /// The line's number (empty for a line still to be drawn).
    pub number: String,
    pub freq: Freq,
    /// A map line keeps the map's own timetable (its headway); a drawn one gets `freq_days`.
    pub keeps_map_timetable: bool,
    pub headway: Option<f64>,
    /// Buses at its busiest, and those the company lacks for it (None: known once drawn).
    pub buses: Option<usize>,
    pub short: usize,
    /// The drivers it needs (hired by the company at night).
    pub drivers: Option<usize>,
    /// The licence at the asking price, for a map line the company takes over.
    pub price: Option<Cents>,
}

/// What the wizard's choices come to (`all`: today's timetable of the map).
pub(crate) fn outcome(c: &Company, pick: &Pick, freq: Freq, all: &[core::LineInfo]) -> Outcome {
    match pick {
        Pick::Draw => Outcome { number: String::new(), freq, keeps_map_timetable: false, headway: None, buses: None, short: 0, drivers: None, price: None },
        Pick::Map(name) => {
            let Some(line) = all.iter().find(|x| x.name == *name) else {
                return Outcome { number: String::new(), freq, keeps_map_timetable: true, headway: None, buses: None, short: 0, drivers: None, price: None };
            };
            let n = co::network::needs(c, line, all, &c.date);
            let headway = map_headway(line);
            let direct = co::concessions::may_add_directly(c);
            Outcome {
                number: co::specials::number_of(line, &[&c.depot]),
                freq: headway.map(Freq::of_headway).unwrap_or(freq),
                keeps_map_timetable: true,
                headway,
                buses: Some(n.buses),
                short: n.short().0,
                drivers: Some(n.drivers),
                price: (!direct).then(|| co::concessions::asking_price_of(c, line)),
            }
        }
    }
}

/// The map's lines the wizard offers: lines the company does not run yet (not the map's depot
/// runs, specials or other operators' traffic, as the Lines page has it).
pub(crate) fn map_choices<'a>(c: &Company, all: &'a [core::LineInfo]) -> Vec<&'a core::LineInfo> {
    all.iter().filter(|x| !c.lines.iter().any(|y| y.name.eq_ignore_ascii_case(&x.name)) && co::concessions::is_line(c, x) && x.tours.iter().any(|t| t.runs)).collect()
}

// --- the money ---------------------------------------------------------------------------------

/// A week of the money: its first day, what came in and what went out.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Week {
    pub from: String,
    pub income: Cents,
    pub expenses: Cents,
}

/// The last `n` weeks of closed days (seven days each, the newest last).
pub(crate) fn weeks(c: &Company, n: usize) -> Vec<Week> {
    let h = &c.history;
    let mut out: Vec<Week> = h.rchunks(7).take(n).map(|w| Week { from: w[0].date.clone(), income: w.iter().map(|d| d.income).sum(), expenses: w.iter().map(|d| d.expenses).sum() }).collect();
    out.reverse();
    out
}

/// The costs of the last 28 days by kind, the biggest first (capital - buses bought, loans
/// repaid - left out).
pub(crate) fn biggest_costs(c: &Company) -> Vec<(BookingKind, Cents)> {
    let since = co::dates::add(&c.date, -28);
    let mut by: Vec<(BookingKind, Cents)> = Vec::new();
    for b in c.ledger.iter().filter(|b| b.amount < 0 && !b.kind.is_capital() && co::dates::between(&since, &b.date) >= 0) {
        match by.iter_mut().find(|x| x.0 == b.kind) {
            Some(x) => x.1 -= b.amount,
            None => by.push((b.kind, -b.amount)),
        }
    }
    by.sort_by_key(|x| std::cmp::Reverse(x.1));
    by
}

/// A cost in plain words: what it is.
pub(crate) fn cost_words(k: BookingKind) -> &'static str {
    match k {
        BookingKind::Wages => "Your drivers' and staff's pay",
        BookingKind::Energy => "Diesel and electricity for the buses",
        BookingKind::Maintenance => "Keeping the buses running",
        BookingKind::Repair => "Repairs after breakdowns",
        BookingKind::Lease => "Leased buses",
        BookingKind::Rent => "Buses rented for the day",
        BookingKind::Insurance => "Insurance of the buses",
        BookingKind::Depot => "The depot's rent and upkeep",
        BookingKind::Penalty => "Fines for bus runs that did not run or ran late",
        BookingKind::Interest => "Interest on loans",
        BookingKind::Concession => "Licences for the lines",
        BookingKind::Parts => "Spare parts",
        BookingKind::Research => "Research",
        BookingKind::Training => "Training for the drivers",
        _ => "Other costs",
    }
}

/// A tip when something costs too much: (what to say, what helps).
pub(crate) fn cost_tip(costs: &[(BookingKind, Cents)]) -> Option<(&'static str, Act)> {
    let total: Cents = costs.iter().map(|x| x.1).sum();
    if total <= 0 {
        return None;
    }
    let share = |k: BookingKind| costs.iter().find(|x| x.0 == k).map(|x| x.1 as f64 / total as f64).unwrap_or(0.0);
    if share(BookingKind::Rent) > 0.08 {
        Some(("Renting buses for the day costs a lot: a bus of your own is cheaper.", Act::BuyBus))
    } else if share(BookingKind::Penalty) > 0.05 {
        Some(("Fines for bus runs that did not run: buy buses so that every run has one.", Act::BuyBus))
    } else if share(BookingKind::Repair) > 0.12 {
        Some(("Repairs cost a lot: old buses break down more often. A newer bus saves money.", Act::BuyBus))
    } else if share(BookingKind::Wages) > 0.65 {
        Some(("Pay is most of your costs: a busier line brings more passengers for the same drivers.", Act::AddLine))
    } else {
        None
    }
}

// --- the drivers -------------------------------------------------------------------------------

/// A driver's skill in stars (1 to 5): driving, punctuality, service and experience.
pub(crate) fn stars(e: &Employee) -> u8 {
    let s = 0.35 * e.skills.driving + 0.3 * e.skills.punctuality + 0.2 * e.skills.service + 0.15 * e.experience.clamp(0.0, 100.0);
    ((s / 20.0).ceil() as i64).clamp(1, 5) as u8
}

/// A driver's shift of today's plan: (the line's number, from, to).
pub(crate) fn shift_of(plan: &Plan, id: u32) -> Option<(String, i32, i32)> {
    running(plan).find_map(|t| t.duties.iter().find(|d| d.driver == Some(id)).map(|d| (t.tour.number.clone(), d.from, d.to)))
}

/// The buses a line runs today.
pub(crate) fn buses_on_line(plan: &Plan, line: &str) -> usize {
    let mut ids: Vec<u32> = plan.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(line)).filter_map(|t| t.bus).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.len()
}

// --- the shop ------------------------------------------------------------------------------------

/// The sizes of bus the company's lines ask for today, the most asked first (a solo bus where a
/// line names none).
pub(crate) fn sizes_wanted(plan: Option<&Plan>, c: &Company) -> Vec<BusSize> {
    let mut count: Vec<(BusSize, usize)> = Vec::new();
    if let Some(p) = plan {
        for t in running(p) {
            let s = co::ownline::wanted(c, &t.tour).or_else(|| co::network::wants_size(&t.tour.ai_group)).unwrap_or(BusSize::Solo);
            match count.iter_mut().find(|x| x.0 == s) {
                Some(x) => x.1 += 1,
                None => count.push((s, 1)),
            }
        }
    }
    count.sort_by_key(|x| std::cmp::Reverse(x.1));
    let mut out: Vec<BusSize> = count.into_iter().map(|x| x.0).collect();
    if out.is_empty() {
        out.push(BusSize::Solo);
    }
    out
}

/// How well a bus of `size` fits the lines (0 best): its place among the sizes wanted, then
/// the others by how near they are to the first.
pub(crate) fn fit_rank(size: BusSize, wanted: &[BusSize]) -> usize {
    match wanted.iter().position(|s| *s == size) {
        Some(k) => k,
        None => wanted.len() + (size as i32 - wanted[0] as i32).unsigned_abs() as usize,
    }
}

/// Every page a Casual page can lead to (for the check that none is a Normal page).
#[cfg(test)]
pub(crate) fn all_acts(c: &Company, plan: Option<&Plan>) -> Vec<Act> {
    let mut v: Vec<Act> = tiles(c, plan, 0).into_iter().flat_map(|t| [Act::Open(t.page), t.action.2]).collect();
    v.extend(tips(c, plan).into_iter().map(|t| t.button.2));
    v.extend([Act::Open(SHIFTS), Act::Open(COMPANY), Act::Open(HOME_TAB), Act::BuyBus, Act::AddLine, Act::BiggerDepot]);
    v.extend(cost_tip(&[(BookingKind::Rent, 100)]).map(|x| x.1));
    v
}

//! The sandbox (Beta 61, Luc: "a sandbox mode where players can change every aspect of the bus
//! company mode the way they want"): a company is played as a Career - the game as it is, the
//! default, and every file of before - or as a Sandbox, whose knobs (`Sandbox`) the player sets
//! on its Sandbox page. A company is founded as one or the other, and a Career company can be
//! turned into a Sandbox once and for good (`make_sandbox`).
//!
//! Every knob is a flag or a factor read where its value is used, never a copy of the rules:
//!
//! - *money*: `can_pay` wherever a cost is checked against the cash (`Company::can_pay`), the
//!   warnings of a company in the red (`quiet_cash`), the loans' interest (`rules`,
//!   `finance::pay_rates`, `finance::early_fee`);
//! - *unlocks*: `unlocked`, asked by `levels::unlocked` (every level's feature and course, the
//!   dealers' bus sizes), `research::has` and `research::stages_of` (all research done),
//!   `levels::max_concessions`, `concessions::rules_of` (the map's lines without a tender) and
//!   `qualify` (every driver's licence, endorsements and bus types);
//! - *economy*: `rules` (the game mode's `economy::Rules` as `economy::rules_for` hands them to
//!   the company: bus prices, compensation, penalties, inflation, illness, applicants),
//!   `econ_uk::factor` (the income: fares, concessions, contracts, grants), `booked` (wages, at `Company::book`) and the day close's
//!   running costs (`day::close_day_with`);
//! - *operations*: the breakdowns and accidents drawn (`clock`, `incidents::trip`), the delay
//!   model (`lateness::model_with`, `staff::signs_on_late`), `reputation::change`,
//!   `complaints::day`, the staff's night (`content`, `staff::after_day`) and the holiday
//!   requests (`holidays::night`);
//! - *staff*: `staff::advert_fee`, `staff::hire` (a new round when a role's applicants are all
//!   hired) and the applicants of a week (`rules`).
//!
//! A Career company reads `CAREER` whatever its file holds (`of`): nothing a sandbox does can
//! reach one, and every knob's default is the Career's own behaviour.

use super::economy::Rules;
use super::model::{BookingKind, Cents, Company, Employee, Licence};
use serde::{Deserialize, Serialize};

/// How the company is played.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum CompanyMode {
    /// The game as it is: its rules, its money, its levels and research.
    #[default]
    Career,
    /// The player sets the rules (`Sandbox`).
    Sandbox,
}

impl CompanyMode {
    pub const ALL: [CompanyMode; 2] = [CompanyMode::Career, CompanyMode::Sandbox];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            CompanyMode::Career => "Career",
            CompanyMode::Sandbox => "Sandbox",
        }
    }

    /// What it means, in a line.
    pub fn info(self) -> &'static str {
        match self {
            CompanyMode::Career => "The game as it is: earn your money, climb the levels, research and win your tenders.",
            CompanyMode::Sandbox => "Set the rules yourself: money, unlocks, prices, wages, disruptions and staff, each on the Sandbox page.",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            CompanyMode::Career => "career",
            CompanyMode::Sandbox => "sandbox",
        }
    }

    pub fn of_key(key: &str) -> Option<CompanyMode> {
        CompanyMode::ALL.iter().copied().find(|m| m.key() == key)
    }
}

/// A sandbox's knobs. Each one's default is what a Career company does.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct Sandbox {
    // the money
    /// Spending never fails, and the balance shows ∞ (the books still count every euro).
    pub unlimited_money: bool,
    /// Spending may take the balance below zero, and nothing warns of being in the red.
    pub no_bankruptcy: bool,
    /// Loans cost no interest and no fee for paying back early.
    pub free_loans: bool,
    // the unlocks
    /// Every level's feature, all research, every bus size, every course, as many concessions as
    /// wanted, the map's lines without a tender, every driver qualified for every bus.
    pub unlock_all: bool,
    // the economy (factors: 1 is the game mode's)
    /// The income: fares, concession reimbursement, contract payments and BSOG (`econ_uk::factor`).
    pub income: f64,
    pub wages: f64,
    /// Fuel and power, maintenance and repairs.
    pub running: f64,
    /// New and used buses (and so their leasing).
    pub prices: f64,
    /// The bus contracts' deductions and default points (`bus_contracts::strictness`).
    pub penalties: f64,
    /// Prices rise over the years, and the contracts' payments are indexed with them.
    pub inflation: bool,
    // the operations (factors: 1 is the game mode's)
    pub breakdowns: f64,
    pub accidents: f64,
    /// The delays of the delay model.
    pub lateness: f64,
    pub sickness: f64,
    pub reputation_gain: f64,
    pub reputation_loss: f64,
    pub complaints: bool,
    /// Satisfaction and stress move, and the unhappy may resign.
    pub morale: bool,
    /// The staff ask for their holidays.
    pub holidays: bool,
    // the staff
    /// Twice the applicants, free job adverts, and a new round when a role's are all hired.
    pub free_hiring: bool,
}

/// What a Career company does: every knob as the game is.
pub const CAREER: Sandbox = Sandbox {
    unlimited_money: false,
    no_bankruptcy: false,
    free_loans: false,
    unlock_all: false,
    income: 1.0,
    wages: 1.0,
    running: 1.0,
    prices: 1.0,
    penalties: 1.0,
    inflation: true,
    breakdowns: 1.0,
    accidents: 1.0,
    lateness: 1.0,
    sickness: 1.0,
    reputation_gain: 1.0,
    reputation_loss: 1.0,
    complaints: true,
    morale: true,
    holidays: true,
    free_hiring: false,
};

impl Default for Sandbox {
    fn default() -> Self {
        CAREER
    }
}

/// The Sandbox page's "Relaxed" preset (Beta 65): the company cannot go broke and borrows free,
/// earns half as much again, pays less for its people, its running and its buses, is fined half,
/// meets half the breakdowns, accidents and illness and fewer delays, gains its reputation faster
/// and loses it slower, and hires without limits. Nothing unlocked, no unlimited money: the game
/// still asks something.
pub const RELAXED: Sandbox = Sandbox {
    unlimited_money: false,
    no_bankruptcy: true,
    free_loans: true,
    unlock_all: false,
    income: 1.5,
    wages: 0.8,
    running: 0.8,
    prices: 0.75,
    penalties: 0.5,
    inflation: true,
    breakdowns: 0.5,
    accidents: 0.5,
    lateness: 0.6,
    sickness: 0.5,
    reputation_gain: 1.5,
    reputation_loss: 0.5,
    complaints: true,
    morale: true,
    holidays: true,
    free_hiring: true,
};

/// The "Hard" preset: less income, dearer people, running and buses, heavier contract penalties,
/// more breakdowns, accidents, delays and illness, reputation slower won and faster lost.
pub const HARD: Sandbox = Sandbox {
    unlimited_money: false,
    no_bankruptcy: false,
    free_loans: false,
    unlock_all: false,
    income: 0.85,
    wages: 1.2,
    running: 1.25,
    prices: 1.25,
    penalties: 1.5,
    inflation: true,
    breakdowns: 1.5,
    accidents: 1.5,
    lateness: 1.3,
    sickness: 1.5,
    reputation_gain: 0.75,
    reputation_loss: 1.5,
    complaints: true,
    morale: true,
    holidays: true,
    free_hiring: false,
};

/// The Sandbox page's presets: their key, their name and their knobs (any other mix is "Custom").
pub const PRESETS: [(&str, &str, Sandbox); 3] = [("relaxed", "Relaxed", RELAXED), ("career", "Career", CAREER), ("hard", "Hard", HARD)];

/// The preset the company's knobs are now (None: a mix of its own, "Custom").
pub fn preset_of(c: &Company) -> Option<&'static str> {
    let s = of(c);
    PRESETS.iter().find(|p| KNOBS.iter().all(|k| s.get(k.key) == p.2.get(k.key))).map(|p| p.0)
}

/// Every knob as a preset has it (`set_knob`, each with what it does at once).
pub fn set_preset(c: &mut Company, key: &str) -> Result<(), &'static str> {
    let Some(p) = PRESETS.iter().find(|p| p.0 == key) else { return Err("There is no such setting.") };
    for k in KNOBS {
        if let Some(v) = p.2.get(k.key) {
            set_knob(c, k.key, v)?;
        }
    }
    Ok(())
}

/// The groups of the Sandbox page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Money,
    Unlocks,
    Economy,
    Operations,
    Staff,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Money, Group::Unlocks, Group::Economy, Group::Operations, Group::Staff];

    pub fn label(self) -> &'static str {
        match self {
            Group::Money => "Money",
            Group::Unlocks => "Unlocks",
            Group::Economy => "Economy",
            Group::Operations => "Operations",
            Group::Staff => "Staff",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Group::Money => "payments",
            Group::Unlocks => "key",
            Group::Economy => "trending_up",
            Group::Operations => "traffic",
            Group::Staff => "groups",
        }
    }
}

/// A knob: a switch, or a factor between its least and most.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Switch,
    Factor { least: f64, most: f64 },
}

/// A knob of the Sandbox page: its key (a field of `Sandbox`), its group, its name and what it
/// does (English, translated when shown).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knob {
    pub key: &'static str,
    pub group: Group,
    pub kind: Kind,
    pub label: &'static str,
    pub info: &'static str,
}

const fn switch(key: &'static str, group: Group, label: &'static str, info: &'static str) -> Knob {
    Knob { key, group, kind: Kind::Switch, label, info }
}

const fn factor(key: &'static str, group: Group, least: f64, most: f64, label: &'static str, info: &'static str) -> Knob {
    Knob { key, group, kind: Kind::Factor { least, most }, label, info }
}

/// The knobs, group by group.
pub const KNOBS: [Knob; 20] = [
    switch("unlimited_money", Group::Money, "Unlimited money", "Spending never fails and the balance shows ∞. The books still count every amount."),
    switch("no_bankruptcy", Group::Money, "Never go broke", "Spending may take the balance below zero, and nothing warns of the company being in the red."),
    switch("free_loans", Group::Money, "Loans without interest", "The bank charges no interest and no fee for paying back early, on new loans and on those running."),
    switch("unlock_all", Group::Unlocks, "Everything unlocked", "Every company level's features and courses, all research, every bus size at the dealers, as many concessions as you like, the map's lines without a tender, and every driver qualified for every bus."),
    factor("income", Group::Economy, 0.0, 10.0, "Income", "Fares, concession reimbursement, contract payments and the BSOG fuel grant."),
    factor("wages", Group::Economy, 0.0, 5.0, "Wages", "Every wage the company pays its people."),
    factor("running", Group::Economy, 0.0, 5.0, "Running costs", "Fuel and power, maintenance and repairs."),
    factor("prices", Group::Economy, 0.1, 5.0, "Bus prices", "New and used buses at the dealers, and so their leasing rates."),
    factor("penalties", Group::Economy, 0.0, 5.0, "Contract penalties", "Deductions for dropped and late journeys under the bus contracts, and their default points."),
    switch("inflation", Group::Economy, "Inflation", "Prices rise over the years, and the contracts' payments are indexed with them. Off: prices stand still."),
    factor("breakdowns", Group::Operations, 0.0, 5.0, "Breakdowns", "How often buses break down on their way."),
    factor("accidents", Group::Operations, 0.0, 5.0, "Accidents", "How often buses have accidents: the scrapes and the serious ones."),
    factor("lateness", Group::Operations, 0.0, 3.0, "Delays", "How late the buses run: the traffic, the drivers and the buses."),
    factor("sickness", Group::Operations, 0.0, 5.0, "Staff illness", "How often people fall ill."),
    factor("reputation_gain", Group::Operations, 0.0, 5.0, "Reputation gained", "What good service and the passengers' goodwill add to the reputation."),
    factor("reputation_loss", Group::Operations, 0.0, 5.0, "Reputation lost", "What delays, dropped trips, accidents and complaints take off the reputation."),
    switch("complaints", Group::Operations, "Passenger complaints", "Passengers write in about what went wrong. Off: no complaints come."),
    switch("morale", Group::Operations, "Staff morale", "Pay, hours, stress and refused holidays move how content people are, and the unhappy may resign. Off: everyone stays content and calm, and nobody resigns."),
    switch("holidays", Group::Operations, "Holiday requests", "People ask for their holidays ahead. Off: nobody asks."),
    switch("free_hiring", Group::Staff, "Hire without limits", "Twice as many applicants, free job adverts, and a role whose applicants are all hired gets new ones at once."),
];

/// The knob of a key.
pub fn knob(key: &str) -> Option<&'static Knob> {
    KNOBS.iter().find(|k| k.key == key)
}

impl Sandbox {
    /// A knob's value: a factor's, or a switch's as 1 (on) or 0 (off).
    pub fn get(&self, key: &str) -> Option<f64> {
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        Some(match key {
            "unlimited_money" => on(self.unlimited_money),
            "no_bankruptcy" => on(self.no_bankruptcy),
            "free_loans" => on(self.free_loans),
            "unlock_all" => on(self.unlock_all),
            "income" => self.income,
            "wages" => self.wages,
            "running" => self.running,
            "prices" => self.prices,
            "penalties" => self.penalties,
            "inflation" => on(self.inflation),
            "breakdowns" => self.breakdowns,
            "accidents" => self.accidents,
            "lateness" => self.lateness,
            "sickness" => self.sickness,
            "reputation_gain" => self.reputation_gain,
            "reputation_loss" => self.reputation_loss,
            "complaints" => on(self.complaints),
            "morale" => on(self.morale),
            "holidays" => on(self.holidays),
            "free_hiring" => on(self.free_hiring),
            _ => return None,
        })
    }

    /// Set a knob: a factor within its range, a switch on for anything but 0.
    pub fn set(&mut self, key: &str, value: f64) -> Result<(), &'static str> {
        let Some(k) = knob(key) else { return Err("There is no such setting.") };
        if !value.is_finite() {
            return Err("Choose a number.");
        }
        let value = match k.kind {
            Kind::Factor { least, most } => (value.clamp(least, most) * 100.0).round() / 100.0,
            Kind::Switch => value,
        };
        let on = value != 0.0;
        match key {
            "unlimited_money" => self.unlimited_money = on,
            "no_bankruptcy" => self.no_bankruptcy = on,
            "free_loans" => self.free_loans = on,
            "unlock_all" => self.unlock_all = on,
            "income" => self.income = value,
            "wages" => self.wages = value,
            "running" => self.running = value,
            "prices" => self.prices = value,
            "penalties" => self.penalties = value,
            "inflation" => self.inflation = on,
            "breakdowns" => self.breakdowns = value,
            "accidents" => self.accidents = value,
            "lateness" => self.lateness = value,
            "sickness" => self.sickness = value,
            "reputation_gain" => self.reputation_gain = value,
            "reputation_loss" => self.reputation_loss = value,
            "complaints" => self.complaints = on,
            "morale" => self.morale = on,
            "holidays" => self.holidays = on,
            "free_hiring" => self.free_hiring = on,
            _ => return Err("There is no such setting."),
        }
        Ok(())
    }

    /// Whether a knob is as a Career company has it.
    pub fn is_career(&self, key: &str) -> bool {
        self.get(key) == CAREER.get(key)
    }
}

// --- the company's mode ------------------------------------------------------------------------

/// The knobs the company plays by: its sandbox's, or a Career company's whatever its file holds.
pub fn of(c: &Company) -> &Sandbox {
    if is_sandbox(c) {
        &c.sandbox
    } else {
        &CAREER
    }
}

pub fn is_sandbox(c: &Company) -> bool {
    c.company_mode == CompanyMode::Sandbox
}

/// Turn a Career company into a Sandbox, for good: its knobs start as the Career's, so nothing
/// changes until the player sets them.
pub fn make_sandbox(c: &mut Company) -> Result<(), &'static str> {
    if is_sandbox(c) {
        return Err("The company is a sandbox already.");
    }
    c.company_mode = CompanyMode::Sandbox;
    c.sandbox = CAREER;
    Ok(())
}

/// Set a knob of the company's sandbox, with what it does at once: everything unlocked stops the
/// research under way (its money and trophies back: it is all researched now) and qualifies the
/// drivers (`qualify`), and switched off takes "any bus type" back from them.
pub fn set_knob(c: &mut Company, key: &str, value: f64) -> Result<(), &'static str> {
    if !is_sandbox(c) {
        return Err("Only a sandbox company has these settings.");
    }
    let was = c.sandbox.unlock_all;
    c.sandbox.set(key, value)?;
    match (was, c.sandbox.unlock_all) {
        (false, true) => {
            if let Some(r) = c.research.running.take() {
                c.book(BookingKind::Research, r.money, format!("{}: everything researched", super::research::topic(&r.topic).map(|t| t.title).unwrap_or("Research")), false);
                c.trophies += r.trophies;
            }
            qualify(c);
        }
        (true, false) => {
            for e in c.staff.iter_mut() {
                e.types.retain(|t| t != super::agency::ANY_TYPE);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Every knob back to the Career's (the company stays a sandbox).
pub fn reset(c: &mut Company) -> Result<(), &'static str> {
    set_preset(c, "career")
}

// --- the money ---------------------------------------------------------------------------------

/// Whether the company can pay `amount` now: its cash holds it, or its sandbox lets spending go
/// on whatever the cash (unlimited money, or never going broke).
pub fn can_pay(c: &Company, amount: Cents) -> bool {
    let s = of(c);
    amount <= 0 || c.cash >= amount || s.unlimited_money || s.no_bankruptcy
}

/// The balance is shown as ∞.
pub fn unlimited(c: &Company) -> bool {
    of(c).unlimited_money
}

/// Nothing warns of the cash: a company with unlimited money, or one that never goes broke.
pub fn quiet_cash(c: &Company) -> bool {
    let s = of(c);
    s.unlimited_money || s.no_bankruptcy
}

/// The most a balance may be set to (a billion euros).
pub const MOST_BALANCE: Cents = 1_000_000_000_00;

/// Set the balance to `cash`: the difference booked as capital (not what running the lines
/// earns), so the books still add up. Returns the difference.
pub fn set_balance(c: &mut Company, cash: Cents) -> Result<Cents, &'static str> {
    if !is_sandbox(c) {
        return Err("Only a sandbox company has these settings.");
    }
    let cash = cash.clamp(-MOST_BALANCE, MOST_BALANCE);
    let delta = cash - c.cash;
    c.book(BookingKind::Capital, delta, "Sandbox: balance set", false);
    Ok(delta)
}

/// Set the reputation (0 to 100).
pub fn set_reputation(c: &mut Company, value: f64) -> Result<(), &'static str> {
    if !is_sandbox(c) {
        return Err("Only a sandbox company has these settings.");
    }
    if !value.is_finite() {
        return Err("Choose a number.");
    }
    c.reputation = value.clamp(0.0, 100.0);
    Ok(())
}

// --- the economy -------------------------------------------------------------------------------

/// The game mode's rules as the company's sandbox sets them (`economy::rules_for`): bus prices,
/// inflation, the loans' interest, illness and the applicants (the penalties are the bus
/// contracts', `bus_contracts::strictness`; the income `econ_uk::factor`'s).
pub fn rules(c: &Company, mut r: Rules) -> Rules {
    let s = of(c);
    r.price_factor *= s.prices;
    if !s.inflation {
        r.inflation = 0.0;
        r.indexation = 0.0;
    }
    if s.free_loans {
        r.loan_rate = 0.0;
    }
    r.sickness *= s.sickness;
    if s.free_hiring {
        r.applicants *= 2;
    }
    r
}

/// An amount as the sandbox books it (`Company::book`): the wages by its factor.
pub fn booked(c: &Company, kind: BookingKind, amount: Cents) -> Cents {
    match kind {
        BookingKind::Wages | BookingKind::Severance if amount < 0 => (amount as f64 * of(c).wages).round() as Cents,
        _ => amount,
    }
}

// --- the unlocks -------------------------------------------------------------------------------

/// Everything is unlocked: levels, research, bus sizes, courses, concessions, licences.
pub fn unlocked(c: &Company) -> bool {
    of(c).unlock_all
}

/// As many concessions as a sandbox with everything unlocked may hold.
pub const UNLIMITED_CONCESSIONS: usize = 999;

/// Every driver qualified for every bus when everything is unlocked: the D licence, every
/// endorsement and any bus type (as an agency driver booked ahead has, `agency::ANY_TYPE`).
pub fn qualify(c: &mut Company) {
    if !unlocked(c) {
        return;
    }
    for e in c.staff.iter_mut().filter(|e| super::staff::drives(e)) {
        qualify_one(e);
    }
}

fn qualify_one(e: &mut Employee) {
    e.licence = Licence::D;
    for x in super::licences::Endorsement::ALL {
        if !e.endorsements.contains(&x) {
            e.endorsements.push(x);
        }
    }
    if !e.types.iter().any(|t| t == super::agency::ANY_TYPE) {
        e.types.push(super::agency::ANY_TYPE.to_string());
    }
}

// --- the staff ---------------------------------------------------------------------------------

/// How content and how calm everyone stays with the staff's morale off.
pub const CONTENT: f64 = 75.0;
pub const CALM: f64 = 25.0;

/// Someone's night with the staff's morale off (`staff::after_day`): content and calm.
pub fn content(s: &Sandbox, e: &mut Employee) {
    if !s.morale {
        e.satisfaction = e.satisfaction.max(CONTENT);
        e.stress = e.stress.min(CALM);
    }
}

/// Someone just hired (`staff::hire`): qualified for every bus when everything is unlocked.
pub fn hired(c: &mut Company, id: u32) {
    if unlocked(c) {
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == id && super::staff::drives(e)) {
            qualify_one(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{BusKind, BusSize, Drive, GameMode};
    use super::super::{economy, found, levels, market, research, staff, Founding};
    use super::*;

    /// (from the cloud session's version of the presets, Beta 65)
    #[test]
    fn a_preset_sets_every_knob_and_is_recognised() {
        let mut c = found(&Founding { name: "P".into(), date: "2024-03-04".into(), company_mode: CompanyMode::Sandbox, ..Default::default() }, "Luc");
        assert_eq!(preset_of(&c), Some("career"));
        set_preset(&mut c, "relaxed").unwrap();
        assert_eq!(c.sandbox, RELAXED);
        assert_eq!(preset_of(&c), Some("relaxed"));
        set_knob(&mut c, "wages", 2.0).unwrap();
        assert_eq!(preset_of(&c), None, "custom");
        set_preset(&mut c, "hard").unwrap();
        assert_eq!(preset_of(&c), Some("hard"));
        // (every preset's values are ones the knobs can hold)
        for (_, _, p) in PRESETS {
            let mut x = CAREER;
            for k in KNOBS {
                x.set(k.key, p.get(k.key).unwrap()).unwrap();
            }
            assert_eq!(x, p);
        }
        assert!(set_preset(&mut c, "nope").is_err());
    }

    fn company(mode: CompanyMode) -> Company {
        found(&Founding { name: "Sandbus".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), company_mode: mode, ..Default::default() }, "Luc")
    }

    const SOLO: BusKind = BusKind { size: BusSize::Solo, drive: Drive::Diesel };

    #[test]
    fn an_old_file_is_a_career_and_a_career_is_untouched() {
        // (a file of before has neither field: a Career with the Career's knobs)
        let c = company(CompanyMode::Career);
        let mut v = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("company_mode");
        v.as_object_mut().unwrap().remove("sandbox");
        let old: Company = serde_json::from_value(v).unwrap();
        assert_eq!(old.company_mode, CompanyMode::Career);
        assert_eq!(old.sandbox, CAREER);
        // a Career company reads the Career's knobs whatever its file holds
        let mut odd = c.clone();
        odd.sandbox.unlimited_money = true;
        odd.sandbox.income = 5.0;
        odd.sandbox.unlock_all = true;
        assert_eq!(of(&odd), &CAREER);
        assert!(!can_pay(&odd, odd.cash + 1));
        assert_eq!(economy::rules_for(&odd), economy::rules(odd.game_mode));
        assert!(!levels::unlocked(&odd, levels::Feature::DoubleDeckers));
        assert!(set_knob(&mut odd, "income", 2.0).is_err());
        // and a sandbox's knobs start as the Career's: nothing changes until they are set
        let s = company(CompanyMode::Sandbox);
        assert_eq!(economy::rules_for(&s), economy::rules_for(&c));
        assert_eq!(market::rules_of(&s), market::rules_of(&c));
        for k in KNOBS {
            assert!(CAREER.get(k.key).is_some(), "{}", k.key);
            assert!(s.sandbox.is_career(k.key));
        }
    }

    #[test]
    fn unlimited_money_never_fails_a_purchase() {
        let mut c = company(CompanyMode::Sandbox);
        c.cash = 0;
        let price = economy::new_price(SOLO, &economy::rules_for(&c), c.price_index);
        assert!(!c.can_pay(price));
        set_knob(&mut c, "unlimited_money", 1.0).unwrap();
        assert!(c.can_pay(price) && c.can_pay(Cents::MAX / 4));
        // research and a training course go through with nothing in the bank
        assert!(research::start(&mut c, "fuel").is_ok());
        assert!(c.cash < 0 && unlimited(&c) && quiet_cash(&c));
        // a career company with the same empty cash is refused
        let mut k = company(CompanyMode::Career);
        k.cash = 0;
        assert_eq!(research::start(&mut k, "fuel"), Err("Not enough cash."));
        // never going broke lets spending go on too, the balance shown as it is
        let mut b = company(CompanyMode::Sandbox);
        b.cash = 0;
        set_knob(&mut b, "no_bankruptcy", 1.0).unwrap();
        assert!(b.can_pay(price) && !unlimited(&b) && quiet_cash(&b));
    }

    #[test]
    fn the_balance_is_set_through_the_books() {
        let mut c = company(CompanyMode::Sandbox);
        let before = c.cash;
        assert_eq!(set_balance(&mut c, 5_000_000_00), Ok(5_000_000_00 - before));
        assert_eq!(c.cash, 5_000_000_00);
        assert!(c.ledger.last().is_some_and(|b| b.kind == BookingKind::Capital));
        // (capital: not what running the lines earned)
        assert_eq!(c.month(&super::super::dates::month_of(&c.date)).result(), 0);
        let mut k = company(CompanyMode::Career);
        assert!(set_balance(&mut k, 1).is_err() && set_reputation(&mut k, 99.0).is_err());
    }

    #[test]
    fn the_economy_factors_apply() {
        let mut c = company(CompanyMode::Sandbox);
        let base = economy::rules(c.game_mode);
        for (k, v) in [("prices", 0.5), ("income", 2.0), ("penalties", 0.0), ("sickness", 3.0), ("free_loans", 1.0), ("inflation", 0.0), ("free_hiring", 1.0)] {
            set_knob(&mut c, k, v).unwrap();
        }
        let r = economy::rules_for(&c);
        assert_eq!(economy::new_price(SOLO, &r, 1.0), economy::new_price(SOLO, &base, 1.0) / 2);
        // (the income knob: the contracts' factor - and with it a weekend line's tourism grant)
        assert!((economy::tourism_per_km(&c) - 2.0 * economy::tourism_per_km(&company(CompanyMode::Sandbox))).abs() < 1e-6);
        // (no penalties: a bus contract deducts nothing and gives no default points)
        assert_eq!(crate::company::bus_contracts::journey_value(&c, "Linie5", None), 0);
        assert_eq!((r.loan_rate, r.inflation, r.indexation), (0.0, 0.0, 0.0));
        assert!((r.sickness - base.sickness * 3.0).abs() < 1e-12);
        assert_eq!(r.applicants, base.applicants * 2);
        assert_eq!(super::super::finance::rate_of(&c), 0.0);
        // the dealer's list price follows
        assert_eq!(super::super::dealer::list_price(&c, SOLO), economy::new_price(SOLO, &base, c.price_index) / 2);
        // the fares a passenger pays
        let mut l = super::super::model::CompanyLine::default();
        l.name = "x".into();
        let plain = company(CompanyMode::Sandbox);
        assert!((super::super::fares::per_passenger(&c, &l) - 2.0 * super::super::fares::per_passenger(&plain, &l)).abs() < 1e-9);
        // the wages booked
        set_knob(&mut c, "wages", 1.5).unwrap();
        let before = c.cash;
        c.book(BookingKind::Wages, -1_000_00, "Someone", false);
        assert_eq!(c.cash, before - 1_500_00);
        c.book(BookingKind::Fares, 1_000_00, "Line 1", false);
        assert_eq!(c.cash, before - 500_00, "only the wages");
        // factors stay in their range
        set_knob(&mut c, "prices", 0.0).unwrap();
        assert_eq!(c.sandbox.prices, 0.1);
        set_knob(&mut c, "income", 99.0).unwrap();
        assert_eq!(c.sandbox.income, 10.0);
        assert!(set_knob(&mut c, "nonsense", 1.0).is_err());
    }

    #[test]
    fn everything_unlocked_opens_every_gate() {
        let mut c = company(CompanyMode::Sandbox);
        c.cash = 10_000_000_00;
        let big = BusKind { size: BusSize::Double, drive: Drive::Electric };
        assert!(market::kind_allowed(&c, big).is_err());
        assert_eq!(levels::max_concessions(&c), 1);
        assert!(!super::super::concessions::may_add_directly(&c));
        let hire = staff::applicants(&c).into_iter().next().unwrap();
        let id = staff::hire(&mut c, &hire).unwrap();
        research::start(&mut c, "fuel").unwrap();
        let running = c.research.running.clone().unwrap();
        let cash = c.cash;
        set_knob(&mut c, "unlock_all", 1.0).unwrap();
        assert!(market::kind_allowed(&c, big).is_ok());
        assert!(levels::Feature::ALL.iter().all(|f| levels::unlocked(&c, *f)));
        assert!(research::TOPICS.iter().all(|t| research::state(&c, t) == research::State::Done));
        assert!(research::has(&c, research::keys::TICKET_INSPECTORS) && staff::hireable(&c, super::super::model::Role::Inspector).is_ok());
        assert_eq!(levels::max_concessions(&c), UNLIMITED_CONCESSIONS);
        assert!(super::super::concessions::may_add_directly(&c));
        // (the research under way is paid back: it is all done now)
        assert!(c.research.running.is_none() && c.cash == cash + running.money);
        // the drivers drive every bus
        let e = c.employee(id).unwrap();
        assert_eq!(super::super::licences::lack(e, big, Some("Vehicles/Any_Model/a.bus")), None);
        // and switched off, the research and the gates are as they were
        set_knob(&mut c, "unlock_all", 0.0).unwrap();
        assert!(market::kind_allowed(&c, big).is_err());
        assert!(!c.employee(id).unwrap().types.iter().any(|t| t == super::super::agency::ANY_TYPE));
    }

    #[test]
    fn morale_off_keeps_everyone_content() {
        let s = Sandbox { morale: false, ..CAREER };
        let mut c = company(CompanyMode::Career);
        let a = staff::applicants(&c).into_iter().next().unwrap();
        let id = staff::hire(&mut c, &a).unwrap();
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        e.satisfaction = 10.0;
        e.stress = 90.0;
        content(&CAREER, e);
        assert_eq!((e.satisfaction, e.stress), (10.0, 90.0), "a career's people as they are");
        content(&s, e);
        assert_eq!((e.satisfaction, e.stress), (CONTENT, CALM));
    }
}

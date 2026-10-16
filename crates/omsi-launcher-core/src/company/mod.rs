//! The bus company: the player's own transport company on a map of theirs (Luc: buy, rent
//! and lease buses, hire staff, run lines - the map's or their own - in a realistic economy
//! of three difficulties). Phase 1 is the core: the company and its books (`model`), the
//! economy (`economy`, `finance`), the fleet and its market (`market`), the staff
//! (`staff`), its lines and their tours (`network`) and the company day (`day`): closing a
//! day settles it with the model and books what the player drove himself as measured. It is
//! saved per company in `~/.openomsi/companies/<id>.json` (`store`).
//!
//! Time is a mix: later the company runs live with the game's clock while the player drives
//! (its buses as AI; `day::record_live` is the hook), and days not played are settled with
//! "Close the day". Multiplayer companies (the server the boss, players with roles), the full
//! planning and the map of the fleet come in later phases.
//!
//! Everything here is plain functions over plain data, without a window; the launcher's pages
//! call them, and the rules are tested on their own.

pub mod adverts;
// (what is coming: Beta 57's agenda)
pub mod agenda;
pub mod agency;
pub mod auction;
pub mod autopilot;
pub mod bus_contracts;
pub mod career;
pub mod clock;
// (Omsi-Hub: a company as one cloud save, docs/omsihub/ONLINE.md)
pub mod cloud;
pub mod concessions;
pub mod complaints;
pub mod contracts;
// (Beta 62: the money shown and typed in the company's own currency)
pub mod currency;
pub mod control;
pub mod crew;
pub mod dates;
pub mod day;
pub mod dealer;
pub mod duty_launch;
pub mod depot;
pub mod economy;
// (the economy's parameters, from UK operator data: Realistic at factor 1.0)
pub mod econ_uk;
pub mod fares;
pub mod finance;
pub mod holidays;
pub mod inbox;
pub mod lateness;
pub mod fleetmap;
pub mod incidents;
// (the lines' income and passengers on the UK model: fares by ticket, concessions, BSOG)
pub mod income;
pub mod levels;
pub mod licences;
pub mod livery;
pub mod market;
pub mod model;
// (the people's names by the country the company plays in)
pub mod names;
pub mod network;
pub mod ownline;
pub mod parts;
// (drivers paid by the hour of duty)
pub mod pay;
pub mod plan;
pub mod plandays;
pub mod rankings;
pub mod registrations;
pub mod remote;
pub mod reputation;
pub mod research;
pub mod rng;
pub mod specials;
pub mod staff;
pub mod store;
pub mod rosterflow;
pub mod rosterpdf;
// (Beta 61: a company played as a sandbox, its rules set by the player)
pub mod sandbox;
pub mod studio;
pub mod timeplan;
// (Beta 58: friends drive duties of the company's day in the boss's world)
pub mod together;
pub mod training;
pub mod unassign;
pub mod variations;
pub mod vehicles;

pub use model::*;

/// What a new company is made of (the founding wizard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Founding {
    pub name: String,
    pub short: String,
    pub colours: [String; 2],
    pub logo: Option<String>,
    pub map: String,
    pub map_name: String,
    pub depot: String,
    /// The first company day (empty: the launcher's default date).
    pub date: String,
    /// The game mode: how much of the company the player runs himself, and how hard it is.
    pub game_mode: GameMode,
    /// How the dealer sells buses first: the quick buy or haggling and a contract (Casual
    /// buys quickly: this is kept for when the company leaves Casual).
    pub buying: dealer::BuyingMode,
    /// The buses it works with (`vehicles`; the founding wizard proposes the map's region).
    pub vehicles: vehicles::VehicleChoice,
    /// Career or Sandbox (`sandbox`).
    pub company_mode: sandbox::CompanyMode,
    /// The country it plays in, its people's names (`names`; empty: the map's).
    pub country: String,
    /// The currency its money is shown and typed in (`currency`).
    pub currency: currency::Currency,
}

/// A name as a file name: lower case, letters and digits, dashes between.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() {
        "company".to_string()
    } else {
        s
    }
}

/// The short name a name suggests: its words' initials ("Stadtbus Grundorf" SG), or its first
/// letters.
pub fn short_of(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_alphanumeric)).collect();
    let s: String = if words.len() >= 2 { words.iter().filter_map(|w| w.chars().next()).take(4).collect() } else { name.chars().filter(|c| c.is_alphanumeric()).take(3).collect() };
    s.to_uppercase()
}

/// Found a company: its starting capital booked, nothing else yet.
pub fn found(f: &Founding, profile: &str) -> Company {
    let r = economy::rules(f.game_mode);
    let date = if dates::parse(&f.date).is_some() { f.date.clone() } else { crate::DEFAULT_DATE.to_string() };
    let short = if f.short.trim().is_empty() { short_of(&f.name) } else { f.short.trim().to_uppercase() };
    let colour = |c: &str, d: &str| if c.trim().is_empty() { d.to_string() } else { c.trim().to_string() };
    let mut c = Company {
        version: VERSION,
        id: slug(&f.name),
        profile: profile.to_string(),
        name: f.name.trim().to_string(),
        short,
        colours: [colour(&f.colours[0], "#f28c28"), colour(&f.colours[1], "#1d2b44")],
        logo: f.logo.clone(),
        map: f.map.clone(),
        map_name: f.map_name.clone(),
        depot: f.depot.clone(),
        founded: date.clone(),
        date,
        game_mode: GameMode::Normal,
        legacy_difficulty: None,
        cash: 0,
        reputation: 50.0,
        punctuality: 90.0,
        price_index: 1.0,
        contract_index: 1.0,
        ledger: Vec::new(),
        months: Vec::new(),
        history: Vec::new(),
        loans: Vec::new(),
        fleet: Vec::new(),
        staff: Vec::new(),
        lines: Vec::new(),
        taken: Taken::default(),
        counters: Counters::default(),
        trips_seen: 0,
        live: Vec::new(),
        last_report: None,
        planning: Default::default(),
        progress: Default::default(),
        site: Default::default(),
        concessions: Default::default(),
        clock: Default::default(),
        dealer: dealer::DealerState { mode: f.buying, ..Default::default() },
        designs: Vec::new(),
        adverts: Default::default(),
        quals: 1,
        trophies: 0,
        // (a new company researches what an older one could buy already: `research::migrate`)
        research: research::Research { checked: true, ..Default::default() },
        stores: parts::Stores::first(),
        contracts: Default::default(),
        inbox: Default::default(),
        staff_rules: Default::default(),
        legacy_mode: None,
        addons: Default::default(),
        autopilot: Default::default(),
        vehicles: f.vehicles.clone(),
        together: Default::default(),
        reputation_log: Default::default(),
        complaints: Default::default(),
        lateness: Default::default(),
        company_mode: f.company_mode,
        sandbox: sandbox::CAREER,
        country: if names::known(&f.country) { names::pool(&f.country).key.clone() } else { names::guess(&f.map, &f.map_name).to_string() },
        currency: f.currency,
        crew: Default::default(),
    };
    // (Casual buys quickly; the buying chosen is kept for the other modes)
    autopilot::set_game_mode(&mut c, f.game_mode);
    c.book(BookingKind::Capital, r.start_capital, c.name.clone(), false);
    c
}

/// What the dashboard warns of.
#[derive(Clone, Debug, PartialEq)]
pub enum Alert {
    NoLines,
    NoBuses,
    NoDrivers,
    /// Tours of today not covered: those without a bus, and duties without a driver.
    Uncovered { tours: usize, buses: usize, duties: usize },
    /// Less cash than a month of wages.
    LowCash,
    /// Buses due for their service.
    ServiceDue(usize),
    /// People unhappy enough to leave.
    Unhappy(usize),
    /// A leased or rented bus goes back within a week.
    GoingBack { number: String, until: String },
    /// Lines not in service yet (not planned): their numbers.
    NotPlanned(Vec<String>),
    /// Tours of tomorrow not covered (lines in service): without a bus, duties without a driver.
    Tomorrow { tours: usize, buses: usize, duties: usize },
}

/// Tomorrow's tours not covered, from tomorrow's plan.
pub fn tomorrow_alert(p: &day::Plan) -> Option<Alert> {
    let n = p.uncovered();
    let (buses, duties) = p.short_of();
    (n > 0).then_some(Alert::Tomorrow { tours: n, buses, duties })
}

/// The company's alerts, with today's plan when it is known.
pub fn alerts(c: &Company, plan: Option<&day::Plan>) -> Vec<Alert> {
    let mut out = Vec::new();
    if c.lines.is_empty() {
        out.push(Alert::NoLines);
    }
    if c.fleet.is_empty() {
        out.push(Alert::NoBuses);
    }
    if !c.staff.iter().any(staff::drives) {
        out.push(Alert::NoDrivers);
    }
    let now = clock::now(c);
    let waiting: Vec<String> = c.lines.iter().filter(|l| !network::in_service(l, now) && l.service_from.is_none_or(|s| s > now)).map(|l| l.number.clone()).collect();
    if !waiting.is_empty() {
        out.push(Alert::NotPlanned(waiting));
    }
    if let Some(p) = plan {
        let n = p.uncovered();
        if n > 0 {
            let (buses, duties) = p.short_of();
            out.push(Alert::Uncovered { tours: n, buses, duties });
        }
    }
    let wages: Cents = c.staff.iter().map(staff::monthly_cost).sum();
    // (a sandbox with unlimited money, or that never goes broke, is not warned)
    if !sandbox::quiet_cash(c) && (c.cash < wages || c.cash < 0) {
        out.push(Alert::LowCash);
    }
    // (a bus with a job waiting for the workshop is seen to already)
    let due = c.fleet.iter().filter(|v| v.km >= v.next_service_km - 1_000.0 && !v.in_workshop(&c.date) && c.site.job_of(v.id).is_none()).count();
    if due > 0 {
        out.push(Alert::ServiceDue(due));
    }
    let unhappy = c.staff.iter().filter(|e| e.satisfaction < 35.0 && e.notice_until.is_none()).count();
    if unhappy > 0 {
        out.push(Alert::Unhappy(unhappy));
    }
    for v in &c.fleet {
        if let Tenure::Leased { until, .. } | Tenure::Rented { until, .. } = &v.tenure {
            if (0..7).contains(&dates::between(&c.date, until)) {
                out.push(Alert::GoingBack { number: v.number.clone(), until: until.clone() });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_company_is_founded_with_its_capital() {
        let c = found(&Founding { name: "Stadtbus Grundorf".into(), game_mode: GameMode::Realistic, date: "2024-05-06".into(), ..Default::default() }, "Luc");
        assert_eq!((c.id.as_str(), c.short.as_str(), c.date.as_str(), c.cash), ("stadtbus-grundorf", "SG", "2024-05-06", economy::rules(GameMode::Realistic).start_capital));
        assert_eq!(c.ledger.len(), 1);
        assert!(c.ledger[0].kind.is_capital());
        assert_eq!(c.month("2024-05").result(), 0);
        assert_eq!(short_of("Omsi"), "OMS");
        assert_eq!(slug("  Bus & Bahn GmbH "), "bus-bahn-gmbh");
        assert_eq!(slug("!!"), "company");
        let a = alerts(&c, None);
        assert!(a.contains(&Alert::NoLines) && a.contains(&Alert::NoBuses) && a.contains(&Alert::NoDrivers));
        // without a date: the launcher's
        assert_eq!(found(&Founding::default(), "x").date, crate::DEFAULT_DATE);
    }
}

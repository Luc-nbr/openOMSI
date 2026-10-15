//! The company's start screen (`crates/omsi-launcher-ui/src/company/home.rs`): every page as a
//! tile, grouped by theme - at a glance, operations, people, business, the company - each with
//! what matters there now and a badge where something waits; under them the ways to what hides
//! behind a page's button. The badges are the strip's too (`Marks`).

use super::words::{count_text, eur, filled, grouped, n_of, short_date, signed, share_tone, tr, trf};
use super::{agenda, overview, sandbox, AGENDA_TAB, CONCESSIONS_TAB, CONTRACTS_TAB, DEPOT_TAB, ICONS, INBOX_TAB, MAP_TAB, OPS_TAB, REPUTATION_TAB, RESEARCH_TAB, SANDBOX_TAB, WORKSHOP_TAB};
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::{self as co, Company, Role};
use serde_json::{json, Value};

/// What wants attention, page by page: the overview's alerts on the page that helps and the
/// inbox's unread messages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub unread: usize,
    /// The passengers' complaints to answer.
    pub complaints: usize,
    /// (the page, what the alert says, its tone)
    pub alerts: Vec<(usize, String, &'static str)>,
    /// What service control has for the player (`control::waiting`).
    pub ops: usize,
}

impl Marks {
    pub fn of(c: &Company, plan: Option<&Plan>) -> Marks {
        let unread = c.inbox.messages.iter().filter(|m| !m.read).count();
        let easy = co::autopilot::on(c);
        let alerts = co::alerts(c, plan)
            .iter()
            .filter_map(|a| overview::alert_for(a, easy))
            // (a bus going back is said on the overview, but is nothing to mend)
            .filter(|(_, _, tone, _)| *tone != "soft")
            .map(|(_, text, tone, tab)| (tab, text, tone))
            .collect();
        Marks { unread, complaints: co::complaints::open_count(c), alerts, ops: co::control::waiting(c) }
    }

    /// The badge of a page: its unread messages (the inbox), else how many alerts it has, in the
    /// tone of the worst; and the alerts' words.
    pub fn badge(&self, tab: usize) -> Option<(String, &'static str, String)> {
        if tab == REPUTATION_TAB {
            return (self.complaints > 0).then(|| (badge_text(self.complaints), "warn", trf("%{n} complaints to answer", &[("n", &self.complaints.to_string())])));
        }
        if tab == INBOX_TAB {
            return (self.unread > 0).then(|| (badge_text(self.unread), "accent", trf("%{n} unread", &[("n", &self.unread.to_string())])));
        }
        if tab == OPS_TAB {
            return (self.ops > 0).then(|| (badge_text(self.ops), "warn", count_text(self.ops, "1 thing for service control", "%{n} things for service control")));
        }
        let mine: Vec<&(usize, String, &'static str)> = self.alerts.iter().filter(|a| a.0 == tab).collect();
        if mine.is_empty() {
            return None;
        }
        let tone = if mine.iter().any(|a| a.2 != "warn") { "danger" } else { "warn" };
        let tip = mine.iter().map(|a| a.1.as_str()).collect::<Vec<_>>().join("\n");
        Some((mine.len().to_string(), tone, tip))
    }
}

/// A count as a badge says it ("9+" past nine).
fn badge_text(n: usize) -> String {
    if n > 9 {
        "9+".to_string()
    } else {
        n.to_string()
    }
}

/// A tile: where it goes, its name and icon, what matters there now (each line in its tone).
struct Tile {
    to: String,
    name: &'static str,
    icon: &'static str,
    lines: Vec<(String, &'static str)>,
    badge: Option<(String, &'static str)>,
    tip: String,
    big: bool,
    extra: Value,
}

impl Tile {
    fn new(to: impl Into<String>, name: &'static str, icon: &'static str) -> Tile {
        Tile { to: to.into(), name, icon, lines: Vec::new(), badge: None, tip: String::new(), big: false, extra: Value::Null }
    }

    fn line(mut self, text: impl Into<String>, tone: &'static str) -> Tile {
        let t = text.into();
        if !t.is_empty() {
            self.lines.push((t, tone));
        }
        self
    }

    fn json(&self) -> Value {
        json!({
            "to": self.to, "name": tr(self.name), "icon": self.icon,
            "lines": self.lines.iter().map(|(t, tone)| json!([t, tone])).collect::<Vec<_>>(),
            "badge": self.badge.as_ref().map(|(n, tone)| json!({ "n": n, "tone": tone })),
            "tip": self.tip, "big": self.big, "extra": self.extra,
        })
    }
}

fn tab(t: usize) -> String {
    format!("tab:{t}")
}

/// The results of the last 30 days up to yesterday, oldest first (null: not closed).
fn last_days(c: &Company) -> Vec<Option<i64>> {
    let Some(t) = co::dates::parse(&c.date) else { return Vec::new() };
    (1..=30)
        .rev()
        .map(|back| {
            let d = co::dates::fmt(t - back);
            c.history.iter().find(|h| h.date == d).map(|h| h.result)
        })
        .collect()
}

/// The start screen as the company is now (`home::sections` and `home::shortcuts`).
pub fn view(s: &mut super::Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let plan = s.plan.clone();
    let marks = Marks::of(&c, plan.as_ref());
    let today = c.date.as_str();
    let mark = |mut t: Tile| {
        if let Some(k) = t.to.strip_prefix("tab:").and_then(|x| x.parse::<usize>().ok()) {
            if let Some((n, tone, tip)) = marks.badge(k) {
                t.badge = Some((n, tone));
                t.tip = tip;
            }
        }
        t
    };
    // at a glance: the overview and the inbox
    let month = c.month(&co::dates::month_of(today));
    let (res, res_c) = signed(month.result());
    let mut ov = Tile::new(tab(0), "Overview", ICONS[0]).line(trf("This month: %{amount}", &[("amount", &res)]), res_c);
    ov.big = true;
    ov.extra = json!({ "kind": "money", "cash": sandbox::cash(&c), "tone": sandbox::cash_tone(&c), "days": last_days(&c), "none": tr("No day closed yet") });
    let mut inbox = Tile::new(tab(INBOX_TAB), "Inbox", ICONS[INBOX_TAB]);
    inbox = if marks.unread > 0 { inbox.line(n_of("%{n} unread", marks.unread), "accent2") } else { inbox.line(tr("Nothing unread"), "dim") };
    inbox.big = true;
    let newest: Vec<Value> = c.inbox.messages.iter().rev().take(8).map(|m| json!([filled(&m.title, &m.args), short_date(&m.date), !m.read])).collect();
    inbox.extra = json!({ "kind": "messages", "list": newest, "none": tr("No messages yet") });
    // (Beta 57: what is coming, the Agenda's next entries; a badge for what needs the player
    // this week)
    let (soon, needs) = agenda::coming(s, 8, false);
    let mut coming = Tile::new(tab(AGENDA_TAB), "Agenda", ICONS[AGENDA_TAB]).line(agenda::needs_text(needs), if needs > 0 { "warn" } else { "dim" });
    coming.big = true;
    if needs > 0 {
        coming.badge = Some((badge_text(needs), "warn"));
        coming.tip = agenda::needs_text(needs);
    }
    coming.extra = json!({ "kind": "agenda", "title": tr("Coming up"), "list": soon, "none": tr("Nothing coming up") });
    // operations: the planning, the lines, the map, the fleet
    let mut planning = Tile::new(tab(5), "Planning", ICONS[5]);
    planning = match &plan {
        Some(p) if p.tours.iter().any(|t| !t.tour.unplanned) => {
            let n = p.tours.iter().filter(|t| !t.tour.unplanned).count();
            let covered = n - p.uncovered();
            let frac = covered as f64 / n as f64;
            let (buses, duties) = p.short_of();
            let t = planning.line(trf("Covered today: %{c} of %{t} tours", &[("c", &covered.to_string()), ("t", &n.to_string())]), share_tone(frac, false));
            if buses + duties > 0 {
                t.line(trf("%{b} without a bus · %{d} without a driver", &[("b", &buses.to_string()), ("d", &duties.to_string())]), "dim")
            } else {
                t.line(tr("Every tour has its bus and drivers"), "dim")
            }
        }
        Some(_) => planning.line(tr("No tours run today."), "soft"),
        None => planning.line(tr("Reading the timetable…"), "dim"),
    };
    // (service control: the buses late, the drivers late, the breakdowns of today)
    let late = c.clock.today.as_ref().filter(|x| x.date == c.date).map_or(0, |x| x.watch.late.len());
    let ops = Tile::new(tab(OPS_TAB), "Operations", ICONS[OPS_TAB]).line(
        if late > 0 { count_text(late, "1 bus running late", "%{n} buses running late") } else { tr("Every bus on time") },
        if late > 0 { "warn" } else { "soft" },
    ).line(tr("Service control: bring late buses back on time"), "dim");
    let now = co::clock::now(&c);
    let running = c.lines.iter().filter(|x| co::network::in_service(x, now)).count();
    let mut lines = Tile::new(tab(3), "Lines", ICONS[3]);
    lines = if c.lines.is_empty() {
        lines.line(tr("No line yet: take one on"), "warn")
    } else {
        let t = lines.line(count_text(c.lines.len(), "1 line", "%{n} lines"), "soft");
        if running < c.lines.len() {
            t.line(n_of("%{n} not in service yet", c.lines.len() - running), "warn")
        } else {
            t.line(tr("All in service"), "dim")
        }
    };
    let minute = c.clock.minute as i32;
    let on_road = plan.as_ref().map_or(0, |p| p.tours.iter().filter(|t| !t.tour.unplanned && t.covered() && t.tour.from() <= minute && minute < t.tour.to()).count());
    let map = Tile::new(tab(MAP_TAB), "Map", ICONS[MAP_TAB]).line(count_text(on_road, "1 bus on the road now", "%{n} buses on the road now"), if on_road > 0 { "soft" } else { "dim" }).line(tr("Every line and bus where it is"), "dim");
    let held: Vec<&co::Vehicle> = c.fleet.iter().filter(|v| v.held_on(today)).collect();
    let in_shop = held.iter().filter(|v| v.in_workshop(today)).count();
    let mut fleet = Tile::new(tab(1), "Fleet", ICONS[1]);
    fleet = if held.is_empty() {
        fleet.line(tr("No bus yet: buy, lease or rent one"), "warn")
    } else {
        let t = fleet.line(count_text(held.len(), "1 bus", "%{n} buses"), "soft").line(n_of("%{n} ready today", held.len() - in_shop), "dim");
        if in_shop > 0 {
            t.line(n_of("%{n} in the workshop", in_shop), "warn")
        } else {
            t
        }
    };
    // people: the staff, hiring, training
    let staff: Vec<&co::Employee> = c.staff.iter().filter(|e| e.employed_on(today)).collect();
    let here = staff.iter().filter(|e| !e.absent(today)).count();
    let mut people = Tile::new(tab(2), "Staff", ICONS[2]);
    people = if staff.is_empty() { people.line(tr("Nobody works here yet"), "warn") } else { people.line(count_text(staff.len(), "1 person", "%{n} people"), "soft").line(n_of("%{n} at work today", here), "dim") };
    people.big = true;
    let roles: Vec<Value> = Role::ALL.iter().map(|r| (r.label(), staff.iter().filter(|e| e.role == *r).count())).filter(|(l, n)| *n > 0 || *l == Role::Driver.label()).map(|(l, n)| json!([tr(l), n])).collect();
    people.extra = json!({ "kind": "roles", "list": roles });
    let applicants = co::staff::applicants(&c).len();
    let hire = Tile::new("hire", "Hire staff", "person").line(count_text(applicants, "1 applicant this week", "%{n} applicants this week"), if applicants > 0 { "soft" } else { "dim" });
    let learning = staff.iter().filter(|e| e.training_until.as_deref().is_some_and(|u| co::dates::between(today, u) >= 0)).count();
    let training = Tile::new("courses", "Training", "badge").line(if learning > 0 { n_of("%{n} on a course today", learning) } else { tr("Courses for drivers and staff") }, if learning > 0 { "soft" } else { "dim" });
    // business: the finances, the concessions, the contracts, the career
    let mut money = Tile::new(tab(4), "Finances", ICONS[4]).line(trf("This month: %{amount}", &[("amount", &res)]), res_c);
    money = if c.debt() > 0 { money.line(trf("Loans: %{amount}", &[("amount", &eur(c.debt()))]), "dim") } else { money.line(tr("No loans"), "dim") };
    let held_c = c.concessions.held.len();
    let open = c.concessions.tenders.iter().filter(|t| t.open()).count();
    let live = c.concessions.tenders.iter().filter(|t| t.running(now)).count();
    let mut concessions = Tile::new(tab(CONCESSIONS_TAB), "Concessions", ICONS[CONCESSIONS_TAB]).line(count_text(held_c, "1 concession held", "%{n} concessions held"), "soft").line(count_text(open, "1 tender open", "%{n} tenders open"), "dim");
    let mut live_badge = None;
    if live > 0 {
        concessions = concessions.line(count_text(live, "1 auction running now", "%{n} auctions running now"), "accent2");
        live_badge = Some((live.to_string(), count_text(live, "1 auction running now", "%{n} auctions running now")));
    }
    let running_goals = c.contracts.goals.iter().filter(|g| g.state == co::contracts::State::Running).count();
    let contracts = Tile::new(tab(CONTRACTS_TAB), "Contracts", ICONS[CONTRACTS_TAB]).line(count_text(c.trophies.max(0) as usize, "1 trophy", "%{n} trophies"), "soft").line(
        if running_goals > 0 {
            count_text(running_goals, "1 goal running", "%{n} goals running")
        } else if co::research::has(&c, co::research::keys::WLAN) {
            tr("Goals, and internet for the buses")
        } else {
            tr("Goals with a reward, service contracts")
        },
        "dim",
    );
    let rep = Tile::new(tab(REPUTATION_TAB), "Reputation", ICONS[REPUTATION_TAB]).line(trf("%{n} of 100", &[("n", &format!("{:.0}", c.reputation))]), "soft").line(if marks.complaints > 0 { trf("%{n} complaints to answer", &[("n", &marks.complaints.to_string())]) } else { tr("What raises and lowers it") }, if marks.complaints > 0 { "warn" } else { "dim" });
    let lv = co::levels::level(&c);
    let (xp, _, next) = co::levels::progress(&c);
    let mut career = Tile::new(tab(6), "Career", ICONS[6]).line(format!("{}  ·  {}", trf("Level %{n}", &[("n", &lv.to_string())]), tr(co::levels::title_of(lv))), "soft");
    if let Some(n) = next {
        career = career.line(trf("%{xp} of %{next} points", &[("xp", &grouped(xp as f64)), ("next", &grouped(n as f64))]), "dim");
    }
    // the company: its depot, workshop, research and settings
    let taken = c.fleet.iter().filter(|v| v.held_on(today)).count();
    let places = co::depot::places(&c);
    let mut depot = Tile::new(tab(DEPOT_TAB), "Depot", ICONS[DEPOT_TAB]).line(trf("%{n} of %{m} places taken", &[("n", &taken.to_string()), ("m", &places.to_string())]), if taken > places { "warn" } else { "soft" });
    if !c.site.works.is_empty() {
        depot = depot.line(count_text(c.site.works.len(), "1 building work under way", "%{n} building works under way"), "accent2");
    }
    let waiting = c.site.jobs.iter().filter(|j| j.started.is_none()).count();
    let mut workshop = Tile::new(tab(WORKSHOP_TAB), "Workshop", ICONS[WORKSHOP_TAB]).line(count_text(in_shop, "1 bus in the workshop", "%{n} buses in the workshop"), "soft");
    let for_parts = c.stores.waiting.len();
    if for_parts > 0 {
        workshop = workshop.line(count_text(for_parts, "1 bus waits for a part", "%{n} buses wait for parts"), "warn");
    } else if waiting > 0 {
        workshop = workshop.line(count_text(waiting, "1 job waiting", "%{n} jobs waiting"), "warn");
    } else {
        workshop = workshop.line(tr("Spare parts and repairs"), "dim");
    }
    let under_way = c.research.running.as_ref().and_then(|r| co::research::topic(&r.topic).map(|t| (t, r)));
    let research = match under_way {
        Some((t, r)) => Tile::new(tab(RESEARCH_TAB), "Research", ICONS[RESEARCH_TAB])
            .line(format!("{}  ·  {}%", tr(t.title), (r.done / r.work.max(1e-9) * 100.0).clamp(0.0, 100.0).floor()), "soft")
            .line(n_of("%{n} researched", c.research.done.len()), "dim"),
        None => Tile::new(tab(RESEARCH_TAB), "Research", ICONS[RESEARCH_TAB]).line(n_of("%{n} researched", c.research.done.len()), "soft").line(count_text(c.trophies.max(0) as usize, "1 trophy to spend", "%{n} trophies to spend"), "dim"),
    };
    let settings = Tile::new("settings", "Settings", "settings").line(tr("Name, mark, date and buying"), "dim");
    // (Beta 61: a sandbox's rules, its own page)
    let sandbox_tile = co::sandbox::is_sandbox(&c).then(|| Tile::new(tab(SANDBOX_TAB), "Sandbox", ICONS[SANDBOX_TAB]).line(sandbox::changed_text(&c), "soft").line(tr("Set the company's rules"), "dim"));
    let mut concessions = mark(concessions);
    if concessions.badge.is_none() {
        if let Some((n, tip)) = live_badge {
            concessions.badge = Some((n, "accent"));
            concessions.tip = tip;
        }
    }
    let sections: Vec<(&str, Vec<Tile>)> = vec![
        ("At a glance", vec![mark(ov), coming, mark(inbox)]),
        ("Operations", vec![mark(planning), mark(ops), mark(lines), mark(map), mark(fleet)]),
        ("People", vec![mark(people), hire, training]),
        ("Business", vec![mark(money), concessions, mark(contracts), mark(rep), mark(career)]),
        ("Company", vec![mark(depot), mark(workshop), mark(research), settings].into_iter().chain(sandbox_tile).collect()),
    ];
    let sections: Vec<Value> = sections
        .into_iter()
        .map(|(title, tiles)| {
            let dot = tiles.iter().any(|t| t.badge.is_some());
            json!({ "title": tr(title), "dot": dot, "tiles": tiles.iter().map(Tile::json).collect::<Vec<_>>() })
        })
        .collect();
    // the ways under the tiles
    let mut ways = vec![("dealer", "Dealer", "airport_shuttle"), ("adverts", "Advertising", "campaign"), ("loan", "Take a loan", "receipt_long"), ("new_line", "Make a new line", "alt_route"), ("licences", "Train drivers", "key"), ("my_duties", "My duties", "schedule"), ("rankings", "Rankings", "stars"), ("statistics", "Statistics", "trending_up")];
    if c.last_report.is_some() {
        ways.push(("report", "Show the report", "description"));
    }
    ways.push(("chooser", "Other company", "sync_alt"));
    let ways: Vec<Value> = ways.into_iter().map(|(to, label, icon)| json!({ "to": to, "label": tr(label), "icon": icon })).collect();
    json!({ "sections": sections, "ways": ways })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    #[test]
    fn a_new_company_is_told_what_to_do_first() {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Hub".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let marks = Marks::of(&c, None);
        assert!(marks.badge(3).is_some() && marks.badge(1).is_some() && marks.badge(2).is_some());
        assert!(marks.badge(INBOX_TAB).is_none());
        assert_eq!(badge_text(10), "9+");
        let mut s = super::super::Session { company: Some(c), ..Default::default() };
        let v = view(&mut s);
        assert_eq!(v["sections"].as_array().map(Vec::len), Some(5));
        // every page has its tile
        let tos: Vec<String> = v["sections"].as_array().unwrap().iter().flat_map(|x| x["tiles"].as_array().unwrap().iter().map(|t| t["to"].as_str().unwrap().to_string())).collect();
        for k in 0..super::super::TABS.len() {
            // (the Sandbox page is a sandbox company's alone)
            assert_eq!(tos.contains(&format!("tab:{k}")), k != SANDBOX_TAB, "page {k}");
        }
        assert_eq!(v["sections"][1]["tiles"][0]["lines"][0][0], "Reading the timetable…");
    }
}

//! RosterFlow's Roster tab (`crates/omsi-launcher-ui/src/company/planning/rf_roster.rs`): the
//! week's roster drawn as the rest of RosterFlow is - the next seven days, the tools (repeat,
//! clear the day, fill the roster), the day's tours by line with their duties on the hours, and
//! beside them the day, a duty or a tour with the drivers and buses to give - and the dialog that
//! puts a line into service (`planning::service_dialog`). What the roster's rules say
//! (`co::plan`) is said here in words, every colour as RosterFlow's tone name ("warn", "err",
//! "ok", "text", "soft", "dim", "faint", "accent").

use super::super::company::kit::{Go, Popup};
use super::super::company::words::{day_label, eur, share_tone, tr, trf, MONTHS, WEEKDAYS};
use super::super::company::{self, Session};
use super::{str_arg, usize_arg, Rf};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::clock::{self as ck, Choice, Level, What};
use omsi_launcher_lib::company::licences::{self, Lack};
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, DayTour, Disruption, Fill, Problem, Source, Warn, Who};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::unassign as un;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// The colours drivers are told apart by (calm, and readable under dark ink: `planning::DRIVERS`).
const DRIVERS: [(u8, u8, u8); 10] = [(110, 168, 230), (96, 196, 170), (196, 150, 214), (226, 184, 110), (140, 190, 120), (226, 150, 130), (150, 206, 240), (200, 200, 120), (180, 160, 230), (120, 200, 210)];

fn driver_colour(c: &Company, id: u32) -> String {
    let k = c.staff.iter().position(|e| e.id == id).unwrap_or(id as usize);
    let (r, g, b) = DRIVERS[k % DRIVERS.len()];
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn hhmm(minutes: i32) -> String {
    co::clock::hhmm(minutes as i64)
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

/// Someone of the staff by name - or an agency driver booked for the day by his number
/// (`co::agency`).
fn name_of(c: &Company, id: u32) -> String {
    match c.employee(id) {
        Some(e) if co::agency::is_booked(e.id) => trf("Agency driver %{n}", &[("n", &co::agency::number_of(e).to_string())]),
        Some(e) => e.name.clone(),
        None => "?".into(),
    }
}

fn first_name(c: &Company, id: u32) -> String {
    if let Some(e) = c.employee(id).filter(|e| co::agency::is_booked(e.id)) {
        return trf("Agency %{n}", &[("n", &co::agency::number_of(e).to_string())]);
    }
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_else(|| "?".into())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        // (a late driver's first trips driven by a spare line: `plan::SPARE_COVER`)
        Some(Who::Staff(id)) if co::plan::spare_of_cover(id).is_some() => trf("Spare line %{d}", &[("d", &co::rosterflow::spare_label(c, id - co::plan::SPARE_COVER))]),
        Some(Who::Staff(id)) => name_of(c, id),
        Some(Who::Player) => tr("You"),
        Some(Who::Agency) => tr("Agency driver"),
        Some(Who::Guest(g)) => co::together::guest_name(c, g),
        None => tr("Nobody"),
    }
}

fn source_text(s: Source) -> &'static str {
    match s {
        Source::Roster => "fixed in the roster",
        Source::Auto => "the dispatcher's suggestion",
        Source::Dispatcher => "your choice for today",
        Source::Central => "filled by the central",
        Source::None => "",
    }
}

/// A problem's tone (`planning::problem_colour`): unplanned a warning, the rest an error.
fn problem_tone(p: Option<Problem>) -> &'static str {
    match p {
        None | Some(Problem::Unassigned) => "warn",
        Some(_) => "err",
    }
}

fn line_title(t: &DayTour, duty: Option<usize>) -> String {
    let mut s = format!("{} {}  ·  {} {}", tr("Line"), t.tour.number, tr("Tour"), t.tour.tour);
    if let Some(k) = duty {
        s.push_str(&format!("  ·  {} {}", tr("Duty"), k + 1));
    }
    s
}

fn lack_text(c: &Company, x: &Lack) -> String {
    match x {
        Lack::Licence => tr("No licence for this bus"),
        Lack::Endorsement(e) => tr(e.lacking()),
        Lack::Type(k) => trf("No type training for the %{bus}", &[("bus", &licences::type_name(c, k))]),
    }
}

/// Why a driver cannot take a duty, in words (`planning::cannot_text`).
fn cannot_text(c: &Company, p: &DayPlan, x: &pl::Cannot) -> String {
    use pl::Cannot as N;
    match x {
        N::NotDriver => tr("Not a driver"),
        N::NotEmployed => tr("Not employed then"),
        N::Ill { until } => trf("Ill until %{date}", &[("date", &day_label(until))]),
        N::Holiday { until } => trf("On holiday until %{date}", &[("date", &day_label(until))]),
        N::WeekDone { days } => trf("Worked %{n} days this week already", &[("n", &days.to_string())]),
        N::Lacks(l) => lack_text(c, l),
        N::OnDuty { key, from, to } => {
            let mut it = key.split('|');
            let (line, tour) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            let number = p.tours.iter().find(|t| t.tour.line == line && t.tour.tour == tour).map(|t| t.tour.number.clone()).unwrap_or_else(|| line.to_string());
            trf("Drives line %{n} tour %{tour} then (%{from}–%{to})", &[("n", &number), ("tour", tour), ("from", &hhmm(*from)), ("to", &hhmm(*to))])
        }
        N::Rest { minutes } => trf("Only %{n} min between duties: %{min} min rest needed", &[("n", &minutes.to_string()), ("min", &co::staff::REST.to_string())]),
        N::NightRest { from } => trf("Night's rest after yesterday: free from %{time}", &[("time", &hhmm(*from))]),
        N::TooLong { minutes } => trf("Working day would be %{t}: %{max} at most", &[("t", &length(*minutes)), ("max", &length(co::staff::DAY_MAX))]),
    }
}

/// A way to cover an open duty in words (`planning::cover_text`).
fn cover_text(c: &Company, x: &pl::Cover) -> (String, String) {
    match x {
        pl::Cover::Colleague { id, standby, warn } => {
            let name = name_of(c, *id);
            let note = match warn.first() {
                Some(Warn::Overtime(m)) => trf("Overtime %{t}", &[("t", &length(*m))]),
                Some(Warn::Transfer(m)) => trf("%{n} min to change stops", &[("n", &m.to_string())]),
                Some(Warn::Experience) => tr("Little experience for this bus"),
                None if *standby => tr("Standby: free all day"),
                None => tr("Can take it"),
            };
            (name, note)
        }
        pl::Cover::Agency { used } => (trf("Agency driver (%{n} sent today: as many as you ask for)", &[("n", &used.to_string())]), String::new()),
        pl::Cover::Player => (tr("Drive it yourself"), String::new()),
        pl::Cover::Drop { penalty } if *penalty > 0 => (tr("Drop its trips today"), trf("Penalty %{amount}", &[("amount", &eur(*penalty))])),
        pl::Cover::Drop { .. } => (tr("Drop its trips today"), tr("No penalty")),
    }
}

/// The ways to run a tour without its bus today (`planning::tour_covers`).
fn tour_covers(c: &Company, p: &DayPlan, ti: usize) -> Vec<(String, String, Option<Fill>)> {
    let t = &p.tours[ti];
    let mut out = Vec::new();
    for id in pl::spare_buses(c, p, ti) {
        let Some(v) = c.vehicle(id) else { continue };
        let note = if t.tour.wants().is_some_and(|w| w != v.kind.size) { tr("Another size than the tour asks for") } else { tr("Free then") };
        out.push((trf("Spare bus %{bus}", &[("bus", &format!("{} {}", v.number, v.name))]), note, Some(Fill::Bus { id })));
    }
    let (rent, ok) = pl::rental_for(c, p, ti);
    if ok {
        out.push((trf("Rent a bus for today (%{amount})", &[("amount", &eur(rent))]), String::new(), Some(Fill::Rental)));
    } else {
        out.push((trf("No rental bus: no place in the depot or not the cash (%{amount})", &[("amount", &eur(rent))]), String::new(), None));
    }
    let penalty = pl::drop_penalty(c, &t.tour, 0, t.tour.trips.len());
    let note = if penalty > 0 { trf("Penalty %{amount}", &[("amount", &eur(penalty))]) } else { tr("No penalty") };
    out.push((tr("Drop the tour today"), note, Some(Fill::Drop)));
    out
}

fn blocks_without(p: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    p.duties_of(Who::Staff(id))
        .into_iter()
        .filter(|x| *x != skip)
        .map(|(i, k)| {
            let t = &p.tours[i];
            let d = &t.duties[k];
            Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
        })
        .collect()
}

fn block_of(t: &DayTour, k: usize) -> Block {
    let d = &t.duties[k];
    Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
}

/// The tone of a reason not to take a duty: grey for those who are away, red for the rest.
fn cannot_tone(x: &pl::Cannot) -> &'static str {
    match x {
        pl::Cannot::NotEmployed | pl::Cannot::Holiday { .. } | pl::Cannot::NotDriver => "dim",
        _ => "err",
    }
}

/// What a driver would make of a duty (`planning::fit_of`).
fn fit_of(c: &Company, p: &DayPlan, ti: usize, k: usize, id: u32) -> (String, &'static str, bool) {
    let Some(e) = c.employee(id) else { return ("?".into(), "faint", false) };
    let t = &p.tours[ti];
    let work = blocks_without(p, id, (ti, k));
    let bus = t.bus_of(c);
    match pl::can_take(e, &p.date, p.today, &bus, &work, &block_of(t, k)) {
        Err(x) => (cannot_text(c, p, &x), cannot_tone(&x), false),
        Ok(w) => match w.first() {
            Some(Warn::Overtime(m)) => (trf("Overtime %{t}", &[("t", &length(*m))]), "warn", true),
            Some(Warn::Transfer(m)) => (trf("%{n} min to change stops", &[("n", &m.to_string())]), "warn", true),
            Some(Warn::Experience) => (tr("Little experience for this bus"), "warn", true),
            None if work.is_empty() => (tr("Can take it: free all day"), "ok", true),
            None => (tr("Can take it"), "ok", true),
        },
    }
}

/// Whether giving duty `k` of tour `ti` to `who` is for today only (`give_driver`): on the
/// company's day, a duty that fell out with somebody fixed for it in the roster (the player
/// himself goes into the roster).
fn today_only(c: &Company, p: &DayPlan, ti: usize, k: usize, who: Option<Who>) -> bool {
    let t = &p.tours[ti];
    let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
    p.today && t.duties[k].problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some() && who != Some(Who::Player)
}

/// An agency driver for the whole duty in the list of who drives it: what it costs, for today only
/// or - in the roster - each time the weekday comes. (The agency sends as many as are asked for:
/// no game mode or level keeps it away, only the cost. Drivers booked ahead are cheaper: the
/// Staff page's Agencies tab, `co::agency`.)
fn agency_state(c: &Company, p: &DayPlan, ti: usize, k: usize) -> (String, &'static str) {
    let cost = eur(pl::agency_cost(c, p, ti, k));
    if !today_only(c, p, ti, k, Some(Who::Agency)) {
        return (trf("%{amount} for this duty, every %{day}", &[("amount", &cost), ("day", &weekday_name(p.weekday))]), "dim");
    }
    (trf("%{amount} for this duty, today only (%{n} sent today)", &[("amount", &cost), ("n", &pl::agency_sent(c, p).to_string())]), "ok")
}

/// A duty's problem in words (`planning::problem_text`).
fn problem_text(c: &Company, p: &DayPlan, ti: usize, k: usize, problem: Option<Problem>) -> String {
    let t = &p.tours[ti];
    if matches!(problem, Some(Problem::Away | Problem::Conflict)) {
        let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
        if let Some(e) = rostered.and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None }) {
            let work = blocks_without(p, e.id, (ti, k));
            if let Err(x) = pl::can_take(e, &p.date, p.today, &t.bus_of(c), &work, &block_of(t, k)) {
                return format!("{}: {}", first_name(c, e.id), cannot_text(c, p, &x));
            }
        }
    }
    if matches!(problem, Some(Problem::Licence | Problem::NoEndorsement | Problem::NoTypeTraining)) {
        let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
        if let Some(e) = rostered.and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None }) {
            let (kind, bus) = t.bus_of(c);
            if let Some(x) = licences::lack(e, kind, bus.as_deref()) {
                return format!("{}: {}", first_name(c, e.id), lack_text(c, &x));
            }
        }
    }
    tr(problem.map(Problem::label).unwrap_or("No driver"))
}

/// Where a bus is on the plan's day besides this tour (`planning::bus_fit`).
fn bus_fit(c: &Company, p: &DayPlan, ti: usize, id: u32) -> (String, &'static str, bool) {
    let Some(v) = c.vehicle(id) else { return ("?".into(), "faint", false) };
    if !v.held_on(&p.date) {
        return (tr("Gone back then"), "faint", false);
    }
    if v.in_workshop(&p.date) || v.condition < 20.0 {
        return (tr("In the workshop"), "err", false);
    }
    let t = &p.tours[ti];
    if !co::ownline::line_allows(c, &t.tour.line, v) {
        return (tr("Not one of the line's buses"), "err", false);
    }
    let (a, b) = (t.tour.from(), t.tour.to());
    let other = p.tours.iter().enumerate().find(|(i, x)| *i != ti && x.bus == Some(BusOf::Own(id)) && a < x.tour.to() + co::staff::BUS_MARGIN && x.tour.from() < b + co::staff::BUS_MARGIN);
    if let Some((_, x)) = other {
        return (trf("On tour %{t} then", &[("t", &format!("{}/{}", x.tour.number, x.tour.tour))]), "warn", false);
    }
    if t.tour.wants().is_some_and(|w| w != v.kind.size) {
        return (tr("Another size than the tour asks for"), "warn", true);
    }
    (tr("Free"), "ok", true)
}

fn weekday_name(d: u8) -> String {
    const NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    tr(NAMES[d as usize % 7])
}

/// Weekdays in words: "Tuesday to Friday" (three or more in a row), else "Monday, Thursday".
fn weekdays_text(days: &[u8]) -> String {
    let in_row = days.windows(2).all(|w| w[1] == w[0] + 1);
    match days {
        [a, .., b] if days.len() >= 3 && in_row => trf("%{from} to %{to}", &[("from", &weekday_name(*a)), ("to", &weekday_name(*b))]),
        _ => days.iter().map(|d| weekday_name(*d)).collect::<Vec<_>>().join(", "),
    }
}

/// What "Fill the roster" left open, in words ("" when nothing).
fn open_text(f: &pl::Filled) -> String {
    let mut parts = Vec::new();
    let open = f.unqualified + f.busy;
    if open > 0 {
        let why = match (f.unqualified, f.busy) {
            (_, 0) => tr("nobody is qualified for their bus"),
            (0, _) => tr("nobody qualified is free"),
            (q, b) => trf("%{q} with nobody qualified for the bus, %{b} with nobody free", &[("q", &q.to_string()), ("b", &b.to_string())]),
        };
        parts.push(trf("%{n} duties still open: %{why}.", &[("n", &open.to_string()), ("why", &why)]));
    }
    if f.no_bus > 0 {
        parts.push(trf("%{n} tours still without a bus: none of the fleet is free for them.", &[("n", &f.no_bus.to_string())]));
    }
    parts.join(" ")
}

// --- the commands -----------------------------------------------------------------------------------

/// A command of the Roster tab on the session and an editor's RosterFlow state (`super::run`).
pub(super) fn run(s: &mut Session, r: &mut Rf, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_roster" => view(s, r, args),
        "rf_roster_act" => act(s, r, args),
        "rf_roster_service" => service_view(s, r, args),
        _ => return None,
    })
}

/// The roster and the day's choices as a fingerprint's text (`live`: changed under a planner).
pub(super) fn mark(c: &Company) -> String {
    serde_json::to_string(&(&c.planning.week, &c.planning.date, &c.planning.fills, given_of(c))).unwrap_or_default()
}

/// A plan of the week (`plan_of`), or of a day besides it (the service dialog's, read here once).
fn plan_any(s: &mut Session, r: &mut Rf, date: &str) -> Option<Result<DayPlan, String>> {
    company::week_work(s);
    if let Some(p) = company::plan_of(s, date) {
        return Some(p);
    }
    let c = s.company.clone()?;
    if (0..7).any(|k| co::dates::add(&c.date, k) == date) {
        return None;
    }
    let _ = r;
    let lines = core::list_lines(&c.map, date).map_err(|e| format!("{e:#}"));
    Some(lines.map(|l| {
        let tours = co::network::tours_of_day(&c, &l, date);
        pl::day_plan(&c, date, tours, &[], &[], false)
    }))
}

/// The Roster tab of day `day` (0-6), with the side panel of `sel` (`{duty: [line, tour, k]}`,
/// `{tour: [line, tour]}` or nothing: the day's).
fn view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    let day = usize_arg(args, "day").min(6);
    let date = co::dates::add(&c.date, day as i64);
    company::week_work(s);
    // the days' tabs
    let days: Vec<Value> = (0..7usize)
        .map(|k| {
            let d = co::dates::add(&c.date, k as i64);
            let (caps, big) = match co::dates::parse(&d) {
                Some(dd) => {
                    let (_, m, dn) = co::dates::civil_from_days(dd);
                    let wd = WEEKDAYS[co::dates::weekday(dd) as usize];
                    (if k == 0 { tr("Company day") } else { tr(wd) }, format!("{} {} {}", tr(wd), dn, tr(MONTHS[(m as usize).clamp(1, 12) - 1])))
                }
                None => (String::new(), d.clone()),
            };
            let status = company::plan_of(s, &d).and_then(|p| p.ok()).map(|p| {
                let (tours, _, open, no_bus) = p.counts();
                let tone = if tours == 0 { "faint" } else if open + no_bus == 0 { "ok" } else { "warn" };
                let tip = if tours == 0 { tr("No tours this day") } else if open + no_bus == 0 { tr("Every tour planned") } else { trf("%{n} open", &[("n", &(open + no_bus).to_string())]) };
                json!({ "tone": tone, "tip": tip })
            });
            json!({ "caps": caps.to_uppercase(), "big": big, "status": status })
        })
        .collect();
    // the toolbar's words: what the fleet lacks at the busiest, else what the roster is
    let wd = pl::weekday_of(&date);
    let lack = company::lines_of(s, &date).map(|lines| co::ownline::shortfall(&c, &co::network::tours_of_day(&c, &lines, &date))).unwrap_or_default();
    // (the duties drawn here are the saved ones, as the day runs them: RosterFlow's draft may cut
    // and join them otherwise, so a duty given here is not what the Drivers tab shows - said)
    let unsaved = co::rosterflow::draft_differs(&c, co::plandays::slot_of(&c, &date));
    let tools = match lack.first() {
        _ if unsaved => json!({ "text": tr("RosterFlow has changes of this day not saved: this tab and the day run the saved duties. Save them to plan with them here."), "tone": "warn", "icon": "warning" }),
        Some(sf) => json!({ "text": trf("At the busiest the tours ask for %{n} × %{bus} (or bigger); the fleet has %{have}.", &[("n", &sf.needed.to_string()), ("bus", &tr(co::BusKind { size: sf.size, drive: co::Drive::Diesel }.label())), ("have", &sf.have.to_string())]), "tone": "warn", "icon": "warning" }),
        None => json!({ "text": trf("%{day}: the roster of this weekday repeats every week.", &[("day", &day_label(&date))]), "tone": "soft", "icon": "event" }),
    };
    let repeat = if wd < 5 { tr("Repeat Mon–Fri") } else { tr("Repeat on the weekend") };
    let mut out = json!({ "date": date, "days": days, "tools": tools, "repeat": repeat });
    let p = match company::plan_of(s, &date) {
        None => {
            out["reading"] = json!(tr("Reading the timetable…"));
            return Ok(out);
        }
        Some(Err(e)) => {
            out["error"] = json!(e);
            return Ok(out);
        }
        Some(Ok(p)) => p,
    };
    out["undo"] = json!(r.roster_undo.len());
    out["redo"] = json!(r.roster_redo.len());
    out["plan"] = plan_json(&c, &p);
    // (the day's trips that ran late or were dropped, marked on the tours and duties)
    out["outcomes"] = company::late::service_json(s, &c, &date);
    out["side"] = side_json(&c, &p, args.get("sel").unwrap_or(&Value::Null));
    Ok(out)
}

/// The chart: the lines in the company's order, each with its tours and their duties.
fn plan_json(c: &Company, p: &DayPlan) -> Value {
    let t0 = p.tours.iter().map(|t| t.tour.from()).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = (p.tours.iter().map(|t| t.tour.to()).max().unwrap_or(1440) + 59).div_euclid(60) * 60;
    let mut groups: Vec<(Option<co::CompanyLine>, Vec<usize>)> = c.lines.iter().map(|x| (Some(x.clone()), Vec::new())).collect();
    for (i, t) in p.tours.iter().enumerate() {
        match groups.iter_mut().find(|g| g.0.as_ref().is_some_and(|x| x.name.eq_ignore_ascii_case(&t.tour.line))) {
            Some(g) => g.1.push(i),
            None => groups.push((None, vec![i])),
        }
    }
    let groups_json: Vec<Value> = groups
        .iter()
        .map(|(line, idx)| {
            let name = line.as_ref().map(|cl| cl.name.clone()).unwrap_or_else(|| p.tours[idx[0]].tour.line.clone());
            let none = idx.is_empty();
            let idle = !none && idx.iter().all(|&i| p.tours[i].tour.unplanned);
            let covered = idx.iter().filter(|&&i| p.tours[i].covered()).count();
            let open: usize = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_some()).map(|t| t.duties.iter().filter(|d| d.open()).count()).sum();
            let no_bus = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_none()).count();
            let mut tags: Vec<Value> = Vec::new();
            if no_bus > 0 {
                tags.push(json!([trf("%{n} without a bus", &[("n", &no_bus.to_string())]), "directions_bus"]));
            }
            if open > 0 {
                tags.push(json!([trf("%{n} without a driver", &[("n", &open.to_string())]), "person"]));
            }
            let service = match line {
                _ if none => json!({ "kind": "none", "text": tr("No tours this day") }),
                Some(cl) if cl.service_from.is_none() || idle => json!({ "kind": "start", "button": tr("Start service…"), "note": trf("Not in service · %{c} of %{t} tours planned", &[("c", &covered.to_string()), ("t", &idx.len().to_string())]) }),
                Some(cl) => {
                    let since = cl.service_from.map(|s| if s <= 0 { String::new() } else { format!("{} {}", day_label(&co::clock::date_of(s)), co::clock::hhmm(s)) }).unwrap_or_default();
                    let text = if since.is_empty() { tr("In service") } else { trf("In service since %{when}", &[("when", &since)]) };
                    let cov = trf("%{c} of %{t} tours covered", &[("c", &covered.to_string()), ("t", &idx.len().to_string())]);
                    json!({ "kind": "running", "text": format!("{text}  ·  {cov}"), "all": covered == idx.len() })
                }
                None => Value::Null,
            };
            json!({
                "name": name, "idx": idx, "none": none, "idle": idle, "folded": pl::is_collapsed(c, &name),
                "line": line.as_ref().map(|cl| json!({ "name": cl.name, "number": cl.number, "caption": if cl.caption.is_empty() { cl.name.clone() } else { cl.caption.clone() }, "colour": rf_colour(c, &cl.name) })),
                "tags": tags, "service": service,
            })
        })
        .collect();
    // (each duty's row in RosterFlow's Drivers tab: "Show in Drivers")
    let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let duties = co::studio::duties(&c.planning.joined, co::plandays::slot_of(c, &p.date), &blocks);
    let rf_id = |ti: usize, k: usize| co::studio::duty_of(&duties, ti, k).and_then(|d| co::rosterflow::duty_key(&blocks, &duties[d])).map(|x| format!("D|{}/{}/{}/{}", x.line, x.tour, x.first.name, x.first.dep));
    let tours: Vec<Value> = p.tours.iter().enumerate().map(|(ti, t)| tour_json(c, p, ti, t, &rf_id)).collect();
    json!({ "t0": t0, "t1": t1.max(t0 + 60), "groups": groups_json, "tours": tours, "today": p.today, "empty": p.tours.is_empty() && c.lines.is_empty() })
}

fn rf_colour(c: &Company, line: &str) -> String {
    co::rosterflow::hex(co::rosterflow::line_colour(c, line))
}

// --- breakdowns on the way ---------------------------------------------------------------------------

/// Today's breakdowns and accidents that came already (`clock::DayScript::breaks` indices), of
/// tour `t`'s bus before the tour ended: on the company's day only (testers: the Roster tab
/// did not show the tour whose bus broke down).
fn breaks_of(c: &Company, p: &DayPlan, t: &DayTour) -> Vec<usize> {
    let Some(BusOf::Own(id)) = t.bus.filter(|_| p.today && !t.by_player) else { return Vec::new() };
    let Some(s) = c.clock.today.as_ref().filter(|s| s.date == p.date) else { return Vec::new() };
    let came = |i: usize| s.events.iter().any(|e| e.done && e.what == What::Breakdown { index: i });
    s.breaks.iter().enumerate().filter(|&(i, b)| b.vehicle == id && b.at < t.tour.to() && came(i)).map(|(i, _)| i).collect()
}

/// A breakdown in words (its inbox message): what happened, and the spare buses free or what
/// was sent; and its tone.
fn break_words(c: &Company, i: usize) -> Option<(String, &'static str)> {
    let m = ck::break_message(c, i)?;
    let (title, text) = company::message_words(&m);
    Some((format!("{title}: {text}"), if m.level == Level::Bad { "err" } else { "warn" }))
}

/// What can still be sent for breakdown `i`: while it waits for the answer, or for trips
/// dropped - each spare bus free, a rental bus, dropping them (only while it waits).
fn break_options(c: &Company, i: usize) -> Vec<Value> {
    let Some(b) = c.clock.today.as_ref().and_then(|s| s.breaks.get(i)) else { return Vec::new() };
    if !matches!(b.choice, None | Some(Choice::Drop)) {
        return Vec::new();
    }
    let (spare_from, rental_from) = ck::arrivals(c, i);
    let mut out: Vec<Value> = ck::spare_offers(c, i)
        .iter()
        .filter_map(|x| c.vehicle(x.vehicle).map(|v| (x, v)))
        .map(|(x, v)| json!({ "index": i, "choice": Choice::Spare { vehicle: v.id }, "label": company::spare_label(&format!("{} {}", v.number, v.name), &hhmm(spare_from), x.until), "note": tr("Free now") }))
        .collect();
    out.push(json!({ "index": i, "choice": Choice::Rental, "label": trf("Rental bus, %{amount}", &[("amount", &eur(ck::rental_cost(c, i)))]), "note": trf("From %{time}", &[("time", &hhmm(rental_from))]) }));
    if b.choice.is_none() {
        out.push(json!({ "index": i, "choice": Choice::Drop, "label": tr("Drop its trips"), "note": "" }));
    }
    out
}

/// A tour's row: its bus chip and its duties on the hours.
fn tour_json(c: &Company, p: &DayPlan, ti: usize, t: &DayTour, rf_id: &dyn Fn(usize, usize) -> Option<String>) -> Value {
    let fixed = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour);
    let bus_tip = tr("The tour's bus: tap a bus in the list, then here - or drag it here. Tap it twice: the line's timetable");
    let bus = if t.by_player {
        json!({ "kind": "player", "text": tr("You drove it.") })
    } else {
        match t.bus {
            None => {
                let why = t.bus_problem.map(|x| tr(x.label())).unwrap_or_default();
                json!({ "kind": "none", "tone": problem_tone(t.bus_problem), "icon": if t.bus_problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" }, "text": tr("No bus"), "tip": if why.is_empty() { bus_tip.clone() } else { format!("{why}\n{bus_tip}") } })
            }
            Some(b) => {
                let (text, suggested) = match b {
                    BusOf::Own(id) => (c.vehicle(id).map(|x| format!("{}  {}", x.number, x.name)).unwrap_or_default(), t.bus_from == Source::Auto),
                    BusOf::Rental => (tr("Rental bus"), false),
                };
                json!({ "kind": "bus", "text": text, "suggested": suggested, "problem": t.bus_problem.map(|x| problem_tone(Some(x))), "tip": bus_tip, "id": if let BusOf::Own(id) = b { Some(id) } else { None } })
            }
        }
    };
    // (what can be taken off: what runs, or what the roster fixed)
    let mut bus = bus;
    bus["off"] = json!(!t.by_player && (t.bus.is_some() || fixed.is_some_and(|r| r.bus.is_some())));
    // (its bus broke down or crashed today: marked, the words under the mouse)
    if let Some((words, tone)) = breaks_of(c, p, t).last().and_then(|&i| break_words(c, i)) {
        let tip = bus["tip"].as_str().unwrap_or("").to_string();
        bus["problem"] = json!(tone);
        bus["broke"] = json!(true);
        bus["tip"] = json!(format!("{words}
{tip}"));
    }
    let duties: Vec<Value> = t
        .duties
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let times = format!("{} – {}", hhmm(d.from), hhmm(d.to));
            let open = d.who.is_none() && !t.by_player;
            let faint = if t.bus.is_none() && !t.by_player { 0.5 } else { 1.0 };
            let mut v = if open {
                let words = problem_text(c, p, ti, k, d.problem);
                json!({ "open": true, "tone": problem_tone(d.problem), "icon": if d.problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" }, "words": words, "full": format!("{words}  {times}") })
            } else {
                let (fill, ink, label, short, alpha) = if t.by_player {
                    ("accent".to_string(), "#ffffff", tr("You"), String::new(), 0.6)
                } else {
                    match d.who {
                        Some(Who::Staff(id)) => {
                            let full = name_of(c, id);
                            let initials: String = full.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
                            (driver_colour(c, id), "#0e121c", first_name(c, id), initials, if d.from_ == Source::Auto { 0.6 } else { 1.0 })
                        }
                        Some(Who::Player) => ("accent".to_string(), "#ffffff", tr("You"), String::new(), 1.0),
                        Some(Who::Agency) => ("grey".to_string(), "#ffffff", tr("Agency"), String::new(), 1.0),
                        // (Beta 58: a player of "Play together")
                        Some(Who::Guest(g)) => (super::super::together::GUEST_COLOUR.to_string(), "#0e121c", co::together::guest_name(c, g), String::new(), 1.0),
                        None => ("grey".to_string(), "#ffffff", String::new(), String::new(), 1.0),
                    }
                };
                let empty: Vec<Value> = t.tour.trips[d.start..d.end].iter().filter(|x| x.empty).map(|x| json!([x.dep.max(d.from), x.arr.min(d.to)])).collect();
                json!({ "open": false, "fill": fill, "ink": ink, "alpha": alpha, "label": label, "short": short, "full": format!("{label}  {times}"), "outline": matches!(d.from_, Source::Dispatcher | Source::Central), "empty": empty })
            };
            v["from"] = json!(d.from);
            v["to"] = json!(d.to);
            v["faint"] = json!(faint);
            v["late"] = json!(d.late.map(|l| json!({ "until": l.until.min(d.to), "covered": l.cover.is_some() })));
            v["warn"] = json!(!d.warn.is_empty());
            v["tip"] = json!(format!("{times}  ·  {}", who_name(c, if t.by_player { Some(Who::Player) } else { d.who })));
            let rostered = fixed.and_then(|r| r.duties.get(k).copied().flatten());
            v["who"] = json!(if t.by_player { None } else { d.who });
            v["off"] = json!(!t.by_player && (d.who.is_some() || rostered.is_some()));
            v["rf"] = json!(rf_id(ti, k));
            v
        })
        .collect();
    json!({
        "line": t.tour.line, "tour": t.tour.tour, "number": t.tour.number, "title": format!("{} {}", tr("Tour"), t.tour.tour),
        "colour": rf_colour(c, &t.tour.line), "unplanned": t.tour.unplanned, "by_player": t.by_player, "bus": bus, "duties": duties,
    })
}

/// The side panel of what is chosen.
fn side_json(c: &Company, p: &DayPlan, sel: &Value) -> Value {
    let find = |line: &str, tour: &str| p.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour);
    if let Some(d) = sel.get("duty").and_then(Value::as_array) {
        let (line, tour, k) = (d.first().and_then(Value::as_str).unwrap_or(""), d.get(1).and_then(Value::as_str).unwrap_or(""), d.get(2).and_then(Value::as_u64).unwrap_or(0) as usize);
        if let Some(ti) = find(line, tour).filter(|&i| k < p.tours[i].duties.len()) {
            return duty_side(c, p, ti, k);
        }
    }
    if let Some(d) = sel.get("tour").and_then(Value::as_array) {
        let (line, tour) = (d.first().and_then(Value::as_str).unwrap_or(""), d.get(1).and_then(Value::as_str).unwrap_or(""));
        if let Some(ti) = find(line, tour) {
            return tour_side(c, p, ti);
        }
    }
    day_side(c, p)
}

/// Nothing chosen: the day in short, what fell out, and the drivers and buses to give.
fn day_side(c: &Company, p: &DayPlan) -> Value {
    let (tours, covered, open, no_bus) = p.counts();
    // (a duty or a bus nobody was planned for, covered today - an agency driver, a rental bus -
    // is open no more: testers saw it "still in the open duties")
    let rows: Vec<pl::OpenRow> = p.open_rows().into_iter().filter(|o| o.late || !(o.filled && o.problem == Some(Problem::Unassigned))).collect();
    let mut morning: Vec<Value> = Vec::new();
    for d in &p.disruptions {
        match *d {
            Disruption::Late { employee, minutes } => morning.push(json!([trf("%{name} comes %{n} minutes late.", &[("name", &who_name(c, Some(Who::Staff(employee)))), ("n", &minutes.to_string())]), "warn"])),
            Disruption::Breakdown { vehicle, cost } => morning.push(json!([trf("Bus %{n} did not start: towed to the workshop (%{amount}).", &[("n", &c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default()), ("amount", &eur(cost))]), "err"])),
        }
    }
    // (the buses that broke down or crashed on the way today, each to its tour)
    let mut on_way: Vec<Value> = Vec::new();
    for (ti, t) in p.tours.iter().enumerate() {
        for i in breaks_of(c, p, t) {
            if on_way.iter().any(|x| x["index"] == json!(i)) {
                continue;
            }
            if let Some((words, tone)) = break_words(c, i) {
                on_way.push(json!({ "index": i, "text": words, "tone": tone, "tour": ti, "line": t.tour.line, "name": t.tour.tour }));
            }
        }
    }
    // (the agency drivers booked for the day too, after the staff: what they are paid for at least)
    let staff: Vec<Value> = c
        .staff
        .iter()
        .filter(|e| co::staff::drives(e))
        .chain(co::agency::drivers_on(c, &p.date))
        .map(|e| {
            let mins: i32 = p.duties_of(Who::Staff(e.id)).iter().map(|&(i, k)| p.tours[i].duties[k].to - p.tours[i].duties[k].from).sum();
            let (state, tone) = if co::agency::is_booked(e.id) {
                let hours = match co::agency::window(c, e.id) {
                    Some((a, z)) => format!("{}–{}", hhmm(a), hhmm(z)),
                    None => length(co::agency::minutes_booked(c, e.id)),
                };
                (trf("Booked %{hours}  ·  %{driven} driven", &[("hours", &hours), ("driven", &length(mins))]), if mins == 0 { "warn" } else { "soft" })
            } else if let Some(x) = pl::day_unavailable(e, &p.date, p.today) {
                let tone = match x {
                    pl::Cannot::Ill { .. } | pl::Cannot::WeekDone { .. } => "err",
                    pl::Cannot::NotEmployed => "faint",
                    _ => "dim",
                };
                (cannot_text(c, p, &x), tone)
            } else if mins == 0 {
                match pl::free_from(e, p.today) {
                    Some(m) => (trf("Free from %{time} (night's rest)", &[("time", &hhmm(m))]), "warn"),
                    None => (tr("Free all day"), "ok"),
                }
            } else {
                (length(mins), if mins > co::staff::DAY_TARGET { "warn" } else { "soft" })
            };
            json!({ "id": e.id, "name": name_of(c, e.id), "colour": driver_colour(c, e.id), "first": first_name(c, e.id), "state": state, "tone": tone })
        })
        .collect();
    let buses: Vec<Value> = c
        .fleet
        .iter()
        .map(|v| {
            let n = p.tours.iter().filter(|t| t.bus == Some(BusOf::Own(v.id))).count();
            let (state, tone) = if !v.held_on(&p.date) {
                (tr("Gone back then"), "faint")
            } else if v.in_workshop(&p.date) || p.disruptions.iter().any(|d| matches!(d, Disruption::Breakdown { vehicle, .. } if *vehicle == v.id)) {
                (tr("In the workshop"), "err")
            } else if n == 0 {
                (tr("Free"), "dim")
            } else {
                (trf("%{n} tours", &[("n", &n.to_string())]), "soft")
            };
            json!({ "id": v.id, "name": format!("{}  {}", v.number, v.name), "number": v.number, "state": state, "tone": tone })
        })
        .collect();
    let open_rows: Vec<Value> = rows
        .iter()
        .map(|o| {
            let t = &p.tours[o.tour];
            let what = match o.duty {
                _ if o.late => tr("Late start"),
                Some(k) => problem_text(c, p, o.tour, k, o.problem),
                None => tr(o.problem.map(Problem::label).unwrap_or("No driver")),
            };
            let fill = if o.filled {
                match (o.duty, o.late) {
                    (None, _) => match t.bus {
                        Some(BusOf::Rental) => tr("Rental bus"),
                        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(),
                        None => String::new(),
                    },
                    (Some(k), true) => t.duties[k].late.and_then(|x| x.cover).map(|id| who_name(c, Some(if id == co::plan::AGENCY { Who::Agency } else { Who::Staff(id) }))).unwrap_or_default(),
                    (Some(k), false) => who_name(c, t.duties[k].who),
                }
            } else {
                tr("Trips dropped")
            };
            let hard = o.problem.is_some_and(|x| x != Problem::Unassigned);
            json!({ "tour": o.tour, "duty": o.duty, "line": t.tour.line, "name": t.tour.tour, "title": format!("{}  ·  {}", line_title(t, o.duty), what), "fill": format!("→ {fill}"), "tone": if o.filled { "soft" } else { problem_tone(o.problem) }, "hard": hard })
        })
        .collect();
    let late_dropped = rows.iter().filter(|o| o.late && !o.filled).count();
    let covered_today = rows.iter().filter(|o| !o.late && o.filled && o.problem.is_some_and(|x| x != Problem::Unassigned)).count();
    let mut gaps = if open + no_bus == 0 { tr("Nothing open.") } else { trf("%{d} duties without a driver, %{b} tours without a bus.", &[("d", &open.to_string()), ("b", &no_bus.to_string())]) };
    if late_dropped > 0 {
        gaps = format!("{gaps} {}", trf("%{n} late starts: their first trips dropped.", &[("n", &late_dropped.to_string())]));
    }
    if covered_today > 0 {
        gaps = format!("{gaps} {}", trf("%{n} covered for today only (see Open duties).", &[("n", &covered_today.to_string())]));
    }
    json!({
        "kind": "day",
        "title": tr(if p.today { "The company's day" } else { "This day" }).to_uppercase(),
        "summary": trf("%{c} of %{t} tours covered", &[("c", &covered.to_string()), ("t", &tours.to_string())]),
        "ok": open + no_bus + late_dropped == 0,
        "gaps": gaps,
        "morning": morning,
        "on_way": on_way, "on_way_caps": tr("On the way today").to_uppercase(),
        "open_rows": open_rows,
        "staff": staff,
        "guests": guest_rows(c, p),
        "buses": buses,
    })
}

/// Beta 58: the players of "Play together" online now, to give a duty of the company's day
/// (`together`): who they are, what they drive.
fn guest_rows(c: &Company, p: &DayPlan) -> Vec<Value> {
    if !p.today {
        return Vec::new();
    }
    super::super::together::online_guests(c)
        .into_iter()
        .map(|(g, name, driving)| {
            let state = match driving {
                Some(r) => trf("Playing together · drives line %{n}", &[("n", &p.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(&r.line)).map(|t| t.tour.number.clone()).unwrap_or(r.line.clone()))]),
                None => tr("Playing together · online"),
            };
            json!({ "who": Who::Guest(g), "name": name, "state": state, "colour": super::super::together::GUEST_COLOUR })
        })
        .collect()
}

/// A duty chosen: its times, who drives it and why, who else could; on the company's day what
/// to do when it is open, and "Drive this duty".
fn duty_side(c: &Company, p: &DayPlan, ti: usize, k: usize) -> Value {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    let key = pl::duty_key(&t.tour.line, &t.tour.tour, k);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let late_key = pl::late_key(&t.tour.line, &t.tour.tour, k);
    let late_fill = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == late_key);
    let first = &t.tour.trips[d.start];
    let last = &t.tour.trips[d.end - 1];
    let mut facts: Vec<Value> = vec![json!([format!("{} – {}  ·  {}  ·  {} {}", hhmm(d.from), hhmm(d.to), length(d.to - d.from), d.end - d.start, tr("trips")), "soft"]), json!([format!("{} → {}", first.from, last.to), "dim"])];
    let who = if t.by_player { tr("You drove it.") } else { format!("{}: {}", tr("Driven by"), who_name(c, d.who)) };
    facts.push(json!([who, "text"]));
    if !source_text(d.from_).is_empty() && d.who.is_some() {
        facts.push(json!([tr(source_text(d.from_)), "dim"]));
    }
    if let Some(pb) = d.problem {
        facts.push(json!([format!("{}: {}", tr("Open because"), tr(pb.label())), problem_tone(Some(pb))]));
    }
    for w in &d.warn {
        let s = match w {
            Warn::Overtime(m) => trf("Overtime %{t}", &[("t", &length(*m))]),
            Warn::Transfer(m) => trf("%{n} min to change stops", &[("n", &m.to_string())]),
            Warn::Experience => tr("Little experience for this bus"),
        };
        facts.push(json!([s, "warn"]));
    }
    if let Some(late) = d.late {
        let s = match late.cover {
            Some(id) => trf("The driver is late until %{t}: %{name} drives the first trips.", &[("t", &hhmm(late.until)), ("name", &who_name(c, Some(if id == co::plan::AGENCY { Who::Agency } else { Who::Staff(id) })))]),
            None => trf("The driver is late until %{t}: the trips before are dropped.", &[("t", &hhmm(late.until))]),
        };
        facts.push(json!([s, if late.cover.is_some() { "warn" } else { "err" }]));
    }
    let open_today = p.today && !t.by_player && (d.problem.is_some() || d.who.is_none());
    let chosen = c.planning.fills.iter().find(|f| c.planning.date == c.date && f.key == key).map(|f| f.fill);
    let mut options: Vec<Value> = Vec::new();
    if open_today {
        for cv in pl::covers_of_duty(c, p, ti, k) {
            let (label, mut note) = cover_text(c, &cv);
            // (the agency's driver for the whole duty: what he costs)
            if matches!(cv, pl::Cover::Agency { .. }) {
                note = eur(pl::agency_cost(c, p, ti, k));
            }
            let fill = cv.fill();
            options.push(json!({ "label": label, "note": note, "fill": fill, "key": key, "on": chosen == Some(fill) }));
        }
        if fill_now {
            options.push(json!({ "label": tr("Leave it to the central"), "note": "", "fill": null, "key": key, "on": false }));
        }
    }
    if p.today && !t.by_player && d.late.is_some() {
        options.push(json!({ "label": tr("Drop the first trips"), "note": "", "fill": Fill::Drop, "key": late_key, "on": false }));
        if late_fill {
            options.push(json!({ "label": tr("Let the central cover the first trips"), "note": "", "fill": null, "key": late_key, "on": false }));
        }
    }
    let current = if t.by_player { Some(Who::Player) } else { d.who };
    let mut cands: Vec<Value> = vec![json!({ "who": Who::Player, "name": tr("You"), "state": tr("Drive it yourself"), "tone": "dim", "swatch": "accent", "on": current == Some(Who::Player) })];
    // (an agency driver for the whole duty, with what it costs: testers, "no option to use agency")
    let (state, tone) = agency_state(c, p, ti, k);
    cands.push(json!({ "who": Who::Agency, "name": tr("Agency driver"), "state": state, "tone": tone, "swatch": "grey", "on": current == Some(Who::Agency) }));
    // (Beta 58: the players of "Play together" online, for today - and the one who has it)
    if p.today && matches!(t.bus, Some(BusOf::Own(_))) {
        let mut guests = guest_rows(c, p);
        if let Some(Who::Guest(g)) = current.filter(|w| !guests.iter().any(|x| x["who"] == json!(w))) {
            guests.push(json!({ "who": Who::Guest(g), "name": co::together::guest_name(c, g), "state": tr("Playing together · offline"), "colour": super::super::together::GUEST_COLOUR }));
        }
        for x in guests {
            let on = x["who"] == json!(current);
            cands.push(json!({ "who": x["who"], "name": x["name"], "state": x["state"], "tone": "ok", "swatch": x["colour"], "on": on }));
        }
    }
    for x in pl::candidates(c, p, ti, k) {
        let (st, tone, _) = fit_of(c, p, ti, k, x.id);
        let name = name_of(c, x.id);
        cands.push(json!({ "who": Who::Staff(x.id), "name": name, "state": st, "tone": tone, "swatch": driver_colour(c, x.id), "on": current == Some(Who::Staff(x.id)) }));
    }
    cands.push(json!({ "who": null, "name": tr("Nobody"), "state": "", "tone": "dim", "swatch": null, "on": false }));
    // (one plain way to take its driver off, on top: testers found it "a lot harder now")
    let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
    let off = (!t.by_player && (d.who.is_some() || rostered.is_some())).then(|| trf("Unassign %{who}", &[("who", &who_name(c, d.who.or(rostered)))]));
    json!({
        "kind": "duty", "title": tr("Duty").to_uppercase(), "ti": ti, "k": k, "line": t.tour.line, "tour": t.tour.tour,
        "colour": rf_colour(c, &t.tour.line), "head": line_title(t, Some(k)), "facts": facts, "off": off,
        "options": options, "cands": cands, "by_player": t.by_player,
        "drive": (p.today && !t.by_player).then(|| co::duty_launch::DutyRef::of(p, ti, k)).flatten(),
    })
}

/// A tour chosen: its bus and which others could run it; on the company's day a rental bus or
/// dropping it.
fn tour_side(c: &Company, p: &DayPlan, ti: usize) -> Value {
    let t = &p.tours[ti];
    let mut facts: Vec<Value> = vec![json!([format!("{} – {}  ·  {} km  ·  {} {}", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km().round(), t.duties.len(), tr("duties")), "soft"])];
    let bus = match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        Some(BusOf::Rental) => tr("Rental bus"),
        None => tr("No bus"),
    };
    facts.push(json!([format!("{}: {}", tr("Bus"), bus), if t.bus.is_none() && !t.by_player { "warn" } else { "text" }]));
    if t.bus.is_some() && !source_text(t.bus_from).is_empty() {
        facts.push(json!([tr(source_text(t.bus_from)), "dim"]));
    }
    if let Some(pb) = t.bus_problem {
        facts.push(json!([format!("{}: {}", tr("Open because"), tr(pb.label())), problem_tone(Some(pb))]));
    }
    if let Some(w) = t.tour.wants() {
        facts.push(json!([trf("The tour asks for: %{kind}", &[("kind", &tr(co::BusKind { size: w, drive: co::Drive::Diesel }.label()))]), "dim"]));
    }
    // (its bus broke down or crashed today: what happened, and what can still be sent)
    let mut sends: Vec<Value> = Vec::new();
    for i in breaks_of(c, p, t) {
        if let Some((words, tone)) = break_words(c, i) {
            facts.push(json!([words, tone]));
        }
        sends.extend(break_options(c, i));
    }
    let key = pl::tour_key(&t.tour.line, &t.tour.tour);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let chosen = c.planning.fills.iter().find(|f| c.planning.date == c.date && f.key == key).map(|f| f.fill);
    let mut options: Vec<Value> = Vec::new();
    if p.today && !t.by_player && (t.bus.is_none() || t.bus_problem.is_some()) {
        for (label, note, fill) in tour_covers(c, p, ti) {
            options.push(json!({ "label": label, "note": note, "fill": fill, "key": key, "on": fill.is_some() && chosen == fill, "live": fill.is_some() }));
        }
        if fill_now {
            options.push(json!({ "label": tr("Leave it to the central"), "note": "", "fill": null, "key": key, "on": false, "live": true }));
        }
    }
    let mut cands: Vec<(bool, Value)> = c
        .fleet
        .iter()
        .map(|v| {
            let (st, tone, ok) = bus_fit(c, p, ti, v.id);
            (ok, json!({ "bus": v.id, "name": format!("{}  {}", v.number, v.name), "state": st, "tone": tone, "on": t.bus == Some(BusOf::Own(v.id)) }))
        })
        .collect();
    cands.sort_by(|a, b| b.0.cmp(&a.0));
    let mut cands: Vec<Value> = cands.into_iter().map(|x| x.1).collect();
    cands.push(json!({ "bus": null, "name": tr("No bus"), "state": "", "tone": "dim", "on": false }));
    // (its bus taken off in one plain step, on top)
    let fixed = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.bus);
    let off = (!t.by_player && (t.bus.is_some() || fixed.is_some())).then(|| match t.bus {
        Some(BusOf::Rental) => tr("Cancel the rental bus"),
        Some(BusOf::Own(id)) => trf("Unassign bus %{bus}", &[("bus", &c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default())]),
        None => trf("Unassign bus %{bus}", &[("bus", &fixed.and_then(|id| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default())]),
    });
    json!({
        "kind": "tour", "title": tr("Tour").to_uppercase(), "ti": ti, "line": t.tour.line, "tour": t.tour.tour,
        "colour": rf_colour(c, &t.tour.line), "head": line_title(t, None), "facts": facts, "options": options, "off": off,
        "sends": sends, "sends_caps": tr("Send a bus for it").to_uppercase(),
        "cands": if t.by_player { Vec::new() } else { cands }, "by_player": t.by_player,
    })
}

// --- giving -------------------------------------------------------------------------------------------

/// Give a duty its driver: into the roster, or - one that fell out today with somebody fixed
/// for it - for today only (`planning::give_driver`). The pieces joined with it in the studio
/// go to the same driver.
fn give_driver(s: &mut Session, p: &DayPlan, ti: usize, k: usize, who: Option<Who>) -> Value {
    // (Beta 58: to a player of "Play together", for today; one who had it gives it up)
    if let Some(Who::Guest(g)) = who {
        return super::super::together::give_to_guest(s, p, ti, k, g);
    }
    if let (Some(Who::Guest(_)), Some(r)) = (p.tours[ti].duties[k].who, co::duty_launch::DutyRef::of(p, ti, k)) {
        if let Err(p) = company::act(s, |c| co::together::book::release(c, &r, None).map(|_| ())) {
            return json!({ "popup": p });
        }
    }
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let partners = co::studio::partners(c, co::plandays::slot_of(c, &p.date), &blocks, ti, k);
    let pieces: Vec<co::studio::PieceRef> = std::iter::once((ti, k)).chain(partners.iter().copied()).filter_map(|(i, m)| co::studio::piece_ref(&blocks, i, m)).collect();
    let date = p.date.clone();
    // (an agency driver booked for the day: the duty given him for his day only - `co::agency`)
    if let Some(Who::Staff(id)) = who.filter(|w| matches!(w, Who::Staff(id) if co::agency::is_booked(*id))) {
        return match company::act(s, |c| {
            for piece in &pieces {
                co::agency::give(c, id, piece.clone());
            }
            Ok(())
        }) {
            Ok(()) => json!({}),
            Err(p) => json!({ "popup": p }),
        };
    }
    let for_today = today_only(c, p, ti, k, who);
    let wd = p.weekday;
    let r = if for_today {
        let fill = match who {
            Some(Who::Staff(id)) => Some(Fill::Colleague { id }),
            Some(Who::Agency) => Some(Fill::Agency),
            _ => Some(Fill::Drop),
        };
        let key = pl::duty_key(&line, &tour, k);
        company::act(s, |c| {
            // (a booked agency driver who had it gives it back)
            for piece in &pieces {
                co::agency::release(c, &date, piece);
            }
            pl::set_fill(c, &key, fill);
            Ok(())
        })
    } else {
        let others: Vec<(String, String, usize)> = partners.iter().map(|&(i, k)| (p.tours[i].tour.line.clone(), p.tours[i].tour.tour.clone(), k)).collect();
        company::act(s, |c| {
            for piece in &pieces {
                co::agency::release(c, &date, piece);
            }
            pl::set_driver(c, wd, &line, &tour, k, who);
            for (line, tour, k) in &others {
                pl::set_driver(c, wd, line, tour, *k, who);
            }
            Ok(())
        })
    };
    match r {
        Ok(()) => json!({}),
        Err(p) => json!({ "popup": p }),
    }
}

/// Give a tour its bus (`planning::give_bus`): a line that asks for buses of its own runs with
/// none other.
fn give_bus(s: &mut Session, p: &DayPlan, ti: usize, bus: Option<u32>) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    if let Some((v, want)) = bus.and_then(|id| c.vehicle(id)).zip(co::ownline::vehicles_of(c, &line)).filter(|(v, _)| !co::ownline::line_allows(c, &line, v)) {
        let text = trf("Line %{n} runs with %{buses}: bus %{bus} is none of them.", &[("n", &t.tour.number), ("buses", &want.summary(&|s| tr(s))), ("bus", &v.number)]);
        let p = company::popup(s, Popup::new("directions_bus", "Not one of the line's buses", text, tr("Buy or lease one of them, or choose other buses for the line in the line editor."), None));
        return json!({ "popup": p });
    }
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.bus);
    let today_only = p.today && t.bus_problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some();
    let wd = p.weekday;
    let r = if today_only {
        let fill = Some(bus.map(|id| Fill::Bus { id }).unwrap_or(Fill::Drop));
        let key = pl::tour_key(&line, &tour);
        company::act(s, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        })
    } else {
        company::act(s, |c| {
            pl::set_bus(c, wd, &line, &tour, bus);
            Ok(())
        })
    };
    match r {
        Ok(()) => json!({}),
        Err(p) => json!({ "popup": p }),
    }
}

fn said(text: impl Into<String>, error: bool) -> Value {
    json!({ "say": { "text": text.into(), "error": error } })
}

// --- undo and redo ------------------------------------------------------------------------------------

/// The roster and the day's choices as they were before a change of the Roster tab, and the
/// change in words (its undo and redo).
#[derive(Clone)]
pub(crate) struct Kept {
    week: Vec<pl::RosterTour>,
    date: String,
    fills: Vec<pl::FillChoice>,
    /// The duties given to the agency drivers booked ahead (their bookings themselves are the
    /// Staff page's).
    given: Vec<(u32, Vec<co::studio::PieceRef>)>,
    label: String,
}

impl Kept {
    fn of(c: &Company, label: String) -> Kept {
        Kept { week: c.planning.week.clone(), date: c.planning.date.clone(), fills: c.planning.fills.clone(), given: given_of(c), label }
    }

    fn same(&self, c: &Company) -> bool {
        self.week == c.planning.week && self.date == c.planning.date && self.fills == c.planning.fills && self.given == given_of(c)
    }

    fn put(&self, c: &mut Company) {
        c.planning.week = self.week.clone();
        c.planning.date = self.date.clone();
        c.planning.fills = self.fills.clone();
        for b in c.planning.agency.iter_mut() {
            for d in b.drivers.iter_mut() {
                if let Some(g) = self.given.iter().find(|g| g.0 == d.employee.id) {
                    d.duties = g.1.clone();
                }
            }
        }
    }
}

/// The duties given to each booked agency driver.
fn given_of(c: &Company) -> Vec<(u32, Vec<co::studio::PieceRef>)> {
    c.planning.agency.iter().flat_map(|b| b.drivers.iter().map(|d| (d.employee.id, d.duties.clone()))).collect()
}

/// Undo the Roster tab's last change (`back`), or redo the last undone.
fn undo(s: &mut Session, r: &mut Rf, back: bool) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let item = if back { r.roster_undo.pop() } else { r.roster_redo.pop() };
    let Some(k) = item else { return json!({}) };
    let now = Kept::of(c, k.label.clone());
    if back {
        r.roster_redo.push(now);
    } else {
        r.roster_undo.push(now);
    }
    let _ = company::act(s, |c| {
        k.put(c);
        Ok(())
    });
    let words = if back { tr("Undone: %{edit}") } else { tr("Redone: %{edit}") };
    said(words.replace("%{edit}", &k.label), false)
}

/// When the company day's late drivers arrive, kept before its roster is first changed here
/// (`pl::pin_arrivals`; the morning keeps them too): moved to a duty that begins after it, a
/// driver is not late for it.
fn pin_arrivals(s: &mut Session) {
    let Some(date) = s.company.as_ref().map(|c| c.date.clone()) else { return };
    let Some(Ok(p)) = company::plan_of(s, &date) else { return };
    if s.company.as_ref().is_some_and(|c| !pl::arrivals_due(c, &p).is_empty()) {
        let _ = company::act(s, |c| Ok(pl::pin_arrivals(c, &p)));
    }
}

/// What a click on the Roster tab does (`act_on`), each change of the roster kept for undo.
fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    if what == "undo" || what == "redo" {
        return Ok(undo(s, r, what == "undo"));
    }
    pin_arrivals(s);
    let before = s.company.as_ref().map(|c| Kept::of(c, String::new()));
    let out = act_on(s, r, args)?;
    if let (Some(mut k), Some(c)) = (before, s.company.as_ref()) {
        if !k.same(c) {
            k.label = out.pointer("/say/text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| tr("The roster changed"));
            r.roster_undo.push(k);
            if r.roster_undo.len() > 60 {
                r.roster_undo.remove(0);
            }
            r.roster_redo.clear();
        }
    }
    Ok(out)
}

// --- taking off -----------------------------------------------------------------------------------------

/// A tour of the plan by its line and name.
fn find(p: &DayPlan, line: &str, tour: &str) -> Option<usize> {
    p.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour)
}

/// What was taken off, in words: a driver of a duty, a tour's bus (`co::unassign`).
fn off_words(p: &DayPlan, o: &un::Off, ti: usize, k: Option<usize>, gone: &str, day: &str) -> String {
    let t = &p.tours[ti];
    let (n, tour) = (t.tour.number.clone(), t.tour.tour.clone());
    let kk = k.map(|k| (k + 1).to_string()).unwrap_or_default();
    match (o, k) {
        (un::Off::Roster, Some(_)) => trf("%{who} taken off line %{n}, tour %{tour}, duty %{k}: nobody for it in the roster of %{day}.", &[("who", gone), ("n", &n), ("tour", &tour), ("k", &kk), ("day", day)]),
        (un::Off::Choice, Some(_)) => trf("Your choice for today taken back: line %{n}, tour %{tour}, duty %{k} is open again.", &[("n", &n), ("tour", &tour), ("k", &kk)]),
        (un::Off::Dropped { penalty, .. }, Some(_)) => trf("Line %{n}, tour %{tour}, duty %{k} without its driver today: its trips are dropped (penalty %{amount}).", &[("n", &n), ("tour", &tour), ("k", &kk), ("amount", &eur(*penalty))]),
        (un::Off::Roster, None) => trf("Bus %{bus} taken off line %{n}, tour %{tour}: no bus for it in the roster of %{day}.", &[("bus", gone), ("n", &n), ("tour", &tour), ("day", day)]),
        (un::Off::Choice, None) => trf("Your bus for today taken back: line %{n}, tour %{tour} has no bus again.", &[("n", &n), ("tour", &tour)]),
        (un::Off::Rental { rent }, _) => trf("The rental bus of line %{n}, tour %{tour} is cancelled: its %{amount} for the day are not paid.", &[("n", &n), ("tour", &tour), ("amount", &eur(*rent))]),
        (un::Off::Dropped { penalty, rent }, None) => {
            let mut s = trf("Line %{n}, tour %{tour} without its bus today: its trips are dropped (penalty %{amount}).", &[("n", &n), ("tour", &tour), ("amount", &eur(*penalty))]);
            if *rent > 0 {
                s = format!("{s} {}", trf("The rental bus is cancelled: %{amount} not paid.", &[("amount", &eur(*rent))]));
            }
            s
        }
    }
}

/// What taking several off did, in words.
fn many_words(m: &un::Many, head: String) -> Value {
    if m.done.is_empty() && m.stayed.is_empty() {
        return said(tr("Nothing to take off."), false);
    }
    let mut s = head;
    let (penalty, rent) = m.money();
    if penalty > 0 {
        s = format!("{s} {}", trf("Trips dropped today: penalty %{amount}.", &[("amount", &eur(penalty))]));
    }
    if rent > 0 {
        s = format!("{s} {}", trf("Rental buses cancelled: %{amount} not paid.", &[("amount", &eur(rent))]));
    }
    if !m.stayed.is_empty() {
        s = format!("{s} {}", m.stayed.iter().map(|w| tr(w)).collect::<Vec<_>>().join(" "));
    }
    said(s.trim(), m.done.is_empty())
}

/// Take off a duty's driver or a tour's bus, what is chosen, or all of a driver's or a bus's of
/// the day (`co::unassign`); what stays says why.
fn off(s: &mut Session, p: &DayPlan, what: &str, args: &Value, day: &str) -> Value {
    let Some(c) = s.company.clone() else { return json!({}) };
    let (line, tour) = (str_arg(args, "line"), str_arg(args, "tour"));
    // (tried on a copy first: a refusal changes nothing, and says why)
    let one = |s: &mut Session, f: &dyn Fn(&mut Company) -> Result<un::Off, &'static str>| -> Result<un::Off, Value> {
        let mut probe = c.clone();
        if let Err(why) = f(&mut probe) {
            return Err(said(tr(why), true));
        }
        company::act(s, |c| f(c)).map_err(|p| json!({ "popup": p }))
    };
    match what {
        "off_driver" => {
            let k = usize_arg(args, "k");
            let Some(ti) = find(p, &line, &tour).filter(|&i| k < p.tours[i].duties.len()) else { return said(tr(un::NOT_FOUND), true) };
            let who = p.tours[ti].duties[k].who.or_else(|| pl::roster(&c, p.weekday, &line, &tour).and_then(|r| r.duties.get(k).copied().flatten()));
            match one(s, &|c| un::driver_off(c, p, ti, k)) {
                Ok(o) => said(off_words(p, &o, ti, Some(k), &who_name(&c, who), day), false),
                Err(v) => v,
            }
        }
        "off_bus" => {
            let Some(ti) = find(p, &line, &tour) else { return said(tr(un::NOT_FOUND), true) };
            let bus = match p.tours[ti].bus {
                Some(BusOf::Own(id)) => Some(id),
                _ => pl::roster(&c, p.weekday, &line, &tour).and_then(|r| r.bus),
            };
            let name = bus.and_then(|id| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default();
            match one(s, &|c| un::bus_off(c, p, ti)) {
                Ok(o) => said(off_words(p, &o, ti, None, &name, day), false),
                Err(v) => v,
            }
        }
        "off_many" => {
            // (the chosen: `{duty: [line, tour, k]}` and `{tour: [line, tour]}`)
            let items: Vec<(usize, Option<usize>)> = args
                .get("items")
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(|x| {
                            if let Some(d) = x.get("duty").and_then(Value::as_array) {
                                let ti = find(p, d.first()?.as_str()?, d.get(1)?.as_str()?)?;
                                Some((ti, Some(d.get(2)?.as_u64()? as usize)))
                            } else {
                                let t = x.get("tour")?.as_array()?;
                                Some((find(p, t.first()?.as_str()?, t.get(1)?.as_str()?)?, None))
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut m = un::Many::default();
            let _ = company::act(s, |c| {
                m = un::many_off(c, p, &items);
                Ok(())
            });
            let head = trf("%{n} taken off.", &[("n", &m.done.len().to_string())]);
            many_words(&m, head)
        }
        "off_staff" => {
            let who: Option<Who> = serde_json::from_value(args.get("who").cloned().unwrap_or(Value::Null)).ok();
            let Some(who) = who else { return json!({}) };
            let mut m = un::Many::default();
            let _ = company::act(s, |c| {
                m = un::driver_off_all(c, p, who);
                Ok(())
            });
            let head = trf("%{who} taken off %{n} duties of %{day}.", &[("who", &who_name(&c, Some(who))), ("n", &m.done.len().to_string()), ("day", day)]);
            many_words(&m, head)
        }
        "off_vehicle" => {
            let bus = args.get("bus").and_then(Value::as_u64).map(|x| x as u32);
            let Some(bus) = bus else { return json!({}) };
            let mut m = un::Many::default();
            let _ = company::act(s, |c| {
                m = un::bus_off_all(c, p, bus);
                Ok(())
            });
            let name = c.vehicle(bus).map(|v| v.number.clone()).unwrap_or_default();
            let head = trf("Bus %{bus} taken off %{n} tours of %{day}.", &[("bus", &name), ("n", &m.done.len().to_string()), ("day", day)]);
            many_words(&m, head)
        }
        _ => json!({}),
    }
}

/// What a click on the Roster tab does (`rf_roster_act`: `act`, `day`, and what it needs).
fn act_on(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let what = str_arg(args, "act");
    let day = usize_arg(args, "day").min(6);
    let date = co::dates::add(&c.date, day as i64);
    let wd = pl::weekday_of(&date);
    let day_name = weekday_name(wd);
    match what.as_str() {
        "fold" => {
            let name = str_arg(args, "line");
            let to = args.get("folded").and_then(Value::as_bool).unwrap_or(!pl::is_collapsed(&c, &name));
            let _ = company::act(s, |c| {
                pl::set_collapsed(c, &name, to);
                Ok(())
            });
            return Ok(json!({}));
        }
        "fold_all" => {
            let names: Vec<String> = serde_json::from_value(args.get("lines").cloned().unwrap_or(Value::Null)).unwrap_or_default();
            let all = args.get("all").and_then(Value::as_bool).unwrap_or(true);
            let _ = company::act(s, |c| {
                pl::set_all_collapsed(c, &names, all);
                Ok(())
            });
            return Ok(json!({}));
        }
        "stop" => {
            let name = str_arg(args, "line");
            return Ok(match company::act(s, |c| co::network::stop_service(c, &name)) {
                Ok(()) => json!({}),
                Err(p) => json!({ "popup": p }),
            });
        }
        // (a bus sent for one that broke down or crashed today: `clock::choose`)
        "break_choose" => {
            let index = usize_arg(args, "index");
            if let Some(choice) = args.get("choice").cloned().and_then(|v| serde_json::from_value::<Choice>(v).ok()) {
                company::choose_break(s, index, choice);
            }
            return Ok(json!({}));
        }
        "set_fill" => {
            let key = str_arg(args, "key");
            let fill: Option<Fill> = serde_json::from_value(args.get("fill").cloned().unwrap_or(Value::Null)).ok();
            let _ = company::act(s, |c| {
                pl::set_fill(c, &key, fill);
                Ok(())
            });
            return Ok(json!({}));
        }
        "clear" => {
            return Ok(match company::act(s, |c| Ok(pl::clear_day(c, wd))) {
                Ok(gone) if gone.is_empty() => json!({ "popup": company::popup(s, Popup::new("delete", "Nothing to clear", trf("The roster of %{day} is empty already.", &[("day", &day_name)]), "", None)) }),
                Ok(gone) => said(trf("The day is cleared: %{d} duties and %{b} buses taken off the roster of %{day}.", &[("d", &gone.duties.to_string()), ("b", &gone.buses.to_string()), ("day", &day_name)]), false),
                Err(p) => json!({ "popup": p }),
            });
        }
        "repeat" => {
            let to: Vec<u8> = if wd < 5 { (0..5).collect() } else { vec![5, 6] };
            return Ok(match company::act(s, |c| Ok(pl::copy_day(c, wd, &to))) {
                Ok(days) if days.is_empty() => json!({ "popup": company::popup(s, Popup::new("content_copy", "Nothing to repeat", trf("The roster of %{day} is empty: there is nothing to repeat. Fill it first (\"Fill the roster\" does it for you), then repeat it on the other days.", &[("day", &day_name)]), "", None)) }),
                Ok(days) => {
                    let key = if days.len() == 1 { "The roster of %{day} is now on %{days} too." } else { "The roster of %{day} is now on %{days} too (%{n} days)." };
                    said(trf(key, &[("day", &day_name), ("days", &weekdays_text(&days)), ("n", &days.len().to_string())]), false)
                }
                Err(p) => json!({ "popup": p }),
            });
        }
        "fill" => {
            company::week_work(s);
            let Some(lines) = company::lines_of(s, &date) else { return Ok(json!({})) };
            let d = date.clone();
            return Ok(match company::act(s, |c| {
                let tours = co::network::tours_of_day(c, &lines, &d);
                Ok(pl::fill_day_told(c, &d, tours))
            }) {
                Ok(f) => {
                    let open = open_text(&f);
                    if f.duties + f.buses > 0 {
                        let done = trf("Filled: %{d} duties given to drivers, %{b} buses given.", &[("d", &f.duties.to_string()), ("b", &f.buses.to_string())]);
                        said(if open.is_empty() { done } else { format!("{done} {open}") }, false)
                    } else if open.is_empty() {
                        json!({ "popup": company::popup(s, Popup::new("event", "Nothing to fill", trf("Every tour of %{day} has its bus and its drivers already.", &[("day", &day_name)]), "", None)) })
                    } else {
                        let go = if f.unqualified > 0 { Go::Licences } else if f.busy > 0 { Go::Hire } else { Go::Dealer };
                        json!({ "popup": company::popup(s, Popup::new("event", "Nothing could be filled", open, tr("Hire drivers, train them for the buses on the Staff page, or get more buses."), Some(go))) })
                    }
                }
                Err(p) => json!({ "popup": p }),
            });
        }
        "service_start" => return Ok(service_start(s, r, args)),
        _ => {}
    }
    let p = match plan_any(s, r, &date) {
        Some(Ok(p)) => p,
        _ => return Ok(json!({})),
    };
    // (taking a driver or a bus off: `off`)
    if what.starts_with("off_") {
        return Ok(off(s, &p, &what, args, &day_name));
    }
    let ti = usize_arg(args, "ti");
    if ti >= p.tours.len() {
        return Ok(json!({}));
    }
    Ok(match what.as_str() {
        "give_driver" => {
            let k = usize_arg(args, "k");
            if k >= p.tours[ti].duties.len() || p.tours[ti].by_player {
                return Ok(json!({}));
            }
            let who: Option<Who> = serde_json::from_value(args.get("who").cloned().unwrap_or(Value::Null)).ok();
            // ("Nobody" for a duty someone drives: taken off as the cross takes it off - the day's
            // choice taken back too, which the roster alone did not)
            if who.is_none() && p.tours[ti].duties[k].who.is_some() {
                let t = &p.tours[ti].tour;
                return Ok(off(s, &p, "off_driver", &json!({ "line": t.line, "tour": t.tour, "k": k }), &day_name));
            }
            give_driver(s, &p, ti, k, who)
        }
        "give_bus" => {
            if p.tours[ti].by_player {
                return Ok(json!({}));
            }
            let bus = args.get("bus").and_then(Value::as_u64).map(|x| x as u32);
            // ("No bus" for a tour that has one: taken off as the cross takes it off - the day's
            // choice of a spare or rental bus taken back too, which the roster alone did not)
            if bus.is_none() && p.tours[ti].bus.is_some() {
                let t = &p.tours[ti].tour;
                return Ok(off(s, &p, "off_bus", &json!({ "line": t.line, "tour": t.tour }), &day_name));
            }
            give_bus(s, &p, ti, bus)
        }
        other => return Err(format!("no action {other}")),
    })
}

// --- putting a line into service --------------------------------------------------------------------

/// "Start service from" (`planning::service_dialog`): now, from tomorrow, or from a date; what
/// the next seven days of the line are planned like, and - with gaps - whether to start with
/// them all the same.
fn service_view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let line = str_arg(args, "line");
    let Some(cl) = c.lines.iter().find(|x| x.name == line).cloned() else { return Ok(json!({ "gone": true })) };
    let when = usize_arg(args, "when").min(2);
    let date = args.get("date").and_then(Value::as_str).filter(|d| co::dates::parse(d).is_some()).unwrap_or(&c.date).to_string();
    let from = start_of(&c, when, &date);
    let start_day = co::clock::date_of(from);
    let mut rows: Vec<Value> = Vec::new();
    let mut short = 0usize;
    let mut reading = false;
    for k in 0..7 {
        let day = co::dates::add(&start_day, k);
        let (text, tone) = match plan_any(s, r, &day) {
            None => {
                reading = true;
                (tr("Reading the timetable…"), "dim")
            }
            Some(Err(_)) => (tr("The timetable could not be read."), "warn"),
            Some(Ok(p)) => {
                let mine: Vec<&DayTour> = p.tours.iter().filter(|t| t.tour.line == line && co::clock::moment(&day, t.tour.from() as i64) >= from).collect();
                let covered = mine.iter().filter(|t| t.covered()).count();
                short += mine.len() - covered;
                if mine.is_empty() {
                    (tr("No tours"), "dim")
                } else {
                    (trf("%{c} of %{t} tours planned", &[("c", &covered.to_string()), ("t", &mine.len().to_string())]), share_tone(covered as f64 / mine.len() as f64, false))
                }
            }
        };
        rows.push(json!({ "day": day_label(&day), "text": text, "tone": tone }));
    }
    Ok(json!({
        "title": trf("Start the service of line %{n}", &[("n", &cl.number)]),
        "intro": tr("From the moment you choose, the line's tours run as they are planned. A tour without its bus or a driver is dropped then, with the contract's penalty - plan first, then start."),
        "whens": [tr("Now"), tr("From tomorrow"), tr("From a date")],
        "rows": rows, "reading": reading, "short": short,
        "short_text": (short > 0).then(|| trf("%{n} tours of these days have no bus or no driver yet: they would be dropped, each with its penalty.", &[("n", &short.to_string())])),
        "gaps_label": tr("Start all the same, with these gaps"),
        "date": date,
    }))
}

/// The moment the service begins: now, tomorrow's midnight, or a date's (not before now).
fn start_of(c: &Company, when: usize, date: &str) -> i64 {
    match when {
        0 => co::clock::now(c),
        1 => co::clock::moment(&co::dates::add(&c.date, 1), 0),
        _ => co::clock::moment(date, 0).max(co::clock::now(c)),
    }
}

/// "Start service": the line runs from the moment chosen, once its week is read and - with
/// gaps - the player said to start with them.
fn service_start(s: &mut Session, r: &mut Rf, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return json!({}) };
    let line = str_arg(args, "line");
    let Some(cl) = c.lines.iter().find(|x| x.name == line).cloned() else { return json!({ "close": true }) };
    let v = match service_view(s, r, args) {
        Ok(v) => v,
        Err(e) => return said(e, true),
    };
    let gaps = args.get("gaps").and_then(Value::as_bool).unwrap_or(false);
    if v["reading"].as_bool().unwrap_or(false) {
        return json!({ "popup": company::popup(s, Popup::new("timer", "Not yet", tr("The line's week is still being read."), tr("Try again in a moment."), None)) });
    }
    if v["short"].as_u64().unwrap_or(0) > 0 && !gaps {
        return json!({ "popup": company::popup(s, Popup::new("event", "Not planned yet", tr("Some of the line's tours have no bus or no driver: they would be dropped, each with the contract's penalty."), tr("Give them buses and drivers on the planning (\"Fill the roster\" does it for you) - or choose to start with the gaps."), None)) });
    }
    let when = usize_arg(args, "when").min(2);
    let date = args.get("date").and_then(Value::as_str).unwrap_or(&c.date).to_string();
    let from = start_of(&c, when, &date);
    match company::act(s, |c| co::network::start_service(c, &line, from)) {
        Ok(at) => json!({ "close": true, "say": { "text": trf("Line %{n} is in service from %{when}.", &[("n", &cl.number), ("when", &format!("{} {}", day_label(&co::clock::date_of(at)), co::clock::hhmm(at)))]), "error": false } }),
        Err(p) => json!({ "popup": p }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_say_what_they_did() {
        assert_eq!(weekdays_text(&[1, 2, 3, 4]), "Tuesday to Friday");
        assert_eq!(weekdays_text(&[0, 1, 3, 4]), "Monday, Tuesday, Thursday, Friday");
        assert_eq!(weekdays_text(&[6]), "Sunday");
        let f = pl::Filled { duties: 9, buses: 4, unqualified: 2, busy: 0, no_bus: 0 };
        assert_eq!(open_text(&f), "2 duties still open: nobody is qualified for their bus.");
        assert_eq!(open_text(&pl::Filled::default()), "");
    }
}

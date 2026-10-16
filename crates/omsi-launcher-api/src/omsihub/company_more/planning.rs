//! The planning without the RosterFlow add-on (`crates/omsi-launcher-ui/src/company/planning.rs`
//! and its Roster tab, `planning/rf_roster.rs`, drawn alone): the next seven days as tabs with
//! whether each is covered; the tools (repeat the day on the other working days or the weekend,
//! clear the day, fill the roster as the dispatcher would); the day's tours as RosterFlow's
//! Gantt, grouped by line under a heading that folds the line away, says what is open, opens its
//! timetable and starts or stops its service; and beside it the day, a duty or a tour chosen,
//! with the drivers and buses to give by tapping or dragging and each driver's reason. "Drive
//! this duty" opens the company's duty launcher with that duty. The rules are `company::plan`'s.
//!
//! What the page holds (the day shown, a duty or tour chosen, a driver or bus taken up, "Clear
//! the day" pressed once) comes with the view's arguments; the company keeps the roster and the
//! folded lines. With the add-on on, the planning is RosterFlow's (phase 4): the view says so
//! and the page hands it over.

use super::super::launch::DutyLaunch;
use super::super::lines::plate;
use super::super::people::lack_text;
use super::super::words::{day_label, eur, share_tone, tr, trf, MONTHS, WEEKDAYS};
use super::super::{act as apply, lines_of, plan_of, week_work, Session};
use super::{popup_answer, str_arg};
use crate::Answer;
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, DayTour, Disruption, Fill, Problem, Source, Warn, Who};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

// --- what the page holds ----------------------------------------------------------------------

/// A driver or a bus taken up to be given (tapped, or dragged).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Arm {
    Driver(Who),
    Bus(u32),
}

/// A duty or a tour chosen.
#[derive(Clone, PartialEq, Debug)]
enum Sel {
    Tour(String, String),
    Duty(String, String, usize),
}

/// Who as the page names them: "player", "agency", "s<id>".
fn who_of(s: &str) -> Option<Who> {
    match s {
        "player" => Some(Who::Player),
        "agency" => Some(Who::Agency),
        x if x.starts_with('g') => x.strip_prefix('g').and_then(|n| n.parse().ok()).map(Who::Guest),
        x => x.strip_prefix('s').and_then(|n| n.parse().ok()).map(Who::Staff),
    }
}

fn who_key(w: Who) -> String {
    match w {
        Who::Player => "player".into(),
        Who::Agency => "agency".into(),
        Who::Staff(id) => format!("s{id}"),
        Who::Guest(g) => format!("g{g}"),
    }
}

/// The driver or bus in hand (`arm`: `{ "driver": "<who>" }` or `{ "bus": <id> }`).
fn arm_of(args: &Value) -> Option<Arm> {
    let a = args.get("arm")?;
    if let Some(w) = a.get("driver").and_then(Value::as_str).and_then(who_of) {
        return Some(Arm::Driver(w));
    }
    a.get("bus").and_then(Value::as_u64).map(|b| Arm::Bus(b as u32))
}

/// The duty or tour chosen (`sel`: `{ "line", "tour", "k" }`, no `k` for a tour).
fn sel_of(args: &Value) -> Option<Sel> {
    let s = args.get("sel")?;
    let line = s.get("line")?.as_str()?.to_string();
    let tour = s.get("tour")?.as_str()?.to_string();
    Some(match s.get("k").and_then(Value::as_u64) {
        Some(k) => Sel::Duty(line, tour, k as usize),
        None => Sel::Tour(line, tour),
    })
}

/// The day shown (0 the company's day, up to 6) and its date.
fn day_of(c: &Company, args: &Value) -> (usize, String) {
    let day = args.get("day").and_then(Value::as_u64).unwrap_or(0).min(6) as usize;
    (day, co::dates::add(&c.date, day as i64))
}

// --- words ------------------------------------------------------------------------------------

fn hhmm(minutes: i32) -> String {
    co::clock::hhmm(minutes as i64)
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

/// The colours drivers are told apart by (calm, and readable under dark ink).
const DRIVERS: [(u8, u8, u8); 10] = [(110, 168, 230), (96, 196, 170), (196, 150, 214), (226, 184, 110), (140, 190, 120), (226, 150, 130), (150, 206, 240), (200, 200, 120), (180, 160, 230), (120, 200, 210)];

fn driver_colour(c: &Company, id: u32) -> String {
    let k = c.staff.iter().position(|e| e.id == id).unwrap_or(id as usize);
    let (r, g, b) = DRIVERS[k % DRIVERS.len()];
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A line's colour (`rf_gantt::line_colour`).
fn line_colour(c: &Company, line: &str) -> String {
    let [r, g, b] = co::rosterflow::line_colour(c, line);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn first_name(c: &Company, id: u32) -> String {
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_else(|| "?".into())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        // (a late driver's first trips driven by a spare line: `plan::SPARE_COVER`)
        Some(Who::Staff(id)) if co::plan::spare_of_cover(id).is_some() => trf("Spare line %{d}", &[("d", &co::rosterflow::spare_label(c, id - co::plan::SPARE_COVER))]),
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()),
        Some(Who::Player) => tr("You"),
        Some(Who::Agency) => tr("Agency driver"),
        Some(Who::Guest(g)) => crate::omsihub::together::guest_label(c, g),
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

/// A problem's tone (`planning::problem_colour`, in RosterFlow's palette).
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

/// A weekday's full name (0 Monday).
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
pub(in super::super) fn open_text(f: &pl::Filled) -> String {
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

/// A name's initials ("Jan Meijer": "JM").
fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

// --- the roster's rules in words ----------------------------------------------------------------

/// The blocks a driver has on the plan's day, without one duty (to see whether it fits).
fn blocks_without(p: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    p.duties_of(Who::Staff(id)).into_iter().filter(|x| *x != skip).map(|(i, k)| block_of(&p.tours[i], k)).collect()
}

fn block_of(t: &DayTour, k: usize) -> Block {
    let d = &t.duties[k];
    Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
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
            let name = c.employee(*id).map(|e| e.name.clone()).unwrap_or_default();
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

/// The tone of a reason not to take a duty: grey for those who are away, red for the rest.
fn cannot_tone(x: &pl::Cannot) -> &'static str {
    match x {
        pl::Cannot::NotEmployed | pl::Cannot::Holiday { .. } | pl::Cannot::NotDriver => "dim",
        _ => "err",
    }
}

/// What a driver would make of a duty: a word and its tone (`planning::fit_of`).
fn fit_of(c: &Company, p: &DayPlan, ti: usize, k: usize, id: u32) -> (String, &'static str) {
    let Some(e) = c.employee(id) else { return ("?".into(), "faint") };
    let t = &p.tours[ti];
    let work = blocks_without(p, id, (ti, k));
    let bus = t.bus_of(c);
    match pl::can_take(e, &p.date, p.today, &bus, &work, &block_of(t, k)) {
        Err(x) => (cannot_text(c, p, &x), cannot_tone(&x)),
        Ok(w) => match w.first() {
            Some(Warn::Overtime(m)) => (trf("Overtime %{t}", &[("t", &length(*m))]), "warn"),
            Some(Warn::Transfer(m)) => (trf("%{n} min to change stops", &[("n", &m.to_string())]), "warn"),
            Some(Warn::Experience) => (tr("Little experience for this bus"), "warn"),
            None if work.is_empty() => (tr("Can take it: free all day"), "ok"),
            None => (tr("Can take it"), "ok"),
        },
    }
}

/// A duty's problem in words; for a driver who cannot, exactly why (`planning::problem_text`).
fn problem_text(c: &Company, p: &DayPlan, ti: usize, k: usize, problem: Option<Problem>) -> String {
    let t = &p.tours[ti];
    let rostered = || pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten()).and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None });
    if matches!(problem, Some(Problem::Away | Problem::Conflict)) {
        if let Some(e) = rostered() {
            let work = blocks_without(p, e.id, (ti, k));
            if let Err(x) = pl::can_take(e, &p.date, p.today, &t.bus_of(c), &work, &block_of(t, k)) {
                return format!("{}: {}", first_name(c, e.id), cannot_text(c, p, &x));
            }
        }
    }
    // (a holiday known ahead, Beta 57: whose, and until when)
    if problem == Some(Problem::Holiday) {
        if let Some((e, until)) = rostered().and_then(|e| e.on_holiday(&p.date).map(|u| (e, u))) {
            return format!("{}: {}", first_name(c, e.id), trf("On holiday until %{date}", &[("date", &day_label(until))]));
        }
    }
    if matches!(problem, Some(Problem::Licence | Problem::NoEndorsement | Problem::NoTypeTraining)) {
        if let Some(e) = rostered() {
            let (kind, bus) = t.bus_of(c);
            if let Some(x) = co::licences::lack(e, kind, bus.as_deref()) {
                return format!("{}: {}", first_name(c, e.id), lack_text(c, &x));
            }
        }
    }
    tr(problem.map(Problem::label).unwrap_or("No driver"))
}

/// Where a bus is on the plan's day besides this tour: (word, tone, free) (`planning::bus_fit`).
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

// --- the view ---------------------------------------------------------------------------------

pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    if co::autopilot::rosterflow(&c) {
        return json!({ "rosterflow": true });
    }
    week_work(s);
    let (day, date) = day_of(&c, args);
    let tabs: Vec<Value> = (0..7).map(|k| day_tab(s, &c, k, day)).collect();
    let tools = tools(s, &c, &date, args.get("clear_armed").and_then(Value::as_bool).unwrap_or(false));
    let mut out = json!({ "day": day, "date": date, "tabs": tabs, "tools": tools });
    match plan_of(s, &date) {
        None => out["reading"] = json!(tr("Reading the timetable…")),
        Some(Err(e)) => out["error"] = json!(e),
        Some(Ok(p)) => {
            let sel = sel_of(args);
            let arm = arm_of(args);
            out["gantt"] = gantt(&c, &p, sel.as_ref(), arm);
            out["side"] = side(&c, &p, sel.as_ref(), arm);
        }
    }
    out
}

/// A day of the strip (`rf_roster::day_tabs`): its weekday in capitals, its date, whether it is
/// covered.
fn day_tab(s: &mut Session, c: &Company, k: usize, day: usize) -> Value {
    let date = co::dates::add(&c.date, k as i64);
    let Some(d) = co::dates::parse(&date) else { return json!({}) };
    let (_, m, dd) = co::dates::civil_from_days(d);
    let wd = WEEKDAYS[co::dates::weekday(d) as usize];
    let caps = if k == 0 { tr("Company day") } else { tr(wd) }.to_uppercase();
    let big = format!("{} {} {}", tr(wd), dd, tr(MONTHS[(m as usize).clamp(1, 12) - 1]));
    let status = plan_of(s, &date).and_then(|p| p.ok()).map(|p| p.counts());
    let (dot, tip) = match status {
        Some((0, ..)) => (Some("faint"), tr("No tours this day")),
        Some((_, _, open, no_bus)) if open + no_bus == 0 => (Some("ok"), tr("Every tour planned")),
        Some((_, _, open, no_bus)) => (Some("warn"), trf("%{n} open", &[("n", &(open + no_bus).to_string())])),
        None => (None, String::new()),
    };
    json!({ "caps": caps, "big": big, "on": k == day, "dot": dot, "tip": tip })
}

/// The toolbar (`rf_roster::tools`): what the roster is - or what the fleet lacks -, and the
/// tools.
fn tools(s: &Session, c: &Company, date: &str, clear_armed: bool) -> Value {
    let wd = pl::weekday_of(date);
    let repeat = if wd < 5 { "Repeat Mon–Fri" } else { "Repeat on the weekend" };
    let lack = lines_of(s, date).map(|lines| co::ownline::shortfall(c, &co::network::tours_of_day(c, &lines, date))).unwrap_or_default();
    let (text, tone, icon) = match lack.first() {
        Some(x) => (
            trf("At the busiest the tours ask for %{n} × %{bus} (or bigger); the fleet has %{have}.", &[("n", &x.needed.to_string()), ("bus", &tr(co::BusKind { size: x.size, drive: co::Drive::Diesel }.label())), ("have", &x.have.to_string())]),
            "warn",
            "warning",
        ),
        None => (trf("%{day}: the roster of this weekday repeats every week.", &[("day", &day_label(date))]), "text86", "event"),
    };
    json!({
        "text": text, "tone": tone, "icon": icon,
        "fill": { "label": tr("Fill the roster").to_uppercase(), "tip": tr("Fix in the roster what the dispatcher would take: free buses, and free drivers by the working-time rules. Only what is planned runs.") },
        "clear": { "label": tr(if clear_armed { "Press again" } else { "Clear the day" }).to_uppercase(), "widest": [tr("Clear the day").to_uppercase(), tr("Press again").to_uppercase()], "armed": clear_armed, "tip": tr("Take every bus and driver of this weekday out of the roster") },
        "repeat": { "label": tr(repeat).to_uppercase(), "tip": tr("This weekday's roster on the other days too (theirs is replaced)") },
    })
}

/// The day's tours as RosterFlow's Gantt, grouped by line (`rf_roster::gantt`).
fn gantt(c: &Company, p: &DayPlan, sel: Option<&Sel>, arm: Option<Arm>) -> Value {
    if p.tours.is_empty() && c.lines.is_empty() {
        return json!({ "empty": tr("No tours on this day: the company runs no line yet, or its lines do not run on this day.") });
    }
    let t0 = p.tours.iter().map(|t| t.tour.from()).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = (p.tours.iter().map(|t| t.tour.to()).max().unwrap_or(1440) + 59).div_euclid(60) * 60;
    // the lines in the company's order, each with its tours
    let mut groups: Vec<(Option<co::CompanyLine>, Vec<usize>)> = c.lines.iter().map(|x| (Some(x.clone()), Vec::new())).collect();
    for (i, t) in p.tours.iter().enumerate() {
        match groups.iter_mut().find(|g| g.0.as_ref().is_some_and(|x| x.name.eq_ignore_ascii_case(&t.tour.line))) {
            Some(g) => g.1.push(i),
            None => groups.push((None, vec![i])),
        }
    }
    let names: Vec<String> = groups.iter().map(|g| g.0.as_ref().map(|cl| cl.name.clone()).unwrap_or_else(|| p.tours[g.1[0]].tour.line.clone())).collect();
    let folded: Vec<bool> = names.iter().map(|n| pl::is_collapsed(c, n)).collect();
    // over the ID column in the axis: what is in hand, or "Collapse all" / "Expand all"
    let head = match arm {
        Some(a) => {
            let (text, who) = match a {
                Arm::Driver(w) => ("Tap a duty to give it to %{who} (Esc: stop).", who_name(c, Some(w))),
                Arm::Bus(id) => ("Tap a tour's bus to give it %{who} (Esc: stop).", c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default()),
            };
            json!({ "hint": trf(text, &[("who", &who)]) })
        }
        None if groups.iter().any(|g| !g.1.is_empty()) => {
            let all = groups.iter().zip(&folded).filter(|(g, _)| !g.1.is_empty()).all(|(_, f)| *f);
            let (label, icon, tip) = if all { ("Expand all", "unfold_more", "Show every line's tours") } else { ("Collapse all", "unfold_less", "Fold every line to its heading: what it has planned, and what is open") };
            json!({ "fold": { "label": tr(label).to_uppercase(), "icon": icon, "tip": tr(tip), "all": all } })
        }
        None => json!({}),
    };
    let lines: Vec<Value> = groups.iter().enumerate().map(|(gi, (line, idx))| group(c, p, line.as_ref(), &names[gi], idx, folded[gi], sel)).collect();
    json!({ "t0": t0, "t1": t1, "head": head, "groups": lines, "names": names })
}

/// A line's heading and its tours (`rf_roster::line_heading` and the rows under it).
fn group(c: &Company, p: &DayPlan, line: Option<&co::CompanyLine>, name: &str, idx: &[usize], folded: bool, sel: Option<&Sel>) -> Value {
    let none = idx.is_empty();
    let idle = !none && idx.iter().all(|&i| p.tours[i].tour.unplanned);
    let covered = idx.iter().filter(|&&i| p.tours[i].covered()).count();
    let open: usize = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_some()).map(|t| t.duties.iter().filter(|d| d.open()).count()).sum();
    let no_bus = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_none()).count();
    let mut tags = Vec::new();
    if no_bus > 0 {
        tags.push(json!({ "text": trf("%{n} without a bus", &[("n", &no_bus.to_string())]), "icon": "directions_bus" }));
    }
    if open > 0 {
        tags.push(json!({ "text": trf("%{n} without a driver", &[("n", &open.to_string())]), "icon": "person" }));
    }
    let service = match line {
        None => Value::Null,
        Some(_) if none => json!({ "kind": "none", "text": tr("No tours this day") }),
        Some(cl) if cl.service_from.is_none() || idle => json!({
            "kind": "start", "label": tr("Start service…").to_uppercase(),
            "tip": tr("Its tours run from the moment you choose, as they are planned; what is not covered then is dropped with the contract's penalty"),
            "note": trf("Not in service · %{c} of %{t} tours planned", &[("c", &covered.to_string()), ("t", &idx.len().to_string())]),
        }),
        Some(cl) => {
            let since = cl.service_from.map(|s| if s <= 0 { String::new() } else { format!("{} {}", day_label(&co::clock::date_of(s)), co::clock::hhmm(s)) }).unwrap_or_default();
            let text = if since.is_empty() { tr("In service") } else { trf("In service since %{when}", &[("when", &since)]) };
            let cov = trf("%{c} of %{t} tours covered", &[("c", &covered.to_string()), ("t", &idx.len().to_string())]);
            json!({ "kind": "in", "text": format!("{text}  ·  {cov}"), "tone": if covered == idx.len() { "ok" } else { "warn" }, "stop_tip": tr("Take the line out of service") })
        }
    };
    let tours: Vec<Value> = if folded { Vec::new() } else { idx.iter().map(|&ti| tour_row(c, p, ti, sel)).collect() };
    json!({
        "name": name,
        "line": line.map(|cl| json!({ "plate": plate(&cl.number, &cl.colour), "colour": line_colour(c, &cl.name), "caption": if cl.caption.is_empty() { cl.name.clone() } else { cl.caption.clone() } })),
        "folded": folded || none, "none": none, "idle": idle, "count": idx.len(),
        "tags": tags, "tag_tone": if idle { "dim" } else { "warn" },
        "fold_tip": tr(if folded || none { "Expand the line: its tours" } else { "Collapse the line to its heading" }),
        "tt": { "label": tr("Timetable"), "tip": tr("The line's trips and timetable: see them and change them here") },
        "service": service,
        "tours": tours,
    })
}

/// A tour's row: its ID (the tour, its bus) and its duties on the hours (`rf_roster::gantt`'s
/// rows, `bus_chip`, `duty_chip`).
fn tour_row(c: &Company, p: &DayPlan, ti: usize, sel: Option<&Sel>) -> Value {
    let t = &p.tours[ti];
    let tour_on = sel == Some(&Sel::Tour(t.tour.line.clone(), t.tour.tour.clone()));
    let tip = tr("The tour's bus: tap a bus in the list, then here - or drag it here. Tap it twice: the line's timetable");
    let bus = if t.by_player {
        json!({ "kind": "you", "text": tr("You drove it.") })
    } else {
        match t.bus {
            None => {
                let why = t.bus_problem.map(|x| tr(x.label())).unwrap_or_default();
                json!({ "kind": "none", "tone": problem_tone(t.bus_problem), "icon": if t.bus_problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" }, "text": tr("No bus"), "tip": if why.is_empty() { tip.clone() } else { format!("{why}\n{tip}") } })
            }
            Some(b) => {
                let (text, suggested) = match b {
                    BusOf::Own(id) => (c.vehicle(id).map(|x| format!("{}  {}", x.number, x.name)).unwrap_or_default(), t.bus_from == Source::Auto),
                    BusOf::Rental => (tr("Rental bus"), false),
                };
                json!({ "kind": "bus", "text": text, "suggested": suggested, "problem": t.bus_problem.map(|x| problem_tone(Some(x))), "tip": tip })
            }
        }
    };
    let duties: Vec<Value> = t.duties.iter().enumerate().map(|(k, _)| duty_chip(c, p, ti, k, sel)).collect();
    json!({
        "ti": ti, "line": t.tour.line, "tour": t.tour.tour, "label": format!("{} {}", tr("Tour"), t.tour.tour),
        "colour": line_colour(c, &t.tour.line), "unplanned": t.tour.unplanned, "on": tour_on, "by_player": t.by_player,
        "bus": bus, "duties": duties,
    })
}

/// A duty on the hours (`rf_roster::duty_chip`).
fn duty_chip(c: &Company, p: &DayPlan, ti: usize, k: usize, sel: Option<&Sel>) -> Value {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    let times = format!("{} – {}", hhmm(d.from), hhmm(d.to));
    let open = d.who.is_none() && !t.by_player;
    let faint = if t.bus.is_none() && !t.by_player { 0.5 } else { 1.0 };
    let chosen = sel == Some(&Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k));
    let mut out = json!({ "k": k, "from": d.from, "to": d.to, "times": times, "open": open, "chosen": chosen, "faint": faint, "warn": !d.warn.is_empty() });
    if open {
        let words = problem_text(c, p, ti, k, d.problem);
        out["tone"] = json!(problem_tone(d.problem));
        out["icon"] = json!(if d.problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" });
        out["full"] = json!(format!("{words}  {times}"));
        out["words"] = json!(words);
    } else {
        let (fill, alpha, ink, label, short) = if t.by_player {
            ("accent".to_string(), 0.6, "white", tr("You"), String::new())
        } else {
            match d.who {
                Some(Who::Staff(id)) => {
                    let full = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
                    (driver_colour(c, id), if d.from_ == Source::Auto { 0.6 } else { 1.0 }, "dark", first_name(c, id), initials(&full))
                }
                Some(Who::Player) => ("accent".to_string(), 1.0, "white", tr("You"), String::new()),
                Some(Who::Agency) => ("grey".to_string(), 1.0, "white", tr("Agency"), String::new()),
                // (Beta 58: a player of "Play together")
                Some(Who::Guest(g)) => (crate::omsihub::together::GUEST_COLOUR.to_string(), 1.0, "dark", crate::omsihub::together::guest_label(c, g), String::new()),
                None => ("grey".to_string(), 1.0, "white", String::new(), String::new()),
            }
        };
        // (its depot and empty runs, shaded: run with it, without passengers)
        let shades: Vec<[i32; 2]> = t.tour.trips[d.start..d.end].iter().filter(|x| x.empty).map(|x| [x.dep.max(d.from), x.arr.min(d.to)]).collect();
        out["fill"] = json!(fill);
        out["alpha"] = json!(alpha);
        out["ink"] = json!(ink);
        out["label"] = json!(label);
        out["short"] = json!(short);
        out["edge"] = json!(matches!(d.from_, Source::Dispatcher | Source::Central));
        out["shades"] = json!(shades);
    }
    if let Some(late) = d.late {
        out["late"] = json!({ "until": late.until.min(d.to), "tone": if late.cover.is_some() { "dim" } else { "err" } });
    }
    out["tip"] = json!(format!("{times}  ·  {}", who_name(c, if t.by_player { Some(Who::Player) } else { d.who })));
    out
}

// --- the side ---------------------------------------------------------------------------------

fn side(c: &Company, p: &DayPlan, sel: Option<&Sel>, arm: Option<Arm>) -> Value {
    let find = |line: &str, tour: &str| p.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour);
    match sel {
        Some(Sel::Duty(line, tour, k)) => match find(line, tour).filter(|&i| *k < p.tours[i].duties.len()) {
            Some(ti) => duty_panel(c, p, ti, *k),
            None => day_panel(c, p, arm),
        },
        Some(Sel::Tour(line, tour)) => match find(line, tour) {
            Some(ti) => tour_panel(c, p, ti),
            None => day_panel(c, p, arm),
        },
        None => day_panel(c, p, arm),
    }
}

/// Nothing chosen: the day in short, what fell out, and the drivers and buses to give
/// (`rf_roster::day_panel`).
fn day_panel(c: &Company, p: &DayPlan, arm: Option<Arm>) -> Value {
    let (tours, covered, open, no_bus) = p.counts();
    let rows = p.open_rows();
    let morning: Vec<Value> = p
        .disruptions
        .iter()
        .map(|d| match *d {
            Disruption::Late { employee, minutes } => json!({ "text": trf("%{name} comes %{n} minutes late.", &[("name", &who_name(c, Some(Who::Staff(employee)))), ("n", &minutes.to_string())]), "tone": "warn" }),
            Disruption::Breakdown { vehicle, cost } => json!({ "text": trf("Bus %{n} did not start: towed to the workshop (%{amount}).", &[("n", &c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default()), ("amount", &eur(cost))]), "tone": "err" }),
        })
        .collect();
    let staff: Vec<Value> = c
        .staff
        .iter()
        .filter(|e| co::staff::drives(e))
        .map(|e| {
            let mins: i32 = p.duties_of(Who::Staff(e.id)).iter().map(|&(i, k)| p.tours[i].duties[k].to - p.tours[i].duties[k].from).sum();
            let (state, tone) = if let Some(x) = pl::day_unavailable(e, &p.date, p.today) {
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
                (length(mins), if mins > co::staff::DAY_TARGET { "warn" } else { "text82" })
            };
            json!({ "who": who_key(Who::Staff(e.id)), "name": e.name, "colour": driver_colour(c, e.id), "state": state, "tone": tone, "on": arm == Some(Arm::Driver(Who::Staff(e.id))) })
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
                (trf("%{n} tours", &[("n", &n.to_string())]), "text82")
            };
            json!({ "id": v.id, "name": format!("{}  {}", v.number, v.name), "state": state, "tone": tone, "on": arm == Some(Arm::Bus(v.id)) })
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
            json!({ "ti": o.tour, "duty": o.duty, "line": t.tour.line, "tour": t.tour.tour, "title": format!("{}  ·  {}", line_title(t, o.duty), what), "fill": format!("→ {fill}"), "tone": if o.filled { "text82" } else { problem_tone(o.problem) }, "hard": hard })
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
        "ok": open + no_bus + late_dropped == 0, "gaps": gaps,
        "heads": { "morning": tr("This morning").to_uppercase(), "open": tr("Open duties").to_uppercase(), "drivers": tr("Drivers").to_uppercase(), "buses": tr("Buses").to_uppercase() },
        "morning": morning, "open": open_rows,
        "hint": tr("Tap one, then a duty - or drag it there; or drag a duty here."),
        "me": { "who": "player", "name": tr("You"), "state": tr("Your own duties"), "on": arm == Some(Arm::Driver(Who::Player)) },
        "staff": staff, "no_staff": tr("Nobody on the payroll yet."),
        "buses": buses, "no_buses": tr("No bus in the fleet yet."),
    })
}

fn tt_button() -> Value {
    json!({ "label": tr("The line's trips and timetable"), "tip": tr("See the line's departures and the tours they make, and change them") })
}

/// A duty chosen: its times, who drives it and why, who else could; on the company's day what to
/// do when it is open, and "Drive this duty" (`rf_roster::duty_panel`).
fn duty_panel(c: &Company, p: &DayPlan, ti: usize, k: usize) -> Value {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    let key = pl::duty_key(&t.tour.line, &t.tour.tour, k);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let late_key = pl::late_key(&t.tour.line, &t.tour.tour, k);
    let late_fill = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == late_key);
    let first = &t.tour.trips[d.start];
    let last = &t.tour.trips[d.end - 1];
    let mut facts = vec![
        json!({ "text": format!("{} – {}  ·  {}  ·  {} {}", hhmm(d.from), hhmm(d.to), length(d.to - d.from), d.end - d.start, tr("trips")), "tone": "text86" }),
        json!({ "text": format!("{} → {}", first.from, last.to), "tone": "dim" }),
        json!({ "text": if t.by_player { tr("You drove it.") } else { format!("{}: {}", tr("Driven by"), who_name(c, d.who)) }, "tone": "text" }),
    ];
    if !source_text(d.from_).is_empty() && d.who.is_some() {
        facts.push(json!({ "text": tr(source_text(d.from_)), "tone": "dim" }));
    }
    if let Some(pb) = d.problem {
        facts.push(json!({ "text": format!("{}: {}", tr("Open because"), tr(pb.label())), "tone": problem_tone(Some(pb)) }));
    }
    for w in &d.warn {
        let s = match w {
            Warn::Overtime(m) => trf("Overtime %{t}", &[("t", &length(*m))]),
            Warn::Transfer(m) => trf("%{n} min to change stops", &[("n", &m.to_string())]),
            Warn::Experience => tr("Little experience for this bus"),
        };
        facts.push(json!({ "text": s, "tone": "warn" }));
    }
    if let Some(late) = d.late {
        let s = match late.cover {
            Some(id) => trf("The driver is late until %{t}: %{name} drives the first trips.", &[("t", &hhmm(late.until)), ("name", &who_name(c, Some(if id == co::plan::AGENCY { Who::Agency } else { Who::Staff(id) })))]),
            None => trf("The driver is late until %{t}: the trips before are dropped.", &[("t", &hhmm(late.until))]),
        };
        facts.push(json!({ "text": s, "tone": if late.cover.is_some() { "warn" } else { "err" } }));
    }
    // (the agency sends as many as are asked for: no "none left" any more - `plan::agency_room`)
    let open_today = p.today && !t.by_player && (d.problem.is_some() || d.who.is_none());
    let chosen = c.planning.fills.iter().find(|f| c.planning.date == c.date && f.key == key).map(|f| f.fill);
    let mut options = Vec::new();
    if open_today {
        for cv in pl::covers_of_duty(c, p, ti, k) {
            let (label, note) = cover_text(c, &cv);
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
    let mut cands = vec![json!({ "who": "player", "name": tr("You"), "state": tr("Drive it yourself"), "tone": "dim", "swatch": "accent", "on": current == Some(Who::Player) })];
    for x in pl::candidates(c, p, ti, k) {
        let (s, tone) = fit_of(c, p, ti, k, x.id);
        let name = c.employee(x.id).map(|e| e.name.clone()).unwrap_or_default();
        cands.push(json!({ "who": who_key(Who::Staff(x.id)), "name": name, "state": s, "tone": tone, "swatch": driver_colour(c, x.id), "on": current == Some(Who::Staff(x.id)) }));
    }
    cands.push(json!({ "who": null, "name": tr("Nobody"), "state": "", "tone": "dim", "swatch": null, "on": false }));
    json!({
        "kind": "duty", "title": tr("Duty").to_uppercase(), "close": tr("Back to the day"),
        "ti": ti, "k": k, "line": t.tour.line, "tour": t.tour.tour, "by_player": t.by_player,
        "head": line_title(t, Some(k)), "colour": line_colour(c, &t.tour.line), "tt": tt_button(),
        "facts": facts,
        "drive": (p.today && !t.by_player).then(|| tr("Drive this duty").to_uppercase()),
        "options_title": tr("Cover it today").to_uppercase(), "options": options,
        "cands_title": tr("Who drives it").to_uppercase(), "cands": cands,
    })
}

/// A tour chosen: its bus and which others could run it; on the company's day a rental bus or
/// dropping it (`rf_roster::tour_panel`).
fn tour_panel(c: &Company, p: &DayPlan, ti: usize) -> Value {
    let t = &p.tours[ti];
    let mut facts = vec![json!({ "text": format!("{} – {}  ·  {} km  ·  {} {}", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km().round(), t.duties.len(), tr("duties")), "tone": "text86" })];
    let bus = match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        Some(BusOf::Rental) => tr("Rental bus"),
        None => tr("No bus"),
    };
    facts.push(json!({ "text": format!("{}: {}", tr("Bus"), bus), "tone": if t.bus.is_none() && !t.by_player { "warn" } else { "text" } }));
    if t.bus.is_some() && !source_text(t.bus_from).is_empty() {
        facts.push(json!({ "text": tr(source_text(t.bus_from)), "tone": "dim" }));
    }
    if let Some(pb) = t.bus_problem {
        facts.push(json!({ "text": format!("{}: {}", tr("Open because"), tr(pb.label())), "tone": problem_tone(Some(pb)) }));
    }
    if let Some(w) = t.tour.wants() {
        facts.push(json!({ "text": trf("The tour asks for: %{kind}", &[("kind", &tr(co::BusKind { size: w, drive: co::Drive::Diesel }.label()))]), "tone": "dim" }));
    }
    let key = pl::tour_key(&t.tour.line, &t.tour.tour);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let chosen = c.planning.fills.iter().find(|f| c.planning.date == c.date && f.key == key).map(|f| f.fill);
    let mut options = Vec::new();
    if p.today && !t.by_player && (t.bus.is_none() || t.bus_problem.is_some()) {
        for (label, note, fill) in tour_covers(c, p, ti) {
            options.push(json!({ "label": label, "note": note, "fill": fill, "key": key, "on": fill.is_some() && chosen == fill, "live": fill.is_some() }));
        }
        if fill_now {
            options.push(json!({ "label": tr("Leave it to the central"), "note": "", "fill": null, "key": key, "on": false, "live": true }));
        }
    }
    let current = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    let mut cands: Vec<(bool, Value)> = c
        .fleet
        .iter()
        .map(|v| {
            let (s, tone, ok) = bus_fit(c, p, ti, v.id);
            (ok, json!({ "bus": v.id, "name": format!("{}  {}", v.number, v.name), "state": s, "tone": tone, "on": current == Some(v.id) }))
        })
        .collect();
    cands.sort_by(|a, b| b.0.cmp(&a.0));
    let mut cands: Vec<Value> = cands.into_iter().map(|x| x.1).collect();
    cands.push(json!({ "bus": null, "name": tr("No bus"), "state": "", "tone": "dim", "on": false }));
    json!({
        "kind": "tour", "title": tr("Tour").to_uppercase(), "close": tr("Back to the day"),
        "ti": ti, "line": t.tour.line, "tour": t.tour.tour, "by_player": t.by_player,
        "head": line_title(t, None), "colour": line_colour(c, &t.tour.line), "tt": tt_button(),
        "facts": facts,
        "options_title": tr("Cover it today").to_uppercase(), "options": options,
        "cands_title": tr("Which bus runs it").to_uppercase(), "cands": if t.by_player { Vec::new() } else { cands },
    })
}

// --- putting a line into service ------------------------------------------------------------------

/// "Start service from": now, from tomorrow or from a date; the line's next seven days as they
/// are planned, and - with gaps - the player's word that he starts with them
/// (`planning::service_dialog`). `when` 0 now, 1 tomorrow, 2 the date `date`.
pub fn service_view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let line = str_arg(args, "line");
    let Some(cl) = c.lines.iter().find(|x| x.name == line).cloned() else { return json!({ "gone": true }) };
    let (from, start_day) = service_from(&c, args);
    week_work(s);
    let mut days = Vec::new();
    let mut short = 0usize;
    let mut reading = false;
    for k in 0..7 {
        let day = co::dates::add(&start_day, k);
        let (text, tone) = match service_plan(s, &c, &day) {
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
        days.push(json!({ "day": day_label(&day), "text": text, "tone": tone }));
    }
    json!({
        "title": trf("Start the service of line %{n}", &[("n", &cl.number)]),
        "intro": tr("From the moment you choose, the line's tours run as they are planned. A tour without its bus or a driver is dropped then, with the contract's penalty - plan first, then start."),
        "from": tr("From when").to_uppercase(), "ways": [tr("Now"), tr("From tomorrow"), tr("From a date")],
        "week": tr("The line's week as it is planned").to_uppercase(), "days": days,
        "short": (short > 0).then(|| trf("%{n} tours of these days have no bus or no driver yet: they would be dropped, each with its penalty.", &[("n", &short.to_string())])),
        "gaps": tr("Start all the same, with these gaps"),
        "go": tr("Start service"), "cancel": tr("Cancel"), "reading": reading,
        "date": args.get("date").and_then(Value::as_str).filter(|d| co::dates::parse(d).is_some()).unwrap_or(&c.date),
    })
}

/// When a line's service would start, and the first day it runs.
fn service_from(c: &Company, args: &Value) -> (i64, String) {
    let when = args.get("when").and_then(Value::as_u64).unwrap_or(0);
    let date = args.get("date").and_then(Value::as_str).filter(|d| co::dates::parse(d).is_some()).unwrap_or(&c.date).to_string();
    let from = match when {
        0 => co::clock::now(c),
        1 => co::clock::moment(&co::dates::add(&c.date, 1), 0),
        _ => co::clock::moment(&date, 0).max(co::clock::now(c)),
    };
    (from, co::clock::date_of(from))
}

/// A day's plan for the service dialog: within the week the planning reads; past it, read here
/// on the spot (`planning::work`'s extra days).
fn service_plan(s: &mut Session, c: &Company, date: &str) -> Option<Result<DayPlan, String>> {
    if co::dates::between(&c.date, date) < 7 {
        return plan_of(s, date);
    }
    Some(omsi_launcher_lib::list_lines(&c.map, date).map_err(|e| format!("{e:#}")).map(|lines| {
        let tours = co::network::tours_of_day(c, &lines, date);
        pl::day_plan(c, date, tours, &[], &[], false)
    }))
}

// --- what a click does ------------------------------------------------------------------------------

/// Give a duty its driver: into the roster, or - for one that fell out today with somebody fixed
/// for it - for today only (`planning::give_driver`).
fn give_driver(s: &mut Session, p: &DayPlan, ti: usize, k: usize, who: Option<Who>) -> Answer {
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    let Some(t) = p.tours.get(ti) else { return Ok(json!({})) };
    if k >= t.duties.len() {
        return Ok(json!({}));
    }
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    // (an agency driver booked ahead for the day: the duty, and those joined with it, given him
    // for his day only; given to anyone else, or to nobody, he gives it back - `co::agency`)
    let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let pieces: Vec<co::studio::PieceRef> = std::iter::once((ti, k)).chain(co::studio::partners(c, co::plandays::slot_of(c, &p.date), &blocks, ti, k)).filter_map(|(i, m)| co::studio::piece_ref(&blocks, i, m)).collect();
    let date = p.date.clone();
    let booked = matches!(who, Some(Who::Staff(id)) if co::agency::is_booked(id));
    let had = matches!(t.duties[k].who, Some(Who::Staff(id)) if co::agency::is_booked(id));
    if booked || had {
        let r = apply(s, |c| {
            for piece in &pieces {
                match who {
                    Some(Who::Staff(id)) if booked => {
                        co::agency::give(c, id, piece.clone());
                    }
                    _ => {
                        co::agency::release(c, &date, piece);
                    }
                }
            }
            Ok(())
        });
        if booked || who.is_none() {
            return Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
        }
    }
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.duties.get(k).copied().flatten());
    let today_only = p.today && t.duties[k].problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some() && who != Some(Who::Player);
    let wd = p.weekday;
    let r = if today_only {
        let fill = match who {
            Some(Who::Staff(id)) => Some(Fill::Colleague { id }),
            Some(Who::Agency) => Some(Fill::Agency),
            _ => Some(Fill::Drop),
        };
        let key = pl::duty_key(&line, &tour, k);
        apply(s, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        })
    } else {
        // (a piece of a duty joined in the studio: its other pieces go to the same driver)
        let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
        let others: Vec<(String, String, usize)> = co::studio::partners(c, co::plandays::slot_of(c, &p.date), &blocks, ti, k).into_iter().map(|(i, k)| (p.tours[i].tour.line.clone(), p.tours[i].tour.tour.clone(), k)).collect();
        apply(s, |c| {
            pl::set_driver(c, wd, &line, &tour, k, who);
            for (line, tour, k) in &others {
                pl::set_driver(c, wd, line, tour, *k, who);
            }
            Ok(())
        })
    };
    Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
}

/// Give a tour its bus (`planning::give_bus`): a line that asks for buses of its own runs with
/// none other.
fn give_bus(s: &mut Session, p: &DayPlan, ti: usize, bus: Option<u32>) -> Answer {
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    let Some(t) = p.tours.get(ti) else { return Ok(json!({})) };
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    if let Some((v, want)) = bus.and_then(|id| c.vehicle(id)).zip(co::ownline::vehicles_of(c, &line)).filter(|(v, _)| !co::ownline::line_allows(c, &line, v)) {
        let text = trf("Line %{n} runs with %{buses}: bus %{bus} is none of them.", &[("n", &t.tour.number), ("buses", &want.summary(&|s| tr(s))), ("bus", &v.number)]);
        let p = super::super::kit::Popup::new("directions_bus", "Not one of the line's buses", text, tr("Buy or lease one of them, or choose other buses for the line in the line editor."), None);
        return popup_answer(p.json(c.game_mode.casual()));
    }
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.bus);
    let today_only = p.today && t.bus_problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some();
    let wd = p.weekday;
    let r = if today_only {
        let fill = Some(bus.map(|id| Fill::Bus { id }).unwrap_or(Fill::Drop));
        let key = pl::tour_key(&line, &tour);
        apply(s, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        })
    } else {
        apply(s, |c| {
            pl::set_bus(c, wd, &line, &tour, bus);
            Ok(())
        })
    };
    Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
}

/// The plan of the day an action is about (`date`), as the page showed it.
fn plan_for(s: &mut Session, args: &Value) -> Option<DayPlan> {
    let date = str_arg(args, "date");
    week_work(s);
    plan_of(s, &date)?.ok()
}

fn popup(s: &Session, icon: &'static str, title: &str, text: String, unlock: String, go: Option<super::super::kit::Go>) -> Answer {
    let casual = s.company.as_ref().is_some_and(|c| c.game_mode.casual());
    popup_answer(super::super::kit::Popup::new(icon, title, text, unlock, go).json(casual))
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    if !what.starts_with("plan_") {
        return None;
    }
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let ti = args.get("ti").and_then(Value::as_u64).unwrap_or(0) as usize;
    let k = args.get("k").and_then(Value::as_u64).map(|k| k as usize);
    let date = str_arg(args, "date");
    let wd = pl::weekday_of(&date);
    let day = weekday_name(wd);
    Some(match what {
        "plan_give_driver" => match (plan_for(s, args), k) {
            (Some(p), Some(k)) if p.tours.get(ti).is_some_and(|t| !t.by_player) => give_driver(s, &p, ti, k, args.get("who").and_then(Value::as_str).and_then(who_of)),
            _ => Ok(json!({})),
        },
        "plan_give_bus" => match plan_for(s, args) {
            Some(p) if p.tours.get(ti).is_some_and(|t| !t.by_player) => give_bus(s, &p, ti, args.get("bus").and_then(Value::as_u64).map(|b| b as u32)),
            _ => Ok(json!({})),
        },
        "plan_set_fill" => {
            let key = str_arg(args, "key");
            let fill: Option<Fill> = args.get("fill").cloned().and_then(|f| serde_json::from_value(f).ok());
            Ok(apply(s, |c| {
                pl::set_fill(c, &key, fill);
                Ok(())
            })
            .err()
            .map(|p| json!({ "popup": p }))
            .unwrap_or(json!({})))
        }
        // the tools: each says what it did - or, when it could do nothing, why, in a popup
        "plan_fill" => {
            let Some(lines) = lines_of(s, &date) else { return Some(Ok(json!({}))) };
            let d = date.clone();
            match apply(s, |c| {
                let tours = co::network::tours_of_day(c, &lines, &d);
                Ok(pl::fill_day_told(c, &d, tours))
            }) {
                Ok(f) => {
                    let open = open_text(&f);
                    if f.duties + f.buses > 0 {
                        let done = trf("Filled: %{d} duties given to drivers, %{b} buses given.", &[("d", &f.duties.to_string()), ("b", &f.buses.to_string())]);
                        s.say(if open.is_empty() { done } else { format!("{done} {open}") }, false);
                        Ok(json!({}))
                    } else if open.is_empty() {
                        popup(s, "event", "Nothing to fill", trf("Every tour of %{day} has its bus and its drivers already.", &[("day", &day)]), String::new(), None)
                    } else {
                        use super::super::kit::Go;
                        let go = if f.unqualified > 0 { Go::Licences } else if f.busy > 0 { Go::Hire } else { Go::Dealer };
                        popup(s, "event", "Nothing could be filled", open, tr("Hire drivers, train them for the buses on the Staff page, or get more buses."), Some(go))
                    }
                }
                Err(p) => popup_answer(p),
            }
        }
        "plan_clear" => match apply(s, |c| Ok(pl::clear_day(c, wd))) {
            Ok(gone) if gone.is_empty() => popup(s, "delete", "Nothing to clear", trf("The roster of %{day} is empty already.", &[("day", &day)]), String::new(), None),
            Ok(gone) => {
                s.say(trf("The day is cleared: %{d} duties and %{b} buses taken off the roster of %{day}.", &[("d", &gone.duties.to_string()), ("b", &gone.buses.to_string()), ("day", &day)]), false);
                Ok(json!({}))
            }
            Err(p) => popup_answer(p),
        },
        "plan_repeat" => {
            let to: Vec<u8> = if wd < 5 { (0..5).collect() } else { vec![5, 6] };
            match apply(s, |c| Ok(pl::copy_day(c, wd, &to))) {
                Ok(days) if days.is_empty() => popup(s, "content_copy", "Nothing to repeat", trf("The roster of %{day} is empty: there is nothing to repeat. Fill it first (\"Fill the roster\" does it for you), then repeat it on the other days.", &[("day", &day)]), String::new(), None),
                Ok(days) => {
                    let key = if days.len() == 1 { "The roster of %{day} is now on %{days} too." } else { "The roster of %{day} is now on %{days} too (%{n} days)." };
                    s.say(trf(key, &[("day", &day), ("days", &weekdays_text(&days)), ("n", &days.len().to_string())]), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        // a line folded to its heading and back, every line at once: kept with the company
        "plan_fold" => {
            let name = str_arg(args, "name");
            let to = !pl::is_collapsed(&c, &name);
            let _ = apply(s, |c| {
                pl::set_collapsed(c, &name, to);
                Ok(())
            });
            Ok(json!({}))
        }
        "plan_fold_all" => {
            let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
            let names: Vec<String> = args.get("names").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let _ = apply(s, |c| {
                pl::set_all_collapsed(c, &names, all);
                Ok(())
            });
            Ok(json!({}))
        }
        "plan_stop" => {
            let name = str_arg(args, "name");
            Ok(apply(s, |c| co::network::stop_service(c, &name)).err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
        }
        "plan_start" => {
            let line = str_arg(args, "line");
            let gaps = args.get("gaps").and_then(Value::as_bool).unwrap_or(false);
            let Some(cl) = c.lines.iter().find(|x| x.name == line).cloned() else { return Some(Ok(json!({ "done": true }))) };
            let v = service_view(s, args);
            let (from, _) = service_from(&c, args);
            if v["reading"].as_bool().unwrap_or(false) {
                popup(s, "timer", "Not yet", tr("The line's week is still being read."), tr("Try again in a moment."), None)
            } else if !v["short"].is_null() && !gaps {
                popup(s, "event", "Not planned yet", tr("Some of the line's tours have no bus or no driver: they would be dropped, each with the contract's penalty."), tr("Give them buses and drivers on the planning (\"Fill the roster\" does it for you) - or choose to start with the gaps."), None)
            } else {
                match apply(s, |c| co::network::start_service(c, &line, from)) {
                    Ok(at) => {
                        s.say(trf("Line %{n} is in service from %{when}.", &[("n", &cl.number), ("when", &format!("{} {}", day_label(&co::clock::date_of(at)), co::clock::hhmm(at)))]), false);
                        Ok(json!({ "done": true }))
                    }
                    Err(p) => popup_answer(p),
                }
            }
        }
        // "Drive this duty": the company's duty launcher with exactly this duty (`planning::drive`)
        "plan_drive" => {
            let Some(p) = plan_for(s, args) else { return Some(Ok(json!({}))) };
            match k.and_then(|k| co::duty_launch::DutyRef::of(&p, ti, k)) {
                Some(r) => {
                    s.launch = Some(DutyLaunch::new(r, 5));
                    s.generation += 1;
                    Ok(json!({ "launch": true }))
                }
                None => Ok(json!({})),
            }
        }
        // a line's trips and timetable (`planning::open_timetable`): RosterFlow's Timeplan with
        // the add-on on (handed to it), else the planning's own sheet (`plan_edit`, Omsi-Hub
        // 6/6) with the day and the tour to mark
        "plan_timetable" => {
            let line = str_arg(args, "line");
            if s.company.as_ref().is_some_and(co::autopilot::rosterflow) {
                return Some(Ok(json!({ "hand_over": format!("timetable={line}") })));
            }
            let tour = Some(str_arg(args, "tour")).filter(|t| !t.is_empty());
            let e = super::plan_edit::open(s, &line, &date, tour);
            let opened = e.is_some();
            super::more(s).edit = e;
            Ok(json!({ "plan_edit": opened }))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_names_who_and_what_it_holds() {
        for w in [Who::Player, Who::Agency, Who::Staff(7)] {
            assert_eq!(who_of(&who_key(w)), Some(w));
        }
        assert_eq!(who_of("nobody"), None);
        assert_eq!(arm_of(&json!({ "arm": { "driver": "s3" } })), Some(Arm::Driver(Who::Staff(3))));
        assert_eq!(arm_of(&json!({ "arm": { "bus": 12 } })), Some(Arm::Bus(12)));
        assert_eq!(arm_of(&json!({})), None);
        assert_eq!(sel_of(&json!({ "sel": { "line": "oo_5", "tour": "2", "k": 1 } })), Some(Sel::Duty("oo_5".into(), "2".into(), 1)));
        assert_eq!(sel_of(&json!({ "sel": { "line": "oo_5", "tour": "2" } })), Some(Sel::Tour("oo_5".into(), "2".into())));
        assert_eq!(weekdays_text(&[1, 2, 3, 4]), "Tuesday to Friday");
        assert_eq!(weekdays_text(&[0, 1, 3, 4]), "Monday, Tuesday, Thursday, Friday");
        assert_eq!(initials("Jan  Meijer"), "JM");
        let f = pl::Filled { duties: 9, buses: 4, unqualified: 2, busy: 0, no_bus: 0 };
        assert_eq!(open_text(&f), "2 duties still open: nobody is qualified for their bus.");
    }
}

//! Late running and the trips dropped as the pages show them (`co::lateness`, Luc 2026-10-15):
//! each trip with its line, block, duty, times and trip ID, its bus (fleet number, plate, model)
//! and its driver (a name, the agency, the player) - each a link -, its minutes late with the
//! chain of causes and its deduction, or why it was dropped and which bus or driver was missing;
//! the advice against late running; the day report's part; the finances' deductions; and what
//! RosterFlow marks on its chips (the day's service).

use super::words::{day_label, eur, tr, trf};
use super::Session;
use omsi_launcher_lib::company::clock as ck;
use omsi_launcher_lib::company::day::{DayReport, DropWhy, Plan};
use omsi_launcher_lib::company::economy;
use omsi_launcher_lib::company::lateness::{self as lt, Advice, Cause, Lateness, Outcome};
use omsi_launcher_lib::company::plan::{Problem, AGENCY, PLAYER};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

fn hm(m: i32) -> String {
    ck::hhmm(m as i64)
}

/// A block as RosterFlow names it ("5/02").
pub fn block(number: &str, tour: &str) -> String {
    let tour = tour.trim();
    let tour = if tour.len() == 1 && tour.chars().all(|c| c.is_ascii_digit()) { format!("0{tour}") } else { tour.to_string() };
    format!("{}/{}", number.trim(), tour)
}

/// A bus as a link: its fleet number, plate and model ("104 · OHB-K 104 · Citaro").
pub fn bus_json(c: &Company, id: Option<u32>) -> Value {
    let Some(id) = id else { return Value::Null };
    match c.vehicle(id) {
        Some(v) => json!({ "id": id, "text": format!("{} · {} · {}", v.number, v.plate, v.name), "short": v.number }),
        None => json!({ "id": null, "text": tr("a bus no longer in the fleet"), "short": "–" }),
    }
}

/// A bus's fleet number (in a cause's words).
fn bus_no(c: &Company, id: u32) -> String {
    c.vehicle(id).map(|v| v.number.clone()).unwrap_or_else(|| "–".into())
}

/// A driver as a link: their name, the agency's, the player.
pub fn driver_json(c: &Company, id: Option<u32>) -> Value {
    match id {
        None => Value::Null,
        Some(AGENCY) => json!({ "id": null, "text": tr("Agency driver") }),
        Some(PLAYER) => json!({ "id": null, "text": tr("You") }),
        Some(id) => match c.employee(id) {
            Some(e) => json!({ "id": id, "text": e.name }),
            None => json!({ "id": null, "text": tr("a former employee") }),
        },
    }
}

fn name_of(c: &Company, id: u32) -> String {
    driver_json(c, Some(id)).get("text").and_then(Value::as_str).unwrap_or("").to_string()
}

/// A cause in words.
pub fn cause_text(c: &Company, x: &Cause) -> String {
    let m = x.minutes().to_string();
    match x {
        Cause::SignOn { driver, .. } => trf("%{name} signed on %{m} min late", &[("name", &name_of(c, *driver)), ("m", &m)]),
        Cause::Carried { dep, arr, stop, late, layover, deadhead, .. } => {
            let t = if *deadhead { "the empty run of %{dep} reached %{stop} at %{at} (%{late} min late), %{layover} min layover" } else { "the %{dep} trip reached %{stop} at %{at} (%{late} min late), %{layover} min layover" };
            trf(t, &[("dep", &hm(*dep)), ("stop", stop.trim()), ("at", &hm(arr + late)), ("late", &late.to_string()), ("layover", &layover.to_string())])
        }
        Cause::Relief { driver, end, gap, .. } => trf("%{name} came from a duty that ended at %{end}: %{gap} min planned for the relief", &[("name", &name_of(c, *driver)), ("end", &hm(*end)), ("gap", &gap.to_string())]),
        Cause::Breakdown { bus, at, spare: Some(s), .. } => trf("bus %{bus} broke down at %{at}; the bus sent came at %{spare}", &[("bus", &bus_no(c, *bus)), ("at", &hm(*at)), ("spare", &hm(*s))]),
        Cause::Breakdown { bus, at, .. } => trf("bus %{bus} broke down at %{at}; no bus was sent", &[("bus", &bus_no(c, *bus)), ("at", &hm(*at))]),
        Cause::Traffic { peak: true, .. } => trf("rush-hour traffic: %{m} min over the running time", &[("m", &m)]),
        Cause::Traffic { .. } => trf("traffic: %{m} min over the running time", &[("m", &m)]),
        Cause::Driver { driver, .. } => trf("%{name} drove %{m} min slower than the timetable", &[("name", &name_of(c, *driver)), ("m", &m)]),
        Cause::Bus { bus, .. } => trf("bus %{bus} in poor condition: %{m} min", &[("bus", &bus_no(c, *bus)), ("m", &m)]),
        Cause::Event { .. } => trf("an event on the line (roadworks, a diversion, the weather): %{m} min", &[("m", &m)]),
        Cause::Chance { .. } => trf("delays on the way: %{m} min", &[("m", &m)]),
    }
}

/// A cause's kind in words (the sums by cause).
pub fn cause_kind(key: &str) -> String {
    tr(match key {
        "sign_on" => "Drivers signing on late",
        "carried" => "Delays carried on (layover too short)",
        "relief" => "Reliefs too tight",
        "breakdown" => "Breakdowns",
        "peak" => "Rush-hour traffic",
        "traffic" => "Traffic",
        "driver" => "Slow driving",
        "bus" => "Buses in poor condition",
        "event" => "Events on the line",
        _ => "Delays on the way",
    })
}

/// The tiers in words, each its own whole minutes (`economy::late_tier`: none in two).
pub fn tier_text(t: usize) -> String {
    let pc = |m: i32| format!("{:.0}", economy::late_share(m) * 100.0);
    let (from, five, full) = (economy::LATE_FROM, 5, economy::LATE_FULL);
    match t {
        1 => trf("%{from} to %{to} min late: %{share} %", &[("from", &from.to_string()), ("to", &five.to_string()), ("share", &pc(five))]),
        2 => trf("%{from} to %{to} min late: %{low} to %{high} %", &[("from", &(five + 1).to_string()), ("to", &(full - 1).to_string()), ("low", &pc(five + 1)), ("high", &pc(full - 1))]),
        _ => trf("%{from} min or more: the whole trip", &[("from", &full.to_string())]),
    }
}

/// The tone of a tier (on time: none).
pub fn tier_tone(t: usize) -> &'static str {
    match t {
        0 => "ok",
        1 => "warn",
        2 => "orange",
        _ => "danger",
    }
}

/// Why a trip was dropped, naming the bus or the driver missing and why it could not come
/// ("No bus: 104 · OHB-K 104 · Citaro, in the workshop"; "No driver: Jan Becker, ill").
pub fn dropped_text(c: &Company, why: DropWhy, problem: Option<Problem>, bus: Option<u32>, driver: Option<u32>) -> String {
    let p = problem.filter(|p| *p != Problem::Unassigned);
    let named = |who: Value| who.get("text").and_then(Value::as_str).map(str::to_string);
    match why {
        DropWhy::NoBus => match (named(bus_json(c, bus)), p) {
            (Some(b), Some(p)) => trf("No bus: %{bus} was planned, %{why}", &[("bus", &b), ("why", &tr(p.label()).to_lowercase())]),
            (None, Some(p)) => format!("{}: {}", tr(why.label()), tr(p.label())),
            _ => trf("No bus: %{why}", &[("why", &tr("none planned"))]),
        },
        DropWhy::NoDriver => match (named(driver_json(c, driver)), p) {
            (Some(d), Some(p)) => trf("No driver: %{name} was planned, %{why}", &[("name", &d), ("why", &tr(p.label()).to_lowercase())]),
            (None, Some(p)) => format!("{}: {}", tr(why.label()), tr(p.label())),
            _ => trf("No driver: %{why}", &[("why", &tr("none planned"))]),
        },
        DropWhy::Late => match named(driver_json(c, driver)) {
            Some(d) => trf("%{name} came late: the trips before they came were dropped", &[("name", &d)]),
            None => tr(why.label()),
        },
        DropWhy::BrokeDown | DropWhy::Crashed => match named(bus_json(c, bus)) {
            Some(b) => format!("{}: {}", tr(why.label()), b),
            None => tr(why.label()),
        },
        DropWhy::NoBlock => tr(why.label()),
    }
}

/// A late or dropped trip with everything the pages show of it.
pub fn outcome_json(c: &Company, o: &Outcome) -> Value {
    let block = block(&o.number, &o.tour);
    let duty = o.duty.map(|k| trf("Duty %{n}", &[("n", &(k + 1).to_string())]));
    let mut place = vec![trf("Line %{n}", &[("n", &o.number)]), trf("block %{b}", &[("b", &block)])];
    place.extend(duty.clone());
    if !o.trip.trim().is_empty() {
        place.push(trf("trip %{id}", &[("id", o.trip.trim())]));
    }
    let time = format!("{}–{}", hm(o.dep), hm(o.arr));
    let mut out = json!({
        "line": o.line, "number": o.number, "tour": o.tour, "block": block, "block_id": format!("B|{}|{}", o.line, o.tour),
        "duty": o.duty, "duty_text": duty, "dep": o.dep, "arr": o.arr, "time": time, "trip": o.trip,
        "place": place.join("  ·  "), "bus": bus_json(c, o.bus), "driver": driver_json(c, o.driver),
        "bus_caps": tr("Bus"), "driver_caps": tr("Driver"),
    });
    match o.dropped {
        Some(why) => {
            out["dropped"] = json!(true);
            out["label"] = json!("×");
            out["tone"] = json!("danger");
            out["head"] = json!(trf("%{dep} trip dropped", &[("dep", &hm(o.dep))]));
            out["why"] = json!(dropped_text(c, why, o.problem, o.bus, o.driver));
            out["chain"] = json!([dropped_text(c, why, o.problem, o.bus, o.driver)]);
        }
        None => {
            let tier = o.tier();
            let mut chain: Vec<String> = o.causes.iter().map(|x| cause_text(c, x)).collect();
            chain.extend(o.earlier.iter().map(|x| trf("before that: %{what}", &[("what", &cause_text(c, x))])));
            out["dropped"] = json!(false);
            out["minutes"] = json!(o.minutes);
            out["label"] = json!(format!("+{}", o.minutes));
            out["tier"] = json!(tier);
            out["tone"] = json!(tier_tone(tier));
            out["head"] = json!(trf("%{dep} trip ran %{m} min late", &[("dep", &hm(o.dep)), ("m", &o.minutes.to_string())]));
            out["why"] = json!(chain.join("; "));
            out["chain"] = json!(chain);
            out["deduction"] = json!(if o.value > 0 {
                trf("%{amount} deducted: %{share} of the trip's %{value}", &[("amount", &eur(o.deduction)), ("share", &format!("{:.0} %", o.share * 100.0)), ("value", &eur(o.value))])
            } else {
                tr("No deduction in this game mode")
            });
            if tier == 3 {
                out["lost"] = json!(tr("Counted as a trip lost"));
            }
        }
    }
    out
}

/// Advice as a page shows it: its words, its icon, its button and where that leads (`rf`:
/// RosterFlow's `openRosterFlow` options; `bus`, `person`: a bus's dialog, a person's card;
/// `go`: a page's key).
pub fn advice_json(c: &Company, a: &Advice) -> Value {
    let wd = co::plan::weekday_of(&co::dates::add(&c.date, -1));
    let day = (0..7).find(|k| co::plan::weekday_of(&co::dates::add(&c.date, *k)) == wd).unwrap_or(0);
    match a {
        Advice::Layover { line, number, tour, stop, at, minutes, trips } => json!({
            "icon": "schedule",
            "text": trf("Add %{m} min layover at %{stop} on line %{n}: %{k} trips carried a delay on from there (block %{block}, %{time}).", &[("m", &minutes.to_string()), ("stop", stop.trim()), ("n", number), ("k", &trips.to_string()), ("block", &block(number, tour)), ("time", &hm(*at))]),
            "buttons": [{ "label": tr("Open in RosterFlow"), "rf": { "view": 1, "day": day, "reveal": format!("B|{line}|{tour}") } }, { "label": tr("Open the Timeplan"), "rf": { "line": line } }],
        }),
        Advice::Running { line, number, from, to, minutes, trips, .. } => json!({
            "icon": "timer",
            "text": trf("Line %{n}: %{m} min more running time from %{from} to %{to}: %{k} trips ran late in the rush hour.", &[("n", number), ("m", &minutes.to_string()), ("from", &format!("{from:02}:00")), ("to", &format!("{to:02}:00")), ("k", &trips.to_string())]),
            "buttons": [{ "label": tr("Open the Timeplan"), "rf": { "line": line } }],
        }),
        Advice::Relief { line, number, tour, duty, at, driver, minutes } => json!({
            "icon": "swap_horiz",
            "text": trf("The relief at %{time} on block %{block} is too tight: %{name} came %{m} min late from the duty before. Plan more time between the two duties, or give it to another driver.", &[("time", &hm(*at)), ("block", &block(number, tour)), ("name", &name_of(c, *driver)), ("m", &minutes.to_string())]),
            "buttons": [{ "label": tr("Open in RosterFlow"), "rf": match duty { Some(k) => json!({ "view": 0, "day": day, "sel": { "duty": [line, tour, k] } }), None => json!({ "view": 1, "day": day, "reveal": format!("B|{line}|{tour}") }) } }],
        }),
        Advice::Driver { id, days, slow } => {
            let name = name_of(c, *id);
            let text = match (*days, *slow) {
                (d, 0) => trf("%{name} signed on late on %{d} of the last days: a punctuality course, a talk, or another duty that begins later.", &[("name", &name), ("d", &d.to_string())]),
                (0, s) => trf("%{name} drove %{s} trips late today: a punctuality course, or a duty with more time.", &[("name", &name), ("s", &s.to_string())]),
                (d, s) => trf("%{name} signed on late on %{d} of the last days and drove %{s} trips late today: a punctuality course, a talk, or another duty.", &[("name", &name), ("d", &d.to_string()), ("s", &s.to_string())]),
            };
            json!({
                "icon": "person",
                "text": text,
                "buttons": [{ "label": tr("Train the drivers"), "go": "training" }, { "label": tr("Show the person"), "person": id }, { "label": tr("Swap the duty"), "rf": { "view": 2, "day": day } }],
            })
        }
        Advice::Bus { id, breakdowns, condition, trips } => json!({
            "icon": "build",
            "text": trf("Bus %{bus} held %{k} trips up (condition %{c} %, %{b} breakdowns so far): give it a service.", &[("bus", &bus_no(c, *id)), ("k", &trips.to_string()), ("c", &format!("{condition:.0}")), ("b", &breakdowns.to_string())]),
            "buttons": [{ "label": tr("Show the bus"), "bus": id }],
        }),
    }
}

/// The advice of the last day closed (the Operations tab's, the report's): `late_advice`.
pub fn advice_view(s: &mut Session) -> Value {
    let Some(c) = s.company.as_ref() else { return Value::Null };
    let Some(r) = c.last_report.as_ref() else { return json!({ "list": [] }) };
    json!({ "caps": tr("Against late running").to_uppercase(), "list": lt::advice(c, &r.lateness).iter().map(|a| advice_json(c, a)).collect::<Vec<_>>() })
}

/// Punctuality as a share in words ("94 %"; no trip: "–").
fn pct(timed: u32, late: u32) -> String {
    if timed == 0 {
        "–".into()
    } else {
        format!("{:.0} %", 100.0 * (timed - late) as f64 / timed as f64)
    }
}

/// A share of trips on time from which it is good (green), and fair (amber); red under.
pub const ON_TIME_GOOD: f64 = 0.90;
pub const ON_TIME_FAIR: f64 = 0.75;

/// The tone of a punctuality: green from `ON_TIME_GOOD` on time, amber from `ON_TIME_FAIR`, red
/// under; no trip timed: none.
pub fn on_time_tone(timed: u32, late: u32) -> &'static str {
    if timed == 0 {
        return "dim";
    }
    let on_time = (timed - late.min(timed)) as f64 / timed as f64;
    if on_time >= ON_TIME_GOOD - 1e-9 {
        "ok"
    } else if on_time >= ON_TIME_FAIR - 1e-9 {
        "warn"
    } else {
        "danger"
    }
}

/// The share of a punctuality's trips late (0 to 1; none timed: 0).
fn late_share_of(p: &lt::Punct) -> f64 {
    if p.timed == 0 {
        0.0
    } else {
        p.late as f64 / p.timed as f64
    }
}

/// The minutes a punctuality's late trips ran late on average.
fn late_avg(p: &lt::Punct) -> f64 {
    if p.late == 0 {
        0.0
    } else {
        p.minutes as f64 / p.late as f64
    }
}

/// The punctualities by line, hour and duty as the report shows them (`report_part`): every
/// line; every hour from the first trip's to the last one's, an hour without trips "–"; the duties
/// with a trip late, the latest first - the share of their trips late, then how late they ran
/// on average -, at the most `DUTIES_SHOWN`.
pub fn punct_rows(all: &Lateness) -> (Vec<lt::Punct>, Vec<lt::Punct>, Vec<lt::Punct>) {
    let mut lines: Vec<lt::Punct> = all.lines.iter().filter(|p| p.timed > 0).cloned().collect();
    lines.sort_by(|a, b| a.key.cmp(&b.key));
    let timed: Vec<&lt::Punct> = all.hours.iter().filter(|p| p.timed > 0).collect();
    let hour = |p: &lt::Punct| p.key.parse::<i32>().unwrap_or(0);
    let mut hours: Vec<lt::Punct> = Vec::new();
    if let (Some(a), Some(z)) = (timed.iter().map(|p| hour(p)).min(), timed.iter().map(|p| hour(p)).max()) {
        for h in a..=z {
            hours.push(timed.iter().find(|p| hour(p) == h).map(|p| (*p).clone()).unwrap_or_else(|| lt::Punct { key: format!("{h:02}"), ..Default::default() }));
        }
    }
    let mut duties: Vec<lt::Punct> = all.duties.iter().filter(|p| p.late > 0).cloned().collect();
    duties.sort_by(|a, b| late_share_of(b).total_cmp(&late_share_of(a)).then(late_avg(b).total_cmp(&late_avg(a))).then(b.worst.cmp(&a.worst)).then(a.key.cmp(&b.key)));
    duties.truncate(DUTIES_SHOWN);
    (lines, hours, duties)
}

/// The duties the report lists at the most.
const DUTIES_SHOWN: usize = 8;

/// The day report's late running (`reports`: the days just closed): the late trips worst first
/// with their causes, punctuality by line, hour and duty, the deductions, and the advice.
pub fn report_part(c: &Company, reports: &[DayReport]) -> Value {
    let mut all = Lateness::default();
    for r in reports {
        let l = &r.lateness;
        all.trips.extend(l.trips.iter().cloned());
        for (to, from) in [(&mut all.lines, &l.lines), (&mut all.hours, &l.hours), (&mut all.duties, &l.duties)] {
            for p in from {
                match to.iter_mut().find(|x| x.key == p.key) {
                    Some(x) => (x.timed, x.late, x.minutes, x.worst) = (x.timed + p.timed, x.late + p.late, x.minutes + p.minutes, x.worst.max(p.worst)),
                    None => to.push(p.clone()),
                }
            }
        }
        for (to, from) in [(&mut all.tiers, &l.tiers), (&mut all.causes, &l.causes)] {
            for p in from {
                match to.iter_mut().find(|x| x.key == p.key) {
                    Some(x) => (x.trips, x.minutes, x.amount) = (x.trips + p.trips, x.minutes + p.minutes, x.amount + p.amount),
                    None => to.push(p.clone()),
                }
            }
        }
        let cy = &mut all.carried;
        (cy.trips, cy.minutes, cy.amount) = (cy.trips + l.carried.trips, cy.minutes + l.carried.minutes, cy.amount + l.carried.amount);
    }
    all.causes.sort_by_key(|s| (std::cmp::Reverse(s.amount), std::cmp::Reverse(s.trips)));
    if all.lines.iter().all(|p| p.timed == 0) {
        return Value::Null;
    }
    let mut worst = all.trips.clone();
    worst.sort_by_key(|o| std::cmp::Reverse(o.minutes));
    let shown: Vec<Value> = worst.iter().take(LATE_SHOWN).map(|o| outcome_json(c, o)).collect();
    let more = worst.len().saturating_sub(LATE_SHOWN);
    // (by line and by hour: the share on time, as every trip timed counts in both; by duty the
    // share late, as the list is of the duties late most)
    let row = |label: String, p: &lt::Punct| {
        let under = if p.timed == 0 { tr("No trips in this hour") } else { trf("%{n} of %{t} late", &[("n", &p.late.to_string()), ("t", &p.timed.to_string())]) };
        json!({ "label": label, "value": pct(p.timed, p.late), "under": under, "tone": on_time_tone(p.timed, p.late) })
    };
    let duty_row = |label: String, p: &lt::Punct| {
        json!({
            "label": label,
            "value": trf("%{p} % late", &[("p", &format!("{:.0}", late_share_of(p) * 100.0))]),
            "under": trf("%{n} of %{t} trips late, %{m} min late on average", &[("n", &p.late.to_string()), ("t", &p.timed.to_string()), ("m", &format!("{:.0}", late_avg(p)))]),
            "tone": on_time_tone(p.timed, p.late),
        })
    };
    let (lines, hours, duties) = punct_rows(&all);
    let duty_label = |p: &lt::Punct| {
        let mut parts = p.key.splitn(3, '/');
        let (n, t) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let who = p.driver.map(|d| name_of(c, d)).unwrap_or_default();
        format!("{} {}  ·  {}", block(n, t), trf("Duty %{n}", &[("n", &(p.duty.unwrap_or(0) + 1).to_string())]), who)
    };
    let last = reports.last();
    let advice: Vec<Value> = last.map(|r| lt::advice(c, &r.lateness).iter().map(|a| advice_json(c, a)).collect()).unwrap_or_default();
    json!({
        "caps": tr("Late running").to_uppercase(), "hint": tr("Click a trip for why"),
        "trips": shown, "more": (more > 0).then(|| trf("and %{n} more", &[("n", &more.to_string())])),
        "none": all.trips.is_empty().then(|| tr("No trip ran late.")),
        "by_line_caps": tr("On time by line"), "by_line": lines.iter().map(|p| row(trf("Line %{n}", &[("n", &p.key)]), p)).collect::<Vec<_>>(),
        "by_hour_caps": tr("On time by hour"), "by_hour": hours.iter().map(|p| row(format!("{}:00", p.key), p)).collect::<Vec<_>>(),
        "by_duty_caps": tr("Duties late most"), "by_duty": duties.iter().map(|p| duty_row(duty_label(p), p)).collect::<Vec<_>>(),
        "money": deductions_json(&all),
        "advice_caps": tr("Against late running").to_uppercase(), "advice": advice,
    })
}

/// The late trips the report lists at the most (the worst; the rest: "and n more").
const LATE_SHOWN: usize = 30;

/// The deductions by tier and cause, with how they were worked out: the same late trips, each
/// once in each column (`Lateness::finish`), so the two add up to the same.
pub fn deductions_json(l: &Lateness) -> Value {
    let total: i64 = l.tiers.iter().map(|t| t.amount).sum();
    json!({
        "caps": tr("Deductions for late running"), "total": eur(total),
        "tiers": l.tiers.iter().map(|t| { let k: usize = t.key.parse().unwrap_or(1); json!({ "label": tier_text(k), "trips": trf("%{n} trips", &[("n", &t.trips.to_string())]), "amount": eur(t.amount), "tone": tier_tone(k) }) }).collect::<Vec<_>>(),
        "causes": l.causes.iter().map(|s| json!({ "label": cause_kind(&s.key), "trips": trf("%{n} trips", &[("n", &s.trips.to_string())]), "amount": eur(s.amount) })).collect::<Vec<_>>(),
        "carried": (l.carried.trips > 0).then(|| trf("%{n} of these trips carried a delay on from the trip before: the layover was too short to take it up.", &[("n", &l.carried.trips.to_string())])),
        "how_label": tr("How was this worked out?"),
        "how": [
            trf("A trip counts as late from %{m} whole minutes behind the timetable, at its start or its end: %{under} min 59 s behind is still on time.", &[("m", &economy::LATE_FROM.to_string()), ("under", &(economy::LATE_FROM - 1).to_string())]),
            tr("Each late trip counts once in each column: in its tier, and under the cause that brought it the most minutes. A delay carried on from the trip before counts under the cause it began with (the traffic, a driver, a bus), less what the layovers took up on the way."),
            tr("Each late trip pays a share of its value under the contract (what dropping it would cost): 10 % up to 5 minutes, rising evenly to 100 % at 30 minutes; from 30 minutes the whole trip is deducted, as for a trip lost."),
            tr("A trip leaves when its bus is back from the trip before and its driver is there: a delay carries on along the block until a layover absorbs it. On the way the hour's traffic, the driver, the bus's condition and the day's events add minutes."),
            tr("Casual deducts nothing; school transport counts twice."),
        ],
    })
}

/// The finances' deductions for late running (the `late` dialog): the days kept, by tier and
/// cause, and each trip.
pub fn finance_view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let mut l = Lateness::default();
    let mut trips: Vec<(String, &Outcome)> = Vec::new();
    for d in &c.lateness.days {
        let late: Vec<Outcome> = d.trips.iter().filter(|o| o.dropped.is_none() && o.minutes >= economy::LATE_FROM).cloned().collect();
        let x = Lateness::default().finish(&late);
        for (to, from) in [(&mut l.tiers, &x.tiers), (&mut l.causes, &x.causes)] {
            for p in from {
                match to.iter_mut().find(|y| y.key == p.key) {
                    Some(y) => (y.trips, y.minutes, y.amount) = (y.trips + p.trips, y.minutes + p.minutes, y.amount + p.amount),
                    None => to.push(p.clone()),
                }
            }
        }
        let cy = &mut l.carried;
        (cy.trips, cy.minutes, cy.amount) = (cy.trips + x.carried.trips, cy.minutes + x.carried.minutes, cy.amount + x.carried.amount);
        trips.extend(d.trips.iter().filter(|o| o.deduction > 0).map(|o| (d.date.clone(), o)));
    }
    l.causes.sort_by_key(|s| std::cmp::Reverse(s.amount));
    trips.sort_by(|a, b| (b.0.as_str(), b.1.dep).cmp(&(a.0.as_str(), a.1.dep)));
    let first = c.lateness.days.first().map(|d| day_label(&d.date)).unwrap_or_default();
    json!({
        "title": tr("Deductions for late running"),
        "span": if c.lateness.days.is_empty() { tr("No day closed yet.") } else { trf("The last %{n} days closed, from %{from}", &[("n", &c.lateness.days.len().to_string()), ("from", &first)]) },
        "money": deductions_json(&l),
        "trips_caps": tr("Each deduction"),
        "trips": trips.iter().take(200).map(|(date, o)| { let mut v = outcome_json(&c, o); v["date"] = json!(day_label(date)); v["amount"] = json!(eur(o.deduction)); v }).collect::<Vec<_>>(),
        "close": tr("Close"),
    })
}

// --- RosterFlow --------------------------------------------------------------------------------

/// The trips of a day RosterFlow shows that ran late or were dropped: today's as the plan runs
/// (the company's clock), another day's those of the last day closed of its weekday. With the
/// day they are of, for the Issues panel's heading.
pub fn service(s: &Session, c: &Company, date: &str) -> Option<(String, bool, Vec<Outcome>)> {
    if date == c.date {
        let plan: &Plan = s.plan.as_ref()?;
        let now = c.clock.minute as i32;
        return Some((c.date.clone(), true, lt::today(c, plan, now)));
    }
    let wd = co::plan::weekday_of(date);
    let d = c.lateness.days.iter().rev().find(|d| co::plan::weekday_of(&d.date) == wd)?;
    Some((d.date.clone(), false, d.trips.clone()))
}

/// What RosterFlow's board shows of a day's service (`rf_board`'s `service`): each trip by its
/// block and departure, with its mark and its card.
pub fn service_json(s: &Session, c: &Company, date: &str) -> Value {
    let Some((of, live, trips)) = service(s, c, date) else { return Value::Null };
    let failed = trips.iter().filter(|o| o.dropped.is_some()).count();
    let late = trips.len() - failed;
    let caps = if live { tr("Today's service") } else { trf("Service of %{day}", &[("day", &day_label(&of))]) };
    // (the advice of the last day closed, against what made it late)
    let advice: Vec<Value> = c.last_report.as_ref().map(|r| lt::advice(c, &r.lateness).iter().map(|a| advice_json(c, a)).collect()).unwrap_or_default();
    json!({
        "advice_caps": tr("Against late running"), "advice": advice,
        "caps": caps, "live": live,
        "say": trf("%{f} trips dropped, %{l} late", &[("f", &failed.to_string()), ("l", &late.to_string())]),
        "none": trips.is_empty().then(|| tr("No trip dropped or late.")),
        "trips": trips.iter().map(|o| outcome_json(c, o)).collect::<Vec<_>>(),
    })
}

/// The day switcher's mark of today: its trips dropped so far (`agenda::rf_marks` alike).
pub fn rf_marks(s: &Session, c: &Company, mut marks: Vec<Value>) -> Vec<Value> {
    let Some((_, _, trips)) = service(s, c, &c.date) else { return marks };
    let failed = trips.iter().filter(|o| o.dropped.is_some()).count();
    if let Some(tags) = marks.first_mut().and_then(Value::as_array_mut).filter(|_| failed > 0) {
        tags.push(json!({ "text": trf("%{n} dropped", &[("n", &failed.to_string())]), "tone": "danger", "kind": "service" }));
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::model::Cents;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    /// Every text of this file in the six languages (`company-lateness.yml`).
    #[test]
    fn every_late_text_is_translated() {
        let src = include_str!("late.rs");
        let mut texts: Vec<String> = Vec::new();
        for lead in ["tr(\"", "trf(\""] {
            let mut rest = src;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let t: String = after.chars().take_while(|c| *c != '"').collect();
                if t.chars().any(char::is_alphabetic) {
                    texts.push(t);
                }
            }
        }
        assert!(texts.len() > 60, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsi-app/locales/company-lateness.yml):\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn on_time_is_green_from_ninety_percent_and_amber_from_seventy_five() {
        assert_eq!([on_time_tone(100, 0), on_time_tone(100, 10), on_time_tone(100, 11), on_time_tone(100, 25), on_time_tone(100, 26), on_time_tone(0, 0)], ["ok", "ok", "warn", "warn", "danger", "dim"]);
    }

    /// A day of line C1, blocks 02 and 03, two duties each (Luc's screenshot, smaller): each
    /// trip's block, duty, departure and minutes late; the late ones with their deductions (100
    /// EUR a trip), the cause each began with, and whether it carried a delay on.
    fn day() -> Lateness {
        let trips: [(&str, usize, i32, i32); 13] = [
            ("02", 0, 8 * 60, 0),
            ("02", 0, 8 * 60 + 30, 4),
            ("02", 0, 9 * 60, 7),
            ("02", 0, 9 * 60 + 30, 2),
            ("02", 1, 13 * 60, 0),
            ("02", 1, 13 * 60 + 30, 1),
            ("03", 0, 8 * 60 + 15, 3),
            ("03", 0, 8 * 60 + 45, 12),
            ("03", 0, 9 * 60 + 15, 31),
            ("03", 1, 13 * 60 + 15, 0),
            ("03", 1, 13 * 60 + 45, 5),
            ("03", 1, 14 * 60 + 15, 0),
            ("03", 1, 14 * 60 + 45, 0),
        ];
        let mut l = Lateness::default();
        let mut out = Vec::new();
        for (k, &(tour, duty, dep, minutes)) in trips.iter().enumerate() {
            l.count("C1", tour, duty, 7, dep, minutes);
            if minutes >= economy::LATE_FROM {
                let share = economy::late_share(minutes);
                let carried = k == 2 || k == 7;
                let causes = if carried { vec![Cause::Carried { dep: dep - 30, arr: dep - 5, stop: "Totnes Castle".into(), late: minutes + 2, layover: 5, deadhead: false, minutes }] } else { vec![Cause::Traffic { peak: true, minutes }] };
                out.push(Outcome { line: "C1".into(), number: "C1".into(), tour: tour.into(), duty: Some(duty), dep, arr: dep + 25, minutes, causes, value: 100_00, share, deduction: (100_00 as f64 * share).round() as Cents, root: if k == 8 { "relief".into() } else { "peak".into() }, ..Default::default() });
            }
        }
        l.finish(&out)
    }

    #[test]
    fn the_report_counts_each_trip_alike_by_line_hour_and_duty() {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Late".into(), game_mode: GameMode::Realistic, date: "2026-10-15".into(), ..Default::default() }, "Luc");
        let l = day();
        // by line, hour and duty: the same 13 trips timed, the same 6 late
        let (lines, hours, duties) = punct_rows(&l);
        let total = |v: &[lt::Punct]| (v.iter().map(|p| p.timed).sum::<u32>(), v.iter().map(|p| p.late).sum::<u32>());
        assert_eq!(total(&lines), (13, 6));
        assert_eq!(total(&hours), total(&lines));
        assert_eq!(total(&l.duties), total(&lines));
        assert_eq!(duties.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["C1/03/0", "C1/02/0", "C1/03/1"]);
        let r = DayReport { lateness: l.clone(), ..Default::default() };
        let v = report_part(&c, &[r]);
        let col = |list: &str, field: &str| v[list].as_array().unwrap().iter().map(|r| r[field].as_str().unwrap().to_string()).collect::<Vec<_>>();
        // the line: 7 of 13 on time, red
        assert_eq!((col("by_line", "value"), col("by_line", "tone")), (vec!["54 %".to_string()], vec!["danger".to_string()]));
        // every hour from the first trip's to the last one's; those without trips "–", not a red 0 %
        assert_eq!(col("by_hour", "label"), ["08:00", "09:00", "10:00", "11:00", "12:00", "13:00", "14:00"]);
        assert_eq!(col("by_hour", "value"), ["25 %", "33 %", "–", "–", "–", "75 %", "100 %"]);
        assert_eq!(col("by_hour", "tone"), ["danger", "danger", "dim", "dim", "dim", "warn", "ok"]);
        // the duties late most: the share of their trips late, the latest first; one with nothing
        // late is not listed
        let labels = col("by_duty", "label");
        assert_eq!(labels.len(), 3);
        for (label, block) in labels.iter().zip(["C1/03 Duty 1", "C1/02 Duty 1", "C1/03 Duty 2"]) {
            assert!(label.starts_with(block), "{label} for {block}");
        }
        assert_eq!(col("by_duty", "value"), ["100 % late", "50 % late", "25 % late"]);
        assert_eq!(col("by_duty", "tone"), ["danger", "danger", "warn"]);
        // the money: the tiers and the causes the same trips, each once, the same total
        let sum = |s: &[lt::Sum]| (s.iter().map(|x| x.trips).sum::<u32>(), s.iter().map(|x| x.amount).sum::<Cents>());
        assert_eq!(l.tiers.iter().map(|t| t.trips).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(sum(&l.tiers), (6, 3 * 10_00 + 17_20 + 35_20 + 100_00));
        assert_eq!(sum(&l.causes), sum(&l.tiers));
        assert_eq!(l.causes.iter().map(|s| (s.key.as_str(), s.trips)).collect::<Vec<_>>(), [("relief", 1), ("peak", 5)]);
        assert_eq!(v["money"]["total"], json!(eur(sum(&l.tiers).1)));
        let tiers: Vec<String> = v["money"]["tiers"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap().to_string()).collect();
        assert_eq!(tiers, ["3 to 5 min late: 10 %", "6 to 29 min late: 14 to 96 %", "30 min or more: the whole trip"]);
        // the trips that carried a delay on, as the advice for more layover counts them
        assert_eq!(l.carried.trips, 2);
        assert!(v["money"]["carried"].as_str().unwrap().starts_with("2 of these trips"));
        let advised: u32 = lt::advice(&c, &l).iter().map(|a| if let Advice::Layover { trips, .. } = a { *trips } else { 0 }).sum();
        assert_eq!(advised, l.carried.trips);
    }
}

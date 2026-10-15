//! The Operations page (Luc, 2026-10-15: "if vehicles are running late, the player should have
//! an option in the operations tab to adjust the vehicle on the route to get it back on time"):
//! service control on the company's day as its clock runs (`co::control`). Every bus running late
//! with its minutes, every late driver nobody covers and every breakdown or accident of today,
//! each with what it costs left as it is and the ways out - a short-turn, running out of service,
//! a shorter layover, a spare bus at the terminus; a colleague, a booked or a sent agency driver;
//! a spare or a rental bus -, each saying what it comes to before it is chosen. Casual's
//! controller acts by himself (said in the feed), Normal's suggests, Realistic leaves it to the
//! player. And when a bus late is told (`Clock::late_notice`).

use super::words::{eur, num, tr, trf};
use super::Session;
use omsi_launcher_lib::company::clock::{self as ck, Choice, Run};
use omsi_launcher_lib::company::control::{self as ctl, Act, Effect};
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, Fill};
use omsi_launcher_lib::company::{self as co, Company, GameMode};
use serde_json::{json, Value};

/// The page's place among the company's pages (`TABS`).
pub const OPS_TAB: usize = 15;
/// The notice's minutes to choose from (0: never).
const NOTICE_STEPS: [u32; 5] = [0, 3, 5, 10, 15];

fn hm(date: &str, m: i32) -> String {
    ck::hhmm(ck::moment(date, m as i64))
}

/// A change of reputation as the page writes it ("+0.1", "−0.3").
fn rep_text(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if r > 0.0 {
        format!("+{}", num(r, 1))
    } else if r < 0.0 {
        format!("−{}", num(-r, 1))
    } else {
        num(0.0, 1)
    }
}

fn bus_number(c: &Company, id: Option<u32>) -> String {
    id.and_then(|id| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_else(|| tr("Rental bus"))
}

/// What is left as it is, in words.
fn nothing_words(date: &str, e: &Effect) -> String {
    if e.late_trips == 0 && e.lost_trips == 0 {
        return match e.on_time_from {
            Some(m) => trf("Left as it is, it is back on time by itself from the %{time} trip.", &[("time", &hm(date, m))]),
            None => tr("Left as it is, it costs nothing more."),
        };
    }
    let mut parts = Vec::new();
    if e.late_trips > 0 {
        parts.push(trf("%{n} late trips ahead", &[("n", &e.late_trips.to_string())]));
    }
    if e.lost_trips > 0 {
        parts.push(trf("%{n} trips dropped", &[("n", &e.lost_trips.to_string())]));
    }
    trf("Left as it is: %{what}, %{amount} in penalties, %{rep} reputation.", &[("what", &parts.join(", ")), ("amount", &eur(e.penalty)), ("rep", &rep_text(-e.rep))])
}

/// What a way out comes to against doing nothing, in words.
fn option_words(date: &str, o: &Effect, base: &Effect) -> String {
    let mut parts = Vec::new();
    match o.on_time_from {
        Some(m) if base.late_trips > 0 || o.late_trips < base.late_trips => parts.push(trf("back on time from the %{time} trip", &[("time", &hm(date, m))])),
        _ if o.late_trips > 0 => parts.push(trf("still %{n} late trips ahead", &[("n", &o.late_trips.to_string())])),
        _ => {}
    }
    let pt = o.part_trips.saturating_sub(base.part_trips);
    if pt > 0 {
        parts.push(trf("%{n} part-trips lost (−%{amount})", &[("n", &pt.to_string()), ("amount", &eur(o.parts_cost - base.parts_cost))]));
    }
    let avoided = base.late_trips.saturating_sub(o.late_trips);
    if avoided > 0 {
        parts.push(trf("avoids %{n} late trips (−%{amount})", &[("n", &avoided.to_string()), ("amount", &eur(base.penalty - o.penalty))]));
    }
    let saved = base.lost_trips.saturating_sub(o.lost_trips);
    if saved > 0 {
        parts.push(trf("saves %{n} trips (−%{amount})", &[("n", &saved.to_string()), ("amount", &eur(base.penalty - o.penalty))]));
    }
    if o.lost_trips > 0 && saved > 0 {
        parts.push(trf("%{n} still dropped", &[("n", &o.lost_trips.to_string())]));
    }
    if o.cost > 0 {
        parts.push(trf("costs %{amount}", &[("amount", &eur(o.cost))]));
    }
    let rep = base.rep - o.rep;
    if rep.abs() >= 0.05 {
        parts.push(trf("%{rep} reputation against doing nothing", &[("rep", &rep_text(rep))]));
    }
    let mut s = parts.join("; ");
    if let Some(f) = s.get(..1) {
        s = f.to_uppercase() + &s[1..];
    }
    s
}

fn act_label(c: &Company, dp: &DayPlan, act: &Act, dep: i32) -> (String, &'static str) {
    let time = hm(&dp.date, dep);
    match act {
        Act::ShortTurn { part, .. } => (trf("Short-turn on the %{time} trip (%{share} % of it and of the next skipped)", &[("time", &time), ("share", &((part * 100.0).round() as i64).to_string())]), "u_turn_left"),
        Act::Express { .. } => (trf("Run out of service to the terminus on the %{time} trip", &[("time", &time)]), "bolt"),
        Act::Layover { saved } => (trf("Shorten the layover before the %{time} trip by %{min} min", &[("time", &time), ("min", &saved.to_string())]), "timer"),
        Act::Swap { vehicle, .. } => (trf("Swap in spare bus %{bus} at the terminus for the %{time} trip", &[("bus", &bus_number(c, Some(*vehicle))), ("time", &time)]), "sync_alt"),
        Act::Cover { .. } => (String::new(), "person"),
    }
}

/// The Operations page's view.
pub fn view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return json!({ "none": true }) };
    let mode = c.game_mode;
    let notice = ctl::notice_minutes(&c);
    let mut out = json!({
        "title": tr("Service control"),
        "now": trf("Company time %{time}", &[("time", &ck::hhmm(ck::now(&c)))]),
        "mode": match mode {
            GameMode::Casual => tr("Casual: the controller does the best by himself and says so in the feed."),
            GameMode::Normal => tr("Normal: the controller suggests the best way, one click away."),
            GameMode::Realistic => tr("Realistic: your choice, the controller suggests nothing."),
        },
        "notice": { "label": tr("Tell me when a bus is more than"), "value": notice, "options": NOTICE_STEPS.iter().map(|n| json!([n, if *n == 0 { tr("Never") } else { trf("%{n} min", &[("n", &n.to_string())]) }])).collect::<Vec<_>>() },
        "caps": { "buses": tr("Buses running late").to_uppercase(), "drivers": tr("Drivers late").to_uppercase(), "breaks": tr("Breakdowns and accidents").to_uppercase(), "done": tr("Done today").to_uppercase() },
        "empty": { "buses": tr("Every bus is on time."), "drivers": tr("No driver is late."), "breaks": tr("No breakdown today.") },
        "best": tr("Suggested"), "choose": tr("Do it"),
    });
    let dp = match super::plan_of(s, &c.date) {
        None => {
            out["reading"] = json!(tr("Reading the timetable…"));
            return out;
        }
        Some(Err(e)) => {
            out["error"] = json!(e);
            return out;
        }
        Some(Ok(p)) => p,
    };
    if c.clock.today.as_ref().is_none_or(|x| x.date != c.date) {
        out["idle"] = json!(tr("The company's day has not begun yet: run the clock and the buses go out."));
    }
    let lv = ctl::live(&c, &dp);
    out["buses"] = json!(lv.buses.iter().map(|b| bus_json(&c, &dp, b)).collect::<Vec<_>>());
    out["drivers"] = json!(lv.drivers.iter().map(|d| driver_json(&c, &dp, d)).collect::<Vec<_>>());
    out["breaks"] = json!(breaks_json(&c, &dp));
    out["done"] = json!(done_json(&c));
    out
}

fn bus_json(c: &Company, dp: &DayPlan, b: &ctl::LateBus) -> Value {
    let t = &dp.tours[b.ti];
    let trip = &t.tour.trips[b.trip];
    let tone = if b.minutes >= 10 { "err" } else { "warn" };
    let where_ = if b.running {
        trf("On the %{time} trip %{from} → %{to}", &[("time", &hm(&dp.date, trip.dep)), ("from", &trip.from), ("to", &trip.to)])
    } else {
        trf("Waits at %{stop} for the %{time} trip", &[("stop", &trip.from), ("time", &hm(&dp.date, trip.dep))])
    };
    let options: Vec<Value> = b
        .options
        .iter()
        .map(|o| {
            let (label, icon) = act_label(c, dp, &o.act, o.dep);
            json!({ "label": label, "icon": icon, "effect": option_words(&dp.date, &o.effect, &b.nothing), "best": o.best, "good": o.gain > 0.0,
                "args": { "line": t.tour.line, "tour": t.tour.tour, "dep": o.dep, "act": serde_json::to_value(o.act).unwrap_or(Value::Null) } })
        })
        .collect();
    json!({
        "key": pl::tour_key(&t.tour.line, &t.tour.tour),
        "title": trf("Bus %{bus} · line %{n} tour %{tour}", &[("bus", &bus_number(c, b.vehicle)), ("n", &t.tour.number), ("tour", &t.tour.tour)]),
        "minutes": trf("+%{n} min", &[("n", &b.minutes.to_string())]), "tone": tone, "where": where_,
        "nothing": nothing_words(&dp.date, &b.nothing), "options": options,
        "none": options.is_empty().then(|| tr("No way to bring it back sooner now: no slack, no spare bus free.")),
    })
}

fn driver_json(c: &Company, dp: &DayPlan, d: &ctl::LateDriver) -> Value {
    let t = &dp.tours[d.ti];
    let name = c.employee(d.employee).map(|e| e.name.clone()).unwrap_or_default();
    let covers: Vec<Value> = d
        .covers
        .iter()
        .map(|x| {
            let label = match x.fill {
                Fill::Colleague { id } => {
                    let who = c.employee(id).map(|e| if co::agency::is_booked(e.id) { trf("agency driver %{n} (booked)", &[("n", &co::agency::number_of(e).to_string())]) } else { e.name.clone() }).unwrap_or_default();
                    if x.standby {
                        trf("%{name} covers them now (standby)", &[("name", &who)])
                    } else {
                        trf("%{name} covers them now", &[("name", &who)])
                    }
                }
                _ => trf("The agency sends a driver (there at %{time})", &[("time", &hm(&dp.date, x.from))]),
            };
            let fill = match x.fill {
                Fill::Colleague { id } => json!({ "kind": "colleague", "id": id }),
                _ => json!({ "kind": "agency" }),
            };
            json!({ "label": label, "icon": if matches!(x.fill, Fill::Agency) { "badge" } else { "person" }, "effect": option_words(&dp.date, &x.effect, &d.nothing), "best": x.best, "good": true,
                "args": { "line": t.tour.line, "tour": t.tour.tour, "duty": d.duty, "fill": fill, "from": x.from } })
        })
        .collect();
    json!({
        "title": trf("%{name} · line %{n} tour %{tour}", &[("name", &name), ("n", &t.tour.number), ("tour", &t.tour.tour)]),
        "minutes": trf("+%{n} min", &[("n", &d.minutes.to_string())]), "tone": "warn",
        "where": trf("Comes at %{time}: the first trips of the duty wait for him", &[("time", &hm(&dp.date, d.until))]),
        "nothing": nothing_words(&dp.date, &d.nothing), "options": covers,
    })
}

/// Today's breakdowns and accidents: what was decided, or the choices with the trips each saves.
fn breaks_json(c: &Company, dp: &DayPlan) -> Vec<Value> {
    let Some(s) = c.clock.today.as_ref().filter(|s| s.date == c.date) else { return Vec::new() };
    let now = c.clock.minute as i32;
    let r = co::economy::rules(c.game_mode);
    let mut out = Vec::new();
    for (index, b) in s.breaks.iter().enumerate() {
        // (told when it comes, not before)
        if b.at > now {
            continue;
        }
        // the trips of its bus after it broke down, and what each choice loses of them
        let trips: Vec<(i32, i64)> = dp
            .tours
            .iter()
            .filter(|t| t.bus == Some(BusOf::Own(b.vehicle)))
            .flat_map(|t| t.tour.trips.iter().filter(|x| x.counts() && x.dep >= b.at).map(move |x| (x.dep, r.drop_per_trip + (x.km * r.drop_per_km as f64).round() as i64)))
            .collect();
        let lost = |back: Option<i32>, until: Option<i32>| -> Effect {
            let mut e = Effect::default();
            for &(dep, pen) in &trips {
                let covered = back.is_some_and(|x| dep >= x) && until.is_none_or(|u| dep < u);
                if !covered {
                    e.lost_trips += 1;
                    e.penalty += pen;
                }
            }
            e.rep = e.lost_trips as f64 * ctl::REP_DROP;
            e
        };
        let title = if b.accident { trf("Bus %{bus} had an accident · line %{n}", &[("bus", &b.bus), ("n", &b.line)]) } else { trf("Bus %{bus} broke down · line %{n}", &[("bus", &b.bus), ("n", &b.line)]) };
        let decided = match b.choice {
            Some(Choice::Spare { vehicle }) => Some(trf("Spare bus %{bus} runs its trips from %{time}.", &[("bus", &bus_number(c, Some(vehicle))), ("time", &hm(&s.date, b.back.unwrap_or(b.at)))])),
            Some(Choice::Rental) => Some(trf("A rental bus runs its trips from %{time}.", &[("time", &hm(&s.date, b.back.unwrap_or(b.at)))])),
            Some(Choice::Drop) => Some(tr("Its trips are dropped for the rest of the day.")),
            None => None,
        };
        let cut = b.cut();
        let now_effect = lost(cut.back, cut.until);
        let mut options = Vec::new();
        // (still open, or dropped: a bus can still be sent from now on)
        if matches!(b.choice, None | Some(Choice::Drop)) {
            let (spare_at, rental_at) = ck::arrivals(c, index);
            let base = lost(None, None);
            for x in ck::spare_offers(c, index).into_iter().take(4) {
                let e = lost(Some(spare_at), x.until);
                let label = super::spare_label(&bus_number(c, Some(x.vehicle)), &hm(&s.date, spare_at), x.until);
                out_option(&mut options, label, "directions_bus", option_words(&s.date, &e, &base), json!({ "index": index, "choice": "spare", "vehicle": x.vehicle }));
            }
            let mut e = lost(Some(rental_at), None);
            e.cost = ck::rental_cost(c, index);
            out_option(&mut options, trf("Rental bus from %{time}", &[("time", &hm(&s.date, rental_at))]), "airport_shuttle", option_words(&s.date, &e, &base), json!({ "index": index, "choice": "rental" }));
            if b.choice.is_none() {
                out_option(&mut options, tr("Drop its trips"), "close", nothing_words(&s.date, &base), json!({ "index": index, "choice": "drop" }));
            }
            // (the dispatcher's way first: Normal's suggestion)
            if c.game_mode != GameMode::Realistic {
                if let Some(o) = options.first_mut() {
                    o["best"] = json!(true);
                }
            }
        }
        out.push(json!({
            "title": title, "tone": if b.choice.is_none() { "err" } else { "warn" },
            "minutes": hm(&s.date, b.at),
            "where": decided.unwrap_or_else(|| tr("Nothing decided yet: its trips are lost until a bus is sent.")),
            "nothing": nothing_words(&s.date, &now_effect), "options": options,
        }));
    }
    out
}

fn out_option(list: &mut Vec<Value>, label: String, icon: &str, effect: String, args: Value) {
    list.push(json!({ "label": label, "icon": icon, "effect": effect, "best": false, "good": true, "args": args }));
}

/// What service control did today (the controller's own marked).
fn done_json(c: &Company) -> Vec<Value> {
    let Some(s) = c.clock.today.as_ref().filter(|s| s.date == c.date) else { return Vec::new() };
    s.controls
        .iter()
        .rev()
        .map(|x| {
            let what = match x.act {
                Act::ShortTurn { .. } => tr("Short-turn"),
                Act::Express { .. } => tr("Out of service to the terminus"),
                Act::Layover { .. } => tr("Shorter layover"),
                Act::Swap { vehicle, .. } => trf("Spare bus %{bus} swapped in", &[("bus", &bus_number(c, Some(vehicle)))]),
                Act::Cover { .. } => tr("Late driver covered"),
            };
            let who = if x.auto { tr("by the controller") } else { tr("by you") };
            json!({ "time": hm(&s.date, x.at), "text": format!("{what} · {} {} · {}", x.line, x.tour, who) })
        })
        .collect()
}

/// What the page does.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let r = match what {
        "ops_notice" => {
            let n = args.get("n").and_then(Value::as_u64).unwrap_or(ctl::NOTICE_MINUTES as u64) as u32;
            if let Some(c) = s.company.as_mut() {
                c.clock.late_notice = Some(n);
            }
            super::changed(s);
            Ok(json!({}))
        }
        "ops_apply" | "ops_cover" => with_plan(s, args, what == "ops_cover"),
        "ops_break" => {
            let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let choice = match args.get("choice").and_then(Value::as_str).unwrap_or("") {
                "spare" => Choice::Spare { vehicle: args.get("vehicle").and_then(Value::as_u64).unwrap_or(0) as u32 },
                "rental" => Choice::Rental,
                _ => Choice::Drop,
            };
            super::choose_break(s, index, choice);
            Ok(json!({}))
        }
        _ => return None,
    };
    Some(r)
}

/// A way out for a late bus (`ops_apply`) or a late driver (`ops_cover`) taken, on today's plan as
/// it is now.
fn with_plan(s: &mut Session, args: &Value, cover: bool) -> Result<Value, String> {
    let Some(date) = s.company.as_ref().map(|c| c.date.clone()) else { return Ok(json!({})) };
    let dp = match super::plan_of(s, &date) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(e),
        None => return Err(tr("Reading the timetable…")),
    };
    let line = args.get("line").and_then(Value::as_str).unwrap_or("");
    let tour = args.get("tour").and_then(Value::as_str).unwrap_or("");
    let Some(ti) = dp.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour) else { return Err(tr("That tour is not on today's plan any more.")) };
    let Some(c) = s.company.as_mut() else { return Ok(json!({})) };
    let mut run = Run::default();
    let done = if cover {
        let duty = args.get("duty").and_then(Value::as_u64).unwrap_or(0) as usize;
        let from = args.get("from").and_then(Value::as_i64).unwrap_or(c.clock.minute as i64) as i32;
        let fill = match args.get("fill").and_then(|f| f.get("kind")).and_then(Value::as_str) {
            Some("colleague") => Fill::Colleague { id: args["fill"].get("id").and_then(Value::as_u64).unwrap_or(0) as u32 },
            _ => Fill::Agency,
        };
        ctl::cover(c, &dp, ti, duty, fill, from, false, &mut run)
    } else {
        let dep = args.get("dep").and_then(Value::as_i64).unwrap_or(0) as i32;
        let act: Act = serde_json::from_value(args.get("act").cloned().unwrap_or(Value::Null)).map_err(|e| e.to_string())?;
        ctl::apply(c, &dp, ti, dep, act, false, &mut run)
    };
    let Some((text, a)) = done else { return Err(tr("That cannot be done now.")) };
    let said = super::words::filled(&text, &a);
    // (the plan changes with it: a cover is the planning's choice)
    s.plan = None;
    super::changed(s);
    s.say(said, false);
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text of service control - the core's, this page's, the start screen's tile - in the six
    /// languages.
    #[test]
    fn every_ops_text_is_translated() {
        let mut texts: Vec<String> = ctl::texts().into_iter().map(String::from).collect();
        for src in [include_str!("ops.rs")] {
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
        }
        texts.extend(["1 thing for service control", "%{n} things for service control", "1 bus running late", "%{n} buses running late", "Every bus on time", "Service control: bring late buses back on time", "Operations"].map(String::from));
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
        assert!(missing.is_empty(), "{} missing (crates/omsi-app/locales/company-ops.yml):
{}", missing.len(), missing.join("
"));
    }

    #[test]
    fn a_reputation_change_is_written_with_its_sign() {
        super::super::words::set_language("en");
        assert_eq!(rep_text(0.26), "+0.3");
        assert_eq!(rep_text(-1.0), "−1.0");
        assert_eq!(rep_text(0.01), "0.0");
    }
}

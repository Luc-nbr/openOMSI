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
    let mut buses: Vec<Value> = lv.buses.iter().map(|b| bus_json(&c, &dp, b)).collect();
    // (a bus the controller saw to and that runs on time again keeps its row: what was done)
    for (ti, t) in dp.tours.iter().enumerate() {
        let seen = ctl::of_day(&c, &dp.date).iter().any(|x| x.line == t.tour.line && x.tour == t.tour.tour && !matches!(x.act, Act::Cover { .. }));
        if seen && !lv.buses.iter().any(|b| b.ti == ti) {
            buses.push(mended_json(&c, &dp, ti));
        }
    }
    out["buses"] = json!(buses);
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
                "args": apply_args(&t.tour.line, &t.tour.tour, o.dep, &o.act) })
        })
        .collect();
    json!({
        "key": pl::tour_key(&t.tour.line, &t.tour.tour),
        "title": trf("Bus %{bus} · line %{n} tour %{tour}", &[("bus", &bus_number(c, b.vehicle)), ("n", &t.tour.number), ("tour", &t.tour.tour)]),
        "minutes": trf("+%{n} min", &[("n", &b.minutes.to_string())]), "tone": tone, "where": where_,
        "nothing": nothing_words(&dp.date, &b.nothing), "options": options,
        "none": options.is_empty().then(|| tr("No way to bring it back sooner now: no slack, no spare bus free.")),
        "done": done_of(c, &dp.date, &t.tour.line, &t.tour.tour, b.nothing.on_time_from.filter(|_| b.nothing.late_trips == 0)),
    })
}

/// What "Do it" sends for a way out of a late bus (`ops_apply`): the act under "how", never "act" -
/// that is the action's own name, which `kit.js`'s `act` would lose to it (Beta 59: "Do it" did
/// nothing).
fn apply_args(line: &str, tour: &str, dep: i32, act: &Act) -> Value {
    json!({ "line": line, "tour": tour, "dep": dep, "how": serde_json::to_value(act).unwrap_or(Value::Null) })
}

/// What service control did to a tour today, the last first ("Short-turn at 14:05 · back on time
/// from the 14:20 trip"; None: nothing).
fn done_of(c: &Company, date: &str, line: &str, tour: &str, back: Option<i32>) -> Option<String> {
    let x = ctl::of_day(c, date).into_iter().rev().find(|x| x.line == line && x.tour == tour && !matches!(x.act, Act::Cover { .. }))?;
    let mut s = trf("%{what} at %{time}", &[("what", &what_of(c, &x.act)), ("time", &hm(date, x.at))]);
    if let Some(m) = back {
        s = format!("{s} · {}", trf("back on time from the %{time} trip", &[("time", &hm(date, m))]));
    }
    Some(s)
}

/// A tour the controller saw to today and that runs on time now: its row stays, saying so.
fn mended_json(c: &Company, dp: &DayPlan, ti: usize) -> Value {
    let t = &dp.tours[ti];
    let bus = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    json!({
        "key": pl::tour_key(&t.tour.line, &t.tour.tour),
        "title": trf("Bus %{bus} · line %{n} tour %{tour}", &[("bus", &bus_number(c, bus)), ("n", &t.tour.number), ("tour", &t.tour.tour)]),
        "minutes": tr("On time"), "tone": "ok", "where": "", "nothing": "", "options": [],
        "done": done_of(c, &dp.date, &t.tour.line, &t.tour.tour, None),
    })
}

fn what_of(c: &Company, act: &Act) -> String {
    match *act {
        Act::ShortTurn { .. } => tr("Short-turn"),
        Act::Express { .. } => tr("Out of service to the terminus"),
        Act::Layover { .. } => tr("Shorter layover"),
        Act::Swap { vehicle, .. } => trf("Spare bus %{bus} swapped in", &[("bus", &bus_number(c, Some(vehicle)))]),
        Act::Cover { .. } => tr("Late driver covered"),
    }
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
                // (a spare line standing by: there once he comes from where it stands by)
                Fill::Spare { id } => trf("The driver of spare line %{spare} covers them (there at %{time})", &[("spare", &co::rosterflow::spare_label(c, id)), ("time", &hm(&dp.date, x.from))]),
                _ => trf("The agency sends a driver (there at %{time})", &[("time", &hm(&dp.date, x.from))]),
            };
            let fill = match x.fill {
                Fill::Colleague { id } => json!({ "kind": "colleague", "id": id }),
                Fill::Spare { id } => json!({ "kind": "spare", "id": id }),
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
            .flat_map(|t| {
                // (on a bus contract each journey lost is not paid; a commercial line loses its passengers)
                let value = co::bus_contracts::journey_value(c, &t.tour.line, None);
                t.tour.trips.iter().filter(|x| x.counts() && x.dep >= b.at).map(move |x| (x.dep, value))
            })
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
            let what = what_of(c, &x.act);
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
        "ops_apply" | "ops_cover" | "ops_break" => match take(s, what, args) {
            Ok((said, to)) => {
                super::changed(s);
                s.say(said.clone(), false);
                // (a breakdown's question answered: the simulation goes on where it was going)
                if let Some(to) = to.filter(|_| s.clock.speed == 0.0) {
                    super::simulate(s, to, false);
                }
                Ok(json!({ "said": said }))
            }
            Err(e) => {
                // (never silent: the reason in the page's corner)
                s.say(e.clone(), true);
                Ok(json!({ "error": e }))
            }
        },
        _ => return None,
    };
    Some(r)
}

/// A way out taken - for a late bus (`ops_apply`), a late driver (`ops_cover`) or a breakdown
/// (`ops_break`) - on today's plan as it is now, not saved yet: what to say, and where a
/// simulation waiting for a breakdown's answer was going. Err: why it cannot be done.
fn take(s: &mut Session, what: &str, args: &Value) -> Result<(String, Option<i64>), String> {
    if what == "ops_break" {
        let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let choice = match args.get("choice").and_then(Value::as_str).unwrap_or("") {
            "spare" => Choice::Spare { vehicle: args.get("vehicle").and_then(Value::as_u64).unwrap_or(0) as u32 },
            "rental" => Choice::Rental,
            _ => Choice::Drop,
        };
        let Some(c) = s.company.as_mut() else { return Err(tr("That cannot be done now.")) };
        let before = c.clock.today.as_ref().and_then(|x| x.breaks.get(index)).map(|b| b.choice);
        let Some(before) = before else { return Err(tr("That cannot be done now.")) };
        let to = ck::choose(c, index, choice);
        let after = c.clock.today.as_ref().and_then(|x| x.breaks.get(index)).and_then(|b| b.choice);
        if after == before {
            return Err(tr("That cannot be done now."));
        }
        let said = ck::break_message(c, index).map(|m| super::message_words(&m)).map(|(t, x)| format!("{t}: {x}")).unwrap_or_default();
        s.plan = None;
        s.plan_gen += 1;
        return Ok((said, to));
    }
    let cover = what == "ops_cover";
    let Some(date) = s.company.as_ref().map(|c| c.date.clone()) else { return Err(tr("That cannot be done now.")) };
    let dp = match super::plan_of(s, &date) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(e),
        None => return Err(tr("Reading the timetable…")),
    };
    let line = args.get("line").and_then(Value::as_str).unwrap_or("");
    let tour = args.get("tour").and_then(Value::as_str).unwrap_or("");
    let Some(ti) = dp.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour) else { return Err(tr("That tour is not on today's plan any more.")) };
    let Some(c) = s.company.as_mut() else { return Err(tr("That cannot be done now.")) };
    let mut run = Run::default();
    let done = if cover {
        let duty = args.get("duty").and_then(Value::as_u64).unwrap_or(0) as usize;
        let from = args.get("from").and_then(Value::as_i64).unwrap_or(c.clock.minute as i64) as i32;
        let fill = match args.get("fill").and_then(|f| f.get("kind")).and_then(Value::as_str) {
            Some("colleague") => Fill::Colleague { id: args["fill"].get("id").and_then(Value::as_u64).unwrap_or(0) as u32 },
            Some("spare") => Fill::Spare { id: args["fill"].get("id").and_then(Value::as_u64).unwrap_or(0) as u32 },
            _ => Fill::Agency,
        };
        ctl::cover(c, &dp, ti, duty, fill, from, false, &mut run)
    } else {
        let dep = args.get("dep").and_then(Value::as_i64).unwrap_or(0) as i32;
        // (the act under "how": "act" is the action's own name, `co_act`)
        let act: Act = serde_json::from_value(args.get("how").cloned().unwrap_or(Value::Null)).map_err(|_| tr("That cannot be done now."))?;
        ctl::apply(c, &dp, ti, dep, act, false, &mut run)
    };
    let Some((text, a)) = done else { return Err(tr("That cannot be done now.")) };
    let said = super::words::filled(&text, &a);
    // (the plan changes with it: a cover is the planning's choice)
    s.plan = None;
    s.plan_gen += 1;
    Ok((said, None))
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
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-ops.yml):
{}", missing.len(), missing.join("
"));
    }

    /// A company running line 5 (two tours of eight 50-minute trips from 06:00 and 06:30, the
    /// dispatcher filling the roster) with three buses and four drivers, its day begun at 07:00.
    fn running_day() -> (Session, DayPlan) {
        use omsi_launcher_lib::{LineInfo, StopInfo, TourInfo, TripInfo};
        let trip = |no: &str, k: i32, from: i32| {
            let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
            let dep = from + k * 60;
            TripInfo { name: format!("5 {no}-{k}"), index: 0, line: "5".into(), from: a.into(), terminus: b.into(), departure: dep as f64 * 60.0, arrival: (dep + 50) as f64 * 60.0, stops: (0..6).map(|s| StopInfo { name: format!("S{s}"), id: s, arr: 0.0, dep: 0.0 }).collect(), km: 20.0 }
        };
        let tour = |no: &str, from: i32| TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips: (0..8).map(|k| trip(no, k, from)).collect() };
        let l = LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 390)] };
        let mut c = co::found(&co::Founding { name: "Ops".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        c.lines.push(co::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        let bus = co::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..3 {
            co::market::buy_new(&mut c, &bus, co::market::Payment::Cash, "").unwrap();
        }
        let drivers = |c: &Company| c.staff.iter().filter(|e| co::staff::drives(e)).count();
        let id = c.id.clone();
        while drivers(&c) < 4 {
            for a in co::staff::applicants(&c) {
                if drivers(&c) < 4 && a.licence == co::model::Licence::D {
                    let _ = co::staff::hire(&mut c, &a);
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        c.planning.auto = true;
        let mut s = Session { company: Some(c), ..Default::default() };
        super::super::test_week(&mut s, vec![l]);
        let date = s.company.as_ref().unwrap().date.clone();
        let dp = super::super::plan_of(&mut s, &date).unwrap().unwrap();
        let c = s.company.as_mut().unwrap();
        c.clock.today = Some(ck::script(c, &dp));
        c.clock.minute = 7 * 60;
        s.plan_gen += 1;
        (s, dp)
    }

    #[test]
    fn do_it_changes_the_next_view_for_every_way_out() {
        super::super::words::set_language("en");
        let (mut s, dp) = running_day();
        assert!(dp.tours.len() == 2 && dp.tours.iter().all(|x| x.bus.is_some()), "both tours run");
        let bus_of = |k: usize| match dp.tours[k].bus {
            Some(BusOf::Own(id)) => id,
            _ => 0,
        };
        let spare = s.company.as_ref().unwrap().fleet.iter().map(|v| v.id).find(|id| *id != bus_of(0) && *id != bus_of(1)).unwrap();
        let (a, b) = (&dp.tours[0].tour, &dp.tours[1].tour);
        let ways = [
            (a, a.trips[1].dep, Act::ShortTurn { part: 0.2, saved: 15 }),
            (a, a.trips[2].dep, Act::Express { part: 0.5, saved: 6 }),
            (b, b.trips[2].dep, Act::Layover { saved: 3 }),
            (b, b.trips[3].dep, Act::Swap { vehicle: spare, from_bus: bus_of(1) }),
        ];
        let mut done = 0;
        for (t, dep, act) in ways {
            let args = apply_args(&t.line, &t.tour, dep, &act);
            // (the action's own name is never overwritten by the act: Beta 59's "Do it" did nothing)
            assert!(args.get("act").is_none());
            let before = view(&mut s);
            let (said, _) = take(&mut s, "ops_apply", &args).unwrap_or_else(|e| panic!("{act:?}: {e}"));
            assert!(!said.is_empty(), "{act:?} says what it did");
            let after = view(&mut s);
            done += 1;
            assert_eq!(after["done"].as_array().map(Vec::len), Some(done), "{act:?} is in Done today");
            assert_ne!(before["buses"], after["buses"], "{act:?} changes the bus's row");
            let key = pl::tour_key(&t.line, &t.tour);
            let row = after["buses"].as_array().unwrap().iter().find(|x| x["key"] == key.as_str()).expect("the bus keeps its row");
            assert!(row["done"].as_str().is_some_and(|d| d.contains(" at 07:00")), "{act:?}: {}", row["done"]);
        }
        // the controls are kept with the day, and the lateness model reads them
        let c = s.company.as_ref().unwrap();
        assert_eq!(ctl::of_day(c, &c.date).len(), 4);
        let d = ctl::delays(c, &dp, Some(7 * 60), None);
        assert!(d[0][1].lost > 0.0, "the short-turn's part lost");
        assert_eq!(d[1][3].bus, Some(spare), "the spare runs the trips from the swap");
        // a cover for a driver nobody is late for: a clear reason
        let no = take(&mut s, "ops_cover", &json!({ "line": a.line, "tour": a.tour, "duty": 0, "fill": { "kind": "agency" }, "from": 420 }));
        assert_eq!(no, Err("That cannot be done now.".to_string()));
        // a breakdown: a rental bus sent, said, and its card says so
        {
            let c = s.company.as_mut().unwrap();
            let v = bus_of(0);
            let number = c.vehicle(v).unwrap().number.clone();
            // (the day's own draws aside: this one alone)
            c.clock.today.as_mut().unwrap().breaks.clear();
            c.clock.today.as_mut().unwrap().breaks.push(ck::Broken { vehicle: v, bus: number, line: "5".into(), at: 6 * 60 + 50, until: 14 * 60, line_name: "Linie5".into(), choice: None, cost: 0, accident: false, back: None, spare_until: None });
        }
        let before = view(&mut s);
        let (said, _) = take(&mut s, "ops_break", &json!({ "index": 0, "choice": "rental" })).unwrap();
        assert!(said.to_lowercase().contains("rental"), "{said}");
        let after = view(&mut s);
        assert_ne!(before["breaks"][0]["where"], after["breaks"][0]["where"]);
        // and the same again: refused, with the reason
        assert!(take(&mut s, "ops_break", &json!({ "index": 0, "choice": "rental" })).is_err());
    }

    #[test]
    fn a_reputation_change_is_written_with_its_sign() {
        super::super::words::set_language("en");
        assert_eq!(rep_text(0.26), "+0.3");
        assert_eq!(rep_text(-1.0), "−1.0");
        assert_eq!(rep_text(0.01), "0.0");
    }
}

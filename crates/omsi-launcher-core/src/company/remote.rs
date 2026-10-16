//! The company on the phone: what the companion's "Company" tab shows of it, and the few
//! things a paired phone may order (build the next level of a depot area, a workshop job for
//! a bus).
//!
//! The phone talks to the game (the companion's server runs in it), and the company belongs
//! to the launcher, which keeps it open and saves it with every change. So the phone never
//! writes the company: an order it sends is checked against the company as saved (the same
//! rules the launcher applies, on a copy) and put in a queue beside it
//! (`companies/<id>.orders.jsonl`); the launcher takes the queue in (`take`), applies each
//! order with the same rules again and saves. Orders are plain JSON of a fixed shape - no
//! names of files, no free text.

use super::day::{self, Plan};
use super::depot::{self, Area, JobKind};
use super::crew::{self, DefectKind, NoteTemplate};
use super::duty_launch::DutyRef;
use super::holidays::Reason;
use super::model::Company;
use super::plan::Who;
use super::{alerts, store, Alert};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// What a phone may order - and the driver's own requests (`crew`), which the game's plugin
/// queues the same way for what the driver did in the bus.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Order {
    /// Build the next level of an area of the depot.
    Build { area: Area },
    /// A workshop job for a bus of the fleet.
    Job { bus: u32, job: JobKind },
    /// A handover note on a bus (a template, a text of at most `crew::TEXT_MAX`, or both).
    Note { bus: u32, template: Option<NoteTemplate>, text: String },
    /// A defect of a bus (the walk-around at sign-off; `fault`: logged by itself on the way).
    Defect { bus: u32, kind: DefectKind, text: String, fault: bool },
    /// Ask colleague `with` (an employee) to swap the player's duty `duty` of `line`/`tour` on
    /// `date`.
    Swap { date: String, line: String, tour: String, duty: usize, with: u32 },
    /// The player asks for days off.
    Leave { from: String, until: String, reason: Reason },
    /// The player calls in sick for `days` days from today, or is well again.
    Sick { days: u32 },
    Well,
}

/// An order from the phone's JSON, strictly: an object of exactly the fields of one order.
pub fn parse(v: &Value) -> Option<Order> {
    let o = v.as_object()?;
    let has = |keys: &[&str]| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k));
    match o.get("do")?.as_str()? {
        "build" if has(&["do", "area"]) => Some(Order::Build { area: Area::from_key(o.get("area")?.as_str()?)? }),
        "job" if has(&["do", "bus", "job"]) => Some(Order::Job { bus: bus_of(o)?, job: JobKind::from_key(o.get("job")?.as_str()?)? }),
        "note" if has(&["do", "bus", "template", "text"]) => {
            let template = match o.get("template")? {
                Value::Null => None,
                t => Some(NoteTemplate::from_key(t.as_str()?)?),
            };
            let text = short_text(o.get("text")?)?;
            (template.is_some() || !text.is_empty()).then_some(Order::Note { bus: bus_of(o)?, template, text })
        }
        "defect" if has(&["do", "bus", "kind", "text", "fault"]) => Some(Order::Defect { bus: bus_of(o)?, kind: DefectKind::from_key(o.get("kind")?.as_str()?)?, text: short_text(o.get("text")?)?, fault: o.get("fault")?.as_bool()? }),
        "swap" if has(&["do", "date", "line", "tour", "duty", "with"]) => {
            let date = day_of(o.get("date")?)?;
            let name = |k: &str| o.get(k)?.as_str().filter(|x| !x.is_empty() && x.len() <= 64 && !x.chars().any(char::is_control)).map(str::to_string);
            let duty = o.get("duty")?.as_u64().filter(|n| *n < 100)? as usize;
            let with = o.get("with")?.as_u64().filter(|n| *n < 1_000_000)? as u32;
            Some(Order::Swap { date, line: name("line")?, tour: name("tour")?, duty, with })
        }
        "leave" if has(&["do", "from", "until", "reason"]) => {
            let reason = Reason::ALL.into_iter().find(|r| serde_json::to_value(r).ok().as_ref() == o.get("reason"))?;
            Some(Order::Leave { from: day_of(o.get("from")?)?, until: day_of(o.get("until")?)?, reason })
        }
        "sick" if has(&["do", "days"]) => Some(Order::Sick { days: o.get("days")?.as_u64().filter(|n| (1..=crew::SICK_MAX as u64).contains(n))? as u32 }),
        "well" if has(&["do"]) => Some(Order::Well),
        _ => None,
    }
}

/// A bus's id of an order.
fn bus_of(o: &serde_json::Map<String, Value>) -> Option<u32> {
    Some(o.get("bus")?.as_u64().filter(|n| *n < 1_000_000)? as u32)
}

/// A typed text of an order: a string of at most `crew::TEXT_MAX` characters, kept without
/// control characters.
fn short_text(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    (s.chars().count() <= crew::TEXT_MAX).then(|| super::together::book::clean(s, crew::TEXT_MAX))
}

/// A day of an order (`yyyy-mm-dd`).
fn day_of(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    super::dates::parse(s).map(super::dates::fmt).filter(|d| d == s)
}

/// Carry out an order (the launcher's side).
pub fn apply(c: &mut Company, o: &Order) -> Result<(), &'static str> {
    let by = c.profile.clone();
    match o {
        Order::Build { area } => depot::build(c, *area),
        Order::Job { bus, job } => depot::order(c, *bus, *job).map(|_| ()),
        Order::Note { bus, template, text } => crew::add_note(c, *bus, &by, *template, text).map(|_| ()),
        Order::Defect { bus, kind, text, fault } => crew::report_defect(c, *bus, *kind, text, if *fault { "" } else { &by }, *fault).map(|_| ()),
        Order::Swap { date, line, tour, duty, with } => {
            let p = plan_on(c, date).ok_or(crew::NO_DUTY)?;
            let r = DutyRef { date: date.clone(), line: line.clone(), tour: tour.clone(), duty: *duty };
            crew::ask_swap(c, &p, Who::Player, &r, Who::Staff(*with)).map(|_| ())
        }
        Order::Leave { from, until, reason } => crew::ask_leave(c, Who::Player, from, until, *reason).map(|_| ()),
        Order::Sick { days } => crew::call_in_sick(c, Who::Player, *days).map(|_| ()),
        Order::Well => {
            crew::back_at_work(c);
            Ok(())
        }
    }
}

/// The company's plan of `date` (today or a later day of the week ahead), from the map's
/// timetable of that date.
pub fn plan_on(c: &Company, date: &str) -> Option<super::plan::DayPlan> {
    if super::dates::between(&c.date, date) < 0 {
        return None;
    }
    let lines = crate::list_lines(&c.map, date).ok()?;
    let tours = super::network::tours_of_day(c, &lines, date);
    Some(super::plan::day_plan(c, date, tours, &[], &[], false))
}

/// Whether an order would be carried out, after the ones still waiting: on a copy.
pub fn check(c: &Company, waiting: &[Order], o: &Order) -> Result<(), &'static str> {
    let mut x = c.clone();
    for w in waiting {
        let _ = apply(&mut x, w);
    }
    apply(&mut x, o)
}

pub fn orders_file(data: &Path, id: &str) -> PathBuf {
    store::dir(data).join(format!("{id}.orders.jsonl"))
}

/// Put an order in the company's queue.
pub fn queue(data: &Path, id: &str, o: &Order) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(store::dir(data))?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(orders_file(data, id))?;
    writeln!(f, "{}", serde_json::to_string(o)?)?;
    Ok(())
}

/// The orders of a queue's file.
fn orders_in(path: &Path) -> Vec<Order> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter_map(|v| parse(&v)).collect()
}

/// The orders waiting for the launcher.
pub fn pending(data: &Path, id: &str) -> Vec<Order> {
    orders_in(&orders_file(data, id))
}

/// Take the queue in: every order applied (or refused), the queue emptied.
pub fn take(data: &Path, c: &mut Company) -> Vec<(Order, Result<(), &'static str>)> {
    let path = orders_file(data, &c.id);
    if !path.is_file() {
        return Vec::new();
    }
    // (moved aside first: an order the phone sends meanwhile goes to a new queue, not lost)
    let taking = path.with_extension("jsonl.taking");
    if std::fs::rename(&path, &taking).is_err() {
        return Vec::new();
    }
    let orders = orders_in(&taking);
    let _ = std::fs::remove_file(&taking);
    orders.into_iter().map(|o| {
        let r = apply(c, &o);
        (o, r)
    }).collect()
}

/// The company saved last (the one the launcher has open), if any.
pub fn latest(data: &Path) -> Option<Company> {
    let rd = std::fs::read_dir(store::dir(data)).ok()?;
    // (the companies' own files only: an id has no dots, `<id>.liveplan.json` lies beside them)
    let mut saved: Vec<(std::time::SystemTime, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".json").filter(|id| !id.contains('.'))?.to_string();
            Some((e.metadata().ok()?.modified().ok()?, id))
        })
        .collect();
    saved.sort_by(|a, b| b.0.cmp(&a.0));
    saved.into_iter().find_map(|(_, id)| store::load(data, &id).ok())
}

/// The company's plan of today, from the map's timetable of its date: the roster's, as the
/// launcher's planning has it (`plan::day_plan`).
pub fn plan_of(c: &Company) -> Option<Plan> {
    let (_, tours) = store::tours_today(c).ok()?;
    Some(super::plan::day_plan(c, &c.date, tours, &[], &[], false).to_plan())
}

fn hhmm(minutes: i32) -> String {
    format!("{:02}:{:02}", (minutes / 60).rem_euclid(24), minutes.rem_euclid(60))
}

/// What the phone's tab shows: money, today's dispositions, the depot and its workshop, the
/// buses that want work, the orders waiting. Amounts in cents.
pub fn summary(c: &Company, plan: Option<&Plan>, waiting: &[Order]) -> Value {
    let today = &c.date;
    let month = super::dates::month_of(today);
    let m = c.month(&month);
    let months: Vec<Value> = c.months.iter().rev().take(6).rev().map(|m| json!({ "month": m.month, "income": m.income(), "expenses": m.expenses(), "result": m.result() })).collect();
    let days: Vec<Value> = c.history.iter().rev().take(14).rev().map(|d| json!({ "date": d.date, "result": d.result })).collect();
    let held: Vec<&super::Vehicle> = c.fleet.iter().filter(|v| v.held_on(today)).collect();
    let in_workshop = held.iter().filter(|v| v.in_workshop(today)).count();
    let ours: Vec<u32> = c.site.jobs.iter().filter(|j| j.started.is_some()).map(|j| j.vehicle).collect();
    let breakdowns: Vec<Value> = held.iter().filter(|v| v.in_workshop(today) && !ours.contains(&v.id)).map(|v| json!({ "number": v.number, "until": v.workshop_until })).collect();
    let today_v = plan.map(|p| {
        // (a line not in service runs nothing: its tours are not the day's)
        let running: Vec<&day::TourPlan> = p.tours.iter().filter(|t| !t.tour.unplanned).collect();
        let open: Vec<Value> = running
            .iter()
            .filter(|t| !t.covered())
            .take(30)
            .map(|t| json!({ "line": t.tour.number, "tour": t.tour.tour, "from": hhmm(t.tour.from()), "to": hhmm(t.tour.to()), "why": if t.bus.is_none() { "bus" } else { "driver" } }))
            .collect();
        let (buses, duties) = p.short_of();
        json!({ "tours": running.len(), "covered": running.len() - p.uncovered(), "open": open, "no_bus": buses, "no_driver": duties })
    });
    let al: Vec<Value> = alerts(c, plan)
        .into_iter()
        .map(|a| match a {
            Alert::NoLines => json!({ "kind": "no_lines" }),
            Alert::NoBuses => json!({ "kind": "no_buses" }),
            Alert::NoDrivers => json!({ "kind": "no_drivers" }),
            Alert::Uncovered { tours, .. } => json!({ "kind": "uncovered", "n": tours }),
            Alert::LowCash => json!({ "kind": "low_cash" }),
            Alert::ServiceDue(n) => json!({ "kind": "service_due", "n": n }),
            Alert::Unhappy(n) => json!({ "kind": "unhappy", "n": n }),
            Alert::GoingBack { number, until } => json!({ "kind": "going_back", "number": number, "until": until }),
            Alert::NotPlanned(lines) => json!({ "kind": "not_planned", "lines": lines }),
            Alert::Tomorrow { tours, buses, duties } => json!({ "kind": "tomorrow", "tours": tours, "buses": buses, "duties": duties }),
        })
        .collect();
    let areas: Vec<Value> = Area::ALL
        .iter()
        .map(|a| {
            let next = depot::next_cost(c, *a);
            let works = c.site.works_on(*a).map(|w| w.until.clone());
            let can = check(c, waiting, &Order::Build { area: *a }).is_ok();
            json!({ "key": a.key(), "label": a.label(), "level": c.site.level(*a), "max": a.max(), "cost": next.map(|x| x.0), "days": next.map(|x| x.1), "building_until": works, "allowed": depot::area_allowed(c, *a), "can": can })
        })
        .collect();
    let jobs: Vec<Value> = c
        .site
        .jobs
        .iter()
        .map(|j| json!({ "id": j.id, "number": c.vehicle(j.vehicle).map(|v| v.number.clone()).unwrap_or_default(), "job": j.kind.label(), "started": j.started, "until": j.until, "cost": j.cost }))
        .collect();
    let buses: Vec<Value> = held
        .iter()
        .filter(|v| c.site.job_of(v.id).is_none() && !v.in_workshop(today))
        .filter(|v| v.condition < 75.0 || v.km >= v.next_service_km - 1_000.0)
        .take(20)
        .map(|v| {
            let due = v.km >= v.next_service_km - 1_000.0;
            let job = if due { JobKind::Service } else { JobKind::Repair };
            json!({ "id": v.id, "number": v.number, "name": v.name, "condition": v.condition.round(), "due": due, "job": job.key(), "job_label": job.label(), "cost": depot::job_cost(c, v.id, job) })
        })
        .collect();
    let pend: Vec<Value> = waiting.iter().map(|o| serde_json::to_value(o).unwrap_or(Value::Null)).collect();
    json!({
        "id": c.id,
        "name": c.name,
        "short": c.short,
        "colour": c.colours[0],
        "date": c.date,
        "difficulty": c.game_mode.label(),
        "cash": c.cash,
        "debt": c.debt(),
        "reputation": c.reputation.round(),
        "punctuality": c.punctuality.round(),
        "clean": c.site.clean.round(),
        "month": { "month": month, "income": m.income(), "expenses": m.expenses(), "result": m.result() },
        "months": months,
        "days": days,
        "fleet": { "buses": held.len(), "workshop": in_workshop, "spaces": depot::places(c), "outside": depot::outside(c), "bays": c.site.bays() },
        "staff": { "people": c.staff.iter().filter(|e| e.employed_on(today)).count(), "absent": c.staff.iter().filter(|e| e.employed_on(today) && e.absent(today)).count() },
        "today": today_v,
        "breakdowns": breakdowns,
        "alerts": al,
        "areas": areas,
        "upkeep": depot::upkeep_month(c),
        "jobs": jobs,
        "buses": buses,
        "pending": pend,
        "concessions": c.concessions.held.iter().map(|h| json!({ "number": h.number, "until": h.until, "price": h.price })).collect::<Vec<_>>(),
        "tenders": c.concessions.tenders.iter().filter(|t| t.open()).map(|t| json!({ "number": t.number, "closes": t.closes, "closes_at": super::clock::hhmm(t.closes_at), "bid": serde_json::Value::Null, "offer": t.offers.last().map(|o| o.1) })).collect::<Vec<_>>(),
    })
}

/// The driver's side for the phone (`crew`): his week as the plans of its days have it (`plans`,
/// today's first; a day ill or on holiday said), his swap requests and the colleagues he may ask,
/// his holidays, a sick call running, the bus he drives today with its notes and defects, and
/// the choices of the forms.
pub fn crew_summary(c: &Company, plans: &[super::plan::DayPlan]) -> Value {
    let mut week: Vec<Value> = Vec::new();
    let mut bus_today: Option<u32> = None;
    for p in plans {
        let mut duties: Vec<Value> = Vec::new();
        for (ti, k) in p.duties_of(Who::Player) {
            let t = &p.tours[ti];
            let d = &t.duties[k];
            let bus = match t.bus {
                Some(super::plan::BusOf::Own(id)) => {
                    if p.date == c.date && bus_today.is_none() && d.to > c.clock.minute as i32 {
                        bus_today = Some(id);
                    }
                    c.vehicle(id).map(|v| v.number.clone())
                }
                _ => None,
            };
            let changes: Vec<Value> = crew::changes(c, p, ti, k).iter().map(|ch| {
                let (text, args) = ch.words();
                json!({ "text": text, "args": args.into_iter().collect::<std::collections::BTreeMap<_, _>>() })
            }).collect();
            duties.push(json!({ "line": t.tour.line, "number": t.tour.number, "tour": t.tour.tour, "duty": k, "from": hhmm(d.from), "to": hhmm(d.to), "bus": bus, "begun": p.date == c.date && d.from <= c.clock.minute as i32, "changes": changes }));
        }
        let off = crew::player_off(c, &p.date).map(|x| x.label());
        week.push(json!({ "date": p.date, "weekday": super::plan::weekday_of(&p.date), "off": off, "duties": duties }));
    }
    let colleagues: Vec<Value> = c
        .staff
        .iter()
        .filter(|e| super::staff::drives(e) && e.employed_on(&c.date) && e.notice_until.is_none())
        .map(|e| json!({ "id": e.id, "name": e.name }))
        .collect();
    let swaps: Vec<Value> = c
        .crew
        .swaps
        .iter()
        .rev()
        .take(12)
        .map(|s| json!({ "no": s.no, "date": s.mine.date, "line": s.mine.line, "tour": s.mine.tour, "duty": s.mine.duty, "with": crew_name(c, s.with), "state": s.state, "why": s.why, "theirs": s.theirs.as_ref().map(|t| format!("{} {} #{}", t.line, t.tour, t.duty + 1)) }))
        .collect();
    let leave: Vec<Value> = c.crew.leave.requests.iter().rev().take(8).map(|r| json!({ "no": r.no, "from": r.from, "until": r.until, "reason": r.reason.label(), "state": r.state })).collect();
    let bus = bus_today.and_then(|id| c.vehicle(id)).map(|v| {
        let notes: Vec<Value> = crew::notes_for(c, v.id).iter().map(|n| json!({ "says": n.says(), "by": n.by, "at": hhmm(n.at) })).collect();
        let defects: Vec<Value> = crew::defects_of(c, v.id).iter().filter(|d| d.open(c)).map(|d| json!({ "kind": d.kind.label(), "text": d.text, "date": d.date })).collect();
        json!({ "id": v.id, "number": v.number, "name": v.name, "notes": notes, "defects": defects })
    });
    json!({
        "week": week,
        "colleagues": colleagues,
        "swaps": swaps,
        "leave": leave,
        "reasons": Reason::ALL.iter().map(|r| json!({ "key": r, "label": r.label() })).collect::<Vec<_>>(),
        "sick_until": c.crew.sick_until,
        "sick_max": crew::SICK_MAX,
        "bus": bus,
        "templates": NoteTemplate::ALL.iter().map(|t| json!({ "key": t.key(), "label": t.text() })).collect::<Vec<_>>(),
        "defects": DefectKind::ALL.iter().map(|k| json!({ "key": k.key(), "label": k.label() })).collect::<Vec<_>>(),
        "text_max": crew::TEXT_MAX,
    })
}

fn crew_name(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_default(),
        Some(Who::Guest(n)) => c.together.name_of(n),
        _ => String::new(),
    }
}

/// The plans of the week ahead from the company's day (seven days, today's first; a day whose
/// timetable cannot be read is left out).
pub fn week_plans(c: &Company) -> Vec<super::plan::DayPlan> {
    (0..7).filter_map(|n| plan_on(c, &super::dates::add(&c.date, n))).collect()
}

#[cfg(test)]
mod tests {
    use super::super::market::{self, MarketBus, Payment};
    use super::super::{found, Founding};
    use super::*;

    #[test]
    fn only_orders_of_the_exact_shape_are_taken() {
        assert_eq!(parse(&json!({ "do": "build", "area": "wash" })), Some(Order::Build { area: Area::Wash }));
        assert_eq!(parse(&json!({ "do": "job", "bus": 3, "job": "repair" })), Some(Order::Job { bus: 3, job: JobKind::Repair }));
        for bad in [
            json!({ "do": "build", "area": "wash", "file": "x" }),
            json!({ "do": "build", "area": "../x" }),
            json!({ "do": "job", "bus": -1, "job": "repair" }),
            json!({ "do": "job", "bus": "3", "job": "repair" }),
            json!({ "do": "sell", "bus": 3 }),
            json!(["build"]),
            json!("build"),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
        // what is queued reads back the same
        let o = Order::Job { bus: 7, job: JobKind::Overhaul };
        assert_eq!(parse(&serde_json::to_value(&o).unwrap()), Some(o));
    }

    #[test]
    fn an_order_is_checked_queued_and_carried_out_by_the_launcher() {
        let data = std::env::temp_dir().join(format!("openomsi-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut c = found(&Founding { name: "Fern".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        let id = market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        store::save(&data, &c).unwrap();
        assert_eq!(latest(&data).map(|x| x.id), Some(c.id.clone()));
        // (the day's plan for the game, written after it beside it, is no company)
        super::super::plan::save_live_plan(&data, &super::super::plan::LivePlan { company: c.id.clone(), ..Default::default() }).unwrap();
        assert_eq!(latest(&data).map(|x| x.id), Some(c.id.clone()));
        let wash = Order::Build { area: Area::Wash };
        assert!(check(&c, &[], &wash).is_ok());
        // (a second order of the same build waits behind the first and would be refused)
        assert_eq!(check(&c, std::slice::from_ref(&wash), &wash), Err("This is being built already."));
        queue(&data, &c.id, &wash).unwrap();
        queue(&data, &c.id, &Order::Job { bus: id, job: JobKind::Service }).unwrap();
        assert_eq!(pending(&data, &c.id).len(), 2);
        let s = summary(&c, None, &pending(&data, &c.id));
        assert_eq!(s["pending"].as_array().map(|a| a.len()), Some(2));
        assert_eq!(s["areas"][2]["key"], "wash");
        assert_eq!(s["areas"][2]["can"], false);
        let done = take(&data, &mut c);
        assert!(done.iter().all(|x| x.1.is_ok()));
        assert!(c.site.works_on(Area::Wash).is_some() && c.site.job_of(id).is_some());
        assert!(pending(&data, &c.id).is_empty() && take(&data, &mut c).is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn the_drivers_orders_are_taken_only_of_their_exact_shape() {
        assert_eq!(parse(&json!({ "do": "note", "bus": 3, "template": "door_sticks", "text": "rear" })), Some(Order::Note { bus: 3, template: Some(NoteTemplate::DoorSticks), text: "rear".into() }));
        assert_eq!(parse(&json!({ "do": "note", "bus": 3, "template": null, "text": "a\u{7}b" })), Some(Order::Note { bus: 3, template: None, text: "ab".into() }));
        assert_eq!(parse(&json!({ "do": "defect", "bus": 3, "kind": "tyres", "text": "", "fault": false })), Some(Order::Defect { bus: 3, kind: DefectKind::Tyres, text: String::new(), fault: false }));
        assert_eq!(parse(&json!({ "do": "swap", "date": "2024-03-05", "line": "Linie5", "tour": "2", "duty": 0, "with": 4 })), Some(Order::Swap { date: "2024-03-05".into(), line: "Linie5".into(), tour: "2".into(), duty: 0, with: 4 }));
        assert_eq!(parse(&json!({ "do": "leave", "from": "2024-03-10", "until": "2024-03-12", "reason": "family" })), Some(Order::Leave { from: "2024-03-10".into(), until: "2024-03-12".into(), reason: Reason::Family }));
        assert_eq!(parse(&json!({ "do": "sick", "days": 2 })), Some(Order::Sick { days: 2 }));
        assert_eq!(parse(&json!({ "do": "well" })), Some(Order::Well));
        for bad in [
            json!({ "do": "note", "bus": 3, "template": null, "text": "" }),
            json!({ "do": "note", "bus": 3, "template": "x", "text": "a" }),
            json!({ "do": "note", "bus": 3, "text": "a" }),
            json!({ "do": "note", "bus": 3, "template": null, "text": "x".repeat(crew::TEXT_MAX + 1) }),
            json!({ "do": "defect", "bus": 3, "kind": "engine", "text": "", "fault": false }),
            json!({ "do": "swap", "date": "2024-02-30", "line": "Linie5", "tour": "2", "duty": 0, "with": 4 }),
            json!({ "do": "swap", "date": "2024-03-05", "line": "", "tour": "2", "duty": 0, "with": 4 }),
            json!({ "do": "leave", "from": "2024-03-10", "until": "2024-03-12", "reason": "beach" }),
            json!({ "do": "sick", "days": 0 }),
            json!({ "do": "sick", "days": 30 }),
            json!({ "do": "well", "days": 1 }),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_note_a_defect_and_a_sick_call_reach_the_company_through_the_queue() {
        let data = std::env::temp_dir().join(format!("openomsi-remote-crew-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut c = found(&Founding { name: "Fern".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        let id = market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        store::save(&data, &c).unwrap();
        let note = Order::Note { bus: id, template: Some(NoteTemplate::Mirror), text: String::new() };
        let defect = Order::Defect { bus: id, kind: DefectKind::Lights, text: "left".into(), fault: false };
        for o in [&note, &defect, &Order::Sick { days: 1 }] {
            assert!(check(&c, &[], o).is_ok());
            queue(&data, &c.id, o).unwrap();
        }
        // (a second sick call behind the first would be refused)
        assert_eq!(check(&c, &pending(&data, &c.id), &Order::Sick { days: 1 }), Err(crew::ILL_ALREADY));
        let done = take(&data, &mut c);
        assert!(done.iter().all(|x| x.1.is_ok()), "{done:?}");
        assert_eq!(crew::notes_for(&c, id)[0].by, "Luc");
        assert!(c.site.job_of(id).is_some() && c.fleet[0].in_workshop(&c.date));
        assert_eq!(c.crew.sick_until.as_deref(), Some("2024-03-04"));
        // the phone's view of it
        let s = crew_summary(&c, &[]);
        assert_eq!(s["sick_until"], "2024-03-04");
        assert_eq!(s["templates"].as_array().map(Vec::len), Some(NoteTemplate::ALL.len()));
        let _ = std::fs::remove_dir_all(&data);
    }
}

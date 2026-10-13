//! The company's Career tab (`crates/omsi-launcher-ui/src/company/career.rs`): the player's own
//! duties of the week, each a click from its duty launcher; the driver's level, rank and
//! licences with the driving test, the company's level and what it opens; the training courses;
//! the workshop's jobs the player can do himself; the rankings; the statistics.

use super::kit::{Go, Popup};
use super::words::{day_label, eur, grade, grouped, num, short_date, short_map, tr, trf};
use super::{launch, plan_of, week_work, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::career::{self as dc, DriverCareer, LicenceClass};
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::levels::{self, Feature};
use omsi_launcher_lib::company::plan::{BusOf, DayPlan, MyDuty, Who};
use omsi_launcher_lib::company::rankings::{self, Entry};
use omsi_launcher_lib::company::training::{self, CourseKind};
use omsi_launcher_lib::company::{self as co, BusSize, Company};
use serde_json::{json, Value};
use std::time::Instant;

pub const PARTS: [&str; 6] = ["My duties", "Progress", "Training", "Workshop", "Rankings", "Statistics"];

/// The driver's career as read: whose, the career file, the trips and what they come to.
pub struct Driver {
    name: String,
    career: DriverCareer,
    trips: Vec<core::TripRun>,
    summary: dc::Summary,
    read: Instant,
}

/// The size of a bus file: the market's kind, the fleet's, else a guess from its name.
fn size_of(s: &Session, bus: &str) -> BusSize {
    let same = |a: &str| a.replace('\\', "/").eq_ignore_ascii_case(&bus.replace('\\', "/"));
    if let Some(b) = s.market.as_ref().and_then(|m| m.iter().find(|b| same(&b.file))) {
        return b.kind.size;
    }
    if let Some(v) = s.company.as_ref().and_then(|c| c.fleet.iter().find(|v| same(&v.bus))) {
        return v.kind.size;
    }
    co::market::guess_kind(&[bus], &[], false, None, None).size
}

/// The driver's career, read again when the driver changed or after a while; a driving test
/// booked is judged when its trip is there.
fn driver(s: &mut Session) -> Option<&Driver> {
    let name = core::load_config().profile.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let stale = s.driver.as_ref().is_none_or(|d| d.name != name || d.read.elapsed().as_secs_f32() > 8.0);
    if stale {
        let data = core::data_dir();
        let trips = core::trips_of(&data, &name);
        let mut career = dc::load(&data, &name);
        let sizes: Vec<(String, BusSize)> = trips.iter().map(|t| (t.bus.clone(), size_of(s, &t.bus))).collect();
        let size = |bus: &str| sizes.iter().find(|x| x.0 == bus).map(|x| x.1).unwrap_or(BusSize::Solo);
        if let Some(exam) = dc::check_exam(&mut career, &trips, &size) {
            let _ = dc::save(&data, &career);
            let text = if exam.passed { "You passed the driving test: the %{class} licence is yours." } else { "You failed the driving test. Book it again when you are ready." };
            let t = trf(text, &[("class", &tr(exam.class.label()))]);
            s.say(t, !exam.passed);
        }
        let summary = dc::summary(&trips, &career);
        s.driver = Some(Driver { name, career, trips, summary, read: Instant::now() });
    }
    s.driver.as_ref()
}

/// The player's own duties of the company's next seven days: the plans read so far, the
/// duties - the next first -, and whether the whole week is read (`career::my_duties`).
pub fn my_duties(s: &mut Session) -> (Vec<DayPlan>, Vec<MyDuty>, bool) {
    let Some(c) = s.company.clone() else { return (Vec::new(), Vec::new(), true) };
    week_work(s);
    let mut plans = Vec::new();
    let mut whole = true;
    for k in 0..7 {
        let date = co::dates::add(&c.date, k);
        match plan_of(s, &date) {
            Some(Ok(p)) => plans.push(p),
            Some(Err(_)) => {}
            None => whole = false,
        }
    }
    let now = (co::clock::now(&c) - co::clock::moment(&c.date, 0)).clamp(0, 48 * 60) as i32;
    let duties = co::plan::my_duties(&plans, &c.date, now);
    (plans, duties, whole)
}

/// A duty as the page hands it back to drive it: its day, line, tour and duty.
pub fn duty_key(d: &MyDuty) -> Value {
    json!({ "date": d.date, "line": d.line, "tour": d.tour_no, "duty": d.duty })
}

/// The duty launcher opened on a duty the page handed back (`planning::drive`).
pub fn drive(s: &mut Session, args: &Value, from: usize) -> Result<Value, String> {
    let r: DutyRef = serde_json::from_value(args.get("duty").cloned().unwrap_or(Value::Null)).map_err(|e| format!("duty: {e}"))?;
    s.launch = Some(launch::DutyLaunch::new(r, from));
    s.generation += 1;
    Ok(json!({ "launch": true }))
}

fn bus_words(c: &Company, d: &MyDuty) -> (String, &'static str) {
    match d.bus {
        Some(BusOf::Own(id)) => match c.vehicle(id) {
            Some(v) => (format!("{} {}", v.number, v.name), "soft"),
            None => (tr("No bus planned yet"), "warn"),
        },
        Some(BusOf::Rental) => (tr("Rental bus"), "soft"),
        None => (tr("No bus planned yet"), "warn"),
    }
}

fn plate_of(c: &Company, line: &str, number: &str) -> Value {
    match c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)) {
        Some(cl) => json!({ "number": cl.number, "colour": cl.colour }),
        None => json!({ "number": number, "colour": "" }),
    }
}

fn duty_row(c: &Company, d: &MyDuty) -> Value {
    let (bus, tone) = bus_words(c, d);
    json!({
        "date": day_label(&d.date), "short": short_date(&d.date),
        "time": format!("{} – {}", co::clock::hhmm(d.from as i64), co::clock::hhmm(d.to as i64)),
        "plate": plate_of(c, &d.line, &d.number),
        "words": trf("Tour %{t} · %{n} trips", &[("t", &d.tour_no), ("n", &d.trips.to_string())]),
        "bus": bus, "bus_tone": tone, "duty": duty_key(d),
    })
}

fn duties_part(s: &mut Session, c: &Company) -> Value {
    let easy = co::autopilot::on(c);
    let (plans, duties, whole) = my_duties(s);
    let rows: Vec<Value> = duties.iter().map(|d| duty_row(c, d)).collect();
    let empty = if !duties.is_empty() {
        Value::Null
    } else if !whole {
        json!({ "reading": tr("Reading the timetable of the week…") })
    } else if easy {
        json!({ "icon": "play_arrow", "title": tr("No duty of yours in the coming week"), "text": tr("Take one of the company's duties in the list beside: it is yours on that weekday every week, and the company's drivers take the others.") })
    } else {
        json!({ "icon": "event", "title": tr("No duty of yours in the coming week"), "text": tr("Plan yourself as a driver in the Planning: choose a duty there and \"You\". It is yours on that weekday every week."), "button": tr("To the planning") })
    };
    let side = if easy {
        let now = (co::clock::now(c) - co::clock::moment(&c.date, 0)).clamp(0, 48 * 60) as i32;
        let open = co::plan::takeable(&plans, &c.date, now);
        let none = if open.is_empty() {
            let tours = plans.iter().any(|p| p.tours.iter().any(|t| !t.tour.unplanned && !t.duties.is_empty()));
            Some(tr(if !whole {
                "Reading the timetable of the week…"
            } else if c.lines.is_empty() {
                "The company runs no line yet: take one on, and its duties are here."
            } else if !tours {
                "The company's lines run no tours in the coming week."
            } else {
                "Every duty of the coming week is yours already."
            }))
        } else {
            None
        };
        json!({ "title": tr("Duties you can take"), "none": none, "take": tr("Take it"), "take_tip": tr("Yours on this weekday every week: drive it from your duties"), "rows": open.iter().map(|d| duty_row(c, d)).collect::<Vec<_>>() })
    } else {
        json!({ "title": tr("How it works"), "text": tr("The duties here are those the Planning gives you yourself (\"You\" as the driver): the same weekday every week. \"Drive\" opens the start of the drive with everything set; what you drive counts for the company as measured when its day is closed, and a duty of yours not driven is open for the central to fill.") })
    };
    json!({
        "title": tr("My duties"), "rows": rows, "empty": empty, "easy": easy, "side": side,
        "drive": tr("Drive"), "up": tr("Up next"),
        "drive_tip": tr("Sets the drive to this duty - the map, the line and tour, the company's bus in its livery, the day and the time - and opens its start: you start it there."),
        "back_tip": tr("Give the duty back: the company's drivers drive it again"),
    })
}

/// A bar of progress with the points written under it.
fn level_bar(xp: i64, floor: i64, next: Option<i64>, tone: &str) -> Value {
    let share = next.map(|n| ((xp - floor) as f64 / (n - floor).max(1) as f64).clamp(0.0, 1.0)).unwrap_or(1.0);
    let text = match next {
        Some(n) => trf("To the next level: %{xp} of %{next} points", &[("xp", &grouped(xp as f64)), ("next", &grouped(n as f64))]),
        None => trf("%{xp} points: the highest level", &[("xp", &grouped(xp as f64))]),
    };
    json!({ "frac": share, "tone": tone, "label": text, "right": format!("{:.0} %", share * 100.0), "tip": tr("Points come with every day closed and every trip driven: tours run, trips on time, passengers carried, a good reputation") })
}

fn progress_part(s: &mut Session, c: &Company) -> Value {
    let lv = levels::level(c);
    let reached = (lv > c.progress.level_seen.max(1)).then(|| {
        let opens: Vec<String> = (c.progress.level_seen.max(1) + 1..=lv).flat_map(levels::opens_at).map(|f| tr(f.label())).collect();
        json!({
            "title": trf("Your company reached level %{n}: %{title}", &[("n", &lv.to_string()), ("title", &tr(levels::title_of(lv)))]),
            "sub": if opens.is_empty() { String::new() } else { trf("New: %{what}", &[("what", &opens.join(", "))]) },
            "button": tr("Wonderful"),
        })
    });
    let profile = core::load_config().profile;
    let me = driver(s).map(|d| (d.summary.clone(), d.career.clone()));
    let driver_v = me.map(|(sm, career)| {
        let licences: Vec<Value> = LicenceClass::ALL
            .iter()
            .map(|class| {
                let held = career.holds(*class);
                let (state, tone, button) = if held {
                    (tr(if class.basic() { "Held from the start" } else { "Won with the driving test" }), "dim", Value::Null)
                } else if career.booked.as_ref().is_some_and(|b| b.class == *class) {
                    (tr("Booked: drive a trip to a timetable with this bus"), "accent2", json!({ "act": "exam_cancel", "label": tr("Cancel") }))
                } else if sm.progress.level < class.level() {
                    (trf("The test from driver level %{n}", &[("n", &class.level().to_string())]), "dim", Value::Null)
                } else if career.booked.is_none() {
                    (tr("Ready for the test"), "soft", json!({ "act": "exam_book", "label": tr("Book the test"), "class": format!("{class:?}") }))
                } else {
                    (tr("Another test is booked"), "dim", Value::Null)
                };
                json!({ "name": tr(class.label()), "held": held, "state": state, "tone": tone, "button": button })
            })
            .collect();
        let last = career.exams.first().map(|e| {
            json!({ "text": trf(if e.passed { "Last test: %{class}, passed (%{score} points)" } else { "Last test: %{class}, failed (%{score} points)" }, &[("class", &tr(e.class.label())), ("score", &e.score.to_string())]), "tone": if e.passed { "ok" } else { "warn" } })
        });
        json!({
            "name": profile, "rank": tr(sm.rank), "level": trf("Level %{n}", &[("n", &sm.progress.level.to_string())]),
            "bar": level_bar(sm.progress.xp, sm.progress.floor, sm.progress.next, "accent2"),
            "facts": [
                [tr("Trips").to_uppercase(), sm.trips.to_string()],
                [tr("Kilometres").to_uppercase(), grouped(sm.km.round())],
                [tr("On time").to_uppercase(), sm.punctuality.map_or("–".into(), |p| format!("{p:.0} %"))],
                [tr("Average score").to_uppercase(), sm.average_score.map_or("–".into(), |a| format!("{a:.0}"))],
            ],
            "licences": licences, "last": last,
            "test": tr("A test is one trip to a timetable with a bus of its kind: driven to the end, on average within three minutes, at most five rough moments, and no red light, camera or collision."),
        })
    });
    let (xp, floor, next) = levels::progress(c);
    let mut list: Vec<Feature> = Feature::ALL.to_vec();
    list.sort_by_key(|f| f.level());
    let features: Vec<Value> = list.iter().map(|f| {
        let open = lv >= f.level();
        json!({ "icon": f.icon(), "name": tr(f.label()), "open": open, "state": if open { tr("Unlocked") } else { trf("Level %{n}", &[("n", &f.level().to_string())]) } })
    }).collect();
    json!({
        "reached": reached,
        "driver_title": tr("You, the driver"), "driver": driver_v, "no_driver": tr("Choose a driver on the start page: the career is the driver's own."),
        "licences_title": tr("Licences"),
        "company_title": tr("The company"),
        "company": {
            "name": c.name, "title": tr(levels::title_of(lv)), "level": trf("Level %{n}", &[("n", &lv.to_string())]),
            "bar": level_bar(xp, floor, next, "ok"),
            "facts": [
                [tr("Fines paid").to_uppercase(), eur(c.progress.fines), if c.progress.fines > 0 { "warn" } else { "text" }],
                [tr("Bonuses").to_uppercase(), eur(c.progress.bonuses), if c.progress.bonuses > 0 { "ok" } else { "text" }],
                [tr("Own work saved").to_uppercase(), eur(c.progress.saved), "text"],
                [tr("Concessions").to_uppercase(), levels::max_concessions(c).to_string(), "text"],
            ],
        },
        "features_title": tr("What the levels open"), "features": features,
    })
}

fn course_terms(c: &Company, k: CourseKind) -> String {
    let days = k.spec().days;
    let d = if days == 1 { tr("1 day") } else { trf("%{n} days", &[("n", &days.to_string())]) };
    format!("{}  ·  {}", eur(training::cost_of(c, k)), d)
}

fn training_part(c: &Company) -> Value {
    let mine: Vec<Value> = CourseKind::PLAYER
        .iter()
        .map(|kind| {
            let state = match training::course_of(c, None, *kind) {
                Some(x) if x.done => json!({ "text": tr("Course completed"), "tone": "ok" }),
                Some(x) => json!({ "text": trf("Until %{date}", &[("date", &day_label(&x.until))]), "tone": "accent2" }),
                None if !levels::unlocked(c, kind.spec().feature) => json!({ "text": trf("Company level %{n}", &[("n", &kind.spec().feature.level().to_string())]), "tone": "dim" }),
                None => json!({ "button": tr("Book the course"), "kind": format!("{kind:?}") }),
            };
            json!({ "icon": kind.icon(), "name": tr(kind.label()), "terms": course_terms(c, *kind), "effect": tr(kind.effect()), "state": state })
        })
        .collect();
    let heads: Vec<Value> = CourseKind::STAFF.iter().map(|k| json!({ "icon": k.icon(), "open": levels::unlocked(c, k.spec().feature), "name": tr(k.label()), "terms": course_terms(c, *k), "tip": tr(k.effect()) })).collect();
    let people: Vec<Value> = c
        .staff
        .iter()
        .filter(|e| e.notice_until.is_none() && co::staff::drives(e))
        .map(|e| {
            let cells: Vec<Value> = CourseKind::STAFF
                .iter()
                .map(|kind| match training::course_of(c, Some(e.id), *kind) {
                    Some(x) if x.done => json!({ "done": tr("Course completed") }),
                    Some(x) => json!({ "until": trf("Until %{date}", &[("date", &day_label(&x.until))]) }),
                    None if !levels::unlocked(c, kind.spec().feature) => json!({ "lock": true }),
                    None if e.absent(&c.date) => json!({ "away": tr("Away") }),
                    None => json!({ "book": tr("Book"), "kind": format!("{kind:?}"), "who": e.id }),
                })
                .collect();
            json!({ "name": e.name, "cells": cells })
        })
        .collect();
    json!({
        "mine_title": tr("Your own training"), "mine": mine,
        "staff_title": tr("Courses for your staff"), "heads": heads, "people": people, "nobody": tr("Nobody works here yet."),
        "foot": tr("A course begins today: the person is away for its days, and has learnt it when its last day is closed."),
    })
}

fn workshop_part(c: &Company) -> Value {
    if !levels::unlocked(c, Feature::Workshop) {
        return json!({ "locked": trf("Your company opens its own workshop at level %{n}. Until then the buses go to a contract workshop.", &[("n", &Feature::Workshop.level().to_string())]), "title": tr("The workshop") });
    }
    let service = training::player_can(c, CourseKind::PlayerService);
    let repairs = training::player_can(c, CourseKind::PlayerRepairs);
    let mut jobs = Vec::new();
    for v in &c.fleet {
        if let Some(bill) = training::repair_due(c, v) {
            jobs.push(json!({ "id": v.id, "kind": "repair", "icon": "construction", "tone": "warn", "name": format!("{} {}", v.number, v.name), "what": trf("Broken down: the repair bill was %{amount}", &[("amount", &eur(bill))]), "save": trf("up to %{amount}", &[("amount", &eur(bill * 55 / 100))]), "can": repairs, "button": tr("Repair it"), "tip": tr("The repairs course first") }));
        } else if !v.in_workshop(&c.date) && (v.km >= v.next_service_km - 5_000.0 || v.condition < 90.0) {
            let done = c.progress.jobs.iter().any(|j| j.vehicle == v.id && j.date == c.date);
            let what = trf("Condition %{n} %, next service at %{km} km", &[("n", &format!("{:.0}", v.condition)), ("km", &grouped(v.next_service_km))]);
            jobs.push(json!({ "id": v.id, "kind": "service", "icon": "tune", "tone": "accent2", "name": format!("{} {}", v.number, v.name), "what": what, "save": trf("up to %{amount}", &[("amount", &eur(training::service_labour(c)))]), "can": service && !done, "button": tr("Service it"), "tip": tr("The workshop course first") }));
        }
    }
    let done: Vec<Value> = c
        .progress
        .jobs
        .iter()
        .rev()
        .take(8)
        .map(|j| {
            let what = if j.repair { tr("Repair job") } else { tr("Service job") };
            json!({ "text": format!("{}  ·  {} {}  ·  {:.0} %", day_label(&j.date), what, j.number, j.quality * 100.0), "amount": eur(j.saved), "tone": if j.saved >= 0 { "ok" } else { "danger" } })
        })
        .collect();
    json!({
        "title": tr("Jobs you can do today"), "jobs": jobs, "none": tr("No bus needs you today: none is due for its service, none broke down."),
        "how_title": tr("How it works"),
        "how": tr("Each part of the bus is checked and dealt with: replace it, adjust it, top it up, or leave it as it is. Read what the check finds - and mind the time. The better the work, the more it saves; a service done badly sends the bus to the workshop after all."),
        "courses": (!(service && repairs)).then(|| tr("To the courses")),
        "done_title": tr("Done lately").to_uppercase(), "done": done, "no_job": tr("No job yet."),
    })
}

fn table(rows: &[(usize, Entry)]) -> Vec<Value> {
    rows.iter().map(|(place, e)| json!({ "place": place, "name": e.name, "you": e.you, "level": e.level, "points": grouped(e.points as f64), "quality": format!("{:.0}", e.quality), "count": e.count })).collect()
}

fn rankings_part(s: &mut Session, c: &Company) -> Value {
    let map = if c.map_name.is_empty() { short_map(&c.map) } else { c.map_name.clone() };
    let companies = rankings::company_table(c);
    let place = rankings::place_of(&companies).unwrap_or(0);
    let profile = core::load_config().profile;
    let drivers = driver(s).map(|d| d.summary.clone()).map(|sm| {
        let t = rankings::driver_table(&c.map, &profile, &sm);
        let place = rankings::place_of(&t).unwrap_or(0);
        json!({ "place": trf("Place %{n} of %{all}", &[("n", &place.to_string()), ("all", &t.len().to_string())]), "rows": table(&t), "heads": ["#", tr("Name").to_uppercase(), tr("Level").to_uppercase(), tr("Points").to_uppercase(), tr("Score").to_uppercase(), tr("Trips").to_uppercase()] })
    });
    json!({
        "companies_title": trf("Companies of %{map}", &[("map", &map)]),
        "companies": { "place": trf("Place %{n} of %{all}", &[("n", &place.to_string()), ("all", &companies.len().to_string())]), "rows": table(&companies), "heads": ["#", tr("Name").to_uppercase(), tr("Level").to_uppercase(), tr("Points").to_uppercase(), tr("On time").to_uppercase(), tr("Buses").to_uppercase()] },
        "drivers_title": tr("Drivers"), "drivers": drivers, "no_driver": tr("Choose a driver on the start page."),
        "foot": tr("The other companies and drivers of the map are the game's own; in multiplayer the table will hold the real players."),
    })
}

fn statistics_part(s: &mut Session, c: &Company) -> Value {
    let mine = driver(s).map(|d| (d.summary.clone(), d.trips.clone())).map(|(sm, trips)| {
        let values: Vec<Value> = trips
            .iter()
            .rev()
            .map(|t| {
                let e = dc::evaluate(t);
                let line = if t.line.trim().is_empty() { tr("Free drive") } else { trf("Line %{n}", &[("n", &t.line)]) };
                json!([e.score, format!("{line}  ·  {}  ·  {} ({})", t.terminus, e.score, tr(e.grade.label()))])
            })
            .collect();
        let avg = sm.average_score.map_or("–".into(), |a| format!("{a:.0}"));
        let best = sm.best_score.map_or("–".into(), |b| b.to_string());
        json!({
            "figures": [
                { "label": tr("Trips"), "value": sm.trips.to_string(), "under": trf("%{n} hours", &[("n", &num(sm.hours, 1))]), "tone": "text" },
                { "label": tr("Kilometres"), "value": grouped(sm.km.round()), "under": trf("%{n} passengers", &[("n", &grouped(sm.passengers as f64))]), "tone": "text" },
                { "label": tr("On time"), "value": sm.punctuality.map_or("–".into(), |p| format!("{p:.0} %")), "under": trf("%{n} excellent trips", &[("n", &sm.excellent.to_string())]), "tone": "text" },
                { "label": tr("Fines"), "value": eur(sm.fines), "under": trf("%{r} red · %{c} cameras", &[("r", &sm.red_lights.to_string()), ("c", &sm.speeding.to_string())]), "tone": if sm.fines > 0 { "warn" } else { "text" } },
            ],
            "title": tr("Your last 30 trips, judged"), "values": values, "empty": tr("Drive a trip to see it judged here."),
            "foot": trf("Average %{avg}  ·  best %{best}  ·  %{n} trips seen by the drive watch", &[("avg", &avg), ("best", &best), ("n", &sm.watched.to_string())]),
        })
    });
    let days = c.history.len();
    let pax: u64 = c.history.iter().map(|d| d.passengers as u64).sum();
    let km: f64 = c.history.iter().map(|d| d.km).sum();
    let judged = &c.progress.judged;
    let avg = (!judged.is_empty()).then(|| judged.iter().map(|j| j.score as f64).sum::<f64>() / judged.len() as f64);
    let values: Vec<Value> = judged
        .iter()
        .map(|j| {
            let mut t = format!("{}  ·  {}  ·  {}", day_label(&j.date), trf("Line %{n}", &[("n", &j.line)]), j.score);
            if j.fines > 0 {
                t.push_str(&format!("  ·  {} {}", tr("Fines"), eur(j.fines)));
            }
            if j.bonus > 0 {
                t.push_str(&format!("  ·  {} {}", tr("Bonus"), eur(j.bonus)));
            }
            json!([j.score, t])
        })
        .collect();
    json!({
        "mine": mine, "no_driver": tr("Choose a driver on the start page."),
        "company": {
            "figures": [
                { "label": tr("Days run"), "value": days.to_string(), "under": trf("since %{date}", &[("date", &day_label(&c.founded))]), "tone": "text" },
                { "label": tr("Passengers"), "value": grouped(pax as f64), "under": format!("{} km", grouped(km.round())), "tone": "text" },
                { "label": tr("Your tours"), "value": judged.len().to_string(), "under": avg.map_or(String::new(), |a| trf("average %{n}", &[("n", &format!("{a:.0}"))])), "tone": "text" },
                { "label": tr("Reputation"), "value": format!("{:.0}", c.reputation), "under": tr("of 100"), "tone": grade(c.reputation + 15.0) },
            ],
            "title": tr("Your tours for the company, judged"), "values": values, "empty": tr("Drive the company's lines yourself: each trip is judged when the day is closed."),
            "foot": trf("Fines %{fines}  ·  bonuses %{bonus}  ·  saved by own work %{saved}", &[("fines", &eur(c.progress.fines)), ("bonus", &eur(c.progress.bonuses)), ("saved", &eur(c.progress.saved))]),
        },
    })
}

/// The Career page: its parts' names, the company's level beside them, and the part shown.
pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let part = args.get("part").and_then(Value::as_u64).unwrap_or(0) as usize;
    let lv = levels::level(&c);
    let body = match part {
        1 => progress_part(s, &c),
        2 => training_part(&c),
        3 => workshop_part(&c),
        4 => rankings_part(s, &c),
        5 => statistics_part(s, &c),
        _ => duties_part(s, &c),
    };
    json!({ "parts": PARTS.iter().map(|p| tr(p)).collect::<Vec<_>>(), "part": part, "level": format!("{}  ·  {}", trf("Level %{n}", &[("n", &lv.to_string())]), tr(levels::title_of(lv))), "body": body })
}

fn class_of(name: &str) -> Option<LicenceClass> {
    LicenceClass::ALL.iter().copied().find(|c| format!("{c:?}") == name)
}

pub fn course_of(name: &str) -> Option<CourseKind> {
    CourseKind::PLAYER.iter().chain(CourseKind::STAFF.iter()).chain(CourseKind::LICENCES.iter()).copied().chain([CourseKind::TypeTraining]).find(|k| format!("{k:?}") == name)
}

/// What the Career page's buttons do.
pub fn act(s: &mut Session, what: &str, args: &Value, _p: &dyn crate::Platform) -> Option<Result<Value, String>> {
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some(match what {
        "drive_duty" => drive(s, args, args.get("from").and_then(Value::as_u64).unwrap_or(6) as usize),
        "take_duty" | "give_back" => {
            let r: DutyRef = match serde_json::from_value(args.get("duty").cloned().unwrap_or(Value::Null)) {
                Ok(r) => r,
                Err(e) => return Some(Err(format!("duty: {e}"))),
            };
            let wd = co::plan::weekday_of(&r.date);
            let who = (what == "take_duty").then_some(Who::Player);
            let done = super::act(s, |c| {
                co::plan::set_driver(c, wd, &r.line, &r.tour, r.duty, who);
                Ok(())
            });
            if done.is_ok() && who.is_some() {
                let t = tr("The duty is yours on that weekday every week: drive it from My duties.");
                s.say(t, false);
            }
            Ok(done.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
        }
        "level_seen" => Ok(super::act(s, |c| {
            levels::take_new_level(c);
            Ok(())
        })
        .err()
        .map(|p| json!({ "popup": p }))
        .unwrap_or(json!({}))),
        "exam_book" | "exam_cancel" => {
            let book = if what == "exam_book" { class_of(&str_of("class")) } else { None };
            let data = core::data_dir();
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = driver(s);
            let mut refused: Option<&'static str> = None;
            let mut said: Option<(String, bool)> = None;
            if let Some(d) = s.driver.as_mut() {
                let result = match book {
                    Some(class) => dc::book_exam(&mut d.career, class, d.summary.progress.level, now),
                    None => {
                        dc::cancel_exam(&mut d.career);
                        Ok(())
                    }
                };
                match result {
                    Err(e) => refused = Some(e),
                    Ok(()) => match dc::save(&data, &d.career) {
                        Ok(()) => {
                            if let Some(class) = book {
                                said = Some((trf("The %{class} test is booked: your next trip to a timetable with such a bus is the test.", &[("class", &tr(class.label()))]), false));
                            }
                        }
                        Err(e) => said = Some((format!("{e:#}"), true)),
                    },
                }
            }
            // (read again at the next look)
            s.driver = None;
            if let Some((t, e)) = said {
                s.say(t, e);
            }
            s.generation += 1;
            Ok(refused.map(|e| json!({ "popup": super::refuse(s, e) })).unwrap_or(json!({})))
        }
        "enrol" => {
            let Some(kind) = course_of(&str_of("kind")) else { return Some(Err("no such course".into())) };
            let who = args.get("who").and_then(Value::as_u64).map(|x| x as u32);
            match super::act(s, |c| training::enrol(c, kind, who)) {
                Ok(until) => {
                    let t = trf("Booked: %{course}, until %{date}.", &[("course", &tr(kind.label())), ("date", &day_label(&until))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "job_blocked" => {
            let repair = str_of("kind") == "repair";
            let (w, course) = if repair { ("Repairs need the repairs course: what is broken is found and mended the right way.", "Book the repairs course under Training: it takes a few days.") } else { ("A service needs the workshop course: which part wants what.", "Book the workshop course under Training: it takes a few days.") };
            Ok(json!({ "popup": super::popup(s, Popup::new("badge", "A course first", tr(w), tr(course), Some(Go::Courses))) }))
        }
        _ => return None,
    })
}

//! The depot page (`crates/omsi-launcher-ui/src/company/depot.rs`): its figures, the yard from
//! above - the offices, the workshop hall with its bays, the washing bay, the diesel station,
//! the parking's blocks with their chargers, the buses where they stand tonight -, the areas to
//! build (a bigger depot, level by level) and the workshop's jobs. The rules are
//! `company::depot`'s.

use super::super::kit;
use super::super::words::{day_label, eur, grade, tr, trf};
use super::super::{act as apply, Session};
use super::{popup_answer, str_arg, u32_arg};
use crate::Answer;
use omsi_launcher_lib::company::depot::{self as dp, Area, JobKind};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let selected = args.get("selected").and_then(Value::as_u64).map(|x| x as u32);
    json!({ "figures": figures(&c), "plan": plan(&c), "build": buildings(&c), "workshop": workshop(&c, selected) })
}

fn figure(label: &str, value: String, under: String, tone: &str) -> Value {
    json!({ "label": tr(label), "value": value, "under": under, "tone": tone })
}

/// Parking, the workshop's bays, the cleanliness and the buildings' upkeep (`depot::draw`).
fn figures(c: &Company) -> Value {
    let held = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let spaces = dp::places(c);
    let out = dp::outside(c);
    let under = if out > 0 { trf("%{n} in the street", &[("n", &out.to_string())]) } else { tr("spaces in use") };
    let busy = dp::bays_used(c, &c.date);
    let waiting = c.site.jobs.iter().filter(|j| j.started.is_none()).count();
    let under_ws = if waiting > 0 { trf("Jobs waiting: %{n}", &[("n", &waiting.to_string())]) } else { tr("bays in use") };
    json!([
        figure("Parking", format!("{} / {}", held.min(spaces), spaces), under, if out > 0 { "warn" } else { "text" }),
        figure("Workshop", format!("{} / {}", busy, c.site.bays()), under_ws, if busy > c.site.bays() { "warn" } else { "text" }),
        figure("Cleanliness", format!("{:.0} %", c.site.clean), tr("of the fleet"), grade(c.site.clean)),
        figure("Upkeep", eur(dp::upkeep_month(c)), tr("a month for the buildings"), "text"),
    ])
}

/// A building of the plan: built or a free plot, its head in capitals, the building work on it.
fn building(c: &Company, area: Area, title: &str) -> Value {
    let built = c.site.level(area) > 0;
    let head = if built { tr(title) } else { tr("Free plot") }.to_uppercase();
    let works = c.site.works_on(area).map(|w| trf("Ready %{date}", &[("date", &day_label(&w.until))]));
    json!({ "built": built, "head": head, "works": works })
}

/// A bus where it stands: its number, and its card in the tooltip.
fn bus_at(c: &Company, v: &co::Vehicle) -> Value {
    let mut tip = format!("{} {}  ·  {} {:.0} %", v.number, v.name, tr("condition"), v.condition);
    if v.in_workshop(&c.date) {
        tip.push_str(&format!("  ·  {}", trf("in the workshop until %{date}", &[("date", &day_label(v.workshop_until.as_deref().unwrap_or("")))])));
    }
    json!({ "id": v.id, "number": v.number, "tip": tip })
}

/// The yard from above (`depot::plan`): what is built, being built, and where the buses stand.
fn plan(c: &Company) -> Value {
    let today = c.date.clone();
    let held: Vec<&co::Vehicle> = c.fleet.iter().filter(|v| v.held_on(&today)).collect();
    let mut in_ws: Vec<&co::Vehicle> = held.iter().copied().filter(|v| v.in_workshop(&today)).collect();
    in_ws.sort_by(|a, b| a.number.cmp(&b.number));
    let mut parked: Vec<&co::Vehicle> = held.iter().copied().filter(|v| !v.in_workshop(&today)).collect();
    parked.sort_by_key(|v| (v.number.len(), v.number.clone()));
    let mut offices = building(c, Area::Offices, "Offices");
    offices["floors"] = json!((c.site.offices as usize).min(3));
    offices["room"] = json!(trf("room for %{n}", &[("n", &c.site.staff_room().to_string())]));
    let mut hall = building(c, Area::Workshop, "Workshop");
    hall["bays"] = json!(c.site.bays());
    hall["max"] = json!(Area::Workshop.max());
    let mut wash = building(c, Area::Wash, "Washing bay");
    wash["gates"] = json!(c.site.wash >= 2);
    let mut fuel = building(c, Area::Fuel, "Diesel station");
    fuel["pumps"] = json!(c.site.fuel);
    let parking = json!({
        "label": tr(Area::Parking.label()).to_uppercase(),
        "works": c.site.works_on(Area::Parking).map(|w| trf("More spaces ready %{date}", &[("date", &day_label(&w.until))])),
        "blocks": Area::Parking.max(),
        "built": c.site.parking,
        "building": c.site.works_on(Area::Parking).is_some(),
        "per": dp::SPACES_PER_LEVEL,
        "chargers": c.site.electric_served(),
    });
    json!({
        "title": tr("The depot from above"),
        "colour": c.colours[0],
        "offices": offices, "hall": hall, "wash": wash, "fuel": fuel, "parking": parking,
        "in_ws": in_ws.iter().map(|v| bus_at(c, v)).collect::<Vec<_>>(),
        "parked": parked.iter().map(|v| bus_at(c, v)).collect::<Vec<_>>(),
    })
}

/// What a level of an area does, in words (`depot::effect`).
fn effect(c: &Company, a: Area) -> String {
    let s = &c.site;
    match a {
        Area::Parking => trf("%{n} spaces", &[("n", &s.spaces().to_string())]),
        Area::Workshop => trf("%{n} bays: buses worked on at once", &[("n", &s.bays().to_string())]),
        Area::Wash if s.wash == 0 => tr("None: the fleet is cleaned by hand"),
        Area::Wash => trf("Washes %{n} buses a night", &[("n", &s.washes().to_string())]),
        Area::Fuel => trf("Fills %{n} diesel buses a night", &[("n", &s.diesel_served().to_string())]),
        Area::Charging if s.charging == 0 => tr("None: electric buses charge outside, dearer"),
        Area::Charging => trf("Charges %{n} electric buses a night", &[("n", &s.electric_served().to_string())]),
        Area::Offices => trf("Room for %{n} people", &[("n", &s.staff_room().to_string())]),
    }
}

/// The areas to build (`depot::buildings`): each with its level, what it does, and the next
/// level's price, the building work under way, or complete.
fn buildings(c: &Company) -> Value {
    let rows: Vec<Value> = Area::ALL
        .iter()
        .map(|a| {
            let lv = c.site.level(*a);
            let mut row = json!({
                "key": a.key(), "icon": a.icon(), "label": tr(a.label()),
                "level": trf("level %{n} of %{max}", &[("n", &lv.to_string()), ("max", &a.max().to_string())]),
                "effect": effect(c, *a),
            });
            if let Some(w) = c.site.works_on(*a) {
                row["works"] = json!(trf("Building until %{date}", &[("date", &day_label(&w.until))]));
            } else if let Some((cost, days)) = dp::next_cost(c, *a) {
                let up = dp::step(*a, lv + 1).upkeep as f64 * c.price_index;
                row["cost"] = json!(eur(cost));
                row["locked"] = json!(!dp::area_allowed(c, *a));
                row["tip"] = json!(trf("Build the next level: %{days} days of work, then %{amount} a month", &[("days", &days.to_string()), ("amount", &eur(up.round() as i64))]));
            } else {
                row["complete"] = json!(tr("Complete"));
            }
            row
        })
        .collect();
    json!({ "title": tr("Build"), "rows": rows })
}

/// The workshop's jobs, then every other bus by its condition, the one picked first
/// (`depot::workshop`).
fn workshop(c: &Company, selected: Option<u32>) -> Value {
    let today = c.date.clone();
    let mut buses: Vec<&co::Vehicle> = c.fleet.iter().filter(|v| v.held_on(&today) && c.site.job_of(v.id).is_none()).collect();
    buses.sort_by(|a, b| (Some(b.id) == selected).cmp(&(Some(a.id) == selected)).then(a.condition.total_cmp(&b.condition)));
    let jobs: Vec<Value> = c
        .site
        .jobs
        .iter()
        .filter_map(|j| {
            let bus = c.vehicle(j.vehicle)?;
            let (what, tone) = match (&j.started, &j.until) {
                (Some(_), Some(u)) => (trf("In a bay until %{date}", &[("date", &day_label(u))]), "accent2"),
                _ => (tr("Waiting for a free bay"), "warn"),
            };
            Some(json!({
                "id": j.id, "number": bus.number, "title": format!("{}  ·  {}", tr(j.kind.label()), bus.name),
                "what": what, "tone": tone, "cost": (j.cost > 0).then(|| eur(j.cost)), "cancel": j.started.is_none(),
            }))
        })
        .collect();
    let rows: Vec<Value> = buses
        .iter()
        .map(|b| {
            let due = b.km >= b.next_service_km - 1_000.0;
            let (sub, tone) = if b.in_workshop(&today) {
                (tr("in the workshop"), "dim")
            } else if due {
                (tr("service due"), "warn")
            } else {
                (format!("{:.0} %", b.condition), "dim")
            };
            let kinds: Vec<Value> = JobKind::ALL
                .iter()
                .map(|k| {
                    let cost = dp::job_cost(c, b.id, *k);
                    let tip = if cost > 0 {
                        trf("%{days} days in a bay, %{amount}", &[("days", &k.days().to_string()), ("amount", &eur(cost))])
                    } else {
                        trf("%{days} days in a bay; paid with the maintenance per kilometre", &[("days", &k.days().to_string())])
                    };
                    json!({ "kind": k.key(), "label": tr(k.label()), "tip": tip })
                })
                .collect();
            json!({
                "id": b.id, "number": b.number, "name": b.name, "sub": sub, "tone": tone,
                "cond": { "frac": b.condition / 100.0, "tone": grade(b.condition), "tip": trf("Condition: %{n} of 100", &[("n", &format!("{:.0}", b.condition))]) },
                "jobs": kinds,
            })
        })
        .collect();
    let empty = (jobs.is_empty() && rows.is_empty()).then(|| tr("No bus yet: the workshop services, repairs and overhauls the fleet's buses, one a bay."));
    json!({ "title": tr("Workshop"), "jobs": jobs, "buses": rows, "empty": empty, "cancel_tip": tr("Take the job back") })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    Some(match what {
        "depot_build" => {
            let Some(a) = Area::from_key(&str_arg(args, "area")) else { return Some(Ok(json!({}))) };
            let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
            let Some((cost, _)) = dp::next_cost(&c, a) else { return Some(Ok(json!({}))) };
            let casual = c.game_mode.casual();
            // (what is in the way is said first: the level that opens it, or the money)
            if !dp::area_allowed(&c, a) {
                let f = if a == Area::Workshop { co::levels::Feature::Workshop } else { co::levels::Feature::ElectricBuses };
                return Some(popup_answer(kit::locked(&c, f).json(casual)));
            }
            if c.cash < cost {
                return Some(popup_answer(kit::no_cash(&c, cost).json(casual)));
            }
            match apply(s, |c| dp::build(c, a)) {
                Ok(()) => {
                    let t = trf("Building work has begun: %{what}.", &[("what", &tr(a.label()))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "depot_cancel" => {
            let id = u32_arg(args, "id");
            Ok(apply(s, |c| dp::cancel(c, id)).err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
        }
        "depot_order" => {
            let id = u32_arg(args, "id");
            let Some(kind) = JobKind::from_key(&str_arg(args, "kind")) else { return Some(Ok(json!({}))) };
            match apply(s, |c| dp::order(c, id, kind)) {
                Ok(_) => {
                    // (an order starts at the night: tomorrow when a bay is free for it then)
                    let started = s.company.as_ref().is_some_and(|c| c.site.job_of(id).is_some_and(|j| j.started.is_some() || dp::starts_tomorrow(c, j.id)));
                    let t = tr(if started { "The bus is in the workshop tomorrow." } else { "The job waits for a free bay." });
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        _ => return None,
    })
}

//! "Request a change" of a tendered line's bus contract (`company::variations`): the dialog the
//! contracts' and the concessions' pages open on a contract - the kinds of change with their
//! controls, what the change does to the kilometres, hours and stops, its class, the fee or the
//! penalty, the payment a week before and after, the default points and reputation it costs, and
//! the offer the company may make instead; the contract's history of variations. The rules are
//! the core's.

use super::super::words::{day_label, eur, grouped, num, tr, trf};
use super::super::{act as apply, Session};
use crate::Answer;
use omsi_launcher_lib::company::bus_contracts::{Contract, Kind};
use omsi_launcher_lib::company::model::BusSize;
use omsi_launcher_lib::company::variations::{self as va, Ask, Class, Quote, Variation};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// The kinds of change the dialog offers: key, words, icon.
const KINDS: [(&str, &str, &str); 4] = [("timetable", "Timetable", "schedule"), ("route", "Route", "alt_route"), ("days", "Days of operation", "calendar_month"), ("vehicle", "Vehicle type", "directions_bus")];

/// The bus types a contract may ask for, and their words.
const SIZES: [(BusSize, &str, &str); 4] = [(BusSize::Midi, "midi", "Midibus"), (BusSize::Solo, "solo", "Standard bus (12 m)"), (BusSize::Articulated, "articulated", "Articulated bus (18 m)"), (BusSize::Double, "double", "Double-decker")];

fn size_words(s: BusSize) -> String {
    tr(SIZES.iter().find(|x| x.0 == s).map_or("Standard bus (12 m)", |x| x.2))
}

fn int(a: &Value, key: &str) -> Option<i64> {
    a.get(key).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64)))
}

/// The change the page asks about (`ask`: its kind and its controls' values, the journeys and
/// days as they would be): the core's `Ask`, from the contract's work as it is.
fn ask_of(c: &Company, k: &Contract, a: &Value) -> Ask {
    let w = va::work_of(c, k);
    match a.get("kind").and_then(Value::as_str).unwrap_or("timetable") {
        "route" => Ask::Route {
            added: int(a, "added").unwrap_or(0).clamp(0, 50) as u32,
            removed: int(a, "removed").unwrap_or(0).clamp(0, 50) as u32,
            km: a.get("km").and_then(Value::as_f64).unwrap_or(0.0).clamp(-50.0, 50.0),
            core: a.get("core").and_then(Value::as_bool).unwrap_or(false),
        },
        "days" => Ask::Days { days: int(a, "days").map_or(days_a_week(&w), |d| d.clamp(1, 7) as u32) },
        "vehicle" => Ask::Vehicle { size: a.get("size").and_then(Value::as_str).and_then(|s| SIZES.iter().find(|x| x.1 == s)).map_or(k.vehicle.unwrap_or(BusSize::Solo), |x| x.0) },
        _ => Ask::Timetable {
            journeys: int(a, "journeys").map_or(0, |n| n.clamp(1, 1000) - w.journeys as i64) as i32,
            span: int(a, "span").unwrap_or(0).clamp(-600, 600) as i32,
            retime: int(a, "retime").unwrap_or(0).clamp(-60, 60) as i32,
        },
    }
}

fn days_a_week(w: &omsi_launcher_lib::company::bus_contracts::Work) -> u32 {
    ((w.days as f64 / 52.0).round() as u32).clamp(1, 7)
}

/// The controls of a kind of change, at the values asked.
fn fields(c: &Company, k: &Contract, ask: &Ask) -> Vec<Value> {
    let w = va::work_of(c, k);
    let step = |key: &str, label: &str, value: f64, min: f64, max: f64, step: f64, unit: &str| json!({ "type": "step", "key": key, "label": tr(label), "value": value, "min": min, "max": max, "step": step, "unit": tr(unit) });
    match *ask {
        Ask::Timetable { journeys, span, retime } => vec![
            step("journeys", "Journeys a day", (w.journeys as i64 + journeys as i64).max(1) as f64, 1.0, (w.journeys * 3).max(10) as f64, 1.0, ""),
            step("span", "First and last trip: the day longer by", span as f64, -240.0, 240.0, 15.0, "min a bus"),
            step("retime", "Running time of a journey", retime as f64, -30.0, 30.0, 1.0, "min"),
        ],
        Ask::Route { added, removed, km, core } => vec![
            step("added", "Stops added", added as f64, 0.0, 20.0, 1.0, ""),
            step("removed", "Stops removed", removed as f64, 0.0, 20.0, 1.0, ""),
            step("km", "Diversion: a journey longer by", km, -10.0, 10.0, 0.1, "km"),
            json!({ "type": "switch", "key": "core", "label": tr("A core stop (a terminus) is given up"), "on": core }),
        ],
        Ask::Days { days } => vec![step("days", "Operating days a week", days as f64, 1.0, 7.0, 1.0, "")],
        Ask::Vehicle { size } => vec![json!({ "type": "choice", "key": "size", "label": tr("Bus type"), "value": SIZES.iter().find(|x| x.0 == size).map_or("solo", |x| x.1), "options": SIZES.iter().map(|x| json!([x.1, tr(x.2)])).collect::<Vec<_>>() })],
        Ask::Edit { .. } => Vec::new(),
    }
}

/// A class as a badge, and what it means.
fn class_json(q: &Quote) -> Value {
    let (tone, note) = match q.class {
        Class::Minor => ("ok", "Minor: an admin fee, the payment stays as it is."),
        Class::Moderate => ("warn", "Moderate: a variation fee, and the payment follows the service pro rata."),
        Class::Major => ("danger", "Major: a penalty of the contract's value still to come, default points and reputation, and the payment pro rata."),
    };
    json!({ "text": tr(q.class.label()).to_uppercase(), "tone": tone, "note": tr(note) })
}

fn signed_pct(share: f64) -> String {
    format!("{:+.0} %", 100.0 * share)
}

/// A row of before and after.
fn row(label: &str, before: String, after: String, delta: Option<f64>) -> Value {
    json!({ "label": tr(label), "before": before, "after": after, "delta": delta.filter(|d| d.abs() >= 0.0005).map(signed_pct), "tone": delta.map_or("dim", |d| if d.abs() < 0.0005 { "dim" } else if d > 0.0 { "ok" } else { "warn" }) })
}

/// What a change does and costs (`variations::Quote`), for the dialog and the line editor's
/// question.
pub fn quote_json(c: &Company, q: &Quote) -> Value {
    let rel = |a: f64, b: f64| if a > 0.0 { Some(b / a - 1.0) } else { None };
    let (km, hours) = (q.km_year(), q.hours_year());
    let mut rows = vec![
        row("Kilometres a year", grouped(km.0.round()), grouped(km.1.round()), rel(km.0, km.1)),
        row("Hours a year", grouped(hours.0.round()), grouped(hours.1.round()), rel(hours.0, hours.1)),
        row("Journeys a day", q.before.journeys.to_string(), q.after.journeys.to_string(), rel(q.before.journeys as f64, q.after.journeys as f64)),
        row("Days a week", days_a_week(&q.before).to_string(), days_a_week(&q.after).to_string(), None),
        row("Buses at the peak", q.before.buses.to_string(), q.after.buses.to_string(), None),
    ];
    if q.sizes.0 != q.sizes.1 {
        rows.push(row("Bus type", size_words(q.sizes.0), size_words(q.sizes.1), Some(q.share)));
    }
    let stops = (q.added > 0 || q.removed > 0 || q.core).then(|| {
        let mut t = trf("%{a} stops added, %{r} removed", &[("a", &q.added.to_string()), ("r", &q.removed.to_string())]);
        if q.core {
            t = format!("{t}  ·  {}", tr("a core stop given up"));
        }
        t
    });
    let fee_label = if q.class == Class::Minor { "Admin fee" } else { "Variation fee" };
    let mut money = vec![json!({ "label": tr(fee_label), "value": eur(q.fee), "tone": "text" })];
    if q.penalty > 0 {
        money.push(json!({ "label": trf("Penalty: %{pct} % of the value still to come", &[("pct", &num(100.0 * va::PENALTY_SHARE * va::lenience(c.game_mode).penalty, 1))]), "value": eur(q.penalty), "tone": "danger" }));
    }
    money.push(json!({ "label": tr("To pay now"), "value": eur(q.price()), "tone": if q.price() > 0 { "warn" } else { "ok" }, "total": true }));
    let pay_tone = if q.year_after > q.year_before { "ok" } else if q.year_after < q.year_before { "warn" } else { "dim" };
    let payment = json!({
        "label": tr(if q.kind == Kind::Net { "Subsidy a week" } else { "Payment a week" }),
        "before": eur(q.week_before()), "after": eur(q.week_after()), "tone": pay_tone,
        "year": trf("%{before} a year, then %{after}", &[("before", &eur(q.year_before)), ("after", &eur(q.year_after))]),
        "why": tr(if q.kind == Kind::Net { "A net subsidy follows the cost less the fares and BSOG you keep: the revenue risk stays yours." } else { "A gross-cost payment follows the cost of the work: the fares stay the authority's." }),
    });
    let mut hits: Vec<Value> = Vec::new();
    if q.points > 0.0 {
        hits.push(json!({ "icon": "flag", "tone": "danger", "text": trf("%{n} default points on the contract: its rating, and your quality in the next tenders", &[("n", &num(q.points, 0))]) }));
    }
    if q.reputation > 0.0 {
        hits.push(json!({ "icon": "trending_down", "tone": "danger", "text": trf("About %{n} points of reputation", &[("n", &num(q.reputation, 1))]) }));
    }
    if q.waits {
        hits.push(json!({ "icon": "schedule", "tone": "accent", "text": tr("The higher payment begins when the line runs the change: save it in the line editor.") }));
    } else if matches!(q.change, va::Change::Timetable | va::Change::Route | va::Change::Days) {
        hits.push(json!({ "icon": "edit_road", "tone": "accent", "text": tr("Then change the line in the line editor: a save that runs the change agreed costs nothing more.") }));
    }
    json!({
        "class": class_json(q), "refused": q.refused.map(tr), "nothing": q.nothing.then(|| tr("Choose what changes: its price is shown here.")),
        "covered": q.covered, "share": signed_pct(q.share),
        "rows": rows, "stops": stops, "money": money, "payment": payment, "hits": hits,
    })
}

/// A variation of the history in words.
pub fn history_text(v: &Variation) -> String {
    let mut t = trf("%{date}: %{change}, %{class}  ·  %{amount} paid  ·  %{year} a year after", &[
        ("date", &day_label(&v.date)),
        ("change", &tr(v.change.label())),
        ("class", &tr(v.class.label()).to_lowercase()),
        ("amount", &eur(v.fee + v.penalty)),
        ("year", &eur(v.year_after)),
    ]);
    if v.offer {
        t = format!("{t}  ·  {}", tr("by your offer"));
    }
    if v.waiting {
        t = format!("{t}  ·  {}", tr("waiting for the line to run it"));
    }
    t
}

/// The dialog of a line's contract (`args`: `line`, `ask`).
pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let line = args.get("line").and_then(Value::as_str).unwrap_or("");
    let Some(k) = va::changeable(&c, line).cloned() else { return json!({ "none": tr("This line has no tendered contract to change.") }) };
    let a = args.get("ask").cloned().unwrap_or(json!({}));
    let ask = ask_of(&c, &k, &a);
    let kind = match ask {
        Ask::Route { .. } => "route",
        Ask::Days { .. } => "days",
        Ask::Vehicle { .. } => "vehicle",
        _ => "timetable",
    };
    let Ok(q) = va::quote(&c, line, &ask) else { return json!({ "none": tr("This line has no tendered contract to change.") }) };
    let caption = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.caption.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| k.number.clone());
    let open = q.refused.is_none() && !q.nothing;
    let confirm = if q.price() > 0 { trf("Agree and pay %{amount}", &[("amount", &eur(q.price()))]) } else { tr("Agree the change") };
    let wait = va::next_offer(&c, &k).map(|d| trf("Your next offer can be made on %{date}.", &[("date", &day_label(&d))]));
    let offer = json!({
        "title": tr("Or make an offer"),
        "text": tr("Offer a sum instead of the price asked. The authority weighs the size of the change and your track record (the contract's rating, your default points); it answers one offer a week on a contract."),
        "amount": (q.price() as f64 * 0.75 / 100.0).round() as i64 * 100, "label": tr("Make the offer"), "wait": wait, "open": open && q.price() > 0,
    });
    let mut history: Vec<Value> = k.variations.iter().rev().map(|v| json!(history_text(v))).collect();
    history.truncate(8);
    json!({
        "title": trf("Request a change: line %{n}", &[("n", &k.number)]), "number": k.number, "caption": caption, "line": k.line,
        "kind_text": super::contracts::kind_text(&k), "pay": super::contracts::pay_text(&c, &k),
        "lead": tr("The authority prices a change of a tendered line by its size: a few minutes or a few per cent cost an admin fee, a larger change a fee with the payment adjusted pro rata, a major one a penalty as well."),
        "kinds": KINDS.iter().map(|x| json!({ "key": x.0, "label": tr(x.1), "icon": x.2 })).collect::<Vec<_>>(), "kind": kind,
        "fields": fields(&c, &k, &ask), "quote": quote_json(&c, &q),
        "confirm": confirm, "open": open, "cancel": tr("Cancel"), "offer": offer,
        "history_title": tr("Variations agreed"), "history": history,
    })
}

/// The dialog's actions: the change agreed at the price asked, or an offer made.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let line = args.get("line").and_then(Value::as_str).unwrap_or("").to_string();
    let a = args.get("ask").cloned().unwrap_or(json!({}));
    let ask = |c: &Company| va::changeable(c, &line).map(|k| ask_of(c, k, &a)).ok_or("This line has no tendered contract to change.");
    Some(match what {
        "contract_vary" => match apply(s, |c| va::agree(c, &line, &ask(c)?)) {
            Ok(v) => {
                s.say(trf("The change is agreed: %{amount} paid.", &[("amount", &eur(v.fee + v.penalty))]), false);
                Ok(json!({ "done": true }))
            }
            Err(p) => Ok(json!({ "popup": p })),
        },
        "contract_offer" => {
            let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(0);
            match apply(s, |c| va::offer(c, &line, &ask(c)?, amount)) {
                Ok(Some(v)) => {
                    s.say(trf("The authority takes your offer: the change is agreed for %{amount}.", &[("amount", &eur(v.fee))]), false);
                    Ok(json!({ "done": true }))
                }
                Ok(None) => {
                    s.say(tr("The authority refuses your offer. You can make another in a week."), true);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        _ => return None,
    })
}

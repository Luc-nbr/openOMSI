//! A bus's age and value as the pages say them (Luc, 2026-10-15): when it was first registered
//! and where that comes from (its plate, the market, the model, the player), how its value is
//! worked out, and the fleet's value by age band for the finances.

use super::dealer::row;
use super::words::{eur, month_label, num, tr, trf};
use super::Session;
use omsi_launcher_lib::company::dealer::{Offer, OfferKind};
use omsi_launcher_lib::company::market::{self, AGE_BANDS};
use omsi_launcher_lib::company::registrations::{self as reg, AgeFrom, System};
use omsi_launcher_lib::company::vehicles::Region;
use omsi_launcher_lib::company::{self as co, Company, Vehicle};
use serde_json::{json, Value};

fn years_text(age: f64) -> String {
    trf("%{n} years", &[("n", &format!("{:.0}", age.floor().max(0.0)))])
}

/// The plate's age mark as it is said: "CN65" of a current plate, the plate of an older one.
fn mark_of(plate: &str, d: &reg::Period) -> String {
    match d.system {
        System::Current => plate.chars().filter(|c| c.is_ascii_alphanumeric()).take(4).collect::<String>().to_ascii_uppercase(),
        _ => plate.trim().to_string(),
    }
}

/// "Registered Sep 2015 (CN65), 11 years", and how that is known (the row's tip).
pub fn registered(plate: &str, region: Option<Region>, from: AgeFrom, built: &str, today: &str) -> (String, String) {
    let age = co::dates::years_between(built, today);
    if let Some(d) = reg::period_of(plate, region, from) {
        let text = trf("Registered %{month} (%{mark}), %{age}", &[("month", &month_label(&d.month())), ("mark", &mark_of(plate, &d)), ("age", &years_text(age))]);
        let tip = trf("The plate %{plate} was first registered between %{from} and %{to}.", &[("plate", plate.trim()), ("from", &month_label(&d.month())), ("to", &month_label(&format!("{:04}-{:02}", d.to.0, d.to.1)))]);
        return (text, tip);
    }
    let text = trf("Registered %{month}, %{age}", &[("month", &month_label(&co::dates::month_of(built))), ("age", &years_text(age))]);
    let tip = match from {
        AgeFrom::Manual => tr("You set the date of first registration yourself."),
        AgeFrom::New => tr("Registered new by the company."),
        AgeFrom::Model => tr("The age follows the model's era: its plate does not tell it. Set it yourself if you know it."),
        AgeFrom::Plate | AgeFrom::Generated if region == Some(Region::Uk) => tr("The model's plate list tells no age: the age is the market's. Set it yourself if you know it."),
        _ => tr("European plates do not tell a bus's age: the age is the market's. Set it yourself if you know it."),
    };
    let text = if from == AgeFrom::Manual { format!("{text}  ({})", tr("set by you")) } else { text };
    (text, tip)
}

/// A row with a tip on hover.
fn tipped(a: String, b: String, tip: String) -> Value {
    let mut r = row(a, b, false);
    r["tip"] = json!(tip);
    r
}

/// A bus's registration row.
pub fn vehicle_row(c: &Company, v: &Vehicle) -> Value {
    let (text, tip) = registered(&v.plate, v.region, v.age_from, &v.built, &c.date);
    tipped(tr("Registered"), text, tip)
}

/// An offer's registration row (a used one: None for a new bus).
pub fn offer_row(c: &Company, o: &Offer) -> Option<Value> {
    if o.is_new() {
        return None;
    }
    let region = Some(o.listing.bus.region);
    let (text, tip) = registered(&o.registration, region, o.age_from, &o.built, &c.date);
    Some(tipped(tr("Registered"), text, tip))
}

/// How a bus's value is worked out, in a line.
pub fn value_tip(c: &Company, v: &Vehicle) -> String {
    let Some(x) = market::valued(c, v) else { return String::new() };
    let pc = |f: f64| format!("{} %", num(f * 100.0, 0));
    let mut t = trf(
        "New price %{new} × %{age} for %{years} × %{km} for the kilometres × %{condition} for the condition = %{market}.",
        &[("new", &eur(x.new_value)), ("age", &pc(x.age_factor)), ("years", &years_text(x.age)), ("km", &pc(x.km_factor)), ("condition", &pc(x.condition_factor)), ("market", &eur(x.market))],
    );
    if let Some(cap) = x.cap {
        t = format!("{t} {}", trf("Never more than what was paid for it less what it lost since: %{amount}.", &[("amount", &eur(cap))]));
    }
    t
}

/// The value row of an owned bus, with its tip.
pub fn value_row(c: &Company, v: &Vehicle, paid: i64) -> Value {
    tipped(tr("Bought for"), format!("{}  ·  {} {}", eur(paid), tr("worth now"), eur(market::value_of(c, v))), value_tip(c, v))
}

/// The switch to set the age oneself (`on`: set; `year`: the year shown).
pub fn age_switch(on: bool, built: &str) -> Value {
    json!({
        "label": tr("Set the age yourself"), "on": on, "year": built.get(..4).unwrap_or(""),
        "placeholder": tr("Year of first registration"), "apply": tr("Set"),
        "tip": tr("Over what the plate or the market says: its value and running costs follow the year you type."),
    })
}

/// The switch of a used offer (None: a new bus or a demonstrator).
pub fn offer_switch(o: &Offer) -> Option<Value> {
    matches!(o.kind, OfferKind::Used | OfferKind::Batch).then(|| age_switch(o.age_from == AgeFrom::Manual, &o.built))
}

fn band_label(k: usize) -> String {
    match AGE_BANDS[k] {
        (from, Some(to)) => trf("%{from}–%{to} years", &[("from", &from.to_string()), ("to", &to.to_string())]),
        (from, None) => trf("%{from}+ years", &[("from", &from.to_string())]),
    }
}

/// The fleet's value by age band (`Dialog::FleetValue`): per band its buses, what they cost new,
/// what they are worth and what they lost; each bus's working-out on hover.
pub fn fleet_value_view(s: &mut Session, _args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let bands = market::by_age_band(&c);
    let (mut new_all, mut now_all, mut n_all) = (0, 0, 0);
    let rows: Vec<Value> = bands
        .iter()
        .map(|(k, ids, new_value, now)| {
            new_all += new_value;
            now_all += now;
            n_all += ids.len();
            let lost = new_value - now;
            let share = if *new_value > 0 { lost as f64 / *new_value as f64 } else { 0.0 };
            let tip: Vec<String> = ids
                .iter()
                .filter_map(|id| c.vehicle(*id))
                .map(|v| format!("{} {} ({}, {}): {} - {}", v.number, v.name, v.plate, years_text(v.age_years(&c.date)), eur(market::value_of(&c, v)), value_tip(&c, v)))
                .collect();
            json!({
                "band": band_label(*k), "buses": ids.len(), "new": eur(*new_value), "now": eur(*now),
                "lost": if ids.is_empty() { "–".to_string() } else { format!("{}  ({} %)", eur(lost), num(share * 100.0, 0)) },
                "tip": tip.join("\n"),
            })
        })
        .collect();
    let other = c.fleet.len() - n_all;
    json!({
        "title": tr("Fleet value by age"),
        "head": [tr("Age"), tr("Buses"), tr("New price"), tr("Worth now"), tr("Lost")],
        "rows": rows,
        "total": [tr("Together"), n_all, eur(new_all), eur(now_all), eur(new_all - now_all)],
        "note": tr("A bus loses some 18 % in its first year, then 8-10 % of its new price a year, less as it ages, to a tenth of it at 15-18 years; more kilometres than usual and a worse condition take off more. It is never worth more than the company paid for it. Hover over a band for its buses."),
        "other": (other > 0).then(|| trf("%{n} leased or rented buses are not the company's and have no value here.", &[("n", &other.to_string())])),
        "close": tr("Close"),
    })
}

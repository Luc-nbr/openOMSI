//! The fleet (`crates/omsi-launcher-ui/src/company/fleet.rs`, `adverts.rs`): the company's buses
//! as tiles, the dealer (`dealer`), and the advertising on the buses; a bus of the fleet opens to
//! its livery, a service, selling or giving it back; a bus of the dealer can be leased or rented.

use super::dealer::{livery_choice, row};
use super::kit;
use super::words::{day_label, eur, grade, grouped, tr, trf};
use super::{act as apply, Session};
use omsi_launcher_lib::company::adverts::{self as ad, AdKind};
use omsi_launcher_lib::company::dealer as dl;
use omsi_launcher_lib::company::market;
use omsi_launcher_lib::company::{self as co, Company, Tenure, Vehicle};
use serde_json::{json, Value};

/// What a bus is to the company, in a few words, and its tone.
fn status_of(c: &Company, v: &Vehicle) -> (String, &'static str) {
    if v.in_workshop(&c.date) {
        return (trf("In the workshop until %{date}", &[("date", &day_label(v.workshop_until.as_deref().unwrap_or("")))]), "warn");
    }
    if v.km >= v.next_service_km - 1_000.0 {
        return (tr("Service due"), "warn");
    }
    match &v.tenure {
        Tenure::Rented { until, .. } => (trf("Rented until %{date}", &[("date", &day_label(until))]), "early"),
        Tenure::Leased { until, .. } => (trf("Leased until %{date}", &[("date", &day_label(until))]), "early"),
        Tenure::Owned { .. } => (tr("Ready"), "ok"),
    }
}

fn kind_tone(k: AdKind) -> &'static str {
    match k {
        AdKind::Rear => "accent2",
        AdKind::Sides => "early",
        AdKind::FullWrap => "ok",
    }
}

/// The fleet page: its tabs (our buses, the dealer, the advertising) and our buses or the
/// advertising (the dealer is `dealer::view`).
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let tab = ui.get("tab").and_then(Value::as_u64).unwrap_or(0) as usize;
    // (Casual: the buses and the dealer; the advertising runs on as it was signed)
    let shown = if co::autopilot::on(&c) && tab < 2 { 2 } else { 3 };
    let tabs: Vec<String> = [trf("Our buses (%{n})", &[("n", &c.fleet.len().to_string())]), tr("Dealer"), tr("Advertising")].into_iter().take(shown).collect();
    let mut out = json!({ "tabs": tabs, "tab": tab.min(2) });
    out["body"] = match tab {
        2 => adverts(&c),
        1 => Value::Null,
        _ => {
            let left = super::vehicles::left_out(s, &c);
            our_buses(&c, &left)
        }
    };
    out
}

fn our_buses(c: &Company, left: &std::collections::HashSet<String>) -> Value {
    let order = c.dealer.orders.iter().min_by_key(|o| dl::minutes_of(&o.delivery)).map(|first| {
        let n: u32 = c.dealer.orders.iter().map(|o| o.contract.count).sum();
        trf("%{n} buses on order: the next arrive on %{date}.", &[("n", &n.to_string()), ("date", &day_label(&dl::day_of(&first.delivery)))])
    });
    let buses: Vec<Value> = c
        .fleet
        .iter()
        .map(|bus| {
            let (status, tone) = status_of(c, bus);
            json!({
                "id": bus.id, "number": bus.number, "name": bus.name, "file": bus.bus, "livery": bus.livery,
                "advert": ad::advert_of(c, bus.id).map(|k| format!("{}  ·  {}", tr("Ad"), k.advertiser)),
                // (a model the company does not work with any more: the bus stays - Beta 54)
                "sub": if left.contains(&omsi_launcher_lib::company::vehicles::key_of(&bus.bus)) { format!("{}  ·  {}  ·  {}", bus.plate, tr(bus.kind.label()), tr("not in your selection")) } else { format!("{}  ·  {}", bus.plate, tr(bus.kind.label())) },
                "facts": format!("{} km  ·  {}  ·  {}", grouped(bus.km.round()), bus.built.get(..4).unwrap_or(""), trf("%{n} years", &[("n", &format!("{:.0}", bus.age_years(&c.date).floor()))])),
                "condition": { "frac": bus.condition / 100.0, "tone": grade(bus.condition), "tip": trf("Condition: %{n} of 100", &[("n", &format!("{:.0}", bus.condition))]) },
                "status": status, "tone": tone,
            })
        })
        .collect();
    json!({
        "kind": "buses", "order": order, "buses": buses,
        "empty": tr("The fleet is empty. Buy buses at the dealer - new, second-hand or one of the day's offers - or lease one for years or rent one for a few days: each is a bus installed in your OMSI."),
        "to_dealer": tr("To the dealer"),
    })
}

fn adverts(c: &Company) -> Value {
    let list = ad::offers(c);
    let free = ad::free_buses(c).len();
    let offers: Vec<Value> = list
        .iter()
        .map(|o| {
            let lock = !co::levels::unlocked(c, o.kind.feature());
            json!({
                "no": o.no, "advertiser": o.advertiser, "trade": tr(&o.trade), "tone": kind_tone(o.kind),
                "terms": trf("%{kind} on %{n} buses for %{m} months", &[("kind", &tr(o.kind.label())), ("n", &o.buses.to_string()), ("m", &o.months.to_string())]),
                "money": trf("%{amount} a month", &[("amount", &eur(o.monthly()))]), "all": trf("%{amount} in all", &[("amount", &eur(o.total()))]),
                "lock": lock.then(|| trf("From level %{n}", &[("n", &o.kind.feature().level().to_string())])),
            })
        })
        .collect();
    let running: Vec<Value> = c
        .adverts
        .contracts
        .iter()
        .filter(|k| k.running())
        .map(|x| {
            let buses: Vec<String> = x.buses.iter().filter_map(|id| c.vehicle(*id)).map(|v| v.number.clone()).collect();
            json!({
                "no": x.no, "advertiser": x.advertiser, "tone": kind_tone(x.kind),
                "what": format!("{}  ·  {}", tr(x.kind.label()), trf("buses %{list}", &[("list", &buses.join(", "))])),
                "until": trf("%{amount} a month, until %{date}", &[("amount", &eur(x.monthly())), ("date", &day_label(&x.until))]),
                "end": trf("End early (%{amount})", &[("amount", &eur(ad::penalty(x)))]), "again": trf("Press again: %{amount}", &[("amount", &eur(ad::penalty(x)))]),
            })
        })
        .collect();
    json!({
        "kind": "adverts", "offers_title": tr("Advertisers' offers this week"),
        "note": trf("%{n} of your buses carry no advert yet. Advertisers pay by the month, more for buses many people see.", &[("n", &free.to_string())]),
        "offers": offers, "no_offers": tr("No more offers this week: new ones come on Monday."), "to_contract": tr("To the contract"),
        "running_title": tr("Advertising contracts"), "running": running,
        "no_running": tr("No advert on the buses yet. Sign an advertiser's offer: the money comes at every month's end, from the month after the signing."),
        "look": tr("The contract"), "end_tip": tr("Ending a contract early costs three months' fee (or what is left, if less); its buses are free for another advert."),
    })
}

/// A bus of the fleet (`Dialog::Vehicle`): its photo and livery, what it is, its status, and the
/// way to sell it, give it back, service it or train its drivers.
pub fn vehicle_view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let Some(v) = c.vehicle(id).cloned() else { return json!({ "gone": true }) };
    let (q, m) = co::licences::qualified_drivers(&c, v.kind, &v.bus);
    let rows = vec![
        row(tr("Plate"), v.plate.clone(), false),
        super::age::vehicle_row(&c, &v),
        row(tr("Kilometres"), format!("{} km", grouped(v.km.round())), false),
        row(tr("Next service"), format!("{} km", grouped(v.next_service_km)), false),
        row(tr("Condition"), format!("{:.0} / 100", v.condition), false),
        row(tr("Breakdowns"), v.breakdowns.to_string(), false),
        row(tr("Drivers"), trf("%{n} of %{m} may drive it", &[("n", &q.to_string()), ("m", &m.to_string())]), false),
        row(
            tr("Advert"),
            match ad::advert_of(&c, id) {
                Some(k) => format!("{}  ·  {}  ·  {}", k.advertiser, tr(k.kind.label()), trf("until %{date}", &[("date", &day_label(&k.until))])),
                None => tr("none"),
            },
            false,
        ),
        match &v.tenure {
            Tenure::Owned { paid, .. } => super::age::value_row(&c, &v, *paid),
            Tenure::Leased { monthly, until, .. } => row(tr("Leased"), format!("{} / {}  ·  {}", eur(*monthly), tr("month"), day_label(until)), false),
            Tenure::Rented { daily, until } => row(tr("Rented"), format!("{} / {}  ·  {}", eur(*daily), tr("day"), day_label(until)), false),
        },
    ];
    let (status, tone) = status_of(&c, &v);
    let fee = co::livery::design_fee(&c);
    json!({
        "title": format!("{}  ·  {}", v.number, v.name), "id": id, "file": v.bus, "name": v.name, "bus_kind": tr(v.kind.label()),
        "livery": livery_choice(s, &v.bus, &v.livery),
        "design": trf("Design a livery (%{amount})", &[("amount", &eur(fee))]),
        "design_tip": tr("Paint the company's own livery for this bus in the livery studio: its design is paid when it is saved, a bus's painting when the bus is painted."),
        "rows": rows, "status": status, "tone": tone,
        "age": super::age::age_switch(v.age_from == co::registrations::AgeFrom::Manual, &v.built),
        "sell": tr(if matches!(v.tenure, Tenure::Owned { .. }) { "Sell" } else { "Give back" }),
        "service": tr("Service tomorrow"), "train": tr("Train drivers"), "close": tr("Close"),
    })
}

/// Leasing or renting a bus of the dealer (`Dialog::New`): the terms and what it costs.
pub fn new_view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let file = args.get("file").and_then(Value::as_str).unwrap_or("").to_string();
    let livery = args.get("livery").and_then(Value::as_str).unwrap_or("").to_string();
    let how = args.get("how").and_then(Value::as_u64).unwrap_or(0).min(1);
    let days = args.get("days").and_then(Value::as_u64).unwrap_or(7).clamp(1, 60) as i64;
    // (as the player has it, and only a model the company works with: Beta 54)
    let Some(bus) = super::vehicles::market_bus(s, &file).filter(|b| c.vehicles.allows(b)) else { return json!({ "gone": true }) };
    let r = co::economy::rules(c.game_mode);
    let (rows, text, action, cost) = if how == 0 {
        let (monthly, months, residual) = market::lease_offer(&c, &bus);
        (
            vec![row(tr("Monthly rate"), eur(monthly), true), row(tr("Term"), trf("%{n} months", &[("n", &months.to_string())]), false), row(tr("Residual value"), eur(residual), false)],
            tr("No price now: the rate is booked at every month's end, and the bus goes back when the term ends. Insurance is the company's. A lease with a haggled rate comes with the dealer's contract."),
            trf("Lease for %{amount} a month", &[("amount", &eur(monthly))]),
            monthly,
        )
    } else {
        let daily = co::economy::rent_per_day(bus.kind, &r, c.price_index);
        (
            vec![row(tr("Per day"), eur(daily), false), row(tr("Together"), eur(daily * days), true)],
            tr("Paid by the day at each day's close; the bus goes back after the last day. A rented bus is a few years old and kept well."),
            trf("Rent for %{n} days", &[("n", &days.to_string())]),
            daily * days,
        )
    };
    json!({
        "title": bus.name, "file": bus.file, "name": bus.name, "bus_kind": super::vehicles::kind_label(&bus),
        "livery": livery_choice(s, &bus.file, &livery), "design_note": tr("A house livery of your own comes with the livery studio."),
        "hows": [tr("Lease"), tr("Rent")], "how": how, "days": days, "days_label": tr("Days"),
        "rows": rows, "text": text, "main": action, "cost": cost, "cancel": tr("Cancel"),
    })
}

/// A bus joined the fleet: say so (`fleet::joined`).
fn joined(s: &mut Session, id: u32) {
    let number = s.company.as_ref().and_then(|c| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default();
    let mut text = trf("Bus %{n} joined the fleet.", &[("n", &number)]);
    if let Some(note) = s.company.as_ref().and_then(|c| super::people::drivers_note(c, id)) {
        text = format!("{text} {note}");
    }
    s.say(text, false);
}

/// What the fleet's pages and dialogs do.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let popup = |p: Value| Ok(json!({ "popup": p }));
    Some(match what {
        "bus_livery" => {
            let paint = str_of("livery");
            let Some(v) = c.vehicle(id).cloned() else { return Some(Ok(json!({}))) };
            if co::livery::design(&c, &paint).is_some() {
                let cost = co::livery::paint_cost(&c, &v);
                match apply(s, |c| co::livery::paint(c, id, &paint)) {
                    Ok(_) => {
                        let t = trf("Bus %{n} is painted in '%{name}' in the workshop (%{amount}).", &[("n", &v.number), ("name", &paint), ("amount", &eur(cost))]);
                        s.say(t, false);
                        Ok(json!({}))
                    }
                    Err(p) => popup(p),
                }
            } else {
                Ok(apply(s, |c| {
                    market::set_livery(c, id, &paint);
                    Ok(())
                })
                .err()
                .map(|p| json!({ "popup": p }))
                .unwrap_or(json!({})))
            }
        }
        "bus_service" => match apply(s, |c| market::service(c, id)) {
            Ok(()) => {
                let n = c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default();
                let t = trf("Bus %{n} goes to the workshop tomorrow.", &[("n", &n)]);
                s.say(t, false);
                Ok(json!({}))
            }
            Err(p) => popup(p),
        },
        "design_livery" => {
            // (the livery studio is an editor of our game-drawn launcher: the company pays when it
            // is saved there)
            // (`id` a bus of the fleet painted in it, else `file` and `livery` the bus to design
            // for: a new bus's, Casual's company page's)
            let mut cost = co::livery::design_fee(&c);
            let bus = c.vehicle(id);
            if let Some(v) = bus {
                cost += co::livery::paint_cost(&c, v);
            }
            if !c.can_pay(cost) {
                popup(super::popup(s, kit::no_cash(&c, cost)))
            } else {
                let (file, paint) = match bus {
                    Some(v) => (v.bus.clone(), v.livery.clone()),
                    None => (str_of("file"), str_of("livery")),
                };
                let vehicle = bus.map(|v| v.id.to_string()).unwrap_or_default();
                Ok(json!({ "hand_over": format!("livery={file}|{paint}|{vehicle}") }))
            }
        }
        "new_take" => {
            let file = str_of("file");
            let paint = str_of("livery");
            let how = args.get("how").and_then(Value::as_u64).unwrap_or(0);
            let days = args.get("days").and_then(Value::as_u64).unwrap_or(7).clamp(1, 60) as u32;
            let Some(bus) = super::vehicles::market_bus(s, &file).filter(|b| c.vehicles.allows(b)) else { return Some(Ok(json!({}))) };
            let r = co::economy::rules(c.game_mode);
            let cost = if how == 0 { market::lease_offer(&c, &bus).0 } else { co::economy::rent_per_day(bus.kind, &r, c.price_index) * days as i64 };
            if !c.can_pay(cost) {
                return Some(popup(super::popup(s, kit::no_cash(&c, cost))));
            }
            match apply(s, |c| if how == 0 { market::lease(c, &bus, &paint) } else { market::rent(c, &bus, days, &paint) }) {
                Ok(id) => {
                    joined(s, id);
                    Ok(json!({ "joined": id }))
                }
                Err(p) => popup(p),
            }
        }
        "bus_age" => {
            // (an empty year: back to what the plate or the market said)
            let year = str_of("year");
            let date = (!year.trim().is_empty()).then_some(year);
            match apply(s, |c| market::set_age(c, id, date.as_deref())) {
                Ok(()) => Ok(json!({})),
                Err(p) => popup(p),
            }
        }
        "advert_end" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            match apply(s, |c| ad::end_early(c, no)) {
                Ok(fee) => {
                    let t = trf("The advertising contract is ended: %{amount} paid to the advertiser.", &[("amount", &eur(fee))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => popup(p),
            }
        }
        _ => return None,
    })
}

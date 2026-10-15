//! The workshop's stores (`crates/omsi-launcher-ui/src/company/workshop.rs`), as City Bus
//! Manager's spare parts shop: a card a part - its price (the research's discount against the
//! list price), its stock, how many are chosen and what they cost -, the delivery, the sum and
//! Buy; beside them the buses waiting for parts, what is on order, the stores' own ordering and
//! the mechanics. The rules are `company::parts`'.
//!
//! What is chosen (the basket) is the page's: it comes with the view's arguments (`counts`,
//! `express`) and with Buy.

use super::super::kit;
use super::super::words::{day_label, eur, num, tr, trf};
use super::super::{act as apply, Session};
use super::popup_answer;
use crate::Answer;
use omsi_launcher_lib::company::clock;
use omsi_launcher_lib::company::parts::{self as pt, Part};
use omsi_launcher_lib::company::{dates, research, Company};
use serde_json::{json, Value};

/// How many of a part one order takes at most.
pub const MOST: u32 = 30;

/// The basket the page sends: how many of each part (`Part::ALL`'s order), and how it comes.
fn basket(args: &Value) -> (Vec<(Part, u32)>, bool) {
    let counts: Vec<u32> = args.get("counts").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0).round().clamp(0.0, MOST as f64) as u32).collect()).unwrap_or_default();
    let list = Part::ALL.iter().enumerate().map(|(k, p)| (*p, counts.get(k).copied().unwrap_or(0))).collect();
    let express = args.get("express").and_then(Value::as_u64).unwrap_or(0) == 1;
    (list, express)
}

/// When an order's 16:00 is, in words (`workshop::when`).
fn when(c: &Company, due: i64) -> String {
    let day = clock::date_of(due);
    match dates::between(&c.date, &day) {
        0 => tr("today at 16:00"),
        1 => tr("tomorrow at 16:00"),
        _ => format!("{} {}", day_label(&day), clock::hhmm(due)),
    }
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let (list, express) = basket(args);
    let today = c.date.clone();
    let mechanics = pt::mechanics(&c, &today).count();
    let waiting = c.stores.waiting.len();
    let on_order: u32 = c.stores.orders.iter().map(pt::Order::count).sum();
    let worth: i64 = Part::ALL.iter().map(|p| pt::price(&c, *p) * c.stores.count(*p) as i64).sum();
    let fig = |label: &str, value: String, under: &str, tone: &str| json!({ "label": tr(label), "value": value, "under": tr(under), "tone": tone });
    let figures = json!([
        fig("Mechanics", mechanics.to_string(), "at work today", if mechanics == 0 { "warn" } else { "text" }),
        fig("Waiting for parts", waiting.to_string(), "buses in the workshop", if waiting > 0 { "warn" } else { "text" }),
        fig("On order", on_order.to_string(), "parts on their way", "text"),
        fig("In store", eur(worth), "worth of spare parts", "text"),
    ]);
    // the parts' cards
    let cards: Vec<Value> = list
        .iter()
        .map(|(p, n)| {
            let (list_price, price) = (pt::list_price(&c, *p), pt::price(&c, *p));
            let stock = c.stores.count(*p);
            let line = price * *n as i64;
            json!({
                "label": tr(p.label()), "icon": p.icon(),
                "price": eur(price), "was": (price < list_price).then(|| eur(list_price)),
                "count": n, "stock": stock, "line": eur(line), "line_on": line > 0,
            })
        })
        .collect();
    // the delivery, the sum and Buy
    let n: u32 = list.iter().map(|x| x.1).sum();
    let sum = pt::order_cost(&c, &list, express);
    let note = if express { tr("In the stores as soon as it is bought.") } else { trf("Ordered now, it comes %{when}.", &[("when", &when(&c, pt::free_delivery(&c)))]) };
    let foot = json!({
        "ways": [tr("Delivery at 16:00: free"), trf("At once: %{amount}", &[("amount", &eur(pt::express_fee(&c)))])],
        "note": note,
        "sum": trf("Sum: %{amount}", &[("amount", &eur(if n == 0 { 0 } else { sum }))]),
        "sum_tone": if !c.can_pay(sum) && n > 0 { "danger" } else { "text" },
        "chosen": n,
        "buy": tr("Buy"), "clear": tr("Choose nothing"),
    });
    json!({ "figures": figures, "cards": cards, "most": MOST, "foot": foot, "side": side(&c), "in_store": tr("In store") })
}

/// The rest of the stores (`workshop::side`): the buses waiting for parts, what is on order, the
/// stores' own ordering and the mechanics.
fn side(c: &Company) -> Value {
    let waiting: Vec<Value> = c
        .stores
        .waiting
        .iter()
        .map(|w| {
            let parts: Vec<String> = w.parts.iter().map(|p| tr(p.label())).collect();
            json!({ "number": w.number, "parts": parts.join(", "), "since": trf("waiting since %{date}", &[("date", &day_label(&w.since))]) })
        })
        .collect();
    let orders: Vec<Value> = c
        .stores
        .orders
        .iter()
        .map(|o| {
            let parts: Vec<String> = o.parts.iter().map(|(p, n)| format!("{n} × {}", tr(p.label()))).collect();
            let by = if o.auto { tr("ordered by the stores") } else { eur(o.cost) };
            json!({ "what": trf("%{n} parts, %{when}", &[("n", &o.count().to_string()), ("when", &when(c, o.due))]), "parts": format!("{}  ·  {by}", parts.join(", ")) })
        })
        .collect();
    let today = c.date.clone();
    let n = pt::mechanics(c, &today).count();
    let line = if n == 0 {
        tr("No mechanics at work: an outside garage repairs the buses that break down, in its own time. Hire mechanics on the Staff page.")
    } else {
        trf("At work today: %{n}, %{hours} hours of repairs a day.", &[("n", &n.to_string()), ("hours", &num(pt::workshop_hours(c, &today), 0))])
    };
    let faster = 1.0 - research::repair_factor(c);
    let text = if faster > 0.0 { format!("{line} {}", trf("The research makes the repairs %{n} % quicker.", &[("n", &num(faster * 100.0, 0))])) } else { line };
    json!({
        "waiting": { "title": tr("Waiting for parts"), "rows": waiting, "none": tr("No bus waits for a part.") },
        "orders": { "title": tr("On order"), "rows": orders, "none": tr("Nothing is on order.") },
        "auto": { "title": tr("Ordering by itself"), "on": c.stores.auto, "label": tr("The stores order what runs short"), "text": tr("What falls under half of what the stores keep - and what a waiting bus needs - is ordered for the free delivery of the next day.") },
        "mechanics": { "title": tr("Mechanics"), "text": text, "tone": if n == 0 { "warn" } else { "soft" } },
    })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    Some(match what {
        "parts_auto" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            let _ = apply(s, |c| {
                c.stores.auto = on;
                Ok(())
            });
            Ok(json!({}))
        }
        // Buy what was chosen: what is in the way said first, else ordered (`workshop::order`)
        "parts_buy" => {
            let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
            let (list, express) = basket(args);
            let list: Vec<(Part, u32)> = list.into_iter().filter(|x| x.1 > 0).collect();
            let n: u32 = list.iter().map(|x| x.1).sum();
            if n > 0 {
                let cost = pt::order_cost(&c, &list, express);
                if !c.can_pay(cost) {
                    return Some(popup_answer(kit::no_cash(&c, cost).json(c.game_mode.casual())));
                }
            }
            match apply(s, |c| pt::order(c, &list, express)) {
                Ok(cost) => {
                    let text = if express {
                        trf("Bought: %{n} spare parts for %{amount}, in the stores now.", &[("n", &n.to_string()), ("amount", &eur(cost))])
                    } else {
                        let due = s.company.as_ref().and_then(|c| c.stores.orders.last()).map(|o| o.due).unwrap_or(0);
                        trf("Ordered: %{n} spare parts for %{amount}; they come %{when}.", &[("n", &n.to_string()), ("amount", &eur(cost)), ("when", &when(&c, due))])
                    };
                    s.say(text, false);
                    Ok(json!({ "bought": true }))
                }
                Err(p) => popup_answer(p),
            }
        }
        _ => return None,
    })
}

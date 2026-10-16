//! The contracts (`crates/omsi-launcher-ui/src/company/contracts.rs`), City Bus Manager's
//! assignments and service contracts: the trophies to spend, the rating, the goals running and
//! the trophies' bar to the next milestone; then the week's goals (on offer, running,
//! finished), the service contracts (the buses' internet from one of four providers once WLAN is
//! researched, a supplier's fixed diesel price, an outside cleaning company), the tenders the
//! company bid on, or its bus contracts - each with its kind, its payment and days, its term
//! and extensions, its rating (RAG) and default points, its journeys not paid, and the track
//! record the next tenders score (`bus_contracts`). The rules are `company::contracts`' and
//! `company::bus_contracts`'.

use super::super::inbox::{goal_progress, trophy_bar, words};
use super::super::words::{day_label, eur, eur_cents, grade, grouped, num, tr, trf};
use super::super::{act as apply, Session};
use super::{concessions, popup_answer, u32_arg};
use crate::Answer;
use omsi_launcher_lib::company::bus_contracts::{self as bc, Contract, End, Kind, Period, Rag};
use omsi_launcher_lib::company::contracts::{self as ct, Goal, State, PROVIDERS};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// The inbox brought up to date before the goals are looked at (`inbox::ensure`): the welcome
/// said, the week's goals drawn.
fn ensure(s: &mut Session) {
    let Some(c) = s.company.as_ref() else { return };
    if c.inbox.welcomed && c.contracts.week == co::dates::week_of(&c.date) {
        return;
    }
    let _ = apply(s, |c| {
        co::inbox::welcome(c);
        let date = c.date.clone();
        co::contracts::refresh(c, &date);
        Ok(())
    });
}

/// A goal's title in the page's language.
fn goal_title(g: &Goal) -> String {
    let (t, args) = g.aim.title();
    words(t, &args)
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    ensure(s);
    let Some(c) = s.company.clone() else { return Value::Null };
    let part = args.get("part").and_then(Value::as_u64).unwrap_or(0).min(3) as usize;
    let running = c.contracts.goals.iter().filter(|g| g.state == State::Running).count();
    let rating = ct::rating(&c);
    let figures = json!({
        "trophies": { "label": tr("Trophies"), "value": grouped(c.trophies as f64), "under": tr("to spend on research"), "tone": "warn", "tip": tr("Trophies are earned with goals; research costs trophies as well as money.") },
        "rating": { "label": tr("Rating"), "value": format!("{} / 5", num(rating, 1)), "under": "", "tone": grade(c.reputation + 15.0), "stars": rating, "tip": tr("The company's rating is its reputation in stars: trips on time and happy passengers raise it.") },
        "running": { "label": tr("Goals running"), "value": format!("{running} / {}", ct::RUNNING_MAX), "under": tr("at most three at once"), "tone": "text" },
        "milestone": { "label": tr("Trophy milestone"), "bar": trophy_bar(&c) },
    });
    let parts = [tr("Goals"), tr("Service contracts"), tr("Tender bids"), tr("Bus contracts")];
    let body = match part {
        0 => goals(&c),
        1 => services(&c, args),
        2 => concessions::bids(&c),
        _ => bus_contracts(&c),
    };
    json!({ "figures": figures, "parts": parts, "part": part, "body": body })
}

/// The goals: on offer, running, finished (`contracts::goals`).
fn goals(c: &Company) -> Value {
    let offered: Vec<&Goal> = c.contracts.goals.iter().filter(|g| g.state == State::Offered).collect();
    let running: Vec<&Goal> = c.contracts.goals.iter().filter(|g| g.state == State::Running).collect();
    let mut finished: Vec<&Goal> = c.contracts.goals.iter().filter(|g| matches!(g.state, State::Done | State::Failed)).collect();
    finished.reverse();
    let full = running.len() >= ct::RUNNING_MAX;
    let offer_cards: Vec<Value> = offered
        .iter()
        .map(|g| {
            json!({
                "id": g.id, "title": goal_title(g), "rule": tr(g.aim.rule()),
                "trophies": g.trophies.to_string(), "reward": eur(g.reward),
                "days": trf("%{n} days to reach it", &[("n", &g.days.to_string())]),
                "take": tr("Take it on"), "primary": !full,
            })
        })
        .collect();
    let today = c.date.clone();
    let run_cards: Vec<Value> = running
        .iter()
        .map(|g| {
            let left = g.days_left(&today).unwrap_or(0);
            let tone = if g.share() >= 1.0 { "ok" } else if left <= 2 { "warn" } else { "accent2" };
            let when = match left {
                0 => tr("Today is its last day"),
                1 => tr("Until tomorrow"),
                n => trf("Until %{date}: %{n} days left", &[("date", &day_label(g.deadline.as_deref().unwrap_or(""))), ("n", &n.to_string())]),
            };
            json!({
                "id": g.id, "title": goal_title(g),
                "bar": { "frac": g.share(), "tone": tone, "label": goal_progress(g) },
                "trophies": g.trophies.to_string(), "reward": eur(g.reward),
                "when": when, "when_tone": if left <= 2 { "warn" } else { "soft" },
                "rule": tr(g.aim.rule()), "give_up": tr("Give up"),
            })
        })
        .collect();
    let done_rows: Vec<Value> = finished
        .iter()
        .map(|g| {
            let done = g.state == State::Done;
            let ended = day_label(g.ended.as_deref().unwrap_or(""));
            let what = if done {
                trf("Reached on %{date}: %{trophies} trophies, %{amount}", &[("date", &ended), ("trophies", &g.trophies.to_string()), ("amount", &eur(g.reward))])
            } else if g.given_up {
                trf("Given up on %{date}", &[("date", &ended)])
            } else {
                trf("Missed on %{date}", &[("date", &ended)])
            };
            json!({ "title": goal_title(g), "done": done, "what": what })
        })
        .collect();
    json!({
        "kind": "goals",
        "offered": { "title": tr("On offer this week"), "until": offered.first().map(|g| trf("until %{date}", &[("date", &day_label(&g.offered_until))])), "empty": tr("Every goal of this week is taken. New ones come on Monday."), "cards": offer_cards },
        "running": { "title": tr("Running"), "empty": tr("No goal runs now. Take on up to three of those on offer: each counts your company's days from today until it is reached or its time runs out."), "cards": run_cards },
        "finished": { "title": tr("Finished"), "empty": tr("The goals you reach or miss are listed here."), "rows": done_rows },
    })
}

/// The GB a day each provider's slider stands at: the page's, else the contract's (40 for the
/// others).
fn sliders(c: &Company, args: &Value) -> [u32; 4] {
    let mut gb = [40u32; 4];
    if let Some(n) = c.contracts.internet.as_ref() {
        gb[n.provider.min(3)] = n.gb;
    }
    if let Some(a) = args.get("gb").and_then(Value::as_array) {
        for (k, v) in a.iter().enumerate().take(4) {
            if let Some(x) = v.as_f64() {
                gb[k] = (x.round() as u32).clamp(ct::GB_MIN, ct::GB_MAX);
            }
        }
    }
    gb
}

/// The service contracts: the internet, the diesel, the cleaning (`contracts::services`).
fn services(c: &Company, args: &Value) -> Value {
    json!({ "kind": "services", "internet": internet(c, args), "fuel": fuel(c), "cleaning": cleaning(c) })
}

/// The buses' internet: the four providers side by side (`contracts::internet`).
fn internet(c: &Company, args: &Value) -> Value {
    let title = tr("Internet on the buses");
    if !co::research::has(c, co::research::keys::WLAN) {
        return json!({ "title": title, "locked": tr("The buses have no WLAN yet. Research it, and the internet providers make their offers here: your passengers like to be online, and a connection that lasts the day raises your reputation."), "to_research": tr("To the research") });
    }
    let active = ct::internet_today(c).cloned();
    let next = c.contracts.internet_next.clone();
    let used = c.contracts.data.used;
    let head = if used > 0.0 {
        trf("Used yesterday: %{n} GB", &[("n", &num(used, 1))])
    } else {
        let pax = c.history.last().map(|d| d.passengers as f64).unwrap_or(0.0);
        trf("Your passengers would use about %{n} GB a day", &[("n", &num(pax * ct::GB_PER_PASSENGER, 0))])
    };
    let gb = sliders(c, args);
    let providers: Vec<Value> = PROVIDERS
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let mine = active.as_ref().is_some_and(|a| a.provider == k);
            let coming = matches!(next, Some(ct::NetChange::Take { provider, .. }) if provider == k);
            let mut out = json!({
                "k": k, "name": p.name, "mine": mine,
                "sat": trf("%{pct} % customer satisfaction", &[("pct", &p.satisfaction.to_string())]), "sat_frac": p.satisfaction as f64 / 100.0, "sat_tone": grade(p.satisfaction as f64),
                "per": trf("%{amount} a GB a day", &[("amount", &eur_cents(p.per_gb * c.price_index))]),
                "gb": gb[k], "price": trf("%{amount} a day", &[("amount", &eur(ct::net_price(c, k, gb[k])))]),
                "from": tr("Changes are active from tomorrow"), "gb_tip": tr("The GB a day the contract gives the buses' WLAN"),
            });
            if let (true, Some(d)) = (mine, ct::data_today(c)) {
                let all = (d.gb + d.topped).max(0.1);
                let left = d.left();
                let tone = if left / all > 0.3 { "ok" } else if left > 0.0 { "warn" } else { "danger" };
                let more = ((d.gb * 0.5 / 5.0).round() * 5.0).max(5.0) as u32;
                out["data"] = json!({ "frac": left / all, "tone": tone, "label": trf("%{n} GB left today", &[("n", &num(left, 1))]) });
                out["top_up"] = json!({ "gb": more, "label": trf("+%{n} GB for %{amount}", &[("n", &more.to_string()), ("amount", &eur(ct::top_up_price(c, k, more)))]), "tip": tr("More data for today only, paid at once") });
            }
            let mut state = String::new();
            if coming {
                if let Some(ct::NetChange::Take { gb: g, .. }) = &next {
                    state = trf("From tomorrow: %{n} GB a day", &[("n", &g.to_string())]);
                }
            } else if mine && next == Some(ct::NetChange::Cancel) {
                state = tr("Ends tonight");
            } else if mine {
                state = trf("Your contract: %{n} GB a day", &[("n", &active.as_ref().map(|a| a.gb).unwrap_or(0).to_string())]);
            }
            out["state"] = json!(state);
            out["state_tone"] = json!(if coming { "accent2" } else if mine { "ok" } else { "warn" });
            let current = mine && active.as_ref().is_some_and(|a| a.gb == gb[k]) && !matches!(next, Some(ct::NetChange::Take { .. }) | Some(ct::NetChange::Cancel));
            if mine {
                if !current {
                    out["change"] = json!(tr(if next == Some(ct::NetChange::Cancel) { "Keep it" } else { "Change" }));
                }
                if next != Some(ct::NetChange::Cancel) {
                    out["cancel"] = json!(tr("Cancel"));
                }
            } else if !coming || next.as_ref().is_some_and(|n| matches!(n, ct::NetChange::Take { gb: g, .. } if *g != gb[k])) {
                out["accept"] = json!(tr("Accept"));
            }
            out
        })
        .collect();
    json!({ "title": title, "head": head, "providers": providers, "min": ct::GB_MIN, "max": ct::GB_MAX, "step": ct::GB_STEP })
}

/// A supplier's fixed diesel price (`contracts::fuel`).
fn fuel(c: &Company) -> Value {
    let usual = co::inbox::usual_diesel(c);
    let market = co::inbox::market_fuel_factor(c, &c.date);
    let today = trf("Diesel today: %{price} a litre", &[("price", &eur_cents(usual * market))]);
    let change = ((market - 1.0).abs() > 0.005).then(|| json!({ "text": trf("%{pct} against the usual %{price}, by the market's events", &[("pct", &format!("{:+.0} %", (market - 1.0) * 100.0)), ("price", &eur_cents(usual))]), "tone": if market > 1.0 { "warn" } else { "ok" } }));
    let mut out = json!({ "title": tr("Diesel supplier"), "today": today, "change": change });
    match c.contracts.fuel.clone() {
        Some(d) => {
            let started = d.from.as_str() <= c.date.as_str();
            let t = if started { "Fixed at %{price} a litre until %{date}." } else { "Fixed at %{price} a litre from tomorrow until %{date}." };
            out["fixed"] = json!(trf(t, &[("price", &eur_cents(d.price)), ("date", &day_label(&d.until))]));
            out["fixed_note"] = json!(tr("A fixed price runs until its last day: it cannot be ended early."));
        }
        None => {
            out["offer"] = json!(tr("Fix the price for a while: from tomorrow the company pays the same, whatever the market does. Sign before a price rise the inbox tells of - but a fixed price also stays when diesel gets cheaper."));
            out["terms"] = json!(ct::FUEL_TERMS
                .iter()
                .enumerate()
                .map(|(k, (days, _))| {
                    let (price, until) = ct::fuel_offer(c, k);
                    json!({ "k": k, "text": trf("%{n} days: %{price} a litre, until %{date}", &[("n", &days.to_string()), ("price", &eur_cents(price)), ("date", &day_label(&until))]) })
                })
                .collect::<Vec<_>>());
            out["sign"] = json!(tr("Fix it"));
        }
    }
    out
}

/// The outside cleaning company (`contracts::cleaning`).
fn cleaning(c: &Company) -> Value {
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let per_bus = (ct::CLEANING_PER_BUS * c.price_index / 100.0).round() as i64 * 100;
    let text = trf(
        "An outside company cleans your buses every night: %{amount} a bus, %{total} a day for your %{n} buses. Clean buses please the passengers: the reputation rises a little every day they run.",
        &[("amount", &eur(per_bus)), ("total", &eur(ct::cleaning_price(c))), ("n", &buses.to_string())],
    );
    let k = c.contracts.cleaning.clone();
    let (state, tone) = match &k {
        Some(k) if k.since.as_str() > c.date.as_str() => (tr("Starts tomorrow"), "accent2"),
        Some(k) if k.until.as_deref() == Some(c.date.as_str()) => (tr("Ends tonight"), "warn"),
        Some(k) => (trf("Cleaning your buses since %{date}", &[("date", &day_label(&k.since))]), "ok"),
        None => (String::new(), "text"),
    };
    let open = k.as_ref().is_some_and(|k| k.until.is_none());
    json!({ "title": tr("Cleaning company"), "text": text, "state": state, "tone": tone, "open": open, "end": tr("End the contract"), "sign": tr("Sign the contract") })
}

// --- the bus contracts -----------------------------------------------------------------------------

/// A contract's kind and how it is paid, in words.
pub fn kind_text(k: &Contract) -> String {
    let per = match k.period {
        Period::Day => tr("paid by the operating day"),
        Period::Week => tr("paid by the week"),
        Period::Month => tr("paid by the month"),
    };
    format!("{}  ·  {per}", tr(k.kind.label()))
}

/// What a contract pays now by its period, and a year.
pub fn pay_text(c: &Company, k: &Contract) -> String {
    let each = eur(bc::payment(c, k));
    let year = eur(k.year_now(c.contract_index).round() as i64);
    match k.period {
        Period::Day => trf("%{amount} an operating day  ·  %{year} a year", &[("amount", &each), ("year", &year)]),
        Period::Week => trf("%{amount} a week  ·  %{year} a year", &[("amount", &each), ("year", &year)]),
        Period::Month => trf("%{amount} a month, %{n} months a year  ·  %{year} a year", &[("amount", &each), ("n", &k.months().to_string()), ("year", &year)]),
    }
}

/// Its days, and whose the fares are.
pub fn days_text(c: &Company, k: &Contract) -> String {
    let days = match k.kind {
        Kind::Hire => trf("%{n} set days a year", &[("n", &k.days_a_year.to_string())]),
        _ => trf("%{n} operating days a year", &[("n", &k.days_a_year.to_string())]),
    };
    let fares = match k.kind {
        Kind::Gross => tr("the fares go to the authority"),
        Kind::Net => match bc::fare_cap(c, &k.line) {
            Some(cap) => trf("the fares are yours, at most %{fare} on the line this year", &[("fare", &eur_cents(cap as f64))]),
            None => tr("the fares are yours, rising by 15 % a year at most"),
        },
        Kind::Hire => tr("no fares: the hirer pays the day"),
    };
    format!("{days}  ·  {fares}")
}

/// Its term and extensions.
pub fn term_text(k: &Contract) -> String {
    let (_, max) = bc::term(k.kind);
    trf("%{from} to %{until}  ·  %{n} of %{max} extension years earned", &[("from", &day_label(&k.from)), ("until", &day_label(&k.until)), ("n", &k.extended.to_string()), ("max", &max.to_string())])
}

/// Its rating as a badge.
pub fn rag_json(r: Rag) -> Value {
    let tone = match r {
        Rag::Green => "ok",
        Rag::Orange => "warn",
        Rag::Red => "danger",
    };
    json!({ "text": tr(r.label()).to_uppercase(), "tone": tone })
}

/// Its points of the last twelve months.
pub fn points_text(c: &Company, k: &Contract) -> String {
    let (def, q, _) = k.points(&c.date);
    trf("%{net} net default points over twelve months (%{def} default, %{q} quality)", &[("net", &num(k.net_points(&c.date), 0)), ("def", &num(def, 0)), ("q", &num(q, 0))])
}

/// Its journeys not paid in the last twelve months.
pub fn unpaid_text(c: &Company, k: &Contract) -> String {
    let (n, amount) = k.unpaid(&c.date);
    if n == 0 {
        tr("Every journey paid in twelve months")
    } else {
        trf("%{n} journeys not paid in twelve months: %{amount}", &[("n", &grouped(n as f64)), ("amount", &eur(amount))])
    }
}

/// The company's track record in words: its quality points in the next tenders.
pub fn record_text(c: &Company) -> String {
    let pts = bc::scored_points(c);
    let q = num(bc::quality_points(pts), 1);
    match bc::track_record(c) {
        Some(_) => trf("Your track record: %{pts} average net default points a contract over twelve months, %{q} of 30 quality points in the next tenders.", &[("pts", &num(pts, 1)), ("q", &q)]),
        None => trf("No track record yet: the authority scores a newcomer as %{pts} points, %{q} of 30 quality points in the next tenders.", &[("pts", &num(pts, 1)), ("q", &q)]),
    }
}

/// The bus contracts (`bus_contracts`): those running and to begin, with their terms, rating and
/// points; those ended in the last twelve months; the track record.
fn bus_contracts(c: &Company) -> Value {
    let caption = |line: &str| c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.caption.clone()).unwrap_or_default();
    let mut live: Vec<&Contract> = bc::all(c).iter().filter(|k| k.live()).collect();
    live.sort_by(|a, b| a.number.cmp(&b.number).then(a.from.cmp(&b.from)));
    let rows: Vec<Value> = live
        .iter()
        .map(|k| {
            let month = if k.month.month == co::dates::month_of(&c.date) && k.month.journeys > 0 {
                trf("This month: %{paid} paid, %{n} journeys, %{p} % on time", &[("paid", &eur(k.month.paid)), ("n", &grouped(k.month.journeys as f64)), ("p", &format!("{:.0}", 100.0 * k.month.on_time as f64 / k.month.journeys as f64))])
            } else {
                tr("This month: no journey yet")
            };
            let later = co::dates::between(&c.date, &k.from) > 0;
            let cost = bc::leaving_cost(c, k, co::econ_uk::EARLY_END_MONTHS);
            json!({
                "line": k.line, "number": k.number, "caption": caption(&k.line),
                "kind": kind_text(k), "pay": pay_text(c, k), "days": days_text(c, k), "term": term_text(k),
                "rag": rag_json(k.rag(&c.date)), "points": points_text(c, k), "unpaid": unpaid_text(c, k),
                "month": if later { trf("Begins on %{date}", &[("date", &day_label(&k.from))]) } else { month },
                "back": trf("Hand back (%{amount})", &[("amount", &eur(cost))]),
                "back_tip": tr("Ends the contract today: it costs a month's average payment, and a map line goes with it."),
            })
        })
        .collect();
    let ended: Vec<Value> = bc::all(c)
        .iter()
        .filter(|k| !k.live())
        .map(|k| {
            let why = match k.end {
                Some(End::HandedBack) => tr("handed back"),
                Some(End::Terminated) => tr("ended by the authority for default"),
                _ => tr("its term ran out"),
            };
            json!({ "number": k.number, "text": trf("Ended on %{date}: %{why}", &[("date", &day_label(k.ended.as_deref().unwrap_or(&k.until))), ("why", &why)]), "points": points_text(c, k) })
        })
        .collect();
    json!({
        "kind": "buses", "title": tr("Your bus contracts"), "record": record_text(c),
        "rows": rows, "ended": ended, "ended_title": tr("Ended in the last twelve months").to_uppercase(),
        "empty": tr("The company has no bus contract. Win a tender on the Concessions page; your own school lines run as a private hire."),
    })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let id = u32_arg(args, "id");
    let answer = |r: Result<(), Value>| -> Answer { Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({}))) };
    Some(match what {
        "goal_take" => match apply(s, |c| ct::take(c, id)) {
            Ok(()) => {
                s.say(tr("Goal taken on: it counts from today."), false);
                Ok(json!({}))
            }
            Err(p) => popup_answer(p),
        },
        "goal_give_up" => answer(apply(s, |c| ct::give_up(c, id))),
        "net_take" => {
            let k = args.get("provider").and_then(Value::as_u64).unwrap_or(0) as usize;
            let gb = (args.get("gb").and_then(Value::as_f64).unwrap_or(40.0).round() as u32).clamp(ct::GB_MIN, ct::GB_MAX);
            match apply(s, |c| ct::take_internet(c, k, gb)) {
                Ok(()) => {
                    s.say(tr("The internet contract changes from tomorrow."), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "net_cancel" => answer(apply(s, |c| {
            ct::cancel_internet(c);
            Ok(())
        })),
        "net_top_up" => {
            let gb = u32_arg(args, "gb");
            match apply(s, |c| ct::top_up(c, gb)) {
                Ok(price) => {
                    s.say(trf("Topped up: %{n} GB for %{amount}.", &[("n", &gb.to_string()), ("amount", &eur(price))]), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "fuel_sign" => {
            let k = args.get("term").and_then(Value::as_u64).unwrap_or(0) as usize;
            match apply(s, |c| ct::sign_fuel(c, k)) {
                Ok(()) => {
                    s.say(tr("The diesel's price is fixed from tomorrow."), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "cleaning_sign" => answer(apply(s, ct::sign_cleaning)),
        "contract_hand_back" => {
            let line = args.get("line").and_then(Value::as_str).unwrap_or("").to_string();
            match apply(s, |c| bc::hand_back(c, &line).ok_or("There is no such contract.")) {
                Ok(cost) => {
                    s.say(trf("The contract is handed back: %{amount}.", &[("amount", &eur(cost))]), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "cleaning_end" => answer(apply(s, |c| {
            ct::cancel_cleaning(c);
            Ok(())
        })),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    /// Every text of the contracts' and the concessions' pages is named in every language (the
    /// bus contracts' in crates/omsihub-locales/locales/company-bus-contracts.yml), and so are the
    /// contracts' kinds and ratings and the core's refusals the pages show.
    #[test]
    fn every_contract_text_is_translated() {
        use omsi_launcher_lib::company::bus_contracts::{Kind, Rag};
        let mut texts: Vec<String> = Vec::new();
        for src in [include_str!("contracts.rs"), include_str!("concessions.rs")] {
            for lead in ["tr(\"", "trf(\""] {
                let mut rest = src;
                while let Some(k) = rest.find(lead) {
                    let after = &rest[k + lead.len()..];
                    rest = after;
                    let t: String = after.chars().take_while(|c| *c != '"').collect();
                    if t.chars().any(char::is_alphabetic) && !t.contains('\\') {
                        texts.push(t);
                    }
                }
            }
        }
        texts.extend(Kind::ALL.iter().map(|k| k.label().to_string()));
        texts.extend([Rag::Green, Rag::Orange, Rag::Red].iter().map(|r| r.label().to_string()));
        texts.extend(["Above the authority's cap: a bid above it does not count.".to_string(), "There is no such contract.".to_string()]);
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}

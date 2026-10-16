//! The concession market (`crates/omsi-launcher-ui/src/company/concessions.rs`): the map's
//! lines out to tender with their week, the concessions the company holds, and the auction of
//! the tender picked as it runs with the clock - the rivals and their bids, the least bid that
//! leads now and its chance, buying the line outright - and how it ended. The rules are
//! `company::concessions`'.
//!
//! The lines' weeks are read from the timetable of the company's week on a thread of their own
//! (the page says so meanwhile). The Contracts page's "Tender bids" (`bids`) lists the tenders
//! the company bid on.

use super::super::words::{day_label, eur, eur_cents, tr, trf};
use super::super::{act as apply, busy, may_step, simulate, Session};
use super::{more, more_state, u32_arg};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::auction::Who;
use omsi_launcher_lib::company::clock::{self as ck, Step};
use omsi_launcher_lib::company::concessions::{self as cn, Outcome, Tender, Week};
use omsi_launcher_lib::company::{dates, Company};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
pub struct TendersState {
    /// The lines' weeks, for the map, the first day and the timetable's revision they were read
    /// for; a read on its way.
    weeks: HashMap<String, Week>,
    weeks_for: Option<(String, String, u64)>,
    reading: bool,
    /// The tender in the auction panel.
    pub selected: Option<u32>,
}

/// The Monday of a date's week (the week is read from it).
fn monday_of(date: &str) -> String {
    let d = dates::parse(date).unwrap_or(0);
    dates::fmt(d - dates::weekday(d) as i64)
}

/// The weeks of every map line, read in the background once per map and week - and again when
/// the line editor wrote the map's timetable (`concessions::weeks`).
fn weeks(s: &Session) {
    let Some(c) = s.company.as_ref() else { return };
    let monday = monday_of(&c.date);
    let key = (c.map.clone(), monday.clone(), s.timetable);
    let mut m = more(s);
    if m.tenders.weeks_for.as_ref() == Some(&key) {
        return;
    }
    m.tenders.weeks_for = Some(key.clone());
    m.tenders.reading = true;
    let map = c.map.clone();
    let _ = std::thread::Builder::new().name("company weeks".into()).spawn(move || {
        let days: Vec<Vec<core::LineInfo>> = (0..7).filter_map(|k| core::list_lines(&map, &dates::add(&monday, k)).ok()).collect();
        let mut out = HashMap::new();
        for l in days.first().map(|d| d.iter().map(|l| l.name.clone()).collect::<Vec<_>>()).unwrap_or_default() {
            out.insert(l.to_lowercase(), cn::week_of(&days, &l));
        }
        let mut m = more_state().lock().unwrap_or_else(|e| e.into_inner());
        // (read for the week still wanted: else a newer read is on its way)
        if m.tenders.weeks_for.as_ref() == Some(&key) {
            m.tenders.weeks = out;
            m.tenders.reading = false;
        }
        drop(m);
        let mut s = super::super::session().lock().unwrap_or_else(|e| e.into_inner());
        s.generation += 1;
    });
}

/// When a minute of the clock is: its time, and its day when it is not today.
fn when(c: &Company, m: i64) -> String {
    let d = ck::date_of(m);
    if d == c.date {
        ck::hhmm(m)
    } else {
        format!("{} {}", day_label(&d), ck::hhmm(m))
    }
}

/// "2:10 h".
fn span(minutes: i64) -> String {
    let m = minutes.max(0);
    format!("{}:{:02} h", m / 60, m % 60)
}

/// The company's own bid on a tender in words and its tone (`concessions::my_bid_text`).
pub fn my_bid_text(c: &Company, t: &Tender) -> Option<(String, &'static str)> {
    let b = cn::my_bid(c, t, ck::now(c))?;
    let bid = trf("Your bid: %{amount} at %{when}", &[("amount", &eur(b.amount)), ("when", &when(c, b.at))]);
    let bid = if b.count > 1 { format!("{bid} ({})", trf("%{n} bids placed", &[("n", &b.count.to_string())])) } else { bid };
    Some(match &b.result {
        None if b.leads => (format!("{bid}  ·  {}", tr("leading")), "ok"),
        None => (format!("{bid}  ·  {}", tr("beaten: bid again before it closes")), "warn"),
        Some((winner, _)) if winner.is_empty() => (format!("{bid}  ·  {}", tr("you won")), "ok"),
        Some((winner, amount)) => (format!("{bid}  ·  {}", trf("won by %{who} with %{amount}", &[("who", winner), ("amount", &eur(*amount))])), "warn"),
    })
}

/// A tender's state in a few words, and its tone (`concessions::state_of`).
pub fn state_of(c: &Company, t: &Tender) -> (String, &'static str) {
    let now = ck::now(c);
    match &t.outcome {
        None if now < t.opens_at => (trf("Opens %{when}", &[("when", &when(c, t.opens_at))]), "soft"),
        None => {
            let placed = cn::bids(c, t, now);
            let lead = match placed.last() {
                Some(p) if p.who == Who::Player => trf("you lead with %{amount}", &[("amount", &eur(p.amount))]),
                Some(p) => trf("%{who} leads with %{amount}", &[("who", &cn::bidder_name(c, t, p.who)), ("amount", &eur(p.amount))]),
                None => tr("no bid yet"),
            };
            let mine = !t.offers.is_empty();
            let tone = match placed.last() {
                Some(p) if p.who == Who::Player => "ok",
                _ if mine => "warn",
                _ => "accent2",
            };
            (format!("{}  ·  {}", trf("Running until %{time}", &[("time", &ck::hhmm(t.closes_at))]), lead), tone)
        }
        Some(Outcome::Won { amount, bought, .. }) => (trf(if *bought { "Bought for %{amount}" } else if t.scored() { "Won with an ask of %{amount}" } else { "Won for %{amount}" }, &[("amount", &eur(*amount))]), "ok"),
        Some(Outcome::Lost { winner, amount, .. }) => (trf(if t.scored() { "Lost to %{who}, who asked %{amount}" } else { "Lost to %{who} for %{amount}" }, &[("who", winner), ("amount", &eur(*amount))]), "warn"),
        Some(Outcome::NoBid { winner, amount }) if !winner.is_empty() => (trf("Went to %{who} for %{amount}", &[("who", winner), ("amount", &eur(*amount))]), "faint"),
        Some(Outcome::NoBid { .. }) => (tr("Nobody bid"), "faint"),
    }
}

fn caption_of(t: &Tender) -> String {
    if t.caption.is_empty() {
        t.line.clone()
    } else {
        t.caption.clone()
    }
}

/// The tenders the company bid on, the latest first (the Contracts page's "Tender bids",
/// `concessions::bids_list`).
pub fn bids(c: &Company) -> Value {
    let mut list: Vec<&Tender> = c.concessions.tenders.iter().filter(|t| !t.offers.is_empty()).collect();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(b.closes_at.cmp(&a.closes_at)));
    let rows: Vec<Value> = list
        .iter()
        .map(|t| {
            let (state, tone) = state_of(c, t);
            let mine = my_bid_text(c, t);
            json!({ "id": t.id, "number": t.number, "caption": caption_of(t), "bid": mine.as_ref().map(|m| &m.0), "bid_tone": mine.map(|m| m.1), "state": state, "tone": tone })
        })
        .collect();
    json!({
        "title": tr("Your tender bids"), "rows": rows,
        "empty": tr("The company has bid on no tender yet. The Concessions page lists the lines out to tender: bid there, and your bids show here with their deadline and result."),
    })
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    weeks(s);
    let Some(c) = s.company.clone() else { return Value::Null };
    let q = cn::quality(&c, false);
    let foot = trf("Scored 70 % price, 30 % quality: the lowest ask gets all 70 price points, others lowest ÷ ask of them. Your quality %{q} of 100 (reputation, punctuality, fleet) is worth %{p} points. The best total wins.", &[("q", &format!("{q:.0}")), ("p", &format!("{:.1}", omsi_launcher_lib::company::auction::quality_points(q)))]);
    let mut list: Vec<Tender> = c.concessions.tenders.clone();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(if a.open() { a.opens_at.cmp(&b.opens_at) } else { b.closes_at.cmp(&a.closes_at) }));
    // (the panel shows the one picked, else the first running or announced)
    let (weeks, reading, selected) = {
        let mut m = more(s);
        if m.tenders.selected.is_none_or(|id| !list.iter().any(|t| t.id == id)) {
            m.tenders.selected = list.first().map(|t| t.id);
        }
        (m.tenders.weeks.clone(), m.tenders.reading, m.tenders.selected)
    };
    let today_read = s.today.is_some();
    json!({
        "foot": foot,
        "tenders": tenders(&c, &list, &weeks, reading, today_read, selected),
        "held": held(&c),
        "auction": auction(&c, selected, args),
    })
}

/// The tenders: announced and running first, then the results (`concessions::tenders`).
fn tenders(c: &Company, list: &[Tender], weeks: &HashMap<String, Week>, reading: bool, today_read: bool, selected: Option<u32>) -> Value {
    let note = cn::may_add_directly(c).then(|| tr("On an easy economy the map's lines are taken on directly on the Lines page and their concessions renew by themselves; tenders are offered all the same, for the practice."));
    let empty = list.is_empty().then(|| tr(if !today_read { "Reading the timetable…" } else { "No line is out to tender now. A new round comes every four weeks; a line of the map can also be applied for on the Lines page." }));
    let now = ck::now(c);
    let rows: Vec<Value> = list
        .iter()
        .map(|t| {
            let week = t.week.or_else(|| weeks.get(&t.line.to_lowercase()).copied());
            let figures = match week {
                Some(wk) if wk.km < 1.0 => trf("%{t} tours, %{n} trips a week", &[("t", &wk.tours.to_string()), ("n", &wk.trips.to_string())]),
                Some(wk) => trf("%{t} tours, %{km} km a week  ·  about %{amount}", &[("t", &wk.tours.to_string()), ("km", &format!("{:.0}", wk.km)), ("amount", &eur(cn::week_revenue(c, &wk, 1.0)))]),
                None if reading => tr("Reading its week…"),
                None => trf("%{t} tours, %{km} km on the day it was offered", &[("t", &t.day_tours.to_string()), ("km", &format!("{:.0}", t.day_km))]),
            };
            let (state, tone) = state_of(c, t);
            let (bid, bid_tone) = my_bid_text(c, t).unwrap_or_else(|| (tr("You have not bid"), "faint"));
            let badge = if t.renewal {
                Some(json!({ "text": tr("renewal").to_uppercase(), "tone": "accent2" }))
            } else if t.running(now) {
                Some(json!({ "text": tr("live").to_uppercase(), "tone": "ok" }))
            } else {
                None
            };
            json!({
                "id": t.id, "number": t.number, "caption": caption_of(t), "dim": !t.open(), "on": selected == Some(t.id),
                "figures": figures, "state": state, "tone": tone, "bid": bid, "bid_tone": bid_tone, "badge": badge,
            })
        })
        .collect();
    json!({ "title": tr("Tenders"), "note": note, "empty": empty, "rows": rows })
}

/// The concessions the company holds and its own lines (`concessions::held`).
fn held(c: &Company) -> Value {
    let today = c.date.clone();
    let per_km = cn::reference_per_km(c);
    let rows: Vec<Value> = c
        .concessions
        .held
        .iter()
        .map(|h| {
            let left = dates::between(&today, &h.until);
            let until = if dates::between(&today, &h.from) > 0 { trf("from %{date}", &[("date", &day_label(&h.from))]) } else { trf("until %{date}", &[("date", &day_label(&h.until))]) };
            let price = trf("%{p} % of the reference: %{km} a kilometre", &[("p", &format!("{:.0}", h.price * 100.0)), ("km", &eur_cents(per_km * h.price))]);
            json!({ "number": h.number, "until": until, "tone": if left <= cn::RENEW_BEFORE { "warn" } else { "text" }, "price": price, "direct": h.direct.then(|| tr("direct").to_uppercase()) })
        })
        .collect();
    let lic = eur((cn::LICENCE as f64 * c.price_index).round() as i64);
    let own: Vec<Value> = c
        .lines
        .iter()
        .filter(|x| x.own)
        .map(|x| json!({ "number": x.number, "caption": x.caption, "text": trf("Your own line: no concession, a licence of %{amount} a month", &[("amount", &lic)]) }))
        .collect();
    let empty = (rows.is_empty() && own.is_empty()).then(|| tr("The company holds no concession yet. Win a tender, or apply for a line of the map on the Lines page."));
    json!({ "title": tr("Your concessions"), "rows": rows, "own": own, "empty": empty })
}

/// The auction of the tender picked (`concessions::auction`). `amount` (euros): the bid slider's
/// sum the page holds.
fn auction(c: &Company, selected: Option<u32>, args: &Value) -> Value {
    let title = tr("Auction");
    let Some(t) = selected.and_then(|id| c.concessions.tenders.iter().find(|t| t.id == id)) else {
        return json!({ "title": title, "none": tr("Pick a tender on the left: its auction is shown here as it runs.") });
    };
    let now = ck::now(c);
    let (state, tone) = state_of(c, t);
    let mine = my_bid_text(c, t);
    let least = cn::min_bid(c, t);
    let figures = json!([
        { "label": tr("The reference").to_uppercase(), "value": eur(t.reference), "tone": "text" },
        if t.open() { json!({ "label": tr("Ask at most now").to_uppercase(), "value": eur(least), "tone": "text" }) } else { json!({ "label": tr("Term").to_uppercase(), "value": trf("%{n} weeks", &[("n", &t.weeks.to_string())]), "tone": "text" }) },
        { "label": tr("Buy outright").to_uppercase(), "value": eur(t.buy_out()), "tone": "accent2" },
    ]);
    let len = (t.closes_at - t.opens_at).max(1);
    let frac = if t.open() { ((now - t.opens_at) as f64 / len as f64).clamp(0.0, 1.0) } else { 1.0 };
    let times = if !t.open() {
        trf("Closed at %{when}", &[("when", &when(c, t.closes_at))])
    } else if now < t.opens_at {
        trf("Opens %{when}, runs %{len}", &[("when", &when(c, t.opens_at)), ("len", &span(len))])
    } else {
        trf("Closes at %{time}: %{left} left", &[("time", &ck::hhmm(t.closes_at)), ("left", &span(t.closes_at - now))])
    };
    // the bidders
    let placed = cn::bids(c, t, if t.open() { now } else { t.closes_at });
    let leader = placed.last().map(|p| p.who);
    // (the points: kept at the close, else as they stand now)
    let board = if t.open() { cn::standings(c, t, &placed) } else { t.standings.clone() };
    let points_of = |name: &str| board.iter().find(|s| s.name == name).map(|s| trf("%{p} price + %{q} quality = %{t} points", &[("p", &format!("{:.1}", s.price)), ("q", &format!("{:.1}", s.quality)), ("t", &format!("{:.1}", s.total))]));
    let mut bidders: Vec<Value> = t
        .bidders
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = cn::rival(c, *id)?;
            let last = placed.iter().rev().find(|p| p.who == Who::Rival(i)).map(|p| p.amount);
            let kind = points_of(&r.name).unwrap_or_else(|| format!("{}  ·  {}", tr(r.character.label()), trf("quality %{q}", &[("q", &format!("{:.0}", r.quality))])));
            Some(json!({ "name": r.name, "kind": kind, "value": last.map(eur).unwrap_or_else(|| tr("no bid")), "has": last.is_some(), "leads": leader == Some(Who::Rival(i)) }))
        })
        .collect();
    let mine_last = placed.iter().rev().find(|p| p.who == Who::Player).map(|p| p.amount);
    let kind = points_of("").unwrap_or_else(|| trf("your quality %{q}", &[("q", &format!("{:.0}", cn::quality_in(c, t)))]));
    bidders.push(json!({ "name": tr("You"), "kind": kind, "value": mine_last.map(eur).unwrap_or_else(|| tr("no bid")), "has": mine_last.is_some(), "leads": leader == Some(Who::Player) }));
    let log: Vec<Value> = placed.iter().rev().take(30).map(|p| json!({ "at": ck::hhmm(p.at), "who": if p.who == Who::Player { tr("You") } else { cn::bidder_name(c, t, p.who) }, "amount": eur(p.amount), "me": p.who == Who::Player })).collect();
    // what to do
    let actions = if t.running(now) {
        // (an ask: from half the reference up to the most that leads now)
        let hi = (least as f64 / 100.0).max(100.0);
        let lo = ((t.reference as f64 * 0.5 / 100.0 / 100.0).round() * 100.0).min(hi - 100.0).max(100.0);
        let normal = c.game_mode == omsi_launcher_lib::company::GameMode::Normal;
        let (rec, rec_chance) = cn::recommended(c, t);
        // (the slider's sum when it is this tender's and not above the bid that leads now;
        // else the recommended ask in Normal, the most that leads otherwise)
        let mine = args.get("amount_for").and_then(Value::as_u64) == Some(t.id as u64);
        let start = if normal { rec as f64 / 100.0 } else { hi };
        let want = args.get("amount").and_then(Value::as_f64).filter(|a| mine && *a <= hi).unwrap_or(start);
        let a = want.clamp(lo, hi);
        let amount = (a * 100.0).round() as i64;
        let chance = cn::chance(c, t, amount);
        let ctone = if chance >= 0.7 { "ok" } else if chance >= 0.35 { "warn" } else { "danger" };
        let mut said = trf("%{s} % of the reference  ·  chance to win, as it looks now: about %{p} %", &[("s", &format!("{:.0}", t.share(amount) * 100.0)), ("p", &format!("{:.0}", chance * 100.0))]);
        if !t.fee_paid && cn::fee(c, t) > 0 {
            said = format!("{said}  ·  {}", trf("taking part costs %{amount}", &[("amount", &eur(cn::fee(c, t)))]));
        }
        json!({
            "kind": "running", "id": t.id, "lo": lo, "hi": hi, "amount": a, "label": tr("Your ask"),
            "chance": said, "chance_tone": ctone,
            "recommend": normal.then(|| trf("Recommended: %{amount} (%{s} % of the reference), about %{p} % to win", &[("amount", &eur(rec)), ("s", &format!("{:.0}", t.share(rec) * 100.0)), ("p", &format!("{:.0}", rec_chance * 100.0))])),
            "recommend_amount": rec as f64 / 100.0,
            "bid": trf("Ask %{amount}", &[("amount", &eur(amount))]),
            "buy": trf("Buy outright, %{amount}", &[("amount", &eur(t.buy_out()))]), "buy_tip": tr("Certain: the auction ends at once in your favour"),
            "plus15": tr("+15 min"), "to_close": tr("To its close"),
        })
    } else if t.open() {
        json!({ "kind": "wait", "id": t.id, "label": tr("Simulate until it opens") })
    } else {
        match &t.outcome {
            Some(Outcome::Lost { winner, amount, .. } | Outcome::NoBid { winner, amount }) if !winner.is_empty() => json!({ "kind": "said", "text": trf(if t.scored() { "%{who} won the line with an ask of %{amount}." } else { "%{who} won the line for %{amount}." }, &[("who", winner), ("amount", &eur(*amount))]), "tone": "soft" }),
            Some(Outcome::Won { .. }) => {
                let text = match cn::of_line(c, &t.line) {
                    Some(h) if dates::between(&c.date, &h.from) > 0 => trf("The company runs the line from %{date}.", &[("date", &day_label(&h.from))]),
                    Some(h) => trf("The concession runs until %{date}, at %{p} % of the reference.", &[("date", &day_label(&h.until)), ("p", &format!("{:.0}", h.price * 100.0))]),
                    None => String::new(),
                };
                json!({ "kind": "said", "text": text, "tone": "ok" })
            }
            _ => json!({ "kind": "none" }),
        }
    };
    json!({
        "title": title, "id": t.id, "number": t.number, "caption": caption_of(t),
        "state": state, "tone": tone, "mine": mine.as_ref().map(|m| &m.0), "mine_tone": mine.map(|m| m.1),
        "figures": figures, "frac": frac, "times": times, "bidders": bidders,
        "log_title": tr("Bids").to_uppercase(), "log": log, "no_log": tr("No bid yet."),
        "actions": actions,
    })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let id = u32_arg(args, "id");
    Some(match what {
        "tender_pick" => {
            more(s).tenders.selected = Some(id);
            Ok(json!({}))
        }
        "tender_bid" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(0);
            // (the least bid in the company's currency: the rules' own words say euros)
            if amount < omsi_launcher_lib::company::auction::ROUND && s.company.is_some() {
                let least = trf("Ask at least %{amount}.", &[("amount", &eur(omsi_launcher_lib::company::auction::ROUND))]);
                return Some(Ok(json!({ "popup": super::super::kit::Popup::new("error", "This cannot be done", least, "", None).json(false) })));
            }
            match apply(s, |c| cn::bid(c, id, amount)) {
                Ok(_) => {
                    s.say(tr("Your bid is in. The rivals answer as the clock goes on."), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "tender_buy" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let number = s.company.as_ref().and_then(|c| c.concessions.tenders.iter().find(|t| t.id == id)).map(|t| t.number.clone()).unwrap_or_default();
            match apply(s, |c| cn::buy_out(c, id)) {
                Ok(_) => {
                    s.say(trf("Line %{n} is yours: plan its tours and start its service on the Planning page.", &[("n", &number)]), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // the clock stepped for the auction: +15 min, to its close, until it opens
        "tender_step" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
            let t = c.concessions.tenders.iter().find(|t| t.id == id);
            let to = match (args.get("to").and_then(Value::as_str).unwrap_or(""), t) {
                ("close", Some(t)) => t.closes_at,
                ("open", Some(t)) => t.opens_at,
                _ => ck::target(&c, Step::Minutes(15)),
            };
            simulate(s, to, false);
            Ok(json!({}))
        }
        _ => return None,
    })
}

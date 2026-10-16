//! The concession market (`crates/omsi-launcher-ui/src/company/concessions.rs`): the map's
//! lines out to tender with their week and the contract each lets, the concessions the company
//! holds with their bus contracts (kind, payment, rating), and the auction of the tender picked
//! as it runs with the clock - the authority's cap, the advice price, the rivals and their bids,
//! the least bid that leads now and its chance, buying the line outright - and how it ended,
//! every bid with its score. The rules are `company::concessions`' and `company::bus_contracts`'.
//!
//! The lines' weeks are read from the timetable of the company's week on a thread of their own
//! (the page says so meanwhile). The Contracts page's "Tender bids" (`bids`) lists the tenders
//! the company bid on.

use super::super::words::{day_label, eur, grouped, num, span_text, tr, trf};
use super::super::{act as apply, busy, may_step, simulate, Session};
use super::contracts as kc;
use super::{more, more_state, u32_arg};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::auction::{Placed, Who};
use omsi_launcher_lib::company::bus_contracts::{self as bc, Period};
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
        Some(Outcome::Won { amount, bought, .. }) => (trf(if *bought { "Bought for %{amount}" } else if t.scored() { "Won with an ask of %{amount} a year" } else { "Won for %{amount}" }, &[("amount", &eur(*amount))]), "ok"),
        Some(Outcome::Lost { winner, amount, .. }) => (trf(if t.scored() { "Lost to %{who}, who asked %{amount} a year" } else { "Lost to %{who} for %{amount}" }, &[("who", winner), ("amount", &eur(*amount))]), "warn"),
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
            // (Luc 2026-10-16: the tender's card - its contract, tours, km a week and cap as chips)
            let mut chips = Vec::new();
            if t.scored() {
                chips.push(json!({ "text": tr(t.kind.label()), "tone": "accent", "icon": "description" }));
            }
            match week {
                Some(wk) => {
                    chips.push(json!({ "text": trf("%{n} tours", &[("n", &wk.tours.to_string())]), "tone": "dim", "icon": "directions_bus" }));
                    chips.push(json!({ "text": if wk.km < 1.0 { trf("%{n} trips a week", &[("n", &wk.trips.to_string())]) } else { trf("%{km} km a week", &[("km", &grouped(wk.km))]) }, "tone": "dim", "icon": "route" }));
                }
                None => chips.push(json!({ "text": trf("%{n} tours", &[("n", &t.day_tours.to_string())]), "tone": "dim", "icon": "directions_bus" })),
            }
            if t.scored() {
                chips.push(json!({ "text": trf("cap %{amount}", &[("amount", &eur(t.reference))]), "tone": "early", "icon": "payments" }));
            }
            let countdown = if !t.open() {
                None
            } else if now < t.opens_at {
                Some(json!([trf("opens in %{span}", &[("span", &span_text(t.opens_at - now))]), "early"]))
            } else {
                Some(json!([trf("closes in %{span}", &[("span", &span_text(t.closes_at - now))]), if t.closes_at - now < 60 { "warn" } else { "ok" }]))
            };
            let figures = match week {
                Some(wk) if wk.km < 1.0 => trf("%{t} tours, %{n} trips a week", &[("t", &wk.tours.to_string()), ("n", &wk.trips.to_string())]),
                Some(wk) => trf("%{t} tours, %{km} km a week", &[("t", &wk.tours.to_string()), ("km", &format!("{:.0}", wk.km))]),
                None if reading => tr("Reading its week…"),
                None => trf("%{t} tours, %{km} km on the day it was offered", &[("t", &t.day_tours.to_string()), ("km", &format!("{:.0}", t.day_km))]),
            };
            // (the contract it lets, and the authority's cap)
            let figures = if t.scored() { format!("{}  ·  {figures}  ·  {}", tr(t.kind.label()), trf("cap %{amount} a year", &[("amount", &eur(t.reference))])) } else { figures };
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
                "chips": chips, "countdown": countdown,
            })
        })
        .collect();
    let open = list.iter().filter(|t| t.open()).count();
    json!({ "title": tr("Open tenders"), "count": open, "note": note, "empty": empty, "rows": rows })
}

/// The concessions the company holds and its own lines (`concessions::held`): each with its bus
/// contract (kind, payment, term, rating and points) - or commercial, without one.
fn held(c: &Company) -> Value {
    let today = c.date.clone();
    let rows: Vec<Value> = c
        .concessions
        .held
        .iter()
        .map(|h| {
            let left = dates::between(&today, &h.until);
            let until = if dates::between(&today, &h.from) > 0 { trf("from %{date}", &[("date", &day_label(&h.from))]) } else { trf("until %{date}", &[("date", &day_label(&h.until))]) };
            let k = bc::of_line(c, &h.line);
            let price = match k {
                Some(k) => format!("{}  ·  {}", tr(k.kind.label()), kc::pay_text(c, k)),
                None => tr("Commercial: no contract, the fares are the company's"),
            };
            let more = k.map(|k| format!("{}  ·  {}", kc::points_text(c, k), kc::unpaid_text(c, k)));
            let caption = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)).map(|l| l.caption.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| h.line.clone());
            json!({ "number": h.number, "caption": caption, "until": until, "tone": if left <= cn::RENEW_BEFORE { "warn" } else { "text" }, "price": price, "more": more, "points": k.map(|k| kc::points_text(c, k)), "unpaid": k.map(|k| kc::unpaid_text(c, k)), "rag": k.map(|k| kc::rag_json(k.rag(&today))), "direct": (h.direct && k.is_none()).then(|| tr("commercial").to_uppercase()) })
        })
        .collect();
    let lic = eur((cn::LICENCE as f64 * c.price_index).round() as i64);
    let own: Vec<Value> = c
        .lines
        .iter()
        .filter(|x| x.own)
        .map(|x| {
            let text = match bc::of_line(c, &x.name) {
                Some(k) => format!("{}  ·  {}", tr(k.kind.label()), kc::pay_text(c, k)),
                None => trf("Your own line: no concession, a licence of %{amount} a month", &[("amount", &lic)]),
            };
            let k = bc::of_line(c, &x.name);
            json!({ "number": x.number, "caption": x.caption, "text": text, "points": k.map(|k| kc::points_text(c, k)), "rag": k.map(|k| kc::rag_json(k.rag(&today))) })
        })
        .collect();
    let empty = (rows.is_empty() && own.is_empty()).then(|| tr("The company holds no concession yet. Win a tender, or apply for a line of the map on the Lines page."));
    json!({ "title": tr("Your contracts"), "sub": record_short(c), "rows": rows, "own": own, "empty": empty, "own_label": tr("own line") })
}

/// The track record in a few words: the default points a contract and the quality points they
/// give in the next tenders.
fn record_short(c: &Company) -> String {
    let pts = bc::scored_points(c);
    trf("Track record: %{pts} default points a contract  ·  %{q} of 30 quality points", &[("pts", &num(pts, 1)), ("q", &num(bc::quality_points(pts), 1))])
}

/// How a tender's contract is paid, in words.
fn per_text(p: Period) -> String {
    match p {
        Period::Day => tr("paid by the operating day"),
        Period::Week => tr("paid by the week"),
        Period::Month => tr("paid by the month"),
    }
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
    let advice = cn::advice(c, t);
    let figures = json!([
        { "label": tr("The cap").to_uppercase(), "value": eur(t.reference), "tone": "text", "tip": tr("The most the authority pays a year: an ask above it does not count.") },
        { "label": tr("Advice").to_uppercase(), "value": eur(advice), "tone": "ok", "tip": tr("The work's cost price at your own costs, with a 5 % margin: drivers' paid hours, fuel, the workshop, a bus and its fixed costs, less BSOG.") },
        if t.open() { json!({ "label": tr("Ask at most now").to_uppercase(), "value": eur(least), "tone": "text" }) } else { json!({ "label": tr("Term").to_uppercase(), "value": trf("%{n} years", &[("n", &t.term_years().to_string())]), "tone": "text" }) },
        { "label": tr("Buy outright").to_uppercase(), "value": eur(t.buy_out()), "tone": "accent2" },
    ]);
    let (_, ext) = bc::term(t.kind);
    let terms = t.scored().then(|| trf("%{kind}, %{per}  ·  %{years} years and up to %{ext} more  ·  asks are a price a year", &[("kind", &tr(t.kind.label())), ("per", &per_text(t.period)), ("years", &t.term_years().to_string()), ("ext", &ext.to_string())]));
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
    // (Luc 2026-10-16: the rivals as a table - their kind, quality and where their bid stands)
    let rivals: Vec<Value> = t
        .bidders
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = cn::rival(c, *id)?;
            let last = placed.iter().rev().find(|p| p.who == Who::Rival(i)).map(|p| p.amount);
            let leads = leader == Some(Who::Rival(i));
            let (bid, tone) = match last {
                _ if !t.open() && !t.scored() && last.is_none() => (tr("no bid"), "dim"),
                Some(a) if leads => (trf("%{amount}, leads", &[("amount", &eur(a))]), "warn"),
                Some(a) => (eur(a), "text"),
                None if now < t.opens_at => (tr("waits for the opening"), "dim"),
                None => (tr("no bid"), "dim"),
            };
            Some(json!({ "name": r.name, "kind": tr(r.character.label()), "quality": r.quality / 100.0, "quality_text": format!("{:.0}", r.quality), "bid": bid, "tone": tone, "leads": leads, "points": points_of(&r.name) }))
        })
        .collect();
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
        let mut said = trf("%{s} % of the cap  ·  chance to win, as it looks now: about %{p} %", &[("s", &format!("{:.0}", t.share(amount) * 100.0)), ("p", &format!("{:.0}", chance * 100.0))]);
        if !t.fee_paid && cn::fee(c, t) > 0 {
            said = format!("{said}  ·  {}", trf("taking part costs %{amount}", &[("amount", &eur(cn::fee(c, t)))]));
        }
        json!({
            "kind": "running", "id": t.id, "lo": lo, "hi": hi, "amount": a, "label": tr("Your ask"),
            "chance": said, "chance_tone": ctone,
            "recommend": normal.then(|| trf("Recommended: %{amount} (%{s} % of the cap), about %{p} % to win", &[("amount", &eur(rec)), ("s", &format!("{:.0}", t.share(rec) * 100.0)), ("p", &format!("{:.0}", rec_chance * 100.0))])),
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
                    Some(h) => match bc::of_line(c, &h.line) {
                        Some(k) => trf("The line runs until %{date} on its contract: %{amount} a year.", &[("date", &day_label(&h.until)), ("amount", &eur(k.year_now(c.contract_index).round() as i64))]),
                        None => trf("The concession runs until %{date}.", &[("date", &day_label(&h.until))]),
                    },
                    None => String::new(),
                };
                json!({ "kind": "said", "text": text, "tone": "ok" })
            }
            _ => json!({ "kind": "none" }),
        }
    };
    // the timeline: open, bidding, award
    let stage = if !t.open() { 2 } else if now < t.opens_at { 0 } else { 1 };
    let timeline = json!([
        { "label": tr("Open"), "sub": when(c, t.opens_at), "state": if stage == 0 { "now" } else { "done" } },
        { "label": tr("Bidding"), "sub": trf("until %{time}", &[("time", &when(c, t.closes_at))]), "state": match stage { 0 => "next", 1 => "now", _ => "done" } },
        { "label": tr("Award"), "sub": match &t.outcome { Some(Outcome::Won { .. }) => tr("you won"), Some(_) => tr("decided"), None => tr("at the close") }, "state": if stage == 2 { "now" } else { "next" } },
    ]);
    // the score the company's ask would get (the slider's while it runs, else its last bid)
    let mine_amount = match actions.get("kind").and_then(Value::as_str) {
        Some("running") => actions.get("amount").and_then(Value::as_f64).map(|a| (a * 100.0).round() as i64),
        _ => mine_last,
    };
    let score = t.scored().then(|| {
        let me = mine_amount.and_then(|a| {
            let mut with: Vec<Placed> = placed.iter().filter(|p| p.who != Who::Player).copied().collect();
            with.push(Placed { at: now.max(t.opens_at), who: Who::Player, amount: a });
            cn::standings(c, t, &with).into_iter().find(|s| s.name.is_empty())
        });
        let q = bc::quality_points(bc::scored_points(c));
        let (price, quality) = me.as_ref().map_or((0.0, q), |s| (s.price, s.quality));
        json!({
            "title": tr("How bids are scored"), "head": tr("70 % price + 30 % quality"),
            "price": price, "quality": quality, "has": me.is_some(),
            "price_label": trf("Price %{p} of 70", &[("p", &format!("{price:.1}"))]), "quality_label": trf("Quality %{q} of 30", &[("q", &format!("{quality:.1}"))]),
            "total": me.as_ref().map(|s| trf("Your expected score: %{t} of 100", &[("t", &format!("{:.1}", s.total))])).unwrap_or_else(|| tr("Set an ask to see your expected score.")),
            "lines": [
                tr("Price: the lowest ask gets all 70 points, every other ask lowest ÷ ask of them."),
                trf("Quality: your track record, 100 % less 2 % a default point - yours %{pts} points, %{q} of 30.", &[("pts", &num(bc::scored_points(c), 1)), ("q", &num(q, 1))]),
                tr("Asks above the cap do not count; on equal totals the price decides, then quality."),
            ],
        })
    });
    json!({
        "timeline": timeline, "score": score, "rivals": rivals,
        "rivals_title": tr("Rivals"), "rivals_heads": [tr("Bidder"), tr("Kind"), tr("Quality"), tr("Bid")],
        "title": title, "id": t.id, "number": t.number, "caption": caption_of(t),
        "state": state, "tone": tone, "mine": mine.as_ref().map(|m| &m.0), "mine_tone": mine.map(|m| m.1),
        "figures": figures, "terms": terms, "frac": frac, "times": times, "bidders": bidders,
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

//! The Reputation page (Luc 2026-10-15: "explain to the player how to increase the reputation
//! of the company or decrease the reputation, and what they can do about it"): the reputation
//! and its trend, what raised or lowered it lately (`company::reputation`'s log), what moves it
//! and by how much, what the player can do (each a way to its page); and the passengers'
//! complaints with their talk (`company::complaints`).

use super::words::{day_label, eur_cents, num, tr, trf};
use super::{act as apply, Session, CONTRACTS_TAB, DEPOT_TAB, RESEARCH_TAB, WORKSHOP_TAB};
use crate::Answer;
use omsi_launcher_lib::company::complaints::{self as cp, Choice, Complaint, Kind, State};
use omsi_launcher_lib::company::reputation::{self as rp, Cause};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// A change in points ("+0.15", "−0.50").
fn points(x: f64) -> String {
    if x >= 0.0 {
        format!("+{:.2}", x)
    } else {
        format!("−{:.2}", -x)
    }
}

fn tone(x: f64) -> &'static str {
    if x > 0.0 {
        "ok"
    } else if x < -0.4 {
        "danger"
    } else {
        "warn"
    }
}

fn cause_text(cause: Cause, count: u32) -> String {
    trf(cause.label(), &[("n", &count.to_string())])
}

pub fn choice_key(c: Choice) -> &'static str {
    match c {
        Choice::Apologise => "apologise",
        Choice::Refund => "refund",
        Choice::Voucher => "voucher",
        Choice::Promise => "promise",
        Choice::Dismiss => "dismiss",
    }
}

fn choice_of(key: &str) -> Option<Choice> {
    Choice::ALL.iter().copied().find(|c| choice_key(*c) == key)
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Late => "Late trip",
        Kind::Dropped => "Dropped trip",
        Kind::Breakdown => "Breakdown",
        Kind::Crowded => "Full bus",
        Kind::Dirty => "Dirty bus",
        Kind::Rude => "Rude driver",
        Kind::Fare => "Fare",
    }
}

fn severity(s: u8) -> (&'static str, &'static str) {
    match s {
        0 | 1 => ("minor", "soft"),
        2 => ("serious", "warn"),
        _ => ("severe", "danger"),
    }
}

/// How a complaint ended, in words and its tone.
fn state_text(k: &Complaint) -> (String, &'static str) {
    let p = points(k.reputation);
    match k.state {
        State::Open => (trf("Answer by %{date}", &[("date", &day_label(&k.deadline))]), "accent2"),
        State::Happy if k.echo => (trf("Happy, and thanked you on social media (%{p})", &[("p", &p)]), "ok"),
        State::Happy => (trf("Resolved: the passenger is happy (%{p})", &[("p", &p)]), "ok"),
        State::Neutral => (trf("Left it at that (%{p})", &[("p", &p)]), "soft"),
        State::Angry if k.echo => (trf("Angry, and wrote a bad review (%{p})", &[("p", &p)]), "danger"),
        State::Angry => (trf("Angry (%{p})", &[("p", &p)]), "danger"),
        State::Expired => (trf("Not answered in time (%{p})", &[("p", &p)]), "warn"),
        State::Auto => (trf("Answered by the company (%{p})", &[("p", &p)]), "soft"),
    }
}

fn words(text: &str, args: &[(String, String)]) -> String {
    // (a complaint's fare and refund are kept as "1.45 \u{20AC}": in the company's currency)
    let money = |v: &str| v.strip_suffix(" \u{20AC}").and_then(|e| e.parse::<f64>().ok()).map(|e| eur_cents(e * 100.0));
    let shown: Vec<(&str, String)> = args.iter().map(|(k, v)| (k.as_str(), money(v).unwrap_or_else(|| v.clone()))).collect();
    let a: Vec<(&str, &str)> = shown.iter().map(|(k, v)| (*k, v.as_str())).collect();
    trf(text, &a)
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let tab = args.get("tab").and_then(Value::as_str).unwrap_or("overview");
    let open = cp::open_count(&c);
    let tabs = json!([
        { "key": "overview", "label": tr("Reputation"), "badge": Value::Null },
        { "key": "complaints", "label": tr("Complaints"), "badge": (open > 0).then(|| open.to_string()) },
    ]);
    let body = if tab == "complaints" { complaints(&c, args) } else { overview(&c) };
    json!({ "tab": if tab == "complaints" { "complaints" } else { "overview" }, "tabs": tabs, "body": body })
}

fn overview(c: &Company) -> Value {
    let m = rp::mode(c.game_mode);
    let trend = rp::trend(c, 7);
    let (trend_text, trend_tone) = match trend {
        Some(t) if t.abs() < 0.05 => (tr("Steady over the last week"), "soft"),
        Some(t) => (trf("%{p} points over the last week", &[("p", &points(t))]), tone(t)),
        None => (tr("The trend shows once a week of days is closed"), "dim"),
    };
    let head = json!({
        "value": num(c.reputation.clamp(0.0, 100.0), 1), "under": tr("of 100"),
        "stars": co::contracts::rating(c),
        "trend": trend_text, "trend_tone": trend_tone,
        "caps": trf("In %{mode} a loss counts ×%{f}; a day costs at most %{d} points and a week %{w}. Below 50, decent days bring it back a little every day.", &[("mode", &tr(c.game_mode.label())), ("f", &format!("{:.1}", m.loss)), ("d", &format!("{:.1}", m.day_cap)), ("w", &format!("{:.0}", m.week_cap))]),
    });
    // what moved it, the last week by cause, and day by day
    let week: Vec<Value> = rp::lately(c, &c.date, 8).into_iter().map(|(cause, x, n)| json!({ "text": cause_text(cause, n), "change": points(x), "tone": tone(x) })).collect();
    let mut log: Vec<&rp::Entry> = c.reputation_log.entries.iter().filter(|e| e.change.abs() >= 0.01 || e.capped > 0.0).collect();
    log.sort_by(|a, b| b.date.cmp(&a.date).then(a.change.total_cmp(&b.change)));
    let log: Vec<Value> = log
        .into_iter()
        .take(40)
        .map(|e| {
            let mut text = cause_text(e.cause, e.count);
            if e.capped > 0.0 {
                text = format!("{text}  ·  {}", trf("%{p} kept off by the cap", &[("p", &format!("{:.2}", e.capped))]));
            }
            json!({ "date": day_label(&e.date), "text": text, "change": points(e.change), "tone": tone(e.change) })
        })
        .collect();
    let effects: Vec<Value> = [
        ("schedule", "A day on time (90 % or more, nothing dropped)", "+0.15"),
        ("event_busy", "A dropped trip", "−0.06 each, at most −1.2 a day"),
        ("timer_off", "A day under 75 % on time", "−0.15; under 60 %: −0.25"),
        ("car_crash", "A bus breaking down", "−0.05 each, at most −0.3 a day"),
        ("groups", "Full buses, passengers left behind", "at most −0.5 a day"),
        ("healing", "Accidents, complaints, passengers taken ill", "−0.25 an accident, at most −1 a day"),
        ("cleaning_services", "Clean or dirty buses, the depot yard", "about +0.1 to −0.2 a day"),
        ("sell", "Fares far above the association's", "a little every day"),
        ("wifi", "WLAN, the cleaning company, air conditioning, better stops", "small gains every day"),
        ("forum", "Passenger complaints", "answered well up to +0.4; badly, or not at all, up to −0.5"),
        ("verified", "A week or a month without a dropped trip", "+0.3 / +1"),
    ]
    .iter()
    .map(|(icon, text, size)| json!({ "icon": icon, "text": tr(text), "size": tr(size) }))
    .collect();
    let drives = tr("What it drives: how many ride (about ±10 %), the authority's payment per kilometre (±10 %), your quality in tenders (30 % of the score), what advertisers pay, and the contracts' rating. Below 40, only half of a further loss counts for these.");
    let tips: Vec<Value> = [
        ("schedule", "Add layover where trips run late, so the buses keep to time", "Planning", 5),
        ("person_add", "Hire spare drivers and keep a spare bus, so no trip is dropped", "Staff", 2),
        ("construction", "Have the buses serviced before they break down", "Workshop", WORKSHOP_TAB),
        ("local_car_wash", "Keep the buses clean: cleaners, a washing bay, or the cleaning company", "Depot", DEPOT_TAB),
        ("badge", "Send drivers on the customer-service course (half the complaints) and first aid", "Staff", 2),
        ("forum", "Answer the passengers' complaints before they run out", "Complaints", 999),
        ("sell", "Keep the fares near the association's", "Lines", 3),
        ("airport_shuttle", "Put bigger buses or more trips on full lines", "Planning", 5),
        ("wifi", "Research WLAN and air conditioning; sign the cleaning company", "Research", RESEARCH_TAB),
        ("description", "Contracts: internet for the buses, the cleaning company", "Contracts", CONTRACTS_TAB),
    ]
    .iter()
    .map(|(icon, text, page, tab)| json!({ "icon": icon, "text": tr(text), "go": tr(page), "tab": if *tab == 999 { Value::Null } else { json!(tab) }, "complaints": *tab == 999 }))
    .collect();
    json!({
        "head": head,
        "week_title": tr("Last week, by cause").to_uppercase(), "week": week, "week_none": tr("Nothing moved it yet: close a day."),
        "log_title": tr("Day by day").to_uppercase(), "log": log,
        "effects_title": tr("What moves it").to_uppercase(), "effects": effects, "drives": drives,
        "tips_title": tr("What you can do").to_uppercase(), "tips": tips,
    })
}

fn complaints(c: &Company, args: &Value) -> Value {
    let mut list: Vec<&Complaint> = c.complaints.list.iter().collect();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(b.id.cmp(&a.id)));
    let picked = args.get("id").and_then(Value::as_u64).map(|x| x as u32).filter(|id| list.iter().any(|k| k.id == *id)).or_else(|| list.first().map(|k| k.id));
    let rows: Vec<Value> = list
        .iter()
        .map(|k| {
            let (sev, sev_tone) = severity(k.severity);
            let (state, state_tone) = state_text(k);
            json!({
                "id": k.id, "on": picked == Some(k.id), "open": k.open(),
                "title": format!("{}  ·  {}", k.passenger, tr(kind_label(k.kind))),
                "sub": trf("Line %{line} at %{time}, %{date}", &[("line", &k.line), ("time", &format!("{:02}:{:02}", k.time / 60, k.time % 60)), ("date", &day_label(&k.date))]),
                "severity": tr(sev), "severity_tone": sev_tone, "state": state, "state_tone": state_tone,
            })
        })
        .collect();
    let talk = picked.and_then(|id| list.iter().find(|k| k.id == id)).map(|k| {
        let lines: Vec<Value> = k.talk.iter().map(|x| json!({ "mine": x.mine, "text": words(&x.text, &x.args) })).collect();
        let choices: Vec<Value> = cp::choices(k)
            .into_iter()
            .map(|ch| {
                let amount = match ch {
                    Choice::Refund => cp::refund(c, k),
                    Choice::Voucher => cp::day_pass(c),
                    _ => 0,
                };
                json!({ "key": choice_key(ch), "label": trf(cp::label(ch), &[("amount", &super::words::eur_cents(amount as f64))]), "danger": ch == Choice::Dismiss })
            })
            .collect();
        let (state, state_tone) = state_text(k);
        let (fix, fix_tab) = cp::promise(k);
        let promised = k.used.contains(&Choice::Promise);
        json!({
            "id": k.id, "title": k.passenger.clone(), "sub": tr(kind_label(k.kind)),
            "mood": k.mood / 100.0, "mood_label": tr("Mood").to_uppercase(),
            "talk": lines, "choices": choices, "state": state, "state_tone": state_tone, "open": k.open(),
            "fix": promised.then(|| json!({ "text": trf(fix, &[("line", &k.line)]), "go": tr("Do it now"), "tab": fix_tab })),
        })
    });
    json!({
        "title": tr("Complaints").to_uppercase(), "rows": rows, "talk": talk,
        "empty": tr("No complaints. Passengers complain when trips are dropped or late, buses break down, are full or dirty, a driver is rude, or a fare is too high."),
        "note": tr("Answer each before its deadline: apologise and explain, offer a refund or a day pass, promise a fix - or dismiss it, and the passenger gets angry."),
    })
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    match what {
        "complaint_reply" => {
            let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let Some(choice) = args.get("choice").and_then(Value::as_str).and_then(choice_of) else { return Some(Ok(json!({}))) };
            let r = apply(s, |c| {
                let date = c.date.clone();
                cp::reply(c, id, choice, &date).map(|_| ())
            });
            Some(Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({}))))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    #[test]
    fn the_page_explains_and_lists() {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Rep".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let mut s = Session { company: Some(c), ..Default::default() };
        let v = view(&mut s, &json!({}));
        assert_eq!(v["tab"], "overview");
        assert!(v["body"]["effects"].as_array().is_some_and(|a| a.len() >= 8));
        assert!(v["body"]["tips"].as_array().is_some_and(|a| a.len() >= 6));
        let v = view(&mut s, &json!({ "tab": "complaints" }));
        assert!(v["body"]["talk"].is_null() && v["body"]["empty"].is_string());
    }
}

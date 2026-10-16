//! The Reputation page (Luc 2026-10-15: "explain to the player how to increase the reputation
//! of the company or decrease the reputation, and what they can do about it"): the reputation
//! and its trend, what raised or lowered it lately (`company::reputation`'s log), what moves it
//! and by how much, what the player can do (each a way to its page); and the passengers'
//! complaints with their talk (`company::complaints`).

use super::words::{day_label, eur_cents, num, tr, trf};
use super::{act as apply, Session, DEPOT_TAB, RESEARCH_TAB, WORKSHOP_TAB};
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
    json!({ "tab": if tab == "complaints" { "complaints" } else { "overview" }, "tabs": tabs, "body": body, "title": tr("Reputation"), "lead": tr("How your passengers and the public see your company") })
}

/// The ISO week number of the week a Monday (a day number) begins.
fn iso_week(monday: i64) -> u32 {
    let thursday = monday + 3;
    let (y, _, _) = co::dates::civil_from_days(thursday);
    ((thursday - co::dates::days_from_civil(y, 1, 1)) / 7 + 1) as u32
}

/// The reputation of the last `n` weeks, the oldest first: each week's last close (the weekly
/// history, the days kept filling in a file of before it), this week's as it stands now.
fn weekly(c: &Company, n: usize) -> Vec<(i64, f64)> {
    let mut by: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    for (d, v) in &c.reputation_log.weeks {
        by.insert(co::dates::week_of(d), *v);
    }
    let mut days = c.reputation_log.history.clone();
    days.sort_by(|a, b| a.0.cmp(&b.0));
    for (d, v) in &days {
        by.insert(co::dates::week_of(d), *v);
    }
    by.insert(co::dates::week_of(&c.date), (c.reputation.clamp(0.0, 100.0) * 10.0).round() / 10.0);
    let all: Vec<(i64, f64)> = by.into_iter().collect();
    all[all.len().saturating_sub(n)..].to_vec()
}

fn overview(c: &Company) -> Value {
    let m = rp::mode(c.game_mode);
    let trend = rp::trend(c, 7);
    let (trend_text, trend_tone) = match trend {
        Some(t) if t.abs() < 0.05 => (tr("Steady over the last week"), "soft"),
        Some(t) => (trf("%{p} points over the last week", &[("p", &points(t))]), tone(t)),
        None => (tr("The trend shows once a week of days is closed"), "dim"),
    };
    // (Luc 2026-10-16: the gauge, and the eight weeks' trend under it)
    let weeks = weekly(c, 8);
    let first = weeks.first().map(|w| w.1).unwrap_or(c.reputation);
    let moved = c.reputation.clamp(0.0, 100.0) - first;
    let (say, say_tone, say_icon) = if weeks.len() < 2 {
        (tr("Your reputation's trend shows once a week is closed"), "dim", "schedule")
    } else if moved >= 3.0 {
        (tr("Your reputation is improving"), "ok", "trending_up")
    } else if moved >= 0.3 {
        (tr("Your reputation is slightly improving"), "ok", "trending_up")
    } else if moved > -0.3 {
        (tr("Your reputation is steady"), "soft", "sync_alt")
    } else if moved > -3.0 {
        (tr("Your reputation is slightly falling"), "warn", "trending_down")
    } else {
        (tr("Your reputation is falling"), "danger", "trending_down")
    };
    let span = (weeks.len().max(2) - 1).to_string();
    let by = if moved.abs() < 0.05 {
        trf("About the same as %{n} weeks ago", &[("n", &span)])
    } else if moved > 0.0 {
        trf("It's up by %{p} points compared to %{n} weeks ago", &[("p", &num(moved, 1)), ("n", &span)])
    } else {
        trf("It's down by %{p} points compared to %{n} weeks ago", &[("p", &num(-moved, 1)), ("n", &span)])
    };
    let head = json!({
        "title": tr("Overall reputation"), "info": tr("Your reputation runs from 0 to 100 and starts at 50. Good days raise it a little, bad days lower it - never by more than the day's and the week's cap."),
        "value": num(c.reputation.clamp(0.0, 100.0), 1), "frac": c.reputation.clamp(0.0, 100.0) / 100.0, "under": tr("/ 100"),
        "stars": co::contracts::rating(c),
        "say": say, "say_tone": say_tone, "say_icon": say_icon, "by": by,
        "trend": trend_text, "trend_tone": trend_tone,
        "chart_title": tr("Trend (last 8 weeks)"),
        "chart": weeks.iter().map(|(monday, v)| json!([trf("W%{n}", &[("n", &iso_week(*monday).to_string())]), v])).collect::<Vec<_>>(),
        "caps": trf("In %{mode} a loss counts ×%{f}; a day costs at most %{d} points and a week %{w}. Below 50, decent days bring it back a little every day.", &[("mode", &tr(c.game_mode.label())), ("f", &format!("{:.1}", m.loss)), ("d", &format!("{:.1}", m.day_cap)), ("w", &format!("{:.0}", m.week_cap))]),
    });
    // what moved it last week, by cause: punctuality, full buses, dropped trips, breakdowns, the rest
    let lately = rp::lately(c, &c.date, 7);
    let sum = |causes: &[Cause]| lately.iter().filter(|x| causes.contains(&x.0)).map(|x| x.1).sum::<f64>();
    let groups = [
        ("On time / punctuality", sum(&[Cause::Punctual, Cause::Late])),
        ("Full buses", sum(&[Cause::Crowded])),
        ("Dropped trips", sum(&[Cause::Dropped])),
        ("Breakdowns", sum(&[Cause::Breakdowns])),
        ("Other", lately.iter().filter(|x| ![Cause::Punctual, Cause::Late, Cause::Crowded, Cause::Dropped, Cause::Breakdowns].contains(&x.0)).map(|x| x.1).sum::<f64>()),
    ];
    let most = groups.iter().map(|g| g.1.abs()).fold(0.01, f64::max);
    let week: Vec<Value> = groups.iter().map(|(label, x)| json!({ "label": tr(label), "value": points(*x), "frac": x.abs() / most, "tone": if *x >= 0.0 { "ok" } else { "danger" }, "zero": x.abs() < 0.005 })).collect();
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
    // what moves it: the gains and the losses, as `close_day` and the day's other parts take them
    let factor = |(icon, name, text, value): &(&str, &str, &str, &str)| json!({ "icon": icon, "name": tr(name), "text": tr(text), "value": tr(value) });
    let positive: Vec<Value> = [
        ("schedule", "A day on time", "90 % or more on time, nothing dropped", "+0.15"),
        ("military_tech", "No dropped trip for a week or a month", "A bonus for the streak", "+0.3 / +1"),
        ("forum", "Complaints answered well", "An apology, a refund, a day pass or a fix", "up to +0.4"),
        ("water_drop", "Clean buses and a tidy depot yard", "Cleaners, a washing bay, the cleaning company", "up to +0.1/day"),
        ("wifi", "WLAN, air conditioning, better stops", "Small gains every day they run", "+0.01 to +0.06/day"),
        ("autorenew", "Decent days below 50", "It comes back towards 50 by itself", "back to 50"),
    ]
    .iter()
    .map(factor)
    .collect();
    let negative: Vec<Value> = [
        ("event", "A dropped trip", "At most −1.2 a day", "−0.06"),
        ("timer", "Late trips", "6 minutes or more late, beyond a quarter of the day's trips; half an hour late counts as dropped", "−0.01"),
        ("warning", "A bus breaking down", "At most −0.3 a day", "−0.05"),
        ("groups", "Full buses, passengers left behind", "Crowded trips and people left at the stop", "−0.5/day max"),
        ("emergency_home", "Accidents, complaints, passengers taken ill", "At most −1 a day", "−0.25"),
        ("confirmation_number", "Fares far above the association's", "The further above, the more every day", "a little a day"),
        ("water_drop", "Dirty buses, a messy depot yard", "Buses not cleaned, a yard left to itself", "down to −0.2/day"),
        ("forum", "Complaints answered badly or not at all", "The passenger gets angry", "down to −0.5"),
    ]
    .iter()
    .map(factor)
    .collect();
    let drives = tr("What it drives: how many ride (from about −9 % to +15 %), what advertisers pay, your rating in stars, an own weekend line's tourism grant and a little of what lenders allow. Your quality in tenders is not the reputation but your contracts' track record of default points. Below 40, only half of a further loss counts for the passengers and the grant.");
    // what the player can do: each a card with its way to the page
    let tips: Vec<Value> = [
        ("schedule", "Improve punctuality", "Add layover where trips run late and keep a spare bus, so the buses keep to time and no trip is dropped.", "Planning", 5),
        ("groups", "Support your staff", "Hire spare drivers, and send them on the customer-service course (half the complaints) and first aid.", "Staff", 2),
        ("construction", "Fix issues faster", "Have the buses serviced before they break down, and repair the broken ones quickly.", "Workshop", WORKSHOP_TAB),
        ("garage", "Optimise your depot", "Keep the buses and the yard clean: cleaners and a washing bay.", "Depot", DEPOT_TAB),
        ("hub", "Invest in research", "Research WLAN and air conditioning; then sign the internet and the cleaning company on the Contracts page.", "Research", RESEARCH_TAB),
        ("forum", "Answer the complaints", "Answer the passengers' complaints before they run out: a good answer wins reputation back.", "Complaints", 999),
        ("confirmation_number", "Keep the fares fair", "Fares far above the association's cost reputation every day; bigger buses or more trips help full lines.", "Lines", 3),
    ]
    .iter()
    .map(|(icon, title, text, page, tab)| json!({ "icon": icon, "title": tr(title), "text": tr(text), "page": tr(page), "go": trf("Go to %{page}", &[("page", &tr(page))]), "tab": if *tab == 999 { Value::Null } else { json!(tab) }, "complaints": *tab == 999 }))
    .collect();
    json!({
        "head": head,
        "week_title": tr("Last week by cause"), "week": week, "week_none": tr("Nothing moved it yet: close a day."), "week_any": !lately.is_empty(),
        "log_title": tr("Day by day"), "log": log,
        "effects_title": tr("What moves it"), "effects_info": tr("What raises and lowers the reputation, and by how much in points; the caps of the day and the week hold back a bad run."),
        "positive_title": tr("Positive factors"), "positive": positive,
        "negative_title": tr("Negative factors"), "negative": negative, "drives": drives,
        "tips_title": tr("What you can do"), "tips_sub": tr("Take action to improve your reputation"), "tips": tips,
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
        "title": tr("Complaints"), "rows": rows, "talk": talk,
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
        assert!(v["body"]["positive"].as_array().is_some_and(|a| a.len() >= 5));
        assert!(v["body"]["negative"].as_array().is_some_and(|a| a.len() >= 5));
        assert_eq!(v["body"]["week"].as_array().map(Vec::len), Some(5));
        assert!(v["body"]["head"]["chart"].as_array().is_some_and(|a| !a.is_empty()));
        // (the payment per kilometre is gone: the bus contracts pay by the day, week or month)
        assert!(!v["body"]["drives"].as_str().unwrap_or("").contains("kilometre"));
        assert!(v["body"]["tips"].as_array().is_some_and(|a| a.len() >= 6));
        let v = view(&mut s, &json!({ "tab": "complaints" }));
        assert!(v["body"]["talk"].is_null() && v["body"]["empty"].is_string());
    }

    /// Every text of the redesigned chooser, finances, reputation and career pages (Luc
    /// 2026-10-16, crates/omsihub-locales/locales/company-pages-a.yml) is named in every
    /// language: those of their sources, and the factors and actions of this page's tables.
    #[test]
    fn the_redesigned_pages_texts_are_translated() {
        let mut texts: Vec<String> = Vec::new();
        for src in [include_str!("chooser.rs"), include_str!("money.rs"), include_str!("reputation.rs"), include_str!("career.rs")] {
            let src = src.replace("\r\n", "\n");
            let src = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
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
        // (the tables' texts, as the page in English shows them)
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Rep".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let mut s = Session { company: Some(c), ..Default::default() };
        let body = view(&mut s, &json!({}))["body"].clone();
        for (list, keys) in [("positive", &["name", "text", "value"][..]), ("negative", &["name", "text", "value"]), ("tips", &["title", "text", "page"]), ("week", &["label"])] {
            for x in body[list].as_array().unwrap() {
                texts.extend(keys.iter().filter_map(|k| x[*k].as_str()).filter(|t| t.chars().filter(|c| c.is_alphabetic()).count() >= 2).map(str::to_string));
            }
        }
        texts.sort();
        texts.dedup();
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}

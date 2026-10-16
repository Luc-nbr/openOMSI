//! The company's Agenda (Beta 57, Luc: "voeg een agenda toe met alle gebeurtenissen die er gaan
//! komen"): `co::agenda`'s entries as a list grouped by day and as a month's calendar - each day
//! with small marks per kind, a day picked shows its entries -, filtered by kind and by what
//! needs the player; every entry a click from the page that handles it (`go`). The start
//! screen's "Coming up" tile and Casual's list on its home are the next few of them.

use super::words::{eur, grouped, month_label, num, tr, trf, MONTHS, WEEKDAYS};
use super::{casual, Session, CONCESSIONS_TAB, CONTRACTS_TAB, DEPOT_TAB, INBOX_TAB, RESEARCH_TAB, WORKSHOP_TAB};
use omsi_launcher_lib::company::agenda::{self as ag, Calendar, Entry, Go, Group, Known, Severity};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A place's value as the agenda writes it, by its name.
fn value(name: &str, v: &str) -> String {
    match name {
        "amount" => v.parse::<i64>().map(eur).unwrap_or_else(|_| v.to_string()),
        "date" | "from" | "until" => short_day(v),
        "count" | "km" => v.parse::<f64>().map(grouped).unwrap_or_else(|_| v.to_string()),
        "stars" => v.parse::<f64>().map(|x| num(x, 1)).unwrap_or_else(|_| v.to_string()),
        "what" | "job" | "role" | "area" => tr(v),
        _ => v.to_string(),
    }
}

/// An entry's English words translated, its places filled.
fn words(text: &str, args: &[(String, String)]) -> String {
    let mut t = tr(text);
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), &value(k, v));
    }
    t
}

/// "Mon 26 Oct".
fn short_day(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (_, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {}", tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, tr(MONTHS[(m as usize).clamp(1, 12) - 1]))
}

/// A day as the list heads it: today, tomorrow, yesterday, else "Mon 26 Oct".
fn day_head(c: &Company, date: &str) -> String {
    match co::dates::between(&c.date, date) {
        0 => tr("Today"),
        1 => tr("Tomorrow"),
        -1 => tr("Yesterday"),
        _ => short_day(date),
    }
}

/// A severity's tone.
fn tone(s: Severity) -> &'static str {
    match s {
        Severity::Info => "accent2",
        Severity::Warn => "warn",
        Severity::Bad => "danger",
    }
}

/// Where a click on an entry goes, as the page carries it out (`agenda.js` `follow`): a page
/// (`tab`), what it shows there (`ui`, as each page keeps it), a dialog, or RosterFlow's day
/// (`rf`). In Casual its own pages; null where Casual has none.
fn go_json(c: &Company, go: &Go, casual: bool) -> Value {
    if casual {
        let tab = match go {
            Go::Bus(id) => return json!({ "dialog": { "kind": "vehicle", "id": id } }),
            Go::Person(_) | Go::Holidays(_) | Go::Agencies(_) => casual::page_for(2),
            Go::Line(_) => casual::page_for(3),
            Go::Finances => casual::page_for(4),
            Go::Workshop | Go::Depot => casual::page_for(DEPOT_TAB),
            Go::Dealer => casual::SHOP,
            Go::Planning(d) if *d == c.date => casual::SHIFTS,
            _ => return Value::Null,
        };
        return json!({ "tab": tab });
    }
    match go {
        Go::Person(id) => json!({ "tab": 2, "ui": { "people": { "tab": 0, "open": id } } }),
        Go::Holidays(d) => json!({ "tab": 2, "ui": { "people": { "tab": 4, "open": null, "hday": d } } }),
        Go::Courses => json!({ "tab": 6, "ui": { "career": { "part": 2 } } }),
        Go::Agencies(d) => json!({ "tab": 2, "ui": { "people": { "tab": 5, "open": null, "aday": d } } }),
        Go::Bus(id) => json!({ "dialog": { "kind": "vehicle", "id": id } }),
        Go::Workshop => json!({ "tab": WORKSHOP_TAB }),
        Go::Depot => json!({ "tab": DEPOT_TAB }),
        Go::Dealer => json!({ "tab": 1, "ui": { "fleet": { "tab": 1 } } }),
        Go::Adverts => json!({ "tab": 1, "ui": { "fleet": { "tab": 2 } } }),
        Go::Line(name) => json!({ "tab": 3, "ui": { "lines": { "selected": name } } }),
        Go::Planning(d) => json!({ "tab": 5, "rf": d }),
        Go::Concessions => json!({ "tab": CONCESSIONS_TAB }),
        Go::Contracts(part) => json!({ "tab": CONTRACTS_TAB, "ui": { "contracts": { "part": part } } }),
        Go::Finances => json!({ "tab": 4 }),
        Go::Research => json!({ "tab": RESEARCH_TAB }),
        Go::Inbox => json!({ "tab": INBOX_TAB }),
    }
}

/// An entry as the page draws it.
fn item(c: &Company, e: &Entry, casual: bool) -> Value {
    let icon = match e.value("event") {
        Some(key) if e.kind == ag::Kind::WorldEvent => super::inbox::event_icon(key),
        _ => e.kind.icon(),
    };
    json!({
        "date": e.date, "icon": icon, "group": e.group().key(), "tone": tone(e.severity),
        "text": words(e.text, &e.args), "note": e.note.map(|n| words(n, &e.args)),
        "time": e.minute.map(|m| format!("{:02}:{:02}", m / 60, m % 60)),
        "needs": e.needs_me, "go": go_json(c, &e.go, casual),
    })
}

/// The map's public and school holidays for some days (`Calendar::of_map`), read again at most
/// once a minute: its `Holidays.txt` and its line registry change seldom.
fn calendar(c: &Company, from: &str, until: &str) -> Calendar {
    type Kept = HashMap<(String, String, String), (Instant, Calendar)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let key = (c.map.clone(), from.to_string(), until.to_string());
    let mut kept = KEPT.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, cal)) = kept.get(&key) {
        if at.elapsed().as_secs() < 60 {
            return cal.clone();
        }
    }
    let cal = Calendar::of_map(&c.map, from, until);
    if kept.len() > 24 {
        kept.clear();
    }
    kept.insert(key, (Instant::now(), cal.clone()));
    cal
}

/// The company's entries from `from` to `until`, with today's and tomorrow's plans and the map's
/// calendar.
fn entries(s: &mut Session, from: &str, until: &str) -> Vec<Entry> {
    let Some(c) = s.company.clone() else { return Vec::new() };
    let today = s.plan.clone();
    let tomorrow = super::tomorrow(s);
    let cal = calendar(&c, from, until);
    ag::agenda(&c, from, until, &Known { today: today.as_ref(), tomorrow: tomorrow.as_ref(), calendar: Some(&cal) })
}

/// The next `n` entries from today on, as the page draws them, and how many of the coming week
/// need the player.
pub fn coming(s: &mut Session, n: usize, casual: bool) -> (Vec<Value>, usize) {
    let Some(c) = s.company.clone() else { return (Vec::new(), 0) };
    let all = entries(s, &c.date, &co::dates::add(&c.date, ag::AHEAD));
    let week = co::dates::add(&c.date, 6);
    let needs = all.iter().filter(|e| e.needs_me && e.date <= week).count();
    let list = all
        .iter()
        .take(n)
        .map(|e| {
            let mut v = item(&c, e, casual);
            let when = match &e.minute {
                Some(_) if e.date == c.date => v["time"].as_str().unwrap_or("").to_string(),
                _ => day_head(&c, &e.date),
            };
            v["when"] = json!(when);
            v
        })
        .collect();
    (list, needs)
}

//// RosterFlow's day switcher (Beta 57): each day of its week (`marks`: the day's own marks,
/// `rosterflow::days::marks`) with a mark of how many entries the agenda has then, in the
/// warning's tone when one of them needs the player.
pub fn rf_marks(c: &Company, mut marks: Vec<Value>) -> Vec<Value> {
    let (from, until) = (c.date.clone(), co::dates::add(&c.date, 6));
    let cal = calendar(c, &from, &until);
    let all = ag::agenda(c, &from, &until, &Known { calendar: Some(&cal), ..Default::default() });
    for (k, m) in marks.iter_mut().enumerate() {
        let date = co::dates::add(&c.date, k as i64);
        let day: Vec<&Entry> = all.iter().filter(|e| e.date == date).collect();
        let Some(tags) = m.as_array_mut().filter(|_| !day.is_empty()) else { continue };
        let tone = if day.iter().any(|e| e.needs_me) { "warn" } else { "dim" };
        tags.push(json!({ "text": trf("%{n} in the agenda", &[("n", &day.len().to_string())]), "tone": tone, "kind": "agenda" }));
    }
    marks
}

/// The entries of the coming week that need the player: the first `n` as the page draws them,
/// and how many there are (the dashboard's "Needs you", the bell over the pages).
pub fn needing(s: &mut Session, n: usize) -> (Vec<Value>, usize) {
    let Some(c) = s.company.clone() else { return (Vec::new(), 0) };
    let casual = c.game_mode.casual();
    let all: Vec<Entry> = entries(s, &c.date, &co::dates::add(&c.date, 6)).into_iter().filter(|e| e.needs_me).collect();
    let list = all
        .iter()
        .take(n)
        .map(|e| {
            let mut v = item(&c, e, casual);
            let when = match &e.minute {
                Some(_) if e.date == c.date => v["time"].as_str().unwrap_or("").to_string(),
                _ => day_head(&c, &e.date),
            };
            v["when"] = json!(when);
            v
        })
        .collect();
    s.needs_kept = Some((s.generation, Instant::now(), all.len()));
    (list, all.len())
}

/// How many entries of the coming week need the player (the bell's badge, at every tick): worked
/// out again once the company changed, at most every five seconds - and not while the clock
/// simulates on its thread.
pub fn needs_count(s: &mut Session) -> usize {
    if let Some((generation, at, n)) = s.needs_kept {
        if generation == s.generation || at.elapsed().as_secs() < 5 || s.closing {
            return n;
        }
    }
    if s.closing {
        return 0;
    }
    needing(s, 0).1
}

/// How many need the player, in words.
pub fn needs_text(n: usize) -> String {
    match n {
        0 => tr("Nothing needs you"),
        1 => tr("1 needs you"),
        n => trf("%{n} need you", &[("n", &n.to_string())]),
    }
}

/// The Agenda page (`ui`: `mode` "list" or "month", `groups` the kinds shown, `mine` only what
/// needs the player, `month` the month shown and `day` the day picked in it).
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let month_mode = ui.get("mode").and_then(Value::as_str) == Some("month");
    let groups: Vec<Group> = match ui.get("groups").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_str).filter_map(Group::from_key).collect(),
        None => Group::ALL.to_vec(),
    };
    let mine = ui.get("mine").and_then(Value::as_bool).unwrap_or(false);
    // the days: the list's window, or the month's weeks (Monday to Sunday)
    let this_month = co::dates::month_of(&c.date);
    let month = ui.get("month").and_then(Value::as_str).filter(|m| co::dates::parse(&format!("{m}-01")).is_some()).unwrap_or(&this_month).to_string();
    let first = co::dates::parse(&format!("{month}-01")).unwrap_or(0);
    let (y, m, _) = co::dates::civil_from_days(first);
    let last = first + co::dates::days_in_month(y, m) as i64 - 1;
    let grid = (first - co::dates::weekday(first) as i64, last + 6 - co::dates::weekday(last) as i64);
    let (from, until) = if month_mode { (co::dates::fmt(grid.0), co::dates::fmt(grid.1)) } else { ag::window(&c) };
    let all = entries(s, &from, &until);
    let fits = |e: &&Entry| !mine || e.needs_me;
    let shown: Vec<&Entry> = all.iter().filter(fits).filter(|e| groups.contains(&e.group())).collect();
    let chips: Vec<Value> = Group::ALL
        .iter()
        .map(|g| json!({ "key": g.key(), "label": tr(g.label()), "icon": g.icon(), "on": groups.contains(g), "n": all.iter().filter(fits).filter(|e| e.group() == *g).count() }))
        .collect();
    let needs = all.iter().filter(|e| e.needs_me && groups.contains(&e.group())).count();
    let items = |date: &str| shown.iter().filter(|e| e.date == date).map(|e| item(&c, e, false)).collect::<Vec<_>>();
    let mut out = json!({
        "title": tr("Agenda"), "sub": tr("Every holiday, breakdown, repair, delivery and deadline the company knows of"),
        "modes": [tr("List"), tr("Month")], "mode": if month_mode { 1 } else { 0 },
        "groups": chips,
        "mine": { "label": tr("Only what needs me"), "on": mine, "n": needs, "tip": tr("Requests to answer and what has no cover"), "text": needs_text(needs) },
        "today": c.date,
    });
    if month_mode {
        let pick = ui.get("day").and_then(Value::as_str).and_then(co::dates::parse).filter(|d| (grid.0..=grid.1).contains(d));
        let today = co::dates::parse(&c.date).unwrap_or(0);
        let pick = pick.unwrap_or(if (first..=last).contains(&today) { today } else { first });
        let weeks: Vec<Vec<Value>> = (grid.0..=grid.1)
            .collect::<Vec<i64>>()
            .chunks(7)
            .map(|week| {
                week.iter()
                    .map(|&d| {
                        let date = co::dates::fmt(d);
                        let of_day: Vec<&&Entry> = shown.iter().filter(|e| e.date == date).collect();
                        // (a mark per kind: how many, in the tone of the worst)
                        let marks: Vec<Value> = Group::ALL
                            .iter()
                            .filter_map(|g| {
                                let of: Vec<&&&Entry> = of_day.iter().filter(|e| e.group() == *g).collect();
                                let worst = of.iter().map(|e| e.severity).max()?;
                                Some(json!({ "group": g.key(), "n": of.len(), "tone": tone(worst) }))
                            })
                            .collect();
                        json!({
                            "date": date, "n": co::dates::civil_from_days(d).2, "in": (first..=last).contains(&d),
                            "today": d == today, "past": d < today, "sel": d == pick,
                            "marks": marks, "count": of_day.len(), "needs": of_day.iter().any(|e| e.needs_me),
                        })
                    })
                    .collect()
            })
            .collect();
        let step = |k: i64| {
            let n = y as i64 * 12 + (m as i64 - 1) + k;
            format!("{:04}-{:02}", n.div_euclid(12), n.rem_euclid(12) + 1)
        };
        let picked = co::dates::fmt(pick);
        // ("Today · Mon 19 Oct 2026", else the day alone)
        let head = day_head(&c, &picked);
        let label = if head == short_day(&picked) { super::words::day_label(&picked) } else { format!("{head}  ·  {}", super::words::day_label(&picked)) };
        out["month"] = json!({
            "key": month, "label": month_label(&month), "prev": step(-1), "next": step(1), "prev_label": month_label(&step(-1)), "next_label": month_label(&step(1)), "this": month == this_month, "this_label": tr("This month"), "this_key": this_month,
            "weekdays": WEEKDAYS.iter().map(|w| tr(w)).collect::<Vec<_>>(),
            "weeks": weeks,
            "day": { "date": picked, "label": label, "items": items(&picked), "empty": tr("Nothing on this day") },
        });
    } else {
        let mut days: Vec<String> = shown.iter().map(|e| e.date.clone()).collect();
        days.dedup();
        let days: Vec<Value> = days
            .iter()
            .map(|d| {
                let n = co::dates::between(&c.date, d);
                // (today: what service control has for the player, a click from the Operations page)
                let waiting = if n == 0 { co::control::waiting(&c) } else { 0 };
                let ops = (waiting > 0).then(|| json!({ "tab": super::OPS_TAB, "label": super::words::count_text(waiting, "1 thing for service control", "%{n} things for service control") }));
                json!({ "date": d, "label": day_head(&c, d), "sub": super::words::day_label(d), "today": n == 0, "past": n < 0, "items": items(d), "ops": ops })
            })
            .collect();
        out["list"] = json!({ "days": days, "empty": tr("Nothing in these days"), "range": format!("{} – {}", short_day(&from), short_day(&until)), "count": trf("%{n} entries", &[("n", &shown.len().to_string())]) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    /// Every text of the agenda - the core's and the page's - in the six languages.
    #[test]
    fn every_agenda_text_is_translated() {
        let mut texts: Vec<String> = ag::texts().into_iter().map(String::from).collect();
        // (this file's words, and the page's own: `agenda.js`)
        let js = include_str!("../../../../../launcher/ui/js/omsihub/company/agenda.js");
        for (src, lead, end) in [(include_str!("agenda.rs"), "tr(\"", '"'), (include_str!("agenda.rs"), "trf(\"", '"'), (js, "tr('", '\'')] {
            let mut rest = src;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let t: String = after.chars().take_while(|c| *c != end).collect();
                if t.chars().any(char::is_alphabetic) {
                    texts.push(t);
                }
            }
        }
        assert!(texts.len() > 100, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-agenda.yml):\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn the_page_lists_the_days_and_draws_the_month() {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Agenda".into(), game_mode: GameMode::Normal, date: "2026-10-15".into(), ..Default::default() }, "Luc");
        let mut s = Session { company: Some(c), ..Default::default() };
        let v = view(&mut s, &json!({}));
        assert_eq!(v["mode"], 0);
        assert_eq!(v["groups"].as_array().map(Vec::len), Some(4));
        // (a company of no lines and no buses still has the month's close)
        let days = v["list"]["days"].as_array().unwrap();
        assert!(days.iter().any(|d| d["date"] == "2026-10-31"));
        let m = view(&mut s, &json!({ "mode": "month", "month": "2026-10" }));
        let weeks = m["month"]["weeks"].as_array().unwrap();
        // (October 2026 begins on a Thursday: five weeks from Monday the 28th of September)
        assert_eq!(weeks.len(), 5);
        assert_eq!(weeks[0][0]["date"], "2026-09-28");
        assert_eq!(m["month"]["day"]["date"], "2026-10-15");
        assert_eq!((m["month"]["prev"].as_str(), m["month"]["next"].as_str()), (Some("2026-09"), Some("2026-11")));
        // (the money's kind filtered out: the month's close goes)
        let v = view(&mut s, &json!({ "groups": ["staff", "fleet", "lines"] }));
        assert!(v["list"]["days"].as_array().unwrap().iter().all(|d| d["items"].as_array().unwrap().iter().all(|i| i["group"] != "money")));
        assert_eq!(go_json(s.company.as_ref().unwrap(), &Go::Bus(3), true)["dialog"]["id"], 3);
    }
}

//! The company's inbox (`crates/omsi-launcher-ui/src/company/inbox.rs`): what happens to the
//! company and around it as cards, the newest first, filtered by kind; a card opened beside the
//! list says all it has; under the list the trophies' bar to the next milestone.

use super::words::{day_label, eur, eur_cents, grouped, month_label, num, tr, trf, MONTHS, WEEKDAYS};
use super::{casual, pages_of, Session, CONTRACTS_TAB, RESEARCH_TAB, WORKSHOP_TAB};
use omsi_launcher_lib::company::clock::Level;
use omsi_launcher_lib::company::contracts::{self as ct, Goal, State, Unit};
use omsi_launcher_lib::company::inbox::{Event, Kind, Message};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// A place's value as the page writes it, by its name.
fn value(name: &str, v: &str) -> String {
    match name {
        "amount" | "reward" | "cost" | "charge" => v.parse::<i64>().map(eur).unwrap_or_else(|_| v.to_string()),
        "price" | "price_after" => v.parse::<f64>().map(eur_cents).unwrap_or_else(|_| v.to_string()),
        "date" | "from" | "until" => day_label(v),
        "month" => month_label(v),
        "count" => v.parse::<f64>().map(grouped).unwrap_or_else(|_| v.to_string()),
        "stars" => v.parse::<f64>().map(|x| num(x, 1)).unwrap_or_else(|_| v.to_string()),
        "what" | "why" | "area" | "job" | "role" => tr(v),
        _ => v.to_string(),
    }
}

/// An English text translated, its `%{name}` places filled as the page writes them.
pub fn words(text: &str, args: &[(String, String)]) -> String {
    let mut t = tr(text);
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), &value(k, v));
    }
    t
}

/// "Fri 8 Mar".
fn short_day(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (_, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {}", tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, tr(MONTHS[(m as usize).clamp(1, 12) - 1]))
}

fn days_of(c: &Company, from: &str, until: &str) -> (f64, String) {
    let days = (co::dates::between(from, until) + 1).max(1) as f64;
    let into = co::dates::between(from, &c.date) as f64 + c.clock.minute as f64 / 1_440.0;
    let label = if from == until { short_day(from) } else { format!("{} – {}", short_day(from), short_day(until)) };
    ((into / days).clamp(0.0, 1.0), label)
}

fn icon_of(c: &Company, m: &Message) -> &'static str {
    if let Some(e) = m.event.and_then(|id| c.inbox.event(id)) {
        return event_icon(&e.key);
    }
    kind_icon(m.kind)
}

pub fn kind_icon(k: Kind) -> &'static str {
    match k {
        Kind::Notice => "info",
        Kind::Event => "event",
        Kind::Staff => "groups",
        Kind::Fleet => "directions_bus",
        Kind::Workshop => "construction",
        Kind::Research => "hub",
        Kind::Contract => "description",
        Kind::Goal => "emoji_events",
        Kind::Finance => "payments",
        Kind::Lines => "route",
    }
}

fn event_icon(key: &str) -> &'static str {
    match key {
        "fuel_shortage" | "fuel_drop" => "water_drop",
        "environment_days" => "air",
        "fair" => "public",
        "football" => "sports_score",
        "christmas_market" => "stars",
        "rail_strike" => "commute",
        "town_festival" => "campaign",
        "concert" => "headphones",
        "marathon" => "directions_walk",
        "tourists" => "photo_camera",
        "school_holidays" => "today",
        "heat_wave" => "wb_sunny",
        "storm" => "thunderstorm",
        "snow" => "weather_snowy",
        "roadworks" => "construction",
        "diversion" => "alt_route",
        _ => "event",
    }
}

/// A level's tone (the ordinary run of things in the accent's).
fn tone_of(level: Level) -> &'static str {
    match level {
        Level::Minor | Level::Info => "accent2",
        other => super::feed_tone(other),
    }
}

/// A goal's progress in words (`contracts::goal_progress`).
pub fn goal_progress(g: &Goal) -> String {
    let (p, t) = (g.progress, g.aim.target());
    match g.aim.unit() {
        Unit::Count => format!("{} / {}", grouped(p), grouped(t)),
        Unit::Km => format!("{} / {} km", grouped(p), grouped(t)),
        Unit::Stars => trf("%{a} / %{b} stars", &[("a", &num(p, 1)), ("b", &num(t, 1))]),
        Unit::Days => trf("%{a} / %{b} days", &[("a", &num(p, 0)), ("b", &num(t, 0))]),
    }
}

/// What a message shows as its bar: how far, its tone, its words.
fn bar_of(c: &Company, m: &Message) -> Option<Value> {
    if let Some(g) = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id)) {
        let tone = match g.state {
            State::Done => "ok",
            State::Failed => "faint",
            _ => "accent2",
        };
        return Some(json!({ "frac": g.share(), "tone": tone, "label": goal_progress(g) }));
    }
    if let (Some(from), Some(until)) = (m.from.as_deref(), m.until.as_deref()) {
        let (frac, label) = days_of(c, from, until);
        return Some(json!({ "frac": frac, "tone": "accent2", "label": label }));
    }
    m.progress.map(|(done, of)| json!({ "frac": (done / of.max(1e-9)).clamp(0.0, 1.0), "tone": "accent2", "label": format!("{} / {}", grouped(done), grouped(of)) }))
}

/// The trophies' bar: what the goals earned against the next milestone (`contracts::trophy_bar`).
pub fn trophy_bar(c: &Company) -> Value {
    let (last, next) = ct::milestone(c);
    let earned = c.contracts.earned;
    let frac = if next > last { (earned - last) as f64 / (next - last) as f64 } else { 1.0 };
    let bonus = ct::milestone_bonus(c, next);
    json!({
        "frac": frac, "label": format!("{} / {}", grouped(earned as f64), grouped(next as f64)),
        "tip": trf("The trophies your goals earned in all. At %{count} the company reaches its next milestone: a bonus of %{amount}.", &[("count", &grouped(next as f64)), ("amount", &eur(bonus))]),
    })
}

/// Where a message is mended in the company's mode: the page and the button's words.
fn page_for(m: &Message, mode: co::GameMode) -> Option<(usize, &'static str, &'static str)> {
    let page = page_of(m);
    if !mode.casual() {
        return page;
    }
    let page = match m.kind {
        Kind::Workshop => Some((1, "To the fleet", "directions_bus")),
        _ => page,
    };
    page.map(|p| (casual::page_for(p.0), p.1, p.2)).filter(|p| pages_of(mode).contains(&p.0))
}

fn page_of(m: &Message) -> Option<(usize, &'static str, &'static str)> {
    match m.kind {
        Kind::Staff => Some((2, "To the staff", "groups")),
        Kind::Fleet => Some((1, "To the fleet", "directions_bus")),
        Kind::Workshop => Some((WORKSHOP_TAB, "To the workshop", "construction")),
        Kind::Research => Some((RESEARCH_TAB, "To the research", "hub")),
        Kind::Contract | Kind::Goal => Some((CONTRACTS_TAB, "To the contracts", "description")),
        Kind::Finance => Some((4, "Show the finances", "payments")),
        Kind::Lines => Some((3, "To the lines", "route")),
        Kind::Event => Some((5, "Open the planning", "event")),
        Kind::Notice => None,
    }
}

fn effects(e: &Event) -> Vec<Value> {
    let pct = |f: f64| format!("{:+.0} %", (f - 1.0) * 100.0);
    let mut out = Vec::new();
    if (e.fuel - 1.0).abs() > 0.001 {
        out.push(json!([trf("Diesel price: %{pct}", &[("pct", &pct(e.fuel))]), if e.fuel > 1.0 { "warn" } else { "ok" }]));
    }
    if (e.demand - 1.0).abs() > 0.001 {
        let t = if e.line.is_some() { "Passengers on the line: %{pct}" } else { "Passengers: %{pct}" };
        out.push(json!([trf(t, &[("pct", &pct(e.demand))]), if e.demand > 1.0 { "ok" } else { "warn" }]));
    }
    if e.late > 0.0 {
        let t = if e.line.is_some() { "Trips on the line late more often: %{pct}" } else { "Trips late more often: %{pct}" };
        out.push(json!([trf(t, &[("pct", &format!("+{:.0} %", e.late * 100.0))]), "warn"]));
    }
    out
}

/// The days `from` to `until` as cells (two weeks at most): gone, today, to come.
fn day_strip(c: &Company, from: &str, until: &str) -> Value {
    let Some(a) = co::dates::parse(from) else { return Value::Null };
    let n = (co::dates::between(from, until) + 1).clamp(1, 14);
    let today = co::dates::parse(&c.date).unwrap_or(a);
    let cells: Vec<Value> = (0..n)
        .map(|k| {
            let d = a + k;
            let (_, _, day) = co::dates::civil_from_days(d);
            json!({ "wd": WEEKDAYS[co::dates::weekday(d) as usize], "day": day, "state": if d < today { "gone" } else if d == today { "today" } else { "coming" } })
        })
        .collect();
    json!(cells)
}

/// The message open, beside the list.
fn details(c: &Company, m: &Message) -> Value {
    let mut out = json!({
        "id": m.id, "kind": tr(m.kind.label()), "tone": tone_of(m.level), "date": day_label(&m.date),
        "title": words(&m.title, &m.args), "text": words(&m.text, &m.args), "read": m.read,
    });
    if let Some(e) = m.event.and_then(|id| c.inbox.event(id)).cloned() {
        let status = if c.date.as_str() < e.from.as_str() {
            trf("Starts in %{n} days", &[("n", &co::dates::between(&c.date, &e.from).to_string())])
        } else if c.date.as_str() <= e.until.as_str() {
            trf("Running: day %{a} of %{b}", &[("a", &(co::dates::between(&e.from, &c.date) + 1).to_string()), ("b", &e.days().to_string())])
        } else {
            tr("Over")
        };
        out["event"] = json!({ "title": tr("What it does").to_uppercase(), "effects": effects(&e), "status": status, "days": day_strip(c, &e.from, &e.until) });
    } else if let (Some(from), Some(until)) = (m.from.as_deref(), m.until.as_deref()) {
        out["days"] = day_strip(c, from, until);
    }
    if let Some(g) = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id)).cloned() {
        let state = match g.state {
            State::Offered => trf("On offer until %{date}", &[("date", &day_label(&g.offered_until))]),
            State::Running => trf("Running until %{date}", &[("date", &day_label(g.deadline.as_deref().unwrap_or("")))]),
            State::Done => tr("Reached"),
            State::Failed => tr("Missed"),
        };
        out["goal"] = json!({ "bar": bar_of(c, m), "trophies": g.trophies, "reward": eur(g.reward), "rule": tr(g.aim.rule()), "state": state });
    } else if m.progress.is_some() {
        out["bar"] = bar_of(c, m).unwrap_or(Value::Null);
    }
    if let Some((tab, label, icon)) = page_for(m, c.game_mode) {
        out["go"] = json!({ "tab": tab, "label": tr(label), "icon": icon });
    }
    out
}

/// The inbox brought up to date before it is looked at (`inbox::ensure`): the welcome said, the
/// week's goals drawn.
fn ensure(s: &mut Session) {
    let Some(c) = s.company.as_ref() else { return };
    if c.inbox.welcomed && c.contracts.week == co::dates::week_of(&c.date) {
        return;
    }
    let _ = super::act(s, |c| {
        co::inbox::welcome(c);
        let date = c.date.clone();
        co::contracts::refresh(c, &date);
        Ok(())
    });
}

/// The filter the page asks for: "all", "unread", or a kind's name.
fn shows(filter: &str, m: &Message) -> bool {
    match filter {
        "" | "all" => true,
        "unread" => !m.read,
        k => format!("{:?}", m.kind) == k,
    }
}

fn kinds_of(filter: &str) -> Vec<Kind> {
    Kind::ALL.iter().copied().filter(|k| format!("{k:?}") == filter).collect()
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    ensure(s);
    let Some(c) = s.company.clone() else { return Value::Null };
    let mut filter = args.get("filter").and_then(Value::as_str).unwrap_or("all").to_string();
    let open = args.get("open").and_then(Value::as_u64);
    let msgs = &c.inbox.messages;
    let mut filters = vec![json!(["all", format!("{}  {}", tr("All"), msgs.len())]), json!(["unread", format!("{}  {}", tr("Unread"), c.inbox.unread())])];
    for k in Kind::ALL {
        let n = msgs.iter().filter(|m| m.kind == k).count();
        if n > 0 {
            filters.push(json!([format!("{k:?}"), format!("{}  {}", tr(k.label()), n)]));
        }
    }
    if !filters.iter().any(|f| f[0] == filter.as_str()) {
        filter = "all".into();
    }
    let list: Vec<Value> = msgs
        .iter()
        .rev()
        .filter(|m| shows(&filter, m))
        .map(|m| {
            let goal = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id));
            json!({
                "id": m.id, "icon": icon_of(&c, m), "tone": tone_of(m.level), "read": m.read,
                "title": words(&m.title, &m.args), "date": short_day(&m.date), "text": words(&m.text, &m.args),
                "goal": goal.map(|g| json!({ "trophies": g.trophies, "reward": eur(g.reward) })), "bar": bar_of(&c, m),
            })
        })
        .collect();
    let empty = tr(if filter == "unread" { "Everything is read." } else { "No messages yet. What happens to your company and around it comes in here as its days go by." });
    let open_m = open.and_then(|id| c.inbox.get(id)).map(|m| details(&c, m));
    json!({
        "filters": filters, "filter": filter, "list": list, "empty": empty,
        "mark_all": tr("Mark all read"), "delete_read": tr("Delete read"), "delete_read_tip": tr("Delete the messages already read (of the kind shown)"),
        "trophies_title": tr("Trophies").to_uppercase(), "trophies": trophy_bar(&c),
        "message_title": tr("Message").to_uppercase(), "open": open_m, "pick": tr("Click a message to read it here."),
        "unread_label": tr("Mark as unread"), "delete": tr("Delete"), "today": tr("Today"),
    })
}

/// What the inbox's buttons do.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let id = args.get("id").and_then(Value::as_u64);
    let filter = args.get("filter").and_then(Value::as_str).unwrap_or("all").to_string();
    let r = match what {
        "inbox_read" => super::act(s, |c| {
            if let Some(id) = id {
                c.inbox.mark(id, true);
            }
            Ok(())
        }),
        "inbox_unread" => super::act(s, |c| {
            if let Some(id) = id {
                c.inbox.mark(id, false);
            }
            Ok(())
        }),
        "inbox_delete" => super::act(s, |c| {
            if let Some(id) = id {
                c.inbox.delete(id);
            }
            Ok(())
        }),
        "inbox_mark_all" => {
            let kinds = kinds_of(&filter);
            super::act(s, |c| {
                c.inbox.mark_all(&kinds);
                Ok(())
            })
        }
        "inbox_delete_read" => {
            let kinds = kinds_of(&filter);
            super::act(s, |c| {
                c.inbox.delete_read(&kinds);
                Ok(())
            })
        }
        _ => return None,
    };
    Some(Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({}))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_filled_as_the_page_writes_it() {
        super::super::words::set_language("en");
        let args = vec![("n".to_string(), "101".to_string()), ("date".to_string(), "2024-03-06".to_string()), ("amount".to_string(), "120000".to_string())];
        assert_eq!(words("Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.", &args), "Bus 101 broke down on its tour: in the workshop until Wed 6 Mar 2024, repairs €1,200.");
        assert_eq!(value("price", "145.0"), "€1.45");
        assert_eq!(short_day("2024-03-08"), "Fri 8 Mar");
    }
}

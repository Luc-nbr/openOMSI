//! The Staff page's Holidays tab (Beta 57, `co::holidays`): how many drivers may be off on holiday a
//! day, a calendar of the coming weeks - who is off, who asked, against the limit, and the cover
//! the roster has then - and the requests to approve or reject; and a request as the inbox shows
//! it (`request`).

use super::super::words::{day_label, short_date, tr, trf, MONTHS, WEEKDAYS};
use super::super::{act as apply, Session};
use omsi_launcher_lib::company::holidays::{self as hd, By, Request, State};
use omsi_launcher_lib::company::{self as co, Company, Employee, GameMode};
use serde_json::{json, Value};

/// The weeks the calendar shows, from this week's Monday.
pub const WEEKS: i64 = 6;

/// "Mon 7 Jul – Sun 20 Jul" (one day: its own).
fn span(from: &str, until: &str) -> String {
    let day = |d: &str| {
        let Some(n) = co::dates::parse(d) else { return d.to_string() };
        format!("{} {}", tr(WEEKDAYS[co::dates::weekday(n) as usize]), short_date(d))
    };
    if from == until {
        day(from)
    } else {
        format!("{} – {}", day(from), day(until))
    }
}

/// What the game mode does with the requests, in a line each (Casual, Normal, Realistic).
const MODES: [&str; 3] = [
    "Casual: requests within the limit are approved by themselves, the others rejected.",
    "Normal: the planner suggests an answer to each request; one not answered by its day gets the planner's answer.",
    "Realistic: every answer is yours. A request not answered by its day counts as rejected.",
];
/// A request's answer, and who gave it.
const STATES: [&str; 5] = ["Waiting for your answer", "Approved", "Rejected", "by the planner", "no answer"];

fn state_of(r: &Request) -> (String, &'static str) {
    let by = match r.by {
        By::Player => "",
        By::Planner => STATES[3],
        By::Silence => STATES[4],
    };
    let (label, tone) = match r.state {
        State::Asked => (STATES[0], "warn"),
        State::Approved => (STATES[1], "ok"),
        State::Refused => (STATES[2], "danger"),
    };
    if by.is_empty() || r.state == State::Asked {
        (tr(label), tone)
    } else {
        (format!("{} ({})", tr(label), tr(by)), tone)
    }
}

/// A request as the pages show it: whose, its days and reason, how much it matters, what is left
/// of their year's days and how often they were rejected, the planner's suggestion (Normal), the
/// days it would go over the limit, and its answer or the buttons.
pub fn request(c: &Company, e: &Employee, r: &Request) -> Value {
    let asked = r.state == State::Asked;
    let over = if asked { hd::over_limit(c, e.id, r) } else { Vec::new() };
    let max = hd::limit_of(c, e.role);
    let over_text = (!over.is_empty()).then(|| {
        let most = over.iter().map(|x| x.1).max().unwrap_or(0);
        trf("Over the limit on %{n} days: %{most} would be off where %{max} may be.", &[("n", &over.len().to_string()), ("most", &most.to_string()), ("max", &max.to_string())])
    });
    let advice = (asked && c.game_mode == GameMode::Normal).then(|| {
        let a = hd::advice(c, e.id, r);
        let lead = if a.approve() { "Suggested: approve - %{what}." } else { "Suggested: reject - %{what}." };
        json!({ "approve": a.approve(), "text": trf(lead, &[("what", &tr(a.why()))]) })
    });
    let refused = e.leave.refused(&c.date);
    let p = r.reason.priority();
    let (state, tone) = state_of(r);
    let days = r.days();
    json!({
        "no": r.no, "id": e.id, "portrait": e.portrait(), "name": e.name, "role": tr(e.role.label()), "role_icon": co::staff::icon(e.role),
        "dates": span(&r.from, &r.until),
        "length": format!("{}  ·  {}", if days == 1 { tr("1 day") } else { trf("%{n} days", &[("n", &days.to_string())]) }, trf("%{n} working days", &[("n", &r.working_days().to_string())])),
        "reason": tr(r.reason.label()), "icon": r.reason.icon(),
        "priority": [tr(p.label()), match p { hd::Priority::High => "warn", hd::Priority::Normal => "accent2", hd::Priority::Low => "dim" }],
        "asked": trf("Asked on %{date}", &[("date", &day_label(&r.asked))]),
        "due": asked.then(|| trf("Answer by %{date}", &[("date", &day_label(&r.due))])),
        "left": trf("%{n} of their holiday days not planned yet", &[("n", &hd::free_days(e, None).to_string())]),
        "refused": (refused > 0).then(|| if refused == 1 { tr("Rejected once this year") } else { trf("Rejected %{n} times this year", &[("n", &refused.to_string())]) }),
        "advice": advice, "over": over_text, "open": asked, "state": state, "tone": tone,
        "approve": tr("Approve"), "reject": tr("Reject"),
    })
}

/// A day of the calendar in words: who is off and asked against the limit, and the cover.
fn day_tip(c: &Company, p: &hd::PlannerDay) -> String {
    let mut v = vec![trf("Off on holiday: %{n} of %{max} allowed", &[("n", &p.off.len().to_string()), ("max", &p.limit.to_string())])];
    if !p.asked.is_empty() {
        v.push(trf("Asked for, not answered: %{n}", &[("n", &p.asked.len().to_string())]));
    }
    v.push(cover_text(p));
    let names = |ids: &[u32]| ids.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect::<Vec<_>>().join(", ");
    if !p.off.is_empty() {
        v.push(format!("{}: {}", tr("On holiday"), names(&p.off)));
    }
    if !p.asked.is_empty() {
        v.push(format!("{}: {}", tr("Asked for"), names(&p.asked)));
    }
    v.join("\n")
}

/// The cover of a day: the roster's drivers away, and the free drivers who could stand in.
fn cover_text(p: &hd::PlannerDay) -> String {
    if p.away == 0 {
        trf("Cover: all %{n} drivers of the roster are there", &[("n", &p.rostered.to_string())])
    } else if p.short() == 0 {
        trf("Cover: %{away} of the roster's %{n} drivers away, %{spare} free drivers can stand in", &[("away", &p.away.to_string()), ("n", &p.rostered.to_string()), ("spare", &p.spare.to_string())])
    } else {
        trf("Cover short: %{away} of the roster's %{n} drivers away, only %{spare} free to stand in", &[("away", &p.away.to_string()), ("n", &p.rostered.to_string()), ("spare", &p.spare.to_string())])
    }
}

/// The Holidays tab: the limit, the mode's way of answering, the calendar (the day picked: `hday`)
/// and the requests - those waiting first, then those answered lately.
pub fn part(c: &Company, ui: &Value) -> Value {
    let day0 = co::dates::parse(&c.date).unwrap_or(0);
    let monday = day0 - co::dates::weekday(day0) as i64;
    let last = monday + WEEKS * 7 - 1;
    let pick = ui.get("hday").and_then(Value::as_str).and_then(co::dates::parse).filter(|d| (day0..=last).contains(d)).unwrap_or(day0);
    let limit = hd::limit(c);
    let days: Vec<Value> = (monday..=last)
        .map(|d| {
            let date = co::dates::fmt(d);
            let (_, m, day) = co::dates::civil_from_days(d);
            let gone = d < day0;
            let p = hd::planner_day(c, &date);
            let (off, asked) = (p.off.len() as u32, p.asked.len() as u32);
            let tone = if gone {
                "faint"
            } else if p.short() > 0 {
                "danger"
            } else if off > limit {
                "warn"
            } else if off == limit || off + asked > limit {
                "accent2"
            } else {
                "ok"
            };
            json!({
                "date": date, "day": day, "month": (day == 1 || d == monday).then(|| tr(MONTHS[(m as usize).clamp(1, 12) - 1])),
                "off": off, "asked": asked, "frac": (off as f64 / limit.max(1) as f64).min(1.0), "tone": tone, "gone": gone, "today": d == day0, "on": d == pick,
                "short": p.short(), "count": format!("{off} / {limit}"), "tip": if gone { String::new() } else { day_tip(c, &p) },
            })
        })
        .collect();
    // the day picked
    let pd = hd::planner_day(c, &co::dates::fmt(pick));
    let names = |ids: &[u32]| ids.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect::<Vec<_>>();
    let day = json!({
        "title": day_label(&pd.date), "off_title": trf("Off on holiday: %{n} of %{max} allowed", &[("n", &pd.off.len().to_string()), ("max", &pd.limit.to_string())]),
        "off": names(&pd.off), "asked_title": trf("Asked for, not answered: %{n}", &[("n", &pd.asked.len().to_string())]), "asked": names(&pd.asked),
        "cover": cover_text(&pd), "cover_tone": if pd.short() > 0 { "danger" } else if pd.away > 0 { "warn" } else { "ok" },
        "nobody": tr("Nobody"),
    });
    // the requests
    let waiting: Vec<Value> = hd::waiting(c).into_iter().map(|(e, r)| request(c, e, r)).collect();
    let mut done: Vec<(&Employee, &Request)> = c
        .staff
        .iter()
        .flat_map(|e| e.leave.requests.iter().filter(|r| r.state != State::Asked && (r.until.as_str() >= c.date.as_str() || r.answered.as_deref().is_some_and(|a| co::dates::between(a, &c.date) <= 14))).map(move |r| (e, r)))
        .collect();
    done.sort_by(|a, b| a.1.from.cmp(&b.1.from).then(a.1.no.cmp(&b.1.no)));
    let done: Vec<Value> = done.into_iter().map(|(e, r)| request(c, e, r)).collect();
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let mode = match c.game_mode {
        GameMode::Casual => MODES[0],
        GameMode::Normal => MODES[1],
        GameMode::Realistic => MODES[2],
    };
    json!({
        "kind": "holidays",
        "limit_title": tr("Drivers off on holiday a day, at most"), "limit": limit, "own": c.staff_rules.holidays.max_off.is_some(),
        "by_fleet": trf("By the fleet: %{n} (a quarter of your %{buses} buses)", &[("n", &hd::default_limit(c).to_string()), ("buses", &buses.to_string())]),
        "reset": tr("By the fleet"), "fewer": tr("Fewer"), "more": tr("More"), "most": hd::MOST,
        "mode": tr(mode),
        "notes": [
            tr("Approved holidays are planned ahead: free colleagues cover the duties of who is off, and the roster keeps their place for when they are back."),
            tr("A rejection costs satisfaction, more each time it happens to the same person, and weighs on them for half a year. An approval pleases them a little."),
        ],
        "calendar_title": tr("The coming weeks").to_uppercase(), "weekdays": WEEKDAYS.iter().map(|w| tr(w)).collect::<Vec<_>>(), "days": days, "day": day,
        "legend": [[tr("Within the limit"), "ok"], [tr("At the limit, or with requests waiting"), "accent2"], [tr("Over the limit"), "warn"], [tr("Cover short"), "danger"]],
        "waiting_title": trf("Requests waiting: %{n}", &[("n", &waiting.len().to_string())]).to_uppercase(), "waiting": waiting,
        "none": tr("No requests are waiting. Your staff ask for their holidays ahead: most for the summer and Christmas, seldom at short notice."),
        "done_title": tr("Answered lately").to_uppercase(), "done": done,
    })
}

/// What the Holidays tab's and the inbox's buttons do: a request answered (`holiday_answer`, an
/// approval over the limit only once confirmed: `force`), the limit set (`holiday_limit`).
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(match what {
        "holiday_answer" => {
            let approve = args.get("approve").and_then(Value::as_bool).unwrap_or(false);
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            Ok(answer(s, no, approve, force))
        }
        "holiday_limit" => {
            let n = args.get("n").and_then(Value::as_u64).map(|n| n as u32);
            let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
            match apply(s, |c| {
                hd::set_limit(c, if reset { None } else { n });
                Ok(())
            }) {
                Ok(()) => Ok(json!({})),
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        _ => return None,
    })
}

/// Answer request `no`: an approval over the limit not confirmed yet says so (`over`: the page
/// asks first, `dialogs::confirm`'s "holiday").
pub fn answer(s: &mut Session, no: u32, approve: bool, force: bool) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({}) };
    let Some((e, r)) = hd::find(c, no).map(|(e, r)| (e.clone(), r.clone())) else { return json!({}) };
    if approve && !force && r.state == State::Asked && !hd::over_limit(c, e.id, &r).is_empty() {
        return json!({ "over": true });
    }
    match apply(s, |c| hd::answer(c, no, approve, By::Player)) {
        Ok(()) => {
            let t = if approve {
                trf("Approved: %{name} is off from %{from} until %{until}.", &[("name", &e.name), ("from", &day_label(&r.from)), ("until", &day_label(&r.until))])
            } else {
                trf("Rejected: %{name} is less satisfied.", &[("name", &e.name)])
            };
            s.say(t, false);
            json!({})
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// The confirmation of an approval over the limit (`dialogs::confirm`): its title and words.
pub fn confirm(c: &Company, no: u32) -> Option<(String, String)> {
    let (e, r) = hd::find(c, no)?;
    let over = hd::over_limit(c, e.id, r);
    let most = over.iter().map(|x| x.1).max().unwrap_or(0);
    let max = hd::limit_of(c, e.role);
    let days = over.iter().map(|x| short_date(&x.0)).collect::<Vec<_>>().join(", ");
    let text = trf(
        "Approving %{name}'s holiday puts %{most} off on holiday where your limit is %{max}, on: %{days}. Their duties then need free colleagues, an agency driver or are dropped.",
        &[("name", &e.name), ("most", &most.to_string()), ("max", &max.to_string()), ("days", &days)],
    );
    Some((trf("%{name}: over the limit", &[("name", &e.name)]), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text of the holidays - the core's (`holidays::texts`), this tab's, and the words the
    /// staff page, the inbox and the confirmation add - in the six languages.
    #[test]
    fn every_holiday_text_is_translated() {
        let src = include_str!("holidays.rs");
        let mut texts: Vec<String> = Vec::new();
        for lead in ["tr(\"", "trf(\""] {
            let mut rest = src;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                texts.push(after.chars().take_while(|c| *c != '"').collect());
            }
        }
        assert!(texts.len() > 30, "only {} texts found", texts.len());
        texts.extend(hd::texts().into_iter().chain(MODES).chain(STATES).map(String::from));
        texts.extend(["Holidays", "Asks for a holiday", "Not planned yet", "Holidays rejected this year", "Approve anyway", "To the holiday planner"].map(String::from));
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-holidays.yml):
{}", missing.len(), missing.join("
"));
    }

    #[test]
    fn a_span_of_days_reads_short() {
        super::super::super::words::set_language("en");
        assert_eq!(span("2024-07-08", "2024-07-21"), "Mon 8 Jul – Sun 21 Jul");
        assert_eq!(span("2024-07-08", "2024-07-08"), "Mon 8 Jul");
    }
}

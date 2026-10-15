//! The company's overview (`crates/omsi-launcher-ui/src/company/overview.rs`): its figures, the
//! last thirty days as a chart, today's tours and what wants attention, the company's day as it
//! went (the clock's feed), the last day closed and the way to another company.

use super::words::{day_label, eur, grouped, month_label, num, short_date, signed, share_tone, tr, trf, grade};
use super::{career, feed_text, feed_tone, Session};
use omsi_launcher_lib::company::clock::{self as ck, Level};
use omsi_launcher_lib::company::{self as co, Alert, Company, DayRecord};
use serde_json::{json, Value};

/// How many days the chart shows up to yesterday: thirty, or - for a young company - the days
/// since its first closed day, at least a week.
pub fn chart_span(history: &[DayRecord], today: i64) -> i64 {
    let oldest = history.iter().filter_map(|d| co::dates::parse(&d.date)).filter(|x| *x < today).min();
    oldest.map_or(30, |o| (today - o).clamp(7, 30))
}

/// A round step for an axis of `span` with about `n` steps (1, 2 or 5 times a power of ten).
pub fn nice_step(span: f64, n: f64) -> f64 {
    let raw = (span / n).max(1.0);
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|k| k * p).find(|s| *s >= raw).unwrap_or(10.0 * p)
}

/// An alert as a mode says it (`overview::alert_for`): the easy mode leaves out what the company
/// sees to itself and sends tours not covered to the fleet or the staff.
pub fn alert_for(a: &Alert, easy: bool) -> Option<(&'static str, String, &'static str, usize)> {
    if !easy {
        return Some(alert_text(a));
    }
    match a {
        Alert::ServiceDue(_) | Alert::NotPlanned(_) | Alert::Tomorrow { .. } => None,
        Alert::Uncovered { buses, .. } => {
            let (icon, text, tone, _) = alert_text(a);
            Some((icon, text, tone, if *buses > 0 { 1 } else { 2 }))
        }
        _ => Some(alert_text(a)),
    }
}

/// An alert's icon, words, tone and the page that helps.
pub fn alert_text(a: &Alert) -> (&'static str, String, &'static str, usize) {
    match a {
        Alert::NoLines => ("route", tr("The company runs no line yet: add one on the Lines page."), "warn", 3),
        Alert::NoBuses => ("directions_bus", tr("There is no bus in the fleet: buy, lease or rent one."), "warn", 1),
        Alert::NoDrivers => ("groups", tr("Nobody works here yet: hire drivers on the Staff page."), "warn", 2),
        Alert::Uncovered { tours, buses, duties } => {
            let mut t = trf("%{n} tours are not covered today", &[("n", &tours.to_string())]);
            if *buses > 0 {
                t.push_str(&trf(": %{n} without a bus", &[("n", &buses.to_string())]));
            }
            if *duties > 0 {
                t.push_str(&trf(", %{n} duties without a driver", &[("n", &duties.to_string())]));
            }
            ("warning", t, "danger", 5)
        }
        Alert::NotPlanned(lines) => ("event", trf("Not in service yet: line %{n}. Plan its tours and start its service.", &[("n", &lines.join(", "))]), "warn", 5),
        Alert::Tomorrow { tours, buses, duties } => ("event", trf("Tomorrow %{n} tours are not covered: %{b} without a bus, %{d} duties without a driver.", &[("n", &tours.to_string()), ("b", &buses.to_string()), ("d", &duties.to_string())]), "warn", 5),
        Alert::LowCash => ("payments", tr("Cash is low: less than a month's wages."), "danger", 4),
        Alert::ServiceDue(n) => ("construction", trf("%{n} buses are due for their service.", &[("n", &n.to_string())]), "warn", 1),
        Alert::Unhappy(n) => ("person", trf("%{n} employees are unhappy and may leave.", &[("n", &n.to_string())]), "warn", 2),
        Alert::GoingBack { number, until } => ("event", trf("Bus %{n} goes back on %{date}.", &[("n", number), ("date", &day_label(until))]), "soft", 1),
    }
}

/// The days' results as the chart draws them: the days' slots, each closed day's bar, the scale's
/// steps and the dates under it.
fn chart(c: &Company) -> Value {
    let Some(t) = co::dates::parse(&c.date) else { return Value::Null };
    let span = chart_span(&c.history, t);
    let first = t - span;
    let days: Vec<&DayRecord> = c.history.iter().filter(|d| co::dates::parse(&d.date).is_some_and(|x| x >= first && x < t)).collect();
    if days.is_empty() {
        return json!({ "span": span, "empty": true, "title": tr("No day closed yet"), "text": tr("Each closed day's result appears here as a bar: green for a profit, red for a loss.") });
    }
    let hi = days.iter().map(|d| d.result).max().unwrap_or(0).max(0) as f64;
    let lo = days.iter().map(|d| d.result).min().unwrap_or(0).min(0) as f64;
    let step = nice_step((hi - lo).max(100_00.0), 4.0);
    let top = (hi / step).ceil() * step;
    let bottom = (lo / step).floor() * step;
    let (top, bottom) = if top == bottom { (step, 0.0) } else { (top, bottom) };
    let mut scale = Vec::new();
    let mut v = bottom;
    while v <= top + 1.0 {
        scale.push(json!([v, eur(v as i64)]));
        v += step;
    }
    let bars: Vec<Value> = days
        .iter()
        .filter_map(|d| {
            let x = co::dates::parse(&d.date)?;
            let tip = format!("{}\n{}  ·  {}", day_label(&d.date), signed(d.result).0, trf("%{c} of %{t} tours", &[("c", &(d.tours - d.dropped_tours).to_string()), ("t", &d.tours.to_string())]));
            Some(json!({ "k": x - first, "v": d.result, "tip": tip }))
        })
        .collect();
    let dates: Vec<Value> = (0..span).map(|k| json!(short_date(&co::dates::fmt(first + k)))).collect();
    json!({ "span": span, "top": top, "bottom": bottom, "scale": scale, "bars": bars, "dates": dates })
}

/// The clock's feed: what happened, the newest first (`clock::feed`).
pub fn feed(c: &Company, minor: bool) -> Value {
    let items: Vec<&ck::FeedItem> = c.clock.feed.iter().rev().filter(|f| minor || f.level != Level::Minor).take(120).collect();
    let today = c.date.clone();
    let empty = if items.is_empty() && c.clock.feed.iter().any(|f| f.at >= ck::moment(&c.date, 0)) {
        Some(tr("The day runs as planned so far. \"All tours\" shows every tour leaving and coming back."))
    } else if items.is_empty() {
        Some(tr("Nothing happened yet. Simulate the company's time at the top: by the hour, to the morning, to tomorrow or days at once; what happens is told here."))
    } else {
        None
    };
    let mut rows = Vec::new();
    let mut day = String::new();
    for f in items {
        let d = ck::date_of(f.at);
        if d != day {
            day = d.clone();
            if d != today {
                rows.push(json!({ "day": day_label(&d).to_uppercase() }));
            }
        }
        rows.push(json!({ "at": ck::hhmm(f.at), "text": feed_text(f), "tone": feed_tone(f.level), "minor": f.level == Level::Minor }));
    }
    json!({ "title": tr("The company's day"), "minor": minor, "toggle": tr("All tours"), "tip": tr("Show every tour leaving and coming back too"), "empty": empty, "rows": rows })
}

pub fn view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let month = c.month(&co::dates::month_of(&c.date));
    let ready = c.fleet.iter().filter(|v| v.held_on(&c.date) && !v.in_workshop(&c.date)).count();
    let here = c.staff.iter().filter(|e| e.employed_on(&c.date) && !e.absent(&c.date)).count();
    let timed = c.history.iter().any(|d| d.punctuality.is_some());
    let figures = vec![
        json!({ "label": tr("Cash"), "value": super::sandbox::cash(&c), "under": if c.debt() > 0 { trf("Loans: %{amount}", &[("amount", &eur(c.debt()))]) } else { String::new() }, "tone": super::sandbox::cash_tone(&c), "tip": tr("The money the company has now") }),
        json!({ "label": tr("This month"), "value": signed(month.result()).0, "under": month_label(&month.month), "tone": if month.result() >= 0 { "ok" } else { "danger" }, "tip": tr("What running the lines brought this month, less what it cost (buying and loans apart)") }),
        json!({ "label": tr("Fleet"), "value": c.fleet.len().to_string(), "under": trf("%{n} ready today", &[("n", &ready.to_string())]), "tone": "text", "tip": tr("Buses of the company, and how many are not in the workshop today") }),
        json!({ "label": tr("Staff"), "value": c.staff.len().to_string(), "under": trf("%{n} at work today", &[("n", &here.to_string())]), "tone": "text", "tip": tr("People on the payroll, and how many are not ill or on holiday today") }),
        json!({ "label": tr("On time"), "value": if timed { format!("{:.0} %", c.punctuality) } else { "–".to_string() }, "under": tr("of the trips, lately"), "tone": if timed { grade(c.punctuality * 1.2 - 20.0) } else { "text" }, "tip": tr("Trips at most three minutes late, over the last days") }),
        json!({ "label": tr("Reputation"), "value": format!("{:.0}", c.reputation), "under": tr("of 100"), "tone": grade(c.reputation + 15.0), "tip": tr("What passengers and the authority think of the company: trips run on time raise it, trips dropped lower it") }),
    ];
    let span = co::dates::parse(&c.date).map_or(30, |t| chart_span(&c.history, t));
    // today: the tours covered, my next duty, what wants attention
    let easy = co::autopilot::on(&c);
    let plan = s.plan.clone();
    let covered = match &plan {
        Some(p) if p.tours.iter().any(|t| !t.tour.unplanned) => {
            let n = p.tours.iter().filter(|t| !t.tour.unplanned).count();
            let cv = n - p.uncovered();
            let frac = cv as f64 / n as f64;
            let (buses, duties) = p.short_of();
            let tip = if cv == n {
                tr("Every tour in service has a bus and its drivers.")
            } else if easy {
                trf("Open: %{b} tours without a bus, %{d} duties without a driver. Get buses at the dealer; the company hires the drivers by itself, at night.", &[("b", &buses.to_string()), ("d", &duties.to_string())])
            } else {
                trf("Open: %{b} tours without a bus, %{d} duties without a driver. Click for the planning.", &[("b", &buses.to_string()), ("d", &duties.to_string())])
            };
            json!({ "bar": { "frac": frac, "tone": share_tone(frac, false), "label": trf("Covered today: %{c} of %{t} tours", &[("c", &cv.to_string()), ("t", &n.to_string())]), "right": format!("{:.0} %", frac * 100.0), "tip": tip, "click": !easy } })
        }
        Some(_) => json!({ "text": tr("No tours run today."), "tone": "soft" }),
        None => {
            let t = if s.today.as_ref().is_some_and(|t| t.error.is_some()) { "The map's timetable could not be read." } else { "Reading the timetable…" };
            json!({ "text": tr(t), "tone": "soft" })
        }
    };
    let (_, mine, whole) = career::my_duties(s);
    let next = match mine.first() {
        Some(d) => {
            let when = format!("{} {}", day_label(&d.date), ck::hhmm(d.from as i64));
            json!({ "text": trf("Your next duty: %{when}, line %{n}, tour %{t}", &[("when", &when), ("n", &d.number), ("t", &d.tour_no)]), "button": tr("Drive"), "tip": tr("All your duties: Career, My duties"), "duty": career::duty_key(d) })
        }
        None if whole => {
            let (label, icon) = if easy { ("Choose a duty", "play_arrow") } else { ("Plan a duty", "event") };
            json!({ "none": tr("No duty of yours is planned."), "button": tr(label), "icon": icon, "easy": easy })
        }
        None => Value::Null,
    };
    let mut alerts = co::alerts(&c, plan.as_ref());
    if !easy {
        if let Some(a) = super::tomorrow(s).as_ref().and_then(co::tomorrow_alert) {
            alerts.push(a);
        }
    }
    let alerts: Vec<Value> = alerts.iter().filter_map(|a| alert_for(a, easy)).map(|(icon, text, tone, tab)| json!({ "icon": icon, "text": text, "tone": tone, "tab": tab })).collect();
    // the last day closed
    let last = c.last_report.as_ref().map(|rep| {
        let (res, tone) = signed(rep.result);
        json!({
            "date": day_label(&rep.date), "result": res, "tone": tone,
            "tip": format!("{} {}  ·  {} {}", signed(rep.income).0, tr("in"), signed(-rep.expenses).0, tr("out")),
            "facts": [
                trf("%{c} of %{t} tours", &[("c", &rep.covered.to_string()), ("t", &rep.tours.to_string())]),
                trf("%{n} trips dropped", &[("n", &rep.dropped.to_string())]),
                trf("%{n} passengers", &[("n", &grouped(rep.passengers as f64))]),
                format!("{} km", grouped(rep.km.round())),
            ],
        })
    });
    json!({
        "figures": figures,
        "chart_title": trf("The last %{n} days", &[("n", &span.to_string())]),
        "chart": chart(&c),
        "today": { "title": tr("Today"), "covered": covered, "next": next, "alerts": alerts, "calm": tr("Nothing wants your attention.") },
        "feed": feed(&c, s.feed_minor),
        "last": { "title": tr("The last day closed"), "report": last, "none": tr("\"Simulate to tomorrow\" at the top runs the rest of the day and closes it at midnight: the tours are run, the money booked, and the next day begins. What you drove yourself on the company's lines counts as measured."), "button": tr("Show the report") },
        "companies": { "title": tr("Your companies"), "text": trf("Founded on %{date}.", &[("date", &day_label(&c.founded))]), "button": tr("Other company") },
        "labels": { "num": num(0.0, 0) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chart_has_round_steps_and_a_young_company_sees_its_few_days_wide() {
        assert_eq!(nice_step(1_000_00.0, 4.0), 500_00.0);
        assert_eq!(nice_step(3.0, 4.0), 1.0);
        let day = |d: &str| DayRecord { date: d.into(), ..Default::default() };
        let t = co::dates::parse("2024-03-05").unwrap();
        assert_eq!(chart_span(&[], t), 30);
        assert_eq!(chart_span(&[day("2024-03-04")], t), 7);
        assert_eq!(chart_span(&[day("2023-12-01"), day("2024-03-04")], t), 30);
    }
}

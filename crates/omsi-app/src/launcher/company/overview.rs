//! The company's overview: its figures (cash, the month's result, fleet, staff, punctuality,
//! reputation), the last thirty days in a chart, today's tours and what wants attention, and
//! the last day closed.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::{day_label, eur, grouped, meter, section};
use glam::Vec2;
use omsi_launcher_lib::company::{self as co, Alert, Company, DayRecord};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let gap = 12.0;
    // the figures
    let month = c.month(&co::dates::month_of(&c.date));
    let ready = c.fleet.iter().filter(|v| v.held_on(&c.date) && !v.in_workshop(&c.date)).count();
    let here = c.staff.iter().filter(|e| e.employed_on(&c.date) && !e.absent(&c.date)).count();
    let fw = (area.w - 5.0 * gap) / 6.0;
    let fh = 84.0;
    let figures: [(&str, String, String, Color); 6] = [
        ("Cash", eur(c.cash), if c.debt() > 0 { omsi_ui::tr("Loans: %{amount}").replace("%{amount}", &eur(c.debt())) } else { String::new() }, if c.cash >= 0 { TEXT } else { DANGER.lighten(0.2) }),
        ("This month", eur(month.result()), super::month_label(&month.month), if month.result() >= 0 { OK } else { DANGER.lighten(0.2) }),
        ("Fleet", c.fleet.len().to_string(), omsi_ui::tr("%{n} ready today").replace("%{n}", &ready.to_string()), TEXT),
        ("Staff", c.staff.len().to_string(), omsi_ui::tr("%{n} at work today").replace("%{n}", &here.to_string()), TEXT),
        ("On time", if c.history.is_empty() { "–".to_string() } else { format!("{:.0} %", c.punctuality) }, omsi_ui::tr("of the trips, lately").into_owned(), if c.history.is_empty() { TEXT } else { super::grade(c.punctuality * 1.2 - 20.0) }),
        ("Reputation", format!("{:.0}", c.reputation), omsi_ui::tr("of 100").into_owned(), super::grade(c.reputation + 15.0)),
    ];
    for (k, (label, value, under, colour)) in figures.iter().enumerate() {
        super::figure(&mut l.ui, Rect::new(area.x + k as f32 * (fw + gap), area.y, fw, fh), label, value, under, *colour);
    }
    let y = area.y + fh + gap;
    let left_w = (area.w - gap) * 0.6;
    let right_w = area.w - gap - left_w;
    let h1 = ((area.bottom() - y - gap) * 0.55).max(180.0);
    // the last thirty days
    let chart = section(&mut l.ui, Rect::new(area.x, y, left_w, h1), "The last 30 days");
    chart_of(&mut l.ui, chart, &c.history);
    // today
    today(l, Rect::new(area.x + left_w + gap, y, right_w, h1), &c);
    // the company's day as it went, the last day closed, and the companies
    let y2 = y + h1 + gap;
    let h2 = (area.bottom() - y2).max(120.0);
    let mut minor = l.company.feed_minor;
    super::clock::feed(l, Rect::new(area.x, y2, left_w, h2), &c, &mut minor);
    l.company.feed_minor = minor;
    let h3 = (h2 * 0.56).max(130.0);
    last_day(l, Rect::new(area.x + left_w + gap, y2, right_w, h3), &c);
    companies(l, Rect::new(area.x + left_w + gap, y2 + h3 + gap, right_w, (h2 - h3 - gap).max(60.0)), &c);
}

/// The days' results as bars around the zero line (green above, red under), with the
/// highest and lowest written beside.
fn chart_of(ui: &mut Ui, r: Rect, history: &[DayRecord]) {
    let days: Vec<&DayRecord> = history.iter().rev().take(30).rev().collect();
    if days.is_empty() {
        ui.paragraph("No day closed yet. The chart shows each day's result once days are closed.", Vec2::new(r.x, r.y + 4.0), r.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let label_w = 86.0;
    let plot = Rect::new(r.x + label_w, r.y + 4.0, r.w - label_w, r.h - 26.0);
    let hi = days.iter().map(|d| d.result).max().unwrap_or(0).max(0);
    let lo = days.iter().map(|d| d.result).min().unwrap_or(0).min(0);
    let span = (hi - lo).max(1) as f32;
    let zero = plot.y + plot.h * hi as f32 / span;
    ui.p().rect(Rect::new(plot.x, zero, plot.w, 1.0), Color::WHITE.alpha(0.18));
    ui.text_in(&eur(hi), Rect::new(r.x, plot.y - 6.0, label_w - 10.0, 14.0), 11.0, Weight::Regular, TEXT_DIM, Align::Right);
    if lo < 0 {
        ui.text_in(&eur(lo), Rect::new(r.x, plot.bottom() - 8.0, label_w - 10.0, 14.0), 11.0, Weight::Regular, TEXT_DIM, Align::Right);
    }
    // (the zero line's mark, where it does not lie on the highest's or the lowest's)
    if zero - plot.y > 16.0 && plot.bottom() - zero > 16.0 {
        ui.text_in("0", Rect::new(r.x, zero - 7.0, label_w - 10.0, 14.0), 11.0, Weight::Regular, TEXT_FAINT, Align::Right);
    }
    let slot = plot.w / 30.0;
    let bw = (slot * 0.66).max(2.0);
    let start = plot.x + (30 - days.len()) as f32 * slot;
    for (k, d) in days.iter().enumerate() {
        let x = start + k as f32 * slot + (slot - bw) * 0.5;
        let h = (d.result.abs() as f32 / span * plot.h).max(1.0);
        let bar = if d.result >= 0 { Rect::new(x, zero - h, bw, h) } else { Rect::new(x, zero, bw, h) };
        let hover = ui.hover(Rect::new(start + k as f32 * slot, plot.y, slot, plot.h));
        let col = if d.result >= 0 { OK } else { DANGER };
        ui.p().rounded(bar, 2.0f32.min(bw * 0.4), if hover { col.lighten(0.25) } else { col.alpha(0.85) });
        if hover {
            let text = format!("{}: {}  ·  {}", day_label(&d.date), eur(d.result), omsi_ui::tr("%{c} of %{t} tours").replace("%{c}", &(d.tours - d.dropped_tours).to_string()).replace("%{t}", &d.tours.to_string()));
            ui.tooltip(Rect::new(start + k as f32 * slot, plot.y, slot, plot.h), &text);
        }
    }
    let first = days.first().map(|d| day_label(&d.date)).unwrap_or_default();
    let last = days.last().map(|d| day_label(&d.date)).unwrap_or_default();
    ui.text_in(&first, Rect::new(start, plot.bottom() + 6.0, 200.0, 14.0), 11.0, Weight::Regular, TEXT_DIM, Align::Left);
    ui.text_in(&last, Rect::new(plot.right() - 200.0, plot.bottom() + 6.0, 200.0, 14.0), 11.0, Weight::Regular, TEXT_DIM, Align::Right);
}

/// Today: the tours and how many are covered, and what wants attention.
fn today(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Today");
    let mut y = inner.y;
    let plan = l.company.plan.clone();
    match &plan {
        Some(p) if !p.tours.is_empty() => {
            let n = p.tours.len();
            let covered = n - p.uncovered();
            let text = omsi_ui::tr("%{c} of %{t} tours covered").replace("%{c}", &covered.to_string()).replace("%{t}", &n.to_string());
            l.ui.text_in(&text, Rect::new(inner.x, y, inner.w, 22.0), 16.0, Weight::Bold, if covered == n { OK } else { TEXT }, Align::Left);
            y += 28.0;
            meter(&mut l.ui, Rect::new(inner.x, y, inner.w, 6.0), covered as f64 / n as f64, if covered == n { OK } else { WARN });
            y += 18.0;
        }
        Some(_) => {
            l.ui.text_in("No tours run today.", Rect::new(inner.x, y, inner.w, 22.0), 14.0, Weight::Medium, TEXT_SOFT, Align::Left);
            y += 30.0;
        }
        None => {
            let t = if l.company.today.as_ref().is_some_and(|t| t.error.is_some()) { "The map's timetable could not be read." } else { "Reading the timetable…" };
            l.ui.text_in(t, Rect::new(inner.x, y, inner.w, 22.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
            y += 30.0;
        }
    }
    let alerts = co::alerts(c, plan.as_ref());
    if alerts.is_empty() {
        l.ui.text_in("Nothing wants your attention.", Rect::new(inner.x, y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    for (k, a) in alerts.iter().enumerate() {
        if y + 30.0 > inner.bottom() + 8.0 {
            break;
        }
        let (icon, text, colour, tab) = alert_text(a);
        let row = Rect::new(inner.x - 8.0, y, inner.w + 16.0, 30.0);
        if l.ui.row(&format!("company-alert-{k}"), row, false) {
            l.company.tab = tab;
        }
        l.ui.icon(icon, Vec2::new(inner.x + 8.0, y + 15.0), 16.0, colour);
        l.ui.text_in(&text, Rect::new(inner.x + 26.0, y, inner.w - 40.0, 30.0), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.icon("chevron_right", Vec2::new(inner.right() - 4.0, y + 15.0), 16.0, TEXT_FAINT);
        y += 32.0;
    }
}

/// An alert's icon, words, colour and the tab that helps.
fn alert_text(a: &Alert) -> (&'static str, String, Color, usize) {
    match a {
        Alert::NoLines => ("route", omsi_ui::tr("The company runs no line yet: add one on the Lines page.").into_owned(), WARN, 3),
        Alert::NoBuses => ("directions_bus", omsi_ui::tr("There is no bus in the fleet: buy, lease or rent one.").into_owned(), WARN, 1),
        Alert::NoDrivers => ("groups", omsi_ui::tr("Nobody works here yet: hire drivers on the Staff page.").into_owned(), WARN, 2),
        Alert::Uncovered { tours, buses, duties } => {
            let mut t = omsi_ui::tr("%{n} tours are not covered today").replace("%{n}", &tours.to_string());
            if *buses > 0 {
                t.push_str(&omsi_ui::tr(": %{n} without a bus").replace("%{n}", &buses.to_string()));
            }
            if *duties > 0 {
                t.push_str(&omsi_ui::tr(", %{n} duties without a driver").replace("%{n}", &duties.to_string()));
            }
            ("warning", t, DANGER.lighten(0.2), if *buses > 0 { 1 } else { 2 })
        }
        Alert::LowCash => ("payments", omsi_ui::tr("Cash is low: less than a month's wages.").into_owned(), DANGER.lighten(0.2), 4),
        Alert::ServiceDue(n) => ("construction", omsi_ui::tr("%{n} buses are due for their service.").replace("%{n}", &n.to_string()), WARN, 1),
        Alert::Unhappy(n) => ("person", omsi_ui::tr("%{n} employees are unhappy and may leave.").replace("%{n}", &n.to_string()), WARN, 2),
        Alert::GoingBack { number, until } => ("event", omsi_ui::tr("Bus %{n} goes back on %{date}.").replace("%{n}", number).replace("%{date}", &day_label(until)), TEXT_SOFT, 1),
    }
}

/// The last day closed, in a few words, and the way to its report.
fn last_day(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "The last day closed");
    let Some(rep) = c.last_report.clone() else {
        l.ui.paragraph("\"Simulate to tomorrow\" at the top runs the rest of the day and closes it at midnight: the tours are run, the money booked, and the next day begins. What you drove yourself on the company's lines counts as measured.", Vec2::new(inner.x, inner.y), inner.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    };
    l.ui.text_in(&day_label(&rep.date), Rect::new(inner.x, inner.y, inner.w * 0.5, 22.0), 15.0, Weight::Bold, TEXT, Align::Left);
    l.ui.text_in(&eur(rep.result), Rect::new(inner.x + inner.w * 0.5, inner.y, inner.w * 0.5, 22.0), 17.0, Weight::Bold, if rep.result >= 0 { OK } else { DANGER.lighten(0.2) }, Align::Right);
    let facts = [
        omsi_ui::tr("%{c} of %{t} tours").replace("%{c}", &rep.covered.to_string()).replace("%{t}", &rep.tours.to_string()),
        omsi_ui::tr("%{n} trips dropped").replace("%{n}", &rep.dropped.to_string()),
        omsi_ui::tr("%{n} passengers").replace("%{n}", &grouped(rep.passengers as f64)),
        format!("{} km", grouped(rep.km.round())),
    ];
    // (two by two: the column is narrow)
    let fw = inner.w / 2.0;
    for (k, f) in facts.iter().enumerate() {
        l.ui.text_in(f, Rect::new(inner.x + (k % 2) as f32 * fw, inner.y + 28.0 + (k / 2) as f32 * 20.0, fw - 8.0, 20.0), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
    }
    let by = (inner.y + 76.0).min(inner.bottom() - 34.0);
    if l.ui.button("company-show-report", Rect::new(inner.x, by, 180.0, 34.0), "Show the report", Some("receipt_long"), ButtonKind::Normal) {
        l.company.reports = Some(vec![rep]);
    }
}

/// The driver's companies: which one is open, and founding another.
fn companies(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Your companies");
    let list = l.company.companies.clone().unwrap_or_default();
    let mut y = inner.y;
    if list.len() > 1 {
        let names: Vec<String> = list.iter().map(|x| format!("{}  ·  {}", x.name, x.map_name)).collect();
        let mut k = list.iter().position(|x| x.id == c.id).unwrap_or(0);
        if l.ui.select("company-pick", Rect::new(inner.x, y, inner.w, ROW), &mut k, &names) {
            l.company.company = list.get(k).cloned();
            l.company.plan = None;
        }
        y += ROW + 10.0;
    } else {
        let text = omsi_ui::tr("Founded on %{date}.").replace("%{date}", &day_label(&c.founded));
        l.ui.text_in(&text, Rect::new(inner.x, y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        y += 28.0;
    }
    if y + 34.0 <= inner.bottom() + 10.0 && l.ui.button("company-found-another", Rect::new(inner.x, y, 220.0f32.min(inner.w), 34.0), "Found another company", Some("add"), ButtonKind::Normal) {
        l.company.wizard = Some(super::wizard::Wizard::new(l));
    }
}

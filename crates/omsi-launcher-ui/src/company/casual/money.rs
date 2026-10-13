//! Money: what came in against what went out, a pair of bars a week for the last eight weeks
//! (green the weeks that earned, red those that cost), the biggest costs of the last four weeks
//! in plain words, and a tip when something costs too much. And My company: its livery (the
//! livery editor, painting a design of the company's own) and its depot - its size, and "Buy a
//! bigger depot" when the buses no longer fit.

use super::super::super::theme::*;
use super::super::super::ui::{ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::{act, eur, kit};
use super::logic::{self, Week};
use glam::Vec2;
use omsi_launcher_lib::company::depot::{self, Area};
use omsi_launcher_lib::company::{self as co, BookingKind, Cents, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

const GAP: f32 = 16.0;
/// The weeks shown.
const WEEKS: usize = 8;

/// The money page.
pub(super) fn page(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let sub = omsi_ui::tr("In the bank: %{cash}").replace("%{cash}", &eur(c.cash));
    let (body, _) = super::page_head(l, area, super::MONEY, &sub, None);
    let weeks = logic::weeks(&c, WEEKS);
    let costs = logic::biggest_costs(&c);
    let tip = logic::cost_tip(&costs);
    let phone = super::super::super::mobile::mobile() || body.w < 760.0;
    let mut go = None;
    super::super::super::tour::anchor("company-casual-money", body);
    l.ui.scroll_area("company-casual-money", body, &mut |ui, v| {
        let w = v.w.min(1180.0);
        let (chart_w, side_w) = if phone { (w, w) } else { ((w * 0.58).floor(), w - (w * 0.58).floor() - GAP) };
        let chart = Rect::new(v.x, v.y, chart_w, 330.0);
        weeks_card(ui, chart, &weeks);
        let side = if phone { Rect::new(v.x, chart.bottom() + GAP, w, 330.0) } else { Rect::new(chart.right() + GAP, v.y, side_w, 330.0) };
        costs_card(ui, side, &costs);
        let mut y = side.bottom().max(chart.bottom()) + GAP;
        if let Some((text, a)) = &tip {
            let r = Rect::new(v.x, y, w, if phone { 112.0 } else { 60.0 });
            ui.p().rounded(r, RADIUS, WARN.alpha(0.09));
            ui.p().rounded(Rect::new(r.x, r.y, 4.0, r.h), 2.0, WARN);
            ui.icon("light", Vec2::new(r.x + 26.0, if phone { r.y + 24.0 } else { r.center().y }), 19.0, WARN);
            let (label, icon) = match a {
                super::Act::AddLine => ("Add a line", "alt_route"),
                _ => ("Buy a bus", "add"),
            };
            let bw = kit::Foot::width(ui, label, Some(icon));
            let b = if phone { Rect::new(r.x + 48.0, r.bottom() - 48.0, bw, 36.0) } else { Rect::new(r.right() - 12.0 - bw, r.y + 12.0, bw, 36.0) };
            if phone {
                ui.paragraph(text, Vec2::new(r.x + 48.0, r.y + 12.0), r.w - 60.0, kit::BODY, Weight::Medium, TEXT);
            } else {
                ui.text_in(text, Rect::new(r.x + 48.0, r.y, b.x - 12.0 - r.x - 48.0, r.h), kit::BODY, Weight::Medium, TEXT, Align::Left);
            }
            if ui.button("company-casual-cost-tip", b, label, Some(icon), ButtonKind::Normal) {
                go = Some(a.clone());
            }
            y += r.h + GAP;
        }
        y - v.y
    });
    if let Some(a) = go {
        super::run(l, a);
    }
}

/// The weeks as pairs of bars: in (green) and out (red), the week's result under them.
fn weeks_card(ui: &mut Ui, r: Rect, weeks: &[Week]) {
    super::card_ground(ui, r, 0.0, None);
    let pad = 20.0;
    kit::caps(ui, Rect::new(r.x + pad, r.y + 16.0, r.w - 2.0 * pad, 16.0), "Week by week");
    // the legend
    let lx = r.right() - pad - 190.0;
    for (k, (word, colour)) in [("In", OK), ("Out", DANGER)].into_iter().enumerate() {
        let x = lx + k as f32 * 95.0;
        ui.p().rounded(Rect::new(x, r.y + 19.0, 12.0, 12.0), 3.0, colour);
        ui.text_in(word, Rect::new(x + 18.0, r.y + 14.0, 70.0, 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
    }
    let area = Rect::new(r.x + pad, r.y + 50.0, r.w - 2.0 * pad, r.h - 50.0 - 58.0);
    if weeks.is_empty() {
        ui.paragraph("No day closed yet: press Next day, and the first week's money shows here.", Vec2::new(area.x, area.y + 20.0), area.w, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    let top = weeks.iter().map(|w| w.income.max(w.expenses)).max().unwrap_or(1).max(1) as f32;
    let slot = area.w / WEEKS as f32;
    let bw = (slot * 0.3).min(22.0);
    ui.p().rect(Rect::new(area.x, area.bottom(), area.w, 1.0), HAIRLINE);
    // (the newest on the right; fewer weeks than eight stand at the right end)
    let skip = WEEKS.saturating_sub(weeks.len());
    for (k, w) in weeks.iter().enumerate() {
        let x = area.x + (k + skip) as f32 * slot + (slot - 2.0 * bw - 4.0) * 0.5;
        for (j, (v, colour)) in [(w.income, OK), (w.expenses, DANGER)].into_iter().enumerate() {
            let h = (area.h * v as f32 / top).max(if v > 0 { 2.0 } else { 0.0 });
            ui.p().rounded(Rect::new(x + j as f32 * (bw + 4.0), area.bottom() - h, bw, h), 3.0f32.min(bw * 0.3), colour.alpha(0.88));
        }
        let res = w.income - w.expenses;
        let (t, colour) = kit::signed(res);
        let short = short_money(res);
        let label = super::super::overview::short_date(&w.from);
        ui.text_in(&label, Rect::new(area.x + (k + skip) as f32 * slot, area.bottom() + 6.0, slot, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Center);
        ui.text_in(&short, Rect::new(area.x + (k + skip) as f32 * slot, area.bottom() + 24.0, slot, 20.0), 12.5, Weight::Bold, colour, Align::Center);
        ui.tooltip(Rect::new(area.x + (k + skip) as f32 * slot, area.y, slot, area.h + 44.0), &format!("{}: {}  ·  {} {}  ·  {} {}", label, t, omsi_ui::tr("In"), eur(w.income), omsi_ui::tr("Out"), eur(w.expenses)));
    }
}

/// A week's result, short: "+12k", "−3k" (a thousand euros as "k").
fn short_money(c: Cents) -> String {
    let e = c as f64 / 100.0;
    let sign = if c >= 0 { "+" } else { "−" };
    let a = e.abs();
    if a >= 1_000_000.0 {
        format!("{sign}{:.1}M", a / 1_000_000.0)
    } else if a >= 1_000.0 {
        format!("{sign}{:.0}k", a / 1_000.0)
    } else {
        format!("{sign}{a:.0}")
    }
}

/// The biggest costs of the last four weeks, in plain words, each with its share as a bar.
fn costs_card(ui: &mut Ui, r: Rect, costs: &[(BookingKind, Cents)]) {
    super::card_ground(ui, r, 0.0, None);
    let pad = 20.0;
    kit::caps(ui, Rect::new(r.x + pad, r.y + 16.0, r.w - 2.0 * pad, 16.0), "Where it goes (four weeks)");
    let total: Cents = costs.iter().map(|x| x.1).sum();
    let mut y = r.y + 48.0;
    if costs.is_empty() || total <= 0 {
        ui.paragraph("Nothing spent yet.", Vec2::new(r.x + pad, y), r.w - 2.0 * pad, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    let w = r.w - 2.0 * pad;
    for (k, amount) in costs.iter().take(5) {
        let share = *amount as f32 / total as f32;
        ui.text_in(logic::cost_words(*k), Rect::new(r.x + pad, y, w * 0.7, 22.0), kit::BODY, Weight::Medium, TEXT, Align::Left);
        ui.text_as_is(&eur(*amount), Vec2::new(r.right() - pad, y + 16.0), kit::BODY, Weight::Bold, TEXT, Align::Right);
        let bar = Rect::new(r.x + pad, y + 28.0, w, 6.0);
        ui.p().rounded(bar, 3.0, FIELD);
        ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * share).max(4.0), bar.h), 3.0, accent());
        y += 52.0;
    }
}

// --- my company ------------------------------------------------------------------------------------

/// The bus a livery of the company's own is painted on first: the fleet's most common model.
fn livery_bus(c: &Company) -> Option<(String, String)> {
    let mut count: Vec<(&str, &str, usize)> = Vec::new();
    for v in c.fleet.iter().filter(|v| v.held_on(&c.date)) {
        match count.iter_mut().find(|x| x.0 == v.bus) {
            Some(x) => x.2 += 1,
            None => count.push((&v.bus, &v.livery, 1)),
        }
    }
    count.sort_by_key(|x| std::cmp::Reverse(x.2));
    count.first().map(|x| (x.0.to_string(), x.1.to_string()))
}

/// The company page: its livery and its depot.
pub(super) fn company(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let (body, _) = super::page_head(l, area, super::COMPANY, &c.name, None);
    let phone = super::super::super::mobile::mobile() || body.w < 760.0;
    let w = body.w.min(1180.0);
    let cw = if phone { w } else { ((w - GAP) * 0.5).floor() };
    let ch = 300.0;
    let livery_r = Rect::new(body.x, body.y, cw, ch);
    let depot_r = if phone { Rect::new(body.x, livery_r.bottom() + GAP, cw, ch) } else { Rect::new(livery_r.right() + GAP, body.y, cw, ch) };
    livery_card(l, livery_r, &c);
    depot_card(l, depot_r, &c);
}

fn livery_card(l: &mut Launcher, r: Rect, c: &Company) {
    super::super::super::tour::anchor("company-casual-livery", r);
    super::card_ground(&mut l.ui, r, 0.0, None);
    let pad = 22.0;
    let x = r.x + pad;
    let w = r.w - 2.0 * pad;
    l.ui.icon("palette", Vec2::new(x + 14.0, r.y + pad + 14.0), 24.0, accent_2());
    l.ui.text_in("Company livery", Rect::new(x + 38.0, r.y + pad, w - 38.0, 28.0), kit::HEAD, Weight::Bold, TEXT, Align::Left);
    let mut y = r.y + pad + 44.0;
    // the company's colours
    for (k, hex) in c.colours.iter().enumerate() {
        let sw = Rect::new(x + k as f32 * 54.0, y, 44.0, 44.0);
        let colour = super::super::super::accent::parse_hex(hex).map(|v| Color::rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0)).unwrap_or(accent());
        l.ui.swatch(sw, RADIUS, colour);
        l.ui.p().rounded_border(sw, RADIUS, 1.0, EDGE);
    }
    let designs = c.designs.len();
    let t = if designs == 0 { omsi_ui::tr("No livery of your own yet: your buses wear the maker's.").into_owned() } else { omsi_ui::tr("%{n} liveries of your own").replace("%{n}", &designs.to_string()) };
    l.ui.text_in(&t, Rect::new(x + 120.0, y, w - 120.0, 44.0), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Left);
    y += 64.0;
    l.ui.paragraph("Paint your buses in your company's own colours in the livery editor. A bus painted in it wears it in the game too.", Vec2::new(x, y), w, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT);
    let bus = livery_bus(c);
    let label = "Paint the company livery";
    let bw = kit::Foot::width(&l.ui, label, Some("livery_brush")).min(w);
    let b = Rect::new(x, r.bottom() - pad - 42.0, bw, 42.0);
    match bus {
        Some((bus, paint)) => {
            if l.ui.button("company-casual-livery", b, label, Some("livery_brush"), ButtonKind::Primary) {
                super::super::fleet::design_livery(l, &bus, &paint, None);
            }
            l.ui.tooltip(b, &omsi_ui::tr("Designing a livery costs %{amount}").replace("%{amount}", &eur(co::livery::design_fee(c))));
        }
        None => {
            l.ui.text_in("Buy a bus first: then paint it.", Rect::new(x, b.y, w, b.h), kit::BODY, Weight::Medium, TEXT_DIM, Align::Left);
        }
    }
}

fn depot_card(l: &mut Launcher, r: Rect, c: &Company) {
    super::super::super::tour::anchor("company-casual-depot", r);
    super::card_ground(&mut l.ui, r, 0.0, None);
    let pad = 22.0;
    let x = r.x + pad;
    let w = r.w - 2.0 * pad;
    l.ui.icon("garage", Vec2::new(x + 14.0, r.y + pad + 14.0), 24.0, accent_2());
    l.ui.text_in("Depot", Rect::new(x + 38.0, r.y + pad, w - 38.0, 28.0), kit::HEAD, Weight::Bold, TEXT, Align::Left);
    let mut y = r.y + pad + 44.0;
    let places = depot::places(c);
    let held = logic::buses(c);
    let t = omsi_ui::tr("%{n} of %{all} places used").replace("%{n}", &held.to_string()).replace("%{all}", &places.to_string());
    l.ui.text_as_is(&c.depot, Vec2::new(x, y + 16.0), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Left);
    y += 30.0;
    l.ui.text_in(&t, Rect::new(x, y, w, 30.0), 20.0, Weight::Bold, TEXT, Align::Left);
    y += 38.0;
    let frac = if places == 0 { 1.0 } else { (held as f32 / places as f32).clamp(0.0, 1.0) };
    let bar = Rect::new(x, y, w, 10.0);
    l.ui.p().rounded(bar, 5.0, FIELD);
    l.ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * frac).max(10.0), bar.h), 5.0, if frac >= 1.0 { DANGER } else if frac >= 0.8 { WARN } else { OK });
    y += 26.0;
    // building now, or the next size and its price
    let building = c.site.works_on(Area::Parking);
    let next = depot::next_cost(c, Area::Parking);
    let label = "Buy a bigger depot";
    let bw = kit::Foot::width(&l.ui, label, Some("garage")).min(w);
    let b = Rect::new(x, r.bottom() - pad - 42.0, bw, 42.0);
    if let Some(wk) = building {
        let t = omsi_ui::tr("Being built: %{n} more places from %{date}.").replace("%{n}", &depot::SPACES_PER_LEVEL.to_string()).replace("%{date}", &super::super::day_label(&wk.until));
        l.ui.paragraph(&t, Vec2::new(x, y), w, kit::NOTE + 0.5, Weight::Medium, accent_2());
        return;
    }
    match next {
        Some((cost, days)) => {
            let t = omsi_ui::tr("A bigger depot: %{n} more places for %{amount}, ready in %{days} days.").replace("%{n}", &depot::SPACES_PER_LEVEL.to_string()).replace("%{amount}", &eur(cost)).replace("%{days}", &days.to_string());
            l.ui.paragraph(&t, Vec2::new(x, y), w, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT);
            if l.ui.button("company-casual-depot", b, label, Some("garage"), if frac >= 0.8 { ButtonKind::Primary } else { ButtonKind::Normal }) && act(l, |c| depot::build(c, Area::Parking)).is_some() {
                l.state.set_status(omsi_ui::tr("The depot grows: %{n} more places in %{days} days.").replace("%{n}", &depot::SPACES_PER_LEVEL.to_string()).replace("%{days}", &days.to_string()), false);
            }
        }
        None => {
            l.ui.paragraph("Your depot is as big as it gets.", Vec2::new(x, y), w, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT);
        }
    }
}

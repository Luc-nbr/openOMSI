//! The company's lines: those it runs, with today's tours and how they are covered (a bus for
//! each tour, a driver for each duty), and the lines it can add - the map's timetable lines
//! and the player's own from the line editor (the switch of `ownlines`).

use super::super::ownlines;
use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::{act, line_plate, meter, plate, section, Confirm, Dialog};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

#[derive(Default)]
pub struct LinesView {
    selected: Option<String>,
    mine: bool,
}

fn hhmm(minutes: i32) -> String {
    super::super::state::hhmm(minutes as f64 * 60.0)
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let gap = 16.0;
    // (what a line earns, under both columns)
    let r = co::economy::rules(c.difficulty);
    let pay = co::economy::compensation_per_km(&r, c.reputation, c.contract_index);
    let foot = omsi_ui::tr("A passenger pays %{fare} on average; the authority pays %{km} for every kilometre run, more the better your reputation.").replace("%{fare}", &super::eur_cents(r.fare as f64)).replace("%{km}", &super::eur_cents(pay));
    l.ui.text_in(&foot, Rect::new(area.x, area.bottom() - 22.0, area.w, 22.0), 12.0, Weight::Regular, TEXT_FAINT, Align::Left);
    let area = Rect::new(area.x, area.y, area.w, (area.h - 32.0).max(0.0));
    let left_w = ((area.w - gap) * 0.5).max(300.0);
    ours(l, Rect::new(area.x, area.y, left_w, area.h), &c);
    let right = Rect::new(area.x + left_w + gap, area.y, area.w - left_w - gap, area.h);
    match l.company.lines.selected.clone().filter(|s| c.lines.iter().any(|x| &x.name == s)) {
        Some(name) => tours_of(l, right, &c, &name),
        None => to_add(l, right, &c),
    }
}

/// The lines the company runs.
fn ours(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "The company's lines");
    if c.lines.is_empty() {
        l.ui.paragraph("The company runs no line yet. Add one of the map's lines, or one of your own from the line editor: every tour it has on a day is then the company's to run.", Vec2::new(inner.x, inner.y), inner.w, 13.5, Weight::Regular, TEXT_DIM);
        return;
    }
    let plan = l.company.plan.clone();
    let list = c.lines.clone();
    let selected = l.company.lines.selected.clone();
    let mut pick = None;
    let mut remove = None;
    l.ui.scroll_area("company-lines", inner, &mut |ui, v| {
        let rh = 62.0;
        for (k, line) in list.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 10.0, rh - 6.0);
            if !ui.rect_visible(r) {
                continue;
            }
            let on = selected.as_deref() == Some(line.name.as_str());
            if ui.row(&format!("company-line-{}", line.name), r, on) {
                pick = Some(line.name.clone());
            }
            let w = line_plate(ui, Vec2::new(r.x + 10.0, r.y + 10.0), line, 24.0);
            let ink = if on { on_accent() } else { TEXT };
            let caption = if line.caption.is_empty() { line.name.clone() } else { line.caption.clone() };
            ui.text_in(&caption, Rect::new(r.x + w + 22.0, r.y + 8.0, r.w - w - 80.0, 20.0), 13.5, Weight::Bold, ink, Align::Left);
            let (covered, all) = plan.as_ref().map(|p| p.coverage_of(&line.name)).unwrap_or((0, line.tours as usize));
            let sub = omsi_ui::tr("%{c} of %{t} tours covered today  ·  %{km} km a day").replace("%{c}", &covered.to_string()).replace("%{t}", &all.to_string()).replace("%{km}", &format!("{:.0}", line.km));
            ui.text_in(&sub, Rect::new(r.x + 10.0, r.y + 36.0, r.w * 0.62, 16.0), 11.5, Weight::Regular, if on { on_accent() } else { TEXT_DIM }, Align::Left);
            if all > 0 {
                meter(ui, Rect::new(r.x + r.w * 0.66, r.y + 42.0, r.w * 0.26, 5.0), covered as f64 / all as f64, if covered == all { OK } else { WARN });
            }
            if line.own {
                ui.badge(Vec2::new(r.right() - 90.0, r.y + 12.0), &omsi_ui::tr("own").to_uppercase(), accent_2());
            }
            if ui.icon_button(&format!("company-line-remove-{}", line.name), Vec2::new(r.right() - 22.0, r.y + 20.0), 14.0, "close", "Stop running this line") {
                remove = Some(line.name.clone());
            }
        }
        list.len() as f32 * rh
    });
    if let Some(name) = remove {
        l.company.dialog = Some(Dialog::Confirm { what: Confirm::RemoveLine(name) });
    } else if let Some(name) = pick {
        l.company.lines.selected = if selected.as_deref() == Some(name.as_str()) { None } else { Some(name) };
    }
}

/// A line's tours today: the bus, the drivers, and whether it runs.
fn tours_of(l: &mut Launcher, r: Rect, c: &Company, name: &str) {
    let Some(line) = c.lines.iter().find(|x| x.name == name).cloned() else { return };
    let title = omsi_ui::tr("Line %{n} today").replace("%{n}", &line.number);
    let inner = section(&mut l.ui, r, &title);
    if l.ui.button("company-line-back", Rect::new(r.right() - 150.0, r.y + 6.0, 140.0, 28.0), "Add lines", Some("add"), ButtonKind::Ghost) {
        l.company.lines.selected = None;
    }
    let Some(plan) = l.company.plan.clone() else {
        l.ui.text_in("Reading the timetable…", Rect::new(inner.x, inner.y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    };
    let tours: Vec<co::day::TourPlan> = plan.tours.into_iter().filter(|t| t.tour.line.eq_ignore_ascii_case(name)).collect();
    if tours.is_empty() {
        l.ui.paragraph("The line has no tour on this day (its timetable runs on other days).", Vec2::new(inner.x, inner.y), inner.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let fleet: Vec<(u32, String)> = c.fleet.iter().map(|v| (v.id, v.number.clone())).collect();
    let staff: Vec<(u32, String)> = c.staff.iter().map(|e| (e.id, e.name.clone())).collect();
    l.ui.scroll_area("company-line-tours", inner, &mut |ui, v| {
        let rh = 50.0;
        for (k, t) in tours.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 10.0, rh - 4.0);
            if !ui.rect_visible(r) {
                continue;
            }
            ui.p().rect(Rect::new(r.x, r.bottom(), r.w, 1.0), HAIRLINE);
            let head = format!("{} {}", omsi_ui::tr("Tour"), t.tour.tour);
            ui.text_in(&head, Rect::new(r.x, r.y + 4.0, 110.0, 20.0), 13.5, Weight::Bold, TEXT, Align::Left);
            let when = format!("{} – {}  ·  {:.0} km", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km());
            ui.text_in(&when, Rect::new(r.x, r.y + 24.0, 200.0, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            let bus = t.bus.and_then(|b| fleet.iter().find(|f| f.0 == b)).map(|f| omsi_ui::tr("Bus %{n}").replace("%{n}", &f.1));
            let drivers: Vec<String> = t.duties.iter().filter_map(|d| d.driver.and_then(|id| staff.iter().find(|s| s.0 == id)).map(|s| s.1.clone())).collect();
            let (what, colour) = if t.by_player {
                (omsi_ui::tr("Driven by you").into_owned(), accent_2())
            } else if t.live {
                (omsi_ui::tr("Reported by the game").into_owned(), accent_2())
            } else if t.bus.is_none() {
                (omsi_ui::tr("No bus: dropped").into_owned(), DANGER.lighten(0.25))
            } else if drivers.len() < t.duties.len() {
                (omsi_ui::tr("%{n} duties without a driver").replace("%{n}", &(t.duties.len() - drivers.len()).to_string()), WARN)
            } else {
                (omsi_ui::tr("Covered").into_owned(), OK)
            };
            let x = r.x + 210.0;
            let mut who = bus.unwrap_or_default();
            if !drivers.is_empty() {
                if !who.is_empty() {
                    who.push_str("  ·  ");
                }
                who.push_str(&drivers.join(", "));
            }
            ui.text_in(&who, Rect::new(x, r.y + 4.0, r.right() - x, 20.0), 12.5, Weight::Medium, TEXT_SOFT, Align::Left);
            ui.text_in(&what, Rect::new(x, r.y + 24.0, r.right() - x, 16.0), 12.0, Weight::Medium, colour, Align::Left);
        }
        tours.len() as f32 * rh
    });
}

/// The lines that can be added: the map's or the player's own.
fn to_add(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Add a line");
    let Some(today) = l.company.today.as_ref() else {
        l.ui.text_in("Reading the timetable…", Rect::new(inner.x, inner.y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    };
    if let Some(e) = &today.error {
        l.ui.paragraph(e, Vec2::new(inner.x, inner.y), inner.w, 13.0, Weight::Regular, WARN);
        return;
    }
    let all: Vec<core::LineInfo> = today.lines.clone();
    let own = l.company.own.clone();
    let (maps, mine) = ownlines::split(&all, &own);
    let showing_mine = ownlines::showing_mine(l.company.lines.mine, mine.len());
    match ownlines::switch(&mut l.ui, "company-lines-switch", Rect::new(inner.x, inner.y, inner.w.min(420.0), ROW), l.company.lines.mine, (maps.len(), mine.len())) {
        ownlines::Switched::To(m) => l.company.lines.mine = m,
        ownlines::Switched::Hint => l.state.set_status(omsi_ui::tr(ownlines::NONE_YET).into_owned(), false),
        ownlines::Switched::No => {}
    }
    let list: Vec<core::LineInfo> = if showing_mine { mine.into_iter().cloned().collect() } else { maps.into_iter().cloned().collect() };
    let list: Vec<core::LineInfo> = list.into_iter().filter(|x| !c.lines.iter().any(|y| y.name.eq_ignore_ascii_case(&x.name))).collect();
    let rows = Rect::new(inner.x, inner.y + ROW + 12.0, inner.w, (inner.h - ROW - 12.0).max(0.0));
    if list.is_empty() {
        l.ui.text_in("The company runs all of them already.", Rect::new(rows.x, rows.y, rows.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let mut add = None;
    l.ui.scroll_area("company-lines-add", rows, &mut |ui, v| {
        let rh = 54.0;
        for (k, line) in list.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 10.0, rh - 6.0);
            if !ui.rect_visible(r) {
                continue;
            }
            ui.row(&format!("company-add-row-{}", line.name), r, false);
            let o = core::lines::own_line_of(&line.name, &own);
            let w = match &o {
                Some(o) => ownlines::plate(ui, Vec2::new(r.x + 10.0, r.y + 12.0), &o.number, &o.colour, 22.0),
                None => {
                    let number = line.tours.iter().flat_map(|t| t.trips.iter()).map(|t| t.line.trim()).find(|n| !n.is_empty()).unwrap_or(&line.name).to_string();
                    plate(ui, Vec2::new(r.x + 10.0, r.y + 12.0), &number, 22.0)
                }
            };
            let caption = match &o {
                Some(o) => ownlines::caption_of(o, line),
                None => format!("{}  ·  {}", line.name, line.termini.join(" – ")),
            };
            ui.text_in(&caption, Rect::new(r.x + w + 22.0, r.y + 4.0, r.w - w - 150.0, 22.0), 13.0, Weight::Bold, TEXT, Align::Left);
            let runs = line.tours.iter().filter(|t| t.runs).count();
            let km: f64 = line.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()).map(|t| t.km).sum();
            let sub = omsi_ui::tr("%{n} tours today  ·  %{km} km").replace("%{n}", &runs.to_string()).replace("%{km}", &format!("{km:.0}"));
            ui.text_in(&sub, Rect::new(r.x + w + 22.0, r.y + 24.0, r.w - w - 150.0, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            if ui.button(&format!("company-add-{}", line.name), Rect::new(r.right() - 110.0, r.y + 8.0, 100.0, 32.0), "Add", Some("add"), ButtonKind::Normal) {
                add = Some(k);
            }
        }
        list.len() as f32 * rh
    });
    if let Some(k) = add {
        let line = list[k].clone();
        let o = core::lines::own_line_of(&line.name, &own);
        if act(l, |c| co::network::add_line(c, &line, o.as_ref())).is_some() {
            let n = l.company.company.as_ref().and_then(|c| c.lines.last()).map(|x| x.number.clone()).unwrap_or_default();
            l.state.set_status(omsi_ui::tr("The company runs line %{n} from today.").replace("%{n}", &n), false);
        }
    }
}

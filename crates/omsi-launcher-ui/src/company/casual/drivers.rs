//! My drivers: who drives for the company and how well - a face, a name, a skill in stars and
//! today's shift - for the feel of it, without rules to keep. The company hires the drivers its
//! shifts need by itself at night ("Hire automatically" does it now); the player may give a
//! bonus or let someone go.

use super::super::super::theme::*;
use super::super::super::ui::{ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::people::portraits;
use super::super::{act, day_label, eur, kit, Confirm, Dialog};
use super::logic;
use glam::Vec2;
use omsi_launcher_lib::company::{self as co, Company, Employee, Role};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::collections::HashMap;

const GAP: f32 = 16.0;
const CARD_H: f32 = 168.0;

/// The drivers its shifts ask for now, beyond those it has: today's shifts without a driver, or
/// what the company counted it needs.
pub(super) fn wanted(c: &Company, plan: Option<&co::day::Plan>) -> usize {
    let short = plan.map(|p| p.short_of().1).unwrap_or(0);
    let have = co::staff::staying(c, Role::Driver, &c.date);
    short.max((c.autopilot.drivers as usize).saturating_sub(have))
}

/// "Hire automatically": the drivers wanted hired now, the best of the week's applicants.
pub(super) fn hire_now(l: &mut Launcher) {
    let Some(c) = l.company.company.as_ref() else { return };
    let n = wanted(c, l.company.plan.as_ref()).min(co::autopilot::DRIVERS_A_NIGHT * 2);
    if n == 0 {
        l.state.set_status(omsi_ui::tr("You have the drivers your shifts need: the company hires more by itself when a line asks for them.").into_owned(), false);
        return;
    }
    let names = act(l, |c| {
        let mut out = Vec::new();
        for _ in 0..n {
            match co::staff::hire_best(c, Role::Driver) {
                Some(name) => out.push(name),
                None => break,
            }
        }
        Ok(out)
    })
    .unwrap_or_default();
    if names.is_empty() {
        l.state.set_status(omsi_ui::tr("Nobody could be hired now: no applicant this week, or the cash is short. The company tries again tonight.").into_owned(), true);
    } else {
        l.state.set_status(omsi_ui::tr("Hired: %{names}.").replace("%{names}", &names.join(", ")), false);
    }
}

/// What a driver does today, in a few words, and its colour.
fn today_of(c: &Company, plan: Option<&co::day::Plan>, e: &Employee) -> (String, Color) {
    if let Some(u) = &e.notice_until {
        return (omsi_ui::tr("Leaves on %{date}").replace("%{date}", &day_label(u)), WARN);
    }
    if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
        return (omsi_ui::tr("Ill today").into_owned(), WARN);
    }
    if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
        return (omsi_ui::tr("On holiday").into_owned(), TEXT_DIM);
    }
    if e.training_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
        return (omsi_ui::tr("On a course today").into_owned(), TEXT_DIM);
    }
    match plan.and_then(|p| logic::shift_of(p, e.id)) {
        Some((n, from, to)) => (omsi_ui::tr("Line %{n} · %{from} – %{to}").replace("%{n}", &n).replace("%{from}", &co::clock::hhmm(from as i64)).replace("%{to}", &co::clock::hhmm(to as i64)), OK),
        None => (omsi_ui::tr("Day off").into_owned(), TEXT_DIM),
    }
}

/// The stars of a skill (1 to 5).
fn stars(ui: &mut Ui, at: Vec2, n: u8) {
    for k in 0..5u8 {
        let on = k < n;
        ui.icon("star", Vec2::new(at.x + 9.0 + k as f32 * 19.0, at.y), 17.0, if on { WARN } else { HAIRLINE.alpha(1.0) });
    }
}

/// What a click on a driver's card asks for.
enum Ask {
    Bonus(u32),
    Fire(u32),
}

/// The drivers page.
pub(super) fn page(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let list: Vec<Employee> = logic::drivers(&c).into_iter().cloned().collect();
    let plan = l.company.plan.clone();
    let want = wanted(&c, plan.as_ref());
    let sub = if want > 0 { omsi_ui::tr("%{n} drivers · %{m} more wanted: hired tonight, or now").replace("%{n}", &list.len().to_string()).replace("%{m}", &want.to_string()) } else { omsi_ui::tr("%{n} drivers · every shift has its driver").replace("%{n}", &list.len().to_string()) };
    let (body, hire) = super::page_head(l, area, super::DRIVERS, &sub, Some(("Hire automatically", "person")));
    if hire {
        hire_now(l);
        return;
    }
    if list.is_empty() {
        if super::empty(l, body, "groups", "No driver yet", "The company hires the drivers its shifts need by itself, at night. Or hire them now.", Some(("Hire automatically", "person"))) {
            hire_now(l);
        }
        return;
    }
    let faces: HashMap<u32, usize> = portraits::textures(l, list.iter().map(|e| e.portrait()));
    let phone = super::super::super::mobile::mobile() || body.w < 640.0;
    let cols = if phone { 1 } else if body.w >= 1100.0 { 3 } else { 2 };
    let cw = ((body.w - 12.0 - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut ask: Option<Ask> = None;
    super::super::super::tour::anchor("company-casual-drivers", body);
    l.ui.scroll_area("company-casual-drivers", body, &mut |ui, v| {
        for (k, e) in list.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (CARD_H + GAP), cw, CARD_H);
            if !ui.rect_visible(r) {
                continue;
            }
            super::card_ground(ui, r, 0.0, None);
            let pad = 16.0;
            let face = Rect::new(r.x + pad, r.y + pad, 76.0, 76.0);
            portraits::draw(ui, face, faces.get(&e.portrait()).copied(), &e.name);
            let tx = face.right() + 14.0;
            let tw = r.right() - pad - tx;
            ui.text_as_is(&e.name, Vec2::new(tx, r.y + pad + 16.0), 16.5, Weight::Bold, TEXT, Align::Left);
            stars(ui, Vec2::new(tx - 2.0, r.y + pad + 38.0), logic::stars(e));
            let (doing, dc) = today_of(&c, plan.as_ref(), e);
            ui.icon("schedule", Vec2::new(tx + 8.0, r.y + pad + 64.0), 15.0, dc);
            ui.text_in(&doing, Rect::new(tx + 22.0, r.y + pad + 53.0, (tw - 22.0).max(0.0), 22.0), kit::NOTE, Weight::Medium, dc, Align::Left);
            // the two buttons
            let by = r.bottom() - pad - 36.0;
            let leaving = e.notice_until.is_some();
            let bonus = omsi_ui::tr("Bonus %{amount}").replace("%{amount}", &eur(co::staff::bonus_cost(e)));
            let bw = ((r.w - 2.0 * pad - 10.0) * 0.5).floor();
            if !leaving {
                if ui.button(&format!("company-casual-bonus-{}", e.id), Rect::new(r.x + pad, by, bw, 36.0), &bonus, Some("stars"), ButtonKind::Normal) {
                    ask = Some(Ask::Bonus(e.id));
                }
                ui.tooltip(Rect::new(r.x + pad, by, bw, 36.0), "A one-off bonus: they are happier and stay longer");
                if ui.button(&format!("company-casual-fire-{}", e.id), Rect::new(r.x + pad + bw + 10.0, by, bw, 36.0), "Fire", Some("logout"), ButtonKind::Ghost) {
                    ask = Some(Ask::Fire(e.id));
                }
            }
        }
        list.len().div_ceil(cols) as f32 * (CARD_H + GAP)
    });
    match ask {
        Some(Ask::Bonus(id)) => {
            let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
            if let Some(cost) = act(l, |c| co::staff::bonus(c, id)) {
                l.state.set_status(omsi_ui::tr("%{name} got a bonus of %{amount}: thank you!").replace("%{name}", &name).replace("%{amount}", &eur(cost)), false);
            }
        }
        Some(Ask::Fire(id)) => l.company.dialog = Some(Dialog::Confirm { what: Confirm::Dismiss(id) }),
        None => {}
    }
}

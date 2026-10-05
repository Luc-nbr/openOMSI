//! The staff and the labour market: who works for the company (experience, licence, wage,
//! reliability, satisfaction and where they are today), a raise or a dismissal with notice;
//! and the week's applicants with what they ask.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::kit::{self, Foot};
use super::{act, day_label, eur, grade, meter, Confirm, Dialog};
use glam::Vec2;
use omsi_launcher_lib::company::staff::{self, Applicant};
use omsi_launcher_lib::company::{self as co, Company, Employee, Licence, Skills};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

#[derive(Default)]
pub struct PeopleView {
    tab: usize,
}

/// The applicants' list (a popup's "Hire drivers").
pub(super) fn to_applicants(l: &mut Launcher) {
    l.company.people.tab = 1;
}

/// The columns: name, experience, licence, wage, reliability, satisfaction or skills, status,
/// actions - as shares of the width.
const COLS: [f32; 8] = [0.20, 0.13, 0.07, 0.11, 0.09, 0.13, 0.15, 0.12];

fn cols(r: Rect) -> Vec<Rect> {
    let mut x = r.x;
    COLS.iter()
        .map(|w| {
            let c = Rect::new(x, r.y, r.w * w, r.h);
            x += r.w * w;
            c
        })
        .collect()
}

fn licence_text(l: Licence) -> &'static str {
    match l {
        Licence::D => "D",
        Licence::D1 => "D1",
    }
}

/// Where someone is today, and its colour.
fn status_of(c: &Company, e: &Employee, working: bool) -> (String, Color) {
    if let Some(u) = e.sick_until.as_deref().filter(|u| co::dates::between(&c.date, u) >= 0) {
        return (omsi_ui::tr("Ill until %{date}").replace("%{date}", &day_label(u)), WARN);
    }
    if let Some(u) = e.holiday_until.as_deref().filter(|u| co::dates::between(&c.date, u) >= 0) {
        return (omsi_ui::tr("On holiday until %{date}").replace("%{date}", &day_label(u)), TEXT_SOFT);
    }
    if let Some(u) = e.notice_until.as_deref() {
        return (omsi_ui::tr("Leaves after %{date}").replace("%{date}", &day_label(u)), DANGER.lighten(0.25));
    }
    if working {
        (omsi_ui::tr("Drives today").into_owned(), OK)
    } else if e.week_days >= staff::WEEK_DAYS {
        (omsi_ui::tr("Days off").into_owned(), TEXT_SOFT)
    } else {
        (omsi_ui::tr("Free today").into_owned(), TEXT_SOFT)
    }
}

fn head(ui: &mut Ui, r: Rect, labels: &[(&str, &str)]) {
    for (c, (t, tip)) in cols(r).iter().zip(labels) {
        ui.text_in(&omsi_ui::tr(t).to_uppercase(), Rect::new(c.x, c.y, c.w - 8.0, c.h), kit::CAPS, Weight::Bold, TEXT_DIM, Align::Left);
        if !tip.is_empty() {
            ui.tooltip(Rect::new(c.x, c.y, c.w - 8.0, c.h), tip);
        }
    }
    ui.p().rect(Rect::new(r.x, r.bottom(), r.w, 1.0), HAIRLINE);
}

fn skills_text(s: &Skills) -> String {
    format!("{:.0} · {:.0} · {:.0}", s.driving, s.punctuality, s.service)
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let market = staff::applicants(&c);
    let labels = [omsi_ui::tr("Employees (%{n})").replace("%{n}", &c.staff.len().to_string()), omsi_ui::tr("Applicants (%{n})").replace("%{n}", &market.len().to_string())];
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut tab = l.company.people.tab;
    if l.ui.segmented("company-staff-tabs", Rect::new(area.x, area.y, 460.0f32.min(area.w), 40.0), &mut tab, &refs) {
        l.company.people.tab = tab;
    }
    // what the staff costs and what today asks
    let wages: i64 = c.staff.iter().map(staff::monthly_cost).sum();
    let duties: usize = l.company.plan.as_ref().map(|p| p.tours.iter().filter(|t| !t.by_player && !t.live && !t.tour.unplanned).map(|t| t.duties.len()).sum()).unwrap_or(0);
    let info = omsi_ui::tr("Wages %{amount} a month with the employer's share  ·  %{n} duties today").replace("%{amount}", &eur(wages)).replace("%{n}", &duties.to_string());
    l.ui.text_in(&info, Rect::new(area.x + 480.0, area.y, area.w - 480.0, 40.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Right);
    let body = Rect::new(area.x, area.y + 40.0 + 18.0, area.w, (area.h - 40.0 - 18.0 - 44.0).max(0.0));
    let foot = Rect::new(area.x, area.bottom() - 30.0, area.w, 30.0);
    if l.company.people.tab == 1 {
        applicants(l, body, &c, market);
        let t = omsi_ui::tr("New applicants come on %{date}. Skills: driving · punctuality · service; a driver of little experience is a warning on an articulated bus or a double-decker.").replace("%{date}", &day_label(&co::network::next_monday(&c)));
        l.ui.text_in(&t, foot, kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    } else {
        employees(l, body, &c);
        l.ui.text_in("A driver works five days a week and has holidays and ill days: count about one and a half drivers for every duty of the day.", foot, kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    }
}

fn employees(l: &mut Launcher, area: Rect, c: &Company) {
    if c.staff.is_empty() {
        let h = l.ui.paragraph("Nobody works here yet. Every duty of a tour needs a driver: hire some among the week's applicants.", Vec2::new(area.x, area.y + 4.0), area.w.min(860.0), kit::BODY, Weight::Regular, TEXT_SOFT);
        let bw = Foot::width(&l.ui, "To the applicants", Some("groups"));
        if l.ui.button("company-to-applicants", Rect::new(area.x, area.y + h + 18.0, bw, 40.0), "To the applicants", Some("groups"), ButtonKind::Primary) {
            l.company.people.tab = 1;
        }
        return;
    }
    head(
        &mut l.ui,
        Rect::new(area.x + 8.0, area.y, area.w - 20.0, 20.0),
        &[("Name", ""), ("Experience", "0 - 100: grows with every day driven"), ("Licence", "D: every bus; D1: midibuses only"), ("Wage", "A month, before the employer's share"), ("Reliable", "How seldom they fall ill or come late"), ("Satisfaction", "Unhappy people leave: pay and hours count"), ("Today", ""), ("", "")],
    );
    let working: Vec<u32> = l.company.plan.as_ref().map(|p| p.tours.iter().filter(|t| !t.tour.unplanned).flat_map(|t| t.duties.iter()).filter_map(|d| d.driver).collect()).unwrap_or_default();
    let list = c.staff.clone();
    let mut action: Option<(u32, u8)> = None;
    let rows = Rect::new(area.x, area.y + 30.0, area.w, (area.h - 30.0).max(0.0));
    l.ui.scroll_area("company-staff", rows, &mut |ui, v| {
        let rh = 56.0;
        for (k, e) in list.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 12.0, rh - 4.0);
            if !ui.rect_visible(r) {
                continue;
            }
            ui.row(&format!("company-staff-row-{}", e.id), r, false);
            let cs = cols(Rect::new(r.x + 8.0, r.y, r.w - 8.0, r.h));
            ui.text_in(&e.name, Rect::new(cs[0].x, r.y + 5.0, cs[0].w - 8.0, 24.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
            let since = omsi_ui::tr("%{age}, since %{date}").replace("%{age}", &e.age.to_string()).replace("%{date}", &day_label(&e.hired));
            ui.text_in(&since, Rect::new(cs[0].x, r.y + 29.0, cs[0].w - 8.0, 20.0), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
            ui.text_in(&format!("{:.0}", e.experience), Rect::new(cs[1].x, r.y, 34.0, r.h), kit::ROWS, Weight::Medium, TEXT, Align::Left);
            meter(ui, Rect::new(cs[1].x + 38.0, r.center().y - 3.0, cs[1].w - 54.0, 6.0), e.experience / 100.0, EARLY_SOFT);
            ui.text_in(licence_text(e.licence), cs[2], kit::ROWS, Weight::Medium, TEXT, Align::Left);
            ui.text_in(&eur(e.wage), cs[3], kit::ROWS, Weight::Medium, TEXT, Align::Left);
            ui.text_in(&format!("{:.0} %", e.reliability * 100.0), cs[4], kit::ROWS, Weight::Regular, TEXT, Align::Left);
            ui.text_in(&format!("{:.0}", e.satisfaction), Rect::new(cs[5].x, r.y, 34.0, r.h), kit::ROWS, Weight::Medium, grade(e.satisfaction), Align::Left);
            meter(ui, Rect::new(cs[5].x + 38.0, r.center().y - 3.0, cs[5].w - 54.0, 6.0), e.satisfaction / 100.0, grade(e.satisfaction));
            let (status, colour) = status_of(c, e, working.contains(&e.id));
            ui.text_in(&status, Rect::new(cs[6].x, r.y, cs[6].w - 8.0, r.h), kit::NOTE + 0.5, Weight::Medium, colour, Align::Left);
            let a = cs[7];
            if ui.icon_button(&format!("company-raise-{}", e.id), Vec2::new(a.x + 18.0, r.center().y), 17.0, "trending_up", "A raise of 5 %") {
                action = Some((e.id, 0));
            }
            if e.notice_until.is_some() {
                if !e.resigned && ui.icon_button(&format!("company-keep-{}", e.id), Vec2::new(a.x + 58.0, r.center().y), 17.0, "restart_alt", "Take the dismissal back") {
                    action = Some((e.id, 2));
                }
            } else if ui.icon_button(&format!("company-dismiss-{}", e.id), Vec2::new(a.x + 58.0, r.center().y), 17.0, "logout", "Dismiss: they work their notice and leave") {
                action = Some((e.id, 1));
            }
        }
        list.len() as f32 * rh
    });
    match action {
        Some((id, 0)) => {
            act(l, |c| {
                staff::raise(c, id, 0.05);
                Ok(())
            });
        }
        Some((id, 1)) => l.company.dialog = Some(Dialog::Confirm { what: Confirm::Dismiss(id) }),
        Some((id, _)) => {
            act(l, |c| staff::withdraw_notice(c, id));
        }
        None => {}
    }
}

fn applicants(l: &mut Launcher, area: Rect, c: &Company, market: Vec<Applicant>) {
    if market.is_empty() {
        l.ui.text_in("Nobody else applies this week.", Rect::new(area.x, area.y, area.w, 26.0), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Left);
        return;
    }
    head(
        &mut l.ui,
        Rect::new(area.x + 8.0, area.y, area.w - 20.0, 20.0),
        &[("Name", ""), ("Experience", "0 - 100: grows with every day driven"), ("Licence", "D: every bus; D1: midibuses only"), ("Asks", "The wage a month they ask"), ("Reliable", "How seldom they fall ill or come late"), ("Skills", "Driving · punctuality · service"), ("Costs a month", "The wage with the employer's share"), ("", "")],
    );
    let mut hire: Option<usize> = None;
    let rows = Rect::new(area.x, area.y + 30.0, area.w, (area.h - 30.0).max(0.0));
    l.ui.scroll_area("company-applicants", rows, &mut |ui, v| {
        let rh = 56.0;
        for (k, a) in market.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 12.0, rh - 4.0);
            if !ui.rect_visible(r) {
                continue;
            }
            ui.row(&format!("company-applicant-row-{}", a.no), r, false);
            let cs = cols(Rect::new(r.x + 8.0, r.y, r.w - 8.0, r.h));
            ui.text_in(&a.name, Rect::new(cs[0].x, r.y + 5.0, cs[0].w - 8.0, 24.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
            ui.text_in(&omsi_ui::tr("%{n} years old").replace("%{n}", &a.age.to_string()), Rect::new(cs[0].x, r.y + 29.0, cs[0].w - 8.0, 20.0), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
            ui.text_in(&format!("{:.0}", a.experience), Rect::new(cs[1].x, r.y, 34.0, r.h), kit::ROWS, Weight::Medium, TEXT, Align::Left);
            meter(ui, Rect::new(cs[1].x + 38.0, r.center().y - 3.0, cs[1].w - 54.0, 6.0), a.experience / 100.0, EARLY_SOFT);
            let lic = licence_text(a.licence);
            ui.text_in(lic, cs[2], kit::ROWS, Weight::Medium, if a.licence == Licence::D1 { WARN } else { TEXT }, Align::Left);
            if a.licence == Licence::D1 {
                ui.tooltip(cs[2], "D1: only midibuses");
            }
            ui.text_in(&eur(a.wage), cs[3], kit::ROWS, Weight::Medium, TEXT, Align::Left);
            ui.text_in(&format!("{:.0} %", a.reliability * 100.0), cs[4], kit::ROWS, Weight::Regular, TEXT, Align::Left);
            ui.text_in(&skills_text(&a.skills), cs[5], kit::ROWS, Weight::Regular, TEXT, Align::Left);
            ui.text_in(&eur(co::economy::employer_cost(a.wage)), cs[6], kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
            if ui.button(&format!("company-hire-{}", a.no), Rect::new(cs[7].x, r.y + 7.0, cs[7].w.min(120.0), r.h - 14.0), "Hire", None, ButtonKind::Normal) {
                hire = Some(k);
            }
        }
        market.len() as f32 * rh
    });
    if let Some(k) = hire {
        let a = market[k].clone();
        if act(l, |c| staff::hire(c, &a)).is_some() {
            l.state.set_status(omsi_ui::tr("%{name} works for %{company} from today.").replace("%{name}", &a.name).replace("%{company}", &c.name), false);
        }
    }
}

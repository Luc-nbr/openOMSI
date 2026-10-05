//! The concession market: the map's lines the authority puts out to tender, with their week
//! (tours, kilometres, what they bring in), the company's bids and the results, and the
//! concessions it holds with their terms (the rules are
//! `omsi_launcher_lib::company::concessions`'). A bid is made in a dialog: a price of the
//! authority's reference, with what it brings in a week and how it scores.
//!
//! The lines' weeks are read from the timetable of the seven days from the company's date on
//! a thread of their own (the page says so meanwhile).

use super::super::theme::*;
use super::super::ui::{ButtonKind, Key};
use super::super::Launcher;
use super::{act, day_label, dialog_panel, eur, eur_cents, plate, section, Dialog};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::concessions::{self as cn, Outcome, Week};
use omsi_launcher_lib::company::{dates, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub struct TendersView {
    /// The lines' weeks, for the map and the first day they were read for.
    weeks: HashMap<String, Week>,
    weeks_for: Option<(String, String)>,
    reading: Option<Receiver<HashMap<String, Week>>>,
}

/// The weeks of every map line, read in the background once per map and week.
fn weeks(l: &mut Launcher, c: &Company) {
    let view = &mut l.company.tenders;
    if let Some(rx) = view.reading.as_ref() {
        if let Ok(w) = rx.try_recv() {
            view.weeks = w;
            view.reading = None;
        }
    }
    let monday = co_monday(&c.date);
    let key = (c.map.clone(), monday.clone());
    if view.weeks_for.as_ref() == Some(&key) {
        return;
    }
    view.weeks_for = Some(key);
    let (tx, rx) = channel();
    view.reading = Some(rx);
    let map = c.map.clone();
    let _ = std::thread::Builder::new().name("company weeks".into()).spawn(move || {
        let days: Vec<Vec<core::LineInfo>> = (0..7).filter_map(|k| core::list_lines(&map, &dates::add(&monday, k)).ok()).collect();
        let mut out = HashMap::new();
        for l in days.first().map(|d| d.iter().map(|l| l.name.clone()).collect::<Vec<_>>()).unwrap_or_default() {
            out.insert(l.to_lowercase(), cn::week_of(&days, &l));
        }
        let _ = tx.send(out);
    });
}

/// The Monday of a date's week (the week is read from it).
fn co_monday(date: &str) -> String {
    let d = dates::parse(date).unwrap_or(0);
    dates::fmt(d - dates::weekday(d) as i64)
}

fn week_of(l: &Launcher, line: &str) -> Option<Week> {
    l.company.tenders.weeks.get(&line.to_lowercase()).copied()
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    weeks(l, &c);
    let gap = 16.0;
    let q = cn::quality(&c, false);
    let foot = omsi_ui::tr("The best score wins a tender: the price counts 60 %, quality 40 % - yours is %{q} of 100, from your reputation and punctuality.").replace("%{q}", &format!("{q:.0}"));
    l.ui.text_in(&foot, Rect::new(area.x, area.bottom() - 22.0, area.w, 22.0), 12.0, Weight::Regular, TEXT_FAINT, Align::Left);
    let area = Rect::new(area.x, area.y, area.w, (area.h - 32.0).max(0.0));
    let left_w = ((area.w - gap) * 0.56).max(360.0);
    tenders(l, Rect::new(area.x, area.y, left_w, area.h), &c);
    held(l, Rect::new(area.x + left_w + gap, area.y, area.w - left_w - gap, area.h), &c);
}

fn tenders(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Tenders");
    if cn::may_add_directly(c) {
        l.ui.paragraph("On an easy economy the map's lines are taken on directly on the Lines page and their concessions renew by themselves; tenders are offered all the same, for the practice.", Vec2::new(inner.x, inner.y), inner.w, 12.5, Weight::Regular, TEXT_DIM);
    }
    let top = if cn::may_add_directly(c) { 48.0 } else { 0.0 };
    let rows = Rect::new(inner.x, inner.y + top, inner.w, (inner.h - top).max(0.0));
    let mut list: Vec<cn::Tender> = c.concessions.tenders.clone();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(a.closes.cmp(&b.closes)));
    if list.is_empty() {
        let t = if l.company.today.is_none() { "Reading the timetable…" } else { "No line is out to tender now. A new round comes every four weeks; a line of the map can also be applied for on the Lines page." };
        l.ui.paragraph(t, Vec2::new(rows.x, rows.y), rows.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let weeks: Vec<Option<Week>> = list.iter().map(|t| t.week.or_else(|| week_of(l, &t.line))).collect();
    let reading = l.company.tenders.reading.is_some();
    let cc = c.clone();
    let mut open_bid: Option<(u32, f32)> = None;
    l.ui.scroll_area("company-tenders", rows, &mut |ui, v| {
        let rh = 76.0;
        for (k, t) in list.iter().enumerate() {
            let r = Rect::new(v.x, v.y + k as f32 * rh, v.w - 10.0, rh - 6.0);
            if !ui.rect_visible(r) {
                continue;
            }
            if k > 0 {
                ui.p().rect(Rect::new(r.x, r.y - 3.0, r.w, 1.0), HAIRLINE);
            }
            let dim = !t.open();
            let w = plate(ui, Vec2::new(r.x, r.y + 6.0), &t.number, 24.0);
            let caption = if t.caption.is_empty() { t.line.clone() } else { t.caption.clone() };
            ui.text_in(&caption, Rect::new(r.x + w + 12.0, r.y + 4.0, r.w - w - 190.0, 18.0), 13.5, Weight::Bold, if dim { TEXT_DIM } else { TEXT }, Align::Left);
            let figures = match weeks[k] {
                // (a timetable without distances: its trips instead)
                Some(wk) if wk.km < 1.0 => omsi_ui::tr("%{t} tours, %{n} trips a week").replace("%{t}", &wk.tours.to_string()).replace("%{n}", &wk.trips.to_string()),
                Some(wk) => omsi_ui::tr("%{t} tours, %{km} km a week  ·  about %{amount}").replace("%{t}", &wk.tours.to_string()).replace("%{km}", &format!("{:.0}", wk.km)).replace("%{amount}", &eur(cn::week_revenue(&cc, &wk, t.bid.unwrap_or(1.0)))),
                None if reading => omsi_ui::tr("Reading its week…").into_owned(),
                None => omsi_ui::tr("%{t} tours, %{km} km on the day it was offered").replace("%{t}", &t.day_tours.to_string()).replace("%{km}", &format!("{:.0}", t.day_km)),
            };
            ui.text_in(&figures, Rect::new(r.x + w + 12.0, r.y + 24.0, r.w - w - 190.0, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            let (state, colour) = match &t.outcome {
                None => {
                    let left = dates::between(&cc.date, &t.closes);
                    let when = if left <= 0 { omsi_ui::tr("closes tonight").into_owned() } else { omsi_ui::tr("closes %{date}").replace("%{date}", &day_label(&t.closes)) };
                    let bid = match t.bid {
                        Some(p) => omsi_ui::tr("your bid %{p} %").replace("%{p}", &format!("{:.0}", p * 100.0)),
                        None => omsi_ui::tr("no bid yet").into_owned(),
                    };
                    (format!("{when}  ·  {bid}  ·  {} {} {}", omsi_ui::tr("term"), t.weeks, omsi_ui::tr("weeks")), if t.bid.is_some() { accent_2() } else { TEXT_SOFT })
                }
                Some(Outcome::Won { .. }) => (omsi_ui::tr("Won").into_owned(), OK),
                Some(Outcome::Lost { winner, .. }) => (omsi_ui::tr("Lost to %{who}").replace("%{who}", winner), WARN),
                Some(Outcome::NoBid { winner }) => (omsi_ui::tr("Went to %{who} without a bid of yours").replace("%{who}", winner), TEXT_FAINT),
            };
            ui.text_in(&state, Rect::new(r.x + w + 12.0, r.y + 44.0, r.w - w - 190.0, 16.0), 11.5, Weight::Medium, colour, Align::Left);
            if t.renewal {
                ui.badge(Vec2::new(r.right() - 176.0 - 70.0, r.y + 6.0), &omsi_ui::tr("renewal").to_uppercase(), accent_2());
            }
            if t.open() {
                let label = if t.bid.is_some() { "Change the bid" } else { "Bid" };
                if ui.button(&format!("company-tender-{}", t.id), Rect::new(r.right() - 168.0, r.y + 14.0, 168.0, 34.0), label, Some("receipt_long"), if t.bid.is_some() { ButtonKind::Ghost } else { ButtonKind::Normal }) {
                    open_bid = Some((t.id, t.bid.unwrap_or(1.0) as f32));
                }
            }
        }
        list.len() as f32 * rh
    });
    if let Some((tender, price)) = open_bid {
        l.company.dialog = Some(Dialog::Bid { tender, price });
    }
}

fn held(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Your concessions");
    let held = c.concessions.held.clone();
    let own: Vec<&omsi_launcher_lib::company::CompanyLine> = c.lines.iter().filter(|x| x.own).collect();
    if held.is_empty() && own.is_empty() {
        l.ui.paragraph("The company holds no concession yet. Win a tender, or apply for a line of the map on the Lines page.", Vec2::new(inner.x, inner.y), inner.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let today = c.date.clone();
    let lic = eur((cn::LICENCE as f64 * c.price_index).round() as i64);
    let own: Vec<(String, String)> = own.iter().map(|x| (x.number.clone(), x.caption.clone())).collect();
    let per_km = cn::reference_per_km(c);
    l.ui.scroll_area("company-concessions", inner, &mut |ui, v| {
        let rh = 58.0;
        let mut y = v.y;
        for h in &held {
            let r = Rect::new(v.x, y, v.w - 10.0, rh - 6.0);
            y += rh;
            if !ui.rect_visible(r) {
                continue;
            }
            let w = plate(ui, Vec2::new(r.x, r.y + 4.0), &h.number, 22.0);
            let left = dates::between(&today, &h.until);
            let until = omsi_ui::tr("until %{date}").replace("%{date}", &day_label(&h.until));
            let colour = if left <= cn::RENEW_BEFORE { WARN } else { TEXT };
            ui.text_in(&until, Rect::new(r.x + w + 12.0, r.y + 2.0, r.w - w - 12.0, 18.0), 13.0, Weight::Bold, colour, Align::Left);
            let price = omsi_ui::tr("%{p} % of the reference: %{km} a kilometre").replace("%{p}", &format!("{:.0}", h.price * 100.0)).replace("%{km}", &eur_cents(per_km * h.price));
            ui.text_in(&price, Rect::new(r.x + w + 12.0, r.y + 22.0, r.w - w - 12.0, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            if h.direct {
                ui.badge(Vec2::new(r.right() - 80.0, r.y + 4.0), &omsi_ui::tr("direct").to_uppercase(), TEXT_DIM);
            }
        }
        for (number, caption) in &own {
            let r = Rect::new(v.x, y, v.w - 10.0, rh - 6.0);
            y += rh;
            if !ui.rect_visible(r) {
                continue;
            }
            let w = plate(ui, Vec2::new(r.x, r.y + 4.0), number, 22.0);
            ui.text_in(caption, Rect::new(r.x + w + 12.0, r.y + 2.0, r.w - w - 12.0, 18.0), 13.0, Weight::Bold, TEXT, Align::Left);
            let t = omsi_ui::tr("Your own line: no concession, a licence of %{amount} a month").replace("%{amount}", &lic);
            ui.text_in(&t, Rect::new(r.x + w + 12.0, r.y + 22.0, r.w - w - 12.0, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        }
        y - v.y
    });
}

/// The bid dialog: a price, what it brings in a week, how it scores; the fee with the first.
pub fn dialog(l: &mut Launcher) {
    let Some(c) = l.company.company.clone() else { return };
    let esc = l.ui.input.keys.contains(&Key::Escape);
    let Some(Dialog::Bid { tender, price }) = l.company.dialog.take() else { return };
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == tender).cloned() else { return };
    let title = omsi_ui::tr("Tender for line %{n}").replace("%{n}", &t.number);
    let inner = dialog_panel(l, 620.0, 470.0, "receipt_long", &title);
    let mut y = inner.y;
    let caption = if t.caption.is_empty() { t.line.clone() } else { t.caption.clone() };
    l.ui.text_in(&caption, Rect::new(inner.x, y, inner.w, 20.0), 14.0, Weight::Bold, TEXT_SOFT, Align::Left);
    y += 28.0;
    let wk = t.week.or_else(|| week_of(l, &t.line));
    let rh = 30.0;
    let row = |l: &mut Launcher, y: f32, label: &str, value: &str, strong: bool| {
        let r = Rect::new(inner.x, y, inner.w, rh);
        l.ui.text_in(label, Rect::new(r.x, r.y, r.w * 0.6, r.h), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(value, Rect::new(r.x + r.w * 0.35, r.y, r.w * 0.65, r.h), if strong { 15.0 } else { 13.0 }, if strong { Weight::Bold } else { Weight::Medium }, TEXT, Align::Right);
        l.ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), HAIRLINE);
    };
    let week_text = match wk {
        Some(w) => omsi_ui::tr("%{t} tours, %{n} trips, %{km} km").replace("%{t}", &w.tours.to_string()).replace("%{n}", &w.trips.to_string()).replace("%{km}", &format!("{:.0}", w.km)),
        None => omsi_ui::tr("%{t} tours, %{km} km a day").replace("%{t}", &t.day_tours.to_string()).replace("%{km}", &format!("{:.0}", t.day_km)),
    };
    row(l, y, &omsi_ui::tr("The line's week"), &week_text, false);
    y += rh;
    let term = omsi_ui::tr("%{n} weeks").replace("%{n}", &t.weeks.to_string());
    row(l, y, &omsi_ui::tr("Term"), &term, false);
    y += rh;
    row(l, y, &omsi_ui::tr("Bids close"), &day_label(&t.closes), false);
    y += rh + 14.0;
    let mut p = price.clamp(cn::PRICE_MIN as f32, cn::PRICE_MAX as f32);
    let fmt = |v: f32| format!("{:.0} %", v * 100.0);
    l.ui.slider("company-bid-price", Rect::new(inner.x, y, inner.w, ROW), &mut p, cn::PRICE_MIN as f32, cn::PRICE_MAX as f32, 0.01, "Your price", &fmt);
    y += ROW + 10.0;
    let price = p as f64;
    let per_km = cn::reference_per_km(&c) * price;
    row(l, y, &omsi_ui::tr("The authority pays a kilometre"), &eur_cents(per_km), false);
    y += rh;
    if let Some(w) = wk.filter(|w| w.km >= 1.0) {
        row(l, y, &omsi_ui::tr("Brings in a week, with the fares"), &eur(cn::week_revenue(&c, &w, price)), true);
        y += rh;
    }
    let q = cn::quality(&c, t.renewal);
    let score = cn::score(price, q);
    row(l, y, &omsi_ui::tr("Your score"), &format!("{:.0}  ({} {q:.0})", score * 100.0, omsi_ui::tr("quality")), false);
    y += rh;
    if !t.fee_paid {
        let fee = cn::fee(&c, &t);
        if fee > 0 {
            row(l, y, &omsi_ui::tr("Fee for taking part"), &eur(fee), false);
        }
    }
    let by = inner.bottom() - 38.0;
    let mut close = false;
    if l.ui.button("company-bid-cancel", Rect::new(inner.x, by, 120.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
        close = true;
    }
    if t.bid.is_some() && l.ui.button("company-bid-withdraw", Rect::new(inner.x + 132.0, by, 150.0, 38.0), "Withdraw", None, ButtonKind::Ghost) {
        act(l, |c| {
            cn::withdraw(c, tender);
            Ok(())
        });
        close = true;
    }
    if !close {
        let label = omsi_ui::tr("Bid %{p} %").replace("%{p}", &format!("{:.0}", price * 100.0));
        if l.ui.button("company-bid-do", Rect::new(inner.right() - 220.0, by, 220.0, 38.0), &label, Some("check_circle"), ButtonKind::Primary) && act(l, |c| cn::bid(c, tender, price)).is_some() {
            l.state.set_status(omsi_ui::tr("Your bid is in: the tender closes on %{date}.").replace("%{date}", &day_label(&t.closes)), false);
            close = true;
        }
    }
    if !close {
        l.company.dialog = Some(Dialog::Bid { tender, price: p });
    }
}

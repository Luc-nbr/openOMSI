//! "Drive a shift": today's shifts of the company as cards - the line, the time, the bus - the
//! player's own first, then those without a driver (`logic::shift_cards`). One click makes the
//! shift the player's for today (`co::autopilot::pick_shift`) and opens the company's duty
//! launcher with it (`planning::drive`); the game is started there, not here.

use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::{act, kit};
use super::logic::{self, ShiftCard};
use glam::Vec2;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan, Who};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// Today's plan and its shifts to drive (None: today's timetable is still being read).
pub(super) fn today(l: &mut Launcher, c: &Company) -> Option<(DayPlan, Vec<ShiftCard>)> {
    super::super::planning::work(l, c);
    let p = super::super::planning::plan_of(l, c, &c.date)?.ok()?;
    let cards = logic::shift_cards(&p, logic::now_minute(c));
    Some((p, cards))
}

/// What a shift is driven with, in words.
pub(super) fn bus_words(c: &Company, s: &ShiftCard) -> (String, bool) {
    match s.bus {
        Some(BusOf::Own(id)) => match c.vehicle(id) {
            Some(v) => (format!("{} · {}", v.number, v.name), true),
            None => (omsi_ui::tr("No bus yet").into_owned(), false),
        },
        Some(BusOf::Rental) => (omsi_ui::tr("A rented bus").into_owned(), true),
        None => (omsi_ui::tr("No bus yet").into_owned(), false),
    }
}

/// The shift picked: the player's for today, and the duty launcher opened with it.
pub(super) fn drive(l: &mut Launcher, c: &Company, p: &DayPlan, s: &ShiftCard) {
    if !s.mine() {
        let (date, line, tour, duty) = (p.date.clone(), s.line.clone(), s.tour_no.clone(), s.duty);
        act(l, |c| {
            co::autopilot::pick_shift(c, &date, &line, &tour, duty);
            Ok(())
        });
    }
    super::super::planning::drive(l, c, p, s.tour, s.duty);
}

/// A shift's card: the line's plate and the times, the bus, whether it is the player's or
/// without a driver. Returns whether it was clicked.
pub(super) fn card(ui: &mut Ui, r: Rect, c: &Company, s: &ShiftCard, k: usize) -> bool {
    let id = id_of(&format!("company-casual-shift-{k}"));
    let m = ui.tile(id, r, SHEET_RADIUS);
    let base = m.r;
    ui.tile_shadow(&m);
    let lit = if s.mine() { Some(accent().alpha(0.7)) } else { None };
    super::card_ground(ui, base, m.hover, lit);
    let pad = 16.0;
    let plate_w = match c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&s.line)) {
        Some(cl) => super::super::line_plate(ui, Vec2::new(base.x + pad, base.y + pad), cl, 28.0),
        None => super::super::plate(ui, Vec2::new(base.x + pad, base.y + pad), &s.number, 28.0),
    };
    // its mark: yours, or nobody's yet
    let tag = if s.mine() {
        Some((omsi_ui::tr("Your shift").into_owned(), accent()))
    } else if s.who.is_none() {
        Some((omsi_ui::tr("No driver yet").into_owned(), WARN))
    } else {
        None
    };
    if let Some((t, colour)) = tag {
        let tw = ui.width(&t, 12.0, Weight::Bold) + 16.0;
        if base.x + pad + plate_w + 10.0 + tw < base.right() - pad {
            kit::tag(ui, Vec2::new(base.right() - pad - tw, base.y + pad + 2.0), &t, colour);
        }
    }
    let time = format!("{} – {}", co::clock::hhmm(s.from as i64), co::clock::hhmm(s.to as i64));
    ui.text_as_is(&time, Vec2::new(base.x + pad, base.y + pad + 28.0 + 32.0), 24.0, Weight::Bold, TEXT, Align::Left);
    let (bus, ok) = bus_words(c, s);
    ui.icon("directions_bus", Vec2::new(base.x + pad + 9.0, base.y + pad + 82.0), 16.0, if ok { TEXT_DIM } else { WARN });
    ui.text_in(&bus, Rect::new(base.x + pad + 24.0, base.y + pad + 71.0, base.w - 2.0 * pad - 24.0, 22.0), kit::NOTE, Weight::Regular, if ok { TEXT_SOFT } else { WARN }, Align::Left);
    let trips = super::super::home::count_text(s.trips, "1 trip", "%{n} trips");
    ui.text_in(&trips, Rect::new(base.x + pad, base.y + pad + 96.0, base.w - 2.0 * pad, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    let b = Rect::new(base.x + pad, base.bottom() - pad - 40.0, base.w - 2.0 * pad, 40.0);
    let pressed = ui.button(&format!("company-casual-shift-go-{k}"), b, "Drive this shift", Some("play_arrow"), if s.mine() || k == 0 { ButtonKind::Primary } else { ButtonKind::Normal });
    pressed || m.clicked
}

/// A card's height.
pub(super) const CARD_H: f32 = 210.0;

/// The page: today's shifts as cards, in a row of up to four (two on a phone).
pub(super) fn page(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let (body, _) = super::page_head(l, area, super::SHIFTS, "Pick one of today's shifts: its start opens with the line, the bus and the times.", None);
    let Some((p, cards)) = today(l, &c) else {
        l.ui.text_in("Reading today's timetable…", Rect::new(body.x, body.y, body.w, 24.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
        return;
    };
    if cards.is_empty() {
        let (title, text, button) = if c.lines.is_empty() {
            ("No line yet", "Add a line first: its shifts are here the next moment.", Some(("Add a line", "alt_route")))
        } else if logic::buses(&c) == 0 {
            ("No bus yet", "Buy a bus for your lines: their shifts are here once it runs.", Some(("Buy a bus", "add")))
        } else {
            ("No shift left today", "Today's shifts have all begun. Run on to the next day for new ones.", Some(("Next day", "wb_sunny")))
        };
        if super::empty(l, body, "play_arrow", title, text, button) {
            let a = if c.lines.is_empty() {
                super::Act::AddLine
            } else if logic::buses(&c) == 0 {
                super::Act::BuyBus
            } else {
                super::Act::NextDay
            };
            super::run(l, a);
        }
        return;
    }
    let phone = super::super::super::mobile::mobile() || body.w < 640.0;
    let cols = if phone { 1 } else if body.w >= 1000.0 { 4 } else { 2 };
    let gap = 16.0;
    let w = ((body.w.min(1240.0) - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut go = None;
    super::super::super::tour::anchor("company-casual-shifts", body);
    l.ui.scroll_area("company-casual-shifts", body, &mut |ui, v| {
        for (k, s) in cards.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (w + gap), v.y + (k / cols) as f32 * (CARD_H + gap), w, CARD_H);
            if card(ui, r, &c, s, k) {
                go = Some(k);
            }
        }
        let rows = cards.len().div_ceil(cols);
        let mut y = v.y + rows as f32 * (CARD_H + gap) + 8.0;
        let note = "A shift you pick is yours for today only: tomorrow your drivers drive it again.";
        ui.icon("info", Vec2::new(v.x + 10.0, y + 10.0), 16.0, TEXT_DIM);
        y += ui.paragraph(note, Vec2::new(v.x + 26.0, y), (v.w - 30.0).max(0.0), kit::NOTE, Weight::Regular, TEXT_DIM);
        y - v.y + 12.0
    });
    if let Some(k) = go {
        drive(l, &c, &p, &cards[k]);
    }
}

/// Whether the player drives a shift of today already.
pub(super) fn mine_today(cards: &[ShiftCard]) -> bool {
    cards.first().is_some_and(|s| s.who == Some(Who::Player))
}

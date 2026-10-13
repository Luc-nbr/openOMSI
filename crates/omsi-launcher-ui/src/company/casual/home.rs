//! The Casual home: one calm screen. On top the "Today" card - how the day goes in one
//! sentence and, under it, what the company solved by itself - with the big "Drive a shift"
//! beside it; under them the four large tiles (my buses, my lines, my drivers, the money), each
//! with one big number and one action; then the tips, each with the button that does it; and,
//! after three weeks, a calm hint that Normal gives more control, until it is put away.
//!
//! On a computer it is kept to a readable width in the middle; a phone has it all under each
//! other, the tiles two by two. A low window scrolls.

use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::kit;
use super::logic::{self, Act, Tile, Tip};
use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// The widest the home grows, and the gap between its parts.
const MAX_W: f32 = 1180.0;
const GAP: f32 = 16.0;

/// What the home shows, worked out before it is drawn.
struct Home {
    date: String,
    sentence: String,
    solved: Vec<String>,
    shifts: usize,
    mine: bool,
    tiles: Vec<Tile>,
    tips: Vec<Tip>,
    hint: bool,
}

/// The home in the company's page area.
pub(super) fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let plan = l.company.plan.clone();
    let now = logic::now_minute(&c);
    let cards = super::shifts::today(l, &c).map(|x| x.1);
    let hidden = l.company.casual.hidden.clone();
    let h = Home {
        date: super::super::day_label(&c.date),
        sentence: logic::day_sentence(&c, plan.as_ref(), now).text(),
        solved: logic::solved(&c).into_iter().take(4).map(|(t, a)| super::super::inbox::words(&t, &a)).collect(),
        shifts: cards.as_ref().map(|x| x.len()).unwrap_or(0),
        mine: cards.as_ref().is_some_and(|x| super::shifts::mine_today(x)),
        tiles: logic::tiles(&c, plan.as_ref(), now),
        tips: logic::tips(&c, plan.as_ref()).into_iter().filter(|t| !hidden.contains(&t.id)).take(logic::TIPS).collect(),
        hint: logic::hint_due(&c),
    };
    let phone = super::super::super::mobile::mobile() || area.w < 760.0;
    let mut go: Option<Act> = None;
    let mut hide: Option<String> = None;
    let mut hint_off = false;
    l.ui.scroll_area("company-casual-home", area, &mut |ui, v| {
        let w = v.w.min(MAX_W);
        let x = (v.x + (v.w - w) * 0.5).floor();
        let mut y = v.y + if phone { 0.0 } else { 4.0 };
        // today, and the drive beside it (under it on a phone)
        let drive_w = if phone { w } else { (w * 0.3).clamp(260.0, 340.0) };
        let today_w = if phone { w } else { w - drive_w - GAP };
        let today_h = today_height(ui, today_w, &h).max(if phone { 0.0 } else { 196.0 });
        today_card(ui, Rect::new(x, y, today_w, today_h), &h);
        let drive_r = if phone { Rect::new(x, y + today_h + 12.0, w, 92.0) } else { Rect::new(x + today_w + GAP, y, drive_w, today_h) };
        if drive_card(ui, drive_r, &h) {
            go = Some(Act::Open(super::SHIFTS));
        }
        y = drive_r.bottom() + GAP + 4.0;
        // the four tiles
        let cols = if phone { 2 } else { 4 };
        let gap = if phone { 10.0 } else { GAP };
        let tw = ((w - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        let th = if phone { 178.0 } else { 196.0 };
        super::super::super::tour::anchor("company-casual-tiles", Rect::new(x, y, w, h.tiles.len().div_ceil(cols) as f32 * (th + gap) - gap));
        for (k, t) in h.tiles.iter().enumerate() {
            let r = Rect::new(x + (k % cols) as f32 * (tw + gap), y + (k / cols) as f32 * (th + gap), tw, th);
            if let Some(a) = tile(ui, r, t) {
                go = Some(a);
            }
        }
        y += h.tiles.len().div_ceil(cols) as f32 * (th + gap) + 8.0;
        // the tips
        let tips_h = tips_height(ui, w, &h.tips, phone);
        let tips_r = Rect::new(x, y, w, tips_h);
        super::super::super::tour::anchor("company-casual-tips", tips_r);
        match tips(ui, tips_r, &h.tips, phone) {
            Some(TipClick::Do(a)) => go = Some(a),
            Some(TipClick::Hide(id)) => hide = Some(id),
            None => {}
        }
        y += tips_h + GAP;
        // the hint to Normal
        if h.hint {
            let r = Rect::new(x, y, w, if phone { 118.0 } else { 64.0 });
            match hint(ui, r, phone) {
                Some(true) => go = Some(Act::ToNormal),
                Some(false) => hint_off = true,
                None => {}
            }
            y += r.h + GAP;
        }
        y - v.y + 8.0
    });
    if let Some(id) = hide {
        l.company.casual.hidden.push(id);
    }
    if hint_off {
        super::super::act(l, |c| {
            c.autopilot.normal_hint_off = true;
            Ok(())
        });
    }
    if let Some(a) = go {
        super::run(l, a);
    }
}

// --- today ----------------------------------------------------------------------------------------

const PAD: f32 = 22.0;
const LINE_H: f32 = 26.0;

fn today_height(ui: &Ui, w: f32, h: &Home) -> f32 {
    let sh = ui.paragraph_height(&h.sentence, w - 2.0 * PAD, 22.0, Weight::Bold);
    let rows = h.solved.len().max(1) as f32;
    PAD + 22.0 + 10.0 + sh + 18.0 + 20.0 + rows * LINE_H + PAD - 6.0
}

/// The "Today" card: the day, its sentence, what the company solved by itself.
fn today_card(ui: &mut Ui, r: Rect, h: &Home) {
    super::super::super::tour::anchor("company-casual-today", r);
    ui.p().rounded(r, SHEET_RADIUS, PANEL);
    ui.p().rounded_border(r, SHEET_RADIUS, 1.0, EDGE);
    let x = r.x + PAD;
    let w = r.w - 2.0 * PAD;
    let mut y = r.y + PAD;
    let caps = omsi_ui::tr("Today").to_uppercase();
    let cw = ui.width(&caps, kit::CAPS, Weight::Bold) + 10.0;
    ui.text_in(&caps, Rect::new(x, y, cw, 18.0), kit::CAPS, Weight::Bold, accent_2(), Align::Left);
    ui.text_as_is(&h.date, Vec2::new(x + cw + 4.0, y + 13.5), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    y += 22.0 + 10.0;
    y += ui.paragraph(&h.sentence, Vec2::new(x, y), w, 22.0, Weight::Bold, TEXT) + 18.0;
    kit::caps(ui, Rect::new(x, y, w, 16.0), "Solved by your company");
    y += 20.0;
    if h.solved.is_empty() {
        ui.icon("check_circle", Vec2::new(x + 9.0, y + LINE_H * 0.5), 16.0, OK);
        ui.text_in("Nothing went wrong today: your company runs smoothly.", Rect::new(x + 26.0, y, w - 26.0, LINE_H), kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT, Align::Left);
        return;
    }
    for t in &h.solved {
        if y + LINE_H > r.bottom() - 6.0 {
            break;
        }
        ui.icon("autorenew", Vec2::new(x + 9.0, y + LINE_H * 0.5), 15.0, accent_2());
        let tr = Rect::new(x + 26.0, y, w - 26.0, LINE_H);
        ui.text_as_is(t, Vec2::new(tr.x, tr.y + 17.5), kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT, Align::Left);
        ui.tooltip(tr, t);
        y += LINE_H;
    }
}

/// The big "Drive a shift": a card filled with the accent. Returns whether it was clicked.
fn drive_card(ui: &mut Ui, r: Rect, h: &Home) -> bool {
    super::super::super::tour::anchor("company-casual-drive", r);
    let id = id_of("company-casual-drive");
    let m = ui.tile(id, r, SHEET_RADIUS);
    let base = m.r;
    ui.tile_shadow(&m);
    ui.p().rounded(base, SHEET_RADIUS, accent().mix(accent().lighten(0.12), m.hover));
    ui.tile_light(&m, 0.08);
    let ink = on_accent();
    let low = base.h < 120.0;
    let ring = if low { 26.0 } else { 34.0 };
    let c = if low { Vec2::new(base.x + 24.0 + ring, base.center().y) } else { Vec2::new(base.x + 26.0 + ring, base.y + 30.0 + ring) };
    ui.p().circle(c, ring, ink.alpha(0.16));
    ui.icon("play_arrow", c, ring * 1.1, ink);
    let sub = if h.mine {
        omsi_ui::tr("Your shift of today is waiting").into_owned()
    } else if h.shifts > 0 {
        omsi_ui::tr("%{n} of today's shifts to choose from").replace("%{n}", &h.shifts.to_string())
    } else {
        omsi_ui::tr("See today's shifts").into_owned()
    };
    if low {
        let tx = c.x + ring + 18.0;
        ui.text_in("Drive a shift", Rect::new(tx, base.y + 18.0, base.right() - tx - 12.0, 30.0), 22.0, Weight::Bold, ink, Align::Left);
        ui.text_in(&sub, Rect::new(tx, base.y + 50.0, base.right() - tx - 12.0, 22.0), kit::NOTE + 0.5, Weight::Medium, ink.alpha(0.85), Align::Left);
    } else {
        let y = c.y + ring + 22.0;
        ui.text_in("Drive a shift", Rect::new(base.x + 26.0, y, base.w - 52.0, 34.0), 26.0, Weight::Bold, ink, Align::Left);
        ui.paragraph(&sub, Vec2::new(base.x + 26.0, y + 40.0), base.w - 52.0, kit::BODY, Weight::Medium, ink.alpha(0.85));
    }
    m.clicked
}

// --- the tiles --------------------------------------------------------------------------------------

/// A tile: its icon and name, its big number, the line under it and its action. Returns what was
/// clicked: the action, or the tile (its page).
fn tile(ui: &mut Ui, r: Rect, t: &Tile) -> Option<Act> {
    let id = id_of(&format!("company-casual-tile-{}", t.page));
    let m = ui.tile(id, r, SHEET_RADIUS);
    let base = m.r;
    ui.tile_shadow(&m);
    super::card_ground(ui, base, m.hover, None);
    ui.tile_light(&m, 0.04);
    let pad = if base.w < 200.0 { 14.0 } else { 18.0 };
    let mark = Rect::new(base.x + pad, base.y + pad, 34.0, 34.0);
    ui.p().rounded(mark, RADIUS, accent().alpha(0.14).mix(accent(), m.hover));
    ui.icon(t.icon, mark.center(), 19.0, accent_2().mix(on_accent(), m.hover));
    ui.text_in(t.name, Rect::new(mark.right() + 10.0, mark.y, base.right() - pad - mark.right() - 10.0, mark.h), 16.0, Weight::Bold, TEXT, Align::Left);
    let colour = match t.good {
        Some(true) => OK,
        Some(false) => if t.page == super::MONEY { DANGER } else { WARN },
        None => TEXT,
    };
    let big_px = if base.w < 200.0 { 28.0 } else { 34.0 };
    ui.text_as_is(&t.big, Vec2::new(base.x + pad, mark.bottom() + 14.0 + big_px * 0.8), big_px, Weight::Bold, colour, Align::Left);
    let under = t.under.text();
    let uy = mark.bottom() + 14.0 + big_px + 4.0;
    ui.text_in(&under, Rect::new(base.x + pad, uy, base.w - 2.0 * pad, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    ui.tooltip(Rect::new(base.x + pad, uy, base.w - 2.0 * pad, 20.0), &under);
    let (label, ic, a) = &t.action;
    let b = Rect::new(base.x + pad, base.bottom() - pad - 36.0, base.w - 2.0 * pad, 36.0);
    if ui.button(&format!("company-casual-tile-act-{}", t.page), b, label, Some(ic), ButtonKind::Normal) {
        return Some(a.clone());
    }
    m.clicked.then_some(Act::Open(t.page))
}

// --- the tips ----------------------------------------------------------------------------------------

const TIP_H: f32 = 56.0;

fn tips_height(ui: &Ui, w: f32, tips: &[Tip], phone: bool) -> f32 {
    let head = 28.0;
    if tips.is_empty() {
        return head + TIP_H;
    }
    if !phone {
        return head + tips.len() as f32 * (TIP_H + 8.0);
    }
    head + tips.iter().map(|t| ui.paragraph_height(&t.words.text(), w - 70.0, kit::BODY, Weight::Medium) + 70.0 + 8.0).sum::<f32>()
}

/// What was clicked in the tips: a tip's button, or its cross.
enum TipClick {
    Do(Act),
    Hide(String),
}

/// The tips: each a row with its icon, its words, its button and a cross that puts it away.
fn tips(ui: &mut Ui, r: Rect, tips: &[Tip], phone: bool) -> Option<TipClick> {
    kit::caps(ui, Rect::new(r.x, r.y, r.w, 16.0), "Tips");
    let mut y = r.y + 28.0;
    let mut out = None;
    if tips.is_empty() {
        let row = Rect::new(r.x, y, r.w, TIP_H);
        ui.p().rounded(row, RADIUS, FIELD.alpha(0.6));
        ui.icon("check_circle", Vec2::new(row.x + 24.0, row.center().y), 18.0, OK);
        ui.text_in("No tips right now: everything runs well.", Rect::new(row.x + 46.0, row.y, row.w - 60.0, row.h), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Left);
        return None;
    }
    for (k, t) in tips.iter().enumerate() {
        let text = t.words.text();
        let h = if phone { ui.paragraph_height(&text, r.w - 70.0, kit::BODY, Weight::Medium) + 70.0 } else { TIP_H };
        let row = Rect::new(r.x, y, r.w, h);
        let tint = if t.warn { WARN } else { accent_2() };
        ui.p().rounded(row, RADIUS, tint.alpha(0.09));
        ui.p().rounded(Rect::new(row.x, row.y, 4.0, row.h), 2.0, tint);
        ui.icon(if t.warn { "warning" } else { "light" }, Vec2::new(row.x + 26.0, if phone { row.y + 24.0 } else { row.center().y }), 19.0, tint);
        let (label, ic, a) = &t.button;
        let bw = kit::Foot::width(ui, label, Some(ic)).min(r.w * 0.45);
        // (the cross that puts it away for the session)
        let x_at = Vec2::new(row.right() - 22.0, if phone { row.y + 22.0 } else { row.center().y });
        if ui.icon_button(&format!("company-casual-tip-x-{k}"), x_at, 14.0, "close", "Not now") {
            out = Some(TipClick::Hide(t.id.clone()));
        }
        let b = if phone { Rect::new(row.x + 48.0, row.bottom() - 12.0 - 36.0, bw, 36.0) } else { Rect::new(x_at.x - 22.0 - bw, row.y + (row.h - 36.0) * 0.5, bw, 36.0) };
        if ui.button(&format!("company-casual-tip-{k}"), b, label, Some(ic), if t.warn { ButtonKind::Primary } else { ButtonKind::Normal }) {
            out = Some(TipClick::Do(a.clone()));
        }
        if phone {
            ui.paragraph(&text, Vec2::new(row.x + 48.0, row.y + 12.0), row.w - 70.0 - 20.0, kit::BODY, Weight::Medium, TEXT);
        } else {
            let tw = row.w - 48.0 - bw - 60.0;
            ui.text_in(&text, Rect::new(row.x + 48.0, row.y, tw.max(0.0), row.h), kit::BODY, Weight::Medium, TEXT, Align::Left);
            ui.tooltip(Rect::new(row.x + 48.0, row.y, tw.max(0.0), row.h), &text);
        }
        y += h + 8.0;
    }
    out
}

/// The calm hint after three weeks: Normal gives more control. Returns Some(true) when it is
/// taken, Some(false) when it is put away for good.
fn hint(ui: &mut Ui, r: Rect, phone: bool) -> Option<bool> {
    super::super::super::tour::anchor("company-casual-hint", r);
    ui.p().rounded(r, RADIUS, FIELD.alpha(0.7));
    ui.p().rounded_border(r, RADIUS, 1.0, HAIRLINE);
    ui.icon("tune", Vec2::new(r.x + 26.0, if phone { r.y + 26.0 } else { r.center().y }), 19.0, accent_2());
    let text = "Want more control? Normal opens every page: planning, workshop, tenders. Nothing of your company is lost.";
    let yes_w = kit::Foot::width(ui, "Switch to Normal", Some("tune"));
    let no_w = kit::Foot::width(ui, "Don't show again", None);
    let (yes, no) = if phone {
        let by = r.bottom() - 12.0 - 36.0;
        (Rect::new(r.x + 48.0, by, yes_w.min((r.w - 60.0) * 0.5), 36.0), Rect::new(r.x + 56.0 + yes_w.min((r.w - 60.0) * 0.5), by, no_w.min((r.w - 60.0) * 0.5), 36.0))
    } else {
        let no = Rect::new(r.right() - 12.0 - no_w, r.y + (r.h - 36.0) * 0.5, no_w, 36.0);
        (Rect::new(no.x - 8.0 - yes_w, no.y, yes_w, 36.0), no)
    };
    if phone {
        ui.paragraph(text, Vec2::new(r.x + 48.0, r.y + 12.0), r.w - 60.0, kit::NOTE + 0.5, Weight::Medium, TEXT_SOFT);
    } else {
        let tw = (yes.x - 12.0 - r.x - 48.0).max(0.0);
        ui.text_in(text, Rect::new(r.x + 48.0, r.y, tw, r.h), kit::NOTE + 0.5, Weight::Medium, TEXT_SOFT, Align::Left);
        ui.tooltip(Rect::new(r.x + 48.0, r.y, tw, r.h), text);
    }
    if ui.button("company-casual-hint-yes", yes, "Switch to Normal", Some("tune"), ButtonKind::Normal) {
        return Some(true);
    }
    if ui.button("company-casual-hint-no", no, "Don't show again", None, ButtonKind::Ghost) {
        return Some(false);
    }
    None
}

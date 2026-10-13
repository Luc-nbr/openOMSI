//! My lines: the company's lines as cards - the number in its colour, the buses on it today,
//! the passengers of the week - each with a fare slider and the advice beside it; and "Add a
//! line", a wizard of three steps: (1) one of the map's lines, or stops of the player's own;
//! (2) how often it runs - quiet, normal, busy; (3) done. The timetable, the shifts, the buses
//! it needs and the drivers are made by themselves: a map line keeps the map's timetable and
//! runs at once (bought at the asking price where it is under licence), a line of the player's
//! own is drawn in the line editor with the timetable of its "how often" (`logic::freq_days`),
//! and confirmed there.

use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::{act, eur, eur_cents, kit};
use super::logic::{self, Freq, Outcome, Pick};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

const GAP: f32 = 16.0;

/// The line wizard: its step (0 pick, 1 how often, 2 done) and the choices made.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in super::super) struct Wizard {
    pub step: usize,
    pub pick: Option<Pick>,
    pub freq: Freq,
}

// --- the lines -------------------------------------------------------------------------------------

/// The lines page.
pub(super) fn page(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let sub = super::super::home::count_text(c.lines.len(), "1 line · its timetable, shifts and drivers run by themselves", "%{n} lines · their timetables, shifts and drivers run by themselves");
    let (body, add) = super::page_head(l, area, super::LINES, &sub, Some(("Add a line", "alt_route")));
    if add {
        super::run(l, super::Act::AddLine);
        return;
    }
    if c.lines.is_empty() {
        if super::empty(l, body, "route", "No line yet", "Add your first line: pick one of the map's, or draw your own stops. It runs by itself from then on.", Some(("Add a line", "alt_route"))) {
            super::run(l, super::Act::AddLine);
        }
        return;
    }
    let plan = l.company.plan.clone();
    let phone = super::super::super::mobile::mobile() || body.w < 640.0;
    let cols = if phone { 1 } else if body.w >= 1100.0 { 3 } else { 2 };
    let cw = ((body.w - 12.0 - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let ch = 186.0;
    let focus = l.company.casual.focus.clone();
    let lines = c.lines.clone();
    let mut set: Option<(String, i64)> = None;
    super::super::super::tour::anchor("company-casual-lines", body);
    l.ui.scroll_area("company-casual-lines", body, &mut |ui, v| {
        for (k, line) in lines.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (ch + GAP), cw, ch);
            if !ui.rect_visible(r) {
                continue;
            }
            let lit = focus.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&line.name));
            if let Some(f) = line_card(ui, r, &c, plan.as_ref(), line, lit) {
                set = Some((line.name.clone(), f));
            }
        }
        lines.len().div_ceil(cols) as f32 * (ch + GAP)
    });
    if let Some((name, fare)) = set {
        let number = c.lines.iter().find(|x| x.name == name).map(|x| x.number.clone()).unwrap_or_default();
        if let Some(done) = act(l, |c| co::fares::set_fare(c, &name, fare)) {
            l.state.set_status(omsi_ui::tr("Line %{n}: a ticket costs %{fare} from now on.").replace("%{n}", &number).replace("%{fare}", &eur_cents(done as f64)), false);
        }
        l.company.casual.focus = None;
    }
}

/// A line's card: its plate and name, its buses today and passengers this week, and its fare
/// with the advice. Returns a fare set (cents).
fn line_card(ui: &mut Ui, r: Rect, c: &Company, plan: Option<&co::day::Plan>, line: &co::CompanyLine, lit: bool) -> Option<i64> {
    super::card_ground(ui, r, 0.0, lit.then(|| accent().alpha(0.8)));
    let pad = 18.0;
    let x = r.x + pad;
    let w = r.w - 2.0 * pad;
    let pw = super::super::line_plate(ui, Vec2::new(x, r.y + pad), line, 30.0);
    ui.text_in(line.public_name(), Rect::new(x + pw + 12.0, r.y + pad, w - pw - 12.0, 30.0), 16.5, Weight::Bold, TEXT, Align::Left);
    let buses = plan.map(|p| logic::buses_on_line(p, &line.name)).unwrap_or(0);
    let pax = logic::week_passengers(c, &line.name);
    let facts = [("directions_bus", super::super::home::count_text(buses, "1 bus today", "%{n} buses today")), ("groups", omsi_ui::tr("%{n} passengers this week").replace("%{n}", &super::super::grouped(pax as f64)))];
    let mut fx = x;
    for (ic, t) in facts {
        ui.icon(ic, Vec2::new(fx + 8.0, r.y + pad + 52.0), 16.0, TEXT_DIM);
        let tw = ui.width(&t, kit::NOTE, Weight::Medium) + 4.0;
        ui.text_in(&t, Rect::new(fx + 22.0, r.y + pad + 41.0, tw.min(w * 0.5), 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
        fx += 22.0 + tw.min(w * 0.5) + 18.0;
    }
    // the fare, and the advice
    let (lo, hi) = co::fares::band(c, line);
    let fare = co::fares::fare_of(c, line);
    let advised = logic::advised_fare(c, line);
    let y = r.y + pad + 80.0;
    ui.p().rect(Rect::new(x, y - 6.0, w, 1.0), HAIRLINE);
    let mut v = fare as f32 / 100.0;
    let name = format!("company-casual-fare-{}", line.name);
    let mut out = None;
    if ui.slider_on_release(&name, Rect::new(x, y + 4.0, w, 36.0), &mut v, lo as f32 / 100.0, hi as f32 / 100.0, 0.1, "A ticket", &|e| eur_cents((e as f64 * 100.0).round())) {
        out = Some(((v as f64 * 10.0).round() * 10.0) as i64);
    }
    let (advice, colour) = if (fare - advised).abs() <= 10 {
        (omsi_ui::tr("Good: this is the advised fare.").into_owned(), OK)
    } else if fare > advised {
        (omsi_ui::tr("Advice: %{fare}. Higher fares drive passengers away.").replace("%{fare}", &eur_cents(advised as f64)), WARN)
    } else {
        (omsi_ui::tr("Advice: %{fare}. The line could earn more.").replace("%{fare}", &eur_cents(advised as f64)), accent_2())
    };
    ui.text_in(&advice, Rect::new(x, y + 46.0, w - 150.0, 22.0), kit::NOTE, Weight::Medium, colour, Align::Left);
    ui.tooltip(Rect::new(x, y + 46.0, w - 150.0, 22.0), &advice);
    if (fare - advised).abs() > 10 && ui.button(&format!("company-casual-fare-advice-{}", line.name), Rect::new(r.right() - pad - 140.0, y + 42.0, 140.0, 32.0), "Use the advice", None, ButtonKind::Ghost) {
        out = Some(advised);
    }
    out
}

// --- the wizard ------------------------------------------------------------------------------------

/// The wizard's steps, by name.
const STEPS: [&str; 3] = ["Pick the line", "How often", "Done"];

/// The line wizard's page.
pub(super) fn wizard(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let (body, _) = super::page_head(l, area, super::NEW_LINE, "Three steps: the timetable, the shifts and the drivers are made by themselves.", None);
    let mut w = l.company.casual.wizard.clone();
    // the steps, as a row of numbered marks
    let bar = Rect::new(body.x, body.y, body.w.min(720.0), 36.0);
    super::super::super::tour::anchor("company-casual-wizard", bar);
    steps_bar(&mut l.ui, bar, w.step);
    let body = Rect::new(body.x, body.y + 52.0, body.w, (body.h - 52.0).max(0.0));
    let foot_h = 44.0;
    let room = Rect::new(body.x, body.y, body.w, (body.h - foot_h - 16.0).max(0.0));
    let foot = Rect::new(body.x, body.bottom() - foot_h, body.w.min(1180.0), foot_h);
    let lines = super::super::map_lines(&l.company).map(|x| x.to_vec());
    let mut finish = false;
    match w.step {
        0 => {
            let Some(all) = lines.as_ref() else {
                l.ui.text_in("Reading the map's timetable…", Rect::new(room.x, room.y, room.w, 24.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
                return;
            };
            if let Some(p) = pick_step(l, room, &c, all, w.pick.as_ref()) {
                if let Pick::Map(name) = &p {
                    if let Some(h) = all.iter().find(|x| x.name == *name).and_then(logic::map_headway) {
                        w.freq = Freq::of_headway(h);
                    }
                }
                w.pick = Some(p);
                w.step = 1;
            }
        }
        1 => freq_step(l, room, &c, &mut w, lines.as_deref().unwrap_or(&[])),
        _ => done_step(l, room, &c, &w, lines.as_deref().unwrap_or(&[])),
    }
    // the foot: back, and on (or done)
    let back_label = if w.step == 0 { "Cancel" } else { "Back" };
    let bw = kit::Foot::width(&l.ui, back_label, Some("chevron_left"));
    if l.ui.button("company-casual-wizard-back", Rect::new(foot.x, foot.y, bw, foot_h), back_label, Some("chevron_left"), ButtonKind::Normal) {
        if w.step == 0 {
            l.company.casual.wizard = Default::default();
            l.company.tab = super::LINES;
            return;
        }
        w.step -= 1;
    }
    if w.step >= 1 {
        let (label, icon) = match (w.step, &w.pick) {
            (1, _) => ("Next", "chevron_right"),
            (_, Some(Pick::Draw)) => ("Draw the stops", "location_on"),
            _ => ("Start the line", "check_circle"),
        };
        let nw = kit::Foot::width(&l.ui, label, Some(icon));
        if l.ui.button("company-casual-wizard-next", Rect::new(foot.right() - nw, foot.y, nw, foot_h), label, Some(icon), ButtonKind::Primary) {
            if w.step == 1 {
                w.step = 2;
            } else {
                finish = true;
            }
        }
    }
    l.company.casual.wizard = w.clone();
    if finish {
        make(l, &c, &w, lines.as_deref().unwrap_or(&[]));
    }
}

/// The steps' marks: done ones ticked, the one now in the accent.
fn steps_bar(ui: &mut Ui, r: Rect, now: usize) {
    let sw = r.w / STEPS.len() as f32;
    for (k, s) in STEPS.iter().enumerate() {
        let x = r.x + k as f32 * sw;
        let mid = Vec2::new(x + 16.0, r.center().y);
        if k < now {
            ui.icon("check_circle", mid, 26.0, OK);
        } else if k == now {
            ui.p().circle(mid, 14.0, accent());
            ui.text_as_is(&(k + 1).to_string(), Vec2::new(mid.x, mid.y + 5.0), 14.0, Weight::Bold, on_accent(), Align::Center);
        } else {
            ui.p().circle(mid, 14.0, HAIRLINE);
            ui.p().circle(mid, 12.5, PANEL);
            ui.text_as_is(&(k + 1).to_string(), Vec2::new(mid.x, mid.y + 5.0), 13.0, Weight::Bold, TEXT_DIM, Align::Center);
        }
        ui.text_in(s, Rect::new(x + 38.0, r.y, sw - 46.0, r.h), kit::BODY, if k == now { Weight::Bold } else { Weight::Medium }, if k <= now { TEXT } else { TEXT_DIM }, Align::Left);
        if k + 1 < STEPS.len() {
            ui.p().rect(Rect::new(x + sw - 30.0, r.center().y, 22.0, 1.5), HAIRLINE);
        }
    }
}

/// A choice's card: its icon or plate, its title and words, lit when chosen. Returns whether it
/// was clicked.
fn choice(ui: &mut Ui, r: Rect, id: &str, icon: &str, title: &str, text: &str, chosen: bool, plate: Option<(&str, &str)>) -> bool {
    let m = ui.tile(id_of(id), r, SHEET_RADIUS);
    let base = m.r;
    ui.tile_shadow(&m);
    super::card_ground(ui, base, m.hover, chosen.then(|| accent()));
    if chosen {
        ui.p().rounded(base, SHEET_RADIUS, accent().alpha(0.08));
    }
    let pad = 18.0;
    let tx = match plate {
        Some((number, colour)) => {
            let w = if colour.is_empty() { super::super::plate(ui, Vec2::new(base.x + pad, base.y + pad), number, 30.0) } else { super::super::super::ownlines::plate(ui, Vec2::new(base.x + pad, base.y + pad), number, colour, 30.0) };
            base.x + pad + w + 12.0
        }
        None => {
            let mark = Rect::new(base.x + pad, base.y + pad, 36.0, 36.0);
            ui.p().rounded(mark, RADIUS, accent().alpha(0.14).mix(accent(), m.hover));
            ui.icon(icon, mark.center(), 20.0, accent_2().mix(on_accent(), m.hover));
            mark.right() + 12.0
        }
    };
    ui.text_in(title, Rect::new(tx, base.y + pad, base.right() - pad - tx, 34.0), 16.5, Weight::Bold, TEXT, Align::Left);
    ui.paragraph(text, Vec2::new(base.x + pad, base.y + pad + 48.0), base.w - 2.0 * pad, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT);
    m.clicked
}

/// Step 1: the map's lines the company does not run yet, and stops of one's own.
fn pick_step(l: &mut Launcher, room: Rect, c: &Company, all: &[core::LineInfo], picked: Option<&Pick>) -> Option<Pick> {
    let choices = logic::map_choices(c, all);
    let phone = super::super::super::mobile::mobile() || room.w < 640.0;
    let cols = if phone { 1 } else if room.w >= 1100.0 { 3 } else { 2 };
    let cw = ((room.w.min(1180.0) - 12.0 - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let ch = 132.0;
    let depot = c.depot.clone();
    let rows: Vec<(String, String, String, String)> = choices
        .iter()
        .map(|x| {
            let runs = x.tours.iter().filter(|t| t.runs).count();
            let often = match logic::map_headway(x) {
                Some(h) => super::super::home::count_text(runs, "1 bus out today · about every %{m} minutes", "%{n} buses out today · about every %{m} minutes").replace("%{m}", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0)),
                None => super::super::home::count_text(runs, "1 bus out today", "%{n} buses out today"),
            };
            let caption = co::specials::caption_of(x, &[&depot]);
            (x.name.clone(), co::specials::number_of(x, &[&depot]), caption, often)
        })
        .collect();
    let mut out = None;
    l.ui.scroll_area("company-casual-wizard-pick", room, &mut |ui, v| {
        // (the first card: stops of one's own)
        let n = rows.len() + 1;
        for k in 0..n {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (ch + GAP), cw, ch);
            if !ui.rect_visible(r) {
                continue;
            }
            if k == 0 {
                if choice(ui, r, "company-casual-pick-draw", "location_on", "Draw your own stops", "Click the stops on the map in the line editor: the way between them is found over the roads.", picked == Some(&Pick::Draw), None) {
                    out = Some(Pick::Draw);
                }
                continue;
            }
            let (name, number, caption, often) = &rows[k - 1];
            let text = often.clone();
            if choice(ui, r, &format!("company-casual-pick-{name}"), "route", caption, &text, picked == Some(&Pick::Map(name.clone())), Some((number, ""))) {
                out = Some(Pick::Map(name.clone()));
            }
        }
        n.div_ceil(cols) as f32 * (ch + GAP)
    });
    out
}

/// Step 2: how often the line runs (a map line keeps the map's timetable: said, not chosen).
fn freq_step(l: &mut Launcher, room: Rect, _c: &Company, w: &mut Wizard, all: &[core::LineInfo]) {
    let map = match &w.pick {
        Some(Pick::Map(name)) => all.iter().find(|x| x.name == *name),
        _ => None,
    };
    let phone = super::super::super::mobile::mobile() || room.w < 640.0;
    let mut y = room.y;
    if let Some(line) = map {
        let t = match logic::map_headway(line) {
            Some(h) => omsi_ui::tr("This line of the map keeps the map's own timetable: a bus about every %{m} minutes in the daytime.").replace("%{m}", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0)),
            None => omsi_ui::tr("This line of the map keeps the map's own timetable.").into_owned(),
        };
        y += l.ui.paragraph(&t, Vec2::new(room.x, y), room.w.min(820.0), kit::BODY, Weight::Regular, TEXT_SOFT) + 18.0;
    } else {
        y += l.ui.paragraph("How busy should your line be? You can change it later in the line editor.", Vec2::new(room.x, y), room.w.min(820.0), kit::BODY, Weight::Regular, TEXT_SOFT) + 18.0;
    }
    let cols = if phone { 1 } else { 3 };
    let cw = ((room.w.min(1180.0) - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let ch = 150.0;
    for (k, f) in Freq::ALL.iter().enumerate() {
        let r = Rect::new(room.x + (k % cols) as f32 * (cw + GAP), y + (k / cols) as f32 * (ch + GAP), cw, ch);
        let chosen = w.freq == *f;
        // (a map line's own is shown, the others faint)
        if map.is_some() && !chosen {
            l.ui.p().rounded(r, SHEET_RADIUS, FIELD.alpha(0.4));
            l.ui.text_in(f.label(), Rect::new(r.x + 18.0, r.y + 18.0, r.w - 36.0, 30.0), 16.5, Weight::Bold, TEXT_FAINT, Align::Left);
            continue;
        }
        if choice(&mut l.ui, r, &format!("company-casual-freq-{k}"), f.icon(), f.label(), f.words(), chosen, None) && map.is_none() {
            w.freq = *f;
        }
    }
}

/// Step 3: what the line comes to, and what the company makes of it by itself.
fn done_step(l: &mut Launcher, room: Rect, c: &Company, w: &Wizard, all: &[core::LineInfo]) {
    let Some(pick) = w.pick.as_ref() else { return };
    let o: Outcome = logic::outcome(c, pick, w.freq, all);
    let x = room.x;
    let width = room.w.min(760.0);
    let mut y = room.y;
    let card = Rect::new(x, y, width, 0.0);
    let title = match pick {
        Pick::Map(_) => omsi_ui::tr("Line %{n}, ready to run").replace("%{n}", &o.number),
        Pick::Draw => omsi_ui::tr("Your own line: draw its stops").into_owned(),
    };
    l.ui.text_in(&title, Rect::new(card.x, y, width, 30.0), kit::HEAD + 2.0, Weight::Bold, TEXT, Align::Left);
    y += 44.0;
    let mut row = |l: &mut Launcher, icon: &str, what: &str, value: String, colour: Color| {
        l.ui.icon(icon, Vec2::new(x + 12.0, y + 15.0), 18.0, accent_2());
        l.ui.text_in(what, Rect::new(x + 34.0, y, width * 0.5, 30.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(&value, Rect::new(x + width * 0.42, y, width * 0.58, 30.0), kit::BODY, Weight::Bold, colour, Align::Right);
        l.ui.p().rect(Rect::new(x, y + 34.0, width, 1.0), HAIRLINE);
        y += 40.0;
    };
    let often = match o.headway {
        Some(h) => omsi_ui::tr("%{freq}: about every %{m} minutes").replace("%{freq}", &omsi_ui::tr(o.freq.label())).replace("%{m}", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0)),
        None => omsi_ui::tr(o.freq.label()).into_owned(),
    };
    row(l, "schedule", "How often", often, TEXT);
    row(l, "event", "Timetable and shifts", omsi_ui::tr("Made by themselves").into_owned(), OK);
    match o.buses {
        Some(b) => {
            let t = if o.short > 0 { omsi_ui::tr("%{n} (buy %{m} more)").replace("%{n}", &b.to_string()).replace("%{m}", &o.short.to_string()) } else { omsi_ui::tr("%{n} (you have them)").replace("%{n}", &b.to_string()) };
            row(l, "directions_bus", "Buses it needs", t, if o.short > 0 { WARN } else { OK });
        }
        None => row(l, "directions_bus", "Buses it needs", omsi_ui::tr("Shown in the line editor").into_owned(), TEXT_SOFT),
    }
    let drivers = match o.drivers {
        Some(d) => omsi_ui::tr("%{n}, hired by themselves").replace("%{n}", &d.to_string()),
        None => omsi_ui::tr("Hired by themselves").into_owned(),
    };
    row(l, "groups", "Drivers", drivers, OK);
    if let Some(p) = o.price {
        row(l, "receipt_long", "Its licence", eur(p), if c.cash >= p { TEXT } else { DANGER });
    }
    y += 10.0;
    let note = match pick {
        Pick::Map(_) => "Start the line, and it runs from now on: every morning the company gives its buses and drivers their work.",
        Pick::Draw => "The line editor opens with this timetable: click the stops on the map, and confirm the line there. It shows what the line costs and brings.",
    };
    l.ui.paragraph(note, Vec2::new(x, y), width, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT);
}

/// The wizard's end: the map's line taken on and run, or the line editor opened to draw one's
/// own with its timetable.
fn make(l: &mut Launcher, c: &Company, w: &Wizard, all: &[core::LineInfo]) {
    match &w.pick {
        Some(Pick::Draw) => {
            l.company.casual.wizard = Default::default();
            l.company.tab = super::LINES;
            super::super::super::lineeditor::open_new_for_company_with(l, logic::freq_days(w.freq));
        }
        Some(Pick::Map(name)) => {
            let Some(line) = all.iter().find(|x| x.name == *name).cloned() else { return };
            let number = co::specials::number_of(&line, &[&c.depot]);
            let direct = co::concessions::may_add_directly(c);
            let ok = if direct {
                let date = c.date.clone();
                act(l, |c| {
                    co::network::add_line(c, &line, None)?;
                    co::autopilot::start_lines(c, &date);
                    Ok(())
                })
                .is_some()
            } else {
                act(l, |c| co::autopilot::take_line(c, &line)).is_some()
            };
            if ok {
                l.company.casual.wizard = Default::default();
                l.company.tab = super::LINES;
                l.company.casual.focus = Some(line.name.clone());
                let t = if direct { "Line %{n} is yours: it runs by itself from now on." } else { "Line %{n} is yours: it runs by itself from tomorrow." };
                l.state.set_status(omsi_ui::tr(t).replace("%{n}", &number), false);
            }
        }
        None => {}
    }
}

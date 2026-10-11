//! The planning's scheduling studio (`co::studio`; Luc: the planning should look and work
//! "like Optibus Scheduling"): the day as one Gantt on a dark navy panel. The Vehicles view
//! has a row per bus's block - its trips as arrow chips in a muted tint of the line's colour,
//! labelled with the line and the direction ("5-A"), its depot runs as dark boxes at the ends
//! and the empty runs between places as thin lines; the Drivers view a row per duty, its
//! pieces and the breaks between them ("Split" where the break is long).
//!
//! A trip is dragged to another bus of its line, or to the "New bus" row; where it would land
//! shows as an empty chip, and what may not be is refused with its reason. The gap between
//! two trips is tapped to cut the block into duties there - only at a terminus or the depot,
//! where the bus stands - and a cut tapped again joins them. In the Drivers view a duty dragged
//! onto another joins them into one driver's day (a split duty, or one that changes buses).
//! The working-time rules are checked as the player edits: the forbidden refused, the rest a
//! badge on the duty with what it is. The figures on top follow every edit, against before
//! it. "Optimise" proposes the automatic blocks and duties, to accept or not; "Undo" takes the
//! last edit back. What is laid out holds for that weekday every week, as the roster does.

use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::{act, eur, kit};
use super::{hhmm, length};
use glam::Vec2;
use omsi_launcher_lib::company::network::{PlannedTrip, TourOfDay};
use omsi_launcher_lib::company::studio::{self as st, Duty, Kpis, Rule, Target};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The studio's panel and its header band (the dark look; the light look is made of them).
const NAVY: Color = Color::rgba(24, 32, 54, 1.0);
const NAVY_HEAD: Color = Color::rgba(32, 42, 70, 1.0);
/// A depot box: the bus leaving the depot or going back to it.
const DEPOT: Color = Color::rgba(12, 17, 31, 1.0);
/// The lines' colours where a line has none of its own (a line of the map's timetable).
const PALETTE: [(u8, u8, u8); 8] = [(46, 156, 242), (233, 78, 138), (245, 166, 35), (126, 107, 242), (47, 191, 160), (217, 91, 67), (90, 200, 250), (196, 120, 220)];
/// A row's pitch and its chips' height.
const ROW: f32 = 34.0;
const CHIP_H: f32 = 22.0;
/// The column of the rows' names.
const LABELS: f32 = 176.0;
/// The time axis's band.
const AXIS: f32 = 34.0;

/// The day as the studio shows it: its blocks (`tours`, as `network::tours_of_day` gives
/// them), its duties, its figures, and the rows in order (blocks by line, then by start).
#[derive(Clone)]
pub(super) struct Day {
    pub tours: Vec<TourOfDay>,
    pub duties: Vec<Duty>,
    pub kpis: Kpis,
    rows: Vec<usize>,
}

fn make_day(c: &Company, weekday: u8, tours: Vec<TourOfDay>, joined: &[st::JoinedDuty]) -> Day {
    let duties = st::duties(joined, weekday, &tours);
    let kpis = st::kpis(c, &tours, &duties);
    let line_at = |l: &str| c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(l)).unwrap_or(usize::MAX);
    let mut rows: Vec<usize> = (0..tours.len()).collect();
    rows.sort_by_key(|&i| (line_at(&tours[i].line), tours[i].from(), i));
    Day { tours, duties, kpis, rows }
}

/// A trip taken up to be moved, or a duty's piece to be joined.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Grab {
    Trip { tour: usize, trip: usize },
    Piece { tour: usize, duty: usize },
}

/// Where something taken up would go.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drop {
    Block(usize),
    New,
    Duty(usize),
}

/// What the player did this frame, done once the chart is drawn.
enum Intent {
    Cut(usize, usize),
    Uncut(usize, usize),
    Unjoin(usize, usize),
}

/// The studio's state on the planning page.
#[derive(Default)]
pub(in super::super) struct StudioView {
    /// The day shown: its date and the company's generation it was made for.
    day: Option<(String, u64, Day)>,
    /// "Optimise"'s proposal for that date and generation (the automatic blocks and duties).
    proposal: Option<(String, u64, Day)>,
    /// The figures before the last edit, of a date.
    before: Option<(String, Kpis)>,
    /// The layouts and joined duties before each edit (the last last), for "Undo".
    undo: Vec<(Vec<st::LineLayout>, Vec<st::JoinedDuty>)>,
    /// Pressed on a trip or a piece (where, and what), and taken up once the mouse moved.
    press: Option<(Vec2, Grab)>,
    grab: Option<Grab>,
    /// Where it was taken up (the mouse's offset from its chip's top).
    grab_dy: f32,
    /// The last drop asked about, and the answer (a join is tried on a copy of the company).
    asked: Option<(Grab, Drop, Result<(), &'static str>)>,
    /// The trip chosen (tapped): its block and trip.
    chosen: Option<(usize, usize)>,
    /// How far the hours are drawn apart (1 the whole day across) and the first minute shown.
    zoom: f32,
    pan: i32,
}

impl StudioView {
    /// Something is in hand that Escape lets go of first.
    pub(in super::super) fn holds(&self) -> bool {
        self.grab.is_some() || self.chosen.is_some() || self.proposal.is_some()
    }

    pub(in super::super) fn let_go(&mut self) {
        self.grab = None;
        self.press = None;
        self.chosen = None;
        self.proposal = None;
    }
}

/// A line's colour: its own, else one of the palette by its place among the company's lines.
fn line_colour(c: &Company, line: &str) -> Color {
    let k = c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(line)).unwrap_or(0);
    match c.lines.get(k).map(|l| l.colour.trim()).filter(|s| !s.is_empty()) {
        Some(hex) => super::super::super::ownlines::colour_of(hex),
        None => {
            let (r, g, b) = PALETTE[k % PALETTE.len()];
            Color::rgba(r, g, b, 1.0)
        }
    }
}

/// A chip's fill and ink: the line's colour itself when lit (hovered, chosen, taken up), else
/// a muted tint of it - dark on the navy, pale in the light look.
fn chip_colours(ui: &Ui, c: Color, lit: bool) -> (Color, Color) {
    if lit {
        (c, super::super::super::ownlines::ink_on(c))
    } else if ui.light {
        (c.mix(Color::WHITE, 0.72), c.mix(Color::rgba(0, 0, 0, 1.0), 0.5))
    } else {
        (c.mix(NAVY, 0.62), c.mix(Color::WHITE, 0.45).alpha(0.92))
    }
}

/// The outline of an arrow chip: flat on the left, pointed to the right.
fn arrow_points(r: Rect) -> [Vec2; 5] {
    let tip = (r.h * 0.42).min(r.w * 0.45);
    [Vec2::new(r.x, r.y), Vec2::new(r.right() - tip, r.y), Vec2::new(r.right(), r.center().y), Vec2::new(r.right() - tip, r.bottom()), Vec2::new(r.x, r.bottom())]
}

/// An arrow chip filled (its left corners a little rounded).
fn arrow(ui: &mut Ui, r: Rect, fill: Color) {
    let tip = (r.h * 0.42).min(r.w * 0.45);
    let body = Rect::new(r.x, r.y, (r.w - tip + 3.0).max(1.0), r.h);
    ui.p().rounded(body, 3.0f32.min(body.w * 0.5), fill);
    ui.p().rect(Rect::new((r.right() - tip - 3.0).max(r.x), r.y, 3.0f32.min(r.w), r.h), fill);
    ui.p().convex(&[Vec2::new(r.right() - tip, r.y), Vec2::new(r.right(), r.center().y), Vec2::new(r.right() - tip, r.bottom())], fill);
}

/// An empty chip: where a trip taken up would land.
fn arrow_outline(ui: &mut Ui, r: Rect, c: Color) {
    let p = arrow_points(r);
    let mut pts: Vec<Vec2> = p.to_vec();
    pts.push(p[0]);
    ui.p().stroke(&pts, 1.6, c);
}

/// A trip's chip in the line's colour, its words as its width allows (as drawn: the line's
/// colour is the same in either look).
fn trip_chip(ui: &mut Ui, r: Rect, colour: Color, lit: bool, label: &str) {
    let (fill, ink) = chip_colours(ui, colour, lit);
    ui.as_drawn();
    arrow(ui, r, fill);
    let px = 11.0;
    if ui.width(label, px, Weight::Bold) + 4.0 <= r.w - r.h * 0.38 {
        ui.text_in(label, Rect::new(r.x + 2.0, r.y, r.w - r.h * 0.42, r.h), px, Weight::Bold, ink, Align::Center);
    }
    ui.end_as_drawn();
}

/// A depot box: the bus leaving the depot (or going back), from `a` to `b` on the axis.
fn depot_box(ui: &mut Ui, a: f32, b: f32, cy: f32, implied: bool) {
    let w = (b - a).max(14.0);
    let r = Rect::new(if b - a < 14.0 { b - 14.0 } else { a }, cy - CHIP_H * 0.5, w, CHIP_H);
    let alpha = if implied { 0.55 } else { 1.0 };
    ui.p().rounded(r, 5.0, DEPOT.alpha(alpha));
    ui.p().rounded_border(r, 5.0, 1.0, Color::WHITE.alpha(0.10 * alpha));
}

/// The direction a trip runs, as a letter of its line's termini (A the first in order).
fn direction(tours: &[TourOfDay], line: &str, to: &str) -> char {
    let mut ends: Vec<&str> = tours.iter().filter(|t| t.line.eq_ignore_ascii_case(line)).flat_map(|t| t.trips.iter()).filter(|x| x.counts()).map(|x| x.to.trim()).collect();
    ends.sort_unstable();
    ends.dedup();
    let k = ends.iter().position(|e| *e == to.trim()).unwrap_or(0);
    (b'A' + (k % 26) as u8) as char
}

fn trip_label(t: &TourOfDay, x: &PlannedTrip, tours: &[TourOfDay]) -> String {
    let n = if x.line.trim().is_empty() { t.number.trim() } else { x.line.trim() };
    format!("{n}-{}", direction(tours, &t.line, &x.to))
}

fn trip_tip(t: &TourOfDay, x: &PlannedTrip) -> String {
    let mut s = format!("{} {}  ·  {} {}\n{} {} → {} {}  ·  {:.0} km", omsi_ui::tr("Line"), t.number, omsi_ui::tr("Tour"), t.tour, hhmm(x.dep), x.from, hhmm(x.arr), x.to, x.km);
    if !x.origin.trim().is_empty() && x.origin.trim() != t.tour.trim() {
        s.push_str(&format!("\n{}", omsi_ui::tr("In the timetable: tour %{t}").replace("%{t}", x.origin.trim())));
    }
    if x.counts() {
        s.push_str(&format!("\n{}", omsi_ui::tr("Drag it to another bus of the line, or to the New bus row.")));
    }
    s
}

/// A rule in words: its name and its minutes.
fn rule_text(r: Rule) -> String {
    match r.minutes() {
        Some(m) => format!("{}  ·  {}", omsi_ui::tr(r.label()), length(m.abs())),
        None => omsi_ui::tr(r.label()).into_owned(),
    }
}

/// A duty's badge: red for what is forbidden, amber for a warning; its rules on hover.
fn badge(ui: &mut Ui, at: Vec2, rules: &[Rule]) {
    if rules.is_empty() {
        return;
    }
    let col = if rules.iter().any(|r| r.hard()) { DANGER } else { WARN };
    ui.as_drawn();
    ui.p().circle(at, 7.0, col);
    ui.text_in("!", Rect::new(at.x - 7.0, at.y - 7.0, 14.0, 14.0), 11.0, Weight::Black, Color::rgba(20, 16, 10, 1.0), Align::Center);
    ui.end_as_drawn();
    let tip = rules.iter().map(|r| rule_text(*r)).collect::<Vec<_>>().join("\n");
    ui.tooltip(Rect::new(at.x - 8.0, at.y - 8.0, 16.0, 16.0), &tip);
}

/// The time axis: the first and last minute of the day's blocks, rounded to hours.
fn day_span(tours: &[TourOfDay]) -> (i32, i32) {
    let t0 = tours.iter().map(|t| t.from()).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = (tours.iter().map(|t| t.to()).max().unwrap_or(1380) + 59).div_euclid(60) * 60;
    (t0, t1.max(t0 + 120))
}

// --- the page --------------------------------------------------------------------------------

/// The studio's tools on the planning's tool row (right of the views' switch): Optimise and
/// Undo, and what holds for the weekday.
pub(super) fn tools(l: &mut Launcher, r: Rect, c: &Company, date: &str) {
    let wd = co::plan::weekday_of(date);
    let mut x = r.right();
    let ow = kit::Foot::width(&l.ui, "Optimise", Some("bolt"));
    x -= ow;
    let optimise = l.ui.button("studio-optimise", Rect::new(x, r.y, ow, r.h), "Optimise", Some("bolt"), ButtonKind::Normal);
    l.ui.tooltip(Rect::new(x, r.y, ow, r.h), "Run the automatic blocking and runcutting: its result is shown as a proposal you can accept, then change by hand");
    let uw = kit::Foot::width(&l.ui, "Undo", Some("history"));
    x -= uw + 8.0;
    let can_undo = !l.company.planning.studio.undo.is_empty();
    let undo = l.ui.button("studio-undo", Rect::new(x, r.y, uw, r.h), "Undo", Some("history"), if can_undo { ButtonKind::Normal } else { ButtonKind::Ghost }) && can_undo;
    l.ui.tooltip(Rect::new(x, r.y, uw, r.h), "Take the last change of the blocks or duties back");
    x -= 12.0;
    let day = super::weekday_name(wd);
    let note = if st::edited(c, wd) { omsi_ui::tr("Laid out by hand: holds every %{day}. The roster gives its buses and drivers.") } else { omsi_ui::tr("The automatic blocks and duties of every %{day}: drag a trip or tap between two trips to change them.") };
    l.ui.paragraph(&note.replace("%{day}", &day), Vec2::new(r.x, r.y + 2.0), (x - r.x).max(0.0), kit::NOTE + 0.5, Weight::Regular, TEXT_DIM);
    if optimise {
        let lines = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).and_then(|d| d.2.as_ref().ok()).cloned();
        if let Some(lines) = lines {
            let tours = co::network::automatic_tours_of_day(c, &lines, date);
            let p = make_day(c, wd, tours, &[]);
            let generation = l.company.generation;
            l.company.planning.studio.proposal = Some((date.to_string(), generation, p));
        }
    }
    if undo {
        if let Some((layouts, joined)) = l.company.planning.studio.undo.pop() {
            let now = l.company.planning.studio.day.as_ref().map(|d| (d.0.clone(), d.2.kpis));
            act(l, |c| {
                c.planning.layouts = layouts;
                c.planning.joined = joined;
                Ok(())
            });
            l.company.planning.studio.before = now;
            l.state.set_status(omsi_ui::tr("The last change is taken back.").into_owned(), false);
        }
    }
}

/// Do an edit of the blocks or duties: what was before kept for "Undo" and for the figures'
/// difference; a refusal said with its reason.
fn edit(l: &mut Launcher, f: impl FnOnce(&mut Company) -> Result<(), &'static str>) -> bool {
    let Some(c) = l.company.company.as_ref() else { return false };
    let snapshot = (c.planning.layouts.clone(), c.planning.joined.clone());
    let before = l.company.planning.studio.day.as_ref().map(|d| (d.0.clone(), d.2.kpis));
    let mut probe = c.clone();
    match f(&mut probe) {
        Ok(()) => {
            let (layouts, joined) = (probe.planning.layouts, probe.planning.joined);
            act(l, |c| {
                c.planning.layouts = layouts;
                c.planning.joined = joined;
                Ok(())
            });
            let s = &mut l.company.planning.studio;
            s.undo.push(snapshot);
            if s.undo.len() > 40 {
                s.undo.remove(0);
            }
            s.before = before;
            s.chosen = None;
            true
        }
        Err(why) => {
            l.state.set_status(omsi_ui::tr(why).into_owned(), true);
            false
        }
    }
}

/// The studio on the planning page: the figures, then the chart (`drivers`: the Drivers view,
/// else the Vehicles view).
pub(super) fn draw(l: &mut Launcher, area: Rect, c: &Company, date: &str, drivers: bool) {
    let wd = co::plan::weekday_of(date);
    let generation = l.company.generation;
    let fresh = l.company.planning.studio.day.as_ref().is_some_and(|d| d.0 == date && d.1 == generation);
    if !fresh {
        // (the day's timetable, read on the planning's thread)
        let Some(lines) = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).map(|d| d.2.clone()) else {
            let inner = super::super::section(&mut l.ui, area, "Duties");
            l.ui.text_in("Reading the timetable…", Rect::new(inner.x, inner.y, inner.w, 24.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
            return;
        };
        let lines = match lines {
            Ok(x) => x,
            Err(e) => {
                let inner = super::super::section(&mut l.ui, area, "Duties");
                l.ui.paragraph(&e, Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, DANGER.lighten(0.2));
                return;
            }
        };
        // (a layout the timetable no longer fits: back to automatic for its line, said)
        if !st::stale(c, &lines, date).is_empty() {
            let d = date.to_string();
            if let Some(gone) = act(l, |c| Ok(st::retire_stale(c, &lines, &d))) {
                if !gone.is_empty() {
                    l.state.set_status(omsi_ui::tr("The timetable changed: line %{n} runs with the automatic blocks and duties again.").replace("%{n}", &gone.join(", ")), true);
                }
            }
            return;
        }
        let tours = co::network::tours_of_day(c, &lines, date);
        let day = make_day(c, wd, tours, &c.planning.joined);
        let s = &mut l.company.planning.studio;
        s.day = Some((date.to_string(), generation, day));
        s.press = None;
        s.grab = None;
        s.asked = None;
        if s.proposal.as_ref().is_some_and(|p| p.0 != date || p.1 != generation) {
            s.proposal = None;
        }
    }
    let Some((_, _, day)) = l.company.planning.studio.day.clone() else { return };
    let proposal = l.company.planning.studio.proposal.as_ref().map(|p| p.2.clone());
    let before = l.company.planning.studio.before.as_ref().filter(|b| b.0 == date).map(|b| b.1);
    // the figures: the proposal's against the day's, else the day's against before the edit
    let bar = Rect::new(area.x, area.y, area.w, 64.0);
    match &proposal {
        Some(p) => figures(&mut l.ui, bar, &p.kpis, Some(&day.kpis)),
        None => figures(&mut l.ui, bar, &day.kpis, before.as_ref()),
    }
    let mut chart = Rect::new(area.x, bar.bottom() + 12.0, area.w, (area.bottom() - bar.bottom() - 12.0).max(0.0));
    if proposal.is_some() {
        let banner = Rect::new(chart.x, chart.y, chart.w, 44.0);
        chart = Rect::new(chart.x, banner.bottom() + 8.0, chart.w, (chart.h - 52.0).max(0.0));
        if proposal_banner(l, banner, c, wd) {
            return;
        }
    }
    let shown = proposal.as_ref().unwrap_or(&day);
    gantt(l, chart, c, wd, shown, drivers, proposal.is_some());
}

/// The figures on top: buses at the busiest, duties, paid driver hours, empty kilometres, the
/// day's cost - each with its difference against `against` (lower is better for them all).
fn figures(ui: &mut Ui, r: Rect, k: &Kpis, against: Option<&Kpis>) {
    let gap = 10.0;
    let w = (r.w - gap * 4.0) / 5.0;
    let items: [(&str, String, Option<f64>, String); 5] = [
        ("Buses at the peak", k.buses.to_string(), against.map(|a| k.buses as f64 - a.buses as f64), String::new()),
        ("Duties", k.duties.to_string(), against.map(|a| k.duties as f64 - a.duties as f64), String::new()),
        ("Paid driver hours", length(k.paid), against.map(|a| (k.paid - a.paid) as f64), "time".into()),
        ("Deadhead km", format!("{:.0} km", k.deadhead_km), against.map(|a| k.deadhead_km - a.deadhead_km), "km".into()),
        ("Cost of the day", eur(k.cost), against.map(|a| (k.cost - a.cost) as f64), "money".into()),
    ];
    for (i, (label, value, delta, unit)) in items.iter().enumerate() {
        let cell = Rect::new(r.x + i as f32 * (w + gap), r.y, w, r.h);
        ui.p().rounded(cell, 12.0, NAVY);
        kit::caps(ui, Rect::new(cell.x + 14.0, cell.y + 10.0, cell.w - 28.0, 14.0), label);
        let vw = ui.text_in(value, Rect::new(cell.x + 14.0, cell.y + 28.0, cell.w - 28.0, 28.0), 21.0, Weight::Bold, TEXT, Align::Left);
        if let Some(d) = delta.filter(|d| d.abs() >= 0.5) {
            let text = match unit.as_str() {
                "time" => format!("{}{}", if d > 0.0 { "+" } else { "−" }, length(d.abs() as i32)),
                "km" => format!("{}{:.0} km", if d > 0.0 { "+" } else { "−" }, d.abs()),
                "money" => format!("{}{}", if d > 0.0 { "+" } else { "−" }, eur(d.abs() as co::Cents)),
                _ => format!("{}{}", if d > 0.0 { "+" } else { "−" }, d.abs() as i64),
            };
            let col = if d > 0.0 { WARN } else { OK };
            let x = cell.x + 14.0 + vw + 10.0;
            ui.text_in(&text, Rect::new(x, cell.y + 30.0, (cell.right() - x - 10.0).max(0.0), 26.0), 13.0, Weight::Bold, col, Align::Left);
        }
    }
}

/// "Optimise"'s proposal over the chart: accept it, or keep the day as it is. Returns whether
/// the page changed under it (nothing more to draw this frame).
fn proposal_banner(l: &mut Launcher, r: Rect, c: &Company, wd: u8) -> bool {
    l.ui.p().rounded(r, 10.0, accent().alpha(0.14));
    l.ui.p().rounded_border(r, 10.0, 1.0, accent().alpha(0.5));
    l.ui.icon("bolt", Vec2::new(r.x + 22.0, r.center().y), 18.0, accent());
    let kw = kit::Foot::width(&l.ui, "Keep mine", None);
    let aw = kit::Foot::width(&l.ui, "Accept", Some("check"));
    let keep_r = Rect::new(r.right() - 8.0 - kw, r.y + 6.0, kw, r.h - 12.0);
    let accept_r = Rect::new(keep_r.x - 8.0 - aw, r.y + 6.0, aw, r.h - 12.0);
    let text = if st::edited(c, wd) { "Proposal: the automatic blocks and duties. The figures show it against yours." } else { "Your day is the automatic one already: the proposal is the same." };
    l.ui.paragraph(text, Vec2::new(r.x + 42.0, r.y + 13.0), (accept_r.x - r.x - 54.0).max(0.0), kit::NOTE + 0.5, Weight::Medium, TEXT);
    let accept = l.ui.button("studio-accept", accept_r, "Accept", Some("check"), ButtonKind::Primary);
    let keep = l.ui.button("studio-keep", keep_r, "Keep mine", None, ButtonKind::Normal);
    if accept {
        l.company.planning.studio.proposal = None;
        if edit(l, |c| {
            st::reset_day(c, wd);
            Ok(())
        }) {
            l.state.set_status(omsi_ui::tr("The automatic blocks and duties are taken: change them by hand from here.").into_owned(), false);
        }
        return true;
    }
    if keep {
        l.company.planning.studio.proposal = None;
        return true;
    }
    false
}

/// The Gantt: the time axis on top, a row per block (or per duty), what is taken up carried
/// to where it would land.
fn gantt(l: &mut Launcher, r: Rect, c: &Company, wd: u8, day: &Day, drivers: bool, read_only: bool) {
    let ui = &mut l.ui;
    ui.p().shadow(r, 14.0, 16.0, Color::rgba(0, 0, 0, 0.25));
    ui.p().rounded(r, 14.0, NAVY);
    if day.tours.is_empty() {
        ui.paragraph("No tours on this day: the company runs no line yet, or its lines do not run on this day.", Vec2::new(r.x + 20.0, r.y + 20.0), r.w - 40.0, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    ui.push_clip(r, 14.0);
    ui.p().rect(Rect::new(r.x, r.y, r.w, AXIS), NAVY_HEAD);
    let (t0, t1) = day_span(&day.tours);
    let total = (t1 - t0) as f32;
    let gx = r.x + LABELS;
    let gw = (r.w - LABELS - 18.0).max(60.0);
    // the zoom (Ctrl and the wheel, or the buttons) and the panning (Shift and the wheel)
    let s = &mut l.company.planning.studio;
    if s.zoom < 1.0 {
        s.zoom = 1.0;
    }
    let over = r.contains(ui.input.mouse);
    if over && ui.input.ctrl && ui.input.wheel.y.abs() > 0.0 {
        let at = t0 as f32 + s.pan as f32 + (ui.input.mouse.x - gx) / (gw / (total / s.zoom));
        s.zoom = (s.zoom * if ui.input.wheel.y > 0.0 { 1.25 } else { 0.8 }).clamp(1.0, 8.0);
        let ppm = gw / (total / s.zoom);
        s.pan = (at - t0 as f32 - (ui.input.mouse.x - gx) / ppm).round() as i32;
        ui.input.wheel = Vec2::ZERO;
    } else if over && (ui.input.shift || ui.input.wheel.x.abs() > 0.0) && ui.input.wheel.length() > 0.0 {
        let d = if ui.input.wheel.x.abs() > 0.0 { -ui.input.wheel.x } else { -ui.input.wheel.y };
        s.pan += (d * total / s.zoom / 12.0).round() as i32;
        ui.input.wheel = Vec2::ZERO;
    }
    let visible = total / s.zoom;
    s.pan = s.pan.clamp(0, (total - visible).max(0.0) as i32);
    let (zoom, pan) = (s.zoom, s.pan);
    let ppm = gw / visible;
    let x_of = move |m: i32| gx + (m - t0 - pan) as f32 * ppm;
    // the zoom's buttons, left in the axis band
    let zb = Vec2::new(r.x + 18.0, r.y + AXIS * 0.5);
    let mut zoom_to: Option<f32> = None;
    if ui.icon_button("studio-zoom-out", zb, 11.0, "zoom_out", "Show more of the day (Ctrl and the wheel)") {
        zoom_to = Some((zoom / 1.5).max(1.0));
    }
    if ui.icon_button("studio-zoom-in", zb + Vec2::new(28.0, 0.0), 11.0, "zoom_in", "Show the hours wider (Ctrl and the wheel; Shift and the wheel moves along the day)") {
        zoom_to = Some((zoom * 1.5).min(8.0));
    }
    kit::caps(ui, Rect::new(r.x + 62.0, r.y + 10.0, LABELS - 70.0, 14.0), if drivers { "Duties" } else { "Buses" });
    // the hours: small grey caps, faint lines down the rows
    let every = if ppm * 60.0 >= 54.0 { 1 } else if ppm * 60.0 >= 27.0 { 2 } else { 3 };
    ui.push_clip(Rect::new(gx - 30.0, r.y, gw + 48.0, r.h), 0.0);
    for h in (t0 / 60..=t1 / 60).filter(|h| h % every == 0) {
        let x = x_of(h * 60);
        if x < gx - 1.0 || x > gx + gw + 1.0 {
            continue;
        }
        ui.text_in(&format!("{:02}:00", h % 24), Rect::new(x - 26.0, r.y, 52.0, AXIS), 11.0, Weight::Bold, TEXT_DIM, Align::Center);
        ui.p().rect(Rect::new(x, r.y + AXIS, 1.0, r.h - AXIS), Color::WHITE.alpha(0.045));
    }
    ui.pop_clip();
    let rows = Rect::new(r.x, r.y + AXIS + 4.0, r.w, r.h - AXIS - 8.0);
    let grab = l.company.planning.studio.grab;
    let chosen = l.company.planning.studio.chosen;
    let mut targets: Vec<(Rect, Drop)> = Vec::new();
    let mut lit: Option<(i32, i32)> = None;
    let mut pressed: Option<(Vec2, Grab, f32)> = None;
    let mut tapped: Option<(usize, usize)> = None;
    let mut intents: Vec<Intent> = Vec::new();
    let mut grab_rect: Option<(Rect, Color, String)> = None;
    let clip = Rect::new(gx - 6.0, rows.y, gw + 12.0, rows.h);
    let mouse = ui.input.mouse;
    ui.scroll_area("studio-rows", rows, &mut |ui, v| {
        let mut y = v.y;
        let n_rows = if drivers { day.duties.len() } else { day.rows.len() };
        for row_k in 0..n_rows {
            let row = Rect::new(v.x, y, v.w, ROW);
            y += ROW;
            if row_k % 2 == 1 {
                ui.p().rect(row, Color::WHITE.alpha(0.022));
            }
            let cy = row.center().y;
            // what the row holds: (block, its trips' range, its duty) pieces
            let pieces: Vec<(usize, std::ops::Range<usize>, usize)> = if drivers {
                day.duties[row_k].pieces.iter().map(|p| (p.tour, co::network::duties_of(&day.tours[p.tour]).get(p.duty).cloned().unwrap_or(0..0), p.duty)).collect()
            } else {
                let ti = day.rows[row_k];
                co::network::duties_of(&day.tours[ti]).into_iter().enumerate().map(|(k, rg)| (ti, rg, k)).collect()
            };
            let drop = if drivers { Drop::Duty(row_k) } else { Drop::Block(day.rows[row_k]) };
            targets.push((row, drop));
            if !ui.rect_visible(row) {
                continue;
            }
            // the row's name
            let label = Rect::new(row.x + 14.0, row.y, LABELS - 22.0, row.h);
            if drivers {
                let d = &day.duties[row_k];
                ui.text_in(&format!("{} {}", omsi_ui::tr("Duty"), row_k + 1), Rect::new(label.x, label.y, 70.0, label.h), 12.5, Weight::Bold, TEXT, Align::Left);
                ui.text_in(&length(d.paid()), Rect::new(label.x + 64.0, label.y, 60.0, label.h), 11.5, Weight::Medium, TEXT_DIM, Align::Left);
                badge(ui, Vec2::new(label.right() - 30.0, cy), &d.rules);
                if d.pieces.len() > 1 && !read_only && ui.icon_button(&format!("studio-unjoin-{row_k}"), Vec2::new(label.right() - 8.0, cy), 10.0, "unfold_more", "Take the duty apart: each piece a duty of its own") {
                    intents.push(Intent::Unjoin(d.pieces[0].tour, d.pieces[0].duty));
                }
            } else {
                let t = &day.tours[day.rows[row_k]];
                let colour = line_colour(c, &t.line);
                let plate = Rect::new(label.x, cy - 9.0, (ui.width(&t.number, 11.0, Weight::Black) + 12.0).max(26.0), 18.0);
                ui.swatch(plate, 4.0, colour);
                ui.as_drawn();
                ui.text_in(&t.number, plate, 11.0, Weight::Black, super::super::super::ownlines::ink_on(colour), Align::Center);
                ui.end_as_drawn();
                let name = format!("{} {}", omsi_ui::tr("Tour"), t.tour);
                ui.text_in(&name, Rect::new(plate.right() + 8.0, label.y, label.right() - plate.right() - 40.0, label.h), 12.5, Weight::Bold, if t.unplanned { TEXT_DIM } else { TEXT }, Align::Left);
                let rules: Vec<Rule> = (0..pieces.len()).filter_map(|k| st::duty_of(&day.duties, day.rows[row_k], k)).flat_map(|d| day.duties[d].rules.iter().copied()).collect();
                badge(ui, Vec2::new(label.right() - 8.0, cy), &rules);
            }
            ui.push_clip(clip, 0.0);
            let mut prev_end: Option<(i32, f32)> = None;
            for (ti, rg, k) in pieces.iter() {
                let t = &day.tours[*ti];
                let colour = line_colour(c, &t.line);
                let trips = &t.trips[rg.clone()];
                // (the Drivers view: the break before this piece, "Split" where it is long)
                if let (true, Some((end, ex))) = (drivers, prev_end) {
                    let a = ex;
                    let b = x_of(trips.first().map(|x| x.dep).unwrap_or(end));
                    ui.p().rect(Rect::new(a + 2.0, cy - 0.5, (b - a - 4.0).max(0.0), 1.0), Color::WHITE.alpha(0.16));
                    let gap = trips.first().map(|x| x.dep).unwrap_or(end) - end;
                    if gap >= st::SPLIT_BREAK && b - a > 44.0 {
                        let w = ui.width(&omsi_ui::tr("Split"), 10.5, Weight::Medium) + 10.0;
                        let lr = Rect::new((a + b) * 0.5 - w * 0.5, cy - 8.0, w, 16.0);
                        ui.p().rect(lr, NAVY.alpha(if row_k % 2 == 1 { 0.97 } else { 1.0 }));
                        ui.text_in("Split", lr, 10.5, Weight::Medium, TEXT_FAINT, Align::Center);
                    }
                }
                let first_empty = trips.first().is_some_and(|x| x.empty);
                let last_empty = trips.last().is_some_and(|x| x.empty);
                // a block's ends: the depot boxes (implied where the timetable has no run)
                let block_start = rg.start == 0;
                let block_end = rg.end == t.trips.len();
                if block_start && !first_empty {
                    if let Some(f) = trips.first() {
                        let x = x_of(f.dep);
                        depot_box(ui, x - 16.0, x - 2.0, cy, true);
                    }
                }
                for (j, x) in trips.iter().enumerate() {
                    let idx = rg.start + j;
                    let a = x_of(x.dep);
                    let b = x_of(x.arr).max(a + 4.0);
                    // the gap before it: an empty run between places, a relief point to cut at
                    if j > 0 || (!drivers && idx > 0) {
                        let p = &t.trips[idx - 1];
                        let pa = x_of(p.arr);
                        if !p.end().trim().eq_ignore_ascii_case(x.from.trim()) {
                            ui.p().rect(Rect::new(pa + 1.0, cy - 0.75, (a - pa - 2.0).max(0.0), 1.5), Color::WHITE.alpha(0.28));
                        }
                    }
                    if x.empty && ((idx == 0) || (idx + 1 == t.trips.len())) {
                        depot_box(ui, a, b, cy, false);
                        ui.tooltip(Rect::new(a, cy - CHIP_H * 0.5, (b - a).max(14.0), CHIP_H), &format!("{}  {} → {}  {}", hhmm(x.dep), x.from, x.to, hhmm(x.arr)));
                        continue;
                    }
                    if x.empty || !x.counts() {
                        // (an empty run in the day: a thin line)
                        ui.p().rect(Rect::new(a, cy - 0.75, (b - a).max(2.0), 1.5), Color::WHITE.alpha(0.35));
                        ui.tooltip(Rect::new(a, cy - 6.0, (b - a).max(4.0), 12.0), &format!("{}  {} → {}  {}", hhmm(x.dep), x.from, x.to, hhmm(x.arr)));
                        continue;
                    }
                    let chip = Rect::new(a + 1.0, cy - CHIP_H * 0.5, (b - a - 3.0).max(5.0), CHIP_H);
                    let id = id_of(&format!("studio-trip-{ti}-{idx}-{row_k}"));
                    let (h, _, click) = ui.interact(id, chip);
                    let taken = match grab {
                        Some(Grab::Trip { tour, trip }) => tour == *ti && trip == idx,
                        Some(Grab::Piece { tour, duty }) => tour == *ti && duty == *k,
                        None => false,
                    };
                    let on = chosen == Some((*ti, idx)) || (h && grab.is_none());
                    if on {
                        lit = Some((x.dep, x.arr));
                    }
                    if taken {
                        // (where it was: faint; it is carried at the mouse)
                        arrow_outline(ui, chip, Color::WHITE.alpha(0.25));
                        if grab_rect.is_none() || matches!(grab, Some(Grab::Piece { .. })) {
                            let label = trip_label(t, x, &day.tours);
                            match &mut grab_rect {
                                Some((gr, _, _)) => *gr = Rect::new(gr.x.min(chip.x), gr.y, chip.right().max(gr.right()) - gr.x.min(chip.x), gr.h),
                                None => grab_rect = Some((chip, colour, label)),
                            }
                        }
                        continue;
                    }
                    trip_chip(ui, chip, colour, on, &trip_label(t, x, &day.tours));
                    if h && ui.input.pressed && !read_only {
                        let what = if drivers { Grab::Piece { tour: *ti, duty: *k } } else { Grab::Trip { tour: *ti, trip: idx } };
                        pressed = Some((mouse, what, mouse.y - chip.y));
                    }
                    if click {
                        tapped = Some((*ti, idx));
                    }
                    ui.tooltip(chip, &trip_tip(t, x));
                }
                if block_end && !last_empty {
                    if let Some(z) = trips.last() {
                        let x = x_of(z.arr);
                        depot_box(ui, x + 2.0, x + 16.0, cy, true);
                    }
                }
                // the gaps between this piece's trips (and, in the Vehicles view, the cut before
                // it): tapped to cut there, or to join the duties again
                if !read_only {
                    let from = if drivers { rg.start + 1 } else { rg.start.max(1) };
                    for at in from..rg.end.max(from) {
                        if at == 0 || at >= t.trips.len() {
                            continue;
                        }
                        let (p, n) = (&t.trips[at - 1], &t.trips[at]);
                        let (a, b) = (x_of(p.arr), x_of(n.dep));
                        let w = (b - a).max(8.0);
                        let gap = Rect::new((a + b) * 0.5 - w * 0.5, cy - CHIP_H * 0.5 - 3.0, w, CHIP_H + 6.0);
                        let is_cut = at == rg.start;
                        let (h, _, click) = ui.interact(id_of(&format!("studio-gap-{ti}-{at}-{row_k}")), gap);
                        if is_cut {
                            // a driver changes here
                            let mx = (a + b) * 0.5;
                            ui.p().rect(Rect::new(mx - 1.0, cy - CHIP_H * 0.5 - 2.0, 2.0, CHIP_H + 4.0), if h { accent() } else { Color::WHITE.alpha(0.55) });
                            if b - a > 40.0 {
                                ui.text_in("Split", Rect::new(a, cy - CHIP_H * 0.5 - 14.0, b - a, 12.0), 9.5, Weight::Medium, TEXT_FAINT, Align::Center);
                            }
                            ui.tooltip(gap, &omsi_ui::tr("A driver takes over here, at %{stop}. Tap to join the two duties.").replace("%{stop}", n.from.trim()));
                            if click {
                                intents.push(Intent::Uncut(*ti, at));
                            }
                        } else if h && grab.is_none() {
                            match st::can_cut(c, &day.tours, *ti, at) {
                                Ok(()) => {
                                    let mx = (a + b) * 0.5;
                                    ui.p().rect(Rect::new(mx - 1.0, cy - CHIP_H * 0.5 - 2.0, 2.0, CHIP_H + 4.0), accent().alpha(0.8));
                                    ui.tooltip(gap, &omsi_ui::tr("Tap to cut the duty here: a driver takes over at %{stop}.").replace("%{stop}", n.from.trim()));
                                    if click {
                                        intents.push(Intent::Cut(*ti, at));
                                    }
                                }
                                Err(why) => ui.tooltip(gap, why),
                            }
                        }
                    }
                }
                prev_end = trips.iter().map(|x| x.arr).max().map(|e| (e, x_of(e)));
            }
            ui.pop_clip();
        }
        // the row a trip is dropped on to run with a new bus
        if !drivers && !read_only {
            let row = Rect::new(v.x, y + 4.0, v.w, ROW);
            y += ROW + 8.0;
            targets.push((row, Drop::New));
            let on = grab.is_some() && row.contains(mouse);
            let line = Rect::new(row.x + 10.0, row.y + 4.0, row.w - 20.0, row.h - 8.0);
            ui.p().rounded_border(line, 8.0, 1.0, if on { accent() } else { Color::WHITE.alpha(0.10) });
            ui.icon("add", Vec2::new(line.x + 18.0, line.center().y), 14.0, if on { accent() } else { TEXT_DIM });
            let words = if grab.is_some() { "Drop here: the trip runs with a new bus" } else { "New bus: drag a trip here" };
            ui.text_in(words, Rect::new(line.x + 34.0, line.y, line.w - 40.0, line.h), 12.0, Weight::Medium, if on { TEXT } else { TEXT_DIM }, Align::Left);
        }
        y - v.y + 8.0
    });
    // the time chosen or under the mouse: a column down the chart
    if let Some((a, b)) = lit {
        let (xa, xb) = (x_of(a).max(gx - 4.0), x_of(b).min(gx + gw + 4.0));
        if xb > xa {
            ui.p().rect(Rect::new(xa, r.y, xb - xa, r.h), Color::WHITE.alpha(0.05));
            ui.p().rect(Rect::new(xa, r.y, xb - xa, AXIS), Color::WHITE.alpha(0.06));
        }
    }
    // what is carried, at the mouse, and where it would land
    let mut drop_now: Option<(Drop, Result<(), &'static str>)> = None;
    if let (Some(g), Some((gr, colour, label))) = (grab, grab_rect.clone()) {
        let target = targets.iter().find(|t| t.0.contains(mouse) && rows.contains(mouse)).map(|t| (t.0, t.1));
        if let Some((row, drop)) = target {
            let s = &l.company.planning.studio;
            let answer = match &s.asked {
                Some((ag, ad, res)) if *ag == g && *ad == drop => *res,
                _ => ask(c, wd, day, g, drop),
            };
            l.company.planning.studio.asked = Some((g, drop, answer));
            let ui = &mut l.ui;
            let col = if answer.is_ok() { OK } else { DANGER };
            let ghost = Rect::new(gr.x, row.center().y - CHIP_H * 0.5, gr.w, CHIP_H);
            ui.push_clip(clip, 0.0);
            arrow_outline(ui, ghost, col);
            ui.pop_clip();
            if let Err(why) = answer {
                let words = omsi_ui::tr(why).into_owned();
                let w = (ui.width(&words, 12.0, Weight::Medium) + 20.0).min(420.0);
                let tip = Rect::new((mouse.x + 16.0).min(r.right() - w - 8.0), mouse.y + 18.0, w, 26.0);
                ui.p().rounded(tip, 6.0, DANGER.darken(0.35));
                ui.text_in(&words, tip.pad(10.0, 0.0), 12.0, Weight::Medium, Color::WHITE, Align::Left);
            }
            drop_now = Some((drop, answer));
        }
        let ui = &mut l.ui;
        let dy = l.company.planning.studio.grab_dy;
        let carried = Rect::new(gr.x, mouse.y - dy, gr.w, CHIP_H);
        ui.p().shadow(carried, 4.0, 10.0, Color::rgba(0, 0, 0, 0.45));
        trip_chip(ui, carried, colour, true, &label);
    }
    l.ui.pop_clip();
    // the hand: pressed, then carried once it moved; let go over a row
    let s = &mut l.company.planning.studio;
    if let Some(z) = zoom_to {
        s.zoom = z;
    }
    if let Some((at, g, dy)) = pressed {
        s.press = Some((at, g));
        s.grab_dy = dy;
    }
    let input = &l.ui.input;
    let mut done: Option<(Grab, Drop)> = None;
    if let Some((at, g)) = s.press {
        if !input.down {
            s.press = None;
        } else if (input.mouse - at).length() > 6.0 {
            s.press = None;
            s.grab = Some(g);
            s.chosen = None;
        }
    }
    if let Some(g) = s.grab {
        if input.released {
            s.grab = None;
            s.asked = None;
            if let Some((drop, _)) = drop_now {
                done = Some((g, drop));
            }
        } else if !input.down {
            s.grab = None;
        }
    }
    if let Some(t) = tapped.filter(|_| s.grab.is_none()) {
        s.chosen = if s.chosen == Some(t) { None } else { Some(t) };
    }
    // the edits
    let tours = day.tours.clone();
    if let Some((g, drop)) = done {
        match (g, drop) {
            (Grab::Trip { tour, trip }, Drop::Block(b)) if b != tour => {
                edit(l, |c| st::move_trip(c, wd, &tours, tour, trip, Target::Block(b)));
            }
            (Grab::Trip { tour, trip }, Drop::New) => {
                edit(l, |c| st::move_trip(c, wd, &tours, tour, trip, Target::New));
            }
            (Grab::Piece { tour, duty }, Drop::Duty(row)) => {
                if let Some(p) = day.duties.get(row).and_then(|d| d.pieces.first()) {
                    let a = (p.tour, p.duty);
                    if st::duty_of(&day.duties, tour, duty) != Some(row) {
                        edit(l, |c| st::join(c, wd, &tours, a, (tour, duty)));
                    }
                }
            }
            _ => {}
        }
    }
    for i in intents {
        match i {
            Intent::Cut(t, at) => edit(l, |c| st::cut(c, wd, &tours, t, at)),
            Intent::Uncut(t, at) => edit(l, |c| st::uncut(c, wd, &tours, t, at)),
            Intent::Unjoin(t, k) => edit(l, |c| st::unjoin(c, wd, &tours, (t, k))),
        };
    }
}

/// Whether what is taken up may go where it is over (nothing kept).
fn ask(c: &Company, wd: u8, day: &Day, g: Grab, drop: Drop) -> Result<(), &'static str> {
    match (g, drop) {
        (Grab::Trip { tour, trip }, Drop::Block(b)) => st::plan_move(c, wd, &day.tours, tour, trip, Target::Block(b)).map(|_| ()),
        (Grab::Trip { tour, trip }, Drop::New) => st::plan_move(c, wd, &day.tours, tour, trip, Target::New).map(|_| ()),
        (Grab::Piece { tour, duty }, Drop::Duty(row)) => {
            let Some(p) = day.duties.get(row).and_then(|d| d.pieces.first()) else { return Err(st::NOT_FOUND) };
            if st::duty_of(&day.duties, tour, duty) == Some(row) {
                return Err(st::SAME_DUTY);
            }
            st::can_join(c, wd, &day.tours, (p.tour, p.duty), (tour, duty))
        }
        _ => Err(st::NOT_FOUND),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_of_the_studio_is_translated() {
        // (the core's texts are no literals of the pages: `locale_check` cannot see them)
        let places = |s: &str| {
            let mut v: Vec<String> = s.match_indices("%{").filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_string())).collect();
            v.sort();
            v
        };
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for t in st::texts() {
                let Some(x) = crate::_rust_i18n_try_translate(lang, t) else { panic!("{lang}: no translation of {t:?}") };
                assert_eq!(places(&x), places(t), "{lang}: {t:?} -> {x:?}");
            }
        }
    }

    #[test]
    fn a_trip_is_labelled_with_its_line_and_direction() {
        let trip = |a: &str, b: &str| PlannedTrip { line: "4".into(), from: a.into(), to: b.into(), stops: 6, ..Default::default() };
        let t = TourOfDay { line: "Linie4".into(), number: "4".into(), tour: "1".into(), trips: vec![trip("Nord", "Süd"), trip("Süd", "Nord")], ..Default::default() };
        let tours = vec![t.clone()];
        assert_eq!(trip_label(&t, &t.trips[0], &tours), "4-B");
        assert_eq!(trip_label(&t, &t.trips[1], &tours), "4-A");
        assert_eq!(day_span(&[]), (300, 1380));
        // the arrow: flat left, its point on the right in the middle
        let p = arrow_points(Rect::new(10.0, 20.0, 60.0, 20.0));
        assert_eq!((p[0], p[2]), (Vec2::new(10.0, 20.0), Vec2::new(70.0, 30.0)));
    }
}

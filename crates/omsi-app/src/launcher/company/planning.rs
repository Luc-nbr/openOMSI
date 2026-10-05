//! The company's planning (Omsi-Hub's Planning page, `busbedrijf-planning.md`): a strip of
//! the next seven days, and the chosen day as a Gantt chart - the tours grouped by line, each
//! tour's duties as blocks coloured by who drives them, the bus beside the tour. A driver or
//! a bus is given by tapping it in the side list and then the duty or the tour, or by
//! dragging it there; tapping a duty or a tour shows who could take it. What is given goes
//! into the weekly roster, which repeats every week (`co::plan`); what fell out on the
//! company's own day is filled for that day only. The tools fill the roster as the
//! dispatcher would, clear the day, or repeat it on the other working days. "Drive this
//! duty" opens the Drive page with exactly that duty.

use super::super::flow::Step;
use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind, Key, Ui};
use super::super::{Launcher, Page};
use super::{act, day_label, eur, line_plate, section};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, DayTour, Disruption, Fill, Problem, Source, Warn, Who};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::sync::mpsc::{channel, Receiver, Sender};

/// A day's timetable as it was read: map, date, its lines.
type Read = (String, String, Result<Vec<core::LineInfo>, String>);

/// A driver or a bus taken up to be given (tapped, or dragged).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Arm {
    Driver(Who),
    Bus(u32),
}

#[derive(Clone, PartialEq, Debug)]
enum Sel {
    Tour(String, String),
    Duty(String, String, usize),
}

pub struct PlanningView {
    tx: Sender<Read>,
    rx: Receiver<Read>,
    days: Vec<Read>,
    asked: Vec<(String, String)>,
    /// The day shown: 0 the company's day, up to 6.
    pub(super) day: usize,
    sel: Option<Sel>,
    arm: Option<Arm>,
    /// Pressed on a driver or a bus of the list: dragged while the button is held.
    drag: Option<Arm>,
    /// "Clear the day" pressed once: pressed again it clears.
    clear_armed: bool,
    /// The plans made: date, the company's generation they were made for, the plan.
    cache: Vec<(String, u64, DayPlan)>,
}

impl Default for PlanningView {
    fn default() -> Self {
        let (tx, rx) = channel();
        PlanningView { tx, rx, days: Vec::new(), asked: Vec::new(), day: 0, sel: None, arm: None, drag: None, clear_armed: false, cache: Vec::new() }
    }
}

/// Where a block or a bus is in the chart (for dropping on it).
struct Hit {
    r: Rect,
    tour: usize,
    duty: Option<usize>,
}

fn hhmm(minutes: i32) -> String {
    super::super::state::hhmm(minutes as f64 * 60.0)
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

/// The colours drivers are told apart by (calm, and readable under dark ink).
const DRIVERS: [(u8, u8, u8); 10] = [(110, 168, 230), (96, 196, 170), (196, 150, 214), (226, 184, 110), (140, 190, 120), (226, 150, 130), (150, 206, 240), (200, 200, 120), (180, 160, 230), (120, 200, 210)];
const INK: Color = Color::rgba(14, 18, 28, 1.0);

fn driver_colour(c: &Company, id: u32) -> Color {
    let k = c.staff.iter().position(|e| e.id == id).unwrap_or(id as usize);
    let (r, g, b) = DRIVERS[k % DRIVERS.len()];
    Color::rgba(r, g, b, 1.0)
}

fn first_name(c: &Company, id: u32) -> String {
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_else(|| "?".into())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()),
        Some(Who::Player) => omsi_ui::tr("You").into_owned(),
        Some(Who::Agency) => omsi_ui::tr("Agency driver").into_owned(),
        None => omsi_ui::tr("Nobody").into_owned(),
    }
}

fn source_text(s: Source) -> &'static str {
    match s {
        Source::Roster => "fixed in the roster",
        Source::Auto => "the dispatcher's suggestion",
        Source::Dispatcher => "your choice for today",
        Source::Central => "filled by the central",
        Source::None => "",
    }
}

fn problem_colour(p: Option<Problem>) -> Color {
    match p {
        None | Some(Problem::Unassigned) => WARN,
        Some(_) => DANGER.lighten(0.2),
    }
}

fn line_title(t: &DayTour, duty: Option<usize>) -> String {
    let mut s = format!("{} {}  ·  {} {}", omsi_ui::tr("Line"), t.tour.number, omsi_ui::tr("Tour"), t.tour.tour);
    if let Some(k) = duty {
        s.push_str(&format!("  ·  {} {}", omsi_ui::tr("Duty"), k + 1));
    }
    s
}

// --- the data --------------------------------------------------------------------------------

/// Take in the timetables read, and ask for those of the week not read yet.
fn work(l: &mut Launcher, c: &Company) {
    let v = &mut l.company.planning;
    while let Ok(r) = v.rx.try_recv() {
        v.days.retain(|d| !(d.0 == r.0 && d.1 == r.1));
        v.days.push(r);
    }
    let week: Vec<(String, String)> = (0..7).map(|k| (c.map.clone(), co::dates::add(&c.date, k))).collect();
    v.days.retain(|d| week.iter().any(|w| w.0 == d.0 && w.1 == d.1));
    v.asked.retain(|a| week.contains(a));
    let missing: Vec<(String, String)> = week.into_iter().filter(|w| !v.asked.contains(w)).collect();
    if missing.is_empty() {
        return;
    }
    v.asked.extend(missing.iter().cloned());
    let tx = v.tx.clone();
    std::thread::spawn(move || {
        for (map, date) in missing {
            let r = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
            if tx.send((map, date, r)).is_err() {
                break;
            }
        }
    });
}

/// The plan of a day of the strip (None: its timetable is still being read).
fn plan_of(l: &mut Launcher, c: &Company, date: &str) -> Option<Result<DayPlan, String>> {
    let generation = l.company.generation;
    let v = &mut l.company.planning;
    if let Some(p) = v.cache.iter().find(|x| x.0 == date && x.1 == generation) {
        return Some(Ok(p.2.clone()));
    }
    let read = v.days.iter().find(|d| d.0 == c.map && d.1 == date)?;
    match &read.2 {
        Err(e) => Some(Err(e.clone())),
        Ok(lines) => {
            let tours = co::network::tours_of_day(c, lines);
            let p = pl::day_plan(c, date, tours, &[], &[], false);
            v.cache.retain(|x| x.1 == generation && x.0 != date);
            v.cache.push((date.to_string(), generation, p.clone()));
            Some(Ok(p))
        }
    }
}

/// The blocks a driver has on the plan's day, without one duty (to see whether it fits).
fn blocks_without(p: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    p.duties_of(Who::Staff(id))
        .into_iter()
        .filter(|x| *x != skip)
        .map(|(i, k)| {
            let t = &p.tours[i];
            let d = &t.duties[k];
            Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
        })
        .collect()
}

fn block_of(t: &DayTour, k: usize) -> Block {
    let d = &t.duties[k];
    Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
}

/// What a driver would make of a duty: a word and its colour.
fn fit_of(c: &Company, p: &DayPlan, ti: usize, k: usize, id: u32) -> (String, Color, bool) {
    let Some(e) = c.employee(id) else { return ("?".into(), TEXT_FAINT, false) };
    if !e.employed_on(&p.date) {
        return (omsi_ui::tr("Not employed then").into_owned(), TEXT_FAINT, false);
    }
    if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
        return (omsi_ui::tr("Ill").into_owned(), DANGER.lighten(0.2), false);
    }
    if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
        return (omsi_ui::tr("On holiday").into_owned(), TEXT_DIM, false);
    }
    let t = &p.tours[ti];
    let work = blocks_without(p, id, (ti, k));
    match pl::fits(e, t.size(c), &work, &block_of(t, k), p.today) {
        Err(Problem::Licence) => (omsi_ui::tr("No licence for this bus").into_owned(), DANGER.lighten(0.2), false),
        Err(_) => (omsi_ui::tr("Clashes with other work").into_owned(), DANGER.lighten(0.2), false),
        Ok(w) => match w.first() {
            Some(Warn::Overtime(m)) => (omsi_ui::tr("Overtime %{t}").replace("%{t}", &length(*m)), WARN, true),
            Some(Warn::Transfer(m)) => (omsi_ui::tr("%{n} min to change stops").replace("%{n}", &m.to_string()), WARN, true),
            Some(Warn::Experience) => (omsi_ui::tr("Little experience for this bus").into_owned(), WARN, true),
            None => (omsi_ui::tr("Fits").into_owned(), OK, true),
        },
    }
}

/// Where a bus is on the plan's day besides this tour: (word, colour, free).
fn bus_fit(c: &Company, p: &DayPlan, ti: usize, id: u32) -> (String, Color, bool) {
    let Some(v) = c.vehicle(id) else { return ("?".into(), TEXT_FAINT, false) };
    if !v.held_on(&p.date) {
        return (omsi_ui::tr("Gone back then").into_owned(), TEXT_FAINT, false);
    }
    if v.in_workshop(&p.date) || v.condition < 20.0 {
        return (omsi_ui::tr("In the workshop").into_owned(), DANGER.lighten(0.2), false);
    }
    let t = &p.tours[ti];
    let (a, b) = (t.tour.from(), t.tour.to());
    let other = p.tours.iter().enumerate().find(|(i, x)| *i != ti && x.bus == Some(BusOf::Own(id)) && a < x.tour.to() + co::staff::BUS_MARGIN && x.tour.from() < b + co::staff::BUS_MARGIN);
    if let Some((_, x)) = other {
        return (omsi_ui::tr("On tour %{t} then").replace("%{t}", &format!("{}/{}", x.tour.number, x.tour.tour)), WARN, false);
    }
    if t.tour.wants().is_some_and(|w| w != v.kind.size) {
        return (omsi_ui::tr("Another size than the tour asks for").into_owned(), WARN, true);
    }
    (omsi_ui::tr("Free").into_owned(), OK, true)
}

// --- giving --------------------------------------------------------------------------------

/// Give a duty its driver: into the roster, or - for one that fell out today with somebody
/// fixed for it - for today only.
fn give_driver(l: &mut Launcher, p: &DayPlan, ti: usize, k: usize, who: Option<Who>) {
    let Some(c) = l.company.company.as_ref() else { return };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.duties.get(k).copied().flatten());
    let today_only = p.today && t.duties[k].problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some() && who != Some(Who::Player);
    let wd = p.weekday;
    if today_only {
        let fill = match who {
            Some(Who::Staff(id)) => Some(Fill::Colleague { id }),
            Some(Who::Agency) => Some(Fill::Agency),
            _ => Some(Fill::Drop),
        };
        let key = pl::duty_key(&line, &tour, k);
        act(l, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        });
    } else {
        act(l, |c| {
            pl::set_driver(c, wd, &line, &tour, k, who);
            Ok(())
        });
    }
}

fn give_bus(l: &mut Launcher, p: &DayPlan, ti: usize, bus: Option<u32>) {
    let Some(c) = l.company.company.as_ref() else { return };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.bus);
    let today_only = p.today && t.bus_problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some();
    let wd = p.weekday;
    if today_only {
        let fill = Some(bus.map(|id| Fill::Bus { id }).unwrap_or(Fill::Drop));
        let key = pl::tour_key(&line, &tour);
        act(l, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        });
    } else {
        act(l, |c| {
            pl::set_bus(c, wd, &line, &tour, bus);
            Ok(())
        });
    }
}

fn set_fill(l: &mut Launcher, key: String, fill: Option<Fill>) {
    act(l, |c| {
        pl::set_fill(c, &key, fill);
        Ok(())
    });
}

/// What was taken up given to what is under it.
fn give(l: &mut Launcher, p: &DayPlan, arm: Arm, hit: &Hit) {
    match (arm, hit.duty) {
        (Arm::Driver(w), Some(k)) => give_driver(l, p, hit.tour, k, Some(w)),
        (Arm::Bus(id), None) => give_bus(l, p, hit.tour, Some(id)),
        // (a bus dropped on a duty: its tour's)
        (Arm::Bus(id), Some(_)) => give_bus(l, p, hit.tour, Some(id)),
        (Arm::Driver(_), None) => {}
    }
}

/// "Drive this duty": the Drive page with exactly this duty - its line and tour, its first
/// trip and as many trips as it has (`--duty-leg`), the company's map, day, depot and the
/// tour's bus in its paint. The player starts it there himself.
fn drive(l: &mut Launcher, c: &Company, p: &DayPlan, ti: usize, k: usize) {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    // the trip's place in its tour as the game counts it (1 the first): the day's timetable
    // has the tour's trips; the plan has them in the order they leave
    let index = l.company.planning.days.iter().find(|x| x.0 == c.map && x.1 == p.date).and_then(|x| x.2.as_ref().ok()).and_then(|lines| {
        let line = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&t.tour.line))?;
        let tour = line.tours.iter().find(|x| x.number.trim() == t.tour.tour.trim())?;
        let mut trips: Vec<&core::TripInfo> = tour.trips.iter().collect();
        trips.sort_by(|a, b| a.departure.partial_cmp(&b.departure).unwrap_or(std::cmp::Ordering::Equal));
        trips.get(d.start).map(|x| x.index)
    });
    let Some(first) = index else {
        l.state.set_status(omsi_ui::tr("The timetable of the day is still being read.").into_owned(), true);
        return;
    };
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let time = t.tour.trips[d.start].dep;
    let ch = &mut l.state.choice;
    ch.map = c.map.clone();
    ch.entry = -1;
    ch.date = c.date.clone();
    ch.free = false;
    ch.own_line = false;
    ch.composed = true;
    ch.line = Some(line.clone());
    ch.tour = Some(tour.clone());
    ch.time = time;
    ch.start_trip = Some((line.clone(), tour.clone(), first, time));
    ch.legs = vec![format!("{line}|{tour}|{first}|{}", d.end - d.start)];
    if let Some(v) = t.bus.and_then(|b| if let BusOf::Own(id) = b { c.vehicle(id) } else { None }).filter(|v| !v.bus.trim().is_empty()) {
        ch.bus = v.bus.clone();
        ch.paint = v.house_livery.clone().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| v.livery.clone());
        ch.plate = v.plate.clone();
    }
    if !c.depot.trim().is_empty() {
        ch.hof = c.depot.clone();
        ch.hof_manual = true;
    }
    l.state.touched();
    l.state.load_lines();
    l.state.load_ibis();
    l.go(Page::Drive);
    l.drive.step = Step::Duty;
    l.state.set_status(omsi_ui::tr("Your duty is set: check the bus and start the duty when you are ready.").into_owned(), false);
}

// --- the page ------------------------------------------------------------------------------

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    work(l, &c);
    let day = l.company.planning.day.min(6);
    let date = co::dates::add(&c.date, day as i64);
    week_strip(l, Rect::new(area.x, area.y, area.w, 52.0), &c);
    let plan = plan_of(l, &c, &date);
    let ty = area.y + 64.0;
    tools(l, Rect::new(area.x, ty, area.w, ROW), &c, &date);
    let body = Rect::new(area.x, ty + ROW + 14.0, area.w, (area.bottom() - ty - ROW - 14.0).max(0.0));
    let side_w = (body.w * 0.3).clamp(280.0, 360.0);
    let chart = Rect::new(body.x, body.y, (body.w - side_w - 16.0).max(200.0), body.h);
    let side = Rect::new(chart.right() + 16.0, body.y, side_w, body.h);
    match plan {
        None => {
            let inner = section(&mut l.ui, chart, "Duties");
            l.ui.text_in("Reading the timetable…", Rect::new(inner.x, inner.y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        }
        Some(Err(e)) => {
            let inner = section(&mut l.ui, chart, "Duties");
            l.ui.paragraph(&e, Vec2::new(inner.x, inner.y), inner.w, 13.0, Weight::Regular, DANGER.lighten(0.2));
        }
        Some(Ok(p)) => {
            let hits = gantt(l, chart, &c, &p);
            side_panel(l, side, &c, &p);
            // a driver or a bus dragged here and let go
            if l.ui.input.released {
                if let Some(arm) = l.company.planning.drag.take() {
                    let at = l.ui.input.mouse;
                    if let Some(h) = hits.iter().find(|h| h.r.contains(at)) {
                        give(l, &p, arm, h);
                    }
                }
            } else if l.ui.input.down {
                if let Some(arm) = l.company.planning.drag {
                    if !side.contains(l.ui.input.mouse) {
                        ghost(&mut l.ui, &c, arm);
                    }
                }
            } else {
                l.company.planning.drag = None;
            }
        }
    }
    if l.ui.input.keys.contains(&Key::Escape) {
        l.company.planning.arm = None;
        l.company.planning.sel = None;
    }
}

/// The driver or bus being dragged, at the mouse.
fn ghost(ui: &mut Ui, c: &Company, arm: Arm) {
    let (text, fill, ink) = match arm {
        Arm::Driver(Who::Staff(id)) => (first_name(c, id), driver_colour(c, id), INK),
        Arm::Driver(Who::Player) => (omsi_ui::tr("You").into_owned(), accent(), on_accent()),
        Arm::Driver(Who::Agency) => (omsi_ui::tr("Agency").into_owned(), TEXT_FAINT, TEXT),
        Arm::Bus(id) => (c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(), FIELD, TEXT),
    };
    let m = ui.input.mouse;
    let w = ui.width(&text, 12.0, Weight::Bold) + 20.0;
    let r = Rect::new(m.x + 10.0, m.y + 6.0, w, 24.0);
    ui.p().shadow(r, 6.0, 10.0, Color::rgba(0, 0, 0, 0.4));
    ui.p().rounded(r, 6.0, fill);
    ui.text_in(&text, r, 12.0, Weight::Bold, ink, Align::Center);
}

/// The next seven days, each with whether it is covered.
fn week_strip(l: &mut Launcher, r: Rect, c: &Company) {
    let gap = 8.0;
    let w = (r.w - gap * 6.0) / 7.0;
    for k in 0..7usize {
        let date = co::dates::add(&c.date, k as i64);
        let cell = Rect::new(r.x + k as f32 * (w + gap), r.y, w, r.h);
        let on = l.company.planning.day == k;
        l.ui.card(cell);
        if l.ui.row(&format!("plan-day-{k}"), cell, on) && !on {
            let v = &mut l.company.planning;
            v.day = k;
            v.sel = None;
            v.clear_armed = false;
        }
        let ink = if on { on_accent() } else { TEXT };
        let dim = if on { on_accent() } else { TEXT_DIM };
        let Some(d) = co::dates::parse(&date) else { continue };
        let (_, m, dd) = co::dates::civil_from_days(d);
        let top = if k == 0 { omsi_ui::tr("Company day").to_uppercase() } else { omsi_ui::tr(super::WEEKDAYS[co::dates::weekday(d) as usize]).to_uppercase() };
        l.ui.text_in(&top, Rect::new(cell.x + 12.0, cell.y + 7.0, cell.w - 24.0, 14.0), 10.0, Weight::Bold, dim, Align::Left);
        let big = format!("{} {} {}", omsi_ui::tr(super::WEEKDAYS[co::dates::weekday(d) as usize]), dd, omsi_ui::tr(super::MONTHS[(m as usize).clamp(1, 12) - 1]));
        l.ui.text_in(&big, Rect::new(cell.x + 12.0, cell.y + 22.0, cell.w - 24.0, 22.0), 14.0, Weight::Bold, ink, Align::Left);
        let status = plan_of(l, c, &date).and_then(|p| p.ok()).map(|p| p.counts());
        if let Some((tours, _, open, no_bus)) = status {
            let col = if tours == 0 { TEXT_FAINT } else if open + no_bus == 0 { OK } else { WARN };
            l.ui.p().circle(Vec2::new(cell.right() - 14.0, cell.y + 14.0), 4.0, col);
            if open + no_bus > 0 {
                l.ui.tooltip(cell, &omsi_ui::tr("%{n} open").replace("%{n}", &(open + no_bus).to_string()));
            }
        }
    }
}

/// The tools of the day: what the roster is, fill, clear, repeat; and the dispatcher's
/// switch.
fn tools(l: &mut Launcher, r: Rect, c: &Company, date: &str) {
    let wd = pl::weekday_of(date);
    let mut x = r.right();
    let bw = 170.0;
    x -= bw;
    let fill = l.ui.button("plan-fill", Rect::new(x, r.y, bw, r.h), "Fill the roster", Some("autorenew"), ButtonKind::Normal);
    l.ui.tooltip(Rect::new(x, r.y, bw, r.h), "Fix in the roster what the dispatcher would take: free buses, and free drivers by the working-time rules");
    x -= bw + 8.0;
    let clear_label = if l.company.planning.clear_armed { "Press again" } else { "Clear the day" };
    let clear = l.ui.button("plan-clear", Rect::new(x, r.y, bw, r.h), clear_label, Some("delete"), ButtonKind::Normal);
    x -= bw + 8.0;
    let (repeat_label, to): (&str, Vec<u8>) = if wd < 5 { ("Repeat Mon–Fri", (0..5).collect()) } else { ("Repeat on the weekend", vec![5, 6]) };
    let repeat = l.ui.button("plan-repeat", Rect::new(x, r.y, bw, r.h), repeat_label, Some("content_copy"), ButtonKind::Normal);
    l.ui.tooltip(Rect::new(x, r.y, bw, r.h), "This weekday's roster on the other days too (theirs is replaced)");
    let tw = 250.0;
    x -= tw + 16.0;
    let mut auto = c.planning.auto;
    if l.ui.toggle("plan-auto", Rect::new(x, r.y, tw, r.h), &mut auto, "Dispatcher fills the gaps") {
        act(l, |c| {
            c.planning.auto = auto;
            Ok(())
        });
    }
    let text = omsi_ui::tr("%{day}: the roster of this weekday repeats every week.").replace("%{day}", &day_label(date));
    l.ui.text_in(&text, Rect::new(r.x, r.y, (x - r.x - 12.0).max(0.0), r.h), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
    if fill {
        if let Some(lines) = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).and_then(|d| d.2.as_ref().ok()).cloned() {
            let d = date.to_string();
            if let Some(n) = act(l, |c| {
                let tours = co::network::tours_of_day(c, &lines);
                Ok(pl::fill_day(c, &d, tours))
            }) {
                let msg = if n == 0 { omsi_ui::tr("Nothing free to fix: the roster stays as it is.").into_owned() } else { omsi_ui::tr("%{n} buses and drivers fixed in the roster.").replace("%{n}", &n.to_string()) };
                l.state.set_status(msg, false);
            }
        }
    }
    if clear {
        if l.company.planning.clear_armed {
            l.company.planning.clear_armed = false;
            act(l, |c| {
                pl::clear_day(c, wd);
                Ok(())
            });
        } else {
            l.company.planning.clear_armed = true;
        }
    }
    if repeat {
        act(l, |c| {
            pl::copy_day(c, wd, &to);
            Ok(())
        });
    }
}

/// The day's tours as a Gantt chart: grouped by line, the bus left of each tour, its duties
/// as blocks on the hours. Returns where the duties and buses are.
fn gantt(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan) -> Vec<Hit> {
    l.ui.card(r);
    let inner = Rect::new(r.x + 14.0, r.y + 10.0, r.w - 24.0, r.h - 16.0);
    if p.tours.is_empty() {
        l.ui.paragraph("No tours on this day: the company runs no line yet, or its lines do not run on this day.", Vec2::new(inner.x, inner.y + 4.0), inner.w, 13.0, Weight::Regular, TEXT_DIM);
        return Vec::new();
    }
    let left = 220.0;
    let t0 = p.tours.iter().map(|t| t.tour.from()).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = (p.tours.iter().map(|t| t.tour.to()).max().unwrap_or(1440) + 59).div_euclid(60) * 60;
    let span = (t1 - t0).max(60) as f32;
    let gx = inner.x + left;
    let gw = (inner.w - left - 14.0).max(60.0);
    let ppm = (gw / span).min(64.0 / 60.0);
    let x_of = move |m: i32| gx + (m - t0) as f32 * ppm;
    // the hours
    let head = 26.0;
    let every = if ppm * 60.0 >= 34.0 { 1 } else if ppm * 60.0 >= 17.0 { 2 } else { 3 };
    let hours: Vec<i32> = (t0 / 60..=t1 / 60).filter(|h| h % every == 0).collect();
    for &h in &hours {
        let x = x_of(h * 60);
        l.ui.text_in(&format!("{:02}", h % 24), Rect::new(x - 14.0, inner.y, 28.0, 16.0), 10.5, Weight::Bold, TEXT_FAINT, Align::Center);
    }
    let armed = l.company.planning.arm;
    if let Some(a) = armed {
        let who = match a {
            Arm::Driver(w) => who_name(c, Some(w)),
            Arm::Bus(id) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        };
        let hint = match a {
            Arm::Driver(_) => omsi_ui::tr("Tap a duty to give it to %{who} (Esc: stop)."),
            Arm::Bus(_) => omsi_ui::tr("Tap a tour's bus to give it %{who} (Esc: stop)."),
        }
        .replace("%{who}", &who);
        l.ui.text_in(&hint, Rect::new(inner.x, inner.y - 2.0, left - 8.0, 18.0), 11.0, Weight::Bold, accent_2(), Align::Left);
    }
    let rows = Rect::new(inner.x, inner.y + head - 4.0, inner.w, inner.h - head + 4.0);
    // the lines in the company's order, each with its tours
    let mut groups: Vec<(Option<co::CompanyLine>, Vec<usize>)> = c.lines.iter().map(|x| (Some(x.clone()), Vec::new())).collect();
    for (i, t) in p.tours.iter().enumerate() {
        match groups.iter_mut().find(|g| g.0.as_ref().is_some_and(|x| x.name.eq_ignore_ascii_case(&t.tour.line))) {
            Some(g) => g.1.push(i),
            None => groups.push((None, vec![i])),
        }
    }
    groups.retain(|g| !g.1.is_empty());
    let sel = l.company.planning.sel.clone();
    let mut hits: Vec<Hit> = Vec::new();
    let mut clicked: Option<(usize, Option<usize>)> = None;
    l.ui.scroll_area("plan-gantt", rows, &mut |ui, v| {
        let mut y = v.y + 4.0;
        for (line, idx) in &groups {
            let hr = Rect::new(v.x, y, v.w - 10.0, 30.0);
            if ui.rect_visible(hr) {
                let mut x = hr.x;
                if let Some(cl) = line {
                    x += line_plate(ui, Vec2::new(hr.x, hr.y + 5.0), cl, 20.0) + 10.0;
                    let caption = if cl.caption.is_empty() { cl.name.clone() } else { cl.caption.clone() };
                    ui.text_in(&caption, Rect::new(x, hr.y, left - (x - hr.x) - 8.0 + gw, 30.0), 12.5, Weight::Bold, TEXT_SOFT, Align::Left);
                }
                let covered = idx.iter().filter(|&&i| p.tours[i].covered()).count();
                let text = omsi_ui::tr("%{c} of %{t} tours covered").replace("%{c}", &covered.to_string()).replace("%{t}", &idx.len().to_string());
                ui.text_in(&text, Rect::new(hr.right() - 220.0, hr.y, 216.0, 30.0), 11.0, Weight::Regular, if covered == idx.len() { TEXT_DIM } else { WARN }, Align::Right);
            }
            y += 32.0;
            for &ti in idx {
                let t = &p.tours[ti];
                let row = Rect::new(v.x, y, v.w - 10.0, 34.0);
                y += 36.0;
                if !ui.rect_visible(row) {
                    continue;
                }
                ui.p().rect(Rect::new(row.x, row.bottom() + 1.0, row.w, 1.0), HAIRLINE);
                for &h in &hours {
                    ui.p().rect(Rect::new(x_of(h * 60), row.y, 1.0, row.h), HAIRLINE.alpha(0.5));
                }
                let tour_on = sel == Some(Sel::Tour(t.tour.line.clone(), t.tour.tour.clone()));
                ui.text_in(&format!("{} {}", omsi_ui::tr("Tour"), t.tour.tour), Rect::new(row.x + 2.0, row.y, 90.0, row.h), 12.5, Weight::Bold, TEXT, Align::Left);
                // the bus
                let chip = Rect::new(row.x + 92.0, row.y + 6.0, left - 104.0, 22.0);
                let (h, _, click) = ui.interact(id_of(&format!("plan-bus-{ti}")), chip);
                if click {
                    clicked = Some((ti, None));
                }
                hits.push(Hit { r: chip, tour: ti, duty: None });
                if t.by_player {
                    ui.text_in(&omsi_ui::tr("You drove it."), chip, 11.5, Weight::Bold, accent_2(), Align::Left);
                } else {
                    let (text, fill, ink) = match t.bus {
                        Some(BusOf::Own(id)) => (c.vehicle(id).map(|x| format!("{}  {}", x.number, x.name)).unwrap_or_default(), if t.bus_from == Source::Auto { FIELD } else { HOVER }, if t.bus_from == Source::Auto { TEXT_SOFT } else { TEXT }),
                        Some(BusOf::Rental) => (omsi_ui::tr("Rental bus").into_owned(), HOVER, TEXT),
                        None => (omsi_ui::tr("No bus").into_owned(), problem_colour(t.bus_problem).alpha(0.12), problem_colour(t.bus_problem)),
                    };
                    ui.p().rounded(chip, 6.0, if h { fill.lighten(0.08) } else { fill });
                    if t.bus.is_none() || t.bus_problem.is_some() {
                        ui.p().rounded_border(chip, 6.0, 1.0, problem_colour(t.bus_problem));
                    }
                    if tour_on {
                        ui.p().rounded_border(chip, 6.0, 2.0, accent());
                    }
                    ui.text_in(&text, Rect::new(chip.x + 8.0, chip.y, chip.w - 12.0, chip.h), 11.5, Weight::Bold, ink, Align::Left);
                }
                // the duties
                for (k, d) in t.duties.iter().enumerate() {
                    let br = Rect::new(x_of(d.from), row.y + 5.0, ((d.to - d.from) as f32 * ppm).max(6.0), 24.0);
                    let (h, _, click) = ui.interact(id_of(&format!("plan-duty-{ti}-{k}")), br);
                    if click {
                        clicked = Some((ti, Some(k)));
                    }
                    hits.push(Hit { r: br, tour: ti, duty: Some(k) });
                    let faint = if t.bus.is_none() && !t.by_player { 0.35 } else { 1.0 };
                    let (fill, ink, label) = if t.by_player {
                        (accent().alpha(0.5), on_accent(), omsi_ui::tr("You").into_owned())
                    } else {
                        match d.who {
                            Some(Who::Staff(id)) => {
                                let col = driver_colour(c, id);
                                (if d.from_ == Source::Auto { col.alpha(0.55) } else { col }, INK, first_name(c, id))
                            }
                            Some(Who::Player) => (accent(), on_accent(), omsi_ui::tr("You").into_owned()),
                            Some(Who::Agency) => (TEXT_FAINT, TEXT, omsi_ui::tr("Agency").into_owned()),
                            None => (problem_colour(d.problem).alpha(0.12), problem_colour(d.problem), omsi_ui::tr(d.problem.map(Problem::label).unwrap_or("No driver")).into_owned()),
                        }
                    };
                    ui.p().rounded(br, 5.0, fill.alpha(faint * if h { 0.85 } else { 1.0 }));
                    if d.who.is_none() && !t.by_player {
                        ui.p().rounded_border(br, 5.0, 1.0, problem_colour(d.problem).alpha(faint));
                    } else if matches!(d.from_, Source::Dispatcher | Source::Central) {
                        ui.p().rounded_border(br, 5.0, 1.5, WARN);
                    }
                    // a late driver's first minutes
                    if let Some(late) = d.late {
                        let lw = ((late.until.min(d.to) - d.from) as f32 * ppm).max(3.0);
                        let col = if late.cover.is_some() { TEXT_DIM } else { DANGER };
                        ui.p().rounded(Rect::new(br.x, br.y, lw, br.h), 5.0, col.alpha(0.6));
                    }
                    if !d.warn.is_empty() {
                        ui.p().circle(Vec2::new(br.right() - 5.0, br.y + 5.0), 3.0, WARN);
                    }
                    if sel == Some(Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k)) {
                        ui.p().rounded_border(br.inset(-2.0), 6.0, 2.0, TEXT);
                    }
                    if br.w > 30.0 {
                        ui.text_in(&label, Rect::new(br.x + 6.0, br.y, br.w - 10.0, br.h), 11.0, Weight::Bold, ink, Align::Left);
                    }
                    let tip = format!("{} – {}  ·  {}", hhmm(d.from), hhmm(d.to), who_name(c, if t.by_player { Some(Who::Player) } else { d.who }));
                    ui.tooltip(br, &tip);
                }
            }
            y += 6.0;
        }
        y - v.y
    });
    if let Some((ti, duty)) = clicked {
        let t = &p.tours[ti];
        match (l.company.planning.arm, duty) {
            (Some(Arm::Driver(w)), Some(k)) if !t.by_player => give_driver(l, p, ti, k, Some(w)),
            (Some(Arm::Bus(id)), _) if !t.by_player => give_bus(l, p, ti, Some(id)),
            (_, Some(k)) => {
                let s = Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k);
                let v = &mut l.company.planning;
                v.sel = if v.sel.as_ref() == Some(&s) { None } else { Some(s) };
            }
            (_, None) => {
                let s = Sel::Tour(t.tour.line.clone(), t.tour.tour.clone());
                let v = &mut l.company.planning;
                v.sel = if v.sel.as_ref() == Some(&s) { None } else { Some(s) };
            }
        }
    }
    hits
}

// --- the side ------------------------------------------------------------------------------

fn side_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan) {
    let sel = l.company.planning.sel.clone();
    let find = |line: &str, tour: &str| p.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour);
    match sel {
        Some(Sel::Duty(line, tour, k)) => match find(&line, &tour).filter(|&i| k < p.tours[i].duties.len()) {
            Some(ti) => duty_panel(l, r, c, p, ti, k),
            None => day_panel(l, r, c, p),
        },
        Some(Sel::Tour(line, tour)) => match find(&line, &tour) {
            Some(ti) => tour_panel(l, r, c, p, ti),
            None => day_panel(l, r, c, p),
        },
        None => day_panel(l, r, c, p),
    }
}

/// A list row with a name, what it says on the right, and a swatch; returns (clicked,
/// pressed on it).
fn pick_row(ui: &mut Ui, name: &str, r: Rect, on: bool, swatch: Option<Color>, text: &str, right: &str, right_c: Color) -> (bool, bool) {
    let pressed = ui.hover(r) && ui.input.pressed;
    let clicked = ui.row(name, r, on);
    let mut x = r.x + 10.0;
    if let Some(s) = swatch {
        ui.p().rounded(Rect::new(x, r.y + r.h * 0.5 - 6.0, 12.0, 12.0), 3.0, s);
        x += 20.0;
    }
    let ink = if on { on_accent() } else { TEXT };
    let rw = ui.width(right, 11.0, Weight::Regular).min(r.w * 0.5);
    ui.text_in(text, Rect::new(x, r.y, r.right() - x - rw - 16.0, r.h), 12.5, Weight::Medium, ink, Align::Left);
    ui.text_in(right, Rect::new(r.right() - rw - 10.0, r.y, rw, r.h), 11.0, Weight::Regular, if on { on_accent() } else { right_c }, Align::Right);
    (clicked, pressed)
}

/// Nothing chosen: the day in short, what fell out, and the drivers and buses to give.
fn day_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan) {
    let inner = section(&mut l.ui, r, if p.today { "The company's day" } else { "This day" });
    let (tours, covered, open, no_bus) = p.counts();
    let rows = p.open_rows();
    let arm = l.company.planning.arm;
    // what the morning brought, and what is open
    let mut morning: Vec<(String, Color)> = Vec::new();
    for d in &p.disruptions {
        match *d {
            Disruption::Late { employee, minutes } => morning.push((omsi_ui::tr("%{name} comes %{n} minutes late.").replace("%{name}", &who_name(c, Some(Who::Staff(employee)))).replace("%{n}", &minutes.to_string()), WARN)),
            Disruption::Breakdown { vehicle, cost } => morning.push((
                omsi_ui::tr("Bus %{n} did not start: towed to the workshop (%{amount}).").replace("%{n}", &c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default()).replace("%{amount}", &eur(cost)),
                DANGER.lighten(0.2),
            )),
        }
    }
    let staff: Vec<(u32, String, Color, String, Color)> = c
        .staff
        .iter()
        .map(|e| {
            let mins: i32 = p.duties_of(Who::Staff(e.id)).iter().map(|&(i, k)| p.tours[i].duties[k].to - p.tours[i].duties[k].from).sum();
            let (state, col) = if !e.employed_on(&p.date) {
                (omsi_ui::tr("Not employed then").into_owned(), TEXT_FAINT)
            } else if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
                (omsi_ui::tr("Ill").into_owned(), DANGER.lighten(0.2))
            } else if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
                (omsi_ui::tr("On holiday").into_owned(), TEXT_DIM)
            } else if mins == 0 {
                (omsi_ui::tr("Free").into_owned(), TEXT_DIM)
            } else {
                (length(mins), if mins > co::staff::DAY_TARGET { WARN } else { TEXT_SOFT })
            };
            (e.id, e.name.clone(), driver_colour(c, e.id), state, col)
        })
        .collect();
    let buses: Vec<(u32, String, String, Color)> = c
        .fleet
        .iter()
        .map(|v| {
            let n = p.tours.iter().filter(|t| t.bus == Some(BusOf::Own(v.id))).count();
            let (state, col) = if !v.held_on(&p.date) {
                (omsi_ui::tr("Gone back then").into_owned(), TEXT_FAINT)
            } else if v.in_workshop(&p.date) || p.disruptions.iter().any(|d| matches!(d, Disruption::Breakdown { vehicle, .. } if *vehicle == v.id)) {
                (omsi_ui::tr("In the workshop").into_owned(), DANGER.lighten(0.2))
            } else if n == 0 {
                (omsi_ui::tr("Free").into_owned(), TEXT_DIM)
            } else {
                (omsi_ui::tr("%{n} tours").replace("%{n}", &n.to_string()), TEXT_SOFT)
            };
            (v.id, format!("{}  {}", v.number, v.name), state, col)
        })
        .collect();
    let open_rows: Vec<(usize, Option<usize>, String, String, Color)> = rows
        .iter()
        .map(|o| {
            let t = &p.tours[o.tour];
            let what = if o.late { omsi_ui::tr("Late start").into_owned() } else { omsi_ui::tr(o.problem.map(Problem::label).unwrap_or("No driver")).into_owned() };
            let fill = if o.filled {
                match (o.duty, o.late) {
                    (None, _) => match t.bus {
                        Some(BusOf::Rental) => omsi_ui::tr("Rental bus").into_owned(),
                        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(),
                        None => String::new(),
                    },
                    (Some(k), true) => t.duties[k].late.and_then(|x| x.cover).map(|id| who_name(c, Some(Who::Staff(id)))).unwrap_or_default(),
                    (Some(k), false) => who_name(c, t.duties[k].who),
                }
            } else {
                omsi_ui::tr("Trips dropped").into_owned()
            };
            (o.tour, o.duty, format!("{}  ·  {}", line_title(t, o.duty), what), fill, if o.filled { TEXT_SOFT } else { problem_colour(o.problem) })
        })
        .collect();
    let mut tapped: Option<Arm> = None;
    let mut pressed: Option<Arm> = None;
    let mut open_pick: Option<(usize, Option<usize>)> = None;
    let summary = omsi_ui::tr("%{c} of %{t} tours covered").replace("%{c}", &covered.to_string()).replace("%{t}", &tours.to_string());
    let gaps = if open + no_bus == 0 { omsi_ui::tr("Nothing open.").into_owned() } else { omsi_ui::tr("%{d} duties without a driver, %{b} tours without a bus.").replace("%{d}", &open.to_string()).replace("%{b}", &no_bus.to_string()) };
    l.ui.scroll_area("plan-side", inner, &mut |ui, v| {
        let mut y = v.y;
        ui.text_in(&summary, Rect::new(v.x, y, v.w, 20.0), 14.0, Weight::Bold, TEXT, Align::Left);
        y += 22.0;
        ui.text_in(&gaps, Rect::new(v.x, y, v.w, 18.0), 12.0, Weight::Regular, if open + no_bus == 0 { OK } else { WARN }, Align::Left);
        y += 28.0;
        let caps = |ui: &mut Ui, y: f32, t: &str| ui.text_in(&omsi_ui::tr(t).to_uppercase(), Rect::new(v.x, y, v.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
        if !morning.is_empty() {
            caps(ui, y, "This morning");
            y += 20.0;
            for (t, col) in &morning {
                ui.p().circle(Vec2::new(v.x + 4.0, y + 8.0), 3.0, *col);
                let h = ui.paragraph(t, Vec2::new(v.x + 14.0, y), v.w - 24.0, 12.0, Weight::Regular, TEXT_SOFT);
                y += h.max(16.0) + 6.0;
            }
            y += 8.0;
        }
        if !open_rows.is_empty() {
            caps(ui, y, "Open duties");
            y += 20.0;
            for (k, (ti, duty, title, fill, col)) in open_rows.iter().enumerate() {
                let rr = Rect::new(v.x, y, v.w - 8.0, 44.0);
                if ui.row(&format!("plan-open-{k}"), rr, false) {
                    open_pick = Some((*ti, *duty));
                }
                ui.text_in(title, Rect::new(rr.x + 8.0, rr.y + 4.0, rr.w - 16.0, 18.0), 11.5, Weight::Bold, TEXT, Align::Left);
                ui.text_in(&format!("→ {fill}"), Rect::new(rr.x + 8.0, rr.y + 22.0, rr.w - 16.0, 18.0), 11.5, Weight::Regular, *col, Align::Left);
                y += 46.0;
            }
            y += 8.0;
        }
        caps(ui, y, "Drivers");
        y += 18.0;
        ui.text_in(&omsi_ui::tr("Tap one, then a duty - or drag it there."), Rect::new(v.x, y, v.w, 16.0), 11.0, Weight::Regular, TEXT_FAINT, Align::Left);
        y += 22.0;
        let me = Rect::new(v.x, y, v.w - 8.0, 30.0);
        let (cl, pr) = pick_row(ui, "plan-me", me, arm == Some(Arm::Driver(Who::Player)), Some(accent()), &omsi_ui::tr("You"), &omsi_ui::tr("Your own duties"), TEXT_DIM);
        if cl {
            tapped = Some(Arm::Driver(Who::Player));
        }
        if pr {
            pressed = Some(Arm::Driver(Who::Player));
        }
        y += 32.0;
        for (id, name, col, state, state_c) in &staff {
            let rr = Rect::new(v.x, y, v.w - 8.0, 30.0);
            let a = Arm::Driver(Who::Staff(*id));
            let (cl, pr) = pick_row(ui, &format!("plan-driver-{id}"), rr, arm == Some(a), Some(*col), name, state, *state_c);
            if cl {
                tapped = Some(a);
            }
            if pr {
                pressed = Some(a);
            }
            y += 32.0;
        }
        if staff.is_empty() {
            ui.text_in(&omsi_ui::tr("Nobody on the payroll yet."), Rect::new(v.x, y, v.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            y += 22.0;
        }
        y += 10.0;
        caps(ui, y, "Buses");
        y += 20.0;
        for (id, name, state, state_c) in &buses {
            let rr = Rect::new(v.x, y, v.w - 8.0, 30.0);
            let a = Arm::Bus(*id);
            let (cl, pr) = pick_row(ui, &format!("plan-bus-pick-{id}"), rr, arm == Some(a), None, name, state, *state_c);
            if cl {
                tapped = Some(a);
            }
            if pr {
                pressed = Some(a);
            }
            y += 32.0;
        }
        if buses.is_empty() {
            ui.text_in(&omsi_ui::tr("No bus in the fleet yet."), Rect::new(v.x, y, v.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            y += 22.0;
        }
        y - v.y + 8.0
    });
    let view = &mut l.company.planning;
    if let Some(a) = pressed {
        view.drag = Some(a);
    }
    if let Some(a) = tapped {
        view.arm = if view.arm == Some(a) { None } else { Some(a) };
        view.drag = None;
    }
    if let Some((ti, duty)) = open_pick {
        let t = &p.tours[ti];
        view.sel = Some(match duty {
            Some(k) => Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k),
            None => Sel::Tour(t.tour.line.clone(), t.tour.tour.clone()),
        });
    }
}

/// A duty chosen: its times, who drives it and why, who else could; on the company's day
/// what to do when it is open, and "Drive this duty".
fn duty_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, ti: usize, k: usize) {
    let t = p.tours[ti].clone();
    let d = t.duties[k].clone();
    let inner = section(&mut l.ui, r, "Duty");
    if l.ui.icon_button("plan-sel-close", Vec2::new(r.right() - 22.0, r.y + 18.0), 13.0, "close", "Back to the day") {
        l.company.planning.sel = None;
        return;
    }
    let key = pl::duty_key(&t.tour.line, &t.tour.tour, k);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let late_key = pl::late_key(&t.tour.line, &t.tour.tour, k);
    let late_fill = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == late_key);
    let first = &t.tour.trips[d.start];
    let last = &t.tour.trips[d.end - 1];
    let mut facts: Vec<(String, Color)> = vec![
        (format!("{} – {}  ·  {}  ·  {} {}", hhmm(d.from), hhmm(d.to), length(d.to - d.from), d.end - d.start, omsi_ui::tr("trips")), TEXT_SOFT),
        (format!("{} → {}", first.from, last.to), TEXT_DIM),
    ];
    let who = if t.by_player { omsi_ui::tr("You drove it.").into_owned() } else { format!("{}: {}", omsi_ui::tr("Driven by"), who_name(c, d.who)) };
    facts.push((who, TEXT));
    if !source_text(d.from_).is_empty() && d.who.is_some() {
        facts.push((omsi_ui::tr(source_text(d.from_)).into_owned(), TEXT_DIM));
    }
    if let Some(pb) = d.problem {
        facts.push((format!("{}: {}", omsi_ui::tr("Open because"), omsi_ui::tr(pb.label())), problem_colour(Some(pb))));
    }
    for w in &d.warn {
        let s = match w {
            Warn::Overtime(m) => omsi_ui::tr("Overtime %{t}").replace("%{t}", &length(*m)),
            Warn::Transfer(m) => omsi_ui::tr("%{n} min to change stops").replace("%{n}", &m.to_string()),
            Warn::Experience => omsi_ui::tr("Little experience for this bus").into_owned(),
        };
        facts.push((s, WARN));
    }
    if let Some(late) = d.late {
        let s = match late.cover {
            Some(id) => omsi_ui::tr("The driver is late until %{t}: %{name} drives the first trips.").replace("%{t}", &hhmm(late.until)).replace("%{name}", &who_name(c, Some(Who::Staff(id)))),
            None => omsi_ui::tr("The driver is late until %{t}: the trips before are dropped.").replace("%{t}", &hhmm(late.until)),
        };
        facts.push((s, if late.cover.is_some() { WARN } else { DANGER.lighten(0.2) }));
    }
    let mut y = inner.y;
    l.ui.text_in(&line_title(&t, Some(k)), Rect::new(inner.x, y, inner.w - 20.0, 20.0), 14.0, Weight::Bold, TEXT, Align::Left);
    y += 26.0;
    for (s, col) in &facts {
        let h = l.ui.paragraph(s, Vec2::new(inner.x, y), inner.w, 12.0, Weight::Regular, *col);
        y += h.max(16.0) + 4.0;
    }
    y += 8.0;
    // on the company's day: drive it, and what to do while it is open
    let foot_h = if p.today && !t.by_player { 46.0 } else { 0.0 };
    if p.today && !t.by_player {
        let b = Rect::new(inner.x, inner.bottom() - 38.0, inner.w, 38.0);
        if l.ui.button("plan-drive", b, "Drive this duty", Some("play_arrow"), ButtonKind::Primary) {
            drive(l, c, p, ti, k);
            return;
        }
    }
    let mut options: Vec<(String, Option<Fill>, bool)> = Vec::new();
    if p.today && !t.by_player {
        if d.problem.is_some() || d.who.is_none() {
            options.push((omsi_ui::tr("An agency driver for today").into_owned(), Some(Fill::Agency), true));
            options.push((omsi_ui::tr("Drop its trips today").into_owned(), Some(Fill::Drop), true));
            if fill_now {
                options.push((omsi_ui::tr("Leave it to the central").into_owned(), None, true));
            }
        }
        if d.late.is_some() {
            options.push((omsi_ui::tr("Drop the first trips").into_owned(), Some(Fill::Drop), false));
            if late_fill {
                options.push((omsi_ui::tr("Let the central cover the first trips").into_owned(), None, false));
            }
        }
    }
    let cands: Vec<(Option<Who>, String, String, Color, Option<Color>)> = {
        let mut v: Vec<(Option<Who>, String, String, Color, Option<Color>)> = Vec::new();
        v.push((Some(Who::Player), omsi_ui::tr("You").into_owned(), omsi_ui::tr("Drive it yourself").into_owned(), TEXT_DIM, Some(accent())));
        let mut people: Vec<(bool, u32, String, String, Color)> = c
            .staff
            .iter()
            .map(|e| {
                let (s, col, ok) = fit_of(c, p, ti, k, e.id);
                (ok, e.id, e.name.clone(), s, col)
            })
            .collect();
        people.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)));
        for (_, id, name, s, col) in people {
            v.push((Some(Who::Staff(id)), name, s, col, Some(driver_colour(c, id))));
        }
        v.push((None, omsi_ui::tr("Nobody").into_owned(), String::new(), TEXT_DIM, None));
        v
    };
    let list = Rect::new(inner.x, y, inner.w, (inner.bottom() - foot_h - y).max(0.0));
    let current = if t.by_player { Some(Who::Player) } else { d.who };
    let mut pick: Option<Option<Who>> = None;
    let mut opt: Option<(Option<Fill>, bool)> = None;
    l.ui.scroll_area("plan-duty-side", list, &mut |ui, v| {
        let mut yy = v.y;
        if !options.is_empty() {
            ui.text_in(&omsi_ui::tr("Today").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
            yy += 18.0;
            for (n, (label, fill, whole)) in options.iter().enumerate() {
                let rr = Rect::new(v.x, yy, v.w - 8.0, 30.0);
                if pick_row(ui, &format!("plan-opt-{n}"), rr, false, None, label, "", TEXT_DIM).0 {
                    opt = Some((*fill, *whole));
                }
                yy += 32.0;
            }
            yy += 10.0;
        }
        ui.text_in(&omsi_ui::tr("Who drives it").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
        yy += 18.0;
        for (n, (w, name, s, col, sw)) in cands.iter().enumerate() {
            let rr = Rect::new(v.x, yy, v.w - 8.0, 30.0);
            if pick_row(ui, &format!("plan-cand-{n}"), rr, *w == current && w.is_some(), *sw, name, s, *col).0 {
                pick = Some(*w);
            }
            yy += 32.0;
        }
        yy - v.y + 8.0
    });
    if let Some(w) = pick {
        if !t.by_player {
            give_driver(l, p, ti, k, w);
        }
    }
    if let Some((fill, whole)) = opt {
        set_fill(l, if whole { key } else { late_key }, fill);
    }
}

/// A tour chosen: its bus and which others could run it; on the company's day a rental bus
/// or dropping it.
fn tour_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, ti: usize) {
    let t = p.tours[ti].clone();
    let inner = section(&mut l.ui, r, "Tour");
    if l.ui.icon_button("plan-sel-close", Vec2::new(r.right() - 22.0, r.y + 18.0), 13.0, "close", "Back to the day") {
        l.company.planning.sel = None;
        return;
    }
    let mut y = inner.y;
    l.ui.text_in(&line_title(&t, None), Rect::new(inner.x, y, inner.w - 20.0, 20.0), 14.0, Weight::Bold, TEXT, Align::Left);
    y += 26.0;
    let mut facts: Vec<(String, Color)> = vec![(format!("{} – {}  ·  {} km  ·  {} {}", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km().round(), t.duties.len(), omsi_ui::tr("duties")), TEXT_SOFT)];
    let bus = match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        Some(BusOf::Rental) => omsi_ui::tr("Rental bus").into_owned(),
        None => omsi_ui::tr("No bus").into_owned(),
    };
    facts.push((format!("{}: {}", omsi_ui::tr("Bus"), bus), TEXT));
    if t.bus.is_some() && !source_text(t.bus_from).is_empty() {
        facts.push((omsi_ui::tr(source_text(t.bus_from)).into_owned(), TEXT_DIM));
    }
    if let Some(pb) = t.bus_problem {
        facts.push((format!("{}: {}", omsi_ui::tr("Open because"), omsi_ui::tr(pb.label())), problem_colour(Some(pb))));
    }
    if let Some(w) = t.tour.wants() {
        facts.push((omsi_ui::tr("The tour asks for: %{kind}").replace("%{kind}", &omsi_ui::tr(co::BusKind { size: w, drive: co::Drive::Diesel }.label())), TEXT_DIM));
    }
    for (s, col) in &facts {
        let h = l.ui.paragraph(s, Vec2::new(inner.x, y), inner.w, 12.0, Weight::Regular, *col);
        y += h.max(16.0) + 4.0;
    }
    y += 8.0;
    let key = pl::tour_key(&t.tour.line, &t.tour.tour);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let mut options: Vec<(String, Option<Fill>)> = Vec::new();
    if p.today && !t.by_player && (t.bus.is_none() || t.bus_problem.is_some()) {
        let rent = co::economy::rent_per_day(co::BusKind { size: t.tour.wants().unwrap_or_default(), drive: co::Drive::Diesel }, &co::economy::rules(c.difficulty), c.price_index);
        options.push((omsi_ui::tr("Rent a bus for today (%{amount})").replace("%{amount}", &eur(rent)), Some(Fill::Rental)));
        options.push((omsi_ui::tr("Drop the tour today").into_owned(), Some(Fill::Drop)));
        if fill_now {
            options.push((omsi_ui::tr("Leave it to the central").into_owned(), None));
        }
    }
    let mut cands: Vec<(bool, Option<u32>, String, String, Color)> = c
        .fleet
        .iter()
        .map(|v| {
            let (s, col, ok) = bus_fit(c, p, ti, v.id);
            (ok, Some(v.id), format!("{}  {}", v.number, v.name), s, col)
        })
        .collect();
    cands.sort_by(|a, b| b.0.cmp(&a.0));
    cands.push((true, None, omsi_ui::tr("No bus").into_owned(), String::new(), TEXT_DIM));
    let current = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    let list = Rect::new(inner.x, y, inner.w, (inner.bottom() - y).max(0.0));
    let mut pick: Option<Option<u32>> = None;
    let mut opt: Option<Option<Fill>> = None;
    let by_player = t.by_player;
    l.ui.scroll_area("plan-tour-side", list, &mut |ui, v| {
        let mut yy = v.y;
        if !options.is_empty() {
            ui.text_in(&omsi_ui::tr("Today").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
            yy += 18.0;
            for (n, (label, fill)) in options.iter().enumerate() {
                let rr = Rect::new(v.x, yy, v.w - 8.0, 30.0);
                if pick_row(ui, &format!("plan-topt-{n}"), rr, false, None, label, "", TEXT_DIM).0 {
                    opt = Some(*fill);
                }
                yy += 32.0;
            }
            yy += 10.0;
        }
        if !by_player {
            ui.text_in(&omsi_ui::tr("Which bus runs it").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
            yy += 18.0;
            for (n, (_, id, name, s, col)) in cands.iter().enumerate() {
                let rr = Rect::new(v.x, yy, v.w - 8.0, 30.0);
                if pick_row(ui, &format!("plan-bcand-{n}"), rr, id.is_some() && *id == current, None, name, s, *col).0 {
                    pick = Some(*id);
                }
                yy += 32.0;
            }
        }
        yy - v.y + 8.0
    });
    if let Some(b) = pick {
        give_bus(l, p, ti, b);
    }
    if let Some(fill) = opt {
        set_fill(l, key, fill);
    }
}

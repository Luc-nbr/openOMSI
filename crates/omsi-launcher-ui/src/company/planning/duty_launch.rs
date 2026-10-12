//! The company's duty launcher (Luc: "a launcher from the bus company that works differently
//! and works ONLY with the data given in the bus company"): a page of the company's own, in
//! RosterFlow's look, with one duty of its planning exactly as it was planned - the company,
//! the day and the times from its clock, the line, the trips in order with the breaks, the
//! deadheads and the relief points, the bus the planning gives the tour in the company's
//! livery, the depot file, the map and the duty's route on it (`co::duty_launch::Sheet`).
//!
//! Nothing of the duty is chosen here: no other duty, bus, map or day. What keeps it from
//! being driven (`Hitch`) is said, and put right in RosterFlow ("Open in RosterFlow"; in the
//! easy mode back to My duties). Only what leaves the duty as it is may be chosen: the
//! weather. "Start duty" starts the game with exactly the duty (`co::duty_launch::start`): the
//! planned bus, its trips, the company's map, day, time and depot file; the result comes back
//! to the company as any company duty's does (`--company`: the trip reports, the live plan).
//! The Drive page keeps its own choice: nothing of the company's duty goes into it.

use super::super::super::mapview::{Look, Pointer};
use super::super::super::theme::RADIUS;
use super::super::super::ui::Key;
use super::super::super::Launcher;
use super::super::{company_mark, day_label, kit};
use super::rf_gantt::line_colour;
use super::rf_scroll;
use super::rosterflow::{caps, pal, plain, rf_button, Pal};
use super::{hhmm, length, plan_of, rf_set_view, work, Sel, RF_ROSTER};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::{self as dl, DutyRef, Hitch, Row, Sheet};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// The launcher open: the duty, the page it was opened from, the weather chosen, and the
/// player's trip reports (read again now and then: a duty driven meanwhile is driven).
pub struct DutyLaunch {
    pub(in super::super) duty: DutyRef,
    pub(in super::super) from_tab: usize,
    /// The weather: "cycle" (as the month brings it, the default), "" (the map's), a METAR
    /// station's ("metar:..."), or a weather file.
    pub(in super::super) weather: String,
    trips: Vec<core::TripRun>,
    trips_read: Option<f32>,
}

impl DutyLaunch {
    pub fn new(duty: DutyRef, from_tab: usize) -> DutyLaunch {
        DutyLaunch { duty, from_tab, weather: "cycle".into(), trips: Vec::new(), trips_read: None }
    }
}

/// What the page has this frame: the sheet (None: the planning is being read), with the
/// hitches the launcher sees besides the company's (the bus not installed).
fn sheet_now(l: &mut Launcher, c: &Company) -> Option<Sheet> {
    let r = l.company.launch.as_ref()?.duty.clone();
    // (the trip reports: read again every few seconds - a duty driven since is driven)
    let now_t = l.ui.time;
    let stale = l.company.launch.as_ref().is_some_and(|x| x.trips_read.is_none_or(|t| now_t - t > 5.0));
    if stale {
        let trips = core::trips_of(&super::super::data(), &c.profile);
        if let Some(x) = l.company.launch.as_mut() {
            x.trips = trips;
            x.trips_read = Some(now_t);
        }
    }
    let now = co::clock::now(c);
    let mut s = if co::dates::between(&c.date, &r.date) < 0 {
        // (a day before the company's: over, and no longer planned)
        Sheet { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), map: c.map.clone(), hitches: vec![Hitch::Over], ..Default::default() }
    } else {
        work(l, c);
        let p = match plan_of(l, c, &r.date)? {
            Ok(p) => p,
            Err(_) => return Some(Sheet { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), map: c.map.clone(), hitches: vec![Hitch::Timetable], ..Default::default() }),
        };
        let trips = l.company.launch.as_ref().map(|x| x.trips.clone()).unwrap_or_default();
        dl::sheet(c, &p, &r, now, &trips)
    };
    // the planned bus installed here (the list read whole: while it is being read nothing is
    // said)
    if let Some(b) = s.bus.as_ref().filter(|b| !b.file.trim().is_empty()) {
        let same = |f: &str| f.replace('\\', "/").eq_ignore_ascii_case(&b.file.replace('\\', "/"));
        if !l.state.loading_content && !l.state.vehicles.is_empty() && !l.state.vehicles.iter().any(|v| same(&v.file)) {
            s.hitches.push(Hitch::NotInstalled(b.file.clone()));
        }
    }
    Some(s)
}

/// The launcher across `area` (the company's page, its tabs too).
pub(in super::super) fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else {
        l.company.launch = None;
        return;
    };
    // (the fleet map's layer is not the duty's)
    super::super::map::leave(l);
    if l.ui.input.keys.contains(&Key::Escape) && l.ui.focus.is_none() && !l.ui.over_popup() {
        close(l);
        return;
    }
    let q = pal(&l.ui);
    let sheet = sheet_now(l, &c);
    l.ui.as_drawn();
    l.ui.solid(area);
    l.ui.p().rounded(area, RADIUS, q.bg);
    let head = Rect::new(area.x, area.y, area.w, 84.0);
    l.ui.p().rounded(head, RADIUS, q.bar);
    l.ui.p().rect(Rect::new(head.x, head.bottom() - 1.0, head.w, 1.0), q.line);
    l.ui.end_as_drawn();
    let mark = Rect::new(head.x + 20.0, head.y + 18.0, 48.0, 48.0);
    company_mark(l, mark, &c);
    l.ui.as_drawn();
    let x = mark.right() + 16.0;
    // the way back, and to RosterFlow
    let back = Rect::new(head.right() - 20.0 - 110.0, head.y + 24.0, 110.0, 36.0);
    if rf_button(&mut l.ui, &q, "duty-launch-back", back, "Back", false, true) {
        l.ui.end_as_drawn();
        close(l);
        return;
    }
    let (to_label, to_w) = to_planning_label(l);
    let to = Rect::new(back.x - 12.0 - to_w, back.y, to_w, 36.0);
    if rf_button(&mut l.ui, &q, "duty-launch-rosterflow", to, to_label, false, true) {
        l.ui.end_as_drawn();
        to_planning(l, &c);
        return;
    }
    let tw = (to.x - x - 20.0).max(80.0);
    l.ui.icon("lock", Vec2::new(x + 6.0, head.y + 22.0), 13.0, q.accent);
    caps(&mut l.ui, Rect::new(x + 18.0, head.y + 14.0, tw - 18.0, 16.0), "Company duty · fixed by the planning", q.accent, Align::Left);
    l.ui.text_as_is(&c.name, Vec2::new(x, head.y + 52.0), 21.0, Weight::Bold, q.text, Align::Left);
    let name_w = l.ui.width(&c.name, 21.0, Weight::Bold);
    let Some(s) = sheet else {
        l.ui.text_in("Reading the planning…", Rect::new(area.x, area.y + area.h * 0.5 - 12.0, area.w, 24.0), 14.0, Weight::Regular, q.dim, Align::Center);
        l.ui.end_as_drawn();
        return;
    };
    if !s.number.is_empty() {
        let title = format!("{} {}  ·  {} {}  ·  {} {}", omsi_ui::tr("Line"), s.number, omsi_ui::tr("Tour"), s.duty.tour, omsi_ui::tr("Duty"), s.duty.duty + 1);
        l.ui.text_as_is(&title, Vec2::new(x + name_w + 18.0, head.y + 52.0), 17.0, Weight::Medium, q.dim, Align::Left);
    }
    // the day and the times
    let facts_r = Rect::new(area.x + 20.0, head.bottom() + 12.0, area.w - 40.0, 44.0);
    day_strip(l, &q, facts_r, &s);
    let body = Rect::new(area.x + 20.0, facts_r.bottom() + 12.0, area.w - 40.0, (area.bottom() - facts_r.bottom() - 32.0).max(120.0));
    let left_w = (body.w * 0.4).clamp(340.0, 560.0);
    let left = Rect::new(body.x, body.y, left_w, body.h);
    let right = Rect::new(left.right() + 16.0, body.y, (body.right() - left.right() - 16.0).max(200.0), body.h);
    duty_card(l, &q, left, &c, &s);
    let map_h = (right.h * 0.52).max(160.0);
    let map_r = Rect::new(right.x, right.y, right.w, map_h);
    let low = Rect::new(right.x, map_r.bottom() + 16.0, right.w, (right.bottom() - map_r.bottom() - 16.0).max(80.0));
    let bus_r = Rect::new(low.x, low.y, (low.w * 0.48).floor(), low.h);
    let go_r = Rect::new(bus_r.right() + 16.0, low.y, (low.right() - bus_r.right() - 16.0).max(120.0), low.h);
    bus_card(l, &q, bus_r, &s);
    start_card(l, &q, go_r, &c, &s);
    l.ui.end_as_drawn();
    route_map(l, &q, map_r, &c, &s);
}

/// The launcher left: nothing of its duty stays anywhere (the Drive page never had it).
pub(in super::super) fn close(l: &mut Launcher) {
    if let Some(x) = l.company.launch.take() {
        l.company.tab = x.from_tab;
    }
}

/// Where the duty is put right: RosterFlow's roster, or in the easy mode (whose planning is
/// the company's own) My duties.
fn to_planning_label(l: &Launcher) -> (&'static str, f32) {
    if super::super::shown(l, 5) {
        ("Open in RosterFlow", 190.0)
    } else {
        ("Back to My duties", 190.0)
    }
}

fn to_planning(l: &mut Launcher, c: &Company) {
    let Some(x) = l.company.launch.take() else { return };
    if super::super::shown(l, 5) {
        l.company.tab = 5;
        l.company.planning.day = co::dates::between(&c.date, &x.duty.date).clamp(0, 6) as usize;
        rf_set_view(l, RF_ROSTER, None);
        l.company.planning.sel = Some(Sel::Duty(x.duty.line.clone(), x.duty.tour.clone(), x.duty.duty));
    } else {
        l.company.tab = 6;
        l.company.career.part = super::super::career::DUTIES;
    }
}

/// The day, the sign-on and sign-off, the length, the trips and the lines, in a row of tiles.
fn day_strip(l: &mut Launcher, q: &Pal, r: Rect, s: &Sheet) {
    let lines = s.lines.join(", ");
    let tiles: [(&str, String); 6] = [
        ("Day", day_label(&s.duty.date)),
        ("Sign on", if s.rows.is_empty() { "-".into() } else { hhmm(s.from) }),
        ("Sign off", if s.rows.is_empty() { "-".into() } else { hhmm(s.to) }),
        ("Length", if s.rows.is_empty() { "-".into() } else { length(s.to - s.from) }),
        ("Trips", omsi_ui::tr("%{n} with passengers · %{m} deadheads").replace("%{n}", &s.trips.to_string()).replace("%{m}", &s.deadheads.to_string())),
        ("Lines", if lines.is_empty() { "-".into() } else { lines }),
    ];
    let gap = 10.0;
    let weights = [1.5, 0.8, 0.8, 0.8, 1.5, 0.9];
    let total: f32 = weights.iter().sum();
    let mut x = r.x;
    for (k, (label, value)) in tiles.iter().enumerate() {
        let w = (r.w - gap * 5.0) * weights[k] / total;
        let t = Rect::new(x, r.y, w, r.h);
        l.ui.p().rounded(t, 4.0, q.sheet);
        caps(&mut l.ui, Rect::new(t.x + 12.0, t.y + 5.0, t.w - 24.0, 14.0), label, q.dim, Align::Left);
        l.ui.text_as_is(value, Vec2::new(t.x + 12.0, t.y + 36.0), 14.5, Weight::Bold, q.text, Align::Left);
        x += w + gap;
    }
}

/// The duty's card: its rows in order, as the planning has them.
fn duty_card(l: &mut Launcher, q: &Pal, r: Rect, c: &Company, s: &Sheet) {
    l.ui.p().rounded(r, 4.0, q.sheet);
    caps(&mut l.ui, Rect::new(r.x + 16.0, r.y + 12.0, r.w - 32.0, 16.0), "The duty as planned", q.dim, Align::Left);
    let list = Rect::new(r.x + 8.0, r.y + 38.0, r.w - 12.0, r.h - 46.0);
    if s.rows.is_empty() {
        l.ui.text_in("Nothing to show: the planning no longer has this duty.", Rect::new(list.x + 8.0, list.y + 8.0, list.w - 16.0, 22.0), 13.0, Weight::Regular, q.dim, Align::Left);
        return;
    }
    let look = rf_scroll::look(q);
    let colour = line_colour(c, &s.duty.line);
    let rows = s.rows.clone();
    let q = *q;
    rf_scroll::area(&mut l.ui, &look, "duty-launch-rows", list, &mut |ui, room| {
        let mut y = room.y;
        let w = room.w - rf_scroll::BAR - 6.0;
        for row in &rows {
            let h = match row {
                Row::Break { .. } => 30.0,
                _ => 44.0,
            };
            let rr = Rect::new(room.x, y, w, h);
            let time_x = rr.x + 8.0;
            let body_x = rr.x + 108.0;
            match row {
                Row::SignOn { at, time, relief } => {
                    ui.icon("login", Vec2::new(body_x - 18.0, rr.center().y), 17.0, q.accent);
                    ui.text_as_is(&hhmm(*time), Vec2::new(time_x, rr.center().y + 5.0), 14.0, Weight::Bold, q.text, Align::Left);
                    ui.text_in(if *relief { "Take the bus over (relief)" } else { "Sign on" }, Rect::new(body_x, rr.y + 4.0, w - 108.0, 20.0), 14.0, Weight::Bold, q.text, Align::Left);
                    ui.text_as_is(at, Vec2::new(body_x, rr.y + 36.0), 12.5, Weight::Regular, q.dim, Align::Left);
                }
                Row::SignOff { at, time, relief } => {
                    ui.icon("logout", Vec2::new(body_x - 18.0, rr.center().y), 17.0, q.accent);
                    ui.text_as_is(&hhmm(*time), Vec2::new(time_x, rr.center().y + 5.0), 14.0, Weight::Bold, q.text, Align::Left);
                    ui.text_in(if *relief { "Hand the bus over (relief)" } else { "Sign off" }, Rect::new(body_x, rr.y + 4.0, w - 108.0, 20.0), 14.0, Weight::Bold, q.text, Align::Left);
                    ui.text_as_is(at, Vec2::new(body_x, rr.y + 36.0), 12.5, Weight::Regular, q.dim, Align::Left);
                }
                Row::Break { at, from, to } => {
                    let mins = to - from;
                    let word = if mins >= 15 { "Break" } else { "Layover" };
                    ui.icon("schedule", Vec2::new(body_x - 18.0, rr.center().y), 14.0, q.faint);
                    let text = format!("{} · {} · {}", omsi_ui::tr(word), omsi_ui::tr("%{n} min").replace("%{n}", &mins.to_string()), at);
                    ui.text_as_is(&text, Vec2::new(body_x, rr.center().y + 4.5), 12.5, Weight::Regular, q.dim, Align::Left);
                }
                Row::Trip { dep, arr, line, from, to, empty, .. } => {
                    ui.p().rect(Rect::new(rr.x, rr.y, w, 1.0), q.line);
                    ui.text_as_is(&hhmm(*dep), Vec2::new(time_x, rr.y + 19.0), 14.0, Weight::Bold, if *empty { q.dim } else { q.text }, Align::Left);
                    ui.text_as_is(&hhmm(*arr), Vec2::new(time_x, rr.y + 37.0), 12.0, Weight::Regular, q.faint, Align::Left);
                    if *empty {
                        let chip = Rect::new(body_x - 34.0, rr.center().y - 10.0, 30.0, 20.0);
                        ui.p().rounded(chip, 3.0, q.grey.alpha(0.35));
                        ui.icon("garage", chip.center(), 13.0, q.dim);
                        ui.text_in("Deadhead", Rect::new(body_x + 4.0, rr.y + 4.0, w - 112.0, 20.0), 13.5, Weight::Medium, q.dim, Align::Left);
                    } else {
                        let label = if line.trim().is_empty() { "?".to_string() } else { line.trim().to_string() };
                        let cw = (ui.width(&label, 12.5, Weight::Bold) + 14.0).max(30.0);
                        let chip = Rect::new((body_x - 4.0 - cw).max(time_x + 46.0), rr.center().y - 10.0, cw, 20.0);
                        ui.p().rounded(chip, 3.0, colour);
                        ui.text_as_is(&label, Vec2::new(chip.center().x, chip.center().y + 4.5), 12.5, Weight::Bold, super::super::super::ownlines::ink_on(colour), Align::Center);
                        ui.text_as_is(&format!("{from}  →  {to}"), Vec2::new(body_x + 4.0, rr.y + 19.0), 13.5, Weight::Medium, q.text, Align::Left);
                    }
                    if *empty {
                        ui.text_as_is(&format!("{from}  →  {to}"), Vec2::new(body_x + 4.0, rr.y + 37.0), 12.0, Weight::Regular, q.faint, Align::Left);
                    } else {
                        ui.text_as_is(&omsi_ui::tr("%{n} min").replace("%{n}", &(arr - dep).to_string()), Vec2::new(body_x + 4.0, rr.y + 37.0), 12.0, Weight::Regular, q.faint, Align::Left);
                    }
                }
            }
            y += h;
        }
        y - room.y + 8.0
    });
}

/// The bus the planning gives the tour, the depot file and the map: fixed.
fn bus_card(l: &mut Launcher, q: &Pal, r: Rect, s: &Sheet) {
    l.ui.p().rounded(r, 4.0, q.sheet);
    caps(&mut l.ui, Rect::new(r.x + 16.0, r.y + 12.0, r.w - 32.0, 16.0), "Bus and depot", q.dim, Align::Left);
    let mut y = r.y + 38.0;
    let x = r.x + 16.0;
    let w = r.w - 32.0;
    let map = if s.map_name.trim().is_empty() { super::super::super::state::short_map(&s.map) } else { s.map_name.clone() };
    let none = omsi_ui::tr("Not planned").into_owned();
    let rows: Vec<(&str, String)> = match s.bus.as_ref() {
        Some(b) => vec![
            ("Bus", if b.number.trim().is_empty() { b.name.clone() } else { format!("{} · {}", b.number.trim(), b.name) }),
            ("Livery", if b.paint.trim().is_empty() { omsi_ui::tr("The company's").into_owned() } else { b.paint.clone() }),
            ("Number plate", b.plate.clone()),
            ("Depot file", s.hof.clone()),
            ("Map", map),
        ],
        None => vec![("Bus", none), ("Depot file", s.hof.clone()), ("Map", map)],
    };
    let label_w = (w * 0.36).clamp(90.0, 140.0);
    for (k, v) in rows {
        if y + 30.0 > r.bottom() - 8.0 {
            break;
        }
        l.ui.text_in(k, Rect::new(x, y, label_w, 28.0), 13.0, Weight::Regular, q.dim, Align::Left);
        let v = if v.trim().is_empty() { "-".to_string() } else { v };
        let shown = fit(&l.ui, &v, w - label_w, 13.0);
        l.ui.text_as_is(&shown, Vec2::new(x + label_w, y + 18.5), 13.0, Weight::Bold, q.text, Align::Left);
        l.ui.p().rect(Rect::new(x, y + 29.0, w, 1.0), q.line);
        y += 33.0;
    }
    if y + 40.0 < r.bottom() {
        l.ui.icon("lock", Vec2::new(x + 6.0, y + 16.0), 13.0, q.faint);
        l.ui.paragraph("The planning gives the duty its bus, in the company's livery: it is not chosen here.", Vec2::new(x + 20.0, y + 6.0), w - 20.0, 12.0, Weight::Regular, q.faint);
    }
}

/// `text` cut to `w` with an ellipsis.
fn fit(ui: &super::super::super::ui::Ui, text: &str, w: f32, px: f32) -> String {
    if ui.width(text, px, Weight::Bold) <= w {
        return text.to_string();
    }
    let mut t: String = text.to_string();
    while !t.is_empty() && ui.width(&format!("{t}…"), px, Weight::Bold) > w {
        t.pop();
    }
    format!("{t}…")
}

/// The weather to choose (what leaves the duty as it is), what keeps the duty from starting,
/// and the start.
fn start_card(l: &mut Launcher, q: &Pal, r: Rect, c: &Company, s: &Sheet) {
    l.ui.p().rounded(r, 4.0, q.sheet);
    let x = r.x + 16.0;
    let w = r.w - 32.0;
    let mut y = r.y + 12.0;
    let go = Rect::new(x, r.bottom() - 16.0 - 52.0, w, 52.0);
    if let Some(h) = s.hitches.first() {
        // what is wrong, and where it is put right
        let (title, what) = h.words();
        let col = q.err;
        caps(&mut l.ui, Rect::new(x, y, w, 16.0), "Cannot start", col, Align::Left);
        y += 26.0;
        l.ui.icon("error", Vec2::new(x + 9.0, y + 10.0), 18.0, col);
        y += l.ui.paragraph(title, Vec2::new(x + 26.0, y - 2.0), w - 26.0, 14.5, Weight::Bold, q.text) + 2.0;
        if let Some(d) = detail(h) {
            y += l.ui.paragraph(&d, Vec2::new(x + 26.0, y), w - 26.0, 12.5, Weight::Medium, col) + 2.0;
        }
        y += l.ui.paragraph(what, Vec2::new(x + 26.0, y), w - 26.0, 12.5, Weight::Regular, q.dim) + 6.0;
        for more in s.hitches.iter().skip(1) {
            if y + 22.0 > go.y - 50.0 {
                break;
            }
            l.ui.icon("error", Vec2::new(x + 9.0, y + 9.0), 14.0, col.alpha(0.8));
            y += l.ui.paragraph(more.words().0, Vec2::new(x + 26.0, y - 2.0), w - 26.0, 12.5, Weight::Medium, q.text) + 2.0;
        }
        let (label, _) = to_planning_label(l);
        let fix = Rect::new(x, go.y - 12.0 - 40.0, w, 40.0);
        if fix.y > y && rf_button(&mut l.ui, q, "duty-launch-fix", fix, label, true, true) {
            to_planning(l, c);
            return;
        }
        rf_button(&mut l.ui, q, "duty-launch-start-off", go, "Start duty", false, false);
        return;
    }
    caps(&mut l.ui, Rect::new(x, y, w, 16.0), "Before you start", q.dim, Align::Left);
    y += 26.0;
    // the weather: the one thing to choose
    let (values, names) = weathers(l, c);
    let mut pick = values.iter().position(|v| *v == l.company.launch.as_ref().map(|x| x.weather.clone()).unwrap_or_default()).unwrap_or(0);
    l.ui.text_in("Weather", Rect::new(x, y, 90.0, 36.0), 13.0, Weight::Regular, q.dim, Align::Left);
    let field = Rect::new(x + 90.0, y, w - 90.0, 36.0);
    if plain(&mut l.ui, |ui| ui.select("duty-launch-weather", field, &mut pick, &names)) {
        if let (Some(v), Some(x)) = (values.get(pick), l.company.launch.as_mut()) {
            x.weather = v.clone();
        }
    }
    y += 48.0;
    // when the game starts: the company's clock, not the computer's
    let lead = core::bus_lead(l.state.settings.get("bus_lead").and_then(|x| x.as_i64()));
    let first = s.planned.iter().find(|x| !x.empty).map(|x| x.dep).unwrap_or(s.from);
    let (date, time) = core::lead_start(&s.duty.date, first, lead);
    let when = omsi_ui::tr("The game starts on %{date} at %{time}, %{lead} min before the first departure: the company's clock, not the computer's.").replace("%{date}", &day_label(&date)).replace("%{time}", &hhmm(time)).replace("%{lead}", &lead.to_string());
    if y + 40.0 < go.y - 8.0 {
        l.ui.icon("schedule", Vec2::new(x + 8.0, y + 9.0), 15.0, q.accent);
        y += l.ui.paragraph(&when, Vec2::new(x + 24.0, y - 2.0), w - 24.0, 12.5, Weight::Regular, q.text) + 8.0;
    }
    if y + 40.0 < go.y - 8.0 {
        l.ui.icon("lock", Vec2::new(x + 8.0, y + 9.0), 14.0, q.faint);
        l.ui.paragraph("Nothing of the duty can be changed here: the planning fixes it. Only the weather is yours to choose; sound, camera and traffic are as in the Settings.", Vec2::new(x + 24.0, y - 2.0), w - 24.0, 12.0, Weight::Regular, q.faint);
    }
    if rf_button(&mut l.ui, q, "duty-launch-start", go, "Start duty", true, true) {
        start(l, c, s);
    }
}

/// What a hitch is about in particular: the bus's problem, the driver planned, the bus file.
fn detail(h: &Hitch) -> Option<String> {
    match h {
        Hitch::NoBus(Some(p)) | Hitch::Bus(p) => Some(omsi_ui::tr(p.label()).into_owned()),
        Hitch::NotYours(name) => Some(omsi_ui::tr("Planned for %{name}").replace("%{name}", &omsi_ui::tr(name))),
        Hitch::NotInstalled(file) => Some(file.clone()),
        _ => None,
    }
}

/// The weathers to choose from: as the month brings it (the default), the map's, the
/// current weather of the airport nearest the map, and the weathers installed.
fn weathers(l: &Launcher, c: &Company) -> (Vec<String>, Vec<String>) {
    let airport = super::super::super::drive::nearest_airport(&l.state.config.root, &c.map);
    let mut values = vec!["cycle".to_string(), String::new(), format!("metar:{airport}")];
    let mut names = vec![omsi_ui::tr("As forecast (weather cycle)").into_owned(), omsi_ui::tr("Map default").into_owned(), omsi_ui::tr("Current weather at %{code}").replace("%{code}", &airport)];
    for w in &l.state.weathers {
        values.push(w.file.clone());
        names.push(w.name.clone());
    }
    (values, names)
}

/// "Start duty": the game with exactly the duty - nothing of the Drive page's choice but the
/// player's game options (`Start::duty`).
fn start(l: &mut Launcher, c: &Company, s: &Sheet) {
    if l.state.in_game() {
        kit::show(l, kit::Popup::new("sports_esports", "A game is running", omsi_ui::tr("The duty waits until the game has ended."), omsi_ui::tr("Close the game, then start the duty again."), None));
        return;
    }
    if !omsi_cfg::missing_original_essentials(std::path::Path::new(&l.state.config.root)).is_empty() {
        kit::show(l, kit::Popup::new("folder_open", "The original OMSI 2 is missing", omsi_ui::tr("A session needs the original OMSI 2."), omsi_ui::tr("Choose its folder under Setup first."), None));
        return;
    }
    let lines = l.company.planning.days.iter().find(|x| x.0 == c.map && x.1 == s.duty.date).and_then(|x| x.2.as_ref().ok()).cloned();
    let Some(lines) = lines else {
        kit::show(l, kit::Popup::new("timer", "Not yet", omsi_ui::tr("The timetable of the day is still being read."), omsi_ui::tr("Try again in a moment."), None));
        return;
    };
    let lead = core::bus_lead(l.state.settings.get("bus_lead").and_then(|x| x.as_i64()));
    let st = match dl::start(s, &lines, lead) {
        Ok(st) => st,
        // (said on the page from now on: the hitch is the sheet's at the next frame too)
        Err(h) => {
            kit::show(l, kit::Popup::new("error", h.words().0, omsi_ui::tr(h.words().1), String::new(), None));
            return;
        }
    };
    let weather = l.company.launch.as_ref().map(|x| x.weather.clone()).unwrap_or_else(|| "cycle".into());
    let mut d = st.duty(l.state.duty(), &weather);
    d.set_vars = l.state.bus_options.for_game(&l.state.config.root, &st.bus, &st.paint);
    d.display_font = l.state.display_fonts.font_for(&st.bus);
    // (a later day's duty: that day's plan for the game too - the company's buses on the road
    // with it; only the company's own day is booked)
    if s.duty.date != c.date {
        if let Some(Ok(p)) = plan_of(l, c, &s.duty.date) {
            if let Err(e) = co::plan::save_drive_plan(&super::super::data(), &co::plan::live_plan(c, &p)) {
                log::warn!("company: the plan of {} for the game: {e:#}", p.date);
            }
        }
    }
    log::info!("company duty launcher: {} {} duty {} on {} with {}", st.line, st.tour, s.duty.duty + 1, st.date, st.bus);
    l.state.set_status(omsi_ui::tr("Starting your company's duty…").into_owned(), false);
    l.state.queued_launch = Some(d);
}

/// The duty's route on the map (the map picture's own, as the Drive page has it): looked at,
/// moved and zoomed - nothing on it is chosen.
fn route_map(l: &mut Launcher, q: &Pal, r: Rect, c: &Company, s: &Sheet) {
    let global = omsi_cfg::find_in_roots(&c.map).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(std::path::Path::new(&l.state.config.root), &c.map));
    l.mapview.want(Look { map: c.map.clone(), global, date: s.duty.date.clone(), trips: s.trip_names(), entry: -1 });
    l.map_background(r);
    l.ui.as_drawn();
    l.ui.p().rounded_border(r, RADIUS, 1.0, q.line);
    let tag = Rect::new(r.x + 10.0, r.y + 10.0, 170.0, 24.0);
    l.ui.p().rounded(tag, 4.0, q.bar.alpha(0.92));
    caps(&mut l.ui, Rect::new(tag.x + 10.0, tag.y, tag.w - 20.0, tag.h), "Route of the duty", q.text, Align::Left);
    l.ui.end_as_drawn();
    let mouse = l.ui.input.mouse;
    let p = Pointer { at: mouse, pressed: l.ui.input.pressed, released: l.ui.input.released, down: l.ui.input.down, wheel: l.ui.input.wheel.y, blocked: !r.contains(mouse) || l.ui.over_popup() };
    let scale = l.ui.scale;
    l.mapview.think(r, r, scale, p);
    // (an entry point clicked is not taken: the start is the duty's)
    let _ = l.mapview.take_clicked();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_texts_are_translated() {
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in ["Company duty · fixed by the planning", "Open in RosterFlow", "Back to My duties", "Start duty", "The duty as planned", "Take the bus over (relief)", "Hand the bus over (relief)", "Deadhead", "Layover", "Bus and depot", "Before you start", "Cannot start", "As forecast (weather cycle)", "Route of the duty", "Planned for %{name}"] {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
            for h in [Hitch::Gone, Hitch::NoBus(None), Hitch::Rental, Hitch::Bus(co::plan::Problem::Workshop), Hitch::NoBusFile, Hitch::NotInstalled(String::new()), Hitch::NoDriver, Hitch::NotYours(String::new()), Hitch::Driven, Hitch::Over, Hitch::Timetable] {
                let (a, b) = h.words();
                assert!(crate::_rust_i18n_try_translate(lang, a).is_some(), "{lang}: {a}");
                assert!(crate::_rust_i18n_try_translate(lang, b).is_some(), "{lang}: {b}");
            }
        }
    }

    #[test]
    fn a_new_launcher_has_the_duty_and_the_forecast_weather() {
        let r = DutyRef { date: "2024-03-04".into(), line: "Linie5".into(), tour: "2".into(), duty: 1 };
        let x = DutyLaunch::new(r.clone(), 6);
        assert_eq!((x.duty, x.from_tab, x.weather.as_str()), (r, 6, "cycle"));
    }
}

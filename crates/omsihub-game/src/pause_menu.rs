//! Omsi-Hub's pause menu (Beta 67, after the concept Luc approved), laid over openOMSI's: the
//! game hands its menu's lines and state to [`draw`] (`omsi-app`'s `shell::hub`), which draws
//! in place of openOMSI's rail; everything the lines do stays the game's.
//!
//! * A floating, rounded glass panel on the left over the paused game, which stays in view
//!   dimmed: the Omsi-Hub wordmark, a big Resume, the lines in small titled groups - this
//!   trip, saving, the settings, multiplayer (only where there is any) - each with its icon,
//!   its key where it has one and a chevron where it opens more; at its foot the driver and
//!   the way out.
//! * On the right, while a duty runs, a card of the trip ([`trip_card`]): its line, where it
//!   goes, how it stands against the timetable, the next stops, the time, the weather and the
//!   driving - as the navigator's board has them (`nav_duty`); what is not known is left out.
//! * At the bottom the keys that matter most, as the game has them ([`keys`]).
//! * A list or settings window opened from the panel floats beside it as a sheet of the same
//!   glass: [`draw`] gives its place, the game draws the list in it.
//!
//! The game menu's lines keep their ids and their actions: [`arrange`] only puts them in the
//! panel's order, so that the keyboard (and a gamepad, [`PadKeys`]) walks them as they are
//! seen.

use glam::Vec2;
use omsi_ui::kit::{id_of, Ui};
use omsi_ui::paint::Align;
use omsi_ui::theme::{accent, accent_deep, on_accent, DANGER_2, INK, INK_2, INK_MUTED, LINE, ON_LINE};
use omsi_ui::{Color, Rect, Weight};
use winit::keyboard::KeyCode;

use crate::nav_duty::{self, Punctuality, Row, Status, StopState};
use omsi_sim::timetable_run::PlayerDuty;

// --- the lines -----------------------------------------------------------------------------

/// The order of the panel's lines (by the game menu's ids): Resume, the trip, saving, the
/// settings, multiplayer, the way out.
const ORDER: [&str; 19] = [
    "resume", "tobus", "duty", "skipstop", "map", "endduty", "save", "saveslot", "load", "options", "controls", "camera", "vehicle", "world", "photo", "shot", "copycode", "admin", "quit",
];

/// The game menu's lines in the panel's order (a line it does not know goes before the way
/// out, the others keep their order among themselves).
pub fn arrange(lines: &mut [(&'static str, &'static str)]) {
    let quit = ORDER.len() - 1;
    lines.sort_by_key(|l| ORDER.iter().position(|o| *o == l.0).map(|k| k * 2).unwrap_or(quit * 2 - 1));
}

/// The group a line is drawn in, by its small title (None: Resume and the way out, which have
/// places of their own).
pub fn section(id: &str) -> Option<&'static str> {
    Some(match id {
        "tobus" | "duty" | "skipstop" | "map" | "endduty" => "This trip",
        "save" | "saveslot" | "load" => "Save",
        "copycode" | "admin" => "Multiplayer",
        "resume" | "quit" => return None,
        _ => "Settings",
    })
}

/// A line's icon (Material Symbols, omsi-ui's set).
fn icon(id: &str) -> &'static str {
    match id {
        "resume" => "play_arrow",
        "tobus" => "directions_walk",
        "duty" => "route",
        "skipstop" => "near_me",
        "map" => "map",
        "endduty" => "flag",
        "save" => "save",
        "saveslot" => "add",
        "load" => "history",
        "options" => "settings",
        "controls" => "sports_esports",
        "camera" => "videocam",
        "vehicle" => "airport_shuttle",
        "world" => "public",
        "photo" | "shot" => "photo_camera",
        "copycode" => "content_copy",
        "admin" => "key",
        "quit" => "logout",
        _ => "chevron_right",
    }
}

/// A label without the "..." that says it opens more, and whether it had one.
fn plain(label: &str) -> (&str, bool) {
    let t = label.trim_end();
    match t.strip_suffix("...").or_else(|| t.strip_suffix('\u{2026}')) {
        Some(x) => (x.trim_end(), true),
        None => (t, false),
    }
}

/// The keys of the lines that have one, and the hints at the bottom (key, what it does), as
/// the game has them: `bound(action)` is the player's own key for one of OMSI's [game]
/// actions (keyboard.cfg), else the game's built-in key stands. Only for lines the menu has.
pub fn keys(lines: &[(&'static str, &'static str)], bound: impl Fn(&str) -> Option<String>) -> (Vec<(&'static str, String)>, Vec<(String, &'static str)>) {
    // (the city map is Shift+M and skipping a stop Ctrl+Shift+H, built in; saving is OMSI's
    // `quicksave`, built in on Alt+S; the photo mode Ctrl+F12, or the player's `photo_mode`)
    let all: [(&'static str, String, Option<&'static str>); 5] = [
        ("resume", "Esc".into(), Some("Resume")),
        ("map", "Shift+M".into(), Some("Map")),
        ("skipstop", "Ctrl+Shift+H".into(), None),
        ("save", bound("quicksave").unwrap_or_else(|| "Alt+S".into()), Some("Save")),
        ("photo", bound("photo_mode").unwrap_or_else(|| "Ctrl+F12".into()), Some("Photo")),
    ];
    let shown: Vec<_> = all.into_iter().filter(|k| lines.iter().any(|l| l.0 == k.0)).collect();
    let hints = shown.iter().filter_map(|(_, key, what)| what.map(|w| (key.clone(), w))).collect();
    (shown.into_iter().map(|(id, key, _)| (id, key)).collect(), hints)
}

// --- what the game hands over ----------------------------------------------------------------

/// The menu as the game has it this frame.
pub struct Menu<'a> {
    /// The game menu's lines (`arrange`d), the one the keyboard is on, and the one whose list
    /// is open.
    pub lines: &'a [(&'static str, &'static str)],
    pub sel: Option<usize>,
    pub open: Option<&'a str>,
    /// A list or settings window is open (it goes in the sheet beside the panel).
    pub list: bool,
    /// The keyboard chose last (its line is shown lit); a keyboard is there (key hints).
    pub kbd: bool,
    pub keyboard: bool,
    /// The simulation stands still (not in a LAN session).
    pub paused: bool,
    pub version: &'a str,
    pub driver: &'a str,
    pub bus: &'a str,
    pub chips: &'a [(&'static str, String)],
    pub hints: &'a [(String, &'static str)],
    pub trip: Option<&'a TripCard>,
    /// Where the menu lays itself out (points): the window, or a triple screen's middle one.
    pub stage: Rect,
    /// The wordmark's texture (`wordmark_pixels`), when the game has it on the card.
    pub mark: Option<usize>,
}

/// What a frame of the menu came to.
#[derive(Debug, Default, PartialEq)]
pub struct Drawn {
    /// The line clicked.
    pub picked: Option<usize>,
    /// Where the open list goes.
    pub sheet: Option<Rect>,
}

/// The trip as the card shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TripCard {
    /// The line (None: an empty run, which has none).
    pub line: Option<String>,
    /// Where the trip starts (empty where that is where it goes) and where it goes.
    pub from: String,
    pub to: String,
    /// How it stands: the chip's words and colour (green on time, red late, blue early);
    /// None on a free drive, which keeps no time.
    pub status: Option<(String, Color)>,
    /// "Tour 3 · trip 2 of 6".
    pub sub: String,
    /// The next stops (four at most), the one the bus heads for first.
    pub stops: Vec<CardStop>,
    /// (icon, label, value): the time, the weather, the driving.
    pub stats: Vec<(&'static str, &'static str, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardStop {
    pub name: String,
    /// Its planned time (empty on a free drive).
    pub time: String,
    /// The one the bus heads for (or stands at).
    pub next: bool,
}

/// The card of the player's duty at the time of day `now` (s) - during the break at a
/// terminus the trip that comes next - with the session's weather and driving rating (per
/// cent).
pub fn trip_card(d: &PlayerDuty, now: f64, weather: String, driving: f64) -> Option<TripCard> {
    card_of(&nav_duty::DutyState::of(d, now)?, d.free, now, weather, driving)
}

/// [`trip_card`] of the duty as the navigator's board sees it (`free`: a free drive along a
/// line, which keeps no time).
fn card_of(state: &nav_duty::DutyState, free: bool, now: f64, weather: String, driving: f64) -> Option<TripCard> {
    let k = state.focus();
    let trip = state.trips.get(k)?;
    let rows = nav_duty::board(Some(state), 4);
    let (line, status) = rows.iter().find_map(|r| match r {
        Row::Head { line, status, .. } => Some((line.clone(), *status)),
        _ => None,
    })?;
    let status = (!free).then(|| {
        let fill = match status {
            Status::Running(late) => match nav_duty::punctuality(late) {
                Punctuality::Late => nav_duty::LATE,
                Punctuality::Early => nav_duty::EARLY,
                Punctuality::OnTime => nav_duty::ON_TIME,
            },
            Status::Break(left) if left < 0.0 => nav_duty::LATE,
            Status::Finished => nav_duty::ON_TIME,
            _ => Color::rgba(255, 255, 255, 0.14),
        };
        (capitalised(&status.label()), fill)
    });
    let tour = state.tours.get(k).map(|t| t.1.trim()).unwrap_or(state.tour.trim()).to_string();
    let first = if free { omsi_ui::tr("Free drive").into_owned() } else { nav_duty::tr_with("Tour %{tour}", &[("tour", tour)]) };
    let sub = format!("{first}  \u{b7}  {}", nav_duty::tr_with("trip %{k} of %{n}", &[("k", (k + 1).to_string()), ("n", state.trips.len().to_string())]));
    let stops = rows
        .iter()
        .filter_map(|r| match r {
            Row::Stop { planned, name, state, .. } if *state != StopState::Done => Some(CardStop { name: name.clone(), time: if free { String::new() } else { nav_duty::hhmm(*planned) }, next: *state == StopState::Now }),
            _ => None,
        })
        .take(4)
        .collect();
    let from = trip.stops.iter().find(|s| s.stops).map(|s| s.name.trim().to_string()).unwrap_or_default();
    let to = Some(trip.terminus.trim()).filter(|t| !t.is_empty()).map(str::to_string).or_else(|| trip.stops.last().map(|s| s.name.trim().to_string())).unwrap_or_default();
    let mut stats = vec![("schedule", "Time", nav_duty::hhmm(now))];
    if !weather.trim().is_empty() {
        stats.push(("partly_cloudy_day", "Weather", weather.trim().to_string()));
    }
    stats.push(("speed", "Driving", format!("{driving:.0} %")));
    Some(TripCard { line, from: if from == to { String::new() } else { from }, to, status, sub, stops, stats })
}

/// `s` with its first letter a capital ("on time" on the card's chip).
fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

// --- the wordmark -------------------------------------------------------------------------------

/// The wordmark - the bus in its ring, "Omsi-" and "Hub" - made from the launcher's mark
/// (`launcher/ui/js/omsihub/logo.js`, without the line and its stops).
const WORDMARK: &str = include_str!("pause_menu/wordmark.svg");
/// Its height in the panel (points), and its width to that.
pub const MARK_H: f32 = 34.0;
pub const MARK_ASPECT: f32 = 1614.6 / 388.0;

/// What the wordmark's texture is drawn for at `scale` physical pixels a point: its height in
/// pixels (as large as it is shown - a larger texture squeezed without mipmaps comes out
/// blurred and jagged) and the player's accent, which its ring is drawn in as the launcher's
/// ("Hub" keeps the logo's orange).
pub fn wordmark_key(scale: f32) -> (u32, u32) {
    ((MARK_H * scale).round().max(8.0) as u32, omsi_ui::accent::chosen())
}

/// The wordmark for `key` as straight RGBA: (width, height, pixels).
pub fn wordmark_pixels((h, rgb): (u32, u32)) -> Option<(u32, u32, Vec<u8>)> {
    let w = (h as f32 * MARK_ASPECT).round() as u32;
    let ring = format!("stroke=\"{}\" stroke-width", omsi_ui::accent::hex(rgb));
    let svg = WORDMARK.replacen("stroke=\"#F58620\" stroke-width", &ring, 1);
    omsi_ui::icons::rasterize_rgba(&svg, w, h).map(|px| (w, h, px))
}

// --- a gamepad ------------------------------------------------------------------------------------

/// A gamepad going through the menu: the d-pad (or the left stick) as the arrow keys, A as
/// Enter, B as Esc; a direction held repeats as a key held does.
#[derive(Debug, Default)]
pub struct PadKeys {
    /// Held last frame: up, down, left, right, A, B.
    held: [bool; 6],
    hold: f32,
}

/// After this long held a direction repeats, every `REPEAT` (s).
const DELAY: f32 = 0.40;
const REPEAT: f32 = 0.11;

impl PadKeys {
    /// What is held as the menu opens (the button that opened it) is no press.
    pub fn start(now: [bool; 6]) -> PadKeys {
        PadKeys { held: now, hold: 0.0 }
    }

    /// The keys pressed by the buttons held `now` (`dt` s after the last frame).
    pub fn step(&mut self, now: [bool; 6], dt: f32) -> Vec<KeyCode> {
        const KEYS: [KeyCode; 6] = [KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::Enter, KeyCode::Escape];
        let mut out: Vec<KeyCode> = (0..6).filter(|&k| now[k] && !self.held[k]).map(|k| KEYS[k]).collect();
        let dirs: Vec<usize> = (0..4).filter(|&k| now[k]).collect();
        if dirs.len() == 1 && self.held[dirs[0]] {
            self.hold += dt;
            if self.hold >= DELAY {
                out.push(KEYS[dirs[0]]);
                self.hold = DELAY - REPEAT;
            }
        } else {
            self.hold = 0.0;
        }
        self.held = now;
        out
    }
}

// --- drawing ----------------------------------------------------------------------------------------

/// The glass (#12151B at 90 %), its edge, the rules in it.
const GLASS: Color = Color::rgba(18, 21, 27, 0.90);
const GLASS_EDGE: Color = Color::rgba(255, 255, 255, 0.08);
const RULE: Color = Color::rgba(255, 255, 255, 0.07);
/// A row under the mouse or the keyboard, and the row whose list is open.
const ROW_HOT: Color = Color::rgba(255, 255, 255, 0.06);
const ROW_OPEN: Color = Color::rgba(255, 255, 255, 0.10);
/// What lies over the game: everywhere, more behind an open list, more towards the panel.
const VEIL: Color = Color::rgba(6, 8, 12, 1.0);
/// The way from the window's edges, the panel's width and corners.
const MARGIN: f32 = 24.0;
const PANEL_W: f32 = 340.0;
const RADIUS: f32 = 16.0;
/// A row, a group's small title, the room between two groups.
const ROW_H: f32 = 34.0;
const SECTION_H: f32 = 26.0;
const SECTION_GAP: f32 = 17.0;
/// The panel's head (the wordmark), Resume, its foot (the driver and the way out).
const HEAD_H: f32 = 74.0;
const RESUME_H: f32 = 48.0;
const FOOT_H: f32 = 72.0;
/// The trip card's width.
const CARD_W: f32 = 380.0;
/// Wider than this (an ultrawide screen) the menu keeps to the middle of the window.
const MAX_ASPECT: f32 = 2.4;

/// The stage, kept to the middle of an ultrawide one.
fn fit(stage: Rect) -> Rect {
    let w = stage.w.min(stage.h * MAX_ASPECT);
    Rect::new(stage.x + (stage.w - w) * 0.5, stage.y, w, stage.h)
}

/// The menu over the game: the veil, the panel, and with no list open the trip card and the
/// key hints; with one open, the sheet it goes in (beside the panel, or on a narrow window in
/// its place, where the list's own way back leads to the panel).
pub fn draw(ui: &mut Ui, m: &Menu) -> Drawn {
    let size = ui.size;
    let stage = fit(m.stage);
    ui.p().rect(Rect::new(0.0, 0.0, size.x, size.y), VEIL.alpha(if m.list { 0.62 } else { 0.38 }));
    ui.p().gradient_h(Rect::new(0.0, 0.0, (stage.x + PANEL_W + MARGIN * 6.0).min(size.x), size.y), VEIL.alpha(0.35), VEIL.alpha(0.0));
    let tall = Rect::new(stage.x + MARGIN, stage.y + MARGIN, PANEL_W.min(stage.w - MARGIN * 2.0), stage.h - MARGIN * 2.0);
    let rows = body_rows(m);
    let panel = Rect::new(tall.x, tall.y, tall.w, tall.h.min(HEAD_H + RESUME_H + 14.0 + body_height(&rows) + 10.0 + FOOT_H));
    let mut out = Drawn::default();
    if m.list {
        let room = stage.right() - MARGIN - (panel.right() + 16.0);
        let beside = room >= 480.0;
        let sheet = if beside { Rect::new(panel.right() + 16.0, tall.y, room, tall.h) } else { Rect::new(stage.x + MARGIN, tall.y, stage.w - MARGIN * 2.0, tall.h) };
        glass(ui, sheet);
        out.sheet = Some(sheet);
        if beside {
            out.picked = panel_draw(ui, m, &rows, panel);
        }
        return out;
    }
    let free = Rect::new(panel.right() + MARGIN, stage.y, stage.right() - panel.right() - MARGIN, stage.h);
    if let Some(t) = m.trip.filter(|_| free.w >= CARD_W + MARGIN) {
        card(ui, t, Vec2::new(stage.right() - MARGIN - CARD_W, stage.y + MARGIN));
    }
    if m.keyboard {
        hints(ui, m.hints, free, stage.bottom() - MARGIN - 13.0);
    }
    out.picked = panel_draw(ui, m, &rows, panel);
    out
}

/// The glass of a panel, sheet or card: a soft shadow, the fill, a hairline edge.
fn glass(ui: &mut Ui, r: Rect) {
    ui.solid(r);
    ui.p().shadow(r.pad(0.0, 4.0), RADIUS, 28.0, Color::rgba(0, 0, 0, 0.30));
    ui.p().rounded(r, RADIUS, GLASS);
    ui.p().rounded_border(r, RADIUS, 1.0, GLASS_EDGE);
}

/// The lines of the panel's middle (not Resume, not the way out): (index among the lines,
/// id, label, the group's title where a new group begins).
fn body_rows(m: &Menu) -> Vec<(usize, &'static str, &'static str, Option<&'static str>)> {
    let mut last: Option<&'static str> = None;
    m.lines
        .iter()
        .enumerate()
        .filter(|(_, l)| section(l.0).is_some())
        .map(|(k, l)| {
            let s = section(l.0);
            let new = s.filter(|s| last != Some(s));
            last = s;
            (k, l.0, l.1, new)
        })
        .collect()
}

/// How tall the middle's lines are, with their titles and the room between the groups.
fn body_height(rows: &[(usize, &str, &str, Option<&str>)]) -> f32 {
    rows.iter().enumerate().map(|(n, r)| ROW_H + if r.3.is_some() { SECTION_H + if n > 0 { SECTION_GAP } else { 0.0 } } else { 0.0 }).sum()
}

/// A key's chip ("Esc", "Shift+M") ending at `right`, centred on `cy`; returns its width.
fn chip(ui: &mut Ui, key: &str, right: f32, cy: f32, on_accent_fill: bool) -> f32 {
    let px = 11.0;
    let w = ui.width(key, px, Weight::Medium) + 14.0;
    let r = Rect::new(right - w, cy - 11.0, w, 22.0);
    let (fill, edge, ink) = if on_accent_fill { (Color::rgba(255, 255, 255, 0.20), Color::rgba(255, 255, 255, 0.28), on_accent()) } else { (Color::rgba(255, 255, 255, 0.06), Color::rgba(255, 255, 255, 0.16), INK_2) };
    ui.p().rounded(r, 6.0, fill);
    ui.p().rounded_border(r, 6.0, 1.0, edge);
    ui.text_as_is(key, Vec2::new(r.center().x, cy + 4.0), px, Weight::Medium, ink, Align::Center);
    w
}

/// The panel: the wordmark, Resume, the lines in their groups, the driver and the way out.
/// Returns the line clicked.
fn panel_draw(ui: &mut Ui, m: &Menu, rows: &[(usize, &'static str, &'static str, Option<&'static str>)], panel: Rect) -> Option<usize> {
    glass(ui, panel);
    let any_hover = ui.hover(panel);
    let lit = |k: usize| m.kbd && !any_hover && !m.list && m.sel == Some(k);
    let mut picked: Option<usize> = None;
    // the head: the wordmark, the version and the state small beside it
    let mark = Rect::new(panel.x + 20.0, panel.y + 20.0, MARK_H * MARK_ASPECT, MARK_H);
    match m.mark {
        Some(tex) => ui.image(mark, tex, 0.0),
        None => {
            ui.text_as_is(crate::APP_NAME, Vec2::new(mark.x, mark.y + 26.0), 24.0, Weight::Bold, INK, Align::Left);
        }
    }
    let vx = panel.right() - 20.0;
    ui.text_as_is(m.version, Vec2::new(vx, mark.y + 14.0), 11.0, Weight::Medium, INK_MUTED, Align::Right);
    // (in a LAN session the game runs on under the menu)
    let state = omsi_ui::tr(if m.paused { "Paused" } else { "Menu" }).into_owned();
    ui.text_as_is(&state, Vec2::new(vx, mark.y + 30.0), 11.0, Weight::Medium, INK_2, Align::Right);
    // Resume
    let b = Rect::new(panel.x + 16.0, panel.y + HEAD_H, panel.w - 32.0, RESUME_H);
    if let Some(k) = m.lines.iter().position(|l| l.0 == "resume") {
        let (h, held, clicked) = ui.interact(id_of("pause-resume"), b);
        if clicked {
            picked = Some(k);
        }
        let t = ui.anim(id_of("pause-resume-hl"), if h || lit(k) { 1.0 } else { 0.0 }, 0.06);
        ui.p().shadow(b.pad(0.0, 3.0), 12.0, 16.0, accent().alpha(0.18 + 0.12 * t));
        ui.p().rounded_gradient(b, 12.0, accent().mix(Color::WHITE, 0.06 * t), accent().mix(accent_deep(), if held { 0.9 } else { 0.55 }));
        if lit(k) {
            ui.p().rounded_border(b.inset(-3.0), 15.0, 2.0, Color::WHITE.alpha(0.85));
        }
        ui.icon("play_arrow", Vec2::new(b.x + 26.0, b.center().y), 22.0, on_accent());
        ui.text_in(m.lines[k].1, Rect::new(b.x + 46.0, b.y, b.w - 110.0, b.h), 15.5, Weight::Bold, on_accent(), Align::Left);
        if let Some((_, key)) = m.chips.iter().find(|c| c.0 == "resume") {
            chip(ui, key, b.right() - 14.0, b.center().y, true);
        }
    }
    // the lines, scrolled when the window is too low for them all
    let foot_y = panel.bottom() - FOOT_H;
    let top = b.bottom() + 14.0;
    let area = Rect::new(panel.x, top, panel.w, (foot_y - 10.0 - top).max(ROW_H));
    let mut sel_y: Option<f32> = None;
    ui.scroll_area("pause-rail", area, &mut |ui, view| {
        let mut y = view.y;
        for (n, &(k, id, label, title)) in rows.iter().enumerate() {
            if let Some(t) = title {
                if n > 0 {
                    y += 8.0;
                    ui.p().rect(Rect::new(view.x + 20.0, y, view.w - 40.0, 1.0), RULE);
                    y += SECTION_GAP - 8.0;
                }
                let words = omsi_ui::tr(t).to_uppercase();
                ui.text_as_is(&words, Vec2::new(view.x + 22.0, y + 17.0), 11.0, Weight::Bold, INK_MUTED, Align::Left);
                y += SECTION_H;
            }
            let row = Rect::new(view.x + 10.0, y, view.w - 20.0, ROW_H);
            let (h, _, clicked) = ui.interact(id_of(&format!("pause-{id}")), row);
            if clicked {
                picked = Some(k);
            }
            if lit(k) {
                sel_y = Some(y - view.y);
            }
            let open = m.open == Some(id);
            let t = ui.anim(id_of(&format!("pause-hl-{id}")), if h || lit(k) { 1.0 } else { 0.0 }, 0.06);
            if open {
                ui.p().rounded(row, 8.0, ROW_OPEN);
            } else if t > 0.01 {
                ui.p().rounded(row, 8.0, ROW_HOT.alpha(ROW_HOT.0[3] * t));
            }
            let translated = omsi_ui::tr(label);
            let (text, more) = plain(&translated);
            let ink = if open { INK } else { INK_2.mix(INK, t) };
            ui.icon(icon(id), Vec2::new(row.x + 20.0, row.center().y), 18.0, if open || lit(k) { accent() } else { ink });
            let mut right = row.right() - 12.0;
            if more {
                ui.icon("chevron_right", Vec2::new(row.right() - 14.0, row.center().y), 16.0, INK_MUTED.mix(INK, t));
                right -= 18.0;
            }
            if let Some((_, key)) = m.chips.iter().find(|c| c.0 == id) {
                right -= chip(ui, key, right, row.center().y, false) + 8.0;
            }
            ui.text_in(text, Rect::new(row.x + 42.0, row.y, (right - row.x - 48.0).max(10.0), row.h), 14.0, Weight::Medium, ink, Align::Left);
            y += ROW_H;
        }
        y - view.y + 4.0
    });
    if let (Some(y), true) = (sel_y, m.kbd) {
        ui.scroll_to("pause-rail", y, ROW_H, area.h);
    }
    // the foot: the way out on the right, the driver beside it
    ui.p().rect(Rect::new(panel.x + 16.0, foot_y, panel.w - 32.0, 1.0), RULE);
    let cy = foot_y + FOOT_H * 0.5;
    let mut name_right = panel.right() - 16.0;
    if let Some(k) = m.lines.iter().position(|l| l.0 == "quit") {
        let label = omsi_ui::tr(m.lines[k].1).into_owned();
        let w = (ui.width(&label, 13.5, Weight::Bold) + 50.0).min(panel.w * 0.55);
        let r = Rect::new(panel.right() - 12.0 - w, cy - 20.0, w, 40.0);
        name_right = r.x - 20.0;
        let (h, _, clicked) = ui.interact(id_of("pause-quit"), r);
        if clicked {
            picked = Some(k);
        }
        let t = ui.anim(id_of("pause-hl-quit"), if h || lit(k) { 1.0 } else { 0.0 }, 0.06);
        if t > 0.01 {
            ui.p().rounded(r, 8.0, DANGER_2.alpha(0.14 * t));
        }
        if lit(k) {
            ui.p().rounded_border(r, 8.0, 1.5, DANGER_2.alpha(0.8));
        }
        let ink = DANGER_2.mix(Color::rgba(255, 176, 166, 1.0), t);
        ui.icon("logout", Vec2::new(r.x + 20.0, cy), 18.0, ink);
        ui.text_as_is(&label, Vec2::new(r.x + 36.0, cy + 5.0), 13.5, Weight::Bold, ink, Align::Left);
        ui.p().rect(Rect::new(r.x - 10.0, cy - 16.0, 1.0, 32.0), RULE);
    }
    let av = Vec2::new(panel.x + 40.0, cy);
    ui.p().circle(av, 20.0, Color::rgba(255, 255, 255, 0.10));
    ui.p().rounded_border(Rect::new(av.x - 20.0, av.y - 20.0, 40.0, 40.0), 20.0, 1.0, Color::rgba(255, 255, 255, 0.10));
    let initial = m.driver.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    ui.text_as_is(&initial, Vec2::new(av.x, av.y + 5.5), 15.0, Weight::Bold, INK_2, Align::Center);
    let tx = av.x + 30.0;
    let tw = (name_right - tx).max(20.0);
    let (name_y, px) = if m.bus.is_empty() { (cy - 10.0, 14.0) } else { (cy - 19.0, 14.0) };
    ui.text_in(m.driver, Rect::new(tx, name_y, tw, 20.0), px, Weight::Bold, INK, Align::Left);
    if !m.bus.is_empty() {
        ui.text_in(m.bus, Rect::new(tx, cy + 1.0, tw, 18.0), 12.0, Weight::Regular, INK_MUTED, Align::Left);
    }
    picked
}

/// The trip card, its top left corner at `at`.
fn card(ui: &mut Ui, t: &TripCard, at: Vec2) {
    let (w, x, pad) = (CARD_W, at.x, 20.0);
    // the head: the line's plate, where the trip goes, how it stands, the tour
    let plate = Rect::new(x + pad, at.y + pad, 48.0, 48.0);
    let pill = t.status.as_ref().map(|(words, _)| ui.width(words, 12.0, Weight::Bold) + 40.0).unwrap_or(0.0);
    let tx = plate.right() + 14.0;
    let title_w = (x + w - pad - tx - if pill > 0.0 { pill + 10.0 } else { 0.0 }).max(40.0);
    let one = if t.from.is_empty() { t.to.clone() } else { format!("{} \u{2192} {}", t.from, t.to) };
    let lines: Vec<String> = if t.from.is_empty() || ui.width(&one, 17.0, Weight::Bold) <= title_w { vec![one] } else { vec![format!("{} \u{2192}", t.from), t.to.clone()] };
    let head_h = (lines.len() as f32 * 22.0 + 22.0).max(48.0);
    let stops_h = if t.stops.is_empty() { 0.0 } else { 24.0 + t.stops.len() as f32 * 30.0 };
    let stats_h = if t.stats.is_empty() { 4.0 } else { 78.0 };
    glass(ui, Rect::new(x, at.y, w, pad + head_h + 16.0 + stops_h + stats_h));
    match t.line.as_deref() {
        Some(l) => {
            ui.p().rounded(plate, 10.0, LINE);
            let px = if l.chars().count() > 3 { 15.0 } else { 20.0 };
            ui.text_in(l, plate.pad(4.0, 0.0), px, Weight::Black, ON_LINE, Align::Center);
        }
        None => {
            ui.p().rounded(plate, 10.0, Color::rgba(255, 255, 255, 0.08));
            ui.icon("directions_bus", plate.center(), 24.0, INK_2);
        }
    }
    let mut y = plate.y;
    for l in &lines {
        ui.text_in(l, Rect::new(tx, y, title_w, 22.0), 17.0, Weight::Bold, INK, Align::Left);
        y += 22.0;
    }
    ui.text_in(&t.sub, Rect::new(tx, y + 2.0, x + w - pad - tx, 18.0), 12.5, Weight::Medium, INK_MUTED, Align::Left);
    if let Some((words, fill)) = t.status.as_ref() {
        let r = Rect::new(x + w - pad - pill, plate.y + 2.0, pill, 26.0);
        ui.p().rounded(r, 13.0, *fill);
        ui.icon("schedule", Vec2::new(r.x + 16.0, r.center().y), 14.0, Color::WHITE);
        ui.text_as_is(words, Vec2::new(r.x + 28.0, r.center().y + 4.5), 12.0, Weight::Bold, Color::WHITE, Align::Left);
    }
    let mut y = at.y + pad + head_h + 16.0;
    // the next stops on a line, the one the bus heads for lit
    if !t.stops.is_empty() {
        ui.p().rect(Rect::new(x + pad, y, w - pad * 2.0, 1.0), RULE);
        y += 12.0;
        let dot_x = x + pad + 14.0;
        for (i, s) in t.stops.iter().enumerate() {
            let cy = y + 15.0;
            if i + 1 < t.stops.len() {
                ui.p().rect(Rect::new(dot_x - 1.0, cy, 2.0, 30.0), Color::rgba(255, 255, 255, 0.16));
            }
            if s.next {
                ui.p().circle(Vec2::new(dot_x, cy), 8.0, accent().alpha(0.25));
                ui.p().circle(Vec2::new(dot_x, cy), 5.5, accent());
            } else {
                ui.p().circle(Vec2::new(dot_x, cy), 5.0, Color::rgba(150, 156, 168, 1.0));
            }
            let (ink, weight) = if s.next { (INK, Weight::Bold) } else { (INK_2, Weight::Medium) };
            ui.text_in(&s.name, Rect::new(dot_x + 22.0, y, w - pad * 2.0 - 110.0, 30.0), 14.0, weight, ink, Align::Left);
            ui.text_as_is(&s.time, Vec2::new(x + w - pad - 6.0, cy + 5.0), 14.0, weight, if s.next { accent() } else { INK_2 }, Align::Right);
            y += 30.0;
        }
        y += 12.0;
    }
    // the time, the weather, the driving
    if !t.stats.is_empty() {
        ui.p().rect(Rect::new(x + pad, y, w - pad * 2.0, 1.0), RULE);
        let cw = (w - pad * 2.0) / t.stats.len() as f32;
        for (i, (glyph, label, value)) in t.stats.iter().enumerate() {
            let cx = x + pad + cw * (i as f32 + 0.5);
            if i > 0 {
                ui.p().rect(Rect::new(x + pad + cw * i as f32, y + 14.0, 1.0, 50.0), RULE);
            }
            let words = omsi_ui::tr(label).into_owned();
            let lw = ui.width(&words, 12.0, Weight::Medium) + 20.0;
            ui.icon(glyph, Vec2::new(cx - lw * 0.5 + 7.0, y + 24.0), 14.0, INK_MUTED);
            ui.text_as_is(&words, Vec2::new(cx - lw * 0.5 + 18.0, y + 28.5), 12.0, Weight::Medium, INK_MUTED, Align::Left);
            ui.text_in(value, Rect::new(cx - cw * 0.5 + 6.0, y + 38.0, cw - 12.0, 28.0), 18.0, Weight::Bold, INK, Align::Center);
        }
    }
}

/// The key hints in a line at height `cy`, in the middle of `free`: each key's chip and what
/// it does, dots between them (none where they do not fit).
fn hints(ui: &mut Ui, hints: &[(String, &'static str)], free: Rect, cy: f32) {
    if hints.is_empty() {
        return;
    }
    let words: Vec<String> = hints.iter().map(|h| omsi_ui::tr(h.1).into_owned()).collect();
    let key_w = |ui: &Ui, k: usize| ui.width(&hints[k].0, 11.0, Weight::Medium) + 14.0;
    let item_w = |ui: &Ui, k: usize| key_w(ui, k) + 8.0 + ui.width(&words[k], 13.0, Weight::Medium);
    let gap = 26.0;
    let total: f32 = (0..hints.len()).map(|k| item_w(ui, k)).sum::<f32>() + gap * (hints.len() - 1) as f32;
    if total > free.w {
        return;
    }
    let mut x = free.center().x - total * 0.5;
    for k in 0..hints.len() {
        let kw = key_w(ui, k);
        chip(ui, &hints[k].0, x + kw, cy, false);
        ui.text_as_is(&words[k], Vec2::new(x + kw + 8.0, cy + 4.5), 13.0, Weight::Medium, INK_2, Align::Left);
        x += item_w(ui, k);
        if k + 1 < hints.len() {
            ui.p().circle(Vec2::new(x + gap * 0.5, cy), 1.6, INK_MUTED);
            x += gap;
        }
    }
}

#[cfg(test)]
mod tests;

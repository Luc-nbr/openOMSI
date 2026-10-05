//! The line editor: a line of the player's own on any map. The map fills the middle of the
//! page with every bus stop on it; the player clicks the stops in order and the way between
//! each two is found over the roads (`lineroute`) - kerb side, lane changes, loose ends, as the
//! game's buses will drive it. "Reverse" makes the way back from the stops across the road.
//! A leg is dragged through a point of the player's choosing; a stop is put in after the one
//! chosen in the list, moved up or down, or taken out there.
//!
//! On the left the player's lines of the map and the line's own settings (number, name,
//! colour, the depot whose buses drive it); on the right a direction's stops with the minutes
//! to each, its destination, and the timetable: per group of days the first and the last
//! departure, how often, and how long a bus stands at the end - made into tours, a bus going
//! back and forth (`core::lines::blocks`).
//!
//! Saved, the line goes into the registry (`~/.openomsi/lines/<map>.json`, the source of
//! truth) and from there into the map's timetable files (`core::lines::export_to_map`), so
//! that the player drives it from the Drive page like any line and the timetable's buses
//! drive it too.

use super::lineroute::{Anchor, Router};
use super::mapview::{Dot, Look, Pointer};
use super::theme::*;
use super::ui::{ButtonKind, Ui};
use super::Launcher;
use glam::{DVec2, Vec2};
use omsi_launcher_lib as core;
use omsi_launcher_lib::linehof;
use omsi_launcher_lib::lines::{self as reg, Direction, LineDesign, Registry, StopRef, DAY_GROUPS};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The colours a line may have.
const PALETTE: [&str; 8] = ["#2a75f7", "#e5484d", "#30a46c", "#f5a524", "#8e4ec6", "#12a594", "#e93d82", "#7a8ca6"];
/// How near the mouse must come to a stop, a dragged point or a leg (points).
const STOP_HIT: f32 = 10.0;
const VIA_HIT: f32 = 9.0;
const LEG_HIT: f32 = 6.0;
/// A stop of the same name across the road is at most this far (metres).
const ACROSS: f64 = 150.0;

/// What the mouse is dragging over the map.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// A point a leg is dragged through: (leg, its place among the leg's points).
    Via(usize, usize),
    /// A new point, pulled out of a leg.
    New(usize),
}

#[derive(Default)]
pub struct LineEditorView {
    /// The map (an index into the map list) and the map file the registry was read for.
    map: usize,
    loaded: Option<String>,
    map_dir: PathBuf,
    folder: String,
    global: PathBuf,
    reg: Registry,
    reg_path: PathBuf,
    /// The line shown, its direction (0 out, 1 back) and the stop chosen in its list.
    sel: Option<u64>,
    dir: usize,
    sel_stop: Option<usize>,
    /// Changed and not saved.
    dirty: bool,
    /// The right panel: 0 the stops, 1 the timetable.
    tab: usize,
    /// The stops' names from the map's `Busstops.cfg`.
    names: HashMap<i64, String>,
    /// The depot groups with buses (`ailists.cfg`) and each one's depot file.
    groups: Vec<(String, String)>,
    /// The depot files of the depot groups as the line editor sees them, without its own
    /// block (by name, lowercased; read once).
    depots: HashMap<String, Option<Arc<linehof::Depot>>>,
    /// The sign's preview lit white rather than amber.
    white: bool,
    /// The router of the map's network (the network's address, to see it is still that one),
    /// and the map's stops as the line editor offers them.
    router: Option<(usize, Router)>,
    stops: Vec<StopRef>,
    /// Each direction's legs as drawn (world points), and for which line they were made.
    shapes: [Vec<Vec<DVec2>>; 2],
    shapes_for: Option<u64>,
    /// Bumped whenever what the map draws of the line changes.
    revision: u64,
    drag: Option<Drag>,
    delete_armed: bool,
    hover_stop: Option<usize>,
    /// "Drive it" was pressed: the Drive page next.
    go_drive: bool,
}

fn colour_of(hex: &str) -> Color {
    let h = hex.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0x2a75f7);
    Color::rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0)
}

fn dv(a: [f64; 2]) -> DVec2 {
    DVec2::new(a[0], a[1])
}

/// The ground points of a leg the router could not route: straight through its points.
fn straight(d: &Direction, k: usize) -> Vec<DVec2> {
    let mut v = vec![dv(d.stops[k].at)];
    v.extend(d.legs[k].vias.iter().map(|p| dv(*p)));
    v.push(dv(d.stops[k + 1].at));
    v
}

/// Route every leg of a direction again (each from where the one before ended), its times
/// with them; returns the legs as drawn.
fn route_direction(router: &Router, d: &mut Direction) -> Vec<Vec<DVec2>> {
    d.fit_legs();
    let mut shapes = Vec::new();
    let mut start: Option<Anchor> = None;
    for k in 0..d.legs.len() {
        let (a, b) = (dv(d.stops[k].at), dv(d.stops[k + 1].at));
        let vias: Vec<DVec2> = d.legs[k].vias.iter().map(|p| dv(*p)).collect();
        // (from where the leg before ended; failing that, from any lane of the stop)
        let r = router.leg(a, b, &vias, start).or_else(|| start.and_then(|_| router.leg(a, b, &vias, None)));
        let leg = &mut d.legs[k];
        match r {
            Some(r) => {
                leg.steps = router.steps(&r);
                leg.length = r.length;
                leg.ok = !leg.steps.is_empty();
                (leg.from_s, leg.to_s, leg.from_lat, leg.to_lat) = (r.from.s, r.to.s, r.from.lateral.abs(), r.to.lateral.abs());
                shapes.push(router.points(&r.lanes, r.from.s, r.to.s));
                start = Some(Anchor { beside: None, ..r.to });
            }
            None => {
                leg.steps.clear();
                leg.ok = false;
                leg.length = (b - a).length() as f32;
                shapes.push(straight(d, k));
                start = None;
            }
        }
    }
    d.refresh_times();
    shapes
}

/// A saved direction's legs as drawn, from the lanes it keeps.
fn stored_shapes(router: &Router, d: &Direction) -> Vec<Vec<DVec2>> {
    (0..d.legs.len().min(d.stops.len().saturating_sub(1)))
        .map(|k| {
            let g = &d.legs[k];
            let lanes = router.lanes_of(&g.steps);
            if g.ok && !lanes.is_empty() {
                router.points(&lanes, g.from_s, g.to_s)
            } else {
                straight(d, k)
            }
        })
        .collect()
}

/// The distance from `p` to a polyline (all in picture points).
fn to_polyline(p: Vec2, pts: &[Vec2]) -> f32 {
    let mut best = f32::MAX;
    for w in pts.windows(2) {
        let ab = w[1] - w[0];
        let t = ((p - w[0]).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
        best = best.min((w[0] + ab * t - p).length());
    }
    best
}

impl LineEditorView {
    fn line(&self) -> Option<&LineDesign> {
        self.sel.and_then(|id| self.reg.line(id))
    }

    fn line_mut(&mut self) -> Option<&mut LineDesign> {
        let id = self.sel?;
        self.reg.line_mut(id)
    }

    /// Something of the line changed: unsaved, and drawn again.
    fn touched(&mut self) {
        self.dirty = true;
        self.revision += 1;
    }

    /// The direction `dir` of the shown line routed again.
    fn reroute(&mut self, dir: usize) {
        let Some((_, router)) = self.router.as_ref() else { return };
        let Some(id) = self.sel else { return };
        let Some(l) = self.reg.lines.iter_mut().find(|l| l.id == id) else { return };
        if let Some(d) = l.directions.get_mut(dir) {
            self.shapes[dir] = route_direction(router, d);
        }
        self.touched();
    }

    /// Read the map's registry, names and depot groups (another map was chosen).
    fn load_map(&mut self, file: &str, root: &Path, date: &str) {
        self.loaded = Some(file.to_string());
        self.global = omsi_cfg::find_in_roots(file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(root, file));
        self.map_dir = self.global.parent().map(Path::to_path_buf).unwrap_or_default();
        self.folder = reg::map_folder(file);
        self.reg_path = reg::registry_path(&self.folder);
        self.reg = reg::load_registry(&self.reg_path);
        self.reg.map = self.folder.clone();
        self.reg.global = file.to_string();
        self.sel = self.reg.lines.first().map(|l| l.id);
        (self.dir, self.sel_stop, self.dirty, self.delete_armed, self.drag) = (0, None, false, false, None);
        self.shapes_for = None;
        self.router = None;
        self.stops.clear();
        self.names = omsi_timetable::TimetableData::load(&self.map_dir).bus_stops.into_iter().filter(|b| !b.name.trim().is_empty()).map(|b| (b.object_id, b.name.trim().to_string())).collect();
        let chrono = omsi_map::date_code(date).map(|c| omsi_map::active_chrono_dirs(&self.map_dir, c)).unwrap_or_default();
        let ai = omsi_map::ailists::ailists_with_chrono(&self.map_dir, &chrono);
        self.groups = ai.groups.iter().filter(|g| g.is_depot && !g.typgroups.is_empty() && !g.name.trim().is_empty()).map(|g| (g.name.trim().to_string(), g.hof.clone().unwrap_or_default())).collect();
        self.depots.clear();
        self.revision += 1;
    }

    /// The depot file a depot group names (`ailists.cfg`).
    fn hof_of(&self, group: &str) -> String {
        self.groups.iter().find(|g| g.0.eq_ignore_ascii_case(group)).map(|g| g.1.trim().to_string()).unwrap_or_default()
    }

    /// A depot group's depot file as the line editor sees it - without the block it wrote,
    /// so that a destination it added is still a new one (read once).
    fn depot_of(&mut self, group: &str) -> Option<Arc<linehof::Depot>> {
        let hof = self.hof_of(group);
        if hof.is_empty() {
            return None;
        }
        let folder = self.folder.clone();
        self.depots
            .entry(hof.to_lowercase())
            .or_insert_with(|| {
                let (bases, _) = core::depot_roots();
                linehof::first_depot(&hof, &bases, &folder).map(Arc::new)
            })
            .clone()
    }

    /// The termini of a depot group's depot file: (what the trip names, what the display says).
    fn termini_of(&mut self, group: &str) -> Vec<(String, String)> {
        let Some(d) = self.depot_of(group) else { return Vec::new() };
        d.hof
            .termini
            .iter()
            .filter(|t| !t.all_exit && !t.texture_id.trim().is_empty())
            .map(|t| (t.texture_id.trim().to_string(), t.strings.first().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| t.texture_id.trim().to_string())))
            .collect()
    }

    /// Before the registry is saved: the codes of the lines in their depot files given, and
    /// what is to be written there (`core::linehof::prepare`).
    fn prepare_depots(&mut self) -> (HashMap<String, String>, linehof::Plan, Option<PathBuf>) {
        let groups = linehof::group_depots(&self.map_dir);
        let (bases, original) = core::depot_roots();
        let plan = linehof::prepare(&mut self.reg, &groups, &bases);
        (groups, plan, original)
    }

    /// The depot files written (after the registry and the timetable): the destinations,
    /// routes and stops of the lines for the displays and the IBIS. The first error.
    fn write_depots(&mut self, content: &Path, prepared: &(HashMap<String, String>, linehof::Plan, Option<PathBuf>)) -> Option<String> {
        let (groups, plan, original) = prepared;
        let (n, err) = linehof::write(content, original.as_deref(), &self.reg, groups, plan);
        log::info!("line editor: {n} depot file(s) written for the displays and the IBIS");
        self.depots.clear();
        err
    }
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let maps: Vec<(String, String)> = l.state.maps.iter().map(|m| (m.friendly.clone(), m.file.clone())).collect();
    if maps.is_empty() {
        l.ui.paragraph("No maps found.", Vec2::new(area.x, area.y), area.w, 14.0, Weight::Regular, TEXT_DIM);
        return;
    }
    {
        let v = &mut l.pages.lines;
        if v.loaded.is_none() {
            v.map = maps.iter().position(|m| m.1 == l.state.choice.map).unwrap_or(0);
        }
        v.map = v.map.min(maps.len() - 1);
        if v.loaded.as_deref() != Some(maps[v.map].1.as_str()) {
            let root = PathBuf::from(&l.state.config.root);
            v.load_map(&maps[v.map].1.clone(), &root, &l.state.choice.date);
        }
    }
    // (a phone, or a window too narrow for the map and both panels: the lines only)
    if area.w < 900.0 {
        narrow(l, area);
        return;
    }
    let side = (area.w * 0.25).clamp(280.0, 340.0);
    let left = Rect::new(area.x, area.y, side, area.h);
    let right = Rect::new(area.right() - side, area.y, side, area.h);
    let map_r = Rect::new(left.right() + GAP, area.y, right.x - left.right() - 2.0 * GAP, area.h);
    // the map: the chosen one, nothing of a duty on it
    let file = maps[l.pages.lines.map].1.clone();
    let look = Look { map: file.clone(), global: l.pages.lines.global.clone(), date: l.state.choice.date.clone(), trips: Vec::new(), entry: -1 };
    l.mapview.want(look);
    l.map_background(map_r);
    // (the sheet under the page counts as interface everywhere: only what lies over the map
    // - the panels' lists, its buttons - keeps the mouse from it)
    l.ui.over_ui = false;
    take_network(l);
    let mut status: Option<(String, bool)> = None;
    left_panel(l, left, &maps, &mut status);
    right_panel(l, right, &mut status);
    map_layer(l, map_r);
    map_buttons(l, map_r);
    map_pointer(l, map_r);
    l.ui.over_ui = true;
    if let Some((s, e)) = status.filter(|s| !s.0.is_empty()) {
        l.state.set_status(s, e);
    }
    if std::mem::take(&mut l.pages.lines.go_drive) {
        l.go(super::Page::Drive);
        l.drive.step = super::flow::Step::Start;
    }
}

/// The map's network once it is read: the router over it and the stops with their names; the
/// shown line's legs as drawn.
fn take_network(l: &mut Launcher) {
    let Some((net, stops, left_hand)) = l.mapview.network() else { return };
    let v = &mut l.pages.lines;
    let key = Arc::as_ptr(&net) as usize;
    if v.router.as_ref().map(|r| r.0) != Some(key) {
        let router = Router::new(net, left_hand);
        log::info!("line editor: {} stops on the map, {} loose lane ends joined", stops.len(), router.joined());
        v.stops = stops
            .iter()
            .map(|s| {
                let name = v.names.get(&s.id).cloned().filter(|n| !n.is_empty()).or_else(|| Some(s.name.clone()).filter(|n| !n.is_empty())).unwrap_or_else(|| format!("Stop {}", s.id));
                StopRef { tile: [s.tile.0, s.tile.1], id: s.id, name, at: [s.at.x, s.at.y], ..Default::default() }
            })
            .collect();
        v.router = Some((key, router));
        v.shapes_for = None;
    }
    if v.shapes_for != v.sel {
        v.shapes_for = v.sel;
        v.shapes = Default::default();
        if let (Some((_, router)), Some(line)) = (v.router.as_ref(), v.sel.and_then(|id| v.reg.line(id))) {
            for (k, d) in line.directions.iter().enumerate().take(2) {
                v.shapes[k] = stored_shapes(router, d);
            }
        }
        v.revision += 1;
    }
}

// --- the left panel: the lines and the line's settings ---------------------------------------

fn left_panel(l: &mut Launcher, r: Rect, maps: &[(String, String)], status: &mut Option<(String, bool)>) {
    let Launcher { ui, pages, state, .. } = l;
    let v = &mut pages.lines;
    ui.card(r);
    let inner = Rect::new(r.x + 16.0, r.y + 12.0, r.w - 32.0, r.h - 24.0);
    let body = ui.heading(inner, "Your lines", Some("route"));
    let names: Vec<String> = maps.iter().map(|m| m.0.clone()).collect();
    let mut m = v.map;
    if ui.select("le-map", Rect::new(body.x, body.y, body.w, ROW), &mut m, &names) && m != v.map {
        if v.dirty {
            *status = Some(("The changes to the line on the map before were not saved".into(), true));
        }
        v.map = m;
        v.loaded = None;
        return;
    }
    // the lines of the map
    let list_y = body.y + ROW + 10.0;
    let rows: Vec<(u64, String, String, Color)> = v.reg.lines.iter().map(|x| (x.id, x.number.clone(), x.name.clone(), colour_of(&x.colour))).collect();
    let list_h = ((rows.len().max(1) as f32) * 40.0).min(r.h * 0.28);
    let sel = v.sel;
    let mut pick = None;
    ui.scroll_area("le-lines", Rect::new(body.x - 6.0, list_y, body.w + 12.0, list_h), &mut |ui, a| {
        for (i, (id, number, name, c)) in rows.iter().enumerate() {
            let rr = Rect::new(a.x + 6.0, a.y + i as f32 * 40.0, a.w - 12.0, 36.0);
            if ui.row(&format!("le-line-{id}"), rr, Some(*id) == sel) {
                pick = Some(*id);
            }
            let plate = Rect::new(rr.x + 8.0, rr.y + 7.0, (ui.width(number, 13.0, Weight::Black) + 16.0).max(40.0), 22.0);
            ui.p().rounded(plate, 5.0, LINE);
            ui.text_in(number, plate, 13.0, Weight::Black, ON_LINE, Align::Center);
            ui.p().rounded(Rect::new(plate.right() + 8.0, rr.y + 12.0, 4.0, 12.0), 2.0, *c);
            ui.text_in(name, Rect::new(plate.right() + 20.0, rr.y, rr.right() - plate.right() - 24.0, rr.h), 13.0, Weight::Medium, TEXT, Align::Left);
        }
        rows.len() as f32 * 40.0
    });
    if rows.is_empty() {
        ui.text_in("No lines of your own on this map yet", Rect::new(body.x, list_y, body.w, 36.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
    }
    if let Some(id) = pick {
        if v.sel != Some(id) {
            (v.sel, v.dir, v.sel_stop, v.delete_armed) = (Some(id), 0, None, false);
        }
    }
    let mut y = list_y + list_h + 8.0;
    if ui.button("le-new", Rect::new(body.x, y, body.w, ROW), "New line", Some("add"), ButtonKind::Normal) {
        let group = v.groups.first().map(|g| g.0.clone()).unwrap_or_default();
        let n = v.reg.lines.len();
        let line = v.reg.add_line(&group);
        line.colour = PALETTE[n % PALETTE.len()].to_string();
        line.name = format!("{} {}", omsi_ui::tr("Line"), line.number);
        let id = line.id;
        (v.sel, v.dir, v.sel_stop) = (Some(id), 0, None);
        v.touched();
        *status = Some(("Now click the line's stops on the map, in the order the bus calls at them".into(), false));
    }
    y += ROW + 14.0;
    if v.line().is_none() {
        return;
    }
    // the line's own settings
    ui.p().rect(Rect::new(body.x, y - 7.0, body.w, 1.0), HAIRLINE);
    ui.text_in(&omsi_ui::tr("Number").to_uppercase(), Rect::new(body.x, y, 90.0, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    ui.text_in(&omsi_ui::tr("Name").to_uppercase(), Rect::new(body.x + 100.0, y, 120.0, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    y += 18.0;
    let groups = v.groups.clone();
    let mut changed = false;
    {
        let line = v.line_mut().unwrap();
        changed |= ui.text_input("le-number", Rect::new(body.x, y, 90.0, ROW), &mut line.number, "42", None);
        changed |= ui.text_input("le-name", Rect::new(body.x + 100.0, y, body.w - 100.0, ROW), &mut line.name, "Name", None);
    }
    y += ROW + 12.0;
    // its colour
    let current = v.line().map(|x| x.colour.clone()).unwrap_or_default();
    let sw = ((body.w - 7.0 * 8.0) / 8.0).min(30.0);
    for (k, hex) in PALETTE.iter().enumerate() {
        let c = Rect::new(body.x + k as f32 * (sw + 8.0), y, sw, sw);
        let on = current.eq_ignore_ascii_case(hex);
        let hover = ui.hover(c);
        ui.p().rounded(c, sw * 0.5, colour_of(hex));
        if on || hover {
            ui.p().rounded_border(c.inset(-3.0), sw * 0.5 + 3.0, 2.0, if on { TEXT } else { TEXT_DIM });
        }
        let (_, _, clicked) = ui.interact(super::ui::id_of(&format!("le-colour-{k}")), c);
        if clicked && !on {
            v.line_mut().unwrap().colour = hex.to_string();
            changed = true;
        }
    }
    y += sw + 16.0;
    // the depot whose buses drive it
    ui.text_in(&omsi_ui::tr("Driven by the buses of").to_uppercase(), Rect::new(body.x, y, body.w, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    y += 18.0;
    if groups.is_empty() {
        let line = v.line_mut().unwrap();
        changed |= ui.text_input("le-group", Rect::new(body.x, y, body.w, ROW), &mut line.ai_group, "Depot group (ailists.cfg)", None);
        y += ROW + 4.0;
        ui.paragraph("This map's ailists.cfg has no depot group with buses: the timetable's buses cannot drive the line (you can).", Vec2::new(body.x, y), body.w, 11.5, Weight::Regular, WARN);
    } else {
        let names: Vec<String> = groups.iter().map(|g| g.0.clone()).collect();
        let line = v.line_mut().unwrap();
        let mut gi = names.iter().position(|n| n.eq_ignore_ascii_case(&line.ai_group)).unwrap_or(0);
        if line.ai_group.trim().is_empty() || !names.iter().any(|n| n.eq_ignore_ascii_case(&line.ai_group)) {
            line.ai_group = names[gi].clone();
            changed = true;
        }
        if ui.select("le-group", Rect::new(body.x, y, body.w, ROW), &mut gi, &names) {
            line.ai_group = names[gi].clone();
            changed = true;
        }
    }
    if changed {
        v.touched();
    }
    // at the foot: save, and delete (two presses)
    let foot = r.bottom() - 12.0 - ROW;
    let problems = v.line().map(reg::problems).unwrap_or_default();
    if let Some(p) = problems.first() {
        let p = omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string());
        let h = ui.paragraph_height(&p, body.w, 11.5, Weight::Regular);
        ui.paragraph(&p, Vec2::new(body.x, foot - ROW - 14.0 - h), body.w, 11.5, Weight::Regular, WARN);
    }
    let half = (body.w - GAP) * 0.5;
    let label = if v.dirty { "Save line" } else { "Saved" };
    if ui.button("le-save", Rect::new(body.x, foot - ROW - 8.0, body.w, ROW), label, Some("save"), if v.dirty { ButtonKind::Primary } else { ButtonKind::Normal }) && v.dirty {
        *status = Some(save(v, state, &problems));
    }
    if ui.button("le-delete", Rect::new(body.x, foot, half, ROW), if v.delete_armed { "Press again" } else { "Delete line" }, Some("delete"), ButtonKind::Normal) {
        if v.delete_armed {
            *status = Some(delete(v, state));
        } else {
            v.delete_armed = true;
            *status = Some(("Press \"Delete line\" again: the line and its files go".into(), false));
        }
    }
    if ui.button("le-drive", Rect::new(body.x + half + GAP, foot, half, ROW), "Drive it", Some("play_arrow"), ButtonKind::Normal) {
        *status = Some(drive_it(v, state));
    }
}

/// The registry written, and every line of it that can be into the map's timetable.
fn save(v: &mut LineEditorView, state: &mut super::state::State, problems: &[reg::Problem]) -> (String, bool) {
    if let Some(line) = v.line_mut() {
        line.modified = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    }
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("Not saved: {e}"), true);
    }
    v.dirty = false;
    let Some(content) = core::content_dir() else { return ("Saved in your lines, but there is no content folder to write the timetable into".into(), true) };
    match reg::export_to_map(&content, &v.map_dir, &v.reg) {
        Ok(_) => {
            let depot_error = v.write_depots(&content, &prepared);
            omsi_cfg::content_changed();
            state.load_lines();
            let number = v.line().map(|x| x.number.clone()).unwrap_or_default();
            if let Some(e) = depot_error {
                (format!("{}: {e}", omsi_ui::tr("The line is saved, but a depot file could not be written (displays and IBIS)")), true)
            } else if let Some(p) = problems.first() {
                (omsi_ui::tr("Line %{n} kept as a draft: %{why}").replace("%{n}", &number).replace("%{why}", &omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string())), false)
            } else {
                (omsi_ui::tr("Line %{n} saved: it is in the map's timetable now (the next game start picks it up)").replace("%{n}", &number), false)
            }
        }
        Err(e) => (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true),
    }
}

/// The shown line out of the registry and its files out of the map's timetable.
fn delete(v: &mut LineEditorView, state: &mut super::state::State) -> (String, bool) {
    let Some(id) = v.sel else { return (String::new(), false) };
    let number = v.line().map(|x| x.number.clone()).unwrap_or_default();
    v.reg.lines.retain(|x| x.id != id);
    v.sel = v.reg.lines.first().map(|x| x.id);
    (v.dir, v.sel_stop, v.delete_armed, v.dirty) = (0, None, false, false);
    v.shapes_for = None;
    v.revision += 1;
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("Not deleted: {e}"), true);
    }
    if let Some(content) = core::content_dir() {
        if let Err(e) = reg::export_to_map(&content, &v.map_dir, &v.reg) {
            return (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true);
        }
        if let Some(e) = v.write_depots(&content, &prepared) {
            return (format!("{}: {e}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true);
        }
        omsi_cfg::content_changed();
        state.load_lines();
    }
    (omsi_ui::tr("Line %{n} deleted").replace("%{n}", &number), false)
}

/// The Drive page set to drive the shown line: a free drive along its first direction on its
/// map (it must be saved: the game reads the files).
fn drive_it(v: &mut LineEditorView, state: &mut super::state::State) -> (String, bool) {
    let Some(line) = v.line() else { return (String::new(), false) };
    if v.dirty || !reg::problems(line).is_empty() {
        return ("Save the line first: the game drives what is saved".into(), true);
    }
    let stem = reg::stems(&v.reg).get(&line.id).cloned().unwrap_or_default();
    let file = v.reg.global.clone();
    let c = &mut state.choice;
    if c.map != file {
        c.map = file;
        c.entry = -1;
    }
    (c.free, c.composed, c.own_line) = (true, false, true);
    // (the line lists open on the player's own lines)
    c.my_lines = true;
    c.free_line = stem.clone();
    c.free_route = reg::trip_name(&stem, 0);
    state.touched();
    state.load_lines();
    v.go_drive = true;
    (String::new(), false)
}

// --- the right panel: a direction's stops, and the timetable ----------------------------------

fn right_panel(l: &mut Launcher, r: Rect, status: &mut Option<(String, bool)>) {
    let Launcher { ui, pages, .. } = l;
    let v = &mut pages.lines;
    ui.card(r);
    let inner = Rect::new(r.x + 16.0, r.y + 12.0, r.w - 32.0, r.h - 24.0);
    let Some(line) = v.line() else {
        ui.heading(inner, "The line", Some("route"));
        ui.paragraph("Choose one of your lines, or make a new one.", Vec2::new(inner.x, inner.y + 30.0), inner.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    };
    let group = line.ai_group.clone();
    let two = line.directions.len() > 1;
    let mut tab = v.tab;
    if ui.segmented("le-tab", Rect::new(inner.x, inner.y, inner.w, ROW), &mut tab, &["Stops", "Displays", "Timetable"]) {
        v.tab = tab;
    }
    let body = Rect::new(inner.x, inner.y + ROW + 12.0, inner.w, inner.h - ROW - 12.0);
    if v.tab == 2 {
        timetable_tab(ui, v, body);
        return;
    }
    if v.tab == 1 {
        displays_tab(ui, v, body);
        return;
    }
    let mut dir = v.dir;
    if ui.segmented("le-dir", Rect::new(body.x, body.y, body.w, ROW), &mut dir, &["Outbound", "Return"]) {
        (v.dir, v.sel_stop) = (dir, None);
        v.revision += 1;
    }
    let mut y = body.y + ROW + 12.0;
    if v.dir == 1 && !two {
        ui.paragraph("Without a way back the line goes round: the bus starts again at the first stop.", Vec2::new(body.x, y), body.w, 12.5, Weight::Regular, TEXT_DIM);
        y += 50.0;
        if ui.button("le-reverse-new", Rect::new(body.x, y, body.w, ROW), "Reverse: make the way back", Some("sync_alt"), ButtonKind::Primary) {
            reverse(v, status);
        }
        return;
    }
    // where it goes: a terminus of the depot's file, or what is typed
    let termini = v.termini_of(&group);
    let d_idx = v.dir;
    let mut changed = false;
    {
        let line = v.line_mut().unwrap();
        let d = &mut line.directions[d_idx];
        ui.text_in(&omsi_ui::tr("Destination").to_uppercase(), Rect::new(body.x, y, body.w, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
        y += 18.0;
        let last = d.stops.last().map(|s| s.name.clone()).unwrap_or_default();
        let mut options = vec![omsi_ui::tr("Last stop: %{name}").replace("%{name}", &last)];
        options.extend(termini.iter().map(|t| t.1.clone()));
        let mut ti = termini.iter().position(|t| t.0.eq_ignore_ascii_case(d.terminus.trim())).map(|i| i + 1).unwrap_or(0);
        let half = (body.w - GAP) * 0.5;
        if ui.select("le-terminus", Rect::new(body.x, y, half, ROW), &mut ti, &options) {
            d.terminus = if ti == 0 { String::new() } else { termini[ti - 1].0.clone() };
            changed = true;
        }
        changed |= ui.text_input("le-terminus-text", Rect::new(body.x + half + GAP, y, half, ROW), &mut d.terminus, "or type it", None);
    }
    y += ROW + 12.0;
    // the stops, the way to each between them
    let foot_h = 2.0 * ROW + 16.0;
    let list = Rect::new(body.x - 6.0, y, body.w + 12.0, body.bottom() - y - foot_h);
    let (rows, legs, times, total) = {
        let d = &v.line().unwrap().directions[d_idx];
        (d.stops.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), d.legs.iter().map(|g| (g.ok, g.length, g.vias.len())).collect::<Vec<_>>(), d.times.clone(), d.minutes())
    };
    let sel_stop = v.sel_stop;
    let colour = colour_of(&v.line().unwrap().colour);
    enum Act {
        Pick(usize),
        Up(usize),
        Down(usize),
        Remove(usize),
        Shift(usize, f32),
        ClearVias(usize),
    }
    let mut act = None;
    let rh = 34.0;
    let lh = 20.0;
    ui.scroll_area(&format!("le-stops-{d_idx}"), list, &mut |ui, a| {
        let mut yy = a.y;
        if rows.is_empty() {
            ui.paragraph("Click the stops on the map in the order the bus calls at them.", Vec2::new(a.x + 6.0, yy + 4.0), a.w - 12.0, 12.5, Weight::Regular, TEXT_DIM);
            return 60.0;
        }
        for (i, name) in rows.iter().enumerate() {
            let rr = Rect::new(a.x + 6.0, yy, a.w - 12.0, rh);
            if ui.row(&format!("le-stop-{d_idx}-{i}"), rr, sel_stop == Some(i)) {
                act = Some(Act::Pick(i));
            }
            let dot = Vec2::new(rr.x + 14.0, rr.center().y);
            ui.p().circle(dot, if i == 0 || i + 1 == rows.len() { 6.0 } else { 4.5 }, colour);
            ui.text_in(name, Rect::new(rr.x + 28.0, rr.y, rr.w - 196.0, rr.h), 12.5, Weight::Medium, TEXT, Align::Left);
            let t = times.get(i).copied().unwrap_or(0.0);
            ui.text_in(&format!("{t:.0}'"), Rect::new(rr.right() - 164.0, rr.y, 34.0, rr.h), 12.0, Weight::Bold, TEXT_SOFT, Align::Right);
            let cy = rr.center().y;
            if i > 0 {
                if ui.icon_button(&format!("le-later-{d_idx}-{i}"), Vec2::new(rr.right() - 88.0, cy), 10.0, "add", "A minute later") {
                    act = Some(Act::Shift(i, 1.0));
                }
                if ui.icon_button(&format!("le-earlier-{d_idx}-{i}"), Vec2::new(rr.right() - 112.0, cy), 10.0, "remove", "A minute earlier") {
                    act = Some(Act::Shift(i, -1.0));
                }
            }
            if sel_stop == Some(i) {
                if i > 0 && ui.icon_button(&format!("le-up-{d_idx}-{i}"), Vec2::new(rr.right() - 54.0, cy), 10.0, "keyboard_arrow_up", "Earlier in the line") {
                    act = Some(Act::Up(i));
                }
                if i + 1 < rows.len() && ui.icon_button(&format!("le-down-{d_idx}-{i}"), Vec2::new(rr.right() - 32.0, cy), 10.0, "keyboard_arrow_down", "Later in the line") {
                    act = Some(Act::Down(i));
                }
            }
            if ui.icon_button(&format!("le-x-{d_idx}-{i}"), Vec2::new(rr.right() - 10.0, cy), 10.0, "close", "Take the stop out") {
                act = Some(Act::Remove(i));
            }
            yy += rh;
            if let Some((ok, len, vias)) = legs.get(i).copied() {
                let lr = Rect::new(a.x + 34.0, yy, a.w - 40.0, lh);
                ui.p().rect(Rect::new(a.x + 19.5, yy - 4.0, 1.5, lh + 8.0), colour.alpha(0.5));
                let text = if ok { format!("{:.0} m", len) } else { omsi_ui::tr("no way over the roads").to_string() };
                ui.text_in(&text, lr, 11.0, Weight::Regular, if ok { TEXT_DIM } else { DANGER }, Align::Left);
                if vias > 0 && ui.icon_button(&format!("le-novia-{d_idx}-{i}"), Vec2::new(lr.right() - 10.0, lr.center().y), 9.0, "restart_alt", "Straighten the leg (its dragged points go)") {
                    act = Some(Act::ClearVias(i));
                }
                yy += lh;
            }
        }
        yy - a.y
    });
    let mut route = false;
    match act {
        Some(Act::Pick(i)) => v.sel_stop = if v.sel_stop == Some(i) { None } else { Some(i) },
        Some(Act::Up(i)) | Some(Act::Down(i)) => {
            let j = if matches!(act, Some(Act::Up(_))) { i - 1 } else { i + 1 };
            let d = &mut v.line_mut().unwrap().directions[d_idx];
            d.stops.swap(i, j);
            for g in &mut d.legs {
                g.vias.clear();
            }
            v.sel_stop = Some(j);
            route = true;
        }
        Some(Act::Remove(i)) => {
            let d = &mut v.line_mut().unwrap().directions[d_idx];
            d.stops.remove(i);
            // (the legs either side become one, without their dragged points)
            if i < d.legs.len() {
                d.legs.remove(i);
            } else if !d.legs.is_empty() {
                d.legs.pop();
            }
            if i > 0 && i - 1 < d.legs.len() {
                d.legs[i - 1].vias.clear();
            }
            v.sel_stop = None;
            route = true;
        }
        Some(Act::Shift(i, by)) => {
            v.line_mut().unwrap().directions[d_idx].shift_from(i, by);
            changed = true;
        }
        Some(Act::ClearVias(i)) => {
            v.line_mut().unwrap().directions[d_idx].legs[i].vias.clear();
            route = true;
        }
        None => {}
    }
    // the foot: the whole trip's minutes, the times from the roads again, the way back
    let fy = body.bottom() - foot_h + 8.0;
    ui.text_in(&omsi_ui::tr("%{n} min from the first stop to the last").replace("%{n}", &format!("{total:.0}")), Rect::new(body.x, fy, body.w - 70.0, ROW), 12.5, Weight::Medium, TEXT_SOFT, Align::Left);
    if ui.icon_button("le-total-less", Vec2::new(body.right() - 52.0, fy + ROW * 0.5), 12.0, "remove", "The whole trip a minute shorter") && total > 1.0 {
        v.line_mut().unwrap().directions[d_idx].set_total(total - 1.0);
        changed = true;
    }
    if ui.icon_button("le-total-more", Vec2::new(body.right() - 14.0, fy + ROW * 0.5), 12.0, "add", "The whole trip a minute longer") {
        v.line_mut().unwrap().directions[d_idx].set_total(total + 1.0);
        changed = true;
    }
    let half = (body.w - GAP) * 0.5;
    let by = fy + ROW + 8.0;
    if ui.button("le-times-auto", Rect::new(body.x, by, half, ROW), "Times from the roads", Some("schedule"), ButtonKind::Normal) {
        let d = &mut v.line_mut().unwrap().directions[d_idx];
        d.manual_times = false;
        d.refresh_times();
        changed = true;
    }
    if ui.button("le-reverse", Rect::new(body.x + half + GAP, by, half, ROW), if two { "Reverse again" } else { "Reverse" }, Some("sync_alt"), ButtonKind::Normal) {
        reverse(v, status);
    }
    if route {
        v.reroute(d_idx);
    } else if changed {
        v.touched();
    }
}

/// The way back made anew: the outbound stops from the last to the first, each the stop of
/// the same name across the road, routed.
fn reverse(v: &mut LineEditorView, status: &mut Option<(String, bool)>) {
    let all = v.stops.clone();
    let Some(line) = v.line_mut() else { return };
    if line.directions[0].stops.len() < 2 {
        *status = Some(("Give the outbound direction its stops first".into(), true));
        return;
    }
    // (the way back keeps the route code it had on the IBIS)
    let ibis_route = line.directions.get(1).map(|d| d.ibis_route).unwrap_or(0);
    let back = Direction { stops: reg::opposite_stops(&line.directions[0].stops, &all, ACROSS), ibis_route, ..Default::default() };
    line.directions.truncate(1);
    line.directions.push(back);
    (v.dir, v.sel_stop) = (1, None);
    v.reroute(1);
    let bad = v.line().map(|x| x.directions[1].legs.iter().filter(|g| !g.ok).count()).unwrap_or(0);
    *status = Some(if bad > 0 {
        (omsi_ui::tr("The way back is made; %{n} leg(s) have no way over the roads (red): drag them, or change a stop").replace("%{n}", &bad.to_string()), true)
    } else {
        ("The way back is made from the stops across the road".into(), false)
    });
}

/// A destination matrix as a bus shows it: the line number on the left, the destination in one
/// or two lines beside it, lit dots on black (amber, or white).
fn led_sign(ui: &mut Ui, r: Rect, line: &str, l1: &str, l2: &str, white: bool) {
    let lit = if white { Color::rgba(236, 242, 255, 1.0) } else { Color::rgba(255, 172, 28, 1.0) };
    let black = Color::rgba(12, 12, 14, 1.0);
    ui.p().rounded(r.inset(-3.0), 9.0, Color::rgba(48, 52, 60, 1.0));
    ui.p().rounded(r, 6.0, black);
    let inner = r.inset(7.0);
    // (a text too wide for its place gets smaller, as the matrix's narrower font)
    let fit = |ui: &Ui, s: &str, px: f32, w: f32| {
        let width = ui.width(s, px, Weight::Bold);
        if width > w { (px * w / width).max(7.0) } else { px }
    };
    let line = line.trim();
    let nw = if line.is_empty() { 0.0 } else { (ui.width(line, 30.0, Weight::Black) + 10.0).max(inner.h * 0.9) };
    if !line.is_empty() {
        ui.text_in(line, Rect::new(inner.x, inner.y, nw, inner.h), 30.0, Weight::Black, lit, Align::Center);
    }
    let dr = Rect::new(inner.x + nw + 6.0, inner.y, inner.w - nw - 6.0, inner.h);
    if l2.is_empty() {
        let px = fit(ui, l1, 21.0, dr.w);
        ui.text_in(l1, dr, px, Weight::Bold, lit, Align::Center);
    } else {
        let half = dr.h * 0.5;
        let px = fit(ui, l1, 15.0, dr.w).min(fit(ui, l2, 15.0, dr.w));
        ui.text_in(l1, Rect::new(dr.x, dr.y, dr.w, half), px, Weight::Bold, lit, Align::Center);
        ui.text_in(l2, Rect::new(dr.x, dr.y + half, dr.w, half), px, Weight::Bold, lit, Align::Center);
    }
    // the dots: a dark grid over what is lit
    let dark = black.alpha(0.62);
    let step = 2.6;
    let mut x = inner.x;
    while x < inner.right() {
        ui.p().rect(Rect::new(x, inner.y, 1.0, inner.h), dark);
        x += step;
    }
    let mut y = inner.y;
    while y < inner.bottom() {
        ui.p().rect(Rect::new(inner.x, y, inner.w, 1.0), dark);
        y += step;
    }
}

/// What the buses show of a direction, and what the IBIS knows of it (`core::linehof`): the
/// sign as it lights up; a destination the depot file lacks with its texts, one per string of
/// the file (each empty one made from the destination, as the file writes its own); the IBIS
/// route code; what the IBIS calls each stop. All of it written into the depot files when the
/// line is saved.
fn displays_tab(ui: &mut Ui, v: &mut LineEditorView, body: Rect) {
    let (group, two, number) = {
        let l = v.line().unwrap();
        (l.ai_group.clone(), l.directions.len() > 1, l.number.trim().to_string())
    };
    let mut dir = v.dir;
    if ui.segmented("le-dir-d", Rect::new(body.x, body.y, body.w, ROW), &mut dir, &["Outbound", "Return"]) {
        (v.dir, v.sel_stop) = (dir, None);
        v.revision += 1;
    }
    let mut y = body.y + ROW + 12.0;
    if v.dir == 1 && !two {
        ui.paragraph("Without a way back the line goes round: the bus starts again at the first stop.", Vec2::new(body.x, y), body.w, 12.5, Weight::Regular, TEXT_DIM);
        return;
    }
    let d_idx = v.dir;
    let Some(depot) = v.depot_of(&group) else {
        let hof = v.hof_of(&group);
        let text = if hof.is_empty() { omsi_ui::tr("The depot group has no depot file: the buses' displays and IBIS cannot know the line.").to_string() } else { omsi_ui::tr("The depot file %{hof} was not found beside any bus: the displays and the IBIS cannot know the line.").replace("%{hof}", &hof) };
        ui.paragraph(&text, Vec2::new(body.x, y), body.w, 12.5, Weight::Regular, WARN);
        return;
    };
    let d = v.line().unwrap().directions[d_idx].clone();
    let dest = d.destination();
    if d.stops.len() < 2 || dest.is_empty() {
        ui.paragraph("Give the direction its stops first: the destination is its last one, or the one chosen with the stops.", Vec2::new(body.x, y), body.w, 12.5, Weight::Regular, TEXT_DIM);
        return;
    }
    let (strings, new) = depot.sign_of(&d);
    let (l1, l2) = depot.front(&strings);
    // the sign (a click lights it the other colour)
    let sign = Rect::new(body.x + 3.0, y + 3.0, body.w - 6.0, 58.0);
    led_sign(ui, sign, &number, &l1, &l2, v.white);
    if ui.interact(super::ui::id_of("le-sign"), sign).2 {
        v.white = !v.white;
    }
    y += 70.0;
    let what = if new { omsi_ui::tr("New destination: %{name}") } else { omsi_ui::tr("%{name}, from the depot file") };
    ui.text_in(&what.replace("%{name}", &dest), Rect::new(body.x, y, body.w - 28.0, 20.0), 12.0, Weight::Medium, if new { accent_2() } else { TEXT_SOFT }, Align::Left);
    let mut changed = false;
    let mut sign_vals = d.sign.clone();
    if new && !d.sign.iter().all(|s| s.is_empty()) && ui.icon_button("le-sign-reset", Vec2::new(body.right() - 10.0, y + 10.0), 10.0, "restart_alt", "The texts made from the destination again") {
        sign_vals.clear();
        changed = true;
    }
    y += 26.0;
    // the rest scrolls: the texts, the IBIS route, the stops' names
    let defaults = if new { depot.sign_defaults(&dest) } else { Vec::new() };
    sign_vals.resize(defaults.len(), String::new());
    let labels: Vec<String> = depot
        .labels()
        .iter()
        .enumerate()
        .map(|(k, (role, note))| if note.is_empty() { omsi_ui::tr(role.label()).replace("%{n}", &(k + 1).to_string()) } else { note.chars().take(48).collect() })
        .collect();
    let line_no = linehof::line_number(&number);
    let mut route = if d.ibis_route > 0 { d.ibis_route.to_string() } else { String::new() };
    let route_hint = (line_no * 100 + d_idx as u32 + 1).to_string();
    let taken: std::collections::HashSet<u32> = depot.hof.info_trips.iter().filter_map(|t| t.code.trim().parse().ok()).collect();
    let stops: Vec<(String, String)> = d.stops.iter().map(|s| (s.name.clone(), depot.stop_display(&s.name))).collect();
    let mut stop_vals: Vec<String> = d.stops.iter().map(|s| s.ibis.clone()).collect();
    let area = Rect::new(body.x - 6.0, y, body.w + 12.0, body.bottom() - y);
    let head = |ui: &mut Ui, text: &str, r: Rect| {
        ui.text_in(&omsi_ui::tr(text).to_uppercase(), r, 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    };
    let mut route_changed = false;
    let mut texts_changed = false;
    let mut stops_changed = false;
    ui.scroll_area(&format!("le-displays-{d_idx}"), area, &mut |ui, a| {
        let x = a.x + 6.0;
        let w = a.w - 12.0;
        let mut yy = a.y + 2.0;
        if new {
            head(ui, "Display texts", Rect::new(x, yy, w, 16.0));
            yy += 20.0;
            for (k, def) in defaults.iter().enumerate() {
                ui.text_in(&labels.get(k).cloned().unwrap_or_default(), Rect::new(x, yy, w, 16.0), 11.0, Weight::Medium, TEXT_SOFT, Align::Left);
                yy += 17.0;
                // (empty: the default, shown greyed)
                texts_changed |= ui.text_input(&format!("le-sign-{d_idx}-{k}"), Rect::new(x, yy, w, ROW - 4.0), &mut sign_vals[k], if def.trim().is_empty() { "-" } else { def.as_str() }, None);
                yy += ROW + 2.0;
            }
            yy += 6.0;
        }
        head(ui, "IBIS route", Rect::new(x, yy, w, 16.0));
        yy += 20.0;
        route_changed |= ui.text_input(&format!("le-route-{d_idx}"), Rect::new(x, yy, 110.0, ROW - 4.0), &mut route, &route_hint, None);
        let code: u32 = route.trim().parse().unwrap_or(0);
        let (hint, colour) = if route.trim().is_empty() {
            (omsi_ui::tr("Given when the line is saved").to_string(), TEXT_DIM)
        } else if code / 100 != line_no || code % 100 == 0 {
            (omsi_ui::tr("Not a route of line %{n} (%{n}01 - %{n}99): another is given on saving").replace("%{n}", &line_no.to_string()), WARN)
        } else if taken.contains(&code) {
            (omsi_ui::tr("The depot file has this code already: another is given on saving").to_string(), WARN)
        } else {
            (omsi_ui::tr("Line %{l}, route %{r} on the IBIS").replace("%{l}", &line_no.to_string()).replace("%{r}", &format!("{:02}", code % 100)), TEXT_DIM)
        };
        let hh = ui.paragraph(&hint, Vec2::new(x + 120.0, yy + 1.0), w - 120.0, 11.0, Weight::Regular, colour);
        yy += (ROW + 4.0).max(hh + 6.0);
        head(ui, "Stop names on the IBIS", Rect::new(x, yy, w, 16.0));
        yy += 20.0;
        for (i, (name, def)) in stops.iter().enumerate() {
            ui.text_in(name, Rect::new(x, yy, w, 16.0), 11.0, Weight::Medium, TEXT_SOFT, Align::Left);
            yy += 17.0;
            stops_changed |= ui.text_input(&format!("le-ibis-{d_idx}-{i}"), Rect::new(x, yy, w, ROW - 4.0), &mut stop_vals[i], def, None);
            yy += ROW + 2.0;
        }
        yy - a.y + 8.0
    });
    if texts_changed || changed {
        // (all empty: the defaults, which follow the destination)
        if sign_vals.iter().all(|s| s.is_empty()) {
            sign_vals.clear();
        }
        v.line_mut().unwrap().directions[d_idx].sign = sign_vals;
        changed = true;
    }
    if route_changed {
        v.line_mut().unwrap().directions[d_idx].ibis_route = route.trim().parse().unwrap_or(0);
        changed = true;
    }
    if stops_changed {
        let dd = &mut v.line_mut().unwrap().directions[d_idx];
        for (s, val) in dd.stops.iter_mut().zip(stop_vals) {
            s.ibis = val;
        }
        changed = true;
    }
    if changed {
        v.touched();
    }
}

/// When the line runs: per group of days the first and last departure, how often, and how long
/// a bus stands at the end; how many buses that takes.
fn timetable_tab(ui: &mut Ui, v: &mut LineEditorView, body: Rect) {
    let run: Vec<f32> = v.line().map(|l| l.directions.iter().filter(|d| d.stops.len() >= 2).map(|d| d.minutes().max(1.0)).collect()).unwrap_or_default();
    let mut changed = false;
    let mut y = body.y;
    let line = v.line_mut().unwrap();
    if line.days.len() < DAY_GROUPS.len() {
        line.days = reg::default_days();
    }
    for (k, (name, _)) in DAY_GROUPS.iter().enumerate() {
        let p = &mut line.days[k];
        changed |= ui.toggle(&format!("le-day-{k}"), Rect::new(body.x, y, body.w, ROW), &mut p.on, name);
        y += ROW + 4.0;
        if p.on {
            let half = (body.w - GAP) * 0.5;
            ui.text_in("First", Rect::new(body.x, y, half, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
            ui.text_in("Last", Rect::new(body.x + half + GAP, y, half, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
            y += 18.0;
            let (mut first, mut last) = (p.first.round() as i32, p.last.round() as i32);
            if ui.time_field(&format!("le-first-{k}"), Rect::new(body.x, y, half, ROW), &mut first) {
                p.first = first as f32;
                changed = true;
            }
            if ui.time_field(&format!("le-last-{k}"), Rect::new(body.x + half + GAP, y, half, ROW), &mut last) {
                p.last = last as f32;
                changed = true;
            }
            y += ROW + 6.0;
            changed |= ui.slider(&format!("le-every-{k}"), Rect::new(body.x, y, body.w, 28.0), &mut p.headway, 5.0, 120.0, 5.0, "Every", &|x| format!("{x:.0} min"));
            y += 30.0;
            changed |= ui.slider(&format!("le-layover-{k}"), Rect::new(body.x, y, body.w, 28.0), &mut p.layover, 0.0, 30.0, 1.0, "Stands", &|x| format!("{x:.0} min"));
            y += 30.0;
            let buses = if run.is_empty() { 0 } else { reg::blocks(&run, p).len() };
            let trips: usize = if run.is_empty() { 0 } else { reg::blocks(&run, p).iter().map(|b| b.len()).sum() };
            ui.text_in(&omsi_ui::tr("%{b} buses, %{t} trips").replace("%{b}", &buses.to_string()).replace("%{t}", &trips.to_string()), Rect::new(body.x, y, body.w, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            y += 26.0;
        }
        ui.p().rect(Rect::new(body.x, y, body.w, 1.0), HAIRLINE);
        y += 10.0;
    }
    if y < body.bottom() - 40.0 {
        ui.paragraph("Each group of days becomes tours of its own; the buses go back and forth, each standing at the end for at least as long as set.", Vec2::new(body.x, y), body.w, 11.5, Weight::Regular, TEXT_FAINT);
    }
    if changed {
        v.touched();
    }
}

// --- the map ----------------------------------------------------------------------------------

/// What the map shows of the line: its legs (the direction worked on bright, the other faint,
/// a leg without a way red and straight) and the stops (the map's quiet, the line's in its
/// colour, the chosen ringed), with names beside the ends and the stop under the mouse.
fn map_layer(l: &mut Launcher, map_r: Rect) {
    let v = &l.pages.lines;
    let line = v.line();
    let colour = line.map(|x| colour_of(&x.colour)).unwrap_or(accent());
    let rev = v.revision;
    let shapes = v.shapes.clone();
    let dirs = line.map(|x| x.directions.clone()).unwrap_or_default();
    let cur = v.dir;
    l.mapview.editor_lines(rev, || {
        let mut out = Vec::new();
        for (k, legs) in shapes.iter().enumerate() {
            if k == cur {
                continue;
            }
            for s in legs {
                out.push((s.clone(), colour.alpha(0.35), 4.0));
            }
        }
        if let Some(legs) = shapes.get(cur) {
            for (i, s) in legs.iter().enumerate() {
                let ok = dirs.get(cur).and_then(|d| d.legs.get(i)).map(|g| g.ok).unwrap_or(false);
                out.push((s.clone(), if ok { colour } else { DANGER }, if ok { 5.0 } else { 3.0 }));
            }
        }
        out
    });
    let mut dots = Vec::with_capacity(v.stops.len() + 32);
    for s in &v.stops {
        dots.push(Dot { at: dv(s.at), fill: Color::rgba(149, 157, 176, 0.9), r: 3.2, ring: None });
    }
    for (k, d) in dirs.iter().enumerate().filter(|(k, _)| *k != cur) {
        let _ = k;
        for s in &d.stops {
            dots.push(Dot { at: dv(s.at), fill: colour.alpha(0.5), r: 4.0, ring: None });
        }
    }
    if let Some(d) = dirs.get(cur) {
        let n = d.stops.len();
        for (i, s) in d.stops.iter().enumerate() {
            let end = i == 0 || i + 1 == n;
            dots.push(Dot { at: dv(s.at), fill: if end { TEXT } else { colour }, r: if end { 6.5 } else { 5.0 }, ring: (v.sel_stop == Some(i)).then_some(LINE) });
        }
        for g in &d.legs {
            for p in &g.vias {
                dots.push(Dot { at: dv(*p), fill: TEXT, r: 3.5, ring: Some(colour) });
            }
        }
    }
    if let Some(i) = v.hover_stop {
        if let Some(s) = v.stops.get(i) {
            dots.push(Dot { at: dv(s.at), fill: TEXT, r: 4.5, ring: Some(LINE) });
        }
    }
    // the names: the ends of the direction and the stop under the mouse
    let mut labels: Vec<(DVec2, String, bool)> = Vec::new();
    if let Some(d) = dirs.get(cur) {
        for s in [d.stops.first(), d.stops.last()].into_iter().flatten() {
            labels.push((dv(s.at), s.name.clone(), false));
        }
    }
    if let Some(s) = v.hover_stop.and_then(|i| v.stops.get(i)) {
        labels.push((dv(s.at), s.name.clone(), true));
    }
    l.mapview.editor_dots(dots);
    if l.mapview.network().is_none() {
        return;
    }
    l.ui.push_clip(map_r, RADIUS);
    for (at, name, hover) in labels {
        let p = l.mapview.project(at);
        let w = l.ui.width(&name, 12.0, Weight::Medium) + 16.0;
        let r = Rect::new(p.x + 10.0, p.y - 11.0, w, 22.0);
        l.ui.p().rounded(r, 11.0, if hover { PANEL } else { ON_MAP });
        l.ui.text_in(&name, r, 12.0, Weight::Medium, TEXT, Align::Center);
    }
    if l.pages.lines.line().is_some_and(|x| x.directions.get(cur).is_some_and(|d| d.stops.is_empty())) {
        let hint = omsi_ui::tr("Click the line's first stop").to_string();
        let w = l.ui.width(&hint, 13.0, Weight::Medium) + 28.0;
        let r = Rect::new(map_r.center().x - w * 0.5, map_r.y + 14.0, w, 30.0);
        l.ui.p().rounded(r, 15.0, ON_MAP);
        l.ui.text_in(&hint, r, 13.0, Weight::Medium, TEXT, Align::Center);
    }
    l.ui.pop_clip();
}

/// Zoom and the whole map, in the map's corner.
fn map_buttons(l: &mut Launcher, map_r: Rect) {
    let x = map_r.right() - 26.0;
    for (k, (icon, tip)) in [("add", "Zoom in"), ("remove", "Zoom out"), ("near_me", "The whole map")].iter().enumerate() {
        let c = Vec2::new(x, map_r.y + 26.0 + k as f32 * 40.0);
        l.ui.solid(Rect::new(c.x - 17.0, c.y - 17.0, 34.0, 34.0));
        l.ui.p().circle(c, 17.0, ON_MAP);
        if l.ui.icon_button(&format!("le-zoom-{k}"), c, 14.0, icon, tip) {
            match k {
                0 => l.mapview.zoom_by(1.0 / 1.5),
                1 => l.mapview.zoom_by(1.5),
                _ => l.mapview.refit(),
            }
        }
    }
}

/// The mouse over the map: a stop clicked goes into the direction (after the stop chosen in
/// the list, else at its end) or is chosen there; a dragged point of a leg moves, a leg
/// dragged gets a new one; anything else moves and zooms the map.
fn map_pointer(l: &mut Launcher, map_r: Rect) {
    let mouse = l.ui.input.mouse;
    let over = map_r.contains(mouse) && !l.ui.over_ui;
    let ready = l.mapview.network().is_some();
    // what is under the mouse (the picture as it was drawn this frame)
    let (hover_stop, hover_via, hover_leg) = {
        let v = &l.pages.lines;
        let mut stop = None;
        let mut best = STOP_HIT;
        if over && ready {
            for (i, s) in v.stops.iter().enumerate() {
                let d = (l.mapview.project(dv(s.at)) - mouse).length();
                if d < best {
                    best = d;
                    stop = Some(i);
                }
            }
        }
        let mut via = None;
        let mut leg = None;
        if let (true, Some(d)) = (over && ready, v.line().and_then(|x| x.directions.get(v.dir))) {
            for (k, g) in d.legs.iter().enumerate() {
                for (j, p) in g.vias.iter().enumerate() {
                    if (l.mapview.project(dv(*p)) - mouse).length() < VIA_HIT {
                        via = Some((k, j));
                    }
                }
            }
            if stop.is_none() && via.is_none() {
                for (k, s) in v.shapes.get(v.dir).map(|x| x.as_slice()).unwrap_or(&[]).iter().enumerate() {
                    let pts: Vec<Vec2> = s.iter().map(|p| l.mapview.project(*p)).collect();
                    if to_polyline(mouse, &pts) < LEG_HIT && k < d.legs.len() {
                        leg = Some(k);
                    }
                }
            }
        }
        (stop, via, leg)
    };
    if l.pages.lines.hover_stop != hover_stop {
        l.pages.lines.hover_stop = hover_stop;
    }
    let has_line = l.pages.lines.line().is_some();
    if has_line && (hover_stop.is_some() || hover_via.is_some() || hover_leg.is_some()) && l.pages.lines.drag.is_none() {
        l.ui.cursor = winit::window::CursorIcon::Pointer;
    }
    // a drag of a leg or of one of its points starts, goes on, ends
    if l.ui.input.pressed && over && has_line {
        l.pages.lines.drag = match (hover_via, hover_leg) {
            (Some((k, j)), _) => Some(Drag::Via(k, j)),
            (None, Some(k)) => Some(Drag::New(k)),
            _ => None,
        };
    }
    let dragging = l.pages.lines.drag;
    if dragging.is_some() {
        l.ui.cursor = winit::window::CursorIcon::Grabbing;
        let at = l.mapview.world_of(mouse);
        if l.ui.input.released {
            l.pages.lines.drag = None;
            let v = &mut l.pages.lines;
            let d_idx = v.dir;
            if let Some(d) = v.line_mut().and_then(|x| x.directions.get_mut(d_idx)) {
                match dragging {
                    Some(Drag::Via(k, j)) => {
                        if let Some(p) = d.legs.get_mut(k).and_then(|g| g.vias.get_mut(j)) {
                            *p = [at.x, at.y];
                        }
                    }
                    Some(Drag::New(k)) => {
                        if k < d.legs.len() {
                            // (in its place along the leg: after the points nearer its start)
                            let a = dv(d.stops[k].at);
                            let along = |p: DVec2| (p - a).length();
                            let g = &mut d.legs[k];
                            let pos = g.vias.iter().position(|p| along(dv(*p)) > along(at)).unwrap_or(g.vias.len());
                            g.vias.insert(pos, [at.x, at.y]);
                        }
                    }
                    None => {}
                }
            }
            v.reroute(d_idx);
        } else {
            // (where the point goes, while it is held)
            let p = l.mapview.project(at);
            l.ui.p().circle(p, 6.0, TEXT);
            l.ui.p().circle(p, 3.5, accent());
        }
    }
    let p = Pointer { at: mouse, pressed: l.ui.input.pressed, released: l.ui.input.released, down: l.ui.input.down, wheel: l.ui.input.wheel.y, blocked: l.ui.over_ui || !map_r.contains(mouse) || dragging.is_some() };
    let scale = l.ui.scale;
    l.mapview.think(map_r, map_r, scale, p);
    // a click on a stop
    if let Some(at) = l.mapview.take_click_at() {
        if !has_line {
            return;
        }
        let v = &mut l.pages.lines;
        let stop = v.stops.iter().enumerate().map(|(i, s)| (i, (l.mapview.project(dv(s.at)) - at).length())).filter(|x| x.1 < STOP_HIT).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
        let Some(i) = stop else { return };
        let s = v.stops[i].clone();
        let d_idx = v.dir;
        let sel = v.sel_stop;
        let Some(line) = v.line_mut() else { return };
        if d_idx >= line.directions.len() {
            return;
        }
        let d = &mut line.directions[d_idx];
        if let Some(k) = d.stops.iter().position(|x| x.id == s.id && x.tile == s.tile) {
            // (a stop of the line: chosen in the list)
            v.sel_stop = Some(k);
            v.revision += 1;
            return;
        }
        let at_k = match sel {
            Some(k) if k + 1 < d.stops.len() => k + 1,
            _ => d.stops.len(),
        };
        d.stops.insert(at_k, s);
        // (the leg it lands in is two legs now, its dragged points gone)
        d.fit_legs();
        if at_k > 0 && at_k < d.stops.len() - 1 {
            d.legs.insert(at_k - 1, Default::default());
            d.legs.truncate(d.stops.len() - 1);
            if let Some(g) = d.legs.get_mut(at_k) {
                g.vias.clear();
            }
        }
        v.sel_stop = if sel.is_some() { Some(at_k) } else { None };
        v.reroute(d_idx);
    }
}

/// A narrow window (a phone): the lines of the map, to read; the editor wants a wide one.
fn narrow(l: &mut Launcher, area: Rect) {
    let v = &l.pages.lines;
    let mut y = area.y;
    let h = l.ui.paragraph("The line editor needs a wider window: open it on a computer. Your lines of this map:", Vec2::new(area.x, y), area.w, 13.0, Weight::Regular, TEXT_DIM);
    y += h + 12.0;
    let lines: Vec<(String, String, usize)> = v.reg.lines.iter().map(|x| (x.number.clone(), x.name.clone(), x.directions.iter().map(|d| d.stops.len()).max().unwrap_or(0))).collect();
    for (number, name, stops) in lines {
        let plate = Rect::new(area.x, y, (l.ui.width(&number, 13.0, Weight::Black) + 16.0).max(40.0), 24.0);
        l.ui.p().rounded(plate, 5.0, LINE);
        l.ui.text_in(&number, plate, 13.0, Weight::Black, ON_LINE, Align::Center);
        l.ui.text_in(&format!("{name} · {}", omsi_ui::tr("%{n} stops").replace("%{n}", &stops.to_string())), Rect::new(plate.right() + 10.0, y, area.w - plate.w - 10.0, 24.0), 13.0, Weight::Medium, TEXT, Align::Left);
        y += 32.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_and_distances() {
        let c = colour_of("#2a75f7");
        assert_eq!(c, Color::rgba(42, 117, 247, 1.0));
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        assert!((to_polyline(Vec2::new(5.0, 3.0), &pts) - 3.0).abs() < 1e-4);
    }
}
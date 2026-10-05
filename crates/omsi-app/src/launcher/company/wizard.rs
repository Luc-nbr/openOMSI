//! Founding a company: its name and short name, its colours (and a logo picture if there is
//! one), its home map and depot, its first day and how hard the economy is - with the
//! starting capital each difficulty gives.

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::{data, eur, monogram, section};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{self as co, Difficulty};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// The colours a company can wear (Omsi-Hub's accent colours and the classic city bus ones).
pub(super) const PALETTE: [&str; 12] = ["#f28c28", "#e03c31", "#c2185b", "#7b1fa2", "#283593", "#1e88e5", "#00897b", "#43a047", "#fdd835", "#6d4c41", "#455a64", "#f5f5f5"];

pub struct Wizard {
    name: String,
    short: String,
    colours: [usize; 2],
    logo: Option<String>,
    map: usize,
    depot: usize,
    date: String,
    difficulty: usize,
}

impl Wizard {
    pub fn new(l: &Launcher) -> Wizard {
        let map = l.state.maps.iter().position(|m| m.file == l.state.choice.map).unwrap_or(0);
        let date = if co::dates::parse(&l.state.choice.date).is_some() { l.state.choice.date.clone() } else { core::DEFAULT_DATE.to_string() };
        Wizard { name: String::new(), short: String::new(), colours: [0, 4], logo: None, map, depot: 0, date, difficulty: 1 }
    }
}

/// The depot files a company on `map` can give its buses: the map's own first, then those of
/// the installed buses that name the map, else all of them.
pub(super) fn depots_for(map: &core::MapInfo, vehicles: &[core::VehicleInfo]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fn push(s: &str, out: &mut Vec<String>) {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            out.push(s.to_string());
        }
    }
    push(&map.hof, &mut out);
    let folder = map.file.split(['/', '\\']).rev().nth(1).unwrap_or("").to_lowercase();
    let words: Vec<String> = format!("{} {} {}", folder, map.name, map.friendly).to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 4).map(str::to_string).collect();
    let mut all: Vec<&String> = vehicles.iter().flat_map(|v| v.hofs.iter()).collect();
    all.sort_by_key(|h| h.to_lowercase());
    for h in &all {
        let lh = h.to_lowercase();
        if words.iter().any(|w| lh.contains(w.as_str())) {
            push(h, &mut out);
        }
    }
    if out.is_empty() {
        for h in all {
            push(h, &mut out);
        }
    }
    out
}

fn map_label(m: &core::MapInfo) -> String {
    if m.friendly.trim().is_empty() {
        m.name.clone()
    } else {
        m.friendly.clone()
    }
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(mut w) = l.company.wizard.take() else { return };
    let has = l.company.companies.as_ref().is_some_and(|c| !c.is_empty());
    // the head: what this is
    l.ui.text_in("Found your bus company", Rect::new(area.x, area.y, area.w, 30.0), 22.0, Weight::Bold, TEXT, Align::Left);
    l.ui.paragraph("Run your own transport company on a map: buy, lease or rent buses, hire drivers and run the map's lines or your own. Every day of the company is settled with \"Close the day\"; what you drive yourself counts as it was driven.", Vec2::new(area.x, area.y + 36.0), area.w.min(900.0), 13.5, Weight::Regular, TEXT_DIM);
    let top = area.y + 92.0;
    let gap = 18.0;
    let col_w = (area.w - gap) / 2.0;
    let h = (area.bottom() - top - 58.0).max(200.0);
    // who the company is
    let left = section(&mut l.ui, Rect::new(area.x, top, col_w, h), "The company");
    let mut y = left.y;
    l.ui.label(Rect::new(left.x, y, left.w, 18.0), "Name");
    y += 20.0;
    l.ui.text_input("company-name", Rect::new(left.x, y, left.w, ROW), &mut w.name, "Stadtbus Grundorf", None);
    y += ROW + 14.0;
    let half = (left.w - GAP) / 2.0;
    l.ui.label(Rect::new(left.x, y, half, 18.0), "Short name (plates and logo)");
    y += 20.0;
    let suggested = co::short_of(&w.name);
    let placeholder = if suggested.is_empty() { "SG".to_string() } else { suggested };
    l.ui.text_input("company-short", Rect::new(left.x, y, half, ROW), &mut w.short, &placeholder, None);
    if w.short.chars().count() > 4 {
        w.short = w.short.chars().take(4).collect();
    }
    y += ROW + 14.0;
    for (k, title) in ["Main colour", "Second colour"].iter().enumerate() {
        l.ui.label(Rect::new(left.x, y, left.w, 18.0), title);
        y += 22.0;
        let sw = ((left.w - 11.0 * 6.0) / 12.0).min(30.0);
        for (i, hex) in PALETTE.iter().enumerate() {
            let r = Rect::new(left.x + i as f32 * (sw + 6.0), y, sw, sw);
            let (hov, _, clicked) = l.ui.interact(super::super::ui::id_of(&format!("company-colour-{k}-{i}")), r);
            l.ui.p().rounded(r, 6.0, super::super::ownlines::colour_of(hex));
            if w.colours[k] == i {
                l.ui.p().rounded_border(r.inset(-3.0), 8.0, 2.0, TEXT);
            } else if hov {
                l.ui.p().rounded_border(r.inset(-2.0), 7.0, 1.0, TEXT_DIM);
            }
            if clicked {
                w.colours[k] = i;
            }
        }
        y += sw + 14.0;
    }
    // the mark as it will look, and a logo picture
    let preview = co::Company { short: if w.short.trim().is_empty() { placeholder.clone() } else { w.short.trim().to_uppercase() }, colours: [PALETTE[w.colours[0]].into(), PALETTE[w.colours[1]].into()], ..co::found(&co::Founding::default(), "") };
    monogram(&mut l.ui, Rect::new(left.x, y, 64.0, 64.0), &preview);
    let logo_text = w.logo.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()).unwrap_or_else(|| omsi_ui::tr("No logo picture: the short name is the mark.").into_owned());
    l.ui.text_in(&logo_text, Rect::new(left.x + 80.0, y + 4.0, left.w - 80.0, 20.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
    if l.ui.button("company-logo", Rect::new(left.x + 80.0, y + 28.0, 200.0f32.min(left.w - 80.0), 32.0), "Choose a logo picture", Some("photo_camera"), ButtonKind::Normal) {
        if let Some(p) = core::pick_file("Choose a logo picture") {
            w.logo = Some(p.to_string_lossy().to_string());
        }
    }
    // where and how
    let rx = area.x + col_w + gap;
    let right = section(&mut l.ui, Rect::new(rx, top, col_w, h), "Home and difficulty");
    let mut y = right.y;
    let maps: Vec<core::MapInfo> = l.state.maps.clone();
    if maps.is_empty() {
        l.ui.paragraph("No maps found: the company needs a map to be at home on.", Vec2::new(right.x, y), right.w, 13.0, Weight::Regular, WARN);
    } else {
        w.map = w.map.min(maps.len() - 1);
        l.ui.label(Rect::new(right.x, y, right.w, 18.0), "Home map");
        y += 20.0;
        let names: Vec<String> = maps.iter().map(map_label).collect();
        let mut m = w.map;
        if l.ui.select("company-map", Rect::new(right.x, y, right.w, ROW), &mut m, &names) {
            w.map = m;
            w.depot = 0;
        }
        y += ROW + 14.0;
        let depots = depots_for(&maps[w.map], &l.state.vehicles);
        l.ui.label(Rect::new(right.x, y, half.max(200.0), 18.0), "Depot (the buses' depot file)");
        l.ui.label(Rect::new(right.x + right.w * 0.6 + GAP, y, right.w * 0.4 - GAP, 18.0), "First company day");
        y += 20.0;
        if depots.is_empty() {
            l.ui.text_in("The map has no depot file.", Rect::new(right.x, y, right.w * 0.6, ROW), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        } else {
            w.depot = w.depot.min(depots.len() - 1);
            let mut d = w.depot;
            if l.ui.select("company-depot", Rect::new(right.x, y, right.w * 0.6, ROW), &mut d, &depots) {
                w.depot = d;
            }
        }
        l.ui.date_field("company-date", Rect::new(right.x + right.w * 0.6 + GAP, y, right.w * 0.4 - GAP, ROW), &mut w.date);
        y += ROW + 18.0;
    }
    l.ui.label(Rect::new(right.x, y, right.w, 18.0), "Difficulty");
    y += 22.0;
    let texts = [
        "Generous: grants on new buses, more passengers, few breakdowns, loans without interest.",
        "German city bus prices and wages, contracts with penalties, a margin of a few per cent.",
        "Tight: prices rise faster than the contract pays, dear loans, more breakdowns and illness.",
    ];
    let cw = (right.w - 2.0 * GAP) / 3.0;
    let ch = (right.bottom() - y).clamp(110.0, 170.0);
    for (k, d) in Difficulty::ALL.iter().enumerate() {
        let r = Rect::new(right.x + k as f32 * (cw + GAP), y, cw, ch);
        let on = w.difficulty == k;
        if l.ui.row(&format!("company-difficulty-{k}"), r, false) {
            w.difficulty = k;
        }
        l.ui.p().rounded_border(r, RADIUS, if on { 2.0 } else { 1.0 }, if on { accent() } else { HAIRLINE });
        l.ui.text_in(d.label(), Rect::new(r.x + 12.0, r.y + 10.0, r.w - 24.0, 20.0), 15.0, Weight::Bold, if on { TEXT } else { TEXT_SOFT }, Align::Left);
        let capital = co::economy::rules(*d).start_capital;
        l.ui.text_in(&eur(capital), Rect::new(r.x + 12.0, r.y + 32.0, r.w - 24.0, 18.0), 13.0, Weight::Bold, LINE, Align::Left);
        l.ui.paragraph(texts[k], Vec2::new(r.x + 12.0, r.y + 56.0), r.w - 24.0, 11.5, Weight::Regular, TEXT_DIM);
    }
    // found it
    let by = area.bottom() - 42.0;
    let ok = !w.name.trim().is_empty() && !maps.is_empty();
    if has && l.ui.button("company-wizard-cancel", Rect::new(area.right() - 400.0, by, 140.0, 40.0), "Cancel", None, ButtonKind::Normal) {
        l.company.wizard = None;
        return;
    }
    if !ok {
        l.ui.text_in("Give the company a name.", Rect::new(area.x, by, area.w - 420.0, 40.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
    }
    if l.ui.button("company-found", Rect::new(area.right() - 240.0, by, 240.0, 40.0), "Found the company", Some("check_circle"), ButtonKind::Primary) && ok {
        let m = &maps[w.map];
        let depots = depots_for(m, &l.state.vehicles);
        let f = co::Founding {
            name: w.name.trim().to_string(),
            short: w.short.clone(),
            colours: [PALETTE[w.colours[0]].to_string(), PALETTE[w.colours[1]].to_string()],
            logo: w.logo.clone(),
            map: m.file.clone(),
            map_name: map_label(m),
            depot: depots.get(w.depot).cloned().unwrap_or_default(),
            date: w.date.clone(),
            difficulty: Difficulty::ALL[w.difficulty.min(2)],
        };
        let mut c = co::found(&f, &l.state.config.profile);
        c.id = co::store::unused_id(&data(), &c.name);
        l.company.company = Some(c);
        l.company.tab = 0;
        super::changed(l);
        l.state.set_status(omsi_ui::tr("%{name} is founded. Add a line, buy a bus and hire drivers.").replace("%{name}", &f.name), false);
        return;
    }
    l.company.wizard = Some(w);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(file: &str, hof: &str) -> core::MapInfo {
        core::MapInfo { name: "Grundorf".into(), friendly: "Grundorf".into(), file: file.into(), description: String::new(), entry_points: Vec::new(), hof: hof.into(), installed: false }
    }

    fn bus(hofs: &[&str]) -> core::VehicleInfo {
        core::VehicleInfo {
            name: "Bus".into(),
            manufacturer: String::new(),
            type_name: String::new(),
            file: "Vehicles/Bus/bus.bus".into(),
            folder: "Bus".into(),
            description: String::new(),
            default_paint: String::new(),
            paints: Vec::new(),
            hofs: hofs.iter().map(|s| s.to_string()).collect(),
            installed: false,
            missing_packs: Vec::new(),
            numbers: Vec::new(),
        }
    }

    #[test]
    fn the_depots_of_a_map_come_first() {
        let m = map("maps/Grundorf/global.cfg", "Grundorf");
        let v = vec![bus(&["Spandau", "Grundorf_Linie"]), bus(&["grundorf"])];
        assert_eq!(depots_for(&m, &v), vec!["Grundorf", "Grundorf_Linie"]);
        // a map whose name no depot file has: all of them
        let other = map("maps/Neustadt/global.cfg", "");
        let mut o = other.clone();
        o.name = "Neustadt".into();
        o.friendly = "Neustadt".into();
        assert_eq!(depots_for(&o, &v), vec!["grundorf", "Grundorf_Linie", "Spandau"]);
    }
}

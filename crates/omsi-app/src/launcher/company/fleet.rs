//! The fleet and the vehicle market: the company's buses as tiles with their photos (the bus
//! picker's, `busphoto`), new buses (every bus installed, at the price of its kind - bought,
//! with a loan, leased or rented) and the week's used offers; a bus of the fleet opens to its
//! livery, a service, selling or giving it back.

use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind, Key, Ui};
use super::super::Launcher;
use super::{act, ask_market, day_label, dialog_panel, eur, grade, grouped, market_busy, meter, Confirm, Dialog};
use glam::Vec2;
use omsi_launcher_lib::company::market::{self, MarketBus, Payment, UsedOffer};
use omsi_launcher_lib::company::{self as co, BusKind, BusSize, Company, Drive, Tenure, Vehicle};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

#[derive(Default)]
pub struct FleetView {
    tab: usize,
    pub(super) selected: Option<u32>,
    search: String,
    kind: usize,
}

const KINDS: [&str; 6] = ["All", "Solo", "Articulated", "Midibus", "Double-decker", "Electric"];

fn kind_fits(k: BusKind, filter: usize) -> bool {
    match filter {
        1 => k.size == BusSize::Solo,
        2 => k.size == BusSize::Articulated,
        3 => k.size == BusSize::Midi,
        4 => k.size == BusSize::Double,
        5 => k.drive == Drive::Electric,
        _ => true,
    }
}

/// The photo of a bus in a livery, once it is there.
type Photo = Option<(usize, u32, u32)>;

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    // (the photos read and taken while the page is open, as on the bus step)
    super::super::busphoto::work(l);
    if l.company.fleet.tab > 0 {
        ask_market(l);
    }
    let offers = l.company.market.as_ref().map(|m| market::used_offers(&c, m)).unwrap_or_default();
    let labels = [
        omsi_ui::tr("Our buses (%{n})").replace("%{n}", &c.fleet.len().to_string()),
        omsi_ui::tr("New buses").into_owned(),
        omsi_ui::tr("Used buses (%{n})").replace("%{n}", &offers.len().to_string()),
    ];
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut tab = l.company.fleet.tab;
    if l.ui.segmented("company-fleet-tabs", Rect::new(area.x, area.y, 520.0f32.min(area.w), ROW), &mut tab, &refs) {
        l.company.fleet.tab = tab;
    }
    let body = Rect::new(area.x, area.y + ROW + 14.0, area.w, (area.h - ROW - 14.0).max(0.0));
    match l.company.fleet.tab {
        1 => new_buses(l, body, &c),
        2 => used_buses(l, body, &c, offers),
        _ => our_buses(l, body, &c),
    }
}

/// The grid's layout: columns, tile width, photo height and tile height.
fn layout(w: f32) -> (usize, f32, f32, f32) {
    let gap = 14.0;
    let cols = (((w + gap) / (250.0 + gap)).floor() as usize).max(1);
    let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
    let ph = tw * 0.52;
    (cols, tw, ph, ph + 96.0)
}

/// A tile's ground and photo (the bus's initials while it is drawn). Returns (hovered,
/// clicked) and the room under the photo.
fn tile(ui: &mut Ui, r: Rect, id: &str, name: &str, pic: Photo) -> (bool, Rect) {
    let (h, _, clicked) = ui.interact(id_of(id), r);
    let t = ui.anim(id_of(id) ^ 0x7e1, if h { 1.0 } else { 0.0 }, 0.08);
    let fill = FIELD.mix(HOVER, t);
    let ph = r.w * 0.52;
    ui.p().rounded(r, SHEET_RADIUS, fill);
    let under = Rect::new(r.x, r.y, r.w, ph + SHEET_RADIUS);
    match pic {
        Some((tex, w, hh)) => ui.image_cover(under, tex, SHEET_RADIUS, w, hh),
        None => {
            ui.p().rounded_gradient(under, SHEET_RADIUS, Color::rgba(38, 48, 70, 1.0), Color::rgba(24, 31, 46, 1.0));
            let mono = Rect::new(r.center().x - 26.0, r.y + ph * 0.5 - 26.0, 52.0, 52.0);
            ui.p().rounded(mono, RADIUS, Color::WHITE.alpha(0.09));
            ui.text_in(&super::super::buspick::initials(name), mono, 19.0, Weight::Bold, TEXT, Align::Center);
        }
    }
    ui.p().rect(Rect::new(r.x, r.y + ph, r.w, SHEET_RADIUS), fill);
    ui.p().rounded(Rect::new(r.x, r.y + ph, r.w, r.h - ph), SHEET_RADIUS, fill);
    ui.p().rounded_border(r, SHEET_RADIUS, 1.0, EDGE.mix(accent().alpha(0.7), t));
    (clicked, Rect::new(r.x + 14.0, r.y + ph + 10.0, r.w - 28.0, r.h - ph - 20.0))
}

/// A bus's paint names: the bus's own first (empty), then its liveries.
fn liveries_of(l: &Launcher, bus: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    if let Some(v) = l.state.vehicles.iter().find(|v| v.file == bus) {
        out.extend(v.paints.iter().filter(|p| !p.eq_ignore_ascii_case(&v.default_paint)).cloned());
    }
    out
}

fn livery_label(p: &str) -> String {
    if p.is_empty() {
        omsi_ui::tr("Its own livery").into_owned()
    } else {
        p.to_string()
    }
}

/// What a bus is to the company, in a few words, and its colour.
fn status_of(c: &Company, v: &Vehicle) -> (String, Color) {
    if v.in_workshop(&c.date) {
        return (omsi_ui::tr("In the workshop until %{date}").replace("%{date}", &day_label(v.workshop_until.as_deref().unwrap_or(""))), WARN);
    }
    if v.km >= v.next_service_km - 1_000.0 {
        return (omsi_ui::tr("Service due").into_owned(), WARN);
    }
    match &v.tenure {
        Tenure::Rented { until, .. } => (omsi_ui::tr("Rented until %{date}").replace("%{date}", &day_label(until)), EARLY_SOFT),
        Tenure::Leased { until, .. } => (omsi_ui::tr("Leased until %{date}").replace("%{date}", &day_label(until)), EARLY_SOFT),
        Tenure::Owned { .. } => (omsi_ui::tr("Ready").into_owned(), OK),
    }
}

fn our_buses(l: &mut Launcher, area: Rect, c: &Company) {
    if c.fleet.is_empty() {
        l.ui.paragraph("The fleet is empty. Buy a new bus, one of the week's used ones, lease one for years or rent one for a few days - each is a bus installed in your OMSI.", Vec2::new(area.x, area.y + 4.0), area.w.min(760.0), 14.0, Weight::Regular, TEXT_DIM);
        if l.ui.button("company-fleet-to-market", Rect::new(area.x, area.y + 64.0, 200.0, 38.0), "To the market", Some("directions_bus"), ButtonKind::Primary) {
            l.company.fleet.tab = 1;
        }
        return;
    }
    let (cols, tw, _, th) = layout(area.w - 12.0);
    let gap = 14.0;
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let fleet = c.fleet.clone();
    let mut open = None;
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-fleet", area, &mut |ui, v| {
        for (k, bus) in fleet.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (tw + gap), v.y + (k / cols) as f32 * (th + gap), tw, th);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos.get(&root, &bus.bus, &bus.livery, now);
            let (clicked, info) = tile(ui, r, &format!("company-bus-{}", bus.id), &bus.name, pic);
            // the fleet number on the photo, as on the bus
            let nw = ui.width(&bus.number, 14.0, Weight::Black) + 18.0;
            let badge = Rect::new(r.x + 10.0, r.y + 10.0, nw, 24.0);
            ui.p().rounded(badge, 6.0, Color::rgba(9, 12, 24, 0.82));
            ui.text_in(&bus.number, badge, 14.0, Weight::Black, TEXT, Align::Center);
            ui.text_in(&bus.name, Rect::new(info.x, info.y, info.w, 20.0), 14.5, Weight::Bold, TEXT, Align::Left);
            let sub = format!("{}  ·  {}", bus.plate, omsi_ui::tr(bus.kind.label()));
            ui.text_in(&sub, Rect::new(info.x, info.y + 20.0, info.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            let facts = format!("{} km  ·  {}", grouped(bus.km.round()), omsi_ui::tr("%{n} years").replace("%{n}", &format!("{:.0}", bus.age_years(&c.date))));
            ui.text_in(&facts, Rect::new(info.x, info.y + 38.0, info.w * 0.62, 18.0), 12.0, Weight::Regular, TEXT_SOFT, Align::Left);
            meter(ui, Rect::new(info.x + info.w * 0.66, info.y + 45.0, info.w * 0.34, 5.0), bus.condition / 100.0, grade(bus.condition));
            let (status, colour) = status_of(c, bus);
            ui.p().circle(Vec2::new(info.x + 4.0, info.y + 66.0), 3.5, colour);
            ui.text_in(&status, Rect::new(info.x + 14.0, info.y + 57.0, info.w - 14.0, 18.0), 12.0, Weight::Medium, colour, Align::Left);
            if clicked {
                open = Some(bus.id);
            }
        }
        fleet.len().div_ceil(cols) as f32 * (th + gap)
    });
    if let Some(id) = open {
        let liveries = liveries_of(l, &c.vehicle(id).map(|v| v.bus.clone()).unwrap_or_default());
        let livery = c.vehicle(id).and_then(|v| liveries.iter().position(|p| *p == v.livery)).unwrap_or(0);
        l.company.fleet.selected = Some(id);
        l.company.dialog = Some(Dialog::Vehicle { id, livery });
    }
}

/// The market's head: a search and the kinds. Returns the room under it and the buses that
/// fit.
fn market_head(l: &mut Launcher, area: Rect) -> Option<(Rect, Vec<MarketBus>)> {
    let Some(market) = l.company.market.clone() else {
        let t = if market_busy(l) { "Reading the installed buses…" } else { "No buses found in the OMSI folder." };
        l.ui.text_in(t, Rect::new(area.x, area.y, area.w, 24.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return None;
    };
    let sw = 280.0f32.min(area.w * 0.35);
    l.ui.text_input("company-market-search", Rect::new(area.x, area.y, sw, ROW), &mut l.company.fleet.search, "Search", Some("search"));
    let refs: Vec<String> = KINDS.iter().map(|k| omsi_ui::tr(k).into_owned()).collect();
    let refs: Vec<&str> = refs.iter().map(String::as_str).collect();
    let mut kind = l.company.fleet.kind;
    if l.ui.chips("company-market-kinds", Rect::new(area.x + sw + 16.0, area.y + 2.0, area.w - sw - 16.0, 32.0), &mut kind, &refs) {
        l.company.fleet.kind = kind;
    }
    let q = l.company.fleet.search.trim().to_lowercase();
    let list: Vec<MarketBus> = market.into_iter().filter(|b| kind_fits(b.kind, l.company.fleet.kind) && (q.is_empty() || b.name.to_lowercase().contains(&q))).collect();
    Some((Rect::new(area.x, area.y + ROW + 14.0, area.w, (area.h - ROW - 14.0).max(0.0)), list))
}

fn new_buses(l: &mut Launcher, area: Rect, c: &Company) {
    let Some((area, list)) = market_head(l, area) else { return };
    if list.is_empty() {
        l.ui.text_in("No bus fits.", Rect::new(area.x, area.y, area.w, 24.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    let (cols, tw, _, th) = layout(area.w - 12.0);
    let gap = 14.0;
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut open = None;
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-market-new", area, &mut |ui, v| {
        for (k, bus) in list.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (tw + gap), v.y + (k / cols) as f32 * (th + gap), tw, th);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos.get(&root, &bus.file, "", now);
            let (clicked, info) = tile(ui, r, &format!("company-new-{}", bus.file), &bus.name, pic);
            ui.text_in(&bus.name, Rect::new(info.x, info.y, info.w, 20.0), 14.5, Weight::Bold, TEXT, Align::Left);
            ui.text_in(bus.kind.label(), Rect::new(info.x, info.y + 20.0, info.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            let (price, grant) = market::new_offer(c, bus);
            ui.text_in(&eur(price - grant), Rect::new(info.x, info.y + 40.0, info.w, 22.0), 17.0, Weight::Bold, TEXT, Align::Left);
            let (lease, _, _) = market::lease_offer(c, bus);
            let r = co::economy::rules(c.difficulty);
            let rent = co::economy::rent_per_day(bus.kind, &r, c.price_index);
            let alt = omsi_ui::tr("or %{lease} a month leased, %{rent} a day rented").replace("%{lease}", &eur(lease)).replace("%{rent}", &eur(rent));
            ui.text_in(&alt, Rect::new(info.x, info.y + 62.0, info.w, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            if clicked {
                open = Some(k);
            }
        }
        list.len().div_ceil(cols) as f32 * (th + gap)
    });
    if let Some(k) = open {
        l.company.dialog = Some(Dialog::New { bus: list[k].clone(), how: 0, days: 7.0, livery: 0 });
    }
}

fn used_buses(l: &mut Launcher, area: Rect, c: &Company, offers: Vec<UsedOffer>) {
    if l.company.market.is_none() {
        let _ = market_head(l, area);
        return;
    }
    let note = omsi_ui::tr("The week's offers: new ones come on %{date}.").replace("%{date}", &day_label(&co::network::next_monday(c)));
    l.ui.text_in(&note, Rect::new(area.x, area.y, area.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
    let area = Rect::new(area.x, area.y + 30.0, area.w, (area.h - 30.0).max(0.0));
    if offers.is_empty() {
        l.ui.text_in("Nothing is offered this week any more.", Rect::new(area.x, area.y, area.w, 24.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    let (cols, tw, _, th) = layout(area.w - 12.0);
    let gap = 14.0;
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut open = None;
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-market-used", area, &mut |ui, v| {
        for (k, o) in offers.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (tw + gap), v.y + (k / cols) as f32 * (th + gap), tw, th);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos.get(&root, &o.bus.file, "", now);
            let (clicked, info) = tile(ui, r, &format!("company-used-{}", o.no), &o.bus.name, pic);
            ui.text_in(&o.bus.name, Rect::new(info.x, info.y, info.w, 20.0), 14.5, Weight::Bold, TEXT, Align::Left);
            let year = o.built.get(..4).unwrap_or("");
            let sub = format!("{}  ·  {} {}  ·  {} km", omsi_ui::tr(o.bus.kind.label()), omsi_ui::tr("built"), year, grouped(o.km));
            ui.text_in(&sub, Rect::new(info.x, info.y + 20.0, info.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            ui.text_in(&eur(o.price), Rect::new(info.x, info.y + 40.0, info.w * 0.6, 22.0), 17.0, Weight::Bold, TEXT, Align::Left);
            ui.text_in(&omsi_ui::tr("Condition %{n}").replace("%{n}", &format!("{:.0}", o.condition)), Rect::new(info.x, info.y + 62.0, info.w * 0.6, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            meter(ui, Rect::new(info.x + info.w * 0.62, info.y + 68.0, info.w * 0.38, 5.0), o.condition / 100.0, grade(o.condition));
            if clicked {
                open = Some(k);
            }
        }
        offers.len().div_ceil(cols) as f32 * (th + gap)
    });
    if let Some(k) = open {
        l.company.dialog = Some(Dialog::Used { offer: offers[k].clone(), how: 0, livery: 0 });
    }
}

// --- the dialogs -----------------------------------------------------------------------------

/// A row of a price table: what, and the amount.
fn price_row(ui: &mut Ui, r: Rect, label: &str, value: &str, strong: bool) {
    ui.text_in(label, Rect::new(r.x, r.y, r.w * 0.6, r.h), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
    ui.text_in(value, Rect::new(r.x + r.w * 0.4, r.y, r.w * 0.6, r.h), if strong { 15.0 } else { 13.0 }, if strong { Weight::Bold } else { Weight::Medium }, TEXT, Align::Right);
    ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), HAIRLINE);
}

/// The photo and the livery choice on a dialog's left. Returns the livery chosen.
fn bus_side(l: &mut Launcher, r: Rect, bus: &str, name: &str, kind: BusKind, livery: usize, liveries: &[String]) -> usize {
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let paint = liveries.get(livery).cloned().unwrap_or_default();
    let pic = l.showroom.photos.get(&root, bus, &paint, now);
    let ph = r.w * 0.62;
    let photo = Rect::new(r.x, r.y, r.w, ph);
    match pic {
        Some((tex, w, h)) => l.ui.image_cover(photo, tex, RADIUS, w, h),
        None => {
            l.ui.p().rounded_gradient(photo, RADIUS, Color::rgba(38, 48, 70, 1.0), Color::rgba(24, 31, 46, 1.0));
            l.ui.text_in(&super::super::buspick::initials(name), photo, 22.0, Weight::Bold, TEXT, Align::Center);
        }
    }
    l.ui.text_in(kind.label(), Rect::new(r.x, photo.bottom() + 8.0, r.w, 18.0), 12.5, Weight::Medium, TEXT_DIM, Align::Left);
    l.ui.label(Rect::new(r.x, photo.bottom() + 34.0, r.w, 18.0), "Livery");
    let names: Vec<String> = liveries.iter().map(|p| livery_label(p)).collect();
    let mut k = livery.min(names.len().saturating_sub(1));
    l.ui.select("company-dialog-livery", Rect::new(r.x, photo.bottom() + 54.0, r.w, ROW), &mut k, &names);
    l.ui.paragraph("A house livery of your own comes with the livery studio.", Vec2::new(r.x, photo.bottom() + 100.0), r.w, 11.5, Weight::Regular, TEXT_FAINT);
    k
}

pub fn dialog(l: &mut Launcher) {
    let Some(c) = l.company.company.clone() else { return };
    let esc = l.ui.input.keys.contains(&Key::Escape);
    match l.company.dialog.take() {
        Some(Dialog::New { bus, how, days, livery }) => {
            let inner = dialog_panel(l, 760.0, 470.0, "directions_bus", &bus.name);
            let liveries = liveries_of(l, &bus.file);
            let side = Rect::new(inner.x, inner.y, 250.0, inner.h - 50.0);
            let livery = bus_side(l, side, &bus.file, &bus.name, bus.kind, livery, &liveries);
            let right = Rect::new(inner.x + 274.0, inner.y, inner.w - 274.0, inner.h - 50.0);
            let labels: Vec<String> = ["Buy", "With a loan", "Lease", "Rent"].iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let mut how = how;
            l.ui.segmented("company-new-how", Rect::new(right.x, right.y, right.w, ROW), &mut how, &refs);
            let r = co::economy::rules(c.difficulty);
            let (price, grant) = market::new_offer(&c, &bus);
            let mut y = right.y + ROW + 16.0;
            let rh = 30.0;
            let mut days = days;
            let action: String;
            let ok: bool;
            match how {
                0 | 1 => {
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("New price"), &eur(price), false);
                    y += rh;
                    if grant > 0 {
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Grant"), &format!("- {}", eur(grant)), false);
                        y += rh;
                    }
                    if how == 0 {
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("You pay"), &eur(price - grant), true);
                        y += rh;
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Cash afterwards"), &eur(c.cash - price + grant), false);
                        ok = c.cash >= price - grant;
                        action = omsi_ui::tr("Buy for %{amount}").replace("%{amount}", &eur(price - grant));
                    } else {
                        let (monthly, months, rate) = co::finance::loan_terms(&c, price - grant);
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("The bank lends"), &eur(price - grant), true);
                        y += rh;
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Monthly rate"), &eur(monthly), false);
                        y += rh;
                        let terms = omsi_ui::tr("%{n} months at %{rate} %").replace("%{n}", &months.to_string()).replace("%{rate}", &format!("{:.1}", rate * 100.0));
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Term"), &terms, false);
                        y += rh;
                        price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("All rates together"), &eur(monthly * months as i64), false);
                        ok = co::finance::credit_left(&c, price - grant) >= price - grant;
                        if !ok {
                            y += rh + 8.0;
                            l.ui.text_in("The bank does not lend that much.", Rect::new(right.x, y, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
                        }
                        action = omsi_ui::tr("Buy with a loan").into_owned();
                    }
                }
                2 => {
                    let (monthly, months, residual) = market::lease_offer(&c, &bus);
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Monthly rate"), &eur(monthly), true);
                    y += rh;
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Term"), &omsi_ui::tr("%{n} months").replace("%{n}", &months.to_string()), false);
                    y += rh;
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Residual value"), &eur(residual), false);
                    y += rh + 10.0;
                    l.ui.paragraph("No price now: the rate is booked at every month's end, and the bus goes back when the term ends. Insurance is the company's.", Vec2::new(right.x, y), right.w, 12.5, Weight::Regular, TEXT_DIM);
                    ok = c.cash >= monthly;
                    action = omsi_ui::tr("Lease for %{amount} a month").replace("%{amount}", &eur(monthly));
                }
                _ => {
                    let daily = co::economy::rent_per_day(bus.kind, &r, c.price_index);
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Per day"), &eur(daily), false);
                    y += rh + 10.0;
                    l.ui.slider("company-rent-days", Rect::new(right.x, y, right.w, 44.0), &mut days, 1.0, 60.0, 1.0, "Days", &|v| format!("{v:.0}"));
                    y += 54.0;
                    let n = days.round().max(1.0) as i64;
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Together"), &eur(daily * n), true);
                    y += rh + 10.0;
                    l.ui.paragraph("Paid by the day at each day's close; the bus goes back after the last day. A rented bus is a few years old and kept well.", Vec2::new(right.x, y), right.w, 12.5, Weight::Regular, TEXT_DIM);
                    ok = c.cash >= daily * n;
                    action = omsi_ui::tr("Rent for %{n} days").replace("%{n}", &n.to_string());
                }
            }
            let by = inner.bottom() - 38.0;
            if l.ui.button("company-new-cancel", Rect::new(inner.right() - 420.0, by, 120.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
                return;
            }
            if !ok && how != 1 {
                l.ui.text_in("Not enough cash.", Rect::new(right.x, by, 160.0, 38.0), 13.0, Weight::Medium, WARN, Align::Left);
            }
            if l.ui.button("company-new-do", Rect::new(inner.right() - 290.0, by, 290.0, 38.0), &action, Some("check_circle"), ButtonKind::Primary) && ok {
                let paint = liveries.get(livery).cloned().unwrap_or_default();
                let n = days.round().max(1.0) as u32;
                let done = act(l, |c| match how {
                    0 => market::buy_new(c, &bus, Payment::Cash, &paint),
                    1 => market::buy_new(c, &bus, Payment::Loan, &paint),
                    2 => market::lease(c, &bus, &paint),
                    _ => market::rent(c, &bus, n, &paint),
                });
                if let Some(id) = done {
                    joined(l, id);
                    return;
                }
            }
            l.company.dialog = Some(Dialog::New { bus, how, days, livery });
        }
        Some(Dialog::Used { offer, how, livery }) => {
            let inner = dialog_panel(l, 760.0, 470.0, "directions_bus", &offer.bus.name);
            let liveries = liveries_of(l, &offer.bus.file);
            let side = Rect::new(inner.x, inner.y, 250.0, inner.h - 50.0);
            let livery = bus_side(l, side, &offer.bus.file, &offer.bus.name, offer.bus.kind, livery, &liveries);
            let right = Rect::new(inner.x + 274.0, inner.y, inner.w - 274.0, inner.h - 50.0);
            let labels: Vec<String> = ["Buy", "With a loan"].iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let mut how = how;
            l.ui.segmented("company-used-how", Rect::new(right.x, right.y, right.w, ROW), &mut how, &refs);
            let mut y = right.y + ROW + 16.0;
            let rh = 30.0;
            let year = offer.built.get(..4).unwrap_or("").to_string();
            price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Built"), &format!("{year}  ({})", omsi_ui::tr("%{n} years").replace("%{n}", &format!("{:.0}", offer.age_years(&c.date)))), false);
            y += rh;
            price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Kilometres"), &format!("{} km", grouped(offer.km)), false);
            y += rh;
            price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Condition"), &format!("{:.0} / 100", offer.condition), false);
            y += rh;
            price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Price"), &eur(offer.price), true);
            y += rh;
            let ok = if how == 0 {
                price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Cash afterwards"), &eur(c.cash - offer.price), false);
                c.cash >= offer.price
            } else {
                let (monthly, months, rate) = co::finance::loan_terms(&c, offer.price);
                let terms = omsi_ui::tr("%{n} months at %{rate} %").replace("%{n}", &months.to_string()).replace("%{rate}", &format!("{:.1}", rate * 100.0));
                price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &omsi_ui::tr("Monthly rate"), &format!("{}  ·  {}", eur(monthly), terms), false);
                co::finance::credit_left(&c, offer.price) >= offer.price
            };
            y += rh + 10.0;
            l.ui.paragraph("A used bus has its service history: the older and the worse its condition, the more it costs to keep and the sooner it breaks down.", Vec2::new(right.x, y), right.w, 12.5, Weight::Regular, TEXT_DIM);
            let by = inner.bottom() - 38.0;
            if l.ui.button("company-used-cancel", Rect::new(inner.right() - 420.0, by, 120.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
                return;
            }
            if !ok {
                let t = if how == 0 { "Not enough cash." } else { "The bank does not lend that much." };
                l.ui.text_in(t, Rect::new(right.x, by, 200.0, 38.0), 13.0, Weight::Medium, WARN, Align::Left);
            }
            let action = omsi_ui::tr("Buy for %{amount}").replace("%{amount}", &eur(offer.price));
            if l.ui.button("company-used-do", Rect::new(inner.right() - 290.0, by, 290.0, 38.0), &action, Some("check_circle"), ButtonKind::Primary) && ok {
                let paint = liveries.get(livery).cloned().unwrap_or_default();
                let pay = if how == 0 { Payment::Cash } else { Payment::Loan };
                if let Some(id) = act(l, |c| market::buy_used(c, &offer, pay, &paint)) {
                    joined(l, id);
                    return;
                }
            }
            l.company.dialog = Some(Dialog::Used { offer, how, livery });
        }
        Some(Dialog::Vehicle { id, livery }) => {
            let Some(v) = c.vehicle(id).cloned() else { return };
            let inner = dialog_panel(l, 760.0, 500.0, "directions_bus", &format!("{}  ·  {}", v.number, v.name));
            let liveries = liveries_of(l, &v.bus);
            let side = Rect::new(inner.x, inner.y, 250.0, inner.h - 50.0);
            let chosen = bus_side(l, side, &v.bus, &v.name, v.kind, livery, &liveries);
            if chosen != livery {
                let paint = liveries.get(chosen).cloned().unwrap_or_default();
                act(l, |c| {
                    market::set_livery(c, id, &paint);
                    Ok(())
                });
            }
            let right = Rect::new(inner.x + 274.0, inner.y, inner.w - 274.0, inner.h - 50.0);
            let rh = 29.0;
            let mut y = right.y;
            let rows: Vec<(String, String)> = vec![
                (omsi_ui::tr("Plate").into_owned(), v.plate.clone()),
                (omsi_ui::tr("Built").into_owned(), format!("{}  ({})", v.built.get(..4).unwrap_or(""), omsi_ui::tr("%{n} years").replace("%{n}", &format!("{:.1}", v.age_years(&c.date))))),
                (omsi_ui::tr("Kilometres").into_owned(), format!("{} km", grouped(v.km.round()))),
                (omsi_ui::tr("Next service").into_owned(), format!("{} km", grouped(v.next_service_km))),
                (omsi_ui::tr("Condition").into_owned(), format!("{:.0} / 100", v.condition)),
                (omsi_ui::tr("Breakdowns").into_owned(), v.breakdowns.to_string()),
                match &v.tenure {
                    Tenure::Owned { paid, .. } => (omsi_ui::tr("Bought for").into_owned(), format!("{}  ·  {} {}", eur(*paid), omsi_ui::tr("worth now"), eur(market::value_of(&c, &v)))),
                    Tenure::Leased { monthly, until, .. } => (omsi_ui::tr("Leased").into_owned(), format!("{} / {}  ·  {}", eur(*monthly), omsi_ui::tr("month"), day_label(until))),
                    Tenure::Rented { daily, until } => (omsi_ui::tr("Rented").into_owned(), format!("{} / {}  ·  {}", eur(*daily), omsi_ui::tr("day"), day_label(until))),
                },
            ];
            for (k, val) in rows {
                price_row(&mut l.ui, Rect::new(right.x, y, right.w, rh), &k, &val, false);
                y += rh;
            }
            let (status, colour) = status_of(&c, &v);
            l.ui.text_in(&status, Rect::new(right.x, y + 6.0, right.w, 22.0), 13.5, Weight::Bold, colour, Align::Left);
            let by = inner.bottom() - 38.0;
            let sell = match v.tenure {
                Tenure::Owned { .. } => "Sell",
                _ => "Give back",
            };
            if l.ui.button("company-bus-sell", Rect::new(inner.x, by, 150.0, 38.0), sell, None, ButtonKind::Danger) {
                l.company.dialog = Some(Dialog::Confirm { what: Confirm::Sell(id) });
                return;
            }
            if l.ui.button("company-bus-service", Rect::new(inner.x + 162.0, by, 220.0, 38.0), "Service tomorrow", Some("construction"), ButtonKind::Normal) && act(l, |c| market::service(c, id)).is_some() {
                l.state.set_status(omsi_ui::tr("Bus %{n} goes to the workshop tomorrow.").replace("%{n}", &v.number), false);
            }
            if l.ui.button("company-bus-close", Rect::new(inner.right() - 130.0, by, 130.0, 38.0), "Close", None, ButtonKind::Primary) || esc {
                l.company.fleet.selected = None;
                return;
            }
            l.company.dialog = Some(Dialog::Vehicle { id, livery: chosen });
        }
        other => l.company.dialog = other,
    }
}

/// A bus joined the fleet: say so, and show the fleet.
fn joined(l: &mut Launcher, id: u32) {
    let number = l.company.company.as_ref().and_then(|c| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default();
    l.state.set_status(omsi_ui::tr("Bus %{n} joined the fleet.").replace("%{n}", &number), false);
    l.company.dialog = None;
    l.company.fleet.tab = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_filter_the_market() {
        let e_art = BusKind { size: BusSize::Articulated, drive: Drive::Electric };
        assert!(kind_fits(e_art, 0) && kind_fits(e_art, 2) && kind_fits(e_art, 5));
        assert!(!kind_fits(e_art, 1) && !kind_fits(e_art, 3));
        let (cols, tw, ph, th) = layout(1100.0);
        assert_eq!(cols, 4);
        assert!(tw >= 250.0 && ph < tw && th > ph);
    }
}

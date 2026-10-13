//! My buses: the fleet as big cards - the bus's photo (the dealer's picture of it in its
//! livery), its number and name, its age and its condition as a simple bar - and the shop under
//! "Buy a bus": the dealer's buses, those that fit the company's lines first, each opening the
//! dealer's quick buy (`dealer::Sheet::Model`). Repairs and services happen by themselves.

use super::super::super::theme::*;
use super::super::super::Launcher;
use super::super::dealer::{self, Sheet};
use super::super::fleet::{liveries_of, tile};
use super::super::{eur, kit, Dialog};
use super::logic;
use glam::Vec2;
use omsi_launcher_lib::company::{self as co, BusSize, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The cards' least width and the gap between them.
const CARD_W: f32 = 280.0;
const GAP: f32 = 16.0;

/// Columns, card width and height in `w`.
fn grid(w: f32, extra: f32) -> (usize, f32, f32) {
    let cols = (((w + GAP) / (CARD_W + GAP)).floor() as usize).clamp(1, 4);
    let cw = ((w - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    (cols, cw, cw * 0.52 + extra)
}

/// What a bus does today, in a few words, and its colour.
fn today_of(c: &Company, plan: Option<&co::day::Plan>, v: &co::Vehicle) -> (String, Color) {
    if v.in_workshop(&c.date) {
        return (omsi_ui::tr("In the workshop: back on %{date}").replace("%{date}", &super::super::overview::short_date(v.workshop_until.as_deref().unwrap_or(""))), WARN);
    }
    let line = plan.and_then(|p| p.tours.iter().find(|t| t.bus == Some(v.id) && !t.tour.unplanned)).map(|t| t.tour.number.clone());
    match line {
        Some(n) => (omsi_ui::tr("On line %{n} today").replace("%{n}", &n), OK),
        None => (omsi_ui::tr("Free today").into_owned(), TEXT_DIM),
    }
}

/// The fleet page.
pub(super) fn page(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let sub = omsi_ui::tr("%{n} buses · repairs and services happen by themselves").replace("%{n}", &logic::buses(&c).to_string());
    let (body, buy) = super::page_head(l, area, super::BUSES, &sub, Some(("Buy a bus", "add_shopping_cart")));
    if buy {
        super::run(l, super::Act::BuyBus);
        return;
    }
    let fleet: Vec<co::Vehicle> = c.fleet.iter().filter(|v| v.held_on(&c.date)).cloned().collect();
    // the buses on order, said first
    let body = match c.dealer.orders.iter().min_by_key(|o| co::dealer::minutes_of(&o.delivery)) {
        Some(first) => {
            let n: u32 = c.dealer.orders.iter().map(|o| o.contract.count).sum();
            let t = omsi_ui::tr("%{n} buses on their way: the next arrives on %{date}.").replace("%{n}", &n.to_string()).replace("%{date}", &super::super::day_label(&co::dealer::day_of(&first.delivery)));
            l.ui.icon("local_shipping", Vec2::new(body.x + 10.0, body.y + 12.0), 18.0, accent_2());
            l.ui.text_in(&t, Rect::new(body.x + 28.0, body.y, body.w - 28.0, 24.0), kit::BODY, Weight::Medium, TEXT, Align::Left);
            Rect::new(body.x, body.y + 38.0, body.w, (body.h - 38.0).max(0.0))
        }
        None => body,
    };
    if fleet.is_empty() {
        if super::empty(l, body, "directions_bus", "No bus yet", "Buy your first bus in the shop: the buses that fit your lines come first, and it drives from the next morning.", Some(("Buy a bus", "add_shopping_cart"))) {
            super::run(l, super::Act::BuyBus);
        }
        return;
    }
    let plan = l.company.plan.clone();
    let (cols, cw, ch) = grid(body.w - 12.0, 112.0);
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut open = None;
    super::super::super::tour::anchor("company-casual-fleet", body);
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-casual-fleet", body, &mut |ui, v| {
        for (k, bus) in fleet.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (ch + GAP), cw, ch);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos().get(&root, &bus.bus, &bus.livery, now);
            let (clicked, info) = tile(ui, r, &format!("company-casual-bus-{}", bus.id), &bus.name, pic);
            let nw = ui.width(&bus.number, 15.0, Weight::Black) + 20.0;
            let badge = Rect::new(r.x + 10.0, r.y + 10.0, nw, 28.0);
            ui.p().rounded(badge, 6.0, Color::rgba(9, 12, 24, 0.85));
            ui.text_as_is(&bus.number, Vec2::new(badge.center().x, badge.y + 19.5), 15.0, Weight::Black, Color::WHITE, Align::Center);
            ui.text_as_is(&bus.name, Vec2::new(info.x, info.y + 17.0), 16.5, Weight::Bold, TEXT, Align::Left);
            let age = omsi_ui::tr("%{n} years old").replace("%{n}", &format!("{:.0}", bus.age_years(&c.date).max(0.0)));
            ui.text_in(&age, Rect::new(info.x, info.y + 24.0, info.w, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
            // the condition as a simple bar
            let cond = (bus.condition / 100.0).clamp(0.0, 1.0);
            let colour = if cond >= 0.7 { OK } else if cond >= 0.5 { WARN } else { DANGER };
            let bar = Rect::new(info.x, info.y + 54.0, info.w, 8.0);
            ui.p().rounded(bar, 4.0, FIELD);
            ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * cond as f32).max(8.0), bar.h), 4.0, colour);
            let word = if cond >= 0.7 { "Good condition" } else if cond >= 0.5 { "Worn: repaired soon" } else { "Worn out: to the workshop" };
            ui.text_in(word, Rect::new(info.x, info.y + 64.0, info.w, 20.0), 12.5, Weight::Medium, TEXT_DIM, Align::Left);
            let (doing, dc) = today_of(&c, plan.as_ref(), bus);
            ui.text_in(&doing, Rect::new(info.x, info.y + 64.0, info.w, 20.0), 12.5, Weight::Bold, dc, Align::Right);
            if clicked {
                open = Some(bus.id);
            }
        }
        fleet.len().div_ceil(cols) as f32 * (ch + GAP)
    });
    if let Some(id) = open {
        let bus = c.vehicle(id).map(|v| v.bus.clone()).unwrap_or_default();
        let liveries = liveries_of(l, &bus);
        let livery = c.vehicle(id).and_then(|v| liveries.iter().position(|p| *p == v.livery)).unwrap_or(0);
        l.company.fleet.selected = Some(id);
        l.company.dialog = Some(Dialog::Vehicle { id, livery });
    }
}

// --- the shop --------------------------------------------------------------------------------------

/// The shop's size choices (0: every size).
const SIZES: [(&str, Option<BusSize>); 5] = [("All", None), ("Midibus", Some(BusSize::Midi)), ("Solo", Some(BusSize::Solo)), ("Articulated", Some(BusSize::Articulated)), ("Double-decker", Some(BusSize::Double))];

/// The shop: the dealer's buses, what fits the lines first.
pub(super) fn shop(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let room = co::depot::places(&c).saturating_sub(logic::buses(&c));
    let sub = omsi_ui::tr("Sorted by what fits your lines · room for %{n} more in the depot").replace("%{n}", &room.to_string());
    let (body, _) = super::page_head(l, area, super::SHOP, &sub, None);
    let Some(all) = l.company.fleet.dealer.listings.clone() else {
        l.ui.text_in("The dealer is setting out the buses…", Rect::new(body.x, body.y, body.w, 24.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
        return;
    };
    // the sizes, as chips
    let labels: Vec<String> = SIZES.iter().map(|s| omsi_ui::tr(s.0).into_owned()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut size = l.company.casual.shop_size.min(SIZES.len() - 1);
    let chips_w = body.w.min(620.0);
    let ch = l.ui.chips_height(chips_w, 34.0, &refs);
    if l.ui.chips("company-casual-shop-size", Rect::new(body.x, body.y, chips_w, 34.0), &mut size, &refs) {
        l.company.casual.shop_size = size;
    }
    let body = Rect::new(body.x, body.y + ch + 14.0, body.w, (body.h - ch - 14.0).max(0.0));
    let plan = l.company.plan.clone();
    let wanted = logic::sizes_wanted(plan.as_ref(), &c);
    let mut list: Vec<(usize, bool, i64, co::dealer::Listing)> = all
        .into_iter()
        .filter(|x| SIZES[size].1.is_none_or(|s| x.bus.kind.size == s))
        .map(|x| {
            let locked = co::market::kind_allowed(&c, x.bus.kind).is_err();
            (logic::fit_rank(x.bus.kind.size, &wanted), locked, co::dealer::quick_price(&c, &x, 1), x)
        })
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)).then(a.2.cmp(&b.2)).then_with(|| a.3.bus.name.cmp(&b.3.bus.name)));
    if list.is_empty() {
        super::empty(l, body, "directions_bus", "No bus of this size", "The dealer sells the buses installed in your OMSI. Choose another size.", None);
        return;
    }
    let (cols, cw, chh) = grid(body.w - 12.0, 120.0);
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut pick = None;
    let cash = c.cash;
    super::super::super::tour::anchor("company-casual-shop", body);
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-casual-shop", body, &mut |ui, v| {
        for (k, (rank, locked, price, x)) in list.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (chh + GAP), cw, chh);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos().get(&root, &x.bus.file, &x.bus.default_paint, now);
            let (clicked, info) = tile(ui, r, &format!("company-casual-shop-{}", x.bus.file), &x.bus.name, pic);
            if *rank == 0 && !locked {
                let t = omsi_ui::tr("Fits your lines").into_owned();
                let tw = ui.width(&t, 12.0, Weight::Bold) + 18.0;
                kit::tag(ui, Vec2::new(r.x + 10.0, r.y + 10.0), &t, OK);
                let _ = tw;
            }
            let title = if x.model.trim().is_empty() { x.bus.name.clone() } else { format!("{} {}", x.maker, x.model) };
            ui.text_in(&title, Rect::new(info.x, info.y, info.w, 24.0), 16.5, Weight::Bold, TEXT, Align::Left);
            let mut facts = omsi_ui::tr(x.bus.kind.label()).into_owned();
            if let Some(s) = x.seats {
                facts = format!("{facts} · {}", omsi_ui::tr("%{n} seats").replace("%{n}", &s.to_string()));
            }
            ui.text_in(&facts, Rect::new(info.x, info.y + 25.0, info.w, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
            if *locked {
                ui.icon("lock", Vec2::new(info.x + 8.0, info.y + 62.0), 15.0, TEXT_DIM);
                ui.text_in("Opens later, as your company grows", Rect::new(info.x + 22.0, info.y + 51.0, info.w - 22.0, 22.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Left);
            } else {
                ui.text_as_is(&eur(*price), Vec2::new(info.x, info.y + 68.0), 19.0, Weight::Bold, if *price <= cash { TEXT } else { WARN }, Align::Left);
                ui.text_in("Buy", Rect::new(info.x, info.y + 50.0, info.w, 24.0), kit::BODY, Weight::Bold, accent_2(), Align::Right);
            }
            if clicked {
                pick = Some(k);
            }
        }
        list.len().div_ceil(cols) as f32 * (chh + GAP)
    });
    if let Some(k) = pick {
        let x = list[k].3.clone();
        if let Err(e) = co::market::kind_allowed(&c, x.bus.kind) {
            kit::refuse(l, e);
            return;
        }
        dealer::open(l, Sheet::Model { listing: x, livery: 0, count: 1.0, pay: 0, quick: true });
    }
}

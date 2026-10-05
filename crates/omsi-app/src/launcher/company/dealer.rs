//! The dealer (the rules are `company::dealer`'s): the showroom by maker, model and version
//! with its filters, the day's special offers, the used market, and the orders and signed
//! contracts. A bus opens to its sheet - its photo and livery, what it is, the years it was
//! built, its price - with a test drive, the quick buy and the talk with the dealer; a talk
//! that ends in a price leads to the contract, which is signed with a drawn signature or a
//! typed name before anything is booked.
//!
//! The company's buying mode decides what a bus's sheet shows first: the quick buy (Simple)
//! or the talk (Advanced); the other is a click away. A test drive is a free drive of the
//! game with that bus at the company's depot, outside the company's time; back from it, the
//! dealer asks how it went.

use super::super::buspick::{self, name_cmp};
use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind, Key};
use super::super::Launcher;
use super::fleet::{bus_side, layout, liveries_of, livery_label, price_row, tile};
use super::{act, ask_market, data, day_label, dialog_panel, eur, grade, grouped, market_busy, meter, Dialog};
use glam::Vec2;
use omsi_launcher_lib::company::dealer::{self as dl, Availability, BuyingMode, Contract, Extra, Filter, Listing, Move, Offer, OfferKind, PayWay, Quote, Reply, Signed, Talk, Years};
use omsi_launcher_lib::company::finance::LoanContract;
use omsi_launcher_lib::company::market::Payment;
use omsi_launcher_lib::company::{self as co, BusSize, Company, Drive};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};

/// Where the showroom is: the makers, a maker's models, or a model's versions.
#[derive(Clone, Debug, Default, PartialEq)]
enum Level {
    #[default]
    Makers,
    Models(String),
    Versions(String, String),
}

/// A sheet over the dealer's pages (in the company's dialog `Dialog::Dealer`).
pub(super) enum Sheet {
    /// A model of the showroom: its photo and livery, what it is, the quick buy or the talk.
    Model { listing: Listing, livery: usize, count: f32, pay: usize, quick: bool },
    /// An offer: a used bus, a special offer, a demonstrator, a fleet sale.
    Offer { offer: Offer, livery: usize, count: f32, pay: usize, quick: bool },
    /// Haggling: the talk, what was said, and the offer being typed.
    Talk { talk: Talk, listing: Listing, offer: Option<Offer>, livery: String, bid: String, said: Vec<(bool, String)> },
    /// The contract: to sign, or one signed to look at.
    Contract { contract: Contract, strokes: Vec<Vec<Vec2>>, readonly: bool },
    /// Back from a test drive.
    Back { drive: co::dealer::TestDrive },
    /// A loan: its amount (euros) and term (an index into the terms offered), and what it is
    /// for; `fixed` when a purchase sets the amount.
    Loan { amount: f32, term: usize, purpose: String, collateral: i64, fixed: bool, then: Then },
    /// The loan contract: to sign, or one signed to look at.
    LoanContract { contract: LoanContract, strokes: Vec<Vec<Vec2>>, readonly: bool, collateral: i64, then: Then },
    /// Paying a loan back early: the confirmation with its fee.
    Repay { id: u32, amount: i64 },
}

/// What a loan pays for, once its contract is signed.
pub(super) enum Then {
    /// Nothing: the money goes into the cash.
    Nothing,
    /// A purchase contract already signed by the buyer.
    Purchase(Box<Contract>),
    /// A quick buy of a model.
    Quick { listing: Listing, count: u32, livery: String },
    /// A quick buy of an offer.
    QuickOffer { offer: Offer, count: u32, livery: String },
}

#[derive(Default)]
pub struct DealerView {
    pub(super) tab: usize,
    level: Level,
    search: String,
    size: usize,
    drive: usize,
    avail: usize,
    /// The highest price in thousands of euros (at the slider's end: any).
    max_price: f32,
    /// Built in this year (at the slider's start: any).
    year: f32,
    pub(super) listings: Option<Vec<Listing>>,
    listings_key: Option<(usize, usize)>,
    rx: Option<Receiver<Vec<Listing>>>,
    pub(super) sheet: Option<Sheet>,
    /// The company and day the dealer's clock last ran for.
    ticked: Option<(String, String)>,
}

const PRICE_ANY: f32 = 1000.0;
const YEAR_ANY: f32 = 1959.0;
const SIZES: [&str; 5] = ["All", "Midibus", "Solo", "Articulated", "Double-decker"];
const DRIVES: [&str; 3] = ["All", "Diesel", "Electric"];
const AVAIL: [&str; 3] = ["All", "New", "Second-hand only"];

impl DealerView {
    fn filter(&self) -> Filter {
        Filter {
            search: self.search.clone(),
            size: match self.size {
                1 => Some(BusSize::Midi),
                2 => Some(BusSize::Solo),
                3 => Some(BusSize::Articulated),
                4 => Some(BusSize::Double),
                _ => None,
            },
            drive: match self.drive {
                1 => Some(Drive::Diesel),
                2 => Some(Drive::Electric),
                _ => None,
            },
            new: match self.avail {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            },
            max_price: if self.max_price <= 0.0 || self.max_price >= PRICE_ANY { 0 } else { (self.max_price as i64) * 1000_00 },
            year: if self.year <= YEAR_ANY { 0 } else { self.year as i32 },
        }
    }
}

// --- the catalogue and the dealer's clock ---------------------------------------------------

/// The showroom's buses: the market's with their families (the bus step's tree), years and
/// cabins (read on a thread of their own once the market is read).
fn ask_listings(l: &mut Launcher) {
    ask_market(l);
    let view = &mut l.company.fleet.dealer;
    if let Some(rx) = &view.rx {
        if let Ok(list) = rx.try_recv() {
            view.listings = Some(list);
            view.rx = None;
        }
    }
    let Some(market) = l.company.market.as_ref() else { return };
    let key = (market.len(), l.state.vehicles.len());
    let view = &mut l.company.fleet.dealer;
    if view.listings_key == Some(key) {
        return;
    }
    view.listings_key = Some(key);
    let market = market.clone();
    let vehicles = l.state.vehicles.clone();
    let (tx, rx) = channel();
    view.rx = Some(rx);
    std::thread::spawn(move || {
        let tree = buspick::build_tree(&vehicles, None, &HashSet::new());
        let overrides = dl::load_overrides(&data());
        let list: Vec<Listing> = market
            .iter()
            .filter_map(|b| {
                let v = vehicles.iter().find(|v| v.file == b.file)?;
                let family = tree.buses.iter().find(|x| x.file == b.file).map(|x| (x.group.clone(), x.model.clone(), x.version.clone())).unwrap_or_else(|| (v.manufacturer.clone(), b.name.clone(), String::new()));
                Some(dl::listing_of(v, b.clone(), family, &dl::read_specs(&b.file), &overrides))
            })
            .collect();
        let _ = tx.send(list);
    });
}

/// The dealer's clock: the orders due are delivered (said in the status line), what is over
/// is forgotten - once a company day, and whenever an order is due. (The company clock calls
/// `dealer::tick` itself once it runs.)
pub(super) fn tick(l: &mut Launcher) {
    // (the showroom is read as soon as the company is open: it is ready when the dealer is)
    if l.company.company.is_some() {
        ask_listings(l);
    }
    let Some(c) = l.company.company.as_ref() else { return };
    let now = dl::now_of(c);
    let key = (c.id.clone(), c.date.clone());
    let due = c.dealer.orders.iter().any(|o| dl::minutes_of(&o.delivery) <= dl::minutes_of(&now));
    if !due && l.company.fleet.dealer.ticked.as_ref() == Some(&key) {
        back_from_test(l);
        return;
    }
    l.company.fleet.dealer.ticked = Some(key);
    let Some(done) = act(l, |c| Ok(dl::tick(c, &now))) else { return };
    for d in done {
        let text = omsi_ui::tr("Contract %{no}: %{bus} delivered - fleet numbers %{numbers}.").replace("%{no}", &d.contract.to_string()).replace("%{bus}", &d.name).replace("%{numbers}", &d.numbers.join(", "));
        l.state.set_status(text, false);
    }
    back_from_test(l);
}

/// A test drive is over (the game ended): the dealer asks how it went.
fn back_from_test(l: &mut Launcher) {
    let Some(drive) = l.company.company.as_ref().and_then(|c| c.dealer.test_drive.clone()) else { return };
    if l.state.in_game() || l.company.dialog.is_some() || l.company.reports.is_some() {
        return;
    }
    act(l, |c| Ok(dl::end_test_drive(c)));
    l.company.tab = 1;
    l.company.fleet.tab = 1;
    open(l, Sheet::Back { drive });
}

pub(super) fn open(l: &mut Launcher, s: Sheet) {
    l.company.fleet.dealer.sheet = Some(s);
    l.company.dialog = Some(Dialog::Dealer);
}

// --- words ------------------------------------------------------------------------------------

/// "1984–1992", "since 2018" (empty: not known).
fn years_label(y: Option<Years>) -> String {
    match y {
        None => String::new(),
        Some(Years { from, to: Some(t) }) if t == from => from.to_string(),
        Some(Years { from, to: Some(t) }) => format!("{from}–{t}"),
        Some(Years { from, to: None }) => omsi_ui::tr("since %{year}").replace("%{year}", &from.to_string()),
    }
}

/// "one model", "%{count} models".
fn count_text(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        omsi_ui::tr(one).into_owned()
    } else {
        omsi_ui::tr(many).replace("%{count}", &n.to_string())
    }
}

/// A listing's own name in its family: its version, or its name.
fn version_of(l: &Listing) -> String {
    if l.version.trim().is_empty() {
        l.bus.name.clone()
    } else {
        l.version.clone()
    }
}

/// The years of a family: the earliest first year, the latest last (none: not known).
fn family_years(ls: &[&Listing]) -> Option<Years> {
    let known: Vec<Years> = ls.iter().filter_map(|l| l.years).collect();
    if known.is_empty() || known.len() < ls.len() {
        return None;
    }
    let from = known.iter().map(|y| y.from).min()?;
    let to = if known.iter().any(|y| y.to.is_none()) { None } else { known.iter().filter_map(|y| y.to).max() };
    Some(Years { from, to })
}

fn places(l: &Listing) -> String {
    match (l.seats, l.standing) {
        (Some(s), Some(t)) if s + t > 0 => omsi_ui::tr("%{seats} seats, %{standing} standing").replace("%{seats}", &s.to_string()).replace("%{standing}", &t.to_string()),
        _ => "–".to_string(),
    }
}

fn offer_line(o: &Offer) -> String {
    if o.is_new() {
        omsi_ui::tr("New, from stock").into_owned()
    } else {
        format!("{} {}  ·  {} km  ·  {} {:.0}", omsi_ui::tr("built"), o.built.get(..4).unwrap_or(""), grouped(o.km), omsi_ui::tr("condition"), o.condition)
    }
}

fn reply_text(r: &Reply) -> String {
    match r {
        Reply::Accepted(p) => omsi_ui::tr("Agreed: %{amount} a bus.").replace("%{amount}", &eur(*p)),
        Reply::Discount(p) => omsi_ui::tr("Let me see... I can do %{amount}.").replace("%{amount}", &eur(*p)),
        Reply::Counter(p) => omsi_ui::tr("Not quite. Let us meet at %{amount}.").replace("%{amount}", &eur(*p)),
        Reply::Firm => omsi_ui::tr("That is a fair price already. I cannot go lower.").into_owned(),
        Reply::TooLow(p) => omsi_ui::tr("That is no offer. I could do %{amount}, no less.").replace("%{amount}", &eur(*p)),
        Reply::ExtraGranted(e) => omsi_ui::tr("%{extra}: agreed.").replace("%{extra}", &omsi_ui::tr(e.label())),
        Reply::ExtraRefused(e) => omsi_ui::tr("%{extra}? Not at this price.").replace("%{extra}", &omsi_ui::tr(e.label())),
        Reply::LastOffer(p) => omsi_ui::tr("My last word: %{amount}. Take it or leave it.").replace("%{amount}", &eur(*p)),
        Reply::BrokeOff(until) => omsi_ui::tr("That is enough. Come back after %{date}.").replace("%{date}", &day_label(&dl::day_of(until))),
    }
}

fn move_text(m: &Move) -> String {
    match m {
        Move::AskDiscount => omsi_ui::tr("Can you do something on the price?").into_owned(),
        Move::Offer(p) => omsi_ui::tr("I offer %{amount} a bus.").replace("%{amount}", &eur(*p)),
        Move::AskExtra(e) => omsi_ui::tr("Could you include this: %{extra}?").replace("%{extra}", &omsi_ui::tr(e.label())),
        Move::Accept => omsi_ui::tr("Agreed.").into_owned(),
    }
}

// --- the page ---------------------------------------------------------------------------------

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    ask_listings(l);
    let now = dl::now_of(&c);
    let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
    let offers = dl::day_offers(&c, &listings, &now);
    let used = dl::used_market(&c, &listings, &now);
    // the head: the dealer's parts, and how the company buys
    let labels = [
        omsi_ui::tr("Showroom").into_owned(),
        omsi_ui::tr("Today's offers (%{n})").replace("%{n}", &offers.len().to_string()),
        omsi_ui::tr("Used (%{n})").replace("%{n}", &used.len().to_string()),
        omsi_ui::tr("Orders (%{n})").replace("%{n}", &c.dealer.orders.len().to_string()),
    ];
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut tab = l.company.fleet.dealer.tab;
    let tw = (area.w - 300.0).clamp(300.0, 760.0);
    if l.ui.segmented("company-dealer-tabs", Rect::new(area.x, area.y, tw, ROW), &mut tab, &refs) {
        l.company.fleet.dealer.tab = tab;
    }
    let mw = 230.0;
    let mode_r = Rect::new(area.right() - mw, area.y, mw, ROW);
    l.ui.text_in(&omsi_ui::tr("Buying").to_uppercase(), Rect::new(mode_r.x - 80.0, area.y, 70.0, ROW), 10.0, Weight::Bold, TEXT_DIM, Align::Right);
    let modes: Vec<String> = BuyingMode::ALL.iter().map(|m| omsi_ui::tr(m.label()).into_owned()).collect();
    let mrefs: Vec<&str> = modes.iter().map(String::as_str).collect();
    let mut mode = BuyingMode::ALL.iter().position(|m| *m == c.dealer.mode).unwrap_or(1);
    if l.ui.segmented("company-dealer-mode", mode_r, &mut mode, &mrefs) {
        let m = BuyingMode::ALL[mode];
        act(l, |c| {
            c.dealer.mode = m;
            Ok(())
        });
    }
    l.ui.tooltip(mode_r, "Simple: buy at the list price in one click. Advanced: haggle with the dealer and sign a contract. Both stay at hand on every bus.");
    let body = Rect::new(area.x, area.y + ROW + 14.0, area.w, (area.h - ROW - 14.0).max(0.0));
    if l.company.fleet.dealer.listings.is_none() {
        let t = if market_busy(l) || l.company.fleet.dealer.rx.is_some() { "Reading the installed buses…" } else { "No buses found in the OMSI folder." };
        l.ui.text_in(t, Rect::new(body.x, body.y, body.w, 24.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    match l.company.fleet.dealer.tab {
        1 => offer_grid(l, body, &c, offers, "company-dealer-offers", "The day's special offers: new buses from stock at a discount, demonstrators, and fleets other operators sell. New ones come every company day.", "No special offers today. Come back tomorrow."),
        2 => offer_grid(l, body, &c, used, "company-dealer-used", "The used market changes every day: each bus stays four days or until it is sold. Older models than the company's year allows new are found here.", "Nothing on the used market today."),
        3 => orders(l, body, &c),
        _ => showroom(l, body, &c, &listings, &offers),
    }
}

/// What a card of the showroom opens.
enum To {
    Maker(String),
    Model(String, String),
    Bus(Listing),
}

struct Card {
    key: String,
    title: String,
    sub: String,
    price: String,
    price_colour: Color,
    badge: Option<(String, Color)>,
    file: String,
    to: To,
}

/// The cheapest new price of some listings ("from €280,000"), or "second-hand only".
fn price_of(c: &Company, ls: &[&Listing], one: bool) -> (String, Color) {
    let year = dl::year_of(&c.date);
    let new: Vec<i64> = ls.iter().filter(|l| l.availability(year) == Availability::New).map(|l| dl::list_price(c, l.bus.kind)).collect();
    match new.iter().min() {
        Some(p) if one || new.iter().all(|x| x == p) => (eur(*p), TEXT),
        Some(p) => (omsi_ui::tr("from %{amount}").replace("%{amount}", &eur(*p)), TEXT),
        None => (omsi_ui::tr("Second-hand only").into_owned(), WARN),
    }
}

fn showroom(l: &mut Launcher, area: Rect, c: &Company, all: &[Listing], offers: &[Offer]) {
    let side_w = 240.0f32.min(area.w * 0.3);
    filters(l, Rect::new(area.x, area.y, side_w, area.h), c);
    let right = Rect::new(area.x + side_w + 22.0, area.y, (area.w - side_w - 22.0).max(0.0), area.h);
    let filter = l.company.fleet.dealer.filter();
    let mut fit: Vec<&Listing> = all.iter().filter(|x| filter.fits(x, c)).collect();
    fit.sort_by(|a, b| name_cmp(&a.maker, &b.maker).then_with(|| name_cmp(&a.model, &b.model)).then_with(|| name_cmp(&version_of(a), &version_of(b))));
    let year = dl::year_of(&c.date);
    let searching = !l.company.fleet.dealer.search.trim().is_empty();
    let level = if searching { Level::Versions(String::new(), String::new()) } else { l.company.fleet.dealer.level.clone() };
    // the way back up
    let mut parts = vec![omsi_ui::tr("All makers").into_owned()];
    match &level {
        Level::Models(m) => parts.push(m.clone()),
        Level::Versions(m, model) if !searching => {
            parts.push(m.clone());
            parts.push(model.clone());
        }
        _ => {}
    }
    if searching {
        parts.push(omsi_ui::tr("Search").into_owned());
    }
    if let Some(k) = buspick::crumbs(&mut l.ui, Rect::new(right.x, right.y, right.w - 200.0, 24.0), &parts) {
        if searching {
            l.company.fleet.dealer.search.clear();
        }
        l.company.fleet.dealer.level = match (k, &level) {
            (1, Level::Versions(m, _)) => Level::Models(m.clone()),
            _ => Level::Makers,
        };
    }
    let offered: HashSet<String> = offers.iter().map(|o| o.listing.bus.file.clone()).collect();
    let mut cards: Vec<Card> = Vec::new();
    match &level {
        Level::Makers => {
            let mut makers: Vec<String> = Vec::new();
            for x in &fit {
                if !makers.contains(&x.maker) {
                    makers.push(x.maker.clone());
                }
            }
            for m in makers {
                let ls: Vec<&Listing> = fit.iter().copied().filter(|x| x.maker == m).collect();
                let mut models: Vec<&str> = ls.iter().map(|x| x.model.as_str()).collect();
                models.dedup();
                let (price, pc) = price_of(c, &ls, false);
                let deals = ls.iter().filter(|x| offered.contains(&x.bus.file)).count();
                cards.push(Card {
                    key: format!("mk-{m}"),
                    title: m.clone(),
                    sub: count_text(models.len(), "one model", "%{count} models"),
                    price,
                    price_colour: pc,
                    badge: (deals > 0).then(|| (omsi_ui::tr("Special offer").into_owned(), accent())),
                    file: ls[0].bus.file.clone(),
                    to: To::Maker(m.clone()),
                });
            }
        }
        Level::Models(m) => {
            let ls: Vec<&Listing> = fit.iter().copied().filter(|x| &x.maker == m).collect();
            let mut models: Vec<String> = Vec::new();
            for x in &ls {
                if !models.contains(&x.model) {
                    models.push(x.model.clone());
                }
            }
            for model in models {
                let vs: Vec<&Listing> = ls.iter().copied().filter(|x| x.model == model).collect();
                let (price, pc) = price_of(c, &vs, false);
                let years = years_label(family_years(&vs));
                let n = count_text(vs.len(), "one version", "%{count} versions");
                let used_only = vs.iter().all(|x| x.availability(year) == Availability::UsedOnly);
                cards.push(Card {
                    key: format!("md-{m}-{model}"),
                    title: model.clone(),
                    sub: if years.is_empty() { n } else { format!("{years}  ·  {n}") },
                    price,
                    price_colour: pc,
                    badge: if vs.iter().any(|x| offered.contains(&x.bus.file)) { Some((omsi_ui::tr("Special offer").into_owned(), accent())) } else if used_only { Some((omsi_ui::tr("No longer built").into_owned(), WARN)) } else { None },
                    file: vs[0].bus.file.clone(),
                    to: To::Model(m.clone(), model.clone()),
                });
            }
        }
        Level::Versions(m, model) => {
            for x in fit.iter().filter(|x| searching || (&x.maker == m && &x.model == model)) {
                let (price, pc) = price_of(c, &[*x], true);
                let years = years_label(x.years);
                let kind = omsi_ui::tr(x.bus.kind.label()).into_owned();
                cards.push(Card {
                    key: format!("v-{}", x.bus.file),
                    title: if searching { x.bus.name.clone() } else { version_of(x) },
                    sub: if years.is_empty() { kind } else { format!("{kind}  ·  {years}") },
                    price,
                    price_colour: pc,
                    badge: if offered.contains(&x.bus.file) { Some((omsi_ui::tr("Special offer").into_owned(), accent())) } else if x.availability(year) == Availability::UsedOnly { Some((omsi_ui::tr("No longer built").into_owned(), WARN)) } else { None },
                    file: x.bus.file.clone(),
                    to: To::Bus((*x).clone()),
                });
            }
        }
    }
    let n_text = count_text(fit.len(), "one bus fits", "%{count} buses fit");
    l.ui.text_in(&n_text, Rect::new(right.right() - 190.0, right.y, 190.0, 24.0), 12.5, Weight::Regular, TEXT_DIM, Align::Right);
    let grid = Rect::new(right.x, right.y + 36.0, right.w, (right.h - 36.0).max(0.0));
    if cards.is_empty() {
        l.ui.paragraph("No bus fits: loosen the filters, or look at the used market.", Vec2::new(grid.x, grid.y), grid.w.min(600.0), 14.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let (cols, tw, _, th) = layout(grid.w - 12.0);
    let gap = 14.0;
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut open_card = None;
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area("company-dealer-showroom", grid, &mut |ui, v| {
        for (k, card) in cards.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (tw + gap), v.y + (k / cols) as f32 * (th + gap), tw, th);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos.get(&root, &card.file, "", now);
            let (clicked, info) = tile(ui, r, &format!("company-dealer-{}", card.key), &card.title, pic);
            if let Some((b, colour)) = &card.badge {
                let bw = ui.width(b, 11.5, Weight::Bold) + 18.0;
                let badge = Rect::new(r.x + 10.0, r.y + 10.0, bw, 22.0);
                ui.p().rounded(badge, 6.0, Color::rgba(9, 12, 24, 0.82));
                ui.text_in(b, badge, 11.5, Weight::Bold, *colour, Align::Center);
            }
            ui.text_in(&card.title, Rect::new(info.x, info.y, info.w, 20.0), 14.5, Weight::Bold, TEXT, Align::Left);
            ui.text_in(&card.sub, Rect::new(info.x, info.y + 20.0, info.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            ui.text_in(&card.price, Rect::new(info.x, info.y + 42.0, info.w, 22.0), 16.5, Weight::Bold, card.price_colour, Align::Left);
            if clicked {
                open_card = Some(k);
            }
        }
        cards.len().div_ceil(cols) as f32 * (th + gap)
    });
    if let Some(k) = open_card {
        match &cards[k].to {
            To::Maker(m) => l.company.fleet.dealer.level = Level::Models(m.clone()),
            To::Model(m, model) => l.company.fleet.dealer.level = Level::Versions(m.clone(), model.clone()),
            To::Bus(x) => {
                let quick = c.dealer.mode == BuyingMode::Simple;
                open(l, Sheet::Model { listing: x.clone(), livery: 0, count: 1.0, pay: 0, quick });
            }
        }
    }
}

/// The showroom's filters, down its left side.
fn filters(l: &mut Launcher, r: Rect, c: &Company) {
    let mut y = r.y;
    l.ui.text_input("company-dealer-search", Rect::new(r.x, y, r.w, ROW), &mut l.company.fleet.dealer.search, "Search a bus", Some("search"));
    y += ROW + 16.0;
    let chips = |l: &mut Launcher, y: &mut f32, title: &str, name: &str, labels: &[&str], value: &mut usize| {
        l.ui.label(Rect::new(r.x, *y, r.w, 16.0), title);
        *y += 20.0;
        let names: Vec<String> = labels.iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let h = l.ui.chips_height(r.w, 30.0, &refs);
        l.ui.chips(name, Rect::new(r.x, *y, r.w, 30.0), value, &refs);
        *y += h + 14.0;
    };
    let v = &l.company.fleet.dealer;
    let (mut size, mut drive, mut avail) = (v.size, v.drive, v.avail);
    chips(l, &mut y, "Size", "company-dealer-size", &SIZES, &mut size);
    chips(l, &mut y, "Drive", "company-dealer-drive", &DRIVES, &mut drive);
    chips(l, &mut y, "To be had", "company-dealer-avail", &AVAIL, &mut avail);
    let v = &mut l.company.fleet.dealer;
    (v.size, v.drive, v.avail) = (size, drive, avail);
    if v.max_price <= 0.0 {
        v.max_price = PRICE_ANY;
    }
    l.ui.label(Rect::new(r.x, y, r.w, 16.0), "Price up to");
    y += 18.0;
    let mut p = v.max_price;
    l.ui.slider("company-dealer-price", Rect::new(r.x, y, r.w, 30.0), &mut p, 100.0, PRICE_ANY, 10.0, "", &|x| if x >= PRICE_ANY { omsi_ui::tr("Any").into_owned() } else { format!("{x:.0}k") });
    y += 40.0;
    let this_year = dl::year_of(&c.date) as f32;
    let v = &mut l.company.fleet.dealer;
    v.max_price = p;
    if v.year <= 0.0 || v.year > this_year {
        v.year = YEAR_ANY;
    }
    l.ui.label(Rect::new(r.x, y, r.w, 16.0), "Built in");
    y += 18.0;
    let mut yr = l.company.fleet.dealer.year;
    l.ui.slider("company-dealer-year", Rect::new(r.x, y, r.w, 30.0), &mut yr, YEAR_ANY, this_year, 1.0, "", &|x| if x <= YEAR_ANY { omsi_ui::tr("Any").into_owned() } else { format!("{x:.0}") });
    l.company.fleet.dealer.year = yr;
    y += 46.0;
    // the company's year, and where the years come from
    let note = omsi_ui::tr("The company's year is %{year}: models no longer built are sold second-hand only, models not built yet are not offered.").replace("%{year}", &(this_year as i32).to_string());
    l.ui.icon("calendar_month", Vec2::new(r.x + 8.0, y + 9.0), 16.0, accent_2());
    let h = l.ui.paragraph(&note, Vec2::new(r.x + 24.0, y), r.w - 24.0, 11.5, Weight::Regular, TEXT_DIM);
    y += h + 8.0;
    if y + 32.0 <= r.bottom() && l.ui.button("company-dealer-years", Rect::new(r.x, y, r.w, 32.0), "Model years file", Some("description"), ButtonKind::Normal) {
        let file = dl::overrides_file(&data());
        if !file.exists() {
            let _ = std::fs::write(&file, dl::OVERRIDES_TEMPLATE);
        }
        crate::updater::open_url(&file.to_string_lossy());
        // (read again when the market is: on the next visit)
        l.company.fleet.dealer.listings_key = None;
    }
    l.ui.tooltip(Rect::new(r.x, y, r.w, 32.0), "Your own model years (bus-years.txt): they win over what openOMSI guesses.");
}

/// A grid of offers (the day's or the used market's).
#[allow(clippy::too_many_arguments)]
fn offer_grid(l: &mut Launcher, area: Rect, c: &Company, offers: Vec<Offer>, name: &str, note: &str, empty: &str) {
    let h = l.ui.paragraph(note, Vec2::new(area.x, area.y), area.w.min(900.0), 13.0, Weight::Regular, TEXT_DIM);
    let area = Rect::new(area.x, area.y + h + 12.0, area.w, (area.h - h - 12.0).max(0.0));
    if offers.is_empty() {
        l.ui.text_in(empty, Rect::new(area.x, area.y, area.w, 24.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    let (cols, tw, _, th) = layout(area.w - 12.0);
    let gap = 14.0;
    let root = l.state.config.root.clone();
    let now = l.ui.time;
    let mut open_at = None;
    let lines: Vec<(String, String, String)> = offers
        .iter()
        .map(|o| {
            let until = omsi_ui::tr("until %{date}").replace("%{date}", &day_label(&dl::day_of(&o.expires)));
            let who = if o.count > 1 { format!("{}  ·  {} × ", o.seller, o.count) } else { format!("{}  ·  ", o.seller) };
            (offer_line(o), format!("{who}{until}"), omsi_ui::tr(o.kind.label()).into_owned())
        })
        .collect();
    let Launcher { ui, showroom, .. } = l;
    ui.scroll_area(name, area, &mut |ui, v| {
        for (k, o) in offers.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (tw + gap), v.y + (k / cols) as f32 * (th + gap), tw, th);
            if !ui.rect_visible(r) {
                continue;
            }
            let pic = showroom.photos.get(&root, &o.listing.bus.file, "", now);
            let (clicked, info) = tile(ui, r, &format!("{name}-{}", o.id), &o.listing.bus.name, pic);
            let (line, who, kind) = &lines[k];
            if o.kind != OfferKind::Used {
                let bw = ui.width(kind, 11.5, Weight::Bold) + 18.0;
                let badge = Rect::new(r.x + 10.0, r.y + 10.0, bw, 22.0);
                ui.p().rounded(badge, 6.0, Color::rgba(9, 12, 24, 0.82));
                ui.text_in(kind, badge, 11.5, Weight::Bold, accent(), Align::Center);
            }
            ui.text_in(&o.listing.bus.name, Rect::new(info.x, info.y, info.w, 20.0), 14.5, Weight::Bold, TEXT, Align::Left);
            ui.text_in(line, Rect::new(info.x, info.y + 20.0, info.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            let pw = ui.width(&eur(o.price), 17.0, Weight::Bold);
            ui.text_in(&eur(o.price), Rect::new(info.x, info.y + 40.0, info.w * 0.6, 22.0), 17.0, Weight::Bold, TEXT, Align::Left);
            let save = o.saving();
            if save >= 0.01 && o.kind != OfferKind::Used {
                ui.text_in(&format!("−{:.0} %", save * 100.0), Rect::new(info.x + pw + 10.0, info.y + 40.0, 80.0, 22.0), 12.5, Weight::Bold, OK, Align::Left);
            }
            if o.kind == OfferKind::Used || o.kind == OfferKind::Batch {
                meter(ui, Rect::new(info.x + info.w * 0.66, info.y + 49.0, info.w * 0.34, 5.0), o.condition / 100.0, grade(o.condition));
            }
            ui.text_in(who, Rect::new(info.x, info.y + 62.0, info.w, 16.0), 11.5, Weight::Regular, TEXT_FAINT, Align::Left);
            if clicked {
                open_at = Some(k);
            }
        }
        offers.len().div_ceil(cols) as f32 * (th + gap)
    });
    if let Some(k) = open_at {
        let quick = c.dealer.mode == BuyingMode::Simple;
        open(l, Sheet::Offer { offer: offers[k].clone(), livery: 0, count: 1.0, pay: 0, quick });
    }
}

/// The orders waiting for their delivery, and the contracts signed.
fn orders(l: &mut Launcher, area: Rect, c: &Company) {
    let gap = 16.0;
    let top_h = (60.0 + 40.0 * c.dealer.orders.len().max(1) as f32).min(area.h * 0.45);
    let top = super::section(&mut l.ui, Rect::new(area.x, area.y, area.w, top_h), "On order");
    if c.dealer.orders.is_empty() {
        l.ui.text_in("Nothing on order. A new bus that is not in stock comes some days after its contract is signed.", Rect::new(top.x, top.y, top.w, 22.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
    }
    let mut y = top.y;
    for o in &c.dealer.orders {
        if y + 34.0 > top.bottom() + 8.0 {
            break;
        }
        let k = &o.contract;
        let days = co::dates::between(&c.date, &dl::day_of(&o.delivery)).max(0);
        l.ui.icon("schedule", Vec2::new(top.x + 10.0, y + 15.0), 16.0, accent_2());
        l.ui.text_in(&format!("{} × {}", k.count, k.listing.bus.name), Rect::new(top.x + 28.0, y, top.w * 0.4, 30.0), 13.5, Weight::Bold, TEXT, Align::Left);
        l.ui.text_in(&omsi_ui::tr("Contract %{no}").replace("%{no}", &k.no.to_string()), Rect::new(top.x + top.w * 0.42, y, top.w * 0.16, 30.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
        let when = omsi_ui::tr("Arrives %{date} (in %{n} days)").replace("%{date}", &day_label(&dl::day_of(&o.delivery))).replace("%{n}", &days.to_string());
        l.ui.text_in(&when, Rect::new(top.x + top.w * 0.58, y, top.w * 0.42, 30.0), 12.5, Weight::Medium, TEXT_SOFT, Align::Right);
        y += 36.0;
    }
    let rest = Rect::new(area.x, area.y + top_h + gap, area.w, (area.h - top_h - gap).max(0.0));
    let list = super::section(&mut l.ui, rest, "Signed contracts");
    if c.dealer.contracts.is_empty() {
        l.ui.text_in("No contracts signed yet.", Rect::new(list.x, list.y, list.w, 22.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let contracts: Vec<Contract> = c.dealer.contracts.iter().rev().cloned().collect();
    let mut opened = None;
    l.ui.scroll_area("company-dealer-contracts", list, &mut |ui, v| {
        let mut y = v.y;
        for (i, k) in contracts.iter().enumerate() {
            let r = Rect::new(v.x, y, v.w, 34.0);
            if ui.rect_visible(r) {
                if ui.row(&format!("company-dealer-contract-{}", k.no), r, false) {
                    opened = Some(i);
                }
                ui.icon("description", Vec2::new(r.x + 14.0, r.center().y), 16.0, TEXT_DIM);
                ui.text_in(&format!("{}", k.no), Rect::new(r.x + 30.0, r.y, 40.0, r.h), 12.5, Weight::Bold, TEXT_DIM, Align::Left);
                ui.text_in(&day_label(&dl::day_of(&k.signed_at)), Rect::new(r.x + 70.0, r.y, r.w * 0.2, r.h), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
                ui.text_in(&format!("{} × {}", k.count, k.listing.bus.name), Rect::new(r.x + 70.0 + r.w * 0.2, r.y, r.w * 0.32, r.h), 13.0, Weight::Bold, TEXT, Align::Left);
                ui.text_in(&k.seller, Rect::new(r.x + 70.0 + r.w * 0.52, r.y, r.w * 0.2, r.h), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
                let amount = match k.pay {
                    PayWay::Lease => omsi_ui::tr("Leasing").into_owned(),
                    PayWay::Loan => format!("{}  ·  {}", eur(k.total()), omsi_ui::tr("Bank loan")),
                    PayWay::Cash => eur(k.total()),
                };
                ui.text_in(&amount, Rect::new(r.right() - 260.0, r.y, 250.0, r.h), 13.0, Weight::Bold, TEXT, Align::Right);
            }
            y += 36.0;
        }
        contracts.len() as f32 * 36.0
    });
    if let Some(i) = opened {
        open(l, Sheet::Contract { contract: contracts[i].clone(), strokes: Vec::new(), readonly: true });
    }
}

// --- the sheets ---------------------------------------------------------------------------------

pub fn dialog(l: &mut Launcher) {
    let Some(c) = l.company.company.clone() else { return };
    let esc = l.ui.input.keys.contains(&Key::Escape);
    let Some(sheet) = l.company.fleet.dealer.sheet.take() else {
        l.company.dialog = None;
        return;
    };
    let next = match sheet {
        Sheet::Model { listing, livery, count, pay, quick } => model_sheet(l, &c, listing, livery, count, pay, quick, esc),
        Sheet::Offer { offer, livery, count, pay, quick } => offer_sheet(l, &c, offer, livery, count, pay, quick, esc),
        Sheet::Talk { talk, listing, offer, livery, bid, said } => talk_sheet(l, &c, talk, listing, offer, livery, bid, said, esc),
        Sheet::Contract { contract, strokes, readonly } => contract_sheet(l, &c, contract, strokes, readonly, esc),
        Sheet::Back { drive } => back_sheet(l, &c, drive, esc),
        Sheet::Loan { amount, term, purpose, collateral, fixed, then } => super::bank::loan_sheet(l, &c, amount, term, purpose, collateral, fixed, then, esc),
        Sheet::LoanContract { contract, strokes, readonly, collateral, then } => super::bank::loan_contract_sheet(l, &c, contract, strokes, readonly, collateral, then, esc),
        Sheet::Repay { id, amount } => super::bank::repay_sheet(l, &c, id, amount, esc),
    };
    match next {
        Some(s) => {
            l.company.fleet.dealer.sheet = Some(s);
            if l.company.dialog.is_none() {
                l.company.dialog = Some(Dialog::Dealer);
            }
        }
        None => {
            if matches!(l.company.dialog, Some(Dialog::Dealer)) {
                l.company.dialog = None;
            }
        }
    }
}

/// The rows of what a bus is.
fn spec_rows(l: &mut Launcher, r: Rect, y: &mut f32, rows: &[(String, String)]) {
    let rh = 27.0;
    for (k, v) in rows {
        price_row(&mut l.ui, Rect::new(r.x, *y, r.w, rh), k, v, false);
        *y += rh;
    }
}

/// The number of buses and how they are paid, for a quick buy. Returns (count, pay).
#[allow(clippy::too_many_arguments)]
fn count_and_pay(l: &mut Launcher, r: Rect, y: &mut f32, name: &str, count: f32, max: u32, pay: usize, with_pay: bool) -> (f32, usize) {
    let mut count = count.clamp(1.0, max.max(1) as f32);
    if max > 1 {
        l.ui.slider(&format!("{name}-count"), Rect::new(r.x, *y, r.w, 32.0), &mut count, 1.0, max as f32, 1.0, "Buses", &|v| format!("{v:.0}"));
        *y += 42.0;
    }
    let mut pay = pay;
    if with_pay {
        let labels: Vec<String> = ["Cash", "Bank loan"].iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        l.ui.segmented(&format!("{name}-pay"), Rect::new(r.x, *y, r.w, 32.0), &mut pay, &refs);
        *y += 42.0;
    }
    (count.round().max(1.0), pay)
}

/// A paid amount's line under a quick buy: the cash afterwards, or the bank's rate.
fn pay_line(l: &mut Launcher, c: &Company, r: Rect, y: &mut f32, amount: i64, pay: usize) -> bool {
    let rh = 28.0;
    if pay == 0 {
        price_row(&mut l.ui, Rect::new(r.x, *y, r.w, rh), &omsi_ui::tr("Cash afterwards"), &eur(c.cash - amount), false);
        *y += rh;
        c.cash >= amount
    } else {
        let (monthly, months, rate) = co::finance::loan_terms(c, amount);
        let terms = omsi_ui::tr("%{n} months at %{rate} %").replace("%{n}", &months.to_string()).replace("%{rate}", &super::num(rate * 100.0, 1));
        price_row(&mut l.ui, Rect::new(r.x, *y, r.w, rh), &omsi_ui::tr("Monthly rate"), &format!("{}  ·  {}", eur(monthly), terms), false);
        *y += rh;
        co::finance::credit_left(c, amount) >= amount
    }
}

/// Start a test drive: a free drive of the game with this bus (and livery) on the company's
/// map at its depot, without the timetable (the company's tours do not run, nothing is
/// booked), marked as a test drive. The player is back at the dealer when the game ends.
fn test_drive(l: &mut Launcher, c: &Company, bus: &str, name: &str, paint: &str, offer: Option<String>) {
    if l.state.in_game() {
        l.state.set_status("A game is running: the test drive waits until it has ended.", true);
        return;
    }
    if !omsi_cfg::missing_original_essentials(std::path::Path::new(&l.state.config.root)).is_empty() {
        l.state.set_status("A session needs the original OMSI 2: choose its folder under Setup first.", true);
        return;
    }
    let mut d = l.state.duty();
    d.map = c.map.clone();
    d.bus = bus.to_string();
    d.paint = Some(paint.to_string()).filter(|p| !p.is_empty());
    d.set_vars = l.state.bus_options.for_game(&l.state.config.root, bus, paint);
    d.display_font = l.state.display_fonts.font_for(bus);
    d.plate = None;
    d.number = None;
    d.hof = Some(c.depot.clone()).filter(|h| !h.trim().is_empty());
    d.entry = Some(depot_entry(l, c));
    (d.line, d.tour, d.trip, d.free_line, d.whole_tour) = (None, None, None, None, false);
    d.legs.clear();
    d.date = Some(c.date.clone());
    d.schedule = Some(false);
    d.passengers = Some(false);
    d.on_foot = Some(false);
    d.lan = Some("off".into());
    d.tutorial = None;
    d.situation = None;
    d.editor = false;
    let (bus, name, paint) = (bus.to_string(), name.to_string(), paint.to_string());
    act(l, |c| {
        let now = dl::now_of(c);
        dl::start_test_drive(c, &bus, &name, &paint, offer, &now);
        Ok(())
    });
    l.state.set_status("Starting the test drive…", false);
    l.state.queued_launch = Some(d);
}

/// The map's entry point nearest to the depot: one named after a depot, or the first.
fn depot_entry(l: &Launcher, c: &Company) -> i32 {
    let Some(m) = l.state.maps.iter().find(|m| m.file == c.map) else { return 0 };
    let depot = c.depot.to_lowercase();
    let words = ["betriebshof", "depot", "garage", "remise", "bushof", "stelplaats", "hof"];
    m.entry_points
        .iter()
        .find(|e| {
            let n = e.name.to_lowercase();
            words.iter().any(|w| n.contains(w)) || (!depot.is_empty() && n.contains(&depot))
        })
        .or_else(|| m.entry_points.first())
        .map(|e| e.index)
        .unwrap_or(0)
}

/// The buttons along a sheet's foot: the test drive on the left. Returns whether it was
/// pressed.
fn test_button(l: &mut Launcher, inner: Rect, by: f32) -> bool {
    let busy = l.state.in_game();
    let pressed = l.ui.button("company-dealer-test", Rect::new(inner.x, by, 170.0, 38.0), "Test drive", Some("sports_score"), ButtonKind::Normal);
    l.ui.tooltip(Rect::new(inner.x, by, 170.0, 38.0), "Drive it on your map from your depot: a free drive outside the company's time. Back here when the game ends.");
    pressed && !busy
}

#[allow(clippy::too_many_arguments)]
fn model_sheet(l: &mut Launcher, c: &Company, listing: Listing, livery: usize, count: f32, pay: usize, quick: bool, esc: bool) -> Option<Sheet> {
    let year = dl::year_of(&c.date);
    let avail = listing.availability(year);
    let inner = dialog_panel(l, 920.0, 610.0, "directions_bus", &listing.bus.name);
    let liveries = liveries_of(l, &listing.bus.file);
    let side = Rect::new(inner.x, inner.y, 270.0, inner.h - 50.0);
    let livery = bus_side(l, side, &listing.bus.file, &listing.bus.name, listing.bus.kind, livery, &liveries);
    let paint = liveries.get(livery).cloned().unwrap_or_default();
    let right = Rect::new(inner.x + 296.0, inner.y, inner.w - 296.0, inner.h - 50.0);
    let r = co::economy::rules(c.difficulty);
    let list = dl::list_price(c, listing.bus.kind);
    let grant = co::economy::grant(listing.bus.kind, list, &r, c.price_index);
    let years = years_label(listing.years);
    let mut rows = vec![
        (omsi_ui::tr("Maker").into_owned(), listing.maker.clone()),
        (omsi_ui::tr("Model").into_owned(), if listing.version.is_empty() { listing.model.clone() } else { format!("{}  ·  {}", listing.model, listing.version) }),
        (omsi_ui::tr("Kind").into_owned(), omsi_ui::tr(listing.bus.kind.label()).into_owned()),
        (omsi_ui::tr("Places").into_owned(), places(&listing)),
        (omsi_ui::tr("Built").into_owned(), if years.is_empty() { omsi_ui::tr("Not known: always to be had").into_owned() } else { years }),
    ];
    if avail == Availability::New {
        rows.push((omsi_ui::tr("List price").into_owned(), eur(list)));
        if grant > 0 {
            rows.push((omsi_ui::tr("Grant").into_owned(), format!("- {}", eur(grant))));
        }
        let days = dl::delivery_days(c, &format!("new:{}", listing.bus.file), false, false);
        rows.push((omsi_ui::tr("Delivery").into_owned(), omsi_ui::tr("about %{n} days after signing").replace("%{n}", &days.to_string())));
    }
    let mut y = right.y;
    spec_rows(l, right, &mut y, &rows);
    y += 14.0;
    let by = inner.bottom() - 38.0;
    let mut count = count;
    let mut pay = pay;
    let mut quick = quick;
    let mut next: Option<Sheet> = None;
    let allowed = co::market::kind_allowed(c, listing.bus.kind);
    match avail {
        Availability::UsedOnly | Availability::NotYet => {
            let text = omsi_ui::tr("No longer built in %{year}: to be had second-hand only.").replace("%{year}", &year.to_string());
            l.ui.text_in(&text, Rect::new(right.x, y, right.w, 20.0), 13.5, Weight::Bold, WARN, Align::Left);
            y += 30.0;
            let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
            let now = dl::now_of(c);
            let mut found: Vec<Offer> = dl::used_market(c, &listings, &now);
            found.extend(dl::day_offers(c, &listings, &now));
            found.retain(|o| o.listing.bus.file == listing.bus.file);
            if found.is_empty() {
                l.ui.paragraph("None of them is on offer today: the used market changes every day.", Vec2::new(right.x, y), right.w, 13.0, Weight::Regular, TEXT_DIM);
            }
            for (k, o) in found.iter().enumerate() {
                let rr = Rect::new(right.x, y, right.w, 34.0);
                if y + 34.0 > by - 8.0 {
                    break;
                }
                if l.ui.row(&format!("company-dealer-model-used-{k}"), rr, false) {
                    let quick = c.dealer.mode == BuyingMode::Simple;
                    next = Some(Sheet::Offer { offer: o.clone(), livery: 0, count: 1.0, pay: 0, quick });
                }
                l.ui.text_in(&offer_line(o), Rect::new(rr.x + 10.0, rr.y, rr.w * 0.66, rr.h), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
                l.ui.text_in(&eur(o.price), Rect::new(rr.right() - 150.0, rr.y, 140.0, rr.h), 13.5, Weight::Bold, TEXT, Align::Right);
                y += 36.0;
            }
            if l.ui.button("company-dealer-model-rent", Rect::new(inner.right() - 330.0, by, 180.0, 38.0), "Rent one", Some("schedule"), ButtonKind::Normal) {
                l.company.dialog = Some(Dialog::New { bus: listing.bus.clone(), how: 1, days: 7.0, livery });
                return None;
            }
        }
        Availability::New => {
            let max = 10;
            if quick {
                let (n, p) = count_and_pay(l, right, &mut y, "company-dealer-quick", count, max, pay, true);
                (count, pay) = (n, p);
                let n = count as i64;
                let painting = if paint.is_empty() { 0 } else { dl::painting_cost(c) };
                let total = n * (list + painting - grant);
                if painting > 0 {
                    price_row(&mut l.ui, Rect::new(right.x, y, right.w, 28.0), &omsi_ui::tr("Painting in the livery chosen"), &format!("{} × {}", n, eur(painting)), false);
                    y += 28.0;
                }
                price_row(&mut l.ui, Rect::new(right.x, y, right.w, 30.0), &omsi_ui::tr("You pay"), &eur(total), true);
                y += 30.0;
                let ok = pay_line(l, c, right, &mut y, total, pay);
                if l.ui.button("company-dealer-to-talk", Rect::new(inner.right() - 470.0, by, 190.0, 38.0), "Haggle instead", Some("forum"), ButtonKind::Normal) {
                    quick = false;
                }
                let label = omsi_ui::tr("Buy %{n} for %{amount}").replace("%{n}", &n.to_string()).replace("%{amount}", &eur(total));
                if let Err(e) = allowed {
                    l.ui.text_in(e, Rect::new(right.x, y + 6.0, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
                } else if !ok {
                    let t = if pay == 0 { "Not enough cash." } else { "The bank does not lend that much." };
                    l.ui.text_in(t, Rect::new(right.x, y + 6.0, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
                }
                if l.ui.button("company-dealer-quick-buy", Rect::new(inner.right() - 270.0, by, 270.0, 38.0), &label, Some("payments"), ButtonKind::Primary) && ok && allowed.is_ok() {
                    if pay == 1 {
                        let purpose = format!("{} × {}", n, listing.bus.name);
                        return Some(Sheet::Loan { amount: (total / 100) as f32, term: usize::MAX, purpose, collateral: total, fixed: true, then: Then::Quick { listing, count: n as u32, livery: paint } });
                    }
                    if let Some(ids) = act(l, |c| dl::quick_buy(c, &listing, n as u32, Payment::Cash, &paint)) {
                        joined(l, &ids);
                        return None;
                    }
                }
            } else {
                let (n, _) = count_and_pay(l, right, &mut y, "company-dealer-talk", count, max, pay, false);
                count = n;
                let now = dl::now_of(c);
                let h = l.ui.paragraph("The list price is where the talk begins: ask for a discount, make an offer, ask for extras. Nothing is booked until you sign the contract.", Vec2::new(right.x, y), right.w, 12.5, Weight::Regular, TEXT_DIM);
                y += h + 8.0;
                let sulk = dl::sulking(c, &listing.maker, &now);
                if let Some(until) = &sulk {
                    let t = omsi_ui::tr("The dealer does not want to talk to you until %{date}.").replace("%{date}", &day_label(&dl::day_of(until)));
                    l.ui.text_in(&t, Rect::new(right.x, y, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
                } else if let Err(e) = allowed {
                    l.ui.text_in(e, Rect::new(right.x, y, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
                }
                if l.ui.button("company-dealer-to-quick", Rect::new(inner.right() - 470.0, by, 190.0, 38.0), "Quick buy instead", Some("payments"), ButtonKind::Normal) {
                    quick = true;
                }
                if l.ui.button("company-dealer-talk", Rect::new(inner.right() - 270.0, by, 270.0, 38.0), "Talk to the dealer", Some("forum"), ButtonKind::Primary) && sulk.is_none() && allowed.is_ok() {
                    let q = Quote::new_bus(c, &listing, count as u32);
                    match dl::open_talk(c, &q, &now) {
                        Ok(talk) => {
                            let said = talk.replies.iter().map(|r| (false, reply_text(r))).collect();
                            return Some(Sheet::Talk { talk, listing, offer: None, livery: paint, bid: String::new(), said });
                        }
                        Err(e) => l.state.set_status(omsi_ui::tr(e).into_owned(), true),
                    }
                }
            }
            if l.ui.button("company-dealer-model-rent", Rect::new(inner.x + 182.0, by, 170.0, 38.0), "Lease or rent", Some("schedule"), ButtonKind::Normal) {
                l.company.dialog = Some(Dialog::New { bus: listing.bus.clone(), how: 0, days: 7.0, livery });
                return None;
            }
        }
    }
    if test_button(l, inner, by) {
        test_drive(l, c, &listing.bus.file, &listing.bus.name, &paint, None);
        return None;
    }
    if esc {
        return None;
    }
    if next.is_some() {
        return next;
    }
    Some(Sheet::Model { listing, livery, count, pay, quick })
}

#[allow(clippy::too_many_arguments)]
fn offer_sheet(l: &mut Launcher, c: &Company, offer: Offer, livery: usize, count: f32, pay: usize, quick: bool, esc: bool) -> Option<Sheet> {
    let title = format!("{}  ·  {}", omsi_ui::tr(offer.kind.label()), offer.listing.bus.name);
    let inner = dialog_panel(l, 920.0, 610.0, "stars", &title);
    let liveries = liveries_of(l, &offer.listing.bus.file);
    let side = Rect::new(inner.x, inner.y, 270.0, inner.h - 50.0);
    let livery = bus_side(l, side, &offer.listing.bus.file, &offer.listing.bus.name, offer.listing.bus.kind, livery, &liveries);
    let paint = liveries.get(livery).cloned().unwrap_or_default();
    let right = Rect::new(inner.x + 296.0, inner.y, inner.w - 296.0, inner.h - 50.0);
    let mut rows = vec![(omsi_ui::tr("Seller").into_owned(), offer.seller.clone()), (omsi_ui::tr("Kind").into_owned(), omsi_ui::tr(offer.listing.bus.kind.label()).into_owned()), (omsi_ui::tr("Places").into_owned(), places(&offer.listing))];
    if offer.is_new() {
        rows.push((omsi_ui::tr("Delivery").into_owned(), omsi_ui::tr("from stock: two days after signing").into_owned()));
    } else {
        rows.push((omsi_ui::tr("Built").into_owned(), format!("{}  ({})", offer.built.get(..7).unwrap_or(""), omsi_ui::tr("%{n} years").replace("%{n}", &super::num(offer.age_years(&c.date), 1)))));
        rows.push((omsi_ui::tr("Kilometres").into_owned(), format!("{} km", grouped(offer.km))));
        rows.push((omsi_ui::tr("Condition").into_owned(), format!("{:.0} / 100", offer.condition)));
    }
    if offer.count > 1 {
        rows.push((omsi_ui::tr("On offer").into_owned(), count_text(offer.count as usize, "one bus", "%{count} buses")));
    }
    let otherwise = if offer.is_new() || offer.kind == OfferKind::Demonstrator { "List price" } else { "Book value" };
    rows.push((omsi_ui::tr(otherwise).into_owned(), eur(offer.reference)));
    rows.push((omsi_ui::tr("Price a bus").into_owned(), if offer.saving() >= 0.01 { format!("{}  (−{:.0} %)", eur(offer.price), offer.saving() * 100.0) } else { eur(offer.price) }));
    rows.push((omsi_ui::tr("Offer ends").into_owned(), format!("{}  {}", day_label(&dl::day_of(&offer.expires)), offer.expires.get(11..).unwrap_or(""))));
    let mut y = right.y;
    spec_rows(l, right, &mut y, &rows);
    y += 14.0;
    let by = inner.bottom() - 38.0;
    let allowed = co::market::kind_allowed(c, offer.listing.bus.kind);
    let (mut count, mut pay, mut quick) = (count, pay, quick);
    if quick {
        let (n, p) = count_and_pay(l, right, &mut y, "company-dealer-offer-quick", count, offer.count, pay, true);
        (count, pay) = (n, p);
        let n = count as i64;
        let painting = if paint.is_empty() { 0 } else { dl::painting_cost(c) };
        let r = co::economy::rules(c.difficulty);
        let grant = if offer.is_new() { co::economy::grant(offer.listing.bus.kind, offer.price, &r, c.price_index) } else { 0 };
        let total = n * (offer.price + painting - grant);
        price_row(&mut l.ui, Rect::new(right.x, y, right.w, 30.0), &omsi_ui::tr("You pay"), &eur(total), true);
        y += 30.0;
        let ok = pay_line(l, c, right, &mut y, total, pay);
        if let Err(e) = allowed {
            l.ui.text_in(e, Rect::new(right.x, y + 6.0, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
        }
        if l.ui.button("company-dealer-offer-to-talk", Rect::new(inner.right() - 470.0, by, 190.0, 38.0), "Haggle instead", Some("forum"), ButtonKind::Normal) {
            quick = false;
        }
        let label = omsi_ui::tr("Buy %{n} for %{amount}").replace("%{n}", &n.to_string()).replace("%{amount}", &eur(total));
        if l.ui.button("company-dealer-offer-buy", Rect::new(inner.right() - 270.0, by, 270.0, 38.0), &label, Some("payments"), ButtonKind::Primary) && ok && allowed.is_ok() {
            if pay == 1 {
                let purpose = format!("{} × {}", n, offer.listing.bus.name);
                return Some(Sheet::Loan { amount: (total / 100) as f32, term: usize::MAX, purpose, collateral: total, fixed: true, then: Then::QuickOffer { offer, count: n as u32, livery: paint } });
            }
            let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
            if let Some(ids) = act(l, |c| dl::quick_buy_offer(c, &offer, n as u32, Payment::Cash, &paint, &listings)) {
                joined(l, &ids);
                return None;
            }
        }
    } else {
        let (n, _) = count_and_pay(l, right, &mut y, "company-dealer-offer-talk", count, offer.count, pay, false);
        count = n;
        let now = dl::now_of(c);
        let sulk = dl::sulking(c, &offer.listing.maker, &now).filter(|_| offer.kind != OfferKind::Used && offer.kind != OfferKind::Batch);
        if let Some(until) = &sulk {
            let t = omsi_ui::tr("The dealer does not want to talk to you until %{date}.").replace("%{date}", &day_label(&dl::day_of(until)));
            l.ui.text_in(&t, Rect::new(right.x, y, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
        } else if let Err(e) = allowed {
            l.ui.text_in(e, Rect::new(right.x, y, right.w, 20.0), 13.0, Weight::Medium, WARN, Align::Left);
        }
        if l.ui.button("company-dealer-offer-to-quick", Rect::new(inner.right() - 470.0, by, 190.0, 38.0), "Quick buy instead", Some("payments"), ButtonKind::Normal) {
            quick = true;
        }
        if l.ui.button("company-dealer-offer-talk", Rect::new(inner.right() - 270.0, by, 270.0, 38.0), "Talk to the seller", Some("forum"), ButtonKind::Primary) && sulk.is_none() && allowed.is_ok() {
            let mut q = Quote::of_offer(&offer, count as u32);
            // (a used bus's seller is not the maker's dealer: his own sulk)
            if !offer.is_new() && offer.kind != OfferKind::Demonstrator {
                q.maker = offer.seller.clone();
            }
            match dl::open_talk(c, &q, &now) {
                Ok(talk) => {
                    let said = talk.replies.iter().map(|r| (false, reply_text(r))).collect();
                    return Some(Sheet::Talk { talk, listing: offer.listing.clone(), offer: Some(offer), livery: paint, bid: String::new(), said });
                }
                Err(e) => l.state.set_status(omsi_ui::tr(e).into_owned(), true),
            }
        }
    }
    if test_button(l, inner, by) {
        test_drive(l, c, &offer.listing.bus.file, &offer.listing.bus.name, &paint, Some(offer.id.clone()));
        return None;
    }
    if esc || l.ui.button("company-dealer-offer-close", Rect::new(inner.x + 182.0, by, 110.0, 38.0), "Close", None, ButtonKind::Normal) {
        return None;
    }
    Some(Sheet::Offer { offer, livery, count, pay, quick })
}

/// Euros typed ("245000", "245.000", "€ 245,000") as cents.
fn typed_euros(s: &str) -> Option<i64> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    // (a decimal part of one or two digits is cents: left out; three are thousands)
    let whole = match t.rfind([',', '.']) {
        Some(p) if t.len() - p - 1 <= 2 => &t[..p],
        _ => t.as_str(),
    };
    let digits: String = whole.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().filter(|v| *v > 0).map(|v| v * 100)
}

#[allow(clippy::too_many_arguments)]
fn talk_sheet(l: &mut Launcher, c: &Company, talk: Talk, listing: Listing, offer: Option<Offer>, livery: String, bid: String, said: Vec<(bool, String)>, esc: bool) -> Option<Sheet> {
    let seller = offer.as_ref().map(|o| o.seller.clone()).unwrap_or_else(|| dl::dealer_name(&listing.maker));
    let title = omsi_ui::tr("Talking to %{who}").replace("%{who}", &seller);
    let inner = dialog_panel(l, 940.0, 640.0, "forum", &title);
    let (mut talk, mut bid, mut said) = (talk, bid, said);
    // the left: the bus and the state of the talk
    let lw = 290.0;
    let root = l.state.config.root.clone();
    let now_t = l.ui.time;
    let photo = Rect::new(inner.x, inner.y, lw, lw * 0.5);
    match l.showroom.photos.get(&root, &listing.bus.file, &livery, now_t) {
        Some((tex, w, h)) => l.ui.image_cover(photo, tex, RADIUS, w, h),
        None => l.ui.p().rounded_gradient(photo, RADIUS, Color::rgba(38, 48, 70, 1.0), Color::rgba(24, 31, 46, 1.0)),
    }
    let mut y = photo.bottom() + 10.0;
    let what = if talk.quote.count > 1 { format!("{} × {}", talk.quote.count, listing.bus.name) } else { listing.bus.name.clone() };
    l.ui.text_in(&what, Rect::new(inner.x, y, lw, 20.0), 14.0, Weight::Bold, TEXT, Align::Left);
    y += 28.0;
    price_row(&mut l.ui, Rect::new(inner.x, y, lw, 28.0), &omsi_ui::tr("Asked at first"), &eur(talk.quote.list), false);
    y += 32.0;
    l.ui.text_in(&omsi_ui::tr("He asks now").to_uppercase(), Rect::new(inner.x, y, lw, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    l.ui.text_in(&eur(talk.asking), Rect::new(inner.x, y + 14.0, lw, 32.0), 26.0, Weight::Bold, if talk.agreed { OK } else { TEXT }, Align::Left);
    let off = talk.discount();
    if off >= 0.001 {
        l.ui.text_in(&format!("−{} %", super::num(off * 100.0, 1)), Rect::new(inner.x + lw - 90.0, y + 14.0, 90.0, 32.0), 14.0, Weight::Bold, OK, Align::Right);
    }
    y += 56.0;
    l.ui.text_in(&omsi_ui::tr("Rounds left: %{n} of %{max}").replace("%{n}", &talk.rounds_left().to_string()).replace("%{max}", &talk.max_rounds.to_string()), Rect::new(inner.x, y, lw, 18.0), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
    y += 24.0;
    let start = dl::terms(c.difficulty).patience.max(0.1);
    let mood = (talk.patience / start).clamp(0.0, 1.0);
    l.ui.text_in("The dealer's patience", Rect::new(inner.x, y, lw, 18.0), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
    meter(&mut l.ui, Rect::new(inner.x, y + 22.0, lw, 6.0), mood, grade(mood * 100.0));
    y += 40.0;
    if !talk.extras.is_empty() {
        l.ui.text_in(&omsi_ui::tr("Agreed extras").to_uppercase(), Rect::new(inner.x, y, lw, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
        y += 18.0;
        for e in &talk.extras {
            l.ui.icon(e.icon(), Vec2::new(inner.x + 9.0, y + 10.0), 15.0, OK);
            l.ui.text_in(e.label(), Rect::new(inner.x + 24.0, y, lw - 24.0, 20.0), 12.5, Weight::Medium, TEXT, Align::Left);
            y += 22.0;
        }
    }
    // the right: what was said, and the moves
    let rx = inner.x + lw + 24.0;
    let rw = inner.w - lw - 24.0;
    let actions_h = 150.0;
    let log = Rect::new(rx, inner.y, rw, inner.h - actions_h - 60.0);
    l.ui.card(log);
    let lines: Vec<(bool, String)> = if said.is_empty() { vec![(false, omsi_ui::tr("Good day. This one is %{amount}. What can I do for you?").replace("%{amount}", &eur(talk.quote.list)))] } else { said.clone() };
    l.ui.scroll_area("company-dealer-talk-log", log.inset(12.0), &mut |ui, v| {
        let mut yy = v.y;
        for (you, text) in &lines {
            let w = (v.w * 0.78).min(ui.width(text, 13.0, Weight::Regular) + 28.0);
            let h = ui.paragraph_height(text, w - 24.0, 13.0, Weight::Regular) + 14.0;
            let x = if *you { v.right() - w } else { v.x };
            let r = Rect::new(x, yy, w, h);
            ui.p().rounded(r, 10.0, if *you { accent().alpha(0.22) } else { FIELD.lighten(0.06) });
            ui.paragraph(text, Vec2::new(r.x + 12.0, r.y + 4.0), w - 24.0, 13.0, Weight::Regular, if *you { TEXT } else { TEXT_SOFT });
            yy += h + 8.0;
        }
        yy - v.y
    });
    let ay = log.bottom() + 14.0;
    let mut mv: Option<Move> = None;
    let open_talk = !talk.closed;
    if open_talk {
        if l.ui.button("company-dealer-ask", Rect::new(rx, ay, 230.0, 36.0), "Ask for a discount", Some("trending_up"), ButtonKind::Normal) {
            mv = Some(Move::AskDiscount);
        }
        let iw = rw - 230.0 - 12.0 - 150.0 - 12.0;
        let placeholder = eur(talk.asking);
        l.ui.text_input("company-dealer-bid", Rect::new(rx + 242.0, ay, iw, 36.0), &mut bid, &placeholder, Some("payments"));
        if l.ui.button("company-dealer-offer", Rect::new(rx + rw - 150.0, ay, 150.0, 36.0), "Make an offer", None, ButtonKind::Normal) {
            match typed_euros(&bid) {
                Some(v) => mv = Some(Move::Offer(v)),
                None => l.state.set_status("Type the price you offer for one bus.", true),
            }
        }
        let ey = ay + 46.0;
        let ew = (rw - 3.0 * 10.0) / 4.0;
        for (k, e) in Extra::ALL.iter().enumerate() {
            if *e == Extra::FastDelivery && talk.quote.used {
                continue;
            }
            let r = Rect::new(rx + k as f32 * (ew + 10.0), ey, ew, 34.0);
            let have = talk.extras.contains(e);
            let label = omsi_ui::tr(e.label()).into_owned();
            if l.ui.button(&format!("company-dealer-extra-{k}"), r, &label, Some(if have { "check" } else { e.icon() }), ButtonKind::Normal) && !have {
                mv = Some(Move::AskExtra(*e));
            }
            let worth = omsi_ui::tr("Worth about %{amount} a bus to the dealer").replace("%{amount}", &eur(dl::extra_value(c, *e, talk.quote.list)));
            l.ui.tooltip(r, &worth);
        }
    } else {
        let t = match talk.replies.last() {
            Some(Reply::BrokeOff(_)) => "The dealer broke the talk off. Come back another day - or buy at the list price.",
            _ if talk.agreed => "You have a deal. The contract says what was agreed.",
            _ => "That was his last word: take it, or leave it.",
        };
        l.ui.paragraph(t, Vec2::new(rx, ay + 4.0), rw, 13.5, Weight::Medium, TEXT_SOFT);
    }
    let by = inner.bottom() - 38.0;
    if l.ui.button("company-dealer-leave", Rect::new(rx, by, 150.0, 38.0), "Leave", None, ButtonKind::Normal) || esc {
        return None;
    }
    let broke = matches!(talk.replies.last(), Some(Reply::BrokeOff(_)));
    if !broke {
        let label = omsi_ui::tr("Take %{amount} and draw up the contract").replace("%{amount}", &eur(talk.asking));
        if l.ui.button("company-dealer-deal", Rect::new(inner.right() - 380.0, by, 380.0, 38.0), &label, Some("description"), ButtonKind::Primary) {
            if !talk.agreed {
                mv = Some(Move::Accept);
            } else {
                return Some(contract_of(c, &talk, &listing, offer.as_ref(), &livery));
            }
        }
    }
    if let Some(m) = mv {
        said.push((true, move_text(&m)));
        let now = dl::now_of(c);
        let mut reply = None;
        act(l, |c| {
            reply = Some(dl::respond(c, &mut talk, m, &now));
            Ok(())
        });
        if let Some(r) = reply {
            said.push((false, reply_text(&r)));
            if matches!(r, Reply::Accepted(_)) && matches!(m, Move::Accept) {
                return Some(contract_of(c, &talk, &listing, offer.as_ref(), &livery));
            }
        }
        bid.clear();
        l.ui.scroll_to("company-dealer-talk-log", 1e6, 0.0, 0.0);
    }
    Some(Sheet::Talk { talk, listing, offer, livery, bid, said })
}

/// The contract a talk led to.
fn contract_of(c: &Company, talk: &Talk, listing: &Listing, offer: Option<&Offer>, livery: &str) -> Sheet {
    let contract = match offer {
        Some(o) => dl::draft_offer(c, o, talk.quote.count, talk.asking, &talk.extras, livery),
        None => dl::draft_new(c, listing, talk.quote.count, talk.asking, &talk.extras, livery, None),
    };
    Sheet::Contract { contract, strokes: Vec::new(), readonly: false }
}

fn contract_sheet(l: &mut Launcher, c: &Company, contract: Contract, strokes: Vec<Vec<Vec2>>, readonly: bool, esc: bool) -> Option<Sheet> {
    let title = if readonly { omsi_ui::tr("Purchase contract no. %{no}").replace("%{no}", &contract.no.to_string()) } else { omsi_ui::tr("Purchase contract").into_owned() };
    let inner = dialog_panel(l, 940.0, 690.0, "description", &title);
    let (mut k, mut strokes) = (contract, strokes);
    let now = if readonly { k.signed_at.clone() } else { dl::now_of(c) };
    let gap = 18.0;
    let cw = (inner.w - gap) / 2.0;
    // the parties
    let home = if c.map_name.is_empty() { c.depot.clone() } else { format!("{}  ·  {}", c.map_name, c.depot) };
    let seller_is = if k.new { "Dealer of new buses" } else { "Seller of used buses" };
    let y = parties(l, inner, ("Seller", &k.seller, seller_is), ("Buyer", &k.buyer, &home));
    // the bus, and the terms
    let left = Rect::new(inner.x, y, cw, 0.0);
    let right = Rect::new(inner.x + cw + gap, y, cw, 0.0);
    let rh = 27.0;
    let mut ly = y;
    let row = |l: &mut Launcher, r: Rect, yy: &mut f32, a: &str, b: String, strong: bool| {
        price_row(&mut l.ui, Rect::new(r.x, *yy, r.w, rh), &omsi_ui::tr(a), &b, strong);
        *yy += rh;
    };
    l.ui.text_in(&omsi_ui::tr("The bus").to_uppercase(), Rect::new(left.x, ly, left.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    ly += 18.0;
    row(l, left, &mut ly, "Bus", format!("{} × {}", k.count, k.listing.bus.name), true);
    row(l, left, &mut ly, "Kind", omsi_ui::tr(k.listing.bus.kind.label()).into_owned(), false);
    if k.new {
        row(l, left, &mut ly, "Condition", omsi_ui::tr("New").into_owned(), false);
    } else {
        row(l, left, &mut ly, "Built", k.built.get(..7).unwrap_or("").to_string(), false);
        row(l, left, &mut ly, "Kilometres", format!("{} km", grouped(k.km)), false);
        row(l, left, &mut ly, "Condition", format!("{:.0} / 100", k.condition), false);
    }
    row(l, left, &mut ly, "Livery", livery_label(&k.livery), false);
    let delivery = if k.delivery_days <= 0 { omsi_ui::tr("At once, on signing").into_owned() } else { day_label(&dl::day_of(&k.delivery(&now))) };
    row(l, left, &mut ly, "Delivery", delivery, false);
    row(l, left, &mut ly, "Warranty", omsi_ui::tr("%{n} months").replace("%{n}", &k.warranty_months.to_string()), false);
    let extras = if k.extras.is_empty() { omsi_ui::tr("None").into_owned() } else { k.extras.iter().map(|e| omsi_ui::tr(e.label()).into_owned()).collect::<Vec<_>>().join(", ") };
    l.ui.text_in(&omsi_ui::tr("Extras").to_uppercase(), Rect::new(left.x, ly + 8.0, left.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    l.ui.paragraph(&extras, Vec2::new(left.x, ly + 24.0), left.w, 12.5, Weight::Regular, TEXT_SOFT);
    let mut ry = y;
    l.ui.text_in(&omsi_ui::tr("Price").to_uppercase(), Rect::new(right.x, ry, right.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    ry += 18.0;
    if k.list != k.price {
        row(l, right, &mut ry, "List price a bus", eur(k.list), false);
    }
    row(l, right, &mut ry, "Agreed price a bus", eur(k.price), false);
    if k.painting > 0 {
        row(l, right, &mut ry, "Painting a bus", eur(k.painting), false);
    } else if !k.livery.is_empty() {
        row(l, right, &mut ry, "Painting a bus", omsi_ui::tr("included").into_owned(), false);
    }
    if k.grant > 0 && k.pay != PayWay::Lease {
        row(l, right, &mut ry, "Grant a bus", format!("- {}", eur(k.grant)), false);
    }
    // how it is paid
    ry += 6.0;
    let mut ways = vec![PayWay::Cash, PayWay::Loan];
    if k.new {
        ways.push(PayWay::Lease);
    }
    if readonly {
        row(l, right, &mut ry, "Payment", omsi_ui::tr(k.pay.label()).into_owned(), false);
    } else {
        let labels: Vec<String> = ways.iter().map(|w| omsi_ui::tr(w.label()).into_owned()).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut sel = ways.iter().position(|w| *w == k.pay).unwrap_or(0);
        l.ui.segmented("company-dealer-contract-pay", Rect::new(right.x, ry, right.w, 32.0), &mut sel, &refs);
        k.pay = ways[sel.min(ways.len() - 1)];
        ry += 40.0;
    }
    let ok = match k.pay {
        PayWay::Cash => {
            row(l, right, &mut ry, "To pay on signing", eur(k.due()), true);
            if !readonly {
                row(l, right, &mut ry, "Cash afterwards", eur(c.cash - k.due()), false);
            }
            readonly || c.cash >= k.due()
        }
        PayWay::Loan => {
            let (monthly, months, rate) = co::finance::loan_terms(c, k.due());
            row(l, right, &mut ry, "The bank lends", eur(k.due()), true);
            let terms = omsi_ui::tr("%{n} months at %{rate} %").replace("%{n}", &months.to_string()).replace("%{rate}", &super::num(rate * 100.0, 1));
            row(l, right, &mut ry, "Monthly rate", format!("{}  ·  {}", eur(monthly), terms), false);
            readonly || co::finance::credit_left(c, k.due()) >= k.due()
        }
        PayWay::Lease => {
            let (monthly, months, residual) = k.lease(c);
            row(l, right, &mut ry, "Monthly rate a bus", eur(monthly), true);
            row(l, right, &mut ry, "Term", omsi_ui::tr("%{n} months").replace("%{n}", &months.to_string()), false);
            row(l, right, &mut ry, "Residual value", eur(residual), false);
            readonly || c.cash >= monthly * k.count as i64
        }
    };
    // the signature
    let day = if readonly { k.signed_at.clone() } else { now.clone() };
    let kept = signature(l, inner, &mut strokes, &mut k.signed_by, &k.strokes.clone(), readonly, &day);
    let by = inner.bottom() - 38.0;
    if readonly {
        if l.ui.button("company-dealer-contract-close", Rect::new(inner.right() - 130.0, by, 130.0, 38.0), "Close", None, ButtonKind::Primary) || esc {
            return None;
        }
        return Some(Sheet::Contract { contract: k, strokes, readonly });
    }
    if l.ui.button("company-dealer-contract-cancel", Rect::new(inner.right() - 440.0, by, 140.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
        return None;
    }
    k.strokes = kept;
    let signed = k.is_signed();
    if !signed {
        l.ui.text_in("Sign the contract first.", Rect::new(inner.x, by, 300.0, 38.0), 13.0, Weight::Medium, TEXT_DIM, Align::Left);
    } else if !ok {
        let t = if k.pay == PayWay::Loan { "The bank does not lend that much." } else { "Not enough cash." };
        l.ui.text_in(t, Rect::new(inner.x, by, 300.0, 38.0), 13.0, Weight::Medium, WARN, Align::Left);
    }
    let sign_label = if k.pay == PayWay::Loan { "Sign, then the loan" } else { "Sign the contract" };
    if l.ui.button("company-dealer-sign", Rect::new(inner.right() - 290.0, by, 290.0, 38.0), sign_label, Some("check_circle"), ButtonKind::Primary) && signed && ok {
        // (paid with a loan: its own dialog and contract first, then both are signed at once)
        if k.pay == PayWay::Loan {
            let purpose = format!("{} × {}", k.count, k.listing.bus.name);
            return Some(Sheet::Loan { amount: (k.due() / 100) as f32, term: usize::MAX, purpose, collateral: k.total(), fixed: true, then: Then::Purchase(Box::new(k)) });
        }
        let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
        let now = dl::now_of(c);
        if let Some(done) = act(l, |c| dl::sign(c, &k, &listings, &now)) {
            signed_status(l, done);
            return None;
        }
    }
    Some(Sheet::Contract { contract: k, strokes, readonly })
}

/// Say what a signed purchase did.
pub(super) fn signed_status(l: &mut Launcher, done: Signed) {
    let no = l.company.company.as_ref().map(|c| c.dealer.counter).unwrap_or(0);
    match done {
        Signed::Delivered(ids) => {
            let numbers: Vec<String> = ids.iter().filter_map(|id| l.company.company.as_ref().and_then(|c| c.vehicle(*id)).map(|v| v.number.clone())).collect();
            l.state.set_status(omsi_ui::tr("Contract %{no} is signed: fleet numbers %{numbers} joined the fleet.").replace("%{no}", &no.to_string()).replace("%{numbers}", &numbers.join(", ")), false);
        }
        Signed::Ordered { no, delivery } => {
            l.state.set_status(omsi_ui::tr("Contract %{no} is signed: the buses arrive on %{date}.").replace("%{no}", &no.to_string()).replace("%{date}", &day_label(&dl::day_of(&delivery))), false);
            l.company.fleet.dealer.tab = 3;
        }
    }
}

/// A contract's head: the two parties side by side, each its role, name and what it is.
/// Returns where the terms begin.
pub(super) fn parties(l: &mut Launcher, inner: Rect, a: (&str, &str, &str), b: (&str, &str, &str)) -> f32 {
    let gap = 18.0;
    let cw = (inner.w - gap) / 2.0;
    for (k, (role, name, under)) in [a, b].into_iter().enumerate() {
        let x = inner.x + k as f32 * (cw + gap);
        let y = inner.y;
        l.ui.text_in(&omsi_ui::tr(role).to_uppercase(), Rect::new(x, y, cw, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
        l.ui.text_in(name, Rect::new(x, y + 16.0, cw, 22.0), 16.0, Weight::Bold, TEXT, Align::Left);
        l.ui.text_in(under, Rect::new(x, y + 38.0, cw, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    }
    let y = inner.y + 66.0;
    l.ui.p().rect(Rect::new(inner.x, y, inner.w, 1.0), HAIRLINE);
    y + 12.0
}

/// The height of a contract's signature, above the buttons.
pub(super) const SIGNATURE_H: f32 = 112.0;

/// A contract's foot: a field to sign in with the mouse and the name typed beside it (signed:
/// what was signed, and when). `kept` is the signature as the contract keeps it (0..1 in the
/// field); returns it as drawn now.
#[allow(clippy::too_many_arguments)]
pub(super) fn signature(l: &mut Launcher, inner: Rect, strokes: &mut Vec<Vec<Vec2>>, name: &mut String, kept: &[Vec<[f32; 2]>], readonly: bool, day: &str) -> Vec<Vec<[f32; 2]>> {
    let sy = inner.bottom() - 50.0 - SIGNATURE_H;
    l.ui.p().rect(Rect::new(inner.x, sy - 12.0, inner.w, 1.0), HAIRLINE);
    let pad = Rect::new(inner.x, sy, 420.0f32.min(inner.w * 0.5), 100.0);
    l.ui.p().rounded(pad, RADIUS, FIELD);
    l.ui.p().rect(Rect::new(pad.x + 16.0, pad.bottom() - 24.0, pad.w - 32.0, 1.0), TEXT_FAINT);
    // (a signature kept - read, or drawn before the sheet was left - is drawn where the field is)
    if strokes.is_empty() && !kept.is_empty() {
        *strokes = kept.iter().map(|s| s.iter().map(|p| Vec2::new(pad.x + p[0] * pad.w, pad.y + p[1] * pad.h)).collect()).collect();
    }
    if !readonly {
        let (hover, held, _) = l.ui.interact(id_of("company-contract-signature"), pad);
        let m = l.ui.input.mouse;
        if held && pad.contains(m) {
            if l.ui.input.pressed || strokes.is_empty() {
                strokes.push(Vec::new());
            }
            if let Some(s) = strokes.last_mut() {
                if s.last().is_none_or(|p| p.distance(m) > 1.5) {
                    s.push(m);
                }
            }
        }
        if hover {
            l.ui.p().rounded_border(pad, RADIUS, 1.0, accent().alpha(0.6));
        }
    }
    for s in strokes.iter() {
        for w in s.windows(2) {
            l.ui.p().line(w[0], w[1], 2.2, TEXT);
        }
    }
    if !strokes.iter().any(|s| s.len() > 1) {
        let hint = if readonly { "Signed by name" } else { "Sign here with the mouse" };
        l.ui.text_in(hint, Rect::new(pad.x + 16.0, pad.y + 10.0, pad.w - 32.0, 40.0), 13.0, Weight::Regular, TEXT_FAINT, Align::Left);
    }
    let nx = pad.right() + 20.0;
    let nw = inner.right() - nx;
    l.ui.text_in(&omsi_ui::tr("Signed for the company").to_uppercase(), Rect::new(nx, sy, nw, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    if readonly {
        let who = if name.trim().is_empty() { "–".to_string() } else { name.clone() };
        l.ui.text_in(&who, Rect::new(nx, sy + 20.0, nw, 26.0), 18.0, Weight::Bold, TEXT, Align::Left);
        l.ui.text_in(&day_label(&dl::day_of(day)), Rect::new(nx, sy + 50.0, nw, 18.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
    } else {
        l.ui.text_input("company-contract-sign-name", Rect::new(nx, sy + 20.0, nw, ROW), name, "or type your name", None);
        if l.ui.button("company-contract-sign-clear", Rect::new(nx, sy + 20.0 + ROW + 10.0, 120.0, 32.0), "Clear", None, ButtonKind::Normal) {
            strokes.clear();
            name.clear();
        }
        l.ui.text_in(&day_label(&dl::day_of(day)), Rect::new(nx + 130.0, sy + 20.0 + ROW + 10.0, nw - 130.0, 32.0), 12.5, Weight::Regular, TEXT_DIM, Align::Right);
    }
    strokes.iter().filter(|s| s.len() > 1).map(|s| s.iter().map(|p| [((p.x - pad.x) / pad.w).clamp(0.0, 1.0), ((p.y - pad.y) / pad.h).clamp(0.0, 1.0)]).collect()).collect()
}

fn back_sheet(l: &mut Launcher, c: &Company, drive: co::dealer::TestDrive, esc: bool) -> Option<Sheet> {
    let inner = dialog_panel(l, 520.0, 250.0, "sports_score", &omsi_ui::tr("Back from the test drive"));
    let text = omsi_ui::tr("How did the %{bus} drive? The dealer is waiting for you - the test drive did not count as company time.").replace("%{bus}", &drive.name);
    l.ui.paragraph(&text, Vec2::new(inner.x, inner.y), inner.w, 13.5, Weight::Regular, TEXT_SOFT);
    let by = inner.bottom() - 38.0;
    if l.ui.button("company-dealer-back-no", Rect::new(inner.right() - 380.0, by, 140.0, 38.0), "Not now", None, ButtonKind::Normal) || esc {
        return None;
    }
    if l.ui.button("company-dealer-back-yes", Rect::new(inner.right() - 230.0, by, 230.0, 38.0), "Back to the bus", Some("directions_bus"), ButtonKind::Primary) {
        let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
        let quick = c.dealer.mode == BuyingMode::Simple;
        let liveries = liveries_of(l, &drive.bus);
        let livery = liveries.iter().position(|p| *p == drive.livery).unwrap_or(0);
        if let Some(o) = drive.offer.as_deref().and_then(|id| dl::offer(c, &listings, id, &dl::now_of(c))) {
            return Some(Sheet::Offer { offer: o, livery, count: 1.0, pay: 0, quick });
        }
        if let Some(x) = listings.iter().find(|x| x.bus.file == drive.bus) {
            return Some(Sheet::Model { listing: x.clone(), livery, count: 1.0, pay: 0, quick });
        }
        l.state.set_status("This offer has ended or is sold.", true);
        return None;
    }
    Some(Sheet::Back { drive })
}

/// Buses joined the fleet: say so.
pub(super) fn joined(l: &mut Launcher, ids: &[u32]) {
    let numbers: Vec<String> = ids.iter().filter_map(|id| l.company.company.as_ref().and_then(|c| c.vehicle(*id)).map(|v| v.number.clone())).collect();
    let text = if numbers.len() == 1 { omsi_ui::tr("Bus %{n} joined the fleet.").replace("%{n}", &numbers[0]) } else { omsi_ui::tr("Buses %{n} joined the fleet.").replace("%{n}", &numbers.join(", ")) };
    l.state.set_status(text, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_euros_are_read() {
        assert_eq!(typed_euros("245000"), Some(245_000_00));
        assert_eq!(typed_euros("€ 245.000"), Some(245_000_00));
        assert_eq!(typed_euros("€245,000"), Some(245_000_00));
        assert_eq!(typed_euros("245000,50"), Some(245_000_00));
        assert_eq!(typed_euros("abc"), None);
    }

    #[test]
    fn the_filters_say_what_they_ask() {
        let v = DealerView { size: 3, drive: 2, avail: 1, max_price: 400.0, year: 2001.0, ..Default::default() };
        let f = v.filter();
        assert_eq!((f.size, f.drive, f.new, f.max_price, f.year), (Some(BusSize::Articulated), Some(Drive::Electric), Some(true), 400_000_00, 2001));
        let any = DealerView { max_price: PRICE_ANY, year: YEAR_ANY, ..Default::default() }.filter();
        assert_eq!((any.max_price, any.year, any.size), (0, 0, None));
        assert_eq!(years_label(Some(Years { from: 1984, to: Some(1992) })), "1984–1992");
        assert_eq!(years_label(None), "");
    }
}

//! The dealer and the bank (`crates/omsi-launcher-ui/src/company/dealer.rs`, `bank.rs`,
//! `adverts.rs`' contract): the showroom by maker, model and version with its filters, the day's
//! special offers, the used market, the orders and signed contracts; a bus's sheet with the quick
//! buy and the talk with the dealer, the contract signed with a drawn signature or a typed name;
//! a loan's sheet and contract, paying a loan back; an advertising contract. The sheet open lives
//! here, in the session, as the game-drawn launcher keeps it; the page draws what `sheet_view`
//! says and hands back what was clicked or typed.

use super::kit::{self, Popup};
use super::words::{day_label, eur, grade, grouped, num, share_tone, tr, trf};
use super::{act as apply, refuse, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::adverts::{self as ad, AdContract};
use omsi_launcher_lib::company::dealer::{self as dl, BuyingMode, Contract, Extra, Filter, Listing, Move, Offer, OfferKind, PayWay, Quote, Reply, Signed, Talk};
use omsi_launcher_lib::company::finance::{self as fi, LoanContract, Room};
use omsi_launcher_lib::company::market::Payment;
use omsi_launcher_lib::company::vehicles::Category;
use omsi_launcher_lib::company::{self as co, Company, Drive};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// A sheet over the dealer's pages.
#[derive(Clone)]
pub enum Sheet {
    Model { listing: Listing, livery: String, count: u32, pay: usize, quick: bool },
    Offer { offer: Offer, livery: String, count: u32, pay: usize, quick: bool },
    Talk { talk: Talk, listing: Listing, offer: Option<Offer>, livery: String, said: Vec<(bool, String)> },
    Contract { contract: Contract, readonly: bool },
    Back { drive: co::dealer::TestDrive },
    Loan { amount: i64, term: usize, purpose: String, collateral: i64, fixed: bool, then: Then },
    LoanContract { contract: LoanContract, readonly: bool, collateral: i64, then: Then },
    Repay { id: u32, amount: i64 },
    Advert { contract: AdContract, readonly: bool },
}

/// What a loan pays for, once its contract is signed.
#[derive(Clone)]
pub enum Then {
    Nothing,
    Purchase(Box<Contract>),
    Quick { listing: Listing, count: u32, livery: String },
    QuickOffer { offer: Offer, count: u32, livery: String },
}

/// The dealer as the session keeps it: its showroom read, its clock, the sheet open.
#[derive(Default)]
pub struct DealerState {
    pub listings: Option<Arc<Vec<Listing>>>,
    listings_key: Option<(usize, usize)>,
    busy: bool,
    ticked: Option<(String, String)>,
    pub sheet: Option<Sheet>,
    /// Cancel pressed once on a contract with a signature: pressed again it throws it away.
    discard_armed: bool,
}

// --- the catalogue and the dealer's clock ------------------------------------------------------

/// Natural order: DL9 before DL10, case not counting (`buspick::name_cmp`).
pub fn name_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let o = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The model and the version of a bus (`buspick::split_type`).
fn split_type(type_name: &str, file: &str, default_paint: &str) -> (String, String) {
    let label = core::vehicle_type_label(type_name, Path::new(file));
    let parts: Vec<&str> = label.split(" - ").map(str::trim).filter(|s| !s.is_empty()).collect();
    let model = parts.first().map(|s| s.to_string()).unwrap_or_else(|| label.clone());
    let version = if parts.len() > 1 {
        parts[1..].join(" · ")
    } else if !default_paint.trim().is_empty() {
        core::display_bus_name(default_paint)
    } else {
        core::display_bus_name(&Path::new(file).file_stem().unwrap_or_default().to_string_lossy())
    };
    (model, version)
}

/// The buses' families as the bus step's tree has them (`buspick::build_tree`): maker, model and
/// version, two files of one version told apart by their pack, then their file.
fn families(vehicles: &[core::VehicleInfo]) -> HashMap<String, (String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = vehicles
        .iter()
        .map(|v| {
            let maker = v.manufacturer.trim();
            let group = if maker.is_empty() { core::display_bus_name(&v.folder) } else { core::display_bus_name(maker) };
            let (model, version) = split_type(&v.type_name, &v.file, &v.default_paint);
            (v.file.clone(), group, model, version)
        })
        .collect();
    let mut by: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (k, x) in out.iter().enumerate() {
        by.entry((x.1.to_lowercase(), x.2.to_lowercase())).or_default().push(k);
    }
    for buses in by.values() {
        for pass in 0..2 {
            let mut seen: HashMap<String, usize> = HashMap::new();
            for &b in buses {
                *seen.entry(out[b].3.to_lowercase()).or_default() += 1;
            }
            for &b in buses {
                if seen[&out[b].3.to_lowercase()] > 1 {
                    let f = out[b].0.replace('\\', "/");
                    let extra = if pass == 0 { f.split('/').nth(1).unwrap_or_default().to_string() } else { Path::new(&f).file_stem().unwrap_or_default().to_string_lossy().into_owned() };
                    out[b].3 = format!("{} · {}", out[b].3, core::display_bus_name(&extra));
                }
            }
        }
    }
    out.into_iter().map(|x| (x.0, (x.1, x.2, x.3))).collect()
}

/// The showroom's buses: the market's with their families and cabins, read on a thread once
/// the market is read (`dealer::ask_listings`).
pub fn listings(s: &mut Session) -> Option<Arc<Vec<Listing>>> {
    s.ask_market();
    let market = s.market.clone()?;
    let vehicles = s.vehicles();
    let key = (market.len(), vehicles.len());
    if s.dealer.listings_key != Some(key) && !s.dealer.busy {
        s.dealer.listings_key = Some(key);
        s.dealer.busy = true;
        std::thread::spawn(move || {
            let fam = families(&vehicles);
            // (each bus's cabin read on several threads: a few hundred bus files)
            let list: Vec<Listing> = super::par_map(&market, |b| {
                let v = vehicles.iter().find(|v| v.file == b.file)?;
                let family = fam.get(&b.file).cloned().unwrap_or_else(|| (v.manufacturer.clone(), b.name.clone(), String::new()));
                Some(dl::listing_of(b.clone(), family, &dl::read_specs(&b.file)))
            })
            .into_iter()
            .flatten()
            .collect();
            let mut s = super::session().lock().unwrap_or_else(|e| e.into_inner());
            s.dealer.listings = Some(Arc::new(list));
            s.dealer.busy = false;
            s.generation += 1;
        });
    }
    let raw = s.dealer.listings.clone()?;
    // (Beta 54: the categories and regions as the player corrected them, and only the models
    // the company works with - the showroom, the day's offers, the used market, the shop)
    let co = super::vehicles::corrections(s).clone();
    let choice = s.company.as_ref().map(|c| c.vehicles.clone());
    Some(Arc::new(
        raw.iter()
            .filter_map(|l| {
                let mut l = l.clone();
                co.apply(&mut l.bus);
                choice.as_ref().is_none_or(|v| v.allows(&l.bus)).then_some(l)
            })
            .collect(),
    ))
}

/// The dealer's clock (`dealer::tick`): the orders due delivered, what is over forgotten - once a
/// company day, and whenever an order is due; and a test drive over (the game ended) asks how it
/// went.
pub fn tick(s: &mut Session) {
    if s.company.is_none() || s.closing {
        return;
    }
    let _ = listings(s);
    let Some(c) = s.company.as_ref() else { return };
    let now = dl::now_of(c);
    let key = (c.id.clone(), c.date.clone());
    let due = c.dealer.orders.iter().any(|o| dl::minutes_of(&o.delivery) <= dl::minutes_of(&now));
    if due || s.dealer.ticked.as_ref() != Some(&key) {
        s.dealer.ticked = Some(key);
        if let Ok(done) = apply(s, |c| Ok(dl::tick(c, &now))) {
            for d in done {
                let mut text = trf("Contract %{no}: %{bus} delivered - fleet numbers %{numbers}.", &[("no", &d.contract.to_string()), ("bus", &d.name), ("numbers", &d.numbers.join(", "))]);
                if let Some(note) = d.ids.first().and_then(|id| s.company.as_ref().and_then(|c| super::people::drivers_note(c, *id))) {
                    text = format!("{text} {note}");
                }
                s.say(text, false);
            }
        }
    }
    // a test drive over: the dealer asks how it went
    let Some(drive) = s.company.as_ref().and_then(|c| c.dealer.test_drive.clone()) else { return };
    if s.in_game() || s.dealer.sheet.is_some() || s.reports.is_some() {
        return;
    }
    let _ = apply(s, |c| Ok(dl::end_test_drive(c)));
    open(s, Sheet::Back { drive });
}

/// Open a sheet over the pages.
pub fn open(s: &mut Session, sheet: Sheet) {
    s.dealer.sheet = Some(sheet);
    s.dealer.discard_armed = false;
    s.generation += 1;
}

// --- words ---------------------------------------------------------------------------------------

/// "one model", "%{count} models".
fn count_text(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        tr(one)
    } else {
        tr(many).replace("%{count}", &n.to_string())
    }
}

fn version_of(l: &Listing) -> String {
    if l.version.trim().is_empty() {
        l.bus.name.clone()
    } else {
        l.version.clone()
    }
}

fn places(l: &Listing) -> String {
    match (l.seats, l.standing) {
        (Some(s), Some(t)) if s + t > 0 => trf("%{seats} seats, %{standing} standing", &[("seats", &s.to_string()), ("standing", &t.to_string())]),
        _ => "–".to_string(),
    }
}

fn offer_line(o: &Offer) -> String {
    if o.is_new() {
        tr("New, from stock")
    } else {
        let plate = if o.registration.is_empty() { String::new() } else { format!("{}  ·  ", o.registration) };
        format!("{plate}{} {}  ·  {} km  ·  {} {:.0}", tr("built"), o.built.get(..4).unwrap_or(""), grouped(o.km), tr("condition"), o.condition)
    }
}

fn reply_text(r: &Reply) -> String {
    match r {
        Reply::Accepted(p) => trf("Agreed: %{amount} a bus.", &[("amount", &eur(*p))]),
        Reply::Discount(p) => trf("Let me see... I can do %{amount}.", &[("amount", &eur(*p))]),
        Reply::Counter(p) => trf("Not quite. Let us meet at %{amount}.", &[("amount", &eur(*p))]),
        Reply::Firm => tr("That is a fair price already. I cannot go lower."),
        Reply::TooLow(p) => trf("That is no offer. I could do %{amount}, no less.", &[("amount", &eur(*p))]),
        Reply::ExtraGranted(e) => trf("%{extra}: agreed.", &[("extra", &tr(e.label()))]),
        Reply::ExtraRefused(e) => trf("%{extra}? Not at this price.", &[("extra", &tr(e.label()))]),
        Reply::LastOffer(p) => trf("My last word: %{amount}. Take it or leave it.", &[("amount", &eur(*p))]),
        Reply::BrokeOff(until) => trf("That is enough. Come back after %{date}.", &[("date", &day_label(&dl::day_of(until)))]),
    }
}

fn move_text(m: &Move) -> String {
    match m {
        Move::AskDiscount => tr("Can you do something on the price?"),
        Move::Offer(p) => trf("I offer %{amount} a bus.", &[("amount", &eur(*p))]),
        Move::AskExtra(e) => trf("Could you include this: %{extra}?", &[("extra", &tr(e.label()))]),
        Move::Accept => tr("Agreed."),
    }
}

/// The cheapest new price of some listings ("from €280,000").
fn price_of(c: &Company, ls: &[&Listing], one: bool) -> String {
    let new: Vec<i64> = ls.iter().map(|l| dl::list_price(c, l.bus.kind)).collect();
    match new.iter().min() {
        Some(p) if one || new.iter().all(|x| x == p) => eur(*p),
        Some(p) => trf("from %{amount}", &[("amount", &eur(*p))]),
        None => "–".into(),
    }
}

fn locked_tag(c: &Company, l: &Listing) -> Option<(String, &'static str)> {
    co::market::kind_allowed(c, l.bus.kind).err().map(|e| (tr(if e.contains("research") { "Research needed" } else { "Higher level" }), "soft"))
}

/// A bus's paint names: its own first (empty), then its liveries (`fleet::liveries_of`).
pub fn liveries_of(s: &mut Session, bus: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    if let Some(v) = s.vehicles().iter().find(|v| v.file == bus) {
        out.extend(v.paints.iter().filter(|p| !p.eq_ignore_ascii_case(&v.default_paint)).cloned());
    }
    out
}

pub fn livery_label(p: &str) -> String {
    if p.is_empty() {
        tr("Its own livery")
    } else {
        p.to_string()
    }
}

/// The livery choice of a sheet: the bus's paints, the company's own marked.
pub fn livery_choice(s: &mut Session, bus: &str, chosen: &str) -> Value {
    let list = liveries_of(s, bus);
    let c = s.company.clone();
    let options: Vec<Value> = list
        .iter()
        .map(|p| {
            let ours = !p.is_empty() && c.as_ref().is_some_and(|c| co::livery::design(c, p).is_some());
            json!([p, if ours { format!("{}  ·  {}", livery_label(p), tr("our livery")) } else { livery_label(p) }])
        })
        .collect();
    json!({ "label": tr("Livery"), "options": options, "value": if list.iter().any(|p| p == chosen) { chosen } else { "" } })
}

/// A row of a price table: what, and the amount.
pub fn row(a: impl Into<String>, b: impl Into<String>, strong: bool) -> Value {
    json!({ "a": a.into(), "b": b.into(), "strong": strong })
}

fn toned(a: impl Into<String>, b: impl Into<String>, tone: Option<&str>) -> Value {
    json!({ "a": a.into(), "b": b.into(), "tone": tone })
}

// --- the page --------------------------------------------------------------------------------------

const PRICE_ANY: f64 = 1000.0;

/// The showroom's filters as the page hands them: search, size (0 all .. 4), drive (0 all .. 2),
/// the highest price in thousands (at its end: any).
fn filter_of(ui: &Value) -> Filter {
    let size = ui.get("size").and_then(Value::as_u64).unwrap_or(0);
    let drive = ui.get("drive").and_then(Value::as_u64).unwrap_or(0);
    let max = ui.get("max_price").and_then(Value::as_f64).unwrap_or(PRICE_ANY);
    Filter {
        search: ui.get("search").and_then(Value::as_str).unwrap_or("").to_string(),
        // (by category: a coach is no solo bus - Beta 54)
        size: None,
        category: match size {
            1 => Some(Category::Midi),
            2 => Some(Category::Solo),
            3 => Some(Category::Articulated),
            4 => Some(Category::Double),
            5 => Some(Category::Coach),
            _ => None,
        },
        drive: match drive {
            1 => Some(Drive::Diesel),
            2 => Some(Drive::Electric),
            _ => None,
        },
        max_price: if max <= 0.0 || max >= PRICE_ANY { 0 } else { (max as i64) * 1000_00 },
    }
}

/// The dealer's page (`dealer::draw`): its tabs with their counts, the buying mode, and the tab
/// shown - the showroom at its level (`level`: [] the makers, [maker], [maker, model]), the day's
/// offers, the used market, the orders and contracts.
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let listings = listings(s);
    let all: Vec<Listing> = listings.as_deref().cloned().unwrap_or_default();
    let now = dl::now_of(&c);
    let offers = dl::day_offers(&c, &all, &now);
    let used = dl::used_market(&c, &all, &now);
    let tab = ui.get("tab").and_then(Value::as_u64).unwrap_or(0);
    let tabs = vec![tr("Showroom"), trf("Today's offers (%{n})", &[("n", &offers.len().to_string())]), trf("Used (%{n})", &[("n", &used.len().to_string())]), trf("Orders (%{n})", &[("n", &c.dealer.orders.len().to_string())])];
    let modes: Vec<String> = BuyingMode::ALL.iter().map(|m| tr(m.label())).collect();
    let mode = BuyingMode::ALL.iter().position(|m| *m == c.dealer.mode).unwrap_or(1);
    let mut out = json!({
        "tabs": tabs, "tab": tab,
        "buying": (!co::autopilot::on(&c)).then(|| json!({ "label": tr("Buying").to_uppercase(), "modes": modes, "mode": mode, "tip": tr("Simple: buy at the list price in one click. Advanced: haggle with the dealer and sign a contract. Both stay at hand on every bus.") })),
    });
    if listings.is_none() {
        out["reading"] = json!(tr(if s.market_busy() || s.dealer.busy { "Reading the installed buses…" } else { "No buses found in the OMSI folder." }));
        return out;
    }
    out["body"] = match tab {
        1 => offer_grid(&c, &offers, "The day's special offers: new buses from stock at a discount, demonstrators, and fleets other operators sell. New ones come every company day.", "No special offers today. Come back tomorrow."),
        2 => offer_grid(&c, &used, "The used market changes every day: each bus stays four days or until it is sold.", "Nothing on the used market today."),
        3 => orders(&c),
        _ => showroom(&c, &all, &offers, ui),
    };
    out
}

fn showroom(c: &Company, all: &[Listing], offers: &[Offer], ui: &Value) -> Value {
    let filter = filter_of(ui);
    let mut fit: Vec<&Listing> = all.iter().filter(|x| filter.fits(x, c)).collect();
    fit.sort_by(|a, b| name_cmp(&a.maker, &b.maker).then_with(|| name_cmp(&a.model, &b.model)).then_with(|| name_cmp(&version_of(a), &version_of(b))));
    let searching = !filter.search.trim().is_empty();
    let level: Vec<String> = ui.get("level").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut crumbs = vec![tr("All brands")];
    if !searching {
        crumbs.extend(level.iter().cloned());
    } else {
        crumbs.push(tr("Search"));
    }
    let offered: HashSet<String> = offers.iter().map(|o| o.listing.bus.file.clone()).collect();
    let mut cards: Vec<Value> = Vec::new();
    let special = || json!([tr("Special offer"), "accent"]);
    if searching || level.len() >= 2 {
        let (m, model) = (level.first().cloned().unwrap_or_default(), level.get(1).cloned().unwrap_or_default());
        for x in fit.iter().filter(|x| searching || (x.maker == m && x.model == model)) {
            let badge = if offered.contains(&x.bus.file) { Some(special()) } else { locked_tag(c, x).map(|(t, tone)| json!([t, tone])) };
            cards.push(json!({ "key": format!("v-{}", x.bus.file), "title": if searching { x.bus.name.clone() } else { version_of(x) }, "sub": super::vehicles::kind_label(&x.bus), "price": price_of(c, &[*x], true), "badge": badge, "file": x.bus.file, "name": x.bus.name, "to": { "bus": x.bus.file } }));
        }
    } else if let Some(m) = level.first() {
        let ls: Vec<&Listing> = fit.iter().copied().filter(|x| &x.maker == m).collect();
        let mut models: Vec<String> = Vec::new();
        for x in &ls {
            if !models.contains(&x.model) {
                models.push(x.model.clone());
            }
        }
        for model in models {
            let vs: Vec<&Listing> = ls.iter().copied().filter(|x| x.model == model).collect();
            let badge = if vs.iter().any(|x| offered.contains(&x.bus.file)) {
                Some(special())
            } else if vs.iter().all(|x| locked_tag(c, x).is_some()) {
                locked_tag(c, vs[0]).map(|(t, tone)| json!([t, tone]))
            } else {
                None
            };
            cards.push(json!({ "key": format!("md-{m}-{model}"), "title": model, "sub": count_text(vs.len(), "one version", "%{count} versions"), "price": price_of(c, &vs, false), "badge": badge, "file": vs[0].bus.file, "name": model, "to": { "level": [m, model] } }));
        }
    } else {
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
            let deals = ls.iter().filter(|x| offered.contains(&x.bus.file)).count();
            let standing = dl::Standing::of(dl::relation(c, &m).points);
            let badge = if deals > 0 { Some(special()) } else { (standing > dl::Standing::New).then(|| json!([tr(standing.label()), "ok"])) };
            cards.push(json!({ "key": format!("mk-{m}"), "title": m, "sub": count_text(models.len(), "one model", "%{count} models"), "price": price_of(c, &ls, false), "badge": badge, "file": ls[0].bus.file, "name": m, "to": { "level": [m] } }));
        }
    }
    json!({
        "kind": "showroom", "crumbs": crumbs, "searching": searching,
        "count": count_text(fit.len(), "one bus fits", "%{count} buses fit"), "cards": cards,
        "empty": tr("No bus fits: loosen the filters, or look at the used market."),
        "filters": {
            "search": tr("Search a bus"),
            "size_label": tr("Size"), "sizes": (["All", "Midibus", "Solo", "Articulated", "Double-decker", "Coach"].iter().map(|x| tr(x)).collect::<Vec<_>>()),
            "drive_label": tr("Drive"), "drives": (["All", "Diesel", "Electric"].iter().map(|x| tr(x)).collect::<Vec<_>>()),
            "price_label": tr("Price up to"), "any": tr("Any"),
        },
    })
}

fn offer_grid(c: &Company, offers: &[Offer], note: &str, empty: &str) -> Value {
    let cards: Vec<Value> = offers
        .iter()
        .map(|o| {
            let until = trf("until %{date}", &[("date", &day_label(&dl::day_of(&o.expires)))]);
            let who = if o.count > 1 { format!("{}  ·  {} × ", o.seller, o.count) } else { format!("{}  ·  ", o.seller) };
            let save = o.saving();
            json!({
                "id": o.id, "file": o.listing.bus.file, "name": o.listing.bus.name,
                "badge": (o.kind != OfferKind::Used).then(|| tr(o.kind.label())),
                "line": offer_line(o), "price": eur(o.price),
                "saving": (save >= 0.01 && o.kind != OfferKind::Used).then(|| format!("−{:.0} %", save * 100.0)),
                "condition": (o.kind == OfferKind::Used || o.kind == OfferKind::Batch).then(|| json!({ "frac": o.condition / 100.0, "tone": grade(o.condition), "tip": trf("Condition: %{n} of 100", &[("n", &format!("{:.0}", o.condition))]) })),
                "who": format!("{who}{until}"),
            })
        })
        .collect();
    let _ = c;
    json!({ "kind": "offers", "note": tr(note), "empty": tr(empty), "cards": cards })
}

fn orders(c: &Company) -> Value {
    let on_order: Vec<Value> = c
        .dealer
        .orders
        .iter()
        .map(|o| {
            let k = &o.contract;
            let days = co::dates::between(&c.date, &dl::day_of(&o.delivery)).max(0);
            json!({ "what": format!("{} × {}", k.count, k.listing.bus.name), "no": trf("Contract %{no}", &[("no", &k.no.to_string())]), "when": trf("Arrives %{date} (in %{n} days)", &[("date", &day_label(&dl::day_of(&o.delivery))), ("n", &days.to_string())]) })
        })
        .collect();
    let contracts: Vec<Value> = c
        .dealer
        .contracts
        .iter()
        .rev()
        .map(|k| {
            let amount = match k.pay {
                PayWay::Lease => tr("Leasing"),
                PayWay::Loan => format!("{}  ·  {}", eur(k.total()), tr("Bank loan")),
                PayWay::Cash => eur(k.total()),
            };
            json!({ "no": k.no, "date": day_label(&dl::day_of(&k.signed_at)), "what": format!("{} × {}", k.count, k.listing.bus.name), "seller": k.seller, "amount": amount })
        })
        .collect();
    json!({
        "kind": "orders", "on_order_title": tr("On order"), "on_order": on_order,
        "none": tr("Nothing on order. A new bus that is not in stock comes some days after its contract is signed."),
        "contracts_title": tr("Signed contracts"), "contracts": contracts, "no_contracts": tr("No contracts signed yet."), "open_tip": tr("Open the contract"),
    })
}

// --- the sheets: what they show ----------------------------------------------------------------------

/// The company's standing with `maker`'s dealer as a line (`dealer::standing_line`).
fn standing_line(c: &Company, maker: &str) -> Value {
    let rel = dl::relation(c, maker);
    let st = dl::Standing::of(rel.points);
    let mut t = trf("Your standing with the %{maker} dealer: %{standing}", &[("maker", maker), ("standing", &tr(st.label()))]);
    if rel.bought > 0 {
        t.push_str("  ·  ");
        t.push_str(&count_text(rel.bought as usize, "one bus bought of him", "%{count} buses bought of him"));
    }
    if st.room() > 0.0 {
        t.push_str("  ·  ");
        t.push_str(&trf("%{n} % more room for a discount", &[("n", &num(st.room() * 100.0, 1))]));
    }
    if let Some(next) = st.next_at() {
        let buses = ((next - rel.points) / 10.0).ceil().max(1.0) as usize;
        let after = tr(dl::Standing::of(next).label());
        t.push_str("  ·  ");
        t.push_str(&count_text(buses, "one new bus more to %{next}", "%{count} new buses more to %{next}").replace("%{next}", &after));
    }
    json!({ "icon": "star", "text": t, "tone": if st > dl::Standing::New { "ok" } else { "soft" } })
}

/// The step of an order of `count` buses as a line (`dealer::bulk_line`).
fn bulk_line(count: u32) -> Value {
    let (step, next) = dl::bulk_step(count);
    let mut t = tr(step);
    if let Some(n) = next {
        t.push_str("  ·  ");
        t.push_str(&trf("more off from %{n} buses", &[("n", &n.to_string())]));
    }
    json!({ "icon": "inventory_2", "text": t, "tone": if count >= 5 { "ok" } else { "soft" } })
}

/// A paid amount's line under a quick buy: the cash afterwards, or the bank's rate; and whether
/// the money is there.
fn pay_line(c: &Company, amount: i64, pay: usize) -> (Value, bool) {
    if pay == 0 {
        (row(tr("Cash afterwards"), eur(c.cash - amount), false), c.cash >= amount)
    } else {
        let (monthly, months, rate) = fi::loan_terms(c, amount);
        let terms = trf("%{n} months at %{rate} %", &[("n", &months.to_string()), ("rate", &num(rate * 100.0, 1))]);
        (row(tr("Monthly rate"), format!("{}  ·  {}", eur(monthly), terms), false), fi::credit_left(c, amount) >= amount)
    }
}

/// "%{rate} %" with one decimal (`bank::percent`).
pub fn percent(rate: f64) -> String {
    let v = rate * 100.0;
    if (v - v.round()).abs() < 0.05 {
        format!("{} %", num(v, 0))
    } else {
        format!("{} %", num(v, 1))
    }
}

pub fn room_tone(r: Room) -> &'static str {
    match r {
        Room::Plenty => "ok",
        Room::Little => "warn",
        Room::None => "danger",
    }
}

/// The contract's parties (`dealer::parties`).
fn parties(a: (&str, &str, &str), b: (&str, &str, &str)) -> Value {
    json!([{ "role": tr(a.0).to_uppercase(), "name": a.1, "under": a.2 }, { "role": tr(b.0).to_uppercase(), "name": b.1, "under": b.2 }])
}

/// The signature's field (`dealer::signature`): what is kept, and its words.
fn signature(strokes: &[Vec<[f32; 2]>], name: &str, readonly: bool, day: &str) -> Value {
    json!({
        "strokes": strokes, "name": name, "readonly": readonly, "day": day_label(&dl::day_of(day)),
        "hint": tr(if readonly { "Signed by name" } else { "Sign here with the mouse" }),
        "caps": tr("Signed for the company").to_uppercase(), "placeholder": tr("or type your name"), "clear": tr("Clear"),
    })
}

fn home_of(c: &Company) -> String {
    if c.map_name.is_empty() {
        c.depot.clone()
    } else {
        format!("{}  ·  {}", c.map_name, c.depot)
    }
}

/// The open sheet as the page draws it.
pub fn sheet_view(s: &mut Session, _ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let Some(sheet) = s.dealer.sheet.clone() else { return Value::Null };
    let armed = s.dealer.discard_armed;
    match sheet {
        Sheet::Model { listing, livery, count, pay, quick } => {
            let r = co::economy::rules(c.game_mode);
            let list = dl::list_price(&c, listing.bus.kind);
            let grant = co::economy::grant(listing.bus.kind, list, &r, c.price_index);
            let days = dl::delivery_days(&c, &format!("new:{}", listing.bus.file), false, false);
            let mut rows = vec![
                row(tr("Maker"), listing.maker.clone(), false),
                row(tr("Model"), if listing.version.is_empty() { listing.model.clone() } else { format!("{}  ·  {}", listing.model, listing.version) }, false),
                row(tr("Kind"), super::vehicles::kind_label(&listing.bus), false),
                row(tr("Places"), places(&listing), false),
                row(tr("List price"), eur(list), false),
            ];
            if grant > 0 {
                rows.push(row(tr("Grant"), format!("- {}", eur(grant)), false));
            }
            rows.push(row(tr("Delivery"), trf("in %{n} days, or at once with a quick delivery", &[("n", &days.to_string())]), false));
            let allowed = co::market::kind_allowed(&c, listing.bus.kind);
            let lock = allowed.err().map(|e| kit::refusal(&c, e).text);
            let mut v = json!({
                "kind": "model", "icon": "directions_bus", "title": listing.bus.name, "file": listing.bus.file, "name": listing.bus.name, "bus_kind": super::vehicles::kind_label(&listing.bus),
                "livery": livery_choice(s, &listing.bus.file, &livery), "design_note": tr("A house livery of your own comes with the livery studio."),
                "rows": rows, "lock": lock, "quick": quick, "count": count, "max": 10, "pay": pay,
                "count_label": tr("Buses"), "pays": [tr("Cash"), tr("Bank loan")],
                "standing": standing_line(&c, &listing.maker), "bulk": bulk_line(count),
                "close": tr("Close"), "test": tr("Test drive"), "test_tip": tr("Drive it on your map from your depot: a free drive outside the company's time. Back here when the game ends."), "rent": tr("Lease or rent"),
            });
            if quick {
                let n = count as i64;
                let painting = if livery.is_empty() { 0 } else { dl::painting_cost(&c) };
                let price = dl::quick_price(&c, &listing, n as u32);
                let mut extra = Vec::new();
                if price < list {
                    let off = 1.0 - price as f64 / list as f64;
                    extra.push(row(trf("The dealer's discount (%{n} %)", &[("n", &num(off * 100.0, 1))]), format!("{} × − {}", n, eur(list - price)), false));
                }
                let total = n * (price + painting - grant);
                if painting > 0 {
                    extra.push(row(tr("Painting in the livery chosen"), format!("{} × {}", n, eur(painting)), false));
                }
                extra.push(row(tr("You pay"), eur(total), true));
                let (pl, _) = pay_line(&c, total, pay);
                extra.push(pl);
                v["price_rows"] = json!(extra);
                v["main"] = json!(trf("Buy %{n} for %{amount}", &[("n", &n.to_string()), ("amount", &eur(total))]));
                v["other"] = json!(tr("Haggle instead"));
            } else {
                let now = dl::now_of(&c);
                v["talk_note"] = json!(tr("The list price is where the talk begins: ask for a discount, make an offer, ask for extras. Nothing is booked until you sign the contract."));
                v["sulk"] = json!(dl::sulking(&c, &listing.maker, &now).map(|until| trf("The dealer does not want to talk to you until %{date}.", &[("date", &day_label(&dl::day_of(&until)))])));
                v["main"] = json!(tr("Talk to the dealer"));
                v["other"] = json!(tr("Quick buy instead"));
            }
            v
        }
        Sheet::Offer { offer, livery, count, pay, quick } => {
            let mut rows = vec![row(tr("Seller"), offer.seller.clone(), false), row(tr("Kind"), super::vehicles::kind_label(&offer.listing.bus), false), row(tr("Places"), places(&offer.listing), false)];
            if offer.is_new() {
                rows.push(row(tr("Delivery"), tr("from stock: two days after signing"), false));
            } else {
                if !offer.registration.is_empty() {
                    rows.push(row(tr("Plate"), offer.registration.clone(), false));
                }
                rows.extend(super::age::offer_row(&c, &offer));
                rows.push(row(tr("Kilometres"), format!("{} km", grouped(offer.km)), false));
                rows.push(row(tr("Condition"), format!("{:.0} / 100", offer.condition), false));
            }
            if offer.count > 1 {
                rows.push(row(tr("On offer"), count_text(offer.count as usize, "one bus", "%{count} buses"), false));
            }
            let otherwise = if offer.is_new() || offer.kind == OfferKind::Demonstrator { "List price" } else { "Book value" };
            rows.push(row(tr(otherwise), eur(offer.reference), false));
            rows.push(row(tr("Price a bus"), if offer.saving() >= 0.01 { format!("{}  (−{:.0} %)", eur(offer.price), offer.saving() * 100.0) } else { eur(offer.price) }, false));
            rows.push(row(tr("Offer ends"), format!("{}  {}", day_label(&dl::day_of(&offer.expires)), offer.expires.get(11..).unwrap_or("")), false));
            let allowed = co::market::kind_allowed(&c, offer.listing.bus.kind);
            let lock = allowed.err().map(|e| kit::refusal(&c, e).text);
            let mut v = json!({
                "kind": "offer", "icon": "stars", "title": format!("{}  ·  {}", tr(offer.kind.label()), offer.listing.bus.name), "file": offer.listing.bus.file, "name": offer.listing.bus.name, "bus_kind": super::vehicles::kind_label(&offer.listing.bus),
                "livery": livery_choice(s, &offer.listing.bus.file, &livery), "design_note": tr("A house livery of your own comes with the livery studio."),
                "rows": rows, "lock": lock, "quick": quick, "count": count, "max": offer.count, "pay": pay, "age": super::age::offer_switch(&offer),
                "count_label": tr("Buses"), "pays": [tr("Cash"), tr("Bank loan")],
                "close": tr("Close"), "test": tr("Test drive"), "test_tip": tr("Drive it on your map from your depot: a free drive outside the company's time. Back here when the game ends."),
            });
            if quick {
                let n = count as i64;
                let painting = if livery.is_empty() { 0 } else { dl::painting_cost(&c) };
                let r = co::economy::rules(c.game_mode);
                let grant = if offer.is_new() { co::economy::grant(offer.listing.bus.kind, offer.price, &r, c.price_index) } else { 0 };
                let total = n * (offer.price + painting - grant);
                let (pl, _) = pay_line(&c, total, pay);
                v["price_rows"] = json!([row(tr("You pay"), eur(total), true), pl]);
                v["main"] = json!(trf("Buy %{n} for %{amount}", &[("n", &n.to_string()), ("amount", &eur(total))]));
                v["other"] = json!(tr("Haggle instead"));
            } else {
                let now = dl::now_of(&c);
                let sulk = dl::sulking(&c, &offer.listing.maker, &now).filter(|_| offer.kind != OfferKind::Used && offer.kind != OfferKind::Batch);
                v["sulk"] = json!(sulk.map(|until| trf("The dealer does not want to talk to you until %{date}.", &[("date", &day_label(&dl::day_of(&until)))])));
                v["main"] = json!(tr("Talk to the seller"));
                v["other"] = json!(tr("Quick buy instead"));
            }
            v
        }
        Sheet::Talk { talk, listing, offer, livery, said } => {
            let seller = offer.as_ref().map(|o| o.seller.clone()).unwrap_or_else(|| dl::dealer_name(&listing.maker));
            let what = if talk.quote.count > 1 { format!("{} × {}", talk.quote.count, listing.bus.name) } else { listing.bus.name.clone() };
            let off = talk.discount();
            let left_n = talk.rounds_left();
            let rf = left_n as f64 / talk.max_rounds.max(1) as f64;
            let start = dl::terms(c.game_mode).patience.max(0.1);
            let mood = (talk.patience / start).clamp(0.0, 1.0);
            let mood_word = if mood >= 0.65 { tr("calm") } else if mood >= 0.35 { tr("getting short") } else { tr("nearly out") };
            let lines: Vec<(bool, String)> = if said.is_empty() { vec![(false, trf("Good day. This one is %{amount}. What can I do for you?", &[("amount", &eur(talk.quote.list))]))] } else { said.clone() };
            let extras: Vec<Value> = Extra::ALL
                .iter()
                .copied()
                .filter(|e| !(*e == Extra::FastDelivery && talk.quote.used))
                .map(|e| {
                    let have = talk.extras.contains(&e);
                    json!({ "key": format!("{e:?}"), "label": tr(e.label()), "icon": if have { "check" } else { e.icon() }, "have": have, "tip": if have { tr("Agreed: it goes into the contract") } else { trf("Ask for it: worth about %{amount} a bus to the dealer", &[("amount", &eur(dl::extra_value(&c, e, talk.quote.list)))]) } })
                })
                .collect();
            let broke = matches!(talk.replies.last(), Some(Reply::BrokeOff(_)));
            let closed_text = (!(!talk.closed)).then(|| {
                tr(match talk.replies.last() {
                    Some(Reply::BrokeOff(_)) => "The dealer broke the talk off. Come back another day - or buy at the list price.",
                    _ if talk.agreed => "You have a deal. The contract says what was agreed.",
                    _ => "That was his last word: take it, or leave it.",
                })
            });
            json!({
                "kind": "talk", "icon": "forum", "title": trf("Talking to %{who}", &[("who", &seller)]), "file": listing.bus.file, "paint": livery, "name": listing.bus.name,
                "what": what, "first": row(tr("Asked at first"), eur(talk.quote.list), false),
                "asks_caps": tr("He asks now").to_uppercase(), "asking": eur(talk.asking), "agreed": talk.agreed,
                "off": (off >= 0.001).then(|| format!("−{} %", num(off * 100.0, 1))),
                "rounds": { "frac": rf, "tone": share_tone(rf, false), "label": trf("Rounds left: %{n} of %{max}", &[("n", &left_n.to_string()), ("max", &talk.max_rounds.to_string())]), "tip": tr("Each move takes a round: when none is left, his word is the last") },
                "mood": { "frac": mood, "tone": grade(mood * 100.0), "label": tr("The dealer's patience"), "right": mood_word, "tip": tr("Low offers and many demands try his patience: when it is gone, he breaks the talk off") },
                "standing": offer.is_none().then(|| standing_line(&c, &listing.maker)), "bulk": offer.is_none().then(|| bulk_line(talk.quote.count)),
                "extras_caps": (!talk.extras.is_empty()).then(|| tr("Agreed extras").to_uppercase()),
                "agreed_extras": talk.extras.iter().map(|e| json!([e.icon(), tr(e.label())])).collect::<Vec<_>>(),
                "said": lines.iter().map(|(you, t)| json!({ "you": you, "text": t })).collect::<Vec<_>>(), "you": tr("You"), "seller": seller,
                "open": !talk.closed, "closed_text": closed_text,
                "ask": tr("Ask for a discount"), "offer": tr("Make an offer"), "bid_placeholder": eur(talk.asking), "bid_tip": tr("Type the price you offer for one bus"),
                "extras": extras,
                "deal": (!broke).then(|| trf("Take %{amount} and draw up the contract", &[("amount", &eur(talk.asking))])), "leave": tr("Leave the talk"),
            })
        }
        Sheet::Contract { contract: k, readonly } => {
            let now = if readonly { k.signed_at.clone() } else { dl::now_of(&c) };
            let seller_is = tr(if k.new { "Dealer of new buses" } else { "Seller of used buses" });
            let mut bus = vec![row(tr("Bus"), format!("{} × {}", k.count, k.listing.bus.name), true), row(tr("Kind"), super::vehicles::kind_label(&k.listing.bus), false)];
            if k.new {
                bus.push(row(tr("Condition"), tr("New"), false));
            } else {
                bus.push(row(tr("Built"), k.built.get(..7).unwrap_or("").to_string(), false));
                bus.push(row(tr("Kilometres"), format!("{} km", grouped(k.km)), false));
                bus.push(row(tr("Condition"), format!("{:.0} / 100", k.condition), false));
            }
            bus.push(row(tr("Livery"), livery_label(&k.livery), false));
            bus.push(row(tr("Delivery"), if k.delivery_days <= 0 { tr("At once, on signing") } else { day_label(&dl::day_of(&k.delivery(&now))) }, false));
            let delivery = (k.new && !readonly).then(|| {
                let fee = if k.extras.contains(&Extra::FastDelivery) { tr("thrown in") } else { eur(dl::quick_fee(&c, k.list)) };
                json!({ "options": [tr("Regular: a working week"), trf("Quick: at once (+%{amount} a bus)", &[("amount", &fee)])], "value": usize::from(k.delivery_days <= 0) })
            });
            let after = vec![row(tr("Warranty"), trf("%{n} months", &[("n", &k.warranty_months.to_string())]), false)];
            let extras = if k.extras.is_empty() { tr("None") } else { k.extras.iter().map(|e| tr(e.label())).collect::<Vec<_>>().join(", ") };
            let mut price = Vec::new();
            if k.list != k.price {
                price.push(row(tr("List price a bus"), eur(k.list), false));
            }
            price.push(row(tr("Agreed price a bus"), eur(k.price), false));
            if k.quick > 0 {
                price.push(row(tr("Quick delivery a bus"), eur(k.quick), false));
            }
            if k.painting > 0 {
                price.push(row(tr("Painting a bus"), eur(k.painting), false));
            } else if !k.livery.is_empty() {
                price.push(row(tr("Painting a bus"), tr("included"), false));
            }
            if k.grant > 0 && k.pay != PayWay::Lease {
                price.push(row(tr("Grant a bus"), format!("- {}", eur(k.grant)), false));
            }
            let mut ways = vec![PayWay::Cash, PayWay::Loan];
            if k.new {
                ways.push(PayWay::Lease);
            }
            let mut pay_rows = Vec::new();
            if readonly {
                pay_rows.push(row(tr("Payment"), tr(k.pay.label()), false));
            }
            match k.pay {
                PayWay::Cash => {
                    pay_rows.push(row(tr("To pay on signing"), eur(k.due()), true));
                    if !readonly {
                        pay_rows.push(row(tr("Cash afterwards"), eur(c.cash - k.due()), false));
                    }
                }
                PayWay::Loan => {
                    let (monthly, months, rate) = fi::loan_terms(&c, k.due());
                    pay_rows.push(row(tr("The bank lends"), eur(k.due()), true));
                    pay_rows.push(row(tr("Monthly rate"), format!("{}  ·  {}", eur(monthly), trf("%{n} months at %{rate} %", &[("n", &months.to_string()), ("rate", &num(rate * 100.0, 1))])), false));
                }
                PayWay::Lease => {
                    let (monthly, months, residual) = k.lease(&c);
                    pay_rows.push(row(tr("Monthly rate a bus"), eur(monthly), true));
                    pay_rows.push(row(tr("Term"), trf("%{n} months", &[("n", &months.to_string())]), false));
                    pay_rows.push(row(tr("Residual value"), eur(residual), false));
                }
            }
            let day = if readonly { k.signed_at.clone() } else { now.clone() };
            json!({
                "kind": "contract", "icon": "description", "readonly": readonly,
                "title": if readonly { trf("Purchase contract no. %{no}", &[("no", &k.no.to_string())]) } else { tr("Purchase contract") },
                "parties": parties(("Seller", &k.seller, &seller_is), ("Buyer", &k.buyer, &home_of(&c))),
                "bus_caps": tr("The bus").to_uppercase(), "bus": bus, "delivery": delivery, "after": after,
                "extras_caps": tr("Extras").to_uppercase(), "extras": extras,
                "price_caps": tr("Price").to_uppercase(), "price": price,
                "ways": (!readonly).then(|| ways.iter().map(|w| json!([format!("{w:?}"), tr(w.label())])).collect::<Vec<_>>()), "pay": format!("{:?}", k.pay),
                "pay_rows": pay_rows,
                "signature": signature(&k.strokes, &k.signed_by, readonly, &day),
                "sign": tr(if k.pay == PayWay::Loan { "Sign, then the loan" } else { "Sign the contract" }),
                "cancel": tr(if armed { "Throw it away" } else { "Cancel" }), "armed": armed,
                "armed_text": tr("The signature is thrown away with the contract: press again to leave."), "unsigned": tr("Sign the contract first."),
                "close": tr("Close"), "paper": tr("Purchase contract"),
            })
        }
        Sheet::Back { drive } => json!({
            "kind": "back", "icon": "sports_score", "title": tr("Back from the test drive"),
            "text": trf("How did the %{bus} drive? The dealer is waiting for you - the test drive did not count as company time.", &[("bus", &drive.name)]),
            "yes": tr("Back to the bus"), "no": tr("Not now"),
        }),
        Sheet::Loan { amount, term, purpose, collateral, fixed, then: _ } => {
            let cr = fi::credit(&c, collateral);
            let terms = fi::terms_offered(&c);
            let longest = terms.len().saturating_sub(1);
            let term = if term == usize::MAX { longest } else { term.min(longest) };
            let cents = amount;
            let months = terms.get(term).copied().unwrap_or(12);
            let k = fi::draft_loan(&c, cents, months, &purpose, collateral);
            let after = (cr.left - cents).max(0);
            let after_room = fi::room(&fi::Credit { left: after, ..cr });
            let rate = if cr.discount > 0.0 { trf("%{rate} a year (%{discount} less for the company's level)", &[("rate", &percent(k.rate)), ("discount", &percent(cr.discount))]) } else { trf("%{rate} a year", &[("rate", &percent(k.rate))]) };
            let presets: Vec<i64> = [50_000_00i64, 100_000_00, 250_000_00, 500_000_00].into_iter().filter(|p| *p <= cr.left).collect();
            let mut chips: Vec<Value> = presets.iter().map(|p| json!([p, eur(*p)])).collect();
            chips.push(json!([cr.left, tr("All that is left")]));
            json!({
                "kind": "loan", "icon": "payments", "title": tr("A loan from the bank"), "fixed": fixed,
                "for": fixed.then(|| trf("For %{what}: the bank lends what is to be paid, and the buses are its security.", &[("what", &purpose)])),
                "amount_caps": tr("Amount").to_uppercase(), "amount": cents, "amount_text": eur(cents),
                "left": trf("The bank lends up to %{amount} more", &[("amount", &eur(cr.left))]), "left_tone": room_tone(fi::room(&cr)),
                "min": fi::SMALLEST_LOAN, "max": cr.left.max(fi::SMALLEST_LOAN), "presets": chips,
                "term_label": tr("Term"), "terms": terms.iter().map(|m| trf("%{n} months", &[("n", &m.to_string())])).collect::<Vec<_>>(), "term": term,
                "rows": [
                    toned(tr("Interest"), rate, None),
                    toned(tr("Monthly payment"), eur(k.monthly), Some("text")),
                    toned(tr("Paid back in all"), eur(k.total), None),
                    toned(tr("Of which interest"), eur(k.interest()), None),
                    toned(tr("First payment"), day_label(&k.first_rate), None),
                    toned(tr("Cash afterwards"), if fixed { eur(c.cash) } else { eur(c.cash + cents) }, None),
                    toned(tr("Credit room afterwards"), eur(after), Some(room_tone(after_room))),
                ],
                "main": tr("Draw up the loan contract"), "cancel": tr("Cancel"),
            })
        }
        Sheet::LoanContract { contract: k, readonly, collateral: _, then: _ } => {
            let early = if k.early_fee <= 0.0 { tr("at any time, free of charge") } else { trf("at any time, a fee of %{rate} of what is paid back", &[("rate", &percent(k.early_fee))]) };
            let purpose = if k.purpose.trim().is_empty() { tr("The company's needs") } else { k.purpose.clone() };
            let plan = fi::schedule(k.amount, k.rate, k.months);
            let shown: Vec<Value> = if plan.len() <= 9 {
                plan.iter().map(|i| json!([i.month.to_string(), eur(i.payment), eur(i.interest), eur(i.principal), eur(i.remaining)])).collect()
            } else {
                plan.iter().take(6).map(|i| json!([i.month.to_string(), eur(i.payment), eur(i.interest), eur(i.principal), eur(i.remaining)])).chain([Value::Null]).chain(plan.iter().rev().take(2).rev().map(|i| json!([i.month.to_string(), eur(i.payment), eur(i.interest), eur(i.principal), eur(i.remaining)]))).collect()
            };
            let interest: i64 = plan.iter().map(|i| i.interest).sum();
            let day = if readonly { k.signed_at.clone() } else { c.date.clone() };
            json!({
                "kind": "loan_contract", "icon": "description", "readonly": readonly,
                "title": if readonly { trf("Loan contract no. %{no}", &[("no", &k.no.to_string())]) } else { tr("Loan contract") },
                "parties": parties(("Lender", &k.lender, &tr("Business bank")), ("Borrower", &k.borrower, &home_of(&c))),
                "loan_caps": tr("The loan").to_uppercase(),
                "rows": [
                    toned(tr("Amount"), eur(k.amount), Some("text")), toned(tr("Purpose"), purpose, None),
                    toned(tr("Interest"), trf("%{rate} a year", &[("rate", &percent(k.rate))]), None),
                    toned(tr("Term"), trf("%{n} months", &[("n", &k.months.to_string())]), None),
                    toned(tr("Monthly payment"), eur(k.monthly), None), toned(tr("Paid back in all"), eur(k.total), None),
                    toned(tr("First payment"), day_label(&k.first_rate), None),
                ],
                "security": trf("Security: the company's buses, worth %{amount}.", &[("amount", &eur(k.security))]),
                "early": trf("Paying back early: %{terms}.", &[("terms", &early)]),
                "plan_caps": tr("Repayment").to_uppercase(), "plan_heads": [tr("Month"), tr("Payment"), tr("Interest"), tr("Repaid"), tr("Owed")], "plan": shown,
                "interest": trf("Interest in all: %{amount}", &[("amount", &eur(interest))]),
                "signature": signature(&k.strokes, &k.signed_by, readonly, &day),
                "sign": tr("Sign the loan contract"), "back": tr("Back"), "close": tr("Close"), "unsigned": tr("Sign the contract first."), "paper": tr("Loan contract"),
            })
        }
        Sheet::Repay { id, amount } => {
            let Some(loan) = c.loans.iter().find(|x| x.id == id).cloned() else { return Value::Null };
            let amount = amount.min(loan.remaining);
            let fee = fi::early_fee(&c, amount);
            let what = if loan.purpose.is_empty() { tr("Loan") } else { loan.purpose.clone() };
            json!({
                "kind": "repay", "icon": "payments", "title": tr("Pay back early"),
                "rows": [
                    toned(tr("Loan"), what, None), toned(tr("Paid back now"), eur(amount), None),
                    toned(trf("Early repayment fee (%{rate})", &[("rate", &percent(fi::early_fee_rate(c.game_mode)))]), eur(fee), (fee > 0).then_some("warn")),
                    toned(tr("From the cash"), eur(amount + fee), Some("text")), toned(tr("Still owed afterwards"), eur(loan.remaining - amount), None),
                ],
                "note": tr(if amount >= loan.remaining { "The loan is paid off: no more rates." } else { "The monthly payment stays the same: the loan ends sooner." }),
                "main": trf("Pay back %{amount}", &[("amount", &eur(amount + fee))]), "cancel": tr("Cancel"),
            })
        }
        Sheet::Advert { contract: k, readonly } => {
            let trade = tr(&k.trade);
            let rows = vec![
                row(tr("Where"), tr(k.kind.label()), false),
                row(tr("Buses"), k.buses.len().to_string(), false),
                row(tr("Term"), trf("%{n} months", &[("n", &k.months.to_string())]), false),
                row(tr("From"), day_label(&k.from), false),
                row(tr("Until"), day_label(&k.until), false),
                row(tr("A bus a month"), eur(k.per_bus), false),
                row(tr("A month in all"), eur(k.monthly()), false),
                row(tr("Over the term"), eur(k.monthly() * k.months as i64), false),
            ];
            let buses: Vec<Value> = k.buses.iter().filter_map(|id| c.vehicle(*id)).map(|v| row(format!("{}  {}", v.number, v.name), tr(v.kind.label()), false)).collect();
            let day = if readonly { k.signed_at.clone() } else { c.date.clone() };
            json!({
                "kind": "advert", "icon": "campaign", "readonly": readonly,
                "title": if readonly { trf("Advertising contract no. %{no}", &[("no", &k.no.to_string())]) } else { tr("Advertising contract") },
                "parties": parties(("Advertiser", &k.advertiser, &trade), ("Bus company", &c.name, &home_of(&c))),
                "advert_caps": tr("The advert").to_uppercase(), "rows": rows,
                "terms": trf("Paid at every month's end, from the month after the signing. Ending it early costs three months' fee (%{amount}). The advertiser pays the printing and the foil.", &[("amount", &eur(ad::penalty(&k)))]),
                "buses_caps": tr("The buses that carry it").to_uppercase(), "buses": buses,
                "signature": signature(&k.strokes, &k.signed_by, readonly, &day),
                "sign": tr("Sign the advertising contract"), "back": tr("Back"), "close": tr("Close"), "unsigned": tr("Sign the contract first."), "paper": tr("Advertising contract"),
            })
        }
    }
}

// --- the sheets: what is done --------------------------------------------------------------------------

/// Euros typed ("245000", "245.000", "€ 245,000") as cents.
pub fn typed_euros(s: &str) -> Option<i64> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    let whole = match t.rfind([',', '.']) {
        Some(p) if t.len() - p - 1 <= 2 => &t[..p],
        _ => t.as_str(),
    };
    let digits: String = whole.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().filter(|v| *v > 0).map(|v| v * 100)
}

/// Buses joined the fleet: say so.
pub fn joined(s: &mut Session, ids: &[u32]) {
    let numbers: Vec<String> = ids.iter().filter_map(|id| s.company.as_ref().and_then(|c| c.vehicle(*id)).map(|v| v.number.clone())).collect();
    let text = if numbers.len() == 1 { trf("Bus %{n} joined the fleet.", &[("n", &numbers[0])]) } else { trf("Buses %{n} joined the fleet.", &[("n", &numbers.join(", "))]) };
    s.say(text, false);
}

/// Say what a signed purchase did.
fn signed_status(s: &mut Session, done: Signed) -> Value {
    let no = s.company.as_ref().map(|c| c.dealer.counter).unwrap_or(0);
    match done {
        Signed::Delivered(ids) => {
            let numbers: Vec<String> = ids.iter().filter_map(|id| s.company.as_ref().and_then(|c| c.vehicle(*id)).map(|v| v.number.clone())).collect();
            let mut text = trf("Contract %{no} is signed: fleet numbers %{numbers} joined the fleet.", &[("no", &no.to_string()), ("numbers", &numbers.join(", "))]);
            if let Some(note) = ids.first().and_then(|id| s.company.as_ref().and_then(|c| super::people::drivers_note(c, *id))) {
                text = format!("{text} {note}");
            }
            s.say(text, false);
            json!({})
        }
        Signed::Ordered { no, delivery } => {
            let t = trf("Contract %{no} is signed: the buses arrive on %{date}.", &[("no", &no.to_string()), ("date", &day_label(&dl::day_of(&delivery)))]);
            s.say(t, false);
            json!({ "dealer_tab": 3 })
        }
    }
}

/// Why a purchase cannot be made now, in a popup.
fn cannot_buy(s: &Session, c: &Company, allowed: Result<(), &'static str>, total: i64, pay: usize) -> Value {
    match allowed {
        Err(e) => refuse(s, e),
        Ok(()) if pay == 0 => super::popup(s, kit::no_cash(c, total)),
        Ok(()) => super::popup(s, kit::no_credit(c, total)),
    }
}

/// The contract a talk led to.
fn contract_of(c: &Company, talk: &Talk, listing: &Listing, offer: Option<&Offer>, livery: &str) -> Sheet {
    let contract = match offer {
        Some(o) => dl::draft_offer(c, o, talk.quote.count, talk.asking, &talk.extras, livery),
        None => dl::draft_new(c, listing, talk.quote.count, talk.asking, &talk.extras, livery, None),
    };
    Sheet::Contract { contract, readonly: false }
}

/// The signature as the page drew it into the sheet's contract (0..1 of the field).
fn strokes_of(args: &Value) -> Vec<Vec<[f32; 2]>> {
    args.get("strokes").and_then(|v| serde_json::from_value::<Vec<Vec<[f32; 2]>>>(v.clone()).ok()).unwrap_or_default().into_iter().filter(|s| s.len() > 1).collect()
}

fn popup_of(v: Value) -> Result<Value, String> {
    Ok(json!({ "popup": v }))
}

/// The test drive (`dealer::test_drive`): marked on the company, and the page starts the game -
/// a free drive with this bus on the company's map at its depot, outside its time.
fn test_drive(s: &mut Session, bus: &str, name: &str, paint: &str, offer: Option<String>) -> Result<Value, String> {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if s.in_game() {
        return popup_of(super::popup(s, Popup::new("sports_esports", "A game is running", tr("The test drive waits until the game has ended."), tr("Close the game, then start the test drive again."), None)));
    }
    let root = core::load_config().root;
    if !omsi_cfg::missing_original_essentials(Path::new(&root)).is_empty() {
        return popup_of(super::popup(s, Popup::new("folder_open", "The original OMSI 2 is missing", tr("A session needs the original OMSI 2."), tr("Choose its folder under Setup first."), None)));
    }
    let entry = depot_entry(s, &c);
    let (b, n, p) = (bus.to_string(), name.to_string(), paint.to_string());
    let _ = apply(s, |c| {
        let now = dl::now_of(c);
        dl::start_test_drive(c, &b, &n, &p, offer, &now);
        Ok(())
    });
    s.dealer.sheet = None;
    let t = tr("Starting the test drive…");
    s.say(t, false);
    Ok(json!({ "test_drive": {
        "map": c.map, "bus": bus, "paint": (!paint.is_empty()).then_some(paint), "hof": (!c.depot.trim().is_empty()).then_some(c.depot.clone()),
        "entry": entry, "date": c.date, "plate": Value::Null, "number": Value::Null,
        "line": Value::Null, "tour": Value::Null, "trip": Value::Null, "whole_tour": false, "free_line": Value::Null, "legs": [],
        "schedule": false, "passengers": false, "on_foot": false, "lan": "off", "tutorial": Value::Null, "situation": Value::Null, "editor": false,
        "display_font": core::busfonts::BusFonts::load().font_for(bus),
    } }))
}

/// The map's entry point nearest to the depot: one named after a depot, or the first.
fn depot_entry(s: &mut Session, c: &Company) -> i32 {
    let maps = s.maps();
    let Some(m) = maps.iter().find(|m| m.file == c.map) else { return 0 };
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

/// Leaving a loan's sheet: back to the purchase that asked for it.
fn back_from_loan(then: Then) -> Option<Sheet> {
    match then {
        Then::Purchase(k) => Some(Sheet::Contract { contract: *k, readonly: false }),
        _ => None,
    }
}

/// Open the loan's sheet from the finances page (`bank::open_loan`).
pub fn open_loan(s: &mut Session) {
    let Some(c) = s.company.as_ref() else { return };
    let left = fi::credit(c, 0).left;
    let amount = (100_000_00i64).min(left).max(fi::SMALLEST_LOAN);
    open(s, Sheet::Loan { amount, term: usize::MAX, purpose: String::new(), collateral: 0, fixed: false, then: Then::Nothing });
}

/// What the dealer's pages and sheets do.
pub fn act(s: &mut Session, what: &str, args: &Value, _p: &dyn crate::Platform) -> Option<Result<Value, String>> {
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    Some(match what {
        "buying_mode" => {
            let m = BuyingMode::ALL[args.get("mode").and_then(Value::as_u64).unwrap_or(1).min(1) as usize];
            Ok(apply(s, |c| {
                c.dealer.mode = m;
                Ok(())
            })
            .err()
            .map(|p| json!({ "popup": p }))
            .unwrap_or(json!({})))
        }
        "open_model" => {
            let file = str_of("file");
            let Some(x) = listings(s).and_then(|l| l.iter().find(|x| x.bus.file == file).cloned()) else { return Some(Ok(json!({}))) };
            let quick = args.get("quick").and_then(Value::as_bool).unwrap_or(c.dealer.mode == BuyingMode::Simple);
            open(s, Sheet::Model { listing: x, livery: String::new(), count: 1, pay: 0, quick });
            Ok(json!({}))
        }
        "open_offer" => {
            let id = str_of("id");
            let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
            let now = dl::now_of(&c);
            match dl::offer(&c, &all, &id, &now) {
                Some(o) => {
                    open(s, Sheet::Offer { offer: o, livery: String::new(), count: 1, pay: 0, quick: c.dealer.mode == BuyingMode::Simple });
                    Ok(json!({}))
                }
                None => popup_of(refuse(s, "This offer has ended or is sold.")),
            }
        }
        "open_contract" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            if let Some(k) = c.dealer.contracts.iter().find(|k| k.no == no).cloned() {
                open(s, Sheet::Contract { contract: k, readonly: true });
            }
            Ok(json!({}))
        }
        "open_loan" => {
            if fi::room(&fi::credit(&c, 0)) == Room::None {
                popup_of(super::popup(s, kit::no_credit(&c, 0)))
            } else {
                open_loan(s);
                Ok(json!({}))
            }
        }
        "open_repay" => {
            let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let amount = c.loans.iter().find(|x| x.id == id).map(|x| x.remaining).unwrap_or(0);
            if c.cash <= 0 {
                popup_of(super::popup(s, kit::no_cash(&c, 0)))
            } else {
                let amount = amount.min(c.cash.max(0) * 100 / (100 + (fi::early_fee_rate(c.game_mode) * 100.0).round() as i64));
                open(s, Sheet::Repay { id, amount });
                Ok(json!({}))
            }
        }
        "open_loan_contract" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            if let Some(k) = c.dealer.loan_contracts.iter().find(|k| k.no == no).cloned() {
                open(s, Sheet::LoanContract { contract: k, readonly: true, collateral: 0, then: Then::Nothing });
            }
            Ok(json!({}))
        }
        "open_advert" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            match args.get("readonly").and_then(Value::as_bool).unwrap_or(false) {
                true => {
                    if let Some(k) = c.adverts.contracts.iter().find(|k| k.no == no).cloned() {
                        open(s, Sheet::Advert { contract: k, readonly: true });
                    }
                    Ok(json!({}))
                }
                false => {
                    let Some(o) = ad::offers(&c).into_iter().find(|o| o.no == no) else { return Some(Ok(json!({}))) };
                    if !co::levels::unlocked(&c, o.kind.feature()) {
                        return Some(popup_of(super::popup(s, kit::locked(&c, o.kind.feature()))));
                    }
                    match ad::draft(&c, &o) {
                        Ok(contract) => {
                            open(s, Sheet::Advert { contract, readonly: false });
                            Ok(json!({}))
                        }
                        Err(e) => popup_of(refuse(s, e)),
                    }
                }
            }
        }
        w if w.starts_with("sheet_") => sheet_act(s, &c, &w["sheet_".len()..], args),
        _ => return None,
    })
}

/// What a sheet's controls do (`sheet_<what>`).
fn sheet_act(s: &mut Session, c: &Company, what: &str, args: &Value) -> Result<Value, String> {
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let Some(sheet) = s.dealer.sheet.clone() else { return Ok(json!({})) };
    let close = |s: &mut Session| {
        s.dealer.sheet = None;
        s.dealer.discard_armed = false;
        s.generation += 1;
    };
    let set = |s: &mut Session, sh: Sheet| {
        s.dealer.sheet = Some(sh);
        s.generation += 1;
    };
    if what == "close" {
        // (a loan's sheet goes back to the purchase it was for; a contract half signed asks first)
        match sheet {
            Sheet::Loan { then, .. } => match back_from_loan(then) {
                Some(sh) => set(s, sh),
                None => close(s),
            },
            Sheet::Contract { contract, readonly: false } if !s.dealer.discard_armed && (!contract.strokes.is_empty() || !contract.signed_by.trim().is_empty()) => {
                s.dealer.discard_armed = true;
                s.generation += 1;
            }
            _ => close(s),
        }
        return Ok(json!({}));
    }
    match sheet {
        Sheet::Model { listing, mut livery, mut count, mut pay, mut quick } => {
            let allowed = co::market::kind_allowed(c, listing.bus.kind);
            match what {
                "set" => {
                    if let Some(v) = args.get("livery").and_then(Value::as_str) {
                        livery = v.to_string();
                    }
                    if let Some(v) = args.get("count").and_then(Value::as_u64) {
                        count = (v as u32).clamp(1, 10);
                    }
                    if let Some(v) = args.get("pay").and_then(Value::as_u64) {
                        pay = (v as usize).min(1);
                    }
                    if let Some(v) = args.get("quick").and_then(Value::as_bool) {
                        quick = v;
                    }
                    set(s, Sheet::Model { listing, livery, count, pay, quick });
                    Ok(json!({}))
                }
                "main" if quick => {
                    let n = count as i64;
                    let painting = if livery.is_empty() { 0 } else { dl::painting_cost(c) };
                    let r = co::economy::rules(c.game_mode);
                    let grant = co::economy::grant(listing.bus.kind, dl::list_price(c, listing.bus.kind), &r, c.price_index);
                    let total = n * (dl::quick_price(c, &listing, count) + painting - grant);
                    let (_, ok) = pay_line(c, total, pay);
                    if !ok || allowed.is_err() {
                        return popup_of(cannot_buy(s, c, allowed, total, pay));
                    }
                    if pay == 1 {
                        let purpose = format!("{} × {}", n, listing.bus.name);
                        set(s, Sheet::Loan { amount: total, term: usize::MAX, purpose, collateral: total, fixed: true, then: Then::Quick { listing, count, livery } });
                        return Ok(json!({}));
                    }
                    match apply(s, |c| dl::quick_buy(c, &listing, count, Payment::Cash, &livery)) {
                        Ok(ids) => {
                            joined(s, &ids);
                            close(s);
                            Ok(json!({}))
                        }
                        Err(p) => popup_of(p),
                    }
                }
                "main" => {
                    let now = dl::now_of(c);
                    if let Some(until) = dl::sulking(c, &listing.maker, &now) {
                        return popup_of(super::popup(s, Popup::new("forum", "The dealer will not talk", trf("The dealer does not want to talk to you until %{date}.", &[("date", &day_label(&dl::day_of(&until)))]), tr("Come back after that day - or buy at the list price with the quick buy."), None)));
                    }
                    if let Err(e) = allowed {
                        return popup_of(refuse(s, e));
                    }
                    let q = Quote::new_bus(c, &listing, count);
                    match dl::open_talk(c, &q, &now) {
                        Ok(talk) => {
                            let said = talk.replies.iter().map(|r| (false, reply_text(r))).collect();
                            set(s, Sheet::Talk { talk, listing, offer: None, livery, said });
                            Ok(json!({}))
                        }
                        Err(e) => popup_of(refuse(s, e)),
                    }
                }
                "test" => test_drive(s, &listing.bus.file, &listing.bus.name, &livery, None),
                "rent" => match allowed {
                    Err(e) => popup_of(refuse(s, e)),
                    Ok(()) => {
                        close(s);
                        Ok(json!({ "new": { "file": listing.bus.file, "livery": livery } }))
                    }
                },
                _ => Ok(json!({})),
            }
        }
        Sheet::Offer { offer, mut livery, mut count, mut pay, mut quick } => {
            let allowed = co::market::kind_allowed(c, offer.listing.bus.kind);
            match what {
                "set" => {
                    if let Some(v) = args.get("livery").and_then(Value::as_str) {
                        livery = v.to_string();
                    }
                    if let Some(v) = args.get("count").and_then(Value::as_u64) {
                        count = (v as u32).clamp(1, offer.count.max(1));
                    }
                    if let Some(v) = args.get("pay").and_then(Value::as_u64) {
                        pay = (v as usize).min(1);
                    }
                    if let Some(v) = args.get("quick").and_then(Value::as_bool) {
                        quick = v;
                    }
                    // (the age the player sets himself: its price follows; empty, the offer's own)
                    let mut offer = offer;
                    if let Some(y) = args.get("age").and_then(Value::as_str) {
                        let now = dl::now_of(c);
                        let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
                        let own = dl::offer(c, &all, &offer.id, &now).unwrap_or_else(|| offer.clone());
                        if y.trim().is_empty() {
                            offer = own;
                        } else {
                            match dl::with_age(c, &own, y) {
                                Some(o) => offer = o,
                                None => return popup_of(refuse(s, "Type a year of first registration, as 2015.")),
                            }
                        }
                    }
                    set(s, Sheet::Offer { offer, livery, count, pay, quick });
                    Ok(json!({}))
                }
                "main" if quick => {
                    let n = count as i64;
                    let painting = if livery.is_empty() { 0 } else { dl::painting_cost(c) };
                    let r = co::economy::rules(c.game_mode);
                    let grant = if offer.is_new() { co::economy::grant(offer.listing.bus.kind, offer.price, &r, c.price_index) } else { 0 };
                    let total = n * (offer.price + painting - grant);
                    let (_, ok) = pay_line(c, total, pay);
                    if !ok || allowed.is_err() {
                        return popup_of(cannot_buy(s, c, allowed, total, pay));
                    }
                    if pay == 1 {
                        let purpose = format!("{} × {}", n, offer.listing.bus.name);
                        set(s, Sheet::Loan { amount: total, term: usize::MAX, purpose, collateral: total, fixed: true, then: Then::QuickOffer { offer, count, livery } });
                        return Ok(json!({}));
                    }
                    let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
                    match apply(s, |c| dl::quick_buy_offer(c, &offer, count, Payment::Cash, &livery, &all)) {
                        Ok(ids) => {
                            joined(s, &ids);
                            close(s);
                            Ok(json!({}))
                        }
                        Err(p) => popup_of(p),
                    }
                }
                "main" => {
                    let now = dl::now_of(c);
                    let sulk = dl::sulking(c, &offer.listing.maker, &now).filter(|_| offer.kind != OfferKind::Used && offer.kind != OfferKind::Batch);
                    if let Some(until) = sulk {
                        return popup_of(super::popup(s, Popup::new("forum", "The dealer will not talk", trf("The dealer does not want to talk to you until %{date}.", &[("date", &day_label(&dl::day_of(&until)))]), tr("Come back after that day - or buy at the price asked with the quick buy."), None)));
                    }
                    if let Err(e) = allowed {
                        return popup_of(refuse(s, e));
                    }
                    let mut q = Quote::of_offer(&offer, count);
                    if !offer.is_new() && offer.kind != OfferKind::Demonstrator {
                        q.maker = offer.seller.clone();
                    }
                    match dl::open_talk(c, &q, &now) {
                        Ok(talk) => {
                            let said = talk.replies.iter().map(|r| (false, reply_text(r))).collect();
                            let listing = offer.listing.clone();
                            set(s, Sheet::Talk { talk, listing, offer: Some(offer), livery, said });
                            Ok(json!({}))
                        }
                        Err(e) => popup_of(refuse(s, e)),
                    }
                }
                "test" => {
                    let id = offer.id.clone();
                    test_drive(s, &offer.listing.bus.file, &offer.listing.bus.name, &livery, Some(id))
                }
                _ => Ok(json!({})),
            }
        }
        Sheet::Talk { mut talk, listing, offer, livery, mut said } => {
            let mv = match what {
                "ask" => Some(Move::AskDiscount),
                "bid" => match typed_euros(&str_of("bid")) {
                    Some(v) => Some(Move::Offer(v)),
                    None => return popup_of(super::popup(s, Popup::new("payments", "No price typed", tr("Type the price you offer for one bus in the field beside the button."), tr("For example 245000 - or ask for a discount first."), None))),
                },
                "extra" => Extra::ALL.iter().copied().find(|e| format!("{e:?}") == str_of("extra")).filter(|e| !talk.extras.contains(e)).map(Move::AskExtra),
                "deal" if talk.agreed => {
                    set(s, contract_of(c, &talk, &listing, offer.as_ref(), &livery));
                    return Ok(json!({}));
                }
                "deal" => Some(Move::Accept),
                _ => None,
            };
            let Some(m) = mv else { return Ok(json!({})) };
            said.push((true, move_text(&m)));
            let now = dl::now_of(c);
            let mut reply = None;
            let _ = apply(s, |c| {
                reply = Some(dl::respond(c, &mut talk, m, &now));
                Ok(())
            });
            if let Some(r) = reply {
                said.push((false, reply_text(&r)));
                if matches!(r, Reply::Accepted(_)) && matches!(m, Move::Accept) {
                    let c = s.company.clone().unwrap_or_else(|| c.clone());
                    set(s, contract_of(&c, &talk, &listing, offer.as_ref(), &livery));
                    return Ok(json!({}));
                }
            }
            set(s, Sheet::Talk { talk, listing, offer, livery, said });
            Ok(json!({ "said": true }))
        }
        Sheet::Contract { contract: mut k, readonly } => {
            if readonly {
                return Ok(json!({}));
            }
            match what {
                "set" => {
                    if let Some(q) = args.get("quick").and_then(Value::as_bool) {
                        dl::set_quick(c, &mut k, q);
                    }
                    if let Some(p) = args.get("pay").and_then(Value::as_str) {
                        if let Some(w) = [PayWay::Cash, PayWay::Loan, PayWay::Lease].into_iter().find(|w| format!("{w:?}") == p) {
                            k.pay = w;
                        }
                    }
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    s.dealer.discard_armed = false;
                    set(s, Sheet::Contract { contract: k, readonly });
                    Ok(json!({}))
                }
                // (signed: the page's pen goes over the paper first, then "book")
                "sign" => {
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    s.dealer.discard_armed = false;
                    let ok = match k.pay {
                        PayWay::Cash => c.cash >= k.due(),
                        PayWay::Loan => fi::credit_left(c, k.due()) >= k.due(),
                        PayWay::Lease => c.cash >= k.lease(c).0 * k.count as i64,
                    };
                    set(s, Sheet::Contract { contract: k.clone(), readonly });
                    if !k.is_signed() {
                        return popup_of(refuse(s, "Sign the contract first."));
                    }
                    if !ok {
                        return popup_of(if k.pay == PayWay::Loan { super::popup(s, kit::no_credit(c, k.due())) } else { super::popup(s, kit::no_cash(c, k.due())) });
                    }
                    if k.pay == PayWay::Loan {
                        let purpose = format!("{} × {}", k.count, k.listing.bus.name);
                        set(s, Sheet::Loan { amount: k.due(), term: usize::MAX, purpose, collateral: k.total(), fixed: true, then: Then::Purchase(Box::new(k)) });
                        return Ok(json!({}));
                    }
                    Ok(json!({ "pen": { "strokes": k.strokes, "name": k.signed_by, "title": tr("Purchase contract") } }))
                }
                "book" => {
                    let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
                    let now = dl::now_of(c);
                    match apply(s, |c| dl::sign(c, &k, &all, &now)) {
                        Ok(done) => {
                            close(s);
                            Ok(signed_status(s, done))
                        }
                        Err(p) => popup_of(p),
                    }
                }
                _ => Ok(json!({})),
            }
        }
        Sheet::Back { drive } => {
            if what != "yes" {
                close(s);
                return Ok(json!({}));
            }
            let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
            let quick = c.dealer.mode == BuyingMode::Simple;
            let livery = drive.livery.clone();
            if let Some(o) = drive.offer.as_deref().and_then(|id| dl::offer(c, &all, id, &dl::now_of(c))) {
                set(s, Sheet::Offer { offer: o, livery, count: 1, pay: 0, quick });
                return Ok(json!({ "dealer": true }));
            }
            if let Some(x) = all.iter().find(|x| x.bus.file == drive.bus) {
                set(s, Sheet::Model { listing: x.clone(), livery, count: 1, pay: 0, quick });
                return Ok(json!({ "dealer": true }));
            }
            close(s);
            popup_of(refuse(s, "This offer has ended or is sold."))
        }
        Sheet::Loan { mut amount, mut term, purpose, collateral, fixed, then } => match what {
            "set" => {
                if let Some(v) = args.get("amount").and_then(Value::as_i64) {
                    amount = v.max(0);
                }
                if let Some(v) = args.get("term").and_then(Value::as_u64) {
                    term = v as usize;
                }
                set(s, Sheet::Loan { amount, term, purpose, collateral, fixed, then });
                Ok(json!({}))
            }
            "main" => {
                let cr = fi::credit(c, collateral);
                let terms = fi::terms_offered(c);
                let t = if term == usize::MAX { terms.len().saturating_sub(1) } else { term.min(terms.len().saturating_sub(1)) };
                let months = terms.get(t).copied().unwrap_or(12);
                let ok = amount > 0 && amount <= cr.left;
                if !ok {
                    return popup_of(super::popup(s, kit::no_credit(c, amount)));
                }
                let k = fi::draft_loan(c, amount, months, &purpose, collateral);
                set(s, Sheet::LoanContract { contract: k, readonly: false, collateral, then });
                Ok(json!({}))
            }
            _ => Ok(json!({})),
        },
        Sheet::LoanContract { contract: mut k, readonly, collateral, then } => {
            if readonly {
                return Ok(json!({}));
            }
            match what {
                "set" => {
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    set(s, Sheet::LoanContract { contract: k, readonly, collateral, then });
                    Ok(json!({}))
                }
                "back" => {
                    let fixed = !matches!(then, Then::Nothing);
                    let term = fi::terms_offered(c).iter().position(|m| *m == k.months).unwrap_or(usize::MAX);
                    set(s, Sheet::Loan { amount: k.amount, term, purpose: k.purpose.clone(), collateral, fixed, then });
                    Ok(json!({}))
                }
                "sign" => {
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    let signed = k.is_signed();
                    let (strokes, name) = (k.strokes.clone(), k.signed_by.clone());
                    set(s, Sheet::LoanContract { contract: k, readonly, collateral, then });
                    if !signed {
                        return popup_of(refuse(s, "Sign the contract first."));
                    }
                    Ok(json!({ "pen": { "strokes": strokes, "name": name, "title": tr("Loan contract") } }))
                }
                "book" => {
                    let all = listings(s).map(|l| l.to_vec()).unwrap_or_default();
                    let now = dl::now_of(c);
                    let r = match then {
                        Then::Nothing => apply(s, |c| fi::sign_loan(c, &k, collateral)).map(|id| {
                            let t = trf("Loan contract %{no} is signed: the bank paid %{amount} into the cash.", &[("no", &id.to_string()), ("amount", &eur(k.amount))]);
                            s.say(t, false);
                            json!({})
                        }),
                        Then::Purchase(ref p) => apply(s, |c| dl::sign_financed(c, p, &k, &all, &now)).map(|(_, done)| signed_status(s, done)),
                        Then::Quick { ref listing, count, ref livery } => apply(s, |c| fi::with_loan(c, &k, collateral, |c| dl::quick_buy(c, listing, count, Payment::Cash, livery))).map(|(_, ids)| {
                            joined(s, &ids);
                            json!({})
                        }),
                        Then::QuickOffer { ref offer, count, ref livery } => apply(s, |c| fi::with_loan(c, &k, collateral, |c| dl::quick_buy_offer(c, offer, count, Payment::Cash, livery, &all))).map(|(_, ids)| {
                            joined(s, &ids);
                            json!({})
                        }),
                    };
                    match r {
                        Ok(v) => {
                            close(s);
                            Ok(v)
                        }
                        Err(p) => popup_of(p),
                    }
                }
                _ => Ok(json!({})),
            }
        }
        Sheet::Repay { id, amount } => {
            if what != "main" {
                return Ok(json!({}));
            }
            let fee = fi::early_fee(c, amount);
            if c.cash < amount + fee {
                return popup_of(super::popup(s, kit::no_cash(c, amount + fee)));
            }
            match apply(s, |c| fi::repay(c, id, amount)) {
                Ok(()) => {
                    let t = trf("Paid back: %{amount}.", &[("amount", &eur(amount + fee))]);
                    s.say(t, false);
                    close(s);
                    Ok(json!({}))
                }
                Err(p) => popup_of(p),
            }
        }
        Sheet::Advert { contract: mut k, readonly } => {
            if readonly {
                return Ok(json!({}));
            }
            match what {
                "set" => {
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    set(s, Sheet::Advert { contract: k, readonly });
                    Ok(json!({}))
                }
                "sign" => {
                    if args.get("strokes").is_some() {
                        k.strokes = strokes_of(args);
                    }
                    if let Some(n) = args.get("name").and_then(Value::as_str) {
                        k.signed_by = n.to_string();
                    }
                    let signed = k.is_signed();
                    let (strokes, name) = (k.strokes.clone(), k.signed_by.clone());
                    set(s, Sheet::Advert { contract: k, readonly });
                    if !signed {
                        return popup_of(refuse(s, "Sign the contract first."));
                    }
                    Ok(json!({ "pen": { "strokes": strokes, "name": name, "title": tr("Advertising contract") } }))
                }
                "book" => match apply(s, |c| ad::sign(c, &k)) {
                    Ok(no) => {
                        let t = trf("Advertising contract %{no} is signed: %{amount} a month from next month.", &[("no", &no.to_string()), ("amount", &eur(k.monthly()))]);
                        s.say(t, false);
                        close(s);
                        Ok(json!({}))
                    }
                    Err(p) => popup_of(p),
                },
                _ => Ok(json!({})),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_euros_are_read_and_names_in_natural_order() {
        assert_eq!(typed_euros("245000"), Some(245_000_00));
        assert_eq!(typed_euros("€ 245.000"), Some(245_000_00));
        assert_eq!(typed_euros("245000,50"), Some(245_000_00));
        assert_eq!(typed_euros("abc"), None);
        assert_eq!(name_cmp("DL9", "dl10"), Ordering::Less);
        let f = filter_of(&json!({ "size": 3, "drive": 2, "max_price": 400.0 }));
        assert_eq!((f.category, f.drive, f.max_price), (Some(Category::Articulated), Some(Drive::Electric), 400_000_00));
        assert_eq!(filter_of(&json!({ "size": 5 })).category, Some(Category::Coach));
        assert_eq!(filter_of(&json!({})).max_price, 0);
    }
}

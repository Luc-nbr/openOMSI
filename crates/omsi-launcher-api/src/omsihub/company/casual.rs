//! The Casual game mode's own interface (`crates/omsi-launcher-ui/src/company/casual/`): a calm
//! home - today in a sentence, what the company solved by itself, "Drive a shift", four large
//! tiles, the tips and after three weeks the hint to Normal - and a handful of simple pages: the
//! buses and the shop, the lines and the three-step line wizard, the drivers, the money, the
//! company's livery and depot, and today's shifts. What the pages say comes from `logic`, the
//! game-drawn launcher's own; the clicks are named (`act`) and the page goes where they say.

mod logic;

use super::kit::Go;
use super::words::{count_text, day_label, eur, eur_cents, grouped, short_date, signed, tr, trf};
use super::{act as apply, Session, HOME_TAB};
use logic::{Act, Freq, Pick};
use omsi_launcher_lib::company::clock::{self as ck, Step};
use omsi_launcher_lib::company::depot::{self as dp, Area};
use omsi_launcher_lib::company::plan::{BusOf, Who};
use omsi_launcher_lib::company::{self as co, BusSize, Company, Role};
use serde_json::{json, Value};

/// The Casual pages (beside the home, `HOME_TAB`), and the steps one level under them.
pub const BUSES: usize = 101;
pub const LINES: usize = 102;
pub const DRIVERS: usize = 103;
pub const MONEY: usize = 104;
pub const COMPANY: usize = 105;
pub const SHIFTS: usize = 106;
pub const SHOP: usize = 107;
pub const NEW_LINE: usize = 108;
/// The pages in the strip under the bar, in order.
pub const PAGES: [usize; 5] = [BUSES, LINES, DRIVERS, MONEY, COMPANY];
const ALL: [usize; 8] = [BUSES, LINES, DRIVERS, MONEY, COMPANY, SHIFTS, SHOP, NEW_LINE];

/// What Casual keeps while the company is open.
#[derive(Default)]
pub struct CasualState {
    /// Tips put away this session (their ids).
    pub hidden: Vec<String>,
}

pub fn is_casual(tab: usize) -> bool {
    tab == HOME_TAB || ALL.contains(&tab)
}

/// A Casual page's name.
pub fn name(tab: usize) -> &'static str {
    match tab {
        BUSES => "My buses",
        LINES => "My lines",
        DRIVERS => "My drivers",
        MONEY => "Money",
        COMPANY => "My company",
        SHIFTS => "Drive a shift",
        SHOP => "Buy a bus",
        NEW_LINE => "Add a line",
        _ => "Home",
    }
}

pub fn icon(tab: usize) -> &'static str {
    match tab {
        BUSES => "directions_bus",
        LINES => "route",
        DRIVERS => "groups",
        MONEY => "payments",
        COMPANY => "flag",
        SHIFTS => "play_arrow",
        SHOP => "add",
        NEW_LINE => "alt_route",
        _ => "home",
    }
}

/// The page a step lies under (the home for a page).
pub fn parent(tab: usize) -> usize {
    match tab {
        SHOP => BUSES,
        NEW_LINE => LINES,
        _ => HOME_TAB,
    }
}

/// The Casual page about what a page of Normal shows.
pub fn page_for(tab: usize) -> usize {
    match tab {
        t if is_casual(t) => t,
        1 => BUSES,
        2 => DRIVERS,
        3 | super::MAP_TAB => LINES,
        4 => MONEY,
        6 => SHIFTS,
        super::DEPOT_TAB => COMPANY,
        _ => HOME_TAB,
    }
}

/// What a page's "?" says.
fn help(tab: usize) -> (&'static str, &'static str) {
    match tab {
        BUSES => ("Your buses", "Every bus of your company, with its age and condition. Repairs and services happen by themselves. Click a bus to paint it or sell it; Buy a bus opens the shop."),
        SHOP => ("The bus shop", "The buses that fit your lines best come first. Pick one to see its price and buy it in one click: it joins the fleet and drives from the next morning."),
        LINES => ("Your lines", "The lines your company runs, with their buses and passengers this week. The slider sets a line's fare: the advice keeps passengers and earns well."),
        NEW_LINE => ("A new line", "Three steps: pick a line of the map or draw your own stops, choose how often it runs, done. The timetable, the shifts and the drivers are made by themselves."),
        DRIVERS => ("Your drivers", "Who drives for you, how well, and today's shift. The company hires the drivers it needs by itself at night; you can hire now, give a bonus or let someone go."),
        MONEY => ("Your money", "What came in and what went out, week by week, and where most of it goes. Green weeks earned money, red weeks cost money."),
        COMPANY => ("Your company", "Your company's livery, painted in the livery editor, and its depot: when the buses no longer fit, buy a bigger one."),
        SHIFTS => ("Drive a shift", "Today's shifts of your company. Pick one and the shift's start opens, with the line, the bus and the times: you start the game there."),
        _ => ("Home", "How your company's day goes, what it solved by itself, and what you can do now. The tips say what helps; Next day runs the rest of the day."),
    }
}

/// A popup's way on in Casual: none to what the company sees to by itself.
pub fn popup_go(g: Go) -> Option<Go> {
    match g {
        Go::Progress | Go::Courses | Go::Training | Go::Licences | Go::Research => None,
        g => Some(g),
    }
}

/// The days' reports of a simulation in Casual: told in a line.
pub fn told(s: &mut Session, reports: &[co::day::DayReport]) {
    let Some(r) = reports.last() else { return };
    let (money, _) = signed(r.result);
    let t = trf("A new day. Yesterday: %{pax} passengers, %{money}.", &[("pax", &grouped(r.passengers as f64)), ("money", &money)]);
    s.say(t, r.result < 0);
}

/// A page of the strip as the page draws it.
pub fn rail_item(tab: usize) -> Value {
    json!({ "tab": tab, "icon": icon(tab), "name": tr(name(tab)), "tip": tr(name(tab)), "badge": null, "away": false })
}

/// Every Casual page as the strip under the bar and its crumbs say it (the home as 100): its
/// name, icon and the page above it; and what the strip says on the home.
pub fn pages_json(company: &str) -> Value {
    let mut pages = serde_json::Map::new();
    for tab in std::iter::once(HOME_TAB).chain(ALL) {
        pages.insert(tab.to_string(), json!({ "name": tr(name(tab)), "icon": icon(tab), "parent": parent(tab) }));
    }
    json!({ "pages": pages, "home": trf("%{name} · Casual", &[("name", company)]) })
}

/// A click of the Casual pages as the page carries it out.
fn act_json(a: &Act) -> Value {
    match a {
        Act::Open(tab) => json!({ "kind": "open", "tab": tab }),
        Act::BuyBus => json!({ "kind": "open", "tab": SHOP }),
        Act::AddLine => json!({ "kind": "add_line" }),
        Act::HireNow => json!({ "kind": "hire_now" }),
        Act::Fare(line) => json!({ "kind": "fare", "line": line }),
        Act::BiggerDepot => json!({ "kind": "open", "tab": COMPANY }),
        Act::NextDay => json!({ "kind": "next_day" }),
        Act::ToNormal => json!({ "kind": "to_normal" }),
        Act::Seen(id) => json!({ "kind": "seen", "id": id }),
    }
}

/// A line's plate.
fn plate_of(c: &Company, line: &str, number: &str) -> Value {
    match c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)) {
        Some(cl) => super::lines::plate(&cl.number, &cl.colour),
        None => super::lines::plate(number, ""),
    }
}

fn head(tab: usize, sub: String, action: Option<(&str, &str)>) -> Value {
    let (title, text) = help(tab);
    json!({ "name": tr(name(tab)), "sub": sub, "action": action.map(|(l, i)| json!([tr(l), i])), "help": { "icon": "help", "title": tr(title), "text": tr(text), "unlock": "", "go": null }, "help_tip": tr("What is this page?") })
}

fn empty(icon: &str, title: &str, text: &str, button: Option<(&str, &str, Value)>) -> Value {
    json!({ "icon": icon, "title": tr(title), "text": tr(text), "button": button.map(|(l, i, a)| json!({ "label": tr(l), "icon": i, "act": a })) })
}

/// Today's plan of the company and its shifts to drive (None: still being read).
fn today_shifts(s: &mut Session, c: &Company) -> Option<(co::plan::DayPlan, Vec<logic::ShiftCard>)> {
    super::week_work(s);
    let p = super::plan_of(s, &c.date)?.ok()?;
    let cards = logic::shift_cards(&p, logic::now_minute(c));
    Some((p, cards))
}

fn bus_words(c: &Company, sc: &logic::ShiftCard) -> (String, bool) {
    match sc.bus {
        Some(BusOf::Own(id)) => match c.vehicle(id) {
            Some(v) => (format!("{} · {}", v.number, v.name), true),
            None => (tr("No bus yet"), false),
        },
        Some(BusOf::Rental) => (tr("A rented bus"), true),
        None => (tr("No bus yet"), false),
    }
}

fn home(s: &mut Session, c: &Company) -> Value {
    let plan = s.plan.clone();
    let now = logic::now_minute(c);
    let cards = today_shifts(s, c).map(|x| x.1);
    let hidden = s.casual.hidden.clone();
    let shifts = cards.as_ref().map(|x| x.len()).unwrap_or(0);
    let mine = cards.as_ref().is_some_and(|x| x.first().is_some_and(|s| s.who == Some(Who::Player)));
    let tiles: Vec<Value> = logic::tiles(c, plan.as_ref(), now)
        .iter()
        .map(|t| {
            let tone = match t.good {
                Some(true) => "ok",
                Some(false) => if t.page == MONEY { "danger" } else { "warn" },
                None => "text",
            };
            json!({ "page": t.page, "name": tr(t.name), "icon": t.icon, "big": t.big, "tone": tone, "under": t.under.text(), "action": { "label": tr(t.action.0), "icon": t.action.1, "act": act_json(&t.action.2) } })
        })
        .collect();
    let tips: Vec<Value> = logic::tips(c, plan.as_ref())
        .into_iter()
        .filter(|t| !hidden.contains(&t.id))
        .take(logic::TIPS)
        .map(|t| json!({ "id": t.id, "text": t.words.text(), "warn": t.warn, "button": { "label": tr(t.button.0), "icon": t.button.1, "act": act_json(&t.button.2) } }))
        .collect();
    json!({
        "page": "home", "today_caps": tr("Today").to_uppercase(), "date": day_label(&c.date),
        "sentence": logic::day_sentence(c, plan.as_ref(), now).text(),
        "solved_caps": tr("Solved by your company").to_uppercase(),
        "solved": logic::solved(c).into_iter().take(3).map(|(t, a)| super::inbox::words(&t, &a)).collect::<Vec<_>>(),
        "calm": tr("Nothing went wrong today: your company runs smoothly."),
        "drive": tr("Drive a shift"),
        "drive_sub": if mine { tr("Your shift of today is waiting") } else if shifts > 0 { trf("%{n} of today's shifts to choose from", &[("n", &shifts.to_string())]) } else { tr("See today's shifts") },
        "tiles": tiles, "tips_caps": tr("Tips").to_uppercase(), "tips": tips, "no_tips": tr("No tips right now: everything runs well."), "not_now": tr("Not now"),
        "hint": logic::hint_due(c).then(|| json!({ "text": tr("Want more control? Normal opens every page: planning, workshop, tenders. Nothing of your company is lost."), "yes": tr("Switch to Normal"), "no": tr("Don't show again") })),
    })
}

fn buses(s: &mut Session, c: &Company) -> Value {
    let sub = count_text(logic::buses(c), "1 bus · repairs and services happen by themselves", "%{n} buses · repairs and services happen by themselves");
    let order = c.dealer.orders.iter().min_by_key(|o| co::dealer::minutes_of(&o.delivery)).map(|first| {
        let n: u32 = c.dealer.orders.iter().map(|o| o.contract.count).sum();
        count_text(n as usize, "1 bus on its way: it arrives on %{date}.", "%{n} buses on their way: the next arrives on %{date}.").replace("%{date}", &day_label(&co::dealer::day_of(&first.delivery)))
    });
    let plan = s.plan.clone();
    let cards: Vec<Value> = c
        .fleet
        .iter()
        .filter(|v| v.held_on(&c.date))
        .map(|v| {
            let cond = (v.condition / 100.0).clamp(0.0, 1.0);
            let (doing, tone) = if v.in_workshop(&c.date) {
                (trf("In the workshop: back on %{date}", &[("date", &short_date(v.workshop_until.as_deref().unwrap_or("")))]), "warn")
            } else {
                match plan.as_ref().and_then(|p| p.tours.iter().find(|t| t.bus == Some(v.id) && !t.tour.unplanned)).map(|t| t.tour.number.clone()) {
                    Some(n) => (trf("On line %{n} today", &[("n", &n)]), "ok"),
                    None => (tr("Free today"), "dim"),
                }
            };
            json!({
                "id": v.id, "number": v.number, "name": v.name, "file": v.bus, "livery": v.livery,
                "age": trf("%{n} years old", &[("n", &format!("{:.0}", v.age_years(&c.date).max(0.0)))]),
                "cond": cond, "cond_tone": if cond >= 0.7 { "ok" } else if cond >= 0.5 { "warn" } else { "danger" },
                "word": tr(if cond >= 0.7 { "Good condition" } else if cond >= 0.5 { "Worn: repaired soon" } else { "Worn out: to the workshop" }),
                "doing": doing, "tone": tone,
            })
        })
        .collect();
    json!({
        "page": "buses", "head": head(BUSES, sub, Some(("Buy a bus", "add"))), "order": order, "cards": cards,
        "empty": empty("directions_bus", "No bus yet", "Buy your first bus in the shop: the buses that fit your lines come first, and it drives from the next morning.", Some(("Buy a bus", "add", json!({ "kind": "open", "tab": SHOP })))),
    })
}

const SIZES: [(&str, Option<BusSize>); 5] = [("All", None), ("Midibus", Some(BusSize::Midi)), ("Solo", Some(BusSize::Solo)), ("Articulated", Some(BusSize::Articulated)), ("Double-decker", Some(BusSize::Double))];

fn shop(s: &mut Session, c: &Company, ui: &Value) -> Value {
    let room = co::depot::places(c).saturating_sub(logic::buses(c));
    let mut out = json!({ "page": "shop", "head": head(SHOP, trf("Sorted by what fits your lines · room for %{n} more in the depot", &[("n", &room.to_string())]), None) });
    let Some(all) = super::dealer::listings(s) else {
        out["reading"] = json!(tr("The dealer is setting out the buses…"));
        return out;
    };
    let size = ui.get("size").and_then(Value::as_u64).unwrap_or(0).min(SIZES.len() as u64 - 1) as usize;
    let plan = s.plan.clone();
    let wanted = logic::sizes_wanted(plan.as_ref(), c);
    let mut list: Vec<(usize, bool, i64, co::dealer::Listing)> = all
        .iter()
        .filter(|x| SIZES[size].1.is_none_or(|s| x.bus.kind.size == s))
        .map(|x| (logic::fit_rank(x.bus.kind.size, &wanted), co::market::kind_allowed(c, x.bus.kind).is_err(), co::dealer::quick_price(c, x, 1), x.clone()))
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)).then(a.2.cmp(&b.2)).then_with(|| a.3.bus.name.cmp(&b.3.bus.name)));
    let cards: Vec<Value> = list
        .iter()
        .map(|(rank, locked, price, x)| {
            let title = if x.model.trim().is_empty() { x.bus.name.clone() } else { format!("{} {} {}", x.maker, x.model, x.version).trim().to_string() };
            let mut facts = tr(x.bus.kind.label());
            if let Some(n) = x.seats {
                facts = format!("{facts} · {}", trf("%{n} seats", &[("n", &n.to_string())]));
            }
            json!({ "file": x.bus.file, "livery": x.bus.default_paint, "name": x.bus.name, "fits": (*rank == 0 && !locked).then(|| tr("Fits your lines")), "title": title, "facts": facts, "locked": locked.then(|| tr("Opens later, as your company grows")), "price": eur(*price), "dear": *price > c.cash, "buy": tr("Buy") })
        })
        .collect();
    out["sizes"] = json!(SIZES.iter().map(|s| tr(s.0)).collect::<Vec<_>>());
    out["size"] = json!(size);
    out["cards"] = json!(cards);
    out["empty"] = empty("directions_bus", "No bus of this size", "The dealer sells the buses installed in your OMSI. Choose another size.", None);
    out
}

fn lines_page(s: &mut Session, c: &Company, ui: &Value) -> Value {
    let sub = count_text(c.lines.len(), "1 line · its timetable, shifts and drivers run by themselves", "%{n} lines · their timetables, shifts and drivers run by themselves");
    let focus = ui.get("focus").and_then(Value::as_str).unwrap_or("").to_string();
    let plan = s.plan.clone();
    let cards: Vec<Value> = c
        .lines
        .iter()
        .map(|line| {
            let buses = plan.as_ref().map(|p| logic::buses_on_line(p, &line.name)).unwrap_or(0);
            let pax = logic::week_passengers(c, &line.name);
            let (lo, hi) = co::fares::band(c, line);
            let fare = co::fares::fare_of(c, line);
            let advised = logic::advised_fare(c, line);
            let (advice, tone) = if (fare - advised).abs() <= 10 {
                (tr("Good: this is the advised fare."), "ok")
            } else if fare > advised {
                (trf("Advice: %{fare}. Higher fares drive passengers away.", &[("fare", &eur_cents(advised as f64))]), "warn")
            } else {
                (trf("Advice: %{fare}. The line could earn more.", &[("fare", &eur_cents(advised as f64))]), "accent2")
            };
            json!({
                "name": line.name, "plate": super::lines::plate(&line.number, &line.colour), "public": line.public_name(), "lit": focus.eq_ignore_ascii_case(&line.name),
                "facts": [["directions_bus", count_text(buses, "1 bus today", "%{n} buses today")], ["groups", trf("%{n} passengers this week", &[("n", &grouped(pax as f64))])]],
                "lo": lo, "hi": hi, "fare": fare, "advised": advised, "label": tr("A ticket"), "advice": advice, "tone": tone,
                "use": ((fare - advised).abs() > 10).then(|| tr("Use the advice")),
            })
        })
        .collect();
    json!({
        "page": "lines", "head": head(LINES, sub, Some(("Add a line", "alt_route"))), "cards": cards,
        "empty": empty("route", "No line yet", "Add your first line: pick one of the map's, or draw your own stops. It runs by itself from then on.", Some(("Add a line", "alt_route", json!({ "kind": "add_line" })))),
    })
}

fn freq_of(name: &str) -> Freq {
    Freq::ALL.iter().copied().find(|f| f.label() == name).unwrap_or_default()
}

/// The line wizard (`step` 0 pick, 1 how often, 2 done; `pick` "draw" or a map line's name;
/// `freq` Quiet/Normal/Busy).
fn new_line(s: &mut Session, c: &Company, ui: &Value) -> Value {
    let step = ui.get("step").and_then(Value::as_u64).unwrap_or(0).min(2);
    let pick = ui.get("pick").and_then(Value::as_str).map(|p| if p == "draw" { Pick::Draw } else { Pick::Map(p.to_string()) });
    let freq = freq_of(ui.get("freq").and_then(Value::as_str).unwrap_or("Normal"));
    let mut out = json!({
        "page": "new_line", "head": head(NEW_LINE, tr("Three steps: the timetable, the shifts and the drivers are made by themselves."), None),
        "steps": (["Pick the line", "How often", "Done"].iter().map(|x| tr(x)).collect::<Vec<_>>()), "step": step,
        "cancel": tr("Cancel"), "back": tr("Back"),
    });
    let Some(all) = s.map_lines().map(|l| l.to_vec()) else {
        out["reading"] = json!(tr("Reading the map's timetable…"));
        return out;
    };
    match step {
        0 => {
            let depot = c.depot.clone();
            let choices: Vec<Value> = logic::map_choices(c, &all)
                .iter()
                .map(|x| {
                    let runs = x.tours.iter().filter(|t| t.runs).count();
                    let often = match logic::map_headway(x) {
                        Some(h) => count_text(runs, "1 bus out today · about every %{m} minutes", "%{n} buses out today · about every %{m} minutes").replace("%{m}", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0)),
                        None => count_text(runs, "1 bus out today", "%{n} buses out today"),
                    };
                    let headway = logic::map_headway(x).map(|h| Freq::of_headway(h).label());
                    json!({ "name": x.name, "number": co::specials::number_of(x, &[&depot]), "caption": co::specials::caption_of(x, &[&depot]), "text": often, "freq": headway })
                })
                .collect();
            out["draw"] = json!({ "title": tr("Draw your own stops"), "text": tr("Click the stops on the map in the line editor: the way between them is found over the roads.") });
            out["choices"] = json!(choices);
        }
        1 => {
            let map = match &pick {
                Some(Pick::Map(name)) => all.iter().find(|x| x.name == *name),
                _ => None,
            };
            out["intro"] = json!(match map {
                Some(line) => match logic::map_headway(line) {
                    Some(h) => trf("This line of the map keeps the map's own timetable: a bus about every %{m} minutes in the daytime.", &[("m", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0))]),
                    None => tr("This line of the map keeps the map's own timetable."),
                },
                None => tr("How busy should your line be? You can change it later in the line editor."),
            });
            out["fixed"] = json!(map.is_some());
            out["freqs"] = json!(Freq::ALL.iter().map(|f| json!({ "key": f.label(), "label": tr(f.label()), "text": tr(f.words()), "icon": f.icon() })).collect::<Vec<_>>());
            out["next"] = json!(tr("Next"));
        }
        _ => {
            let Some(pick) = pick.as_ref() else { return out };
            let o = logic::outcome(c, pick, freq, &all);
            let often = match o.headway {
                Some(h) => trf("%{freq}: about every %{m} minutes", &[("freq", &tr(o.freq.label())), ("m", &format!("{:.0}", (h / 5.0).round().max(1.0) * 5.0))]),
                None => tr(o.freq.label()),
            };
            let mut rows = vec![json!(["schedule", tr("How often"), often, "text"]), json!(["event", tr("Timetable and shifts"), tr("Made by themselves"), "ok"])];
            rows.push(match o.buses {
                Some(b) => json!(["directions_bus", tr("Buses it needs"), if o.short > 0 { trf("%{n} (buy %{m} more)", &[("n", &b.to_string()), ("m", &o.short.to_string())]) } else { trf("%{n} (you have them)", &[("n", &b.to_string())]) }, if o.short > 0 { "warn" } else { "ok" }]),
                None => json!(["directions_bus", tr("Buses it needs"), tr("Shown in the line editor"), "soft"]),
            });
            rows.push(json!(["groups", tr("Drivers"), match o.drivers { Some(d) => trf("%{n}, hired by themselves", &[("n", &d.to_string())]), None => tr("Hired by themselves") }, "ok"]));
            if let Some(p) = o.price {
                rows.push(json!(["receipt_long", tr("Its licence"), eur(p), if c.cash >= p { "text" } else { "danger" }]));
            }
            out["title"] = json!(match pick {
                Pick::Map(_) => trf("Line %{n}, ready to run", &[("n", &o.number)]),
                Pick::Draw => tr("Your own line: draw its stops"),
            });
            out["rows"] = json!(rows);
            out["note"] = json!(tr(match pick {
                Pick::Map(_) => "Start the line, and it runs from now on: every morning the company gives its buses and drivers their work.",
                Pick::Draw => "The line editor opens with this timetable: click the stops on the map, and confirm the line there. It shows what the line costs and brings.",
            }));
            out["finish"] = json!(match pick {
                Pick::Draw => [tr("Draw the stops"), "location_on".to_string()],
                Pick::Map(_) => [tr("Start the line"), "check_circle".to_string()],
            });
        }
    }
    if step == 1 {
        out["freq"] = json!(freq.label());
    }
    out
}

/// The drivers its shifts ask for now, beyond those it has.
fn wanted(c: &Company, plan: Option<&co::day::Plan>) -> usize {
    let short = plan.map(|p| p.short_of().1).unwrap_or(0);
    let have = co::staff::staying(c, Role::Driver, &c.date);
    short.max((c.autopilot.drivers as usize).saturating_sub(have))
}

fn drivers(s: &mut Session, c: &Company) -> Value {
    let list: Vec<&co::Employee> = logic::drivers(c);
    let plan = s.plan.clone();
    let want = wanted(c, plan.as_ref());
    let sub = if want > 0 {
        count_text(list.len(), "1 driver · %{m} more wanted: hired tonight, or now", "%{n} drivers · %{m} more wanted: hired tonight, or now").replace("%{m}", &want.to_string())
    } else {
        count_text(list.len(), "1 driver · every shift has its driver", "%{n} drivers · every shift has its driver")
    };
    let cards: Vec<Value> = list
        .iter()
        .map(|e| {
            let (doing, tone) = if let Some(u) = &e.notice_until {
                (trf("Leaves on %{date}", &[("date", &day_label(u))]), "warn")
            } else if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
                (tr("Ill today"), "warn")
            } else if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
                (tr("On holiday"), "dim")
            } else if e.training_until.as_deref().is_some_and(|u| co::dates::between(&c.date, u) >= 0) {
                (tr("On a course today"), "dim")
            } else {
                match plan.as_ref().and_then(|p| logic::shift_of(p, e.id)) {
                    Some((n, from, to)) => (trf("Line %{n} · %{from} – %{to}", &[("n", &n), ("from", &ck::hhmm(from as i64)), ("to", &ck::hhmm(to as i64))]), "ok"),
                    None => (tr("Day off"), "dim"),
                }
            };
            json!({
                "id": e.id, "portrait": e.portrait(), "name": e.name, "stars": logic::stars(e), "doing": doing, "tone": tone, "leaving": e.notice_until.is_some(),
                "bonus": trf("Bonus %{amount}", &[("amount", &eur(co::staff::bonus_cost(e)))]), "bonus_tip": tr("A one-off bonus: they are happier and stay longer"), "fire": tr("Fire"),
            })
        })
        .collect();
    json!({
        "page": "drivers", "head": head(DRIVERS, sub, Some(("Hire automatically", "person"))), "cards": cards,
        "empty": empty("groups", "No driver yet", "The company hires the drivers its shifts need by itself, at night. Or hire them now.", Some(("Hire automatically", "person", json!({ "kind": "hire_now" })))),
    })
}

/// A week's result, short: "+12k", "−3k".
fn short_money(c: i64) -> String {
    let e = c as f64 / 100.0;
    let sign = if c >= 0 { "+" } else { "−" };
    let a = e.abs();
    if a >= 1_000_000.0 {
        format!("{sign}{:.1}M", a / 1_000_000.0)
    } else if a >= 1_000.0 {
        format!("{sign}{:.0}k", a / 1_000.0)
    } else {
        format!("{sign}{a:.0}")
    }
}

fn money(c: &Company) -> Value {
    let weeks = logic::weeks(c, 8);
    let costs = logic::biggest_costs(c);
    let total: i64 = costs.iter().map(|x| x.1).sum();
    let tip = logic::cost_tip(&costs).map(|(text, a)| {
        let (label, icon) = match a {
            Act::AddLine => ("Add a line", "alt_route"),
            _ => ("Buy a bus", "add"),
        };
        json!({ "text": tr(text), "label": tr(label), "icon": icon, "act": act_json(&a) })
    });
    json!({
        "page": "money", "head": head(MONEY, trf("In the bank: %{cash}", &[("cash", &eur(c.cash))]), None),
        "weeks_caps": tr("Week by week").to_uppercase(), "in": tr("In"), "out": tr("Out"),
        "weeks": weeks.iter().map(|w| { let res = w.income - w.expenses; let (t, tone) = signed(res); let label = short_date(&w.from); json!({ "label": label, "income": w.income, "expenses": w.expenses, "short": short_money(res), "tone": tone, "tip": format!("{}: {}  ·  {} {}  ·  {} {}", label, t, tr("In"), eur(w.income), tr("Out"), eur(w.expenses)) }) }).collect::<Vec<_>>(),
        "no_weeks": tr("No day closed yet: press Next day, and the first week's money shows here."),
        "costs_caps": tr("Where it goes (four weeks)").to_uppercase(), "no_costs": tr("Nothing spent yet."),
        "costs": costs.iter().take(5).map(|(k, a)| json!([tr(logic::cost_words(*k)), eur(*a), if total > 0 { *a as f64 / total as f64 } else { 0.0 }])).collect::<Vec<_>>(),
        "tip": tip,
    })
}

fn company_page(c: &Company) -> Value {
    let mut count: Vec<(&str, &str, usize)> = Vec::new();
    for v in c.fleet.iter().filter(|v| v.held_on(&c.date)) {
        match count.iter_mut().find(|x| x.0 == v.bus) {
            Some(x) => x.2 += 1,
            None => count.push((&v.bus, &v.livery, 1)),
        }
    }
    count.sort_by_key(|x| std::cmp::Reverse(x.2));
    let designs = c.designs.len();
    let places = dp::places(c);
    let held = logic::buses(c);
    let frac = if places == 0 { 1.0 } else { (held as f64 / places as f64).clamp(0.0, 1.0) };
    let depot_state = if let Some(wk) = c.site.works_on(Area::Parking) {
        json!({ "building": trf("Being built: %{n} more places from %{date}.", &[("n", &dp::SPACES_PER_LEVEL.to_string()), ("date", &day_label(&wk.until))]) })
    } else {
        match dp::next_cost(c, Area::Parking) {
            Some((cost, days)) => json!({ "next": trf("A bigger depot: %{n} more places for %{amount}, ready in %{days} days.", &[("n", &dp::SPACES_PER_LEVEL.to_string()), ("amount", &eur(cost)), ("days", &days.to_string())]), "button": tr("Buy a bigger depot"), "primary": frac >= 0.8 }),
            None => json!({ "max": tr("Your depot is as big as it gets.") }),
        }
    };
    json!({
        "page": "company", "head": head(COMPANY, c.name.clone(), None),
        "livery": {
            "title": tr("Company livery"), "colours": c.colours,
            "designs": if designs == 0 { tr("No livery of your own yet: your buses wear the maker's.") } else { count_text(designs, "1 livery of your own", "%{n} liveries of your own") },
            "text": tr("Paint your buses in your company's own colours in the livery editor. A bus painted in it wears it in the game too."),
            "button": count.first().map(|_| tr("Paint the company livery")), "bus": count.first().map(|x| json!([x.0, x.1])), "tip": trf("Designing a livery costs %{amount}", &[("amount", &eur(co::livery::design_fee(c)))]),
            "none": tr("Buy a bus first: then paint it."),
        },
        "depot": {
            "title": tr("Depot"), "name": c.depot, "used": trf("%{n} of %{all} places used", &[("n", &held.to_string()), ("all", &places.to_string())]),
            "frac": frac, "tone": if frac >= 1.0 { "danger" } else if frac >= 0.8 { "warn" } else { "ok" }, "state": depot_state,
        },
    })
}

fn shifts(s: &mut Session, c: &Company) -> Value {
    let mut out = json!({ "page": "shifts", "head": head(SHIFTS, tr("Pick one of today's shifts: its start opens with the line, the bus and the times."), None) });
    let Some((p, cards)) = today_shifts(s, c) else {
        out["reading"] = json!(tr("Reading today's timetable…"));
        return out;
    };
    if cards.is_empty() {
        out["empty"] = if c.lines.is_empty() {
            empty("play_arrow", "No line yet", "Add a line first: its shifts are here the next moment.", Some(("Add a line", "alt_route", json!({ "kind": "add_line" }))))
        } else if logic::buses(c) == 0 {
            empty("play_arrow", "No bus yet", "Buy a bus for your lines: their shifts are here once it runs.", Some(("Buy a bus", "add", json!({ "kind": "open", "tab": SHOP }))))
        } else {
            empty("play_arrow", "No shift left today", "Today's shifts have all begun. Run on to the next day for new ones.", Some(("Next day", "wb_sunny", json!({ "kind": "next_day" }))))
        };
        return out;
    }
    let list: Vec<Value> = cards
        .iter()
        .enumerate()
        .map(|(k, sc)| {
            let (bus, ok) = bus_words(c, sc);
            let tag = if sc.mine() { Some(json!([tr("Your shift"), "accent"])) } else if sc.who.is_none() { Some(json!([tr("No driver yet"), "warn"])) } else { None };
            json!({
                "k": k, "plate": plate_of(c, &sc.line, &sc.number), "tag": tag,
                "time": format!("{} – {}", ck::hhmm(sc.from as i64), ck::hhmm(sc.to as i64)), "bus": bus, "ok": ok,
                "trips": count_text(sc.trips, "1 trip", "%{n} trips"), "go": tr("Drive this shift"), "primary": sc.mine() || k == 0,
                "duty": { "date": p.date, "line": sc.line, "tour": sc.tour_no, "duty": sc.duty }, "mine": sc.mine(),
            })
        })
        .collect();
    out["cards"] = json!(list);
    out["note"] = json!(tr("A shift you pick is yours for today only: tomorrow your drivers drive it again."));
    out
}

/// A Casual page's view (`page`: home, buses, shop, lines, new_line, drivers, money, company, shifts).
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    match ui.get("page").and_then(Value::as_str).unwrap_or("home") {
        "buses" => buses(s, &c),
        "shop" => shop(s, &c, ui),
        "lines" => lines_page(s, &c, ui),
        "new_line" => new_line(s, &c, ui),
        "drivers" => drivers(s, &c),
        "money" => money(&c),
        "company" => company_page(&c),
        "shifts" => shifts(s, &c),
        _ => home(s, &c),
    }
}

/// "Next day": the rest of the company's day simulated and closed, to the next morning.
fn next_day(s: &mut Session) -> Result<Value, String> {
    if !super::may_step(s) {
        return Ok(json!({ "popup": super::busy(s) }));
    }
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    let to = ck::target(c, Step::Midnight) + ck::MORNING;
    super::simulate(s, to, true);
    Ok(json!({}))
}

/// "Hire automatically": the drivers wanted hired now.
fn hire_now(s: &mut Session) -> Result<Value, String> {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let n = wanted(&c, s.plan.as_ref()).min(co::autopilot::DRIVERS_A_NIGHT * 2);
    if n == 0 {
        let t = tr("You have the drivers your shifts need: the company hires more by itself when a line asks for them.");
        s.say(t, false);
        return Ok(json!({ "tab": DRIVERS }));
    }
    let names = apply(s, |c| {
        let mut out = Vec::new();
        for _ in 0..n {
            match co::staff::hire_best(c, Role::Driver) {
                Some(name) => out.push(name),
                None => break,
            }
        }
        Ok(out)
    })
    .unwrap_or_default();
    if names.is_empty() {
        let t = tr("Nobody could be hired now: no applicant this week, or the cash is short. The company tries again tonight.");
        s.say(t, true);
    } else {
        let t = trf("Hired: %{names}.", &[("names", &names.join(", "))]);
        s.say(t, false);
    }
    Ok(json!({ "tab": DRIVERS }))
}

/// What the Casual pages' buttons do.
pub fn act(s: &mut Session, what: &str, args: &Value, _p: &dyn crate::Platform) -> Option<Result<Value, String>> {
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(match what {
        "next_day" => next_day(s),
        "hire_now" => hire_now(s),
        "tip_hide" => {
            if let Some(t) = args.get("tip").and_then(Value::as_str) {
                s.casual.hidden.push(t.to_string());
            }
            s.generation += 1;
            Ok(json!({}))
        }
        "hint_off" => Ok(apply(s, |c| {
            c.autopilot.normal_hint_off = true;
            Ok(())
        })
        .err()
        .map(|p| json!({ "popup": p }))
        .unwrap_or(json!({}))),
        "seen" => {
            let mid = args.get("id").and_then(Value::as_u64).unwrap_or(0);
            Ok(apply(s, |c| {
                if let Some(m) = c.inbox.messages.iter_mut().find(|m| m.id == mid) {
                    m.read = true;
                }
                Ok(())
            })
            .err()
            .map(|p| json!({ "popup": p }))
            .unwrap_or(json!({})))
        }
        "bonus" => {
            let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
            match apply(s, |c| co::staff::bonus(c, id)) {
                Ok(cost) => {
                    let t = trf("%{name} got a bonus of %{amount}: thank you!", &[("name", &name), ("amount", &eur(cost))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "build_depot" => match apply(s, |c| dp::build(c, Area::Parking)) {
            Ok(_) => {
                let days = dp::next_cost(&c, Area::Parking).map(|x| x.1).unwrap_or(0);
                let t = trf("The depot grows: %{n} more places in %{days} days.", &[("n", &dp::SPACES_PER_LEVEL.to_string()), ("days", &days.to_string())]);
                s.say(t, false);
                Ok(json!({}))
            }
            Err(p) => Ok(json!({ "popup": p })),
        },
        "shop_buy" => {
            let file = args.get("file").and_then(Value::as_str).unwrap_or("").to_string();
            let Some(x) = super::dealer::listings(s).and_then(|l| l.iter().find(|x| x.bus.file == file).cloned()) else { return Some(Ok(json!({}))) };
            if let Err(e) = co::market::kind_allowed(&c, x.bus.kind) {
                return Some(Ok(json!({ "popup": super::refuse(s, e) })));
            }
            super::dealer::open(s, super::dealer::Sheet::Model { listing: x, livery: String::new(), count: 1, pay: 0, quick: true });
            Ok(json!({}))
        }
        "casual_line" => {
            // the wizard's end: the map's line taken on and run (a line of one's own is drawn in
            // the line editor - the game-drawn launcher's for now)
            let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let Some(all) = s.map_lines().map(|l| l.to_vec()) else { return Some(Ok(json!({}))) };
            let Some(line) = all.iter().find(|x| x.name == name).cloned() else { return Some(Ok(json!({}))) };
            let number = co::specials::number_of(&line, &[&c.depot]);
            let direct = co::concessions::may_add_directly(&c);
            let r = if direct {
                let date = c.date.clone();
                apply(s, |c| {
                    co::network::add_line(c, &line, None)?;
                    co::autopilot::start_lines(c, &date);
                    Ok(())
                })
            } else {
                apply(s, |c| co::autopilot::take_line(c, &line).map(|_| ()))
            };
            match r {
                Ok(()) => {
                    let t = trf(if direct { "Line %{n} is yours: it runs by itself from now on." } else { "Line %{n} is yours: it runs by itself from tomorrow." }, &[("n", &number)]);
                    s.say(t, false);
                    Ok(json!({ "tab": LINES, "focus": line.name }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "drive_shift" => {
            // the shift is the player's for today, and the duty launcher opens with it
            let mine = args.get("mine").and_then(Value::as_bool).unwrap_or(false);
            let d = args.get("duty").cloned().unwrap_or(Value::Null);
            if !mine {
                let (date, line, tour, duty) = (d["date"].as_str().unwrap_or("").to_string(), d["line"].as_str().unwrap_or("").to_string(), d["tour"].as_str().unwrap_or("").to_string(), d["duty"].as_u64().unwrap_or(0) as usize);
                let _ = apply(s, |c| {
                    co::autopilot::pick_shift(c, &date, &line, &tour, duty);
                    Ok(())
                });
            }
            super::career::drive(s, args, SHIFTS)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_casual_page_has_its_place() {
        for t in ALL {
            assert!(is_casual(t));
            assert_eq!(page_for(t), t);
        }
        assert_eq!(parent(SHOP), BUSES);
        assert_eq!(parent(NEW_LINE), LINES);
        assert_eq!(page_for(1), BUSES);
        assert_eq!(page_for(super::super::DEPOT_TAB), COMPANY);
        assert_eq!(popup_go(Go::Research), None);
        assert_eq!(popup_go(Go::Dealer), Some(Go::Dealer));
        assert_eq!(short_money(12_345_00), "+12k");
        assert_eq!(short_money(-500_00), "−500");
    }
}

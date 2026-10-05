//! The bus company's pages (the rules are `omsi_launcher_lib::company`'s): founding a
//! company, its overview, its fleet and the vehicle market, its staff and the labour market,
//! its lines and how today's tours are covered, its finances, and "Close the day" with the
//! day's report. Reached from the start's fifth tile, as Omsi-Hub's bus company is from the
//! tile beside its ways to drive.
//!
//! Calm, as Omsi-Hub's company pages are: sections with a hairline, figures in big type,
//! nothing moves but what is under the mouse. What takes time - reading the buses for the
//! market, the timetable of the company's day, closing a day - runs on a thread of its own.

mod career;
mod concessions;
mod depot;
mod fleet;
mod lines;
mod map;
mod money;
mod overview;
mod people;
mod planning;
mod repair_game;
mod wizard;

use super::theme::*;
use super::ui::{ButtonKind, Ui};
use super::Launcher;
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::day::{DayReport, Note, Plan};
use omsi_launcher_lib::company::market::MarketBus;
use omsi_launcher_lib::company::{self as co, Cents, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::sync::mpsc::{channel, Receiver, Sender};

enum Msg {
    Companies(String, Vec<Company>),
    Market(usize, Vec<MarketBus>),
    Lines { map: String, date: String, result: Result<Vec<core::LineInfo>, String> },
    Own(String, Vec<core::lines::OwnLine>),
    Closed(Result<(Company, Vec<DayReport>), String>),
}

/// The timetable of the company's day.
pub(super) struct Today {
    map: String,
    date: String,
    lines: Vec<core::LineInfo>,
    error: Option<String>,
}

pub(super) const TABS: [&str; 10] = ["Overview", "Fleet", "Staff", "Lines", "Finances", "Planning", "Career", "Depot", "Concessions", "Map"];
/// The tabs of the depot, the concession market and the fleet map (see `TABS`).
pub(super) const DEPOT_TAB: usize = 7;
pub(super) const CONCESSIONS_TAB: usize = 8;
pub(super) const MAP_TAB: usize = 9;

/// A dialog over the company's pages.
pub(super) enum Dialog {
    /// A new bus of the market: bought (cash or loan), leased or rented.
    New { bus: MarketBus, how: usize, days: f32, livery: usize },
    /// A used offer of the week.
    Used { offer: co::market::UsedOffer, how: usize, livery: usize },
    /// A bus of the fleet: its livery, a service, selling or giving it back.
    Vehicle { id: u32, livery: usize },
    /// Something that cannot be undone.
    Confirm { what: Confirm },
    /// A bid on a tender of the concession market (its price, of the reference).
    Bid { tender: u32, price: f32 },
}

#[derive(Clone, Debug)]
pub(super) enum Confirm {
    Sell(u32),
    Dismiss(u32),
    RemoveLine(String),
}

pub struct CompanyView {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// The driver's companies (None: not read yet), and for which driver.
    companies: Option<Vec<Company>>,
    companies_for: Option<String>,
    pub(super) company: Option<Company>,
    pub(super) tab: usize,
    pub(super) wizard: Option<wizard::Wizard>,
    /// The market's buses with their kinds, for how many installed buses, and how far the
    /// reading is.
    pub(super) market: Option<Vec<MarketBus>>,
    market_for: Option<usize>,
    market_busy: bool,
    pub(super) today: Option<Today>,
    today_asked: Option<(String, String)>,
    pub(super) own: Vec<core::lines::OwnLine>,
    own_for: Option<String>,
    /// Today's plan (None: to be made again).
    pub(super) plan: Option<Plan>,
    closing: bool,
    pub(super) reports: Option<Vec<DayReport>>,
    pub(super) dialog: Option<Dialog>,
    pub(super) fleet: fleet::FleetView,
    pub(super) people: people::PeopleView,
    pub(super) lines: lines::LinesView,
    pub(super) money: money::MoneyView,
    pub(super) planning: planning::PlanningView,
    /// Raised whenever the company changed (the planning keeps its plans until then).
    pub(super) generation: u64,
    pub(super) career: career::CareerView,
    pub(super) depot: depot::DepotView,
    pub(super) tenders: concessions::TendersView,
    pub(super) map: map::FleetMap,
}

impl Default for CompanyView {
    fn default() -> Self {
        let (tx, rx) = channel();
        CompanyView {
            tx,
            rx,
            companies: None,
            companies_for: None,
            company: None,
            tab: 0,
            wizard: None,
            market: None,
            market_for: None,
            market_busy: false,
            today: None,
            today_asked: None,
            own: Vec::new(),
            own_for: None,
            plan: None,
            closing: false,
            reports: None,
            dialog: None,
            fleet: Default::default(),
            people: Default::default(),
            lines: Default::default(),
            money: Default::default(),
            planning: Default::default(),
            generation: 0,
            career: Default::default(),
            depot: Default::default(),
            tenders: Default::default(),
            map: Default::default(),
        }
    }
}

fn spawn(tx: &Sender<Msg>, f: impl FnOnce() -> Msg + Send + 'static) {
    let tx = tx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
}

/// The data folder (`~/.openomsi`).
fn data() -> std::path::PathBuf {
    core::data_dir()
}

// --- money and dates as the pages show them -------------------------------------------------

/// Whole euros, grouped as the language groups them ("€1,234,567", "€ 1.234.567",
/// "1.234.567 €", "1 234 567 €").
pub(super) fn eur(c: Cents) -> String {
    eur_in(c, &omsi_ui::i18n::language())
}

fn eur_in(c: Cents, lang: &str) -> String {
    let neg = c < 0;
    let whole = ((c.abs() as f64) / 100.0).round() as i64;
    let sep = match lang {
        "" | "en" => ',',
        "nl" | "de" | "it" | "es" | "pt" | "tr" => '.',
        _ => ' ',
    };
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(sep);
        }
        grouped.push(ch);
    }
    let sign = if neg { "-" } else { "" };
    match lang {
        "" | "en" => format!("{sign}€{grouped}"),
        "nl" => format!("{sign}€ {grouped}"),
        _ => format!("{sign}{grouped} €"),
    }
}

/// Euros with their cents (a fare, a price per kilometre).
pub(super) fn eur_cents(c: f64) -> String {
    let lang = omsi_ui::i18n::language();
    let v = format!("{:.2}", c / 100.0);
    match lang.as_str() {
        "" | "en" => format!("€{v}"),
        "nl" => format!("€ {}", v.replace('.', ",")),
        _ => format!("{} €", v.replace('.', ",")),
    }
}

/// A number grouped like money (kilometres, passengers).
pub(super) fn grouped(n: f64) -> String {
    let s = eur((n * 100.0).round() as Cents);
    s.replace('€', "").trim().to_string()
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Tue 5 Mar 1989".
pub(super) fn day_label(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (y, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {} {}", omsi_ui::tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, omsi_ui::tr(MONTHS[(m as usize).clamp(1, 12) - 1]), y)
}

/// "Mar 1989" of a `YYYY-MM`.
pub(super) fn month_label(month: &str) -> String {
    let y = month.get(..4).unwrap_or("");
    let m: usize = month.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!("{} {}", omsi_ui::tr(MONTHS[m.clamp(1, 12) - 1]), y)
}

// --- shared pieces ----------------------------------------------------------------------------

/// A section: its hairline box and its name in capitals; returns the room inside.
pub(super) fn section(ui: &mut Ui, r: Rect, title: &str) -> Rect {
    ui.card(r);
    ui.text_in(&omsi_ui::tr(title).to_uppercase(), Rect::new(r.x + 16.0, r.y + 12.0, r.w - 32.0, 14.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    Rect::new(r.x + 16.0, r.y + 36.0, r.w - 32.0, (r.h - 48.0).max(0.0))
}

/// A figure: its name, the value in big type and a line under it.
pub(super) fn figure(ui: &mut Ui, r: Rect, label: &str, value: &str, under: &str, c: Color) {
    ui.card(r);
    ui.text_in(&omsi_ui::tr(label).to_uppercase(), Rect::new(r.x + 14.0, r.y + 10.0, r.w - 28.0, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    let px = if r.w < 170.0 { 20.0 } else { 24.0 };
    ui.text_in(value, Rect::new(r.x + 14.0, r.y + 28.0, r.w - 28.0, 30.0), px, Weight::Bold, c, Align::Left);
    if !under.is_empty() {
        ui.text_in(under, Rect::new(r.x + 14.0, r.y + 60.0, r.w - 28.0, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
    }
}

/// A thin bar of 0..1.
pub(super) fn meter(ui: &mut Ui, r: Rect, frac: f64, c: Color) {
    ui.p().rounded(r, r.h * 0.5, HAIRLINE);
    let w = (r.w * frac.clamp(0.0, 1.0) as f32).max(r.h);
    ui.p().rounded(Rect::new(r.x, r.y, w, r.h), r.h * 0.5, c);
}

/// The colour of a 0-100 condition or satisfaction.
pub(super) fn grade(v: f64) -> Color {
    if v >= 65.0 {
        OK
    } else if v >= 40.0 {
        WARN
    } else {
        DANGER
    }
}

/// The company's mark: its short name on its colours.
pub(super) fn monogram(ui: &mut Ui, r: Rect, c: &Company) {
    let main = super::ownlines::colour_of(&c.colours[0]);
    let second = super::ownlines::colour_of(&c.colours[1]);
    ui.p().rounded(r, RADIUS, main);
    ui.p().rounded(Rect::new(r.x, r.bottom() - r.h * 0.22, r.w, r.h * 0.22), RADIUS * 0.5, second);
    ui.p().rect(Rect::new(r.x, r.bottom() - r.h * 0.22, r.w, r.h * 0.11), second);
    ui.text_in(&c.short, Rect::new(r.x, r.y, r.w, r.h * 0.8), (r.h * 0.34).min(18.0), Weight::Black, super::ownlines::ink_on(main), Align::Center);
}

/// A plain text line, cut to fit.
pub(super) fn line(ui: &mut Ui, r: Rect, text: &str, px: f32, c: Color) {
    ui.text_in(text, r, px, Weight::Regular, c, Align::Left);
}

// --- the page --------------------------------------------------------------------------------

/// What the page asks for in the background, and what came.
fn work(l: &mut Launcher) {
    let profile = l.state.config.profile.clone();
    let view = &mut l.company;
    while let Ok(m) = view.rx.try_recv() {
        match m {
            Msg::Companies(p, list) => {
                if p == profile {
                    // (the one open stays open; else the first)
                    let keep = view.company.as_ref().map(|c| c.id.clone());
                    view.company = keep.and_then(|id| list.iter().find(|c| c.id == id).cloned()).or_else(|| list.first().cloned());
                    view.companies = Some(list);
                    view.plan = None;
                }
            }
            Msg::Market(n, list) => {
                view.market = Some(list);
                view.market_for = Some(n);
                view.market_busy = false;
            }
            Msg::Lines { map, date, result } => {
                let (lines, error) = match result {
                    Ok(l) => (l, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                view.today = Some(Today { map, date, lines, error });
                view.plan = None;
            }
            Msg::Own(map, own) => {
                if view.own_for.as_deref() == Some(map.as_str()) {
                    view.own = own;
                }
            }
            Msg::Closed(r) => {
                view.closing = false;
                match r {
                    Ok((c, reports)) => {
                        if let Some(list) = view.companies.as_mut() {
                            if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                                *x = c.clone();
                            }
                        }
                        view.company = Some(c);
                        view.reports = Some(reports);
                        view.plan = None;
                    }
                    Err(e) => l.state.set_status(e, true),
                }
            }
        }
    }
    let view = &mut l.company;
    if view.companies_for.as_deref() != Some(profile.as_str()) {
        view.companies_for = Some(profile.clone());
        view.companies = None;
        view.company = None;
        let p = profile.clone();
        spawn(&view.tx, move || Msg::Companies(p.clone(), co::store::list(&data(), &p)));
    }
    // the timetable of the company's day, and the player's own lines of its map
    if let Some(c) = view.company.as_ref() {
        let key = (c.map.clone(), c.date.clone());
        if view.today_asked.as_ref() != Some(&key) {
            view.today_asked = Some(key.clone());
            let (map, date) = key;
            spawn(&view.tx, move || {
                let result = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
                Msg::Lines { map, date, result }
            });
        }
        if view.own_for.as_deref() != Some(c.map.as_str()) {
            view.own_for = Some(c.map.clone());
            view.own.clear();
            let map = c.map.clone();
            spawn(&view.tx, move || Msg::Own(map.clone(), core::lines::own_lines_of_map(&map)));
        }
    }
    // today's plan (the roster's, see `planning`), and the company's day for the game: its
    // tours run on the map with the company's buses while the player drives
    let view = &mut l.company;
    if view.plan.is_none() {
        view.generation += 1;
        if let (Some(c), Some(t)) = (view.company.as_ref(), view.today.as_ref()) {
            if t.map == c.map && t.date == c.date {
                let tours = co::network::tours_of_day(c, &t.lines);
                let day = co::plan::day_plan(c, &c.date, tours, &[], &[], false);
                view.plan = Some(day.to_plan());
                if let Err(e) = co::plan::save_live_plan(&data(), &co::plan::live_plan(c, &day)) {
                    log::warn!("company: the day's plan for the game: {e:#}");
                }
            }
        }
    }
}

/// The market's buses (read once, again when the installed buses changed).
pub(super) fn ask_market(l: &mut Launcher) {
    let n = l.state.vehicles.len();
    let view = &mut l.company;
    if view.market_busy || view.market_for == Some(n) {
        return;
    }
    view.market_busy = true;
    view.market_for = Some(n);
    let vehicles = l.state.vehicles.clone();
    spawn(&view.tx, move || Msg::Market(vehicles.len(), co::market::market_of(&vehicles)));
}

/// The market is being read.
pub(super) fn market_busy(l: &Launcher) -> bool {
    l.company.market_busy
}

/// After a change made on a page: saved, and today's plan made again.
pub(super) fn changed(l: &mut Launcher) {
    let view = &mut l.company;
    view.plan = None;
    if let Some(c) = view.company.as_ref() {
        if let Some(list) = view.companies.as_mut() {
            match list.iter_mut().find(|x| x.id == c.id) {
                Some(x) => *x = c.clone(),
                None => list.push(c.clone()),
            }
        }
        if let Err(e) = co::store::save(&data(), c) {
            l.state.set_status(format!("{e:#}"), true);
        }
    }
}

/// Do something to the company; its refusal is said in the status line.
pub(super) fn act<T>(l: &mut Launcher, f: impl FnOnce(&mut Company) -> Result<T, &'static str>) -> Option<T> {
    let c = l.company.company.as_mut()?;
    match f(c) {
        Ok(v) => {
            changed(l);
            Some(v)
        }
        Err(e) => {
            l.state.set_status(omsi_ui::tr(e).into_owned(), true);
            None
        }
    }
}

/// The company's page: the founding wizard, or the company with its tabs.
pub fn draw(l: &mut Launcher, area: Rect) {
    work(l);
    depot::tick(l);
    if l.company.companies.is_none() {
        l.ui.text_in("Reading your companies…", Rect::new(area.x, area.y, area.w, 30.0), 14.0, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    if l.company.wizard.is_some() || l.company.company.is_none() {
        if l.company.wizard.is_none() {
            l.company.wizard = Some(wizard::Wizard::new(l));
        }
        wizard::draw(l, area);
        return;
    }
    // a dialog lies over the pages: they get no mouse meanwhile
    let modal = l.company.dialog.is_some() || l.company.reports.is_some() || l.company.closing;
    let saved = modal.then(|| {
        let i = l.ui.input.clone();
        l.ui.input.mouse = Vec2::new(-1e4, -1e4);
        l.ui.input.pressed = false;
        l.ui.input.released = false;
        l.ui.input.wheel = Vec2::ZERO;
        l.ui.input.keys.clear();
        l.ui.input.text.clear();
        i
    });
    let body = strip(l, area);
    match l.company.tab {
        1 => fleet::draw(l, body),
        2 => people::draw(l, body),
        3 => lines::draw(l, body),
        4 => money::draw(l, body),
        5 => planning::draw(l, body),
        6 => career::draw(l, body),
        DEPOT_TAB => depot::draw(l, body),
        CONCESSIONS_TAB => concessions::draw(l, body),
        MAP_TAB => map::draw(l, body),
        _ => overview::draw(l, body),
    }
    if let Some(i) = saved {
        l.ui.input = i;
        if l.company.closing {
            closing_cover(l);
        } else if l.company.reports.is_some() {
            report_dialog(l);
        } else {
            dialog(l);
        }
    }
}

/// The company's strip over its tabs: its mark, its name and where it is at home, and the
/// tabs. Returns the room under it.
fn strip(l: &mut Launcher, area: Rect) -> Rect {
    let Some(c) = l.company.company.clone() else { return area };
    let mark = Rect::new(area.x, area.y, 44.0, 44.0);
    monogram(&mut l.ui, mark, &c);
    let tabs_w = (area.w * 0.66).clamp(380.0, 860.0);
    let text_w = (area.w - tabs_w - 70.0).max(80.0);
    l.ui.text_in(&c.name, Rect::new(mark.right() + 14.0, area.y, text_w, 24.0), 18.0, Weight::Bold, TEXT, Align::Left);
    let map = if c.map_name.is_empty() { super::state::short_map(&c.map) } else { c.map_name.clone() };
    let sub = format!("{}  ·  {}  ·  {}", map, c.depot, omsi_ui::tr(c.difficulty.label()));
    l.ui.text_in(&sub, Rect::new(mark.right() + 14.0, area.y + 25.0, text_w, 18.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
    let labels: Vec<String> = TABS.iter().map(|t| omsi_ui::tr(t).into_owned()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut tab = l.company.tab;
    if l.ui.segmented("company-tabs", Rect::new(area.right() - tabs_w, area.y + 4.0, tabs_w, ROW), &mut tab, &refs) {
        l.company.tab = tab;
    }
    Rect::new(area.x, area.y + 60.0, area.w, (area.h - 60.0).max(0.0))
}

/// What the page keeps in its sheet's head: the company's day and "Close the day". Returns
/// where it begins.
pub fn head_tools(l: &mut Launcher, r: Rect) -> f32 {
    let Some(c) = l.company.company.as_ref() else { return r.right() };
    if l.company.wizard.is_some() {
        return r.right();
    }
    let date = day_label(&c.date);
    let week_w = 92.0;
    let close_w = 170.0;
    let x_week = r.right() - week_w;
    let x_close = x_week - 10.0 - close_w;
    let busy = l.company.closing || l.state.in_game();
    if l.ui.button("company-week", Rect::new(x_week, r.y, week_w, r.h), "7 days", None, ButtonKind::Normal) && !busy {
        close(l, 7);
    }
    l.ui.tooltip(Rect::new(x_week, r.y, week_w, r.h), "Close the next seven days one after the other");
    if l.ui.button("company-close", Rect::new(x_close, r.y, close_w, r.h), "Close the day", Some("check_circle"), ButtonKind::Primary) && !busy {
        close(l, 1);
    }
    let dw = l.ui.width(&date, 15.0, Weight::Bold).max(l.ui.width(&omsi_ui::tr("Company day"), 11.0, Weight::Bold)) + 8.0;
    let dx = x_close - 16.0 - dw;
    l.ui.text_in(&omsi_ui::tr("Company day").to_uppercase(), Rect::new(dx, r.y, dw, 16.0), 10.0, Weight::Bold, TEXT_DIM, Align::Right);
    l.ui.text_in(&date, Rect::new(dx, r.y + 16.0, dw, 22.0), 15.0, Weight::Bold, TEXT, Align::Right);
    dx
}

/// Close `n` days on a thread of their own (the company as it is now; the page shows the
/// result when it is back).
fn close(l: &mut Launcher, n: usize) {
    let Some(c) = l.company.company.clone() else { return };
    l.company.closing = true;
    spawn(&l.company.tx, move || {
        let mut c = c;
        let mut reports = Vec::new();
        for _ in 0..n {
            match co::store::close_day(&data(), &mut c) {
                Ok(r) => reports.push(r),
                Err(e) => return Msg::Closed(Err(format!("{e:#}"))),
            }
        }
        Msg::Closed(Ok((c, reports)))
    });
}

fn closing_cover(l: &mut Launcher) {
    let size = l.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    l.ui.solid(full);
    l.ui.p().rect(full, Color::rgba(0, 0, 0, 0.45));
    let r = Rect::new((size.x - 320.0) * 0.5, (size.y - 90.0) * 0.5, 320.0, 90.0);
    l.ui.panel(r);
    l.ui.text_in("Closing the day…", Rect::new(r.x, r.y + 18.0, r.w, 24.0), 16.0, Weight::Bold, TEXT, Align::Center);
    l.ui.progress(Rect::new(r.x + 40.0, r.y + 58.0, r.w - 80.0, 6.0), 1.0, true);
}

/// A dialog's panel in the middle of the window; returns the room inside.
pub(super) fn dialog_panel(l: &mut Launcher, w: f32, h: f32, icon: &str, title: &str) -> Rect {
    let size = l.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    l.ui.solid(full);
    l.ui.p().rect(full, Color::rgba(0, 0, 0, 0.62));
    let w = (size.x - 48.0).min(w);
    let h = (size.y - 48.0).min(h);
    let r = Rect::new((size.x - w) * 0.5, (size.y - h) * 0.5, w, h);
    l.ui.panel(r);
    let inner = Rect::new(r.x + 24.0, r.y + 20.0, r.w - 48.0, r.h - 40.0);
    l.ui.icon(icon, Vec2::new(inner.x + 12.0, inner.y + 14.0), 22.0, accent_2());
    l.ui.text_in(title, Rect::new(inner.x + 34.0, inner.y, inner.w - 34.0, 28.0), 18.0, Weight::Bold, TEXT, Align::Left);
    Rect::new(inner.x, inner.y + 42.0, inner.w, inner.h - 42.0)
}

fn dialog(l: &mut Launcher) {
    match &l.company.dialog {
        Some(Dialog::New { .. }) | Some(Dialog::Used { .. }) | Some(Dialog::Vehicle { .. }) => fleet::dialog(l),
        Some(Dialog::Confirm { what }) => {
            let what = what.clone();
            confirm_dialog(l, what);
        }
        Some(Dialog::Bid { .. }) => concessions::dialog(l),
        None => {}
    }
}

fn confirm_dialog(l: &mut Launcher, what: Confirm) {
    let (title, text, button) = {
        let Some(c) = l.company.company.as_ref() else { return };
        match &what {
            Confirm::Sell(id) => {
                let Some(v) = c.vehicle(*id) else {
                    l.company.dialog = None;
                    return;
                };
                let amount = co::market::sale_offer(c, v);
                let text = match v.tenure {
                    co::Tenure::Owned { .. } => omsi_ui::tr("A dealer pays %{amount} for %{bus}.").replace("%{amount}", &eur(amount)).replace("%{bus}", &format!("{} {}", v.number, v.name)),
                    co::Tenure::Leased { .. } => omsi_ui::tr("Giving the leased bus back early costs %{amount} (three monthly rates).").replace("%{amount}", &eur(-amount)),
                    co::Tenure::Rented { .. } => omsi_ui::tr("The rented bus goes back today; the days rented are paid.").into_owned(),
                };
                let button = if matches!(v.tenure, co::Tenure::Owned { .. }) { "Sell" } else { "Give back" };
                (format!("{} {}", v.number, v.name), text, button)
            }
            Confirm::Dismiss(id) => {
                let Some(e) = c.employee(*id) else {
                    l.company.dialog = None;
                    return;
                };
                let r = co::economy::rules(c.difficulty);
                let mut text = omsi_ui::tr("%{name} works %{days} more days (the notice) and then leaves.").replace("%{name}", &e.name).replace("%{days}", &r.notice_days.to_string());
                if r.severance_months_per_year > 0.0 {
                    text.push(' ');
                    text.push_str(&omsi_ui::tr("They are paid half a month's wage for every year they worked here."));
                }
                (e.name.clone(), text, "Dismiss")
            }
            Confirm::RemoveLine(name) => {
                let number = c.lines.iter().find(|x| &x.name == name).map(|x| x.number.clone()).unwrap_or_default();
                (omsi_ui::tr("Line %{n}").replace("%{n}", &number), omsi_ui::tr("The company stops running this line from today. Its buses and drivers stay.").into_owned(), "Stop running it")
            }
        }
    };
    let inner = dialog_panel(l, 480.0, 200.0, "warning", &title);
    l.ui.paragraph(&text, Vec2::new(inner.x, inner.y), inner.w, 13.5, Weight::Regular, TEXT_SOFT);
    let by = inner.bottom() - 38.0;
    if l.ui.button("company-confirm-no", Rect::new(inner.right() - 300.0, by, 110.0, 38.0), "Cancel", None, ButtonKind::Normal) {
        l.company.dialog = None;
    }
    if l.ui.button("company-confirm-yes", Rect::new(inner.right() - 180.0, by, 180.0, 38.0), button, None, ButtonKind::Danger) {
        l.company.dialog = None;
        match what {
            Confirm::Sell(id) => {
                if act(l, |c| co::market::sell(c, id)).is_some() {
                    l.company.fleet.selected = None;
                }
            }
            Confirm::Dismiss(id) => {
                if let Some(until) = act(l, |c| co::staff::dismiss(c, id)) {
                    l.state.set_status(omsi_ui::tr("Their last day is %{date}.").replace("%{date}", &day_label(&until)), false);
                }
            }
            Confirm::RemoveLine(name) => {
                act(l, |c| {
                    co::network::remove_line(c, &name);
                    Ok(())
                });
            }
        }
    }
    if l.ui.input.keys.contains(&super::ui::Key::Escape) {
        l.company.dialog = None;
    }
}

/// What a note of the day's report says.
fn note_text(n: &Note) -> (String, Color) {
    match n {
        Note::Breakdown { number, until, cost } => (
            omsi_ui::tr("Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.").replace("%{n}", number).replace("%{date}", &day_label(until)).replace("%{amount}", &eur(*cost)),
            DANGER.lighten(0.25),
        ),
        Note::Service { number } => (omsi_ui::tr("Bus %{n} is due for its service: in the workshop tomorrow.").replace("%{n}", number), TEXT_SOFT),
        Note::Returned { number, name } => (omsi_ui::tr("Bus %{n} (%{name}) went back: its lease or rental ended.").replace("%{n}", number).replace("%{name}", name), TEXT_SOFT),
        Note::LoanPaid { purpose } => (omsi_ui::tr("A loan is paid off: %{what}.").replace("%{what}", purpose), OK),
        Note::Month { month, result } => (
            omsi_ui::tr("%{month} is closed: wages, leases, insurance, the depot and loan rates are booked. The month's result: %{amount}.").replace("%{month}", &month_label(month)).replace("%{amount}", &eur(*result)),
            if *result >= 0 { OK } else { WARN },
        ),
        Note::Built { area } => (omsi_ui::tr("The depot's building work is done: %{what}.").replace("%{what}", &omsi_ui::tr(area)), OK),
        Note::BayWait { number } => (omsi_ui::tr("Bus %{n} waits for a free workshop bay.").replace("%{n}", number), WARN),
        Note::Won { number, until } => (omsi_ui::tr("The concession for line %{n} is yours until %{date}.").replace("%{n}", number).replace("%{date}", &day_label(until)), OK),
        Note::Lost { number, winner } => (omsi_ui::tr("%{who} won the tender for line %{n}.").replace("%{n}", number).replace("%{who}", winner), WARN),
        Note::Ended { number } => (omsi_ui::tr("The concession for line %{n} has ended: the line is no longer yours.").replace("%{n}", number), DANGER.lighten(0.25)),
    }
}

fn staff_text(n: &co::staff::StaffNote) -> (String, Color) {
    use co::staff::StaffNote as S;
    match n {
        S::Sick { name, until } => (omsi_ui::tr("%{name} is ill until %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), WARN),
        S::Holiday { name, until } => (omsi_ui::tr("%{name} is on holiday until %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), TEXT_SOFT),
        S::Resigned { name, until } => (omsi_ui::tr("%{name} has resigned; their last day is %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), DANGER.lighten(0.25)),
        S::Unhappy { name } => (omsi_ui::tr("%{name} is unhappy with their pay or their hours.").replace("%{name}", name), WARN),
        S::Left { name } => (omsi_ui::tr("%{name} has left the company.").replace("%{name}", name), TEXT_SOFT),
    }
}

/// The report of the day (or days) just closed.
fn report_dialog(l: &mut Launcher) {
    let Some(reports) = l.company.reports.clone() else { return };
    let Some(last) = reports.last().cloned() else {
        l.company.reports = None;
        return;
    };
    let first = reports.first().cloned().unwrap_or_default();
    let title = if reports.len() > 1 { format!("{} – {}", day_label(&first.date), day_label(&last.date)) } else { day_label(&last.date) };
    let inner = dialog_panel(l, 760.0, 640.0, "receipt_long", &title);
    let sum = |f: &dyn Fn(&DayReport) -> i64| reports.iter().map(f).sum::<i64>();
    let result = sum(&|r| r.result);
    let income = sum(&|r| r.income);
    let expenses = sum(&|r| r.expenses);
    let tours = sum(&|r| r.tours as i64);
    let covered = sum(&|r| r.covered as i64);
    let trips = sum(&|r| r.trips as i64);
    let dropped = sum(&|r| r.dropped as i64);
    let late = sum(&|r| r.late as i64);
    let pax = sum(&|r| r.passengers as i64);
    let km: f64 = reports.iter().map(|r| r.km).sum();
    let measured = sum(&|r| r.measured as i64);
    let penalties = sum(&|r| r.penalties);
    let rep: f64 = reports.iter().map(|r| r.reputation_change).sum();
    // the figures
    let gap = 10.0;
    let fw = (inner.w - 3.0 * gap) / 4.0;
    let fy = inner.y;
    figure(&mut l.ui, Rect::new(inner.x, fy, fw, 82.0), "Result", &eur(result), &format!("{} {}  ·  {} {}", omsi_ui::tr("in"), eur(income), omsi_ui::tr("out"), eur(expenses)), if result >= 0 { OK } else { DANGER.lighten(0.2) });
    figure(&mut l.ui, Rect::new(inner.x + fw + gap, fy, fw, 82.0), "Tours run", &format!("{covered} / {tours}"), &omsi_ui::tr("%{n} trips dropped").replace("%{n}", &dropped.to_string()), if covered == tours { OK } else { WARN });
    let punct = if trips - dropped > 0 { format!("{:.0} %", 100.0 * (trips - dropped - late).max(0) as f64 / (trips - dropped) as f64) } else { "–".into() };
    figure(&mut l.ui, Rect::new(inner.x + 2.0 * (fw + gap), fy, fw, 82.0), "On time", &punct, &omsi_ui::tr("%{n} trips late").replace("%{n}", &late.to_string()), TEXT);
    figure(&mut l.ui, Rect::new(inner.x + 3.0 * (fw + gap), fy, fw, 82.0), "Passengers", &grouped(pax as f64), &format!("{} km", grouped(km.round())), TEXT);
    // the lines and what happened
    let mut y = fy + 96.0;
    let mut facts: Vec<(String, Color)> = Vec::new();
    if measured > 0 {
        facts.push((omsi_ui::tr("You drove %{n} trips yourself: booked as measured.").replace("%{n}", &measured.to_string()), accent_2()));
    }
    if penalties > 0 {
        facts.push((omsi_ui::tr("Contract penalties: %{amount}.").replace("%{amount}", &eur(penalties)), WARN));
    }
    if rep.abs() >= 0.05 {
        let t = if rep > 0.0 { "Your reputation rose to %{n}." } else { "Your reputation fell to %{n}." };
        facts.push((omsi_ui::tr(t).replace("%{n}", &format!("{:.1}", last.reputation)), if rep > 0.0 { OK } else { WARN }));
    }
    for r in &reports {
        facts.extend(r.notes.iter().map(note_text));
        facts.extend(r.staff.iter().map(staff_text));
    }
    if tours == 0 {
        facts.insert(0, (omsi_ui::tr("No tours ran: add lines on the Lines page, and buy buses and hire drivers for them.").into_owned(), TEXT_DIM));
    }
    let list = Rect::new(inner.x, y, inner.w, inner.bottom() - 50.0 - y);
    let lines_by: Vec<(String, String, u32, u32, u32, Cents)> = {
        let mut v: Vec<(String, String, u32, u32, u32, Cents)> = Vec::new();
        for r in &reports {
            for ld in &r.lines {
                match v.iter_mut().find(|x| x.0 == ld.line) {
                    Some(x) => {
                        x.2 += ld.tours;
                        x.3 += ld.covered;
                        x.4 += ld.dropped;
                        x.5 += ld.revenue;
                    }
                    None => v.push((ld.line.clone(), ld.number.clone(), ld.tours, ld.covered, ld.dropped, ld.revenue)),
                }
            }
        }
        v
    };
    l.ui.scroll_area("company-report", list, &mut |ui, v| {
        let mut yy = v.y;
        if !lines_by.is_empty() {
            ui.text_in(&omsi_ui::tr("Lines").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
            yy += 20.0;
            for (_, number, t, c, d, rev) in &lines_by {
                let w = plate(ui, Vec2::new(v.x, yy + 4.0), number, 22.0);
                let text = omsi_ui::tr("%{c} of %{t} tours  ·  %{d} trips dropped").replace("%{c}", &c.to_string()).replace("%{t}", &t.to_string()).replace("%{d}", &d.to_string());
                line(ui, Rect::new(v.x + w + 12.0, yy, v.w * 0.6, 30.0), &text, 13.0, TEXT_SOFT);
                ui.text_in(&eur(*rev), Rect::new(v.right() - 160.0, yy, 150.0, 30.0), 13.0, Weight::Bold, TEXT, Align::Right);
                yy += 32.0;
            }
            yy += 8.0;
        }
        if !facts.is_empty() {
            ui.text_in(&omsi_ui::tr("What happened").to_uppercase(), Rect::new(v.x, yy, v.w, 14.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
            yy += 22.0;
            for (t, c) in &facts {
                ui.p().circle(Vec2::new(v.x + 4.0, yy + 9.0), 3.0, *c);
                let h = ui.paragraph(t, Vec2::new(v.x + 16.0, yy), v.w - 30.0, 13.0, Weight::Regular, TEXT_SOFT);
                yy += h.max(18.0) + 6.0;
            }
        }
        yy - v.y + 8.0
    });
    y = inner.bottom() - 38.0;
    if l.ui.button("company-report-ok", Rect::new(inner.right() - 200.0, y, 200.0, 38.0), "Close the report", None, ButtonKind::Primary) || l.ui.input.keys.contains(&super::ui::Key::Escape) || l.ui.input.keys.contains(&super::ui::Key::Enter) {
        l.company.reports = None;
    }
}

/// A line number's plate: a company line's colour, or the yellow of the timetable's lines.
pub(super) fn plate(ui: &mut Ui, at: Vec2, number: &str, h: f32) -> f32 {
    let px = h * 0.6;
    let w = (ui.width(number, px, Weight::Black) + h * 0.7).max(h * 1.8);
    let r = Rect::new(at.x, at.y, w, h);
    ui.p().rounded(r, 5.0f32.min(h * 0.25), LINE);
    ui.text_in(number, r, px, Weight::Black, ON_LINE, Align::Center);
    w
}

/// A company line's plate (its own colour when it has one).
pub(super) fn line_plate(ui: &mut Ui, at: Vec2, l: &co::CompanyLine, h: f32) -> f32 {
    if l.colour.trim().is_empty() {
        plate(ui, at, &l.number, h)
    } else {
        super::ownlines::plate(ui, at, &l.number, &l.colour, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_grouped_as_the_language_groups_it() {
        assert_eq!(eur_in(123_456_789, ""), "€1,234,568");
        assert_eq!(eur_in(-5_000_00, ""), "-€5,000");
        assert_eq!(eur_in(0, "en"), "€0");
        assert_eq!(eur_in(123_456_789, "nl"), "€ 1.234.568");
        assert_eq!(eur_in(123_456_789, "de"), "1.234.568 €");
        assert_eq!(eur_in(123_456_789, "fr"), "1 234 568 €");
        assert_eq!(day_label("nonsense"), "nonsense");
    }
}

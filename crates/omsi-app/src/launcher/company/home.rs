//! The bus company's start screen (Luc: "the start screen is not clear: make a nice start
//! screen with tiles of all the functions you can go to"): every page of the company as a
//! tile, grouped by theme under a title - at a glance, operations, people, business, the
//! company - each with its icon, its name and what matters there now from the company's own
//! figures, and a badge where something waits (the inbox's unread messages, an alert of the
//! overview's on the page that mends it). Under the tiles, the ways to what otherwise hides
//! behind a page's button or a dialog: the dealer, the advertising, a loan, a new line, the
//! licences, my duties, the rankings, the statistics, the last day's report, another company.
//!
//! The company opens on it (`HOME_TAB`); every page has its way back to it in the strip under
//! the bar (`super::nav`) and with Escape. On a large window the tiles keep a readable size
//! in the middle; on a low one they shrink to what fits, and scroll when even that is too
//! much. A phone has them two by two, scrolled.

use super::super::theme::*;
use super::super::ui::{id_of, Ui};
use super::super::Launcher;
use super::kit;
use super::{eur, grouped, CONCESSIONS_TAB, CONTRACTS_TAB, DEPOT_TAB, INBOX_TAB, MAP_TAB, RESEARCH_TAB, WORKSHOP_TAB};
use glam::Vec2;
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::{self as co, Company, Role};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

// --- what the tiles say -------------------------------------------------------------------------

/// Where a tile (or a way under the tiles) goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum To {
    /// A page of the company (its tab in `TABS`).
    Tab(usize),
    /// The fleet's dealer and its advertising.
    Dealer,
    Adverts,
    /// A loan's dialog, over the finances.
    Loan,
    /// The staff's applicants, their courses and the licences and type trainings.
    Hire,
    Courses,
    Licences,
    /// The career's parts: the player's own duties, the rankings, the statistics.
    MyDuties,
    Rankings,
    Statistics,
    /// The line editor, working for the company.
    NewLine,
    /// The report of the last day closed.
    Report,
    /// The company's settings (its name, mark, date and how it buys).
    Settings,
    /// The founding of another company.
    Found,
}

/// What a big tile shows under its words.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Extra {
    None,
    /// The overview's: the cash in big type and the last days' results as bars.
    Money { cash: String, colour: Color, days: Vec<Option<i64>> },
    /// The inbox's newest messages: title, short date, unread.
    Messages(Vec<(String, String, bool)>),
    /// The staff by role (the roles with someone in them, the drivers always).
    Roles(Vec<(&'static str, usize)>),
}

/// A tile of the start screen.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tile {
    pub to: To,
    /// Its name and icon (the page's).
    pub name: &'static str,
    pub icon: &'static str,
    /// What matters there now, each line in its colour (the first the strongest).
    pub lines: Vec<(String, Color)>,
    /// A count in its corner: unread messages, alerts.
    pub badge: Option<(String, Color)>,
    /// What the badge is about (the alerts' words), for the tooltip.
    pub tip: String,
    /// Two slots high.
    pub big: bool,
    pub extra: Extra,
}

impl Tile {
    fn new(to: To, name: &'static str, icon: &'static str) -> Tile {
        Tile { to, name, icon, lines: Vec::new(), badge: None, tip: String::new(), big: false, extra: Extra::None }
    }

    fn line(mut self, text: impl Into<String>, c: Color) -> Tile {
        let t = text.into();
        if !t.is_empty() {
            self.lines.push((t, c));
        }
        self
    }

    /// Slots it takes in its column.
    fn slots(&self) -> usize {
        if self.big {
            2
        } else {
            1
        }
    }
}

/// A section of the start screen: its title and its tiles.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Section {
    pub title: &'static str,
    pub tiles: Vec<Tile>,
}

/// What wants attention, page by page: the overview's alerts on the page that helps
/// (`overview::alert_text`) and the inbox's unread messages.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Marks {
    pub unread: usize,
    /// (the page, what the alert says, its colour)
    pub alerts: Vec<(usize, String, Color)>,
}

impl Marks {
    pub fn of(c: &Company, plan: Option<&Plan>) -> Marks {
        let unread = c.inbox.messages.iter().filter(|m| !m.read).count();
        let alerts = co::alerts(c, plan)
            .iter()
            .map(super::overview::alert_text)
            // (a bus going back is said on the overview, but is nothing to mend)
            .filter(|(_, _, colour, _)| *colour != TEXT_SOFT)
            .map(|(_, text, colour, tab)| (tab, text, colour))
            .collect();
        Marks { unread, alerts }
    }

    /// The badge of a page: its unread messages (the inbox), else how many alerts it has, in
    /// the colour of the worst; and the alerts' words.
    pub fn badge(&self, tab: usize) -> Option<(String, Color, String)> {
        if tab == INBOX_TAB {
            return (self.unread > 0).then(|| (count_text(self.unread), accent(), omsi_ui::tr("%{n} unread").replace("%{n}", &self.unread.to_string())));
        }
        let mine: Vec<&(usize, String, Color)> = self.alerts.iter().filter(|a| a.0 == tab).collect();
        if mine.is_empty() {
            return None;
        }
        let colour = if mine.iter().any(|a| a.2 != WARN) { DANGER } else { WARN };
        let tip = mine.iter().map(|a| a.1.as_str()).collect::<Vec<_>>().join("\n");
        Some((mine.len().to_string(), colour, tip))
    }
}

/// A count as a badge says it ("9+" past nine).
fn count_text(n: usize) -> String {
    if n > 9 {
        "9+".to_string()
    } else {
        n.to_string()
    }
}

/// A text with its `%{n}` filled.
fn n_of(key: &str, n: usize) -> String {
    omsi_ui::tr(key).replace("%{n}", &n.to_string())
}

/// A message's words in the interface's language, its `%{name}`s filled.
pub(super) fn filled(text: &str, args: &[(String, String)]) -> String {
    let mut t = omsi_ui::tr(text).into_owned();
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), v);
    }
    t
}

/// The results of the last 30 days up to yesterday, oldest first (None: not closed).
fn last_days(c: &Company) -> Vec<Option<i64>> {
    let Some(t) = co::dates::parse(&c.date) else { return Vec::new() };
    (1..=30).rev().map(|back| {
        let d = co::dates::fmt(t - back);
        c.history.iter().find(|h| h.date == d).map(|h| h.result)
    }).collect()
}

/// The start screen's sections as the company is now (`plan`: today's, when made).
pub(super) fn sections(c: &Company, plan: Option<&Plan>, marks: &Marks) -> Vec<Section> {
    let today = c.date.as_str();
    let mark = |mut t: Tile| {
        if let To::Tab(tab) = t.to {
            if let Some((n, colour, tip)) = marks.badge(tab) {
                t.badge = Some((n, colour));
                t.tip = tip;
            }
        }
        t
    };
    // at a glance: the overview and the inbox
    let month = c.month(&co::dates::month_of(today));
    let (res, res_c) = kit::signed(month.result());
    let mut overview = Tile::new(To::Tab(0), "Overview", super::ICONS[0]).line(omsi_ui::tr("This month: %{amount}").replace("%{amount}", &res), res_c);
    overview.big = true;
    overview.extra = Extra::Money { cash: eur(c.cash), colour: if c.cash >= 0 { TEXT } else { DANGER.lighten(0.2) }, days: last_days(c) };
    let mut inbox = Tile::new(To::Tab(INBOX_TAB), "Inbox", super::ICONS[INBOX_TAB]);
    inbox = if marks.unread > 0 { inbox.line(n_of("%{n} unread", marks.unread), accent_2()) } else { inbox.line(omsi_ui::tr("Nothing unread"), TEXT_DIM) };
    inbox.big = true;
    let newest: Vec<(String, String, bool)> = c.inbox.messages.iter().rev().take(8).map(|m| (filled(&m.title, &m.args), super::overview::short_date(&m.date), !m.read)).collect();
    inbox.extra = Extra::Messages(newest);
    // operations: the planning, the lines, the map, the fleet
    let mut planning = Tile::new(To::Tab(5), "Planning", super::ICONS[5]);
    planning = match plan {
        Some(p) if p.tours.iter().any(|t| !t.tour.unplanned) => {
            let n = p.tours.iter().filter(|t| !t.tour.unplanned).count();
            let covered = n - p.uncovered();
            let frac = covered as f64 / n as f64;
            let (buses, duties) = p.short_of();
            let t = planning.line(omsi_ui::tr("Covered today: %{c} of %{t} tours").replace("%{c}", &covered.to_string()).replace("%{t}", &n.to_string()), kit::share_colour(frac, false));
            if buses + duties > 0 {
                t.line(omsi_ui::tr("%{b} without a bus · %{d} without a driver").replace("%{b}", &buses.to_string()).replace("%{d}", &duties.to_string()), TEXT_DIM)
            } else {
                t.line(omsi_ui::tr("Every tour has its bus and drivers"), TEXT_DIM)
            }
        }
        Some(_) => planning.line(omsi_ui::tr("No tours run today."), TEXT_SOFT),
        None => planning.line(omsi_ui::tr("Reading the timetable…"), TEXT_DIM),
    };
    let now = co::clock::now(c);
    let running = c.lines.iter().filter(|x| co::network::in_service(x, now)).count();
    let mut lines = Tile::new(To::Tab(3), "Lines", super::ICONS[3]);
    lines = if c.lines.is_empty() {
        lines.line(omsi_ui::tr("No line yet: take one on"), WARN)
    } else {
        let t = lines.line(n_of("%{n} lines", c.lines.len()), TEXT_SOFT);
        if running < c.lines.len() {
            t.line(n_of("%{n} not in service yet", c.lines.len() - running), WARN)
        } else {
            t.line(omsi_ui::tr("All in service"), TEXT_DIM)
        }
    };
    let minute = c.clock.minute as i32;
    let on_road = plan.map_or(0, |p| p.tours.iter().filter(|t| !t.tour.unplanned && t.covered() && t.tour.from() <= minute && minute < t.tour.to()).count());
    let map = Tile::new(To::Tab(MAP_TAB), "Map", super::ICONS[MAP_TAB]).line(n_of("%{n} buses on the road now", on_road), if on_road > 0 { TEXT_SOFT } else { TEXT_DIM }).line(omsi_ui::tr("Every line and bus where it is"), TEXT_DIM);
    let held: Vec<&co::Vehicle> = c.fleet.iter().filter(|v| v.held_on(today)).collect();
    let in_shop = held.iter().filter(|v| v.in_workshop(today)).count();
    let mut fleet = Tile::new(To::Tab(1), "Fleet", super::ICONS[1]);
    fleet = if held.is_empty() {
        fleet.line(omsi_ui::tr("No bus yet: buy, lease or rent one"), WARN)
    } else {
        let t = fleet.line(n_of("%{n} buses", held.len()), TEXT_SOFT).line(n_of("%{n} ready today", held.len() - in_shop), TEXT_DIM);
        if in_shop > 0 {
            t.line(n_of("%{n} in the workshop", in_shop), WARN)
        } else {
            t
        }
    };
    // people: the staff, hiring, training
    let staff: Vec<&co::Employee> = c.staff.iter().filter(|e| e.employed_on(today)).collect();
    let here = staff.iter().filter(|e| !e.absent(today)).count();
    let mut people = Tile::new(To::Tab(2), "Staff", super::ICONS[2]);
    people = if staff.is_empty() {
        people.line(omsi_ui::tr("Nobody works here yet"), WARN)
    } else {
        people.line(n_of("%{n} people", staff.len()), TEXT_SOFT).line(n_of("%{n} at work today", here), TEXT_DIM)
    };
    people.big = true;
    let roles: Vec<(&'static str, usize)> = Role::ALL.iter().map(|r| (r.label(), staff.iter().filter(|e| e.role == *r).count())).filter(|(l, n)| *n > 0 || *l == Role::Driver.label()).collect();
    people.extra = Extra::Roles(roles);
    let applicants = co::staff::applicants(c).len();
    let hire = Tile::new(To::Hire, "Hire staff", "person").line(n_of("%{n} applicants this week", applicants), if applicants > 0 { TEXT_SOFT } else { TEXT_DIM });
    let learning = staff.iter().filter(|e| e.training_until.as_deref().is_some_and(|u| co::dates::between(today, u) >= 0)).count();
    let training = Tile::new(To::Courses, "Training", "badge").line(if learning > 0 { n_of("%{n} on a course today", learning) } else { omsi_ui::tr("Courses for drivers and staff").into_owned() }, if learning > 0 { TEXT_SOFT } else { TEXT_DIM });
    // business: the finances, the concessions, the contracts, the career
    let mut money = Tile::new(To::Tab(4), "Finances", super::ICONS[4]).line(omsi_ui::tr("This month: %{amount}").replace("%{amount}", &res), res_c);
    money = if c.debt() > 0 { money.line(omsi_ui::tr("Loans: %{amount}").replace("%{amount}", &eur(c.debt())), TEXT_DIM) } else { money.line(omsi_ui::tr("No loans"), TEXT_DIM) };
    let held_c = c.concessions.held.len();
    let open = c.concessions.tenders.iter().filter(|t| t.open()).count();
    let live = c.concessions.tenders.iter().filter(|t| t.running(now)).count();
    let mut concessions = Tile::new(To::Tab(CONCESSIONS_TAB), "Concessions", super::ICONS[CONCESSIONS_TAB]).line(n_of("%{n} concessions held", held_c), TEXT_SOFT).line(n_of("%{n} tenders open", open), TEXT_DIM);
    if live > 0 {
        concessions = concessions.line(n_of("%{n} auctions running now", live), accent_2());
        if concessions.badge.is_none() {
            concessions.badge = Some((live.to_string(), accent()));
            concessions.tip = n_of("%{n} auctions running now", live);
        }
    }
    let contracts = Tile::new(To::Tab(CONTRACTS_TAB), "Contracts", super::ICONS[CONTRACTS_TAB])
        .line(n_of("%{n} trophies", c.trophies.max(0) as usize), TEXT_SOFT)
        .line(if c.research.has(co::research::keys::WLAN) { omsi_ui::tr("Goals, and internet for the buses") } else { omsi_ui::tr("Goals with a reward, service contracts") }, TEXT_DIM);
    let lv = co::levels::level(c);
    let (xp, _, next) = co::levels::progress(c);
    let mut career = Tile::new(To::Tab(6), "Career", super::ICONS[6]).line(format!("{}  ·  {}", omsi_ui::tr("Level %{n}").replace("%{n}", &lv.to_string()), omsi_ui::tr(co::levels::title_of(lv))), TEXT_SOFT);
    if let Some(n) = next {
        career = career.line(omsi_ui::tr("%{xp} of %{next} points").replace("%{xp}", &grouped(xp as f64)).replace("%{next}", &grouped(n as f64)), TEXT_DIM);
    }
    // the company: its depot, workshop, research and settings
    let taken = c.fleet.iter().filter(|v| v.held_on(today)).count();
    let places = co::depot::places(c);
    let mut depot = Tile::new(To::Tab(DEPOT_TAB), "Depot", super::ICONS[DEPOT_TAB]).line(omsi_ui::tr("%{n} of %{m} places taken").replace("%{n}", &taken.to_string()).replace("%{m}", &places.to_string()), if taken > places { WARN } else { TEXT_SOFT });
    if !c.site.works.is_empty() {
        depot = depot.line(n_of("%{n} building works under way", c.site.works.len()), accent_2());
    }
    let waiting = c.site.jobs.iter().filter(|j| j.started.is_none()).count();
    let mut workshop = Tile::new(To::Tab(WORKSHOP_TAB), "Workshop", super::ICONS[WORKSHOP_TAB]).line(n_of("%{n} buses in the workshop", in_shop), TEXT_SOFT);
    if waiting > 0 {
        workshop = workshop.line(n_of("%{n} jobs waiting", waiting), WARN);
    } else {
        workshop = workshop.line(omsi_ui::tr("Spare parts and repairs"), TEXT_DIM);
    }
    let research = Tile::new(To::Tab(RESEARCH_TAB), "Research", super::ICONS[RESEARCH_TAB]).line(n_of("%{n} researched", c.research.done.len()), TEXT_SOFT).line(n_of("%{n} trophies to spend", c.trophies.max(0) as usize), TEXT_DIM);
    let settings = Tile::new(To::Settings, "Settings", "settings").line(omsi_ui::tr("Name, mark, date and buying"), TEXT_DIM);
    vec![
        Section { title: "At a glance", tiles: vec![mark(overview), mark(inbox)] },
        Section { title: "Operations", tiles: vec![mark(planning), mark(lines), mark(map), mark(fleet)] },
        Section { title: "People", tiles: vec![mark(people), hire, training] },
        Section { title: "Business", tiles: vec![mark(money), mark(concessions), mark(contracts), mark(career)] },
        Section { title: "Company", tiles: vec![mark(depot), mark(workshop), mark(research), settings] },
    ]
}

/// The ways under the tiles to what otherwise hides behind a page's button: (where, words,
/// icon).
pub(super) fn shortcuts(c: &Company) -> Vec<(To, &'static str, &'static str)> {
    let mut out = vec![
        (To::Dealer, "Dealer", "airport_shuttle"),
        (To::Adverts, "Advertising", "campaign"),
        (To::Loan, "Take a loan", "receipt_long"),
        (To::NewLine, "Make a new line", "alt_route"),
        (To::Licences, "Train drivers", "key"),
        (To::MyDuties, "My duties", "schedule"),
        (To::Rankings, "Rankings", "stars"),
        (To::Statistics, "Statistics", "trending_up"),
    ];
    if c.last_report.is_some() {
        out.push((To::Report, "Show the report", "description"));
    }
    out.push((To::Found, "Found another company", "add"));
    out
}

/// Go where a tile or a way under them leads.
pub(super) fn open(l: &mut Launcher, to: To) {
    match to {
        To::Tab(tab) => l.company.tab = tab,
        To::Dealer => {
            l.company.tab = 1;
            l.company.fleet.tab = 1;
        }
        To::Adverts => {
            l.company.tab = 1;
            l.company.fleet.tab = 2;
        }
        To::Loan => {
            l.company.tab = 4;
            super::bank::open_loan(l);
        }
        To::Hire => {
            l.company.tab = 2;
            super::people::to_applicants(l);
        }
        To::Courses => super::people::to_courses(l),
        To::Licences => super::people::to_training(l),
        To::MyDuties => {
            l.company.tab = 6;
            l.company.career.part = super::career::DUTIES;
        }
        To::Rankings => {
            l.company.tab = 6;
            l.company.career.part = 4;
        }
        To::Statistics => {
            l.company.tab = 6;
            l.company.career.part = 5;
        }
        To::NewLine => super::super::lineeditor::open_for_company(l, None),
        To::Report => {
            if let Some(rep) = l.company.company.as_ref().and_then(|c| c.last_report.clone()) {
                l.company.reports = Some(vec![rep]);
            }
        }
        To::Settings => {
            let date = l.company.company.as_ref().map(|c| c.date.clone()).unwrap_or_default();
            l.company.dialog = Some(super::Dialog::Settings { date });
        }
        To::Found => l.company.wizard = Some(super::wizard::Wizard::new(l)),
    }
}

// --- the layout -----------------------------------------------------------------------------------

/// The rows of tiles a column has.
const SLOTS: usize = 4;
/// A section's title over its column, the gap between tiles and between columns.
const TITLE_H: f32 = 26.0;
const GAP: f32 = 14.0;
const COL_GAP: f32 = 20.0;
/// The widest the screen grows (in the middle beyond), and a tile's least and most height.
const MAX_W: f32 = 1720.0;
const MIN_TILE_H: f32 = 84.0;
const MAX_TILE_H: f32 = 178.0;
/// The ways under the tiles: a row's height, the gap between rows and above them.
const MORE_H: f32 = 34.0;
const MORE_GAP: f32 = 8.0;
const MORE_TOP: f32 = 20.0;
/// How much wider the first column (at a glance) is than the others.
const FIRST_WEIGHT: f32 = 1.35;

/// The start screen laid out in its area on a desktop.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Grid {
    /// Each section's column (its title at its top).
    pub columns: Vec<Rect>,
    pub tile_h: f32,
    /// Where the ways under the tiles begin, and how wide they may run.
    pub more: Rect,
    /// All of it, from the titles to the last row of ways.
    pub height: f32,
}

impl Grid {
    /// The place of a tile `slot` rows down its column, `slots` high.
    pub fn tile(&self, col: usize, slot: usize, slots: usize) -> Rect {
        let c = self.columns[col];
        let y = c.y + TITLE_H + slot as f32 * (self.tile_h + GAP);
        Rect::new(c.x, y, c.w, self.tile_h * slots as f32 + GAP * (slots as f32 - 1.0))
    }
}

/// The grid for `n` columns in `area`, with `more_rows` rows of ways under it: the columns
/// share the width (the first wider), the tiles the height - between their least and most -
/// and what is left stays round it, a little more under it than over it.
pub(super) fn grid(area: Rect, n: usize, more_rows: usize) -> Grid {
    let w = area.w.min(MAX_W);
    let x0 = area.x + (area.w - w) * 0.5;
    let n = n.max(1);
    let unit = (w - COL_GAP * (n as f32 - 1.0)) / (FIRST_WEIGHT + (n as f32 - 1.0));
    let more_h = if more_rows == 0 { 0.0 } else { MORE_TOP + more_rows as f32 * MORE_H + (more_rows as f32 - 1.0) * MORE_GAP };
    let fixed = TITLE_H + (SLOTS as f32 - 1.0) * GAP + more_h;
    let tile_h = ((area.h - fixed) / SLOTS as f32).clamp(MIN_TILE_H, MAX_TILE_H).floor();
    let height = fixed + SLOTS as f32 * tile_h;
    let top = area.y + ((area.h - height) * 0.42).max(0.0).floor();
    let mut x = x0;
    let mut columns = Vec::with_capacity(n);
    for k in 0..n {
        let cw = if k == 0 { unit * FIRST_WEIGHT } else { unit };
        columns.push(Rect::new(x.floor(), top, cw.floor(), height - more_h));
        x += cw + COL_GAP;
    }
    let more = Rect::new(x0, top + height - more_h + if more_rows == 0 { 0.0 } else { MORE_TOP }, w, (more_h - MORE_TOP).max(0.0));
    Grid { columns, tile_h, more, height }
}

/// A way under the tiles: its width for its words and icon.
fn way_w(ui: &Ui, label: &str) -> f32 {
    ui.width(&omsi_ui::tr(label), 13.0, Weight::Bold) + 52.0
}

/// How the ways lie in `w`: each one's (row, x), and how many rows.
fn ways_laid(ui: &Ui, w: f32, ways: &[(To, &'static str, &'static str)]) -> (Vec<(usize, f32)>, usize) {
    let (mut row, mut x) = (0, 0.0);
    let mut out = Vec::with_capacity(ways.len());
    for (_, label, _) in ways {
        let bw = way_w(ui, label);
        if x > 0.0 && x + bw > w {
            row += 1;
            x = 0.0;
        }
        out.push((row, x));
        x += bw + MORE_GAP;
    }
    (out, if ways.is_empty() { 0 } else { row + 1 })
}

// --- drawing --------------------------------------------------------------------------------------

/// The start screen in the company's page area.
pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let plan = l.company.plan.clone();
    let marks = Marks::of(&c, plan.as_ref());
    let list = sections(&c, plan.as_ref(), &marks);
    let ways = shortcuts(&c);
    let phone = super::super::mobile::mobile() || area.w < 760.0;
    let mut go: Option<To> = None;
    if phone {
        l.ui.scroll_area("company-home", area, &mut |ui, v| phone_grid(ui, v, &list, &ways, &mut go));
    } else {
        let (_, rows) = ways_laid(&l.ui, area.w.min(MAX_W), &ways);
        let g = grid(area, list.len(), rows);
        if g.height <= area.h + 1.0 {
            desk_grid(&mut l.ui, &g, &list, &ways, &mut go);
        } else {
            // (a window lower than the least tiles need: the whole of it scrolls)
            l.ui.scroll_area("company-home", area, &mut |ui, v| {
                let g = grid(Rect::new(v.x, v.y, v.w, 0.0), list.len(), rows);
                desk_grid(ui, &g, &list, &ways, &mut go);
                g.height + 8.0
            });
        }
    }
    if let Some(to) = go {
        open(l, to);
    }
}

/// The desktop's columns, one per section, and the ways under them.
fn desk_grid(ui: &mut Ui, g: &Grid, list: &[Section], ways: &[(To, &'static str, &'static str)], go: &mut Option<To>) {
    let all = Rect::new(g.columns[0].x, g.columns[0].y, g.more.w, g.height);
    super::super::tour::anchor("company-tiles", all);
    for (k, s) in list.iter().enumerate() {
        let Some(col) = g.columns.get(k).copied() else { break };
        kit::caps(ui, Rect::new(col.x + 2.0, col.y + 2.0, col.w, 16.0), s.title);
        let mut slot = 0;
        for t in &s.tiles {
            if slot + t.slots() > SLOTS {
                break;
            }
            let r = g.tile(k, slot, t.slots());
            if tile(ui, r, t) {
                *go = Some(t.to);
            }
            slot += t.slots();
        }
    }
    let (at, _) = ways_laid(ui, g.more.w, ways);
    for ((to, label, icon), (row, x)) in ways.iter().zip(at) {
        let r = Rect::new(g.more.x + x, g.more.y + row as f32 * (MORE_H + MORE_GAP), way_w(ui, label), MORE_H);
        if way(ui, r, label, icon) {
            *go = Some(*to);
        }
    }
}

/// A phone's start: the sections under each other, their tiles two by two (a big one the
/// whole width), the ways at the end. Returns the height used.
fn phone_grid(ui: &mut Ui, v: Rect, list: &[Section], ways: &[(To, &'static str, &'static str)], go: &mut Option<To>) -> f32 {
    let gap = 10.0;
    let tw = ((v.w - gap) / 2.0).floor();
    let th = 96.0;
    let mut y = v.y;
    super::super::tour::anchor("company-tiles", Rect::new(v.x, v.y, v.w, v.h));
    for s in list {
        kit::caps(ui, Rect::new(v.x + 2.0, y, v.w, 16.0), s.title);
        y += TITLE_H;
        let mut col = 0;
        for t in s.tiles.iter() {
            let r = if t.big {
                if col == 1 {
                    y += th + gap;
                    col = 0;
                }
                let r = Rect::new(v.x, y, v.w, 150.0);
                y += 150.0 + gap;
                r
            } else {
                let r = Rect::new(v.x + col as f32 * (tw + gap), y, tw, th);
                col += 1;
                if col == 2 {
                    col = 0;
                    y += th + gap;
                }
                r
            };
            if tile(ui, r, t) {
                *go = Some(t.to);
            }
        }
        if col == 1 {
            y += th + gap;
        }
        y += 8.0;
    }
    let (at, rows) = ways_laid(ui, v.w, ways);
    for ((to, label, icon), (row, x)) in ways.iter().zip(at) {
        let r = Rect::new(v.x + x, y + row as f32 * (MORE_H + MORE_GAP), way_w(ui, label), MORE_H);
        if way(ui, r, label, icon) {
            *go = Some(*to);
        }
    }
    y += rows as f32 * (MORE_H + MORE_GAP);
    y - v.y + 8.0
}

/// A tile: its ground lit at the edge under the mouse (as the launcher's start tiles), its
/// icon on a mark, its name, a badge in its corner, what matters there now, and a big tile's
/// more. Returns whether it was clicked.
fn tile(ui: &mut Ui, base: Rect, t: &Tile) -> bool {
    let id = id_of(&format!("company-home-{:?}", t.to));
    let m = ui.tile(id, base, SHEET_RADIUS);
    let r = m.r;
    ui.tile_shadow(&m);
    ui.p().rounded(r, SHEET_RADIUS, PANEL.mix(HOVER, m.hover));
    ui.tile_light(&m, 0.04);
    ui.tile_edge(&m, 1.0, EDGE);
    let roomy = base.h >= 130.0 && base.w >= 240.0;
    let pad = if roomy { 18.0 } else { 14.0 };
    let side = if roomy { 40.0 } else { 34.0 };
    let mark = Rect::new(base.x + pad, base.y + pad, side, side);
    ui.p().rounded(mark, RADIUS, accent().alpha(0.14).mix(accent(), m.hover));
    ui.icon(t.icon, mark.center(), side * 0.55, accent_2().mix(on_accent(), m.hover));
    // the badge in the corner
    let mut right = base.right() - pad;
    if let Some((n, colour)) = &t.badge {
        let bw = (ui.width(n, 12.0, Weight::Bold) + 14.0).max(24.0);
        let b = Rect::new(right - bw, mark.y + (side - 24.0) * 0.5, bw, 24.0);
        ui.p().rounded(b, 12.0, *colour);
        let ink = if *colour == accent() { on_accent() } else if *colour == WARN { ON_LINE } else { Color::WHITE };
        ui.text_in(n, b, 12.0, Weight::Bold, ink, Align::Center);
        right = b.x - 8.0;
    }
    let name_px = if roomy { 18.0 } else if base.w < 200.0 { 15.0 } else { 16.5 };
    ui.text_in(t.name, Rect::new(mark.right() + 12.0, mark.y, (right - mark.right() - 12.0).max(0.0), side), name_px, Weight::Bold, TEXT, Align::Left);
    // what matters now
    let inner_w = base.w - 2.0 * pad;
    let bottom = base.bottom() - pad * 0.7;
    let mut y = mark.bottom() + if roomy { 14.0 } else { 9.0 };
    let line_h = if roomy { 22.0 } else { 19.0 };
    if let Extra::Money { cash, colour, .. } = &t.extra {
        let px = if roomy { 30.0 } else { 24.0 };
        if y + px + 6.0 <= bottom {
            ui.text_in(cash, Rect::new(base.x + pad, y, inner_w, px + 6.0), px, Weight::Bold, *colour, Align::Left);
            y += px + 10.0;
        }
    }
    for (k, (text, colour)) in t.lines.iter().enumerate() {
        if y + line_h > bottom + 2.0 {
            break;
        }
        let (px, weight) = if k == 0 { (if roomy { 15.0 } else { 14.0 }, Weight::Medium) } else { (if roomy { 14.0 } else { 13.0 }, Weight::Regular) };
        ui.text_in(text, Rect::new(base.x + pad, y, inner_w, line_h), px, weight, *colour, Align::Left);
        y += line_h;
    }
    let rest = Rect::new(base.x + pad, y + 8.0, inner_w, (bottom - y - 8.0).max(0.0));
    match &t.extra {
        Extra::Money { days, .. } => bars(ui, rest, days),
        Extra::Messages(list) => messages(ui, rest, list),
        Extra::Roles(roles) => role_rows(ui, rest, roles),
        Extra::None => {}
    }
    // (the way in, said under the mouse)
    if m.hover > 0.02 {
        ui.icon("chevron_right", Vec2::new(base.right() - pad + 4.0, base.bottom() - pad), 18.0, TEXT_DIM.alpha(m.hover));
    }
    if !t.tip.is_empty() {
        ui.tooltip(base, &t.tip);
    }
    m.clicked
}

/// The last days' results as small bars round a zero line (green a profit, red a loss).
fn bars(ui: &mut Ui, r: Rect, days: &[Option<i64>]) {
    if r.h < 26.0 || days.is_empty() {
        return;
    }
    let r = Rect::new(r.x, r.y, r.w, r.h.min(90.0));
    if days.iter().all(Option::is_none) {
        ui.text_in("No day closed yet", Rect::new(r.x, r.y, r.w, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let hi = days.iter().flatten().copied().max().unwrap_or(0).max(0) as f32;
    let lo = days.iter().flatten().copied().min().unwrap_or(0).min(0) as f32;
    let span = (hi - lo).max(1.0);
    let zero = r.y + r.h * hi / span;
    ui.p().rect(Rect::new(r.x, zero, r.w, 1.0), HAIRLINE);
    let slot = r.w / days.len() as f32;
    let bw = (slot * 0.62).max(1.5);
    for (k, d) in days.iter().enumerate() {
        let Some(v) = d else { continue };
        let h = (r.h * (*v as f32).abs() / span).max(1.5);
        let x = r.x + k as f32 * slot + (slot - bw) * 0.5;
        let bar = if *v >= 0 { Rect::new(x, zero - h, bw, h) } else { Rect::new(x, zero, bw, h) };
        ui.p().rounded(bar, 1.5f32.min(bw * 0.4), if *v >= 0 { OK.alpha(0.85) } else { DANGER.alpha(0.85) });
    }
}

/// The inbox's newest messages: a dot (lit when unread), the title, its day.
fn messages(ui: &mut Ui, r: Rect, list: &[(String, String, bool)]) {
    if r.h < 20.0 {
        return;
    }
    if list.is_empty() {
        ui.text_in("No messages yet", Rect::new(r.x, r.y, r.w, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let row = 24.0;
    let mut y = r.y;
    for (title, date, unread) in list {
        if y + row > r.bottom() + 2.0 {
            break;
        }
        ui.p().circle(Vec2::new(r.x + 4.0, y + row * 0.5), 3.5, if *unread { accent() } else { TEXT_FAINT });
        let dw = ui.width(date, 12.0, Weight::Regular) + 4.0;
        ui.text_in(title, Rect::new(r.x + 16.0, y, (r.w - 16.0 - dw - 8.0).max(0.0), row), 13.5, if *unread { Weight::Medium } else { Weight::Regular }, if *unread { TEXT } else { TEXT_DIM }, Align::Left);
        ui.text_in(date, Rect::new(r.right() - dw, y, dw, row), 12.0, Weight::Regular, TEXT_DIM, Align::Right);
        y += row;
    }
}

/// The staff by role: each role's name and how many, in two columns where there is room.
fn role_rows(ui: &mut Ui, r: Rect, roles: &[(&'static str, usize)]) {
    if r.h < 20.0 || roles.is_empty() {
        return;
    }
    let cols = if r.w >= 300.0 && roles.len() > 3 { 2 } else { 1 };
    let cw = (r.w - 16.0 * (cols as f32 - 1.0)) / cols as f32;
    let row = 22.0;
    let per = roles.len().div_ceil(cols);
    for (k, (label, n)) in roles.iter().enumerate() {
        let (c, k2) = (k / per, k % per);
        let y = r.y + k2 as f32 * row;
        if y + row > r.bottom() + 2.0 {
            continue;
        }
        let x = r.x + c as f32 * (cw + 16.0);
        ui.text_in(label, Rect::new(x, y, cw - 36.0, row), 13.5, Weight::Regular, TEXT_SOFT, Align::Left);
        ui.text_in(&n.to_string(), Rect::new(x + cw - 36.0, y, 36.0, row), 13.5, Weight::Bold, if *n > 0 { TEXT } else { TEXT_DIM }, Align::Right);
        ui.p().rect(Rect::new(x, y + row - 1.0, cw, 1.0), HAIRLINE.alpha(0.5));
    }
}

/// A way under the tiles: a pill with its icon and words.
fn way(ui: &mut Ui, r: Rect, label: &str, icon: &str) -> bool {
    let (h, _, clicked) = ui.interact(id_of(&format!("company-way-{label}")), r);
    let t = ui.anim(id_of(&format!("company-way-{label}")) ^ 0x3a, if h { 1.0 } else { 0.0 }, 0.08);
    ui.p().rounded(r, r.h * 0.5, PANEL.mix(HOVER, t));
    ui.p().rounded_border(r, r.h * 0.5, 1.0, EDGE.mix(accent().alpha(0.5), t));
    ui.icon(icon, Vec2::new(r.x + 20.0, r.center().y), 16.0, accent_2());
    ui.text_in(label, Rect::new(r.x + 34.0, r.y, r.w - 42.0, r.h), 13.0, Weight::Bold, TEXT, Align::Left);
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Difficulty, Founding};

    fn company() -> Company {
        found(&Founding { name: "Hub".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc")
    }

    fn tile_of<'a>(s: &'a [Section], to: To) -> &'a Tile {
        s.iter().flat_map(|x| x.tiles.iter()).find(|t| t.to == to).unwrap_or_else(|| panic!("no tile for {to:?}"))
    }

    #[test]
    fn every_page_has_its_tile_and_every_column_fits() {
        let c = company();
        let s = sections(&c, None, &Marks::of(&c, None));
        for tab in 0..super::super::TABS.len() {
            assert!(s.iter().flat_map(|x| x.tiles.iter()).any(|t| t.to == To::Tab(tab)), "page {tab} has no tile");
        }
        for x in &s {
            let slots: usize = x.tiles.iter().map(Tile::slots).sum();
            assert!(slots <= SLOTS, "{}: {slots} slots", x.title);
            assert!(x.tiles.iter().all(|t| !t.lines.is_empty()), "{}: a tile says nothing", x.title);
        }
        // the ways under them go somewhere else than the tiles
        for (to, _, _) in shortcuts(&c) {
            assert!(!matches!(to, To::Tab(_)), "{to:?}");
        }
        // (the report only once there is one)
        assert!(!shortcuts(&c).iter().any(|w| w.0 == To::Report));
    }

    #[test]
    fn a_new_company_is_told_what_to_do_first() {
        let c = company();
        let marks = Marks::of(&c, None);
        let s = sections(&c, None, &marks);
        // no line, no bus, nobody: the alerts are on the pages that mend them
        assert!(marks.badge(3).is_some(), "lines");
        assert!(marks.badge(1).is_some(), "fleet");
        assert!(marks.badge(2).is_some(), "staff");
        assert_eq!(tile_of(&s, To::Tab(3)).badge.as_ref().map(|b| b.1), Some(WARN));
        assert_eq!(tile_of(&s, To::Tab(1)).lines[0].1, WARN);
        // nothing unread yet, no badge on the inbox
        assert!(marks.badge(INBOX_TAB).is_none());
        assert_eq!(tile_of(&s, To::Tab(5)).lines[0].0, omsi_ui::tr("Reading the timetable…"));
    }

    #[test]
    fn the_inbox_counts_what_is_unread_and_shows_the_newest_first() {
        let mut c = company();
        for k in 0..12 {
            let m = co::inbox::Message::new(co::inbox::Kind::Event, "Fuel price change", "x").arg("n", k.to_string());
            co::inbox::post(&mut c, m);
        }
        c.inbox.messages[0].read = true;
        let marks = Marks::of(&c, None);
        assert_eq!(marks.unread, 11);
        assert_eq!(marks.badge(INBOX_TAB).map(|b| b.0), Some("9+".to_string()));
        let s = sections(&c, None, &marks);
        let inbox = tile_of(&s, To::Tab(INBOX_TAB));
        match &inbox.extra {
            Extra::Messages(list) => {
                assert_eq!(list.len(), 8);
                assert!(list.iter().all(|m| m.2), "the newest eight are unread");
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(inbox.badge.as_ref().map(|b| b.1), Some(accent()));
        // a title's arguments are filled
        assert_eq!(filled("Line %{n} is late", &[("n".into(), "12".into())]), "Line 12 is late");
    }

    #[test]
    fn the_staff_tile_counts_every_role() {
        let mut c = company();
        c.cash = 10_000_000_00;
        for a in co::staff::applicants(&c).iter().take(3) {
            co::staff::hire(&mut c, a).expect("hired");
        }
        assert_eq!(c.staff.len(), 3);
        c.staff[1].role = Role::Mechanic;
        c.staff[2].role = Role::Mechanic;
        let s = sections(&c, None, &Marks::of(&c, None));
        match &tile_of(&s, To::Tab(2)).extra {
            Extra::Roles(r) => assert_eq!(r, &vec![(Role::Driver.label(), 1), (Role::Mechanic.label(), 2)]),
            e => panic!("{e:?}"),
        }
        // (and nobody yet: the drivers' row all the same)
        let s = sections(&company(), None, &Marks::default());
        assert_eq!(tile_of(&s, To::Tab(2)).extra, Extra::Roles(vec![(Role::Driver.label(), 0)]));
    }

    #[test]
    fn the_grid_fits_from_a_small_window_to_a_large_one() {
        // (the company's page area at 1280 x 720 and 2560 x 1440: under its bar and the strip)
        for (w, h, rows) in [(1280.0, 720.0, 2), (1366.0, 768.0, 2), (1920.0, 1080.0, 1), (2560.0, 1440.0, 1)] {
            let area = Rect::new(24.0, 146.0, w - 48.0, h - 186.0);
            let g = grid(area, 5, rows);
            assert!(g.height <= area.h + 0.5, "{w}x{h}: {} > {}", g.height, area.h);
            assert!(g.tile_h >= MIN_TILE_H && g.tile_h <= MAX_TILE_H, "{w}x{h}: {}", g.tile_h);
            assert!(g.columns.iter().all(|c| c.x >= area.x - 0.5 && c.right() <= area.right() + 0.5), "{w}x{h}");
            // the columns do not meet, the first is the widest
            for p in g.columns.windows(2) {
                assert!(p[0].right() + COL_GAP - 1.0 <= p[1].x, "{w}x{h}");
            }
            assert!(g.columns[0].w > g.columns[1].w);
            // the last row of tiles ends above the ways
            let last = g.tile(0, SLOTS - 1, 1);
            assert!(last.bottom() <= g.more.y - MORE_TOP + 0.5, "{w}x{h}");
            assert!(g.more.bottom() <= area.bottom() + 0.5, "{w}x{h}");
        }
        // a large window keeps it in the middle, at most so wide
        let g = grid(Rect::new(0.0, 0.0, 2512.0, 1254.0), 5, 1);
        assert!(g.more.w <= MAX_W && (g.more.x - (2512.0 - MAX_W) / 2.0).abs() < 1.0);
        // too low a window: the least tiles, taller than the area (it scrolls)
        let g = grid(Rect::new(0.0, 0.0, 1200.0, 300.0), 5, 2);
        assert_eq!(g.tile_h, MIN_TILE_H);
        assert!(g.height > 300.0);
    }

    #[test]
    fn the_tiles_and_ways_drawn_lie_in_the_area_and_never_meet() {
        let mut c = company();
        c.last_report = Some(Default::default());
        let marks = Marks::of(&c, None);
        let list = sections(&c, None, &marks);
        let ways = shortcuts(&c);
        let tiles: usize = list.iter().map(|s| s.tiles.len()).sum();
        for (w, h) in [(1280.0, 720.0), (1600.0, 900.0), (2560.0, 1440.0)] {
            let mut ui = Ui::new();
            ui.begin(Vec2::new(w, h), 1.0, 1.0 / 60.0);
            let area = Rect::new(24.0, 146.0, w - 48.0, h - 186.0);
            let (_, rows) = ways_laid(&ui, area.w.min(MAX_W), &ways);
            let g = grid(area, list.len(), rows);
            assert!(g.height <= area.h + 0.5, "{w}x{h}: {rows} rows of ways");
            let mut go = None;
            desk_grid(&mut ui, &g, &list, &ways, &mut go);
            let rects: Vec<Rect> = ui.drawn.values().copied().collect();
            assert_eq!(rects.len(), tiles + ways.len(), "{w}x{h}: every tile and way drawn once");
            for r in &rects {
                assert!(r.x >= area.x - 0.5 && r.right() <= area.right() + 0.5 && r.y >= area.y - 0.5 && r.bottom() <= area.bottom() + 0.5, "{w}x{h}: {r:?} outside {area:?}");
            }
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let apart = a.right() <= b.x + 0.5 || b.right() <= a.x + 0.5 || a.bottom() <= b.y + 0.5 || b.bottom() <= a.y + 0.5;
                    assert!(apart, "{w}x{h}: {a:?} and {b:?} meet");
                }
            }
            assert_eq!(go, None);
        }
    }

    #[test]
    fn a_tile_clicked_goes_to_its_page() {
        let c = company();
        let list = sections(&c, None, &Marks::of(&c, None));
        let ways = shortcuts(&c);
        let area = Rect::new(24.0, 146.0, 1232.0, 534.0);
        let mut ui = Ui::new();
        let mut go = None;
        let frame = |ui: &mut Ui, go: &mut Option<To>| {
            ui.begin(Vec2::new(1280.0, 720.0), 1.0, 1.0 / 60.0);
            let (_, rows) = ways_laid(ui, area.w, &ways);
            desk_grid(ui, &grid(area, list.len(), rows), &list, &ways, go);
        };
        frame(&mut ui, &mut go);
        let r = ui.drawn[&id_of(&format!("company-home-{:?}", To::Tab(MAP_TAB)))];
        ui.input.mouse = r.center();
        ui.input.pressed = true;
        ui.input.down = true;
        frame(&mut ui, &mut go);
        ui.input.pressed = false;
        ui.input.down = false;
        ui.input.released = true;
        frame(&mut ui, &mut go);
        assert_eq!(go, Some(To::Tab(MAP_TAB)));
    }

    #[test]
    fn a_count_past_nine_is_nine_plus() {
        assert_eq!(count_text(3), "3");
        assert_eq!(count_text(10), "9+");
    }
}

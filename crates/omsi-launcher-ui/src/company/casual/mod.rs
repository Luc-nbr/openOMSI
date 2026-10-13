//! The Casual game mode's own interface (Trello "Game modes 3/4"): not the full company with
//! pages hidden, but a calm home and a handful of simple pages for a player who mostly wants
//! to drive and to build a little. The company runs its routine by itself (`co::autopilot`);
//! what the player still shapes is a click from the home - buses, lines, drivers, the money,
//! the company's livery and depot - and today's shifts to drive.
//!
//! - *The home* (`home`): a "Today" card - how the day goes in one sentence, what the company
//!   solved by itself, the big "Drive a shift" - four large tiles (my buses, my lines, my
//!   drivers, the money), each with one big number and one action, and a strip of tips worked
//!   out from the company's state, each with the button that does it. After three weeks a calm
//!   hint offers Normal, once, until it is put away.
//! - *The pages* (`buses`, `lines`, `drivers`, `money`, `shifts`), one level deep at the most:
//!   the fleet as big cards and a simple shop sorted by what fits the lines; the lines as cards
//!   with a fare slider and its advice, and the three-step wizard for a new line; the drivers
//!   with their faces, stars and today's shift, Fire and Give a bonus; the money by the week and
//!   its biggest costs in plain words; the company's livery and depot; and today's shifts as
//!   cards, each opening the company's duty launcher.
//! - *The bar*: the day and the time with one "Next day" in place of the clock's steps.
//!
//! The planning and its roster, the workshop, tenders and contracts, the working-time rules,
//! the research and the inbox are the company's own here: none of their pages is reached from
//! a Casual page (`page_for` sends a way from elsewhere to the Casual page about the same).
//! Every page has its "?" with a few words about it; the company's tour has Casual stops of its
//! own (`tour`).

mod buses;
mod drivers;
mod home;
mod lines;
mod logic;
mod money;
mod shifts;

pub(super) use logic::Act;

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::kit::{self, Foot};
use super::{act, Confirm, Dialog, HOME_TAB};
use glam::Vec2;
use omsi_launcher_lib::company::clock::{self as ck, Step};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The Casual pages (beside the home, `HOME_TAB`), and the steps one level under them: the shop
/// under the buses, the line wizard under the lines, the shifts under the home.
pub(crate) const BUSES: usize = 101;
pub(crate) const LINES: usize = 102;
pub(crate) const DRIVERS: usize = 103;
pub(crate) const MONEY: usize = 104;
pub(crate) const COMPANY: usize = 105;
pub(super) const SHIFTS: usize = 106;
pub(super) const SHOP: usize = 107;
pub(super) const NEW_LINE: usize = 108;

/// The pages in the strip under the bar, in order.
pub(super) const PAGES: [usize; 5] = [BUSES, LINES, DRIVERS, MONEY, COMPANY];
/// Every Casual page and step.
pub(super) const ALL: [usize; 8] = [BUSES, LINES, DRIVERS, MONEY, COMPANY, SHIFTS, SHOP, NEW_LINE];

/// Whether `tab` is one of Casual's (the home among them).
pub(super) fn is_casual(tab: usize) -> bool {
    tab == HOME_TAB || ALL.contains(&tab)
}

/// A Casual page by the name `OMSI_LAUNCHER_PAGE` gives it ("bus company:shop").
pub(super) fn tab_named(name: &str) -> Option<usize> {
    let n = name.trim().to_ascii_lowercase();
    Some(match n.as_str() {
        "buses" => BUSES,
        "shop" => SHOP,
        "lines" => LINES,
        "new-line" => NEW_LINE,
        "drivers" => DRIVERS,
        "money" => MONEY,
        "company" => COMPANY,
        "shifts" => SHIFTS,
        _ => return None,
    })
}

/// A Casual page's name.
pub(super) fn name(tab: usize) -> &'static str {
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

/// A Casual page's icon.
pub(super) fn icon(tab: usize) -> &'static str {
    match tab {
        BUSES => "directions_bus",
        LINES => "route",
        DRIVERS => "groups",
        MONEY => "payments",
        COMPANY => "storefront",
        SHIFTS => "play_arrow",
        SHOP => "add_shopping_cart",
        NEW_LINE => "add_road",
        _ => "home",
    }
}

/// The page a step lies under (the home for a page).
pub(super) fn parent(tab: usize) -> usize {
    match tab {
        SHOP => BUSES,
        NEW_LINE => LINES,
        _ => HOME_TAB,
    }
}

/// The Casual page about what a page of Normal shows: what a way from elsewhere (a popup, the
/// cash under the name, the dealer back from a test drive) opens in Casual.
pub(super) fn page_for(tab: usize) -> usize {
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

/// What a page's "?" says: its title and a few words.
pub(super) fn help(tab: usize) -> (&'static str, &'static str) {
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

/// What the Casual pages keep while they are open.
#[derive(Default)]
pub struct CasualView {
    /// The line wizard's step and choices.
    pub(super) wizard: lines::Wizard,
    /// The shop's sizes: all, or one of them.
    pub(super) shop_size: usize,
    /// Tips put away this session (their ids).
    pub(super) hidden: Vec<String>,
    /// A line to light on the lines page (its fare asked for by a tip, a line just added).
    pub(super) focus: Option<String>,
}

// --- the page ----------------------------------------------------------------------------------

/// A Casual page in the company's page area.
pub(super) fn draw(l: &mut Launcher, body: Rect) {
    match l.company.tab {
        BUSES => buses::page(l, body),
        SHOP => buses::shop(l, body),
        LINES => lines::page(l, body),
        NEW_LINE => lines::wizard(l, body),
        DRIVERS => drivers::page(l, body),
        MONEY => money::page(l, body),
        COMPANY => money::company(l, body),
        SHIFTS => shifts::page(l, body),
        _ => home::draw(l, body),
    }
}

/// Do what a button of the Casual pages says.
pub(super) fn run(l: &mut Launcher, a: Act) {
    match a {
        Act::Open(tab) => l.company.tab = tab,
        Act::BuyBus => l.company.tab = SHOP,
        Act::AddLine => {
            l.company.casual.wizard = Default::default();
            l.company.tab = NEW_LINE;
        }
        Act::HireNow => {
            l.company.tab = DRIVERS;
            drivers::hire_now(l);
        }
        Act::Fare(line) => {
            l.company.tab = LINES;
            l.company.casual.focus = Some(line);
        }
        Act::BiggerDepot => l.company.tab = COMPANY,
        Act::NextDay => next_day(l),
        Act::ToNormal => l.company.dialog = Some(Dialog::Confirm { what: Confirm::GameMode { to: co::GameMode::Normal, tab: Some(HOME_TAB), settings: None } }),
        Act::Seen(id) => {
            act(l, |c| {
                if let Some(m) = c.inbox.messages.iter_mut().find(|m| m.id == id) {
                    m.read = true;
                }
                Ok(())
            });
        }
    }
}

/// A popup's way on in Casual: none to what the company sees to by itself (its progress, the
/// courses, the licences, the research).
pub(super) fn popup_go(g: kit::Go) -> Option<kit::Go> {
    use kit::Go;
    match g {
        Go::Progress | Go::Courses | Go::Training | Go::Licences | Go::Research => None,
        g => Some(g),
    }
}

/// Where a popup's way on goes in Casual: the Casual page about it.
pub(super) fn go(l: &mut Launcher, g: kit::Go) {
    use kit::Go;
    match g {
        Go::Depot => l.company.tab = COMPANY,
        Go::Loan => super::bank::open_loan(l),
        Go::Finances => l.company.tab = MONEY,
        Go::Planning => l.company.tab = SHIFTS,
        Go::Lines => l.company.tab = LINES,
        Go::Dealer => l.company.tab = SHOP,
        Go::Hire => l.company.tab = DRIVERS,
        Go::ToNormal(tab) => l.company.dialog = Some(Dialog::Confirm { what: Confirm::GameMode { to: co::GameMode::Normal, tab: Some(tab), settings: None } }),
        Go::Progress | Go::Courses | Go::Training | Go::Licences | Go::Research => {}
    }
}

/// "Next day": the rest of the company's day simulated and closed, to the next morning (the
/// dispatcher decides what comes up on the way, nothing waits for an answer).
pub(super) fn next_day(l: &mut Launcher) {
    if !super::clock::may_step(l) {
        super::clock::busy(l);
        return;
    }
    let Some(c) = l.company.company.as_ref() else { return };
    let to = ck::target(c, Step::Midnight) + co::clock::MORNING;
    super::clock::simulate(l, to, true);
}

/// The days' reports of a simulation, in Casual: told in a line, not in the report's dialog.
pub(super) fn told(l: &mut Launcher, reports: &[co::day::DayReport]) {
    let Some(r) = reports.last() else { return };
    let (money, _) = kit::signed(r.result);
    let text = omsi_ui::tr("A new day. Yesterday: %{pax} passengers, %{money}.").replace("%{pax}", &super::grouped(r.passengers as f64)).replace("%{money}", &money);
    l.state.set_status(text, r.result < 0);
}

// --- the bar and the strip -----------------------------------------------------------------------

/// The bar's middle in Casual: the company's day and time, and "Next day". Returns where it
/// begins.
pub(super) fn head(l: &mut Launcher, r: Rect) -> f32 {
    let Some(c) = l.company.company.clone() else { return r.right() };
    let label = omsi_ui::tr("Next day");
    let bw = Foot::width(&l.ui, &label, Some("wb_sunny")).max(150.0);
    let b = Rect::new(r.right() - bw, r.y, bw, r.h);
    super::super::tour::anchor("company-casual-next", b);
    if l.ui.button("company-casual-next", b, "Next day", Some("wb_sunny"), ButtonKind::Primary) {
        next_day(l);
    }
    l.ui.tooltip(b, "Run the rest of the day: the buses drive, the money comes in, and you start the next morning");
    let date = super::day_label(&c.date);
    let time = ck::hhmm(ck::now(&c));
    let tw = l.ui.width(&time, 20.0, Weight::Bold);
    let dw = l.ui.width(&date, 14.0, Weight::Medium);
    let w = tw + dw + 14.0;
    let x = b.x - 18.0 - w;
    if x < r.x {
        return b.x;
    }
    l.ui.text_in(&date, Rect::new(x, r.y, dw, r.h), 14.0, Weight::Medium, TEXT_SOFT, Align::Left);
    l.ui.text_in(&time, Rect::new(x + dw + 14.0, r.y, tw + 2.0, r.h), 20.0, Weight::Bold, TEXT, Align::Left);
    x
}

/// The way round under the bar in Casual: Home, the page one is on, and the pages as icons.
pub(super) fn nav(l: &mut Launcher, r: Rect, c: &Company) {
    super::super::tour::anchor("company-nav", r);
    let tab = l.company.tab;
    let home = tab == HOME_TAB;
    // the pages on the right, each an icon with its name under the mouse
    let side = 40.0;
    let gap = 6.0;
    let rail_w = PAGES.len() as f32 * side + (PAGES.len() - 1) as f32 * gap;
    let x0 = r.right() - rail_w;
    let mut x = x0;
    super::super::tour::anchor("company-tabs", Rect::new(x0, r.y, rail_w, r.h));
    for p in PAGES {
        let b = Rect::new(x, r.y + (r.h - side) * 0.5, side, side);
        let id = super::super::ui::id_of(&format!("company-casual-rail-{p}"));
        let (h, _, clicked) = l.ui.interact(id, b);
        let on = tab == p || parent(tab) == p;
        let t = l.ui.anim(id ^ 0x5a, if h { 1.0 } else { 0.0 }, 0.08);
        if on {
            l.ui.p().rounded(b, RADIUS, accent());
        } else if t > 0.01 {
            l.ui.p().rounded(b, RADIUS, HOVER.alpha(t));
        }
        l.ui.icon(icon(p), b.center(), 20.0, if on { on_accent() } else { TEXT_DIM.mix(TEXT, t) });
        l.ui.tooltip(b, name(p));
        if clicked {
            l.company.tab = p;
        }
        x = b.right() + gap;
    }
    // Home, and where one is
    let label = omsi_ui::tr("Home");
    let bw = (l.ui.width(&label, kit::CONTROL, Weight::Bold) + 58.0).max(110.0);
    let b = Rect::new(r.x, r.y + 1.0, bw, r.h - 2.0);
    if l.ui.button("company-home", b, "Home", Some("home"), if home { ButtonKind::Primary } else { ButtonKind::Normal }) && !home {
        l.company.tab = HOME_TAB;
    }
    l.ui.tooltip(b, "Back to your company's home (Escape)");
    let room = (x0 - 20.0 - b.right() - 14.0).max(0.0);
    if home {
        let t = omsi_ui::tr("%{name} · Casual").replace("%{name}", &c.name);
        l.ui.text_in(&t, Rect::new(b.right() + 18.0, r.y, room, r.h), kit::BODY, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let mut cx = b.right() + 16.0;
    for step in [parent(tab), tab] {
        if step == HOME_TAB {
            continue;
        }
        l.ui.icon("chevron_right", Vec2::new(cx, r.center().y), 18.0, TEXT_DIM);
        cx += 16.0;
        let w = l.ui.width(&omsi_ui::tr(name(step)), 18.0, Weight::Bold) + 6.0;
        if cx + w > x0 - 20.0 {
            break;
        }
        let last = step == tab;
        let hit = Rect::new(cx - 4.0, r.y, w + 8.0, r.h);
        if !last {
            let (_, _, clicked) = l.ui.interact(super::super::ui::id_of(&format!("company-casual-crumb-{step}")), hit);
            if clicked {
                l.company.tab = step;
            }
        }
        l.ui.text_in(name(step), Rect::new(cx, r.y, w, r.h), 18.0, Weight::Bold, if last { TEXT } else { TEXT_DIM }, Align::Left);
        cx += w + 10.0;
    }
}

/// The strip's way round on a phone in Casual: back (to the page above), and the page's name.
/// Returns where the rest begins.
pub(super) fn phone_nav(l: &mut Launcher, r: Rect) -> f32 {
    let tab = l.company.tab;
    if tab == HOME_TAB {
        return r.y;
    }
    let up = parent(tab);
    let label = omsi_ui::tr(name(up));
    let bw = (l.ui.width(&label, kit::CONTROL, Weight::Bold) + 52.0).min(r.w * 0.5);
    let back = Rect::new(r.x, r.y, bw, ROW + 4.0);
    super::super::tour::anchor("company-nav", Rect::new(r.x, r.y, r.w, ROW + 4.0));
    if l.ui.button("company-home", back, name(up), Some("chevron_left"), ButtonKind::Normal) {
        l.company.tab = up;
    }
    l.ui.text_in(name(tab), Rect::new(back.right() + 14.0, r.y, (r.w - bw - 14.0).max(0.0), ROW + 4.0), 17.0, Weight::Bold, TEXT, Align::Left);
    back.bottom() + 10.0
}

/// The phone's strip in Casual: the day and "Next day" (in place of the clock's head).
pub(super) fn phone_head(l: &mut Launcher, r: Rect) {
    head(l, r);
}

// --- a page's head ---------------------------------------------------------------------------------

/// A Casual page's head: its title and one line of what it is, its "?" and its one main action
/// on the right. Returns the room under it and whether the action was clicked.
pub(super) fn page_head(l: &mut Launcher, r: Rect, tab: usize, sub: &str, action: Option<(&str, &str)>) -> (Rect, bool) {
    let phone = super::super::mobile::mobile() || r.w < 640.0;
    let mut right = r.right();
    let mut clicked = false;
    let bh = 42.0;
    if let Some((label, ic)) = action {
        let bw = Foot::width(&l.ui, label, Some(ic)).min(r.w * if phone { 0.55 } else { 0.4 });
        let b = Rect::new(right - bw, r.y, bw, bh);
        clicked = l.ui.button(&format!("company-casual-act-{tab}"), b, label, Some(ic), ButtonKind::Primary);
        right = b.x - 10.0;
    }
    let q = Vec2::new(right - 20.0, r.y + bh * 0.5);
    super::super::tour::anchor("company-casual-help", Rect::new(q.x - 20.0, q.y - 20.0, 40.0, 40.0));
    if l.ui.icon_button(&format!("company-casual-help-{tab}"), q, 18.0, "help", "What is this page?") {
        let (title, text) = help(tab);
        kit::show(l, kit::Popup::new("help", title, omsi_ui::tr(text).into_owned(), "", None));
    }
    right = q.x - 28.0;
    let tw = (right - r.x).max(0.0);
    if phone {
        l.ui.paragraph(sub, Vec2::new(r.x, r.y + 4.0), tw, kit::NOTE, Weight::Regular, TEXT_SOFT);
        let h = l.ui.paragraph_height(sub, tw, kit::NOTE, Weight::Regular).max(bh) + 12.0;
        return (Rect::new(r.x, r.y + h, r.w, (r.h - h).max(0.0)), clicked);
    }
    l.ui.text_in(name(tab), Rect::new(r.x, r.y - 4.0, tw, 28.0), kit::TITLE, Weight::Bold, TEXT, Align::Left);
    l.ui.text_in(sub, Rect::new(r.x, r.y + 24.0, tw, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    let h = 62.0;
    (Rect::new(r.x, r.y + h, r.w, (r.h - h).max(0.0)), clicked)
}

/// A calm card for a page with nothing in it yet: an icon, what is missing, and its button.
/// Returns whether the button was clicked.
pub(super) fn empty(l: &mut Launcher, r: Rect, ic: &str, title: &str, text: &str, button: Option<(&str, &str)>) -> bool {
    let w = r.w.min(520.0);
    let x = r.x + (r.w - w) * 0.5;
    let th = l.ui.paragraph_height(text, w, kit::BODY, Weight::Regular);
    let h = 60.0 + 34.0 + th + if button.is_some() { 66.0 } else { 0.0 };
    let y = r.y + ((r.h - h) * 0.35).max(0.0);
    l.ui.p().circle(Vec2::new(x + w * 0.5, y + 26.0), 26.0, accent().alpha(0.14));
    l.ui.icon(ic, Vec2::new(x + w * 0.5, y + 26.0), 26.0, accent_2());
    l.ui.text_in(title, Rect::new(x, y + 62.0, w, 28.0), kit::HEAD, Weight::Bold, TEXT, Align::Center);
    let tx = x + (w - w.min(l.ui.width(&omsi_ui::tr(text), kit::BODY, Weight::Regular) + 2.0)) * 0.5;
    l.ui.paragraph(text, Vec2::new(tx, y + 96.0), w, kit::BODY, Weight::Regular, TEXT_SOFT);
    if let Some((label, ic)) = button {
        let bw = Foot::width(&l.ui, label, Some(ic));
        return l.ui.button(&format!("company-casual-empty-{label}"), Rect::new(x + (w - bw) * 0.5, y + 96.0 + th + 20.0, bw, 44.0), label, Some(ic), ButtonKind::Primary);
    }
    false
}

/// A card's ground: a panel with its edge, lit under the mouse when `hover`.
pub(super) fn card_ground(ui: &mut super::super::ui::Ui, r: Rect, hover: f32, lit: Option<Color>) {
    ui.p().rounded(r, SHEET_RADIUS, PANEL.mix(HOVER, hover));
    ui.p().rounded_border(r, SHEET_RADIUS, 1.0, lit.unwrap_or(EDGE));
}

#[cfg(test)]
mod tests;

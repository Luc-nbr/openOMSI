//! The bus company in the desktop launcher's page (Trello "Tauri 2/6"), working as it works in
//! our game-drawn launcher (`crates/omsi-launcher-ui/src/company/`): the company's rules are
//! `omsi_launcher_lib::company`'s, called here the way the game-drawn launcher calls them, and
//! the page (`launcher/ui/js/omsihub/company/`) draws what these commands say.
//!
//! * A session of the company open lives here, as the game-drawn launcher keeps it in its
//!   `CompanyView`: the driver's companies, the one open, the timetable of its day (read on a
//!   thread), today's plan, its clock and the days' reports. Every change is saved as the
//!   game-drawn launcher saves it (`co::store::save`, a playing clock's quiet minutes now and
//!   then), and a simulation past midnight or a catch-up with the real clock runs on a thread
//!   of its own while the page shows its cover: nothing else changes the company meanwhile.
//! * The page ticks the clock (`co_tick`), asks for the view of what it shows (`co_view`: the
//!   bar, the start screen, a page, a dialog - every text translated and every amount written
//!   in the page's language), and does what a click does (`co_act`).
//! * The pages of later phases hand over to the game-drawn launcher on that page
//!   (`hand_over`): its `OMSI_LAUNCHER_PAGE` is "bus company:<page>:<company>".

mod career;
pub mod casual;
mod chooser;
mod dealer;
mod dialogs;
mod fleet;
mod home;
mod inbox;
pub mod kit;
mod launch;
mod lines;
mod money;
mod overview;
mod people;
mod portraits;
pub mod words;

use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::clock::{self as ck, Away, Level, Run, Step, DAY};
use omsi_launcher_lib::company::day::{DayReport, Plan};
use omsi_launcher_lib::company::market::MarketBus;
use omsi_launcher_lib::company::store::Disk;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use words::{day_label, eur, grouped, num, tr, trf};

// --- the pages -------------------------------------------------------------------------------

/// The company's pages, as the game-drawn launcher numbers them (`company::TABS`): the
/// `OMSI_LAUNCHER_PAGE` it reads is "bus company:<number>".
pub const TABS: [&str; 14] = ["Overview", "Fleet", "Staff", "Lines", "Finances", "Planning", "Career", "Depot", "Concessions", "Map", "Research", "Workshop", "Contracts", "Inbox"];
pub const DEPOT_TAB: usize = 7;
pub const CONCESSIONS_TAB: usize = 8;
pub const MAP_TAB: usize = 9;
pub const RESEARCH_TAB: usize = 10;
pub const WORKSHOP_TAB: usize = 11;
pub const CONTRACTS_TAB: usize = 12;
pub const INBOX_TAB: usize = 13;
/// The company's start screen (no page of `TABS`).
pub const HOME_TAB: usize = 100;
/// The pages' icons, in the order of `TABS`.
pub const ICONS: [&str; 14] = ["signal_cellular_alt", "directions_bus", "groups", "route", "payments", "calendar_month", "military_tech", "garage", "public", "map", "search", "construction", "description", "chat"];
/// The pages in the strip under the bar, grouped as the start screen groups them.
pub const RAIL: [&[usize]; 5] = [&[0, INBOX_TAB], &[5, 3, MAP_TAB, 1], &[2], &[4, CONCESSIONS_TAB, CONTRACTS_TAB, 6], &[DEPOT_TAB, WORKSHOP_TAB, RESEARCH_TAB]];

/// The pages this page draws itself; the others (`HANDED_OVER`) open in the game-drawn launcher
/// until their phase: the planning and RosterFlow, the depot, the concessions, the fleet map,
/// the research, the workshop and the contracts.
pub const DRAWN: [usize; 7] = [0, 1, 2, 3, 4, 6, INBOX_TAB];
pub const HANDED_OVER: [usize; 7] = [5, DEPOT_TAB, CONCESSIONS_TAB, MAP_TAB, RESEARCH_TAB, WORKSHOP_TAB, CONTRACTS_TAB];

/// The pages a game mode shows, in the strip's order (`company::pages_of`).
pub fn pages_of(mode: co::GameMode) -> Vec<usize> {
    if mode.casual() {
        return casual::PAGES.to_vec();
    }
    RAIL.iter().flat_map(|g| g.iter().copied()).collect()
}

/// A page's name as the strip under the bar says it.
pub fn page_name(tab: usize) -> &'static str {
    TABS.get(tab).copied().unwrap_or("Start screen")
}

/// A game mode's icon.
pub fn mode_icon(mode: co::GameMode) -> &'static str {
    match mode {
        co::GameMode::Casual => "autorenew",
        co::GameMode::Normal => "tune",
        co::GameMode::Realistic => "badge",
    }
}

/// A game mode's key (as the company file writes it).
pub fn mode_key(mode: co::GameMode) -> &'static str {
    match mode {
        co::GameMode::Casual => "casual",
        co::GameMode::Normal => "normal",
        co::GameMode::Realistic => "realistic",
    }
}

pub fn mode_of(key: &str) -> Option<co::GameMode> {
    co::GameMode::ALL.iter().copied().find(|m| mode_key(*m) == key)
}

/// A company's mode as its pill says it.
pub fn mode_json(mode: co::GameMode) -> Value {
    json!({ "key": mode_key(mode), "label": tr(mode.label()), "icon": mode_icon(mode), "casual": mode.casual() })
}

/// Where the company is at home: its map, its depot and its game mode.
pub fn home_of(c: &Company) -> String {
    let map = if c.map_name.is_empty() { words::short_map(&c.map) } else { c.map_name.clone() };
    format!("{}  ·  {}  ·  {}", map, c.depot, tr(c.game_mode.label()))
}

/// The company's mark: its logo picture (as a `data:` address the page can show), or its
/// monogram on its colours.
pub fn mark_json(c: &Company) -> Value {
    json!({ "short": c.short, "colours": c.colours, "logo": c.logo.as_deref().and_then(logo_picture) })
}

/// A logo picture as the page shows it (read once; at most 3 MB).
fn logo_picture(path: &str) -> Option<String> {
    static KEPT: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(x) = kept.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
        return x.clone();
    }
    let read = std::fs::read(path).ok().filter(|b| !b.is_empty() && b.len() <= 3 << 20).map(|b| {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        format!("data:{mime};base64,{}", base64(&b))
    });
    kept.lock().unwrap_or_else(|e| e.into_inner()).insert(path.to_string(), read.clone());
    read
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

// --- the session ---------------------------------------------------------------------------------

/// The timetable of the company's day.
pub struct Today {
    pub map: String,
    pub date: String,
    pub lines: Vec<core::LineInfo>,
    pub error: Option<String>,
}

/// The company's clock on the pages (`clock::ClockView`).
pub struct ClockState {
    /// Playing: company minutes per second (0: it stands, or with the real clock goes as the
    /// real one does).
    pub speed: f64,
    acc: f64,
    last: Option<Instant>,
    /// Changes of the clock not saved yet.
    dirty: Option<Instant>,
    follow_at: Option<Instant>,
    live_seen: HashMap<(String, String), usize>,
    /// The time dialog's "until": a minute of the day.
    pub until: f64,
    /// A catch-up on its thread: how many minutes it catches up.
    pub catching: Option<i64>,
    /// What happened while the player was away.
    pub away: Option<Away>,
    /// A step of the real clock failed: the next waits until then.
    held: Option<Instant>,
}

impl Default for ClockState {
    fn default() -> Self {
        ClockState { speed: 0.0, acc: 0.0, last: None, dirty: None, follow_at: None, live_seen: HashMap::new(), until: 16.0 * 60.0, catching: None, away: None, held: None }
    }
}

/// How long the real clock's steps wait after one failed.
const HOLD_SECS: u64 = 300;

impl ClockState {
    fn hold(&mut self) {
        self.held = Some(Instant::now());
    }

    fn holding(&self) -> bool {
        self.held.is_some_and(|t| t.elapsed().as_secs() < HOLD_SECS)
    }
}

/// The bus company as the page has it open.
#[derive(Default)]
pub struct Session {
    /// The driver's companies (None: not read yet), and for which driver.
    pub companies: Option<Vec<Company>>,
    companies_for: Option<String>,
    pub company: Option<Company>,
    pub today: Option<Today>,
    today_asked: Option<(String, String)>,
    /// The revision of the map's timetable (a read of an older one is dropped).
    timetable: u64,
    pub own: Vec<core::lines::OwnLine>,
    own_for: Option<String>,
    /// Today's plan (None: to be made again).
    pub plan: Option<Plan>,
    /// A simulation (or a catch-up) has the company on its thread.
    pub closing: bool,
    /// The report of the days just closed.
    pub reports: Option<Vec<DayReport>>,
    pub clock: ClockState,
    /// Raised whenever the company changed: the page asks for its view again.
    pub generation: u64,
    /// What the page says in its corner, as the game-drawn launcher says it in its status line.
    pub notices: Vec<(String, bool)>,
    /// A popup the session raised itself (the clock's "busy").
    pub popup: Option<Value>,
    /// The market's buses, for how many installed buses.
    pub market: Option<Arc<Vec<MarketBus>>>,
    market_for: Option<usize>,
    market_busy: bool,
    /// The installed buses (read on a thread once).
    pub vehicles: Option<Arc<Vec<core::VehicleInfo>>>,
    vehicles_busy: bool,
    /// The maps (read once).
    pub maps: Option<Arc<Vec<core::MapInfo>>>,
    /// The running games, read every two seconds.
    instances: Vec<core::Instance>,
    instances_at: Option<Instant>,
    /// The phone's orders and the concessions' market: when they were last looked at.
    orders_at: Option<Instant>,
    concessions_for: Option<(String, String, usize)>,
    /// The dealer: its showroom, its clock and the sheet open (`dealer`).
    pub dealer: dealer::DealerState,
    /// The company's duty launcher (`launch`).
    pub launch: Option<launch::DutyLaunch>,
    /// The day report's line opened to its money.
    pub report_line: Option<String>,
    /// The feed shows the ordinary run of the day too.
    pub feed_minor: bool,
    /// Raised whenever today's plan is made again (`company::CompanyView::generation`: the
    /// week's plans are kept until then).
    pub plan_gen: u64,
    /// The week's timetables and plans (`planning::work`, `planning::plan_of`).
    week: Week,
    /// Casual's own: tips put away this session, a line to light, the shop's size.
    pub casual: casual::CasualState,
    /// The driver's career as read (`career`).
    pub driver: Option<career::Driver>,
}

/// The timetables of the company's week, read on a thread, and the plans made of them for a
/// plan generation (`planning::PlanningView`'s days and cache).
#[derive(Default)]
struct Week {
    days: Vec<(String, String, Result<Vec<core::LineInfo>, String>)>,
    asked: Vec<(String, String)>,
    cache: Vec<(String, u64, co::plan::DayPlan)>,
    read_at: u64,
}

fn session() -> &'static Mutex<Session> {
    static S: OnceLock<Mutex<Session>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Session::default()))
}

/// Run `f` on the session (the background's work done first, as every frame of the game-drawn
/// launcher does it before it draws).
fn with<T>(f: impl FnOnce(&mut Session) -> T) -> T {
    let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
    work(&mut s);
    f(&mut s)
}

/// The data folder (`~/.openomsi`).
pub fn data() -> std::path::PathBuf {
    core::data_dir()
}

/// The real time now, in Unix seconds.
pub fn real_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Session {
    /// Say something in the page's corner.
    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.notices.push((text.into(), error));
    }

    /// The lines of the company's map in the timetable of its day (None: not read yet).
    pub fn map_lines(&self) -> Option<&[core::LineInfo]> {
        let c = self.company.as_ref()?;
        self.today.as_ref().filter(|t| t.map == c.map && t.date == c.date && t.error.is_none()).map(|t| t.lines.as_slice())
    }

    /// Whether the open company runs in the easy mode (Casual).
    pub fn easy(&self) -> bool {
        self.company.as_ref().is_some_and(co::autopilot::on)
    }

    /// The installed buses (asked for on a thread the first time: empty until then).
    pub fn vehicles(&mut self) -> Arc<Vec<core::VehicleInfo>> {
        if self.vehicles.is_none() && !self.vehicles_busy {
            self.vehicles_busy = true;
            std::thread::spawn(|| {
                let list = core::list_vehicles().unwrap_or_default();
                let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
                s.vehicles = Some(Arc::new(list));
                s.vehicles_busy = false;
                s.generation += 1;
            });
        }
        self.vehicles.clone().unwrap_or_default()
    }

    pub fn vehicles_read(&self) -> bool {
        self.vehicles.is_some()
    }

    /// The maps (read the first time they are asked for).
    pub fn maps(&mut self) -> Arc<Vec<core::MapInfo>> {
        if self.maps.is_none() {
            self.maps = Some(Arc::new(core::list_maps().unwrap_or_default()));
        }
        self.maps.clone().unwrap_or_default()
    }

    /// The market's buses (read once, again when the installed buses changed).
    pub fn ask_market(&mut self) {
        let vehicles = self.vehicles();
        if !self.vehicles_read() {
            return;
        }
        let n = vehicles.len();
        if self.market_busy || self.market_for == Some(n) {
            return;
        }
        self.market_busy = true;
        self.market_for = Some(n);
        std::thread::spawn(move || {
            let list = co::market::market_of(&vehicles);
            let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
            s.market = Some(Arc::new(list));
            s.market_busy = false;
            s.generation += 1;
        });
    }

    pub fn market_busy(&self) -> bool {
        self.market_busy || !self.vehicles_read()
    }

    /// The running games (read again every two seconds).
    pub fn instances(&mut self) -> &[core::Instance] {
        if self.instances_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.0) {
            self.instances_at = Some(Instant::now());
            self.instances = core::list_instances();
        }
        &self.instances
    }

    /// A game runs.
    pub fn in_game(&mut self) -> bool {
        self.instances().iter().any(|i| i.running)
    }
}

/// After a change made on a page: saved, and today's plan made again (`company::changed`).
pub fn changed(s: &mut Session) {
    s.plan = None;
    s.generation += 1;
    if let Some(c) = s.company.as_ref() {
        if let Some(list) = s.companies.as_mut() {
            match list.iter_mut().find(|x| x.id == c.id) {
                Some(x) => *x = c.clone(),
                None => list.push(c.clone()),
            }
        }
        if let Err(e) = co::store::save(&data(), c) {
            let t = format!("{e:#}");
            s.say(t, true);
        }
    }
}

/// Do something to the company; its refusal comes back as the popup that says why and what
/// opens it (`company::act`).
pub fn act<T>(s: &mut Session, f: impl FnOnce(&mut Company) -> Result<T, &'static str>) -> Result<T, Value> {
    let Some(c) = s.company.as_mut() else { return Err(json!(null)) };
    match f(c) {
        Ok(v) => {
            changed(s);
            Ok(v)
        }
        Err(e) => Err(refuse(s, e)),
    }
}

/// A refusal of the company's rules as its popup.
pub fn refuse(s: &Session, reason: &str) -> Value {
    match s.company.as_ref() {
        Some(c) => kit::refusal(c, reason).json(c.game_mode.casual()),
        None => kit::Popup::new("error", "This cannot be done", tr(reason), "", None).json(false),
    }
}

/// A popup of the session's open company.
pub fn popup(s: &Session, p: kit::Popup) -> Value {
    p.json(s.company.as_ref().is_some_and(|c| c.game_mode.casual()))
}

// --- what goes on in the background ------------------------------------------------------------

/// What the page needs read and kept up to date (`company::work`): the driver's companies, the
/// timetable of the company's day and the own lines of its map, the easy mode's morning, today's
/// plan and the company's day for the game, the dealer's and the depot's clocks.
fn work(s: &mut Session) {
    let profile = core::load_config().profile;
    if s.companies_for.as_deref() != Some(profile.as_str()) {
        s.companies_for = Some(profile.clone());
        let list = co::store::list(&data(), &profile);
        let keep = s.company.as_ref().map(|c| c.id.clone());
        s.company = keep.and_then(|id| list.iter().find(|c| c.id == id).cloned());
        s.companies = Some(list);
        s.plan = None;
        s.generation += 1;
    }
    let Some(c) = s.company.as_ref() else { return };
    let key = (c.map.clone(), c.date.clone());
    if s.today_asked.as_ref() != Some(&key) {
        s.today_asked = Some(key.clone());
        let revision = s.timetable;
        std::thread::spawn(move || {
            let (map, date) = key;
            let result = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
            let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
            // (read before the timetable was written again: the read asked for since is on its way)
            if revision != s.timetable {
                return;
            }
            let (lines, error) = match result {
                Ok(l) => (l, None),
                Err(e) => (Vec::new(), Some(e)),
            };
            s.today = Some(Today { map, date, lines, error });
            s.plan = None;
            s.generation += 1;
        });
    }
    if s.own_for.as_deref() != Some(c.map.as_str()) {
        let map = c.map.clone();
        s.own_for = Some(map.clone());
        s.own = core::lines::own_lines_of_map(&map);
    }
    // the easy mode's morning before today's plan is made: saved when it changed anything
    if s.plan.is_none() && !s.closing && easy_morning(s) {
        changed(s);
    }
    // today's plan, and the company's day for the game
    if s.plan.is_none() {
        s.plan_gen += 1;
        if let (Some(c), Some(t)) = (s.company.as_ref(), s.today.as_ref()) {
            if t.map == c.map && t.date == c.date {
                let tours = co::network::tours_of_day(c, &t.lines, &c.date);
                let day = co::plan::day_plan(c, &c.date, tours, &[], &[], false);
                s.plan = Some(day.to_plan());
                if let Err(e) = co::plan::save_live_plan(&data(), &co::plan::live_plan(c, &day)) {
                    log_warn(&format!("company: the day's plan for the game: {e:#}"));
                }
            }
        }
    }
    depot_tick(s);
    dealer::tick(s);
}

fn log_warn(text: &str) {
    core::log_to_file(text);
}

/// The timetables of the company's week read (on a thread), those of the days asked for besides
/// (`planning::work`).
pub fn week_work(s: &mut Session) {
    let Some(c) = s.company.as_ref() else { return };
    if s.week.read_at != s.timetable {
        s.week = Week { read_at: s.timetable, ..Default::default() };
    }
    let week: Vec<(String, String)> = (0..7).map(|k| (c.map.clone(), co::dates::add(&c.date, k))).collect();
    let v = &mut s.week;
    v.days.retain(|d| week.iter().any(|w| w.0 == d.0 && w.1 == d.1));
    v.asked.retain(|a| week.contains(a));
    let missing: Vec<(String, String)> = week.into_iter().filter(|w| !v.asked.contains(w)).collect();
    if missing.is_empty() {
        return;
    }
    v.asked.extend(missing.iter().cloned());
    let revision = s.timetable;
    std::thread::spawn(move || {
        for (map, date) in missing {
            let r = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
            let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
            if s.timetable != revision {
                break;
            }
            s.week.days.retain(|d| !(d.0 == map && d.1 == date));
            s.week.days.push((map, date, r));
            s.generation += 1;
        }
    });
}

/// The plan of a day of the week (None: its timetable is still being read; `planning::plan_of`).
pub fn plan_of(s: &mut Session, date: &str) -> Option<Result<co::plan::DayPlan, String>> {
    let c = s.company.as_ref()?;
    let generation = s.plan_gen;
    let v = &mut s.week;
    if let Some(p) = v.cache.iter().find(|x| x.0 == date && x.1 == generation) {
        return Some(Ok(p.2.clone()));
    }
    let read = v.days.iter().find(|d| d.0 == c.map && d.1 == date)?;
    match &read.2 {
        Err(e) => Some(Err(e.clone())),
        Ok(lines) => {
            let tours = co::network::tours_of_day(c, lines, date);
            let p = co::plan::day_plan(c, date, tours, &[], &[], false);
            v.cache.retain(|x| x.1 == generation && x.0 != date);
            v.cache.push((date.to_string(), generation, p.clone()));
            Some(Ok(p))
        }
    }
}

/// The lines of a day of the week as read (for the duty launcher's start).
pub fn lines_of(s: &Session, date: &str) -> Option<Vec<core::LineInfo>> {
    let c = s.company.as_ref()?;
    s.week.days.iter().find(|x| x.0 == c.map && x.1 == date).and_then(|x| x.2.as_ref().ok()).cloned()
}

/// Tomorrow's plan, for the overview's warnings (`planning::tomorrow`).
pub fn tomorrow(s: &mut Session) -> Option<Plan> {
    week_work(s);
    let date = co::dates::add(&s.company.as_ref()?.date, 1);
    plan_of(s, &date)?.ok().map(|p| p.to_plan())
}

/// The easy mode's morning on the company's day (`co::autopilot::morning`).
fn easy_morning(s: &mut Session) -> bool {
    let (Some(c), Some(t)) = (s.company.as_mut(), s.today.as_ref()) else { return false };
    if !co::autopilot::on(c) || t.map != c.map || t.date != c.date || t.error.is_some() {
        return false;
    }
    let date = c.date.clone();
    co::autopilot::morning(c, &date, &t.lines)
}

/// The phone's orders carried out (every two seconds), and the concessions' market kept up
/// with the day's timetable (`depot::tick`).
fn depot_tick(s: &mut Session) {
    if s.closing || s.company.is_none() {
        return;
    }
    if s.orders_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.0) {
        s.orders_at = Some(Instant::now());
        let d = data();
        let done = s.company.as_mut().map(|c| co::remote::take(&d, c)).unwrap_or_default();
        if !done.is_empty() {
            changed(s);
            match done.iter().find_map(|x| x.1.err()) {
                Some(e) => {
                    let t = trf("An order from the phone was refused: %{why}", &[("why", &tr(e))]);
                    s.say(t, true);
                }
                None => {
                    let t = trf("Orders from the phone carried out: %{n}.", &[("n", &done.len().to_string())]);
                    s.say(t, false);
                }
            }
        }
    }
    let (Some(c), Some(t)) = (s.company.as_ref(), s.today.as_ref()) else { return };
    if t.map != c.map || t.date != c.date || t.error.is_some() {
        return;
    }
    let key = (c.id.clone(), c.date.clone(), c.lines.len());
    if s.concessions_for.as_ref() == Some(&key) {
        return;
    }
    s.concessions_for = Some(key);
    let lines = t.lines.clone();
    if s.company.as_mut().is_some_and(|c| co::concessions::refresh(c, &lines)) {
        changed(s);
    }
}

// --- the clock (`clock.rs` of the game-drawn launcher) ------------------------------------------

fn realtime(c: &Company) -> bool {
    c.clock.real.on
}

/// The game runs on the company's map: its clock leads (the minute of its day). A test drive at
/// the dealer's runs outside the company's time (`map::time_of_day`).
fn game_minute(s: &mut Session, c: &Company) -> Option<i64> {
    let unix = real_now().max(0) as u64;
    let norm = |m: &str| m.replace('\\', "/");
    let same = |m: &str| norm(m).eq_ignore_ascii_case(&norm(&c.map));
    let test = |i: &core::Instance| i.line.is_none() && c.dealer.test_drive.as_ref().is_some_and(|t| norm(&i.bus).eq_ignore_ascii_case(&norm(&t.bus)));
    let i = s.instances().iter().find(|i| i.running && same(&i.map) && !test(i))?;
    let start = started_at(&i.args).unwrap_or(0) as f64 * 60.0;
    let secs = (start + unix.saturating_sub(i.started) as f64).rem_euclid(86_400.0);
    Some((secs / 60.0).floor() as i64)
}

/// The time of day a game was started at (its `--time`), in minutes.
fn started_at(args: &[String]) -> Option<i32> {
    let t = args.iter().position(|a| a == "--time").and_then(|k| args.get(k + 1))?;
    let (h, m) = t.trim().split_once(':')?;
    Some(h.trim().parse::<i32>().ok()? * 60 + m.trim().parse::<i32>().ok()?)
}

fn game_leads(s: &mut Session) -> bool {
    match s.company.clone() {
        Some(c) => game_minute(s, &c).is_some(),
        None => false,
    }
}

/// A step may be taken now (no other running, no game leading).
pub fn may_step(s: &mut Session) -> bool {
    !s.closing && !game_leads(s) && s.company.as_ref().is_some_and(|c| c.clock.ask.is_none())
}

/// Simulate to `to`: within the company's day at once, past its midnight on a thread.
pub fn simulate(s: &mut Session, to: i64, quick: bool) {
    let Some(c) = s.company.clone() else { return };
    if s.closing || game_minute(s, &c).is_some() {
        return;
    }
    let midnight = (ck::now(&c).div_euclid(DAY) + 1) * DAY;
    let lines = s.map_lines().map(|l| l.to_vec());
    if to < midnight {
        if let Some(lines) = lines {
            let mut c = c;
            let d = data();
            let mut w = Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), lines);
            match ck::advance(&mut c, to, &mut w, quick) {
                Ok(run) => adopt(s, c, run),
                Err(e) => {
                    s.clock.speed = 0.0;
                    s.clock.hold();
                    s.say(e, true);
                }
            }
            return;
        }
    }
    s.closing = true;
    s.clock.dirty = None;
    s.generation += 1;
    std::thread::spawn(move || {
        let d = data();
        let mut c = c;
        let w = lines.map(|x| Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), x));
        let r = match co::store::simulate(&d, &mut c, to, quick, w) {
            Ok(run) => {
                if ck::keep_lead(&mut c, real_now()) {
                    if let Err(e) = co::store::save(&d, &c) {
                        log_warn(&format!("company: saving the clock's lead: {e:#}"));
                    }
                }
                Ok((c, run))
            }
            Err(e) => Err(format!("{e:#}")),
        };
        let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
        simulated(&mut s, r);
    });
}

/// A simulation past midnight came back (`Msg::Simulated`).
fn simulated(s: &mut Session, r: Result<(Company, Run), String>) {
    s.closing = false;
    s.generation += 1;
    match r {
        Ok((c, run)) => {
            if let Some(list) = s.companies.as_mut() {
                if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                    *x = c.clone();
                }
            }
            let casual = c.game_mode.casual();
            // (the company still open: nothing else can be while the cover is up)
            if s.company.as_ref().is_none_or(|x| x.id == c.id) {
                s.company = Some(c);
                if casual {
                    // (Casual says the day in a line: no report to read through)
                    casual::told(s, &run.reports);
                } else if !run.reports.is_empty() {
                    s.reports = Some(run.reports);
                }
                s.plan = None;
            }
        }
        Err(e) => {
            s.clock.speed = 0.0;
            s.clock.hold();
            s.say(e, true);
        }
    }
}

/// The company after a simulation within its day: its days' reports shown, saved (a running
/// clock's quiet minutes now and then). Stepped ahead of the real clock, its lead is kept.
pub fn adopt(s: &mut Session, mut c: Company, run: Run) {
    ck::keep_lead(&mut c, real_now());
    let quiet = run.reports.is_empty() && !run.stopped && c.clock.feed.iter().rev().take(run.told).all(|f| f.level == Level::Minor);
    let running = s.clock.speed > 0.0 || realtime(&c);
    s.company = Some(c);
    s.generation += 1;
    if !run.reports.is_empty() {
        s.reports = Some(run.reports);
    }
    if quiet && running {
        if let (Some(c), Some(list)) = (s.company.as_ref(), s.companies.as_mut()) {
            if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                *x = c.clone();
            }
        }
        if s.clock.dirty.is_none() {
            s.clock.dirty = Some(Instant::now());
        }
        return;
    }
    s.clock.dirty = None;
    changed(s);
}

/// Save what a playing clock left unsaved.
fn flush(s: &mut Session) {
    if s.clock.dirty.take().is_some() {
        changed(s);
    }
}

/// The clock runs (company minutes a second) or stands.
pub fn play(s: &mut Session, speed: f64) {
    s.clock.speed = speed;
    s.clock.acc = 0.0;
    s.clock.last = Some(Instant::now());
    if speed == 0.0 {
        flush(s);
    }
}

/// The clock on the company's pages, every tick of the page (`clock::tick`): with the real one
/// (in step from the first time it is opened; behind it, caught up), the playing clock, and the
/// game's clock leading. `modal`: a dialog or a popup lies over the pages.
fn tick(s: &mut Session, modal: bool) {
    let now = Instant::now();
    let dt = s.clock.last.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0).min(1.0);
    s.clock.last = Some(now);
    let Some(c) = s.company.clone() else { return };
    let real = real_now();
    if game_minute(s, &c).is_some() {
        s.clock.speed = 0.0;
        if s.clock.follow_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.0) {
            s.clock.follow_at = Some(now);
            follow(s, &c);
        }
        return;
    }
    let live = realtime(&c) && !s.clock.holding();
    let running = s.clock.speed > 0.0 || realtime(&c);
    if s.clock.dirty.is_some_and(|t| t.elapsed().as_secs_f32() >= 8.0) || (!running && s.clock.dirty.is_some()) {
        flush(s);
    }
    if s.closing {
        return;
    }
    if live && c.clock.real.synced.is_none() {
        if let Some(x) = s.company.as_mut() {
            ck::sync(x, real);
        }
        changed(s);
        return;
    }
    let other = modal || s.reports.is_some() || s.popup.is_some() || s.clock.away.is_some();
    if live && !other && ck::behind(&c, real) >= ck::AWAY_TOLD {
        catch_up(s, c, real);
        return;
    }
    if modal || s.reports.is_some() || s.clock.away.is_some() {
        return;
    }
    if c.clock.ask.is_some() {
        s.clock.speed = 0.0;
        return;
    }
    if s.clock.speed <= 0.0 {
        let to = ck::course(&c, real).unwrap_or(i64::MIN);
        if live && to > ck::now(&c) {
            let midnight = (ck::now(&c).div_euclid(DAY) + 1) * DAY;
            if s.map_lines().is_some() || to >= midnight {
                simulate(s, to, false);
            }
        }
        return;
    }
    s.clock.acc += dt * s.clock.speed;
    if s.clock.acc < 1.0 {
        return;
    }
    let steps = s.clock.acc.floor() as i64;
    s.clock.acc -= steps as f64;
    simulate(s, ck::now(&c) + steps, false);
}

/// The company caught up with the real clock on a thread, under a cover.
fn catch_up(s: &mut Session, c: Company, real: i64) {
    s.clock.speed = 0.0;
    s.clock.catching = Some(ck::behind(&c, real).min(ck::CATCH_UP_MAX));
    s.closing = true;
    s.clock.dirty = None;
    s.generation += 1;
    let lines = s.map_lines().map(|l| l.to_vec());
    std::thread::spawn(move || {
        let d = data();
        let mut c = c;
        let w = lines.map(|x| Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), x));
        let r = co::store::catch_up(&d, &mut c, real, w).map(|away| (c, away)).map_err(|e| format!("{e:#}"));
        let mut s = session().lock().unwrap_or_else(|e| e.into_inner());
        s.closing = false;
        s.clock.catching = None;
        s.generation += 1;
        match r {
            Ok((c, away)) => {
                if let Some(list) = s.companies.as_mut() {
                    if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                        *x = c.clone();
                    }
                }
                if s.company.as_ref().is_some_and(|x| x.id == c.id) {
                    s.company = Some(c);
                    s.plan = None;
                    s.clock.away = away.filter(|a| a.caught.to - a.caught.from >= ck::AWAY_TOLD);
                }
            }
            Err(e) => {
                s.clock.hold();
                s.say(e, true);
            }
        }
    });
}

/// The game's minute taken over, and what it reported told.
fn follow(s: &mut Session, c: &Company) {
    let Some(minute) = game_minute(s, c) else { return };
    let mut c = c.clone();
    let key = (c.id.clone(), c.date.clone());
    let seen = &mut s.clock.live_seen;
    seen.retain(|k, _| k.0 != key.0 || k.1 == key.1);
    let already = seen.get(&key).copied().unwrap_or(0);
    let d = data();
    let live = co::store::peek_live(&d, &c.id, already);
    let mut told = 0;
    if !live.is_empty() {
        s.clock.live_seen.insert(key, already + live.len());
        told += ck::tell_live(&mut c, &live);
    }
    if let Some(lines) = s.map_lines().map(|l| l.to_vec()) {
        let mut w = Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), lines);
        match ck::follow(&mut c, minute, &mut w) {
            Ok(run) => told += run.told,
            Err(e) => log_warn(&format!("company: following the game's clock: {e}")),
        }
    }
    let synced = ck::with_game(&mut c, real_now());
    let moved = s.company.as_ref().is_none_or(|x| ck::now(x) != ck::now(&c));
    if told > 0 || synced || moved {
        s.company = Some(c);
        changed(s);
    }
}

/// Why the clock cannot be stepped now, in a popup (`clock::busy`).
pub fn busy(s: &mut Session) -> Value {
    let p = if game_leads(s) {
        kit::Popup::new("sports_esports", "The game leads the clock", tr("The game runs on the company's map: the company's day goes along with the game's time."), tr("Simulating is offered again when the game is closed."), None)
    } else if s.company.as_ref().is_some_and(|c| c.clock.ask.is_some()) {
        kit::Popup::new("help", "A decision waits", tr("The company's clock waits for your answer first."), "", None)
    } else {
        kit::Popup::new("timer", "The clock is running", tr("The company's time is being simulated: wait a moment."), "", None)
    };
    popup(s, p)
}

/// A step of the clock asked for (+1 h, Simulate to tomorrow, the time dialog's): taken, or the
/// popup that says why not.
fn step(s: &mut Session, st: Step, quick: bool) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if !may_step(s) {
        return Ok(json!({ "popup": busy(s) }));
    }
    let to = ck::target(&c, st);
    simulate(s, to, quick);
    Ok(json!({}))
}

/// What the clock's head says of the real clock under the mouse.
fn realtime_tip(c: &Company) -> String {
    if !realtime(c) {
        return tr("The company's time stands still while it is closed: the real clock is off in the company's settings.");
    }
    let mut t = tr("In step with the real clock: the company's time goes on while openOMSI is closed, and is caught up when you open the company.");
    if c.clock.real.lead > 0 {
        t.push('\n');
        t.push_str(&trf("Fast-forwarded: %{span} ahead of the real clock.", &[("span", &words::span_text(c.clock.real.lead))]));
    }
    t
}

/// What the start's tile says of a company's real clock (`clock::tile_line`).
pub fn tile_line(c: &Company) -> Option<(String, &'static str)> {
    if !realtime(c) {
        return None;
    }
    let behind = ck::behind(c, real_now());
    if behind >= ck::AWAY_TOLD {
        return Some((trf("%{span} to catch up", &[("span", &words::span_text(behind))]), "accent"));
    }
    match &c.clock.real.caught {
        Some(k) => Some((trf("Caught up %{span}", &[("span", &words::span_text(k.to - k.from))]), "ok")),
        None => Some((tr("In step with the real clock"), "dim")),
    }
}

/// The company's now as a dialog's title says it.
pub fn now_label(c: &Company) -> String {
    format!("{}  ·  {}", day_label(&c.date), ck::hhmm(ck::now(c)))
}

// --- the bar and the strip under it ---------------------------------------------------------------

/// The company's bar and the strip under it, the cover over the pages and what lies over them
/// (`company::screen`): sent at every tick.
fn shell(s: &mut Session) -> Value {
    let notices: Vec<Value> = std::mem::take(&mut s.notices).into_iter().map(|(t, e)| json!({ "text": t, "error": e })).collect();
    let popup = s.popup.take();
    let base = json!({ "gen": s.generation, "notices": notices, "popup": popup, "companies": s.companies.as_ref().map(Vec::len) });
    let Some(c) = s.company.clone() else {
        return json!({ "open": false, "gen": base["gen"], "notices": base["notices"], "popup": base["popup"], "companies": base["companies"] });
    };
    let in_game = game_leads(s);
    let casual = c.game_mode.casual();
    // what the company has: cash, trophies, reputation, each a click from the page about it
    let status = [
        ("payments", eur(c.cash), "ok", if c.cash >= 0 { "text" } else { "danger" }, "Cash: the money the company has now", 4),
        ("emoji_events", grouped(c.trophies as f64), "warn", "text", if co::autopilot::on(&c) { "Trophies: earned with goals, spent by the company on research by itself" } else { "Trophies: earned with goals and contracts, spent on research" }, RESEARCH_TAB),
        ("star", format!("{} %", num(c.reputation.clamp(0.0, 100.0), 1)), "early", "text", "Reputation: what passengers and the authority think of the company", 0),
    ];
    let pages = pages_of(c.game_mode);
    let status: Vec<Value> = status
        .iter()
        .map(|(icon, value, tone, text_tone, tip, tab)| {
            let tab = if casual { casual::page_for(*tab) } else { *tab };
            json!({ "icon": icon, "value": value, "tone": tone, "text": text_tone, "tip": tr(tip), "tab": pages.contains(&tab).then_some(tab) })
        })
        .collect();
    // the clock's head
    let date = day_label(&c.date);
    let time = ck::hhmm(ck::now(&c));
    let live = realtime(&c) && !in_game;
    let lead = if live { c.clock.real.lead } else { 0 };
    let head = json!({
        "label": tr(if in_game { "Company time · the game's" } else { "Company time" }).to_uppercase(),
        "date": date, "time": time, "live": live, "in_game": in_game,
        "lead": (lead > 0).then(|| format!("+{}", words::span_text(lead))),
        "speed": s.clock.speed, "real": realtime(&c),
        "tip": (!in_game).then(|| realtime_tip(&c)),
    });
    // the strip: the pages with their badges
    let marks = home::Marks::of(&c, s.plan.as_ref());
    let rail: Vec<Vec<Value>> = if casual {
        vec![casual::PAGES.iter().map(|t| casual::rail_item(*t)).collect()]
    } else {
        RAIL.iter()
            .map(|g| {
                g.iter()
                    .map(|&tab| {
                        let mut tip = tr(TABS[tab]);
                        let badge = marks.badge(tab).map(|(n, tone, why)| {
                            tip = format!("{tip}\n{why}");
                            json!({ "n": n, "tone": tone })
                        });
                        json!({ "tab": tab, "icon": ICONS[tab], "name": tr(TABS[tab]), "badge": badge, "tip": tip, "away": HANDED_OVER.contains(&tab) })
                    })
                    .collect()
            })
            .collect()
    };
    let cover = s.closing.then(|| match s.clock.catching {
        Some(m) => trf("Catching up %{span} of the company's time…", &[("span", &words::span_text(m))]),
        None => tr("Simulating the company's time…"),
    });
    json!({
        "open": true,
        "gen": s.generation,
        "notices": base["notices"],
        "popup": base["popup"],
        "companies": base["companies"],
        "id": c.id, "name": c.name, "mark": mark_json(&c), "mode": mode_json(c.game_mode), "home": home_of(&c),
        "status": status, "clock": head, "rail": rail, "pages": pages,
        "names": TABS.iter().map(|t| tr(t)).collect::<Vec<_>>(),
        "casual": casual, "cover": cover,
        "asking": c.clock.ask.is_some(),
        "reports": s.reports.is_some(), "away": s.clock.away.is_some(),
        "launch": s.launch.is_some(),
        "sheet": s.dealer.sheet.is_some(),
        "casual_pages": if casual { casual::pages_json(&c.name) } else { Value::Null },
        "today_read": s.map_lines().is_some(),
    })
}

// --- opening, founding and leaving -----------------------------------------------------------------

/// Another of the driver's companies opened (None: none is left): a playing clock stops and saves
/// the one open first, and what the pages kept of it is let go (`company::switch_company`).
fn switch_company(s: &mut Session, to: Option<Company>) {
    play(s, 0.0);
    s.company = to;
    s.plan = None;
    s.reports = None;
    s.launch = None;
    s.clock.away = None;
    s.report_line = None;
    s.dealer = Default::default();
    s.week = Week::default();
    s.casual = Default::default();
    s.generation += 1;
}

/// The bus company entered (`chooser::choose`): a playing clock stops, the companies are read
/// again, and the start's tiles are said.
fn boot(s: &mut Session) -> Answer {
    play(s, 0.0);
    s.companies_for = None;
    work(s);
    // (the company the game-drawn launcher wants opened, or the one open here)
    Ok(json!({ "open": s.company.as_ref().map(|c| c.id.clone()), "chooser": chooser::view(s) }))
}

fn open(s: &mut Session, id: &str) -> Answer {
    let Some(c) = s.companies.as_ref().and_then(|l| l.iter().find(|c| c.id == id).cloned()) else { return Err(format!("no company {id}")) };
    switch_company(s, Some(c));
    Ok(json!({ "open": id }))
}

/// Delete a company for good (`company::delete_company`).
fn delete(s: &mut Session, id: &str) -> Answer {
    if s.closing {
        let t = tr("Wait until the company's day is simulated.");
        s.say(t, true);
        return Ok(json!({}));
    }
    let name = s.companies.as_ref().and_then(|cs| cs.iter().find(|x| x.id == id)).map(|x| x.name.clone()).unwrap_or_default();
    let open = s.company.as_ref().is_none_or(|c| c.id == id);
    if open {
        play(s, 0.0);
    }
    if let Err(e) = co::store::delete(&data(), id) {
        s.say(format!("{e:#}"), true);
        return Ok(json!({}));
    }
    if let Some(list) = s.companies.as_mut() {
        list.retain(|x| x.id != id);
    }
    if open {
        switch_company(s, None);
    }
    s.plan = None;
    s.reports = None;
    let t = trf("%{name} is deleted.", &[("name", &name)]);
    s.say(t, false);
    Ok(json!({ "deleted": id, "chooser": chooser::view(s) }))
}

/// The company's pages left (their way back, a hand-over, a duty to drive): a playing clock stops
/// and what it left unsaved is saved (`company::leave`); the duty launcher closes with them.
fn leave(s: &mut Session) -> Answer {
    play(s, 0.0);
    s.launch = None;
    Ok(json!({}))
}

// --- the commands --------------------------------------------------------------------------------

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

fn str_arg(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or("").to_string()
}

/// A command of the bus company, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    Some(match cmd {
        "co_boot" => with(boot),
        "co_open" => with(|s| open(s, &str_arg(args, "id"))),
        "co_delete" => with(|s| delete(s, &str_arg(args, "id"))),
        "co_leave" => with(leave),
        "co_found" => with(|s| chooser::found(s, args)),
        "co_wizard" => with(|s| chooser::wizard(s, args)),
        "co_tick" => with(|s| {
            if s.company.is_some() && !args.get("still").and_then(Value::as_bool).unwrap_or(false) {
                tick(s, args.get("modal").and_then(Value::as_bool).unwrap_or(false));
            }
            Ok(shell(s))
        }),
        "co_company" => with(|s| Ok(serde_json::to_value(&s.company).unwrap_or(Value::Null))),
        "co_view" => with(|s| view(s, args)),
        "co_act" => with(|s| do_act(s, args, p)),
        "co_hand_over" => with(|s| hand_over(s, args)),
        _ => return None,
    })
}

/// A command of the bus company that answers bytes: a staff portrait's JPEG (`co_portrait`, `n`
/// 1 to 300). None when `cmd` is not one.
pub fn call_bytes(cmd: &str, args: &Value) -> Option<Result<Vec<u8>, String>> {
    match cmd {
        "co_portrait" => {
            let n = args.get("n").and_then(Value::as_u64).unwrap_or(0) as u32;
            Some(portraits::jpeg(n).map(<[u8]>::to_vec).ok_or_else(|| format!("no portrait {n}")))
        }
        _ => None,
    }
}

/// The view of what the page shows (`view`: home, overview, fleet …, a dialog's).
fn view(s: &mut Session, args: &Value) -> Answer {
    let what = str_arg(args, "view");
    if what == "chooser" {
        return Ok(chooser::view(s));
    }
    if s.company.is_none() {
        return Ok(json!({ "none": true }));
    }
    Ok(match what.as_str() {
        "home" => home::view(s),
        "overview" => overview::view(s),
        "fleet" => fleet::view(s, args),
        "dealer" => dealer::view(s, args),
        "dealer_sheet" => dealer::sheet_view(s, args),
        "people" => people::view(s, args),
        "lines" => lines::view(s, args),
        "money" => money::view(s, args),
        "inbox" => inbox::view(s, args),
        "career" => career::view(s, args),
        "casual" => casual::view(s, args),
        "launch" => launch::view(s, args),
        "settings" | "time" | "confirm" | "report" | "away" | "ask" | "vehicle" | "new" | "add_line" | "fare" => dialogs::view(s, &what, args),
        other => return Err(format!("no view {other}")),
    })
}

/// What a click does (`act`: the action, and what it needs).
fn do_act(s: &mut Session, args: &Value, p: &dyn Platform) -> Answer {
    let what = str_arg(args, "act");
    let r = match what.as_str() {
        // the clock
        "hour" => step(s, Step::Minutes(60), false),
        "tomorrow" => step(s, Step::Midnight, false),
        "quick_close" => step(s, Step::Midnight, true),
        "step" => {
            let st = match str_arg(args, "step").as_str() {
                "morning" => Step::Morning,
                "midnight" => Step::Midnight,
                "until" => Step::Until(arg::<i64>(args, "minute").unwrap_or(16 * 60)),
                "days" => Step::Days(arg::<i64>(args, "n").unwrap_or(1)),
                _ => Step::Minutes(arg::<i64>(args, "n").unwrap_or(60)),
            };
            step(s, st, false)
        }
        "play" => {
            let speed = arg::<f64>(args, "speed").unwrap_or(0.0);
            if speed != 0.0 && !may_step(s) {
                Ok(json!({ "popup": busy(s) }))
            } else {
                play(s, speed);
                Ok(json!({}))
            }
        }
        "until" => {
            s.clock.until = arg::<f64>(args, "minute").unwrap_or(16.0 * 60.0);
            Ok(json!({}))
        }
        "dispatcher" => {
            let on = arg::<bool>(args, "on").unwrap_or(false);
            if let Some(c) = s.company.as_mut() {
                c.clock.dispatcher = on;
            }
            changed(s);
            Ok(json!({}))
        }
        "feed_minor" => {
            s.feed_minor = arg::<bool>(args, "on").unwrap_or(false);
            s.generation += 1;
            Ok(json!({}))
        }
        "close_reports" => {
            s.reports = None;
            s.report_line = None;
            Ok(json!({}))
        }
        "report_line" => {
            let line = str_arg(args, "line");
            s.report_line = if s.report_line.as_deref() == Some(line.as_str()) { None } else { Some(line) };
            Ok(json!({}))
        }
        "show_report" => {
            if let Some(rep) = s.company.as_ref().and_then(|c| c.last_report.clone()) {
                s.reports = Some(vec![rep]);
            }
            Ok(json!({}))
        }
        "close_away" => {
            let report = arg::<bool>(args, "report").unwrap_or(false);
            if let Some(a) = s.clock.away.take() {
                if report && !a.reports.is_empty() {
                    s.reports = Some(a.reports);
                }
            }
            Ok(json!({}))
        }
        _ => dialogs::act(s, &what, args, p)
            .or_else(|| fleet::act(s, &what, args))
            .or_else(|| dealer::act(s, &what, args, p))
            .or_else(|| people::act(s, &what, args))
            .or_else(|| lines::act(s, &what, args))
            .or_else(|| money::act(s, &what, args))
            .or_else(|| inbox::act(s, &what, args))
            .or_else(|| career::act(s, &what, args, p))
            .or_else(|| casual::act(s, &what, args, p))
            .or_else(|| launch::act(s, &what, args, p))
            .unwrap_or_else(|| Err(format!("no action {what}"))),
    };
    r.map(|mut v| {
        // (the session's own popup goes with the answer)
        if v.get("popup").is_none() {
            if let Some(p) = s.popup.take() {
                v["popup"] = p;
            }
        }
        v
    })
}

/// A page of a later phase opened in the game-drawn launcher, on that page and with this company
/// (`OMSI_LAUNCHER_PAGE` "bus company:<page>:<company>"; `page` a number of `TABS`, a Casual
/// page's name, or "home"): the clock stops and the company is saved before the window goes.
fn hand_over(s: &mut Session, args: &Value) -> Answer {
    leave(s)?;
    let part = str_arg(args, "page");
    let id = s.company.as_ref().map(|c| c.id.clone()).unwrap_or_default();
    Ok(json!({ "page": hand_over_page(&part, &id) }))
}

/// The `OMSI_LAUNCHER_PAGE` that opens the game-drawn launcher on the company's page `part` with
/// the company `id` (`part` a page, or what to open there - "tour", "line-editor[=<id>]",
/// "map-line=<name>", "casual-line=<freq>", "timetable=<line>", "livery=<bus>|<livery>|<id>":
/// `company::handed` of the game-drawn launcher - its ':' and '%' written %3A and %25).
pub fn hand_over_page(part: &str, id: &str) -> String {
    let part = part.replace('%', "%25").replace(':', "%3A");
    if id.is_empty() {
        format!("bus company:{part}")
    } else {
        format!("bus company:{part}:{id}")
    }
}

/// A day of the company as the views write it with its time (`clock::text` of a feed line).
pub fn feed_text(f: &ck::FeedItem) -> String {
    let mut t = tr(&f.text);
    for (k, v) in &f.args {
        let v = match k.as_str() {
            "amount" => v.parse::<i64>().map(eur).unwrap_or_else(|_| v.clone()),
            "date" => day_label(v),
            "month" => words::month_label(v),
            "why" | "job" | "area" => tr(v),
            "bus" if v == "rental" => tr("rental bus"),
            _ => v.clone(),
        };
        t = t.replace(&format!("%{{{k}}}"), &v);
    }
    t
}

/// A feed line's tone.
pub fn feed_tone(level: Level) -> &'static str {
    match level {
        Level::Minor => "dim",
        Level::Info => "soft",
        Level::Good => "ok",
        Level::Warn => "warn",
        Level::Bad => "danger",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_are_the_game_drawn_launchers() {
        assert_eq!(TABS.len(), ICONS.len());
        let mut all: Vec<usize> = RAIL.iter().flat_map(|g| g.iter().copied()).collect();
        all.sort_unstable();
        assert_eq!(all, (0..TABS.len()).collect::<Vec<_>>());
        // every page is drawn here or handed over, once
        let mut both: Vec<usize> = DRAWN.iter().chain(HANDED_OVER.iter()).copied().collect();
        both.sort_unstable();
        assert_eq!(both, (0..TABS.len()).collect::<Vec<_>>());
        assert_eq!(page_name(HOME_TAB), "Start screen");
        assert_eq!(hand_over_page("7", "stadtbus"), "bus company:7:stadtbus");
        assert_eq!(hand_over_page("home", ""), "bus company:home");
        assert_eq!(started_at(&["--time".into(), "06:45".into()]), Some(405));
        for m in co::GameMode::ALL {
            assert_eq!(mode_of(mode_key(m)), Some(m));
        }
    }
}

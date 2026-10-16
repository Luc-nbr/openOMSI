//! The company's duty launcher (`crates/omsi-launcher-ui/src/company/planning/duty_launch.rs`):
//! one duty of the planning, fixed - the company, the day and its times, the trips in order with
//! the breaks, the deadheads and the relief points, the bus the planning gives the tour, the depot
//! file and the map - what keeps it from being driven, and its start. Only the weather is chosen
//! here. "Start duty" starts the game with exactly the duty (`co::duty_launch::start`), with the
//! command line the game-drawn launcher builds: the page's duty with the company's fixed in it.

use super::kit::Popup;
use super::words::{day_label, tr, trf};
use super::{lines_of, plan_of, week_work, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::{self as dl, DutyRef, Hitch, Row, Sheet};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::time::Instant;

/// The launcher open: the duty, the page it was opened from, the weather chosen, and the player's
/// trip reports (read again now and then).
pub struct DutyLaunch {
    pub duty: DutyRef,
    pub from_tab: usize,
    pub weather: String,
    trips: Vec<core::TripRun>,
    trips_read: Option<Instant>,
}

impl DutyLaunch {
    pub fn new(duty: DutyRef, from_tab: usize) -> DutyLaunch {
        DutyLaunch { duty, from_tab, weather: "cycle".into(), trips: Vec::new(), trips_read: None }
    }
}

fn hhmm(m: i32) -> String {
    co::clock::hhmm(m as i64)
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

/// The sheet as it is now (None: the planning is being read), with the hitches the launcher sees
/// besides the company's (the bus not installed).
fn sheet_now(s: &mut Session, c: &Company) -> Option<Sheet> {
    let r = s.launch.as_ref()?.duty.clone();
    let stale = s.launch.as_ref().is_some_and(|x| x.trips_read.is_none_or(|t| t.elapsed().as_secs_f32() > 5.0));
    if stale {
        let trips = core::trips_of(&super::data(), &c.profile);
        if let Some(x) = s.launch.as_mut() {
            x.trips = trips;
            x.trips_read = Some(Instant::now());
        }
    }
    let now = co::clock::now(c);
    let mut sh = if co::dates::between(&c.date, &r.date) < 0 {
        Sheet { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), map: c.map.clone(), hitches: vec![Hitch::Over], ..Default::default() }
    } else {
        week_work(s);
        match plan_of(s, &r.date)? {
            Ok(p) => {
                let trips = s.launch.as_ref().map(|x| x.trips.clone()).unwrap_or_default();
                dl::sheet(c, &p, &r, now, &trips)
            }
            Err(_) => Sheet { duty: r.clone(), company: c.id.clone(), company_name: c.name.clone(), map: c.map.clone(), hitches: vec![Hitch::Timetable], ..Default::default() },
        }
    };
    if let Some(b) = sh.bus.as_ref().filter(|b| !b.file.trim().is_empty()) {
        let same = |f: &str| f.replace('\\', "/").eq_ignore_ascii_case(&b.file.replace('\\', "/"));
        let read = s.vehicles_read();
        let file = b.file.clone();
        if read && !s.vehicles().iter().any(|v| same(&v.file)) {
            sh.hitches.push(Hitch::NotInstalled(file));
        }
    }
    Some(sh)
}

/// What a hitch is about in particular.
fn detail(h: &Hitch) -> Option<String> {
    match h {
        Hitch::NoBus(Some(p)) | Hitch::Bus(p) => Some(tr(p.label())),
        Hitch::NotYours(name) => Some(trf("Planned for %{name}", &[("name", &tr(name))])),
        Hitch::NotInstalled(file) => Some(file.clone()),
        _ => None,
    }
}

/// Where the duty is put right: RosterFlow, the planning, or in Casual its shifts.
fn to_planning_label(c: &Company) -> &'static str {
    let shown = super::pages_of(c.game_mode).contains(&5);
    if shown && co::autopilot::rosterflow(c) {
        "Open in RosterFlow"
    } else if shown {
        "Open the planning"
    } else {
        "Back to the shifts"
    }
}

/// The duty launcher's view.
pub fn view(s: &mut Session, _args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let Some(sheet) = sheet_now(s, &c) else {
        return json!({ "reading": tr("Reading the planning…"), "name": c.name, "mark": super::mark_json(&c), "caps": tr("Company duty · fixed by the planning"), "back": tr("Back"), "to": tr(to_planning_label(&c)) });
    };
    let lines = sheet.lines.join(", ");
    let none = sheet.rows.is_empty();
    let tiles = json!([
        [tr("Day").to_uppercase(), day_label(&sheet.duty.date)],
        [tr("Sign on").to_uppercase(), if none { "-".into() } else { hhmm(sheet.from) }],
        [tr("Sign off").to_uppercase(), if none { "-".into() } else { hhmm(sheet.to) }],
        [tr("Length").to_uppercase(), if none { "-".into() } else { length(sheet.to - sheet.from) }],
        [tr("Trips").to_uppercase(), trf("%{n} with passengers · %{m} deadheads", &[("n", &sheet.trips.to_string()), ("m", &sheet.deadheads.to_string())])],
        [tr("Lines").to_uppercase(), if lines.is_empty() { "-".into() } else { lines }],
    ]);
    let colour = co::rosterflow::line_colour(&c, &sheet.duty.line);
    let rows: Vec<Value> = sheet
        .rows
        .iter()
        .map(|row| match row {
            Row::SignOn { at, time, relief } => json!({ "kind": "on", "time": hhmm(*time), "title": tr(if *relief { "Take the bus over (relief)" } else { "Sign on" }), "at": at }),
            Row::SignOff { at, time, relief } => json!({ "kind": "off", "time": hhmm(*time), "title": tr(if *relief { "Hand the bus over (relief)" } else { "Sign off" }), "at": at }),
            Row::Break { at, from, to } => {
                let mins = to - from;
                json!({ "kind": "break", "text": format!("{} · {} · {}", tr(if mins >= 15 { "Break" } else { "Layover" }), trf("%{n} min", &[("n", &mins.to_string())]), at) })
            }
            Row::Trip { dep, arr, line, from, to, empty, .. } => json!({
                "kind": "trip", "dep": hhmm(*dep), "arr": hhmm(*arr), "empty": empty, "line": if line.trim().is_empty() { "?".to_string() } else { line.trim().to_string() },
                "route": format!("{from}  →  {to}"), "deadhead": tr("Deadhead"), "length": trf("%{n} min", &[("n", &(arr - dep).to_string())]),
            }),
        })
        .collect();
    let map = if sheet.map_name.trim().is_empty() { super::words::short_map(&sheet.map) } else { sheet.map_name.clone() };
    let bus_rows: Vec<Value> = match sheet.bus.as_ref() {
        Some(b) => vec![
            json!([tr("Bus"), if b.number.trim().is_empty() { b.name.clone() } else { format!("{} · {}", b.number.trim(), b.name) }]),
            json!([tr("Livery"), if b.paint.trim().is_empty() { tr("The company's") } else { b.paint.clone() }]),
            json!([tr("Number plate"), b.plate.clone()]),
            json!([tr("Depot file"), sheet.hof.clone()]),
            json!([tr("Map"), map]),
        ],
        None => vec![json!([tr("Bus"), tr("Not planned")]), json!([tr("Depot file"), sheet.hof.clone()]), json!([tr("Map"), map])],
    };
    let start = match sheet.hitches.first() {
        Some(h) => {
            let (title, what) = h.words();
            json!({
                "blocked": true, "caps": tr("Cannot start").to_uppercase(), "title": tr(title), "detail": detail(h), "what": tr(what),
                "more": sheet.hitches.iter().skip(1).map(|x| tr(x.words().0)).collect::<Vec<_>>(),
                "fix": tr(to_planning_label(&c)), "off": tr("Start duty").to_uppercase(), "why": trf("Not possible yet: %{reason}", &[("reason", &tr(title))]),
            })
        }
        None => {
            let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
            let lead = core::bus_lead(settings.get("bus_lead").and_then(|x| x.as_i64()));
            let first = sheet.planned.iter().find(|x| !x.empty).map(|x| x.dep).unwrap_or(sheet.from);
            let (date, time) = core::lead_start(&sheet.duty.date, first, lead);
            let airport = super::super::nearest_airport(&core::root_dir().unwrap_or_default(), &c.map);
            let mut weathers = vec![json!(["cycle", tr("As forecast (weather cycle)")]), json!(["", tr("Map default")]), json!([format!("metar:{airport}"), trf("Current weather at %{code}", &[("code", &airport)])])];
            weathers.extend(core::list_weather().unwrap_or_default().into_iter().map(|w| json!([w.file, w.name])));
            json!({
                "blocked": false, "caps": tr("Before you start").to_uppercase(), "weather_label": tr("Weather"), "weathers": weathers,
                "weather": s.launch.as_ref().map(|x| x.weather.clone()).unwrap_or_else(|| "cycle".into()),
                "when": trf("The game starts on %{date} at %{time}, %{lead} min before the first departure: the company's clock, not the computer's.", &[("date", &day_label(&date)), ("time", &hhmm(time)), ("lead", &lead.to_string())]),
                "fixed": tr("Nothing of the duty can be changed here: the planning fixes it. Only the weather is yours to choose; sound, camera and traffic are as in the Settings."),
                "go": tr("Start duty"),
            })
        }
    };
    json!({
        "name": c.name, "mark": super::mark_json(&c), "caps": tr("Company duty · fixed by the planning"),
        "title": (!sheet.number.is_empty()).then(|| format!("{} {}  ·  {} {}  ·  {} {}", tr("Line"), sheet.number, tr("Tour"), sheet.duty.tour, tr("Duty"), sheet.duty.duty + 1)),
        "back": tr("Back"), "to": tr(to_planning_label(&c)), "tiles": tiles,
        "duty_caps": tr("The duty as planned").to_uppercase(), "rows": rows, "no_rows": tr("Nothing to show: the planning no longer has this duty."),
        "colour": format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2]),
        "bus_caps": tr("Bus and depot").to_uppercase(), "bus": bus_rows, "bus_note": tr("The planning gives the duty its bus, in the company's livery: it is not chosen here."),
        "map_caps": tr("Route of the duty").to_uppercase(), "map": c.map, "date": sheet.duty.date, "trips": sheet.trip_names(),
        "start": start,
    })
}

/// "Start duty" (`duty_launch::start`): the game started with exactly the duty - the page's
/// duty (`base`: the player's game options) with everything of the company's fixed in it - or,
/// a dry run (`dry`), the command line it becomes.
fn start(s: &mut Session, args: &Value, p: &dyn crate::Platform) -> Result<Value, String> {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let dry = args.get("dry").and_then(Value::as_bool).unwrap_or(false);
    if !dry && s.in_game() {
        return Ok(json!({ "popup": super::popup(s, Popup::new("sports_esports", "A game is running", tr("The duty waits until the game has ended."), tr("Close the game, then start the duty again."), None)) }));
    }
    let root = core::load_config().root;
    if !dry && !omsi_cfg::missing_original_essentials(std::path::Path::new(&root)).is_empty() {
        return Ok(json!({ "popup": super::popup(s, Popup::new("folder_open", "The original OMSI 2 is missing", tr("A session needs the original OMSI 2."), tr("Choose its folder under Setup first."), None)) }));
    }
    let Some(sheet) = sheet_now(s, &c) else { return Ok(json!({ "popup": super::popup(s, Popup::new("timer", "Not yet", tr("The timetable of the day is still being read."), tr("Try again in a moment."), None)) })) };
    let Some(lines) = lines_of(s, &sheet.duty.date) else {
        return Ok(json!({ "popup": super::popup(s, Popup::new("timer", "Not yet", tr("The timetable of the day is still being read."), tr("Try again in a moment."), None)) }));
    };
    let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
    let lead = core::bus_lead(settings.get("bus_lead").and_then(|x| x.as_i64()));
    let st = match dl::start(&sheet, &lines, lead) {
        Ok(st) => st,
        Err(h) => return Ok(json!({ "popup": super::popup(s, Popup::new("error", h.words().0, tr(h.words().1), String::new(), None)) })),
    };
    let base: core::Duty = serde_json::from_value(args.get("base").cloned().unwrap_or(Value::Null)).map_err(|e| format!("base: {e}"))?;
    let weather = s.launch.as_ref().map(|x| x.weather.clone()).unwrap_or_else(|| "cycle".into());
    let mut d = st.duty(base, &weather);
    d.display_font = core::busfonts::BusFonts::load().font_for(&st.bus);
    // (the bus options chosen for the bus, as `duty_launch::start` passes them: Omsi-Hub 6/6)
    d.set_vars = super::super::hub_more::setvars(&st.bus, &st.paint);
    if dry {
        return Ok(json!({ "args": core::duty_args(&d).map_err(|e| format!("{e:#}"))? }));
    }
    // (a later day's duty: that day's plan for the game too)
    if sheet.duty.date != c.date {
        if let Some(Ok(p)) = plan_of(s, &sheet.duty.date) {
            if let Err(e) = co::plan::save_drive_plan(&super::data(), &co::plan::live_plan(&c, &p)) {
                core::log_to_file(&format!("company: the plan of {} for the game: {e:#}", p.date));
            }
        }
    }
    core::log_to_file(&format!("company duty launcher: {} {} duty {} on {} with {}", st.line, st.tour, sheet.duty.duty + 1, st.date, st.bus));
    // (the add-on's plugin told of the company's duty: `addon.rs`)
    super::super::addon::assign_duty(&d, Some((&c.name, &format!("{} {} {}", st.line, st.tour, sheet.duty.duty + 1))));
    let launched = core::launch(&d).map_err(|e| format!("{e:#}"))?;
    core::log_to_file(&format!("game {}: {}", launched.pid, launched.command));
    p.launched();
    let t = tr("Starting your company's duty…");
    s.say(t, false);
    Ok(json!({ "launched": serde_json::to_value(launched).unwrap_or(Value::Null) }))
}

/// What the duty launcher does.
pub fn act(s: &mut Session, what: &str, args: &Value, p: &dyn crate::Platform) -> Option<Result<Value, String>> {
    Some(match what {
        "launch_weather" => {
            let w = args.get("weather").and_then(Value::as_str).unwrap_or("cycle").to_string();
            if let Some(x) = s.launch.as_mut() {
                x.weather = w;
            }
            s.generation += 1;
            Ok(json!({}))
        }
        "launch_close" => {
            let from = s.launch.take().map(|x| x.from_tab);
            s.generation += 1;
            Ok(json!({ "tab": from }))
        }
        "launch_fix" => {
            let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
            let x = s.launch.take();
            s.generation += 1;
            // (the planning is the game-drawn launcher's until its phase; Casual's are its shifts)
            if super::pages_of(c.game_mode).contains(&5) {
                // Omsi-Hub 4/6: the duty with it, for RosterFlow's Roster to open on it
                // (`duty_launch::to_planning`).
                let duty = x.as_ref().map(|x| json!({ "date": x.duty.date, "line": x.duty.line, "tour": x.duty.tour, "duty": x.duty.duty }));
                Ok(json!({ "hand_over": "5", "date": x.map(|x| x.duty.date), "duty": duty }))
            } else {
                Ok(json!({ "tab": super::casual::SHIFTS }))
            }
        }
        "launch_start" => start(s, args, p),
        _ => return None,
    })
}

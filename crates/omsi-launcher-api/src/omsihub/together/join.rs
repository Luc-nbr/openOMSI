//! A friend's side of "Play together": joined with the boss's code, his launcher keeps in touch
//! with the boss's on a thread of its own (every few seconds, whatever page is shown and while the
//! game runs: who he is and what he drives, and the trips his game reports, sent as they end),
//! and shows the boss's company in a simple view - the day, the open duties to take, his own
//! duties to drive or give back, his results. Drive starts his game on the duty in the boss's
//! world (`--lan-join` with the boss's game session code). What he drove for a company is kept in
//! his own history (`~/.openomsi/together/<driver>-history.json`); his career counts the trips
//! from his own trip records, as every drive's.

use super::super::company::words::{day_label, tr, trf};
use super::super::company::{self};
use super::{duty_arg, duty_words, notice_json, unix_now};
use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::together::client;
use omsi_launcher_lib::company::together::code::{self, CompanyCode};
use omsi_launcher_lib::company::together::wire::{DriveStart, DutyState, Notice, Request, SnapDuty, SnapResult, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How often the boss's launcher is asked (s).
const EVERY: Duration = Duration::from_millis(2500);
/// After this many failed messages in a row the boss's launcher is looked for again.
const LOST_AFTER: u32 = 2;
/// How many notices are kept.
const NOTICES_KEPT: usize = 60;
/// How many duties the history keeps.
const HISTORY_KEPT: usize = 300;

/// Who the player is: the id his launcher keeps (per driver), his name.
#[derive(Clone, Debug)]
struct Me {
    id: String,
    name: String,
}

/// The duty his game drives now.
#[derive(Clone, Debug)]
struct Drive {
    duty: DutyRef,
    /// The line's number and the timetable's tour the game drives.
    number: String,
    tour: String,
    /// When it started (Unix seconds, a minute's slack before) and the game's process.
    since: u64,
    pid: u32,
    /// The departures (minutes) reported already.
    sent: Vec<i32>,
}

struct Joined {
    text: String,
    code: CompanyCode,
    me: Me,
    /// Where the boss's launcher answered (None: being looked for).
    base: Option<String>,
    snap: Option<Snapshot>,
    since: u64,
    notices: Vec<Notice>,
    /// Why the boss's launcher is not reached now (English).
    lost: Option<String>,
    failures: u32,
    drive: Option<Drive>,
    stop: Arc<AtomicBool>,
    /// Ask again at once.
    wake: bool,
    /// What the boss's launcher refused last, and when (English, with its arguments).
    said: Option<(String, Instant)>,
}

static JOINED: Mutex<Option<Joined>> = Mutex::new(None);

fn joined() -> MutexGuard<'static, Option<Joined>> {
    JOINED.lock().unwrap_or_else(|e| e.into_inner())
}

fn data() -> PathBuf {
    core::data_dir()
}

/// A driver's name as a file name.
fn slug(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    if s.is_empty() {
        "driver".into()
    } else {
        s
    }
}

/// The driver the launcher drives as now.
fn driver() -> String {
    let p = core::load_config().profile;
    if p.trim().is_empty() {
        "Driver".into()
    } else {
        p.trim().to_string()
    }
}

/// Who the player is (his id made the first time and kept: the boss's company knows him by it).
fn me() -> Me {
    let name = driver();
    let path = data().join("together").join(format!("{}-me.json", slug(&name)));
    let kept = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string)).filter(|s| s.len() >= 16);
    let id = kept.unwrap_or_else(|| {
        let b: [u8; 12] = omsi_net::http::random_bytes();
        let id: String = b.iter().map(|x| format!("{x:02x}")).collect();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&path, json!({ "id": id }).to_string());
        id
    });
    Me { id, name }
}

fn request(j: &Joined, what: &str) -> Request {
    Request { v: code::VERSION, secret: j.code.secret_hex(), company: j.code.company, player: j.me.id.clone(), name: j.me.name.clone(), what: what.into(), driving: j.drive.as_ref().map(|d| d.duty.clone()), in_game: j.drive.is_some(), since: j.since, ..Default::default() }
}

/// An answer of the boss's launcher taken in: the day, the notices.
fn take_answer(j: &mut Joined, v: &Value) {
    if let Some(s) = v.get("snapshot").and_then(|x| serde_json::from_value::<Snapshot>(x.clone()).ok()) {
        for n in &s.notices {
            if n.n > j.since {
                j.notices.push(n.clone());
            }
        }
        j.since = s.notices.iter().map(|n| n.n).max().unwrap_or(j.since).max(j.since);
        if j.notices.len() > NOTICES_KEPT {
            let extra = j.notices.len() - NOTICES_KEPT;
            j.notices.drain(..extra);
        }
        j.snap = Some(s);
    }
    // (the session ended, or another began: the code is no good now)
    let error = v.get("error").and_then(Value::as_str).unwrap_or("");
    j.lost = [omsi_launcher_lib::company::together::host::BAD_SECRET, omsi_launcher_lib::company::together::host::NOT_OPEN_COMPANY, code::NEWER].contains(&error).then(|| error.to_string());
    j.failures = 0;
}

/// Send a message of the player now (from a page's action): the answer, the day taken in.
fn send_now(what: &str, fill: impl FnOnce(&mut Request)) -> Result<Value, String> {
    let (base, mut req) = {
        let g = joined();
        let j = g.as_ref().ok_or_else(|| tr("Not in a company session."))?;
        let base = j.base.clone().ok_or_else(|| tr("Looking for the boss's launcher…"))?;
        (base, request(j, what))
    };
    fill(&mut req);
    let v = client::send(&base, &req, client::TIMEOUT).map_err(|_| tr(client::UNREACHED))?;
    if let Some(j) = joined().as_mut() {
        take_answer(j, &v);
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            j.said = Some((e.to_string(), Instant::now()));
        }
    }
    Ok(v)
}

/// The trips the game reported of the duty since it started and not sent yet.
fn new_trips(d: &Drive, name: &str) -> Vec<core::TripRun> {
    core::trips_of(&data(), name)
        .into_iter()
        .filter(|t| t.time >= d.since && !t.free && t.tour.trim() == d.tour.trim() && t.departure.is_finite())
        .filter(|t| !d.sent.contains(&((t.departure / 60.0).round() as i32)))
        .collect()
}

/// Whether the game of the duty still runs.
fn game_runs(pid: u32) -> bool {
    core::instances::list().iter().any(|i| i.pid == pid && i.running)
}

/// The keeping in touch with the boss's launcher (a thread for the session).
fn run(stop: Arc<AtomicBool>) {
    let mut last = Instant::now() - EVERY;
    while !stop.load(Ordering::Relaxed) {
        let due = last.elapsed() >= EVERY || joined().as_ref().is_some_and(|j| j.wake);
        if !due {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        last = Instant::now();
        let Some((code, base, mut req, drive)) = joined().as_mut().map(|j| {
            j.wake = false;
            (j.code.clone(), j.base.clone(), request(j, "snapshot"), j.drive.clone())
        }) else {
            return;
        };
        let Some(base) = base else {
            req.what = "hello".into();
            match client::connect(&code, &req) {
                Ok((b, v)) => {
                    if let Some(j) = joined().as_mut() {
                        j.base = Some(b);
                        take_answer(j, &v);
                    }
                }
                Err(e) => {
                    if let Some(j) = joined().as_mut() {
                        j.lost = Some(e);
                    }
                }
            }
            continue;
        };
        // (driving: the trips as they end, and the end of the duty when the game ended)
        let mut done = false;
        if let Some(d) = drive.as_ref() {
            let trips = new_trips(d, &req.name);
            done = !game_runs(d.pid);
            if !trips.is_empty() || done {
                req.what = "report".into();
                req.duty = Some(d.duty.clone());
                req.trips = trips;
                req.done = done;
            }
        }
        match client::send(&base, &req, client::TIMEOUT) {
            Ok(v) => {
                let mut g = joined();
                let Some(j) = g.as_mut() else { return };
                take_answer(j, &v);
                if req.what == "report" {
                    if let Some(d) = j.drive.as_mut() {
                        d.sent.extend(req.trips.iter().map(|t| (t.departure / 60.0).round() as i32));
                    }
                    if done {
                        if let Some(d) = j.drive.take() {
                            remember(j, &d);
                        }
                    }
                }
            }
            Err(_) => {
                let mut g = joined();
                let Some(j) = g.as_mut() else { return };
                j.failures += 1;
                if j.failures >= LOST_AFTER {
                    j.base = None;
                    j.lost = Some(client::UNREACHED.to_string());
                }
            }
        }
    }
}

/// A duty driven for a company, as the player's history keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct Driven {
    company: String,
    company_name: String,
    boss: String,
    date: String,
    number: String,
    tour: String,
    duty: usize,
    trips: usize,
    score: u32,
    xp: i64,
    passengers: i32,
    km: f64,
    /// When it ended (Unix seconds).
    at: u64,
}

fn history_file() -> PathBuf {
    data().join("together").join(format!("{}-history.json", slug(&driver())))
}

fn history() -> Vec<Driven> {
    std::fs::read(history_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The duty just driven into the player's history (with what the boss's company made of its
/// trips, else what his game reported).
fn remember(j: &Joined, d: &Drive) {
    let res: Option<SnapResult> = j.snap.as_ref().and_then(|s| s.results.iter().find(|r| omsi_launcher_lib::company::together::book::same_duty(&r.duty, &d.duty)).cloned());
    let card = j.snap.as_ref().map(|s| s.card.clone()).unwrap_or_default();
    let mut e = Driven { company: card.id, company_name: card.name, boss: card.boss, date: d.duty.date.clone(), number: d.number.clone(), tour: d.duty.tour.clone(), duty: d.duty.duty + 1, at: unix_now(), ..Default::default() };
    match res {
        Some(r) => (e.trips, e.score, e.xp, e.passengers, e.km) = (r.trips, r.score, r.xp, r.passengers, r.km),
        None => {
            let trips: Vec<core::TripRun> = core::trips_of(&data(), &j.me.name).into_iter().filter(|t| t.time >= d.since && t.tour.trim() == d.tour.trim()).collect();
            let evs: Vec<_> = trips.iter().map(omsi_launcher_lib::company::career::evaluate).collect();
            e.trips = trips.len();
            e.xp = evs.iter().map(|x| x.xp).sum();
            e.score = if evs.is_empty() { 0 } else { evs.iter().map(|x| x.score).sum::<u32>() / evs.len() as u32 };
            e.passengers = trips.iter().map(|t| t.passengers.max(0)).sum();
            e.km = trips.iter().map(omsi_launcher_lib::company::career::believable_km).sum();
        }
    }
    let mut list = history();
    list.insert(0, e);
    list.truncate(HISTORY_KEPT);
    let path = history_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&list).unwrap_or_default());
}

/// The commands of the friend's side.
pub(super) fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    Some(match cmd {
        "tg_join" => Ok(join(args.get("code").and_then(Value::as_str).unwrap_or(""))),
        "tg_check" => Ok(check(args.get("code").and_then(Value::as_str).unwrap_or(""))),
        "tg_view" => Ok(view()),
        "tg_take" | "tg_release" => Ok(duty_act(if cmd == "tg_take" { "take" } else { "release" }, args)),
        "tg_drive" => drive(args, p),
        "tg_say" => {
            let text = args.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            Ok(match send_now("say", |r| r.text = text) {
                Ok(_) => json!({}),
                Err(e) => json!({ "error": e }),
            })
        }
        "tg_leave" => {
            leave();
            Ok(json!({}))
        }
        _ => return None,
    })
}

/// What a code typed in is (as it is typed): None when it reads.
fn check(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({ "ok": false });
    }
    match CompanyCode::decode(text) {
        Ok(_) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": tr(e) }),
    }
}

/// Join the company of `text` (a code): the keeping in touch started, the view.
fn join(text: &str) -> Value {
    let code = match CompanyCode::decode(text) {
        Ok(c) => c,
        Err(e) => return json!({ "error": tr(e) }),
    };
    leave();
    let stop = Arc::new(AtomicBool::new(false));
    *joined() = Some(Joined { text: text.trim().to_string(), code, me: me(), base: None, snap: None, since: 0, notices: Vec::new(), lost: None, failures: 0, drive: None, stop: stop.clone(), wake: true, said: None });
    let _ = std::thread::Builder::new().name("company session".into()).spawn(move || run(stop));
    view()
}

/// Leave the session (the boss's launcher told, best effort).
fn leave() {
    let gone = joined().take();
    let Some(j) = gone else { return };
    j.stop.store(true, Ordering::Relaxed);
    if let Some(base) = j.base.clone() {
        let req = request(&j, "leave");
        let _ = std::thread::Builder::new().name("company leave".into()).spawn(move || {
            let _ = client::send(&base, &req, Duration::from_secs(4));
        });
    }
}

/// Take or give back a duty.
fn duty_act(what: &str, args: &Value) -> Value {
    let Some(r) = duty_arg(args) else { return json!({}) };
    match send_now(what, |q| q.duty = Some(r)) {
        Ok(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => json!({}),
        Ok(v) => json!({ "error": tr(v.get("error").and_then(Value::as_str).unwrap_or(client::UNREACHED)) }),
        Err(e) => json!({ "error": e }),
    }
}

/// Drive a duty: its start asked of the boss's launcher, the game started on it in his world.
fn drive(args: &Value, p: &dyn Platform) -> Answer {
    let Some(r) = duty_arg(args) else { return Ok(json!({})) };
    // (one game at a time; the bus planned must be here)
    let (running, installed) = company::with(|s| {
        let file = joined().as_ref().and_then(|j| j.snap.as_ref()).and_then(|x| x.duties.iter().find(|d| omsi_launcher_lib::company::together::book::same_duty(&d.duty, &r)).map(|d| d.file.clone())).unwrap_or_default();
        let same = |f: &str| f.replace('\\', "/").eq_ignore_ascii_case(&file.replace('\\', "/"));
        let read = s.vehicles_read();
        let have = file.trim().is_empty() || !read || s.vehicles().iter().any(|v| same(&v.file));
        (s.in_game(), if have { None } else { Some(file) })
    });
    if running {
        return Ok(json!({ "error": tr("A game is running: close it, then drive the duty.") }));
    }
    if let Some(file) = installed {
        return Ok(json!({ "error": trf("The duty's bus is not installed on this computer: %{bus}. Install it, or ask the boss to give the tour another bus.", &[("bus", &file)]) }));
    }
    let root = core::load_config().root;
    if !omsi_cfg::missing_original_essentials(std::path::Path::new(&root)).is_empty() {
        return Ok(json!({ "error": tr("A session needs the original OMSI 2.") }));
    }
    let v = match send_now("start", |q| q.duty = Some(r.clone())) {
        Ok(v) => v,
        Err(e) => return Ok(json!({ "error": e })),
    };
    let Some(start) = v.get("start").and_then(|x| serde_json::from_value::<DriveStart>(x.clone()).ok()) else {
        return Ok(json!({ "error": tr(v.get("error").and_then(Value::as_str).unwrap_or(client::UNREACHED)) }));
    };
    let base: core::Duty = serde_json::from_value(args.get("base").cloned().unwrap_or(Value::Null)).map_err(|e| format!("base: {e}"))?;
    let name = driver();
    let mut d = start.duty(base, &name);
    d.display_font = core::busfonts::BusFonts::load().font_for(&start.bus);
    d.set_vars = super::super::hub_more::setvars(&start.bus, &start.paint);
    core::log_to_file(&format!("company session: {} drives {} {} duty {} in the boss's world", name, start.line, start.tour, r.duty + 1));
    let launched = core::launch(&d).map_err(|e| format!("{e:#}"))?;
    core::log_to_file(&format!("game {}: {}", launched.pid, launched.command));
    p.launched();
    let number = joined().as_ref().and_then(|j| j.snap.as_ref()).and_then(|s| s.duties.iter().find(|x| omsi_launcher_lib::company::together::book::same_duty(&x.duty, &r)).map(|x| x.number.clone())).unwrap_or_default();
    if let Some(j) = joined().as_mut() {
        j.drive = Some(Drive { duty: r, number, tour: start.tour.clone(), since: unix_now().saturating_sub(60), pid: launched.pid, sent: Vec::new() });
        j.wake = true;
    }
    Ok(json!({ "launched": serde_json::to_value(launched).unwrap_or(Value::Null) }))
}

/// A duty's line in the friend's view: what it is, who drives it, what he can do with it.
fn duty_row(d: &SnapDuty, open_claims: bool, world: bool) -> Value {
    let hhmm = |m: i32| format!("{:02}:{:02}", (m / 60).rem_euclid(24), m.rem_euclid(60));
    let mut facts = vec![trf("%{n} trips", &[("n", &d.trips.to_string())]), format!("{} km", d.km)];
    if !d.bus.is_empty() {
        facts.push(trf("Bus %{bus}", &[("bus", &tr(&d.bus))]));
    }
    let take = if open_claims { tr("Take this duty") } else { tr("Ask for this duty") };
    let (state, tone, acts): (String, &str, Vec<Value>) = match d.state {
        DutyState::Open => (tr("Open"), "accent", vec![json!({ "label": take, "act": "take", "kind": "primary" })]),
        DutyState::Mine => (tr("Yours"), "ok", vec![json!({ "label": tr("Drive"), "act": "drive", "kind": "primary", "off": !world }), json!({ "label": tr("Give back"), "act": "release", "kind": "" })]),
        DutyState::Asked => (tr("Asked for: the boss says yes or no"), "warn", vec![json!({ "label": tr("Withdraw"), "act": "release", "kind": "" })]),
        DutyState::Driving => (tr("You are driving it"), "ok", Vec::new()),
        DutyState::Done => (tr("Driven"), "dim", Vec::new()),
        DutyState::Player => (trf("%{name} has it", &[("name", &d.who)]), "soft", Vec::new()),
        DutyState::Company => (trf("Driven by %{name}", &[("name", &tr(&d.who))]), "dim", Vec::new()),
        DutyState::Closed => (tr(&d.why), "dim", Vec::new()),
    };
    json!({
        "duty": d.duty, "number": d.number, "colour": d.colour, "times": format!("{} – {}", hhmm(d.from), hhmm(d.to)),
        "route": format!("{} → {}", d.first, d.last), "facts": facts.join("  ·  "), "title": duty_words(&d.number, &d.duty),
        "state": state, "tone": tone, "acts": acts,
    })
}

/// The friend's view: not joined (the code field), looking for the boss, or the company's day.
fn view() -> Value {
    let words = json!({
        "title": tr("Join a company"), "text": tr("A friend runs a bus company and plays as its boss. Paste the session code he sent you: you drive duties of his company's day for it, in his world."),
        "field": tr("Session code (HUB-…)"), "join": tr("Join"), "back": tr("Back"),
        "need": [tr("Both need the same version of Omsi-Hub."), tr("The same network, the same VPN (Hamachi, Radmin VPN, ZeroTier, Tailscale), or the boss's tunnel."), tr("The boss's company map, and the buses of his duties, installed.")],
        "leave": tr("Leave the session"), "connecting": tr("Looking for the boss's launcher…"),
        "open_caps": tr("Open duties").to_uppercase(), "no_open": tr("No open duty right now: the boss can give you one."),
        "mine_caps": tr("My duties").to_uppercase(), "no_mine": tr("No duty of yours yet: take an open one."),
        "rest_caps": tr("The rest of the day").to_uppercase(),
        "results_caps": tr("My results").to_uppercase(), "history_caps": tr("My history").to_uppercase(),
        "players_caps": tr("Players").to_uppercase(), "notices_caps": tr("What happened").to_uppercase(),
        "say": tr("Say something to everyone"), "send": tr("Send"),
    });
    let g = joined();
    let Some(j) = g.as_ref() else {
        return json!({ "joined": false, "words": words, "history": history_rows() });
    };
    let lost = j.lost.as_deref().map(tr);
    let said = j.said.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 12).map(|(e, _)| tr(e));
    let Some(s) = j.snap.as_ref() else {
        return json!({ "joined": true, "connecting": true, "words": words, "code": j.text, "lost": lost });
    };
    let world = s.world;
    let rows = |pick: &dyn Fn(&SnapDuty) -> bool| -> Vec<Value> { s.duties.iter().filter(|d| pick(d)).map(|d| duty_row(d, s.open_claims, world)).collect() };
    let open = rows(&|d| d.state == DutyState::Open);
    let mine = rows(&|d| matches!(d.state, DutyState::Mine | DutyState::Asked | DutyState::Driving | DutyState::Done));
    let rest = rows(&|d| matches!(d.state, DutyState::Player | DutyState::Company | DutyState::Closed));
    let players: Vec<Value> = s
        .players
        .iter()
        .map(|p| {
            let text = match (&p.driving, p.boss, p.online) {
                (Some((n, tour, k)), _, _) => trf("Drives %{duty}", &[("duty", &duty_words(n, &DutyRef { tour: tour.clone(), duty: k.saturating_sub(1), ..Default::default() }))]),
                (None, true, _) => tr("The boss"),
                (None, false, true) => tr("Online"),
                (None, false, false) => tr("Offline"),
            };
            json!({ "name": p.name, "me": p.me, "boss": p.boss, "online": p.online, "text": text })
        })
        .collect();
    let results: Vec<Value> = s
        .results
        .iter()
        .map(|r| {
            let on_time = if r.stops > 0 { (100 * r.on_time / r.stops).to_string() + " %" } else { "-".into() };
            json!({ "title": duty_words(&r.number, &r.duty), "text": trf("%{trips} trips · %{passengers} passengers · %{km} km · on time %{p} · score %{score} · +%{xp} XP", &[("trips", &r.trips.to_string()), ("passengers", &r.passengers.to_string()), ("km", &format!("{:.0}", r.km)), ("p", &on_time), ("score", &r.score.to_string()), ("xp", &r.xp.to_string())]), "done": r.done })
        })
        .collect();
    let driving = j.drive.as_ref().map(|d| trf("Your game drives %{duty} in the boss's world: its trips go to the company as they end.", &[("duty", &duty_words(&d.number, &d.duty))]));
    let notices: Vec<Value> = j.notices.iter().rev().take(30).map(notice_json).collect();
    json!({
        "joined": true, "words": words, "code": j.text, "lost": lost, "said": said,
        "mark": { "short": s.card.short, "colours": s.card.colours, "logo": null },
        "name": s.card.name, "sub": trf("Boss: %{name}  ·  %{map}", &[("name", &s.card.boss), ("map", &s.card.map_name)]),
        "day": format!("{}  ·  {:02}:{:02}", day_label(&s.date), s.minute / 60, s.minute % 60),
        "world": { "open": world, "text": if world { tr("The boss's world is open: you can drive your duties.") } else { tr("The boss's world isn't open yet: Drive works once his game runs the company's map.") } },
        "reading": s.reading, "open": open, "mine": mine, "rest": rest, "players": players, "results": results,
        "history": history_rows(), "driving": driving, "notices": notices, "last_notice": j.notices.last().map(|n| n.n).unwrap_or(0),
        "take_note": if s.open_claims { tr("Take an open duty: it is yours at once.") } else { tr("The boss decides: ask for an open duty, and he says yes or no.") },
    })
}

/// The player's history in words: "Drove duty 3 of line 5 for Connexion".
fn history_rows() -> Vec<Value> {
    history()
        .iter()
        .take(40)
        .map(|e| {
            json!({
                "title": trf("Drove duty %{k} of line %{n} for %{company}", &[("k", &e.duty.to_string()), ("n", &e.number), ("company", &e.company_name)]),
                "text": format!("{}  ·  {}", day_label(&e.date), trf("%{trips} trips · score %{score} · +%{xp} XP", &[("trips", &e.trips.to_string()), ("score", &e.score.to_string()), ("xp", &e.xp.to_string())])),
            })
        })
        .collect()
}

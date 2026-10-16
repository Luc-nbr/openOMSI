//! A friend's side of "Play together": joined with the boss's code, his launcher keeps in touch
//! with the boss's on a thread of its own (every few seconds, whatever page is shown and while the
//! game runs: who he is and what he drives, and the trips his game reports, sent as they end),
//! and shows the boss's company in a menu of its own (`together.js`'s `JoinPage`): his role and
//! what it lets him do, his duties with Drive, the open duties to ask for, his figures for the
//! company, the players and the news - and, as his role allows, the requests to approve, the
//! duties to give, the players to manage and the company's own pages (relayed: `relay`). Drive
//! starts his game on the duty, alone, at its day and time: no world of the boss's is needed.
//! What he drove for a company is kept in his own history
//! (`~/.openomsi/together/<driver>-history.json`); his career counts the trips from his own trip
//! records, as every drive's.
//!
//! Beta 63: the session ends for him when the boss closes it (said to every message), when the
//! boss's launcher is lost (`client::Link`), or when he is sent away: his launcher stops asking -
//! and stops sending what his game drives -, and every page of the boss's company says so
//! (`tg_status`, `session.js`) with the way back to the menu. A duty the AI drives since he left it
//! is taken back from its next trip (`tg_take_back`).
//!
//! While his game drives a duty, the control room's messages to it (the snapshot's) go to his
//! game's plugin as the boss's launcher writes them to its own (`messages_link`), and what he
//! answers there or tells control goes back to the boss's (`msg_reply`, `msg_tell`).

use super::super::company::words::{day_label, eur, tr, trf};
use super::super::company::{self, TABS};
use super::{duty_arg, duty_words, notice_json, perm_json, perms_arg, relay, role_json, roles_json, unix_now};
use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::together::client::{self, Ended, Link};
use omsi_launcher_lib::company::together::code::{self, CompanyCode};
use omsi_launcher_lib::company::together::host as th;
use omsi_launcher_lib::company::together::wire::{DriveStart, DutyState, Notice, Request, SnapDuty, SnapResult, Snapshot};
use omsi_launcher_lib::company::together::{Perm, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use omsihub_plugin::bridge;
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
    /// The company's logo as the boss's launcher sent it (asked for once).
    logo: Option<String>,
    logo_asked: bool,
    /// How the session stands (when the boss's launcher last answered), and how it ended for
    /// him (closed, lost, sent away): no more asking then.
    link: Link,
    ended: Option<Ended>,
    /// The duty his game drove when the session ended: its results can no longer reach the boss.
    cut_off: Option<Drive>,
    /// How far his plugin's replies to the control room's messages were read.
    replies: bridge::ReplyReader,
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
    Request { v: code::VERSION, secret: j.code.secret_hex(), company: j.code.company, player: j.me.id.clone(), name: j.me.name.clone(), what: what.into(), driving: j.drive.as_ref().map(|d| d.duty.clone()), in_game: j.drive.is_some(), since: j.since, logo: !j.logo_asked, ..Default::default() }
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
        if !j.logo_asked {
            j.logo_asked = true;
            j.logo = s.card.logo.clone();
        }
        j.snap = Some(s);
    }
    // (the session ended, or another began: the code is no good now; closed or sent away: no
    // more asking)
    let error = v.get("error").and_then(Value::as_str).unwrap_or("");
    j.lost = [th::BAD_SECRET, th::NOT_OPEN_COMPANY, code::NEWER].contains(&error).then(|| error.to_string());
    if let Some(e) = j.link.answer(v, unix_now()) {
        end(j, e);
    }
    j.failures = 0;
}

/// The session ended for him (`e`): his launcher stops asking, and what his game drives is no
/// longer sent (said on his page).
fn end(j: &mut Joined, e: Ended) {
    if j.ended.is_none() {
        core::log_to_file(&format!("company session: ended for {} ({e:?})", j.me.name));
    }
    j.ended = Some(e);
    if let Some(d) = j.drive.take() {
        j.cut_off = Some(d);
    }
}

/// No answer of the boss's launcher (now): lost after a while (`client::HOST_LOST`).
fn unanswered(j: &mut Joined) {
    if let Some(e) = j.link.failed(unix_now()) {
        end(j, e);
    }
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
        let Some((code, base, mut req, drive)) = joined().as_mut().filter(|j| j.ended.is_none()).map(|j| {
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
                        unanswered(j);
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
                // (a report the boss's launcher could not take now is sent again; the session
                // closed: nothing is sent any more)
                let error = v.get("error").and_then(Value::as_str).unwrap_or("");
                if req.what == "report" && !th::passing(error) && j.ended.is_none() {
                    if let Some(d) = j.drive.as_mut() {
                        d.sent.extend(req.trips.iter().map(|t| (t.departure / 60.0).round() as i32));
                    }
                    if done {
                        if let Some(d) = j.drive.take() {
                            remember(j, &d);
                        }
                    }
                }
                drop(g);
                // (the control room's messages of the duty his game drives)
                if let Some(d) = drive.as_ref().filter(|_| !done) {
                    messages_link(&base, d);
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
                unanswered(j);
            }
        }
    }
}

/// The control room's messages to the duty his game drives (`d`), as the last snapshot has them:
/// to his plugin, in his launcher's language; what he answered there or told control, to the
/// boss's launcher (`base`).
fn messages_link(base: &str, d: &Drive) {
    let Some(dir) = super::super::addon::game_plugin_dir() else { return };
    let Some((list, company, req, mut reader)) = joined().as_ref().map(|j| (j.snap.as_ref().map(|s| s.messages.clone()).unwrap_or_default(), j.snap.as_ref().map(|s| s.card.name.clone()).unwrap_or_default(), request(j, "msg_reply"), j.replies.clone())) else { return };
    super::super::company::words::set_language(&super::super::addon::language());
    let inbox = company::messages::inbox_of(&company, list.iter(), &d.duty.line, &d.duty.tour, None);
    if let Err(e) = bridge::write_inbox(&dir, &inbox) {
        core::log_to_file(&format!("company session: the messages not written for the plugin: {e}"));
    }
    let new = reader.read_new(&dir);
    if let Some(j) = joined().as_mut() {
        j.replies = reader;
    }
    for r in new {
        let mut q = req.clone();
        if r.kind == "quick" {
            q.what = "msg_tell".into();
            q.text = r.key.clone();
        } else {
            q.msg = r.id;
            q.text = if r.kind == "reply" { r.key.clone() } else { r.kind.clone() };
        }
        if let Ok(v) = client::send(base, &q, client::TIMEOUT) {
            if let Some(j) = joined().as_mut() {
                take_answer(j, &v);
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
    let guest = args.get("guest").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(match cmd {
        "tg_join" => Ok(join(args.get("code").and_then(Value::as_str).unwrap_or(""))),
        "tg_check" => Ok(check(args.get("code").and_then(Value::as_str).unwrap_or(""))),
        "tg_view" => Ok(view()),
        "tg_take" | "tg_release" => Ok(duty_act(if cmd == "tg_take" { "take" } else { "release" }, args, |_| {})),
        "tg_drive" => drive(args, p, "start"),
        // (Beta 63: a duty the AI drives since he left it, back from its next trip)
        "tg_take_back" => drive(args, p, "take_back"),
        "tg_status" => Ok(status()),
        "tg_rejoin" => Ok(rejoin()),
        // (what his role lets him do for the company: checked again by the boss's launcher)
        "tg_assign" => Ok(duty_act("assign", args, |r| r.target = guest)),
        "tg_unassign" => Ok(duty_act("unassign", args, |_| {})),
        "tg_answer" => {
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            Ok(duty_act("answer", args, |r| r.yes = yes))
        }
        "tg_role" => {
            let role = args.get("role").and_then(Value::as_str).and_then(Role::of).unwrap_or_default();
            let perms = if args.get("perms").is_some() { perms_arg(args) } else { role.preset().to_vec() };
            Ok(answer_of(send_now("set_role", |r| {
                (r.target, r.role, r.perms) = (guest, role, perms);
            })))
        }
        "tg_kick" => {
            let ban = args.get("ban").and_then(Value::as_bool).unwrap_or(false);
            Ok(answer_of(send_now("kick", |r| (r.target, r.ban) = (guest, ban))))
        }
        // (a page of the company, run by the boss's launcher)
        "tg_relay" => relayed(args),
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

/// A refusal of the boss's launcher in the page's words (with the permission it lacked).
fn refusal_words(v: &Value) -> String {
    let error = tr(v.get("error").and_then(Value::as_str).unwrap_or(client::UNREACHED));
    match v.get("perm").and_then(Value::as_str).and_then(Perm::of) {
        Some(p) => format!("{error} ({})", tr(p.label())),
        None => error,
    }
}

/// What an action's answer says to the page: nothing, or why not.
fn answer_of(r: Result<Value, String>) -> Value {
    match r {
        Ok(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => json!({}),
        Ok(v) => json!({ "error": refusal_words(&v) }),
        Err(e) => json!({ "error": e }),
    }
}

/// A command of the company's pages (`relay::RELAYED`) or of RosterFlow (`relay::relayed_rf`),
/// relayed to the boss's launcher: its answer, or why not (the page says it as any command's
/// error). RosterFlow's print comes back as the file, saved here.
fn relayed(args: &Value) -> Answer {
    let cmd = args.get("cmd").and_then(Value::as_str).unwrap_or("").to_string();
    if !relay::RELAYED.contains(&cmd.as_str()) && !relay::relayed_rf(&cmd) {
        return Err(tr(relay::FROM_AFAR));
    }
    let mut inner = args.get("args").cloned().unwrap_or(json!({}));
    if inner.get("lang").is_none() {
        inner["lang"] = args.get("lang").cloned().unwrap_or(Value::Null);
    }
    let v = send_page(json!({ "cmd": cmd, "args": inner }))?;
    if v.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(refusal_words(&v));
    }
    let answer = v.get("answer").cloned().unwrap_or(Value::Null);
    if cmd == "rf_print" {
        if let Some(pdf) = answer.get("pdf").and_then(Value::as_str) {
            let bytes = super::super::bus3d::photo::base64(pdf)?;
            let name = answer.get("name").and_then(Value::as_str).unwrap_or("graph.pdf");
            return Ok(super::super::rosterflow::save_pdf(name, &bytes));
        }
    }
    Ok(answer)
}

/// A page's command sent to the boss's launcher now (its answer carries no day: the thread keeps
/// that).
fn send_page(page: Value) -> Result<Value, String> {
    let (base, mut req) = {
        let g = joined();
        let j = g.as_ref().ok_or_else(|| tr("Not in a company session."))?;
        let base = j.base.clone().ok_or_else(|| tr("Looking for the boss's launcher…"))?;
        (base, request(j, "page"))
    };
    req.page = page;
    req.logo = false;
    client::send(&base, &req, client::TIMEOUT).map_err(|_| tr(client::UNREACHED))
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
    *joined() = Some(Joined { text: text.trim().to_string(), code, me: me(), base: None, snap: None, since: 0, notices: Vec::new(), lost: None, failures: 0, drive: None, stop: stop.clone(), wake: true, said: None, logo: None, logo_asked: false, link: Link::new(unix_now()), ended: None, cut_off: None, replies: Default::default() });
    let _ = std::thread::Builder::new().name("company session".into()).spawn(move || run(stop));
    view()
}

/// Join the same session again, with the code it was joined with (the boss's launcher was lost).
fn rejoin() -> Value {
    let text = joined().as_ref().map(|j| j.text.clone());
    match text {
        Some(t) => join(&t),
        None => view(),
    }
}

/// Leave the session (the boss's launcher told, best effort; not when it ended): nothing of it is
/// kept - its day, its pages, its notices go with it.
fn leave() {
    let gone = joined().take();
    let Some(j) = gone else { return };
    j.stop.store(true, Ordering::Relaxed);
    if let Some(base) = j.base.clone().filter(|_| j.ended.is_none()) {
        let req = request(&j, "leave");
        let _ = std::thread::Builder::new().name("company leave".into()).spawn(move || {
            let _ = client::send(&base, &req, Duration::from_secs(4));
        });
    }
}

/// Something done with a duty (taken, given back, given to a player, approved...).
fn duty_act(what: &str, args: &Value, fill: impl FnOnce(&mut Request)) -> Value {
    let Some(r) = duty_arg(args) else { return json!({}) };
    answer_of(send_now(what, |q| {
        q.duty = Some(r);
        fill(q);
    }))
}

/// Drive a duty: its start asked of the boss's launcher (`what`: `start`, or `take_back` from the
/// AI at its next trip), the game started on it - alone, at the duty's day and time.
fn drive(args: &Value, p: &dyn Platform, what: &str) -> Answer {
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
    let v = match send_now(what, |q| q.duty = Some(r.clone())) {
        Ok(v) => v,
        Err(e) => return Ok(json!({ "error": e })),
    };
    let Some(start) = v.get("start").and_then(|x| serde_json::from_value::<DriveStart>(x.clone()).ok()) else {
        return Ok(json!({ "error": refusal_words(&v) }));
    };
    let base: core::Duty = serde_json::from_value(args.get("base").cloned().unwrap_or(Value::Null)).map_err(|e| format!("base: {e}"))?;
    let name = driver();
    let mut d = start.duty(base);
    d.display_font = core::busfonts::BusFonts::load().font_for(&start.bus);
    d.set_vars = super::super::hub_more::setvars(&start.bus, &start.paint);
    core::log_to_file(&format!("company session: {} drives {} {} duty {} for the boss's company", name, start.line, start.tour, r.duty + 1));
    let launched = core::launch(&d).map_err(|e| format!("{e:#}"))?;
    core::log_to_file(&format!("game {}: {}", launched.pid, launched.command));
    p.launched();
    let number = joined().as_ref().and_then(|j| j.snap.as_ref()).and_then(|s| s.duties.iter().find(|x| omsi_launcher_lib::company::together::book::same_duty(&x.duty, &r)).map(|x| x.number.clone())).unwrap_or_default();
    // (his plugin's earlier replies are not this duty's: read from here on)
    let replies = super::super::addon::game_plugin_dir().map(|d| std::fs::metadata(d.join(bridge::DATA_DIR).join(bridge::REPLIES_FILE)).map_or(0, |m| m.len())).unwrap_or(0);
    if let Some(j) = joined().as_mut() {
        j.drive = Some(Drive { duty: r, number, tour: start.tour.clone(), since: unix_now().saturating_sub(60), pid: launched.pid, sent: Vec::new() });
        j.replies = bridge::ReplyReader { offset: replies };
        j.wake = true;
    }
    Ok(json!({ "launched": serde_json::to_value(launched).unwrap_or(Value::Null) }))
}

/// A duty's line in the friend's view: what it is, who drives it, what he can do with it (`ask`:
/// he may drive and ask for duties).
fn duty_row(d: &SnapDuty, open_claims: bool, ask: bool) -> Value {
    let hhmm = |m: i32| format!("{:02}:{:02}", (m / 60).rem_euclid(24), m.rem_euclid(60));
    let mut facts = vec![trf("%{n} trips", &[("n", &d.trips.to_string())]), format!("{} km", d.km)];
    if !d.bus.is_empty() {
        facts.push(trf("Bus %{bus}", &[("bus", &tr(&d.bus))]));
    }
    let take = if open_claims { tr("Take this duty") } else { tr("Ask for this duty") };
    let drive = json!({ "label": tr("Drive"), "act": "drive", "kind": "primary", "icon": "play_arrow" });
    let (state, tone, acts): (String, &str, Vec<Value>) = match d.state {
        DutyState::Open if ask => (tr("Open"), "accent", vec![json!({ "label": take, "act": "take", "kind": "primary" })]),
        DutyState::Open => (tr("Open"), "accent", Vec::new()),
        DutyState::Mine => (tr("Yours"), "ok", vec![drive, json!({ "label": tr("Give back"), "act": "release", "kind": "" })]),
        DutyState::Late => (tr("Yours: the company's day closed, drive it when you can"), "warn", vec![drive, json!({ "label": tr("Give back"), "act": "release", "kind": "" })]),
        DutyState::Asked => (tr("Asked for: waiting for a yes"), "warn", vec![json!({ "label": tr("Withdraw"), "act": "release", "kind": "" })]),
        DutyState::Driving => (tr("You are driving it"), "ok", vec![json!({ "label": tr("Drive again"), "act": "drive", "kind": "", "icon": "replay" })]),
        DutyState::Done => (tr("Driven"), "dim", Vec::new()),
        DutyState::Ai if d.back => (tr("You left it: the AI drives it"), "warn", vec![json!({ "label": tr("Take the duty back from the next trip"), "act": "take_back", "kind": "primary", "icon": "play_arrow" })]),
        DutyState::Ai => (tr("You left it: the AI drives the rest of it"), "warn", Vec::new()),
        DutyState::Player if d.ai => (super::ai_label(&d.who), "soft", Vec::new()),
        DutyState::Player => (trf("%{name} has it", &[("name", &d.who)]), "soft", Vec::new()),
        DutyState::Company => (trf("Driven by %{name}", &[("name", &tr(&d.who))]), "dim", Vec::new()),
        DutyState::Closed => (tr(&d.why), "dim", Vec::new()),
    };
    json!({
        "duty": d.duty, "number": d.number, "colour": d.colour, "times": format!("{} – {}", hhmm(d.from), hhmm(d.to)),
        "from": d.from, "date": day_label(&d.duty.date),
        "route": format!("{} → {}", d.first, d.last), "facts": facts.join("  ·  "), "title": duty_words(&d.number, &d.duty),
        "state": state, "tone": tone, "acts": acts, "key": format!("{:?}", d.state).to_lowercase(), "guest": d.guest, "who": d.who,
    })
}

/// The friend's menu: not joined (the code field), looking for the boss, sent away, or the
/// company as his role sees it.
fn view() -> Value {
    let words = json!({
        "title": tr("Join a company"), "text": tr("A friend runs a bus company. Paste the session code they sent you: you get a role in their company and drive its duties in your own game, at your own pace."),
        "field": tr("Session code (HUB-…)"), "join": tr("Join"), "back": tr("Back"),
        "need": [tr("Both need the same version of Omsi-Hub."), tr("The same network, the same VPN (Hamachi, Radmin VPN, ZeroTier, Tailscale), or the boss's tunnel."), tr("The boss's company map, and the buses of your duties, installed.")],
        "leave": tr("Leave"), "connecting": tr("Looking for the boss's launcher…"),
        "tab_work": tr("My work"), "tab_duties": tr("Duties"), "tab_team": tr("Team"), "tab_news": tr("News"),
        "open_caps": tr("Open duties").to_uppercase(), "no_open": tr("No open duty right now: the boss can give you one."),
        "mine_caps": tr("My duties").to_uppercase(), "no_mine": tr("No duty of yours yet: ask for an open one."),
        "rest_caps": tr("The rest of the day").to_uppercase(),
        "next_caps": tr("Next up").to_uppercase(), "nothing_next": tr("Nothing planned for you. Ask for an open duty, or wait for the boss to give you one."),
        "results_caps": tr("My results").to_uppercase(), "history_caps": tr("My history").to_uppercase(),
        "players_caps": tr("Players").to_uppercase(), "notices_caps": tr("Company news").to_uppercase(),
        "stats_caps": tr("For the company").to_uppercase(),
        "trips": tr("Trips"), "punctuality": tr("On time"), "score": tr("Score"), "fares": tr("Fares"), "fines": tr("Fines"), "duties": tr("Duties"),
        "role_caps": tr("Your role").to_uppercase(), "can_caps": tr("What you may do").to_uppercase(),
        "asks_caps": tr("Duty requests").to_uppercase(), "no_asks": tr("No duty requests."), "yes": tr("Approve"), "no": tr("Decline"),
        "give_caps": tr("Give duties").to_uppercase(), "give": tr("Give"), "off": tr("Take off"), "pick_player": tr("Choose a player"),
        "members_caps": tr("Manage players").to_uppercase(), "kick": tr("Send away"), "ban": tr("Remove"),
        "pages": tr("Company pages"), "pages_tip": tr("Open the company's pages your role allows"),
        "say": tr("Say something to everyone"), "send": tr("Send"),
        "online": tr("Online"), "offline": tr("Offline"), "connected": tr("Connected"), "reconnecting": tr("Reconnecting…"),
        "ended_title": tr("The session ended for you"), "ended_text": tr("Ask the boss for a new code to join again."),
        "role_changed": tr("Your role is now %{role}."),
    });
    let g = joined();
    let Some(j) = g.as_ref() else {
        return json!({ "joined": false, "words": words, "history": history_rows() });
    };
    if let Some(e) = j.ended {
        return json!({ "joined": true, "ended": ended_json(j, e), "words": words, "code": j.text });
    }
    let lost = j.lost.as_deref().map(tr);
    let said = j.said.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 12).map(|(e, _)| tr(e));
    let Some(s) = j.snap.as_ref() else {
        return json!({ "joined": true, "connecting": true, "words": words, "code": j.text, "lost": lost });
    };
    let you = s.you.clone().unwrap_or_default();
    let can = |p: Perm| you.perms.contains(&p);
    let ask = can(Perm::Drive);
    let rows = |pick: &dyn Fn(&SnapDuty) -> bool| -> Vec<Value> { s.duties.iter().filter(|d| pick(d)).map(|d| duty_row(d, s.open_claims || can(Perm::Approve), ask)).collect() };
    let mine = rows(&|d| matches!(d.state, DutyState::Mine | DutyState::Asked | DutyState::Driving | DutyState::Done | DutyState::Late | DutyState::Ai));
    let open = rows(&|d| d.state == DutyState::Open);
    let rest = rows(&|d| matches!(d.state, DutyState::Player | DutyState::Company | DutyState::Closed));
    // (what comes next: the duty being driven, else the first of his still to drive)
    let next = s
        .duties
        .iter()
        .filter(|d| matches!(d.state, DutyState::Driving | DutyState::Mine | DutyState::Late) || (d.state == DutyState::Ai && d.back))
        .min_by_key(|d| (d.state != DutyState::Driving, d.state != DutyState::Late, d.from))
        .map(|d| duty_row(d, s.open_claims, ask));
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
            let role = if p.boss { json!({ "label": tr("Owner"), "icon": "star" }) } else { role_json(p.role) };
            json!({ "name": p.name, "me": p.me, "boss": p.boss, "online": p.online, "text": text, "role": role, "driving": p.driving.is_some(), "initial": p.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default() })
        })
        .collect();
    let results: Vec<Value> = s
        .results
        .iter()
        .map(|r| {
            let on_time = if r.stops > 0 { (100 * r.on_time / r.stops).to_string() + " %" } else { "-".into() };
            let text = trf("%{trips} trips · %{passengers} passengers · %{km} km · on time %{p} · score %{score} · +%{xp} XP", &[("trips", &r.trips.to_string()), ("passengers", &r.passengers.to_string()), ("km", &format!("{:.0}", r.km)), ("p", &on_time), ("score", &r.score.to_string()), ("xp", &r.xp.to_string())]);
            // (Beta 63: the rest of it the AI drove, since he left it)
            let ai = (r.ai_trips > 0).then(|| format!("  ·  {}", trf("The AI drove the rest of it: %{n} trips", &[("n", &r.ai_trips.to_string())]))).unwrap_or_default();
            json!({ "title": duty_words(&r.number, &r.duty), "text": format!("{text}  ·  {}{ai}", trf("for the company %{amount}", &[("amount", &eur(r.fares))])), "done": r.done, "ai": r.ai_trips > 0 })
        })
        .collect();
    let driving = j.drive.as_ref().map(|d| trf("Your game drives %{duty}: its trips go to the company as they end.", &[("duty", &duty_words(&d.number, &d.duty))]));
    let notices: Vec<Value> = j.notices.iter().rev().take(40).map(notice_json).collect();
    // (what his role lets him manage)
    let asks: Vec<Value> = s
        .asks
        .iter()
        .map(|a| json!({ "duty": a.duty, "name": a.name, "no": a.no, "what": duty_words(&a.number, &a.duty), "times": format!("{} – {}", omsi_launcher_lib::company::clock::hhmm(a.from as i64), omsi_launcher_lib::company::clock::hhmm(a.to as i64)) }))
        .collect();
    let members: Vec<Value> = s
        .members
        .iter()
        .filter(|m| m.no != you.no)
        .map(|m| json!({ "no": m.no, "name": m.name, "role": role_json(m.role), "perms": m.perms.iter().map(|p| p.key()).collect::<Vec<_>>(), "online": m.online, "banned": m.banned, "duties": m.duties, "stats": super::host::stats_json(&m.stats) }))
        .collect();
    let giveable: Vec<Value> = if can(Perm::Assign) { s.duties.iter().filter(|d| matches!(d.state, DutyState::Open | DutyState::Company | DutyState::Player)).map(|d| duty_row(d, s.open_claims, false)).collect() } else { Vec::new() };
    // (the company's own pages his role lets him see, besides the map)
    let pages = (0..TABS.len()).any(|t| t != company::MAP_TAB && relay::tab_need(t).met(&you.perms));
    json!({
        "joined": true, "words": words, "code": j.text, "lost": lost, "said": said,
        "mark": { "short": s.card.short, "colours": s.card.colours, "logo": j.logo },
        "name": s.card.name, "boss": s.card.boss, "map": s.card.map_name,
        "sub": trf("Boss: %{name}  ·  %{map}", &[("name", &s.card.boss), ("map", &s.card.map_name)]),
        "day": format!("{}  ·  {:02}:{:02}", day_label(&s.date), s.minute / 60, s.minute % 60),
        "me": { "name": you.name, "role": role_json(you.role), "role_key": you.role.key(), "stats": super::host::stats_json(&you.stats),
                "perms": Perm::ALL.iter().map(|p| { let mut v = perm_json(*p); v["on"] = json!(can(*p)); v }).collect::<Vec<_>>() },
        "can": { "drive": ask, "assign": can(Perm::Assign), "approve": can(Perm::Approve), "players": can(Perm::Players), "pages": pages },
        "reading": s.reading, "next": next, "open": open, "mine": mine, "rest": rest, "players": players, "results": results,
        "asks": asks, "members": members, "giveable": giveable, "roles": if can(Perm::Players) { roles_json() } else { Value::Null },
        "history": history_rows(), "driving": driving, "notices": notices, "last_notice": j.notices.last().map(|n| n.n).unwrap_or(0),
        "online": s.players.iter().filter(|p| p.online).count(),
        "take_note": if !ask { tr("Your role does not drive: the boss can change it.") } else if s.open_claims || can(Perm::Approve) { tr("Take an open duty: it is yours at once.") } else { tr("Ask for an open duty: the boss, or a player who may, says yes or no.") },
    })
}

/// How the session ended for him, as every page of it says (`session.js`): the boss closed it,
/// his launcher was lost, or he was sent away - and the duty his game drives, whose results no
/// longer reach the boss.
fn ended_json(j: &Joined, e: Ended) -> Value {
    let company = if j.link.company.is_empty() { j.snap.as_ref().map(|s| s.card.name.clone()).unwrap_or_default() } else { j.link.company.clone() };
    let (title, text, icon) = match e {
        Ended::Closed => (trf("%{company} - the host closed this session", &[("company", &company)]), tr("The host stopped sharing the company, closed it or quit Omsi-Hub."), "power_settings_new"),
        Ended::Lost => (tr("Lost the connection to the host"), tr("The host's launcher has not answered for a while: it may have closed, or the network dropped."), "wifi_off"),
        Ended::Removed => (tr("The host removed you from this session"), tr("Ask the host if you want to join again."), "person_remove"),
    };
    let driving = j.cut_off.as_ref().map(|d| trf("Your game still drives %{duty}, but its results can no longer reach the host.", &[("duty", &duty_words(&d.number, &d.duty))]));
    json!({ "kind": e, "title": title, "text": text, "icon": icon, "driving": driving, "retry": e == Ended::Lost, "back": tr("Back to the menu"), "again": tr("Try again"), "company": company })
}

/// Whether he is in a session, and how it ended (every page of the boss's company asks:
/// `session.js`).
fn status() -> Value {
    let g = joined();
    let Some(j) = g.as_ref() else { return json!({ "joined": false }) };
    json!({ "joined": true, "ended": j.ended.map(|e| ended_json(j, e)) })
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

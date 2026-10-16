//! The boss's side of "Play together": the session of his open company - its company server
//! (`together::server`, answered on the company itself under the session's lock, so it answers
//! whatever page the launcher shows and while the game runs), its code, the players online with
//! their roles and permissions and what each drives, the duties asked for, the activity - and the
//! company's pages relayed for the players whose roles allow them (`relay`). The boss opens no
//! world: each player drives his duties in his own game. The company's "Multiplayer" page
//! (`multiplayer.js`) is this part's.

use super::super::company::kit::Popup;
use super::super::company::words::{self, eur, num, tr, trf};
use super::super::company::{self, Session, Viewer};
use super::relay;
use super::{duty_arg, duty_words, notice_json, notice_text, perms_arg, role_json, roles_json, unix_now};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::plan::DayPlan;
use omsi_launcher_lib::company::together::book::{self, ClaimState};
use omsi_launcher_lib::company::together::code::{self, CompanyCode};
use omsi_launcher_lib::company::together::host::{self as th, Here, HostState};
use omsi_launcher_lib::company::together::server::{Server, TunnelState};
use omsi_launcher_lib::company::together::wire::{DutyState, Request};
use omsi_launcher_lib::company::together::{Perm, Role};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// The session running: for which company, its code, the server, the session's state.
struct Hosting {
    company: String,
    /// The code as the boss copies it.
    text: String,
    server: Server,
    st: HostState,
    /// The company's day as last made (the session's plan generation and the company's date).
    plan: Option<(u64, u64, String, DayPlan)>,
    /// The last notice told in the company's corner.
    told: u64,
    /// The pages' own state of each player looking at the company's pages (`company::Viewer`).
    viewers: HashMap<u32, Viewer>,
}

static HOSTING: Mutex<Option<Hosting>> = Mutex::new(None);
/// The boss's page's language: the server's threads tell in it.
static LANG: Mutex<String> = Mutex::new(String::new());

fn hosting() -> MutexGuard<'static, Option<Hosting>> {
    HOSTING.lock().unwrap_or_else(|e| e.into_inner())
}

/// No session is hosted (the gate's test).
#[cfg(test)]
pub(super) fn hosting_none() -> bool {
    hosting().is_none()
}

/// The code of the session hosted now, if any (Omsi-Hub online tells it to the friends and
/// sends it with an invite: `online.rs`).
pub fn session_code() -> Option<String> {
    hosting().as_ref().map(|h| h.text.clone())
}

/// The page's language kept for the server's threads.
pub(super) fn remember_language() {
    *LANG.lock().unwrap_or_else(|e| e.into_inner()) = words::language();
}

fn lead() -> i64 {
    let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
    core::bus_lead(settings.get("bus_lead").and_then(|x| x.as_i64()))
}

/// The company's day as the session answers with it (made again when the company changed).
fn plan_of(s: &Session, h: &mut Hosting) -> Option<DayPlan> {
    let c = s.company.as_ref()?;
    let lines = s.map_lines()?;
    if let Some((g, gen, date, p)) = &h.plan {
        if *g == s.plan_gen && *gen == s.generation && *date == c.date {
            return Some(p.clone());
        }
    }
    let tours = co::network::tours_of_day(c, lines, &c.date);
    let p = co::plan::day_plan(c, &c.date, tours, &[], &[], false);
    h.plan = Some((s.plan_gen, s.generation, c.date.clone(), p.clone()));
    Some(p)
}

/// The day's timetable as the session has it (empty: not read yet).
fn lines_of(s: &Session) -> Vec<core::LineInfo> {
    s.map_lines().map(<[core::LineInfo]>::to_vec).unwrap_or_default()
}

/// The notices the boss is told in the company's corner (the rest are on his Multiplayer page).
const TOLD: [&str; 5] = [th::JOINED, th::LEFT, th::ASKS, th::FINISHED, th::SAYS];

/// Tell the boss what came since he was last told (in his page's language).
fn tell_boss(s: &mut Session, h: &mut Hosting) {
    let new: Vec<String> = h.st.notices.iter().filter(|n| n.n > h.told && TOLD.contains(&n.text.as_str())).map(notice_text).collect();
    h.told = h.st.last_notice();
    for t in new {
        if s.notices.len() < 20 {
            s.say(t, false);
        }
    }
}

/// A message of a friend's launcher (the server's thread): answered on the company.
fn handle(body: &str) -> String {
    company::with(|s| {
        let lang = LANG.lock().map(|l| l.clone()).unwrap_or_default();
        words::set_language(&lang);
        let mut g = hosting();
        let Some(h) = g.as_mut() else { return th::refusal(th::BAD_SECRET).to_string() };
        let req = match h.st.admits(body) {
            Ok(r) => r,
            Err(v) => return v.to_string(),
        };
        if s.company.as_ref().map(|c| c.id.as_str()) != Some(h.company.as_str()) {
            return th::refusal(th::NOT_OPEN_COMPANY).to_string();
        }
        // (the company's day simulated on a thread, which hands its own copy back: nothing is
        // changed meanwhile - the friend's launcher asks again)
        if s.closing {
            return th::refusal(th::BUSY).to_string();
        }
        if req.what == "page" {
            drop(g);
            return relay_page(s, &req).to_string();
        }
        let plan = plan_of(s, h);
        let lines = lines_of(s);
        let (now, lead) = (unix_now(), lead());
        let answered = {
            let Some(c) = s.company.as_mut() else { return th::refusal(th::NOT_OPEN_COMPANY).to_string() };
            let here = Here { plan: plan.as_ref(), lines: &lines, now, lead };
            let mut a = h.st.answer(c, &here, body);
            // (the company's logo, once the player's page asks for it)
            if req.logo {
                if let Some(card) = a.body.get_mut("snapshot").and_then(|x| x.get_mut("card")) {
                    card["logo"] = json!(c.logo.as_deref().and_then(company::logo_picture));
                }
            }
            a
        };
        if answered.changed {
            company::changed(s);
        }
        tell_boss(s, h);
        answered.body.to_string()
    })
}

/// A page of the company relayed by a player's launcher (`relay`): run for him when his role
/// allows it, with the pages' state of his own. The session is held, the hosting is not (the
/// pages may ask it: the Multiplayer tile).
fn relay_page(s: &mut Session, req: &Request) -> Value {
    let now = unix_now();
    let member = {
        let mut g = hosting();
        let Some(h) = g.as_mut() else { return th::refusal(th::BAD_SECRET) };
        let Some(c) = s.company.as_mut() else { return th::refusal(th::NOT_OPEN_COMPANY) };
        h.st.member(c, req, now).map(|(guest, perms)| (guest, perms, h.viewers.remove(&guest).unwrap_or_default()))
    };
    let (guest, perms, mut viewer) = match member {
        Ok(x) => x,
        Err(v) => return v,
    };
    let cmd = req.page.get("cmd").and_then(Value::as_str).unwrap_or("");
    let args = req.page.get("args").cloned().unwrap_or(json!({}));
    viewer.guest = guest;
    let out = page_for(s, &mut viewer, &perms, cmd, &args);
    if let Some(h) = hosting().as_mut() {
        h.viewers.insert(guest, viewer);
    }
    out
}

/// A page's command of a player (`viewer`'s, with `perms`) run on the company when they allow
/// it: its answer, or the refusal. A change of the planning made on its own pages is told to the
/// planners in RosterFlow (theirs is told by RosterFlow itself).
pub(super) fn page_for(s: &mut Session, viewer: &mut Viewer, perms: &[Perm], cmd: &str, args: &Value) -> Value {
    let need = relay::need(cmd, args);
    if !need.met(perms) {
        return need.refusal();
    }
    let gen = s.generation;
    let out = match company::relayed(s, viewer, cmd, args, perms) {
        Ok(v) => json!({ "ok": true, "answer": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    let act = args.get("act").and_then(Value::as_str).unwrap_or("");
    if cmd == "co_more_act" && (act.starts_with("plan_") || act.starts_with("pe_")) && s.generation != gen {
        super::super::rosterflow::planning_changed(s, viewer.guest);
    }
    out
}

/// The players online now in the open company's session: (number, name, what he drives).
pub fn online_guests(c: &Company) -> Vec<(u32, String, Option<DutyRef>)> {
    let g = hosting();
    let Some(h) = g.as_ref().filter(|h| h.company == c.id) else { return Vec::new() };
    h.st.online_now(unix_now()).map(|o| (o.guest, o.name.clone(), o.driving.clone())).collect()
}

/// The Multiplayer page's badge in the strip: the duties asked for, else the players online.
pub fn rail_badge(c: &Company) -> (Value, String) {
    let g = hosting();
    let tip = tr("Multiplayer");
    let Some(h) = g.as_ref().filter(|h| h.company == c.id) else { return (Value::Null, format!("{tip}\n{}", tr("Not shared: start sharing to let friends join"))) };
    let online = h.st.online_now(unix_now()).count();
    let asked = c.together.claims.iter().filter(|x| x.state == ClaimState::Asked && x.duty.date == c.date).count();
    let badge = if asked > 0 {
        json!({ "n": asked.to_string(), "tone": "warn" })
    } else if online > 0 {
        json!({ "n": online.to_string(), "tone": "ok" })
    } else {
        Value::Null
    };
    (badge, format!("{tip}\n{}", trf("Shared · %{n} online", &[("n", &online.to_string())])))
}

/// The Multiplayer tile on the start screen: whether the company is shared, who is online.
pub fn tile_line(c: &Company) -> (String, &'static str) {
    let g = hosting();
    match g.as_ref().filter(|h| h.company == c.id) {
        Some(h) => (trf("Shared · %{n} online", &[("n", &h.st.online_now(unix_now()).count().to_string())]), "ok"),
        None if c.together.guests.is_empty() => (tr("Not shared"), "soft"),
        None => (trf("Not shared · %{n} players", &[("n", &c.together.guests.len().to_string())]), "soft"),
    }
}

/// Give duty `k` of tour `ti` of the day `p` to player `guest` (RosterFlow's "Who drives it").
pub fn give_to_guest(s: &mut Session, p: &DayPlan, ti: usize, k: usize, guest: u32) -> Value {
    let Some(r) = DutyRef::of(p, ti, k) else { return json!({}) };
    give(s, p, &r, guest)
}

/// Give duty `r` of the day `p` to player `guest`, told to everyone.
fn give(s: &mut Session, p: &DayPlan, r: &DutyRef, guest: u32) -> Value {
    let now = s.company.as_ref().map(co::clock::now).unwrap_or(0);
    let (r2, p2) = (r.clone(), p.clone());
    match company::act(s, |c| book::give(c, &p2, &r2, guest, now)) {
        Ok(()) => {
            let name = s.company.as_ref().map(|c| book::guest_name(c, guest)).unwrap_or_default();
            if let Some(h) = hosting().as_mut() {
                h.st.tell_duty(unix_now(), th::GAVE, &name, Some(p), r);
            }
            prepare(s);
            json!({})
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// How the players' duties start, worked out for those that became theirs (`HostState::prepare_starts`).
fn prepare(s: &mut Session) {
    let mut g = hosting();
    let Some(h) = g.as_mut() else { return };
    let plan = plan_of(s, h);
    let lines = lines_of(s);
    let Some(c) = s.company.as_mut() else { return };
    let before = c.together.clone();
    let here = Here { plan: plan.as_ref(), lines: &lines, now: unix_now(), lead: lead() };
    h.st.prepare_starts(c, &here);
    let changed = c.together != before;
    drop(g);
    if changed {
        company::changed(s);
    }
}

/// A number argument.
fn u32_arg(args: &Value, name: &str) -> u32 {
    args.get(name).and_then(Value::as_u64).unwrap_or(0) as u32
}

/// The commands of the boss's Multiplayer page.
pub(super) fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "tg_host_view" => company::with(|s| Ok(view(s))),
        "tg_host_badge" => company::with(|s| Ok(badge(s))),
        "tg_host_start" => company::with(start),
        "tg_host_stop" => company::with(|s| {
            // (the players' duties not under way go back to the company: nobody can be told now)
            let mine = s.company.as_ref().is_some_and(|c| hosting().as_ref().is_some_and(|h| h.company == c.id));
            stop(s);
            if mine {
                let _ = company::act(s, |c| {
                    c.together.claims.retain(|x| matches!(x.state, ClaimState::Driving | ClaimState::Done) || x.late);
                    Ok(())
                });
            }
            Ok(json!({}))
        }),
        "tg_host_open" => company::with(|s| {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            let r = company::act(s, |c| {
                c.together.open_claims = on;
                Ok(())
            });
            Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
        }),
        "tg_host_answer" => company::with(|s| Ok(answer(s, args))),
        "tg_host_off" => company::with(|s| Ok(take_off(s, args))),
        "tg_host_give" => company::with(|s| {
            let Some(r) = duty_arg(args) else { return Ok(json!({})) };
            let plan = {
                let mut g = hosting();
                g.as_mut().and_then(|h| plan_of(s, h))
            };
            Ok(match plan {
                Some(p) => give(s, &p, &r, u32_arg(args, "guest")),
                None => json!({ "error": tr(th::READING) }),
            })
        }),
        "tg_host_role" => company::with(|s| {
            let role = args.get("role").and_then(Value::as_str).and_then(Role::of).unwrap_or_default();
            // (no ticks sent: the role's own)
            let perms = if args.get("perms").is_some() { perms_arg(args) } else { role.preset().to_vec() };
            Ok(with_players(s, |st, c| {
                st.set_role(c, unix_now(), u32_arg(args, "guest"), role, &perms);
            }))
        }),
        "tg_host_kick" => company::with(|s| {
            let ban = args.get("ban").and_then(Value::as_bool).unwrap_or(false);
            Ok(with_players(s, |st, c| st.send_away(c, unix_now(), u32_arg(args, "guest"), ban)))
        }),
        "tg_host_back" => company::with(|s| Ok(with_players(s, |st, c| st.let_back(c, unix_now(), u32_arg(args, "guest"))))),
        "tg_host_say" => company::with(|s| {
            let text = book::clean(args.get("text").and_then(Value::as_str).unwrap_or(""), 160);
            let boss = s.company.as_ref().map(|c| c.profile.clone()).unwrap_or_default();
            if let Some(h) = hosting().as_mut().filter(|_| !text.is_empty()) {
                h.st.tell(unix_now(), th::SAYS, vec![("name".into(), boss), ("text".into(), text)]);
            }
            Ok(json!({}))
        }),
        _ => return None,
    })
}

/// Something the boss does to his players (their roles, sending one away): on the company, with
/// the session's state (a session must run for the players to be told; their roles are kept
/// with the company either way).
fn with_players(s: &mut Session, f: impl FnOnce(&mut HostState, &mut Company)) -> Value {
    let Some(id) = s.company.as_ref().map(|c| c.id.clone()) else { return json!({}) };
    let mut g = hosting();
    let mut spare = HostState::new(&id, "", [0; 8]);
    let st = match g.as_mut().filter(|h| h.company == id) {
        Some(h) => &mut h.st,
        None => &mut spare,
    };
    let Some(c) = s.company.as_mut() else { return json!({}) };
    let before = c.together.clone();
    f(st, c);
    let changed = c.together != before;
    drop(g);
    if changed {
        company::changed(s);
    }
    json!({})
}

/// Start the session of the open company: its server, its code, the tunnel.
fn start(s: &mut Session) -> Answer {
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    if hosting().as_ref().is_some_and(|h| h.company == c.id) {
        return Ok(json!({}));
    }
    stop(s);
    let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
    let key = Arc::new(omsi_net::hostkey::HostKey::generate().ok_or("no key for the session")?);
    let secret: [u8; 8] = omsi_net::http::random_bytes();
    let server = match Server::start(0, false, Arc::new(|body: &str| handle(body))) {
        Ok(x) => x,
        Err(e) => {
            let text = trf("No port for the session: %{e}", &[("e", &e.to_string())]);
            return Ok(json!({ "popup": company::popup(s, Popup::new("error", "The session could not start", text, tr("Close what uses the port, or start the launcher again."), None)) }));
        }
    };
    let ips: Vec<std::net::Ipv4Addr> = omsi_net::code_ipv4s().into_iter().filter(|ip| !ip.is_loopback()).collect();
    let code = CompanyCode { secret, session: key.session(), version: code::VERSION, port: server.port(), company: code::tag_of(&c.id), ips };
    server.open_tunnel(key.clone());
    let text = code.encode();
    core::log_to_file(&format!("company: Play together for {} on port {}", c.id, server.port()));
    *hosting() = Some(Hosting { company: c.id.clone(), text, server, st: HostState::new(&c.id, &c.profile, secret), plan: None, told: 0, viewers: HashMap::new() });
    Ok(json!({}))
}

/// End the session (the server and the tunnel go; what the players drove stays with the company).
fn stop(_s: &mut Session) {
    let gone = hosting().take();
    if let Some(h) = gone {
        core::log_to_file(&format!("company: Play together for {} ended", h.company));
        drop(h);
    }
}

/// The figures on the bar's button: players online, duties asked for.
fn badge(s: &mut Session) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({ "on": false }) };
    let g = hosting();
    let Some(h) = g.as_ref().filter(|h| h.company == c.id) else { return json!({ "on": false, "tip": tr("Multiplayer: share your company with friends") }) };
    let online = h.st.online_now(unix_now()).count();
    let asked = c.together.claims.iter().filter(|x| x.state == ClaimState::Asked && x.duty.date == c.date).count();
    json!({ "on": true, "online": online, "asked": asked, "tip": trf("Shared · %{n} online", &[("n", &online.to_string())]) })
}

/// Yes or no to a duty asked for.
fn answer(s: &mut Session, args: &Value) -> Value {
    let Some(r) = duty_arg(args) else { return json!({}) };
    let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
    let r2 = r.clone();
    let res = company::act(s, |c| book::answer(c, &r2, yes).ok_or(book::NO_DUTY));
    let plan = {
        let mut g = hosting();
        g.as_mut().and_then(|h| plan_of(s, h))
    };
    match res {
        Ok(guest) => {
            let name = s.company.as_ref().map(|c| book::guest_name(c, guest)).unwrap_or_default();
            if let Some(h) = hosting().as_mut() {
                h.st.tell_duty(unix_now(), if yes { th::GAVE } else { th::REFUSED }, &name, plan.as_ref(), &r);
            }
            if yes {
                prepare(s);
            }
            json!({})
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// Take a player off his duty.
fn take_off(s: &mut Session, args: &Value) -> Value {
    let Some(r) = duty_arg(args) else { return json!({}) };
    let r2 = r.clone();
    let res = company::act(s, |c| book::release(c, &r2, None));
    let plan = {
        let mut g = hosting();
        g.as_mut().and_then(|h| plan_of(s, h))
    };
    match res {
        Ok(guest) => {
            let name = s.company.as_ref().map(|c| book::guest_name(c, guest)).unwrap_or_default();
            if let Some(h) = hosting().as_mut() {
                h.st.tell_duty(unix_now(), th::OFF, &name, plan.as_ref(), &r);
            }
            json!({})
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// A duty's times ("06:00 – 08:10").
fn times(from: i32, to: i32) -> String {
    format!("{} – {}", co::clock::hhmm(from as i64), co::clock::hhmm(to as i64))
}

/// A player's figures for the page: his trips, his punctuality and score, what he brought the
/// company and cost it in fines.
pub fn stats_json(st: &book::GuestStats) -> Value {
    json!({
        "duties": st.duties, "trips": st.trips, "passengers": st.passengers,
        "km": num(st.km, 0),
        "punctuality": st.punctuality().map(|p| format!("{} %", num(p, 0))),
        "score": (st.trips > 0).then(|| st.score()),
        "fares": eur(st.fares), "fines": eur(st.fines), "fined": st.fines > 0,
        "late": st.late,
    })
}

/// A duty of the day in the page's words: its line on its colour, times, route and who has it.
fn duty_row(c: &Company, d: &omsi_launcher_lib::company::together::wire::SnapDuty) -> Value {
    let colour = d.colour.clone();
    json!({
        "duty": d.duty, "number": d.number, "colour": colour, "times": times(d.from, d.to),
        "title": duty_words(&d.number, &d.duty), "route": format!("{} → {}", d.first, d.last),
        "who": d.who, "guest": d.guest, "state": format!("{:?}", d.state).to_lowercase(),
        "open": d.state == DutyState::Open || d.state == DutyState::Company,
        "facts": format!("{}  ·  {} km", trf("%{n} trips", &[("n", &d.trips.to_string())]), d.km),
        "late": c.together.claim_of(&d.duty).is_some_and(|x| x.late),
    })
}

/// A player's line on the page: online, role and permissions, what he does now, his duties of
/// the day and his figures.
fn player_row(c: &Company, st: Option<&HostState>, g: &book::Guest, p: Option<&DayPlan>) -> Value {
    let now = unix_now();
    let online = st.and_then(|st| st.online_now(now).find(|o| o.guest == g.no));
    let kicked = st.is_some_and(|st| st.kicked.contains(&g.no));
    let number = |r: &DutyRef| th::duty_args(p, r)[0].1.clone();
    let mine: Vec<&book::Claim> = c.together.claims.iter().filter(|x| x.guest == g.no && (x.duty.date == c.date || x.late)).collect();
    let (text, tone) = match (online, &online.and_then(|o| o.driving.clone())) {
        _ if g.banned => (tr("Removed from the company"), "danger"),
        _ if kicked => (tr("Sent away from this session"), "warn"),
        (Some(_), Some(r)) => (trf("Drives %{duty}", &[("duty", &duty_words(&number(r), r))]), "ok"),
        (Some(_), None) => (tr("Online"), "ok"),
        (None, _) => (tr("Offline"), "dim"),
    };
    let duties: Vec<Value> = mine
        .iter()
        .map(|x| {
            let num = if x.number.is_empty() { number(&x.duty) } else { x.number.clone() };
            let state = match x.state {
                ClaimState::Asked => tr("Asked for"),
                ClaimState::Driving => tr("Driving it now"),
                ClaimState::Done => tr("Driven"),
                ClaimState::Planned if x.late => tr("Day closed: drives it later"),
                ClaimState::Planned => tr("Planned"),
            };
            let r = th::result_of(c, p, &x.duty);
            json!({
                "duty": x.duty, "what": duty_words(&num, &x.duty), "times": times(x.from, x.to), "state": state,
                "off": matches!(x.state, ClaimState::Planned), "asked": x.state == ClaimState::Asked,
                "result": (r.trips > 0).then(|| trf("%{trips} trips · %{passengers} passengers · score %{score}", &[("trips", &r.trips.to_string()), ("passengers", &r.passengers.to_string()), ("score", &r.score.to_string())])),
            })
        })
        .collect();
    let perms = g.perms();
    json!({
        "no": g.no, "name": g.name, "online": online.is_some(), "banned": g.banned, "kicked": kicked,
        "text": text, "tone": tone, "role": role_json(g.role),
        "perms": perms.iter().map(|p| p.key()).collect::<Vec<_>>(),
        "custom": !g.grant.is_empty() || !g.deny.is_empty(),
        "duties": duties, "stats": stats_json(&g.stats),
        "initial": g.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default(),
    })
}

/// The Multiplayer page (`multiplayer.js`).
fn view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return json!({ "none": true }) };
    let words = json!({
        "title": tr("Multiplayer"), "copy": tr("Copy"), "copied": tr("Copied"),
        "start": tr("Start sharing"), "stop": tr("Stop sharing"),
        "intro": tr("Share your company with friends: they join with a session code, you give each a role, and they drive the duties you give them - or ask for - in their own game, at their own pace. Everything they drive is your company's: its fares, its fines, its figures."),
        "how": [tr("Start sharing: you get a session code to send your friends."), tr("They choose \"Join a company\" in Omsi-Hub and paste it."), tr("Give each player a role here, and duties: they drive them in their own game, no world of yours needed.")],
        "code_caps": tr("Session code").to_uppercase(), "ways_caps": tr("How they reach you").to_uppercase(),
        "players_caps": tr("Players").to_uppercase(), "no_players": tr("Nobody has joined yet: send them the session code."),
        "asked_caps": tr("Duty requests").to_uppercase(), "no_asked": tr("No duty requests."), "yes": tr("Approve"), "no": tr("Decline"),
        "give_caps": tr("Give a duty").to_uppercase(), "give": tr("Give"), "pick_player": tr("Choose a player"), "no_open": tr("No duty to give right now."),
        "activity_caps": tr("Activity").to_uppercase(), "everyone": tr("Everyone"),
        "say": tr("Say something to everyone"), "send": tr("Send"),
        "open_label": tr("Players may take open duties themselves"), "open_note": tr("Off: they ask for a duty, and you (or a player who may approve) say yes or no."),
        "role": tr("Role"), "perms_caps": tr("Permissions").to_uppercase(), "custom": tr("Custom"), "reset": tr("Back to the role's"),
        "kick": tr("Send away"), "ban": tr("Remove"), "back": tr("Let back in"), "off": tr("Take off"),
        "duties_caps": tr("Duties").to_uppercase(), "no_duties": tr("No duties today."),
        "stats_caps": tr("For the company").to_uppercase(),
        "trips": tr("Trips"), "punctuality": tr("On time"), "score": tr("Score"), "fares": tr("Fares"), "fines": tr("Fines"),
        "online": tr("online"), "kick_tip": tr("Send away from this session: they can join again with a new code."), "ban_tip": tr("Remove from the company: they cannot join again until you let them back in."),
        "not_shared": tr("Not shared: the players keep their roles, but cannot reach you."),
    });
    let mut g = hosting();
    let roles = roles_json();
    let Some(h) = g.as_mut().filter(|h| h.company == c.id) else {
        let other = g.as_ref().map(|h| h.company.clone());
        drop(g);
        // (the players the company keeps, with their roles, also while it is not shared)
        let players: Vec<Value> = c.together.guests.iter().map(|g| player_row(&c, None, g, None)).collect();
        return json!({ "on": false, "words": words, "roles": roles, "players": players, "other": other.map(|_| tr("A session of another of your companies runs: starting one here ends it.")) });
    };
    let plan = plan_of(s, h);
    let port = h.server.port();
    let mut ways: Vec<Value> = omsi_net::addrs::joinable_addresses().iter().take(4).map(|a| json!([tr(a.label()), format!("{}:{port}", a.ip), if a.kind.is_vpn() { "ok" } else { "soft" }])).collect();
    ways.push(match h.server.tunnel() {
        TunnelState::Ready(u) => json!([tr("Internet"), trf("through a Cloudflare tunnel (%{url})", &[("url", &u)]), "ok"]),
        TunnelState::Starting => json!([tr("Internet"), tr("the tunnel is starting…"), "dim"]),
        TunnelState::Failed => json!([tr("Internet"), tr("no tunnel: friends over the internet need the same VPN"), "warn"]),
        TunnelState::Off => json!([tr("Internet"), tr("no tunnel (switched off): friends over the internet need the same VPN"), "dim"]),
    });
    // (the boss's own view of the day: every duty, who has it)
    let lines = lines_of(s);
    let here = Here { plan: plan.as_ref(), lines: &lines, now: unix_now(), lead: 0 };
    let snap = th::snapshot(&c, &here, &h.st, None, 0);
    let giveable: Vec<Value> = snap.duties.iter().filter(|d| matches!(d.state, DutyState::Open | DutyState::Company | DutyState::Player)).map(|d| duty_row(&c, d)).collect();
    let asked: Vec<Value> = c
        .together
        .claims
        .iter()
        .filter(|x| x.state == ClaimState::Asked && x.duty.date == c.date)
        .map(|x| json!({ "duty": x.duty, "no": x.guest, "name": book::guest_name(&c, x.guest), "what": duty_words(&x.number, &x.duty), "times": times(x.from, x.to) }))
        .collect();
    // (the players online first, then by name)
    let now = unix_now();
    let mut guests: Vec<&book::Guest> = c.together.guests.iter().collect();
    guests.sort_by_key(|g| (!h.st.is_online(g.no, now), g.banned, g.name.to_lowercase()));
    let players: Vec<Value> = guests.iter().map(|g| player_row(&c, Some(&h.st), g, plan.as_ref())).collect();
    let notices: Vec<Value> = h
        .st
        .notices
        .iter()
        .rev()
        .take(80)
        .map(|n| {
            let mut v = notice_json(n);
            v["who"] = json!(n.args.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()));
            v
        })
        .collect();
    let online = h.st.online_now(now).count();
    tell_boss(s, h);
    json!({
        "on": true, "words": words, "roles": roles, "code": h.text, "ways": ways,
        "open_claims": c.together.open_claims, "players": players, "asked": asked, "giveable": giveable, "notices": notices,
        "online": trf("%{n} online", &[("n", &online.to_string())]),
        "note": tr("Over the internet: both need the same VPN (Hamachi, Radmin VPN, ZeroTier, Tailscale), or a friend comes in through your tunnel. Everybody needs the same version of Omsi-Hub."),
        "perm_keys": Perm::ALL.iter().map(|p| p.key()).collect::<Vec<_>>(),
    })
}

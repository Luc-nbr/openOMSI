//! The boss's side of "Play together": the session of his open company - its company server
//! (`together::server`, answered on the company itself under the session's lock, so it answers
//! whatever page the launcher shows and while the game runs), its code, the players online and
//! what each drives, the duties asked for, the notices - and his world: his own duties and
//! "Open the world" start his game as the host of a game session on the company's map, whose
//! code a friend's game joins.

use super::super::company::kit::Popup;
use super::super::company::words::{self, tr, trf};
use super::super::company::{self, Session};
use super::{duty_arg, duty_words, notice_json, notice_text, unix_now};
use crate::{Answer, Platform};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::plan::DayPlan;
use omsi_launcher_lib::company::together::book::{self, ClaimState};
use omsi_launcher_lib::company::together::code::{self, CompanyCode};
use omsi_launcher_lib::company::together::host::{self as th, Here, HostState};
use omsi_launcher_lib::company::together::server::{Server, TunnelState};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
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
    /// The boss's world was open when last looked at.
    world_open: bool,
}

static HOSTING: Mutex<Option<Hosting>> = Mutex::new(None);
/// The boss's page's language: the server's threads tell in it.
static LANG: Mutex<String> = Mutex::new(String::new());

fn hosting() -> MutexGuard<'static, Option<Hosting>> {
    HOSTING.lock().unwrap_or_else(|e| e.into_inner())
}

/// The page's language kept for the server's threads.
pub(super) fn remember_language() {
    *LANG.lock().unwrap_or_else(|e| e.into_inner()) = words::language();
}

/// The boss's game hosting a session: its code and its map.
fn world() -> Option<(String, String)> {
    core::instances::local_hosts().into_iter().find_map(|v| {
        let code = v.get("code")?.as_str()?.to_string();
        Some((code, v.get("map").and_then(Value::as_str).unwrap_or("").to_string()))
    })
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

/// The boss's world opened since it was last looked at: told to everyone.
fn see_world(h: &mut Hosting, c: &Company, world: &Option<(String, String)>) {
    let open = world.as_ref().is_some_and(|(_, m)| th::same_map(m, &c.map));
    if open && !h.world_open {
        h.st.tell(unix_now(), th::WORLD_OPEN, Vec::new());
    }
    h.world_open = open;
}

/// The notices the boss is told in the company's corner (the rest are in his panel).
const TOLD: [&str; 6] = [th::JOINED, th::LEFT, th::ASKS, th::WAITS, th::FINISHED, th::SAYS];

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
        if let Err(v) = h.st.admits(body) {
            return v.to_string();
        }
        if s.company.as_ref().map(|c| c.id.as_str()) != Some(h.company.as_str()) {
            return th::refusal(th::NOT_OPEN_COMPANY).to_string();
        }
        // (the company's day simulated on a thread, which hands its own copy back: nothing is
        // changed meanwhile - the friend's launcher asks again)
        if s.closing {
            return th::refusal(th::BUSY).to_string();
        }
        let plan = plan_of(s, h);
        let world = world();
        if let Some(c) = s.company.as_ref() {
            see_world(h, c, &world);
        }
        let (now, lead) = (unix_now(), lead());
        let answered = {
            let Some(c) = s.company.as_mut() else { return th::refusal(th::NOT_OPEN_COMPANY).to_string() };
            let lines: &[core::LineInfo] = match s.today.as_ref() {
                Some(t) if t.map == c.map && t.date == c.date && t.error.is_none() => &t.lines,
                _ => &[],
            };
            let here = Here { plan: plan.as_ref(), lines, world, now, lead };
            h.st.answer(c, &here, body)
        };
        if answered.changed {
            company::changed(s);
        }
        tell_boss(s, h);
        answered.body.to_string()
    })
}

/// The players online now in the open company's session: (number, name, what he drives).
pub fn online_guests(c: &Company) -> Vec<(u32, String, Option<DutyRef>)> {
    let g = hosting();
    let Some(h) = g.as_ref().filter(|h| h.company == c.id) else { return Vec::new() };
    h.st.online_now(unix_now()).map(|o| (o.guest, o.name.clone(), o.driving.clone())).collect()
}

/// The boss's own duty while his session runs: his game hosts, so that friends join his world.
pub fn boss_duty(c: &Company, d: &mut core::Duty) {
    if hosting().as_ref().is_some_and(|h| h.company == c.id) {
        d.lan = Some("host".into());
        d.lan_name = Some(c.profile.clone()).filter(|n| !n.trim().is_empty());
    }
}

/// Give duty `k` of tour `ti` of the day `p` to player `guest` (RosterFlow's "Who drives it").
pub fn give_to_guest(s: &mut Session, p: &DayPlan, ti: usize, k: usize, guest: u32) -> Value {
    let Some(r) = DutyRef::of(p, ti, k) else { return json!({}) };
    let now = s.company.as_ref().map(co::clock::now).unwrap_or(0);
    let r2 = r.clone();
    let p2 = p.clone();
    match company::act(s, |c| book::give(c, &p2, &r2, guest, now)) {
        Ok(()) => {
            let name = s.company.as_ref().map(|c| book::guest_name(c, guest)).unwrap_or_default();
            if let Some(h) = hosting().as_mut() {
                h.st.tell_duty(unix_now(), th::GAVE, &name, Some(p), &r);
            }
            json!({})
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// The commands of the boss's panel.
pub(super) fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    Some(match cmd {
        "tg_host_view" => company::with(|s| Ok(view(s))),
        "tg_host_badge" => company::with(|s| Ok(badge(s))),
        "tg_host_start" => company::with(start),
        "tg_host_stop" => company::with(|s| {
            // (the players' duties not driven go back to the company: nobody can drive them now)
            let mine = s.company.as_ref().is_some_and(|c| hosting().as_ref().is_some_and(|h| h.company == c.id));
            stop(s);
            if mine {
                let _ = company::act(s, |c| {
                    c.together.claims.retain(|x| x.state == ClaimState::Done);
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
        "tg_host_say" => company::with(|s| {
            let text = book::clean(args.get("text").and_then(Value::as_str).unwrap_or(""), 160);
            let boss = s.company.as_ref().map(|c| c.profile.clone()).unwrap_or_default();
            if let Some(h) = hosting().as_mut().filter(|_| !text.is_empty()) {
                h.st.tell(unix_now(), th::SAYS, vec![("name".into(), boss), ("text".into(), text)]);
            }
            Ok(json!({}))
        }),
        "tg_host_world" => company::with(|s| open_world(s, args, p)),
        _ => return None,
    })
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
    *hosting() = Some(Hosting { company: c.id.clone(), text, server, st: HostState::new(&c.id, &c.profile, secret), plan: None, told: 0, world_open: false });
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
    let Some(h) = g.as_ref().filter(|h| h.company == c.id) else { return json!({ "on": false, "tip": tr("Play together: friends drive duties of your company in your world") }) };
    let online = h.st.online_now(unix_now()).count();
    let asked = c.together.claims.iter().filter(|x| x.state == ClaimState::Asked && x.duty.date == c.date).count();
    json!({ "on": true, "online": online, "asked": asked, "tip": trf("Play together: %{n} online", &[("n", &online.to_string())]) })
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

/// "Open the world": the boss's game started as the host of a game session on the company's
/// map, at the company's day and time, with its service (its buses on their tours, its
/// passengers), the boss on foot by a bus of the fleet - friends' games join it.
fn open_world(s: &mut Session, args: &Value, p: &dyn Platform) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if s.in_game() {
        return Ok(json!({ "popup": company::popup(s, Popup::new("sports_esports", "A game is running", tr("Your world is open while it runs."), tr("Friends join it when it hosts a session; close it to open another."), None)) }));
    }
    let root = core::load_config().root;
    if !omsi_cfg::missing_original_essentials(std::path::Path::new(&root)).is_empty() {
        return Ok(json!({ "popup": company::popup(s, Popup::new("folder_open", "The original OMSI 2 is missing", tr("A session needs the original OMSI 2."), tr("Choose its folder under Setup first."), None)) }));
    }
    let Some(v) = c.fleet.iter().find(|v| v.held_on(&c.date) && !v.bus.trim().is_empty()) else {
        return Ok(json!({ "popup": company::popup(s, Popup::new("directions_bus", "No bus to start with", tr("The world opens with you on foot beside a bus of the fleet."), tr("Buy or lease a bus first."), None)) }));
    };
    let mut d: core::Duty = serde_json::from_value(args.get("base").cloned().unwrap_or(Value::Null)).map_err(|e| format!("base: {e}"))?;
    d.map = c.map.clone();
    d.bus = v.bus.clone();
    d.paint = v.house_livery.clone().filter(|h| !h.trim().is_empty()).or_else(|| Some(v.livery.clone())).filter(|x| !x.trim().is_empty());
    d.number = Some(v.number.clone()).filter(|x| !x.trim().is_empty());
    d.plate = Some(v.plate.clone()).filter(|x| !x.trim().is_empty());
    d.hof = Some(c.depot.clone()).filter(|x| !x.trim().is_empty());
    d.set_vars = super::super::hub_more::setvars(&d.bus, d.paint.as_deref().unwrap_or(""));
    d.display_font = core::busfonts::BusFonts::load().font_for(&d.bus);
    (d.entry, d.line, d.tour, d.trip, d.whole_tour, d.legs, d.free_line) = (None, None, None, None, false, Vec::new(), None);
    d.time = co::clock::hhmm(c.clock.minute as i64);
    d.date = Some(c.date.clone());
    (d.passengers, d.schedule, d.on_foot) = (Some(true), Some(true), Some(true));
    (d.season, d.tutorial, d.situation, d.editor) = (None, None, None, false);
    d.company = Some(c.id.clone());
    d.lan = Some("host".into());
    d.lan_name = Some(c.profile.clone()).filter(|n| !n.trim().is_empty());
    core::log_to_file(&format!("company: Play together opens the world of {} on {} at {}", c.id, c.date, d.time));
    let launched = core::launch(&d).map_err(|e| format!("{e:#}"))?;
    core::log_to_file(&format!("game {}: {}", launched.pid, launched.command));
    p.launched();
    s.say(tr("Opening your company's world…"), false);
    Ok(json!({ "launched": serde_json::to_value(launched).unwrap_or(Value::Null) }))
}

/// A player's line in the panel: his name, and what he does now.
fn player_rows(c: &Company, h: &Hosting, p: Option<&DayPlan>) -> Vec<Value> {
    let now = unix_now();
    let number = |r: &DutyRef| th::duty_args(p, r)[0].1.clone();
    let mut rows: Vec<Value> = Vec::new();
    for o in h.st.online_now(now) {
        let (text, tone) = match (&o.driving, &o.waiting) {
            (Some(r), _) => (trf("Drives %{duty}", &[("duty", &duty_words(&number(r), r))]), "ok"),
            (None, Some(r)) => (trf("Waits for your world to drive %{duty}", &[("duty", &duty_words(&number(r), r))]), "warn"),
            _ => {
                let n = c.together.claims.iter().filter(|x| x.guest == o.guest && x.duty.date == c.date && x.state != ClaimState::Asked).count();
                (if n == 0 { tr("Online") } else { trf("Online · %{n} duties today", &[("n", &n.to_string())]) }, "soft")
            }
        };
        rows.push(json!({ "name": o.name, "online": true, "text": text, "tone": tone }));
    }
    for g in &c.together.guests {
        if h.st.is_online(g.no, now) {
            continue;
        }
        let n = c.together.claims.iter().filter(|x| x.guest == g.no && x.duty.date == c.date && x.state != ClaimState::Asked).count();
        if n > 0 {
            rows.push(json!({ "name": g.name, "online": false, "text": trf("Offline · %{n} duties today", &[("n", &n.to_string())]), "tone": "dim" }));
        }
    }
    rows
}

/// The players' duties of the day (to take them off), and those asked for (to say yes or no).
fn duty_rows(c: &Company, p: Option<&DayPlan>) -> (Vec<Value>, Vec<Value>) {
    let (mut theirs, mut asked) = (Vec::new(), Vec::new());
    for x in c.together.claims.iter().filter(|x| x.duty.date == c.date) {
        let number = th::duty_args(p, &x.duty)[0].1.clone();
        let times = p.and_then(|p| x.duty.find(p).map(|(ti, k)| p.tours[ti].duties[k].clone())).map(|d| format!("{} – {}", co::clock::hhmm(d.from as i64), co::clock::hhmm(d.to as i64))).unwrap_or_default();
        let name = book::guest_name(c, x.guest);
        let base = json!({ "duty": x.duty, "name": name, "what": duty_words(&number, &x.duty), "times": times });
        match x.state {
            ClaimState::Asked => asked.push(base),
            st => {
                let mut v = base;
                v["state"] = json!(tr(match st {
                    ClaimState::Driving => "Driving it now",
                    ClaimState::Done => "Driven",
                    _ => "Planned",
                }));
                v["off"] = json!(st == ClaimState::Planned);
                // (what his trips came to so far)
                let r = th::result_of(c, p, &x.duty);
                if r.trips > 0 {
                    v["result"] = json!(trf("%{trips} trips · %{passengers} passengers · score %{score} · for the company %{amount}", &[("trips", &r.trips.to_string()), ("passengers", &r.passengers.to_string()), ("score", &r.score.to_string()), ("amount", &super::super::company::words::eur(r.fares))]));
                }
                theirs.push(v);
            }
        }
    }
    (theirs, asked)
}

/// The panel (`together.js`'s `TogetherDialog`).
fn view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return json!({ "none": true }) };
    let mut g = hosting();
    let words = json!({
        "title": tr("Play together"), "close": tr("Close"), "copy": tr("Copy"), "copied": tr("Copied"),
        "start": tr("Start a session"), "stop": tr("End the session"),
        "intro": tr("Friends join your company with a session code and drive duties of its day for it, in your world. Money, planning and staff stay yours: their trips are booked as your drivers' are."),
        "how": [tr("Start a session: you get a code to send your friends."), tr("They choose \"Join a company\" in Omsi-Hub and paste it."), tr("They take open duties (or you give them one in RosterFlow's Roster), and drive them in your world: open it here, or drive a duty of your own.")],
        "code_caps": tr("Session code").to_uppercase(),
        "players_caps": tr("Players").to_uppercase(), "no_players": tr("Nobody has joined yet."),
        "asked_caps": tr("Asked for").to_uppercase(), "yes": tr("Yes"), "no": tr("No"),
        "duties_caps": tr("Their duties today").to_uppercase(), "off": tr("Take off"),
        "notices_caps": tr("What happened").to_uppercase(),
        "say": tr("Say something to everyone"), "send": tr("Send"),
        "open_label": tr("Players may take open duties themselves"), "open_note": tr("Off: they ask for a duty, and you say yes or no here."),
    });
    let Some(h) = g.as_mut().filter(|h| h.company == c.id) else {
        let other = g.as_ref().map(|h| h.company.clone());
        return json!({ "on": false, "words": words, "other": other.map(|_| tr("A session of another of your companies runs: starting one here ends it.")) });
    };
    let plan = plan_of(s, h);
    let world = world();
    see_world(h, &c, &world);
    let port = h.server.port();
    let mut ways: Vec<Value> = omsi_net::addrs::joinable_addresses().iter().take(4).map(|a| json!([tr(a.label()), format!("{}:{port}", a.ip), if a.kind.is_vpn() { "ok" } else { "soft" }])).collect();
    ways.push(match h.server.tunnel() {
        TunnelState::Ready(u) => json!([tr("Internet"), trf("through a Cloudflare tunnel (%{url})", &[("url", &u)]), "ok"]),
        TunnelState::Starting => json!([tr("Internet"), tr("the tunnel is starting…"), "dim"]),
        TunnelState::Failed => json!([tr("Internet"), tr("no tunnel: friends over the internet need the same VPN"), "warn"]),
        TunnelState::Off => json!([tr("Internet"), tr("no tunnel (switched off): friends over the internet need the same VPN"), "dim"]),
    });
    let world_json = match &world {
        Some((_, m)) if th::same_map(m, &c.map) => json!({ "open": true, "text": tr("Your world is open: friends drive their duties in it.") }),
        Some(_) => json!({ "open": false, "text": tr("Your game runs another map: friends can only drive in the company's world."), "button": null }),
        None => json!({ "open": false, "text": tr("Your world isn't open yet: friends cannot drive until it is. Drive a duty of your own, or open the world with you on foot at the depot."), "button": tr("Open the world") }),
    };
    let (theirs, asked) = duty_rows(&c, plan.as_ref());
    let notices: Vec<Value> = h.st.notices.iter().rev().take(30).map(notice_json).collect();
    let players = player_rows(&c, h, plan.as_ref());
    tell_boss(s, h);
    json!({
        "on": true, "words": words, "code": h.text, "ways": ways, "world": world_json,
        "open_claims": c.together.open_claims, "players": players, "asked": asked, "theirs": theirs, "notices": notices,
        "note": tr("Over the internet: both need the same VPN (Hamachi, Radmin VPN, ZeroTier, Tailscale), or a friend comes in through your tunnel. Everybody needs the same version of Omsi-Hub."),
    })
}

//! Omsi-Hub online, the launcher's side (docs/omsihub/ONLINE.md; the client is the crate
//! `omsihub-online`): the optional account, the friends, their online state and the invites to
//! a company session. Without an account - or without a network - the launcher is as it was:
//! nothing here blocks it, and the page is told "offline" quietly.
//!
//! * `ol_state` - what the page shows, at once from what the worker last fetched (no network);
//!   the first call starts the worker: the heartbeat about once a minute and whenever the state
//!   changes (online, away, hosting a session), the friends and the invites with it;
//! * `ol_sign_in {email, password}`, `ol_verify {email, code, password}`, `ol_resend {email}`,
//!   `ol_sign_out`; an account is made on the website (`sign_up_url` in the state, which the page
//!   opens), and its display name becomes the profile at the first sign-in here;
//! * `ol_forgot {email}`, `ol_reset {email, code, password}` - the password reset by a code;
//! * `ol_local {on}` - "Play without an account" (only that choice is kept, in
//!   `~/.openomsi/omsihub-online.json`);
//! * `ol_profile {name}` - the display name chosen or changed;
//! * `ol_delete {password}` - the account and everything of it deleted, its cloud saves first
//!   (`cloud.rs` then makes every company here local);
//! * `ol_add {code}`, `ol_respond {id, accept}`, `ol_remove {id}`, `ol_block {id}`;
//! * `ol_invite {id}` - the company session hosted now, to a friend; `ol_dismiss {id}`;
//! * `ol_presence {away}`, `ol_seen`, `ol_refresh`.
//!
//! Every answer is JSON with the texts in the page's language (`lang`): `{ok, message}` or
//! `{error, code}`, the error one of `omsihub_online::codes`' never the server's own words. The
//! email address and the password pass through and are kept nowhere; the refresh token is in
//! the OS's credential store only (`omsihub_online::store`). Nothing here is logged.

use super::company::words::{self, tr, trf};
use super::together;
use crate::Answer;
use omsi_launcher_lib as core;
use omsihub_online::{codes, store, Client, Endpoints, Error, Friend, Invite, Profile};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;

/// What the page is shown, as the worker and the commands last saw it.
#[derive(Default)]
struct View {
    worker: bool,
    signed_in: bool,
    /// The session outlives the launcher (kept in the OS's store).
    lasting: bool,
    offline: bool,
    /// Signed in without a profile yet: the display name to choose.
    need_profile: bool,
    profile: Option<Profile>,
    friends: Vec<Friend>,
    invites: Vec<Invite>,
    /// The requests and invites the page has shown (the badge counts the others).
    seen: HashSet<String>,
    /// The window in the background a while (the page says so).
    away: bool,
    /// The state and the session last told the server, and when.
    told: Option<(&'static str, Option<String>)>,
    beat_at: u64,
    refreshed_at: u64,
}

static VIEW: LazyLock<Mutex<View>> = LazyLock::new(Default::default);
/// The client (made at the first need; its calls run one at a time, under this lock, never
/// under `VIEW`'s).
static CLIENT: Mutex<Option<Client>> = Mutex::new(None);

fn view() -> MutexGuard<'static, View> {
    VIEW.lock().unwrap_or_else(|e| e.into_inner())
}

fn client() -> MutexGuard<'static, Option<Client>> {
    CLIENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn endpoints() -> Endpoints {
    // (the tests reach nothing: an HTTPS address nobody answers on)
    #[cfg(test)]
    return Endpoints::new("https://127.0.0.1:9", "sb_publishable_test").unwrap();
    #[cfg(not(test))]
    return standin().unwrap_or_else(Endpoints::supabase);
}

/// A debug build's stand-in on this computer (`OMSIHUB_STANDIN=http://127.0.0.1:54321`, the key
/// `OMSIHUB_STANDIN_KEY`; online/standin/standin.mjs): the flows tried without the real project.
/// A release build never reads it.
#[cfg(not(test))]
fn standin() -> Option<Endpoints> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let base = std::env::var("OMSIHUB_STANDIN").ok().filter(|b| !b.is_empty())?;
    Endpoints::local(&base, &std::env::var("OMSIHUB_STANDIN_KEY").unwrap_or_else(|_| "sb_publishable_standin".into())).ok()
}

fn new_store() -> Box<dyn store::SecretStore> {
    #[cfg(test)]
    return Box::new(store::MemoryStore::new());
    // (a stand-in's session is not the player's: it stays out of the OS's store)
    #[cfg(not(test))]
    return if standin().is_some() { Box::new(store::MemoryStore::new()) } else { store::os_store() };
}

/// `f` with the client, made (and the stored session read) on the first call.
pub(super) fn with<T>(f: impl FnOnce(&mut Client) -> T) -> T {
    let mut g = client();
    let c = g.get_or_insert_with(|| Client::new(endpoints(), new_store()));
    f(c)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

// --- the choice of playing without an account --------------------------------------------------

const FILE: &str = "omsihub-online.json";

fn local() -> bool {
    std::fs::read_to_string(core::data_dir().join(FILE)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.get("local").and_then(Value::as_bool)).unwrap_or(false)
}

fn set_local(on: bool) {
    let _ = std::fs::write(core::data_dir().join(FILE), json!({ "local": on }).to_string());
}

// --- the answers ------------------------------------------------------------------------------

/// An error as the page shows it; signed out or offline, the view says so too.
fn fail(e: Error) -> Value {
    {
        let mut v = view();
        match e {
            Error::Offline => v.offline = true,
            Error::SignedOut => signed_out(&mut v),
            _ => v.offline = false,
        }
    }
    json!({ "error": tr(e.text()), "code": e.code() })
}

fn done(message: &str) -> Value {
    view().offline = false;
    json!({ "ok": true, "message": message })
}

fn answer(r: Result<Value, Error>) -> Value {
    r.unwrap_or_else(fail)
}

fn signed_out(v: &mut View) {
    v.signed_in = false;
    v.need_profile = false;
    v.profile = None;
    v.friends.clear();
    v.invites.clear();
    v.told = None;
}

fn str_arg<'a>(args: &'a Value, name: &str) -> &'a str {
    args.get(name).and_then(Value::as_str).unwrap_or("")
}

// --- what is fetched -------------------------------------------------------------------------

/// The state to tell: in a session while hosting one (its code with it), else online or away.
fn presence(away: bool) -> (&'static str, Option<String>) {
    match together::session_code() {
        Some(code) => ("busy", Some(code)),
        None if away => ("away", None),
        None => ("online", None),
    }
}

/// The heartbeat, then the profile, the friends and the invites, into the view.
fn refresh(beat: bool) -> Result<(), Error> {
    let away = view().away;
    let now_state = presence(away);
    let got = with(|c| -> Result<_, Error> {
        if !c.signed_in() {
            return Ok(None);
        }
        if beat {
            // (no profile yet: the heartbeat has nothing to tell)
            match c.heartbeat(now_state.0, now_state.1.as_deref()) {
                Err(e @ (Error::Offline | Error::SignedOut)) => return Err(e),
                _ => {}
            }
        }
        let profile = c.profile()?;
        let (friends, invites) = if profile.is_some() { (c.friends()?, c.invites()?) } else { (Vec::new(), Vec::new()) };
        Ok(Some((profile, friends, invites, c.lasting())))
    });
    let mut v = view();
    match got {
        Ok(Some((profile, friends, invites, lasting))) => {
            v.signed_in = true;
            v.lasting = lasting;
            v.offline = false;
            v.need_profile = profile.is_none();
            v.profile = profile;
            v.friends = friends;
            v.invites = invites;
            v.refreshed_at = unix_now();
            if beat {
                v.told = Some(now_state);
                v.beat_at = unix_now();
            }
            Ok(())
        }
        Ok(None) => {
            signed_out(&mut v);
            Ok(())
        }
        Err(e) => {
            match e {
                Error::Offline => v.offline = true,
                Error::SignedOut => signed_out(&mut v),
                _ => {}
            }
            // (tried again at the next beat, not at the next look)
            if beat {
                v.beat_at = unix_now();
                v.told = Some(now_state);
            }
            Err(e)
        }
    }
}

/// The worker: a look every few seconds, the heartbeat once a minute or when the state to tell
/// changed. It runs while the launcher does; it never holds the view during a request.
fn worker() {
    let mut first = true;
    loop {
        let due = {
            let v = view();
            let state = presence(v.away);
            first || (v.signed_in && (unix_now().saturating_sub(v.beat_at) >= 60 || v.told.as_ref() != Some(&state)))
        };
        if due {
            let signed = with(|c| c.signed_in());
            if signed {
                // (a session the store kept is signed in, offline too)
                view().signed_in = true;
                let _ = refresh(true);
            }
            first = false;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

fn start_worker() {
    let mut v = view();
    if v.worker {
        return;
    }
    v.worker = true;
    let _ = std::thread::Builder::new().name("omsihub online".into()).spawn(worker);
}

/// "Offline" told as the launcher closes (best effort, a few seconds at most; skipped while
/// another request runs).
pub fn goodbye() {
    if let Ok(mut g) = CLIENT.try_lock() {
        if let Some(c) = g.as_mut() {
            c.goodbye();
        }
    }
}

// --- the page's view ----------------------------------------------------------------------------

/// Seconds since the Unix epoch of an RFC 3339 time (`2026-10-16T10:00:00.123+02:00`).
pub(super) fn unix_of(ts: &str) -> Option<i64> {
    let ts = ts.trim();
    let n = |a: usize, b: usize| ts.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let (y, mo, d, h, mi, s) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let rest = &ts[19..];
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match rest.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let z: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let (oh, om) = (z.get(0..2)?.parse::<i64>().ok()?, z.get(2..4).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0));
            (oh * 3600 + om * 60) * if sign == '-' { -1 } else { 1 }
        }
        _ => 0,
    };
    // (days from the civil date, Howard Hinnant's)
    let (y, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + h * 3600 + mi * 60 + s - offset)
}

/// "Last seen 5 min ago".
fn seen_words(last: Option<&str>) -> String {
    let Some(t) = last.and_then(unix_of) else { return String::new() };
    let ago = (unix_now() as i64 - t).max(0);
    match ago {
        0..=119 => tr("Last seen just now"),
        120..=3599 => trf("Last seen %{n} min ago", &[("n", &(ago / 60).to_string())]),
        3600..=86399 => trf("Last seen %{n} h ago", &[("n", &(ago / 3600).to_string())]),
        86400..=2591999 => trf("Last seen %{n} days ago", &[("n", &(ago / 86400).to_string())]),
        _ => tr("Last seen a long time ago"),
    }
}

/// A friend's state: `online`, `away`, `busy` (in a session) or `offline`.
fn state_of(f: &Friend) -> &'static str {
    match (f.online, f.status.as_str()) {
        (false, _) | (_, "offline") => "offline",
        (true, "busy") => "busy",
        (true, "away") => "away",
        _ => "online",
    }
}

fn state_label(s: &str) -> String {
    tr(match s {
        "online" => "Online",
        "away" => "Away",
        "busy" => "In a session",
        _ => "Offline",
    })
}

fn friend_json(f: &Friend) -> Value {
    let st = state_of(f);
    let session = f.session_code.clone().filter(|_| st != "offline");
    json!({
        "id": f.user_id, "name": f.display_name, "state": st, "state_label": state_label(st),
        "seen": if st == "offline" { seen_words(f.last_seen.as_deref()) } else { String::new() },
        "session": session, "join": session.is_some() && together::enabled(),
    })
}

fn state() -> Value {
    start_worker();
    let hosting = together::session_code().is_some();
    let v = view();
    let mode = if v.signed_in { "in" } else if local() { "local" } else { "out" };
    let friends: Vec<&Friend> = v.friends.iter().filter(|f| f.relation == "accepted").collect();
    let requests: Vec<&Friend> = v.friends.iter().filter(|f| f.relation == "pending").collect();
    let name_of = |id: &str| v.friends.iter().find(|f| f.user_id == id).map(|f| f.display_name.clone()).unwrap_or_else(|| tr("A friend"));
    let new_requests = requests.iter().filter(|f| f.incoming && !v.seen.contains(&format!("r:{}", f.user_id))).count();
    // (an invite holds for 15 minutes)
    let invites: Vec<&Invite> = v.invites.iter().filter(|i| together::enabled() && i.created_at.as_deref().and_then(unix_of).is_none_or(|t| unix_now() as i64 - t < 15 * 60)).collect();
    let new_invites = invites.iter().filter(|i| !v.seen.contains(&format!("i:{}", i.key()))).count();
    let mut order: Vec<&&Friend> = friends.iter().collect();
    let rank = |f: &Friend| match state_of(f) {
        "busy" => 0,
        "online" => 1,
        "away" => 2,
        _ => 3,
    };
    order.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
    json!({
        "mode": mode,
        "offline": v.offline,
        "lasting": v.lasting,
        "need_profile": v.need_profile,
        "me": v.profile.as_ref().map(|p| json!({ "name": p.display_name, "code": p.friend_code })),
        "friends": order.iter().map(|f| friend_json(f)).collect::<Vec<_>>(),
        "online_count": friends.iter().filter(|f| state_of(f) != "offline").count(),
        "requests": requests.iter().map(|f| json!({ "id": f.user_id, "name": f.display_name, "incoming": f.incoming, "new": f.incoming && !v.seen.contains(&format!("r:{}", f.user_id)) })).collect::<Vec<_>>(),
        "invites": invites.iter().map(|i| {
            let from = name_of(&i.from);
            json!({ "id": i.key(), "from": from, "session": i.session_code, "text": trf("%{name} invites you to their company", &[("name", &from)]), "new": !v.seen.contains(&format!("i:{}", i.key())) })
        }).collect::<Vec<_>>(),
        "badge": new_requests + new_invites,
        "hosting": hosting,
        "together": together::enabled(),
        "sign_up_url": omsihub_online::sign_up_page(),
    })
}

// --- the account ------------------------------------------------------------------------------

/// Signed in: the profile (made, when there is none, with the display name chosen on the
/// website's sign-up), the friends, the heartbeat.
fn after_sign_in() -> Value {
    set_local(false);
    {
        let mut v = view();
        v.signed_in = true;
        v.seen.clear();
    }
    let made = with(|c| -> Result<(), Error> {
        if let Some(n) = c.signed_up_name().map(str::to_string) {
            if c.profile()?.is_none() {
                // (a name the profiles table will not take, or taken since, is chosen again here)
                if let Err(e @ (Error::Offline | Error::SignedOut)) = c.create_profile(&n) {
                    return Err(e);
                }
            }
        }
        Ok(())
    });
    if let Err(e) = made.and_then(|_| refresh(true)) {
        return fail(e);
    }
    let v = view();
    let message = match &v.profile {
        Some(p) => trf("Signed in as %{name}.", &[("name", &p.display_name)]),
        None => tr("Signed in. Choose your display name."),
    };
    drop(v);
    done(&message)
}

fn verify(args: &Value) -> Value {
    let (email, password) = (str_arg(args, "email"), str_arg(args, "password"));
    let signed = with(|c| c.verify_email(email, str_arg(args, "code")).and_then(|in_| if in_ || password.is_empty() { Ok(in_) } else { c.sign_in(email, password).map(|_| true) }));
    match signed {
        Ok(true) => after_sign_in(),
        Ok(false) => {
            let mut a = done(&tr("Email address confirmed. Sign in now."));
            a["step"] = json!("sign_in");
            a
        }
        Err(e) => fail(e),
    }
}

fn sign_in(args: &Value) -> Value {
    match with(|c| c.sign_in(str_arg(args, "email"), str_arg(args, "password"))) {
        Ok(()) => after_sign_in(),
        Err(e) => {
            let mut a = fail(e);
            if e == Error::Code("email_not_verified") {
                a["step"] = json!("verify");
            }
            a
        }
    }
}

fn reset(args: &Value) -> Value {
    let (email, password) = (str_arg(args, "email"), str_arg(args, "password"));
    // (the code signs in, and the new password is set with it)
    match with(|c| c.reset_password(email, str_arg(args, "code"), password)) {
        Ok(()) => {
            let mut a = after_sign_in();
            if a.get("ok").is_some() {
                a["message"] = json!(tr("Password changed. You are signed in."));
            }
            a
        }
        Err(e) => fail(e),
    }
}

fn sign_out() -> Value {
    with(|c| {
        c.goodbye();
        c.sign_out();
    });
    signed_out(&mut view());
    done(&tr("Signed out. Omsi-Hub plays on locally."))
}

fn delete(args: &Value) -> Value {
    match with(|c| c.delete_account(str_arg(args, "password"))) {
        Ok(()) => {
            signed_out(&mut view());
            // (its cloud saves went with it: the companies here are local saves now)
            super::cloud::forget_account();
            done(&tr("Your account, your cloud saves and everything of it are deleted."))
        }
        Err(e) => fail(e),
    }
}

fn profile(args: &Value) -> Value {
    let name = str_arg(args, "name");
    let need = view().need_profile;
    answer(with(|c| if need { c.create_profile(name) } else { c.set_display_name(name) }).and_then(|_| refresh(false)).map(|_| done(&tr("Display name saved."))))
}

// --- the friends ----------------------------------------------------------------------------

fn name_of(id: &str) -> String {
    view().friends.iter().find(|f| f.user_id == id).map(|f| f.display_name.clone()).unwrap_or_else(|| tr("A friend"))
}

/// A change to the friends, then the list fetched again.
fn friends_act(f: impl FnOnce(&mut Client) -> Result<&'static str, Error>, message: impl FnOnce(&'static str) -> String) -> Value {
    match with(f) {
        Ok(code) => {
            let _ = refresh(false);
            done(&message(code))
        }
        Err(e) => fail(e),
    }
}

fn invite(args: &Value) -> Value {
    let id = str_arg(args, "id").to_string();
    let Some(code) = together::session_code() else { return fail(Error::Code("no_session")) };
    let name = name_of(&id);
    match with(|c| c.send_invite(&id, &code)) {
        Ok(()) => done(&trf("Invite sent to %{name}.", &[("name", &name)])),
        Err(e) => fail(e),
    }
}

fn dismiss(args: &Value) -> Value {
    let key = str_arg(args, "id");
    let Some(id) = view().invites.iter().find(|i| i.key() == key).map(|i| i.id.clone()) else { return done("") };
    match with(|c| c.dismiss_invite(&id)) {
        // (gone from the view at once; the server's list follows at the next beat)
        Ok(()) | Err(Error::Code("not_found")) => {
            view().invites.retain(|i| i.key() != key);
            done("")
        }
        Err(e) => fail(e),
    }
}

/// A command of Omsi-Hub online, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    let id = || str_arg(args, "id").to_string();
    Some(Ok(match cmd {
        "ol_state" => state(),
        "ol_local" => {
            set_local(args.get("on").and_then(Value::as_bool).unwrap_or(true));
            state()
        }
        "ol_verify" => verify(args),
        "ol_resend" => answer(with(|c| c.send_verification(str_arg(args, "email"))).map(|_| done(&tr("A new code is on its way.")))),
        "ol_sign_in" => sign_in(args),
        "ol_forgot" => answer(with(|c| c.request_reset(str_arg(args, "email"))).map(|_| done(&tr("If an account has this address, a code is on its way to it.")))),
        "ol_reset" => reset(args),
        "ol_sign_out" => sign_out(),
        "ol_delete" => delete(args),
        "ol_profile" => profile(args),
        "ol_add" => friends_act(|c| c.send_friend_request(str_arg(args, "code")), |code| tr(codes::text(code))),
        "ol_respond" => {
            let accept = args.get("accept").and_then(Value::as_bool).unwrap_or(false);
            let name = name_of(&id());
            friends_act(|c| c.respond_friend_request(&id(), accept).map(|_| ""), |_| if accept { trf("You and %{name} are friends now.", &[("name", &name)]) } else { tr("Request declined.") })
        }
        "ol_remove" => {
            let name = name_of(&id());
            friends_act(|c| c.remove_friend(&id()).map(|_| ""), |_| trf("%{name} is no longer your friend.", &[("name", &name)]))
        }
        "ol_block" => {
            let name = name_of(&id());
            friends_act(|c| c.block_player(&id()).map(|_| ""), |_| trf("%{name} is blocked.", &[("name", &name)]))
        }
        "ol_invite" => invite(args),
        "ol_dismiss" => dismiss(args),
        "ol_presence" => {
            view().away = args.get("away").and_then(Value::as_bool).unwrap_or(false);
            json!({})
        }
        "ol_seen" => {
            let mut v = view();
            let keys: Vec<String> = v.friends.iter().filter(|f| f.relation == "pending").map(|f| format!("r:{}", f.user_id)).chain(v.invites.iter().map(|i| format!("i:{}", i.key()))).collect();
            v.seen.extend(keys);
            json!({})
        }
        "ol_refresh" => {
            // (the panel opened: at most every ten seconds)
            let recent = unix_now().saturating_sub(view().refreshed_at) < 10;
            if !recent && with(|c| c.signed_in()) {
                let _ = refresh(false);
            }
            state()
        }
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text of Omsi-Hub online - the codes, this file's, the page's (`online.js`) - in the
    /// six languages.
    #[test]
    fn every_online_text_is_translated() {
        let mut texts: Vec<String> = codes::TEXTS.iter().map(|(_, t)| t.to_string()).collect();
        let own = include_str!("online.rs").replace("\r\n", "\n");
        let own = own.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
        let js = include_str!("../../../../launcher/ui/js/omsihub/online.js");
        for (src, lead, end) in [(own, "tr(\"", '"'), (own, "trf(\"", '"'), (js, "tr('", '\'')] {
            let mut rest = src;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let mut t = String::new();
                let mut chars = after.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => t.extend(chars.next()),
                        c if c == end => break,
                        c => t.push(c),
                    }
                }
                if t.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                    texts.push(t);
                }
            }
        }
        assert!(texts.len() > 80, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsi-app/locales/online.yml):\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn the_servers_times_are_read() {
        assert_eq!(unix_of("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_of("2026-10-16T10:00:00+00:00"), Some(1_792_144_800));
        assert_eq!(unix_of("2026-10-16T12:00:00.123456+02:00"), Some(1_792_144_800));
        assert_eq!(unix_of("2026-10-16T09:30:00-0030"), Some(1_792_144_800));
        assert_eq!(unix_of("garbage"), None);
        assert_eq!(seen_words(None), "");
    }

    #[test]
    fn a_friends_state_is_one_of_four() {
        let f = |online, status: &str| Friend { online, status: status.into(), ..Default::default() };
        assert_eq!(state_of(&f(false, "busy")), "offline");
        assert_eq!(state_of(&f(true, "busy")), "busy");
        assert_eq!(state_of(&f(true, "away")), "away");
        assert_eq!(state_of(&f(true, "online")), "online");
        assert_eq!(state_of(&f(true, "offline")), "offline");
    }

    /// Without a network the commands answer at once with "offline", in the page's language,
    /// and what is typed wrong is said before anything is sent.
    #[test]
    fn offline_and_typos_are_said_in_the_pages_language() {
        let v = call("ol_sign_in", &json!({ "email": "not an address", "password": "12345678", "lang": "de" })).unwrap().unwrap();
        assert_eq!(v["code"], "invalid_email");
        assert_eq!(v["error"], crate::i18n::table("de")["That is not an email address."]);
        let t = std::time::Instant::now();
        let v = call("ol_sign_in", &json!({ "email": "a@b.example", "password": "12345678" })).unwrap().unwrap();
        assert_eq!(v["code"], "offline");
        assert!(t.elapsed().as_secs() < 8);
        let s = call("ol_state", &json!({})).unwrap().unwrap();
        assert_eq!(s["offline"], true);
        assert_ne!(s["mode"], "in");
        assert_eq!(call("ol_add", &json!({ "code": "AB12CD34" })).unwrap().unwrap()["code"], "not_signed_in");
        assert_eq!(call("ol_invite", &json!({ "id": "u2" })).unwrap().unwrap()["code"], "no_session");
        assert!(call("tg_nothing", &json!({})).is_none());
    }
}

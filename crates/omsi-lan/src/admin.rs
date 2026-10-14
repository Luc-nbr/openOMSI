//! A dedicated server's administration (see the app's `admin` for the whole picture): the
//! admins and their challenges, the commands players send the server (`server_command`),
//! and the pieces the host's own game menu shares with it - the password's answer, a duty's
//! argument, whom a notice is for.

use omsi_net::LanSession;

/// What of the game a dedicated server's administration goes by.
pub struct ServerRules<'a> {
    /// The server's clock follows the real time: no admin moves it or changes its speed.
    pub real_time: bool,
    /// The most AI traffic the game allows (`traffic <density>` is held to it).
    pub traffic_max: usize,
    /// Whether a notice (`notify`) is one a game can show.
    pub notice_ok: &'a dyn Fn(&str) -> bool,
}

/// A finite number (NaN passes `clamp` through and would stop everybody's clock).
pub fn finite(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub const KDF_ROUNDS: u32 = 600_000;
const MIN_ROUNDS: u32 = 100_000;
const MAX_ROUNDS: u32 = 5_000_000;
pub const MIN_PASSWORD: usize = 8;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

fn derive(password: &str, salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    let rounds = std::num::NonZeroU32::new(rounds.max(1)).expect("not zero");
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, rounds, salt, password.as_bytes(), &mut key);
    key
}

fn keyed_response(key: &[u8; 32], challenge: &str) -> String {
    let mut ctx = ring::hmac::Context::with_key(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key));
    ctx.update(b"omsi2rw-admin");
    ctx.update(challenge.as_bytes());
    hex(ctx.sign().as_ref())
}

pub fn answer(challenge: &str, password: &str) -> Option<String> {
    let mut it = challenge.split_whitespace();
    let (c, salt, rounds) = (it.next()?, it.next()?, it.next()?.parse::<u32>().ok()?);
    if it.next().is_some() || c.len() > 64 || !(MIN_ROUNDS..=MAX_ROUNDS).contains(&rounds) {
        return None;
    }
    let salt = unhex(salt).filter(|s| (16..=64).contains(&s.len()))?;
    Some(keyed_response(&derive(password, &salt, rounds), c))
}

fn fresh_salt() -> [u8; 16] {
    use ring::rand::SecureRandom;
    let mut salt = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut salt).is_err() {
        salt[..8].copy_from_slice(&omsi_net::random_session_id().to_le_bytes());
        salt[8..].copy_from_slice(&omsi_net::random_session_id().to_le_bytes());
    }
    salt
}

/// Equal strings, compared in time independent of where they differ.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A point `side` metres to the right of a vehicle at `pos` facing `heading`.
pub fn beside(pos: glam::DVec3, heading: f64, side: f64) -> (f64, f64) {
    let h = heading.to_radians();
    (pos.x + h.cos() * side, pos.y - h.sin() * side)
}

/// `duty`'s argument: `<line> <tour> <trip> <stop>`, a space in the line or tour name written
/// `%20` (and `%` as `%25`).
pub fn duty_arg(line: &str, tour: &str, trip: usize, stop: usize) -> String {
    let esc = |s: &str| s.trim().replace('%', "%25").replace(' ', "%20");
    format!("{} {} {trip} {stop}", esc(line), esc(tour))
}

/// `duty_arg` read back: (line, tour, trip, stop).
pub fn parse_duty(arg: &str) -> Option<(String, String, usize, usize)> {
    let unesc = |s: &str| s.replace("%20", " ").replace("%25", "%");
    let v: Vec<&str> = arg.split_whitespace().collect();
    let [line, tour, trip, stop] = v[..] else { return None };
    Some((unesc(line), unesc(tour), trip.parse().ok()?, stop.parse().ok()?))
}

/// Who a `notify` is for: `all`, every other player and the host's own screen; a player's
/// number, that player - or the host's own screen when it is the host's number (`peers` has
/// only the others, and `LanSession::command` sends nothing to oneself).
pub fn notice_targets(who: &str, others: impl Iterator<Item = u32>, my_id: u32) -> (Vec<u32>, bool) {
    if who == "all" {
        return (others.filter(|id| *id != my_id).collect(), true);
    }
    match who.trim().parse::<u32>() {
        Ok(id) if id == my_id => (Vec::new(), true),
        Ok(id) => (vec![id], false),
        Err(_) => (Vec::new(), false),
    }
}

/// The administration of a dedicated server (`omsi --server`): what it keeps.
#[derive(Default)]
pub struct ServerAdmin {
    pub password: String,
    pub admins: std::collections::HashSet<u32>,
    /// The clock moved by an admin (s).
    pub shift: f64,
    /// An admin asked for the next weather.
    pub next_weather: bool,
    /// An admin set the clock to this time of day (s).
    pub set_clock: Option<f64>,
    /// An admin chose this weather (`Weather/….owt`, checked against the installed ones by
    /// the host loop).
    pub set_weather: Option<String>,
    /// An admin's traffic order, for the host loop (`traffic <density>`, `traffic clear`).
    pub traffic: Option<TrafficOrder>,
    /// The challenge each asking player was given (used once).
    challenges: std::collections::HashMap<u32, String>,
    /// When wrong answers came lately (the lock counts them, whoever sent them: a player
    /// who reconnects is somebody new).
    failures: Vec<std::time::Instant>,
    kdf: Option<(String, [u8; 16], [u8; 32])>,
    admin_at: std::collections::HashMap<u32, Option<std::net::SocketAddr>>,
}

/// A dedicated server admin's order for the AI traffic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrafficOrder {
    Density(usize),
    Clear,
}

impl TrafficOrder {
    /// `clear`, or a density (held to 0 .. the top of the game menu's Traffic steps, the
    /// same limit as server.cfg's `traffic`: it was 100 here, 120 in the menu, #1327).
    pub fn parse(arg: &str, max: usize) -> Option<TrafficOrder> {
        match arg.trim() {
            "clear" => Some(TrafficOrder::Clear),
            v => v.parse::<usize>().ok().map(|n| TrafficOrder::Density(n.min(max))),
        }
    }
}

/// Wrong answers within `LOCK_WINDOW` that lock the administration for everybody.
const LOCK_AFTER: usize = 5;
const LOCK_WINDOW: std::time::Duration = std::time::Duration::from_secs(120);

impl ServerAdmin {
    /// Forget the rights and challenges of players no longer in the session.
    pub fn prune(&mut self, lan: &LanSession) {
        let here: std::collections::HashMap<u32, Option<std::net::SocketAddr>> = lan.peers().map(|p| (p.pose.id, p.addr)).collect();
        let at = &self.admin_at;
        self.admins.retain(|id| {
            let moved = matches!((at.get(id), here.get(id)), (Some(a), Some(h)) if a != h);
            if moved {
                log::warn!("server: player {id} talks from another address now: no longer an admin until the password is given again");
            }
            here.contains_key(id) && !moved
        });
        let admins = &self.admins;
        self.admin_at.retain(|id, _| admins.contains(id));
        self.challenges.retain(|id, _| here.contains_key(id));
    }

    fn derived(&mut self) -> ([u8; 16], [u8; 32]) {
        if self.kdf.as_ref().map(|k| &k.0) != Some(&self.password) {
            let salt = fresh_salt();
            self.kdf = Some((self.password.clone(), salt, derive(&self.password, &salt, KDF_ROUNDS)));
        }
        let k = self.kdf.as_ref().expect("just made");
        (k.1, k.2)
    }

    fn locked(&mut self) -> bool {
        self.failures.retain(|t| t.elapsed() < LOCK_WINDOW);
        self.failures.len() >= LOCK_AFTER
    }
}

/// A weather file an admin may choose: a `Weather/….owt` path, nothing above it.
fn weather_file_ok(file: &str) -> bool {
    let f = file.replace('\\', "/").to_ascii_lowercase();
    f.starts_with("weather/") && f.ends_with(".owt") && !f.contains("..") && f.matches('/').count() == 1
}

/// Who the commands of the web gateway's `POST /admin` come from: no player has this id.
pub const LOCAL_ADMIN: u32 = u32::MAX;

/// A command a player sent the dedicated server.
pub fn server_command(lan: &mut LanSession, from: u32, text: &str, adm: &mut ServerAdmin, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules) {
    let (verb, arg) = text.split_once(' ').unwrap_or((text, ""));
    match verb {
        // a joining game asks which voice server the session talks on (`voice`)
        "voice?" => lan.command(from, &crate::voice::VoiceServer::command(crate::voice::hosted().as_ref())),
        "auth?" => {
            if adm.password.is_empty() {
                lan.command(from, "admin-no");
            } else if adm.locked() {
                lan.command(from, "admin-locked");
            } else {
                let c = format!("{:016x}{:016x}", omsi_net::random_session_id(), omsi_net::random_session_id());
                let (salt, _) = adm.derived();
                adm.challenges.insert(from, c.clone());
                lan.command(from, &format!("admin-challenge {c} {} {KDF_ROUNDS}", hex(&salt)));
            }
        }
        "auth" => {
            let challenge = adm.challenges.remove(&from);
            if adm.locked() {
                lan.command(from, "admin-locked");
            } else if let (false, Some(c)) = (adm.password.is_empty(), challenge) {
                if same(arg.trim(), &keyed_response(&adm.derived().1, &c)) {
                    adm.admins.insert(from);
                    adm.admin_at.insert(from, lan.peers().find(|p| p.pose.id == from).and_then(|p| p.addr));
                    log::info!("server: player {from} administers the server now");
                    lan.command(from, "admin-ok");
                } else {
                    adm.failures.push(std::time::Instant::now());
                    log::warn!("server: player {from} gave a wrong admin password");
                    lan.command(from, "admin-no");
                }
            } else {
                lan.command(from, "admin-no");
            }
        }
        "admin" if adm.admins.contains(&from) => {
            let (v, a) = arg.split_once(' ').unwrap_or((arg, ""));
            let id = a.trim().parse::<u32>().ok();
            log::info!("server: admin {from}: {arg}");
            match v {
                "kick" | "ban" => {
                    // (`kick <id> [reason]`: the reason is what the player reads when the game closes)
                    let why = a.trim().split_once(' ').map(|x| x.1.trim()).filter(|r| !r.is_empty());
                    if let Some(id) = a.trim().split(' ').next().and_then(|x| x.parse::<u32>().ok()) {
                        lan.kick(id, why.unwrap_or(if v == "ban" { "sent away for this session" } else { "sent away by an admin" }), v == "ban");
                    }
                }
                "bring" => {
                    if let (Some(id), Some((pos, h))) = (id, positions(from)) {
                        let (x, y) = beside(pos, h, 8.0);
                        lan.command(id, &format!("teleport {x:.2} {y:.2} {:.2} {h:.1}", pos.z));
                    }
                }
                "goto" => {
                    if let Some((pos, h)) = id.and_then(positions) {
                        let (x, y) = beside(pos, h, 8.0);
                        lan.command(from, &format!("teleport {x:.2} {y:.2} {:.2} {h:.1}", pos.z));
                    }
                }
                // (a server on the real time keeps its clock and its speed)
                "time" if !rules.real_time => {
                    if let Some(s) = finite(a) {
                        adm.shift += s.clamp(-86400.0, 86400.0);
                    }
                }
                "speed" if !rules.real_time => {
                    if let Some(s) = finite(a) {
                        lan.clock_speed = s.clamp(1.0, 30.0);
                    }
                }
                // the menu offers "weather next" and "weather set <file>" for each installed
                // weather; a server took every one of them for "next"
                "weather" => match a.trim().split_once(' ').map(|(k, f)| (k, f.trim())) {
                    Some(("set", file)) if weather_file_ok(file) => adm.set_weather = Some(file.replace('\\', "/")),
                    _ => adm.next_weather = true,
                },
                "say" => {
                    let _ = lan.say(a);
                }
                // a word for one player only: `tell <id> <text>`, a chat line from "Admin
                // (private)" that the others do not get
                "tell" => {
                    if let Some((who, msg)) = a.trim().split_once(' ') {
                        if let Ok(id) = who.parse::<u32>() {
                            if let Err(e) = lan.say_to(id, "Admin (private)", msg.trim()) {
                                log::info!("server: tell {id}: {e}");
                            }
                        }
                    }
                }
                // a duty for one player: `duty <id> <line> <tour> <trip> <stop>` (see `command`)
                "duty" => {
                    if let Some((who, rest)) = a.trim().split_once(' ') {
                        if let (Ok(id), Some(_)) = (who.parse::<u32>(), parse_duty(rest)) {
                            lan.command(id, &format!("duty {}", rest.trim()));
                        }
                    }
                }
                // a notification on one player's screen, or everybody's: `notify <id|all> <notice
                // id> <seconds> <info|warn|alert> <text>` (a game that shows it answers
                // `notify-seen <notice id>`)
                // (a dedicated server has no screen: its own number is nobody's)
                "notify" => {
                    if let Some((who, rest)) = a.trim().split_once(' ') {
                        if (rules.notice_ok)(rest) {
                            let (ids, _) = notice_targets(who, lan.peers().map(|p| p.pose.id), lan.my_id);
                            for id in ids {
                                lan.command(id, &format!("notify {}", rest.trim()));
                            }
                        }
                    }
                }
                // the duty taken back from a player: `duty-off <id>` (see `command`)
                "duty-off" => {
                    if let Some(id) = id {
                        lan.command(id, "duty-off");
                    }
                }
                // the AI traffic: `traffic <density>` (as server.cfg's `traffic`) or `traffic
                // clear` (every AI car off the road, a jam; the timetable's buses stay)
                "traffic" => {
                    if let Some(o) = TrafficOrder::parse(a, rules.traffic_max) {
                        adm.traffic = Some(o);
                    }
                }
                "bringall" => {
                    if let Some((pos, h)) = positions(from) {
                        let ids: Vec<u32> = lan.peers().map(|p| p.pose.id).filter(|id| *id != from && *id != lan.my_id).collect();
                        for (k, id) in ids.iter().enumerate() {
                            let (x, y) = beside(pos, h, 5.0 * (k as f64 + 1.0));
                            lan.command(*id, &format!("teleport {x:.2} {y:.2} {:.2} {h:.1}", pos.z));
                        }
                    }
                }
                "service" => {
                    let (kind, who) = a.split_once(' ').unwrap_or((a, "all"));
                    if matches!(kind, "repair" | "refuel" | "wash") {
                        let ids: Vec<u32> = if who == "all" { lan.peers().map(|p| p.pose.id).filter(|id| *id != lan.my_id).collect() } else { who.trim().parse::<u32>().ok().into_iter().collect() };
                        for id in ids {
                            lan.command(id, &format!("service {kind}"));
                        }
                    }
                }
                "unstick" => {
                    if let Some(id) = id {
                        lan.command(id, "unstick");
                    }
                }
                "clock" if !rules.real_time => {
                    // (the server's clock is the session's: moved by the difference)
                    if let Some(s) = finite(a) {
                        adm.set_clock = Some(s.rem_euclid(86400.0));
                    }
                }
                _ => {}
            }
        }
        // a player's game took the duty it was given (`duty`), or could not: said for the tool
        // that gave it
        "duty-off-ok" => log::info!("server: player {from} left the duty"),
        "duty-off-none" => log::info!("server: player {from} had no duty to leave"),
        "duty-ok" => log::info!("server: player {from} took duty {}", arg.trim()),
        "duty-no" => log::info!("server: player {from} could not take the duty: {}", arg.trim()),
        // a player's game showed a notification (`notify`): said for the tool that sent it
        "notify-seen" => log::info!("server: player {from} saw notice {}", arg.trim()),
        _ => log::info!("server: command '{text}' from player {from} not taken"),
    }
}

#[cfg(test)]
mod password_tests {
    use super::*;

    #[test]
    fn the_answer_needs_the_password_and_a_slow_key() {
        let salt = "00112233445566778899aabbccddeeff";
        let a = answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "correct horse").unwrap();
        assert_eq!(a, keyed_response(&derive("correct horse", &unhex(salt).unwrap(), MIN_ROUNDS), "c0ffee"));
        assert_ne!(answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "wrong horse"), Some(a));
        assert_eq!(answer(&format!("c0ffee {salt} 1"), "correct horse"), None);
        assert_eq!(answer("c0ffee", "correct horse"), None);
        assert_eq!(answer(&format!("c0ffee 00 {MIN_ROUNDS}"), "correct horse"), None);
        assert_eq!(answer(&format!("c0ffee {salt} {}", MAX_ROUNDS + 1), "correct horse"), None);
    }

    #[test]
    fn the_server_keeps_its_key_until_the_password_changes() {
        let mut adm = ServerAdmin { password: "a long password".into(), ..Default::default() };
        let first = adm.derived();
        assert_eq!(adm.derived(), first);
        adm.password = "another long one".into();
        assert_ne!(adm.derived(), first);
    }
}

#[cfg(test)]
mod duty_tests {
    use super::{duty_arg, parse_duty};

    #[test]
    fn a_duty_goes_to_the_game_and_back() {
        assert_eq!(duty_arg("15", "5", 2, 7), "15 5 2 7");
        assert_eq!(parse_duty("15 5 2 7"), Some(("15".into(), "5".into(), 2, 7)));
        // names with spaces and percent signs
        let a = duty_arg("KI-Zug", "ZOB RB 100%", 0, 0);
        assert_eq!(a, "KI-Zug ZOB%20RB%20100%25 0 0");
        assert_eq!(parse_duty(&a), Some(("KI-Zug".into(), "ZOB RB 100%".into(), 0, 0)));
        // missing or extra parts, not numbers: not a duty
        for bad in ["", "15", "15 5", "15 5 2", "15 5 2 7 9", "15 5 x 7", "15 5 2 -1"] {
            assert_eq!(parse_duty(bad), None, "{bad}");
        }
    }
}

#[cfg(test)]
mod notice_target_tests {
    use super::notice_targets;

    #[test]
    fn all_is_every_player_and_the_host_itself() {
        // the host is player 1, the others 2 and 5
        assert_eq!(notice_targets("all", [2, 5].into_iter(), 1), (vec![2, 5], true));
        assert_eq!(notice_targets("all", [].into_iter(), 1), (vec![], true));
        // one player; the host's own number: its own screen, nothing sent
        assert_eq!(notice_targets("5", [2, 5].into_iter(), 1), (vec![5], false));
        assert_eq!(notice_targets("1", [2, 5].into_iter(), 1), (vec![], true));
        // not a number: nobody
        assert_eq!(notice_targets("x", [2, 5].into_iter(), 1), (vec![], false));
    }
}

#[cfg(test)]
mod weather_file_tests {
    use super::weather_file_ok;

    #[test]
    fn only_a_weather_file() {
        assert!(weather_file_ok("Weather/#CAVOK.owt"));
        assert!(weather_file_ok("weather\\Bodennebel.OWT"));
        assert!(!weather_file_ok("Weather/../server.cfg"));
        assert!(!weather_file_ok("Weather/sub/x.owt"));
        assert!(!weather_file_ok("maps/x.owt"));
        assert!(!weather_file_ok("Weather/x.cfg"));
    }
}

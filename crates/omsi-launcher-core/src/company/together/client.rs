//! A friend's launcher's way to the boss's company server: the addresses of the code tried all at
//! once (the first that answers is taken), then the boss's tunnel as he posted it on the game's
//! relay under the code's session (`omsi_net::bridge::lookup_tunnel`, signed with his key); a
//! message posted there, its answer read. A link that stops answering is found again the same
//! way (`connect`), so a boss's launcher started anew, a new tunnel or a new address is followed.
//!
//! How the session stands for the friend (`Link`, Beta 63): the boss closed it (he says so to
//! every message: `host::CLOSED`), the boss's launcher is lost (not heard from for `HOST_LOST`),
//! or he was sent away; his launcher then stops asking and his page says so.

use super::code::CompanyCode;
use super::host;
use super::wire::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::mpsc;
use std::time::Duration;

/// How long one message may take (the tunnel goes round the world).
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// How long an address of the code is tried.
const TRY_FOR: Duration = Duration::from_secs(3);

/// Why the boss's launcher is not reached (English: a key of the translations).
pub const UNREACHED: &str = "The boss's launcher does not answer. Is his \"Play together\" session still on? Over the internet both need the same VPN, or his tunnel must be up.";

/// The address of a company server's messages at `base` (`http://ip:port`, `https://…`).
pub fn url_of(base: &str) -> String {
    format!("{}/together", base.trim_end_matches('/'))
}

/// Post `req` to the server at `base`; its answer.
pub fn send(base: &str, req: &Request, timeout: Duration) -> Result<Value, String> {
    let body = serde_json::to_string(req).map_err(|e| e.to_string())?;
    let text = omsi_net::http::post_json(&url_of(base), &body, timeout)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Find the boss's server for `code` and post `req` (a hello) there: where it answered, and its
/// answer. The code's addresses first (all at once), then the tunnel.
pub fn connect(code: &CompanyCode, req: &Request) -> Result<(String, Value), String> {
    let bases: Vec<String> = code.addrs().iter().map(|a| format!("http://{a}")).collect();
    if !bases.is_empty() {
        let (tx, rx) = mpsc::channel();
        for b in &bases {
            let (tx, b, req) = (tx.clone(), b.clone(), req.clone());
            let _ = std::thread::Builder::new().name("company join".into()).spawn(move || {
                if let Ok(v) = send(&b, &req, TRY_FOR) {
                    let _ = tx.send((b, v));
                }
            });
        }
        drop(tx);
        if let Ok(found) = rx.recv_timeout(TRY_FOR + Duration::from_millis(500)) {
            return Ok(found);
        }
    }
    // (none answered: the tunnel the boss posted)
    if let Some(url) = omsi_net::bridge::lookup_tunnel(code.session) {
        if let Ok(v) = send(&url, req, TIMEOUT) {
            return Ok((url, v));
        }
    }
    Err(UNREACHED.to_string())
}

/// After this long without an answer of the boss's launcher it is lost (s).
pub const HOST_LOST: u64 = 45;

/// How the session ended for a friend.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ended {
    /// The boss closed it: he stopped sharing, closed the company or quit his launcher.
    Closed,
    /// The boss's launcher has not answered for `HOST_LOST` (gone, or the network): joined again
    /// with the same code, when it is back.
    Lost,
    /// The boss (or a player who may) sent him away from the session, or removed him.
    Removed,
}

/// A friend's session with the boss's launcher as it stands: when it last answered (Unix seconds),
/// the company's name, and how it ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    pub since: u64,
    pub answered: Option<u64>,
    pub company: String,
    pub ended: Option<Ended>,
}

impl Link {
    pub fn new(now: u64) -> Link {
        Link { since: now, ..Default::default() }
    }

    /// An answer of the boss's launcher taken in (at `now`): how the session ended, if this ends
    /// it. A refusal that passes (the timetable being read, the day simulated) counts as heard.
    pub fn answer(&mut self, v: &Value, now: u64) -> Option<Ended> {
        if self.ended.is_some() {
            return self.ended;
        }
        if let Some(name) = v.get("snapshot").and_then(|s| s.get("card")).and_then(|c| c.get("name")).and_then(Value::as_str) {
            self.company = name.to_string();
        }
        let error = v.get("error").and_then(Value::as_str).unwrap_or("");
        match error {
            host::CLOSED => {
                if let Some(name) = v.get("company").and_then(Value::as_str).filter(|n| !n.is_empty()) {
                    self.company = name.to_string();
                }
                self.ended = Some(Ended::Closed);
            }
            host::KICKED | host::BANNED => self.ended = Some(Ended::Removed),
            _ if v.get("snapshot").is_some() || host::passing(error) || error.is_empty() => self.answered = Some(now),
            // (a code no longer valid, another version: not heard, as no answer)
            _ => return self.failed(now),
        }
        self.ended
    }

    /// A message that got no answer (at `now`): the boss's launcher is lost once it has not
    /// answered for `HOST_LOST`.
    pub fn failed(&mut self, now: u64) -> Option<Ended> {
        if self.ended.is_none() && now.saturating_sub(self.answered.unwrap_or(self.since)) >= HOST_LOST {
            self.ended = Some(Ended::Lost);
        }
        self.ended
    }
}

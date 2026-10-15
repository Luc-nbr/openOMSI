//! A friend's launcher's way to the boss's company server: the addresses of the code tried all at
//! once (the first that answers is taken), then the boss's tunnel as he posted it on the game's
//! relay under the code's session (`omsi_net::bridge::lookup_tunnel`, signed with his key); a
//! message posted there, its answer read. A link that stops answering is found again the same
//! way (`connect`), so a boss's launcher started anew, a new tunnel or a new address is followed.

use super::code::CompanyCode;
use super::wire::Request;
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

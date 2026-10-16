//! Signing in with Google, Discord or Steam: the system's browser does the provider's sign-in and
//! hands the result back to a one-shot listener on this computer (`http://127.0.0.1:<port>/callback`,
//! a free port chosen by the system), never to a page of ours on the web.
//!
//! * Google and Discord are Supabase Auth's own providers, with PKCE: the browser goes to
//!   `/auth/v1/authorize?provider=…&redirect_to=<listener>?state=<state>&code_challenge=…`, comes
//!   back with `?code=…&state=…`, and the code is exchanged here with the verifier
//!   (`Client::sign_in_with`, `POST /token?grant_type=pkce`). A code without its verifier is
//!   worth nothing, and an answer whose `state` is not this flow's is turned away.
//! * Steam goes through the Edge Function `steam-auth` (`?to=<listener>`): its one-time link makes
//!   Supabase hand the session over in the address's fragment (`#access_token=…&refresh_token=…`),
//!   which never reaches a server. The listener's page reads it with a few lines of script and posts
//!   it back, with this flow's `state`; the refresh token is then renewed at once with the auth
//!   (which checks it) and only that answer is kept.
//!
//! The listener answers only `127.0.0.1:<its port>` (the `Host` checked), shows the player a small
//! page in the launcher's language - signed in, or what went wrong - and stops at the first answer,
//! after a few minutes, or when the player cancels. Nothing of it is logged.

use crate::codes::Error;
use crate::http::encode;
use crate::Endpoints;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long the browser has for the sign-in.
pub const TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The listener's path (`steam-auth` accepts this one only).
pub const PATH: &str = "/callback";
/// The longest request the listener reads (the fragment with two tokens fits well).
const MAX_REQUEST: usize = 16 * 1024;

/// Where the player signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    Discord,
    Steam,
}

impl Provider {
    pub fn of(name: &str) -> Option<Self> {
        match name {
            "google" => Some(Self::Google),
            "discord" => Some(Self::Discord),
            "steam" => Some(Self::Steam),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Discord => "discord",
            Self::Steam => "steam",
        }
    }

    /// Its name as the player knows it (a brand: never translated).
    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Discord => "Discord",
            Self::Steam => "Steam",
        }
    }
}

/// The listener's page in the launcher's language (`lang` its HTML language).
#[derive(Debug, Clone)]
pub struct Texts {
    pub lang: String,
    pub done_title: String,
    pub done_text: String,
    pub failed_title: String,
    pub failed_text: String,
    pub cancelled_text: String,
    pub working: String,
    pub no_script: String,
}

impl Default for Texts {
    fn default() -> Self {
        Self {
            lang: "en".into(),
            done_title: "You are signed in to Omsi-Hub".into(),
            done_text: "You may close this tab and return to Omsi-Hub.".into(),
            failed_title: "The sign-in could not be completed".into(),
            failed_text: "Please return to Omsi-Hub and try again.".into(),
            cancelled_text: "The sign-in was cancelled. Please return to Omsi-Hub.".into(),
            working: "Completing the sign-in…".into(),
            no_script: "Please enable JavaScript for this page to complete the sign-in.".into(),
        }
    }
}

/// What the browser handed back, for `Client::sign_in_with`.
#[derive(Clone, PartialEq, Eq)]
pub enum Handoff {
    /// Supabase's PKCE code and this flow's verifier.
    Code { code: String, verifier: String },
    /// A session's refresh token (Steam's: renewed with the auth before it is kept).
    Refresh(String),
}

// (no Debug with the secrets in it)
impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Handoff::Code { .. } => "Handoff::Code",
            Handoff::Refresh(_) => "Handoff::Refresh",
        })
    }
}

/// One sign-in in the browser: the listener, and the address to open.
pub struct Flow {
    listener: TcpListener,
    port: u16,
    state: String,
    verifier: String,
    provider: Provider,
    url: String,
    texts: Texts,
}

impl std::fmt::Debug for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow").field("port", &self.port).field("provider", &self.provider).finish_non_exhaustive()
    }
}

impl Flow {
    /// The listener started (a free port on 127.0.0.1), the state and the PKCE verifier made.
    pub fn start(ep: &Endpoints, provider: Provider, texts: Texts) -> Result<Flow, Error> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| Error::Code("oauth_failed"))?;
        listener.set_nonblocking(true).map_err(|_| Error::Code("oauth_failed"))?;
        let port = listener.local_addr().map_err(|_| Error::Code("oauth_failed"))?.port();
        let (state, verifier) = (random(16)?, random(32)?);
        let back = format!("http://127.0.0.1:{port}{PATH}");
        let url = match provider {
            Provider::Steam => format!("{}?to={}", ep.functions("/steam-auth"), encode(&back)),
            _ => format!(
                "{}?provider={}&redirect_to={}&code_challenge={}&code_challenge_method=s256",
                ep.auth("/authorize"),
                provider.id(),
                encode(&format!("{back}?state={state}")),
                challenge(&verifier)
            ),
        };
        Ok(Flow { listener, port, state, verifier, provider, url, texts })
    }

    /// The address to open in the system's browser.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// Waits for the browser: the handoff, or `oauth_cancelled` (the player said no there, or
    /// `cancel` was set here), `oauth_timeout` after `timeout`, `oauth_failed` (the provider or the
    /// function said something went wrong).
    pub fn wait(self, timeout: Duration, cancel: &AtomicBool) -> Result<Handoff, Error> {
        let end = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Code("oauth_cancelled"));
            }
            if Instant::now() >= end {
                return Err(Error::Code("oauth_timeout"));
            }
            match self.listener.accept() {
                Ok((stream, peer)) if peer.ip().is_loopback() => {
                    if let Some(r) = self.serve(stream) {
                        return r;
                    }
                }
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    /// One request: Some(the result) when it ended the flow, None to wait on.
    fn serve(&self, mut stream: TcpStream) -> Option<Result<Handoff, Error>> {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let Some(req) = read_request(&mut stream) else {
            reply(&mut stream, 400, "text/plain", "Bad request");
            return None;
        };
        // (only this listener's own address: a page that rebinds a name to 127.0.0.1 is turned away)
        if req.host != format!("127.0.0.1:{}", self.port) {
            reply(&mut stream, 400, "text/plain", "Bad request");
            return None;
        }
        let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
        if path != PATH {
            reply(&mut stream, 404, "text/plain", "Not found");
            return None;
        }
        match req.method.as_str() {
            "GET" => {
                let q = form(query);
                let state = value(&q, "state");
                if let Some(code) = value(&q, "code").filter(|c| !c.is_empty()) {
                    if !same(state.unwrap_or(""), &self.state) {
                        self.page(&mut stream, Shown::Failed);
                        return None;
                    }
                    self.page(&mut stream, Shown::Done);
                    return Some(Ok(Handoff::Code { code: code.to_string(), verifier: self.verifier.clone() }));
                }
                if value(&q, "error").is_some() {
                    if !same(state.unwrap_or(""), &self.state) {
                        self.page(&mut stream, Shown::Failed);
                        return None;
                    }
                    let e = error_of(&q);
                    self.page(&mut stream, if e == Error::Code("oauth_cancelled") { Shown::Cancelled } else { Shown::Failed });
                    return Some(Err(e));
                }
                // (nothing in the query: the session, or an error, is in the fragment)
                self.page(&mut stream, Shown::Relay);
                None
            }
            "POST" => {
                // (from the relay page only: the same origin, this flow's state)
                if req.origin.as_deref().is_some_and(|o| o != format!("http://127.0.0.1:{}", self.port)) {
                    reply(&mut stream, 403, "text/plain", "Forbidden");
                    return None;
                }
                let f = form(&String::from_utf8_lossy(&req.body));
                if !same(value(&f, "state").unwrap_or(""), &self.state) {
                    reply(&mut stream, 403, "text/plain", "Forbidden");
                    return None;
                }
                let h = form(value(&f, "hash").unwrap_or("").trim_start_matches('#'));
                if let Some(t) = value(&h, "refresh_token").filter(|t| !t.is_empty()) {
                    self.page(&mut stream, Shown::Done);
                    return Some(Ok(Handoff::Refresh(t.to_string())));
                }
                let e = error_of(&h);
                self.page(&mut stream, if e == Error::Code("oauth_cancelled") { Shown::Cancelled } else { Shown::Failed });
                Some(Err(e))
            }
            _ => {
                reply(&mut stream, 405, "text/plain", "Method not allowed");
                None
            }
        }
    }

    fn page(&self, stream: &mut TcpStream, shown: Shown) {
        let t = &self.texts;
        let (title, text, ok) = match shown {
            Shown::Done => (t.done_title.as_str(), t.done_text.as_str(), true),
            Shown::Failed => (t.failed_title.as_str(), t.failed_text.as_str(), false),
            Shown::Cancelled => (t.failed_title.as_str(), t.cancelled_text.as_str(), false),
            Shown::Relay => (t.working.as_str(), "", true),
        };
        let relay = matches!(shown, Shown::Relay);
        let mark = if relay {
            ""
        } else if ok {
            r##"<svg viewBox="0 0 24 24" width="40" height="40" aria-hidden="true"><circle cx="12" cy="12" r="11" fill="#f58620"/><path d="M7 12.5l3.2 3.2L17.5 8.5" fill="none" stroke="#181006" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##
        } else {
            r##"<svg viewBox="0 0 24 24" width="40" height="40" aria-hidden="true"><circle cx="12" cy="12" r="11" fill="#e5484d"/><path d="M8.5 8.5l7 7M15.5 8.5l-7 7" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round"/></svg>"##
        };
        let script = if relay {
            // (the fragment posted back with the state; first taken out of the address and the history)
            format!(
                r#"<form id="f" method="post" action="{PATH}"><input type="hidden" name="state" value="{state}"><input type="hidden" name="hash" id="h"></form><script>var h=location.hash;history.replaceState(null,"","{PATH}");document.getElementById("h").value=h;document.getElementById("f").submit();</script><noscript><p>{ns}</p></noscript>"#,
                state = self.state,
                ns = esc(&t.no_script)
            )
        } else {
            String::new()
        };
        let body = format!(
            r#"<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Omsi-Hub</title><style>{CSS}</style></head><body><main><p class="brand">Omsi-Hub</p>{mark}<h1>{title}</h1>{text}{script}</main></body></html>"#,
            lang = esc(&t.lang),
            title = esc(title),
            text = if text.is_empty() { String::new() } else { format!("<p>{}</p>", esc(text)) },
        );
        reply(stream, 200, "text/html; charset=utf-8", &body);
    }
}

enum Shown {
    Done,
    Failed,
    Cancelled,
    /// The page that posts the fragment back.
    Relay,
}

const CSS: &str = "html{color-scheme:dark}body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#0c1018;color:#e8ebf2;font:15px/1.5 system-ui,-apple-system,'Segoe UI',sans-serif}main{width:min(420px,calc(100% - 32px));padding:28px 24px;border-radius:14px;background:#141a26;border:1px solid #232b3b;text-align:center;box-shadow:0 18px 40px rgba(0,0,0,.4)}.brand{margin:0 0 16px;font-weight:800;letter-spacing:.02em;color:#f58620}h1{margin:14px 0 6px;font-size:18px}p{margin:0;color:#959db0}";

/// The provider's, Supabase's or `steam-auth`'s `error` as ours: the player said no, or something
/// else (`steam-auth` says `access_denied` too, with its `error_code=steam_failed`).
fn error_of(answer: &[(String, String)]) -> Error {
    let said_no = value(answer, "error") == Some("access_denied") && value(answer, "error_code") != Some("steam_failed");
    Error::Code(if said_no { "oauth_cancelled" } else { "oauth_failed" })
}

struct Request {
    method: String,
    target: String,
    host: String,
    origin: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_REQUEST {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = std::str::from_utf8(&buf[..head_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    let (mut host, mut origin, mut length) = (String::new(), None, 0usize);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "host" => host = v.to_string(),
            "origin" => origin = Some(v.to_string()),
            "content-length" => length = v.parse().ok()?,
            _ => {}
        }
    }
    if length > MAX_REQUEST {
        return None;
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(Request { method, target, host, origin, body })
}

fn reply(stream: &mut TcpStream, status: u16, kind: &str, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Method Not Allowed",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body.as_bytes())).and_then(|_| stream.flush());
}

/// `a=1&b=2` (a query, a form, a fragment) as pairs, percent-decoded.
fn form(s: &str) -> Vec<(String, String)> {
    s.split('&').filter(|p| !p.is_empty()).map(|p| {
        let (k, v) = p.split_once('=').unwrap_or((p, ""));
        (decode(k), decode(v))
    }).collect()
}

fn value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h << 4 | l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// The same text, compared in a time that does not tell how much of it matched.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `n` random bytes from the system's generator, base64url without padding.
fn random(n: usize) -> Result<String, Error> {
    use ring::rand::SecureRandom;
    let mut b = vec![0u8; n];
    ring::rand::SystemRandom::new().fill(&mut b).map_err(|_| Error::Code("oauth_failed"))?;
    Ok(b64url(&b))
}

/// PKCE's S256: base64url(sha256(verifier)), RFC 7636.
pub(crate) fn challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(b.len() * 4 / 3 + 2);
    for ch in b.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for k in 0..=ch.len() {
            s.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_rfc_7636s() {
        // (RFC 7636, appendix B)
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let v = random(32).unwrap();
        assert_eq!(v.len(), 43);
        assert!(v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        assert_ne!(v, random(32).unwrap());
    }

    #[test]
    fn forms_are_decoded() {
        let f = form("a=1%2B2&b=x+y&c&d=%zz&e=%E2%9C%93");
        assert_eq!(value(&f, "a"), Some("1+2"));
        assert_eq!(value(&f, "b"), Some("x y"));
        assert_eq!(value(&f, "c"), Some(""));
        assert_eq!(value(&f, "d"), Some("%zz"));
        assert_eq!(value(&f, "e"), Some("✓"));
        assert_eq!(decode("%4"), "%4");
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        assert_eq!(esc(r#"<a href="x">&"#), "&lt;a href=&quot;x&quot;&gt;&amp;");
    }
}

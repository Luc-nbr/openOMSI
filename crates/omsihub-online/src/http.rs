//! The requests: JSON in and out over HTTPS (rustls, the certificates checked against the
//! Mozilla roots), no redirects followed (a token goes nowhere else), short timeouts. An answer
//! whatever its status comes back as a `Reply`; no answer at all is `Error::Offline`.

use crate::codes::Error;
use serde_json::Value;
use std::time::Duration;

/// An answer: its status, its JSON (Null when it had none) and the auth's headers we read.
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    pub status: u16,
    pub body: Value,
    /// Better Auth's bearer plugin: the session token, signed (`set-auth-token`).
    pub auth_token: Option<String>,
    /// Its JWT plugin: a fresh JWT beside the session (`set-auth-jwt`).
    pub auth_jwt: Option<String>,
    /// The session cookie's value, where the auth set one (`…session_token=`).
    pub cookie_token: Option<String>,
}

impl Reply {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[derive(Clone)]
pub(crate) struct Http {
    agent: ureq::Agent,
}

/// The longest an answer may be (the friends list is the largest: 500 rows).
const MAX_BODY: usize = 2 << 20;

/// How long a cloud save's transfer may take (25 MB on a slow line).
const TRANSFER: Duration = Duration::from_secs(300);

impl Http {
    pub fn new(https_only: bool) -> Self {
        let agent = ureq::AgentBuilder::new()
            .https_only(https_only)
            .redirects(0)
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(12))
            .user_agent(concat!("Omsi-Hub/", env!("CARGO_PKG_VERSION")))
            .build();
        Self { agent }
    }

    /// One request; `bearer` goes as `Authorization: Bearer`, `body` as JSON, `origin` as `Origin`. `quick` keeps it
    /// to a few seconds (the goodbye as the launcher closes).
    pub fn send(&self, method: &str, url: &str, bearer: Option<&str>, body: Option<&Value>, origin: Option<&str>, quick: bool) -> Result<Reply, Error> {
        let mut req = self.agent.request(method, url).set("Accept", "application/json");
        if quick {
            req = req.timeout(Duration::from_secs(3));
        }
        if let Some(t) = bearer {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        if let Some(o) = origin {
            req = req.set("Origin", o);
        }
        let res = match body {
            Some(b) => req.set("Content-Type", "application/json").send_string(&b.to_string()),
            None => req.call(),
        };
        let res = match res {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(_)) => return Err(Error::Offline),
        };
        let status = res.status();
        let head = |name: &str| res.header(name).map(str::to_string).filter(|v| !v.is_empty());
        let auth_token = head("set-auth-token");
        let auth_jwt = head("set-auth-jwt");
        let cookie_token = res.all("set-cookie").into_iter().find_map(session_cookie);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::Read::take(res.into_reader(), MAX_BODY as u64), &mut text).map_err(|_| Error::Offline)?;
        let body = if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::Null) };
        Ok(Reply { status, body, auth_token, auth_jwt, cookie_token })
    }

    /// A presigned upload: `PUT` the bytes with exactly the `headers` the function returned
    /// (they are part of the signature) and nothing of ours - no token goes to the storage.
    /// Its status, or `Error::Offline`.
    pub fn put_bytes(&self, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<u16, Error> {
        let mut req = self.agent.put(url).timeout(TRANSFER);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_bytes(body) {
            Ok(r) => Ok(r.status()),
            Err(ureq::Error::Status(code, _)) => Ok(code),
            Err(ureq::Error::Transport(_)) => Err(Error::Offline),
        }
    }

    /// A presigned download: its status and at most `max` bytes (more is `too_large`).
    pub fn get_bytes(&self, url: &str, max: u64) -> Result<(u16, Vec<u8>), Error> {
        let res = match self.agent.get(url).timeout(TRANSFER).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return Ok((code, Vec::new())),
            Err(ureq::Error::Transport(_)) => return Err(Error::Offline),
        };
        let status = res.status();
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(res.into_reader(), max + 1), &mut out).map_err(|_| Error::Offline)?;
        if out.len() as u64 > max {
            return Err(Error::Code("too_large"));
        }
        Ok((status, out))
    }
}

/// The session token of a `Set-Cookie` line (Neon Auth's `__Secure-neon-auth.session_token`,
/// Better Auth's `better-auth.session_token`), unless the cookie is being removed.
fn session_cookie(line: &str) -> Option<String> {
    let (pair, rest) = line.split_once(';').unwrap_or((line, ""));
    let (name, value) = pair.split_once('=')?;
    let gone = rest.to_ascii_lowercase().replace(' ', "").contains("max-age=0");
    (name.trim().ends_with("session_token") && !value.trim().is_empty() && !gone).then(|| percent_decode(value.trim()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(x) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A query string's value, percent-encoded (`user_id=eq.<this>`).
pub(crate) fn encode(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_session_cookie_is_read_and_a_removed_one_is_not() {
        assert_eq!(super::session_cookie("__Secure-neon-auth.session_token=abc.d%2Bef; Path=/; HttpOnly; Secure").as_deref(), Some("abc.d+ef"));
        assert_eq!(super::session_cookie("__Secure-neon-auth.session_token=; Max-Age=0"), None);
        assert_eq!(super::session_cookie("__Secure-neon-auth.session_token=x; Max-Age=0; Path=/"), None);
        assert_eq!(super::session_cookie("other=1"), None);
        assert_eq!(super::encode("a b/ü"), "a%20b%2F%C3%BC");
    }
}

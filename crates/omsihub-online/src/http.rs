//! The requests over HTTPS (rustls, the certificates checked against the Mozilla roots), no
//! redirects followed (a token goes nowhere else), short timeouts; every one carries the project's
//! publishable key as `apikey`. An answer whatever its status comes back as a `Reply`; no answer at
//! all is `Error::Offline`.

use crate::codes::Error;
use serde_json::Value;
use std::time::Duration;

/// What a request sends.
#[derive(Clone, Copy)]
pub(crate) enum Body<'a> {
    None,
    Json(&'a Value),
    /// A file, with its content type (a cloud save).
    Bytes(&'a [u8], &'a str),
}

/// An answer: its status, its JSON (Null when it had none or was not JSON) and, where asked for,
/// its bytes as they came.
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    pub status: u16,
    pub body: Value,
    pub bytes: Vec<u8>,
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

/// The longest JSON answer (the friends list is the largest: 500 rows).
const MAX_BODY: u64 = 2 << 20;
/// The longest file (a cloud save: the bucket's limit, and a little more to notice one past it).
const MAX_FILE: u64 = crate::saves::MAX_BYTES as u64 + 1;

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

    /// One request with `apikey`; `bearer` goes as `Authorization: Bearer`, `headers` as they are.
    /// `file` keeps a successful answer's bytes (a download) and gives it the time a file takes;
    /// `quick` keeps it to a few seconds (the goodbye as the launcher closes).
    #[allow(clippy::too_many_arguments)]
    pub fn send(&self, method: &str, url: &str, apikey: &str, bearer: Option<&str>, body: Body, headers: &[(&str, &str)], file: bool, quick: bool) -> Result<Reply, Error> {
        let mut req = self.agent.request(method, url).set("apikey", apikey).set("Accept", if file { "*/*" } else { "application/json" });
        if quick {
            req = req.timeout(Duration::from_secs(3));
        } else if file || matches!(body, Body::Bytes(..)) {
            req = req.timeout(Duration::from_secs(300));
        }
        if let Some(t) = bearer {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let res = match body {
            Body::Json(b) => req.set("Content-Type", "application/json").send_string(&b.to_string()),
            Body::Bytes(b, kind) => req.set("Content-Type", kind).send_bytes(b),
            Body::None => req.call(),
        };
        let res = match res {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(_)) => return Err(Error::Offline),
        };
        let status = res.status();
        let raw = file && (200..300).contains(&status);
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(res.into_reader(), if raw { MAX_FILE } else { MAX_BODY }), &mut bytes).map_err(|_| Error::Offline)?;
        if raw {
            return Ok(Reply { status, body: Value::Null, bytes });
        }
        let body = if bytes.iter().all(u8::is_ascii_whitespace) { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
        Ok(Reply { status, body, bytes: Vec::new() })
    }
}

/// A query string's value, or a storage path's part, percent-encoded (`user_id=eq.<this>`).
pub(crate) fn encode(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn values_are_percent_encoded() {
        assert_eq!(super::encode("a b/ü"), "a%20b%2F%C3%BC");
        assert_eq!(super::encode("0b7c-41d2_x.y~"), "0b7c-41d2_x.y~");
    }
}

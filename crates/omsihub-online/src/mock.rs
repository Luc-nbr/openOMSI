//! The tests' server: plain HTTP on 127.0.0.1, one answer per request from a function of the
//! request, every request kept for the tests to look at. Nothing here reaches the real project.
//! (Storage answers files: `start_raw`, or `start` with a `content-type` and a text.)

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    /// The header names in lower case.
    pub headers: Vec<(String, String)>,
    /// The body as JSON (Null when it is not), and as it came (a save's bytes).
    pub body: Value,
    pub raw: Vec<u8>,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn bearer(&self) -> Option<&str> {
        self.header("authorization").and_then(|v| v.strip_prefix("Bearer "))
    }
}

/// An answer: status, extra headers, JSON body (with a `content-type` among the extra headers, a
/// text body goes as it is: a file).
pub type Answer = (u16, Vec<(&'static str, String)>, Value);

/// An answer's body: JSON, or bytes as they are.
pub enum Body {
    Json(Value),
    Raw(Vec<u8>),
}

pub struct Mock {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Req>>>,
}

impl Mock {
    pub fn start(answer: impl Fn(&Req) -> Answer + Send + Sync + 'static) -> Mock {
        Self::start_raw(move |r| {
            let (status, extra, out) = answer(r);
            let file = extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
            match out {
                Value::String(t) if file => (status, extra, Body::Raw(t.into_bytes())),
                v => (status, extra, Body::Json(v)),
            }
        })
    }

    pub fn start_raw(answer: impl Fn(&Req) -> (u16, Vec<(&'static str, String)>, Body) + Send + Sync + 'static) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim_end_matches(['\r', '\n']).is_empty() {
                        break;
                    }
                    if let Some((k, v)) = h.trim_end_matches(['\r', '\n']).split_once(':') {
                        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                    }
                }
                let len = headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
                let mut buf = vec![0; len];
                let _ = reader.read_exact(&mut buf);
                let body = serde_json::from_slice(&buf).unwrap_or(Value::Null);
                let req = Req { method, path, headers, body, raw: buf };
                let (status, extra, out) = answer(&req);
                kept.lock().unwrap().push(req);
                let (kind, bytes) = match out {
                    Body::Json(v) if v.is_null() => ("application/json", Vec::new()),
                    Body::Json(v) => ("application/json", v.to_string().into_bytes()),
                    Body::Raw(b) => ("application/octet-stream", b),
                };
                let kind = if extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) { String::new() } else { format!("Content-Type: {kind}\r\n") };
                let mut head = format!("HTTP/1.1 {status} X\r\n{kind}Content-Length: {}\r\nConnection: close\r\n", bytes.len());
                for (k, v) in extra {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                let _ = stream.write_all(&[format!("{head}\r\n").into_bytes(), bytes].concat());
            }
        });
        Mock { base, seen }
    }

    /// The paths asked for, in order (`POST /auth/v1/token?grant_type=password`).
    pub fn calls(&self) -> Vec<String> {
        self.seen.lock().unwrap().iter().map(|r| format!("{} {}", r.method, r.path)).collect()
    }

    pub fn requests(&self) -> Vec<Req> {
        self.seen.lock().unwrap().clone()
    }
}

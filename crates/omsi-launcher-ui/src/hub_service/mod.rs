//! Omsi-Hub's editors in the desktop launcher (Trello "Tauri 5/6"): what their page asks of the
//! game program through the launcher service (`openomsi --launcher-service`, the game's
//! `launcher::service`, which hands every request it does not know here - one line), because
//! only the game's own code can do it:
//!
//! * `display_preview`: a text on a bus's own destination display in its own font, as the
//!   depot editor and the line editor show it (`displayfont`);
//! * `le`: the line editor, kept here (`lineeditor::headless`): the page's request under `le`,
//!   the view it draws (or the map's roads and stops) in the bytes;
//! * `studio`: the livery studio, run here as our game-drawn launcher runs it (`studio`): the
//!   page's request under `studio`, the studio's picture in the bytes;
//! * `photo`: a bus photographed for the bus picker's tiles and the company's, kept where our
//!   game-drawn launcher keeps its photos (`photo`, Trello "Tauri 6/6");
//! * `pads`: the Controls page's game controllers, read and set up here as our game-drawn
//!   launcher does it (`pads`, Trello "Tauri 6/6");
//!
//! Each answer is the service's: a JSON head and the bytes after it (`Reply`). Kept in this file
//! of our own beside the editors it serves, so that the modules it needs stay private.

use serde_json::Value;
use std::sync::Arc;

mod display;
mod pads;
mod photo;
mod studio;

/// Where an answer goes: what it says and the bytes after it.
pub type Reply = Arc<dyn Fn(Value, Vec<u8>) + Send + Sync>;

/// Answer the request `req` (its command `cmd`) if it is one of ours: true when it was (the
/// answer may come later, from a worker), false when the service does not know it. `renderer`
/// is the service's (made with `open` when there is none yet), for what is drawn.
pub fn serve(cmd: &str, req: &Value, reply: Reply, renderer: &mut Option<omsi_render::Renderer>, open: &dyn Fn() -> anyhow::Result<omsi_render::Renderer>) -> bool {
    let _ = renderer;
    let s = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match cmd {
        "display_preview" => {
            let (bus, text) = (s("bus"), s("text"));
            // (a bus's displays take a moment to read: off the service's thread, which draws
            // the pictures)
            std::thread::spawn(move || {
                let (head, png) = display::preview(&bus, &text);
                reply(head, png);
            });
            true
        }
        // the line editor (`lineeditor::headless`): its view, or the map's roads and stops
        "le" => {
            line_editor(req, reply);
            true
        }
        // the game controllers (`pads`), on their thread
        "pads" => {
            pads::ask(req, reply);
            true
        }
        // the livery studio (`studio`): its thread started here, with a renderer of its own
        "studio" => {
            let ask = req.get("studio").cloned().unwrap_or(Value::Null);
            if !studio::ask(&ask, reply.clone()) {
                match open() {
                    Ok(r) => studio::start(r, &ask, reply),
                    Err(e) => reply(serde_json::json!({ "ok": false, "error": format!("{e:#}") }), Vec::new()),
                }
            }
            true
        }
        // a bus's photo (`photo`): on its thread, with a renderer of its own
        "photo" => {
            photo::ask(req, reply, open);
            true
        }
        _ => false,
    }
}

/// Answer `req` where it is read, if it is one of ours that draws nothing: true when it was.
/// The service draws its pictures one after the other on one thread, and the first is slow
/// (`warm` builds the renderer's pipelines while the launcher starts): the line editor, opened
/// then, waited seconds for a picture it does not need. Its requests go to a thread of their
/// own instead, in the order they came.
pub fn at_once(req: &Value, reply: Reply) -> bool {
    match req.get("cmd").and_then(Value::as_str) {
        Some("le") => {
            line_editor(req, reply);
            true
        }
        Some("pads") => {
            pads::ask(req, reply);
            true
        }
        // (the studio, once its thread runs: its pictures are drawn there)
        Some("studio") if studio::running() => studio::ask(req.get("studio").unwrap_or(&Value::Null), reply),
        _ => false,
    }
}

/// The line editor's request, on its thread (started with the first).
fn line_editor(req: &Value, reply: Reply) {
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Mutex, OnceLock};
    static WORKER: OnceLock<Mutex<Sender<(Value, Reply)>>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<(Value, Reply)>();
        let _ = std::thread::Builder::new().name("line editor".into()).spawn(move || {
            while let Ok((req, reply)) = rx.recv() {
                let v = crate::lineeditor::headless::serve(req.get("le").unwrap_or(&Value::Null));
                reply(serde_json::json!({ "ok": true }), v.to_string().into_bytes());
            }
        });
        Mutex::new(tx)
    });
    let _ = worker.lock().unwrap_or_else(|e| e.into_inner()).send((req.clone(), reply));
}

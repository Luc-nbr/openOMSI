//! Omsi-Hub's editors in the desktop launcher (Trello "Tauri 5/6"): what their page asks of the
//! game program through the launcher service (`openomsi --launcher-service`, the game's
//! `launcher::service`, which hands every request it does not know here - one line), because
//! only the game's own code can do it:
//!
//! * `display_preview`: a text on a bus's own destination display in its own font, as the
//!   depot editor and the line editor show it (`displayfont`);
//! * `le`: the line editor, kept here (`lineeditor::headless`): the page's request under `le`,
//!   the view it draws (or the map's roads and stops) in the bytes;
//!
//! Each answer is the service's: a JSON head and the bytes after it (`Reply`). Kept in this file
//! of our own beside the editors it serves, so that the modules it needs stay private.

use serde_json::Value;
use std::sync::Arc;

mod display;

/// Where an answer goes: what it says and the bytes after it.
pub type Reply = Arc<dyn Fn(Value, Vec<u8>) + Send + Sync>;

/// Answer the request `req` (its command `cmd`) if it is one of ours: true when it was (the
/// answer may come later, from a worker), false when the service does not know it. `renderer`
/// is the service's (made with `open` when there is none yet), for what is drawn.
pub fn serve(cmd: &str, req: &Value, reply: Reply, renderer: &mut Option<omsi_render::Renderer>, open: &dyn Fn() -> anyhow::Result<omsi_render::Renderer>) -> bool {
    let _ = (renderer, open);
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
            let v = crate::lineeditor::headless::serve(req.get("le").unwrap_or(&Value::Null));
            reply(serde_json::json!({ "ok": true }), v.to_string().into_bytes());
            true
        }
        _ => false,
    }
}

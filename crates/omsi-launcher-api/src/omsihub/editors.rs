//! Omsi-Hub's editors in the desktop launcher's page (Trello "Tauri 5/6"), after our
//! game-drawn launcher's (`crates/omsi-launcher-ui/src/{timetable, hof, lineeditor,
//! depoteditor, livery}`): the page draws them (`launcher/ui/js/omsihub/editors`), the work is
//! done here, in Rust, by the same functions of `omsi-launcher-core` the game-drawn launcher
//! calls - and what needs the game program itself (the map's lane network, the bus on the
//! game's renderer) by the launcher service (`Platform::renderer`, the game's
//! `launcher::service_editors`).
//!
//! * `timetable`: the Timetable page - a map's lines, their tours and the trips each runs;
//! * `hof`: the bus's depot files on the bus step (`hof.rs`): which one the drive takes, how
//!   each stands to the map, and adding the map's;
//! * `depots`: the depot editor - depot files of the player's own, the file open kept here;
//!
//! `omsihub::call` hands every command it does not know here (one line).

use crate::{Answer, Platform};
use serde_json::Value;

mod depots;
mod hof;
mod timetable;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// A value of the arguments (`null` when it is not there: an `Option` takes that as `None`).
fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// A map's `global.cfg` as the editors read it: the content folder's when the map lies there,
/// else under the OMSI folder (`timetable::draw`'s way), else in a content root (a mod's map).
fn global_of(file: &str) -> std::path::PathBuf {
    let root = omsi_launcher_lib::root_dir().unwrap_or_default();
    if let Some(p) = omsi_launcher_lib::content_dir().map(|c| c.join(file)).filter(|p| p.is_file()) {
        return p;
    }
    let p = omsi_cfg::resolve_path(&root, file);
    if omsi_cfg::vfs::is_file(&p) {
        return p;
    }
    omsi_cfg::find_in_roots(file).map(|(_, p)| p).unwrap_or(p)
}

/// `text` on `bus`'s own destination display in its own font, drawn by the game program
/// (`hub_service::display`): `{ failed, none, too_wide, font, picture }`, the picture a PNG as a
/// `data:` address.
fn display_preview(p: &dyn Platform, bus: &str, text: &str) -> Answer {
    let (mut head, png) = p.renderer(serde_json::json!({ "cmd": "display_preview", "bus": bus, "text": text }), std::time::Duration::from_secs(30))?;
    if !png.is_empty() {
        head["picture"] = Value::String(format!("data:image/png;base64,{}", crate::base64(&png)));
    }
    Ok(head)
}

/// The line editor (`lineeditor::headless`, kept by the game program: it has the map's lane
/// network and the router): `args` is the request (`op` and what it takes), the answer what
/// the page shows. (Under `le`: the service numbers its requests with an `id` of its own, and a
/// line's id is one of the editor's.)
fn line_editor(p: &dyn Platform, args: &Value) -> Answer {
    let req = serde_json::json!({ "cmd": "le", "le": args });
    let (_, body) = p.renderer(req, std::time::Duration::from_secs(120))?;
    serde_json::from_slice(&body).map_err(err)
}

/// A command of the editors, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Option<Answer> {
    timetable::call(cmd, args).or_else(|| hof::call(cmd, args)).or_else(|| depots::call(cmd, args)).or_else(|| match cmd {
        "hub_le" => Some(line_editor(p, args)),
        "hub_display_preview" => Some((|| display_preview(p, &arg::<String>(args, "bus")?, &arg::<String>(args, "text")?))()),
        // (a script error of the page, into the launcher's log: the window has no console)
        "hub_page_error" => Some(arg::<String>(args, "text").map(|t| {
            omsi_launcher_lib::log_to_file(&format!("page: {t}"));
            Value::Null
        })),
        _ => None,
    })
}

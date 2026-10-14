//! Bus3D: the page draws the buses itself (Omsi-Hub's 3D view, livery studio and bus photos,
//! ported from the old app). This side reads a bus as the game reads it, keeps its geometry
//! pack on disk and hands the page the pack, the textures and the bus's look in a livery - by
//! id, never by path.
//!
//! * `b3d_open { bus, paint?, options? }`: the bus's manifest (names, box, parts, cameras, its
//!   textures) and its look in `paint` - built the first time, from the cache after that;
//! * `b3d_look { bus, pack, paint?, options? }`: another livery's look of an open bus;
//! * `b3d_env`: OMSI's own sky (`envir.cfg`) and clouds (`Weather/clouds.cfg`) for the view;
//! * bytes: `b3d_pack { id }` (the pack, scrambled blocks descrambled in memory),
//!   `b3d_tex { id, from?, to?, skip? }` (a DXT texture's bytes `from..to`, else the picture
//!   decoded to RGBA, halved `skip` times).
//!
//! The page's half is `launcher/ui/js/omsihub/bus3d/`; the bytes come through the window's
//! own protocol (`omsi://localhost/bytes/<cmd>?args=…`, see `launcher/src-tauri/src/protocol.rs`)
//! so that the renderer's worker can fetch them itself.

pub mod cache;
pub mod look;
pub mod pack;
pub mod photo;
pub mod scramble;
pub mod tex;

#[cfg(test)]
mod tests;

use crate::Answer;
use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::path::Path;

fn arg_str(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The bus options of the arguments (`[[var, value]]`), else the ones chosen for the bus in
/// this livery on the bus step (what the game gets as `--setvar`).
fn options_of(args: &Value, bus: &str, paint: &str) -> Vec<(String, f32)> {
    match args.get("options").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(|x| Some((x.get(0)?.as_str()?.to_string(), x.get(1)?.as_f64()? as f32))).collect(),
        None => super::hub_more::busoptions::vars_for(bus, paint),
    }
}

fn root() -> Result<std::path::PathBuf, String> {
    core::root_dir().map_err(|e| format!("{e:#}"))
}

/// Open a bus: its manifest and its look.
fn open(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    if bus.trim().is_empty() {
        return Err("no bus".into());
    }
    let paint = arg_str(args, "paint");
    let options = options_of(args, &bus, &paint);
    let root = root()?;
    let o = std::panic::catch_unwind(|| cache::open(&root, &bus, Some(paint.as_str()).filter(|p| !p.trim().is_empty()), &options)).map_err(|_| "reading the bus failed".to_string())??;
    Ok(json!({ "manifest": *o.manifest, "look": o.look, "cache": o.from_cache, "ms": o.ms }))
}

/// Another livery's look of an open bus.
fn look(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    let paint = arg_str(args, "paint");
    let options = options_of(args, &bus, &paint);
    let root = root()?;
    let (manifest, _) = cache::pack_of(&root, &bus)?;
    let look = std::panic::catch_unwind(|| cache::look_of(&root, &bus, &manifest, Some(paint.as_str()).filter(|p| !p.trim().is_empty()), &options)).map_err(|_| "the look failed".to_string())??;
    Ok(json!({ "pack": manifest.pack, "look": look }))
}

/// The lines of a text file of the OMSI folder.
fn lines(p: &Path) -> Vec<String> {
    omsi_cfg::vfs::read(p).map(|b| omsi_cfg::codepage::decode(&b).lines().map(|l| l.trim_end().to_string()).collect()).unwrap_or_default()
}

/// OMSI's sky and clouds for the 3D view (the old app's `bus3domgeving.ts`): the first of
/// `[sky_textures]` in `envir.cfg` (the day's panorama) and the clouds of the weather
/// "Cumulus 1" in `Weather/clouds.cfg` (else the first scattered ones), with the metres one
/// repeat of the clouds takes. Nothing of OMSI is shipped: this is read on the player's
/// computer; without it the page draws a sky of its own.
fn env() -> Answer {
    let root = root()?;
    let mut out = json!({ "cloudSize": 2000.0 });
    let envir = lines(&omsi_cfg::resolve_path(&root, "envir.cfg"));
    if let Some(i) = envir.iter().position(|l| l.trim() == "[sky_textures]") {
        if let Some(rel) = envir.get(i + 1).map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if let Some((info, src)) = tex::probe(&omsi_cfg::resolve_path(&root, rel)) {
                cache::register(&[src]);
                out["sky"] = json!(info);
            }
        }
    }
    let clouds = lines(&omsi_cfg::resolve_path(&root, "Weather/clouds.cfg"));
    let kinds: Vec<(String, String, f32, String)> = clouds
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == "[cloudtype]")
        .map(|(i, _)| {
            let at = |k: usize| clouds.get(i + k).map(|l| l.trim().to_string()).unwrap_or_default();
            (at(1), at(2), omsi_cfg::parse_f32(&at(3)), at(4).to_lowercase())
        })
        .collect();
    let pick = kinds.iter().find(|k| k.0.eq_ignore_ascii_case("cumulus 1")).or_else(|| kinds.iter().find(|k| k.3 == "sct"));
    if let Some((_, file, size, _)) = pick {
        if let Some((info, src)) = tex::probe(&omsi_cfg::resolve_path(&root.join("Texture"), file)) {
            cache::register(&[src]);
            out["clouds"] = json!(info);
            if size.is_finite() && *size > 10.0 {
                out["cloudSize"] = json!(size);
            }
        }
    }
    Ok(out)
}

/// A command of Bus3D, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "b3d_open" => open(args),
        "b3d_look" => look(args),
        "b3d_env" => env(),
        "b3d_photo_put" => photo::put(args),
        "b3d_photo_has" => photo::has(args),
        _ => return None,
    })
}

/// A command of Bus3D that answers bytes, or None when `cmd` is not one.
pub fn call_bytes(cmd: &str, args: &Value) -> Option<Result<Vec<u8>, String>> {
    Some(match cmd {
        "b3d_pack" => {
            let num = |k: &str| args.get(k).and_then(Value::as_u64);
            cache::pack_bytes(&arg_str(args, "id"), num("from").zip(num("to")))
        }
        "b3d_tex" => (|| {
            let path = cache::texture(&arg_str(args, "id"))?;
            let num = |k: &str| args.get(k).and_then(Value::as_u64);
            match (num("from"), num("to")) {
                (Some(from), Some(to)) => tex::range(&path, from, to),
                _ => tex::rgba(&path, num("skip").unwrap_or(0) as u32),
            }
        })(),
        "b3d_photo" => photo::get(args),
        _ => return None,
    })
}

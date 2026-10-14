//! The livery studio of the desktop launcher's page (Omsi-Hub's Lakstudio, ported from the old
//! app into the page: `launcher/ui/js/omsihub/bus3d/lak`). The page paints the livery on its
//! own GPU while it is made; this side keeps what the page cannot: the projects on disk, the
//! pictures brought in, the bus's shape as the paint sees it (its textures, their colour zones,
//! what each mesh is to the paint), the texts and shapes drawn into masks in their fonts, and the
//! save into the game - through `omsihub-livery`, the game program's studio's own core, so that
//! a livery saved here is the files that studio writes, and a project of either opens in both.
//!
//! * `lak_projects`, `lak_project { id }`, `lak_new { bus, paint, start }`,
//!   `lak_keep { project, final }`: the projects (`~/.openomsi/liveries/<id>/project.json`,
//!   every change at once into `project.autosave.json`);
//! * `lak_prepare { bus, paint, start, options, buses }`: the bus ready to paint - its
//!   textures, the colour zones of each, every mesh slot's part in the paint, its box and window
//!   line (`prepare.rs`);
//! * `lak_picture_add { id, name, data }`, bytes `lak_picture { id, hash, white }`: a picture
//!   brought in, kept by its hash in the project's `beelden`;
//! * bytes `lak_decal { layer, k }`: a text, shape or own shape drawn at `k` pixels a metre;
//!   `lak_sizes { id, layers }`: every decal's size on the bus;
//! * bytes `lak_template { target }`: the maker's template of a texture (MU, and MA in alpha);
//! * `lak_quick`, `lak_stripe`, `lak_name`, `lak_fonts`, `lak_cost`: the model's own rules;
//! * `lak_place`, `lak_progress`, `lak_remove`: the livery into the game and out (`place.rs`).
//!
//! Positions go to the page in the o3d frame the 3D view draws in (x right, y up, z forward),
//! as the project file keeps a decal's place; a brush stroke's points are kept in the bus's own
//! frame (x right, y forward, z up), as the game program's studio writes them.
//!
//! THE FORMATS. One project format for both studios: `project.json` (and `project.autosave.json`)
//! under `~/.openomsi/liveries/<id>/`, `omsihub-livery`'s `model::Project` - the old Electron
//! app's JSON (`shared/lak.ts`: its Dutch field names, a decal's `midden` in o3d axes) with what
//! the game program's studio added: `lakgroep` (a paint group's fill), `verloop` (gradients),
//! `omlijning` (outlines), `pad` (own shapes), `hoogteM`/`verhoudingVast`, `spiegelbeeld` beside
//! the old `zelfdeRichting`, `letterafstand`, `ookOverRamen`, `ookOp` (the family). Two things of
//! the old app are read differently: a brush stroke is a polyline on the bus in the bus's frame
//! with its normals (`punten` as `[[x, y, z]]`, `normalen`), where the old app kept flat o3d
//! triples with the camera it was painted from (such a project does not open here, as it did not
//! in the game program's studio); and a fill's `bron: 'start'` is left out - `lakgroep` does its
//! work. What goes into the game is the same everywhere: the textures written as DDS (BC1, BC3
//! with alpha) under `<CTC>\LiveryStudio\<nnnn>_<slug>\` and a `~LiveryStudio_<nnnn>_<slug>.cti`
//! beside the bus's own, in openOMSI's content folder.

mod place;
mod prepare;
#[cfg(test)]
mod tests;

use crate::Answer;
use omsihub_livery::model::{self, Kind, Layer, Project};
use omsihub_livery::shapes::{self, Raster};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use super::company::words::{self, tr};

fn arg_str(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// The core's texts (a name refused) in the language this call answers in.
fn core_tr(text: &str) -> Option<String> {
    let t = tr(text);
    (t != text).then_some(t)
}

/// A project's id as a folder name: letters, digits, `-` and `_` only (never a path).
fn clean_id(id: &str) -> Result<String, String> {
    let ok = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| id.to_string()).ok_or_else(|| "not a project".to_string())
}

fn project_dir(id: &str) -> Result<PathBuf, String> {
    Ok(omsihub_livery::projects_dir().join(clean_id(id)?))
}

// --- the pictures --------------------------------------------------------------------------

/// The pictures of the projects opened, by project and hash (decoded once).
fn pictures() -> &'static Mutex<HashMap<String, Arc<HashMap<String, Arc<Raster>>>>> {
    static P: OnceLock<Mutex<HashMap<String, Arc<HashMap<String, Arc<Raster>>>>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// The pictures of project `id` (its `beelden` folder), read the first time.
pub(crate) fn pictures_of(id: &str) -> Arc<HashMap<String, Arc<Raster>>> {
    if let Some(p) = pictures().lock().unwrap_or_else(|e| e.into_inner()).get(id) {
        return p.clone();
    }
    let mut map = HashMap::new();
    if let Ok(dir) = project_dir(id) {
        for e in std::fs::read_dir(dir.join("beelden")).into_iter().flatten().flatten() {
            let p = e.path();
            let (Some(stem), Ok(bytes)) = (p.file_stem().map(|s| s.to_string_lossy().into_owned()), std::fs::read(&p)) else { continue };
            if let Ok(r) = shapes::decode_image(&bytes, &p.to_string_lossy()) {
                map.insert(stem, Arc::new(r));
            }
        }
    }
    let map = Arc::new(map);
    pictures().lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), map.clone());
    map
}

/// A picture brought in (`data` its file's bytes in base64, `name` its file's name): kept by
/// its hash with project `id`, as the game program's studio keeps them.
fn picture_add(args: &Value) -> Answer {
    let id = clean_id(&arg_str(args, "id"))?;
    let bytes = super::bus3d::photo::base64(&arg_str(args, "data"))?;
    let (hash, pic) = store_picture(&project_dir(&id)?.join("beelden"), &arg_str(args, "name"), &bytes)?;
    let white = shapes::wants_white_clear(&pic);
    let (w, h) = (pic.w, pic.h);
    let mut all = (*pictures_of(&id)).clone();
    all.insert(hash.clone(), Arc::new(pic));
    pictures().lock().unwrap_or_else(|e| e.into_inner()).insert(id, Arc::new(all));
    Ok(json!({ "hash": hash, "w": w, "h": h, "aspect": h as f32 / w.max(1) as f32, "whiteClear": white }))
}

/// A picture's file (`name`, `bytes`) read and kept in `dir` under its hash: the hash and the
/// picture.
fn store_picture(dir: &std::path::Path, name: &str, bytes: &[u8]) -> Result<(String, Raster), String> {
    if bytes.len() > 20 << 20 {
        return Err(tr("The picture is larger than 20 MB."));
    }
    let pic = shapes::decode_image(bytes, name).map_err(|e| tr("This picture cannot be read: %{e}").replace("%{e}", &e))?;
    let hash = hash_of(bytes);
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).filter(|e| e.len() <= 4 && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_else(|| "png".into());
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{hash}.{ext}")), bytes).map_err(|e| e.to_string())?;
    Ok((hash, pic))
}

/// The hash a picture is kept by (as the game program's studio names them: the first 20 bytes of
/// its SHA-256 in hex).
fn hash_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().take(20).map(|b| format!("{b:02x}")).collect()
}

/// A raster as bytes for the page: width, height (u32 LE), its channels (u32), then the pixels.
fn raster_bytes(out: &mut Vec<u8>, r: &Raster) {
    out.extend_from_slice(&r.w.to_le_bytes());
    out.extend_from_slice(&r.h.to_le_bytes());
    out.extend_from_slice(&(r.channels as u32).to_le_bytes());
    out.extend_from_slice(&r.data);
}

/// A picture of a project: RGBA, its white made see-through when `white`.
fn picture(args: &Value) -> Result<Vec<u8>, String> {
    let id = clean_id(&arg_str(args, "id"))?;
    let hash = arg_str(args, "hash");
    let pics = pictures_of(&id);
    let pic = pics.get(&hash).ok_or_else(|| format!("no picture {hash}"))?;
    Ok(picture_bytes(pic, args.get("white").and_then(Value::as_bool).unwrap_or(false)))
}

fn picture_bytes(pic: &Raster, white: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if white {
        raster_bytes(&mut out, &shapes::white_clear(pic));
    } else {
        raster_bytes(&mut out, pic);
    }
    out
}

// --- the decals ----------------------------------------------------------------------------

/// A decal's size on the bus (metres): a text's from its letters, a picture's from its own
/// proportions, a shape's from its place (`paint::decal_size`).
fn sizes(args: &Value) -> Answer {
    let layers: Vec<Layer> = arg(args, "layers")?;
    let id = arg_str(args, "id");
    let pics = if id.is_empty() { Arc::new(HashMap::new()) } else { pictures_of(&clean_id(&id)?) };
    let mut out = serde_json::Map::new();
    for l in &layers {
        if let Some((w, h)) = omsihub_livery::paint::decal_size(&l.kind, &pics) {
            out.insert(l.id.clone(), json!([w, h]));
        }
    }
    Ok(Value::Object(out))
}

/// A text, shape or own shape (`layer`) drawn at `k` pixels a metre (at most 2048 on its long
/// side, as the game program's studio draws them while it edits): its size (metres), then its
/// fill's mask and its outline's (`raster_bytes` each; an outline of 0 x 0 when it has none).
fn decal(args: &Value) -> Result<Vec<u8>, String> {
    let layer: Layer = arg(args, "layer")?;
    let k_asked = args.get("k").and_then(Value::as_f64).unwrap_or(100.0) as f32;
    let max_px = args.get("max").and_then(Value::as_u64).unwrap_or(2048).clamp(64, 4096) as u32;
    let none = HashMap::new();
    let (w, h) = omsihub_livery::paint::decal_size(&layer.kind, &none).ok_or("not a decal")?;
    if !(w > 0.0 && h > 0.0) {
        return Err("an empty decal".into());
    }
    let k = k_asked.min(max_px as f32 / w.max(h)).max(1.0);
    let (pw, ph) = (((w * k).ceil() as u32).clamp(4, max_px), ((h * k).ceil() as u32).clamp(4, max_px));
    let outline_px = |o: &Option<model::Outline>| o.as_ref().map(|o| o.width_cm / 100.0 * k).unwrap_or(0.0);
    let (fill, outline) = match &layer.kind {
        Kind::Text { text, font, height_cm, outline, spacing, .. } => shapes::text_raster(text, font, *spacing, height_cm / 100.0 * k, outline_px(outline)),
        Kind::Shape { shape, outline, .. } => shapes::shape_raster(shape, pw, ph, outline_px(outline)),
        Kind::Path { nodes, outline, .. } => shapes::path_raster(nodes, pw, ph, outline_px(outline)),
        _ => return Err("not a text or shape".into()),
    };
    let mut out = Vec::new();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    raster_bytes(&mut out, &fill);
    match outline {
        Some(o) => raster_bytes(&mut out, &o),
        None => raster_bytes(&mut out, &Raster { w: 0, h: 0, channels: 1, data: Vec::new() }),
    }
    Ok(out)
}

// --- the projects --------------------------------------------------------------------------

/// What the list of liveries shows of a project.
fn summary(p: &Project) -> Value {
    json!({
        "id": p.id, "name": p.name, "bus": p.bus, "start": p.start, "paint": p.start_paint,
        "layers": p.layers.len(), "saved": p.saved, "placed": p.placed.as_ref().map(|x| x.name.clone()),
    })
}

fn projects_list() -> Answer {
    Ok(Value::Array(omsihub_livery::projects().iter().map(summary).collect()))
}

/// A project as its folder has it (its temporary file when that is newer: `restored`).
fn project(args: &Value) -> Answer {
    let dir = project_dir(&arg_str(args, "id"))?;
    let (p, restored) = omsihub_livery::load_project(&dir).ok_or_else(|| tr("This livery could not be read."))?;
    Ok(json!({ "project": p, "restored": restored }))
}

/// A new project on `bus` from `paint`, begun in one of the ways of `model::STARTS` (not on disk
/// until it changes).
fn new_project(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    if bus.trim().is_empty() {
        return Err("no bus".into());
    }
    let paint = Some(arg_str(args, "paint")).filter(|p| !p.trim().is_empty());
    let mut p = Project::new(model::new_id("p"), bus, paint, omsihub_livery::now_text());
    let start = arg_str(args, "start");
    if model::STARTS.iter().any(|(k, _)| *k == start) {
        p.start = start;
    }
    Ok(json!({ "project": p }))
}

/// The project as the page has it: into its temporary file (every change), or (`final`) into
/// `project.json`, the temporary file gone. A project the page could not read back is refused.
fn keep(args: &Value) -> Answer {
    let mut p: Project = arg(args, "project")?;
    let dir = project_dir(&p.id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if args.get("final").and_then(Value::as_bool).unwrap_or(false) {
        p.saved = omsihub_livery::now_text();
        omsihub_livery::write_project(&dir, &p).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(dir.join(omsihub_livery::TEMP_FILE));
    } else {
        let bytes = serde_json::to_vec(&p).map_err(|e| e.to_string())?;
        let part = dir.join(format!("{}.part", omsihub_livery::TEMP_FILE));
        std::fs::write(&part, bytes).and_then(|_| std::fs::rename(&part, dir.join(omsihub_livery::TEMP_FILE))).map_err(|e| e.to_string())?;
    }
    Ok(json!({ "saved": p.saved }))
}

// --- the model's rules ---------------------------------------------------------------------

/// The quick livery applied to the project's layers (`model::apply_quick`, the bus prepared).
fn quick(args: &Value) -> Answer {
    let p: Project = arg(args, "project")?;
    let q: model::Quick = arg(args, "quick")?;
    let dims = prepare::dims()?;
    let layers = model::apply_quick(&p.layers, &q, &dims, &|t, f| shapes::text_width(t, f));
    Ok(json!({ "layers": layers }))
}

/// A stripe after its template on the bus prepared (`model::stripe`).
fn stripe(args: &Value) -> Answer {
    let template: model::StripeTemplate = arg(args, "template")?;
    let dims = prepare::dims()?;
    Ok(serde_json::to_value(model::stripe(template, &arg_str(args, "colour"), &dims)).unwrap_or(Value::Null))
}

/// Why `name` cannot be the livery's in OMSI (None: it can), among the names the bus's liveries
/// have (`taken`, the prepared bus's when not given).
fn name_check(args: &Value) -> Answer {
    let name = arg_str(args, "name");
    let taken: Vec<String> = match args.get("taken").and_then(Value::as_array) {
        Some(t) => t.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        None => prepare::taken(),
    };
    Ok(match model::name_error(&name, taken.iter().map(String::as_str)) {
        Some(e) => json!({ "error": e.message() }),
        None => json!({ "error": null }),
    })
}

/// The fonts this computer has, the launcher's own first, and the built-in shapes.
fn fonts() -> Answer {
    Ok(json!({
        "fonts": shapes::font_names(),
        "default": shapes::DEFAULT_FONT,
        "shapes": shapes::SHAPES.iter().map(|(k, label)| json!([k, tr(label)])).collect::<Vec<_>>(),
    }))
}

/// A command of the livery studio, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    omsihub_livery::set_tr(core_tr);
    Some(match cmd {
        "lak_projects" => projects_list(),
        "lak_project" => project(args),
        "lak_new" => new_project(args),
        "lak_keep" => keep(args),
        "lak_prepare" => std::panic::catch_unwind(|| prepare::prepare(args)).unwrap_or_else(|_| Err(tr("The bus cannot be painted: %{e}").replace("%{e}", "the preparation failed"))),
        "lak_picture_add" => picture_add(args),
        "lak_sizes" => sizes(args),
        "lak_quick" => quick(args),
        "lak_stripe" => stripe(args),
        "lak_name" => name_check(args),
        "lak_fonts" => fonts(),
        "lak_cost" => place::cost(args),
        "lak_place" => place::place(args),
        "lak_progress" => place::progress(),
        "lak_remove" => place::remove(args),
        _ => return None,
    })
}

/// A command of the livery studio that answers bytes, or None when `cmd` is not one.
pub fn call_bytes(cmd: &str, args: &Value) -> Option<Result<Vec<u8>, String>> {
    Some(match cmd {
        "lak_picture" => picture(args),
        "lak_decal" => std::panic::catch_unwind(|| decal(args)).unwrap_or_else(|_| Err("the decal could not be drawn".into())),
        "lak_template" => prepare::template(args),
        _ => return None,
    })
}

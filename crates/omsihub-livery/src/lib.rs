//! Omsi-Hub's livery studio's own core, shared by the game program's studio (`omsihub-game`'s
//! `livery`) and the desktop launcher's page studio (`omsi-launcher-api`'s `omsihub::lak`): the
//! project and its layers (`model`), what the bus's shape is to the paint (`bake`), the layers
//! painted into the textures on the CPU (`paint`, `shapes`, `colour`) and the livery written into
//! the game (`export`) - and round them which textures a bus's livery paints, the maker's
//! templates, the windows, the bus's family and the projects kept on disk.
//!
//! Nothing here draws: the game program shows the studio with its renderer, the launcher's page
//! with its own WebGL2 (`launcher/ui/js/omsihub/bus3d/lak`); both save through `export`.

pub mod bake;
pub mod colour;
pub mod export;
pub mod model;
pub mod paint;
pub mod shapes;

use bake::{MeshIn, Role, SeeThrough, Slot, SlotUse};
use glam::Mat4;
use model::Project;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// The interface's language for the texts this crate says (a name refused, the save's
/// progress): set by the program that shows them (`set_tr`); English until then.
static TR: RwLock<Option<fn(&str) -> Option<String>>> = RwLock::new(None);

/// Translate this crate's texts with `f` (None where it has no translation).
pub fn set_tr(f: fn(&str) -> Option<String>) {
    if let Ok(mut t) = TR.write() {
        *t = Some(f);
    }
}

/// `text` in the interface's language.
pub fn tr(text: &str) -> Cow<'_, str> {
    match TR.read().ok().and_then(|t| *t).and_then(|f| f(text)) {
        Some(t) => Cow::Owned(t),
        None => Cow::Borrowed(text),
    }
}

/// The maker's template of a texture (`file`, as the model names it): `<stem>.rpc` in a folder
/// of the game's `SDK\RepaintTool`, whose first five lines name its BS, AL, MA, AD and MU
/// pictures (the advertising and window variants, `_NL`, `_full`, are other files).
pub fn find_template(root: &Path, file: &str) -> Option<[PathBuf; 2]> {
    let stem = Path::new(file.trim()).file_stem()?.to_string_lossy().to_lowercase();
    let tool = omsi_cfg::resolve_path(root, "SDK/RepaintTool");
    for dir in std::fs::read_dir(&tool).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        let Some(rpc) = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == format!("{stem}.rpc"))) else {
            continue;
        };
        let text = omsi_cfg::codepage::decode(&std::fs::read(&rpc).ok()?);
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).take(5).collect();
        if lines.len() < 5 {
            continue;
        }
        let path = |l: &str| Some(omsi_cfg::resolve_path(&dir, l)).filter(|p| p.is_file());
        if let (Some(ma), Some(mu)) = (path(lines[2]), path(lines[4])) {
            return Some([ma, mu]);
        }
    }
    None
}

/// A template's pictures at `w` x `h` (RGBA each).
pub fn template_pictures(t: &[PathBuf; 2], w: u32, h: u32) -> Option<[Vec<u8>; 2]> {
    let one = |p: &PathBuf| -> Option<Vec<u8>> {
        let img = omsi_texture::decode_file(p).ok()?;
        Some(if (img.width, img.height) != (w, h) { omsi_texture::bc::resize(&img.rgba, img.width, img.height, w, h) } else { img.rgba })
    };
    Some([one(&t[0])?, one(&t[1])?])
}

/// What a bus's windows can take of the livery: a print on the glass (window advertising, a
/// wrap over the windows) needs the glass's texture and how see-through it is both to be ones
/// the livery changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Windows {
    /// A print goes onto the glass, seen from outside; where it covers less, the glass stays a
    /// little see-through.
    Print,
    /// The glass's texture is the livery's, but how see-through it is comes from a map the
    /// livery cannot change: a print shows only as a tint.
    Faint,
    /// The glass's texture is not one a livery changes: nothing goes onto the windows.
    No,
}

/// What the windows of a bus can take: `glass` its windows' slots (the painted texture each
/// shows, where its see-through comes from), `painted` the textures the livery paints.
pub fn windows_of(glass: &[(Option<u8>, SeeThrough)], painted: &[u8]) -> Option<Windows> {
    if glass.is_empty() {
        return None;
    }
    let ours = |k: Option<u8>| k.is_some_and(|k| painted.contains(&k));
    let print = glass.iter().any(|(t, s)| {
        ours(*t)
            && match s {
                SeeThrough::Own => true,
                SeeThrough::Map(k) => painted.contains(k),
                SeeThrough::Fixed => false,
            }
    });
    Some(if print {
        Windows::Print
    } else if glass.iter().any(|(t, _)| ours(*t)) {
        Windows::Faint
    } else {
        Windows::No
    })
}

/// A bus of the livery's family: another bus file of its folder whose model takes the same
/// paint texture - the livery fits it as it is. Its `[CTC]` folder (the same as the bus's: the
/// livery comes on it by itself) and its slots per texture (file key, slot name).
#[derive(Clone, Debug)]
pub struct Kin {
    pub bus: String,
    pub name: String,
    pub ctc: Option<PathBuf>,
    pub slots: Vec<(String, String)>,
}

/// The family of `bus` among `buses` (file, name) of the same folder, for the textures `keys`.
///
/// `player_bus_path` gives the vehicle file the game spawns for a bus (the game's `spawn`).
pub fn family(root: &Path, bus: &str, buses: &[(String, String)], keys: &[String], player_bus_path: &dyn Fn(&Path, &str) -> Option<PathBuf>) -> Vec<Kin> {
    let folder = |f: &str| f.replace('\\', "/").rsplit_once('/').map(|x| x.0.to_ascii_lowercase()).unwrap_or_default();
    let mine = folder(bus);
    let mut out = Vec::new();
    for (file, name) in buses.iter().filter(|(f, _)| !f.eq_ignore_ascii_case(bus) && folder(f) == mine) {
        let Some(path) = player_bus_path(root, file) else { continue };
        let Ok(def) = omsi_vehicle::Vehicle::load(&path) else { continue };
        let Some(model_rel) = def.model.clone() else { continue };
        let Ok(model) = omsi_model::Model::load(&omsi_cfg::resolve_path(def.dir(), &model_rel)) else { continue };
        let slots: Vec<(String, String)> = model.ctc_textures.iter().map(|(slot, f)| (file_key(f), slot.clone())).filter(|(k, _)| keys.contains(k)).collect();
        if slots.is_empty() {
            continue;
        }
        let ctc = model.ctc.first().map(|c| omsi_cfg::resolve_path(def.dir(), &c.path));
        out.push(Kin { bus: file.clone(), name: name.clone(), ctc, slots });
    }
    out
}

/// Where the projects are kept.
pub fn projects_dir() -> PathBuf {
    omsi_launcher_lib::data_dir().join("liveries")
}

/// Beside a project's `project.json`: the project as it was at its last change, written at once
/// with every change (a stroke as it goes, at most five times a second) and gone again once
/// `project.json` has caught up. Found newer than `project.json` - the studio ended without
/// saving, a crash, the power - it is the project.
pub const TEMP_FILE: &str = "project.autosave.json";

/// `project.json` written whole, as the temporary file is: into a file beside it, which then
/// takes its place (a crash while it is written leaves the one before, not half of it).
pub fn write_project(dir: &Path, project: &Project) -> std::io::Result<()> {
    let part = dir.join("project.json.part");
    std::fs::write(&part, serde_json::to_vec_pretty(project).unwrap_or_default()).and_then(|_| std::fs::rename(&part, dir.join("project.json")))
}

/// The project in `dir`: its temporary file's when that is newer than `project.json` (or
/// there is no `project.json` yet, or it cannot be read), and whether it was.
pub fn load_project(dir: &Path) -> Option<(Project, bool)> {
    let saved = dir.join("project.json");
    let temp = dir.join(TEMP_FILE);
    let when = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let read = |p: &Path| std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Project>(&b).ok());
    if let Some(t) = when(&temp) {
        if when(&saved).is_none_or(|s| t > s) {
            if let Some(p) = read(&temp) {
                return Some((p, true));
            }
        }
    }
    // (a `project.json` that cannot be read - cut off as it was written - gives way to the
    // temporary file, which still has the project)
    read(&saved).map(|p| (p, false)).or_else(|| read(&temp).map(|p| (p, true)))
}

/// The projects there are, newest first.
pub fn projects() -> Vec<Project> {
    let mut v: Vec<Project> = std::fs::read_dir(projects_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| load_project(&e.path()).map(|p| p.0))
        .collect();
    v.sort_by(|a, b| b.saved.cmp(&a.saved));
    v
}

pub fn now_text() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}")
}

pub fn today() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    model::cti_date(s.div_euclid(86400))
}

/// The file name of a texture as a material names it, lower case and without its folder.
pub fn file_key(name: &str) -> String {
    name.trim().rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase()
}

/// One texture the livery paints: its file as the model names it (lower case, `key`), and the
/// `[CTCTexture]` slots of the bus's parts that take it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintTexture {
    pub key: String,
    pub file: String,
    /// (part, slot): 0 the front, then the rear sections.
    pub slots: Vec<(usize, String)>,
    /// The slots of its `<name>_#low` copy (the far LOD), which take the same picture.
    pub low: Vec<(usize, String)>,
}

/// The textures a livery paints on a bus of `parts` - per part its `[CTCTexture]` list and the
/// files of it a display shows (which keep theirs): every file once, with the slots that take
/// it in every part. An articulated bus's rear section has its own model, its own slots and
/// usually its own texture; a file both parts show is one texture with the slots of both.
pub fn paint_textures(parts: &[(&[(String, String)], &[String])]) -> Vec<PaintTexture> {
    let stem = |key: &str| key.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| key.to_string());
    let low_of = |key: &str| stem(key).strip_suffix("_#low").map(str::to_string);
    let mut out: Vec<PaintTexture> = Vec::new();
    for (p, (ctc, displays)) in parts.iter().enumerate() {
        for (slot, file) in ctc.iter() {
            let key = file_key(file);
            if key.is_empty() || displays.contains(&key) || low_of(&key).is_some() {
                continue;
            }
            match out.iter_mut().find(|t| t.key == key) {
                Some(t) => {
                    if !t.slots.iter().any(|s| s.0 == p && s.1.eq_ignore_ascii_case(slot)) {
                        t.slots.push((p, slot.clone()));
                    }
                }
                None => out.push(PaintTexture { key, file: file.trim().to_string(), slots: vec![(p, slot.clone())], low: Vec::new() }),
            }
        }
    }
    for (p, (ctc, _)) in parts.iter().enumerate() {
        for (slot, file) in ctc.iter() {
            let Some(of) = low_of(&file_key(file)) else { continue };
            if let Some(t) = out.iter_mut().find(|t| stem(&t.key) == of) {
                t.low.push((p, slot.clone()));
            }
        }
    }
    out
}

/// A part of the bus as the studio reads it: its type, the livery it shows (`part_scheme` for a
/// rear section), which of its meshes are shown and where each lies in the bus's frame.
pub struct PartIn<'a> {
    pub ty: &'a omsi_sim::VehicleType,
    pub scheme: Option<usize>,
    pub visible: Vec<bool>,
    pub xf: Vec<Mat4>,
}

/// The textures a livery paints on the bus of `parts` and its meshes as the bake takes them -
/// each with its part and its index there, every slot with its texture, and whether it is glass
/// or kept from the paint (`Role`) - and the meshes kept from the paint, by name (for the log).
pub fn bus_meshes<'a>(parts: &[PartIn<'a>]) -> (Vec<PaintTexture>, Vec<(usize, usize, MeshIn<'a>)>, Vec<String>) {
    // the painted textures: the parts' [CTCTexture]s, not a display's
    let displays: Vec<Vec<String>> = parts
        .iter()
        .map(|p| {
            p.ty.model
                .ctc_textures
                .iter()
                .map(|(_, f)| file_key(f))
                .filter(|key| p.ty.model.meshes.iter().flat_map(|m| m.materials.iter()).any(|o| file_key(&o.texture) == *key && (o.transmap.as_deref().is_some_and(|t| t.trim().starts_with("\\S:")) || o.use_script_texture.is_some() || o.use_text_texture.is_some())))
                .collect()
        })
        .collect();
    let lists: Vec<(&[(String, String)], &[String])> = parts.iter().zip(&displays).map(|(p, d)| (p.ty.model.ctc_textures.as_slice(), d.as_slice())).collect();
    let textures = paint_textures(&lists);
    let keys: Vec<&str> = textures.iter().map(|d| d.key.as_str()).collect();
    let mut meshes = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for (pi, p) in parts.iter().enumerate() {
        for (i, vm) in p.ty.meshes.iter().enumerate() {
            if !p.visible.get(i).copied().unwrap_or(false) || vm.data.indices.is_empty() {
                continue;
            }
            let def = &p.ty.model.meshes[vm.def_index];
            let role = Role::of_mesh(&def.file, turns_as_wheel(def), def.viewpoint == 0 || def.viewpoint & 1 != 0);
            let slots: Vec<Slot> = vm
                .materials
                .iter()
                .enumerate()
                .map(|(slot, m)| {
                    let target = keys.iter().position(|k| *k == file_key(&m.texture)).map(|k| k as u8);
                    let defs = || def.materials.iter().filter(|d| omsi_sim::vehicle::override_slot(&vm.materials, d) == Some(slot));
                    let blended = defs().any(|d| d.alpha == 2) || m.diffuse[3] < 0.99;
                    let see_through = SeeThrough::of(&m.texture, defs().find_map(|d| d.transmap.as_deref()), &keys);
                    let (target, glass, keep) = match role.slot(blended) {
                        SlotUse::Paint => (target, false, false),
                        SlotUse::Glass => (target, true, false),
                        SlotUse::Keep => (target, blended, true),
                        SlotUse::Skip => (None, blended, false),
                    };
                    Slot { target, glass, keep, see_through }
                })
                .collect();
            if !matches!(role, Role::Body | Role::Decal | Role::Glass) && vm.materials.iter().any(|m| keys.contains(&file_key(&m.texture).as_str())) {
                kept.push(format!("{} ({role:?})", def.file));
            }
            meshes.push((pi, i, MeshIn { positions: &vm.data.positions, normals: &vm.data.normals, uvs: &vm.data.uvs, indices: &vm.data.indices, ranges: &vm.data.ranges, slots, transform: p.xf[i] }));
        }
    }
    (textures, meshes, kept)
}

/// Whether the model turns mesh `def` as a wheel (`Wheel_Rotation_<n>_*`).
pub fn turns_as_wheel(def: &omsi_model::MeshDef) -> bool {
    def.animations.iter().any(|a| a.kind == Some(omsi_model::AnimKind::Rot) && a.variable.to_ascii_lowercase().starts_with("wheel_rotation_"))
}

/// What the project keeps of a save into the game that wrote `files`.
pub fn placed_after(project: &Project, files: Vec<String>) -> model::Placed {
    let nnnn = files.iter().find_map(|f| f.rsplit('/').next().and_then(|n| n.strip_prefix(&format!("~{}_", model::PREFIX))).and_then(|r| r.get(..4)).and_then(|n| n.parse().ok())).unwrap_or(1);
    let version = project.placed.as_ref().map(|p| p.version + 1).unwrap_or(1);
    model::Placed { name: project.name.clone(), nnnn, version, files }
}

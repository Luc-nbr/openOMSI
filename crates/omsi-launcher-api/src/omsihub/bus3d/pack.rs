//! Reading a bus as the game reads it and building its geometry pack for the page's renderer
//! (Omsi-Hub's Bus3D, the old app's `core/bus3d.ts`, ported onto openOMSI's readers).
//!
//! The old app read the `.bus`, the `model.cfg` and the `.o3d`s with readers of its own that
//! it had measured against Omsi.exe. openOMSI has those readers too, measured the same way,
//! and the game draws with them: the bus is read here with `omsi_sim::VehicleType::load` -
//! the `.bus`, its `model.cfg` (only the highest level of detail), every `[mesh]` through
//! `omsi_o3d` (an `.o3d` of any version, scrambled or not, an `.x`, a glTF ...), the winding
//! of each pack's exporter, the paint schemes of its `[CTC]` folders - and its coupled parts
//! (an articulated bus's rear section) as the game couples them. What the page then shows is
//! what the game drives.
//!
//! The old rules this keeps:
//! * the vertices of an `.o3d` stay in its own frame (x right, y up, z forward; the matrix in
//!   the file is the mesh's pivot, never applied); the page mirrors x;
//! * a mesh with viewpoint 4 only (the AI's) is left out; viewpoints 1 and 2 are kept with
//!   their viewpoint, the page shows the outside or the inside;
//! * each `[mesh]` line is an entry with its `[visible]` condition and its materials; two
//!   lines of one `.o3d` share its geometry (a piece);
//! * per o3d material: its colours (or `[matl_allcolor]`), the texture after the paint
//!   scheme's `[CTCTexture]` swap, `[matl_alpha]` (the last of the plain `[matl]`s that sets
//!   one, as Omsi.exe), `[matl_transmap]`, `[matl_envmap]` with its mask, `[matl_noZwrite]`,
//!   `[matl_noZcheck]`, `[matl_texadress_clamp]`, `[matl_alphascale]`, `[matl_change]` with
//!   its `[matl_item]`s; a text or script texture has nothing to show without the game and
//!   is left out, a `[texchanges]` slot shows its first entry;
//! * textures are looked for as the game looks for them (`omsi_texture::find_texture` in the
//!   vehicle's texture folders) and go to the page by id, never by path;
//! * the outside area and UV area per texture, and the share of the bus's shell (sides and
//!   roof, facing out) - the page's texture plan and the livery studio's paint targets;
//! * **descrambled geometry never goes to disk** (see `scramble`).

use super::scramble::{self, Scramble};
use super::tex::{self, TexInfo, TexSource};
use omsi_sim::vehicle::{override_slot, VehicleMesh};
use omsi_sim::VehicleType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A run of bytes in the pack's tail.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub off: u64,
    pub len: u64,
}

/// One material's triangles in a piece's indices (in indices, not bytes).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub mat: u32,
    pub first: u32,
    pub count: u32,
}

/// One mesh file of one part of the bus: its vertices (32 bytes each: position, normal, uv in
/// the `.o3d`'s frame) and its triangles grouped by material.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Piece {
    pub part: usize,
    /// The mesh file's name (no folder).
    pub file: String,
    pub n: u32,
    pub verts: Block,
    pub idx: Block,
    /// Bytes per index: 2 or 4.
    pub wide: u8,
    pub groups: Vec<Group>,
}

/// How a slot is transparent (`[matl_transmap]`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Trans {
    /// The transparency map (an index into the textures); none: the texture's own alpha.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tex: Option<usize>,
}

/// `[matl_envmap]`: how much it reflects, and the mask (an index into the textures).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Env {
    pub k: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<usize>,
}

/// A material slot's look: the plain one, or a `[matl_item]`'s.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Stand {
    /// Diffuse rgba, specular rgb, emissive rgb, specular power.
    pub d: [f32; 4],
    pub s: [f32; 3],
    pub e: [f32; 3],
    pub p: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tex: Option<usize>,
    /// 0 opaque, 1 alpha test, 2 blended.
    pub alpha: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trans: Option<Trans>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<Env>,
}

/// A material slot of an entry.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mat {
    #[serde(flatten)]
    pub base: Stand,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_zw: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_zc: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clamp: bool,
    /// `[matl_change]`: the items (item 1, 2 ...); which one shows is the look's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Stand>,
}

/// One `[mesh]` line in the pack.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub piece: usize,
    pub part: usize,
    /// Its place in the part's meshes (the game's `VehicleType::meshes`).
    pub mesh: usize,
    /// `[viewpoint]`: 0 always, bit 1 outside, bit 2 inside.
    pub vp: i32,
    /// `[isshadow]`: a shadow blob under the bus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blob: bool,
    /// Per o3d material; None for a slot with nothing to show (a text or script texture).
    pub mats: Vec<Option<Mat>>,
}

/// The JSON head of a pack: its offsets count from the start of the binary tail.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Head {
    pub version: u32,
    pub id: String,
    pub pieces: Vec<Piece>,
    pub entries: Vec<Entry>,
}

/// A texture of the pack with what the plan needs to size it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PackTex {
    #[serde(flatten)]
    pub info: TexInfo,
    /// Outside area (m²) and UV area of the triangles showing it.
    pub area: f32,
    pub uv: f32,
    /// How much of that area is the bus's shell (sides and roof facing out), m².
    #[serde(default)]
    pub skin: f32,
    /// The `[CTCTexture]` slot it is, if a paint scheme swaps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctc: Option<String>,
    /// The names it is asked for by, per part (lower case): a scheme swaps by them.
    #[serde(default)]
    pub names: Vec<(usize, String)>,
}

/// A camera of the `.bus` (`[add_camera_driver]`, `[add_camera_pax]`), in the o3d frame.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cam {
    pub pos: [f32; 3],
    pub dist: f32,
    pub fov: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// A part of the bus: its file, where it stands from the front part (o3d frame) and whether it
/// is turned round.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Part {
    pub bus: String,
    pub offset: [f32; 3],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub turned: bool,
}

/// What the page knows of a pack before its bytes: names, box, parts, cameras, textures.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub pack: String,
    pub bus: String,
    /// `[friendlyname]`: maker, type, the default paint's name.
    pub names: [String; 3],
    #[serde(default)]
    pub description: String,
    /// The whole train's box in the o3d frame.
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<Cam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passenger: Option<Cam>,
    pub textures: Vec<PackTex>,
    pub counts: Counts,
    /// Texture names the model asks for that are not there.
    #[serde(default)]
    pub missing: Vec<String>,
    /// The size of the pack in bytes.
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Counts {
    pub triangles: u64,
    pub pieces: usize,
    pub entries: usize,
    pub textures: usize,
    pub missing: usize,
    /// Meshes of the model's first level and how many of their files are missing here.
    pub meshes: usize,
    pub meshes_gone: usize,
    /// Pieces kept scrambled (descrambled when served).
    pub scrambled: usize,
}

/// A scrambled vertex block in a pack's tail, descrambled as it is served.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScrambledBlock {
    pub off: u64,
    pub len: u64,
    pub version: u8,
    pub flags: u8,
    pub key: u32,
}

/// A bus read and its pack built.
pub struct Built {
    pub head: Head,
    pub tail: Vec<u8>,
    pub manifest: Manifest,
    pub scrambled: Vec<ScrambledBlock>,
    pub sources: Vec<TexSource>,
    /// Every file and folder the pack depends on, with its size and time.
    pub stamps: Vec<(PathBuf, u64, u64)>,
    /// Whether it may be kept on disk (a scrambled mesh whose raw block could not be found
    /// would otherwise go there descrambled).
    pub cacheable: bool,
    pub read: Arc<BusRead>,
    pub ms: u64,
}

/// The bus as the game reads it: its front part and the parts coupled behind it.
pub struct BusRead {
    pub root: PathBuf,
    pub bus: String,
    pub vt: Arc<VehicleType>,
    /// The parts behind it, each with whether it is turned round.
    pub parts: Vec<(Arc<VehicleType>, bool)>,
}

impl BusRead {
    /// The front part and the rear parts, in order.
    pub fn all(&self) -> Vec<Arc<VehicleType>> {
        std::iter::once(self.vt.clone()).chain(self.parts.iter().map(|p| p.0.clone())).collect()
    }
}

/// The vehicle file the game drives for `bus` (an articulated bus's rear section: its front).
pub fn player_bus_path(root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
    let path = omsi_cfg::resolve_path(root, bus);
    let def = omsi_vehicle::Vehicle::load(&path)?;
    let fronts = if def.coupling_front.is_some() { omsi_vehicle::vehicle::front_sections_of(&path) } else { Vec::new() };
    if (fronts.is_empty() && !def.is_rear_section()) || def.is_rail() {
        return Ok(path);
    }
    fronts.first().cloned().ok_or_else(|| anyhow::anyhow!("{} is a rear section", path.display()))
}

/// The next part of the train behind a part with definition `def` (turned round: `rev`), as
/// the game couples it (`spawn::next_coupled`).
fn next_coupled(def: &omsi_vehicle::Vehicle, rev: bool) -> Option<(PathBuf, bool)> {
    let (file, flag) = if !rev { def.couple_back.as_ref() } else { def.couple_front.as_ref() }?;
    let path = omsi_cfg::resolve_path(def.dir(), file);
    let same = path.file_name().map(|f| f.to_ascii_lowercase()) == def.path.file_name().map(|f| f.to_ascii_lowercase());
    if !flag && same {
        return None;
    }
    Some((path, flag ^ rev))
}

/// Read `bus` (relative to the OMSI folder) as the game reads it.
pub fn read_bus(root: &Path, bus: &str) -> anyhow::Result<BusRead> {
    let path = player_bus_path(root, bus)?;
    let vt = Arc::new(VehicleType::load(root, &path)?);
    let mut parts = Vec::new();
    let mut lead = vt.clone();
    let mut rev = false;
    let mut seen = vec![vt.def.path.clone()];
    for _ in 0..8 {
        let Some((p, reversed)) = next_coupled(&lead.def, rev) else { break };
        if seen.iter().any(|s| omsi_vehicle::vehicle::same_file(s, &p)) {
            break;
        }
        match VehicleType::load(root, &p) {
            Ok(t) => {
                let t = Arc::new(t);
                parts.push((t.clone(), reversed));
                seen.push(p);
                lead = t;
                rev = reversed;
            }
            Err(e) => {
                omsi_launcher_lib::log_to_file(&format!("bus3d: the part {} of {bus}: {e:#}", p.display()));
                break;
            }
        }
    }
    Ok(BusRead { root: root.to_path_buf(), bus: bus.to_string(), vt, parts })
}

/// A file name without its folder.
fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A texture name the game takes for none (`omsi_scene::is_null_texture`).
pub fn is_null_texture(name: &str) -> bool {
    let n = name.trim();
    n.is_empty() || Path::new(&n.replace('\\', "/")).file_stem().is_some_and(|s| s.eq_ignore_ascii_case("null"))
}

/// The textures of a pack being built: by path, with the names they are asked for by.
pub struct TexList {
    pub list: Vec<PackTex>,
    pub sources: Vec<TexSource>,
    by_path: HashMap<String, Option<usize>>,
    pub missing: Vec<String>,
}

impl TexList {
    fn new() -> TexList {
        TexList { list: Vec::new(), sources: Vec::new(), by_path: HashMap::new(), missing: Vec::new() }
    }

    /// The texture `name` (as the model names it) of part `part`: looked for after the paint
    /// swap `subst` in `dirs`, as the game looks for it.
    fn find(&mut self, part: usize, name: &str, subst: &dyn Fn(&str) -> Option<String>, dirs: &[&Path]) -> Option<usize> {
        if is_null_texture(name) {
            return None;
        }
        let key = name.trim().to_ascii_lowercase();
        let swapped = subst(&key).unwrap_or_else(|| name.trim().to_string());
        let Some(path) = omsi_texture::find_texture(&swapped, dirs) else {
            if !self.missing.iter().any(|m| m.eq_ignore_ascii_case(name.trim())) {
                self.missing.push(name.trim().to_string());
            }
            return None;
        };
        let pk = path.to_string_lossy().to_lowercase();
        let i = match self.by_path.get(&pk) {
            Some(i) => *i,
            None => {
                let i = tex::probe(&path).map(|(info, src)| {
                    self.list.push(PackTex { info, area: 0.0, uv: 0.0, skin: 0.0, ctc: None, names: Vec::new() });
                    self.sources.push(src);
                    self.list.len() - 1
                });
                if i.is_none() {
                    omsi_launcher_lib::log_to_file(&format!("bus3d: {} cannot be read", path.display()));
                }
                self.by_path.insert(pk, i);
                i
            }
        }?;
        if !self.list[i].names.iter().any(|(p, n)| *p == part && *n == key) {
            self.list[i].names.push((part, key));
        }
        Some(i)
    }
}

/// The material slots of a part's mesh `vm` that the model.cfg speaks of, in their order.
fn overrides_of<'a>(vm: &'a VehicleMesh, slot: usize) -> Vec<&'a omsi_model::MaterialDef> {
    vm.overrides.iter().filter(|o| override_slot(&vm.materials, o) == Some(slot)).collect()
}

/// The look of one slot (the plain one, or an item's over it).
#[allow(clippy::too_many_arguments)]
fn stand_of(m: &omsi_o3d::Material, ov: &[&omsi_model::MaterialDef], alpha: u8, tex: Option<usize>, find: &mut dyn FnMut(&str) -> Option<usize>) -> Stand {
    let allcolor = ov.iter().find_map(|o| o.allcolor);
    let mut s = match allcolor {
        // diffuse rgba, ambient rgb, specular rgb, emissive rgb, power
        Some(c) => Stand { d: [c[0], c[1], c[2], c[3]], s: [c[7], c[8], c[9]], e: [c[10], c[11], c[12]], p: c[13], tex, alpha, trans: None, env: None },
        None => Stand { d: m.diffuse, s: m.specular, e: m.emissive, p: m.specular_power, tex, alpha, trans: None, env: None },
    };
    if let Some(t) = ov.iter().find_map(|o| o.transmap.clone()) {
        let t = t.trim();
        // (`\S:n` - a script texture as the mask - has nothing to show without the game)
        if !t.starts_with("\\S:") {
            s.trans = Some(Trans { tex: if t.is_empty() { None } else { find(t) } });
        }
    }
    if let Some((_, k)) = ov.iter().find_map(|o| o.envmap.clone()) {
        let mask = ov.iter().find_map(|o| o.envmap_mask.clone()).filter(|t| !t.trim().is_empty()).and_then(|t| find(&t));
        s.env = Some(Env { k, mask });
    }
    s
}

/// Whether a slot shows a script's or a text's picture, or a script texture as its mask:
/// nothing to show without the game.
fn generated(ov: &[&omsi_model::MaterialDef]) -> bool {
    ov.iter().any(|o| o.use_text_texture.is_some() || o.use_script_texture.is_some() || o.transmap.as_deref().is_some_and(|t| t.trim().starts_with("\\S:")))
}

/// The material of slot `k` of mesh `vm` of a part, its textures looked for with `find`.
fn mat_of(vt: &VehicleType, vm: &VehicleMesh, k: usize, find: &mut dyn FnMut(&str) -> Option<usize>) -> Option<Mat> {
    let m = vm.materials.get(k)?;
    let all = overrides_of(vm, k);
    let plain: Vec<&omsi_model::MaterialDef> = all.iter().copied().filter(|o| !o.item).collect();
    if generated(&plain) {
        return None;
    }
    // [matl_alpha]: the last plain [matl] (not a [matl_change]) that sets one, as Omsi.exe
    let mode = {
        let pl = || plain.iter().filter(|o| o.change.is_none());
        pl().rev().find(|o| o.alpha_set).or_else(|| pl().next()).or(plain.first()).map(|o| o.alpha).unwrap_or(0)
    };
    // (a dirt film is blended whatever the cfg says, as the game draws it)
    let dirt = plain.iter().any(|o| o.alphascale.as_deref().is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "dirt_norm" | "dirt_wiped")));
    let alpha = if dirt { 2 } else { mode.clamp(0, 2) as u8 };
    // the texture: a [texchanges] slot shows its first entry, a [matl_freetex] its key
    let tex = match vt.texchange(&m.texture) {
        Some(master) => master.entries.first().and_then(|e| find(e)),
        None => find(&m.texture),
    };
    let base = stand_of(m, &plain, alpha, tex, find);
    let items: Vec<Stand> = if plain.iter().any(|o| o.change.is_some()) {
        all.iter()
            .filter(|o| o.item)
            .map(|o| {
                let mut ov: Vec<&omsi_model::MaterialDef> = vec![*o];
                ov.extend(plain.iter().copied());
                let a = if o.alpha_set { o.alpha.clamp(0, 2) as u8 } else { alpha };
                stand_of(m, &ov, a, tex, find)
            })
            .collect()
    } else {
        Vec::new()
    };
    let clamp = plain.iter().any(|o| matches!(o.tex_address, omsi_model::TexAddress::Clamp | omsi_model::TexAddress::Border));
    Some(Mat { base, no_zw: plain.iter().any(|o| o.no_z_write), no_zc: plain.iter().any(|o| o.no_z_check), clamp, items })
}

/// 4-aligned binary tail of a pack.
#[derive(Default)]
pub struct Tail(pub Vec<u8>);

impl Tail {
    pub fn add(&mut self, b: &[u8]) -> Block {
        let off = self.0.len() as u64;
        self.0.extend_from_slice(b);
        while self.0.len() % 4 != 0 {
            self.0.push(0);
        }
        Block { off, len: b.len() as u64 }
    }
}

/// The o3d frame (x, y up, z forward) of a point of the game's (x, y forward, z up).
fn o3d(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], v[1]]
}

/// A camera of the `.bus` in the o3d frame.
fn cam_of(c: &omsi_vehicle::vehicle::Camera) -> Cam {
    Cam { pos: o3d(c.pos), dist: c.dist, fov: c.fov, yaw: c.yaw, pitch: c.pitch }
}

/// Build the pack of a bus read with `read_bus`. The offsets of the parts behind come from
/// their couplings (the parts straight behind each other).
pub fn build(read: Arc<BusRead>) -> anyhow::Result<Built> {
    let t0 = std::time::Instant::now();
    let types = read.all();
    let mut stamps: Vec<(PathBuf, u64, u64)> = Vec::new();
    let stamp = |p: &Path, stamps: &mut Vec<(PathBuf, u64, u64)>| {
        if stamps.iter().any(|s| s.0 == p) {
            return;
        }
        let (size, mtime) = tex::stamp(p).unwrap_or((u64::MAX, u64::MAX));
        stamps.push((p.to_path_buf(), size, mtime));
    };
    // the parts' places: each hangs at its coupling on the one in front
    let mut parts = vec![Part { bus: read.bus.clone(), offset: [0.0; 3], turned: false }];
    {
        let mut v = super::look::instance(&read, None, &[]);
        let mut at = [0.0f32; 3];
        for (k, t) in v.trailers.iter_mut().enumerate() {
            let (back, front) = t.couplings();
            let d = o3d([back.x - front.x, back.y - front.y, back.z - front.z]);
            at = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
            let rel = rel_path(&read.root, &read.parts[k].0.def.path);
            parts.push(Part { bus: rel, offset: at, turned: read.parts[k].1 });
        }
    }
    let mut texs = TexList::new();
    let mut tail = Tail::default();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut scrambled: Vec<ScrambledBlock> = Vec::new();
    let mut cacheable = true;
    let mut piece_of: HashMap<(usize, String, bool), Option<usize>> = HashMap::new();
    // per piece: its vertices in the o3d frame (for the areas and the box), in memory only
    let mut floats: Vec<Vec<f32>> = Vec::new();
    let mut counts = Counts::default();
    for (p, vt) in types.iter().enumerate() {
        stamp(&vt.def.path, &mut stamps);
        if let Some(m) = vt.def.model.as_ref() {
            stamp(&omsi_cfg::resolve_path(vt.def.dir(), m), &mut stamps);
        }
        // (a mesh file that comes or goes changes its folder)
        stamp(vt.def.dir(), &mut stamps);
        stamp(&vt.model_dir, &mut stamps);
        let dirs = vt.texture_dirs(&read.root);
        let dirs_ref: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
        for d in &dirs {
            stamp(d, &mut stamps);
        }
        let subst = vt.default_substitutions(&read.root);
        // the model's first level and how many of its meshes are missing here
        let first: usize = if vt.model.lods.is_empty() { 0 } else { vt.model.lods.get(1).map(|l| l.first_mesh).unwrap_or(vt.model.meshes.len()) - vt.model.lods[0].first_mesh };
        counts.meshes += first;
        counts.meshes_gone += first.saturating_sub(vt.meshes.len());
        for (i, vm) in vt.meshes.iter().enumerate() {
            let vp = vm.viewpoint;
            if vp != 0 && vp & 3 == 0 {
                continue;
            }
            let def = &vt.model.meshes[vm.def_index];
            stamp(&vm.file, &mut stamps);
            let key = (p, vm.file.to_string_lossy().to_lowercase(), vm.keep_winding);
            let piece = match piece_of.get(&key) {
                Some(x) => *x,
                None => {
                    let made = piece_from(vt, i, p, &mut tail, &mut scrambled, &mut cacheable);
                    let x = made.map(|(pc, f)| {
                        pieces.push(pc);
                        floats.push(f);
                        pieces.len() - 1
                    });
                    piece_of.insert(key, x);
                    x
                }
            };
            let Some(piece) = piece else { continue };
            let swap = |k: &str| subst.get(k).cloned();
            let mut find = |name: &str| texs.find(p, name, &swap, &dirs_ref);
            let mats: Vec<Option<Mat>> = (0..vm.materials.len()).map(|k| mat_of(vt, vm, k, &mut find)).collect();
            entries.push(Entry { piece, part: p, mesh: i, vp, blob: def.is_shadow, mats });
        }
        // the paint schemes' files: a new livery changes the looks, not the pack (`look`)
    }
    // the [CTCTexture] slots
    for (p, vt) in types.iter().enumerate() {
        for (slot, default) in &vt.model.ctc_textures {
            let d = default.trim().to_ascii_lowercase();
            for t in texs.list.iter_mut() {
                if t.ctc.is_none() && t.names.iter().any(|(q, n)| *q == p && *n == d) {
                    t.ctc = Some(slot.clone());
                }
            }
        }
    }
    areas(&pieces, &entries, &tail.0, &floats, &mut texs.list);
    // the box: the parts' [boundingbox]es, else the vertices measured
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (k, vt) in types.iter().enumerate() {
        if let Some(bb) = vt.def.bounding_box {
            let size = o3d([bb[0], bb[1], bb[2]]);
            let mid = o3d([bb[3], bb[4], bb[5]]);
            for a in 0..3 {
                lo[a] = lo[a].min(mid[a] - size[a].abs() * 0.5 + parts[k].offset[a]);
                hi[a] = hi[a].max(mid[a] + size[a].abs() * 0.5 + parts[k].offset[a]);
            }
        }
    }
    if lo[0] > hi[0] {
        let (l, h) = measured_box(&pieces, &floats, &parts);
        lo = l;
        hi = h;
    }
    counts.pieces = pieces.len();
    counts.entries = entries.len();
    counts.textures = texs.list.len();
    counts.missing = texs.missing.len();
    counts.scrambled = scrambled.len();
    counts.triangles = pieces.iter().map(|p| p.groups.iter().map(|g| g.count as u64 / 3).sum::<u64>()).sum();
    for s in &texs.sources {
        stamp(&s.path, &mut stamps);
    }
    stamps.sort_by(|a, b| a.0.cmp(&b.0));
    // the pack's id: the bus and everything it was read from
    let mut idtext = format!("b3d-2|{}", read.bus.to_lowercase());
    for (p, s, m) in &stamps {
        idtext.push_str(&format!("|{}|{s}|{m}", p.to_string_lossy().to_lowercase()));
    }
    let id = tex::id_of(&idtext);
    let vt = &read.vt;
    let names = [vt.def.manufacturer.clone(), vt.def.type_name.clone(), vt.def.default_paint.clone()];
    let driver = vt.def.cameras_driver.get(vt.def.camera_std).or(vt.def.cameras_driver.first()).map(cam_of);
    let passenger = vt.def.cameras_pax.first().map(cam_of).or_else(|| read.parts.iter().find_map(|(t, _)| t.def.cameras_pax.first().map(cam_of)));
    let head = Head { version: 2, id: id.clone(), pieces, entries };
    let manifest = Manifest {
        version: 2,
        pack: id,
        bus: read.bus.clone(),
        names,
        description: vt.def.description.trim().to_string(),
        min: lo,
        max: hi,
        parts,
        driver,
        passenger,
        textures: texs.list,
        counts,
        missing: texs.missing,
        bytes: 0,
    };
    Ok(Built { head, tail: tail.0, manifest, scrambled, sources: texs.sources, stamps, cacheable, read, ms: t0.elapsed().as_millis() as u64 })
}

/// A path relative to the OMSI folder (as the bus step names buses), else as it is.
pub fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// The piece of mesh `i` of part `p`: its vertices into the tail (as the file has them when
/// it is an `.o3d`, so that a scrambled block stays scrambled), its indices by material.
fn piece_from(vt: &VehicleType, i: usize, p: usize, tail: &mut Tail, scrambled: &mut Vec<ScrambledBlock>, cacheable: &mut bool) -> Option<(Piece, Vec<f32>)> {
    let vm = &vt.meshes[i];
    let data = vt.mesh_data(i)?;
    let n = data.positions.len();
    if n == 0 || data.indices.is_empty() || data.ranges.is_empty() {
        return None;
    }
    // the vertices in the o3d frame (x, y up, z forward): in memory, for the areas and the box
    let mut f = Vec::with_capacity(n * 8);
    for k in 0..n {
        let (q, nn, uv) = (data.positions[k], data.normals.get(k).copied().unwrap_or_default(), data.uvs.get(k).copied().unwrap_or_default());
        f.extend_from_slice(&[q.x, q.z, q.y, nn.x, nn.z, nn.y, uv.x, uv.y]);
    }
    let bytes = if vm.file.extension().is_some_and(|e| e.eq_ignore_ascii_case("o3d")) { omsi_cfg::vfs::read(&vm.file).ok() } else { None };
    let raw = bytes.as_deref().and_then(scramble::vertex_block).filter(|(_, count, _)| *count == n);
    let verts = match (raw, bytes.as_deref()) {
        (Some((at, count, s)), Some(b)) => {
            let block = tail.add(&b[at..at + count * 32]);
            if let Some(s) = s {
                scrambled.push(ScrambledBlock { off: block.off, len: block.len, version: s.version, flags: s.flags, key: s.key });
            }
            block
        }
        _ => {
            // (a scrambled .o3d whose block was not where it should be: the pack is not kept)
            if bytes.as_deref().is_some_and(|b| b.len() > 8 && b[2] >= 4 && b[4..8] != [0xFF; 4]) {
                *cacheable = false;
            }
            let mut b = Vec::with_capacity(n * 32);
            for x in &f {
                b.extend_from_slice(&x.to_le_bytes());
            }
            tail.add(&b)
        }
    };
    let wide: u8 = if n <= 0xFFFF { 2 } else { 4 };
    let mut ib = Vec::with_capacity(data.indices.len() * wide as usize);
    for &x in &data.indices {
        if wide == 2 {
            ib.extend_from_slice(&(x as u16).to_le_bytes());
        } else {
            ib.extend_from_slice(&x.to_le_bytes());
        }
    }
    let idx = tail.add(&ib);
    let groups = data.ranges.iter().map(|&(first, count, mat)| Group { mat, first, count }).collect();
    Some((Piece { part: p, file: file_name(&vm.file), n: n as u32, verts, idx, wide, groups }, f))
}

/// The outside area, UV area and shell share of every texture (the old app's surfaces): the
/// triangles of the entries seen from outside, not the shadow blobs; a texture the cfg names
/// beside a slot's own (its transparency map, its reflection mask) gets the slot's area. The
/// shell is what lies within 25 cm of the bus's rim (the 98th percentile of |x|, the 99th of
/// the height over the outside vertices) and faces out.
fn areas(pieces: &[Piece], entries: &[Entry], tail: &[u8], floats: &[Vec<f32>], texs: &mut [PackTex]) {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let outside = |e: &&Entry| (e.vp == 0 || e.vp & 1 != 0) && !e.blob;
    for e in entries.iter().filter(outside) {
        let f = &floats[e.piece];
        for o in (0..f.len()).step_by(8 * 16) {
            xs.push(f[o].abs());
            ys.push(f[o + 1]);
        }
    }
    let q = |v: &mut Vec<f32>, k: f32| {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            f32::INFINITY
        } else {
            v[((v.len() - 1) as f32 * k) as usize]
        }
    };
    let x_rim = q(&mut xs, 0.98) - 0.25;
    let y_rim = q(&mut ys, 0.99) - 0.25;
    let mut done: std::collections::HashSet<(usize, u32)> = std::collections::HashSet::new();
    for e in entries.iter().filter(outside) {
        let pc = &pieces[e.piece];
        let f = &floats[e.piece];
        let idx = |k: usize| -> usize {
            let o = pc.idx.off as usize + k * pc.wide as usize;
            if pc.wide == 2 {
                u16::from_le_bytes([tail[o], tail[o + 1]]) as usize
            } else {
                u32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]) as usize
            }
        };
        for g in &pc.groups {
            let Some(Some(m)) = e.mats.get(g.mat as usize) else { continue };
            if !done.insert((e.piece, g.mat)) {
                continue;
            }
            let (mut a, mut u, mut s) = (0f64, 0f64, 0f64);
            for t in (g.first as usize..(g.first + g.count) as usize).step_by(3) {
                let (i0, i1, i2) = (idx(t) * 8, idx(t + 1) * 8, idx(t + 2) * 8);
                if i2.max(i1).max(i0) + 8 > f.len() {
                    continue;
                }
                let v = |i: usize| [f[i], f[i + 1], f[i + 2]];
                let (p0, p1, p2) = (v(i0), v(i1), v(i2));
                let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                let c = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let area = 0.5 * ((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) as f64).sqrt();
                let (su, sv, tu, tv) = (f[i1 + 6] - f[i0 + 6], f[i1 + 7] - f[i0 + 7], f[i2 + 6] - f[i0 + 6], f[i2 + 7] - f[i0 + 7]);
                let uv = 0.5 * ((su * tv - sv * tu) as f64).abs();
                if !area.is_finite() || !uv.is_finite() {
                    continue;
                }
                a += area;
                u += uv;
                let mx = (p0[0] + p1[0] + p2[0]) / 3.0;
                let my = (p0[1] + p1[1] + p2[1]) / 3.0;
                let nx = (f[i0 + 3] + f[i1 + 3] + f[i2 + 3]) / 3.0;
                let ny = (f[i0 + 4] + f[i1 + 4] + f[i2 + 4]) / 3.0;
                if (mx.abs() >= x_rim && nx * mx.signum() > 0.3) || (my >= y_rim && ny > 0.3) {
                    s += area;
                }
            }
            let mut give = |t: Option<usize>, with_skin: bool| {
                if let Some(t) = t.and_then(|t| texs.get_mut(t)) {
                    t.area += a as f32;
                    t.uv += u as f32;
                    if with_skin {
                        t.skin += s as f32;
                    }
                }
            };
            for st in std::iter::once(&m.base).chain(m.items.iter()) {
                give(st.tex, std::ptr::eq(st, &m.base));
                give(st.trans.as_ref().and_then(|t| t.tex), false);
                give(st.env.as_ref().and_then(|e| e.mask), false);
            }
        }
    }
    for t in texs.iter_mut() {
        t.area = (t.area * 1e4).round() / 1e4;
        t.uv = (t.uv * 1e6).round() / 1e6;
        t.skin = (t.skin * 1e4).round() / 1e4;
    }
}

/// The box of the outside vertices, the 1st to the 99th percentile each way (helpers parked
/// far off do not count): for a bus without a `[boundingbox]`.
fn measured_box(pieces: &[Piece], floats: &[Vec<f32>], parts: &[Part]) -> ([f32; 3], [f32; 3]) {
    let mut ax: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (pc, f) in pieces.iter().zip(floats) {
        let off = parts.get(pc.part).map(|p| p.offset).unwrap_or([0.0; 3]);
        for o in (0..f.len()).step_by(64) {
            for a in 0..3 {
                ax[a].push(f[o + a] + off[a]);
            }
        }
    }
    if ax[0].len() < 8 {
        return ([-1.25, 0.0, -6.0], [1.25, 3.0, 6.0]);
    }
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for a in 0..3 {
        ax[a].sort_by(|x, y| x.total_cmp(y));
        let n = ax[a].len() - 1;
        lo[a] = ax[a][n / 100];
        hi[a] = ax[a][n * 99 / 100];
    }
    (lo, hi)
}

/// A built pack's bytes: `B3D2`, the head's length, the head (padded with spaces to four),
/// the tail.
pub fn pack_bytes(head: &Head, tail: &[u8]) -> Vec<u8> {
    let mut json = serde_json::to_vec(head).unwrap_or_default();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut out = Vec::with_capacity(8 + json.len() + tail.len());
    out.extend_from_slice(b"B3D2");
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(tail);
    out
}

/// A pack's head and where its tail begins; Err when the bytes are no pack.
pub fn read_pack(bytes: &[u8]) -> Result<(Head, usize), String> {
    if bytes.len() < 8 || &bytes[..4] != b"B3D2" {
        return Err("not a B3D2 pack".into());
    }
    let n = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if 8 + n > bytes.len() {
        return Err("the pack is cut off".into());
    }
    let head: Head = serde_json::from_slice(&bytes[8..8 + n]).map_err(|e| e.to_string())?;
    Ok((head, 8 + n))
}

/// Descramble, in `pack` (the bytes of a whole pack), the blocks kept scrambled.
pub fn descramble_pack(pack: &mut [u8], tail_at: usize, blocks: &[ScrambledBlock]) {
    for b in blocks {
        let (s, e) = (tail_at + b.off as usize, tail_at + (b.off + b.len) as usize);
        if e <= pack.len() {
            scramble::descramble_bytes(&mut pack[s..e], Scramble { version: b.version, flags: b.flags, key: b.key });
        }
    }
}

//! The bus at rest in a livery: which meshes show, which `[matl_item]` each switched slot
//! shows, how transparent each `[matl_alphascale]` slot is, where an animated part stands and
//! which textures the livery swaps - as the game puts the bus down (the showroom's `made`):
//! the livery's variables and the bus options set before its scripts' `{init}`, its parts
//! coupled on, three frames of its scripts, the options once more and a frame.
//!
//! The old app had a calculator of its own for this (`core/oscrust.ts`, `busrust.ts`) and
//! rules for what it could not work out; here the game's own scripts run (`omsi_sim`), so the
//! page shows what the game shows.

use super::pack::{BusRead, Head, Manifest};
use super::tex::{self, TexInfo, TexSource};
use omsi_sim::{SimClock, VehicleHost, VehicleInstance, VehicleType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A livery's look of a pack's entries.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Look {
    /// The livery (None: the model's own textures).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<String>,
    /// One character per entry: '1' shown, '0' hidden.
    pub visible: String,
    /// `entry * 4096 + slot` -> the `[matl_item]` shown (1 the first; none: the slot's own).
    #[serde(default)]
    pub items: BTreeMap<String, u32>,
    /// `entry * 4096 + slot` -> its `[matl_alphascale]` (only where it is not 1).
    #[serde(default)]
    pub alpha: BTreeMap<String, f32>,
    /// `entry` -> where its animation holds it at rest (column-major, o3d frame), where that
    /// is not where the file has it.
    #[serde(default)]
    pub xf: BTreeMap<String, [f32; 16]>,
    /// The pack's textures the livery swaps: (index into the manifest's textures, the file).
    #[serde(default)]
    pub textures: Vec<(usize, TexInfo)>,
    /// The bus options it wears.
    #[serde(default)]
    pub options: Vec<(String, f32)>,
    pub ms: u64,
}

/// The paint scheme named `paint` (or numbered so) of `vt`, as the game picks it for
/// `--paint`; none named: the model's own textures.
pub fn paint_scheme(vt: &VehicleType, paint: Option<&str>) -> Option<usize> {
    let asked = paint.map(str::trim).filter(|p| !p.is_empty())?;
    vt.paint_schemes.iter().position(|s| s.name.eq_ignore_ascii_case(asked)).or_else(|| asked.parse::<usize>().ok().filter(|i| *i < vt.paint_schemes.len()))
}

/// The paint scheme of a coupled part for the lead's (`omsihub_game::spawn::part_scheme`):
/// the part's scheme of the same name, else the same place in its list.
pub fn part_scheme(lead: &VehicleType, scheme: Option<usize>, part: &VehicleType) -> Option<usize> {
    let i = scheme?;
    let named = lead.paint_schemes.get(i).and_then(|s| part.paint_schemes.iter().position(|p| p.name.eq_ignore_ascii_case(&s.name)));
    named.or_else(|| (i < part.paint_schemes.len()).then_some(i))
}

/// Midsummer noon: the light every look and every photo is taken in.
fn noon() -> SimClock {
    SimClock { year: 2000, day_of_year: 173, time: 12.5 * 3600.0, ..Default::default() }
}

/// The bus as the game puts it down in paint scheme `scheme` with the bus options `options`.
pub fn instance(read: &BusRead, scheme: Option<usize>, options: &[(String, f32)]) -> VehicleInstance {
    let mut host = VehicleHost::new(noon());
    host.paint_scheme = Some(scheme);
    host.start_vars = options.to_vec();
    let mut v = VehicleInstance::new(read.vt.clone(), host);
    v.apply_paint_vars(scheme);
    for (var, x) in options {
        v.set_var(var, *x);
    }
    for (t, reversed) in &read.parts {
        v.attach_trailer_ex(t.clone(), *reversed);
    }
    for _ in 0..3 {
        v.update(1.0 / 30.0);
    }
    if !options.is_empty() {
        for (var, x) in options {
            v.set_var(var, *x);
        }
        v.update(1.0 / 30.0);
    }
    v
}

/// A transform of the game's frame (x, y forward, z up) in the o3d frame (x, y up, z
/// forward): the y and z rows and columns swapped.
fn to_o3d(m: &[f32; 16]) -> [f32; 16] {
    let sw = |i: usize| match i {
        1 => 2,
        2 => 1,
        x => x,
    };
    let mut out = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[sw(c) * 4 + sw(r)] = m[c * 4 + r];
        }
    }
    out
}

fn identity(m: &[f32; 16]) -> bool {
    m.iter().enumerate().all(|(i, x)| (x - if i % 5 == 0 { 1.0 } else { 0.0 }).abs() < 1e-5)
}

/// The look of `paint` with the bus options `options`, and the files of the textures it
/// swaps.
pub fn look_of(read: &BusRead, head: &Head, manifest: &Manifest, paint: Option<&str>, options: &[(String, f32)]) -> (Look, Vec<TexSource>) {
    let t0 = std::time::Instant::now();
    let vt = &read.vt;
    let scheme = paint_scheme(vt, paint);
    let v = instance(read, scheme, options);
    let var = |name: &str| -> f32 { name.trim().parse::<f32>().ok().or_else(|| v.var(name)).unwrap_or(0.0) };
    let mut look = Look { paint: scheme.and_then(|i| vt.paint_schemes.get(i)).map(|s| s.name.clone()), options: options.to_vec(), ..Default::default() };
    let types: Vec<Arc<VehicleType>> = read.all();
    for (i, e) in head.entries.iter().enumerate() {
        let (props, xfs) = if e.part == 0 { (v.mesh_props.get(e.mesh), v.mesh_transforms.get(e.mesh)) } else { v.trailers.get(e.part - 1).map(|t| (t.mesh_props.get(e.mesh), t.mesh_transforms.get(e.mesh))).unwrap_or((None, None)) };
        look.visible.push(if props.is_none_or(|p| p.visible) { '1' } else { '0' });
        if let Some(m) = xfs.map(|m| m.to_cols_array()) {
            if !identity(&m) {
                look.xf.insert(i.to_string(), to_o3d(&m));
            }
        }
        let Some(vt_p) = types.get(e.part) else { continue };
        let vm = &vt_p.meshes[e.mesh];
        for (k, mat) in e.mats.iter().enumerate() {
            let Some(mat) = mat else { continue };
            let key = (i * 4096 + k).to_string();
            if let Some(a) = props.and_then(|p| p.slot_alpha.get(k)) {
                if (a - 1.0).abs() > 1e-4 {
                    look.alpha.insert(key.clone(), a.clamp(0.0, 1.0));
                }
            }
            if mat.items.is_empty() {
                continue;
            }
            // the item as Omsi.exe picks it: the variable rounded; 1 the first item
            let change = vm.overrides.iter().filter(|o| !o.item && omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(k)).find_map(|o| o.change.as_ref().map(|c| c.2.clone()));
            if let Some(name) = change {
                let x = var(&name);
                let n = if x.is_finite() { x.round_ties_even() } else { 0.0 };
                if n >= 1.0 && n as usize <= mat.items.len() {
                    look.items.insert(key, n as u32);
                }
            }
        }
    }
    // the textures the livery swaps, per part (a rear section has its own scheme of the name)
    let mut sources = Vec::new();
    if scheme.is_some() {
        for (p, vt_p) in types.iter().enumerate() {
            let sp = if p == 0 { scheme } else { part_scheme(vt, scheme, vt_p) };
            let Some(sp) = sp else { continue };
            let (subst, _) = vt_p.scheme_substitutions(sp);
            if subst.is_empty() {
                continue;
            }
            let dirs = vt_p.texture_dirs(&read.root);
            let dirs_ref: Vec<&std::path::Path> = dirs.iter().map(|d| d.as_path()).collect();
            for (ti, t) in manifest.textures.iter().enumerate() {
                if look.textures.iter().any(|(x, _)| *x == ti) {
                    continue;
                }
                let Some(file) = t.names.iter().filter(|(q, _)| *q == p).find_map(|(_, n)| subst.get(n)) else { continue };
                let Some(path) = omsi_texture::find_texture(file, &dirs_ref) else { continue };
                let Some((info, src)) = tex::probe(&path) else { continue };
                if info.id != t.info.id {
                    look.textures.push((ti, info));
                    sources.push(src);
                }
            }
        }
    }
    look.ms = t0.elapsed().as_millis() as u64;
    (look, sources)
}

/// The files a bus's looks depend on beyond its pack: per part the `.bus`, its scripts, its
/// constant files and variable lists, its `[CTC]` folders and every `.cti` in them.
pub fn look_sources(read: &BusRead) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for vt in read.all() {
        out.push(vt.def.path.clone());
        for p in vt.def.scripts.scripts.iter().chain(&vt.def.scripts.constfiles).chain(&vt.def.scripts.varlists).chain(&vt.def.scripts.stringvarlists) {
            out.push(p.clone());
        }
        for c in &vt.model.ctc {
            let d = omsi_cfg::resolve_path(vt.def.dir(), &c.path);
            for m in omsi_cfg::mirrored_dirs(&d) {
                for e in omsi_cfg::vfs::read_dir_paths(&m) {
                    if e.extension().is_some_and(|x| x.eq_ignore_ascii_case("cti")) {
                        out.push(e);
                    }
                }
                out.push(m);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_of_the_game_is_put_into_the_o3d_frame() {
        // a move 1 m forward (y) and 2 m up (z) in the game's frame
        let mut m = [0f32; 16];
        for i in [0, 5, 10, 15] {
            m[i] = 1.0;
        }
        m[13] = 1.0;
        m[14] = 2.0;
        let o = to_o3d(&m);
        // in the o3d frame: 2 m up (y), 1 m forward (z)
        assert_eq!((o[12], o[13], o[14]), (0.0, 2.0, 1.0));
        assert!(identity(&to_o3d(&[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.])));
        // a turn about the game's up axis (z) is a turn about the o3d's up axis (y)
        let (s, c) = (0.5f32.sin(), 0.5f32.cos());
        let rz = [c, s, 0., 0., -s, c, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
        let o = to_o3d(&rz);
        assert_eq!(o[5], 1.0);
        assert!((o[0] - c).abs() < 1e-6 && (o[10] - c).abs() < 1e-6);
    }
}

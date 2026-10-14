//! The bus ready to paint (the game program's studio's `prepare`, from the 3D view's read of
//! the bus): which textures its livery paints and their bases, what each mesh slot is to the
//! paint (paint, glass, kept), the bus's box and window line, the colour zones of each texture,
//! the maker's templates, the names its liveries have, its family and its `[CTC]` folders - kept
//! for the save into the game, which paints from the same shape and zones.
//!
//! The page bakes the textures itself, at their full size, on its GPU; here the zones only need
//! the outside's colours, from a small bake.

use super::*;
use crate::omsihub::bus3d::{cache, look, pack, tex};
use omsihub_livery::bake::{self, BusGeom, Outside, SeeThrough, Zones};
use omsihub_livery::model::BusDims;
use omsihub_livery::{export, paint, Kin, PartIn, Windows};
use glam::{Mat4, Vec3};
use std::path::Path;

/// A painted texture of the bus prepared.
#[derive(Clone)]
pub(crate) struct Target {
    pub index: u8,
    /// The model's file name for it, and the `[CTCTexture]` slots that take it per part.
    pub default: String,
    pub slots: Vec<(usize, String)>,
    pub base: PathBuf,
    pub template: Option<[PathBuf; 2]>,
    /// The size it is painted and written at.
    pub full: (u32, u32),
}

/// The bus prepared, for the save into the game.
pub(crate) struct Prepared {
    pub key: String,
    pub bus: String,
    pub geom: Arc<BusGeom>,
    pub outside: Arc<Outside>,
    pub zones: Arc<Vec<Zones>>,
    pub targets: Vec<Target>,
    /// Per part of the bus: its `[CTC]` folder and the start livery's variables.
    pub parts: Vec<(Option<PathBuf>, Vec<(String, f32)>)>,
    pub taken: Vec<String>,
    pub family: Vec<Kin>,
    /// What the page was told.
    pub answer: Value,
}

fn state() -> &'static Mutex<Option<Arc<Prepared>>> {
    static S: OnceLock<Mutex<Option<Arc<Prepared>>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// The bus prepared last.
pub(crate) fn current() -> Option<Arc<Prepared>> {
    state().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The prepared bus's measures.
pub(crate) fn dims() -> Result<BusDims, String> {
    current().map(|p| p.geom.dims).ok_or_else(|| "no bus prepared".to_string())
}

/// The names the prepared bus's liveries have.
pub(crate) fn taken() -> Vec<String> {
    current().map(|p| p.taken.clone()).unwrap_or_default()
}

/// Texels the zones are found from, over all the textures (they need only the outside's
/// colours, which the clusters sample anyway).
const ZONE_TEXELS: u64 = 3 << 20;

/// The bus frame (x right, y forward, z up) as the page's o3d frame (x right, y up, z forward).
fn o3d(v: Vec3) -> [f32; 3] {
    [v.x, v.z, v.y]
}

fn zones_json(z: &Zones) -> Value {
    json!({ "centres": z.centres, "lin": z.lin, "painted": z.painted, "group": z.group, "rel": z.rel, "share": z.share })
}

/// Prepare `bus` for a livery begun from `paint` in the way `start`, wearing the bus options
/// `options` (`[[var, value]]`); `buses` (`[[file, name]]`) the buses its family is looked for
/// among.
pub(crate) fn prepare(args: &Value) -> Answer {
    let t0 = std::time::Instant::now();
    let bus = arg_str(args, "bus");
    if bus.trim().is_empty() {
        return Err("no bus".into());
    }
    let paint_name = Some(arg_str(args, "paint")).filter(|p| !p.trim().is_empty());
    let start = arg_str(args, "start");
    let plain = matches!(start.as_str(), "effen" | "effenKleuren");
    let tinted = start == "effenKleuren";
    let mut options: Vec<(String, f32)> = args.get("options").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| Some((x.get(0)?.as_str()?.to_string(), x.get(1)?.as_f64()? as f32))).collect()).unwrap_or_default();
    options.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    let buses: Vec<(String, String)> = args.get("buses").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| Some((x.get(0)?.as_str()?.to_string(), x.get(1).and_then(Value::as_str).unwrap_or("").to_string()))).collect()).unwrap_or_default();
    let key = format!("{}|{}|{plain}|{tinted}|{options:?}", bus.to_lowercase(), paint_name.as_deref().unwrap_or("").to_lowercase());
    if let Some(p) = current().filter(|p| p.key == key) {
        return Ok(p.answer.clone());
    }
    let root = omsi_launcher_lib::root_dir().map_err(|e| format!("{e:#}"))?;
    let mut marks: Vec<(&str, u64)> = Vec::new();
    let mark = |marks: &mut Vec<(&'static str, u64)>, what: &'static str| marks.push((what, t0.elapsed().as_millis() as u64));
    let (read, head, manifest) = cache::read_of(&root, &bus)?;
    mark(&mut marks, "read");
    let vt = read.vt.clone();
    // the bus as the game puts it down in its start livery, with the livery's bus options
    let scheme = look::paint_scheme(&vt, paint_name.as_deref());
    let v = look::instance(&read, scheme, &options);
    let types = read.all();
    // each part's meshes where the page draws them: the rest pose, the part turned and moved
    let part_xf = |p: usize| -> Mat4 {
        let part = manifest.parts.get(p);
        let off = part.map(|x| x.offset).unwrap_or([0.0; 3]);
        let turn = if part.is_some_and(|x| x.turned) { Mat4::from_rotation_z(std::f32::consts::PI) } else { Mat4::IDENTITY };
        Mat4::from_translation(Vec3::new(off[0], off[2], off[1])) * turn
    };
    let mut list: Vec<PartIn> = Vec::new();
    for (p, ty) in types.iter().enumerate() {
        let (props, xfs) = if p == 0 { (&v.mesh_props, &v.mesh_transforms) } else {
            match v.trailers.get(p - 1) {
                Some(t) => (&t.mesh_props, &t.mesh_transforms),
                None => continue,
            }
        };
        let sch = if p == 0 { scheme } else { look::part_scheme(&vt, scheme, ty) };
        let at = part_xf(p);
        list.push(PartIn { ty, scheme: sch, visible: (0..ty.meshes.len()).map(|i| props.get(i).is_none_or(|x| x.visible)).collect(), xf: (0..ty.meshes.len()).map(|i| at * xfs.get(i).copied().unwrap_or(Mat4::IDENTITY)).collect() });
    }
    mark(&mut marks, "instance");
    let (textures, found, kept) = omsihub_livery::bus_meshes(&list);
    if textures.is_empty() {
        return Err(tr("This bus has no paint the game lets change ([CTC])."));
    }
    // what each slot of each mesh is to the paint, and the windows' slots
    let mut slots_of: HashMap<(usize, usize), Vec<bake::Slot>> = HashMap::new();
    let mut glass: Vec<(Option<u8>, SeeThrough)> = Vec::new();
    let mut meshes = Vec::with_capacity(found.len());
    for (p, i, m) in found {
        for (slot, s) in m.slots.iter().enumerate() {
            if s.glass && !s.keep && m.ranges.iter().any(|r| r.2 as usize == slot && r.1 > 0) && !glass.contains(&(s.target, s.see_through)) {
                glass.push((s.target, s.see_through));
            }
        }
        slots_of.insert((p, i), m.slots.clone());
        meshes.push(m);
    }
    let geom = BusGeom::build(&meshes);
    drop(meshes);
    mark(&mut marks, "geometry");
    let mut area = vec![0.0f32; textures.len()];
    for t in &geom.tris {
        for k in t.target.into_iter().chain(t.trans) {
            area[k as usize] += (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5;
        }
    }
    // the bases: the start livery's textures (each part's own), or the model's own
    let mut targets: Vec<(f32, Target, Option<PathBuf>)> = Vec::new();
    for (k, t) in textures.iter().enumerate() {
        if area[k] < 0.5 {
            continue;
        }
        let PartIn { ty, scheme: sch, .. } = &list[t.slots.first().map(|s| s.0).unwrap_or(0)];
        let find = |sch: Option<usize>| {
            let (subs, sdir) = sch.map(|i| ty.scheme_substitutions(i)).unwrap_or_default();
            let mut dirs: Vec<PathBuf> = sdir.iter().cloned().collect();
            dirs.extend(ty.texture_dirs(&root));
            let dir_refs: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
            let wanted = subs.get(&t.key).cloned().unwrap_or_else(|| t.file.clone());
            sdir.as_ref().map(|d| omsi_cfg::resolve_path(d, &wanted)).filter(|p| p.is_file()).or_else(|| omsi_texture::find_texture(&wanted, &dir_refs)).or_else(|| omsi_texture::find_texture(&t.file, &dir_refs))
        };
        let Some(base) = find(if plain { None } else { *sch }) else {
            omsi_launcher_lib::log_to_file(&format!("livery studio: the texture {} of {bus} was not found", t.file));
            continue;
        };
        let tint = if tinted && sch.is_some() { find(*sch).filter(|p| *p != base) } else { None };
        let slots = t.slots.iter().chain(&t.low).cloned().collect();
        let template = omsihub_livery::find_template(&root, &t.file);
        targets.push((area[k], Target { index: k as u8, default: t.file.clone(), slots, base, template, full: (0, 0) }, tint));
    }
    targets.sort_by(|a, b| b.0.total_cmp(&a.0));
    targets.truncate(8);
    if targets.is_empty() {
        return Err(tr("The paint textures of this bus were not found."));
    }
    let geom = Arc::new(geom);
    let outside = Arc::new(Outside::build(&geom.tris));
    mark(&mut marks, "outside");
    // per texture: its base at its full size and a small bake for its zones
    let mut kept_t: Vec<(Target, Option<PathBuf>, bake::Bake, omsi_texture::Image, bool, tex::TexInfo, f32)> = Vec::new();
    let mut texels = 0u64;
    for (_, mut t, tint) in targets {
        let img = omsi_texture::decode_file(&t.base).map_err(|e| format!("{}: {e}", t.base.display()))?;
        let (ow, oh) = export::paint_size(&geom, t.index, img.width, img.height);
        t.full = (ow, oh);
        let alpha = {
            let full = if (ow, oh) != (img.width, img.height) { omsi_texture::bc::resize(&img.rgba, img.width, img.height, ow, oh) } else { img.rgba.clone() };
            full.chunks_exact(4).any(|p| p[3] < 255)
        };
        let (mut w, mut h) = (ow, oh);
        while texels + w as u64 * h as u64 > ZONE_TEXELS && w.min(h) >= 256 {
            w /= 2;
            h /= 2;
        }
        let rgba = if (w, h) != (img.width, img.height) { omsi_texture::bc::resize(&img.rgba, img.width, img.height, w, h) } else { img.rgba };
        let small = bake::Bake::build(&geom.tris, t.index, w, h, 0, h, &outside);
        // (a texture nothing of the outside shows, an interior's, is not painted)
        if !small.first.iter().any(|s| s.flags & bake::OUTSIDE != 0) {
            continue;
        }
        texels += w as u64 * h as u64;
        let Some((info, src)) = tex::probe(&t.base) else { continue };
        cache::register(&[src]);
        let density = export::density_of(&geom, t.index, ow, oh);
        kept_t.push((t, tint, small, omsi_texture::Image { width: w, height: h, has_alpha: alpha, rgba }, alpha, info, density));
    }
    if kept_t.is_empty() {
        return Err(tr("The paint textures of this bus were not found."));
    }
    mark(&mut marks, "bakes");
    let painted: Vec<u8> = kept_t.iter().map(|k| k.0.index).collect();
    let windows = omsihub_livery::windows_of(&glass, &painted);
    let colours: Vec<Vec<[u8; 3]>> = kept_t.iter().map(|(_, _, b, base, ..)| paint::outside_colours(b, &base.rgba)).collect();
    let zones = Arc::new(Zones::of_targets(&colours));
    mark(&mut marks, "zones");
    // a plain start in a livery's colours: per paint group the livery's median there
    let mut by_group: std::collections::BTreeMap<u8, Vec<[u8; 3]>> = Default::default();
    for ((t, tint, small, base, ..), z) in kept_t.iter().zip(zones.iter()) {
        let Some(tint) = tint.as_ref() else { continue };
        let Ok(img) = omsi_texture::decode_file(tint) else { continue };
        let rgba = if (img.width, img.height) != (base.width, base.height) { omsi_texture::bc::resize(&img.rgba, img.width, img.height, base.width, base.height) } else { img.rgba };
        let cv = paint::Canvas::with(small.clone(), &base.rgba, z);
        for (i, smp) in cv.bake.first.iter().enumerate() {
            if smp.flags & bake::OUTSIDE == 0 || smp.flags & (bake::KEEP | bake::GLASS) != 0 || i % 3 != 0 {
                continue;
            }
            let (a, b, tt) = cv.mix[i];
            let tt = tt as f32 / 50_000.0;
            let zz = if tt >= 0.8 { a } else if tt <= 0.2 { b } else { continue };
            if zz == bake::BLACK || !cv.zones.is_paint(zz as usize) {
                continue;
            }
            by_group.entry(cv.zones.group[zz as usize]).or_default().push([rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]]);
        }
        let _ = t;
    }
    let start_colours: Vec<(u8, String)> = by_group
        .into_iter()
        .filter(|(_, v)| v.len() >= 16)
        .map(|(g, mut v)| {
            let mut med = [0u8; 3];
            for (k, m) in med.iter_mut().enumerate() {
                v.sort_by_key(|c| c[k]);
                *m = v[v.len() / 2][k];
            }
            (g, model::hex(med))
        })
        .collect();
    // the names the game knows for this bus, and where each part's .cti files lie
    let mut taken: Vec<String> = Vec::new();
    for PartIn { ty, .. } in &list {
        for p in &ty.paint_schemes {
            if !taken.iter().any(|n| n.eq_ignore_ascii_case(&p.name)) {
                taken.push(p.name.clone());
            }
        }
    }
    let parts: Vec<(Option<PathBuf>, Vec<(String, f32)>)> = list.iter().map(|PartIn { ty, scheme: sch, .. }| (ty.model.ctc.first().map(|c| omsi_cfg::resolve_path(ty.def.dir(), &c.path)), sch.map(|i| ty.paint_schemes[i].set_vars.clone()).unwrap_or_default())).collect();
    let keys: Vec<String> = kept_t.iter().map(|k| omsihub_livery::file_key(&k.0.default)).collect();
    mark(&mut marks, "start colours");
    let family = if buses.is_empty() { Vec::new() } else { omsihub_livery::family(&root, &bus, &buses, &keys, &|r, f| pack::player_bus_path(r, f).ok()) };
    // what every slot of the 3D view's pack is to the paint: [target, glass, keep, see-through
    // map's target], the targets numbered as the page gets them (-1: none)
    let pos = |k: Option<u8>| k.and_then(|k| painted.iter().position(|x| *x == k)).map(|x| x as i64).unwrap_or(-1);
    let roles: Vec<Value> = head
        .entries
        .iter()
        .map(|e| match slots_of.get(&(e.part, e.mesh)) {
            Some(slots) => Value::Array(
                (0..e.mats.len())
                    .map(|k| {
                        let s = slots.get(k).copied().unwrap_or_default();
                        let trans = match s.see_through {
                            SeeThrough::Map(m) if s.glass && !s.keep && Some(m) != s.target => pos(Some(m)),
                            _ => -1,
                        };
                        json!([pos(s.target), s.glass as u8, s.keep as u8, trans])
                    })
                    .collect(),
            ),
            None => Value::Null,
        })
        .collect();
    let d = geom.dims;
    let answer = json!({
        "key": tex::id_of(&key),
        "pack": manifest.pack,
        "dims": { "min": o3d(d.min), "max": o3d(d.max), "window": d.window, "windowHeight": d.window_height(), "middleX": d.middle_x() },
        "targets": kept_t.iter().zip(zones.iter()).map(|((t, _, _, _, alpha, info, density), z)| json!({
            "index": t.index, "file": t.default, "w": t.full.0, "h": t.full.1, "density": density, "alpha": alpha,
            "base": info, "template": t.template.is_some(), "zones": zones_json(z),
            "windowsOnly": geom.tris.iter().filter(|x| x.target == Some(t.index) || x.trans == Some(t.index)).all(|x| x.glass),
        })).collect::<Vec<_>>(),
        "roles": roles,
        "windows": windows.map(|w| match w { Windows::Print => "print", Windows::Faint => "faint", Windows::No => "no" }),
        "startColours": start_colours,
        "taken": taken,
        "family": family.iter().map(|k| json!({ "bus": k.bus, "name": k.name })).collect::<Vec<_>>(),
        "ms": t0.elapsed().as_millis() as u64,
        "marks": marks.iter().map(|(w, t)| json!([w, t])).collect::<Vec<_>>(),
    });
    omsi_launcher_lib::log_to_file(&format!(
        "livery studio: {bus} prepared in {} ms ({} triangles; painting {}; kept from the paint: {})",
        t0.elapsed().as_millis(),
        geom.tris.len(),
        kept_t.iter().map(|k| format!("{} {}x{}", k.0.default, k.0.full.0, k.0.full.1)).collect::<Vec<_>>().join(", "),
        if kept.is_empty() { "nothing".into() } else { kept.join(", ") }
    ));
    let prepared = Prepared { key, bus, geom, outside, zones, targets: kept_t.into_iter().map(|k| k.0).collect(), parts, taken, family, answer: answer.clone() };
    *state().lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(prepared));
    Ok(answer)
}

/// The maker's template of target `target` of the prepared bus at its full size: width and
/// height (u32), MU's median where MA paints (3 x f32, `paint::Template`), then RGBA - MU's
/// colour, MA's grey in alpha.
pub(crate) fn template(args: &Value) -> Result<Vec<u8>, String> {
    let p = current().ok_or("no bus prepared")?;
    let k = args.get("target").and_then(Value::as_u64).unwrap_or(0) as usize;
    let t = p.targets.get(k).ok_or("no such texture")?;
    let tp = t.template.as_ref().ok_or("no template")?;
    let (w, h) = t.full;
    let [ma, mu] = omsihub_livery::template_pictures(tp, w, h).ok_or("the template could not be read")?;
    let rows = paint::Template::rows(&ma, &mu, w, 0, h);
    let mut out = Vec::with_capacity(20 + (w * h * 4) as usize);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    for v in rows.mu_ref {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for (i, c) in rows.mu.iter().enumerate() {
        out.extend_from_slice(&[c[0], c[1], c[2], rows.ma[i]]);
    }
    Ok(out)
}

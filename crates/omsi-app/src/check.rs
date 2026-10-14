//! `openomsi --check-vehicle <file> [--root <folder>]`: a vehicle loaded exactly
//! as the game loads it - the .bus, its model, meshes, scripts and constfiles, the paint
//! schemes - with every warning and error the loaders say caught instead of scrolled past
//! in a terminal, and the model's textures looked for in the folders the game looks in.
//! The findings go to stdout as one JSON line, for the launcher's "Model test" page.
//! No window, no graphics device: a modder sees what is wrong with a model in seconds.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Everything logged while the vehicle loads.
struct Capture(Mutex<Vec<(log::Level, String, String)>>);

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push((r.level(), r.target().to_string(), r.args().to_string()));
        }
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

/// What the parts add up to.
#[derive(Default)]
struct Counts {
    meshes: usize,
    materials: usize,
    textures: usize,
    missing: usize,
}

/// The textures of one part's meshes looked for in the folders the game looks in, a
/// finding for each that is not there. (As the scene decides it: a slot whose picture is
/// made at run time - a text texture, a script texture, a [matl_freetex], a texture-change
/// master - and a null texture are not files that should be there.)
fn check_textures(ty: &omsi_sim::VehicleType, root: &Path, part: &str, findings: &mut Vec<Value>, counts: &mut Counts) {
    let dirs = ty.texture_dirs(root);
    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let mut seen = std::collections::HashSet::new();
    counts.meshes += ty.meshes.len();
    for vm in &ty.meshes {
        let file = vm.file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        for (slot, m) in vm.materials.iter().enumerate() {
            counts.materials += 1;
            let t = m.texture.trim();
            if t.is_empty() || omsi_scene::is_null_texture(t) || !seen.insert(t.to_ascii_lowercase()) {
                continue;
            }
            let ov: Vec<&omsi_model::MaterialDef> = vm.overrides.iter().filter(|o| omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(slot)).collect();
            let generated = ov.iter().any(|o| o.use_text_texture.is_some() || o.use_script_texture.is_some() || o.freetex.is_some()) || ty.texchange(t).is_some();
            if generated {
                continue;
            }
            if omsi_texture::find_texture(t, &dir_refs).is_none() {
                counts.missing += 1;
                findings.push(json!({"level": "warn", "what": "texture", "text": format!("{t}: not found (mesh {file})"), "hint": format!("looked in {}", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")), "part": part}));
            }
        }
    }
    counts.textures += seen.len();
}

/// `args`: what follows `--check-vehicle` on the command line.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Info);
    let mut file: Option<PathBuf> = None;
    let mut root: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = it.next().map(PathBuf::from),
            _ if file.is_none() => file = Some(PathBuf::from(a)),
            _ => {}
        }
    }
    let Some(file) = file else {
        println!("{}", json!({"ok": false, "findings": [{"level": "error", "what": "arguments", "text": "no vehicle file given"}]}));
        return Ok(());
    };
    let root = root.or_else(crate::startup::find_root).unwrap_or_else(|| PathBuf::from("."));
    omsi_cfg::add_content_root(root.clone());
    let bus = if file.is_absolute() { file } else { omsi_cfg::resolve_path(&root, &file.to_string_lossy()) };
    let started = std::time::Instant::now();
    let mut findings: Vec<Value> = Vec::new();
    let mut summary = json!({});
    // the lines logged so far belong to the part loaded last (the bus, then each part
    // coupled behind it): the part is written on them
    let mut logged = 0usize;
    let mut take_log = |part: &str, findings: &mut Vec<Value>, warnings: &mut usize, errors: &mut usize| {
        let log = CAPTURE.0.lock().unwrap_or_else(|e| e.into_inner());
        for (level, target, text) in log.iter().skip(logged) {
            if *level <= log::Level::Warn {
                if *level == log::Level::Error {
                    *errors += 1;
                } else {
                    *warnings += 1;
                }
                findings.push(json!({"level": level.to_string().to_lowercase(), "what": target.rsplit("::").next().unwrap_or(target), "text": text, "part": part}));
            }
        }
        logged = log.len();
    };
    let mut warnings = 0usize;
    let mut errors = 0usize;
    let mut parts: Vec<String> = Vec::new();
    let mut counts = Counts::default();
    let name_of = |p: &Path| p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let ok = match omsi_sim::VehicleType::load(&root, &bus) {
        Ok(ty) => {
            let part = name_of(&bus);
            check_textures(&ty, &root, &part, &mut findings, &mut counts);
            take_log(&part, &mut findings, &mut warnings, &mut errors);
            parts.push(part);
            let name = format!("{} {}", ty.def.manufacturer.trim(), ty.def.type_name.trim()).trim().to_string();
            let (schemes, vars) = (ty.paint_schemes.len(), ty.program.var_names().len());
            // the parts coupled behind it (a trailer, an articulated rear section): loaded
            // by the game with the bus, so checked with it - their meshes and textures are
            // the usual place of a warning the bus alone does not show
            let mut lead = std::sync::Arc::new(ty);
            let mut lead_rev = false;
            let mut seen = vec![lead.def.path.clone()];
            for _ in 0..crate::spawn::MAX_COUPLED_PARTS {
                let Some((path, reversed)) = crate::spawn::next_coupled(&lead.def, lead_rev, true) else { break };
                if crate::spawn::chain_has_part(&seen, &path) {
                    break;
                }
                let part = name_of(&path);
                match omsi_sim::VehicleType::load(&root, &path) {
                    Ok(t) => {
                        check_textures(&t, &root, &part, &mut findings, &mut counts);
                        take_log(&part, &mut findings, &mut warnings, &mut errors);
                        parts.push(part);
                        seen.push(path);
                        lead = std::sync::Arc::new(t);
                        lead_rev = reversed;
                    }
                    Err(e) => {
                        findings.push(json!({"level": "error", "what": "load", "text": format!("coupled behind {}: {e:#}", lead.def.path.file_name().unwrap_or_default().to_string_lossy()), "part": part}));
                        take_log(&part, &mut findings, &mut warnings, &mut errors);
                        errors += 1;
                        break;
                    }
                }
            }
            summary = json!({
                "name": name,
                "parts": parts,
                "meshes": counts.meshes,
                "materials": counts.materials,
                "textures": counts.textures,
                "textures_missing": counts.missing,
                "paint_schemes": schemes,
                "variables": vars,
            });
            true
        }
        Err(e) => {
            findings.push(json!({"level": "error", "what": "load", "text": format!("{e:#}"), "part": name_of(&bus)}));
            take_log(&name_of(&bus), &mut findings, &mut warnings, &mut errors);
            false
        }
    };
    let log = CAPTURE.0.lock().unwrap_or_else(|e| e.into_inner());
    println!(
        "{}",
        json!({
            "ok": ok && errors == 0,
            "file": bus.display().to_string(),
            "root": root.display().to_string(),
            "seconds": started.elapsed().as_secs_f32(),
            "summary": summary,
            "errors": errors + usize::from(!ok),
            "warnings": warnings,
            "log_lines": log.len(),
            "findings": findings,
        })
    );
    Ok(())
}

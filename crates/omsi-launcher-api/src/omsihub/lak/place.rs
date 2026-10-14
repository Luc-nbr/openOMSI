//! The livery into the game and out again (the game program's studio's `save`, `saved` and
//! `remove_from_game`): every painted texture at its full size by `omsihub_livery::export` -
//! the same files that studio writes - into openOMSI's content folder, never the OMSI 2 folder;
//! for the bus company its design booked and its bus painted.

use super::prepare::current;
use super::*;
use crate::omsihub::company::{self as company, words::eur};
use omsi_launcher_lib::company::livery as co_livery;
use omsihub_livery::export;
use std::sync::mpsc::Receiver;

/// A save on its way (or waiting for the game to close).
struct Saving {
    project: Project,
    name: String,
    company: Option<(String, Option<u32>)>,
    rx: Option<Receiver<export::Msg>>,
    job: Option<export::Job>,
    progress: (String, f32),
}

/// What the last save ended in, until the page has heard it.
#[derive(Default)]
struct Saves {
    now: Option<Saving>,
    ended: Option<Value>,
}

fn saves() -> &'static Mutex<Saves> {
    static S: OnceLock<Mutex<Saves>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Whether a game runs (the game reads the liveries as it loads a bus: a texture half written
/// then would be read half).
fn in_game() -> bool {
    omsi_launcher_lib::list_instances().iter().any(|i| i.running)
}

/// The company `id`'s name, what saving `name` costs it (its design, and painting `vehicle`)
/// and its cash; None when it is not the company open.
fn company_cost(id: &str, vehicle: Option<u32>, name: &str) -> Option<(String, i64, i64)> {
    company::with(|s| {
        let c = s.company.as_ref().filter(|c| c.id == id)?;
        let paint = vehicle.and_then(|v| c.vehicle(v)).map(|v| co_livery::paint_cost(c, v)).unwrap_or(0);
        Some((c.name.clone(), (co_livery::save_cost(c, name) + paint) as i64, c.cash as i64))
    })
}

/// What saving costs the company: `{ company, cost, cash, enough, label }` (null: no company).
pub(super) fn cost(args: &Value) -> Answer {
    let id = arg_str(args, "company");
    let vehicle = args.get("vehicle").and_then(Value::as_u64).map(|v| v as u32);
    let Some((name, cost, cash)) = company_cost(&id, vehicle, &arg_str(args, "name")) else { return Ok(Value::Null) };
    Ok(json!({
        "company": name, "cost": cost, "cash": cash, "enough": cash >= cost,
        "label": tr("Save · %{amount}").replace("%{amount}", &eur(cost as _)),
        "tip": tr("For %{company}: the design and the painting cost %{amount}; it has %{cash}.").replace("%{company}", &name).replace("%{amount}", &eur(cost as _)).replace("%{cash}", &eur(cash as _)),
    }))
}

/// Put the livery into the game: `project` as the page has it, under `name`, for the buses of
/// its family in `family` too; for the bus company `company` (its bus `vehicle`).
pub(super) fn place(args: &Value) -> Answer {
    let mut project: Project = arg(args, "project")?;
    let name = arg_str(args, "name").trim().to_string();
    let family: Vec<String> = arg::<Option<Vec<String>>>(args, "family")?.unwrap_or_default();
    let company_id = Some(arg_str(args, "company")).filter(|c| !c.is_empty());
    let vehicle = args.get("vehicle").and_then(Value::as_u64).map(|v| v as u32);
    if saves().lock().unwrap_or_else(|e| e.into_inner()).now.is_some() {
        return Err(tr("The livery is being saved."));
    }
    let p = current().ok_or_else(|| tr("Preparing the paint…"))?;
    if !p.bus.eq_ignore_ascii_case(&project.bus) {
        return Err(tr("Go back to your own bus to save the livery."));
    }
    if let Some(id) = company_id.as_deref() {
        if let Some((company, cost, cash)) = company_cost(id, vehicle, &name).filter(|x| x.2 < x.1) {
            return Err(tr("%{company} has %{cash}: not enough for the livery (%{amount}). A loan on the company's Finances page helps.").replace("%{company}", &company).replace("%{cash}", &eur(cash as _)).replace("%{amount}", &eur(cost as _)));
        }
    }
    let own = project.placed.as_ref().map(|x| x.name.clone());
    if let Some(e) = model::name_error(&name, p.taken.iter().filter(|n| own.as_ref().is_none_or(|o| !o.eq_ignore_ascii_case(n))).map(String::as_str)) {
        return Err(e.message());
    }
    let content = omsi_launcher_lib::content_dir().ok_or_else(|| tr("The content folder was not found."))?;
    let root = omsi_launcher_lib::root_dir().map_err(|e| format!("{e:#}"))?;
    // every part's [CTC] folder as the content folder has it
    let in_content = |ctc: &std::path::Path| -> Option<PathBuf> {
        if ctc.starts_with(&content) {
            Some(ctc.to_path_buf())
        } else {
            ctc.strip_prefix(&root).ok().map(|rel| content.join(rel))
        }
    };
    let mut parts = Vec::new();
    for (ctc, mut setvars) in p.parts.clone() {
        let Some(ctc) = ctc else {
            parts.push(None);
            continue;
        };
        // (the bus options chosen in the studio over the start livery's)
        for (var, v) in &project.options {
            match setvars.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(var)) {
                Some(x) => x.1 = *v,
                None => setvars.push((var.clone(), *v)),
            }
        }
        let ctc_dir = in_content(&ctc).ok_or_else(|| tr("This bus lies outside the game's folders; its livery cannot be saved."))?;
        if !export::inside(&ctc_dir, &content) {
            return Err(tr("This bus lies outside the game's folders; its livery cannot be saved."));
        }
        std::fs::create_dir_all(&ctc_dir).map_err(|e| format!("{}: {e}", ctc_dir.display()))?;
        parts.push(Some(export::Part { ctc_dir, setvars }));
    }
    if parts.first().is_none_or(|x| x.is_none()) {
        return Err(tr("This bus has no paint the game lets change ([CTC])."));
    }
    // the buses of its family it is saved for too, with a [CTC] folder of their own
    let mut extra: Vec<(usize, Vec<(String, String)>)> = Vec::new();
    for kin in p.family.iter().filter(|k| family.iter().any(|f| f.eq_ignore_ascii_case(&k.bus))) {
        let Some(ctc_dir) = kin.ctc.as_deref().and_then(in_content) else { continue };
        if parts.iter().flatten().any(|x| export::same_folder(&x.ctc_dir, &ctc_dir)) || !export::inside(&ctc_dir, &content) || std::fs::create_dir_all(&ctc_dir).is_err() {
            continue;
        }
        parts.push(Some(export::Part { ctc_dir, setvars: project.options.iter().map(|(k, v)| (k.clone(), *v)).collect() }));
        extra.push((parts.len() - 1, kin.slots.clone()));
    }
    let nnnn = project.placed.as_ref().map(|x| x.nnnn).unwrap_or_else(|| parts.iter().flatten().map(|x| export::next_number(&x.ctc_dir)).max().unwrap_or(1));
    let mirror = project.mirror.on.then(|| project.mirror.plane_x.unwrap_or(p.geom.dims.middle_x()));
    let job = export::Job {
        name: name.clone(),
        nnnn,
        date: omsihub_livery::today(),
        content,
        parts,
        targets: p
            .targets
            .iter()
            .map(|t| {
                let mut slots = t.slots.clone();
                for (k, slot) in &extra {
                    for (key, sname) in slot {
                        if *key == omsihub_livery::file_key(&t.default) && !slots.contains(&(*k, sname.clone())) {
                            slots.push((*k, sname.clone()));
                        }
                    }
                }
                export::Target { index: t.index, default: t.default.clone(), slots, base: t.base.clone(), template: t.template.clone() }
            })
            .collect(),
        geom: p.geom.clone(),
        outside: p.outside.clone(),
        zones: p.zones.clone(),
        layers: project.layers.clone(),
        pictures: pictures_of(&project.id),
        mirror,
        old: project.placed.as_ref().map(|x| x.files.clone()).unwrap_or_default(),
    };
    project.name = name.clone();
    project.family = family;
    let mut saving = Saving { project, name, company: company_id.map(|c| (c, vehicle)), rx: None, job: Some(job), progress: (String::from("Painting the textures…"), 0.0) };
    let queued = in_game();
    if !queued {
        start(&mut saving);
    }
    let mut s = saves().lock().unwrap_or_else(|e| e.into_inner());
    s.ended = None;
    s.now = Some(saving);
    Ok(json!({ "started": !queued, "queued": queued, "text": if queued { tr("The game is running: the livery is saved into it as soon as the game is closed.") } else { tr("Painting the textures…") } }))
}

/// The export of a save on a thread of its own.
fn start(s: &mut Saving) {
    let Some(job) = s.job.take() else { return };
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("livery export".into()).spawn(move || export::run(job, tx));
    s.rx = Some(rx);
}

/// How the save goes: `{ busy, queued, text, f }`, and once it ended `{ done: { project, text,
/// name, company } }` or `{ error }`.
pub(super) fn progress() -> Answer {
    let mut all = saves().lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = all.now.as_mut() else {
        return Ok(all.ended.take().unwrap_or_else(|| json!({ "busy": false })));
    };
    if s.rx.is_none() {
        if in_game() {
            return Ok(json!({ "busy": true, "queued": true, "text": tr("The game is running: the livery is saved into it as soon as the game is closed."), "f": 0.0 }));
        }
        start(s);
    }
    let mut end = None;
    if let Some(rx) = s.rx.as_ref() {
        loop {
            match rx.try_recv() {
                Ok(export::Msg::Progress(t, f)) => s.progress = (t, f),
                Ok(export::Msg::Done(files)) => {
                    end = Some(Ok(files));
                    break;
                }
                Ok(export::Msg::Failed(e)) => {
                    end = Some(Err(e));
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    end = Some(Err("the save stopped".into()));
                    break;
                }
            }
        }
    }
    let Some(end) = end else {
        return Ok(json!({ "busy": true, "text": tr(&s.progress.0), "f": s.progress.1 }));
    };
    let s = all.now.take().expect("a save");
    drop(all);
    let answer = match end {
        Ok(files) => saved(s, files),
        Err(e) => json!({ "error": tr("The livery was not saved: %{e}").replace("%{e}", &e) }),
    };
    Ok(answer)
}

/// The livery is in the game: kept in the project; for the bus company its design booked and
/// its bus painted in it.
fn saved(s: Saving, files: Vec<String>) -> Value {
    let Saving { mut project, name, company, .. } = s;
    project.placed = Some(omsihub_livery::placed_after(&project, files));
    project.saved = omsihub_livery::now_text();
    if let Ok(dir) = project_dir(&project.id) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = omsihub_livery::write_project(&dir, &project);
        let _ = std::fs::remove_file(dir.join(omsihub_livery::TEMP_FILE));
    }
    super::super::bus3d::photo::forget(&project.bus);
    let mut text = tr("'%{name}' is in the game: choose it on the bus step.").replace("%{name}", &name);
    let mut changed = None;
    if let Some((id, vehicle)) = company {
        let bus = project.bus.clone();
        let booked = company::with(|s| {
            if s.company.as_ref().is_none_or(|c| c.id != id) {
                return None;
            }
            let cost = company::act(s, |c| co_livery::save_design(c, &name, &bus)).ok()?;
            let mut t = tr("'%{name}' is the company's livery: %{amount} for its design.").replace("%{name}", &name).replace("%{amount}", &eur(cost));
            if let Some(v) = vehicle {
                if company::act(s, |c| co_livery::paint(c, v, &name)).is_ok() {
                    let number = s.company.as_ref().and_then(|c| c.vehicle(v)).map(|x| x.number.clone()).unwrap_or_default();
                    t = format!("{t} {}", tr("Bus %{n} is painted in it in the workshop.").replace("%{n}", &number));
                }
            }
            Some(t)
        });
        if let Some(t) = booked {
            text = t;
            changed = Some(id);
        }
    }
    omsi_launcher_lib::log_to_file(&format!("livery studio: '{name}' saved into the game for {}", project.bus));
    json!({ "done": { "project": project, "name": name, "text": text, "company": changed } })
}

/// Take the livery out of the game again: the files its last save wrote - only the studio's own,
/// only in the content folder, the `.cti` files first - and their folders once empty.
pub(super) fn remove(args: &Value) -> Answer {
    let mut project: Project = arg(args, "project")?;
    let Some(placed) = project.placed.clone() else { return Ok(json!({ "project": project })) };
    if in_game() {
        return Err(tr("Close the game first: it may be using the livery."));
    }
    let content = omsi_launcher_lib::content_dir().ok_or_else(|| tr("The content folder was not found."))?;
    let mut files = placed.files.clone();
    files.sort_by_key(|f| !f.to_ascii_lowercase().ends_with(".cti"));
    let mut left = Vec::new();
    for f in files {
        let p = content.join(&f);
        if !f.contains(model::PREFIX) || !export::inside(&p, &content) {
            continue;
        }
        if p.is_file() && std::fs::remove_file(&p).is_err() {
            left.push(f);
            continue;
        }
        let mut dir = p.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&content) && d.to_string_lossy().contains(model::PREFIX)) {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    let dir = project_dir(&project.id)?;
    if !left.is_empty() {
        project.placed = Some(model::Placed { files: left, ..placed });
        let _ = omsihub_livery::write_project(&dir, &project);
        return Err(tr("Some files of the livery could not be removed (is the game using them?)."));
    }
    omsi_cfg::content_changed();
    project.placed = None;
    let _ = std::fs::create_dir_all(&dir);
    let _ = omsihub_livery::write_project(&dir, &project);
    super::super::bus3d::photo::forget(&project.bus);
    Ok(json!({ "project": project, "text": tr("'%{name}' is no longer in the game.").replace("%{name}", &placed.name) }))
}

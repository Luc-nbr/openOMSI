//! The depot editor (`crates/omsi-launcher-ui/src/depoteditor.rs`): depot files (`.hof`) of the
//! player's own (`core::owndepot`), kept in `~/.openomsi/depots`, never in the OMSI 2 folder.
//! Its parts are tables: the destinations, the IBIS's stops, the routes with their stops, the
//! special and service trips, and the file itself.
//!
//! The file open is kept here (`OPEN`), as the game-drawn launcher keeps it: the page shows it
//! (`de_view`) and every change is one command (`de_edit`), made by `owndepot::Doc`'s own
//! functions - the problems of the file (`Doc::issues`) worked out again after each.

use super::arg;
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::linehof::{Coding, Role};
use omsi_launcher_lib::owndepot::{self, Doc, Place};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// The file being edited.
struct Open {
    key: String,
    path: PathBuf,
    doc: Doc,
    coding: Coding,
    blocks: Vec<(String, String)>,
    dirty: bool,
    /// When the file was written as it is open (the line editor writes it too).
    stamp: Option<SystemTime>,
}

/// The file open, and every map's folder with its depot file (the problems compare the name
/// with the map's).
struct State {
    open: Option<Open>,
    maps: Vec<(String, String)>,
}

static STATE: Mutex<State> = Mutex::new(State { open: None, maps: Vec::new() });

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "de_show" => (|| show(args))(),
        "de_open" => (|| open(&arg::<String>(args, "key")?))(),
        "de_view" => Ok(view(&state())),
        "de_edit" => (|| edit(args))(),
        "de_save" => save(),
        "de_delete" => delete(),
        "de_sources" => (|| sources(args))(),
        "de_make" => (|| make(args))(),
        _ => return None,
    })
}

fn written_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The player's depot files: `[{ key, name, map, termini }]`.
fn entries() -> Vec<Value> {
    owndepot::list(&owndepot::dir()).iter().map(|e| json!({ "key": e.key, "name": e.name, "map": e.map, "termini": e.termini.len() })).collect()
}

/// Open the player's depot file at `path` - not over changes to another not saved yet.
fn open_path(s: &mut State, path: &Path) -> Result<(), String> {
    if let Some(o) = s.open.as_ref().filter(|o| o.dirty) {
        if o.path == path {
            return Ok(());
        }
        return Err(format!("{}|{}", "%{name} has changes that are not saved: save it first", o.doc.hof.name));
    }
    let stamp = written_at(path);
    let l = owndepot::load(path)?;
    let key = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    s.open = Some(Open { key, path: path.to_path_buf(), doc: l.doc, coding: l.coding, blocks: l.blocks, dirty: false, stamp });
    Ok(())
}

/// The page shown (`depoteditor::draw`'s first frame): every map's folder and depot file
/// (`maps`), the file open read again when something else wrote it since - not over changes
/// not saved (`changed_elsewhere` says so then) - and the file `key` opened, else the first
/// when none is open. Answers `{ entries, view, changed_elsewhere, error }`.
fn show(args: &Value) -> Answer {
    let mut s = state();
    s.maps = arg::<Option<Vec<(String, String)>>>(args, "maps")?.unwrap_or_default();
    let mut changed_elsewhere = None;
    if let Some(o) = s.open.as_mut() {
        let now = written_at(&o.path);
        if now.is_some() && now != o.stamp {
            if o.dirty {
                changed_elsewhere = Some(o.doc.hof.name.clone());
            } else if let Ok(l) = owndepot::load(&o.path) {
                (o.doc, o.coding, o.blocks, o.stamp) = (l.doc, l.coding, l.blocks, now);
            }
        }
        // (deleted meanwhile: nothing open)
        if !o.path.is_file() && !o.dirty {
            s.open = None;
        }
    }
    let key = arg::<Option<String>>(args, "key")?.unwrap_or_default();
    let list = owndepot::list(&owndepot::dir());
    let want = owndepot::path_of(&owndepot::dir(), &key).or_else(|| s.open.is_none().then(|| list.first().map(|e| e.path.clone())).flatten());
    let mut error = None;
    if let Some(p) = want {
        if let Err(e) = open_path(&mut s, &p) {
            error = Some(e);
        }
    }
    Ok(json!({ "entries": entries(), "view": view(&s), "changed_elsewhere": changed_elsewhere, "error": error }))
}

fn open(key: &str) -> Answer {
    let mut s = state();
    let path = owndepot::path_of(&owndepot::dir(), key).ok_or_else(|| format!("{key}: not found"))?;
    open_path(&mut s, &path)?;
    Ok(view(&s))
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::IbisDisplay => "ibis",
        Role::FrontTop => "front_top",
        Role::FrontBottom => "front_bottom",
        Role::Side => "side",
        Role::RollerBlind => "roller_blind",
        Role::ClearName => "clear_name",
        Role::ExtraSign => "extra_sign",
        Role::Bitmap => "bitmap",
        Role::Special => "special",
        Role::Rear => "rear",
        Role::Fallback(_) => "fallback",
        Role::Other => "other",
    }
}

/// A problem's place: `[kind, index]`.
fn place_json(p: Place) -> Value {
    match p {
        Place::File => json!(["file", 0]),
        Place::Terminus(i) => json!(["terminus", i]),
        Place::Stop(i) => json!(["stop", i]),
        Place::Trip(i) => json!(["trip", i]),
    }
}

/// The file open as the page shows it (null when none is): its name, map, file, whether it is
/// saved; its destinations, stops, routes, service trip, notes and global texts; what each
/// text of a destination is for (`labels`: its role and the file's own note, and the most
/// characters its display shows); the line editor's blocks in it; and its problems.
fn view(s: &State) -> Value {
    let Some(o) = s.open.as_ref() else { return Value::Null };
    let d = &o.doc;
    let maps_hof = s.maps.iter().find(|(f, _)| f.eq_ignore_ascii_case(&d.map)).map(|m| m.1.clone()).unwrap_or_default();
    let labels: Vec<Value> = d
        .labels()
        .iter()
        .enumerate()
        .map(|(k, (role, note))| json!({ "role": role_name(*role), "note": note, "limit": owndepot::limit(*role, d.notes.get(k).map(String::as_str).unwrap_or("")) }))
        .collect();
    let termini: Vec<Value> = d.hof.termini.iter().enumerate().map(|(i, t)| json!({ "code": t.code, "name": t.texture_id, "all_exit": t.all_exit, "strings": t.strings, "special": d.is_special(i), "marked": d.specials.contains(&t.code) })).collect();
    let stops: Vec<Value> = d.hof.bus_stops.iter().map(|b| json!({ "ident": b.ident, "strings": b.strings })).collect();
    let trips: Vec<Value> = d.hof.info_trips.iter().enumerate().map(|(k, t)| json!({ "code": t.code, "line": t.line, "name": t.name, "route": t.route, "stops": d.hof.info_busstop_lists.get(k).cloned().unwrap_or_default() })).collect();
    let blocks: Vec<Value> = o
        .blocks
        .iter()
        .map(|(map, body)| {
            let h = owndepot::block_entries(d, body);
            json!({ "map": map, "d": h.termini.len(), "s": h.bus_stops.len(), "routes": h.info_trips.iter().map(|t| format!("{} ({})", t.code, t.line)).collect::<Vec<_>>() })
        })
        .collect();
    let issues: Vec<Value> = d.issues(&maps_hof, o.coding).iter().map(|i| json!({ "text": i.text, "args": i.args, "serious": i.serious, "at": place_json(i.at) })).collect();
    json!({
        "key": o.key, "file": format!("{}.hof", o.key), "name": d.hof.name, "map": d.map, "dirty": o.dirty,
        "termini": termini, "stops": stops, "trips": trips, "service_trip": d.hof.service_trip,
        "notes": d.notes, "stop_notes": d.stop_notes, "global_strings": d.hof.global_strings,
        "string_count": d.hof.string_count_terminus, "labels": labels, "blocks": blocks, "issues": issues,
    })
}

/// A change of the file open (`depoteditor`'s): `op` and what it takes. Answers the view and
/// `sel`, the row to choose after it (a destination, stop or route made, moved or deleted).
fn edit(args: &Value) -> Answer {
    let mut s = state();
    let op = arg::<String>(args, "op")?;
    let i = arg::<Option<usize>>(args, "i")?.unwrap_or(0);
    let k = arg::<Option<usize>>(args, "k")?.unwrap_or(0);
    let text = arg::<Option<String>>(args, "value")?.unwrap_or_default();
    let flag = arg::<Option<bool>>(args, "on")?.unwrap_or(false);
    let Some(o) = s.open.as_mut() else { return Err("no depot file open".into()) };
    let d = &mut o.doc;
    let mut sel: Option<usize> = None;
    // (a destination's name as it is typed: a space at its end is before the next word - trimmed
    // when the file is saved)
    let mut typed: Option<(usize, String)> = None;
    match op.as_str() {
        "name" => d.hof.name = text,
        "map" => d.map = text,
        "t_add" => sel = Some(d.add_destination(&text, false, false)),
        "t_name" => {
            if let Some(t) = d.hof.termini.get_mut(i) {
                t.texture_id = text.clone();
                typed = Some((i, text));
            }
        }
        "t_code" => {
            if let (Some(t), Ok(c)) = (d.hof.termini.get_mut(i), text.trim().parse::<i32>()) {
                t.code = c;
            }
        }
        "t_all_exit" => {
            if let Some(t) = d.hof.termini.get_mut(i) {
                t.all_exit = flag;
            }
        }
        "t_string" => {
            if let Some(x) = d.hof.termini.get_mut(i).and_then(|t| t.strings.get_mut(k)) {
                *x = text;
            }
        }
        "t_special" => d.set_special(i, flag),
        "t_dup" => {
            if let Some(mut t) = d.hof.termini.get(i).cloned() {
                t.code = d.free_code(t.code + 1);
                d.hof.termini.insert(i + 1, t);
                sel = Some(i + 1);
            }
        }
        "t_del" => {
            if i < d.hof.termini.len() {
                let code = d.hof.termini.remove(i).code;
                d.specials.retain(|c| *c != code);
                sel = Some(i.saturating_sub(1));
            }
        }
        "s_add" => sel = Some(d.add_stop(&text)),
        "s_ident" => {
            if let Some(b) = d.hof.bus_stops.get_mut(i) {
                b.ident = text;
            }
        }
        "s_string" => {
            if let Some(x) = d.hof.bus_stops.get_mut(i).and_then(|b| b.strings.get_mut(k)) {
                *x = text;
            }
        }
        "s_del" => {
            if i < d.hof.bus_stops.len() {
                d.hof.bus_stops.remove(i);
                sel = Some(i.saturating_sub(1));
            }
        }
        "r_add" => {
            let code = d.free_route(0);
            let dest = d.hof.termini.iter().find(|t| !t.all_exit).map(|t| t.code.to_string()).unwrap_or_default();
            d.hof.info_trips.push(omsi_vehicle::hof::InfoTrip { code: code.to_string(), name: text.to_uppercase(), route: dest, line: (code / 100).to_string(), extra: Vec::new() });
            d.tidy();
            sel = Some(d.hof.info_trips.len() - 1);
        }
        "r_code" | "r_line" | "r_name" | "r_route" => {
            if let Some(t) = d.hof.info_trips.get_mut(i) {
                match op.as_str() {
                    "r_code" => t.code = text,
                    "r_line" => t.line = text,
                    "r_name" => t.name = text,
                    _ => t.route = text,
                }
            }
        }
        "r_stop_del" => {
            if let Some(l) = d.hof.info_busstop_lists.get_mut(i).filter(|l| k < l.len()) {
                l.remove(k);
            }
        }
        "r_stop_move" => {
            let to = arg::<i64>(args, "to")?;
            if let Some(l) = d.hof.info_busstop_lists.get_mut(i) {
                if k < l.len() && to >= 0 && (to as usize) < l.len() {
                    l.swap(k, to as usize);
                }
            }
        }
        "r_stop_add" => {
            d.tidy();
            if let Some(l) = d.hof.info_busstop_lists.get_mut(i) {
                l.push(text);
            }
        }
        "r_del" => {
            if i < d.hof.info_trips.len() {
                d.hof.info_trips.remove(i);
                if i < d.hof.info_busstop_lists.len() {
                    d.hof.info_busstop_lists.remove(i);
                }
                sel = Some(i.saturating_sub(1));
            }
        }
        "service" => d.hof.service_trip = text,
        "usual" => sel = Some(d.add_destination(&text, flag, true)),
        "note" => {
            if let Some(x) = d.notes.get_mut(k) {
                *x = text;
            }
        }
        "global" => {
            if let Some(x) = d.hof.global_strings.get_mut(k) {
                *x = text;
            }
        }
        "global_add" => d.hof.global_strings.push(String::new()),
        "count" => d.set_string_count(k),
        other => return Err(format!("unknown change {other}")),
    }
    // (a change of a destination: the file in step with its counts again, the name as typed)
    if op.starts_with("t_") || op == "usual" {
        d.tidy();
        if let Some((i, name)) = typed {
            if let Some(t) = d.hof.termini.get_mut(i) {
                t.texture_id = name;
            }
        }
    }
    o.dirty = true;
    let v = view(&s);
    Ok(json!({ "view": v, "sel": sel }))
}

/// Save the file open (tidied first): `{ view, name, copies }` - the copies beside buses in
/// the content folder written too.
fn save() -> Answer {
    let mut s = state();
    let Some(o) = s.open.as_mut() else { return Err("no depot file open".into()) };
    o.doc.tidy();
    let bytes = owndepot::save(&o.path, &o.doc, o.coding)?;
    o.dirty = false;
    o.stamp = written_at(&o.path);
    let file = o.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let n = core::content_dir().map(|c| owndepot::refresh(&c, &file, &bytes)).unwrap_or(0);
    omsi_cfg::content_changed();
    let name = o.doc.hof.name.clone();
    Ok(json!({ "view": view(&s), "name": name, "copies": n, "entries": entries() }))
}

/// The file open put aside (`owndepot::delete`): `{ name, entries }`.
fn delete() -> Answer {
    let mut s = state();
    let Some(o) = s.open.take() else { return Err("no depot file open".into()) };
    let name = o.doc.hof.name.clone();
    if let Err(e) = owndepot::delete(&o.path, core::content_dir().as_deref()) {
        s.open = Some(o);
        return Err(e);
    }
    omsi_cfg::content_changed();
    Ok(json!({ "name": name, "entries": entries() }))
}

/// What a new file may start from for the map (`hof`: its depot file) and the bus `bus`
/// (`depoteditor::sources`): `[{ text, args, source }]` - an interface text with its
/// `%{..}`s, and `{ kind: empty | map | file, path }`.
fn sources(args: &Value) -> Answer {
    let hof = arg::<Option<String>>(args, "hof")?.unwrap_or_default();
    let bus = arg::<Option<String>>(args, "bus")?.unwrap_or_default();
    let bus_name = arg::<Option<String>>(args, "bus_name")?.unwrap_or_default();
    let mut out = vec![json!({ "text": "Empty, laid out as OMSI's own", "args": {}, "source": { "kind": "empty" } })];
    if !hof.trim().is_empty() {
        out.push(json!({ "text": "A copy of the map's: %{hof}", "args": { "hof": hof.trim() }, "source": { "kind": "map" } }));
    }
    if !bus.trim().is_empty() {
        let path = omsi_cfg::resolve_path(&core::root_dir().unwrap_or_default(), &bus);
        if let Some(dir) = path.parent() {
            for f in omsi_vehicle::hof::depot_files(&omsi_vehicle::hof::depot_dir(dir)) {
                let file = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                out.push(json!({ "text": "A copy of %{bus}'s: %{file}", "args": { "bus": bus_name, "file": file }, "source": { "kind": "file", "path": f.to_string_lossy() } }));
            }
        }
    }
    for e in owndepot::list(&owndepot::dir()) {
        out.push(json!({ "text": "A copy of mine: %{name}", "args": { "name": e.name }, "source": { "kind": "file", "path": e.path.to_string_lossy() } }));
    }
    Ok(json!(out))
}

/// A new file (`depoteditor::make_file`): `name`, for the map `map` (its file) whose depot file
/// is `hof`, from `source` (`de_sources`'s) - made and opened. Answers `{ view, entries }`, or
/// the error (an interface text with `|` and its argument: "The map's depot file %{hof} was not
/// found on this computer|...").
fn make(args: &Value) -> Answer {
    let name = arg::<String>(args, "name")?;
    let map = arg::<String>(args, "map")?;
    let hof = arg::<Option<String>>(args, "hof")?.unwrap_or_default();
    let source: Value = arg(args, "source")?;
    let folder = core::lines::map_folder(&map);
    let kind = source.get("kind").and_then(Value::as_str).unwrap_or("empty");
    let from = source.get("path").and_then(Value::as_str).map(PathBuf::from);
    let bytes = match kind {
        "map" => {
            let found = core::depot::find_sources(hof.trim(), &map);
            match core::depot::best(&found).and_then(|i| omsi_cfg::vfs::read(&found[i].path).ok()) {
                Some(b) => Some(b),
                None => return Err(format!("The map's depot file %{{hof}} was not found on this computer|{}", hof.trim())),
            }
        }
        "file" => {
            let p = from.clone().ok_or("no file")?;
            Some(omsi_cfg::vfs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?)
        }
        _ => None,
    };
    let (mut doc, coding) = match bytes {
        Some(b) => {
            let r = owndepot::read(&b);
            (r.doc, r.coding)
        }
        None => (Doc::blank(&name, &folder), Coding::Page(omsi_cfg::codepage::CodePage::Windows1252)),
    };
    // (a copy of another of the player's keeps its special trips; of anyone else's, they are
    // marked anew)
    if !(kind == "file" && from.as_ref().is_some_and(|p| p.starts_with(owndepot::dir()))) {
        doc.specials.clear();
    }
    doc.hof.name = name.clone();
    doc.map = folder;
    let path = owndepot::create(&owndepot::dir(), &doc, coding)?;
    let mut s = state();
    if let Some(o) = s.open.as_ref().filter(|o| o.dirty) {
        return Err(format!("{}|{}", "%{name} has changes that are not saved: save it first", o.doc.hof.name));
    }
    open_path(&mut s, &path)?;
    Ok(json!({ "view": view(&s), "entries": entries() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_depot_file_is_shown_and_changed_as_the_game_drawn_launcher_does() {
        let mut s = State { open: None, maps: vec![("Dorf".into(), "Dorf".into())] };
        assert_eq!(view(&s), Value::Null);
        let doc = Doc::blank("Mine", "Dorf");
        s.open = Some(Open { key: "oo_Mine".into(), path: PathBuf::from("oo_Mine.hof"), doc, coding: Coding::Page(omsi_cfg::codepage::CodePage::Windows1252), blocks: vec![], dirty: false, stamp: None });
        let o = s.open.as_mut().unwrap();
        let k = o.doc.add_destination("Bahnhof", false, false);
        o.doc.add_destination("Betriebsfahrt", true, true);
        let v = view(&s);
        assert_eq!(v["file"], json!("oo_Mine.hof"));
        assert_eq!(v["termini"][k]["name"], json!("Bahnhof"));
        assert_eq!(v["termini"][1]["special"], json!(true));
        assert_eq!(v["labels"][1]["role"], json!("front_top"));
        assert_eq!(v["labels"][0]["limit"], json!(16));
        // (OMSI's own layout: eight texts a destination)
        assert_eq!(v["string_count"], json!(8));
        assert!(v["issues"].as_array().unwrap().iter().all(|i| i["text"].is_string() && i["at"].is_array()));
    }
}

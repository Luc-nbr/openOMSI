//! The bus picker's photos for the desktop launcher (Trello "Tauri 6/6"): a bus photographed as
//! our game-drawn launcher photographs it (`busphoto::work`) - the studio showroom with the bus
//! always seen from the same front corner in the same noon light, read back at `PHOTO_W` x
//! `PHOTO_H` - and kept under the same name in `~/.openomsi/cache/busphotos/`, which both
//! launchers read. One bus at a time, on a thread of its own with a renderer of its own, so that
//! the bus preview's pictures go on meanwhile.

use super::Reply;
use crate::busphoto::{cache_dir, photo_name, PHOTO_H, PHOTO_W};
use crate::host::host;
use crate::showroom::Look;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The light of every photo (`busphoto::PHOTO_DATE`, `PHOTO_TIME`).
const PHOTO_DATE: &str = "2000-06-21";
const PHOTO_TIME: i32 = 12 * 60 + 30;
/// A bus that is not read and placed in this long is given up (`GIVE_UP_S`).
const GIVE_UP: Duration = Duration::from_secs(120);

struct Ask(Value, Reply);

static WORKER: OnceLock<Mutex<Option<Sender<Ask>>>> = OnceLock::new();

fn worker() -> std::sync::MutexGuard<'static, Option<Sender<Ask>>> {
    WORKER.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner())
}

/// Where the photo of `bus` in `paint` is kept (`busphoto::photo_file`).
fn file_of(root: &str, bus: &str, paint: &str) -> PathBuf {
    let path = omsi_cfg::resolve_path(Path::new(root), bus);
    let mtime = omsi_launcher_lib::index::mtime_ns(&path);
    cache_dir().join(photo_name(&path.to_string_lossy(), paint, mtime))
}

/// Photograph the bus `req` names (`bus`, `paint`, `map`): the answer once it is kept, with
/// where, or why there is none. The thread is started with the first, with `open`'s renderer.
pub(super) fn ask(req: &Value, reply: Reply, open: &dyn Fn() -> anyhow::Result<omsi_render::Renderer>) {
    let mut w = worker();
    if w.is_none() {
        let renderer = match open() {
            Ok(r) => r,
            Err(e) => return reply(json!({ "ok": false, "error": format!("{e:#}") }), Vec::new()),
        };
        let (tx, rx) = channel::<Ask>();
        let started = std::thread::Builder::new().name("bus photos".into()).spawn(move || {
            let mut renderer = renderer;
            let mut studio = host().new_studio();
            while let Ok(Ask(req, reply)) = rx.recv() {
                let (head, body) = take(&mut *studio, &mut renderer, &req);
                reply(head, body);
            }
        });
        if let Err(e) = started {
            return reply(json!({ "ok": false, "error": e.to_string() }), Vec::new());
        }
        *w = Some(tx);
    }
    if let Some(tx) = w.as_ref() {
        let _ = tx.send(Ask(req.clone(), reply));
    }
}

/// One bus photographed and kept (`busphoto::work`'s studio part).
fn take(studio: &mut dyn crate::host::Showroom, renderer: &mut omsi_render::Renderer, req: &Value) -> (Value, Vec<u8>) {
    let s = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let root = omsi_launcher_lib::load_config().root;
    let (bus, paint, map) = (s("bus"), s("paint"), s("map"));
    if bus.trim().is_empty() || map.trim().is_empty() {
        return (json!({ "ok": false, "error": "no bus or map" }), Vec::new());
    }
    let file = file_of(&root, &bus, &paint);
    // (taken meanwhile: by the game-drawn launcher, or an earlier request for it)
    if file.is_file() {
        return (json!({ "ok": true, "file": file.to_string_lossy() }), Vec::new());
    }
    let look = Look { root: PathBuf::from(&root), map, bus: bus.clone(), paint, weather: String::new(), time: PHOTO_TIME, date: PHOTO_DATE.to_string() };
    studio.want(look.clone());
    let t0 = Instant::now();
    let mut last = Instant::now();
    loop {
        let dt = last.elapsed().as_secs_f32().min(0.1);
        last = Instant::now();
        studio.update(renderer, dt);
        if studio.shows(&look) {
            break;
        }
        if studio.gave_up(&look) || t0.elapsed() > GIVE_UP {
            let why = studio.error().unwrap_or_else(|| "it took too long".into());
            log::warn!("bus photos: no photo of {bus}: {why}");
            studio.forget();
            return (json!({ "ok": false, "error": why }), Vec::new());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let picture = studio.photograph(renderer, PHOTO_W, PHOTO_H);
    studio.forget();
    let Some(img) = picture else { return (json!({ "ok": false, "error": "the bus could not be drawn" }), Vec::new()) };
    let _ = std::fs::create_dir_all(cache_dir());
    if let Err(e) = img.save_with_format(&file, image::ImageFormat::Png) {
        return (json!({ "ok": false, "error": format!("not saved to {}: {e}", file.display()) }), Vec::new());
    }
    log::info!("bus photos: photographed {bus} in {:.1} s", t0.elapsed().as_secs_f32());
    (json!({ "ok": true, "file": file.to_string_lossy() }), Vec::new())
}

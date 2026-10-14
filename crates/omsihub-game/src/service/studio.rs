//! The livery studio for the desktop launcher's page (Trello "Tauri 5/6"): the studio of
//! Omsi-Hub's game-drawn launcher (`crate::livery`), run here by the game program without a
//! window (`EditorHost`). The page shows its pictures over the whole window and sends back what
//! the mouse and the keys do; the studio draws the bus with the game's renderer, paints its
//! textures, reads the fonts and pictures, and saves the livery into the game, so the page has
//! its texts, its layout, its look in either light and its behaviour without a second copy of
//! them.
//!
//! Why not panels of the page's own around a picture of the bus: the studio is worked on the
//! bus itself - its layers are picked, dragged, turned and sized on it, its brush and pen paint
//! on it, its guide lines lie over it - and each of those needs the bus's geometry and its
//! camera, which are here. The page would have been a second studio to keep the same as the
//! first.
//!
//! The studio has a thread of its own (with its own renderer): its pictures are drawn there,
//! beside the bus preview's.

use super::Reply;
use crate::editor_host::{EditorHost, Page};
use crate::host::host;
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_ui::Draw;
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use winit::keyboard::{KeyCode, ModifiersState};

/// A request of the page, with where its answer goes.
pub(super) struct Ask(pub Value, pub Reply);

static STUDIO: OnceLock<Mutex<Option<Sender<Ask>>>> = OnceLock::new();

fn worker() -> std::sync::MutexGuard<'static, Option<Sender<Ask>>> {
    STUDIO.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether the studio's thread runs (it is started with a renderer of its own, `start`).
pub(super) fn running() -> bool {
    worker().is_some()
}

/// The request to the studio's thread; false when it does not run.
pub(super) fn ask(req: &Value, reply: Reply) -> bool {
    match worker().as_ref() {
        Some(tx) => tx.send(Ask(req.clone(), reply)).is_ok(),
        None => false,
    }
}

/// Start the studio's thread with `renderer`, and give it the request.
pub(super) fn start(renderer: omsi_render::Renderer, req: &Value, reply: Reply) {
    let (tx, rx) = channel::<Ask>();
    let started = std::thread::Builder::new().name("livery studio".into()).spawn(move || {
        let mut studio = Studio::new(renderer);
        while let Ok(Ask(req, reply)) = rx.recv() {
            let (head, body) = studio.answer(&req);
            reply(head, body);
        }
    });
    match started {
        Ok(_) => {
            let _ = tx.send(Ask(req.clone(), reply));
            *worker() = Some(tx);
        }
        Err(e) => reply(json!({ "ok": false, "error": e.to_string() }), Vec::new()),
    }
}

/// The studio: its host without a window, on the livery page.
struct Studio {
    l: EditorHost,
    /// What the last picture showed (its vertices, layers and the bus's pictures' generations):
    /// the same again is not drawn and sent again.
    shown: Option<(Vec<omsi_ui::Vertex>, String)>,
    /// The size the page asked for last, in pixels, and its interface scale.
    size: (u32, u32),
    scale: f32,
    /// The bus company's file as it was when the studio opened (its livery saved: changed).
    company: Option<String>,
    /// The bus and livery chosen, as the page was last told.
    choice: (String, String),
}

impl Studio {
    fn new(renderer: omsi_render::Renderer) -> Studio {
        let l = EditorHost::new(renderer);
        let choice = (l.state.choice.bus.clone(), l.state.choice.paint.clone());
        Studio { l, shown: None, size: (1280, 800), scale: 1.0, company: None, choice }
    }

    fn answer(&mut self, req: &Value) -> (Value, Vec<u8>) {
        match req.get("op").and_then(Value::as_str).unwrap_or("") {
            "open" => self.open(req),
            "frame" => self.frame(req),
            op => (json!({ "ok": false, "error": format!("unknown studio request {op}") }), Vec::new()),
        }
    }

    /// The studio opened (`livery::open`, `open_for_company`): on `bus` in `paint`, for the bus
    /// company `company` and its bus `vehicle`; the settings as the launcher has them now.
    fn open(&mut self, req: &Value) -> (Value, Vec<u8>) {
        let s = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let l = &mut self.l;
        // (the look, the accent, the language and the size the page has now)
        if let Ok(settings) = core::get_settings() {
            l.state.settings = settings;
        }
        host().ui_language(l.state.settings.get("language").and_then(|x| x.as_str()).unwrap_or("ENG"));
        let (bus, paint, company) = (s("bus"), s("paint"), s("company"));
        // (the bus step's choice as the page has it: the bus opened and its livery - a livery
        // saved for it becomes the bus step's - and the map, whose light the bus stands in)
        let duty: Value = std::fs::read_to_string(core::data_dir().join("launcher-duty.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        l.state.choice = crate::editor_host::Choice { bus: bus.clone(), paint: paint.clone(), map: duty.get("map").and_then(Value::as_str).unwrap_or("").to_string() };
        if company.is_empty() {
            crate::livery::open(l, Some(bus).filter(|b| !b.is_empty()), Some(paint));
            self.company = None;
        } else {
            // (the company's file read now: its cash and its buses as they are)
            match omsi_launcher_lib::company::store::load(&core::data_dir(), &company) {
                Ok(c) => {
                    self.company = serde_json::to_string(&c).ok();
                    l.company.company = Some(c);
                }
                Err(e) => return (json!({ "ok": false, "error": format!("{e:#}") }), Vec::new()),
            }
            let vehicle = req.get("vehicle").and_then(Value::as_u64).map(|v| v as u32);
            crate::livery::open_for_company(l, &bus, &paint, &company, vehicle);
        }
        self.choice = (l.state.choice.bus.clone(), l.state.choice.paint.clone());
        self.shown = None;
        l.last = Instant::now();
        (json!({ "ok": true }), Vec::new())
    }

    /// What the page did since its last picture (`events`), then the next picture when it is
    /// not the last one again: a PNG after the head, which says where the studio went when it
    /// was left, the mouse's pointer and what changed elsewhere.
    fn frame(&mut self, req: &Value) -> (Value, Vec<u8>) {
        let f = |k: &str, d: f64| req.get(k).and_then(Value::as_f64).unwrap_or(d);
        let (w, h) = ((f("w", 1280.0) as u32).clamp(64, 8192), (f("h", 800.0) as u32).clamp(64, 8192));
        let dpr = f("dpr", 1.0).clamp(0.5, 4.0) as f32;
        if (w, h) != self.size {
            self.shown = None;
        }
        self.size = (w, h);
        self.scale = ui_scale(&self.l, w, h, dpr);
        if let Some(events) = req.get("events").and_then(Value::as_array) {
            for e in events {
                self.event(e);
            }
        }
        let force = req.get("force").and_then(Value::as_bool).unwrap_or(false);
        let picture = self.draw(force);
        let l = &mut self.l;
        // where the studio went, left (`livery::leave`)
        let left = (l.page != Page::Livery).then(|| match l.page {
            Page::Company => "company",
            _ => "drive",
        });
        // the bus company's file written (its livery booked, its bus sent to be painted)
        let company = l.company.company.as_ref().and_then(|c| serde_json::to_string(c).ok());
        let company_changed = match (&self.company, company) {
            (Some(was), Some(now)) if *was != now => {
                self.company = Some(now);
                l.company.company.as_ref().map(|c| c.id.clone())
            }
            _ => None,
        };
        // the bus step's livery chosen (`livery::saved`)
        let choice = (l.state.choice.bus.clone(), l.state.choice.paint.clone());
        let chose = (choice != self.choice).then(|| json!({ "bus": choice.0, "paint": choice.1 }));
        self.choice = choice;
        let png = picture.map(png_of).unwrap_or_default();
        (json!({ "ok": true, "changed": !png.is_empty(), "left": left, "cursor": self.l.ui.cursor.name(), "company_changed": company_changed, "choice": chose }), png)
    }

    /// One thing the mouse or the keyboard did, as the window tells it (`window_event`):
    /// `{ t: move|down|up|wheel|key|leave|drop, ... }`, the modifiers held with it.
    fn event(&mut self, e: &Value) {
        let l = &mut self.l;
        let scale = self.scale;
        let f = |k: &str| e.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let flag = |k: &str| e.get("m").and_then(|m| m.get(k)).and_then(Value::as_bool).unwrap_or(false);
        let mut m = ModifiersState::empty();
        for (k, bit) in [("s", ModifiersState::SHIFT), ("c", ModifiersState::CONTROL), ("a", ModifiersState::ALT), ("w", ModifiersState::SUPER)] {
            if flag(k) {
                m |= bit;
            }
        }
        l.modifiers.told(m);
        l.modifiers.apply(&mut l.ui.input);
        l.last_input = Instant::now();
        match e.get("t").and_then(Value::as_str).unwrap_or("") {
            "move" => l.ui.input.mouse = Vec2::new(f("x"), f("y")) / scale,
            "leave" => l.ui.input.mouse = Vec2::new(-1e4, -1e4),
            "down" | "up" => {
                let down = e.get("t").and_then(Value::as_str) == Some("down");
                match f("b") as u32 {
                    0 => {
                        if down {
                            l.ui.input.pressed = true;
                        } else {
                            l.ui.input.released = true;
                        }
                        l.ui.input.down = down;
                    }
                    1 => l.livery.middle = down,
                    2 => {
                        l.ui.input.right_down = down;
                        if down {
                            l.ui.input.right_pressed = true;
                        }
                    }
                    _ => {}
                }
            }
            "wheel" => l.ui.input.wheel += Vec2::new(f("x"), f("y")),
            "key" => {
                let down = e.get("down").and_then(Value::as_bool).unwrap_or(false);
                let Some(code) = e.get("code").and_then(Value::as_str).and_then(code_of) else { return };
                // (the studio holds O for its before and after)
                crate::livery::key(l, code, down);
                if !down {
                    return;
                }
                let cmd = l.modifiers.command();
                l.raw_key = Some(code);
                use crate::ui::Key;
                let k = match code {
                    KeyCode::ArrowLeft => Some(Key::Left),
                    KeyCode::ArrowRight => Some(Key::Right),
                    KeyCode::ArrowUp => Some(Key::Up),
                    KeyCode::ArrowDown => Some(Key::Down),
                    KeyCode::Home => Some(Key::Home),
                    KeyCode::End => Some(Key::End),
                    KeyCode::Backspace => Some(Key::Backspace),
                    KeyCode::Delete => Some(Key::Delete),
                    KeyCode::Enter | KeyCode::NumpadEnter => Some(Key::Enter),
                    KeyCode::Escape => Some(Key::Escape),
                    KeyCode::Tab => Some(Key::Tab),
                    KeyCode::KeyA if cmd => Some(Key::SelectAll),
                    KeyCode::KeyC if cmd => Some(Key::Copy),
                    KeyCode::KeyV if cmd => Some(Key::Paste),
                    KeyCode::KeyX if cmd => Some(Key::Cut),
                    _ => None,
                };
                if let Some(k) = k {
                    if k == Key::Paste {
                        l.ui.clipboard_in = l.clipboard.as_mut().and_then(|c| c.get_text().ok());
                    }
                    l.ui.input.keys.push(k);
                }
                if !cmd {
                    if let Some(t) = e.get("text").and_then(Value::as_str) {
                        l.ui.input.text.push_str(t);
                    }
                }
            }
            // a picture dropped on the studio is the studio's
            "drop" => {
                if let Some(p) = e.get("path").and_then(Value::as_str) {
                    crate::livery::dropped(l, std::path::PathBuf::from(p));
                }
            }
            _ => {}
        }
    }

    /// The studio's frame (`draw_frame`, the livery page alone): its picture, unless it is the
    /// one sent last and `force` is not set.
    fn draw(&mut self, force: bool) -> Option<image::RgbaImage> {
        let (pw, ph) = self.size;
        let scale = self.scale;
        let l = &mut self.l;
        let now = Instant::now();
        let dt = now.duration_since(l.last).as_secs_f32().min(0.1);
        l.last = now;
        l.state.update(dt);
        crate::livery::update(l, dt);
        l.ui.begin(Vec2::new(pw as f32, ph as f32) / scale, scale, dt);
        l.ui.motion = l.state.settings.get("animations").and_then(|v| v.as_bool()).unwrap_or(true);
        crate::accent::set_from_setting(l.state.settings.get("accent").and_then(|v| v.as_str()));
        l.look();
        if l.page == Page::Livery {
            crate::livery::draw(l);
        }
        l.status_bar();
        if let Some(t) = l.ui.clipboard_out.take() {
            if let Some(c) = l.clipboard.as_mut() {
                let _ = c.set_text(t);
            }
        }
        let (layers, verts, ranges) = l.ui.finish();
        l.raw_key = None;
        // the bus's pictures and the shapes' onto the interface's GPU, then the interface
        let (device, queue, _) = l.dev.clone()?;
        if let (Some(renderer), Some(gpu)) = (l.renderer.as_mut(), l.gpu.as_mut()) {
            crate::livery::gpu(&mut l.livery, renderer, gpu, scale);
        }
        let gpu = l.gpu.as_mut()?;
        gpu.upload(&device, &queue, 0, &verts);
        gpu.upload_atlas(&queue, &mut l.ui.atlas);
        let key = format!("{pw}x{ph} {} {} {} {layers:?} {ranges:?}", l.livery.showroom.generation(), l.livery.showroom.second_generation(), l.ui.atlas.generation);
        if !force && self.shown.as_ref().is_some_and(|(v, k)| *v == verts && *k == key) {
            return None;
        }
        let draws: Vec<Draw> = ranges.iter().enumerate().map(|(k, (r, tex))| Draw { buffer: 0, range: r.clone(), layer: k, texture: *tex }).collect();
        let picture = l.shot_image(pw, ph, &layers, &draws, host().window_background());
        self.shown = Some((verts, key));
        picture
    }
}

/// The interface's scale for a window `w` x `h` pixels at `dpr` (as the launcher had it): the
/// system's size, smaller only under 1440 x 820, times the player's own size.
fn ui_scale(l: &EditorHost, w: u32, h: u32, dpr: f32) -> f32 {
    let (lw, lh) = (w as f32 / dpr, h as f32 / dpr);
    dpr * (lw / 1440.0).min(lh / 820.0).clamp(0.8, 1.0) * l.own_scale() as f32
}

/// The picture as a PNG, quickly (the interface's flat colours pack well).
fn png_of(img: image::RgbaImage) -> Vec<u8> {
    use image::ImageEncoder;
    let (w, h) = img.dimensions();
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut out = Vec::with_capacity((w * h) as usize / 2);
    let enc = image::codecs::png::PngEncoder::new_with_quality(&mut out, image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::Sub);
    if enc.write_image(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8).is_err() {
        return Vec::new();
    }
    out
}

/// A key by its name on the page (`KeyboardEvent.code`, the names winit's `KeyCode` has).
fn code_of(name: &str) -> Option<KeyCode> {
    use KeyCode as K;
    let letters = [K::KeyA, K::KeyB, K::KeyC, K::KeyD, K::KeyE, K::KeyF, K::KeyG, K::KeyH, K::KeyI, K::KeyJ, K::KeyK, K::KeyL, K::KeyM, K::KeyN, K::KeyO, K::KeyP, K::KeyQ, K::KeyR, K::KeyS, K::KeyT, K::KeyU, K::KeyV, K::KeyW, K::KeyX, K::KeyY, K::KeyZ];
    let digits = [K::Digit0, K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9];
    let pad = [K::Numpad0, K::Numpad1, K::Numpad2, K::Numpad3, K::Numpad4, K::Numpad5, K::Numpad6, K::Numpad7, K::Numpad8, K::Numpad9];
    if let Some(c) = name.strip_prefix("Key").filter(|c| c.len() == 1).and_then(|c| c.chars().next()).filter(char::is_ascii_uppercase) {
        return Some(letters[(c as u8 - b'A') as usize]);
    }
    if let Some(d) = name.strip_prefix("Digit").and_then(|d| d.parse::<usize>().ok()).filter(|d| *d < 10) {
        return Some(digits[d]);
    }
    if let Some(d) = name.strip_prefix("Numpad").and_then(|d| d.parse::<usize>().ok()).filter(|d| *d < 10) {
        return Some(pad[d]);
    }
    Some(match name {
        "ArrowLeft" => K::ArrowLeft,
        "ArrowRight" => K::ArrowRight,
        "ArrowUp" => K::ArrowUp,
        "ArrowDown" => K::ArrowDown,
        "Home" => K::Home,
        "End" => K::End,
        "PageUp" => K::PageUp,
        "PageDown" => K::PageDown,
        "Insert" => K::Insert,
        "Backspace" => K::Backspace,
        "Delete" => K::Delete,
        "Enter" => K::Enter,
        "NumpadEnter" => K::NumpadEnter,
        "Escape" => K::Escape,
        "Tab" => K::Tab,
        "Space" => K::Space,
        "ShiftLeft" => K::ShiftLeft,
        "ShiftRight" => K::ShiftRight,
        "ControlLeft" => K::ControlLeft,
        "ControlRight" => K::ControlRight,
        "AltLeft" => K::AltLeft,
        "AltRight" => K::AltRight,
        "MetaLeft" => K::SuperLeft,
        "MetaRight" => K::SuperRight,
        "Minus" => K::Minus,
        "Equal" => K::Equal,
        "BracketLeft" => K::BracketLeft,
        "BracketRight" => K::BracketRight,
        "Backslash" => K::Backslash,
        "Semicolon" => K::Semicolon,
        "Quote" => K::Quote,
        "Backquote" => K::Backquote,
        "Comma" => K::Comma,
        "Period" => K::Period,
        "Slash" => K::Slash,
        "NumpadAdd" => K::NumpadAdd,
        "NumpadSubtract" => K::NumpadSubtract,
        "NumpadMultiply" => K::NumpadMultiply,
        "NumpadDivide" => K::NumpadDivide,
        "NumpadDecimal" => K::NumpadDecimal,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_keys_are_the_windows() {
        assert_eq!(code_of("KeyO"), Some(KeyCode::KeyO));
        assert_eq!(code_of("KeyZ"), Some(KeyCode::KeyZ));
        assert_eq!(code_of("Digit7"), Some(KeyCode::Digit7));
        assert_eq!(code_of("Numpad0"), Some(KeyCode::Numpad0));
        assert_eq!(code_of("ArrowLeft"), Some(KeyCode::ArrowLeft));
        assert_eq!(code_of("MetaLeft"), Some(KeyCode::SuperLeft));
        assert_eq!(code_of("Keyz"), None);
        assert_eq!(code_of("F13"), None);
    }
}

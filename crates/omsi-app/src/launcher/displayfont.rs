//! The bus step's display font: the `.oft` font the chosen bus's destination displays are
//! drawn in - "As the bus" (its own matrix fonts), or one of the installed fonts (every
//! `Fonts` folder of the content roots: the OMSI installation's and openOMSI's content
//! folder's), each offered with the duty's destination written in it as a small LED sign the
//! way the game would draw it on this bus (`omsi_sim::texttex::apply_display_font`: fitted to
//! the height of the bus's own display font). "Add font…" copies a `.oft` and its bitmaps into
//! the content folder's `Fonts` (never the installation's). The choice is kept per bus file
//! (`omsi_launcher_lib::busfonts`) and goes to the game as `--display-font`.
//!
//! A bus whose destination displays are a script's pictures (the Lion's City's and the
//! O560's matrices draw their letters themselves) has no text texture a font draws: the row
//! says so instead of offering fonts that would change nothing.

use super::theme::*;
use super::ui::{picture_option, ButtonKind, Ui};
use glam::Vec2;
use omsi_content::font::{resample_into, Font, FontAtlas};
use omsi_launcher_lib::busfonts::{self, BusFonts};
use omsi_model::TextTexture;
use omsi_ui::paint::Align;
use omsi_ui::{tr, Color, Rect, Weight};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How many rows of pixels a preview sign has about (a pixel font is magnified by whole
/// times up to it, a taller one shrunk to it).
const PREVIEW_ROWS: u32 = 44;
/// The widest a preview is (its text cut off there).
const PREVIEW_MAX_W: u32 = 560;
/// The sign's panel, and an unlit dot on it.
const PANEL_RGB: [u8; 3] = [10, 9, 8];
const DOT_RGB: [u8; 3] = [26, 22, 16];
/// The colour of a sign whose display gives none (black text on black).
const AMBER: [f32; 3] = [255.0, 170.0, 0.0];

/// What the previews write when no duty gives a destination.
pub const SAMPLE: &str = "12 Hauptbahnhof";

// --- a bus's sign -----------------------------------------------------------------------------

/// A bus's destination display as the previews draw it: the widest of its destination
/// displays (the terminus, where a line number has its own beside it) and its own font.
#[derive(Clone)]
pub struct Sign {
    pub def: TextTexture,
    pub own: Option<Arc<FontAtlas>>,
    /// A line of the display's own font: what a chosen font is fitted to.
    pub line_h: i32,
    /// How many destination displays the bus has (the main model's).
    pub displays: usize,
}

/// Read `bus`'s destination displays (None: it has none a font draws).
pub fn read_sign(root: &Path, bus: &str) -> anyhow::Result<Option<Sign>> {
    use anyhow::Context;
    let path = crate::spawn::player_bus_path(root, bus)?;
    let def = omsi_vehicle::Vehicle::load(&path).with_context(|| format!("loading {}", path.display()))?;
    let model_rel = def.model.clone().context("the bus has no [model]")?;
    let model_path = omsi_cfg::resolve_path(def.dir(), &model_rel);
    let model = omsi_model::Model::load(&model_path).with_context(|| format!("loading {}", model_path.display()))?;
    let displays = omsi_sim::texttex::destination_displays(&model);
    let Some(&main) = displays.iter().max_by_key(|&&i| (model.text_textures[i].width, std::cmp::Reverse(i))) else { return Ok(None) };
    let t = model.text_textures[main].clone();
    let own = omsi_sim::texttex::FontLibrary::new(root).load(&t.font);
    let line_h = own.as_ref().map(|a| a.font.height).filter(|h| *h > 0).unwrap_or(t.height).max(1);
    Ok(Some(Sign { def: t, own, line_h, displays: displays.len() }))
}

/// A preview's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// `text` on a small LED sign: as `sign`'s display draws it in `atlas` - fitted to the line of
/// the display's own font when `fitted` (a font the player chose), as the bus has it else -
/// the line cut out of the texture, as wide as its letters, on a dark panel. A pixel font is
/// magnified by whole times to about `PREVIEW_ROWS` rows with the dots of a matrix sign
/// between its pixels; a taller font is shrunk to them.
pub fn sign_picture(sign: &Sign, atlas: Option<Arc<FontAtlas>>, fitted: bool, text: &str) -> Picture {
    let mut def = sign.def.clone();
    // (black letters say the colour comes from the mesh: amber, as most signs are)
    if def.color.iter().sum::<f32>() < 90.0 {
        def.color = AMBER;
    }
    def.full_color = false;
    let line_h = sign.line_h.max(1) as u32;
    // (the texture as wide as the text needs: a preview shows the font, not the clipping)
    if let Some(a) = atlas.as_ref() {
        let s = if fitted { omsi_content::font::fit_scale(line_h as f32, a.font.height.max(1) as f32) } else { 1.0 };
        def.width = def.width.max((a.text_width(text) as f32 * s) as i32 + 8);
    }
    let (w, h) = (def.width.max(1) as u32, def.height.max(1) as u32);
    let mut state = omsi_sim::texttex::TextTextureState::new(def.clone(), atlas);
    state.fit = fitted.then_some(line_h);
    let img = state.image(text);
    // the line's rows (where the display's own font has its line), the letters' columns
    let top = ((h as i32 - line_h as i32) / 2).max(0) as u32;
    let rows = line_h.min(h - top.min(h)).max(1);
    let lit = |x: u32, y: u32| img[((y * w + x) * 4 + 3) as usize] > 0;
    let cols: Vec<u32> = (0..w).filter(|&x| (top..top + rows).any(|y| lit(x, y))).collect();
    let (x0, x1) = match (cols.first(), cols.last()) {
        (Some(&a), Some(&b)) => (a.saturating_sub(2), (b + 3).min(w)),
        _ => (0, w.min(48)),
    };
    let cw = (x1 - x0).max(1);
    let mut cut = vec![0u8; (cw * rows * 4) as usize];
    for y in 0..rows {
        let from = (((top + y) * w + x0) * 4) as usize;
        cut[(y * cw * 4) as usize..((y + 1) * cw * 4) as usize].copy_from_slice(&img[from..from + (cw * 4) as usize]);
    }
    led(&cut, cw, rows)
}

/// The cut-out line `src` (`w` x `h`, its colours laid over nothing) on the sign's panel.
fn led(src: &[u8], w: u32, h: u32) -> Picture {
    let k = (PREVIEW_ROWS / h.max(1)).clamp(1, 6);
    if h > PREVIEW_ROWS {
        // a large font: shrunk to the preview's rows, no dots of its own
        let dh = PREVIEW_ROWS;
        let dw = ((w as f32 * dh as f32 / h as f32).round() as u32).clamp(1, PREVIEW_MAX_W);
        let mut over = vec![0u8; (dw * dh * 4) as usize];
        resample_into(src, w, h, &mut over, dw, dh, 0, 0, (w as f32 * dh as f32 / h as f32).round() as i32, dh as i32);
        return on_panel(&over, dw, dh, |_, _| PANEL_RGB);
    }
    let (pw, ph) = ((w * k).min(PREVIEW_MAX_W), h * k);
    let mut big = vec![0u8; (pw * ph * 4) as usize];
    for y in 0..ph {
        for x in 0..pw {
            // (the matrix's dark seam between the dots: the last row and column of each cell)
            if k >= 3 && (x % k == k - 1 || y % k == k - 1) {
                continue;
            }
            let si = (((y / k) * w + x / k) * 4) as usize;
            let di = ((y * pw + x) * 4) as usize;
            big[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    on_panel(&big, pw, ph, |x, y| if k >= 3 && x % k != k - 1 && y % k != k - 1 { DOT_RGB } else { PANEL_RGB })
}

/// `over` (its colours laid over nothing) over the panel, whose colour at a pixel `under`
/// gives: an opaque picture.
fn on_panel(over: &[u8], w: u32, h: u32, under: impl Fn(u32, u32) -> [u8; 3]) -> Picture {
    let mut rgba = vec![255u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = over[i + 3] as f32 / 255.0;
            let bg = under(x, y);
            for c in 0..3 {
                rgba[i + c] = (over[i + c] as f32 + bg[c] as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Picture { w, h, rgba }
}

/// The share of `text`'s letters (not its spaces) `font` has a glyph for.
pub fn coverage(font: &Font, text: &str) -> f32 {
    let letters: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if letters.is_empty() {
        return 1.0;
    }
    letters.iter().filter(|&&c| font.glyph(c).is_some()).count() as f32 / letters.len() as f32
}

/// The fonts a player is offered for `text`: those that can write most of it (no plate's or
/// clock's digits alone), each name once, by name; `keep` among them whatever it can write.
pub fn offered(fonts: &[Font], text: &str, keep: Option<&str>) -> Vec<Font> {
    let keep = keep.map(|k| k.trim().to_ascii_lowercase());
    let mut out: Vec<Font> = fonts.iter().filter(|f| coverage(f, text) >= 0.6 || keep.as_deref() == Some(f.name.trim().to_ascii_lowercase().as_str())).cloned().collect();
    out.sort_by(|a, b| super::buspick::name_cmp(a.name.trim(), b.name.trim()));
    out.dedup_by(|a, b| a.name.trim().eq_ignore_ascii_case(b.name.trim()));
    out
}

/// The size of `chosen`'s font file that the game draws on a line `line_h` high
/// (`FontLibrary::display_atlas`), among `fonts`.
pub fn size_for<'a>(fonts: &'a [Font], chosen: &str, line_h: i32) -> Option<&'a Font> {
    let first = fonts.iter().find(|f| f.name.trim().eq_ignore_ascii_case(chosen.trim()))?;
    let family: Vec<&Font> = fonts.iter().filter(|f| f.path == first.path).collect();
    let me = family.iter().position(|f| f.name.trim().eq_ignore_ascii_case(chosen.trim())).unwrap_or(0);
    let heights: Vec<i32> = family.iter().map(|f| f.height).collect();
    Some(family[omsi_sim::texttex::pick_size(&heights, me, line_h)])
}

// --- the launcher's side ----------------------------------------------------------------------

enum Slot<T> {
    Reading,
    Ready(T),
    Failed,
}

/// The previews being drawn for one bus and text: what a preview's key is made of.
fn preview_key(bus: &str, text: &str, font: &str) -> String {
    format!("{}|{text}|{}", omsi_launcher_lib::busoptions::bus_key(bus), font.trim().to_ascii_lowercase())
}

/// The choices (kept in their file), the installed fonts and each bus's sign (read on workers
/// the first time they are asked for), and the previews (drawn on a worker, sent to the GPU
/// in `upload`).
pub struct DisplayFonts {
    choices: BusFonts,
    /// Where the choices are kept (none: not kept, a test's).
    file: Option<PathBuf>,
    fonts: Arc<Mutex<HashMap<String, Slot<Arc<Vec<Font>>>>>>,
    signs: Arc<Mutex<HashMap<String, Slot<Option<Sign>>>>>,
    /// The fonts offered for a text (worked out again when the list or the text changes).
    offered: Option<(String, usize, Arc<Vec<Font>>)>,
    /// The previews drawn, waiting for the GPU; on it (texture, size) by their key; the bus
    /// and text they are drawn for, and the worker drawing them (its number: a newer one
    /// stops it).
    pending: Arc<Mutex<Vec<(String, Picture)>>>,
    textures: HashMap<String, (usize, u32, u32)>,
    drawing: Option<String>,
    worker: Arc<AtomicU64>,
    /// Textures of previews no longer shown, to free on the GPU (`freeing`: next frame).
    stale: Vec<usize>,
    freeing: Vec<usize>,
    /// What "Add font…" did, and whether it went wrong.
    pub note: Option<(String, bool)>,
}

/// What a click in the row asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// This font for the bus; None: as the bus.
    Pick(Option<String>),
    /// Add a font file to the content folder.
    Add,
}

impl DisplayFonts {
    /// The choices kept in `~/.openomsi`.
    pub fn load() -> DisplayFonts {
        let file = BusFonts::path();
        Self::with(BusFonts::read(&file), Some(file))
    }

    fn with(choices: BusFonts, file: Option<PathBuf>) -> DisplayFonts {
        DisplayFonts { choices, file, fonts: Default::default(), signs: Default::default(), offered: None, pending: Default::default(), textures: HashMap::new(), drawing: None, worker: Arc::new(AtomicU64::new(0)), stale: Vec::new(), freeing: Vec::new(), note: None }
    }

    /// Choices kept nowhere (tests).
    #[cfg(test)]
    pub fn in_memory() -> DisplayFonts {
        Self::with(BusFonts::default(), None)
    }

    /// The font `bus`'s destination displays are drawn in (None: as the bus) - what the game
    /// gets as `--display-font`.
    pub fn font_for(&self, bus: &str) -> Option<String> {
        self.choices.font_for(bus)
    }

    /// The installed fonts under `root` (None while they are read: asked for now, the first
    /// time).
    pub fn fonts(&self, root: &str) -> Option<Arc<Vec<Font>>> {
        let mut slots = self.fonts.lock().ok()?;
        match slots.get(root) {
            Some(Slot::Ready(f)) => return Some(f.clone()),
            Some(_) => return None,
            None => {}
        }
        slots.insert(root.to_string(), Slot::Reading);
        let (map, key) = (self.fonts.clone(), root.to_string());
        std::thread::spawn(move || {
            let list = std::panic::catch_unwind(|| omsi_sim::texttex::FontLibrary::new(Path::new(&key)).installed());
            if let Ok(mut m) = map.lock() {
                m.insert(key, list.map(|l| Slot::Ready(Arc::new(l))).unwrap_or(Slot::Failed));
            }
        });
        None
    }

    /// `bus`'s sign: None while it is read (or when it could not be), Some(None) for a bus
    /// without a destination display a font draws.
    pub fn sign(&self, root: &str, bus: &str) -> Option<Option<Sign>> {
        if bus.trim().is_empty() {
            return None;
        }
        let key = format!("{root}|{}", omsi_launcher_lib::busoptions::bus_key(bus));
        let mut slots = self.signs.lock().ok()?;
        match slots.get(&key) {
            Some(Slot::Ready(s)) => return Some(s.clone()),
            Some(Slot::Failed) => return Some(None),
            Some(Slot::Reading) => return None,
            None => {}
        }
        slots.insert(key.clone(), Slot::Reading);
        let (map, root, bus) = (self.signs.clone(), PathBuf::from(root), bus.to_string());
        std::thread::spawn(move || {
            let slot = match std::panic::catch_unwind(|| read_sign(&root, &bus)) {
                Ok(Ok(s)) => Slot::Ready(s),
                Ok(Err(e)) => {
                    log::info!("display font: the displays of {bus}: {e:#}");
                    Slot::Failed
                }
                Err(_) => Slot::Failed,
            };
            if let Ok(mut m) = map.lock() {
                m.insert(key, slot);
            }
        });
        None
    }

    /// The fonts offered for `text` among `fonts` (see `offered`), `keep` among them.
    fn offered_for(&mut self, fonts: &Arc<Vec<Font>>, text: &str, keep: Option<&str>) -> Arc<Vec<Font>> {
        let key = format!("{text}|{}", keep.unwrap_or_default().to_ascii_lowercase());
        let id = Arc::as_ptr(fonts) as usize;
        if let Some((k, i, list)) = self.offered.as_ref() {
            if *k == key && *i == id {
                return list.clone();
            }
        }
        let list = Arc::new(offered(fonts, text, keep));
        self.offered = Some((key, id, list.clone()));
        list
    }

    /// The preview of `font` ("" as the bus) on `bus`'s sign with `text`, once it is on the GPU.
    fn picture(&self, bus: &str, text: &str, font: &str) -> Option<(usize, u32, u32)> {
        self.textures.get(&preview_key(bus, text, font)).copied()
    }

    /// Draw the previews of `bus`'s sign with `text` - as the bus, `first`, then every font
    /// offered - on a worker, unless they are being drawn already. The previews of another bus
    /// or text are let go.
    fn draw(&mut self, bus: &str, text: &str, sign: &Sign, fonts: Arc<Vec<Font>>, offered: Arc<Vec<Font>>, first: Option<String>) {
        let what = preview_key(bus, text, "");
        if self.drawing.as_deref() == Some(what.as_str()) {
            return;
        }
        self.drawing = Some(what.clone());
        let prefix = what.trim_end_matches('|').to_string();
        // (the other bus's pictures: off the GPU)
        let old: Vec<String> = self.textures.keys().filter(|k| !k.starts_with(&format!("{prefix}|"))).cloned().collect();
        for k in old {
            if let Some((t, _, _)) = self.textures.remove(&k) {
                self.stale.push(t);
            }
        }
        if let Ok(mut p) = self.pending.lock() {
            p.retain(|(k, _)| k.starts_with(&format!("{prefix}|")));
        }
        let me = self.worker.fetch_add(1, Ordering::SeqCst) + 1;
        let (worker, pending, sign, bus, text) = (self.worker.clone(), self.pending.clone(), sign.clone(), bus.to_string(), text.to_string());
        let done: Vec<String> = self.textures.keys().cloned().collect();
        std::thread::spawn(move || {
            let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
            let put = |name: &str, pic: Picture| {
                if let Ok(mut p) = pending.lock() {
                    p.push((preview_key(&bus, &text, name), pic));
                }
            };
            if !done.contains(&preview_key(&bus, &text, "")) {
                put("", sign_picture(&sign, sign.own.clone(), false, &text));
            }
            let order = first.iter().cloned().chain(offered.iter().map(|f| f.name.trim().to_string()).filter(|n| first.as_deref().is_none_or(|f| !f.eq_ignore_ascii_case(n))));
            for name in order {
                if worker.load(Ordering::SeqCst) != me {
                    return;
                }
                if done.contains(&preview_key(&bus, &text, &name)) {
                    continue;
                }
                let Some(font) = size_for(&fonts, &name, sign.line_h) else { continue };
                let atlas = std::panic::catch_unwind(|| omsi_sim::texttex::atlas_of(font.clone(), Path::new(""), &decode)).ok().flatten();
                if let Some(a) = atlas {
                    put(&name, sign_picture(&sign, Some(Arc::new(a)), true, &text));
                }
            }
        });
    }

    /// The previews drawn since the last frame go to the GPU, those let go off it (`mod.rs`,
    /// where the device is).
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, gpu: &mut omsi_ui::Gpu) {
        // (a frame after they were let go: this frame's drawing may still name them)
        for t in std::mem::take(&mut self.freeing) {
            gpu.free(t);
        }
        self.freeing = std::mem::take(&mut self.stale);
        let new: Vec<(String, Picture)> = self.pending.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
        for (key, pic) in new {
            if self.textures.contains_key(&key) {
                continue;
            }
            let id = gpu.add_image(device, queue, pic.w, pic.h, &pic.rgba);
            self.textures.insert(key, (id, pic.w, pic.h));
        }
    }

    /// Everything on the GPU went with it: the previews are drawn again when next shown.
    pub fn drop_gpu(&mut self) {
        self.textures.clear();
        self.stale.clear();
        self.freeing.clear();
        self.drawing = None;
    }

    /// Do what the row asked for `bus` (under the OMSI folder `root`).
    pub fn edit(&mut self, root: &str, bus: &str, e: Edit) {
        match e {
            Edit::Pick(font) => {
                self.choices.set(bus, font.as_deref());
                self.note = None;
                self.save();
            }
            Edit::Add => self.add(root, bus),
        }
    }

    fn save(&self) {
        if let Some(f) = &self.file {
            if let Err(err) = self.choices.write(f) {
                log::warn!("display fonts: {} not written: {err}", f.display());
            }
        }
    }

    /// "Add font…": a `.oft` chosen in a file dialog into the content folder's `Fonts`, and
    /// its first font chosen for `bus`.
    fn add(&mut self, root: &str, bus: &str) {
        let Some(file) = omsi_launcher_lib::pick_file_of(&tr("Add a display font"), &tr("OMSI font"), &["oft"]) else { return };
        let Some(dir) = busfonts::content_fonts_dir() else {
            self.note = Some((tr("There is no content folder to add fonts to: set the game under Setup.").into_owned(), true));
            return;
        };
        match busfonts::add_font(&file, &dir) {
            Ok(added) => {
                let mut text = tr("Added: %{fonts}").replace("%{fonts}", &added.names.join(", "));
                if !added.missing.is_empty() {
                    text.push_str(&format!(" · {}", tr("missing beside it: %{files}").replace("%{files}", &added.missing.join(", "))));
                }
                crate::mt::protect([text.as_str()]);
                self.note = Some((text, !added.missing.is_empty()));
                // (the fonts read again, with the new one; it is chosen for the bus)
                if let Ok(mut f) = self.fonts.lock() {
                    f.remove(root);
                }
                self.offered = None;
                self.drawing = None;
                if let Some(first) = added.names.first() {
                    self.choices.set(bus, Some(first));
                    self.save();
                }
            }
            Err(e) => self.note = Some((format!("{e:#}"), true)),
        }
    }
}

// --- the row in the bus sheet -----------------------------------------------------------------

/// The display font row at (`x`, `y`), `w` wide, for `bus` with the destination `text`: its
/// heading with "Add font…", the choice (each font with its sign), and the chosen one's sign
/// large under it. Returns the height it takes and what was clicked.
#[allow(clippy::too_many_arguments)]
pub(super) fn section(ui: &mut Ui, x: f32, y: f32, w: f32, df: &mut DisplayFonts, root: &str, bus: &str, text: &str) -> (f32, Option<Edit>) {
    let touch = ui.input.touch || super::mobile::mobile();
    let top = y;
    let mut edit = None;
    ui.heading(Rect::new(x, y, w, 26.0), "Display font", None);
    // (a phone has no file dialog to add one with)
    if !cfg!(target_os = "android") && !super::mobile::mobile() {
        let label = tr("Add font…");
        let bw = ui.width(&label, 13.0, Weight::Medium) + 44.0;
        let r = Rect::new(x + w - bw, y - 2.0, bw, 26.0);
        if ui.button("display-font-add", r, "Add font…", Some("add"), ButtonKind::Ghost) {
            edit = Some(Edit::Add);
        }
        ui.tooltip(r, "Copy a .oft font (and its bitmaps) into openOMSI's content folder");
    }
    let mut y = y + 32.0;
    let note = |ui: &mut Ui, y: &mut f32, words: &str, c: Color| {
        let h = ui.paragraph_height(words, w - 10.0, 12.0, Weight::Regular);
        if ui.rect_visible(Rect::new(x, *y, w, h)) {
            ui.paragraph(words, Vec2::new(x + 2.0, *y), w - 10.0, 12.0, Weight::Regular, c);
        }
        *y += h + 6.0;
    };
    let (sign, fonts) = (df.sign(root, bus), df.fonts(root));
    match (sign, fonts) {
        (Some(None), _) => note(ui, &mut y, "This bus draws its destination displays as pictures of its own: a display font does not change them.", TEXT_DIM),
        (Some(Some(sign)), Some(fonts)) => {
            let chosen = df.font_for(bus);
            let offered = df.offered_for(&fonts, text, chosen.as_deref());
            df.draw(bus, text, &sign, fonts.clone(), offered.clone(), chosen.clone());
            let mut options: Vec<String> = Vec::with_capacity(offered.len() + 2);
            options.push(picture_option(df.picture(bus, text, ""), &tr("As the bus")));
            for f in offered.iter() {
                options.push(picture_option(df.picture(bus, text, f.name.trim()), f.name.trim()));
            }
            // (a font chosen that is no longer installed: still there, so that it can be seen)
            let mut sel = match chosen.as_deref() {
                None => 0,
                Some(c) => match offered.iter().position(|f| f.name.trim().eq_ignore_ascii_case(c)) {
                    Some(i) => i + 1,
                    None => {
                        options.push(picture_option(None, &tr("%{font} (not installed)").replace("%{font}", c)));
                        options.len() - 1
                    }
                },
            };
            crate::mt::protect(options.iter().map(String::as_str));
            let field = Rect::new(x, y, w, if touch { 44.0 } else { ROW + 4.0 });
            if ui.rect_visible(field) && ui.select("display-font", field, &mut sel, &options) {
                let pick = sel.checked_sub(1).and_then(|i| offered.get(i)).map(|f| f.name.trim().to_string());
                // (the missing one picked again: kept)
                if sel == 0 || pick.is_some() {
                    edit = Some(Edit::Pick(pick));
                }
            }
            y += field.h + 8.0;
            // the chosen one large: the sign as the bus will show it
            let big = Rect::new(x, y, w, 58.0);
            if ui.rect_visible(big) {
                ui.p().rounded(big, 8.0, Color::rgba(PANEL_RGB[0], PANEL_RGB[1], PANEL_RGB[2], 1.0));
                ui.p().rounded_border(big, 8.0, 1.0, EDGE);
                let shown = df.picture(bus, text, chosen.as_deref().unwrap_or(""));
                match shown {
                    Some((tex, pw, ph)) => {
                        let room = big.inset(7.0);
                        let k = (room.h / ph as f32).min(room.w / pw as f32).min(1.0);
                        let (iw, ih) = (pw as f32 * k, ph as f32 * k);
                        ui.image(Rect::new(room.center().x - iw * 0.5, room.center().y - ih * 0.5, iw, ih), tex, 0.0);
                    }
                    None => {
                        ui.text_in("Drawing the sign…", big, 12.0, Weight::Regular, TEXT_FAINT, Align::Center);
                    }
                }
            }
            y += big.h + 6.0;
            let says = match chosen.as_deref() {
                None => tr("As the bus: its own font (%{font}).").replace("%{font}", sign.def.font.trim()),
                Some(_) if sign.displays == 1 => tr("Drawn on its destination display, fitted to the height of the bus's own font.").into_owned(),
                Some(_) => tr("Drawn on its %{n} destination displays, each fitted to the height of the bus's own font.").replace("%{n}", &sign.displays.to_string()),
            };
            crate::mt::protect([says.as_str()]);
            note(ui, &mut y, &says, TEXT_FAINT);
        }
        _ => {
            if ui.rect_visible(Rect::new(x, y, w, 20.0)) {
                ui.text_in("Reading the bus's displays…", Rect::new(x + 2.0, y, w, 20.0), 12.0, Weight::Regular, TEXT_FAINT, Align::Left);
            }
            y += 26.0;
        }
    }
    if let Some((words, bad)) = df.note.clone() {
        note(ui, &mut y, &words, if bad { WARN } else { OK });
    }
    (y - top, edit)
}

/// The destination the previews write: the duty's first trip's line and terminus, else `SAMPLE`.
pub fn sample_of(state: &super::state::State) -> String {
    let trip = state.first_trip().and_then(|k| state.tour()?.trips.get(k));
    match trip {
        Some(t) if !t.terminus.trim().is_empty() => format!("{} {}", t.line.trim(), t.terminus.trim()).trim().to_string(),
        _ => SAMPLE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_content::font::FontChar;

    fn pixel_font(name: &str, h: i32, chars: &str) -> (Font, FontAtlas) {
        let mut x = 0;
        let glyphs: Vec<FontChar> = chars
            .chars()
            .map(|ch| {
                let g = FontChar { ch, x0: x, x1: x + 4, y: 0 };
                x += 4;
                g
            })
            .collect();
        let font = Font { name: name.into(), path: PathBuf::from(format!("{name}.oft")), height: h, gap: 1, chars: glyphs, ..Default::default() };
        let (aw, ah) = (x.max(1) as u32, h as u32);
        let alpha: Vec<u8> = (0..aw * ah).flat_map(|i| if (i % aw) % 4 != 3 { [255u8; 4] } else { [0u8; 4] }).collect();
        (font.clone(), FontAtlas::new(font, aw, ah, alpha.clone(), alpha))
    }

    fn sign(line_h: i32) -> Sign {
        let def = TextTexture { variable: "Matrix_Terminus".into(), font: "Own".into(), width: 128, height: 32, full_color: false, color: [0.0, 0.0, 0.0], orientation: 0, grid: 1 };
        Sign { def, own: None, line_h, displays: 2 }
    }

    #[test]
    fn a_preview_is_the_line_as_the_bus_draws_it_on_a_dark_sign() {
        let (_, a) = pixel_font("Pix 7", 7, "HBF12 ");
        let p = sign_picture(&sign(14), Some(Arc::new(a)), true, "12 HBF");
        // fitted twice (14 rows), magnified three times more to the preview: dots with seams
        assert_eq!(p.h, 14 * 3);
        assert_eq!(p.rgba.len(), (p.w * p.h * 4) as usize);
        assert!(p.rgba.chunks(4).all(|c| c[3] == 255), "opaque");
        let amber = p.rgba.chunks(4).filter(|c| c[0] > 200 && c[1] > 120 && c[2] < 40).count();
        assert!(amber > 0, "black letters are written amber");
        // (a seam between two dots is dark)
        let seam = (2 * p.w * 4) as usize;
        assert!(p.rgba[seam..seam + (p.w * 4) as usize].chunks(4).all(|c| c[0] < 40));
        // a tall font: shrunk to the preview's rows
        let (_, tall) = pixel_font("Big 64", 64, "HBF12 ");
        let mut high = sign(64);
        high.def.height = 80;
        let p = sign_picture(&high, Some(Arc::new(tall)), true, "12 HBF");
        assert_eq!(p.h, PREVIEW_ROWS);
        assert!(p.w <= PREVIEW_MAX_W);
        // no font: an empty sign, not nothing
        let p = sign_picture(&sign(14), None, false, "12 HBF");
        assert!(p.w > 0 && p.h > 0);
    }

    #[test]
    fn the_fonts_offered_can_write_the_destination() {
        let (letters, _) = pixel_font("Letters", 7, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ");
        let (digits, _) = pixel_font("Digits", 7, "0123456789");
        let (also, _) = pixel_font("Also letters", 9, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
        let all = vec![letters.clone(), digits.clone(), also.clone(), letters.clone()];
        let names = |v: Vec<Font>| v.into_iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(offered(&all, "12 Hauptbahnhof", None)), ["Also letters", "Letters"], "small letters as their capitals; each once");
        assert_eq!(names(offered(&all, "12 Hauptbahnhof", Some("digits"))), ["Also letters", "Digits", "Letters"], "the chosen one stays");
        assert!(coverage(&digits, "12 Hauptbahnhof") < 0.2);
        assert_eq!(coverage(&digits, "  "), 1.0);
    }

    #[test]
    fn the_size_the_game_takes_of_a_font_file() {
        let size = |name: &str, h: i32| Font { name: name.into(), path: PathBuf::from("Krueger.oft"), height: h, ..Default::default() };
        let other = Font { name: "Other".into(), path: PathBuf::from("Other.oft"), height: 5, ..Default::default() };
        let fonts = vec![size("Krueger 7x4", 7), size("Krueger 16x9", 16), other];
        assert_eq!(size_for(&fonts, "Krueger 16x9", 16).map(|f| f.name.as_str()), Some("Krueger 16x9"));
        assert_eq!(size_for(&fonts, "krueger 16x9", 8).map(|f| f.name.as_str()), Some("Krueger 7x4"), "too tall: the size of its file that fits");
        assert_eq!(size_for(&fonts, "Krueger 7x4", 32).map(|f| f.name.as_str()), Some("Krueger 7x4"));
        assert_eq!(size_for(&fonts, "Missing", 16), None);
    }

    /// Real buses and fonts of the installed OMSI 2 (`OMSI_ROOT`; skipped without it): each
    /// bus's destination display drawn as the game draws it - in its own font and in two
    /// others - and the launcher's preview of each, written as PNGs to `OMSI_FONT_SHOTS` when
    /// that is set. No line runs higher than the bus's own font's.
    #[test]
    fn real_buses_destinations_in_other_fonts() {
        let Some(root) = omsi_cfg::env::var_os("OMSI_ROOT").map(PathBuf::from) else {
            eprintln!("skipped: no OMSI_ROOT");
            return;
        };
        let shots = std::env::var_os("OMSI_FONT_SHOTS").map(PathBuf::from);
        if let Some(d) = shots.as_ref() {
            std::fs::create_dir_all(d).unwrap();
        }
        let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
        let mut lib = omsi_sim::texttex::FontLibrary::new(&root);
        let save = |name: &str, w: u32, h: u32, rgba: &[u8], k: u32| {
            let Some(d) = shots.as_ref() else { return };
            // (laid over black, each pixel k x k: as it is, larger)
            let img = image::RgbaImage::from_fn(w * k, h * k, |x, y| {
                let i = (((y / k) * w + x / k) * 4) as usize;
                image::Rgba([rgba[i], rgba[i + 1], rgba[i + 2], 255])
            });
            img.save(d.join(format!("{name}.png"))).unwrap();
        };
        for (bus, text) in [("Vehicles/MAN_SD200/MAN_SD82.bus", "Hauptbahnhof"), ("Vehicles/HH20_EBus2021/HHEBus2021_main.bus", "Rathausmarkt"), ("Vehicles/Citybus 530 by Kajosoft/01a_o530_e2_2.bus", "Dworzec Glowny")] {
            if !root.join(bus).is_file() {
                eprintln!("skipped: no {bus}");
                continue;
            }
            let sign = read_sign(&root, bus).unwrap().expect("a destination display");
            let stem = Path::new(bus).file_stem().unwrap().to_string_lossy().to_string();
            eprintln!("{stem}: {} ({}, {}x{}, line {} px), {} displays", sign.def.variable, sign.def.font, sign.def.width, sign.def.height, sign.line_h, sign.displays);
            for font in ["", "CRNL_NL2x3_LAWO_16x8", "krueger-font_klein", "Annax Small"] {
                let (atlas, fit) = if font.is_empty() { (sign.own.clone(), None) } else { (lib.display_atlas(font, sign.line_h, &decode), Some(sign.line_h as u32)) };
                let Some(atlas) = atlas else {
                    eprintln!("  {font}: not installed");
                    continue;
                };
                let mut s = omsi_sim::texttex::TextTextureState::new(sign.def.clone(), Some(atlas.clone()));
                s.fit = fit;
                let img = s.image(text);
                let (w, h) = (sign.def.width as u32, sign.def.height as u32);
                let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| img[((y * w + x) * 4 + 3) as usize] > 0)).collect();
                eprintln!("  {}: {} -> rows {:?}..{:?}", if font.is_empty() { "own" } else { font }, atlas.font.name.trim(), rows.first(), rows.last());
                if fit.is_some() && !rows.is_empty() {
                    let top = (h as i32 - sign.line_h) / 2;
                    assert!(rows[0] as i32 >= top && (*rows.last().unwrap() as i32) < top + sign.line_h, "{font} on {bus} runs out of its line");
                }
                let tag = if font.is_empty() { "own".to_string() } else { font.replace(' ', "_") };
                save(&format!("{stem}_{tag}_texture"), w, h, &img, if w <= 256 { 2 } else { 1 });
                let p = sign_picture(&sign, Some(atlas), fit.is_some(), &format!("12 {text}"));
                save(&format!("{stem}_{tag}_preview"), p.w, p.h, &p.rgba, 1);
            }
        }
    }

    #[test]
    fn a_choice_is_kept_for_the_bus_and_goes_to_the_game() {
        let mut df = DisplayFonts::in_memory();
        assert_eq!(df.font_for("Vehicles/MAN_SD200/SD200.bus"), None);
        df.edit("C:/OMSI", "Vehicles/MAN_SD200/SD200.bus", Edit::Pick(Some("Annax Small".into())));
        assert_eq!(df.font_for("vehicles/man_sd200/sd200.bus").as_deref(), Some("Annax Small"));
        df.edit("C:/OMSI", "Vehicles/MAN_SD200/SD200.bus", Edit::Pick(None));
        assert_eq!(df.font_for("Vehicles/MAN_SD200/SD200.bus"), None);
        assert_eq!(preview_key("Vehicles\\A.bus", "12 X", " Annax Small"), "vehicles/a.bus|12 X|annax small");
    }
}

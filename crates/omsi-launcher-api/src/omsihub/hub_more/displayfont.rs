//! The bus step's display font (Trello "Tauri 3/6"), as our game-drawn launcher's
//! (`crates/omsi-launcher-ui/src/displayfont.rs`, its reading and drawing taken over as they
//! are): the font the chosen bus's destination displays are drawn in - "As the bus", an OMSI
//! `.oft` font or a TrueType/OpenType one - each offered with the duty's destination written
//! in it as a small LED sign the way the game draws it on this bus (`omsi_content::dotfont`,
//! `omsi_sim::texttex::substitute`: the same atlas the game draws with). A chosen font has the
//! settings that matter on a display: how tall its letters are, bold, the spacing. "Add font…"
//! copies a font into the content folder's `Fonts`. The choice is kept per bus file
//! (`omsi_launcher_lib::busfonts`, the file the game-drawn launcher keeps it in) and goes to the
//! game through the Omsi-Hub plugin, `bus.set_display_font` (`hub_duty_extras`).
//!
//! The previews are drawn on a worker and handed to the page as pixels (`hub_more_font_pic`:
//! width and height, then RGBA); what a font changes on the bus is answered as what it is, the
//! page says it in words.

use super::busoptions::player_bus_path;
use crate::Answer;
use omsi_content::dotfont::{DisplayFontSpec, DotGrid, LetterBand, VectorFace};
use omsi_content::font::{resample_into, Font, FontAtlas};
use omsi_launcher_lib as core;
use omsi_launcher_lib::busfonts::{self, BusFonts};
use omsi_model::TextTexture;
use omsi_sim::texttex::{DisplayRole, ScriptSign, Target};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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
/// The share of a destination's letters a font must have to be offered for it.
const ENOUGH_LETTERS: f32 = 0.6;
/// The share of the bus's own font's ink a font must come out with on its sign to be offered
/// for it (a font that still cannot be drawn in the sign's dots would leave it dark).
const ENOUGH_INK: f32 = 0.2;

// --- a bus's signs (as `displayfont.rs`) ------------------------------------------------------

/// A bus's destination display as the previews draw it: the widest of its destination
/// displays (the terminus, where a line number has its own beside it) and its own font - or,
/// on a bus whose script draws its matrix, that matrix's largest letter font.
#[derive(Clone)]
pub struct Sign {
    pub def: TextTexture,
    pub own: Option<Arc<FontAtlas>>,
    /// A line of the display's own font: what a chosen font is fitted to.
    pub line_h: i32,
    /// How many destination displays the bus has (the main model's).
    pub displays: usize,
    /// What the display shows: a line number's preview writes the line alone.
    pub role: DisplayRole,
    /// What a chosen font is fitted to on the display (`omsi_sim::texttex::Target::read`): the
    /// grid of dots its own font draws in, where its letters sit (filling its rows without a
    /// font of its own), its letters and pictograms - read once, not for every font previewed
    /// (a large picture of LEDs takes a while).
    pub target: Target,
}

impl Sign {
    pub fn new(def: TextTexture, own: Option<Arc<FontAtlas>>, line_h: i32, displays: usize, role: DisplayRole) -> Sign {
        let target = Target::read(own.as_deref(), line_h.max(1) as u32);
        Sign { def, own, line_h, displays, role, target }
    }

    /// The grid of dots the display's own font draws in (see `DotGrid::of_atlas`).
    pub fn grid(&self) -> DotGrid {
        self.target.grid
    }

    /// Where the letters of the display's own font sit (see `LetterBand::of`).
    pub fn band(&self) -> &LetterBand {
        &self.target.band
    }

    /// What its preview writes of the destination `text` ("12 Hauptbahnhof"): a line number's
    /// display the line alone.
    pub fn text_of<'a>(&self, text: &'a str) -> &'a str {
        match self.role {
            DisplayRole::LineNumber => text.split_whitespace().next().unwrap_or(text),
            _ => text,
        }
    }
}

/// What a display font changes on a bus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Its text displays a font draws, by what they show.
    pub texts: Vec<DisplayRole>,
    /// The matrices its scripts draw with fonts they ask for by name (the font takes their
    /// places there).
    pub scripts: Vec<ScriptSign>,
    /// It shows pictures its scripts draw on its outside (a matrix) that no font draws: its
    /// destination drawn by itself, pixel by pixel or from bitmaps.
    pub pictures: bool,
    /// It has text displays of the cab or the saloon (IBIS, printer, the passengers'
    /// display inside), which keep their own fonts.
    pub devices: bool,
}

/// What a display font changes on the bus of `model` whose scripts are `scripts` (see `Plan`).
pub fn plan_of(model: &omsi_model::Model, scripts: &[PathBuf]) -> Plan {
    let displays = omsi_sim::texttex::destination_displays(model);
    let texts: Vec<DisplayRole> = displays.iter().map(|&i| DisplayRole::of(&model.text_textures[i])).collect();
    let signs = omsi_sim::texttex::script_signs(scripts);
    // (a mesh seen from outside showing a script's picture, as its texture or its transparency
    // map `\S:n`, that is no device of the cab or the saloon)
    let devices_words = ["ibis", "cockpit", "dash", "drucker", "printer", "ticket", "innen", "interior", "monitor", "tacho", "radio"];
    let pictures = !model.script_textures.is_empty()
        && model.meshes.iter().any(|m| {
            let outside = m.viewpoint == 0 || m.viewpoint & 1 != 0;
            let file = m.file.to_lowercase();
            outside && !devices_words.iter().any(|w| file.contains(w)) && m.materials.iter().any(|t| t.use_script_texture.is_some() || t.transmap.as_deref().is_some_and(|tm| tm.trim().to_ascii_lowercase().starts_with("\\s:")))
        });
    let device_words = ["ibis", "drucker", "printer", "ticket", "innen", "interior", "cockpit", "dash", "almex"];
    let devices = model.text_textures.iter().enumerate().any(|(i, t)| !displays.contains(&i) && device_words.iter().any(|w| t.variable.to_lowercase().contains(w) || t.font.to_lowercase().contains(w)));
    Plan { texts, scripts: signs, pictures, devices }
}



/// A bus's signs: what a font changes, and the sign its previews draw (none: no font draws
/// any of its displays) - or that its files could not be read (`failed`: then neither is
/// known).
#[derive(Clone)]
pub struct BusSigns {
    pub plan: Plan,
    pub sign: Option<Sign>,
    pub failed: bool,
}

/// Read `bus`'s destination displays and what a font changes on it.
pub fn read_signs(root: &Path, bus: &str) -> anyhow::Result<BusSigns> {
    use anyhow::Context;
    let path = player_bus_path(root, bus)?;
    let def = omsi_vehicle::Vehicle::load(&path).with_context(|| format!("loading {}", path.display()))?;
    let model_rel = def.model.clone().context("the bus has no [model]")?;
    let model_path = omsi_cfg::resolve_path(def.dir(), &model_rel);
    let model = omsi_model::Model::load(&model_path).with_context(|| format!("loading {}", model_path.display()))?;
    let plan = plan_of(&model, &def.scripts.scripts);
    let mut lib = omsi_sim::texttex::FontLibrary::new(root);
    let displays = omsi_sim::texttex::destination_displays(&model);
    // the terminus (a destination before a line number), the widest of them
    let main = displays.iter().copied().max_by_key(|&i| (DisplayRole::of(&model.text_textures[i]) != DisplayRole::LineNumber, model.text_textures[i].width, std::cmp::Reverse(i)));
    if let Some(main) = main {
        let t = model.text_textures[main].clone();
        let own = lib.load(&t.font);
        let line_h = own.as_ref().map(|a| a.font.height).filter(|h| *h > 0).unwrap_or(t.height).max(1);
        let role = DisplayRole::of(&t);
        return Ok(BusSigns { plan, sign: Some(Sign::new(t, own, line_h, displays.len(), role)), failed: false });
    }
    // a matrix a script draws: its tallest font of small letters too, of the most letters (the
    // one line of a destination, not the line number's)
    for s in &plan.scripts {
        let tallest = s.fonts.iter().filter_map(|f| lib.load(f)).filter(|a| omsi_sim::texttex::is_letter_font(&a.font) && ('a'..='z').filter(|&c| a.font.has_glyph(c)).count() >= 13).max_by_key(|a| (a.font.height, a.font.chars.len()));
        if let Some(own) = tallest {
            let stem = s.script.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let def = TextTexture { variable: stem, font: own.font.name.trim().to_string(), width: 0, height: own.font.height, full_color: false, color: [0.0; 3], orientation: 0, grid: 1 };
            let line_h = own.font.height.max(1);
            return Ok(BusSigns { plan: plan.clone(), sign: Some(Sign::new(def, Some(own), line_h, 0, DisplayRole::Destination)), failed: false });
        }
    }
    Ok(BusSigns { plan, sign: None, failed: false })
}

/// Read `bus`'s destination displays (None: it has none a font draws).
#[allow(dead_code)]
pub fn read_sign(root: &Path, bus: &str) -> anyhow::Result<Option<Sign>> {
    Ok(read_signs(root, bus)?.sign)
}

/// A preview's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// `text` on a small LED sign: as `sign`'s display draws it in `atlas` - a chosen font
/// (`fitted`: made for the display's line, see `atlas_for`), or the bus's own - the line cut
/// out of the texture, as wide as its letters, on a dark panel. A pixel font is magnified by
/// whole times to about `PREVIEW_ROWS` rows with the dots of a matrix sign between its pixels;
/// a taller font is shrunk to them.
pub fn sign_picture(sign: &Sign, atlas: Option<Arc<FontAtlas>>, fitted: bool, text: &str) -> Picture {
    let (img, w, top, rows) = sign_line(sign, atlas, fitted, text);
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

/// How much ink `text` has on `sign`'s display in `atlas` (`sign_picture`'s line, its pixels
/// drawn).
fn ink_on(sign: &Sign, atlas: &Arc<FontAtlas>, fitted: bool, text: &str) -> usize {
    let (img, w, top, rows) = sign_line(sign, Some(atlas.clone()), fitted, text);
    img[(top * w * 4) as usize..((top + rows) * w * 4) as usize].chunks(4).filter(|p| p[3] > 0).count()
}

/// The texture `sign_picture` cuts its line out of: its pixels and width, and the line's top
/// and rows.
fn sign_line(sign: &Sign, atlas: Option<Arc<FontAtlas>>, fitted: bool, text: &str) -> (Vec<u8>, u32, u32, u32) {
    // (a chosen font made for the display's own font is placed as that font is: fitted only on
    // a display of no font of its own, as the game draws it)
    let fitted = fitted && sign.own.is_none();
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
    def.height = def.height.max(line_h as i32);
    let (w, h) = (def.width.max(1) as u32, def.height.max(1) as u32);
    let mut state = omsi_sim::texttex::TextTextureState::new(def.clone(), atlas);
    state.fit = fitted.then_some(line_h);
    let img = state.image(text);
    // the line's rows (where the display's own font has its line)
    let top = ((h as i32 - line_h as i32) / 2).max(0) as u32;
    let rows = line_h.min(h - top.min(h)).max(1);
    (img, w, top, rows)
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
    let mut out: Vec<Font> = fonts.iter().filter(|f| coverage(f, text) >= ENOUGH_LETTERS || keep.as_deref() == Some(f.name.trim().to_ascii_lowercase().as_str())).cloned().collect();
    out.sort_by(|a, b| super::name_cmp(a.name.trim(), b.name.trim()));
    out.dedup_by(|a, b| a.name.trim().eq_ignore_ascii_case(b.name.trim()));
    out
}

/// The letters a preview of `text` needs from a vector font: its own and their capitals.
fn letters_of(text: &str) -> Vec<char> {
    let mut out: Vec<char> = text.chars().flat_map(|c| std::iter::once(c).chain(c.to_uppercase())).filter(|c| !c.is_whitespace()).collect();
    out.sort();
    out.dedup();
    out
}

/// The display font `spec` as `sign`'s display draws it - what the game draws with
/// (`omsi_sim::texttex::substitute`, as `FontLibrary::display_font` makes it), for the letters
/// of `text` - and the share of them it has. An `.oft` font from `lib` (the size of its file the
/// game picks for the sign's letters, its bitmaps read with `decode`), a vector font from its
/// file.
pub fn atlas_for(sign: &Sign, spec: &DisplayFontSpec, lib: &mut omsi_sim::texttex::FontLibrary, text: &str, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<(FontAtlas, f32)> {
    use omsi_sim::texttex::{substitute, SubstituteSource};
    let mut target = sign.target.sized(spec.rows);
    // (a preview writes its text alone: its letters, each drawn as the game draws it)
    let wanted = letters_of(text);
    target.chars.retain(|c| wanted.contains(c));
    let own = sign.own.as_deref();
    if spec.is_vector() {
        let data = spec.file.as_deref().and_then(|f| std::fs::read(f).ok())?;
        let share = omsi_content::dotfont::vector_coverage(&data, spec.face, text);
        substitute(SubstituteSource::Vector(&data, spec.face), spec, &target, own).map(|s| (s.atlas, share))
    } else {
        let dots = lib.oft_size(&spec.name, &target.band, decode)?;
        let share = dots.0.coverage(text);
        substitute(SubstituteSource::Oft(&dots.0), spec, &target, own).map(|s| (s.atlas, share))
    }
}

// --- the launcher's side ----------------------------------------------------------------------

enum Slot<T> {
    Reading,
    Ready(T),
    Failed,
}

/// The previews being drawn for one bus and text: what a preview's key is made of.
fn preview_key(bus: &str, text: &str, font: &str) -> String {
    format!("{}|{text}|{}", core::busoptions::bus_key(bus), font.trim().to_ascii_lowercase())
}

/// What is read once and kept: each bus's signs, the installed fonts and faces, the previews
/// drawn and the fonts found unable to write a text, and the worker drawing them (its number:
/// a newer one stops it).
#[derive(Default)]
struct Kept {
    signs: HashMap<String, Slot<BusSigns>>,
    fonts: HashMap<String, Slot<Arc<Vec<Font>>>>,
    faces: HashMap<String, Slot<Arc<Vec<VectorFace>>>>,
    pictures: HashMap<String, Arc<Picture>>,
    unfit: HashSet<String>,
    drawing: Option<String>,
}

fn kept() -> &'static Mutex<Kept> {
    static K: OnceLock<Mutex<Kept>> = OnceLock::new();
    K.get_or_init(Default::default)
}

fn lock() -> std::sync::MutexGuard<'static, Kept> {
    kept().lock().unwrap_or_else(|e| e.into_inner())
}

static WORKER: AtomicU64 = AtomicU64::new(0);

/// `bus`'s signs (None while they are read: asked for now, the first time).
fn signs(root: &Path, bus: &str) -> Option<BusSigns> {
    let key = format!("{}|{}", root.display(), core::busoptions::bus_key(bus));
    let mut k = lock();
    match k.signs.get(&key) {
        Some(Slot::Ready(s)) => return Some(s.clone()),
        Some(Slot::Failed) => return Some(BusSigns { plan: Plan::default(), sign: None, failed: true }),
        Some(Slot::Reading) => return None,
        None => {}
    }
    k.signs.insert(key.clone(), Slot::Reading);
    let (root, bus) = (root.to_path_buf(), bus.to_string());
    std::thread::spawn(move || {
        let slot = match std::panic::catch_unwind(|| read_signs(&root, &bus)) {
            Ok(Ok(s)) => Slot::Ready(s),
            _ => Slot::Failed,
        };
        lock().signs.insert(key, slot);
    });
    None
}

/// The installed `.oft` fonts (None while they are read).
fn fonts(root: &Path) -> Option<Arc<Vec<Font>>> {
    let key = root.display().to_string();
    let mut k = lock();
    match k.fonts.get(&key) {
        Some(Slot::Ready(f)) => return Some(f.clone()),
        Some(_) => return None,
        None => {}
    }
    k.fonts.insert(key.clone(), Slot::Reading);
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let list = std::panic::catch_unwind(|| omsi_sim::texttex::FontLibrary::new(&root).installed());
        lock().fonts.insert(key, list.map(|l| Slot::Ready(Arc::new(l))).unwrap_or(Slot::Failed));
    });
    None
}

/// The TrueType/OpenType faces installed - the content folders' first, then the system's, each
/// name once (None while they are read).
fn faces(root: &Path) -> Option<Arc<Vec<VectorFace>>> {
    let key = root.display().to_string();
    let mut k = lock();
    match k.faces.get(&key) {
        Some(Slot::Ready(f)) => return Some(f.clone()),
        Some(_) => return None,
        None => {}
    }
    k.faces.insert(key.clone(), Slot::Reading);
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let list = std::panic::catch_unwind(|| {
            let mut faces = omsi_sim::texttex::FontLibrary::new(&root).vector_faces().to_vec();
            let mut seen = HashSet::new();
            faces.retain(|f| seen.insert(f.name.trim().to_lowercase()));
            let system = omsi_content::dotfont::system_font_dirs();
            faces.sort_by(|a, b| {
                let sys = |f: &VectorFace| system.iter().any(|d| f.path.starts_with(d));
                sys(a).cmp(&sys(b)).then_with(|| super::name_cmp(a.name.trim(), b.name.trim()))
            });
            faces
        });
        lock().faces.insert(key, list.map(|l| Slot::Ready(Arc::new(l))).unwrap_or(Slot::Failed));
    });
    None
}

/// Draw the previews of `bus`'s sign with `text` - as the bus, the chosen font with its
/// settings, then every font of the list - on a worker, unless they are being drawn already;
/// the previews of another bus or text are let go (`DisplayFonts::draw`).
fn draw(bus: &str, text: &str, sign: &Sign, fonts: Arc<Vec<Font>>, list: Vec<DisplayFontSpec>, chosen: Option<DisplayFontSpec>) {
    let listed = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for s in &list {
            s.key().hash(&mut h);
        }
        h.finish()
    };
    let what = format!("{}|{}|{listed:x}", preview_key(bus, text, ""), chosen.as_ref().map(|c| c.key()).unwrap_or_default());
    let done: Vec<String> = {
        let mut k = lock();
        if k.drawing.as_deref() == Some(what.as_str()) {
            return;
        }
        k.drawing = Some(what);
        let prefix = format!("{}|", preview_key(bus, text, "").trim_end_matches('|'));
        k.pictures.retain(|key, _| key.starts_with(&prefix));
        k.pictures.keys().cloned().collect()
    };
    let me = WORKER.fetch_add(1, Ordering::SeqCst) + 1;
    let (sign, bus, text) = (sign.clone(), bus.to_string(), text.to_string());
    std::thread::spawn(move || {
        let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
        let mut lib = omsi_sim::texttex::FontLibrary::with_fonts(Path::new(""), fonts.as_ref().clone());
        let shown = sign.text_of(&text).to_string();
        let own_ink = sign.own.as_ref().map(|a| ink_on(&sign, a, false, &shown)).unwrap_or(0);
        let put = |key: &str, pic: Picture| {
            lock().pictures.insert(preview_key(&bus, &text, key), Arc::new(pic));
        };
        if !done.contains(&preview_key(&bus, &text, "")) {
            put("", sign_picture(&sign, sign.own.clone(), false, &shown));
        }
        let order = chosen.iter().cloned().chain(list.into_iter().filter(|s| chosen.as_ref().is_none_or(|c| c.key() != s.key())));
        for spec in order {
            if WORKER.load(Ordering::SeqCst) != me {
                return;
            }
            let key = spec.key();
            let pk = preview_key(&bus, &text, &key);
            if done.contains(&pk) || lock().unfit.contains(&pk) {
                continue;
            }
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| atlas_for(&sign, &spec, &mut lib, &shown, &decode))).ok().flatten().map(|(a, share)| (Arc::new(a), share));
            let other = chosen.as_ref().is_none_or(|c| !c.same_font(&spec));
            match made {
                // (a vector font that cannot write the destination, or a font that comes out on
                // this sign with next to none of its own font's ink: not offered for it)
                Some((ref a, share)) if other && ((spec.is_vector() && share < ENOUGH_LETTERS) || (own_ink > 0 && (ink_on(&sign, a, true, &shown) as f32) < own_ink as f32 * ENOUGH_INK)) => {
                    lock().unfit.insert(pk);
                }
                Some((a, _)) => put(&key, sign_picture(&sign, Some(a), true, &shown)),
                None if spec.is_vector() => {
                    lock().unfit.insert(pk);
                }
                None => {}
            }
        }
    });
}

fn arg_str(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// What a font changes on the bus, as it is (the page says it in words, `plan_text`).
fn plan_json(plan: &Plan) -> Value {
    let mut roles: Vec<DisplayRole> = plan.texts.clone();
    roles.sort();
    roles.dedup();
    let role = |r: &DisplayRole| match r {
        DisplayRole::Destination => "destination",
        DisplayRole::LineNumber => "line",
        DisplayRole::Side => "side",
        DisplayRole::Rear => "rear",
    };
    let script = plan.scripts.first().map(|s| s.script.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    json!({
        "texts": roles.iter().map(role).collect::<Vec<_>>(),
        "script": script,
        "kept": plan.scripts.iter().any(|s| !s.kept.is_empty()),
        "script_pictures": plan.scripts.iter().any(|s| s.pictures),
        "pictures": plan.pictures,
        "devices": plan.devices,
    })
}

/// The display font section for `bus` with the destination `text` (`displayfont::section`):
/// `{ reading }`, `{ failed }`, a bus whose displays no font draws (`{ plan }`), else what a
/// font changes, its sign, the font chosen with its settings, the list shown (`source`: 0 OMSI's
/// fonts, 1 TrueType/OpenType ones; none: the kind of the font chosen) - as the bus first, the
/// fonts that can write the text, a font chosen that is not in it - and the one chosen in it.
pub fn view(args: &Value) -> Answer {
    let (bus, text) = (arg_str(args, "bus"), arg_str(args, "text"));
    let Ok(root) = core::root_dir() else { return Ok(json!({ "failed": true })) };
    if bus.trim().is_empty() {
        return Ok(json!({ "failed": true }));
    }
    let (signs, fonts) = (signs(&root, &bus), fonts(&root));
    let (signs, fonts) = match (signs, fonts) {
        (Some(BusSigns { failed: true, .. }), _) => return Ok(json!({ "failed": true })),
        (Some(BusSigns { plan, sign: None, .. }), _) => return Ok(json!({ "plan": plan_json(&plan) })),
        (Some(s), Some(f)) => (s, f),
        _ => return Ok(json!({ "reading": true })),
    };
    let sign = signs.sign.clone().expect("a sign");
    let chosen = BusFonts::load().spec_for(&bus);
    let source = args.get("source").and_then(Value::as_u64).map(|s| s.min(1) as usize).unwrap_or_else(|| usize::from(chosen.as_ref().is_some_and(|s| s.is_vector())));
    let mut faces_reading = false;
    let mut entries: Vec<(DisplayFontSpec, String)> = if source == 0 {
        let keep = chosen.as_ref().filter(|c| !c.is_vector()).map(|c| c.name.clone());
        offered(&fonts, &text, keep.as_deref()).iter().map(|f| (DisplayFontSpec::named(f.name.trim()), f.name.trim().to_string())).collect()
    } else {
        match faces(&root) {
            Some(faces) => faces.iter().map(|f| (f.spec(), f.name.clone())).collect(),
            None => {
                faces_reading = true;
                Vec::new()
            }
        }
    };
    draw(&bus, &text, &sign, fonts.clone(), entries.iter().map(|e| e.0.clone()).collect(), chosen.clone());
    let (unfit, drawn): (HashSet<String>, HashSet<String>) = {
        let k = lock();
        (k.unfit.clone(), k.pictures.keys().cloned().collect())
    };
    entries.retain(|e| chosen.as_ref().is_some_and(|c| c.same_font(&e.0)) || !unfit.contains(&preview_key(&bus, &text, &e.0.key())));
    let option = |key: String, label: String, arg: Option<String>| json!({ "key": key, "label": label, "arg": arg, "drawn": drawn.contains(&preview_key(&bus, &text, &key)) });
    let mut options: Vec<Value> = vec![option(String::new(), String::new(), None)];
    options.extend(entries.iter().map(|(s, label)| option(s.key(), label.clone(), Some(s.to_arg()))));
    let sel = match chosen.as_ref() {
        None => 0,
        Some(c) => match entries.iter().position(|e| e.0.same_font(c)) {
            Some(i) => i + 1,
            None => {
                let installed = if c.is_vector() { c.file.as_deref().is_some_and(Path::is_file) } else { fonts.iter().any(|f| f.name.trim().eq_ignore_ascii_case(c.name.trim())) };
                options.push(json!({ "key": c.key(), "label": c.name, "arg": c.to_arg(), "missing": !installed, "drawn": drawn.contains(&preview_key(&bus, &text, &c.key())) }));
                options.len() - 1
            }
        },
    };
    let grid = sign.grid();
    let band = chosen.as_ref().and_then(|c| c.rows).map(|r| sign.band().scaled_to(r)).unwrap_or_else(|| sign.band().clone());
    Ok(json!({
        "plan": plan_json(&signs.plan),
        "sign": { "font": sign.def.font.trim(), "displays": sign.displays, "rows": grid.rows, "fill": sign.band().fill, "caps": band.cap_rows() },
        "chosen": chosen.as_ref().map(|c| json!({ "key": c.key(), "name": c.name, "vector": c.is_vector(), "rows": c.rows, "bold": c.bold, "spacing": c.spacing })),
        "source": source,
        "faces_reading": faces_reading,
        "options": options,
        "sel": sel,
    }))
}

/// A preview drawn (`key`: a font's, "" as the bus): its width and height (little-endian
/// `u32`s), then its pixels (RGBA); an error while it is not drawn yet.
pub fn picture(args: &Value) -> Result<Vec<u8>, String> {
    let (bus, text, key) = (arg_str(args, "bus"), arg_str(args, "text"), arg_str(args, "key"));
    let pic = lock().pictures.get(&preview_key(&bus, &text, &key)).cloned().ok_or("not drawn yet")?;
    let mut out = Vec::with_capacity(8 + pic.rgba.len());
    out.extend_from_slice(&pic.w.to_le_bytes());
    out.extend_from_slice(&pic.h.to_le_bytes());
    out.extend_from_slice(&pic.rgba);
    Ok(out)
}

/// A font chosen for `bus` (`font`: a `DisplayFontSpec` argument, null: as the bus), the
/// settings chosen so far staying with it; or the chosen font's settings (`style`: rows, bold,
/// spacing). Kept at once (`DisplayFonts::edit`).
pub fn set(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    let mut choices = BusFonts::load();
    if let Some(style) = args.get("style") {
        if let Some(mut f) = choices.spec_for(&bus) {
            f.rows = style.get("rows").and_then(Value::as_u64).map(|r| r as u32);
            f.bold = style.get("bold").and_then(Value::as_bool).unwrap_or(false);
            f.spacing = style.get("spacing").and_then(Value::as_u64).map(|s| s as u32);
            choices.set_spec(&bus, Some(&f));
        }
    } else {
        let style = choices.spec_for(&bus);
        let font = args.get("font").and_then(Value::as_str).and_then(DisplayFontSpec::parse).map(|mut f| {
            if let Some(s) = style.as_ref() {
                (f.rows, f.bold, f.spacing) = (s.rows, s.bold, s.spacing);
            }
            f
        });
        choices.set_spec(&bus, font.as_ref());
    }
    choices.save().map_err(|e| e.to_string())?;
    Ok(json!(true))
}

/// "Add font…": a font file chosen in a file dialog into the content folder's `Fonts`, and its
/// first font chosen for `bus`: `{ names, missing }`, `{ error }`, or null when none was chosen.
pub fn add(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    let title = arg_str(args, "title");
    let filter = arg_str(args, "filter");
    let Some(file) = core::pick_file_of(&title, &filter, busfonts::FONT_EXTENSIONS) else { return Ok(Value::Null) };
    let Some(dir) = busfonts::content_fonts_dir() else { return Ok(json!({ "error": "no content folder" })) };
    match busfonts::add_font(&file, &dir) {
        Ok(added) => {
            // (the fonts read again, with the new one; it is chosen for the bus)
            if let Ok(root) = core::root_dir() {
                let mut k = lock();
                k.fonts.remove(&root.display().to_string());
                k.faces.remove(&root.display().to_string());
                k.drawing = None;
            }
            let first = added.names.first().map(|n| match added.file.as_deref() {
                Some(f) => DisplayFontSpec::vector(n, f, 0),
                None => DisplayFontSpec::named(n),
            });
            if let Some(first) = first {
                set(&json!({ "bus": bus, "font": first.to_arg() }))?;
            }
            Ok(json!({ "names": added.names, "missing": added.missing, "vector": added.file.is_some() }))
        }
        Err(e) => Ok(json!({ "error": format!("{e:#}") })),
    }
}

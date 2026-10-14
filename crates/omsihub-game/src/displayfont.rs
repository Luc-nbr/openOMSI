//! A text on a bus's own destination display, as the game draws it: in the grid of dots of the
//! bus's own font, a line of its height, its capitals as tall as the bus's and about as wide
//! (`omsi_content::dotfont`, `omsi_sim::texttex::substitute` - the same atlas the game draws
//! with), or in a font chosen for the bus (`omsi_launcher_lib::busfonts`, the bus step's display
//! font, which goes to the game as `--display-font`). The desktop launcher's depot and line
//! editors and its bus step show such a sign (`service::display`).
//!
//! What a font changes on the bus is said as it is ([`Plan`]): its text displays a font draws
//! (the destination, the line number, a side or rear sign), the matrices its scripts write
//! with fonts they ask the game for by name (the Krüger matrices: the font takes those fonts'
//! places), and when the bus draws its destination as pictures of its own.

use omsi_content::dotfont::{DisplayFontSpec, DotGrid, LetterBand};
use omsi_content::font::{resample_into, Font, FontAtlas};
use omsi_model::TextTexture;
use omsi_sim::texttex::{DisplayRole, ScriptSign, Target};
use omsi_ui::tr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
/// What the previews write when no duty gives a destination.
pub const SAMPLE: &str = "12 Hauptbahnhof";

// --- a bus's signs ----------------------------------------------------------------------------

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

/// What and how much of the bus a chosen font changes, in words (see `Plan`).
pub fn plan_text(plan: &Plan) -> String {
    let mut roles: Vec<DisplayRole> = plan.texts.clone();
    roles.sort();
    roles.dedup();
    let what = roles
        .iter()
        .map(|r| match r {
            DisplayRole::Destination => tr("the destination"),
            DisplayRole::LineNumber => tr("the line number"),
            DisplayRole::Side => tr("the side sign"),
            DisplayRole::Rear => tr("the rear sign"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = plan.scripts.first() {
        let script = s.script.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        parts.push(tr("This bus's matrix script (%{script}) writes its destination and line number itself, in fonts it asks the game for by name: a chosen font takes their place there, each fitted to the height, the baseline and the width of the letters it replaces, so the matrix picks its sizes as with its own fonts.").replace("%{script}", &script));
        if !what.is_empty() {
            parts.push(tr("It also changes %{what}.").replace("%{what}", &what));
        }
        if plan.scripts.iter().any(|s| !s.kept.is_empty()) {
            parts.push(tr("The matrix's pictograms and clock keep their own.").into_owned());
        }
        if plan.scripts.iter().any(|s| s.pictures) {
            parts.push(tr("Destinations its depot file gives as pictures stay pictures.").into_owned());
        }
    } else if plan.pictures && !what.is_empty() && !roles.contains(&DisplayRole::Destination) {
        parts.push(tr("This bus draws its destination itself: a font changes only %{what}.").replace("%{what}", &what));
    } else if !what.is_empty() {
        parts.push(tr("A chosen font changes %{what}.").replace("%{what}", &what));
    } else {
        parts.push(tr("This bus draws its destination displays as pictures of its own: a display font does not change them.").into_owned());
    }
    if plan.devices && (!plan.scripts.is_empty() || !what.is_empty()) {
        parts.push(tr("The cab's and the saloon's displays (IBIS, printer, inside) keep their own fonts.").into_owned());
    }
    parts.join(" ")
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
    let path = crate::spawn::player_bus_path(root, bus)?;
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
    out.sort_by(|a, b| crate::words::name_cmp(a.name.trim(), b.name.trim()));
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
        Sign::new(def, None, line_h, 2, DisplayRole::Destination)
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

    /// An `.oft` font's preview is drawn with the atlas the game draws with: the size of its
    /// file the game picks for the sign's letters (by their capitals' height), the same dots.
    #[test]
    fn an_oft_font_is_previewed_as_the_game_draws_it() {
        // a file of two sizes, block letters 7 and 16 rows high, its bitmaps by their names
        let (small, small_atlas) = pixel_font("Krueger 7x4", 7, "ABFHUP12 ");
        let (big, big_atlas) = pixel_font("Krueger 16x9", 16, "ABFHUP12 ");
        let in_file = |f: Font| Font { path: PathBuf::from("/fonts/Krueger.oft"), alpha: format!("{}.bmp", f.name), bitmap: format!("{}.bmp", f.name), ..f };
        let fonts = vec![in_file(small), in_file(big)];
        let bitmaps: Vec<(String, u32, u32, Vec<u8>)> = [small_atlas, big_atlas].into_iter().map(|a| (format!("{}.bmp", a.font.name), a.width, a.height, a.alpha)).collect();
        let decode = move |p: &Path| -> Option<(u32, u32, Vec<u8>)> { bitmaps.iter().find(|b| p.file_name().is_some_and(|n| n.to_string_lossy() == b.0)).map(|b| (b.1, b.2, b.3.clone())) };
        // the bus's own: capitals of 7 rows in a line of 9
        let (_, own) = pixel_font("Own", 7, "ABFHUP12 ");
        let own = Arc::new(FontAtlas::new(Font { height: 9, ..own.font.clone() }, own.width, 9, [own.alpha.clone(), vec![0; (own.width * 2 * 4) as usize]].concat(), [own.alpha, vec![0; (own.width * 2 * 4) as usize]].concat()));
        let mut s = sign(9);
        s = Sign::new(s.def, Some(own.clone()), 9, 1, DisplayRole::Destination);
        assert_eq!((s.band().cap_top, s.band().cap_rows()), (0, 7));
        let spec = DisplayFontSpec::named("Krueger 16x9");
        let mut lib = omsi_sim::texttex::FontLibrary::with_fonts(Path::new("/fonts"), fonts.clone());
        assert_eq!(lib.oft_size(&spec.name, s.band(), &decode).map(|d| d.0.name.clone()).as_deref(), Some("Krueger 7x4"), "too tall: the size of its file whose capitals fit");
        let (a, share) = atlas_for(&s, &spec, &mut lib, "12 HBF", &decode).unwrap();
        assert_eq!(share, 1.0);
        let mut game = omsi_sim::texttex::FontLibrary::with_fonts(Path::new("/fonts"), fonts);
        let from_game = game.display_font(&spec, Some(&own), 9, &decode).unwrap();
        let glyph = |a: &FontAtlas, c: char| {
            let g = a.font.chars.iter().find(|g| g.ch == c).unwrap().clone();
            (0..9).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect::<Vec<u8>>()
        };
        for c in ['H', 'B', '1'] {
            assert_eq!(glyph(&a, c), glyph(&from_game, c), "{c}: the same dots as the game's");
        }
        assert_eq!(a.font.height, 9);
        assert!(atlas_for(&s, &DisplayFontSpec::named("Missing"), &mut lib, "12 HBF", &decode).is_none());
    }

    /// A vector font's preview is drawn with the atlas the game draws with: the same dots in
    /// the display's grid, as wide as its letters.
    #[test]
    fn a_vector_font_is_previewed_as_the_game_draws_it() {
        let dir = std::env::temp_dir().join(format!("omsi_preview_vector_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Hanken.ttf");
        std::fs::write(&file, include_bytes!("../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf")).unwrap();
        // a matrix whose pixels are its LEDs, 16 rows (a script's churafont)
        let s = sign(16);
        let mut spec = DisplayFontSpec::vector("Hanken Grotesk Bold", &file, 0);
        let mut lib = omsi_sim::texttex::FontLibrary::with_fonts(&dir, Vec::new());
        let (a, share) = atlas_for(&s, &spec, &mut lib, "12 Hauptbahnhof", &|_| None).unwrap();
        assert_eq!(share, 1.0);
        assert_eq!(a.font.height, 16);
        let mut game = omsi_sim::texttex::FontLibrary::new(&dir);
        let from_game = game.display_font_for(&spec, None, 16).unwrap();
        let glyph = |a: &FontAtlas, c: char| {
            let g = a.font.chars.iter().find(|g| g.ch == c).unwrap().clone();
            (0..16).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect::<Vec<u8>>()
        };
        assert_eq!(glyph(&a, 'H'), glyph(&from_game, 'H'), "the same dots as the game's");
        assert!(glyph(&a, 'H').iter().all(|v| *v == 0 || *v == 255), "on or off, as a matrix");
        // bold: wider
        spec.bold = true;
        let (b, _) = atlas_for(&s, &spec, &mut lib, "H", &|_| None).unwrap();
        assert!(glyph(&b, 'H').len() > glyph(&a, 'H').len());
        let p = sign_picture(&s, Some(Arc::new(a)), true, "12 Hauptbahnhof");
        assert!(p.w > 40 && p.h >= 16);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every font of the list is previewed fitted to what the sign read of the bus's own font
    /// once (`Sign::target`), sized to the letter height chosen: the bus's font is not read
    /// again for each (a large picture of LEDs took a tenth of a second every time, half a
    /// minute for the fonts of a system), and the preview is still what the game draws.
    #[test]
    fn a_preview_fits_its_font_to_what_the_sign_read() {
        let dir = std::env::temp_dir().join(format!("omsi_preview_target_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Hanken.ttf");
        std::fs::write(&file, include_bytes!("../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf")).unwrap();
        let (_, own) = pixel_font("Own", 16, "EFHILNT ");
        let own = Arc::new(own);
        let s = Sign::new(sign(16).def, Some(own.clone()), 16, 1, DisplayRole::Destination);
        assert_eq!(s.target, Target::read(Some(&own), 16));
        let mut lib = omsi_sim::texttex::FontLibrary::with_fonts(&dir, Vec::new());
        let mut game = omsi_sim::texttex::FontLibrary::new(&dir);
        let glyph = |a: &FontAtlas, c: char| {
            let g = a.font.chars.iter().find(|g| g.ch == c).unwrap().clone();
            (0..16).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect::<Vec<u8>>()
        };
        let lit_rows = |a: &FontAtlas, c: char| {
            let g = a.font.chars.iter().find(|g| g.ch == c).unwrap().clone();
            (0..16).filter(|y| (g.x0..g.x1).any(|x| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize] > 0)).count()
        };
        for rows in [None, Some(10)] {
            let spec = DisplayFontSpec { rows, ..DisplayFontSpec::vector("Hanken Grotesk Bold", &file, 0) };
            let (a, _) = atlas_for(&s, &spec, &mut lib, "HELL", &|_| None).unwrap();
            let from_game = game.display_font_for(&spec, Some(&own), 16).unwrap();
            assert_eq!(glyph(&a, 'H'), glyph(&from_game, 'H'), "{rows:?}: the same dots as the game's");
        }
        // (its letters as tall as the sign's target says: that one, not the font read again)
        let mut short = s.clone();
        short.target = s.target.sized(Some(6));
        let spec = DisplayFontSpec::vector("Hanken Grotesk Bold", &file, 0);
        let (a, _) = atlas_for(&short, &spec, &mut lib, "HELL", &|_| None).unwrap();
        assert!(lit_rows(&a, 'H') <= 6, "{} rows", lit_rows(&a, 'H'));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a font changes, said as it is: the Lion's City's line number is the passengers'
    /// display inside and its destination matrix a script's (`VMatrix.osc`, fonts by name).
    #[test]
    fn what_a_font_changes_on_the_bus() {
        use omsi_model::{MaterialDef, MeshDef, Model};
        let dir = std::env::temp_dir().join(format!("omsi_font_plan_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("churaKrueger")).unwrap();
        let matrix = dir.join("churaKrueger").join("VMatrix.osc");
        std::fs::write(&matrix, "{macro:Matrix_frame}\n\"churafont++ 32x16 Bold\" (M.V.GetFontIndex) (S.L.Font_A)\n\"churafont++ Numeric 32x24\" (M.V.GetFontIndex) (S.L.Font_B)\n\"churafont++ Pictogram 32\" (M.V.GetFontIndex) (S.L.Font_Icon)\n\"churafont++ Auxiliary 1\" (M.V.GetFontIndex) (S.L.Font_Aux)\n{end}\n{macro:Matrix_Write}\n0 0 0 (L.L.Font_A) 1 0 \"X\" (M.V.STTextOut)\n\"a.bmp\" 0 (M.V.STLoadTex)\n{end}\n").unwrap();
        let ibis = dir.join("IBIS.osc");
        std::fs::write(&ibis, "{macro:x}\n\"IBIS 5x7\" (M.V.GetFontIndex) (S.L.f)\n0 0 0 (L.L.f) 1 0 \"X\" (M.V.STTextOut)\n{end}\n").unwrap();
        let tt = |v: &str, f: &str| TextTexture { variable: v.into(), font: f.into(), width: 200, height: 50, color: [0.0; 3], grid: 1, ..Default::default() };
        let mesh = |file: &str, text: Option<i32>, transmap: Option<&str>| MeshDef { file: file.into(), materials: vec![MaterialDef { use_text_texture: text, transmap: transmap.map(str::to_string), ..Default::default() }], ..Default::default() };
        // the Lion's City: the line number inside, the matrix outside a script's
        let lions = Model {
            text_textures: vec![tt("IBIS", "IBIS-2_5x7"), tt("Matrix_Liniennummerstring", "DIN Narrow")],
            script_textures: vec![(256, 64), (1024, 256)],
            meshes: vec![mesh("IBIS\\ibis_display.o3d", Some(0), None), mesh("Drucker\\Innenanzeige\\Innenanzeige_Linie.o3d", Some(1), None), mesh("A20\\Aussenanzeige.o3d", None, Some("\\S:1"))],
            ..Default::default()
        };
        let plan = plan_of(&lions, &[matrix.clone(), ibis.clone()]);
        assert!(plan.texts.is_empty(), "the passengers' line number inside is no destination display: {plan:?}");
        assert_eq!(plan.scripts.len(), 1, "the matrix script, not the IBIS's");
        assert_eq!(plan.scripts[0].fonts, ["churafont++ 32x16 Bold", "churafont++ Numeric 32x24"]);
        assert_eq!(plan.scripts[0].kept, ["churafont++ Pictogram 32", "churafont++ Auxiliary 1"]);
        assert!(plan.scripts[0].pictures && plan.pictures && plan.devices);
        let says = plan_text(&plan);
        assert!(says.contains("VMatrix.osc") && says.contains("takes their place") && says.contains("pictograms") && says.contains("stay pictures") && says.contains("IBIS"), "{says}");
        // the same bus with its line number outside as a text display and no font script: the
        // destination its own, only the line number changes
        let mut line_outside = lions.clone();
        line_outside.meshes[1].file = "A20\\Linie_front.o3d".into();
        let plan = plan_of(&line_outside, &[ibis.clone()]);
        assert_eq!(plan.texts, [DisplayRole::LineNumber]);
        assert!(plan.scripts.is_empty() && plan.pictures);
        assert_eq!(plan_text(&plan), "This bus draws its destination itself: a font changes only the line number. The cab's and the saloon's displays (IBIS, printer, inside) keep their own fonts.");
        // a bus of text displays: what they show
        let sd200 = Model { text_textures: vec![tt("Matrix_Terminus", "Annax Small"), tt("Matrix_Nr", "Annax Large"), tt("Matrix_Seite", "Annax Small")], meshes: vec![mesh("matrix.o3d", Some(0), None), mesh("matrix.o3d", Some(1), None), mesh("matrix_side.o3d", Some(2), None)], ..Default::default() };
        let plan = plan_of(&sd200, &[]);
        assert_eq!(plan.texts, [DisplayRole::Destination, DisplayRole::LineNumber, DisplayRole::Side]);
        assert_eq!(plan_text(&plan), "A chosen font changes the destination, the line number, the side sign.");
        // nothing a font draws
        let none = Model { script_textures: vec![(64, 32)], meshes: vec![mesh("front.o3d", None, Some("\\S:0"))], ..Default::default() };
        assert!(plan_text(&plan_of(&none, &[])).contains("does not change them"));
        // a line number's preview writes the line alone
        let mut s = sign(14);
        s.role = DisplayRole::LineNumber;
        assert_eq!(s.text_of("12 Hauptbahnhof"), "12");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real buses and fonts of the installed OMSI 2 (`OMSI_ROOT`; skipped without it): each
    /// bus's destination display drawn as the game draws it - in its own font and in others -
    /// and the launcher's preview of each, written as PNGs to `OMSI_FONT_SHOTS` when that is
    /// set. No line runs higher than the bus's own font's.
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
        let windows_font = omsi_content::dotfont::system_font_dirs().into_iter().map(|d| d.join("arialbd.ttf")).find(|f| f.is_file());
        for (bus, text) in [("Vehicles/MAN_SD200/MAN_SD82.bus", "Hauptbahnhof"), ("Vehicles/HH20_EBus2021/HHEBus2021_main.bus", "Rathausmarkt"), ("Vehicles/Citybus 530 by Kajosoft/01a_o530_e2_2.bus", "Dworzec Glowny"), ("Vehicles/BHD_MAN_LionsCity/MAN_A20.bus", "U Feldstraße")] {
            if !root.join(bus).is_file() {
                eprintln!("skipped: no {bus}");
                continue;
            }
            let signs = read_signs(&root, bus).unwrap();
            eprintln!("{bus}: {}", plan_text(&signs.plan));
            let Some(sign) = signs.sign else { continue };
            let stem = Path::new(bus).file_stem().unwrap().to_string_lossy().to_string();
            let g = sign.grid();
            eprintln!("{stem}: {} ({}, {}x{}, line {} px, grid {}/{} px, {} rows), {} displays", sign.def.variable, sign.def.font, sign.def.width, sign.def.height, sign.line_h, g.pitch, g.dot, g.rows, sign.displays);
            let mut specs: Vec<DisplayFontSpec> = ["CRNL_NL2x3_LAWO_16x8", "krueger-font_klein", "Annax Small", "X10_Lawo_3", "4_LW_Benefit_Gross", "17_LED_Gross", "Krueger 16x9"].iter().map(|n| DisplayFontSpec::named(n)).collect();
            if let Some(f) = windows_font.as_ref() {
                specs.push(DisplayFontSpec::vector("Arial Bold", f, 0));
            }
            let mut own = sign.def.clone();
            own.width = own.width.max(256);
            for spec in std::iter::once(None).chain(specs.iter().map(Some)) {
                // (a chosen font is placed as the bus's own; fitted only on a display of none)
                let (atlas, fit) = match spec {
                    None => (sign.own.clone(), None),
                    Some(s) => (lib.display_font(s, sign.own.as_deref(), sign.line_h, &decode), sign.own.is_none().then_some(sign.line_h as u32)),
                };
                let name = spec.map(|s| s.name.clone()).unwrap_or_else(|| "own".into());
                let Some(atlas) = atlas else {
                    eprintln!("  {name}: not installed");
                    continue;
                };
                let mut s = omsi_sim::texttex::TextTextureState::new(own.clone(), Some(atlas.clone()));
                s.fit = fit;
                let img = s.image(text);
                let (w, h) = (own.width as u32, own.height as u32);
                let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| img[((y * w + x) * 4 + 3) as usize] > 0)).collect();
                eprintln!("  {name}: rows {:?}..{:?}, {} px wide", rows.first(), rows.last(), atlas.text_width(text));
                assert!(!rows.is_empty(), "{name} on {bus} draws nothing");
                if spec.is_some() {
                    let top = (h as i32 - sign.line_h) / 2;
                    assert!(rows[0] as i32 >= top && (*rows.last().unwrap() as i32) < top + sign.line_h, "{name} on {bus} runs out of its line");
                }
                let tag = name.replace(' ', "_");
                save(&format!("{stem}_{tag}_texture"), w, h, &img, if w <= 256 { 2 } else { 1 });
                let p = sign_picture(&sign, Some(atlas), fit.is_some(), &format!("12 {text}"));
                save(&format!("{stem}_{tag}_preview"), p.w, p.h, &p.rgba, 1);
            }
        }
    }

}

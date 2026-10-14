//! A text on a bus's own destination display in its own font (`DisplayFonts::preview`), for the
//! page: the picture as a PNG - the bus's own font, or the one chosen for it with its settings
//! - whether the text is wider than the display, the font's name; or that the bus draws its
//! displays as pictures of its own (`none`), or that its files could not be read (`failed`).

use crate::displayfont::{atlas_for, read_signs, sign_picture};
use omsi_launcher_lib::busfonts::BusFonts;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

/// The preview's head and its PNG (empty when there is no picture).
pub fn preview(bus: &str, text: &str) -> (Value, Vec<u8>) {
    let root = omsi_launcher_lib::root_dir().unwrap_or_default();
    let signs = match read_signs(&root, bus) {
        Ok(s) if !s.failed => s,
        _ => return (json!({ "ok": true, "failed": true }), Vec::new()),
    };
    let Some(sign) = signs.sign else { return (json!({ "ok": true, "none": true }), Vec::new()) };
    let chosen = BusFonts::load().spec_for(bus);
    let shown = sign.text_of(text).to_string();
    let pic = match chosen.as_ref() {
        None => sign_picture(&sign, sign.own.clone(), false, &shown),
        Some(spec) => {
            let fonts = if spec.is_vector() { Vec::new() } else { omsi_sim::texttex::FontLibrary::new(&root).installed() };
            let mut lib = omsi_sim::texttex::FontLibrary::with_fonts(Path::new(""), fonts);
            let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| atlas_for(&sign, spec, &mut lib, &shown, &decode))).ok().flatten() {
                Some((a, _)) => sign_picture(&sign, Some(Arc::new(a)), true, &shown),
                None => sign_picture(&sign, sign.own.clone(), false, &shown),
            }
        }
    };
    let too_wide = chosen.is_none() && sign.own.as_ref().is_some_and(|a| a.text_width(text) > sign.def.width);
    let font = chosen.map(|c| c.name).unwrap_or_else(|| sign.def.font.trim().to_string());
    let mut png = Vec::new();
    let made = image::RgbaImage::from_raw(pic.w, pic.h, pic.rgba).map(|img| img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png));
    if !matches!(made, Some(Ok(()))) {
        png.clear();
    }
    (json!({ "ok": true, "too_wide": too_wide, "font": font, "w": pic.w, "h": pic.h }), png)
}

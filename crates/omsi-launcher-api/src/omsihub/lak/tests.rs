//! The page studio's side here: the projects as the game program's studio keeps them, a
//! picture brought in by its hash, the decals drawn in their fonts, the model's rules.

use super::*;

/// A project of the game program's studio (its JSON, Dutch names and o3d places): read, kept
/// and read back the same.
#[test]
fn a_project_of_the_game_program_s_studio_is_read_and_kept_the_same() {
    let text = r##"{
      "versie": 1, "id": "ptest1", "naam": "Lucstad", "bus": "Vehicles\\Test\\test.bus", "start": "snel",
      "lagen": [
        { "id": "r-grond", "naam": "Base colour", "soort": "zone", "centrum": [50, 0, 0], "straal": 1000, "kleur": "#1d3f8f", "uitRecept": true },
        { "id": "l1", "naam": "Text", "soort": "tekst", "tekst": "Lucstad", "lettertype": "Hanken Grotesk", "hoogteCm": 30, "kleur": "#ffffff",
          "plaats": { "zijde": "R", "midden": [1.25, 1.2, 0.5], "breedteM": 1.4, "draai": 0, "spiegel": "gekoppeld" } },
        { "id": "l2", "naam": "Brush", "soort": "penseel", "kleur": "#ff0000",
          "streken": [ { "punten": [[1.25, 0.5, 1.0], [1.25, 0.8, 1.1]], "normalen": [[1, 0, 0], [1, 0, 0]], "straalCm": 10 } ] }
      ],
      "spiegel": { "aan": true }, "gemaakt": "1", "bewaard": "2"
    }"##;
    let p: Project = serde_json::from_str(text).expect("the project reads");
    assert_eq!(p.layers.len(), 3);
    // the place in the o3d frame (x, up, forward) as the bus's (x, forward, up)
    let place = p.layers[1].kind.place().unwrap();
    assert_eq!((place.centre.x, place.centre.y, place.centre.z), (1.25, 0.5, 1.2));
    let back = serde_json::to_value(&p).unwrap();
    let near = |v: &Value, want: [f64; 3]| v.as_array().unwrap().iter().zip(want).all(|(a, b)| (a.as_f64().unwrap() - b).abs() < 1e-5);
    assert!(near(&back["lagen"][1]["plaats"]["midden"], [1.25, 1.2, 0.5]), "{}", back["lagen"][1]["plaats"]["midden"]);
    assert!(near(&back["lagen"][2]["streken"][0]["punten"][0], [1.25, 0.5, 1.0]));
    assert_eq!(serde_json::from_value::<Project>(back).unwrap(), p);
}

#[test]
fn a_project_id_is_never_a_path() {
    assert!(clean_id("p1a2b").is_ok());
    for bad in ["", "..", "../x", "a/b", "a\\b", "C:", "x y"] {
        assert!(clean_id(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_text_is_drawn_into_its_box_with_its_outline() {
    let layer = model::layer(
        "Text",
        Kind::Text { text: "Bus 42".into(), font: shapes::DEFAULT_FONT.into(), height_cm: 30.0, colour: "#fff".into(), outline: Some(model::Outline { colour: "#000".into(), width_cm: 2.0 }), spacing: 0.0, gradient: None, place: model::Place::new(model::Side::R, glam::Vec3::ZERO, 1.0) },
    );
    let bytes = decal(&json!({ "layer": layer, "k": 200.0 })).expect("drawn");
    let f = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let u = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let (wm, hm) = (f(0), f(4));
    assert!(wm > 0.5 && wm < 2.0, "{wm}");
    assert!((hm - 0.3 * model::TEXT_BOX).abs() < 1e-3, "{hm}");
    let (w, h, ch) = (u(8), u(12), u(16));
    assert_eq!(ch, 1);
    assert!((w as f32 - wm * 200.0).abs() <= 2.0 && (h as f32 - hm * 200.0).abs() <= 2.0, "{w}x{h}");
    let fill = &bytes[20..20 + (w * h) as usize];
    assert!(fill.iter().any(|a| *a > 200), "the letters are there");
    let o = 20 + (w * h) as usize;
    let (ow, oh) = (u(o), u(o + 4));
    assert_eq!((ow, oh), (w, h), "the outline as large as the fill");
    let outline = &bytes[o + 12..];
    assert!(outline.iter().filter(|a| **a > 128).count() > fill.iter().filter(|a| **a > 128).count(), "the outline is round the letters");
    // a picture is no text to draw
    let img = model::layer("Logo", Kind::Image { image: "x".into(), white_clear: false, aspect: Some(0.5), place: model::Place::new(model::Side::R, glam::Vec3::ZERO, 1.0) });
    assert!(decal(&json!({ "layer": img, "k": 100.0 })).is_err());
}

#[test]
fn the_decals_sizes_come_from_their_kind() {
    let mut shape = model::layer("Shape", Kind::Shape { shape: "ster".into(), colour: "#fff".into(), outline: None, gradient: None, place: model::Place::new(model::Side::L, glam::Vec3::ZERO, 0.8) });
    let size = |s: &Value, id: &str| -> (f64, f64) { (s[id][0].as_f64().unwrap(), s[id][1].as_f64().unwrap()) };
    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5;
    let s = sizes(&json!({ "layers": [shape.clone()] })).unwrap();
    assert!(close(size(&s, &shape.id), (0.8, 0.8)));
    if let Some(p) = shape.kind.place_mut() {
        p.height_m = Some(0.4);
    }
    let s = sizes(&json!({ "layers": [shape.clone()] })).unwrap();
    assert!(close(size(&s, &shape.id), (0.8, 0.4)));
    let img = model::layer("Logo", Kind::Image { image: "none".into(), white_clear: false, aspect: Some(0.25), place: model::Place::new(model::Side::R, glam::Vec3::ZERO, 2.0) });
    let s = sizes(&json!({ "layers": [img.clone()] })).unwrap();
    assert!(close(size(&s, &img.id), (2.0, 0.5)));
}

#[test]
fn a_name_omsi_cannot_read_is_refused() {
    let e = name_check(&json!({ "name": "", "taken": [] })).unwrap();
    assert!(e["error"].is_string());
    let e = name_check(&json!({ "name": "Lucstad", "taken": ["lucstad"] })).unwrap();
    assert!(e["error"].as_str().unwrap().contains("Lucstad") || e["error"].as_str().unwrap().contains("lucstad"));
    let ok = name_check(&json!({ "name": "Lucstad 2", "taken": ["Lucstad"] })).unwrap();
    assert!(ok["error"].is_null());
}

/// A picture brought in is kept by its hash in the folder given and handed back as RGBA, its
/// white see-through when asked.
#[test]
fn a_picture_is_kept_by_its_hash_and_given_back() {
    let dir = std::env::temp_dir().join(format!("lak-pictures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // (a small PNG: a red square on white)
    let mut img = image::RgbaImage::from_pixel(16, 8, image::Rgba([255, 255, 255, 255]));
    for y in 2..6 {
        for x in 4..12 {
            img.put_pixel(x, y, image::Rgba([220, 20, 20, 255]));
        }
    }
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let (hash, pic) = store_picture(&dir, "logo.png", &png).expect("kept");
    assert_eq!(hash.len(), 40);
    assert!(dir.join(format!("{hash}.png")).is_file());
    assert_eq!((pic.w, pic.h), (16, 8));
    assert!(shapes::wants_white_clear(&pic));
    let back = picture_bytes(&pic, true);
    let u = |o: usize| u32::from_le_bytes(back[o..o + 4].try_into().unwrap());
    assert_eq!((u(0), u(4), u(8)), (16, 8, 4));
    assert_eq!(back[12 + 3], 0, "the white round the logo is see-through");
    let at = 12 + ((4 * 16 + 8) * 4) as usize;
    assert!(back[at + 3] > 200, "the logo is not ({})", back[at + 3]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A real bus prepared for the studio, with the time it takes (the report's numbers). Run it
/// with `HOME` (or `USERPROFILE`) on a sandbox home whose `launcher.json` names the OMSI folder:
/// the 3D view's pack is kept under that home's `.openomsi`.
/// `LAK_BUS=Vehicles/... cargo test -p omsi-launcher-api --release -- --ignored a_real_bus_is_prepared --nocapture`
#[test]
#[ignore]
fn a_real_bus_is_prepared() {
    let bus = std::env::var("LAK_BUS").unwrap_or_else(|_| "Vehicles/MAN_SD200/SD200_Bj1973.bus".into());
    let t0 = std::time::Instant::now();
    let r = prepare::prepare(&json!({ "bus": bus, "paint": "", "start": "snel" }));
    let r = match r {
        Ok(r) => r,
        Err(e) => panic!("{bus}: {e}"),
    };
    let t1 = t0.elapsed();
    let again = prepare::prepare(&json!({ "bus": bus, "paint": "", "start": "snel" })).unwrap();
    let t2 = t0.elapsed() - t1;
    eprintln!(
        "{bus}: prepared in {t1:?} (again {t2:?}), {}; targets {}; dims {}; windows {}; roles {}",
        r["marks"],
        r["targets"].as_array().unwrap().iter().map(|t| format!("{} {}x{} density {:.0} zones {}", t["file"], t["w"], t["h"], t["density"].as_f64().unwrap_or(0.0), t["zones"]["centres"].as_array().map(|z| z.len()).unwrap_or(0))).collect::<Vec<_>>().join(", "),
        r["dims"],
        r["windows"],
        r["roles"].as_array().unwrap().iter().filter(|x| !x.is_null()).count()
    );
    assert_eq!(again["key"], r["key"]);
    assert!(!r["targets"].as_array().unwrap().is_empty());
    let dims = prepare::dims().unwrap();
    assert!(dims.length() > 5.0);
}

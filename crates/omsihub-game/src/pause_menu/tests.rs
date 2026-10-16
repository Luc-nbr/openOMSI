use super::*;
use crate::nav_duty::tests::{duty, raster, state};

/// The game menu's lines of a session on a duty, in openOMSI's order (`GAME_MENU` with the
/// lines `game_menu_items` puts in), and the lines of a LAN host.
fn game_menu(lan: bool) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("resume", "Resume"),
        ("options", "Options..."),
        ("controls", "Controls..."),
        ("camera", "Camera..."),
        ("vehicle", "Vehicle options..."),
        ("world", "World options..."),
        ("map", "City map"),
        ("duty", "Line and tour..."),
        ("skipstop", "Skip the next stop"),
        ("endduty", "End the tour"),
        ("save", "Save the situation"),
        ("saveslot", "Save to a new slot"),
        ("load", "Load the quicksave"),
        ("photo", "Photo mode"),
        ("quit", "End the session"),
    ];
    if lan {
        v.insert(6, ("copycode", "Copy server code"));
        v.insert(v.len() - 1, ("admin", "Administration..."));
    }
    v
}

#[test]
fn the_lines_come_in_the_panels_order_and_none_is_lost() {
    let mut v = game_menu(true);
    v.push(("someday", "A line of the future"));
    v.swap(15, 16);
    let before: Vec<_> = v.iter().map(|l| l.0).collect();
    arrange(&mut v);
    let ids: Vec<_> = v.iter().map(|l| l.0).collect();
    assert_eq!(ids, ["resume", "duty", "skipstop", "map", "endduty", "save", "saveslot", "load", "options", "controls", "camera", "vehicle", "world", "photo", "copycode", "admin", "someday", "quit"]);
    let mut a = before.clone();
    let mut b = ids.clone();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

#[test]
fn the_lines_have_their_groups() {
    assert_eq!(section("map"), Some("This trip"));
    assert_eq!(section("load"), Some("Save"));
    assert_eq!(section("camera"), Some("Settings"));
    assert_eq!(section("admin"), Some("Multiplayer"));
    assert_eq!(section("resume"), None);
    assert_eq!(section("quit"), None);
    // (multiplayer only where a line of it is: a session alone has no such title)
    let mut v = game_menu(false);
    arrange(&mut v);
    let m = menu(&v, None);
    let titles: Vec<_> = body_rows(&m).iter().filter_map(|r| r.3).collect();
    assert_eq!(titles, ["This trip", "Save", "Settings"]);
}

#[test]
fn the_keys_are_the_games_own() {
    let mut v = game_menu(false);
    arrange(&mut v);
    let (chips, hints) = keys(&v, |_| None);
    assert_eq!(chips, vec![("resume", "Esc".to_string()), ("map", "Shift+M".to_string()), ("skipstop", "Ctrl+Shift+H".to_string()), ("save", "Alt+S".to_string()), ("photo", "Ctrl+F12".to_string())]);
    assert_eq!(hints.iter().map(|h| h.1).collect::<Vec<_>>(), ["Resume", "Map", "Save", "Photo"]);
    // the player's own key for OMSI's quicksave; no city map without a navigator
    v.retain(|l| l.0 != "map");
    let (chips, hints) = keys(&v, |a| (a == "quicksave").then(|| "Ctrl+S".to_string()));
    assert!(chips.contains(&("save", "Ctrl+S".to_string())));
    assert!(!chips.iter().any(|c| c.0 == "map") && !hints.iter().any(|h| h.1 == "Map"));
}

#[test]
fn the_card_tells_the_trip_as_the_board_does() {
    crate::ui_language("ENG");
    let (trips, tours) = duty();
    // due at Kirchweg, 4 min late
    let c = card_of(&state(&trips, &tours, 0, 3, false, 250.0), false, 11.0 * 3600.0 + 35.0 * 60.0, "Rain".into(), 92.4).unwrap();
    assert_eq!(c.line.as_deref(), Some("35"));
    assert_eq!((c.from.as_str(), c.to.as_str()), ("Hauptbahnhof", "Diakonissenkrankenhaus"));
    assert_eq!(c.status.as_ref().map(|s| (s.0.as_str(), s.1)), Some(("+4:10", nav_duty::LATE)));
    assert_eq!(c.sub, "Tour 1  \u{b7}  trip 1 of 3");
    let stops: Vec<_> = c.stops.iter().map(|s| (s.name.as_str(), s.time.as_str(), s.next)).collect();
    assert_eq!(stops, [("Kirchweg", "11:32", true), ("Rathaus", "11:35", false), ("Schule", "11:38", false), ("Park", "11:41", false)]);
    assert_eq!(c.stats, vec![("schedule", "Time", "11:35".to_string()), ("partly_cloudy_day", "Weather", "Rain".to_string()), ("speed", "Driving", "92 %".to_string())]);
    // on time, and early
    let c = card_of(&state(&trips, &tours, 0, 3, false, 30.0), false, 0.0, String::new(), 100.0).unwrap();
    assert_eq!(c.status.as_ref().map(|s| (s.0.as_str(), s.1)), Some(("On time", nav_duty::ON_TIME)));
    assert!(!c.stats.iter().any(|s| s.1 == "Weather"), "an unknown weather is left out");
    let c = card_of(&state(&trips, &tours, 0, 3, false, -200.0), false, 0.0, String::new(), 100.0).unwrap();
    assert_eq!(c.status.map(|s| s.1), Some(nav_duty::EARLY));
    // a free drive keeps no time: no chip, no times
    let c = card_of(&state(&trips, &tours, 0, 3, false, 0.0), true, 0.0, String::new(), 100.0).unwrap();
    assert!(c.status.is_none() && c.stops.iter().all(|s| s.time.is_empty()));
    assert!(c.sub.starts_with("Free drive"));
    // the third trip is line 25's tour 3
    let c = card_of(&state(&trips, &tours, 2, 0, false, 0.0), false, 0.0, String::new(), 100.0).unwrap();
    assert!(c.sub.starts_with("Tour 3"), "{}", c.sub);
}

#[test]
fn a_gamepad_presses_keys_and_a_held_direction_repeats() {
    let mut pad = PadKeys::start([false, false, false, false, true, false]);
    let none = [false; 6];
    // (A held as the menu opened is no press)
    assert!(pad.step([false, false, false, false, true, false], 0.016).is_empty());
    let down = [false, true, false, false, false, false];
    assert_eq!(pad.step(down, 0.016), vec![KeyCode::ArrowDown]);
    assert!(pad.step(down, 0.2).is_empty());
    assert_eq!(pad.step(down, 0.25), vec![KeyCode::ArrowDown]);
    assert!(pad.step(down, 0.05).is_empty());
    assert_eq!(pad.step(down, 0.07), vec![KeyCode::ArrowDown]);
    assert!(pad.step(none, 0.016).is_empty());
    assert_eq!(pad.step([false, false, false, false, true, false], 0.016), vec![KeyCode::Enter]);
    assert_eq!(pad.step([false, false, false, false, false, true], 0.016), vec![KeyCode::Escape]);
}

#[test]
fn the_wordmark_draws_in_the_accent() {
    let (w, h, px) = wordmark_pixels((34, omsi_ui::accent::DEFAULT)).unwrap();
    assert_eq!((w, h), (141, 34));
    assert_eq!(px.len(), (w * h * 4) as usize);
    // the ring's orange on the left, white letters, the background open
    let at = |x: u32, y: u32| &px[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
    assert!(px.chunks(4).any(|p| p[3] > 200 && p[0] > 230 && p[1] > 110 && p[1] < 150 && p[2] < 60), "orange");
    assert!(px.chunks(4).any(|p| p[3] > 200 && p[0] > 240 && p[1] > 240 && p[2] > 240), "white");
    assert_eq!(at(w - 1, 0)[3], 0);
    let (_, _, blue) = wordmark_pixels((34, 0x2A75F7)).unwrap();
    assert!(blue.chunks(4).any(|p| p[3] > 200 && p[2] > 200 && p[0] < 80), "a blue accent's ring");
}

#[test]
fn every_text_is_translated() {
    let keys = ["This trip", "Save", "Settings", "Multiplayer", "Resume", "Map", "Photo", "Time", "Weather", "Driving", "Paused", "Menu", "Free drive", "Tour %{tour}", "trip %{k} of %{n}", "on time"];
    for language in ["nl", "de", "fr", "ru", "uk", "pl"] {
        for key in keys {
            let t = crate::_rust_i18n_try_translate(language, key);
            assert!(t.as_ref().is_some_and(|t| !t.trim().is_empty()), "{language}: {key}");
        }
    }
}

fn menu<'a>(lines: &'a [(&'static str, &'static str)], trip: Option<&'a TripCard>) -> Menu<'a> {
    Menu {
        lines,
        sel: Some(0),
        open: None,
        list: false,
        kbd: false,
        keyboard: true,
        paused: true,
        version: "0.2.49-beta67",
        driver: "Luc",
        bus: "Thueringer Wald S 315 UL-GT Facelift",
        chips: &[],
        hints: &[],
        trip,
        stage: Rect::new(0.0, 0.0, 1600.0, 900.0),
        mark: None,
    }
}

/// Pictures of the menu over a stand-in for the street - on a duty, late, the mouse on a line;
/// a LAN host's at 1920 x 1080 with the keyboard on "Options", no duty - written to the
/// folder of `OMSI_UI_PREVIEW` (or `target/`):
/// `cargo test -p omsihub-game --lib pause_menu -- --ignored`.
#[test]
#[ignore]
fn preview_pictures() {
    crate::ui_language("ENG");
    let dir = std::env::var("OMSI_UI_PREVIEW").ok().and_then(|p| std::path::Path::new(&p).parent().map(|d| d.to_path_buf())).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let (trips, tours) = duty();
    let card = card_of(&state(&trips, &tours, 0, 3, false, 250.0), false, 11.0 * 3600.0 + 35.0 * 60.0, "Rain".into(), 92.0).unwrap();
    for (name, lan, (w, h), mouse, trip) in [("pause-menu.png", false, (1600.0, 900.0), Vec2::new(150.0, 420.0), Some(&card)), ("pause-menu-lan.png", true, (1920.0, 1080.0), Vec2::new(-1.0, -1.0), None)] {
        let mut lines = game_menu(lan);
        arrange(&mut lines);
        let (chips, hints) = keys(&lines, |_| None);
        let mut ui = Ui::new();
        ui.flat = true;
        let mut m = menu(&lines, trip);
        m.chips = &chips;
        m.hints = &hints;
        m.stage = Rect::new(0.0, 0.0, w, h);
        m.mark = Some(1);
        if lan {
            m.kbd = true;
            m.sel = lines.iter().position(|l| l.0 == "options");
            m.paused = false;
        }
        // (twice: the eases settle)
        for _ in 0..2 {
            ui.begin(Vec2::new(w, h), 1.0, 1.0);
            ui.input.mouse = mouse;
            draw(&mut ui, &m);
        }
        let (layers, verts, ranges) = ui.finish();
        let mut img = image::RgbaImage::from_fn(w as u32, h as u32, |_, y| {
            let t = y as f32 / h;
            let c: [f32; 3] = if t < 0.42 { [96.0 + 60.0 * t, 112.0 + 50.0 * t, 128.0] } else if t < 0.62 { [178.0, 140.0, 64.0] } else { [74.0, 72.0, 68.0] };
            image::Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255])
        });
        for (k, (range, tex)) in ranges.iter().enumerate() {
            let c = layers[k].clip;
            let clip = Rect::new(c[0], c[1], c[2] - c[0], c[3] - c[1]);
            let vs = &verts[range.start as usize..range.end as usize];
            if *tex == 0 {
                raster(vs, &ui.atlas, &mut img, clip);
            } else if let Some(v) = vs.first() {
                // the wordmark, from its own pixels
                let (mw, mh, px) = wordmark_pixels((MARK_H as u32, omsi_ui::accent::DEFAULT)).unwrap();
                let (x0, y0) = (v.pos[0].round() as u32, v.pos[1].round() as u32);
                for y in 0..mh {
                    for x in 0..mw {
                        let i = ((y * mw + x) * 4) as usize;
                        let a = px[i + 3] as f32 / 255.0;
                        let d = img.get_pixel_mut(x0 + x, y0 + y);
                        for ch in 0..3 {
                            d.0[ch] = (px[i + ch] as f32 * a + d.0[ch] as f32 * (1.0 - a)).round() as u8;
                        }
                    }
                }
            }
        }
        img.save(dir.join(name)).unwrap();
        println!("wrote {}", dir.join(name).display());
    }
}

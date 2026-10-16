//! Omsi-Hub: the game's side of Omsi-Hub's pause menu (`omsihub_game::pause_menu`), drawn in
//! place of openOMSI's rail (`pause::draw`). What the menu shows of the session is gathered
//! here - the keys as the player has them, the duty for the trip card, where on the screens
//! it goes - and what it is clicked for goes back as openOMSI's own [`Action`]s; a gamepad's
//! d-pad and buttons are the menu keys (`App::menu_key`, see `pad_keys`).

use std::cell::RefCell;

use omsihub_game::pause_menu::{self, Menu, PadKeys, TripCard};
use winit::keyboard::KeyCode;

use super::pause::PauseView;
use super::{Action, Shell};
use crate::App;

/// What Omsi-Hub's menu shows of the session this frame, beside openOMSI's `PauseView`.
pub(crate) struct Hub {
    chips: Vec<(&'static str, String)>,
    hints: Vec<(String, &'static str)>,
    trip: Option<TripCard>,
    driver: String,
    stage: omsi_ui::Rect,
}

impl Hub {
    /// The session as the menu shows it, for the game menu's `lines`; `scale` physical pixels
    /// a point; the gamepad read (`dt` s since the last frame).
    pub(crate) fn of(app: &App, lines: &[(&'static str, &'static str)], scale: f32, dt: f32) -> Hub {
        read_pad(app, dt);
        let (chips, hints) = pause_menu::keys(
            lines,
            |action| app.input.game_keys.iter().find(|b| b.action.eq_ignore_ascii_case(action) && b.scan_code != 0).map(|b| crate::keys::key_name(b.scan_code as i64, b.modifier as i64)),
            // (a built-in key stands back for a key of the keyboard file on the same chord,
            // `shortcut_key`: then it does nothing here and shows no chip)
            |id| match id {
                "map" => !app.chord_bound(KeyCode::KeyM, true, false, false),
                "skipstop" => !app.chord_bound(KeyCode::KeyH, true, true, false),
                "save" => !app.chord_bound(KeyCode::KeyS, false, false, true),
                "photo" => !app.chord_bound(KeyCode::F12, false, true, false),
                _ => true,
            },
        );
        let mut trip = app.session.duty.as_ref().and_then(|d| pause_menu::trip_card(d, app.clock.time, crate::game_lists::weather_shown(app), app.session.career.driving_rating()));
        if let (Some(t), Some(d)) = (trip.as_mut(), app.session.duty.as_ref()) {
            t.colour = t.line.as_deref().and_then(|number| line_colour(&app.args.map, &d.line, number));
        }
        // (the name the launcher gave, else the personnel file's)
        let driver = Some(app.args.lan_name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| app.session.career.driver_name());
        // (on three screens the menu keeps to the middle one, as the rest of the interface)
        let stage = app.gfx.surface.as_ref().map(|s| app.settings.hud_viewport((s.config.width, s.config.height))).unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let stage = omsi_ui::Rect::new(stage[0] / scale, stage[1] / scale, stage[2] / scale, stage[3] / scale);
        Hub { chips, hints, trip, driver, stage }
    }
}

/// The menu: Omsi-Hub's panel, and an open list in the sheet beside it as openOMSI draws it.
pub(crate) fn draw(sh: &mut Shell, v: &PauseView, hub: &Hub) {
    let m = Menu {
        lines: v.rail,
        sel: v.rail_sel,
        open: v.rail_open,
        list: v.list.is_some(),
        kbd: v.kbd,
        keyboard: v.keys,
        paused: v.paused,
        version: crate::startup::VERSION,
        driver: &hub.driver,
        bus: &v.summary.bus,
        // (no keys on a touch screen)
        chips: if v.keys { &hub.chips } else { &[] },
        hints: &hub.hints,
        trip: hub.trip.as_ref(),
        stage: hub.stage,
        mark: sh.logo_tex,
    };
    let drawn = pause_menu::draw(&mut sh.ui, &m);
    if let (Some(list), Some(sheet)) = (v.list.as_ref(), drawn.sheet) {
        super::pause::list_page(sh, v, list, sheet);
    }
    if let Some(k) = drawn.picked {
        sh.actions.push(if v.list.is_some() { Action::Rail(k) } else { Action::Choose(k) });
    }
}

/// The colour the player gave the duty's line (`ttl`, shown as `number`) in Omsi-Hub's line
/// editor, from the map's registry - read once a map and line, not every frame.
fn line_colour(map: &str, ttl: &str, number: &str) -> Option<omsi_ui::Color> {
    thread_local! {
        static SEEN: RefCell<Option<((String, String, String), Option<omsi_ui::Color>)>> = const { RefCell::new(None) };
    }
    let key = (map.to_string(), ttl.to_string(), number.to_string());
    SEEN.with(|s| {
        let mut s = s.borrow_mut();
        if let Some((_, c)) = s.as_ref().filter(|(k, _)| *k == key) {
            return *c;
        }
        let path = omsi_launcher_lib::lines::registry_path(&omsi_launcher_lib::lines::map_folder(map));
        let c = path.is_file().then(|| omsi_launcher_lib::lines::load_registry(&path)).and_then(|r| pause_menu::line_colour(&r, ttl, number));
        *s = Some((key, c));
        c
    })
}

/// The gamepad the menu listens to, and the keys it pressed for the next frame's start.
#[derive(Default)]
struct Pad {
    id: Option<gilrs::GamepadId>,
    /// When the pad was last looked for, and when it was last read (a menu opened again
    /// starts afresh: what is held as it opens is no press).
    chosen: Option<std::time::Instant>,
    read: Option<std::time::Instant>,
    keys: Option<PadKeys>,
    pressed: Vec<KeyCode>,
}

thread_local! {
    static PAD: RefCell<Pad> = RefCell::new(Pad::default());
}

/// The gamepad this frame, as the photo mode's finds it (a gamepad, not a wheel, pedals or a
/// joystick, nor anything that steers the bus): its d-pad or left stick, A and B.
fn read_pad(app: &App, dt: f32) {
    use gilrs::{Axis, Button};
    PAD.with(|p| {
        let mut p = p.borrow_mut();
        let now = std::time::Instant::now();
        if p.read.is_none_or(|t| now.duration_since(t).as_secs_f32() > 0.5) {
            p.keys = None;
        }
        p.read = Some(now);
        let Some((ctl, g)) = app.input.controllers.as_ref().and_then(|c| c.devices.gilrs.as_ref().map(|g| (c, g))) else { return };
        if p.chosen.is_none_or(|t| now.duration_since(t).as_secs_f32() > 2.0) {
            p.chosen = Some(now);
            let devices = ctl.connected();
            p.id = g
                .gamepads()
                .find(|(_, pad)| pad.is_connected() && pad.mapping_source() != gilrs::MappingSource::None && !ctl.steers(pad.name()) && devices.iter().any(|c| c.gamepad && crate::controllers::names_match(&c.name, pad.name())))
                .map(|(id, _)| id);
        }
        let Some(pad) = p.id.and_then(|id| g.connected_gamepad(id)) else { return };
        let held = [
            pad.is_pressed(Button::DPadUp) || pad.value(Axis::LeftStickY) > 0.6,
            pad.is_pressed(Button::DPadDown) || pad.value(Axis::LeftStickY) < -0.6,
            pad.is_pressed(Button::DPadLeft) || pad.value(Axis::LeftStickX) < -0.6,
            pad.is_pressed(Button::DPadRight) || pad.value(Axis::LeftStickX) > 0.6,
            pad.is_pressed(Button::South),
            pad.is_pressed(Button::East),
        ];
        let keys = match p.keys.as_mut() {
            Some(k) => k.step(held, dt),
            None => {
                p.keys = Some(PadKeys::start(held));
                Vec::new()
            }
        };
        p.pressed.extend(keys);
    });
}

/// The keys the gamepad pressed in the menu since the last call.
pub(crate) fn pad_keys() -> Vec<KeyCode> {
    PAD.with(|p| std::mem::take(&mut p.borrow_mut().pressed))
}

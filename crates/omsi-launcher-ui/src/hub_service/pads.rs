//! The game controllers of the desktop launcher's Controls page (Trello "Tauri 6/6"), run here as
//! our game-drawn launcher runs them (`pages::controls`, Controls → Game controllers): the game
//! program reads the devices the way the game does (DirectInput beside gilrs,
//! `omsi_input::controllers`), which the page cannot. The devices set up and connected, what each
//! axis does (its direction, its characteristic), each button's action, the step-by-step set-up,
//! a device left out or taken off the list, and Save into the content folder's
//! `Inputs/gamectrler.cfg`.
//!
//! The page asks some ten times a second (`{ "op": "view" }`, or `"act"` with what changed) and
//! gets the view back; the devices are read at each ask, on a thread of their own, and let go a
//! few seconds after the last (the page left). The force feedback's direction test - it turns the
//! wheel - stays in the game-drawn launcher: here the set-up asks for the direction by hand.

use super::Reply;
use crate::pages::controls::{action_text, button_label, moved_most, save_gamectrler, shown_button_count, wizard_result, WIZARD_STEPS};
use omsi_input::controllers::{self as pads, DeviceCfg, Devices, Func, AXIS_SHAPES};
use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// The axes' names, in DirectInput's order.
const AXES: [&str; 8] = ["X axis", "Y axis", "Z axis", "X rotation", "Y rotation", "Z rotation", "Slider 1", "Slider 2"];

/// The page's state between its asks (our game-drawn launcher's `PadsView`).
#[derive(Default)]
struct Pads {
    io: Option<Devices>,
    devices: Option<Vec<DeviceCfg>>,
    selected: usize,
    /// "Add a button": waiting for a button of the device shown.
    capturing: bool,
    /// A button found by pressing it, kept in the list past the ones the device says it has.
    revealed_button: Option<usize>,
    dirty: bool,
    /// The button last pressed on the device shown, and when: its line is lit.
    last_pressed: Option<(usize, Instant)>,
    /// "Remove this device" pressed once, and when: a second press removes it.
    confirm_remove: Option<Instant>,
    wizard: Option<Wizard>,
    /// What the page says once (a toast): the text, and whether it went wrong.
    say: Option<(String, bool)>,
    /// The actions a button can be given, and their names (`controller_action_choices`).
    choices: Option<(Vec<String>, Vec<String>)>,
}

/// The set-up assistant (`pages::Wizard`): everything let go, the wheel to the left, each pedal
/// in turn; then, for a wheel with force feedback, its direction.
struct Wizard {
    step: usize,
    rest: [Option<f32>; 8],
    at: Vec<[Option<f32>; 8]>,
    error: Option<String>,
    ff_choice: Option<bool>,
}

impl Wizard {
    fn new() -> Wizard {
        Wizard { step: 0, rest: [None; 8], at: Vec::new(), error: None, ff_choice: None }
    }
}

/// The page's ask, on the controllers' thread (started with the first).
pub(super) fn ask(req: &Value, reply: Reply) {
    use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
    use std::sync::{Mutex, OnceLock};
    static WORKER: OnceLock<Mutex<Sender<(Value, Reply)>>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<(Value, Reply)>();
        let _ = std::thread::Builder::new().name("game controllers page".into()).spawn(move || {
            let window = window::hidden();
            let mut st = Pads::default();
            let mut last = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok((req, reply)) => {
                        last = Instant::now();
                        let v = serve(&mut st, window, req.get("pads").unwrap_or(&Value::Null));
                        reply(json!({ "ok": true }), v.to_string().into_bytes());
                    }
                    // (the page left: the devices let go, as the game-drawn launcher does when
                    // it hands them to the game; what was not saved stays)
                    Err(RecvTimeoutError::Timeout) => {
                        if st.io.is_some() && last.elapsed() > Duration::from_secs(3) {
                            st.io = None;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                window::pump();
            }
        });
        Mutex::new(tx)
    });
    let _ = worker.lock().unwrap_or_else(|e| e.into_inner()).send((req.clone(), reply));
}

fn tr(t: &str) -> String {
    omsi_ui::tr(t).into_owned()
}

/// One ask: the act in it done, the devices read, the view.
fn serve(st: &mut Pads, window: Option<isize>, ask: &Value) -> Value {
    let settings = core::get_settings().unwrap_or(Value::Null);
    let root = std::path::PathBuf::from(core::load_config().root);
    if ask.get("op").and_then(Value::as_str) == Some("open") {
        // (the page opened: the file read again, as the game-drawn launcher's page does)
        st.devices = None;
        st.wizard = None;
        st.dirty = false;
        st.capturing = false;
        st.revealed_button = None;
        st.choices = None;
    }
    if st.io.is_none() {
        st.io = Some(Devices::new(window, false));
    }
    if st.devices.is_none() {
        st.devices = Some(pads::read_cfg(&root));
        st.selected = st.selected.min(st.devices.as_ref().map(|d| d.len().saturating_sub(1)).unwrap_or(0));
    }
    let lang = settings.get("language").and_then(Value::as_str).unwrap_or("ENG").to_string();
    let names = omsi_input::describe::names(&root, &lang);
    if st.choices.is_none() {
        let bindings = core::get_keybindings().unwrap_or(Value::Null);
        st.choices = Some(crate::pages::keybind_picker::controller_action_choices(names, &bindings));
    }
    // what the devices do now (and the buttons pressed since the last ask)
    let mut pressed: Vec<(String, usize)> = Vec::new();
    let mut connected = Vec::new();
    if let Some(io) = st.io.as_mut() {
        for (name, n, down) in io.poll() {
            if down {
                pressed.push((name, n));
            }
        }
        connected = io.connected();
    }
    if let Some(act) = ask.get("act").and_then(Value::as_str) {
        act_on(st, act, ask, &connected, &settings);
    }
    let buttons_only = |st: &Pads, name: &str| st.io.as_ref().is_some_and(|io| io.buttons_only(name));
    let devices = st.devices.get_or_insert_with(Vec::new);
    // a button pressed on the device shown: its line, lit (and added up to it)
    if let Some(d) = devices.get_mut(st.selected) {
        if let Some((name, n)) = pressed.into_iter().find(|(name, n)| pads::names_match(&d.name, name) && *n < pads::HAT_BUTTONS + 16) {
            while d.buttons.len() <= n {
                d.buttons.push((String::new(), "0".into()));
                st.dirty = true;
            }
            st.capturing = false;
            st.revealed_button = Some(n);
            st.last_pressed = Some((n, Instant::now()));
            let now = d.buttons.get(n).map(|b| b.0.clone()).filter(|a| !a.is_empty());
            let text = match now {
                Some(a) => omsi_ui::tr("%{device}: %{button} - %{action} (lit in the list: choose another there)").replace("%{action}", &omsi_ui::tr(&action_text(names, &a))),
                None => tr("%{device}: %{button} - nothing yet (lit in the list: choose what it does)"),
            };
            st.say = Some((text.replace("%{device}", &name).replace("%{button}", &button_label(n)), false));
        }
    }
    let devices = st.devices.as_ref().cloned().unwrap_or_default();
    let list: Vec<Value> = devices
        .iter()
        .map(|d| json!({ "name": d.name, "on": connected.iter().any(|c| pads::names_match(&d.name, &c.name)) }))
        .collect();
    let add: Vec<String> = connected.iter().filter(|c| !devices.iter().any(|d| pads::names_match(&d.name, &c.name))).map(|c| c.name.clone()).collect();
    let (actions, labels) = st.choices.clone().unwrap_or_default();
    let mut v = json!({
        "devices": list,
        "add": add,
        "add_label": tr("Set up %{device}"),
        "sel": st.selected,
        "dirty": st.dirty,
        "held_gears": omsi_input::hpattern::has_held_bindings(&devices),
        "funcs": Func::LABELS.iter().map(|s| tr(s)).collect::<Vec<_>>(),
        "shapes": AXIS_SHAPES.iter().map(|s| tr(s.0)).collect::<Vec<_>>(),
        "actions": actions.iter().zip(labels.iter()).enumerate().map(|(i, (a, l))| json!([if i == 0 { String::new() } else { a.clone() }, if i == 0 { tr(l) } else { omsi_ui::tr(l).into_owned() }])).collect::<Vec<_>>(),
        "say": st.say.take().map(|(t, bad)| json!([t, bad])),
    });
    let Some(d) = devices.get(st.selected) else { return v };
    let live_dev = pads::find_connected(&connected, &d.name);
    let live: Vec<(usize, f32)> = live_dev.map(|c| c.axes.clone()).unwrap_or_default();
    let only_buttons = buttons_only(st, &d.name);
    let mut d = d.clone();
    // (every button the device has gets its line: DirectInput says how many)
    if let Some(n) = live_dev.map(|c| c.buttons).filter(|n| *n > d.buttons.len()) {
        d.buttons.resize(n, (String::new(), "0".into()));
        if let Some(x) = st.devices.as_mut().and_then(|x| x.get_mut(st.selected)) {
            x.buttons.resize(n, (String::new(), "0".into()));
        }
    }
    let feedback = live_dev.is_some_and(|c| c.ff_capable && !c.gamepad);
    v["live"] = json!(live.iter().map(|(k, x)| json!([k, x])).collect::<Vec<_>>());
    if let Some(w) = st.wizard.as_ref() {
        v["wizard"] = if w.step < WIZARD_STEPS.len() {
            let (title, text) = WIZARD_STEPS[w.step];
            json!({
                "title": omsi_ui::tr("Step %{n} of %{total}: %{title}").replace("%{n}", &(w.step + 1).to_string()).replace("%{total}", &WIZARD_STEPS.len().to_string()).replace("%{title}", &omsi_ui::tr(title)),
                "text": tr(text),
                "absent": live_dev.is_none().then(|| tr("The device is not connected: plug it in (the list on the left marks it green).")),
                "error": w.error.as_deref().map(tr),
                "skip": w.step >= 2,
                "next": tr(if w.step + 1 == WIZARD_STEPS.len() && !feedback { "Finish" } else { "Next" }),
            })
        } else {
            // (the direction of the wheel's force, chosen by hand)
            json!({ "ff": true, "title": tr("Force feedback direction"), "invert": w.ff_choice.or(d.ff_invert).unwrap_or(settings.get("ff_invert").and_then(Value::as_bool).unwrap_or(false)) })
        };
        return v;
    }
    let lit = st.last_pressed.filter(|(_, t)| t.elapsed().as_secs_f32() < 4.0).map(|(b, _)| b);
    let shown = shown_button_count(&d.buttons, live_dev.map(|c| c.buttons).unwrap_or(0), st.revealed_button);
    let ff = (live_dev.is_some_and(|c| c.ff_capable) || d.ff_invert.is_some()).then(|| {
        let (steering, vibration) = d.ff_scale.unwrap_or((1.0, 1.0));
        json!({ "steering": steering, "vibration": vibration, "invert": (!live_dev.is_some_and(|c| c.gamepad)).then(|| d.ff_invert.unwrap_or_else(|| settings.get("ff_invert").and_then(Value::as_bool).unwrap_or(false))) })
    });
    v["device"] = json!({
        "name": d.name,
        "wizard": !only_buttons,
        "ff": ff,
        "axes": if only_buttons { Vec::new() } else {
            (0..8).map(|a| json!({
                "name": tr(AXES[a]),
                "live": live.iter().find(|(k, _)| *k == a).map(|(_, x)| *x),
                "func": Func::code(d.axes[a].map(|x| x.0)) + 1,
                "inv": d.axes[a].is_some_and(|x| x.1),
                "shape": AXIS_SHAPES.iter().position(|s| s.1 == d.axis_flags[a] & (4 | 8 | 0x10)).unwrap_or(0),
            })).collect::<Vec<_>>()
        },
        "buttons": d.buttons.iter().take(shown).enumerate().map(|(b, (act, _))| json!({
            "name": button_label(b),
            "action": actions.iter().skip(1).find(|a| a.eq_ignore_ascii_case(act)).cloned().unwrap_or_default(),
            "latching": d.latching.contains(&b),
            "lit": lit == Some(b),
        })).collect::<Vec<_>>(),
        "capturing": st.capturing,
        "armed": st.confirm_remove.is_some_and(|t| t.elapsed().as_secs() < 4),
    });
    v
}

/// What the page changed (`act`, its values beside it).
fn act_on(st: &mut Pads, act: &str, ask: &Value, connected: &[pads::Connected], settings: &Value) {
    let num = |k: &str| ask.get(k).and_then(Value::as_f64);
    let idx = |k: &str| num(k).map(|x| x.max(0.0) as usize);
    let on = ask.get("on").and_then(Value::as_bool).unwrap_or(false);
    let devices = st.devices.get_or_insert_with(Vec::new);
    match act {
        "select" => {
            if let Some(n) = idx("n").filter(|n| *n < devices.len()) {
                if n != st.selected {
                    st.selected = n;
                    st.capturing = false;
                    st.revealed_button = None;
                    st.wizard = None;
                    st.confirm_remove = None;
                }
            }
        }
        // a device connected but not set up: added, and its set-up begun (not for one of
        // buttons only - a gear shifter, a button box)
        "add" => {
            let Some(name) = ask.get("name").and_then(Value::as_str).map(str::to_string) else { return };
            let axes = !st.io.as_ref().is_some_and(|io| io.buttons_only(&name));
            devices.push(DeviceCfg { name, second: "0".into(), ..Default::default() });
            st.selected = devices.len() - 1;
            st.revealed_button = None;
            st.dirty = true;
            if axes {
                st.wizard = Some(Wizard::new());
            }
        }
        "save" => match save_gamectrler(devices) {
            Ok(p) => {
                st.dirty = false;
                st.say = Some((omsi_ui::tr("Game controllers saved to %{path}").replace("%{path}", &p.display().to_string()), false));
            }
            Err(e) => st.say = Some((omsi_ui::tr("Not saved: %{error}").replace("%{error}", &e), true)),
        },
        "capture" => st.capturing = !st.capturing,
        // (a second press within four seconds removes it; Save keeps it so)
        "remove" => {
            if st.confirm_remove.is_some_and(|t| t.elapsed().as_secs() < 4) && st.selected < devices.len() {
                let name = devices.remove(st.selected).name;
                st.selected = st.selected.min(devices.len().saturating_sub(1));
                st.capturing = false;
                st.revealed_button = None;
                st.last_pressed = None;
                st.confirm_remove = None;
                st.dirty = true;
                st.say = Some((format!("{name} removed: press Save to keep it so."), false));
            } else {
                st.confirm_remove = Some(Instant::now());
            }
        }
        "wizard" => st.wizard = Some(Wizard::new()),
        "wizard_cancel" => st.wizard = None,
        "wizard_invert" => {
            if let Some(w) = st.wizard.as_mut() {
                w.ff_choice = Some(on);
            }
        }
        "wizard_next" | "wizard_skip" => wizard_step(st, act == "wizard_skip", connected, settings),
        _ => {
            let Some(d) = devices.get_mut(st.selected) else { return };
            let a = idx("a").unwrap_or(0).min(7);
            let b = idx("b").unwrap_or(0);
            match act {
                "axis" => {
                    let inv = d.axes[a].map(|x| x.1).unwrap_or(false);
                    d.axes[a] = Func::from_code(num("func").unwrap_or(0.0) as i32 - 1).map(|f| (f, inv));
                }
                "inv" => {
                    if let Some(x) = d.axes[a].as_mut() {
                        x.1 = on;
                    }
                }
                // (the characteristic: the curve's bits of the flags, the range's extension kept)
                "shape" => {
                    if let Some(s) = idx("k").and_then(|k| AXIS_SHAPES.get(k)) {
                        d.axis_flags[a] = (d.axis_flags[a] & !(4 | 8 | 0x10)) | s.1;
                    }
                }
                "button" => {
                    if b < d.buttons.len() {
                        d.buttons[b].0 = ask.get("action").and_then(Value::as_str).unwrap_or("").to_string();
                    }
                }
                "latch" => {
                    d.latching.retain(|x| *x != b);
                    if on {
                        d.latching.push(b);
                        d.latching.sort_unstable();
                    }
                }
                "ff_scale" => {
                    let (s0, v0) = d.ff_scale.unwrap_or((1.0, 1.0));
                    d.ff_scale = Some((num("steering").map(|x| x as f32).unwrap_or(s0), num("vibration").map(|x| x as f32).unwrap_or(v0)));
                }
                "ff_invert" => d.ff_invert = Some(on),
                _ => return,
            }
            st.dirty = true;
        }
    }
}

/// Next (or Skip) in the set-up: where the axes stand taken, as the game-drawn launcher's
/// `wizard` takes it.
fn wizard_step(st: &mut Pads, skip: bool, connected: &[pads::Connected], settings: &Value) {
    let Some(devices) = st.devices.as_mut() else { return };
    let Some(d) = devices.get_mut(st.selected) else { return };
    let Some(w) = st.wizard.as_mut() else { return };
    let live_dev = pads::find_connected(connected, &d.name);
    let feedback = live_dev.is_some_and(|c| c.ff_capable && !c.gamepad);
    // the direction of the force chosen: the set-up done
    if w.step >= WIZARD_STEPS.len() {
        d.axes = wizard_result(&w.rest, &w.at);
        d.ff_invert = Some(w.ff_choice.or(d.ff_invert).unwrap_or(settings.get("ff_invert").and_then(Value::as_bool).unwrap_or(false)));
        st.wizard = None;
        st.dirty = true;
        st.say = Some((tr("Set up: press Save to keep it (the buttons can be given their keys below)."), false));
        return;
    }
    let live: Vec<(usize, f32)> = live_dev.map(|c| c.axes.clone()).unwrap_or_default();
    let mut cur = [None; 8];
    for (k, v) in &live {
        cur[*k] = Some(*v);
    }
    w.error = None;
    if w.step == 0 {
        if live.is_empty() {
            w.error = Some("The device shows no axis yet: move the wheel and the pedals a little, let go, and press Next again.".into());
            return;
        }
        w.rest = cur;
    } else if skip {
        w.at.push([None; 8]);
    } else {
        // the axis that moved most since everything was let go (one taken before is not taken
        // again - but the throttle's axis may turn out to be the brake's too)
        let used: Vec<usize> = w.at.iter().filter_map(|a| moved_most(&w.rest, a, &[]).map(|m| m.0)).collect();
        let exclude: Vec<usize> = if w.step == 3 { used.iter().copied().take(1).collect() } else { used.clone() };
        if moved_most(&w.rest, &cur, &exclude).is_none() {
            w.error = Some("Nothing moved far enough. Hold it all the way, then press Next (or Skip).".into());
            return;
        }
        w.at.push(cur);
    }
    w.step += 1;
    if w.step < WIZARD_STEPS.len() {
        return;
    }
    let axes = wizard_result(&w.rest, &w.at);
    // (a wheel with force feedback: its direction next)
    if feedback && axes.iter().any(|a| matches!(a, Some((Func::Steering, _)))) {
        return;
    }
    d.axes = axes;
    st.wizard = None;
    st.dirty = true;
    st.say = Some((tr("Set up: press Save to keep it (the buttons can be given their keys below)."), false));
}

/// Windows: a window of this thread, never shown, for DirectInput (whose devices belong to a
/// window; read in the background, they do not need it in front); its messages handled at each
/// turn of the thread. Elsewhere none is needed.
mod window {
    #[cfg(windows)]
    pub fn hidden() -> Option<isize> {
        use windows::core::w;
        use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, RegisterClassW, WINDOW_EX_STYLE, WNDCLASSW, WS_POPUP};
        unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        unsafe {
            let hinst: HINSTANCE = GetModuleHandleW(None).ok()?.into();
            let class = w!("Omsi-Hub game controllers page");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hinst, lpszClassName: class, ..Default::default() };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, Some(hinst), None).ok()?;
            Some(hwnd.0 as isize)
        }
    }

    #[cfg(windows)]
    pub fn pump() {
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, MSG, PM_REMOVE};
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
        }
    }

    #[cfg(not(windows))]
    pub fn hidden() -> Option<isize> {
        None
    }

    #[cfg(not(windows))]
    pub fn pump() {}
}

//! Tobii eye trackers natively (the Eye Tracker 5, the 4C, the ones built into laptops): the
//! head's pose read straight from Tobii's own Stream Engine - the `tobii_stream_engine.dll`
//! the Tobii Experience installs (opentrack ships one too), loaded at run time - with no
//! opentrack in between.
//!
//! The tracker gives the head where it is in front of the screen (mm; x to the player's right,
//! y up, z towards the player) and how it is turned (radians about those axes). The pose is
//! taken relative to where the head was when tracking began or F1 centred it, in opentrack's
//! terms (`HeadPose`: cm left, up, back; degrees yaw right, pitch up, roll): one source for the
//! camera whichever device gave it. A head looks at the screen, so it turns little: the turn
//! is made larger (`turn`, 2.5 times by default), a small dead zone keeps the view still at the
//! middle, and the pose is smoothed a little. With `edges` on, a look at the screen's left or
//! right edge turns the view further that way (as Tobii's "extended view" in games).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::headtrack::HeadPose;

/// What the tracker is doing, for the settings and the log.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Status {
    /// Looking for Tobii's runtime and a tracker.
    #[default]
    Starting,
    /// No `tobii_stream_engine.dll` was found (the Tobii Experience is not installed).
    NoRuntime,
    /// The runtime is there, no tracker is connected (or its service does not run).
    NoDevice,
    /// A tracker is connected: its name.
    Connected(String),
    /// It gave up: why.
    Failed(String),
}

/// How the head's pose is made into the camera's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// The head's turn times this (yaw; pitch at four fifths of it).
    pub turn: f32,
    /// A look at the screen's edges turns the view further.
    pub edges: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { turn: 2.5, edges: false }
    }
}

/// A pose as the tracker gives it: position (mm) and rotation (radians), each when valid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Raw {
    pos: Option<[f32; 3]>,
    rot: [Option<f32>; 3],
}

#[derive(Default)]
struct Shared {
    /// The last pose and when it came, smoothed.
    last: Option<(Raw, Instant)>,
    smooth: Option<([f32; 3], [f32; 3])>,
    /// Where the head was at the middle (the first poses after the start or F1), and the
    /// poses gathered for it.
    centre: Option<([f32; 3], [f32; 3])>,
    gathering: Vec<([f32; 3], [f32; 3])>,
    /// The gaze on the screen (0..1 across and down) and when, and the extra turn it gives.
    gaze: Option<([f32; 2], Instant)>,
    edge_turn: f32,
    status: Status,
    /// The last pose taken (a jump from it is a glitch) and how many in a row were left out.
    prev: Option<[f32; 3]>,
    glitches: u32,
}

/// The tracker, read on a thread of its own while this lives.
pub struct Tobii {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    options: Arc<Mutex<Options>>,
}

/// How many poses make the middle (Tobii gives some 30 a second).
const CENTRE_POSES: usize = 12;
/// The dead zone round the middle (degrees of the head's own turn).
const DEAD: f32 = 1.2;
/// How quickly the pose follows (s) - a little smoother than the tracker's own.
const SMOOTH: f32 = 0.07;
/// A turn this large between two poses (a thirtieth of a second apart: 750 degrees a second,
/// more than a head turns) is the tracker losing the face, not the head (radians).
const JUMP: f32 = 0.44;
/// The head's own turn used at most (degrees): a head that looks at the screen turns little
/// (Luc's, looking at its edges: some 20 degrees), farther the tracker loses the face.
const MAX_YAW: f32 = 45.0;
const MAX_PITCH: f32 = 35.0;

impl Tobii {
    /// Start reading: the runtime looked for, a tracker connected (tried again now and then
    /// while there is none). Always gives a reader; `status` says what came of it.
    pub fn start(options: Options) -> Tobii {
        let shared: Arc<Mutex<Shared>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let opts = Arc::new(Mutex::new(options));
        #[cfg(windows)]
        {
            let (s, st, o) = (shared.clone(), stop.clone(), opts.clone());
            let _ = std::thread::Builder::new().name("tobii".into()).spawn(move || engine::run(s, st, o));
        }
        #[cfg(not(windows))]
        {
            shared.lock().unwrap_or_else(|e| e.into_inner()).status = Status::NoRuntime;
        }
        Tobii { shared, stop, options: opts }
    }

    pub fn status(&self) -> Status {
        self.shared.lock().map(|s| s.status.clone()).unwrap_or_default()
    }

    pub fn set_options(&self, o: Options) {
        if let Ok(mut x) = self.options.lock() {
            *x = o;
        }
    }

    /// The head's pose for the camera, while poses keep coming (none for half a second: the
    /// head is centred), relative to the middle.
    pub fn pose(&self) -> Option<HeadPose> {
        let s = self.shared.lock().ok()?;
        let (_, at) = s.last?;
        if at.elapsed() > Duration::from_millis(500) {
            return None;
        }
        let (pos, rot) = s.smooth?;
        let (cp, cr) = s.centre?;
        let o = *self.options.lock().ok()?;
        Some(head_pose(pos, rot, cp, cr, o.turn, if o.edges { s.edge_turn } else { 0.0 }))
    }

    /// Take where the head is now as the middle (F1, as the view is put straight).
    pub fn recenter(&self) {
        if let Ok(mut s) = self.shared.lock() {
            s.centre = None;
            s.gathering.clear();
        }
    }
}

impl Drop for Tobii {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The dead zone: nothing within `dead` of the middle, then on from there without a jump.
fn dead_zone(v: f32, dead: f32) -> f32 {
    if v.abs() <= dead {
        0.0
    } else {
        v - dead * v.signum()
    }
}

/// The camera's pose from the tracker's (`pos` mm, `rot` radians, both as the tracker has
/// them) relative to the middle (`cp`, `cr`): in opentrack's terms, the turn made `turn` times
/// larger, `edge` degrees more yaw from a look at the screen's edge.
fn head_pose(pos: [f32; 3], rot: [f32; 3], cp: [f32; 3], cr: [f32; 3], turn: f32, edge: f32) -> HeadPose {
    let d = [pos[0] - cp[0], pos[1] - cp[1], pos[2] - cp[2]];
    let r = [(rot[0] - cr[0]).to_degrees(), (rot[1] - cr[1]).to_degrees(), (rot[2] - cr[2]).to_degrees()];
    // (x: the tracker's is to the player's right, opentrack's to the left; z: both away from
    // the screen. A turn about y to the left is positive for the tracker, opentrack's yaw is
    // to the right; about x up is positive for both.)
    let yaw = -dead_zone(r[1].clamp(-MAX_YAW, MAX_YAW), DEAD) * turn + edge;
    let pitch = dead_zone(r[0].clamp(-MAX_PITCH, MAX_PITCH), DEAD) * turn * 0.8;
    HeadPose { pos: [-d[0] / 10.0, d[1] / 10.0, d[2] / 10.0], rot: [yaw, pitch, r[2]] }
}

/// The extra yaw a look at the screen's edge asks (degrees): nothing in the middle three fifths,
/// up to `max` at the very edge.
fn edge_yaw(gaze_x: f32, max: f32) -> f32 {
    let x = (gaze_x - 0.5) * 2.0;
    let beyond = ((x.abs() - 0.6) / 0.4).clamp(0.0, 1.0);
    x.signum() * beyond * beyond * max
}

impl Shared {
    /// A pose from the tracker: kept, smoothed, and while the middle is being taken gathered
    /// for it.
    fn take(&mut self, raw: Raw, now: Instant) {
        let dt = self.last.map(|(_, t)| now.duration_since(t).as_secs_f32()).unwrap_or(0.0).min(0.5);
        self.last = Some((raw, now));
        let (Some(pos), [Some(rx), Some(ry), Some(rz)]) = (raw.pos, raw.rot) else { return };
        let rot = [rx, ry, rz];
        // (a jump no head makes is the tracker losing the face: left out - three in a row, and
        // it is where the head is now)
        if let Some(prev) = self.prev {
            if (0..3).any(|i| (rot[i] - prev[i]).abs() > JUMP) && self.glitches < 3 {
                self.glitches += 1;
                return;
            }
        }
        self.prev = Some(rot);
        self.glitches = 0;
        let k = if dt > 0.0 { 1.0 - (-dt / SMOOTH).exp() } else { 1.0 };
        let (p, r) = match self.smooth {
            Some((p, r)) => ([0, 1, 2].map(|i| p[i] + (pos[i] - p[i]) * k), [0, 1, 2].map(|i| r[i] + (rot[i] - r[i]) * k)),
            None => (pos, rot),
        };
        self.smooth = Some((p, r));
        if self.centre.is_none() {
            self.gathering.push((pos, rot));
            if self.gathering.len() >= CENTRE_POSES {
                let n = self.gathering.len() as f32;
                let mut cp = [0.0; 3];
                let mut cr = [0.0; 3];
                for (gp, gr) in &self.gathering {
                    for i in 0..3 {
                        cp[i] += gp[i] / n;
                        cr[i] += gr[i] / n;
                    }
                }
                self.centre = Some((cp, cr));
                self.gathering.clear();
                log::info!("tobii: the head's middle taken at ({:.0}, {:.0}, {:.0}) mm", cp[0], cp[1], cp[2]);
            }
        }
    }

    /// A gaze point: the edge turn follows it slowly (a glance does not swing the view).
    fn gazed(&mut self, at: Option<[f32; 2]>, now: Instant) {
        let dt = self.gaze.map(|(_, t)| now.duration_since(t).as_secs_f32()).unwrap_or(0.0).min(0.5);
        let want = at.map_or(0.0, |g| edge_yaw(g[0], 40.0));
        let k = 1.0 - (-dt / 0.35).exp();
        self.edge_turn += (want - self.edge_turn) * k;
        if let Some(g) = at {
            self.gaze = Some((g, now));
        }
    }
}

#[cfg(windows)]
mod engine {
    //! Tobii's Stream Engine C API, loaded from its DLL (the calls as `tobii.h` has them, 4.x and
    //! the 2.x opentrack ships).

    use super::{Options, Raw, Shared, Status};
    use std::ffi::{c_char, c_void, CStr};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows::core::{HSTRING, PCSTR};
        use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

    type Api = c_void;
    type Device = c_void;
    type Error = i32;
    const NO_ERROR: Error = 0;
    const CONNECTION_FAILED: Error = 5;
    /// `TOBII_VALIDITY_VALID`.
    const VALID: i32 = 1;
    /// `TOBII_FIELD_OF_USE_INTERACTIVE` (4.x): the pose drives the game, nothing is kept.
    const INTERACTIVE: i32 = 1;

    #[repr(C)]
    #[derive(Default, Debug)]
    struct Version {
        major: i32,
        minor: i32,
        revision: i32,
        build: i32,
    }

    #[repr(C)]
    struct HeadPoseT {
        timestamp_us: i64,
        position_validity: i32,
        position_xyz: [f32; 3],
        rotation_validity_xyz: [i32; 3],
        rotation_xyz: [f32; 3],
    }

    #[repr(C)]
    struct GazePointT {
        timestamp_us: i64,
        validity: i32,
        position_xy: [f32; 2],
    }

    type UrlReceiver = extern "C" fn(*const c_char, *mut c_void);
    type HeadPoseCallback = extern "C" fn(*const HeadPoseT, *mut c_void);
    type GazePointCallback = extern "C" fn(*const GazePointT, *mut c_void);

    struct Lib {
        get_api_version: extern "C" fn(*mut Version) -> Error,
        api_create: extern "C" fn(*mut *mut Api, *const c_void, *const c_void) -> Error,
        api_destroy: extern "C" fn(*mut Api) -> Error,
        enumerate_local_device_urls: extern "C" fn(*mut Api, UrlReceiver, *mut c_void) -> Error,
        device_create4: extern "C" fn(*mut Api, *const c_char, i32, *mut *mut Device) -> Error,
        device_create2: extern "C" fn(*mut Api, *const c_char, *mut *mut Device) -> Error,
        device_destroy: extern "C" fn(*mut Device) -> Error,
        device_reconnect: extern "C" fn(*mut Device) -> Error,
        device_process_callbacks: extern "C" fn(*mut Device) -> Error,
        head_pose_subscribe: extern "C" fn(*mut Device, HeadPoseCallback, *mut c_void) -> Error,
        gaze_point_subscribe: extern "C" fn(*mut Device, GazePointCallback, *mut c_void) -> Error,
        version: Version,
    }

    /// Where the DLL may be: OMSI_TOBII_DLL, beside the game, the Tobii Experience's, opentrack's.
    fn candidates() -> Vec<PathBuf> {
        let mut v = Vec::new();
        if let Ok(p) = omsi_cfg::env::var("OMSI_TOBII_DLL") {
            v.push(PathBuf::from(p));
        }
        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            v.push(d.join("tobii_stream_engine.dll"));
        }
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
                for sub in ["Tobii\\Tobii EyeX", "Tobii\\Tobii Experience", "Tobii\\Service", "opentrack\\modules"] {
                    v.push(base.join(sub).join("tobii_stream_engine.dll"));
                }
            }
        }
        v.dedup();
        v
    }

    fn load() -> Option<(Lib, PathBuf)> {
        for path in candidates().into_iter().filter(|p| p.is_file()) {
            // SAFETY: a DLL loaded by its full path (its own folder first for what it needs);
            // the functions are looked up by name and called as `tobii.h` declares them.
            unsafe {
                let Ok(module) = LoadLibraryExW(&HSTRING::from(path.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH) else { continue };
                let get = |name: &[u8]| GetProcAddress(module, PCSTR(name.as_ptr()));
                macro_rules! f {
                    ($name:literal) => {
                        match get(concat!($name, "\0").as_bytes()) {
                            Some(p) => std::mem::transmute::<unsafe extern "system" fn() -> isize, _>(p),
                            None => {
                                log::warn!("tobii: {} has no {}", path.display(), $name);
                                continue;
                            }
                        }
                    };
                }
                let mut lib = Lib {
                    get_api_version: f!("tobii_get_api_version"),
                    api_create: f!("tobii_api_create"),
                    api_destroy: f!("tobii_api_destroy"),
                    enumerate_local_device_urls: f!("tobii_enumerate_local_device_urls"),
                    device_create4: f!("tobii_device_create"),
                    device_create2: f!("tobii_device_create"),
                    device_destroy: f!("tobii_device_destroy"),
                    device_reconnect: f!("tobii_device_reconnect"),
                    device_process_callbacks: f!("tobii_device_process_callbacks"),
                    head_pose_subscribe: f!("tobii_head_pose_subscribe"),
                    gaze_point_subscribe: f!("tobii_gaze_point_subscribe"),
                    version: Version::default(),
                };
                (lib.get_api_version)(&mut lib.version);
                log::info!("tobii: Stream Engine {}.{}.{} from {}", lib.version.major, lib.version.minor, lib.version.revision, path.display());
                return Some((lib, path));
            }
        }
        None
    }

    extern "C" fn url_found(url: *const c_char, data: *mut c_void) {
        if url.is_null() || data.is_null() {
            return;
        }
        // SAFETY: `data` is the Vec handed to the enumeration below, alive through it.
        let urls = unsafe { &mut *(data as *mut Vec<String>) };
        urls.push(unsafe { CStr::from_ptr(url) }.to_string_lossy().into_owned());
    }

    extern "C" fn head_pose(pose: *const HeadPoseT, data: *mut c_void) {
        if pose.is_null() || data.is_null() {
            return;
        }
        // SAFETY: `data` is the shared state's Mutex, kept alive by the reading thread for as
        // long as the device it subscribed on; `pose` is valid during the call.
        let (p, shared) = unsafe { (&*pose, &*(data as *const Mutex<Shared>)) };
        let raw = Raw {
            pos: (p.position_validity == VALID).then_some(p.position_xyz),
            rot: [0, 1, 2].map(|i| (p.rotation_validity_xyz[i] == VALID).then_some(p.rotation_xyz[i])),
        };
        if let Ok(mut s) = shared.lock() {
            s.take(raw, Instant::now());
        }
    }

    extern "C" fn gaze_point(g: *const GazePointT, data: *mut c_void) {
        if g.is_null() || data.is_null() {
            return;
        }
        // SAFETY: as `head_pose`.
        let (g, shared) = unsafe { (&*g, &*(data as *const Mutex<Shared>)) };
        if let Ok(mut s) = shared.lock() {
            s.gazed((g.validity == VALID).then_some(g.position_xy), Instant::now());
        }
    }

    fn set(shared: &Mutex<Shared>, st: Status) {
        if let Ok(mut s) = shared.lock() {
            if s.status != st {
                log::info!("tobii: {st:?}");
                s.status = st;
            }
        }
    }

    pub(super) fn run(shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>, options: Arc<Mutex<Options>>) {
        let Some((lib, _)) = load() else {
            set(&shared, Status::NoRuntime);
            return;
        };
        let mut api: *mut Api = std::ptr::null_mut();
        if (lib.api_create)(&mut api, std::ptr::null(), std::ptr::null()) != NO_ERROR || api.is_null() {
            set(&shared, Status::Failed("the Stream Engine did not start".into()));
            return;
        }
        let data = Arc::as_ptr(&shared) as *mut c_void;
        'devices: while !stop.load(Ordering::Relaxed) {
            // a tracker: the first one connected (looked for again every few seconds)
            let mut urls: Vec<String> = Vec::new();
            (lib.enumerate_local_device_urls)(api, url_found, &mut urls as *mut Vec<String> as *mut c_void);
            let Some(url) = urls.first().cloned() else {
                set(&shared, Status::NoDevice);
                for _ in 0..30 {
                    if stop.load(Ordering::Relaxed) {
                        break 'devices;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                continue;
            };
            let c_url = std::ffi::CString::new(url.clone()).unwrap_or_default();
            let mut dev: *mut Device = std::ptr::null_mut();
            let made = if lib.version.major >= 4 { (lib.device_create4)(api, c_url.as_ptr(), INTERACTIVE, &mut dev) } else { (lib.device_create2)(api, c_url.as_ptr(), &mut dev) };
            if made != NO_ERROR || dev.is_null() {
                set(&shared, Status::Failed(format!("the tracker {url} could not be opened (error {made})")));
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
            let e = (lib.head_pose_subscribe)(dev, head_pose, data);
            if e != NO_ERROR {
                set(&shared, Status::Failed(format!("the tracker gives no head pose (error {e})")));
                (lib.device_destroy)(dev);
                break;
            }
            let mut gaze_on = false;
            set(&shared, Status::Connected(url.trim_start_matches("tobii-ttp://").to_string()));
            let mut failures = 0;
            while !stop.load(Ordering::Relaxed) {
                // (the edges' gaze only while it is asked for)
                let edges = options.lock().map(|o| o.edges).unwrap_or(false);
                if edges && !gaze_on {
                    gaze_on = (lib.gaze_point_subscribe)(dev, gaze_point, data) == NO_ERROR;
                }
                match (lib.device_process_callbacks)(dev) {
                    NO_ERROR => failures = 0,
                    CONNECTION_FAILED => {
                        failures += 1;
                        if (lib.device_reconnect)(dev) != NO_ERROR {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    }
                    _ => failures += 1,
                }
                if failures > 200 {
                    // (the tracker was unplugged or its service stopped: looked for anew)
                    set(&shared, Status::NoDevice);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            (lib.device_destroy)(dev);
        }
        (lib.api_destroy)(api);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every raw pose for `OMSI_TOBII_SECS` seconds (8 by default) into `OMSI_TOBII_RAW`: the
    /// time, the position (mm) and the rotation (degrees), each with its validity. By hand:
    /// `OMSI_TOBII_RAW=<file> cargo test -p omsi-app --lib tobii::tests::record_the_tracker_here -- --ignored`
    #[test]
    #[ignore]
    fn record_the_tracker_here() {
        let Ok(file) = omsi_cfg::env::var("OMSI_TOBII_RAW") else { return };
        let secs: u64 = omsi_cfg::env::var("OMSI_TOBII_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
        let t = Tobii::start(Options::default());
        let start = Instant::now();
        let mut out = String::new();
        let mut last = None;
        while start.elapsed() < Duration::from_secs(secs) {
            if let Ok(s) = t.shared.lock() {
                if let Some((raw, at)) = s.last {
                    if last != Some(at) {
                        last = Some(at);
                        let p = raw.pos.map(|p| format!("{:.0} {:.0} {:.0}", p[0], p[1], p[2])).unwrap_or_else(|| "- - -".into());
                        let r = raw.rot.map(|r| r.map(|v| format!("{:.1}", v.to_degrees())).unwrap_or_else(|| "-".into())).join(" ");
                        out.push_str(&format!("{:.3} {p} {r}
", at.duration_since(start).as_secs_f32()));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        std::fs::write(&file, out).expect("written");
    }

    /// The tracker on this computer read for a few seconds (only read): what the runtime and
    /// the tracker say and the poses that come. Run by hand, in front of the tracker:
    /// `cargo test -p omsi-app --lib tobii::tests::read_the_tracker_here -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn read_the_tracker_here() {
        let t = Tobii::start(Options::default());
        let until = Instant::now() + Duration::from_secs(9);
        let mut said = None;
        let mut n = 0;
        while Instant::now() < until {
            let st = t.status();
            if said.as_ref() != Some(&st) {
                println!("status: {st:?}");
                said = Some(st);
            }
            if let Ok(s) = t.shared.lock() {
                if let (Some((raw, _)), true) = (s.last, n % 25 == 0) {
                    println!("raw: pos {:?} rot {:?} centre {:?} -> {:?}", raw.pos, raw.rot.map(|r| r.map(f32::to_degrees)), s.centre.map(|c| c.0), None::<()>);
                }
            }
            if n % 25 == 0 {
                println!("pose for the camera: {:?}", t.pose());
            }
            n += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The head turned 10 degrees to the left (the tracker's +y), 4 up and 3 cm to its right:
    /// the camera turns left, made larger, the eye moves to the right (opentrack's -x).
    #[test]
    fn the_trackers_pose_in_the_cameras_terms() {
        let c = ([0.0, 0.0, 600.0], [0.0, 0.0, 0.0]);
        let p = head_pose([30.0, 0.0, 600.0], [4f32.to_radians(), 10f32.to_radians(), 0.0], c.0, c.1, 2.5, 0.0);
        assert!((p.rot[0] - -(10.0 - DEAD) * 2.5).abs() < 1e-3, "{p:?}");
        assert!((p.rot[1] - (4.0 - DEAD) * 2.5 * 0.8).abs() < 1e-3, "{p:?}");
        assert!((p.pos[0] + 3.0).abs() < 1e-4 && p.pos[1].abs() < 1e-4, "{p:?}");
        // (a head turned past what looks at the screen: held there)
        let far = head_pose([0.0, 0.0, 600.0], [0.0, 80f32.to_radians(), 0.0], c.0, c.1, 2.0, 0.0);
        assert!((far.rot[0] + (MAX_YAW - DEAD) * 2.0).abs() < 1e-3, "{far:?}");
        // still at the middle: nothing
        let still = head_pose([0.0, 0.0, 600.0], [0.5f32.to_radians(), -0.8f32.to_radians(), 0.0], c.0, c.1, 2.5, 0.0);
        assert_eq!((still.rot[0], still.rot[1]), (0.0, 0.0));
    }

    #[test]
    fn a_look_at_the_edge_turns_further() {
        assert_eq!(edge_yaw(0.5, 40.0), 0.0);
        assert_eq!(edge_yaw(0.75, 40.0), 0.0);
        assert!((edge_yaw(1.0, 40.0) - 40.0).abs() < 1e-4);
        assert!((edge_yaw(0.0, 40.0) + 40.0).abs() < 1e-4);
    }

    /// The middle is the mean of the first poses; F1 takes it anew.
    #[test]
    fn the_middle_is_where_the_head_was_at_first() {
        let mut s = Shared::default();
        let now = Instant::now();
        for k in 0..CENTRE_POSES {
            s.take(Raw { pos: Some([k as f32, 0.0, 600.0]), rot: [Some(0.0); 3] }, now + Duration::from_millis(33 * k as u64));
        }
        let (cp, _) = s.centre.expect("a middle");
        assert!((cp[0] - (CENTRE_POSES - 1) as f32 / 2.0).abs() < 1e-4);
        // (a turn of 40 degrees in a thirtieth of a second: a glitch, left out)
        let before = s.smooth;
        s.take(Raw { pos: Some([0.0, 0.0, 600.0]), rot: [Some(0.0), Some(40f32.to_radians()), Some(0.0)] }, now + Duration::from_millis(500));
        assert_eq!(s.smooth, before);
        // (a pose with nothing valid is kept as the last, not used)
        s.take(Raw::default(), now + Duration::from_secs(1));
        assert!(s.smooth.is_some());
    }
}

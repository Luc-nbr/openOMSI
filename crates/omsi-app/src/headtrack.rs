//! Head tracking: the head's pose from opentrack's "UDP over network" output (six little-
//! endian doubles - x, y, z in cm, yaw, pitch, roll in degrees - to UDP port 4242). opentrack
//! takes TrackIR, Tobii, webcams (neuralnet tracker) and phones, on every platform.
//!
//! On Windows the pose is also read from opentrack's default output there, "freetrack 2.0
//! Enhanced": the `FT_SharedMem` mapping that TrackIR (NPClient) games read, as Omsi.exe
//! does through its TrackIR support. A UDP pose wins over it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The last pose received, in opentrack's terms: x, y, z (cm; left, up, back) and yaw, pitch,
/// roll (degrees; yaw to the right, pitch up). (opentrack's own outputs to simulators that
/// count x to the right, FlightGear's and SimConnect's, send them `-x`.)
#[derive(Debug, Clone, Copy, Default)]
pub struct HeadPose {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

impl HeadPose {
    /// How far the head moved the eye, in the bus's frame (m; right, forward, up). Omsi.exe
    /// (0x829860) moves its camera across by `-x` of the TrackIR pose, which opentrack fills
    /// with its own x: a head moved to the right took the camera to the left (#1157).
    pub fn seat_offset(&self) -> glam::Vec3 {
        glam::Vec3::new(-self.pos[0], -self.pos[2], self.pos[1]).clamp(glam::Vec3::splat(-60.0), glam::Vec3::splat(60.0)) / 100.0
    }

    /// The pose seen from `centre` (a middle taken anew, Shift+F1): the head's way from there,
    /// its turn the short way round.
    pub fn from_centre(&self, centre: &HeadPose) -> HeadPose {
        let turn = |a: f32, b: f32| (a - b + 180.0).rem_euclid(360.0) - 180.0;
        HeadPose { pos: [0, 1, 2].map(|i| self.pos[i] - centre.pos[i]), rot: [0, 1, 2].map(|i| turn(self.rot[i], centre.rot[i])) }
    }

    /// The pose with the player's sensitivities: the turn left and right times `yaw`, up and
    /// down times `pitch`, the tilt times `roll` (0: the horizon stays level), the head's way
    /// times `moved` (0: the eye stays where the seat has it).
    pub fn scaled(&self, yaw: f32, pitch: f32, roll: f32, moved: f32) -> HeadPose {
        HeadPose { pos: self.pos.map(|x| x * moved), rot: [self.rot[0] * yaw, self.rot[1] * pitch, self.rot[2] * roll] }
    }
}

/// opentrack's last pose: when it came, its number and the time since the one before (s).
#[derive(Clone, Copy)]
struct Udp {
    pose: HeadPose,
    at: Instant,
    seq: u64,
    interval: f32,
}

/// A pose from opentrack into `slot`, numbered.
fn put(slot: &Mutex<Option<Udp>>, pose: HeadPose) {
    let now = Instant::now();
    let mut l = slot.lock().unwrap_or_else(|e| e.into_inner());
    let (seq, interval) = match *l {
        Some(u) => (u.seq + 1, now.saturating_duration_since(u.at).as_secs_f32()),
        None => (0, 0.004),
    };
    *l = Some(Udp { pose, at: now, seq, interval });
}

pub struct HeadTracker {
    last: Arc<Mutex<Option<Udp>>>,
    /// opentrack's pose taken as the middle (Shift+F1): its poses are given from there.
    centre: Mutex<HeadPose>,
    /// A Tobii eye tracker read natively (`crate::tobii`), when the source asks for one.
    tobii: Option<crate::tobii::Tobii>,
}

impl HeadTracker {
    /// Listen on `port` (on every interface: opentrack may run on another machine).
    pub fn start(port: u16) -> Option<HeadTracker> {
        // (the port may be taken: opentrack's own "UDP over network" input listens on 4242
        // too, which is how FreePIE hands it a TrackIR's pose. On Windows the freetrack
        // mapping is still read then; elsewhere there is nothing to read.)
        let sock = match std::net::UdpSocket::bind(("0.0.0.0", port)) {
            Ok(s) => Some(s),
            Err(e) if cfg!(windows) => {
                log::warn!("head tracking: cannot listen on UDP port {port} ({e}), reading opentrack's freetrack output only");
                None
            }
            Err(e) => {
                log::warn!("head tracking: cannot listen on UDP port {port}: {e}");
                return None;
            }
        };
        // (on Windows the loop also polls the freetrack mapping between datagrams)
        let wait = if cfg!(windows) { 10 } else { 500 };
        if let Some(sock) = &sock {
            let _ = sock.set_read_timeout(Some(Duration::from_millis(wait)));
        }
        let listening = sock.is_some();
        let last: Arc<Mutex<Option<Udp>>> = Arc::default();
        let out = last.clone();
        std::thread::Builder::new()
            .name("head tracking".into())
            .spawn(move || {
                let mut buf = [0u8; 64];
                let mut announced = false;
                #[cfg(windows)]
                let mut freetrack = freetrack::Reader::default();
                #[cfg(windows)]
                let mut last_udp: Option<Instant> = None;
                loop {
                    // (the game's end takes the thread with it)
                    if Arc::strong_count(&out) == 1 {
                        return;
                    }
                    let got = match &sock {
                        Some(sock) => sock.recv(&mut buf),
                        None => {
                            std::thread::sleep(Duration::from_millis(wait));
                            Err(std::io::ErrorKind::WouldBlock.into())
                        }
                    };
                    let Ok(n) = got else {
                        #[cfg(windows)]
                        if last_udp.is_none_or(|t| t.elapsed() >= Duration::from_millis(500)) {
                            if let Some(pose) = freetrack.poll() {
                                put(&out, pose);
                            }
                        }
                        continue;
                    };
                    if n < 48 {
                        continue;
                    }
                    let d = |i: usize| f64::from_le_bytes(buf[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
                    let v = [d(0), d(1), d(2), d(3), d(4), d(5)];
                    if v.iter().any(|x| !x.is_finite()) {
                        continue;
                    }
                    if !announced {
                        announced = true;
                        log::info!("head tracking: receiving poses on UDP port {port}");
                    }
                    #[cfg(windows)]
                    {
                        last_udp = Some(Instant::now());
                    }
                    put(&out, HeadPose { pos: [v[0], v[1], v[2]], rot: [v[3], v[4], v[5]] });
                }
            })
            .ok()?;
        if listening {
            log::info!("head tracking: listening for opentrack on UDP port {port}");
        }
        Some(HeadTracker { last, centre: Mutex::default(), tobii: None })
    }

    /// The source the setting `head_tracking_source` names: "opentrack" (its UDP output and
    /// freetrack, as `start`), "tobii" (a Tobii eye tracker natively, `crate::tobii`) or
    /// "auto" (both: opentrack's pose while it comes - its own curves - else the tracker's).
    pub fn start_with(port: u16, source: &str, tobii: crate::tobii::Options) -> Option<HeadTracker> {
        let reader = (source != "opentrack").then(|| crate::tobii::Tobii::start(tobii));
        let udp = if source == "tobii" { None } else { HeadTracker::start(port) };
        match (udp, reader) {
            (Some(mut t), r) => {
                t.tobii = r;
                Some(t)
            }
            (None, Some(r)) => Some(HeadTracker { last: Arc::default(), centre: Mutex::default(), tobii: Some(r) }),
            (None, None) => None,
        }
    }

    /// The middle taken anew where the head is now (Shift+F1, and C as the view is put
    /// straight): opentrack's pose as it comes now, and the Tobii tracker's.
    pub fn recenter(&self) {
        if let (Some(u), Ok(mut c)) = (self.udp(), self.centre.lock()) {
            *c = u.pose;
        }
        if let Some(t) = &self.tobii {
            t.recenter();
        }
    }

    /// The Tobii tracker's settings as they are now (they may change in the game's options).
    pub fn set_tobii(&self, o: crate::tobii::Options) {
        if let Some(t) = &self.tobii {
            t.set_options(o);
        }
    }

    /// The latest pose for the camera's filter (`crate::head_filter`), while poses keep coming
    /// (none for half a second: the head is centred): opentrack's, else the Tobii tracker's.
    pub fn sample(&self) -> Option<crate::head_filter::Sample> {
        let centre = self.centre.lock().map(|c| *c).unwrap_or_default();
        let udp = self.udp().map(|u| crate::head_filter::Sample { pose: u.pose.from_centre(&centre), gain: [1.0; 3], edge: 0.0, seq: u.seq, at: u.at, interval: u.interval, tobii: false });
        udp.or_else(|| self.tobii.as_ref().and_then(|t| t.sample()))
    }

    /// opentrack's pose as it came, while poses keep coming.
    fn udp(&self) -> Option<Udp> {
        self.last.lock().ok().and_then(|l| l.filter(|u| u.at.elapsed() < Duration::from_millis(500)))
    }
}

/// A freetrack pose as opentrack writes it into `FT_SharedMem` - yaw, pitch, roll in
/// radians with yaw and pitch negated, x, y, z in mm - in the UDP output's terms (cm and
/// degrees).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn freetrack_pose(yaw: f32, pitch: f32, roll: f32, x: f32, y: f32, z: f32) -> HeadPose {
    HeadPose {
        pos: [x / 10.0, y / 10.0, z / 10.0],
        rot: [-yaw.to_degrees(), -pitch.to_degrees(), roll.to_degrees()],
    }
}

#[cfg(windows)]
mod freetrack {
    use super::{freetrack_pose, HeadPose};
    use std::time::{Duration, Instant};
    use windows::core::w;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, FILE_MAP_READ};

    /// `FTData`: DataID, CamWidth, CamHeight (i32), Yaw, Pitch, Roll, X, Y, Z (f32), ...
    const FIELDS: usize = 9;

    /// The mapping, opened once opentrack has made it (looked for once a second).
    #[derive(Default)]
    pub struct Reader {
        view: Option<(HANDLE, *const u32)>,
        tried: Option<Instant>,
        last_id: Option<u32>,
        announced: bool,
    }

    // (the view is only read, from the head tracking thread)
    unsafe impl Send for Reader {}

    impl Reader {
        /// A new pose, when opentrack has written one since the last call.
        pub fn poll(&mut self) -> Option<HeadPose> {
            if self.view.is_none() {
                if self.tried.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                    return None;
                }
                self.tried = Some(Instant::now());
                // SAFETY: plain Win32 calls; the view stays mapped for the life of the game.
                unsafe {
                    let handle = OpenFileMappingW(FILE_MAP_READ.0, false, w!("FT_SharedMem")).ok()?;
                    let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, FIELDS * 4);
                    if view.Value.is_null() {
                        let _ = windows::Win32::Foundation::CloseHandle(handle);
                        return None;
                    }
                    self.view = Some((handle, view.Value as *const u32));
                }
            }
            let (_, base) = self.view?;
            // SAFETY: the view spans the FIELDS words read here; opentrack writes them
            // from its own process, hence the volatile reads.
            let word = |i: usize| unsafe { base.add(i).read_volatile() };
            let id = word(0);
            if self.last_id.replace(id) == Some(id) {
                return None;
            }
            let f = |i: usize| f32::from_bits(word(i));
            let v = [f(3), f(4), f(5), f(6), f(7), f(8)];
            if v.iter().any(|x| !x.is_finite()) {
                return None;
            }
            if !self.announced {
                self.announced = true;
                log::info!("head tracking: receiving poses from freetrack (FT_SharedMem)");
            }
            Some(freetrack_pose(v[0], v[1], v[2], v[3], v[4], v[5]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A middle taken with the head turned 170 degrees right: a head turned on to 175 left is
    /// 15 degrees further round, not 345 back; the sensitivities make the turn, the tilt and
    /// the way larger or smaller.
    #[test]
    fn the_middle_and_the_sensitivities() {
        let centre = HeadPose { pos: [2.0, 1.0, 0.0], rot: [170.0, 5.0, 0.0] };
        let p = HeadPose { pos: [5.0, 1.0, -3.0], rot: [-175.0, 10.0, 4.0] }.from_centre(&centre);
        assert!((p.rot[0] - 15.0).abs() < 1e-3 && (p.rot[1] - 5.0).abs() < 1e-3, "{p:?}");
        assert_eq!(p.pos, [3.0, 0.0, -3.0]);
        let s = p.scaled(2.0, 0.5, 1.5, 0.0);
        assert!((s.rot[0] - 30.0).abs() < 1e-3 && (s.rot[1] - 2.5).abs() < 1e-3 && (s.rot[2] - 6.0).abs() < 1e-3);
        assert_eq!(s.seat_offset(), glam::Vec3::ZERO);
        // (no tilt: the horizon level)
        assert_eq!(p.scaled(1.0, 1.0, 0.0, 1.0).rot[2], 0.0);
    }

    #[test]
    fn freetrack_pose_is_in_cm_and_degrees() {
        let p = freetrack_pose(-0.5f32.to_radians() * 60.0, 10f32.to_radians(), 5f32.to_radians(), 15.0, -20.0, 100.0);
        assert!((p.rot[0] - 30.0).abs() < 1e-3 && (p.rot[1] + 10.0).abs() < 1e-3 && (p.rot[2] - 5.0).abs() < 1e-3);
        assert_eq!(p.pos, [1.5, -2.0, 10.0]);
    }

    /// The head 10 cm to the right (opentrack's x -10), 5 cm up and 20 cm back: the eye goes
    /// right, up and back with it (#1157).
    #[test]
    fn the_eye_follows_the_head_across() {
        let p = HeadPose { pos: [-10.0, 5.0, 20.0], rot: [0.0; 3] };
        let o = p.seat_offset();
        assert!((o.x - 0.1).abs() < 1e-6 && (o.y + 0.2).abs() < 1e-6 && (o.z - 0.05).abs() < 1e-6, "{o}");
        // (held within 60 cm)
        assert!((HeadPose { pos: [-200.0, 0.0, 0.0], rot: [0.0; 3] }.seat_offset().x - 0.6).abs() < 1e-6);
    }
}

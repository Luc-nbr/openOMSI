//! Head tracking's pose made calm for the camera, frame by frame (Luc: the picture shook).
//! A Tobii eye tracker gives the head some 30 times a second, with noise in it: drawn as it
//! came, the view stood still for a few frames and then jumped, and the noise - made 2.5 times
//! larger and more by the sensitivities - shook it. As opentrack and Tobii's own Extended View
//! (what Euro Truck Simulator 2's eye tracking settings are made of) do it:
//!
//! - between two of the tracker's poses the view glides: the pose aimed at goes from where the
//!   last glide was to the new pose in the time between the two (a pose behind the head);
//! - an adaptive filter follows that, per axis and in steps of 4 ms whatever the frame rate: a
//!   small difference (the tracker's trembling) is followed hardly at all, a large one (a real
//!   turn) at once - and for a moment after one the view keeps up, so it does not creep after
//!   it; a dead zone round where the view is leaves out movements smaller than it, wherever
//!   the head looks (not only straight ahead);
//! - with no pose (the tracker lost the face, the middle being taken, head tracking switched
//!   off) the view glides back straight ahead instead of jumping there.
//!
//! The player sets the smoothing and the dead zone (`Params::tobii`); opentrack's poses, which
//! opentrack filters itself, are only glided (`Params::OPENTRACK`). All in the head's own terms
//! (degrees, cm), before the tracker's turn is made larger (`Sample::gain`).

use std::time::{Duration, Instant};

use crate::headtrack::HeadPose;

/// A pose as a tracker gave it, for `HeadFilter`.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// The head's pose from the middle, in its own terms (opentrack's: degrees, cm).
    pub pose: HeadPose,
    /// The turn made larger after the filter (yaw, pitch, roll).
    pub gain: [f32; 3],
    /// Yaw added after the gain (degrees: a look at the screen's edge).
    pub edge: f32,
    /// Which pose it is (one more with every pose of its source), when it came, and the time
    /// since the one before it (s).
    pub seq: u64,
    pub at: Instant,
    pub interval: f32,
    /// From a Tobii eye tracker (the player's smoothing), else from opentrack.
    pub tobii: bool,
}

/// How strongly the filter works, for the rotation (degrees) and the position (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// A difference this large (beyond the dead zone) is followed at full speed, smaller ones
    /// less and less (0: all at full speed).
    knee: [f32; 2],
    /// A difference within this is left out.
    dead: [f32; 2],
    /// The time the view takes to come most of the way at full speed (s).
    tau: [f32; 2],
}

impl Params {
    /// A Tobii eye tracker's: `smoothing` 0..1 (the player's; at 0.5 the knee is opentrack
    /// Accela's 1.5 degrees and 1 cm), `dead_zone` in degrees (and as many cm). Position is
    /// smoothed more: its noise moves the eye, and the dashboard and the mirrors swim with it.
    pub fn tobii(smoothing: f32, dead_zone: f32) -> Params {
        let s = smoothing.clamp(0.0, 1.0);
        let d = dead_zone.max(0.0);
        Params { knee: [3.0 * s, 2.0 * s], dead: [d, d], tau: [0.02 + 0.06 * s, 0.03 + 0.09 * s] }
    }

    /// opentrack's poses, which opentrack filters itself (its Accela): only glided.
    pub const OPENTRACK: Params = Params { knee: [0.0; 2], dead: [0.0; 2], tau: [0.02; 2] };
}

/// The channels: yaw, pitch, roll (degrees), x, y, z (cm), and the edge's yaw (degrees).
const N: usize = 7;
/// The filter's step at most (s): opentrack's 250 Hz.
const STEP: f32 = 0.004;
/// After a real movement the view keeps up for about this long (s).
const HOLD: f32 = 0.3;
/// With no pose the view goes back straight ahead in about this long (s): Tobii's 5 a second.
const HOME: f32 = 0.2;

/// The filter's state (one for the camera, one for the launcher's preview).
#[derive(Default)]
pub struct HeadFilter {
    out: [f32; N],
    hold: [f32; N],
    /// The glide between two poses: from, to, when it began and how long it takes (s).
    from: [f32; N],
    to: [f32; N],
    t0: Option<Instant>,
    span: f32,
    /// The pose the glide goes to (its source, its number) and its gain.
    key: Option<(bool, u64)>,
    gain: [f32; 3],
    /// The last frame's time.
    last: Option<Instant>,
    /// A pose is followed, or the view is on its way home.
    live: bool,
}

impl HeadFilter {
    /// The pose for the frame at `now` from the tracker's latest (`s`; none: there is none),
    /// with `tobii` for a Tobii tracker's poses. None once the view is straight ahead again.
    pub fn step(&mut self, s: Option<&Sample>, now: Instant, tobii: Params) -> Option<HeadPose> {
        let dt = self.last.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32()).min(0.1);
        self.last = Some(now);
        let Some(s) = s else {
            self.key = None;
            self.t0 = None;
            if !self.live {
                return None;
            }
            // (home: no dead zone, nothing held)
            let k = (-dt / HOME).exp();
            for v in self.out.iter_mut() {
                *v *= k;
            }
            self.hold = [0.0; N];
            if self.out.iter().all(|v| v.abs() < 0.05) {
                *self = HeadFilter { last: self.last, ..HeadFilter::default() };
                return None;
            }
            return Some(self.pose());
        };
        let p = if s.tobii { tobii } else { Params::OPENTRACK };
        let v = [s.pose.rot[0], s.pose.rot[1], s.pose.rot[2], s.pose.pos[0], s.pose.pos[1], s.pose.pos[2], s.edge];
        if self.key != Some((s.tobii, s.seq)) {
            // (a new pose: the glide goes on from where the last one is at the time it came -
            // after none, from where the view is)
            self.from = if self.t0.is_some() { self.target(s.at) } else { self.out };
            self.to = v;
            self.to[0] = self.from[0] + wrap(v[0] - self.from[0]);
            self.t0 = Some(s.at);
            self.span = s.interval.clamp(0.004, 0.1);
            self.key = Some((s.tobii, s.seq));
            self.gain = s.gain;
            self.live = true;
        }
        let n = (dt / STEP).ceil().max(1.0) as u32;
        let h = dt / n as f32;
        for j in 1..=n {
            let t = now.checked_sub(Duration::from_secs_f32((n - j) as f32 * h)).unwrap_or(now);
            let target = self.target(t);
            for i in 0..N {
                let (knee, dead, tau) = match i {
                    0..=2 => (p.knee[0], p.dead[0], p.tau[0]),
                    3..=5 => (p.knee[1], p.dead[1], p.tau[1]),
                    _ => (0.0, 0.0, 0.02),
                };
                let d = if i == 0 { wrap(target[i] - self.out[i]) } else { target[i] - self.out[i] };
                // (the dead zone round the view as it is: no jump at its edge)
                let m = (d.abs() - dead).max(0.0);
                // (a soft gate: the trembling hardly moves the view, a turn moves it at once)
                let g = if knee > 0.0 {
                    let f = (m / knee).powi(2);
                    f / (1.0 + f)
                } else {
                    1.0
                };
                self.hold[i] *= (-h / HOLD).exp();
                if m >= knee {
                    self.hold[i] = 1.0;
                }
                let k = g.max(self.hold[i]);
                self.out[i] += d.signum() * m * (1.0 - (-k * h / tau).exp());
            }
        }
        Some(self.pose())
    }

    /// Where the glide is at `t`.
    fn target(&self, t: Instant) -> [f32; N] {
        let f = self.t0.map_or(1.0, |t0| (t.saturating_duration_since(t0).as_secs_f32() / self.span).clamp(0.0, 1.0));
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * f)
    }

    /// The filtered pose, the tracker's turn made larger and the edge's yaw on top.
    fn pose(&self) -> HeadPose {
        let o = &self.out;
        HeadPose { pos: [o[3], o[4], o[5]], rot: [o[0] * self.gain[0] + o[6], o[1] * self.gain[1], o[2] * self.gain[2]] }
    }
}

/// An angle the short way round (degrees, -180..180).
fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise as a tracker's: normal, from a fixed seed (xorshift, Box-Muller).
    struct Noise(u64);

    impl Noise {
        fn uniform(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32
        }

        fn normal(&mut self, sigma: f32) -> f32 {
            let (a, b) = (self.uniform().max(1e-7), self.uniform());
            (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos() * sigma
        }
    }

    /// A tracker at `hz` giving `head(t)` (yaw, pitch degrees; x cm) with noise `sigma`
    /// (degrees, and half of it in cm), read by frames at `fps` for `secs` seconds; each
    /// frame's yaw, pitch and x from the filter, and the head's yaw then.
    #[allow(clippy::type_complexity)]
    fn run(head: &dyn Fn(f32) -> (f32, f32, f32), sigma: f32, hz: f32, fps: f32, secs: f32, p: Params, tobii: bool) -> Vec<(f32, f32, f32, f32, f32)> {
        let start = Instant::now();
        let mut noise = Noise(0x9e37_79b9_7f4a_7c15);
        let mut f = HeadFilter::default();
        let mut out = Vec::new();
        let mut seq = 0u64;
        let mut sample: Option<Sample> = None;
        let frames = (secs * fps) as usize;
        for k in 0..=frames {
            let t = k as f32 / fps;
            // (the tracker's poses that came by this frame: the last of them is read)
            while (seq as f32) / hz <= t {
                let ts = seq as f32 / hz;
                let (y, pi, x) = head(ts);
                let pose = HeadPose { pos: [x + noise.normal(sigma * 0.5), 0.0, 0.0], rot: [y + noise.normal(sigma), pi + noise.normal(sigma), 0.0] };
                sample = Some(Sample { pose, gain: [1.0; 3], edge: 0.0, seq, at: start + Duration::from_secs_f32(ts), interval: 1.0 / hz, tobii });
                seq += 1;
            }
            let now = start + Duration::from_secs_f32(t);
            let r = f.step(sample.as_ref(), now, p).unwrap_or_default();
            out.push((t, r.rot[0], r.rot[1], r.pos[0], head(t).0));
        }
        out
    }

    fn std(v: &[f32]) -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    /// A head held still (straight ahead and turned 15 degrees, where the old dead zone did
    /// not reach) with the tracker trembling 0.3 degrees: the view stays still.
    #[test]
    fn a_still_head_gives_a_still_view() {
        for at in [0.0, 15.0] {
            let r = run(&|_| (at, 0.0, 0.0), 0.3, 30.0, 144.0, 10.0, Params::tobii(0.5, 0.1), true);
            let rest: Vec<_> = r.iter().filter(|x| x.0 >= 1.0).collect();
            let yaw: Vec<f32> = rest.iter().map(|x| x.1).collect();
            let pitch: Vec<f32> = rest.iter().map(|x| x.2).collect();
            let pos: Vec<f32> = rest.iter().map(|x| x.3).collect();
            let spread = yaw.iter().cloned().fold(f32::MIN, f32::max) - yaw.iter().cloned().fold(f32::MAX, f32::min);
            let step = yaw.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
            assert!(std(&yaw) <= 0.06 && std(&pitch) <= 0.06, "at {at}: std yaw {} pitch {}", std(&yaw), std(&pitch));
            assert!(spread <= 0.3 && step <= 0.05, "at {at}: spread {spread}, step {step}");
            assert!(std(&pos) <= 0.03, "at {at}: std x {}", std(&pos));
            assert!((yaw.iter().sum::<f32>() / yaw.len() as f32 - at).abs() < 0.5);
        }
    }

    /// The head turned 20 degrees at once: the view follows within a fifth of a second of
    /// the pose's coming, never past it.
    #[test]
    fn a_fast_turn_is_followed() {
        for fps in [60.0, 144.0] {
            let r = run(&|t| (if t >= 1.0 { 20.0 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, fps, 2.0, Params::tobii(0.5, 0.1), true);
            let at = |s: f32| r.iter().find(|x| x.0 >= 1.0 + s).unwrap().1;
            assert!(at(0.2) >= 18.0, "{fps} fps: {} after 0.2 s", at(0.2));
            assert!(at(0.35) >= 19.6, "{fps} fps: {} after 0.35 s", at(0.35));
            assert!(r.iter().all(|x| x.1 <= 20.0 + 1e-3));
        }
    }

    /// A steady turn: the view turns as steadily, frame by frame - no frame standing still
    /// and the next jumping, as with the tracker's 30 poses a second drawn as they came.
    #[test]
    fn a_turn_glides_between_samples() {
        let fps = 144.0;
        let r = run(&|t| (30.0 * t, 0.0, 0.0), 0.0, 30.0, fps, 2.0, Params::tobii(0.5, 0.1), true);
        for w in r.windows(2).filter(|w| w[0].0 >= 0.3) {
            let speed = (w[1].1 - w[0].1) * fps;
            assert!((27.0..=33.0).contains(&speed), "{speed} degrees a second at {}", w[0].0);
        }
        let r = run(&|t| (60.0 * t, 0.0, 0.0), 0.0, 30.0, fps, 1.5, Params::tobii(0.5, 0.1), true);
        for x in r.iter().filter(|x| x.0 >= 0.5) {
            assert!(x.4 - x.1 <= 6.0, "{} behind at {}", x.4 - x.1, x.0);
        }
    }

    /// A turn that stops: the view settles where the head stopped, without creeping after it.
    #[test]
    fn the_view_settles_after_a_turn() {
        let r = run(&|t| (60.0 * t.min(0.6), 0.0, 0.0), 0.0, 30.0, 144.0, 1.5, Params::tobii(0.5, 0.1), true);
        let x = r.iter().find(|x| x.0 >= 0.85).unwrap();
        assert!((36.0 - x.1).abs() <= 0.1 + 0.3, "{} at 0.25 s after the stop", x.1);
    }

    /// The tracker lost: the view glides home (never faster than Tobii's 5 a second) and
    /// lets go; the tracker back: the view glides there again rather than jumping.
    #[test]
    fn lost_tracking_glides_home() {
        let start = Instant::now();
        let p = Params::tobii(0.5, 0.1);
        let mut f = HeadFilter::default();
        let held = |seq: u64, yaw: f32, at: f32| Sample { pose: HeadPose { pos: [0.0; 3], rot: [yaw, 0.0, 0.0] }, gain: [1.0; 3], edge: 0.0, seq, at: start + Duration::from_secs_f32(at), interval: 1.0 / 30.0, tobii: true };
        let fps = 144.0;
        let mut t = 0.0;
        let mut k = 0;
        while t < 1.0 {
            f.step(Some(&held(k / 5, 20.0, (k / 5) as f32 / 30.0)), start + Duration::from_secs_f32(t), p);
            k += 1;
            t = k as f32 / fps;
        }
        let mut was = f.step(None, start + Duration::from_secs_f32(t), p).unwrap().rot[0];
        let mut gone = None;
        for j in 1..=(2.0 * fps) as usize {
            let now = t + j as f32 / fps;
            match f.step(None, start + Duration::from_secs_f32(now), p) {
                Some(x) => {
                    let most = was * (1.0 - (-1.0 / fps / HOME).exp()) + 1e-3;
                    assert!(was - x.rot[0] <= most && x.rot[0] <= was, "{} -> {}", was, x.rot[0]);
                    was = x.rot[0];
                }
                None => {
                    gone = Some(now - t);
                    break;
                }
            }
        }
        assert!(gone.is_some_and(|g| g <= 1.5), "{gone:?}");
        // (back, at 10 degrees: no frame closes more than a sixth of the gap)
        let back = t + 1.6;
        let mut was = 0.0f32;
        for j in 0..(0.5 * fps) as usize {
            let now = back + j as f32 / fps;
            let s = held(100 + (j / 5) as u64, 10.0, back + (j / 5) as f32 / 30.0);
            let x = f.step(Some(&s), start + Duration::from_secs_f32(now), p).map_or(0.0, |x| x.rot[0]);
            assert!(x - was <= 0.15 * 10.0, "{was} -> {x}");
            was = x;
        }
        assert!(was > 9.0, "{was}");
    }

    /// The frame rate does not change how the view follows.
    #[test]
    fn the_same_at_every_frame_rate() {
        let at: Vec<f32> = [30.0, 60.0, 144.0, 240.0]
            .iter()
            .map(|&fps| {
                let r = run(&|t| (if t >= 1.0 { 20.0 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, fps, 1.5, Params::tobii(0.5, 0.1), true);
                r.iter().find(|x| x.0 >= 1.2 - 1e-4).unwrap().1
            })
            .collect();
        let spread = at.iter().cloned().fold(f32::MIN, f32::max) - at.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread <= 0.5, "{at:?}");
    }

    /// opentrack's poses (filtered by opentrack, some 250 a second) are only glided: they are
    /// followed closely.
    #[test]
    fn opentrack_is_only_glided() {
        let r = run(&|t| (60.0 * t, 0.0, 0.0), 0.0, 250.0, 144.0, 1.0, Params::tobii(1.0, 1.0), false);
        for x in r.iter().filter(|x| x.0 >= 0.3) {
            assert!(x.4 - x.1 <= 1.5, "{} behind", x.4 - x.1);
        }
        let r = run(&|t| (if t >= 0.5 { 1.0 } else { 0.0 }, 0.0, 0.0), 0.0, 250.0, 144.0, 1.0, Params::tobii(1.0, 1.0), false);
        let x = r.iter().find(|x| x.0 >= 0.56).unwrap();
        assert!(x.1 >= 0.9, "{} after 60 ms", x.1);
    }

    /// The tracker's gain and the edge's yaw come after the filter.
    #[test]
    fn the_gain_comes_after() {
        let start = Instant::now();
        let mut f = HeadFilter::default();
        let s = Sample { pose: HeadPose { pos: [0.0; 3], rot: [10.0, 4.0, 0.0] }, gain: [2.5, 2.0, 1.0], edge: 5.0, seq: 1, at: start, interval: 0.033, tobii: true };
        let mut x = HeadPose::default();
        for k in 0..300 {
            x = f.step(Some(&s), start + Duration::from_millis(4 * k), Params::tobii(0.5, 0.0)).unwrap();
        }
        assert!((x.rot[0] - 30.0).abs() < 0.05 && (x.rot[1] - 8.0).abs() < 0.05, "{x:?}");
    }
}

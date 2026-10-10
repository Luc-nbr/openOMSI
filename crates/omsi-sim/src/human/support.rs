//! Refresh physical support when streaming or a frame transition changes the floor.
use super::*;

impl Pose {
    pub(super) fn refresh_support(&mut self, input: &PoseInput) {
        // Flat-floor callers without a sampler retain their previous animation behavior.
        // Do not drag a planted foot up a stair just because the body's origin rose:
        // sample at that foot's own x/y, in the frame of the floor.
        if input.floor.is_none() {
            return;
        }
        for f in &mut self.feet {
            if f.planted {
                f.pos.z = Self::sample_floor(input, f.pos.truncate(), f.pos.z);
            } else {
                // A landing at the same x/y needs resampling after a vertical change too.
                f.to_sampled = DVec2::splat(f64::MAX);
            }
        }
    }
}

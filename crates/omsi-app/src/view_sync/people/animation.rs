//! Opt-in restoration of the earlier procedural human poses.
//! Keep the original OMSI animation as the default for compatibility and A/B testing.

use super::*;
use glam::{Affine3A, Vec3};
use omsi_sim::human::{Activity, Pose, PoseInput, SLOTS};
use omsi_sim::people::{pax::Task, BusId};

mod floor;

fn is_procedural_mode(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "procedural" | "enhanced" | "1")
}

pub(super) fn enabled() -> bool {
    // Read once per game session (PeopleView::new), not once per process: on Android
    // another game is started in the launcher's existing process.
    let mode = omsi_cfg::flags::OMSI_PAX_ANIMATION.live_var()
        .unwrap_or_else(|_| crate::settings::Settings::load().passenger_animation);
    is_procedural_mode(&mode)
}

/// Only riders assigned a standing place should hold a pole. Queued and walking
/// passengers must keep their hands free to use the doors and ticket machines.
fn grip_for_standing_rider(inside: bool, assigned_standing: bool, walking: bool, reaching: bool) -> f32 {
    if inside && assigned_standing && !walking && !reaching { 1.0 } else { 0.0 }
}

/// A passenger is anchored at a model's [seatheight] below the seat's hip point.
/// Its actual floor is at the cabin [passpos] height below that point, which may differ.
/// Re-anchor the procedural floor and keep the pelvis on the seat's [passpos], not below it.
fn seated_pose(model_height: f32, place_height: f32, seat_front: f32, seat_lift: f32) -> (Vec3, f32) {
    let lift = model_height - place_height;
    (Vec3::new(0.0, seat_front - 0.03, place_height - seat_lift), lift)
}

/// The previous foot-planted / IK pose system, driven by the current people's state.
/// This changes only how bodies are skinned, never boarding, movement or ticket logic.
pub(super) fn bones(p: &mut Person, dt: f32, world: &World, buses: &HashMap<BusId, &omsi_sim::people::cabin::BusNow>) -> [Affine3A; SLOTS] {
    let (origin, heading, frame) = match p.place {
        Place::Ground => (p.position, p.heading, 0),
        Place::Bus(bus, at) => {
            let frame = match bus {
                BusId::Player => 1,
                BusId::Ai(id) => id.wrapping_add(2),
            };
            (at.as_dvec3(), p.lheading, frame)
        }
    };
    let (activity, seat, reach, floor_lift, hold) = match &p.state {
        State::Pax(x) => {
            let kind = x.pax_state.round().clamp(0.0, 2.0) as u8;
            let activity = match kind {
                2 => Activity::Sit,
                1 => Activity::Walk,
                _ if x.reach => Activity::Pay,
                _ => Activity::Stand,
            };
            let (seat, floor_lift) = if kind == 2 {
                let (seat, lift) = seated_pose(p.ty.def.seat_height, x.seat_h, p.ty.rig.seat_front(), p.ty.rig.seat_lift);
                (Some(seat), lift)
            } else {
                (None, 0.0)
            };
            let reach = (x.reach && x.inside.is_some()).then(|| {
                let d = x.reach_at.as_dvec3() - x.pos;
                let (s, c) = x.yaw.sin_cos();
                Vec3::new((d.x * c - d.y * s) as f32, (d.x * s + d.y * c) as f32, d.z as f32)
            });
            let hold = grip_for_standing_rider(
                x.inside.is_some(),
                x.task == Task::SittingInBus && kind == 0,
                kind == 1,
                x.reach,
            );
            (activity, seat, reach, floor_lift, hold)
        }
        _ => (p.activity, None, None, 0.0, 0.0),
    };
    let bus = match p.place {
        Place::Bus(id, _) => buses.get(&id).copied(),
        Place::Ground => None,
    };
    // A seated renderer anchor is [passpos] - human [seatheight], not the floor.
    let level = match &p.state {
        State::Pax(x) if x.pax_state.round() >= 1.5 => origin.z + (p.ty.def.seat_height - x.seat_h) as f64,
        _ => origin.z,
    };
    let sample = |at| floor::sample(world, bus, at, level);
    let input = PoseInput {
        activity,
        // The simulation's renderer still places a seated person at hip - model height.
        // The IK foot floor is hip - physical seat height, which can be higher.
        origin: origin + glam::DVec3::Z * floor_lift as f64,
        heading,
        frame,
        velocity: p.vel,
        seat,
        reach,
        hold,
        floor: Some(&sample),
        ..PoseInput::default()
    };
    let seed = p.id;
    let pose = p.procedural.get_or_insert_with(|| Pose::new(seed));
    pose.advance(&p.ty.rig, &input, dt);
    let mut posed = pose.bones(&p.ty.rig);
    // Shift the local pose back to the renderer's original anchor; no game logic moves.
    if floor_lift != 0.0 {
        let lift = Affine3A::from_translation(Vec3::Z * floor_lift);
        for bone in &mut posed.bones {
            *bone = lift * *bone;
        }
        posed.ankle = posed.ankle.map(|a| lift.transform_point3(a));
    }
    // OMSI_TRACE_PAX records the deformed ankles rather than the untouched spawn markers.
    p.ankles = posed.ankle;
    posed.bones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seated_floor_and_pelvis_use_different_seat_heights() {
        // Test the geometry used by the renderer: an OMSI model with [seatheight]
        // 0.82 m placed at a 0.45 m physical seat height.
        let model = 0.82;
        let physical = 0.45;
        let seat_front = 0.34;
        let seat_lift = 0.10;
        // The same calculation as seated_pose, with rig measurements supplied.
        let (seat, floor_lift) = seated_pose(model, physical, seat_front, seat_lift);
        // The pelvis is still precisely at [passpos] (the simulation origin + 0.82).
        assert!((floor_lift + seat.z + seat_lift - model).abs() < 1e-5);
        // The hip aligns with [passpos] instead of being behind the seat.
        assert!((seat.y - seat_front + 0.03).abs() < 1e-5);
        // The soles follow the real floor of the seat, not model [seatheight].
        assert!((floor_lift - (model - physical)).abs() < 1e-5);
    }

    #[test]
    fn only_stationary_standing_riders_hold_the_rail() {
        assert_eq!(grip_for_standing_rider(true, true, false, false), 1.0);
        assert_eq!(grip_for_standing_rider(true, false, false, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, true, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, false, true), 0.0);
        assert_eq!(grip_for_standing_rider(false, true, false, false), 0.0);
    }

    #[test]
    fn enhanced_pose_requires_explicit_opt_in() {
        assert!(!is_procedural_mode(""));
        assert!(!is_procedural_mode("original"));
        assert!(!is_procedural_mode("0"));
        assert!(is_procedural_mode("procedural"));
        assert!(is_procedural_mode("enhanced"));
        assert!(is_procedural_mode("ENHANCED"));
        assert!(is_procedural_mode("1"));
    }
}

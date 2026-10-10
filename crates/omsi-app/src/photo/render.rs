//! The photo's pictures: a quick one while the camera moves, and once it stands the pictures
//! of the shot one after another (as many as fit a frame's time), added up and developed -
//! what is shown is what is saved. "Take photo" waits for the last of them, at the size
//! asked for, and writes the PNG.

use std::time::Instant;

use super::develop::{self, Accum};
use super::lens::{self, Shot};
use super::{Photo, ASPECTS, BLADES, FSTOPS, QUALITY, SHUTTERS, SIZES};

/// Where the photo stands.
#[derive(Default)]
pub(crate) struct State {
    accum: Option<Accum>,
    /// What the pictures added up were taken with: another camera, lens or light starts the
    /// shot again.
    key: Option<String>,
    /// The pictures wanted for the shot as it stands.
    target: u32,
    /// "Take photo" was asked for: saved once the shot is complete.
    pub capture: bool,
    /// The developed picture is out of date (a darkroom setting changed).
    grade: Option<super::Grade>,
    crop: Option<(u32, u32, u32, u32)>,
    /// The last developed picture (w, h, RGBA).
    pub developed: Option<(u32, u32, Vec<u8>)>,
    /// Since when the camera stands still (None: it moves). While it moves the window shows
    /// the live picture at the game's own frame rate; once it has stood a moment the photo's
    /// pictures are taken.
    still_since: Option<Instant>,
    /// The developed picture is of the camera as it stands now (else the live picture shows).
    ready: bool,
}

/// How long the camera stands before the photo is taken over the live picture.
const SETTLE_SECS: f32 = 0.2;

impl State {
    pub(crate) fn reset(&mut self) {
        self.key = None;
    }

    /// How far the shot is (pictures taken, wanted).
    pub(crate) fn progress(&self) -> (u32, u32) {
        (self.accum.as_ref().map(|a| a.n).unwrap_or(0), self.target)
    }

    /// The window shows the developed photo (else the live picture): the camera stands and
    /// the photo of it is there.
    pub(crate) fn showing_photo(&self) -> bool {
        (self.ready && !self.moving()) || (self.capture && self.ready)
    }

    /// The camera moves (or has only just stopped): the window shows the live picture.
    pub(crate) fn moving(&self) -> bool {
        !self.capture && self.still_since.is_none_or(|t| t.elapsed().as_secs_f32() < SETTLE_SECS)
    }
}

impl Photo {
    /// How the shot is taken now.
    pub(crate) fn shot(&self) -> Shot {
        let s = &self.settings;
        Shot {
            dof: s.dof,
            fstop: FSTOPS[s.fstop.min(FSTOPS.len() - 1)],
            focus: self.focus_distance(),
            blades: BLADES[s.blades.min(BLADES.len() - 1)].0,
            motion: (s.motion && self.bus_velocity.length() > 0.2).then(|| (self.bus_velocity, SHUTTERS[s.shutter.min(SHUTTERS.len() - 1)].0)),
        }
    }

    /// The frame cut by the chosen aspect ratio.
    pub(crate) fn aspect(&self) -> Option<f32> {
        ASPECTS[self.settings.aspect.min(ASPECTS.len() - 1)].0
    }
}

impl crate::App {
    /// The photo mode's pictures for this frame, developed and handed to the screens.
    pub(crate) fn photo_render(&mut self, lighting: &omsi_render::Lighting) {
        let (Some(ph), Some(s), Some(r), Some(scene)) = (self.photo.as_mut(), self.gfx.surface.as_ref(), self.renderer.as_mut(), self.scene.as_mut()) else { return };
        let (ww, wh) = (s.config.width, s.config.height);
        let shot = ph.shot();
        let cam = ph.cam;
        // what the shot is: the camera to the millimetre and the tenth of a degree, the lens,
        // the clock (the light), the window
        let key = format!(
            "{:.3},{:.3},{:.3},{:.2},{:.2},{:.2},{:.3}|{:?}|{}|{}x{}",
            cam.position.x, cam.position.y, cam.position.z, cam.yaw, cam.pitch, cam.roll, cam.fov_deg, shot, self.clock.time as i64, ww, wh
        );
        let changed = ph.render.key.as_deref() != Some(key.as_str());
        let camera_moved = ph.render.key.as_ref().is_none_or(|k| k.split('|').next() != key.split('|').next());
        if camera_moved {
            ph.render.still_since = None;
        }
        if ph.render.still_since.is_none() && !camera_moved {
            ph.render.still_since = Some(Instant::now());
        }
        if camera_moved {
            // (the live picture shows: nothing is taken, the shot starts again once it stands)
            ph.render.key = Some(key);
            ph.render.accum = None;
            ph.render.target = 1;
            ph.render.ready = false;
            return;
        }
        if ph.render.moving() {
            return;
        }
        let samples = if shot.dof || shot.motion.is_some() { QUALITY[ph.settings.quality.min(QUALITY.len() - 1)].0 } else { 1 };
        // (the size: the window's, the saved photo's while one is being taken)
        let scale = if ph.render.capture { SIZES[ph.settings.size.min(SIZES.len() - 1)].0 } else { 1.0 };
        let max = r.device.limits().max_texture_dimension_2d.min(8192) as f32;
        let k = (scale).min(max / ww.max(1) as f32).min(max / wh.max(1) as f32);
        let (w, h) = (((ww as f32 * k).round() as u32).max(1), ((wh as f32 * k).round() as u32).max(1));
        let target = samples;
        if changed || ph.render.accum.as_ref().is_none_or(|a| a.w != w || a.h != h) {
            ph.render.accum = Some(Accum::new(w, h));
            ph.render.key = Some(key);
        }
        ph.render.target = target;
        let aspect = ph.aspect();
        let Some(acc) = ph.render.accum.as_mut() else { return };
        let mut added = false;
        if acc.n < target {
            // the bus's parts, to move with the shutter (a rolling shot)
            let parts: Vec<(usize, glam::DVec3, glam::Mat4)> = match (shot.motion.is_some(), self.player.as_ref()) {
                (true, Some(p)) => p.render.instances.iter().chain(p.trailer_renders.iter().flat_map(|t| t.instances.iter())).filter_map(|&i| scene.instances.get(i).map(|x| (i, x.origin, x.transform))).collect(),
                _ => Vec::new(),
            };
            // and its wheels, turning as far as they roll in that time
            let spin: Vec<Option<(glam::Mat4, glam::Vec3, f32)>> = match (shot.motion.is_some(), self.player.as_ref()) {
                (true, Some(p)) => wheel_spin(&p.vehicle, p.render.instances.len()),
                _ => Vec::new(),
            };
            let speed = shot.motion.map(|m| m.0.length()).unwrap_or(0.0);
            let t0 = Instant::now();
            // as many pictures as fit some 40 ms (one at least)
            while acc.n < target && (acc.n == 0 || t0.elapsed().as_secs_f32() < 0.040 || ph.render.capture) {
                let (mut c, t) = lens::sample(&cam, acc.n, &shot, h as f32);
                // (the camera travels with the bus: the bus stays sharp, the street passes)
                let d = lens::travel(&shot, t);
                c.position += d;
                for (k, (i, o, tr)) in parts.iter().enumerate() {
                    let tr = match spin.get(k).copied().flatten() {
                        // (about the axle through the wheel's centre, in the body's frame)
                        Some((body, centre, radius)) => {
                            let turn = glam::Mat4::from_translation(centre) * glam::Mat4::from_rotation_x(speed * t / radius.max(0.1)) * glam::Mat4::from_translation(-centre);
                            body * turn * body.inverse() * *tr
                        }
                        None => *tr,
                    };
                    r.set_transform(scene, *i, *o + d, tr);
                }
                match r.render_to_image(scene, w, h, &c, lighting) {
                    Ok(px) => acc.add(&px),
                    Err(e) => {
                        log::warn!("photo mode: the picture could not be rendered: {e}");
                        break;
                    }
                }
                added = true;
                // (a photo being taken goes on in the next frame: the window stays alive)
                if ph.render.capture && t0.elapsed().as_secs_f32() > 0.25 {
                    break;
                }
            }
            for (i, o, tr) in &parts {
                r.set_transform(scene, *i, *o, *tr);
            }
        }
        let crop = Some(develop::crop_rect(w, h, aspect)).filter(|c| (c.2, c.3) != (w, h));
        if added || ph.render.grade != Some(ph.settings.grade) || ph.render.crop != crop {
            let px = develop::develop(acc, &ph.settings.grade, crop);
            ph.render.grade = Some(ph.settings.grade);
            ph.render.crop = crop;
            self.shell.set_picture(w, h, px.clone());
            ph.render.developed = Some((w, h, px));
            ph.render.ready = true;
        }
        if ph.render.capture && acc.n >= target {
            ph.render.capture = false;
            log::info!("photo mode: {} pictures of {w}x{h} in the photo", acc.n);
            let note = match ph.render.developed.as_ref() {
                Some((w, h, px)) => save(&self.args.root, *w, *h, px, crop),
                None => Err("nothing to save".into()),
            };
            ph.saved = None;
            ph.note = Some(match note {
                Ok(path) => {
                    log::info!("photo mode: saved {}", path.display());
                    crate::plugins::queue_event(&mut self.integrations.plugin_events, "screenshot", vec![omsi_plugin::InfoValue::Text(path.to_string_lossy().into_owned())]);
                    crate::platform::to_gallery(&path);
                    ph.saved = Some(path);
                    (omsi_ui::tr(if crate::platform::MOBILE { "Photo saved to the gallery" } else { "Photo saved" }).into_owned(), 8.0)
                }
                Err(e) => {
                    log::warn!("photo mode: the photo could not be saved: {e}");
                    (format!("{} {e}", omsi_ui::tr("Not saved:")), 6.0)
                }
            });
        }
    }
}

/// For each mesh of the vehicle (in the order of its render's instances): a wheel's body
/// rotation, its centre in the body's frame and its radius - the meshes a `Wheel_Rotation_*`
/// variable turns, the tallest of each giving the centre - else None.
fn wheel_spin(v: &omsi_sim::VehicleInstance, n: usize) -> Vec<Option<(glam::Mat4, glam::Vec3, f32)>> {
    let ty = &v.ty;
    let body = v.body_rotation();
    let inv = body.inverse();
    // (by variable: the centre and half height of its tallest mesh, at rest)
    let mut by_var: std::collections::HashMap<String, (usize, glam::Vec3, f32)> = std::collections::HashMap::new();
    let mut var_of: Vec<Option<String>> = vec![None; n];
    for (i, vm) in ty.meshes.iter().enumerate().take(n) {
        let def = &ty.model.meshes[vm.def_index];
        let Some(an) = def.animations.iter().find(|a| a.variable.to_ascii_lowercase().starts_with("wheel_rotation_")) else { continue };
        let Some(data) = ty.mesh_data(i) else { continue };
        if data.positions.is_empty() {
            continue;
        }
        let (lo, hi) = data.positions.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let half = (hi.z - lo.z) * 0.5;
        let key = an.variable.to_ascii_lowercase();
        var_of[i] = Some(key.clone());
        let e = by_var.entry(key).or_insert((i, (lo + hi) * 0.5, half));
        if half > e.2 {
            *e = (i, (lo + hi) * 0.5, half);
        }
    }
    var_of
        .iter()
        .map(|var| {
            let (i, c, r) = *by_var.get(var.as_ref()?)?;
            // (the centre where the wheel is now: in the body's frame)
            let centre = (inv * v.mesh_local_transform(i)).transform_point3(c);
            Some((body, centre, r))
        })
        .collect()
}

/// The photo written as a PNG into the content folder's `Screenshots`, named by the date and
/// time; cut to its frame.
fn save(root: &std::path::Path, w: u32, h: u32, px: &[u8], crop: Option<(u32, u32, u32, u32)>) -> Result<std::path::PathBuf, String> {
    let dir = crate::startup::content_dir().unwrap_or_else(|| root.to_path_buf()).join("Screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono_like_stamp();
    let mut path = dir.join(format!("openOMSI_photo_{stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("openOMSI_photo_{stamp}_{n}.png"));
        n += 1;
    }
    let (cw, ch, data) = match crop {
        Some(c) => (c.2, c.3, develop::cropped(px, w, c)),
        None => (w, h, px.to_vec()),
    };
    image::save_buffer(&path, &data, cw, ch, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The local date and time as `YYYY-MM-DD_HH-MM-SS` (the seconds since 1970 where the
/// device's calendar cannot be read).
fn chrono_like_stamp() -> String {
    match crate::real_time::now() {
        Some(n) => {
            let s = n.secs as i64;
            format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", n.year, n.month, n.day, s / 3600, s % 3600 / 60, s % 60)
        }
        None => {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            format!("{secs:019}")
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stamp_reads_as_a_date_and_time() {
        let s = super::chrono_like_stamp();
        assert_eq!(s.len(), 19, "{s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "_");
    }
}

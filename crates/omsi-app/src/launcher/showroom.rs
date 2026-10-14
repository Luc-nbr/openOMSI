//! The launcher's bus preview: the chosen bus drawn by the game's own renderer - its
//! model, paint and materials - on the plain (Vanilla+) path without the costly passes,
//! under the light of the chosen time and weather, into a picture the launcher shows
//! in a card. It is drawn again only when something changed (another bus, paint, light,
//! the preview turned by the mouse), never every frame.
//!
//! A bus is read on a worker (its type, its scripts run to their resting state, its
//! textures and meshes put on the GPU ahead) and then placed into a scene of its own with
//! a fresh `World` (whose caches belong to one scene); the old scene goes when the new one
//! is ready, so the picture never goes blank while switching.
//!
//! Omsi-Hub: a second showroom, the studio, takes the bus step's photos (`omsihub_game::
//! service::photo`): the same bus in the same light, from a corner that never moves, read
//! back into a picture; and the livery studio draws its bus with a camera of its own here.

use super::super::*;
// Omsi-Hub: what it shows and how a bus is framed, as the livery studio and the photos need it
pub(crate) use omsihub_game::showroom::*;
// (its `Parts` is the bus shown, not the app's)
pub(crate) use omsihub_game::showroom::Parts;
use glam::{DVec3, Vec3};
use omsi_render::{Camera, Lighting, Renderer, Scene};
use std::sync::mpsc::{channel, Receiver};


struct Ready {
    look: Look,
    world: Arc<scene::World>,
    vt: Arc<omsi_sim::VehicleType>,
    vehicle: omsi_sim::VehicleInstance,
    scheme: Option<usize>,
    /// The bus options it was made with.
    options: Vec<(String, f32)>,
}

struct Shown {
    look: Look,
    scene: Scene,
    world: Option<Arc<scene::World>>,
    vehicle: Option<omsi_sim::VehicleInstance>,
    /// Its livery, and the bus options it wears.
    scheme: Option<usize>,
    options: Vec<(String, f32)>,
    /// The display font its destination displays are drawn in (None: its own).
    letters: Option<String>,
    render: Option<scene::VehicleRender>,
    trailers: Vec<scene::VehicleRender>,
    /// The bus's type (the livery studio reads its meshes and paint slots).
    vt: Option<Arc<omsi_sim::VehicleType>>,
    /// Centre and size of the bus (its bounding box), as the studio frames its photos.
    centre: glam::Vec3,
    length: f32,
    /// The whole bus's box (the rear section of an articulated one in it), as the showroom
    /// frames it.
    bus: BusBox,
    weather: omsi_content::weather::Weather,
    /// The sky's cloud type and weather model its look is drawn with.
    sky: crate::weather_setup::SkyState,
    lighting: Lighting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewView {
    #[default]
    Outside,
    Passenger,
    Driver,
}

impl PreviewView {
    pub const ALL: [PreviewView; 3] = [PreviewView::Outside, PreviewView::Passenger, PreviewView::Driver];

    pub fn name(self) -> &'static str {
        match self {
            PreviewView::Outside => "outside",
            PreviewView::Passenger => "passenger",
            PreviewView::Driver => "driver",
        }
    }

    pub fn from_name(name: &str) -> Option<PreviewView> {
        PreviewView::ALL.into_iter().find(|v| v.name() == name)
    }
}

fn authored_camera(def: &omsi_vehicle::Vehicle, view: PreviewView) -> Option<&omsi_vehicle::Camera> {
    match view {
        PreviewView::Outside => None,
        PreviewView::Passenger => def.cameras_pax.first(),
        PreviewView::Driver => def.cameras_driver.get(def.camera_std).or(def.cameras_driver.first()),
    }
}

impl Shown {
    fn inside_camera(&self, view: PreviewView, look: (f32, f32), zoom: f32) -> Option<Camera> {
        let vehicle = self.vehicle.as_ref()?;
        let camera = |c: &omsi_vehicle::Camera, (position, yaw, pitch, roll): (DVec3, f32, f32, f32)| Camera {
            position,
            yaw,
            pitch: pitch.clamp(-89.0, 89.0),
            roll,
            fov_deg: (c.fov * zoom).clamp(20.0, 100.0),
            near: 0.1,
            far: 6000.0,
        };
        let turned = |c: &omsi_vehicle::Camera| omsi_vehicle::Camera { yaw: c.yaw + look.0, pitch: (c.pitch + look.1).clamp(-89.0, 89.0), ..c.clone() };
        if let Some(c) = authored_camera(&vehicle.ty.def, view) {
            return Some(camera(c, vehicle.camera_world_full(&turned(c))));
        }
        vehicle.trailers.iter().find_map(|t| {
            let c = authored_camera(&t.ty.def, view)?;
            Some(camera(c, t.camera_world_full(&turned(c))))
        })
    }
}

pub struct Showroom {
    shown: Option<Shown>,
    wanted: Option<Look>,
    /// The last look that could not be shown (not read again until something changes).
    failed: Option<Look>,
    /// The bus shown is read again even when the look stays (its files changed: a livery
    /// saved from the studio).
    reread: bool,
    loading: Option<(Look, Receiver<Result<Ready, String>>)>,
    /// The bus options to put on the bus (see `dress`), and the bus being made anew with
    /// them (the options it gets, the bus when it is made).
    options: Vec<(String, f32)>,
    dressing: Option<(Vec<(String, f32)>, Receiver<omsi_sim::VehicleInstance>)>,
    /// The display font to draw the bus's destination displays in (see `letter`).
    letters: Option<String>,
    /// The map the bus being read opened on its worker, for this thread to give it the
    /// renderer's prefetch (see `start_loading`).
    opening: Option<(Receiver<Arc<scene::World>>, std::sync::mpsc::Sender<scene::VehiclePrefetch>)>,
    pub error: Option<String>,
    /// Orbit: yaw and pitch (degrees) and distance factor, eased towards the targets.
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    yaw_to: f32,
    pitch_to: f32,
    zoom_to: f32,
    pub auto_turn: bool,
    pub view: PreviewView,
    inside_look: (f32, f32),
    inside_zoom: f32,
    idle: f32,
    /// Where the bus stands in the picture (see `frame`): the part asked for, the part it is
    /// framed in now (eased towards it), and the seconds since it was last asked for.
    stage_to: Stage,
    stage_now: Stage,
    stage_idle: f32,
    /// The bus picker's photographer: the bus framed the old way, so that the photos taken
    /// before stay like the new ones.
    studio: bool,
    pub busy: bool,
    /// The picture: its texture and size, and whether it must be drawn again.
    target: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    dirty: bool,
    /// Bumped whenever `target` was made anew (the interface binds it again).
    pub generation: u64,
    /// Where the camera aims: ahead of the bus's middle by this share of its length.
    aim: f32,
    /// A camera of the caller's own in place of the orbit (the livery studio's).
    camera: Option<Camera>,
    /// A second picture with a camera of its own, drawn with the first (the livery studio's
    /// other side while it mirrors), and its generation.
    second: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    second_camera: Option<Camera>,
    pub second_generation: u64,
    /// The second picture's size, to be made at the next `preview`.
    pending_second: Option<(u32, u32)>,
    /// Bumped at every drawing of the picture (the launcher shows an unchanged one only once).
    pub drawn: u64,
}

/// Names the bus whose preview is being read and placed, until it is in the picture: the bus
/// and the process placing it. Omsi-Hub: the photo studio keeps a mark of its own (sharing one,
/// a bus it was still photographing when it was chosen was taken for one that had hung, and the
/// studio took away the mark of the bus being shown).
fn placing_mark(studio: bool) -> PathBuf {
    preview_guard_path(&omsi_launcher_lib::data_dir(), placing_kind(studio), omsi_platform::VERSION)
}

/// The placing mark's kind (`preview_guard_path`): the bus preview's, or the photo studio's.
fn placing_kind(studio: bool) -> &'static str {
    if studio {
        "studio-placing"
    } else {
        "placing"
    }
}

fn preview_guard_path(dir: &Path, kind: &str, version: &str) -> PathBuf {
    // A newer renderer or loader must get a chance to retry previews an older one failed.
    dir.join(format!("showroom-{kind}-{version}.txt"))
}

/// Whether the preview of `bus` is left out: placing it hung the program of this version once
/// (its mark of `kind` names it, and not this process `pid`: Omsi-Hub's other showroom - the
/// photo studio's - may be placing the same bus now), now or before.
fn preview_is_skipped(dir: &Path, version: &str, kind: &str, bus: &str, pid: u32) -> bool {
    let mark = preview_guard_path(dir, kind, version);
    let skip = preview_guard_path(dir, "skip", version);
    if std::fs::read_to_string(&mark).is_ok_and(|m| hung(&m, bus, pid)) {
        log::warn!("showroom: the preview of {bus} did not finish when it was last tried; it is left out for this version ({})", skip.display());
        let _ = std::fs::remove_file(mark);
        let mut list = std::fs::read_to_string(&skip).unwrap_or_default();
        list.push_str(bus.trim());
        list.push('\n');
        let _ = std::fs::write(&skip, list);
    }
    std::fs::read_to_string(skip).is_ok_and(|t| t.lines().any(|b| b.trim() == bus.trim()))
}

/// Whether a placing mark (the file's text) says that placing `bus` hung a launcher service: it
/// names the bus, and not this process `pid`. A mark without a process names the bus alone.
fn hung(mark: &str, bus: &str, pid: u32) -> bool {
    let mut lines = mark.lines();
    lines.next().is_some_and(|b| b.trim() == bus.trim()) && lines.next().is_none_or(|p| p.trim() != pid.to_string())
}

/// The launcher ends in order (closed, or a test's time is up) while a preview is being
/// placed: that is no hang, and the bus must not be left out of the previews from then on
/// (a test ended mid-placing put the bus it showed into the skip list). Both marks go: the
/// bus preview's and the photo studio's.
pub(super) fn clear_placing_mark() {
    for studio in [false, true] {
        let _ = std::fs::remove_file(placing_mark(studio));
    }
}

fn args_for(look: &Look) -> Args {
    let mut v = vec!["omsi".to_string(), "--root".into(), look.root.to_string_lossy().to_string()];
    if !look.map.is_empty() {
        v.extend(["--map".into(), look.map.clone()]);
    }
    if !look.bus.is_empty() {
        v.extend(["--bus".into(), look.bus.clone()]);
    }
    if !look.paint.is_empty() {
        v.extend(["--paint".into(), look.paint.clone()]);
    }
    // (the current weather is fetched by the game, not by the preview)
    if !look.weather.is_empty() && !look.weather.starts_with("metar:") {
        v.extend(["--weather".into(), look.weather.clone()]);
    }
    v.extend(["--time".into(), format!("{:02}:{:02}", look.time / 60, look.time % 60)]);
    if !look.date.is_empty() {
        v.extend(["--date".into(), look.date.clone()]);
    }
    let mut args = Args::try_parse_from(v).unwrap_or_else(|_| Args::parse_from(["omsi"]));
    // (drawn the way the game will be)
    args.session_settings = crate::startup::SessionSettings::for_showroom(&crate::settings::Settings::load());
    args
}

// (a launcher closed while a preview loads did not hang: the mark goes with it)
impl Drop for Showroom {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(placing_mark(self.studio));
    }
}

impl Showroom {
    pub fn new() -> Showroom {
        Showroom {
            shown: None,
            wanted: None,
            failed: None,
            loading: None,
            options: Vec::new(),
            dressing: None,
            letters: None,
            opening: None,
            error: None,
            yaw: 215.0,
            pitch: 8.0,
            zoom: 1.0,
            yaw_to: 215.0,
            pitch_to: 8.0,
            zoom_to: 1.0,
            auto_turn: false,
            view: PreviewView::Outside,
            inside_look: (0.0, 0.0),
            inside_zoom: 1.0,
            idle: 0.0,
            stage_to: WHOLE,
            stage_now: WHOLE,
            stage_idle: f32::INFINITY,
            studio: false,
            busy: false,
            target: None,
            dirty: true,
            generation: 0,
            aim: 0.0,
            reread: false,
            camera: None,
            second: None,
            second_camera: None,
            second_generation: 0,
            pending_second: None,
            drawn: 0,
        }
    }

    /// The bus shown, its scene and type, for the livery studio (None while none is placed).
    pub fn parts(&mut self) -> Option<Parts<'_>> {
        let s = self.shown.as_mut()?;
        Some(Parts { scene: &mut s.scene, vt: s.vt.as_ref()?, vehicle: s.vehicle.as_ref()?, render: s.render.as_ref()?, trailers: &s.trailers })
    }

    /// Draw the bus with `cam` instead of the orbit (None: the orbit again); the picture is drawn
    /// again when it changed.
    pub fn set_camera(&mut self, cam: Option<Camera>) {
        let differs = match (&self.camera, &cam) {
            (Some(a), Some(b)) => a.position != b.position || a.yaw != b.yaw || a.pitch != b.pitch,
            (None, None) => false,
            _ => true,
        };
        if differs {
            self.camera = cam;
            self.dirty = true;
        }
    }

    /// A second picture of the bus with `cam` (None: none), `w` x `h` pixels, drawn whenever the
    /// first is: its view, made with the first's `preview`.
    pub fn set_second(&mut self, cam: Option<(Camera, u32, u32)>) {
        let Some((cam, w, h)) = cam else {
            self.second = None;
            self.second_camera = None;
            return;
        };
        let differs = self.second_camera.is_none_or(|a| a.position != cam.position || a.yaw != cam.yaw || a.pitch != cam.pitch);
        let (w, h) = (w.max(16), h.max(16));
        if self.second.as_ref().is_none_or(|t| (t.2, t.3) != (w, h)) {
            self.second = None;
            self.second_camera = None;
            self.pending_second = Some((w, h));
        }
        if differs {
            self.dirty = true;
        }
        self.second_camera = Some(cam);
    }

    /// The second picture (see `set_second`).
    pub fn second_view(&self) -> Option<(wgpu::TextureView, u32, u32)> {
        self.second.as_ref().map(|t| (t.1.clone(), t.2, t.3))
    }

    /// Draw the picture again (a texture of the bus changed).
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// A showroom that takes the bus picker's photos: the bus in the middle of the picture,
    /// from the same corner every time, nothing eased or turned.
    pub fn studio() -> Showroom {
        let mut s = Showroom::new();
        (s.yaw, s.yaw_to, s.pitch, s.pitch_to, s.zoom, s.zoom_to) = (PHOTO_YAW, PHOTO_YAW, PHOTO_PITCH, PHOTO_PITCH, PHOTO_ZOOM, PHOTO_ZOOM);
        (s.studio, s.aim) = (true, PHOTO_AIM);
        s
    }

    /// The part of the picture the bus is to stand in (shares of it; see `stage_in`), asked
    /// for every frame the showroom is on screen. Away for a moment (the tiles were shown),
    /// the bus is put there at once; while shown it glides there (the window resized).
    /// (Omsi-Hub's game-drawn bus step asked for it; the desktop launcher's shows the whole
    /// picture and does not yet.)
    #[allow(dead_code)]
    pub fn set_stage(&mut self, stage: Stage) {
        if self.stage_idle > 0.3 && self.stage_now != stage {
            self.stage_now = stage;
            self.dirty = true;
        }
        self.stage_to = stage;
        self.stage_idle = 0.0;
    }

    /// The stage eased towards the one asked for; the picture is drawn again while it moves.
    fn ease_stage(&mut self, dt: f32) {
        self.stage_idle += dt;
        let k = 1.0 - (-dt / 0.35).exp();
        for (now, to) in self.stage_now.iter_mut().zip(self.stage_to) {
            if (to - *now).abs() > 1e-4 {
                *now += (to - *now) * k;
                self.dirty = true;
            } else {
                *now = to;
            }
        }
    }

    /// `look` is in place, ready to be drawn.
    pub fn shows(&self, look: &Look) -> bool {
        self.shown.as_ref().is_some_and(|s| &s.look == look && s.vehicle.is_some())
    }

    /// `look` will not be shown: the bus did not load, or the map did not open.
    pub fn gave_up(&self, look: &Look) -> bool {
        self.failed.as_ref() == Some(look) || (self.wanted.is_none() && self.loading.is_none() && !self.shows(look))
    }

    /// The files of bus `bus` changed (a livery saved into the game): when it is the bus
    /// shown or asked for, it is read again with whatever paint is chosen; the picture before
    /// stays until then.
    pub fn reread(&mut self, bus: &str) {
        let of_bus = |l: &Look| l.bus == bus;
        if self.wanted.as_ref().is_some_and(of_bus) || self.shown.as_ref().is_some_and(|s| of_bus(&s.look)) {
            self.reread = true;
            self.failed = None;
        }
    }

    /// Let go of the bus shown and its scene (the studio between two photos holds nothing).
    pub fn forget(&mut self) {
        self.shown = None;
        self.wanted = None;
        self.loading = None;
        self.target = None;
    }

    /// The bus shown, drawn at `w` x `h` pixels into a texture of its own and read back.
    pub fn photograph(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<image::RgbaImage> {
        self.shown.as_ref()?;
        let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bus photo"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.render(renderer, &view, w, h, None);
        let stride = (w * 4).div_ceil(256) * 256;
        let buf = renderer.device.create_buffer(&wgpu::BufferDescriptor { label: Some("bus photo"), size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = renderer.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        renderer.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buf.slice(..).get_mapped_range();
        let bgra = matches!(renderer.format(), wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm);
        let mut img = image::RgbaImage::new(w, h);
        for (y, row) in img.rows_mut().enumerate() {
            let line = &data[y * stride as usize..];
            for (x, px) in row.enumerate() {
                let i = x * 4;
                *px = if bgra { image::Rgba([line[i + 2], line[i + 1], line[i], 255]) } else { image::Rgba([line[i], line[i + 1], line[i + 2], 255]) };
            }
        }
        Some(img)
    }

    /// Show this (a bus, its paint, the time and weather to light it with).
    pub fn want(&mut self, look: Look) {
        if self.wanted.as_ref() != Some(&look) {
            self.wanted = Some(look);
            self.failed = None;
            self.error = None;
        }
    }

    /// Put these bus options (variable, value) on the bus, over its livery's: the bus shown is
    /// made anew with them, as the game makes it (see `made`) - its picture stays meanwhile.
    pub fn dress(&mut self, options: Vec<(String, f32)>) {
        self.options = options;
    }

    /// Draw the bus's destination displays in the display font `font` (None: its own), as the
    /// game will (`--display-font`): the bus shown takes it at once. (The desktop launcher's
    /// bus step does not ask for it yet.)
    #[allow(dead_code)]
    pub fn letter(&mut self, font: Option<String>) {
        self.letters = font;
    }

    /// The bus options the bus shown wears (None: no bus shown).
    pub fn dressed(&self) -> Option<&[(String, f32)]> {
        self.shown.as_ref().map(|s| s.options.as_slice())
    }

    fn fail(&mut self, look: Look, error: String) {
        self.error = Some(error);
        self.failed = Some(look);
        // A failed selection must not expose the previous bus through any preview API.
        self.shown = None;
        self.target = None;
        self.loading = None;
        self.opening = None;
    }

    /// The mouse dragged over the empty part of the window (degrees), or turned the wheel.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if self.view != PreviewView::Outside {
            self.inside_look.0 += dx * 0.35;
            self.inside_look.1 = (self.inside_look.1 - dy * 0.25).clamp(-85.0, 85.0);
            self.dirty = true;
            self.idle = 0.0;
            return;
        }
        self.yaw_to += dx * 0.35;
        self.pitch_to = (self.pitch_to + dy * 0.25).clamp(-4.0, 55.0);
        self.idle = 0.0;
    }
    pub fn zoom_by(&mut self, k: f32) {
        if self.view != PreviewView::Outside {
            self.inside_zoom = (self.inside_zoom * k).clamp(0.5, 1.5);
            self.dirty = true;
            self.idle = 0.0;
            return;
        }
        self.zoom_to = (self.zoom_to * k).clamp(0.55, 2.2);
        self.idle = 0.0;
    }

    pub fn can_view(&self, view: PreviewView) -> bool {
        self.shown.as_ref().is_some_and(|s| s.vehicle.is_some() && (view == PreviewView::Outside || s.inside_camera(view, (0.0, 0.0), 1.0).is_some()))
    }

    pub fn views(&self) -> Vec<&'static str> {
        PreviewView::ALL.into_iter().filter(|v| self.can_view(*v)).map(PreviewView::name).collect()
    }

    pub fn set_view(&mut self, view: PreviewView) {
        if self.view != view && self.can_view(view) {
            self.view = view;
            self.inside_look = (0.0, 0.0);
            self.inside_zoom = 1.0;
            self.idle = 0.0;
            self.dirty = true;
        }
    }

    /// Per frame: start loading what is wanted, take over what finished loading, move the
    /// camera. Returns true when a new scene was put in place.
    pub fn update(&mut self, renderer: &Renderer, dt: f32) -> bool {
        let mut swapped = false;
        // the map of the bus being read is open: the worker goes on with the renderer's
        // prefetch of the bus's textures and meshes
        if let Some((worlds, prefetch)) = self.opening.as_ref() {
            match worlds.try_recv() {
                Ok(world) => {
                    let _ = prefetch.send(world.vehicle_prefetch(renderer));
                    self.opening = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.opening = None,
            }
        }
        // what finished loading
        if let Some((look, rx)) = self.loading.as_ref() {
            match rx.try_recv() {
                Ok(Ok(ready)) => {
                    self.loading = None;
                    self.shown = Some(self.place(renderer, ready));
                    if !self.can_view(self.view) {
                        self.view = PreviewView::Outside;
                    }
                    self.inside_look = (0.0, 0.0);
                    self.inside_zoom = 1.0;
                    self.error = None;
                    self.dirty = true;
                    swapped = true;
                }
                Ok(Err(e)) => {
                    let _ = std::fs::remove_file(placing_mark(self.studio));
                    log::warn!("showroom {}: {e}", look.bus);
                    self.fail(look.clone(), e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    let _ = std::fs::remove_file(placing_mark(self.studio));
                    self.fail(look.clone(), "The preview loader stopped before the bus was ready".into());
                }
            }
        }
        // what is wanted and not shown or loading
        if let Some(w) = self.wanted.clone() {
            let shown = self.shown.as_ref().map(|s| &s.look);
            let loading = self.loading.as_ref().map(|l| &l.0);
            if (shown != Some(&w) || self.reread) && loading != Some(&w) && self.loading.is_none() && self.failed.as_ref() != Some(&w) {
                // only the light changed: no need to read the bus again
                let same_bus = !self.reread && shown.map(|s| s.bus == w.bus && s.paint == w.paint && s.map == w.map && s.root == w.root).unwrap_or(false);
                if same_bus {
                    if let Some(s) = self.shown.as_mut() {
                        s.look = w.clone();
                        let args = args_for(&w);
                        (s.weather, s.sky.model) = crate::weather_setup::load_weather_run(&args);
                        setup_sky(&args, renderer, &mut s.scene, omsi_content::Envir::load(&args.root.join("envir.cfg")).ok().as_ref(), Some(&s.weather), &mut s.sky.clouds);
                        s.lighting = lighting_for(&args, &s.weather, &s.sky);
                    }
                    self.dirty = true;
                } else {
                    self.start_loading(w);
                }
            }
        }
        // the bus options changed: the bus shown made anew with them (a bus or livery being
        // read is made with them from the start)
        if let Some((_, rx)) = self.dressing.as_ref() {
            match rx.try_recv() {
                Ok(vehicle) => {
                    if let Some((options, _)) = self.dressing.take() {
                        self.put_on(vehicle, options);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    // (it could not be made: not tried again for these options)
                    if let (Some((options, _)), Some(s)) = (self.dressing.take(), self.shown.as_mut()) {
                        s.options = options;
                    }
                }
            }
        }
        if self.dressing.is_none() && self.loading.is_none() {
            if let Some((s, v)) = self.shown.as_ref().filter(|s| s.options != self.options).and_then(|s| Some((s, s.vehicle.as_ref()?))) {
                let parts: Vec<(Arc<omsi_sim::VehicleType>, bool)> = v.trailers.iter().map(|t| (t.ty.clone(), t.reversed)).collect();
                let (vt, scheme, root, clock) = (v.ty.clone(), s.scheme, s.look.root.clone(), start_clock(&args_for(&s.look)));
                let options = self.options.clone();
                let wear = options.clone();
                let (tx, rx) = channel();
                std::thread::spawn(move || {
                    let _ = tx.send(made(&root, &vt, Some(&parts), scheme, clock, &wear));
                });
                self.dressing = Some((options, rx));
            }
        }
        // another display font: the bus shown writes its displays in it
        if self.dressing.is_none() {
            if let Some(s) = self.shown.as_mut().filter(|s| s.letters != self.letters) {
                if let (Some(w), Some(v)) = (s.world.as_ref(), s.vehicle.as_mut()) {
                    fonts_on(w, v, self.letters.as_deref());
                    self.dirty = true;
                }
                s.letters = self.letters.clone();
            }
        }
        self.busy = self.loading.is_some();
        // camera
        self.idle += dt;
        if self.auto_turn && self.view == PreviewView::Outside && self.idle > 4.0 {
            self.yaw_to += dt * 6.0;
        }
        let k = 1.0 - (-dt / 0.18).exp();
        // still turning: the picture must follow
        if (self.yaw_to - self.yaw).abs() > 0.05 || (self.pitch_to - self.pitch).abs() > 0.05 || (self.zoom_to - self.zoom).abs() > 0.001 {
            self.dirty = true;
        }
        self.yaw += (self.yaw_to - self.yaw) * k;
        self.pitch += (self.pitch_to - self.pitch) * k;
        self.zoom += (self.zoom_to - self.zoom) * k;
        // (the stage moving moves the bus: drawn again too, which it was not while it eased)
        self.ease_stage(dt);
        swapped
    }

    fn start_loading(&mut self, look: Look) {
        self.reread = false;
        self.error = None;
        // Prevent a repeat hang on restart, but retry after an update (#1478, #1635).
        if preview_is_skipped(&omsi_launcher_lib::data_dir(), omsi_platform::VERSION, placing_kind(self.studio), &look.bus, std::process::id()) {
            self.fail(look, "The 3D preview of this bus stopped the launcher once, so it is left out".into());
            return;
        }
        let _ = std::fs::write(placing_mark(self.studio), format!("{}\n{}\n", look.bus.trim(), std::process::id()));
        let args = args_for(&look);
        let root = look.root.clone();
        let map_cfg = omsi_cfg::resolve_path(&root, &look.map);
        let date = start_clock(&args).date_code();
        let (tx, rx) = channel();
        // (the map is opened on the worker as well: on this thread it held the launcher's
        // frames up - a big map's, or any on a phone's storage, for a second and more)
        let (world_tx, world_rx) = channel();
        let (prefetch_tx, prefetch_rx) = channel::<scene::VehiclePrefetch>();
        let l2 = look.clone();
        let options = self.options.clone();
        std::thread::spawn(move || {
            let r = (|| -> Result<Ready> {
                let t0 = std::time::Instant::now();
                let world = Arc::new(scene::World::open(&root, &map_cfg, date)?);
                log::info!("showroom: {} opened in {:.2} s", map_cfg.display(), t0.elapsed().as_secs_f64());
                let _ = world_tx.send(world.clone());
                // (made by the launcher's thread, which has the renderer; gone with the
                // showroom when the device is given up meanwhile)
                let prefetch = prefetch_rx.recv().map_err(|_| anyhow!("the preview was let go of"))?;
                if l2.bus.is_empty() {
                    return Err(anyhow!("no bus"));
                }
                let path = player_bus_path(&root, &l2.bus)?;
                let vt = Arc::new(omsi_sim::VehicleType::load(&root, &path)?);
                let scheme = paint_scheme(&vt, Some(l2.paint.as_str()).filter(|p| !p.is_empty()));
                let vehicle = made(&root, &vt, None, scheme, start_clock(&args), &options);
                prefetch.prefetch(&vt, scheme);
                for t in &vehicle.trailers {
                    prefetch.prefetch(&t.ty, crate::spawn::part_scheme(&vt, scheme, &t.ty));
                }
                Ok(Ready { look: l2, world, vt, vehicle, scheme, options })
            })();
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        self.opening = Some((world_rx, prefetch_tx));
        self.loading = Some((look, rx));
    }

    fn place(&mut self, renderer: &Renderer, r: Ready) -> Shown {
        let t0 = std::time::Instant::now();
        let mut scene = renderer.new_scene();
        let args = args_for(&r.look);
        let (weather, model) = crate::weather_setup::load_weather_run(&args);
        let envir = omsi_content::Envir::load(&args.root.join("envir.cfg")).ok();
        let mut sky = crate::weather_setup::SkyState { clouds: None, model, look: args.session_settings };
        setup_sky(&args, renderer, &mut scene, envir.as_ref(), Some(&weather), &mut sky.clouds);
        add_floor(renderer, &mut scene);
        let world = r.world;
        // (the panes wear this weather's snow as the bus goes onto the GPU)
        world.settings.set_snow(weather.snow);
        let mut vehicle = r.vehicle;
        vehicle.position = DVec3::ZERO;
        vehicle.heading = 0.0;
        let render = world.add_vehicle(renderer, &mut scene, &r.vt, r.scheme);
        let trailers: Vec<scene::VehicleRender> = vehicle.trailers.iter().map(|t| world.add_vehicle_part(renderer, &mut scene, &t.ty, crate::spawn::part_scheme(&r.vt, r.scheme, &t.ty), &render)).collect();
        fonts_on(&world, &mut vehicle, self.letters.as_deref());
        vehicle.update(1.0 / 30.0);
        // the bus's size from its bounding box (with the rear section behind it)
        let bb = r.vt.def.bounding_box.unwrap_or([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let mut length = bb[1];
        let mut centre = glam::Vec3::new(bb[3], bb[4], bb[5]);
        let mut bus = bus_box(r.vt.def.bounding_box, r.vt.model_box(), bb);
        for t in &vehicle.trailers {
            let tb = t.ty.def.bounding_box.unwrap_or([2.5, 8.0, 3.0, 0.0, 0.0, 1.5]);
            let behind = (t.position - vehicle.position).truncate().length() as f32;
            let back = behind + tb[1] * 0.5;
            let total = bb[1] * 0.5 + back;
            centre.y = bb[4] + bb[1] * 0.5 - total * 0.5;
            length = total;
            // (the section stands straight behind at rest)
            let mut rear = bus_box(t.ty.def.bounding_box, t.ty.model_box(), tb);
            rear.centre += Vec3::new(0.0, -behind, (t.position.z - vehicle.position.z) as f32);
            bus = bus.with(rear);
        }
        let lighting = lighting_for(&args, &weather, &sky);
        log::info!("showroom: {} ({} meshes, {:.1} m long) placed in {:.2} s", r.look.bus, render.instances.len(), length, t0.elapsed().as_secs_f64());
        let _ = std::fs::remove_file(placing_mark(self.studio));
        Shown { look: r.look, scene, world: Some(world), vehicle: Some(vehicle), scheme: r.scheme, options: r.options, letters: self.letters.clone(), render: Some(render), trailers, vt: Some(r.vt.clone()), centre, length, bus, weather, sky, lighting }
    }

    /// The bus made anew with the bus options `options` takes the place of the one shown (the
    /// same type: its meshes on the GPU stay; one made for a bus no longer shown is let go).
    fn put_on(&mut self, mut vehicle: omsi_sim::VehicleInstance, options: Vec<(String, f32)>) {
        let Some(s) = self.shown.as_mut() else { return };
        if !s.vehicle.as_ref().is_some_and(|v| Arc::ptr_eq(&v.ty, &vehicle.ty) && v.trailers.len() == vehicle.trailers.len()) {
            return;
        }
        vehicle.position = DVec3::ZERO;
        vehicle.heading = 0.0;
        if let Some(w) = s.world.as_ref() {
            fonts_on(w, &mut vehicle, s.letters.as_deref());
        }
        vehicle.update(1.0 / 30.0);
        log::info!("showroom: {} made anew with {} bus option(s)", s.look.bus, options.len());
        s.vehicle = Some(vehicle);
        s.options = options;
        self.dirty = true;
    }

    /// The picture of the bus at `w` x `h` pixels, drawn again when something changed.
    pub fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView> {
        self.shown.as_ref()?;
        let (w, h) = (w.max(16), h.max(16));
        if self.target.as_ref().map(|t| (t.2, t.3) != (w, h)).unwrap_or(true) {
            let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bus preview"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.target = Some((tex, view, w, h));
            self.generation += 1;
            self.dirty = true;
        }
        if let Some((sw, sh)) = self.pending_second.take() {
            let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bus preview, second"),
                size: wgpu::Extent3d { width: sw, height: sh, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.second = Some((tex, view, sw, sh));
            self.second_generation += 1;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.drawn += 1;
            let view = self.target.as_ref().unwrap().1.clone();
            self.render(renderer, &view, w, h, None);
            if let (Some((_, v2, sw, sh)), Some(cam)) = (self.second.as_ref().map(|t| (0, t.1.clone(), t.2, t.3)), self.second_camera) {
                self.render(renderer, &v2, sw, sh, Some(cam));
            }
        }
        self.target.as_ref().map(|t| t.1.clone())
    }

    fn render(&mut self, renderer: &mut Renderer, target: &wgpu::TextureView, w: u32, h: u32, with: Option<Camera>) {
        let (yaw, pitch, zoom, stage, aim, studio) = (self.yaw, self.pitch, self.zoom, self.stage_now, self.aim, self.studio);
        let Some(s) = self.shown.as_mut() else { return };
        if let (Some(v), Some(r)) = (s.vehicle.as_mut(), s.render.as_mut()) {
            player::sync_vehicle_transforms(renderer, &mut s.scene, v, r, &mut s.trailers, self.view != PreviewView::Outside);
        }
        if let Some(cam) = s.inside_camera(self.view, self.inside_look, self.inside_zoom) {
            s.scene.overlays.clear();
            renderer.render(&mut s.scene, target, w, h, &cam, &s.lighting);
            return;
        }
        let aspect = w as f32 / h.max(1) as f32;
        let cam = match with.or(self.camera) {
            Some(c) => c,
            None if studio => studio_camera(s.centre, s.length, yaw, pitch, zoom, aim, aspect),
            None => frame(&s.bus, yaw, pitch, zoom, aspect, stage),
        };
        s.scene.overlays.clear();
        let _ = &s.weather;
        renderer.render(&mut s.scene, target, w, h, &cam, &s.lighting);
    }

    /// The picture's texture as last drawn (the desktop launcher's service reads it back).
    pub fn picture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.0)
    }

    /// The camera is still easing towards where the mouse turned it (or turning by itself):
    /// more pictures are coming.
    pub fn moving(&self) -> bool {
        self.dirty || (self.yaw_to - self.yaw).abs() > 0.05 || (self.pitch_to - self.pitch).abs() > 0.05 || (self.zoom_to - self.zoom).abs() > 0.001 || (self.auto_turn && self.view == PreviewView::Outside && self.idle > 4.0)
    }

    /// The last picture drawn, as RGBA rows (width, height, pixels) - read back from the
    /// graphics card for a launcher that shows it itself (the Swift one, see `apple_bridge`).
    #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
    pub fn pixels(&self, renderer: &Renderer) -> Option<(u32, u32, Vec<u8>)> {
        let (tex, _, w, h) = self.target.as_ref()?;
        let (w, h) = (*w, *h);
        // (rows of a copy are 256-byte aligned)
        let row = w * 4;
        let padded = row.div_ceil(256) * 256;
        let device = &renderer.device;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("bus preview read-back"), size: (padded * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bus preview read-back") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        renderer.queue.submit([enc.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().ok()?.ok()?;
        let data = slice.get_mapped_range();
        let bgra = matches!(renderer.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            let line = &data[y * padded as usize..y * padded as usize + row as usize];
            if bgra {
                for p in line.chunks_exact(4) {
                    out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
                }
            } else {
                out.extend_from_slice(line);
            }
        }
        drop(data);
        buffer.unmap();
        Some((w, h, out))
    }

    /// A bus is there to show.
    pub fn has_picture(&self) -> bool {
        self.shown.as_ref().map(|s| s.vehicle.is_some()).unwrap_or(false) && self.target.is_some()
    }
}

/// The bus of type `vt` as the game puts it down (`spawn_player`): its livery's variables and
/// the bus options `options` there for its scripts' `{init}`, both set again after it,
/// its coupled parts on (`parts`, else read as the game reads them), three steps of the
/// scripts, then the options once more and a step - as `--setvar` comes - so that a script
/// taking them over in its first frames (the NLC's `setvar.osc`) sees them too.
fn made(root: &Path, vt: &Arc<omsi_sim::VehicleType>, parts: Option<&[(Arc<omsi_sim::VehicleType>, bool)]>, scheme: Option<usize>, clock: omsi_sim::SimClock, options: &[(String, f32)]) -> omsi_sim::VehicleInstance {
    let mut host = omsi_sim::VehicleHost::new(clock);
    host.paint_scheme = Some(scheme);
    host.start_vars = options.to_vec();
    let mut vehicle = omsi_sim::VehicleInstance::new(vt.clone(), host);
    vehicle.apply_paint_vars(scheme);
    for (var, v) in options {
        vehicle.set_var(var, *v);
    }
    match parts {
        Some(parts) => {
            for (t, reversed) in parts {
                vehicle.attach_trailer_ex(t.clone(), *reversed);
            }
        }
        None => {
            load_coupled_parts(root, &mut vehicle);
        }
    }
    for _ in 0..3 {
        vehicle.update(1.0 / 30.0);
    }
    if !options.is_empty() {
        for (var, v) in options {
            vehicle.set_var(var, *v);
        }
        vehicle.update(1.0 / 30.0);
    }
    vehicle
}

/// The fonts of the bus's text displays (its rear sections' too), its destination displays in
/// the display font `letters` the player chose (None: their own).
fn fonts_on(world: &scene::World, vehicle: &mut omsi_sim::VehicleInstance, letters: Option<&str>) {
    vehicle.init_text_textures(&mut world.fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
    for t in vehicle.trailers.iter_mut() {
        t.init_text_textures(&mut world.fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
    }
    if let Some(font) = letters.map(str::trim).filter(|f| !f.is_empty()) {
        vehicle.apply_display_font(font, &mut world.fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn look(bus: &str) -> Look {
        Look { bus: bus.into(), ..Look::default() }
    }

    #[test]
    fn uses_the_authored_standard_driver_camera_and_passenger_camera() {
        let mut def = omsi_vehicle::Vehicle::default();
        def.cameras_driver = vec![omsi_vehicle::Camera { yaw: 10.0, ..Default::default() }, omsi_vehicle::Camera { yaw: 35.0, ..Default::default() }];
        def.camera_std = 1;
        def.cameras_pax = vec![omsi_vehicle::Camera { yaw: 180.0, ..Default::default() }];
        assert_eq!(authored_camera(&def, PreviewView::Driver).unwrap().yaw, 35.0);
        assert_eq!(authored_camera(&def, PreviewView::Passenger).unwrap().yaw, 180.0);
        def.camera_std = 99;
        assert_eq!(authored_camera(&def, PreviewView::Driver).unwrap().yaw, 10.0);
        assert!(authored_camera(&def, PreviewView::Outside).is_none());
        assert!(authored_camera(&omsi_vehicle::Vehicle::default(), PreviewView::Passenger).is_none());
    }

    #[test]
    fn looking_and_zooming_inside_preserves_the_exterior_orbit() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        showroom.view = PreviewView::Passenger;
        let exterior = (showroom.yaw_to, showroom.pitch_to, showroom.zoom_to);
        showroom.orbit(100.0, 100.0);
        showroom.zoom_by(0.8);
        assert_eq!((showroom.yaw_to, showroom.pitch_to, showroom.zoom_to), exterior);
        assert_eq!(showroom.inside_look, (35.0, -25.0));
        assert!((showroom.inside_zoom - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_view_needs_a_bus_and_its_camera() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        assert!(!showroom.can_view(PreviewView::Outside));
        assert!(showroom.views().is_empty());
        showroom.set_view(PreviewView::Driver);
        assert_eq!(showroom.view, PreviewView::Outside);
        assert_eq!(PreviewView::from_name("passenger"), Some(PreviewView::Passenger));
        assert_eq!(PreviewView::from_name("roof"), None);
    }

    #[test]
    fn failure_ends_loading_and_hides_the_picture() {
        // Avoid Drop's installation-wide placing marker in these self-contained tests.
        let mut showroom = ManuallyDrop::new(Showroom::new());
        let selected = look("unavailable.bus");
        showroom.want(selected.clone());
        let (_tx, rx) = channel();
        showroom.loading = Some((selected.clone(), rx));
        let (_world_tx, world_rx) = channel();
        let (prefetch_tx, _prefetch_rx) = channel();
        showroom.opening = Some((world_rx, prefetch_tx));

        showroom.fail(selected.clone(), "Cannot load the bus".into());

        assert_eq!(showroom.failed.as_ref(), Some(&selected));
        assert_eq!(showroom.error.as_deref(), Some("Cannot load the bus"));
        assert!(showroom.loading.is_none());
        assert!(showroom.opening.is_none());
        assert!(!showroom.has_picture());
        assert!(showroom.picture().is_none());
    }

    #[test]
    fn changed_selection_clears_failure_but_unchanged_selection_does_not_retry() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        let failed = look("unavailable.bus");
        showroom.want(failed.clone());
        showroom.fail(failed.clone(), "Cannot load the bus".into());

        showroom.want(failed.clone());
        assert_eq!(showroom.failed.as_ref(), Some(&failed));
        assert!(showroom.error.is_some());

        showroom.want(look("available.bus"));
        assert!(showroom.failed.is_none());
        assert!(showroom.error.is_none());

        showroom.want(failed.clone());
        assert_eq!(showroom.wanted.as_ref(), Some(&failed));
        assert!(showroom.failed.is_none());
        assert!(showroom.error.is_none());
    }
}

/// The light of the look's time and weather, with the sun's shadow under the bus. Always
/// the plain renderer, whatever the game's graphics setting: a preview is to be quick and
/// clear, not the game's picture (no enhanced exposure and glow, no weather effects).
fn lighting_for(args: &Args, weather: &omsi_content::weather::Weather, sky: &crate::weather_setup::SkyState) -> Lighting {
    let clock = start_clock(args);
    let envir = omsi_content::Envir::load(&args.root.join("envir.cfg")).ok();
    let daylight = omsi_sim::Daylight::compute(&clock, envir.as_ref());
    let mut l = weather_lighting(&daylight, weather, crate::weather_setup::cloud_drift_at(weather, clock.time), 0.0, true, sky);
    l.shadows = daylight.altitude_deg > 2.0;
    l.enhanced = false;
    l.classic = false;
    l.detail = false;
    l.wetness = 0.0;
    l.snow = 0.0;
    l.fog_density = 0.0;
    l
}

/// A round showroom floor under the bus: dark, matt, catching its shadow.
fn add_floor(renderer: &Renderer, scene: &mut Scene) {
    let n = 96;
    let r = 400.0f32;
    let mut positions = vec![glam::Vec3::ZERO];
    let mut normals = vec![glam::Vec3::Z];
    let mut uvs = vec![glam::Vec2::ZERO];
    for k in 0..n {
        let a = std::f32::consts::TAU * k as f32 / n as f32;
        positions.push(glam::Vec3::new(a.cos() * r, a.sin() * r, 0.0));
        normals.push(glam::Vec3::Z);
        uvs.push(glam::Vec2::new(a.cos(), a.sin()));
    }
    let mut indices = Vec::new();
    for k in 0..n {
        indices.extend([0u32, 1 + ((k + 1) % n) as u32, 1 + k as u32]);
    }
    let data = omsi_geometry::MeshData { positions, normals, uvs, ranges: vec![(0, indices.len() as u32, 0)], indices, one_sided: false };
    let mesh = renderer.add_mesh(scene, &data);
    let mat = renderer.add_material(scene, None, omsi_render::AlphaMode::Opaque, [0.12, 0.125, 0.135, 1.0], false);
    renderer.add_instance(scene, mesh, DVec3::new(0.0, 0.0, -0.005), glam::Mat4::IDENTITY, vec![mat]);
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn old_preview_failures_do_not_block_a_new_version() {
        let dir = std::env::temp_dir().join(format!("openomsi-preview-old-failures-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bus = "Vehicles/test/bus.bus";
        std::fs::write(dir.join("showroom-skip.txt"), bus).unwrap();
        std::fs::write(dir.join("showroom-placing.txt"), bus).unwrap();
        std::fs::write(preview_guard_path(&dir, "skip", "0.1.1"), bus).unwrap();
        std::fs::write(preview_guard_path(&dir, "placing", "0.1.1"), bus).unwrap();

        assert!(!preview_is_skipped(&dir, "0.1.2", "placing", bus, 0));
        assert!(!preview_guard_path(&dir, "skip", "0.1.2").exists());
        assert!(preview_guard_path(&dir, "placing", "0.1.1").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unfinished_preview_is_still_blocked_in_the_same_version() {
        let dir = std::env::temp_dir().join(format!("openomsi-preview-current-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bus = "Vehicles/test/bus.bus";
        std::fs::write(preview_guard_path(&dir, "placing", "0.1.1"), bus).unwrap();

        assert!(preview_is_skipped(&dir, "0.1.1", "placing", bus, 0));
        assert!(!preview_guard_path(&dir, "placing", "0.1.1").exists());
        assert!(preview_is_skipped(&dir, "0.1.1", "placing", bus, 0));
        assert!(!preview_is_skipped(&dir, "0.1.1", "placing", "Vehicles/test/other.bus", 0));
        assert!(!preview_is_skipped(&dir, "0.1.2", "placing", bus, 0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_bus_hung_only_when_another_process_was_placing_it() {
        let bus = "Vehicles\\MAN_SD200\\SD200.bus";
        assert!(hung(&format!("{bus}\n41\n"), bus, 42), "a service before this one");
        assert!(hung(bus, bus, 42), "a mark of before the process was in it");
        assert!(!hung(&format!("{bus}\n42\n"), bus, 42), "this process's other showroom, placing it now");
        assert!(!hung("Vehicles\\MAN_NL202\\NL202.bus\n41\n", bus, 42), "another bus");
        assert!(!hung("", bus, 42));
    }

    #[test]
    fn the_stage_is_taken_at_once_after_a_while_away_and_glided_to_while_shown() {
        let mut s = std::mem::ManuallyDrop::new(Showroom::new());
        let (a, b) = ([0.3, 0.1, 0.98, 0.8], [0.25, 0.1, 0.98, 0.85]);
        s.set_stage(a);
        assert_eq!(s.stage_now, a, "the first time: at once");
        s.dirty = false;
        s.ease_stage(1.0 / 60.0);
        assert!(!s.dirty, "standing still: nothing to draw again");
        s.set_stage(b);
        assert_eq!(s.stage_now, a, "shown: it glides");
        s.ease_stage(1.0 / 60.0);
        assert!(s.dirty && s.stage_now[0] < a[0] && s.stage_now[0] > b[0]);
        for _ in 0..240 {
            s.set_stage(b);
            s.ease_stage(1.0 / 60.0);
        }
        assert_eq!(s.stage_now, b);
        // (away a while: back, the bus stands where it should)
        s.ease_stage(0.5);
        s.set_stage(a);
        assert_eq!(s.stage_now, a);
    }
}

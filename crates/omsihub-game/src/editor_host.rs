//! What the livery studio is drawn in (`EditorHost`), now that the game draws no launcher of its
//! own: the interface kit's frame, the launcher's choice and settings as the studio reads them
//! (`EditorState`), the bus company it may paint for, and the GPU it draws on. The launcher
//! service runs one on a thread of its own (`service::studio`) and sends the desktop launcher
//! its pictures.
//!
//! The studio was a page of Omsi-Hub's game-drawn launcher and still reads like one (`l.ui`,
//! `l.state`, `l.livery`, `l.go(Page::Company)`): this is what is left of that launcher for it.

use crate::livery::LiveryView;
use crate::theme::*;
use crate::ui::{Modifiers, Ui};
use omsi_launcher_lib as core;
use omsi_render::Renderer;
use omsi_ui::paint::Align;
use omsi_ui::{Draw, Rect, Weight};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

/// Where the studio is: on its page, or left to where it was opened from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    /// The livery studio itself.
    Livery,
    /// Left for the bus company it painted for.
    Company,
    /// Left for the bus step (or the editor's page it was opened from).
    Drive,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) type Clipboard = arboard::Clipboard;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) struct Clipboard;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Clipboard {
    pub fn get_text(&mut self) -> Result<String, ()> {
        Err(())
    }
    pub fn set_text(&mut self, _t: String) -> Result<(), ()> {
        Ok(())
    }
}

/// The bus company the studio may paint for (its file as it was read when the studio opened).
#[derive(Default)]
pub struct CompanyHold {
    pub company: Option<core::company::Company>,
}

/// The launcher's choice as the studio reads it: the bus and its livery, and the map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub bus: String,
    pub paint: String,
    pub map: String,
}

/// What the studio knows of the launcher: the game's folder, the installed maps and buses, the
/// choice, the settings, the buses' options and what to say in the status line.
pub struct EditorState {
    pub config: core::Config,
    pub maps: Vec<core::MapInfo>,
    pub vehicles: Vec<core::VehicleInfo>,
    pub choice: Choice,
    pub settings: serde_json::Value,
    pub bus_options: crate::busoptions::Options,
    /// The status line: its text, whether it is an error, since when.
    pub status: (String, bool, Instant),
    /// The maps and buses being read (on a worker, once).
    content: Option<Receiver<(Vec<core::MapInfo>, Vec<core::VehicleInfo>)>>,
}

impl EditorState {
    fn new() -> EditorState {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("studio content".into())
            .spawn(move || {
                let maps = core::list_maps().unwrap_or_default();
                let vehicles = core::list_vehicles().unwrap_or_default();
                let _ = tx.send((maps, vehicles));
            })
            .ok();
        let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
        EditorState { config: core::load_config(), maps: Vec::new(), vehicles: Vec::new(), choice: Choice::default(), settings, bus_options: crate::busoptions::Options::load(), status: (String::new(), false, Instant::now()), content: Some(rx) }
    }

    /// The maps and buses once they are read.
    pub fn update(&mut self, _dt: f32) {
        if let Some(rx) = self.content.as_ref() {
            if let Ok((maps, vehicles)) = rx.try_recv() {
                (self.maps, self.vehicles) = (maps, vehicles);
                self.content = None;
            }
        }
    }

    /// A game runs: the studio does not write into the game then. (The desktop launcher steps
    /// aside while one runs, and its studio with it.)
    pub fn in_game(&self) -> bool {
        false
    }

    /// The choice changed (the bus step's livery): the page hears of it with the next picture
    /// (`service::studio`), and keeps it.
    pub fn touched(&mut self) {}

    /// A bus the company's planning gives the duty (the studio then leaves the bus step's
    /// livery alone): none here, the page keeps the duty.
    pub fn planned_bus(&self) -> Option<()> {
        None
    }

    pub fn set_status(&mut self, text: impl Into<String>, err: bool) {
        self.status = (text.into(), err, Instant::now());
    }
}

/// The livery studio's frame and all it is drawn with.
pub struct EditorHost {
    pub ui: Ui,
    pub state: EditorState,
    pub livery: LiveryView,
    pub company: CompanyHold,
    pub page: Page,
    pub renderer: Option<Renderer>,
    pub gpu: Option<omsi_ui::Gpu>,
    /// The device, queue and format the interface is drawn with (the renderer's).
    pub dev: Option<(wgpu::Device, wgpu::Queue, wgpu::TextureFormat)>,
    pub clipboard: Option<Clipboard>,
    pub modifiers: Modifiers,
    /// The key pressed this frame, as the keyboard has it (the studio's shortcuts: Ctrl+Z, 1-6,
    /// F, E ...), gone after the frame.
    pub raw_key: Option<winit::keyboard::KeyCode>,
    pub last: Instant,
    pub last_input: Instant,
}

impl EditorHost {
    /// The studio's host, drawing with `renderer` (the launcher service's own for the studio).
    pub fn new(renderer: Renderer) -> EditorHost {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let clipboard = arboard::Clipboard::new().ok();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let clipboard = Some(Clipboard);
        let mut h = EditorHost {
            ui: Ui::new(),
            state: EditorState::new(),
            livery: LiveryView::default(),
            company: CompanyHold::default(),
            page: Page::Drive,
            renderer: None,
            gpu: None,
            dev: None,
            clipboard,
            modifiers: Modifiers::default(),
            raw_key: None,
            last: Instant::now(),
            last_input: Instant::now(),
        };
        h.gpu = Some(omsi_ui::Gpu::new(&renderer.device, renderer.format(), 4, h.ui.atlas.size));
        h.dev = Some((renderer.device.clone(), renderer.queue.clone(), renderer.format()));
        h.renderer = Some(renderer);
        h
    }

    pub fn go(&mut self, p: Page) {
        self.page = p;
    }

    /// The size the player set for the launcher (`launcher_scale`, 1 = the system's).
    pub fn own_scale(&self) -> f64 {
        core::launcher_scale(self.state.settings.get("launcher_scale").and_then(|v| v.as_f64()))
    }

    /// The look the settings ask for (`dark_mode`), at the frame's start.
    pub fn look(&mut self) {
        let dark = self.state.settings.get("dark_mode").and_then(|v| v.as_bool()).unwrap_or(false);
        crate::accent::set_dark(dark);
        self.ui.light = !dark;
        self.ui.lighten.set_ground(light_ground(crate::accent::chosen()));
    }

    /// The status line at the bottom, as the launcher drew it: on a dark pill, an error until
    /// the next word, anything else for a few seconds.
    pub fn status_bar(&mut self) {
        let (text, err, at) = self.state.status.clone();
        if text.is_empty() {
            return;
        }
        let fade = if err { 1.0 } else { (1.0 - (at.elapsed().as_secs_f32() - 6.0) / 1.5).clamp(0.0, 1.0) };
        if fade <= 0.0 {
            return;
        }
        let size = self.ui.size;
        let first = text.lines().next().unwrap_or("").to_string();
        let w = (self.ui.width(&first, 12.0, Weight::Medium) + 28.0).min(size.x * 0.6);
        let r = Rect::new(22.0, size.y - 29.0, w, 23.0);
        self.ui.p().rounded(r, r.h * 0.5, ON_MAP.alpha(fade));
        let c = if err { DANGER.lighten(0.2) } else { TEXT_SOFT };
        self.ui.text_in(&first, Rect::new(r.x + 14.0, r.y, r.w - 22.0, r.h), 12.0, Weight::Medium, c.alpha(fade), Align::Left);
        self.ui.tooltip(r, &text);
        if !err {
            self.ui.keep_moving();
        }
    }

    /// The frame drawn into a texture and read back.
    pub fn shot_image(&mut self, w: u32, h: u32, layers: &[omsi_ui::Layer], draws: &[Draw], bg: wgpu::Color) -> Option<image::RgbaImage> {
        let (device, queue, format) = self.dev.clone()?;
        let gpu = self.gpu.as_mut()?;
        let tex = device.create_texture(&wgpu::TextureDescriptor { label: Some("livery studio"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let mut enc = device.create_command_encoder(&Default::default());
        gpu.render(&device, &queue, &mut enc, &view, (w, h), Some(bg), layers, draws);
        let stride = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let data = buf.slice(..).get_mapped_range();
        let bgra = matches!(format, wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm);
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = (y * stride + x * 4) as usize;
                let (r_, g, b) = if bgra { (data[i + 2], data[i + 1], data[i]) } else { (data[i], data[i + 1], data[i + 2]) };
                img.put_pixel(x, y, image::Rgba([r_, g, b, 255]));
            }
        }
        Some(img)
    }

    /// Do something to the bus company the studio paints for, and keep it in its file; its
    /// refusal is said in the status line. None: refused, or no company.
    pub fn company_act<T>(&mut self, f: impl FnOnce(&mut core::company::Company) -> Result<T, &'static str>) -> Option<T> {
        let c = self.company.company.as_mut()?;
        match f(c) {
            Ok(v) => {
                if let Err(e) = core::company::store::save(&core::data_dir(), c) {
                    self.state.set_status(format!("{e:#}"), true);
                }
                Some(v)
            }
            Err(e) => {
                self.state.set_status(omsi_ui::tr(e).into_owned(), true);
                None
            }
        }
    }
}

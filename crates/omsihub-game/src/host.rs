//! What Omsi-Hub's pieces here ask of the game program they run in: the program (omsi-app)
//! installs a `Host` once, at its start (`set`). Everything here is what only the game has -
//! the bus drawn with the game's scene (`Showroom`, the livery studio's and the bus photos'),
//! the city map read the way the navigator reads it, the interface's language and machine
//! translation, the player's bus as the game spawns it - so none of the game's modules is
//! needed here.
//!
//! Without a host (the tests here) every call answers as if nothing were there.

use glam::{DVec3, Vec3};
use omsi_render::Renderer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// The game program's side (see the module).
pub trait Host: Send + Sync {
    /// Show the interface in `code` (ENG, DEU ...).
    fn ui_language(&self, _code: &str) {}
    /// Texts machine translation must leave alone (names of maps, buses, lines).
    fn mt_protect(&self, _texts: &mut dyn Iterator<Item = &str>) {}
    /// The colour the bus's picture stands on where nothing is drawn.
    fn window_background(&self) -> wgpu::Color {
        wgpu::Color { r: 0.0027, g: 0.0037, b: 0.0091, a: 1.0 }
    }
    /// The bus drawn with the game's scene, as the bus step shows it (the program's
    /// `Showroom::new`): the livery studio's, with a camera of its own.
    fn new_showroom(&self) -> Box<dyn Showroom> {
        Box::new(NoShowroom)
    }
    /// The bus drawn with the game's scene for the bus photos: from the same front corner every
    /// time, nothing eased or turned (the program's `Showroom::studio`).
    fn new_studio(&self) -> Box<dyn Showroom> {
        Box::new(NoShowroom)
    }
    /// The city map of the map whose tiles are `tiles` (x, y, file), read the way the
    /// navigator reads it: its roads, lane network, objects and bus stops.
    fn city_map(&self, _root: &Path, _tiles: &[(i32, i32, PathBuf)], _chrono: &[PathBuf]) -> CityMap {
        CityMap::default()
    }
    /// A line thinned to within `tol` metres (the navigator's).
    fn simplify(&self, pts: &[Vec3], _tol: f32) -> Vec<Vec3> {
        pts.to_vec()
    }
    /// The vehicle file the game spawns for `bus` (an articulated bus's rear section: the
    /// whole bus).
    fn player_bus_path(&self, root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
        Ok(omsi_cfg::resolve_path(root, bus))
    }
    /// The `[couple_back]` chain behind `vehicle`, read and attached as the game does it.
    fn load_coupled_parts(&self, _root: &Path, _vehicle: &mut omsi_sim::VehicleInstance) -> Vec<Arc<omsi_sim::VehicleType>> {
        Vec::new()
    }
}

/// The bus the game program draws (its `launcher::showroom::Showroom`), as the livery studio
/// and the bus photos use it.
pub trait Showroom {
    /// Advance it: what was read is placed, what moves moves. True when the picture changed.
    fn update(&mut self, renderer: &Renderer, dt: f32) -> bool;
    /// The picture drawn at `w` x `h` pixels when something changed.
    fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView>;
    fn has_picture(&self) -> bool;
    /// Bumped whenever the picture's texture was made anew.
    fn generation(&self) -> u64;
    /// Why the bus could not be shown.
    fn error(&self) -> Option<String> {
        None
    }
    /// Show this look (a bus, its paint, the time and weather to light it with).
    fn want(&mut self, _look: crate::showroom::Look) {}
    /// `look` is in place, ready to be drawn.
    fn shows(&self, _look: &crate::showroom::Look) -> bool {
        false
    }
    /// `look` will not be shown: the bus did not load, or the map did not open.
    fn gave_up(&self, _look: &crate::showroom::Look) -> bool {
        true
    }
    /// Put these bus options (variable, value) on the bus, over its livery's.
    fn dress(&mut self, _options: Vec<(String, f32)>) {}
    fn dressed(&self) -> Option<Vec<(String, f32)>> {
        None
    }
    /// The files of bus `bus` changed (a livery saved into the game): read it again.
    fn reread(&mut self, _bus: &str) {}
    /// Let go of the bus shown and its scene.
    fn forget(&mut self) {}
    /// Draw the picture again (a texture of the bus changed).
    fn touch(&mut self) {}
    /// Draw the bus with this camera instead of the orbit (None: the orbit again).
    fn set_camera(&mut self, _cam: Option<omsi_render::Camera>) {}
    /// A second picture with a camera of its own, drawn with the first.
    fn set_second(&mut self, _cam: Option<(omsi_render::Camera, u32, u32)>) {}
    fn second_view(&self) -> Option<(wgpu::TextureView, u32, u32)> {
        None
    }
    fn second_generation(&self) -> u64 {
        0
    }
    /// The bus shown, its scene and type (None while none is placed).
    fn parts(&mut self) -> Option<crate::showroom::Parts<'_>> {
        None
    }
    /// The bus shown, drawn at `w` x `h` pixels into a picture of its own and read back.
    fn photograph(&mut self, _renderer: &mut Renderer, _w: u32, _h: u32) -> Option<image::RgbaImage> {
        None
    }
}

/// A showroom that shows nothing (no host).
struct NoShowroom;
impl Showroom for NoShowroom {
    fn update(&mut self, _renderer: &Renderer, _dt: f32) -> bool {
        false
    }
    fn preview(&mut self, _renderer: &mut Renderer, _w: u32, _h: u32) -> Option<wgpu::TextureView> {
        None
    }
    fn has_picture(&self) -> bool {
        false
    }
    fn generation(&self) -> u64 {
        0
    }
}

/// A carriageway of the city map: points in world metres.
pub struct MapRoad {
    pub points: Vec<DVec3>,
    pub width: f32,
    /// A road cars drive far along: drawn brighter.
    pub main: bool,
}

/// A map as the city map draws it (see `Host::city_map`).
#[derive(Default)]
pub struct CityMap {
    pub roads: Vec<MapRoad>,
    /// The linked network behind the roads (a trip's road pieces are lanes of it).
    pub net: omsi_sim::traffic::Network,
    /// Every placed object's world place by id.
    pub places: HashMap<i64, DVec3>,
    /// The map's bus stops (the line editor offers them).
    pub stops: Vec<omsi_scene::MapStop>,
}

// (an install-once hook: the program's side)
static HOST: OnceLock<Box<dyn Host>> = OnceLock::new();

/// Install the program's side (once; later calls are ignored).
pub fn set(host: Box<dyn Host>) {
    let _ = HOST.set(host);
}

struct NoHost;
impl Host for NoHost {}

/// The installed host (or one that has nothing).
pub fn host() -> &'static dyn Host {
    static NONE: NoHost = NoHost;
    HOST.get().map(|h| h.as_ref()).unwrap_or(&NONE)
}

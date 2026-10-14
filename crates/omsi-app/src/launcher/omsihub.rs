//! Omsi-Hub: the game program's side of `omsihub-game` (`omsihub_game::host::Host`), installed
//! with the launcher's model (`launcher::install`): the bus drawn with the game's scene for the
//! livery studio and the bus photos (`showroom`), the city map read the way the navigator reads
//! it (with the map's bus stops, for the line editor), the interface's language and machine
//! translation, and the player's bus as the game spawns it.

use super::showroom;
use omsi_render::Renderer;
use omsihub_game::host::{CityMap, Host, MapRoad};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) fn install() {
    omsihub_game::host::set(Box::new(HubHost));
}

struct HubHost;

impl Host for HubHost {
    fn ui_language(&self, code: &str) {
        crate::ui_language(code);
    }
    fn mt_protect(&self, texts: &mut dyn Iterator<Item = &str>) {
        crate::mt::protect(texts);
    }
    fn new_showroom(&self) -> Box<dyn omsihub_game::host::Showroom> {
        Box::new(showroom::Showroom::new())
    }
    fn new_studio(&self) -> Box<dyn omsihub_game::host::Showroom> {
        Box::new(showroom::Showroom::studio())
    }
    fn city_map(&self, root: &Path, tiles: &[(i32, i32, PathBuf)], chrono: &[PathBuf]) -> CityMap {
        let crate::scene::NavigationMap { lanes, road_surfaces, places, signs: _, stops } = crate::scene::navigation_map_of(root, tiles, chrono);
        let (roads, net) = crate::navigator::city_roads(lanes, &road_surfaces);
        CityMap { roads: roads.into_iter().map(|r| MapRoad { points: r.points, width: r.width, main: r.main }).collect(), net, places: places.into_iter().collect(), stops }
    }
    fn simplify(&self, pts: &[glam::Vec3], tol: f32) -> Vec<glam::Vec3> {
        crate::navigator::simplify(pts, tol)
    }
    fn player_bus_path(&self, root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
        crate::spawn::player_bus_path(root, bus)
    }
    fn load_coupled_parts(&self, root: &Path, vehicle: &mut omsi_sim::VehicleInstance) -> Vec<Arc<omsi_sim::VehicleType>> {
        crate::spawn::load_coupled_parts(root, vehicle)
    }
}

impl omsihub_game::host::Showroom for showroom::Showroom {
    fn update(&mut self, renderer: &Renderer, dt: f32) -> bool {
        showroom::Showroom::update(self, renderer, dt)
    }
    fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView> {
        showroom::Showroom::preview(self, renderer, w, h)
    }
    fn has_picture(&self) -> bool {
        showroom::Showroom::has_picture(self)
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn error(&self) -> Option<String> {
        self.error.clone()
    }
    fn want(&mut self, look: showroom::Look) {
        showroom::Showroom::want(self, look)
    }
    fn shows(&self, look: &showroom::Look) -> bool {
        showroom::Showroom::shows(self, look)
    }
    fn gave_up(&self, look: &showroom::Look) -> bool {
        showroom::Showroom::gave_up(self, look)
    }
    fn dress(&mut self, options: Vec<(String, f32)>) {
        showroom::Showroom::dress(self, options)
    }
    fn dressed(&self) -> Option<Vec<(String, f32)>> {
        showroom::Showroom::dressed(self).map(<[_]>::to_vec)
    }
    fn reread(&mut self, bus: &str) {
        showroom::Showroom::reread(self, bus)
    }
    fn forget(&mut self) {
        showroom::Showroom::forget(self)
    }
    fn touch(&mut self) {
        showroom::Showroom::touch(self)
    }
    fn set_camera(&mut self, cam: Option<omsi_render::Camera>) {
        showroom::Showroom::set_camera(self, cam)
    }
    fn set_second(&mut self, cam: Option<(omsi_render::Camera, u32, u32)>) {
        showroom::Showroom::set_second(self, cam)
    }
    fn second_view(&self) -> Option<(wgpu::TextureView, u32, u32)> {
        showroom::Showroom::second_view(self)
    }
    fn second_generation(&self) -> u64 {
        self.second_generation
    }
    fn parts(&mut self) -> Option<showroom::Parts<'_>> {
        showroom::Showroom::parts(self)
    }
    fn photograph(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<image::RgbaImage> {
        showroom::Showroom::photograph(self, renderer, w, h)
    }
}

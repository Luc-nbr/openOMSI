//! The main pass's pipelines for each of the three ways the scene is drawn: vanilla into
//! the target, enhanced into the high-range picture, and vanilla with reflections.

use super::coronas::{Coronas, Snow, ADDITIVE, ALPHA_BLEND, SCREEN};
use super::scene::SceneBase;
use super::sky::SkyBase;
use crate::*;

/// What the main pass's pipelines are made from.
pub(crate) struct PassKit<'a> {
    pub device: &'a wgpu::Device,
    pub scene: &'a SceneBase,
    pub coronas: &'a Coronas,
    pub snow: &'a Snow,
    pub sky: &'a SkyBase,
    pub msaa: u32,
}

/// The fragment entry points (and the coronas' blending) of one way of drawing.
struct PassShaders<'a> {
    scene: &'a str,
    corona: &'a str,
    corona_blend: wgpu::BlendState,
    smoke: &'a str,
    snow: &'a str,
    sky: &'a str,
}

pub(crate) struct Passes {
    pub pass: PassPipelines,
    pub hdr_pass: Option<PassPipelines>,
    pub sky_mirror_pipeline: Option<wgpu::RenderPipeline>,
    pub reflection_pass: Option<PassPipelines>,
    pub leave_out_enhanced: bool,
}

/// Decide before making shader modules and cloud/probe resources, not only before the
/// draw pipelines: WGSL validation still processes unused entry points, and the probe's
/// unused pipelines would invoke the native shader compiler.
pub(crate) fn leave_out_enhanced(options: &RenderOptions, adapter_name: &str) -> bool {
    options.preview_only
        || sixteen_texture_units()
        || (options.no_enhanced
            && (cfg!(target_os = "android")
                || adapter_name.to_ascii_lowercase().contains("opengl")
                || gl_backend()))
}

impl PassKit<'_> {
    fn pass(&self, f: wgpu::TextureFormat, fs: PassShaders) -> PassPipelines {
        let device = self.device;
        PassPipelines {
            pipelines: self.scene.pipelines(device, f, fs.scene, self.msaa),
            rain_pipelines: self.scene.pipelines(device, f, fs.scene, 1),
            corona_pipeline: self.coronas.pipeline(device, f, fs.corona, fs.corona_blend, self.msaa),
            smoke_pipeline: self.coronas.pipeline(device, f, fs.smoke, ALPHA_BLEND, self.msaa),
            snow_pipeline: (!basic_pipelines()).then(|| optional_pipeline(device, "the snowfall", || self.snow.pipeline(device, f, fs.snow, self.msaa))).flatten(),
            sky_pipeline: self.sky.pipeline(device, f, fs.sky, self.msaa),
        }
    }

    pub(crate) fn build(&self, format: wgpu::TextureFormat, hdr_format: wgpu::TextureFormat, leave_out_enhanced: bool) -> Passes {
        let pass = self.pass(format, PassShaders { scene: "fs_main", corona: "fs_main", corona_blend: SCREEN, smoke: "fs_smoke", snow: "fs_snow", sky: "fs_main" });
        let sky_mirror_pipeline = (!leave_out_enhanced).then(|| self.sky.pipeline(self.device, format, "fs_enhanced_mirror", self.msaa));
        let hdr_pass = (!leave_out_enhanced).then(|| {
            self.pass(hdr_format, PassShaders { scene: "fs_enhanced", corona: "fs_enhanced", corona_blend: ADDITIVE, smoke: "fs_smoke_enhanced", snow: "fs_snow_enhanced", sky: "fs_enhanced" })
        });
        let reflection_pass = (!leave_out_enhanced && !GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed)).then(|| {
            self.pass(hdr_format, PassShaders { scene: "fs_vanilla_reflections", corona: "fs_main", corona_blend: SCREEN, smoke: "fs_smoke", snow: "fs_snow", sky: "fs_main" })
        });
        Passes { pass, hdr_pass, sky_mirror_pipeline, reflection_pass, leave_out_enhanced }
    }
}

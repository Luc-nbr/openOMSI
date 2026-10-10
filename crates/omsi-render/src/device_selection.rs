//! GPU array path selection for downlevel/ANGLE devices during renderer creation.
use super::{ArrayPath, ARRAY_PATH};

pub(super) fn select_array_path(adapter: &wgpu::Adapter, limits: &wgpu::Limits, info: &wgpu::AdapterInfo) {
    // the per-draw arrays as storage buffers where the device reads them in a vertex
    // shader (three there, two lights arrays in a fragment shader), else as textures;
    // OMSI_GPU_ARRAYS=textures|nostorage takes those paths on any device
    let downlevel = adapter.get_downlevel_capabilities().flags;
    let storage = limits.max_storage_buffers_per_shader_stage;
    let path = match omsi_cfg::flags::OMSI_GPU_ARRAYS.var() {
        Some("textures") => ArrayPath::VertexTextures,
        Some("nostorage") => ArrayPath::NoStorage,
        _ if !downlevel.contains(wgpu::DownlevelFlags::FRAGMENT_STORAGE) || storage < 2 => ArrayPath::NoStorage,
        _ if !downlevel.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) || storage < 3 => ArrayPath::VertexTextures,
        // (the draw list and the lamps' grid are arrays of u32, 4 bytes, which such a device
        // cannot bind as storage buffers: ANGLE on Vulkan, an Exynos' Xclipse, failed the
        // shadow pipeline with "a size that is a multiple of 16 bytes", #1857)
        _ if !downlevel.contains(wgpu::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED) => ArrayPath::NoStorage,
        _ => ArrayPath::Storage,
    };
    ARRAY_PATH.store(path as u8, std::sync::atomic::Ordering::Relaxed);
    match path {
        ArrayPath::Storage => {}
        ArrayPath::VertexTextures => log::warn!("{}: no storage buffers in vertex shaders; the scene's arrays are read from textures", info.name),
        ArrayPath::NoStorage => log::warn!("{}: no storage buffers; the scene's arrays are read from textures and the lamps light no pixels of their own", info.name),
    }
}

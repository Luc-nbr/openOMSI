//! Pictures of the pause menu with the game's own lists open in the sheet beside the panel
//! (Omsi-Hub's menu, `shell::hub`), drawn as the game draws them, for a look by eye: they need
//! a graphics adapter and are left out unless asked for -
//! `cargo test -p omsi-app --lib shell::preview -- --ignored`, written beside
//! `OMSI_UI_PREVIEW` (its folder) or into `target/`.

use crate::game_lists::ListKind;
use omsi_ui::{Draw, Gpu};

/// The menu of `app` as it is now, in a window of `w` x `h`, written to `name` over a stand-in
/// for the street.
fn picture(app: &mut crate::App, name: &str, (w, h): (u32, u32)) {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut gpu = Gpu::new(&device, format, 1, app.shell.ui.atlas.size);
    // (the wordmark on the card first: the menu draws it where the game has it)
    let key = omsihub_game::pause_menu::wordmark_key(1.0);
    let (lw, lh, px) = omsihub_game::pause_menu::wordmark_pixels(key).expect("wordmark");
    app.shell.logo_tex = Some(gpu.add_image(&device, &queue, lw, lh, &px));
    // (twice: the eases settle)
    for _ in 0..2 {
        app.draw_pause(w as f32, h as f32, 1.0, 1.0);
    }
    let sh = &mut app.shell;
    let d = sh.drawn.as_ref().expect("drawn");
    sh.ui.atlas.mark_all_dirty();
    gpu.upload(&device, &queue, 0, &d.verts);
    gpu.upload_atlas(&queue, &mut sh.ui.atlas);
    let draws: Vec<Draw> = d.ranges.iter().enumerate().map(|(k, (range, tex))| Draw { buffer: 0, range: range.clone(), layer: k, texture: *tex }).collect();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.render(&device, &queue, &mut enc, &view, (w, h), Some(wgpu::Color::TRANSPARENT), &d.layers, &draws);
    // read back, laid over sky, houses and the street
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let data = buf.slice(..).get_mapped_range();
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let t = y as f32 / h as f32;
        let bg: [f32; 3] = if t < 0.42 { [96.0 + 60.0 * t, 112.0 + 50.0 * t, 128.0] } else if t < 0.62 { [178.0, 140.0, 64.0] } else { [74.0, 72.0, 68.0] };
        let i = (y * stride + x * 4) as usize;
        let a = data[i + 3] as f32 / 255.0;
        let px = |c: usize| (data[i + c] as f32 + bg[c] * (1.0 - a)).min(255.0) as u8;
        image::Rgba([px(0), px(1), px(2), 255])
    });
    let dir = omsi_cfg::flags::OMSI_UI_PREVIEW
        .live_var()
        .ok()
        .and_then(|p| std::path::PathBuf::from(p).parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&dir).ok();
    img.save(dir.join(name)).unwrap();
    println!("wrote {}", dir.join(name).display());
}

#[test]
#[ignore]
fn preview() {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.open_game_menu();
    let camera = crate::game_lists::tab_of(&app, ListKind::Options(0), "Camera");
    for (name, kind) in [
        ("pause-options.png", ListKind::Options(0)),
        ("pause-camera.png", ListKind::Options(camera)),
        ("pause-controls.png", ListKind::Controls),
        ("pause-vehicle.png", ListKind::Vehicle(0)),
        ("pause-world.png", ListKind::World(0)),
        ("pause-lines.png", ListKind::Lines),
    ] {
        app.open_list(kind);
        picture(&mut app, name, (1920, 1080));
        app.close_list();
    }
}

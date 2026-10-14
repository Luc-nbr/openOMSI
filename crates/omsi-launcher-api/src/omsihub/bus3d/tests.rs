//! Bus3D's own checks: an `.o3d` read as the old app measured the format, and a small bus read,
//! packed, kept and served - with its scrambled mesh kept scrambled on disk.

use super::pack::{self, Head};
use super::scramble;
use std::path::{Path, PathBuf};

/// An `.o3d`: its version, flags and key, its vertices, its triangles (three indices and a
/// material each) and its materials' texture names.
fn o3d(version: u8, flags: u8, key: u32, verts: &[[f32; 8]], tris: &[([u32; 3], u16)], mats: &[&str], matrix: Option<[f32; 16]>) -> Vec<u8> {
    let mut b = vec![0x84, 0x19, version];
    if version >= 3 {
        b.push(flags);
    }
    if version >= 4 {
        b.extend_from_slice(&key.to_le_bytes());
    }
    let count = |b: &mut Vec<u8>, n: usize| {
        if version >= 3 {
            b.extend_from_slice(&(n as u32).to_le_bytes());
        } else {
            b.extend_from_slice(&(n as u16).to_le_bytes());
        }
    };
    b.push(0x17);
    count(&mut b, verts.len());
    for v in verts {
        for f in v {
            b.extend_from_slice(&f.to_le_bytes());
        }
    }
    b.push(0x49);
    count(&mut b, tris.len());
    let long = version >= 3 && flags & 1 != 0;
    for (t, m) in tris {
        for i in t {
            if long {
                b.extend_from_slice(&i.to_le_bytes());
            } else {
                b.extend_from_slice(&(*i as u16).to_le_bytes());
            }
        }
        b.extend_from_slice(&m.to_le_bytes());
    }
    b.push(0x26);
    // (always a u16, in every version)
    b.extend_from_slice(&(mats.len() as u16).to_le_bytes());
    for name in mats {
        for f in [0.9f32, 0.8, 0.7, 1.0, 0.1, 0.1, 0.1, 0.0, 0.0, 0.0, 20.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.push(name.len() as u8);
        b.extend_from_slice(name.as_bytes());
    }
    if let Some(m) = matrix {
        b.push(0x79);
        for f in m {
            b.extend_from_slice(&f.to_le_bytes());
        }
    }
    b
}

/// A box's corner vertices (o3d frame: x right, y up, z forward).
fn quad(z: f32) -> Vec<[f32; 8]> {
    vec![[-1.2, 0.5, z, -1.0, 0.0, 0.0, 0.0, 0.0], [-1.2, 2.5, z, -1.0, 0.0, 0.0, 0.0, 1.0], [-1.2, 2.5, z + 4.0, -1.0, 0.0, 0.0, 1.0, 1.0], [-1.2, 0.5, z + 4.0, -1.0, 0.0, 0.0, 1.0, 0.0]]
}

#[test]
fn an_o3d_reads_as_the_old_app_measured_it() {
    // version 1: u16 counts, no flag byte, blocks at once
    let v1 = o3d(1, 0, 0, &quad(0.0), &[([0, 1, 2], 0), ([0, 2, 3], 0)], &["a.dds"], None);
    let m = omsi_o3d::parse_o3d(&v1).unwrap();
    assert_eq!((m.version, m.vertices.len(), m.triangles.len(), m.materials.len()), (1, 4, 2, 1));
    assert_eq!(m.materials[0].texture, "a.dds");
    // version 3 with long indices (flag bit 0): 14 bytes a triangle; the material count is a u16
    let v3 = o3d(3, 1, 0, &quad(0.0), &[([0, 1, 2], 0), ([0, 2, 3], 1)], &["a.dds", "b.tga"], None);
    let m = omsi_o3d::parse_o3d(&v3).unwrap();
    assert_eq!((m.triangles.len(), m.materials.len()), (2, 2));
    assert_eq!(m.triangles[1].indices, [0, 2, 3]);
    // the matrix is the mesh's pivot: the vertices stay where the file has them
    let mut mx = [0f32; 16];
    for i in [0, 5, 10, 15] {
        mx[i] = 1.0;
    }
    mx[13] = 1.688;
    mx[14] = 4.028;
    let v7 = o3d(7, 0, 0xFFFF_FFFF, &quad(0.0), &[([0, 1, 2], 0)], &["a.dds"], Some(mx));
    let m = omsi_o3d::parse_o3d(&v7).unwrap();
    assert!(m.has_transform);
    assert_eq!(m.vertices[1].position.y, 2.5, "not moved by the matrix");
    // a triangle naming a material the file does not have (195 files of an installation)
    // is read with its number: no index into the materials is taken on trust
    let odd = o3d(7, 0, 0xFFFF_FFFF, &quad(0.0), &[([0, 1, 2], 0), ([0, 2, 3], 5)], &["a.dds"], None);
    let m = omsi_o3d::parse_o3d(&odd).unwrap();
    assert_eq!((m.triangles.len(), m.triangles[1].material, m.materials.len()), (2, 5, 1));
    // a triangle that points past the vertices is dropped, the rest read
    let bad = o3d(7, 0, 0xFFFF_FFFF, &quad(0.0), &[([0, 1, 2], 0), ([0, 2, 99], 0)], &["a.dds"], None);
    assert_eq!(omsi_o3d::parse_o3d(&bad).unwrap().triangles.len(), 1);
    // no magic: refused, never a panic
    assert!(omsi_o3d::parse_o3d(b"\x00\x01\x02\x03").is_err());
    assert!(omsi_o3d::parse_o3d(&[]).is_err());
}

/// A sandbox OMSI folder with one small bus: a body (open `.o3d`, a TGA texture, alpha-tested
/// by the cfg), a mirror shown by a variable and a scrambled roof.
fn sandbox(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("omsihub-bus3d-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bus = root.join("Vehicles").join("Test");
    std::fs::create_dir_all(bus.join("model")).unwrap();
    std::fs::create_dir_all(bus.join("texture")).unwrap();
    std::fs::write(bus.join("test.bus"), "[friendlyname]\nMaker\nTester 12\nPlain\n\n[model]\nmodel\\model.cfg\n\n[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n").unwrap();
    std::fs::write(
        bus.join("model").join("model.cfg"),
        "[mesh]\nbody.o3d\n\n[matl]\nbody.tga\n0\n[matl_alpha]\n1\n\n[mesh]\nmirror.o3d\n[visible]\nvis_mirror\n1\n\n[mesh]\nroof.o3d\n\n[mesh]\nbody.o3d\n[viewpoint]\n4\n\n[mesh]\nnomirror.o3d\n[visible]\nvis_mirror\n0\n",
    )
    .unwrap();
    std::fs::write(bus.join("model").join("body.o3d"), o3d(7, 0, 0xFFFF_FFFF, &quad(-6.0), &[([0, 1, 2], 0), ([0, 2, 3], 0)], &["body.tga"], None)).unwrap();
    std::fs::write(bus.join("model").join("mirror.o3d"), o3d(1, 0, 0, &quad(5.0), &[([0, 1, 2], 0)], &["mirror.bmp"], None)).unwrap();
    std::fs::write(bus.join("model").join("nomirror.o3d"), o3d(1, 0, 0, &quad(5.5), &[([0, 1, 2], 0)], &["mirror.bmp"], None)).unwrap();
    let roof: Vec<[f32; 8]> = (0..12).map(|i| [0.1 * i as f32 - 0.6, 3.0, -5.5 + i as f32, 0.0, 1.0, 0.0, 0.3, 0.7]).collect();
    let tris: Vec<([u32; 3], u16)> = (0..10).map(|i| ([i, i + 1, i + 2], 0)).collect();
    std::fs::write(bus.join("model").join("roof.o3d"), o3d(7, 1, 12726, &roof, &tris, &["roof.dds"], None)).unwrap();
    // a 4 x 2 TGA, 32 bits
    let mut tga = vec![0u8; 18];
    tga[2] = 2;
    tga[12] = 4;
    tga[14] = 2;
    tga[16] = 32;
    tga[17] = 8;
    tga.extend(std::iter::repeat_n([10u8, 20, 200, 255], 8).flatten());
    std::fs::write(bus.join("texture").join("body.tga"), tga).unwrap();
    root
}

#[test]
fn a_bus_is_read_packed_and_its_look_worked_out_as_the_game_puts_it_down() {
    let root = sandbox("pack");
    let read = std::sync::Arc::new(pack::read_bus(&root, "Vehicles/Test/test.bus").unwrap());
    let built = pack::build(read.clone()).unwrap();
    let m = &built.manifest;
    assert_eq!(m.names, ["Maker".to_string(), "Tester 12".into(), "Plain".into()]);
    // four entries in the outside view; the AI's copy (viewpoint 4) left out
    assert_eq!(built.head.entries.len(), 4);
    assert!(built.head.entries.iter().all(|e| e.vp != 4));
    // one piece per mesh file
    assert_eq!(built.head.pieces.len(), 4);
    // the body: its texture found and read from its head, alpha-tested as the cfg says
    let body = built.head.entries.iter().find(|e| built.head.pieces[e.piece].file == "body.o3d").unwrap();
    let mat = body.mats[0].as_ref().unwrap();
    assert_eq!(mat.base.alpha, 1);
    let t = &m.textures[mat.base.tex.unwrap()];
    assert_eq!((t.info.kind.as_str(), t.info.w, t.info.h, t.info.alpha, t.info.form.as_str()), ("rgba", 4, 2, true, "TGA32"));
    assert!(t.area > 7.9 && t.area < 8.1, "the body's 2 x 4 m, {}", t.area);
    // the missing textures named, not their paths
    assert!(m.missing.iter().any(|n| n.eq_ignore_ascii_case("mirror.bmp")));
    // the box from [boundingbox], in the o3d frame (y up, z forward)
    assert_eq!((m.min[1], m.max[1], m.min[2], m.max[2]), (0.0, 3.0, -6.0, 6.0));
    // the mirror's [visible] variable picks between two meshes: 0 at rest (as the game
    // starts a model-only selector), the mirror hidden and the other shown
    let (look, _) = super::look::look_of(&read, &built.head, m, None, &[]);
    let mirror = built.head.entries.iter().position(|e| built.head.pieces[e.piece].file == "mirror.o3d").unwrap();
    assert_eq!(look.visible.len(), 4);
    assert_eq!(&look.visible[mirror..mirror + 1], "0");
    assert_eq!(look.visible.matches('1').count(), 3);
    // ... and shown with the bus option that switches it on
    let (on, _) = super::look::look_of(&read, &built.head, m, None, &[("vis_mirror".into(), 1.0)]);
    assert_eq!(&on.visible[mirror..mirror + 1], "1");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_scrambled_mesh_stays_scrambled_on_disk_and_is_served_descrambled() {
    let root = sandbox("scramble");
    let read = std::sync::Arc::new(pack::read_bus(&root, "Vehicles/Test/test.bus").unwrap());
    let built = pack::build(read).unwrap();
    assert!(built.cacheable);
    assert_eq!(built.scrambled.len(), 1);
    let roof_file = std::fs::read(root.join("Vehicles/Test/model/roof.o3d")).unwrap();
    let (at, n, s) = scramble::vertex_block(&roof_file).unwrap();
    assert!(s.is_some());
    let piece = built.head.pieces.iter().find(|p| p.file == "roof.o3d").unwrap();
    let kept = &built.tail[piece.verts.off as usize..(piece.verts.off + piece.verts.len) as usize];
    assert_eq!(kept, &roof_file[at..at + n * 32], "byte for byte as in its file");
    // the bytes as the page gets them: descrambled, as the game reads the file
    let mut bytes = pack::pack_bytes(&built.head, &built.tail);
    let (head, tail_at): (Head, usize) = pack::read_pack(&bytes).unwrap();
    assert_eq!(head, built.head);
    pack::descramble_pack(&mut bytes, tail_at, &built.scrambled);
    let read = omsi_o3d::parse_o3d(&roof_file).unwrap();
    let f = |k: usize, j: usize| f32::from_le_bytes(bytes[tail_at + piece.verts.off as usize + k * 32 + j * 4..][..4].try_into().unwrap());
    for (k, v) in read.vertices.iter().enumerate() {
        assert_eq!([f(k, 0), f(k, 1), f(k, 2)], [v.position.x, v.position.y, v.position.z], "vertex {k}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pack_that_is_no_pack_is_refused() {
    assert!(pack::read_pack(b"B3D1\0\0\0\0").is_err());
    assert!(pack::read_pack(b"B3D2\xff\0\0\0{}").is_err());
    let head = Head { version: 2, id: "x".into(), pieces: Vec::new(), entries: Vec::new() };
    let b = pack::pack_bytes(&head, &[1, 2, 3, 4]);
    let (h, at) = pack::read_pack(&b).unwrap();
    assert_eq!((h, &b[at..]), (head, &[1u8, 2, 3, 4][..]));
    assert_eq!(at % 4, 0, "the tail starts on a multiple of four");
}

/// The OMSI 2 folder the launcher is set to, for the checks over a real installation.
fn real_root() -> Option<PathBuf> {
    let p = omsi_launcher_lib::data_dir().join("launcher.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
    Some(PathBuf::from(v.get("root")?.as_str()?)).filter(|p| p.is_dir())
}

/// Every `.o3d` under `Vehicles` of the real installation (the old app's `probe-o3d.ts`):
/// read without a panic, no triangle pointing past its vertices, the normals of unit length.
/// `cargo test -p omsi-launcher-api --release -- --ignored every_o3d`
#[test]
#[ignore]
fn every_o3d_of_the_installation_reads() {
    let Some(root) = real_root() else { return };
    let mut files = Vec::new();
    let mut stack = vec![root.join("Vehicles")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("o3d")) {
                files.push(p);
            }
        }
    }
    let (mut ok, mut failed, mut normals, mut unit) = (0, 0, 0u64, 0u64);
    for f in &files {
        let Ok(b) = std::fs::read(f) else { continue };
        match omsi_o3d::parse_o3d(&b) {
            Ok(m) => {
                ok += 1;
                assert!(m.triangles.iter().all(|t| t.indices.iter().all(|&i| (i as usize) < m.vertices.len())), "{}", f.display());
                for v in m.vertices.iter().step_by(7) {
                    normals += 1;
                    if (v.normal.length() - 1.0).abs() < 0.02 {
                        unit += 1;
                    }
                }
            }
            Err(_) => failed += 1,
        }
    }
    eprintln!("{} .o3d files: {ok} read, {failed} not; normals of unit length {:.2} %", files.len(), 100.0 * unit as f64 / normals.max(1) as f64);
    assert!(ok as f64 >= files.len() as f64 * 0.99);
}

/// A real heavy bus read and packed, with the time it takes (the report's numbers):
/// `BUS3D_BUS=Vehicles/... cargo test -p omsi-launcher-api --release -- --ignored a_real_bus --nocapture`
#[test]
#[ignore]
fn a_real_bus_is_packed() {
    let Some(root) = real_root() else { return };
    let bus = std::env::var("BUS3D_BUS").unwrap_or_else(|_| "Vehicles/MAN_SD200/SD200_Bj1973.bus".into());
    let t0 = std::time::Instant::now();
    let read = std::sync::Arc::new(pack::read_bus(&root, &bus).unwrap());
    let t_read = t0.elapsed();
    let built = pack::build(read.clone()).unwrap();
    let t_built = t0.elapsed();
    let (look, _) = super::look::look_of(&read, &built.head, &built.manifest, None, &[]);
    let t_look = t0.elapsed();
    let m = &built.manifest;
    eprintln!(
        "{bus}: read {:?}, packed {:?}, look {:?}; {} triangles, {} pieces, {} entries ({} shown), {} textures ({} missing), {} scrambled, {:.1} MB, box {:?}..{:?}, parts {:?}",
        t_read,
        t_built - t_read,
        t_look - t_built,
        m.counts.triangles,
        m.counts.pieces,
        m.counts.entries,
        look.visible.matches('1').count(),
        m.counts.textures,
        m.counts.missing,
        m.counts.scrambled,
        built.tail.len() as f64 / 1e6,
        m.min,
        m.max,
        m.parts.iter().map(|p| p.offset).collect::<Vec<_>>()
    );
    assert!(m.counts.triangles > 0);
    let _ = Path::new("");
}

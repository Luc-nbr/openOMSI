//! The vertex block of an `.o3d` as it lies in the file, and the in-memory descrambling of a
//! scrambled one.
//!
//! An `.o3d` of version 4 and later carries a key after its flags; any key but `ffffffff`
//! means its vertices are scrambled (the word is the add-on's article number). Omsi-Hub's
//! rule (Luc's choice 1, 28-09-2026, kept by the old app's `core/bus3d.ts`): **descrambled
//! geometry never goes to disk**. The pack in the cache therefore keeps the vertex block of a
//! scrambled file byte for byte as in its source, and it is descrambled only in memory, as
//! the pack is served to the page (`super::cache::serve`).
//!
//! The descrambling is the one `omsi_o3d::parse_o3d` does while it reads (`unscramble`,
//! `frac_byte`), done here on a block of raw floats; `the_block_descrambles_as_the_reader_does`
//! keeps the two the same.

/// What the descrambling of a vertex block needs from the file's head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scramble {
    pub version: u8,
    pub flags: u8,
    pub key: u32,
}

/// Where an `.o3d`'s vertex block lies in `bytes`: its first byte and its vertex count, and
/// the scrambling of its vertices (None: open). None when the file does not begin with a
/// vertex block where the format puts it (then the caller does not keep raw bytes).
pub fn vertex_block(bytes: &[u8]) -> Option<(usize, usize, Option<Scramble>)> {
    if bytes.len() < 4 || bytes[0] != 0x84 || bytes[1] != 0x19 {
        return None;
    }
    let version = bytes[2];
    // version 1 begins with its blocks; 3 has a flag byte; 4 and later four key bytes after it
    let (mut p, flags, key) = if version >= 4 {
        if bytes.len() < 8 {
            return None;
        }
        (8, bytes[3], u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
    } else if version >= 3 {
        (4, bytes[3], 0xFFFF_FFFF)
    } else {
        (3, 0, 0xFFFF_FFFF)
    };
    if bytes.get(p) != Some(&0x17) {
        return None;
    }
    p += 1;
    let n = if version >= 3 {
        let b = bytes.get(p..p + 4)?;
        p += 4;
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize
    } else {
        let b = bytes.get(p..p + 2)?;
        p += 2;
        u16::from_le_bytes([b[0], b[1]]) as usize
    };
    if p + n * 32 > bytes.len() {
        return None;
    }
    let scrambled = version >= 4 && key != 0xFFFF_FFFF;
    Some((p, n, scrambled.then_some(Scramble { version, flags, key })))
}

/// Mixing byte from a vertex's raw (still scrambled) position; it feeds the next vertex.
fn frac_byte(p: [f32; 3]) -> u32 {
    let f = |x: f32| (x as f64) - (x as f64).trunc();
    let prod = (f(p[0]) * f(p[1]) * f(p[2])).abs() * 600.0;
    (prod.trunc() as i64).rem_euclid(256) as u32
}

/// One vertex (8 floats: position, normal, uv) descrambled with state `s`.
fn unscramble(s: u16, v: &mut [f32]) {
    if s < 1000 {
        v.swap(0, 1);
    } else if s < 3000 {
        v.swap(0, 2);
    } else if s > 7000 {
        v.swap(1, 2);
    }
    if s & 3 == 0 {
        v[3] = -v[3];
    }
    if s % 6 == 0 {
        v[4] = -v[4];
    }
    if s % 7 == 0 {
        v[5] = -v[5];
    }
    // (the reader's dead `s > 6500` test after `s > 4500` is left out: it never runs)
    if s < 600 {
        v.swap(4, 5);
    } else if s > 4500 {
        v.swap(3, 4);
    }
    if s % 5 == 0 {
        let m = (s % 100) as f32;
        v[6] -= m * m / 10000.0;
    }
    if s % 3 == 0 {
        let m = (s % 50) as f32;
        v[7] -= m * m / 2500.0;
    }
}

/// Descramble a vertex block of `floats` (8 per vertex) in place, as `omsi_o3d` reads it.
pub fn descramble(floats: &mut [f32], s: Scramble) {
    let n = floats.len() / 8;
    let alt = s.flags & 2 != 0;
    let mut state: u16 = {
        let k = s.key as i64 + (s.version as i64 - 4) + if alt { 0x17D } else { 0 };
        k.rem_euclid(0xFDE8) as u16
    };
    let n16 = (n as i64 % 0xFDE8) as u32;
    let mut fb: u32 = 0;
    for v in floats.chunks_exact_mut(8) {
        if s.key == 0 {
            state = if alt { 0x130 } else { 0 };
        }
        state = ((state as u32 * n16 + n16 * fb) % 8000) as u16;
        fb = frac_byte([v[0], v[1], v[2]]);
        unscramble(state, v);
    }
}

/// Descramble a block of little-endian bytes in place (the served pack's).
pub fn descramble_bytes(bytes: &mut [u8], s: Scramble) {
    let mut floats: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    descramble(&mut floats, s);
    for (b, f) in bytes.chunks_exact_mut(4).zip(floats) {
        b.copy_from_slice(&f.to_le_bytes());
    }
}

#[cfg(test)]
/// (tests) An `.o3d` of `version` with `key`, the vertices as given, one triangle and one material.
pub(crate) fn o3d_file(version: u8, flags: u8, key: u32, verts: &[[f32; 8]]) -> Vec<u8> {
    let mut b = vec![0x84, 0x19, version];
    if version >= 3 {
        b.push(flags);
    }
    if version >= 4 {
        b.extend_from_slice(&key.to_le_bytes());
    }
    b.push(0x17);
    if version >= 3 {
        b.extend_from_slice(&(verts.len() as u32).to_le_bytes());
    } else {
        b.extend_from_slice(&(verts.len() as u16).to_le_bytes());
    }
    for v in verts {
        for f in v {
            b.extend_from_slice(&f.to_le_bytes());
        }
    }
    b.push(0x49);
    let long = version >= 3 && flags & 1 != 0;
    if version >= 3 {
        b.extend_from_slice(&1u32.to_le_bytes());
    } else {
        b.extend_from_slice(&1u16.to_le_bytes());
    }
    for i in [0u32, 1, 2] {
        if long {
            b.extend_from_slice(&i.to_le_bytes());
        } else {
            b.extend_from_slice(&(i as u16).to_le_bytes());
        }
    }
    b.extend_from_slice(&0u16.to_le_bytes());
    b.push(0x26);
    b.extend_from_slice(&1u16.to_le_bytes());
    for f in [1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b.push(5);
    b.extend_from_slice(b"a.dds");
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verts(n: usize) -> Vec<[f32; 8]> {
        (0..n).map(|i| {
            let f = i as f32;
            [0.13 * f + 0.37, 1.71 - 0.07 * f, 5.03 + 0.11 * f, 0.6, -0.8, 0.0, 0.25 + 0.01 * f, 0.75]
        }).collect()
    }

    #[test]
    fn the_block_descrambles_as_the_reader_does() {
        // keys of add-ons (12726: OMSI 2 - Hamburg), the plain key 0, the second flag bit
        for (version, flags, key) in [(4u8, 0u8, 12726u32), (7, 1, 12726), (7, 3, 0x3d29), (5, 0, 0), (5, 2, 0), (7, 2, 999_999)] {
            let file = o3d_file(version, flags, key, &verts(37));
            let (at, n, s) = vertex_block(&file).unwrap();
            assert_eq!(n, 37);
            let s = s.expect("scrambled");
            let mut raw = file[at..at + n * 32].to_vec();
            descramble_bytes(&mut raw, s);
            let read = omsi_o3d::parse_o3d(&file).unwrap();
            for (k, v) in read.vertices.iter().enumerate() {
                let f = |j: usize| f32::from_le_bytes(raw[k * 32 + j * 4..k * 32 + j * 4 + 4].try_into().unwrap());
                let ours = [f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7)];
                let theirs = [v.position.x, v.position.y, v.position.z, v.normal.x, v.normal.y, v.normal.z, v.uv.x, v.uv.y];
                assert_eq!(ours, theirs, "vertex {k} of v{version} flags {flags} key {key}");
            }
        }
    }

    #[test]
    fn an_open_file_is_open_and_the_counts_are_as_wide_as_the_version_says() {
        // version 1: u16 counts and no flag byte; version 3: a flag byte and u32 counts;
        // version 4 with key ffffffff: open
        for (version, flags, key) in [(1u8, 0u8, 0u32), (3, 1, 0), (4, 0, 0xFFFF_FFFF), (7, 1, 0xFFFF_FFFF)] {
            let file = o3d_file(version, flags, key, &verts(5));
            let (at, n, s) = vertex_block(&file).unwrap();
            assert_eq!(n, 5);
            assert!(s.is_none(), "v{version} key {key:x} is open");
            assert_eq!(at, match version { 1 => 6, 3 => 9, _ => 13 }, "the block starts where the format puts it");
            let read = omsi_o3d::parse_o3d(&file).unwrap();
            assert_eq!(read.triangles.len(), 1);
            assert_eq!(read.vertices[2].position.x, f32::from_le_bytes(file[at + 64..at + 68].try_into().unwrap()));
        }
    }

    #[test]
    fn what_is_no_o3d_or_has_no_vertex_block_first_is_left_alone() {
        assert!(vertex_block(b"").is_none());
        assert!(vertex_block(b"xof 0302txt").is_none());
        let mut file = o3d_file(7, 0, 0xFFFF_FFFF, &verts(3));
        file[8] = 0x49; // a triangle block first
        assert!(vertex_block(&file).is_none());
        // a block that promises more than there is
        let file = o3d_file(1, 0, 0, &verts(3));
        assert!(vertex_block(&file[..30]).is_none());
    }
}

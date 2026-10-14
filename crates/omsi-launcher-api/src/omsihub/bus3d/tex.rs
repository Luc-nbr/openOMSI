//! The bus's textures for the page's renderer: what a file is (read from its head only) and
//! its bytes as the renderer wants them.
//!
//! Two routes, as the old app's `texturen.ts` had them, with the decoding done here instead of
//! in decoder workers of the page:
//! * `dxt`: a DDS of DXT1, DXT3 or DXT5 blocks goes to the GPU as it is, level by level; the
//!   page asks for a run of its levels by byte range (`from`, `to`), the small tail levels
//!   first so that the bus has colour at once;
//! * `rgba`: everything else (TGA, BMP, PNG, JPEG, an uncompressed or BC4-BC7 DDS, a DXT whose
//!   sides do not divide by four) is decoded with the game's own decoder (`omsi_texture`) and
//!   sent as plain RGBA, halved `skip` times on the way when the page's texture plan wants it
//!   smaller - never premultiplied: the alpha of an OMSI body texture is its reflection mask.
//!
//! Paths never reach the page: a texture is asked for by its id, which the register here
//! knows (`super::cache::register`).

use serde::Serialize;
use std::path::{Path, PathBuf};

/// One texture file as the page's plan sees it.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq)]
pub struct TexInfo {
    /// The id the page asks for it by (`b3d_tex`).
    pub id: String,
    /// `dxt` or `rgba` (see the module's head).
    pub kind: String,
    /// The DXT format: `bc1`, `bc2` or `bc3` (`dxt` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmt: Option<String>,
    pub w: u32,
    pub h: u32,
    /// Mip levels in the file (1: only the largest).
    pub mips: u32,
    /// Where each level lies in the file (`dxt` only): offset and length.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<[u64; 2]>,
    /// The file has an alpha channel (a transparency map without one gives its brightness).
    pub alpha: bool,
    /// The file's size in bytes.
    pub bytes: u64,
    /// What it is, for the log and the self-check (`DXT5`, `TGA32`, `BMP24` ...).
    pub form: String,
    /// The file's name without its folder.
    pub name: String,
}

/// A texture file and what its id stands for: changed (another size or time), the id is gone.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq)]
pub struct TexSource {
    pub id: String,
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
}

/// FNV-1a over `s` from `seed`.
fn fnv(s: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for b in s {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// A 32-character id of a text (two FNV-1a runs; ids, not secrets).
pub fn id_of(text: &str) -> String {
    format!("{:016x}{:016x}", fnv(text.as_bytes(), 0xcbf29ce484222325), fnv(text.as_bytes(), 0x84222325cbf29ce4))
}

/// A file's size and modification time (ms), or None when it cannot be read.
pub fn stamp(p: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
    Some((if m.is_dir() { 0 } else { m.len() }, t))
}

/// The id of a texture file in its present state.
pub fn tex_id(p: &Path, size: u64, mtime: u64) -> String {
    id_of(&format!("t|{}|{size}|{mtime}", p.to_string_lossy().to_lowercase()))
}

fn u16le(b: &[u8], o: usize) -> u32 {
    u16::from_le_bytes([b[o], b[o + 1]]) as u32
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u32be(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// What the head of a picture file says: its route, format, size, levels and alpha. `size`
/// is the whole file's length. None: not a picture this knows by its head.
pub fn probe_head(b: &[u8], size: u64) -> Option<(String, Option<String>, u32, u32, u32, Vec<[u64; 2]>, bool, String)> {
    if b.len() >= 128 && &b[..4] == b"DDS " {
        let h = u32le(b, 12);
        let w = u32le(b, 16);
        let flags = u32le(b, 8);
        let mips = if flags & 0x20000 != 0 { u32le(b, 28).max(1) } else { 1 };
        let pf_flags = u32le(b, 80);
        let fourcc = &b[84..88];
        let (fmt, block) = if pf_flags & 4 != 0 {
            match fourcc {
                b"DXT1" => (Some("bc1"), 8u64),
                b"DXT2" | b"DXT3" => (Some("bc2"), 16),
                b"DXT4" | b"DXT5" => (Some("bc3"), 16),
                _ => (None, 0),
            }
        } else {
            (None, 0)
        };
        if w == 0 || h == 0 {
            return None;
        }
        if let Some(fmt) = fmt {
            // the levels as far as the file has them
            let mut levels = Vec::new();
            let mut off = 128u64;
            for k in 0..mips.min(16) {
                let lw = (w >> k).max(1) as u64;
                let lh = (h >> k).max(1) as u64;
                let len = lw.div_ceil(4) * lh.div_ceil(4) * block;
                if off + len > size {
                    break;
                }
                levels.push([off, len]);
                off += len;
            }
            // (WebGL takes a compressed level 0 only when both sides divide by four)
            if !levels.is_empty() && w % 4 == 0 && h % 4 == 0 {
                let n = levels.len() as u32;
                return Some(("dxt".into(), Some(fmt.into()), w, h, n, levels, true, String::from_utf8_lossy(fourcc).into_owned()));
            }
        }
        let alpha = pf_flags & 1 != 0 || fmt.is_some() || fourcc == b"DX10";
        return Some(("rgba".into(), None, w, h, mips, Vec::new(), alpha, format!("DDS-{}", String::from_utf8_lossy(fourcc).trim_matches('\0'))));
    }
    if b.len() >= 30 && &b[..2] == b"BM" {
        let w = u32le(b, 18);
        let h = (u32le(b, 22) as i32).unsigned_abs();
        let bpp = u16le(b, 28);
        return (w > 0 && h > 0).then(|| ("rgba".into(), None, w, h, 1, Vec::new(), bpp == 32, format!("BMP{bpp}")));
    }
    if b.len() >= 26 && b.starts_with(b"\x89PNG") {
        let w = u32be(b, 16);
        let h = u32be(b, 20);
        let colour = b[25];
        let mut alpha = colour == 4 || colour == 6;
        // a palette or grey picture with a tRNS chunk before its data
        let mut p = 8usize;
        while !alpha && p + 8 <= b.len() {
            let len = u32be(b, p) as usize;
            let name = &b[p + 4..p + 8];
            if name == b"tRNS" {
                alpha = true;
            }
            if name == b"IDAT" {
                break;
            }
            p += 12 + len;
        }
        return (w > 0 && h > 0).then(|| ("rgba".into(), None, w, h, 1, Vec::new(), alpha, "PNG".into()));
    }
    if b.len() >= 4 && b[0] == 0xFF && b[1] == 0xD8 {
        let mut p = 2usize;
        while p + 9 < b.len() {
            if b[p] != 0xFF {
                p += 1;
                continue;
            }
            let m = b[p + 1];
            if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([b[p + 5], b[p + 6]]) as u32;
                let w = u16::from_be_bytes([b[p + 7], b[p + 8]]) as u32;
                return (w > 0 && h > 0).then(|| ("rgba".into(), None, w, h, 1, Vec::new(), false, "JPEG".into()));
            }
            if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
                p += 2;
                continue;
            }
            let len = u16::from_be_bytes([b[p + 2], b[p + 3]]) as usize;
            p += 2 + len;
        }
        return None;
    }
    // TGA has no signature: its head must make sense
    if b.len() >= 18 && (1..=3).contains(&(b[2] & 7)) && b[2] & 0xF0 == 0 && matches!(b[16], 8 | 15 | 16 | 24 | 32) {
        let w = u16le(b, 12);
        let h = u16le(b, 14);
        let bpp = b[16] as u32;
        let rle = if b[2] & 8 != 0 { "-RLE" } else { "" };
        return (w > 0 && h > 0).then(|| ("rgba".into(), None, w, h, 1, Vec::new(), bpp == 32, format!("TGA{rle}{bpp}")));
    }
    None
}

/// The texture file `p` as the page's plan sees it, from its head (the whole file only for a
/// format whose head this does not read). None when it cannot be read.
pub fn probe(p: &Path) -> Option<(TexInfo, TexSource)> {
    let (size, mtime) = stamp(p).unwrap_or((0, 0));
    let head = read_head(p, 1 << 16)?;
    let size = if size == 0 { head.1 } else { size };
    let found = probe_head(&head.0, size).or_else(|| {
        // a JPEG with a long head, or a format known by the decoder only
        let img = omsi_texture::decode_file(p).ok()?;
        Some(("rgba".into(), None, img.width, img.height, 1, Vec::new(), img.has_alpha, "decoded".into()))
    })?;
    let (kind, fmt, w, h, mips, levels, alpha, form) = found;
    let id = tex_id(p, size, mtime);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some((TexInfo { id: id.clone(), kind, fmt, w, h, mips, levels, alpha, bytes: size, form, name }, TexSource { id, path: p.to_path_buf(), size, mtime }))
}

/// The first `n` bytes of a file (through the content's file system: a mod used in place), and
/// the file's length.
fn read_head(p: &Path, n: usize) -> Option<(Vec<u8>, u64)> {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open(p) {
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let mut b = vec![0u8; n.min(len as usize)];
        f.read_exact(&mut b).ok()?;
        return Some((b, len));
    }
    let all = omsi_cfg::vfs::read(p).ok()?;
    let len = all.len() as u64;
    Some((all[..n.min(all.len())].to_vec(), len))
}

/// Bytes `from..to` of a file (a DXT texture's run of levels).
pub fn range(p: &Path, from: u64, to: u64) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    if to < from {
        return Err("empty range".into());
    }
    match std::fs::File::open(p) {
        Ok(mut f) => {
            f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
            let mut b = vec![0u8; (to - from) as usize];
            f.read_exact(&mut b).map_err(|e| e.to_string())?;
            Ok(b)
        }
        Err(_) => {
            let all = omsi_cfg::vfs::read(p).map_err(|e| e.to_string())?;
            all.get(from as usize..to as usize).map(<[u8]>::to_vec).ok_or_else(|| "the file is shorter".into())
        }
    }
}

/// The picture of `p` decoded to RGBA, halved `skip` times (at least to 1 x 1): eight bytes
/// of width and height (little-endian u32) and the pixels, row by row from the top.
pub fn rgba(p: &Path, skip: u32) -> Result<Vec<u8>, String> {
    let img = omsi_texture::decode_file(p).map_err(|e| e.to_string())?;
    let (mut w, mut h, mut px) = (img.width, img.height, img.rgba);
    for _ in 0..skip.min(12) {
        if w <= 1 && h <= 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        px = omsi_texture::bc::resize(&px, w, h, nw, nh);
        w = nw;
        h = nh;
    }
    let mut out = Vec::with_capacity(8 + px.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&px);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds(fourcc: &[u8; 4], w: u32, h: u32, mips: u32, data: usize) -> Vec<u8> {
        let mut b = vec![0u8; 128 + data];
        b[..4].copy_from_slice(b"DDS ");
        b[8..12].copy_from_slice(&(0x1007u32 | 0x20000).to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[28..32].copy_from_slice(&mips.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(fourcc);
        b
    }

    #[test]
    fn a_dxt_dds_lists_its_levels_as_far_as_the_file_has_them() {
        // 64 x 32 DXT5: 16 x 8 blocks of 16 bytes = 2048, then 512, 128, 32, 16, 16, 16
        let b = dds(b"DXT5", 64, 32, 7, 2048 + 512 + 128 + 32 + 16 + 16 + 16);
        let (kind, fmt, w, h, mips, levels, alpha, _) = probe_head(&b, b.len() as u64).unwrap();
        assert_eq!((kind.as_str(), fmt.as_deref(), w, h, mips, alpha), ("dxt", Some("bc3"), 64, 32, 7, true));
        assert_eq!(levels[0], [128, 2048]);
        assert_eq!(levels[1], [2176, 512]);
        assert_eq!(levels.last().unwrap()[1], 16);
        // cut off after two levels: only those
        let short = &b[..128 + 2048 + 512];
        let (_, _, _, _, mips, levels, _, _) = probe_head(short, short.len() as u64).unwrap();
        assert_eq!((mips, levels.len()), (2, 2));
    }

    #[test]
    fn a_dxt_that_webgl_cannot_take_as_it_is_is_decoded() {
        // 250 x 250: level 0 does not divide by four
        let b = dds(b"DXT1", 250, 250, 1, 63 * 63 * 8);
        let (kind, ..) = probe_head(&b, b.len() as u64).unwrap();
        assert_eq!(kind, "rgba");
    }

    #[test]
    fn the_other_formats_are_known_by_their_heads() {
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[18..22].copy_from_slice(&512u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-256i32).to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        let (kind, _, w, h, _, _, alpha, form) = probe_head(&bmp, 54).unwrap();
        assert_eq!((kind.as_str(), w, h, alpha, form.as_str()), ("rgba", 512, 256, false, "BMP24"));
        let mut tga = vec![0u8; 18];
        tga[2] = 2;
        tga[12..14].copy_from_slice(&1024u16.to_le_bytes());
        tga[14..16].copy_from_slice(&512u16.to_le_bytes());
        tga[16] = 32;
        let (_, _, w, h, _, _, alpha, form) = probe_head(&tga, 18).unwrap();
        assert_eq!((w, h, alpha, form.as_str()), (1024, 512, true, "TGA32"));
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&300u32.to_be_bytes());
        png.extend_from_slice(&200u32.to_be_bytes());
        png.extend_from_slice(&[8, 2, 0, 0, 0]);
        let (_, _, w, h, _, _, alpha, _) = probe_head(&png, png.len() as u64).unwrap();
        assert_eq!((w, h, alpha), (300, 200, false));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8, 0, 100, 0, 200, 3, 0, 0, 0, 0];
        let (_, _, w, h, _, _, alpha, _) = probe_head(&jpeg, 16).unwrap();
        assert_eq!((w, h, alpha), (200, 100, false));
        assert!(probe_head(b"not a picture at all", 20).is_none());
    }

    #[test]
    fn ids_change_with_the_file() {
        let p = Path::new("C:/OMSI 2/Vehicles/MAN/texture/a.dds");
        assert_eq!(tex_id(p, 10, 1), tex_id(Path::new("c:/omsi 2/vehicles/man/texture/A.DDS"), 10, 1));
        assert_ne!(tex_id(p, 10, 1), tex_id(p, 10, 2));
        assert_eq!(tex_id(p, 10, 1).len(), 32);
    }
}

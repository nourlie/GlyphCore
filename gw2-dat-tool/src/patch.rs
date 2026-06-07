use anyhow::{bail, Result};
use fontdue::{Font, FontSettings};

use crate::afnt::GLYPH_RANGES;

/// Rasterization size as a fraction of the font's `extents_y` (cell height). Lower = smaller glyphs.
pub const PX_FACTOR: f32 = 0.80;
/// Baseline position as a fraction of cell height (from the top). Lower = glyphs sit higher.
pub const BASELINE_FACTOR: f32 = 0.74;

/// Encode a stream of grayscale bitmaps into GW2 glyph file format.
/// Each glyph is encoded as:
///   byte 0: y_offset
///   byte 1: width - 1
///   byte 2: height - 1
///   byte 3: rle_type
///   ...stream...
pub fn encode_glyph_file(glyphs: &[EncodedGlyph]) -> Vec<u8> {
    let mut out = Vec::new();
    for g in glyphs {
        encode_single_glyph(&mut out, g);
    }
    out
}

pub struct EncodedGlyph {
    pub y_offset: u8,
    pub width: u32,
    pub height: u32,
    /// Grayscale pixels row-major, width*height bytes (0=black, 255=white)
    pub pixels: Vec<u8>,
}

fn encode_single_glyph(out: &mut Vec<u8>, g: &EncodedGlyph) {
    let w = g.width.min(255) as u8;
    let h = g.height.min(255) as u8;
    out.push(g.y_offset);
    out.push(w.saturating_sub(1));  // width - 1
    out.push(h.saturating_sub(1)); // height - 1

    let pixels = &g.pixels;
    let num_pixels = (g.width * g.height) as usize;

    // type 0 = all black, type 255 = all white (no stream) — matches genuine GW2 glyph files.
    let all_same = pixels.windows(2).all(|w| w[0] == w[1]);
    if all_same && !pixels.is_empty() {
        let val = pixels[0];
        if val == 0 { out.push(0); return; }
        else if val == 255 { out.push(255); return; }
    }

    // rle_type=1: stream of runs (0/255 value + run-length) and grayscale literal pixels (1..254).
    out.push(1);

    let mut i = 0;
    while i < num_pixels {
        let val = pixels[i];
        if val == 0 || val == 255 {
            // Count run length
            let start = i;
            while i < num_pixels && pixels[i] == val {
                i += 1;
            }
            let run = i - start;
            out.push(val);
            encode_run_length(out, run);
        } else {
            // Single literal pixel
            out.push(val);
            i += 1;
        }
    }
}

/// Encode run length N as: one or more 0xFF bytes for each 255, then remainder - 1.
/// Decoder reads: advance = 1; while stream++ == 255 { advance += 255 }; advance += last_byte.
/// So to encode run=N: emit floor((N-1)/255) bytes of 0xFF, then (N-1) % 255.
fn encode_run_length(out: &mut Vec<u8>, n: usize) {
    let mut remaining = n - 1; // decoder starts at advance=1
    while remaining >= 255 {
        out.push(255);
        remaining -= 255;
    }
    out.push(remaining as u8);
}

/// Rasterize one codepoint from the font at the given pixel size.
/// Returns None if the codepoint has no glyph (renders as 1x1 empty).
pub fn rasterize_glyph(font: &Font, codepoint: char, px: f32, cell_height: u32) -> EncodedGlyph {
    let (metrics, bitmap) = font.rasterize(codepoint, px);

    // If no visible pixels, emit a 1×1 empty glyph
    if metrics.width == 0 || metrics.height == 0 || bitmap.iter().all(|&b| b == 0) {
        return EncodedGlyph {
            y_offset: 0,
            width: 1,
            height: 1,
            pixels: vec![0],
        };
    }

    // fontdue y_min: distance from baseline down (can be negative for descenders)
    // GW2 y_offset: pixels from top of cell to top of glyph
    // fontdue metrics.ymin: distance from baseline to bottom of glyph (positive = above baseline)
    // Baseline is at cell_height - descender_reserve. We assume baseline at 80% of cell height.
    let baseline = (cell_height as f32 * BASELINE_FACTOR) as i32;
    let glyph_top = baseline - metrics.ymin as i32 - metrics.height as i32;
    let y_offset = glyph_top.max(0).min(255) as u8;

    EncodedGlyph {
        y_offset,
        width: metrics.width as u32,
        height: metrics.height as u32,
        pixels: bitmap,
    }
}

/// Load a font from TTF bytes and rasterize all glyphs for the Cyrillic range (range index 3).
/// Returns encoded glyph file bytes ready to inject into .dat.
/// Rasterize range-3 (Greek/Cyrillic) glyph structs (one per codepoint, range_end-range_start).
pub fn build_cyrillic_glyphs(ttf_bytes: &[u8], px: f32, cell_height: u32) -> Result<Vec<EncodedGlyph>> {
    let font = Font::from_bytes(ttf_bytes, FontSettings::default())
        .map_err(|e| anyhow::anyhow!("Failed to load font: {}", e))?;
    let (range_start, range_end, _) = GLYPH_RANGES[3];
    let mut glyphs = Vec::new();
    for cp in range_start..range_end {
        if let Some(ch) = char::from_u32(cp) {
            glyphs.push(rasterize_glyph(&font, ch, px, cell_height));
        } else {
            glyphs.push(EncodedGlyph { y_offset: 0, width: 1, height: 1, pixels: vec![0] });
        }
    }
    Ok(glyphs)
}

pub fn build_cyrillic_glyph_file(ttf_bytes: &[u8], px: f32, cell_height: u32) -> Result<Vec<u8>> {
    Ok(encode_glyph_file(&build_cyrillic_glyphs(ttf_bytes, px, cell_height)?))
}

/// Build a single type-1 "filler" glyph encoded to occupy EXACTLY `target` bytes (>=6).
/// Used to pad an atlas to GW2's decompressed buffer size while keeping the glyph count fixed —
/// GW2 parses the whole buffer, so trailing bytes must be consumed by a valid glyph, not left
/// as a stale tail (which desyncs FntRle → m_rleType crash). Stream = (target-6) literal pixels
/// (value 0x80) + a 2-byte run of 0; w*h chosen factorable ≤256×256.
pub fn make_filler_glyph(target: usize) -> Vec<u8> {
    assert!(target >= 6, "filler target too small");
    let stream = target - 4;          // bytes after the 4-byte header
    let lits = stream - 2;            // literal pixels (1 byte each); + 2-byte trailing run
    // pick run_pixels in 1..=255 so P=lits+run_pixels factors into w,h <= 256
    let (mut rw, mut rh, mut rrun) = (0usize, 0usize, 0usize);
    for run_pixels in 1..=255usize {
        let p = lits + run_pixels;
        if p > 256 * 256 { break; }
        let start = ((p + 255) / 256).max(1);
        for w in start..=256.min(p) {
            if p % w == 0 { rw = w; rh = p / w; rrun = run_pixels; break; }
        }
        if rw != 0 { break; }
    }
    assert!(rw != 0, "no factorable filler size for target {}", target);
    let mut out = Vec::with_capacity(target);
    out.push(0);                 // y_offset
    out.push((rw - 1) as u8);    // width - 1
    out.push((rh - 1) as u8);    // height - 1
    out.push(1);                 // rle_type = 1
    out.extend(std::iter::repeat(0x80u8).take(lits)); // literal grayscale pixels
    out.push(0);                 // run value 0
    out.push((rrun - 1) as u8);  // run length - 1
    debug_assert_eq!(out.len(), target);
    out
}

/// Wrap raw glyph file bytes in GW2 compression (inflate format the game uses).
/// The game reads these via its own decompressor. We store them as-is (uncompressed MFT entry).
/// Actually GW2 can read uncompressed entries — we just need the raw bytes.
pub fn wrap_uncompressed(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}

/// Find an available MFT slot: an entry with offset=0 and size=0.
pub fn find_free_mft_slot(entries: &[crate::dat::MftEntry]) -> Option<usize> {
    entries.iter().enumerate()
        .find(|(_, e)| e.offset == 0 && e.size == 0)
        .map(|(i, _)| i)
}

/// Build a new ANetFileReference from a file_id.
/// file_id = 0xFF00 * (p1 - 0x100) + (p0 - 0x100) + 1
/// Inverse: p1 = file_id / 0xFF00 + 0x100, p0 = (file_id - 1) % 0xFF00 + 0x100
pub fn file_id_to_reference(file_id: u32) -> [u16; 3] {
    let id = file_id - 1;
    let p1 = (id / 0xFF00 + 0x100) as u16;
    let p0 = (id % 0xFF00 + 0x100) as u16;
    [p0, p1, 0x0000]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_id_roundtrip() {
        for id in [439864u32, 439867, 3631837, 1, 100, 65280] {
            let parts = file_id_to_reference(id);
            let recovered = 0xFF00u32 * (parts[1] as u32).saturating_sub(0x100)
                + (parts[0] as u32).saturating_sub(0x100) + 1;
            assert_eq!(id, recovered, "roundtrip failed for id={}", id);
        }
    }

    #[test]
    fn test_run_length_encode() {
        let mut buf = Vec::new();
        encode_run_length(&mut buf, 1);
        assert_eq!(buf, vec![0]);

        buf.clear();
        encode_run_length(&mut buf, 4);
        assert_eq!(buf, vec![3]);

        buf.clear();
        encode_run_length(&mut buf, 256);
        assert_eq!(buf, vec![255, 0]);

        buf.clear();
        encode_run_length(&mut buf, 261);
        assert_eq!(buf, vec![255, 5]);
    }
}

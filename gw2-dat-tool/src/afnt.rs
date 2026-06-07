use anyhow::{bail, Result};

/// Hardcoded glyph ranges (same as GW2 client), 13 ranges.
/// Each entry: (range_start, range_end) — range_end is exclusive.
pub const GLYPH_RANGES: [(u32, u32, &str); 13] = [
    (0x00021, 0x0007F, "Basic Latin"),
    (0x000A1, 0x000FF, "Latin-1 Supplement"),
    (0x00100, 0x00180, "Latin Extended-A"),
    (0x00391, 0x00460, "Greek/Coptic/Cyrillic"),
    (0x02010, 0x0266B, "Symbols"),
    (0x03000, 0x03020, "CJK Symbols/Punct"),
    (0x03041, 0x03100, "Hiragana"),
    (0x03105, 0x0312A, "Bopomofo"),
    (0x03131, 0x0318F, "Hangul Compat Jamo"),
    (0x0AC00, 0x0D7A4, "Hangul Syllables"),
    (0x04E00, 0x09FA6, "CJK Unified Ideographs"),
    (0x0F900, 0x0FA6B, "CJK Compat Ideographs"),
    (0x0FF01, 0x0FFE7, "Halfwidth/Fullwidth"),
];

pub struct FontDescriptor {
    pub extents_x: u8,
    pub extents_y: u8,
    /// Resolved file IDs for each of the 13 ranges (0 = not present)
    pub file_ids: [u32; 13],
}

pub struct Glyph {
    pub codepoint: u32,
    pub width: u32,
    pub height: u32,
    pub y_offset: u8,
    /// Alpha pixels, row-major, width*height bytes
    pub pixels: Vec<u8>,
}

/// Parse the PF/AFNT container and return font descriptors.
/// `data` is the decompressed AFNT entry from the .dat.
pub fn parse_afnt(data: &[u8]) -> Result<Vec<FontDescriptor>> {
    if data.len() < 12 || &data[..2] != b"PF" {
        bail!("not a PF file");
    }

    // Find AFNT chunk. PF layout:
    //   0..2   "PF"
    //   2..4   pf_version (u16)
    //   4..6   reserved (u16)
    //   6..8   chunk_count (u16)  — may be 0 for single-chunk files
    //   8..12  chunk_type FourCC
    //  12..14  chunk_version (u16)
    //  14..16  reserved (u16)
    //  16..20  data_size (u32) — size of chunk data after this header
    //  20..24  chunk_header_size (u16) + flags (u16)
    // chunk data starts at: 8 (PF base) + chunk_header_size
    if &data[8..12] != b"AFNT" {
        bail!("not an AFNT file (FourCC = {:?})", &data[8..12]);
    }

    let chunk_header_size = u16::from_le_bytes(data[22..24].try_into().unwrap()) as usize;
    // PF header = 12 bytes, chunk header starts at 12, chunk data at 12 + chunk_header_size
    let data_start = 12 + chunk_header_size;

    let d = &data[data_start..];
    if d.len() < 8 {
        bail!("AFNT chunk too short");
    }

    let font_count = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
    // d[4..8] is a pointer to the font array (self-relative from that field's address)
    let array_ptr_offset = 4usize; // offset of the pointer field within d
    let array_ptr_value = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
    let font_array_start = array_ptr_offset + array_ptr_value; // self-relative

    // FontDescriptor layout (total 68 bytes):
    //   0..14   int8[14] unknown
    //   14      uint8 extents_x
    //   15      uint8 extents_y
    //   16..68  uint32[13] fileNames (self-relative offsets → ANetFileReference)
    const FONT_DESC_SIZE: usize = 68;

    let mut fonts = Vec::with_capacity(font_count);

    for i in 0..font_count {
        let fd_off = font_array_start + i * FONT_DESC_SIZE;
        if fd_off + FONT_DESC_SIZE > d.len() {
            bail!("FontDescriptor {} out of bounds (offset {} > {})", i, fd_off, d.len());
        }

        let extents_x = d[fd_off + 14];
        let extents_y = d[fd_off + 15];

        let mut file_ids = [0u32; 13];
        for y in 0..13usize {
            let ptr_field_off = fd_off + 16 + y * 4; // offset of fileNames[y] within d
            let ptr_val = u32::from_le_bytes(d[ptr_field_off..ptr_field_off + 4].try_into().unwrap());
            if ptr_val == 0 {
                continue;
            }
            // self-relative: ref is at ptr_field_off + ptr_val
            let ref_off = ptr_field_off + ptr_val as usize;
            if ref_off + 6 > d.len() {
                continue;
            }
            // ANetFileReference: uint16[3]
            //   parts[0]: high byte of file id component
            //   parts[1]: 0x100 or 0x101
            //   parts[2]: always 0x00
            let p0 = u16::from_le_bytes(d[ref_off..ref_off + 2].try_into().unwrap());
            let p1 = u16::from_le_bytes(d[ref_off + 2..ref_off + 4].try_into().unwrap());
            let _p2 = u16::from_le_bytes(d[ref_off + 4..ref_off + 6].try_into().unwrap());

            let file_id = 0xFF00u32 * (p1 as u32).saturating_sub(0x100)
                + (p0 as u32).saturating_sub(0x100)
                + 1;
            file_ids[y] = file_id;
        }

        fonts.push(FontDescriptor { extents_x, extents_y, file_ids });
    }

    Ok(fonts)
}

/// Decode the sequential RLE glyph stream for one range.
/// `data` is the raw content of the referenced glyph file.
/// Returns glyphs in codepoint order for [range_start, range_end).
pub fn decode_glyph_file(data: &[u8], range_start: u32, range_end: u32) -> Result<Vec<Glyph>> {
    let count = (range_end - range_start) as usize;
    let mut glyphs = Vec::with_capacity(count);
    let mut pos = 0usize;

    for cp in range_start..range_end {
        if pos + 4 > data.len() {
            bail!("glyph stream truncated at codepoint U+{:04X}", cp);
        }
        let y_offset = data[pos];
        let width = data[pos + 1] as u32 + 1;
        let height = data[pos + 2] as u32 + 1;
        let rle_type = data[pos + 3];
        pos += 4;

        let num_pixels = (width * height) as usize;
        let pixels = decode_rle(&data, &mut pos, num_pixels, rle_type)?;

        glyphs.push(Glyph { codepoint: cp, width, height, y_offset, pixels });
    }

    Ok(glyphs)
}

fn decode_rle(data: &[u8], pos: &mut usize, mut unpacked: usize, rle_type: u8) -> Result<Vec<u8>> {
    let mut out = vec![0u8; unpacked];
    let mut image_pos = 0usize;
    let mut repeated_byte: u8 = if rle_type == 0 || rle_type == 255 {
        255u8.wrapping_sub(rle_type)
    } else {
        0
    };

    while unpacked > 0 {
        let advance;

        if rle_type != 0 && rle_type != 255 {
            // rle_type == 1: literal byte then run
            // byte is 0 or 255 → run of that value; anything else → single pixel
            if *pos >= data.len() {
                bail!("RLE stream underflow");
            }
            repeated_byte = data[*pos];
            *pos += 1;
            if repeated_byte != 0 && repeated_byte != 255 {
                advance = 1;
            } else {
                // repeated_byte stays as-is (0 or 255), read run length
                advance = count_run(data, pos)?;
            }
        } else {
            // rle_type 0/255: alternating 0/255 runs
            repeated_byte = 255u8.wrapping_sub(repeated_byte);
            advance = count_run(data, pos)?;
        }

        if advance == 0 || advance > unpacked {
            bail!("RLE advance {} out of range (remaining {})", advance, unpacked);
        }
        out[image_pos..image_pos + advance].fill(repeated_byte);
        image_pos += advance;
        unpacked -= advance;
    }

    Ok(out)
}

/// Read a variable-length run count: sum of 0xFF bytes + final byte.
fn count_run(data: &[u8], pos: &mut usize) -> Result<usize> {
    let mut advance = 1usize;
    loop {
        if *pos >= data.len() {
            bail!("run-length count underflow");
        }
        let b = data[*pos] as usize;
        *pos += 1;
        advance += b;
        if b != 255 {
            break;
        }
    }
    Ok(advance)
}

/// Save glyphs as a PNG atlas (glyphs arranged in a grid).
/// Returns (atlas_width, atlas_height, png_bytes).
pub fn glyphs_to_png_atlas(glyphs: &[Glyph], cell_w: u32, cell_h: u32, cols: u32) -> Vec<u8> {
    let rows = (glyphs.len() as u32 + cols - 1) / cols;
    let atlas_w = cols * cell_w;
    let atlas_h = rows * cell_h;

    let mut atlas = image::GrayImage::new(atlas_w, atlas_h);

    for (i, glyph) in glyphs.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x_off = col * cell_w;
        let y_off = row * cell_h;

        for py in 0..glyph.height.min(cell_h) {
            for px in 0..glyph.width.min(cell_w) {
                let src = (py * glyph.width + px) as usize;
                let pixel = glyph.pixels.get(src).copied().unwrap_or(0);
                atlas.put_pixel(x_off + px, y_off + py, image::Luma([pixel]));
            }
        }
    }

    let mut buf = Vec::new();
    atlas.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
    buf
}

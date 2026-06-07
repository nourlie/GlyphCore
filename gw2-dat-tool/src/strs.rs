//! Parser for GW2 `strs` string files (the decompressed text tables in `Gw2.dat`).
//!
//! Container layout (little-endian):
//! ```text
//! char   magic[4] = "strs"
//! repeated entries, each:
//!   u16 size    // entry size in BYTES, including this field
//!   u16 count   // decoded UTF-16 char count (for Huffman entries; 0 for raw)
//!   u16 enc     // encoding tag: 0x10 = raw UTF-16LE, 6/7 = Huffman variant
//!   u8  data[size - 6]
//! ```
//! Raw (`enc == 0x10`) entries carry UTF-16LE text directly. Huffman entries pack the text as a
//! bitstream against a (still-to-be-reversed) Huffman table — we record them but can't decode the
//! text yet. NOTE: an empty entry is `size == 6` with no data.

/// Encoding tag for a plain UTF-16LE entry (no compression).
pub const ENC_RAW_UTF16: u16 = 0x10;

#[derive(Debug, Clone)]
pub struct StrEntry {
    /// Byte offset of this entry within the file (start of its `size` field).
    pub offset: usize,
    /// Total entry size in bytes (including the 6-byte header).
    pub size: u16,
    /// Decoded char count field (meaningful for Huffman; 0 for raw).
    pub count: u16,
    /// Encoding tag (`ENC_RAW_UTF16` or a Huffman variant id).
    pub enc: u16,
    /// Raw payload bytes after the 6-byte header.
    pub data: Vec<u8>,
}

impl StrEntry {
    pub fn is_raw(&self) -> bool {
        self.enc == ENC_RAW_UTF16
    }

    /// Decode the text if this is a raw UTF-16LE entry; `None` for Huffman (not yet decodable).
    pub fn text(&self) -> Option<String> {
        if !self.is_raw() {
            return None;
        }
        let u16s: Vec<u16> = self
            .data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    }
}

/// Parse a decompressed `strs` file into its entries. Returns an error if the magic is wrong or an
/// entry size runs past the end of the buffer.
pub fn parse(data: &[u8]) -> Result<Vec<StrEntry>, String> {
    if data.len() < 4 || &data[0..4] != b"strs" {
        return Err(format!("not a strs file (magic = {:02x?})", &data[..data.len().min(4)]));
    }
    let mut entries = Vec::new();
    let mut pos = 4usize;
    // Need a full 6-byte header to be a real entry. Fewer bytes remaining = the trailing language
    // footer (1-2 bytes), so stop cleanly there rather than misreading it as an entry.
    while pos + 6 <= data.len() {
        let size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        if size < 6 {
            break; // 0 = terminator, <6 = can't hold a header — stop cleanly.
        }
        let end = pos + size;
        if end > data.len() {
            break; // size runs past the buffer (trailing footer/corruption) — stop with what we have.
        }
        let count = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
        let enc = u16::from_le_bytes([data[pos + 4], data[pos + 5]]);
        entries.push(StrEntry {
            offset: pos,
            size: size as u16,
            count,
            enc,
            data: data[pos + 6..end].to_vec(),
        });
        pos = end;
    }
    Ok(entries)
}

/// Build a raw UTF-16LE entry from a string (used to overwrite an entry with a translation —
/// avoids needing a Huffman encoder; any entry can be re-emitted as raw).
pub fn build_raw_entry(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let u16s: Vec<u16> = text.encode_utf16().collect();
    let size = (6 + u16s.len() * 2) as u16;
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // count = 0 (derivable for raw)
    out.extend_from_slice(&ENC_RAW_UTF16.to_le_bytes());
    for w in u16s {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

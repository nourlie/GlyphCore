use anyhow::{bail, Result};

// GW2 .dat custom Huffman+LZ77 decompressor.
// Ported from gw2dattools (inflateDatFileBuffer.cpp, HuffmanTree.i, BitArray.i).
//
// CRITICAL: bytes are loaded as LE uint32 words; bits are read from the MSB of the word.
// So for bytes b0,b1,b2,b3 → word = b0|(b1<<8)|(b2<<16)|(b3<<24); first bit read = bit31 = MSB of b3.
//
// Every 16384 words loaded, the next 4 bytes are skipped (block CRC).
//
// C++ read<N>(val) is LAZY (peek, no advance); drop<N>() advances.
// Our read() combines peek+advance (= C++ read+drop).

// ── BitReader ──────────────────────────────────────────────────────────────────

struct BitReader<'a> {
    data: &'a [u8],
    buf: u64,            // bits ready to read, MSB = next bit
    buf_bits: u32,       // how many bits are valid in buf (from MSB)
    byte_pos: usize,     // next byte to load from data
    words_loaded: u32,   // number of 4-byte words loaded (for CRC skip)
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut r = Self { data, buf: 0, buf_bits: 0, byte_pos: 0, words_loaded: 0 };
        r.fill();
        r
    }

    fn load_word(&mut self) -> Option<u32> {
        // Skip 4 bytes every 16384 words (block CRC)
        if self.words_loaded > 0 && self.words_loaded % 16384 == 0 {
            self.byte_pos = self.byte_pos.saturating_add(4);
        }
        if self.byte_pos + 4 > self.data.len() {
            return None;
        }
        let w = u32::from_le_bytes(self.data[self.byte_pos..self.byte_pos+4].try_into().unwrap());
        self.byte_pos += 4;
        self.words_loaded += 1;
        Some(w)
    }

    fn fill(&mut self) {
        while self.buf_bits <= 32 {
            match self.load_word() {
                Some(w) => {
                    // Place new word below the currently valid bits
                    self.buf |= (w as u64) << (32 - self.buf_bits);
                    self.buf_bits += 32;
                }
                None => break,
            }
        }
    }

    #[inline]
    fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 { return 0; }
        (self.buf >> (64 - n)) as u32
    }

    #[inline]
    fn drop(&mut self, n: u32) {
        if n > self.buf_bits {
            self.buf = 0;
            self.buf_bits = 0;
            return;
        }
        self.buf <<= n;
        self.buf_bits -= n;
        if self.buf_bits <= 32 {
            self.fill();
        }
    }

    #[inline]
    fn read(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.drop(n);
        v
    }
}

// ── Static Dictionary Tree ────────────────────────────────────────────────────
//
// The dictionary tree is a static Huffman tree hardcoded in gw2dattools.
// It decodes the "code metadata" used in parseHuffmanTree.
//
// Pre-built hash table (8-bit keys) and comparison array (>8 bit codes).
// Values are (symbol, nbBits).
//
// Hash table built by tracing buildHuffmanTree with the known (symbol, nbBits) pairs.
// aCode counts DOWN from max within each length group (first symbol = highest code).
// After HEAD-insertion into linked lists, processing order is ASCENDING by symbol value.

fn dict_lookup_hash(hash8: u8) -> Option<(u16, u32)> {
    match hash8 {
        0xE0..=0xFF => Some((0x08, 3)),
        0xC0..=0xDF => Some((0x09, 3)),
        0xA0..=0xBF => Some((0x0A, 3)),
        0x90..=0x9F => Some((0x00, 4)),
        0x80..=0x8F => Some((0x07, 4)),
        0x70..=0x7F => Some((0x0B, 4)),
        0x60..=0x6F => Some((0x0C, 4)),
        0x58..=0x5F => Some((0x06, 5)),
        0x50..=0x57 => Some((0x29, 5)),
        0x48..=0x4F => Some((0x2A, 5)),
        0x40..=0x47 => Some((0xE0, 5)),
        0x3C..=0x3F => Some((0x04, 6)),
        0x38..=0x3B => Some((0x05, 6)),
        0x34..=0x37 => Some((0x20, 6)),
        0x30..=0x33 => Some((0x28, 6)),
        0x2C..=0x2F => Some((0x2B, 6)),
        0x28..=0x2B => Some((0x2C, 6)),
        0x24..=0x27 => Some((0x40, 6)),
        0x20..=0x23 => Some((0x4A, 6)),
        0x1E..=0x1F => Some((0x03, 7)),
        0x1C..=0x1D => Some((0x0D, 7)),
        0x1A..=0x1B => Some((0x25, 7)),
        0x18..=0x19 => Some((0x26, 7)),
        0x16..=0x17 => Some((0x27, 7)),
        0x14..=0x15 => Some((0x48, 7)),
        0x12..=0x13 => Some((0x49, 7)),
        0x11        => Some((0x24, 8)),
        0x10        => Some((0x47, 8)),
        0x0F        => Some((0x4B, 8)),
        0x0E        => Some((0x4C, 8)),
        0x0D        => Some((0x69, 8)),
        0x0C        => Some((0x6A, 8)),
        _           => None,
    }
}

// Long-code lookup for dict tree (nbBits > 8).
// comp = (minCode + 1) << (32 - nbBits), computed by tracing buildHuffmanTree.
// Symbols stored ascending by value (= linked-list processing order after HEAD insertions).
// Lookup: idx = (peek32 - comp) >> (32 - nb); result = symbols[len-1-idx] (reversed).
// Entries in DESCENDING comp order (largest first = shortcodes first).
fn dict_read_long(bits: &mut BitReader) -> Result<u16> {
    let peek32 = bits.peek(32);

    static LONG_CODES: &[(u32, u32, &[u16])] = &[
        // 9-bit: aCode 23→13 (10 syms), comp=(13+1)<<23=0x07000000
        (0x07000000, 9,  &[0x23,0x46,0x60,0x63,0x67,0x68,0x88,0x89,0xA0,0xE8]),
        // 10-bit: aCode 27→11 (16 syms), comp=(11+1)<<22=0x03000000
        (0x03000000, 10, &[0x01,0x02,0x2D,0x43,0x44,0x45,0x65,0x66,0x80,0x87,0x8A,0xA8,0xA9,0xC0,0xC9,0xE9]),
        // 11-bit: aCode 23→10 (13 syms), comp=(10+1)<<21=0x01600000
        (0x01600000, 11, &[0x0E,0x4D,0x64,0x6B,0x6C,0x84,0x85,0x8B,0xA4,0xA5,0xAA,0xC8,0xE5]),
        // 12-bit: aCode 21→14 (7 syms), comp=(14+1)<<20=0x00F00000
        (0x00F00000, 12, &[0x83,0x86,0xA6,0xA7,0xC7,0xCA,0xE7]),
        // 13-bit: aCode 29→23 (6 syms), comp=(23+1)<<19=0x00C00000
        (0x00C00000, 13, &[0x22,0x2E,0x8C,0xC4,0xE4,0xE6]),
        // 14-bit: aCode 47→43 (4 syms), comp=(43+1)<<18=0x00B00000
        (0x00B00000, 14, &[0x4E,0x6D,0xC6,0xEC]),
        // 15-bit: aCode 87→79 (8 syms), comp=(79+1)<<17=0x00A00000
        (0x00A00000, 15, &[0x0F,0x10,0x11,0x8D,0xAB,0xAC,0xCC,0xEA]),
        // 16-bit: aCode 159→-1 (160 syms), comp=0
        (0x00000000, 16, &[
            0x12,0x13,0x14,0x15,0x16,0x17,0x18,0x19,0x1A,0x1B,0x1C,0x1D,0x1E,0x1F,0x21,
            0x2F,0x30,0x31,0x32,0x33,0x34,0x35,0x36,0x37,0x38,0x39,0x3A,0x3B,0x3C,0x3D,
            0x3E,0x3F,0x41,0x42,0x4F,0x50,0x51,0x52,0x53,0x54,0x55,0x56,0x57,0x58,0x59,
            0x5A,0x5B,0x5C,0x5D,0x5E,0x5F,0x61,0x62,0x6E,0x6F,0x70,0x71,0x72,0x73,0x74,
            0x75,0x76,0x77,0x78,0x79,0x7A,0x7B,0x7C,0x7D,0x7E,0x7F,0x81,0x82,0x8E,0x8F,
            0x90,0x91,0x92,0x93,0x94,0x95,0x96,0x97,0x98,0x99,0x9A,0x9B,0x9C,0x9D,0x9E,
            0x9F,0xA1,0xA2,0xA3,0xAD,0xAE,0xAF,0xB0,0xB1,0xB2,0xB3,0xB4,0xB5,0xB6,0xB7,
            0xB8,0xB9,0xBA,0xBB,0xBC,0xBD,0xBE,0xBF,0xC1,0xC2,0xC3,0xC5,0xCB,0xCD,0xCE,
            0xCF,0xD0,0xD1,0xD2,0xD3,0xD4,0xD5,0xD6,0xD7,0xD8,0xD9,0xDA,0xDB,0xDC,0xDD,
            0xDE,0xDF,0xE1,0xE2,0xE3,0xEB,0xED,0xEE,0xEF,0xF0,0xF1,0xF2,0xF3,0xF4,0xF5,
            0xF6,0xF7,0xF8,0xF9,0xFA,0xFB,0xFC,0xFD,0xFE,0xFF,
        ]),
    ];

    for &(comp, nb, symbols) in LONG_CODES {
        if peek32 >= comp {
            let idx = ((peek32 - comp) >> (32 - nb)) as usize;
            let n = symbols.len();
            if idx < n {
                bits.drop(nb);
                return Ok(symbols[n - 1 - idx]);
            }
            bail!("dict long: idx={} >= n={} nb={} peek={:#010x} comp={:#010x}", idx, n, nb, peek32, comp);
        }
    }
    bail!("dict long: no match peek32={:#010x}", peek32)
}

fn dict_read_code(bits: &mut BitReader) -> Result<u16> {
    let hash8 = bits.peek(8) as u8;
    if let Some((sym, nb)) = dict_lookup_hash(hash8) {
        bits.drop(nb);
        return Ok(sym);
    }
    dict_read_long(bits)
}

// ── HuffmanTree ───────────────────────────────────────────────────────────────
//
// Uses the same descending-code assignment as gw2dattools.
// Hash table for short codes (≤8 bits), comparison array for longer codes.

struct HuffmanTree {
    hash_sym: [u16;  256],
    hash_nb:  [u8;   256],
    hash_ok:  [bool; 256],
    single_value: Option<u16>,
    comp_vals:    Vec<u32>,
    comp_nb_bits: Vec<u8>,
    comp_offset:  Vec<usize>,
    comp_syms:    Vec<u16>,
}

impl HuffmanTree {
    fn new() -> Self {
        Self {
            hash_sym: [0; 256],
            hash_nb:  [0; 256],
            hash_ok:  [false; 256],
            single_value: None,
            comp_vals: Vec::new(),
            comp_nb_bits: Vec::new(),
            comp_offset: Vec::new(),
            comp_syms: Vec::new(),
        }
    }

    fn parse(&mut self, bits: &mut BitReader, max_sym: u16) -> Result<()> {
        // C++: read(num_symbols) [lazy peek u16], then drop<u16>()
        let num_symbols = bits.read(16) as u16;  // our read() = peek + drop
        if num_symbols > max_sym + 1 {
            bail!("too many symbols: {} > {}", num_symbols, max_sym + 1);
        }

        self.hash_ok.fill(false);
        self.single_value = None;
        self.comp_vals.clear();
        self.comp_nb_bits.clear();
        self.comp_offset.clear();
        self.comp_syms.clear();

        // lengths sized exactly to num_symbols (symbols 0..num_symbols-1)
        let mut lengths = vec![0u8; num_symbols as usize];
        let mut remaining = num_symbols as i32 - 1;
        let single_value = remaining as u16;

        while remaining >= 0 {
            let code = dict_read_code(bits)?;
            let nb_bits = (code & 0x1F) as u8;
            let count   = (code >> 5) as i32 + 1;

            if nb_bits == 0 {
                remaining -= count;
            } else {
                let mut c = count;
                while c > 0 && remaining >= 0 {
                    lengths[remaining as usize] = nb_bits;
                    remaining -= 1;
                    c -= 1;
                }
            }
        }

        self.build(&lengths, single_value)
    }

    fn build(&mut self, lengths: &[u8], single_value: u16) -> Result<()> {
        let has_any = lengths.iter().any(|&l| l > 0);
        if !has_any {
            self.single_value = Some(single_value);
            return Ok(());
        }

        let max_len = lengths.iter().copied().max().unwrap_or(0) as usize;
        let nb_bits_hash: usize = 8;

        // Group symbols by length, sorted ascending (simulates HEAD insertion reversal)
        let mut by_len: Vec<Vec<u16>> = vec![Vec::new(); max_len + 1];
        for (sym, &l) in lengths.iter().enumerate() {
            if l > 0 { by_len[l as usize].push(sym as u16); }
        }
        for g in &mut by_len { g.sort_unstable(); }

        let mut acode: i64 = 0;

        for nb in 0..=max_len {
            if nb <= nb_bits_hash {
                if !by_len[nb].is_empty() {
                    for &sym in &by_len[nb] {
                        if nb == 0 {
                            acode -= 1;
                            continue;
                        }
                        let shift = nb_bits_hash - nb;
                        let h_base = (acode as usize) << shift;
                        let h_next = ((acode + 1) as usize) << shift;
                        for h in h_base..h_next.min(256) {
                            self.hash_ok[h]  = true;
                            self.hash_sym[h] = sym;
                            self.hash_nb[h]  = nb as u8;
                        }
                        acode -= 1;
                    }
                }
            } else {
                if !by_len[nb].is_empty() {
                    for &sym in &by_len[nb] {
                        self.comp_syms.push(sym);
                        acode -= 1;
                    }
                    // C++: comp = (aCode + 1) << (32 - nbBits), aCode is post-decrement value
                    let comp = ((acode + 1) as u32).wrapping_shl(32 - nb as u32);
                    self.comp_vals.push(comp);
                    self.comp_nb_bits.push(nb as u8);
                    self.comp_offset.push(self.comp_syms.len() - 1);
                }
            }
            acode = (acode << 1) + 1;
        }

        Ok(())
    }

    fn read_code(&self, bits: &mut BitReader) -> Result<u16> {
        if let Some(v) = self.single_value {
            return Ok(v);
        }

        let hash8 = bits.peek(8) as usize;
        if self.hash_ok[hash8] {
            let sym = self.hash_sym[hash8];
            bits.drop(self.hash_nb[hash8] as u32);
            return Ok(sym);
        }

        let peek32 = bits.peek(32);
        for i in 0..self.comp_vals.len() {
            if peek32 >= self.comp_vals[i] {
                let nb = self.comp_nb_bits[i] as u32;
                let idx = ((peek32 - self.comp_vals[i]) >> (32 - nb)) as usize;
                let offset = self.comp_offset[i];
                if idx <= offset {
                    bits.drop(nb);
                    return Ok(self.comp_syms[offset - idx]);
                }
                bail!("huffman: idx={} > offset={} nb={}", idx, offset, nb);
            }
        }
        bail!("huffman: no code for peek32={:#010x}", peek32)
    }
}

// ── Main decompressor ─────────────────────────────────────────────────────────

pub fn decompress_gw2(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        bail!("data too short: {} bytes", data.len());
    }

    let mut bits = BitReader::new(data);

    // Header: drop u32 (skip), read u32 (output size), drop u32 (skip)
    // C++: drop<u32>, read(outputSize) [lazy], drop<u32>
    // Our read() = peek+drop, so: drop(32), read(32) already advances, no extra drop needed.
    bits.drop(32);
    let output_size = bits.read(32) as usize;

    if output_size == 0 || output_size > 256 * 1024 * 1024 {
        bail!("unexpected output size: {}", output_size);
    }

    // C++: drop<4>, read<4>(writeSizeConst) [lazy], drop<4>
    // = skip 4 bits, read next 4 bits
    bits.drop(4);
    let write_size_add = bits.read(4) + 1;


    let mut output = vec![0u8; output_size];
    let mut out_pos = 0usize;

    let mut sym_tree = HuffmanTree::new();
    let mut cpy_tree = HuffmanTree::new();

    while out_pos < output_size {
        sym_tree.parse(&mut bits, 284)?;
        cpy_tree.parse(&mut bits, 29)?;

        // C++: read<4>(maxCount) [lazy], drop<4>
        let block_raw = bits.read(4) as usize;
        let max_codes = (block_raw + 1) << 12;

        let mut codes_read = 0usize;

        while codes_read < max_codes && out_pos < output_size {
            let symbol = sym_tree.read_code(&mut bits)?;
            codes_read += 1;

            if symbol < 0x100 {
                output[out_pos] = symbol as u8;
                out_pos += 1;
            } else {
                let sym = symbol as u32 - 0x100;
                let div4 = sym / 4;
                let rem4 = sym % 4;

                let mut write_size = if div4 == 0 {
                    sym
                } else if div4 < 7 {
                    (1 << (div4 - 1)) * (4 + rem4)
                } else if sym == 28 {
                    0xFF
                } else {
                    bail!("invalid write size sym: {}", sym)
                };

                if div4 > 1 && sym != 28 {
                    // C++: read(k, val) [lazy], drop(k)
                    write_size |= bits.read(div4 - 1);
                }
                write_size += write_size_add;

                let off_sym = cpy_tree.read_code(&mut bits)? as u32;
                let div2 = off_sym / 2;
                let rem2 = off_sym % 2;

                let mut write_offset = if div2 == 0 {
                    off_sym
                } else if div2 < 17 {
                    (1 << (div2 - 1)) * (2 + rem2)
                } else {
                    bail!("invalid offset sym: {}", off_sym)
                };

                if div2 > 1 {
                    write_offset |= bits.read(div2 - 1);
                }
                write_offset += 1;

                let offset = write_offset as usize;
                let size   = write_size as usize;

                if out_pos < offset {
                    bail!("LZ offset {} > out_pos {}", offset, out_pos);
                }
                for _ in 0..size {
                    if out_pos >= output_size { break; }
                    output[out_pos] = output[out_pos - offset];
                    out_pos += 1;
                }
            }
        }
    }

    Ok(output)
}

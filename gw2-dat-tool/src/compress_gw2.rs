/// GW2 .dat custom Huffman+LZ77 compressor.
/// Produces bitstreams compatible with decompress_gw2().
use anyhow::Result;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

// ── BitWriter ──────────────────────────────────────────────────────────────────

struct BitWriter {
    buf: u64,
    buf_bits: u32,
    out: Vec<u8>,
    words_written: u32,
}

impl BitWriter {
    fn new() -> Self { Self { buf: 0, buf_bits: 0, out: Vec::new(), words_written: 0 } }

    fn write_bits(&mut self, value: u32, n: u32) {
        if n == 0 { return; }
        self.buf |= (value as u64) << (64 - self.buf_bits - n);
        self.buf_bits += n;
        while self.buf_bits >= 32 { self.flush_word(); }
    }

    fn flush_word(&mut self) {
        let word = (self.buf >> 32) as u32;
        self.buf <<= 32;
        self.buf_bits -= 32;
        if self.words_written > 0 && self.words_written % 16384 == 0 {
            self.out.extend_from_slice(&0u32.to_le_bytes());
        }
        self.out.extend_from_slice(&word.to_le_bytes());
        self.words_written += 1;
    }

    fn finish(&mut self) {
        if self.buf_bits > 0 {
            let word = (self.buf >> 32) as u32;
            if self.words_written > 0 && self.words_written % 16384 == 0 {
                self.out.extend_from_slice(&0u32.to_le_bytes());
            }
            self.out.extend_from_slice(&word.to_le_bytes());
        }
    }
}

// ── Static dictionary encoder ──────────────────────────────────────────────────

fn dict_encode(sym: u16) -> (u32, u32) {
    match sym {
        0x08 => (0b111,    3),  0x09 => (0b110,    3),  0x0A => (0b101,    3),
        0x00 => (0b1001,   4),  0x07 => (0b1000,   4),  0x0B => (0b0111,   4),
        0x0C => (0b0110,   4),
        0x06 => (0b01011,  5),  0x29 => (0b01010,  5),  0x2A => (0b01001,  5),
        0xE0 => (0b01000,  5),
        0x04 => (0b001111, 6),  0x05 => (0b001110, 6),  0x20 => (0b001101, 6),
        0x28 => (0b001100, 6),  0x2B => (0b001011, 6),  0x2C => (0b001010, 6),
        0x40 => (0b001001, 6),  0x4A => (0b001000, 6),
        // 7-bit codes: hash 0x1E..0x1F → code = hash>>1 = 0x0F=15, etc.
        0x03 => (15, 7), 0x0D => (14, 7), 0x25 => (13, 7),
        0x26 => (12, 7), 0x27 => (11, 7), 0x48 => (10, 7),
        0x49 => (9,  7),
        0x24 => (17, 8), 0x47 => (16, 8), 0x4B => (15, 8), 0x4C => (14, 8),
        0x69 => (13, 8), 0x6A => (12, 8),
        _ => dict_encode_long(sym),
    }
}

fn dict_encode_long(sym: u16) -> (u32, u32) {
    // 9-bit codes: comp=0x07000000 >> 23 = 14
    let s9 = [0x23u16,0x46,0x60,0x63,0x67,0x68,0x88,0x89,0xA0,0xE8];
    for (i, &s) in s9.iter().enumerate() {
        if s == sym { return (14 + (s9.len()-1-i) as u32, 9); }
    }
    // 10-bit: comp=0x03000000>>22=12
    let s10 = [0x01u16,0x02,0x2D,0x43,0x44,0x45,0x65,0x66,0x80,0x87,0x8A,0xA8,0xA9,0xC0,0xC9,0xE9];
    for (i, &s) in s10.iter().enumerate() {
        if s == sym { return (12 + (s10.len()-1-i) as u32, 10); }
    }
    // 11-bit: comp=0x01600000>>21=11
    let s11 = [0x0Eu16,0x4D,0x64,0x6B,0x6C,0x84,0x85,0x8B,0xA4,0xA5,0xAA,0xC8,0xE5];
    for (i, &s) in s11.iter().enumerate() {
        if s == sym { return (11 + (s11.len()-1-i) as u32, 11); }
    }
    // 12-bit: comp=0x00F00000>>20=15
    let s12 = [0x83u16,0x86,0xA6,0xA7,0xC7,0xCA,0xE7];
    for (i, &s) in s12.iter().enumerate() {
        if s == sym { return (15 + (s12.len()-1-i) as u32, 12); }
    }
    // 13-bit: comp=0x00C00000>>19=6
    let s13 = [0x22u16,0x2E,0x8C,0xC4,0xE4,0xE6];
    for (i, &s) in s13.iter().enumerate() {
        if s == sym { return (6 + (s13.len()-1-i) as u32, 13); }
    }
    // 14-bit: comp=0x00B00000>>18=44
    let s14 = [0x4Eu16,0x6D,0xC6,0xEC];
    for (i, &s) in s14.iter().enumerate() {
        if s == sym { return (44 + (s14.len()-1-i) as u32, 14); }
    }
    // 15-bit: comp=0x00A00000>>17=80
    let s15 = [0x0Fu16,0x10,0x11,0x8D,0xAB,0xAC,0xCC,0xEA];
    for (i, &s) in s15.iter().enumerate() {
        if s == sym { return (80 + (s15.len()-1-i) as u32, 15); }
    }
    // 16-bit: code=position in list (descending from last)
    let s16: &[u16] = &[
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
    ];
    for (i, &s) in s16.iter().enumerate() {
        if s == sym { return ((s16.len()-1-i) as u32, 16); }
    }
    panic!("dict_encode: unknown symbol 0x{:02X}", sym);
}

// ── Huffman code length builder ───────────────────────────────────────────────

fn build_code_lengths(freqs: &[u32], max_len: u8) -> Vec<u8> {
    let n = freqs.len();
    let mut lengths = vec![0u8; n];

    let mut active: Vec<(u32, usize)> = freqs.iter().enumerate()
        .filter(|(_, f)| **f > 0)
        .map(|(i, &f)| (f, i))
        .collect();

    if active.is_empty() { return lengths; }
    if active.len() == 1 { lengths[active[0].1] = 1; return lengths; }

    let m = active.len();
    // Node storage: 0..m = leaves, m..2m = internal
    let mut lchild = vec![0usize; 2*m];
    let mut rchild = vec![0usize; 2*m];
    let mut node_freq = vec![0u32; 2*m];
    let mut is_internal = vec![false; 2*m];

    for (i, &(f, _)) in active.iter().enumerate() { node_freq[i] = f; }

    let mut heap: BinaryHeap<Reverse<(u32, usize)>> =
        (0..m).map(|i| Reverse((node_freq[i], i))).collect();

    let mut next = m;
    while heap.len() > 1 {
        let Reverse((f1, n1)) = heap.pop().unwrap();
        let Reverse((f2, n2)) = heap.pop().unwrap();
        node_freq[next] = f1.saturating_add(f2);
        lchild[next] = n1;
        rchild[next] = n2;
        is_internal[next] = true;
        heap.push(Reverse((node_freq[next], next)));
        next += 1;
    }

    // Depth via BFS from root
    let root = next - 1;
    let mut depth = vec![0u8; next];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root);
    while let Some(node) = queue.pop_front() {
        if is_internal[node] {
            let d = depth[node] + 1;
            depth[lchild[node]] = d;
            depth[rchild[node]] = d;
            queue.push_back(lchild[node]);
            queue.push_back(rchild[node]);
        }
    }

    for (i, &(_, sym)) in active.iter().enumerate() {
        lengths[sym] = depth[i].max(1).min(max_len);
    }

    // If any length exceeds max_len, redistribute (simple: cap and move bits)
    // The lengths might violate Kraft sum after capping — do a simple fixup
    loop {
        let kraft_scaled: i64 = lengths.iter()
            .filter(|&&l| l > 0)
            .map(|&l| 1i64 << (max_len as i64 - l as i64))
            .sum();
        let max_kraft = 1i64 << max_len;
        if kraft_scaled <= max_kraft { break; }
        // Find shortest non-max length and increase it
        if let Some(pos) = lengths.iter_mut().find(|l| **l > 0 && **l < max_len) {
            *pos += 1;
        } else { break; }
    }

    lengths
}

// ── Huffman code assignment (GW2 descending style) ────────────────────────────

fn assign_codes(lengths: &[u8]) -> Vec<(u32, u32)> {
    let n = lengths.len();
    let mut codes = vec![(0u32, 0u32); n];
    let max_len = lengths.iter().copied().max().unwrap_or(0) as usize;

    let mut by_len: Vec<Vec<usize>> = vec![Vec::new(); max_len + 1];
    for (sym, &l) in lengths.iter().enumerate() {
        if l > 0 { by_len[l as usize].push(sym); }
    }
    for g in &mut by_len { g.sort_unstable(); }

    let mut acode: i64 = 0;
    for nb in 0..=max_len {
        for &sym in &by_len[nb] {
            if nb == 0 { acode -= 1; continue; }
            codes[sym] = (acode as u32, nb as u32);
            acode -= 1;
        }
        acode = (acode << 1) + 1;
    }
    codes
}

// ── Tree encoder ───────────────────────────────────────────────────────────────

fn encode_tree(lengths: &[u8], num_symbols: usize, bits: &mut BitWriter) {
    bits.write_bits(num_symbols as u32, 16);

    // Emit (nb_bits, count) runs in DESCENDING symbol order (matching decoder)
    let mut i = num_symbols;
    while i > 0 {
        i -= 1;
        let nb = if i < lengths.len() { lengths[i] } else { 0 };
        let mut count = 1usize;
        while i > 0 {
            let prev = i - 1;
            let prev_nb = if prev < lengths.len() { lengths[prev] } else { 0 };
            if prev_nb == nb { count += 1; i -= 1; } else { break; }
        }
        // Emit in batches of ≤8 (dict symbols go up to count-1=7 within 0xFF range)
        let mut rem = count;
        while rem > 0 {
            let batch = rem.min(8);
            let dict_sym = (nb as u16) | (((batch - 1) as u16) << 5);
            let (code, len) = dict_encode(dict_sym);
            bits.write_bits(code, len);
            rem -= batch;
        }
    }
}

// ── Match length/offset encoding ───────────────────────────────────────────────

/// Returns (sym_offset_from_256, extra_value, extra_bit_count).
/// write_size_add = 1.
fn encode_length(len: u32) -> (u32, u32, u32) {
    // write_size = len - 1 (since len = write_size + write_size_add = write_size + 1)
    let ws = len - 1;

    if ws == 0xFF { return (28, 0, 0); } // special max-length code

    // sym 0-7: write_size = sym (directly, no extra bits)
    if ws <= 7 { return (ws, 0, 0); }

    // ws 8-255: parameterized by k (= div4 in decoder terms)
    // k=2: ws in [8,15],  block=2,  start=8
    // k=3: ws in [16,31], block=4,  start=16
    // k=4: ws in [32,63], block=8,  start=32
    // k=5: ws in [64,127],block=16, start=64
    // k=6: ws in [128,254],block=32,start=128
    let (k, block, start): (u32, u32, u32) = if ws < 16 { (2,2,8) }
        else if ws < 32 { (3,4,16) }
        else if ws < 64 { (4,8,32) }
        else if ws < 128 { (5,16,64) }
        else { (6,32,128) };

    let rem4 = (ws - start) / block;
    let sym  = k * 4 + rem4;
    let extra_val  = (ws - start) % block;
    let extra_bits = k - 1;
    (sym, extra_val, extra_bits)
}

/// Returns (off_sym, extra_value, extra_bit_count).
fn encode_offset(offset: u32) -> (u32, u32, u32) {
    // write_offset = offset - 1
    let wo = offset - 1;

    // off_sym 0,1: write_offset = off_sym (no extra)
    if wo <= 1 { return (wo, 0, 0); }

    // off_sym 2-3 (div2=1): write_offset = 2+rem2, no extra
    if wo <= 3 {
        let rem2 = wo - 2;
        return (2 + rem2, 0, 0);
    }

    // For wo >= 4: msb = floor(log2(wo))
    let msb = 31 - wo.leading_zeros(); // position of highest set bit
    let rem2 = (wo >> (msb - 1)) & 1;
    let base = (1u32 << (msb - 1)) * (2 + rem2);
    let extra_val  = wo - base;
    let extra_bits = msb - 1;
    let off_sym    = msb * 2 + rem2;
    (off_sym, extra_val, extra_bits)
}

// ── LZ77 ──────────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Token { Literal(u8), Match { len: u32, offset: u32 } }

const MIN_MATCH: u32 = 3;
const MAX_MATCH: u32 = 254;
const WINDOW:    usize = 32768;
const HASH_BITS: usize = 16;
const HASH_SIZE: usize = 1 << HASH_BITS;
const CHAIN_DEPTH: usize = 512;

fn hash3(d: &[u8], p: usize) -> usize {
    if p + 2 >= d.len() {
        return (d[p] as usize) & (HASH_SIZE - 1);
    }
    let v = (d[p] as u32).wrapping_mul(2654435761)
        ^ (d[p+1] as u32).wrapping_mul(40503)
        ^ (d[p+2] as u32);
    (v as usize) & (HASH_SIZE - 1)
}

fn find_match(data: &[u8], pos: usize, head: &[usize], next: &[usize]) -> (u32, u32) {
    let n = data.len();
    if pos + 3 >= n { return (0, 0); }
    let h = hash3(data, pos);
    let mut best_len = MIN_MATCH - 1;
    let mut best_off = 0u32;
    let mut cur = head[h];
    let mut steps = 0;
    while cur != usize::MAX && steps < CHAIN_DEPTH {
        if cur >= pos || pos - cur > WINDOW { break; }
        let offset = (pos - cur) as u32;
        let max_ml = ((n - pos) as u32).min(MAX_MATCH) as usize;
        let mut ml = 0;
        while ml < max_ml && data[cur+ml] == data[pos+ml] { ml += 1; }
        if ml as u32 > best_len { best_len = ml as u32; best_off = offset; }
        if best_len >= MAX_MATCH { break; }
        cur = next[cur];
        steps += 1;
    }
    (best_len, best_off)
}

fn insert(pos: usize, head: &mut [usize], next: &mut [usize], data: &[u8]) {
    if pos + 3 >= data.len() { return; }
    let h = hash3(data, pos);
    next[pos] = head[h];
    head[h] = pos;
}

fn lz77(data: &[u8]) -> Vec<Token> {
    let n = data.len();
    let mut tokens = Vec::with_capacity(n);
    let mut head = vec![usize::MAX; HASH_SIZE];
    let mut next = vec![usize::MAX; n];
    let mut pos = 0;

    while pos < n {
        if pos + 3 >= n {
            tokens.push(Token::Literal(data[pos]));
            pos += 1;
            continue;
        }

        // find_match BEFORE insert (so pos is not in its own chain)
        let (len0, off0) = find_match(data, pos, &head, &next);
        insert(pos, &mut head, &mut next, data);

        if len0 < MIN_MATCH {
            tokens.push(Token::Literal(data[pos]));
            pos += 1;
            continue;
        }

        // 2-level lazy: check pos+1 and pos+2 before committing
        let (len1, off1) = if pos + 4 < n {
            find_match(data, pos + 1, &head, &next)
        } else { (0, 0) };

        if len1 > len0 {
            // pos+1 is better — emit literal at pos, use match at pos+1
            tokens.push(Token::Literal(data[pos]));
            pos += 1;
            insert(pos, &mut head, &mut next, data);
            tokens.push(Token::Match { len: len1, offset: off1 });
            for skip in 1..len1 as usize { insert(pos + skip, &mut head, &mut next, data); }
            pos += len1 as usize;
        } else {
            tokens.push(Token::Match { len: len0, offset: off0 });
            for skip in 1..len0 as usize { insert(pos + skip, &mut head, &mut next, data); }
            pos += len0 as usize;
        }
    }
    tokens
}

// ── Main compressor ───────────────────────────────────────────────────────────

pub fn compress_gw2(data: &[u8]) -> Result<Vec<u8>> {
    let output_size = data.len() as u32;
    let write_size_add: u32 = 1;

    let tokens = lz77(data);

    // Count frequencies
    let mut sym_freq = vec![0u32; 285]; // 256 literals + 29 match lengths
    let mut cpy_freq = vec![0u32; 30];

    for tok in &tokens {
        match tok {
            Token::Literal(b) => sym_freq[*b as usize] += 1,
            Token::Match { len, offset } => {
                let (s, _, _) = encode_length(*len);
                sym_freq[(256 + s) as usize] += 1;
                let (o, _, _) = encode_offset(*offset);
                cpy_freq[o as usize] += 1;
            }
        }
    }
    // Ensure at least one offset symbol
    if cpy_freq.iter().all(|&f| f == 0) { cpy_freq[0] = 1; }

    let mut bits = BitWriter::new();

    // Header: decoder reads drop(32)+read(32)+drop(4)+read(4) = 72 bits total
    bits.write_bits(0, 32);
    bits.write_bits(output_size, 32);
    bits.write_bits(0, 4);
    bits.write_bits(write_size_add - 1, 4);

    // Multi-block encoding: try several block sizes, pick smallest output
    // block_raw: max_codes_per_block = (block_raw+1) << 12
    let best = (0u32..=5).map(|block_raw| {
        let max_per = ((block_raw + 1) as usize) << 12;
        let mut b = BitWriter::new();
        // Write full header for each candidate
        b.write_bits(0, 32);
        b.write_bits(output_size, 32);
        b.write_bits(0, 4);
        b.write_bits(write_size_add - 1, 4);
        if encode_blocks(&tokens, &mut b, block_raw, max_per).is_ok() {
            b.finish();
            Some(b.out)
        } else { None }
    }).flatten().min_by_key(|v| v.len());

    if let Some(out) = best {
        return Ok(out);
    }

    // Fallback: single large block
    encode_blocks(&tokens, &mut bits, 15, 65536)?;
    bits.finish();
    Ok(bits.out)
}

fn encode_blocks(tokens: &[Token], bits: &mut BitWriter, block_raw: u32, max_per: usize) -> Result<()> {
    let chunk_size = max_per;
    let mut start = 0;
    while start < tokens.len() {
        let end = (start + chunk_size).min(tokens.len());
        let block = &tokens[start..end];

        // Count frequencies for this block only
        let mut sym_freq = vec![0u32; 285];
        let mut cpy_freq = vec![0u32; 30];
        for tok in block {
            match tok {
                Token::Literal(b) => sym_freq[*b as usize] += 1,
                Token::Match { len, offset } => {
                    let (s, _, _) = encode_length(*len);
                    sym_freq[(256 + s) as usize] += 1;
                    let (o, _, _) = encode_offset(*offset);
                    cpy_freq[o as usize] += 1;
                }
            }
        }
        if cpy_freq.iter().all(|&f| f == 0) { cpy_freq[0] = 1; }

        let sym_lengths = build_code_lengths(&sym_freq, 15);
        let cpy_lengths = build_code_lengths(&cpy_freq, 15);
        let sym_codes = assign_codes(&sym_lengths);
        let cpy_codes = assign_codes(&cpy_lengths);

        let sym_num = sym_lengths.iter().rposition(|&l| l > 0).map(|i| i+1).unwrap_or(1);
        let cpy_num = cpy_lengths.iter().rposition(|&l| l > 0).map(|i| i+1).unwrap_or(1);

        encode_tree(&sym_lengths, sym_num, bits);
        encode_tree(&cpy_lengths, cpy_num, bits);
        bits.write_bits(block_raw, 4);

        for tok in block {
            match tok {
                Token::Literal(b) => {
                    let (code, len) = sym_codes[*b as usize];
                    if len == 0 { anyhow::bail!("literal 0x{:02x} no code", b); }
                    bits.write_bits(code, len);
                }
                Token::Match { len, offset } => {
                    let (sym_off, ev, eb) = encode_length(*len);
                    let sym = 256 + sym_off as usize;
                    let (code, clen) = sym_codes[sym];
                    if clen == 0 { anyhow::bail!("match sym {} no code", sym); }
                    bits.write_bits(code, clen);
                    if eb > 0 { bits.write_bits(ev, eb); }
                    let (off_sym, oe, ob) = encode_offset(*offset);
                    let (ocode, olen) = cpy_codes[off_sym as usize];
                    if olen == 0 { anyhow::bail!("offset sym {} no code", off_sym); }
                    bits.write_bits(ocode, olen);
                    if ob > 0 { bits.write_bits(oe, ob); }
                }
            }
        }
        start = end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::decompress_gw2;

    fn roundtrip(data: &[u8]) {
        let compressed = compress_gw2(data).expect("compress failed");
        let decompressed = decompress_gw2(&compressed).expect("decompress failed");
        assert_eq!(data, decompressed.as_slice(), "roundtrip mismatch");
        println!("  {} → {} bytes ({:.1}%)", data.len(), compressed.len(),
            compressed.len() as f64 / data.len() as f64 * 100.0);
    }

    #[test] fn test_two_chars()   { roundtrip(b"AB"); }
    /// Force all-literals (no LZ77) to check if tree is the issue
    #[test] fn test_no_lz77() {
        // These inputs fail with matches - try as pure literals
        let inputs: &[&[u8]] = &[b"HelloHell", b"HelloHello",
            b"Hello, GW2 font patch! Hello, GW2 font patch!"];
        for &input in inputs {
            let tokens: Vec<Token> = input.iter().map(|&b| Token::Literal(b)).collect();
            // Use the same flow as compress_gw2 but with forced literal tokens
            let mut sym_freq = vec![0u32; 285];
            let cpy_freq = vec![1u32; 1]; // minimal cpy_tree
            for tok in &tokens {
                if let Token::Literal(b) = tok { sym_freq[*b as usize] += 1; }
            }
            let sym_lengths = build_code_lengths(&sym_freq, 15);
            let cpy_lengths = build_code_lengths(&cpy_freq, 15);
            let sym_codes = assign_codes(&sym_lengths);
            let cpy_codes = assign_codes(&cpy_lengths);
            let sym_num = sym_lengths.iter().rposition(|&l| l > 0).map(|i| i+1).unwrap_or(1);
            let cpy_num = cpy_lengths.iter().rposition(|&l| l > 0).map(|i| i+1).unwrap_or(1);
            let mut bits = BitWriter::new();
            bits.write_bits(0, 32);
            bits.write_bits(input.len() as u32, 32);
            bits.write_bits(0, 4);
            bits.write_bits(0, 4);
            encode_tree(&sym_lengths, sym_num, &mut bits);
            encode_tree(&cpy_lengths, cpy_num, &mut bits);
            bits.write_bits(15, 4);
            for tok in &tokens {
                if let Token::Literal(b) = tok {
                    let (code, len) = sym_codes[*b as usize];
                    bits.write_bits(code, len);
                }
            }
            bits.finish();
            let compressed = bits.out;
            let dec = crate::compression::decompress_gw2(&compressed).expect("decompress");
            if input != dec.as_slice() {
                eprintln!("  compressed: {:02x?}", &compressed[..compressed.len().min(32)]);
                eprintln!("  expected:   {:?}", std::str::from_utf8(input));
                eprintln!("  got:        {:?}", std::str::from_utf8(&dec));
                panic!("mismatch for {:?}", std::str::from_utf8(input));
            }
            eprintln!("  no-lz77 {:?} ok ({} bytes)", std::str::from_utf8(input), compressed.len());
        }
    }

    #[test] fn test_dict_sym() {
        // Verify which batch sizes work for nb=0 (skip)
        for k in 1u16..=8 {
            let sym = 0u16 | ((k-1)<<5);
            // Try to call dict_encode(sym) - will panic if invalid
            eprintln!("nb=0 k={} sym=0x{:02X}", k, sym);
            let _ = dict_encode(sym);
        }
        eprintln!("all ok");
    }
    #[test] fn test_hello_only()  { roundtrip(b"Hello"); }
    #[test] fn test_real_afnt() {
        let afnt = std::fs::read("font_afnt.bin").expect("font_afnt.bin not found");
        let compressed = compress_gw2(&afnt).expect("compress");
        let decompressed = crate::compression::decompress_gw2(&compressed).expect("decompress");
        assert_eq!(afnt, decompressed, "roundtrip mismatch");
        eprintln!("  original AFNT: {} → {} bytes ({:.1}%, target ≤8704)",
            afnt.len(), compressed.len(),
            compressed.len() as f64 / afnt.len() as f64 * 100.0);

        // Test: can we decompress truncated stream? (8704 bytes of 8732-byte stream)
        if compressed.len() > 8704 {
            let truncated = &compressed[..8704];
            match crate::compression::decompress_gw2(truncated) {
                Ok(d) => {
                    let matches = d == afnt;
                    eprintln!("  truncated to 8704 → decompressed {} bytes, correct={}", d.len(), matches);
                }
                Err(e) => eprintln!("  truncated to 8704 → ERROR: {}", e),
            }
        }
    }
    #[test] fn test_small_match() { roundtrip(b"HelloHello"); }  // has match
    #[test] fn test_debug() {
        // Test with increasingly complex inputs to find the breaking point
        for input in [b"H".as_ref(), b"He", b"Hel", b"Hell", b"Hello",
                      b"HelloH", b"HelloHe", b"HelloHel", b"HelloHell", b"HelloHello"] {
            let c = compress_gw2(input).expect("compress");
            let d = crate::compression::decompress_gw2(&c).expect("decompress");
            if input != d.as_slice() {
                eprintln!("FAIL len={}: input={:?} got={:?}", input.len(),
                    std::str::from_utf8(input), std::str::from_utf8(&d));
                panic!("mismatch");
            }
        }
    }
    #[test] fn test_hello()  { roundtrip(b"Hello, GW2 font patch! Hello, GW2 font patch!"); }
    #[test] fn test_zeros()  { roundtrip(&vec![0u8; 1000]); }
    #[test] fn test_ramp()   { roundtrip(&(0u8..=255).collect::<Vec<_>>()); }
    #[test] fn test_repeat() { roundtrip(&b"ABCDABCDABCDABCDABCD".repeat(50)); }
    #[test] fn test_long_match() {
        let mut d = vec![0u8; 10000];
        for i in 0..d.len() { d[i] = ((i * 7 + 13) % 17) as u8; }
        roundtrip(&d);
    }
}

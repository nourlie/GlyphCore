mod afnt;
mod compress_gw2;
mod compression;
mod dat;
mod patch;
mod strs;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gw2-dat-tool", about = "GW2 .dat file parser and extractor")]
struct Cli {
    /// Path to Gw2.dat
    #[arg(short, long)]
    dat: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all MFT entries with their type
    List {
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// Extract a single file by MFT index
    Extract {
        index: usize,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Show info about the .dat header
    Info,
    /// Dump raw hex bytes of an entry (no decompression)
    Dump {
        index: usize,
        #[arg(short, long, default_value = "64")]
        bytes: usize,
    },
    /// Scan entries with decompression to find real file types (slow)
    Scan {
        #[arg(short, long)]
        filter: Option<String>,
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Parse AFNT font by MFT index: show font descriptors and file IDs
    FontInfo {
        index: usize,
    },
    /// Extract glyphs from AFNT font as PNG atlases
    FontExtract {
        /// MFT index of the AFNT entry
        index: usize,
        /// Output directory for PNG files
        #[arg(short, long)]
        out: PathBuf,
        /// Font index inside AFNT (default: all)
        #[arg(short, long)]
        font: Option<usize>,
        /// Only extract Basic Latin range (range 0)
        #[arg(long)]
        latin_only: bool,
        /// Atlas columns (default: 16)
        #[arg(short, long, default_value = "16")]
        cols: u32,
    },
    /// Generate patch data files for the Nexus addon (no .dat modification)
    GenPatch {
        /// MFT index of the AFNT entry (e.g. 37306)
        afnt_index: usize,
        /// Path to TTF font file
        #[arg(short, long)]
        ttf: PathBuf,
        /// Font index to patch (default: 0)
        #[arg(short, long, default_value = "0")]
        font: usize,
        /// Output directory for generated files
        #[arg(short, long, default_value = ".")]
        out: PathBuf,
    },
    /// Generate per-font Cyrillic glyph atlases (one per font size) as a single blob for the
    /// in-memory decompressor-hook injector. Blob: count(u32) then [range11_file_id(u32),
    /// compressed_len(u32), compress_gw2(363-glyph atlas)] per font that has range 11.
    GenAtlases {
        /// MFT index of the AFNT entry
        afnt_index: usize,
        /// Path to TTF font file
        #[arg(short, long)]
        ttf: PathBuf,
        /// Output blob path
        #[arg(short, long, default_value = "cyrillic_atlases.bin")]
        out: PathBuf,
    },
    /// Re-pack: fix fileNames[3] in afnt_patched_raw.bin to use a NEW unique ANetFileReference.
    /// Appends 6-byte ref at end, fixes self-relative ptr, compresses, writes afnt_patched.bin.
    RepackAfnt {
        /// Directory with afnt_patched_raw.bin (also writes afnt_patched.bin here)
        #[arg(short, long, default_value = "patch_output")]
        dir: PathBuf,
        /// file_id to embed in the new ANetFileReference (default: 3631839 = CJK Compat)
        #[arg(long, default_value = "3631839")]
        file_id: u32,
        /// Font descriptor index whose fileNames[3] to fix (default: 0)
        #[arg(long, default_value = "0")]
        font: usize,
    },
    /// Diagnostic: resolve a file_id via ATOC and dump its strs entries (enc histogram + samples).
    StrsProbe {
        file_id: u32,
    },
    /// Full-dat export (like gw2browser): scan EVERY MFT entry, find `strs` string files whose
    /// language byte is English (0), and dump all their raw (decodable) strings into one CSV
    /// translation template. Slow (decompresses every entry) but complete and automatic.
    StrsExportAll {
        /// Output CSV template path
        #[arg(short, long, default_value = "dict_full.csv")]
        out: PathBuf,
        /// Skip strings longer than this many chars
        #[arg(long, default_value = "100000")]
        max_len: usize,
        /// Language code to extract (0=English,1=Korean,2=French,3=German,4=Spanish,5=Chinese)
        #[arg(long, default_value = "0")]
        language: u8,
    },
    /// Build an English→Russian translation TEMPLATE from many strs files: classify which files
    /// are English, collect their raw (UTF-16) strings deduped, and write `English<TAB>` lines
    /// (empty RU) ready to fill in. Only raw entries (the ones the in-place patcher can translate).
    StrsTemplate {
        /// File with comma/whitespace-separated MFT indices of strs files (from `scan --filter strs`).
        #[arg(long)]
        indices_file: PathBuf,
        /// Output CSV template path
        #[arg(short, long, default_value = "dict_template.csv")]
        out: PathBuf,
        /// Skip strings longer than this many chars (UI strings; long descriptions rarely fit RU≤EN)
        #[arg(long, default_value = "60")]
        max_len: usize,
    },
    /// Parse a `strs` string-table entry (by MFT index) and dump its strings. Decodes raw
    /// UTF-16 entries; Huffman entries are listed but not yet decoded.
    StrsDump {
        /// MFT index of the strs entry
        index: usize,
        /// Only show entries whose decoded text contains this substring (raw entries only)
        #[arg(short, long)]
        filter: Option<String>,
        /// Max entries to print
        #[arg(short, long)]
        limit: Option<usize>,
        /// Only print raw UTF-16 (decodable) entries
        #[arg(long)]
        raw_only: bool,
    },
    /// Validate that a TTF produces structurally-clean atlases for every font (repro for the
    /// in-game FntRle m_rleType desync crash). Builds each atlas exactly like GenAtlases, then
    /// decodes it back and reports any font whose stream is malformed or doesn't consume `gen`.
    ValidateAtlases {
        afnt_index: usize,
        #[arg(short, long)]
        ttf: PathBuf,
        /// Dump per-glyph dims for this font index instead of validating.
        #[arg(long)]
        dump_font: Option<usize>,
    },
    /// Patch Cyrillic glyphs into AFNT from a TTF font file
    PatchCyrillic {
        /// MFT index of the AFNT entry (e.g. 37306)
        afnt_index: usize,
        /// Path to TTF font file (e.g. Roboto.ttf)
        #[arg(short, long)]
        ttf: PathBuf,
        /// Font index inside AFNT to patch (default: 0)
        #[arg(short, long, default_value = "0")]
        font: usize,
        /// Pixel size for rasterization (default: auto from extents_y)
        #[arg(short, long)]
        px: Option<f32>,
        /// Dry run: show what would be done without writing
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut reader = dat::DatReader::open(&cli.dat)?;

    match cli.command {
        Command::Info => {
            let hdr = reader.header();
            println!("GW2 .dat info");
            println!("  Version     : {}", hdr.version);
            println!("  Chunk size  : {}", hdr.chunk_size);
            println!("  MFT offset  : {:#x}", hdr.mft_offset);
            println!("  MFT size    : {} bytes", hdr.mft_size);
            println!("  MFT entries : {}", reader.entry_count());
        }

        Command::List { filter } => {
            println!("{:>8}  {:>10}  {:>4}  {:>6}  {}", "index", "offset", "cmp", "size", "type");
            println!("{}", "-".repeat(50));
            for (i, entry) in reader.entries().iter().enumerate() {
                let type_str = entry.file_type.as_deref().unwrap_or("????");
                if let Some(ref f) = filter {
                    if !type_str.contains(f.as_str()) { continue; }
                }
                println!(
                    "{:>8}  {:#010x}  {:>4}  {:>6}  {}",
                    i, entry.offset,
                    if entry.compressed { "zlib" } else { "raw" },
                    entry.size, type_str,
                );
            }
        }

        Command::Dump { index, bytes } => {
            let entry = &reader.entries()[index];
            println!("Entry {}: offset={:#x} size={} compressed={}", index, entry.offset, entry.size, entry.compressed);
            let data = reader.read_raw(index)?;
            let show = bytes.min(data.len());
            for (i, chunk) in data[..show].chunks(16).enumerate() {
                print!("  {:04x}  ", i * 16);
                for b in chunk { print!("{:02x} ", b); }
                for _ in chunk.len()..16 { print!("   "); }
                print!(" |");
                for &b in chunk { print!("{}", if b.is_ascii_graphic() { b as char } else { '.' }); }
                println!("|");
            }
        }

        Command::Scan { filter, limit } => {
            let count = limit.unwrap_or(reader.entry_count());
            println!("{:>8}  {:>6}  {}", "index", "size", "type");
            println!("{}", "-".repeat(30));
            for i in 0..count.min(reader.entry_count()) {
                if let Some(t) = reader.read_entry_type(i) {
                    if let Some(ref f) = filter {
                        if !t.contains(f.as_str()) { continue; }
                    }
                    let size = reader.entries()[i].size;
                    println!("{:>8}  {:>6}  {}", i, size, t);
                }
            }
        }

        Command::FontInfo { index } => {
            let data = reader.read_entry(index)?;
            let fonts = afnt::parse_afnt(&data)?;
            println!("AFNT entry {}: {} font(s)", index, fonts.len());
            for (fi, font) in fonts.iter().enumerate() {
                println!("\n  Font [{}]: extents {}x{}", fi, font.extents_x, font.extents_y);
                for (ri, &(start, end, name)) in afnt::GLYPH_RANGES.iter().enumerate() {
                    let fid = font.file_ids[ri];
                    if fid == 0 {
                        println!("    range {:2}: {:30} U+{:04X}..U+{:04X}  (not present)", ri, name, start, end);
                    } else {
                        println!("    range {:2}: {:30} U+{:04X}..U+{:04X}  file_id={}", ri, name, start, end, fid);
                    }
                }
            }
        }

        Command::FontExtract { index, out, font, latin_only, cols } => {
            let data = reader.read_entry(index)?;
            let fonts = afnt::parse_afnt(&data)?;

            // Build file_id → MFT index map from ATOC
            println!("Reading ATOC file-id map...");
            let atoc = reader.read_atoc()?;
            println!("ATOC: {} entries", atoc.len());

            std::fs::create_dir_all(&out)?;

            let font_range = match font {
                Some(fi) => fi..fi + 1,
                None => 0..fonts.len(),
            };

            for fi in font_range {
                let fnt = &fonts[fi];
                println!("\nFont [{}]: extents {}x{}", fi, fnt.extents_x, fnt.extents_y);

                let range_iter: Vec<usize> = if latin_only {
                    vec![0]
                } else {
                    (0..13).collect()
                };

                for ri in range_iter {
                    let (range_start, range_end, range_name) = afnt::GLYPH_RANGES[ri];
                    let file_id = fnt.file_ids[ri];
                    if file_id == 0 {
                        println!("  range {:2} ({}) — not present", ri, range_name);
                        continue;
                    }

                    let mft_idx = match atoc.get(&file_id) {
                        Some(&idx) => idx as usize,
                        None => {
                            println!("  range {:2} ({}) — file_id {} not in ATOC", ri, range_name, file_id);
                            continue;
                        }
                    };

                    println!("  range {:2} ({}) file_id={} mft={} U+{:04X}..U+{:04X} ({} glyphs)",
                        ri, range_name, file_id, mft_idx, range_start, range_end, range_end - range_start);

                    let glyph_data = match reader.read_entry(mft_idx) {
                        Ok(d) => d,
                        Err(e) => {
                            println!("    ERROR reading entry: {}", e);
                            continue;
                        }
                    };

                    let glyphs = match afnt::decode_glyph_file(&glyph_data, range_start, range_end) {
                        Ok(g) => g,
                        Err(e) => {
                            println!("    ERROR decoding glyphs: {}", e);
                            continue;
                        }
                    };

                    // Find max dimensions for atlas cell
                    let cell_w = glyphs.iter().map(|g| g.width).max().unwrap_or(1);
                    let cell_h = glyphs.iter().map(|g| g.height).max().unwrap_or(1);

                    println!("    decoded {} glyphs, max cell {}x{}", glyphs.len(), cell_w, cell_h);

                    // Print info for each glyph (first 20)
                    for g in glyphs.iter().take(20) {
                        let ch = char::from_u32(g.codepoint).unwrap_or('?');
                        println!("      U+{:04X} '{}' {}x{} yoff={}", g.codepoint, ch, g.width, g.height, g.y_offset);
                    }
                    if glyphs.len() > 20 {
                        println!("      ... ({} more)", glyphs.len() - 20);
                    }

                    let png = afnt::glyphs_to_png_atlas(&glyphs, cell_w, cell_h, cols);
                    let fname = out.join(format!("font{}_range{:02}_{}.png", fi, ri, range_name.replace('/', "_").replace(' ', "_")));
                    std::fs::write(&fname, &png)?;
                    println!("    saved {}", fname.display());
                }
            }
        }

        Command::Extract { index, out } => {
            let data = reader.read_entry(index)?;
            std::fs::write(&out, &data)?;
            println!("Extracted entry {} -> {} ({} bytes)", index, out.display(), data.len());
        }

        Command::GenPatch { afnt_index, ttf, font: font_idx, out } => {
            let afnt_data = reader.read_entry(afnt_index)?;
            let fonts = afnt::parse_afnt(&afnt_data)?;
            let fnt = fonts.get(font_idx).ok_or_else(|| anyhow::anyhow!("font {} not found", font_idx))?;
            let px = fnt.extents_y as f32 * patch::PX_FACTOR;

            println!("Font [{}]: extents {}x{}, rasterizing at {:.1}px", font_idx, fnt.extents_x, fnt.extents_y, px);
            let ttf_bytes = std::fs::read(&ttf)?;
            let cyrillic_bytes = patch::build_cyrillic_glyph_file(&ttf_bytes, px, fnt.extents_y as u32)?;

            // Strategy: reuse file_id=3631839 (CJK Compat Ideographs, range 11, 363 glyphs).
            // This file is COMPRESSED and large (17684 bytes), perfect for our needs.
            // We point fileNames[3] to its EXISTING ANetFileReference.
            // ptr_val=0x2978: fileNames[11] at d[72] has ptr_val=0x2958 → ref at d[10656]
            // From d[40]: ptr_val = 10656-40 = 10616 = 0x2978
            let cyrillic_file_id: u32 = 3631839; // CJK Compat Ideographs, range 11
            let cyrillic_mft_idx = {
                let atoc_tmp = reader.read_atoc()?;
                *atoc_tmp.get(&cyrillic_file_id)
                    .ok_or_else(|| anyhow::anyhow!("file_id={} not in ATOC", cyrillic_file_id))?
            };
            let cyrillic_mft_entry = &reader.entries()[cyrillic_mft_idx as usize];
            let cyrillic_original_offset = cyrillic_mft_entry.offset;
            let cyrillic_original_size   = cyrillic_mft_entry.size;
            println!("Cyrillic reuses file_id={} (mft={}, offset=0x{:X}, size={}, compressed={})",
                cyrillic_file_id, cyrillic_mft_idx,
                cyrillic_original_offset, cyrillic_original_size, cyrillic_mft_entry.compressed);

            // Build 363-glyph file: 207 Cyrillic + 156 empty 1x1 (for range 11 = 363 total)
            // Range 11 = U+F900..U+FA6B = 363 glyphs
            // Range 3 (Cyrillic) reads first 207 glyphs → correct Cyrillic
            // Range 11 reads all 363 → first 207 Cyrillic + 156 empty (CJK chars show as dots)
            let range11_count = 363usize;
            let mut glyph_data_363 = cyrillic_bytes.clone(); // our 207 Cyrillic glyphs
            // Append 156 empty 1x1 glyphs: yoffset=0, width-1=0, height-1=0, rle_type=0 (all black)
            for _ in 207..range11_count {
                glyph_data_363.extend_from_slice(&[0u8, 0, 0, 0]);
            }
            println!("Glyph file: {} bytes ({} glyphs: 207 Cyrillic + 156 empty)",
                glyph_data_363.len(), range11_count);

            // Compress the 363-glyph file using GW2's format
            println!("Compressing glyph file...");
            let cyrillic_compressed = crate::compress_gw2::compress_gw2(&glyph_data_363)?;
            println!("  Glyph: {} → {} bytes ({:.1}%, must be ≤{})",
                glyph_data_363.len(), cyrillic_compressed.len(),
                cyrillic_compressed.len() as f64 / glyph_data_363.len() as f64 * 100.0,
                cyrillic_original_size);
            // Verify
            let dec = crate::compression::decompress_gw2(&cyrillic_compressed)?;
            if dec != glyph_data_363 { anyhow::bail!("glyph compression roundtrip failed"); }
            println!("  Roundtrip: OK");

            let ptr_val: u32 = 0x2978; // d[40+10616] = d[10656] = CJK Compat ref
            let patched_afnt = patch_afnt_inplace(&afnt_data, font_idx, ptr_val)?;

            // Find the AFNT MFT entry info
            let afnt_entry = &reader.entries()[afnt_index];
            let afnt_offset = afnt_entry.offset;
            let afnt_compressed_size = afnt_entry.size;
            let mft_base = reader.header().mft_offset;
            // ATOC is MFT entry 1
            let atoc_entry = &reader.entries()[1];
            let atoc_offset = atoc_entry.offset;
            let atoc_size   = atoc_entry.size;

            std::fs::create_dir_all(&out)?;
            let cyrillic_path = out.join("cyrillic_glyphs.bin");
            let afnt_path = out.join("afnt_patched.bin");
            let meta_path = out.join("patch_meta.bin");

            // Compress the patched AFNT using GW2's compression format
            println!("Compressing patched AFNT...");
            let afnt_compressed = crate::compress_gw2::compress_gw2(&patched_afnt)?;
            println!("  Compressed: {} → {} bytes ({:.1}%)",
                patched_afnt.len(), afnt_compressed.len(),
                afnt_compressed.len() as f64 / patched_afnt.len() as f64 * 100.0);

            // Verify roundtrip
            let decompressed = crate::compression::decompress_gw2(&afnt_compressed)?;
            if decompressed != patched_afnt {
                anyhow::bail!("Compression roundtrip verification FAILED!");
            }
            println!("  Roundtrip verification: OK");

            std::fs::write(&cyrillic_path, &cyrillic_compressed)?;
            std::fs::write(&afnt_path, &afnt_compressed)?;
            // Also write uncompressed version for proxies that serve raw data
            std::fs::write(out.join("afnt_patched_raw.bin"), &patched_afnt)?;

            // Write metadata: afnt_offset(u64), afnt_compressed_size(u32), mft_base(u64),
            //                 afnt_mft_index(u32), cyrillic_file_id(u32),
            //                 atoc_offset(u64), atoc_size(u32)
            let mut meta = Vec::new();
            meta.extend_from_slice(&afnt_offset.to_le_bytes());
            meta.extend_from_slice(&afnt_compressed_size.to_le_bytes());
            meta.extend_from_slice(&mft_base.to_le_bytes());
            meta.extend_from_slice(&(afnt_index as u32).to_le_bytes());
            meta.extend_from_slice(&cyrillic_file_id.to_le_bytes());
            meta.extend_from_slice(&atoc_offset.to_le_bytes());
            meta.extend_from_slice(&atoc_size.to_le_bytes());
            meta.extend_from_slice(&reader.header().mft_size.to_le_bytes());
            // Extra fields for new approach:
            meta.extend_from_slice(&cyrillic_mft_idx.to_le_bytes());
            meta.extend_from_slice(&cyrillic_original_offset.to_le_bytes());
            meta.extend_from_slice(&cyrillic_original_size.to_le_bytes());
            meta.extend_from_slice(&(afnt_compressed.len() as u32).to_le_bytes());
            std::fs::write(&meta_path, &meta)?;

            println!("Generated:");
            println!("  {} ({} bytes) - Cyrillic glyph data", cyrillic_path.display(), cyrillic_bytes.len());
            println!("  {} ({} bytes) - patched AFNT", afnt_path.display(), patched_afnt.len());
            println!("  {} - patch metadata", meta_path.display());
            println!("\nAFNT: offset=0x{:X} compressed_size={}", afnt_offset, afnt_compressed_size);
            println!("MFT base: 0x{:X}", mft_base);
            println!("AFNT MFT entry at: 0x{:X}", mft_base + 24 + afnt_index as u64 * 24);
            println!("Cyrillic file_id: 0x{:X}", cyrillic_file_id);
        }

        Command::RepackAfnt { dir, file_id, font: font_idx } => {
            let raw_path = dir.join("afnt_patched_raw.bin");
            let out_path = dir.join("afnt_patched.bin");

            let mut afnt_data = std::fs::read(&raw_path)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", raw_path.display(), e))?;
            println!("Read {} bytes from {}", afnt_data.len(), raw_path.display());

            // data_start = PF(12) + chunk_header(16) = 28
            let data_start = 28usize;
            let d_len = afnt_data.len() - data_start;

            // fileNames[3] for font[font_idx] is at d[12 + font_idx*68 + 16 + 3*4] = d[40] for font 0
            let font_array_start = 12usize;
            let font_desc_size = 68usize;
            let filename3_offset_in_d = font_array_start + font_idx * font_desc_size + 16 + 3 * 4;
            let field_pos = data_start + filename3_offset_in_d;

            // New ANetFileReference will be appended at end of current data
            let new_ref_offset_in_d = d_len;
            let new_ptr_val = new_ref_offset_in_d - filename3_offset_in_d;
            if new_ptr_val > u32::MAX as usize { bail!("ptr_val too large"); }

            println!("d_len={}, filename3_offset_in_d={}, new_ref_offset_in_d={}, ptr_val=0x{:X}",
                d_len, filename3_offset_in_d, new_ref_offset_in_d, new_ptr_val);

            // Write new ptr_val at fileNames[3]
            let old_ptr = u32::from_le_bytes(afnt_data[field_pos..field_pos+4].try_into().unwrap());
            println!("Old ptr_val=0x{:X} → new ptr_val=0x{:X}", old_ptr, new_ptr_val);
            afnt_data[field_pos..field_pos+4].copy_from_slice(&(new_ptr_val as u32).to_le_bytes());

            // Append new 6-byte ANetFileReference for file_id
            let ref_bytes = patch::file_id_to_reference(file_id);
            println!("ANetFileReference for file_id={}: [{:04X}, {:04X}, {:04X}]",
                file_id, ref_bytes[0], ref_bytes[1], ref_bytes[2]);
            afnt_data.extend_from_slice(&ref_bytes[0].to_le_bytes());
            afnt_data.extend_from_slice(&ref_bytes[1].to_le_bytes());
            afnt_data.extend_from_slice(&ref_bytes[2].to_le_bytes());

            // Update chunk_data_size at data[0x10..0x14]
            let new_chunk_data_size = (afnt_data.len() - 12 - 8) as u32;
            afnt_data[0x10..0x14].copy_from_slice(&new_chunk_data_size.to_le_bytes());
            println!("New afnt size: {} bytes, chunk_data_size={}", afnt_data.len(), new_chunk_data_size);

            // Compress
            println!("Compressing...");
            let compressed = crate::compress_gw2::compress_gw2(&afnt_data)?;
            println!("Compressed: {} → {} bytes", afnt_data.len(), compressed.len());

            // Verify roundtrip
            let decompressed = crate::compression::decompress_gw2(&compressed)?;
            if decompressed != afnt_data { bail!("Compression roundtrip FAILED!"); }
            println!("Roundtrip: OK");

            // Also write updated raw
            std::fs::write(raw_path, &afnt_data)?;
            std::fs::write(&out_path, &compressed)?;
            println!("Written: {} ({} bytes)", out_path.display(), compressed.len());
        }

        Command::StrsProbe { file_id } => {
            let atoc = reader.read_atoc()?;
            let mft = match atoc.get(&file_id) { Some(&m) => m as usize, None => { println!("file_id {} not in ATOC", file_id); return Ok(()); } };
            println!("file_id {} -> mft index {}", file_id, mft);
            let data = reader.read_entry(mft)?;
            println!("decompressed len={} magic={:?} lang_byte(data[len-2])={:#x}", data.len(), &data[0..4.min(data.len())], data.get(data.len().wrapping_sub(2)).copied().unwrap_or(0));
            let entries = strs::parse(&data).map_err(|e| anyhow::anyhow!(e))?;
            let raw = entries.iter().filter(|e| e.is_raw()).count();
            let mut enc_hist: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
            for e in &entries { *enc_hist.entry(e.enc).or_default() += 1; }
            println!("entries={} raw={} enc_hist={:?}", entries.len(), raw, enc_hist);
            for (i, e) in entries.iter().enumerate().take(6) {
                println!("  [{}] size={} count={} enc={:#x} text={:?}", i, e.size, e.count, e.enc, e.text());
            }
        }
        Command::StrsExportAll { out, max_len, language } => {
            fn csv_field(s: &str) -> String {
                if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
            }
            let total = reader.entry_count();
            eprintln!("Scanning {} MFT entries for English strs files...", total);
            let mut dict: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut strs_files = 0usize;
            let mut lang_files = 0usize;
            for i in 0..total {
                if i % 50000 == 0 && i > 0 {
                    eprintln!("  {}/{} ({} strs, {} {}-lang, {} unique strings)", i, total, strs_files, lang_files,
                        match language { 0=>"EN",2=>"FR",3=>"DE",4=>"ES",_=>"?" }, dict.len());
                }
                // Skip empty MFT slots cheaply (no decompression).
                if reader.entries().get(i).map(|e| e.offset == 0 || e.size < 8).unwrap_or(true) { continue; }
                let data = match reader.read_entry(i) { Ok(d) => d, Err(_) => continue };
                if data.len() < 6 || &data[0..4] != b"strs" { continue; }
                strs_files += 1;
                // Language byte is the 2nd-to-last byte (0=English, 2=French, ...).
                if data[data.len() - 2] != language { continue; }
                lang_files += 1;
                let entries = match strs::parse(&data) { Ok(e) => e, Err(_) => continue };
                for e in &entries {
                    if let Some(t) = e.text() {
                        if t.is_empty() { continue; }
                        let n = t.chars().count();
                        if n < 1 || n > max_len { continue; }
                        dict.insert(t);
                    }
                }
            }
            let mut buf = String::from("english,russian\n");
            for s in &dict { buf.push_str(&csv_field(s)); buf.push_str(",\n"); }
            std::fs::write(&out, &buf)?;
            println!("Done: {} strs files, {} in target language, {} unique raw strings -> {}",
                strs_files, lang_files, dict.len(), out.display());
        }
        Command::StrsTemplate { indices_file, out, max_len } => {
            let raw_idx = std::fs::read_to_string(&indices_file)?;
            let indices: Vec<usize> = raw_idx
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .collect();
            println!("Scanning {} strs files for English content...", indices.len());

            let is_ascii_text = |s: &str| s.chars().all(|c| (c as u32) < 0x80);
            let letters = |s: &str| s.chars().filter(|c| c.is_ascii_alphabetic()).count();
            // A string carries a Latin accent (é à ä ö ü ñ ç ß … U+00C0..U+017F) used by FR/DE/ES but
            // ~never by English. A file with a non-trivial fraction of such strings is NOT English.
            let has_accent = |s: &str| s.chars().any(|c| { let u = c as u32; u >= 0xC0 && u <= 0x17F });

            let mut dict: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut en_files = 0usize;
            let mut total_raw = 0usize;
            for &idx in &indices {
                let data = match reader.read_entry(idx) { Ok(d) => d, Err(_) => continue };
                let entries = match strs::parse(&data) { Ok(e) => e, Err(_) => continue };
                let raws: Vec<String> = entries.iter().filter_map(|e| e.text()).filter(|t| !t.is_empty()).collect();
                if raws.is_empty() { continue; }
                // No per-file language gate: collect ALL ASCII-only raw strings from every file (the
                // per-string ASCII filter below drops accented FR/DE/ES; English/ASCII remains).
                // This yields the COMPLETE raw corpus rather than a heuristically-classified subset.
                en_files += 1;
                for t in raws {
                    total_raw += 1;
                    if !is_ascii_text(&t) { continue; }
                    let chars = t.chars().count();
                    if chars < 2 || chars > max_len { continue; }
                    if letters(&t) < 2 { continue; } // skip pure punctuation/codes
                    dict.insert(t);
                }
            }
            // Write a CSV template: header + `"English",` rows (empty Russian to fill in).
            fn csv_field(s: &str) -> String {
                if s.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.to_string()
                }
            }
            let mut buf = String::new();
            buf.push_str("english,russian\n");
            for s in &dict {
                buf.push_str(&csv_field(s));
                buf.push_str(",\n");
            }
            std::fs::write(&out, &buf)?;
            println!("English files: {}, raw strings seen: {}, unique template entries: {}", en_files, total_raw, dict.len());
            println!("Wrote template to {}", out.display());
        }
        Command::StrsDump { index, filter, limit, raw_only } => {
            let data = reader.read_entry(index)?;
            let entries = strs::parse(&data).map_err(|e| anyhow::anyhow!(e))?;
            let raw = entries.iter().filter(|e| e.is_raw()).count();
            let huff = entries.len() - raw;
            let mut enc_hist: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
            for e in &entries { *enc_hist.entry(e.enc).or_default() += 1; }
            println!("strs entry {} ({} bytes): {} entries — {} raw UTF-16, {} Huffman",
                index, data.len(), entries.len(), raw, huff);
            println!("encoding tags: {:?}", enc_hist);
            println!("---");
            let mut printed = 0usize;
            for (i, e) in entries.iter().enumerate() {
                if raw_only && !e.is_raw() { continue; }
                let text = e.text();
                if let Some(f) = &filter {
                    match &text {
                        Some(t) if t.contains(f.as_str()) => {}
                        _ => continue,
                    }
                }
                match text {
                    Some(t) => println!("[{:>5}] enc={:#04x} size={:>4} {:?}", i, e.enc, e.size, t),
                    None => println!("[{:>5}] enc={:#04x} size={:>4} count={} <huffman>", i, e.enc, e.size, e.count),
                }
                printed += 1;
                if let Some(l) = limit { if printed >= l { break; } }
            }
            println!("--- printed {} ---", printed);
        }
        Command::ValidateAtlases { afnt_index, ttf, dump_font } => {
            let afnt_data = reader.read_entry(afnt_index)?;
            let fonts = afnt::parse_afnt(&afnt_data)?;
            let ttf_bytes = std::fs::read(&ttf)?;
            let atoc = reader.read_atoc()?;
            if let Some(fi) = dump_font {
                let fnt = &fonts[fi];
                let px = fnt.extents_y as f32 * patch::PX_FACTOR;
                let glyphs = patch::build_cyrillic_glyphs(&ttf_bytes, px, fnt.extents_y as u32)?;
                let (rstart, _, _) = afnt::GLYPH_RANGES[3];
                let mut maxw = 0u32; let mut maxh = 0u32; let mut uniform = 0; let mut empty = 0;
                for (gi, g) in glyphs.iter().enumerate() {
                    maxw = maxw.max(g.width); maxh = maxh.max(g.height);
                    let pl = g.pixels.len(); let expect = (g.width * g.height) as usize;
                    let all_same = g.pixels.windows(2).all(|w| w[0]==w[1]) && !g.pixels.is_empty();
                    if g.width==1 && g.height==1 && g.pixels==[0] { empty += 1; }
                    else if all_same { uniform += 1; }
                    if pl != expect || g.width > 255 || g.height > 255 {
                        println!("  glyph#{} U+{:04X} {}x{} pixels.len={} expect={} {}",
                            gi, rstart + gi as u32, g.width, g.height, pl, expect,
                            if g.width>255||g.height>255 {"OVERSIZE"} else {"LEN-MISMATCH"});
                    }
                }
                println!("font[{}] extents {}x{} px={:.1}: maxglyph {}x{}, empty={}, uniform(non-empty)={}",
                    fi, fnt.extents_x, fnt.extents_y, px, maxw, maxh, empty, uniform);
                // Direct fontdue probe at a large size for a few codepoints.
                let fdfont = fontdue::Font::from_bytes(ttf_bytes.as_slice(), fontdue::FontSettings::default());
                match fdfont {
                    Ok(f) => {
                        for &(cp, name) in &[(0x41u32, "Latin A"), (0x391, "Greek Alpha"), (0x410, "Cyr A"), (0x43F, "Cyr p"), (0x440, "Cyr r")] {
                            let ch = char::from_u32(cp).unwrap();
                            let idx = f.lookup_glyph_index(ch);
                            let (m, bm) = f.rasterize(ch, 40.0);
                            println!("  U+{:04X} {}: glyph_index={} raster {}x{} ink={}",
                                cp, name, idx, m.width, m.height, bm.iter().any(|&p| p != 0));
                        }
                    }
                    Err(e) => println!("  fontdue FAILED to load font: {}", e),
                }
                return Ok(());
            }
            let glyph_count = (afnt::GLYPH_RANGES[3].1 - afnt::GLYPH_RANGES[3].0) as usize; // 207
            let (rstart, rend, _) = afnt::GLYPH_RANGES[3];
            let mut bad = 0usize;
            for (i, fnt) in fonts.iter().enumerate() {
                let r11 = fnt.file_ids[11];
                if r11 == 0 { continue; }
                let gen_size = match atoc.get(&r11) {
                    Some(&m) => reader.read_entry(m as usize)?.len(),
                    None => continue,
                };
                let px = fnt.extents_y as f32 * patch::PX_FACTOR;
                let glyphs = patch::build_cyrillic_glyphs(&ttf_bytes, px, fnt.extents_y as u32)?;
                // Flag glyphs whose rasterized dims exceed the 1-byte header (clamp → pixel-count desync).
                for (gi, g) in glyphs.iter().enumerate() {
                    if g.width > 256 || g.height > 256 {
                        println!("  font[{}] {}x{} glyph#{} U+{:04X} OVERSIZE {}x{} (header clamps -> DESYNC)",
                            i, fnt.extents_x, fnt.extents_y, gi, rstart + gi as u32, g.width, g.height);
                        bad += 1;
                    }
                }
                let real_full = patch::encode_glyph_file(&glyphs);
                if real_full.len() > gen_size { continue; } // skipped font (no atlas) — fine
                // Build the atlas exactly like GenAtlases (filler-pad to gen_size).
                let atlas: Vec<u8> = if real_full.len() == gen_size {
                    real_full
                } else {
                    const MAXF: usize = 65000;
                    let mut nfill = 1usize;
                    loop {
                        if nfill >= glyph_count { bad += 1; println!("  font[{}] cannot pad", i); break Vec::new(); }
                        let real_count = glyph_count - nfill;
                        let prefix = patch::encode_glyph_file(&glyphs[0..real_count]);
                        if prefix.len() + 6 * nfill > gen_size { nfill += 1; continue; }
                        let total = gen_size - prefix.len();
                        if total > MAXF * nfill { nfill += 1; continue; }
                        let mut a = prefix;
                        let mut need = total;
                        for k in 0..nfill {
                            let rem = nfill - k;
                            let this = if rem == 1 { need } else { MAXF.min(need - 6 * (rem - 1)) };
                            a.extend(patch::make_filler_glyph(this));
                            need -= this;
                        }
                        break a;
                    }
                };
                if atlas.is_empty() { continue; }
                // Decode it back the way GW2 does: walk 207 glyphs, check we consume EXACTLY gen_size.
                match afnt::decode_glyph_file(&atlas, rstart, rend) {
                    Ok(_) => {
                        // decode_glyph_file doesn't report final pos; re-walk to get it.
                        let consumed = walk_consumed(&atlas, glyph_count);
                        match consumed {
                            Some(p) if p == gen_size => {}
                            Some(p) => { bad += 1; println!("  font[{}] {}x{} r11={} LEFTOVER: consumed {} of {} bytes (tail desync)", i, fnt.extents_x, fnt.extents_y, r11, p, gen_size); }
                            None => { bad += 1; println!("  font[{}] {}x{} r11={} WALK FAILED", i, fnt.extents_x, fnt.extents_y, r11); }
                        }
                    }
                    Err(e) => { bad += 1; println!("  font[{}] {}x{} r11={} DECODE ERROR: {}", i, fnt.extents_x, fnt.extents_y, r11, e); }
                }
            }
            if bad == 0 { println!("All atlases structurally clean."); }
            else { println!("{} problem(s) found.", bad); }
        }
        Command::PatchCyrillic { afnt_index, ttf, font: font_idx, px, dry_run } => {
            cmd_patch_cyrillic(&mut reader, afnt_index, &ttf, font_idx, px, dry_run, &cli.dat)?;
        }

        Command::GenAtlases { afnt_index, ttf, out } => {
            let afnt_data = reader.read_entry(afnt_index)?;
            let fonts = afnt::parse_afnt(&afnt_data)?;
            let ttf_bytes = std::fs::read(&ttf)?;
            println!("Generating Cyrillic atlases for {} fonts...", fonts.len());

            let mut blob: Vec<u8> = Vec::new();
            let mut count: u32 = 0;
            let mut body: Vec<u8> = Vec::new();
            let atoc = reader.read_atoc()?;
            let glyph_count = (afnt::GLYPH_RANGES[3].1 - afnt::GLYPH_RANGES[3].0) as usize; // 207
            for (i, fnt) in fonts.iter().enumerate() {
                let r11 = fnt.file_ids[11];
                if r11 == 0 { continue; } // only fonts that have range 11 (which range 3 reuses)
                let px = fnt.extents_y as f32 * patch::PX_FACTOR;
                let glyphs = patch::build_cyrillic_glyphs(&ttf_bytes, px, fnt.extents_y as u32)?;
                let real_full = patch::encode_glyph_file(&glyphs);
                // GW2 parses the WHOLE decompressed buffer (genuine size). Our atlas must fill it
                // EXACTLY with `glyph_count` glyphs — else stale tail desyncs FntRle (m_rleType crash).
                let gen_size = match atoc.get(&r11) {
                    Some(&m) => reader.read_entry(m as usize)?.len(),
                    None => { eprintln!("  font[{}] r11={} not in ATOC, skip", i, r11); continue; }
                };
                let atlas: Vec<u8> = if real_full.len() == gen_size {
                    real_full
                } else if real_full.len() > gen_size {
                    // Genuine range-11 buffer too small for our Cyrillic — skip this font. version.dll
                    // must then NOT patch this font's range-3 (no atlas → would hit genuine CJK → crash).
                    eprintln!("  font[{}] {}x{} r11={} SKIP (atlas {}b > genuine {}b)", i, fnt.extents_x, fnt.extents_y, r11, real_full.len(), gen_size);
                    continue;
                } else {
                    // Replace the last `nfill` glyphs with fillers so the atlas is exactly `gen` bytes
                    // (keeping `glyph_count` total). Filler glyphs are valid type-1 (≤65000 bytes each).
                    const MAXF: usize = 65000;
                    let mut nfill = 1usize;
                    loop {
                        if nfill >= glyph_count { anyhow::bail!("font {} cannot pad to {}b", i, gen_size); }
                        let real_count = glyph_count - nfill;
                        let prefix = patch::encode_glyph_file(&glyphs[0..real_count]);
                        if prefix.len() + 6 * nfill > gen_size { nfill += 1; continue; }
                        let total = gen_size - prefix.len();
                        if total > MAXF * nfill { nfill += 1; continue; }
                        let mut a = prefix;
                        let mut need = total;
                        for k in 0..nfill {
                            let rem = nfill - k;
                            let this = if rem == 1 { need } else { MAXF.min(need - 6 * (rem - 1)) };
                            a.extend(patch::make_filler_glyph(this));
                            need -= this;
                        }
                        assert_eq!(a.len(), gen_size, "font {} pad mismatch", i);
                        break a;
                    }
                };
                body.extend_from_slice(&r11.to_le_bytes());
                body.extend_from_slice(&(atlas.len() as u32).to_le_bytes());
                body.extend_from_slice(&atlas);
                count += 1;
                if count <= 6 {
                    println!("  font[{}] {}x{} r11={} atlas {}b == gen {}b", i, fnt.extents_x, fnt.extents_y, r11, atlas.len(), gen_size);
                }
            }
            blob.extend_from_slice(&count.to_le_bytes());
            blob.extend_from_slice(&body);
            std::fs::write(&out, &blob)?;
            println!("Wrote {} atlases ({} bytes) to {}", count, blob.len(), out.display());
        }
    }

    Ok(())
}

/// Walk a glyph stream the way GW2 does and return the byte offset consumed after `count` glyphs,
/// or None if a run overruns its glyph (the desync that trips FntRle m_rleType).
fn walk_consumed(data: &[u8], count: usize) -> Option<usize> {
    let mut pos = 0usize;
    for _ in 0..count {
        if pos + 4 > data.len() { return None; }
        let w = data[pos + 1] as usize + 1;
        let h = data[pos + 2] as usize + 1;
        let rle_type = data[pos + 3];
        pos += 4;
        let mut remaining = w * h;
        if rle_type == 0 || rle_type == 255 {
            // alternating runs, no per-pixel literal byte
            while remaining > 0 {
                let mut adv = 1usize;
                loop {
                    if pos >= data.len() { return None; }
                    let b = data[pos] as usize; pos += 1; adv += b;
                    if b != 255 { break; }
                }
                if adv > remaining { return None; }
                remaining -= adv;
            }
        } else {
            while remaining > 0 {
                if pos >= data.len() { return None; }
                let v = data[pos]; pos += 1;
                if v != 0 && v != 255 {
                    remaining -= 1; // single literal pixel
                } else {
                    let mut adv = 1usize;
                    loop {
                        if pos >= data.len() { return None; }
                        let b = data[pos] as usize; pos += 1; adv += b;
                        if b != 255 { break; }
                    }
                    if adv > remaining { return None; }
                    remaining -= adv;
                }
            }
        }
    }
    Some(pos)
}

fn cmd_patch_cyrillic(
    reader: &mut dat::DatReader,
    afnt_index: usize,
    ttf_path: &std::path::Path,
    font_idx: usize,
    px_override: Option<f32>,
    dry_run: bool,
    dat_path: &std::path::Path,
) -> Result<()> {
    // 1. Load AFNT and pick the font descriptor
    let afnt_data = reader.read_entry(afnt_index)?;
    let fonts = afnt::parse_afnt(&afnt_data)?;

    let fnt = fonts.get(font_idx)
        .ok_or_else(|| anyhow::anyhow!("font index {} out of range (total: {})", font_idx, fonts.len()))?;

    if fnt.file_ids[3] != 0 {
        println!("Font [{}] already has Cyrillic (file_id={}). Nothing to do.", font_idx, fnt.file_ids[3]);
        return Ok(());
    }

    let px = px_override.unwrap_or(fnt.extents_y as f32 * patch::PX_FACTOR);
    println!("Font [{}]: extents {}x{}, rasterizing at {:.1}px", font_idx, fnt.extents_x, fnt.extents_y, px);

    // 2. Rasterize Cyrillic from TTF
    println!("Loading TTF: {}", ttf_path.display());
    let ttf_bytes = std::fs::read(ttf_path)?;
    let glyph_bytes = patch::build_cyrillic_glyph_file(&ttf_bytes, px, fnt.extents_y as u32)?;
    println!("  Encoded glyph file: {} bytes", glyph_bytes.len());

    if dry_run {
        println!("\n[DRY RUN] Would write {} bytes of Cyrillic glyph data to .dat", glyph_bytes.len());
        println!("[DRY RUN] Would update AFNT font[{}] Cyrillic range reference", font_idx);

        // Preview a few glyphs
        preview_cyrillic(&glyph_bytes)?;
        return Ok(());
    }

    // 3. Find a free MFT slot or use an overflow slot
    println!("Reading ATOC...");
    let atoc = reader.read_atoc()?;
    println!("ATOC: {} entries", atoc.len());

    // Pick a new file_id: max existing + 1
    let new_file_id = atoc.keys().copied().max().unwrap_or(0) + 1;
    println!("New file_id: {}", new_file_id);

    // Find free MFT slot (offset=0, size=0)
    let free_slot = patch::find_free_mft_slot(reader.entries())
        .ok_or_else(|| anyhow::anyhow!("No free MFT slot found — all entries are occupied"))?;
    println!("Using MFT slot: {}", free_slot);

    // 4. Write glyph data to .dat at the end of file (append)
    use std::io::{Seek, SeekFrom, Write};
    let dat_len = {
        let f = std::fs::OpenOptions::new().read(true).open(dat_path)?;
        f.metadata()?.len()
    };

    println!("Appending {} bytes at offset 0x{:X}", glyph_bytes.len(), dat_len);
    {
        let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
        f.seek(SeekFrom::End(0))?;
        f.write_all(&glyph_bytes)?;
        f.flush()?;
    }

    // 5. Update MFT entry for the free slot
    //    Each MFT entry: offset(u64) + size(u32) + compression_flag(u16) + flags(u16) + counter(u32) + crc(u32)
    //    = 24 bytes. Entry N is at: mft_offset + 24 (header) + N * 24
    let mft_entry_offset = reader.header().mft_offset + 24 + free_slot as u64 * 24;
    println!("Writing MFT entry at 0x{:X}", mft_entry_offset);
    {
        let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
        f.seek(SeekFrom::Start(mft_entry_offset))?;
        // offset
        f.write_all(&dat_len.to_le_bytes())?;
        // size
        f.write_all(&(glyph_bytes.len() as u32).to_le_bytes())?;
        // compression_flag = 0 (uncompressed)
        f.write_all(&0u16.to_le_bytes())?;
        // flags = 0
        f.write_all(&0u16.to_le_bytes())?;
        // counter = 0
        f.write_all(&0u32.to_le_bytes())?;
        // crc = 0 (game doesn't verify)
        f.write_all(&0u32.to_le_bytes())?;
        f.flush()?;
    }

    // 6. Write new ATOC entry (append at end of ATOC block)
    //    ATOC is at entries[1]: raw (id, mft_index_1based) pairs
    {
        let atoc_entry = reader.entries().get(1)
            .ok_or_else(|| anyhow::anyhow!("no ATOC entry"))?;
        let _atoc_end = atoc_entry.offset + atoc_entry.size as u64;
        // The ATOC entries might not be at the very end of their block.
        // We overwrite the first zero-id entry or append 8 bytes.
        // Simplest: write at end of ATOC block (if there's room) — but this is risky.
        // Instead we find an existing zero-pair in ATOC and overwrite it.
        let atoc_raw = {
            let mut f = std::fs::OpenOptions::new().read(true).open(dat_path)?;
            f.seek(SeekFrom::Start(atoc_entry.offset))?;
            let mut buf = vec![0u8; atoc_entry.size as usize];
            use std::io::Read;
            f.read_exact(&mut buf)?;
            buf
        };

        // Find a zero-pair slot in ATOC
        let mut zero_pos: Option<u64> = None;
        let mut pos = 0usize;
        while pos + 8 <= atoc_raw.len() {
            let id = u32::from_le_bytes(atoc_raw[pos..pos+4].try_into().unwrap());
            let mft = u32::from_le_bytes(atoc_raw[pos+4..pos+8].try_into().unwrap());
            if id == 0 && mft == 0 {
                zero_pos = Some(atoc_entry.offset + pos as u64);
                break;
            }
            pos += 8;
        }

        let slot = zero_pos.ok_or_else(|| anyhow::anyhow!("No free slot in ATOC — cannot add mapping"))?;
        println!("Writing ATOC entry at 0x{:X}: id={} mft={}", slot, new_file_id, free_slot + 1);

        let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
        f.seek(SeekFrom::Start(slot))?;
        f.write_all(&new_file_id.to_le_bytes())?;
        f.write_all(&((free_slot + 1) as u32).to_le_bytes())?; // 1-based
        f.flush()?;
    }

    // 7. Patch AFNT: update font[font_idx] to reference the new file
    //    We need to rebuild the AFNT data with the Cyrillic fileNames[3] set
    let new_afnt = patch_afnt_cyrillic(&afnt_data, font_idx, new_file_id)?;
    println!("Patched AFNT: {} -> {} bytes", afnt_data.len(), new_afnt.len());

    // Write new AFNT back to MFT entry (or append if larger)
    let afnt_mft = reader.entries().get(afnt_index)
        .ok_or_else(|| anyhow::anyhow!("AFNT entry not found"))?;

    if new_afnt.len() <= afnt_mft.size as usize {
        // Write in place
        let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
        f.seek(SeekFrom::Start(afnt_mft.offset))?;
        f.write_all(&new_afnt)?;
        println!("AFNT written in-place at 0x{:X}", afnt_mft.offset);
    } else {
        // Append new AFNT and update MFT entry
        let dat_len2 = {
            let f = std::fs::OpenOptions::new().read(true).open(dat_path)?;
            f.metadata()?.len()
        };
        {
            let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
            f.seek(SeekFrom::End(0))?;
            f.write_all(&new_afnt)?;
            f.flush()?;
        }
        // Update MFT for afnt_index
        let entry_off = reader.header().mft_offset + 24 + afnt_index as u64 * 24;
        let mut f = std::fs::OpenOptions::new().write(true).open(dat_path)?;
        f.seek(SeekFrom::Start(entry_off))?;
        f.write_all(&dat_len2.to_le_bytes())?;
        f.write_all(&(new_afnt.len() as u32).to_le_bytes())?;
        f.write_all(&0u16.to_le_bytes())?; // not compressed
        f.write_all(&0u16.to_le_bytes())?;
        f.write_all(&0u32.to_le_bytes())?;
        f.write_all(&0u32.to_le_bytes())?;
        f.flush()?;
        println!("AFNT appended at 0x{:X} and MFT updated", dat_len2);
    }

    println!("\nDone! Cyrillic glyphs patched into font[{}].", font_idx);
    println!("Restart GW2 to see the changes.");
    Ok(())
}

/// In-place patch: set fileNames[3] of font[font_idx] to ptr_val (pointing to existing ref).
/// Does NOT append any bytes — file size stays the same.
fn patch_afnt_inplace(afnt_data: &[u8], font_idx: usize, ptr_val: u32) -> Result<Vec<u8>> {
    let data_start = 12 + 16usize;
    let font_array_start = 12usize;
    let font_desc_size = 68usize;
    let filename3_offset_in_d = font_array_start + font_idx * font_desc_size + 16 + 3 * 4;
    let field_pos = data_start + filename3_offset_in_d;

    let mut new_data = afnt_data.to_vec();
    new_data[field_pos..field_pos + 4].copy_from_slice(&ptr_val.to_le_bytes());
    // chunkDataSize unchanged (no size change)
    Ok(new_data)
}

/// Patch the AFNT binary data to set fileNames[3] for the given font to reference new_file_id.
fn patch_afnt_cyrillic(afnt_data: &[u8], font_idx: usize, new_file_id: u32) -> Result<Vec<u8>> {
    // The AFNT is stored uncompressed (or we decompressed it).
    // We need to rebuild it with the new ANetFileReference appended and fileNames[3] pointing to it.
    //
    // Layout recap (all offsets within the decompressed data):
    //   0x00..0x0C  PF header (12 bytes)
    //   0x0C..0x1C  Chunk header (16 bytes): type + dataSize + version + chunkHeaderSize + offsetTable
    //   0x1C        Chunk data start:
    //     d[0..4]   fontCount
    //     d[4..8]   ptr_to_font_array (self-relative from d[4])
    //     d[12..]   FontDescriptor[fontCount] (font_array_start = 4 + 8 = 12)
    //
    // FontDescriptor for font[i] at d[12 + i*68]:
    //   +0..+14   unknown
    //   +14       extents_x
    //   +15       extents_y
    //   +16..+68  fileNames[13] (4 bytes each, self-relative)
    //
    // fileNames[3] is at d[12 + i*68 + 16 + 3*4] = d[12 + i*68 + 28]
    //
    // Strategy: append a new ANetFileReference at end of chunk data,
    // write a self-relative offset into fileNames[3].

    let data_start = 12 + 16usize; // PF(12) + chunk_header(16) = 28
    let d_len = afnt_data.len() - data_start;

    // font_array_start within d = 12 (self-relative ptr at d[4] = 8, so 4+8=12)
    let font_array_start = 12usize;
    let font_desc_size = 68usize;
    let filename3_offset_in_d = font_array_start + font_idx * font_desc_size + 16 + 3 * 4;

    // The new ANetFileReference will be appended at the end of the current data
    let ref_offset_in_d = d_len; // will be at end
    let ref_bytes = patch::file_id_to_reference(new_file_id);

    // Self-relative value: from &d[filename3_offset_in_d] to ref_offset_in_d
    let ptr_val = ref_offset_in_d
        .checked_sub(filename3_offset_in_d)
        .ok_or_else(|| anyhow::anyhow!("ANetFileReference would be before fileNames field"))?;

    if ptr_val > u32::MAX as usize {
        bail!("ptr_val {} too large for u32", ptr_val);
    }

    let mut new_data = afnt_data.to_vec();

    // Write self-relative pointer into fileNames[3]
    let field_pos = data_start + filename3_offset_in_d;
    let ptr_bytes = (ptr_val as u32).to_le_bytes();
    new_data[field_pos..field_pos + 4].copy_from_slice(&ptr_bytes);

    // Append ANetFileReference (6 bytes: p0, p1, p2 as u16 LE)
    new_data.extend_from_slice(&ref_bytes[0].to_le_bytes());
    new_data.extend_from_slice(&ref_bytes[1].to_le_bytes());
    new_data.extend_from_slice(&ref_bytes[2].to_le_bytes());

    // Update chunk data size in chunk header (at data[0x10..0x14])
    // chunkDataSize = total_size - 0x0C(PF) - 8(chunkType+chunkDataSize field itself)
    let new_chunk_data_size = (new_data.len() - 12 - 8) as u32;
    new_data[0x10..0x14].copy_from_slice(&new_chunk_data_size.to_le_bytes());

    // Update offsetTableOffset in chunk header to point past the font data
    // (we set it to 0 since we're not maintaining offset tables)
    // Actually leave it as-is or set to new end
    // For safety, update it to point to just after the font array
    // (game may not need this for AFNT parsing)

    Ok(new_data)
}

fn preview_cyrillic(glyph_bytes: &[u8]) -> Result<()> {
    use crate::afnt::{decode_glyph_file, GLYPH_RANGES};
    let (start, end, _) = GLYPH_RANGES[3];
    let glyphs = decode_glyph_file(glyph_bytes, start, end)?;
    println!("\nPreview of first 20 Cyrillic glyphs:");
    for g in glyphs.iter().take(20) {
        if let Some(ch) = char::from_u32(g.codepoint) {
            println!("  U+{:04X} '{}' {}x{} yoff={}", g.codepoint, ch, g.width, g.height, g.y_offset);
        }
    }
    Ok(())
}

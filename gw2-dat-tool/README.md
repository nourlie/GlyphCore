# gw2-dat-tool

A Rust CLI for reading **Guild Wars 2**'s `Gw2.dat` archive: list / extract MFT
entries, parse `AFNT` font chunks, render glyph atlases to PNG, and **generate
the Cyrillic glyph data** consumed by [`version-proxy`](../version-proxy/).

It never writes to `Gw2.dat`; it only reads, and emits new files elsewhere.

## Build

```powershell
cargo build --release
# binary: target/release/gw2-dat-tool.exe
```

All commands take a global `--dat <path-to-Gw2.dat>`.

## Commands

| Command | Description |
|---------|-------------|
| `info` | Print archive header: version, chunk size, MFT offset/size, entry count. |
| `list [--filter <s>]` | List MFT entries (index, offset, compression, size, type), optionally filtered by type substring. |
| `dump <index> [--bytes N]` | Hex-dump the first `N` raw (still-compressed) bytes of an entry. |
| `extract <index> --out <file>` | Decompress one entry and write it to `<file>`. |
| `scan [--filter <s>] [--limit N]` | Decompress entries to discover their *real* type (slow; the MFT type is unreliable). |
| `font-info <index>` | Parse an `AFNT` entry: list every font descriptor and its per-range `file_id`s. |
| `font-extract <index> --out <dir> [--font N] [--latin-only] [--cols 16]` | Render an `AFNT` font's glyph ranges to PNG atlases. |
| `gen-patch <afnt_index> --ttf <ttf> [--font 0] [--out .]` | Produce the file-injection blobs (`afnt_patched.bin`, `cyrillic_glyphs.bin`, `patch_meta.bin`) — the older/secondary delivery path. |
| `gen-atlases <afnt_index> --ttf <ttf> [--out cyrillic_atlases.bin]` | **Main output.** Render a correctly-sized Cyrillic atlas for *every* font that has a `range11`, packed into one blob for the in-memory injector. |
| `repack-afnt [--dir patch_output] [--file-id 3631839] [--font 0]` | Rewrite `fileNames[3]` in `afnt_patched_raw.bin` to use a fresh `ANetFileReference`, then recompress. |
| `patch-cyrillic <afnt_index> --ttf <ttf> [--font 0] [--px F] [--dry-run]` | Older direct AFNT-patching path (kept for reference). |

### Typical usage

```powershell
# Find the AFNT chunk (slow — ~800k entries):
gw2-dat-tool --dat "<...>\Gw2.dat" scan --filter AFNT

# Inspect it:
gw2-dat-tool --dat "<...>\Gw2.dat" font-info <AFNT_INDEX>

# Generate the Cyrillic atlases the proxy injects:
gw2-dat-tool --dat "<...>\Gw2.dat" gen-atlases <AFNT_INDEX> `
    --ttf "Roboto-VariableFont_wdth,wght.ttf" --out ..\version-proxy\blobs\cyrillic_atlases.bin
```

## The AFNT / glyph format (notes)

- An `AFNT` chunk is a `PF…AFNT` container of **156 font descriptors** (68 bytes
  each). Each descriptor holds per-range `fileNames[i]` references (6-byte
  self-relative `ANetFileReference`s). Decode a reference's `file_id` from its
  three `u16`s `(a,b,c)` as `(a-0x100) + 0xFF00*(b-0x100) + 1`.
- Glyph files are flat glyph streams. A glyph header is
  `[yoff, w-1, h-1, rle_type]`; `rle_type` 0 = all-black run, 255 = all-white
  run, 1 = grayscale stream (literal 1px for byte 1–254, run for 0/255 via
  `[b0+1, 0xFF-continue]`).
- The engine builds **exactly `count`** glyphs where `count` is parsed from the
  file, then walks `firstChar..termChar` — so a `range3` glyph file must contain
  exactly `termChar − firstChar` glyphs (207 for Cyrillic, `U+0391..U+0460`) and
  be padded to the genuine buffer size with filler glyphs.

## Source layout

| File | Role |
|------|------|
| `src/dat.rs` | `Gw2.dat` reader: header, MFT, entry decompression. |
| `src/compression.rs` | GW2 inflate (Huffman/LZ) decoder. |
| `src/compress_gw2.rs` | GW2 compressor (round-trips with the decoder). |
| `src/afnt.rs` | `AFNT` parser: font descriptors, ranges, `file_id`s. |
| `src/patch.rs` | Glyph rasterization + AFNT patching / atlas building. |
| `src/main.rs` | CLI (clap) wiring all of the above. |

Roboto (`Roboto-VariableFont_wdth,wght.ttf`, Apache-2.0) is bundled as the
default glyph source.

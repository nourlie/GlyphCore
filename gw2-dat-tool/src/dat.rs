use anyhow::{anyhow, bail, Result};
use byteorder::{LE, ReadBytesExt};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::compression::decompress_gw2;

const DAT_MAGIC: [u8; 3] = [0x41, 0x4E, 0x1A]; // "AN\x1a"
const MFT_MAGIC: [u8; 4] = [0x4D, 0x66, 0x74, 0x1A]; // "Mft\x1a"
// Размер блока для нешатых файлов: 65532 байт данных + 4 байт CRC
const RAW_BLOCK_DATA: usize = 65532;
const RAW_BLOCK_TOTAL: usize = 65536;

pub struct DatHeader {
    pub version: u8,
    pub chunk_size: u32,
    pub mft_offset: u64,
    pub mft_size: u32,
}

pub struct MftEntry {
    pub offset: u64,
    pub size: u32,
    pub compressed: bool,
    pub _crc: u32,
    pub file_type: Option<String>,
}

pub struct DatReader {
    file: BufReader<File>,
    header: DatHeader,
    entries: Vec<MftEntry>,
}

impl DatReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let header = Self::read_header(&mut reader)?;
        let entries = Self::read_mft(&mut reader, &header)?;

        Ok(Self { file: reader, header, entries })
    }

    pub fn header(&self) -> &DatHeader {
        &self.header
    }

    pub fn entries(&self) -> &[MftEntry] {
        &self.entries
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Читает сырые байты без декомпрессии
    pub fn read_raw(&mut self, index: usize) -> Result<Vec<u8>> {
        let entry = self.entries.get(index).ok_or_else(|| anyhow!("index {} out of range", index))?;
        let offset = entry.offset;
        let size = entry.size as usize;
        self.file.seek(SeekFrom::Start(offset))?;
        let mut raw = vec![0u8; size];
        self.file.read_exact(&mut raw)?;
        Ok(raw)
    }

    /// Парсит ATOC (entry 1) и возвращает Map<id → mft_index (0-based)>
    /// ATOC — сырой массив (id: u32, mftIndex: u32) без заголовка.
    /// mftIndex в файле 1-based, возвращаем 0-based (mftIndex - 1).
    pub fn read_atoc(&mut self) -> Result<std::collections::HashMap<u32, u32>> {
        let entry = self.entries.get(1).ok_or_else(|| anyhow!("no entry 1"))?;
        let offset = entry.offset;
        let size = entry.size as usize;

        // Read raw bytes — ATOC has no compression and no CRC blocks
        self.file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; size];
        self.file.read_exact(&mut data)?;

        let count = size / 8;
        let mut map = std::collections::HashMap::with_capacity(count);
        let mut pos = 0usize;
        while pos + 8 <= data.len() {
            let id        = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
            let mft_index = u32::from_le_bytes(data[pos+4..pos+8].try_into().unwrap());
            if id != 0 && mft_index > 0 {
                // mftIndex is 1-based in ATOC, store as 0-based
                map.insert(id, mft_index - 1);
            }
            pos += 8;
        }
        Ok(map)
    }

    /// Определяет тип файла по FourCC после распаковки (для сжатых записей)
    pub fn read_entry_type(&mut self, index: usize) -> Option<String> {
        let entry = self.entries.get(index)?;
        if entry.offset == 0 || entry.size < 4 {
            return None;
        }
        let offset = entry.offset;
        let size = entry.size as usize;
        let compressed = entry.compressed;

        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut raw = vec![0u8; size];
        self.file.read_exact(&mut raw).ok()?;

        let data = if compressed {
            decompress_gw2(&raw).ok()?
        } else {
            strip_raw_crc_blocks(&raw).ok()?
        };

        if data.len() < 4 {
            return None;
        }

        // PF format: magic(2) + version(2) + reserved(2) + chunkHeaderSize(2) + type(4) at offset 8
        let peek = if data.len() >= 12 && &data[..2] == b"PF" {
            &data[8..12]
        } else {
            &data[..4]
        };
        let s: String = peek.iter().map(|&b| {
            if b.is_ascii_graphic() { b as char } else { '.' }
        }).collect();
        Some(s)
    }

    pub fn read_entry(&mut self, index: usize) -> Result<Vec<u8>> {
        let entry = self.entries.get(index).ok_or_else(|| anyhow!("index {} out of range", index))?;
        let offset = entry.offset;
        let size = entry.size as usize;
        let compressed = entry.compressed;

        self.file.seek(SeekFrom::Start(offset))?;
        let mut raw = vec![0u8; size];
        self.file.read_exact(&mut raw)?;

        if compressed {
            decompress_gw2(&raw)
        } else {
            strip_raw_crc_blocks(&raw)
        }
    }

    fn read_header(r: &mut BufReader<File>) -> Result<DatHeader> {
        let version = r.read_u8()?;

        let mut magic = [0u8; 3];
        r.read_exact(&mut magic)?;
        if magic != DAT_MAGIC {
            bail!("not a GW2 .dat file (bad magic)");
        }

        let header_size = r.read_u32::<LE>()?;
        let _unknown1 = r.read_u32::<LE>()?;
        let chunk_size = r.read_u32::<LE>()?;
        let _crc = r.read_u32::<LE>()?;
        let _unknown2 = r.read_u32::<LE>()?;
        let mft_offset = r.read_u64::<LE>()?;
        let mft_size = r.read_u32::<LE>()?;
        let _flags = r.read_u32::<LE>()?;

        let _ = header_size; // уже за пределами 40 байт, пропускаем

        Ok(DatHeader { version, chunk_size, mft_offset, mft_size })
    }

    fn read_mft(r: &mut BufReader<File>, header: &DatHeader) -> Result<Vec<MftEntry>> {
        r.seek(SeekFrom::Start(header.mft_offset))?;

        // MFT header: magic(4) + unknown1(u64) + nbOfEntries(u32) + unknown2(u32) + unknown3(u32) = 24 bytes
        let mut mft_magic = [0u8; 4];
        r.read_exact(&mut mft_magic)?;
        if mft_magic != MFT_MAGIC {
            bail!("bad MFT magic: {:?}", mft_magic);
        }

        let _unknown1 = r.read_u64::<LE>()?; // skip 8 bytes
        let num_entries = r.read_u32::<LE>()? as usize;
        let _unknown2 = r.read_u32::<LE>()?;
        let _unknown3 = r.read_u32::<LE>()?;

        // nbOfEntries counts the header itself, so actual data entries = num_entries - 1
        let data_entries = num_entries.saturating_sub(1);
        let mut raw_entries: Vec<RawMftEntry> = Vec::with_capacity(data_entries);
        for _ in 0..data_entries {
            raw_entries.push(RawMftEntry::read(r)?);
        }

        // Читаем первые 8 байт каждого файла чтобы определить тип
        let mut entries = Vec::with_capacity(num_entries);
        for raw in &raw_entries {
            let file_type = if raw.offset > 0 && raw.size >= 4 {
                read_fourcc(r, raw.offset).ok()
            } else {
                None
            };

            entries.push(MftEntry {
                offset: raw.offset,
                size: raw.size,
                compressed: raw.compression_flag == 0x0008,
                _crc: raw.crc,
                file_type,
            });
        }

        Ok(entries)
    }
}

struct RawMftEntry {
    offset: u64,
    size: u32,
    compression_flag: u16,
    _entry_flags: u16,
    _counter: u32,
    crc: u32,
}

impl RawMftEntry {
    fn read(r: &mut BufReader<File>) -> Result<Self> {
        Ok(Self {
            offset: r.read_u64::<LE>()?,
            size: r.read_u32::<LE>()?,
            compression_flag: r.read_u16::<LE>()?,
            _entry_flags: r.read_u16::<LE>()?,
            _counter: r.read_u32::<LE>()?,
            crc: r.read_u32::<LE>()?,
        })
    }
}

fn read_fourcc(r: &mut BufReader<File>, offset: u64) -> Result<String> {
    r.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;

    // Попытка читать как ASCII FourCC
    let s: String = buf.iter().map(|&b| {
        if b.is_ascii_graphic() { b as char } else { '.' }
    }).collect();

    Ok(s)
}

fn strip_raw_crc_blocks(data: &[u8]) -> Result<Vec<u8>> {
    // Нешатые файлы: каждые 65536 байт = 65532 данных + 4 байта CRC
    if data.len() <= RAW_BLOCK_DATA {
        return Ok(data.to_vec());
    }

    let mut out = Vec::with_capacity(data.len());
    let mut pos = 0;
    while pos < data.len() {
        let end = (pos + RAW_BLOCK_DATA).min(data.len());
        out.extend_from_slice(&data[pos..end]);
        pos += RAW_BLOCK_TOTAL; // пропускаем 4 байта CRC
    }
    Ok(out)
}

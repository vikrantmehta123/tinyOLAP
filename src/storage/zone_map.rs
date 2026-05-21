use std::fs::File;
use std::io::{self, BufWriter, Write, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::collections::HashMap;

const ZONE_MAP_VERSION: u8 = 1;


/// Min/max bounds for one column within one part.
///
/// Built during the column write pass. Multiple entries (one per numeric
/// column) are later serialized together into `part.zonemap`.
pub struct ZoneMapEntry<T> {
    pub min: T,
    pub max: T,
}

/// One min/max pair — the unit shared by the read and write paths.
pub struct ZoneEntry {
    pub min_bytes: [u8; 8],
    pub max_bytes: [u8; 8],
}

/// Write-path: one column's bound, ready to serialize. Carries `col_name`
/// because the writer gets a flat slice with no map to key on.
pub struct EncodedZoneMapEntry {
    pub col_name: String,
    pub type_tag: u8,
    pub entry: ZoneEntry,
}

/// Read-path: one column's bounds. `entries.len()` == 1 today (part-level),
/// N later (granule-level). Keyed by name in the `ZoneMap`, so no `col_name`.
pub struct ColumnZone {
    pub type_tag: u8,
    pub entries: Vec<ZoneEntry>,
}

/// Serialize zone map entries into `<part_dir>/part.zonemap`.
///
/// Layout:
///   Header:
///     [version: u8]
///     [col_count: u16]
///     for each column:
///       [col_name_len: u16][col_name: utf8]
///       [type_tag: u8]
///       [entry_count: u32]   // 1 today (part-level); N later for granule-level
///       [data_offset: u64]   // absolute byte offset where this column's entries begin
///   Data:
///     for each column: (entry_count × [min: 8 bytes][max: 8 bytes])
///
/// `data_offset` is computed at write time so the reader can seek directly
/// to a column's entries without scanning others.
pub fn write_zone_map(part_dir: &Path, entries: &[EncodedZoneMapEntry]) -> io::Result<()> {
    let path = part_dir.join("part.zonemap");
    let mut out = BufWriter::new(File::create(path)?);

    // Per-column header size: name_len(2) + name + type_tag(1) + entry_count(4) + offset(8)
    let header_size: u64 = 1 + 2 + entries
        .iter()
        .map(|e| 2 + e.col_name.len() + 1 + 4 + 8)
        .sum::<usize>() as u64;

    // Part-level: one (min, max) pair per column = 16 bytes each.
    let entry_count: u32 = 1;
    let bytes_per_column: u64 = (entry_count as u64) * 16;

    // ---- Header ----
    out.write_all(&[ZONE_MAP_VERSION])?;
    out.write_all(&(entries.len() as u16).to_le_bytes())?;

    let mut data_offset = header_size;
    for entry in entries {
        let name_bytes = entry.col_name.as_bytes();
        out.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        out.write_all(name_bytes)?;
        out.write_all(&[entry.type_tag])?;
        out.write_all(&entry_count.to_le_bytes())?;
        out.write_all(&data_offset.to_le_bytes())?;
        data_offset += bytes_per_column;
    }

    // ---- Data ----
    for entry in entries {
        out.write_all(&entry.entry.min_bytes)?;
        out.write_all(&entry.entry.max_bytes)?;
    }


    out.flush()?;
    out.get_ref().sync_all()?;
    Ok(())
}



pub type ZoneMap = HashMap<String, ColumnZone>;

/// Read `<part_dir>/part.zonemap` back into memory.
/// See `write_zone_map` for the on-disk layout.
pub fn read_zone_map(part_dir: &Path) -> io::Result<ZoneMap> {
    let path = part_dir.join("part.zonemap");
    let mut file = Cursor::new(std::fs::read(path)?);

    let mut version = [0u8; 1];
    file.read_exact(&mut version)?;
    if version[0] != ZONE_MAP_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported zone map version: {}", version[0]),
        ));
    }

    let mut col_count = [0u8; 2];
    file.read_exact(&mut col_count)?;
    let col_count = u16::from_le_bytes(col_count);

    // Pass 1: read every column's header entry sequentially.
    struct Header {
        name: String,
        type_tag: u8,
        entry_count: u32,
        data_offset: u64,
    }
    let mut headers = Vec::with_capacity(col_count as usize);
    for _ in 0..col_count {
        let mut name_len = [0u8; 2];
        file.read_exact(&mut name_len)?;
        let name_len = u16::from_le_bytes(name_len) as usize;

        let mut name_bytes = vec![0u8; name_len];
        file.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut type_tag = [0u8; 1];
        file.read_exact(&mut type_tag)?;

        let mut entry_count = [0u8; 4];
        file.read_exact(&mut entry_count)?;

        let mut data_offset = [0u8; 8];
        file.read_exact(&mut data_offset)?;

        headers.push(Header {
            name,
            type_tag: type_tag[0],
            entry_count: u32::from_le_bytes(entry_count),
            data_offset: u64::from_le_bytes(data_offset),
        });
    }

    // Pass 2: seek to each column's data section, read all its (min, max) pairs.
    let mut zone_map = ZoneMap::new();
    for h in headers {
        file.seek(SeekFrom::Start(h.data_offset))?;
        let mut entries = Vec::with_capacity(h.entry_count as usize);
        for _ in 0..h.entry_count {
            let mut min_bytes = [0u8; 8];
            let mut max_bytes = [0u8; 8];
            file.read_exact(&mut min_bytes)?;
            file.read_exact(&mut max_bytes)?;
            entries.push(ZoneEntry { min_bytes, max_bytes });
        }
        zone_map.insert(h.name, ColumnZone { type_tag: h.type_tag, entries });
    }

    Ok(zone_map)
}

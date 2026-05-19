use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

const ZONE_MAP_VERSION: u8 = 1;


/// Min/max bounds for one column within one part.
///
/// Built during the column write pass. Multiple entries (one per numeric
/// column) are later serialized together into `part.zonemap`.
pub struct ZoneMapEntry<T> {
    pub min: T,
    pub max: T,
}


/// Type-erased, ready-to-serialize zone map entry for one column.
///
/// Built by widening a typed `ZoneMapEntry<T>` to 8 raw bytes per bound and
/// pairing it with the column's name and `type_tag`. Multiple `EncodedZoneEntry`
/// values are written into a single `part.zonemap` file.
pub struct EncodedZoneMapEntry {
    pub col_name: String,
    pub type_tag: u8,
    pub min_bytes: [u8; 8],
    pub max_bytes: [u8; 8],
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
        out.write_all(&entry.min_bytes)?;
        out.write_all(&entry.max_bytes)?;
    }

    out.flush()?;
    out.get_ref().sync_all()?;
    Ok(())
}

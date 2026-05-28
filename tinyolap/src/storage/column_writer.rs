use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::config::{BLOCK_BUFFER_SIZE, GRANULE_SIZE};
use crate::encoding::{Codec, Primitive};
use crate::storage::mark::{Mark, MarkWriter};
use crate::storage::zone_map::ZoneMapEntry;

/// Write one non-string column to disk for a single part.
///
/// Pipeline per block:
///   Vec<T>  --serialize-->  raw bytes  --codec.encode-->  encoded bytes  --lz4-->  disk
///
/// One mark per granule, buffered until the block lands on disk so we can
/// stamp it with the real block_offset. fsync is called once at the end —
/// durability boundary is one INSERT = one part.
/// Tracks min/max across all values in a single pass and returns them as a
/// `ZoneMapEntry`. Returns `None` if the column has no rows.
pub fn write_column<T: Primitive + PartialOrd>(
    part_dir: &Path,
    col_name: &str,
    values: &[T],
    codec: Codec,
) -> io::Result<Option<ZoneMapEntry<T>>> {
    let bin_path = part_dir.join(format!("{col_name}.bin"));
    let mrk_path = part_dir.join(format!("{col_name}.mrk"));

    let mut bin = BufWriter::new(File::create(bin_path)?);
    let mut marks = MarkWriter::create(&mrk_path)?;

    // Typed staging buffer so the codec sees a full block at once.
    // Delta and RLE need neighbouring values — encoding byte-by-byte breaks them.
    let cap = (BLOCK_BUFFER_SIZE / T::WIDTH).max(GRANULE_SIZE);
    let mut block_values: Vec<T> = Vec::with_capacity(cap);
    let mut pending_marks: Vec<Mark> = Vec::new();

    let mut rows_in_current_granule: usize = 0;
    let mut rows_in_current_block: usize = 0;
    let mut bin_bytes: u64 = 0;

    let mut min_val: Option<T> = None;
    let mut max_val: Option<T> = None;

    for &v in values {
        // First row of a granule: push a mark before appending the value.
        // decompressed_offset is a byte offset into the decoded value stream,
        // which is fixed-width T, so it's simply rows_in_block * T::WIDTH.
        if rows_in_current_granule == 0 {
            pending_marks.push(Mark {
                block_offset: 0, // patched in flush_block once we know the offset
                decompressed_offset: (rows_in_current_block * T::WIDTH) as u64,
            });
        }

        block_values.push(v);
        rows_in_current_granule += 1;
        rows_in_current_block += 1;

        min_val = Some(match min_val {
            None => v,
            Some(m) => if v < m { v } else { m },
        });
        max_val = Some(match max_val {
            None => v,
            Some(m) => if v > m { v } else { m },
        });

        // Granules are atomic — only consider flushing on a boundary, never mid-granule.
        if rows_in_current_granule == GRANULE_SIZE {
            rows_in_current_granule = 0;
            if block_values.len() * T::WIDTH >= BLOCK_BUFFER_SIZE {
                flush_block(
                    codec,
                    &mut block_values,
                    &mut pending_marks,
                    &mut bin,
                    &mut marks,
                    &mut bin_bytes,
                    &mut rows_in_current_block,
                )?;
            }
        }
    }

    // Flush the tail block — may be a partial granule, its mark was already pushed.
    flush_block(
        codec,
        &mut block_values,
        &mut pending_marks,
        &mut bin,
        &mut marks,
        &mut bin_bytes,
        &mut rows_in_current_block,
    )?;

    // fsync once for the whole INSERT. MarkWriter::flush calls sync_all internally.
    bin.flush()?;
    bin.get_ref().sync_all()?;
    marks.flush()?;

    Ok(min_val.zip(max_val).map(|(min, max)| ZoneMapEntry { min, max }))

}

/// Serialize, encode, compress and write the current block. Patches pending
/// marks with the real on-disk block_offset. No-op on empty buffer.
fn flush_block<T: Primitive>(
    codec: Codec,
    block_values: &mut Vec<T>,
    pending_marks: &mut Vec<Mark>,
    bin: &mut BufWriter<File>,
    marks: &mut MarkWriter,
    bin_bytes: &mut u64,
    rows_in_current_block: &mut usize,
) -> io::Result<()> {
    if block_values.is_empty() {
        return Ok(());
    }

    // Phase 1: typed values → raw LE bytes.
    let mut raw: Vec<u8> = Vec::with_capacity(block_values.len() * T::WIDTH);
    for v in block_values.iter() {
        v.encode_le(&mut raw);
    }

    // Phase 2: codec transforms bytes → bytes (type-blind, knows only stride).
    let mut encoded: Vec<u8> = Vec::new();
    codec.encode(&raw, T::WIDTH, &mut encoded);

    // Phase 3: lz4 compress and write.
    // Block framing: [u8 codec_tag][u32 LE compressed_len][compressed bytes]
    // The codec tag lets the reader choose the right decode path after decompression.
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    let block_offset = *bin_bytes;

    bin.write_all(&[codec.tag()])?;
    bin.write_all(&(compressed.len() as u32).to_le_bytes())?;
    bin.write_all(&compressed)?;
    *bin_bytes += 1 + 4 + compressed.len() as u64;

    // Now that block_offset is known, patch and queue all pending marks.
    for mut mark in pending_marks.drain(..) {
        mark.block_offset = block_offset;
        marks.write(&mark);
    }

    block_values.clear();
    *rows_in_current_block = 0;
    Ok(())
}

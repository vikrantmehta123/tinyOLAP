use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::config::{BLOCK_BUFFER_SIZE, GRANULE_SIZE};
use crate::storage::mark::{Mark, MarkWriter};
use crate::encoding::StringCodec;

/// Write one string column to disk for a single part.
///
/// Block framing: [u8 codec_tag][u32 LE compressed_len][compressed bytes]
///
/// `decompressed_offset` in marks is a STRING COUNT — how many strings precede
/// this granule in the current block. The reader slices Vec<String> by index
/// directly, avoiding any byte-offset arithmetic or re-serialization.
pub fn write_string_column(
    part_dir: &Path,
    col_name: &str,
    values: &[String],
    codec: StringCodec,
) -> io::Result<()> {
    let bin_path = part_dir.join(format!("{col_name}.bin"));
    let mrk_path = part_dir.join(format!("{col_name}.mrk"));

    let mut bin = BufWriter::new(File::create(bin_path)?);
    let mut marks = MarkWriter::create(&mrk_path)?;

    let mut block_strings: Vec<String> = Vec::new();
    let mut pending_marks: Vec<Mark> = Vec::new();

    let mut rows_in_current_granule: usize = 0;
    // plain_bytes_in_block is used only for deciding when to flush —
    // marks use strings_in_block (a count) instead of byte offsets.
    let mut plain_bytes_in_block: usize = 0;
    let mut strings_in_block: usize = 0;
    let mut bin_bytes: u64 = 0;

    for s in values {
        let plain_size = 4 + s.len();

        // Size cap: flush before adding a string that would overflow the block.
        // Never split a string across blocks.
        if rows_in_current_granule > 0
            && plain_bytes_in_block + plain_size > BLOCK_BUFFER_SIZE
        {
            rows_in_current_granule = 0;
            flush_block(
                codec,
                &mut block_strings,
                &mut pending_marks,
                &mut bin,
                &mut marks,
                &mut bin_bytes,
                &mut plain_bytes_in_block,
                &mut strings_in_block,
            )?;
        }

        // First row of a new granule: record how many strings precede it in
        // this block. The reader uses this as a direct slice index into Vec<String>.
        if rows_in_current_granule == 0 {
            pending_marks.push(Mark {
                block_offset: 0,
                decompressed_offset: strings_in_block as u64,
            });
        }

        block_strings.push(s.clone());
        rows_in_current_granule += 1;
        plain_bytes_in_block += plain_size;
        strings_in_block += 1;

        // Row cap: granules are also bounded by GRANULE_SIZE rows.
        if rows_in_current_granule == GRANULE_SIZE {
            rows_in_current_granule = 0;
            if plain_bytes_in_block >= BLOCK_BUFFER_SIZE {
                flush_block(
                    codec,
                    &mut block_strings,
                    &mut pending_marks,
                    &mut bin,
                    &mut marks,
                    &mut bin_bytes,
                    &mut plain_bytes_in_block,
                    &mut strings_in_block,
                )?;
            }
        }
    }

    // Flush the tail block — its mark was already pushed when the granule started.
    flush_block(
        codec,
        &mut block_strings,
        &mut pending_marks,
        &mut bin,
        &mut marks,
        &mut bin_bytes,
        &mut plain_bytes_in_block,
        &mut strings_in_block,
    )?;

    bin.flush()?;
    bin.get_ref().sync_all()?;
    marks.flush()?;

    Ok(())
}

fn flush_block(
    codec: StringCodec,
    block_strings: &mut Vec<String>,
    pending_marks: &mut Vec<Mark>,
    bin: &mut BufWriter<File>,
    marks: &mut MarkWriter,
    bin_bytes: &mut u64,
    plain_bytes_in_block: &mut usize,
    strings_in_block: &mut usize,
) -> io::Result<()> {
    if block_strings.is_empty() {
        return Ok(());
    }

    let mut encoded: Vec<u8> = Vec::new();
    codec.encode(block_strings, &mut encoded);
    let compressed = lz4_flex::compress_prepend_size(&encoded);

    let block_offset = *bin_bytes;
    bin.write_all(&[codec.tag()])?;
    bin.write_all(&(compressed.len() as u32).to_le_bytes())?;
    bin.write_all(&compressed)?;
    *bin_bytes += 1 + 4 + compressed.len() as u64;

    for mut mark in pending_marks.drain(..) {
        mark.block_offset = block_offset;
        marks.write(&mark);
    }

    block_strings.clear();
    *plain_bytes_in_block = 0;
    *strings_in_block = 0;
    Ok(())
}

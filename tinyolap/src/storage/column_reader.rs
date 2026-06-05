use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::os::unix::io::AsRawFd;

use crate::encoding::{Codec, Primitive};
use crate::storage::mark::{Mark, MarkReader};

pub struct ColumnReader {
    bin: File,
    marks: Vec<Mark>,
    /// Single-block cache keyed by block_offset. Stores post-codec-decode bytes
    /// so `read_granule` can slice straight into plain LE values.
    cache: Option<(u64, Vec<u8>)>,
}

impl ColumnReader {
    pub fn open(part_dir: &Path, col_name: &str) -> io::Result<Self> {
        let bin_path = part_dir.join(format!("{col_name}.bin"));
        let mrk_path = part_dir.join(format!("{col_name}.mrk"));

        let bin = File::open(bin_path)?;
        
        unsafe { 
            libc::posix_fadvise(bin.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
        }

        let marks = MarkReader::open(&mrk_path)?.read_all()?;

        Ok(Self { bin, marks, cache: None })
    }

    pub fn granule_count(&self) -> usize {
        self.marks.len()
    }

    pub fn read_granule<T: Primitive>(&mut self, idx: usize) -> io::Result<Vec<T>> {
        let mark = &self.marks[idx];

        let cache_hit = matches!(&self.cache, Some((off, _)) if *off == mark.block_offset);
        if !cache_hit {
            self.bin.seek(SeekFrom::Start(mark.block_offset))?;

            // New framing: [u8 codec_tag][u32 LE compressed_len][compressed bytes]
            let mut tag = [0u8; 1];
            self.bin.read_exact(&mut tag)?;
            let codec = Codec::from_tag(tag[0])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

            let mut len_buf = [0u8; 4];
            self.bin.read_exact(&mut len_buf)?;
            let compressed_len = u32::from_le_bytes(len_buf) as usize;

            let mut compressed = vec![0u8; compressed_len];
            self.bin.read_exact(&mut compressed)?;

            let decompressed = lz4_flex::decompress_size_prepended(&compressed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

            // Decode with the codec that was used at write time. After this step,
            // `decoded` contains plain LE bytes — identical to what Plain would produce.
            // The granule slicing below works the same regardless of which codec was used.
            let mut decoded = Vec::new();
            codec.decode(&decompressed, T::WIDTH, &mut decoded)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

            self.cache = Some((mark.block_offset, decoded));
        }

        let block = &self.cache.as_ref().unwrap().1;

        // Granule byte range inside the decoded block:
        //   start = this mark's decompressed_offset
        //   end   = next mark's decompressed_offset if it's in the same block,
        //           otherwise the block ends here.
        let start = mark.decompressed_offset as usize;
        let end = match self.marks.get(idx + 1) {
            Some(next) if next.block_offset == mark.block_offset => {
                next.decompressed_offset as usize
            }
            _ => block.len(),
        };

        debug_assert!(
            (end - start) % T::WIDTH == 0,
            "granule byte range not a multiple of element width"
        );

        Ok(block[start..end]
            .chunks_exact(T::WIDTH)
            .map(T::decode_le)
            .collect())
    }
}

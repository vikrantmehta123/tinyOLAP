use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::os::unix::io::AsRawFd;

use crate::encoding::StringCodec;
use crate::storage::mark::{Mark, MarkReader};

pub struct StringColumnReader {
    bin: File,
    marks: Vec<Mark>,
    /// Single-block cache keyed by block_offset. Stores decoded strings so
    /// read_granule can slice by index directly — no byte math, no re-encode.
    cache: Option<(u64, Vec<String>)>,
}

impl StringColumnReader {
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

    pub fn read_granule(&mut self, idx: usize) -> io::Result<Vec<String>> {
        let mark = &self.marks[idx];

        let cache_hit = matches!(&self.cache, Some((off, _)) if *off == mark.block_offset);
        if !cache_hit {
            self.bin.seek(SeekFrom::Start(mark.block_offset))?;

            // Framing: [u8 codec_tag][u32 LE compressed_len][compressed bytes]
            let mut tag = [0u8; 1];
            self.bin.read_exact(&mut tag)?;
            let codec = StringCodec::from_tag(tag[0])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

            let mut len_buf = [0u8; 4];
            self.bin.read_exact(&mut len_buf)?;
            let compressed_len = u32::from_le_bytes(len_buf) as usize;

            let mut compressed = vec![0u8; compressed_len];
            self.bin.read_exact(&mut compressed)?;

            let decompressed = lz4_flex::decompress_size_prepended(&compressed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

            let mut strings: Vec<String> = Vec::new();
            codec.decode(&decompressed, &mut strings)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

            self.cache = Some((mark.block_offset, strings));
        }

        let strings = &self.cache.as_ref().unwrap().1;

        // decompressed_offset is a string count — slice directly by index.
        let start = mark.decompressed_offset as usize;
        let end = match self.marks.get(idx + 1) {
            Some(next) if next.block_offset == mark.block_offset => {
                next.decompressed_offset as usize
            }
            _ => strings.len(),
        };

        Ok(strings[start..end].to_vec())
    }
}

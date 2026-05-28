use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub struct Mark {
    pub block_offset: u64,
    pub decompressed_offset: u64, 
}


impl Mark {
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..8].copy_from_slice(&self.block_offset.to_le_bytes());
        buf[8..16].copy_from_slice(&self.decompressed_offset.to_le_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Mark {
        let block_offset = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let decompressed_offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());

        Mark { block_offset, decompressed_offset }
    }
}


pub struct MarkWriter {
    file: File,
    buf: Vec<u8>,
}

impl MarkWriter {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        Ok(MarkWriter{file, buf: Vec::new()})
    }

    pub fn write(&mut self, mark: &Mark) {
        self.buf.extend_from_slice(&mark.to_bytes());
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.file.write_all(&self.buf)?;
        self.file.sync_all()?;
        Ok(())
    }
}

pub struct MarkReader {
    file: File
}

impl MarkReader {
    pub fn open(path: &Path) -> std::io::Result<Self> {

        Ok(MarkReader { file: File::open(path)? })
    }

    pub fn read_all(&mut self) -> std::io::Result<Vec<Mark>> {
        let mut buf = Vec::new();
        self.file.read_to_end(&mut buf)?;
        Ok(buf.chunks_exact(24).map(Mark::from_bytes).collect())
    }
}




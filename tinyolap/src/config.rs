pub const GRANULE_SIZE: usize = 512;
pub const BLOCK_BUFFER_SIZE: usize = 8 * 1024; // size of uncompressed buffer before compression happens
pub const DATA_DIR: &str = "data/tinyolap_smoke";
pub const N_WORKERS:usize = 4;
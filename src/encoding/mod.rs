//! Encoding library for fixed-width column data.
//!
//! Two-phase pipeline for numeric columns:
//!   1. Serialize typed values → raw bytes  (`Primitive` trait, type-aware)
//!   2. Encode raw bytes → encoded bytes    (`Codec`, fully type-blind, stride-based)
//!
//! Keeping these phases separate means:
//!   - New numeric type  → implement `Primitive`. No codec changes.
//!   - New codec         → add a `Codec` variant + byte-level impl. No type changes.
//!
//! String columns are variable-length and cannot share this stride-based interface.
//! They will have their own encoding abstraction.

pub mod plain;
pub mod delta;
pub mod rle;
pub mod string_dictionary;
pub mod string_plain;

// Sealing prevents external crates from implementing `Primitive` for
// arbitrary types.  Without it, a caller could pass a type with a wrong WIDTH
// or a non-LE byte layout and silently corrupt encoded data.
mod sealed {
    pub trait Sealed {}
}

/// Serialization contract for fixed-width column types.
///
/// Intentionally minimal — just "how does this value become bytes and back."
/// No arithmetic, no codec knowledge. All encoding lives in `Codec`, which
/// operates on the serialized bytes and never sees the Rust type.
pub trait Primitive: Copy + PartialEq + sealed::Sealed {
    /// Byte width of this type on disk. Passed to `Codec` as the stride so it
    /// knows how many bytes constitute one value.
    const WIDTH: usize;

    /// Append this value's little-endian bytes to `out`.
    fn encode_le(self, out: &mut Vec<u8>);

    /// Reconstruct a value from the first `WIDTH` bytes of `bytes`.
    fn decode_le(bytes: &[u8]) -> Self;
}

// ---- Primitive impls -------------------------------------------------------

impl sealed::Sealed for i8 {}
impl Primitive for i8 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for i16 {}
impl Primitive for i16 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for i32 {}
impl Primitive for i32 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for i64 {}
impl Primitive for i64 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for u8 {}
impl Primitive for u8 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for u16 {}
impl Primitive for u16 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for u32 {}
impl Primitive for u32 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for u64 {}
impl Primitive for u64 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for f32 {}
impl Primitive for f32 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for f64 {}
impl Primitive for f64 {
    const WIDTH: usize = std::mem::size_of::<Self>();
    fn encode_le(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
    fn decode_le(bytes: &[u8]) -> Self { Self::from_le_bytes(bytes.try_into().unwrap()) }
}

impl sealed::Sealed for bool {}
impl Primitive for bool {
    // Stored as a single byte: 0 = false, 1 = true.
    const WIDTH: usize = 1;
    fn encode_le(self, out: &mut Vec<u8>) { out.push(self as u8); }
    fn decode_le(bytes: &[u8]) -> Self { bytes[0] != 0 }
}

// ---- Codec -----------------------------------------------------------------

/// Which encoding was applied to a block's byte stream.
///
/// Stored as a single tag byte at the start of each compressed block so the
/// reader knows which decode path to take after lz4 decompression.
///
/// `Codec` is type-blind: it operates on flat byte slices with a `stride`
/// parameter that tells it how many bytes make up one value. The caller
/// (column writer) serializes typed values into bytes first, then hands those
/// bytes to the codec. Adding a new codec means adding a variant here plus a
/// byte-level implementation — no changes to `Primitive` or column types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Plain,
    Delta,
    RLE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingError {
    /// Input was too short to contain a complete value or header.
    Truncated,
    /// The tag byte on disk does not match any known codec.
    BadHeader,
    /// This codec does not support the given stride (value width in bytes).
    UnsupportedStride(usize),

    InvalidUtf8,   // string bytes on disk are not valid UTF-8
    Corrupt,       // structural corruption (e.g. dictionary index out of bounds)
}

impl Codec {
    /// Encode `src` into `out`.
    ///
    /// `src` is a flat slice of little-endian serialized values; `stride` is
    /// the byte width of each value (`T::WIDTH` for whatever `Primitive` type
    /// was serialized into `src`).
    pub fn encode(&self, src: &[u8], stride: usize, out: &mut Vec<u8>) {
        match self {
            Codec::Plain => plain::encode(src, stride, out),
            Codec::Delta => delta::encode(src, stride, out),
            Codec::RLE => rle::encode(src, stride, out),
        }
    }

    /// Decode `src` back to raw little-endian value bytes, appending to `out`.
    ///
    /// On success `out` contains the same bytes that were passed to `encode`.
    /// The caller then uses `T::decode_le` to reconstruct typed values.
    pub fn decode(
        &self,
        src: &[u8],
        stride: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), EncodingError> {
        match self {
            Codec::Plain => plain::decode(src, stride, out),
            Codec::Delta => delta::decode(src, stride, out),
            Codec::RLE => rle::decode(src, stride, out),
        }
    }

    /// Single-byte tag written to disk so the reader can reconstruct the codec.
    pub fn tag(self) -> u8 {
        match self {
            Codec::Plain => 0,
            Codec::Delta => 1,
            Codec::RLE => 2,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self, EncodingError> {
        match tag {
            0 => Ok(Codec::Plain),
            1 => Ok(Codec::Delta),
            2 => Ok(Codec::RLE),
            _ => Err(EncodingError::BadHeader),
        }
    }
}


/// Which encoding was applied to a string column's byte stream.
/// Stored as a single tag byte at the start of each compressed block,
/// parallel to `Codec` for numeric columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringCodec {
    Plain,
    Dictionary,
}

impl StringCodec {
    pub fn encode(&self, src: &[String], out: &mut Vec<u8>) {
        match self {
            StringCodec::Plain      => string_plain::encode(src, out),
            StringCodec::Dictionary => string_dictionary::encode(src, out),
        }
    }

    pub fn decode(&self, src: &[u8], out: &mut Vec<String>) -> Result<(), EncodingError> {
        match self {
            StringCodec::Plain      => string_plain::decode(src, out),
            StringCodec::Dictionary => string_dictionary::decode(src, out),
        }
    }

    pub fn tag(self) -> u8 {
        match self {
            StringCodec::Plain      => 0,
            StringCodec::Dictionary => 1,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self, EncodingError> {
        match tag {
            0 => Ok(StringCodec::Plain),
            1 => Ok(StringCodec::Dictionary),
            _ => Err(EncodingError::BadHeader),
        }
    }
}

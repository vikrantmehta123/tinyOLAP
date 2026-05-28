//! Delta encoding for fixed-width column data.
//!
//! Operates on raw little-endian bytes with a stride (element width in bytes).
//! The column writer serializes typed values to bytes before calling this module —
//! we never see Rust types here.
//!
//! Format: [first_value: stride bytes][delta_1: stride bytes]...[delta_n: stride bytes]
//! Each delta = current wrapping_sub previous, using unsigned integer arithmetic
//! at the given stride width.
//!
//! Supported strides: 1, 2, 4, 8 (matching u8/u16/u32/u64 widths).
//! The caller (table_writer) is responsible for not routing float or bool columns
//! through Delta — the round-trip is correct but deltas are meaningless for those types.

use crate::encoding::EncodingError;

pub fn encode(src: &[u8], stride: usize, out: &mut Vec<u8>) {
    if src.is_empty() {
        return;
    }

    match stride {
        1 => {
            out.push(src[0]);
            let mut prev = src[0];
            for &curr in &src[1..] {
                out.push(curr.wrapping_sub(prev));
                prev = curr;
            }
        }
        2 => {
            out.extend_from_slice(&src[..2]);
            let mut prev = u16::from_le_bytes(src[..2].try_into().unwrap());
            for chunk in src[2..].chunks_exact(2) {
                let curr = u16::from_le_bytes(chunk.try_into().unwrap());
                out.extend_from_slice(&curr.wrapping_sub(prev).to_le_bytes());
                prev = curr;
            }
        }
        4 => {
            out.extend_from_slice(&src[..4]);
            let mut prev = u32::from_le_bytes(src[..4].try_into().unwrap());
            for chunk in src[4..].chunks_exact(4) {
                let curr = u32::from_le_bytes(chunk.try_into().unwrap());
                out.extend_from_slice(&curr.wrapping_sub(prev).to_le_bytes());
                prev = curr;
            }
        }
        8 => {
            out.extend_from_slice(&src[..8]);
            let mut prev = u64::from_le_bytes(src[..8].try_into().unwrap());
            for chunk in src[8..].chunks_exact(8) {
                let curr = u64::from_le_bytes(chunk.try_into().unwrap());
                out.extend_from_slice(&curr.wrapping_sub(prev).to_le_bytes());
                prev = curr;
            }
        }
        // table_writer validates the codec+type pairing before we get here,
        // so this arm should never be reached in practice.
        _ => out.extend_from_slice(src),
    }
}

pub fn decode(src: &[u8], stride: usize, out: &mut Vec<u8>) -> Result<(), EncodingError> {
    if src.is_empty() {
        return Ok(());
    }

    if src.len() % stride != 0 {
        return Err(EncodingError::Truncated);
    }

    match stride {
        1 => {
            out.push(src[0]);
            let mut prev = src[0];
            for &delta in &src[1..] {
                let curr = prev.wrapping_add(delta);
                out.push(curr);
                prev = curr;
            }
        }
        2 => {
            out.extend_from_slice(&src[..2]);
            let mut prev = u16::from_le_bytes(src[..2].try_into().unwrap());
            for chunk in src[2..].chunks_exact(2) {
                let delta = u16::from_le_bytes(chunk.try_into().unwrap());
                let curr = prev.wrapping_add(delta);
                out.extend_from_slice(&curr.to_le_bytes());
                prev = curr;
            }
        }
        4 => {
            out.extend_from_slice(&src[..4]);
            let mut prev = u32::from_le_bytes(src[..4].try_into().unwrap());
            for chunk in src[4..].chunks_exact(4) {
                let delta = u32::from_le_bytes(chunk.try_into().unwrap());
                let curr = prev.wrapping_add(delta);
                out.extend_from_slice(&curr.to_le_bytes());
                prev = curr;
            }
        }
        8 => {
            out.extend_from_slice(&src[..8]);
            let mut prev = u64::from_le_bytes(src[..8].try_into().unwrap());
            for chunk in src[8..].chunks_exact(8) {
                let delta = u64::from_le_bytes(chunk.try_into().unwrap());
                let curr = prev.wrapping_add(delta);
                out.extend_from_slice(&curr.to_le_bytes());
                prev = curr;
            }
        }
        s => return Err(EncodingError::UnsupportedStride(s)),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::Primitive;

    // Helper: serialize a typed slice to bytes, delta-encode, delta-decode,
    // deserialize back, and compare. This is the exact pipeline the column
    // writer uses.
    fn roundtrip<T: Primitive + PartialEq + std::fmt::Debug>(values: &[T]) -> Vec<T> {
        let mut raw = Vec::new();
        for &v in values { v.encode_le(&mut raw); }

        let mut encoded = Vec::new();
        encode(&raw, T::WIDTH, &mut encoded);

        let mut decoded_raw = Vec::new();
        decode(&encoded, T::WIDTH, &mut decoded_raw).unwrap();

        decoded_raw.chunks_exact(T::WIDTH)
            .map(T::decode_le)
            .collect()
    }

    #[test]
    fn roundtrip_i32_basic() {
        let xs = vec![100_i32, 105, 103, 200, 199];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_empty() {
        let xs: Vec<i32> = vec![];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_single() {
        let xs = vec![42_i32];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_i32_overflow() {
        // wrapping arithmetic must handle deltas that don't fit in i32
        let xs = vec![i32::MAX, i32::MIN, 0, i32::MAX];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_i64_overflow() {
        let xs = vec![i64::MAX, i64::MIN, 0, i64::MAX];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_u32_basic() {
        let xs = vec![10_u32, 12, 15, 3, u32::MAX];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn encoded_format_u32() {
        // Verify the on-disk format: first value verbatim, then deltas.
        let xs = vec![10_u32, 12, 15];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }

        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);

        let expected: Vec<u8> = vec![
            10, 0, 0, 0,  // 10 verbatim
             2, 0, 0, 0,  // delta: 12 - 10 = 2
             3, 0, 0, 0,  // delta: 15 - 12 = 3
        ];
        assert_eq!(encoded, expected);
    }

    #[test]
    fn truncated_input_is_rejected() {
        // 5 bytes is not a multiple of stride=4
        let bad = vec![0u8; 5];
        let mut out = Vec::new();
        assert_eq!(decode(&bad, 4, &mut out), Err(EncodingError::Truncated));
    }

    #[test]
    fn unsupported_stride_is_rejected() {
        let src = vec![0u8; 6];
        let mut out = Vec::new();
        assert_eq!(decode(&src, 3, &mut out), Err(EncodingError::UnsupportedStride(3)));
    }
}

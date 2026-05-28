//! Run-Length Encoding for fixed-width column data.
//!
//! Operates on raw little-endian bytes with a stride (element width in bytes).
//! Equality is determined by byte identity — no arithmetic needed, so RLE
//! works correctly for all Primitive types including floats and bools.
//!
//! Format: [count: u16 LE][value: stride bytes] pairs.
//!
//! `count` is u16 because at the current BLOCK_BUFFER_SIZE of 8 KiB, the most
//! values a block can hold is 8192 / 1 = 8192 (for u8 columns), well under
//! u16::MAX. If BLOCK_BUFFER_SIZE grows past ~64 KiB, revisit or rely on the
//! u16::MAX split already implemented in encode.

use crate::encoding::EncodingError;

pub fn encode(src: &[u8], stride: usize, out: &mut Vec<u8>) {
    if src.is_empty() {
        return;
    }

    let mut prev = &src[..stride];
    let mut count: u16 = 1;

    for chunk in src[stride..].chunks_exact(stride) {
        if chunk == prev && count < u16::MAX {
            count += 1;
        } else {
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(prev);
            prev = chunk;
            count = 1;
        }
    }

    // Flush the final run — the loop never emits its last pair.
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(prev);
}

pub fn decode(src: &[u8], stride: usize, out: &mut Vec<u8>) -> Result<(), EncodingError> {
    let pair_size = 2 + stride;
    let mut i = 0;

    while i < src.len() {
        if src.len() - i < pair_size {
            return Err(EncodingError::Truncated);
        }

        let count = u16::from_le_bytes(src[i..i + 2].try_into().unwrap());
        let value = &src[i + 2..i + 2 + stride];

        for _ in 0..count {
            out.extend_from_slice(value);
        }

        i += pair_size;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::Primitive;

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
    fn roundtrip_basic() {
        let xs = vec![7_i32, 7, 7, 3, 3, 9, 9, 9, 9];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_empty() {
        let xs: Vec<i32> = vec![];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_single_value() {
        let xs = vec![42_i32];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }
        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);
        assert_eq!(encoded.len(), 6); // one pair: 2 (count) + 4 (value)
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_no_runs() {
        // worst case: every value is unique — one pair per value
        let xs = vec![1_i32, 2, 3, 4, 5];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }
        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);
        assert_eq!(encoded.len(), 5 * 6); // 5 pairs × (2 + 4) bytes
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_one_giant_run() {
        let xs = vec![42_i32; 1000];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }
        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);
        assert_eq!(encoded.len(), 6); // collapses to a single pair
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn run_split_at_u16_max() {
        // 65536 identical values must split into two pairs: (65535 + 1)
        let xs = vec![5_i32; 65536];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }
        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);
        assert_eq!(encoded.len(), 2 * 6);
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_bool() {
        // RLE is particularly effective for bool columns (long runs of true/false)
        let xs = vec![true, true, true, false, false, true];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn truncated_input_rejected() {
        let bytes = vec![0_u8]; // not enough for even a count
        let mut out = Vec::new();
        assert_eq!(decode(&bytes, 4, &mut out), Err(EncodingError::Truncated));
    }

    #[test]
    fn format_lock() {
        let xs = vec![7_u32, 7, 7, 3];
        let mut raw = Vec::new();
        for &v in &xs { v.encode_le(&mut raw); }
        let mut encoded = Vec::new();
        encode(&raw, 4, &mut encoded);

        let expected: Vec<u8> = vec![
            3, 0,          // count = 3 (u16 LE)
            7, 0, 0, 0,    // value = 7 (u32 LE)
            1, 0,          // count = 1
            3, 0, 0, 0,    // value = 3
        ];
        assert_eq!(encoded, expected);
    }
}

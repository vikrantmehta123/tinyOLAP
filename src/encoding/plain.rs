//! Plain encoding: raw bytes copied through unchanged.

use crate::encoding::EncodingError;

// stride is accepted but unused — Plain copies bytes verbatim.  The parameter
// exists so all codec encode functions share the same signature, letting
// Codec::encode dispatch them uniformly.
pub fn encode(src: &[u8], stride: usize, out: &mut Vec<u8>) {
    debug_assert!(stride > 0, "stride must be non-zero");
    out.extend_from_slice(src);
}

pub fn decode(src: &[u8], stride: usize, out: &mut Vec<u8>) -> Result<(), EncodingError> {
    if stride == 0 {
        return Err(EncodingError::UnsupportedStride(stride));
    }

    if src.len() % stride != 0 {
        return Err(EncodingError::Truncated);
    }

    out.extend_from_slice(src);
    Ok(())
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::Primitive;

    fn encode_typed<T: Primitive>(src: &[T]) -> Vec<u8> {
        let mut out = Vec::new();
        for &value in src {
            value.encode_le(&mut out);
        }
        out
    }

    fn decode_typed<T: Primitive>(src: &[u8]) -> Vec<T> {
        src.chunks_exact(T::WIDTH).map(T::decode_le).collect()
    }

    #[test]
    fn roundtrip_i32_basic() {
        let xs = vec![100_i32, 105, 103, 200, 199];
        let raw = encode_typed(&xs);
        let mut bytes = Vec::new();
        encode(&raw, i32::WIDTH, &mut bytes);
        let mut out = Vec::new();
        decode(&bytes, i32::WIDTH, &mut out).unwrap();
        let out: Vec<i32> = decode_typed(&out);
        assert_eq!(xs, out);
    }

    #[test]
    fn roundtrip_empty() {
        let xs: Vec<i32> = vec![];
        let raw = encode_typed(&xs);
        let mut bytes = Vec::new();
        encode(&raw, i32::WIDTH, &mut bytes);
        assert!(bytes.is_empty());
        let mut out = Vec::new();
        decode(&bytes, i32::WIDTH, &mut out).unwrap();
        let out: Vec<i32> = decode_typed(&out);
        assert_eq!(xs, out);
    }

    #[test]
    fn roundtrip_u64_basic() {
        let xs = vec![0_u64, 1, u64::MAX, 42, u64::MAX / 2];
        let raw = encode_typed(&xs);
        let mut bytes = Vec::new();
        encode(&raw, u64::WIDTH, &mut bytes);
        assert_eq!(bytes.len(), xs.len() * std::mem::size_of::<u64>());
        let mut out = Vec::new();
        decode(&bytes, u64::WIDTH, &mut out).unwrap();
        let out: Vec<u64> = decode_typed(&out);
        assert_eq!(xs, out);
    }

    #[test]
    fn roundtrip_i8_extremes() {
        let xs = vec![i8::MIN, -1, 0, 1, i8::MAX];
        let raw = encode_typed(&xs);
        let mut bytes = Vec::new();
        encode(&raw, i8::WIDTH, &mut bytes);
        let mut out = Vec::new();
        decode(&bytes, i8::WIDTH, &mut out).unwrap();
        let out: Vec<i8> = decode_typed(&out);
        assert_eq!(xs, out);
    }

    #[test]
    fn format_u32() {
        let xs = vec![10_u32, 12, 15];
        let raw = encode_typed(&xs);
        let mut bytes = Vec::new();
        encode(&raw, u32::WIDTH, &mut bytes);

        let expected = vec![
            10, 0, 0, 0,
            12, 0, 0, 0,
            15, 0, 0, 0,
        ];

        assert_eq!(bytes, expected);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        let src = vec![1_u8, 2, 3];
        let mut out = Vec::new();
        let err = decode(&src, 2, &mut out).unwrap_err();
        assert_eq!(err, EncodingError::Truncated);
    }
}

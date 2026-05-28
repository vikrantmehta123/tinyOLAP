//! Plain encoding for string columns.
//!
//! Format: [u32 LE byte_len][utf8 bytes] per string.
//! Simple and lossless — correct for any string content including high-cardinality
//! or free-text columns where dictionary encoding would waste space.

use crate::encoding::EncodingError;

pub fn encode(src: &[String], out: &mut Vec<u8>) {
    for s in src {
        let bytes = s.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
}

pub fn decode(src: &[u8], out: &mut Vec<String>) -> Result<(), EncodingError> {
    let mut i = 0;

    while i < src.len() {
        if src.len() - i < 4 {
            return Err(EncodingError::Truncated);
        }

        let len = u32::from_le_bytes(src[i..i + 4].try_into().unwrap()) as usize;
        i += 4;

        if src.len() - i < len {
            return Err(EncodingError::Truncated);
        }

        let s = std::str::from_utf8(&src[i..i + len])
            .map_err(|_| EncodingError::InvalidUtf8)?;
        out.push(s.to_owned());
        i += len;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(values: &[&str]) -> Vec<String> {
        let owned: Vec<String> = values.iter().map(|s| s.to_string()).collect();
        let mut encoded = Vec::new();
        encode(&owned, &mut encoded);
        let mut decoded = Vec::new();
        decode(&encoded, &mut decoded).unwrap();
        decoded
    }

    #[test]
    fn roundtrip_basic() {
        let xs = vec!["hello", "world", "foo"];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_empty_slice() {
        let xs: Vec<String> = vec![];
        let mut encoded = Vec::new();
        encode(&xs, &mut encoded);
        assert!(encoded.is_empty());
        let mut decoded = Vec::new();
        decode(&encoded, &mut decoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn roundtrip_empty_string() {
        let xs = vec!["", "non-empty", ""];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_unicode() {
        let xs = vec!["こんにちは", "héllo", "🦀"];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn format_lock() {
        let xs = vec!["hi".to_string()];
        let mut encoded = Vec::new();
        encode(&xs, &mut encoded);

        let expected: Vec<u8> = vec![
            2, 0, 0, 0,   // length = 2 (u32 LE)
            b'h', b'i',   // utf8 bytes
        ];
        assert_eq!(encoded, expected);
    }

    #[test]
    fn truncated_length_header_rejected() {
        let bad = vec![0u8; 3]; // 3 bytes — not enough for a u32 length header
        let mut out = Vec::new();
        assert_eq!(decode(&bad, &mut out), Err(EncodingError::Truncated));
    }

    #[test]
    fn truncated_body_rejected() {
        // length header says 10 bytes but only 2 follow
        let mut bad = Vec::new();
        bad.extend_from_slice(&10_u32.to_le_bytes());
        bad.extend_from_slice(b"hi");
        let mut out = Vec::new();
        assert_eq!(decode(&bad, &mut out), Err(EncodingError::Truncated));
    }
}

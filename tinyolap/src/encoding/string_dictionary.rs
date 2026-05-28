//! Dictionary encoding for string columns.
//!
//! Efficient for low-cardinality columns (status codes, country names, categories)
//! where many rows repeat the same small set of strings. Poor fit for
//! high-cardinality columns (UUIDs, free-text) — use Plain there instead.
//!
//! Format:
//!   [u32 LE dict_len]
//!   for each dictionary entry: [u32 LE byte_len][utf8 bytes]
//!   for each row:              [u32 LE index into dictionary]

use std::collections::HashMap;
use crate::encoding::EncodingError;

pub fn encode(src: &[String], out: &mut Vec<u8>) {
    // Build dictionary in insertion order so the first occurrence of each
    // string gets the lowest index. HashMap tracks the index; Vec tracks order.
    let mut index_map: HashMap<&str, u32> = HashMap::new();
    let mut dict: Vec<&str> = Vec::new();

    for s in src {
        if !index_map.contains_key(s.as_str()) {
            let idx = dict.len() as u32;
            index_map.insert(s.as_str(), idx);
            dict.push(s.as_str());
        }
    }

    // Write dictionary.
    out.extend_from_slice(&(dict.len() as u32).to_le_bytes());
    for entry in &dict {
        let bytes = entry.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }

    // Write one u32 index per row.
    for s in src {
        let idx = index_map[s.as_str()];
        out.extend_from_slice(&idx.to_le_bytes());
    }
}

pub fn decode(src: &[u8], out: &mut Vec<String>) -> Result<(), EncodingError> {
    let mut i = 0;

    // Read dictionary.
    if src.len() - i < 4 {
        return Err(EncodingError::Truncated);
    }
    let dict_len = u32::from_le_bytes(src[i..i + 4].try_into().unwrap()) as usize;
    i += 4;

    let mut dict: Vec<String> = Vec::with_capacity(dict_len);
    for _ in 0..dict_len {
        if src.len() - i < 4 {
            return Err(EncodingError::Truncated);
        }
        let str_len = u32::from_le_bytes(src[i..i + 4].try_into().unwrap()) as usize;
        i += 4;

        if src.len() - i < str_len {
            return Err(EncodingError::Truncated);
        }
        let s = std::str::from_utf8(&src[i..i + str_len])
            .map_err(|_| EncodingError::InvalidUtf8)?;
        dict.push(s.to_owned());
        i += str_len;
    }

    // Read indices and look up strings.
    while i < src.len() {
        if src.len() - i < 4 {
            return Err(EncodingError::Truncated);
        }
        let idx = u32::from_le_bytes(src[i..i + 4].try_into().unwrap()) as usize;
        i += 4;

        if idx >= dict.len() {
            return Err(EncodingError::Corrupt);
        }
        out.push(dict[idx].clone());
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
        let xs = vec!["a", "b", "a", "c", "b", "a"];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_empty_slice() {
        let xs: Vec<String> = vec![];
        let mut encoded = Vec::new();
        encode(&xs, &mut encoded);
        let mut decoded = Vec::new();
        decode(&encoded, &mut decoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn roundtrip_single_unique() {
        // entire column is the same value — dict has one entry, all indices are 0
        let xs = vec!["status_ok"; 100];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_all_unique() {
        // worst case: no compression benefit, dict size == row count
        let xs: Vec<&str> = vec!["alpha", "beta", "gamma", "delta"];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn roundtrip_unicode() {
        let xs = vec!["🦀", "🦀", "🐍", "🦀"];
        assert_eq!(roundtrip(&xs), xs);
    }

    #[test]
    fn corrupt_index_rejected() {
        // manually craft a payload where the index points past the dictionary
        let mut bad = Vec::new();
        bad.extend_from_slice(&1_u32.to_le_bytes()); // dict_len = 1
        bad.extend_from_slice(&3_u32.to_le_bytes()); // entry byte_len = 3
        bad.extend_from_slice(b"foo");               // entry = "foo"
        bad.extend_from_slice(&99_u32.to_le_bytes()); // index 99 — out of bounds
        let mut out = Vec::new();
        assert_eq!(decode(&bad, &mut out), Err(EncodingError::Corrupt));
    }
}

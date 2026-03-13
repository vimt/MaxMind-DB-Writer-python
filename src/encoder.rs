use crate::value::MmdbValue;
use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashMap;

#[allow(dead_code)]
struct MmdbTypeID;

impl MmdbTypeID {
    const STRING: u8 = 2;
    const DOUBLE: u8 = 3;
    const BYTES: u8 = 4;
    const UINT16: u8 = 5;
    const UINT32: u8 = 6;
    const MAP: u8 = 7;
    const INT32: u8 = 8;
    const UINT64: u8 = 9;
    const UINT128: u8 = 10;
    const ARRAY: u8 = 11;
    const BOOLEAN: u8 = 14;
    const FLOAT: u8 = 15;
}

pub struct Encoder {
    data_segment: Vec<u8>,
    cache: HashMap<MmdbValue, u32>,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            data_segment: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// Encode a value into the data segment (cached for scalars, with pointer indirection).
    /// Returns pointer bytes for embedding in parent structures.
    /// Map/Array values skip the top-level cache to avoid expensive hash computations.
    pub fn encode_value(&mut self, value: &MmdbValue) -> Vec<u8> {
        let cacheable = !matches!(value, MmdbValue::Map(_) | MmdbValue::Array(_));

        if cacheable {
            if let Some(&offset) = self.cache.get(value) {
                let mut buf = Vec::new();
                Self::encode_pointer(offset, &mut buf);
                return buf;
            }
        }

        let mut actual_bytes = Vec::new();
        self.encode_cached_bytes(value, &mut actual_bytes);

        let offset = self.data_segment.len() as u32;
        self.data_segment.extend_from_slice(&actual_bytes);

        if cacheable {
            self.cache.insert(value.clone(), offset);
        }

        let mut buf = Vec::new();
        Self::encode_pointer(offset, &mut buf);
        buf
    }

    /// Encode a value into the data segment and return its offset directly.
    /// Used for tree leaf encoding where we track offsets externally by pointer.
    pub fn encode_leaf(&mut self, value: &MmdbValue) -> u32 {
        let mut actual_bytes = Vec::new();
        self.encode_cached_bytes(value, &mut actual_bytes);

        let offset = self.data_segment.len() as u32;
        self.data_segment.extend_from_slice(&actual_bytes);
        offset
    }

    /// Encode an ordered map inline. Keys are written in the given order.
    /// Critical for metadata: build_epoch must be last to avoid libmaxminddb v1.13 issue.
    pub fn encode_ordered_map_inline(entries: &[(&str, MmdbValue)]) -> Vec<u8> {
        let mut buffer = Vec::new();
        Self::encode_header(MmdbTypeID::MAP, entries.len(), &mut buffer);
        for (key, value) in entries {
            Self::encode_header(MmdbTypeID::STRING, key.len(), &mut buffer);
            buffer.extend_from_slice(key.as_bytes());
            Self::encode_inline_recursive(value, &mut buffer);
        }
        buffer
    }

    pub fn get_data_segment(&self) -> &[u8] {
        &self.data_segment
    }

    pub fn get_offset(&self, value: &MmdbValue) -> Option<u32> {
        self.cache.get(value).copied()
    }


    // --- Private: cached encoding (for data section) ---

    fn encode_cached_bytes(&mut self, value: &MmdbValue, buffer: &mut Vec<u8>) {
        match value {
            MmdbValue::String(s) => {
                Self::encode_header(MmdbTypeID::STRING, s.len(), buffer);
                buffer.extend_from_slice(s.as_bytes());
            }
            MmdbValue::Double(d) => {
                Self::encode_header(MmdbTypeID::DOUBLE, 8, buffer);
                buffer.write_f64::<BigEndian>(*d).unwrap();
            }
            MmdbValue::Bytes(b) => {
                Self::encode_header(MmdbTypeID::BYTES, b.len(), buffer);
                buffer.extend_from_slice(b);
            }
            MmdbValue::Uint16(u) => Self::encode_uint(MmdbTypeID::UINT16, *u as u128, 2, buffer),
            MmdbValue::Uint32(u) => Self::encode_uint(MmdbTypeID::UINT32, *u as u128, 4, buffer),
            MmdbValue::Map(m) => {
                Self::encode_header(MmdbTypeID::MAP, m.len(), buffer);
                for (key, val) in m {
                    let key_val = MmdbValue::String(key.clone());
                    let key_bytes = self.encode_value(&key_val);
                    buffer.extend_from_slice(&key_bytes);
                    let val_bytes = self.encode_value(val);
                    buffer.extend_from_slice(&val_bytes);
                }
            }
            MmdbValue::Int32(i) => {
                Self::encode_header(MmdbTypeID::INT32, 4, buffer);
                buffer.write_i32::<BigEndian>(*i).unwrap();
            }
            MmdbValue::Uint64(u) => Self::encode_uint(MmdbTypeID::UINT64, *u as u128, 8, buffer),
            MmdbValue::Uint128(u) => Self::encode_uint(MmdbTypeID::UINT128, *u, 16, buffer),
            MmdbValue::Array(a) => {
                Self::encode_header(MmdbTypeID::ARRAY, a.len(), buffer);
                for item in a {
                    let item_bytes = self.encode_value(item);
                    buffer.extend_from_slice(&item_bytes);
                }
            }
            MmdbValue::Boolean(b) => {
                Self::encode_header(MmdbTypeID::BOOLEAN, if *b { 1 } else { 0 }, buffer);
            }
            MmdbValue::Float(f) => {
                Self::encode_header(MmdbTypeID::FLOAT, 4, buffer);
                buffer.write_f32::<BigEndian>(*f).unwrap();
            }
        }
    }

    // --- Private: inline encoding (for metadata) ---

    fn encode_inline_recursive(value: &MmdbValue, buffer: &mut Vec<u8>) {
        match value {
            MmdbValue::String(s) => {
                Self::encode_header(MmdbTypeID::STRING, s.len(), buffer);
                buffer.extend_from_slice(s.as_bytes());
            }
            MmdbValue::Double(d) => {
                Self::encode_header(MmdbTypeID::DOUBLE, 8, buffer);
                buffer.write_f64::<BigEndian>(*d).unwrap();
            }
            MmdbValue::Bytes(b) => {
                Self::encode_header(MmdbTypeID::BYTES, b.len(), buffer);
                buffer.extend_from_slice(b);
            }
            MmdbValue::Uint16(u) => Self::encode_uint(MmdbTypeID::UINT16, *u as u128, 2, buffer),
            MmdbValue::Uint32(u) => Self::encode_uint(MmdbTypeID::UINT32, *u as u128, 4, buffer),
            MmdbValue::Map(m) => {
                Self::encode_header(MmdbTypeID::MAP, m.len(), buffer);
                for (key, val) in m {
                    Self::encode_header(MmdbTypeID::STRING, key.len(), buffer);
                    buffer.extend_from_slice(key.as_bytes());
                    Self::encode_inline_recursive(val, buffer);
                }
            }
            MmdbValue::Int32(i) => {
                Self::encode_header(MmdbTypeID::INT32, 4, buffer);
                buffer.write_i32::<BigEndian>(*i).unwrap();
            }
            MmdbValue::Uint64(u) => Self::encode_uint(MmdbTypeID::UINT64, *u as u128, 8, buffer),
            MmdbValue::Uint128(u) => Self::encode_uint(MmdbTypeID::UINT128, *u, 16, buffer),
            MmdbValue::Array(a) => {
                Self::encode_header(MmdbTypeID::ARRAY, a.len(), buffer);
                for item in a {
                    Self::encode_inline_recursive(item, buffer);
                }
            }
            MmdbValue::Boolean(b) => {
                Self::encode_header(MmdbTypeID::BOOLEAN, if *b { 1 } else { 0 }, buffer);
            }
            MmdbValue::Float(f) => {
                Self::encode_header(MmdbTypeID::FLOAT, 4, buffer);
                buffer.write_f32::<BigEndian>(*f).unwrap();
            }
        }
    }

    // --- Shared encoding primitives ---

    fn encode_pointer(pointer: u32, buffer: &mut Vec<u8>) {
        if pointer < 2048 {
            buffer.push(0x20 | ((pointer >> 8) & 0x07) as u8);
            buffer.push((pointer & 0xFF) as u8);
        } else if pointer < 526336 {
            let adjusted = pointer - 2048;
            buffer.push(0x28 | ((adjusted >> 16) & 0x07) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else if pointer < 134744064 {
            let adjusted = pointer - 526336;
            buffer.push(0x30 | ((adjusted >> 24) & 0x07) as u8);
            buffer.push(((adjusted >> 16) & 0xFF) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else {
            buffer.push(0x38);
            buffer.write_u32::<BigEndian>(pointer).unwrap();
        }
    }

    fn encode_header(type_id: u8, length: usize, buffer: &mut Vec<u8>) {
        let five_bits: u8;

        if type_id <= 7 {
            if length < 29 {
                buffer.push((type_id << 5) | length as u8);
                return;
            }
            five_bits = if length < 285 { 29 } else if length < 65821 { 30 } else { 31 };
            buffer.push((type_id << 5) | five_bits);
        } else {
            five_bits = if length < 29 {
                length as u8
            } else if length < 285 {
                29
            } else if length < 65821 {
                30
            } else if length < 16843036 {
                31
            } else {
                panic!("Length too large for MMDB: {}", length);
            };
            buffer.push(five_bits);
            buffer.push(type_id - 7);
        }

        match five_bits {
            29 => buffer.push((length - 29) as u8),
            30 => {
                let adj = length - 285;
                buffer.push((adj >> 8) as u8);
                buffer.push((adj & 0xFF) as u8);
            }
            31 => {
                let adj = length - 65821;
                buffer.push(((adj >> 16) & 0xFF) as u8);
                buffer.push(((adj >> 8) & 0xFF) as u8);
                buffer.push((adj & 0xFF) as u8);
            }
            _ => {}
        }
    }

    fn encode_uint(type_id: u8, value: u128, max_bytes: usize, buffer: &mut Vec<u8>) {
        if value == 0 {
            Self::encode_header(type_id, 0, buffer);
            return;
        }

        let mut bytes = Vec::with_capacity(max_bytes);
        let mut v = value;
        while v > 0 {
            bytes.push((v & 0xFF) as u8);
            v >>= 8;
        }
        bytes.reverse();

        Self::encode_header(type_id, bytes.len(), buffer);
        buffer.extend_from_slice(&bytes);
    }
}

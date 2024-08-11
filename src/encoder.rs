use crate::value::MmdbValue;
use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashMap;
use std::hash::Hash;

struct MmdbTypeID;

impl MmdbTypeID {
    const POINTER: u8 = 1;
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
    const DATA_CACHE: u8 = 12;
    const END_MARKER: u8 = 13;
    const BOOLEAN: u8 = 14;
    const FLOAT: u8 = 15;
}

pub struct Encoder {
    pub data: Vec<u8>,
    data_cache: HashMap<MmdbValue, u32>,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            data: Vec::new(),
            data_cache: HashMap::new(),
        }
    }

    pub fn encode(&mut self, value: &MmdbValue) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(5);
        self.encode_to_buffer(value, &mut buffer);
        buffer
    }

    fn encode_to_buffer(&mut self, value: &MmdbValue, buffer: &mut Vec<u8>) {
        if let Some(&pointer) = self.data_cache.get(value) {
            Self::encode_pointer(pointer, buffer);
            return;
        }

        let start_pos = self.data.len();
        match value {
            MmdbValue::Pointer(p) => Self::encode_pointer(*p, buffer),
            MmdbValue::String(s) => Self::encode_string(s, buffer),
            MmdbValue::Double(d) => Self::encode_double(*d, buffer),
            MmdbValue::Bytes(b) => Self::encode_bytes(b, buffer),
            MmdbValue::Uint16(u) => Self::encode_uint16(*u, buffer),
            MmdbValue::Uint32(u) => Self::encode_uint32(*u, buffer),
            MmdbValue::Map(m) => self.encode_map(m, buffer),
            MmdbValue::Int32(i) => Self::encode_int32(*i, buffer),
            MmdbValue::Uint64(u) => Self::encode_uint64(*u, buffer),
            MmdbValue::Uint128(u) => Self::encode_uint128(*u, buffer),
            MmdbValue::Array(a) => self.encode_array(a, buffer),
            MmdbValue::Boolean(b) => Self::encode_boolean(*b, buffer),
            MmdbValue::Float(f) => Self::encode_float(*f, buffer),
        }

        let pointer = start_pos as u32;
        self.data_cache.insert(value.clone(), pointer);
        Self::encode_pointer(pointer, buffer);
    }

    fn encode_pointer(pointer: u32, buffer: &mut Vec<u8>) {
        if pointer >= 0x8080800 {
            buffer.push(0x38);
            buffer.extend_from_slice(&pointer.to_be_bytes());
        } else if pointer >= 0x80800 {
            let adjusted = pointer - 0x80800;
            buffer.push(0x30 | ((adjusted >> 24) & 0x07) as u8);
            buffer.push(((adjusted >> 16) & 0xFF) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else if pointer >= 0x800 {
            let adjusted = pointer - 0x800;
            buffer.push(0x28 | ((adjusted >> 16) & 0x07) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else {
            buffer.push(0x20 | ((pointer >> 8) & 0x07) as u8);
            buffer.push((pointer & 0xFF) as u8);
        }
    }

    fn encode_string(s: &str, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::STRING, s.len(), buffer);
        buffer.extend_from_slice(s.as_bytes());
    }

    fn encode_double(d: f64, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::DOUBLE, 8, buffer);
        buffer.write_f64::<BigEndian>(d).unwrap();
    }

    fn encode_bytes(b: &[u8], buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::BYTES, b.len(), buffer);
        buffer.extend_from_slice(b);
    }

    fn encode_uint16(u: u16, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::UINT16, 2, buffer);
        buffer.write_u16::<BigEndian>(u).unwrap();
    }

    fn encode_uint32(u: u32, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::UINT32, 4, buffer);
        buffer.write_u32::<BigEndian>(u).unwrap();
    }

    fn encode_map(&mut self, m: &HashMap<String, MmdbValue>, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::MAP, m.len(), buffer);
        for (key, value) in m {
            Self::encode_string(key, buffer);
            self.encode_to_buffer(value, buffer);
        }
    }

    fn encode_int32(i: i32, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::INT32, 4, buffer);
        buffer.write_i32::<BigEndian>(i).unwrap();
    }

    fn encode_uint64(u: u64, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::UINT64, 8, buffer);
        buffer.write_u64::<BigEndian>(u).unwrap();
    }

    fn encode_uint128(u: u128, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::UINT128, 16, buffer);
        buffer.write_u128::<BigEndian>(u).unwrap();
    }

    fn encode_array(&mut self, a: &[MmdbValue], buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::ARRAY, a.len(), buffer);
        for value in a {
            self.encode_to_buffer(value, buffer);
        }
    }

    fn encode_boolean(b: bool, buffer: &mut Vec<u8>) {
        buffer.push((MmdbTypeID::BOOLEAN << 5) | if b { 1 } else { 0 });
    }

    fn encode_float(f: f32, buffer: &mut Vec<u8>) {
        Self::encode_type_and_length(MmdbTypeID::FLOAT, 4, buffer);
        buffer.write_f32::<BigEndian>(f).unwrap();
    }

    fn encode_type_and_length(type_num: u8, length: usize, buffer: &mut Vec<u8>) {
        if length < 29 {
            buffer.push((type_num << 5) | length as u8);
        } else if length < 285 {
            buffer.push((type_num << 5) | 29);
            buffer.push((length - 29) as u8);
        } else if length < 65821 {
            buffer.push((type_num << 5) | 30);
            let adjusted_length = length - 285;
            buffer.push((adjusted_length >> 8) as u8);
            buffer.push((adjusted_length & 0xFF) as u8);
        } else {
            buffer.push((type_num << 5) | 31);
            buffer.write_u32::<BigEndian>(length as u32).unwrap();
        }
    }
}

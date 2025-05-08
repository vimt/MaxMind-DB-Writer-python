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
    data_segment: Vec<u8>,
    cache: HashMap<MmdbValue, u32>, // Stores MmdbValue -> offset in data_segment
    // Configuration for int/float types, if needed later for automatic conversion from rust primitives.
    // For now, MmdbValue itself carries the specific type.
    // int_type: String, // e.g., "auto", "u32", etc.
    // float_type: String, // e.g., "f64", "f32"
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            data_segment: Vec::new(),
            cache: HashMap::new(),
            // int_type: "auto".to_string(), // Default, if we add this feature
            // float_type: "f64".to_string(), // Default, if we add this feature
        }
    }

    // This is the main method called by TreeWriter for each data value.
    // It returns the bytes that should be written into the database tree/data section
    // for this value, which will often be a pointer to the data_segment.
    pub fn encode_value(&mut self, value: &MmdbValue) -> Vec<u8> {
        if let Some(&offset) = self.cache.get(value) {
            let mut pointer_bytes = Vec::new();
            Self::encode_pointer_representation(offset, &mut pointer_bytes);
            return pointer_bytes;
        }

        // If the value itself is a Pointer type, we don't cache it further.
        // We just encode its representation. This case might not be hit if TreeWriter
        // always passes application data.
        if let MmdbValue::Pointer(p_val) = value {
            let mut pointer_bytes = Vec::new();
            Self::encode_pointer_representation(*p_val, &mut pointer_bytes);
            return pointer_bytes;
        }

        let mut actual_value_bytes = Vec::new();
        self.encode_value_to_actual_bytes(value, &mut actual_value_bytes);

        let offset_for_this_value = self.data_segment.len() as u32;
        self.data_segment.extend_from_slice(&actual_value_bytes);

        self.cache.insert(value.clone(), offset_for_this_value);

        let mut pointer_bytes = Vec::new();
        Self::encode_pointer_representation(offset_for_this_value, &mut pointer_bytes);
        pointer_bytes
    }
    
    // Directly encodes a value to its byte representation (type, length, data).
    // This is used to get the bytes to store in data_segment.
    // For complex types (map/array), it recursively calls `encode_value` for children.
    fn encode_value_to_actual_bytes(&mut self, value: &MmdbValue, buffer: &mut Vec<u8>) {
        match value {
            MmdbValue::Pointer(p) => { 
                // This case implies we are writing a raw pointer value directly into the data stream,
                // not creating a pointer *to* this pointer.
                Self::encode_pointer_representation(*p, buffer);
            }
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
            MmdbValue::Uint16(u) => self.encode_uint16_data(*u, buffer),
            MmdbValue::Uint32(u) => self.encode_uint32_data(*u, buffer),
            MmdbValue::Map(m) => self.encode_map_data(m, buffer),
            MmdbValue::Int32(i) => {
                // MMDB i32 is always 4 bytes for data part
                Self::encode_header(MmdbTypeID::INT32, 4, buffer);
                buffer.write_i32::<BigEndian>(*i).unwrap();
            }
            MmdbValue::Uint64(u) => self.encode_uint64_data(*u, buffer),
            MmdbValue::Uint128(u) => self.encode_uint128_data(*u, buffer),
            MmdbValue::Array(a) => self.encode_array_data(a, buffer),
            MmdbValue::Boolean(b) => {
                // Boolean has size in control byte directly.
                buffer.push((MmdbTypeID::BOOLEAN << 5) | if *b { 1 } else { 0 });
            }
            MmdbValue::Float(f) => {
                Self::encode_header(MmdbTypeID::FLOAT, 4, buffer);
                buffer.write_f32::<BigEndian>(*f).unwrap();
            }
        }
    }

    // Encodes the pointer *value* (an offset) into its MMDB pointer type representation.
    // This is equivalent to Python's `_encode_pointer`.
    fn encode_pointer_representation(pointer: u32, buffer: &mut Vec<u8>) {
        if pointer < 2048 { // 001 SSSSS PPPPPPPPPP (S is 3 bits of size, P is 8 bits of pointer)
            buffer.push(0x20 | ((pointer >> 8) & 0x07) as u8);
            buffer.push((pointer & 0xFF) as u8);
        } else if pointer < 526336 { // 001 000 SSS P(16) (pointer - 2048)
            let adjusted = pointer - 2048;
            buffer.push(0x28 | ((adjusted >> 16) & 0x07) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else if pointer < 134744064 { // 001 001 SSS P(24) (pointer - 526336)
            let adjusted = pointer - 526336; // Corrected constant from Python's comments for 0x30
            buffer.push(0x30 | ((adjusted >> 24) & 0x07) as u8);
            buffer.push(((adjusted >> 16) & 0xFF) as u8);
            buffer.push(((adjusted >> 8) & 0xFF) as u8);
            buffer.push((adjusted & 0xFF) as u8);
        } else { // 001 010 00 P(32)
            buffer.push(0x38); // Control byte 00111000 indicates 32-bit pointer
            buffer.write_u32::<BigEndian>(pointer).unwrap();
        }
    }
    
    // Encodes type and length header for non-pointer types.
    // Equivalent to Python's `_make_header`.
    fn encode_header(type_id: u8, length: usize, buffer: &mut Vec<u8>) {
        let five_bits_len: u8;
        let mut additional_bytes: Vec<u8> = Vec::new();

        if length < 29 {
            five_bits_len = length as u8;
        } else if length < 285 { // 29 + 256
            five_bits_len = 29;
            additional_bytes.push((length - 29) as u8);
        } else if length < 65821 { // 29 + 256 + (255*256) = 285 + 65536 - 256 = 65536 + 29
                                   // Python: length >= 285 -> five_bits = 30, length -= 285
                                   // 285 + 65535 = 65820
            five_bits_len = 30;
            let adjusted_length = length - 285;
            additional_bytes.push((adjusted_length >> 8) as u8);
            additional_bytes.push((adjusted_length & 0xFF) as u8);
        } else if length < 16843036 { // 29 + 256 + 65536 + (255*256*256) = 65821 + 16777215 = 16843036
                                     // Python: length >= 65821 -> five_bits = 31, length -= 65821
            five_bits_len = 31;
            let adjusted_length = length - 65821;
             // The length is written as 3 bytes, big-endian.
            additional_bytes.push(((adjusted_length >> 16) & 0xFF) as u8);
            additional_bytes.push(((adjusted_length >> 8) & 0xFF) as u8);
            additional_bytes.push((adjusted_length & 0xFF) as u8);
        }
         else {
            panic!("Length too large for MMDB: {}", length);
        }

        if type_id <= 7 {
            buffer.push((type_id << 5) | five_bits_len);
        } else {
            // Extended type: first byte is five_bits_len, second is type_id - 7
            buffer.push(five_bits_len);
            buffer.push(type_id - 7);
        }
        buffer.extend_from_slice(&additional_bytes);
    }

    fn encode_uint_data(&self, type_id: u8, value: u128, max_bytes: usize, buffer: &mut Vec<u8>) {
        if value == 0 {
            Self::encode_header(type_id, 0, buffer);
            return;
        }
        
        let mut bytes = Vec::with_capacity(max_bytes);
        let mut temp_val = value;
        while temp_val > 0 {
            bytes.push((temp_val & 0xFF) as u8);
            temp_val >>= 8;
            if bytes.len() > max_bytes { // Should not happen if check is done prior
                 panic!("Value {} too large for {} bytes", value, max_bytes);
            }
        }
        bytes.reverse(); // to big-endian

        Self::encode_header(type_id, bytes.len(), buffer);
        buffer.extend_from_slice(&bytes);
    }

    fn encode_uint16_data(&mut self, u: u16, buffer: &mut Vec<u8>) {
        if u > 0xFFFF { panic!("Value too large for u16: {}", u); }
        self.encode_uint_data(MmdbTypeID::UINT16, u as u128, 2, buffer);
    }

    fn encode_uint32_data(&mut self, u: u32, buffer: &mut Vec<u8>) {
        if u > 0xFFFFFFFF { panic!("Value too large for u32: {}", u); }
        self.encode_uint_data(MmdbTypeID::UINT32, u as u128, 4, buffer);
    }
    
    fn encode_map_data(&mut self, m: &HashMap<String, MmdbValue>, buffer: &mut Vec<u8>) {
        Self::encode_header(MmdbTypeID::MAP, m.len(), buffer);
        for (key_str, val_mmdb) in m {
            // Keys are always strings and encoded by value (header + data)
            let mut key_actual_bytes = Vec::new();
            Self::encode_header(MmdbTypeID::STRING, key_str.len(), &mut key_actual_bytes);
            key_actual_bytes.extend_from_slice(key_str.as_bytes());
            buffer.extend_from_slice(&key_actual_bytes);

            // Values go through the full encode_value logic to handle caching/pointers
            let value_representation_bytes = self.encode_value(val_mmdb);
            buffer.extend_from_slice(&value_representation_bytes);
        }
    }

    fn encode_uint64_data(&mut self, u: u64, buffer: &mut Vec<u8>) {
        if u > 0xFFFFFFFFFFFFFFFF { panic!("Value too large for u64: {}", u); }
        self.encode_uint_data(MmdbTypeID::UINT64, u as u128, 8, buffer);
    }

    fn encode_uint128_data(&mut self, u: u128, buffer: &mut Vec<u8>) {
        self.encode_uint_data(MmdbTypeID::UINT128, u, 16, buffer);
    }

    fn encode_array_data(&mut self, a: &[MmdbValue], buffer: &mut Vec<u8>) {
        Self::encode_header(MmdbTypeID::ARRAY, a.len(), buffer);
        for item_mmdb in a {
            let item_representation_bytes = self.encode_value(item_mmdb);
            buffer.extend_from_slice(&item_representation_bytes);
        }
    }

    // Getter for the data_segment, needed by TreeWriter
    pub fn get_data_segment(&self) -> &Vec<u8> {
        &self.data_segment
    }

    // 获取缓存中的偏移量
    pub fn get_offset(&self, value: &MmdbValue) -> Option<u32> {
        self.cache.get(value).copied()
    }
}


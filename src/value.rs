use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq)]
pub enum MmdbValue {
    String(String),
    Double(f64),
    Bytes(Vec<u8>),
    Uint16(u16),
    Uint32(u32),
    Map(HashMap<String, MmdbValue>),
    Int32(i32),
    Uint64(u64),
    Uint128(u128),
    Array(Vec<MmdbValue>),
    Boolean(bool),
    Float(f32),
}

impl Eq for MmdbValue {}

impl Hash for MmdbValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MmdbValue::String(s) => s.hash(state),
            MmdbValue::Double(d) => d.to_bits().hash(state),
            MmdbValue::Bytes(b) => b.hash(state),
            MmdbValue::Uint16(u) => u.hash(state),
            MmdbValue::Uint32(u) => u.hash(state),
            MmdbValue::Map(m) => {
                for (k, v) in m {
                    k.hash(state);
                    v.hash(state);
                }
            }
            MmdbValue::Int32(i) => i.hash(state),
            MmdbValue::Uint64(u) => u.hash(state),
            MmdbValue::Uint128(u) => u.hash(state),
            MmdbValue::Array(a) => {
                for v in a {
                    v.hash(state);
                }
            }
            MmdbValue::Boolean(b) => b.hash(state),
            MmdbValue::Float(f) => f.to_bits().hash(state),
        }
    }
}

impl From<String> for MmdbValue {
    fn from(s: String) -> Self {
        MmdbValue::String(s)
    }
}
impl From<&str> for MmdbValue {
    fn from(s: &str) -> Self {
        MmdbValue::String(s.to_string())
    }
}
impl From<f64> for MmdbValue {
    fn from(f: f64) -> Self {
        MmdbValue::Double(f)
    }
}
impl From<f32> for MmdbValue {
    fn from(f: f32) -> Self {
        MmdbValue::Float(f)
    }
}
impl From<Vec<u8>> for MmdbValue {
    fn from(b: Vec<u8>) -> Self {
        MmdbValue::Bytes(b)
    }
}
impl From<u16> for MmdbValue {
    fn from(u: u16) -> Self {
        MmdbValue::Uint16(u)
    }
}
impl From<u32> for MmdbValue {
    fn from(u: u32) -> Self {
        MmdbValue::Uint32(u)
    }
}
impl From<i32> for MmdbValue {
    fn from(i: i32) -> Self {
        MmdbValue::Int32(i)
    }
}
impl From<u64> for MmdbValue {
    fn from(u: u64) -> Self {
        MmdbValue::Uint64(u)
    }
}
impl From<u128> for MmdbValue {
    fn from(u: u128) -> Self {
        MmdbValue::Uint128(u)
    }
}
impl From<bool> for MmdbValue {
    fn from(b: bool) -> Self {
        MmdbValue::Boolean(b)
    }
}
impl<T: Into<MmdbValue>> From<Vec<T>> for MmdbValue {
    fn from(a: Vec<T>) -> Self {
        MmdbValue::Array(a.into_iter().map(Into::into).collect())
    }
}
impl<T: Into<MmdbValue>> From<HashMap<String, T>> for MmdbValue {
    fn from(m: HashMap<String, T>) -> Self {
        MmdbValue::Map(
            m.into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        )
    }
}
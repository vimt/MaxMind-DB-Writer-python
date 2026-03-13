mod encoder;
mod value;
mod writer;

use crate::value::MmdbValue;
use crate::writer::MMDBWriter;
use ipnetwork::IpNetwork;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyFloat, PyInt, PyList, PyTuple};
use pyo3::PyTypeInfo;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::BufWriter;

#[pymodule]
fn mmdb_writer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMmdbWriter>()?;
    m.add_class::<MmdbI32>()?;
    m.add_class::<MmdbU16>()?;
    m.add_class::<MmdbU32>()?;
    m.add_class::<MmdbU64>()?;
    m.add_class::<MmdbU128>()?;
    Ok(())
}

fn py_type_err(e: impl Debug) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!("{e:?}"))
}

fn py_runtime_err(e: impl Debug) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e:?}"))
}

fn py_value_err(msg: impl Into<String>) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(msg.into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IntType {
    Auto,
    Uint16,
    Uint32,
    Uint64,
    Uint128,
    Int32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FloatType {
    Auto,
    Float,
    Double,
}

#[pyclass(name = "MmdbWriter")]
struct PyMmdbWriter {
    inner: MMDBWriter,
    int_type: IntType,
    float_type: FloatType,
}

fn parse_int_type(obj: &Bound<'_, pyo3::PyAny>) -> PyResult<IntType> {
    if let Ok(s) = obj.extract::<String>() {
        return match s.as_str() {
            "auto" => Ok(IntType::Auto),
            "u16" | "uint16" => Ok(IntType::Uint16),
            "u32" | "uint32" => Ok(IntType::Uint32),
            "u64" | "uint64" => Ok(IntType::Uint64),
            "u128" | "uint128" => Ok(IntType::Uint128),
            "i32" | "int32" => Ok(IntType::Int32),
            _ => Err(py_value_err(format!(
                "Invalid int type: {s}, expected: auto, u16, u32, u64, u128, i32"
            ))),
        };
    }

    let py = obj.py();
    if obj.is(&MmdbI32::type_object(py)) || obj.is_instance_of::<MmdbI32>() {
        Ok(IntType::Int32)
    } else if obj.is(&MmdbU16::type_object(py)) || obj.is_instance_of::<MmdbU16>() {
        Ok(IntType::Uint16)
    } else if obj.is(&MmdbU32::type_object(py)) || obj.is_instance_of::<MmdbU32>() {
        Ok(IntType::Uint32)
    } else if obj.is(&MmdbU64::type_object(py)) || obj.is_instance_of::<MmdbU64>() {
        Ok(IntType::Uint64)
    } else if obj.is(&MmdbU128::type_object(py)) || obj.is_instance_of::<MmdbU128>() {
        Ok(IntType::Uint128)
    } else {
        Err(py_value_err(format!(
            "Invalid int type, expected: auto, u16, u32, u64, u128, i32 or MmdbXxx class"
        )))
    }
}

fn parse_float_type(obj: &Bound<'_, pyo3::PyAny>) -> PyResult<FloatType> {
    if let Ok(s) = obj.extract::<String>() {
        match s.as_str() {
            "auto" => Ok(FloatType::Auto),
            "f32" | "float" | "float32" => Ok(FloatType::Float),
            "f64" | "double" | "float64" => Ok(FloatType::Double),
            _ => Err(py_value_err(format!(
                "Invalid float type: {s}, expected: auto, f32, f64"
            ))),
        }
    } else {
        Err(py_value_err("Invalid float type"))
    }
}

#[pymethods]
impl PyMmdbWriter {
    #[new]
    #[pyo3(signature = (
        ip_version = 4,
        database_type = "GeoIP",
        languages = Vec::<String>::new(),
        description = HashMap::<String, String>::new(),
        ipv4_compatible = false,
        int_type = None,
        float_type = None,
    ))]
    fn new(
        ip_version: u8,
        database_type: &str,
        languages: Vec<String>,
        description: HashMap<String, String>,
        ipv4_compatible: bool,
        int_type: Option<&Bound<'_, pyo3::PyAny>>,
        float_type: Option<&Bound<'_, pyo3::PyAny>>,
    ) -> PyResult<Self> {
        let int_type = match int_type {
            Some(obj) => parse_int_type(obj)?,
            None => IntType::Auto,
        };
        let float_type = match float_type {
            Some(obj) => parse_float_type(obj)?,
            None => FloatType::Double,
        };

        Ok(Self {
            inner: MMDBWriter::new(
                ip_version,
                database_type.to_string(),
                languages,
                description,
                ipv4_compatible,
            ),
            int_type,
            float_type,
        })
    }

    fn to_file(&mut self, filename: &str) -> PyResult<()> {
        let file = File::create(filename)?;
        let mut writer = BufWriter::new(file);
        self.inner
            .build(&mut writer)
            .map_err(|e| py_runtime_err(format!("Build failed: {e}")))?;
        Ok(())
    }

    fn insert_network(
        &mut self,
        network: &str,
        content: &Bound<'_, pyo3::PyAny>,
    ) -> PyResult<()> {
        let network = network
            .parse::<IpNetwork>()
            .map_err(|e| py_value_err(format!("Invalid network {network}: {e}")))?;
        let value = self.python_to_mmdb_value(content)?;
        self.inner
            .insert_network(network, value)
            .map_err(|e| py_runtime_err(format!("Insert failed: {e}")))
    }

    /// Batch insert: accepts an iterable of (network_str, data) tuples.
    /// Crossing the Python→Rust boundary once for the whole batch eliminates
    /// per-call overhead and yields much higher throughput for large datasets.
    fn insert_networks(&mut self, records: &Bound<'_, pyo3::PyAny>) -> PyResult<()> {
        let iter = records.try_iter()?;
        for item in iter {
            let item = item?;
            let (net_obj, data_obj) = if let Ok(t) = item.downcast::<PyTuple>() {
                if t.len() != 2 {
                    return Err(py_value_err("Each record must be a (network, data) tuple"));
                }
                (t.get_item(0)?, t.get_item(1)?)
            } else if let Ok(l) = item.downcast::<PyList>() {
                if l.len() != 2 {
                    return Err(py_value_err("Each record must be a [network, data] list"));
                }
                (l.get_item(0)?, l.get_item(1)?)
            } else {
                return Err(py_value_err(
                    "Each record must be a (network, data) tuple or list",
                ));
            };

            let network_str = net_obj.extract::<String>()?;
            let network = network_str
                .parse::<IpNetwork>()
                .map_err(|e| py_value_err(format!("Invalid network {network_str}: {e}")))?;
            let value = self.python_to_mmdb_value(&data_obj)?;
            self.inner
                .insert_network(network, value)
                .map_err(|e| py_runtime_err(format!("Insert failed: {e}")))?;
        }
        Ok(())
    }

}

impl PyMmdbWriter {
    fn python_to_mmdb_value(&self, obj: &Bound<'_, pyo3::PyAny>) -> PyResult<MmdbValue> {
        if let Ok(dict) = obj.downcast::<PyDict>() {
            let mut map = HashMap::new();
            for (key, value) in dict.iter() {
                let key = key.extract::<String>()?;
                let value = self.python_to_mmdb_value(&value)?;
                map.insert(key, value);
            }
            return Ok(MmdbValue::Map(map));
        }
        if let Ok(list) = obj.downcast::<PyList>() {
            let mut vec = Vec::new();
            for item in list.iter() {
                vec.push(self.python_to_mmdb_value(&item)?);
            }
            return Ok(MmdbValue::Array(vec));
        }
        if let Ok(bytes) = obj.downcast::<PyBytes>() {
            return Ok(MmdbValue::Bytes(bytes.as_bytes().to_vec()));
        }
        if let Ok(s) = obj.extract::<String>() {
            return Ok(MmdbValue::String(s));
        }

        // Bool MUST be checked before int (Python's bool is a subclass of int)
        if obj.is_instance_of::<pyo3::types::PyBool>() {
            let b = obj.extract::<bool>()?;
            return Ok(MmdbValue::Boolean(b));
        }

        if obj.is_instance_of::<PyFloat>() {
            let f = obj.extract::<f64>()?;
            return match self.float_type {
                FloatType::Float => Ok(MmdbValue::Float(f as f32)),
                FloatType::Double | FloatType::Auto => Ok(MmdbValue::Double(f)),
            };
        }

        if obj.is_instance_of::<PyInt>() {
            return self.convert_int(obj);
        }

        Err(py_type_err(format!("Unsupported type: {:?}", obj)))
    }

    fn convert_int(&self, obj: &Bound<'_, pyo3::PyAny>) -> PyResult<MmdbValue> {
        match self.int_type {
            IntType::Int32 => {
                let v = obj
                    .extract::<i32>()
                    .map_err(|_| py_value_err("value out of range for i32"))?;
                Ok(MmdbValue::Int32(v))
            }
            IntType::Uint16 => {
                let v = obj
                    .extract::<u16>()
                    .map_err(|_| py_value_err("value out of range for u16"))?;
                Ok(MmdbValue::Uint16(v))
            }
            IntType::Uint32 => {
                let v = obj
                    .extract::<u32>()
                    .map_err(|_| py_value_err("value out of range for u32"))?;
                Ok(MmdbValue::Uint32(v))
            }
            IntType::Uint64 => {
                let v = obj
                    .extract::<u64>()
                    .map_err(|_| py_value_err("value out of range for u64"))?;
                Ok(MmdbValue::Uint64(v))
            }
            IntType::Uint128 => {
                let v = obj
                    .extract::<u128>()
                    .map_err(|_| py_value_err("value out of range for u128"))?;
                Ok(MmdbValue::Uint128(v))
            }
            IntType::Auto => {
                if let Ok(i) = obj.extract::<i64>() {
                    if i < 0 {
                        Ok(MmdbValue::Int32(i as i32))
                    } else if i <= u16::MAX as i64 {
                        Ok(MmdbValue::Uint16(i as u16))
                    } else if i <= u32::MAX as i64 {
                        Ok(MmdbValue::Uint32(i as u32))
                    } else {
                        Ok(MmdbValue::Uint64(i as u64))
                    }
                } else if let Ok(u) = obj.extract::<u128>() {
                    if u <= u64::MAX as u128 {
                        Ok(MmdbValue::Uint64(u as u64))
                    } else {
                        Ok(MmdbValue::Uint128(u))
                    }
                } else {
                    Err(py_value_err("integer value too large"))
                }
            }
        }
    }
}

#[pyclass(name = "MmdbI32")]
#[derive(Clone, Copy)]
pub struct MmdbI32;

#[pymethods]
impl MmdbI32 {
    #[new]
    fn new() -> Self {
        MmdbI32
    }
}

#[pyclass(name = "MmdbU16")]
#[derive(Clone, Copy)]
pub struct MmdbU16;

#[pymethods]
impl MmdbU16 {
    #[new]
    fn new() -> Self {
        MmdbU16
    }
}

#[pyclass(name = "MmdbU32")]
#[derive(Clone, Copy)]
pub struct MmdbU32;

#[pymethods]
impl MmdbU32 {
    #[new]
    fn new() -> Self {
        MmdbU32
    }
}

#[pyclass(name = "MmdbU64")]
#[derive(Clone, Copy)]
pub struct MmdbU64;

#[pymethods]
impl MmdbU64 {
    #[new]
    fn new() -> Self {
        MmdbU64
    }
}

#[pyclass(name = "MmdbU128")]
#[derive(Clone, Copy)]
pub struct MmdbU128;

#[pymethods]
impl MmdbU128 {
    #[new]
    fn new() -> Self {
        MmdbU128
    }
}

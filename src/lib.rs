mod encoder;
mod value;
mod writer;

use crate::value::MmdbValue;
use crate::writer::MMDBWriter;
use ipnetwork::IpNetwork;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;

#[pymodule]
fn mmdb_writer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMmdbWriter>()?;
    m.add_class::<PyMmdbValue>()?;
    Ok(())
}

pub fn py_type_err(e: impl Debug) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!("{e:?}"))
}

pub fn py_runtime_err(e: impl Debug) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e:?}"))
}

pub fn py_value_err(e: impl Debug) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{e:?}"))
}

#[pyclass(name = "MmdbValue")]
#[derive(Copy, Clone)]
enum PyMmdbValue {
    Float(f32),
    Double(f64),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Uint128(u128),
    Int32(i32),
}

impl PyMmdbValue {
    pub fn to_mmdb_value(&self) -> MmdbValue {
        match *self {
            PyMmdbValue::Float(f) => MmdbValue::Float(f),
            PyMmdbValue::Double(d) => MmdbValue::Double(d),
            PyMmdbValue::Uint16(u) => MmdbValue::Uint16(u),
            PyMmdbValue::Uint32(u) => MmdbValue::Uint32(u),
            PyMmdbValue::Uint64(u) => MmdbValue::Uint64(u),
            PyMmdbValue::Uint128(u) => MmdbValue::Uint128(u),
            PyMmdbValue::Int32(i) => MmdbValue::Int32(i),
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(PartialOrd, PartialEq)]
enum IntType {
    Uint16,
    Uint32,
    Uint64,
    Uint128,
    Int32,
}

#[pyclass(eq, eq_int)]
#[derive(PartialOrd, PartialEq)]
enum FloatType {
    Float,
    Double,
}

enum Description {
    String(String),
    Map(HashMap<String, String>),
}

fn get_language(obj: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if let Ok(s) = obj.extract::<String>() {
        Ok(vec![s])
    } else if let Ok(s) = obj.extract::<Vec<String>>() {
        Ok(s)
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Invalid language type",
        ))
    }
}

#[pyclass(name = "MmdbWriter")]
struct PyMmdbWriter {
    inner: MMDBWriter,
    int_type: Option<IntType>,
    float_type: Option<FloatType>,
}

#[pymethods]
impl PyMmdbWriter {
    #[new]
    #[pyo3(signature = (
        ip_version=4,
        database_type="GeoIP",
        languages=Vec::<String>::new(),
        description=HashMap::<String, String>::new(),
        ipv4_compatible=false, int_type="auto",
        float_type="f64")
    )]
    fn new(
        ip_version: u8,
        database_type: &str,
        languages: Vec<String>,
        description: HashMap<String, String>,
        ipv4_compatible: bool,
        int_type: &str,
        float_type: &str,
    ) -> PyResult<Self> {
        let int_type = match int_type {
            "uint16" | "u16" => Some(IntType::Uint16),
            "uint32" | "u32" => Some(IntType::Uint32),
            "uint64" | "u64" => Some(IntType::Uint64),
            "uint128" | "u128" => Some(IntType::Uint128),
            "int32" | "i32" => Some(IntType::Int32),
            "auto" => None,
            _ => {
                return Err(py_value_err(format!(
                    "Invalid int type: {}, possible value: u16, u32, u64, u128, i32, auto",
                    int_type
                )))
            }
        };
        let float_type = match float_type {
            "float" | "f32" => Some(FloatType::Float),
            "double" | "f64" => Some(FloatType::Double),
            "auto" => None,
            _ => {
                return Err(py_value_err(format!(
                    "Invalid float type: {}, possible value: f32, f64, auto",
                    float_type
                )))
            }
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

    fn to_file(&self, filename: &str) -> PyResult<()> {
        let mut file = File::create(filename)?;
        self.inner.build(&mut file);
        Ok(())
    }

    fn insert_network(&mut self, network: &str, content: &Bound<'_, PyAny>) -> PyResult<()> {
        let network = network
            .parse::<IpNetwork>()
            .map_err(|e| py_value_err(format!("Invalid network {}: {}", network, e)))?;
        let value = self.python_to_mmdb_value(content)?;
        self.inner
            .insert_network(network, value)
            .map_err(|e| py_runtime_err(format!("Failed to insert network: {}", e)))
    }
}

impl PyMmdbWriter {
    fn python_to_mmdb_value(&self, obj: &Bound<'_, PyAny>) -> PyResult<MmdbValue> {
        // Check if the object is a PyMmdbValue
        if let Ok(s) = obj.extract::<PyMmdbValue>() {
            return Ok(s.to_mmdb_value());
        }

        // Check if global int_type or float_type is set
        if let Some(int_type) = &self.int_type {
            match int_type {
                IntType::Int32 => {
                    if let Ok(i) = obj.extract::<i32>() {
                        return Ok(MmdbValue::Int32(i));
                    }
                }
                IntType::Uint16 => {
                    if let Ok(u) = obj.extract::<u16>() {
                        return Ok(MmdbValue::Uint16(u));
                    }
                }
                IntType::Uint32 => {
                    if let Ok(u) = obj.extract::<u32>() {
                        return Ok(MmdbValue::Uint32(u));
                    }
                }
                IntType::Uint64 => {
                    if let Ok(u) = obj.extract::<u64>() {
                        return Ok(MmdbValue::Uint64(u));
                    }
                }
                IntType::Uint128 => {
                    if let Ok(u) = obj.extract::<u128>() {
                        return Ok(MmdbValue::Uint128(u));
                    }
                }
            }
        }
        if let Some(float_type) = &self.float_type {
            match float_type {
                FloatType::Float => {
                    if let Ok(f) = obj.extract::<f32>() {
                        return Ok(MmdbValue::Float(f));
                    }
                }
                FloatType::Double => {
                    if let Ok(f) = obj.extract::<f64>() {
                        return Ok(MmdbValue::Double(f));
                    }
                }
            }
        }

        // auto detect type
        if let Ok(s) = obj.extract::<String>() {
            Ok(MmdbValue::String(s))
        } else if let Ok(b) = obj.extract::<bool>() {
            Ok(MmdbValue::Boolean(b))
        } else if let Ok(i) = obj.extract::<i32>() {
            Ok(MmdbValue::Int32(i))
        } else if let Ok(u) = obj.extract::<u32>() {
            Ok(MmdbValue::Uint32(u))
        } else if let Ok(f) = obj.extract::<f64>() {
            Ok(MmdbValue::Double(f))
            // TODO u128?
        } else if let Ok(dict) = obj.downcast::<PyDict>() {
            let mut map = HashMap::new();
            for (key, value) in dict.iter() {
                let key = key.extract::<String>()?;
                let value = self.python_to_mmdb_value(&value)?;
                map.insert(key, value);
            }
            Ok(MmdbValue::Map(map))
        } else if let Ok(list) = obj.downcast::<PyList>() {
            let mut vec = Vec::new();
            for item in list.iter() {
                vec.push(self.python_to_mmdb_value(&item)?);
            }
            Ok(MmdbValue::Array(vec))
        } else if let Ok(bytes) = obj.downcast::<PyBytes>() {
            Ok(MmdbValue::Bytes(bytes.as_bytes().to_vec()))
        } else {
            Err(py_type_err(format!("Unsupported variant: {:?}", obj)))
        }
    }
}

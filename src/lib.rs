// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

pub mod converter;
pub mod error;
pub mod schema;

use std::path::Path;

use pyo3::prelude::*;

use crate::converter::walker;
use crate::error::X2JError;
use crate::schema::model::SchemaModel;
use crate::schema::parser::parse_xsd;

/// A parsed XSD schema that can be reused across multiple conversions.
#[pyclass]
struct Schema {
    model: SchemaModel,
}

#[pymethods]
impl Schema {
    /// Parse an XSD file into a reusable Schema object.
    #[staticmethod]
    #[pyo3(signature = (xsd_path,))]
    fn from_file(xsd_path: &str) -> PyResult<Self> {
        let model = parse_xsd(Path::new(xsd_path))?;
        Ok(Schema { model })
    }

    /// Parse XSD bytes into a reusable Schema object.
    #[staticmethod]
    #[pyo3(signature = (xsd_bytes,))]
    fn from_bytes(xsd_bytes: &[u8]) -> PyResult<Self> {
        let xsd_str = std::str::from_utf8(xsd_bytes)
            .map_err(|e| X2JError::Io(format!("XSD is not valid UTF-8: {}", e)))?;

        let tmp_dir = std::env::temp_dir();
        let xsd_tmp = tmp_dir.join("_xml2json_temp.xsd");
        std::fs::write(&xsd_tmp, xsd_str)
            .map_err(|e| X2JError::Io(format!("Failed to write temp XSD: {}", e)))?;

        let model = parse_xsd(&xsd_tmp)?;
        let _ = std::fs::remove_file(&xsd_tmp);

        Ok(Schema { model })
    }
}

/// Convert an XML file to JSON using an XSD schema, returning the JSON as a string.
#[pyfunction]
#[pyo3(signature = (xml_path, xsd_path))]
fn convert(xml_path: &str, xsd_path: &str) -> PyResult<String> {
    let schema = parse_xsd(Path::new(xsd_path))?;
    let xml_content = std::fs::read_to_string(xml_path)
        .map_err(|e| X2JError::Io(format!("Failed to read {}: {}", xml_path, e)))?;
    let result = walker::convert_to_string(&xml_content, &schema)?;
    Ok(result)
}

/// Convert an XML file to a JSON file using an XSD schema. Streaming, low memory.
#[pyfunction]
#[pyo3(signature = (xml_path, xsd_path, output_path))]
fn convert_to_file(
    py: Python<'_>,
    xml_path: &str,
    xsd_path: &str,
    output_path: &str,
) -> PyResult<()> {
    py.allow_threads(|| {
        let schema = parse_xsd(Path::new(xsd_path))?;
        walker::convert_file(Path::new(xml_path), &schema, Path::new(output_path))?;
        Ok::<(), X2JError>(())
    })?;
    Ok(())
}

/// Convert XML bytes + XSD bytes to a JSON string (fully in-memory).
#[pyfunction]
#[pyo3(signature = (xml_bytes, xsd_bytes))]
fn convert_bytes(xml_bytes: &[u8], xsd_bytes: &[u8]) -> PyResult<String> {
    let xsd_str = std::str::from_utf8(xsd_bytes)
        .map_err(|e| X2JError::Io(format!("XSD is not valid UTF-8: {}", e)))?;
    let xml_str = std::str::from_utf8(xml_bytes)
        .map_err(|e| X2JError::Io(format!("XML is not valid UTF-8: {}", e)))?;

    // Write XSD to a temp file so the parser can resolve relative imports
    let tmp_dir = std::env::temp_dir();
    let xsd_tmp = tmp_dir.join("_xml2json_temp.xsd");
    std::fs::write(&xsd_tmp, xsd_str)
        .map_err(|e| X2JError::Io(format!("Failed to write temp XSD: {}", e)))?;

    let schema = parse_xsd(&xsd_tmp)?;
    let _ = std::fs::remove_file(&xsd_tmp);

    let result = walker::convert_to_string(xml_str, &schema)?;
    Ok(result)
}

/// Convert an XML file to JSON using a pre-parsed Schema, returning the JSON as a string.
#[pyfunction]
#[pyo3(signature = (xml_path, schema))]
fn convert_with_schema(xml_path: &str, schema: &Schema) -> PyResult<String> {
    let xml_content = std::fs::read_to_string(xml_path)
        .map_err(|e| X2JError::Io(format!("Failed to read {}: {}", xml_path, e)))?;
    let result = walker::convert_to_string(&xml_content, &schema.model)?;
    Ok(result)
}

/// Convert an XML file to a JSON file using a pre-parsed Schema. Streaming, low memory.
#[pyfunction]
#[pyo3(signature = (xml_path, schema, output_path))]
fn convert_to_file_with_schema(
    xml_path: &str,
    schema: &Schema,
    output_path: &str,
) -> PyResult<()> {
    walker::convert_file(Path::new(xml_path), &schema.model, Path::new(output_path))?;
    Ok(())
}

/// Convert XML bytes to a JSON string using a pre-parsed Schema (fully in-memory).
#[pyfunction]
#[pyo3(signature = (xml_bytes, schema))]
fn convert_bytes_with_schema(xml_bytes: &[u8], schema: &Schema) -> PyResult<String> {
    let xml_str = std::str::from_utf8(xml_bytes)
        .map_err(|e| X2JError::Io(format!("XML is not valid UTF-8: {}", e)))?;
    let result = walker::convert_to_string(xml_str, &schema.model)?;
    Ok(result)
}

#[pymodule]
fn xml2json(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<Schema>()?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(convert_to_file, m)?)?;
    m.add_function(wrap_pyfunction!(convert_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(convert_with_schema, m)?)?;
    m.add_function(wrap_pyfunction!(convert_to_file_with_schema, m)?)?;
    m.add_function(wrap_pyfunction!(convert_bytes_with_schema, m)?)?;
    Ok(())
}

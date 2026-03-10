// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use pyo3::exceptions::PyRuntimeError;
use pyo3::PyErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum X2JError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("XSD parse error: {0}")]
    XsdParse(String),

    #[error("Conversion error: {0}")]
    Conversion(String),
}

impl From<X2JError> for PyErr {
    fn from(err: X2JError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

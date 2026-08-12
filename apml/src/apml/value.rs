//! The value model of parsed variables.
//!
//! A `Context` maps variable names to [`Value`]s, which can be either a
//! scalar string or a Bash array.

use serde::{Deserialize, Serialize};

/// The value of a parsed variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A scalar string value, e.g. `PKGDEP="a b c"`.
    Scalar(String),
    /// A Bash array, e.g. `CMAKE_AFTER=("a" "b")`.
    Array(Vec<String>),
}

impl Value {
    /// Returns the scalar value if this is a scalar.
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            Value::Scalar(s) => Some(s),
            Value::Array(_) => None,
        }
    }

    /// Returns the array elements if this is an array.
    pub fn as_array(&self) -> Option<&Vec<String>> {
        match self {
            Value::Array(a) => Some(a),
            Value::Scalar(_) => None,
        }
    }

    /// Joins the value into a single string: scalars are returned as-is,
    /// arrays are joined with `sep` (matching `"${arr[@]}"` behaviour in a
    /// scalar context).
    pub fn join(&self, sep: &str) -> String {
        match self {
            Value::Scalar(s) => s.clone(),
            Value::Array(a) => a.join(sep),
        }
    }
}

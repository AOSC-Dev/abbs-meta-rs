use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PackageError {
    pub pkgname: String,
    pub error: PackageErrorType,
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to process {}: {}", self.pkgname, self.error)
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum PackageErrorType {
    MissingField(String),
    FieldTypeError(String, String),
    FieldSyntaxError(String),
}

impl fmt::Display for PackageErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageErrorType::MissingField(field_name) => {
                write!(f, "Field {} missing.", &field_name)
            }
            PackageErrorType::FieldTypeError(field_name, supposed_type) => {
                write!(
                    f,
                    "Field {} cannot be parsed as {}.",
                    field_name, supposed_type
                )
            }
            PackageErrorType::FieldSyntaxError(field_name) => {
                write!(f, "Malformed syntax for field {}.", field_name)
            }
        }
    }
}

impl std::error::Error for PackageError {}

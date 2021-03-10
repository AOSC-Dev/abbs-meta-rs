use abbs_meta_apml::ParseError;
use std::fmt;

#[derive(Debug)]
pub enum PackageError {
    MissingField(String),
    FieldTypeError(String, String),
    ParseError(ParseError),
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageError::MissingField(field_name) => {
                write!(f, "Field {} missing.", &field_name)
            }
            PackageError::FieldTypeError(field_name, supposed_type) => {
                write!(
                    f,
                    "Field {} cannot be parsed as {}.",
                    field_name, supposed_type
                )
            }
            PackageError::ParseError(err) => {
                write!(f, "{}", err)
            }
        }
    }
}

impl std::error::Error for PackageError {}

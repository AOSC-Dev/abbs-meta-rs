use crate::package::PackageError;
use abbs_meta_apml::ParseError;
use std::fmt;

#[derive(Debug)]
pub enum TreeError {
    FsError(String),
    ParseError(ParseError),
    PackageError(PackageError),
}

impl From<walkdir::Error> for TreeError {
    fn from(err: walkdir::Error) -> Self {
        TreeError::FsError(err.to_string())
    }
}

impl From<std::io::Error> for TreeError {
    fn from(err: std::io::Error) -> Self {
        TreeError::FsError(err.to_string())
    }
}

impl From<ParseError> for TreeError {
    fn from(err: ParseError) -> Self {
        TreeError::ParseError(err)
    }
}

impl From<PackageError> for TreeError {
    fn from(err: PackageError) -> Self {
        TreeError::PackageError(err)
    }
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => write!(f, ""),
        }
    }
}

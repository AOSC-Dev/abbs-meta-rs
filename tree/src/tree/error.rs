use crate::package::PackageError;
use abbs_meta_apml::ParseError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum TreeError {
    #[error("Filesystem error: {0}")]
    FsError(String),
    #[error("Parse error: {0}")]
    ParseError(#[from] ParseError),
    #[error("Package error: {0}")]
    PackageError(#[from] PackageError),
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

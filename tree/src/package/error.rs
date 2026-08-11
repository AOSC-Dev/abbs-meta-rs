#[derive(Debug, Clone, Hash, Eq, PartialEq, thiserror::Error)]
#[error("Failed to process {pkgname}: {error}")]
pub struct PackageError {
    pub pkgname: String,
    pub error: PackageErrorType,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, thiserror::Error)]
pub enum PackageErrorType {
    #[error("Field {0} missing.")]
    MissingField(String),
    #[error("Field {0} cannot be parsed as {1}.")]
    FieldTypeError(String, String),
    #[error("Malformed syntax for field {0}.")]
    FieldSyntaxError(String),
    #[error("Invalid PKGSEC: {0}.")]
    InvalidPKGSECError(String),
}

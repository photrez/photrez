use std::error::Error;
use std::fmt;

/// Typed failures at native service boundaries.
#[derive(Debug)]
pub enum ServiceError {
    Validation(String),
    ResourceLimit(String),
    Conflict(String),
    Io(std::io::Error),
    Internal(String),
}

impl ServiceError {
    /// The single mapping from Rust service failures to the standard codes.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "E_VALIDATION",
            Self::ResourceLimit(_) => "E_RESOURCE_LIMIT",
            Self::Conflict(_) => "E_CONFLICT",
            Self::Io(_) => "E_IO",
            Self::Internal(_) => "E_INTERNAL",
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => write!(formatter, "Validation failed: {message}"),
            Self::ResourceLimit(message) => write!(formatter, "Resource limit exceeded: {message}"),
            Self::Conflict(message) => write!(formatter, "Conflict: {message}"),
            Self::Io(error) => write!(formatter, "I/O failed: {error}"),
            Self::Internal(message) => write!(formatter, "Internal service error: {message}"),
        }
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::ServiceError;

    #[test]
    fn standard_service_codes_are_mapped_from_typed_variants() {
        assert_eq!(
            ServiceError::Validation("bad path".into()).code(),
            "E_VALIDATION"
        );
        assert_eq!(
            ServiceError::ResourceLimit("too large".into()).code(),
            "E_RESOURCE_LIMIT"
        );
        assert_eq!(
            ServiceError::Conflict("expired handle".into()).code(),
            "E_CONFLICT"
        );
        assert_eq!(
            ServiceError::Io(std::io::Error::other("disk failure")).code(),
            "E_IO"
        );
        assert_eq!(
            ServiceError::Internal("invariant".into()).code(),
            "E_INTERNAL"
        );
    }
}

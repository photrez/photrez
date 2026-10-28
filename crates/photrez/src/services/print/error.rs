use crate::services::error::ServiceError;
use std::fmt;

#[derive(Debug)]
pub(crate) enum PrintError {
    Service(ServiceError),
    RateLimited,
    Cancelled,
    UnsupportedPlatform,
    Printer(String),
}

impl PrintError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Service(ServiceError::Validation(message.into()))
    }

    pub(crate) fn resource_limit(message: impl Into<String>) -> Self {
        Self::Service(ServiceError::ResourceLimit(message.into()))
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Service(error) => error.code(),
            Self::RateLimited => "E_RATE_LIMIT",
            Self::Cancelled => "E_CANCELLED",
            Self::UnsupportedPlatform => "E_UNSUPPORTED_PLATFORM",
            Self::Printer(_) => "E_PRINTER",
        }
    }
}

impl From<ServiceError> for PrintError {
    fn from(error: ServiceError) -> Self {
        Self::Service(error)
    }
}

impl fmt::Display for PrintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Service(error) => fmt::Display::fmt(error, formatter),
            Self::RateLimited => write!(formatter, "Print command called too frequently"),
            Self::Cancelled => write!(formatter, "Print job was cancelled"),
            Self::UnsupportedPlatform => {
                write!(formatter, "Printing is unsupported on this platform")
            }
            Self::Printer(message) => write!(formatter, "Printer operation failed: {message}"),
        }
    }
}

impl std::error::Error for PrintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Service(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrintError;
    use crate::services::error::ServiceError;

    #[test]
    fn print_failure_kinds_keep_distinct_codes() {
        assert_eq!(
            PrintError::from(ServiceError::Validation("bad input".into())).code(),
            "E_VALIDATION"
        );
        assert_eq!(PrintError::RateLimited.code(), "E_RATE_LIMIT");
        assert_eq!(PrintError::Cancelled.code(), "E_CANCELLED");
        assert_eq!(
            PrintError::UnsupportedPlatform.code(),
            "E_UNSUPPORTED_PLATFORM"
        );
        assert_eq!(PrintError::Printer("failed".into()).code(), "E_PRINTER");
    }
}

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum RogError {
    #[error("dependency missing: {0}")]
    DependencyMissing(String),
    #[error("not supported: {0}")]
    NotSupported(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("transient failure: {0}")]
    TransientFailure(String),
    #[error("temporarily unavailable: {0}")]
    TemporarilyUnavailable(String),
    #[error("unexpected error: {0}")]
    Unexpected(String),
}

pub type RogResult<T> = Result<T, RogError>;

/// Stable error categories for presentation and diagnostics. This is internal
/// semantics over existing DBus/FDO errors, not a wire-format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    InvalidInput,
    Unsupported,
    ReadOnly,
    MissingDependency,
    PermissionDenied,
    TransientFailure,
    Unavailable,
    Unknown,
}

impl ErrorCategory {
    pub fn classify_message(message: &str) -> Self {
        let value = message.trim().to_ascii_lowercase();
        if value.contains("invalidargs")
            || value.contains("invalid args")
            || value.contains("invalid input")
        {
            Self::InvalidInput
        } else if value.contains("permission denied")
            || value.contains("accessdenied")
            || value.contains("access denied")
        {
            Self::PermissionDenied
        } else if value.contains("read-only") || value.contains("read only") {
            Self::ReadOnly
        } else if value.contains("requires asusd")
            || value.contains("requires supergfxd")
            || value.contains("dependencymissing")
            || value.contains("dependency missing")
            || value.contains("serviceunknown")
            || value.contains("name has no owner")
        {
            Self::MissingDependency
        } else if value.contains("notsupported")
            || value.contains("not supported")
            || value.contains("unsupported")
        {
            Self::Unsupported
        } else if value.contains("timedout")
            || value.contains("timed out")
            || value.contains("timeout")
            || value.contains("temporarily unavailable")
            || value.contains("unreachable")
        {
            Self::TransientFailure
        } else if value.contains("unavailable") || value.contains("not available") {
            Self::Unavailable
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: Cow<'static, str>,
    pub message: String,
}

impl ValidationWarning {
    pub fn new(code: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCategory;

    #[test]
    fn error_messages_share_stable_presentation_categories() {
        for (message, expected) in [
            (
                "org.freedesktop.DBus.Error.InvalidArgs: bad value",
                ErrorCategory::InvalidInput,
            ),
            (
                "permission denied while opening brightness",
                ErrorCategory::PermissionDenied,
            ),
            ("control is read-only", ErrorCategory::ReadOnly),
            (
                "profile control requires asusd",
                ErrorCategory::MissingDependency,
            ),
            ("operation not supported", ErrorCategory::Unsupported),
            ("backend timed out", ErrorCategory::TransientFailure),
            ("telemetry unavailable", ErrorCategory::Unavailable),
            ("an unclassified failure", ErrorCategory::Unknown),
        ] {
            assert_eq!(
                ErrorCategory::classify_message(message),
                expected,
                "{message}"
            );
        }
    }
}

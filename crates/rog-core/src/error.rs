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

use thiserror::Error;

use crate::port::SinkError;
use gridforge_store::StoreError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid envelope: {0}")]
    Validation(String),
    #[error("event sink unavailable: {0}")]
    Sink(#[from] SinkError),
    #[error("store operation failed: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub detail: String,
}

impl From<ValidationError> for AppError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error.detail)
    }
}

impl AppError {
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Validation(_) => "invalid-envelope",
            Self::Sink(_) => "sink-unavailable",
            Self::Store(_) => "store-unavailable",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Validation(_) => "Invalid envelope",
            Self::Sink(_) => "Event sink unavailable",
            Self::Store(_) => "Store unavailable",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Validation(detail) => detail.clone(),
            Self::Sink(error) => format!("event sink is {}", error.reason()),
            Self::Store(error) => error.to_string(),
        }
    }

    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::Validation(_) => None,
            Self::Sink(_) | Self::Store(_) => Some(1),
        }
    }
}

use thiserror::Error;

use crate::port::SinkError;
use gridforge_store::StoreError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("event sink unavailable: {0}")]
    Sink(#[from] SinkError),
    #[error("store operation failed: {0}")]
    Store(#[from] StoreError),
}

impl AppError {
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Sink(_) => "sink-unavailable",
            Self::Store(_) => "store-unavailable",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Sink(_) => "Event sink unavailable",
            Self::Store(_) => "Store unavailable",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Sink(error) => format!("event sink is {}", error.reason()),
            Self::Store(error) => error.to_string(),
        }
    }

    pub fn retry_after(&self) -> Option<u64> {
        Some(1)
    }
}

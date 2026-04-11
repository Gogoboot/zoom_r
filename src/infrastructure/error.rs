//! Ошибки инфраструктурного слоя.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InfraError {
    #[error("Store error: {0}")]
    Store(String),
}

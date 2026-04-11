//! Ошибки обработчиков.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("Failed to send message: {0}")]
    Send(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

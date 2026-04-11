//! Ошибки оркестратора.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Orchestration failed: {0}")]
    General(String),
}

//! Ошибки доменного слоя.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Room not found: {0}")]
    RoomNotFound(String),
    #[error("Participant not found: {0}")]
    ParticipantNotFound(String),
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

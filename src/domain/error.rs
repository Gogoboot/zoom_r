//! Ошибки доменного слоя.
// src/domain/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    // 👇 Добавь или обнови эту строку:
    #[error("Комната не найдена или уже удалена: {0}")]
    RoomNotFound(String),

    #[error("Участник не найден: {0}")]
    ParticipantNotFound(String),

    #[error("Неверное сообщение: {0}")]
    InvalidMessage(String),
}

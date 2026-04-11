//! Ошибки транспортного слоя (WebSocket, сеть).
//!
//! Этот модуль определяет [`TransportError`] — тип ошибок, связанных
//! с передачей данных через транспортные протоколы.

use thiserror::Error;

/// Ошибки транспортного уровня.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Ошибка WebSocket: разрыв соединения, протокольная ошибка и т.д.
    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

// ✅ Утилита: позволяет писать .into() для конвертации строк
// Полезно при использовании оператора ? с функциями, возвращающими String
impl From<String> for TransportError {
    fn from(s: String) -> Self {
        Self::WebSocket(s)
    }
}

impl From<&str> for TransportError {
    fn from(s: &str) -> Self {
        Self::WebSocket(s.to_string())
    }
}

//! Реестр активных соединений.
//!
//! Хранит маппинг ParticipantId -> WebSocketSender.
//! Позволяет отправлять сообщения, не храня sender внутри сущностей домена.

use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::mpsc::UnboundedSender;

pub type WebSocketSender = UnboundedSender<String>;

/// Реестр подключений.
/// Потокобезопасен благодаря Arc<DashMap>.
#[derive(Clone)]
pub struct ConnectionRegistry {
    connections: Arc<DashMap<String, WebSocketSender>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Регистрирует новый сокет для участника.
    pub fn register(&self, participant_id: String, sender: WebSocketSender) {
        self.connections.insert(participant_id, sender);
    }

    /// Удаляет сокет при отключении.
    pub fn unregister(&self, participant_id: &str) {
        self.connections.remove(participant_id);
    }

    /// Получает отправителя для конкретного участника.
    pub fn get_sender(&self, participant_id: &str) -> Option<WebSocketSender> {
        self.connections.get(participant_id).map(|entry| entry.value().clone())
    }

    /// Проверяет, онлайн ли участник.
    pub fn is_connected(&self, participant_id: &str) -> bool {
        self.connections.contains_key(participant_id)
    }
}

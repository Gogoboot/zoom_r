//! Реестр активных соединений.
use axum::extract::ws::{CloseFrame, Message};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

pub type WebSocketSender = mpsc::Sender<Message>;

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

    pub fn register(&self, participant_id: String, sender: WebSocketSender) {
        self.connections.insert(participant_id, sender);
    }

    pub fn unregister(&self, participant_id: &str) {
        self.connections.remove(participant_id);
    }

    pub fn get_sender(&self, participant_id: &str) -> Option<WebSocketSender> {
        self.connections
            .get(participant_id)
            .map(|entry| entry.value().clone())
    }

    pub fn is_connected(&self, participant_id: &str) -> bool {
        self.connections.contains_key(participant_id)
    }

    /// 🛑 Корректно закрывает все активные соединения.
    /// Отправляет WebSocket Close frame с кодом 1001 (Going Away).
    pub async fn shutdown_all(&self, code: u16, reason: &str) {
        use axum::extract::ws::{CloseCode, CloseFrame, Message};
        use tracing::info;

        let count = self.connections.len();
        if count == 0 {
            return;
        }

        tracing::info!(
            " Отправка Close frame (code: {}) {} активным соединениям...",
            code,
            count
        );

        // Используем типы axum напрямую. CloseCode implements From<u16>
        let close_frame = Message::Close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: reason.to_string().into(),
        }));

        for entry in self.connections.iter() {
            let sender = entry.value().clone();
            // Отправляем асинхронно. Ошибки во время shutdown игнорируем,
            // так как сокет может быть уже частично закрыт.
            if let Err(e) = sender.send(close_frame.clone()).await {
                tracing::debug!("Не удалось отправить Close frame участнику: {}", e);
            }
        }
        info!("Отправлено Close({}) всем {} соединениям", code, count);
    }
}

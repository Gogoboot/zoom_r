//! Оркестратор соединений.
//!
//! Управляет жизненным циклом WebSocket-подключения:
//! 1. Принимает входящие сообщения
//! 2. Маршрутизирует их к соответствующим обработчикам
//! 3. Отслеживает состояние соединения (рукопожатие / в комнате / закрыто)
//! 4. Обрабатывает ошибки и отправляет ответы клиентам
//! 5. Гарантирует очистку ресурсов при разрыве соединения
//!
//! # Архитектура
//!
//! Оркестратор параметризован трейтом [`RoomRepository`], что позволяет:
//! - Легко заменять хранилище (in-memory, Redis, etc.)
//! - Тестировать логику с моками
//! - Соблюдать принцип инверсии зависимостей (DIP)

pub mod error;

pub use error::OrchestratorError;

use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, instrument};

use crate::domain::{ClientMessage, ServerMessage};
use crate::error::{AppError, AppResult};
use crate::handlers::{handle_create_room, handle_join_room, handle_leave_room};
use crate::infrastructure::RoomRepository;
use crate::transport::split_socket;

/// Состояние активного соединения.
///
/// Явное представление состояния упрощает отладку и предотвращает ошибки,
/// связанные с неинициализированным контекстом.
#[derive(Debug, Clone)]
enum ConnectionState {
    /// Соединение установлено, но участник ещё не в комнате.
    Handshaking,
    /// Участник присоединён к комнате.
    InRoom { room_id: String, participant_id: String },
}

/// Оркестратор, управляющий одним WebSocket-соединением.
///
/// Параметризован трейтом [`RoomRepository`] для инверсии зависимостей.
///
/// # Пример
///
/// ```rust
/// # use signaling_server::{Orchestrator, MemoryRoomStore};
/// let store = MemoryRoomStore::new();
/// let orchestrator = Orchestrator::new(store);
/// // orchestrator.handle_connection(socket).await;
/// ```
pub struct Orchestrator<R: RoomRepository> {
    repo: R,
}

impl<R: RoomRepository + Clone> Orchestrator<R> {
    /// Создаёт новый оркестратор с заданным репозиторием.
    ///
    /// # Аргументы
    ///
    /// - `repo`: реализация [`RoomRepository`] для хранения данных о комнатах
    ///
    /// # Возвращает
    ///
    /// Новый экземпляр `Orchestrator<R>`
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// Обрабатывает WebSocket-соединение от начала до завершения.
    ///
    /// # Процесс
    ///
    /// 1. Разделяет сокет на каналы чтения/записи
    /// 2. Инициализирует состояние `Handshaking`
    /// 3. В цикле читает сообщения и маршрутизирует их
    /// 4. При ошибке отправляет ответ клиенту и закрывает соединение
    /// 5. При разрыве — удаляет участника из комнаты (очистка)
    ///
    /// # Аргументы
    ///
    /// - `socket`: установленное WebSocket-соединение от Axum
    #[instrument(skip(self, socket), fields(connection = "websocket"))]
    pub async fn handle_connection(&self, socket: WebSocket) {
        let (sender, mut receiver) = split_socket(socket);
        let mut state = ConnectionState::Handshaking;

        debug!("WebSocket connection established");

        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                debug!("Received message: {}", text);

                let result = match &state {
                    ConnectionState::Handshaking => {
                        self.handle_initial_message(&text, &sender).await
                    }
                    ConnectionState::InRoom { room_id, participant_id } => {
                        self.handle_message_in_room(room_id, participant_id, &text, &sender).await
                    }
                };

                match result {
                    Ok(new_state) => {
                        if let Some(new_state) = new_state {
                            state = new_state;
                            debug!("Connection state updated: {:?}", state);
                        }
                    }
                    Err(e) => {
                        error!("Error handling message: {}", e);
                        if let Err(send_err) = self.send_error(&sender, &e.to_string()).await {
                            error!("Failed to send error to client: {}", send_err);
                        }
                        break;
                    }
                }
            }
        }

        // Очистка: удаляем участника из комнаты при разрыве соединения
        if let ConnectionState::InRoom { room_id, participant_id } = state {
            debug!("Cleaning up: removing participant {} from room {}", participant_id, room_id);
            if let Err(e) = handle_leave_room(&self.repo, &room_id, &participant_id).await {
                error!("Failed to cleanup participant on disconnect: {}", e);
            }
        }

        debug!("WebSocket connection closed");
    }

    /// Отправляет ошибку клиенту в формате JSON.
    ///
    /// # Аргументы
    ///
    /// - `sender`: канал для отправки сообщений клиенту
    /// - `message`: текст ошибки для отображения
    ///
    /// # Возвращает
    ///
    /// - `Ok(())` — сообщение успешно отправлено
    /// - `Err(AppError)` — ошибка отправки или сериализации
    async fn send_error(
        &self,
        sender: &UnboundedSender<String>,
        message: &str,
    ) -> AppResult<()> {
        let err_msg = serde_json::to_string(&ServerMessage::Error {
            message: message.to_string(),
        })?;
        sender.send(err_msg)
            .map_err(|e| AppError::Transport(crate::transport::TransportError::WebSocket(
                format!("Failed to send error message: {}", e)
    )))?;
            Ok(())
    }

    /// Обрабатывает первое сообщение соединения (рукопожатие).
    ///
    /// Ожидает `CreateRoom` или `JoinRoom`. Любое другое сообщение — ошибка.
    ///
    /// # Возвращает
    ///
    /// - `Ok(Some((room_id, participant_id)))` — успешное присоединение
    /// - `Ok(None)` — не применяется для начального состояния
    /// - `Err(AppError)` — ошибка парсинга или бизнес-логики
    #[instrument(skip(self, sender))]
    async fn handle_initial_message(
        &self,
        text: &str,
        sender: &UnboundedSender<String>,
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::CreateRoom => {
                let (room_id, participant_id) = handle_create_room(&self.repo, sender.clone()).await?;
                debug!("Created room {} with participant {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            ClientMessage::JoinRoom { room_id, display_name } => {
                let (room_id, participant_id) = handle_join_room(&self.repo, room_id, display_name, sender.clone()).await?;
                debug!("Joined room {} as participant {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            _ => Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                "First message must be CreateRoom or JoinRoom".into(),
            ))),
        }
    }

    /// Обрабатывает сообщения, когда участник уже в комнате.
    ///
    /// Поддерживает: `Offer`, `Answer`, `IceCandidate`, `LeaveRoom`.
    ///
    /// # Возвращает
    ///
    /// - `Ok(Some(state))` — продолжить соединение в том же состоянии
    /// - `Ok(None)` — участник покинул комнату, соединение можно закрыть
    /// - `Err(AppError)` — ошибка обработки
/// Обрабатывает сообщения, когда участник уже в комнате.
///
/// Поддерживает: `Offer`, `Answer`, `IceCandidate`, `LeaveRoom`.
    #[instrument(skip(self, _sender), fields(room_id, participant_id))]
    async fn handle_message_in_room(
        &self,
        room_id: &str,
        participant_id: &str,
        text: &str,
        _sender: &UnboundedSender<String>,  // ← изменено на _sender
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::Offer { target_id, sdp } => {
                debug!("Forwarding offer from {} to {}", participant_id, target_id);
                crate::handlers::handle_offer(&self.repo, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::Answer { target_id, sdp } => {
                debug!("Forwarding answer from {} to {}", participant_id, target_id);
                crate::handlers::handle_answer(&self.repo, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::IceCandidate { target_id, candidate } => {
                debug!("Forwarding ICE candidate from {} to {}", participant_id, target_id);
                crate::handlers::handle_ice_candidate(&self.repo, room_id, participant_id, &target_id, candidate).await?;
            }
            ClientMessage::LeaveRoom => {
                debug!("Participant {} leaving room {}", participant_id, room_id);
                handle_leave_room(&self.repo, room_id, participant_id).await?;
                return Ok(None);
            }
            _ => {
                return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                    "Unexpected message type in room state".into(),
                )));
            }
        }

        Ok(Some(ConnectionState::InRoom {
            room_id: room_id.to_string(),
            participant_id: participant_id.to_string(),
        }))
    }
}

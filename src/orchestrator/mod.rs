//! Оркестратор соединений.
//!
//! Управляет жизненным циклом WebSocket-подключений:
//! - Принимает входящие сообщения
//! - Маршрутизирует к обработчикам (handlers)
//! - Управляет состоянием соединения (рукопожатие / в комнате)
//! - Гарантирует очистку ресурсов при разрыве связи

pub mod error;
pub use error::OrchestratorError;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{pin_mut, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{sleep, interval, Duration, Instant, MissedTickBehavior};
use tracing::{debug, error, instrument, warn};

use crate::domain::{ClientMessage, ServerMessage};
use crate::error::{AppError, AppResult};
use crate::handlers::{handle_create_room, handle_join_room, handle_leave_room};
use crate::infrastructure::{ConnectionRegistry, RoomRepository};
use crate::transport::split_socket;

// ─────────────────────────────────────────────────────────────
// Константы валидации полезных нагрузок (payload validation)
// ─────────────────────────────────────────────────────────────
const MAX_SDP_LENGTH: usize = 32_000;
const MAX_ICE_CANDIDATE_LENGTH: usize = 2_048; // Увеличили лимит для JSON-объекта
const SDP_REQUIRED_PREFIX: &str = "v=0\r\n";
// ICE-кандидаты приходят как JSON-объект, ищем ключ "candidate":
const ICE_REQUIRED_JSON_KEY: &str = "\"candidate\":";

#[derive(Debug, Clone)]
enum ConnectionState {
    Handshaking,
    InRoom { room_id: String, participant_id: String },
}

#[derive(Clone)]
pub struct Orchestrator<R: RoomRepository> {
    repo: R,
    registry: ConnectionRegistry,
}

impl<R: RoomRepository + Clone> Orchestrator<R> {
    pub fn new(repo: R, registry: ConnectionRegistry) -> Self {
        Self { repo, registry }
    }

    /// Валидирует размер и базовый формат WebRTC-сообщений.
    fn validate_signaling_message(msg: &ClientMessage) -> AppResult<()> {
        match msg {
            ClientMessage::Offer { sdp, .. } | ClientMessage::Answer { sdp, .. } => {
                if sdp.len() > MAX_SDP_LENGTH {
                    return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                        format!("SDP превышает лимит: {} байт (макс. {})", sdp.len(), MAX_SDP_LENGTH)
                    )));
                }
                if !sdp.starts_with(SDP_REQUIRED_PREFIX) {
                    return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                        "Неверный формат SDP: отсутствует заголовок 'v=0\\r\\n'".into()
                    )));
                }
            }
            ClientMessage::IceCandidate { candidate, .. } => {
                if candidate.len() > MAX_ICE_CANDIDATE_LENGTH {
                    return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                        format!("ICE-кандидат превышает лимит: {} байт", candidate.len())
                    )));
                }
                // ✅ ИСПРАВЛЕНО: проверяем наличие JSON-ключа, а не SDP-префикса
                if !candidate.contains(ICE_REQUIRED_JSON_KEY) {
                    return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                        "Неверный формат ICE-кандидата: отсутствует поле 'candidate'".into()
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }

    #[instrument(skip(self, socket), fields(connection = "websocket"))]
    pub async fn handle_connection(&self, socket: WebSocket) {
        let (sender, mut receiver) = split_socket(socket);
        let mut state = ConnectionState::Handshaking;

        let idle_timer = sleep(Duration::from_secs(60));
        pin_mut!(idle_timer);

        let mut keepalive = interval(Duration::from_secs(30));
        keepalive.set_missed_tick_behavior(MissedTickBehavior::Skip);

        debug!("WebSocket соединение установлено");

        loop {
            tokio::select! {
                biased;

                _ = keepalive.tick() => {
                    if sender.try_send(Message::Ping(vec![])).is_err() {
                        warn!("Не удалось отправить Keepalive Ping");
                        break;
                    }
                }

                frame = receiver.next() => {
                    match frame {
                        Some(Ok(msg)) => {
                            idle_timer.as_mut().reset(Instant::now() + Duration::from_secs(60));

                            match msg {
                                Message::Ping(payload) => {
                                    if sender.try_send(Message::Pong(payload)).is_err() {
                                        warn!("Не удалось отправить Pong");
                                    }
                                }
                                Message::Pong(_) => {
                                    debug!("Получен Pong (keepalive подтверждён)");
                                }
                                Message::Close(_) => {
                                    debug!("Клиент отправил Close");
                                    break;
                                }
                                Message::Text(text) => {
                                    let result = match &state {
                                        ConnectionState::Handshaking => self.handle_initial_message(&text, &sender).await,
                                        ConnectionState::InRoom { room_id, participant_id } => self.handle_message_in_room(room_id, participant_id, &text, &sender).await,
                                    };

                                    match result {
                                        Ok(Some(new_state)) => state = new_state,
                                        Ok(None) => break,
                                        Err(e) => {
                                            error!("Ошибка обработки сообщения: {}", e);
                                            let _ = self.send_error(&sender, &e.to_string()).await;
                                            break;
                                        }
                                    }
                                }
                                Message::Binary(_) => {
                                    debug!("Игнорируем бинарный кадр");
                                }
                            }
                        }
                        Some(Err(e)) => { error!("Ошибка потока WebSocket: {}", e); break; }
                        None => {
                            debug!("Поток завершён");
                            break;
                        }
                    }
                }

                _ = &mut idle_timer => {
                    warn!("⏱️ Таймаут бездействия. Закрываем соединение.");
                    break;
                }
            }
        }

        if let ConnectionState::InRoom { ref room_id, participant_id } = state {
            debug!("🧹 Очистка: удаление участника {} из комнаты {}", participant_id, room_id);
            if let Err(e) = handle_leave_room(&self.repo, &self.registry, room_id, &participant_id).await {
                error!("Не удалось выполнить очистку участника: {}", e);
            }

            if let Err(e) = self.repo.remove_if_empty(room_id).await {
                error!("Не удалось атомарно удалить пустую комнату {}: {}", room_id, e);
            }
        }

        debug!("Соединение WebSocket закрыто");
    }

    async fn send_error(&self, sender: &mpsc::Sender<Message>, message: &str) -> AppResult<()> {
        let err_msg = serde_json::to_string(&ServerMessage::Error { message: message.to_string() })?;
        match sender.try_send(Message::Text(err_msg)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("⚠️ Канал переполнен. Ошибка не доставлена.");
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(AppError::Transport(crate::transport::TransportError::WebSocket("Канал закрыт".into())))
            }
        }
    }

    #[instrument(skip(self, sender))]
    async fn handle_initial_message(
        &self,
        text: &str,
        sender: &mpsc::Sender<Message>,
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;
        Self::validate_signaling_message(&msg)?;

        match msg {
            ClientMessage::CreateRoom => {
                let (room_id, participant_id) = handle_create_room(&self.repo, &self.registry, sender.clone()).await?;
                debug!("Создана комната {} с участником {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            ClientMessage::JoinRoom { room_id, display_name } => {
                let (room_id, participant_id) = handle_join_room(&self.repo, &self.registry, room_id, display_name, sender.clone()).await?;
                debug!("Присоединение к комнате {} как участник {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            _ => Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                "Первое сообщение должно быть CreateRoom или JoinRoom".into(),
            ))),
        }
    }

    #[instrument(skip(self, _sender), fields(room_id, participant_id))]
    async fn handle_message_in_room(
        &self,
        room_id: &str,
        participant_id: &str,
        text: &str,
        _sender: &mpsc::Sender<Message>,
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;
        Self::validate_signaling_message(&msg)?;

        match msg {
            ClientMessage::Offer { target_id, sdp } => {
                debug!("Пересылка offer от {} к {}", participant_id, target_id);
                crate::handlers::handle_offer(&self.repo, &self.registry, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::Answer { target_id, sdp } => {
                debug!("Пересылка answer от {} к {}", participant_id, target_id);
                crate::handlers::handle_answer(&self.repo, &self.registry, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::IceCandidate { target_id, candidate } => {
                debug!("Пересылка ICE candidate от {} к {}", participant_id, target_id);
                crate::handlers::handle_ice_candidate(&self.repo, &self.registry, room_id, participant_id, &target_id, candidate).await?;
            }
            ClientMessage::LeaveRoom => {
                debug!("Участник {} покидает комнату {}", participant_id, room_id);
                return Ok(None);
            }
            _ => {
                return Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                    "Неожиданный тип сообщения в состоянии 'в комнате'".into(),
                )));
            }
        }
        Ok(Some(ConnectionState::InRoom {
            room_id: room_id.to_string(),
            participant_id: participant_id.to_string(),
        }))
    }

    /// 🛑 Делегирует команду завершения всех соединений в инфраструктурный слой.
    pub async fn shutdown_connections(&self, code: u16, reason: &str) {
        self.registry.shutdown_all(code, reason).await;
    }

}


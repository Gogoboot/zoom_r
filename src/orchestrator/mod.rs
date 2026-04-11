//! Оркестратор соединений.
//!
//! Управляет жизненным циклом, связывает Repository и Registry.

pub mod error;
pub use error::OrchestratorError;

use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, instrument};

use crate::domain::{ClientMessage, ServerMessage};
use crate::error::{AppError, AppResult};
use crate::handlers::{handle_create_room, handle_join_room, handle_leave_room};
use crate::infrastructure::{ConnectionRegistry, RoomRepository};
use crate::transport::split_socket;

#[derive(Debug, Clone)]
enum ConnectionState {
    Handshaking,
    InRoom { room_id: String, participant_id: String },
}

/// Оркестратор.
/// Теперь хранит и Repository, и Registry.
#[derive(Clone)]
pub struct Orchestrator<R: RoomRepository> {
    repo: R,
    registry: ConnectionRegistry,
}

impl<R: RoomRepository + Clone> Orchestrator<R> {
    pub fn new(repo: R, registry: ConnectionRegistry) -> Self {
        Self { repo, registry }
    }

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

        // ✅ Очистка при разрыве
        if let ConnectionState::InRoom { room_id, participant_id } = state {
            debug!("Cleaning up: removing participant {} from room {}", participant_id, room_id);
            if let Err(e) = handle_leave_room(&self.repo, &self.registry, &room_id, &participant_id).await {
                error!("Failed to cleanup on disconnect: {}", e);
            }
        }
        debug!("WebSocket connection closed");
    }

    async fn send_error(&self, sender: &UnboundedSender<String>, message: &str) -> AppResult<()> {
        let err_msg = serde_json::to_string(&ServerMessage::Error { message: message.to_string() })?;
        sender.send(err_msg)
            .map_err(|e| AppError::Transport(crate::transport::TransportError::WebSocket(
                format!("Failed to send error: {}", e)
            )))?;
        Ok(())
    }

    #[instrument(skip(self, sender))]
    async fn handle_initial_message(
        &self,
        text: &str,
        sender: &UnboundedSender<String>,
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::CreateRoom => {
                let (room_id, participant_id) = handle_create_room(&self.repo, &self.registry, sender.clone()).await?;
                debug!("Created room {} with participant {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            ClientMessage::JoinRoom { room_id, display_name } => {
                let (room_id, participant_id) = handle_join_room(&self.repo, &self.registry, room_id, display_name, sender.clone()).await?;
                debug!("Joined room {} as participant {}", room_id, participant_id);
                Ok(Some(ConnectionState::InRoom { room_id, participant_id }))
            }
            _ => Err(AppError::Domain(crate::domain::DomainError::InvalidMessage(
                "First message must be CreateRoom or JoinRoom".into(),
            ))),
        }
    }

    #[instrument(skip(self, _sender), fields(room_id, participant_id))]
    async fn handle_message_in_room(
        &self,
        room_id: &str,
        participant_id: &str,
        text: &str,
        _sender: &UnboundedSender<String>,
    ) -> AppResult<Option<ConnectionState>> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::Offer { target_id, sdp } => {
                debug!("Forwarding offer from {} to {}", participant_id, target_id);
                crate::handlers::handle_offer(&self.repo, &self.registry, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::Answer { target_id, sdp } => {
                debug!("Forwarding answer from {} to {}", participant_id, target_id);
                crate::handlers::handle_answer(&self.repo, &self.registry, room_id, participant_id, &target_id, sdp).await?;
            }
            ClientMessage::IceCandidate { target_id, candidate } => {
                debug!("Forwarding ICE candidate from {} to {}", participant_id, target_id);
                crate::handlers::handle_ice_candidate(&self.repo, &self.registry, room_id, participant_id, &target_id, candidate).await?;
            }
            ClientMessage::LeaveRoom => {
                debug!("Participant {} leaving room {}", participant_id, room_id);
                handle_leave_room(&self.repo, &self.registry, room_id, participant_id).await?;
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

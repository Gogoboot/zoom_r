//! Обработка WebRTC сигнальных сообщений.
//!
//! Handlers берут sender из ConnectionRegistry, не лезут в сущности домена.

use crate::domain::{DomainError, ServerMessage};
use crate::error::AppResult;
use crate::handlers::HandlerError;
use crate::infrastructure::{ConnectionRegistry, RoomRepository};

pub async fn handle_offer<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    // Проверяем, что целевой участник существует в комнате
    let room = repo.get(room_id).await?
        .ok_or_else(|| DomainError::RoomNotFound(room_id.into()))?;
    if room.get_participant(target_id).is_none() {
        return Err(crate::error::AppError::Domain(DomainError::ParticipantNotFound(target_id.into())));
    }

    // ✅ Берём канал из реестра
    let target_sender = registry.get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let offer_msg = ServerMessage::Offer { from_id: from_id.to_string(), sdp };
    let text = serde_json::to_string(&offer_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send offer failed".into()))?;
    Ok(())
}

pub async fn handle_answer<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    let room = repo.get(room_id).await?
        .ok_or_else(|| DomainError::RoomNotFound(room_id.into()))?;
    if room.get_participant(target_id).is_none() {
        return Err(crate::error::AppError::Domain(DomainError::ParticipantNotFound(target_id.into())));
    }

    let target_sender = registry.get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let answer_msg = ServerMessage::Answer { from_id: from_id.to_string(), sdp };
    let text = serde_json::to_string(&answer_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send answer failed".into()))?;
    Ok(())
}

pub async fn handle_ice_candidate<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    candidate: String,
) -> AppResult<()> {
    let target_sender = registry.get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let ice_msg = ServerMessage::IceCandidate { from_id: from_id.to_string(), candidate };
    let text = serde_json::to_string(&ice_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send ICE failed".into()))?;
    Ok(())
}

//! Обработка WebRTC сигнальных сообщений.
//!
//! Handlers берут sender из ConnectionRegistry, не лезут в сущности домена.
use axum::extract::ws::Message; // ✅ Добавляем импорт Message
use crate::domain::{DomainError, ServerMessage};
use crate::error::AppResult;
use crate::handlers::HandlerError;
use crate::infrastructure::{ConnectionRegistry, RoomRepository};

pub async fn handle_offer<R: RoomRepository>(
    _repo: &R, // ✅ Префикс _ для неиспользуемого параметра
    registry: &ConnectionRegistry,
    _room_id: &str, // ✅ Префикс _ для неиспользуемого параметра
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    // ✅ Берём канал из реестра
    let target_sender = registry
        .get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let offer_msg = ServerMessage::Offer {
        from_id: from_id.to_string(),
        sdp,
    };
    let text = serde_json::to_string(&offer_msg)?;

    // ✅ Отправляем Message::Text и ждём подтверждения (.await)
    target_sender
        .send(Message::Text(text))
        .await
        .map_err(|_| HandlerError::Send("Send offer failed".into()))?;

    Ok(())
}

pub async fn handle_answer<R: RoomRepository>(
    _repo: &R,
    registry: &ConnectionRegistry,
    _room_id: &str,
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    let target_sender = registry
        .get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let answer_msg = ServerMessage::Answer {
        from_id: from_id.to_string(),
        sdp,
    };
    let text = serde_json::to_string(&answer_msg)?;

    target_sender
        .send(Message::Text(text))
        .await
        .map_err(|_| HandlerError::Send("Send answer failed".into()))?;

    Ok(())
}

pub async fn handle_ice_candidate<R: RoomRepository>(
    _repo: &R,
    registry: &ConnectionRegistry,
    _room_id: &str,
    from_id: &str,
    target_id: &str,
    candidate: String,
) -> AppResult<()> {
    let target_sender = registry
        .get_sender(target_id)
        .ok_or_else(|| DomainError::ParticipantNotFound(target_id.into()))?;

    let ice_msg = ServerMessage::IceCandidate {
        from_id: from_id.to_string(),
        candidate,
    };
    let text = serde_json::to_string(&ice_msg)?;

    target_sender
        .send(Message::Text(text))
        .await
        .map_err(|_| HandlerError::Send("Send ICE failed".into()))?;

    Ok(())
}

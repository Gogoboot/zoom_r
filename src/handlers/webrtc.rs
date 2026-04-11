//! Обработка WebRTC сигнальных сообщений.

use crate::domain::{DomainError, ServerMessage};
use crate::infrastructure::RoomRepository;
use crate::handlers::HandlerError;
use crate::error::AppResult;

// Вспомогательная функция для получения отправителя целевого участника
async fn get_target_sender<R: RoomRepository>(
    repo: &R,
    room_id: &str,
    target_id: &str,
) -> AppResult<tokio::sync::mpsc::UnboundedSender<String>> {
    let room = repo.get(room_id).await?
        .ok_or(DomainError::RoomNotFound(room_id.into()))?;
    let participant = room.get_participant(target_id)
        .ok_or(DomainError::ParticipantNotFound(target_id.into()))?;
    Ok(participant.sender.clone())
}

pub async fn handle_offer<R: RoomRepository>(
    repo: &R,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    let target_sender = get_target_sender(repo, room_id, target_id).await?;
    let offer_msg = ServerMessage::Offer { from_id: from_id.to_string(), sdp };
    let text = serde_json::to_string(&offer_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send failed".into()))?;
    Ok(())
}

pub async fn handle_answer<R: RoomRepository>(
    repo: &R,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    sdp: String,
) -> AppResult<()> {
    let target_sender = get_target_sender(repo, room_id, target_id).await?;
    let answer_msg = ServerMessage::Answer { from_id: from_id.to_string(), sdp };
    let text = serde_json::to_string(&answer_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send failed".into()))?;
    Ok(())
}

pub async fn handle_ice_candidate<R: RoomRepository>(
    repo: &R,
    room_id: &str,
    from_id: &str,
    target_id: &str,
    candidate: String,
) -> AppResult<()> {
    let target_sender = get_target_sender(repo, room_id, target_id).await?;
    let ice_msg = ServerMessage::IceCandidate { from_id: from_id.to_string(), candidate };
    let text = serde_json::to_string(&ice_msg)?;
    target_sender.send(text).map_err(|_| HandlerError::Send("Send failed".into()))?;
    Ok(())
}

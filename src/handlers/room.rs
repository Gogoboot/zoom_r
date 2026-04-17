//! Обработка команд управления комнатами.
//!
//! Теперь handlers управляют транспортом через ConnectionRegistry,
//! сохраняя домен чистым.
use axum::extract::ws::Message;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::domain::{Participant, ParticipantInfo, Room, ServerMessage};
use crate::error::AppResult;
use crate::handlers::HandlerError;
use crate::infrastructure::{ConnectionRegistry, RoomRepository};

// ✅ Тип совпадает с connection_registry.rs
pub type WebSocketSender = mpsc::Sender<Message>;

/// Вспомогательная функция: рассылает сообщение всем участникам комнаты, кроме одного.
async fn notify_room_except<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: &str,
    exclude_id: &str,
    message: ServerMessage,
) -> AppResult<()> {
    let room = repo
        .get(room_id)
        .await?
        .ok_or_else(|| crate::domain::DomainError::RoomNotFound(room_id.into()))?;

    let text = serde_json::to_string(&message)?;

    for p in room.get_all_participants() {
        if p.id != exclude_id {
            if let Some(sender) = registry.get_sender(&p.id) {
                // Игнорируем ошибку отправки: если канал закрыт, участник скоро удалится
                let _ = sender.send(Message::Text(text.clone())).await;
            }
        }
    }
    Ok(())
}

pub async fn handle_create_room<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    sender: WebSocketSender,
) -> AppResult<(String, String)> {
    let room_id = Uuid::new_v4().to_string();
    let participant_id = Uuid::new_v4().to_string();
    let display_name = format!("User_{}", &participant_id[0..5]);

    let participant = Participant::new(participant_id.clone(), display_name);
    registry.register(participant_id.clone(), sender.clone());

    let mut room = Room::new(room_id.clone());
    room.add_participant(participant);
    repo.insert(room).await?;

    let response = ServerMessage::RoomCreated {
        room_id: room_id.clone(),
        participant_id: participant_id.clone(),
    };
    let msg = serde_json::to_string(&response)?;

    sender
        .send(Message::Text(msg))
        .await
        .map_err(|_| HandlerError::Send("Failed to send RoomCreated".into()))?;

    Ok((room_id, participant_id))
}

pub async fn handle_join_room<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: String,
    display_name: String,
    sender: WebSocketSender,
) -> AppResult<(String, String)> {
    let mut room = repo
        .get_mut(&room_id)
        .await?
        .ok_or_else(|| crate::domain::DomainError::RoomNotFound(room_id.clone()))?;

    let participant_id = Uuid::new_v4().to_string();
    let display_name = if display_name.is_empty() || display_name == "Anonymous" {
        format!("User_{}", &participant_id[0..5])
    } else {
        display_name
    };

    let participant = Participant::new(participant_id.clone(), display_name.clone());
    registry.register(participant_id.clone(), sender.clone());

    let joined_msg = ServerMessage::ParticipantJoined {
        participant: ParticipantInfo {
            id: participant_id.clone(),
            display_name: display_name.clone(),
        },
    };
    notify_room_except(repo, registry, &room_id, &participant_id, joined_msg).await?;

    room.add_participant(participant);
    repo.remove(&room_id).await?;
    repo.insert(room.clone()).await?;

    let participants_list: Vec<ParticipantInfo> = room
        .get_all_participants()
        .iter()
        .map(|p| ParticipantInfo {
            id: p.id.clone(),
            display_name: p.display_name.clone(),
        })
        .collect();

    let room_joined_msg = ServerMessage::RoomJoined {
        room_id: room_id.clone(),
        participants: participants_list,
        participant_id: participant_id.clone(),
    };
    let msg = serde_json::to_string(&room_joined_msg)?;

    sender
        .send(Message::Text(msg))
        .await
        .map_err(|_| HandlerError::Send("Failed to send RoomJoined".into()))?;

    Ok((room_id, participant_id))
}

pub async fn handle_leave_room<R: RoomRepository>(
    repo: &R,
    registry: &ConnectionRegistry,
    room_id: &str,
    participant_id: &str,
) -> AppResult<()> {
    // ✅ ПУНКТ 5: Идемпотентность. Если уже отключён — выходим сразу.
    if !registry.is_connected(participant_id) {
        tracing::debug!("Participant {} already unregistered. Skipping leave.", participant_id);
        return Ok(());
    }

    // ✅ Удаляем канал из реестра
    registry.unregister(participant_id);

    let mut room = match repo.get_mut(room_id).await? {
        Some(r) => r,
        None => return Ok(()),
    };

    if room.remove_participant(participant_id).is_some() {
        // ✅ Уведомляем остальных
        let left_msg = ServerMessage::ParticipantLeft {
            participant_id: participant_id.to_string(),
        };
        notify_room_except(repo, registry, room_id, participant_id, left_msg).await?;

        // ✅ ПУНКТ 11: Атомарное удаление пустой комнаты
        if room.is_empty() {
            repo.remove_if_empty(room_id).await?;
            tracing::debug!("Room {} deleted atomically (empty)", room_id);
        } else {
            repo.remove(room_id).await?;
            repo.insert(room).await?;
        }
    }
    Ok(())
}

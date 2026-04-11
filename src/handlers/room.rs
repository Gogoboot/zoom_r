//! Обработка команд управления комнатами.

use crate::domain::{DomainError, Participant, ParticipantInfo, Room, ServerMessage};
use crate::error::AppResult;
use crate::handlers::HandlerError;
use crate::infrastructure::RoomRepository;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub async fn handle_create_room<R: RoomRepository>(
    repo: &R,
    sender: UnboundedSender<String>,
) -> AppResult<(String, String)> {
    let room_id = Uuid::new_v4().to_string();
    let participant_id = Uuid::new_v4().to_string();
    let display_name = format!("User_{}", &participant_id[0..5]);

    let participant = Participant::new(participant_id.clone(), display_name, sender.clone());
    let mut room = Room::new(room_id.clone());
    room.add_participant(participant);
    repo.insert(room).await?;

    let response = ServerMessage::RoomCreated {
        room_id: room_id.clone(),
        participant_id: participant_id.clone(),
    };
    let msg = serde_json::to_string(&response)?;
    sender
        .send(msg)
        .map_err(|_| HandlerError::Send("Failed to send".into()))?;

    Ok((room_id, participant_id))
}

pub async fn handle_join_room<R: RoomRepository>(
    repo: &R,
    room_id: String,
    display_name: String, // ✅ Serde уже гарантировал: это String, не Option
    sender: UnboundedSender<String>,
) -> AppResult<(String, String)> {
    // Получаем комнату для мутации
    let mut room = repo
        .get_mut(&room_id)
        .await?
        .ok_or(DomainError::RoomNotFound(room_id.clone()))?;

    let participant_id = Uuid::new_v4().to_string();

    // ✅ Исправление: проверяем на пустую строку, а не на None
    let display_name = if display_name.is_empty() || display_name == "Anonymous" {
        // Генерируем уникальное имя, если клиент не указал своё или прислал дефолт
        format!("User_{}", &participant_id[0..5])
    } else {
        // Используем имя, которое прислал клиент (уже валидировано через serde)
        display_name
    };

    // Создаём участника
    let participant =
        Participant::new(participant_id.clone(), display_name.clone(), sender.clone());

    // Уведомить остальных участников о новом участнике
    let joined_msg = ServerMessage::ParticipantJoined {
        participant: ParticipantInfo {
            id: participant_id.clone(),
            display_name: display_name.clone(),
        },
    };
    let joined_text = serde_json::to_string(&joined_msg)?;
    room.broadcast_except(&participant_id, joined_text);

    // Добавить нового участника в комнату
    room.add_participant(participant);

    // Сохранить обновлённую комнату
    // ⚠️ Временное решение: удаляем и вставляем заново, т.к. DashMap Guard не позволяет мутацию
    // В будущем можно оптимизировать через трейт RoomRepository::update()
    repo.remove(&room_id).await?;
    repo.insert(room.clone()).await?;

    // Сформировать список участников для новичка
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
        .send(msg)
        .map_err(|_| HandlerError::Send("Failed to send".into()))?;

    Ok((room_id, participant_id))
}

pub async fn handle_leave_room<R: RoomRepository>(
    repo: &R,
    room_id: &str,
    participant_id: &str,
) -> AppResult<()> {
    let mut room = match repo.get_mut(room_id).await? {
        Some(r) => r,
        None => return Ok(()),
    };
    if room.remove_participant(participant_id).is_some() {
        let left_msg = ServerMessage::ParticipantLeft {
            participant_id: participant_id.to_string(),
        };
        let left_text = serde_json::to_string(&left_msg)?;
        room.broadcast_except(participant_id, left_text);
        if room.participant_count() == 0 {
            repo.remove(room_id).await?;
            tracing::debug!("Room {} deleted (empty)", room_id);
        } else {
            // Обновить комнату в хранилище
            repo.remove(room_id).await?;
            repo.insert(room).await?;
        }
    }
    Ok(())
}

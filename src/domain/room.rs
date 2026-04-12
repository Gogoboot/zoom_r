//! Сущность комнаты.
//!
//! [`Room`] управляет списком участников.
//! Не занимается отправкой сообщений (это задача Handlers/Orchestrator).

use std::collections::HashMap;
use crate::domain::participant::Participant;

/// Комната для обмена WebRTC-сигналами.
#[derive(Debug, Clone)]
pub struct Room {
    pub id: String,
    participants: HashMap<String, Participant>,
}

impl Room {
    pub fn new(id: String) -> Self {
        Self {
            id,
            participants: HashMap::new(),
        }
    }

    pub fn add_participant(&mut self, participant: Participant) {
        self.participants.insert(participant.id.clone(), participant);
    }

    pub fn remove_participant(&mut self, participant_id: &str) -> Option<Participant> {
        self.participants.remove(participant_id)
    }

    pub fn get_participant(&self, participant_id: &str) -> Option<&Participant> {
        self.participants.get(participant_id)
    }

    /// Возвращает всех участников.
    /// Используется обработчиками для рассылки уведомлений.
    pub fn get_all_participants(&self) -> Vec<&Participant> {
        self.participants.values().collect()
    }

    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    #[inline]
    pub fn has_participant(&self, participant_id: &str) -> bool {
        self.participants.contains_key(participant_id)
    }

    /// Безопасно проверяет, есть ли в комнате участники
    pub fn is_empty(&self) -> bool {
        self.participants.is_empty()
    }

}

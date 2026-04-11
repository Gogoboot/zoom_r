//! Сущность участника комнаты.
//!
//! [`Participant`] представляет подключённого клиента.
//! Содержит только бизнес-данные, без привязки к транспорту.

/// Участник WebRTC-комнаты.
#[derive(Debug, Clone)]
pub struct Participant {
    /// Уникальный идентификатор участника.
    pub id: String,
    /// Отображаемое имя участника.
    pub display_name: String,
}

impl Participant {
    /// Создаёт нового участника.
    pub fn new(id: String, display_name: String) -> Self {
        Self { id, display_name }
    }

    /// Возвращает публичную информацию (DTO) для отправки клиентам.
    #[inline]
    pub fn to_info(&self) -> crate::domain::ParticipantInfo {
        crate::domain::ParticipantInfo {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

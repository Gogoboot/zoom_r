//! Сущность комнаты.
//!
//! [`Room`] представляет виртуальную комнату для WebRTC-сессии.
//! Управляет участниками, обеспечивает рассылку сообщений и инварианты.

use std::collections::HashMap;
use tracing::warn;  // 🆕 Для логирования ошибок отправки

use crate::domain::participant::Participant;

/// Комната для обмена WebRTC-сигналами.
///
/// # Инварианты
///
/// - `id` уникален в рамках системы
/// - Каждый участник имеет уникальный `id` внутри комнаты
/// - Рассылка сообщений игнорирует отправителя (чтобы не дублировать)
///
/// # Потокобезопасность
///
/// Сама структура [`Room`] не является потокобезопасной.
/// Синхронизация обеспечивается на уровне хранилища (например, [`DashMap`](https://docs.rs/dashmap)
/// в `MemoryRoomStore`). При работе с [`Room`] из нескольких асинхронных задач
/// используйте внешние примитивы синхронизации (`Mutex`, `RwLock`) или передавайте
/// владение через каналы.
#[derive(Debug, Clone)]
pub struct Room {
    /// Уникальный идентификатор комнаты (генерируется при создании).
    pub id: String,

    /// Участники комнаты, ключ — `participant.id`.
    ///
    /// Приватное поле: доступ только через методы, чтобы контролировать инварианты.
    participants: HashMap<String, Participant>,
}

impl Room {
    /// Создаёт новую пустую комнату с заданным идентификатором.
    ///
    /// # Аргументы
    ///
    /// - `id`: уникальный идентификатор комнаты (обычно UUID)
    ///
    /// # Возвращает
    ///
    /// Новый экземпляр [`Room`] без участников.
    ///
    /// # Пример
    ///
    /// ```rust
    /// use signaling_server::domain::Room;
    ///
    /// let room = Room::new("room-123".to_string());
    /// assert_eq!(room.id, "room-123");
    /// assert_eq!(room.participant_count(), 0);
    /// ```
    pub fn new(id: String) -> Self {
        Self {
            id,
            participants: HashMap::new(),
        }
    }

    /// Добавляет участника в комнату.
    ///
    /// Если участник с таким `id` уже существует, он будет **заменён**.
    /// Это допустимо для сценария переподключения.
    ///
    /// # Аргументы
    ///
    /// - `participant`: участник для добавления
    pub fn add_participant(&mut self, participant: Participant) {
        let id = participant.id.clone();
        self.participants.insert(id, participant);
    }

    /// Удаляет участника из комнаты по его идентификатору.
    ///
    /// # Аргументы
    ///
    /// - `participant_id`: идентификатор участника для удаления
    ///
    /// # Возвращает
    ///
    /// - `Some(Participant)` — если участник был найден и удалён
    /// - `None` — если участник не найден (уже удалён или не существовал)
    pub fn remove_participant(&mut self, participant_id: &str) -> Option<Participant> {
        self.participants.remove(participant_id)
    }

    /// Возвращает ссылку на участника по его идентификатору.
    ///
    /// # Аргументы
    ///
    /// - `participant_id`: идентификатор искомого участника
    ///
    /// # Возвращает
    ///
    /// - `Some(&Participant)` — если участник найден
    /// - `None` — если не найден
    pub fn get_participant(&self, participant_id: &str) -> Option<&Participant> {
        self.participants.get(participant_id)
    }

    /// Возвращает вектор ссылок на всех участников комнаты.
    ///
    /// # Возвращает
    ///
    /// Вектор ссылок [`&Participant`]. Порядок не гарантируется.
    ///
    /// # Пример
    ///
    /// ```rust
    /// # use signaling_server::domain::{Room, Participant};
    /// # let mut room = Room::new("test".into());
    /// for p in room.get_all_participants() {
    ///     println!("Participant: {}", p.id);
    /// }
    /// ```
    pub fn get_all_participants(&self) -> Vec<&Participant> {
        self.participants.values().collect()
    }

    /// Возвращает количество участников в комнате.
    ///
    /// # Возвращает
    ///
    /// Число участников (`usize`).
    #[inline]
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Рассылает сообщение всем участникам, кроме указанного.
    ///
    /// # Аргументы
    ///
    /// - `exclude_id`: идентификатор участника, которому **не** нужно отправлять сообщение
    ///   (обычно это отправитель, чтобы не дублировать сообщение)
    /// - `message`: текст сообщения для отправки (обычно сериализованный JSON)
    ///
    /// # Поведение при ошибке
    ///
    /// Если отправка участнику не удалась (канал закрыт, участник отключился):
    /// - Ошибка **не прерывает** рассылку остальным участникам
    /// - Ошибка логируется на уровне `warn` для отладки
    /// - Участник **не удаляется автоматически** — это задача оркестратора
    ///
    /// # Пример
    ///
    /// ```rust
    /// # use signaling_server::domain::Room;
    /// # let room = Room::new("test".into());
    /// room.broadcast_except("sender-123", r#"{"type":"offer"}"#.to_string());
    /// ```
    pub fn broadcast_except(&self, exclude_id: &str, message: String) {
        for participant in self.participants.values() {
            if participant.id != exclude_id {
                // 🆕 Обрабатываем ошибку отправки: логируем, но не паникуем
                if let Err(e) = participant.sender.send(message.clone()) {
                    warn!(
                        "Failed to send message to participant {}: {}",
                        participant.id, e
                    );
                }
            }
        }
    }

    /// Проверяет, является ли указанный участник владельцем комнаты.
    ///
    /// # Аргументы
    ///
    /// - `participant_id`: идентификатор участника для проверки
    ///
    /// # Возвращает
    ///
    /// `true`, если участник существует в комнате (в простой модели любой участник — «владелец»).
    /// В более сложной модели можно добавить поле `owner_id` в [`Room`].
    #[inline]
    pub fn has_participant(&self, participant_id: &str) -> bool {
        self.participants.contains_key(participant_id)
    }
}
